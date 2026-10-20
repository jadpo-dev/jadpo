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
//! - [`package`]: the package driver, behind the `package` feature: JSON
//!   over HTTP through the [`package::Fetch`] the host gives (chunk 7.2c).
//! - [`smtp`]: the SMTP driver, behind the `smtp` feature: a rendered
//!   email written as one MIME message and handed over through the
//!   [`smtp::Relay`] the host gives (chunk 7.3).
//!
//! Built so far: PostgreSQL (chunk 3.1), the package driver (chunk 7.2c)
//! and SMTP (chunk 7.3). Redis and S3 come with their areas.

// A panic aborts a Workers instance, so runtime code reports faults as
// values (docs/compiler-runtime.md §2.6); tests may still unwrap.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::unreachable,
        clippy::unimplemented
    )
)]

#[cfg(feature = "package")]
pub mod package;
pub mod postgres;
#[cfg(feature = "smtp")]
pub mod smtp;
mod transport;

pub use transport::{Endpoint, Stream, Transport};
