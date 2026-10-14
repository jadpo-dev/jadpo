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

    /// By value: `1.5` equals `1.50`. Exact whatever the scales, as
    /// [`compare_exact`] is.
    pub fn compare(self, other: Num) -> Ordering {
        compare_exact(self.mantissa, self.scale, other.mantissa, other.scale)
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

/// `a × 10^-a_scale` against `b × 10^-b_scale`, exactly: the one
/// comparison of exact numbers the checker's bounds and the runtime's
/// decimals share (area 2, question 8). Nothing is lifted to a common
/// scale, so no scale or mantissa overflows into a wrong answer: the
/// number with fewer places is compared with the other's whole part at
/// its scale, and what is left over breaks a tie. It is a total order,
/// consistent with equality by value.
pub fn compare_exact(a: i128, a_scale: u32, b: i128, b_scale: u32) -> Ordering {
    let sign = |n: i128| n.cmp(&0);
    match sign(a).cmp(&sign(b)) {
        Ordering::Equal => {}
        unequal => return unequal,
    }
    let magnitudes = if a_scale <= b_scale {
        magnitude(a.unsigned_abs(), b.unsigned_abs(), b_scale - a_scale)
    } else {
        magnitude(b.unsigned_abs(), a.unsigned_abs(), a_scale - b_scale).reverse()
    };
    // Both negative: the larger magnitude is the smaller number.
    if a < 0 {
        magnitudes.reverse()
    } else {
        magnitudes
    }
}

/// `x × 10^places` against `y`, for magnitudes.
fn magnitude(x: u128, y: u128, places: u32) -> Ordering {
    if x == 0 {
        return 0.cmp(&y);
    }
    // Past 10^38 the lifted `x` is larger than any `u128`.
    let Some(factor) = 10u128.checked_pow(places) else {
        return Ordering::Greater;
    };
    // `x × factor` against `y` is `x` against `y / factor`, with the
    // remainder deciding a tie.
    match x.cmp(&(y / factor)) {
        Ordering::Equal if !y.is_multiple_of(factor) => Ordering::Less,
        other => other,
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

    #[test]
    fn numbers_far_apart_in_scale_compare_exactly() {
        let big = num("100000000000000000000000000000.0");
        let tiny = num("0.0000000005");
        assert_eq!(tiny.compare(big), Ordering::Less);
        assert_eq!(big.compare(tiny), Ordering::Greater);
        assert_eq!(
            num("-0.0000000005").compare(num("-100000000000000000000000000000.0")),
            Ordering::Greater
        );
        // Lifting either to the other's scale overflows an `i128`.
        let huge = Num {
            mantissa: i128::MAX,
            scale: 0,
        };
        let fine = Num {
            mantissa: 1,
            scale: 60,
        };
        assert_eq!(huge.compare(fine), Ordering::Greater);
        assert_eq!(fine.compare(huge), Ordering::Less);
        assert_eq!(
            Num {
                mantissa: i128::MIN,
                scale: 0
            }
            .compare(fine),
            Ordering::Less
        );
        assert_eq!(
            Num {
                mantissa: 10,
                scale: 1
            }
            .compare(Num {
                mantissa: 1_000_000,
                scale: 6
            }),
            Ordering::Equal
        );
        assert_eq!(
            Num {
                mantissa: 1,
                scale: 0
            }
            .compare(Num {
                mantissa: 1_000_001,
                scale: 6
            }),
            Ordering::Less
        );
        assert_eq!(
            num("0").compare(Num {
                mantissa: 0,
                scale: 70
            }),
            Ordering::Equal
        );
    }

    #[test]
    fn the_order_is_total_and_transitive() {
        let values = [
            Num {
                mantissa: i128::MAX,
                scale: 0,
            },
            Num {
                mantissa: i128::MAX,
                scale: 38,
            },
            Num {
                mantissa: i128::MIN,
                scale: 3,
            },
            Num {
                mantissa: -1,
                scale: 70,
            },
            Num {
                mantissa: 1,
                scale: 70,
            },
            Num {
                mantissa: 0,
                scale: 5,
            },
            num("1.50"),
            num("1.5"),
            num("-2"),
            num("100000000000000000000000000000.0"),
            num("0.0000000005"),
        ];
        for a in values {
            assert_eq!(a.compare(a), Ordering::Equal);
            for b in values {
                assert_eq!(a.compare(b), b.compare(a).reverse(), "{a:?} {b:?}");
                for c in values {
                    if a.compare(b).is_le() && b.compare(c).is_le() {
                        assert!(a.compare(c).is_le(), "{a:?} <= {b:?} <= {c:?}");
                    }
                }
            }
        }
    }
}
