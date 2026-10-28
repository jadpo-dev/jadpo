//! Maths on `Decimal` (area 19, "Numbers"): `sqrt`, `power`,
//! `exp`, `ln`, `sin`, `cos`, `atan2` and `pi`, each correctly rounded to
//! the `scale:` asked for by the `rounding:` named, by Jadpo's own integer
//! arithmetic, so every target gives the same digits.
//!
//! "Correctly rounded" means the result is the true value rounded to
//! `scale` places as the mode says. `sqrt` finds it exactly, from whole
//! square roots. The others follow Ziv's strategy: [`series`] gives the
//! value at `q` places within [`series::ERROR`] units of the last place;
//! if both ends of that interval round to the same result, it is the
//! result, since every mode rounds a larger value to a result no smaller.
//! Otherwise the value is worked out again at more places. A true value
//! never lies on a rounding boundary, so the loop ends: the values that
//! would (`exp(0)`, `ln(1)`, `sin(0)`, `cos(0)` and `atan2(0, x)` for `x`
//! at least 0) are given exactly, and every other is transcendental, as
//! `e^r`, `ln r`, `sin r`, `cos r` and `atan r` are for every rational
//! `r` they aren't exact at (Lindemann–Weierstrass), and π is.

mod big;
pub(crate) use jadpo_std::fixed;
pub(crate) mod geodesic;
mod series;

use big::{Big, Int};
use jadpo_std::{Rounding, RuleKey};

use crate::Fault;
use crate::value::Decimal;

/// The places the first try works at past the scale asked for. A value
/// that close to a rounding boundary needs a second try, about once in
/// 10^10 calls.
const FIRST_GUARD: u32 = 12;

/// The places past the scale beyond which the loop stops. A true value
/// is never on a boundary, so it ends long before; this only bounds the
/// work, and reaching it is a defect.
const LAST_GUARD: u32 = 5000;

/// `exp` of this or more is past 38 digits at every scale: e^88 is above
/// 10^38.
const EXP_OVERFLOW: i128 = 88;

/// `exp` of this or less is below half of 10^-18, the smallest place a
/// scale keeps: e^-50 is below 2 × 10^-22.
const EXP_UNDERFLOW: i128 = -50;

/// The scale from `scale:`, which the checker reads written out, from 0
/// to 18.
fn places(scale: i64) -> Result<u32, Fault> {
    u32::try_from(scale)
        .ok()
        .filter(|&scale| scale <= 18)
        .ok_or(Fault::Shape)
}

/// `value / 10^shift`, rounded as `rounding` says, as `rounding::divide`
/// rounds: `up` away from zero, `down` toward it, `half_up` ties away.
fn round_shifted(value: &Int, shift: u32, rounding: Rounding) -> Int {
    let divisor = Big::pow10(shift);
    let (quotient, remainder) = value.magnitude().div_rem(&divisor);
    let step = if remainder.is_zero() {
        false
    } else {
        let twice = remainder.add(&remainder);
        match rounding {
            Rounding::Up => true,
            Rounding::Down => false,
            Rounding::HalfUp => twice >= divisor,
            Rounding::HalfEven => twice > divisor || (twice == divisor && quotient.is_odd()),
        }
    };
    let magnitude = if step {
        quotient.add(&Big::from_u128(1))
    } else {
        quotient
    };
    Int::new(value.is_negative(), magnitude)
}

/// A mantissa at `scale` places as a `Decimal`, or overflow past 38
/// digits.
fn decimal(mantissa: &Int, scale: u32) -> Result<Decimal, Fault> {
    let magnitude = mantissa
        .magnitude()
        .to_u128()
        .filter(|&m| m < 10u128.pow(Decimal::MAX_DIGITS))
        .ok_or(Fault::DecimalOverflow)?;
    let magnitude = i128::try_from(magnitude).map_err(|_| Fault::DecimalOverflow)?;
    Ok(Decimal::new(
        if mantissa.is_negative() {
            -magnitude
        } else {
            magnitude
        },
        scale,
    ))
}

/// Ziv's loop: `approximate(q)` is the value at `q` places within
/// [`series::ERROR`] units; the result is the one both ends of that
/// interval round to.
fn correctly_rounded(
    scale: u32,
    rounding: Rounding,
    approximate: impl Fn(u32) -> Int,
) -> Result<Decimal, Fault> {
    let error = Int::from_i128(i128::from(series::ERROR));
    let mut guard = FIRST_GUARD;
    while guard <= LAST_GUARD {
        let q = scale + guard;
        let value = approximate(q);
        let low = round_shifted(&value.sub(&error), guard, rounding);
        let high = round_shifted(&value.add(&error), guard, rounding);
        if low == high {
            return decimal(&low, scale);
        }
        guard += guard / 2;
    }
    Err(Fault::DecimalOverflow)
}

/// A whole number of units at `scale` places: 0, 1 or -1.
fn exactly(units: i128, scale: u32) -> Result<Decimal, Fault> {
    let one = 10i128.checked_pow(scale).ok_or(Fault::Shape)?;
    Ok(Decimal::new(units * one, scale))
}

/// The square root of `value`, never negative, which the checker proves.
/// `√(m / 10^s) × 10^t` is `√(P / Q)` for whole numbers `P` and `Q`, so
/// its whole part `f` is the whole square root of `⌊P / Q⌋`, it is exact
/// when `f² Q = P`, and it is past halfway to `f + 1` when
/// `4P > (2f + 1)² Q`, exactly halfway when they are equal.
pub(crate) fn sqrt(value: Decimal, scale: i64, rounding: Rounding) -> Result<Decimal, Fault> {
    let scale = places(scale)?;
    if value.mantissa < 0 {
        return Err(Fault::Shape);
    }
    let mantissa = Big::from_u128(value.mantissa.unsigned_abs());
    let (p, q) = if 2 * scale >= value.scale {
        (
            mantissa.mul(&Big::pow10(2 * scale - value.scale)),
            Big::from_u128(1),
        )
    } else {
        (mantissa, Big::pow10(value.scale - 2 * scale))
    };
    let root = p.div_rem(&q).0.isqrt();
    let four_p = p.mul_word(4);
    let odd = root.add(&root).add(&Big::from_u128(1));
    let halfway = odd.mul(&odd).mul(&q);
    let step = match rounding {
        Rounding::Down => false,
        Rounding::Up => root.mul(&root).mul(&q) != p,
        Rounding::HalfUp => four_p >= halfway,
        Rounding::HalfEven => four_p > halfway || (four_p == halfway && root.is_odd()),
    };
    let root = if step {
        root.add(&Big::from_u128(1))
    } else {
        root
    };
    decimal(&Int::from_big(root), scale)
}

/// `value` to the power `exponent`, never negative, which the checker
/// proves: exact, as repeated `*` gives it, its places past `value`'s
/// that are only trailing zeros dropped; `value^0` is 1 at `value`'s
/// scale. Past 38 digits, or 38 places, is overflow.
pub(crate) fn power(value: Decimal, exponent: i64) -> Result<Decimal, Fault> {
    let exponent = u32::try_from(exponent).map_err(|_| Fault::Shape)?;
    let one = 10i128.checked_pow(value.scale).ok_or(Fault::Shape)?;
    let negative = value.mantissa < 0 && exponent % 2 == 1;
    let sign = if negative { -1 } else { 1 };
    if exponent == 0 {
        return Ok(Decimal::new(one, value.scale));
    }
    if value.mantissa == 0 || value.mantissa.unsigned_abs() == one.unsigned_abs() {
        return Ok(Decimal::new(sign * value.mantissa.abs(), value.scale));
    }
    // Any other value past the 256th power has more than 38 digits or 38
    // places: its digits without trailing zeros are at least 2^exponent,
    // or it is a power of ten, 10^(±k × exponent).
    if exponent > 256 {
        return Err(Fault::DecimalOverflow);
    }
    let base = Big::from_u128(value.mantissa.unsigned_abs());
    let mut result = Big::from_u128(1);
    for _ in 0..exponent {
        result = result.mul(&base);
    }
    let mut scale = value.scale * exponent;
    while scale > value.scale {
        let (shorter, remainder) = result.div_word(10);
        if remainder != 0 {
            break;
        }
        result = shorter;
        scale -= 1;
    }
    if scale > Decimal::MAX_DIGITS {
        return Err(Fault::DecimalOverflow);
    }
    decimal(&Int::new(negative, result), scale)
}

/// e to the power `value`. Below 88; past 38 digits is overflow.
pub(crate) fn exp(value: Decimal, scale: i64, rounding: Rounding) -> Result<Decimal, Fault> {
    let scale = places(scale)?;
    if value.mantissa == 0 {
        return exactly(1, scale);
    }
    let whole = value.mantissa / 10i128.pow(value.scale);
    if whole >= EXP_OVERFLOW {
        return Err(Fault::DecimalOverflow);
    }
    if whole <= EXP_UNDERFLOW {
        // Above 0, below half the last place: only `up` leaves it.
        return Ok(Decimal::new(i128::from(rounding == Rounding::Up), scale));
    }
    correctly_rounded(scale, rounding, |q| series::exp(value, q))
}

/// The natural logarithm of `value`; at or below 0 it fails with
/// `out_of_range`, which the call's `attempt` maps.
pub(crate) fn ln(value: Decimal, scale: i64, rounding: Rounding) -> Result<Decimal, Fault> {
    let scale = places(scale)?;
    if value.mantissa <= 0 {
        return Err(Fault::Broke(RuleKey::OutOfRange));
    }
    if value.mantissa == 10i128.pow(value.scale) {
        return exactly(0, scale);
    }
    correctly_rounded(scale, rounding, |q| series::ln(value, q))
}

/// The sine of `angle`, in radians.
pub(crate) fn sin(angle: Decimal, scale: i64, rounding: Rounding) -> Result<Decimal, Fault> {
    let scale = places(scale)?;
    if angle.mantissa == 0 {
        return exactly(0, scale);
    }
    correctly_rounded(scale, rounding, |q| series::sin_cos(angle, q).0)
}

/// The cosine of `angle`, in radians.
pub(crate) fn cos(angle: Decimal, scale: i64, rounding: Rounding) -> Result<Decimal, Fault> {
    let scale = places(scale)?;
    if angle.mantissa == 0 {
        return exactly(1, scale);
    }
    correctly_rounded(scale, rounding, |q| series::sin_cos(angle, q).1)
}

/// The angle from the positive `x` axis to the point (`x`, `y`), in
/// radians from -π to π: π on the negative `x` axis, and 0 at the
/// origin, where no angle is meant.
pub(crate) fn atan2(
    y: Decimal,
    x: Decimal,
    scale: i64,
    rounding: Rounding,
) -> Result<Decimal, Fault> {
    let scale = places(scale)?;
    match (y.mantissa.signum(), x.mantissa.signum()) {
        (0, 0 | 1) => exactly(0, scale),
        (0, _) => correctly_rounded(scale, rounding, series::pi),
        (sign, 0) => correctly_rounded(scale, rounding, |q| {
            let half = series::pi(q + 1).div_word(20);
            if sign < 0 { half.neg() } else { half }
        }),
        _ => correctly_rounded(scale, rounding, |q| series::atan2(y, x, q)),
    }
}

/// π.
pub(crate) fn pi(scale: i64, rounding: Rounding) -> Result<Decimal, Fault> {
    let scale = places(scale)?;
    correctly_rounded(scale, rounding, series::pi)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> Decimal {
        Decimal::parse(text).unwrap_or_else(|| panic!("{text}"))
    }

    fn shown(result: Result<Decimal, Fault>) -> String {
        result.map_or_else(|fault| format!("{fault:?}"), |d| d.to_string())
    }

    #[test]
    fn square_roots_round_exactly_by_every_mode() {
        let root = |value: &str, scale, mode| shown(sqrt(d(value), scale, mode));
        assert_eq!(root("2", 3, Rounding::HalfEven), "1.414");
        assert_eq!(root("2", 3, Rounding::Up), "1.415");
        assert_eq!(root("4.00", 2, Rounding::Up), "2.00", "exact");
        // √0.0625 is 0.25: a tie at one place.
        assert_eq!(root("0.0625", 1, Rounding::HalfEven), "0.2");
        assert_eq!(root("0.0625", 1, Rounding::HalfUp), "0.3");
        assert_eq!(root("0.0625", 1, Rounding::Down), "0.2");
        assert_eq!(root("0", 2, Rounding::Up), "0.00");
        assert_eq!(root("-1", 2, Rounding::Up), "Shape");
    }

    #[test]
    fn powers_are_exact() {
        let raised = |value: &str, n| shown(power(d(value), n));
        assert_eq!(raised("1.5", 3), "3.375");
        assert_eq!(raised("1.50", 2), "2.25", "as `*` keeps places");
        assert_eq!(raised("-2", 3), "-8");
        assert_eq!(raised("1.50", 0), "1.00");
        assert_eq!(raised("-1.0", 1_000_001), "-1.0");
        assert_eq!(raised("0.1", 39), "DecimalOverflow", "39 places");
        assert_eq!(raised("10", 38), "DecimalOverflow");
        assert_eq!(raised("2", 1_000_000), "DecimalOverflow");
    }

    #[test]
    fn exact_and_far_values_need_no_loop() {
        assert_eq!(shown(exp(d("0"), 2, Rounding::Up)), "1.00");
        assert_eq!(
            shown(exp(d("-60"), 18, Rounding::Up)),
            "0.000000000000000001"
        );
        assert_eq!(
            shown(exp(d("-60"), 18, Rounding::HalfUp)),
            "0.000000000000000000"
        );
        assert_eq!(shown(exp(d("88"), 0, Rounding::Down)), "DecimalOverflow");
        assert_eq!(shown(ln(d("1.000"), 3, Rounding::Up)), "0.000");
        assert_eq!(shown(ln(d("0"), 3, Rounding::Up)), "Broke(OutOfRange)");
        assert_eq!(shown(sin(d("0"), 1, Rounding::Up)), "0.0");
        assert_eq!(shown(cos(d("0"), 1, Rounding::Up)), "1.0");
        assert_eq!(shown(atan2(d("0"), d("0"), 1, Rounding::Up)), "0.0");
        assert_eq!(shown(atan2(d("0"), d("-1"), 4, Rounding::Down)), "3.1415");
        assert_eq!(shown(atan2(d("-1"), d("0"), 4, Rounding::Down)), "-1.5707");
    }
}
