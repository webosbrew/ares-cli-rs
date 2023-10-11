use std::io::Write;

use crossbeam_channel::{select, Receiver};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use libssh_rs::Channel;
use libssh_rs::Error::TryAgain;

pub(crate) fn shell(
    ch: &Channel,
    rx: Receiver<Event>,
    has_pty: bool,
) -> Result<(), std::io::Error> {
    let mut buf = [0; 1024];
    let mut stderr = false;
    loop {
        if ch.is_eof() {
            break;
        }
        select! {
            recv(rx) -> ev => match ev {
                Ok(Event::Key(key)) => {
                    if key.kind == KeyEventKind::Release {
                        continue;
                    }
                    let mut stdin = ch.stdin();
                    match key.code {
                        KeyCode::Backspace => {
                            stdin.write_all(&[0x08, 0x20, 0x08])?;
                        }
                        KeyCode::Enter => {
                            stdin.write_all(&[0xd])?;
                        }
                        KeyCode::Left => {
                            stdin.write_all(&[0x1b, 0x5b, 0x44])?;
                        }
                        KeyCode::Right => {
                            stdin.write_all(&[0x1b, 0x5b, 0x43])?;
                        }
                        KeyCode::Up => {
                            stdin.write_all(&[0x1b, 0x5b, 0x41])?;
                        }
                        KeyCode::Down => {
                            stdin.write_all(&[0x1b, 0x5b, 0x42])?;
                        }
                        KeyCode::Home => {
                            stdin.write_all(&[0x1b, 0x5b, 0x48])?;
                        }
                        KeyCode::End => {
                            stdin.write_all(&[0x1b, 0x5b, 0x46])?;
                        }
                        KeyCode::PageUp => {
                            stdin.write_all(&[0x1b, 0x5b, 0x35, 0x7e])?;
                        }
                        KeyCode::PageDown => {
                            stdin.write_all(&[0x1b, 0x5b, 0x36, 0x7e])?;
                        }
                        KeyCode::Tab => {
                            stdin.write_all(&[0x09])?;
                        }
                        KeyCode::BackTab => {
                            stdin.write_all(&[0x1b, 0x5b, 0x5a])?;
                        }
                        KeyCode::Delete => {
                            stdin.write_all(&[0x1b, 0x5b, 0x33, 0x7e])?;
                        }
                        KeyCode::Insert => {
                            stdin.write_all(&[0x1b, 0x5b, 0x32, 0x7e])?;
                        }
                        KeyCode::F(n) => {
                            stdin.write_all(&[0x1b, 0x5b, 0x4f, 0x30 + n as u8])?;
                        }
                        KeyCode::Char(c) => {
                            if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                                stdin.write_all(&[c as u8 - 0x61])?;
                            } else if key.modifiers.contains(crossterm::event::KeyModifiers::ALT) {
                                stdin.write_all(&[0x1b, c as u8])?;
                            } else {
                                stdin.write_all(&[c as u8])?;
                            }
                        }
                        KeyCode::Null => {
                            stdin.write_all(&[0])?;
                        }
                        KeyCode::Esc => {
                            stdin.write_all(&[27])?;
                        }
                        _ => {}
                    }
                }
                Ok(Event::Resize(width, height)) => {
                    ch.change_pty_size(width as u32, height as u32)?;
                }
                Ok(e) => {
                }
                Err(e) => {
                    break;
                }
            },
            default => {
                match ch.read_nonblocking(&mut buf, stderr) {
                    Ok(size) => {
                        if stderr {
                            let mut stderr = std::io::stderr();
                            stderr.write_all(&buf[..size])?;
                            stderr.flush()?;
                        } else {
                            let mut stdout = std::io::stdout();
                            stdout.write_all(&buf[..size])?;
                            stdout.flush()?;
                        }
                    }
                    Err(TryAgain) => {}
                    Err(e) => {
                        let mut stderr = std::io::stdout();
                        stderr.write_fmt(format_args!("Error: {:?}\r\n", e))?;
                        stderr.flush()?;
                        break;
                    }
                }
                if !has_pty {
                    stderr = !stderr;
                }
            }
        }
    }
    return Ok(());
}
