//! The identity registry's file, `database/identities.json` (area 18,
//! "Stable identity"): one JSON object, each line a thing renamed or moved
//! since it was born, `"kind name-now": "born name"`, sorted, one entry per
//! line (area 5's layout).

use std::collections::BTreeMap;

/// The registry's path in a project.
pub const IDENTITIES: &str = "database/identities.json";

/// The registry's lines from its file's text, or why they can't be read: it
/// is one JSON object whose values are all text.
pub fn parse_identities(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    let mut out = BTreeMap::new();
    reader.expect(b'{')?;
    if !reader.eat(b'}') {
        loop {
            let now = reader.string()?;
            reader.expect(b':')?;
            let born = reader.string()?;
            if out.insert(now.clone(), born).is_some() {
                return Err(format!("`{now}` is listed twice"));
            }
            if reader.eat(b',') {
                continue;
            }
            reader.expect(b'}')?;
            break;
        }
    }
    reader.space();
    if reader.at != reader.bytes.len() {
        return Err(reader.error("text after the object"));
    }
    Ok(out)
}

/// The registry's file for `lines`: sorted, one entry per line, two spaces
/// in, ending in a newline.
pub fn write_identities(lines: &BTreeMap<String, String>) -> String {
    if lines.is_empty() {
        return "{}\n".to_string();
    }
    let entries: Vec<String> = lines
        .iter()
        .map(|(now, born)| format!("  {}: {}", quoted(now), quoted(born)))
        .collect();
    format!("{{\n{}\n}}\n", entries.join(",\n"))
}

fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

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

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.eat(byte) {
            Ok(())
        } else {
            Err(self.error(&format!("expected `{}`", byte as char)))
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(&byte) = self.bytes.get(self.at) else {
                return Err(self.error("unfinished text"));
            };
            self.at += 1;
            match byte {
                b'"' => break,
                b'\\' => {
                    let Some(&escaped) = self.bytes.get(self.at) else {
                        return Err(self.error("unfinished text"));
                    };
                    self.at += 1;
                    match escaped {
                        b'"' | b'\\' | b'/' => out.push(escaped),
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'u' => {
                            let hex = self
                                .bytes
                                .get(self.at..self.at + 4)
                                .and_then(|hex| std::str::from_utf8(hex).ok())
                                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                                .and_then(char::from_u32)
                                .ok_or_else(|| self.error("a bad `\\u` escape"))?;
                            self.at += 4;
                            let mut buffer = [0; 4];
                            out.extend_from_slice(hex.encode_utf8(&mut buffer).as_bytes());
                        }
                        _ => return Err(self.error("a bad escape")),
                    }
                }
                byte => out.push(byte),
            }
        }
        String::from_utf8(out).map_err(|_| self.error("text that isn't UTF-8"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_reads_back_what_it_writes() {
        let mut lines = BTreeMap::new();
        lines.insert("type Address".to_string(), "billing.Address".to_string());
        lines.insert("entity Task".to_string(), "Todo".to_string());
        let text = write_identities(&lines);
        assert_eq!(
            text,
            "{\n  \"entity Task\": \"Todo\",\n  \"type Address\": \"billing.Address\"\n}\n"
        );
        assert_eq!(parse_identities(&text), Ok(lines));
        assert_eq!(parse_identities("{}\n"), Ok(BTreeMap::new()));
        assert_eq!(write_identities(&BTreeMap::new()), "{}\n");
    }

    #[test]
    fn a_registry_that_isnt_an_object_of_text_is_refused() {
        assert!(parse_identities("[]").is_err());
        assert!(parse_identities("{\"a\": 1}").is_err());
        assert!(parse_identities("{\"a\": \"b\", \"a\": \"c\"}").is_err());
        assert!(parse_identities("{} x").is_err());
    }
}
