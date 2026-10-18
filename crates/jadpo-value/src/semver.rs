//! `semver`'s operation (area 19, "Web and software", chunk 5.5n2) and a
//! `SemanticVersion`'s text and order, over `jadpo_std::standard::software`,
//! where the grammar they share with the types' validators lives.

use std::cmp::Ordering;

use jadpo_std::standard::software::{self, SEMANTIC_VERSION, SEMANTIC_VERSION_FIELDS, Version};

use crate::{Fault, Record, Value};

/// A `SemanticVersion` record's parts as a version; `None` for any other
/// record.
pub fn version_of(record: &Record) -> Option<Version<'_>> {
    if &*record.ty != SEMANTIC_VERSION {
        return None;
    }
    let number = |name: &str| match record.get(name) {
        Some(Value::Int(n)) => Some(*n),
        _ => None,
    };
    let text = |name: &str| match record.get(name) {
        Some(Value::Text(text)) => Some(Some(&**text)),
        Some(Value::None) => Some(None),
        _ => None,
    };
    Some(Version {
        major: number("major")?,
        minor: number("minor")?,
        patch: number("patch")?,
        pre_release: text("pre_release")?,
        build: text("build")?,
    })
}

/// A `SemanticVersion`'s canonical text.
pub fn version_text(record: &Record) -> Result<String, Fault> {
    version_of(record)
        .map(|version| software::version_text(&version))
        .ok_or(Fault::Shape)
}

/// A `SemanticVersion` read from SemVer 2.0.0's text.
pub fn read_version(raw: &str) -> Option<Value> {
    let version = software::read_version(raw)?;
    let text = |part: Option<&str>| part.map_or(Value::None, Value::text);
    let values = [
        Value::Int(version.major),
        Value::Int(version.minor),
        Value::Int(version.patch),
        text(version.pre_release),
        text(version.build),
    ];
    Some(Value::record(
        SEMANTIC_VERSION,
        SEMANTIC_VERSION_FIELDS
            .iter()
            .zip(values)
            .map(|((name, _), value)| ((*name).into(), value))
            .collect(),
    ))
}

/// Two `SemanticVersion`s by their precedence (SemVer 2.0.0 §11), which
/// `<` gives: build metadata is ignored, so neither of `1.0.0+a` and
/// `1.0.0+b` is below the other. `None` unless both are versions.
pub fn precedence(a: &Record, b: &Record) -> Option<Ordering> {
    Some(software::precedence(&version_of(a)?, &version_of(b)?))
}

/// Two `SemanticVersion`s in the one order (`order::compare`): by
/// precedence, then by build metadata as text, none first, so the order
/// stays total and agrees with `==` while sorting as `<` does.
pub fn canonical_order(a: &Record, b: &Record) -> Option<Ordering> {
    let (x, y) = (version_of(a)?, version_of(b)?);
    Some(software::precedence(&x, &y).then_with(|| x.build.cmp(&y.build)))
}

/// `semver.satisfies(version, range)`.
pub fn satisfies(version: &Value, range: &Value) -> Result<Value, Fault> {
    let (Value::Record(version), Value::Text(range)) = (version, range) else {
        return Err(Fault::Shape);
    };
    let version = version_of(version).ok_or(Fault::Shape)?;
    software::satisfies(&version, range)
        .map(Value::Bool)
        .ok_or(Fault::Shape)
}
