//! `ai.distance` (area 21, "The nearest-neighbour query"; chunk 5.5v1): the
//! distance between two vectors of one type, worked out as pgvector works
//! out the operator its type's `distance:` names
//! ([`jadpo_std::standard::ai::distance`]), and given as the decimal that
//! PostgreSQL writes its `float8` as.

use jadpo_std::VectorDistance;
use jadpo_std::standard::ai;

use crate::ops::Fault;
use crate::{Decimal, Value};

/// The distance from `a` to `b` by `metric`, `VectorDistance`'s variant
/// name the checker fills from their type. A cosine with a vector of zeros
/// is a fault, since such a vector has no direction; a sum past a float's
/// range is an overflow.
pub fn distance(a: &Value, b: &Value, metric: &Value) -> Result<Value, Fault> {
    let metric = match metric {
        Value::Text(name) => VectorDistance::from_name(name).ok_or(Fault::Shape)?,
        _ => return Err(Fault::Shape),
    };
    let (a, b) = (floats(a)?, floats(b)?);
    if a.len() != b.len() {
        return Err(Fault::Shape);
    }
    let Some(found) = ai::distance(&a, &b, metric) else {
        let zero = |items: &[f32]| items.iter().all(|item| *item == 0.0);
        return Err(
            if metric == VectorDistance::Cosine && (zero(&a) || zero(&b)) {
                Fault::ZeroVector
            } else {
                Fault::DecimalOverflow
            },
        );
    };
    Decimal::parse(&ai::distance_text(found))
        .map(Value::Decimal)
        .ok_or(Fault::DecimalOverflow)
}

/// A vector's items as the single-precision floats it keeps: each held
/// decimal is exactly one (chunk 5.5s2).
pub fn floats(vector: &Value) -> Result<Vec<f32>, Fault> {
    let Value::List(items) = vector else {
        return Err(Fault::Shape);
    };
    items
        .iter()
        .map(|item| match item {
            Value::Decimal(decimal) => decimal.to_string().parse::<f32>().map_err(|_| Fault::Shape),
            Value::Int(n) => Ok(*n as f32),
            _ => Err(Fault::Shape),
        })
        .collect()
}

/// A vector of `items`, each the decimal its float is, as a vector keeps
/// it (chunk 5.5s2): what a vector read back from pgvector is.
pub fn from_floats(items: &[f32]) -> Option<Value> {
    items
        .iter()
        .map(|item| Value::decimal(&ai::float(&item.to_string())?))
        .collect::<Option<Vec<Value>>>()
        .map(Value::list)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(items: &[&str]) -> Value {
        Value::list(
            items
                .iter()
                .map(|item| Value::decimal(item).unwrap())
                .collect(),
        )
    }

    #[test]
    fn a_distance_is_the_metric_s_decimal() {
        let a = vector(&["1", "2", "3"]);
        let b = vector(&["4", "-5", "6"]);
        let run = |metric: &str| distance(&a, &b, &Value::text(metric)).map(|v| v.to_string());
        assert_eq!(run("inner_product"), Ok("-12".to_string()));
        assert_eq!(run("euclidean"), Ok("8.18535277187245".to_string()));
        assert_eq!(
            distance(
                &vector(&["0", "0"]),
                &vector(&["1", "0"]),
                &Value::text("cosine")
            ),
            Err(Fault::ZeroVector)
        );
        assert_eq!(
            distance(
                &vector(&["1"]),
                &vector(&["1", "2"]),
                &Value::text("cosine")
            ),
            Err(Fault::Shape)
        );
    }

    #[test]
    fn a_vector_reads_back_from_its_floats_unchanged() {
        let kept = vector(&["0.5", "-0.25", "0.12345679", "1250.5", "0.0000001"]);
        let floats = floats(&kept).unwrap();
        assert_eq!(from_floats(&floats).unwrap().to_string(), kept.to_string());
    }
}
