//! The standard type contract's operations (area 19, "Every standard type
//! meets one contract"): `text.from`, `text.parse` and `text.format`, each
//! one implementation that reads a type's data entry
//! (`jadpo_std::standard`), so a type added later is an entry, not a new
//! code path. `text.from` and `text.parse` also take the representations
//! JSON carries as text (area 19, "Text"): an `Int`'s digits, a `Decimal`
//! with its places, `true` or `false`, and, for `text.parse`, text itself;
//! `text.format` shows an `Int` or a `Decimal` in the locale's digits and
//! separators (area 19, "Numbers").

use jadpo_std::RuleKey;
use jadpo_std::standard::{FormatKey, Holds, StandardType, TextForm, standard_type};

use crate::{Decimal, Fault, Value, Zoned, present, temporal};

/// What fails when text isn't the type asked for at all.
const MALFORMED: Fault = Fault::Broke(RuleKey::Malformed);

/// The standard type a value is, from what the runtime holds; `None` for
/// text, whose standard types (`Email`, `Url`, `IpAddress`) are their text,
/// and for representations.
pub fn entry_of(value: &Value) -> Option<&'static StandardType> {
    let builtin = match value {
        Value::Uuid(_) => jadpo_std::Builtin::Uuid,
        Value::Instant(_) => jadpo_std::Builtin::Instant,
        Value::Date(_) => jadpo_std::Builtin::CalendarDate,
        Value::Zoned(_) => jadpo_std::Builtin::ZonedTime,
        Value::Duration(_) => jadpo_std::Builtin::Duration,
        Value::Money(_) => jadpo_std::Builtin::Money,
        Value::Quantity(quantity) => jadpo_std::Builtin::Quantity(quantity.kind),
        Value::Variant(variant) => return standard_type(&variant.ty),
        Value::Record(record) => return standard_type(&record.ty),
        _ => return None,
    };
    jadpo_std::standard::of_builtin(builtin)
}

/// `text.from(value, form:)`: the value's canonical text, or its code in
/// the code system `form` names (a `CodeForm` variant, or `none` for the
/// type's own), which the checker has made sure the type has data for.
pub fn from_in(value: &Value, form: &Value) -> Result<Value, Fault> {
    let Some(form) = code_form(form)? else {
        return from(value);
    };
    let Value::Variant(variant) = value else {
        return Err(Fault::Shape);
    };
    let code = standard_type(&variant.ty)
        .and_then(|entry| entry.form_codes(form))
        .and_then(|codes| codes.get(variant.ordinal as usize))
        .ok_or(Fault::Shape)?;
    Ok(Value::text(code))
}

/// The code system a `form:` argument names, `None` for `none`.
fn code_form(form: &Value) -> Result<Option<jadpo_std::CodeForm>, Fault> {
    match form {
        Value::None => Ok(None),
        Value::Variant(variant) if &*variant.ty == jadpo_std::CodeForm::NAME => {
            jadpo_std::CodeForm::from_name(&variant.name)
                .map(Some)
                .ok_or(Fault::Shape)
        }
        _ => Err(Fault::Shape),
    }
}

/// `text.from(value)`: the value's canonical text.
pub fn from(value: &Value) -> Result<Value, Fault> {
    let text = match (entry_of(value), value) {
        (Some(entry), value) => canonical(entry, value)?,
        (None, Value::Int(n)) => n.to_string(),
        (None, Value::Decimal(d)) => d.to_string(),
        (None, Value::Bool(flag)) => flag.to_string(),
        // `Email`, `Url` and `IpAddress` are their text.
        (None, Value::Text(text)) => text.to_string(),
        _ => return Err(Fault::Shape),
    };
    Ok(Value::text(&text))
}

/// A standard type's value as its canonical text, as its entry's text form
/// writes it.
pub fn canonical(entry: &StandardType, value: &Value) -> Result<String, Fault> {
    let form = entry.text.ok_or(Fault::Shape)?;
    if let Some(text) = crate::calendar::canonical(form, value) {
        return text;
    }
    Ok(match (form, value) {
        (TextForm::Checked, Value::Text(text)) => text.to_string(),
        (TextForm::Uuid, Value::Uuid(text)) => text.to_ascii_lowercase(),
        (TextForm::Instant, Value::Instant(millis)) => jadpo_std::format_instant(*millis),
        (TextForm::Date, Value::Date(day)) => jadpo_std::format_date(*day),
        (TextForm::Zoned, Value::Zoned(zoned)) => zoned.to_string(),
        (TextForm::Duration, Value::Duration(millis)) => jadpo_std::iso_duration(*millis),
        (TextForm::Money, Value::Money(money)) => money.to_string(),
        (TextForm::Quantity, Value::Quantity(quantity)) => {
            jadpo_std::quantity::canonical(quantity.kind, quantity.amount)
        }
        (TextForm::ExchangeRate, Value::Record(record)) => exchange_rate_text(record)?,
        (TextForm::Mailbox, Value::Record(record)) => crate::mailbox::mailbox_text(record)?,
        (TextForm::Code, Value::Variant(variant)) => entry
            .codes()
            .and_then(|codes| codes.get(variant.ordinal as usize))
            .map(|(_, code)| (*code).to_string())
            .ok_or(Fault::Shape)?,
        (TextForm::Interval, Value::Record(record)) => {
            let (start, end) = interval(entry, record)?;
            format!("{}/{}", text_of(from(start)?)?, text_of(from(end)?)?)
        }
        (TextForm::Range, Value::Record(record)) => {
            let (start, end) = range_ends(record)?;
            let text = |end: Option<&Value>| match end {
                Some(value) => text_of(from(value)?),
                None => Ok(jadpo_std::standard::OPEN_END.to_string()),
            };
            format!("{}/{}", text(start)?, text(end)?)
        }
        _ => return Err(Fault::Shape),
    })
}

/// A range's two ends, `None` for an open one.
fn range_ends(record: &crate::Record) -> Result<(Option<&Value>, Option<&Value>), Fault> {
    let [start, end] = jadpo_std::RANGE_FIELDS;
    let end_of = |name: &str| match record.get(name) {
        Some(Value::None) => Ok(None),
        Some(value) => Ok(Some(value)),
        None => Err(Fault::Shape),
    };
    Ok((end_of(start)?, end_of(end)?))
}

/// `Range<T>` read from its canonical text, each end as `item` (the
/// standard type or representation `T` is built on), `..` for an open
/// end; a start after its end is `malformed`, as for an `InstantRange`.
pub fn read_range(raw: &str, item: &str) -> Result<Value, Fault> {
    let (from, to) = raw.split_once('/').ok_or(MALFORMED)?;
    let end = |text: &str| -> Result<Option<Value>, Fault> {
        if text == jadpo_std::standard::OPEN_END {
            return Ok(None);
        }
        match parse(text, item) {
            Ok(value) => Ok(Some(value)),
            Err(Fault::Broke(_)) => Err(MALFORMED),
            Err(other) => Err(other),
        }
    };
    let (start, end) = (end(from)?, end(to)?);
    if let (Some(start), Some(end)) = (&start, &end)
        && crate::ops::compare(start, end) == Some(std::cmp::Ordering::Greater)
    {
        return Err(MALFORMED);
    }
    Ok(crate::range::range_of(start, end))
}

/// A compound standard type's value as the columns its entry lists hold
/// it (area 19): `Money`'s minor units and currency code, an object's parts
/// in its fields' order (chunk 5.5l), a standard enum's as its code. `None`
/// for a type stored in one column.
pub fn to_parts(value: &Value) -> Option<Vec<Value>> {
    match value {
        Value::Money(money) => Some(vec![Value::Int(money.minor), Value::text(money.code())]),
        Value::Record(record) => {
            let entry = standard_type(&record.ty)?;
            let Holds::Object(fields) = entry.holds else {
                return None;
            };
            if entry.columns().len() != fields.len() {
                return None;
            }
            fields
                .iter()
                .map(|(name, _)| match record.get(name)? {
                    Value::Variant(variant) => standard_type(&variant.ty)
                        .and_then(StandardType::codes)
                        .and_then(|codes| codes.get(variant.ordinal as usize))
                        .map(|(_, code)| Value::text(code)),
                    other => Some(other.clone()),
                })
                .collect()
        }
        _ => None,
    }
}

/// A compound standard type's value from its columns' values, in the order
/// its entry lists them.
pub fn from_parts(entry: &StandardType, parts: &[Value]) -> Result<Value, Fault> {
    match (entry.holds, parts) {
        (Holds::Builtin(jadpo_std::Builtin::Money), [Value::Int(minor), Value::Text(code)]) => {
            let currency = jadpo_std::money::ordinal_of_code(code.trim()).ok_or(Fault::Shape)?;
            Ok(crate::money::value(
                currency,
                crate::money::minor(i128::from(*minor))?,
            ))
        }
        (Holds::Object(fields), parts) if parts.len() == fields.len() => {
            let values = fields
                .iter()
                .zip(parts)
                .map(|((name, part), value)| {
                    let value = match (part.required().0, value) {
                        (jadpo_std::Part::Standard(other), Value::Text(code)) => {
                            let other = standard_type(other).ok_or(Fault::Shape)?;
                            read(other, code.trim()).map_err(|_| Fault::Shape)?
                        }
                        (_, value) => value.clone(),
                    };
                    Ok(((*name).into(), value))
                })
                .collect::<Result<Vec<_>, Fault>>()?;
            Ok(Value::record(entry.name, values))
        }
        _ => Err(Fault::Shape),
    }
}

/// The item type a range type's name gives it: `Int` for `Range<Int>`.
fn range_item(ty: &str) -> Option<&str> {
    ty.strip_prefix(jadpo_std::RANGE)?
        .strip_prefix('<')?
        .strip_suffix('>')
}

/// An interval's two ends, by its entry's fields in order: an
/// `InstantRange`'s instants, a `DateRange`'s dates.
fn interval<'v>(
    entry: &StandardType,
    record: &'v crate::Record,
) -> Result<(&'v Value, &'v Value), Fault> {
    let Holds::Object([(start, _), (end, _)]) = entry.holds else {
        return Err(Fault::Shape);
    };
    match (record.get(start), record.get(end)) {
        (Some(start @ Value::Instant(_)), Some(end @ Value::Instant(_)))
        | (Some(start @ Value::Date(_)), Some(end @ Value::Date(_))) => Ok((start, end)),
        _ => Err(Fault::Shape),
    }
}

/// `text.parse(raw, as:, form:, country:)`: as [`parse`]; given a
/// `CodeForm` variant, a closed list's variant by its code in that code
/// system, ignoring case, a code a retired variant shares with a current
/// one read as the current one; given a `Country`, a phone number or a
/// postal code as people write it there (chunk 5.5l2).
pub fn parse_in(raw: &str, ty: &str, form: &Value, country: &Value) -> Result<Value, Fault> {
    if let Value::Variant(country) = country {
        return written_in(raw, ty, country);
    }
    let Some(form) = code_form(form)? else {
        return parse(raw, ty);
    };
    let entry = standard_type(ty).ok_or(Fault::Shape)?;
    let (Some(codes), Some(forms)) = (entry.codes(), entry.form_codes(form)) else {
        return Err(Fault::Shape);
    };
    let matching = |current: bool| {
        forms.iter().enumerate().find(|(at, code)| {
            !code.is_empty()
                && code.eq_ignore_ascii_case(raw)
                && codes
                    .get(*at)
                    .is_some_and(|(name, _)| entry.is_retired(name) != current)
        })
    };
    let (at, _) = matching(true)
        .or_else(|| matching(false))
        .ok_or(MALFORMED)?;
    let (name, _) = codes.get(at).ok_or(Fault::Shape)?;
    Ok(Value::variant(
        entry.name,
        u32::try_from(at).map_err(|_| Fault::Shape)?,
        name,
        Vec::new(),
    ))
}

/// A phone number or a postal code as people write it in `country`, a
/// `Country` variant: its canonical text, or `malformed` where it isn't
/// one of the country's.
fn written_in(raw: &str, ty: &str, country: &crate::Variant) -> Result<Value, Fault> {
    if &*country.ty != jadpo_std::standard::places::COUNTRY {
        return Err(Fault::Shape);
    }
    let code = standard_type(&country.ty)
        .and_then(StandardType::codes)
        .and_then(|codes| codes.get(country.ordinal as usize))
        .map(|(_, code)| *code)
        .ok_or(Fault::Shape)?;
    let read = match ty {
        "PhoneNumber" => jadpo_std::standard::phones::from_written(raw, code),
        "PostalCode" => jadpo_std::standard::postal::from_written(raw, code),
        _ => return Err(Fault::Shape),
    };
    read.map(|text| Value::text(&text)).ok_or(MALFORMED)
}

/// The part of a standard object a rule across its parts refuses (an
/// address's postal code that doesn't fit its country), if any.
pub fn broken_part(record: &crate::Record) -> Option<&'static str> {
    jadpo_std::standard::part_rules(&record.ty)
        .iter()
        .find(|rule| {
            let (Some(Value::Text(text)), Some(Value::Variant(by))) =
                (record.get(rule.part), record.get(rule.by))
            else {
                return false;
            };
            let code = standard_type(&by.ty)
                .and_then(StandardType::codes)
                .and_then(|codes| codes.get(by.ordinal as usize))
                .map(|(_, code)| *code);
            code.is_some_and(|code| !(rule.fits)(text, code))
        })
        .map(|rule| rule.part)
}

/// `text.parse(raw, as:)`, given the name of the standard type or the
/// representation whose canonical text `raw` is read as; the rules of the
/// type `as:` names are checked by its construction after this. Text that
/// isn't that type at all fails `malformed`.
pub fn parse(raw: &str, ty: &str) -> Result<Value, Fault> {
    if let Some(item) = range_item(ty) {
        return read_range(raw, item);
    }
    if let Some(entry) = standard_type(ty) {
        return read(entry, raw);
    }
    match ty {
        "Text" => Ok(Value::text(raw)),
        "Int" => int(raw).map(Value::Int).ok_or(MALFORMED),
        "Decimal" => decimal(raw).map(Value::Decimal).ok_or(MALFORMED),
        "Bool" => match raw {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(MALFORMED),
        },
        _ => Err(Fault::Shape),
    }
}

/// An `Int`'s digits, with a `-` when negative and nothing else.
fn int(raw: &str) -> Option<i64> {
    let digits = raw.strip_prefix('-').unwrap_or(raw);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
}

/// A `Decimal` as JSON carries it: digits with an optional `-` and
/// places.
fn decimal(raw: &str) -> Option<Decimal> {
    let digits = raw.strip_prefix('-').unwrap_or(raw);
    let (whole, places) = digits.split_once('.').unwrap_or((digits, "0"));
    let plain = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    if !plain(whole) || !plain(places) {
        return None;
    }
    Decimal::parse(raw)
}

/// A standard type's value read from its canonical text.
pub fn read(entry: &StandardType, raw: &str) -> Result<Value, Fault> {
    let form = entry.text.ok_or(Fault::Shape)?;
    if let Some(read) = crate::calendar::read(form, raw) {
        return read.ok_or(MALFORMED);
    }
    let read = match form {
        TextForm::Checked => entry.accepts(raw).then(|| Value::text(raw)),
        TextForm::Uuid => entry
            .validator
            .is_some_and(|validator| validator.accepts(raw))
            .then(|| Value::uuid(raw)),
        TextForm::Instant => jadpo_std::instant_millis(raw)
            .filter(|_| raw.ends_with('Z'))
            .map(Value::Instant),
        TextForm::Date => jadpo_std::date_number(raw)
            .filter(|_| jadpo_std::check_calendar_date(raw).is_ok())
            .map(Value::Date),
        TextForm::Zoned => zoned(raw).map(Value::Zoned),
        TextForm::Duration => crate::json::iso_duration(raw).map(Value::Duration),
        TextForm::Code => code(entry, raw),
        TextForm::Money => crate::money::read(raw).map(Value::Money),
        TextForm::Quantity => {
            let Holds::Builtin(jadpo_std::Builtin::Quantity(kind)) = entry.holds else {
                return Err(Fault::Shape);
            };
            return match jadpo_std::quantity::parse_text(kind, raw) {
                Ok(amount) => Ok(Value::Quantity(crate::Quantity { kind, amount })),
                Err(jadpo_std::quantity::TextProblem::Malformed) => Err(MALFORMED),
                Err(jadpo_std::quantity::TextProblem::UnknownUnit) => {
                    Err(Fault::Broke(RuleKey::UnknownUnit))
                }
            };
        }
        TextForm::ExchangeRate => read_exchange_rate(raw),
        TextForm::Mailbox => crate::mailbox::read_mailbox(entry, raw),
        // Its ends' type is the range type's argument, which `parse` reads.
        TextForm::Range => return Err(Fault::Shape),
        // The time group's forms, which `calendar` reads above.
        TextForm::LocalTime
        | TextForm::LocalDateTime
        | TextForm::YearMonth
        | TextForm::Year
        | TextForm::Quarter
        | TextForm::IsoWeek
        | TextForm::UtcOffset
        | TextForm::Period => return Err(Fault::Shape),
        TextForm::Interval => {
            let Holds::Object([(start, ends), (end, _)]) = entry.holds else {
                return Err(Fault::Shape);
            };
            // Each end in its own canonical text: an instant ending in `Z`,
            // or a date.
            let end_of = |text: &str| match ends.builtin() {
                Some(jadpo_std::Builtin::Instant) => {
                    jadpo_std::instant_millis(text).map(Value::Instant)
                }
                Some(jadpo_std::Builtin::CalendarDate) => jadpo_std::date_number(text)
                    .filter(|_| jadpo_std::check_calendar_date(text).is_ok())
                    .map(Value::Date),
                _ => None,
            };
            raw.split_once('/').and_then(|(from, to)| {
                let (from, to) = (end_of(from)?, end_of(to)?);
                (crate::ops::compare(&from, &to) != Some(std::cmp::Ordering::Greater)).then(|| {
                    Value::record(
                        entry.name,
                        vec![((*start).into(), from), ((*end).into(), to)],
                    )
                })
            })
        }
    };
    read.ok_or(MALFORMED)
}

/// An exchange rate's canonical text, `GBP/EUR 1.17 2026-10-18T09:00:00Z`:
/// its pair as codes, its rate with the places it was given, and the
/// instant it was quoted at (chosen in chunk 5.5h2).
fn exchange_rate_text(record: &crate::Record) -> Result<String, Fault> {
    let code = |name: &str| match record.get(name) {
        Some(Value::Variant(variant)) if &*variant.ty == jadpo_std::CURRENCY => {
            jadpo_std::money::code(variant.ordinal).ok_or(Fault::Shape)
        }
        _ => Err(Fault::Shape),
    };
    let (Some(Value::Decimal(rate)), Some(Value::Instant(at))) =
        (record.get("rate"), record.get("at"))
    else {
        return Err(Fault::Shape);
    };
    Ok(format!(
        "{}/{} {rate} {}",
        code("from")?,
        code("to")?,
        jadpo_std::format_instant(*at)
    ))
}

/// An exchange rate read from its canonical text: two currency codes,
/// read ignoring case, joined by `/`, one space, a decimal rate, one
/// space, and an instant in UTC ending `Z`.
fn read_exchange_rate(raw: &str) -> Option<Value> {
    let mut parts = raw.split(' ');
    let (pair, rate, at) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let (from, to) = pair.split_once('/')?;
    let currency = |code: &str| {
        jadpo_std::money::ordinal_of_code(code)
            .and_then(|ordinal| crate::money::currency_value(ordinal).ok())
    };
    let at = jadpo_std::instant_millis(at).filter(|_| at.ends_with('Z'))?;
    Some(Value::record(
        jadpo_std::standard::EXCHANGE_RATE,
        vec![
            ("from".into(), currency(from)?),
            ("to".into(), currency(to)?),
            ("rate".into(), Value::Decimal(decimal(rate)?)),
            ("at".into(), Value::Instant(at)),
        ],
    ))
}

/// An enum's variant by its code, ignoring case (question 22).
fn code(entry: &StandardType, raw: &str) -> Option<Value> {
    let codes = entry.codes()?;
    let at = codes
        .iter()
        .position(|(_, code)| code.eq_ignore_ascii_case(raw))?;
    let (name, _) = codes.get(at)?;
    Some(Value::variant(
        entry.name,
        u32::try_from(at).ok()?,
        name,
        Vec::new(),
    ))
}

/// A `ZonedTime` from RFC 9557: its local time with the offset the zone
/// has then, and the zone, `2026-10-25T09:30:00+00:00[Europe/London]`.
fn zoned(raw: &str) -> Option<Zoned> {
    let (at, zone) = raw.strip_suffix(']')?.split_once('[')?;
    let written = at.get(at.len().checked_sub(6)?..)?;
    let millis = jadpo_std::instant_millis(at)?;
    let zone = jadpo_std::ZONES
        .iter()
        .position(|(_, iana)| iana.eq_ignore_ascii_case(zone))?;
    let zoned = Zoned {
        millis,
        zone: u32::try_from(zone).ok()?,
    };
    // The offset written is the one the zone has at that instant.
    let (_, offset) = temporal::local(zoned)?;
    let sign = if offset < 0 { '-' } else { '+' };
    let minutes = offset.unsigned_abs() / 60_000;
    let expected = format!("{sign}{:02}:{:02}", minutes / 60, minutes % 60);
    (expected == written).then_some(zoned)
}

/// `text.format`'s options for an amount of money (area 19, "Money and
/// payments"), each its default where the call leaves it out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoneyOptions {
    pub precision: Option<jadpo_std::MoneyPrecision>,
    pub display: Option<jadpo_std::MoneyDisplay>,
    pub sign: Option<jadpo_std::MoneySign>,
}

impl MoneyOptions {
    /// The options from `precision:`, `display:` and `sign:` as a call
    /// gives them, each `none` when left out.
    pub fn of(precision: &Value, display: &Value, sign: &Value) -> Result<MoneyOptions, Fault> {
        fn read<T>(
            value: &Value,
            name: &str,
            from: fn(&str) -> Option<T>,
        ) -> Result<Option<T>, Fault> {
            match value {
                Value::None => Ok(None),
                Value::Variant(variant) if &*variant.ty == name => {
                    from(&variant.name).map(Some).ok_or(Fault::Shape)
                }
                _ => Err(Fault::Shape),
            }
        }
        Ok(MoneyOptions {
            precision: read(
                precision,
                jadpo_std::MoneyPrecision::NAME,
                jadpo_std::MoneyPrecision::from_name,
            )?,
            display: read(
                display,
                jadpo_std::MoneyDisplay::NAME,
                jadpo_std::MoneyDisplay::from_name,
            )?,
            sign: read(
                sign,
                jadpo_std::MoneySign::NAME,
                jadpo_std::MoneySign::from_name,
            )?,
        })
    }
}

/// The unit a quantity is shown in: `unit:` as a call gives it, a variant
/// of the kind's unit enum, or the kind's base unit when left out.
pub fn unit_of(
    kind: jadpo_std::QuantityKind,
    unit: &Value,
) -> Result<&'static jadpo_std::QuantityUnit, Fault> {
    match unit {
        Value::None => Ok(kind.base_unit()),
        Value::Variant(variant) if &*variant.ty == kind.unit_enum() => {
            kind.unit_named(&variant.name).ok_or(Fault::Shape)
        }
        _ => Err(Fault::Shape),
    }
}

/// `text.format` as a call runs it: given, besides the value and its
/// options, the name of the standard type held as text the value is, which
/// the checker fills (an IBAN is text to the runtime), or `none`.
pub fn format_as(
    standard: &Value,
    value: &Value,
    locale: &Value,
    zone: &Value,
    format_spec: &Value,
    options: &MoneyOptions,
    unit: &Value,
) -> Result<Value, Fault> {
    if let (Value::Text(name), Value::Text(_)) = (standard, value)
        && let Some(entry) = standard_type(name)
        && matches!(entry.holds, Holds::Text { .. })
    {
        let tag = present::tag_of(locale)?;
        return Ok(Value::text(&shown(entry, value, tag, zone, format_spec)?));
    }
    format(value, locale, zone, format_spec, options, unit)
}

/// `text.format(value, locale:, zone:, format:, precision:, display:,
/// sign:, unit:)`: the value for people in `locale`, as its entry's format
/// key says. Each option is `none` where the type doesn't take it.
pub fn format(
    value: &Value,
    locale: &Value,
    zone: &Value,
    format_spec: &Value,
    options: &MoneyOptions,
    unit: &Value,
) -> Result<Value, Fault> {
    let tag = present::tag_of(locale)?;
    let text = match (entry_of(value), value) {
        (Some(_), Value::Money(money)) => present::money::format(*money, tag, options)?,
        (Some(_), Value::Quantity(quantity)) => {
            present::units::format(quantity.amount, unit_of(quantity.kind, unit)?, tag)?
        }
        (Some(entry), value) => shown(entry, value, tag, zone, format_spec)?,
        // `Email`, `Url` and `IpAddress` are shown as they are written.
        (None, Value::Text(text)) => text.to_string(),
        // A number in the locale's digits and separators (area 19,
        // "Numbers").
        (None, Value::Int(_) | Value::Decimal(_)) => present::number(value, tag)?,
        _ => return Err(Fault::Shape),
    };
    Ok(Value::text(&text))
}

/// A standard type's value for people in the locale tagged `tag`.
pub fn shown(
    entry: &StandardType,
    value: &Value,
    tag: &str,
    zone: &Value,
    format_spec: &Value,
) -> Result<String, Fault> {
    Ok(match (entry.format.key, value) {
        (FormatKey::Canonical, value) => canonical(entry, value)?,
        (FormatKey::DateTime, value) => {
            text_of(present::format_in(value, zone, tag, format_spec)?)?
        }
        (FormatKey::Interval, Value::Record(record)) => {
            let (start, end) = interval(entry, record)?;
            let start = present::format_in(start, zone, tag, format_spec)?;
            let end = present::format_in(end, zone, tag, format_spec)?;
            format!("{} – {}", text_of(start)?, text_of(end)?)
        }
        (FormatKey::MonthName, Value::Variant(variant)) => {
            present::names::month(variant.ordinal, tag)?
        }
        (FormatKey::YearMonth | FormatKey::Year, value) => present::names::year_month(value, tag)?,
        (FormatKey::OffsetName, value) => present::names::offset(value, tag)?,
        (FormatKey::PeriodUnits, value) => {
            present::names::period(crate::calendar::period_parts(value)?, tag)?
        }
        (FormatKey::DurationUnits, Value::Duration(millis)) => {
            present::names::duration(*millis, tag)?
        }
        (FormatKey::WeekdayName, Value::Variant(variant)) => {
            present::names::weekday(variant.ordinal, tag)?
        }
        (FormatKey::LocaleName, value) => present::names::locale(&canonical(entry, value)?, tag)?,
        (FormatKey::ZoneName, value) => present::names::zone(&canonical(entry, value)?, tag)?,
        (FormatKey::Money, Value::Money(money)) => {
            present::money::format(*money, tag, &MoneyOptions::default())?
        }
        (FormatKey::Units, Value::Quantity(quantity)) => {
            present::units::format(quantity.amount, quantity.kind.base_unit(), tag)?
        }
        (FormatKey::CurrencyName, value) => {
            present::money::currency_name(&canonical(entry, value)?, tag)?
        }
        // A place, a language or a script by CLDR's name for it, a
        // withdrawn code by its list's English name (chunk 5.5l).
        (FormatKey::RegionName, Value::Variant(variant)) => {
            let code = canonical(entry, value)?;
            match present::names::region(&code, tag)? {
                Some(name) => name,
                None => fallback_name(entry, variant, &code),
            }
        }
        (FormatKey::ContinentName, Value::Variant(variant)) => {
            let region = jadpo_std::standard::places::CONTINENT_REGIONS
                .get(variant.ordinal as usize)
                .ok_or(Fault::Shape)?;
            present::names::region(region, tag)?.unwrap_or_else(|| canonical_or_name(variant))
        }
        (FormatKey::LanguageName, Value::Variant(variant)) => {
            let code = canonical(entry, value)?;
            match present::names::language(&code, tag)? {
                Some(name) => name,
                None => fallback_name(entry, variant, &code),
            }
        }
        (FormatKey::ScriptName, Value::Variant(variant)) => {
            let code = canonical(entry, value)?;
            match present::names::script(&code, tag)? {
                Some(name) => name,
                None => fallback_name(entry, variant, &code),
            }
        }
        (FormatKey::SubdivisionName, Value::Variant(variant)) => {
            match jadpo_std::standard::subdivision_name(variant.ordinal) {
                Some(name) => name.to_string(),
                None => canonical(entry, value)?,
            }
        }
        (FormatKey::PersonName, value) => present::contact::person_name(value, tag)?,
        (FormatKey::PostalAddress, value) => present::contact::postal_address(value, tag)?,
        // National for a reader whose region shares the number's calling
        // code, international otherwise (chunk 5.5l2).
        (FormatKey::PhoneNumber, value) => {
            let text = canonical(entry, value)?;
            let reader = present::contact::reader_region(tag)?;
            jadpo_std::standard::phones::shown(&text, reader.as_deref()).ok_or(Fault::Shape)?
        }
        // A payment or identity number's printed form (area 19, chunk
        // 5.5i): grouped, or its last four after a mask.
        (FormatKey::Grouped(grouping), value) => grouping.apply(&canonical(entry, value)?),
        (FormatKey::LastFour { mask }, value) => {
            let text = canonical(entry, value)?;
            let skip = text.chars().count().saturating_sub(4);
            format!("{mask}{}", text.chars().skip(skip).collect::<String>())
        }
        (FormatKey::Range, Value::Record(record)) => {
            let (start, end) = range_ends(record)?;
            let end_shown = |end: Option<&Value>| match end {
                Some(value @ (Value::Int(_) | Value::Decimal(_))) => present::number(value, tag),
                Some(value) => {
                    let entry = entry_of(value).ok_or(Fault::Shape)?;
                    shown(entry, value, tag, zone, format_spec)
                }
                None => Ok("…".to_string()),
            };
            format!("{} – {}", end_shown(start)?, end_shown(end)?)
        }
        _ => return Err(Fault::Shape),
    })
}

/// A withdrawn code's English name from its list, which CLDR doesn't
/// name, or the code itself.
fn fallback_name(entry: &StandardType, variant: &crate::Variant, code: &str) -> String {
    jadpo_std::standard::retired(entry.name, &variant.name)
        .filter(|name| !name.is_empty())
        .map_or_else(|| code.to_string(), str::to_string)
}

/// A variant's own name, for a continent CLDR has no name for.
fn canonical_or_name(variant: &crate::Variant) -> String {
    variant.name.to_string()
}

fn text_of(value: Value) -> Result<String, Fault> {
    match value {
        Value::Text(text) => Ok(text.to_string()),
        _ => Err(Fault::Shape),
    }
}
