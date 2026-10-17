//! Every standard-library operation, typed (docs/compiler-runtime.md §2.1):
//! its parameters with their kinds, its result, and the one Rust function in
//! `jadpo-value` that runs it for both executors. The Jadpo stubs in
//! `jadpo-check` declare the same operations for programs to call, and a
//! test holds the two to the same arity and kinds; `jadpo-value` matches on
//! [`Op`] exhaustively, so an operation listed here without an
//! implementation does not compile.

use crate::standard::INSTANT_RANGE;
use crate::{Builtin, Dst, InvalidDay, Rounding, RuleKey, TextDirection, Weekday};

/// What a parameter or a result is, as far as an operation cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A type variable (`T`): any type, fixed by the arguments at each call.
    Any,
    /// `List<T>`.
    List,
    /// `Set<T>`.
    Set,
    /// `Map<K, V>`.
    Map,
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
    /// A standard-library type the checker knows by name, whose fields
    /// the runtime builds as a record.
    Std(&'static str),
    /// A range (area 19, "Ranges"): `Range<T>`, or a standard range type
    /// whose ends are a `T`, `InstantRange`'s instants. A result of this
    /// kind is the range type its arguments are.
    Range,
    /// A clock time written as text, `"09:30"`, checked while compiling
    /// (area 10, "Zones and local times"): there is no time-of-day type,
    /// so only a literal gives one.
    Clock,
    /// A time format written out (area 10, "Showing dates and times to
    /// people"): `TimeFormat.date_long` or a declared format's name, which
    /// the checker reads into a [`crate::TimeFormatSpec`] and the operation
    /// is given as its text.
    Format,
    /// A value the standard type contract's operations take (area 19,
    /// "Every standard type meets one contract"): which types fit, and
    /// what each gives, is the value's standard type's data entry
    /// ([`crate::standard`]), which the checker reads.
    Contract,
    /// A type written out, `as: Email` (area 19): the checker reads it, a
    /// standard type or a type built on one or on a representation, and
    /// the operation is given the name of the standard type or
    /// representation whose canonical text it reads; the call's result is
    /// that type.
    Type,
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
    /// A value that may be `none` but is still given: its stub writes it
    /// `T?` and a call must pass it, as `locale.from_accept_language`'s
    /// `header`, which a header read with `optional: true` fills.
    pub nullable: bool,
    /// An option that means something only when the call's first argument
    /// is built on this type, as `dst:` only for a `ZonedTime`.
    pub only_for: Option<Builtin>,
    /// An option that a call must give whenever it means something, as
    /// `zone:` for an `Instant` being formatted.
    pub needed: bool,
    /// An option whose meaning is the value's standard type's: its data
    /// entry's format options say whether the type takes it and needs it
    /// (`zone:` and `format:` on `text.format`, area 19).
    pub by_type: bool,
    /// A count or a position: the checker rules a negative one out while
    /// compiling, as `never_zero` rules out a zero divisor (area 19,
    /// question 7): a literal isn't below 0 and a value's type has a
    /// `min` of at least 0, so the operation never counts from the other
    /// end.
    pub never_negative: bool,
    /// An inline function whose result orders the items (`sort_by`'s
    /// `key`): it must give a number, a time, a date, a duration or text,
    /// which the canonical order compares as what they are (area 19).
    pub ordered: bool,
    /// Never written by a call: the checker fills it from the call's types
    /// and the operation is given it like any other value. `list.sum`'s
    /// `zero` is `0` of its items' representation, so an empty list sums
    /// to the `Int` or the `Decimal` its type says.
    pub implicit: bool,
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
    DecimalDivide,
    IntegerDivide,
    ListAll,
    ListAny,
    ListAppend,
    ListAt,
    ListConcat,
    ListContains,
    ListCount,
    ListDistinct,
    ListDropFirst,
    ListDropLast,
    ListFilter,
    ListFind,
    ListFirst,
    ListFlatten,
    ListGroupBy,
    ListKeepFirst,
    ListKeepLast,
    ListLast,
    ListMap,
    ListMaxBy,
    ListMinBy,
    ListReverse,
    ListSlice,
    ListSortBy,
    ListSum,
    ListZipWith,
    LocaleDirection,
    LocaleFromAcceptLanguage,
    MapCount,
    MapFilter,
    MapGet,
    MapHas,
    MapKeys,
    MapMapValues,
    MapMerge,
    MapValues,
    MapWith,
    MapWithout,
    RangeContains,
    RangeIntersection,
    RangeOverlaps,
    SetContains,
    SetCount,
    SetDifference,
    SetIntersection,
    SetUnion,
    SetWith,
    SetWithout,
    TemporalAddDays,
    TemporalAddMonths,
    TemporalAddYears,
    TemporalBetween,
    TemporalDate,
    TemporalDay,
    TemporalDayBounds,
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
    TextFormat,
    TextFrom,
    TextParse,
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
    /// `list.count` never gives less than 0, though its result is a
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
        nullable: false,
        only_for: None,
        needed: false,
        by_type: false,
        never_negative: false,
        ordered: false,
        implicit: false,
    }
}

/// A count or a position, never negative.
const fn count(name: &'static str) -> Param {
    Param {
        never_negative: true,
        ..param(name, INT)
    }
}

/// An inline function whose result orders the items.
const fn key() -> Param {
    Param {
        ordered: true,
        ..param("key", Kind::Function)
    }
}

/// A parameter that is given but may be `none`.
const fn nullable(name: &'static str, kind: Kind) -> Param {
    Param {
        nullable: true,
        ..param(name, kind)
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

/// An option whose meaning is the value's standard type's (area 19).
const fn by_type(name: &'static str, kind: Kind) -> Param {
    Param {
        by_type: true,
        ..option(name, kind)
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
const BOOL: Kind = Kind::Builtin(Builtin::Bool);
const ZONE: Kind = Kind::Enum(crate::ZONE);
/// What `temporal`'s bounds give.
const RANGE: Kind = Kind::Std(INSTANT_RANGE);
const LOCALE: Kind = Kind::Enum(crate::LOCALE);
const SHOWN_TEXT: Kind = Kind::Builtin(Builtin::PresentationText);

/// What `temporal.format_relative` shows: a moment, which a date isn't.
const MOMENT: Kind = Kind::OneOf(&[Builtin::Instant, Builtin::ZonedTime]);
/// `zone:` on a format: an `Instant` is shown in it, and anything else
/// carries its own zone or has none.
const SHOWN_IN: Param = option_needed_for("zone", ZONE, Builtin::Instant);

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

/// `decimal.divide`, the table's first entry.
const DIVIDE: Operation = Operation {
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
};

const ITEMS: Param = param("items", Kind::List);

/// A `list` operation that never fails (area 19, "Lists"), by its
/// parameters and result.
const fn list(
    op: Op,
    name: &'static str,
    params: &'static [Param],
    result: Kind,
    rust: &'static str,
) -> Operation {
    Operation {
        op,
        namespace: "list",
        name,
        params,
        result,
        at_least: None,
        conditions: &[],
        rust,
    }
}

const SET: Param = param("items", Kind::Set);

/// A `set` operation that never fails (area 19, "Sets"), by its
/// parameters and result.
const fn set(
    op: Op,
    name: &'static str,
    params: &'static [Param],
    result: Kind,
    rust: &'static str,
) -> Operation {
    Operation {
        op,
        namespace: "set",
        name,
        params,
        result,
        at_least: None,
        conditions: &[],
        rust,
    }
}

const ENTRIES: Param = param("entries", Kind::Map);

/// A `map` operation that never fails (area 19, "Maps"), by its
/// parameters and result.
const fn map(
    op: Op,
    name: &'static str,
    params: &'static [Param],
    result: Kind,
    rust: &'static str,
) -> Operation {
    Operation {
        op,
        namespace: "map",
        name,
        params,
        result,
        at_least: None,
        conditions: &[],
        rust,
    }
}

/// A `range` operation that never fails (area 19, "Ranges"), by its
/// parameters and result: on a `Range<T>` or an `InstantRange`, both of
/// the same range type.
const fn range(
    op: Op,
    name: &'static str,
    params: &'static [Param],
    result: Kind,
    rust: &'static str,
) -> Operation {
    Operation {
        op,
        namespace: "range",
        name,
        params,
        result,
        at_least: None,
        conditions: &[],
        rust,
    }
}

/// Every operation, sorted by namespace and name.
pub const OPERATIONS: &[Operation] = &[
    DIVIDE,
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
    // The list operations (area 19, "Lists"), each one function in
    // `jadpo-value`'s `list` module. Those whose result is `T`, `T?` or
    // `List<T>` give items of the list unchanged, so they keep the items'
    // named type; a named list type is built back by construction (area 2).
    list(
        Op::ListAll,
        "all",
        &[ITEMS, param("check", Kind::Function)],
        BOOL,
        "list::all",
    ),
    list(
        Op::ListAny,
        "any",
        &[ITEMS, param("check", Kind::Function)],
        BOOL,
        "list::any",
    ),
    list(
        Op::ListAppend,
        "append",
        &[ITEMS, param("item", Kind::Any)],
        Kind::List,
        "list::append",
    ),
    list(
        Op::ListAt,
        "at",
        &[ITEMS, count("position")],
        Kind::Any,
        "list::at",
    ),
    list(
        Op::ListConcat,
        "concat",
        &[param("first", Kind::List), param("second", Kind::List)],
        Kind::List,
        "list::concat",
    ),
    list(
        Op::ListContains,
        "contains",
        &[ITEMS, param("item", Kind::Any)],
        BOOL,
        "list::contains",
    ),
    Operation {
        at_least: Some(0),
        ..list(Op::ListCount, "count", &[ITEMS], INT, "list::count")
    },
    list(
        Op::ListDistinct,
        "distinct",
        &[ITEMS],
        Kind::List,
        "list::distinct",
    ),
    list(
        Op::ListDropFirst,
        "drop_first",
        &[ITEMS, count("n")],
        Kind::List,
        "list::drop_first",
    ),
    list(
        Op::ListDropLast,
        "drop_last",
        &[ITEMS, count("n")],
        Kind::List,
        "list::drop_last",
    ),
    list(
        Op::ListFilter,
        "filter",
        &[ITEMS, param("keep", Kind::Function)],
        Kind::List,
        "list::filter",
    ),
    list(
        Op::ListFind,
        "find",
        &[ITEMS, param("check", Kind::Function)],
        Kind::Any,
        "list::find",
    ),
    list(Op::ListFirst, "first", &[ITEMS], Kind::Any, "list::first"),
    list(
        Op::ListFlatten,
        "flatten",
        &[param("lists", Kind::List)],
        Kind::List,
        "list::flatten",
    ),
    list(
        Op::ListGroupBy,
        "group_by",
        &[ITEMS, param("key", Kind::Function)],
        Kind::Map,
        "list::group_by",
    ),
    list(
        Op::ListKeepFirst,
        "keep_first",
        &[ITEMS, count("n")],
        Kind::List,
        "list::keep_first",
    ),
    list(
        Op::ListKeepLast,
        "keep_last",
        &[ITEMS, count("n")],
        Kind::List,
        "list::keep_last",
    ),
    list(Op::ListLast, "last", &[ITEMS], Kind::Any, "list::last"),
    list(
        Op::ListMap,
        "map",
        &[ITEMS, param("each", Kind::Function)],
        Kind::List,
        "list::map",
    ),
    list(
        Op::ListMaxBy,
        "max_by",
        &[ITEMS, key()],
        Kind::Any,
        "list::max_by",
    ),
    list(
        Op::ListMinBy,
        "min_by",
        &[ITEMS, key()],
        Kind::Any,
        "list::min_by",
    ),
    list(
        Op::ListReverse,
        "reverse",
        &[ITEMS],
        Kind::List,
        "list::reverse",
    ),
    list(
        Op::ListSlice,
        "slice",
        &[ITEMS, count("from"), count("count")],
        Kind::List,
        "list::slice",
    ),
    list(
        Op::ListSortBy,
        "sort_by",
        &[ITEMS, key()],
        Kind::List,
        "list::sort_by",
    ),
    // The items added, from the `zero` the checker gives, of the items'
    // representation (area 19); overflow is a defect, as it is for `+`.
    list(
        Op::ListSum,
        "sum",
        &[
            ITEMS,
            Param {
                implicit: true,
                ..param("zero", Kind::Number)
            },
        ],
        Kind::Number,
        "list::sum",
    ),
    // Items paired by position, as long as the shorter list, each pair
    // given to a two-parameter inline function (area 19).
    list(
        Op::ListZipWith,
        "zip_with",
        &[
            param("first", Kind::List),
            param("second", Kind::List),
            param("each", Kind::Function),
        ],
        Kind::List,
        "list::zip_with",
    ),
    // The way a locale's text runs (area 10, "Locales"), from CLDR.
    Operation {
        op: Op::LocaleDirection,
        namespace: "locale",
        name: "direction",
        params: &[param("value", LOCALE)],
        result: Kind::Enum(TextDirection::NAME),
        at_least: None,
        conditions: &[],
        rust: "locale::direction",
    },
    // The locale a request's `Accept-Language` header asks for, of the ones
    // the application supports (area 10, "Locales"): `jadpo-value`'s
    // `locale` module, given the application's `LocalePlan`.
    Operation {
        op: Op::LocaleFromAcceptLanguage,
        namespace: "locale",
        name: "from_accept_language",
        params: &[nullable("header", Kind::Builtin(Builtin::Text))],
        result: LOCALE,
        at_least: None,
        conditions: &[],
        rust: "locale::from_accept_language",
    },
    // The map operations (area 19, "Maps"), each one function in
    // `jadpo-value`'s `map` module. A map keeps its keys in the canonical
    // order (area 2), so `keys` and `values` come out in it.
    Operation {
        at_least: Some(0),
        ..map(Op::MapCount, "count", &[ENTRIES], INT, "map::count")
    },
    // An entry's key and value, given to a two-parameter inline function
    // (area 19).
    map(
        Op::MapFilter,
        "filter",
        &[ENTRIES, param("keep", Kind::Function)],
        Kind::Map,
        "map::filter",
    ),
    map(
        Op::MapGet,
        "get",
        &[ENTRIES, param("key", Kind::Any)],
        Kind::Any,
        "map::get",
    ),
    map(
        Op::MapHas,
        "has",
        &[ENTRIES, param("key", Kind::Any)],
        BOOL,
        "map::has",
    ),
    map(Op::MapKeys, "keys", &[ENTRIES], Kind::Set, "map::keys"),
    map(
        Op::MapMapValues,
        "map_values",
        &[ENTRIES, param("each", Kind::Function)],
        Kind::Map,
        "map::map_values",
    ),
    map(
        Op::MapMerge,
        "merge",
        &[param("first", Kind::Map), param("second", Kind::Map)],
        Kind::Map,
        "map::merge",
    ),
    map(
        Op::MapValues,
        "values",
        &[ENTRIES],
        Kind::List,
        "map::values",
    ),
    map(
        Op::MapWith,
        "with",
        &[ENTRIES, param("key", Kind::Any), param("value", Kind::Any)],
        Kind::Map,
        "map::with",
    ),
    map(
        Op::MapWithout,
        "without",
        &[ENTRIES, param("key", Kind::Any)],
        Kind::Map,
        "map::without",
    ),
    // The range operations (area 19, "Ranges"), each one function in
    // `jadpo-value`'s `range` module, by the range's own end rule: a
    // `Range<T>` and an `InstantRange` hold their start and not their end,
    // and an open end holds everything on its side.
    range(
        Op::RangeContains,
        "contains",
        &[param("range", Kind::Range), param("value", Kind::Any)],
        BOOL,
        "range::contains",
    ),
    range(
        Op::RangeIntersection,
        "intersection",
        &[param("a", Kind::Range), param("b", Kind::Range)],
        Kind::Range,
        "range::intersection",
    ),
    range(
        Op::RangeOverlaps,
        "overlaps",
        &[param("a", Kind::Range), param("b", Kind::Range)],
        BOOL,
        "range::overlaps",
    ),
    // The set operations (area 19, "Sets"), each one function in
    // `jadpo-value`'s `set` module. A set is held in the canonical order
    // without repeats (area 2), and every result is a new one; its items
    // are the same values, so they keep their named type.
    set(
        Op::SetContains,
        "contains",
        &[SET, param("item", Kind::Any)],
        BOOL,
        "set::contains",
    ),
    Operation {
        at_least: Some(0),
        ..set(Op::SetCount, "count", &[SET], INT, "set::count")
    },
    set(
        Op::SetDifference,
        "difference",
        &[param("a", Kind::Set), param("b", Kind::Set)],
        Kind::Set,
        "set::difference",
    ),
    set(
        Op::SetIntersection,
        "intersection",
        &[param("a", Kind::Set), param("b", Kind::Set)],
        Kind::Set,
        "set::intersection",
    ),
    set(
        Op::SetUnion,
        "union",
        &[param("a", Kind::Set), param("b", Kind::Set)],
        Kind::Set,
        "set::union",
    ),
    set(
        Op::SetWith,
        "with",
        &[SET, param("item", Kind::Any)],
        Kind::Set,
        "set::with",
    ),
    set(
        Op::SetWithout,
        "without",
        &[SET, param("item", Kind::Any)],
        Kind::Set,
        "set::without",
    ),
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
    // The text this gives is `jadpo-value`'s `present` module, over the
    // locale data of the locales the application supports (area 10,
    // "Locales"); `text.format` shows the other time types.
    temporal(
        Op::TemporalFormatRelative,
        "format_relative",
        &[
            param("value", MOMENT),
            param("now", INSTANT),
            SHOWN_IN,
            param("locale", LOCALE),
        ],
        SHOWN_TEXT,
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
    // The standard type contract's three operations (area 19, "Every
    // standard type meets one contract"), each one implementation in
    // `jadpo-value`'s `standard` module over the data entries in
    // [`crate::standard`]. `text.format` writes text for people over the
    // locale data of the locales the application supports.
    Operation {
        op: Op::TextFormat,
        namespace: "text",
        name: "format",
        params: &[
            param("value", Kind::Contract),
            param("locale", LOCALE),
            by_type("zone", ZONE),
            by_type("format", Kind::Format),
        ],
        result: SHOWN_TEXT,
        at_least: None,
        conditions: &[],
        rust: "standard::format",
    },
    Operation {
        op: Op::TextFrom,
        namespace: "text",
        name: "from",
        params: &[param("value", Kind::Contract)],
        result: Kind::Builtin(Builtin::Text),
        at_least: None,
        conditions: &[],
        rust: "standard::from",
    },
    // Fails `malformed` for text that isn't the type at all, and by the
    // rules of the type `as:` names, which the checker works out per call.
    Operation {
        op: Op::TextParse,
        namespace: "text",
        name: "parse",
        params: &[
            param("raw", Kind::Builtin(Builtin::Text)),
            param("as", Kind::Type),
        ],
        result: Kind::Any,
        at_least: None,
        conditions: &[],
        rust: "standard::parse",
    },
];

/// Operations that moved, by their old name and their new one: a call by
/// the old name is `semantic.standard_operation_moved`, whose edit writes
/// the new one (area 19). `temporal.format` became the time types'
/// `text.format` (question 2).
pub const MOVED: &[(&str, &str)] = &[("temporal.format", "text.format")];

/// Where `namespace.name` moved to, if it did.
pub fn moved(namespace: &str, name: &str) -> Option<&'static str> {
    MOVED
        .iter()
        .find(|(from, _)| {
            from.split_once('.')
                .is_some_and(|(space, member)| space == namespace && member == name)
        })
        .map(|(_, to)| *to)
}

/// Namespaces that moved whole, each with the namespaces it became: a
/// call `collection.<name>` is `semantic.standard_operation_moved`, whose
/// edit writes `list.`, `set.` or `map.` by what its first argument is, a
/// list unless it is a set or a map (area 19, question 6). The old name
/// stays reserved, so the habit is always caught.
pub const MOVED_NAMESPACES: &[(&str, &[&str])] = &[("collection", &["list", "set", "map"])];

/// The namespaces `namespace` became, if it moved whole.
pub fn moved_namespace(namespace: &str) -> Option<&'static [&'static str]> {
    MOVED_NAMESPACES
        .iter()
        .find(|(from, _)| *from == namespace)
        .map(|(_, to)| *to)
}

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
            .unwrap_or(&DIVIDE)
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
            operation("list", "count").map(|op| op.at_least),
            Some(Some(0))
        );
        assert!(operation("text", "format").is_some());
        assert!(
            operation("temporal", "format").is_none(),
            "moved to `text.format`"
        );
        assert_eq!(moved("temporal", "format"), Some("text.format"));
        for (_, to) in MOVED {
            let (namespace, name) = to.split_once('.').unwrap_or_default();
            assert!(operation(namespace, name).is_some(), "{to} exists");
        }
        assert_eq!(
            moved_namespace("collection"),
            Some(&["list", "set", "map"][..])
        );
        assert!(
            operation("collection", "count").is_none(),
            "moved to `list.`"
        );
        for operation in OPERATIONS {
            // A value the checker fills comes after every written one.
            let written = operation.params.iter().take_while(|p| !p.implicit).count();
            assert!(operation.params[written..].iter().all(|p| p.implicit));
            for param in operation.params {
                assert!(
                    !param.never_negative || param.kind == INT,
                    "a count is an `Int`"
                );
                assert!(
                    !param.ordered || param.kind == Kind::Function,
                    "a key is inline"
                );
            }
        }
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
