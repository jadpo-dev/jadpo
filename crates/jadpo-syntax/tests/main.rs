//! The integration tests of `jadpo-syntax`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-syntax --test integration <module>::` runs one.

// The test helpers in crates/jadpo-syntax/tests/support, loaded once.
#[path = "support/examples.rs"]
mod syntax_examples;
#[path = "support/mutate.rs"]
mod syntax_mutate;

mod corpus;
mod docs;
mod errors;
mod fuzz;
mod sourcegen;
mod tree;
