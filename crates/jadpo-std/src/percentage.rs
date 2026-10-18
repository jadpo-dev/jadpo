//! `Percentage`, `Ratio` and `Fraction` (area 19, "The standard operator
//! table", chunk 5.5j2): the `%` literal and the three types' canonical
//! texts, which the checker, the plan and `jadpo-value` all read from here.
//!
//! A percentage is held as the exact fraction it is, so `20%` is 0.20 and
//! `12.5%` is 0.125: the places written plus two. Its canonical text is the
//! fraction as a percentage with no trailing zeros, `20%`, `12.5%`.

use crate::Num;

/// The `%` literal's value, the fraction it is: `20%` is 20 at 2 places,
/// 0.20, and `-12.5%` is -0.125. `None` for text that isn't a number and
/// `%`, or past 38 digits.
pub fn parse_literal(written: &str) -> Option<Num> {
    let number = written.strip_suffix('%')?;
    if !plain_number(number) {
        return None;
    }
    let value = Num::parse(number)?;
    let fraction = Num {
        mantissa: value.mantissa,
        scale: value.scale.checked_add(2)?,
    };
    fraction.at_scale(fraction.scale)
}

/// Digits with an optional `-` and places, nothing else: `20`, `-2.5`.
fn plain_number(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let (whole, places) = match digits.split_once('.') {
        Some((whole, places)) => (whole, Some(places)),
        None => (digits, None),
    };
    let plain = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    plain(whole) && places.is_none_or(plain)
}

/// A fraction as a percentage, the number with no trailing zeros: the
/// mantissa and places of `fraction × 100`, so 0.125 is (125, 1).
pub fn as_percent(mantissa: i128, scale: u32) -> (i128, u32) {
    let (mut mantissa, mut places) = if scale >= 2 {
        (mantissa, scale - 2)
    } else {
        (mantissa.saturating_mul(10i128.pow(2 - scale)), 0)
    };
    while places > 0 && mantissa % 10 == 0 {
        mantissa /= 10;
        places -= 1;
    }
    (mantissa, places)
}

/// A percentage's canonical text: `20%`, `12.5%`, `-5%`.
pub fn canonical(mantissa: i128, scale: u32) -> String {
    let (mantissa, places) = as_percent(mantissa, scale);
    format!(
        "{}%",
        Num {
            mantissa,
            scale: places
        }
    )
}

/// A percentage read from its canonical text, the fraction: a number, an
/// optional space, and `%` (`20%`, `20 %`, as French and German write
/// one); `None` for anything else.
pub fn parse_text(raw: &str) -> Option<Num> {
    let number = raw.strip_suffix('%')?;
    let number = number.strip_suffix(' ').unwrap_or(number);
    parse_literal(&format!("{number}%"))
}

/// A fraction's canonical text, `3/4`: its numerator and denominator as
/// whole numbers, the denominator at least 1.
pub fn fraction_text(numerator: i64, denominator: i64) -> String {
    format!("{numerator}/{denominator}")
}

/// A fraction read from its canonical text: a whole number, `/`, and a
/// whole number of at least 1, with no spaces: `3/4`, `-1/2`.
pub fn parse_fraction(raw: &str) -> Option<(i64, i64)> {
    let (top, bottom) = raw.split_once('/')?;
    let whole = |text: &str| {
        let digits = text.strip_prefix('-').unwrap_or(text);
        (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
            .then(|| text.parse::<i64>().ok())
            .flatten()
    };
    let (numerator, denominator) = (whole(top)?, whole(bottom)?);
    (denominator >= 1).then_some((numerator, denominator))
}

/// A ratio read from its canonical text: two decimal numbers joined by
/// `:`, each with the places it was given, with no spaces: `16:9`,
/// `1.85:1`.
pub fn parse_ratio(raw: &str) -> Option<(Num, Num)> {
    let (left, right) = raw.split_once(':')?;
    let number = |text: &str| plain_number(text).then(|| Num::parse(text)).flatten();
    Some((number(left)?, number(right)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_percent_literal_is_its_fraction() {
        let fraction = |written: &str| parse_literal(written).map(|n| (n.mantissa, n.scale));
        assert_eq!(fraction("20%"), Some((20, 2)));
        assert_eq!(fraction("12.5%"), Some((125, 3)));
        assert_eq!(fraction("-5%"), Some((-5, 2)));
        assert_eq!(fraction("100%"), Some((100, 2)));
        assert_eq!(fraction("20"), None);
        assert_eq!(fraction("%"), None);
        assert_eq!(fraction("2_0%"), None);
        assert_eq!(fraction("1.%"), None);
    }

    #[test]
    fn canonical_text_reads_back() {
        assert_eq!(canonical(20, 2), "20%");
        assert_eq!(canonical(125, 3), "12.5%");
        assert_eq!(canonical(25, 2), "25%");
        assert_eq!(canonical(2, 1), "20%");
        assert_eq!(canonical(1, 0), "100%");
        assert_eq!(canonical(-5, 2), "-5%");
        assert_eq!(canonical(0, 0), "0%");
        let read = |raw: &str| parse_text(raw).map(|n| canonical(n.mantissa, n.scale));
        assert_eq!(read("20%").as_deref(), Some("20%"));
        assert_eq!(read("20 %").as_deref(), Some("20%"));
        assert_eq!(read("20"), None);
        assert_eq!(read("20  %"), None);
        assert_eq!(read("twenty%"), None);
    }

    #[test]
    fn fractions_and_ratios_read_as_written() {
        assert_eq!(parse_fraction("3/4"), Some((3, 4)));
        assert_eq!(parse_fraction("-1/2"), Some((-1, 2)));
        assert_eq!(parse_fraction("3/0"), None);
        assert_eq!(parse_fraction("3/-4"), None);
        assert_eq!(parse_fraction("3 / 4"), None);
        assert_eq!(parse_fraction("0.5/1"), None);
        assert_eq!(fraction_text(3, 4), "3/4");
        let ratio = parse_ratio("1.85:1").expect("a ratio");
        assert_eq!((ratio.0.mantissa, ratio.0.scale), (185, 2));
        assert_eq!(parse_ratio("16 : 9"), None);
        assert_eq!(parse_ratio("16/9"), None);
    }
}
