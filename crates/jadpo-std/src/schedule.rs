//! The schedule table (area 12, "Schedules"; chunk 6.2): plain data the
//! plan decides and the runtime runs on each tick, so both executors run
//! a schedule, take its lease and sweep the runtime's own tables in the
//! same way.

/// Every schedule, where their state and leases are kept, and the stores
/// the tick sweeps.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SchedulesPlan {
    /// In key order.
    pub schedules: Vec<SchedulePlan>,
    /// The store each schedule's state row, lease and run records are in
    /// (`jadpo_schedules`, `jadpo_schedule_runs`): `schedules: { store: }`,
    /// or the only database (area 12, "Outbox placement, settled").
    /// `None` when this deployable executes no schedules.
    pub store: Option<String>,
    /// Every store holding the runtime's own tables (the outbox, delivery
    /// records and idempotency keys), which the tick sweeps, by name.
    pub stores: Vec<String>,
    /// Whole-application stores holding schedule run history, independent
    /// of the schedules this deployable executes. An hourly cleanup owner
    /// must cover them even when `store` is `None` after pruning.
    pub cleanup_schedules: Vec<String>,
    /// Whole-application stores holding batch runs, problems and cancels,
    /// independent of the batch bodies this deployable reaches.
    pub cleanup_batches: Vec<String>,
    /// The store browser sessions and API tokens are kept in
    /// (`jadpo_sessions`), beside the principal's users, whose ended ones
    /// the tick deletes (area 8; chunk 6.10); `None` when nothing signs one
    /// in.
    pub sessions: Option<String>,
    /// The store service keys are kept in (`jadpo_service_keys`), beside
    /// the principal's services, whose expired ones the tick deletes;
    /// `None` when no service key is declared.
    pub keys: Option<String>,
    /// The store the ids of events received from outside are kept in
    /// (`jadpo_received_events`), whose old ones the tick deletes (area 11,
    /// "Receiving from outside"; chunk 7.7); `None` when nothing is
    /// received.
    pub received: Option<String>,
    /// The stores the records of actions with several changing calls are
    /// kept in (`jadpo_step_records`), whose answers the tick drops and
    /// whose abandoned records it deletes after 30 days (area 12; chunk
    /// 7.5c).
    pub steps: Vec<String>,
    /// The store queue messages are parked in for an operator
    /// (`jadpo_parked_messages`), whose dismissed ones the tick deletes,
    /// beside the queues' events (chunk 7.7c); `None` when the app
    /// receives no queue, and the tick receives each queue's messages.
    pub parked: Option<String>,
    /// The store the counts of what `ai` calls used are kept in
    /// (`jadpo_ai_usage`), whose old days the tick deletes (area 21,
    /// "Budgets"; chunk 5.5v5); `None` when the app asks no model.
    pub ai_usage: Option<String>,
    /// What ticks a native deployable, `schedules: { trigger: }` (area 12,
    /// question 16; chunk 8.2g): the server's own timer, or another
    /// scheduler running its `tick` command. `jadpo dev` ticks on its
    /// timer and a Worker on its cron whichever it is.
    pub trigger: crate::ScheduleTrigger,
}

impl SchedulesPlan {
    /// The schedule whose identity key is `key`.
    pub fn schedule(&self, key: &str) -> Option<&SchedulePlan> {
        self.schedules.iter().find(|schedule| schedule.key == key)
    }

    /// Whether a tick has anything to do: a store to keep schedules'
    /// state in, sweep or delete from. Every step of the tick works in
    /// one, so a program with none is never ticked on Workers, whose cron
    /// trigger would run it for nothing (chunk 8.2f).
    pub fn ticks(&self) -> bool {
        self.store.is_some()
            || !self.stores.is_empty()
            || !self.cleanup_schedules.is_empty()
            || !self.cleanup_batches.is_empty()
            || self.sessions.is_some()
            || self.keys.is_some()
            || self.received.is_some()
            || !self.steps.is_empty()
            || self.parked.is_some()
            || self.ai_usage.is_some()
    }

    /// The shortest `every:`, in milliseconds: how often a host's own
    /// timer has to tick for every schedule to run when it is due.
    pub fn shortest_every(&self) -> Option<u64> {
        self.schedules
            .iter()
            .map(|schedule| schedule.every_ms)
            .min()
    }
}

/// One schedule (area 12, "Schedules").
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SchedulePlan {
    /// Its identity key, which its state row and run records name:
    /// `schedule planning.overdue_reminders`.
    pub key: String,
    /// As a log names it.
    pub name: String,
    /// `every:`, in milliseconds.
    pub every_ms: u64,
    /// What it runs.
    pub run: ScheduleRun,
    /// Where it is written, for logs.
    pub file: String,
    pub line: u32,
}

/// What a schedule runs.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ScheduleRun {
    /// An action, by callable number, with no arguments.
    Action(u32),
    /// The built-in purge of the entity of this name (area 5,
    /// "Lifecycle"; chunk 6.2c), the plan's `purge` of it.
    Purge(String),
}
