//! The flags table (area 16, "Feature flags"; chunk B9): plain data the
//! plan decides, so both executors read a flag through the runtime's one
//! path, which works its value out for the caller from the state kept in
//! `jadpo_flags` ([`crate::system::FLAGS`]).

use serde::{Deserialize, Serialize};

/// Every flag `flags { }` declares, and where their state is kept.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlagsPlan {
    /// In name order; a flag's number is its place here.
    pub flags: Vec<FlagPlan>,
    /// The store `jadpo_flags` is in: `flags: { store: }`, or the only
    /// database. `None` when no flag is declared.
    pub store: Option<String>,
}

/// One flag.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlagPlan {
    /// Its name in `flags { }` and in code, `flags.<name>`.
    pub name: String,
    /// An enum flag's type, as its values carry it; `None` for a `Bool`.
    pub enum_type: Option<String>,
    /// Its values in order, as `jadpo_flags` spells them: `false` and
    /// `true` for a `Bool`, an enum's variants by name in the order it
    /// declares them, so a value's place is its variant's ordinal.
    pub values: Vec<String>,
    /// Its `default:`, as a place in `values`.
    pub default: u32,
    /// `client: true`: also answered, worked out for the caller, by each
    /// API's generated `GET /flags` (chunk B10).
    #[serde(default)]
    pub client: bool,
}

/// A `Bool` flag's values, in their order.
pub const BOOL_VALUES: [&str; 2] = ["false", "true"];

impl FlagsPlan {
    /// The flag called `name`, with its number.
    pub fn flag(&self, name: &str) -> Option<(u32, &FlagPlan)> {
        self.flags
            .iter()
            .enumerate()
            .find(|(_, flag)| flag.name == name)
            .and_then(|(at, flag)| Some((u32::try_from(at).ok()?, flag)))
    }
}

impl FlagPlan {
    /// The place of value `text` among its values: `None` for text that
    /// names none, such as a variant since removed.
    pub fn value(&self, text: &str) -> Option<u32> {
        self.values
            .iter()
            .position(|value| value == text)
            .and_then(|at| u32::try_from(at).ok())
    }
}
