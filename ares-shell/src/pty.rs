use std::io::Write;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{select, unbounded, Sender};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::terminal;
use libssh_rs::Error::TryAgain;
use libssh_rs::{Channel, Error};

pub(crate) fn shell(ch: Channel) -> Result<i32, std::io::Error> {
    terminal::enable_raw_mode().unwrap();
    let (tx, rx) = unbounded::<Event>();
    let events = EventThread::new(tx);
    let mut buf = [0; 1024];
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
                    send_key(&mut ch.stdin(), &key)?;
                }
                Ok(Event::Resize(width, height)) => {
                    ch.change_pty_size(width as u32, height as u32)?;
                }
                Ok(e) => {
                }
                Err(_) => {
                    break;
                }
            },
            default => {
                match ch.read_nonblocking(&mut buf, false) {
                     Err(TryAgain) | Ok(0) => {
                        if ch.is_closed() {
                            break;
                        }
                        thread::sleep(Duration::from_millis(1))
                    }
                    Ok(size) => {
                        let mut stdout = std::io::stdout();
                        stdout.write_all(&buf[..size])?;
                        stdout.flush()?;
                    }
                    Err(e) => {
                        return Err(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()));
                    }
                }
            }
        }
    }
    drop(events);
    return Ok(ch.get_exit_status().unwrap_or(-1) as i32);
}

fn send_key<Stdin: Write>(stdin: &mut Stdin, key: &KeyEvent) -> Result<(), Error> {
    match key.code {
        KeyCode::Backspace => {
            stdin.write_all(&[0x08, 0x20, 0x08])?;
        }
        KeyCode::Enter => {
            stdin.write_all(&[0x0d])?;
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
            stdin.write_all(&[0x1b, 0x5b, 0x4f, 0x30 + n])?;
        }
        KeyCode::Char(c) => {
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
            {
                stdin.write_all(&[c as u8 - 0x61])?;
            } else if key.modifiers.contains(crossterm::event::KeyModifiers::ALT) {
                stdin.write_all(&[0x1b, c as u8])?;
            } else {
                stdin.write_all(&[c as u8])?;
            }
        }
        KeyCode::Null => {
            stdin.write_all(&[0x0])?;
        }
        KeyCode::Esc => {
            stdin.write_all(&[0x1b])?;
        }
        _ => {}
    }
    return Ok(());
}

struct EventThread {
    handle: Mutex<Option<JoinHandle<()>>>,
    terminated: Arc<Mutex<bool>>,
}

impl EventThread {
    fn new(tx: Sender<Event>) -> Self {
        let terminated = Arc::new(Mutex::new(false));
        let thread_terminated = Arc::downgrade(&terminated);
        Self {
            terminated,
            handle: Mutex::new(Some(thread::spawn(move || loop {
                if let Some(terminated) = thread_terminated.upgrade() {
                    if *terminated.lock().unwrap() {
                        break;
                    }
                } else {
                    break;
                }
                let Ok(has_event) = crossterm::event::poll(Duration::from_millis(20)) else {
                    break;
                };
                if !has_event {
                    continue;
                }
                let Ok(event) = crossterm::event::read() else {
                    break;
                };
                if !tx.send(event).is_ok() {
                    break;
                }
            }))),
        }
    }
}

impl Drop for EventThread {
    fn drop(&mut self) {
        terminal::disable_raw_mode().unwrap();
        *self.terminated.lock().unwrap() = true;
        if let Some(hnd) = self.handle.lock().unwrap().take() {
            hnd.join().unwrap();
        }
    }
}
