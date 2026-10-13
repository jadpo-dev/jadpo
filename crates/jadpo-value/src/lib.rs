//! Runtime values and the standard library's operations
//! (docs/compiler-runtime.md §2.1 and §2.6).
//!
//! Every standard-library operation is declared once, as a Jadpo stub in
//! `jadpo-check`, and implemented once, here, as one Rust function that both
//! the interpreter and compiled code call. [`BINDINGS`] lists the
//! operations by their qualified names; a test in `jadpo-check` holds the
//! stubs and this list to each other.
//!
//! This crate builds for `wasm32-unknown-unknown` and depends on no other
//! workspace crate.

pub mod collection;
mod value;

pub use value::{Decimal, Value};

/// Every standard-library operation implemented here, as
/// `namespace.operation`, sorted.
pub const BINDINGS: &[&str] = &[
    "collection.contains",
    "collection.count",
    "collection.filter",
    "collection.map",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_are_sorted_and_unique() {
        let mut sorted = BINDINGS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, BINDINGS);
    }
}
