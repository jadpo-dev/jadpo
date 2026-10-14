//! Canonical JSON (docs/compiler-runtime.md §2.3): one writer for every
//! file the graph produces, so the same graph always gives the same bytes.
//! Object keys are sorted, the indent is two spaces, lines end in LF, the
//! document ends in one newline, and there are no floats or timestamps.
//! Text is written as UTF-8, escaping only what JSON requires.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// A JSON value whose objects keep their keys sorted.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    /// An empty object.
    pub fn object() -> Value {
        Value::Object(BTreeMap::new())
    }

    /// Sets `key` on an object; anything else is left alone.
    pub fn set(&mut self, key: &str, value: impl Into<Value>) {
        if let Value::Object(entries) = self {
            entries.insert(key.to_string(), value.into());
        }
    }

    /// Sets `key` only when there is something to say: not `null`, `false`,
    /// an empty list or an empty object, so absent and empty read the same.
    pub fn set_some(&mut self, key: &str, value: impl Into<Value>) {
        let value = value.into();
        let empty = match &value {
            Value::Null | Value::Bool(false) => true,
            Value::Array(items) => items.is_empty(),
            Value::Object(entries) => entries.is_empty(),
            _ => false,
        };
        if !empty {
            self.set(key, value);
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(entries) => entries.get(key),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    /// The document as written to disk.
    pub fn to_document(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out.push('\n');
        out
    }

    fn write(&self, out: &mut String, indent: usize) {
        match self {
            Value::Null => out.push_str("null"),
            Value::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Value::Int(value) => {
                let _ = write!(out, "{value}");
            }
            Value::Str(value) => write_string(out, value),
            Value::Array(items) if items.is_empty() => out.push_str("[]"),
            Value::Object(entries) if entries.is_empty() => out.push_str("{}"),
            Value::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    out.push_str(if i == 0 { "\n" } else { ",\n" });
                    pad(out, indent + 1);
                    item.write(out, indent + 1);
                }
                out.push('\n');
                pad(out, indent);
                out.push(']');
            }
            Value::Object(entries) => {
                out.push('{');
                for (i, (key, value)) in entries.iter().enumerate() {
                    out.push_str(if i == 0 { "\n" } else { ",\n" });
                    pad(out, indent + 1);
                    write_string(out, key);
                    out.push_str(": ");
                    value.write(out, indent + 1);
                }
                out.push('\n');
                pad(out, indent);
                out.push('}');
            }
        }
    }
}

fn pad(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
}

fn write_string(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

impl From<&str> for Value {
    fn from(value: &str) -> Value {
        Value::Str(value.to_string())
    }
}

impl From<String> for Value {
    fn from(value: String) -> Value {
        Value::Str(value)
    }
}

impl From<&String> for Value {
    fn from(value: &String) -> Value {
        Value::Str(value.clone())
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Value {
        Value::Bool(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Value {
        Value::Int(value)
    }
}

impl From<u32> for Value {
    fn from(value: u32) -> Value {
        Value::Int(i64::from(value))
    }
}

impl From<u16> for Value {
    fn from(value: u16) -> Value {
        Value::Int(i64::from(value))
    }
}

impl From<u8> for Value {
    fn from(value: u8) -> Value {
        Value::Int(i64::from(value))
    }
}

impl From<usize> for Value {
    fn from(value: usize) -> Value {
        Value::Int(i64::try_from(value).unwrap_or(i64::MAX))
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Value {
        value.map_or(Value::Null, Into::into)
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(value: Vec<T>) -> Value {
        Value::Array(value.into_iter().map(Into::into).collect())
    }
}

impl From<BTreeMap<String, Value>> for Value {
    fn from(value: BTreeMap<String, Value>) -> Value {
        Value::Object(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_sorted_and_the_layout_is_fixed() {
        let mut value = Value::object();
        value.set("zeta", 1i64);
        value.set("alpha", vec!["b", "a"]);
        value.set("empty", Value::object());
        assert_eq!(
            value.to_document(),
            "{\n  \"alpha\": [\n    \"b\",\n    \"a\"\n  ],\n  \"empty\": {},\n  \"zeta\": 1\n}\n"
        );
    }

    #[test]
    fn text_escapes_only_what_json_requires() {
        let value = Value::from("a\"b\\c\nd\u{1}é→");
        assert_eq!(value.to_document(), "\"a\\\"b\\\\c\\nd\\u0001é→\"\n");
    }

    #[test]
    fn set_some_leaves_out_what_says_nothing() {
        let mut value = Value::object();
        value.set_some("none", Value::Null);
        value.set_some("no", false);
        value.set_some("list", Vec::<Value>::new());
        value.set_some("yes", true);
        assert_eq!(value.to_document(), "{\n  \"yes\": true\n}\n");
    }
}

/// Reads a JSON document into a [`Value`]: what a committed record such as
/// a migration's `snapshot.json` is read back with. Numbers are whole
/// numbers that fit an `i64`, as everything this crate writes; anything
/// else, and text after the document, is an error saying where.
pub fn parse(text: &str) -> Result<Value, String> {
    let mut reader = Reader {
        text,
        bytes: text.as_bytes(),
        at: 0,
        depth: 0,
    };
    let value = reader.value()?;
    reader.space();
    if reader.at != reader.bytes.len() {
        return Err(reader.error("text after the document"));
    }
    Ok(value)
}

struct Reader<'a> {
    text: &'a str,
    bytes: &'a [u8],
    at: usize,
    depth: usize,
}

/// Nesting deeper than this is refused rather than recursed into.
const MAX_DEPTH: usize = 64;

impl Reader<'_> {
    fn error(&self, what: &str) -> String {
        let line = 1 + self.bytes[..self.at.min(self.bytes.len())]
            .iter()
            .filter(|b| **b == b'\n')
            .count();
        format!("{what} on line {line}")
    }

    fn space(&mut self) {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        self.space();
        if self.bytes.get(self.at) == Some(&byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn word(&mut self, word: &str, value: Value) -> Result<Value, String> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.error("an unknown word"))
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.space();
        match self.bytes.get(self.at) {
            None => Err(self.error("the document ends early")),
            Some(b'{') => self.nested(|reader| reader.object()),
            Some(b'[') => self.nested(|reader| reader.array()),
            Some(b'"') => self.string().map(Value::Str),
            Some(b't') => self.word("true", Value::Bool(true)),
            Some(b'f') => self.word("false", Value::Bool(false)),
            Some(b'n') => self.word("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("an unexpected character")),
        }
    }

    fn nested(
        &mut self,
        read: impl FnOnce(&mut Self) -> Result<Value, String>,
    ) -> Result<Value, String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error("nesting too deep"));
        }
        let value = read(self);
        self.depth -= 1;
        value
    }

    fn object(&mut self) -> Result<Value, String> {
        self.at += 1;
        let mut entries = BTreeMap::new();
        if self.eat(b'}') {
            return Ok(Value::Object(entries));
        }
        loop {
            self.space();
            if self.bytes.get(self.at) != Some(&b'"') {
                return Err(self.error("a key that isn't text"));
            }
            let key = self.string()?;
            if !self.eat(b':') {
                return Err(self.error("a missing `:`"));
            }
            let value = self.value()?;
            if entries.insert(key, value).is_some() {
                return Err(self.error("a key given twice"));
            }
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                return Ok(Value::Object(entries));
            }
            return Err(self.error("a missing `,` or `}`"));
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.at += 1;
        let mut items = Vec::new();
        if self.eat(b']') {
            return Ok(Value::Array(items));
        }
        loop {
            items.push(self.value()?);
            if self.eat(b',') {
                continue;
            }
            if self.eat(b']') {
                return Ok(Value::Array(items));
            }
            return Err(self.error("a missing `,` or `]`"));
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.at;
        if self.bytes.get(self.at) == Some(&b'-') {
            self.at += 1;
        }
        while self.bytes.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        if self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(b, b'.' | b'e' | b'E'))
        {
            return Err(self.error("a number that isn't whole"));
        }
        std::str::from_utf8(&self.bytes[start..self.at])
            .ok()
            .and_then(|digits| digits.parse().ok())
            .map(Value::Int)
            .ok_or_else(|| self.error("a number out of range"))
    }

    fn string(&mut self) -> Result<String, String> {
        self.at += 1;
        let mut out = String::new();
        loop {
            let Some(&byte) = self.bytes.get(self.at) else {
                return Err(self.error("text that never ends"));
            };
            match byte {
                b'"' => {
                    self.at += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.at += 1;
                    let escaped = self.bytes.get(self.at).copied();
                    self.at += 1;
                    match escaped {
                        Some(b'"') => out.push('"'),
                        Some(b'\\') => out.push('\\'),
                        Some(b'/') => out.push('/'),
                        Some(b'n') => out.push('\n'),
                        Some(b'r') => out.push('\r'),
                        Some(b't') => out.push('\t'),
                        Some(b'b') => out.push('\u{8}'),
                        Some(b'f') => out.push('\u{c}'),
                        Some(b'u') => {
                            let code = self.hex4()?;
                            let c = if (0xd800..0xdc00).contains(&code) {
                                if !self.bytes[self.at..].starts_with(b"\\u") {
                                    return Err(self.error("half a surrogate pair"));
                                }
                                self.at += 2;
                                let low = self.hex4()?;
                                char::from_u32(0x10000 + ((code - 0xd800) << 10) + (low - 0xdc00))
                            } else {
                                char::from_u32(code)
                            };
                            out.push(
                                c.ok_or_else(|| self.error("an escape that isn't a character"))?,
                            );
                        }
                        _ => return Err(self.error("an unknown escape")),
                    }
                }
                byte if byte < 0x20 => return Err(self.error("a control character in text")),
                _ => {
                    // One whole character: the reader only ever stops on a
                    // character's first byte.
                    let Some(c) = self
                        .text
                        .get(self.at..)
                        .and_then(|rest| rest.chars().next())
                    else {
                        return Err(self.error("text that isn't UTF-8"));
                    };
                    out.push(c);
                    self.at += c.len_utf8();
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self
            .bytes
            .get(self.at..self.at + 4)
            .and_then(|d| std::str::from_utf8(d).ok())
            .and_then(|d| u32::from_str_radix(d, 16).ok())
            .ok_or_else(|| self.error("a bad `\\u` escape"))?;
        self.at += 4;
        Ok(digits)
    }
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn what_the_writer_writes_reads_back() {
        let mut value = Value::object();
        value.set("name", "caf\u{e9} \"x\"\n");
        value.set("count", -12i64);
        value.set(
            "list",
            vec![Value::Bool(true), Value::Null, Value::object()],
        );
        let text = value.to_document();
        assert_eq!(parse(&text), Ok(value));
        assert_eq!(
            parse("\"\\u00e9\\ud83d\\ude00\""),
            Ok(Value::Str("é😀".into()))
        );
    }

    #[test]
    fn bad_documents_say_where() {
        assert!(parse("{\"a\": 1.5}").is_err_and(|e| e.contains("whole")));
        assert!(parse("{\"a\": 1,\n\"a\": 2}").is_err_and(|e| e.contains("line 2")));
        assert!(parse("[1] x").is_err());
        assert!(parse(&"[".repeat(100)).is_err_and(|e| e.contains("deep")));
    }
}
