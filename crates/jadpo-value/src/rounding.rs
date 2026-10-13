//! Rounding a division that doesn't come out exact (area 3, question 7),
//! shared by `integer.divide` and `decimal.divide`. `Rounding` itself is
//! declared once, in `jadpo-std`.

use jadpo_std::Rounding;

/// `dividend / divisor` rounded as `rounding` says; `None` when the divisor
/// is zero or the result overflows.
pub(crate) fn divide(dividend: i128, divisor: i128, rounding: Rounding) -> Option<i128> {
    if divisor == 0 {
        return None;
    }
    let quotient = dividend.checked_div(divisor)?;
    let remainder = dividend.checked_rem(divisor)?;
    if remainder == 0 {
        return Some(quotient);
    }
    // The exact result lies between `quotient` and the next value away
    // from zero, on the side of the result's sign.
    let away = if (dividend < 0) == (divisor < 0) {
        1
    } else {
        -1
    };
    let twice = remainder.unsigned_abs().checked_mul(2)?;
    let whole = divisor.unsigned_abs();
    let step = match rounding {
        Rounding::Up => true,
        Rounding::HalfUp => twice >= whole,
        Rounding::HalfEven => twice > whole || (twice == whole && quotient % 2 != 0),
    };
    if step {
        quotient.checked_add(away)
    } else {
        Some(quotient)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_mode_rounds_its_own_way() {
        assert_eq!(divide(10, 4, Rounding::HalfEven), Some(2));
        assert_eq!(divide(14, 4, Rounding::HalfEven), Some(4));
        assert_eq!(divide(10, 4, Rounding::HalfUp), Some(3));
        assert_eq!(divide(-10, 4, Rounding::HalfUp), Some(-3));
        assert_eq!(divide(9, 4, Rounding::Up), Some(3));
        assert_eq!(divide(-9, 4, Rounding::Up), Some(-3));
        assert_eq!(divide(8, 4, Rounding::Up), Some(2));
        assert_eq!(divide(1, 0, Rounding::Up), None);
    }
}
