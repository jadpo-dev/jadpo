//! `colour`'s operations (area 19, "Colour", question 13, chunk 5.5p),
//! each giving an sRGB `Colour`, or WCAG 2's contrast ratio.
//!
//! The conversions are CSS Color 4's (its sample code, `conversions.js`):
//! sRGB's transfer function to and from linear light, linear sRGB to CIE
//! XYZ (D65) by its rational matrix, XYZ to OKLab through LMS and cube
//! roots, and OKLab to OKLCH by its chroma and hue, a hue being powerless
//! (missing) at a chroma of 0.000004 or less. HSL is CSS Color 4's
//! `hslToRgb`, and `mix` is CSS Color 5's `color-mix()` in OKLCH:
//! premultiplied by alpha, the hue the shorter way round, a missing hue
//! taking the other colour's. A colour sRGB can't show is clipped into it
//! channel by channel, and each channel is rounded to the nearest of 0 to
//! 255, a half up, as CSS and culori serialise one.
//!
//! The arithmetic is Jadpo's own, as maths on `Decimal` is (question 18):
//! fixed point at 18 places in an `i128`, with the powers, roots and
//! angles of `maths`, correctly rounded there, so every target gives the
//! same channels. No value it works with is past ±170, which keeps every
//! product inside an `i128`.

use jadpo_std::Rounding;
use jadpo_std::standard::colour::COLOUR;

use crate::value::Decimal;
use crate::{Fault, Value};

/// The places every value is held to.
const PLACES: u32 = 18;
/// 1, at [`PLACES`].
const ONE: i128 = 1_000_000_000_000_000_000;

/// A number at [`PLACES`] places.
type Fx = i128;

/// `n / d`, rounded half away from zero.
fn round_div(n: i128, d: i128) -> i128 {
    let (q, r) = (n / d, n % d);
    if 2 * r.abs() >= d.abs() {
        q + if (n < 0) == (d < 0) { 1 } else { -1 }
    } else {
        q
    }
}

/// `a × b`, rounded half away from zero, split into whole and fractional
/// parts so no product passes an `i128`.
fn mul(a: Fx, b: Fx) -> Fx {
    let negative = (a < 0) != (b < 0);
    let (a, b) = (a.abs(), b.abs());
    let (a_whole, a_part) = (a / ONE, a % ONE);
    let (b_whole, b_part) = (b / ONE, b % ONE);
    let magnitude = a_whole * b_whole * ONE
        + a_whole * b_part
        + a_part * b_whole
        + (a_part * b_part + ONE / 2) / ONE;
    if negative { -magnitude } else { magnitude }
}

/// `a ÷ b`, rounded half away from zero, the whole part first so no
/// product passes an `i128`; every divisor is below 170.
fn div(a: Fx, b: Fx) -> Fx {
    let (whole, rest) = (a / b, a % b);
    whole * ONE + round_div(rest * ONE, b)
}

/// The rational `n / d` at [`PLACES`].
const fn ratio(n: i128, d: i128) -> Fx {
    // Rounded half up: every ratio used is positive or rounds the same.
    let shifted = n * ONE;
    let (q, r) = (shifted / d, shifted % d);
    if 2 * r.abs() >= d.abs() {
        if (shifted < 0) == (d < 0) {
            q + 1
        } else {
            q - 1
        }
    } else {
        q
    }
}

fn decimal(x: Fx) -> Decimal {
    Decimal::new(x, PLACES)
}

/// A `Decimal` at [`PLACES`], however many places it has.
fn fx(d: Decimal) -> Result<Fx, Fault> {
    d.rescale(PLACES, Rounding::HalfEven)
        .map(|d| d.mantissa)
        .ok_or(Fault::DecimalOverflow)
}

fn exp(x: Fx) -> Result<Fx, Fault> {
    fx(crate::maths::exp(
        decimal(x),
        PLACES.into(),
        Rounding::HalfEven,
    )?)
}

fn ln(x: Fx) -> Result<Fx, Fault> {
    fx(crate::maths::ln(
        decimal(x),
        PLACES.into(),
        Rounding::HalfEven,
    )?)
}

/// `x` to the power `y`, for `x` at 0 or more.
fn pow(x: Fx, y: Fx) -> Result<Fx, Fault> {
    if x <= 0 {
        return Ok(0);
    }
    exp(mul(y, ln(x)?))
}

/// The cube root, with `x`'s sign, as `Math.cbrt` gives it.
fn cbrt(x: Fx) -> Result<Fx, Fault> {
    let root = pow(x.abs(), ratio(1, 3))?;
    Ok(if x < 0 { -root } else { root })
}

fn sqrt(x: Fx) -> Result<Fx, Fault> {
    fx(crate::maths::sqrt(
        decimal(x.max(0)),
        PLACES.into(),
        Rounding::HalfEven,
    )?)
}

fn pi() -> Result<Fx, Fault> {
    fx(crate::maths::pi(PLACES.into(), Rounding::HalfEven)?)
}

/// Degrees to radians.
fn radians(degrees: Fx) -> Result<Fx, Fault> {
    Ok(mul(round_div(degrees, 180), pi()?))
}

/// A 3 by 3 matrix times a vector.
fn times(m: &[[Fx; 3]; 3], v: [Fx; 3]) -> [Fx; 3] {
    m.map(|row| row.iter().zip(v).map(|(a, b)| mul(*a, b)).sum())
}

/// Linear-light sRGB to CIE XYZ, D65 (CSS Color 4's `lin_sRGB_to_XYZ`).
const LIN_SRGB_TO_XYZ: [[Fx; 3]; 3] = [
    [
        ratio(506_752, 1_228_815),
        ratio(87_881, 245_763),
        ratio(12_673, 70_218),
    ],
    [
        ratio(87_098, 409_605),
        ratio(175_762, 245_763),
        ratio(12_673, 175_545),
    ],
    [
        ratio(7_918, 409_605),
        ratio(87_881, 737_289),
        ratio(1_001_167, 1_053_270),
    ],
];

/// CIE XYZ, D65, to linear-light sRGB (`XYZ_to_lin_sRGB`).
const XYZ_TO_LIN_SRGB: [[Fx; 3]; 3] = [
    [ratio(12_831, 3_959), ratio(-329, 214), ratio(-1_974, 3_959)],
    [
        ratio(-851_781, 878_810),
        ratio(1_648_619, 878_810),
        ratio(36_519, 878_810),
    ],
    [ratio(705, 12_673), ratio(-2_585, 12_673), ratio(705, 667)],
];

/// A constant as CSS Color 4 writes it, to 16 places.
const fn places16(n: i128) -> Fx {
    n * 100
}

/// XYZ to LMS (`XYZ_to_OKLab`'s first matrix).
const XYZ_TO_LMS: [[Fx; 3]; 3] = [
    [
        places16(8_190_224_379_967_030),
        places16(3_619_062_600_528_904),
        places16(-1_288_737_815_209_879),
    ],
    [
        places16(329_836_539_323_885),
        places16(9_292_868_615_863_434),
        places16(361_446_663_506_424),
    ],
    [
        places16(481_771_893_596_242),
        places16(2_642_395_317_527_308),
        places16(6_335_478_284_694_309),
    ],
];

/// The cube roots of LMS to OKLab (`XYZ_to_OKLab`'s second matrix).
const LMS_TO_OKLAB: [[Fx; 3]; 3] = [
    [
        places16(2_104_542_683_093_140),
        places16(7_936_177_747_023_054),
        places16(-40_720_430_116_193),
    ],
    [
        places16(19_779_985_324_311_684),
        places16(-24_285_922_420_485_799),
        places16(4_505_937_096_174_110),
    ],
    [
        places16(259_040_424_655_478),
        places16(7_827_717_124_575_296),
        places16(-8_086_757_549_230_774),
    ],
];

/// OKLab to the cube roots of LMS (`OKLab_to_XYZ`'s first matrix).
const OKLAB_TO_LMS: [[Fx; 3]; 3] = [
    [
        ONE,
        places16(3_963_377_773_761_749),
        places16(2_158_037_573_099_136),
    ],
    [
        ONE,
        places16(-1_055_613_458_156_586),
        places16(-638_541_728_258_133),
    ],
    [
        ONE,
        places16(-894_841_775_298_119),
        places16(-12_914_855_480_194_092),
    ],
];

/// LMS to XYZ (`OKLab_to_XYZ`'s second matrix).
const LMS_TO_XYZ: [[Fx; 3]; 3] = [
    [
        places16(12_268_798_758_459_243),
        places16(-5_578_149_944_602_171),
        places16(2_813_910_456_659_647),
    ],
    [
        places16(-405_757_452_148_008),
        places16(11_122_868_032_803_170),
        places16(-717_110_580_655_164),
    ],
    [
        places16(-763_729_366_746_601),
        places16(-4_214_933_324_022_432),
        places16(15_869_240_198_367_816),
    ],
];

/// A chroma at or below which a hue is powerless (`OKLab_to_OKLCH`'s
/// epsilon).
const POWERLESS: Fx = 4_000_000_000_000;

/// sRGB's transfer function to linear light (`lin_sRGB`), for a channel
/// from 0 to 1.
fn linear(c: Fx) -> Result<Fx, Fault> {
    if c <= ratio(4_045, 100_000) {
        return Ok(div(c, ratio(1_292, 100)));
    }
    pow(div(c + ratio(55, 1000), ratio(1055, 1000)), ratio(24, 10))
}

/// Linear light to sRGB's transfer function (`gam_sRGB`), with its
/// extension below 0 and above 1, before the channel is clipped.
fn gamma(c: Fx) -> Result<Fx, Fault> {
    let magnitude = c.abs();
    if magnitude <= ratio(31_308, 10_000_000) {
        return Ok(mul(c, ratio(1_292, 100)));
    }
    let curved = mul(ratio(1055, 1000), pow(magnitude, ratio(10, 24))?) - ratio(55, 1000);
    Ok(if c < 0 { -curved } else { curved })
}

/// A channel from 0 to 1 as one of 0 to 255: clipped, then rounded to the
/// nearest, a half up.
fn channel(c: Fx) -> i64 {
    let clipped = c.clamp(0, ONE);
    i64::try_from((clipped * 255 + ONE / 2) / ONE).unwrap_or(255)
}

/// An OKLCH colour: its lightness from 0 to 1, its chroma, and its hue in
/// degrees from 0 below 360, `None` where it is powerless.
#[derive(Clone, Copy, Debug)]
struct Oklch {
    lightness: Fx,
    chroma: Fx,
    hue: Option<Fx>,
}

fn to_oklch(rgb: [i64; 3]) -> Result<Oklch, Fault> {
    let mut linear_rgb = [0; 3];
    for (out, c) in linear_rgb.iter_mut().zip(rgb) {
        *out = linear(ratio(i128::from(c), 255))?;
    }
    let lms = times(&XYZ_TO_LMS, times(&LIN_SRGB_TO_XYZ, linear_rgb));
    let [l, m, s] = lms;
    let lab = times(&LMS_TO_OKLAB, [cbrt(l)?, cbrt(m)?, cbrt(s)?]);
    let [lightness, a, b] = lab;
    let chroma = sqrt(mul(a, a) + mul(b, b))?;
    let hue = if chroma <= POWERLESS {
        None
    } else {
        let turned = fx(crate::maths::atan2(
            decimal(b),
            decimal(a),
            PLACES.into(),
            Rounding::HalfEven,
        )?)?;
        let degrees = div(turned * 180, pi()?);
        Some(if degrees < 0 {
            degrees + 360 * ONE
        } else {
            degrees
        })
    };
    Ok(Oklch {
        lightness,
        chroma,
        hue,
    })
}

/// An OKLCH colour as sRGB channels from 0 to 255, clipped.
fn from_oklch_parts(colour: Oklch) -> Result<[i64; 3], Fault> {
    let hue = radians(colour.hue.unwrap_or(0))?;
    let (sine, cosine) = (
        fx(crate::maths::sin(
            decimal(hue),
            PLACES.into(),
            Rounding::HalfEven,
        )?)?,
        fx(crate::maths::cos(
            decimal(hue),
            PLACES.into(),
            Rounding::HalfEven,
        )?)?,
    );
    let lab = [
        colour.lightness,
        mul(colour.chroma, cosine),
        mul(colour.chroma, sine),
    ];
    let cubed = times(&OKLAB_TO_LMS, lab).map(|c| mul(mul(c, c), c));
    let linear_rgb = times(&XYZ_TO_LIN_SRGB, times(&LMS_TO_XYZ, cubed));
    let mut rgb = [0; 3];
    for (out, c) in rgb.iter_mut().zip(linear_rgb) {
        *out = channel(gamma(c)?);
    }
    Ok(rgb)
}

/// A `Colour` record's channels: red, green, blue, alpha.
fn channels(value: &Value) -> Result<[i64; 4], Fault> {
    let Value::Record(record) = value else {
        return Err(Fault::Shape);
    };
    let mut out = [0; 4];
    for (slot, name) in out.iter_mut().zip(["red", "green", "blue", "alpha"]) {
        match record.get(name) {
            Some(Value::Int(n)) if (0..=255).contains(n) => *slot = *n,
            _ => return Err(Fault::Shape),
        }
    }
    Ok(out)
}

/// A `Colour` of these channels.
pub fn colour_value(rgba: [i64; 4]) -> Value {
    let [red, green, blue, alpha] = rgba;
    Value::record(
        COLOUR,
        vec![
            ("red".into(), Value::Int(red)),
            ("green".into(), Value::Int(green)),
            ("blue".into(), Value::Int(blue)),
            ("alpha".into(), Value::Int(alpha)),
        ],
    )
}

/// A `Percentage` as its fraction, clamped to 0 to 1.
fn fraction(value: &Value) -> Result<Fx, Fault> {
    match value {
        Value::Percentage(fraction) => Ok(fx(*fraction)
            .unwrap_or(if fraction.mantissa < 0 { 0 } else { ONE })
            .clamp(0, ONE)),
        _ => Err(Fault::Shape),
    }
}

/// An `Angle` in degrees from 0 below 360, rounded to 6 places, so an
/// angle written in whole degrees (`120deg`, held in radians to 9 places)
/// is those degrees exactly.
fn degrees(value: &Value) -> Result<Fx, Fault> {
    let Value::Quantity(angle) = value else {
        return Err(Fault::Shape);
    };
    if angle.kind != jadpo_std::QuantityKind::Angle {
        return Err(Fault::Shape);
    }
    let radians = fx(Decimal::new(angle.amount, angle.kind.scale()))?;
    // Whole turns off first, so the product stays small.
    let turn = 2 * pi()?;
    let within = radians.rem_euclid(turn);
    let exact = div(within * 180, pi()?);
    let micro = ONE / 1_000_000;
    Ok((round_div(exact, micro) * micro).rem_euclid(360 * ONE))
}

/// `colour.from_hsl(hue, saturation, lightness)`: CSS Color 4's
/// `hslToRgb`, the percentages clamped to 0% to 100%, opaque.
pub fn from_hsl(hue: &Value, saturation: &Value, lightness: &Value) -> Result<Value, Fault> {
    let (hue, saturation, lightness) = (degrees(hue)?, fraction(saturation)?, fraction(lightness)?);
    let a = mul(saturation, lightness.min(ONE - lightness));
    let f = |n: i128| {
        let k = (n * ONE + round_div(hue, 30)).rem_euclid(12 * ONE);
        let step = (k - 3 * ONE).min(9 * ONE - k).clamp(-ONE, ONE);
        channel(lightness - mul(a, step))
    };
    Ok(colour_value([f(0), f(8), f(4), 255]))
}

/// `colour.from_oklch(lightness, chroma, hue)`: the lightness clamped to
/// 0% to 100% and the chroma to 0 to 1, past every display's gamut;
/// clipped into sRGB, opaque.
pub fn from_oklch(lightness: &Value, chroma: &Value, hue: &Value) -> Result<Value, Fault> {
    let Value::Decimal(chroma) = chroma else {
        return Err(Fault::Shape);
    };
    let chroma = fx(*chroma)
        .unwrap_or(if chroma.mantissa < 0 { 0 } else { ONE })
        .clamp(0, ONE);
    let [red, green, blue] = from_oklch_parts(Oklch {
        lightness: fraction(lightness)?,
        chroma,
        hue: Some(degrees(hue)?),
    })?;
    Ok(colour_value([red, green, blue, 255]))
}

/// `colour.with_lightness(value, lightness)`: `value` with its OKLCH
/// lightness set, its chroma, hue and alpha kept, clipped into sRGB.
pub fn with_lightness(value: &Value, lightness: &Value) -> Result<Value, Fault> {
    let [red, green, blue, alpha] = channels(value)?;
    let mut colour = to_oklch([red, green, blue])?;
    colour.lightness = fraction(lightness)?;
    let [red, green, blue] = from_oklch_parts(colour)?;
    Ok(colour_value([red, green, blue, alpha]))
}

/// `colour.mix(a, b, ratio)`: CSS Color 5's `color-mix(in oklch, a, b
/// ratio)`, `ratio` of `b` and the rest of `a`, clamped to 0% to 100%.
pub fn mix(a: &Value, b: &Value, ratio_of_b: &Value) -> Result<Value, Fault> {
    let (first, second, p) = (channels(a)?, channels(b)?, fraction(ratio_of_b)?);
    let q = ONE - p;
    let (one, two) = (
        to_oklch([first[0], first[1], first[2]])?,
        to_oklch([second[0], second[1], second[2]])?,
    );
    let (alpha_one, alpha_two) = (
        ratio(i128::from(first[3]), 255),
        ratio(i128::from(second[3]), 255),
    );
    // A missing hue takes the other's; with neither, the mix has none.
    let (mut hue_one, mut hue_two) = match (one.hue, two.hue) {
        (Some(x), Some(y)) => (x, y),
        (Some(x), None) => (x, x),
        (None, Some(y)) => (y, y),
        (None, None) => (0, 0),
    };
    // The shorter way round.
    if hue_two - hue_one > 180 * ONE {
        hue_one += 360 * ONE;
    } else if hue_two - hue_one < -180 * ONE {
        hue_two += 360 * ONE;
    }
    let alpha = mul(alpha_one, q) + mul(alpha_two, p);
    let premultiplied = |x: Fx, y: Fx| mul(mul(x, alpha_one), q) + mul(mul(y, alpha_two), p);
    let (mut lightness, mut chroma) = (
        premultiplied(one.lightness, two.lightness),
        premultiplied(one.chroma, two.chroma),
    );
    if alpha > 0 {
        lightness = div(lightness, alpha);
        chroma = div(chroma, alpha);
    }
    let hue = (mul(hue_one, q) + mul(hue_two, p)).rem_euclid(360 * ONE);
    let [red, green, blue] = from_oklch_parts(Oklch {
        lightness,
        chroma,
        hue: (one.hue.is_some() || two.hue.is_some()).then_some(hue),
    })?;
    Ok(colour_value([red, green, blue, channel(alpha)]))
}

/// WCAG 2's relative luminance of an sRGB colour: its channels in linear
/// light, by WCAG's own threshold of 0.03928 (no 8-bit channel falls
/// between it and sRGB's 0.04045), weighted 0.2126, 0.7152 and 0.0722.
fn luminance(rgba: [i64; 4]) -> Result<Fx, Fault> {
    let coefficients = [ratio(2126, 10_000), ratio(7152, 10_000), ratio(722, 10_000)];
    let mut sum = 0;
    for (c, coefficient) in rgba.iter().zip(coefficients) {
        let c = ratio(i128::from(*c), 255);
        let linear = if c <= ratio(3_928, 100_000) {
            div(c, ratio(1_292, 100))
        } else {
            pow(div(c + ratio(55, 1000), ratio(1055, 1000)), ratio(24, 10))?
        };
        sum += mul(coefficient, linear);
    }
    Ok(sum)
}

/// `colour.contrast(a, b)`: WCAG 2's contrast ratio, `(L1 + 0.05) / (L2 +
/// 0.05)` of the lighter and the darker, from 1.00 to 21.00, worked out to
/// 12 places and then rounded down to 2, since WCAG says a ratio isn't
/// rounded up to meet a threshold. Alpha isn't read.
pub fn contrast(a: &Value, b: &Value) -> Result<Value, Fault> {
    let (one, two) = (luminance(channels(a)?)?, luminance(channels(b)?)?);
    let (lighter, darker) = (one.max(two), one.min(two));
    let offset = ratio(5, 100);
    let ratio_fx = div(lighter + offset, darker + offset);
    let twelve = round_div(ratio_fx, 1_000_000);
    let hundredths = twelve / 10_000_000_000;
    Ok(Value::Decimal(Decimal::new(hundredths, 2)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Value {
        let [r, g, b, a] = jadpo_std::standard::colour::read_hex(text).expect("a colour");
        colour_value([r, g, b, a].map(i64::from))
    }

    fn written(value: &Value) -> String {
        let [r, g, b, a] = channels(value).expect("a colour");
        let byte = |n: i64| u8::try_from(n).expect("a channel");
        jadpo_std::standard::colour::hex(byte(r), byte(g), byte(b), byte(a))
    }

    #[test]
    fn every_grey_and_primary_survives_oklch_and_back() {
        for c in [0, 1, 10, 11, 64, 127, 128, 200, 254, 255] {
            for rgb in [[c, c, c], [c, 0, 0], [0, c, 0], [0, 0, c], [c, 255 - c, 37]] {
                let back = from_oklch_parts(to_oklch(rgb).expect("converts")).expect("converts");
                assert_eq!(back, rgb, "{rgb:?}");
            }
        }
    }

    #[test]
    fn red_is_css_color_4s_oklch() {
        // CSS Color 4's sample code gives #ff0000 as oklch(0.62795536
        // 0.25768330 29.23388028); culori agrees to 6 places.
        let red = to_oklch([255, 0, 0]).expect("converts");
        assert_eq!(round_div(red.lightness, 10_000_000_000), 62_795_536);
        assert_eq!(round_div(red.chroma, 10_000_000_000), 25_768_330);
        assert_eq!(
            red.hue.map(|hue| round_div(hue, 10_000_000_000)),
            Some(2_923_388_028)
        );
        assert_eq!(to_oklch([255, 255, 255]).expect("converts").hue, None);
        assert_eq!(written(&hex("#1e6fd9")), "#1e6fd9");
    }
}
