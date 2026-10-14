//! The runtime's capability drivers (docs/compiler-runtime.md §2.1 and
//! §2.6): each implements a capability trait of `jadpo-runtime` over a
//! [`Transport`] the host supplies, and classifies its own errors into the
//! runtime's fixed set, so executors and targets agree on what happened.
//!
//! - [`Transport`], [`Stream`] and [`Endpoint`]: how a driver reaches a
//!   server, which the host decides (tokio natively, `connect()` sockets on
//!   Workers).
//! - [`postgres`]: the PostgreSQL driver, behind the `postgres` feature,
//!   and its error classification.
//!
//! Built so far: PostgreSQL (chunk 3.1). Redis, SMTP, S3 and the package
//! driver come with their areas.

// A panic aborts a Workers instance, so runtime code reports faults as
// values (docs/compiler-runtime.md §2.6); tests may still unwrap.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod postgres;
mod transport;

pub use transport::{Endpoint, Stream, Transport};
