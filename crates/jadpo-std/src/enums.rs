//! The standard library's enums that Rust code interprets, declared once:
//! the checker builds the Jadpo enums of the same names from these lists,
//! so a variant added here exists for programs and for the runtime alike.

macro_rules! std_enum {
    (
        $(#[$meta:meta])*
        $name:ident: $written:literal { $($(#[$vmeta:meta])* $variant:ident: $word:literal,)* }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $($(#[$vmeta])* $variant,)*
        }

        impl $name {
            /// The enum's name in Jadpo.
            pub const NAME: &'static str = $written;

            /// Every variant, in the order Jadpo lists them.
            pub const ALL: &'static [$name] = &[$($name::$variant,)*];

            /// The variants' names in Jadpo, in order.
            pub const NAMES: &'static [&'static str] = &[$($word,)*];

            /// The variant's name in Jadpo.
            pub fn name(self) -> &'static str {
                match self {
                    $($name::$variant => $word,)*
                }
            }

            pub fn from_name(name: &str) -> Option<$name> {
                $name::ALL.iter().copied().find(|variant| variant.name() == name)
            }
        }
    };
}

std_enum! {
    /// How a division that doesn't come out exact is rounded (area 3,
    /// question 7).
    Rounding: "Rounding" {
        /// To the nearest; a tie goes to the even neighbour.
        HalfEven: "half_even",
        /// To the nearest; a tie goes away from zero.
        HalfUp: "half_up",
        /// Away from zero.
        Up: "up",
    }
}

std_enum! {
    /// The formats a `Text` type's `format:` setting accepts (area 2,
    /// question 9).
    TextFormat: "TextFormat" {
        /// An email address.
        Email: "email",
    }
}

std_enum! {
    /// The UUID versions a `Uuid` type's `version:` setting can pin (area 2,
    /// question 13).
    UuidVersion: "UuidVersion" {
        V1: "v1",
        V2: "v2",
        V3: "v3",
        V4: "v4",
        V5: "v5",
        V6: "v6",
        V7: "v7",
        V8: "v8",
    }
}

impl UuidVersion {
    /// The version digit a UUID's text carries.
    pub fn digit(self) -> u8 {
        match self {
            UuidVersion::V1 => 1,
            UuidVersion::V2 => 2,
            UuidVersion::V3 => 3,
            UuidVersion::V4 => 4,
            UuidVersion::V5 => 5,
            UuidVersion::V6 => 6,
            UuidVersion::V7 => 7,
            UuidVersion::V8 => 8,
        }
    }
}

std_enum! {
    /// A day of the week (area 10).
    Weekday: "Weekday" {
        Monday: "monday",
        Tuesday: "tuesday",
        Wednesday: "wednesday",
        Thursday: "thursday",
        Friday: "friday",
        Saturday: "saturday",
        Sunday: "sunday",
    }
}

/// The name of the built-in enum of IANA time zones (area 10).
pub const ZONE: &str = "Zone";

/// The name of the built-in enum of standard locales (area 10).
pub const LOCALE: &str = "Locale";

/// Every enum declared here, by name, with its variants in order: the
/// ones Rust interprets, then `Zone` and `Locale` from their tables.
pub fn enums() -> Vec<(&'static str, Vec<&'static str>)> {
    let names = |list: &'static [(&'static str, &'static str)]| {
        list.iter().map(|(variant, _)| *variant).collect()
    };
    vec![
        (Rounding::NAME, Rounding::NAMES.to_vec()),
        (TextFormat::NAME, TextFormat::NAMES.to_vec()),
        (UuidVersion::NAME, UuidVersion::NAMES.to_vec()),
        (Weekday::NAME, Weekday::NAMES.to_vec()),
        (ZONE, names(crate::zones::ZONES)),
        (LOCALE, names(crate::zones::LOCALES)),
    ]
}

/// Where variant `variant` stands among enum `name`'s variants, from 0, in
/// the order they are declared: what a set orders them by (area 2, "Set
/// order").
pub fn variant_ordinal(name: &str, variant: &str) -> Option<u32> {
    let position = |names: &[&str]| names.iter().position(|v| *v == variant);
    let found = match name {
        Rounding::NAME => position(Rounding::NAMES),
        TextFormat::NAME => position(TextFormat::NAMES),
        UuidVersion::NAME => position(UuidVersion::NAMES),
        Weekday::NAME => position(Weekday::NAMES),
        ZONE => crate::zones::ZONES.iter().position(|(v, _)| *v == variant),
        LOCALE => crate::zones::LOCALES
            .iter()
            .position(|(v, _)| *v == variant),
        _ => None,
    };
    found.and_then(|at| u32::try_from(at).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_stand_in_declared_order() {
        assert_eq!(variant_ordinal(Weekday::NAME, "monday"), Some(0));
        assert_eq!(variant_ordinal(Weekday::NAME, "sunday"), Some(6));
        assert_eq!(variant_ordinal(Rounding::NAME, "up"), Some(2));
        assert!(variant_ordinal(ZONE, "utc").is_some());
        assert_eq!(variant_ordinal(Weekday::NAME, "someday"), None);
        assert_eq!(variant_ordinal("Shade", "plain"), None);
    }

    #[test]
    fn variants_read_back_by_name() {
        assert_eq!(Rounding::from_name("half_up"), Some(Rounding::HalfUp));
        assert_eq!(TextFormat::from_name("email"), Some(TextFormat::Email));
        assert_eq!(
            UuidVersion::from_name("v7").map(UuidVersion::digit),
            Some(7)
        );
        assert_eq!(UuidVersion::NAMES.len(), 8);
        assert_eq!(Rounding::from_name("down"), None);
        let all = enums();
        let zone = all.iter().find(|(name, _)| *name == ZONE).expect("Zone");
        assert!(zone.1.contains(&"europe_london") && zone.1.contains(&"utc"));
        let mut sorted = zone.1.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, zone.1, "zones sorted, each once");
        let locale = all
            .iter()
            .find(|(name, _)| *name == LOCALE)
            .expect("Locale");
        for tag in ["en_gb", "en_us", "fr_fr", "fr", "zh_hant_tw"] {
            assert!(locale.1.contains(&tag), "{tag}");
        }
        let mut sorted = locale.1.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, locale.1, "locales sorted, each once");
    }
}
