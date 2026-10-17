//! Area 19, "Sets", "Maps" and "Ranges" (chunk 5.5c), run: each `set`,
//! `map` and `range` operation gives what the page says through the
//! interpreter, `for` walks a set and a map's keys in the canonical order,
//! and `[]` written where a map is expected is an empty map.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::support;

use jadpo_interp::Value;
use support::{int, list, main, text, value};

const SOURCE: &str = "type Label = Text {\n    min_length: 1\n}\n\ntype Weight = Int {\n    min: 0\n}\n\ntype Flag = Bool {}\n\ntype Moment = Instant {}\n\nfunction counted(tags: Set<Label>) -> Weight {\n    return Weight(set.count(tags))\n}\n\nfunction has(tags: Set<Label>, tag: Label) -> Flag {\n    return Flag(set.contains(tags, tag))\n}\n\nfunction with(tags: Set<Label>, tag: Label) -> Set<Label> {\n    return set.with(tags, tag)\n}\n\nfunction without(tags: Set<Label>, tag: Label) -> Set<Label> {\n    return set.without(tags, tag)\n}\n\nfunction combined(a: Set<Label>, b: Set<Label>) -> List<Set<Label>> {\n    return [set.union(a, b), set.intersection(a, b), set.difference(a, b)]\n}\n\nfunction walked(tags: Set<Label>) -> List<Label> {\n    var mut out: List<Label> = []\n    for tag in tags {\n        out = list.append(out, tag)\n    }\n    return out\n}\n\nfunction empty() -> Map<Label, Weight> {\n    return []\n}\n\nfunction stocked(shelves: Map<Label, Weight>, at: Label, weight: Weight) -> Map<Label, Weight> {\n    return map.with(shelves, at, weight)\n}\n\nfunction unstocked(shelves: Map<Label, Weight>, at: Label) -> Map<Label, Weight> {\n    return map.without(shelves, at)\n}\n\nfunction merged(first: Map<Label, Weight>, second: Map<Label, Weight>) -> Map<Label, Weight> {\n    return map.merge(first, second)\n}\n\nfunction read(shelves: Map<Label, Weight>, at: Label) -> Weight? {\n    return map.get(shelves, at)\n}\n\nfunction held(shelves: Map<Label, Weight>, at: Label) -> Flag {\n    return Flag(map.has(shelves, at))\n}\n\nfunction size(shelves: Map<Label, Weight>) -> Weight {\n    return Weight(map.count(shelves))\n}\n\nfunction keys(shelves: Map<Label, Weight>) -> Set<Label> {\n    return map.keys(shelves)\n}\n\nfunction values(shelves: Map<Label, Weight>) -> List<Weight> {\n    return map.values(shelves)\n}\n\nfunction labels(shelves: Map<Label, Weight>) -> List<Label> {\n    var mut out: List<Label> = []\n    for label in map.keys(shelves) {\n        out = list.append(out, label)\n    }\n    return out\n}\n\nfunction inside(open: InstantRange, at: Moment) -> Flag {\n    return Flag(range.contains(open, at))\n}\n\nfunction clash(a: InstantRange, b: InstantRange) -> Flag {\n    return Flag(range.overlaps(a, b))\n}\n\nfunction shared(a: InstantRange, b: InstantRange) -> InstantRange? {\n    return range.intersection(a, b)\n}\n";

fn texts(items: &[&str]) -> Vec<Value> {
    items.iter().map(|item| text(item)).collect()
}

fn set(items: &[&str]) -> Value {
    Value::set(texts(items))
}

fn shelves(entries: &[(&str, i64)]) -> Value {
    Value::Map(Rc::new(
        entries
            .iter()
            .map(|(label, weight)| (text(label), int(*weight)))
            .collect::<BTreeMap<_, _>>(),
    ))
}

fn instant(at: &str) -> Value {
    Value::Instant(jadpo_std::instant_millis(at).unwrap_or_else(|| panic!("{at} is an instant")))
}

fn span(start: &str, end: &str) -> Value {
    Value::record(
        "InstantRange",
        vec![
            (Rc::from("start"), instant(start)),
            (Rc::from("end"), instant(end)),
        ],
    )
}

#[test]
fn set_operations_give_new_sets() {
    let program = main(SOURCE).with_proof_checks();
    let tags = set(&["rye", "bap"]);
    assert_eq!(value(&program, "counted", vec![tags.clone()]), int(2));
    assert_eq!(
        value(&program, "has", vec![tags.clone(), text("rye")]),
        Value::Bool(true)
    );
    assert_eq!(
        value(&program, "has", vec![tags.clone(), text("oat")]),
        Value::Bool(false)
    );
    assert_eq!(
        value(&program, "with", vec![tags.clone(), text("rye")]),
        tags
    );
    assert_eq!(
        value(&program, "with", vec![tags.clone(), text("oat")]),
        set(&["bap", "oat", "rye"])
    );
    assert_eq!(
        value(&program, "without", vec![tags.clone(), text("rye")]),
        set(&["bap"])
    );
    assert_eq!(
        value(&program, "without", vec![tags.clone(), text("oat")]),
        tags
    );
    assert_eq!(
        value(&program, "combined", vec![tags, set(&["rye", "oat"])]),
        list(vec![
            set(&["bap", "oat", "rye"]),
            set(&["rye"]),
            set(&["bap"])
        ])
    );
}

#[test]
fn for_walks_a_set_in_the_canonical_order() {
    let program = main(SOURCE);
    // Text sorts by code point: upper case before lower.
    assert_eq!(
        value(&program, "walked", vec![set(&["rye", "Bap", "oat"])]),
        list(texts(&["Bap", "oat", "rye"]))
    );
    assert_eq!(value(&program, "walked", vec![set(&[])]), list(Vec::new()));
}

#[test]
fn map_operations_give_new_maps() {
    let program = main(SOURCE).with_proof_checks();
    let empty = value(&program, "empty", Vec::new());
    assert_eq!(empty, shelves(&[]));
    let stock = shelves(&[("rye", 4), ("bap", 0)]);
    assert_eq!(
        value(
            &program,
            "stocked",
            vec![empty.clone(), text("rye"), int(4)]
        ),
        shelves(&[("rye", 4)])
    );
    // Added or replaced.
    assert_eq!(
        value(
            &program,
            "stocked",
            vec![stock.clone(), text("rye"), int(6)]
        ),
        shelves(&[("rye", 6), ("bap", 0)])
    );
    assert_eq!(
        value(&program, "unstocked", vec![stock.clone(), text("bap")]),
        shelves(&[("rye", 4)])
    );
    assert_eq!(
        value(&program, "unstocked", vec![stock.clone(), text("oat")]),
        stock
    );
    // The second wins where both have a key.
    assert_eq!(
        value(
            &program,
            "merged",
            vec![stock.clone(), shelves(&[("rye", 9), ("oat", 1)])]
        ),
        shelves(&[("rye", 9), ("bap", 0), ("oat", 1)])
    );
    assert_eq!(
        value(&program, "read", vec![stock.clone(), text("rye")]),
        int(4)
    );
    assert_eq!(
        value(&program, "read", vec![stock.clone(), text("oat")]),
        Value::None
    );
    assert_eq!(
        value(&program, "held", vec![stock.clone(), text("bap")]),
        Value::Bool(true)
    );
    assert_eq!(value(&program, "size", vec![stock.clone()]), int(2));
    assert_eq!(value(&program, "size", vec![empty]), int(0));
    assert_eq!(
        value(&program, "keys", vec![stock.clone()]),
        set(&["bap", "rye"])
    );
    // Values in the canonical order of their keys.
    assert_eq!(
        value(&program, "values", vec![stock.clone()]),
        list(vec![int(0), int(4)])
    );
    assert_eq!(
        value(&program, "labels", vec![stock]),
        list(texts(&["bap", "rye"]))
    );
}

#[test]
fn ranges_include_their_start_and_exclude_their_end() {
    let program = main(SOURCE);
    let morning = span("2026-10-26T09:00:00Z", "2026-10-26T12:00:00Z");
    let lunch = span("2026-10-26T11:00:00Z", "2026-10-26T13:00:00Z");
    let afternoon = span("2026-10-26T12:00:00Z", "2026-10-26T17:00:00Z");
    let nothing = span("2026-10-26T10:00:00Z", "2026-10-26T10:00:00Z");
    let yes = Value::Bool(true);
    let no = Value::Bool(false);
    assert_eq!(
        value(
            &program,
            "inside",
            vec![morning.clone(), instant("2026-10-26T09:00:00Z")]
        ),
        yes
    );
    assert_eq!(
        value(
            &program,
            "inside",
            vec![morning.clone(), instant("2026-10-26T12:00:00Z")]
        ),
        no
    );
    assert_eq!(
        value(
            &program,
            "inside",
            vec![nothing.clone(), instant("2026-10-26T10:00:00Z")]
        ),
        no
    );
    assert_eq!(
        value(&program, "clash", vec![morning.clone(), lunch.clone()]),
        yes
    );
    assert_eq!(
        value(&program, "clash", vec![morning.clone(), afternoon.clone()]),
        no
    );
    assert_eq!(
        value(&program, "clash", vec![morning.clone(), nothing.clone()]),
        no
    );
    assert_eq!(
        value(&program, "shared", vec![morning.clone(), lunch]),
        span("2026-10-26T11:00:00Z", "2026-10-26T12:00:00Z")
    );
    assert_eq!(
        value(&program, "shared", vec![morning.clone(), afternoon]),
        Value::None
    );
    assert_eq!(
        value(&program, "shared", vec![morning, nothing]),
        Value::None
    );
}

/// An `InstantRange` written as an object literal is built from its data
/// entry's fields, as the range operations' examples write one.
#[test]
fn a_standard_object_is_built_from_its_fields() {
    let program = main(
        "function opening() -> InstantRange {\n    return InstantRange {\n        start: \"2026-10-26T11:00:00Z\"\n        end: \"2026-10-26T12:00:00Z\"\n    }\n}\n",
    );
    assert_eq!(
        value(&program, "opening", Vec::new()),
        span("2026-10-26T11:00:00Z", "2026-10-26T12:00:00Z")
    );
}

const TWO: &str = "type Label = Text {\n    min_length: 1\n}\n\ntype Weight = Int {\n    min: 0\n}\n\ntype Flag = Bool {}\n\nfunction shown(shelves: Map<Label, Weight>, hidden: Label) -> Map<Label, Weight> {\n    return map.filter(shelves, (label, weight) -> weight > 0 and label != hidden)\n}\n\nfunction restocked(shelves: Map<Label, Weight>, more: Weight) -> Map<Label, Weight> {\n    return map.map_values(shelves, (label, weight) -> Weight(weight + more))\n}\n\nfunction labelled(shelves: Map<Label, Weight>) -> Map<Label, Label> {\n    return map.map_values(shelves, (label, weight) -> label)\n}\n\nfunction heavier(first: List<Weight>, second: List<Weight>) -> List<Flag> {\n    return list.zip_with(first, second, (a, b) -> Flag(a > b))\n}\n";

#[test]
fn two_parameter_inline_functions_are_given_each_pair() {
    let program = main(TWO).with_proof_checks();
    let stock = shelves(&[("bap", 0), ("rye", 4), ("spelt", 2)]);
    assert_eq!(
        value(&program, "shown", vec![stock.clone(), text("spelt")]),
        shelves(&[("rye", 4)])
    );
    assert_eq!(
        value(&program, "restocked", vec![stock.clone(), int(6)]),
        shelves(&[("bap", 6), ("rye", 10), ("spelt", 8)])
    );
    assert_eq!(
        value(&program, "labelled", vec![stock]),
        Value::Map(Rc::new(
            [("bap", "bap"), ("rye", "rye"), ("spelt", "spelt")]
                .iter()
                .map(|(key, label)| (text(key), text(label)))
                .collect::<BTreeMap<_, _>>(),
        ))
    );
    let ints = |items: &[i64]| list(items.iter().map(|n| int(*n)).collect());
    assert_eq!(
        value(&program, "heavier", vec![ints(&[5, 1, 7]), ints(&[3, 4])]),
        list(vec![Value::Bool(true), Value::Bool(false)]),
        "as long as the shorter"
    );
}

const RANGES: &str = "type Weight = Int {\n    min: 0\n}\n\ntype Flag = Bool {}\n\ntype Wire = Text {}\n\nfunction large() -> Range<Weight> {\n    return Range {\n        start: 800\n        end: none\n    }\n}\n\nfunction inside(band: Range<Weight>, at: Weight) -> Flag {\n    return Flag(range.contains(band, at))\n}\n\nfunction clash(a: Range<Weight>, b: Range<Weight>) -> Flag {\n    return Flag(range.overlaps(a, b))\n}\n\nfunction shared(a: Range<Weight>, b: Range<Weight>) -> Range<Weight>? {\n    return range.intersection(a, b)\n}\n\nfunction lowest(band: Range<Weight>) -> Weight {\n    return match band.start {\n        some(start) -> start\n        none -> 0\n    }\n}\n\nfunction written(band: Range<Weight>) -> Wire {\n    return Wire(text.from(band))\n}\n";

fn band(start: Option<i64>, end: Option<i64>) -> Value {
    let end_of = |end: Option<i64>| end.map_or(Value::None, int);
    Value::record(
        "Range",
        vec![
            (Rc::from("start"), end_of(start)),
            (Rc::from("end"), end_of(end)),
        ],
    )
}

#[test]
fn a_range_of_any_ordered_type_with_open_ends_runs() {
    let program = main(RANGES).with_proof_checks();
    let large = value(&program, "large", Vec::new());
    assert_eq!(large, band(Some(800), None));
    let medium = band(Some(400), Some(800));
    let small = band(None, Some(500));
    let yes = Value::Bool(true);
    let no = Value::Bool(false);
    assert_eq!(
        value(&program, "inside", vec![large.clone(), int(5000)]),
        yes
    );
    assert_eq!(
        value(&program, "inside", vec![medium.clone(), int(800)]),
        no,
        "a range doesn't hold its end"
    );
    assert_eq!(
        value(&program, "clash", vec![medium.clone(), large.clone()]),
        no
    );
    assert_eq!(
        value(&program, "clash", vec![small.clone(), medium.clone()]),
        yes
    );
    assert_eq!(
        value(&program, "shared", vec![medium.clone(), small.clone()]),
        band(Some(400), Some(500))
    );
    assert_eq!(
        value(&program, "shared", vec![medium.clone(), large.clone()]),
        Value::None
    );
    assert_eq!(value(&program, "lowest", vec![small]), int(0));
    assert_eq!(value(&program, "lowest", vec![medium]), int(400));
    assert_eq!(value(&program, "written", vec![large]), text("800/.."));
}
