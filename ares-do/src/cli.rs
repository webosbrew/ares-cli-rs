//! The command surface — used twice.
//!
//! [`Cli`] is what the shell hands us. [`FlowLine`] wraps the same
//! [`Command`] with `no_binary_name`, so a line of a flow file parses through
//! exactly the same code. That is the whole reason the flow language needs no
//! grammar of its own: it *is* this.

use std::time::Duration;

use clap::{ArgAction, Parser, Subcommand};
use serde_json::{Map, Value, json};

use crate::keycode::{self, Key};
use crate::screenshot::CaptureMethod;

const AFTER_HELP: &str = "\
Examples:
  ares-do key OK                       press the OK button
  ares-do key TAB TAB ENTER            a web view starts unfocused; TAB first
  ares-do text demo                    type lowercase text
  ares-do screenshot shot.png          save a screenshot
  ares-do screenshot > shot.png        or send the PNG to stdout
  ares-do keys                         list the buttons a remote has
  ares-do launch com.example.app -p url=http://x
  ares-do luna luna://com.webos.service.tv.power/getPowerState
  ares-do exec 'cat /var/run/nyx/device_info.json'
  ares-do run flow.txt --out ./frames  replay a flow, saving every shot

A flow file is one command per line, in exactly the syntax above.
Read one from stdin with `ares-do run -`.";

// A CLI is where flags live; folding these into enums would only make the
// clap derive harder to read.
#[allow(clippy::struct_excessive_bools)]
#[derive(Parser, Debug)]
#[command(about, version, after_help = AFTER_HELP)]
pub(crate) struct Cli {
    #[arg(
        short,
        long,
        value_name = "DEVICE",
        env = "ARES_DEVICE",
        global = true,
        help = "Specify DEVICE to use"
    )]
    pub device: Option<String>,

    #[arg(
        long,
        global = true,
        help = "Print one JSON object per action on stdout"
    )]
    pub json: bool,

    #[arg(short, long, global = true, help = "Only print errors")]
    pub quiet: bool,

    #[arg(
        short,
        long,
        global = true,
        action = ArgAction::Count,
        help = "Print every luna call before it is sent"
    )]
    pub verbose: u8,

    #[arg(
        long,
        global = true,
        help = "Resolve everything and print it, without touching the device"
    )]
    pub dry_run: bool,

    #[arg(long, global = true, help = "Run even when the session is not root")]
    pub allow_non_root: bool,

    #[arg(
        long,
        global = true,
        value_name = "MS",
        default_value = "30s",
        value_parser = parse_duration,
        help = "Give up on a device command after this long. 0 waits forever"
    )]
    pub timeout: Duration,

    #[command(subcommand)]
    pub command: Command,
}

/// One line of a flow file.
#[derive(Parser, Debug)]
#[command(no_binary_name = true, name = "")]
pub(crate) struct FlowLine {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug, Clone)]
pub(crate) enum Command {
    /// Press one or more buttons
    Key(KeyArgs),
    /// Type lowercase text, one key per character
    Text(TextArgs),
    /// Capture the screen
    #[command(alias = "shot")]
    Screenshot(ShotArgs),
    /// Do nothing for a while
    #[command(alias = "sleep")]
    Wait(WaitArgs),
    /// Launch an app
    Launch(AppArgs),
    /// Close a running app
    Close(AppArgs),
    /// Make one luna call and print the reply
    Luna(LunaArgs),
    /// Run one command on the device
    #[command(alias = "sh")]
    Exec(ExecArgs),
    /// Print a message, to annotate a flow
    Echo(EchoArgs),
    /// Replay a flow file, or `-` for stdin
    Run(RunArgs),
    /// List the key names this tool accepts
    Keys(KeysArgs),
}

impl Command {
    /// What this step is called, for the `--json` stream.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Command::Key(_) => "key",
            Command::Text(_) => "text",
            Command::Screenshot(_) => "screenshot",
            Command::Wait(_) => "wait",
            Command::Launch(_) => "launch",
            Command::Close(_) => "close",
            Command::Luna(_) => "luna",
            Command::Exec(_) => "exec",
            Command::Echo(_) => "echo",
            Command::Run(_) => "run",
            Command::Keys(_) => "keys",
        }
    }

    /// Does this need a device at all?
    ///
    /// `keys` does not, which is what lets it answer with no TV in the room.
    pub(crate) fn needs_device(&self) -> bool {
        !matches!(self, Command::Keys(_))
    }
}

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct KeyArgs {
    #[arg(
        value_name = "KEY",
        required = true,
        num_args = 1..,
        value_parser = keycode::parse,
        help = "Key names (OK, TAB, KEY_ENTER) or raw evdev codes (28, 0x1c)"
    )]
    pub keys: Vec<Key>,

    #[arg(
        long,
        value_name = "MS",
        default_value = "100",
        value_parser = parse_duration,
        help = "Wait between keys. 0 is allowed, and drops keys on a slow app"
    )]
    pub delay: Duration,

    #[arg(
        long,
        value_name = "N",
        default_value_t = 1,
        help = "Send the whole sequence N times"
    )]
    pub repeat: u32,
}

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct TextArgs {
    #[arg(
        value_name = "TEXT",
        help = "Lowercase text; use -- before a leading dash"
    )]
    pub text: String,

    #[arg(long, help = "Fold A-Z to lower case instead of refusing")]
    pub lower: bool,

    #[arg(
        long,
        value_name = "MS",
        default_value = "100",
        value_parser = parse_duration,
        help = "Wait between characters"
    )]
    pub delay: Duration,
}

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct ShotArgs {
    #[arg(
        value_name = "FILE",
        help = "Where to write the PNG. Omit, or pass -, for stdout"
    )]
    pub file: Option<String>,

    #[arg(
        long,
        value_enum,
        ignore_case = true,
        default_value_t = CaptureMethod::Display,
        help = "Which plane to capture"
    )]
    pub method: CaptureMethod,

    #[arg(
        long,
        value_name = "N",
        default_value_t = 1,
        help = "Retries when the captured file is not a whole PNG"
    )]
    pub retries: u8,

    #[arg(
        long,
        value_name = "DIR",
        default_value = "/tmp",
        help = "Where on the DEVICE to stage the capture"
    )]
    pub remote_dir: String,
}

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct WaitArgs {
    #[arg(
        value_name = "TIME",
        value_parser = parse_duration,
        help = "Milliseconds, or a value with a unit: 500ms, 1.5s"
    )]
    pub time: Duration,
}

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct AppArgs {
    #[arg(value_name = "APP_ID", help = "An app id from appinfo.json")]
    pub app_id: String,

    #[arg(
        short,
        long,
        value_name = "PARAMS",
        help = "Launch parameters: key=value, or a {\"json\":\"object\"}. Repeatable"
    )]
    pub params: Vec<String>,
}

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct LunaArgs {
    #[arg(value_name = "URI", help = "A luna:// URI")]
    pub uri: String,

    #[arg(
        value_name = "PAYLOAD",
        default_value = "{}",
        help = "The JSON payload to send"
    )]
    pub payload: String,

    #[arg(
        long,
        help = "Use luna-send-pub. ares-do uses the private bus by default"
    )]
    pub public: bool,

    #[arg(
        long,
        help = "Print the reply and succeed even when returnValue is false"
    )]
    pub allow_false: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct ExecArgs {
    #[arg(value_name = "COMMAND", help = "A shell command to run on the device")]
    pub command: String,
}

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct EchoArgs {
    #[arg(value_name = "MESSAGE", help = "Text to print")]
    pub message: String,
}

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct RunArgs {
    #[arg(value_name = "FLOW", help = "A flow file, or - for stdin")]
    pub flow: String,

    #[arg(
        long,
        value_name = "DIR",
        default_value = ".",
        help = "Where screenshots go"
    )]
    pub out: String,

    #[arg(
        long,
        value_name = "PREFIX",
        default_value = "shot",
        help = "Name for unnamed screenshots, before the number"
    )]
    pub prefix: String,

    #[arg(
        long,
        value_name = "MS",
        default_value = "300",
        value_parser = parse_duration,
        help = "Settle before each screenshot; a key is never done when it returns"
    )]
    pub settle: Duration,

    #[arg(long, help = "Carry on after a failing step, and still exit non-zero")]
    pub keep_going: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct KeysArgs {
    #[arg(value_name = "PATTERN", help = "Show only names containing PATTERN")]
    pub pattern: Option<String>,

    #[arg(long, help = "Every name the kernel defines, not just a remote's")]
    pub all: bool,
}

/// Merge `-p` arguments into one object.
///
/// Same two shapes `ares-launch` accepts — `key=value` and a whole `{...}`
/// object, repeatable and merged — so the two tools take the same input.
/// Unlike `ares-launch`, a param that makes no sense is an error rather than a
/// warning: a launch that quietly dropped half its parameters is worse than
/// one that did not happen.
///
/// # Errors
///
/// A string that is neither shape, or JSON that is not an object.
pub(crate) fn parse_params(params: &[String]) -> Result<Value, String> {
    if params.is_empty() {
        return Ok(Value::Null);
    }
    let mut map = Map::new();
    for p in params {
        if p.starts_with('{') {
            let value: Value = serde_json::from_str(p).map_err(|e| format!("{p}: {e}"))?;
            let Value::Object(object) = value else {
                return Err(format!("{p}: expected a JSON object"));
            };
            map.extend(object);
        } else if let Some((key, value)) = p.split_once('=') {
            map.insert(key.to_string(), json!(value));
        } else {
            return Err(format!(
                "{p}: expected key=value or a JSON object starting with {{"
            ));
        }
    }
    Ok(Value::Object(map))
}

/// `500` and `500ms` are milliseconds; `1.5s` is seconds.
///
/// Bare numbers mean milliseconds because that is what `adb` and `xdotool`
/// take, and a flow full of `wait 2` meaning two seconds would be a trap.
fn parse_duration(value: &str) -> Result<Duration, String> {
    let value = value.trim();
    let (number, scale) = if let Some(rest) = value.strip_suffix("ms") {
        (rest, 1.0)
    } else if let Some(rest) = value.strip_suffix('s') {
        (rest, 1000.0)
    } else {
        (value, 1.0)
    };

    let number: f64 = number
        .trim()
        .parse()
        .map_err(|_| format!("{value:?} is not a time; try 500, 500ms or 1.5s"))?;
    if !number.is_finite() || number < 0.0 {
        return Err(format!("{value:?} is not a time a wait can be"));
    }
    Ok(Duration::from_secs_f64(number * scale / 1000.0))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use clap::Parser;

    use super::{Cli, Command, FlowLine, parse_duration, parse_params};

    #[test]
    fn a_bare_number_is_milliseconds() {
        assert_eq!(parse_duration("500").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
    }

    #[test]
    fn a_unit_is_honoured_and_may_be_fractional() {
        assert_eq!(parse_duration("2s").unwrap(), Duration::from_secs(2));
        assert_eq!(parse_duration("1.5s").unwrap(), Duration::from_millis(1500));
        assert_eq!(parse_duration("0").unwrap(), Duration::ZERO);
    }

    #[test]
    fn nonsense_and_negatives_are_refused() {
        for bad in ["soon", "-1", "1.5x", ""] {
            assert!(parse_duration(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_command_line_and_a_flow_line_parse_the_same_way() {
        let from_cli = Cli::try_parse_from(["ares-do", "key", "TAB", "OK", "--delay", "300"])
            .unwrap()
            .command;
        let from_flow = FlowLine::try_parse_from(["key", "TAB", "OK", "--delay", "300"])
            .unwrap()
            .command;
        assert_eq!(format!("{from_cli:?}"), format!("{from_flow:?}"));
    }

    #[test]
    fn shot_is_screenshot() {
        let a = FlowLine::try_parse_from(["shot", "menu.png"])
            .unwrap()
            .command;
        let b = FlowLine::try_parse_from(["screenshot", "menu.png"])
            .unwrap()
            .command;
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
    }

    #[test]
    fn global_flags_work_before_or_after_the_verb() {
        for args in [
            ["ares-do", "-d", "tv", "key", "OK"],
            ["ares-do", "key", "OK", "-d", "tv"],
        ] {
            let cli = Cli::try_parse_from(args).unwrap();
            assert_eq!(cli.device.as_deref(), Some("tv"));
        }
    }

    #[test]
    fn only_keys_runs_without_a_device() {
        assert!(
            !FlowLine::try_parse_from(["keys"])
                .unwrap()
                .command
                .needs_device()
        );
        assert!(
            FlowLine::try_parse_from(["key", "OK"])
                .unwrap()
                .command
                .needs_device()
        );
    }

    #[test]
    fn a_bad_key_name_fails_at_parse_time() {
        let error = FlowLine::try_parse_from(["key", "OKAY"])
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown key"), "{error}");
    }

    #[test]
    fn params_take_both_shapes_and_merge() {
        let v = parse_params(&[
            String::from("a=1"),
            String::from(r#"{"b":2}"#),
            String::from("c=hello world"),
        ])
        .unwrap();
        assert_eq!(v["a"], "1");
        assert_eq!(v["b"], 2);
        assert_eq!(v["c"], "hello world");
    }

    #[test]
    fn no_params_means_no_params_object() {
        assert!(parse_params(&[]).unwrap().is_null());
    }

    #[test]
    fn a_value_may_contain_an_equals_sign() {
        let v = parse_params(&[String::from("url=http://x/?a=b")]).unwrap();
        assert_eq!(v["url"], "http://x/?a=b");
    }

    #[test]
    fn a_param_that_makes_no_sense_is_refused_not_dropped() {
        // ares-launch warns and carries on. A launch missing half its
        // parameters is worse than one that did not happen.
        assert!(parse_params(&[String::from("bare")]).is_err());
        assert!(parse_params(&[String::from("{not json}")]).is_err());
        assert!(parse_params(&[String::from("[1,2]")]).is_err());
    }

    #[test]
    fn timeout_defaults_to_something_and_zero_is_allowed() {
        let cli = Cli::try_parse_from(["ares-do", "key", "OK"]).unwrap();
        assert_eq!(cli.timeout, Duration::from_secs(30));
        let cli = Cli::try_parse_from(["ares-do", "--timeout", "0", "key", "OK"]).unwrap();
        assert!(cli.timeout.is_zero());
    }

    #[test]
    fn luna_defaults_to_the_private_bus_and_an_empty_payload() {
        let Command::Luna(args) = FlowLine::try_parse_from(["luna", "luna://x/y"])
            .unwrap()
            .command
        else {
            panic!("expected luna");
        };
        assert!(!args.public);
        assert_eq!(args.payload, "{}");
    }

    #[test]
    fn exec_is_also_spelled_sh() {
        let a = FlowLine::try_parse_from(["exec", "ls"]).unwrap().command;
        let b = FlowLine::try_parse_from(["sh", "ls"]).unwrap().command;
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
    }

    #[test]
    fn a_leading_dash_in_text_needs_the_separator() {
        assert!(FlowLine::try_parse_from(["text", "-5"]).is_err());
        let ok = FlowLine::try_parse_from(["text", "--", "-5"]);
        assert!(ok.is_ok(), "{:?}", ok.err());
    }
}
