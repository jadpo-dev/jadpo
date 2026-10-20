//! The development call log (area 17, "Provider calls in development";
//! area 11; chunk 7.2d): each provider call `jadpo dev` makes, faked or
//! let through to the sandbox, with what was sent and what came back in
//! full, which `jadpo calls` lists.
//!
//! The capability seam logs every call on its invocation without its
//! values ([`crate::capability::Logged`]), which is all production keeps.
//! A host that keeps a development log gives a [`CallLog`]
//! ([`crate::Host::call_log`]), and a provider call's site hands it each
//! call as it ends ([`Seen`]): its request as sent, but never its headers,
//! which hold the credential, and its answer as the provider sent it, or
//! as the fake generated it.

use crate::capability::{Ended, Source};

/// One provider call, as the development log keeps it.
#[derive(Clone, Copy)]
pub struct Seen<'a> {
    pub connection: &'a str,
    /// `pickups.create`.
    pub operation: &'a str,
    /// The request it was made for, when it was made for one.
    pub request_id: Option<&'a str>,
    /// When it went out, in milliseconds since 1970-01-01T00:00:00Z.
    pub at: i64,
    pub source: Source,
    pub took_ms: Option<u64>,
    /// The request as sent: its method, address and body, never its
    /// headers.
    pub method: &'a str,
    pub url: &'a str,
    pub body: Option<&'a str>,
    /// The answer as the provider sent it, or as the fake generated it;
    /// `None` when none came.
    pub answer: Option<&'a str>,
    pub ended: &'a Ended,
    /// For a model asked a prompt, the prompt's values as the code gave
    /// them, each field as its type's JSON (area 21; chunk 5.5v4b): what
    /// `jadpo ai record` asks the model with again.
    pub values: Option<&'a str>,
}

impl std::fmt::Debug for Seen<'_> {
    /// Never its values.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Seen")
            .field("connection", &self.connection)
            .field("operation", &self.operation)
            .field("source", &self.source)
            .field("ended", &self.ended)
            .finish_non_exhaustive()
    }
}

/// A delivery whose subscriber's action kept a record of several changing
/// calls, as an attempt at it ended (chunk 7.5f): what became of each step
/// it reached, never a request, an answer or a key.
#[derive(Clone, Copy, Debug)]
pub struct DeliveryAttempt<'a> {
    pub delivery: [u8; 16],
    /// The subscriber, as a log names it.
    pub subscriber: &'a str,
    /// The state the attempt left it in: `succeeded`, `pending` (to be
    /// tried again), `failed`, `outcome_unknown` or `diverged`.
    pub state: &'a str,
    pub steps: &'a [crate::steps::RunSteps],
}

/// Where a host keeps its development call log.
pub trait CallLog {
    /// Keeps `seen`.
    fn seen(&self, seen: &Seen<'_>);

    /// Hears a delivery whose action reached steps under a record, as an
    /// attempt at it ends: `jadpo dev` prints its line (chunk 7.5f).
    fn attempt_ended(&self, attempt: &DeliveryAttempt<'_>) {
        let _ = attempt;
    }
}
