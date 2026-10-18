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

pub mod addresses_data;
mod contact;
pub mod emoji_data;
pub mod events;
mod identifiers;
pub mod opening_hours;
pub mod person_names_data;
pub mod phones;
mod phones_data;
pub mod places;
mod places_data;
pub mod postal;
mod postal_codes_data;
pub mod recurrence;
mod schedules;
pub mod web;

pub use contact::{LINE_MAX, LINE_PATTERN};
pub use places::{ClosedList, closed_list, retired, subdivision_name};

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
    /// An object with these fields, in declared order, each a [`Part`].
    Object(&'static [(&'static str, Part)]),
    /// `Range<T>` (area 19, "Ranges"): a `start` and an `end` ([`RANGE_FIELDS`])
    /// of one ordered type, the type argument, either of them `none` for
    /// an open end.
    Range,
    /// Text that matches `pattern` whole and passes the entry's named
    /// validator, if it has one (area 19, "Groups with the baseline
    /// only"): an IBAN, a card number. A program holds it as a type built
    /// on `Text` that the standard library declares, whose own check
    /// ([`crate::Rules::standard`]) a value of other text breaks as
    /// `format`. `personal` is the `PersonalData.` category its values
    /// are (area 15), for a type that is personal data whatever field
    /// holds it: area 15's build treats such a field as marked with it.
    Text {
        pattern: &'static str,
        personal: Option<&'static str>,
    },
    /// A whole number from `min` to `max`, both included (chunk 5.5n): an
    /// HTTP status, a port. A program holds it as a type built on `Int`
    /// that the standard library declares, with the range as its rules
    /// and its name as their `standard` ([`declared`]), so construction,
    /// `<`, JSON (a number) and storage are an `Int`'s, and `text.parse`
    /// reads only its canonical digits, failing `malformed` outside the
    /// range, as a type held as text fails it for text outside its check.
    Whole { min: i64, max: i64 },
}

/// What one field of a standard object type holds (area 19, chunk 5.5k).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// A value built on this built-in type, with no rules of its own:
    /// `InstantRange`'s instants, `CalendarPeriod`'s parts.
    Of(Builtin),
    /// A whole number from `min` to `max`, both included: `LocalTime`'s
    /// `hour` from 0 to 23. Its object literal proves the range as an
    /// object's field rules are proved (area 2).
    Whole(i64, i64),
    /// A value of another standard type, by its name: `LocalDateTime`'s
    /// `time`, a `LocalTime`.
    Standard(&'static str),
    /// One line of text for people ([`LINE_PATTERN`]): an address's
    /// lines, the name in a mailbox (chunk 5.5l).
    Line,
    /// Text for people that may run over several lines, with no control
    /// character but a tab or a line feed ([`events::PARAGRAPH_PATTERN`]):
    /// an event's description (chunk 5.5k3).
    Paragraph,
    /// The part, or `none`: a person's family name.
    Optional(&'static Part),
    /// A list of the part: an address's lines.
    List(&'static Part),
}

impl Part {
    /// The built-in type a field's values are built on; `None` for a
    /// field holding another standard type.
    pub const fn builtin(self) -> Option<Builtin> {
        match self {
            Part::Of(builtin) => Some(builtin),
            Part::Whole(..) => Some(Builtin::Int),
            Part::Line | Part::Paragraph => Some(Builtin::Text),
            Part::Standard(_) | Part::Optional(_) | Part::List(_) => None,
        }
    }

    /// The rules a value of the part keeps, where it is a whole number with
    /// a range or a line of text; none for any other.
    pub fn rules(self) -> crate::Rules {
        let whole = |n: i64| crate::Num {
            mantissa: i128::from(n),
            scale: 0,
        };
        match self {
            Part::Whole(min, max) => crate::Rules {
                min: Some(whole(min)),
                max: Some(whole(max)),
                ..crate::Rules::default()
            },
            Part::Line => crate::Rules {
                pattern: Some(LINE_PATTERN.to_string()),
                ..crate::Rules::default()
            },
            Part::Paragraph => crate::Rules {
                pattern: Some(events::PARAGRAPH_PATTERN.to_string()),
                ..crate::Rules::default()
            },
            Part::Optional(inner) | Part::List(inner) => inner.rules(),
            Part::Of(_) | Part::Standard(_) => crate::Rules::default(),
        }
    }

    /// The part itself, for one that may be `none`, with whether it may.
    pub const fn required(self) -> (Part, bool) {
        match self {
            Part::Optional(inner) => (*inner, true),
            other => (other, false),
        }
    }
}

/// A standard type's canonical text: the one form `text.from` writes and
/// `text.parse` reads back (the contract's "canonical text").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextForm {
    /// The text itself, accepted when the type's named validator accepts
    /// it: an email address, a URL, an IP address; for a type held as
    /// text, when its pattern matches too ([`StandardType::accepts`]).
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
    /// ISO 8601's interval of two instants, `<start>/<end>`; of two
    /// dates for a `DateRange`, `2026-10-01/2026-10-31`, its end included
    /// as its type says.
    Interval,
    /// ISO 8601's local time, to the second: `09:30:00`, from `00:00:00`
    /// to `23:59:59`.
    LocalTime,
    /// ISO 8601's local date and time with no offset, to the second:
    /// `2026-10-25T09:30:00`.
    LocalDateTime,
    /// ISO 8601's calendar month, `2026-10`.
    YearMonth,
    /// ISO 8601's year, in four digits: `2026`.
    Year,
    /// A year and its quarter, `2026-Q4`.
    Quarter,
    /// ISO 8601's week date without a day, `2026-W43`: a week of an ISO
    /// week-numbering year, which has 52 or 53.
    IsoWeek,
    /// ISO 8601's offset from UTC, a sign and hours and minutes:
    /// `+01:00`, `-05:30`, and `+00:00` for UTC itself.
    UtcOffset,
    /// ISO 8601's period of years, months and days, `P1Y2M10D`: each
    /// part that isn't zero, in that order, with its own sign when
    /// negative (`P-1M-3D`), as PostgreSQL writes an interval; nothing at
    /// all is `P0D`.
    Period,
    /// A range's two ends, each in its item type's canonical text, joined
    /// by `/`, with an open end written `..` as ISO 8601-2 writes one:
    /// `2026-10-01/..`.
    Range,
    /// An amount of money: its currency's ISO 4217 code, a space, and the
    /// amount in major units with exactly the currency's places, `GBP
    /// 10.50`, `JPY 500`, `BHD 1.250` (chosen in chunk 5.5h); the code is
    /// read ignoring case.
    Money,
    /// A rate between two currencies: the pair as ISO 4217 codes, base
    /// first, a space, the rate as a `Decimal` with the places it was
    /// given, a space, and the instant it was quoted at, `GBP/EUR 1.17
    /// 2026-10-18T09:00:00Z`: one pound is 1.17 euros (chosen in chunk
    /// 5.5h2); the codes are read ignoring case.
    ExchangeRate,
    /// A mailbox (RFC 5322, with RFC 6532's UTF-8): the name, a space and
    /// the address in angle brackets, `Ada Lovelace <ada@example.com>`, the
    /// name quoted only when it holds a character an atom can't, `"Lovelace,
    /// Ada" <ada@example.com>`; with no name, the address alone (chosen in
    /// chunk 5.5l).
    Mailbox,
    /// A physical quantity (chosen in chunk 5.5j): its number in its
    /// kind's base unit with no trailing zeros, a space, and the base
    /// unit's code, `2.5 kg`, `1000 metre`; read back with any of the
    /// kind's units, `5 km`, the space optional.
    Quantity,
    /// A percentage (chosen in chunk 5.5j2): the fraction it holds as a
    /// percentage with no trailing zeros, and `%`, `20%`, `12.5%`; read
    /// back with a space before the `%` too, as French and German write it.
    Percentage,
    /// A ratio of two decimal numbers, each with the places it was given,
    /// joined by `:`, `16:9`, `1.85:1` (chosen in chunk 5.5j2).
    Ratio,
    /// A fraction of two whole numbers joined by `/`, its denominator at
    /// least 1, `3/4`, `-1/2` (chosen in chunk 5.5j2).
    Fraction,
    /// An RFC 5545 `VEVENT` (chosen in chunk 5.5k3, [`events`]): a
    /// property a line, joined by line feeds, an all-day event's `DTEND`
    /// the day after its last.
    Event,
    /// A whole number's digits, with no sign and no leading zero, within
    /// its type's range: `404` (chunk 5.5n).
    Whole,
    /// The query that carries a visit's UTM parameters (chunk 5.5n,
    /// [`web::utm_text`]): `utm_source=newsletter&utm_medium=email&...`,
    /// each part it has in its parts' order, percent-encoded.
    UtmQuery,
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
    /// An object of these members, as RFC 9457 sends a problem (chunk
    /// 5.5n): like [`Json::Fields`], but a part that is `none` is left out
    /// rather than sent as `null`, and one left out reads as `none`.
    Members(&'static [&'static str]),
    /// Its value as a JSON number: a whole number held by
    /// [`Holds::Whole`], whose range is within JavaScript's safe integers.
    Number,
}

impl Json {
    /// The object's names on the wire, for a type sent as an object.
    pub const fn fields(self) -> Option<&'static [&'static str]> {
        match self {
            Json::Fields(names) | Json::Members(names) => Some(names),
            Json::Text | Json::Number => None,
        }
    }
}

/// The locale data a standard type is formatted for people with: what
/// `text.format(value, locale:)` reads (the contract's "format for
/// people").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatKey {
    /// Its canonical text: the locale doesn't change how it is shown.
    Canonical,
    /// CLDR's date and time patterns, in the format `format:` names (area
    /// 10, "Showing dates and times to people"); a `LocalTime` or a
    /// `LocalDateTime` shows only the parts it has.
    DateTime,
    /// Each end in the format `format:` names, joined by CLDR's interval
    /// fallback, `{0} – {1}`: an `InstantRange`'s instants, a
    /// `DateRange`'s dates.
    Interval,
    /// CLDR's duration units, written out in full: "1 hour, 30 minutes".
    DurationUnits,
    /// CLDR's stand-alone weekday names, in full: "Monday".
    WeekdayName,
    /// CLDR's locale display names: "British English", "French (France)".
    LocaleName,
    /// CLDR's generic location format for a zone: "UK Time".
    ZoneName,
    /// CLDR's stand-alone month names, in full: "October".
    MonthName,
    /// CLDR's pattern for a month and its year, the month in full:
    /// "October 2026".
    YearMonth,
    /// CLDR's pattern for a year on its own: "2026".
    Year,
    /// CLDR's localized GMT format, in full: "GMT+01:00", "GMT+00:00".
    OffsetName,
    /// CLDR's units for years, months and days, written out in full and
    /// joined as the locale joins a list of units: "1 year, 2 months".
    PeriodUnits,
    /// Each end as its item type is shown, with the item type's options,
    /// joined by CLDR's interval fallback, `{0} – {1}`; an open end is
    /// `…`.
    Range,
    /// CLDR's currency formats: the locale's grouping and decimal
    /// separator, the currency's places, and its symbol, code or name in
    /// the place the locale puts it, `£1,234.50`, `1 234,50 €`.
    Money,
    /// CLDR's currency display names: "British Pound".
    CurrencyName,
    /// A quantity in the unit `unit:` names, its kind's base unit by
    /// default: the locale's number and CLDR's short unit pattern where
    /// ICU4X has one (`2.5 m`, `247,62 cm`), else the number, a space and
    /// the unit's symbol (`21 °C`) (chosen in chunk 5.5j).
    Units,
    /// CLDR's percent pattern, through ICU4X: the fraction as a
    /// percentage in the locale's digits and separators, with every place
    /// it keeps, `12.5%`, `20 %` (chosen in chunk 5.5j2).
    Percent,
    /// Its canonical text in groups, as it is printed: an IBAN in fours,
    /// a sort code in pairs with dashes. The locale doesn't change it.
    Grouped(Grouping),
    /// Its last four characters after `mask`, as PCI DSS lets a card
    /// number be shown: `•••• 4242`.
    LastFour { mask: &'static str },
    /// CLDR's territory names, for a country by its code: "United
    /// Kingdom"; a withdrawn code by its list's English name.
    RegionName,
    /// CLDR's names for UN M49's regions: "Europe".
    ContinentName,
    /// CLDR's language names: "Japanese".
    LanguageName,
    /// CLDR's script names: "Latin".
    ScriptName,
    /// CLDR's English subdivision names, "England", in every locale, since
    /// CLDR names few in other languages; the code where it has none.
    SubdivisionName,
    /// CLDR's person-name patterns (UTS #35, "Person Names"), medium and
    /// formal, in the order the name's script and the reader's locale give:
    /// "Ada Lovelace", "Kovács János", "山田太郎".
    PersonName,
    /// libaddressinput's format for the address's country, in its own
    /// script for a reader of one of its languages and in Latin script for
    /// any other, one line per line, with the country's CLDR name last when
    /// the reader's locale is another country's.
    PostalAddress,
    /// libphonenumber's formats for a number's calling code: national,
    /// `07400 123456`, for a reader in a region with that code, and
    /// international, `+44 7400 123456`, for any other (chunk 5.5l2).
    PhoneNumber,
    /// A problem as people read it (RFC 9457, chunk 5.5n): its title and
    /// its detail, each on its own line; with neither, its status; with no
    /// status either, its type, `about:blank` when it has none.
    ProblemDetails,
}

/// How a printed form groups its canonical text: groups of `sizes` in
/// order, the last size repeating to the end, joined by `separator`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grouping {
    pub sizes: &'static [usize],
    pub separator: &'static str,
}

impl Grouping {
    /// `text` in its groups: `GB29 NWBK 6016 1331 9268 19`.
    pub fn apply(&self, text: &str) -> String {
        let chars: Vec<char> = text.chars().collect();
        let mut groups: Vec<String> = Vec::new();
        let mut at = 0;
        let mut sizes = self.sizes.iter();
        let mut size = sizes.next().copied().unwrap_or(chars.len()).max(1);
        while at < chars.len() {
            let end = (at + size).min(chars.len());
            groups.push(chars.get(at..end).unwrap_or_default().iter().collect());
            at = end;
            if let Some(next) = sizes.next() {
                size = (*next).max(1);
            }
        }
        groups.join(self.separator)
    }
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
/// can't say (area 19, question 2), one function each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Validator {
    Email,
    Url,
    IpAddress,
    Uuid,
    /// An IBAN's length for its country and its mod-97 (ISO 13616).
    Iban,
    /// A card number's Luhn check digit (ISO/IEC 7812).
    Luhn,
    /// An ISIN's Luhn check digit over its letters as numbers (ISO 6166).
    Isin,
    /// An LEI's mod-97 (ISO 17442, ISO 7064 mod 97-10).
    Lei,
    /// An ABA routing number's 3-7-1 checksum.
    AbaRoutingNumber,
    /// An NHS number's mod-11 check digit.
    NhsNumber,
    /// A National Insurance number's prefix, none HMRC never allocates.
    UkNationalInsuranceNumber,
    /// An RFC 5545 recurrence whose start is its rule's first occurrence
    /// ([`recurrence`]).
    RecurrenceRule,
    /// OpenStreetMap's opening hours with a zone ([`opening_hours`]).
    OpeningHours,
    /// A phone number libphonenumber's metadata says is valid for its
    /// region ([`phones`]).
    PhoneNumber,
    /// A calling code ITU-T assigned ([`phones`]).
    CallingCode,
    /// A UN/LOCODE whose country is a current one ([`places`]).
    UnLocode,
    /// An address range with its host bits zero (RFC 4632, [`web`]).
    Cidr,
    /// A cookie as a server sets it (RFC 6265, [`web`]).
    Cookie,
    /// A domain name in UTS 46's strict ASCII form ([`web`]).
    DomainName,
    /// One fully-qualified emoji (UTS #51, [`web`]).
    Emoji,
    /// A domain name or an IP address ([`web`]).
    Hostname,
    /// A relative reference (RFC 3986 §4.2, [`web`]).
    RelativeUrl,
    /// A `User-Agent` (RFC 9110 §10.1.5, [`web`]).
    UserAgent,
    /// A web origin's serialisation (RFC 6454, [`web`]).
    WebOrigin,
}

impl Validator {
    /// Whether `text` passes.
    pub fn accepts(self, text: &str) -> bool {
        match self {
            Validator::Email => crate::is_email(text),
            Validator::Url => crate::is_url(text),
            Validator::IpAddress => crate::is_ip_address(text),
            Validator::Uuid => crate::uuid_version(text).is_some(),
            Validator::Iban => identifiers::is_iban(text),
            Validator::Luhn => identifiers::passes_luhn(text),
            Validator::Isin => identifiers::is_isin(text),
            Validator::Lei => identifiers::is_lei(text),
            Validator::AbaRoutingNumber => identifiers::is_aba_routing_number(text),
            Validator::NhsNumber => identifiers::is_nhs_number(text),
            Validator::UkNationalInsuranceNumber => {
                identifiers::is_uk_national_insurance_number(text)
            }
            Validator::RecurrenceRule => recurrence::Rule::read(text).is_some(),
            Validator::OpeningHours => opening_hours::OpeningHours::read(text).is_some(),
            Validator::PhoneNumber => phones::is_valid(text),
            Validator::CallingCode => text.strip_prefix('+').is_some_and(phones::is_assigned),
            Validator::UnLocode => places::is_un_locode(text),
            Validator::Cidr => web::is_cidr(text),
            Validator::Cookie => web::is_cookie(text),
            Validator::DomainName => web::is_domain_name(text),
            Validator::Emoji => web::is_emoji(text),
            Validator::Hostname => web::is_hostname(text),
            Validator::RelativeUrl => web::is_relative_url(text),
            Validator::UserAgent => web::is_user_agent(text),
            Validator::WebOrigin => web::is_web_origin(text),
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
    /// Whether a field holding an enum matches too, as a currency held as
    /// a project's own enum does.
    pub or_enum: bool,
}

/// How a standard type is stored (area 5, "Storage types"; area 19).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Storage {
    /// One column of this type, named for the field.
    Column(SqlType),
    /// Named columns, each its suffix, joined to the field's column name
    /// with `_`, and its PostgreSQL type, in order, always all of them
    /// (`Money`: `amount bigint` and `currency char(3)`). `fixed` names the
    /// one a type built on it can fix to one value, which a check
    /// constraint then holds it to (`Money`'s `currency`, which a type's
    /// `currency:` fixes). One codec turns a value into its columns and
    /// back (`jadpo-value`'s `standard::to_parts` and `from_parts`).
    Columns {
        parts: &'static [(&'static str, &'static str)],
        fixed: Option<&'static str>,
    },
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
    /// For a type that takes a type argument, the one its examples hold:
    /// `CalendarDate` for `Range<CalendarDate>`.
    pub of: Option<&'static str>,
    /// The settings a named type built on it takes in the checker's half of
    /// the harness, where the type needs one to be ordered: a `Money`
    /// type's `currency:`, since only amounts in one currency compare.
    pub settings: &'static str,
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
    /// How it is stored: one column, or named columns.
    pub storage: Storage,
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
        storage: Storage::Column(SqlType::Text),
        format: canonical(),
        ordered: false,
        validator: Some(validator),
        shape: None,
        examples,
        release: FIRST_RELEASE,
    }
}

/// A time type held as an object (area 19, "Time and calendar
/// additions", chunk 5.5k): its fields for code to read, its canonical
/// text as its JSON, as a `Duration`'s is, and stored as that JSON until
/// objects get columns of their own.
const fn time_object(
    name: &'static str,
    fields: &'static [(&'static str, Part)],
    text: TextForm,
    format: Format,
    ordered: bool,
    examples: Examples,
) -> StandardType {
    StandardType {
        name,
        holds: Holds::Object(fields),
        text: Some(text),
        json: Json::Text,
        storage: Storage::Column(SqlType::Json),
        format,
        ordered,
        validator: None,
        shape: None,
        examples,
        release: FIRST_RELEASE,
    }
}

/// A quantity's fields on the wire, in order: its number in the unit and
/// the unit's code (`{ "value": "2.5", "unit": "kg" }`), the number as a
/// string, as area 2 sends every `Decimal` (chosen in chunk 5.5j).
pub const QUANTITY_FIELDS: [&str; 2] = ["value", "unit"];

/// Texts no quantity reads: no number, no unit, or neither.
const NOT_A_QUANTITY: &[&str] = &["5", "", "1.2.3", "five"];

/// A physical quantity (area 19, "Physical quantities", chunk 5.5j): one
/// `numeric` column in its base unit at its scale, ordered, shown in the
/// unit `unit:` names.
const fn quantity(
    kind: crate::QuantityKind,
    valid: &'static [(&'static str, &'static str)],
) -> StandardType {
    StandardType {
        name: kind.name(),
        holds: Holds::Builtin(Builtin::Quantity(kind)),
        text: Some(TextForm::Quantity),
        json: Json::Fields(&QUANTITY_FIELDS),
        storage: Storage::Column(SqlType::Numeric),
        format: Format {
            key: FormatKey::Units,
            options: &[FormatOption {
                name: "unit",
                needed: false,
            }],
        },
        ordered: true,
        validator: None,
        shape: None,
        examples: Examples {
            valid,
            format: None,
            malformed: NOT_A_QUANTITY,
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    }
}

const fn shown_as(key: FormatKey) -> Format {
    Format {
        key,
        options: NO_OPTIONS,
    }
}

/// Shown in a format written out, `format:`, as the time types are.
const IN_A_FORMAT: Format = Format {
    key: FormatKey::DateTime,
    options: &[FORMAT],
};

/// The years a year field holds: those a `CalendarDate` holds (area 10).
const YEARS: Part = Part::Whole(0, 9999);

/// `Month`'s variants, each its own code, as `Weekday`'s are.
pub const MONTHS: &[(&str, &str)] = &[
    ("january", "january"),
    ("february", "february"),
    ("march", "march"),
    ("april", "april"),
    ("may", "may"),
    ("june", "june"),
    ("july", "july"),
    ("august", "august"),
    ("september", "september"),
    ("october", "october"),
    ("november", "november"),
    ("december", "december"),
];

/// `CalendarPeriod` (area 19, "Time and calendar additions").
pub const CALENDAR_PERIOD: &str = "CalendarPeriod";

/// `CalendarPeriod`'s parts, in order: whole years, months and days,
/// each of either sign.
pub const CALENDAR_PERIOD_FIELDS: [(&str, Part); 3] = [
    ("years", Part::Of(Builtin::Int)),
    ("months", Part::Of(Builtin::Int)),
    ("days", Part::Of(Builtin::Int)),
];

/// `RecurrenceRule` (area 19, "Time and calendar additions"), held as its
/// RFC 5545 text.
pub const RECURRENCE_RULE: &str = "RecurrenceRule";

/// `OpeningHours` (area 19, "Time and calendar additions"), held as its
/// OpenStreetMap text and zone.
pub const OPENING_HOURS: &str = "OpeningHours";

/// `DateRange`: the dates from `start` to `end`, both included (area 19,
/// "Ranges").
pub const DATE_RANGE: &str = "DateRange";

/// `ExchangeRate` (area 19, "Money and payments"): one `from` is `rate`
/// of `to`, as quoted at `at`.
pub const EXCHANGE_RATE: &str = "ExchangeRate";

/// `ExchangeRate`'s fields, in order.
pub const EXCHANGE_RATE_FIELDS: [(&str, Part); 4] = [
    ("from", Part::Standard(crate::CURRENCY)),
    ("to", Part::Standard(crate::CURRENCY)),
    ("rate", Part::Of(Builtin::Decimal)),
    ("at", Part::Of(Builtin::Instant)),
];

/// `Ratio` (area 19, "Maths and data"): one decimal number to another,
/// as written.
pub const RATIO: &str = "Ratio";

/// `Ratio`'s fields, in order.
pub const RATIO_FIELDS: [(&str, Part); 2] = [
    ("numerator", Part::Of(Builtin::Decimal)),
    ("denominator", Part::Of(Builtin::Decimal)),
];

/// `Fraction` (area 19, "Maths and data"): an exact rational number, as
/// written.
pub const FRACTION: &str = "Fraction";

/// `Fraction`'s fields, in order: its denominator at least 1.
pub const FRACTION_FIELDS: [(&str, Part); 2] = [
    ("numerator", Part::Of(Builtin::Int)),
    ("denominator", Part::Whole(1, i64::MAX)),
];

/// `DateRange`'s fields, in order.
pub const DATE_RANGE_FIELDS: [(&str, Part); 2] = [
    ("start", Part::Of(Builtin::CalendarDate)),
    ("end", Part::Of(Builtin::CalendarDate)),
];

/// `LocalTime`'s fields, in order.
pub const LOCAL_TIME_FIELDS: [(&str, Part); 3] = [
    ("hour", Part::Whole(0, 23)),
    ("minute", Part::Whole(0, 59)),
    ("second", Part::Whole(0, 59)),
];

/// Every standard type built so far, sorted by name: the 13 areas 2, 10
/// and 16 built, run through the contract in chunk 5.5a, `Range<T>`
/// (chunk 5.5c2), `Money` and `Currency` (chunk 5.5h), ten of the time
/// group (chunk 5.5k), `ExchangeRate` (chunk 5.5h2), `Percentage`,
/// `Ratio` and `Fraction` (chunk 5.5j2), the 17 payment
/// and identity numbers held as text (chunk 5.5i, in `identifiers`),
/// `RecurrenceRule` and `OpeningHours`, held as text (chunk 5.5k2, in
/// `schedules`), 15 of people and places (chunk 5.5l, in `places` and
/// `contact`), and `PhoneNumber`, `CallingCode`, `PostalCode` and
/// `UnLocode`, held as text (chunk 5.5l2, in `phones`, `postal` and
/// `places`), and `CalendarEvent` (chunk 5.5k3, in `events`).
pub const STANDARD_TYPES: &[StandardType] = &[
    identifiers::ABA_ROUTING_NUMBER,
    quantity(
        crate::QuantityKind::Angle,
        &[
            ("0 rad", "0 rad"),
            ("1.570796327 rad", "1.570796327 rad"),
            ("3.141592654 rad", "3.141592654 rad"),
        ],
    ),
    quantity(
        crate::QuantityKind::Area,
        &[
            ("0.5 sqm", "0.5 m²"),
            ("12 sqm", "12 m²"),
            ("10000 sqm", "10,000 m²"),
        ],
    ),
    identifiers::BIC,
    quantity(
        crate::QuantityKind::Bitrate,
        &[
            ("8000 bps", "8,000 bit/s"),
            ("1000000 bps", "1,000,000 bit/s"),
            ("100000000 bps", "100,000,000 bit/s"),
        ],
    ),
    // Day-and-month calendar dates, ordered by day.
    StandardType {
        name: "CalendarDate",
        holds: Holds::Builtin(Builtin::CalendarDate),
        text: Some(TextForm::Date),
        json: Json::Text,
        storage: Storage::Column(SqlType::Date),
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
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    },
    events::CALENDAR_EVENT_ENTRY,
    // An amount of calendar time, unordered: one month and 30 days don't
    // compare (area 19, "Time and calendar additions").
    time_object(
        CALENDAR_PERIOD,
        &CALENDAR_PERIOD_FIELDS,
        TextForm::Period,
        shown_as(FormatKey::PeriodUnits),
        false,
        Examples {
            valid: &[
                ("P1Y2M10D", "1 year, 2 months, 10 days"),
                ("P1M", "1 month"),
                ("P-1M-3D", "-1 month, -3 days"),
                ("P0D", "0 days"),
            ],
            format: None,
            malformed: &["1 month", "P", "PT1H", "P1D2M", "P1W", "P1M0D", "P+1M"],
            of: None,
            settings: "",
        },
    ),
    phones::CALLING_CODE,
    identifiers::CARD_BIN,
    web::CIDR,
    quantity(
        crate::QuantityKind::Concentration,
        &[
            ("0.005 gL", "0.005 g/L"),
            ("1 gL", "1 g/L"),
            ("35 gL", "35 g/L"),
        ],
    ),
    places::CONTINENT_ENTRY,
    web::COOKIE,
    places::COUNTRY_ENTRY,
    places::COUNTRY_SUBDIVISION_ENTRY,
    // The active ISO 4217 codes (area 19, "Money and payments", question
    // 22), written as their codes and shown by CLDR's display names.
    StandardType {
        name: crate::CURRENCY,
        holds: Holds::Enum(crate::CURRENCIES),
        text: Some(TextForm::Code),
        json: Json::Text,
        storage: Storage::Column(SqlType::Text),
        format: Format {
            key: FormatKey::CurrencyName,
            options: NO_OPTIONS,
        },
        ordered: false,
        validator: None,
        shape: None,
        examples: Examples {
            valid: &[
                ("GBP", "British Pound"),
                ("HRK", "Croatian Kuna"),
                ("JPY", "Japanese Yen"),
            ],
            format: None,
            malformed: &["pound", "XYZ", "XAU", "GB"],
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    },
    quantity(
        crate::QuantityKind::Current,
        &[("0.005 A", "0.005 A"), ("1 A", "1 A"), ("16 A", "16 A")],
    ),
    quantity(
        crate::QuantityKind::DataSize,
        &[
            ("0 B", "0 B"),
            ("1024 B", "1,024 B"),
            ("5000000 B", "5,000,000 B"),
        ],
    ),
    // The dates from `start` to `end`, both included (area 19, "Ranges").
    StandardType {
        name: DATE_RANGE,
        holds: Holds::Object(&DATE_RANGE_FIELDS),
        text: Some(TextForm::Interval),
        json: Json::Fields(&["start", "end"]),
        // Stored as its JSON, as `InstantRange` is.
        storage: Storage::Column(SqlType::Json),
        format: Format {
            key: FormatKey::Interval,
            options: &[FORMAT],
        },
        ordered: false,
        validator: None,
        shape: Some(&[
            FieldMatch {
                meaning: "a first day",
                names: &["start", "starts_on", "start_date"],
                built_on: &[Builtin::CalendarDate],
                or_enum: false,
            },
            FieldMatch {
                meaning: "a last day",
                names: &["end", "ends_on", "end_date"],
                built_on: &[Builtin::CalendarDate],
                or_enum: false,
            },
        ]),
        examples: Examples {
            valid: &[
                ("2026-10-01/2026-10-31", "1 Oct 2026 – 31 Oct 2026"),
                ("2026-10-25/2026-10-25", "25 Oct 2026 – 25 Oct 2026"),
            ],
            format: Some("date_medium"),
            malformed: &[
                "2026-10-31/2026-10-01",
                "2026-10-01",
                "2026-10-01/..",
                "2026-10-01T00:00:00Z/2026-10-02T00:00:00Z",
            ],
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    },
    web::DOMAIN_NAME,
    StandardType {
        name: "Duration",
        holds: Holds::Builtin(Builtin::Duration),
        text: Some(TextForm::Duration),
        json: Json::Text,
        // Milliseconds, as the runtime holds a duration.
        storage: Storage::Column(SqlType::Int8),
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
            of: None,
            settings: "",
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
            of: None,
            settings: "",
        },
    ),
    contact::EMAIL_WITH_NAME,
    web::EMOJI_ENTRY,
    quantity(
        crate::QuantityKind::Energy,
        &[
            ("0.5 J", "0.5 J"),
            ("4184 J", "4,184 J"),
            ("3600000 J", "3,600,000 J"),
        ],
    ),
    // A rate between two currencies at an instant (area 19, "Money and
    // payments"): what `money.convert` applies, never fetched.
    StandardType {
        name: EXCHANGE_RATE,
        holds: Holds::Object(&EXCHANGE_RATE_FIELDS),
        text: Some(TextForm::ExchangeRate),
        json: Json::Fields(&["from", "to", "rate", "at"]),
        // Stored as its JSON, as `InstantRange` is.
        storage: Storage::Column(SqlType::Json),
        format: canonical(),
        ordered: false,
        validator: None,
        shape: Some(&[
            FieldMatch {
                meaning: "a currency it converts from",
                names: &["from", "from_currency", "base", "base_currency"],
                built_on: &[Builtin::Text],
                or_enum: true,
            },
            FieldMatch {
                meaning: "a currency it converts to",
                names: &["to", "to_currency"],
                built_on: &[Builtin::Text],
                or_enum: true,
            },
            FieldMatch {
                meaning: "a rate",
                names: &["rate", "exchange_rate"],
                built_on: &[Builtin::Decimal],
                or_enum: false,
            },
            FieldMatch {
                meaning: "an instant it was quoted at",
                names: &["at", "as_of", "quoted_at"],
                built_on: &[Builtin::Instant],
                or_enum: false,
            },
        ]),
        examples: Examples {
            valid: &[
                (
                    "GBP/EUR 1.17 2026-10-18T09:00:00Z",
                    "GBP/EUR 1.17 2026-10-18T09:00:00Z",
                ),
                (
                    "EUR/JPY 162.4031 2026-10-18T09:30:00Z",
                    "EUR/JPY 162.4031 2026-10-18T09:30:00Z",
                ),
            ],
            format: None,
            malformed: &[
                "GBP/EUR 1.17",
                "GBP-EUR 1.17 2026-10-18T09:00:00Z",
                "GBP/XYZ 1.17 2026-10-18T09:00:00Z",
                "GBP/EUR 1,17 2026-10-18T09:00:00Z",
                "GBP/EUR 1.17 2026-10-18",
                "GBP/EUR  1.17 2026-10-18T09:00:00Z",
                "",
            ],
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    },
    contact::FAMILY_NAME,
    StandardType {
        name: FILE_TYPE,
        holds: Holds::Enum(FILE_TYPES),
        text: Some(TextForm::Code),
        json: Json::Text,
        storage: Storage::Column(SqlType::Text),
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
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    },
    // An exact rational number as written (chunk 5.5j2): `1/2` and `2/4`
    // are two values, as any object's fields are, and unordered. Sent as
    // its canonical text and stored as that JSON, as the time objects are.
    StandardType {
        name: FRACTION,
        holds: Holds::Object(&FRACTION_FIELDS),
        text: Some(TextForm::Fraction),
        json: Json::Text,
        storage: Storage::Column(SqlType::Json),
        format: canonical(),
        ordered: false,
        validator: None,
        shape: Some(&[
            FieldMatch {
                meaning: "a numerator",
                names: &["numerator"],
                built_on: &[Builtin::Int],
                or_enum: false,
            },
            FieldMatch {
                meaning: "a denominator",
                names: &["denominator"],
                built_on: &[Builtin::Int],
                or_enum: false,
            },
        ]),
        examples: Examples {
            valid: &[
                ("3/4", "3/4"),
                ("-1/2", "-1/2"),
                ("30000/1001", "30000/1001"),
            ],
            format: None,
            malformed: &["3/0", "3", "3 / 4", "0.5/1", "3/-4", ""],
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    },
    quantity(
        crate::QuantityKind::Frequency,
        &[
            ("50 Hz", "50 Hz"),
            ("440 Hz", "440 Hz"),
            ("2400000000 Hz", "2,400,000,000 Hz"),
        ],
    ),
    contact::GIVEN_NAME,
    web::HOSTNAME,
    web::HTTP_HEADER_NAME,
    web::HTTP_METHOD,
    web::HTTP_STATUS,
    identifiers::IBAN,
    quantity(
        crate::QuantityKind::Illuminance,
        &[
            ("0.5 lx", "0.5 lx"),
            ("500 lx", "500 lx"),
            ("100000 lx", "100,000 lx"),
        ],
    ),
    web::IMEI,
    StandardType {
        name: "Instant",
        holds: Holds::Builtin(Builtin::Instant),
        text: Some(TextForm::Instant),
        json: Json::Text,
        storage: Storage::Column(SqlType::Timestamp),
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
            of: None,
            settings: "",
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
        storage: Storage::Column(SqlType::Json),
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
                or_enum: false,
            },
            FieldMatch {
                meaning: "an end",
                names: &["end", "ends_at", "end_at"],
                built_on: &[Builtin::Instant],
                or_enum: false,
            },
        ]),
        examples: Examples {
            valid: &[(
                "2026-10-25T09:00:00Z/2026-10-25T17:00:00Z",
                "25 Oct 2026, 09:00:00 – 25 Oct 2026, 17:00:00",
            )],
            format: Some("date_time_medium"),
            malformed: &["2026-10-25T09:00:00Z", "2026-10-25T09:00:00Z/tomorrow"],
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    },
    identifiers::INVOICE_NUMBER,
    checked(
        "IpAddress",
        Builtin::IpAddress,
        Validator::IpAddress,
        Examples {
            valid: &[("192.0.2.1", "192.0.2.1"), ("2001:db8::1", "2001:db8::1")],
            format: None,
            malformed: &["192.0.2", "example.com"],
            of: None,
            settings: "",
        },
    ),
    identifiers::ISIN,
    // A week of an ISO week-numbering year, ordered by year and week.
    time_object(
        "IsoWeek",
        &[("year", YEARS), ("week", Part::Whole(1, 53))],
        TextForm::IsoWeek,
        // CLDR's week patterns aren't in ICU4X yet (chosen in chunk 5.5k).
        canonical(),
        true,
        Examples {
            valid: &[("2026-W01", "2026-W01"), ("2026-W53", "2026-W53")],
            format: None,
            malformed: &["2027-W53", "2026-W54", "2026-W00", "2026-W1", "2026W01"],
            of: None,
            settings: "",
        },
    ),
    web::JSON_POINTER,
    places::LANGUAGE_ENTRY,
    identifiers::LEI,
    quantity(
        crate::QuantityKind::Length,
        &[
            ("-1.5 metre", "-1.5 m"),
            ("0.0254 metre", "0.0254 m"),
            ("1000 metre", "1,000 m"),
        ],
    ),
    // A date and a clock time with no zone: not an instant until a zone
    // is chosen.
    time_object(
        "LocalDateTime",
        &[
            ("date", Part::Of(Builtin::CalendarDate)),
            ("time", Part::Standard("LocalTime")),
        ],
        TextForm::LocalDateTime,
        IN_A_FORMAT,
        true,
        Examples {
            valid: &[
                ("2026-03-29T01:30:00", "29 Mar 2026, 01:30:00"),
                ("2026-10-25T01:30:00", "25 Oct 2026, 01:30:00"),
            ],
            format: Some("date_time_medium"),
            malformed: &[
                "2026-10-25T01:30:00Z",
                "2026-10-25 01:30:00",
                "2026-10-25T24:00:00",
                "2026-10-25T01:30",
                "2026-02-30T01:30:00",
            ],
            of: None,
            settings: "",
        },
    ),
    // A clock time with no date or zone, to the second.
    time_object(
        "LocalTime",
        &LOCAL_TIME_FIELDS,
        TextForm::LocalTime,
        IN_A_FORMAT,
        true,
        Examples {
            valid: &[
                ("00:00:00", "00:00:00"),
                ("09:30:00", "09:30:00"),
                ("23:59:59", "23:59:59"),
            ],
            format: Some("time_medium"),
            malformed: &["24:00:00", "09:30", "9:30:00", "09:30:60", "09:30:00Z"],
            of: None,
            settings: "",
        },
    ),
    StandardType {
        name: LOCALE,
        holds: Holds::Enum(crate::zones::LOCALES),
        text: Some(TextForm::Code),
        json: Json::Text,
        storage: Storage::Column(SqlType::Text),
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
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    },
    web::MAC_ADDRESS,
    quantity(
        crate::QuantityKind::Mass,
        &[
            ("0.25 kg", "0.25 kg"),
            ("2.5 kg", "2.5 kg"),
            ("1000 kg", "1,000 kg"),
        ],
    ),
    // An amount in a currency's minor units (area 19, "Money and
    // payments", question 16): its JSON is the amount and the code, as
    // Stripe sends it, and it is stored in two columns, a `bigint` of minor
    // units and the code, also for a type that fixes the currency. Only
    // amounts in one currency are ordered, so its examples are all pounds.
    StandardType {
        name: "Money",
        holds: Holds::Builtin(Builtin::Money),
        text: Some(TextForm::Money),
        json: Json::Fields(&crate::MONEY_FIELDS),
        storage: Storage::Columns {
            parts: &[("amount", "bigint"), ("currency", "char(3)")],
            fixed: Some("currency"),
        },
        format: Format {
            key: FormatKey::Money,
            options: &[
                FormatOption {
                    name: "precision",
                    needed: false,
                },
                FormatOption {
                    name: "display",
                    needed: false,
                },
                FormatOption {
                    name: "sign",
                    needed: false,
                },
            ],
        },
        ordered: true,
        validator: None,
        shape: Some(&[
            FieldMatch {
                meaning: "an amount",
                names: &["amount", "value", "amount_minor", "minor", "cents", "pence"],
                built_on: &[Builtin::Int, Builtin::Decimal],
                or_enum: false,
            },
            FieldMatch {
                meaning: "a currency",
                names: &["currency", "currency_code"],
                built_on: &[Builtin::Text],
                or_enum: true,
            },
        ]),
        examples: Examples {
            valid: &[
                ("GBP -5.00", "-£5.00"),
                ("GBP 0.05", "£0.05"),
                ("GBP 10.50", "£10.50"),
                ("GBP 1234.50", "£1,234.50"),
            ],
            format: None,
            malformed: &[
                "10.50",
                "GBP 10.505",
                "GBP 10.5",
                "GBP10.50",
                "XYZ 10.50",
                "£10.50",
                "GBP 1,234.50",
                "",
            ],
            of: None,
            settings: "currency: Currency.gbp",
        },
        release: FIRST_RELEASE,
    },
    // A month of the year (question 22): its own name is its code, as a
    // `Weekday`'s is.
    StandardType {
        name: MONTH,
        holds: Holds::Enum(MONTHS),
        text: Some(TextForm::Code),
        json: Json::Text,
        storage: Storage::Column(SqlType::Text),
        format: shown_as(FormatKey::MonthName),
        ordered: false,
        validator: None,
        shape: None,
        examples: Examples {
            valid: &[("january", "January"), ("october", "October")],
            format: None,
            malformed: &["jan", "10", "smarch"],
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    },
    places::NATIONALITY_ENTRY,
    web::NETWORK_PORT,
    identifiers::NHS_NUMBER,
    schedules::OPENING_HOURS,
    identifiers::PAYMENT_CARD_NUMBER,
    identifiers::PAYMENT_REFERENCE,
    // A proportion where 0.20 is 20% (chunk 5.5j2): one `numeric` column
    // holding the fraction, sent as its canonical text, so a client never
    // reads 0.2 as 0.2%; ordered.
    StandardType {
        name: "Percentage",
        holds: Holds::Builtin(Builtin::Percentage),
        text: Some(TextForm::Percentage),
        json: Json::Text,
        storage: Storage::Column(SqlType::Numeric),
        format: shown_as(FormatKey::Percent),
        ordered: true,
        validator: None,
        shape: None,
        examples: Examples {
            valid: &[
                ("-5%", "-5%"),
                ("0%", "0%"),
                ("12.5%", "12.5%"),
                ("20%", "20%"),
                ("1500%", "1,500%"),
            ],
            format: None,
            malformed: &["20", "0.2", "%", "twenty%", "20 percent", ""],
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    },
    contact::PERSON_NAME,
    contact::PHONE_EXTENSION,
    phones::PHONE_NUMBER,
    contact::POSTAL_ADDRESS,
    postal::POSTAL_CODE,
    quantity(
        crate::QuantityKind::Power,
        &[
            ("0.001 W", "0.001 W"),
            ("60 W", "60 W"),
            ("3000 W", "3,000 W"),
        ],
    ),
    quantity(
        crate::QuantityKind::Pressure,
        &[
            ("0 Pa", "0 Pa"),
            ("1013.25 Pa", "1,013.25 Pa"),
            ("101325 Pa", "101,325 Pa"),
        ],
    ),
    web::PROBLEM_DETAILS_ENTRY,
    web::PUSH_TOKEN,
    // A quarter of a calendar year, ordered by year and quarter.
    time_object(
        "Quarter",
        &[("year", YEARS), ("quarter", Part::Whole(1, 4))],
        TextForm::Quarter,
        // CLDR's quarter patterns aren't in ICU4X yet (chosen in chunk
        // 5.5k).
        canonical(),
        true,
        Examples {
            valid: &[("2026-Q1", "2026-Q1"), ("2026-Q4", "2026-Q4")],
            format: None,
            malformed: &["2026-Q5", "2026-Q0", "2026Q1", "Q1 2026", "2026-q1"],
            of: None,
            settings: "",
        },
    ),
    // A range of any ordered type, either end open (area 19, "Ranges"),
    // holding its start and not its end, as `InstantRange` and
    // PostgreSQL's ranges do. Its examples are ranges of dates.
    StandardType {
        name: RANGE,
        holds: Holds::Range,
        text: Some(TextForm::Range),
        json: Json::Fields(&RANGE_FIELDS),
        // Stored as its JSON, as `InstantRange` is.
        storage: Storage::Column(SqlType::Json),
        // The options its item type takes (`zone:`, `format:`).
        format: Format {
            key: FormatKey::Range,
            options: NO_OPTIONS,
        },
        ordered: false,
        validator: None,
        shape: None,
        examples: Examples {
            valid: &[
                ("2026-10-01/2026-11-01", "1 Oct 2026 – 1 Nov 2026"),
                ("2026-10-01/..", "1 Oct 2026 – …"),
                ("../2026-11-01", "… – 1 Nov 2026"),
                ("../..", "… – …"),
            ],
            format: Some("date_medium"),
            malformed: &[
                "2026-10-01",
                "2026-11-01/2026-10-01",
                "2026-10-01/tomorrow",
                "/2026-11-01",
            ],
            of: Some("CalendarDate"),
            settings: "",
        },
        release: FIRST_RELEASE,
    },
    // One number to another as written (chunk 5.5j2): `16:9` and `32:18`
    // are two values, unordered. Sent as its canonical text and stored as
    // that JSON, as the time objects are.
    StandardType {
        name: RATIO,
        holds: Holds::Object(&RATIO_FIELDS),
        text: Some(TextForm::Ratio),
        json: Json::Text,
        storage: Storage::Column(SqlType::Json),
        format: canonical(),
        ordered: false,
        validator: None,
        shape: Some(&[
            FieldMatch {
                meaning: "a numerator",
                names: &["numerator"],
                built_on: &[Builtin::Decimal],
                or_enum: false,
            },
            FieldMatch {
                meaning: "a denominator",
                names: &["denominator"],
                built_on: &[Builtin::Decimal],
                or_enum: false,
            },
        ]),
        examples: Examples {
            valid: &[
                ("16:9", "16:9"),
                ("1.85:1", "1.85:1"),
                ("4.0:3.0", "4.0:3.0"),
            ],
            format: None,
            malformed: &["16/9", "16 : 9", "16:", ":9", "16:9:1", ""],
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    },
    schedules::RECURRENCE_RULE,
    web::RELATIVE_URL,
    web::SLUG,
    quantity(
        crate::QuantityKind::SoundLevel,
        &[
            ("-3.5 dB", "-3.5 dB"),
            ("60 dB", "60 dB"),
            ("120 dB", "120 dB"),
        ],
    ),
    quantity(
        crate::QuantityKind::Speed,
        &[
            ("0.5 mps", "0.5 m/s"),
            ("13.4112 mps", "13.4112 m/s"),
            ("340 mps", "340 m/s"),
        ],
    ),
    quantity(
        crate::QuantityKind::Temperature,
        &[
            ("0 kelvin", "0 K"),
            ("273.15 kelvin", "273.15 K"),
            ("373.15 kelvin", "373.15 K"),
        ],
    ),
    quantity(
        crate::QuantityKind::TemperatureDifference,
        &[
            ("-5 kelvin", "-5 K"),
            ("0 kelvin", "0 K"),
            ("5 kelvin", "5 K"),
        ],
    ),
    places::TEXT_DIRECTION_ENTRY,
    identifiers::UK_ACCOUNT_NUMBER,
    identifiers::UK_COMPANY_NUMBER,
    identifiers::UK_NATIONAL_INSURANCE_NUMBER,
    identifiers::UK_SORT_CODE,
    identifiers::UK_UTR,
    places::UN_LOCODE,
    contact::UPRN,
    checked(
        "Url",
        Builtin::Url,
        Validator::Url,
        Examples {
            valid: &[("https://example.com/orders", "https://example.com/orders")],
            format: None,
            malformed: &["example.com", "ftp://example.com"],
            of: None,
            settings: "",
        },
    ),
    web::URL_PATH,
    identifiers::US_EIN,
    identifiers::US_SSN,
    web::USER_AGENT,
    contact::USERNAME,
    // An offset from UTC, as a zone has at a moment; unordered.
    time_object(
        "UtcOffset",
        &[("minutes", Part::Whole(-1080, 1080))],
        TextForm::UtcOffset,
        shown_as(FormatKey::OffsetName),
        false,
        Examples {
            valid: &[
                ("+00:00", "GMT+00:00"),
                ("+05:30", "GMT+05:30"),
                ("-05:00", "GMT-05:00"),
            ],
            format: None,
            malformed: &["Z", "+0100", "+01", "05:00", "+18:01", "-00:00", "+01:60"],
            of: None,
            settings: "",
        },
    ),
    web::UTM_PARAMETERS_ENTRY,
    StandardType {
        name: "Uuid",
        holds: Holds::Builtin(Builtin::Uuid),
        text: Some(TextForm::Uuid),
        json: Json::Text,
        storage: Storage::Column(SqlType::Uuid),
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
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    },
    quantity(
        crate::QuantityKind::Voltage,
        &[
            ("1.5 V", "1.5 V"),
            ("230 V", "230 V"),
            ("11000 V", "11,000 V"),
        ],
    ),
    quantity(
        crate::QuantityKind::Volume,
        &[
            ("0.0005 cubic_metre", "0.0005 m³"),
            ("0.001 cubic_metre", "0.001 m³"),
            ("2 cubic_metre", "2 m³"),
        ],
    ),
    web::WEB_ORIGIN,
    StandardType {
        name: crate::Weekday::NAME,
        holds: Holds::Enum(WEEKDAYS),
        text: Some(TextForm::Code),
        json: Json::Text,
        storage: Storage::Column(SqlType::Text),
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
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    },
    places::WRITING_SCRIPT_ENTRY,
    // A calendar year on its own, ordered.
    time_object(
        "Year",
        &[("year", YEARS)],
        TextForm::Year,
        shown_as(FormatKey::Year),
        true,
        Examples {
            valid: &[("1999", "1999"), ("2026", "2026")],
            format: None,
            malformed: &["26", "10000", "-2026", "2026-10"],
            of: None,
            settings: "",
        },
    ),
    // A month of a year, ordered.
    time_object(
        "YearMonth",
        &[("year", YEARS), ("month", Part::Whole(1, 12))],
        TextForm::YearMonth,
        shown_as(FormatKey::YearMonth),
        true,
        Examples {
            valid: &[("2026-02", "February 2026"), ("2026-10", "October 2026")],
            format: None,
            malformed: &["2026-13", "2026-00", "2026-1", "202610", "2026-10-01"],
            of: None,
            settings: "",
        },
    ),
    StandardType {
        name: ZONE,
        holds: Holds::Enum(crate::zones::ZONES),
        text: Some(TextForm::Code),
        json: Json::Text,
        storage: Storage::Column(SqlType::Text),
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
            of: None,
            settings: "",
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
        storage: Storage::Column(SqlType::Json),
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
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    },
];

/// `InstantRange`: what `temporal`'s bounds give, a half-open range from
/// `start` up to but not including `end` (area 10, "Zones and local
/// times").
pub const INSTANT_RANGE: &str = "InstantRange";

/// `InstantRange`'s fields, in order.
pub const INSTANT_RANGE_FIELDS: [(&str, Part); 2] = [
    ("start", Part::Of(Builtin::Instant)),
    ("end", Part::Of(Builtin::Instant)),
];

/// `Month` (area 19, question 22).
pub const MONTH: &str = "Month";

/// The standard range types besides `Range<T>` (area 19, "Ranges"): each
/// with the built-in type its ends are, and whether it holds its end. An
/// `InstantRange` holds every instant up to its end and a `DateRange`
/// every date up to and including its end, each saying so once in its
/// type.
pub const RANGE_TYPES: &[(&str, Builtin, bool)] = &[
    (INSTANT_RANGE, Builtin::Instant, false),
    (DATE_RANGE, Builtin::CalendarDate, true),
];

/// The standard range type called `name`, with its ends' type and whether
/// it holds its end.
pub fn range_type(name: &str) -> Option<(Builtin, bool)> {
    RANGE_TYPES
        .iter()
        .find(|(range, _, _)| *range == name)
        .map(|(_, item, included)| (*item, *included))
}

/// `Range<T>` (area 19, "Ranges"): a range of any ordered type.
pub const RANGE: &str = "Range";

/// `Range<T>`'s fields, in order, each a `T?`.
pub const RANGE_FIELDS: [&str; 2] = ["start", "end"];

/// The built-in types a range's item may be built on: those `<` orders
/// (area 3).
pub const RANGE_ITEMS: &[Builtin] = &[
    Builtin::Int,
    Builtin::Decimal,
    Builtin::Instant,
    Builtin::CalendarDate,
    Builtin::Duration,
];

/// How a range's open end is written in its canonical text.
pub const OPEN_END: &str = "..";

/// The enums whose columns hold each variant's code rather than its name,
/// so the database reads as the wire does: `Currency` holds `GBP` (chosen
/// in chunk 5.5h), and every closed list holds its code, `Country` `GB`
/// and `IN` rather than `in_` (chunk 5.5l). Whether every standard enum
/// should is chunk 5.5w's.
pub fn stored_by_code(ty: &str) -> Option<&'static [(&'static str, &'static str)]> {
    closed_list(ty)
        .and(standard_type(ty))
        .and_then(StandardType::codes)
}

/// What variant `variant` of enum `ty` is stored as in its column: its
/// code for an enum [`stored_by_code`], else its name.
pub fn stored_variant<'v>(ty: &str, variant: &'v str) -> &'v str {
    stored_by_code(ty)
        .and_then(|codes| codes.iter().find(|(name, _)| *name == variant))
        .map_or(variant, |(_, code)| code)
}

/// The variant a column of enum `ty` holds as `stored`: by its code for an
/// enum [`stored_by_code`], else by its name.
pub fn variant_stored<'v>(ty: &str, stored: &'v str) -> &'v str {
    stored_by_code(ty)
        .and_then(|codes| codes.iter().find(|(_, code)| *code == stored.trim()))
        .map_or(stored, |(name, _)| name)
}

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

/// The rules a type held as text adds where a program builds one (area
/// 19, "Groups with the baseline only"): its own check, broken as
/// `format`. `None` for any other type.
pub fn text_rules(name: &str) -> Option<crate::Rules> {
    let entry = standard_type(name)?;
    matches!(entry.holds, Holds::Text { .. }).then(|| crate::Rules {
        standard: Some(entry.name),
        ..crate::Rules::default()
    })
}

/// What a standard type held as text or as a whole number is built on, and
/// the rules a program's type of it keeps (area 19, chunks 5.5i and 5.5n):
/// `Text` with its own check, or `Int` with its range, each naming the
/// type as their `standard`. `None` for any other type.
pub fn declared(name: &str) -> Option<(Builtin, crate::Rules)> {
    let entry = standard_type(name)?;
    let whole = |n: i64| crate::Num {
        mantissa: i128::from(n),
        scale: 0,
    };
    match entry.holds {
        Holds::Text { .. } => text_rules(name).map(|rules| (Builtin::Text, rules)),
        Holds::Whole { min, max } => Some((
            Builtin::Int,
            crate::Rules {
                min: Some(whole(min)),
                max: Some(whole(max)),
                standard: Some(entry.name),
                ..crate::Rules::default()
            },
        )),
        _ => None,
    }
}

/// Standard types held as text that are each a narrower form of another,
/// so a value of the first is a value of the second (chunk 5.5n): every
/// `UrlPath` is a `RelativeUrl`, which `web.resolve` takes, and every
/// `DomainName` a `Hostname`. A program's type table builds the first on
/// the second, as its stub writes it.
pub const WITHIN: &[(&str, &str)] = &[("DomainName", "Hostname"), ("UrlPath", "RelativeUrl")];

/// The standard type a type held as text is a narrower form of, if any.
pub fn within(name: &str) -> Option<&'static str> {
    WITHIN
        .iter()
        .find(|(narrow, _)| *narrow == name)
        .map(|(_, wide)| *wide)
}

/// A standard object's part as the wire names it (chunk 5.5n): its name,
/// less the trailing `_` a reserved word takes in code, as a closed list's
/// code does (question 22), so `ProblemDetails`' `type_` is sent as
/// RFC 9457's `type`. Any other type's field is sent as it is named.
pub fn wire_name<'a>(ty: &str, field: &'a str) -> &'a str {
    match standard_type(ty) {
        Some(entry) if matches!(entry.holds, Holds::Object(_)) => {
            field.strip_suffix('_').unwrap_or(field)
        }
        _ => field,
    }
}

/// Whether `text.parse` reads standard type `name` as people write it in
/// a country, given `country:` (chunk 5.5l2).
pub fn reads_by_country(name: &str) -> bool {
    matches!(name, "PhoneNumber" | "PostalCode")
}

/// A rule across two parts of a standard object (chunk 5.5l2): the text
/// in part `part` fits the code of the closed list in part `by`, as a
/// postal code fits its address's country. A literal that writes both is
/// proved while compiling and a value read from JSON is checked as it is
/// read, breaking `format` at `part`; one built in code from values waits
/// on cross-field rules (chunk 5.5t).
#[derive(Clone, Copy, Debug)]
pub struct PartRule {
    pub part: &'static str,
    pub by: &'static str,
    /// Whether the part's text fits the other part's code.
    pub fits: fn(&str, &str) -> bool,
    /// What a refusal says the part must be: "a postal code of the
    /// address's country".
    pub says: &'static str,
}

/// The rules across the parts of standard object type `ty`.
pub fn part_rules(ty: &str) -> &'static [PartRule] {
    const POSTAL_ADDRESS: &[PartRule] = &[PartRule {
        part: "postal_code",
        by: "country",
        fits: postal::fits,
        says: "a postal code of the address's country",
    }];
    match ty {
        "PostalAddress" => POSTAL_ADDRESS,
        _ => &[],
    }
}

impl StandardType {
    /// Whether `text` is this type's canonical text: its pattern matched
    /// whole, for a type held as text, and its named validator.
    pub fn accepts(&self, text: &str) -> bool {
        let pattern = match self.holds {
            Holds::Text { pattern, .. } => Some(pattern),
            _ => None,
        };
        pattern.is_none_or(|pattern| matches_whole(pattern, text))
            && self
                .validator
                .is_none_or(|validator| validator.accepts(text))
            && (pattern.is_some() || self.validator.is_some())
    }

    /// The enum's variants and their codes, for an enum.
    pub fn codes(&self) -> Option<&'static [(&'static str, &'static str)]> {
        match self.holds {
            Holds::Enum(codes) => Some(codes),
            _ => None,
        }
    }

    /// The codes of every variant in `form`, in the variants' order, for a
    /// closed list with data for it.
    pub fn form_codes(&self, form: crate::CodeForm) -> Option<&'static [&'static str]> {
        closed_list(self.name)?
            .forms
            .iter()
            .find(|(listed, _)| *listed == form)
            .map(|(_, codes)| *codes)
    }

    /// Whether its variant `variant` is retired: its code was withdrawn,
    /// and it is only read (question 22).
    pub fn is_retired(&self, variant: &str) -> bool {
        retired(self.name, variant).is_some()
    }

    /// The `PersonalData.` category its values are (area 15), for a type
    /// that is personal data whatever field holds it.
    pub fn personal(&self) -> Option<&'static str> {
        match self.holds {
            Holds::Text { personal, .. } => personal,
            _ => contact::PERSONAL
                .iter()
                .find(|(name, _)| *name == self.name)
                .map(|(_, category)| *category),
        }
    }

    /// The format option called `name`, if the type takes it.
    pub fn format_option(&self, name: &str) -> Option<&'static FormatOption> {
        self.format
            .options
            .iter()
            .find(|option| option.name == name)
    }

    /// The named columns of a type stored in more than one (`Money`'s);
    /// none for a type stored in one.
    pub fn columns(&self) -> &'static [(&'static str, &'static str)] {
        match self.storage {
            Storage::Column(_) => &[],
            Storage::Columns { parts, .. } => parts,
        }
    }

    /// Whether the part its named column `at` holds may be `none`: an
    /// object's part written `Part::Optional` (chunk 5.5l).
    pub fn part_may_be_none(&self, at: usize) -> bool {
        match self.holds {
            Holds::Object(fields) => fields.get(at).is_some_and(|(_, part)| part.required().1),
            _ => false,
        }
    }

    /// Of its named columns, the one a type built on it can fix.
    pub fn fixed_part(&self) -> Option<&'static str> {
        match self.storage {
            Storage::Column(_) => None,
            Storage::Columns { fixed, .. } => fixed,
        }
    }

    /// How its column becomes a value, for a type stored in a column of its
    /// own; `None` for one stored as its JSON or in more than one column.
    pub fn column(&self) -> Option<crate::read::Column> {
        use crate::read::Column;
        let Storage::Column(storage) = self.storage else {
            return None;
        };
        Some(match (storage, self.holds) {
            (SqlType::Text, Holds::Enum(codes)) => Column::Enum {
                ty: self.name.to_string(),
                variants: codes.iter().map(|(name, _)| (*name).to_string()).collect(),
            },
            (SqlType::Text, _) => Column::Text,
            (SqlType::Uuid, _) => Column::Uuid,
            (SqlType::Timestamp, _) => Column::Instant,
            (SqlType::Date, _) => Column::Date,
            (SqlType::Int8, Holds::Builtin(Builtin::Duration)) => Column::Duration,
            (SqlType::Int4, Holds::Whole { .. }) => Column::Int,
            (SqlType::Numeric, Holds::Builtin(Builtin::Quantity(kind))) => Column::Quantity(kind),
            (SqlType::Numeric, Holds::Builtin(Builtin::Percentage)) => Column::Percentage,
            _ => return None,
        })
    }

    /// The PostgreSQL column type its storage is: the first named column's,
    /// for a type stored in more than one.
    pub fn column_type(&self) -> &'static str {
        if let Holds::Builtin(Builtin::Quantity(kind)) = self.holds {
            return kind.column_type();
        }
        let storage = match self.storage {
            Storage::Column(storage) => storage,
            Storage::Columns { parts, .. } => {
                return parts.first().map_or("jsonb", |(_, ty)| *ty);
            }
        };
        match storage {
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

/// Whether the standard pattern `pattern` matches the whole of `text`,
/// each pattern compiled once.
fn matches_whole(pattern: &'static str, text: &str) -> bool {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static COMPILED: OnceLock<Mutex<HashMap<&'static str, Option<crate::Pattern>>>> =
        OnceLock::new();
    let Ok(mut compiled) = COMPILED.get_or_init(Mutex::default).lock() else {
        return false;
    };
    compiled
        .entry(pattern)
        .or_insert_with(|| crate::compile_pattern(pattern).ok())
        .as_ref()
        .is_some_and(|compiled| compiled.is_match(text))
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
        assert_eq!(names.len(), 108, "the 108 standard types built so far");
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
                assert!(
                    entry.validator.is_some() || matches!(entry.holds, Holds::Text { .. }),
                    "{} checks its text",
                    entry.name
                );
            }
            // Area 15's categories.
            let categories = [
                "identifying",
                "contact",
                "financial",
                "online",
                "demographic",
                "government_id",
                "location",
                "health",
                "biometric",
                "genetic",
                "beliefs",
                "ethnicity",
                "sex_life",
                "criminal",
            ];
            assert!(
                entry
                    .personal()
                    .is_none_or(|category| categories.contains(&category)),
                "{}",
                entry.name
            );
            if let Holds::Text { pattern, .. } = entry.holds {
                assert!(crate::compile_pattern(pattern).is_ok(), "{}", entry.name);
                assert_eq!(entry.text, Some(TextForm::Checked), "{}", entry.name);
                assert_eq!(entry.storage, Storage::Column(SqlType::Text));
                for (text, _) in entry.examples.valid {
                    assert!(entry.accepts(text), "{}: `{text}`", entry.name);
                }
                for text in entry.examples.malformed {
                    assert!(!entry.accepts(text), "{}: `{text}`", entry.name);
                }
            }
            if let Holds::Enum(codes) = entry.holds {
                assert_eq!(entry.text, Some(TextForm::Code), "{}", entry.name);
                assert!(!codes.is_empty());
            }
            if let (Holds::Object(fields), Some(names)) = (entry.holds, entry.json.fields()) {
                let declared: Vec<&str> = fields
                    .iter()
                    .map(|(name, _)| wire_name(entry.name, name))
                    .collect();
                assert_eq!(declared, names, "{}'s JSON is its fields", entry.name);
            }
            if let Holds::Whole { min, max } = entry.holds {
                assert_eq!(entry.text, Some(TextForm::Whole), "{}", entry.name);
                assert_eq!(entry.json, Json::Number, "{}", entry.name);
                assert!(min.abs() < 1 << 53 && max.abs() < 1 << 53, "{}", entry.name);
            }
            if let Some(wide) = within(entry.name) {
                let wide = standard_type(wide).expect("a standard type");
                for (text, _) in entry.examples.valid {
                    assert!(wide.accepts(text), "{}: `{text}` is a {}", entry.name, wide.name);
                }
            }
            // A range takes its item type's options.
            let formatted = match entry.examples.of {
                Some(item) => standard_type(item).expect("an item type"),
                None => entry,
            };
            assert_eq!(
                entry.examples.format.is_some(),
                formatted.format_option("format").is_some(),
                "{}'s examples name a format exactly when it takes one",
                entry.name
            );
            assert_eq!(
                entry.examples.of.is_some(),
                entry.holds == Holds::Range,
                "{}'s examples hold a type argument exactly when it takes one",
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
                    ["zone", "format", "precision", "display", "sign", "unit"]
                        .contains(&option.name),
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
        let iban = standard_type("Iban").expect("an IBAN");
        assert!(iban.accepts("GB29NWBK60161331926819"));
        assert!(!iban.accepts("GB29NWBK60161331926818"));
        assert_eq!(
            text_rules("Iban").and_then(|rules| rules.standard),
            Some("Iban")
        );
        assert_eq!(text_rules("Email"), None);
        let broken = text_rules("NhsNumber")
            .expect("rules")
            .check_text("9434765918")
            .expect_err("the check digit");
        assert_eq!(broken.key, crate::RuleKey::Format);
    }

    #[test]
    fn printed_forms_group_the_canonical_text() {
        let fours = Grouping {
            sizes: &[4],
            separator: " ",
        };
        assert_eq!(fours.apply("NO9386011117947"), "NO93 8601 1117 947");
        let nhs = Grouping {
            sizes: &[3, 3, 4],
            separator: " ",
        };
        assert_eq!(nhs.apply("9434765919"), "943 476 5919");
        assert_eq!(fours.apply(""), "");
    }
}
