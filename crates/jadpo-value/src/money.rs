//! `Money` (area 19, "Money and payments", question 16): its operations
//! and the operator table's rows for it, defined once for both executors.
//! An amount is a whole number of its currency's minor units, so adding
//! and comparing are exact; `*` and `/` by a number round to a whole minor
//! unit by the rounding the operand's type declares; and sharing an amount
//! out (`split`, `allocate`) never rounds: every minor unit goes to some
//! share, the leftover ones one each to the first shares, so the shares
//! always add up to the total.

use std::cmp::Ordering;

use jadpo_std::{Rounding, RuleKey};

use crate::{Decimal, Fault, Value, value::Money};

/// Two amounts in different currencies, where only the running program
/// knows them: the condition an `attempt` maps (area 19).
pub const MISMATCH: Fault = Fault::Broke(RuleKey::CurrencyMismatch);

/// An amount as a value.
pub fn value(currency: u32, minor: i64) -> Value {
    Value::Money(Money { currency, minor })
}

/// The currency `value` is, from a `Currency` variant.
fn currency_of(value: &Value) -> Result<u32, Fault> {
    match value {
        Value::Variant(variant) if &*variant.ty == jadpo_std::CURRENCY => Ok(variant.ordinal),
        _ => Err(Fault::Shape),
    }
}

/// A currency as a `Currency` value.
pub fn currency_value(ordinal: u32) -> Result<Value, Fault> {
    let name = jadpo_std::money::variant(ordinal).ok_or(Fault::Shape)?;
    Ok(Value::variant(
        jadpo_std::CURRENCY,
        ordinal,
        name,
        Vec::new(),
    ))
}

fn money_of(value: &Value) -> Result<Money, Fault> {
    match value {
        Value::Money(money) => Ok(*money),
        _ => Err(Fault::Shape),
    }
}

/// The most minor units an amount holds either side of zero: JavaScript's
/// largest safe integer, 2^53 - 1, so the wire's `amount` is always a JSON
/// number a client reads exactly (chosen in chunk 5.5h).
pub const MOST_MINOR: i64 = jadpo_std::http::SAFE_INTEGER as i64;

/// Minor units as an amount holds them; past [`MOST_MINOR`] either side is
/// overflow, a defect as it is for `+` on `Int`.
pub fn minor(units: i128) -> Result<i64, Fault> {
    if units.unsigned_abs() > MOST_MINOR as u128 {
        return Err(Fault::IntOverflow);
    }
    i64::try_from(units).map_err(|_| Fault::IntOverflow)
}

/// `money.from_minor(amount, currency)`: `amount` minor units.
pub fn from_minor(amount: &Value, currency: &Value) -> Result<Value, Fault> {
    let Value::Int(amount) = amount else {
        return Err(Fault::Shape);
    };
    Ok(value(currency_of(currency)?, minor(i128::from(*amount))?))
}

/// `money.of(amount, currency)`: a decimal of major units, which fails
/// `too_many_places` when it has more places than the currency, trailing
/// zeros aside (`10.500` is ten pounds fifty).
pub fn of(amount: &Value, currency: &Value) -> Result<Value, Fault> {
    let amount = match amount {
        Value::Decimal(decimal) => *decimal,
        Value::Int(n) => Decimal::from_int(*n),
        _ => return Err(Fault::Shape),
    };
    let currency = currency_of(currency)?;
    let exponent = jadpo_std::money::exponent(currency).ok_or(Fault::Shape)?;
    let places = jadpo_std::Num {
        mantissa: amount.mantissa,
        scale: amount.scale,
    }
    .significant_places();
    if places > exponent {
        return Err(Fault::Broke(RuleKey::TooManyPlaces));
    }
    let units = amount
        .rescale(exponent, Rounding::HalfEven)
        .ok_or(Fault::IntOverflow)?;
    Ok(value(currency, minor(units.mantissa)?))
}

/// `money.minor(value)`: its minor units.
pub fn minor_units(amount: &Value) -> Result<Value, Fault> {
    Ok(Value::Int(money_of(amount)?.minor))
}

/// `money.currency(value)`: its currency.
pub fn currency(amount: &Value) -> Result<Value, Fault> {
    currency_value(money_of(amount)?.currency)
}

/// `money.sum(items, currency:)`: the items added up, in `currency`,
/// which is also the zero an empty list gives; an item in another
/// currency fails `currency_mismatch`.
pub fn sum(items: &Value, currency: &Value) -> Result<Value, Fault> {
    let currency = currency_of(currency)?;
    let items = items.items().ok_or(Fault::Shape)?;
    let mut total: i64 = 0;
    for item in items {
        let item = money_of(item)?;
        if item.currency != currency {
            return Err(MISMATCH);
        }
        total = minor(i128::from(total) + i128::from(item.minor))?;
    }
    Ok(value(currency, total))
}

/// `money.split(total, parts)`: `parts` shares, each the total divided by
/// `parts` in whole minor units, and the remainder one unit each to the
/// first shares: £10.00 in 3 is £3.34, £3.33, £3.33. `parts` is at least
/// 1, which the checker proves.
pub fn split(total: &Value, parts: &Value) -> Result<Value, Fault> {
    let total = money_of(total)?;
    let Value::Int(parts) = parts else {
        return Err(Fault::Shape);
    };
    if *parts < 1 {
        return Err(Fault::Shape);
    }
    let ratios = vec![1i64; usize::try_from(*parts).map_err(|_| Fault::Shape)?];
    shared_out(total, &ratios)
}

/// `money.allocate(total, ratios)`: a share for each ratio in proportion
/// to it, rounded down to a whole minor unit, and the units left over one
/// each to the first shares whose ratio isn't 0, so a share of ratio 0 is
/// always nothing. The ratios are never negative and there is at least
/// one, which the checker proves; all of them 0 shares out nothing in
/// proportion, which is a defect, as dividing by zero would be.
pub fn allocate(total: &Value, ratios: &Value) -> Result<Value, Fault> {
    let total = money_of(total)?;
    let ratios = ratios
        .items()
        .ok_or(Fault::Shape)?
        .iter()
        .map(|ratio| match ratio {
            Value::Int(n) if *n >= 0 => Ok(*n),
            _ => Err(Fault::Shape),
        })
        .collect::<Result<Vec<i64>, Fault>>()?;
    shared_out(total, &ratios)
}

/// The shares of `total` in proportion to `ratios`: worked out on the
/// amount's size, the sign given back to every share, so a refund shares
/// out as a payment does.
fn shared_out(total: Money, ratios: &[i64]) -> Result<Value, Fault> {
    if ratios.is_empty() || ratios.iter().any(|ratio| *ratio < 0) {
        return Err(Fault::Shape);
    }
    let whole: i128 = ratios.iter().map(|ratio| i128::from(*ratio)).sum();
    if whole == 0 {
        return Err(Fault::DivisionByZero);
    }
    let size = i128::from(total.minor).abs();
    let mut each: Vec<i128> = ratios
        .iter()
        .map(|ratio| size * i128::from(*ratio) / whole)
        .collect();
    let mut left = size - each.iter().sum::<i128>();
    for (share, ratio) in each.iter_mut().zip(ratios) {
        if left == 0 {
            break;
        }
        if *ratio > 0 {
            *share += 1;
            left -= 1;
        }
    }
    let sign = if total.minor < 0 { -1 } else { 1 };
    let items = each
        .into_iter()
        .map(|share| Ok(value(total.currency, minor(share * sign)?)))
        .collect::<Result<Vec<Value>, Fault>>()?;
    Ok(Value::list(items))
}

/// `money.convert(value, rate)`: `value` in the rate's `to` currency,
/// given in its `from` one: the major units times the rate, rounded half
/// to even to the `to` currency's minor unit. An amount in any other
/// currency fails `currency_mismatch`, so a rate the wrong way round is
/// never applied (area 19, "Money and payments").
pub fn convert(amount: &Value, rate: &Value) -> Result<Value, Fault> {
    let amount = money_of(amount)?;
    let Value::Record(record) = rate else {
        return Err(Fault::Shape);
    };
    let field = |name: &str| record.get(name).ok_or(Fault::Shape);
    let from = currency_of(field("from")?)?;
    let to = currency_of(field("to")?)?;
    let rate = decimal_of(field("rate")?)?;
    if amount.currency != from {
        return Err(MISMATCH);
    }
    let from_places = jadpo_std::money::exponent(from).ok_or(Fault::Shape)?;
    let to_places = jadpo_std::money::exponent(to).ok_or(Fault::Shape)?;
    // minor units of `from`, as major units, times the rate, as minor
    // units of `to`.
    let exact = Decimal::from_int(amount.minor)
        .checked_mul(rate)
        .ok_or(Fault::IntOverflow)?;
    let exact = if to_places >= from_places {
        let factor = 10i128
            .checked_pow(to_places - from_places)
            .ok_or(Fault::IntOverflow)?;
        exact
            .checked_mul(Decimal::new(factor, 0))
            .ok_or(Fault::IntOverflow)?
    } else {
        Decimal::new(exact.mantissa, exact.scale + (from_places - to_places))
    };
    let units = exact
        .rescale(0, Rounding::HalfEven)
        .ok_or(Fault::IntOverflow)?;
    Ok(value(to, minor(units.mantissa)?))
}

/// `a + b` in one currency.
pub fn add(a: Money, b: Money) -> Result<Money, Fault> {
    if a.currency != b.currency {
        return Err(MISMATCH);
    }
    Ok(Money {
        currency: a.currency,
        minor: minor(i128::from(a.minor) + i128::from(b.minor))?,
    })
}

/// `a - b` in one currency.
pub fn subtract(a: Money, b: Money) -> Result<Money, Fault> {
    if a.currency != b.currency {
        return Err(MISMATCH);
    }
    Ok(Money {
        currency: a.currency,
        minor: minor(i128::from(a.minor) - i128::from(b.minor))?,
    })
}

/// The order of two amounts in one currency.
pub fn compare(a: Money, b: Money) -> Result<Ordering, Fault> {
    if a.currency != b.currency {
        return Err(MISMATCH);
    }
    Ok(a.minor.cmp(&b.minor))
}

/// A number as a decimal.
fn decimal_of(value: &Value) -> Result<Decimal, Fault> {
    match value {
        Value::Decimal(decimal) => Ok(*decimal),
        Value::Int(n) => Ok(Decimal::from_int(*n)),
        _ => Err(Fault::Shape),
    }
}

/// `amount * factor`, a number: exact, then rounded to a whole minor unit
/// as `rounding` says (the amount's type's `rounding:`, area 19).
pub fn multiply(amount: Money, factor: &Value, rounding: Rounding) -> Result<Money, Fault> {
    let exact = Decimal::from_int(amount.minor)
        .checked_mul(decimal_of(factor)?)
        .ok_or(Fault::IntOverflow)?;
    let units = exact.rescale(0, rounding).ok_or(Fault::IntOverflow)?;
    Ok(Money {
        currency: amount.currency,
        minor: minor(units.mantissa)?,
    })
}

/// `amount / divisor`, a decimal rate (`/ 1.20` takes VAT off): rounded to
/// a whole minor unit as `rounding` says. The divisor is never zero, which
/// the checker proves.
pub fn divide(amount: Money, divisor: &Value, rounding: Rounding) -> Result<Money, Fault> {
    let quotient = crate::decimal::divide(
        Decimal::from_int(amount.minor),
        decimal_of(divisor)?,
        rounding,
    )
    .map_err(|fault| match fault {
        Fault::DecimalOverflow => Fault::IntOverflow,
        other => other,
    })?;
    Ok(Money {
        currency: amount.currency,
        minor: minor(quotient.mantissa)?,
    })
}

/// `a / b` in one currency: how many times `b` goes into `a`, as `/` gives
/// for two decimals (area 3).
pub fn ratio(a: Money, b: Money) -> Result<Value, Fault> {
    if a.currency != b.currency {
        return Err(MISMATCH);
    }
    Decimal::from_int(a.minor)
        .quotient(Decimal::from_int(b.minor))
        .map(Value::Decimal)
        .ok_or(if b.minor == 0 {
            Fault::DivisionByZero
        } else {
            Fault::DecimalOverflow
        })
}

/// An amount read from its canonical text, `GBP 10.50`: the code, read
/// ignoring case, one space, and the amount in major units with exactly
/// the currency's places (`JPY 500`, `BHD 1.250`); anything else is
/// `malformed`.
pub fn read(raw: &str) -> Option<Money> {
    let (code, amount) = raw.split_once(' ')?;
    let currency = jadpo_std::money::ordinal_of_code(code)?;
    let exponent = jadpo_std::money::exponent(currency)?;
    let digits = amount.strip_prefix('-').unwrap_or(amount);
    let (whole, places) = match digits.split_once('.') {
        Some((whole, places)) => (whole, places),
        None => (digits, ""),
    };
    let plain = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty()
        || !plain(whole)
        || !plain(places)
        || places.len() != exponent as usize
        || (exponent > 0 && !digits.contains('.'))
    {
        return None;
    }
    let units: i128 = format!("{whole}{places}").parse().ok()?;
    let units = if amount.starts_with('-') {
        -units
    } else {
        units
    };
    Some(Money {
        currency,
        minor: minor(units).ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gbp() -> u32 {
        jadpo_std::money::ordinal_of_code("GBP").unwrap()
    }

    fn pounds(minor: i64) -> Money {
        Money {
            currency: gbp(),
            minor,
        }
    }

    fn minors(value: Value) -> Vec<i64> {
        value
            .items()
            .unwrap()
            .iter()
            .map(|share| match share {
                Value::Money(money) => money.minor,
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn splitting_hands_every_penny_to_someone() {
        let total = Value::Money(pounds(1000));
        assert_eq!(
            minors(split(&total, &Value::Int(3)).unwrap()),
            [334, 333, 333]
        );
        assert_eq!(
            minors(split(&Value::Money(pounds(-1000)), &Value::Int(3)).unwrap()),
            [-334, -333, -333]
        );
        let ratios = Value::list(vec![Value::Int(0), Value::Int(1), Value::Int(1)]);
        assert_eq!(
            minors(allocate(&Value::Money(pounds(5)), &ratios).unwrap()),
            [0, 3, 2],
            "a share of ratio 0 is nothing"
        );
        let none = Value::list(vec![Value::Int(0), Value::Int(0)]);
        assert_eq!(
            allocate(&total, &none),
            Err(Fault::DivisionByZero),
            "all ratios 0 is a defect"
        );
    }

    #[test]
    fn rates_round_by_the_types_rounding() {
        let rate = Value::decimal("0.125").unwrap();
        assert_eq!(
            multiply(pounds(100), &rate, Rounding::HalfEven).map(|m| m.minor),
            Ok(12)
        );
        assert_eq!(
            multiply(pounds(100), &rate, Rounding::HalfUp).map(|m| m.minor),
            Ok(13)
        );
        let vat = Value::decimal("1.20").unwrap();
        assert_eq!(
            divide(pounds(1200), &vat, Rounding::HalfEven).map(|m| m.minor),
            Ok(1000)
        );
        let other = Money {
            currency: jadpo_std::money::ordinal_of_code("EUR").unwrap(),
            minor: 1,
        };
        assert_eq!(add(pounds(1), other), Err(MISMATCH));
        assert_eq!(compare(pounds(1), other), Err(MISMATCH));
        assert_eq!(
            ratio(pounds(1050), pounds(350)),
            Ok(Value::Decimal(Decimal::new(3, 0)))
        );
    }

    fn rate(from: &str, to: &str, rate: &str) -> Value {
        let code =
            |code: &str| currency_value(jadpo_std::money::ordinal_of_code(code).unwrap()).unwrap();
        Value::record(
            jadpo_std::standard::EXCHANGE_RATE,
            vec![
                ("from".into(), code(from)),
                ("to".into(), code(to)),
                ("rate".into(), Value::decimal(rate).unwrap()),
                ("at".into(), Value::Instant(0)),
            ],
        )
    }

    #[test]
    fn a_rate_converts_from_its_own_currency_only() {
        let minor_of = |value: Value| match value {
            Value::Money(money) => (money.code(), money.minor),
            other => panic!("{other:?}"),
        };
        let cake = Value::Money(pounds(1050));
        // €12.285, half to even.
        assert_eq!(
            convert(&cake, &rate("GBP", "EUR", "1.17")).map(minor_of),
            Ok(("EUR", 1228))
        );
        // ¥2000.25 to a whole yen, and 5.250 dinars in three places.
        assert_eq!(
            convert(&cake, &rate("GBP", "JPY", "190.5")).map(minor_of),
            Ok(("JPY", 2000))
        );
        assert_eq!(
            convert(&cake, &rate("GBP", "BHD", "0.5")).map(minor_of),
            Ok(("BHD", 5250))
        );
        let yen = Value::Money(Money {
            currency: jadpo_std::money::ordinal_of_code("JPY").unwrap(),
            minor: 2001,
        });
        assert_eq!(
            convert(&yen, &rate("JPY", "GBP", "0.005")).map(minor_of),
            Ok(("GBP", 1000)),
            "¥2001 is £10.005, half to even"
        );
        assert_eq!(
            convert(&cake, &rate("EUR", "GBP", "0.85")),
            Err(MISMATCH),
            "a rate from euros, the wrong way round"
        );
        assert_eq!(
            convert(&Value::Money(pounds(-1050)), &rate("GBP", "EUR", "1.17")).map(minor_of),
            Ok(("EUR", -1228))
        );
    }

    #[test]
    fn canonical_text_has_the_currencys_places() {
        assert_eq!(read("GBP 10.50"), Some(pounds(1050)));
        assert_eq!(read("gbp -0.05"), Some(pounds(-5)));
        assert_eq!(read("GBP 10.5"), None);
        assert_eq!(read("GBP 10"), None);
        assert_eq!(
            read("JPY 500").map(|m| (m.code(), m.minor)),
            Some(("JPY", 500))
        );
        assert_eq!(read("JPY 500.0"), None);
        assert_eq!(read("BHD 1.250").map(|m| m.minor), Some(1250));
        assert_eq!(pounds(1050).to_string(), "GBP 10.50");
        assert_eq!(pounds(-5).to_string(), "GBP -0.05");
    }
}
