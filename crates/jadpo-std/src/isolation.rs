//! The isolation level an action's transaction runs at (area 6, "Counters
//! and other one-step updates"; decided 2026-10-15, built in chunk 3.7),
//! which follows from what the action does: `jadpo-check` decides it for
//! each action and fixture from the graph, `jadpo-plan` carries it, and
//! `jadpo-runtime` begins the transaction at it, for both executors.

/// How much of what other transactions commit an action's transaction
/// sees while it runs.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum Isolation {
    /// Each statement sees what was committed before it began: for an
    /// action that loads no record, itself or through any action it calls.
    /// Of two counting the same row at once, the second waits for the
    /// first, adds to its count and commits, and neither reruns.
    ReadCommitted,
    /// One snapshot, taken at the first statement: for every other action.
    /// A write to a row another transaction changed since is a concurrent
    /// change, and the whole action runs again (area 6, "All-or-nothing
    /// actions and concurrent changes"). What runs outside an action
    /// (migrations, purge, tools) runs here too.
    #[default]
    RepeatableRead,
}

impl Isolation {
    pub const ALL: [Isolation; 2] = [Isolation::ReadCommitted, Isolation::RepeatableRead];

    /// As the manifest writes it: `read_committed`.
    pub fn word(self) -> &'static str {
        match self {
            Isolation::ReadCommitted => "read_committed",
            Isolation::RepeatableRead => "repeatable_read",
        }
    }

    /// The level of an action that loads a record, or doesn't.
    pub fn of(loads: bool) -> Isolation {
        if loads {
            Isolation::RepeatableRead
        } else {
            Isolation::ReadCommitted
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_action_that_loads_a_record_runs_at_repeatable_read() {
        assert_eq!(Isolation::of(false), Isolation::ReadCommitted);
        assert_eq!(Isolation::of(true), Isolation::RepeatableRead);
        assert_eq!(Isolation::default(), Isolation::RepeatableRead);
        let words: Vec<&str> = Isolation::ALL.iter().map(|level| level.word()).collect();
        assert_eq!(words, ["read_committed", "repeatable_read"]);
    }
}
