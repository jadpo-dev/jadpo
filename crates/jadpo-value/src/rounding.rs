//! Rounding a division that doesn't come out exact (area 3, question 7),
//! shared by `integer.divide` and `decimal.divide`. `Rounding` itself is
//! declared once, in `jadpo-std`.

use jadpo_std::Rounding;

/// `dividend / divisor` rounded as `rounding` says; `None` when the divisor
/// is zero or the result overflows.
pub(crate) fn divide(dividend: i128, divisor: i128, rounding: Rounding) -> Option<i128> {
    divide_shifted(dividend, 0, divisor, rounding, 0).map(|(quotient, _)| quotient)
}

/// `dividend × 10^shift / divisor`, worked out exactly however wide the
/// product, rounded as `rounding` says, then with up to `trim` trailing
/// zeros dropped: the result and how many zeros were dropped. `None` when
/// the divisor is zero or the result is past an `i128`. This is how `/`
/// keeps every one of its 18 extra places on large operands (area 3,
/// question 7) rather than as many as an `i128` has room for.
pub(crate) fn divide_shifted(
    dividend: i128,
    shift: u32,
    divisor: i128,
    rounding: Rounding,
    trim: u32,
) -> Option<(i128, u32)> {
    if divisor == 0 {
        return None;
    }
    let mut numerator = Wide::from(dividend.unsigned_abs());
    for _ in 0..shift {
        numerator.multiply_by(10);
    }
    let whole = divisor.unsigned_abs();
    let (mut quotient, remainder) = numerator.div_rem(whole);
    if remainder != 0 {
        // The exact result lies between `quotient` and the next value
        // away from zero; `twice` can't overflow, as the remainder is
        // below a divisor of at most 2^127.
        let twice = remainder << 1;
        let step = match rounding {
            Rounding::Up => true,
            Rounding::HalfUp => twice >= whole,
            Rounding::HalfEven => twice > whole || (twice == whole && quotient.is_odd()),
        };
        if step {
            quotient.add_one();
        }
    }
    let mut dropped = 0;
    while dropped < trim && !quotient.is_zero() && quotient.remainder_by(10) == 0 {
        quotient.divide_by(10);
        dropped += 1;
    }
    let magnitude = i128::try_from(quotient.to_u128()?).ok()?;
    let negative = (dividend < 0) != (divisor < 0);
    Some((if negative { -magnitude } else { magnitude }, dropped))
}

/// An unsigned whole number as wide as a scaled dividend needs: 64-bit
/// limbs, least significant first. A dividend has at most 38 digits and a
/// shift at most 94 places, so it never grows past a handful of limbs.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Wide(Vec<u64>);

impl Wide {
    fn from(n: u128) -> Wide {
        Wide(vec![n as u64, (n >> 64) as u64])
    }

    fn is_zero(&self) -> bool {
        self.0.iter().all(|&limb| limb == 0)
    }

    fn is_odd(&self) -> bool {
        self.0.first().is_some_and(|limb| limb & 1 == 1)
    }

    fn multiply_by(&mut self, factor: u64) {
        let mut carry = 0u128;
        for limb in &mut self.0 {
            let wide = u128::from(*limb) * u128::from(factor) + carry;
            *limb = wide as u64;
            carry = wide >> 64;
        }
        if carry != 0 {
            self.0.push(carry as u64);
        }
    }

    fn add_one(&mut self) {
        for limb in &mut self.0 {
            let (sum, carried) = limb.overflowing_add(1);
            *limb = sum;
            if !carried {
                return;
            }
        }
        self.0.push(1);
    }

    fn remainder_by(&self, divisor: u64) -> u64 {
        let mut remainder = 0u128;
        for &limb in self.0.iter().rev() {
            remainder = ((remainder << 64) | u128::from(limb)) % u128::from(divisor);
        }
        remainder as u64
    }

    fn divide_by(&mut self, divisor: u64) {
        let mut remainder = 0u128;
        for limb in self.0.iter_mut().rev() {
            let current = (remainder << 64) | u128::from(*limb);
            *limb = (current / u128::from(divisor)) as u64;
            remainder = current % u128::from(divisor);
        }
    }

    /// The quotient and remainder by `divisor`, one bit at a time; the
    /// remainder stays below the divisor, at most 2^127, so shifting it
    /// left never overflows.
    fn div_rem(&self, divisor: u128) -> (Wide, u128) {
        let mut quotient = vec![0u64; self.0.len()];
        let mut remainder = 0u128;
        for (limb, out) in self.0.iter().zip(quotient.iter_mut()).rev() {
            for bit in (0..64).rev() {
                remainder = (remainder << 1) | u128::from((limb >> bit) & 1);
                if remainder >= divisor {
                    remainder -= divisor;
                    *out |= 1 << bit;
                }
            }
        }
        (Wide(quotient), remainder)
    }

    fn to_u128(&self) -> Option<u128> {
        if self.0.iter().skip(2).any(|&limb| limb != 0) {
            return None;
        }
        let low = u128::from(self.0.first().copied().unwrap_or(0));
        let high = u128::from(self.0.get(1).copied().unwrap_or(0));
        Some(low | (high << 64))
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
        assert_eq!(divide(i128::MIN, -1, Rounding::Up), None, "past an i128");
        assert_eq!(
            divide(i128::MIN, 1, Rounding::Up),
            None,
            "its magnitude too"
        );
        assert_eq!(divide(i128::MAX, 1, Rounding::Up), Some(i128::MAX));
    }

    #[test]
    fn a_scaled_division_is_exact_however_wide() {
        // 10^37 × 10^56 / 3 overflows an i128 long before the division,
        // and the result does not fit either.
        assert_eq!(
            divide_shifted(10i128.pow(37), 56, 3, Rounding::HalfEven, 0),
            None
        );
        // ...but trailing zeros may bring it back: 10^37 × 10^20 / 10^20.
        assert_eq!(
            divide_shifted(10i128.pow(37), 20, 10i128.pow(20), Rounding::HalfEven, 20),
            Some((10i128.pow(17), 20))
        );
        // 10000000049999999999999 × 10^20 / 30000000000000000, half even.
        assert_eq!(
            divide_shifted(
                10_000_000_049_999_999_999_999,
                20,
                30_000_000_000_000_000,
                Rounding::HalfEven,
                18
            ),
            Some((33_333_333_499_999_999_999_996_667, 0))
        );
        assert_eq!(
            divide_shifted(-7, 1, 2, Rounding::HalfEven, 1),
            Some((-35, 0))
        );
        assert_eq!(divide_shifted(-1, 0, 4, Rounding::Up, 0), Some((-1, 0)));
        assert_eq!(divide_shifted(5, 0, 2, Rounding::HalfEven, 0), Some((2, 0)));
        assert_eq!(
            divide_shifted(15, 0, 2, Rounding::HalfEven, 0),
            Some((8, 0))
        );
    }
}
