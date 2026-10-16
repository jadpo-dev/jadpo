//! What policy (area 9) leaves the runtime to do, as plain data both
//! executors read: the plan compiles every grant into the statements it
//! runs (`jadpo-plan`), so the runtime only binds who is calling, and
//! raises what a statement policy refused means.
//!
//! - A statement's caller parameters ([`CallerPart`]): the signed-in
//!   user's or service's `id`, `NULL` for anyone else, and whether anyone
//!   is signed in.
//! - Each statement policy limits also comes without its policy
//!   (`unpoliced`), which a call a test makes without a caller runs: what
//!   it proves is behaviour and data, and the route test proves policy
//!   (area 13). Nothing else runs it. A read of exactly one record instead
//!   selects its grant as a column (`read::Select::permitted`), so one
//!   statement serves both, and tells a record policy hid from one that
//!   isn't there.
//! - A [`Gate`]: who may call an action that reaches no entity, from its
//!   line under root `policy { actions: }`.

use crate::read::{Param, SqlType};

/// Which part of the caller a statement's parameter is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CallerPart {
    /// The signed-in user's `id`; `NULL` for a service or no one.
    User,
    /// The signed-in service's `id`; `NULL` for a user or no one.
    Service,
    /// Whether anyone is signed in: `Access.authenticated`.
    Signed,
}

impl CallerPart {
    /// The type it is sent as.
    pub fn ty(self) -> SqlType {
        match self {
            CallerPart::User | CallerPart::Service => SqlType::Uuid,
            CallerPart::Signed => SqlType::Bool,
        }
    }
}

/// Who may call an action that reaches no entity (area 9, "Actions"):
/// checked as it starts, wherever it is called from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Gate {
    /// Anyone signed in.
    Signed,
    /// A caller holding one of the roles its line names: `SELECT 1` of
    /// their membership, in the store memberships live in.
    Roles {
        store: String,
        sql: String,
        params: Vec<Param>,
    },
    /// Nobody: the line names only schedules and subscribers
    /// (`Access.system`, area 12), which no request is.
    Nobody,
}

/// A statement about a principal's credentials that the owning record's
/// grants limit (area 9, question 8): listing a user's tokens or a
/// service's keys needs `read` on its record, and revoking one or issuing
/// a key `update`. Its first parameters are the runtime's own, as the
/// statement without policy takes them; the caller's follow, in order.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Gated {
    pub sql: String,
    /// The parameters after the runtime's own: the caller's parts, and
    /// the time now when the holder's lifecycle reads `clock.now`.
    pub extra: Vec<Param>,
}
