//! What policy (area 9) leaves the runtime to do, as plain data both
//! executors read: the plan compiles every grant into the statements it
//! runs (`jadpo-plan`), so the runtime only binds who is calling, and
//! raises what a statement policy refused means.
//!
//! - A statement's caller parameters ([`CallerPart`]): the signed-in
//!   user's or service's `id`, `NULL` for anyone else, whether anyone
//!   is signed in, and whether the system runs it (`Access.system`:
//!   schedules and subscribers, area 12) and in which tenant.
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
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum CallerPart {
    /// The signed-in user's `id`; `NULL` for a service or no one.
    User,
    /// The signed-in service's `id`; `NULL` for a user or no one.
    Service,
    /// Whether anyone is signed in: `Access.authenticated`.
    Signed,
    /// Whether the system runs the statement: `Access.system`, the caller
    /// of a schedule's run and a subscriber's delivery (area 12), which no
    /// request ever is (chunk 6.1a).
    System,
    /// The tenant the system is limited to, the one a subscriber's event
    /// came from; `NULL` for a schedule's run, which every tenant's
    /// records it is granted are open to, and for every other caller.
    SystemTenant,
}

impl CallerPart {
    /// The type it is sent as.
    pub fn ty(self) -> SqlType {
        match self {
            CallerPart::User | CallerPart::Service | CallerPart::SystemTenant => SqlType::Uuid,
            CallerPart::Signed | CallerPart::System => SqlType::Bool,
        }
    }
}

/// Who may call an action that reaches no entity (area 9, "Actions"):
/// checked as it starts, wherever it is called from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Gate {
    /// Which signed-in callers its line lets in.
    pub callers: Callers,
    /// Whether its line names `Access.system`, so a schedule's run or a
    /// subscriber's delivery may call it (area 12; chunk 6.1a). Nothing a
    /// request runs is the system.
    pub system: bool,
}

impl Gate {
    /// A line's callers, without the system.
    pub fn of(callers: Callers) -> Gate {
        Gate {
            callers,
            system: false,
        }
    }
}

/// The callers a [`Gate`] lets in besides the system.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Callers {
    /// Anyone signed in.
    Signed,
    /// A caller holding one of the roles its line names: `SELECT 1` of
    /// their membership, in the store memberships live in.
    Roles {
        store: String,
        sql: String,
        params: Vec<Param>,
    },
    /// Nobody a request signs in as: the line names only schedules and
    /// subscribers (`Access.system`, area 12), or callers that only a
    /// record the action never writes could name.
    Nobody,
}

/// A statement about a principal's credentials that the owning record's
/// grants limit (area 9, question 8): listing a user's tokens or a
/// service's keys needs `read` on its record, and revoking one or issuing
/// a key `update`. Its first parameters are the runtime's own, as the
/// statement without policy takes them; the caller's follow, in order.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Gated {
    pub sql: String,
    /// The parameters after the runtime's own: the caller's parts, and
    /// the time now when the holder's lifecycle reads `clock.now`.
    pub extra: Vec<Param>,
}
