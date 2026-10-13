//! A Jadpo value at runtime. Every value is immutable (area 2), and `==`
//! compares contents: objects and variants field by field, lists in order,
//! sets and maps as sets (area 2, question 12).

use std::cmp::Ordering;
use std::collections::BTreeMap;

/// An exact decimal: `mantissa × 10^-scale` (area 2, question 8).
#[derive(Clone, Copy, Debug)]
pub struct Decimal {
    pub mantissa: i128,
    pub scale: u32,
}

impl Decimal {
    pub fn new(mantissa: i128, scale: u32) -> Decimal {
        Decimal { mantissa, scale }
    }

    /// Both mantissas at the larger scale, or `None` if that overflows.
    fn aligned(self, other: Decimal) -> Option<(i128, i128)> {
        let scale = self.scale.max(other.scale);
        let lift = |d: Decimal| {
            10i128
                .checked_pow(scale - d.scale)
                .and_then(|factor| d.mantissa.checked_mul(factor))
        };
        Some((lift(self)?, lift(other)?))
    }
}

impl PartialEq for Decimal {
    fn eq(&self, other: &Decimal) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Decimal {}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Decimal) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Decimal {
    /// By value: `1.5` equals `1.50`.
    fn cmp(&self, other: &Decimal) -> Ordering {
        match self.aligned(*other) {
            Some((a, b)) => a.cmp(&b),
            // Only reachable with scales far beyond any declared `scale:`.
            None => (self.mantissa.signum(), self.scale).cmp(&(other.mantissa.signum(), other.scale)),
        }
    }
}

/// A runtime value. Named types are erased: a value has already been
/// checked against its type's rules when it was built.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Value {
    Unit,
    None,
    Bool(bool),
    /// An exact 64-bit integer; overflow is a fault, never a wrap.
    Int(i64),
    Decimal(Decimal),
    Text(String),
    Bytes(Vec<u8>),
    /// Normalised to lowercase hexadecimal with hyphens.
    Uuid(String),
    List(Vec<Value>),
    /// Kept sorted and without repeats, so equality is set equality.
    Set(Vec<Value>),
    Map(BTreeMap<Value, Value>),
    /// An object, by field name.
    Record(BTreeMap<String, Value>),
    /// An enum value: the variant's name and, for a payload variant, its
    /// fields.
    Variant(String, BTreeMap<String, Value>),
}

impl Value {
    /// A set from any values: sorted, repeats dropped.
    pub fn set(mut items: Vec<Value>) -> Value {
        items.sort();
        items.dedup();
        Value::Set(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equality_compares_contents() {
        let record = |amount: i64| {
            Value::Record(BTreeMap::from([("amount".to_string(), Value::Int(amount))]))
        };
        assert_eq!(record(5), record(5));
        assert_ne!(record(5), record(6));
        assert_eq!(
            Value::set(vec![Value::Int(2), Value::Int(1), Value::Int(2)]),
            Value::set(vec![Value::Int(1), Value::Int(2)])
        );
        assert_eq!(
            Value::Decimal(Decimal::new(150, 2)),
            Value::Decimal(Decimal::new(15, 1))
        );
    }
}
