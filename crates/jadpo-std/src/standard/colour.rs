//! `Colour` (area 19, "Colour", question 13, chunk 5.5p): `red`, `green`,
//! `blue` and `alpha`, each 0 to 255, in sRGB (IEC 61966-2-1), written as
//! CSS Color 4 writes a hex colour, `#rrggbb` or `#rrggbbaa`. `colour`'s
//! operations, which go through linear light, CIE XYZ and OKLab, are
//! `jadpo-value`'s `colour` module; this one holds the type and its text.

use super::{
    Examples, FIRST_RELEASE, FieldMatch, Format, FormatKey, Holds, Json, NO_OPTIONS, Part,
    StandardType, Storage, TextForm,
};
use crate::Builtin;

/// `Colour`.
pub const COLOUR: &str = "Colour";

/// `Colour`'s channels, in order.
pub const COLOUR_FIELDS: [(&str, Part); 4] = [
    ("red", Part::Whole(0, 255)),
    ("green", Part::Whole(0, 255)),
    ("blue", Part::Whole(0, 255)),
    ("alpha", Part::Whole(0, 255)),
];

/// A colour's canonical text: `#` and two lower-case hexadecimal digits a
/// channel, the alpha's left out when it is 255, opaque.
pub fn hex(red: u8, green: u8, blue: u8, alpha: u8) -> String {
    if alpha == 255 {
        format!("#{red:02x}{green:02x}{blue:02x}")
    } else {
        format!("#{red:02x}{green:02x}{blue:02x}{alpha:02x}")
    }
}

/// A colour read from its text: `#` and six or eight hexadecimal digits,
/// in either case, as CSS reads them; the alpha is 255 when there are six.
/// CSS's three- and four-digit short forms aren't read, so a colour has one
/// length of text.
pub fn read_hex(raw: &str) -> Option<[u8; 4]> {
    let digits = raw.strip_prefix('#')?;
    if !(digits.len() == 6 || digits.len() == 8) || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(digits.get(at..at + 2)?, 16).ok();
    let alpha = if digits.len() == 8 { channel(6)? } else { 255 };
    Some([channel(0)?, channel(2)?, channel(4)?, alpha])
}

/// What `type.standard_exists` matches: a red, a green and a blue, and
/// optionally an alpha, by their full names or their initials, each built
/// on `Int` or `Decimal` (area 19, "Packages use the standard types").
const COLOUR_SHAPE: &[FieldMatch] = &[
    FieldMatch {
        meaning: "a red",
        names: &["red", "r"],
        built_on: &[Builtin::Int, Builtin::Decimal],
        or_enum: false,
        optional: false,
    },
    FieldMatch {
        meaning: "a green",
        names: &["green", "g"],
        built_on: &[Builtin::Int, Builtin::Decimal],
        or_enum: false,
        optional: false,
    },
    FieldMatch {
        meaning: "a blue",
        names: &["blue", "b"],
        built_on: &[Builtin::Int, Builtin::Decimal],
        or_enum: false,
        optional: false,
    },
    FieldMatch {
        meaning: "an alpha",
        names: &["alpha", "a"],
        built_on: &[Builtin::Int, Builtin::Decimal],
        or_enum: false,
        optional: true,
    },
];

/// A colour in sRGB with its alpha, sent as its text and stored a
/// `smallint` column a channel.
pub const COLOUR_ENTRY: StandardType = StandardType {
    name: COLOUR,
    holds: Holds::Object(&COLOUR_FIELDS),
    text: Some(TextForm::Colour),
    json: Json::Text,
    storage: Storage::Columns {
        parts: &[
            ("red", "smallint"),
            ("green", "smallint"),
            ("blue", "smallint"),
            ("alpha", "smallint"),
        ],
        fixed: None,
    },
    format: Format {
        key: FormatKey::Canonical,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: Some(COLOUR_SHAPE),
    examples: Examples {
        valid: &[
            ("#000000", "#000000"),
            ("#ffffff", "#ffffff"),
            ("#1e6fd9", "#1e6fd9"),
            ("#1e6fd980", "#1e6fd980"),
            ("#00000000", "#00000000"),
        ],
        format: None,
        malformed: &[
            "1e6fd9",
            "#1e6fd",
            "#1e6fd9f",
            "#fff",
            "#ffff",
            "#1e6fdg",
            "rgb(30 111 217)",
            "#ffffff ",
            "",
        ],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_reads_either_case_and_writes_lower_case() {
        assert_eq!(read_hex("#1E6FD9"), Some([30, 111, 217, 255]));
        assert_eq!(read_hex("#1e6fd9ff"), Some([30, 111, 217, 255]));
        assert_eq!(hex(30, 111, 217, 255), "#1e6fd9");
        assert_eq!(hex(30, 111, 217, 128), "#1e6fd980");
        assert_eq!(read_hex("#fff"), None);
        assert_eq!(read_hex("#+1e6fd9"), None);
    }
}
