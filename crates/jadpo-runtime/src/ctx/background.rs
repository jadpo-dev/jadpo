//! What an invocation carries for background work (area 12; chunks 6.1a
//! to 6.7, gathered in 7.0b): whether the system runs it and for which
//! tenant, the subscriber's delivery whose handled record commits with
//! it, how deep in a chain of events it is and its trace, the work left
//! for after its response, and how many events the actions it ran kept.
//! The runner sets it, and the action boundary reads it.

use crate::System;
use crate::outbox::Trace;
use crate::runner::Deferred;

/// A subscriber's delivery being run: the handled record its action
/// commits with its writes, in the store it writes (area 12, chunk 6.1b).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Handled {
    pub store: String,
    /// The delivery's `id`.
    pub id: [u8; 16],
}

/// One invocation's background-run state.
#[derive(Default)]
pub(crate) struct Background {
    /// The system, when the runner runs a schedule's or a subscriber's
    /// work; never alongside a caller.
    system: Option<System>,
    /// The subscriber's delivery being run, whose handled record the
    /// outermost action commits.
    handled: Option<Handled>,
    /// How many events led to the work under way from the request that
    /// set the chain off: 0 for a request (area 12, "Limits").
    chain: u32,
    /// The trace context of the work under way (area 12, question 17): a
    /// request's, or the event's a delivery continues.
    trace: Option<Trace>,
    /// What runs after the response, in the order it was left: what
    /// committed actions emitted, and runs the runner was handed (chunk
    /// 6.1a).
    deferred: Vec<Deferred>,
    /// How many events the actions it ran kept in the outbox once they
    /// committed, which a schedule's run record counts (area 12,
    /// "Watching schedules").
    emitted: u32,
    /// How a test delivers what its steps emit (area 13, "Caches and
    /// events"; chunk 6.11): set by `jadpo test`, `None` anywhere else.
    between_steps: Option<crate::test_steps::BetweenSteps>,
}

impl Background {
    pub(crate) fn system(&self) -> Option<System> {
        self.system
    }

    pub(crate) fn set_system(&mut self, system: Option<System>) {
        self.system = system;
    }

    pub(crate) fn handled(&self) -> Option<&Handled> {
        self.handled.as_ref()
    }

    /// Runs what follows as subscriber's delivery `handled`, or, with
    /// `None`, as none.
    pub(crate) fn set_handled(&mut self, handled: Option<Handled>) {
        self.handled = handled;
    }

    pub(crate) fn chain(&self) -> u32 {
        self.chain
    }

    pub(crate) fn set_chain(&mut self, chain: u32) {
        self.chain = chain;
    }

    pub(crate) fn trace(&self) -> Option<Trace> {
        self.trace
    }

    pub(crate) fn set_trace(&mut self, trace: Option<Trace>) {
        self.trace = trace;
    }

    pub(crate) fn defer(&mut self, work: Deferred) {
        self.deferred.push(work);
    }

    /// Leaves each of `work` to run after the response, after what is left
    /// already, in order.
    pub(crate) fn defer_all(&mut self, work: impl IntoIterator<Item = Deferred>) {
        self.deferred.extend(work);
    }

    pub(crate) fn deferred(&self) -> &[Deferred] {
        &self.deferred
    }

    pub(crate) fn take_deferred(&mut self) -> Vec<Deferred> {
        std::mem::take(&mut self.deferred)
    }

    /// Takes the work left for after the response from the `from`th piece
    /// on: what was left since it held `from` pieces.
    pub(crate) fn take_deferred_from(&mut self, from: usize) -> Vec<Deferred> {
        let from = from.min(self.deferred.len());
        self.deferred.split_off(from)
    }

    pub(crate) fn between_steps(&self) -> Option<&crate::test_steps::BetweenSteps> {
        self.between_steps.as_ref()
    }

    pub(crate) fn between_steps_mut(&mut self) -> Option<&mut crate::test_steps::BetweenSteps> {
        self.between_steps.as_mut()
    }

    pub(crate) fn set_between_steps(&mut self, between: crate::test_steps::BetweenSteps) {
        self.between_steps = Some(between);
    }

    pub(crate) fn emitted(&self) -> u32 {
        self.emitted
    }

    /// Counts `events` more kept in the outbox by an action that
    /// committed.
    pub(crate) fn count_kept(&mut self, events: u32) {
        self.emitted = self.emitted.saturating_add(events);
    }
}
