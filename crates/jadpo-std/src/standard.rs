//! The standard types' data entries (area 19, "The standard types" and
//! "Every standard type meets one contract").
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

pub mod across;
pub mod addresses_data;
pub mod ai;
pub mod banks;
pub mod card_types;
pub mod certificates;
mod codes;
pub mod colour;
mod contact;
pub mod documents;
pub mod emoji_data;
pub mod events;
pub mod geo;
pub mod health;
mod identifiers;
pub mod jwk;
pub mod lists;
pub mod markup;
pub mod media;
pub mod modulus;
pub mod named;
pub mod opening_hours;
pub mod person_names_data;
pub mod phones;
mod phones_data;
pub mod places;
mod places_data;
pub mod postal;
mod postal_codes_data;
pub mod prices;
pub mod records;
pub mod recurrence;
pub mod rich_text;
mod schedules;
pub mod security;
pub mod signals;
pub mod software;
pub mod storage;
pub mod web;

pub use codes::isbn_from_isbn10;
pub use contact::{LINE_MAX, LINE_PATTERN};
pub use places::{ClosedList, closed_list, retired, subdivision_name};
pub use signals::{LOOP_SIGNALS, LoopShape, LoopSignal, NO_SIGNALS, Signals, Validation};

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
    /// A type built on the built-in type `of` with the bounds the standard
    /// library gives it, both included where given (chunk 5.5i3): a
    /// `TaxRate` is a `Percentage` never below 0%. A program holds it as a
    /// type built on `of` whose rules are those bounds ([`declared`]),
    /// as a declared refinement is, so its construction, text, JSON,
    /// storage and format are `of`'s, its bounds broken as `min` and `max`.
    Bounded {
        of: Builtin,
        min: Option<crate::Num>,
        max: Option<crate::Num>,
    },
    /// A choice of other types (chunk 5.5i3): an enum whose variants each
    /// carry fields, area 2's tagged union, each variant with its fields in
    /// declared order, each field a [`Part`]. A `Discount` is an amount off
    /// or a percentage off. A program builds and matches one as it does its
    /// own enum with data (`Discount.amount_off { amount: ... }`); it is
    /// sent in area 2's tagged form ([`Json::Tagged`]) and stored as that
    /// JSON.
    Choice(&'static [(&'static str, &'static [(&'static str, Part)])]),
    /// A list of the part (chunk 5.5o2): a `RichText` is its blocks in
    /// order. A program holds it as a named list type the standard library
    /// declares (area 2, "Named collection construction"), built from a
    /// list of its items, sent as a JSON array of them and stored as that
    /// JSON.
    List(Part),
    /// A map from the first part to the second (chunk 5.5o2): a
    /// `LocalisedText` maps locales to text. A program holds it as a named
    /// map type the standard library declares, built from a map of its
    /// entries, sent as a JSON object keyed by each key's wire text and
    /// stored as that JSON.
    Map(Part, Part),
    /// Bytes with a check of their own (chunk 5.5r3): a SHA-256 digest, a
    /// sealed value. A program holds it as a type built on `Bytes` that
    /// the standard library declares, whose own check
    /// ([`crate::Rules::standard`], the entry's validator over the bytes)
    /// a value of other bytes breaks as `format`, so `Hash(hash.sha256(x))`
    /// builds one and the runtime holds it as the bytes. Its canonical text
    /// and its JSON are area 2's Base64 of the bytes ([`TextForm::Base64`]),
    /// as every `Bytes` is sent, and it is stored as `bytea`.
    Bytes,
    /// `Feature<G, P>` (area 20, "Interop"; chunk 5.5u6): a GeoJSON
    /// `Feature`, its `geometry` of the geometry type `G` and its
    /// `properties` of the application's `Object` type `P`, the two type
    /// arguments ([`geo::FEATURE_FIELDS`]).
    Feature,
    /// `FeatureCollection<G, P>` (chunk 5.5u6): a GeoJSON
    /// `FeatureCollection`, its `features` a list of `Feature<G, P>`.
    FeatureCollection,
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
    /// Text matching this pattern whole: a masked card's last four digits
    /// (chunk 5.5i2).
    Matching(&'static str),
    /// Text for people that may run over several lines, with no control
    /// character but a tab or a line feed ([`events::PARAGRAPH_PATTERN`]):
    /// an event's description (chunk 5.5k3).
    Paragraph,
    /// A value built on the built-in type `of`, from `min` to `max`, both
    /// included where given (chunk 5.5i3): a price's amount never below 0,
    /// a discount's percentage from 0% to 100%. `min:` and `max:` read as
    /// they do on a type built on `of` (area 2): minor units for `Money`,
    /// the fraction for a `Percentage`, the base unit for a quantity, and
    /// milliseconds for a `Duration` (chunk 5.5m2).
    Bounded {
        of: Builtin,
        min: Option<crate::Num>,
        max: Option<crate::Num>,
    },
    /// A single-precision float, held as the decimal that is the nearest
    /// one ([`ai::float`]) and sent as a JSON number: a vector's item
    /// (chunk 5.5s2).
    Float,
    /// The part, or `none`: a person's family name.
    Optional(&'static Part),
    /// A list of the part: an address's lines.
    List(&'static Part),
    /// A set of the part, each item once: a key's `key_ops` (RFC 7517
    /// §4.3), stored as a list is.
    Set(&'static Part),
}

impl Part {
    /// The built-in type a field's values are built on; `None` for a
    /// field holding another standard type.
    pub const fn builtin(self) -> Option<Builtin> {
        match self {
            Part::Of(builtin) => Some(builtin),
            Part::Whole(..) => Some(Builtin::Int),
            Part::Float => Some(Builtin::Decimal),
            Part::Bounded { of, .. } => Some(of),
            Part::Line | Part::Matching(_) | Part::Paragraph => Some(Builtin::Text),
            Part::Standard(_) | Part::Optional(_) | Part::List(_) | Part::Set(_) => None,
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
            // A duration's bounds are in milliseconds (chunk 5.5m2).
            Part::Bounded {
                of: Builtin::Duration,
                min,
                max,
            } => crate::Rules {
                min_duration: min.map(|n| crate::Dur { millis: n.mantissa }),
                max_duration: max.map(|n| crate::Dur { millis: n.mantissa }),
                ..crate::Rules::default()
            },
            // Bytes are bounded by their length (chunk 5.5r3).
            Part::Bounded {
                of: Builtin::Bytes,
                min,
                max,
            } => crate::Rules {
                min_length: min.and_then(|n| u64::try_from(n.mantissa).ok()),
                max_length: max.and_then(|n| u64::try_from(n.mantissa).ok()),
                ..crate::Rules::default()
            },
            Part::Bounded { min, max, .. } => crate::Rules {
                min,
                max,
                ..crate::Rules::default()
            },
            Part::Line => crate::Rules {
                pattern: Some(LINE_PATTERN.to_string()),
                ..crate::Rules::default()
            },
            Part::Matching(pattern) => crate::Rules {
                pattern: Some(pattern.to_string()),
                ..crate::Rules::default()
            },
            Part::Paragraph => crate::Rules {
                pattern: Some(events::PARAGRAPH_PATTERN.to_string()),
                ..crate::Rules::default()
            },
            Part::Optional(inner) | Part::List(inner) | Part::Set(inner) => inner.rules(),
            Part::Of(_) | Part::Standard(_) | Part::Float => crate::Rules::default(),
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
    /// A secret value's text (chunk 5.5r2), read as [`TextForm::Checked`]
    /// reads text but never written: `text.from` and `text.format` don't
    /// take one, so `text.parse` is the one way its text is read.
    Secret,
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
    /// A card's expiry as ISO 8601's calendar month, `2028-12`, its year
    /// from 2000 to 2099 so the card's own `12/28` names one month (chosen
    /// in chunk 5.5i2).
    CardExpiry,
    /// A VAT number as VIES writes it: its country's code (`EL` for
    /// Greece), then the number with no spaces or punctuation,
    /// `GB980780684` (chosen in chunk 5.5i2).
    VatNumber,
    /// An RFC 5545 `VEVENT` (chosen in chunk 5.5k3, [`events`]): a
    /// property a line, joined by line feeds, an all-day event's `DTEND`
    /// the day after its last.
    Event,
    /// A whole number's digits, with no leading zero and `-` before a
    /// negative one, within its type's range: `404` (chunk 5.5n), `-25`
    /// (chunk 5.5i3).
    Whole,
    /// The query that carries a visit's UTM parameters (chunk 5.5n,
    /// [`web::utm_text`]): `utm_source=mailing&utm_medium=email&...`,
    /// each part it has in its parts' order, percent-encoded.
    UtmQuery,
    /// SemVer 2.0.0's text, `1.2.3-alpha.1+build.5`, with no `v` before it
    /// (chunk 5.5n2, [`software::read_version`]).
    SemanticVersion,
    /// CSS Color 4's hex colour, `#rrggbb`, or `#rrggbbaa` when it isn't
    /// opaque, written in lower case and read in either (chunk 5.5p,
    /// [`colour::read_hex`]).
    Colour,
    /// An object's parts, each a whole number with no sign or leading
    /// zero within its part's range, in order, joined by the separator:
    /// `1920x1080`, `16:9`, `30000/1001` (chunk 5.5p).
    Wholes(&'static str),
    /// An object's parts, each a decimal number with the places it was
    /// given and no leading zero, within its part's bounds, in order,
    /// joined by the separator: a `Rect`'s `0 0 300 200` (chunk 5.5p).
    Decimals(&'static str),
    /// An object of an enum and a text part: the enum's code, a space,
    /// and the text, the rules across the parts held: `BBFC 12A` (chunk
    /// 5.5p).
    CodeThenText,
    /// A WebVTT cue's timings line and its text (chunk 5.5p,
    /// [`media::read_cue`]).
    Cue,
    /// An object of two parts joined by the separator (chunk 5.5m2): the
    /// first an enum's code, a standard type's text or text its part's
    /// pattern takes, the second text its part's pattern takes; read by
    /// splitting at the separator's first place, the rules across the
    /// parts held: `UPSN 1Z999AA10123456784`, `openai/gpt-4o`.
    Joined(&'static str),
    /// A flight's designator as IATA's SSIM writes it (chunk 5.5m2,
    /// [`records`]): the airline's two characters, the number with no
    /// leading zero and the suffix, if it has one: `BA123`, `BA5A`.
    FlightNumber,
    /// A decimal number with the places it was given and no exponent, as
    /// area 2 writes a `Decimal`: a `Latitude`'s `51.5072` (chunk 5.5s).
    Decimal,
    /// Area 2's Base64 of a type built on `Bytes` (chunk 5.5r3): RFC
    /// 4648's standard alphabet, padded, in its one spelling, as `Bytes`
    /// is sent in JSON, its bytes passing the entry's check.
    Base64,
    /// An X.509 certificate's PEM (RFC 7468, chunk 5.5r5,
    /// [`certificates`]): one `CERTIFICATE` block, its DER in Base64 lines
    /// of 64 characters, each line ending in a line feed (or each in a
    /// carriage return and a line feed, when read), and nothing else.
    Pem,
    /// HTML as the sanitiser keeps it (chunk 5.5o, [`markup::sanitise`]):
    /// written as it is held, and read from any text, made safe rather than
    /// refused, so reading it never fails.
    Html,
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
    /// An object of these members, for a type built into the language
    /// (chunk 5.5w3): `Money`'s `amount` and `currency`, a `ZonedTime`'s
    /// `instant` and `zone`, a quantity's `value` and `unit`. Each member
    /// is a [`Member`], so the JSON reader and writer and OpenAPI's schema
    /// all follow the one list.
    Parts(&'static [(&'static str, Member)]),
    /// Area 2's form of an enum's variant with data: an object of `tag`,
    /// the variant's name, and the variant's fields, each in its own JSON
    /// shape, `{"tag": "percentage_off", "percentage": "20%"}` (a
    /// [`Holds::Choice`], chunk 5.5i3).
    Tagged,
    /// An object of these members, as RFC 9457 sends a problem (chunk
    /// 5.5n): like [`Json::Fields`], but a part that is `none` is left out
    /// rather than sent as `null`, and one left out reads as `none`.
    Members(&'static [&'static str]),
    /// Its value as a JSON number: a whole number held by
    /// [`Holds::Whole`], whose range is within JavaScript's safe integers.
    Number,
    /// A JSON array of its items, each in its own shape: a list held by
    /// [`Holds::List`], as Portable Text sends a document's blocks (chunk
    /// 5.5o2).
    Array,
    /// A JSON object of its entries, each key its wire text and each value
    /// in its own shape: a map held by [`Holds::Map`], as JSON-LD sends a
    /// language map, `{"en-GB": "Rye loaf", "fr-FR": "Pain de seigle"}`
    /// (chunk 5.5o2).
    Keyed,
    /// A secret value's text as a JSON string, read as [`Json::Text`] is
    /// but never written (chunk 5.5r2): the checker keeps one out of every
    /// `Output` and `json.write`, and a value that reached a writer anyway
    /// is written redacted, never as its text.
    Secret,
    /// A GeoJSON geometry object (RFC 7946, chunk 5.5s), `{"type":
    /// "Point", "coordinates": [-0.1276, 51.5072]}`: the one place a
    /// position's `[longitude, latitude]` order exists, read and written by
    /// `jadpo-value`'s `geo` module, which checks each coordinate's range
    /// and each ring's closure and writes rings by the right-hand rule.
    GeoJson,
    /// A GeoJSON `Feature` or `FeatureCollection` object (RFC 7946 §3.2
    /// and §3.3, chunk 5.5u6), `{"type": "Feature", "geometry": ...,
    /// "properties": {...}}`: its geometry as [`Json::GeoJson`] and its
    /// properties as their own type's JSON, read and written through the
    /// codecs of its type arguments.
    Feature,
}

impl Json {
    /// The object's names on the wire, for a type sent as an object.
    pub const fn fields(self) -> Option<&'static [&'static str]> {
        match self {
            Json::Fields(names) | Json::Members(names) => Some(names),
            Json::Parts(_)
            | Json::Text
            | Json::Secret
            | Json::Number
            | Json::Tagged
            | Json::Array
            | Json::Keyed
            | Json::GeoJson
            | Json::Feature => None,
        }
    }
}

/// One member of a type built into the language that the contract sends
/// as an object ([`Json::Parts`], chunk 5.5w3): what the member holds and
/// how it is written, which the JSON reader, the writer and OpenAPI read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Member {
    /// A whole number of a currency's minor units as a JSON number, within
    /// JavaScript's safe integers either side
    /// ([`crate::http::SAFE_INTEGER`]), past which it breaks `max`:
    /// `Money`'s `amount`, as Stripe sends it.
    MinorUnits,
    /// The code of the standard enum this names, read ignoring case as the
    /// enum's own code is (question 22): `Money`'s `currency`, a
    /// `ZonedTime`'s `zone`. A type built on it may fix it to one code, by
    /// the setting its entry's storage names for a column of the member's
    /// name ([`Storage::Columns`]'s `fixed`).
    Code(&'static str),
    /// An instant's canonical text, ending in `Z`: a `ZonedTime`'s
    /// `instant`.
    Instant,
    /// A decimal written as text, as area 2 sends every `Decimal`: a
    /// quantity's `value`, in its kind's base unit when written.
    Number,
    /// The code of one of the quantity's kind's units, exactly as written,
    /// another kind's breaking `unknown_unit`: a quantity's `unit`, its
    /// kind's base unit when written.
    Unit,
}

/// A quantity's members on the wire ([`QUANTITY_FIELDS`]).
pub const QUANTITY_PARTS: [(&str, Member); 2] = [
    (QUANTITY_FIELDS[0], Member::Number),
    (QUANTITY_FIELDS[1], Member::Unit),
];

/// The locale data a standard type is formatted for people with: what
/// `text.format(value, locale:)` reads (the contract's "format for
/// people").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatKey {
    /// Its canonical text: the locale doesn't change how it is shown.
    Canonical,
    /// Never shown (chunk 5.5r2): a secret value, which `text.format`
    /// doesn't take, and which a report shows as `<redacted Password>`.
    Redacted,
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
    /// Each variant's own name, the same in every locale, in the variants'
    /// order: a card brand's, "American Express" (chunk 5.5i2).
    Names(&'static [&'static str]),
    /// A masked card as a receipt prints it: its brand's name, if known,
    /// and its last four after `•••• `, "Visa •••• 4242" (chunk 5.5i2).
    MaskedCard,
    /// A card's expiry as the card prints it, `12/28` (chunk 5.5i2).
    CardExpiry,
    /// The parts named, in order, each shown as its own type is with no
    /// options, joined by `separator`: a price's amount, `£10.50`; an
    /// interest rate's rate and basis, `5.5% APR`. A choice's variant shows
    /// the parts it has (chunk 5.5i3).
    Parts {
        parts: &'static [&'static str],
        separator: &'static str,
    },
    /// A number in the locale's digits and separators, then `suffix`,
    /// which no locale changes: basis points, `25 bp` (chunk 5.5i3).
    Suffixed(&'static str),
    /// The text in the first of `parts` the value has, an object's part
    /// or a choice's variant's, shown as its last four after `mask`: a tax
    /// or identity document's number, `•••• 6789` (chunk 5.5i2); a bank
    /// account's number or IBAN (chunk 5.5i4).
    PartLastFour {
        parts: &'static [&'static str],
        mask: &'static str,
    },
    /// SMPTE ST 12-1's notation, `01:00:00:00`, with `;` before the frames
    /// when it counts drop-frame, the same in every locale (chunk 5.5p).
    Timecode,
    /// The text people read in it, each block on its own line, as
    /// `markup.to_plain_text` gives it: an `Html` value's, or a `Markdown`
    /// text's once rendered (chunk 5.5o); a `RichText`'s, one of its
    /// blocks' or one of its spans' once written as HTML (chunk 5.5o2).
    /// The locale doesn't change it.
    PlainText,
    /// The entry `text.pick` gives for the locale it is shown in: a
    /// `LocalisedText`'s (chunk 5.5o2).
    Picked,
    /// A quantity in the first unit listed, at its places where it names
    /// them, or in the unit `unit:` names, at that unit's places where it is
    /// listed and exactly otherwise (chunk 5.5m2): a heart rate in whole
    /// beats a minute, `72 bpm`.
    UnitsIn(&'static [ShownIn]),
    /// The parts named that the value has, each a quantity shown in the
    /// listed unit of its kind (or the unit `unit:` names, for its kind),
    /// joined by `separator`; with `unit_once`, their numbers joined and
    /// the unit written once after them (chunk 5.5m2): `120/80 mmHg`, `30 ×
    /// 20 × 10 cm`, a dose's `500 mg`.
    Measures {
        parts: &'static [&'static str],
        separator: &'static str,
        units: &'static [ShownIn],
        unit_once: bool,
    },
    /// A dosage instruction in English, as its route's name is (chunk
    /// 5.5m2): its dose, its route, how many times in what period, and
    /// `when needed` when it is taken only then: `500 mg, Oral use, 2 times
    /// per 1 day`.
    Dosage,
    /// The first of the parts named that the value has, text as it is
    /// written and an instant as its canonical text, the same in every
    /// locale (chunk 5.5r): a token's claims by their subject, else their
    /// issuer; a key by its id, else the key itself. The last part named is
    /// one the value always has, so there is always something to show.
    FirstPart(&'static [&'static str]),
    /// A latitude or a longitude in the locale's digits, a degree sign and
    /// its hemisphere's letter, `positive` above 0 and `negative` below,
    /// the same letters in every locale; 0 has none: `51.5072° N`, `0°`
    /// (chunk 5.5s).
    Coordinate {
        positive: &'static str,
        negative: &'static str,
    },
    /// A geospatial record as `jadpo-value`'s `geo` module shows it (chunk
    /// 5.5s): a point as its latitude and longitude with their
    /// hemispheres, and its altitude; a line or a ring as its points in
    /// brackets, joined by `; `; a geometry made of others as theirs in
    /// brackets, joined by `, `; a bounding box from its south-west corner
    /// to its north-east one; projected coordinates by name; a tile as its
    /// URL writes it.
    Geometry,
    /// A vector's numbers in the locale's digits, in brackets, joined by
    /// `; ` as a line's points are; an embedding as its model, then its
    /// vector (chunk 5.5s2): `[0.5; -0.25; 1]`.
    Vector,
    /// Each part the value has, `none` left out, as its label, a space and
    /// the part shown as its own type is, a number in the locale's digits,
    /// joined by `, `, the labels the same in every locale (chunk 5.5s2):
    /// a token usage's `input 1,200, output 350`. Each pair is a label and
    /// the part it names.
    ByLabel(&'static [(&'static str, &'static str)]),
}

/// A unit a measure is shown in (chunk 5.5m2): its kind, its name in the
/// kind's unit enum, and the places it is shown to, rounded half to even,
/// where it is rounded at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShownIn {
    pub kind: crate::QuantityKind,
    pub unit: &'static str,
    pub places: Option<u32>,
}

impl ShownIn {
    /// The unit itself.
    pub fn unit(&self) -> Option<&'static crate::QuantityUnit> {
        self.kind.unit_named(self.unit)
    }
}

/// Whether `text.parse` reads standard type `name` in code system `form`,
/// a form it reads but never writes (chunk 5.5m2): an `Isbn` from an
/// ISBN-10, which `text.from` never gives, as the contract's conversion
/// "`Isbn` 10 to 13" says.
pub fn reads_form(name: &str, form: crate::CodeForm) -> bool {
    name == "Isbn" && form == crate::CodeForm::Isbn10
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
/// can't say (area 19, "The contract, generated"), one function each.
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
    /// A BIC whose country is a current one (ISO 9362, ISO 3166-1).
    Bic,
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
    /// A container image reference, fully qualified (OCI distribution,
    /// [`software`]).
    ContainerImage,
    /// A digest by the OCI image specification's grammar ([`software`]).
    Digest,
    /// A portable file name ([`software`]).
    FileName,
    /// A portable path written with `/` ([`software`]).
    FilePath,
    /// A ref name `git check-ref-format` accepts ([`software`]).
    GitRef,
    /// A package URL in ECMA-427's canonical form ([`software`]).
    PackageUrl,
    /// A pattern area 2's linear-time engine compiles ([`software`]).
    RegularExpression,
    /// An SPDX licence expression on the bundled list ([`software`]).
    SpdxLicence,
    /// A W3C Trace Context id: lower-case hexadecimal, not all zero
    /// ([`software`]).
    TraceHex,
    /// A `traceparent` of version `00` ([`software`]).
    TraceParent,
    /// A node-semver range in one spelling ([`software`]).
    VersionRange,
    /// GS1's mod-10 check digit: a GTIN, an ISBN-13, an SSCC.
    Gs1,
    /// A VIN's check digit, where 49 CFR 565 requires one.
    Vin,
    /// An ISO 6346 container number's check digit.
    ContainerNumber,
    /// A SNOMED CT concept's partition and Verhoeff check digit.
    SnomedConcept,
    /// An ORCID iD's ISO 7064 MOD 11-2 check character.
    Orcid,
    /// An MPAN core's check digit.
    Mpan,
    /// An H3 cell's index, its bits valid ([`geo::is_h3_cell`]).
    H3Cell,
    /// An Argon2id PHC string with bounded costs ([`security`]).
    PasswordHash,
    /// An Ed25519 public key as unpadded Base64url ([`security`]).
    PublicKey,
    /// A JWS in compact form, its parts unpadded Base64url ([`security`]).
    Jwt,
    /// A password as a person types it ([`security`]).
    Password,
    /// An API key ([`security`]).
    ApiKey,
    /// An RFC 6750 bearer token ([`security`]).
    BearerToken,
    /// A TOTP shared secret as RFC 4648 Base32 ([`security`]).
    TotpSecret,
    /// Any secret text ([`security`]).
    SecretText,
    /// Six to eight digits ([`security`], chunk 5.5r3).
    OneTimeCode,
    /// A SHA-256 digest's 32 bytes ([`security`], chunk 5.5r3).
    Hash,
    /// An HMAC-SHA-256 tag's 32 bytes ([`security`], chunk 5.5r3).
    Hmac,
    /// The sealed format's version and algorithm, and room for its key
    /// identity, nonce and tag ([`security`], chunk 5.5r3).
    EncryptedValue,
    /// HTML already as the sanitiser keeps it ([`markup::is_safe`]): what
    /// an `Html` literal must be, since its construction from any other
    /// text makes it safe first (chunk 5.5o).
    Html,
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
            Validator::Bic => identifiers::is_bic(text),
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
            Validator::ContainerImage => software::is_container_image(text),
            Validator::Digest => software::is_digest(text),
            Validator::FileName => software::is_file_name(text),
            Validator::FilePath => software::is_file_path(text),
            Validator::GitRef => software::is_git_ref(text),
            Validator::PackageUrl => software::is_package_url(text),
            Validator::RegularExpression => software::is_regular_expression(text),
            Validator::SpdxLicence => software::is_spdx_licence(text),
            Validator::TraceHex => software::is_trace_hex(text),
            Validator::TraceParent => software::is_trace_parent(text),
            Validator::VersionRange => software::is_version_range(text),
            Validator::Gs1 => codes::passes_gs1(text),
            Validator::Vin => codes::is_vin(text),
            Validator::ContainerNumber => codes::is_container_number(text),
            Validator::SnomedConcept => codes::is_snomed_concept(text),
            Validator::Orcid => codes::is_orcid(text),
            Validator::Mpan => codes::is_mpan(text),
            Validator::H3Cell => geo::is_h3_cell(text),
            Validator::Html => markup::is_safe(text),
            Validator::PasswordHash => security::is_password_hash(text),
            Validator::PublicKey => security::is_public_key(text),
            Validator::Jwt => security::is_jwt(text),
            Validator::Password => security::is_password(text),
            Validator::ApiKey => security::is_api_key(text),
            Validator::BearerToken => security::is_bearer_token(text),
            Validator::TotpSecret => security::is_totp_secret(text),
            Validator::SecretText => security::is_secret_text(text),
            Validator::OneTimeCode => security::is_one_time_code(text),
            // A type built on `Bytes`: its canonical text, its Base64,
            // holding bytes that pass (chunk 5.5r3).
            Validator::Hash | Validator::Hmac | Validator::EncryptedValue => {
                security::decode_base64(text).is_some_and(|bytes| self.accepts_bytes(&bytes))
            }
        }
    }

    /// Whether `bytes` pass, for a type built on `Bytes` (chunk 5.5r3);
    /// `false` for a validator of text.
    pub fn accepts_bytes(self, bytes: &[u8]) -> bool {
        match self {
            Validator::Hash => security::is_hash(bytes),
            Validator::Hmac => security::is_hmac(bytes),
            Validator::EncryptedValue => security::is_sealed(bytes),
            _ => false,
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
    /// Whether an `Object` may leave the field out and still match, as a
    /// colour's alpha (chunk 5.5p).
    pub optional: bool,
    /// A field that is a list instead, each item this standard type or an
    /// `Object` of its shape: `LineString`'s and `Polygon`'s list of
    /// points (area 19, "Packages use the standard types"; chunk 5.5u6).
    /// `built_on` is then empty.
    pub list_of: Option<&'static str>,
}

/// How a standard type is stored (area 5, "Storage types"; area 19).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Storage {
    /// One column of this type, named for the field.
    Column(SqlType),
    /// Named columns, each its suffix, joined to the field's column name
    /// with `_`, and its PostgreSQL type, in order, always all of them
    /// (`Money`: `amount bigint` and `currency char(3)`); a suffix of `""`
    /// is the field's own name, for a type stored natively in one column
    /// (a range, a `LocalTime`, chunk 5.5w). What each column holds follows
    /// from the type's parts ([`storage::layout`]). `fixed` names those a
    /// type built on it can fix to one value, each with the setting that
    /// fixes it, which a check constraint then holds it to (`Money`'s
    /// `currency`, which a type's `currency:` fixes; a rate's `from` and
    /// `to`). One codec turns a value into its columns and back
    /// (`jadpo-value`'s `standard::to_parts` and `from_parts`).
    Columns {
        parts: &'static [(&'static str, &'static str)],
        fixed: &'static [(&'static str, Fixed)],
    },
    /// A PostGIS `geography` column in WGS84 (area 20, "Storage"; chunk
    /// 5.5u4), checked to hold the type's own geometry: one of the six
    /// geometries, or any of them for a `Geometry`.
    Geography,
    /// A pgvector `vector` column (area 19, "AI types"), which
    /// the AI area builds (roadmap 5.5v): until then a field can't store a
    /// vector or an embedding, `toolchain.not_built_yet` (chunk 5.5s2).
    Vector,
    /// Never stored as itself (chunk 5.5r2): a secret value, whose field
    /// in an entity the checker refuses, or reports as not built yet where
    /// it will be stored encrypted.
    Never,
}

/// The setting of a type built on a standard type that fixes one of its
/// named columns to one currency (area 19, chunks 5.5h and 5.5w).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fixed {
    /// A `Money` type's `currency:`.
    Currency,
    /// A rate type's `from:`.
    RateFrom,
    /// A rate type's `to:`.
    RateTo,
}

impl Fixed {
    /// The currency `rules` fix the column to, by its place among
    /// `Currency`'s variants.
    pub fn of(self, rules: &crate::Rules) -> Option<u32> {
        match self {
            Fixed::Currency => rules.currency,
            Fixed::RateFrom => rules.rate_from,
            Fixed::RateTo => rules.rate_to,
        }
    }
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
    /// The Jadpo release that added it (area 19, "A release adding a standard type"): an
    /// application's own declaration of its name then clashes,
    /// `type.standard_clash` says since when, and the release's notes list
    /// it under "Names each release adds".
    pub release: &'static str,
    /// What `jadpo suggest` finds hand-rolled uses of it by (area 14,
    /// "Suggestions"), beside [`StandardType::shape`].
    pub signals: Signals,
}

/// The release the standard types built before Jadpo's first release
/// arrive in: the workspace's version until then.
pub const FIRST_RELEASE: &str = "0.1.0";

/// The encoding this release writes every value that outlives it in
/// (area 19, "Changing a standard type's encoding", chunk 5.9): each
/// standard type's canonical text, contract JSON and storage, as its golden
/// file pins them, and the runtime's own records holding them. A row or a
/// token kept beyond a request records it (a signed part's `enc`, the
/// outbox's and a passkey's `encoding` column), so a release that changes a
/// form raises it and reads what an earlier one wrote by its encoding.
/// Encoding 2 (chunk A3): `Jwk` holds `key_ops`, a member and a column
/// encoding 1 didn't have, so every encoding-1 form reads as it did.
pub const ENCODING: u32 = 2;

/// Whether this release reads what encoding `kept` wrote: its own, or an
/// earlier one, whose forms it still reads; never a later release's.
pub const fn reads_encoding(kept: u64) -> bool {
    kept >= 1 && kept <= ENCODING as u64
}

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
    // A JSON Lines file, as `json_lines.write` writes one (chunk 6.4b2):
    // the media type jsonlines.org names.
    ("json_lines", "application/jsonl"),
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
        signals: NO_SIGNALS,
    }
}

/// A time type held as an object (area 19, "Time and calendar
/// additions", chunk 5.5k): its fields for code to read, its canonical
/// text as its JSON, as a `Duration`'s is, and stored in `columns` (chunk
/// 5.5w): its parts, or one native column.
const fn time_object(
    name: &'static str,
    fields: &'static [(&'static str, Part)],
    columns: &'static [(&'static str, &'static str)],
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
        storage: Storage::Columns {
            parts: columns,
            fixed: &[],
        },
        format,
        ordered,
        validator: None,
        shape: None,
        examples,
        release: FIRST_RELEASE,
        signals: NO_SIGNALS,
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
        json: Json::Parts(&QUANTITY_PARTS),
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
        signals: signals::QUANTITY,
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
/// `places`), `CalendarEvent` (chunk 5.5k3, in `events`), the 20 web
/// and network types (chunk 5.5n, in `web`), the money group's records
/// (chunk 5.5i3, in `prices`), the 18 software types (chunk 5.5n2, in
/// `software`), `Html` and `Markdown` (chunk 5.5o, in `markup`), the 15
/// media and visual types with `ContentRatingScheme` (chunk 5.5p, in
/// `colour` and `media`), `RichText` with its blocks, spans and marks,
/// and `LocalisedText` (chunk 5.5o2, in `rich_text`), the 16 records
/// and measures of commerce, health, travel and the rest with the size
/// systems and `RouteOfAdministration` (chunk 5.5m2, in `records` and
/// `health`), `PasswordHash`, `PasswordCheck` and `PublicKey` (chunk
/// 5.5q2, in `security`), the 17 geospatial types (chunk 5.5s, in
/// `geo`), `Jwt`, `JwtClaims`, `Jwk`, `OAuthScope` and `OidcClaims`
/// (chunk 5.5r, in `security`), the secret values `Password`,
/// `ApiKey`, `BearerToken`, `TotpSecret` and `SecretText` (chunk 5.5r2,
/// in `security`), and the AI types with `ChatRole` (chunk 5.5s2, in
/// `ai`).
pub const STANDARD_TYPES: &[StandardType] = &[
    identifiers::ABA_ROUTING_NUMBER,
    lists::ALLERGEN_ENTRY,
    media::ALT_TEXT_ENTRY,
    quantity(
        crate::QuantityKind::Angle,
        &[
            ("0 rad", "0 rad"),
            ("1.570796327 rad", "1.570796327 rad"),
            ("3.141592654 rad", "3.141592654 rad"),
        ],
    ),
    security::API_KEY_ENTRY,
    quantity(
        crate::QuantityKind::Area,
        &[
            ("0.5 sqm", "0.5 m²"),
            ("12 sqm", "12 m²"),
            ("10000 sqm", "10,000 m²"),
        ],
    ),
    media::ASPECT_RATIO_ENTRY,
    media::AUDIO_CODEC_ENTRY,
    banks::BANK_ACCOUNT_ENTRY,
    records::BARCODE_ENTRY,
    prices::BASIS_POINTS_ENTRY,
    security::BEARER_TOKEN_ENTRY,
    geo::BEARING_ENTRY,
    identifiers::BIC,
    quantity(
        crate::QuantityKind::Bitrate,
        &[
            ("8000 bps", "8,000 bit/s"),
            ("1000000 bps", "1,000,000 bit/s"),
            ("100000000 bps", "100,000,000 bit/s"),
        ],
    ),
    health::BLOOD_GLUCOSE_ENTRY,
    health::BLOOD_PRESSURE_ENTRY,
    lists::BLOOD_TYPE_ENTRY,
    codes::BOOKING_REFERENCE,
    geo::BOUNDING_BOX_ENTRY,
    lists::CABIN_CLASS_ENTRY,
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
        signals: NO_SIGNALS,
    },
    events::CALENDAR_EVENT_ENTRY,
    // An amount of calendar time, unordered: one month and 30 days don't
    // compare (area 19, "Time and calendar additions").
    time_object(
        CALENDAR_PERIOD,
        &CALENDAR_PERIOD_FIELDS,
        &[
            ("years", "integer"),
            ("months", "integer"),
            ("days", "integer"),
        ],
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
    card_types::CARD_BRAND_TYPE,
    card_types::CARD_EXPIRY_TYPE,
    codes::CARRIER,
    lists::CEFR_LEVEL_ENTRY,
    certificates::CERTIFICATE_ENTRY,
    ai::CHAT_MESSAGE_ENTRY,
    ai::CHAT_ROLE_ENTRY,
    web::CIDR,
    geo::CIRCLE_ENTRY,
    records::CLOTHING_SIZE_ENTRY,
    records::CLOTHING_SIZE_SYSTEM_ENTRY,
    colour::COLOUR_ENTRY,
    quantity(
        crate::QuantityKind::Concentration,
        &[
            ("0.005 gL", "0.005 g/L"),
            ("1 gL", "1 g/L"),
            ("35 gL", "35 g/L"),
        ],
    ),
    software::CONTAINER_IMAGE,
    codes::CONTAINER_NUMBER,
    media::CONTENT_RATING_ENTRY,
    media::CONTENT_RATING_SCHEME_ENTRY,
    places::CONTINENT_ENTRY,
    web::COOKIE,
    places::COUNTRY_ENTRY,
    places::COUNTRY_SUBDIVISION_ENTRY,
    geo::CRS_ENTRY,
    // The active ISO 4217 codes (area 19, "Money and payments"), written as their codes and shown by CLDR's display names.
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
        signals: NO_SIGNALS,
    },
    quantity(
        crate::QuantityKind::Current,
        &[("0.005 A", "0.005 A"), ("1 A", "1 A"), ("16 A", "16 A")],
    ),
    software::CVE,
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
        // Stored in one `daterange` column (area 19, "Every standard type
        // meets one contract").
        storage: Storage::Columns {
            parts: &[("", "daterange")],
            fixed: &[],
        },
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
                optional: false,
                list_of: None,
            },
            FieldMatch {
                meaning: "a last day",
                names: &["end", "ends_on", "end_date"],
                built_on: &[Builtin::CalendarDate],
                or_enum: false,
                optional: false,
                list_of: None,
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
        signals: NO_SIGNALS,
    },
    software::DIGEST,
    records::DIMENSIONS_ENTRY,
    prices::DISCOUNT_ENTRY,
    software::DOI,
    web::DOMAIN_NAME,
    health::DOSAGE_INSTRUCTION_ENTRY,
    health::DOSE_ENTRY,
    documents::DRIVING_LICENCE_NUMBER_TYPE,
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
        signals: signals::DURATION,
    },
    codes::EAN13,
    geo::EASTING_NORTHING_ENTRY,
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
    )
    .with_signals(signals::EMAIL),
    contact::EMAIL_WITH_NAME,
    ai::EMBEDDING_ENTRY,
    web::EMOJI_ENTRY,
    security::ENCRYPTED_VALUE_ENTRY,
    quantity(
        crate::QuantityKind::Energy,
        &[
            ("0.5 J", "0.5 J"),
            ("4184 J", "4,184 J"),
            ("3600000 J", "3,600,000 J"),
        ],
    ),
    lists::EPC_RATING_ENTRY,
    // A rate between two currencies at an instant (area 19, "Money and
    // payments"): what `money.convert` applies, never fetched.
    StandardType {
        name: EXCHANGE_RATE,
        holds: Holds::Object(&EXCHANGE_RATE_FIELDS),
        text: Some(TextForm::ExchangeRate),
        json: Json::Fields(&["from", "to", "rate", "at"]),
        // Stored in a column per field, its currencies held to its type's
        // `from:` and `to:`.
        storage: Storage::Columns {
            parts: &[
                ("from", "char(3)"),
                ("to", "char(3)"),
                ("rate", "numeric"),
                ("at", "timestamptz(3)"),
            ],
            fixed: &[("from", Fixed::RateFrom), ("to", Fixed::RateTo)],
        },
        format: canonical(),
        ordered: false,
        validator: None,
        shape: Some(&[
            FieldMatch {
                meaning: "a currency it converts from",
                names: &["from", "from_currency", "base", "base_currency"],
                built_on: &[Builtin::Text],
                or_enum: true,
                optional: false,
                list_of: None,
            },
            FieldMatch {
                meaning: "a currency it converts to",
                names: &["to", "to_currency"],
                built_on: &[Builtin::Text],
                or_enum: true,
                optional: false,
                list_of: None,
            },
            FieldMatch {
                meaning: "a rate",
                names: &["rate", "exchange_rate"],
                built_on: &[Builtin::Decimal],
                or_enum: false,
                optional: false,
                list_of: None,
            },
            FieldMatch {
                meaning: "an instant it was quoted at",
                names: &["at", "as_of", "quoted_at"],
                built_on: &[Builtin::Instant],
                or_enum: false,
                optional: false,
                list_of: None,
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
        signals: NO_SIGNALS,
    },
    media::EXIF_ORIENTATION_ENTRY,
    contact::FAMILY_NAME,
    geo::FEATURE_ENTRY,
    geo::FEATURE_COLLECTION_ENTRY,
    software::FILE_NAME,
    software::FILE_PATH,
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
        signals: NO_SIGNALS,
    },
    records::FLIGHT_NUMBER_ENTRY,
    // An exact rational number as written (chunk 5.5j2): `1/2` and `2/4`
    // are two values, as any object's fields are, and unordered. Sent as
    // its canonical text and stored in a column per part.
    StandardType {
        name: FRACTION,
        holds: Holds::Object(&FRACTION_FIELDS),
        text: Some(TextForm::Fraction),
        json: Json::Text,
        storage: Storage::Columns {
            parts: &[("numerator", "bigint"), ("denominator", "bigint")],
            fixed: &[],
        },
        format: canonical(),
        ordered: false,
        validator: None,
        shape: Some(&[
            FieldMatch {
                meaning: "a numerator",
                names: &["numerator"],
                built_on: &[Builtin::Int],
                or_enum: false,
                optional: false,
                list_of: None,
            },
            FieldMatch {
                meaning: "a denominator",
                names: &["denominator"],
                built_on: &[Builtin::Int],
                or_enum: false,
                optional: false,
                list_of: None,
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
        signals: NO_SIGNALS,
    },
    media::FRAME_RATE_ENTRY,
    quantity(
        crate::QuantityKind::Frequency,
        &[
            ("50 Hz", "50 Hz"),
            ("440 Hz", "440 Hz"),
            ("2400000000 Hz", "2,400,000,000 Hz"),
        ],
    ),
    geo::GEOHASH_ENTRY,
    geo::GEOMETRY_ENTRY,
    software::GIT_REF,
    contact::GIVEN_NAME,
    geo::H3_CELL_ENTRY,
    security::HASH_ENTRY,
    health::HEART_RATE_ENTRY,
    security::HMAC_ENTRY,
    web::HOSTNAME,
    codes::HS_CODE,
    markup::HTML_ENTRY,
    web::HTTP_HEADER_NAME,
    web::HTTP_METHOD,
    web::HTTP_STATUS,
    codes::IATA_AIRLINE,
    codes::IATA_AIRPORT,
    identifiers::IBAN,
    codes::ICAO_AIRPORT,
    codes::ICD10_CODE,
    software::IDEMPOTENCY_KEY,
    documents::IDENTITY_DOCUMENT_TYPE,
    documents::IDENTITY_DOCUMENT_KIND_TYPE,
    quantity(
        crate::QuantityKind::Illuminance,
        &[
            ("0.5 lx", "0.5 lx"),
            ("500 lx", "500 lx"),
            ("100000 lx", "100,000 lx"),
        ],
    ),
    media::IMAGE_DIMENSIONS_ENTRY,
    web::IMEI,
    lists::INCOTERM_ENTRY,
    media::INSETS_ENTRY,
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
            // An offset isn't its canonical text, which ends in `Z`, and
            // no boundary reads one (chunk 5.5w2).
            malformed: &[
                "2026-10-25",
                "2026-10-25T09:30:00",
                "yesterday",
                "2026-10-25T10:30:00+01:00",
            ],
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
        signals: signals::INSTANT,
    },
    // A half-open range of instants, from `start` up to but not including
    // `end` (area 10, "Zones and local times").
    StandardType {
        name: INSTANT_RANGE,
        holds: Holds::Object(&INSTANT_RANGE_FIELDS),
        text: Some(TextForm::Interval),
        json: Json::Fields(&["start", "end"]),
        // Stored in one `tstzrange` column.
        storage: Storage::Columns {
            parts: &[("", "tstzrange")],
            fixed: &[],
        },
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
                optional: false,
                list_of: None,
            },
            FieldMatch {
                meaning: "an end",
                names: &["end", "ends_at", "end_at"],
                built_on: &[Builtin::Instant],
                or_enum: false,
                optional: false,
                list_of: None,
            },
        ]),
        examples: Examples {
            valid: &[(
                "2026-10-25T09:00:00Z/2026-10-25T17:00:00Z",
                "25 Oct 2026, 09:00:00 – 25 Oct 2026, 17:00:00",
            )],
            format: Some("date_time_medium"),
            malformed: &[
                "2026-10-25T09:00:00Z",
                "2026-10-25T09:00:00Z/tomorrow",
                "2026-10-25T10:00:00+01:00/2026-10-25T17:00:00Z",
            ],
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
        signals: NO_SIGNALS,
    },
    prices::INTEREST_RATE_ENTRY,
    prices::INTEREST_RATE_BASIS_ENTRY,
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
    codes::ISBN,
    identifiers::ISIN,
    // A week of an ISO week-numbering year, ordered by year and week.
    time_object(
        ISO_WEEK,
        &[("year", YEARS), ("week", Part::Whole(1, 53))],
        &[("year", "smallint"), ("week", "smallint")],
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
    records::JURISDICTION_ENTRY,
    security::JWK_ENTRY,
    security::JWT_ENTRY,
    security::JWT_CLAIMS_ENTRY,
    places::LANGUAGE_ENTRY,
    geo::LATITUDE_ENTRY,
    lists::LAWFUL_BASIS_ENTRY,
    identifiers::LEI,
    quantity(
        crate::QuantityKind::Length,
        &[
            ("-1.5 metre", "-1.5 m"),
            ("0.0254 metre", "0.0254 m"),
            ("1000 metre", "1,000 m"),
        ],
    ),
    geo::LINE_STRING_ENTRY,
    // A date and a clock time with no zone: not an instant until a zone
    // is chosen.
    time_object(
        "LocalDateTime",
        &[
            ("date", Part::Of(Builtin::CalendarDate)),
            ("time", Part::Standard("LocalTime")),
        ],
        &[("", "timestamp")],
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
        &[("", "time")],
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
        signals: NO_SIGNALS,
    },
    rich_text::LOCALISED_TEXT_ENTRY,
    software::LOG_LEVEL,
    geo::LONGITUDE_ENTRY,
    codes::LOT_NUMBER,
    web::MAC_ADDRESS,
    geo::MAP_TILE_ENTRY,
    markup::MARKDOWN_ENTRY,
    card_types::MASKED_CARD_NUMBER_TYPE,
    quantity(
        crate::QuantityKind::Mass,
        &[
            ("0.25 kg", "0.25 kg"),
            ("2.5 kg", "2.5 kg"),
            ("1000 kg", "1,000 kg"),
        ],
    ),
    records::MODEL_ID_ENTRY,
    // An amount in a currency's minor units (area 19, "Money and
    // payments"): its JSON is the amount and the code, as
    // Stripe sends it, and it is stored in two columns, a `bigint` of minor
    // units and the code, also for a type that fixes the currency. Only
    // amounts in one currency are ordered, so its examples are all pounds.
    StandardType {
        name: "Money",
        holds: Holds::Builtin(Builtin::Money),
        text: Some(TextForm::Money),
        json: Json::Parts(&[
            (crate::MONEY_FIELDS[0], Member::MinorUnits),
            (crate::MONEY_FIELDS[1], Member::Code(crate::CURRENCY)),
        ]),
        storage: Storage::Columns {
            parts: &[("amount", "bigint"), ("currency", "char(3)")],
            fixed: &[("currency", Fixed::Currency)],
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
                optional: false,
                list_of: None,
            },
            FieldMatch {
                meaning: "a currency",
                names: &["currency", "currency_code"],
                built_on: &[Builtin::Text],
                or_enum: true,
                optional: false,
                list_of: None,
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
        signals: signals::MONEY,
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
        signals: NO_SIGNALS,
    },
    codes::MPAN,
    codes::MPRN,
    geo::MULTI_LINE_STRING_ENTRY,
    geo::MULTI_POINT_ENTRY,
    geo::MULTI_POLYGON_ENTRY,
    places::NATIONALITY_ENTRY,
    web::NETWORK_PORT,
    identifiers::NHS_NUMBER,
    security::OAUTH_SCOPE_ENTRY,
    security::OIDC_CLAIMS_ENTRY,
    security::ONE_TIME_CODE_ENTRY,
    schedules::OPENING_HOURS,
    codes::ORCID,
    software::PACKAGE_URL,
    media::PAPER_SIZE_ENTRY,
    documents::PASSPORT_NUMBER_TYPE,
    security::PASSWORD_ENTRY,
    security::PASSWORD_CHECK_ENTRY,
    security::PASSWORD_HASH_ENTRY,
    security::PASSWORD_POLICY_ENTRY,
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
        signals: signals::PERCENTAGE,
    },
    contact::PERSON_NAME,
    contact::PHONE_EXTENSION,
    phones::PHONE_NUMBER,
    geo::POINT_ENTRY,
    geo::POLYGON_ENTRY,
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
    prices::PRICE_ENTRY,
    web::PROBLEM_DETAILS_ENTRY,
    security::PUBLIC_KEY_ENTRY,
    web::PUSH_TOKEN,
    // A quarter of a calendar year, ordered by year and quarter.
    time_object(
        "Quarter",
        &[("year", YEARS), ("quarter", Part::Whole(1, 4))],
        &[("year", "smallint"), ("quarter", "smallint")],
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
        // Stored in one column of the range type its item type has.
        storage: Storage::Columns {
            parts: &[("", "range")],
            fixed: &[],
        },
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
        signals: NO_SIGNALS,
    },
    records::RATING_ENTRY,
    // One number to another as written (chunk 5.5j2): `16:9` and `32:18`
    // are two values, unordered. Sent as its canonical text and stored in a
    // column per part.
    StandardType {
        name: RATIO,
        holds: Holds::Object(&RATIO_FIELDS),
        text: Some(TextForm::Ratio),
        json: Json::Text,
        storage: Storage::Columns {
            parts: &[("numerator", "numeric"), ("denominator", "numeric")],
            fixed: &[],
        },
        format: canonical(),
        ordered: false,
        validator: None,
        shape: Some(&[
            FieldMatch {
                meaning: "a numerator",
                names: &["numerator"],
                built_on: &[Builtin::Decimal],
                or_enum: false,
                optional: false,
                list_of: None,
            },
            FieldMatch {
                meaning: "a denominator",
                names: &["denominator"],
                built_on: &[Builtin::Decimal],
                or_enum: false,
                optional: false,
                list_of: None,
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
        signals: NO_SIGNALS,
    },
    media::RECT_ENTRY,
    schedules::RECURRENCE_RULE,
    software::REGULAR_EXPRESSION,
    web::RELATIVE_URL,
    rich_text::RICH_TEXT_ENTRY,
    rich_text::RICH_TEXT_BLOCK_ENTRY,
    rich_text::RICH_TEXT_MARK_ENTRY,
    rich_text::RICH_TEXT_SPAN_ENTRY,
    health::ROUTE_OF_ADMINISTRATION_ENTRY,
    security::SECRET_TEXT_ENTRY,
    software::SEMANTIC_VERSION_ENTRY,
    codes::SERIAL_NUMBER,
    records::SHOE_SIZE_ENTRY,
    records::SHOE_SIZE_SYSTEM_ENTRY,
    codes::SKU,
    web::SLUG,
    codes::SNOMED_CONCEPT,
    quantity(
        crate::QuantityKind::SoundLevel,
        &[
            ("-3.5 dB", "-3.5 dB"),
            ("60 dB", "60 dB"),
            ("120 dB", "120 dB"),
        ],
    ),
    software::SPAN_ID,
    software::SPDX_LICENCE,
    quantity(
        crate::QuantityKind::Speed,
        &[
            ("0.5 mps", "0.5 m/s"),
            ("13.4112 mps", "13.4112 m/s"),
            ("340 mps", "340 m/s"),
        ],
    ),
    codes::SSCC,
    media::SUBTITLE_CUE_ENTRY,
    documents::TAX_ID_TYPE,
    prices::TAX_RATE_ENTRY,
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
    lists::TENURE_ENTRY,
    places::TEXT_DIRECTION_ENTRY,
    media::TIMECODE_ENTRY,
    ai::TOKEN_COUNT_ENTRY,
    ai::TOKEN_USAGE_ENTRY,
    security::TOTP_SECRET_ENTRY,
    software::TRACE_ID,
    software::TRACE_PARENT,
    records::TRACKING_NUMBER_ENTRY,
    identifiers::UK_ACCOUNT_NUMBER,
    identifiers::UK_COMPANY_NUMBER,
    identifiers::UK_NATIONAL_INSURANCE_NUMBER,
    identifiers::UK_SORT_CODE,
    identifiers::UK_UTR,
    software::ULID,
    places::UN_LOCODE,
    codes::UN_NUMBER,
    prices::UNIT_PRICE_ENTRY,
    records::UNIT_QUANTITY_ENTRY,
    codes::UPC_A,
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
    )
    .with_signals(signals::URL),
    web::URL_PATH,
    identifiers::US_EIN,
    identifiers::US_SSN,
    web::USER_AGENT,
    contact::USERNAME,
    // An offset from UTC, as a zone has at a moment; unordered.
    time_object(
        "UtcOffset",
        &[("minutes", Part::Whole(-1080, 1080))],
        &[("", "smallint")],
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
        signals: signals::UUID,
    },
    documents::VAT_NUMBER_TYPE,
    ai::VECTOR_ENTRY,
    records::VEHICLE_REGISTRATION_ENTRY,
    software::VERSION_RANGE,
    media::VIDEO_CODEC_ENTRY,
    media::VIDEO_RESOLUTION_ENTRY,
    codes::VIN,
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
    security::WEB_AUTHN_CREDENTIAL_ENTRY,
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
        signals: NO_SIGNALS,
    },
    places::WRITING_SCRIPT_ENTRY,
    // A calendar year on its own, ordered.
    time_object(
        "Year",
        &[("year", YEARS)],
        &[("", "smallint")],
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
        &[("year", "smallint"), ("month", "smallint")],
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
        signals: NO_SIGNALS,
    },
    StandardType {
        name: "ZonedTime",
        holds: Holds::Builtin(Builtin::ZonedTime),
        text: Some(TextForm::Zoned),
        json: Json::Parts(&[("instant", Member::Instant), ("zone", Member::Code(ZONE))]),
        // An instant and its zone, stored in two columns.
        storage: Storage::Columns {
            parts: &[("instant", "timestamptz(3)"), ("zone", "text")],
            fixed: &[],
        },
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
        signals: NO_SIGNALS,
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

/// `IsoWeek` (area 19, "Time and calendar additions").
pub const ISO_WEEK: &str = "IsoWeek";

/// `IsoWeek`'s rule across its parts (chunk 5.5t2): week 53 only in a year
/// that has one.
pub const ISO_WEEK_RULES: &[across::StandardRule] = &[across::StandardRule {
    name: "week_in_year",
    written: "week is 53 only in a year with 53 ISO weeks",
    label: "a week its year doesn't have",
    reason: "An ISO 8601 week-numbering year has 53 weeks when it starts on a Thursday, or on a Wednesday in a leap year, and 52 otherwise, so week 53 of a 52-week year isn't a week.",
    next: "Write a week from 1 to 52, or a year with 53 weeks",
    check: across::Check::By(|parts| {
        let week = parts.part("week").value().and_then(|week| week.whole());
        if week.is_some_and(|week| week <= 52) {
            return across::Verdict::Holds;
        }
        let year = parts.part("year").value().and_then(|year| year.whole());
        match (week, year.and_then(crate::iso_weeks_in)) {
            (Some(week), Some(weeks)) if week <= weeks => across::Verdict::Holds,
            (Some(_), Some(_)) => across::Verdict::Breaks("week"),
            _ => across::Verdict::Unknown,
        }
    }),
}];

/// `Month` (area 19, "Closed value sets are enums").
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
/// in chunk 5.5h), every closed list its code, `Country` `GB` and `IN`
/// rather than `in_` (chunk 5.5l), and every other standard enum too,
/// `Zone` `Europe/London` and `HttpMethod` `GET` (chunk 5.5w): each
/// standard enum by its entry's codes.
pub fn stored_by_code(ty: &str) -> Option<&'static [(&'static str, &'static str)]> {
    standard_type(ty).and_then(StandardType::codes)
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

/// The standard type called `name`, looked up once in a table by name,
/// since every boundary asks for a value's entry (chunk 5.5w3).
pub fn standard_type(name: &str) -> Option<&'static StandardType> {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static BY_NAME: OnceLock<HashMap<&'static str, &'static StandardType>> = OnceLock::new();
    BY_NAME
        .get_or_init(|| {
            let mut out = HashMap::with_capacity(STANDARD_TYPES.len());
            for entry in STANDARD_TYPES {
                out.entry(entry.name).or_insert(entry);
            }
            out
        })
        .get(name)
        .copied()
}

/// Every release that added a standard type, oldest first, each with the
/// names it added in name order: the release notes' "Names each release
/// adds" (area 19, "A release adding a standard type").
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

/// The standard type a built-in type is, if one: `Email`, `Instant`;
/// `None` for a representation (`Int`, `Text`). Looked up once in a table,
/// since the JSON reader and writer ask for every value (chunk 5.5w3).
pub fn of_builtin(builtin: Builtin) -> Option<&'static StandardType> {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static BY_BUILTIN: OnceLock<HashMap<Builtin, &'static StandardType>> = OnceLock::new();
    BY_BUILTIN
        .get_or_init(|| {
            let mut out = HashMap::new();
            for entry in STANDARD_TYPES {
                if let Holds::Builtin(builtin) = entry.holds {
                    out.entry(builtin).or_insert(entry);
                }
            }
            out
        })
        .get(&builtin)
        .copied()
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

/// The variant `variant` of the standard choice `name`, with its fields
/// (chunk 5.5i3); `None` when `name` isn't a choice or has no such variant.
pub fn choice_variant(name: &str, variant: &str) -> Option<&'static [(&'static str, Part)]> {
    let Holds::Choice(variants) = standard_type(name)?.holds else {
        return None;
    };
    variants
        .iter()
        .find(|(listed, _)| *listed == variant)
        .map(|(_, fields)| *fields)
}

/// What a standard type held as text, as a whole number or with bounds of
/// its own is built on, and the rules a program's type of it keeps (area
/// 19, chunks 5.5i, 5.5n and 5.5i3): `Text` with its own check, or `Int`
/// with its range, each naming the type as their `standard`; or its
/// built-in type with its bounds, a `TaxRate`'s `Percentage` from 0%, which
/// name none, so a plain value of that type builds one by its bounds alone.
/// `None` for any other type.
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
        Holds::Bounded { of, min, max } => Some((
            of,
            crate::Rules {
                min,
                max,
                double: geo::holds_double(entry.name),
                ..crate::Rules::default()
            },
        )),
        // Bytes with the entry's own check (chunk 5.5r3).
        Holds::Bytes => Some((
            Builtin::Bytes,
            crate::Rules {
                standard: Some(entry.name),
                ..crate::Rules::default()
            },
        )),
        _ => None,
    }
}

/// The rules a named collection keeps as a type, where it has any (chunk
/// 5.5s2): a `Vector`'s lengths, and its items kept as floats.
pub fn collection_rules(name: &str) -> Option<crate::Rules> {
    (name == ai::VECTOR).then(ai::vector_rules)
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

impl StandardType {
    /// The entry with `signals` (area 14, "Suggestions"), for an entry a
    /// helper builds.
    pub const fn with_signals(self, signals: Signals) -> StandardType {
        StandardType { signals, ..self }
    }

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
    /// The kind of quantity it is, for a quantity or a type built on one
    /// with bounds of its own (a heart rate): what a [`Member::Unit`] is a
    /// unit of.
    pub fn quantity(&self) -> Option<crate::QuantityKind> {
        match self.holds {
            Holds::Builtin(Builtin::Quantity(kind))
            | Holds::Bounded {
                of: Builtin::Quantity(kind),
                ..
            } => Some(kind),
            _ => None,
        }
    }

    pub fn codes(&self) -> Option<&'static [(&'static str, &'static str)]> {
        match self.holds {
            Holds::Enum(codes) => Some(codes),
            _ => None,
        }
    }

    /// The names of its variants, for an enum or a choice (chunk 5.5i3).
    pub fn variants(&self) -> Option<Vec<&'static str>> {
        match self.holds {
            Holds::Enum(codes) => Some(codes.iter().map(|(name, _)| *name).collect()),
            Holds::Choice(variants) => Some(variants.iter().map(|(name, _)| *name).collect()),
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
                .chain(ai::PERSONAL)
                .chain(banks::PERSONAL)
                .chain(card_types::PERSONAL)
                .chain(documents::PERSONAL)
                .chain(health::PERSONAL)
                .chain(lists::PERSONAL)
                .find(|(name, _)| *name == self.name)
                .map(|(_, category)| *category),
        }
    }

    /// The kind of quantity `text.format`'s `unit:` names a unit of for
    /// it: a quantity's own, or a type built on one's (chunk 5.5m2), or
    /// the kind its measures are shown in, for an object whose parts are
    /// all one kind (`Dimensions`' lengths); `None` for a type that takes
    /// no unit.
    pub fn unit_kind(&self) -> Option<crate::QuantityKind> {
        self.format_option("unit")?;
        match (self.holds, self.format.key) {
            (
                Holds::Builtin(Builtin::Quantity(kind))
                | Holds::Bounded {
                    of: Builtin::Quantity(kind),
                    ..
                },
                _,
            ) => Some(kind),
            (_, FormatKey::Measures { units: [only], .. }) => Some(only.kind),
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

    /// The named columns of a type stored in more than one (`Money`'s);
    /// none for a type stored in one.
    pub fn columns(&self) -> &'static [(&'static str, &'static str)] {
        match self.storage {
            Storage::Column(_) | Storage::Geography | Storage::Vector | Storage::Never => &[],
            Storage::Columns { parts, .. } => parts,
        }
    }

    /// Whether its named column `at` may be `NULL` while the value is
    /// there: one holding an object's part written `Part::Optional` (chunk
    /// 5.5l), or a field a choice's stored variant may not have (chunk
    /// 5.5w).
    pub fn part_may_be_none(&self, at: usize) -> bool {
        storage::layout(self)
            .and_then(|columns| columns.get(at).map(|column| column.nullable))
            .unwrap_or(false)
    }

    /// The comment on its named column `at`: the unit a quantity there is
    /// held in (chunk 5.5w).
    pub fn part_comment(&self, at: usize) -> Option<String> {
        storage::layout(self)?.get(at)?.comment.clone()
    }

    /// Of its named columns, those a type built on it can fix, each with
    /// the setting that fixes it.
    pub fn fixed_parts(&self) -> &'static [(&'static str, Fixed)] {
        match self.storage {
            Storage::Column(_) | Storage::Geography | Storage::Vector | Storage::Never => &[],
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
            (SqlType::Bytes, Holds::Bytes) => Column::Bytes,
            (SqlType::Uuid, _) => Column::Uuid,
            (SqlType::Timestamp, _) => Column::Instant,
            (SqlType::Date, _) => Column::Date,
            (SqlType::Int8, Holds::Builtin(Builtin::Duration)) => Column::Duration,
            (SqlType::Int4 | SqlType::Int8, Holds::Whole { .. }) => Column::Int,
            (SqlType::Numeric, Holds::Builtin(Builtin::Quantity(kind)))
            | (
                SqlType::Numeric,
                Holds::Bounded {
                    of: Builtin::Quantity(kind),
                    ..
                },
            ) => Column::Quantity(kind),
            (SqlType::Numeric, Holds::Builtin(Builtin::Percentage))
            | (
                SqlType::Numeric,
                Holds::Bounded {
                    of: Builtin::Percentage,
                    ..
                },
            ) => Column::Percentage,
            // A latitude, a longitude, a bearing (chunk 5.5s).
            (
                SqlType::Numeric,
                Holds::Bounded {
                    of: Builtin::Decimal,
                    ..
                },
            ) => Column::Decimal,
            _ => return None,
        })
    }

    /// The PostgreSQL column type its storage is: the first named column's,
    /// for a type stored in more than one.
    pub fn column_type(&self) -> &'static str {
        if let Holds::Builtin(Builtin::Quantity(kind))
        | Holds::Bounded {
            of: Builtin::Quantity(kind),
            ..
        } = self.holds
        {
            return kind.column_type();
        }
        let storage = match self.storage {
            Storage::Column(storage) => storage,
            Storage::Columns { parts, .. } => {
                return parts.first().map_or("jsonb", |(_, ty)| *ty);
            }
            Storage::Geography => return "geography",
            Storage::Vector => return "vector",
            Storage::Never => return "never",
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
            SqlType::LocalTimestamp => "timestamp",
            SqlType::Vector => "vector",
            SqlType::Geography => "geography",
            SqlType::Float8 => "double precision",
            SqlType::Json | SqlType::Array(_) | SqlType::Range(_) => "jsonb",
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
        assert_eq!(names.len(), 259, "the 259 standard types built so far");
    }

    #[test]
    fn every_entry_is_whole() {
        for entry in STANDARD_TYPES {
            // Any text is `Html` once it is made safe (chunk 5.5o), so it
            // alone has no malformed examples.
            assert!(
                !entry.examples.valid.is_empty()
                    && (entry.examples.malformed.is_empty()
                        == (entry.text == Some(TextForm::Html))),
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
            assert!(
                entry.personal().is_none_or(|category| crate::ai::CATEGORIES
                    .iter()
                    .any(|(name, _)| *name == category)),
                "{}",
                entry.name
            );
            if let Holds::Text { pattern, .. } = entry.holds {
                assert!(crate::compile_pattern(pattern).is_ok(), "{}", entry.name);
                // Checked, made safe as `Html` is (chunk 5.5o), or a
                // secret value, read and never written (chunk 5.5r2).
                assert!(
                    matches!(
                        entry.text,
                        Some(TextForm::Checked | TextForm::Html | TextForm::Secret)
                    ),
                    "{}",
                    entry.name
                );
                assert_eq!(
                    entry.text == Some(TextForm::Html),
                    markup::sanitises(entry.name),
                    "{}",
                    entry.name
                );
                // A secret value is read and never written, shown or
                // stored, every one of them alike.
                let secret = security::secret_value(entry.name).is_some();
                assert_eq!(
                    entry.text == Some(TextForm::Secret),
                    secret,
                    "{}",
                    entry.name
                );
                assert_eq!(entry.json == Json::Secret, secret, "{}", entry.name);
                assert_eq!(
                    entry.format.key == FormatKey::Redacted,
                    secret,
                    "{}",
                    entry.name
                );
                assert_eq!(
                    entry.storage,
                    if secret {
                        Storage::Never
                    } else {
                        Storage::Column(SqlType::Text)
                    },
                    "{}",
                    entry.name
                );
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
            // A choice is sent in area 2's tagged form, and has no single
            // text form (chunk 5.5i3); `Geometry` is sent as the GeoJSON
            // geometry it holds (chunk 5.5s).
            assert_eq!(
                matches!(entry.holds, Holds::Choice(_)) && entry.json != Json::GeoJson,
                entry.json == Json::Tagged,
                "{} is sent tagged exactly when it is a choice",
                entry.name
            );
            // GeoJSON is a geometry's alone, and a geometry has no single
            // text form, since one would hold an axis order (chunk 5.5s).
            if entry.json == Json::GeoJson {
                assert!(entry.text.is_none(), "{}", entry.name);
                assert_eq!(entry.storage, Storage::Geography, "{}", entry.name);
            }
            if let Holds::Choice(variants) = entry.holds {
                assert!(entry.text.is_none(), "{}", entry.name);
                assert!(variants.iter().all(|(_, fields)| !fields.is_empty()));
            }
            // A named collection is sent as JSON's own collection, an
            // array or an object, and has no single text form (chunk
            // 5.5o2).
            assert_eq!(
                (
                    matches!(entry.holds, Holds::List(_)),
                    matches!(entry.holds, Holds::Map(..))
                ),
                (entry.json == Json::Array, entry.json == Json::Keyed),
                "{} is sent as an array or an object exactly when it is a list or a map",
                entry.name
            );
            // A vector is stored in pgvector, which the AI area builds
            // (chunk 5.5s2).
            if matches!(entry.holds, Holds::List(_) | Holds::Map(..)) {
                assert!(entry.text.is_none(), "{}", entry.name);
                let column = if entry.holds == Holds::List(Part::Float) {
                    "vector"
                } else {
                    "jsonb"
                };
                assert_eq!(entry.column_type(), column, "{}", entry.name);
            }
            if let Holds::Whole { min, max } = entry.holds {
                assert_eq!(entry.text, Some(TextForm::Whole), "{}", entry.name);
                assert_eq!(entry.json, Json::Number, "{}", entry.name);
                assert!(min.abs() < 1 << 53 && max.abs() < 1 << 53, "{}", entry.name);
            }
            if let Some(wide) = within(entry.name) {
                let wide = standard_type(wide).expect("a standard type");
                for (text, _) in entry.examples.valid {
                    assert!(
                        wide.accepts(text),
                        "{}: `{text}` is a {}",
                        entry.name,
                        wide.name
                    );
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
            // A distinctive shape (question 5): two fields or more, or a
            // list of a shape (`LineString`'s points, chunk 5.5u6), at
            // least one named for the domain.
            if let Some(fields) = entry.shape {
                assert!(
                    fields.len() >= 2 || fields.iter().any(|field| field.list_of.is_some()),
                    "{}",
                    entry.name
                );
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
            Some("tstzrange")
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

    /// Every standard object and choice is stored through named columns
    /// (chunk 5.5w): each entry's columns hold its parts as
    /// [`storage::layout`] lays them out, a whole number in a column of any
    /// width, and a choice's kind is a column no variant's field takes.
    /// Only the collections stay JSON, and the types whose storage another
    /// area builds.
    #[test]
    fn objects_and_choices_are_stored_in_named_columns() {
        for entry in STANDARD_TYPES {
            let collection = matches!(entry.holds, Holds::List(_) | Holds::Map(..));
            let elsewhere = matches!(
                entry.storage,
                Storage::Geography | Storage::Vector | Storage::Never
            );
            if matches!(entry.holds, Holds::Object(_) | Holds::Choice(_)) && !elsewhere {
                // A choice whose variants hold lists of objects (a rich
                // text's blocks) is stored as JSON, with the collections.
                if entry.name == rich_text::RICH_TEXT_BLOCK {
                    assert_eq!(entry.storage, Storage::Column(SqlType::Json));
                    continue;
                }
                assert!(
                    !entry.columns().is_empty(),
                    "{} is stored in named columns",
                    entry.name
                );
            }
            if collection && !elsewhere {
                assert_eq!(
                    entry.storage,
                    Storage::Column(SqlType::Json),
                    "{}",
                    entry.name
                );
            }
            if entry.columns().is_empty() {
                continue;
            }
            let laid = storage::layout(entry)
                .unwrap_or_else(|| panic!("{}: its parts are laid out", entry.name));
            assert_eq!(laid.len(), entry.columns().len(), "{}", entry.name);
            let class = |ty: &str| SqlType::of_column(ty).map(SqlType::param);
            for ((suffix, declared), column) in entry.columns().iter().zip(&laid) {
                if entry.name == RANGE {
                    continue;
                }
                assert!(
                    class(declared).is_some(),
                    "{}: `{declared}` is a column type",
                    entry.name
                );
                let same = if class(declared) == Some(SqlType::Int8) {
                    class(&column.sql_type) == Some(SqlType::Int8)
                } else {
                    *declared == column.sql_type
                };
                assert!(
                    same,
                    "{}.{suffix}: declared `{declared}`, its part is `{}`",
                    entry.name, column.sql_type
                );
            }
            let suffixes: Vec<&str> = entry.columns().iter().map(|(suffix, _)| *suffix).collect();
            let mut unique = suffixes.clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(
                unique.len(),
                suffixes.len(),
                "{}: its columns' names",
                entry.name
            );
            if let Holds::Choice(variants) = entry.holds {
                assert_eq!(suffixes.first(), Some(&storage::KIND), "{}", entry.name);
                assert!(
                    storage::choice_fields(variants)
                        .iter()
                        .all(|(name, _)| *name != storage::KIND),
                    "{}: no variant's field is called `kind`",
                    entry.name
                );
            }
        }
    }
}
