//! The integration tests of `jadpo-check`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-check --test integration <module>::` runs one.

// The test helpers in crates/jadpo-syntax/tests/support, loaded once.
#[path = "../../jadpo-syntax/tests/support/examples.rs"]
mod syntax_examples;

mod support;

mod bodies;
mod client_address;
mod conditions;
mod config;
mod determinism;
mod examples;
mod failures;
mod graph;
mod habits;
mod identity;
mod imports;
mod incremental;
mod isolation;
mod logic;
mod lookup;
mod named_collections;
mod one_error;
mod policy;
mod project;
mod review;
mod scale;
mod signed;
mod soundness;
mod stores;
mod types;
