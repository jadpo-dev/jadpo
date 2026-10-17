//! Area 19, "Lists" (chunk 5.5b): every `list` operation checks, results
//! keep their items' named type, a sum is its items' representation,
//! counts are never negative, keys are ordered, and `collection.` is
//! reported with an edit to the namespace its first value needs.

use crate::support;

use jadpo_db::{Database, Layout};
use jadpo_diag::Diagnostic;

const NAMES: &str = "type Label = Text {\n    min_length: 1\n}\n\ntype Weight = Int {\n    min: 0\n}\n\ntype Slot = Int {\n    min: 1\n    max: 6\n}\n\ntype Amount = Decimal {\n    scale: 2\n    min: 0.00\n}\n\ntype Swing = Int {\n    min: -5\n    max: 5\n}\n\ntype Step = Int {}\n\ntype Flag = Bool {}\n\ntype Seen = Instant {}\n\ntype Local = ZonedTime {}\n\ntype Crate = Object {\n    label: Label\n    weight: Weight\n    slot: Slot\n    amount: Amount\n    seen: Seen\n    local: Local\n    flag: Flag\n    note: Label?\n}\n\ntype Crates = List<Crate> {\n    min_length: 1\n}\n\n";

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    );
    jadpo_check::check(&db, project)
}

fn rules(source: &str) -> Vec<String> {
    diagnostics(source)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect()
}

const NONE: &[&str] = &[];

#[test]
fn every_list_operation_checks() {
    let source = "function counted(crates: List<Crate>) -> Weight {\n    return Weight(list.count(crates))\n}\n\nfunction weighed(crates: List<Crate>) -> Weight {\n    return Weight(list.sum(list.map(crates, c -> c.weight)))\n}\n\nfunction priced(crates: List<Crate>) -> Amount {\n    return Amount(list.sum(list.map(crates, c -> c.amount)))\n}\n\nfunction tests(crates: List<Crate>, first: Crate) -> Flag {\n    return Flag(list.any(crates, c -> c.slot == 1) and list.all(crates, c -> c.weight > 0) and list.contains(crates, first))\n}\n\nfunction found(crates: List<Crate>) -> Crate? {\n    return list.find(crates, c -> c.slot == 2)\n}\n\nfunction ends(crates: List<Crate>) -> List<Crate?> {\n    return [list.first(crates), list.last(crates), list.at(crates, 0), list.min_by(crates, c -> c.amount), list.max_by(crates, c -> c.label)]\n}\n\nfunction reordered(crates: List<Crate>, n: Weight) -> List<Crate> {\n    return list.concat(list.sort_by(list.reverse(crates), c -> c.seen), list.distinct(list.slice(crates, n, 2)))\n}\n\nfunction cut(crates: List<Crate>, n: Weight) -> List<List<Crate>> {\n    return [list.keep_first(crates, n), list.keep_last(crates, 1), list.drop_first(crates, n), list.drop_last(crates, 0)]\n}\n\nfunction joined(groups: List<List<Crate>>, extra: Crate) -> List<Crate> {\n    return list.append(list.flatten(groups), extra)\n}\n\nfunction shelved(crates: List<Crate>) -> Map<Slot, List<Crate>> {\n    return list.group_by(crates, c -> c.slot)\n}\n";
    assert_eq!(rules(source), NONE);
}

#[test]
fn results_keep_their_items_named_type_but_not_the_list_s() {
    // An item, or a list of items, is the items' own type.
    assert_eq!(
        rules(
            "function f(crates: Crates) -> List<Crate> {\n    return list.filter(crates, c -> c.weight > 1)\n}\n\nfunction g(crates: Crates) -> Crate? {\n    return list.last(crates)\n}\n"
        ),
        NONE
    );
    // A named list's rules can break (`filter` may leave none), so the
    // result is a plain list, built back by construction.
    assert_eq!(
        rules(
            "function f(crates: Crates) -> Crates {\n    return list.filter(crates, c -> c.weight > 1)\n}\n"
        ),
        ["type.mismatch"]
    );
    // An item that may be missing isn't the item.
    assert_eq!(
        rules("function f(crates: Crates) -> Crate {\n    return list.first(crates)\n}\n"),
        ["type.mismatch"]
    );
    assert_eq!(
        rules(
            "function f(crates: List<Crate>, label: Label) -> List<Crate> {\n    return list.append(crates, label)\n}\n"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        rules(
            "function f(crates: List<Crate>, labels: List<Label>) -> List<Crate> {\n    return list.concat(crates, labels)\n}\n"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        rules(
            "function f(crates: List<Crate>) -> Map<Slot, List<Label>> {\n    return list.group_by(crates, c -> c.slot)\n}\n"
        ),
        ["type.mismatch"]
    );
}

#[test]
fn a_sum_is_its_items_representation() {
    // A sum is built back into a named type: items never below zero prove
    // the sum isn't either, so only a `max` needs an arm.
    assert_eq!(
        rules("function f(weights: List<Weight>) -> Weight {\n    return list.sum(weights)\n}\n"),
        ["type.mismatch"]
    );
    assert_eq!(
        rules(
            "function f(swings: List<Swing>) -> Weight {\n    return Weight(list.sum(swings))\n}\n"
        ),
        ["failure.attempt_required"]
    );
    assert_eq!(
        rules(
            "function f(weights: List<Weight>) -> Slot {\n    return Slot(list.sum(weights))\n}\n"
        ),
        ["failure.attempt_required"]
    );
    // Only numbers are added.
    assert_eq!(
        rules(
            "function f(labels: List<Label>) -> Weight {\n    return Weight(list.sum(labels))\n}\n"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        rules(
            "function f(notes: List<Label?>) -> Weight {\n    return Weight(list.sum(notes))\n}\n"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        rules("function f(weight: Weight) -> Weight {\n    return Weight(list.sum(weight))\n}\n"),
        ["type.mismatch"]
    );
}

#[test]
fn counts_and_positions_are_never_negative() {
    let call = |n: &str, ty: &str| {
        rules(&format!(
            "function f(crates: List<Crate>, n: {ty}) -> List<Crate> {{\n    return list.keep_first(crates, {n})\n}}\n"
        ))
    };
    assert_eq!(call("0", "Weight"), NONE);
    assert_eq!(call("n", "Weight"), NONE);
    assert_eq!(call("n", "Slot"), NONE);
    assert_eq!(call("-1", "Weight"), ["type.argument_negative"]);
    assert_eq!(call("n", "Step"), ["type.argument_negative"]);
    assert_eq!(call("n", "Swing"), ["type.argument_negative"]);
    // A call's result is a plain `Int`, `list.count`'s included.
    assert_eq!(
        call("list.count(crates)", "Weight"),
        ["type.argument_negative"]
    );
    for operation in ["keep_last", "drop_first", "drop_last", "at"] {
        let found = rules(&format!(
            "function f(crates: List<Crate>, n: Step) -> Flag {{\n    return Flag(list.{operation}(crates, n) == list.{operation}(crates, 0))\n}}\n"
        ));
        assert_eq!(found, ["type.argument_negative"], "{operation}");
    }
    assert_eq!(
        rules(
            "function f(crates: List<Crate>, n: Step) -> List<Crate> {\n    return list.slice(crates, from: n, count: -2)\n}\n"
        ),
        ["type.argument_negative", "type.argument_negative"]
    );
    let found = diagnostics(
        "function f(crates: List<Crate>, n: Step) -> List<Crate> {\n    return list.drop_last(crates, n)\n}\n",
    );
    assert_eq!(found[0].message, "`n` may be negative");
}

#[test]
fn keys_are_ordered() {
    let sorted = |key: &str| {
        rules(&format!(
            "function f(crates: List<Crate>) -> List<Crate> {{\n    return list.sort_by(crates, c -> {key})\n}}\n"
        ))
    };
    for key in [
        "c.weight",
        "c.amount",
        "c.label",
        "c.seen",
        "c.seen - c.seen",
    ] {
        assert_eq!(sorted(key), NONE, "{key}");
    }
    for key in ["c.flag", "c.local", "c.note", "c"] {
        assert_eq!(sorted(key), ["type.ordering_operand"], "{key}");
    }
    assert_eq!(
        rules(
            "function f(crates: List<Crate>) -> Crate? {\n    return list.max_by(crates, c -> c.flag)\n}\n"
        ),
        ["type.ordering_operand"]
    );
    assert_eq!(
        rules(
            "function f(crates: List<Crate>) -> Crate? {\n    return list.min_by(crates, c -> c.local)\n}\n"
        ),
        ["type.ordering_operand"]
    );
    // `group_by` groups by `==`, so its key needs no order.
    assert_eq!(
        rules(
            "function f(crates: List<Crate>) -> Map<Flag, List<Crate>> {\n    return list.group_by(crates, c -> c.flag)\n}\n"
        ),
        NONE
    );
}

#[test]
fn collection_calls_move_to_the_namespace_their_first_value_needs() {
    let moved = |source: &str| -> Vec<(String, Vec<String>)> {
        diagnostics(source)
            .iter()
            .map(|d| {
                let edits = d
                    .next
                    .iter()
                    .flat_map(|next| next.edits.iter())
                    .map(|edit| edit.replacement.clone())
                    .collect();
                (d.rule.id.to_string(), edits)
            })
            .collect()
    };
    let rule = "semantic.standard_operation_moved".to_string();
    // Checked as the `list` operation once moved: the count proves `Weight`.
    assert_eq!(
        moved(
            "function f(crates: List<Crate>) -> Weight {\n    return Weight(collection.count(crates))\n}\n"
        ),
        [(rule.clone(), vec!["list.count".to_string()])]
    );
    assert_eq!(
        moved(
            "function f(crates: Crates) -> List<Crate> {\n    return collection.filter(crates, c -> c.weight > 1)\n}\n"
        ),
        [(rule.clone(), vec!["list.filter".to_string()])]
    );
    // A set's and a map's operations come with chunk 5.5c.
    assert_eq!(
        moved(
            "function f(labels: Set<Label>) -> Flag {\n    return Flag(collection.contains(labels, \"a\"))\n}\n"
        ),
        [(rule.clone(), vec!["set.contains".to_string()])]
    );
    assert_eq!(
        moved(
            "function f(slots: Map<Label, Slot>) -> Weight {\n    return Weight(collection.count(slots))\n}\n"
        ),
        [(rule.clone(), vec!["map.count".to_string()])]
    );
    // A value that isn't a name or a field is taken as a list.
    assert_eq!(
        moved("function f() -> Weight {\n    return Weight(collection.count([1, 2]))\n}\n"),
        [(rule, vec!["list.count".to_string()])]
    );
    // `collection` stays reserved, and `list` is too.
    assert_eq!(
        rules("function f(list: Weight) -> Weight {\n    return list\n}\n"),
        ["semantic.standard_namespace_reserved"]
    );
}
