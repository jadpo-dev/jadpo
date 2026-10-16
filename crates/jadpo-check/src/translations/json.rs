//! Reading a translation file (area 10, "Translations"): a JSON object
//! whose keys are the translation keys and whose values are messages,
//! strings. A general JSON reader would lose where each key and message
//! sits, which the errors point at, so this one keeps the byte range of
//! every key and message in the file, and for a message the file position
//! of each of its characters (an escape such as `é` is six bytes of
//! file and one character of message).

/// One `"key": "message"` pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pair {
    pub key: String,
    /// The key with its quotes.
    pub key_range: (u32, u32),
    pub message: String,
    /// The message with its quotes.
    pub message_range: (u32, u32),
    /// For each byte of `message`, and one past the end, the file offset
    /// it was written at.
    pub offsets: Vec<u32>,
}

/// A file that is not an object of messages, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonError {
    pub range: (u32, u32),
    pub label: String,
    pub reason: String,
    pub next: String,
}

/// The pairs of the object `text` holds, in order (a key may appear twice;
/// the caller says so), and where the object opens.
pub struct Object {
    pub open: u32,
    pub pairs: Vec<Pair>,
}

pub fn read(text: &str) -> Result<Object, JsonError> {
    let mut reader = Reader {
        chars: text.char_indices().collect(),
        at: 0,
        length: text.len(),
    };
    reader.object()
}

struct Reader {
    chars: Vec<(usize, char)>,
    at: usize,
    length: usize,
}

type Read<T> = Result<T, JsonError>;

fn error(start: usize, end: usize, label: &str, reason: &str, next: &str) -> JsonError {
    let start = u32::try_from(start).unwrap_or(u32::MAX);
    let end = u32::try_from(end).unwrap_or(u32::MAX).max(start);
    JsonError {
        range: (start, end),
        label: label.to_string(),
        reason: reason.to_string(),
        next: next.to_string(),
    }
}

const SHAPE: &str = "A translation file is one JSON object: each key is a translation key and each value its message, a string in ICU MessageFormat.";

impl Reader {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).map(|(_, c)| *c)
    }

    fn offset(&self) -> usize {
        self.chars.get(self.at).map_or(self.length, |(at, _)| *at)
    }

    /// The offset just past the character the cursor is on, for a range
    /// that covers it.
    fn after(&self) -> usize {
        self.chars
            .get(self.at)
            .map_or(self.length, |(at, c)| at + c.len_utf8())
    }

    fn skip_space(&mut self) {
        if self.at == 0 && self.peek() == Some('\u{feff}') {
            self.at += 1;
        }
        while self.peek().is_some_and(|c| matches!(c, ' ' | '\t' | '\n' | '\r')) {
            self.at += 1;
        }
    }

    fn object(&mut self) -> Read<Object> {
        self.skip_space();
        let open = self.offset();
        match self.peek() {
            Some('{') => self.at += 1,
            None => {
                return Err(error(
                    0,
                    0,
                    "empty",
                    SHAPE,
                    "Write the file as an object, `{ \"key\": \"message\" }`",
                ));
            }
            Some(_) => {
                return Err(error(
                    open,
                    self.after(),
                    "not an object",
                    SHAPE,
                    "Start the file with `{`",
                ));
            }
        }
        let mut pairs: Vec<Pair> = Vec::new();
        self.skip_space();
        if self.peek() == Some('}') {
            self.at += 1;
            return self.end(Object {
                open: u32::try_from(open).unwrap_or(0),
                pairs,
            });
        }
        loop {
            self.skip_space();
            let key = match self.peek() {
                Some('"') => self.string()?,
                Some('}') => {
                    let at = self.offset();
                    return Err(error(
                        at.saturating_sub(1),
                        self.after(),
                        "trailing comma",
                        "JSON has no comma after the last pair.",
                        "Remove the comma before the `}`",
                    ));
                }
                None => {
                    return Err(error(
                        open,
                        open + 1,
                        "never closed",
                        SHAPE,
                        "Close the object with `}`",
                    ));
                }
                Some(_) => {
                    return Err(error(
                        self.offset(),
                        self.after(),
                        "not a key",
                        "A key is a string in double quotes.",
                        "Write the key in double quotes",
                    ));
                }
            };
            self.skip_space();
            if self.peek() != Some(':') {
                return Err(error(
                    self.offset(),
                    self.after(),
                    "expected `:`",
                    "A key is followed by a colon and its message.",
                    "Write `:` after the key",
                ));
            }
            self.at += 1;
            self.skip_space();
            let message = match self.peek() {
                Some('"') => self.string()?,
                None => {
                    return Err(error(
                        open,
                        open + 1,
                        "never closed",
                        SHAPE,
                        "Close the object with `}`",
                    ));
                }
                Some(_) => {
                    let start = self.offset();
                    self.skip_value();
                    return Err(error(
                        start,
                        self.offset().max(start + 1),
                        "not a message",
                        "A message is a string. Messages are flat: a nested object or list isn't a translation, and a number is not text.",
                        "Write the message as a string in double quotes",
                    ));
                }
            };
            pairs.push(Pair {
                key: key.text,
                key_range: key.range,
                message: message.text,
                message_range: message.range,
                offsets: message.offsets,
            });
            self.skip_space();
            match self.peek() {
                Some(',') => self.at += 1,
                Some('}') => {
                    self.at += 1;
                    return self.end(Object {
                        open: u32::try_from(open).unwrap_or(0),
                        pairs,
                    });
                }
                None => {
                    return Err(error(
                        open,
                        open + 1,
                        "never closed",
                        SHAPE,
                        "Close the object with `}`",
                    ));
                }
                Some(_) => {
                    return Err(error(
                        self.offset(),
                        self.after(),
                        "expected `,`",
                        "Pairs are separated by commas.",
                        "Write `,` after the message, or `}` if it is the last",
                    ));
                }
            }
        }
    }

    /// Nothing but space may follow the object.
    fn end(&mut self, object: Object) -> Read<Object> {
        self.skip_space();
        if self.peek().is_some() {
            return Err(error(
                self.offset(),
                self.after(),
                "after the object",
                SHAPE,
                "Remove what follows the closing `}`",
            ));
        }
        Ok(object)
    }

    /// Steps over a value that isn't a string, to say where it ends.
    fn skip_value(&mut self) {
        let mut depth = 0usize;
        while let Some(c) = self.peek() {
            match c {
                '{' | '[' => depth += 1,
                '}' | ']' if depth == 0 => return,
                '}' | ']' => depth -= 1,
                ',' if depth == 0 => return,
                '"' => {
                    let _ = self.string();
                    continue;
                }
                _ => {}
            }
            self.at += 1;
            if depth == 0
                && self
                    .peek()
                    .is_some_and(|c| matches!(c, ' ' | '\t' | '\n' | '\r'))
            {
                return;
            }
        }
    }

    /// A string in double quotes.
    fn string(&mut self) -> Read<Text> {
        let start = self.offset();
        self.at += 1;
        let mut text = String::new();
        let mut offsets: Vec<u32> = Vec::new();
        let to_u32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        loop {
            let here = self.offset();
            let Some(c) = self.peek() else {
                return Err(error(
                    start,
                    start + 1,
                    "never closed",
                    "A string ends with a closing double quote.",
                    "Close the string with `\"`",
                ));
            };
            match c {
                '"' => {
                    self.at += 1;
                    offsets.push(to_u32(here));
                    return Ok(Text {
                        text,
                        range: (to_u32(start), to_u32(self.offset())),
                        offsets,
                    });
                }
                '\\' => {
                    self.at += 1;
                    let Some(escape) = self.peek() else {
                        return Err(error(
                            here,
                            here + 1,
                            "unfinished escape",
                            "A backslash begins an escape such as `\\n` or `\\\"`.",
                            "Finish the escape, or write the backslash as `\\\\`",
                        ));
                    };
                    self.at += 1;
                    let decoded = match escape {
                        '"' => '"',
                        '\\' => '\\',
                        '/' => '/',
                        'b' => '\u{8}',
                        'f' => '\u{c}',
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        'u' => self.unicode(here)?,
                        _ => {
                            return Err(error(
                                here,
                                self.offset(),
                                "no such escape",
                                "JSON escapes are `\\\"`, `\\\\`, `\\/`, `\\b`, `\\f`, `\\n`, `\\r`, `\\t` and `\\u` with four hexadecimal digits.",
                                "Write the backslash as `\\\\`, or use one of the escapes",
                            ));
                        }
                    };
                    for _ in 0..decoded.len_utf8() {
                        offsets.push(to_u32(here));
                    }
                    text.push(decoded);
                }
                c if (c as u32) < 0x20 => {
                    return Err(error(
                        here,
                        here + 1,
                        "a line break in a string",
                        "A JSON string can't hold a raw line break or control character.",
                        "Write a line break as `\\n`",
                    ));
                }
                c => {
                    self.at += 1;
                    for _ in 0..c.len_utf8() {
                        offsets.push(to_u32(here));
                    }
                    text.push(c);
                }
            }
        }
    }

    /// The four digits after `\u`, joined with a second escape when they
    /// are the first half of a surrogate pair.
    fn unicode(&mut self, escape_at: usize) -> Read<char> {
        let first = self.hex4(escape_at)?;
        if (0xD800..0xDC00).contains(&first) {
            if self.peek() == Some('\\') && self.chars.get(self.at + 1).map(|(_, c)| *c) == Some('u')
            {
                self.at += 2;
                let second = self.hex4(escape_at)?;
                if (0xDC00..0xE000).contains(&second) {
                    let code = 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00);
                    if let Some(c) = char::from_u32(code) {
                        return Ok(c);
                    }
                }
            }
            return Err(error(
                escape_at,
                self.offset(),
                "half a character",
                "A `\\u` escape in the range D800 to DBFF must be followed by one in DC00 to DFFF.",
                "Write the character itself, or both halves",
            ));
        }
        char::from_u32(first).ok_or_else(|| {
            error(
                escape_at,
                self.offset(),
                "half a character",
                "This `\\u` escape is not a character.",
                "Write the character itself",
            )
        })
    }

    fn hex4(&mut self, escape_at: usize) -> Read<u32> {
        let mut value = 0u32;
        for _ in 0..4 {
            let digit = self.peek().and_then(|c| c.to_digit(16));
            let Some(digit) = digit else {
                return Err(error(
                    escape_at,
                    self.after(),
                    "not four digits",
                    "A `\\u` escape is followed by four hexadecimal digits.",
                    "Write four hexadecimal digits after `\\u`",
                ));
            };
            self.at += 1;
            value = value * 16 + digit;
        }
        Ok(value)
    }
}

struct Text {
    text: String,
    range: (u32, u32),
    offsets: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(text: &str) -> Vec<(String, String)> {
        read(text)
            .map(|o| o.pairs.into_iter().map(|p| (p.key, p.message)).collect())
            .unwrap_or_default()
    }

    #[test]
    fn reads_pairs_with_escapes() {
        assert_eq!(
            pairs("{\"a\": \"x\\n\\u00e9\\\"y\", \"b\": \"\"}"),
            [("a".into(), "x\né\"y".into()), ("b".into(), String::new())]
        );
        assert_eq!(pairs("{}"), []);
        assert_eq!(pairs("\u{feff} { \"a\" : \"b\" } \n"), [("a".into(), "b".into())]);
    }

    #[test]
    fn a_surrogate_pair_is_one_character() {
        assert_eq!(pairs("{\"a\": \"\\ud83d\\ude00\"}"), [("a".into(), "😀".into())]);
    }

    #[test]
    fn offsets_follow_the_file() {
        let text = "{\"a\": \"c\\u00e9!\"}";
        let object = read(text).ok().unwrap_or(Object { open: 0, pairs: vec![] });
        let pair = &object.pairs[0];
        // `c` at 7, `é` (two bytes) from the escape at 8, `!` after it, the
        // closing quote last.
        assert_eq!(pair.offsets, [7, 8, 8, 14, 15]);
    }

    #[test]
    fn mistakes_say_where() {
        let at = |text: &str| read(text).err().map(|e| (e.range, e.label));
        assert_eq!(at("[]"), Some(((0, 1), "not an object".into())));
        assert_eq!(at(""), Some(((0, 0), "empty".into())));
        assert_eq!(at("{\"a\": 1}").map(|(r, _)| r.0), Some(6));
        assert_eq!(at("{\"a\": \"b\",}").map(|(_, l)| l), Some("trailing comma".into()));
        assert_eq!(at("{\"a\" \"b\"}").map(|(_, l)| l), Some("expected `:`".into()));
        assert_eq!(at("{\"a\": \"b\"").map(|(_, l)| l), Some("never closed".into()));
        assert_eq!(at("{\"a\": \"b\"} x").map(|(_, l)| l), Some("after the object".into()));
        assert_eq!(at("{\"a\": \"b\nc\"}").map(|(_, l)| l), Some("a line break in a string".into()));
    }
}
