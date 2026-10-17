//! The `map` namespace (area 19, "Maps"): each operation once, for both
//! executors. A map keeps its keys in the canonical order (area 2), so
//! `keys`, `values` and a `for` over its keys come out in that order on
//! every target. Values never change: every operation gives a new map.

use std::collections::BTreeMap;

use crate::Value;

/// How many entries a map holds.
pub fn count(entries: &BTreeMap<Value, Value>) -> i64 {
    // A map never holds more than i64::MAX entries on any target.
    i64::try_from(entries.len()).unwrap_or(i64::MAX)
}

/// The value at `key`, or `none`.
pub fn get(entries: &BTreeMap<Value, Value>, key: &Value) -> Value {
    entries.get(key).cloned().unwrap_or(Value::None)
}

/// Whether the map has `key`.
pub fn has(entries: &BTreeMap<Value, Value>, key: &Value) -> bool {
    entries.contains_key(key)
}

/// Its keys, as a set in the canonical order.
pub fn keys(entries: &BTreeMap<Value, Value>) -> Vec<Value> {
    entries.keys().cloned().collect()
}

/// Its values, in the canonical order of their keys.
pub fn values(entries: &BTreeMap<Value, Value>) -> Vec<Value> {
    entries.values().cloned().collect()
}

/// The map with `key` set to `value`, added or replaced.
pub fn with(
    entries: &BTreeMap<Value, Value>,
    key: &Value,
    value: &Value,
) -> BTreeMap<Value, Value> {
    let mut out = entries.clone();
    out.insert(key.clone(), value.clone());
    out
}

/// The map without `key`.
pub fn without(entries: &BTreeMap<Value, Value>, key: &Value) -> BTreeMap<Value, Value> {
    let mut out = entries.clone();
    out.remove(key);
    out
}

/// Both maps' entries; `second`'s value wins where both have a key.
pub fn merge(
    first: &BTreeMap<Value, Value>,
    second: &BTreeMap<Value, Value>,
) -> BTreeMap<Value, Value> {
    let mut out = first.clone();
    out.extend(
        second
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(entries: &[(&str, i64)]) -> BTreeMap<Value, Value> {
        entries
            .iter()
            .map(|(key, value)| (Value::text(key), Value::Int(*value)))
            .collect()
    }

    #[test]
    fn entries_are_read_in_the_canonical_order_of_their_keys() {
        let stock = map(&[("rye", 4), ("bap", 0)]);
        assert_eq!(count(&stock), 2);
        assert_eq!(get(&stock, &Value::text("rye")), Value::Int(4));
        assert_eq!(get(&stock, &Value::text("oat")), Value::None);
        assert!(has(&stock, &Value::text("bap")));
        assert_eq!(keys(&stock), vec![Value::text("bap"), Value::text("rye")]);
        assert_eq!(values(&stock), vec![Value::Int(0), Value::Int(4)]);
    }

    #[test]
    fn maps_are_rebuilt_never_changed() {
        let stock = map(&[("rye", 4)]);
        assert_eq!(
            with(&stock, &Value::text("rye"), &Value::Int(6)),
            map(&[("rye", 6)])
        );
        assert_eq!(without(&stock, &Value::text("rye")), map(&[]));
        assert_eq!(without(&stock, &Value::text("oat")), stock);
        assert_eq!(
            merge(&stock, &map(&[("rye", 9), ("oat", 1)])),
            map(&[("oat", 1), ("rye", 9)])
        );
        assert_eq!(stock, map(&[("rye", 4)]));
    }
}
