//! The runtime's capability drivers (docs/compiler-runtime.md §2.1 and
//! §2.6): each implements a capability trait of `jadpo-runtime` over a
//! [`Transport`] the host supplies, and classifies its own errors into the
//! runtime's fixed set, so executors and targets agree on what happened.
//!
//! - [`Transport`], [`Stream`] and [`Endpoint`]: how a driver reaches a
//!   server and secures the connection, which the host decides (tokio and
//!   rustls natively, `connect()` sockets on Workers).
//! - [`postgres`]: the PostgreSQL driver, behind the `postgres` feature,
//!   and its error classification.
//!
//! - [`package`]: the package driver, behind the `package` feature: JSON
//!   over HTTP through the [`package::Fetch`] the host gives (chunk 7.2c).
//! - [`smtp`]: the SMTP driver, behind the `smtp` feature: a rendered
//!   email written as one MIME message and handed to its server in an
//!   SMTP conversation over the host's [`Transport`] (chunks 7.3 and
//!   8.1b).
//!
//! - [`s3`]: the S3 driver, behind the `package` feature: the `files`
//!   capability over the same [`package::Fetch`], signed with AWS
//!   Signature Version 4, natively and on Workers.
//! - [`webhook`]: the webhook driver, behind the `package` feature: the
//!   `publishing` capability, each chosen event posted to one address over
//!   the same [`package::Fetch`], signed as Standard Webhooks signs.
//! - [`redis`]: the Redis driver, behind the `redis` feature: RESP over
//!   the host's [`Transport`], one connection carrying every caller's
//!   commands, and the rate limits counted in it (chunk Q8).
//! - [`factory`]: each connection's driver built from the plan's
//!   connections table and what the program started with, over what the
//!   host supplies (chunk 8.1d).
//!
//! Built so far: PostgreSQL (chunk 3.1), the package driver (chunk 7.2c),
//! SMTP (chunk 7.3), the factory (chunk 8.1d), S3 and Redis (chunk Q8).

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

pub mod factory;
#[cfg(feature = "package")]
pub mod package;
pub mod postgres;
#[cfg(feature = "redis")]
pub mod redis;
#[cfg(feature = "package")]
pub mod s3;
#[cfg(feature = "smtp")]
pub mod smtp;
mod transport;
#[cfg(feature = "package")]
pub mod webhook;

pub use transport::{Endpoint, Stream, Transport, Trust, within};
