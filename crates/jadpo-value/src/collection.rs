//! The `collection` namespace (area 2, question 4): `count` and `contains`,
//! and `map` and `filter`, whose function argument is written inline at the
//! call (area 3). Collections are invariant and never changed in place.

use crate::{Fault, Value, ops};

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

/// A new list of the items `keep` accepts, in order; the first item `keep`
/// gives no `Bool` for stops it.
pub fn filter(
    items: &[Value],
    keep: &mut dyn FnMut(&Value) -> Option<Value>,
) -> Result<Vec<Value>, Fault> {
    let mut kept = Vec::new();
    for item in items {
        match keep(item) {
            Some(Value::Bool(true)) => kept.push(item.clone()),
            Some(Value::Bool(false)) => {}
            Some(_) => return Err(Fault::Shape),
            None => return Err(Fault::Stopped),
        }
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_operations_work_on_lists() {
        let items = vec![Value::Int(1), Value::Int(2), Value::Int(3)];
        assert_eq!(count(&items), 3);
        assert!(contains(&items, &Value::Int(2)));
        assert!(!contains(&items, &Value::Int(4)));
        let doubled = map(&items, &mut |item| match item {
            Value::Int(n) => Some(Value::Int(n * 2)),
            _ => None,
        });
        assert_eq!(
            doubled,
            Ok(vec![Value::Int(2), Value::Int(4), Value::Int(6)])
        );
        let odd = filter(&items, &mut |item| {
            Some(Value::Bool(matches!(item, Value::Int(n) if n % 2 == 1)))
        });
        assert_eq!(odd, Ok(vec![Value::Int(1), Value::Int(3)]));
        assert_eq!(map(&items, &mut |_| None), Err(Fault::Stopped));
    }
}
