//! The runtime's limits (docs/compiler-runtime.md §2.5): constants, the
//! same for every executor and target.

/// The most frames a call stack holds. Recursion is allowed in functions
/// only, and a call past this depth ends in the `recursion_limit` fault
/// rather than a crash (area 3, question 14; area 4, question 9). A
/// thousand frames walk any reasonable recursive value, such as a comment
/// thread, and fit the stack of a WebAssembly instance on Workers.
pub const MAX_DEPTH: usize = 1_000;
