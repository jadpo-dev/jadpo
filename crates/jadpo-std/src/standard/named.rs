//! The standard library's own data a plan points at rather than holds
//! (a standard type's name, a standard enum's codes), written in the
//! canonical byte encoding by the standard type's name and read back as
//! the standard library's own (chunk 8.1a), so a decoded plan points at the
//! same tables the built one did. Each module is a serde `with` module.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::StandardType;

/// A standard type's name, as the standard library's table holds it. A
/// field of this type, rather than `&'static str` written out, keeps
/// serde from reading it as text borrowed from the bytes, which a plan
/// decoded once and kept can't be.
pub type Name = &'static str;

/// The standard type called `name`, as the table holds it.
fn entry<E: serde::de::Error>(name: &str) -> Result<&'static StandardType, E> {
    crate::standard_type(name)
        .ok_or_else(|| E::custom(format!("no standard type is called `{name}`")))
}

/// A standard type's name.
pub mod name {
    use super::*;

    pub fn serialize<S: Serializer>(name: &&'static str, s: S) -> Result<S::Ok, S::Error> {
        name.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<&'static str, D::Error> {
        let name = String::deserialize(d)?;
        Ok(entry::<D::Error>(&name)?.name)
    }
}

/// A standard type's name, or none.
pub mod optional {
    use super::*;

    pub fn serialize<S: Serializer>(name: &Option<&'static str>, s: S) -> Result<S::Ok, S::Error> {
        name.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<&'static str>, D::Error> {
        match Option::<String>::deserialize(d)? {
            Some(name) => Ok(Some(entry::<D::Error>(&name)?.name)),
            None => Ok(None),
        }
    }
}

/// A standard enum's codes, by the enum's name.
pub mod codes {
    use super::*;

    type Codes = &'static [(&'static str, &'static str)];

    pub fn serialize<S: Serializer>(codes: &Codes, s: S) -> Result<S::Ok, S::Error> {
        let owner = crate::STANDARD_TYPES.iter().find(|entry| {
            entry
                .codes()
                .is_some_and(|held| std::ptr::eq(held, *codes) || held == *codes)
        });
        match owner {
            Some(entry) => entry.name.serialize(s),
            None => Err(serde::ser::Error::custom("codes no standard enum holds")),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Codes, D::Error> {
        let name = String::deserialize(d)?;
        entry::<D::Error>(&name)?
            .codes()
            .ok_or_else(|| D::Error::custom(format!("`{name}` is no standard enum")))
    }
}
