//! The `integer` namespace (area 3, question 7; area 19, "Numbers").

use crate::Fault;
use crate::value::Decimal;
pub use jadpo_std::Rounding;

/// `value` without its sign; the smallest `Int` has no positive
/// counterpart, which is a defect, as it is for `-`.
pub fn abs(value: i64) -> Result<i64, Fault> {
    value.checked_abs().ok_or(Fault::IntOverflow)
}

/// The smaller of `a` and `b`.
pub fn min(a: i64, b: i64) -> i64 {
    a.min(b)
}

/// The larger of `a` and `b`.
pub fn max(a: i64, b: i64) -> i64 {
    a.max(b)
}

/// `value` rounded to a whole number as `rounding` says; one past an `Int`
/// is a defect.
pub fn from_decimal(value: Decimal, rounding: Rounding) -> Result<i64, Fault> {
    let whole = value.rescale(0, rounding).ok_or(Fault::IntOverflow)?;
    i64::try_from(whole.mantissa).map_err(|_| Fault::IntOverflow)
}

/// `dividend / divisor`, rounded as `rounding` says. The checker rules out
/// a zero divisor; it is a fault if one arrives anyway, as is a result that
/// does not fit an `Int`.
pub fn divide(dividend: i64, divisor: i64, rounding: Rounding) -> Result<i64, Fault> {
    if divisor == 0 {
        return Err(Fault::DivisionByZero);
    }
    let result = crate::rounding::divide(i128::from(dividend), i128::from(divisor), rounding)
        .ok_or(Fault::IntOverflow)?;
    i64::try_from(result).map_err(|_| Fault::IntOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_round_up() {
        assert_eq!(divide(25, 10, Rounding::Up), Ok(3));
        assert_eq!(divide(i64::MIN, -1, Rounding::Up), Err(Fault::IntOverflow));
        assert_eq!(divide(1, 0, Rounding::Up), Err(Fault::DivisionByZero));
    }

    #[test]
    fn abs_min_max_and_rounding_a_decimal() {
        assert_eq!(abs(-4), Ok(4));
        assert_eq!(abs(i64::MIN), Err(Fault::IntOverflow));
        assert_eq!((min(3, -2), max(3, -2)), (-2, 3));
        let mode = |name| Rounding::from_name(name).unwrap_or(Rounding::HalfEven);
        let cases = [
            ("2.5", "half_even", 2),
            ("3.5", "half_even", 4),
            ("2.5", "half_up", 3),
            ("-2.5", "half_up", -3),
            ("2.1", "up", 3),
            ("-2.1", "up", -3),
            ("2.9", "down", 2),
            ("-2.9", "down", -2),
        ];
        for (value, rounding, expected) in cases {
            let decimal = crate::Value::decimal(value);
            let Some(crate::Value::Decimal(decimal)) = decimal else {
                panic!("{value}");
            };
            assert_eq!(
                from_decimal(decimal, mode(rounding)),
                Ok(expected),
                "{value} {rounding}"
            );
        }
        let huge = Decimal::new(10i128.pow(30), 0);
        assert_eq!(from_decimal(huge, Rounding::Down), Err(Fault::IntOverflow));
    }
}
