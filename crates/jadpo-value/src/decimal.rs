//! The `decimal` namespace (area 3, question 7).

use crate::value::Decimal;
pub use jadpo_std::Rounding;

/// `dividend / divisor` at the dividend's scale, rounded as `rounding`
/// says. The checker rules out a zero divisor; `None` is the fault if one
/// arrives anyway, or if the result overflows.
pub fn divide(dividend: Decimal, divisor: Decimal, rounding: Rounding) -> Option<Decimal> {
    // a / 10^s1 ÷ b / 10^s2, kept at scale s1, is a × 10^s2 ÷ b.
    let lift = 10i128.checked_pow(divisor.scale)?;
    let numerator = dividend.mantissa.checked_mul(lift)?;
    let mantissa = crate::rounding::divide(numerator, divisor.mantissa, rounding)?;
    Some(Decimal::new(mantissa, dividend.scale))
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
        assert_eq!(share, Some(Decimal::new(333, 2)));
        // 0.05 by 2 is 0.025: half even gives 0.02, half up 0.03.
        let half = Decimal::new(5, 2);
        assert_eq!(
            divide(half, Decimal::new(2, 0), Rounding::HalfEven),
            Some(Decimal::new(2, 2))
        );
        assert_eq!(
            divide(half, Decimal::new(2, 0), Rounding::HalfUp),
            Some(Decimal::new(3, 2))
        );
        assert_eq!(divide(half, Decimal::new(0, 0), Rounding::Up), None);
    }
}
