//! Whole numbers as wide as a predicate needs, for the great-circle
//! geometry's exact arithmetic ([`super`]): a sign and a magnitude in
//! 64-bit limbs, the least significant first, never a float. A point's
//! vector has components of about 273 bits, a determinant of three of them
//! about 820, and a point where two edges cross about 1,100; each grows
//! as it needs to, so no sign is ever guessed.

use std::cmp::Ordering;

/// A signed whole number of any size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wide {
    /// Below 0; never set for 0.
    negative: bool,
    /// The magnitude, the least significant limb first, with no leading
    /// zero limb, so each number has one form and `==` compares values.
    limbs: Vec<u64>,
}

impl Wide {
    pub fn zero() -> Wide {
        Wide {
            negative: false,
            limbs: Vec::new(),
        }
    }

    /// A whole number from its sign and magnitude.
    pub fn from_parts(negative: bool, limbs: &[u64]) -> Wide {
        Wide {
            negative,
            limbs: limbs.to_vec(),
        }
        .trimmed()
    }

    pub fn from_i64(n: i64) -> Wide {
        Wide::from_parts(n < 0, &[n.unsigned_abs()])
    }

    /// Leading zero limbs dropped, and 0 never negative.
    fn trimmed(mut self) -> Wide {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
        if self.limbs.is_empty() {
            self.negative = false;
        }
        self
    }

    pub fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    /// -1, 0 or 1.
    pub fn signum(&self) -> i32 {
        if self.limbs.is_empty() {
            0
        } else if self.negative {
            -1
        } else {
            1
        }
    }

    pub fn neg(&self) -> Wide {
        Wide {
            negative: !self.negative && !self.limbs.is_empty(),
            limbs: self.limbs.clone(),
        }
    }

    /// How many bits the magnitude takes: 0 for zero.
    pub fn bits(&self) -> u32 {
        match self.limbs.last() {
            Some(top) => (self.limbs.len() as u32 - 1) * 64 + (64 - top.leading_zeros()),
            None => 0,
        }
    }

    /// The magnitude's limbs from `shift` bits up, as a `u64`: its top bits
    /// once `shift` is its size less 63.
    pub fn bits_from(&self, shift: u32) -> u64 {
        let at = (shift / 64) as usize;
        let within = shift % 64;
        let low = self.limbs.get(at).copied().unwrap_or(0);
        let high = self.limbs.get(at + 1).copied().unwrap_or(0);
        if within == 0 {
            low
        } else {
            (low >> within) | (high << (64 - within))
        }
    }

    pub fn add(&self, other: &Wide) -> Wide {
        if self.negative == other.negative {
            return Wide {
                negative: self.negative,
                limbs: add_magnitudes(&self.limbs, &other.limbs),
            }
            .trimmed();
        }
        match compare_magnitudes(&self.limbs, &other.limbs) {
            Ordering::Equal => Wide::zero(),
            Ordering::Greater => Wide {
                negative: self.negative,
                limbs: sub_magnitudes(&self.limbs, &other.limbs),
            }
            .trimmed(),
            Ordering::Less => Wide {
                negative: other.negative,
                limbs: sub_magnitudes(&other.limbs, &self.limbs),
            }
            .trimmed(),
        }
    }

    pub fn sub(&self, other: &Wide) -> Wide {
        self.add(&other.neg())
    }

    pub fn mul(&self, other: &Wide) -> Wide {
        if self.is_zero() || other.is_zero() {
            return Wide::zero();
        }
        let mut out = vec![0u64; self.limbs.len() + other.limbs.len()];
        for (i, &x) in self.limbs.iter().enumerate() {
            if x == 0 {
                continue;
            }
            let mut carry: u128 = 0;
            for (j, &y) in other.limbs.iter().enumerate() {
                if let Some(slot) = out.get_mut(i + j) {
                    let wide = u128::from(x) * u128::from(y) + u128::from(*slot) + carry;
                    *slot = wide as u64;
                    carry = wide >> 64;
                }
            }
            let mut at = i + other.limbs.len();
            while carry != 0 {
                if let Some(slot) = out.get_mut(at) {
                    let wide = u128::from(*slot) + carry;
                    *slot = wide as u64;
                    carry = wide >> 64;
                    at += 1;
                } else {
                    break;
                }
            }
        }
        Wide {
            negative: self.negative != other.negative,
            limbs: out,
        }
        .trimmed()
    }

    /// `self × 2^bits`.
    pub fn shl(&self, bits: u32) -> Wide {
        if self.is_zero() {
            return Wide::zero();
        }
        let limbs = (bits / 64) as usize;
        let within = bits % 64;
        let mut out = vec![0u64; limbs];
        let mut carry = 0u64;
        for &limb in &self.limbs {
            if within == 0 {
                out.push(limb);
            } else {
                out.push((limb << within) | carry);
                carry = limb >> (64 - within);
            }
        }
        if carry != 0 {
            out.push(carry);
        }
        Wide {
            negative: self.negative,
            limbs: out,
        }
        .trimmed()
    }
}

impl PartialOrd for Wide {
    fn partial_cmp(&self, other: &Wide) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Wide {
    fn cmp(&self, other: &Wide) -> Ordering {
        match (self.negative, other.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => compare_magnitudes(&self.limbs, &other.limbs),
            (true, true) => compare_magnitudes(&other.limbs, &self.limbs),
        }
    }
}

fn compare_magnitudes(a: &[u64], b: &[u64]) -> Ordering {
    a.len()
        .cmp(&b.len())
        .then_with(|| a.iter().rev().cmp(b.iter().rev()))
}

fn add_magnitudes(a: &[u64], b: &[u64]) -> Vec<u64> {
    let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = false;
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        let (sum, over1) = x.overflowing_add(y);
        let (sum, over2) = sum.overflowing_add(u64::from(carry));
        out.push(sum);
        carry = over1 || over2;
    }
    if carry {
        out.push(1);
    }
    out
}

/// `a - b`, `b` never the larger.
fn sub_magnitudes(a: &[u64], b: &[u64]) -> Vec<u64> {
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = false;
    for (i, &x) in a.iter().enumerate() {
        let y = b.get(i).copied().unwrap_or(0);
        let (difference, under1) = x.overflowing_sub(y);
        let (difference, under2) = difference.overflowing_sub(u64::from(borrow));
        out.push(difference);
        borrow = under1 || under2;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(value: i128) -> Wide {
        Wide::from_parts(
            value < 0,
            &[
                value.unsigned_abs() as u64,
                (value.unsigned_abs() >> 64) as u64,
            ],
        )
    }

    #[test]
    fn arithmetic_matches_i128() {
        let values = [
            0i128,
            1,
            -1,
            7,
            -12_345_678_901_234_567,
            u64::MAX as i128,
            -(u64::MAX as i128),
            (1i128 << 100) + 12_345,
            -(1i128 << 90) - 3,
        ];
        for &a in &values {
            for &b in &values {
                assert_eq!(n(a).add(&n(b)), n(a + b), "{a} + {b}");
                assert_eq!(n(a).sub(&n(b)), n(a - b), "{a} - {b}");
                assert_eq!(n(a).cmp(&n(b)), a.cmp(&b), "{a} vs {b}");
                if a.unsigned_abs() < (1 << 62) && b.unsigned_abs() < (1 << 62) {
                    assert_eq!(n(a).mul(&n(b)), n(a * b), "{a} × {b}");
                }
            }
        }
        assert_eq!(n(-5).shl(70), n(-5 * (1 << 70)));
        // (2^64 - 1)^2 = 2^128 - 2^65 + 1, across a limb.
        let big = n(u64::MAX as i128).mul(&n(u64::MAX as i128));
        assert_eq!(big.bits(), 128);
        assert_eq!(big, n(1).shl(128).sub(&n(1).shl(65)).add(&n(1)));
    }
}
