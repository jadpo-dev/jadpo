//! A Jadpo value at runtime (docs/compiler-runtime.md §2.5). Every value is
//! immutable (area 2), so values are shared behind `Rc` and copying one
//! never copies its contents; `==` compares contents: objects and variants
//! field by field, lists in order, sets and maps as sets (area 2, question
//! 12), and decimals by value, so `1.5` equals `1.50`. `Ord` is the
//! canonical order sets and maps keep ([`crate::order`]).

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use jadpo_std::{Dur, Num, Rounding};

/// An exact decimal: `mantissa × 10^-scale` (area 2, question 8), never
/// more than [`Decimal::MAX_DIGITS`] digits, the most an exact number holds.
#[derive(Clone, Copy, Debug)]
pub struct Decimal {
    pub mantissa: i128,
    pub scale: u32,
}

/// 10^38: the first number with more digits than a decimal holds.
const LIMIT: u128 = 100_000_000_000_000_000_000_000_000_000_000_000_000;

/// How many decimal places past the operands' own a quotient keeps before
/// it is rounded where it is stored (area 3, question 7).
pub const QUOTIENT_PLACES: u32 = 18;

impl Decimal {
    /// The most digits a decimal holds, as a literal may have (area 2).
    pub const MAX_DIGITS: u32 = 38;

    pub fn new(mantissa: i128, scale: u32) -> Decimal {
        Decimal { mantissa, scale }
    }

    pub fn from_int(n: i64) -> Decimal {
        Decimal::new(i128::from(n), 0)
    }

    /// A decimal as written, `12.50`; `None` past 38 digits.
    pub fn parse(text: &str) -> Option<Decimal> {
        let num = Num::parse(text)?;
        bounded(Decimal::new(num.mantissa, num.scale))
    }

    pub fn is_zero(self) -> bool {
        self.mantissa == 0
    }

    /// Both mantissas at the larger scale, or `None` if that overflows.
    fn aligned(self, other: Decimal) -> Option<(i128, i128, u32)> {
        let scale = self.scale.max(other.scale);
        let lift = |d: Decimal| {
            10i128
                .checked_pow(scale - d.scale)
                .and_then(|factor| d.mantissa.checked_mul(factor))
        };
        Some((lift(self)?, lift(other)?, scale))
    }

    /// `self + other` at the larger scale; `None` past 38 digits.
    pub fn checked_add(self, other: Decimal) -> Option<Decimal> {
        let (a, b, scale) = self.aligned(other)?;
        bounded(Decimal::new(a.checked_add(b)?, scale))
    }

    /// `self - other` at the larger scale; `None` past 38 digits.
    pub fn checked_sub(self, other: Decimal) -> Option<Decimal> {
        let (a, b, scale) = self.aligned(other)?;
        bounded(Decimal::new(a.checked_sub(b)?, scale))
    }

    /// `self × other`, exact; places beyond the larger operand's that are
    /// only trailing zeros are dropped. `None` past 38 digits.
    pub fn checked_mul(self, other: Decimal) -> Option<Decimal> {
        let multiplied = Decimal::new(
            self.mantissa.checked_mul(other.mantissa)?,
            self.scale.checked_add(other.scale)?,
        );
        bounded(multiplied.trimmed(self.scale.max(other.scale)))
    }

    pub fn checked_neg(self) -> Option<Decimal> {
        Some(Decimal::new(self.mantissa.checked_neg()?, self.scale))
    }

    /// `self / other` as `/` gives it: exact to [`QUOTIENT_PLACES`] past
    /// the operands' larger scale, rounded half to even there, and rounded
    /// again, half to even, when it is stored in a type with a fixed scale
    /// (area 3, question 7). Trailing zeros past the operands' scale are
    /// dropped, so `10.00 / 4.00` is `2.50`. The scaled dividend is worked
    /// out as wide as it needs, so large operands keep every place. `None`
    /// for a zero divisor, or when the result itself is past 38 digits or
    /// 38 places.
    pub fn quotient(self, other: Decimal) -> Option<Decimal> {
        if other.is_zero() {
            return None;
        }
        let base = self.scale.max(other.scale);
        let scale = base + QUOTIENT_PLACES;
        // a / 10^sa ÷ b / 10^sb, kept at `scale`, is a × 10^(scale - sa + sb) ÷ b.
        let shift = scale + other.scale - self.scale;
        let (mantissa, dropped) = crate::rounding::divide_shifted(
            self.mantissa,
            shift,
            other.mantissa,
            Rounding::HalfEven,
            QUOTIENT_PLACES,
        )?;
        bounded(Decimal::new(mantissa, scale - dropped))
    }

    /// The same value at exactly `scale` places, rounded as `rounding` says
    /// when places are dropped; `None` past 38 digits.
    pub fn rescale(self, scale: u32, rounding: Rounding) -> Option<Decimal> {
        if scale >= self.scale {
            let factor = 10i128.checked_pow(scale - self.scale)?;
            return bounded(Decimal::new(self.mantissa.checked_mul(factor)?, scale));
        }
        let factor = 10i128.checked_pow(self.scale - scale)?;
        let mantissa = crate::rounding::divide(self.mantissa, factor, rounding)?;
        bounded(Decimal::new(mantissa, scale))
    }

    /// Without trailing zeros past `keep` places.
    fn trimmed(mut self, keep: u32) -> Decimal {
        while self.scale > keep && self.mantissa % 10 == 0 {
            self.mantissa /= 10;
            self.scale -= 1;
        }
        self
    }
}

/// The decimal when it has at most 38 digits.
fn bounded(decimal: Decimal) -> Option<Decimal> {
    (decimal.mantissa.unsigned_abs() < LIMIT && decimal.scale <= Decimal::MAX_DIGITS)
        .then_some(decimal)
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
    /// By value: `1.5` equals `1.50`. Exact whatever the scales, through
    /// the comparison the checker's bounds use ([`jadpo_std::compare_exact`]),
    /// so the order is total and agrees with equality.
    fn cmp(&self, other: &Decimal) -> Ordering {
        jadpo_std::compare_exact(self.mantissa, self.scale, other.mantissa, other.scale)
    }
}

impl fmt::Display for Decimal {
    /// As written, with its places: `0.50`, `-3`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Num {
            mantissa: self.mantissa,
            scale: self.scale,
        }
        .fmt(f)
    }
}

/// An object's value: its type's name and its fields in the order the type
/// declares them, so two objects built with their fields written in
/// different orders are equal.
#[derive(Clone, Debug)]
pub struct Record {
    pub ty: Rc<str>,
    pub fields: Vec<(Rc<str>, Value)>,
    /// The optional `Input` fields it was built without, by their place
    /// among its fields. Each reads as `none`, and only an update's
    /// `patch:` tells one left out from one sent as `none` (areas 2 and 6),
    /// so equality ignores it.
    pub omitted: Option<Rc<[u32]>>,
}

impl Record {
    pub fn get(&self, name: &str) -> Option<&Value> {
        field(&self.fields, name)
    }

    /// Whether the field called `name` was given, rather than left out of
    /// an `Input` that let it be (area 6, "patch").
    pub fn supplied(&self, name: &str) -> bool {
        let Some(at) = self.fields.iter().position(|(field, _)| &**field == name) else {
            return false;
        };
        !self
            .omitted
            .as_deref()
            .is_some_and(|omitted| omitted.iter().any(|place| *place as usize == at))
    }
}

impl PartialEq for Record {
    fn eq(&self, other: &Record) -> bool {
        self.ty == other.ty && self.fields == other.fields
    }
}

impl Eq for Record {}

/// An enum's value: the enum's name, the variant's and its place among the
/// enum's variants, and the fields a payload variant carries, in the order
/// it declares them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub ty: Rc<str>,
    /// Where the variant stands in its enum, from 0, as declared: what sets
    /// order variants by (area 2, "Set order").
    pub ordinal: u32,
    pub name: Rc<str>,
    pub fields: Vec<(Rc<str>, Value)>,
}

impl Variant {
    pub fn get(&self, name: &str) -> Option<&Value> {
        field(&self.fields, name)
    }
}

fn field<'a>(fields: &'a [(Rc<str>, Value)], name: &str) -> Option<&'a Value> {
    fields
        .iter()
        .find(|(field, _)| &**field == name)
        .map(|(_, value)| value)
}

/// A runtime value. Named types are erased to what they are built on: a
/// value has already been checked against its type's rules when it was
/// built. A nullable value is the value itself or [`Value::None`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Unit,
    None,
    Bool(bool),
    /// An exact 64-bit integer; overflow is a fault, never a wrap.
    Int(i64),
    Decimal(Decimal),
    Text(Rc<str>),
    Bytes(Rc<[u8]>),
    /// Normalised to lowercase hexadecimal with hyphens.
    Uuid(Rc<str>),
    /// Milliseconds since 1970-01-01T00:00:00Z.
    Instant(i64),
    /// Days since 1970-01-01.
    Date(i32),
    /// Milliseconds since midnight.
    Time(u32),
    /// Milliseconds.
    Duration(i64),
    List(Rc<[Value]>),
    /// Kept in the canonical order without repeats, so equality is set
    /// equality.
    Set(Rc<[Value]>),
    Map(Rc<BTreeMap<Value, Value>>),
    Record(Rc<Record>),
    Variant(Rc<Variant>),
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Value) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Value {
    /// The canonical order (area 2, "Set order").
    fn cmp(&self, other: &Value) -> Ordering {
        crate::order::compare(self, other)
    }
}

impl Value {
    pub fn text(text: &str) -> Value {
        Value::Text(Rc::from(text))
    }

    /// A UUID from its text, normalised to lowercase (area 2, question 13).
    pub fn uuid(text: &str) -> Value {
        Value::Uuid(Rc::from(text.to_ascii_lowercase()))
    }

    /// A decimal as written; `None` past 38 digits.
    pub fn decimal(text: &str) -> Option<Value> {
        Decimal::parse(text).map(Value::Decimal)
    }

    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Rc::from(items))
    }

    /// A set from any values: in the canonical order, repeats dropped.
    pub fn set(mut items: Vec<Value>) -> Value {
        items.sort();
        items.dedup();
        Value::Set(Rc::from(items))
    }

    /// An object of type `ty` with `fields` in its declared order.
    pub fn record(ty: &str, fields: Vec<(Rc<str>, Value)>) -> Value {
        Value::Record(Rc::new(Record {
            ty: Rc::from(ty),
            fields,
            omitted: None,
        }))
    }

    /// Variant `name` of enum `ty`, the `ordinal`th it declares, with a
    /// payload variant's `fields` in their declared order.
    pub fn variant(ty: &str, ordinal: u32, name: &str, fields: Vec<(Rc<str>, Value)>) -> Value {
        Value::Variant(Rc::new(Variant {
            ty: Rc::from(ty),
            ordinal,
            name: Rc::from(name),
            fields,
        }))
    }

    /// The items of a list or a set.
    pub fn items(&self) -> Option<&[Value]> {
        match self {
            Value::List(items) | Value::Set(items) => Some(items),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    /// As Jadpo writes the value: `5.00`, `"Buy milk"`, `[1, 2]`,
    /// `Money { amount: 5.00 }`, `TicketStatus.open`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Unit => f.write_str("Unit"),
            Value::None => f.write_str("none"),
            Value::Bool(value) => write!(f, "{value}"),
            Value::Int(n) => write!(f, "{n}"),
            Value::Decimal(d) => write!(f, "{d}"),
            Value::Text(text) | Value::Uuid(text) => quoted(f, text),
            Value::Bytes(bytes) => write!(f, "<{} bytes>", bytes.len()),
            Value::Instant(millis) => quoted(f, &jadpo_std::format_instant(*millis)),
            Value::Date(day) => quoted(f, &jadpo_std::format_date(*day)),
            Value::Time(millis) => quoted(f, &jadpo_std::format_time(*millis)),
            Value::Duration(millis) => write!(
                f,
                "{}",
                Dur {
                    millis: i128::from(*millis)
                }
            ),
            Value::List(items) | Value::Set(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            Value::Map(entries) => {
                f.write_str("{")?;
                for (i, (key, value)) in entries.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{key}: {value}")?;
                }
                f.write_str("}")
            }
            Value::Record(record) => {
                f.write_str(&record.ty)?;
                fields(f, &record.fields)
            }
            Value::Variant(variant) => {
                write!(f, "{}.{}", variant.ty, variant.name)?;
                if variant.fields.is_empty() {
                    return Ok(());
                }
                fields(f, &variant.fields)
            }
        }
    }
}

fn quoted(f: &mut fmt::Formatter<'_>, text: &str) -> fmt::Result {
    f.write_str("\"")?;
    for c in text.chars() {
        match c {
            '"' => f.write_str("\\\"")?,
            '\\' => f.write_str("\\\\")?,
            '\n' => f.write_str("\\n")?,
            '\r' => f.write_str("\\r")?,
            '\t' => f.write_str("\\t")?,
            c => write!(f, "{c}")?,
        }
    }
    f.write_str("\"")
}

fn fields(f: &mut fmt::Formatter<'_>, fields: &[(Rc<str>, Value)]) -> fmt::Result {
    if fields.is_empty() {
        return f.write_str(" {}");
    }
    f.write_str(" { ")?;
    for (i, (name, value)) in fields.iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        write!(f, "{name}: {value}")?;
    }
    f.write_str(" }")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decimal(text: &str) -> Decimal {
        Decimal::parse(text).expect("a decimal")
    }

    #[test]
    fn equality_compares_contents() {
        let record =
            |amount: i64| Value::record("Money", vec![("amount".into(), Value::Int(amount))]);
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

    #[test]
    fn decimals_stay_exact() {
        assert_eq!(
            decimal("1.25").checked_add(decimal("0.5")),
            Some(decimal("1.75"))
        );
        assert_eq!(
            decimal("1.25")
                .checked_mul(decimal("1.10"))
                .map(|d| d.to_string())
                .as_deref(),
            Some("1.375")
        );
        assert_eq!(
            decimal("10.00")
                .quotient(decimal("4.00"))
                .map(|d| d.to_string())
                .as_deref(),
            Some("2.50")
        );
        let third = decimal("10.00")
            .quotient(decimal("3.00"))
            .expect("a quotient");
        assert_eq!(third.scale, 2 + QUOTIENT_PLACES);
        assert_eq!(third.rescale(2, Rounding::HalfEven), Some(decimal("3.33")));
        assert_eq!(
            decimal("0.125").rescale(2, Rounding::HalfEven),
            Some(decimal("0.12"))
        );
        assert_eq!(
            decimal("0.135").rescale(2, Rounding::HalfEven),
            Some(decimal("0.14"))
        );
        assert_eq!(decimal("1").quotient(decimal("0")), None);
        // Large operands keep all 18 places before the value is stored:
        // fewer would round 333333.3349999... up to 333333.335 first.
        let share = decimal("100000000499999999999.99")
            .quotient(decimal("300000000000000.00"))
            .expect("a quotient");
        assert_eq!(share.scale, 2 + QUOTIENT_PLACES);
        assert_eq!(
            share.rescale(2, Rounding::HalfEven),
            Some(decimal("333333.33"))
        );
        assert_eq!(
            decimal("10000000000000000000000000000000000000")
                .quotient(decimal("1"))
                .map(|d| d.to_string())
                .as_deref(),
            Some("10000000000000000000000000000000000000"),
            "38 digits divided by 1"
        );
        assert_eq!(
            decimal("10000000000000000000000000000000000000").quotient(decimal("0.1")),
            None,
            "39 digits"
        );
        assert_eq!(
            decimal("-1.00")
                .quotient(decimal("8.00"))
                .map(|d| d.to_string())
                .as_deref(),
            Some("-0.125")
        );
        let big = Decimal::new(99_999_999_999_999_999_999_999_999_999_999_999_999, 0);
        assert_eq!(big.checked_add(decimal("1")), None, "39 digits");
        assert_eq!(Decimal::parse(&"9".repeat(39)), None);
    }

    #[test]
    fn decimals_far_apart_in_scale_compare_exactly() {
        let big = decimal("10000000000000000000.00");
        let third = decimal("1.00")
            .quotient(decimal("3.00"))
            .expect("a quotient");
        assert!(big > third, "{big} > {third}");
        assert!(third < big);
        assert_ne!(big, third);
        let set = Value::set(vec![
            Value::Decimal(big),
            Value::Decimal(third),
            Value::Decimal(decimal("10000000000000000000.0")),
        ]);
        assert_eq!(set.items().map(<[Value]>::len), Some(2), "{set}");
    }

    #[test]
    fn values_print_as_jadpo_writes_them() {
        let paid = Value::variant(
            "PaymentOutcome",
            0,
            "paid",
            vec![("receipt".into(), Value::text("r-1"))],
        );
        assert_eq!(paid.to_string(), "PaymentOutcome.paid { receipt: \"r-1\" }");
        assert_eq!(
            Value::variant("TicketStatus", 0, "open", Vec::new()).to_string(),
            "TicketStatus.open"
        );
        let money = Value::record(
            "Money",
            vec![("amount".into(), Value::Decimal(decimal("5.00")))],
        );
        assert_eq!(money.to_string(), "Money { amount: 5.00 }");
        assert_eq!(
            Value::list(vec![Value::Int(1), Value::None]).to_string(),
            "[1, none]"
        );
        assert_eq!(
            Value::text("say \"hi\"\n").to_string(),
            "\"say \\\"hi\\\"\\n\""
        );
        assert_eq!(Value::Duration(900_000).to_string(), "15m");
    }
}
