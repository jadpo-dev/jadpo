//! Area 19's "Sets", "Maps" and "Ranges" examples, in the runtime: the
//! values the pages' tests (`project=set-operations`, `map-operations`,
//! `range-operations` and `list-operations`' `by_shelf`) assert, given to
//! each operation's one implementation as compiled code calls it
//! (`jadpo_value::run`, which the interpreter calls too). The
//! interpreter's half is those page projects, run by `jadpo test`.

use std::collections::BTreeMap;
use std::rc::Rc;

use jadpo_std::Op;
use jadpo_value::{Value, run};

fn call(op: Op, args: &[Value]) -> Value {
    run(op, args, None, &mut |_| None).unwrap_or_else(|fault| panic!("{op:?}: {fault:?}"))
}

fn warning(name: &str) -> Value {
    let ordinal = ["gluten", "milk", "nuts", "sesame"]
        .iter()
        .position(|variant| *variant == name)
        .unwrap_or_else(|| panic!("{name} is a warning"));
    Value::variant("Hazard", ordinal as u32, name, Vec::new())
}

fn warnings(names: &[&str]) -> Value {
    Value::set(names.iter().map(|name| warning(name)).collect())
}

#[test]
fn the_set_operations_give_the_page_s_values() {
    let loaf = warnings(&["sesame", "gluten"]);
    let flapjack = warnings(&["milk"]);
    let yes = Value::Bool(true);
    assert_eq!(
        call(Op::SetCount, std::slice::from_ref(&loaf)),
        Value::Int(2)
    );
    assert_eq!(
        call(Op::SetContains, &[loaf.clone(), warning("sesame")]),
        yes
    );
    assert_eq!(
        call(Op::SetContains, &[flapjack.clone(), warning("nuts")]),
        Value::Bool(false)
    );
    assert_eq!(call(Op::SetWith, &[loaf.clone(), warning("gluten")]), loaf);
    assert_eq!(
        call(Op::SetWith, &[flapjack.clone(), warning("nuts")]),
        warnings(&["milk", "nuts"])
    );
    assert_eq!(
        call(Op::SetWithout, &[loaf.clone(), warning("sesame")]),
        warnings(&["gluten"])
    );
    assert_eq!(
        call(Op::SetUnion, &[loaf.clone(), flapjack]),
        warnings(&["gluten", "milk", "sesame"])
    );
    assert_eq!(
        call(
            Op::SetIntersection,
            &[loaf.clone(), warnings(&["sesame", "nuts"])]
        ),
        warnings(&["sesame"])
    );
    assert_eq!(
        call(Op::SetDifference, &[loaf.clone(), warnings(&["sesame"])]),
        warnings(&["gluten"])
    );
    // A set holds its items in the canonical order, which `for` walks: an
    // enum's in the order its variants are declared.
    assert_eq!(
        loaf.items().map(<[Value]>::to_vec),
        Some(vec![warning("gluten"), warning("sesame")])
    );
}

fn stock(entries: &[(&str, i64)]) -> Value {
    Value::Map(Rc::new(
        entries
            .iter()
            .map(|(loaf, count)| (Value::text(loaf), Value::Int(*count)))
            .collect::<BTreeMap<_, _>>(),
    ))
}

#[test]
fn the_map_operations_give_the_page_s_values() {
    let empty = stock(&[]);
    let shop = stock(&[("Rye", 4), ("Bap", 0)]);
    let rye = Value::text("Rye");
    let bap = Value::text("Bap");
    let spelt = Value::text("Spelt");
    assert_eq!(
        call(Op::MapCount, std::slice::from_ref(&empty)),
        Value::Int(0)
    );
    assert_eq!(
        call(Op::MapCount, std::slice::from_ref(&shop)),
        Value::Int(2)
    );
    assert_eq!(
        call(Op::MapGet, &[shop.clone(), rye.clone()]),
        Value::Int(4)
    );
    assert_eq!(
        call(Op::MapGet, &[shop.clone(), spelt.clone()]),
        Value::None
    );
    assert_eq!(
        call(Op::MapHas, &[shop.clone(), bap.clone()]),
        Value::Bool(true)
    );
    assert_eq!(
        call(Op::MapHas, &[shop.clone(), spelt.clone()]),
        Value::Bool(false)
    );
    assert_eq!(
        call(Op::MapKeys, std::slice::from_ref(&shop)),
        Value::set(vec![bap.clone(), rye.clone()])
    );
    assert_eq!(
        call(Op::MapValues, std::slice::from_ref(&shop)),
        Value::list(vec![Value::Int(0), Value::Int(4)]),
        "in the canonical order of their keys"
    );
    assert_eq!(
        call(Op::MapWith, &[shop.clone(), rye.clone(), Value::Int(6)]),
        stock(&[("Bap", 0), ("Rye", 6)])
    );
    assert_eq!(
        call(Op::MapWith, &[empty, rye.clone(), Value::Int(4)]),
        stock(&[("Rye", 4)])
    );
    assert_eq!(
        call(Op::MapWithout, &[shop.clone(), bap]),
        stock(&[("Rye", 4)])
    );
    assert_eq!(call(Op::MapWithout, &[shop.clone(), spelt]), shop);
    assert_eq!(
        call(Op::MapMerge, &[shop, stock(&[("Rye", 9)])]),
        stock(&[("Bap", 0), ("Rye", 9)])
    );
}

/// `by_shelf`'s map, read as its page test reads it.
#[test]
fn group_by_s_map_is_read_by_key() {
    let batch = |name: &str, shelf: i64| {
        Value::record(
            "Batch",
            vec![
                (Rc::from("name"), Value::text(name)),
                (Rc::from("shelf"), Value::Int(shelf)),
            ],
        )
    };
    let (rye, bap, spelt) = (batch("Rye", 2), batch("Bap", 1), batch("Spelt", 2));
    let day = Value::list(vec![rye.clone(), bap.clone(), spelt.clone()]);
    let shelves = run(Op::ListGroupBy, &[day], None, &mut |given| match given {
        [Value::Record(record)] => record
            .fields
            .iter()
            .find(|(field, _)| &**field == "shelf")
            .map(|(_, shelf)| shelf.clone()),
        _ => None,
    })
    .unwrap_or_else(|fault| panic!("{fault:?}"));
    assert_eq!(
        call(Op::MapKeys, std::slice::from_ref(&shelves)),
        Value::set(vec![Value::Int(1), Value::Int(2)])
    );
    assert_eq!(
        call(Op::MapGet, &[shelves.clone(), Value::Int(2)]),
        Value::list(vec![rye, spelt])
    );
    assert_eq!(
        call(Op::MapGet, &[shelves.clone(), Value::Int(1)]),
        Value::list(vec![bap])
    );
    assert_eq!(call(Op::MapGet, &[shelves, Value::Int(3)]), Value::None);
}

fn instant(at: &str) -> Value {
    Value::Instant(jadpo_std::instant_millis(at).unwrap_or_else(|| panic!("{at} is an instant")))
}

fn slot(start: &str, end: &str) -> Value {
    Value::record(
        jadpo_std::INSTANT_RANGE,
        vec![
            (Rc::from("start"), instant(start)),
            (Rc::from("end"), instant(end)),
        ],
    )
}

#[test]
fn the_range_operations_give_the_page_s_values() {
    let morning = slot("2026-10-26T09:00:00Z", "2026-10-26T12:00:00Z");
    let lunch = slot("2026-10-26T11:00:00Z", "2026-10-26T13:00:00Z");
    let afternoon = slot("2026-10-26T12:00:00Z", "2026-10-26T17:00:00Z");
    let nothing = slot("2026-10-26T10:00:00Z", "2026-10-26T10:00:00Z");
    let (yes, no) = (Value::Bool(true), Value::Bool(false));
    assert_eq!(
        call(
            Op::RangeContains,
            &[morning.clone(), instant("2026-10-26T09:00:00Z")]
        ),
        yes
    );
    assert_eq!(
        call(
            Op::RangeContains,
            &[morning.clone(), instant("2026-10-26T12:00:00Z")]
        ),
        no,
        "the end is excluded"
    );
    assert_eq!(
        call(Op::RangeOverlaps, &[morning.clone(), lunch.clone()]),
        yes
    );
    assert_eq!(
        call(Op::RangeOverlaps, &[morning.clone(), afternoon.clone()]),
        no,
        "they meet at noon"
    );
    assert_eq!(
        call(Op::RangeOverlaps, &[morning.clone(), nothing.clone()]),
        no,
        "an empty range overlaps nothing"
    );
    assert_eq!(
        call(Op::RangeIntersection, &[morning.clone(), lunch]),
        slot("2026-10-26T11:00:00Z", "2026-10-26T12:00:00Z")
    );
    assert_eq!(
        call(Op::RangeIntersection, &[morning.clone(), afternoon]),
        Value::None
    );
    assert_eq!(
        call(Op::RangeIntersection, &[morning, nothing]),
        Value::None
    );
}

/// `on_show` and `restocked`: `map.filter` and `map.map_values` give their
/// two-parameter function each key and its value, in the canonical order
/// of the keys (chunk 5.5c2).
#[test]
fn filter_and_map_values_are_given_each_key_and_value() {
    let shop = stock(&[("Bap", 0), ("Rye", 4), ("Spelt", 2)]);
    let mut seen = Vec::new();
    let shown = run(
        Op::MapFilter,
        std::slice::from_ref(&shop),
        None,
        &mut |given| {
            seen.push(given.to_vec());
            match given {
                [loaf, Value::Int(count)] => {
                    Some(Value::Bool(*count > 0 && *loaf != Value::text("Spelt")))
                }
                _ => None,
            }
        },
    )
    .unwrap_or_else(|fault| panic!("{fault:?}"));
    assert_eq!(shown, stock(&[("Rye", 4)]));
    assert_eq!(
        seen,
        vec![
            vec![Value::text("Bap"), Value::Int(0)],
            vec![Value::text("Rye"), Value::Int(4)],
            vec![Value::text("Spelt"), Value::Int(2)],
        ],
        "in the canonical order of the keys"
    );
    let restocked = run(
        Op::MapMapValues,
        &[stock(&[("Bap", 0), ("Rye", 4)])],
        None,
        &mut |given| match given {
            [_, Value::Int(count)] => Some(Value::Int(count + 6)),
            _ => None,
        },
    )
    .unwrap_or_else(|fault| panic!("{fault:?}"));
    assert_eq!(restocked, stock(&[("Bap", 6), ("Rye", 10)]));
    for op in [Op::MapFilter, Op::MapMapValues] {
        assert_eq!(
            run(op, std::slice::from_ref(&shop), None, &mut |_| None),
            Err(jadpo_value::Fault::Stopped),
            "{op:?}"
        );
    }
}

/// `project=generic-ranges`: a `Range<T>` holds its start and not its end,
/// and an open end holds everything on its side (chunk 5.5c2).
#[test]
fn a_range_of_any_ordered_type_gives_the_page_s_values() {
    let band = |start: Option<i64>, end: Option<i64>| {
        jadpo_value::range::range_of(start.map(Value::Int), end.map(Value::Int))
    };
    let large = band(Some(800), None);
    let medium = band(Some(400), Some(800));
    let small = band(None, Some(500));
    let yes = Value::Bool(true);
    let no = Value::Bool(false);
    assert_eq!(
        call(Op::RangeContains, &[large.clone(), Value::Int(800)]),
        yes
    );
    assert_eq!(
        call(Op::RangeContains, &[medium.clone(), Value::Int(800)]),
        no
    );
    assert_eq!(
        call(Op::RangeIntersection, &[medium.clone(), large.clone()]),
        Value::None
    );
    assert_eq!(
        call(Op::RangeIntersection, &[medium.clone(), small.clone()]),
        band(Some(400), Some(500))
    );
    assert_eq!(call(Op::RangeOverlaps, &[small, medium]), yes);
    assert_eq!(
        jadpo_value::standard::from(&large),
        Ok(Value::text("800/.."))
    );
    assert_eq!(
        jadpo_value::standard::format(
            &band(Some(1200), None),
            &jadpo_value::locale::value(
                jadpo_std::variant_ordinal(jadpo_std::LOCALE, "en_gb").unwrap()
            ),
            &Value::None,
            &Value::None,
            &Default::default(),
            &Value::None
        ),
        Ok(Value::text("1,200 – …"))
    );
    assert_eq!(
        jadpo_value::standard::parse("800/..", "Range<Int>"),
        Ok(large)
    );
}
