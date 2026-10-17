//! Area 19's "Lists" example, in the runtime: the values the page's test
//! (`project=list-operations`) asserts, given to each operation's one
//! implementation as compiled code calls it (`jadpo_value::run`, which the
//! interpreter calls too), with each inline function a field read. The
//! interpreter's half is that page project, run by `jadpo test`.

use std::rc::Rc;

use jadpo_std::Op;
use jadpo_value::{Fault, Value, run};

fn batch(name: &str, grams: i64, shelf: i64, price: &str) -> Value {
    Value::record(
        "Batch",
        vec![
            (Rc::from("name"), Value::text(name)),
            (Rc::from("grams"), Value::Int(grams)),
            (Rc::from("shelf"), Value::Int(shelf)),
            (Rc::from("price"), decimal(price)),
        ],
    )
}

fn decimal(text: &str) -> Value {
    Value::decimal(text).unwrap_or_else(|| panic!("{text} is a decimal"))
}

/// Field `name` of a batch: what `batch -> batch.<name>` gives.
fn field(item: &Value, name: &str) -> Option<Value> {
    match item {
        Value::Record(record) => record
            .fields
            .iter()
            .find(|(field, _)| &**field == name)
            .map(|(_, value)| value.clone()),
        _ => None,
    }
}

/// Runs `op` on `args`, its inline function `each`, which takes one item.
fn call(op: Op, args: &[Value], each: &mut dyn FnMut(&Value) -> Option<Value>) -> Value {
    run(op, args, None, &mut |given| match given {
        [item] => each(item),
        _ => None,
    })
    .unwrap_or_else(|fault| panic!("{op:?}: {fault:?}"))
}

fn plain(op: Op, args: &[Value]) -> Value {
    call(op, args, &mut |_| None)
}

fn by(op: Op, items: &Value, key: &str) -> Value {
    call(op, std::slice::from_ref(items), &mut |item| {
        field(item, key)
    })
}

fn test(op: Op, items: &Value, check: impl Fn(&Value) -> bool) -> Value {
    call(op, std::slice::from_ref(items), &mut |item| {
        Some(Value::Bool(check(item)))
    })
}

fn ints(items: &[i64]) -> Value {
    Value::list(items.iter().map(|n| Value::Int(*n)).collect())
}

#[test]
fn the_list_operations_give_the_page_s_values() {
    let rye = batch("Rye", 800, 2, "4.20");
    let bap = batch("Bap", 90, 1, "0.60");
    let spelt = batch("Spelt", 800, 2, "3.80");
    let day = Value::list(vec![rye.clone(), bap.clone(), spelt.clone()]);
    let list = |items: &[&Value]| Value::list(items.iter().map(|item| (*item).clone()).collect());
    let weights = ints(&[800, 90, 800]);
    let n = Value::Int;

    // `flour_needed` and `takings`: a sum from the zero the checker gives.
    let grams = by(Op::ListMap, &day, "grams");
    assert_eq!(plain(Op::ListSum, &[grams, n(0)]), n(1690));
    let prices = by(Op::ListMap, &day, "price");
    assert_eq!(plain(Op::ListSum, &[prices, decimal("0")]), decimal("8.60"));
    assert_eq!(
        plain(Op::ListSum, &[Value::list(Vec::new()), decimal("0")]),
        decimal("0.00")
    );

    let shelf = |item: &Value, on: i64| field(item, "shelf") == Some(n(on));
    assert_eq!(
        test(Op::ListAny, &day, |item| shelf(item, 1)),
        Value::Bool(true)
    );
    assert_eq!(
        test(Op::ListAll, &day, |item| field(item, "grams")
            .is_some_and(|grams| grams > n(100))),
        Value::Bool(false)
    );
    assert_eq!(test(Op::ListFind, &day, |item| shelf(item, 2)), rye);
    assert_eq!(plain(Op::ListFirst, std::slice::from_ref(&day)), rye);
    assert_eq!(plain(Op::ListLast, std::slice::from_ref(&day)), spelt);
    assert_eq!(plain(Op::ListAt, &[day.clone(), n(1)]), bap);
    assert_eq!(plain(Op::ListAt, &[day.clone(), n(3)]), Value::None);
    assert_eq!(
        by(Op::ListMaxBy, &day, "grams"),
        rye,
        "the first of equal keys"
    );
    assert_eq!(by(Op::ListMinBy, &day, "price"), bap);
    assert_eq!(
        by(Op::ListSortBy, &day, "name"),
        list(&[&bap, &rye, &spelt])
    );
    assert_eq!(
        by(Op::ListSortBy, &day, "grams"),
        list(&[&bap, &rye, &spelt]),
        "equal keys keep their order"
    );
    assert_eq!(
        plain(Op::ListReverse, std::slice::from_ref(&day)),
        list(&[&spelt, &bap, &rye])
    );
    assert_eq!(plain(Op::ListDistinct, &[weights]), ints(&[800, 90]));
    assert_eq!(
        plain(Op::ListSlice, &[day.clone(), n(1), n(5)]),
        list(&[&bap, &spelt])
    );
    assert_eq!(
        plain(Op::ListKeepFirst, &[day.clone(), n(2)]),
        list(&[&rye, &bap])
    );
    assert_eq!(
        plain(Op::ListKeepLast, &[day.clone(), n(2)]),
        list(&[&bap, &spelt])
    );
    assert_eq!(
        plain(Op::ListDropFirst, &[day.clone(), n(1)]),
        list(&[&bap, &spelt])
    );
    assert_eq!(plain(Op::ListDropLast, &[day.clone(), n(5)]), list(&[]));
    assert_eq!(
        plain(Op::ListAppend, &[list(&[&rye]), bap.clone()]),
        list(&[&rye, &bap])
    );
    assert_eq!(
        plain(Op::ListConcat, &[list(&[&rye]), list(&[&bap, &spelt])]),
        day
    );
    assert_eq!(
        plain(
            Op::ListFlatten,
            &[Value::list(vec![
                list(&[&rye]),
                list(&[]),
                list(&[&bap, &spelt])
            ])]
        ),
        day
    );
    assert_eq!(plain(Op::ListCount, std::slice::from_ref(&day)), n(3));
    assert_eq!(
        plain(Op::ListContains, &[day.clone(), bap.clone()]),
        Value::Bool(true)
    );

    // `by_shelf`: each shelf with its batches in baking order.
    let Value::Map(shelves) = by(Op::ListGroupBy, &day, "shelf") else {
        panic!("a map");
    };
    let shelves: Vec<(Value, Value)> = shelves
        .iter()
        .map(|(shelf, batches)| (shelf.clone(), batches.clone()))
        .collect();
    assert_eq!(
        shelves,
        vec![(n(1), list(&[&bap])), (n(2), list(&[&rye, &spelt]))]
    );
}

/// `over_plan`: `zip_with` gives its two-parameter function an item from
/// each list, as long as the shorter (chunk 5.5c2).
#[test]
fn zip_with_pairs_items_by_position() {
    let day = Value::list(vec![
        batch("Rye", 800, 2, "4.20"),
        batch("Bap", 90, 1, "0.60"),
        batch("Spelt", 800, 2, "3.80"),
    ]);
    let over = |planned: &[i64]| {
        run(
            Op::ListZipWith,
            &[day.clone(), ints(planned)],
            None,
            &mut |given| match given {
                [batch, Value::Int(plan)] => {
                    Some(Value::Bool(field(batch, "grams")? > Value::Int(*plan)))
                }
                _ => None,
            },
        )
        .unwrap_or_else(|fault| panic!("{fault:?}"))
    };
    let yes = Value::Bool(true);
    let no = Value::Bool(false);
    assert_eq!(
        over(&[700, 100, 800]),
        Value::list(vec![yes.clone(), no.clone(), no])
    );
    assert_eq!(
        over(&[700]),
        Value::list(vec![yes]),
        "as long as the shorter"
    );
    assert_eq!(
        run(Op::ListZipWith, &[day, ints(&[1])], None, &mut |_| None),
        Err(Fault::Stopped)
    );
}

#[test]
fn an_inline_function_that_stops_stops_the_operation() {
    let day = ints(&[1, 2]);
    for op in [
        Op::ListSortBy,
        Op::ListMinBy,
        Op::ListGroupBy,
        Op::ListAny,
        Op::ListFind,
    ] {
        assert_eq!(
            run(op, std::slice::from_ref(&day), None, &mut |_| None),
            Err(Fault::Stopped),
            "{op:?}"
        );
    }
    // A negative count is a shape the checker refuses, never a count from
    // the other end.
    assert_eq!(
        run(Op::ListKeepLast, &[day, Value::Int(-1)], None, &mut |_| {
            None
        }),
        Err(Fault::Shape)
    );
}
