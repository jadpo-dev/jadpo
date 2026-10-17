//! Area 1, "Declining a judgement call" (question 11, decided 2026-10-17,
//! chunk 5.4b): every finding is an error, and a declinable one is
//! answered by a `decline <rule>: "<reason>"` line directly above the
//! declaration it is on. A decline whose rule no longer fires there is
//! `decline.unused`; naming a rule that can't be declined is
//! `decline.not_declinable`; a decline without a reason is
//! `decline.reason_required`.

use crate::support;

use jadpo_db::{Database, Layout};
use support::problems;

fn loose(text: &str) -> Vec<String> {
    problems(Layout::Loose, &[("main.jadpo", text)])
}

const SIMILAR: &str = "type Pence = Int {\n    min: 0\n}\n\nlibrary rounding {\n    /// The amount, capped at a thousand pence.\n    function capped(amount: Pence) -> Pence {\n        if (amount > Pence(1000)) {\n            return Pence(1000)\n        }\n        return amount\n    }\n}\n\n";

const CAP: &str = "function cap_total(total: Pence) -> Pence {\n    if (total > Pence(1000)) {\n        return Pence(1000)\n    }\n    return total\n}\n";

#[test]
fn a_similar_library_function_is_an_error_until_declined() {
    assert_eq!(
        loose(&format!("{SIMILAR}{CAP}")),
        ["main.jadpo library.similar_exists: A similar library function exists: `rounding.capped`"]
    );
    assert_eq!(
        loose(&format!(
            "{SIMILAR}decline library.similar_exists: \"Caps the basket, not a line\"\n{CAP}"
        )),
        Vec::<String>::new()
    );
}

#[test]
fn a_similar_constant_is_declined_on_the_constant() {
    let text = "type Pence = Int {\n    min: 0\n}\n\nconstant fee: Pence = 250\ndecline constant.similar_exists: \"A delivery charge that happens to match the fee\"\nconstant charge: Pence = 250\n\nfunction total() -> Pence {\n    return Pence(fee + charge)\n}\n";
    assert_eq!(loose(text), Vec::<String>::new());
}

#[test]
fn a_decline_covers_its_rule_on_its_declaration_only() {
    // On another declaration, the finding stays and the decline is unused
    // once nothing else is wrong; here the finding still stands.
    let text = format!(
        "{SIMILAR}decline library.similar_exists: \"Wrong place\"\nfunction other(total: Pence) -> Pence {{\n    return total\n}}\n\n{CAP}"
    );
    assert_eq!(
        loose(&text),
        [
            "main.jadpo decline.unused: Nothing to decline: `library.similar_exists` doesn't fire here",
            "main.jadpo library.similar_exists: A similar library function exists: `rounding.capped`",
        ]
    );
}

#[test]
fn a_decline_whose_rule_no_longer_fires_is_unused() {
    let text = "type Pence = Int {\n    min: 0\n}\n\ndecline library.similar_exists: \"Once a copy\"\nfunction cap_total(total: Pence) -> Pence {\n    return total\n}\n";
    assert_eq!(
        loose(text),
        [
            "main.jadpo decline.unused: Nothing to decline: `library.similar_exists` doesn't fire here"
        ]
    );
}

#[test]
fn an_unused_decline_waits_while_other_errors_stand() {
    let text = "type Pence = Int {\n    min: 0\n}\n\ndecline library.similar_exists: \"Once a copy\"\nfunction cap_total(total: Pence) -> Pence {\n    return totl\n}\n";
    let found = loose(text);
    assert!(
        found
            .iter()
            .all(|problem| !problem.contains("decline.unused")),
        "{found:?}"
    );
}

#[test]
fn a_rule_that_cant_be_declined_is_not_declinable() {
    let text = "type Pence = Int {\n    min: 0\n}\n\ndecline semantic.unknown_name: \"It is fine\"\nfunction f(a: Pence) -> Pence {\n    return a\n}\n";
    assert_eq!(
        loose(text),
        ["main.jadpo decline.not_declinable: `semantic.unknown_name` can't be declined"]
    );
}

#[test]
fn an_unknown_rule_is_not_declinable_with_the_nearest_declinable_one() {
    let db = Database::default();
    let text = format!("{SIMILAR}decline library.similar_exist: \"Caps the basket\"\n{CAP}");
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &text)]);
    let found = jadpo_check::check(&db, project);
    let rules: Vec<&str> = found.iter().map(|d| d.rule.id).collect();
    assert_eq!(rules, ["decline.not_declinable", "library.similar_exists"]);
    let next = found[0].next.as_ref().expect("a next step");
    assert_eq!(
        next.text,
        "Use `library.similar_exists` instead of `library.similar_exist`"
    );
    assert_eq!(next.edits[0].replacement, "library.similar_exists");
}

#[test]
fn a_decline_needs_a_reason() {
    for reason in [": \"\"", ": \"   \"", ""] {
        let text = format!("{SIMILAR}decline library.similar_exists{reason}\n{CAP}");
        assert_eq!(
            loose(&text),
            ["main.jadpo decline.reason_required: This decline gives no reason"],
            "{reason}"
        );
    }
}

#[test]
fn a_library_function_is_declined_inside_its_library() {
    // The later of two alike library functions is reported, and declined
    // inside the library.
    let text = "type Pence = Int {\n    min: 0\n}\n\nlibrary rounding {\n    function capped(amount: Pence) -> Pence {\n        return amount\n    }\n\n    decline library.similar_exists: \"Kept for the old till\"\n    function kept(total: Pence) -> Pence {\n        return total\n    }\n}\n";
    assert_eq!(loose(text), Vec::<String>::new());
}

#[test]
fn the_unused_decline_is_removed_line_and_all_by_its_edit() {
    let db = Database::default();
    let text = "type Pence = Int {\n    min: 0\n}\n\n/// Caps a total.\ndecline library.similar_exists: \"Once a copy\"\nfunction cap_total(total: Pence) -> Pence {\n    return total\n}\n";
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", text)]);
    let found = jadpo_check::check(&db, project);
    let edit = &found[0].next.as_ref().expect("next").edits[0];
    let mut fixed = text.to_string();
    fixed.replace_range(
        edit.span.start as usize..edit.span.end as usize,
        &edit.replacement,
    );
    assert_eq!(
        fixed,
        "type Pence = Int {\n    min: 0\n}\n\n/// Caps a total.\nfunction cap_total(total: Pence) -> Pence {\n    return total\n}\n"
    );
}

#[test]
fn declines_are_listed_and_cleared_through_one_path() {
    let db = Database::default();
    let text = format!("{SIMILAR}decline library.similar_exists: \"Caps the basket\"\n{CAP}");
    let project = support::project(
        &db,
        Layout::Loose,
        &[
            ("main.jadpo", &text),
            ("other.jadpo", "type Count = Int {\n    min: 0\n}\n"),
        ],
    );
    let all = jadpo_check::declines::declines(&db, project);
    assert_eq!(all.len(), 1);
    let decline = &all[0];
    assert_eq!(
        (decline.rule.as_str(), decline.path.as_str(), decline.line),
        ("library.similar_exists", "main.jadpo", 15)
    );
    assert_eq!(decline.reason.as_deref(), Some("Caps the basket"));
    assert_eq!(decline.subject.as_deref(), Some("cap_total"));
    let filter = jadpo_check::declines::Filter {
        rule: Some("constant.similar_exists".into()),
        path: None,
    };
    assert!(!filter.matches(decline));
    let filter = jadpo_check::declines::Filter {
        rule: None,
        path: Some("main.jadpo".into()),
    };
    assert!(filter.matches(decline));
    let cleared = jadpo_check::declines::without(&text, &[decline]);
    assert_eq!(cleared, format!("{SIMILAR}{CAP}"));
}

#[test]
fn declining_a_finding_inserts_the_line_below_its_comments() {
    let db = Database::default();
    let text = format!(
        "{SIMILAR}/// Caps the total.\npublic function cap_total(total: Pence) -> Pence {{\n    if (total > Pence(1000)) {{\n        return Pence(1000)\n    }}\n    return total\n}}\n"
    );
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &text)]);
    let found = jadpo_check::check(&db, project);
    let similar = found
        .iter()
        .find(|d| d.rule.id == "library.similar_exists")
        .expect("the finding");
    let edit = jadpo_check::declines::insertion(&db, project, similar).expect("an edit");
    let mut declined = text.clone();
    declined.insert_str(edit.span.start as usize, &edit.replacement);
    assert!(
        declined.contains(
            "/// Caps the total.\ndecline library.similar_exists: \"\"\npublic function cap_total"
        ),
        "{declined}"
    );
}
