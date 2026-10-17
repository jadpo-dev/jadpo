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
    /// (area 10, "Zones and local times"): only a literal gives one, not a
    /// value, a `LocalTime` included.
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
    /// end. On a list, each item: a literal list's items aren't below 0
    /// and its item type has a `min` of at least 0 (`money.allocate`'s
    /// ratios).
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
    /// A whole number written out at the call, never a value (area 19,
    /// "Numbers"): `decimal.round`'s `scale:`, from 0 to 18 places as a
    /// type's `scale` is, so the places a result keeps are read while
    /// compiling.
    pub written_out: bool,
    /// Text that is never empty (area 19, "What every operation follows"):
    /// the checker rules `""` out while compiling, as `never_zero` rules
    /// out a zero divisor: a literal isn't `""` and a value's type has a
    /// `min_length` of at least 1, so `text.replace` never searches for
    /// nothing and `text.split` never splits on it. On a list, a literal
    /// list has items and a value's type has a `min_length` of at least 1
    /// (`money.allocate`'s ratios).
    pub never_empty: bool,
    /// One character written out at the call, never a value (area 19,
    /// "Text"): `text.pad_start`'s `fill:`, a text literal of exactly one
    /// code point.
    pub character: bool,
    /// The text whose length the result never passes (area 19, question
    /// 29, `at_most_length_of`): the result is never longer than this
    /// argument, so constructing a type from it can't break a
    /// `max_length` the argument's own type keeps.
    pub bounds_length: bool,
    /// A value the operation has a result for only above 0, as `ln`'s:
    /// proved while compiling as `never_negative` is, a literal above 0
    /// or a value whose type's `min` is, the call never fails; otherwise
    /// it may fail with the operation's condition that no option settles.
    pub above_zero: bool,
}

/// A condition an operation fails on unless an option settles it (area
/// 10, question 7): the call's `attempt` maps it by `key`, as a
/// construction's broken rule is mapped (area 2). It applies while the
/// option named `option` applies and is left out, or given as anything
/// but a written value other than `reject`. One with no option is
/// settled only by proof: `decimal.ln`'s `out_of_range`, by a value
/// proved above 0 ([`Param::above_zero`], area 19, "Numbers"); or, for
/// `malformed` and `mismatch` on what an `encoding` or `json` operation
/// decodes, never while compiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Condition {
    pub key: RuleKey,
    pub option: Option<&'static str>,
}

/// Which operation, for an exhaustive match in each executor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Op {
    DecimalAbs,
    DecimalAtan2,
    DecimalCos,
    DecimalDivide,
    DecimalExp,
    DecimalLn,
    DecimalMax,
    DecimalMin,
    DecimalPi,
    DecimalPower,
    DecimalRound,
    DecimalSin,
    DecimalSqrt,
    EncodingFromBase64,
    EncodingFromBase64url,
    EncodingFromHex,
    EncodingFromUtf8,
    EncodingToBase64,
    EncodingToBase64url,
    EncodingToHex,
    EncodingToUrlComponent,
    EncodingToUtf8,
    HashSha256,
    IntegerAbs,
    IntegerDivide,
    IntegerFromDecimal,
    IntegerMax,
    IntegerMin,
    JsonRead,
    JsonWrite,
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
    MoneyAllocate,
    MoneyConvert,
    MoneyCurrency,
    MoneyFromMinor,
    MoneyMinor,
    MoneyOf,
    MoneySplit,
    MoneySum,
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
    TemporalPeriodBetween,
    TemporalResolve,
    TemporalWeekBounds,
    TemporalWeekday,
    TemporalYear,
    TemporalYearBounds,
    TemporalZone,
    TextContains,
    TextDropFirst,
    TextDropLast,
    TextEndsWith,
    TextFormat,
    TextFrom,
    TextJoin,
    TextKeepFirst,
    TextKeepLast,
    TextLength,
    TextLower,
    TextPadStart,
    TextParse,
    TextReplace,
    TextSlice,
    TextSlug,
    TextSplit,
    TextStartsWith,
    TextTrim,
    TextUpper,
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
        written_out: false,
        never_empty: false,
        character: false,
        bounds_length: false,
        above_zero: false,
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

/// `scale:`, the places a rounding keeps, written out.
const SCALE: Param = Param {
    written_out: true,
    ..param("scale", INT)
};

/// The maths on `Decimal` (area 19, "Numbers", question 18): each result
/// is correctly rounded to `scale:` by `rounding:`, the same on every
/// target, by its parameters before those two.
const fn maths(
    op: Op,
    name: &'static str,
    params: &'static [Param],
    rust: &'static str,
) -> Operation {
    number(op, "decimal", name, params, DECIMAL, rust)
}

/// `ln`'s value, at or below 0, unless proved above it.
const ON_OUT_OF_RANGE: Condition = Condition {
    key: RuleKey::OutOfRange,
    option: None,
};

/// Text or bytes that aren't the format an `encoding` or `json` operation
/// decodes at all (area 19, "Encodings" and "Data formats"): nothing
/// proves it while compiling, so the call's `attempt` maps it.
const ON_MALFORMED: Condition = Condition {
    key: RuleKey::Malformed,
    option: None,
};

/// A JSON document that isn't the type `json.read`'s `as:` names.
const ON_MISMATCH: Condition = Condition {
    key: RuleKey::Mismatch,
    option: None,
};

/// Plain text and plain bytes, as the encodings take and give them.
const TEXT_VALUE: Kind = Kind::Builtin(Builtin::Text);
const BYTES_VALUE: Kind = Kind::Builtin(Builtin::Bytes);
/// A value built on `Bytes`, or a `Text`, taken as its UTF-8 bytes
/// (`hash.sha256`, `json.read`).
const TEXT_OR_BYTES: Kind = Kind::OneOf(&[Builtin::Text, Builtin::Bytes]);

/// An `encoding` operation (area 19, "Encodings"), by its parameter, its
/// result and whether it decodes, failing `malformed`.
const fn encoding(
    op: Op,
    name: &'static str,
    params: &'static [Param],
    result: Kind,
    decodes: bool,
    rust: &'static str,
) -> Operation {
    Operation {
        op,
        namespace: "encoding",
        name,
        params,
        result,
        at_least: None,
        conditions: if decodes { &[ON_MALFORMED] } else { &[] },
        rust,
    }
}

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
    option: Some("dst"),
};
const ON_INVALID_DAY: Condition = Condition {
    key: RuleKey::InvalidDay,
    option: Some("invalid_day"),
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

/// A value built on `Int`, and one built on `Decimal`.
const WHOLE: Kind = Kind::Builtin(Builtin::Int);
const DECIMAL: Kind = Kind::Builtin(Builtin::Decimal);
/// Two values of one type built on `Int` or on `Decimal`, and the result
/// that is one of them, keeping their named type (area 19, "Numbers").
const SAME_WHOLE: Kind = Kind::OneOf(&[Builtin::Int]);
const SAME_DECIMAL: Kind = Kind::OneOf(&[Builtin::Decimal]);

/// A number operation that never fails (area 19, "Numbers"), by its
/// namespace, parameters and result.
const fn number(
    op: Op,
    namespace: &'static str,
    name: &'static str,
    params: &'static [Param],
    result: Kind,
    rust: &'static str,
) -> Operation {
    Operation {
        op,
        namespace,
        name,
        params,
        result,
        at_least: None,
        conditions: &[],
        rust,
    }
}

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

/// A `money` operation (area 19, "Money and payments", question 16), by
/// its parameters and result; amounts are whole numbers of minor units.
const fn money(
    op: Op,
    name: &'static str,
    params: &'static [Param],
    result: Kind,
    rust: &'static str,
) -> Operation {
    Operation {
        op,
        namespace: "money",
        name,
        params,
        result,
        at_least: None,
        conditions: &[],
        rust,
    }
}

const MONEY: Kind = Kind::Builtin(Builtin::Money);
const CURRENCY: Kind = Kind::Enum(crate::CURRENCY);

/// Two amounts, or an amount and a list's currency, in different
/// currencies, where only the running program knows them: settled only by
/// proof, a type's `currency:` or a currency written out (area 19).
const ON_CURRENCY_MISMATCH: Condition = Condition {
    key: RuleKey::CurrencyMismatch,
    option: None,
};

/// More places than the currency has: settled only by proof, a currency
/// written out and an amount with no more places than it has.
const ON_TOO_MANY_PLACES: Condition = Condition {
    key: RuleKey::TooManyPlaces,
    option: None,
};

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

const TEXT: Kind = Kind::Builtin(Builtin::Text);

/// Text an operation reads: any value built on `Text`.
const VALUE: Param = param("value", TEXT);

/// The text whose length a result never passes (`at_most_length_of`).
const CUT: Param = Param {
    bounds_length: true,
    ..VALUE
};

/// Text that is never empty.
const fn never_empty(name: &'static str) -> Param {
    Param {
        never_empty: true,
        ..param(name, TEXT)
    }
}

/// A `text` operation that never fails (area 19, "Text"), by its
/// parameters and result. Text is counted, cut and padded in code points.
const fn text(
    op: Op,
    name: &'static str,
    params: &'static [Param],
    result: Kind,
    rust: &'static str,
) -> Operation {
    Operation {
        op,
        namespace: "text",
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
    // The number operations (area 19, "Numbers"), each one function in
    // `jadpo-value`'s `integer` or `decimal` module. `min` and `max` give
    // one of their two values as it is, so they keep its named type; `abs`
    // and the roundings give a representation, never below 0 for `abs`.
    Operation {
        at_least: Some(0),
        ..number(
            Op::DecimalAbs,
            "decimal",
            "abs",
            &[param("value", DECIMAL)],
            DECIMAL,
            "decimal::abs",
        )
    },
    // The maths (area 19, "Numbers", question 18), each one function in
    // `jadpo-value`'s `decimal` module: an angle is in radians, `sqrt`'s
    // value and `power`'s exponent are never negative, which the checker
    // proves, and `ln` fails with `out_of_range` unless its value is
    // proved above 0. Overflow past 38 digits is a defect.
    maths(
        Op::DecimalAtan2,
        "atan2",
        &[
            param("y", DECIMAL),
            param("x", DECIMAL),
            SCALE,
            param("rounding", ROUNDING),
        ],
        "decimal::atan2",
    ),
    maths(
        Op::DecimalCos,
        "cos",
        &[
            param("radians", DECIMAL),
            SCALE,
            param("rounding", ROUNDING),
        ],
        "decimal::cos",
    ),
    DIVIDE,
    Operation {
        at_least: Some(0),
        ..maths(
            Op::DecimalExp,
            "exp",
            &[param("value", DECIMAL), SCALE, param("rounding", ROUNDING)],
            "decimal::exp",
        )
    },
    Operation {
        conditions: &[ON_OUT_OF_RANGE],
        ..maths(
            Op::DecimalLn,
            "ln",
            &[
                Param {
                    above_zero: true,
                    ..param("value", DECIMAL)
                },
                SCALE,
                param("rounding", ROUNDING),
            ],
            "decimal::ln",
        )
    },
    number(
        Op::DecimalMax,
        "decimal",
        "max",
        &[param("a", SAME_DECIMAL), param("b", SAME_DECIMAL)],
        SAME_DECIMAL,
        "decimal::max",
    ),
    number(
        Op::DecimalMin,
        "decimal",
        "min",
        &[param("a", SAME_DECIMAL), param("b", SAME_DECIMAL)],
        SAME_DECIMAL,
        "decimal::min",
    ),
    Operation {
        at_least: Some(3),
        ..maths(
            Op::DecimalPi,
            "pi",
            &[SCALE, param("rounding", ROUNDING)],
            "decimal::pi",
        )
    },
    maths(
        Op::DecimalPower,
        "power",
        &[
            param("value", DECIMAL),
            Param {
                never_negative: true,
                ..param("exponent", INT)
            },
        ],
        "decimal::power",
    ),
    number(
        Op::DecimalRound,
        "decimal",
        "round",
        &[param("value", DECIMAL), SCALE, param("rounding", ROUNDING)],
        DECIMAL,
        "decimal::round",
    ),
    maths(
        Op::DecimalSin,
        "sin",
        &[
            param("radians", DECIMAL),
            SCALE,
            param("rounding", ROUNDING),
        ],
        "decimal::sin",
    ),
    Operation {
        at_least: Some(0),
        ..maths(
            Op::DecimalSqrt,
            "sqrt",
            &[
                Param {
                    never_negative: true,
                    ..param("value", DECIMAL)
                },
                SCALE,
                param("rounding", ROUNDING),
            ],
            "decimal::sqrt",
        )
    },
    // The encodings (area 19, "Encodings"), each one function in
    // `jadpo-value`'s `encoding` module; decoding fails `malformed` only.
    encoding(
        Op::EncodingFromBase64,
        "from_base64",
        &[param("text", TEXT_VALUE)],
        BYTES_VALUE,
        true,
        "encoding::from_base64",
    ),
    encoding(
        Op::EncodingFromBase64url,
        "from_base64url",
        &[param("text", TEXT_VALUE)],
        BYTES_VALUE,
        true,
        "encoding::from_base64url",
    ),
    encoding(
        Op::EncodingFromHex,
        "from_hex",
        &[param("text", TEXT_VALUE)],
        BYTES_VALUE,
        true,
        "encoding::from_hex",
    ),
    encoding(
        Op::EncodingFromUtf8,
        "from_utf8",
        &[param("bytes", BYTES_VALUE)],
        TEXT_VALUE,
        true,
        "encoding::from_utf8",
    ),
    encoding(
        Op::EncodingToBase64,
        "to_base64",
        &[param("bytes", BYTES_VALUE)],
        TEXT_VALUE,
        false,
        "encoding::to_base64",
    ),
    encoding(
        Op::EncodingToBase64url,
        "to_base64url",
        &[param("bytes", BYTES_VALUE)],
        TEXT_VALUE,
        false,
        "encoding::to_base64url",
    ),
    encoding(
        Op::EncodingToHex,
        "to_hex",
        &[param("bytes", BYTES_VALUE)],
        TEXT_VALUE,
        false,
        "encoding::to_hex",
    ),
    encoding(
        Op::EncodingToUrlComponent,
        "to_url_component",
        &[param("text", TEXT_VALUE)],
        TEXT_VALUE,
        false,
        "encoding::to_url_component",
    ),
    encoding(
        Op::EncodingToUtf8,
        "to_utf8",
        &[param("text", TEXT_VALUE)],
        BYTES_VALUE,
        false,
        "encoding::to_utf8",
    ),
    // SHA-256 (area 19, "Hashing and cryptography", question 24): a
    // digest is `Bytes`, written as text by `encoding`.
    Operation {
        op: Op::HashSha256,
        namespace: "hash",
        name: "sha256",
        params: &[param("value", TEXT_OR_BYTES)],
        result: BYTES_VALUE,
        at_least: None,
        conditions: &[],
        rust: "hash::sha256",
    },
    // Overflow on the smallest `Int` is a defect, as it is for `-`.
    Operation {
        at_least: Some(0),
        ..number(
            Op::IntegerAbs,
            "integer",
            "abs",
            &[param("value", WHOLE)],
            WHOLE,
            "integer::abs",
        )
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
    // A whole number past an `Int` is a defect.
    number(
        Op::IntegerFromDecimal,
        "integer",
        "from_decimal",
        &[param("value", DECIMAL), param("rounding", ROUNDING)],
        WHOLE,
        "integer::from_decimal",
    ),
    number(
        Op::IntegerMax,
        "integer",
        "max",
        &[param("a", SAME_WHOLE), param("b", SAME_WHOLE)],
        SAME_WHOLE,
        "integer::max",
    ),
    number(
        Op::IntegerMin,
        "integer",
        "min",
        &[param("a", SAME_WHOLE), param("b", SAME_WHOLE)],
        SAME_WHOLE,
        "integer::min",
    ),
    // JSON in memory (area 19, "Data formats", question 14), through the
    // wire's codecs (areas 2 and 7): `json.read` is given the codec of the
    // type `as:` names, and `json.write` the codec of its value's type,
    // which the plan numbers per call, so both run in `jadpo-value`'s
    // `json` module with the plan's codecs rather than through
    // [`crate::Op`]'s plain `run`. `as:` is a type written out, as
    // `text.parse`'s is, read with an `Input`'s rules.
    Operation {
        op: Op::JsonRead,
        namespace: "json",
        name: "read",
        params: &[param("text", TEXT_OR_BYTES), param("as", Kind::Type)],
        result: Kind::Any,
        at_least: None,
        conditions: &[ON_MALFORMED, ON_MISMATCH],
        rust: "json::read_value",
    },
    Operation {
        op: Op::JsonWrite,
        namespace: "json",
        name: "write",
        params: &[param("value", Kind::Any)],
        result: TEXT_VALUE,
        at_least: None,
        conditions: &[],
        rust: "json::write_value",
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
    // Text keys go by code point, as queries sort them, or by a locale's
    // rules with `locale:`, through ICU4X's collator (area 19, question
    // 26); `locale:` means nothing for a key that isn't text.
    list(
        Op::ListSortBy,
        "sort_by",
        &[ITEMS, key(), option("locale", LOCALE)],
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
    // The money operations (area 19, "Money and payments"), each one
    // function in `jadpo-value`'s `money` module. Arithmetic is the
    // operator table's; these say what no operator says. `allocate` and
    // `split` hand every minor unit to some share, the leftover one each
    // to the first shares, so the shares always add up to the total.
    money(
        Op::MoneyAllocate,
        "allocate",
        &[
            param("total", MONEY),
            Param {
                never_negative: true,
                never_empty: true,
                ..param("ratios", Kind::List)
            },
        ],
        Kind::List,
        "money::allocate",
    ),
    // A rate applied, never fetched: the amount must be in the rate's
    // `from` currency, and the result is in its `to`, rounded half to
    // even to a whole minor unit.
    Operation {
        conditions: &[ON_CURRENCY_MISMATCH],
        ..money(
            Op::MoneyConvert,
            "convert",
            &[
                param("value", MONEY),
                param("rate", Kind::Std(crate::standard::EXCHANGE_RATE)),
            ],
            MONEY,
            "money::convert",
        )
    },
    money(
        Op::MoneyCurrency,
        "currency",
        &[param("value", MONEY)],
        CURRENCY,
        "money::currency",
    ),
    money(
        Op::MoneyFromMinor,
        "from_minor",
        &[param("amount", INT), param("currency", CURRENCY)],
        MONEY,
        "money::from_minor",
    ),
    money(
        Op::MoneyMinor,
        "minor",
        &[param("value", MONEY)],
        INT,
        "money::minor",
    ),
    Operation {
        conditions: &[ON_TOO_MANY_PLACES],
        ..money(
            Op::MoneyOf,
            "of",
            &[param("amount", DECIMAL), param("currency", CURRENCY)],
            MONEY,
            "money::of",
        )
    },
    money(
        Op::MoneySplit,
        "split",
        &[
            param("total", MONEY),
            Param {
                never_zero: true,
                never_negative: true,
                ..param("parts", INT)
            },
        ],
        Kind::List,
        "money::split",
    ),
    Operation {
        conditions: &[ON_CURRENCY_MISMATCH],
        ..money(
            Op::MoneySum,
            "sum",
            &[param("items", Kind::List), param("currency", CURRENCY)],
            MONEY,
            "money::sum",
        )
    },
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
    // Whole years, months and days from one date to another (area 19,
    // "Time and calendar additions").
    temporal(
        Op::TemporalPeriodBetween,
        "period_between",
        &[param("start", DATE), param("end", DATE)],
        Kind::Std(crate::standard::CALENDAR_PERIOD),
        "temporal::period_between",
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
    // The text operations (area 19, "Text"), each one function in
    // `jadpo-value`'s `text` module; the contract's three are among them,
    // in order.
    text(
        Op::TextContains,
        "contains",
        &[VALUE, param("part", TEXT)],
        BOOL,
        "text::contains",
    ),
    text(
        Op::TextDropFirst,
        "drop_first",
        &[CUT, count("n")],
        TEXT,
        "text::drop_first",
    ),
    text(
        Op::TextDropLast,
        "drop_last",
        &[CUT, count("n")],
        TEXT,
        "text::drop_last",
    ),
    text(
        Op::TextEndsWith,
        "ends_with",
        &[VALUE, param("suffix", TEXT)],
        BOOL,
        "text::ends_with",
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
            by_type("precision", Kind::Enum(crate::MoneyPrecision::NAME)),
            by_type("display", Kind::Enum(crate::MoneyDisplay::NAME)),
            by_type("sign", Kind::Enum(crate::MoneySign::NAME)),
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
    text(
        Op::TextJoin,
        "join",
        &[ITEMS, param("separator", TEXT)],
        TEXT,
        "text::join",
    ),
    text(
        Op::TextKeepFirst,
        "keep_first",
        &[CUT, count("n")],
        TEXT,
        "text::keep_first",
    ),
    text(
        Op::TextKeepLast,
        "keep_last",
        &[CUT, count("n")],
        TEXT,
        "text::keep_last",
    ),
    Operation {
        at_least: Some(0),
        ..text(Op::TextLength, "length", &[VALUE], INT, "text::length")
    },
    // Unicode's default case mapping, or a locale's with `locale:`
    // (`İ` in Turkish), through ICU4X; either can lengthen text (`ß`
    // becomes `SS`), so neither bounds its length.
    text(
        Op::TextLower,
        "lower",
        &[VALUE, option("locale", LOCALE)],
        TEXT,
        "text::lower",
    ),
    text(
        Op::TextPadStart,
        "pad_start",
        &[
            VALUE,
            param("length", INT),
            Param {
                character: true,
                ..param("fill", TEXT)
            },
        ],
        TEXT,
        "text::pad_start",
    ),
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
    text(
        Op::TextReplace,
        "replace",
        &[VALUE, never_empty("find"), param("with", TEXT)],
        TEXT,
        "text::replace",
    ),
    text(
        Op::TextSlice,
        "slice",
        &[CUT, count("from"), count("count")],
        TEXT,
        "text::slice",
    ),
    text(Op::TextSlug, "slug", &[VALUE], TEXT, "text::slug"),
    text(
        Op::TextSplit,
        "split",
        &[VALUE, never_empty("separator")],
        Kind::List,
        "text::split",
    ),
    text(
        Op::TextStartsWith,
        "starts_with",
        &[VALUE, param("prefix", TEXT)],
        BOOL,
        "text::starts_with",
    ),
    text(Op::TextTrim, "trim", &[CUT], TEXT, "text::trim"),
    text(
        Op::TextUpper,
        "upper",
        &[VALUE, option("locale", LOCALE)],
        TEXT,
        "text::upper",
    ),
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
                    !param.never_negative
                        || param.kind == INT
                        || param.kind == DECIMAL
                        || param.kind == Kind::List,
                    "a count is an `Int`; `sqrt`'s value a `Decimal`; `allocate`'s ratios a list of them"
                );
                assert!(
                    !param.above_zero || param.kind == DECIMAL,
                    "a value above 0 is a `Decimal`"
                );
                assert!(
                    !param.ordered || param.kind == Kind::Function,
                    "a key is inline"
                );
                assert!(
                    !param.written_out || param.kind == INT,
                    "a scale is a whole number"
                );
                assert!(
                    !(param.character || param.bounds_length) || param.kind == TEXT,
                    "one character and a length bound are text's"
                );
                assert!(
                    !param.never_empty || param.kind == TEXT || param.kind == Kind::List,
                    "never empty is text's or a list's"
                );
            }
        }
        for operation in OPERATIONS {
            for condition in operation.conditions {
                let settled = match condition.option {
                    Some(option) => operation
                        .params
                        .iter()
                        .any(|param| param.name == option && param.optional),
                    // What text or bytes decode to is known only when the
                    // program runs, so only an `attempt` settles it.
                    None if matches!(condition.key, RuleKey::Malformed | RuleKey::Mismatch) => true,
                    // `ln`'s by a value proved above 0; the money
                    // operations' by a currency the types or the call fix,
                    // and `convert`'s by a rate's currency a condition
                    // proves.
                    None => {
                        operation.params.iter().any(|param| param.above_zero)
                            || operation.params.iter().any(|param| {
                                param.kind == CURRENCY
                                    || param.kind == Kind::Std(crate::standard::EXCHANGE_RATE)
                            })
                    }
                };
                assert!(
                    settled,
                    "{}'s `{}` is settled by an option it takes, a proof or an `attempt`",
                    operation.qualified(),
                    condition.key.word()
                );
            }
        }
    }
}
