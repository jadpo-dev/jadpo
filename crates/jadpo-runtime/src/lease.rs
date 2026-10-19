//! Leases on the runtime's own rows (chunk 6.7, from the Phase 6
//! architecture review): one way background work claims a delivery's
//! record, a schedule's state row or a batch run's row, renews it and
//! gives it back.
//!
//! A claim is one conditional statement, so two runners never both hold
//! a row, and it lasts what is left of the claimer's budget and a margin
//! ([`until`]): the claimer has stopped and given it back by then, so a
//! claimer that died loses it no later. A batch run renews its lease with
//! each page it commits, in the page's own transaction. Each kind of row
//! says how it is claimed and given back ([`Leased`]).

use crate::budget::{Budget, RUN_LIMIT_MS};
use crate::{Database, Row, SqlValue, StoreError};

/// How much longer than what is left of the claimer's budget a lease
/// lasts: time for a claimer cut short to give back what it holds, and
/// for clocks that differ a little between instances.
pub const MARGIN_MS: i64 = 60_000;

/// The longest any lease lasts: a whole budget, which is at most the run
/// limit, and the margin. How long `jadpo schedules run` waits for a run
/// a deployment's tick took, since it can't know the deployment's budget.
pub const LONGEST_MS: i64 = RUN_LIMIT_MS as i64 + MARGIN_MS;

/// A row background work claims for a while.
pub(crate) trait Leased {
    /// The statement that claims it at `now` until `until`, and what it
    /// binds: one statement, giving a row when it was claimed.
    fn claim(&self, now: i64, until: i64) -> (&'static str, Vec<SqlValue>);

    /// The statement that gives it back at `now`, while it is still this
    /// claimer's, and what it binds: giving a row when it was.
    fn release(&self, now: i64) -> (&'static str, Vec<SqlValue>);
}

/// When a lease taken or renewed at `now` within `budget` ends: what is
/// left of the budget and [`MARGIN_MS`] from now.
pub fn until(budget: &Budget<'_>, now: i64) -> i64 {
    now.saturating_add(i64::try_from(budget.left_ms()).unwrap_or(i64::MAX))
        .saturating_add(MARGIN_MS)
}

/// Claims `row` in `database` at `now` for as long as `budget` lasts and
/// the margin: the claim's first row, or `None` when another holds it or
/// it isn't there to claim.
pub(crate) async fn claim(
    database: &dyn Database,
    budget: &Budget<'_>,
    row: &dyn Leased,
    now: i64,
) -> Result<Option<Row>, StoreError> {
    let (sql, params) = row.claim(now, until(budget, now));
    Ok(database.query_once(sql, &params).await?.into_iter().next())
}

/// Gives `row` back at `now`: whether it was still this claimer's. A
/// store that can't be reached keeps it until its lease ends.
pub(crate) async fn release(database: &dyn Database, row: &dyn Leased, now: i64) -> bool {
    let (sql, params) = row.release(now);
    database
        .query_once(sql, &params)
        .await
        .is_ok_and(|rows| !rows.is_empty())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::budget::Invocation;
    use crate::{Host, LocalFuture};

    struct Clocked {
        now: Cell<u64>,
    }

    impl Host for Clocked {
        fn database(&self, _store: &str) -> Option<&dyn Database> {
            None
        }

        fn random(&self, _bytes: &mut [u8]) -> bool {
            false
        }

        fn monotonic_ms(&self) -> Option<u64> {
            Some(self.now.get())
        }

        fn budget_ms(&self, _invocation: Invocation) -> u64 {
            30_000
        }

        fn sleep(&self, _millis: u64) -> LocalFuture<'_, ()> {
            Box::pin(std::future::pending())
        }
    }

    #[test]
    fn a_lease_lasts_what_is_left_of_the_budget_and_a_margin() {
        let host = Clocked { now: Cell::new(0) };
        let budget = Budget::start(&host, Invocation::AfterResponse);
        assert_eq!(until(&budget, 1_000_000), 1_000_000 + 30_000 + 60_000);
        // Renewed later in the invocation, it lasts less.
        host.now.set(20_000);
        assert_eq!(until(&budget, 1_020_000), 1_020_000 + 10_000 + 60_000);
        // Never longer than a whole run and the margin.
        let none = crate::MemoryHost::new();
        let generous = Budget::start(&none, Invocation::Tick);
        assert_eq!(until(&generous, 0), LONGEST_MS);
    }
}
