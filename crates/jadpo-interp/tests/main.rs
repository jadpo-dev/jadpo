//! The integration tests of `jadpo-interp`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-interp --test integration <module>::` runs one.

// The test helpers in crates/jadpo-syntax/tests/support, loaded once.
#[path = "../../jadpo-syntax/tests/support/examples.rs"]
mod syntax_examples;

mod support;

mod assertions;
mod boundary;
mod calendar;
mod conditions;
mod constants;
mod data_formats;
mod decode_problems;
mod domain_names;
mod effects;
mod failures;
mod lists;
mod logic;
mod maths;
mod money;
mod named_collections;
mod numbers;
mod proofs;
mod quantities;
mod reports;
mod schedules;
mod sets_maps;
mod soundness;
mod text;
mod values;
