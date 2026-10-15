//! The integration tests of `jadpo-graph`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-graph --test integration <module>::` runs one.

mod model;
