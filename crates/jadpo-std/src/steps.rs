//! The record of an action with several changing calls (area 12, "Several
//! changing calls in one action"; area 11, the same; chunk 7.5b): plain
//! data the plan decides and the runtime keeps, so both executors record
//! and replay steps the same way.

/// What the runtime keeps for an action whose longest way makes several
/// changing calls, when it is run where its key survives a retry: by a
/// subscriber, or by a route whose API requires `Idempotency-Key`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RecordPlan {
    /// The store its record is kept in: the one it writes, or the one
    /// `events: { store: }` names, beside its outbox.
    pub store: String,
    /// A digest of its step shape: the outside calls on each way through
    /// it, those of the actions it calls included (area 11, "Deploys").
    pub shape: String,
}

/// The largest answer a step keeps, in bytes: a larger one is a fault
/// naming the step (area 12, "Settled for the build").
pub const MOST_ANSWER_BYTES: usize = 1 << 20;

/// The margin a step in doubt is sent again within, short of its
/// provider's `window:`: the smaller of a tenth of the window and an hour
/// (area 11, "Keys and windows").
pub fn window_margin_ms(window_ms: u64) -> u64 {
    (window_ms / 10).min(60 * 60 * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_margin_is_a_tenth_of_the_window_at_most_an_hour() {
        assert_eq!(window_margin_ms(24 * 60 * 60 * 1000), 60 * 60 * 1000);
        assert_eq!(window_margin_ms(60 * 60 * 1000), 6 * 60 * 1000);
        assert_eq!(window_margin_ms(0), 0);
    }
}
