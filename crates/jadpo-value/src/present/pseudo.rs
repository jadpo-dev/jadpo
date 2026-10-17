//! The pseudo-locale's text (area 10, "Tooling"): every message accented
//! and lengthened, for development and tests, so text that is hard-coded
//! stands out as plain, and a layout that is too tight for a longer
//! language breaks while it is being built rather than in German.
//!
//! The letters of the wording are written as look-alikes with accents, so
//! it stays readable; the digits, punctuation and spaces are left alone,
//! and so are the values a wording writes, which are not its words. The
//! whole is then lengthened by about two fifths and wrapped in brackets,
//! so a cut-off end shows as a missing `]`: `1 parcel` is `[1 þáŕçéļ ~~~~]`.

/// A letter's accented look-alike.
fn accented(c: char) -> char {
    match c {
        'a' => 'á',
        'b' => 'ƀ',
        'c' => 'ç',
        'd' => 'ð',
        'e' => 'é',
        'f' => 'ƒ',
        'g' => 'ĝ',
        'h' => 'ĥ',
        'i' => 'í',
        'j' => 'ĵ',
        'k' => 'ķ',
        'l' => 'ļ',
        'm' => 'ɱ',
        'n' => 'ñ',
        'o' => 'ó',
        'p' => 'þ',
        'q' => 'ǫ',
        'r' => 'ŕ',
        's' => 'š',
        't' => 'ţ',
        'u' => 'ú',
        'v' => 'ṽ',
        'w' => 'ŵ',
        'x' => 'ẋ',
        'y' => 'ý',
        'z' => 'ž',
        'A' => 'Á',
        'B' => 'Ɓ',
        'C' => 'Ç',
        'D' => 'Ð',
        'E' => 'É',
        'F' => 'Ƒ',
        'G' => 'Ĝ',
        'H' => 'Ĥ',
        'I' => 'Í',
        'J' => 'Ĵ',
        'K' => 'Ķ',
        'L' => 'Ļ',
        'M' => 'Ṁ',
        'N' => 'Ñ',
        'O' => 'Ó',
        'P' => 'Þ',
        'Q' => 'Ǫ',
        'R' => 'Ŕ',
        'S' => 'Š',
        'T' => 'Ţ',
        'U' => 'Ú',
        'V' => 'Ṽ',
        'W' => 'Ŵ',
        'X' => 'Ẋ',
        'Y' => 'Ý',
        'Z' => 'Ž',
        other => other,
    }
}

/// `text` with its ASCII letters accented.
pub fn accent(text: &str) -> String {
    text.chars().map(accented).collect()
}

/// `shown` lengthened by about two fifths and wrapped in brackets.
pub fn frame(shown: &str) -> String {
    let added = (shown.chars().count() * 2).div_ceil(5).max(1);
    format!("[{shown} {}]", "~".repeat(added))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_are_accented_and_the_rest_is_left_alone() {
        assert_eq!(accent("1 parcel, {x}."), "1 þáŕçéļ, {ẋ}.");
        assert_eq!(accent("Straße №5"), "Šţŕáßé №5");
    }

    #[test]
    fn the_text_is_lengthened_and_bracketed() {
        assert_eq!(frame("1 þáŕçéļ"), "[1 þáŕçéļ ~~~~]");
        assert_eq!(frame(""), "[ ~]");
        // About two fifths longer.
        let long = "x".repeat(50);
        assert_eq!(frame(&long).chars().count(), 50 + 20 + 3);
    }
}
