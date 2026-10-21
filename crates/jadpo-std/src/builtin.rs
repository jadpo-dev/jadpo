//! The types built into the language (area 2): no Jadpo declaration can say
//! what they are made of, so the compiler and the runtime both know them
//! from here.

use crate::QuantityKind;

/// A built-in type.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Builtin {
    Bool,
    Int,
    Decimal,
    Text,
    Bytes,
    Uuid,
    Email,
    Url,
    IpAddress,
    Instant,
    CalendarDate,
    /// An `Instant` seen in a `Zone` (area 10, question 5): local time
    /// for showing or reasoning about, never a time of day on its own.
    ZonedTime,
    Duration,
    /// An amount of money (area 19, "Money and payments", question 16): a
    /// whole number of its currency's minor units (pence, cents) and its
    /// `Currency`, never a decimal of pounds.
    Money,
    /// A physical quantity, `DataSize` or `Bitrate` (area 19, "Physical
    /// quantities", question 17): an exact number in its kind's SI base
    /// unit at the kind's scale, its unit chosen only where it is made or
    /// shown.
    Quantity(crate::QuantityKind),
    /// A proportion where 0.20 is 20% (area 19, "The standard operator
    /// table"): an exact `Decimal` fraction, written `20%`, shown as the
    /// locale writes a percentage.
    Percentage,
    Unit,
    /// The opaque cursor a page of results gives for the next one (area
    /// 6, "Order and pages"): text to the program, read only by the query
    /// that made it.
    PageCursor,
    /// How many records a page holds: an `Int` from 1 to 100 (area 6).
    PageSize,
    /// An API token as `sessions.issue_token` gives it, once (area 8):
    /// text an `Output` can send, which no entity can keep.
    ApiToken,
    /// A service key as `Service.issue_key` gives it, once (area 8).
    ServiceKey,
    /// A magic link's six-digit code (area 8, "Magic links"), text so a
    /// leading `0` stays: what the application's own sign-in email
    /// writes out and its own sending sends, which no entity can keep.
    SignInCode,
    /// A magic link's token (area 8, "Writing the routes yourself"; chunk
    /// 7.3b3): the 64 lower-case hexadecimal digits the email's link
    /// carries, which the application's own route takes to
    /// `magic_links.redeem` and no entity, response, failure or event can
    /// hold.
    SignInToken,
    /// A PostgreSQL address, `postgres://` or `postgresql://` (area 10,
    /// "Database and other connection URLs"). Always secret: it normally
    /// carries a password.
    PostgresUrl,
    /// A Redis address, `redis://` or `rediss://` (area 10). Always
    /// secret.
    RedisUrl,
    /// The authority a database server's certificate is checked against,
    /// as PEM (area 5, "PostgreSQL over TLS"; chunk 8.1n): what a
    /// connection's `authority:` reads, for a server whose certificate a
    /// private authority signed (Amazon RDS's, Cloud SQL's). Public, so
    /// not secret; never stored.
    Certificate,
    /// A key sessions and tokens are signed with (area 10, "Signing key
    /// rotation"): at least [`SIGNING_KEY_MIN`] characters. Always secret.
    SigningKey,
    /// The key `crypto.encrypt` and `crypto.decrypt` seal with (area 19,
    /// "Hashing and cryptography"): 32 random bytes, written as 43
    /// characters of unpadded Base64url ([`is_key_text`]). Always secret,
    /// and rotates through `NAME_PREVIOUS` as a `SigningKey` does.
    EncryptionKey,
    /// The Ed25519 key `crypto.sign` signs with (area 19): its 32-byte
    /// seed, written as unpadded Base64url like an `EncryptionKey`. Always
    /// secret; only a setting holds one until keys held as data arrive.
    PrivateKey,
    /// Text for a person, in their language, as `text.format` and
    /// translations give it (area 10, "Showing dates and times to
    /// people"): it goes into a response, an email or a page, and is never
    /// stored, read back as a time, ordered or rebuilt into `Text`, so it
    /// is its own representation.
    PresentationText,
    /// A whole number that picks a message's plural form (area 10,
    /// "Translations as messages"): used directly as a `Message` field, and
    /// written as `{count}` in each form's wording.
    PluralCount,
}

/// The fewest characters a [`Builtin::SigningKey`] has: 256 bits of a
/// hexadecimal key, as `jadpo test` generates for a test run.
pub const SIGNING_KEY_MIN: usize = 32;

/// How many bytes an [`Builtin::EncryptionKey`] and a
/// [`Builtin::PrivateKey`] hold: 256 bits.
pub const KEY_SIZE: usize = 32;

/// `PageSize`'s own rules: from 1 to 100 (area 6, "Order and pages").
static PAGE_SIZE_RULES: crate::Rules = crate::Rules {
    min: Some(crate::Num {
        mantissa: 1,
        scale: 0,
    }),
    max: Some(crate::Num {
        mantissa: 100,
        scale: 0,
    }),
    min_length: None,
    max_length: None,
    pattern: None,
    format: None,
    scale: None,
    version: None,
    min_duration: None,
    max_duration: None,
    currency: None,
    rounding: None,
    rate_from: None,
    rate_to: None,
    dimensions: None,
    distance: None,
    standard: None,
    double: false,
};

impl Builtin {
    pub const ALL: [Builtin; 49] = [
        Builtin::Bool,
        Builtin::Int,
        Builtin::Decimal,
        Builtin::Text,
        Builtin::Bytes,
        Builtin::Uuid,
        Builtin::Email,
        Builtin::Url,
        Builtin::IpAddress,
        Builtin::Instant,
        Builtin::CalendarDate,
        Builtin::ZonedTime,
        Builtin::Duration,
        Builtin::Money,
        Builtin::Quantity(QuantityKind::Length),
        Builtin::Quantity(QuantityKind::Area),
        Builtin::Quantity(QuantityKind::Volume),
        Builtin::Quantity(QuantityKind::Angle),
        Builtin::Quantity(QuantityKind::Mass),
        Builtin::Quantity(QuantityKind::Pressure),
        Builtin::Quantity(QuantityKind::Energy),
        Builtin::Quantity(QuantityKind::Power),
        Builtin::Quantity(QuantityKind::Temperature),
        Builtin::Quantity(QuantityKind::TemperatureDifference),
        Builtin::Quantity(QuantityKind::Speed),
        Builtin::Quantity(QuantityKind::Frequency),
        Builtin::Quantity(QuantityKind::Voltage),
        Builtin::Quantity(QuantityKind::Current),
        Builtin::Quantity(QuantityKind::Illuminance),
        Builtin::Quantity(QuantityKind::SoundLevel),
        Builtin::Quantity(QuantityKind::Concentration),
        Builtin::Quantity(QuantityKind::DataSize),
        Builtin::Quantity(QuantityKind::Bitrate),
        Builtin::Percentage,
        Builtin::Unit,
        Builtin::PageCursor,
        Builtin::PageSize,
        Builtin::ApiToken,
        Builtin::ServiceKey,
        Builtin::SignInCode,
        Builtin::SignInToken,
        Builtin::PostgresUrl,
        Builtin::RedisUrl,
        Builtin::Certificate,
        Builtin::SigningKey,
        Builtin::EncryptionKey,
        Builtin::PrivateKey,
        Builtin::PresentationText,
        Builtin::PluralCount,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Builtin::Bool => "Bool",
            Builtin::Int => "Int",
            Builtin::Decimal => "Decimal",
            Builtin::Text => "Text",
            Builtin::Bytes => "Bytes",
            Builtin::Uuid => "Uuid",
            Builtin::Email => "Email",
            Builtin::Url => "Url",
            Builtin::IpAddress => "IpAddress",
            Builtin::Instant => "Instant",
            Builtin::CalendarDate => "CalendarDate",
            Builtin::ZonedTime => "ZonedTime",
            Builtin::Duration => "Duration",
            Builtin::Money => "Money",
            Builtin::Quantity(kind) => kind.name(),
            Builtin::Percentage => "Percentage",
            Builtin::Unit => "Unit",
            Builtin::PageCursor => "PageCursor",
            Builtin::PageSize => "PageSize",
            Builtin::ApiToken => "ApiToken",
            Builtin::ServiceKey => "ServiceKey",
            Builtin::SignInCode => "SignInCode",
            Builtin::SignInToken => "SignInToken",
            Builtin::PostgresUrl => "PostgresUrl",
            Builtin::RedisUrl => "RedisUrl",
            Builtin::Certificate => "Certificate",
            Builtin::SigningKey => "SigningKey",
            Builtin::EncryptionKey => "EncryptionKey",
            Builtin::PrivateKey => "PrivateKey",
            Builtin::PresentationText => "PresentationText",
            Builtin::PluralCount => "PluralCount",
        }
    }

    pub fn from_name(name: &str) -> Option<Builtin> {
        Builtin::ALL
            .into_iter()
            .find(|builtin| builtin.name() == name)
    }

    /// What its values are made of: the validated text types are text.
    pub fn representation(self) -> Builtin {
        match self {
            Builtin::Email
            | Builtin::Url
            | Builtin::IpAddress
            | Builtin::PageCursor
            | Builtin::ApiToken
            | Builtin::ServiceKey
            | Builtin::SignInCode
            | Builtin::SignInToken
            | Builtin::PostgresUrl
            | Builtin::RedisUrl
            | Builtin::Certificate
            | Builtin::SigningKey
            | Builtin::EncryptionKey
            | Builtin::PrivateKey => Builtin::Text,
            Builtin::PageSize | Builtin::PluralCount => Builtin::Int,
            other => other,
        }
    }

    /// Whether it is raw material rather than a meaning: a named type
    /// built on it never widens to it, and a signature may not use it
    /// directly (area 2, question 11).
    pub fn is_representation(self) -> bool {
        !matches!(
            self,
            Builtin::Email
                | Builtin::Url
                | Builtin::IpAddress
                | Builtin::Unit
                | Builtin::PageCursor
                | Builtin::PageSize
                | Builtin::ApiToken
                | Builtin::ServiceKey
                | Builtin::SignInCode
                | Builtin::SignInToken
                | Builtin::PostgresUrl
                | Builtin::RedisUrl
                | Builtin::Certificate
                | Builtin::SigningKey
                | Builtin::EncryptionKey
                | Builtin::PrivateKey
                | Builtin::PresentationText
                | Builtin::PluralCount
        )
    }

    /// Whether a configuration value of this type is secret whatever the
    /// field says (area 10): an address that normally carries a password,
    /// and a key.
    pub fn is_secret(self) -> bool {
        matches!(
            self,
            Builtin::PostgresUrl
                | Builtin::RedisUrl
                | Builtin::SigningKey
                | Builtin::EncryptionKey
                | Builtin::PrivateKey
        )
    }

    /// Whether it is a key a `crypto` operation takes as its `key:`, the
    /// one place code may give it (area 19, "Hashing and cryptography"):
    /// opaque, never read, printed, stored or compared.
    pub fn is_key(self) -> bool {
        matches!(
            self,
            Builtin::SigningKey | Builtin::EncryptionKey | Builtin::PrivateKey
        )
    }

    /// Whether a setting of this type also reads `NAME_PREVIOUS`, the key
    /// it replaced, still accepted while a deployment rotates (area 10,
    /// "Signing keys"; area 19 for an `EncryptionKey`).
    pub fn rotates(self) -> bool {
        matches!(self, Builtin::SigningKey | Builtin::EncryptionKey)
    }

    /// The rules the type itself carries, below any a declaration adds:
    /// `PageSize`'s range.
    pub fn rules(self) -> Option<&'static crate::Rules> {
        match self {
            Builtin::PageSize => Some(&PAGE_SIZE_RULES),
            _ => None,
        }
    }

    pub fn is_number(self) -> bool {
        matches!(self, Builtin::Int | Builtin::Decimal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_read_back() {
        for builtin in Builtin::ALL {
            assert_eq!(Builtin::from_name(builtin.name()), Some(builtin));
        }
        assert_eq!(Builtin::Email.representation(), Builtin::Text);
        assert!(!Builtin::Url.is_representation());
        assert_eq!(Builtin::PageSize.representation(), Builtin::Int);
        assert_eq!(
            Builtin::PageSize.rules().and_then(|rules| rules.max),
            Some(crate::Num::whole(100))
        );
    }
}
