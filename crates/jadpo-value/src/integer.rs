//! The `integer` namespace (area 3, question 7).

use crate::Fault;
pub use jadpo_std::Rounding;

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
}
