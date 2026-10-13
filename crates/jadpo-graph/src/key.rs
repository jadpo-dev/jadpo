//! Stable identity (area 18, "Stable identity"). A node is known by the
//! name it was born with: its key is its kind's word and its born name,
//! `action save_note`, `type Count`, `field Note.body`, `function
//! rounding.half_even`, the same form the identity registry's lines take
//! (`"entity Task": "Todo"`).
//!
//! A key never holds a position, a file or a domain. Names are unique
//! across the project and across kinds (area 1), so the name alone
//! identifies a declaration, and the moves the layout rules ask for keep
//! it: reordering, moving to the domain's `types/` folder, or promotion to
//! `shared/`, which no command records in the registry.

use std::collections::BTreeMap;
use std::fmt;

use crate::Kind;

/// A node's identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(String);

impl Key {
    /// The key of a node of `kind` born as `born`.
    pub fn new(kind: Kind, born: &str) -> Key {
        Key(format!("{} {born}", kind.word()))
    }

    /// A key as printed, without checking its kind.
    pub fn from_text(text: impl Into<String>) -> Key {
        Key(text.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The kind its first word names.
    pub fn kind(&self) -> Option<Kind> {
        Kind::from_word(self.0.split(' ').next()?)
    }

    /// The born name after the kind's word.
    pub fn name(&self) -> &str {
        self.0.split_once(' ').map_or("", |(_, name)| name)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The identity registry's renames (`database/identities.json`): for each
/// thing renamed since it was born, its name now and its born name.
///
/// Children follow their parent: once `entity Task` is registered as born
/// `Todo`, its field `Task.title` is `field Todo.title` without a line of
/// its own. Only `jadpo rename` and the domain commands write the registry
/// (area 14); until they exist it is empty, and every born name is the
/// name now.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Registry {
    /// By `kind name-now`: the born name.
    renames: BTreeMap<String, String>,
}

impl Registry {
    pub fn new() -> Registry {
        Registry::default()
    }

    /// A registry from its lines, as `database/identities.json` holds them:
    /// `kind name-now` and the born name.
    pub fn from_lines<'a>(lines: impl IntoIterator<Item = (&'a String, &'a String)>) -> Registry {
        Registry {
            renames: lines
                .into_iter()
                .map(|(now, born)| (now.clone(), born.clone()))
                .collect(),
        }
    }

    /// Records that the `kind` called `now` was born `born`.
    pub fn rename(&mut self, kind: Kind, now: &str, born: &str) {
        self.renames
            .insert(format!("{} {now}", kind.word()), born.to_string());
    }

    pub fn is_empty(&self) -> bool {
        self.renames.is_empty()
    }

    /// The born name of the `kind` called `now`; `parent` is the born name
    /// of what it belongs to, for a child whose own name is the part after
    /// its parent's (`Task.title` under a `Task` born `Todo`).
    pub fn born(&self, kind: Kind, now: &str, parent: Option<(&str, &str)>) -> String {
        if let Some(born) = self.renames.get(&format!("{} {now}", kind.word())) {
            return born.clone();
        }
        match parent {
            Some((parent_now, parent_born)) if parent_now != parent_born => {
                match now
                    .strip_prefix(parent_now)
                    .and_then(|r| r.strip_prefix('.'))
                {
                    Some(rest) => format!("{parent_born}.{rest}"),
                    None => now.to_string(),
                }
            }
            _ => now.to_string(),
        }
    }

    /// The key of the `kind` called `now`.
    pub fn key(&self, kind: Kind, now: &str, parent: Option<(&str, &str)>) -> Key {
        Key::new(kind, &self.born(kind, now, parent))
    }
}
