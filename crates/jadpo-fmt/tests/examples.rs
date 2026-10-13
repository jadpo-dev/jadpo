//! Formatting every example in the documentation and `examples/` (the gate's
//! "format idempotence over every example", docs/compiler-runtime.md §2.7):
//! formatting twice changes nothing, the result parses to the same tree,
//! and every comment survives.

#[path = "../../jadpo-syntax/tests/support/examples.rs"]
mod examples;

use jadpo_fmt::format_snippet;
use jadpo_syntax::SyntaxKind::{COMMA, COMMENT, DOC_COMMENT, R_BRACK, R_PAREN};
use jadpo_syntax::{SyntaxNode, parse_snippet};

/// The significant tokens a tree is made of, without trailing commas (the
/// formatter removes them), as (kind, text).
fn shape(node: &SyntaxNode) -> Vec<String> {
    let tokens: Vec<_> = node
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
        .collect();
    let mut out = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        let next = tokens.get(i + 1).map(|t| t.kind());
        if token.kind() == COMMA && matches!(next, Some(R_PAREN | R_BRACK)) {
            continue;
        }
        out.push(format!("{:?} {}", token.kind(), token.text()));
    }
    // Node kinds too, in order, so a token can't move to another node.
    out.extend(node.descendants().map(|n| format!("{:?}", n.kind())));
    out
}

fn comments(node: &SyntaxNode) -> Vec<String> {
    node.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| matches!(t.kind(), COMMENT | DOC_COMMENT))
        .map(|t| t.text().trim_end().to_string())
        .collect()
}

#[test]
fn every_example_formats_idempotently_to_the_same_tree() {
    let mut failures = Vec::new();
    let mut formatted = 0;
    for example in examples::all() {
        let before = parse_snippet(&example.source);
        if before.has_errors() {
            continue;
        }
        let Ok(once) = format_snippet(&example.source) else {
            continue;
        };
        formatted += 1;
        let after = parse_snippet(&once);
        if after.has_errors() {
            failures.push(format!(
                "{}: the formatted example doesn't parse: {}\n{once}",
                example.place,
                after.errors()[0].message
            ));
            continue;
        }
        let twice = format_snippet(&once).unwrap_or_default();
        if twice != once {
            failures.push(format!(
                "{}: formatting twice changes it\n--- once\n{once}--- twice\n{twice}",
                example.place
            ));
        }
        if shape(&before.syntax()) != shape(&after.syntax()) {
            failures.push(format!(
                "{}: the formatted example parses to a different tree\n{once}",
                example.place
            ));
        }
        if comments(&before.syntax()) != comments(&after.syntax()) {
            failures.push(format!("{}: comments changed\n{once}", example.place));
        }
    }
    assert!(
        failures.is_empty(),
        "{} examples fail:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(formatted > 200, "only {formatted} examples were formatted");
}
