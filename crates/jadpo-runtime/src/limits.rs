//! The runtime's limits (docs/compiler-runtime.md §2.5): constants, the
//! same for every executor and target.

/// The most frames a call stack holds. Recursion is allowed in functions
/// only, and a call past this depth ends in the `recursion_limit` fault
/// rather than a crash (area 3, question 14; area 4, question 9). A
/// thousand frames walk any reasonable recursive value, such as a comment
/// thread, and fit the stack of a WebAssembly instance on Workers.
pub const MAX_DEPTH: usize = 1_000;

/// The most bytes a value kept as JSON may take (area 5, "Storage types";
/// chunk 5.9b): the 1 MiB a request's body may be (area 7), so whatever a
/// column holds as JSON a request could have sent, and a value read from
/// one past it, written outside Jadpo, is a fault rather than a read of
/// any size.
pub const JSON_KEPT_LIMIT: usize = jadpo_std::http::BODY_LIMIT;

/// The most arrays and objects a value kept as JSON nests (chunk 5.9b):
/// as deep as a request's body is read (the JSON reader stops past 127),
/// so a value is never kept that can't be read back.
pub const JSON_KEPT_DEPTH: usize = 127;
