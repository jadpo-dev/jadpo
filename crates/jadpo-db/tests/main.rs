//! The integration tests of `jadpo-db`, one binary for the crate (fewer
//! binaries to link): each file beside this one is a module, and
//! `cargo test -p jadpo-db --test integration <module>::` runs one.

mod load;
