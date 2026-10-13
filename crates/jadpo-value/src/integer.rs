//! The `integer` namespace (area 3, question 7).

pub use crate::rounding::Rounding;

/// `dividend / divisor`, rounded as `rounding` says. The checker rules out
/// a zero divisor; `None` is the fault if one arrives anyway, or if the
/// result does not fit an `Int`.
pub fn divide(dividend: i64, divisor: i64, rounding: Rounding) -> Option<i64> {
    let result = crate::rounding::divide(i128::from(dividend), i128::from(divisor), rounding)?;
    i64::try_from(result).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_round_up() {
        assert_eq!(divide(25, 10, Rounding::Up), Some(3));
        assert_eq!(divide(i64::MIN, -1, Rounding::Up), None);
    }
}
