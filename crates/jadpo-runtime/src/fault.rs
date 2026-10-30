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
    /// An `Instant` or `CalendarDate` result outside the years 0000 to
    /// 9999 (area 10).
    TimeRange,
    /// A test moved its clock backwards, which only moves forward (area
    /// 13, "Controlling time").
    ClockBackward,
    /// A call past [`crate::MAX_DEPTH`] frames (area 3, question 14).
    RecursionLimit,
    /// The route's deadline passed while the body ran without waiting:
    /// the executor stopped it (area 7, "Time limits"), and the route
    /// answers 504 `deadline_exceeded` with nothing kept.
    DeadlineExceeded,
    /// Something the language declares that a later chunk builds, as
    /// written: `Todo.get`, `emit`, `clock.now`.
    NotBuiltYet(String),
    /// A defect the checker should have ruled out, in a few words.
    Defect(String),
    /// A store refused in a way no driver classifies, with its reason
    /// (docs/compiler-runtime.md §2.6): a database a test couldn't get, or
    /// an error the server gave.
    Store(String),
    /// A provider's answer the call can't read (area 11, "Decoding";
    /// chunk 7.2c): one that doesn't decode as its operation's output,
    /// named by the operation, the field and the rule, never the value, or
    /// a refusal with a code the operation doesn't declare; or a call
    /// refused before it is sent, because a path placeholder's value
    /// would leave its segment (area 11, "How the input is sent").
    Provider(String),
    /// An outside call, named (`mail.send`), made while the action's
    /// transaction was open, which the runtime refuses before it goes out
    /// (area 6, "Outside calls hold no transaction"; chunk 7.0c).
    CallInTransaction(String),
    /// A rerun of an action with several changing calls that no longer
    /// matches its record, named by the step, its operation and the fields
    /// that differ, never a value (area 12, "A rerun replays"; chunk
    /// 7.5b): stopped before anything more is sent, and held for an
    /// operator rather than failed.
    Diverged(String),
    /// A crash `jadpo test` made at a step of an action with several
    /// changing calls (area 13; chunk 7.5d): the run stops as a process
    /// that died would, nothing it wrote kept, and what ran it starts it
    /// again.
    Crashed,
}

/// The form a bug in Jadpo is reported on. It is public text: no
/// request, header or setting value is ever put in a link to it.
pub const BUG_REPORT_URL: &str = "https://github.com/jadpo-dev/jadpo/issues/new?template=bug.yml";

impl FaultKind {
    /// Whether the fault is a bug in Jadpo rather than in the app: a
    /// defect the checker should have ruled out. The client still gets
    /// `500 internal_fault`; only the operator's log says more.
    pub fn is_jadpo_bug(&self) -> bool {
        matches!(self, FaultKind::Defect(_))
    }

    /// The classification its log event names (area 4, question 9; area
    /// 17): `int_overflow`, `recursion_limit`, and `internal_fault` for any
    /// other defect.
    pub fn classification(&self) -> &'static str {
        match self {
            FaultKind::IntOverflow => "int_overflow",
            FaultKind::DecimalOverflow => "decimal_overflow",
            FaultKind::TimeRange => "time_out_of_range",
            FaultKind::ClockBackward => "clock_backward",
            FaultKind::RecursionLimit => "recursion_limit",
            FaultKind::DeadlineExceeded => "deadline_exceeded",
            FaultKind::NotBuiltYet(_) => "not_built_yet",
            FaultKind::CallInTransaction(_) => "call_in_transaction",
            FaultKind::Diverged(_) => "diverged",
            FaultKind::Crashed => "crashed",
            FaultKind::Defect(_) | FaultKind::Store(_) | FaultKind::Provider(_) => "internal_fault",
        }
    }
}

impl From<jadpo_value::Fault> for FaultKind {
    /// What an operator's or operation's fault is to the runtime: overflow
    /// is the program's, the rest are defects the checker rules out.
    fn from(fault: jadpo_value::Fault) -> FaultKind {
        match fault {
            jadpo_value::Fault::IntOverflow => FaultKind::IntOverflow,
            jadpo_value::Fault::DecimalOverflow => FaultKind::DecimalOverflow,
            jadpo_value::Fault::TimeRange => FaultKind::TimeRange,
            // The interpreter turns a condition into the failure its
            // `attempt` maps; one that reaches here had no `attempt`.
            jadpo_value::Fault::Broke(_) => {
                FaultKind::Defect("a temporal condition no `attempt` maps".into())
            }
            jadpo_value::Fault::DivisionByZero => {
                FaultKind::Defect("a division by zero, which the checker rules out".into())
            }
            jadpo_value::Fault::NoRandomness => {
                FaultKind::Defect("randomness on a host with no generator and no seed".into())
            }
            jadpo_value::Fault::EmptyRange => {
                FaultKind::Defect("`random.integer` given a `min` above its `max`".into())
            }
            jadpo_value::Fault::DateOnlyRule => FaultKind::Defect(
                "a recurrence on dates asked for instants; its days come from a date `from:`"
                    .into(),
            ),
            jadpo_value::Fault::NoCentre => FaultKind::Defect(
                "`geo.centroid` of a shape whose weights cancel, such as two opposite points"
                    .into(),
            ),
            jadpo_value::Fault::ZeroVector => FaultKind::Defect(
                "a cosine distance from a vector of zeros, which has no direction".into(),
            ),
            jadpo_value::Fault::Shape | jadpo_value::Fault::Stopped => {
                FaultKind::Defect("a value of a kind the checker would have refused".into())
            }
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
