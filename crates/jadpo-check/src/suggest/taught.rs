//! `taught_suggestions.jadpo_record` (area 14, "Teaching a miss"; area
//! 18's record format; chunk 5.5y): the patterns `jadpo suggest found`
//! taught, committed so a teammate's scan finds them too, one line each,
//! sorted, under the header every record has.
//!
//! ```text
//! taught 8c1d0e5a4f3b2a19 Money "Pence held as Int" "(+ delivery subtotal)"
//! ```
//!
//! The hash is the canonical form's; the suggestion says what the code
//! should use; the reason and the canonical form are quoted, the canonical
//! form confirming a match as it does for a scan's own signals. The
//! snippet's text stays out of the record.

use super::Taught;
use super::record::{quote, unquote};

/// The record's file name, at the project root.
pub const FILE: &str = "taught_suggestions.jadpo_record";

/// What the header says the record is generated from.
const FROM: &str = "the patterns this project taught `jadpo suggest`";

fn is_hash(text: &str) -> bool {
    text.len() == 16
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The record's lines, sorted and one per hash and suggestion, under its
/// header; a later entry for the same pair replaces an earlier one.
pub fn render(entries: &[Taught]) -> String {
    let mut kept: Vec<&Taught> = Vec::new();
    for entry in entries.iter().rev() {
        if !kept
            .iter()
            .any(|k| k.hash == entry.hash && k.suggestion == entry.suggestion)
        {
            kept.push(entry);
        }
    }
    kept.sort_by(|a, b| (&a.hash, &a.suggestion).cmp(&(&b.hash, &b.suggestion)));
    let mut out = jadpo_db::record::header(&jadpo_db::record::TAUGHT_SUGGESTIONS, FROM);
    if !kept.is_empty() {
        out.push('\n');
    }
    for entry in kept {
        out.push_str(&format!(
            "taught {} {} {} {}\n",
            entry.hash,
            entry.suggestion,
            quote(&entry.reason),
            quote(&entry.canonical)
        ));
    }
    out
}

/// The text of the quoted string `rest` starts with, and what follows it.
fn quoted(rest: &str) -> Option<(String, &str)> {
    if !rest.starts_with('"') {
        return None;
    }
    let mut escaped = false;
    for (i, c) in rest.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            let (literal, after) = rest.split_at(i + 1);
            return Some((unquote(literal)?, after));
        }
    }
    None
}

/// The lines of a record, or what is wrong with it and on which line
/// (from 0).
pub fn parse(text: &str) -> Result<Vec<Taught>, (usize, String)> {
    let kind = &jadpo_db::record::TAUGHT_SUGGESTIONS;
    jadpo_db::record::read_header(&jadpo_db::record::normalise(text), kind, FROM)
        .map_err(|problem| (problem.line(), problem.describe(kind)))?;
    let lines = text.lines().enumerate().skip(2);
    let mut entries = Vec::new();
    for (at, line) in lines {
        if line.is_empty() {
            continue;
        }
        let wrong = || (at, "this line is not a taught pattern".to_string());
        let rest = line.strip_prefix("taught ").ok_or_else(wrong)?;
        let (hash, rest) = rest.split_once(' ').ok_or_else(wrong)?;
        let (suggestion, rest) = rest.split_once(' ').ok_or_else(wrong)?;
        let (reason, rest) = quoted(rest).ok_or_else(wrong)?;
        let rest = rest.strip_prefix(' ').ok_or_else(wrong)?;
        let (canonical, rest) = quoted(rest).ok_or_else(wrong)?;
        if !rest.is_empty() || !is_hash(hash) || suggestion.is_empty() || canonical.is_empty() {
            return Err(wrong());
        }
        if super::hash(&canonical) != hash {
            return Err((at, "the hash is not the canonical form's".into()));
        }
        entries.push(Taught {
            hash: hash.to_string(),
            canonical,
            suggestion: suggestion.to_string(),
            reason,
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taught(canonical: &str, suggestion: &str, reason: &str) -> Taught {
        Taught {
            hash: super::super::hash(canonical),
            canonical: canonical.to_string(),
            suggestion: suggestion.to_string(),
            reason: reason.to_string(),
        }
    }

    #[test]
    fn a_record_reads_back_what_it_wrote() {
        let entries = vec![
            taught("(+ delivery subtotal)", "Money", "Pence \"held\" as Int"),
            taught("(== kind \"a \\\" b\")", "Uuid", ""),
            taught("(+ delivery subtotal)", "Money", "Later reason wins"),
        ];
        let text = render(&entries);
        assert!(text.starts_with(HEADER));
        let back = parse(&text).expect("reads");
        assert_eq!(back.len(), 2);
        assert!(back.iter().any(|t| t.reason == "Later reason wins"));
        assert!(back.iter().any(|t| t.canonical == entries[1].canonical));
    }

    #[test]
    fn a_hand_edit_is_refused_by_line() {
        let text = format!("{HEADER}\n{KIND}\n\ntaught 0000000000000000 Money \"x\" \"(+ a b)\"\n");
        assert_eq!(parse(&text).expect_err("refused").0, 3);
        assert!(parse("taught").is_err());
    }
}
