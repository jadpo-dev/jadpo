//! The integration tests of `jadpo-fmt`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-fmt --test integration <module>::` runs one.

// The test helpers in crates/jadpo-syntax/tests/support, loaded once.
#[path = "../../jadpo-syntax/tests/support/examples.rs"]
mod syntax_examples;
#[path = "../../jadpo-syntax/tests/support/mutate.rs"]
mod syntax_mutate;

mod examples;
mod format;
mod fuzz;
mod sections;
