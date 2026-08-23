//! Executing one command, and executing a whole flow of them.
//!
//! A flow runs in one process over one SSH session: one handshake, one root
//! probe, one file-transfer channel, however many steps. Spawning a process
//! per key would spend most of the wall clock on handshakes.

use std::fmt::Write as _;
use std::fs;
use std::io::{IsTerminal, Read, stdin, stdout};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use ares_connection_lib::session::DeviceSession;
use ares_connection_lib::transfer::Transfer;
use clap::Parser;
use clap::error::ErrorKind;
use serde_json::{Value, json};

use crate::app::AppControl;
use crate::cli::{self, Cli, Command, ExecArgs, FlowLine, LunaArgs, RunArgs, ShotArgs};
use crate::error::DoError;
use crate::flow::{self, FlowError, Step};
use crate::key::SendKey;
use crate::luna::{self, LunaReply};
use crate::output::{Reporter, Timer};
use crate::screenshot::{Capturer, ShotTarget, write_shot};
use crate::text::TypeText;
use crate::{exec, keycode};

/// Everything a step needs that does not come from the step itself.
pub(crate) struct Runner<'a> {
    session: &'a DeviceSession,
    transfer: Transfer<'a>,
    reporter: &'a Reporter,
    /// Bounds every command sent to the device. `None` waits forever.
    timeout: Option<Duration>,
    /// Set only while running a flow; a one-shot command has no output dir.
    flow: Option<FlowContext>,
}

struct FlowContext {
    out: PathBuf,
    prefix: String,
    settle: Duration,
    counter: u32,
}

impl<'a> Runner<'a> {
    pub(crate) fn new(
        session: &'a DeviceSession,
        timeout: Option<Duration>,
        reporter: &'a Reporter,
    ) -> Self {
        Runner {
            session,
            transfer: Transfer::open(session),
            reporter,
            timeout,
            flow: None,
        }
    }

    /// Run one command, as typed on the command line.
    ///
    /// # Errors
    ///
    /// Whatever the command failed with.
    pub(crate) fn one(&mut self, command: &Command) -> Result<(), DoError> {
        match command {
            Command::Key(args) => {
                for _ in 0..args.repeat.max(1) {
                    self.session
                        .send_keys(&args.keys, args.delay, self.timeout, self.reporter)?;
                }
                Ok(())
            }
            Command::Text(args) => self.session.type_text(
                &args.text,
                args.lower,
                args.delay,
                self.timeout,
                self.reporter,
            ),
            Command::Screenshot(args) => self.screenshot(args),
            Command::Wait(args) => {
                let timer = Timer::start();
                sleep(args.time);
                self.reporter.event(&json!({
                    "event": "wait", "ok": true, "ms": timer.ms(),
                }));
                Ok(())
            }
            Command::Launch(args) => {
                let params = cli::parse_params(&args.params).map_err(DoError::Usage)?;
                self.session
                    .launch_app(&args.app_id, params, self.timeout, self.reporter)
            }
            Command::Close(args) => {
                let params = cli::parse_params(&args.params).map_err(DoError::Usage)?;
                self.session
                    .close_app(&args.app_id, params, self.timeout, self.reporter)
            }
            Command::Luna(args) => self.luna(args),
            Command::Exec(args) => self.exec(args),
            Command::Echo(args) => {
                self.reporter.info(&args.message);
                self.reporter.event(&json!({
                    "event": "echo", "ok": true, "message": args.message,
                }));
                Ok(())
            }
            Command::Run(args) => self.flow(args),
            // Handled before a device is ever opened.
            Command::Keys(_) => Ok(()),
        }
    }

    /// One luna call, with the reply on stdout.
    ///
    /// stdout gets the reply verbatim, because the reply is the data — that is
    /// what makes `ares-do luna ... | jq .returnValue` work.
    fn luna(&mut self, args: &LunaArgs) -> Result<(), DoError> {
        let payload: Value = serde_json::from_str(&args.payload)
            .map_err(|e| DoError::Usage(format!("payload is not JSON: {e}")))?;
        let timer = Timer::start();

        let reply: Value = if args.public {
            luna::raw_pub(
                &self.session.session,
                &args.uri,
                &payload,
                self.timeout,
                self.reporter,
            )?
        } else {
            luna::raw(
                &self.session.session,
                &args.uri,
                &payload,
                self.timeout,
                self.reporter,
            )?
        };

        if self.reporter.is_json() {
            self.reporter.event(&json!({
                "event": "luna", "ok": true, "uri": args.uri,
                "reply": reply, "ms": timer.ms(),
            }));
        } else {
            println!("{reply}");
        }

        // A negative reply fails by default, so `luna` works as an assertion
        // in a flow without a wrapper around it.
        if !args.allow_false {
            let typed: LunaReply = serde_json::from_value(reply)
                .map_err(|e| DoError::Capture(format!("reply had an odd shape: {e}")))?;
            typed.check(&args.uri)?;
        }
        Ok(())
    }

    /// One command on the device.
    ///
    /// Exits with the command's own status, the way `ares-shell` does — see
    /// the README, this is the one place `ares-do` does not use its own table.
    fn exec(&mut self, args: &ExecArgs) -> Result<(), DoError> {
        let timer = Timer::start();
        let output = exec::run(&self.session.session, &args.command, self.timeout)?;

        if self.reporter.is_json() {
            self.reporter.event(&json!({
                "event": "exec", "ok": output.code == 0, "command": args.command,
                "exitCode": output.code, "stdout": output.stdout, "ms": timer.ms(),
            }));
        } else {
            print!("{}", output.stdout);
        }

        if output.code == 0 {
            Ok(())
        } else {
            Err(DoError::RemoteCommand {
                command: args.command.clone(),
                code: output.code,
            })
        }
    }

    fn screenshot(&mut self, args: &ShotArgs) -> Result<(), DoError> {
        let target = self.shot_target(args)?;
        if let Some(flow) = &self.flow
            && !flow.settle.is_zero()
        {
            // A key is not finished when sendKeyCode returns, so give the UI a
            // moment before believing what is on screen.
            sleep(flow.settle);
        }

        let timer = Timer::start();
        let mut capturer = Capturer::new(
            args.method,
            args.remote_dir.clone(),
            args.retries,
            self.timeout,
        );
        let shot = capturer.capture(self.session, &self.transfer, self.reporter)?;
        write_shot(&target, &shot.bytes)?;

        let where_to = match &target {
            ShotTarget::Stdout => String::from("<stdout>"),
            ShotTarget::File(path) => absolute(path),
        };
        self.reporter
            .info(&format!("Captured {} to {where_to}", shot.method));
        self.reporter.event(&json!({
            "event": "screenshot",
            "ok": true,
            "path": matches!(target, ShotTarget::File(_)).then(|| where_to.clone()),
            "bytes": shot.bytes.len(),
            "method": shot.method.to_string(),
            "service": shot.service,
            "foregroundAppId": self.session.foreground_app(self.timeout, self.reporter),
            "ms": timer.ms(),
        }));
        Ok(())
    }

    /// Decide where a capture goes, and refuse the ways that end badly.
    fn shot_target(&mut self, args: &ShotArgs) -> Result<ShotTarget, DoError> {
        let named = args.file.as_deref().filter(|f| *f != "-");

        let Some(flow) = &mut self.flow else {
            // One-shot: a name is a path, and no name means stdout.
            return match named {
                Some(path) => Ok(ShotTarget::File(PathBuf::from(path))),
                None if self.reporter.is_json() => Err(DoError::Usage(String::from(
                    "--json needs a FILE argument for screenshot, because stdout is carrying \
                     the JSON stream",
                ))),
                None if stdout().is_terminal() => Err(DoError::Usage(String::from(
                    "refusing to write PNG bytes to a terminal.\n  \
                     Give a file: ares-do screenshot shot.png\n  \
                     Or redirect: ares-do screenshot > shot.png",
                ))),
                None => Ok(ShotTarget::Stdout),
            };
        };

        // In a flow, every shot is a file under --out; stdout would interleave
        // several PNGs into one stream, which is never what was meant.
        flow.counter += 1;
        let name = shot_name(named, &flow.prefix, flow.counter)?;
        Ok(ShotTarget::File(flow.out.join(name)))
    }

    /// Read, parse and replay a flow.
    fn flow(&mut self, args: &RunArgs) -> Result<(), DoError> {
        let (origin, text) = read_flow(&args.flow)?;
        let steps = flow::parse(&origin, &text).map_err(DoError::Flow)?;
        let commands = compile(&origin, &steps)?;

        let out = PathBuf::from(&args.out);
        fs::create_dir_all(&out)?;
        self.flow = Some(FlowContext {
            out,
            prefix: args.prefix.clone(),
            settle: args.settle,
            counter: 0,
        });

        let timer = Timer::start();
        let mut failed: Option<DoError> = None;
        let mut ran = 0usize;

        for (step, command) in &commands {
            let step_timer = Timer::start();
            match self.one(command) {
                Ok(()) => {
                    ran += 1;
                    self.reporter.event(&json!({
                        "event": "step", "ok": true, "line": step.line,
                        "source": step.source, "command": command.name(),
                        "ms": step_timer.ms(),
                    }));
                }
                Err(e) => {
                    self.reporter.event(&json!({
                        "event": "step", "ok": false, "line": step.line,
                        "source": step.source, "command": command.name(),
                        "kind": e.kind(), "message": e.to_string(),
                        "ms": step_timer.ms(),
                    }));
                    if args.keep_going {
                        self.reporter.warn(&format!("{origin}:{}: {e}", step.line));
                        if failed.is_none() {
                            failed = Some(e);
                        }
                    } else {
                        self.reporter.event(&json!({
                            "event": "summary", "ok": false, "steps": commands.len(),
                            "ran": ran, "failedLine": step.line, "ms": timer.ms(),
                        }));
                        return Err(e);
                    }
                }
            }
        }

        let ok = failed.is_none();
        self.reporter.event(&json!({
            "event": "summary", "ok": ok, "steps": commands.len(),
            "ran": ran, "ms": timer.ms(),
        }));
        self.flow = None;
        match failed {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

/// What a `shot` step inside a flow is called on disk.
///
/// Shared with `--dry-run` on purpose: a dry run that predicts a different
/// filename from the real one is worse than no dry run.
///
/// # Errors
///
/// [`DoError::Usage`] for a name that would escape `--out`.
fn shot_name(named: Option<&str>, prefix: &str, counter: u32) -> Result<String, DoError> {
    let Some(name) = named else {
        return Ok(format!("{prefix}-{counter:03}.png"));
    };
    let candidate = Path::new(name);
    if candidate.is_absolute()
        || candidate
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(DoError::Usage(format!(
            "screenshot name {name:?} must stay inside --out"
        )));
    }
    Ok(if candidate.extension().is_none() {
        format!("{name}.png")
    } else {
        name.to_string()
    })
}

/// Turn tokenized lines into commands, reporting every bad line at once.
///
/// Nothing runs until the whole file is known to be good — half a flow is
/// worse than none, and one round trip should be enough to fix a file.
fn compile<'a>(origin: &str, steps: &'a [Step]) -> Result<Vec<(&'a Step, Command)>, DoError> {
    let mut commands = Vec::new();
    let mut errors = Vec::new();

    for step in steps {
        let parsed = FlowLine::try_parse_from(&step.tokens);
        let message = match parsed {
            Ok(line) => match line.command {
                // Nesting would need a second output directory and a second
                // counter; there is no good answer yet, so refuse clearly.
                Command::Run(_) => Some(String::from("`run` cannot be nested inside a flow")),
                Command::Keys(_) => Some(String::from(
                    "`keys` lists names, it is not an action a flow can take",
                )),
                command => {
                    commands.push((step, command));
                    None
                }
            },
            // `key --help` on line 7 must not print help and exit 0 halfway
            // through a flow.
            Err(e) if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) => {
                Some(String::from("--help and --version are not flow steps"))
            }
            Err(e) => Some(single_line(&e.to_string())),
        };

        if let Some(message) = message {
            errors.push(FlowError {
                origin: origin.to_string(),
                line: step.line,
                source: step.source.clone(),
                message,
            });
        }
    }

    if errors.is_empty() {
        Ok(commands)
    } else {
        Err(DoError::Flow(errors))
    }
}

/// clap renders a whole usage block; a flow error wants the first sentence of
/// it, with the rest folded in behind.
fn single_line(message: &str) -> String {
    message
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take_while(|l| !l.starts_with("Usage:"))
        .collect::<Vec<_>>()
        .join(" ")
        .trim_start_matches("error: ")
        .to_string()
}

fn read_flow(path: &str) -> Result<(String, String), DoError> {
    if path == "-" {
        let mut text = String::new();
        stdin().read_to_string(&mut text)?;
        return Ok((String::from("<stdin>"), text));
    }
    let text = fs::read_to_string(path)
        .map_err(|e| DoError::Io(std::io::Error::new(e.kind(), format!("{path}: {e}"))))?;
    Ok((path.to_string(), text))
}

fn absolute(path: &Path) -> String {
    // A relative path in machine output makes the reader guess our cwd.
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// Print what would happen, and touch nothing.
///
/// Deliberately does not look the device up either, so a generated flow can be
/// checked on a machine that has never seen a TV.
pub(crate) fn dry_run(cli: &Cli, reporter: &Reporter) -> Result<(), DoError> {
    let commands: Vec<(usize, Command)> = match &cli.command {
        Command::Run(args) => {
            let (origin, text) = read_flow(&args.flow)?;
            let steps = flow::parse(&origin, &text).map_err(DoError::Flow)?;
            let compiled = compile(&origin, &steps)?;
            compiled
                .into_iter()
                .map(|(step, command)| (step.line, command))
                .collect()
        }
        command => vec![(0, command.clone())],
    };

    let mut counter = 0u32;
    let (out, prefix) = match &cli.command {
        Command::Run(args) => (args.out.clone(), args.prefix.clone()),
        _ => (String::from("."), String::from("shot")),
    };

    for (line, command) in &commands {
        let mut lines: Vec<String> = Vec::new();
        match command {
            Command::Key(args) => {
                for _ in 0..args.repeat.max(1) {
                    for key in &args.keys {
                        lines.push(format!(
                            "{} {{\"keyCode\":{}}}   # {key}",
                            crate::key::SEND_KEY_URI,
                            key.code
                        ));
                    }
                }
            }
            Command::Text(args) => match crate::text::text_to_keys(&args.text, args.lower) {
                Ok(keys) => {
                    for key in keys {
                        lines.push(format!(
                            "{} {{\"keyCode\":{}}}   # {key}",
                            crate::key::SEND_KEY_URI,
                            key.code
                        ));
                    }
                }
                Err(e) => return Err(e),
            },
            Command::Screenshot(args) => {
                let named = args.file.as_deref().filter(|f| *f != "-");
                let where_to = if matches!(cli.command, Command::Run(_)) {
                    counter += 1;
                    PathBuf::from(&out)
                        .join(shot_name(named, &prefix, counter)?)
                        .to_string_lossy()
                        .into_owned()
                } else {
                    named.map_or_else(|| String::from("<stdout>"), ToString::to_string)
                };
                lines.push(format!("capture {} -> {where_to}", args.method));
            }
            Command::Wait(args) => lines.push(format!("sleep {}ms", args.time.as_millis())),
            Command::Launch(args) | Command::Close(args) => {
                let params = cli::parse_params(&args.params).map_err(DoError::Usage)?;
                let verb = command.name();
                if params.is_null() {
                    lines.push(format!("{verb} {}", args.app_id));
                } else {
                    lines.push(format!("{verb} {} {params}", args.app_id));
                }
            }
            Command::Luna(args) => {
                let bus = if args.public {
                    "luna-send-pub"
                } else {
                    "luna-send"
                };
                lines.push(format!("{bus} -n 1 {} {}", args.uri, args.payload));
            }
            Command::Exec(args) => lines.push(format!("exec {}", args.command)),
            Command::Echo(args) => lines.push(format!("echo {}", args.message)),
            Command::Run(_) | Command::Keys(_) => {}
        }

        for text in lines {
            let prefixed = if *line > 0 {
                format!("{line:>4} | {text}")
            } else {
                text
            };
            reporter.info(&prefixed);
            reporter.event(&json!({
                "event": command.name(), "ok": true, "dryRun": true,
                "line": (*line > 0).then_some(*line), "action": prefixed,
            }));
        }
    }
    Ok(())
}

/// `ares-do keys`, which never opens a connection.
pub(crate) fn list_keys(pattern: Option<&str>, all: bool, reporter: &Reporter) {
    let rows = keycode::list(all, pattern);
    if reporter.is_json() {
        reporter.event(&json!({
            "event": "keys",
            "keys": rows.iter().map(|k| json!({
                "name": k.name, "code": k.code, "aliases": k.aliases, "note": k.note,
            })).collect::<Vec<_>>(),
        }));
        return;
    }
    let mut out = String::new();
    for row in &rows {
        let names = if row.aliases.is_empty() {
            row.name.to_string()
        } else {
            format!("{} ({})", row.name, row.aliases.join(", "))
        };
        let _ = write!(out, "{names:<34} {:>4}", row.code);
        if let Some(note) = row.note {
            let _ = write!(out, "   # {note}");
        }
        out.push('\n');
    }
    print!("{out}");
}

/// Best-effort sweep of anything a killed run left on the device.
///
/// `Drop` does not run on Ctrl-C, so a previous run's temp file can outlive
/// it. Cleaning at the start rather than trapping a signal keeps this
/// dependency-free.
pub(crate) fn sweep_leftovers(
    session: &DeviceSession,
    timeout: Option<Duration>,
    reporter: &Reporter,
) {
    let pattern = format!("/tmp/ares-do-{}-*.png", std::process::id());
    if let Ok(output) = exec::run(&session.session, &format!("rm -f {pattern}"), timeout)
        && output.code != 0
    {
        reporter.trace(&format!("could not sweep {pattern}"));
    }
}
