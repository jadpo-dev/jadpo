//! The PostgreSQL driver: [`Postgres`] implements the runtime's
//! [`Database`](jadpo_runtime::Database) with tokio-postgres over the
//! host's [`Transport`](crate::Transport), and [`classify`] reads its
//! errors into [`StoreError`](jadpo_runtime::StoreError).
//!
//! Hyperdrive pools by transaction (§2.6), so the driver keeps no session
//! state outside a transaction: every statement is unnamed
//! (`query_typed`), with each parameter's type taken from its
//! [`SqlValue`](jadpo_runtime::SqlValue), and each transaction sets its
//! own isolation level in its `BEGIN`: read committed for an action that
//! loads no record, repeatable read for every other (area 6).

mod classify;
#[cfg(feature = "postgres")]
mod driver;
#[cfg(any(feature = "postgres", test))]
mod wire;

pub use classify::{Cause, classify};
#[cfg(feature = "postgres")]
pub use driver::Postgres;
