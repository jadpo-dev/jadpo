//! The standard types' data entries (area 19, "The standard types" and
//! "Every standard type meets one contract", question 2).
//!
//! Every standard type is a stub declaration in `jadpo-check`'s
//! `std/standard.jadpo`, for programs to name, plus one entry here, which
//! says everything the contract needs that a declaration can't: its
//! canonical text, its JSON shape, its storage column, the locale data it
//! is formatted for people with and the options that takes, whether it is
//! ordered, the named validator its text needs, the shape `type.standard_exists`
//! matches, and the examples the one harness checks it with. Each baseline
//! operation (`text.parse`, `text.format`, `text.from`, JSON, storage, `==`
//! and order) has one implementation that reads these entries, so a later
//! type group is a stub and an entry, and at most one validator, never a
//! new code path. A test holds the stubs and the entries to each other.

use crate::read::SqlType;
use crate::{Builtin, LOCALE, ZONE};

/// What a standard type's values are held as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holds {
    /// A type built into the language (`jadpo_std::Builtin`): its stub's
    /// base is its own name, since no declaration can say what it is made
    /// of.
    Builtin(Builtin),
    /// An enum whose variants are a table here or in `jadpo-std` (its stub
    /// lists them, or none when the table is long).
    Enum(&'static [(&'static str, &'static str)]),
    /// An object with these fields, in declared order, each built on a
    /// built-in type.
    Object(&'static [(&'static str, Builtin)]),
}

/// A standard type's canonical text: the one form `text.from` writes and
/// `text.parse` reads back (the contract's "canonical text").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextForm {
    /// The text itself, accepted when the type's named validator accepts
    /// it: an email address, a URL, an IP address.
    Checked,
    /// A UUID in lowercase hexadecimal with hyphens, read in either case.
    Uuid,
    /// RFC 3339 in UTC to the millisecond, `2026-10-25T09:30:00Z`.
    Instant,
    /// ISO 8601, `2026-10-25`.
    Date,
    /// RFC 9557: the local time, its offset and its zone,
    /// `2026-10-25T09:30:00+00:00[Europe/London]`.
    Zoned,
    /// ISO 8601 of days, hours, minutes and seconds, `PT1H30M`.
    Duration,
    /// An enum's code (question 22): the variant's entry in the enum's
    /// table, read ignoring case.
    Code,
    /// ISO 8601's interval of two instants, `<start>/<end>`.
    Interval,
}

/// How a standard type is sent and read as JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Json {
    /// Its canonical text as a JSON string.
    Text,
    /// An object of these fields, each in its own canonical text:
    /// `ZonedTime`'s `instant` and `zone` (area 10), `InstantRange`'s
    /// `start` and `end`.
    Fields(&'static [&'static str]),
}

/// The locale data a standard type is formatted for people with: what
/// `text.format(value, locale:)` reads (the contract's "format for
/// people").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatKey {
    /// Its canonical text: the locale doesn't change how it is shown.
    Canonical,
    /// CLDR's date and time patterns, in the format `format:` names (area
    /// 10, "Showing dates and times to people").
    DateTime,
    /// Each end in the format `format:` names, joined by CLDR's interval
    /// fallback, `{0} – {1}`.
    Interval,
    /// CLDR's duration units, written out in full: "1 hour, 30 minutes".
    DurationUnits,
    /// CLDR's stand-alone weekday names, in full: "Monday".
    WeekdayName,
    /// CLDR's locale display names: "British English", "French (France)".
    LocaleName,
    /// CLDR's generic location format for a zone: "UK Time".
    ZoneName,
}

/// An option `text.format` takes for a type, by its parameter's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatOption {
    pub name: &'static str,
    /// Whether a call must give it, as `zone:` for an `Instant`.
    pub needed: bool,
}

/// How `text.format` shows a type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    pub key: FormatKey,
    /// The options the type takes; any other of `text.format`'s options
    /// means nothing for it.
    pub options: &'static [FormatOption],
}

/// A named validator: the one check a type's text needs that its rules
/// can't say (area 19, question 2): IBAN's mod-97, Luhn and the rest
/// arrive with their groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Validator {
    Email,
    Url,
    IpAddress,
    Uuid,
}

impl Validator {
    /// Whether `text` passes.
    pub fn accepts(self, text: &str) -> bool {
        match self {
            Validator::Email => crate::is_email(text),
            Validator::Url => crate::is_url(text),
            Validator::IpAddress => crate::is_ip_address(text),
            Validator::Uuid => crate::uuid_version(text).is_some(),
        }
    }
}

/// One field of a shape `type.standard_exists` matches: the names it goes
/// by and what it is built on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldMatch {
    /// What the field is, as an error says it: "a start".
    pub meaning: &'static str,
    /// The field's common names (question 5), the standard's own first.
    pub names: &'static [&'static str],
    /// The built-in types a matching field is built on.
    pub built_on: &'static [Builtin],
}

/// The examples the harness checks a type with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Examples {
    /// Canonical texts of valid values, in ascending order when the type
    /// is ordered, with each one formatted for people in `en_gb` (with
    /// `zone: Zone.europe_london` and the format below where the type
    /// takes them).
    pub valid: &'static [(&'static str, &'static str)],
    /// The built-in format the examples are shown in, where the type
    /// takes `format:`.
    pub format: Option<&'static str>,
    /// Texts that aren't the type at all: `text.parse` fails `malformed`.
    pub malformed: &'static [&'static str],
}

/// One standard type's data entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StandardType {
    pub name: &'static str,
    pub holds: Holds,
    /// `None` for a type with no single text form (`PostalAddress`), for
    /// which `text.from` and `text.parse` are compile errors.
    pub text: Option<TextForm>,
    pub json: Json,
    /// Its storage column.
    pub storage: SqlType,
    pub format: Format,
    /// Whether `<` and ordering take it.
    pub ordered: bool,
    pub validator: Option<Validator>,
    /// The shape an application's own `Object` copies it by, when it has a
    /// distinctive one (question 5): its fields, in any order, and no
    /// others.
    pub shape: Option<&'static [FieldMatch]>,
    pub examples: Examples,
    /// The Jadpo release that added it (area 19, question 4): an
    /// application's own declaration of its name then clashes,
    /// `type.standard_clash` says since when, and the release's notes list
    /// it under "Names each release adds".
    pub release: &'static str,
}

/// The release the standard types built before Jadpo's first release
/// arrive in: the workspace's version until then.
pub const FIRST_RELEASE: &str = "0.1.0";

/// `FileType` (area 16): a file's IANA media type. Its variants are the
/// media types the pages name (chosen in chunk 5.5a); a later chunk adds
/// more.
pub const FILE_TYPE: &str = "FileType";

/// `FileType`'s variants with their media types, in declared order.
pub const FILE_TYPES: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("pdf", "application/pdf"),
    ("csv", "text/csv"),
    ("tsv", "text/tab-separated-values"),
    ("json", "application/json"),
];

/// `Weekday`'s variants, each its own code.
const WEEKDAYS: &[(&str, &str)] = &[
    ("monday", "monday"),
    ("tuesday", "tuesday"),
    ("wednesday", "wednesday"),
    ("thursday", "thursday"),
    ("friday", "friday"),
    ("saturday", "saturday"),
    ("sunday", "sunday"),
];

const NO_OPTIONS: &[FormatOption] = &[];
const FORMAT: FormatOption = FormatOption {
    name: "format",
    needed: true,
};
const ZONE_OPTION: FormatOption = FormatOption {
    name: "zone",
    needed: true,
};

const fn canonical() -> Format {
    Format {
        key: FormatKey::Canonical,
        options: NO_OPTIONS,
    }
}

/// A type built into the language whose text is checked by a validator.
const fn checked(
    name: &'static str,
    builtin: Builtin,
    validator: Validator,
    examples: Examples,
) -> StandardType {
    StandardType {
        name,
        holds: Holds::Builtin(builtin),
        text: Some(TextForm::Checked),
        json: Json::Text,
        storage: SqlType::Text,
        format: canonical(),
        ordered: false,
        validator: Some(validator),
        shape: None,
        examples,
        release: FIRST_RELEASE,
    }
}

/// Every standard type built so far, sorted by name: the 13 areas 2, 10
/// and 16 built, run through the contract in chunk 5.5a.
pub const STANDARD_TYPES: &[StandardType] = &[
    // Day-and-month calendar dates, ordered by day.
    StandardType {
        name: "CalendarDate",
        holds: Holds::Builtin(Builtin::CalendarDate),
        text: Some(TextForm::Date),
        json: Json::Text,
        storage: SqlType::Date,
        format: Format {
            key: FormatKey::DateTime,
            options: &[FORMAT],
        },
        ordered: true,
        validator: None,
        shape: None,
        examples: Examples {
            valid: &[("2026-02-28", "28 Feb 2026"), ("2026-10-25", "25 Oct 2026")],
            format: Some("date_medium"),
            malformed: &["2026-02-30", "25/10/2026", ""],
        },
        release: FIRST_RELEASE,
    },
    StandardType {
        name: "Duration",
        holds: Holds::Builtin(Builtin::Duration),
        text: Some(TextForm::Duration),
        json: Json::Text,
        // Milliseconds, as the runtime holds a duration.
        storage: SqlType::Int8,
        format: Format {
            key: FormatKey::DurationUnits,
            options: NO_OPTIONS,
        },
        ordered: true,
        validator: None,
        shape: None,
        examples: Examples {
            valid: &[
                ("PT5M", "5 minutes"),
                ("PT1H30M", "1 hour, 30 minutes"),
                ("PT26H", "1 day, 2 hours"),
            ],
            format: None,
            malformed: &["5 minutes", "P", "PT"],
        },
        release: FIRST_RELEASE,
    },
    checked(
        "Email",
        Builtin::Email,
        Validator::Email,
        Examples {
            valid: &[("someone@example.com", "someone@example.com")],
            format: None,
            malformed: &["someone", "someone@", "@example.com"],
        },
    ),
    StandardType {
        name: FILE_TYPE,
        holds: Holds::Enum(FILE_TYPES),
        text: Some(TextForm::Code),
        json: Json::Text,
        storage: SqlType::Text,
        format: canonical(),
        ordered: false,
        validator: None,
        shape: None,
        examples: Examples {
            valid: &[
                ("image/png", "image/png"),
                ("application/pdf", "application/pdf"),
            ],
            format: None,
            malformed: &["png", "image/tiff"],
        },
        release: FIRST_RELEASE,
    },
    StandardType {
        name: "Instant",
        holds: Holds::Builtin(Builtin::Instant),
        text: Some(TextForm::Instant),
        json: Json::Text,
        storage: SqlType::Timestamp,
        format: Format {
            key: FormatKey::DateTime,
            options: &[ZONE_OPTION, FORMAT],
        },
        ordered: true,
        validator: None,
        shape: None,
        examples: Examples {
            valid: &[
                ("2026-03-29T00:30:00Z", "29 Mar 2026, 00:30:00"),
                ("2026-10-25T09:30:00.250Z", "25 Oct 2026, 09:30:00"),
            ],
            format: Some("date_time_medium"),
            malformed: &["2026-10-25", "2026-10-25T09:30:00", "yesterday"],
        },
        release: FIRST_RELEASE,
    },
    // A half-open range of instants, from `start` up to but not including
    // `end` (area 10, "Zones and local times").
    StandardType {
        name: INSTANT_RANGE,
        holds: Holds::Object(&INSTANT_RANGE_FIELDS),
        text: Some(TextForm::Interval),
        json: Json::Fields(&["start", "end"]),
        // Stored as its JSON, as an object is, until objects get named
        // columns with `Money` (chunk 5.5h).
        storage: SqlType::Json,
        format: Format {
            key: FormatKey::Interval,
            options: &[ZONE_OPTION, FORMAT],
        },
        ordered: false,
        validator: None,
        shape: Some(&[
            FieldMatch {
                meaning: "a start",
                names: &["start", "starts_at", "start_at"],
                built_on: &[Builtin::Instant],
            },
            FieldMatch {
                meaning: "an end",
                names: &["end", "ends_at", "end_at"],
                built_on: &[Builtin::Instant],
            },
        ]),
        examples: Examples {
            valid: &[(
                "2026-10-25T09:00:00Z/2026-10-25T17:00:00Z",
                "25 Oct 2026, 09:00:00 – 25 Oct 2026, 17:00:00",
            )],
            format: Some("date_time_medium"),
            malformed: &["2026-10-25T09:00:00Z", "2026-10-25T09:00:00Z/tomorrow"],
        },
        release: FIRST_RELEASE,
    },
    checked(
        "IpAddress",
        Builtin::IpAddress,
        Validator::IpAddress,
        Examples {
            valid: &[("192.0.2.1", "192.0.2.1"), ("2001:db8::1", "2001:db8::1")],
            format: None,
            malformed: &["192.0.2", "example.com"],
        },
    ),
    StandardType {
        name: LOCALE,
        holds: Holds::Enum(crate::zones::LOCALES),
        text: Some(TextForm::Code),
        json: Json::Text,
        storage: SqlType::Text,
        format: Format {
            key: FormatKey::LocaleName,
            options: NO_OPTIONS,
        },
        ordered: false,
        validator: None,
        shape: None,
        examples: Examples {
            valid: &[("en-GB", "British English"), ("fr-FR", "French (France)")],
            format: None,
            malformed: &["english", "xx-YY"],
        },
        release: FIRST_RELEASE,
    },
    checked(
        "Url",
        Builtin::Url,
        Validator::Url,
        Examples {
            valid: &[("https://example.com/orders", "https://example.com/orders")],
            format: None,
            malformed: &["example.com", "ftp://example.com"],
        },
    ),
    StandardType {
        name: "Uuid",
        holds: Holds::Builtin(Builtin::Uuid),
        text: Some(TextForm::Uuid),
        json: Json::Text,
        storage: SqlType::Uuid,
        format: canonical(),
        ordered: false,
        validator: Some(Validator::Uuid),
        shape: None,
        examples: Examples {
            valid: &[(
                "0192f0c4-9a1e-7b3c-8d2e-4f5a6b7c8d9e",
                "0192f0c4-9a1e-7b3c-8d2e-4f5a6b7c8d9e",
            )],
            format: None,
            malformed: &["0192f0c4", "not-a-uuid"],
        },
        release: FIRST_RELEASE,
    },
    StandardType {
        name: crate::Weekday::NAME,
        holds: Holds::Enum(WEEKDAYS),
        text: Some(TextForm::Code),
        json: Json::Text,
        storage: SqlType::Text,
        format: Format {
            key: FormatKey::WeekdayName,
            options: NO_OPTIONS,
        },
        ordered: false,
        validator: None,
        shape: None,
        examples: Examples {
            valid: &[("monday", "Monday"), ("sunday", "Sunday")],
            format: None,
            malformed: &["mon", "funday"],
        },
        release: FIRST_RELEASE,
    },
    StandardType {
        name: ZONE,
        holds: Holds::Enum(crate::zones::ZONES),
        text: Some(TextForm::Code),
        json: Json::Text,
        storage: SqlType::Text,
        format: Format {
            key: FormatKey::ZoneName,
            options: NO_OPTIONS,
        },
        ordered: false,
        validator: None,
        shape: None,
        examples: Examples {
            valid: &[
                ("Europe/London", "UK Time"),
                ("America/New_York", "New York Time"),
            ],
            format: None,
            malformed: &["London", "GMT+1"],
        },
        release: FIRST_RELEASE,
    },
    StandardType {
        name: "ZonedTime",
        holds: Holds::Builtin(Builtin::ZonedTime),
        text: Some(TextForm::Zoned),
        json: Json::Fields(&["instant", "zone"]),
        // An instant and its zone, stored as its JSON (area 5's structured
        // values), as an object is.
        storage: SqlType::Json,
        format: Format {
            key: FormatKey::DateTime,
            options: &[FORMAT],
        },
        // Its instant is ordered; a local time on its own isn't (area 10).
        ordered: false,
        validator: None,
        shape: None,
        examples: Examples {
            valid: &[(
                "2026-10-25T09:30:00+00:00[Europe/London]",
                "25 Oct 2026, 09:30:00",
            )],
            format: Some("date_time_medium"),
            malformed: &["2026-10-25T09:30:00Z", "2026-10-25T09:30:00+00:00"],
        },
        release: FIRST_RELEASE,
    },
];

/// `InstantRange`: what `temporal`'s bounds give, a half-open range from
/// `start` up to but not including `end` (area 10, "Zones and local
/// times").
pub const INSTANT_RANGE: &str = "InstantRange";

/// `InstantRange`'s fields, in order.
pub const INSTANT_RANGE_FIELDS: [(&str, Builtin); 2] =
    [("start", Builtin::Instant), ("end", Builtin::Instant)];

/// The standard type called `name`.
pub fn standard_type(name: &str) -> Option<&'static StandardType> {
    STANDARD_TYPES.iter().find(|entry| entry.name == name)
}

/// Every release that added a standard type, oldest first, each with the
/// names it added in name order: the release notes' "Names each release
/// adds" (area 19, question 4).
pub fn releases() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut out: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
    for entry in STANDARD_TYPES {
        match out
            .iter_mut()
            .find(|(release, _)| *release == entry.release)
        {
            Some((_, names)) => names.push(entry.name),
            None => out.push((entry.release, vec![entry.name])),
        }
    }
    out.sort_by_key(|(release, _)| version(release));
    out
}

/// A release's version as numbers, for ordering: `0.1.0` is `[0, 1, 0]`.
pub fn version(release: &str) -> Vec<u32> {
    release
        .split('.')
        .map(|part| part.parse().unwrap_or(u32::MAX))
        .collect()
}

/// The standard type a built-in type is, if one: `Email`, `Instant`.
pub fn of_builtin(builtin: Builtin) -> Option<&'static StandardType> {
    STANDARD_TYPES
        .iter()
        .find(|entry| entry.holds == Holds::Builtin(builtin))
}

impl StandardType {
    /// The enum's variants and their codes, for an enum.
    pub fn codes(&self) -> Option<&'static [(&'static str, &'static str)]> {
        match self.holds {
            Holds::Enum(codes) => Some(codes),
            _ => None,
        }
    }

    /// The format option called `name`, if the type takes it.
    pub fn format_option(&self, name: &str) -> Option<&'static FormatOption> {
        self.format
            .options
            .iter()
            .find(|option| option.name == name)
    }

    /// How its column becomes a value, for a type stored in a column of its
    /// own; `None` for one stored as its JSON.
    pub fn column(&self) -> Option<crate::read::Column> {
        use crate::read::Column;
        Some(match (self.storage, self.holds) {
            (SqlType::Text, Holds::Enum(codes)) => Column::Enum {
                ty: self.name.to_string(),
                variants: codes.iter().map(|(name, _)| (*name).to_string()).collect(),
            },
            (SqlType::Text, _) => Column::Text,
            (SqlType::Uuid, _) => Column::Uuid,
            (SqlType::Timestamp, _) => Column::Instant,
            (SqlType::Date, _) => Column::Date,
            (SqlType::Int8, Holds::Builtin(Builtin::Duration)) => Column::Duration,
            _ => return None,
        })
    }

    /// The PostgreSQL column type its storage is.
    pub fn column_type(&self) -> &'static str {
        match self.storage {
            SqlType::Bool => "boolean",
            SqlType::Int2 => "smallint",
            SqlType::Int4 => "integer",
            SqlType::Int8 => "bigint",
            SqlType::Numeric => "numeric",
            SqlType::Text => "text",
            SqlType::Bytes => "bytea",
            SqlType::Uuid => "uuid",
            SqlType::Timestamp => "timestamptz(3)",
            SqlType::Date => "date",
            SqlType::Time => "time",
            SqlType::Json | SqlType::Array(_) => "jsonb",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_sorted_and_each_listed_once() {
        let names: Vec<&str> = STANDARD_TYPES.iter().map(|entry| entry.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted);
        assert_eq!(names.len(), 13, "the 13 standard types built so far");
    }

    #[test]
    fn every_entry_is_whole() {
        for entry in STANDARD_TYPES {
            assert!(
                !entry.examples.valid.is_empty() && !entry.examples.malformed.is_empty(),
                "{} has examples",
                entry.name
            );
            if entry.text == Some(TextForm::Checked) {
                assert!(entry.validator.is_some(), "{} checks its text", entry.name);
            }
            if let Holds::Enum(codes) = entry.holds {
                assert_eq!(entry.text, Some(TextForm::Code), "{}", entry.name);
                assert!(!codes.is_empty());
            }
            if let (Holds::Object(fields), Json::Fields(names)) = (entry.holds, entry.json) {
                let declared: Vec<&str> = fields.iter().map(|(name, _)| *name).collect();
                assert_eq!(declared, names, "{}'s JSON is its fields", entry.name);
            }
            assert_eq!(
                entry.examples.format.is_some(),
                entry.format_option("format").is_some(),
                "{}'s examples name a format exactly when it takes one",
                entry.name
            );
            // A distinctive shape (question 5): two fields or more, at
            // least one named for the domain.
            if let Some(fields) = entry.shape {
                assert!(fields.len() >= 2, "{}", entry.name);
                let generic = ["name", "value", "id", "code", "type", "text"];
                assert!(
                    fields
                        .iter()
                        .any(|field| field.names.iter().all(|name| !generic.contains(name))),
                    "{}'s shape names a field for its domain",
                    entry.name
                );
            }
            for option in entry.format.options {
                assert!(
                    ["zone", "format"].contains(&option.name),
                    "{}: `{}:` is one of `text.format`'s options",
                    entry.name,
                    option.name
                );
            }
        }
        assert_eq!(of_builtin(Builtin::Email).map(|e| e.name), Some("Email"));
        assert_eq!(of_builtin(Builtin::Text), None);
        assert_eq!(
            standard_type(INSTANT_RANGE).map(StandardType::column_type),
            Some("jsonb")
        );
    }

    #[test]
    fn each_entry_names_a_release_no_later_than_this_one() {
        let this = version(env!("CARGO_PKG_VERSION"));
        for entry in STANDARD_TYPES {
            let release = version(entry.release);
            assert_eq!(release.len(), 3, "{}: `major.minor.patch`", entry.name);
            assert!(release <= this, "{} is from a later release", entry.name);
        }
        let all: usize = releases().iter().map(|(_, names)| names.len()).sum();
        assert_eq!(all, STANDARD_TYPES.len());
    }

    #[test]
    fn named_validators_check_text() {
        assert!(Validator::Email.accepts("someone@example.com"));
        assert!(!Validator::Url.accepts("example.com"));
        assert!(Validator::IpAddress.accepts("::1"));
        assert!(!Validator::Uuid.accepts("0192f0c4"));
    }
}
