//! The integration tests of `jadpo-test`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-test --test integration <module>::` runs one.

// The test helpers in crates/jadpo-syntax/tests/support, loaded once.
#[path = "../../jadpo-syntax/tests/support/examples.rs"]
mod syntax_examples;

// The certificate authority jadpo-host-native's tests make (chunk 8.1b),
// for a server offering TLS (chunk 8.1n).
#[path = "../../jadpo-host-native/tests/authority.rs"]
#[allow(dead_code)]
mod authority;

mod postgres;
mod postgres_tls;
mod records;

mod ai_accounting;
