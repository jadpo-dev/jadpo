//! The integration tests of `jadpo-test`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-test --test integration <module>::` runs one.

mod postgres;
