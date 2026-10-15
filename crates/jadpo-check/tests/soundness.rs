//! The Phase 2 review's soundness findings in the checker (roadmap chunk
//! 2.7): a proof that passed a value its type refuses, and a call that
//! checked clean and then faulted, are each reported now.

mod support;

use jadpo_db::{Database, Layout};
use jadpo_diag::Diagnostic;
use support::rules;

fn check(source: &str) -> Vec<String> {
    rules(Layout::Loose, &[("main.jadpo", source)])
}

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", source)]);
    jadpo_check::check(&db, project)
}

const CLEAN: &[String] = &[];

// --- finding 1: a built-in text type's own check is its `format` ----------

const ADDRESSES: &str = "type Raw = Text {}\n\ntype Mail = Text {\n    format: TextFormat.email\n}\n\ntype Plain = Email {}\n\ntype WorkEmail = Email {\n    pattern: \".*@acme[.]com\"\n}\n\ntype Link = Url {}\n\ntype Contact = Object {\n    email: Email\n}\n\ntype NotWork = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype NotAnAddress = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn with_addresses(source: &str) -> Vec<String> {
    check(&format!("{ADDRESSES}{source}"))
}

#[test]
fn text_built_into_an_address_type_can_break_its_format() {
    assert_eq!(
        with_addresses("function f(t: Raw) -> Plain {\n    return Plain(t)\n}\n"),
        ["failure.attempt_required"]
    );
    assert_eq!(
        with_addresses("function f(t: Raw) -> Email {\n    return Email(t)\n}\n"),
        ["failure.attempt_required"]
    );
    assert_eq!(
        with_addresses("function f(t: Raw) -> Link {\n    return Link(t)\n}\n"),
        ["failure.attempt_required"]
    );
    assert_eq!(
        with_addresses("function f(t: Raw) -> Contact.email {\n    return Contact.email(t)\n}\n"),
        ["failure.attempt_required"]
    );
    assert_eq!(
        with_addresses(
            "function f(t: Raw) fails NotWork -> WorkEmail {\n    var e: WorkEmail = attempt WorkEmail(t) {\n        pattern: NotWork\n    }\n    return e\n}\n"
        ),
        ["failure.rule_unmapped"]
    );
    assert_eq!(
        with_addresses(
            "function f(t: Raw) fails NotWork, NotAnAddress -> WorkEmail {\n    var e: WorkEmail = attempt WorkEmail(t) {\n        pattern: NotWork\n        format: NotAnAddress\n    }\n    return e\n}\n"
        ),
        CLEAN
    );
}

#[test]
fn a_value_already_an_address_proves_the_format() {
    // An `Email`, or text whose own rules check the same format.
    assert_eq!(
        with_addresses(
            "function f(e: Email) fails NotWork, NotAnAddress -> WorkEmail {\n    var w: WorkEmail = attempt WorkEmail(e) {\n        pattern: NotWork\n        format: NotAnAddress\n    }\n    return w\n}\n"
        ),
        ["failure.rule_cannot_break"]
    );
    assert_eq!(
        with_addresses("function f(m: Mail) -> Plain {\n    return Plain(m)\n}\n"),
        CLEAN
    );
}

#[test]
fn the_format_arm_names_the_built_in_check() {
    let found = diagnostics(&format!(
        "{ADDRESSES}function f(t: Raw) fails NotWork -> WorkEmail {{\n    var e: WorkEmail = attempt WorkEmail(t) {{\n        pattern: NotWork\n    }}\n    return e\n}}\n"
    ));
    let reason = found
        .iter()
        .find_map(|d| d.reason.clone())
        .unwrap_or_default();
    assert!(
        reason.contains("`Email`"),
        "the reason names the check `format` stands for: {reason}"
    );
}

// --- finding 2: decimal bounds far apart in scale compare exactly ---------

#[test]
fn a_decimal_bound_far_apart_in_scale_is_not_proved() {
    assert_eq!(
        check(
            "type Big = Decimal {\n    scale: 1\n    min: 0.0\n    max: 100000000000000000000000000000.0\n}\n\ntype Tiny = Decimal {\n    scale: 10\n    min: 0.0\n    max: 0.0000000005\n}\n\nfunction shrink(b: Big) -> Tiny {\n    return Tiny(b)\n}\n"
        ),
        ["failure.attempt_required"]
    );
}

// --- finding 5: repeats in a set literal collapse --------------------------

const SETS: &str = "type Name = Text {}\n\ntype Pair = Set<Name> {\n    min_length: 2\n}\n\ntype Few = Set<Name> {\n    max_length: 2\n}\n\n";

#[test]
fn a_set_literal_of_computed_items_may_hold_fewer() {
    assert_eq!(
        check(&format!(
            "{SETS}function pair(a: Name, b: Name) -> Pair {{\n    return [a, b]\n}}\n"
        )),
        ["type.invalid_literal"]
    );
    // Distinct literals count, and so does one item of any kind.
    assert_eq!(
        check(&format!(
            "{SETS}function pair(a: Name) -> Pair {{\n    return [a, \"x\", \"y\"]\n}}\n"
        )),
        CLEAN
    );
    assert_eq!(
        check(&format!(
            "{SETS}function pair() -> Pair {{\n    return [\"x\", \"y\"]\n}}\n"
        )),
        CLEAN
    );
    // Collapsing only shrinks a set, so its `max_length` still holds.
    assert_eq!(
        check(&format!(
            "{SETS}function few(a: Name, b: Name) -> Few {{\n    return [a, b]\n}}\n"
        )),
        CLEAN
    );
}

// --- finding 8: no conversion between collections ---------------------------

const LISTS: &str = "type Tag = Text {\n    max_length: 3\n}\n\ntype Raw = Text {}\n\ntype Name = Text {}\n\ntype Pair = Set<Name> {}\n\ntype Count = Int {}\n\n";

#[test]
fn a_collection_type_is_not_a_constructor() {
    assert_eq!(
        check(&format!(
            "{LISTS}function tags(xs: List<Raw>) -> List<Tag> {{\n    return List<Tag>(xs)\n}}\n"
        )),
        ["type.constructor_input"]
    );
    assert_eq!(
        check(&format!(
            "{LISTS}function names(xs: List<Name>) -> List<Name> {{\n    return List<Name>(xs)\n}}\n"
        )),
        ["type.constructor_input"]
    );
}

#[test]
fn a_set_where_a_list_is_required_suggests_no_conversion() {
    let found = diagnostics(&format!(
        "{LISTS}function count(p: Pair) -> Count {{\n    return Count(collection.count(p))\n}}\n"
    ));
    assert_eq!(found.len(), 1, "{found:#?}");
    let problem = &found[0];
    assert_eq!(problem.rule.id, "type.mismatch");
    let next = problem
        .next
        .as_ref()
        .map(|n| n.text.clone())
        .unwrap_or_default();
    let reason = problem.reason.clone().unwrap_or_default();
    assert!(
        !next.contains("List<T>(") && !reason.contains("refinement"),
        "no conversion is suggested: {reason} / {next}"
    );
}

// --- finding 9: calls name one owner ---------------------------------------

fn in_project(path: &str, text: &str) -> Vec<String> {
    rules(
        Layout::Project,
        &[
            support::APP,
            (
                "domains/pantry/types/sack_count.jadpo",
                "public type SackCount = Int {\n    min: 1\n    max: 6\n}\n",
            ),
            (
                "domains/pantry/types/grams.jadpo",
                "public type Grams = Int {\n    min: 0\n    max: 100000\n}\n",
            ),
            (
                "domains/pantry/libraries/sacks.jadpo",
                "use pantry/types { Grams, SackCount }\n\nlibrary sacks {\n    function grams_in(count: SackCount) -> Grams {\n        return Grams(count * 16000)\n    }\n}\n",
            ),
            (
                "domains/pantry/actions/weigh.jadpo",
                "use pantry/types { Grams, SackCount }\nuse pantry/libraries { sacks }\n\npublic action weigh(count: SackCount) -> Grams {\n    return sacks.grams_in(count)\n}\n",
            ),
            // What makes the pantry's declarations public: another part
            // of the project uses them (area 1, "Unused `public`").
            (
                "tests/the_pantry_weighs.test.jadpo",
                "use pantry\n\ntest \"the pantry weighs a sack\" {\n    assert pantry.weigh(1) == 16000\n}\n",
            ),
            (path, text),
        ],
    )
}

#[test]
fn a_call_through_three_names_is_reported() {
    assert_eq!(
        in_project(
            "tests/weighing.test.jadpo",
            "use pantry/types { Grams }\nuse pantry\n\ntest \"a sack weighs\" {\n    var g: Grams = pantry.sacks.grams_in(1)\n    assert g == 16000\n}\n"
        ),
        ["semantic.unknown_callee"]
    );
    assert_eq!(
        in_project(
            "domains/pantry/actions/count.jadpo",
            "use pantry/types { Grams, SackCount }\n\naction count(n: SackCount) -> Grams {\n    return pantry.nonexistent.thing(n)\n}\n"
        ),
        ["semantic.unknown_callee"]
    );
    assert_eq!(
        in_project(
            "domains/pantry/actions/count.jadpo",
            "use pantry/types { Grams, SackCount }\nuse pantry/libraries { sacks }\n\naction count(n: SackCount) -> Grams {\n    return sacks.grams_in.again(n)\n}\n"
        ),
        ["semantic.unknown_callee"]
    );
    assert_eq!(
        in_project(
            "domains/pantry/actions/count.jadpo",
            "use pantry/types { Grams, SackCount }\nuse pantry/libraries { sacks }\n\naction count(n: SackCount) -> Grams {\n    return sacks.grams_in(n)\n}\n"
        ),
        CLEAN
    );
}

#[test]
fn a_member_of_a_type_that_is_not_a_field_is_reported() {
    assert_eq!(
        check(&format!(
            "{LISTS}function f(n: Count) -> Count {{\n    return Count.nothing(n)\n}}\n"
        )),
        ["semantic.unknown_callee"]
    );
    // A value has no operations of its own.
    assert_eq!(
        check(&format!(
            "{LISTS}function f(n: Count) -> Count {{\n    return n.nothing(1)\n}}\n"
        )),
        ["semantic.not_callable"]
    );
}

// --- finding 10: a refinement only adds rules -------------------------------

#[test]
fn a_refinement_that_loosens_an_inherited_rule_is_an_error() {
    let base = "type JarName = Text {\n    min_length: 1\n    max_length: 40\n}\n\ntype Weight = Int {\n    min: 0\n    max: 500\n}\n\n";
    let with = |source: &str| check(&format!("{base}{source}"));
    assert_eq!(
        with("type ShelfName = JarName {\n    max_length: 50\n}\n"),
        ["type.constraint_loosened"]
    );
    assert_eq!(
        with("type ShelfName = JarName {\n    min_length: 0\n}\n"),
        ["type.constraint_loosened"]
    );
    assert_eq!(
        with("type Heavy = Weight {\n    max: 1000\n}\n"),
        ["type.constraint_loosened"]
    );
    assert_eq!(
        with("type Light = Weight {\n    min: -5\n}\n"),
        ["type.constraint_loosened"]
    );
    assert_eq!(
        with("type Jar = Object {\n    name: JarName {\n        max_length: 60\n    }\n}\n"),
        ["type.constraint_loosened"]
    );
    // Tightening, or saying the same again, adds to the rules.
    assert_eq!(
        with(
            "type ShelfName = JarName {\n    max_length: 12\n}\n\ntype Same = Weight {\n    max: 500\n}\n"
        ),
        CLEAN
    );
}
