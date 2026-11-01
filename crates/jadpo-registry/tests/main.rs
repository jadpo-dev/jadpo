//! The integration tests of `jadpo-registry`, one binary for the crate:
//! each file beside this one is a module, and `cargo test -p
//! jadpo-registry --test integration <module>::` runs one.
//!
//! `conformance::` is the test the registry application is held to: with
//! `JADPO_REGISTRY_OPENAPI` set to a file or an `http://` address, it also
//! checks that served document against the protocol.

mod archive;
mod conformance;
mod local;
mod log;
mod names;
mod protocol;
