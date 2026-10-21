//! The integration tests of `jadpo-plan`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-plan --test integration <module>::` runs one.

// The test helpers in crates/jadpo-syntax/tests/support, loaded once.
#[path = "../../jadpo-syntax/tests/support/examples.rs"]
mod syntax_examples;

mod call_sites;
mod plan;
mod runtime_plan;
mod sql;
mod sql_text;
