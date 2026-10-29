//! The integration tests of `jadpo-lsp`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-lsp --test integration <module>::` runs one.

mod completion;
mod declines;
mod fixes;
mod formatting;
mod incremental;
mod make_setting;
mod query_sql;
mod quick_fixes;
mod references;
mod route_hover;
mod support;
mod translations;
