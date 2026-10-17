//! The `list` namespace (area 19, "Lists"; `collection` until chunk 5.5b):
//! each operation once, for both executors. A function argument is written
//! inline at the call (area 3) and arrives as `each`, which gives no value
//! when the function stopped, so the operation stops too. Values never
//! change (area 2): every operation that gives a list makes a new one, and
//! the items it keeps are the same values, so they keep their named type.
//!
//! Counts and positions are never negative: the checker proves it
//! (`never_negative`), so a negative one here is a shape it would have
//! refused, never a count from the other end.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use crate::{Fault, Value, ops, order};

/// How many items a list holds.
pub fn count(items: &[Value]) -> i64 {
    // A list never holds more than i64::MAX items on any target.
    i64::try_from(items.len()).unwrap_or(i64::MAX)
}

/// Whether a list holds an item equal to `item`, by contents.
pub fn contains(items: &[Value], item: &Value) -> bool {
    items.iter().any(|candidate| ops::equal(candidate, item))
}

/// A new list with `each` applied to every item, in order; the first item
/// `each` gives no value for stops it.
pub fn map(
    items: &[Value],
    each: &mut dyn FnMut(&Value) -> Option<Value>,
) -> Result<Vec<Value>, Fault> {
    items
        .iter()
        .map(|item| each(item).ok_or(Fault::Stopped))
        .collect()
}

/// Whether `test` accepts `item`: a `Bool` it gives, or why it gave none.
fn accepts(test: &mut dyn FnMut(&Value) -> Option<Value>, item: &Value) -> Result<bool, Fault> {
    match test(item) {
        Some(Value::Bool(accepted)) => Ok(accepted),
        Some(_) => Err(Fault::Shape),
        None => Err(Fault::Stopped),
    }
}

/// A new list of the items `keep` accepts, in order; the first item `keep`
/// gives no `Bool` for stops it.
pub fn filter(
    items: &[Value],
    keep: &mut dyn FnMut(&Value) -> Option<Value>,
) -> Result<Vec<Value>, Fault> {
    let mut kept = Vec::new();
    for item in items {
        if accepts(keep, item)? {
            kept.push(item.clone());
        }
    }
    Ok(kept)
}

/// The items added to `zero`, which is `0` of their representation, so an
/// empty list gives the `Int` or the `Decimal` its type says. Overflow is
/// a fault, as it is for `+`.
pub fn sum(items: &[Value], zero: &Value) -> Result<Value, Fault> {
    if !matches!(zero, Value::Int(_) | Value::Decimal(_)) {
        return Err(Fault::Shape);
    }
    let mut total = zero.clone();
    for item in items {
        if !matches!(item, Value::Int(_) | Value::Decimal(_)) {
            return Err(Fault::Shape);
        }
        total = ops::add(&total, item)?;
    }
    Ok(total)
}

/// Whether `test` accepts at least one item, stopping at the first it
/// does; `false` for none.
pub fn any(items: &[Value], test: &mut dyn FnMut(&Value) -> Option<Value>) -> Result<bool, Fault> {
    for item in items {
        if accepts(test, item)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether `test` accepts every item, stopping at the first it doesn't;
/// `true` for none.
pub fn all(items: &[Value], test: &mut dyn FnMut(&Value) -> Option<Value>) -> Result<bool, Fault> {
    for item in items {
        if !accepts(test, item)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The first item `test` accepts, or `none`.
pub fn find(
    items: &[Value],
    test: &mut dyn FnMut(&Value) -> Option<Value>,
) -> Result<Value, Fault> {
    for item in items {
        if accepts(test, item)? {
            return Ok(item.clone());
        }
    }
    Ok(Value::None)
}

/// An item or `none`.
fn maybe(item: Option<&Value>) -> Value {
    item.cloned().unwrap_or(Value::None)
}

/// The first item, or `none` for an empty list.
pub fn first(items: &[Value]) -> Value {
    maybe(items.first())
}

/// The last item, or `none` for an empty list.
pub fn last(items: &[Value]) -> Value {
    maybe(items.last())
}

/// A count or a position as an index; a negative one is a shape the
/// checker refuses.
fn index(n: i64) -> Result<usize, Fault> {
    if n < 0 {
        return Err(Fault::Shape);
    }
    // Past what a list can hold is past its end.
    Ok(usize::try_from(n).unwrap_or(usize::MAX))
}

/// The item at `position`, 0 being the first, or `none` past the end.
pub fn at(items: &[Value], position: i64) -> Result<Value, Fault> {
    Ok(maybe(items.get(index(position)?)))
}

/// Each item with its key, in order.
fn keyed(
    items: &[Value],
    key: &mut dyn FnMut(&Value) -> Option<Value>,
) -> Result<Vec<(Value, Value)>, Fault> {
    items
        .iter()
        .map(|item| Ok((key(item).ok_or(Fault::Stopped)?, item.clone())))
        .collect()
}

/// The item whose key comes first by `wins`, keeping the first of equal
/// ones; `none` for an empty list.
fn best_by(
    items: &[Value],
    key: &mut dyn FnMut(&Value) -> Option<Value>,
    wins: std::cmp::Ordering,
) -> Result<Value, Fault> {
    let mut best: Option<(Value, Value)> = None;
    for (found, item) in keyed(items, key)? {
        let better = match &best {
            Some((held, _)) => order::compare(&found, held) == wins,
            None => true,
        };
        if better {
            best = Some((found, item));
        }
    }
    Ok(best.map_or(Value::None, |(_, item)| item))
}

/// The item with the smallest key in the canonical order; the first of
/// equal ones.
pub fn min_by(
    items: &[Value],
    key: &mut dyn FnMut(&Value) -> Option<Value>,
) -> Result<Value, Fault> {
    best_by(items, key, std::cmp::Ordering::Less)
}

/// The item with the largest key in the canonical order; the first of
/// equal ones.
pub fn max_by(
    items: &[Value],
    key: &mut dyn FnMut(&Value) -> Option<Value>,
) -> Result<Value, Fault> {
    best_by(items, key, std::cmp::Ordering::Greater)
}

/// A new list ordered by `key` ascending, in the canonical order (numbers
/// as numbers, text by code point, times as the moments they are); items
/// with equal keys keep their order, the same on every target.
pub fn sort_by(
    items: &[Value],
    key: &mut dyn FnMut(&Value) -> Option<Value>,
) -> Result<Vec<Value>, Fault> {
    let mut pairs = keyed(items, key)?;
    // `sort_by` is stable.
    pairs.sort_by(|(a, _), (b, _)| order::compare(a, b));
    Ok(pairs.into_iter().map(|(_, item)| item).collect())
}

/// A new list with the last item first.
pub fn reverse(items: &[Value]) -> Vec<Value> {
    items.iter().rev().cloned().collect()
}

/// A new list of each item's first occurrence, by `==`. The canonical
/// order agrees with `==` for values of one type, which a list holds.
pub fn distinct(items: &[Value]) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    items
        .iter()
        .filter(|item| seen.insert((*item).clone()))
        .cloned()
        .collect()
}

/// Up to `count` items from position `from`, cut short at the end.
pub fn slice(items: &[Value], from: i64, count: i64) -> Result<Vec<Value>, Fault> {
    let (from, count) = (index(from)?, index(count)?);
    Ok(items.iter().skip(from).take(count).cloned().collect())
}

/// At most `n` items from the start.
pub fn keep_first(items: &[Value], n: i64) -> Result<Vec<Value>, Fault> {
    slice(items, 0, n)
}

/// At most `n` items from the end.
pub fn keep_last(items: &[Value], n: i64) -> Result<Vec<Value>, Fault> {
    let n = index(n)?;
    Ok(items
        .iter()
        .skip(items.len().saturating_sub(n))
        .cloned()
        .collect())
}

/// The list without its first `n` items; none left when `n` is longer.
pub fn drop_first(items: &[Value], n: i64) -> Result<Vec<Value>, Fault> {
    Ok(items.iter().skip(index(n)?).cloned().collect())
}

/// The list without its last `n` items; none left when `n` is longer.
pub fn drop_last(items: &[Value], n: i64) -> Result<Vec<Value>, Fault> {
    let n = index(n)?;
    Ok(items
        .iter()
        .take(items.len().saturating_sub(n))
        .cloned()
        .collect())
}

/// A new list with `item` added at the end.
pub fn append(items: &[Value], item: &Value) -> Vec<Value> {
    items.iter().chain(std::iter::once(item)).cloned().collect()
}

/// `first`'s items, then `second`'s.
pub fn concat(first: &[Value], second: &[Value]) -> Vec<Value> {
    first.iter().chain(second).cloned().collect()
}

/// The items of each list in turn.
pub fn flatten(lists: &[Value]) -> Result<Vec<Value>, Fault> {
    let mut out = Vec::new();
    for list in lists {
        match list {
            Value::List(items) => out.extend(items.iter().cloned()),
            _ => return Err(Fault::Shape),
        }
    }
    Ok(out)
}

/// Each key `key` gives, with its items in their order; a map keeps its
/// keys in the canonical order (area 2).
pub fn group_by(
    items: &[Value],
    key: &mut dyn FnMut(&Value) -> Option<Value>,
) -> Result<Value, Fault> {
    let mut groups: BTreeMap<Value, Vec<Value>> = BTreeMap::new();
    for (found, item) in keyed(items, key)? {
        groups.entry(found).or_default().push(item);
    }
    let entries = groups
        .into_iter()
        .map(|(found, items)| (found, Value::list(items)))
        .collect();
    Ok(Value::Map(Rc::new(entries)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Decimal;

    fn ints(items: &[i64]) -> Vec<Value> {
        items.iter().map(|n| Value::Int(*n)).collect()
    }

    fn odd(item: &Value) -> Option<Value> {
        Some(Value::Bool(matches!(item, Value::Int(n) if n % 2 == 1)))
    }

    #[test]
    fn the_operations_work_on_lists() {
        let items = ints(&[1, 2, 3]);
        assert_eq!(count(&items), 3);
        assert!(contains(&items, &Value::Int(2)));
        assert!(!contains(&items, &Value::Int(4)));
        let doubled = map(&items, &mut |item| match item {
            Value::Int(n) => Some(Value::Int(n * 2)),
            _ => None,
        });
        assert_eq!(doubled, Ok(ints(&[2, 4, 6])));
        assert_eq!(filter(&items, &mut odd), Ok(ints(&[1, 3])));
        assert_eq!(map(&items, &mut |_| None), Err(Fault::Stopped));
    }

    #[test]
    fn sums_start_from_the_representation_given() {
        assert_eq!(sum(&ints(&[1, 2, 3]), &Value::Int(0)), Ok(Value::Int(6)));
        let zero = Value::Decimal(Decimal::new(0, 0));
        assert_eq!(sum(&[], &zero), Ok(zero.clone()));
        let prices = [
            Value::Decimal(Decimal::new(250, 2)),
            Value::Decimal(Decimal::new(125, 2)),
        ];
        assert_eq!(
            sum(&prices, &zero),
            Ok(Value::Decimal(Decimal::new(375, 2)))
        );
        assert_eq!(
            sum(&ints(&[i64::MAX, 1]), &Value::Int(0)),
            Err(Fault::IntOverflow)
        );
        assert_eq!(sum(&[Value::text("a")], &Value::Int(0)), Err(Fault::Shape));
    }

    #[test]
    fn tests_stop_at_the_first_answer() {
        let items = ints(&[2, 3, 4]);
        assert_eq!(any(&items, &mut odd), Ok(true));
        assert_eq!(all(&items, &mut odd), Ok(false));
        assert_eq!(any(&[], &mut odd), Ok(false));
        assert_eq!(all(&[], &mut odd), Ok(true));
        assert_eq!(find(&items, &mut odd), Ok(Value::Int(3)));
        assert_eq!(find(&ints(&[2]), &mut odd), Ok(Value::None));
        // The item after the answer is never tested.
        let mut seen = 0;
        let found = any(&items, &mut |item| {
            seen += 1;
            odd(item)
        });
        assert_eq!((found, seen), (Ok(true), 2));
        assert_eq!(any(&items, &mut |_| Some(Value::Int(1))), Err(Fault::Shape));
    }

    #[test]
    fn positions_count_from_the_start_only() {
        let items = ints(&[5, 6, 7]);
        assert_eq!(first(&items), Value::Int(5));
        assert_eq!(last(&items), Value::Int(7));
        assert_eq!(first(&[]), Value::None);
        assert_eq!(at(&items, 2), Ok(Value::Int(7)));
        assert_eq!(at(&items, 3), Ok(Value::None));
        assert_eq!(at(&items, -1), Err(Fault::Shape));
        assert_eq!(slice(&items, 1, 5), Ok(ints(&[6, 7])));
        assert_eq!(slice(&items, 4, 1), Ok(Vec::new()));
        assert_eq!(keep_first(&items, 2), Ok(ints(&[5, 6])));
        assert_eq!(keep_last(&items, 2), Ok(ints(&[6, 7])));
        assert_eq!(keep_last(&items, 9), Ok(items.clone()));
        assert_eq!(drop_first(&items, 1), Ok(ints(&[6, 7])));
        assert_eq!(drop_last(&items, 1), Ok(ints(&[5, 6])));
        assert_eq!(drop_last(&items, 9), Ok(Vec::new()));
        assert_eq!(keep_first(&items, i64::MAX), Ok(items.clone()));
        assert_eq!(drop_first(&items, -2), Err(Fault::Shape));
    }

    #[test]
    fn keys_order_items_and_ties_keep_their_order() {
        let words = [
            Value::text("rye"),
            Value::text("spelt"),
            Value::text("oat"),
            Value::text("barley"),
        ];
        let mut length = |item: &Value| match item {
            Value::Text(text) => Some(Value::Int(text.chars().count() as i64)),
            _ => None,
        };
        assert_eq!(min_by(&words, &mut length), Ok(Value::text("rye")));
        assert_eq!(max_by(&words, &mut length), Ok(Value::text("barley")));
        assert_eq!(min_by(&[], &mut length), Ok(Value::None));
        assert_eq!(
            sort_by(&words, &mut length),
            Ok(vec![
                Value::text("rye"),
                Value::text("oat"),
                Value::text("spelt"),
                Value::text("barley"),
            ])
        );
        // Text keys sort by code point: upper case before lower.
        let mut itself = |item: &Value| Some(item.clone());
        assert_eq!(
            sort_by(
                &[Value::text("b"), Value::text("B"), Value::text("a")],
                &mut itself
            ),
            Ok(vec![Value::text("B"), Value::text("a"), Value::text("b")])
        );
        assert_eq!(sort_by(&words, &mut |_| None), Err(Fault::Stopped));
    }

    #[test]
    fn lists_are_rebuilt_never_changed() {
        let items = ints(&[1, 2, 1, 3, 2]);
        assert_eq!(reverse(&items), ints(&[2, 3, 1, 2, 1]));
        assert_eq!(distinct(&items), ints(&[1, 2, 3]));
        assert_eq!(append(&items[..1], &Value::Int(9)), ints(&[1, 9]));
        assert_eq!(concat(&items[..1], &items[3..]), ints(&[1, 3, 2]));
        assert_eq!(
            flatten(&[
                Value::list(ints(&[1, 2])),
                Value::list(Vec::new()),
                Value::list(ints(&[3]))
            ]),
            Ok(ints(&[1, 2, 3]))
        );
        assert_eq!(flatten(&[Value::Int(1)]), Err(Fault::Shape));
    }

    #[test]
    fn groups_keep_their_items_in_order() {
        let grouped = group_by(&ints(&[4, 1, 2, 3]), &mut odd);
        let Ok(Value::Map(entries)) = grouped else {
            panic!("a map: {grouped:?}");
        };
        let found: Vec<(Value, Value)> = entries
            .iter()
            .map(|(key, items)| (key.clone(), items.clone()))
            .collect();
        assert_eq!(
            found,
            vec![
                (Value::Bool(false), Value::list(ints(&[4, 2]))),
                (Value::Bool(true), Value::list(ints(&[1, 3]))),
            ]
        );
    }
}
