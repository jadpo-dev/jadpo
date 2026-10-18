//! The standard library's facts, shared by the compiler and the runtime
//! (docs/compiler-runtime.md §2.1): what each is decided once, here, so
//! `jadpo-check` checks a literal and `jadpo-value` checks a runtime value
//! with the same code.
//!
//! - [`Builtin`]: the types built into the language, which no Jadpo
//!   declaration can say what they are made of.
//! - [`Num`]: an exact number as written, for literals and rules, and
//!   [`Literal`], a literal as written.
//! - [`Rules`]: the rules a valid value follows (area 2), and the
//!   validators that check text, numbers and lengths against them and
//!   against the validated built-in types (email, URL, UUID, IP address).
//! - [`Rounding`], [`TextFormat`], [`UuidVersion`] and [`Weekday`], and
//!   the `Zone` and `Locale` tables ([`ZONES`], [`LOCALES`]): the standard
//!   library's enums that Rust code interprets, declared once.
//! - [`TimeFormatSpec`]: a time format as `text.format` is given it,
//!   built in or declared from [`Component`]s and their
//!   [`ComponentStyle`]s (area 10).
//! - [`types`]: type plans, what a value of each type is made of and what
//!   a declared failure holds, and [`Constant`], a value known while
//!   compiling, as plain data `jadpo-plan` decides and the runtime reads.
//! - [`read`] and [`write`]: what a read and a write run, SQL and codecs,
//!   and the failures they raise when nothing matches or the store refuses,
//!   decided by `jadpo-plan` and run by `jadpo-runtime` for both executors
//!   (area 6), the [`Isolation`] an action's transaction runs at, and
//!   each store's share of a [`Migration`].
//! - [`standard`]: every standard type's data entry (area 19), which the
//!   contract's one implementation of each baseline operation reads.
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
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::unreachable,
        clippy::unimplemented
    )
)]

mod builtin;
pub mod config;
mod constant;
mod enums;
mod format;
pub mod http;
mod isolation;
pub mod jadpo_text;
mod literal;
mod locale;
mod migration;
pub mod money;
mod num;
mod operations;
pub mod policy;
pub mod read;
mod rules;
pub mod standard;
mod time;
pub mod translation;
pub mod types;
pub mod write;
mod zones;

pub use builtin::{Builtin, SIGNING_KEY_MIN};
pub use constant::Constant;
pub use enums::{
    Access, AuthenticationMethod, AuthenticationStrength, ClientAddressSource, CodeForm,
    ComponentStyle, Driver, Dst, FieldAccess, FieldChange, Generated, HttpMethod, Idempotency,
    InvalidDay, LOCALE, MembershipScope, MoneyDisplay, MoneyPrecision, MoneySign, Month, OnDelete,
    Rounding, RouteAuthentication, SuccessStatus, Tenancy, TextDirection, TextFormat,
    TranslationReview, UuidVersion, Weekday, ZONE, enums, variant_ordinal,
};
pub use format::{BuiltinFormat, Component, FormatLength, FormatPart, TimeFormatSpec};
pub use isolation::Isolation;
pub use literal::Literal;
pub use locale::{LocalePlan, locale_ordinal};
pub use migration::Migration;
pub use money::{CURRENCIES, CURRENCY, MONEY_FIELDS};
pub use num::{Num, compare_exact};
pub use operations::{
    Condition, Kind, MOVED, MOVED_NAMESPACES, OPERATIONS, Op, Operation, Param, moved,
    moved_namespace, operation,
};
pub use read::SqlType;
pub use rules::{
    Broken, Pattern, RuleKey, Rules, check_builtin_text, compile_pattern, is_email, is_ip_address,
    is_url, uuid_version,
};
pub use standard::{
    CALENDAR_PERIOD, DATE_RANGE, FILE_TYPE, FILE_TYPES, INSTANT_RANGE, INSTANT_RANGE_FIELDS, Part,
    RANGE, RANGE_FIELDS, RANGE_ITEMS, STANDARD_TYPES, StandardType, range_type, standard_type,
};
pub use time::{
    DATE_MAX, DATE_MIN, Dur, INSTANT_MAX, INSTANT_MIN, UnitSuffix, check_calendar_date,
    check_instant, civil_date, clock_millis, date_number, days_in_month, epoch_day,
    format_clock_time, format_date, format_instant, instant_millis, iso_duration, unit_suffixes,
};
pub use zones::{LOCALES, ZONES};
