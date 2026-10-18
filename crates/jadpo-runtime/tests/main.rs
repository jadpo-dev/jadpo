//! The integration tests of `jadpo-runtime`, one binary for the crate:
//! each file beside this one is a module, and `cargo test -p jadpo-runtime
//! --test integration <module>::` runs one.

mod calendar;
mod formats;
mod lists;
mod maths;
mod money;
mod numbers;
mod percentages;
mod quantities;
mod schedules;
mod sets_maps;
mod standard_types;
mod text;
mod web_network;
