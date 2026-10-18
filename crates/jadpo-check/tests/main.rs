//! The integration tests of `jadpo-check`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-check --test integration <module>::` runs one.

// The test helpers in crates/jadpo-syntax/tests/support, loaded once.
#[path = "../../jadpo-syntax/tests/support/examples.rs"]
mod syntax_examples;

mod support;

mod across;
mod ai;
mod bodies;
mod calendar;
mod calendar_events;
mod cards_documents;
mod certificates;
mod client_address;
mod colour_media;
mod conditions;
mod config;
mod constants;
mod crypto;
mod data_formats;
mod declines;
mod decode_problems;
mod determinism;
mod domain_names;
mod examples;
mod failures;
mod fixes;
mod formats;
mod geo_interop;
mod geo_shapes;
mod geodesics;
mod geospatial;
mod graph;
mod habits;
mod identity;
mod imports;
mod incremental;
mod isolation;
mod lists;
mod locales;
mod logic;
mod lookup;
mod maths;
mod money;
mod named_collections;
mod not_values;
mod numbers;
mod one_error;
mod passkeys;
mod people_places;
mod percentages;
mod policy;
mod project;
mod prompts;
mod quantities;
mod randomness;
mod records_measures;
mod review;
mod scale;
mod schedules;
mod secrets;
mod security_records;
mod sets_maps;
mod setting_move;
mod signed;
mod software;
mod soundness;
mod standard_rules;
mod standard_types;
mod stores;
mod suggest;
mod text;
mod text_formats;
mod time;
mod tokens;
mod translation_exchange;
mod translation_failures;
mod translation_jadpo;
mod translation_sources;
mod translations;
mod types;
mod web_network;
