//! The integration tests of `jadpo-term`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-term --test integration <module>::` runs one.

mod detect;
mod json;
mod report;
mod screens;
