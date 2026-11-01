//! Search on PostgreSQL (area 16, "Search"; chunk B12): what the checker,
//! the plan and the runtime share about an entity's `search:`. The SQL
//! itself is written by `jadpo-sql`'s `search` module.
//!
//! An entity's searchable fields are kept as one `tsvector`, a column
//! PostgreSQL works out from the fields on every write
//! ([`COLUMN`], `GENERATED ALWAYS AS (...) STORED`), each field's words
//! weighted by where it is listed, so its index can never disagree with
//! the record. What a person types, a `SearchText`, is read by PostgreSQL's
//! `websearch_to_tsquery`: words, quoted phrases and `-word`, never raw
//! query syntax. Its one operator besides those, `or`, is made a word here
//! ([`phrase`]), since the page gives search boxes no `or`.

/// The column an entity's searchable words are kept in. A field can't take
/// the name while its entity has `search:`.
pub const COLUMN: &str = "jadpo_search";

/// The most characters a `SearchText` holds (area 2, "Services"): what
/// someone types into a search box.
pub const MOST_CHARACTERS: u64 = 200;

/// The weight the field at `position` (from 0) of `fields:` gives its
/// words: `A` for the first, then `B` and `C`, and `D` for the fourth and
/// every one after, PostgreSQL's four weights. Fields listed first count
/// most when ordering results.
pub fn weight(position: usize) -> char {
    match position {
        0 => 'A',
        1 => 'B',
        2 => 'C',
        _ => 'D',
    }
}

/// `text`, a `SearchText`, as `websearch_to_tsquery` is given it: the
/// same text with each `or` that the function would read as its `OR`
/// operator quoted, so it is a word (a stop word in most languages) and
/// two words either side of it must both match. Inside a quoted phrase
/// nothing changes. Quoting an `or` that wasn't an operator changes
/// nothing either, so the test errs that way.
pub fn phrase(text: &str) -> String {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::with_capacity(text.len() + 4);
    let mut quoted = false;
    let mut before: Option<char> = None;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            quoted = !quoted;
            out.push(c);
            before = Some(c);
            continue;
        }
        let starts = before.is_none_or(|b| !word(b));
        if !quoted && starts && c.eq_ignore_ascii_case(&'o') {
            let mut ahead = chars.clone();
            if let Some(r) = ahead.next().filter(|r| r.eq_ignore_ascii_case(&'r'))
                && ahead.peek().is_none_or(|next| !word(*next) && *next != '-')
            {
                chars.next();
                out.push('"');
                out.push(c);
                out.push(r);
                out.push('"');
                before = Some(r);
                continue;
            }
        }
        out.push(c);
        before = Some(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_or_between_words_becomes_a_word() {
        assert_eq!(phrase("cats or dogs"), "cats \"or\" dogs");
        assert_eq!(phrase("cats OR dogs"), "cats \"OR\" dogs");
        assert_eq!(phrase("cats or, dogs"), "cats \"or\", dogs");
        assert_eq!(phrase("or dogs"), "\"or\" dogs");
        assert_eq!(phrase("cats or"), "cats \"or\"");
        // Inside a phrase, and inside a word, nothing changes.
        assert_eq!(phrase("\"cats or dogs\""), "\"cats or dogs\"");
        assert_eq!(phrase("for oregano orb"), "for oregano orb");
        assert_eq!(phrase("or-ange oré"), "or-ange oré");
        assert_eq!(phrase("-cats \"red"), "-cats \"red");
    }

    #[test]
    fn weights_fall_with_each_field_then_stay() {
        assert_eq!((0..6).map(weight).collect::<String>(), "ABCDDD".to_string());
    }
}
