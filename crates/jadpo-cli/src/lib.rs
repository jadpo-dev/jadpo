//! The `jadpo` command: a declarative command table that generates help,
//! `jadpo help --json`, Tab completion and suggestions, and writes every
//! result in the CLI design language through `jadpo-term`.

mod ai;
mod ai_record;
mod ai_usage;
mod api;
mod calls;
mod check;
mod commands;
mod completion;
pub mod config;
mod declines;
mod deliveries;
mod derived;
pub mod dev;
mod dev_calls;
mod exchange;
mod failure_text;
mod fix;
mod fmt;
mod help;
mod hq;
mod install;
mod local;
mod migrate;
mod package;
mod package_cache;
mod package_import;
mod package_recipe;
mod package_record;
mod provider_review;
mod records;
mod routes;
mod run;
mod runs;
mod schedules;
mod share;
mod sql;
mod suggest;
mod suggestions;
mod table;
mod test;
mod translations;
pub mod watch;

pub use run::{Output, Session, run, spinner};
