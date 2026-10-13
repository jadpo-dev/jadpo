//! The `collection` namespace (area 2, question 4): `count` and `contains`,
//! and `map` and `filter`, whose function argument is written inline at the
//! call (area 3). Collections are invariant and never changed in place.

use crate::Value;

/// How many items a list holds.
pub fn count(items: &[Value]) -> i64 {
    // A list never holds more than i64::MAX items on any target.
    i64::try_from(items.len()).unwrap_or(i64::MAX)
}

/// Whether a list holds an item equal to `item`, by contents.
pub fn contains(items: &[Value], item: &Value) -> bool {
    items.contains(item)
}

/// A new list with `each` applied to every item, in order.
pub fn map(items: &[Value], each: impl FnMut(&Value) -> Value) -> Vec<Value> {
    items.iter().map(each).collect()
}

/// A new list of the items `keep` accepts, in order.
pub fn filter(items: &[Value], mut keep: impl FnMut(&Value) -> bool) -> Vec<Value> {
    items.iter().filter(|item| keep(item)).cloned().collect()
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
        let doubled = map(&items, |item| match item {
            Value::Int(n) => Value::Int(n * 2),
            other => other.clone(),
        });
        assert_eq!(doubled, [Value::Int(2), Value::Int(4), Value::Int(6)]);
        let odd = filter(&items, |item| matches!(item, Value::Int(n) if n % 2 == 1));
        assert_eq!(odd, [Value::Int(1), Value::Int(3)]);
    }
}
