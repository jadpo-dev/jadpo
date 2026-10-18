//! The live line a slow step shows on stderr (CLI design language,
//! "Timing"): nothing for the first 300 ms, so quick steps never flash;
//! then a spinner in the accent colour and what is running, a frame every
//! 80 ms (130 ms in ASCII), with the elapsed time joining after 2 s. When
//! the step ends the line is erased, and the step's own result follows on
//! stdout. Only where the style animates: stderr a terminal, rich, and not
//! in CI.

use std::io::Write;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::detect::{Style, Symbols};
use crate::palette::Token;

/// How long a step runs before the spinner appears.
pub const SPINNER_DELAY: Duration = Duration::from_millis(300);
/// When the elapsed time joins the live line.
const SHOW_ELAPSED_FROM: Duration = Duration::from_secs(2);

const UNICODE_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const ASCII_FRAMES: [&str; 4] = ["-", "\\", "|", "/"];

/// Clears the line the cursor is on.
const ERASE: &str = "\r\x1b[2K";

/// A spinner running on its own thread until [`Spinner::finish`].
#[derive(Debug)]
pub struct Spinner {
    stop: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
    /// What the live line says, which [`Spinner::say`] changes as the step
    /// goes on (`Filling Todo.priority  2000 of 5000 rows`).
    words: Arc<Mutex<String>>,
}

impl Spinner {
    /// Starts the live line for `words` ("Running tests") on stderr, or
    /// nothing when `style` doesn't animate.
    pub fn start(style: &Style, words: &str) -> Spinner {
        let shared = Arc::new(Mutex::new(words.to_string()));
        if !style.animate {
            return Spinner {
                stop: None,
                thread: None,
                words: shared,
            };
        }
        let (stop, stopped) = mpsc::channel::<()>();
        let style = *style;
        let words = Arc::clone(&shared);
        let thread = std::thread::spawn(move || {
            let started = Instant::now();
            if !matches!(
                stopped.recv_timeout(SPINNER_DELAY),
                Err(RecvTimeoutError::Timeout)
            ) {
                return;
            }
            let (frames, every): (&[&str], _) = match style.symbols {
                Symbols::Unicode => (&UNICODE_FRAMES, Duration::from_millis(80)),
                Symbols::Ascii => (&ASCII_FRAMES, Duration::from_millis(130)),
            };
            let mut stderr = std::io::stderr();
            for i in 0.. {
                let now = words.lock().map(|w| w.clone()).unwrap_or_default();
                let line = frame(&style, frames[i % frames.len()], &now, started.elapsed());
                let _ = stderr.write_all(format!("{ERASE}{line}").as_bytes());
                let _ = stderr.flush();
                if !matches!(stopped.recv_timeout(every), Err(RecvTimeoutError::Timeout)) {
                    break;
                }
            }
            let _ = stderr.write_all(ERASE.as_bytes());
            let _ = stderr.flush();
        });
        Spinner {
            stop: Some(stop),
            thread: Some(thread),
            words: shared,
        }
    }

    /// What the live line says, shared, for code that can't hold the
    /// spinner to change it.
    pub fn words(&self) -> Arc<Mutex<String>> {
        Arc::clone(&self.words)
    }

    /// Changes what the live line says, from its next frame.
    pub fn say(&self, words: &str) {
        if let Ok(mut now) = self.words.lock() {
            *now = words.to_string();
        }
    }

    /// Stops the spinner and erases its line, before the result is written.
    pub fn finish(mut self) {
        self.stop.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// One frame of the live line: the spinner in the accent colour, what is
/// running, and after 2 s the time so far, muted.
pub fn frame(style: &Style, symbol: &str, words: &str, elapsed: Duration) -> String {
    let paint = |token: Token, text: &str| match style.color {
        Some(depth) => format!("\x1b[{}m{text}\x1b[0m", token.sgr(depth, style.theme)),
        None => text.to_string(),
    };
    let mut line = format!("{} {words}", paint(Token::Accent, symbol));
    if elapsed >= SHOW_ELAPSED_FROM {
        line.push_str(&format!(
            "  {}",
            paint(Token::Muted, &format!("{} s", elapsed.as_secs()))
        ));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::Mode;

    #[test]
    fn a_frame_shows_the_time_only_after_two_seconds() {
        let style = Style::new(Mode::Rich);
        assert_eq!(
            frame(&style, "⠋", "Running tests", Duration::from_millis(500)),
            "⠋ Running tests"
        );
        assert_eq!(
            frame(&style, "⠙", "Running tests", Duration::from_millis(3200)),
            "⠙ Running tests  3 s"
        );
    }

    #[test]
    fn nothing_runs_where_the_style_doesnt_animate() {
        let spinner = Spinner::start(&Style::new(Mode::Rich), "Running tests");
        assert!(spinner.thread.is_none());
        spinner.finish();
    }
}
