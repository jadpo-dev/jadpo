//! Crash points (area 13, "Provider calls and capabilities in tests";
//! area 11, "Tests and tools"; chunk 7.5d): what `jadpo test` makes of a
//! test that reaches an action with several changing calls.
//!
//! A test's context counts each step it reaches under a record (each time
//! [`crate::steps`] writes or sends one, in order, across every action the
//! test runs), and names it ([`Reached`]). Told to crash at the `n`th at
//! one of its three boundaries ([`Ctx::crash_at`]), the run stops there
//! once, as a process that died would: before the step is sent, once it
//! is sent and before its answer is kept, or once its answer is kept. The
//! action ends with [`FaultKind::Crashed`], nothing it wrote is kept, and
//! what runs it starts it again as a restart would: a delivery is given
//! back and delivered again ([`crate::runner`]), a route's request is sent
//! again under its key ([`crate::http`]). The rerun takes up its record
//! from where the crash left it.
//!
//! Only `jadpo test` turns this on ([`Ctx::track_steps`]); everything else
//! keeps no count.
//!
//! [`FaultKind::Crashed`]: crate::FaultKind::Crashed

use crate::Ctx;

/// Where at a step a crash happens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Boundary {
    /// Its record says it is sent; it never went out.
    BeforeSend,
    /// It went out and was answered; the answer was never kept.
    AfterSend,
    /// Its answer is kept; nothing after it ran.
    AfterAnswer,
}

impl Boundary {
    /// The three, in the order a step passes them.
    pub const ALL: [Boundary; 3] = [
        Boundary::BeforeSend,
        Boundary::AfterSend,
        Boundary::AfterAnswer,
    ];

    /// How `--json` names it.
    pub fn word(self) -> &'static str {
        match self {
            Boundary::BeforeSend => "before_send",
            Boundary::AfterSend => "after_send",
            Boundary::AfterAnswer => "after_answer",
        }
    }
}

/// A step a test's run reached under a record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reached {
    /// The action whose record it is, as written: `pay_invoice`.
    pub action: String,
    /// Its place along the action's way, from 1.
    pub position: u32,
    /// `pay.payments.create`, `sheets.put`.
    pub operation: String,
}

/// A context's crash points: off, unless `jadpo test` turns them on.
#[derive(Clone, Debug, Default)]
pub(crate) struct Crashes {
    tracking: bool,
    /// The step, counted from 1 across the run, and the boundary to crash
    /// at.
    plan: Option<(usize, Boundary)>,
    fired: bool,
    reached: Vec<Reached>,
}

impl Ctx<'_> {
    /// Counts each step the run reaches under a record, for
    /// [`Ctx::steps_reached`]: what `jadpo test` turns on for a test with a
    /// database.
    pub fn track_steps(&mut self) {
        self.crashes_mut().tracking = true;
    }

    /// Crashes the run once, at the `at`th step it reaches (from 1), at
    /// `boundary`; counts its steps too.
    pub fn crash_at(&mut self, at: usize, boundary: Boundary) {
        let crashes = self.crashes_mut();
        crashes.tracking = true;
        crashes.plan = Some((at, boundary));
        crashes.fired = false;
    }

    /// The steps the run reached under a record, in order, a step sent
    /// again counted again.
    pub fn steps_reached(&self) -> &[Reached] {
        &self.crashes().reached
    }

    /// Whether the crash [`Ctx::crash_at`] asked for happened.
    pub fn crashed(&self) -> bool {
        self.crashes().fired
    }
}

/// Counts step `position` of `action`'s record, a call of `operation`, as
/// reached, when steps are counted.
pub(crate) fn reached(ctx: &mut Ctx<'_>, action: &str, position: u32, operation: &str) {
    let crashes = ctx.crashes_mut();
    if crashes.tracking {
        crashes.reached.push(Reached {
            action: action.to_string(),
            position,
            operation: operation.to_string(),
        });
    }
}

/// Whether the run crashes now, at `boundary` of the step it reached
/// last: once, at the step and boundary it was told to.
pub(crate) fn due(ctx: &mut Ctx<'_>, boundary: Boundary) -> bool {
    let crashes = ctx.crashes_mut();
    let now = crashes.plan == Some((crashes.reached.len(), boundary)) && !crashes.fired;
    if now {
        crashes.fired = true;
    }
    now
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crash_happens_once_at_its_step_and_boundary() {
        let places = crate::Places::new();
        let host = crate::MemoryHost::new();
        let mut ctx = Ctx::new(&places, &host, crate::Env::default());
        reached(&mut ctx, "pay_invoice", 1, "pay.payments.create");
        assert!(
            ctx.steps_reached().is_empty(),
            "nothing counted until asked"
        );
        ctx.crash_at(2, Boundary::AfterSend);
        reached(&mut ctx, "pay_invoice", 1, "pay.payments.create");
        assert!(!due(&mut ctx, Boundary::AfterSend));
        reached(&mut ctx, "pay_invoice", 2, "sheets.put");
        assert!(!due(&mut ctx, Boundary::BeforeSend));
        assert!(due(&mut ctx, Boundary::AfterSend));
        assert!(ctx.crashed());
        assert!(!due(&mut ctx, Boundary::AfterSend), "only once");
        assert_eq!(ctx.steps_reached().len(), 2);
        assert_eq!(ctx.steps_reached()[1].operation, "sheets.put");
    }
}
