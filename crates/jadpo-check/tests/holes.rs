//! Typed holes (area 2, "Typed holes"): `_` where a value goes is always
//! the error `type.hole`, which names the type the place requires, as the
//! code would write it, and up to five values in scope that fit, closest
//! first, with the edit that writes the closest.

use jadpo_db::{Database, Layout};
use jadpo_diag::{Diagnostic, rule};

use crate::support;

fn checked(layout: Layout, files: &[(&str, &str)]) -> Vec<Diagnostic> {
    let db = Database::default();
    let project = support::project(&db, layout, files);
    jadpo_check::check(&db, project)
}

fn loose(code: &str) -> Vec<Diagnostic> {
    checked(Layout::Loose, &[("main.jadpo", code)])
}

/// The one error a project gives, which must be a hole.
fn one_hole(found: Vec<Diagnostic>) -> Diagnostic {
    let rules: Vec<&str> = found.iter().map(|d| d.rule.id).collect();
    assert_eq!(rules, ["type.hole"], "{found:#?}");
    found.into_iter().next().unwrap()
}

fn fits(hole: &Diagnostic) -> Vec<String> {
    hole.hole
        .as_ref()
        .map(|hole| hole.fits.iter().map(|fit| fit.text.clone()).collect())
        .unwrap_or_default()
}

fn next(hole: &Diagnostic) -> (String, Vec<String>) {
    let next = hole.next.as_ref().expect("a next step");
    (
        next.text.clone(),
        next.edits
            .iter()
            .map(|edit| edit.replacement.clone())
            .collect(),
    )
}

const GREET: &str = "use hello/types { GreetInput, Greeting }\n\n/// Greets someone by name.\naction greet(input: GreetInput) -> Greeting {\n    return Greeting {\n        name: _\n    }\n}\n";

fn greeting(action: &str) -> Vec<Diagnostic> {
    let mut files = vec![
        ("app.jadpo", "application {}\n"),
        (
            "domains/hello/types/greeting.jadpo",
            "/// The greeting.\ntype Greeting = Output {\n    name: PersonName\n}\n",
        ),
        ("domains/hello/actions/greet.jadpo", action),
    ];
    if action.contains("GreetInput") {
        files.push((
            "domains/hello/types/greet_input.jadpo",
            "/// Who to greet.\ntype GreetInput = Input {\n    name: PersonName\n}\n",
        ));
    }
    checked(Layout::Project, &files)
}

#[test]
fn a_field_of_an_object_names_its_type_and_what_fits() {
    let hole = one_hole(greeting(GREET));
    // Building an object accepts the field's declared type (area 2,
    // question 2), so that is the type a hole in it names.
    assert_eq!(hole.message, "`_` stands for a `PersonName`");
    let label = hole.primary.as_ref().expect("a place");
    assert_eq!(label.text, "a `PersonName` goes here");
    assert_eq!(
        &GREET[label.span.start as usize..label.span.end as usize],
        "_"
    );
    assert_eq!(
        hole.reason.as_deref(),
        Some(
            "`PersonName` is an object: `PersonName { given: GivenName, family: FamilyName? }`. In scope, `input.name` fits."
        )
    );
    assert_eq!(
        next(&hole),
        (
            "Write `input.name`".to_string(),
            vec!["input.name".to_string()]
        )
    );
    let found = hole.hole.as_ref().expect("the hole's data");
    assert_eq!(found.expected.as_deref(), Some("PersonName"));
    assert_eq!(fits(&hole), ["input.name"]);
    assert_eq!(found.fits[0].ty, "GreetInput.name");
    assert!(!rule::TYPE_HOLE.declinable);
}

#[test]
fn with_nothing_that_fits_the_object_is_written_out_with_holes() {
    let action = GREET
        .replace("action greet(input: GreetInput)", "action greet()")
        .replace(
            "use hello/types { GreetInput, Greeting }",
            "use hello/types { Greeting }",
        );
    let hole = one_hole(greeting(&action));
    assert_eq!(
        hole.reason.as_deref(),
        Some(
            "`PersonName` is an object: `PersonName { given: GivenName, family: FamilyName? }`. Nothing in scope fits."
        )
    );
    assert_eq!(
        next(&hole),
        (
            "Write `PersonName { given: _, family: none }`, and see `jadpo help PersonName` for the type".to_string(),
            vec!["PersonName { given: _, family: none }".to_string()]
        )
    );
    assert!(fits(&hole).is_empty());
}

const PRELUDE: &str =
    "type Stock = Int {\n    min: 0\n}\n\ntype Title = Text {\n    max_length: 80\n}\n\n";

#[test]
fn a_hole_stands_wherever_a_value_goes() {
    for (code, expected) in [
        (
            "function f(a: Stock) -> Stock {\n    return _\n}\n",
            "Stock",
        ),
        (
            "function f(a: Stock) -> Stock {\n    var b: Stock = _\n    return b\n}\n",
            "Stock",
        ),
        (
            "function g(a: Stock) -> Stock {\n    return a\n}\n\nfunction f(a: Stock) -> Stock {\n    return g(_)\n}\n",
            "Stock",
        ),
    ] {
        let hole = one_hole(loose(&format!("{PRELUDE}{code}")));
        assert_eq!(
            hole.hole
                .as_ref()
                .and_then(|h| h.expected.clone())
                .as_deref(),
            Some(expected),
            "{code}"
        );
        assert_eq!(fits(&hole), ["a"], "{code}");
        assert_eq!(next(&hole).1, ["a"], "{code}");
    }
}

#[test]
fn the_nearest_value_comes_first_and_five_at_most() {
    let code = format!(
        "{PRELUDE}function f(a: Stock, b: Stock) -> Stock {{\n    var c: Stock = a\n    var d: Stock = b\n    var e: Stock = c\n    var g: Stock = d\n    var h: Title = \"x\"\n    return _\n}}\n"
    );
    let hole = one_hole(loose(&code));
    assert_eq!(fits(&hole), ["g", "e", "d", "c", "b"]);
    assert_eq!(
        hole.reason.as_deref(),
        Some("`Stock` is built on `Int`. In scope, `g`, `e`, `d`, `c` and `b` fit, closest first.")
    );
}

#[test]
fn a_value_that_widens_fits_and_its_fields_follow_the_locals() {
    let code = format!(
        "{PRELUDE}type ShortTitle = Title {{\n    max_length: 20\n}}\n\ntype Book = Object {{\n    title: Title\n    count: Stock\n}}\n\nfunction f(book: Book, label: ShortTitle) -> Stock {{\n    var shown: Title = _\n    return 1\n}}\n"
    );
    let hole = one_hole(loose(&code));
    assert_eq!(fits(&hole), ["label", "book.title"]);
    let types: Vec<String> = hole
        .hole
        .as_ref()
        .unwrap()
        .fits
        .iter()
        .map(|fit| fit.ty.clone())
        .collect();
    assert_eq!(types, ["ShortTitle", "Book.title"]);
}

#[test]
fn a_constant_and_the_clock_fit() {
    let code = format!(
        "{PRELUDE}constant limit: Stock = 10\n\nfunction f() -> Stock {{\n    return _\n}}\n\naction when() -> Stock {{\n    var at: Instant = _\n    return 1\n}}\n\nfunction pure() -> Stock {{\n    var at: Instant = _\n    return 1\n}}\n"
    );
    let found = loose(&code);
    let holes: Vec<Vec<String>> = found.iter().map(fits).collect();
    assert_eq!(
        found.iter().map(|d| d.rule.id).collect::<Vec<_>>(),
        ["type.hole", "type.hole", "type.hole"],
        "{found:#?}"
    );
    assert_eq!(holes[0], ["limit"]);
    assert_eq!(holes[1], ["clock.now"]);
    // A function is pure: it never reads the clock.
    assert!(holes[2].is_empty(), "{:?}", holes[2]);
}

#[test]
fn a_call_fits_when_its_arguments_are_in_scope() {
    let code = format!(
        "{PRELUDE}function f() -> Stock {{\n    var name: Text = \"Ann\"\n    var length: Int = _\n    return 1\n}}\n"
    );
    let hole = one_hole(loose(&code));
    let found = fits(&hole);
    assert!(
        found.iter().any(|fit| fit == "text.length(name)"),
        "{found:?}"
    );
    assert!(found.iter().all(|fit| fit.contains("(name")), "{found:?}");
}

#[test]
fn where_nothing_says_the_type_the_hole_says_so() {
    for code in [
        "type Stock = Int {}\n\nfunction f() -> Stock {\n    return Stock(_ + 1)\n}\n",
        "type Stock = Int {}\n\nfunction f() -> Stock {\n    return _.count\n}\n",
    ] {
        let hole = one_hole(loose(code));
        assert_eq!(hole.message, "`_` has no type here", "{code}");
        assert_eq!(
            next(&hole).0,
            "Declare the value with its type first, `var name: Type = _`, and use the name here"
        );
        assert_eq!(hole.hole.as_ref().unwrap().expected, None);
    }
}

#[test]
fn a_hole_leaves_what_is_in_scope_unreported() {
    let code = format!(
        "{PRELUDE}function f() -> Stock {{\n    var spare: Title = \"x\"\n    return _\n}}\n"
    );
    one_hole(loose(&code));
}

#[test]
fn a_hole_outside_code_is_still_one() {
    let hole = one_hole(loose("type Count = Int {\n    min: _\n}\n"));
    assert_eq!(hole.message, "`_` stands for a value here");
}

#[test]
fn a_hole_cannot_be_declined() {
    let found = loose(&format!(
        "{PRELUDE}decline type.hole: \"Not decided yet\"\nfunction f() -> Stock {{\n    return _\n}}\n"
    ));
    let rules: Vec<&str> = found.iter().map(|d| d.rule.id).collect();
    assert!(rules.contains(&"type.hole"), "{rules:?}");
    assert!(rules.contains(&"decline.not_declinable"), "{rules:?}");
}
