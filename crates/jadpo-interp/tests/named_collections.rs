//! Area 2, "Named collection construction" (chunk 2.11), run: a named
//! list, set or map type built from a collection of its item type checks
//! the collection's own rules when it runs, raising the arm's failure for
//! the rule that broke; a set built from a list drops its repeats and
//! keeps the canonical order; a proven construction holds with its proof
//! checked again.

use crate::support;

use jadpo_interp::Value;
use support::{failure, list, main, text, value};

const SOURCE: &str = "type Name = Text {\n    min_length: 1\n}\n\ntype Names = List<Name> {\n    min_length: 1\n    max_length: 3\n}\n\ntype NameSet = Set<Name> {\n    min_length: 1\n    max_length: 2\n}\n\ntype Pair = List<Name> {\n    min_length: 2\n}\n\ntype NoNames = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype TooMany = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction gather(found: List<Name>) fails NoNames, TooMany -> Names {\n    var result: Names = attempt Names(found) {\n        min_length: NoNames\n        max_length: TooMany\n    }\n    return result\n}\n\nfunction distinct(found: List<Name>) fails NoNames, TooMany -> NameSet {\n    var result: NameSet = attempt NameSet(found) {\n        min_length: NoNames\n        invalid: TooMany\n    }\n    return result\n}\n\nfunction checked(found: List<Name>) fails NoNames -> Names {\n    if (list.count(found) == 0 or list.count(found) > 3) {\n        reject NoNames\n    }\n    return Names(found)\n}\n\nfunction from_pair(pair: Pair) fails TooMany -> Names {\n    var result: Names = attempt Names(pair) {\n        max_length: TooMany\n    }\n    return result\n}\n\nfunction written(a: Name, b: Name) -> NameSet {\n    return NameSet([b, a])\n}\n";

fn names(items: &[&str]) -> Value {
    list(items.iter().map(|item| text(item)).collect())
}

fn set(items: &[&str]) -> Value {
    Value::set(items.iter().map(|item| text(item)).collect())
}

#[test]
fn a_named_list_checks_its_own_rules_when_it_runs() {
    let program = main(SOURCE);
    assert_eq!(
        value(&program, "gather", vec![names(&["Ada", "Grace"])]),
        names(&["Ada", "Grace"])
    );
    let empty = failure(&program, "gather", vec![names(&[])]);
    assert_eq!(empty.name, "NoNames");
    assert_eq!(empty.broken.as_deref(), Some("`min_length: 1`"));
    let many = failure(&program, "gather", vec![names(&["a", "b", "c", "d"])]);
    assert_eq!(many.name, "TooMany");
    assert_eq!(many.broken.as_deref(), Some("`max_length: 3`"));
}

#[test]
fn a_named_set_built_from_a_list_drops_repeats_in_canonical_order() {
    let program = main(SOURCE);
    let built = value(
        &program,
        "distinct",
        vec![names(&["Grace", "Ada", "Grace"])],
    );
    assert!(matches!(built, Value::Set(_)), "{built:?}");
    assert_eq!(built, set(&["Ada", "Grace"]));
    assert_eq!(built.to_string(), "[\"Ada\", \"Grace\"]");
    // Its lengths count what is left once repeats collapse.
    assert_eq!(
        value(&program, "distinct", vec![names(&["Ada", "Ada", "Ada"])]),
        set(&["Ada"])
    );
    assert_eq!(
        failure(&program, "distinct", vec![names(&[])]).name,
        "NoNames"
    );
    let many = failure(&program, "distinct", vec![names(&["a", "b", "c"])]);
    assert_eq!(many.name, "TooMany");
    assert_eq!(many.broken.as_deref(), Some("`max_length: 2`"));
    // A list written in the construction is a set as well.
    assert_eq!(
        value(&program, "written", vec![text("Grace"), text("Ada")]),
        set(&["Ada", "Grace"])
    );
}

#[test]
fn a_proven_named_collection_holds_with_its_proof_checked_again() {
    let program = main(SOURCE).with_proof_checks();
    for items in [&["Ada"][..], &["a", "b", "c"]] {
        assert_eq!(value(&program, "checked", vec![names(items)]), names(items));
    }
    assert_eq!(
        failure(&program, "checked", vec![names(&[])]).name,
        "NoNames"
    );
    assert_eq!(
        value(&program, "from_pair", vec![names(&["a", "b"])]),
        names(&["a", "b"])
    );
    assert_eq!(
        failure(&program, "from_pair", vec![names(&["a", "b", "c", "d"])]).name,
        "TooMany"
    );
}

#[test]
fn a_construction_from_a_value_that_may_be_none_never_runs() {
    let db = jadpo_db::Database::default();
    let file = jadpo_db::File::new(
        &db,
        "main.jadpo".to_string(),
        "type Count = Int {}\n\ntype Quantity = Int {\n    min: 1\n}\n\nfunction quantity(maybe: Count?) -> Quantity {\n    return Quantity(maybe)\n}\n".to_string(),
    );
    let project = jadpo_db::Project::new(
        &db,
        vec![file],
        Vec::new(),
        jadpo_db::Layout::Loose,
        Default::default(),
        Vec::new(),
    );
    let Err(errors) = jadpo_interp::Program::load(&db, project) else {
        panic!("a construction from `Count?` loaded");
    };
    assert_eq!(
        errors.iter().map(|d| d.rule.id).collect::<Vec<_>>(),
        ["type.constructor_input"]
    );
}
