//! A mailbox's canonical text (area 19, "People and contact", chunk 5.5l):
//! an `EmailWithName` written as RFC 5322 writes a mailbox, with RFC 6532's
//! UTF-8 in the name. Only the canonical form is read, as every type's
//! `text.parse` reads only its canonical text: the name bare where every
//! word of it is an atom, quoted only where it has to be, one space before
//! the address in angle brackets, and the address alone when there is no
//! name.

use jadpo_std::standard::StandardType;

use crate::{Fault, Record, Value};

/// Whether `c` may stand in an atom (RFC 5322 `atext`, with any character
/// beyond ASCII, as RFC 6532 allows).
fn atom_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-/=?^_`{|}~".contains(c) || !c.is_ascii()
}

/// Whether `name` is written bare: words of atoms with one space between.
fn bare(name: &str) -> bool {
    name.split(' ')
        .all(|word| !word.is_empty() && word.chars().all(atom_char))
}

/// A mailbox's canonical text.
pub fn mailbox_text(record: &Record) -> Result<String, Fault> {
    let Some(Value::Text(address)) = record.get("address") else {
        return Err(Fault::Shape);
    };
    Ok(match record.get("name") {
        Some(Value::Text(name)) if bare(name) => format!("{name} <{address}>"),
        Some(Value::Text(name)) => {
            let quoted = name.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{quoted}\" <{address}>")
        }
        _ => address.to_string(),
    })
}

/// A quoted name's text, its `\` escapes removed; `None` for text that
/// isn't one quoted string.
fn unquoted(raw: &str) -> Option<String> {
    let inner = raw.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(chars.next()?),
            '"' => return None,
            other => out.push(other),
        }
    }
    Some(out)
}

/// An `EmailWithName` read from its canonical text.
pub fn read_mailbox(entry: &StandardType, raw: &str) -> Option<Value> {
    let (name, address) = match raw.strip_suffix('>') {
        Some(head) => {
            let (name, address) = head.rsplit_once(" <")?;
            let name = match unquoted(name) {
                // Quoted only where it has to be.
                Some(name) if !bare(&name) => name,
                Some(_) => return None,
                None if bare(name) => name.to_string(),
                None => return None,
            };
            (Some(name), address)
        }
        None => (None, raw),
    };
    if !jadpo_std::is_email(address) {
        return None;
    }
    if let Some(name) = &name
        && jadpo_std::standard::Part::Line
            .rules()
            .check_text(name)
            .is_err()
    {
        return None;
    }
    Some(Value::record(
        entry.name,
        vec![
            (
                "name".into(),
                name.as_deref().map_or(Value::None, Value::text),
            ),
            ("address".into(), Value::text(address)),
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_quoted_only_where_it_has_to_be() {
        let entry = jadpo_std::standard_type("EmailWithName").unwrap();
        for text in [
            "Ada Lovelace <ada@example.com>",
            "\"Lovelace, Ada\" <ada@example.com>",
            "\"Ada \\\"Countess\\\" Lovelace\" <ada@example.com>",
            "José María <jose@example.com>",
            "ada@example.com",
        ] {
            let Some(Value::Record(record)) = read_mailbox(entry, text) else {
                panic!("`{text}` reads");
            };
            assert_eq!(mailbox_text(&record).as_deref(), Ok(text));
        }
        for text in [
            "\"Ada\" <ada@example.com>",
            "Ada  Lovelace <ada@example.com>",
            "Ada, Lovelace <ada@example.com>",
            "<ada@example.com>",
            "Ada <not an address>",
        ] {
            assert_eq!(read_mailbox(entry, text), None, "`{text}`");
        }
    }
}
