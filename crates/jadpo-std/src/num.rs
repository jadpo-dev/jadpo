//! An exact number as written (area 2, question 8): `mantissa × 10^-scale`,
//! for literals, the bounds rules name, and the checks between them.

use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Num {
    pub mantissa: i128,
    pub scale: u32,
}

impl Num {
    /// Reads `12`, `-3`, `0.50` or `1_000`; `None` past 38 digits.
    pub fn parse(text: &str) -> Option<Num> {
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let digits = digits.replace('_', "");
        let (whole, places) = digits.split_once('.').unwrap_or((&digits, ""));
        let joined = format!("{whole}{places}");
        let mantissa: i128 = joined.parse().ok()?;
        Some(Num {
            mantissa: if negative { -mantissa } else { mantissa },
            scale: u32::try_from(places.len()).ok()?,
        })
    }

    /// A whole number.
    pub fn whole(n: i128) -> Num {
        Num {
            mantissa: n,
            scale: 0,
        }
    }

    /// By value: `1.5` equals `1.50`.
    pub fn compare(self, other: Num) -> Ordering {
        let scale = self.scale.max(other.scale);
        let lift = |n: Num| {
            10i128
                .checked_pow(scale - n.scale)
                .and_then(|factor| n.mantissa.checked_mul(factor))
        };
        match (lift(self), lift(other)) {
            (Some(a), Some(b)) => a.cmp(&b),
            _ => self.mantissa.signum().cmp(&other.mantissa.signum()),
        }
    }

    /// Whether it is a whole number that fits an `Int`, a 64-bit integer.
    pub fn fits_int(self) -> bool {
        self.scale == 0 && i64::try_from(self.mantissa).is_ok()
    }

    /// Decimal places, ignoring trailing zeros: `1.50` has one.
    pub fn significant_places(self) -> u32 {
        let mut places = self.scale;
        let mut mantissa = self.mantissa;
        while places > 0 && mantissa % 10 == 0 {
            mantissa /= 10;
            places -= 1;
        }
        places
    }
}

impl fmt::Display for Num {
    /// As written, with its decimal places: `0.50`, `-3`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.scale == 0 {
            return write!(f, "{}", self.mantissa);
        }
        let digits = self.mantissa.unsigned_abs().to_string();
        let places = self.scale as usize;
        let padded = format!("{digits:0>width$}", width = places + 1);
        let (whole, frac) = padded.split_at(padded.len() - places);
        let sign = if self.mantissa < 0 { "-" } else { "" };
        write!(f, "{sign}{whole}.{frac}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(text: &str) -> Num {
        Num::parse(text).expect("a number")
    }

    #[test]
    fn numbers_compare_exactly_and_print_as_written() {
        assert_eq!(num("1.50").compare(num("1.5")), Ordering::Equal);
        assert_eq!(num("-3").compare(num("1.50")), Ordering::Less);
        assert_eq!(num("1.50").significant_places(), 1);
        assert_eq!(num("0.05").to_string(), "0.05");
        assert_eq!(num("-0.05").to_string(), "-0.05");
        assert_eq!(num("1_000.5").to_string(), "1000.5");
        assert!(num("9223372036854775807").fits_int());
        assert!(!num("9223372036854775808").fits_int());
        assert_eq!(Num::parse(&"9".repeat(40)), None);
    }
}
