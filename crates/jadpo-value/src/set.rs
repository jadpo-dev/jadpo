//! The `set` namespace (area 19, "Sets"): each operation once, for both
//! executors. A set is held in the canonical order without repeats (area
//! 2, "Set order"), so equality is set equality and `for` walks it in that
//! order on every target. Values never change: every operation gives a new
//! set, and the items it keeps are the same values, so they keep their
//! named type.

use std::cmp::Ordering;

use crate::{Value, order};

/// Where `item` is in a set's canonical order, or where it would go.
fn find(items: &[Value], item: &Value) -> Result<usize, usize> {
    items.binary_search_by(|held| order::compare(held, item))
}

/// How many items a set holds.
pub fn count(items: &[Value]) -> i64 {
    // A set never holds more than i64::MAX items on any target.
    i64::try_from(items.len()).unwrap_or(i64::MAX)
}

/// Whether a set holds `item`.
pub fn contains(items: &[Value], item: &Value) -> bool {
    find(items, item).is_ok()
}

/// The set with `item`; the same items when it is already there.
pub fn with(items: &[Value], item: &Value) -> Vec<Value> {
    let mut out = items.to_vec();
    if let Err(at) = find(items, item) {
        out.insert(at, item.clone());
    }
    out
}

/// The set without `item`.
pub fn without(items: &[Value], item: &Value) -> Vec<Value> {
    let mut out = items.to_vec();
    if let Ok(at) = find(items, item) {
        out.remove(at);
    }
    out
}

/// Two sets walked together in the canonical order, keeping each item
/// `keep` accepts by whether it is in the first, the second or both.
fn merged(a: &[Value], b: &[Value], keep: (bool, bool, bool)) -> Vec<Value> {
    let (only_a, only_b, both) = keep;
    let mut out = Vec::new();
    let (mut left, mut right) = (a.iter().peekable(), b.iter().peekable());
    loop {
        match (left.peek(), right.peek()) {
            (Some(x), Some(y)) => match order::compare(x, y) {
                Ordering::Less => {
                    if only_a {
                        out.push((*x).clone());
                    }
                    left.next();
                }
                Ordering::Greater => {
                    if only_b {
                        out.push((*y).clone());
                    }
                    right.next();
                }
                Ordering::Equal => {
                    if both {
                        out.push((*x).clone());
                    }
                    left.next();
                    right.next();
                }
            },
            (Some(x), None) => {
                if only_a {
                    out.push((*x).clone());
                }
                left.next();
            }
            (None, Some(y)) => {
                if only_b {
                    out.push((*y).clone());
                }
                right.next();
            }
            (None, None) => return out,
        }
    }
}

/// The items in either set.
pub fn union(a: &[Value], b: &[Value]) -> Vec<Value> {
    merged(a, b, (true, true, true))
}

/// The items in both sets.
pub fn intersection(a: &[Value], b: &[Value]) -> Vec<Value> {
    merged(a, b, (false, false, true))
}

/// The items in `a` and not in `b`.
pub fn difference(a: &[Value], b: &[Value]) -> Vec<Value> {
    merged(a, b, (true, false, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[i64]) -> Vec<Value> {
        let Value::Set(items) = Value::set(items.iter().map(|n| Value::Int(*n)).collect()) else {
            unreachable!("a set");
        };
        items.to_vec()
    }

    #[test]
    fn items_are_found_added_and_removed_in_the_canonical_order() {
        let items = set(&[3, 1]);
        assert_eq!(count(&items), 2);
        assert!(contains(&items, &Value::Int(3)));
        assert!(!contains(&items, &Value::Int(2)));
        assert_eq!(with(&items, &Value::Int(2)), set(&[1, 2, 3]));
        assert_eq!(with(&items, &Value::Int(3)), items);
        assert_eq!(without(&items, &Value::Int(1)), set(&[3]));
        assert_eq!(without(&items, &Value::Int(9)), items);
        assert_eq!(count(&[]), 0);
    }

    #[test]
    fn two_sets_combine_in_the_canonical_order() {
        let (a, b) = (set(&[1, 2, 4]), set(&[2, 3, 4, 5]));
        assert_eq!(union(&a, &b), set(&[1, 2, 3, 4, 5]));
        assert_eq!(intersection(&a, &b), set(&[2, 4]));
        assert_eq!(difference(&a, &b), set(&[1]));
        assert_eq!(difference(&b, &a), set(&[3, 5]));
        assert_eq!(union(&a, &[]), a);
        assert_eq!(intersection(&a, &[]), Vec::new());
    }
}
