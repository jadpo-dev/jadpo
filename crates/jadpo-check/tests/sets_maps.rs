//! Area 19, "Sets", "Maps" and "Ranges" (chunk 5.5c): every `set`, `map`
//! and `range` operation checks, results keep their items' named type,
//! `for` walks a set and a map's keys but not a map, `[]` is an empty map
//! where a map is expected, and `range` is a reserved namespace.

use crate::support;

use jadpo_db::{Database, Layout};
use jadpo_diag::Diagnostic;

const NAMES: &str = "type Label = Text {\n    min_length: 1\n}\n\ntype Weight = Int {\n    min: 0\n}\n\ntype Flag = Bool {}\n\ntype Moment = Instant {}\n\ntype Day = CalendarDate {}\n\ntype Tags = Set<Label> {\n    min_length: 1\n}\n\ntype Shelves = Map<Label, Weight> {\n    min_length: 1\n}\n\n";

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
fn every_set_operation_checks() {
    let source = "function counted(tags: Set<Label>) -> Weight {\n    return Weight(set.count(tags))\n}\n\nfunction has(tags: Set<Label>, tag: Label) -> Flag {\n    return Flag(set.contains(tags, tag) and set.contains(tags, \"a\"))\n}\n\nfunction changed(tags: Set<Label>, tag: Label) -> Set<Label> {\n    return set.without(set.with(tags, tag), \"b\")\n}\n\nfunction combined(a: Set<Label>, b: Set<Label>) -> List<Set<Label>> {\n    return [set.union(a, b), set.intersection(a, b), set.difference(a, b)]\n}\n";
    assert_eq!(rules(source), NONE);
}

#[test]
fn every_map_operation_checks() {
    let source = "function counted(shelves: Map<Label, Weight>) -> Weight {\n    return Weight(map.count(shelves))\n}\n\nfunction read(shelves: Map<Label, Weight>, at: Label) -> Weight? {\n    return map.get(shelves, at)\n}\n\nfunction has(shelves: Map<Label, Weight>) -> Flag {\n    return Flag(map.has(shelves, \"a\"))\n}\n\nfunction keys(shelves: Map<Label, Weight>) -> Set<Label> {\n    return map.keys(shelves)\n}\n\nfunction values(shelves: Map<Label, Weight>) -> List<Weight> {\n    return map.values(shelves)\n}\n\nfunction changed(shelves: Map<Label, Weight>, other: Map<Label, Weight>) -> Map<Label, Weight> {\n    return map.merge(map.without(map.with(shelves, \"a\", 3), \"b\"), other)\n}\n";
    assert_eq!(rules(source), NONE);
}

#[test]
fn every_range_operation_checks() {
    let source = "function inside(open: InstantRange, at: Moment) -> Flag {\n    return Flag(range.contains(open, at))\n}\n\nfunction clash(a: InstantRange, b: InstantRange) -> Flag {\n    return Flag(range.overlaps(a, b))\n}\n\nfunction shared(a: InstantRange, b: InstantRange) -> InstantRange? {\n    return range.intersection(a, b)\n}\n";
    assert_eq!(rules(source), NONE);
    // A range holds instants, not dates.
    assert_eq!(
        rules(
            "function f(open: InstantRange, day: Day) -> Flag {\n    return Flag(range.contains(open, day))\n}\n"
        ),
        ["type.mismatch"]
    );
    // What may not overlap may be `none`.
    assert_eq!(
        rules(
            "function f(a: InstantRange, b: InstantRange) -> InstantRange {\n    return range.intersection(a, b)\n}\n"
        ),
        ["type.mismatch"]
    );
}

#[test]
fn results_keep_their_items_named_type_but_not_the_collection_s() {
    // An item or a key is the collection's own item type.
    assert_eq!(
        rules(
            "function f(shelves: Shelves) -> Set<Label> {\n    return map.keys(shelves)\n}\n\nfunction g(tags: Tags, other: Tags) -> Set<Label> {\n    return set.union(tags, other)\n}\n"
        ),
        NONE
    );
    // A named collection's rules can break (`without` may leave none), so
    // the result is a plain collection, built back by construction.
    assert_eq!(
        rules("function f(tags: Tags) -> Tags {\n    return set.without(tags, \"a\")\n}\n"),
        ["type.mismatch"]
    );
    assert_eq!(
        rules(
            "function f(shelves: Shelves) -> Shelves {\n    return map.without(shelves, \"a\")\n}\n"
        ),
        ["type.mismatch"]
    );
    // A value that may be missing isn't the value.
    assert_eq!(
        rules(
            "function f(shelves: Map<Label, Weight>) -> Weight {\n    return map.get(shelves, \"a\")\n}\n"
        ),
        ["type.mismatch"]
    );
    // An item of another type isn't added.
    assert_eq!(
        rules(
            "function f(tags: Set<Label>, weight: Weight) -> Set<Label> {\n    return set.with(tags, weight)\n}\n"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        rules(
            "function f(shelves: Map<Label, Weight>, other: Map<Weight, Weight>) -> Map<Label, Weight> {\n    return map.merge(shelves, other)\n}\n"
        ),
        ["type.mismatch"]
    );
    // A list isn't a set.
    assert_eq!(
        rules("function f(tags: List<Label>) -> Weight {\n    return Weight(set.count(tags))\n}\n"),
        ["type.mismatch"]
    );
}

#[test]
fn for_walks_a_set_and_a_map_s_keys() {
    assert_eq!(
        rules(
            "function f(tags: Set<Label>) -> List<Label> {\n    var mut out: List<Label> = []\n    for tag in tags {\n        out = list.append(out, tag)\n    }\n    return out\n}\n\nfunction g(shelves: Shelves) -> List<Label> {\n    var mut out: List<Label> = []\n    for label in map.keys(shelves) {\n        out = list.append(out, label)\n    }\n    return out\n}\n\nfunction h(tags: Tags) -> Flag {\n    for tag in tags {\n        return Flag(tag == \"a\")\n    }\n    return false\n}\n"
        ),
        NONE
    );
    // A map itself isn't walked: the edit loops over its keys.
    let found = diagnostics(
        "function f(shelves: Map<Label, Weight>) -> List<Label> {\n    var mut out: List<Label> = []\n    for label in shelves {\n        out = list.append(out, label)\n    }\n    return out\n}\n",
    );
    let rules: Vec<&str> = found.iter().map(|d| d.rule.id).collect();
    assert_eq!(rules, ["type.mismatch"], "{found:#?}");
    let edits: Vec<String> = found[0]
        .next
        .iter()
        .flat_map(|next| next.edits.iter())
        .map(|edit| edit.replacement.clone())
        .collect();
    assert_eq!(edits, ["map.keys(shelves)"]);
}

#[test]
fn an_empty_list_is_an_empty_map_where_a_map_is_expected() {
    assert_eq!(
        rules(
            "function f() -> Map<Label, Weight> {\n    var none_yet: Map<Label, Weight> = []\n    return map.with(none_yet, \"a\", 1)\n}\n\nfunction g() -> Weight {\n    return Weight(map.count([]))\n}\n"
        ),
        NONE
    );
    // Entries aren't written as a list.
    assert_eq!(
        rules("function f() -> Map<Label, Weight> {\n    return [\"a\"]\n}\n"),
        ["type.mismatch"]
    );
    // An empty map breaks a named map's `min_length`.
    assert_eq!(
        rules("function f() -> Shelves {\n    return []\n}\n"),
        ["type.invalid_literal"]
    );
}

#[test]
fn collection_calls_on_sets_and_maps_are_checked_as_what_they_moved_to() {
    let found = diagnostics(
        "function f(tags: Set<Label>) -> Flag {\n    return Flag(collection.contains(tags, 3))\n}\n",
    );
    let rules: Vec<&str> = found.iter().map(|d| d.rule.id).collect();
    // Moved, then checked as `set.contains`: `3` isn't a `Label`.
    assert_eq!(
        rules,
        ["semantic.standard_operation_moved", "type.mismatch"],
        "{found:#?}"
    );
}

#[test]
fn range_is_a_reserved_namespace() {
    assert_eq!(
        rules(
            "function f(range: InstantRange, at: Moment) -> Flag {\n    return Flag(at > range.start)\n}\n"
        ),
        ["semantic.standard_namespace_reserved"]
    );
    assert_eq!(
        rules("function f(set: Set<Label>) -> Set<Label> {\n    return set\n}\n"),
        ["semantic.standard_namespace_reserved"]
    );
    // A field or a named argument keeps the word.
    assert_eq!(
        rules(
            "type Opening = Object {\n    range: InstantRange\n}\n\nfunction f(opening: Opening, at: Moment) -> Flag {\n    return Flag(range.contains(range: opening.range, value: at))\n}\n"
        ),
        NONE
    );
}

#[test]
fn an_instant_range_is_built_from_its_two_instants() {
    assert_eq!(
        rules(
            "function f(open: InstantRange) -> InstantRange {\n    return InstantRange {\n        start: open.start\n        end: \"2026-10-26T12:00:00Z\"\n    }\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function f(open: InstantRange) -> InstantRange {\n    return InstantRange {\n        start: open.start\n    }\n}\n"
        ),
        ["type.missing_field"]
    );
    assert_eq!(
        rules(
            "function f(day: Day, open: InstantRange) -> InstantRange {\n    return InstantRange {\n        start: day\n        end: open.end\n    }\n}\n"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        rules(
            "function f(open: InstantRange) -> InstantRange {\n    return InstantRange {\n        start: open.start\n        end: open.end\n        middle: open.end\n    }\n}\n"
        ),
        ["type.unknown_field"]
    );
}
