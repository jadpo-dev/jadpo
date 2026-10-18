//! The integration tests of `jadpo-runtime`, one binary for the crate:
//! each file beside this one is a module, and `cargo test -p jadpo-runtime
//! --test integration <module>::` runs one.

mod calendar;
mod colour;
mod formats;
mod geospatial;
mod lists;
mod maths;
mod media;
mod money;
mod numbers;
mod percentages;
mod prices;
mod quantities;
mod records_measures;
mod schedules;
mod sets_maps;
mod software;
mod text;
mod text_formats;
mod web_network;
