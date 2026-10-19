//! The identity registry, `database/identities.jadpo_record` (area 18,
//! "Stable identity"; the `identities` record): one line for each thing
//! renamed or moved since it was born, its kind, its born name, `->` and
//! its name now, sorted by born name, then kind.
//!
//! ```text
//! entity Todo -> Task
//! field Todo.priority -> importance
//! type billing.Address -> Address
//! ```
//!
//! A field, variant or rule follows what it belongs to, so its line gives
//! only its own new name: renaming `Task` again never touches the field's
//! line. In memory the registry is keyed the other way, `kind name-now`
//! to the born name, which is how the compiler looks names up.
//!
//! Before chunk 7.0 the registry was `database/identities.json`, one JSON
//! object of `"kind name-now": "born name"`; [`parse_legacy_identities`]
//! still reads it, so `jadpo records update` can rewrite it in the record
//! format.

use std::collections::BTreeMap;

use crate::record;

/// The registry's path in a project.
pub const IDENTITIES: &str = "database/identities.jadpo_record";

/// The registry's path before chunk 7.0.
pub const LEGACY_IDENTITIES: &str = "database/identities.json";

/// What the registry's header says it is generated from.
const FROM: &str = "renames and moves";

/// The kinds whose name is their owner's and their own (`Todo.priority`),
/// so a line gives only their own new name.
const MEMBERS: &[&str] = &["field", "variant", "rule"];

/// The born name a name now resolves to through the registry's lines for
/// things other than members: `Todo` to `Task` once `entity Todo -> Task`
/// is a line, a member of something renamed following it.
fn resolve(owners: &BTreeMap<&str, &str>, born: &str) -> String {
    if let Some(now) = owners.get(born) {
        return (*now).to_string();
    }
    match born.rsplit_once('.') {
        Some((owner, own)) => format!("{}.{own}", resolve(owners, owner)),
        None => born.to_string(),
    }
}

/// The registry's lines from its file's text, keyed `kind name-now` to the
/// born name, or the line that can't be read (from 0) and why.
pub fn parse_identities(text: &str) -> Result<BTreeMap<String, String>, (usize, String)> {
    let text = record::normalise(text);
    record::read_header(&text, &record::IDENTITIES, FROM)
        .map_err(|problem| (problem.line(), problem.describe(&record::IDENTITIES)))?;
    let entries = record::read(&text, &record::IDENTITIES)?;
    let mut lines: Vec<(usize, &str, &str, &str)> = Vec::new();
    for (at, line) in text.lines().enumerate().skip(2) {
        if line.is_empty() {
            continue;
        }
        let words: Vec<&str> = line.split(' ').collect();
        let [kind, born, "->", now] = words.as_slice() else {
            return Err((at, "a line is `<kind> <born name> -> <name now>`".into()));
        };
        let named = |name: &str| {
            !name.is_empty()
                && name.split('.').all(|part| {
                    !part.is_empty()
                        && part
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_')
                })
        };
        let lower = kind.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
        if kind.is_empty() || !lower || !named(born) || !named(now) {
            return Err((at, "a line is `<kind> <born name> -> <name now>`".into()));
        }
        lines.push((at, kind, born, now));
    }
    debug_assert_eq!(lines.len(), entries.len());
    let owners: BTreeMap<&str, &str> = lines
        .iter()
        .filter(|(_, kind, ..)| !MEMBERS.contains(kind))
        .map(|(_, _, born, now)| (*born, *now))
        .collect();
    let mut out = BTreeMap::new();
    let mut seen: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for (at, kind, born, now) in &lines {
        if let Some(first) = seen.insert((kind, born), *at) {
            return Err((
                *at,
                format!(
                    "`{kind} {born}` is on lines {} and {}: two renames of one thing, as a merge of two branches can leave",
                    first + 1,
                    at + 1
                ),
            ));
        }
        let full = if MEMBERS.contains(kind) {
            let own = now.split('.').count();
            let parts: Vec<&str> = born.split('.').collect();
            if own >= parts.len() {
                return Err((*at, format!("`{now}` has more parts than `{born}`")));
            }
            format!("{}.{now}", resolve(&owners, &parts[..parts.len() - own].join(".")))
        } else {
            (*now).to_string()
        };
        if out.insert(format!("{kind} {full}"), (*born).to_string()).is_some() {
            return Err((*at, format!("two lines name `{kind} {full}`")));
        }
    }
    Ok(out)
}

/// The registry's file for `lines`, keyed `kind name-now` to the born
/// name: one line each, sorted by born name, then kind.
pub fn write_identities(lines: &BTreeMap<String, String>) -> String {
    let split: Vec<(&str, &str, &str)> = lines
        .iter()
        .filter_map(|(key, born)| {
            let (kind, now) = key.split_once(' ')?;
            Some((kind, now, born.as_str()))
        })
        .collect();
    let owners: BTreeMap<&str, &str> = split
        .iter()
        .filter(|(kind, ..)| !MEMBERS.contains(kind))
        .map(|(_, now, born)| (*born, *now))
        .collect();
    let mut entries: Vec<(&str, &str, String)> = split
        .iter()
        .map(|(kind, now, born)| {
            let shown = if MEMBERS.contains(kind) {
                own_name(&owners, now, born)
            } else {
                (*now).to_string()
            };
            (*born, *kind, shown)
        })
        .collect();
    entries.sort();
    let entries: Vec<record::Entry> = entries
        .into_iter()
        .map(|(born, kind, now)| record::Entry::new(format!("{kind} {born} -> {now}")))
        .collect();
    record::write(&record::IDENTITIES, FROM, &entries)
}

/// A member's own new name: the shortest end of its name now that, put
/// after what its owner is called now, gives that name.
fn own_name(owners: &BTreeMap<&str, &str>, now: &str, born: &str) -> String {
    let now_parts: Vec<&str> = now.split('.').collect();
    let born_parts: Vec<&str> = born.split('.').collect();
    for own in 1..born_parts.len().min(now_parts.len() + 1) {
        let owner = born_parts[..born_parts.len() - own].join(".");
        let tail = now_parts[now_parts.len() - own..].join(".");
        if format!("{}.{tail}", resolve(owners, &owner)) == now {
            return tail;
        }
    }
    now_parts.last().copied().unwrap_or(now).to_string()
}

/// The registry's lines from `database/identities.json`, the file it was
/// before chunk 7.0: one JSON object whose values are all text.
pub fn parse_legacy_identities(text: &str) -> Result<BTreeMap<String, String>, String> {
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

    fn lines(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(now, born)| (now.to_string(), born.to_string()))
            .collect()
    }

    const HEADER: &str = "// Generated by Jadpo from renames and moves; update it with `jadpo rename`, `jadpo domain` or `jadpo fmt --layout`, never by hand.\nrecord identities 1\n";

    #[test]
    fn the_registry_is_written_by_born_name_a_members_line_giving_its_own_name() {
        let renames = lines(&[
            ("entity Task", "Todo"),
            ("field Task.importance", "Todo.priority"),
            ("type Address", "billing.Address"),
        ]);
        let text = write_identities(&renames);
        assert_eq!(
            text,
            format!(
                "{HEADER}\nentity Todo -> Task\nfield Todo.priority -> importance\ntype billing.Address -> Address\n"
            )
        );
        assert_eq!(parse_identities(&text), Ok(renames));
        assert_eq!(write_identities(&BTreeMap::new()), HEADER);
        assert_eq!(parse_identities(HEADER), Ok(BTreeMap::new()));
    }

    #[test]
    fn renaming_the_owner_again_leaves_a_members_line_alone() {
        let once = write_identities(&lines(&[
            ("entity Task", "Todo"),
            ("field Task.importance", "Todo.priority"),
        ]));
        let twice = write_identities(&lines(&[
            ("entity Job", "Todo"),
            ("field Job.importance", "Todo.priority"),
        ]));
        let changed: Vec<(&str, &str)> = once
            .lines()
            .zip(twice.lines())
            .filter(|(a, b)| a != b)
            .collect();
        assert_eq!(changed, vec![("entity Todo -> Task", "entity Todo -> Job")]);
        // A member renamed under an owner that kept its name, and one of
        // a type in a domain.
        let kept = lines(&[
            ("field Todo.importance", "Todo.priority"),
            ("field billing.Address.road", "billing.Address.street"),
        ]);
        let text = write_identities(&kept);
        assert!(text.contains("field Todo.priority -> importance\n"), "{text}");
        assert!(text.contains("field billing.Address.street -> road\n"), "{text}");
        assert_eq!(parse_identities(&text), Ok(kept));
    }

    #[test]
    fn two_renames_of_one_thing_are_refused() {
        let text = format!("{HEADER}\nentity Todo -> Task\nentity Todo -> Job\n");
        assert_eq!(
            parse_identities(&text).map_err(|(at, _)| at),
            Err(4)
        );
        assert!(parse_identities(&format!("{HEADER}\nentity Todo Task\n")).is_err());
        assert!(parse_identities("{}\n").is_err());
    }

    #[test]
    fn the_old_registry_still_reads() {
        assert_eq!(
            parse_legacy_identities("{\n  \"entity Task\": \"Todo\"\n}\n"),
            Ok(lines(&[("entity Task", "Todo")]))
        );
        assert_eq!(parse_legacy_identities("{}\n"), Ok(BTreeMap::new()));
        assert!(parse_legacy_identities("[]").is_err());
        assert!(parse_legacy_identities("{\"a\": 1}").is_err());
        assert!(parse_legacy_identities("{\"a\": \"b\", \"a\": \"c\"}").is_err());
        assert!(parse_legacy_identities("{} x").is_err());
    }
}
