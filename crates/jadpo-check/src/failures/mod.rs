//! Failures (area 4): what each declared failure is, its kind and public
//! code, and the rules on `type X = Failure { ... }` declarations.
//!
//! The rules inside bodies (`reject`, `attempt`, `fails`, outcome `match`)
//! sit with the rest of the body checks in [`crate::types`], which reads
//! the table built here; each callable's inferred failure set is the
//! [`crate::types::failure_sets`] query.

mod decl;
mod table;

pub use decl::failure_problems;
pub use table::{FailureInfo, FailureTable, client_fields, failure_table, provided_failures};

/// The kinds an application may declare (area 4, question 3); the runtime
/// raises the others itself.
pub const DECLARABLE_KINDS: [&str; 7] = [
    "invalid_value",
    "not_found",
    "not_permitted",
    "conflict",
    "rejected",
    "rate_limited",
    "unavailable",
];

/// Codes the runtime answers with itself (area 4, "Faults" and "What the
/// client sees"), so no declared failure may take them.
pub const RUNTIME_CODES: [&str; 8] = [
    "deadline_exceeded",
    "internal_fault",
    "invalid_input",
    "invalid_request",
    "outcome_unknown",
    "read_unavailable",
    "route_not_found",
    "transaction_unavailable",
];

/// The members a failure's block may hold.
pub const MEMBERS: [&str; 5] = ["kind", "code", "message", "client", "internal"];
