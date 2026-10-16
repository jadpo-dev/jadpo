//! Every standard-library operation, typed (docs/compiler-runtime.md §2.1):
//! its parameters with their kinds, its result, and the one Rust function in
//! `jadpo-value` that runs it for both executors. The Jadpo stubs in
//! `jadpo-check` declare the same operations for programs to call, and a
//! test holds the two to the same arity and kinds; `jadpo-value` matches on
//! [`Op`] exhaustively, so an operation listed here without an
//! implementation does not compile.

use crate::{Builtin, Dst, InvalidDay, Rounding, RuleKey, Weekday};

/// What a parameter or a result is, as far as an operation cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A type variable (`T`): any type, fixed by the arguments at each call.
    Any,
    /// `List<T>`.
    List,
    /// An inline function, `item -> expression` (area 3).
    Function,
    /// A value built on `Int` or `Decimal`.
    Number,
    /// A value built on this built-in type.
    Builtin(Builtin),
    /// A value of a standard-library enum, by its name.
    Enum(&'static str),
    /// A value built on one of these built-in types: `T` in its stub,
    /// fixed by the argument, so a result of the same kind keeps the
    /// argument's named type (area 10, question 6).
    OneOf(&'static [Builtin]),
    /// A standard-library type the checker knows by name, such as
    /// `InstantRange`, whose fields the runtime builds as a record.
    Std(&'static str),
    /// A clock time written as text, `"09:30"`, checked while compiling
    /// (area 10, "Zones and local times"): there is no time-of-day type,
    /// so only a literal gives one.
    Clock,
    /// A time format written out (area 10, "Showing dates and times to
    /// people"): `TimeFormat.date_long` or a declared format's name, which
    /// the checker reads into a [`crate::TimeFormatSpec`] and the operation
    /// is given as its text.
    Format,
}

/// One parameter: its name, which a call may give it by (area 3), and its
/// kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Param {
    pub name: &'static str,
    pub kind: Kind,
    /// A divisor: the checker rules zero out while compiling, as it does
    /// for `/` (area 3, question 7), so the operation never divides by it.
    pub never_zero: bool,
    /// An option a call may leave out: its stub writes it `T?`, and the
    /// operation is given `none` for it.
    pub optional: bool,
    /// An option that means something only when the call's first argument
    /// is built on this type, as `dst:` only for a `ZonedTime`.
    pub only_for: Option<Builtin>,
    /// An option that a call must give whenever it means something, as
    /// `zone:` for an `Instant` being formatted.
    pub needed: bool,
}

/// A condition an operation fails on unless an option settles it (area
/// 10, question 7): the call's `attempt` maps it by `key`, as a
/// construction's broken rule is mapped (area 2). It applies while the
/// option named `option` applies and is left out, or given as anything
/// but a written value other than `reject`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Condition {
    pub key: RuleKey,
    pub option: &'static str,
}

/// Which operation, for an exhaustive match in each executor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Op {
    CollectionContains,
    CollectionCount,
    CollectionFilter,
    CollectionMap,
    DecimalDivide,
    IntegerDivide,
    TemporalAddDays,
    TemporalAddMonths,
    TemporalAddYears,
    TemporalBetween,
    TemporalDate,
    TemporalDay,
    TemporalDayBounds,
    TemporalFormat,
    TemporalFormatRelative,
    TemporalHour,
    TemporalInZone,
    TemporalInstant,
    TemporalMinute,
    TemporalMonth,
    TemporalMonthBounds,
    TemporalResolve,
    TemporalWeekBounds,
    TemporalWeekday,
    TemporalYear,
    TemporalYearBounds,
    TemporalZone,
}

/// One standard-library operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operation {
    pub op: Op,
    pub namespace: &'static str,
    pub name: &'static str,
    pub params: &'static [Param],
    pub result: Kind,
    /// The smallest result it can give, when its result type can't say:
    /// `collection.count` never gives less than 0, though its result is a
    /// plain `Int`. Proving a construction safe reads it (area 4,
    /// question 6).
    pub at_least: Option<i64>,
    /// The conditions it can fail on, each settled by an option.
    pub conditions: &'static [Condition],
    /// The Rust function that runs it, by its path from `jadpo-value`'s
    /// root: what generated code calls.
    pub rust: &'static str,
}

impl Operation {
    /// `namespace.name`.
    pub fn qualified(&self) -> String {
        format!("{}.{}", self.namespace, self.name)
    }
}

const fn param(name: &'static str, kind: Kind) -> Param {
    Param {
        name,
        kind,
        never_zero: false,
        optional: false,
        only_for: None,
        needed: false,
    }
}

/// A parameter whose value is never zero.
const fn divisor(name: &'static str, kind: Kind) -> Param {
    Param {
        never_zero: true,
        ..param(name, kind)
    }
}

/// An option a call may leave out.
const fn option(name: &'static str, kind: Kind) -> Param {
    Param {
        optional: true,
        ..param(name, kind)
    }
}

/// An option that means something only for a first argument built on
/// `builtin`.
const fn option_only_for(name: &'static str, kind: Kind, builtin: Builtin) -> Param {
    Param {
        only_for: Some(builtin),
        ..option(name, kind)
    }
}

/// An option given exactly when the call's first argument is built on
/// `builtin`.
const fn option_needed_for(name: &'static str, kind: Kind, builtin: Builtin) -> Param {
    Param {
        needed: true,
        ..option_only_for(name, kind, builtin)
    }
}

const ROUNDING: Kind = Kind::Enum(Rounding::NAME);

/// What `temporal`'s calendar operations work on: a date, or a local time
/// in its zone.
const DATED: Kind = Kind::OneOf(&[Builtin::CalendarDate, Builtin::ZonedTime]);
const INSTANT: Kind = Kind::Builtin(Builtin::Instant);
const DATE: Kind = Kind::Builtin(Builtin::CalendarDate);
const ZONED: Kind = Kind::Builtin(Builtin::ZonedTime);
const INT: Kind = Kind::Builtin(Builtin::Int);
const ZONE: Kind = Kind::Enum(crate::ZONE);
const RANGE: Kind = Kind::Std(INSTANT_RANGE);
const LOCALE: Kind = Kind::Enum(crate::LOCALE);
const TEXT_FOR_PEOPLE: Kind = Kind::Builtin(Builtin::PresentationText);

/// What `temporal.format` shows: an instant in a zone, a date, or a local
/// time.
const SHOWN: Kind = Kind::OneOf(&[Builtin::Instant, Builtin::CalendarDate, Builtin::ZonedTime]);
/// What `temporal.format_relative` shows: a moment, which a date isn't.
const MOMENT: Kind = Kind::OneOf(&[Builtin::Instant, Builtin::ZonedTime]);
/// `zone:` on a format: an `Instant` is shown in it, and anything else
/// carries its own zone or has none.
const SHOWN_IN: Param = option_needed_for("zone", ZONE, Builtin::Instant);

/// `InstantRange`: what `temporal`'s bounds give, a half-open range from
/// `start` up to but not including `end` (area 10, "Zones and local
/// times").
pub const INSTANT_RANGE: &str = "InstantRange";

/// `InstantRange`'s fields, in order.
pub const INSTANT_RANGE_FIELDS: [(&str, Builtin); 2] =
    [("start", Builtin::Instant), ("end", Builtin::Instant)];

/// `dst:` on a local time.
const DST: Param = option_only_for("dst", Kind::Enum(Dst::NAME), Builtin::ZonedTime);
/// `invalid_day:` on a calendar move.
const INVALID_DAY: Param = option("invalid_day", Kind::Enum(InvalidDay::NAME));

const ON_DST: Condition = Condition {
    key: RuleKey::Dst,
    option: "dst",
};
const ON_INVALID_DAY: Condition = Condition {
    key: RuleKey::InvalidDay,
    option: "invalid_day",
};

/// A `temporal` operation that never fails, by its parameters and result.
const fn temporal(
    op: Op,
    name: &'static str,
    params: &'static [Param],
    result: Kind,
    rust: &'static str,
) -> Operation {
    Operation {
        op,
        namespace: "temporal",
        name,
        params,
        result,
        at_least: None,
        conditions: &[],
        rust,
    }
}

/// `collection.contains`, the table's first entry.
const CONTAINS: Operation = Operation {
    op: Op::CollectionContains,
    namespace: "collection",
    name: "contains",
    params: &[param("items", Kind::List), param("item", Kind::Any)],
    result: Kind::Builtin(Builtin::Bool),
    at_least: None,
    conditions: &[],
    rust: "collection::contains",
};

/// Every operation, sorted by namespace and name.
pub const OPERATIONS: &[Operation] = &[
    CONTAINS,
    Operation {
        op: Op::CollectionCount,
        namespace: "collection",
        name: "count",
        params: &[param("items", Kind::List)],
        result: Kind::Builtin(Builtin::Int),
        at_least: Some(0),
        conditions: &[],
        rust: "collection::count",
    },
    Operation {
        op: Op::CollectionFilter,
        namespace: "collection",
        name: "filter",
        params: &[param("items", Kind::List), param("keep", Kind::Function)],
        result: Kind::List,
        at_least: None,
        conditions: &[],
        rust: "collection::filter",
    },
    Operation {
        op: Op::CollectionMap,
        namespace: "collection",
        name: "map",
        params: &[param("items", Kind::List), param("each", Kind::Function)],
        result: Kind::List,
        at_least: None,
        conditions: &[],
        rust: "collection::map",
    },
    Operation {
        op: Op::DecimalDivide,
        namespace: "decimal",
        name: "divide",
        params: &[
            param("dividend", Kind::Number),
            divisor("divisor", Kind::Number),
            param("rounding", ROUNDING),
        ],
        result: Kind::Builtin(Builtin::Decimal),
        at_least: None,
        conditions: &[],
        rust: "decimal::divide",
    },
    Operation {
        op: Op::IntegerDivide,
        namespace: "integer",
        name: "divide",
        params: &[
            param("dividend", Kind::Builtin(Builtin::Int)),
            divisor("divisor", Kind::Builtin(Builtin::Int)),
            param("rounding", ROUNDING),
        ],
        result: Kind::Builtin(Builtin::Int),
        at_least: None,
        conditions: &[],
        rust: "integer::divide",
    },
    Operation {
        conditions: &[ON_DST],
        ..temporal(
            Op::TemporalAddDays,
            "add_days",
            &[param("value", DATED), param("days", INT), DST],
            DATED,
            "temporal::add_days",
        )
    },
    Operation {
        conditions: &[ON_INVALID_DAY, ON_DST],
        ..temporal(
            Op::TemporalAddMonths,
            "add_months",
            &[
                param("value", DATED),
                param("months", INT),
                INVALID_DAY,
                DST,
            ],
            DATED,
            "temporal::add_months",
        )
    },
    Operation {
        conditions: &[ON_INVALID_DAY, ON_DST],
        ..temporal(
            Op::TemporalAddYears,
            "add_years",
            &[param("value", DATED), param("years", INT), INVALID_DAY, DST],
            DATED,
            "temporal::add_years",
        )
    },
    temporal(
        Op::TemporalBetween,
        "between",
        &[param("start", INSTANT), param("end", INSTANT)],
        Kind::Builtin(Builtin::Duration),
        "temporal::between",
    ),
    temporal(
        Op::TemporalDate,
        "date",
        &[param("value", ZONED)],
        DATE,
        "temporal::date",
    ),
    Operation {
        at_least: Some(1),
        ..temporal(
            Op::TemporalDay,
            "day",
            &[param("value", DATED)],
            INT,
            "temporal::day",
        )
    },
    temporal(
        Op::TemporalDayBounds,
        "day_bounds",
        &[param("date", DATE), param("zone", ZONE)],
        RANGE,
        "temporal::day_bounds",
    ),
    // The text these give comes with locales (area 10, "Locales", chunk
    // 5.3): until then the checker refuses every call, so `jadpo-value`
    // is never asked to run them.
    temporal(
        Op::TemporalFormat,
        "format",
        &[
            param("value", SHOWN),
            SHOWN_IN,
            param("locale", LOCALE),
            param("format", Kind::Format),
        ],
        TEXT_FOR_PEOPLE,
        "temporal::format",
    ),
    temporal(
        Op::TemporalFormatRelative,
        "format_relative",
        &[
            param("value", MOMENT),
            param("now", INSTANT),
            SHOWN_IN,
            param("locale", LOCALE),
        ],
        TEXT_FOR_PEOPLE,
        "temporal::format_relative",
    ),
    Operation {
        at_least: Some(0),
        ..temporal(
            Op::TemporalHour,
            "hour",
            &[param("value", ZONED)],
            INT,
            "temporal::hour",
        )
    },
    temporal(
        Op::TemporalInZone,
        "in_zone",
        &[param("instant", INSTANT), param("zone", ZONE)],
        ZONED,
        "temporal::in_zone",
    ),
    temporal(
        Op::TemporalInstant,
        "instant",
        &[param("value", ZONED)],
        INSTANT,
        "temporal::instant",
    ),
    Operation {
        at_least: Some(0),
        ..temporal(
            Op::TemporalMinute,
            "minute",
            &[param("value", ZONED)],
            INT,
            "temporal::minute",
        )
    },
    Operation {
        at_least: Some(1),
        ..temporal(
            Op::TemporalMonth,
            "month",
            &[param("value", DATED)],
            INT,
            "temporal::month",
        )
    },
    temporal(
        Op::TemporalMonthBounds,
        "month_bounds",
        &[param("date", DATE), param("zone", ZONE)],
        RANGE,
        "temporal::month_bounds",
    ),
    Operation {
        conditions: &[ON_DST],
        ..temporal(
            Op::TemporalResolve,
            "resolve",
            &[
                param("date", DATE),
                param("at", Kind::Clock),
                param("zone", ZONE),
                option("dst", Kind::Enum(Dst::NAME)),
            ],
            ZONED,
            "temporal::resolve",
        )
    },
    temporal(
        Op::TemporalWeekBounds,
        "week_bounds",
        &[
            param("date", DATE),
            param("zone", ZONE),
            param("starts_on", Kind::Enum(Weekday::NAME)),
        ],
        RANGE,
        "temporal::week_bounds",
    ),
    temporal(
        Op::TemporalWeekday,
        "weekday",
        &[param("value", DATED)],
        Kind::Enum(Weekday::NAME),
        "temporal::weekday",
    ),
    temporal(
        Op::TemporalYear,
        "year",
        &[param("value", DATED)],
        INT,
        "temporal::year",
    ),
    temporal(
        Op::TemporalYearBounds,
        "year_bounds",
        &[param("date", DATE), param("zone", ZONE)],
        RANGE,
        "temporal::year_bounds",
    ),
    temporal(
        Op::TemporalZone,
        "zone",
        &[param("value", ZONED)],
        ZONE,
        "temporal::zone",
    ),
];

/// The operation `namespace.name`, if the standard library has one.
pub fn operation(namespace: &str, name: &str) -> Option<&'static Operation> {
    OPERATIONS
        .iter()
        .find(|operation| operation.namespace == namespace && operation.name == name)
}

impl Op {
    /// The table's entry for this operation.
    pub fn operation(self) -> &'static Operation {
        OPERATIONS
            .iter()
            .find(|operation| operation.op == self)
            // The table lists every operation, which its test holds it to.
            .unwrap_or(&CONTAINS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_and_each_op_listed_once() {
        let names: Vec<String> = OPERATIONS.iter().map(Operation::qualified).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names, sorted);
        for operation in OPERATIONS {
            assert_eq!(operation.op.operation(), operation);
        }
        assert_eq!(
            operation("collection", "count").map(|op| op.at_least),
            Some(Some(0))
        );
        assert!(operation("temporal", "format").is_some());
        for operation in OPERATIONS {
            for condition in operation.conditions {
                let settled_by = operation
                    .params
                    .iter()
                    .find(|param| param.name == condition.option);
                assert!(
                    settled_by.is_some_and(|param| param.optional),
                    "{}'s `{}` is settled by an option it takes",
                    operation.qualified(),
                    condition.key.word()
                );
            }
        }
    }
}
