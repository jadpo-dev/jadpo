//! The functions themselves, in fixed point: a working value `v` is the
//! [`Int`] `v × 10^w`, `w` the working places. Each public function gives
//! its value at `q` places within [`ERROR`] units of the last place,
//! having worked at `q` plus enough guard places that every truncation
//! along the way, and what a reduction multiplies it by, stays below a
//! tenth of a unit at `q`. The bounds are worked out beside each step.
//! Nothing here rounds to a scale: [`super`]'s loop does that, asking for
//! more places until the bounds settle the rounding.

use super::big::{Big, Int};
use crate::value::Decimal;

/// How far, in units of the last place, a value given at `q` places may
/// be from the true one. Each function's own error is below 1.1.
pub(crate) const ERROR: u32 = 2;

/// How many digits `n` has.
fn digits(n: u32) -> u32 {
    n.checked_ilog10().map_or(1, |log| log + 1)
}

/// Guard places for working at `q` places: enough that an error of a few
/// thousand units per working place stays below a tenth of a unit at `q`,
/// plus `extra` for what a reduction multiplies it by.
fn guard(q: u32, extra: u32) -> u32 {
    digits(q) + 8 + extra
}

/// `a × b` at `one`'s places, toward zero: below a unit off.
fn mulf(a: &Int, b: &Int, one: &Big) -> Int {
    a.mul(b).div_big(one)
}

/// `a ÷ b` at `one`'s places, toward zero: below a unit off.
fn divf(a: &Int, b: &Int, one: &Big) -> Int {
    a.mul_big(one).div(b)
}

/// A working value at `guard` more places than wanted, cut to the wanted
/// places: a unit more error at most.
fn finish(value: &Int, guard: u32) -> Int {
    value.div_big(&Big::pow10(guard))
}

/// `value × 10^w`, toward zero: exact when `w` keeps its places, below a
/// unit off otherwise.
pub(crate) fn fixed(value: Decimal, w: u32) -> Int {
    let mantissa = Int::from_i128(value.mantissa);
    if w >= value.scale {
        mantissa.mul_big(&Big::pow10(w - value.scale))
    } else {
        mantissa.div_big(&Big::pow10(value.scale - w))
    }
}

/// `atan(1/n) × 10^w`, `n` at least 2. Each term `10^w / n^(2k+1)` is
/// found by whole division of the last, which is exactly its whole part;
/// dividing by `2k + 1` costs below a unit more, and the terms left out
/// sum to less than the first of them, below a unit. So it is within
/// `2 × terms + 1` units, and there are at most `w / log10(n²) + 1` terms.
fn atan_inverse(n: u32, w: u32) -> Int {
    let square = n * n;
    let mut term = Big::pow10(w).div_small(n).0;
    let mut sum = Int::from_big(term.clone());
    let mut k: u32 = 1;
    loop {
        term = term.div_small(square).0;
        if term.is_zero() {
            return sum;
        }
        let part = Int::from_big(term.div_small(2 * k + 1).0);
        sum = if k % 2 == 1 {
            sum.sub(&part)
        } else {
            sum.add(&part)
        };
        k += 1;
    }
}

/// `π × 10^w` by Machin's formula, `16 atan(1/5) - 4 atan(1/239)`: within
/// `16 × (2 × (w/1.39 + 1) + 1) + 4 × (2 × (w/4.75 + 1) + 1)` units, below
/// `25w + 70`.
fn pi_at(w: u32) -> Int {
    atan_inverse(5, w)
        .mul_small(16)
        .sub(&atan_inverse(239, w).mul_small(4))
}

/// `π × 10^q`, within [`ERROR`]: `25w + 70` units at `w = q + guard` is
/// below `10^(guard - 1)`.
pub(crate) fn pi(q: u32) -> Int {
    let guard = guard(q, 0);
    finish(&pi_at(q + guard), guard)
}

/// `atanh(z) × 10^w` for `0 ≤ z ≤ 1/3`, by `z + z³/3 + z⁵/5 + ...`: `z²`
/// is within 2 units, each term within 3 of the last times `z²`, and each
/// division costs a unit, so with at most `w / log10(9) + 1` terms it is
/// within `4 × terms + 3` units of the value at the `z` given; the slope
/// of `atanh` there is at most 9/8.
fn atanh_series(z: &Int, one: &Big) -> Int {
    let square = mulf(z, z, one);
    let mut power = z.clone();
    let mut sum = z.clone();
    let mut odd: u32 = 3;
    loop {
        power = mulf(&power, &square, one);
        if power.is_zero() {
            return sum;
        }
        sum = sum.add(&power.div_small(odd));
        odd += 2;
    }
}

/// `atan(t) × 10^w` for `0 ≤ t ≤ 0.2`, by `t - t³/3 + t⁵/5 - ...`: as
/// [`atanh_series`], within `4 × terms + 3` units, with at most
/// `w / log10(25) + 1` terms and a slope of at most 1.
fn atan_series(t: &Int, one: &Big) -> Int {
    let square = mulf(t, t, one);
    let mut power = t.clone();
    let mut sum = t.clone();
    let mut odd: u32 = 3;
    loop {
        power = mulf(&power, &square, one).neg();
        if power.is_zero() {
            return sum;
        }
        sum = sum.add(&power.div_small(odd));
        odd += 2;
    }
}

/// `e^x × 10^q`, within [`ERROR`], for `x` below 88 and above -51, so
/// the result has at most 39 whole digits.
///
/// `x` is halved `m` times, to below 2^-10, `e^r` summed by its series,
/// and squared `m` times. `r` is within 2 units, the series within
/// `3 × terms + 4`, with at most `w / 3 + 2` terms. Squaring at most
/// doubles the error relative to the value, plus a unit, so the result is
/// within `2^(m+1) × (3 × terms + 6) × max(e^x, 1)` units: `m` is at most
/// 17, so that is below `10^6 × (w + 12) × 10^whole`, `whole` the digits
/// of `e^x`'s whole part, and `guard` covers it.
pub(crate) fn exp(x: Decimal, q: u32) -> Int {
    // 0.4343 × x + 1 digits, at least 1: 1 + x / 2 bounds it from above.
    let whole = u32::try_from(x.mantissa.max(0) / 10i128.pow(x.scale)).unwrap_or(88) / 2 + 2;
    let magnitude = x.mantissa.unsigned_abs() / 10u128.pow(x.scale) + 1;
    let halvings = 10 + (128 - magnitude.leading_zeros());
    let guard = guard(q, whole + 8);
    let w = q + guard;
    let one = Big::pow10(w);
    let r = fixed(x, w).div_big(&Big::from_u128(1).shl(u64::from(halvings)));
    let mut term = Int::from_big(one.clone());
    let mut sum = term.clone();
    let mut k: u32 = 1;
    loop {
        term = mulf(&term, &r, &one).div_small(k);
        if term.is_zero() {
            break;
        }
        sum = sum.add(&term);
        k += 1;
    }
    for _ in 0..halvings {
        sum = mulf(&sum, &sum, &one);
    }
    finish(&sum, guard)
}

/// `ln(x) × 10^q`, within [`ERROR`], for `x` above 0 with at most 38
/// digits, so `x` is `y × 2^k` with `1 ≤ y < 2` and `|k|` at most 127.
///
/// `ln(y)` is `2 atanh((y - 1) / (y + 1))`, the quotient below 1/3, and
/// `ln(2)` is `2 atanh(1/3)`; each is within `2 × (4 × terms + 6)` units
/// with at most `1.05w + 1` terms, and `ln(2)` counts `|k|` times, so the
/// sum is within `128 × 2 × (4.2w + 10)`, below `10^4 × (w + 3)`.
pub(crate) fn ln(x: Decimal, q: u32) -> Int {
    let guard = guard(q, 4);
    let w = q + guard;
    let one = Big::pow10(w);
    let numerator = Big::from_u128(x.mantissa.unsigned_abs());
    let denominator = Big::pow10(x.scale);
    // 2^k ≤ x < 2^(k+1): `x / 2^k` as numerator over denominator.
    let mut k = numerator.bits() as i64 - denominator.bits() as i64;
    let scaled = |k: i64| {
        if k >= 0 {
            (numerator.clone(), denominator.shl(k.unsigned_abs()))
        } else {
            (numerator.shl(k.unsigned_abs()), denominator.clone())
        }
    };
    loop {
        let (top, bottom) = scaled(k);
        if top < bottom {
            k -= 1;
        } else if top >= bottom.shl(1) {
            k += 1;
        } else {
            break;
        }
    }
    let (top, bottom) = scaled(k);
    // `y × 10^w`, below a unit off, and `z = (y - 1) / (y + 1)`.
    let y = Int::from_big(top.mul(&one).div_rem(&bottom).0);
    let unit = Int::from_big(one.clone());
    let z = divf(&y.sub(&unit), &y.add(&unit), &one);
    let ln_y = atanh_series(&z, &one).mul_small(2);
    let third = Int::from_big(one.div_small(3).0);
    let ln_2 = atanh_series(&third, &one).mul_small(2);
    let times = ln_2.mul_small(k.unsigned_abs() as u32);
    let sum = if k >= 0 {
        ln_y.add(&times)
    } else {
        ln_y.sub(&times)
    };
    finish(&sum, guard)
}

/// `sin(x) × 10^q` and `cos(x) × 10^q`, each within [`ERROR`].
///
/// `x` is brought to `r = x - k × π/2` with `|r|` at most about π/4, by
/// working out `k × π/2` at `whole + 1` more places than `w`, `whole` the
/// digits of `x`'s whole part, so `|k|` times π's error is still below 2
/// units at `w`: `r` is within 3 units. Each series is within
/// `4 × terms + 4` units, with at most `w / 2 + 2` terms, and a slope of
/// at most 1.
pub(crate) fn sin_cos(x: Decimal, q: u32) -> (Int, Int) {
    // |x| < 10^whole.
    let whole = (x.mantissa.unsigned_abs() / 10u128.pow(x.scale))
        .checked_ilog10()
        .map_or(1, |log| log + 1);
    let guard = guard(q, 4);
    let w = q + guard;
    let wide = w + whole + 1;
    let half_pi = pi(wide).div_small(2);
    let x_wide = fixed(x, wide);
    // The nearest whole number of half-turns, rounded half away from zero:
    // any near one keeps `r` small, so this needn't be exact.
    let k = x_wide.mul_small(2).add(&if x_wide.is_negative() {
        half_pi.neg()
    } else {
        half_pi.clone()
    });
    let k = k.div(&half_pi.mul_small(2));
    let r = finish(&x_wide.sub(&k.mul(&half_pi)), whole + 1);
    let one = Big::pow10(w);
    let square = mulf(&r, &r, &one);
    let mut term = r.clone();
    let mut sin = r.clone();
    let mut n: u32 = 2;
    loop {
        term = mulf(&term, &square, &one).div_small(n * (n + 1)).neg();
        if term.is_zero() {
            break;
        }
        sin = sin.add(&term);
        n += 2;
    }
    let mut term = Int::from_big(one.clone());
    let mut cos = term.clone();
    let mut n: u32 = 1;
    loop {
        term = mulf(&term, &square, &one).div_small(n * (n + 1)).neg();
        if term.is_zero() {
            break;
        }
        cos = cos.add(&term);
        n += 2;
    }
    // Which quarter turn: k mod 4, from its magnitude and sign.
    let quarter = k.magnitude().div_small(4).1;
    let quarter = if k.is_negative() {
        (4 - quarter) % 4
    } else {
        quarter
    };
    let (sin, cos) = match quarter {
        0 => (sin, cos),
        1 => (cos, sin.neg()),
        2 => (sin.neg(), cos.neg()),
        _ => (cos.neg(), sin),
    };
    (finish(&sin, guard), finish(&cos, guard))
}

/// `atan2(y, x) × 10^q`, within [`ERROR`], for `y` and `x` not both 0 and
/// neither 0: [`super`] gives those exactly or as a multiple of π.
///
/// The smaller of `|y|` and `|x|` over the larger is `t`, at most 1, within
/// a unit. Halving the angle twice, `t / (1 + √(1 + t²))`, brings it below
/// 0.2, each time within 3 more units; the series is within `4 × terms + 3`
/// with at most `w / 1.4 + 1` terms, and is counted four times. Then π/2
/// or π, within 1.1, may be added: below `20w + 60` units in all.
pub(crate) fn atan2(y: Decimal, x: Decimal, q: u32) -> Int {
    let guard = guard(q, 2);
    let w = q + guard;
    let one = Big::pow10(w);
    let unit = Int::from_big(one.clone());
    // Both at the larger scale, so their magnitudes compare as integers.
    let scale = y.scale.max(x.scale);
    let lifted =
        |d: Decimal| Big::from_u128(d.mantissa.unsigned_abs()).mul(&Big::pow10(scale - d.scale));
    let (a, b) = (lifted(y), lifted(x));
    let steep = a > b;
    let (small, large) = if steep { (&b, &a) } else { (&a, &b) };
    let mut t = Int::from_big(small.mul(&one).div_rem(large).0);
    for _ in 0..2 {
        let hypotenuse =
            Int::from_big(one.mul(&one).add(&t.magnitude().mul(t.magnitude())).isqrt());
        t = divf(&t, &unit.add(&hypotenuse), &one);
    }
    let mut angle = atan_series(&t, &one).mul_small(4);
    let pi = pi(w);
    if steep {
        angle = pi.div_small(2).sub(&angle);
    }
    if x.mantissa < 0 {
        angle = pi.sub(&angle);
    }
    if y.mantissa < 0 {
        angle = angle.neg();
    }
    finish(&angle, guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(value: &Int) -> String {
        let digits = value.magnitude().to_u128().map(|n| n.to_string());
        format!(
            "{}{}",
            if value.is_negative() { "-" } else { "" },
            digits.unwrap_or_default()
        )
    }

    fn within(value: &Int, expected: i128) {
        let difference = value.sub(&Int::from_i128(expected));
        let off = difference.magnitude().to_u128().unwrap_or(u128::MAX);
        assert!(
            off <= u128::from(ERROR),
            "{} is not {expected}",
            shown(value)
        );
    }

    #[test]
    fn each_function_lands_within_its_error() {
        // References: the digits of each value as Python's `decimal`
        // module gives them at 50 places (tests/data/decimal_maths.tsv
        // has the full set and their source).
        within(&pi(30), 3_141_592_653_589_793_238_462_643_383_279);
        within(
            &exp(Decimal::new(1, 0), 30),
            2_718_281_828_459_045_235_360_287_471_352,
        );
        within(
            &ln(Decimal::new(2, 0), 30),
            693_147_180_559_945_309_417_232_121_458,
        );
        let (sin, cos) = sin_cos(Decimal::new(1, 0), 30);
        within(&sin, 841_470_984_807_896_506_652_502_321_630);
        within(&cos, 540_302_305_868_139_717_400_936_607_442);
        within(
            &atan2(Decimal::new(1, 0), Decimal::new(1, 0), 30),
            785_398_163_397_448_309_615_660_845_819,
        );
    }
}
