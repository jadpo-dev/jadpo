//! The parser never panics and never loses text (docs/compiler-runtime.md
//! §2.2): every example, mutated at random many times, still parses into a
//! tree that holds its text exactly. The mutations are seeded, so a failure
//! repeats; the failing input is printed.

#[path = "support/examples.rs"]
mod examples;

use std::panic::{AssertUnwindSafe, catch_unwind};

use jadpo_syntax::{parse_file, parse_snippet, parse_template};

#[path = "support/mutate.rs"]
mod mutate;

use mutate::{Rng, mutate};

fn check(text: &str) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        for parse in [parse_file(text), parse_snippet(text), parse_template(text)] {
            assert_eq!(parse.syntax().to_string(), text, "the tree lost text");
            for error in parse.errors() {
                assert!(
                    error.start <= error.end && error.end as usize <= text.len(),
                    "an error points outside the text: {error:?}"
                );
            }
        }
    }));
    if result.is_err() {
        panic!("the parser failed on this input:\n{text:?}");
    }
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
    for input in inputs {
        check(&input);
    }
}
