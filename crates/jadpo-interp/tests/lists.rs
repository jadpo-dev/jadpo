//! Area 19, "Lists" (chunk 5.5b), run: each `list` operation gives what
//! the page says through the interpreter, with its inline function run
//! per item in the calling frame, a sum of its items' representation even
//! when there are none, and `group_by`'s map in the canonical order of its
//! keys.

use std::rc::Rc;

use crate::support;

use jadpo_interp::{FaultKind, Value};
use support::{decimal, fault, int, list, main, text, value};

const SOURCE: &str = "type Label = Text {\n    min_length: 1\n}\n\ntype Weight = Int {\n    min: 0\n}\n\ntype Amount = Decimal {\n    scale: 2\n    min: 0.00\n}\n\ntype Flag = Bool {}\n\ntype Parcel = Object {\n    label: Label\n    weight: Weight\n}\n\nfunction weighed(weights: List<Weight>) -> Weight {\n    return Weight(list.sum(weights))\n}\n\nfunction priced(amounts: List<Amount>) -> Amount {\n    return Amount(list.sum(amounts))\n}\n\nfunction by_weight(parcels: List<Parcel>) -> Map<Weight, List<Parcel>> {\n    return list.group_by(parcels, p -> p.weight)\n}\n\nfunction by_label(parcels: List<Parcel>) -> List<Parcel> {\n    return list.sort_by(parcels, p -> p.label)\n}\n\nfunction heaviest(parcels: List<Parcel>) -> Parcel? {\n    return list.max_by(parcels, p -> p.weight)\n}\n\nfunction lightest(parcels: List<Parcel>) -> Parcel? {\n    return list.min_by(parcels, p -> p.weight)\n}\n\nfunction found(parcels: List<Parcel>, over: Weight) -> Parcel? {\n    return list.find(parcels, p -> p.weight > over)\n}\n\nfunction any_over(parcels: List<Parcel>, over: Weight) -> Flag {\n    return Flag(list.any(parcels, p -> p.weight > over))\n}\n\nfunction all_over(parcels: List<Parcel>, over: Weight) -> Flag {\n    return Flag(list.all(parcels, p -> p.weight > over))\n}\n\nfunction ends(weights: List<Weight>, n: Weight) -> List<Weight> {\n    return list.concat(list.keep_last(weights, n), list.drop_first(weights, n))\n}\n\nfunction window(weights: List<Weight>, from: Weight, n: Weight) -> List<Weight> {\n    return list.slice(weights, from, n)\n}\n\nfunction trimmed(weights: List<Weight>, n: Weight) -> List<Weight> {\n    return list.append(list.drop_last(list.keep_first(weights, 3), n), 0)\n}\n\nfunction picked(weights: List<Weight>, at: Weight) -> Weight? {\n    return list.at(weights, at)\n}\n\nfunction tidied(groups: List<List<Weight>>) -> List<Weight> {\n    return list.reverse(list.distinct(list.flatten(groups)))\n}\n";

fn parcel(label: &str, weight: i64) -> Value {
    Value::record(
        "Parcel",
        vec![
            (Rc::from("label"), text(label)),
            (Rc::from("weight"), int(weight)),
        ],
    )
}

fn ints(items: &[i64]) -> Value {
    list(items.iter().map(|n| int(*n)).collect())
}

#[test]
fn a_sum_is_its_items_representation_even_for_none() {
    let program = main(SOURCE).with_proof_checks();
    assert_eq!(value(&program, "weighed", vec![ints(&[2, 3])]), int(5));
    assert_eq!(value(&program, "weighed", vec![ints(&[])]), int(0));
    let none = value(&program, "priced", vec![list(Vec::new())]);
    assert!(matches!(none, Value::Decimal(_)), "{none:?}");
    assert_eq!(none.to_string(), "0.00");
    assert_eq!(
        value(
            &program,
            "priced",
            vec![list(vec![decimal("1.25"), decimal("2.50")])]
        )
        .to_string(),
        "3.75"
    );
    // Overflow is a defect, as it is for `+`.
    let overflow = fault(&program, "weighed", vec![ints(&[i64::MAX, 1])]);
    assert_eq!(overflow.kind, FaultKind::IntOverflow);
}

#[test]
fn keys_order_items_and_the_first_of_equal_ones_wins() {
    let program = main(SOURCE);
    let (rye, bap, oat) = (parcel("rye", 800), parcel("bap", 90), parcel("oat", 800));
    let day = list(vec![rye.clone(), bap.clone(), oat.clone()]);
    assert_eq!(value(&program, "heaviest", vec![day.clone()]), rye);
    assert_eq!(value(&program, "lightest", vec![day.clone()]), bap);
    assert_eq!(
        value(&program, "heaviest", vec![list(Vec::new())]),
        Value::None
    );
    assert_eq!(
        value(&program, "by_label", vec![day.clone()]),
        list(vec![bap.clone(), oat.clone(), rye.clone()])
    );
    assert_eq!(value(&program, "found", vec![day.clone(), int(100)]), rye);
    assert_eq!(
        value(&program, "found", vec![day.clone(), int(900)]),
        Value::None
    );
    assert_eq!(
        value(&program, "any_over", vec![day.clone(), int(500)]),
        Value::Bool(true)
    );
    assert_eq!(
        value(&program, "all_over", vec![day.clone(), int(500)]),
        Value::Bool(false)
    );
    assert_eq!(
        value(&program, "all_over", vec![list(Vec::new()), int(500)]),
        Value::Bool(true)
    );
    // Grouped by weight: keys in the canonical order, items in theirs.
    let Value::Map(groups) = value(&program, "by_weight", vec![day]) else {
        panic!("a map");
    };
    let groups: Vec<(Value, Value)> = groups
        .iter()
        .map(|(key, items)| (key.clone(), items.clone()))
        .collect();
    assert_eq!(
        groups,
        vec![(int(90), list(vec![bap])), (int(800), list(vec![rye, oat]))]
    );
}

#[test]
fn positions_and_counts_cut_from_the_start_or_the_end() {
    let program = main(SOURCE).with_proof_checks();
    let weights = ints(&[1, 2, 3, 4]);
    assert_eq!(
        value(&program, "ends", vec![weights.clone(), int(1)]),
        ints(&[4, 2, 3, 4])
    );
    assert_eq!(
        value(&program, "ends", vec![weights.clone(), int(9)]),
        ints(&[1, 2, 3, 4])
    );
    assert_eq!(
        value(&program, "window", vec![weights.clone(), int(1), int(2)]),
        ints(&[2, 3])
    );
    assert_eq!(
        value(&program, "window", vec![weights.clone(), int(3), int(5)]),
        ints(&[4])
    );
    assert_eq!(
        value(&program, "trimmed", vec![weights.clone(), int(1)]),
        ints(&[1, 2, 0])
    );
    assert_eq!(
        value(&program, "picked", vec![weights.clone(), int(0)]),
        int(1)
    );
    assert_eq!(
        value(&program, "picked", vec![weights, int(4)]),
        Value::None
    );
    assert_eq!(
        value(
            &program,
            "tidied",
            vec![list(vec![ints(&[1, 2]), ints(&[]), ints(&[2, 3])])]
        ),
        ints(&[3, 2, 1])
    );
}
