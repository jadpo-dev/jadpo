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

/// Every enum declared here, by name, with its variants in order.
pub const ENUMS: [(&str, &[&str]); 3] = [
    (Rounding::NAME, Rounding::NAMES),
    (TextFormat::NAME, TextFormat::NAMES),
    (UuidVersion::NAME, UuidVersion::NAMES),
];

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
