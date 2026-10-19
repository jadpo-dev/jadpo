//! The schedule table (area 12, "Schedules"; chunk 6.2): plain data the
//! plan decides and the runtime runs on each tick, so both executors run
//! a schedule, take its lease and sweep the runtime's own tables in the
//! same way.

/// Every schedule, where their state and leases are kept, and the stores
/// the tick sweeps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchedulesPlan {
    /// In key order.
    pub schedules: Vec<SchedulePlan>,
    /// The store each schedule's state row, lease and run records are in
    /// (`jadpo_schedules`, `jadpo_schedule_runs`): `schedules: { store: }`,
    /// or the only database (area 12, "Outbox placement, settled").
    /// `None` when there are no schedules.
    pub store: Option<String>,
    /// Every store holding the runtime's own tables (the outbox, delivery
    /// records and idempotency keys), which the tick sweeps, by name.
    pub stores: Vec<String>,
}

impl SchedulesPlan {
    /// The schedule whose identity key is `key`.
    pub fn schedule(&self, key: &str) -> Option<&SchedulePlan> {
        self.schedules.iter().find(|schedule| schedule.key == key)
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScheduleRun {
    /// An action, by callable number, with no arguments.
    Action(u32),
    /// The built-in purge of the entity of this name (area 5,
    /// "Lifecycle"; chunk 6.2c), the plan's `purge` of it.
    Purge(String),
}
