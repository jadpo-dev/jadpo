//! `ignored_suggestions.jadpo_record` (area 14, "Ignoring a suggestion";
//! area 18's record format): the suggestions a team keeps quiet, one line
//! each, sorted, under the header every record has.
//!
//! ```text
//! ignore 3f9a1c0e2b7d4a51 conversion Money "Basis points, not money"
//! ```
//!
//! The hash is the snippet's canonical form's, so every variant of it stays
//! quiet; the signal and what it suggests say which finding; the reason is
//! quoted, with `\"` and `\\` inside.

use super::Signal;

/// The record's file name, at the project root.
pub const FILE: &str = "ignored_suggestions.jadpo_record";

/// What the header says the record is generated from.
const FROM: &str = "the suggestions this project ignores";

/// One ignored suggestion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ignore {
    pub hash: String,
    pub signal: Signal,
    /// The standard type or operation it suggested.
    pub suggestion: String,
    pub reason: String,
}

impl Ignore {
    /// Whether it keeps a finding with this hash, signal and suggestion
    /// quiet.
    pub fn covers(&self, hash: &str, signal: Signal, suggestion: &str) -> bool {
        self.hash == hash && self.signal == signal && self.suggestion == suggestion
    }
}

/// The record's lines, sorted, under its header.
pub fn render(entries: &[Ignore]) -> String {
    let mut sorted: Vec<&Ignore> = entries.iter().collect();
    sorted.sort_by(|a, b| {
        (&a.hash, a.signal.word(), &a.suggestion).cmp(&(&b.hash, b.signal.word(), &b.suggestion))
    });
    sorted.dedup_by(|a, b| a.covers(&b.hash, b.signal, &b.suggestion));
    let mut out = jadpo_db::record::header(&jadpo_db::record::IGNORED_SUGGESTIONS, FROM);
    if !sorted.is_empty() {
        out.push('\n');
    }
    for entry in sorted {
        out.push_str(&format!(
            "ignore {} {} {} {}\n",
            entry.hash,
            entry.signal.word(),
            entry.suggestion,
            quote(&entry.reason)
        ));
    }
    out
}

/// `text` quoted, with `\"` and `\\` inside and a line break as `\n`.
pub(crate) fn quote(text: &str) -> String {
    let mut out = String::from('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// The lines of a record, or what is wrong with it and on which line
/// (from 0).
pub fn parse(text: &str) -> Result<Vec<Ignore>, (usize, String)> {
    let kind = &jadpo_db::record::IGNORED_SUGGESTIONS;
    jadpo_db::record::read_header(&jadpo_db::record::normalise(text), kind, FROM)
        .map_err(|problem| (problem.line(), problem.describe(kind)))?;
    let lines = text.lines().enumerate().skip(2);
    let mut entries = Vec::new();
    for (at, line) in lines {
        if line.is_empty() {
            continue;
        }
        let wrong = || (at, "this line is not an ignored suggestion".to_string());
        let rest = line.strip_prefix("ignore ").ok_or_else(wrong)?;
        let mut words = rest.splitn(4, ' ');
        let (Some(hash), Some(signal), Some(suggestion), Some(reason)) =
            (words.next(), words.next(), words.next(), words.next())
        else {
            return Err(wrong());
        };
        let is_hash = hash.len() == 16
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        let Some(signal) = Signal::from_word(signal) else {
            return Err((at, format!("`{signal}` isn't a signal")));
        };
        let reason = unquote(reason).ok_or_else(wrong)?;
        if !is_hash || suggestion.is_empty() {
            return Err(wrong());
        }
        entries.push(Ignore {
            hash: hash.to_string(),
            signal,
            suggestion: suggestion.to_string(),
            reason,
        });
    }
    Ok(entries)
}

/// The text inside `"…"`, its escapes undone.
pub(crate) fn unquote(text: &str) -> Option<String> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                other => out.push(other),
            },
            '"' => return None,
            other => out.push(other),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_reads_back_what_it_wrote() {
        let entries = vec![
            Ignore {
                hash: "3f9a1c0e2b7d4a51".into(),
                signal: Signal::Conversion,
                suggestion: "Money".into(),
                reason: "Basis points, \"not\" money \\ really".into(),
            },
            Ignore {
                hash: "0000000000000001".into(),
                signal: Signal::Name,
                suggestion: "Duration".into(),
                reason: String::new(),
            },
        ];
        let text = render(&entries);
        assert!(text.starts_with(&jadpo_db::record::header(&jadpo_db::record::IGNORED_SUGGESTIONS, FROM)));
        let back = parse(&text).expect("reads");
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].suggestion, "Duration");
        assert_eq!(back[1].reason, entries[0].reason);
    }

    #[test]
    fn a_hand_edit_is_refused_by_line() {
        let text = format!(
            "{}\nignore nothash conversion Money \"x\"\n",
            jadpo_db::record::header(&jadpo_db::record::IGNORED_SUGGESTIONS, FROM)
        );
        assert_eq!(parse(&text).expect_err("refused").0, 3);
        assert!(parse("ignore").is_err());
    }
}
