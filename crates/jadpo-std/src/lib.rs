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
//! - [`Rounding`], [`TextFormat`], [`UuidVersion`] and [`Weekday`], and
//!   the `Zone` and `Locale` tables ([`ZONES`], [`LOCALES`]): the standard
//!   library's enums that Rust code interprets, declared once.
//! - [`OPERATIONS`]: every standard-library operation with its parameters'
//!   kinds, its result and the Rust function that runs it in
//!   `jadpo-value`, which the Jadpo stubs in `jadpo-check` are tested
//!   against.
//!
//! It depends on no other workspace crate, so compiler and runtime crates
//! may both depend on it. Its one dependency is `regex`, the linear-time
//! engine `pattern:` needs on every target (area 2, question 9).

// A panic aborts a Workers instance, so runtime code reports faults as
// values (docs/compiler-runtime.md §2.6); tests may still unwrap.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod builtin;
mod enums;
mod num;
mod operations;
mod rules;
mod time;
mod zones;

pub use builtin::Builtin;
pub use enums::{LOCALE, Rounding, TextFormat, UuidVersion, Weekday, ZONE, enums};
pub use num::{Num, compare_exact};
pub use operations::{Kind, OPERATIONS, Op, Operation, Param, operation};
pub use rules::{
    Broken, Pattern, Rules, check_builtin_text, compile_pattern, is_email, is_ip_address, is_url,
    uuid_version,
};
pub use time::{
    Dur, check_calendar_date, check_instant, date_number, format_date, format_instant, format_time,
    instant_millis, time_millis,
};
pub use zones::{LOCALES, ZONES};
