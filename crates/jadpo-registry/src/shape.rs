//! The shapes of the protocol's JSON: each answer and body as the table
//! gives it, which the local registry's answers are checked against, its
//! OpenAPI document is written from, and a served document is compared
//! with.

use serde_json::Value;

use crate::names::{Digest, Handle, ScopedName, Version, is_part};

/// What a text value holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Text {
    Plain,
    /// RFC 3339, in UTC: `2026-10-31T09:30:00Z`.
    DateTime,
    ScopedName,
    Handle,
    /// A package's part of a scoped name: `postcodes`.
    NamePart,
    Version,
    Digest,
    /// A log hash: 64 lower-case hex digits.
    Hash,
}

impl Text {
    /// The pattern an OpenAPI document gives it, if any.
    pub fn pattern(self) -> Option<&'static str> {
        Some(match self {
            Text::Plain | Text::DateTime => return None,
            Text::ScopedName => "^@[a-z](-?[a-z0-9])*/[a-z](-?[a-z0-9])*$",
            Text::Handle => "^@[a-z](-?[a-z0-9])*$",
            Text::NamePart => "^[a-z](-?[a-z0-9])*$",
            Text::Version => {
                "^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)(-[0-9A-Za-z.-]+)?$"
            }
            Text::Digest => "^sha256:[0-9a-f]{64}$",
            Text::Hash => "^[0-9a-f]{64}$",
        })
    }

    fn holds(self, text: &str) -> bool {
        match self {
            Text::Plain => true,
            Text::DateTime => is_utc_date_time(text),
            Text::ScopedName => ScopedName::parse(text).is_ok(),
            Text::Handle => Handle::parse(text).is_ok(),
            Text::NamePart => is_part(text),
            Text::Version => Version::parse(text).is_ok(),
            Text::Digest => Digest::parse(text).is_ok(),
            Text::Hash => crate::log::Hash::from_hex(text).is_some(),
        }
    }
}

/// `2026-10-31T09:30:00Z`, with fractions of a second allowed.
fn is_utc_date_time(text: &str) -> bool {
    let b = text.as_bytes();
    let digits = |range: std::ops::Range<usize>| {
        b.get(range)
            .is_some_and(|d| d.iter().all(u8::is_ascii_digit))
    };
    if b.len() < 20 || !text.ends_with('Z') {
        return false;
    }
    let shape = digits(0..4)
        && b[4] == b'-'
        && digits(5..7)
        && b[7] == b'-'
        && digits(8..10)
        && b[10] == b'T'
        && digits(11..13)
        && b[13] == b':'
        && digits(14..16)
        && b[16] == b':'
        && digits(17..19);
    let fraction = &b[19..b.len() - 1];
    shape
        && (fraction.is_empty()
            || (fraction[0] == b'.'
                && fraction.len() > 1
                && fraction[1..].iter().all(u8::is_ascii_digit)))
}

/// A JSON value's shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    Text(Text),
    /// A whole number, sent as a JSON number.
    Integer,
    Bool,
    /// Text that is one of these values.
    Enum(&'static [&'static str]),
    List(Box<Shape>),
    /// The value, or `null`.
    Nullable(Box<Shape>),
    /// An object whose fields are exactly these.
    Object(Vec<Field>),
    /// One of [`named`]'s shapes.
    Named(&'static str),
}

/// One field of an object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: &'static str,
    pub shape: Shape,
    /// May be left out of a body; an answer always sends every field.
    pub optional: bool,
}

fn field(name: &'static str, shape: Shape) -> Field {
    Field {
        name,
        shape,
        optional: false,
    }
}

fn text(kind: Text) -> Shape {
    Shape::Text(kind)
}

fn list(shape: Shape) -> Shape {
    Shape::List(Box::new(shape))
}

fn nullable(shape: Shape) -> Shape {
    Shape::Nullable(Box::new(shape))
}

fn object(fields: Vec<Field>) -> Shape {
    Shape::Object(fields)
}

/// Every named shape, in the order an OpenAPI document lists them.
pub const NAMES: &[&str] = &[
    "Package",
    "PackageKind",
    "Publisher",
    "ReleaseSummary",
    "Release",
    "Facts",
    "HostFact",
    "PersonalDataFact",
    "FailureFact",
    "StandardCandidate",
    "ChangeEntry",
    "Change",
    "LogHead",
    "LogEntry",
    "InclusionProof",
    "ConsistencyProof",
    "SearchAnswer",
    "SearchResult",
    "AdvisoryQuery",
    "Installed",
    "AdvisoryAnswer",
    "Advisory",
    "StartRelease",
    "StartedRelease",
    "ReleaseRequest",
    "ReleaseState",
    "BarRow",
];

/// The shape named `name`, as the protocol's types send it.
pub fn named(name: &str) -> Option<Shape> {
    use Shape::{Bool, Integer, Named};
    let plain = || text(Text::Plain);
    Some(match name {
        "Package" => object(vec![
            field("package", text(Text::ScopedName)),
            field("description", plain()),
            field("kind", Named("PackageKind")),
            field("code_name", plain()),
            field("label", plain()),
            field("publisher", Named("Publisher")),
            field("maintained", Bool),
            field("successor", nullable(text(Text::ScopedName))),
            field("releases", list(Named("ReleaseSummary"))),
        ]),
        "PackageKind" => Shape::Enum(crate::protocol::PackageKind::VALUES),
        "Publisher" => object(vec![
            field("handle", text(Text::Handle)),
            field("verified", Bool),
            field("official", Bool),
        ]),
        "ReleaseSummary" => object(vec![
            field("version", text(Text::Version)),
            field("digest", text(Text::Digest)),
            field("released_at", text(Text::DateTime)),
            field("yanked", Bool),
            field("urgent", Bool),
        ]),
        "Release" => object(vec![
            field("package", text(Text::ScopedName)),
            field("version", text(Text::Version)),
            field("repository", plain()),
            field("tag", plain()),
            field("commit", plain()),
            field("digest", text(Text::Digest)),
            field("surface", plain()),
            field("facts", Named("Facts")),
            field("change_record", list(Named("ChangeEntry"))),
            field("account", plain()),
            field("jadpo_releases", list(text(Text::Version))),
            field("log_index", Integer),
        ]),
        "Facts" => object(vec![
            field("kind", Named("PackageKind")),
            field("pure", Bool),
            field("hosts", list(Named("HostFact"))),
            field("changing_calls", list(plain())),
            field("personal_data", list(Named("PersonalDataFact"))),
            field("erasure", Bool),
            field("storable_types", list(plain())),
            field("failures", list(Named("FailureFact"))),
            field("confirmed_operations", list(plain())),
            field("last_contract_run", nullable(text(Text::DateTime))),
            field("standard_candidates", list(Named("StandardCandidate"))),
        ]),
        "HostFact" => object(vec![
            field("host", plain()),
            field("operations", list(plain())),
        ]),
        "PersonalDataFact" => object(vec![
            field("field", plain()),
            field("host", plain()),
            field("operation", plain()),
            field("categories", list(plain())),
            field("decided_only", Bool),
        ]),
        "FailureFact" => object(vec![
            field("declaration", plain()),
            field("failures", list(plain())),
        ]),
        "StandardCandidate" => object(vec![
            field("declaration", plain()),
            field("standard", plain()),
        ]),
        "ChangeEntry" => object(vec![
            field("declaration", plain()),
            field("kind", plain()),
            field("change", Named("Change")),
            field("detail", nullable(plain())),
            field("why", nullable(plain())),
        ]),
        "Change" => Shape::Enum(crate::protocol::Change::VALUES),
        "LogHead" => object(vec![
            field("size", Integer),
            field("root", text(Text::Hash)),
        ]),
        "LogEntry" => object(vec![
            field("package", text(Text::ScopedName)),
            field("version", text(Text::Version)),
            field("digest", text(Text::Digest)),
        ]),
        "InclusionProof" => object(vec![
            field("entry", Named("LogEntry")),
            field("index", Integer),
            field("size", Integer),
            field("hashes", list(text(Text::Hash))),
        ]),
        "ConsistencyProof" => object(vec![
            field("from", Integer),
            field("to", Integer),
            field("hashes", list(text(Text::Hash))),
        ]),
        "SearchAnswer" => object(vec![field("results", list(Named("SearchResult")))]),
        "SearchResult" => object(vec![
            field("package", nullable(text(Text::ScopedName))),
            field("declaration", plain()),
            field("why", plain()),
        ]),
        "AdvisoryQuery" => object(vec![field("packages", list(Named("Installed")))]),
        "Installed" => object(vec![
            field("package", text(Text::ScopedName)),
            field("version", text(Text::Version)),
        ]),
        "AdvisoryAnswer" => object(vec![field("advisories", list(Named("Advisory")))]),
        "Advisory" => object(vec![
            field("id", plain()),
            field("package", text(Text::ScopedName)),
            field("declarations", list(plain())),
            field("from", text(Text::Version)),
            field("to", text(Text::Version)),
            field("summary", plain()),
            field("copies", list(text(Text::ScopedName))),
        ]),
        "StartRelease" => object(vec![
            field("repository", plain()),
            field("tag", plain()),
            field("dry_run", Bool),
        ]),
        "StartedRelease" => object(vec![field("id", plain())]),
        "ReleaseRequest" => object(vec![
            field("id", plain()),
            field("state", Named("ReleaseState")),
            field("bar", list(Named("BarRow"))),
            field("facts", nullable(Named("Facts"))),
            field("change_record", nullable(list(Named("ChangeEntry")))),
        ]),
        "ReleaseState" => Shape::Enum(crate::protocol::ReleaseState::VALUES),
        "BarRow" => object(vec![
            field("check", plain()),
            field("passed", Bool),
            field("detail", plain()),
        ]),
        _ => return None,
    })
}

/// Checks `value` has `shape` exactly: every field present, none other,
/// and each value of its kind.
pub fn check_value(shape: &Shape, value: &Value) -> Result<(), String> {
    check_at(shape, value, "answer")
}

fn check_at(shape: &Shape, value: &Value, at: &str) -> Result<(), String> {
    let wrong = |what: &str| Err(format!("{at}: {what}"));
    match shape {
        Shape::Named(name) => match named(name) {
            Some(shape) => check_at(&shape, value, at),
            None => wrong(&format!("no shape `{name}`")),
        },
        Shape::Nullable(inner) => {
            if value.is_null() {
                Ok(())
            } else {
                check_at(inner, value, at)
            }
        }
        Shape::Text(kind) => match value.as_str() {
            Some(text) if kind.holds(text) => Ok(()),
            Some(text) => wrong(&format!("`{text}` isn't {kind:?}")),
            None => wrong("isn't text"),
        },
        Shape::Integer => match value.as_u64() {
            Some(n) if n <= (1 << 53) => Ok(()),
            _ => wrong("isn't a whole number"),
        },
        Shape::Bool => {
            if value.is_boolean() {
                Ok(())
            } else {
                wrong("isn't true or false")
            }
        }
        Shape::Enum(values) => match value.as_str() {
            Some(text) if values.contains(&text) => Ok(()),
            _ => wrong(&format!("isn't one of {}", values.join(", "))),
        },
        Shape::List(inner) => match value.as_array() {
            Some(items) => items
                .iter()
                .try_for_each(|item| check_at(inner, item, &format!("{at}[]"))),
            None => wrong("isn't a list"),
        },
        Shape::Object(fields) => {
            let Some(members) = value.as_object() else {
                return wrong("isn't an object");
            };
            for name in members.keys() {
                if !fields.iter().any(|f| f.name == name) {
                    return Err(format!("{at}.{name}: isn't in the protocol"));
                }
            }
            for f in fields {
                match members.get(f.name) {
                    Some(member) => check_at(&f.shape, member, &format!("{at}.{}", f.name))?,
                    None if f.optional => {}
                    None => return Err(format!("{at}.{}: missing", f.name)),
                }
            }
            Ok(())
        }
    }
}
