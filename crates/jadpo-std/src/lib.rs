//! The standard library's facts, shared by the compiler and the runtime
//! (docs/compiler-runtime.md §2.1): what each is decided once, here, so
//! `jadpo-check` checks a literal and `jadpo-value` checks a runtime value
//! with the same code.
//!
//! - [`Builtin`]: the types built into the language, which no Jadpo
//!   declaration can say what they are made of.
//! - [`Num`]: an exact number as written, for literals and rules.
//! - [`Rules`]: the rules a valid value follows (area 2), and the
//!   validators that check text, numbers and lengths against them and
//!   against the validated built-in types (email, URL, UUID, IP address).
//! - [`Rounding`], [`TextFormat`] and [`UuidVersion`]: the standard
//!   library's enums that Rust code interprets, declared once.
//! - [`OPERATIONS`]: every standard-library operation with its parameters'
//!   kinds, its result and the Rust function that runs it in
//!   `jadpo-value`, which the Jadpo stubs in `jadpo-check` are tested
//!   against.
//!
//! It depends on no other workspace crate, so compiler and runtime crates
//! may both depend on it. Its one dependency is `regex`, the linear-time
//! engine `pattern:` needs on every target (area 2, question 9).

mod builtin;
mod enums;
mod num;
mod operations;
mod rules;

pub use builtin::Builtin;
pub use enums::{ENUMS, Rounding, TextFormat, UuidVersion};
pub use num::Num;
pub use operations::{Kind, OPERATIONS, Op, Operation, Param, operation};
pub use rules::{
    Broken, Rules, check_builtin_text, compile_pattern, is_email, is_ip_address, is_url,
    uuid_version,
};
