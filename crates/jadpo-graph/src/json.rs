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
