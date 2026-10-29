//! One record of what an invocation did (docs/compiler-runtime.md §2.6,
//! "What an invocation did"; roadmap chunk 10.3c): a span for each action,
//! read, write and outside call, kept on its [`crate::Ctx`] in the order
//! each started, whichever executor runs it.
//!
//! Each is appended in one place: an action at the runtime's action
//! boundary ([`crate::action`]), a read or a write at the store's entry
//! point ([`crate::store`]), an outside call where every one goes out
//! ([`crate::effects::call`] and the other callers of the one seam). A
//! request's spans go to its host's [`crate::serve::Observer`] with the
//! rest of its answer ([`crate::serve::Answered::spans`]), and a test's
//! into its record ([`crate::record::Entry::Span`]), so the differential
//! run holds the interpreter and compiled code to the same ones.
//!
//! A span names where it is by a frame of the program, never by a value
//! (area 16, "Telemetry": names, identifiers and failure codes). Its start
//! and end are on the host's monotonic clock, `None` on a host without
//! one, as a test's has none.

use crate::{Failure, Fault, Frame};

/// The most spans one invocation keeps; past it each is counted, not kept
/// ([`Spans::dropped`]), so a long piece of background work holds a
/// bounded list.
pub const MAX_SPANS: usize = 10_000;

/// What a span is of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An action, from its boundary's start to its end, every run of it
    /// after a concurrent change included.
    Action,
    /// A query, an entity's `get`, or a read the runtime runs on a
    /// route's behalf.
    Read,
    /// A `create`, `update` or `delete`, a lifecycle's transitions among
    /// them.
    Write,
    /// An outside call through connection `connection`, from its first
    /// send to its last answer, its retries included.
    Call {
        connection: String,
        operation: String,
    },
}

impl Kind {
    /// How a record writes it: `action`, `read`, `write`, `call
    /// stripe.charges.create`.
    pub fn word(&self) -> String {
        match self {
            Kind::Action => "action".to_string(),
            Kind::Read => "read".to_string(),
            Kind::Write => "write".to_string(),
            Kind::Call {
                connection,
                operation,
            } => format!("call {connection}.{operation}"),
        }
    }
}

/// How a span ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ended {
    /// It hasn't: the invocation stopped while it was running, as a route
    /// whose deadline passed abandons its work.
    Open,
    /// It gave a value.
    Value,
    /// It failed with this code: its own failure, `not_found`'s, a
    /// refused write's `constraint_refused`, or what the runtime raises in
    /// a call's place (`unavailable`, `outcome_unknown`, `misconfigured`).
    Failed(String),
    /// It faulted, with this classification (`defect`, `int_overflow`).
    Fault(&'static str),
}

impl Ended {
    /// How a failure ends one.
    pub fn failed(failure: &Failure) -> Ended {
        Ended::Failed(failure.code.clone())
    }

    /// How a fault ends one.
    pub fn fault(fault: &Fault) -> Ended {
        Ended::Fault(fault.kind.classification())
    }

    /// How a record writes it: `open`, `value`, `failed todo_not_found`,
    /// `fault int_overflow`.
    pub fn word(&self) -> String {
        match self {
            Ended::Open => "open".to_string(),
            Ended::Value => "value".to_string(),
            Ended::Failed(code) => format!("failed {code}"),
            Ended::Fault(classification) => format!("fault {classification}"),
        }
    }
}

/// One thing an invocation did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub kind: Kind,
    /// Where it is, with [`Span::site`]: for an action, the action itself,
    /// at its declaration ([`Frame::ENTRY`]); for a read, a write or a
    /// call, the callable on top of the stack that made it. `None` for one
    /// the runtime makes on its own behalf: a route's query, a call of
    /// its own.
    pub callable: Option<u32>,
    /// The site of `callable` it is at: its declaration for an action, the
    /// operation's own site for anything else.
    pub site: u32,
    /// When it started and ended on the host's monotonic clock
    /// ([`crate::Host::monotonic_ms`]); `None` on a host without one, and
    /// the end while it is open.
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    /// How many records a read gave or a write wrote; `None` for an action
    /// or a call, and for a read or a write that didn't finish.
    pub rows: Option<u64>,
    pub ended: Ended,
}

impl Span {
    /// Where it is as a frame of the program, for [`crate::Places::place`]:
    /// `None` for one the runtime made on its own behalf.
    pub fn frame(&self) -> Option<Frame> {
        self.callable.map(|callable| Frame {
            callable,
            site: self.site,
        })
    }

    /// How long it took, in milliseconds on the host's monotonic clock:
    /// `None` while open or on a host without one.
    pub fn ms(&self) -> Option<u64> {
        Some(self.end_ms?.saturating_sub(self.start_ms?))
    }
}

/// The spans of one invocation, in the order each started.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spans {
    list: Vec<Span>,
    /// How many started past [`MAX_SPANS`], not kept.
    dropped: u32,
}

/// Which span [`Spans::begin`] started, for [`Spans::end`]: `None` when
/// it wasn't kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Begun(Option<usize>);

impl Spans {
    /// Starts a span of `kind` at `frame`, at `now` on the host's
    /// monotonic clock.
    pub(crate) fn begin(&mut self, kind: Kind, frame: Option<Frame>, now: Option<u64>) -> Begun {
        if self.list.len() >= MAX_SPANS {
            self.dropped = self.dropped.saturating_add(1);
            return Begun(None);
        }
        self.list.push(Span {
            kind,
            callable: frame.map(|frame| frame.callable),
            site: frame.map_or(Frame::ENTRY, |frame| frame.site),
            start_ms: now,
            end_ms: None,
            rows: None,
            ended: Ended::Open,
        });
        Begun(Some(self.list.len() - 1))
    }

    /// Ends the span `begun` started, at `now`, as `ended`, having given
    /// or written `rows` records.
    pub(crate) fn end(&mut self, begun: Begun, now: Option<u64>, rows: Option<u64>, ended: Ended) {
        let Some(span) = begun.0.and_then(|at| self.list.get_mut(at)) else {
            return;
        };
        span.end_ms = now;
        span.rows = rows;
        span.ended = ended;
    }

    /// Every span kept, in the order each started.
    pub fn iter(&self) -> std::slice::Iter<'_, Span> {
        self.list.iter()
    }

    pub fn as_slice(&self) -> &[Span] {
        &self.list
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// How many started past [`MAX_SPANS`], which it didn't keep.
    pub fn dropped(&self) -> u32 {
        self.dropped
    }
}

impl<'s> IntoIterator for &'s Spans {
    type Item = &'s Span;
    type IntoIter = std::slice::Iter<'s, Span>;

    fn into_iter(self) -> Self::IntoIter {
        self.list.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Past its limit a list counts what it doesn't keep, and ending one
    /// it didn't keep changes nothing.
    #[test]
    fn past_its_limit_a_list_counts_what_it_does_not_keep() {
        let mut spans = Spans::default();
        for _ in 0..MAX_SPANS {
            let begun = spans.begin(Kind::Read, None, Some(1));
            spans.end(begun, Some(2), Some(1), Ended::Value);
        }
        let past = spans.begin(Kind::Write, None, Some(3));
        spans.end(past, Some(4), Some(1), Ended::Value);
        assert_eq!(spans.len(), MAX_SPANS);
        assert_eq!(spans.dropped(), 1);
        assert!(spans.iter().all(|span| span.kind == Kind::Read));
        assert_eq!(spans.as_slice()[0].ms(), Some(1));
    }
}
