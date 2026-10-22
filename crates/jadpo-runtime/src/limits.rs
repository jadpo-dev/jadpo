//! The runtime's limits (docs/compiler-runtime.md §2.5): constants, the
//! same for every executor and target.

/// The most frames a call stack holds. Recursion is allowed in functions
/// only, and a call past this depth ends in the `recursion_limit` fault
/// rather than a crash (area 3, question 14; area 4, question 9). A
/// thousand frames walk any reasonable recursive value, such as a comment
/// thread, and fit the stacks below: a deployable's threads' natively, and
/// a WebAssembly instance's on Workers (chunk 8.4a).
pub const MAX_DEPTH: usize = 1_000;

/// The most inline functions running at once, one inside another's body
/// through the functions it calls: past it, the operation that would run
/// one more ends in the `recursion_limit` fault too, whichever executor
/// runs it (area 3, "Recursion", 2026-10-22; chunk 8.4a). Compiled code
/// runs an inline function inside its operation, a few of the runtime's
/// frames deep, which on Workers take V8's own stack, about a megabyte
/// that nothing in the module can grow: a recursion through one inline
/// function a level runs out of it after about 650 levels, where a plain
/// one holds 6,800. Four hundred, with a thousand frames beside them,
/// leave a quarter of it spare.
pub const MAX_INLINE_DEPTH: usize = 400;

/// How much stack a thread that runs Jadpo code is given: `jadpo test`'s
/// and `jadpo dev`'s threads, and a native deployable's workers (chunk
/// 8.4a), so a recursion to [`MAX_DEPTH`] through functions of any
/// reasonable size, or through [`MAX_INLINE_DEPTH`] inline functions, ends
/// in its fault rather than exhausting the stack. Only what is used is
/// ever touched, so it costs address space, not memory.
pub const STACK_BYTES: usize = 64 * 1024 * 1024;

/// How much stack a Workers deployable's WebAssembly module is linked with.
pub use jadpo_std::compiled::WASM_STACK_BYTES;

/// How much of a WebAssembly module's own stack a callable must find left
/// to be entered: with less, it is the `recursion_limit` fault, so a
/// recursion through frames too large for [`WASM_STACK_BYTES`] stops
/// there rather than writing past the stack's end (chunk 8.4a).
pub const WASM_STACK_RESERVE: usize = 256 * 1024;

/// The most bytes a value kept as JSON may take (area 5, "Storage types";
/// chunk 5.9b): the 1 MiB a request's body may be (area 7), so whatever a
/// column holds as JSON a request could have sent, and a value read from
/// one past it, written outside Jadpo, is a fault rather than a read of
/// any size.
pub const JSON_KEPT_LIMIT: usize = jadpo_std::http::BODY_LIMIT;

/// The most arrays and objects a value kept as JSON nests (chunk 5.9b):
/// as deep as a request's body is read (`jadpo_value::json::parse`), so a
/// value is never kept that can't be read back.
pub const JSON_KEPT_DEPTH: usize = 128;
