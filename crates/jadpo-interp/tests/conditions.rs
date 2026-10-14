//! Conditions as proof at runtime (area 2, "Conditions as proof"; chunk
//! 2.6). A construction an `if` condition proves runs no check, as compiled
//! code won't, so every value it gives must already follow its type's
//! rules. Each function below is proved by a different shape of
//! condition; each runs over a spread of inputs, and every value it gives
//! is held to `CakeCount`'s rules here, so a proof the checker got wrong
//! shows up as a value out of range.

mod support;

use jadpo_interp::{Outcome, Program, Value};
use support::{decimal, failure, int, list, main, run, text, value};

const SOURCE: &str = "type Count = Int {}\n\ntype Limit = Count {\n    max: 12\n}\n\ntype CakeCount = Int {\n    min: 1\n    max: 12\n}\n\ntype Stock = Int {\n    min: 0\n    max: 500\n}\n\ntype People = Int {\n    min: 1\n}\n\ntype Flag = Bool {}\n\ntype Tag = Text {}\n\ntype OrderEmpty = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype OrderTooLarge = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction doubled_checked(count: CakeCount) fails OrderTooLarge -> CakeCount {\n    if (count * 2 > 12) {\n        reject OrderTooLarge\n    }\n    return CakeCount(count * 2)\n}\n\nfunction either_end(n: Count) fails OrderTooLarge -> CakeCount {\n    if (n < 1 or n > 12) {\n        reject OrderTooLarge\n    }\n    return CakeCount(n)\n}\n\nfunction inside(n: Count) fails OrderTooLarge -> CakeCount {\n    if (n >= 1 and n <= 12) {\n        return CakeCount(n)\n    }\n    reject OrderTooLarge\n}\n\nfunction chain(n: Count) fails OrderEmpty, OrderTooLarge -> CakeCount {\n    if (n < 1) {\n        reject OrderEmpty\n    } else if (n > 12) {\n        reject OrderTooLarge\n    } else {\n        return CakeCount(n)\n    }\n}\n\nfunction turned(n: Count) fails OrderTooLarge -> CakeCount {\n    if (not (n >= 1 and n <= 12)) {\n        reject OrderTooLarge\n    }\n    return CakeCount(n)\n}\n\nfunction limited(n: Count, limit: Limit) fails OrderEmpty, OrderTooLarge -> CakeCount {\n    if (n < 1) {\n        reject OrderEmpty\n    }\n    if (n > limit) {\n        reject OrderTooLarge\n    }\n    return CakeCount(n)\n}\n\nfunction share(stock: Stock) fails OrderEmpty -> People {\n    if (stock == 0) {\n        reject OrderEmpty\n    }\n    return People(stock)\n}\n\nfunction kept(start: Count) fails OrderTooLarge -> CakeCount {\n    var mut n: Count = start\n    if (n < 1 or n > 12) {\n        reject OrderTooLarge\n    }\n    return CakeCount(n)\n}\n\nfunction is_small(count: CakeCount) -> Flag {\n    return Flag(count < 4)\n}\n\nfunction small(n: Count) -> Flag {\n    if (n >= 1 and n <= 12 and is_small(CakeCount(n))) {\n        return true\n    }\n    return false\n}\n\nfunction counted(items: List<Tag>) fails OrderEmpty, OrderTooLarge -> CakeCount {\n    if (collection.count(items) == 0) {\n        reject OrderEmpty\n    }\n    if (collection.count(items) > 12) {\n        reject OrderTooLarge\n    }\n    return CakeCount(collection.count(items))\n}\n";

/// The whole number a call gives, held to `low..=high`, or `None` when it
/// fails with a declared failure.
fn within(program: &Program, name: &str, args: Vec<Value>, low: i64, high: i64) -> Option<i64> {
    match run(program, name, args.clone()) {
        Outcome::Value(Value::Int(n)) => {
            assert!(
                (low..=high).contains(&n),
                "{name}({args:?}) gave {n}, outside {low}..={high}"
            );
            Some(n)
        }
        Outcome::Failed(_) => None,
        other => panic!("{name}({args:?}) gave {other:?}"),
    }
}

#[test]
fn every_value_a_condition_proves_follows_its_rules() {
    let program = main(SOURCE);
    for n in 1..=12 {
        let doubled = within(&program, "doubled_checked", vec![int(n)], 1, 12);
        assert_eq!(doubled, (n <= 6).then_some(n * 2), "doubled_checked({n})");
    }
    for n in -20..=40 {
        let fits = (1..=12).contains(&n).then_some(n);
        for name in ["either_end", "inside", "chain", "turned", "kept"] {
            assert_eq!(
                within(&program, name, vec![int(n)], 1, 12),
                fits,
                "{name}({n})"
            );
        }
        for limit in [0, 1, 7, 12] {
            let fits = (1..=limit).contains(&n).then_some(n);
            assert_eq!(
                within(&program, "limited", vec![int(n), int(limit)], 1, 12),
                fits,
                "limited({n}, {limit})"
            );
        }
        let small = value(&program, "small", vec![int(n)]);
        assert_eq!(small, Value::Bool((1..4).contains(&n)), "small({n})");
    }
    for stock in 0..=500 {
        let people = within(&program, "share", vec![int(stock)], 1, 500);
        assert_eq!(people, (stock > 0).then_some(stock));
    }
    for size in 0..=14 {
        let items = list((0..size).map(|_| text("tag")).collect());
        let count = within(&program, "counted", vec![items], 1, 12);
        assert_eq!(count, (1..=12).contains(&size).then_some(size));
    }
}

#[test]
fn a_target_with_a_fixed_scale_is_checked_after_rounding() {
    // `Coarse` keeps one decimal place, so 10.07 becomes 10.1, past its
    // maximum: the construction can fail, and its arm says what with.
    let program = main(
        "type Coarse = Decimal {\n    scale: 1\n    max: 10.07\n}\n\ntype Fine = Decimal {\n    scale: 2\n    max: 10.07\n}\n\ntype TooFine = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction coarse(value: Fine) fails TooFine -> Coarse {\n    var kept: Coarse = attempt Coarse(value) {\n        max: TooFine\n    }\n    return kept\n}\n",
    );
    assert_eq!(
        value(&program, "coarse", vec![decimal("10.04")]),
        decimal("10.0")
    );
    let rounded_up = failure(&program, "coarse", vec![decimal("10.07")]);
    assert_eq!(rounded_up.name, "TooFine");
    assert_eq!(rounded_up.broken.as_deref(), Some("`max: 10.07`"));
}
