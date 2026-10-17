//! The `decimal` namespace (area 3, question 7; area 19, "Numbers").

use crate::Fault;
use crate::value::Decimal;
pub use jadpo_std::Rounding;

/// `value` without its sign, at the same scale.
pub fn abs(value: Decimal) -> Result<Decimal, Fault> {
    if value.mantissa < 0 {
        value.checked_neg().ok_or(Fault::DecimalOverflow)
    } else {
        Ok(value)
    }
}

/// The smaller of `a` and `b`, by value; of equal ones, `a`, so its places
/// are kept.
pub fn min(a: Decimal, b: Decimal) -> Decimal {
    if b < a { b } else { a }
}

/// The larger of `a` and `b`, by value; of equal ones, `a`.
pub fn max(a: Decimal, b: Decimal) -> Decimal {
    if b > a { b } else { a }
}

/// `value` with at most `scale` places, rounded as `rounding` says when
/// places are dropped; a value with fewer places is kept as it is.
pub fn round(value: Decimal, scale: i64, rounding: Rounding) -> Result<Decimal, Fault> {
    // The checker reads `scale:` written out, from 0 to 18.
    let scale = u32::try_from(scale).map_err(|_| Fault::Shape)?;
    if value.scale <= scale {
        return Ok(value);
    }
    value.rescale(scale, rounding).ok_or(Fault::DecimalOverflow)
}

/// The square root of `value`, which is never negative (the checker
/// proves it), correctly rounded to `scale` places as `rounding` says.
pub fn sqrt(value: Decimal, scale: i64, rounding: Rounding) -> Result<Decimal, Fault> {
    crate::maths::sqrt(value, scale, rounding)
}

/// `value` to the power `exponent`, never negative: exact, as repeated
/// `*` gives it; past 38 digits or places is overflow.
pub fn power(value: Decimal, exponent: i64) -> Result<Decimal, Fault> {
    crate::maths::power(value, exponent)
}

/// e to the power `value`, correctly rounded; past 38 digits is overflow.
pub fn exp(value: Decimal, scale: i64, rounding: Rounding) -> Result<Decimal, Fault> {
    crate::maths::exp(value, scale, rounding)
}

/// The natural logarithm of `value`, correctly rounded; `value` at or
/// below 0 fails with `out_of_range`.
pub fn ln(value: Decimal, scale: i64, rounding: Rounding) -> Result<Decimal, Fault> {
    crate::maths::ln(value, scale, rounding)
}

/// The sine of `angle`, in radians, correctly rounded.
pub fn sin(angle: Decimal, scale: i64, rounding: Rounding) -> Result<Decimal, Fault> {
    crate::maths::sin(angle, scale, rounding)
}

/// The cosine of `angle`, in radians, correctly rounded.
pub fn cos(angle: Decimal, scale: i64, rounding: Rounding) -> Result<Decimal, Fault> {
    crate::maths::cos(angle, scale, rounding)
}

/// The angle of the point (`x`, `y`) from the positive `x` axis, in
/// radians from -π to π, correctly rounded; 0 at the origin.
pub fn atan2(y: Decimal, x: Decimal, scale: i64, rounding: Rounding) -> Result<Decimal, Fault> {
    crate::maths::atan2(y, x, scale, rounding)
}

/// π, correctly rounded.
pub fn pi(scale: i64, rounding: Rounding) -> Result<Decimal, Fault> {
    crate::maths::pi(scale, rounding)
}

/// `dividend / divisor` at the dividend's scale, rounded as `rounding`
/// says. The checker rules out a zero divisor; it is a fault if one arrives
/// anyway, as is a result past 38 digits.
pub fn divide(dividend: Decimal, divisor: Decimal, rounding: Rounding) -> Result<Decimal, Fault> {
    if divisor.is_zero() {
        return Err(Fault::DivisionByZero);
    }
    // a / 10^s1 ÷ b / 10^s2, kept at scale s1, is a × 10^s2 ÷ b, worked
    // out as wide as it needs.
    let (mantissa, _) = crate::rounding::divide_shifted(
        dividend.mantissa,
        divisor.scale,
        divisor.mantissa,
        rounding,
        0,
    )
    .ok_or(Fault::DecimalOverflow)?;
    Decimal::new(mantissa, dividend.scale)
        .rescale(dividend.scale, rounding)
        .ok_or(Fault::DecimalOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_share_keeps_the_totals_scale() {
        // 10.00 shared by 3 is 3.33.
        let share = divide(
            Decimal::new(1000, 2),
            Decimal::new(3, 0),
            Rounding::HalfEven,
        );
        assert_eq!(share, Ok(Decimal::new(333, 2)));
        // 0.05 by 2 is 0.025: half even gives 0.02, half up 0.03.
        let half = Decimal::new(5, 2);
        assert_eq!(
            divide(half, Decimal::new(2, 0), Rounding::HalfEven),
            Ok(Decimal::new(2, 2))
        );
        assert_eq!(
            divide(half, Decimal::new(2, 0), Rounding::HalfUp),
            Ok(Decimal::new(3, 2))
        );
        assert_eq!(
            divide(half, Decimal::new(0, 0), Rounding::Up),
            Err(Fault::DivisionByZero)
        );
        // A divisor with many places lifts a large dividend past an i128
        // on the way, though the share itself fits.
        let large = Decimal::new(10i128.pow(36), 2);
        assert_eq!(
            divide(
                large,
                Decimal::new(2 * 10i128.pow(10), 10),
                Rounding::HalfEven
            ),
            Ok(Decimal::new(5 * 10i128.pow(35), 2))
        );
    }

    #[test]
    fn each_rounding_mode_rounds_its_own_way() {
        let d = |m, s| Decimal::new(m, s);
        let mode = |name| Rounding::from_name(name).unwrap_or(Rounding::HalfEven);
        // (value, places, mode, result)
        let cases = [
            (d(225, 2), 1, "half_even", d(22, 1)),
            (d(235, 2), 1, "half_even", d(24, 1)),
            (d(225, 2), 1, "half_up", d(23, 1)),
            (d(-225, 2), 1, "half_up", d(-23, 1)),
            (d(221, 2), 1, "up", d(23, 1)),
            (d(-221, 2), 1, "up", d(-23, 1)),
            (d(229, 2), 1, "down", d(22, 1)),
            (d(-229, 2), 1, "down", d(-22, 1)),
            (d(229, 2), 0, "down", d(2, 0)),
            // Fewer places than `scale` are kept as they are.
            (d(25, 1), 3, "up", d(25, 1)),
        ];
        for (value, places, rounding, expected) in cases {
            let rounded = round(value, places, mode(rounding));
            assert_eq!(rounded, Ok(expected), "{value} {places} {rounding}");
            assert_eq!(rounded.map(|r| r.scale), Ok(expected.scale));
        }
        assert_eq!(abs(d(-150, 2)), Ok(d(150, 2)));
        assert_eq!(abs(d(150, 2)).map(|a| a.scale), Ok(2));
        assert_eq!(min(d(150, 2), d(15, 1)).scale, 2, "the first of equal ones");
        assert_eq!(max(d(150, 2), d(151, 2)), d(151, 2));
    }
}
