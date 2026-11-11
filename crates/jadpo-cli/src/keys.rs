//! Keys read from a terminal, for the keyboard view and `jadpo test
//! --watch` (CLI design language, "Keys"): the terminal is put in a mode
//! where each key arrives as it is pressed, without echo, and ctrl-c as a
//! key rather than a signal, so the session can put the terminal back
//! before it ends. `stty` sets the mode, as `jadpo config set` hides a
//! secret's echo, so there is no dependency; the mode is given back when
//! [`Raw`] is dropped, whatever ended the session.
//!
//! What reads the keys is any [`std::io::Read`], so a test gives the bytes
//! a terminal would send, and [`Keys`] any source of them.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// One key pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Enter,
    Escape,
    Up,
    Down,
    PageUp,
    PageDown,
    Backspace,
    /// ctrl-c, which quits wherever it is pressed.
    Interrupt,
    Char(char),
}

/// The keys in `bytes`, one read's worth. A terminal sends a key's whole
/// sequence in one write, so an escape at the end of a read is the escape
/// key itself, and one followed by `[` and a letter an arrow.
pub fn decode(bytes: &[u8]) -> Vec<Key> {
    let text = String::from_utf8_lossy(bytes);
    let mut keys = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let key = match c {
            '\r' | '\n' => Key::Enter,
            '\x03' => Key::Interrupt,
            '\x7f' | '\x08' => Key::Backspace,
            '\x10' => Key::Up,
            '\x0e' => Key::Down,
            '\x1b' => match chars.peek() {
                Some('[' | 'O') => {
                    chars.next();
                    // Parameters, then the final letter or `~`.
                    let mut sequence = String::new();
                    for c in chars.by_ref() {
                        sequence.push(c);
                        if c.is_ascii_alphabetic() || c == '~' {
                            break;
                        }
                    }
                    match sequence.as_str() {
                        "A" => Key::Up,
                        "B" => Key::Down,
                        "5~" => Key::PageUp,
                        "6~" => Key::PageDown,
                        _ => continue,
                    }
                }
                _ => Key::Escape,
            },
            c if c.is_control() => continue,
            c => Key::Char(c),
        };
        keys.push(key);
    }
    keys
}

/// The keys a session reads: from a thread reading the terminal, or a
/// test's list. [`Keys::pending`] looks without waiting, so a session that
/// watches saves can stop looking for them when a key comes.
#[derive(Debug)]
pub struct Keys {
    source: Source,
    waiting: RefCell<VecDeque<Key>>,
}

#[derive(Debug)]
enum Source {
    Channel(Receiver<Vec<Key>>),
    Script,
}

impl Keys {
    /// Keys read from `input` on a thread of their own until it ends.
    pub fn reading(mut input: impl Read + Send + 'static) -> Keys {
        let (send, receive) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("jadpo-keys".into())
            .spawn(move || {
                let mut buffer = [0u8; 64];
                loop {
                    match input.read(&mut buffer) {
                        // A raw terminal's read times out with nothing.
                        Ok(0) => {
                            std::thread::sleep(Duration::from_millis(10));
                            if send.send(Vec::new()).is_err() {
                                return;
                            }
                        }
                        Ok(n) => {
                            if send.send(decode(&buffer[..n])).is_err() {
                                return;
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(_) => return,
                    }
                }
            });
        Keys {
            source: Source::Channel(receive),
            waiting: RefCell::new(VecDeque::new()),
        }
    }

    /// These keys, then nothing more: when they run out, ctrl-c.
    pub fn script(keys: impl IntoIterator<Item = Key>) -> Keys {
        Keys {
            source: Source::Script,
            waiting: RefCell::new(keys.into_iter().collect()),
        }
    }

    fn take_ready(&self) {
        if let Source::Channel(receive) = &self.source {
            while let Ok(keys) = receive.try_recv() {
                self.waiting.borrow_mut().extend(keys);
            }
        }
    }

    /// Whether a key is waiting.
    pub fn pending(&self) -> bool {
        self.take_ready();
        !self.waiting.borrow().is_empty()
    }

    /// The next key, waiting for one; ctrl-c when no more can come.
    pub fn next(&self) -> Key {
        loop {
            if let Some(key) = self.waiting.borrow_mut().pop_front() {
                return key;
            }
            match &self.source {
                Source::Script => return Key::Interrupt,
                Source::Channel(receive) => match receive.recv() {
                    Ok(keys) => self.waiting.borrow_mut().extend(keys),
                    Err(_) => return Key::Interrupt,
                },
            }
        }
    }
}

/// The terminal in raw mode until this is dropped: keys arrive as they
/// are pressed, unechoed, ctrl-c among them, and a read with nothing to
/// read gives up after a tenth of a second.
#[derive(Debug)]
pub struct Raw {
    saved: String,
}

impl Raw {
    /// Puts standard input's terminal in raw mode; `None` when it isn't a
    /// terminal `stty` can set.
    pub fn enter() -> Option<Raw> {
        let saved = stty(&["-g"])?;
        stty(&[
            "-icanon", "-echo", "-isig", "-ixon", "min", "0", "time", "1",
        ])?;
        Some(Raw {
            saved: saved.trim().to_string(),
        })
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        let _ = stty(&[self.saved.as_str()]);
    }
}

/// Runs `stty` on standard input's terminal; its output when it succeeds.
fn stty(args: &[&str]) -> Option<String> {
    let output = Command::new("stty")
        .args(args)
        .stdin(Stdio::inherit())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The terminal's rows and columns, asked of `stty` at each redraw so a
/// resize is followed; 24 by 80 when it can't say.
pub fn size() -> (usize, usize) {
    stty(&["size"])
        .and_then(|text| {
            let mut parts = text.split_whitespace();
            let rows = parts.next()?.parse().ok()?;
            let columns = parts.next()?.parse().ok()?;
            Some((rows, columns))
        })
        .filter(|(rows, columns)| *rows > 0 && *columns > 0)
        .unwrap_or((24, 80))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_terminals_bytes_are_keys() {
        assert_eq!(
            decode(b"a\r\x1b[A\x1b[B\x1b\x7f\x03/"),
            [
                Key::Char('a'),
                Key::Enter,
                Key::Up,
                Key::Down,
                Key::Escape,
                Key::Backspace,
                Key::Interrupt,
                Key::Char('/'),
            ]
        );
        assert_eq!(decode(b"\x1b[5~\x1b[6~"), [Key::PageUp, Key::PageDown]);
        assert_eq!(decode(b"\x1bOA"), [Key::Up]);
        // A key with no meaning here is left out, as is a control byte.
        assert_eq!(decode(b"\x1b[C\x01x"), [Key::Char('x')]);
        assert_eq!(decode("é".as_bytes()), [Key::Char('é')]);
    }

    #[test]
    fn a_script_ends_with_ctrl_c() {
        let keys = Keys::script([Key::Char('a')]);
        assert!(keys.pending());
        assert_eq!(keys.next(), Key::Char('a'));
        assert!(!keys.pending());
        assert_eq!(keys.next(), Key::Interrupt);
    }

    #[test]
    fn keys_read_from_a_stream_arrive_in_order() {
        let keys = Keys::reading(std::io::Cursor::new(b"f\x1b[Bq".to_vec()));
        assert_eq!(keys.next(), Key::Char('f'));
        assert_eq!(keys.next(), Key::Down);
        assert_eq!(keys.next(), Key::Char('q'));
    }
}
