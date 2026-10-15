//! The `jadpo` command: a declarative command table that generates help,
//! `jadpo help --json`, Tab completion and suggestions, and writes every
//! result in the CLI design language through `jadpo-term`.

mod api;
mod check;
mod commands;
mod completion;
mod derived;
pub mod dev;
mod fmt;
mod help;
mod migrate;
mod routes;
mod run;
mod sql;
mod suggest;
mod table;
mod test;
pub mod watch;

pub use run::{Output, Session, run, spinner};
