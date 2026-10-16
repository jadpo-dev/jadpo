//! The integration tests of `jadpo-cli`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-cli --test integration <module>::` runs one.

mod support;

mod binary;
mod check;
mod cli;
mod crate_graph;
mod fmt;
mod http;
mod lifecycle;
mod lsp;
mod manifest;
mod migrate;
mod policy;
mod queries;
mod round_trips;
mod sql;
#[path = "test.rs"]
mod test_command;
mod watch;
mod writes;
