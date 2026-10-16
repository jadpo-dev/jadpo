//! The action boundary (docs/compiler-runtime.md §2.5), which the runtime
//! owns rather than either executor: every action is all-or-nothing (areas
//! 3 and 6). The outermost action opens the boundary; the body, which the
//! executor supplies, runs; the transaction is begun at the body's first
//! store operation ([`Ctx::transaction`]), at the isolation level the body
//! names (area 6, "Counters and other one-step updates"); when another
//! transaction changed what it read, the whole body runs again within the
//! budget; then it commits, or rolls back when the body failed, and only
//! after a commit are the events it emitted delivered. An action called
//! inside another joins its transaction (area 1, "Cross-domain writes"),
//! at the caller's level, which already counts what every action it calls
//! does; so does a fixture's, since a fixture is a test-only action (area
//! 13); a test, or a fixture's caller, opens a boundary per action it
//! calls, so each commits on its own. A called action whose failure its
//! caller handles (an outcome `match`) runs inside a savepoint: when it
//! fails, what it wrote and emitted is undone and the caller carries on,
//! since the action is all-or-nothing whoever calls it (area 6).

use crate::{Ctx, Failure, Isolation, LocalFuture, Outcome, StoreError};

/// How many times a body runs before a concurrent change gives up with
/// `unavailable` (area 6: "after three attempts within a second, the
/// existing retry budget"). The second's bound comes with the host's
/// clock.
pub const RERUN_ATTEMPTS: u32 = 3;

/// An action's body, as an executor runs it; the boundary may run it more
/// than once, each time from the start.
pub trait ActionBody<'p> {
    fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome>;

    /// The level a boundary of its own begins its transaction at: read
    /// committed when the action loads no record, itself or through any
    /// action it calls, repeatable read otherwise, as its plan says (area
    /// 6). Joining a running action's boundary, it takes that action's.
    fn isolation(&self) -> Isolation;

    /// Whether the caller handles this action's failure (an outcome
    /// `match` or `assert … fails` on the call) rather than passing it on:
    /// then, inside a running action, it runs in a savepoint its failure
    /// returns to.
    fn caught(&self) -> bool {
        false
    }
}

/// What the runtime raises when an action can't finish because its store
/// can't (area 4: `unavailable`, 503, "a dependency is temporarily
/// unavailable"; raised by the runtime).
pub fn unavailable() -> Failure {
    let mut failure = Failure::new("runtime unavailable", "Unavailable", "unavailable");
    failure.kind = Some("unavailable".to_string());
    failure.status = Some(503);
    failure
}

/// How a run of the body ended at the store.
enum Ended {
    Committed,
    RolledBack,
    /// Another transaction changed what it read: run it again.
    Changed,
    /// The store couldn't finish.
    Unavailable,
}

/// Runs `body` as an action: inside the running action's boundary when
/// there is one, otherwise in a boundary of its own, which commits or
/// rolls back, reruns on a concurrent change, and delivers what the body
/// emitted after it commits. Asked to hold ([`Ctx::hold`]), a boundary of
/// its own leaves a succeeding body's transaction and outbox open for
/// [`commit_held`] instead.
pub async fn action<'p>(ctx: &mut Ctx<'p>, body: &mut dyn ActionBody<'p>) -> Outcome {
    if ctx.in_action() {
        return if body.caught() {
            caught(ctx, body).await
        } else {
            body.run(ctx).await
        };
    }
    ctx.actions += 1;
    let hold = std::mem::take(&mut ctx.hold);
    let isolation = body.isolation();
    let mut attempt = 1;
    loop {
        ctx.boundary = Some(crate::ctx::Boundary {
            isolation,
            ..Default::default()
        });
        let outcome = body.run(ctx).await;
        let boundary = ctx.boundary.take().unwrap_or_default();
        let succeeded = matches!(outcome, Outcome::Value(_));
        if hold && succeeded && !boundary.concurrent_change {
            ctx.held = Some(crate::ctx::Held {
                transaction: boundary.transaction,
                outbox: boundary.outbox,
                session: boundary.session,
                issued: boundary.issued,
            });
            return outcome;
        }
        let ended = match boundary.transaction {
            _ if boundary.concurrent_change => {
                if let Some(transaction) = boundary.transaction {
                    let _ = transaction.rollback().await;
                }
                Ended::Changed
            }
            Some(transaction) if succeeded => match transaction.commit().await {
                Ok(()) => Ended::Committed,
                Err(StoreError::ConcurrentChange) => Ended::Changed,
                Err(_) => Ended::Unavailable,
            },
            // A failed body's own outcome stands whatever the rollback
            // says: nothing it wrote is kept either way.
            Some(transaction) => {
                let _ = transaction.rollback().await;
                Ended::RolledBack
            }
            None if succeeded => Ended::Committed,
            None => Ended::RolledBack,
        };
        match ended {
            Ended::Committed => {
                if !boundary.outbox.is_empty() {
                    ctx.host().deliver(boundary.outbox).await;
                }
                return outcome;
            }
            Ended::RolledBack => return outcome,
            Ended::Changed if attempt < RERUN_ATTEMPTS => attempt += 1,
            Ended::Changed | Ended::Unavailable => {
                return Outcome::Failed(Box::new(Failure {
                    frames: ctx.frames(),
                    ..unavailable()
                }));
            }
        }
    }
}

/// Commits what a held boundary left open, first running `then` in its
/// transaction (beginning one in `store` when the body began none and
/// `then` has something to write), and delivers its events once it has
/// committed. `Err` when the store couldn't commit: nothing it wrote is
/// kept, and the caller runs the action again on a concurrent change and
/// answers `unavailable` otherwise.
pub async fn commit_held(
    ctx: &mut Ctx<'_>,
    held: crate::ctx::Held,
    then: Option<(&str, &str, &[crate::SqlValue])>,
) -> Result<(), StoreError> {
    let crate::ctx::Held {
        mut transaction,
        outbox,
        ..
    } = held;
    if let Some((store, sql, params)) = then {
        if transaction.is_none() {
            let database = ctx
                .host()
                .database(store)
                .ok_or_else(|| StoreError::Fault(format!("no store called `{store}`")))?;
            transaction = Some(database.begin_at(Isolation::ReadCommitted).await?);
        }
        if let Some(open) = transaction.as_mut() {
            let changed = open.execute(sql, params).await;
            if !matches!(changed, Ok(1)) {
                if let Some(open) = transaction {
                    let _ = open.rollback().await;
                }
                // A claim that lapsed and was taken by another request
                // can't be written over: nothing is kept, and a retry runs
                // again.
                return Err(match changed {
                    Err(error) => error,
                    Ok(_) => StoreError::Unavailable,
                });
            }
        }
    }
    if let Some(open) = transaction {
        open.commit().await?;
    }
    if !outbox.is_empty() {
        ctx.host().deliver(outbox).await;
    }
    Ok(())
}

/// Runs `body`, called inside a running action by a caller that handles
/// its failure, so that a failure leaves nothing it did behind: its writes
/// are undone back to a savepoint taken before it ran (a refused statement
/// among them, so the transaction carries on), or, when it began the
/// transaction itself, the transaction is rolled back and the caller's
/// next store operation begins a fresh one; what it emitted is dropped,
/// and what it did to the request's session (a `sessions.start` or
/// `sessions.end`, or a token or key it issued) is forgotten, so the
/// response sets no cookie for a session that was never kept.
async fn caught<'p>(ctx: &mut Ctx<'p>, body: &mut dyn ActionBody<'p>) -> Outcome {
    let (open, emitted, session, issued) = match &mut ctx.boundary {
        Some(boundary) => (
            boundary.transaction.is_some(),
            boundary.outbox.len(),
            boundary.session.clone(),
            boundary.issued,
        ),
        None => (false, 0, None, false),
    };
    if open && let Some(Err(error)) = savepoint(ctx, Mark::Take).await {
        return lost(ctx, error);
    }
    let outcome = body.run(ctx).await;
    let failed = matches!(outcome, Outcome::Failed(_));
    if open {
        let mark = if failed { Mark::Return } else { Mark::Release };
        if let Some(Err(error)) = savepoint(ctx, mark).await {
            return lost(ctx, error);
        }
    } else if failed && let Some(boundary) = &mut ctx.boundary {
        boundary.store = None;
        if let Some(transaction) = boundary.transaction.take() {
            let _ = transaction.rollback().await;
        }
    }
    if failed && let Some(boundary) = &mut ctx.boundary {
        boundary.outbox.truncate(emitted);
        boundary.session = session;
        boundary.issued = issued;
    }
    outcome
}

/// What [`caught`] does with its savepoint.
enum Mark {
    Take,
    Release,
    Return,
}

async fn savepoint(ctx: &mut Ctx<'_>, mark: Mark) -> Option<Result<(), StoreError>> {
    let transaction = ctx.boundary.as_mut()?.transaction.as_mut()?;
    Some(match mark {
        Mark::Take => transaction.savepoint().await,
        Mark::Release => transaction.release_savepoint().await,
        Mark::Return => transaction.rollback_to_savepoint().await,
    })
}

/// The store couldn't keep the savepoint: the action can't finish.
fn lost(ctx: &mut Ctx<'_>, error: StoreError) -> Outcome {
    if matches!(error, StoreError::ConcurrentChange) {
        ctx.concurrent_change();
    }
    Outcome::Failed(Box::new(Failure {
        frames: ctx.frames(),
        ..unavailable()
    }))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use jadpo_value::Value;

    use super::*;
    use crate::{
        Database, Env, Event, Fault, FaultKind, Host, Places, Row, SqlValue, Transaction, block_on,
    };

    /// A store that writes down what happens to it, and reports a
    /// concurrent change at the first `changes` commits.
    #[derive(Default)]
    struct Log {
        lines: RefCell<Vec<String>>,
        changes: RefCell<u32>,
    }

    struct Tx(Rc<Log>);

    impl Transaction for Tx {
        fn query<'a>(
            &'a mut self,
            _sql: &'a str,
            _params: &'a [SqlValue],
        ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn execute<'a>(
            &'a mut self,
            _sql: &'a str,
            _params: &'a [SqlValue],
        ) -> LocalFuture<'a, Result<u64, StoreError>> {
            Box::pin(async { Ok(0) })
        }

        fn batch<'a>(&'a mut self, _sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
            Box::pin(async { Ok(()) })
        }

        fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
            let log = Rc::clone(&self.0);
            Box::pin(async move {
                let mut changes = log.changes.borrow_mut();
                if *changes > 0 {
                    *changes -= 1;
                    log.lines.borrow_mut().push("changed".into());
                    return Err(StoreError::ConcurrentChange);
                }
                log.lines.borrow_mut().push("commit".into());
                Ok(())
            })
        }

        fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
            let log = Rc::clone(&self.0);
            Box::pin(async move {
                log.lines.borrow_mut().push("rollback".into());
                Ok(())
            })
        }
    }

    struct Store(Rc<Log>);

    impl Database for Store {
        fn begin_at(
            &self,
            isolation: Isolation,
        ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
            let log = Rc::clone(&self.0);
            Box::pin(async move {
                let line = match isolation {
                    Isolation::RepeatableRead => "begin".to_string(),
                    other => format!("begin {}", other.word()),
                };
                log.lines.borrow_mut().push(line);
                Ok(Box::new(Tx(log)) as Box<dyn Transaction>)
            })
        }
    }

    struct Recorded {
        store: Store,
        log: Rc<Log>,
    }

    impl Recorded {
        fn new(changes: u32) -> Recorded {
            let log = Rc::new(Log::default());
            *log.changes.borrow_mut() = changes;
            Recorded {
                store: Store(Rc::clone(&log)),
                log,
            }
        }

        fn lines(&self) -> Vec<String> {
            self.log.lines.borrow().clone()
        }
    }

    impl Host for Recorded {
        fn database(&self, _store: &str) -> Option<&dyn Database> {
            Some(&self.store)
        }

        fn deliver(&self, events: Vec<Event>) -> LocalFuture<'_, ()> {
            for event in events {
                self.log
                    .lines
                    .borrow_mut()
                    .push(format!("deliver {}", event.key));
            }
            Box::pin(async {})
        }

        fn random(&self, _bytes: &mut [u8]) -> bool {
            false
        }
    }

    /// A body that writes (or not), emits, then ends as `end` says, and
    /// runs `inner` as an action called inside it. Its plan says it loads a
    /// record unless `isolation` says otherwise.
    struct Write<'i> {
        writes: bool,
        end: fn() -> Outcome,
        inner: Option<&'i mut Write<'i>>,
        runs: u32,
        isolation: Isolation,
        /// The level of the transaction it found itself in.
        found: Option<Isolation>,
        /// Its caller handles its failure.
        caught: bool,
        /// What it does to the request's session, and whether it issues a
        /// token, before it ends.
        session: Option<crate::SessionEffect>,
        issues: bool,
    }

    impl<'i> Write<'i> {
        fn new(writes: bool, end: fn() -> Outcome) -> Write<'i> {
            Write {
                writes,
                end,
                inner: None,
                runs: 0,
                isolation: Isolation::RepeatableRead,
                found: None,
                caught: false,
                session: None,
                issues: false,
            }
        }
    }

    impl<'p> ActionBody<'p> for Write<'_> {
        fn isolation(&self) -> Isolation {
            self.isolation
        }

        fn caught(&self) -> bool {
            self.caught
        }

        fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
            Box::pin(async move {
                self.runs += 1;
                self.found = ctx.isolation();
                if self.writes && ctx.transaction("main").await.is_err() {
                    return Outcome::Value(Value::Unit);
                }
                ctx.emit(Event {
                    key: format!("type Ran{}", self.runs),
                    value: Value::Unit,
                });
                if let Some(effect) = self.session.clone() {
                    ctx.session(effect);
                }
                if self.issues {
                    ctx.issued();
                }
                if let Some(inner) = self.inner.as_deref_mut() {
                    let caught = inner.caught;
                    let outcome = action(ctx, inner).await;
                    if !caught && !matches!(outcome, Outcome::Value(_)) {
                        return outcome;
                    }
                }
                (self.end)()
            })
        }
    }

    fn ok() -> Outcome {
        Outcome::Value(Value::Unit)
    }

    fn refused() -> Outcome {
        Outcome::Failed(Box::new(Failure::new("type Refused", "Refused", "refused")))
    }

    fn fault() -> Outcome {
        Outcome::Fault(Box::new(Fault {
            kind: FaultKind::IntOverflow,
            frames: Vec::new(),
        }))
    }

    fn run(host: &Recorded, body: &mut Write<'_>) -> Outcome {
        let places = Places::new();
        let mut ctx = Ctx::new(&places, host, Env::default());
        block_on(action(&mut ctx, body)).expect("nothing waits")
    }

    #[test]
    fn the_transaction_opens_at_the_first_store_operation_and_events_follow_the_commit() {
        let host = Recorded::new(0);
        let mut body = Write::new(true, ok);
        assert_eq!(run(&host, &mut body), ok());
        assert_eq!(host.lines(), ["begin", "commit", "deliver type Ran1"]);
        let host = Recorded::new(0);
        let mut reading_nothing = Write::new(false, ok);
        run(&host, &mut reading_nothing);
        assert_eq!(
            host.lines(),
            ["deliver type Ran1"],
            "no store, no transaction"
        );
    }

    #[test]
    fn the_transaction_begins_at_the_level_the_body_names_and_a_called_action_joins_at_it() {
        let host = Recorded::new(0);
        let mut counter = Write::new(true, ok);
        counter.isolation = Isolation::ReadCommitted;
        assert_eq!(run(&host, &mut counter), ok());
        assert_eq!(counter.found, Some(Isolation::ReadCommitted));
        assert_eq!(
            host.lines(),
            ["begin read_committed", "commit", "deliver type Ran1"],
            "an action that loads no record"
        );
        // An action called inside another takes the caller's level, which
        // already counts what the callee does.
        let host = Recorded::new(0);
        let mut inner = Write::new(true, ok);
        inner.isolation = Isolation::ReadCommitted;
        let mut outer = Write::new(true, ok);
        outer.inner = Some(&mut inner);
        assert_eq!(run(&host, &mut outer), ok());
        assert_eq!(host.lines()[0], "begin", "at repeatable read");
        assert_eq!(outer.found, Some(Isolation::RepeatableRead));
    }

    #[test]
    fn a_body_that_fails_rolls_back_and_delivers_nothing() {
        let host = Recorded::new(0);
        let mut body = Write::new(true, fault);
        assert_eq!(run(&host, &mut body), fault());
        assert_eq!(host.lines(), ["begin", "rollback"]);
    }

    #[test]
    fn an_action_called_inside_another_joins_its_transaction() {
        let host = Recorded::new(0);
        let mut inner = Write::new(true, ok);
        let mut outer = Write::new(true, ok);
        outer.inner = Some(&mut inner);
        assert_eq!(run(&host, &mut outer), ok());
        assert_eq!(
            host.lines(),
            ["begin", "commit", "deliver type Ran1", "deliver type Ran1"],
            "one transaction, both actions' events after it commits"
        );
    }

    #[test]
    fn a_caught_failure_forgets_what_it_did_to_the_session() {
        let held = |inner_end: fn() -> Outcome| {
            let host = Recorded::new(0);
            let mut inner = Write::new(true, inner_end);
            inner.caught = true;
            inner.session = Some(crate::SessionEffect::Start {
                token: "t".into(),
                csrf: "c".into(),
            });
            inner.issues = true;
            let mut outer = Write::new(true, ok);
            outer.inner = Some(&mut inner);
            let places = Places::new();
            let mut ctx = Ctx::new(&places, &host, Env::default());
            ctx.hold();
            assert_eq!(
                block_on(action(&mut ctx, &mut outer)).expect("nothing waits"),
                ok()
            );
            let held = ctx.take_held().expect("held");
            (held.session.is_some(), held.issued)
        };
        assert_eq!(held(refused), (false, false), "undone with its writes");
        assert_eq!(held(ok), (true, true), "kept when it succeeds");
        // Ending the request's session, then failing, leaves it running.
        let host = Recorded::new(0);
        let mut inner = Write::new(true, refused);
        inner.caught = true;
        inner.session = Some(crate::SessionEffect::End);
        let mut outer = Write::new(true, ok);
        outer.inner = Some(&mut inner);
        let places = Places::new();
        let mut ctx = Ctx::new(&places, &host, Env::default());
        ctx.hold();
        block_on(action(&mut ctx, &mut outer)).expect("nothing waits");
        assert_eq!(ctx.take_held().expect("held").session, None);
    }

    #[test]
    fn a_concurrent_change_reruns_the_whole_body_within_the_budget() {
        let host = Recorded::new(1);
        let mut body = Write::new(true, ok);
        assert_eq!(run(&host, &mut body), ok());
        assert_eq!(body.runs, 2);
        assert_eq!(
            host.lines(),
            ["begin", "changed", "begin", "commit", "deliver type Ran2"],
            "the first attempt's events are thrown away with it"
        );
        let host = Recorded::new(RERUN_ATTEMPTS);
        let mut body = Write::new(true, ok);
        let Outcome::Failed(failure) = run(&host, &mut body) else {
            panic!("unavailable");
        };
        assert_eq!(body.runs, RERUN_ATTEMPTS);
        assert_eq!(
            (failure.code.as_str(), failure.status),
            ("unavailable", Some(503))
        );
    }
}
