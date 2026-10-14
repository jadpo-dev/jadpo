//! The `decimal` namespace (area 3, question 7).

use crate::Fault;
use crate::value::Decimal;
pub use jadpo_std::Rounding;

/// `dividend / divisor` at the dividend's scale, rounded as `rounding`
/// says. The checker rules out a zero divisor; it is a fault if one arrives
/// anyway, as is a result past 38 digits.
pub fn divide(dividend: Decimal, divisor: Decimal, rounding: Rounding) -> Result<Decimal, Fault> {
    if divisor.is_zero() {
        return Err(Fault::DivisionByZero);
    }
    // a / 10^s1 ÷ b / 10^s2, kept at scale s1, is a × 10^s2 ÷ b.
    let lift = 10i128
        .checked_pow(divisor.scale)
        .ok_or(Fault::DecimalOverflow)?;
    let numerator = dividend
        .mantissa
        .checked_mul(lift)
        .ok_or(Fault::DecimalOverflow)?;
    let mantissa = crate::rounding::divide(numerator, divisor.mantissa, rounding)
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
    }
}
