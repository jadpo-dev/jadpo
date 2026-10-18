//! Runtime values, the operators and the standard library's operations
//! (docs/compiler-runtime.md §2.1, §2.5 and §2.6).
//!
//! Every standard-library operation is listed once, typed, in `jadpo-std`'s
//! table, declared for programs in a Jadpo stub in `jadpo-check`, and
//! implemented once, here, as one Rust function that both the interpreter
//! and compiled code call. [`run`] matches on the table's [`Op`]
//! exhaustively, so an operation without an implementation does not
//! compile. [`ops`] defines the operators once, with checked arithmetic,
//! and a [`Validator`] checks a value against its type's rules with the
//! same validators the checker runs on literals.
//!
//! [`Value`] is shared behind `Rc`: values never change (area 2) and the
//! runtime is single-threaded (§2.6), so copying a value is a reference
//! count.
//!
//! This crate builds for `wasm32-unknown-unknown` and depends on no other
//! workspace crate than `jadpo-std`, the leaf it shares with the compiler.

// A panic aborts a Workers instance, so runtime code reports faults as
// values (docs/compiler-runtime.md §2.6); tests may still unwrap.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::unreachable,
        clippy::unimplemented
    )
)]

pub mod calendar;
pub mod decimal;
pub mod encoding;
mod event;
pub mod hash;
pub mod integer;
pub mod json;
pub mod list;
pub mod locale;
mod mailbox;
pub mod map;
mod maths;
pub mod money;
pub mod ops;
pub mod order;
pub mod present;
pub mod range;
mod rounding;
pub mod set;
pub mod shape;
pub mod standard;
pub mod temporal;
pub mod text;
pub mod units;
pub mod validate;
mod value;

pub use jadpo_std::{LocalePlan, Op, Rounding};
pub use ops::Fault;
pub use validate::{Invalid, Validator};
pub use value::{Decimal, Money, QUOTIENT_PLACES, Quantity, Record, Value, Variant, Zoned};

/// Runs one standard-library operation with the values of its parameters
/// in table order, its inline-function parameter left out; `each` runs the
/// call's inline function on the values it is given, one item or, for a
/// two-parameter function (area 19), a pair, and gives no value when it
/// stopped (the caller knows why: it is [`Fault::Stopped`] here). `locales` is
/// what the application's `locales { }` declares: the operations that
/// choose among its locales read it, and every other ignores it.
pub fn run(
    op: Op,
    args: &[Value],
    locales: Option<&LocalePlan>,
    each: &mut dyn FnMut(&[Value]) -> Option<Value>,
) -> Result<Value, Fault> {
    match (op, args) {
        // The locale a header asks for, of the ones the application
        // supports (area 10, "Locales"); with none declared there is
        // nothing to choose among.
        (Op::LocaleFromAcceptLanguage, [header]) => {
            let plan = locales.ok_or(Fault::Shape)?;
            let header = match header {
                Value::None => None,
                Value::Text(text) => Some(&**text),
                _ => return Err(Fault::Shape),
            };
            Ok(locale::value(locale::from_accept_language(header, plan)))
        }
        (Op::LocaleFromAcceptLanguage, _) => Err(Fault::Shape),
        (Op::LocaleDirection, [Value::Variant(variant)]) if &*variant.ty == jadpo_std::LOCALE => {
            Ok(locale::direction(variant.ordinal))
        }
        (Op::LocaleDirection, _) => Err(Fault::Shape),
        (Op::DecimalDivide, [dividend, divisor, rounding]) => {
            let decimal = |value: &Value| match value {
                Value::Decimal(d) => Ok(*d),
                Value::Int(n) => Ok(Decimal::from_int(*n)),
                _ => Err(Fault::Shape),
            };
            decimal::divide(
                decimal(dividend)?,
                decimal(divisor)?,
                rounding_of(rounding)?,
            )
            .map(Value::Decimal)
        }
        (Op::IntegerDivide, [Value::Int(dividend), Value::Int(divisor), rounding]) => {
            integer::divide(*dividend, *divisor, rounding_of(rounding)?).map(Value::Int)
        }
        (Op::DecimalDivide | Op::IntegerDivide, _) => Err(Fault::Shape),
        // The number operations (area 19, "Numbers").
        (Op::IntegerAbs, [Value::Int(n)]) => integer::abs(*n).map(Value::Int),
        (Op::IntegerMin, [Value::Int(a), Value::Int(b)]) => Ok(Value::Int(integer::min(*a, *b))),
        (Op::IntegerMax, [Value::Int(a), Value::Int(b)]) => Ok(Value::Int(integer::max(*a, *b))),
        (Op::IntegerFromDecimal, [Value::Decimal(d), rounding]) => {
            integer::from_decimal(*d, rounding_of(rounding)?).map(Value::Int)
        }
        (Op::DecimalAbs, [Value::Decimal(d)]) => decimal::abs(*d).map(Value::Decimal),
        (Op::DecimalMin, [Value::Decimal(a), Value::Decimal(b)]) => {
            Ok(Value::Decimal(decimal::min(*a, *b)))
        }
        (Op::DecimalMax, [Value::Decimal(a), Value::Decimal(b)]) => {
            Ok(Value::Decimal(decimal::max(*a, *b)))
        }
        (Op::DecimalRound, [Value::Decimal(d), Value::Int(scale), rounding]) => {
            decimal::round(*d, *scale, rounding_of(rounding)?).map(Value::Decimal)
        }
        // The maths on `Decimal` (area 19, question 18).
        (Op::DecimalSqrt, [value, Value::Int(scale), rounding]) => {
            decimal::sqrt(decimal_of(value)?, *scale, rounding_of(rounding)?).map(Value::Decimal)
        }
        (Op::DecimalPower, [value, Value::Int(exponent)]) => {
            decimal::power(decimal_of(value)?, *exponent).map(Value::Decimal)
        }
        (Op::DecimalExp, [value, Value::Int(scale), rounding]) => {
            decimal::exp(decimal_of(value)?, *scale, rounding_of(rounding)?).map(Value::Decimal)
        }
        (Op::DecimalLn, [value, Value::Int(scale), rounding]) => {
            decimal::ln(decimal_of(value)?, *scale, rounding_of(rounding)?).map(Value::Decimal)
        }
        (Op::DecimalSin, [angle, Value::Int(scale), rounding]) => {
            decimal::sin(decimal_of(angle)?, *scale, rounding_of(rounding)?).map(Value::Decimal)
        }
        (Op::DecimalCos, [angle, Value::Int(scale), rounding]) => {
            decimal::cos(decimal_of(angle)?, *scale, rounding_of(rounding)?).map(Value::Decimal)
        }
        (Op::DecimalAtan2, [y, x, Value::Int(scale), rounding]) => decimal::atan2(
            decimal_of(y)?,
            decimal_of(x)?,
            *scale,
            rounding_of(rounding)?,
        )
        .map(Value::Decimal),
        (Op::DecimalPi, [Value::Int(scale), rounding]) => {
            decimal::pi(*scale, rounding_of(rounding)?).map(Value::Decimal)
        }
        (
            Op::DecimalSqrt
            | Op::DecimalPower
            | Op::DecimalExp
            | Op::DecimalLn
            | Op::DecimalSin
            | Op::DecimalCos
            | Op::DecimalAtan2
            | Op::DecimalPi,
            _,
        ) => Err(Fault::Shape),
        (
            Op::IntegerAbs
            | Op::IntegerMin
            | Op::IntegerMax
            | Op::IntegerFromDecimal
            | Op::DecimalAbs
            | Op::DecimalMin
            | Op::DecimalMax
            | Op::DecimalRound,
            _,
        ) => Err(Fault::Shape),
        (
            Op::ListAll
            | Op::ListAny
            | Op::ListAppend
            | Op::ListAt
            | Op::ListConcat
            | Op::ListContains
            | Op::ListCount
            | Op::ListDistinct
            | Op::ListDropFirst
            | Op::ListDropLast
            | Op::ListFilter
            | Op::ListFind
            | Op::ListFirst
            | Op::ListFlatten
            | Op::ListGroupBy
            | Op::ListKeepFirst
            | Op::ListKeepLast
            | Op::ListLast
            | Op::ListMap
            | Op::ListMaxBy
            | Op::ListMinBy
            | Op::ListReverse
            | Op::ListSlice
            | Op::ListSortBy
            | Op::ListSum,
            _,
        ) => run_list(op, args, &mut |item| each(std::slice::from_ref(item))),
        (Op::ListZipWith, [Value::List(first), Value::List(second)]) => {
            list::zip_with(first, second, each).map(Value::list)
        }
        (Op::ListZipWith, _) => Err(Fault::Shape),
        (
            Op::SetContains
            | Op::SetCount
            | Op::SetDifference
            | Op::SetIntersection
            | Op::SetUnion
            | Op::SetWith
            | Op::SetWithout,
            _,
        ) => run_set(op, args),
        (
            Op::MapCount
            | Op::MapGet
            | Op::MapHas
            | Op::MapKeys
            | Op::MapMerge
            | Op::MapValues
            | Op::MapWith
            | Op::MapWithout,
            _,
        ) => run_map(op, args),
        (Op::MapFilter, [Value::Map(entries)]) => {
            map::filter(entries, each).map(|kept| Value::Map(std::rc::Rc::new(kept)))
        }
        (Op::MapMapValues, [Value::Map(entries)]) => {
            map::map_values(entries, each).map(|given| Value::Map(std::rc::Rc::new(given)))
        }
        (Op::MapFilter | Op::MapMapValues, _) => Err(Fault::Shape),
        // The money operations (area 19, "Money and payments").
        (Op::MoneyFromMinor, [amount, currency]) => money::from_minor(amount, currency),
        (Op::MoneyOf, [amount, currency]) => money::of(amount, currency),
        (Op::MoneyMinor, [amount]) => money::minor_units(amount),
        (Op::MoneyCurrency, [amount]) => money::currency(amount),
        (Op::MoneySum, [items, currency]) => money::sum(items, currency),
        (Op::MoneySplit, [total, parts]) => money::split(total, parts),
        (Op::MoneyAllocate, [total, ratios]) => money::allocate(total, ratios),
        (Op::MoneyConvert, [amount, rate]) => money::convert(amount, rate),
        (
            Op::MoneyConvert
            | Op::MoneyFromMinor
            | Op::MoneyOf
            | Op::MoneyMinor
            | Op::MoneyCurrency
            | Op::MoneySum
            | Op::MoneySplit
            | Op::MoneyAllocate,
            _,
        ) => Err(Fault::Shape),
        // The `units` operations (area 19, "Physical quantities").
        (Op::UnitsOf, [value, unit]) => units::of(value, unit),
        (Op::UnitsIn, [value, unit, scale, rounding]) => {
            units::in_unit(value, unit, scale, rounding)
        }
        (Op::UnitsSum, [items, zero]) => units::sum(items, zero),
        (Op::UnitsOf | Op::UnitsIn | Op::UnitsSum, _) => Err(Fault::Shape),
        (Op::RangeContains, [span, value]) => range::contains(span, value).map(Value::Bool),
        (Op::RangeOverlaps, [a, b]) => range::overlaps(a, b).map(Value::Bool),
        (Op::RangeIntersection, [a, b]) => range::intersection(a, b),
        (Op::RangeContains | Op::RangeOverlaps | Op::RangeIntersection, _) => Err(Fault::Shape),
        (
            Op::TemporalAddDays
            | Op::TemporalAddMonths
            | Op::TemporalAddYears
            | Op::TemporalBetween
            | Op::TemporalDate
            | Op::TemporalDay
            | Op::TemporalDayBounds
            | Op::TemporalHour
            | Op::TemporalInZone
            | Op::TemporalInstant
            | Op::TemporalMinute
            | Op::TemporalMonth
            | Op::TemporalMonthBounds
            | Op::TemporalOccurrences
            | Op::TemporalOpenAt
            | Op::TemporalPeriodBetween
            | Op::TemporalResolve
            | Op::TemporalWeekBounds
            | Op::TemporalWeekday
            | Op::TemporalYear
            | Op::TemporalYearBounds
            | Op::TemporalZone,
            _,
        ) => run_temporal(op, args),
        // The text for people, over locale data (area 10, "Showing dates
        // and times to people").
        (Op::TemporalFormatRelative, [value, now, zone, locale]) => {
            present::format_relative_value(value, now, zone, locale)
        }
        (Op::TemporalFormatRelative, _) => Err(Fault::Shape),
        // The standard type contract (area 19): one implementation each,
        // over the types' data entries. `text.parse` is given the name of
        // the standard type or representation it reads.
        (
            Op::TextFormat,
            [
                value,
                locale,
                zone,
                format,
                precision,
                display,
                sign,
                unit,
                standard,
            ],
        ) => standard::format_as(
            standard,
            value,
            locale,
            zone,
            format,
            &standard::MoneyOptions::of(precision, display, sign)?,
            unit,
        ),
        (Op::TextFrom, [value, form]) => standard::from_in(value, form),
        (Op::TextParse, [Value::Text(raw), Value::Text(ty), form, country]) => {
            standard::parse_in(raw, ty, form, country)
        }
        (Op::TextFormat | Op::TextFrom | Op::TextParse, _) => Err(Fault::Shape),
        // The encodings (area 19, "Encodings"): decoding fails `malformed`.
        (Op::EncodingToBase64, [Value::Bytes(bytes)]) => {
            Ok(Value::text(&encoding::to_base64(bytes)))
        }
        (Op::EncodingToBase64url, [Value::Bytes(bytes)]) => {
            Ok(Value::text(&encoding::to_base64url(bytes)))
        }
        (Op::EncodingToHex, [Value::Bytes(bytes)]) => Ok(Value::text(&encoding::to_hex(bytes))),
        (Op::EncodingToUtf8, [Value::Text(text)]) => Ok(bytes_value(text.as_bytes())),
        (Op::EncodingToUrlComponent, [Value::Text(text)]) => {
            Ok(Value::text(&encoding::to_url_component(text)))
        }
        (Op::EncodingFromBase64, [Value::Text(text)]) => decoded(encoding::from_base64(text)),
        (Op::EncodingFromBase64url, [Value::Text(text)]) => decoded(encoding::from_base64url(text)),
        (Op::EncodingFromHex, [Value::Text(text)]) => decoded(encoding::from_hex(text)),
        (Op::EncodingFromUtf8, [Value::Bytes(bytes)]) => encoding::from_utf8(bytes)
            .map(|text| Value::text(&text))
            .ok_or(Fault::Broke(jadpo_std::RuleKey::Malformed)),
        (
            Op::EncodingToBase64
            | Op::EncodingToBase64url
            | Op::EncodingToHex
            | Op::EncodingToUtf8
            | Op::EncodingToUrlComponent
            | Op::EncodingFromBase64
            | Op::EncodingFromBase64url
            | Op::EncodingFromHex
            | Op::EncodingFromUtf8,
            _,
        ) => Err(Fault::Shape),
        // SHA-256 of bytes, or of text's UTF-8 (area 19, question 24).
        (Op::HashSha256, [Value::Bytes(bytes)]) => Ok(bytes_value(&hash::sha256(bytes))),
        (Op::HashSha256, [Value::Text(text)]) => Ok(bytes_value(&hash::sha256(text.as_bytes()))),
        (Op::HashSha256, _) => Err(Fault::Shape),
        // `json.read` and `json.write` take the codec the plan numbers for
        // their call besides their values, so each executor runs them
        // through [`json::Codecs::read_value`] and
        // [`json::Codecs::write_value`] over the plan's codecs.
        (Op::JsonRead | Op::JsonWrite, _) => Err(Fault::Shape),
        (
            Op::TextContains
            | Op::TextDropFirst
            | Op::TextDropLast
            | Op::TextEndsWith
            | Op::TextJoin
            | Op::TextKeepFirst
            | Op::TextKeepLast
            | Op::TextLength
            | Op::TextLower
            | Op::TextPadStart
            | Op::TextReplace
            | Op::TextSlice
            | Op::TextSlug
            | Op::TextSplit
            | Op::TextStartsWith
            | Op::TextTrim
            | Op::TextUpper,
            _,
        ) => run_text(op, args),
    }
}

/// A `text` operation (area 19, "Text"): [`text`] holds what each does.
/// Every one but `join` takes text first.
fn run_text(op: Op, args: &[Value]) -> Result<Value, Fault> {
    if let (Op::TextJoin, [Value::List(pieces), Value::Text(separator)]) = (op, args) {
        let pieces = pieces
            .iter()
            .map(|piece| match piece {
                Value::Text(piece) => Ok(&**piece),
                _ => Err(Fault::Shape),
            })
            .collect::<Result<Vec<&str>, Fault>>()?;
        return Ok(Value::text(&text::join(pieces, separator)));
    }
    let Some((Value::Text(value), rest)) = args.split_first() else {
        return Err(Fault::Shape);
    };
    let shown = |text: String| Value::text(&text);
    let locale = |given: &Value| match given {
        Value::None => Ok(None),
        given => present::tag_of(given).map(Some),
    };
    match (op, rest) {
        (Op::TextLength, []) => Ok(Value::Int(text::length(value))),
        (Op::TextTrim, []) => Ok(Value::text(text::trim(value))),
        (Op::TextLower, [given]) => text::lower(value, locale(given)?).map(shown),
        (Op::TextUpper, [given]) => text::upper(value, locale(given)?).map(shown),
        (Op::TextContains, [Value::Text(part)]) => Ok(Value::Bool(text::contains(value, part))),
        (Op::TextStartsWith, [Value::Text(prefix)]) => {
            Ok(Value::Bool(text::starts_with(value, prefix)))
        }
        (Op::TextEndsWith, [Value::Text(suffix)]) => {
            Ok(Value::Bool(text::ends_with(value, suffix)))
        }
        (Op::TextReplace, [Value::Text(find), Value::Text(with)]) => {
            text::replace(value, find, with).map(shown)
        }
        (Op::TextSplit, [Value::Text(separator)]) => Ok(Value::list(
            text::split(value, separator)?
                .into_iter()
                .map(Value::text)
                .collect(),
        )),
        (Op::TextSlice, [Value::Int(from), Value::Int(count)]) => {
            text::slice(value, *from, *count).map(shown)
        }
        (Op::TextKeepFirst, [Value::Int(n)]) => text::keep_first(value, *n).map(shown),
        (Op::TextKeepLast, [Value::Int(n)]) => text::keep_last(value, *n).map(shown),
        (Op::TextDropFirst, [Value::Int(n)]) => text::drop_first(value, *n).map(shown),
        (Op::TextDropLast, [Value::Int(n)]) => text::drop_last(value, *n).map(shown),
        (Op::TextPadStart, [Value::Int(length), Value::Text(fill)]) => {
            text::pad_start(value, *length, fill).map(shown)
        }
        (Op::TextSlug, []) => Ok(shown(text::slug(value))),
        _ => Err(Fault::Shape),
    }
}

/// Bytes as a value holds them.
fn bytes_value(bytes: &[u8]) -> Value {
    Value::Bytes(std::rc::Rc::from(bytes))
}

/// What a decoding gave, or `malformed` (area 19, "Encodings").
fn decoded(bytes: Option<Vec<u8>>) -> Result<Value, Fault> {
    bytes
        .map(|bytes| Value::Bytes(std::rc::Rc::from(bytes)))
        .ok_or(Fault::Broke(jadpo_std::RuleKey::Malformed))
}

/// A `list` operation (area 19, "Lists"): [`list`] holds what each does.
/// Every one takes the list first; the inline function, where there is
/// one, is `each`.
fn run_list(
    op: Op,
    args: &[Value],
    each: &mut dyn FnMut(&Value) -> Option<Value>,
) -> Result<Value, Fault> {
    let Some((Value::List(items), rest)) = args.split_first() else {
        return Err(Fault::Shape);
    };
    let n = |at: usize| match rest.get(at) {
        Some(Value::Int(n)) => Ok(*n),
        _ => Err(Fault::Shape),
    };
    match (op, rest) {
        (Op::ListCount, []) => Ok(Value::Int(list::count(items))),
        (Op::ListContains, [item]) => Ok(Value::Bool(list::contains(items, item))),
        (Op::ListMap, []) => list::map(items, each).map(Value::list),
        (Op::ListFilter, []) => list::filter(items, each).map(Value::list),
        (Op::ListSum, [zero]) => list::sum(items, zero),
        (Op::ListAny, []) => list::any(items, each).map(Value::Bool),
        (Op::ListAll, []) => list::all(items, each).map(Value::Bool),
        (Op::ListFind, []) => list::find(items, each),
        (Op::ListFirst, []) => Ok(list::first(items)),
        (Op::ListLast, []) => Ok(list::last(items)),
        (Op::ListAt, [_]) => list::at(items, n(0)?),
        (Op::ListMinBy, []) => list::min_by(items, each),
        (Op::ListMaxBy, []) => list::max_by(items, each),
        (Op::ListSortBy, [Value::None]) => list::sort_by(items, each).map(Value::list),
        (Op::ListSortBy, [locale]) => {
            list::sort_by_locale(items, each, present::tag_of(locale)?).map(Value::list)
        }
        (Op::ListReverse, []) => Ok(Value::list(list::reverse(items))),
        (Op::ListDistinct, []) => Ok(Value::list(list::distinct(items))),
        (Op::ListSlice, [_, _]) => list::slice(items, n(0)?, n(1)?).map(Value::list),
        (Op::ListKeepFirst, [_]) => list::keep_first(items, n(0)?).map(Value::list),
        (Op::ListKeepLast, [_]) => list::keep_last(items, n(0)?).map(Value::list),
        (Op::ListDropFirst, [_]) => list::drop_first(items, n(0)?).map(Value::list),
        (Op::ListDropLast, [_]) => list::drop_last(items, n(0)?).map(Value::list),
        (Op::ListAppend, [item]) => Ok(Value::list(list::append(items, item))),
        (Op::ListConcat, [Value::List(second)]) => Ok(Value::list(list::concat(items, second))),
        (Op::ListFlatten, []) => list::flatten(items).map(Value::list),
        (Op::ListGroupBy, []) => list::group_by(items, each),
        _ => Err(Fault::Shape),
    }
}

/// A `set` operation (area 19, "Sets"): [`set`] holds what each does.
/// Every one takes a set first, held in the canonical order, so each
/// result it builds from those items is in that order too.
fn run_set(op: Op, args: &[Value]) -> Result<Value, Fault> {
    let Some((Value::Set(items), rest)) = args.split_first() else {
        return Err(Fault::Shape);
    };
    let built = |items: Vec<Value>| Value::Set(std::rc::Rc::from(items));
    match (op, rest) {
        (Op::SetCount, []) => Ok(Value::Int(set::count(items))),
        (Op::SetContains, [item]) => Ok(Value::Bool(set::contains(items, item))),
        (Op::SetWith, [item]) => Ok(built(set::with(items, item))),
        (Op::SetWithout, [item]) => Ok(built(set::without(items, item))),
        (Op::SetUnion, [Value::Set(other)]) => Ok(built(set::union(items, other))),
        (Op::SetIntersection, [Value::Set(other)]) => Ok(built(set::intersection(items, other))),
        (Op::SetDifference, [Value::Set(other)]) => Ok(built(set::difference(items, other))),
        _ => Err(Fault::Shape),
    }
}

/// A `map` operation (area 19, "Maps"): [`map`] holds what each does.
/// Every one takes a map first.
fn run_map(op: Op, args: &[Value]) -> Result<Value, Fault> {
    let Some((Value::Map(entries), rest)) = args.split_first() else {
        return Err(Fault::Shape);
    };
    let built = |entries| Value::Map(std::rc::Rc::new(entries));
    match (op, rest) {
        (Op::MapCount, []) => Ok(Value::Int(map::count(entries))),
        (Op::MapGet, [key]) => Ok(map::get(entries, key)),
        (Op::MapHas, [key]) => Ok(Value::Bool(map::has(entries, key))),
        (Op::MapKeys, []) => Ok(Value::Set(std::rc::Rc::from(map::keys(entries)))),
        (Op::MapValues, []) => Ok(Value::list(map::values(entries))),
        (Op::MapWith, [key, value]) => Ok(built(map::with(entries, key, value))),
        (Op::MapWithout, [key]) => Ok(built(map::without(entries, key))),
        (Op::MapMerge, [Value::Map(second)]) => Ok(built(map::merge(entries, second))),
        _ => Err(Fault::Shape),
    }
}

/// A `temporal` operation (area 10): [`temporal`] holds what each does.
fn run_temporal(op: Op, args: &[Value]) -> Result<Value, Fault> {
    use temporal::{Dated, Period};
    let bounds = |date: &Value, zone: &Value, period: Period| match date {
        Value::Date(day) => {
            let (start, end) = temporal::bounds(*day, temporal::zone_of(zone)?, period)?;
            Ok(temporal::range(start, end))
        }
        _ => Err(Fault::Shape),
    };
    let zoned = |value: &Value| match value {
        Value::Zoned(zoned) => Ok(*zoned),
        _ => Err(Fault::Shape),
    };
    let part = |value: &Value, at: usize| -> Result<Value, Fault> {
        let (year, month, day) = temporal::civil(Dated::of(value)?)?;
        Ok(Value::Int([year, month, day].get(at).copied().unwrap_or(0)))
    };
    match (op, args) {
        (Op::TemporalAddDays, [value, Value::Int(days), dst]) => {
            temporal::add_days(Dated::of(value)?, *days, temporal::dst_of(dst)?).map(Dated::value)
        }
        (Op::TemporalAddMonths, [value, Value::Int(months), invalid_day, dst]) => {
            temporal::add_months(
                Dated::of(value)?,
                *months,
                temporal::invalid_day_of(invalid_day)?,
                temporal::dst_of(dst)?,
            )
            .map(Dated::value)
        }
        (Op::TemporalAddYears, [value, Value::Int(years), invalid_day, dst]) => {
            temporal::add_years(
                Dated::of(value)?,
                *years,
                temporal::invalid_day_of(invalid_day)?,
                temporal::dst_of(dst)?,
            )
            .map(Dated::value)
        }
        (Op::TemporalBetween, [Value::Instant(start), Value::Instant(end)]) => {
            temporal::between(*start, *end).map(Value::Duration)
        }
        (Op::TemporalDate, [value]) => temporal::local_date(zoned(value)?).map(Value::Date),
        (Op::TemporalDay, [value]) => part(value, 2),
        (Op::TemporalMonth, [value]) => part(value, 1),
        (Op::TemporalYear, [value]) => part(value, 0),
        (Op::TemporalWeekday, [value]) => {
            temporal::weekday(Dated::of(value)?).map(temporal::weekday_value)
        }
        (Op::TemporalHour, [value]) => {
            temporal::clock(zoned(value)?).map(|clock| Value::Int(i64::from(clock / 3_600_000)))
        }
        (Op::TemporalMinute, [value]) => {
            temporal::clock(zoned(value)?).map(|clock| Value::Int(i64::from(clock / 60_000 % 60)))
        }
        (Op::TemporalInZone, [Value::Instant(at), zone]) => {
            temporal::in_zone(*at, temporal::zone_of(zone)?).map(Value::Zoned)
        }
        (Op::TemporalInstant, [value]) => zoned(value).map(|zoned| Value::Instant(zoned.millis)),
        (Op::TemporalZone, [value]) => zoned(value).map(|zoned| temporal::zone_value(zoned.zone)),
        (Op::TemporalResolve, [Value::Date(day), Value::Text(at), zone, dst]) => {
            let clock = jadpo_std::clock_millis(at).ok_or(Fault::Shape)?;
            temporal::resolve(
                *day,
                clock,
                temporal::zone_of(zone)?,
                temporal::dst_of(dst)?,
            )
            .map(Value::Zoned)
        }
        (Op::TemporalDayBounds, [date, zone]) => bounds(date, zone, Period::Day),
        (Op::TemporalWeekBounds, [date, zone, starts_on]) => {
            bounds(date, zone, Period::Week(temporal::weekday_of(starts_on)?))
        }
        (Op::TemporalMonthBounds, [date, zone]) => bounds(date, zone, Period::Month),
        (Op::TemporalYearBounds, [date, zone]) => bounds(date, zone, Period::Year),
        (Op::TemporalPeriodBetween, [Value::Date(start), Value::Date(end)]) => {
            calendar::period_between(*start, *end)
        }
        (Op::TemporalOccurrences, [Value::Text(rule), Value::Instant(from), Value::Int(count)]) => {
            temporal::occurrences(rule, *from, *count)
                .map(|found| Value::list(found.into_iter().map(Value::Zoned).collect()))
        }
        (Op::TemporalOpenAt, [Value::Text(hours), Value::Instant(at)]) => {
            temporal::open_at(hours, *at).map(Value::Bool)
        }
        _ => Err(Fault::Shape),
    }
}

/// A `Rounding` value as the runtime holds it: a plain variant.
/// A value built on `Decimal`, or a whole number a literal gave it.
fn decimal_of(value: &Value) -> Result<Decimal, Fault> {
    match value {
        Value::Decimal(d) => Ok(*d),
        Value::Int(n) => Ok(Decimal::from_int(*n)),
        _ => Err(Fault::Shape),
    }
}

pub(crate) fn rounding_of(value: &Value) -> Result<Rounding, Fault> {
    match value {
        Value::Variant(variant) if variant.fields.is_empty() => {
            Rounding::from_name(&variant.name).ok_or(Fault::Shape)
        }
        _ => Err(Fault::Shape),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variant(name: &str) -> Value {
        let ordinal = jadpo_std::variant_ordinal(Rounding::NAME, name).unwrap_or(0);
        Value::variant(Rounding::NAME, ordinal, name, Vec::new())
    }

    #[test]
    fn every_operation_in_the_table_runs() {
        let london = temporal::zone_value(
            jadpo_std::variant_ordinal(jadpo_std::ZONE, "europe_london").unwrap_or(0),
        );
        let english = Value::variant(
            jadpo_std::LOCALE,
            jadpo_std::locale_ordinal("en_gb").unwrap_or(0),
            "en_gb",
            Vec::new(),
        );
        let plan = LocalePlan {
            default: jadpo_std::locale_ordinal("en_gb").unwrap_or(0),
            supported: vec![
                jadpo_std::locale_ordinal("en_gb").unwrap_or(0),
                jadpo_std::locale_ordinal("fr_fr").unwrap_or(0),
            ],
            in_progress: Vec::new(),
        };
        let date = Value::Date(jadpo_std::date_number("2026-10-25").unwrap_or(0));
        let zoned = Value::Zoned(Zoned {
            millis: 0,
            zone: temporal::zone_of(&london).unwrap_or(0),
        });
        let items = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let gbp = jadpo_std::money::ordinal_of_code("GBP").unwrap_or(0);
        let pounds = money::currency_value(gbp).unwrap_or(Value::None);
        let price = money::value(gbp, 1050);
        let tags = Value::set(vec![Value::Int(3), Value::Int(1)]);
        let stock = Value::Map(std::rc::Rc::new(
            [(Value::Int(1), Value::Int(4))].into_iter().collect(),
        ));
        let mut double = |given: &[Value]| match given {
            [Value::Int(n)] => Some(Value::Int(n * 2)),
            _ => None,
        };
        let bytes = Value::Bytes(std::rc::Rc::from(&b"loaf"[..]));
        let mile = Value::variant("LengthUnit", 7, "mile", Vec::new());
        for operation in jadpo_std::OPERATIONS {
            let args: Vec<Value> = match operation.op {
                // Run with the plan's codecs (`json`'s own tests).
                Op::JsonRead | Op::JsonWrite => continue,
                Op::EncodingToBase64
                | Op::EncodingToBase64url
                | Op::EncodingToHex
                | Op::EncodingFromUtf8
                | Op::HashSha256 => vec![bytes.clone()],
                Op::EncodingToUtf8 | Op::EncodingToUrlComponent => vec![Value::text("rye loaf")],
                Op::EncodingFromBase64 => vec![Value::text("bG9hZg==")],
                Op::EncodingFromBase64url => vec![Value::text("bG9hZg")],
                Op::EncodingFromHex => vec![Value::text("6c6f6166")],
                Op::TextFormat => {
                    vec![
                        Value::Instant(0),
                        english.clone(),
                        london.clone(),
                        Value::text("date_long"),
                        Value::None,
                        Value::None,
                        Value::None,
                        Value::None,
                        Value::None,
                    ]
                }
                Op::UnitsOf => vec![Value::Int(5), mile.clone()],
                Op::UnitsIn => vec![
                    units::value(jadpo_std::QuantityKind::Length, 5_000_000_000),
                    mile.clone(),
                    Value::Int(2),
                    variant("half_even"),
                ],
                Op::UnitsSum => vec![
                    Value::list(vec![units::value(jadpo_std::QuantityKind::Length, 1)]),
                    units::value(jadpo_std::QuantityKind::Length, 0),
                ],
                Op::MoneyFromMinor => vec![Value::Int(1050), pounds.clone()],
                Op::MoneyOf => vec![Value::Decimal(Decimal::new(1050, 2)), pounds.clone()],
                Op::MoneyMinor | Op::MoneyCurrency => vec![price.clone()],
                Op::MoneySum => vec![Value::list(vec![price.clone()]), pounds.clone()],
                Op::MoneySplit => vec![price.clone(), Value::Int(3)],
                Op::MoneyAllocate => vec![price.clone(), items.clone()],
                Op::MoneyConvert => vec![
                    price.clone(),
                    Value::record(
                        jadpo_std::standard::EXCHANGE_RATE,
                        vec![
                            ("from".into(), pounds.clone()),
                            ("to".into(), pounds.clone()),
                            ("rate".into(), Value::Decimal(Decimal::new(1, 0))),
                            ("at".into(), Value::Instant(0)),
                        ],
                    ),
                ],
                Op::TextFrom => vec![Value::Instant(0), Value::None],
                Op::TextParse => vec![
                    Value::text("2026-10-25"),
                    Value::text("CalendarDate"),
                    Value::None,
                    Value::None,
                ],
                Op::TemporalFormatRelative => {
                    vec![
                        Value::Instant(0),
                        Value::Instant(1000),
                        london.clone(),
                        english.clone(),
                    ]
                }
                Op::LocaleFromAcceptLanguage => vec![Value::text("en-GB, fr;q=0.5")],
                Op::LocaleDirection => vec![locale::value(0)],
                Op::ListContains | Op::ListAppend | Op::ListAt => {
                    vec![items.clone(), Value::Int(2)]
                }
                Op::ListKeepFirst | Op::ListKeepLast | Op::ListDropFirst | Op::ListDropLast => {
                    vec![items.clone(), Value::Int(1)]
                }
                Op::ListSlice => vec![items.clone(), Value::Int(1), Value::Int(1)],
                Op::ListSum => vec![items.clone(), Value::Int(0)],
                Op::ListConcat | Op::ListZipWith => vec![items.clone(), items.clone()],
                Op::ListFlatten => vec![Value::list(vec![items.clone(), items.clone()])],
                Op::ListAll
                | Op::ListAny
                | Op::ListCount
                | Op::ListDistinct
                | Op::ListFilter
                | Op::ListFind
                | Op::ListFirst
                | Op::ListGroupBy
                | Op::ListLast
                | Op::ListMap
                | Op::ListMaxBy
                | Op::ListMinBy
                | Op::ListReverse => vec![items.clone()],
                Op::ListSortBy => vec![items.clone(), Value::None],
                Op::TextLength | Op::TextTrim | Op::TextSlug => vec![Value::text("Rye loaf")],
                Op::TextLower | Op::TextUpper => vec![Value::text("Rye"), english.clone()],
                Op::TextContains | Op::TextStartsWith | Op::TextEndsWith | Op::TextSplit => {
                    vec![Value::text("Rye loaf"), Value::text(" ")]
                }
                Op::TextReplace => vec![
                    Value::text("Rye loaf"),
                    Value::text("Rye"),
                    Value::text("Spelt"),
                ],
                Op::TextJoin => vec![
                    Value::list(vec![Value::text("Rye"), Value::text("loaf")]),
                    Value::text(" "),
                ],
                Op::TextSlice => vec![Value::text("Rye loaf"), Value::Int(1), Value::Int(2)],
                Op::TextKeepFirst | Op::TextKeepLast | Op::TextDropFirst | Op::TextDropLast => {
                    vec![Value::text("Rye loaf"), Value::Int(3)]
                }
                Op::TextPadStart => vec![Value::text("42"), Value::Int(6), Value::text("0")],
                Op::SetCount => vec![tags.clone()],
                Op::SetContains | Op::SetWith | Op::SetWithout => {
                    vec![tags.clone(), Value::Int(2)]
                }
                Op::SetUnion | Op::SetIntersection | Op::SetDifference => {
                    vec![tags.clone(), tags.clone()]
                }
                Op::MapCount | Op::MapFilter | Op::MapKeys | Op::MapMapValues | Op::MapValues => {
                    vec![stock.clone()]
                }
                Op::MapGet | Op::MapHas | Op::MapWithout => vec![stock.clone(), Value::Int(1)],
                Op::MapWith => vec![stock.clone(), Value::Int(1), Value::Int(2)],
                Op::MapMerge => vec![stock.clone(), stock.clone()],
                Op::RangeContains => vec![temporal::range(0, 10), Value::Instant(5)],
                Op::RangeOverlaps | Op::RangeIntersection => {
                    vec![temporal::range(0, 10), temporal::range(5, 15)]
                }
                Op::DecimalDivide => vec![
                    Value::Decimal(Decimal::new(1000, 2)),
                    Value::Int(3),
                    variant("half_even"),
                ],
                Op::IntegerDivide => vec![Value::Int(25), Value::Int(10), variant("up")],
                Op::IntegerAbs => vec![Value::Int(-3)],
                Op::IntegerMin | Op::IntegerMax => vec![Value::Int(3), Value::Int(4)],
                Op::IntegerFromDecimal => {
                    vec![Value::Decimal(Decimal::new(25, 1)), variant("down")]
                }
                Op::DecimalAbs => vec![Value::Decimal(Decimal::new(-25, 1))],
                Op::DecimalMin | Op::DecimalMax => vec![
                    Value::Decimal(Decimal::new(25, 1)),
                    Value::Decimal(Decimal::new(3, 0)),
                ],
                Op::DecimalRound => vec![
                    Value::Decimal(Decimal::new(225, 2)),
                    Value::Int(1),
                    variant("half_up"),
                ],
                Op::DecimalSqrt
                | Op::DecimalExp
                | Op::DecimalLn
                | Op::DecimalSin
                | Op::DecimalCos => vec![
                    Value::Decimal(Decimal::new(2, 0)),
                    Value::Int(4),
                    variant("half_even"),
                ],
                Op::DecimalPower => vec![Value::Decimal(Decimal::new(15, 1)), Value::Int(3)],
                Op::DecimalAtan2 => vec![
                    Value::Decimal(Decimal::new(1, 0)),
                    Value::Decimal(Decimal::new(-1, 0)),
                    Value::Int(4),
                    variant("half_even"),
                ],
                Op::DecimalPi => vec![Value::Int(4), variant("half_even")],
                Op::TemporalAddDays => vec![date.clone(), Value::Int(1), Value::None],
                Op::TemporalAddMonths | Op::TemporalAddYears => {
                    vec![zoned.clone(), Value::Int(1), Value::None, Value::None]
                }
                Op::TemporalBetween => vec![Value::Instant(0), Value::Instant(1000)],
                Op::TemporalPeriodBetween => vec![date.clone(), date.clone()],
                Op::TemporalOccurrences => vec![
                    Value::text(
                        "DTSTART;TZID=Europe/London:20261026T093000\nRRULE:FREQ=WEEKLY;BYDAY=MO",
                    ),
                    Value::Instant(0),
                    Value::Int(3),
                ],
                Op::TemporalOpenAt => vec![
                    Value::text("Mo-Fr 09:00-17:30 [Europe/London]"),
                    Value::Instant(0),
                ],
                Op::TemporalInZone => vec![Value::Instant(0), london.clone()],
                Op::TemporalResolve => vec![
                    date.clone(),
                    Value::text("09:30"),
                    london.clone(),
                    Value::None,
                ],
                Op::TemporalDayBounds | Op::TemporalMonthBounds | Op::TemporalYearBounds => {
                    vec![date.clone(), london.clone()]
                }
                Op::TemporalWeekBounds => {
                    vec![date.clone(), london.clone(), temporal::weekday_value(0)]
                }
                Op::TemporalDay | Op::TemporalMonth | Op::TemporalYear | Op::TemporalWeekday => {
                    vec![date.clone()]
                }
                Op::TemporalDate
                | Op::TemporalHour
                | Op::TemporalMinute
                | Op::TemporalInstant
                | Op::TemporalZone => vec![zoned.clone()],
            };
            let result = run(operation.op, &args, Some(&plan), &mut |given| {
                Some(Value::Bool(
                    matches!(given.last(), Some(Value::Int(n)) if n % 2 == 1),
                ))
            });
            assert!(result.is_ok(), "{} runs", operation.qualified());
        }
        assert_eq!(
            run(Op::ListMap, std::slice::from_ref(&items), None, &mut double),
            Ok(Value::list(vec![
                Value::Int(2),
                Value::Int(4),
                Value::Int(6)
            ]))
        );
        assert_eq!(
            run(
                Op::DecimalDivide,
                &[
                    Value::Decimal(Decimal::new(1000, 2)),
                    Value::Int(3),
                    variant("half_even")
                ],
                None,
                &mut double
            ),
            Ok(Value::Decimal(Decimal::new(333, 2)))
        );
        assert_eq!(
            run(
                Op::IntegerDivide,
                &[Value::Int(1), Value::Int(0), variant("up")],
                None,
                &mut double
            ),
            Err(Fault::DivisionByZero),
            "a zero divisor is a fault"
        );
        assert_eq!(
            run(Op::ListCount, &[Value::Int(1)], None, &mut double),
            Err(Fault::Shape)
        );
        // The locale a header asks for is a `Locale` variant, and a call
        // with no locales declared, or a header that is not text, is not
        // run.
        let mut asks = |header: Value, locales: Option<&LocalePlan>| {
            run(
                Op::LocaleFromAcceptLanguage,
                &[header],
                locales,
                &mut double,
            )
        };
        assert_eq!(
            asks(Value::text("fr-CA, en;q=0.1"), Some(&plan)),
            Ok(Value::variant(
                jadpo_std::LOCALE,
                jadpo_std::locale_ordinal("fr_fr").unwrap_or(0),
                "fr_fr",
                Vec::new()
            ))
        );
        assert_eq!(
            asks(Value::None, Some(&plan)),
            Ok(Value::variant(
                jadpo_std::LOCALE,
                jadpo_std::locale_ordinal("en_gb").unwrap_or(0),
                "en_gb",
                Vec::new()
            ))
        );
        assert_eq!(asks(Value::None, None), Err(Fault::Shape));
        assert_eq!(asks(Value::Int(1), Some(&plan)), Err(Fault::Shape));
    }
}
