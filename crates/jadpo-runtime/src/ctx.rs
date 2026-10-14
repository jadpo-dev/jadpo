//! The per-invocation context (docs/compiler-runtime.md §2.5): what an
//! executor is handed, never what it owns. It keeps the shadow stack of
//! frames both executors push, the depth limit, the faults and failures
//! that capture the stack, the recording of a failed `assert`'s parts, and
//! the state of the action boundary the runtime runs ([`crate::action`]):
//! the transaction it opens at the first store operation and the events
//! waiting to be delivered once it commits. Effects (`read`, `write`,
//! `call`) join it with their areas.

use std::fmt;

use jadpo_value::Value;

use crate::outcome::{Assertion, AssertionDetail, Part};
use crate::{
    Env, Event, Failure, Fault, FaultKind, Frame, Host, MAX_DEPTH, Places, StoreError, Transaction,
};

/// The values an `assert`'s condition gave, site by site, while it runs.
struct Recording {
    /// The asserting callable and the depth of its frame: callees run
    /// deeper, and their values aren't the condition's.
    callable: u32,
    depth: usize,
    values: Vec<(u32, Value)>,
    /// How many inline functions it is inside: their values are per item,
    /// not one a report could show.
    paused: u32,
}

/// An action's boundary while its body runs: opened when the outermost
/// action starts, joined by every action it calls (area 6).
#[derive(Default)]
pub(crate) struct Boundary {
    /// Begun at the first store operation.
    pub transaction: Option<Box<dyn Transaction>>,
    /// The store it was begun in.
    pub store: Option<String>,
    /// What the action emitted, for after it commits.
    pub outbox: Vec<Event>,
    /// A store operation found another transaction had changed what this
    /// one read, so the body is run again.
    pub concurrent_change: bool,
}

/// One invocation's context.
pub struct Ctx<'p> {
    places: &'p Places,
    host: &'p dyn Host,
    env: Env,
    /// Outermost first.
    frames: Vec<Frame>,
    recording: Option<Recording>,
    pub(crate) boundary: Option<Boundary>,
    /// How many boundaries it has opened.
    pub(crate) actions: u32,
    /// How many actions running inside the open boundary were called by
    /// another action, which joined its transaction (area 1).
    pub(crate) nesting: u32,
    /// How many identities it has made.
    ids: u64,
}

impl fmt::Debug for Ctx<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ctx")
            .field("env", &self.env)
            .field("frames", &self.frames)
            .field("recording", &self.recording.is_some())
            .field("in_action", &self.boundary.is_some())
            .finish()
    }
}

impl<'p> Ctx<'p> {
    /// A context for one invocation on `host`, under `env`, naming its
    /// frames from `places`. Its stack is empty until the executor enters
    /// the callable the invocation starts at.
    pub fn new(places: &'p Places, host: &'p dyn Host, env: Env) -> Ctx<'p> {
        Ctx {
            places,
            host,
            env,
            frames: Vec::new(),
            recording: None,
            boundary: None,
            actions: 0,
            nesting: 0,
            ids: 0,
        }
    }

    pub fn places(&self) -> &'p Places {
        self.places
    }

    pub fn host(&self) -> &'p dyn Host {
        self.host
    }

    pub fn env(&self) -> &Env {
        &self.env
    }

    // --- frames -----------------------------------------------------------

    /// How many frames the stack holds.
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Enters `callable`, called from `site` of the current frame. Past
    /// [`MAX_DEPTH`] frames it is the `recursion_limit` fault, at that site.
    pub fn enter(&mut self, callable: u32, site: u32) -> Result<(), Box<Fault>> {
        if self.frames.len() >= MAX_DEPTH {
            return Err(self.fault(FaultKind::RecursionLimit, site));
        }
        self.at(site);
        self.frames.push(Frame {
            callable,
            site: Frame::ENTRY,
        });
        Ok(())
    }

    /// Leaves the current callable.
    pub fn leave(&mut self) {
        self.frames.pop();
    }

    /// Moves the current frame to `site`.
    pub fn at(&mut self, site: u32) {
        if let Some(top) = self.frames.last_mut() {
            top.site = site;
        }
    }

    /// The frames, innermost first.
    pub fn frames(&self) -> Vec<Frame> {
        self.frames.iter().rev().copied().collect()
    }

    /// A fault at `site` of the current frame, with the stack.
    pub fn fault(&mut self, kind: FaultKind, site: u32) -> Box<Fault> {
        self.at(site);
        Box::new(Fault {
            kind,
            frames: self.frames(),
        })
    }

    /// `failure`, raised at `site` of the current frame.
    pub fn fail(&mut self, mut failure: Failure, site: u32) -> Box<Failure> {
        self.at(site);
        failure.frames = self.frames();
        Box::new(failure)
    }

    // --- recording an assertion -------------------------------------------

    /// An `assert` starts working out its condition in the current frame:
    /// from here until [`Ctx::assertion_end`], the values its frame's
    /// expressions give are kept for the report if it fails.
    pub fn assertion_begin(&mut self) {
        self.recording = self.frames.last().map(|frame| Recording {
            callable: frame.callable,
            depth: self.frames.len(),
            values: Vec::new(),
            paused: 0,
        });
    }

    /// Whether a value the current frame works out now is one of the
    /// condition's parts.
    pub fn recording(&self) -> bool {
        self.recording
            .as_ref()
            .is_some_and(|recording| recording.paused == 0 && recording.depth == self.frames.len())
    }

    /// Keeps the value site `site` of the current frame gave, when it is
    /// one of the condition's parts.
    pub fn record(&mut self, site: u32, value: &Value) {
        if self.recording()
            && let Some(recording) = &mut self.recording
        {
            recording.values.push((site, value.clone()));
        }
    }

    /// An inline function runs: its values are per item, so none is kept
    /// until [`Ctx::resume_recording`].
    pub fn pause_recording(&mut self) {
        if let Some(recording) = &mut self.recording {
            recording.paused += 1;
        }
    }

    pub fn resume_recording(&mut self) {
        if let Some(recording) = &mut self.recording {
            recording.paused = recording.paused.saturating_sub(1);
        }
    }

    /// The `assert` is done: the parts it recorded, each site once, in the
    /// order their anchors are written.
    pub fn assertion_end(&mut self) -> Vec<Part> {
        let Some(recording) = self.recording.take() else {
            return Vec::new();
        };
        let sites = self
            .places
            .callable(recording.callable)
            .map(|callable| callable.sites.as_slice())
            .unwrap_or_default();
        let mut seen = std::collections::HashSet::new();
        let mut parts: Vec<Part> = Vec::new();
        for (site, value) in recording.values {
            let Some(place) = sites.get(site as usize) else {
                continue;
            };
            if !seen.insert(site) {
                continue;
            }
            parts.push(Part {
                start: place.start,
                end: place.end,
                anchor: place.anchor,
                value,
            });
        }
        parts.sort_by_key(|part| (part.anchor, std::cmp::Reverse(part.end)));
        parts
    }

    /// The `assert` at `site` of the current frame didn't hold.
    pub fn assertion_failed(&mut self, site: u32, detail: AssertionDetail) -> Box<Assertion> {
        self.at(site);
        Box::new(Assertion {
            frames: self.frames(),
            detail,
            parts: Vec::new(),
        })
    }

    // --- the action boundary ----------------------------------------------

    /// How many actions this invocation has run in a boundary of their
    /// own: each action or fixture a test calls opens one, and an action it
    /// calls joins it.
    pub fn actions(&self) -> u32 {
        self.actions
    }

    /// Whether an action is running: its boundary is open.
    pub fn in_action(&self) -> bool {
        self.boundary.is_some()
    }

    /// Whether the running action was called by another action, whose
    /// transaction it joined: a failure it raises may be handled there, and
    /// the transaction carry on (area 6).
    pub fn nested(&self) -> bool {
        self.nesting > 0
    }

    /// `clock.now`: the time the invocation's clock reads, in milliseconds
    /// since 1970-01-01T00:00:00Z. A test's clock is fixed (area 13); the
    /// same source fills generated fields and a query's `clock.now`.
    pub fn now(&self) -> i64 {
        self.env.clock.now()
    }

    /// The running action's transaction in `store`, begun at the first
    /// store operation (area 6): every action it calls joins it. Outside
    /// an action there is none, and an action writes one store (area 5),
    /// so asking for a second is a fault.
    pub async fn transaction(&mut self, store: &str) -> Result<&mut dyn Transaction, StoreError> {
        let host = self.host;
        let Some(boundary) = &mut self.boundary else {
            return Err(StoreError::Fault(
                "a store operation outside an action".into(),
            ));
        };
        match &boundary.store {
            Some(open) if open != store => {
                return Err(StoreError::Fault(format!(
                    "an action writes one store, and this one already uses `{open}`, not `{store}`"
                )));
            }
            Some(_) => {}
            None => {
                let Some(database) = host.database(store) else {
                    return Err(StoreError::Fault(format!("no store called `{store}`")));
                };
                boundary.transaction = Some(database.begin().await?);
                boundary.store = Some(store.to_string());
            }
        }
        match &mut boundary.transaction {
            Some(transaction) => Ok(transaction.as_mut()),
            None => Err(StoreError::Fault("no transaction".into())),
        }
    }

    /// A new identity: a version 7 UUID (area 5), its first 48 bits the
    /// clock's milliseconds and the rest from the invocation's seed, so a
    /// test that makes records reproduces with its seed.
    pub fn new_id(&mut self) -> [u8; 16] {
        self.ids += 1;
        let mut state = self.env.seed ^ self.ids.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut next = || {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        let (a, b) = (next(), next());
        let millis = u64::try_from(self.env.clock.now()).unwrap_or(0) & 0xFFFF_FFFF_FFFF;
        let mut bytes = [0u8; 16];
        bytes[..6].copy_from_slice(&millis.to_be_bytes()[2..]);
        bytes[6..8].copy_from_slice(&((a & 0x0FFF) as u16 | 0x7000).to_be_bytes());
        bytes[8..].copy_from_slice(&b.to_be_bytes());
        bytes[8] = (bytes[8] & 0x3F) | 0x80;
        bytes
    }

    /// Queues `event` in the running action's outbox, delivered once it
    /// commits (area 12); `false` outside an action.
    pub fn emit(&mut self, event: Event) -> bool {
        match &mut self.boundary {
            Some(boundary) => {
                boundary.outbox.push(event);
                true
            }
            None => false,
        }
    }

    /// A store operation found what this action read changed by another
    /// transaction: the boundary runs the whole action again within its
    /// budget (area 6).
    pub fn concurrent_change(&mut self) {
        if let Some(boundary) = &mut self.boundary {
            boundary.concurrent_change = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallablePlace, MemoryHost, SitePlace};

    fn places() -> Places {
        let mut places = Places::new();
        for (label, line) in [("action place_order", 10), ("function countdown", 20)] {
            places.push(CallablePlace {
                label: label.into(),
                file: "main.jadpo".into(),
                line,
                sites: vec![
                    SitePlace {
                        line: line + 1,
                        start: 5,
                        end: 9,
                        anchor: 7,
                    },
                    SitePlace {
                        line: line + 2,
                        start: 1,
                        end: 9,
                        anchor: 1,
                    },
                ],
            });
        }
        places
    }

    #[test]
    fn faults_carry_the_frames_innermost_first() {
        let places = places();
        let host = MemoryHost::new();
        let mut ctx = Ctx::new(&places, &host, Env::default());
        ctx.enter(0, Frame::ENTRY).expect("room");
        ctx.enter(1, 1).expect("room");
        let fault = ctx.fault(FaultKind::IntOverflow, 0);
        assert_eq!(
            fault.stack(&places),
            [
                "main.jadpo:21  function countdown",
                "main.jadpo:12  action place_order"
            ]
        );
        ctx.leave();
        let failure = ctx.fail(Failure::new("type Gone", "Gone", "gone"), 0);
        assert_eq!(failure.trace(&places), ["action place_order"]);
        assert_eq!(
            failure.site(),
            Some(Frame {
                callable: 0,
                site: 0
            })
        );
    }

    #[test]
    fn the_depth_limit_is_a_fault_at_the_call() {
        let places = places();
        let host = MemoryHost::new();
        let mut ctx = Ctx::new(&places, &host, Env::default());
        ctx.enter(0, Frame::ENTRY).expect("room");
        for _ in 1..MAX_DEPTH {
            ctx.enter(1, 0).expect("room");
        }
        let fault = ctx.enter(1, 1).expect_err("too deep");
        assert_eq!(fault.kind, FaultKind::RecursionLimit);
        assert_eq!(fault.frames.len(), MAX_DEPTH);
        assert_eq!(
            fault.site(),
            Some(Frame {
                callable: 1,
                site: 1
            })
        );
        assert_eq!(fault.kind.classification(), "recursion_limit");
    }

    #[test]
    fn an_assertion_records_its_own_frames_values_once_in_anchor_order() {
        let places = places();
        let host = MemoryHost::new();
        let mut ctx = Ctx::new(&places, &host, Env::default());
        ctx.enter(0, Frame::ENTRY).expect("room");
        ctx.assertion_begin();
        assert!(ctx.recording());
        ctx.record(0, &Value::Int(1));
        ctx.record(0, &Value::Int(2));
        ctx.enter(1, 1).expect("room");
        ctx.record(1, &Value::Int(3));
        ctx.leave();
        ctx.pause_recording();
        ctx.record(1, &Value::Int(4));
        ctx.resume_recording();
        ctx.record(1, &Value::Int(5));
        let parts: Vec<(u32, Value)> = ctx
            .assertion_end()
            .into_iter()
            .map(|part| (part.anchor, part.value))
            .collect();
        assert_eq!(parts, [(1, Value::Int(5)), (7, Value::Int(1))]);
        assert!(!ctx.recording());
    }
}
