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

/// Where a host keeps its development call log.
pub trait CallLog {
    /// Keeps `seen`.
    fn seen(&self, seen: &Seen<'_>);
}
