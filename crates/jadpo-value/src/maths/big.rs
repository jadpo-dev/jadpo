//! Whole numbers as wide as `Decimal` maths needs (area 19, "Numbers"):
//! an unsigned [`Big`] of 32-bit limbs and a signed [`Int`] over it. Only
//! integer arithmetic, so every target gives the same digits; nothing here
//! is a binary float. The numbers stay a few hundred digits wide, so the
//! plain schoolbook methods are quick enough.

use std::cmp::Ordering;

/// An unsigned whole number: 32-bit limbs, least significant first, with
/// no zero limb at the top, so zero has none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Big(Vec<u32>);

impl Big {
    pub(crate) fn zero() -> Big {
        Big(Vec::new())
    }

    pub(crate) fn from_u128(n: u128) -> Big {
        Big(vec![
            n as u32,
            (n >> 32) as u32,
            (n >> 64) as u32,
            (n >> 96) as u32,
        ])
        .trimmed()
    }

    /// 10^n.
    pub(crate) fn pow10(n: u32) -> Big {
        let mut power = Big::from_u128(1);
        let mut left = n;
        while left >= 9 {
            power = power.mul_word(1_000_000_000);
            left -= 9;
        }
        power.mul_word(10u32.pow(left))
    }

    fn trimmed(mut self) -> Big {
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
        self
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn is_odd(&self) -> bool {
        self.0.first().is_some_and(|limb| limb & 1 == 1)
    }

    /// The value when it fits a `u128`.
    pub(crate) fn to_u128(&self) -> Option<u128> {
        if self.0.len() > 4 {
            return None;
        }
        Some(
            self.0
                .iter()
                .rev()
                .fold(0u128, |acc, &limb| (acc << 32) | u128::from(limb)),
        )
    }

    /// How many bits it takes: 0 for zero.
    pub(crate) fn bits(&self) -> u64 {
        match self.0.last() {
            None => 0,
            Some(top) => (self.0.len() as u64 - 1) * 32 + u64::from(32 - top.leading_zeros()),
        }
    }

    fn bit(&self, at: u64) -> bool {
        self.0
            .get((at / 32) as usize)
            .is_some_and(|limb| (limb >> (at % 32)) & 1 == 1)
    }

    pub(crate) fn add(&self, other: &Big) -> Big {
        let (long, short) = if self.0.len() >= other.0.len() {
            (self, other)
        } else {
            (other, self)
        };
        let mut out = Vec::with_capacity(long.0.len() + 1);
        let mut carry = 0u64;
        let mut rest = short.0.iter();
        for &limb in &long.0 {
            let sum = u64::from(limb) + u64::from(rest.next().copied().unwrap_or(0)) + carry;
            out.push(sum as u32);
            carry = sum >> 32;
        }
        if carry != 0 {
            out.push(carry as u32);
        }
        Big(out).trimmed()
    }

    /// `self - other`; `other` is never the larger.
    pub(crate) fn sub(&self, other: &Big) -> Big {
        let mut out = self.clone();
        out.sub_assign(other);
        out
    }

    fn sub_assign(&mut self, other: &Big) {
        let mut borrow = 0i64;
        let mut rest = other.0.iter();
        for limb in &mut self.0 {
            let mut difference =
                i64::from(*limb) - i64::from(rest.next().copied().unwrap_or(0)) - borrow;
            borrow = 0;
            if difference < 0 {
                difference += 1 << 32;
                borrow = 1;
            }
            *limb = difference as u32;
        }
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
    }

    pub(crate) fn mul(&self, other: &Big) -> Big {
        if self.is_zero() || other.is_zero() {
            return Big::zero();
        }
        let mut out = vec![0u32; self.0.len() + other.0.len()];
        for (at, &a) in self.0.iter().enumerate() {
            let mut carry = 0u64;
            let mut slots = out.iter_mut().skip(at);
            for &b in &other.0 {
                if let Some(slot) = slots.next() {
                    let wide = u64::from(a) * u64::from(b) + u64::from(*slot) + carry;
                    *slot = wide as u32;
                    carry = wide >> 32;
                }
            }
            for slot in slots {
                if carry == 0 {
                    break;
                }
                let sum = u64::from(*slot) + carry;
                *slot = sum as u32;
                carry = sum >> 32;
            }
        }
        Big(out).trimmed()
    }

    pub(crate) fn mul_word(&self, by: u32) -> Big {
        let mut out = Vec::with_capacity(self.0.len() + 1);
        let mut carry = 0u64;
        for &limb in &self.0 {
            let wide = u64::from(limb) * u64::from(by) + carry;
            out.push(wide as u32);
            carry = wide >> 32;
        }
        if carry != 0 {
            out.push(carry as u32);
        }
        Big(out).trimmed()
    }

    /// The quotient and remainder by a nonzero `divisor`.
    pub(crate) fn div_word(&self, divisor: u32) -> (Big, u32) {
        let mut out = vec![0u32; self.0.len()];
        let mut remainder = 0u64;
        for (limb, slot) in self.0.iter().zip(out.iter_mut()).rev() {
            let current = (remainder << 32) | u64::from(*limb);
            *slot = (current / u64::from(divisor)) as u32;
            remainder = current % u64::from(divisor);
        }
        (Big(out).trimmed(), remainder as u32)
    }

    /// The quotient and remainder by a nonzero `divisor`, a bit at a time.
    pub(crate) fn div_rem(&self, divisor: &Big) -> (Big, Big) {
        if self < divisor {
            return (Big::zero(), self.clone());
        }
        let mut quotient = vec![0u32; self.0.len()];
        let mut remainder = Big::zero();
        for at in (0..self.bits()).rev() {
            remainder.shift_in(self.bit(at));
            if &remainder >= divisor {
                remainder.sub_assign(divisor);
                if let Some(limb) = quotient.get_mut((at / 32) as usize) {
                    *limb |= 1 << (at % 32);
                }
            }
        }
        (Big(quotient).trimmed(), remainder)
    }

    /// `self × 2 + bit`.
    fn shift_in(&mut self, bit: bool) {
        let mut carry = u32::from(bit);
        for limb in &mut self.0 {
            let next = *limb >> 31;
            *limb = (*limb << 1) | carry;
            carry = next;
        }
        if carry != 0 {
            self.0.push(carry);
        }
    }

    /// `self × 2^bits`.
    pub(crate) fn shl(&self, bits: u64) -> Big {
        if self.is_zero() {
            return Big::zero();
        }
        let limbs = (bits / 32) as usize;
        let shift = (bits % 32) as u32;
        let mut out = vec![0u32; limbs];
        let mut carry = 0u32;
        for &limb in &self.0 {
            out.push(if shift == 0 {
                limb
            } else {
                (limb << shift) | carry
            });
            carry = if shift == 0 { 0 } else { limb >> (32 - shift) };
        }
        if carry != 0 {
            out.push(carry);
        }
        Big(out).trimmed()
    }

    /// The whole part of the square root.
    pub(crate) fn isqrt(&self) -> Big {
        if self.is_zero() {
            return Big::zero();
        }
        // Newton's method from above: 2^⌈bits/2⌉ is at least the root, and
        // each step falls until it would rise.
        let mut root = Big::from_u128(1).shl(self.bits().div_ceil(2));
        loop {
            let (quotient, _) = self.div_rem(&root);
            let (next, _) = root.add(&quotient).div_word(2);
            if next >= root {
                return root;
            }
            root = next;
        }
    }
}

impl PartialOrd for Big {
    fn partial_cmp(&self, other: &Big) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Big {
    fn cmp(&self, other: &Big) -> Ordering {
        self.0
            .len()
            .cmp(&other.0.len())
            .then_with(|| self.0.iter().rev().cmp(other.0.iter().rev()))
    }
}

/// A signed whole number: a sign and a magnitude, zero never negative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Int {
    negative: bool,
    magnitude: Big,
}

impl Int {
    pub(crate) fn new(negative: bool, magnitude: Big) -> Int {
        Int {
            negative: negative && !magnitude.is_zero(),
            magnitude,
        }
    }

    pub(crate) fn from_big(magnitude: Big) -> Int {
        Int::new(false, magnitude)
    }

    pub(crate) fn from_i128(n: i128) -> Int {
        Int::new(n < 0, Big::from_u128(n.unsigned_abs()))
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.magnitude.is_zero()
    }

    pub(crate) fn is_negative(&self) -> bool {
        self.negative
    }

    pub(crate) fn magnitude(&self) -> &Big {
        &self.magnitude
    }

    pub(crate) fn neg(&self) -> Int {
        Int::new(!self.negative, self.magnitude.clone())
    }

    pub(crate) fn add(&self, other: &Int) -> Int {
        if self.negative == other.negative {
            return Int::new(self.negative, self.magnitude.add(&other.magnitude));
        }
        match self.magnitude.cmp(&other.magnitude) {
            Ordering::Less => Int::new(other.negative, other.magnitude.sub(&self.magnitude)),
            _ => Int::new(self.negative, self.magnitude.sub(&other.magnitude)),
        }
    }

    pub(crate) fn sub(&self, other: &Int) -> Int {
        self.add(&other.neg())
    }

    pub(crate) fn mul(&self, other: &Int) -> Int {
        Int::new(
            self.negative != other.negative,
            self.magnitude.mul(&other.magnitude),
        )
    }

    pub(crate) fn mul_big(&self, other: &Big) -> Int {
        Int::new(self.negative, self.magnitude.mul(other))
    }

    pub(crate) fn mul_word(&self, by: u32) -> Int {
        Int::new(self.negative, self.magnitude.mul_word(by))
    }

    /// `self / divisor`, the remainder dropped: toward zero.
    pub(crate) fn div_word(&self, divisor: u32) -> Int {
        Int::new(self.negative, self.magnitude.div_word(divisor).0)
    }

    /// `self / divisor`, toward zero.
    pub(crate) fn div_big(&self, divisor: &Big) -> Int {
        Int::new(self.negative, self.magnitude.div_rem(divisor).0)
    }

    /// `self / divisor`, toward zero.
    pub(crate) fn div(&self, divisor: &Int) -> Int {
        Int::new(
            self.negative != divisor.negative,
            self.magnitude.div_rem(&divisor.magnitude).0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn big(n: u128) -> Big {
        Big::from_u128(n)
    }

    #[test]
    fn arithmetic_agrees_with_u128() {
        let pairs = [
            (0u128, 1u128),
            (1, 1),
            (u128::from(u32::MAX), 1),
            (u128::from(u64::MAX), u128::from(u64::MAX) - 7),
            (123_456_789_012_345_678_901_234_567, 987_654_321),
            (10u128.pow(37), 3),
        ];
        for (a, b) in pairs {
            assert_eq!(big(a).add(&big(b)).to_u128(), Some(a + b));
            assert_eq!(
                big(a.max(b)).sub(&big(a.min(b))).to_u128(),
                Some(a.max(b) - a.min(b))
            );
            if let Some(wide) = a.checked_mul(b) {
                assert_eq!(big(a).mul(&big(b)).to_u128(), Some(wide));
            }
            let (quotient, remainder) = big(a).div_rem(&big(b));
            assert_eq!(quotient.to_u128(), Some(a / b), "{a} / {b}");
            assert_eq!(remainder.to_u128(), Some(a % b));
            assert_eq!(big(a).cmp(&big(b)), a.cmp(&b));
        }
        assert_eq!(big(1000).div_word(7), (big(142), 6));
        assert_eq!(big(5).shl(100).to_u128(), Some(5 << 100));
        assert_eq!(Big::pow10(38).to_u128(), Some(10u128.pow(38)));
        assert_eq!(big(1).bits(), 1);
        assert_eq!(big(0).bits(), 0);
        assert_eq!(big(u128::MAX).bits(), 128);
    }

    #[test]
    fn wide_products_divide_back() {
        let a = Big::pow10(70).add(&big(12_345));
        let b = Big::pow10(45).add(&big(678));
        let (quotient, remainder) = a.mul(&b).add(&big(99)).div_rem(&b);
        assert_eq!(quotient, a);
        assert_eq!(remainder, big(99));
    }

    #[test]
    fn square_roots_are_whole_parts() {
        for n in [0u128, 1, 2, 3, 4, 15, 16, 17, 99, 100, 10u128.pow(36) - 1] {
            let root = big(n).isqrt().to_u128().unwrap_or(u128::MAX);
            assert!(root * root <= n && (root + 1) * (root + 1) > n, "{n}");
        }
        let wide = Big::pow10(80);
        assert_eq!(wide.isqrt(), Big::pow10(40));
    }

    #[test]
    fn signed_arithmetic_keeps_its_sign() {
        let int = Int::from_i128;
        assert_eq!(int(-7).add(&int(3)), int(-4));
        assert_eq!(int(7).sub(&int(9)), int(-2));
        assert_eq!(int(-7).mul(&int(-3)), int(21));
        assert_eq!(int(-7).div_word(2), int(-3), "toward zero");
        assert_eq!(int(-7).div(&int(2)), int(-3));
        assert!(int(3).sub(&int(3)).is_zero());
        assert!(!int(0).neg().is_negative(), "zero is never negative");
    }
}
