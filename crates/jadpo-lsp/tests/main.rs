//! The integration tests of `jadpo-lsp`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-lsp --test integration <module>::` runs one.

mod declines;
mod incremental;
mod make_setting;
mod translations;
