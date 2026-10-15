//! Every syntax error the area pages document, from the same examples: each
//! mistake gives exactly one error, with its rule, headline and carets in
//! the right place, and any edit it offers fixes the mistake.

use jadpo_syntax::ast::{AstNode, SourceFile};
use jadpo_syntax::{SyntaxError, parse_file};

/// Parses `text`, expects exactly one error, and returns it with the text
/// its carets cover.
fn one_error(text: &str) -> (SyntaxError, String) {
    let parse = parse_file(text);
    let errors = parse.errors();
    assert_eq!(
        errors.len(),
        1,
        "expected one error, got {}:\n{:#?}",
        errors.len(),
        errors
            .iter()
            .map(|e| (&e.message, e.rule.id, e.start))
            .collect::<Vec<_>>()
    );
    let error = errors[0].clone();
    let covered = text[error.start as usize..error.end as usize].to_string();
    (error, covered)
}

/// Applies an error's edits and checks the result parses cleanly.
fn fixed_by_edits(text: &str, error: &SyntaxError) -> String {
    let mut edits = error.edits.clone();
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));
    let mut out = text.to_string();
    for edit in edits {
        out.replace_range(edit.start as usize..edit.end as usize, &edit.text);
    }
    let parse = parse_file(&out);
    assert!(
        parse.errors().is_empty(),
        "the edits leave errors:\n{out}\n{:#?}",
        parse.errors()
    );
    out
}

#[test]
fn unexpected_character() {
    let text = "type Count = Int {\n    min: 0 $\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.unexpected_character");
    assert_eq!(error.message, "`$` isn't part of Jadpo");
    assert_eq!(covered, "$");
    fixed_by_edits(text, &error);
}

#[test]
fn familiar_operators_are_named() {
    let (error, _) = one_error("function f(a: Flag) -> Flag {\n    return a && a\n}\n");
    assert_eq!(error.next, "Use `and` instead of `&&`");
    let (error, _) = one_error("type Count = Int {\n    min: 0;\n}\n");
    assert_eq!(error.next, "Remove `;`");
}

#[test]
fn unterminated_string() {
    let text = "type Label = Text {\n    pattern: \"abc\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.unterminated_string");
    assert_eq!(covered, "\"abc");
    fixed_by_edits(text, &error);
}

#[test]
fn invalid_escape() {
    let text = "type Label = Text {\n    pattern: \"a\\qb\"\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.invalid_escape");
    assert_eq!(covered, "\\q");
    assert_eq!(error.message, "`\\q` isn't an escape Jadpo knows");
    fixed_by_edits(text, &error);
}

#[test]
fn expected_declaration() {
    let text = "Count = Int {}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.expected_declaration");
    assert_eq!(covered, "Count");
    assert_eq!(fixed_by_edits(text, &error), "type Count = Int {}\n");
}

#[test]
fn misspelt_singleton_is_suggested() {
    let (error, _) = one_error("aplication {\n    revocation_delay: 5m\n}\n");
    assert_eq!(error.rule.id, "syntax.expected_declaration");
    assert_eq!(error.next, "Use `application` instead of `aplication`");
}

#[test]
fn expected_name() {
    let text = "type Pair = Object {\n    : Int\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.expected_name");
    assert_eq!(error.message, "This member has no name");
    assert_eq!(covered, ":");
}

#[test]
fn reserved_word_as_a_name() {
    let (error, covered) = one_error("type Pair = Object {\n    match: Count\n}\n");
    assert_eq!(error.rule.id, "syntax.expected_name");
    assert_eq!(error.message, "`match` is a reserved word");
    assert_eq!(covered, "match");
}

#[test]
fn unclosed_parameters() {
    let text =
        "type Count = Int {\n    min: 0\n}\n\nfunction f(x: Count -> Count {\n    return x\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.unexpected_token");
    assert_eq!(error.message, "This parameter list isn't closed");
    assert_eq!(covered, "->");
    assert_eq!(error.label, "expected `)` before `->`");
    assert_eq!(error.next, "Add `)` after `Count`");
    fixed_by_edits(text, &error);
}

#[test]
fn unclosed_block() {
    let text = "type Pair = Object {\n    first: Count\n\ntype Count = Int {\n    min: 0\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.message, "This `{` isn't closed");
    assert_eq!(covered, "{");
}

#[test]
fn type_parent_required() {
    let (error, covered) = one_error("type Dimensions = {\n    width: Size\n}\n");
    assert_eq!(error.rule.id, "syntax.type_parent_required");
    assert_eq!(error.message, "`Dimensions` needs a base type");
    assert_eq!(covered, "{");
}

#[test]
fn type_block_required() {
    let text = "type OrderId = Uuid\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.type_block_required");
    assert_eq!(error.message, "`OrderId` needs a block");
    assert_eq!(covered, "Uuid");
    assert_eq!(fixed_by_edits(text, &error), "type OrderId = Uuid {}\n");
}

#[test]
fn variable_type_required() {
    let text = "function free() -> Price {\n    var price = Price(0.00)\n    return price\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.variable_type_required");
    assert_eq!(error.message, "`price` needs a type");
    assert_eq!(covered, "price");
    assert_eq!(error.next, "Write `var price: Price = 0.00`");
    assert_eq!(
        fixed_by_edits(text, &error),
        "function free() -> Price {\n    var price: Price = 0.00\n    return price\n}\n"
    );
}

#[test]
fn expected_expression() {
    let text = "function total(count: Count) -> Count {\n    var doubled: Count =\n    return doubled\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.expected_expression");
    assert_eq!(error.message, "`doubled` has no value");
    assert_eq!(covered, "=");
}

#[test]
fn member_without_a_value() {
    let (error, covered) = one_error("type Count = Int {\n    min:\n}\n");
    assert_eq!(error.rule.id, "syntax.expected_expression");
    assert_eq!(error.message, "`min:` has no value");
    assert_eq!(covered, ":");
}

#[test]
fn expected_statement() {
    let text = "function first(count: Count) -> Count {\n    while (count > 0) {\n        return count\n    }\n    return count\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.expected_statement");
    assert_eq!(error.message, "Jadpo has no `while` loop");
    assert_eq!(covered, "while");
}

#[test]
fn match_pattern() {
    let text = "function label(outcome: PaymentOutcome) -> Label {\n    return match outcome {\n        PaymentOutcome.pending -> \"pending\"\n        PaymentOutcome.paid { receipt_id } -> \"paid\"\n    }\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.match_pattern");
    assert_eq!(covered, "{ receipt_id }");
    assert_eq!(error.next, "Write `PaymentOutcome.paid(paid)`");
    fixed_by_edits(text, &error);
}

#[test]
fn match_arrow() {
    let text = "function rank(status: Status) -> Rank {\n    return match status {\n        Status.open => 1\n        Status.closed -> 2\n    }\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.match_arrow");
    assert_eq!(covered, "=>");
    fixed_by_edits(text, &error);
}

#[test]
fn named_argument_duplicate() {
    let text = "function morning(date: Day) -> Instant {\n    return temporal.resolve(\n        date: date,\n        at: \"09:00\",\n        zone: Zone.europe_london,\n        zone: Zone.europe_london\n    )\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.named_argument_duplicate");
    assert_eq!(error.message, "`zone:` is given twice");
    assert_eq!(covered, "zone");
    assert_eq!(error.secondary.len(), 1);
}

#[test]
fn positional_after_named() {
    let text = "function morning(date: Day) -> Instant {\n    return temporal.resolve(\n        date: date,\n        \"09:00\",\n        zone: Zone.europe_london\n    )\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.positional_after_named");
    assert_eq!(covered, "\"09:00\"");
}

#[test]
fn unsupported_throw() {
    let (error, covered) =
        one_error("function fail_now(note: Note) -> Note {\n    throw Error(\"boom\")\n}\n");
    assert_eq!(error.rule.id, "syntax.unsupported_throw");
    assert_eq!(covered, "throw");
}

#[test]
fn colon_required() {
    let text = "type ContactEmail = Text {\n    format TextFormat.email\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.colon_required");
    assert_eq!(error.message, "`format` needs a colon before its value");
    assert_eq!(error.next, "Write `format: TextFormat.email`");
    assert_eq!(covered, "format");
    assert_eq!(
        fixed_by_edits(text, &error),
        "type ContactEmail = Text {\n    format: TextFormat.email\n}\n"
    );
}

#[test]
fn record_fields_have_no_shorthand() {
    let text =
        "function make(name: Name) -> Product {\n    return Product {\n        name\n    }\n}\n";
    let (error, _) = one_error(text);
    assert_eq!(error.rule.id, "syntax.colon_required");
    assert_eq!(error.message, "`name` needs a value");
    fixed_by_edits(text, &error);
}

#[test]
fn condition_brackets() {
    let text = "function size(count: Count) -> Size {\n    if count > 10 {\n        return \"large\"\n    }\n    return \"small\"\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.condition_brackets");
    assert_eq!(covered, "count > 10");
    fixed_by_edits(text, &error);
}

#[test]
fn attempt_placement() {
    let text = "function doubled(count: Count) fails TooBig -> Count {\n    return Count(attempt checked(count) * 2)\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.attempt_placement");
    assert_eq!(covered, "attempt");
}

#[test]
fn a_construction_maps_its_failures_in_a_block() {
    let text = "function doubled(count: Count) fails TooBig -> Count {\n    var doubled: Count = attempt Count(count * 2) invalid: TooBig\n    return doubled\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.mapping_block_required");
    assert_eq!(covered, "invalid: TooBig");
    assert_eq!(
        fixed_by_edits(text, &error),
        "function doubled(count: Count) fails TooBig -> Count {\n    var doubled: Count = attempt Count(count * 2) {\n        invalid: TooBig\n    }\n    return doubled\n}\n"
    );
    // Several trailing mappings move into one block, each on its own line.
    let text = "function f(n: Count) fails Low, High -> Count {\n    return attempt Count(n) min: Low max: High\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.mapping_block_required");
    assert_eq!(covered, "min: Low max: High");
    assert_eq!(
        fixed_by_edits(text, &error),
        "function f(n: Count) fails Low, High -> Count {\n    return attempt Count(n) {\n        min: Low\n        max: High\n    }\n}\n"
    );
}

#[test]
fn a_block_after_an_attempted_call_holds_its_mappings() {
    let text = "function f(n: Count) fails Low, High -> Count {\n    return attempt Count(n) {\n        min: Low\n        max: High\n    }\n}\n";
    let parse = parse_file(text);
    assert!(parse.errors().is_empty(), "{:#?}", parse.errors());
    let file = SourceFile::cast(parse.syntax()).expect("a file");
    let attempt = file
        .syntax()
        .descendants()
        .find_map(jadpo_syntax::ast::AttemptExpr::cast)
        .expect("an attempt");
    assert!(matches!(
        attempt.expr(),
        Some(jadpo_syntax::ast::Expr::CallExpr(_))
    ));
    let keys: Vec<String> = attempt
        .member_list()
        .expect("a block")
        .member_items()
        .map(|item| item.syntax().text().to_string())
        .collect();
    assert_eq!(keys, ["min: Low", "max: High"]);
}

#[test]
fn fails_after_result() {
    let text = "function check(title: Title) -> Title fails TitleBlank {\n    return title\n}\n";
    let (error, covered) = one_error(text);
    assert_eq!(error.rule.id, "syntax.fails_after_result");
    assert_eq!(covered, "fails TitleBlank");
    assert_eq!(
        fixed_by_edits(text, &error),
        "function check(title: Title) fails TitleBlank -> Title {\n    return title\n}\n"
    );
}

#[test]
fn elision_is_only_for_examples() {
    let (error, _) = one_error("entity Order {\n    ...\n}\n");
    assert_eq!(
        error.message,
        "`...` stands for code left out of an example"
    );
    assert!(
        jadpo_syntax::parse_snippet("entity Order {\n    ...\n}\n")
            .errors()
            .is_empty()
    );
}

#[test]
fn recovery_keeps_later_declarations() {
    let text = "type A = Object {\n    first: $\n}\n\ntype B = Uuid {}\n\nfunction f() -> B {\n    return\n}\n";
    let parse = parse_file(text);
    assert_eq!(parse.errors().len(), 1, "{:#?}", parse.errors());
    let file = SourceFile::cast(parse.syntax()).expect("a source file");
    assert_eq!(file.decls().count(), 3);
}

/// Operators other languages spell differently (area 3, "Expressions and
/// operators"): each is one error whose edit writes Jadpo's spelling, and
/// the rest of the line parses as meant, so nothing follows from it.
#[test]
fn other_languages_operators_are_one_error_each() {
    let cases = [
        ("a === b", "syntax.equality_operator", "===", "a == b"),
        ("a !== b", "syntax.equality_operator", "!==", "a != b"),
        ("a && b", "syntax.unexpected_character", "&&", "a and b"),
        ("a || b", "syntax.unexpected_character", "||", "a or b"),
        ("!a", "syntax.unexpected_character", "!", "not a"),
    ];
    for (written, rule, covered, meant) in cases {
        let text = format!(
            "type Flag = Bool {{}}\n\nfunction f(a: Flag, b: Flag) -> Flag {{\n    return {written}\n}}\n"
        );
        let (error, carets) = one_error(&text);
        assert_eq!(
            (error.rule.id, carets.as_str()),
            (rule, covered),
            "{written}"
        );
        let fixed = fixed_by_edits(&text, &error);
        assert!(fixed.contains(&format!("return {meant}\n")), "{fixed}");
    }
    // In a query's condition too, with the rest of the line kept.
    let (error, _) = one_error(
        "query q(filter: Filter) -> Page<View> {\n    from: Book\n    where_present: Book.wear === filter.wear\n}\n",
    );
    assert_eq!(error.message, "Jadpo compares with `==`, not `===`");
}
