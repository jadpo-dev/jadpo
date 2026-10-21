//! A JSON document as Jadpo reads one (chunk 5.8c): RFC 8259's grammar,
//! as `serde_json` reads it, with two differences. An object that names a
//! member twice isn't a document Jadpo reads at all, as a JWT's claims
//! aren't (chunk 5.5r): a reader that kept either one could take what its
//! sender didn't mean, and readers differ on which. And each number's own
//! text is kept beside the value, by where it stands, so a vector's item
//! is the single-precision float nearest what was written, never the
//! float nearest the double nearest it.

use std::collections::HashMap;

use serde_json::{Map, Number, Value};

/// How deep arrays and objects may nest, as `serde_json` allows.
const DEPTH: usize = 128;

/// Each number's text as written, by where it stands in the document: its
/// members' names and its items' places from the top.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Numbers(HashMap<Vec<String>, String>);

impl Numbers {
    /// The text of the number at `at`, if one stands there.
    pub fn get(&self, at: &[String]) -> Option<&str> {
        self.0.get(at).map(String::as_str)
    }

    /// The numbers inside the value at `prefix`, by where they stand from
    /// it: a provider's answer inside its envelope (chunk 7.2c2).
    pub fn within(&self, prefix: &[String]) -> Numbers {
        Numbers(
            self.0
                .iter()
                .filter_map(|(at, text)| {
                    let rest = at.strip_prefix(prefix)?;
                    Some((rest.to_vec(), text.clone()))
                })
                .collect(),
        )
    }
}

/// The document `bytes` hold, with its numbers' texts; `None` when they
/// aren't one JSON document in UTF-8, or an object names a member twice.
pub fn parse(bytes: &[u8]) -> Option<(Value, Numbers)> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut reader = Reader {
        text,
        at: 0,
        path: Vec::new(),
        numbers: Numbers::default(),
    };
    reader.space();
    let value = reader.value(0)?;
    reader.space();
    (reader.at == text.len()).then_some((value, reader.numbers))
}

struct Reader<'t> {
    text: &'t str,
    at: usize,
    path: Vec<String>,
    numbers: Numbers,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn word(&mut self, word: &str, value: Value) -> Option<Value> {
        self.text[self.at..].starts_with(word).then(|| {
            self.at += word.len();
            value
        })
    }

    fn value(&mut self, depth: usize) -> Option<Value> {
        match self.peek()? {
            b'{' => self.object(depth + 1),
            b'[' => self.array(depth + 1),
            b'"' => self.string().map(Value::String),
            b't' => self.word("true", Value::Bool(true)),
            b'f' => self.word("false", Value::Bool(false)),
            b'n' => self.word("null", Value::Null),
            b'-' | b'0'..=b'9' => self.number(),
            _ => None,
        }
    }

    fn object(&mut self, depth: usize) -> Option<Value> {
        if depth > DEPTH {
            return None;
        }
        self.at += 1;
        let mut members = Map::new();
        self.space();
        if self.peek()? == b'}' {
            self.at += 1;
            return Some(Value::Object(members));
        }
        loop {
            self.space();
            if self.peek()? != b'"' {
                return None;
            }
            let name = self.string()?;
            if members.contains_key(&name) {
                return None;
            }
            self.space();
            if self.peek()? != b':' {
                return None;
            }
            self.at += 1;
            self.space();
            self.path.push(name.clone());
            let value = self.value(depth)?;
            self.path.pop();
            members.insert(name, value);
            self.space();
            match self.peek()? {
                b',' => self.at += 1,
                b'}' => {
                    self.at += 1;
                    return Some(Value::Object(members));
                }
                _ => return None,
            }
        }
    }

    fn array(&mut self, depth: usize) -> Option<Value> {
        if depth > DEPTH {
            return None;
        }
        self.at += 1;
        let mut items = Vec::new();
        self.space();
        if self.peek()? == b']' {
            self.at += 1;
            return Some(Value::Array(items));
        }
        loop {
            self.space();
            self.path.push(items.len().to_string());
            let item = self.value(depth)?;
            self.path.pop();
            items.push(item);
            self.space();
            match self.peek()? {
                b',' => self.at += 1,
                b']' => {
                    self.at += 1;
                    return Some(Value::Array(items));
                }
                _ => return None,
            }
        }
    }

    /// A string, its escapes read as `serde_json` reads them.
    fn string(&mut self) -> Option<String> {
        let start = self.at;
        self.at += 1;
        loop {
            match self.peek()? {
                b'"' => break,
                b'\\' => self.at += 2,
                _ => self.at += 1,
            }
        }
        self.at += 1;
        serde_json::from_str(self.text.get(start..self.at)?).ok()
    }

    /// A number: `-`, a whole part without leading zeros, then a fraction
    /// and an exponent, each optional; kept as `serde_json` keeps it, a
    /// whole number that fits as one and anything else as a finite double,
    /// with its text beside it.
    fn number(&mut self) -> Option<Value> {
        let bytes = self.text.as_bytes();
        let start = self.at;
        let digits = |at: &mut usize| {
            let from = *at;
            while bytes.get(*at).is_some_and(u8::is_ascii_digit) {
                *at += 1;
            }
            *at > from
        };
        let mut at = start;
        if bytes.get(at) == Some(&b'-') {
            at += 1;
        }
        if bytes.get(at) == Some(&b'0') {
            at += 1;
        } else if !digits(&mut at) {
            return None;
        }
        let mut whole = true;
        if bytes.get(at) == Some(&b'.') {
            at += 1;
            whole = false;
            if !digits(&mut at) {
                return None;
            }
        }
        if matches!(bytes.get(at), Some(b'e' | b'E')) {
            at += 1;
            whole = false;
            if matches!(bytes.get(at), Some(b'+' | b'-')) {
                at += 1;
            }
            if !digits(&mut at) {
                return None;
            }
        }
        self.at = at;
        let written = &self.text[start..at];
        let number = number_of(written, whole)?;
        self.numbers
            .0
            .insert(self.path.clone(), written.to_string());
        Some(Value::Number(number))
    }
}

/// The number `written` is, as `serde_json` keeps it: a whole number that
/// fits as one and anything else as a finite double; `None` past a
/// double's range. `whole` when it has no fraction or exponent.
pub(super) fn number_of(written: &str, whole: bool) -> Option<Number> {
    Some(if whole && written == "-0" {
        Number::from_f64(-0.0)?
    } else if let Some(n) = whole.then(|| written.parse::<u64>().ok()).flatten() {
        Number::from(n)
    } else if let Some(n) = whole.then(|| written.parse::<i64>().ok()).flatten() {
        Number::from(n)
    } else {
        let double: f64 = written.parse().ok()?;
        if !double.is_finite() {
            return None;
        }
        Number::from_f64(double)?
    })
}

impl Numbers {
    /// Each number's text by where it stands, as given.
    pub(super) fn from_map(numbers: HashMap<Vec<String>, String>) -> Numbers {
        Numbers(numbers)
    }
}
