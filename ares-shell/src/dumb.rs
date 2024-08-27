use std::io::{stdin, Error, Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{select, unbounded, Sender};
use libssh_rs::Channel;
use libssh_rs::Error::TryAgain;

pub(crate) fn shell(ch: Channel) -> Result<i32, Error> {
    let (tx, rx) = unbounded::<Vec<u8>>();
    let events = EventThread::new(tx);
    let mut buf = [0; 1024];
    let mut stderr = false;
    loop {
        if ch.is_eof() {
            break;
        }
        select! {
            recv(rx) -> item => match item {
                Ok(data) => {
                    ch.stdin().write_all(&data[..])?;
                    ch.stdin().flush()?;
                }
                Err(_) => {
                    break;
                }
            },
            default => {
                match ch.read_nonblocking(&mut buf, stderr) {
                    Err(TryAgain) | Ok(0) => {
                        if ch.is_closed() {
                            break;
                        }
                        thread::sleep(Duration::from_millis(1))
                    }
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
                    Err(e) => {
                        return Err(Error::new(std::io::ErrorKind::Other, e.to_string()));
                    }
                }
                stderr = !stderr;
            }
        }
    }
    drop(events);
    Ok(ch.get_exit_status().unwrap_or(-1) as i32)
}

struct EventThread {
    handle: Mutex<Option<JoinHandle<()>>>,
    terminated: Arc<Mutex<bool>>,
}

impl EventThread {
    fn new(tx: Sender<Vec<u8>>) -> Self {
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
                let mut buf = [0; 1024];
                match stdin().read(&mut buf) {
                    Ok(size) => {
                        if !tx.send(buf[..size].to_vec()).is_ok() {
                            break;
                        }
                    }
                    Err(_) => {
                        break;
                    }
                }
            }))),
        }
    }
}

impl Drop for EventThread {
    fn drop(&mut self) {
        *self.terminated.lock().unwrap() = true;
    }
}
