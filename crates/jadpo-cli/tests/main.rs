//! The integration tests of `jadpo-cli`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-cli --test integration <module>::` runs one.

mod support;

mod accept_language;
mod binary;
mod check;
mod cli;
mod client_address;
mod config;
mod crate_graph;
mod declines;
mod decode_problems;
mod fix;
mod fmt;
mod http;
mod lifecycle;
mod locales;
mod lsp;
mod manifest;
mod migrate;
mod passkeys;
mod policy;
mod queries;
mod round_trips;
mod signed;
mod sql;
#[path = "test.rs"]
mod test_command;
mod translation_exchange;
mod translation_jadpo;
mod translation_tooling;
mod translations;
mod watch;
mod writes;
