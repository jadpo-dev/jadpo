//! The formatter's properties over random edits of every example
//! (docs/compiler-runtime.md §2.2, §2.7): whatever parses cleanly formats
//! to text that parses to the same tokens and comments and formats to
//! itself. The edits are the parser fuzz test's, seeded, so a failure
//! repeats; the failing input is printed.

use crate::syntax_examples as examples;
use crate::syntax_mutate as mutate;

use jadpo_fmt::{format_file, format_snippet};
use jadpo_syntax::{parse_file, parse_snippet};
use mutate::{Rng, mutate};

/// Formats `text` as a file and as a snippet wherever it parses cleanly;
/// the formatter refuses (`format.unsafe`) only on its own mistake.
fn holds(text: &str) -> Result<(), String> {
    if !parse_file(text).has_errors()
        && let Err(errors) = format_file(text)
    {
        return Err(format!("format_file: {}", errors[0].message));
    }
    if !parse_snippet(text).has_errors()
        && let Err(errors) = format_snippet(text)
    {
        return Err(format!("format_snippet: {}", errors[0].message));
    }
    Ok(())
}

#[test]
fn mutated_examples_format_safely_and_idempotently() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let mut failures = Vec::new();
    for example in examples::all() {
        let mut inputs = vec![example.source.clone()];
        for _ in 0..4 {
            inputs.push(mutate(&example.source, &mut rng));
        }
        for input in inputs {
            if let Err(why) = holds(&input) {
                failures.push(format!("{}: {why}\n{input:?}", example.place));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} input(s) the formatter can't format safely; the first:\n{}",
        failures.len(),
        failures[0]
    );
}

/// The review's two reproducers: a member that starts like a root
/// declaration, and trailing comments in the middle of a statement.
#[test]
fn the_reviews_reproducers_format_safely() {
    for text in [
        "library wording { config {} }\n",
        // Two `?` apart: written together they read as one mistake.
        "entity Book {\n    id: Uuid\n    at: Instant? ?\n}\n",
        // A comment on its own line between `.` and the field's name.
        "function f(a: A) -> B {\n    return g([a.owner, st.\n//\n b])\n}\n",
        "type Quantity = Int {}\n\naction reserve_stock(quantity: Quantity) -> Unit { // TODO\n}\n",
        "type Count = Int {}\n\nfunction f(x: Count) -> Count {\n    return Count(x + // the base\n    1) // plus one\n}\n",
    ] {
        assert_eq!(holds(text), Ok(()), "{text}");
    }
    assert_eq!(
        format_file("library wording { config {} }\n").expect("formats"),
        "library wording {\n    config {}\n}\n"
    );
    assert_eq!(
        format_file("type Count = Int {}\n\nfunction f(x: Count) -> Count {\n    return x + // the base\n        1 // plus one\n}\n")
            .expect("formats"),
        "type Count = Int {}\n\nfunction f(x: Count) -> Count {\n    return x +  // the base\n    1  // plus one\n}\n"
    );
}

/// A comment alone inside empty brackets, which a mutation of an example
/// found: it goes on its own line, and formatting again keeps it there.
#[test]
fn a_comment_alone_in_empty_brackets_formats_safely() {
    let text = "action f(\n/// why\n) -> Unit {\n    return\n}\n";
    assert_eq!(holds(text), Ok(()), "{text}");
    assert_eq!(
        format_file("action f(\n// none yet\n) -> Unit {}\n").expect("formats"),
        "action f(\n    // none yet\n) -> Unit {}\n"
    );
}
