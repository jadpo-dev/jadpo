//! The parser never panics and never loses text (docs/compiler-runtime.md
//! §2.2): every example, mutated at random many times, still parses into a
//! tree that holds its text exactly. The mutations are seeded, so a failure
//! repeats; the failing input is printed.
//!
//! The mutations insert habits from other languages too, and every parse
//! keeps the rule behind them: a syntax
//! error never brings a second on its line, and when every error is a clean
//! habit (`Parse::checkable`), applying the edits gives, round by round, a
//! file with none.

use crate::syntax_examples as examples;

use std::panic::{AssertUnwindSafe, catch_unwind};

use jadpo_syntax::{Parse, parse_file, parse_snippet, parse_template};

use crate::syntax_mutate as mutate;

use mutate::{Rng, mutate};

fn check(text: &str) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        for parse in [parse_file(text), parse_snippet(text), parse_template(text)] {
            assert_eq!(parse.syntax().to_string(), text, "the tree lost text");
            one_error_a_line(text, &parse);
        }
        clean_edits_converge(text);
    }));
    if result.is_err() {
        panic!("the parser failed on this input:\n{text:?}");
    }
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
/// their edits are made, a round at a time (a line shows one at a time).
fn clean_edits_converge(text: &str) {
    let mut current = text.to_string();
    for _ in 0..12 {
        let parse = parse_file(&current);
        if !parse.checkable() {
            assert_eq!(
                current,
                text,
                "an edit left a file that isn't checkable:\n{current:?}\n{:#?}",
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
            "clean errors with no edit:\n{current:?}\n{:#?}",
            parse.errors()
        );
        current = next;
    }
    panic!(
        "clean edits didn't converge:\n{current:?}\n{:#?}",
        parse_file(&current).errors()
    );
}

#[test]
fn mutated_examples_never_break_the_parser() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let examples = examples::all();
    assert!(examples.len() > 500);
    for example in &examples {
        check(&example.source);
        for _ in 0..6 {
            check(&mutate(&example.source, &mut rng));
        }
    }
}

#[test]
fn hostile_input_is_survived() {
    let mut inputs = vec![
        "(".repeat(20_000),
        "{".repeat(20_000),
        "[".repeat(20_000),
        "<".repeat(5_000),
        format!(
            "function f() -> T {{\n    return {}\n}}\n",
            "(".repeat(5_000)
        ),
        format!("type A = Object {{\n{}\n}}\n", "    x: List<".repeat(2_000)),
        "\"".repeat(1_000),
        "}}}}{{{{".repeat(1_000),
        "/{a}/{b}".repeat(500),
        String::from_utf8_lossy(&(0u8..=255).collect::<Vec<u8>>()).into_owned(),
        "\u{feff}type A = Int {}\r\n".to_string(),
        // An `if` with no condition before a closing brace.
        "type A = Int {}\nif }\n".to_string(),
        String::new(),
    ];
    inputs.push("not not not not ".repeat(5_000));
    // A long `else if` chain and deeply nested template tags (Phase 1
    // review, finding 18).
    inputs.push(format!(
        "function f(n: Count) -> Count {{\n    if (n == 0) {{\n        return n\n    }}{}\n    return n\n}}\n",
        " else if (n == 1) {\n        return n\n    }".repeat(5_000)
    ));
    inputs.push("{{ if a }}".repeat(5_000));
    inputs.push("{{ for a in b }}".repeat(5_000));
    inputs.push("- ".repeat(20_000));
    // Habits written tight against their neighbours: the edits keep the
    // words apart.
    inputs.push("function f(a: Flag, b: Flag): Flag {\n    return !a&&b||a===b;\n}\n".to_string());
    inputs.push("function f(a: Flag)->Flag {\n    return a AND(b) OR NOT(a)\n}\n".to_string());
    // A `;` before what can't start a line isn't a clean split.
    inputs.push(
        "type A = Output {\n    flour: Grams;?\n    total: Grams;, ;: ;= ;+ ;)\n}\n".to_string(),
    );
    // `=` before a `/` isn't a clean `:`: after `:` the lexer reads
    // `/_length` as a route path, so the edit changes the rest of the line.
    inputs.push(
        "/// How a baker signs in.{\ntype Handle = Text {\n    mine =/_length: 3\n    max_length: 40\n}\n"
            .to_string(),
    );
    inputs.push("function f() -> Unit {\n    g(mine =/_length: 3)\n}\n".to_string());
    // A `#` comment tight after a `/`: `//` there would be `///`, a doc
    // comment, so the edit keeps them apart.
    inputs.push("routes Outlets {\n    prefix: /#outlets\n}\n".to_string());
    inputs.push("function f(a: Count) -> Count {\n    return a /# half\n}\n".to_string());
    for input in inputs {
        check(&input);
    }
}
