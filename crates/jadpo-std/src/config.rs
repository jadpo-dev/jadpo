//! Configuration as the runtime reads it (area 10, "Settings:
//! `config.jadpo`"): the settings `config { }` declares, as plain data
//! `jadpo-plan` decides and `jadpo-runtime` runs for either executor, on
//! any platform. A deployable asks its host for each setting's variable
//! ([`ConfigField::variable`]), checks the value against the setting's type
//! with the codec ([`ConfigPlan::codecs`]) and refuses to start if any is
//! missing or invalid; nothing here reads an environment itself.
//!
//! Every value, a default and a test value included, is held in the one
//! spelling an environment variable has (`25`, `5s`, `Europe/London`,
//! `info`), so one reading decides what a valid value is whichever way it
//! arrived.

use crate::http::{Codec, CodecId};

/// How a setting's environment spelling, and the wire, differ from the
/// name its enum codec reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Spelling {
    /// The codec reads the text as it is: numbers, text, durations, a
    /// declared enum's variant name.
    Plain,
    /// A standard enum (area 19, question 22), spelt by its data entry's
    /// codes: an IANA zone name (`Europe/London`) for a `Zone` variant
    /// (`europe_london`), a BCP 47 tag for a `Locale`, a media type for a
    /// `FileType`, an ISO 4217 code (`GBP`) for a `Currency`; read
    /// ignoring case.
    Codes(&'static [(&'static str, &'static str)]),
    /// A duration as source writes it (`5m`), which the codec reads as the
    /// wire's ISO 8601 (`PT5M`, area 10).
    Duration,
}

impl Spelling {
    /// How enum `name` is spelt: by its codes, for a standard enum.
    pub fn of(name: &str) -> Spelling {
        match crate::standard_type(name).and_then(crate::StandardType::codes) {
            Some(codes) => Spelling::Codes(codes),
            None => Spelling::Plain,
        }
    }

    /// The text the codec reads for `text` as an environment gives it, or
    /// `None` when this spelling has no such zone or locale.
    pub fn variant(self, text: &str) -> Option<String> {
        let table = match self {
            Spelling::Plain => return Some(text.to_string()),
            Spelling::Duration => {
                let millis = i64::try_from(crate::Dur::parse(text)?.millis).ok()?;
                return Some(crate::iso_duration(millis));
            }
            Spelling::Codes(table) => table,
        };
        table
            .iter()
            .find(|(_, tag)| tag.eq_ignore_ascii_case(text))
            .map(|(variant, _)| (*variant).to_string())
    }

    /// The environment and wire spelling of variant `variant`.
    pub fn spelling(self, variant: &str) -> String {
        let table = match self {
            Spelling::Plain | Spelling::Duration => return variant.to_string(),
            Spelling::Codes(table) => table,
        };
        table
            .iter()
            .find(|(name, _)| *name == variant)
            .map_or_else(|| variant.to_string(), |(_, tag)| (*tag).to_string())
    }
}

/// What `jadpo test` makes for a secret, which a test run is given at
/// random and never writes anywhere (area 10, question 11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TestSecret {
    /// Hexadecimal digits: at least `min` and at most `max` of them.
    Hex { min: u32, max: Option<u32> },
    /// A PostgreSQL address no one listens at.
    PostgresUrl,
    /// A Redis address no one listens at.
    RedisUrl,
}

/// One setting.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConfigField {
    /// Its name in `config { }` and in code, `config.<name>`.
    pub name: String,
    /// The environment variable it is read from: its name in capitals, or
    /// the `binding:` it gives.
    pub variable: String,
    /// A secret never leaves the server: it is not printed, logged or
    /// returned, and code cannot read it (area 10, "Secrets").
    pub secret: bool,
    /// How its value is read and checked.
    pub codec: CodecId,
    pub spelling: Spelling,
    /// Its `default:`, in the spelling a variable has; none for a secret.
    pub default: Option<String>,
    /// Its `test:` value, used by every test; none for a secret.
    pub test: Option<String>,
    /// How a test run makes a secret's value.
    pub generated: Option<TestSecret>,
    /// A signing key, which a previous value may also be given for
    /// ([`previous_variable`]).
    pub rotates: bool,
}

/// A database connection and the setting holding its address.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConfigConnection {
    /// The connection's name, which an entity's `store:` names.
    pub connection: String,
    pub driver: crate::Driver,
    /// The setting's number in [`ConfigPlan::fields`].
    pub field: u32,
}

/// The settings of a project, with the codecs their values are read by.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConfigPlan {
    /// In the order `config { }` declares them; a setting's number is its
    /// place here.
    pub fields: Vec<ConfigField>,
    pub codecs: Vec<Codec>,
    /// Each connection whose `url:` reads a setting.
    pub connections: Vec<ConfigConnection>,
}

impl ConfigPlan {
    pub fn field(&self, name: &str) -> Option<(u32, &ConfigField)> {
        self.fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == name)
            .map(|(at, field)| (at as u32, field))
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

/// Whether `name` is an environment variable name Jadpo reads: capitals,
/// digits and `_`, starting with a capital.
pub fn is_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// The variable a setting named `field` reads when it gives no
/// `binding:`: its name in capitals (area 10, question 1).
pub fn derived_variable(field: &str) -> String {
    field.to_ascii_uppercase()
}

/// The variable holding the previous key of a `SigningKey` setting read
/// from `variable`, which new tokens are no longer signed with but old
/// ones are still accepted by (area 10, "Signing key rotation").
pub fn previous_variable(variable: &str) -> String {
    format!("{variable}_PREVIOUS")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_are_capitals() {
        assert!(is_variable_name("MAIL_SENDER"));
        assert!(is_variable_name("HEROKU_POSTGRESQL_URL"));
        assert!(!is_variable_name("MAIL-SENDER"));
        assert!(!is_variable_name("mail_sender"));
        assert!(!is_variable_name("1MAIL"));
        assert!(!is_variable_name(""));
        assert_eq!(derived_variable("mail_sender"), "MAIL_SENDER");
        assert_eq!(
            previous_variable("SESSION_SIGNING_KEY"),
            "SESSION_SIGNING_KEY_PREVIOUS"
        );
    }

    #[test]
    fn zones_and_locales_are_read_from_their_tags() {
        assert_eq!(
            Spelling::of(crate::ZONE)
                .variant("Europe/London")
                .as_deref(),
            Some("europe_london")
        );
        assert_eq!(Spelling::of(crate::ZONE).variant("Nowhere/Land"), None);
        assert_eq!(
            Spelling::of(crate::LOCALE).variant("en-GB").as_deref(),
            Some("en_gb")
        );
        assert_eq!(
            Spelling::of(crate::ZONE).spelling("europe_london"),
            "Europe/London"
        );
        assert_eq!(Spelling::Plain.variant("info").as_deref(), Some("info"));
    }
}
