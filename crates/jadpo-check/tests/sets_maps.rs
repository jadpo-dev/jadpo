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
        ["failure.attempt_required"],
        "its start may be after the end written (chunk 5.5t2)"
    );
    assert_eq!(
        rules(
            "function f(open: InstantRange) -> InstantRange {\n    return InstantRange {\n        start: open.start\n        end: open.end\n    }\n}\n"
        ),
        ["failure.attempt_required"],
        "the parts of another range prove nothing of themselves"
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

#[test]
fn two_parameter_inline_functions_check_and_keep_named_types() {
    // Each parameter takes the type of the value given in its place, and
    // the result keeps the named type it gives (chunk 5.5c2).
    let source = "function shown(shelves: Map<Label, Weight>) -> Map<Label, Weight> {\n    return map.filter(shelves, (label, weight) -> weight > 0 and label != \"a\")\n}\n\nfunction flagged(shelves: Map<Label, Weight>) -> Map<Label, Flag> {\n    return map.map_values(shelves, (label, weight) -> Flag(weight > 0))\n}\n\nfunction paired(labels: List<Label>, weights: List<Weight>) -> List<Flag> {\n    return list.zip_with(labels, weights, (label, weight) -> Flag(weight > 0 and label != \"b\"))\n}\n";
    assert_eq!(rules(source), NONE);
    // The key is the map's key type, not its value's.
    assert_eq!(
        rules(
            "function f(shelves: Map<Label, Weight>) -> Map<Label, Weight> {\n    return map.filter(shelves, (label, weight) -> label > 0)\n}\n"
        ),
        ["type.ordering_operand"]
    );
    // A named map type's rules can break, so `filter` gives a plain map.
    assert_eq!(
        rules(
            "function f(shelves: Shelves) -> Shelves {\n    return map.filter(shelves, (label, weight) -> weight > 0)\n}\n"
        ),
        ["type.mismatch"]
    );
}

#[test]
fn an_inline_function_has_as_many_parameters_as_its_operation_gives() {
    for source in [
        "function f(shelves: Map<Label, Weight>) -> Map<Label, Weight> {\n    return map.filter(shelves, weight -> weight > 0)\n}\n",
        "function f(labels: List<Label>) -> List<Label> {\n    return list.map(labels, (label, other) -> label)\n}\n",
        "function f(labels: List<Label>, weights: List<Weight>) -> List<Weight> {\n    return list.zip_with(labels, weights, (a, b, c) -> b)\n}\n",
    ] {
        assert_eq!(
            rules(source),
            ["type.inline_function_parameters"],
            "{source}"
        );
    }
    let found = diagnostics(
        "function f(labels: List<Label>) -> List<Label> {\n    return list.map(labels, (label, other) -> label)\n}\n",
    );
    assert_eq!(
        found[0].message,
        "`list.map`'s `each` takes one parameter, not two"
    );
}

#[test]
fn two_parameters_are_only_for_a_standard_operation() {
    for source in [
        "function same(weight: Weight) -> Weight {\n    return weight\n}\n\nfunction f(weight: Weight) -> Weight {\n    return same((a, b) -> a)\n}\n",
        "function f(labels: List<Label>) -> List<Label> {\n    return list.append(labels, (a, b) -> a)\n}\n",
    ] {
        assert_eq!(
            rules(source),
            ["type.inline_function_placement"],
            "{source}"
        );
    }
    // Two parameters can't share a name (area 3, question 3).
    assert_eq!(
        rules(
            "function f(shelves: Map<Label, Weight>) -> Map<Label, Weight> {\n    return map.filter(shelves, (weight, weight) -> weight > 0)\n}\n"
        ),
        ["semantic.duplicate_declaration"]
    );
}

#[test]
fn a_range_of_any_ordered_type_takes_the_range_operations() {
    // `Range<T>` with open ends (chunk 5.5c2): the operations take it as
    // they take an `InstantRange`, and give its own range type.
    let source = "function inside(band: Range<Weight>, at: Weight) -> Flag {\n    return Flag(range.contains(band, at) and range.contains(band, 3))\n}\n\nfunction clash(a: Range<Weight>, b: Range<Weight>) -> Flag {\n    return Flag(range.overlaps(a, b))\n}\n\nfunction shared(a: Range<Weight>, b: Range<Weight>) -> Range<Weight>? {\n    return range.intersection(a, b)\n}\n\nfunction days(a: Range<Day>, b: Range<Day>) -> Range<Day>? {\n    return range.intersection(a, b)\n}\n\nfunction open() -> Range<Weight> {\n    return Range {\n        start: 5\n        end: none\n    }\n}\n\nfunction written(at: Weight) -> Flag {\n    return Flag(range.contains(Range {\n        start: 1\n        end: 9\n    }, at))\n}\n\nfunction first(band: Range<Weight>) -> Weight? {\n    return band.start\n}\n";
    assert_eq!(rules(source), NONE);
    let cases: &[(&str, &[&str])] = &[
        // Its ends are a type `<` orders.
        (
            "function f(band: Range<Flag>) -> Flag {\n    return Flag(range.contains(band, true))\n}\n",
            &["type.ordering_operand"],
        ),
        (
            "function f() -> Flag {\n    return Flag(range.contains(Range {\n        start: true\n        end: none\n    }, true))\n}\n",
            &["type.ordering_operand"],
        ),
        // It takes one type argument.
        (
            "function f(band: Range) -> Flag {\n    return Flag(range.overlaps(band, band))\n}\n",
            &["type.type_argument_count"],
        ),
        // A call's ranges are one range type, each with its own end rule.
        (
            "function f(a: InstantRange, b: Range<Moment>) -> Flag {\n    return Flag(range.overlaps(a, b))\n}\n",
            &["type.mismatch"],
        ),
        // The value is the range's item type.
        (
            "function f(band: Range<Weight>, day: Day) -> Flag {\n    return Flag(range.contains(band, day))\n}\n",
            &["type.mismatch"],
        ),
        // Something that isn't a range.
        (
            "function f(band: List<Weight>, at: Weight) -> Flag {\n    return Flag(range.contains(band, at))\n}\n",
            &["type.mismatch"],
        ),
        // An end may be open, so reading one needs `match`.
        (
            "function f(band: Range<Weight>) -> Weight {\n    return band.end\n}\n",
            &["type.mismatch"],
        ),
        (
            "function f(band: Range<Weight>) -> Weight? {\n    return band.finish\n}\n",
            &["type.unknown_field"],
        ),
        // Both ends are given, `none` for an open one.
        (
            "function f() -> Range<Weight> {\n    return Range {\n        start: 5\n    }\n}\n",
            &["type.missing_field"],
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(&rules(source), expected, "{source}");
    }
}

#[test]
fn a_range_meets_the_contract_through_its_ends() {
    let types = "type Wire = Text {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";
    let cases: &[(&str, &[&str])] = &[
        (
            "function f(band: Range<Weight>) -> Wire {\n    return Wire(text.from(band))\n}\n",
            &[],
        ),
        // Shown as its ends are, with their options; a number takes none.
        (
            "function f(band: Range<Moment>, shown: Locale) -> PresentationText {\n    return text.format(band, locale: shown, zone: Zone.utc, format: TimeFormat.date_medium)\n}\n",
            &[],
        ),
        (
            "function f(band: Range<Moment>, shown: Locale) -> PresentationText {\n    return text.format(band, locale: shown, format: TimeFormat.date_medium)\n}\n",
            &["type.temporal_zone_required"],
        ),
        (
            "function f(band: Range<Weight>, shown: Locale) -> PresentationText {\n    return text.format(band, locale: shown)\n}\n",
            &[],
        ),
        (
            "function f(band: Range<Weight>, shown: Locale) -> PresentationText {\n    return text.format(band, locale: shown, zone: Zone.utc)\n}\n",
            &["type.temporal_option_unused"],
        ),
        // Read end by end as a standard type or a representation.
        (
            "function f(raw: Wire) fails Unreadable -> Range<Int> {\n    var read: Range<Int> = attempt text.parse(raw, as: Range<Int>) {\n        malformed: Unreadable\n        start_not_after_end: Unreadable\n    }\n    return read\n}\n",
            &[],
        ),
        (
            "function f(raw: Wire) fails Unreadable -> Range<Weight> {\n    var read: Range<Weight> = attempt text.parse(raw, as: Range<Weight>) {\n        malformed: Unreadable\n    }\n    return read\n}\n",
            &["type.mismatch"],
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(&rules(&format!("{types}{source}")), expected, "{source}");
    }
}
