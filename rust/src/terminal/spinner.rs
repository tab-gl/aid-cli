//! Lightweight progress spinner.
//!
//! v1 animated a spinner while waiting for the browser and while exchanging the
//! authorization code. v2 keeps that feedback but never writes escape sequences
//! when stdout is not a terminal, so piped output stays clean.

use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const INTERVAL: Duration = Duration::from_millis(100);

pub struct Spinner {
    message: Arc<Mutex<String>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    enabled: bool,
}

impl Spinner {
    pub fn start(message: &str) -> Self {
        let enabled = std::io::stdout().is_terminal();
        let message = Arc::new(Mutex::new(message.to_string()));
        let stop = Arc::new(AtomicBool::new(false));

        let handle = if enabled {
            let thread_message = Arc::clone(&message);
            let thread_stop = Arc::clone(&stop);
            Some(std::thread::spawn(move || {
                let mut index = 0;
                while !thread_stop.load(Ordering::Relaxed) {
                    let current = thread_message
                        .lock()
                        .map(|value| value.clone())
                        .unwrap_or_default();
                    let mut stdout = std::io::stdout();
                    let _ = write!(stdout, "\r  {} {}", FRAMES[index % FRAMES.len()], current);
                    let _ = stdout.flush();
                    index += 1;
                    std::thread::sleep(INTERVAL);
                }
            }))
        } else {
            None
        };

        Self {
            message,
            stop,
            handle,
            enabled,
        }
    }

    pub fn update(&self, message: &str) {
        if let Ok(mut current) = self.message.lock() {
            *current = message.to_string();
        }
    }

    /// Finish the spinner and print a success line (same shape as `success`).
    pub fn succeed(mut self, message: &str) {
        self.finish();
        crate::terminal::success(message);
    }

    /// Finish the spinner without printing; the caller reports the error once.
    pub fn fail(mut self) {
        self.finish();
    }

    fn finish(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        if self.enabled {
            let mut stdout = std::io::stdout();
            let _ = write!(stdout, "\r\x1b[2K");
            let _ = stdout.flush();
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_is_a_no_op_without_a_terminal() {
        let spinner = Spinner::start("working");
        assert!(!spinner.enabled);
        spinner.update("still working");
        spinner.fail();
    }
}
