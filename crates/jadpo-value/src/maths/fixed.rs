//! Fixed-point real numbers for the geodesics (area 20, "Precision and
//! determinism"): a [`Real`] is a 256-bit two's-complement whole number
//! `x × 2^136`, so every step is whole-number arithmetic on the stack and
//! every target gives the same digits. Each product and quotient is cut
//! toward zero, under a unit of the last place (2⁻¹³⁶, about 10⁻⁴¹) off;
//! `sqrt` and `hypot` are whole square roots, and `sin`, `cos` and
//! `atan2` are Taylor's and Euler's series, as 5.5e's are, run on working
//! values. A value is decimal only where it comes in, a coordinate read
//! to the last place, and where a result goes out ([`Real::rounded`],
//! rounded half to even from the exact binary value).

use crate::value::Decimal;
use jadpo_std::Rounding;
use std::cmp::Ordering;

/// The bits after the point: 136, about 41 decimal places, so a
/// coordinate's places are kept to within 10⁻⁴¹ of a degree, and a
/// geodesic's few thousand truncations leave its metres and square metres
/// far below the micrometre and the square millimetre its results keep.
/// The 119 bits before the point hold the ellipsoid's area in square
/// metres (5 × 10¹⁴) with room to spare.
pub(crate) const FRACTION: u32 = 136;

const LIMBS: usize = 4;
const WIDE: usize = 2 * LIMBS;

/// A real number: `x × 2^FRACTION` in 256 bits, two's complement, the
/// least significant 64 bits first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Real([u64; LIMBS]);

impl PartialOrd for Real {
    fn partial_cmp(&self, other: &Real) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Real {
    fn cmp(&self, other: &Real) -> Ordering {
        // Two's complement orders as unsigned once the sign bit is flipped.
        let flip = |limbs: &[u64; LIMBS]| {
            let mut out = *limbs;
            if let Some(top) = out.last_mut() {
                *top ^= 1 << 63;
            }
            out
        };
        order(&flip(&self.0), &flip(&other.0))
    }
}

/// Limb `i` of `x`, 0 past its end.
fn at(x: &[u64], i: usize) -> u64 {
    x.get(i).copied().unwrap_or(0)
}

/// Sets limb `i` of `x`; past its end, nothing.
fn put(x: &mut [u64], i: usize, value: u64) {
    if let Some(slot) = x.get_mut(i) {
        *slot = value;
    }
}

/// Two whole numbers of one length compared, the most significant limb
/// first.
fn order(a: &[u64], b: &[u64]) -> Ordering {
    a.iter().rev().cmp(b.iter().rev())
}

/// How many limbs `x` has without its leading zeros.
fn length(x: &[u64]) -> usize {
    x.iter()
        .rposition(|&limb| limb != 0)
        .map_or(0, |top| top + 1)
}

/// How many bits `x` takes: 0 for zero.
fn bits(x: &[u64]) -> u32 {
    match length(x) {
        0 => 0,
        n => (n as u32 - 1) * 64 + (64 - at(x, n - 1).leading_zeros()),
    }
}

/// `a × b`, all 512 bits.
fn mul_wide(a: &[u64; LIMBS], b: &[u64; LIMBS]) -> [u64; WIDE] {
    let mut out = [0u64; WIDE];
    for (i, &x) in a.iter().enumerate() {
        if x == 0 {
            continue;
        }
        let mut carry: u128 = 0;
        for (j, &y) in b.iter().enumerate() {
            let wide = u128::from(x) * u128::from(y) + u128::from(at(&out, i + j)) + carry;
            put(&mut out, i + j, wide as u64);
            carry = wide >> 64;
        }
        put(&mut out, i + LIMBS, carry as u64);
    }
    out
}

/// `x × 2^shift`, the bits past 512 dropped.
fn shl_wide(x: &[u64], shift: u32) -> [u64; WIDE] {
    let limbs = (shift / 64) as usize;
    let within = shift % 64;
    let mut out = [0u64; WIDE];
    for (i, slot) in out.iter_mut().enumerate().skip(limbs) {
        let here = at(x, i - limbs);
        let below = if within == 0 || i == limbs {
            0
        } else {
            at(x, i - limbs - 1) >> (64 - within)
        };
        *slot = (here << within) | below;
    }
    out
}

/// `x / 2^shift`, toward zero.
fn shr_wide(x: &[u64; WIDE], shift: u32) -> [u64; WIDE] {
    let limbs = (shift / 64) as usize;
    let within = shift % 64;
    let mut out = [0u64; WIDE];
    for (i, slot) in out.iter_mut().enumerate() {
        let low = at(x, i + limbs);
        let high = at(x, i + limbs + 1);
        *slot = if within == 0 {
            low
        } else {
            (low >> within) | (high << (64 - within))
        };
    }
    out
}

/// The low 256 bits of a wide number; the rest is never set for a value
/// a geodesic reaches.
fn narrow(x: &[u64]) -> [u64; LIMBS] {
    [at(x, 0), at(x, 1), at(x, 2), at(x, 3)]
}

/// `x` widened.
fn wide(x: &[u64]) -> [u64; WIDE] {
    let mut out = [0u64; WIDE];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = at(x, i);
    }
    out
}

/// `a + b`, the carry out of the top dropped.
fn add_limbs<const N: usize>(a: &[u64; N], b: &[u64; N]) -> [u64; N] {
    let mut out = [0u64; N];
    let mut carry = false;
    for ((slot, &x), &y) in out.iter_mut().zip(a).zip(b) {
        let (sum, over1) = x.overflowing_add(y);
        let (sum, over2) = sum.overflowing_add(u64::from(carry));
        *slot = sum;
        carry = over1 || over2;
    }
    out
}

/// `a - b`, `b` never the larger.
fn sub_limbs<const N: usize>(a: &[u64; N], b: &[u64; N]) -> [u64; N] {
    let mut out = [0u64; N];
    let mut borrow = false;
    for ((slot, &x), &y) in out.iter_mut().zip(a).zip(b) {
        let (difference, under1) = x.overflowing_sub(y);
        let (difference, under2) = difference.overflowing_sub(u64::from(borrow));
        *slot = difference;
        borrow = under1 || under2;
    }
    out
}

/// `n / d` for a nonzero `d` of at most 4 limbs: Knuth's long division
/// (The Art of Computer Programming, vol. 2, §4.3.1, algorithm D), with
/// 64-bit limbs, each quotient limb guessed from the top two and
/// corrected at most twice.
fn div_wide(n: &[u64; WIDE], d: &[u64]) -> [u64; WIDE] {
    let mut quotient = [0u64; WIDE];
    let dlen = length(d);
    if dlen == 0 || dlen > LIMBS {
        return quotient;
    }
    if dlen == 1 {
        let divisor = u128::from(at(d, 0));
        let mut rest: u128 = 0;
        for (slot, &limb) in quotient.iter_mut().zip(n).rev() {
            let current = (rest << 64) | u128::from(limb);
            *slot = (current / divisor) as u64;
            rest = current % divisor;
        }
        return quotient;
    }
    // Both shifted left until the divisor's top bit is set.
    let shift = at(d, dlen - 1).leading_zeros();
    let v = narrow(&shl_wide(d, shift));
    let mut u = [0u64; WIDE + 1];
    let shifted = shl_wide(n, shift);
    for (slot, &limb) in u.iter_mut().zip(&shifted) {
        *slot = limb;
    }
    put(
        &mut u,
        WIDE,
        if shift == 0 {
            0
        } else {
            at(n, WIDE - 1) >> (64 - shift)
        },
    );
    let ulen = length(n) + 1;
    if ulen <= dlen {
        return quotient;
    }
    let base: u128 = 1 << 64;
    let (v1, v2) = (u128::from(at(&v, dlen - 1)), u128::from(at(&v, dlen - 2)));
    for j in (0..ulen - dlen).rev() {
        let numerator = (u128::from(at(&u, j + dlen)) << 64) | u128::from(at(&u, j + dlen - 1));
        let mut guess = numerator / v1;
        let mut rest = numerator % v1;
        while guess >= base || guess * v2 > ((rest << 64) | u128::from(at(&u, j + dlen - 2))) {
            guess -= 1;
            rest += v1;
            if rest >= base {
                break;
            }
        }
        // u[j..=j+dlen] -= guess × v; a borrow out of the top means the
        // guess was one too many.
        let mut borrow = false;
        let mut carry: u128 = 0;
        for (i, &limb) in v.iter().enumerate().take(dlen) {
            let product = guess * u128::from(limb) + carry;
            carry = product >> 64;
            let (difference, under1) = at(&u, i + j).overflowing_sub(product as u64);
            let (difference, under2) = difference.overflowing_sub(u64::from(borrow));
            put(&mut u, i + j, difference);
            borrow = under1 || under2;
        }
        let (difference, under1) = at(&u, j + dlen).overflowing_sub(carry as u64);
        let (difference, under2) = difference.overflowing_sub(u64::from(borrow));
        put(&mut u, j + dlen, difference);
        if under1 || under2 {
            guess -= 1;
            let mut carry = false;
            for (i, &limb) in v.iter().enumerate().take(dlen) {
                let (sum, over1) = at(&u, i + j).overflowing_add(limb);
                let (sum, over2) = sum.overflowing_add(u64::from(carry));
                put(&mut u, i + j, sum);
                carry = over1 || over2;
            }
            let top = at(&u, j + dlen).wrapping_add(u64::from(carry));
            put(&mut u, j + dlen, top);
        }
        put(&mut quotient, j, guess as u64);
    }
    quotient
}

/// The whole part of the square root of `n`: Newton's method from just
/// above, its first guess from `n`'s top 126 bits, so two or three steps
/// finish it.
fn isqrt_wide(n: &[u64; WIDE]) -> [u64; WIDE] {
    let size = bits(n);
    if size == 0 {
        return [0u64; WIDE];
    }
    let shift = size.saturating_sub(126).next_multiple_of(2);
    let top = shr_wide(n, shift);
    let top = u128::from(at(&top, 0)) | (u128::from(at(&top, 1)) << 64);
    let first = top.isqrt() + 1;
    let mut root = shl_wide(&[first as u64, (first >> 64) as u64], shift / 2);
    loop {
        let quotient = div_wide(n, &narrow(&root));
        let next = shr_wide(&add_limbs(&root, &quotient), 1);
        if order(&next, &root) != Ordering::Less {
            return root;
        }
        root = next;
    }
}

/// `x × n` for a small `n`.
fn mul_small(x: &[u64; LIMBS], n: u64) -> [u64; WIDE] {
    let mut out = [0u64; WIDE];
    let mut carry: u128 = 0;
    for (slot, &limb) in out.iter_mut().zip(x) {
        let wide = u128::from(limb) * u128::from(n) + carry;
        *slot = wide as u64;
        carry = wide >> 64;
    }
    put(&mut out, LIMBS, carry as u64);
    out
}

impl Real {
    pub(crate) fn zero() -> Real {
        Real([0; LIMBS])
    }

    /// The smallest positive value, a unit of the last place: what
    /// GeographicLib's `tiny_` stands in for, a value that is not zero
    /// but too small to change any sum.
    pub(crate) fn tiny() -> Real {
        Real([1, 0, 0, 0])
    }

    /// The smallest value with the sign of `self`, or 0: GeographicLib's
    /// `tiny_ * x` for an `x` below 1.
    pub(crate) fn tiny_as(&self) -> Real {
        match self.signum() {
            1 => Real::tiny(),
            -1 => Real::tiny().neg(),
            _ => Real::zero(),
        }
    }

    pub(crate) fn is_negative(&self) -> bool {
        at(&self.0, LIMBS - 1) >> 63 == 1
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.0 == [0; LIMBS]
    }

    /// -1, 0 or 1.
    pub(crate) fn signum(&self) -> i32 {
        if self.is_zero() {
            0
        } else if self.is_negative() {
            -1
        } else {
            1
        }
    }

    pub(crate) fn add(&self, other: &Real) -> Real {
        Real(add_limbs(&self.0, &other.0))
    }

    pub(crate) fn neg(&self) -> Real {
        let mut out = [0u64; LIMBS];
        let mut carry = true;
        for (slot, &limb) in out.iter_mut().zip(&self.0) {
            let (sum, over) = (!limb).overflowing_add(u64::from(carry));
            *slot = sum;
            carry = over;
        }
        Real(out)
    }

    pub(crate) fn sub(&self, other: &Real) -> Real {
        self.add(&other.neg())
    }

    pub(crate) fn abs(&self) -> Real {
        if self.is_negative() {
            self.neg()
        } else {
            *self
        }
    }

    /// The size, without the sign.
    fn magnitude(&self) -> [u64; LIMBS] {
        self.abs().0
    }

    /// A value from its size and sign.
    fn signed(negative: bool, magnitude: [u64; LIMBS]) -> Real {
        let value = Real(magnitude);
        if negative { value.neg() } else { value }
    }

    /// `self × n`.
    pub(crate) fn times(&self, n: i64) -> Real {
        let product = narrow(&mul_small(&self.magnitude(), n.unsigned_abs()));
        Real::signed(self.is_negative() != (n < 0), product)
    }

    /// `self / n`, cut toward zero; 0 for an `n` of 0.
    pub(crate) fn div_whole(&self, n: i64) -> Real {
        if n == 0 {
            return Real::zero();
        }
        let quotient = div_wide(&wide(&self.magnitude()), &[n.unsigned_abs()]);
        Real::signed(self.is_negative() != (n < 0), narrow(&quotient))
    }

    /// The larger of the two.
    pub(crate) fn max(self, other: Real) -> Real {
        if other > self { other } else { self }
    }

    /// The value rounded to `scale` places by `rounding`, as a `Decimal`'s
    /// mantissa: exact, the binary value being a fraction over a power of
    /// two; `None` past 38 digits.
    pub(crate) fn rounded(&self, scale: u32, rounding: Rounding) -> Option<i128> {
        let mut in_places = wide(&self.magnitude());
        for _ in 0..scale.min(Decimal::MAX_DIGITS) {
            let mut carry: u128 = 0;
            for limb in &mut in_places {
                let product = u128::from(*limb) * 10 + carry;
                *limb = product as u64;
                carry = product >> 64;
            }
        }
        let whole = shr_wide(&in_places, FRACTION);
        let fraction = sub_limbs(&in_places, &shl_wide(&whole, FRACTION));
        let half = shl_wide(&[1], FRACTION - 1);
        let against_half = order(&fraction, &half);
        let odd = at(&whole, 0) & 1 == 1;
        let exact = fraction == [0u64; WIDE];
        let step = !exact
            && match rounding {
                Rounding::Up => true,
                Rounding::Down => false,
                Rounding::HalfUp => against_half != Ordering::Less,
                Rounding::HalfEven => {
                    against_half == Ordering::Greater || (against_half == Ordering::Equal && odd)
                }
            };
        if length(&whole) > 2 {
            return None;
        }
        let magnitude =
            (u128::from(at(&whole, 0)) | (u128::from(at(&whole, 1)) << 64)) + u128::from(step);
        let magnitude = i128::try_from(magnitude).ok()?;
        if magnitude >= 10i128.pow(Decimal::MAX_DIGITS) {
            return None;
        }
        Some(if self.is_negative() {
            -magnitude
        } else {
            magnitude
        })
    }
}

/// The working constants the functions need, worked out once: π, π/2,
/// and the arctangents of `k/16` that bring an arctangent's argument
/// below 1/32.
pub(crate) struct Unit {
    pi: Real,
    half_pi: Real,
    arctangents: Vec<Real>,
}

impl Unit {
    pub(crate) fn new() -> Unit {
        let pi = machin();
        let mut unit = Unit {
            pi,
            half_pi: pi.div_whole(2),
            arctangents: Vec::new(),
        };
        unit.arctangents = (0..=16).map(|k| unit.atan(&unit.ratio(k, 16))).collect();
        unit
    }

    pub(crate) fn pi(&self) -> &Real {
        &self.pi
    }

    /// A whole number.
    pub(crate) fn int(&self, n: i64) -> Real {
        Real::signed(n < 0, narrow(&shl_wide(&[n.unsigned_abs()], FRACTION)))
    }

    /// `numerator / denominator`, cut toward zero.
    pub(crate) fn ratio(&self, numerator: i64, denominator: i64) -> Real {
        self.int(numerator).div_whole(denominator)
    }

    /// A decimal of at most 180 in size, to within a unit of the last
    /// place: its whole part exactly, its fraction cut toward zero, its
    /// sign kept, so a coordinate and its negation are each other's
    /// negation exactly.
    pub(crate) fn decimal(&self, value: Decimal) -> Real {
        let size = value.mantissa.unsigned_abs();
        let power = 10u128.pow(value.scale.min(Decimal::MAX_DIGITS));
        let (whole, rest) = (size / power, size % power);
        let whole = Real(narrow(&shl_wide(
            &[whole as u64, (whole >> 64) as u64],
            FRACTION,
        )));
        let numerator = shl_wide(&[rest as u64, (rest >> 64) as u64], FRACTION);
        let fraction = div_wide(&numerator, &[power as u64, (power >> 64) as u64]);
        let total = whole.add(&Real(narrow(&fraction)));
        Real::signed(value.mantissa < 0, total.0)
    }

    pub(crate) fn mul(&self, a: &Real, b: &Real) -> Real {
        let product = mul_wide(&a.magnitude(), &b.magnitude());
        Real::signed(
            a.is_negative() != b.is_negative(),
            narrow(&shr_wide(&product, FRACTION)),
        )
    }

    /// `a / b`; 0 for a `b` of 0, which no caller divides by.
    pub(crate) fn div(&self, a: &Real, b: &Real) -> Real {
        if b.is_zero() {
            return Real::zero();
        }
        let quotient = div_wide(&shl_wide(&a.magnitude(), FRACTION), &b.magnitude());
        Real::signed(a.is_negative() != b.is_negative(), narrow(&quotient))
    }

    pub(crate) fn sq(&self, a: &Real) -> Real {
        self.mul(a, a)
    }

    /// The square root of `a`, 0 for one below 0.
    pub(crate) fn sqrt(&self, a: &Real) -> Real {
        if a.is_negative() {
            return Real::zero();
        }
        Real(narrow(&isqrt_wide(&shl_wide(&a.0, FRACTION))))
    }

    /// `√(a² + b²)`, from the whole square root of the exact sum.
    pub(crate) fn hypot(&self, a: &Real, b: &Real) -> Real {
        let (a, b) = (a.magnitude(), b.magnitude());
        Real(narrow(&isqrt_wide(&add_limbs(
            &mul_wide(&a, &a),
            &mul_wide(&b, &b),
        ))))
    }

    /// The cube root of `a`, its sign kept: Newton's method from a power
    /// of two above it, `x ← (2x + a/x²) / 3`, until it stops falling.
    pub(crate) fn cbrt(&self, a: &Real) -> Real {
        let m = a.abs();
        let size = bits(&m.0);
        if size == 0 {
            return Real::zero();
        }
        // m < 2^(size - FRACTION), so 2^⌈(size - FRACTION) / 3⌉ + 1 is above
        // its cube root.
        let exponent = (i64::from(size) - i64::from(FRACTION)).div_euclid(3) + 2;
        let shift = u32::try_from(i64::from(FRACTION) + exponent).unwrap_or(FRACTION);
        let mut root = Real(narrow(&shl_wide(&[1], shift)));
        loop {
            let next = root
                .times(2)
                .add(&self.div(&self.div(&m, &root), &root))
                .div_whole(3);
            if next >= root {
                break;
            }
            root = next;
        }
        if a.is_negative() { root.neg() } else { root }
    }

    /// `(s, c) / √(s² + c²)`: a sine and a cosine from any two values
    /// proportional to them.
    pub(crate) fn norm(&self, s: &Real, c: &Real) -> (Real, Real) {
        let r = self.hypot(s, c);
        (self.div(s, &r), self.div(c, &r))
    }

    /// The sine and cosine of `x` radians: `x` brought within π/4 of 0 by
    /// whole quarter turns, then eighthed, Taylor's series to the last
    /// place, and doubled back three times.
    pub(crate) fn sin_cos(&self, x: &Real) -> (Real, Real) {
        if x.is_zero() {
            return (Real::zero(), self.int(1));
        }
        let half = self.half_pi.div_whole(2);
        let toward = if x.is_negative() {
            x.sub(&half)
        } else {
            x.add(&half)
        };
        let turns = self.div(&toward, &self.half_pi);
        let k = i64::try_from(turns.rounded(0, Rounding::Down).unwrap_or(0)).unwrap_or(0);
        let small = x.sub(&self.half_pi.times(k)).div_whole(8);
        let square = self.sq(&small);
        let mut term = small;
        let mut sin = small;
        let mut n: i64 = 2;
        loop {
            term = self.mul(&term, &square).div_whole(n * (n + 1)).neg();
            if term.is_zero() {
                break;
            }
            sin = sin.add(&term);
            n += 2;
        }
        let one = self.int(1);
        let mut term = one;
        let mut cos = one;
        let mut n: i64 = 1;
        loop {
            term = self.mul(&term, &square).div_whole(n * (n + 1)).neg();
            if term.is_zero() {
                break;
            }
            cos = cos.add(&term);
            n += 2;
        }
        for _ in 0..3 {
            let doubled = self.mul(&sin, &cos).times(2);
            cos = one.sub(&self.sq(&sin).times(2));
            sin = doubled;
        }
        match k.rem_euclid(4) {
            0 => (sin, cos),
            1 => (cos, sin.neg()),
            2 => (sin.neg(), cos.neg()),
            _ => (cos.neg(), sin),
        }
    }

    /// `atan(t)` for `t` from 0 to 1: the angle halved, `t / (1 + √(1 +
    /// t²))`, until `t` is at most 1/8, then the series, doubled back.
    fn atan(&self, t: &Real) -> Real {
        let one = self.int(1);
        let eighth = self.ratio(1, 8);
        let mut t = *t;
        let mut halvings = 0;
        while t > eighth {
            t = self.div(&t, &one.add(&self.hypot(&one, &t)));
            halvings += 1;
        }
        self.atan_series(&t).times(1 << halvings)
    }

    /// `atan(t)` by Euler's series `t - t³/3 + t⁵/5 - ...`, for a small `t`.
    fn atan_series(&self, t: &Real) -> Real {
        let square = self.sq(t);
        let mut power = *t;
        let mut sum = *t;
        let mut odd: i64 = 3;
        loop {
            power = self.mul(&power, &square).neg();
            let term = power.div_whole(odd);
            if term.is_zero() {
                return sum;
            }
            sum = sum.add(&term);
            odd += 2;
        }
    }

    /// The angle from the positive `x` axis to (`x`, `y`), in radians from
    /// -π to π: π on the negative `x` axis, 0 at the origin. The lesser
    /// of `|x|` and `|y|` over the greater, `t` at most 1, is taken from
    /// the nearest `k/16`: `atan t = atan(k/16) + atan((t - k/16) / (1 +
    /// t k/16))`, the second's argument below 1/32.
    pub(crate) fn atan2(&self, y: &Real, x: &Real) -> Real {
        match (y.signum(), x.signum()) {
            (0, 0 | 1) => return Real::zero(),
            (0, _) => return self.pi,
            (1, 0) => return self.half_pi,
            (_, 0) => return self.half_pi.neg(),
            _ => {}
        }
        let (a, b) = (y.abs(), x.abs());
        let steep = a > b;
        let (lesser, greater) = if steep { (b, a) } else { (a, b) };
        let t = self.div(&lesser, &greater);
        let nearest = t.times(16).add(&self.ratio(1, 2));
        let k = nearest.rounded(0, Rounding::Down).unwrap_or(0).clamp(0, 16);
        let c = self.ratio(k as i64, 16);
        let remainder_t = self.div(&t.sub(&c), &self.int(1).add(&self.mul(&t, &c)));
        let base = self
            .arctangents
            .get(k as usize)
            .copied()
            .unwrap_or_else(Real::zero);
        let mut angle = base.add(&self.atan_series(&remainder_t));
        if steep {
            angle = self.half_pi.sub(&angle);
        }
        if x.is_negative() {
            angle = self.pi.sub(&angle);
        }
        if y.is_negative() { angle.neg() } else { angle }
    }

    /// `p[0] x^N + ... + p[N]` by Horner's rule, `p` whole numbers, the
    /// sum divided by `divisor`: how GeographicLib writes its series'
    /// coefficients, each polynomial exact in whole numbers over one
    /// divisor.
    pub(crate) fn polynomial(&self, p: &[i64], x: &Real, divisor: i64) -> Real {
        let mut sum = Real::zero();
        for &coefficient in p {
            sum = self.mul(&sum, x).add(&self.int(coefficient));
        }
        sum.div_whole(divisor)
    }

    /// `p[0] x^N + ... + p[N]`, each `p` a working value.
    pub(crate) fn horner(&self, p: &[Real], x: &Real) -> Real {
        let mut sum = Real::zero();
        for coefficient in p {
            sum = self.mul(&sum, x).add(coefficient);
        }
        sum
    }
}

/// π by Machin's formula, `16 atan(1/5) - 4 atan(1/239)`, each series's
/// terms found by whole division of the last.
fn machin() -> Real {
    let atan_inverse = |n: i64| {
        let mut power = Real(narrow(&shl_wide(&[1], FRACTION))).div_whole(n);
        let mut sum = power;
        let mut k: i64 = 1;
        loop {
            power = power.div_whole(n * n);
            let term = power.div_whole(2 * k + 1);
            if term.is_zero() {
                return sum;
            }
            sum = if k % 2 == 1 {
                sum.sub(&term)
            } else {
                sum.add(&term)
            };
            k += 1;
        }
    };
    atan_inverse(5).times(16).sub(&atan_inverse(239).times(4))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(value: &Real, places: u32) -> String {
        let mantissa = value.rounded(places, Rounding::HalfEven).unwrap_or(0);
        Decimal::new(mantissa, places).to_string()
    }

    #[test]
    fn the_functions_agree_with_known_digits() {
        // Python's `decimal` module at 80 digits.
        let unit = Unit::new();
        let one = unit.int(1);
        assert_eq!(
            shown(unit.pi(), 37),
            "3.1415926535897932384626433832795028842"
        );
        let (sin, cos) = unit.sin_cos(&one);
        assert_eq!(shown(&sin, 37), "0.8414709848078965066525023216302989996");
        assert_eq!(shown(&cos, 37), "0.5403023058681397174009366074429766037");
        let (sin, cos) = unit.sin_cos(&unit.int(-4));
        assert_eq!(shown(&sin, 37), "0.7568024953079282513726390945118290941");
        assert_eq!(shown(&cos, 37), "-0.6536436208636119146391681830977503814");
        assert_eq!(
            shown(&unit.atan2(&one, &one), 37),
            "0.7853981633974483096156608458198757210"
        );
        assert_eq!(
            shown(&unit.atan2(&unit.int(-3), &unit.int(-2)), 37),
            "-2.1587989303424641704769327722648368697"
        );
        assert_eq!(
            shown(&unit.atan2(&Real::zero(), &unit.int(-2)), 37),
            "3.1415926535897932384626433832795028842"
        );
        assert_eq!(
            shown(&unit.sqrt(&unit.int(2)), 37),
            "1.4142135623730950488016887242096980786"
        );
        assert_eq!(
            shown(&unit.cbrt(&unit.int(-2)), 37),
            "-1.2599210498948731647672106072782283506"
        );
        assert_eq!(
            shown(&unit.hypot(&unit.int(3), &unit.int(4)), 37),
            "5.0000000000000000000000000000000000000"
        );
        assert_eq!(
            shown(&unit.decimal(Decimal::new(-12345, 4)), 37),
            "-1.2345000000000000000000000000000000000"
        );
    }

    #[test]
    fn rounding_out_is_exact() {
        let unit = Unit::new();
        // 0.5, 1.5 and 2.5 are exact in binary: ties.
        let half = unit.ratio(1, 2);
        assert_eq!(half.rounded(0, Rounding::HalfEven), Some(0));
        assert_eq!(half.rounded(0, Rounding::HalfUp), Some(1));
        assert_eq!(unit.ratio(3, 2).rounded(0, Rounding::HalfEven), Some(2));
        assert_eq!(unit.ratio(-5, 2).rounded(0, Rounding::HalfEven), Some(-2));
        assert_eq!(unit.ratio(-5, 2).rounded(0, Rounding::Up), Some(-3));
        assert_eq!(unit.int(7).rounded(6, Rounding::Up), Some(7_000_000));
    }

    #[test]
    fn long_division_divides_back() {
        // A fixed sequence of numerators of up to 8 limbs and divisors of
        // 1 to 4, some all ones or nearly zero, which exercise the guess's
        // corrections and the add-back step.
        let mut state: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..4000 {
            let pattern = next() % 4;
            let limb = |next: &mut dyn FnMut() -> u64| match pattern {
                0 => u64::MAX,
                1 => next() % 3,
                _ => next(),
            };
            let n_len = (next() % 8 + 1) as usize;
            let d_len = (next() % 4 + 1) as usize;
            let mut n = [0u64; WIDE];
            for slot in n.iter_mut().take(n_len) {
                *slot = limb(&mut next);
            }
            let mut d = [0u64; LIMBS];
            for slot in d.iter_mut().take(d_len) {
                *slot = limb(&mut next);
            }
            if d == [0; LIMBS] {
                continue;
            }
            let q = div_wide(&n, &d);
            // q × d ≤ n < (q + 1) × d, checked in 1024 bits by parts:
            // the product of q's low half and d, and of its high half.
            let low = mul_wide(&narrow(&q), &d);
            let high_q = narrow(&shr_wide(&q, 256));
            let high = mul_wide(&high_q, &d);
            let shifted = shl_wide(&high, 256);
            assert!(high[4..].iter().all(|&l| l == 0), "fits");
            let product = add_limbs(&low, &shifted);
            assert_ne!(order(&product, &n), Ordering::Greater, "q × d ≤ n");
            let rest = sub_limbs(&n, &product);
            assert_eq!(order(&rest, &wide(&d)), Ordering::Less, "n - q × d < d");
        }
    }
}
