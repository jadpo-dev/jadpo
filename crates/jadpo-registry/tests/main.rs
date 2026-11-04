//! The integration tests of `jadpo-registry`, one binary for the crate:
//! each file beside this one is a module, and `cargo test -p
//! jadpo-registry --test integration <module>::` runs one.
//!
//! `conformance::` tests what the registry application is held to, through
//! `jadpo package protocol`.

mod archive;
mod conformance;
mod local;
mod log;
mod names;
mod protocol;
