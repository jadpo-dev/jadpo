//! Faults: what the runtime raises by itself (area 4, "Faults"). They are
//! never declared, listed in `fails`, rejected or matched; they go straight
//! to the boundary, which answers `internal_fault`, and carry the frames of
//! where they happened.

use crate::{Frame, Places};

/// What went wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FaultKind {
    /// An `Int` result past the 64-bit range (area 2, question 8).
    IntOverflow,
    /// A `Decimal` result past 38 digits.
    DecimalOverflow,
    /// A call past [`crate::MAX_DEPTH`] frames (area 3, question 14).
    RecursionLimit,
    /// Something the language declares that a later chunk builds, as
    /// written: `Todo.get`, `emit`, `clock.now`.
    NotBuiltYet(String),
    /// A defect the checker should have ruled out, in a few words.
    Defect(String),
}

impl FaultKind {
    /// The classification its log event names (area 4, question 9; area
    /// 17): `int_overflow`, `recursion_limit`, and `internal_fault` for any
    /// other defect.
    pub fn classification(&self) -> &'static str {
        match self {
            FaultKind::IntOverflow => "int_overflow",
            FaultKind::DecimalOverflow => "decimal_overflow",
            FaultKind::RecursionLimit => "recursion_limit",
            FaultKind::NotBuiltYet(_) => "not_built_yet",
            FaultKind::Defect(_) => "internal_fault",
        }
    }
}

/// A fault and where it happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fault {
    pub kind: FaultKind,
    /// The shadow stack when it happened, innermost first: the first
    /// frame's site is where.
    pub frames: Vec<Frame>,
}

impl Fault {
    /// The frame it happened in.
    pub fn site(&self) -> Option<Frame> {
        self.frames.first().copied()
    }

    /// Its frames as area 17 prints them, innermost first:
    /// `domains/ordering/actions/price_line.jadpo:4  action price_line`.
    pub fn stack(&self, places: &Places) -> Vec<String> {
        self.frames
            .iter()
            .filter_map(|frame| places.place(*frame))
            .map(|place| place.to_string())
            .collect()
    }
}
