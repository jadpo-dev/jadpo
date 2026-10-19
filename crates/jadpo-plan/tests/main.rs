//! The integration tests of `jadpo-plan`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-plan --test integration <module>::` runs one.

mod plan;
mod sql;
mod sql_text;
