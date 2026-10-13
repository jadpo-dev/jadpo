//! The `jadpo` command: a declarative command table that generates help,
//! `jadpo help --json`, Tab completion and suggestions, and writes every
//! result in the CLI design language through `jadpo-term`.

mod check;
mod commands;
mod completion;
mod fmt;
mod help;
mod run;
mod suggest;
mod table;

pub use run::{Output, run};
