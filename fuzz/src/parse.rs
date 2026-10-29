//! Source text (docs/compiler-runtime.md §2.2, "Lexer and parser" and
//! "Formatter"): the parser never panics and never loses text, keeps one
//! error a line with its edits inside the text, and a file whose errors
//! are all clean habits becomes one with none when their edits are made;
//! whatever parses cleanly formats without `format.unsafe`. The
//! properties are the mutation fuzz tests' (`crates/jadpo-syntax/tests/
//! fuzz.rs`, `crates/jadpo-fmt/tests/fuzz.rs`), here driven by coverage.

use jadpo_syntax::{Parse, parse_file, parse_snippet, parse_template};

pub fn run(text: &str) {
    for (what, parse) in [
        ("file", parse_file(text)),
        ("snippet", parse_snippet(text)),
        ("template", parse_template(text)),
    ] {
        assert_eq!(
            parse.syntax().to_string(),
            text,
            "the {what} tree lost text"
        );
        one_error_a_line(text, &parse);
    }
    clean_edits_converge(text);
    formats(text);
}

/// Errors and their edits stay in the text, and no line has two errors.
fn one_error_a_line(text: &str, parse: &Parse) {
    let mut lines = std::collections::HashSet::new();
    for error in parse.errors() {
        assert!(
            error.start <= error.end && error.end as usize <= text.len(),
            "an error points outside the text: {error:?}"
        );
        for edit in &error.edits {
            assert!(
                edit.start <= edit.end
                    && edit.end as usize <= text.len()
                    && text.is_char_boundary(edit.start as usize)
                    && text.is_char_boundary(edit.end as usize),
                "an edit points outside the text: {error:?}"
            );
        }
        let line = text[..error.start as usize].matches('\n').count();
        assert!(
            lines.insert(line),
            "a second error on line {}: {error:?}",
            line + 1
        );
    }
}

/// A file whose errors are all clean habits becomes a file with none when
/// their edits are made, a round at a time.
fn clean_edits_converge(text: &str) {
    let mut current = text.to_string();
    for _ in 0..12 {
        let parse = parse_file(&current);
        if !parse.checkable() {
            assert_eq!(
                current,
                text,
                "an edit left a file that isn't checkable: {:#?}",
                parse.errors()
            );
            return;
        }
        if parse.errors().is_empty() {
            return;
        }
        let mut edits: Vec<_> = parse
            .errors()
            .iter()
            .flat_map(|e| e.edits.clone())
            .collect();
        edits.sort_by_key(|edit| (edit.start, edit.end));
        let mut next = current.clone();
        let mut last_start = u32::MAX;
        for edit in edits.iter().rev() {
            if edit.end > last_start {
                continue;
            }
            next.replace_range(edit.start as usize..edit.end as usize, &edit.text);
            last_start = edit.start;
        }
        assert_ne!(
            next,
            current,
            "clean errors with no edit: {:#?}",
            parse.errors()
        );
        current = next;
    }
    panic!("clean edits didn't converge: {current:?}");
}

/// What parses cleanly formats: `format.unsafe` is the formatter's own
/// mistake.
fn formats(text: &str) {
    if !parse_file(text).has_errors()
        && let Err(errors) = jadpo_fmt::format_file(text)
    {
        panic!("format_file: {}", errors[0].message);
    }
    if !parse_snippet(text).has_errors()
        && let Err(errors) = jadpo_fmt::format_snippet(text)
    {
        panic!("format_snippet: {}", errors[0].message);
    }
}
