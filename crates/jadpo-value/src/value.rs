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

use jadpo_std::{Constant, Dur, Num, Rounding};

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

/// A `ZonedTime` (area 10): an instant and the zone it is seen in. Two are
/// equal when both are, so the same instant seen in two zones is two
/// values; its local date and time come from the zone database
/// ([`crate::temporal`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Zoned {
    /// Milliseconds since 1970-01-01T00:00:00Z.
    pub millis: i64,
    /// The zone, as its place among `Zone`'s variants ([`jadpo_std::ZONES`]).
    pub zone: u32,
}

impl Zoned {
    /// The zone's IANA name, `Europe/London`.
    pub fn zone_name(self) -> &'static str {
        jadpo_std::ZONES
            .get(self.zone as usize)
            .map_or("UTC", |(_, name)| *name)
    }
}

impl fmt::Display for Zoned {
    /// Its local time with its offset and zone, as RFC 9557 writes it:
    /// `2026-10-25T09:30:00+01:00[Europe/London]`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match crate::temporal::local(*self) {
            Some((local, offset)) => {
                let date = jadpo_std::format_date(local.date);
                let clock = jadpo_std::format_clock_time(local.clock);
                let clock = if clock.len() == 5 {
                    format!("{clock}:00")
                } else {
                    clock
                };
                let sign = if offset < 0 { '-' } else { '+' };
                let minutes = offset.unsigned_abs() / 60_000;
                write!(
                    f,
                    "{date}T{clock}{sign}{:02}:{:02}[{}]",
                    minutes / 60,
                    minutes % 60,
                    self.zone_name()
                )
            }
            None => write!(
                f,
                "{}[{}]",
                jadpo_std::format_instant(self.millis),
                self.zone_name()
            ),
        }
    }
}

/// An amount of money (area 19, "Money and payments"): a whole number of
/// its currency's minor units, and the currency. Two are equal when both
/// are, so £5.00 and €5.00 differ; only amounts in one currency are
/// ordered by `<`, while the canonical order puts the currency first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Money {
    /// The currency, as its place among `Currency`'s variants
    /// ([`jadpo_std::CURRENCIES`]).
    pub currency: u32,
    /// Minor units: pence for `GBP`, yen for `JPY`, fils for `BHD`.
    pub minor: i64,
}

impl Money {
    /// The currency's ISO 4217 code: `GBP`.
    pub fn code(self) -> &'static str {
        jadpo_std::money::code(self.currency).unwrap_or("XXX")
    }

    /// How many places the currency's major unit has.
    pub fn exponent(self) -> u32 {
        jadpo_std::money::exponent(self.currency).unwrap_or(2)
    }

    /// The amount in major units, with exactly the currency's places:
    /// `10.50` for 1050 pence, `500` for 500 yen.
    pub fn major(self) -> Decimal {
        Decimal::new(i128::from(self.minor), self.exponent())
    }
}

impl fmt::Display for Money {
    /// Its canonical text (area 19): the code and the amount in major
    /// units with the currency's places, `GBP 10.50`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.code(), self.major())
    }
}

/// A physical quantity, `DataSize` or `Bitrate` (area 19, "Physical
/// quantities"): its kind and its amount in the kind's SI base unit, a
/// mantissa at the kind's scale, so `1km` and `1000metre` are the same
/// value. Only quantities of one kind are ordered by `<`; the canonical
/// order puts the kind first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Quantity {
    pub kind: jadpo_std::QuantityKind,
    pub amount: i128,
}

impl Quantity {
    /// The amount in the base unit as a decimal at the kind's scale.
    pub fn decimal(self) -> Decimal {
        Decimal::new(self.amount, self.kind.scale())
    }
}

/// A runtime value. Named types are erased to what they are built on: a
/// value has already been checked against its type's rules when it was
/// built. A nullable value is the value itself or [`Value::None`].
#[derive(Clone, Debug)]
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
    /// An instant seen in a zone (area 10).
    Zoned(Zoned),
    /// Milliseconds.
    Duration(i64),
    /// An amount of money in a currency (area 19).
    Money(Money),
    /// A physical quantity in its kind's base unit (area 19).
    Quantity(Quantity),
    /// A percentage, as the exact fraction it is: `20%` is 0.20 (area 19).
    Percentage(Decimal),
    List(Rc<[Value]>),
    /// Kept in the canonical order without repeats, so equality is set
    /// equality.
    Set(Rc<[Value]>),
    Map(Rc<BTreeMap<Value, Value>>),
    Record(Rc<Record>),
    Variant(Rc<Variant>),
    /// A secret value (area 19, chunk 5.5r2): a `Password`, an `ApiKey`, a
    /// `BearerToken`, a `TotpSecret`, a `SecretText` or a `Jwt`, written
    /// out only redacted and compared in constant time.
    Secret(crate::secret::Secret),
}

impl PartialEq for Value {
    /// Contents compared, `Bytes` in time that depends on their lengths
    /// alone, wherever they are, so how long comparing two digests, tags or
    /// sealed values takes says nothing of where they differ (chunk 5.5r3);
    /// a secret value compares so too, with another or with text (chunks
    /// 5.5r2 and 5.8a).
    fn eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Bytes(a), Value::Bytes(b)) => crate::secret::constant_time_eq(a, b),
            (Value::Unit, Value::Unit) | (Value::None, Value::None) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b))
            | (Value::Instant(a), Value::Instant(b))
            | (Value::Duration(a), Value::Duration(b)) => a == b,
            (Value::Decimal(a), Value::Decimal(b))
            | (Value::Percentage(a), Value::Percentage(b)) => a == b,
            (Value::Text(a), Value::Text(b)) | (Value::Uuid(a), Value::Uuid(b)) => a == b,
            (Value::Date(a), Value::Date(b)) => a == b,
            (Value::Zoned(a), Value::Zoned(b)) => a == b,
            (Value::Money(a), Value::Money(b)) => a == b,
            (Value::Quantity(a), Value::Quantity(b)) => a == b,
            (Value::List(a), Value::List(b)) | (Value::Set(a), Value::Set(b)) => a == b,
            (Value::Map(a), Value::Map(b)) => a == b,
            (Value::Record(a), Value::Record(b)) => a == b,
            (Value::Variant(a), Value::Variant(b)) => a == b,
            (Value::Secret(a), Value::Secret(b)) => a == b,
            // A secret value against text written in source, in constant
            // time, wherever the two meet: at the top, and inside a list, a
            // set, a map, an object or an enum's data (chunk 5.8a).
            (Value::Secret(secret), Value::Text(text))
            | (Value::Text(text), Value::Secret(secret)) => {
                crate::secret::constant_time_eq(secret.expose().as_bytes(), text.as_bytes())
            }
            _ => false,
        }
    }
}

impl Eq for Value {}

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

    /// A value known while compiling (a literal, a field's default, a
    /// value a plan gives a failure's field), as the runtime holds it.
    pub fn constant(constant: &Constant) -> Value {
        match constant {
            Constant::Unit => Value::Unit,
            Constant::None => Value::None,
            Constant::Bool(value) => Value::Bool(*value),
            Constant::Int(n) => Value::Int(*n),
            Constant::Decimal { mantissa, scale } => {
                Value::Decimal(Decimal::new(*mantissa, *scale))
            }
            Constant::Text(text) => Value::text(text),
            Constant::Uuid(text) => Value::Uuid(Rc::from(text.as_str())),
            Constant::Instant(millis) => Value::Instant(*millis),
            Constant::Date(day) => Value::Date(*day),
            Constant::Duration(millis) => Value::Duration(*millis),
            Constant::Quantity { kind, amount } => Value::Quantity(Quantity {
                kind: *kind,
                amount: *amount,
            }),
            Constant::Percentage { mantissa, scale } => {
                Value::Percentage(Decimal::new(*mantissa, *scale))
            }
            Constant::Variant {
                ty,
                ordinal,
                name,
                fields,
            } => Value::variant(
                ty,
                *ordinal,
                name,
                fields
                    .iter()
                    .map(|(name, field)| (Rc::from(name.as_str()), Value::constant(field)))
                    .collect(),
            ),
            Constant::Secret { kind, text } => Value::secret(kind, &text.0),
            // The checker proved the text is one of the type, so its
            // contract reads it; were it not, the text is kept, as it was
            // written, and the value faults where it is used.
            Constant::Standard { ty, text } => {
                crate::standard::parse(text, ty).unwrap_or_else(|_| Value::text(text))
            }
        }
    }

    /// As the storage codec takes it (`jadpo_std::standard::storage`,
    /// chunk 5.5w3b); `None` for a value no column holds as itself (an
    /// amount of money, a `ZonedTime`, an object other than a geometry, a
    /// map, a secret value),
    /// which `standard::to_parts` turns into its columns first.
    pub fn stored(&self) -> Option<jadpo_std::standard::storage::Stored<'_>> {
        use jadpo_std::standard::storage::Stored;
        Some(match self {
            Value::None => Stored::None,
            Value::Bool(value) => Stored::Bool(*value),
            Value::Int(n) => Stored::Int(*n),
            Value::Decimal(d) => Stored::Decimal {
                mantissa: d.mantissa,
                scale: d.scale,
            },
            Value::Text(text) => Stored::Text(text),
            Value::Uuid(text) => Stored::Uuid(text),
            Value::Bytes(bytes) => Stored::Bytes(bytes),
            Value::Instant(millis) => Stored::Instant(*millis),
            Value::Date(day) => Stored::Date(*day),
            Value::Duration(millis) => Stored::Duration(*millis),
            Value::Quantity(quantity) => Stored::Quantity {
                kind: quantity.kind,
                amount: quantity.amount,
            },
            Value::Percentage(fraction) => Stored::Percentage {
                mantissa: fraction.mantissa,
                scale: fraction.scale,
            },
            Value::Variant(variant) if variant.fields.is_empty() => Stored::Variant {
                ty: &variant.ty,
                name: &variant.name,
            },
            Value::List(items) => Stored::List(
                items
                    .iter()
                    .map(Value::stored)
                    .collect::<Option<Vec<_>>>()?,
            ),
            Value::Set(items) => Stored::Set(
                items
                    .iter()
                    .map(Value::stored)
                    .collect::<Option<Vec<_>>>()?,
            ),
            // A geometry, as PostGIS's `geography` holds it (area 20,
            // chunk 5.5u4); no other object or choice.
            Value::Record(_) | Value::Variant(_) => Stored::Shape(crate::geo::shape(self)?),
            Value::Unit | Value::Zoned(_) | Value::Money(_) | Value::Map(_) | Value::Secret(_) => {
                return None;
            }
        })
    }

    /// `text` held as a secret value of standard type `kind` (chunk
    /// 5.5r2).
    pub fn secret(kind: &'static str, text: &str) -> Value {
        Value::Secret(crate::secret::Secret::new(kind, text))
    }

    /// The items of a list or a set.
    pub fn items(&self) -> Option<&[Value]> {
        match self {
            Value::List(items) | Value::Set(items) => Some(items),
            _ => None,
        }
    }

    /// What kind of value it is, never what it holds: how a fault or an
    /// error names a value it refuses, since the value may be personal or
    /// secret and the text may reach a log (area 4, question 4; chunk
    /// 5.5w2).
    pub fn described(&self) -> String {
        let kind = match self {
            Value::Unit => "`Unit`",
            Value::None => "`none`",
            Value::Bool(_) => "a `Bool`",
            Value::Int(_) => "an `Int`",
            Value::Decimal(_) => "a `Decimal`",
            Value::Text(_) => "text",
            Value::Bytes(_) => "bytes",
            Value::Uuid(_) => "a `Uuid`",
            Value::Instant(_) => "an `Instant`",
            Value::Date(_) => "a `CalendarDate`",
            Value::Zoned(_) => "a `ZonedTime`",
            Value::Duration(_) => "a `Duration`",
            Value::Money(_) => "an amount of money",
            Value::Quantity(_) => "a quantity",
            Value::Percentage(_) => "a `Percentage`",
            Value::List(_) => "a list",
            Value::Set(_) => "a set",
            Value::Map(_) => "a map",
            Value::Record(record) => return format!("a `{}`", record.ty),
            Value::Variant(variant) => return format!("a `{}`", variant.ty),
            Value::Secret(_) => "a secret value",
        };
        kind.to_string()
    }
}

impl fmt::Display for Value {
    /// As Jadpo writes the value: `5.00`, `"Buy milk"`, `[1, 2]`,
    /// `PriceTag { amount: 5.00 }`, `TicketStatus.open`.
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
            Value::Zoned(zoned) => quoted(f, &zoned.to_string()),
            Value::Duration(millis) => write!(
                f,
                "{}",
                Dur {
                    millis: i128::from(*millis)
                }
            ),
            // As code makes one: there is no literal for money (area 19).
            Value::Money(money) => write!(
                f,
                "money.from_minor({}, {}.{})",
                money.minor,
                jadpo_std::CURRENCY,
                jadpo_std::money::variant(money.currency).unwrap_or("?")
            ),
            // As code writes one: its base unit's suffix (area 19).
            Value::Quantity(quantity) => f.write_str(&jadpo_std::quantity::written(
                quantity.kind,
                quantity.amount,
            )),
            // As its literal is written: `20%` (area 19).
            Value::Percentage(fraction) => f.write_str(&jadpo_std::percentage::canonical(
                fraction.mantissa,
                fraction.scale,
            )),
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
            // Redacted: `<redacted Password>` (chunk 5.5r2).
            Value::Secret(secret) => write!(f, "{secret}"),
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
            |amount: i64| Value::record("PriceTag", vec![("amount".into(), Value::Int(amount))]);
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
        let tag = Value::record(
            "PriceTag",
            vec![("amount".into(), Value::Decimal(decimal("5.00")))],
        );
        assert_eq!(tag.to_string(), "PriceTag { amount: 5.00 }");
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
