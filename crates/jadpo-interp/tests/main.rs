//! The integration tests of `jadpo-interp`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-interp --test integration <module>::` runs one.

// The test helpers in crates/jadpo-syntax/tests/support, loaded once.
#[path = "../../jadpo-syntax/tests/support/examples.rs"]
mod syntax_examples;

mod support;

mod across;
mod ai;
mod assertions;
mod boundary;
mod calendar;
mod certificates;
mod colour;
mod conditions;
mod constants;
mod crypto;
mod data_formats;
mod decode_problems;
mod domain_names;
mod effects;
mod failures;
mod geospatial;
mod lists;
mod logic;
mod maths;
mod media;
mod money;
mod named_collections;
mod numbers;
mod percentages;
mod proofs;
mod quantities;
mod randomness;
mod records_measures;
mod reports;
mod schedules;
mod secrets;
mod security_records;
mod sets_maps;
mod software;
mod soundness;
mod standard_rules;
mod standard_types;
mod text;
mod text_formats;
mod tokens;
mod values;
mod web_network;
