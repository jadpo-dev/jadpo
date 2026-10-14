//! Values to probe a callable with (chunk 2.7): for each parameter, values
//! its type allows at and near the edges of its rules, so a test that runs
//! every callable with them, proofs checked again
//! ([`crate::Program::with_proof_checks`]), finds a construction the
//! checker proved that a valid argument breaks.

use jadpo_graph::TypeRef;
use jadpo_plan::Types;
use jadpo_std::{Builtin, Num};
use jadpo_value::{Decimal, Validator, Value};

/// Text worth trying: empty, short, each validated built-in type's kind of
/// text and text that is none of them.
const TEXTS: [&str; 9] = [
    "",
    "a",
    "junk",
    "sam@acme.com",
    "a@b.co",
    "https://example.com",
    "10.0.0.1",
    "ABC-1234",
    "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e",
];

/// The values of `ty` to probe with, each one `ty` allows; `None` for a
/// type that isn't a number, text, a UUID or a flag, or a list or set of
/// them, nullable or not.
pub(crate) fn values(types: &Types, ty: &TypeRef) -> Option<Vec<Value>> {
    if let Some(collection) = types.collection(ty) {
        return collections(types, ty, collection);
    }
    let (builtin, chain) = types.chain(ty)?;
    let mut nums: Vec<Num> = Vec::new();
    let mut lengths: Vec<u64> = vec![0, 1, 3];
    for rules in &chain {
        for bound in [rules.min, rules.max].into_iter().flatten() {
            nums.push(bound);
            nums.push(step(bound, 1));
            nums.push(step(bound, -1));
        }
        for length in [rules.min_length, rules.max_length].into_iter().flatten() {
            lengths.extend([length.saturating_sub(1), length, length + 1]);
        }
    }
    let candidates: Vec<Value> = match builtin.representation() {
        Builtin::Bool => vec![Value::Bool(false), Value::Bool(true)],
        Builtin::Int => {
            for n in [0, 1, -1, 2, 6, 7, 12, 13, 100, 1000, -1000] {
                nums.push(Num::whole(n));
            }
            for n in [i64::MAX, i64::MIN, i64::MAX / 2, i64::MIN / 2] {
                nums.push(Num::whole(i128::from(n)));
            }
            nums.iter()
                .filter(|n| n.scale == 0)
                .filter_map(|n| i64::try_from(n.mantissa).ok())
                .map(Value::Int)
                .collect()
        }
        Builtin::Decimal => {
            for text in [
                "0", "1", "-1", "0.5", "0.01", "10.07", "1000000", "-1000000",
            ] {
                nums.extend(Num::parse(text));
            }
            nums.push(Num::whole(10i128.pow(20)));
            nums.iter()
                .map(|n| Value::Decimal(Decimal::new(n.mantissa, n.scale)))
                .collect()
        }
        Builtin::Text | Builtin::Uuid => {
            let mut texts: Vec<String> = TEXTS.iter().map(|t| t.to_string()).collect();
            texts.extend(
                lengths
                    .iter()
                    .filter(|&&n| n <= 4096)
                    .map(|&n| "x".repeat(n as usize)),
            );
            texts
                .into_iter()
                .map(|text| match builtin.representation() {
                    Builtin::Uuid => Value::uuid(&text),
                    _ => Value::text(&text),
                })
                .collect()
        }
        _ => return None,
    };
    let validator = validator(types, ty);
    let mut out: Vec<Value> = Vec::new();
    for candidate in candidates {
        let Some(value) = validator.store(candidate) else {
            continue;
        };
        if validator.check(&value).is_ok() && !out.contains(&value) {
            out.push(value);
        }
    }
    if types.is_nullable(ty) {
        out.push(Value::None);
    }
    Some(out)
}

/// Lists or sets of the item values to probe with, of lengths at and near
/// the edges of `ty`'s `min_length` and `max_length`, repeats included.
fn collections(types: &Types, ty: &TypeRef, collection: &TypeRef) -> Option<Vec<Value>> {
    let (TypeRef::List(item) | TypeRef::Set(item)) = collection else {
        return None;
    };
    let items = values(types, item)?;
    if items.is_empty() {
        return Some(Vec::new());
    }
    let mut lengths: Vec<u64> = vec![0, 1, 2, 3, 13];
    let validator = validator(types, ty);
    for rules in types.length_rules(ty) {
        for length in [rules.min_length, rules.max_length].into_iter().flatten() {
            lengths.extend([length.saturating_sub(1), length, length + 1]);
        }
    }
    let set = matches!(collection, TypeRef::Set(_));
    let mut out: Vec<Value> = Vec::new();
    for length in lengths.into_iter().filter(|&n| n <= 64) {
        // Distinct items where there are enough, then the first again.
        let picked: Vec<Value> = (0..length as usize)
            .map(|i| items[i % items.len()].clone())
            .collect();
        let value = if set {
            Value::set(picked)
        } else {
            Value::list(picked)
        };
        if validator.check(&value).is_ok() && !out.contains(&value) {
            out.push(value);
        }
    }
    if types.is_nullable(ty) {
        out.push(Value::None);
    }
    Some(out)
}

/// `ty`'s rules ready to run.
fn validator(types: &Types, ty: &TypeRef) -> Validator {
    let plan = types.validator_plan(ty);
    Validator::new(plan.builtin, plan.rules)
}

/// `n` moved by one unit in its last place: the value just inside or just
/// outside a bound.
fn step(n: Num, by: i128) -> Num {
    Num {
        mantissa: n.mantissa.saturating_add(by),
        scale: n.scale,
    }
}

/// Sets of arguments taking one value per parameter from `choices`, every
/// combination when there are at most `limit`, otherwise `limit` spread
/// across them.
pub(crate) fn combine(choices: &[Vec<Value>], limit: usize) -> Vec<Vec<Value>> {
    let total = choices
        .iter()
        .try_fold(1usize, |total, values| total.checked_mul(values.len()))
        .unwrap_or(usize::MAX);
    let count = total.min(limit);
    // A stride coprime with nothing in particular spreads the picks; every
    // parameter still sees each of its values while `count` allows.
    let stride = if total > limit {
        total / limit.max(1)
    } else {
        1
    };
    (0..count)
        .map(|i| {
            let mut index = i.saturating_mul(stride);
            choices
                .iter()
                .map(|values| {
                    let value = values[index % values.len()].clone();
                    index /= values.len();
                    value
                })
                .collect()
        })
        .collect()
}
