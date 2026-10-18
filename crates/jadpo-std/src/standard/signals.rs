//! What `jadpo suggest` finds hand-rolled uses of a standard type by (area
//! 14, "Suggestions"; chunk 5.5x): data on each entry, beside the shape
//! `type.standard_exists` already matches, read by one engine for each
//! kind of signal in `jadpo-check`'s `suggest` module, so a type adds
//! signals by adding data. Operations have no contract entry, so the loops
//! they replace are a table of their own, [`LOOP_SIGNALS`].

use crate::Builtin;

/// One entry's signals. Every part may be empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Signals {
    /// Words in a name that say a value is one of these: `price`,
    /// `pence`. A word matches a name's word, or its plural.
    pub names: &'static [&'static str],
    /// The built-in types a field, parameter or type with one of
    /// [`Signals::names`] must be built on for the `name` signal to fire:
    /// `timeout_ms: Int` is a `Duration` held as a number.
    pub named_on: &'static [Builtin],
    /// The constants code multiplies or divides a value by to convert it
    /// to or from the type, as decimals: `100` for pence to pounds. The
    /// `conversion` signal fires on one only when the other side has a
    /// name with one of [`Signals::names`].
    pub conversions: &'static [&'static str],
    /// Whether the type's units (a quantity's) add their own: every factor
    /// between two of its units as a conversion, and each unit's name
    /// and code (two letters or more) as a name and a format literal.
    pub units: bool,
    /// Text that formats a value by hand when a literal holding it is
    /// joined to one: `£`.
    pub format_literals: &'static [&'static str],
    /// The texts a validation pattern for the type accepts and refuses.
    pub validation: Option<Validation>,
}

/// What a regular expression must accept and refuse to be a type's
/// validation written by hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Validation {
    pub accepts: &'static [&'static str],
    pub refuses: &'static [&'static str],
}

/// No signals: most types.
pub const NO_SIGNALS: Signals = Signals {
    names: &[],
    named_on: &[],
    conversions: &[],
    units: false,
    format_literals: &[],
    validation: None,
};

impl Signals {
    /// Whether it has no signal of any kind.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
            && self.conversions.is_empty()
            && !self.units
            && self.format_literals.is_empty()
            && self.validation.is_none()
    }
}

/// `Money`: amounts in minor units converted by hand (100 for two-place
/// currencies, 1000 for three-place ones), named as prices, and shown with
/// a currency symbol by hand.
pub const MONEY: Signals = Signals {
    names: &["price", "pence", "penny", "cents", "cost"],
    named_on: &[Builtin::Int, Builtin::Decimal],
    conversions: &["100", "1000"],
    units: false,
    format_literals: &["£", "$", "€", "¥"],
    validation: None,
};

/// A quantity: its units' factors, names and codes.
pub const QUANTITY: Signals = Signals {
    names: &[],
    named_on: &[Builtin::Int, Builtin::Decimal],
    conversions: &[],
    units: true,
    format_literals: &[],
    validation: None,
};

/// `Duration`: milliseconds, seconds, minutes and hours held as numbers.
pub const DURATION: Signals = Signals {
    names: &[
        "ms",
        "millis",
        "milliseconds",
        "seconds",
        "secs",
        "minutes",
        "mins",
        "hours",
        "timeout",
    ],
    named_on: &[Builtin::Int, Builtin::Decimal],
    conversions: &["1000", "60", "3600", "86400"],
    units: false,
    format_literals: &[],
    validation: None,
};

/// `Instant`: a moment held as a count since the Unix epoch.
pub const INSTANT: Signals = Signals {
    names: &["epoch", "timestamp", "unix"],
    named_on: &[Builtin::Int],
    conversions: &["1000"],
    units: false,
    format_literals: &[],
    validation: None,
};

/// `Email`: an address held as text, or checked by a pattern of its own.
pub const EMAIL: Signals = Signals {
    names: &["email"],
    named_on: &[Builtin::Text],
    conversions: &[],
    units: false,
    format_literals: &[],
    validation: Some(Validation {
        accepts: &["someone@example.com", "first.last@mail.example.co.uk"],
        refuses: &["someone", "someone@", "@example.com", "some one"],
    }),
};

/// `Url`: a web address held as text, or checked by a pattern of its own.
pub const URL: Signals = Signals {
    names: &["url", "link", "website", "homepage"],
    named_on: &[Builtin::Text],
    conversions: &[],
    units: false,
    format_literals: &[],
    validation: Some(Validation {
        accepts: &["https://example.com/orders", "http://example.com"],
        refuses: &["example.com", "orders", "https//example.com"],
    }),
};

/// `Uuid`: an identifier held as text, or checked by a pattern of its own.
pub const UUID: Signals = Signals {
    names: &["uuid", "guid"],
    named_on: &[Builtin::Text],
    conversions: &[],
    units: false,
    format_literals: &[],
    validation: Some(Validation {
        accepts: &[
            "0192f0c4-9a1e-7b3c-8d2e-4f5a6b7c8d9e",
            "123e4567-e89b-12d3-a456-426614174000",
        ],
        refuses: &[
            "0192f0c4",
            "not-a-uuid",
            "0192f0c49a1e7b3c8d2e4f5a6b7c8d9e1",
        ],
    }),
};

/// The loop a standard operation replaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopShape {
    /// `total = total + <item's value>`: the loop's only statement adds
    /// to a variable declared before it.
    Accumulate,
    /// `if <item's value> < <kept's value> { kept = item }`: keeps the
    /// smallest.
    KeepSmallest,
    /// The same with `>`: keeps the largest.
    KeepLargest,
}

/// One operation's loop signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoopSignal {
    /// The operation, `namespace.name`.
    pub operation: &'static str,
    pub shape: LoopShape,
    /// What the loop does, for the catalogue.
    pub described: &'static str,
}

/// The loops operations replace. `list.group_by`'s builds a map through a
/// chain of calls that the graph shows only as calls, so it has none yet.
pub const LOOP_SIGNALS: &[LoopSignal] = &[
    LoopSignal {
        operation: "list.sum",
        shape: LoopShape::Accumulate,
        described: "a loop adding each item to a total",
    },
    LoopSignal {
        operation: "list.min_by",
        shape: LoopShape::KeepSmallest,
        described: "a loop keeping the item with the smallest value",
    },
    LoopSignal {
        operation: "list.max_by",
        shape: LoopShape::KeepLargest,
        described: "a loop keeping the item with the largest value",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_loop_signal_names_an_operation() {
        for signal in LOOP_SIGNALS {
            let (namespace, name) = signal.operation.split_once('.').expect("qualified");
            assert!(
                crate::operations::operation(namespace, name).is_some(),
                "{}",
                signal.operation
            );
        }
    }

    #[test]
    fn every_conversion_is_a_number_and_every_entry_with_names_says_what_they_are_built_on() {
        for entry in crate::standard::STANDARD_TYPES {
            let signals = entry.signals;
            for constant in signals.conversions {
                assert!(crate::Num::parse(constant).is_some(), "{constant}");
            }
            if !signals.names.is_empty() {
                assert!(!signals.named_on.is_empty(), "{}", entry.name);
            }
        }
    }

    #[test]
    fn the_validation_texts_are_the_types_own() {
        for entry in crate::standard::STANDARD_TYPES {
            let Some(validation) = entry.signals.validation else {
                continue;
            };
            for text in validation.accepts {
                assert!(entry.accepts(text), "{} {text}", entry.name);
            }
            for text in validation.refuses {
                assert!(!entry.accepts(text), "{} {text}", entry.name);
            }
        }
    }
}
