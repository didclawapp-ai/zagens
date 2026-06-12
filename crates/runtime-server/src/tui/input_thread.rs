//! Dedicated blocking thread for terminal input (avoids `spawn_blocking` poll races).

use std::thread::{self, JoinHandle};

use crossterm::event::{self, Event};
use tokio::sync::mpsc;

/// Async receiver bridged from a std thread that blocks on `event::read()`.
pub struct TerminalInput {
    rx: Option<mpsc::Receiver<Event>>,
    thread: Option<JoinHandle<()>>,
}

impl TerminalInput {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel(256);
        let thread = thread::spawn(move || {
            loop {
                match event::read() {
                    Ok(ev) => {
                        if tx.blocking_send(ev).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            rx: Some(rx),
            thread: Some(thread),
        }
    }

    pub async fn recv(&mut self) -> Option<Event> {
        self.rx.as_mut()?.recv().await
    }
}

impl Drop for TerminalInput {
    fn drop(&mut self) {
        self.rx.take();
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}
