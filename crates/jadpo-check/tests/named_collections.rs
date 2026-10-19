//! Named collection construction (area 2, "Named collection construction";
//! chunk 2.11). A named list, set or map type is built from a collection
//! whose items are already its item type, as an ordinary construction that
//! checks only the collection's own rules: `attempt Names(found) {
//! min_length: NoNames }`, with arms, `invalid:` and proof as for any
//! construction. Items are never converted in the same step, and
//! `List<T>(xs)` stays an error.

use crate::support;

use jadpo_db::{Database, Layout};
use support::{project, rules};

const CLEAN: &[String] = &[];

/// Names, the named collections built from them, and failures for their
/// rules.
const TYPES: &str = "type Name = Text {\n    min_length: 1\n}\n\ntype ShortName = Name {\n    max_length: 8\n}\n\ntype Count = Int {}\n\ntype Names = List<Name> {\n    min_length: 1\n}\n\ntype FewNames = Names {\n    max_length: 3\n}\n\ntype Shortlist = List<Name> {\n    max_length: 3\n}\n\ntype Pair = List<Name> {\n    min_length: 2\n}\n\ntype NameSet = Set<Name> {\n    min_length: 1\n    max_length: 3\n}\n\ntype PairSet = Set<Name> {\n    min_length: 2\n}\n\ntype Scores = Map<Name, Count> {\n    min_length: 1\n}\n\ntype Team = Object {\n    members: List<Name> {\n        max_length: 3\n    }\n}\n\ntype NoNames = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype TooMany = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn with_types(source: &str) -> Vec<String> {
    rules(
        Layout::Loose,
        &[("types.jadpo", TYPES), ("main.jadpo", source)],
    )
}

/// Each diagnostic of `source` beside [`TYPES`] as its rule, message,
/// reason and next step.
fn explained(source: &str) -> Vec<[String; 4]> {
    let db = Database::default();
    let project = project(
        &db,
        Layout::Loose,
        &[("types.jadpo", TYPES), ("main.jadpo", source)],
    );
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| {
            [
                d.rule.id.to_string(),
                d.message.clone(),
                d.reason.clone().unwrap_or_default(),
                d.next.as_ref().map(|n| n.text.clone()).unwrap_or_default(),
            ]
        })
        .collect()
}

#[test]
fn a_named_list_is_built_from_a_list_of_its_items() {
    let clean = [
        // The decision's own example: only `min_length` can break.
        "function gather(found: List<Name>) fails NoNames -> Names {\n    var result: Names = attempt Names(found) {\n        min_length: NoNames\n    }\n    return result\n}\n",
        // `invalid:` covers every rule that can break.
        "function few(found: List<Name>) fails NoNames -> FewNames {\n    var result: FewNames = attempt FewNames(found) {\n        invalid: NoNames\n    }\n    return result\n}\n",
        // Each rule its own arm, from the type and the type it builds on.
        "function few(found: List<Name>) fails NoNames, TooMany -> FewNames {\n    var result: FewNames = attempt FewNames(found) {\n        min_length: NoNames\n        max_length: TooMany\n    }\n    return result\n}\n",
        // A list type with no rules can't fail.
        "type Roll = List<Name> {}\n\nfunction roll(found: List<Name>) -> Roll {\n    return Roll(found)\n}\n",
        // A list written in the construction is checked while compiling.
        "function two(a: Name, b: Name) -> Names {\n    return Names([a, b])\n}\n",
        // A field type with collection rules is built the same way.
        "function staff(found: List<Name>) fails TooMany -> Team {\n    var members: Team.members = attempt Team.members(found) {\n        max_length: TooMany\n    }\n    return Team {\n        members: members\n    }\n}\n",
    ];
    for source in clean {
        assert_eq!(with_types(source), CLEAN, "for:\n{source}");
    }
}

#[test]
fn a_construction_that_can_fail_needs_its_arms() {
    assert_eq!(
        with_types("function gather(found: List<Name>) -> Names {\n    return Names(found)\n}\n"),
        ["failure.attempt_required"]
    );
    assert_eq!(
        with_types(
            "function few(found: List<Name>) fails NoNames -> FewNames {\n    var result: FewNames = attempt FewNames(found) {\n        min_length: NoNames\n    }\n    return result\n}\n"
        ),
        ["failure.rule_unmapped"]
    );
    // `Shortlist` has no `min_length` to break.
    assert_eq!(
        with_types(
            "function short(found: List<Name>) fails NoNames, TooMany -> Shortlist {\n    var result: Shortlist = attempt Shortlist(found) {\n        min_length: NoNames\n        max_length: TooMany\n    }\n    return result\n}\n"
        ),
        ["failure.unknown_mapping"]
    );
}

#[test]
fn declared_lengths_and_conditions_prove_a_named_collection() {
    let clean = [
        // A `Pair` holds at least 2, so it is at least 1.
        "function from_pair(pair: Pair) -> Names {\n    return Names(pair)\n}\n",
        // A narrower named collection widens without a construction.
        "function widened(few: FewNames) -> Names {\n    return few\n}\n",
        // ... and a construction of its parent is plain.
        "function rebuilt(few: FewNames) -> Names {\n    return Names(few)\n}\n",
        // A sibling: `Shortlist` proves `max_length`, so only `min_length`
        // can break.
        "function from_short(short: Shortlist) fails NoNames -> FewNames {\n    var result: FewNames = attempt FewNames(short) {\n        min_length: NoNames\n    }\n    return result\n}\n",
        // A condition on `list.count` proves the lengths.
        "function checked(found: List<Name>) fails NoNames -> Names {\n    if (list.count(found) == 0) {\n        reject NoNames\n    }\n    return Names(found)\n}\n",
        "function checked(found: List<Name>) fails NoNames, TooMany -> FewNames {\n    if (list.count(found) < 1) {\n        reject NoNames\n    } else if (list.count(found) > 3) {\n        reject TooMany\n    }\n    return FewNames(found)\n}\n",
        "function inside(found: List<Name>) fails NoNames -> FewNames {\n    if (list.count(found) >= 1 and list.count(found) <= 3) {\n        return FewNames(found)\n    }\n    reject NoNames\n}\n",
    ];
    for source in clean {
        assert_eq!(with_types(source), CLEAN, "for:\n{source}");
    }
    // A proven rule takes no arm, and a proven construction no `attempt`.
    assert_eq!(
        with_types(
            "function from_pair(pair: Pair) fails NoNames -> Names {\n    var result: Names = attempt Names(pair) {\n        min_length: NoNames\n    }\n    return result\n}\n"
        ),
        ["failure.attempt_infallible"]
    );
    assert_eq!(
        with_types(
            "function checked(found: List<Name>) fails NoNames, TooMany -> FewNames {\n    if (list.count(found) > 3) {\n        reject TooMany\n    }\n    var result: FewNames = attempt FewNames(found) {\n        min_length: NoNames\n        max_length: TooMany\n    }\n    return result\n}\n"
        ),
        ["failure.rule_cannot_break"]
    );
    // A check that proves the wrong end proves nothing.
    assert_eq!(
        with_types(
            "function wrong(found: List<Name>) fails TooMany -> Names {\n    if (list.count(found) > 3) {\n        reject TooMany\n    }\n    return Names(found)\n}\n"
        ),
        ["failure.attempt_required"]
    );
}

#[test]
fn a_named_set_is_built_from_a_set_or_a_list_of_its_items() {
    let clean = [
        "function from_set(found: Set<Name>) fails NoNames, TooMany -> NameSet {\n    var result: NameSet = attempt NameSet(found) {\n        min_length: NoNames\n        max_length: TooMany\n    }\n    return result\n}\n",
        // A list's repeats collapse; it holds at most as many as the list,
        // so `max_length` follows from the list's own.
        "function from_short(short: Shortlist) fails NoNames -> NameSet {\n    var result: NameSet = attempt NameSet(short) {\n        min_length: NoNames\n    }\n    return result\n}\n",
        // ... and at least one when the list has any items.
        "function from_names(names: FewNames) -> NameSet {\n    return NameSet(names)\n}\n",
        "function counted(found: List<Name>) fails NoNames, TooMany -> NameSet {\n    if (list.count(found) == 0) {\n        reject NoNames\n    } else if (list.count(found) > 3) {\n        reject TooMany\n    }\n    return NameSet(found)\n}\n",
        // A set's own lengths carry over.
        "function from_pairs(pairs: PairSet) -> PairSet {\n    return PairSet(pairs)\n}\n",
    ];
    for source in clean {
        assert_eq!(with_types(source), CLEAN, "for:\n{source}");
    }
    // Two items may be the same name, so a list proves no more than one
    // distinct item, however long it is.
    assert_eq!(
        with_types("function pairs(pair: Pair) -> PairSet {\n    return PairSet(pair)\n}\n"),
        ["failure.attempt_required"]
    );
    let required = explained(
        "function counted(found: List<Name>) fails NoNames -> PairSet {\n    if (list.count(found) < 2) {\n        reject NoNames\n    }\n    return PairSet(found)\n}\n",
    );
    assert_eq!(required.len(), 1, "{required:#?}");
    assert_eq!(required[0][0], "failure.attempt_required");
}

#[test]
fn a_named_map_is_built_from_a_map_of_its_keys_and_values() {
    assert_eq!(
        with_types(
            "function tally(found: Map<Name, Count>) fails NoNames -> Scores {\n    var result: Scores = attempt Scores(found) {\n        min_length: NoNames\n    }\n    return result\n}\n"
        ),
        CLEAN
    );
    assert_eq!(
        with_types(
            "type Raw = Text {}\n\nfunction tally(found: Map<Raw, Count>) fails NoNames -> Scores {\n    var result: Scores = attempt Scores(found) {\n        min_length: NoNames\n    }\n    return result\n}\n"
        ),
        ["type.constructor_input"]
    );
}

/// A map built in the construction from an empty one, `map.with([], key,
/// value)`, whose types only the literals give: the named map fixes them,
/// so its rules are still checked, and a construction that can break one
/// still needs its arms (found in chunk 5.7, where it was taken as
/// unknown and needed none).
#[test]
fn a_map_built_in_the_construction_is_typed_by_the_named_map() {
    let wrapped = |body: &str| {
        format!(
            "type Labels = Map<Text, Text> {{\n    min_length: 2\n}}\n\nfunction labels() fails NoNames -> Labels {{\n{body}}}\n"
        )
    };
    assert_eq!(
        with_types(&wrapped(
            "    return Labels(map.with([], \"aisle\", \"bread\"))\n"
        )),
        ["failure.attempt_required"]
    );
    assert_eq!(
        with_types(&wrapped(
            "    return attempt Labels(map.with([], \"aisle\", \"bread\")) {\n        min_length: NoNames\n    }\n"
        )),
        CLEAN
    );
}

#[test]
fn items_are_never_converted_in_the_same_step() {
    let found = explained(
        "type Raw = Text {}\n\nfunction gather(found: List<Raw>) fails NoNames -> Names {\n    var result: Names = attempt Names(found) {\n        min_length: NoNames\n    }\n    return result\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    let [rule, message, reason, next] = &found[0];
    assert_eq!(rule, "type.constructor_input");
    assert_eq!(message, "`Names` can't be built from `List<Raw>`");
    assert_eq!(
        reason,
        "`Names` is a list of `Name`, and this is a list of `Raw`. Building a `Names` checks only the list's own rules; it never converts the items."
    );
    assert_eq!(
        next,
        "Build each item as a `Name` where it is made, so the list is a `List<Name>`"
    );
    // Collections stay invariant, so even items that widen don't fit.
    assert_eq!(
        with_types(
            "function gather(found: List<ShortName>) fails NoNames -> Names {\n    var result: Names = attempt Names(found) {\n        min_length: NoNames\n    }\n    return result\n}\n"
        ),
        ["type.constructor_input"]
    );
}

#[test]
fn a_named_collection_is_built_only_from_a_collection_it_can_keep() {
    // A set has no order a list could keep.
    let found = explained(
        "function gather(found: Set<Name>) fails NoNames -> Names {\n    var result: Names = attempt Names(found) {\n        min_length: NoNames\n    }\n    return result\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    let [rule, message, reason, next] = &found[0];
    assert_eq!(rule, "type.constructor_input");
    assert_eq!(message, "`Names` can't be built from `Set<Name>`");
    assert_eq!(
        reason,
        "`Names` is a list of `Name`, and this is a set. A set has no order a list could keep, so a list is built only from a list."
    );
    assert_eq!(
        next,
        "Keep the items in a `List<Name>` where they are collected"
    );
    let cases = [
        // Not a collection at all.
        "function gather(n: Count) fails NoNames -> Names {\n    var result: Names = attempt Names(n) {\n        min_length: NoNames\n    }\n    return result\n}\n",
        // A map is not a list.
        "function gather(m: Map<Name, Count>) fails NoNames -> Names {\n    var result: Names = attempt Names(m) {\n        min_length: NoNames\n    }\n    return result\n}\n",
        // A value that may be `none`.
        "function gather(found: List<Name>?) fails NoNames -> Names {\n    var result: Names = attempt Names(found) {\n        min_length: NoNames\n    }\n    return result\n}\n",
        // A list into a map.
        "function tally(found: List<Name>) fails NoNames -> Scores {\n    var result: Scores = attempt Scores(found) {\n        min_length: NoNames\n    }\n    return result\n}\n",
        // A list's items into a scalar.
        "function counting(found: List<Name>) -> Count {\n    return Count(found)\n}\n",
    ];
    for source in cases {
        assert_eq!(
            with_types(source),
            ["type.constructor_input"],
            "for:\n{source}"
        );
    }
}

#[test]
fn an_unnamed_collection_type_is_still_not_a_constructor() {
    assert_eq!(
        with_types(
            "function gather(found: List<Name>) -> List<Name> {\n    return List<Name>(found)\n}\n"
        ),
        ["type.constructor_input"]
    );
}

#[test]
fn a_written_list_takes_the_named_collections_lengths_while_compiling() {
    assert_eq!(
        with_types("function none_at_all() -> Names {\n    return Names([])\n}\n"),
        ["type.invalid_literal"]
    );
    assert_eq!(
        with_types(
            "function couple(a: Name, b: Name) -> PairSet {\n    return PairSet([a, b])\n}\n"
        ),
        ["type.invalid_literal"]
    );
}

#[test]
fn a_value_that_may_be_none_builds_nothing() {
    // Every construction, not only a collection's: `none` follows no
    // type's rules, so the absent case is handled first.
    let source = "type Quantity = Int {\n    min: 1\n}\n\ntype Order = Object {\n    count: Count {\n        min: 1\n    }\n}\n\nfunction quantity(maybe: Count?) -> Quantity {\n    return Quantity(maybe)\n}\n\nfunction order_count(maybe: Count?) -> Order.count {\n    return Order.count(maybe)\n}\n\nfunction plain(maybe: Count?) -> Count {\n    return Count(maybe)\n}\n";
    let found = explained(source);
    assert_eq!(
        found.iter().map(|d| d[0].as_str()).collect::<Vec<_>>(),
        [
            "type.constructor_input",
            "type.constructor_input",
            "type.constructor_input"
        ],
        "{found:#?}"
    );
    let [_, message, reason, next] = &found[0];
    assert_eq!(message, "`Quantity` can't be built from `Count?`");
    assert_eq!(reason, "`Count?` may be `none`, and a `Quantity` can't be.");
    assert_eq!(
        next,
        "Use `match` to handle `none` before building a `Quantity`"
    );
    // A field type that may be `none` itself takes one.
    assert_eq!(
        with_types(
            "type Note = Object {\n    count: Count?\n}\n\nfunction note_count(maybe: Count?) -> Note.count {\n    return Note.count(maybe)\n}\n"
        ),
        CLEAN
    );
}
