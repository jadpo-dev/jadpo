//! The action boundary (docs/compiler-runtime.md §2.5), which the runtime
//! owns rather than either executor: every action is all-or-nothing (areas
//! 3 and 6). The outermost action opens the boundary; the body, which the
//! executor supplies, runs; the transaction is begun at the body's first
//! store operation ([`Ctx::transaction`]), at the isolation level the body
//! names (area 6, "Counters and other one-step updates"); when another
//! transaction changed what it read, the whole body runs again within the
//! budget; then it commits, or rolls back when the body failed, and only
//! after a commit are the events it emitted left for the runner, to be
//! delivered after the response (chunk 6.1a). An action called
//! inside another joins its transaction (area 1, "Cross-domain writes"),
//! at the caller's level, which already counts what every action it calls
//! does; so does a fixture's, since a fixture is a test-only action (area
//! 13); a test, or a fixture's caller, opens a boundary per action it
//! calls, so each commits on its own. A called action whose failure its
//! caller handles (an outcome `match`) runs inside a savepoint: when it
//! fails, what it wrote and emitted is undone and the caller carries on,
//! since the action is all-or-nothing whoever calls it (area 6).

use std::collections::{BTreeMap, BTreeSet};

use jadpo_std::policy::Gate;
use jadpo_std::read::RowVersions;
use jadpo_std::write::RowCheck;

use crate::ctx::{Binding, Capabilities, EmitRefused, Held, SessionEffect};
use crate::{
    Ctx, Event, Failure, Isolation, LocalFuture, Outcome, SqlType, SqlValue, StoreError, Stored,
    Transaction,
};

/// A row by its entity's table and its `id`.
type RowKey = (String, SqlValue);

/// An action's boundary while its body runs: opened when the outermost
/// action starts, joined by every action it calls (area 6). Only this
/// module reads and writes what it holds; the context keeps it
/// ([`Ctx::open_boundary`]).
#[derive(Default)]
pub(crate) struct Boundary {
    /// The level its transaction begins at: the outermost action's.
    isolation: Isolation,
    /// Begun at the first store operation.
    transaction: Option<Box<dyn Transaction>>,
    /// The store it was begun in.
    store: Option<String>,
    /// What the action emitted, for after it commits.
    outbox: Vec<Event>,
    /// Where in `outbox` the running action's own events begin, which its
    /// allowance of [`crate::outbox::MAX_EVENTS`] counts from: 0, but for
    /// each item of a batch, an action of its own whose events join its
    /// page's (chunk 6.6a).
    allowance: usize,
    /// Its events' deliveries, once written to the outbox in its
    /// transaction as it commits (chunk 6.1b), for the runner after it
    /// commits.
    sealed: Vec<crate::outbox::Delivery>,
    /// How many events it wrote to the outbox as it sealed (chunk 6.2).
    kept: u32,
    /// A store operation found another transaction had changed what this
    /// one read, so the body is run again.
    concurrent_change: bool,
    /// What the body did to the request's session, for its response once
    /// it commits (area 8).
    session: Option<SessionEffect>,
    /// Whether the body issued an API token or a service key, whose
    /// response is never kept for an idempotent repeat (area 8).
    issued: bool,
    /// What the body did to a `same_browser:` magic link's cookie, whose
    /// response is never kept for a repeat either (chunk 7.3b3).
    binding: Option<Binding>,
    /// Whether the body asked for a magic link, whose row its transaction
    /// holds: using one in the same action would wait on itself.
    asked: bool,
    /// Whether its open transaction has run anything but reads. One that
    /// has only read is a snapshot, which an outside call closes before
    /// it goes out (area 6, "Outside calls hold no transaction"; chunk
    /// 7.4); one that has written can't be closed, and a call is refused.
    wrote: bool,
    /// Counts the transactions it has begun and ended, so a savepoint
    /// taken in one is never sought in another ([`Undo`]).
    generation: u32,
    /// The rows its open transaction loaded while it had only read, by
    /// table: the statement reading their versions, and their ids.
    loaded: BTreeMap<String, (String, Vec<SqlValue>)>,
    /// The rows loaded in a snapshot an outside call closed, each with the
    /// version it was loaded at (the first, when two snapshots loaded it):
    /// what a write after the call holds it to.
    versions: BTreeMap<RowKey, String>,
    /// Of `versions`, the rows its open transaction checked and locked,
    /// which stay checked while their locks survive, including savepoints.
    checked: BTreeSet<RowKey>,
}

impl Boundary {
    pub(crate) fn new(isolation: Isolation) -> Boundary {
        Boundary {
            isolation,
            ..Default::default()
        }
    }

    pub(crate) fn isolation(&self) -> Isolation {
        self.isolation
    }

    /// Its transaction in `store`, begun at the first store operation at
    /// its isolation level in the store `capabilities` give (area 6), or
    /// again after an outside call closed the last ([`Boundary::close_snapshot`]).
    /// An action writes one store (area 5), so asking for a second is a
    /// fault. `writes` when what runs in it may write: then an outside call
    /// can't close it.
    pub(crate) async fn transaction(
        &mut self,
        capabilities: &Capabilities<'_>,
        store: &str,
        writes: bool,
    ) -> Result<&mut dyn Transaction, StoreError> {
        if let Some(open) = &self.store
            && open != store
        {
            return Err(StoreError::Fault(format!(
                "an action writes one store, and this one already uses `{open}`, not `{store}`"
            )));
        }
        if self.transaction.is_none() {
            let Some(database) = capabilities.database(store) else {
                return Err(StoreError::Fault(format!("no store called `{store}`")));
            };
            self.transaction = Some(database.begin_at(self.isolation).await?);
            self.store = Some(store.to_string());
            self.generation = self.generation.wrapping_add(1);
            self.wrote = false;
            self.loaded.clear();
            self.checked.clear();
        }
        self.wrote |= writes;
        match &mut self.transaction {
            Some(transaction) => Ok(transaction.as_mut()),
            None => Err(StoreError::Fault("no transaction".into())),
        }
    }

    /// Keeps `ids`, rows of `versions`' table its open transaction just
    /// loaded, while it has only read: the rows whose versions an outside
    /// call reads before it closes the transaction (chunk 7.4).
    pub(crate) fn loaded(&mut self, versions: &RowVersions, ids: Vec<SqlValue>) {
        if self.transaction.is_none() || self.wrote || ids.is_empty() {
            return;
        }
        let (_, kept) = self
            .loaded
            .entry(versions.table.clone())
            .or_insert_with(|| (versions.sql.clone(), Vec::new()));
        kept.extend(ids);
    }

    /// Closes its open transaction when it has only read, before an
    /// outside call goes out (area 6, "Outside calls hold no
    /// transaction"; chunk 7.4): the snapshot reads the versions of the
    /// rows it loaded, which a write after the call is held to, and rolls
    /// back, having written nothing. A transaction that has written is
    /// left open, and the call refused.
    pub(crate) async fn close_snapshot(&mut self) -> Result<(), StoreError> {
        if self.wrote {
            return Ok(());
        }
        let Some(mut transaction) = self.transaction.take() else {
            return Ok(());
        };
        self.generation = self.generation.wrapping_add(1);
        let loaded = std::mem::take(&mut self.loaded);
        for (table, (sql, mut ids)) in loaded {
            ids.sort();
            ids.dedup();
            let rows = match transaction
                .query(&sql, &[SqlValue::Array(SqlType::Uuid, ids)])
                .await
            {
                Ok(rows) => rows,
                Err(error) => {
                    let _ = transaction.rollback().await;
                    return Err(error);
                }
            };
            for row in rows {
                if let [id, SqlValue::Text(version)] = row.as_slice() {
                    self.versions
                        .entry((table.clone(), id.clone()))
                        .or_insert_with(|| version.clone());
                }
            }
        }
        transaction.rollback().await
    }

    /// Whether it loaded rows in a snapshot an outside call closed.
    pub(crate) fn loaded_before_call(&self) -> bool {
        !self.versions.is_empty()
    }

    /// Expected version for a write in the current transaction. Its
    /// caller opens the transaction first, clearing obsolete checked keys.
    pub(crate) fn expected(&self, row: &RowCheck, id: &SqlValue) -> Option<String> {
        let key = (row.table.clone(), id.clone());
        if self.checked.contains(&key) {
            None
        } else {
            self.versions.get(&key).cloned()
        }
    }

    /// A successful mutation checked and locked this row in its own WHERE.
    /// Later writes in the same transaction must accept our own new xmin.
    pub(crate) fn checked(&mut self, row: &RowCheck, id: SqlValue) {
        self.checked.insert((row.table.clone(), id));
    }

    /// Holds the row `row` names, `id`, to the version a snapshot an
    /// outside call closed loaded it at, when a mutation changed no row:
    /// read and locked in its transaction in `store`, before classifying
    /// the ordinary refusal. A row changed or gone is a concurrent change.
    pub(crate) async fn check(
        &mut self,
        capabilities: &Capabilities<'_>,
        store: &str,
        row: &RowCheck,
        id: SqlValue,
    ) -> Result<(), StoreError> {
        let key = (row.table.clone(), id);
        let Some(loaded) = self.versions.get(&key).cloned() else {
            return Ok(());
        };
        if self.checked.contains(&key) {
            return Ok(());
        }
        let transaction = self.transaction(capabilities, store, true).await?;
        let rows = transaction
            .query(&row.sql, std::slice::from_ref(&key.1))
            .await?;
        match rows.first().and_then(|found| found.first()) {
            Some(SqlValue::Text(version)) if *version == loaded => {
                self.checked.insert(key);
                Ok(())
            }
            _ => Err(StoreError::ConcurrentChange),
        }
    }

    /// Whether it has begun its transaction.
    pub(crate) fn has_transaction(&self) -> bool {
        self.transaction.is_some()
    }

    /// Its transaction, when it has begun one.
    pub(crate) fn open_transaction(&mut self) -> Option<&mut dyn Transaction> {
        match &mut self.transaction {
            Some(transaction) => Some(transaction.as_mut()),
            None => None,
        }
    }

    /// Queues `event`, refused past the limits of one action's events.
    pub(crate) fn emit(&mut self, event: Event) -> Result<(), EmitRefused> {
        if self.outbox.len().saturating_sub(self.allowance) >= crate::outbox::MAX_EVENTS {
            return Err(EmitRefused::TooMany);
        }
        if event.payload.len() > crate::outbox::MAX_EVENT_BYTES {
            return Err(EmitRefused::TooLarge);
        }
        self.outbox.push(event);
        Ok(())
    }

    /// The next action counts its events from here: an item of a batch, an
    /// action of its own whose events join its page's (chunk 6.6a).
    pub(crate) fn own_allowance(&mut self) {
        self.allowance = self.outbox.len();
    }

    pub(crate) fn set_session(&mut self, effect: SessionEffect) {
        self.session = Some(effect);
    }

    pub(crate) fn set_issued(&mut self) {
        self.issued = true;
    }

    pub(crate) fn set_binding(&mut self, binding: Binding) {
        self.binding = Some(binding);
    }

    pub(crate) fn ask_sign_in(&mut self) {
        self.asked = true;
    }

    pub(crate) fn asked_sign_in(&self) -> bool {
        self.asked
    }

    /// Another transaction changed what this one read.
    pub(crate) fn mark_changed(&mut self) {
        self.concurrent_change = true;
    }

    pub(crate) fn changed(&self) -> bool {
        self.concurrent_change
    }

    pub(crate) fn take_transaction(&mut self) -> Option<Box<dyn Transaction>> {
        self.transaction.take()
    }

    /// The deliveries of what it kept in the outbox as it sealed.
    pub(crate) fn take_sealed(&mut self) -> Vec<crate::outbox::Delivery> {
        std::mem::take(&mut self.sealed)
    }
}

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

    /// Who may call it, for an action whose line under root `policy {
    /// actions: }` needs no record (area 9, "Actions"), as its plan says:
    /// checked as it starts, before its body runs, however it is called.
    fn gate(&self) -> Option<&Gate> {
        None
    }

    /// The site it is called from, in the frame on top of the context:
    /// where its gate's refusal is raised.
    fn site(&self) -> u32 {
        crate::Frame::ENTRY
    }

    /// The store its events are kept in when it began no transaction: the
    /// one it writes, or the one `events: { store: }` names, as its plan
    /// says (area 12, chunk 6.1b).
    fn outbox(&self) -> Option<&str> {
        None
    }

    /// For an action whose longest way makes several changing calls, run
    /// where its key survives a retry: where the record of its steps is
    /// kept, its step shape, and the key its answers are sealed with, as
    /// its plan says (area 12; chunk 7.5b). Only a boundary of its own
    /// keeps one, and only for a runner that set [`Ctx::set_resumable`].
    fn record(&self) -> Option<crate::steps::Spec<'_>> {
        None
    }
}

/// Runs `body` once, its gate checked first, so a caller the gate refuses
/// never reaches the body, whichever executor supplies it.
async fn gated<'p>(ctx: &mut Ctx<'p>, body: &mut dyn ActionBody<'p>) -> Outcome {
    if let Some(gate) = body.gate() {
        match crate::admit(ctx, gate, body.site()).await {
            Stored::Value(_) => {}
            Stored::Failed(failure) | Stored::Conflict { cause: failure, .. } => {
                return Outcome::Failed(failure);
            }
            Stored::Fault(fault) => return Outcome::Fault(fault),
        }
    }
    body.run(ctx).await
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

/// What the runtime raises when an outside call may have acted and
/// sending it again could act twice (area 4: `outcome_unknown`, 500, "the
/// change may or may not have happened, so retrying blindly is unsafe";
/// raised by the runtime; chunk 7.0c).
pub fn outcome_unknown() -> Failure {
    let mut failure = Failure::new(
        "runtime outcome_unknown",
        "OutcomeUnknown",
        "outcome_unknown",
    );
    failure.kind = Some("outcome_unknown".to_string());
    failure.status = Some(500);
    failure
}

/// What the runtime raises when a connection can't be used as deployed:
/// no driver fills it, or its credential was refused (area 4:
/// `misconfigured`, 500; area 11; chunk 7.0c).
pub fn misconfigured() -> Failure {
    let mut failure = Failure::new("runtime misconfigured", "Misconfigured", "misconfigured");
    failure.kind = Some("misconfigured".to_string());
    failure.status = Some(500);
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
/// rolls back, reruns on a concurrent change, and once it commits leaves
/// what the body emitted for the runner ([`Ctx::defer`]). Asked to hold
/// ([`Ctx::hold`]), a boundary of its own leaves a succeeding body's
/// transaction and outbox open for [`commit_held`] instead.
pub async fn action<'p>(ctx: &mut Ctx<'p>, body: &mut dyn ActionBody<'p>) -> Outcome {
    if ctx.in_action() {
        return if body.caught() {
            caught(ctx, body).await
        } else {
            gated(ctx, body).await
        };
    }
    // The record of its steps, kept where its key survives a retry
    // (chunk 7.5b): opened before its first run, closed after its last.
    match crate::steps::open(ctx, body.record()).await {
        Ok(_) => {}
        Err(unanswered) => {
            return match unanswered {
                crate::effects::Unanswered::Fault(kind) => {
                    Outcome::Fault(ctx.fault(kind, crate::Frame::ENTRY))
                }
                crate::effects::Unanswered::Failure(failure) => {
                    Outcome::Failed(Box::new(Failure {
                        frames: ctx.frames(),
                        ..*failure
                    }))
                }
                crate::effects::Unanswered::Failed(_) => Outcome::Failed(Box::new(Failure {
                    frames: ctx.frames(),
                    ..unavailable()
                })),
            };
        }
    }
    let outcome = own(ctx, body).await;
    crate::steps::close(ctx);
    outcome
}

/// [`action`] in a boundary of its own.
async fn own<'p>(ctx: &mut Ctx<'p>, body: &mut dyn ActionBody<'p>) -> Outcome {
    let hold = ctx.own_boundary();
    let isolation = body.isolation();
    let mut attempt = 1;
    // Run again after a concurrent change, its keyed calls take the same
    // steps, so each sends the same key (area 12, "Keys"; chunk 7.2c).
    let steps = ctx.steps();
    // Where its calls start in the invocation's log: an unkeyed change
    // among them that went out is never sent again (chunk 7.9a).
    let mark = ctx.capabilities().calls().len();
    loop {
        ctx.set_steps(steps);
        crate::steps::rewind(ctx);
        ctx.open_boundary(isolation);
        let mut outcome = gated(ctx, body).await;
        // A run that ends before replaying every recorded step no longer
        // runs as the record's did (chunk 7.5b).
        if matches!(outcome, Outcome::Value(_))
            && let Err(kind) = crate::steps::finished(ctx).await
        {
            outcome = Outcome::Fault(ctx.fault(kind, crate::Frame::ENTRY));
        }
        if matches!(outcome, Outcome::Value(_)) {
            outcome = sealed(ctx, body.outbox(), outcome).await;
        }
        let boundary = ctx.close_boundary();
        let succeeded = matches!(outcome, Outcome::Value(_));
        if hold && succeeded && !boundary.concurrent_change {
            ctx.keep_held(Held {
                transaction: boundary.transaction,
                deliveries: boundary.sealed,
                kept: boundary.kept,
                session: boundary.session,
                issued: boundary.issued,
                binding: boundary.binding,
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
                ctx.background_mut().count_kept(boundary.kept);
                committed(ctx, boundary.sealed);
                return outcome;
            }
            Ended::RolledBack => return outcome,
            // A change with no key that went out would act again on a
            // rerun: the action stops there for an operator instead (area
            // 16, "Parked, never sent again blindly"; chunk 7.9a).
            Ended::Changed if ctx.capabilities().may_have_acted(mark) => {
                return Outcome::Failed(Box::new(Failure {
                    frames: ctx.frames(),
                    ..outcome_unknown()
                }));
            }
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

/// Leaves the deliveries of what a committed action emitted for the
/// runner, to run after the response (area 12; chunks 6.1a and 6.1b).
pub(crate) fn committed(ctx: &mut Ctx<'_>, deliveries: Vec<crate::outbox::Delivery>) {
    if !deliveries.is_empty() {
        ctx.defer(crate::runner::Deferred::Deliveries(deliveries));
    }
}

/// `outcome`, a body's success, once what commits with its writes is
/// written in its transaction ([`seal`]); the failure the action ends in
/// when the store couldn't keep it.
async fn sealed(ctx: &mut Ctx<'_>, outbox: Option<&str>, outcome: Outcome) -> Outcome {
    match seal(ctx, outbox).await {
        Ok(()) => outcome,
        Err(StoreError::ConcurrentChange) => {
            ctx.concurrent_change();
            outcome
        }
        Err(StoreError::Fault(reason)) => {
            Outcome::Fault(ctx.fault(crate::FaultKind::Defect(reason), crate::Frame::ENTRY))
        }
        Err(_) => Outcome::Failed(Box::new(Failure {
            frames: ctx.frames(),
            ..unavailable()
        })),
    }
}

/// Writes what commits with a succeeding body's writes, in its
/// transaction (area 12, chunk 6.1b): each event it emitted in the
/// outbox, with a delivery record for each subscriber, in the store its
/// transaction is in, or, having begun none, the store `outbox` names;
/// and then, for a subscriber's delivery, its handled record in the store
/// its transaction is in once those are written, so an action that only
/// emits keeps one beside its events (chunk 6.6a).
pub(crate) async fn seal(ctx: &mut Ctx<'_>, outbox: Option<&str>) -> Result<(), StoreError> {
    // Kept for a rerun after a concurrent change; the runner clears it.
    let handled = ctx.background().handled().cloned();
    let Some(boundary) = ctx.boundary_mut() else {
        return Ok(());
    };
    let events = std::mem::take(&mut boundary.outbox);
    if !events.is_empty() {
        let open = boundary.store.clone();
        let Some(store) = open.or_else(|| outbox.map(str::to_string)) else {
            return Err(StoreError::Fault(
                "an action emitted events with no store to keep them in".into(),
            ));
        };
        let mut sealed = crate::outbox::Sealed {
            outbox: Vec::with_capacity(events.len()),
            deliveries: Vec::new(),
        };
        // The host's time, never a record's fixed `clock.now` (chunk 7.9a).
        let emitted_at = crate::steps::wall(ctx);
        let trace = ctx.trace();
        let depth = ctx.chain();
        for event in events {
            let Some(id) = ctx.new_id() else {
                return Err(StoreError::Fault(
                    "no random bytes for an event's identity".into(),
                ));
            };
            for subscriber in &event.subscribers {
                let Some(delivery) = ctx.new_id() else {
                    return Err(StoreError::Fault(
                        "no random bytes for a delivery's identity".into(),
                    ));
                };
                sealed.deliveries.push(crate::outbox::Delivery {
                    store: store.clone(),
                    id: delivery,
                    outbox: id,
                    subscriber: subscriber.clone(),
                    event: event.key.clone(),
                    value: event.value.clone(),
                    emitted_at,
                    trace,
                    depth,
                });
            }
            sealed.outbox.push((id, event.key, event.payload));
        }
        let written = u32::try_from(sealed.outbox.len()).unwrap_or(u32::MAX);
        let transaction = ctx.transaction(&store).await?;
        crate::outbox::write(transaction, &sealed, emitted_at, trace, depth).await?;
        if let Some(boundary) = ctx.boundary_mut() {
            boundary.sealed = sealed.deliveries;
            boundary.kept = written;
        }
    }
    // The store the transaction is in now, the outbox's when only it
    // began one.
    let open = ctx.boundary().and_then(|boundary| boundary.store.clone());
    if let Some(handled) = handled
        && open.as_deref() == Some(handled.store.as_str())
    {
        let now = crate::steps::wall(ctx);
        let transaction = ctx.transaction(&handled.store).await?;
        transaction
            .execute(
                crate::outbox::INSERT_HANDLED,
                &[
                    crate::SqlValue::Uuid(handled.id),
                    crate::outbox::micros(now),
                ],
            )
            .await?;
    }
    // The record of its steps goes in the same commit (chunk 7.5b).
    crate::steps::seal(ctx).await
}

/// Commits what a held boundary left open, first running `then`, one
/// statement that changes one row, in its transaction, or on its own in
/// `store` when the body began none, and once it has committed leaves its
/// events for the runner. `Err` when the store couldn't commit: nothing it
/// wrote is kept, and the caller runs the action again on a concurrent
/// change and answers `unavailable` otherwise.
pub async fn commit_held(
    ctx: &mut Ctx<'_>,
    held: Held,
    then: Option<(&str, &str, &[crate::SqlValue])>,
) -> Result<(), StoreError> {
    let Held {
        transaction,
        deliveries,
        kept,
        ..
    } = held;
    if let Some((store, sql, params)) = then {
        // A claim that lapsed and was taken by another request can't be
        // written over: nothing is kept, and a retry runs again.
        let lapsed = |changed: Result<u64, StoreError>| match changed {
            Ok(1) => Ok(()),
            Ok(_) => Err(StoreError::Unavailable),
            Err(error) => Err(error),
        };
        match transaction {
            Some(mut open) => {
                if let Err(error) = lapsed(open.execute(sql, params).await) {
                    let _ = open.rollback().await;
                    return Err(error);
                }
                open.commit().await?;
            }
            // Nothing else to keep with it: the one statement, on its own.
            None => {
                let database = ctx
                    .capabilities()
                    .database(store)
                    .ok_or_else(|| StoreError::Fault(format!("no store called `{store}`")))?;
                let changed = database.query_once(sql, params).await;
                lapsed(changed.map(|rows| rows.len() as u64))?;
            }
        }
        ctx.background_mut().count_kept(kept);
        committed(ctx, deliveries);
        return Ok(());
    }
    if let Some(open) = transaction {
        open.commit().await?;
    }
    ctx.background_mut().count_kept(kept);
    committed(ctx, deliveries);
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
    let undo = match Undo::take(ctx).await {
        Ok(undo) => undo,
        Err(outcome) => return outcome,
    };
    let outcome = gated(ctx, body).await;
    undo.settle(ctx, outcome).await
}

/// What a call inside a running action that is undone alone when it fails
/// returns to (area 6): a savepoint when the transaction is open, and what
/// the boundary held before it. The runner runs each item of a batch the
/// same way ([`crate::runner::Batch`]).
pub(crate) struct Undo {
    open: bool,
    /// Which transaction was open: an outside call the call makes closes
    /// a snapshot, and its savepoint with it (chunk 7.4).
    generation: u32,
    emitted: usize,
    session: Option<SessionEffect>,
    issued: bool,
    binding: Option<Binding>,
    checked: BTreeSet<RowKey>,
}

impl Undo {
    /// Marks where the call returns to if it fails; `Err` with the
    /// outcome the call ends in when the store couldn't.
    pub(crate) async fn take(ctx: &mut Ctx<'_>) -> Result<Undo, Outcome> {
        let undo = match ctx.boundary() {
            Some(boundary) => Undo {
                open: boundary.transaction.is_some(),
                generation: boundary.generation,
                emitted: boundary.outbox.len(),
                session: boundary.session.clone(),
                issued: boundary.issued,
                binding: boundary.binding.clone(),
                checked: boundary.checked.clone(),
            },
            None => Undo {
                open: false,
                generation: 0,
                emitted: 0,
                session: None,
                issued: false,
                binding: None,
                checked: BTreeSet::new(),
            },
        };
        if undo.open
            && let Some(Err(error)) = savepoint(ctx, Mark::Take).await
        {
            return Err(lost(ctx, error));
        }
        Ok(undo)
    }

    /// The call ended in `outcome`: what it did stands when it gave its
    /// value, and is undone when it failed or faulted, so a batch's item
    /// that faulted leaves nothing behind either (chunk 6.4).
    pub(crate) async fn settle(self, ctx: &mut Ctx<'_>, outcome: Outcome) -> Outcome {
        let failed = !matches!(outcome, Outcome::Value(_));
        // An outside call the call made closed the snapshot its savepoint
        // was taken in: whatever transaction is open now is the call's own
        // (chunk 7.4).
        let same = ctx
            .boundary()
            .is_some_and(|boundary| boundary.generation == self.generation);
        if self.open && same {
            let mark = if failed { Mark::Return } else { Mark::Release };
            if let Some(Err(error)) = savepoint(ctx, mark).await {
                return lost(ctx, error);
            }
        } else if failed && let Some(boundary) = ctx.boundary_mut() {
            if !self.open {
                boundary.store = None;
            }
            if let Some(transaction) = boundary.transaction.take() {
                boundary.generation = boundary.generation.wrapping_add(1);
                let _ = transaction.rollback().await;
            }
        }
        if failed && let Some(boundary) = ctx.boundary_mut() {
            // ROLLBACK TO releases locks acquired after the savepoint.
            // Checks established before it survive; those inside do not.
            boundary.checked = if self.open && same {
                self.checked
            } else {
                BTreeSet::new()
            };
            boundary.outbox.truncate(self.emitted);
            boundary.session = self.session;
            boundary.issued = self.issued;
            boundary.binding = self.binding;
        }
        outcome
    }
}

/// What [`caught`] does with its savepoint.
enum Mark {
    Take,
    Release,
    Return,
}

async fn savepoint(ctx: &mut Ctx<'_>, mark: Mark) -> Option<Result<(), StoreError>> {
    let transaction = ctx.boundary_mut()?.open_transaction()?;
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

    use jadpo_std::policy::Callers;
    use jadpo_value::Value;

    use super::*;
    use crate::{Database, Env, Event, Fault, FaultKind, Host, Places, Row, Transaction, block_on};

    /// A store that writes down what happens to it, and reports a
    /// concurrent change at the first `changes` commits. Its one row,
    /// [`ROW`], is at `version`: what [`VERSIONS`] reads and [`CHECK`]
    /// finds, each written down too, as are savepoints once `savepoints`.
    #[derive(Default)]
    struct Log {
        lines: RefCell<Vec<String>>,
        changes: RefCell<u32>,
        version: RefCell<String>,
        savepoints: std::cell::Cell<bool>,
    }

    const ROW: [u8; 16] = [7; 16];

    fn versions() -> RowVersions {
        RowVersions {
            table: "todo".into(),
            id: 0,
            sql: "VERSIONS".into(),
        }
    }

    fn check() -> RowCheck {
        RowCheck {
            table: "todo".into(),
            id: jadpo_std::write::WriteSource::Input(0),
            sql: "CHECK".into(),
            version: 0,
        }
    }

    struct Tx(Rc<Log>);

    impl Transaction for Tx {
        fn query<'a>(
            &'a mut self,
            sql: &'a str,
            _params: &'a [SqlValue],
        ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
            let log = Rc::clone(&self.0);
            Box::pin(async move {
                let version = SqlValue::Text(log.version.borrow().clone());
                Ok(match sql {
                    "VERSIONS" => {
                        log.lines.borrow_mut().push("versions".into());
                        vec![vec![SqlValue::Uuid(ROW), version]]
                    }
                    "CHECK" => {
                        log.lines.borrow_mut().push("check".into());
                        vec![vec![version]]
                    }
                    _ => Vec::new(),
                })
            })
        }

        fn savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
            if self.0.savepoints.get() {
                self.0.lines.borrow_mut().push("savepoint".into());
            }
            Box::pin(async { Ok(()) })
        }

        fn release_savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
            if self.0.savepoints.get() {
                self.0.lines.borrow_mut().push("release".into());
            }
            Box::pin(async { Ok(()) })
        }

        fn rollback_to_savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
            if self.0.savepoints.get() {
                self.0.lines.borrow_mut().push("return".into());
            }
            Box::pin(async { Ok(()) })
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

        fn random(&self, _bytes: &mut [u8]) -> bool {
            false
        }
    }

    impl Recorded {
        /// Writes down what `ctx` left for the runner to deliver, as
        /// `deliver <event>` lines after the store's own: only a committed
        /// action leaves any (chunk 6.1a).
        fn deferred(&self, ctx: &mut Ctx<'_>) {
            for work in ctx.take_deferred() {
                let crate::runner::Deferred::Deliveries(deliveries) = work else {
                    continue;
                };
                for delivery in deliveries {
                    self.log
                        .lines
                        .borrow_mut()
                        .push(format!("deliver {}", delivery.event));
                }
            }
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
        /// Who may call it, as its plan would say.
        gate: Option<Gate>,
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
                gate: None,
            }
        }
    }

    impl<'p> ActionBody<'p> for Write<'_> {
        fn isolation(&self) -> Isolation {
            self.isolation
        }

        fn outbox(&self) -> Option<&str> {
            Some("main")
        }

        fn caught(&self) -> bool {
            self.caught
        }

        fn gate(&self) -> Option<&Gate> {
            self.gate.as_ref()
        }

        fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
            Box::pin(async move {
                self.runs += 1;
                self.found = ctx.isolation();
                if self.writes && ctx.transaction("main").await.is_err() {
                    return Outcome::Value(Value::Unit);
                }
                let _ = ctx.emit(Event {
                    key: format!("type Ran{}", self.runs),
                    value: Value::Unit,
                    payload: "null".into(),
                    subscribers: vec!["subscriber s".into()],
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

    fn run(host: &Recorded, body: &mut dyn for<'q> ActionBody<'q>) -> Outcome {
        let places = Places::new();
        let mut ctx = Ctx::new(&places, host, Env::default());
        let outcome = block_on(action(&mut ctx, body)).expect("nothing waits");
        host.deferred(&mut ctx);
        outcome
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
            ["begin", "commit", "deliver type Ran1"],
            "no store of its own: its events begin one in its outbox's store"
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
    fn the_boundary_checks_an_actions_gate_whichever_executor_supplies_it() {
        let refused = |gate: Gate, set: fn(&mut Ctx<'_>)| {
            let host = Recorded::new(0);
            let mut body = Write::new(true, ok);
            body.gate = Some(gate);
            let places = Places::new();
            let mut ctx = Ctx::new(&places, &host, Env::default());
            set(&mut ctx);
            let outcome = block_on(action(&mut ctx, &mut body)).expect("nothing waits");
            let code = match outcome {
                Outcome::Failed(failure) => Some(failure.code),
                _ => None,
            };
            (code, body.runs)
        };
        let nobody = |_: &mut Ctx<'_>| {};
        let signed_in = |ctx: &mut Ctx<'_>| {
            ctx.set_caller(Some(crate::Caller {
                value: Value::Unit,
                key: "user 1".into(),
                session: None,
            }));
        };
        assert_eq!(
            refused(Gate::of(Callers::Signed), nobody),
            (Some("not_permitted".into()), 0),
            "no caller: the body never runs"
        );
        assert_eq!(refused(Gate::of(Callers::Signed), signed_in), (None, 1));
        assert_eq!(
            refused(Gate::of(Callers::Nobody), signed_in),
            (Some("not_permitted".into()), 0)
        );
        assert_eq!(
            refused(Gate::of(Callers::Nobody), |ctx| ctx.set_unpoliced()),
            (None, 1),
            "a test's call without a caller isn't checked"
        );
        // Called inside another action, and caught, it is checked too.
        let host = Recorded::new(0);
        let mut inner = Write::new(true, ok);
        inner.gate = Some(Gate::of(Callers::Nobody));
        inner.caught = true;
        let mut outer = Write::new(true, ok);
        outer.inner = Some(&mut inner);
        let places = Places::new();
        let mut ctx = Ctx::new(&places, &host, Env::default());
        assert_eq!(
            block_on(action(&mut ctx, &mut outer)).expect("nothing waits"),
            ok()
        );
        host.deferred(&mut ctx);
        assert_eq!(
            host.lines(),
            ["begin", "commit", "deliver type Ran1"],
            "only the caller's event: the refused action never ran"
        );
    }

    #[test]
    fn the_system_passes_a_gate_only_when_its_line_names_access_system() {
        let ran = |gate: Gate, set: fn(&mut Ctx<'_>)| {
            let host = Recorded::new(0);
            let mut body = Write::new(true, ok);
            body.gate = Some(gate);
            let places = Places::new();
            let mut ctx = Ctx::new(&places, &host, Env::default());
            set(&mut ctx);
            let outcome = block_on(action(&mut ctx, &mut body)).expect("nothing waits");
            match outcome {
                Outcome::Failed(failure) => Some(failure.code),
                _ => None,
            }
        };
        let system = |ctx: &mut Ctx<'_>| ctx.become_system(crate::System::default());
        let signed_in = |ctx: &mut Ctx<'_>| {
            ctx.set_caller(Some(crate::Caller {
                value: Value::Unit,
                key: "user 1".into(),
                session: None,
            }));
        };
        let only_system = Gate {
            callers: Callers::Nobody,
            system: true,
        };
        assert_eq!(ran(only_system.clone(), system), None);
        assert_eq!(
            ran(only_system, signed_in),
            Some("not_permitted".into()),
            "a line naming only Access.system lets in no request's caller"
        );
        assert_eq!(
            ran(Gate::of(Callers::Signed), system),
            Some("not_permitted".into()),
            "the system is nobody signed in"
        );
        let either = Gate {
            callers: Callers::Signed,
            system: true,
        };
        assert_eq!(ran(either.clone(), system), None);
        assert_eq!(ran(either, signed_in), None);
    }

    /// A body that reads a row, makes an outside call as the seam does
    /// (closing what it has only read), then writes the row it loaded:
    /// the first time, another request changes the row while the call is
    /// out.
    struct Called {
        runs: u32,
        changed_during_call: bool,
        /// Whether a transaction was open while each call was out.
        open_during_call: Vec<bool>,
        /// The store's, whose row the other request changes.
        log: Rc<Log>,
    }

    impl<'p> ActionBody<'p> for Called {
        fn isolation(&self) -> Isolation {
            Isolation::RepeatableRead
        }

        fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
            Box::pin(async move {
                self.runs += 1;
                if ctx.read_transaction("main").await.is_err() {
                    return refused();
                }
                ctx.loaded(&versions(), vec![SqlValue::Uuid(ROW)]);
                if ctx.close_snapshot().await.is_err() {
                    return refused();
                }
                self.open_during_call.push(ctx.transaction_open());
                if self.runs == 1 && self.changed_during_call {
                    *self.log.version.borrow_mut() = "2".into();
                }
                match ctx.check_row("main", &check(), SqlValue::Uuid(ROW)).await {
                    Ok(()) => {}
                    Err(StoreError::ConcurrentChange) => {
                        ctx.concurrent_change();
                        return fault();
                    }
                    Err(_) => return refused(),
                }
                if ctx.transaction("main").await.is_err() {
                    return refused();
                }
                ok()
            })
        }
    }

    /// No transaction is open while an outside call is out (area 6,
    /// "Outside calls hold no transaction"; chunk 7.4): what the action
    /// read is a snapshot, which reads its rows' versions and closes
    /// before the call; the transaction after the call checks the row it
    /// writes against the version loaded; and a change made while the call
    /// was out runs the whole action again.
    #[test]
    fn reads_before_a_call_close_and_a_change_meanwhile_reruns_the_action() {
        let host = Recorded::new(0);
        *host.log.version.borrow_mut() = "1".into();
        let mut body = Called {
            runs: 0,
            changed_during_call: false,
            open_during_call: Vec::new(),
            log: Rc::clone(&host.log),
        };
        assert_eq!(run(&host, &mut body), ok());
        assert_eq!(body.open_during_call, [false]);
        assert_eq!(
            host.lines(),
            ["begin", "versions", "rollback", "begin", "check", "commit"],
            "a snapshot closed before the call, the write's transaction after it"
        );

        let host = Recorded::new(0);
        *host.log.version.borrow_mut() = "1".into();
        let mut body = Called {
            runs: 0,
            changed_during_call: true,
            open_during_call: Vec::new(),
            log: Rc::clone(&host.log),
        };
        assert_eq!(run(&host, &mut body), ok());
        assert_eq!(body.runs, 2, "the change while the call was out reran it");
        assert_eq!(body.open_during_call, [false, false]);
        assert_eq!(
            host.lines(),
            [
                "begin", "versions", "rollback", "begin", "check", "rollback", "begin", "versions",
                "rollback", "begin", "check", "commit"
            ]
        );
    }

    /// A caller that read handles the failure of an action it calls that
    /// made an outside call, then wrote and failed: the snapshot its
    /// savepoint was in closed with the call, so the callee's transaction
    /// is rolled back whole and the caller carries on.
    #[test]
    fn a_caught_call_that_closed_the_snapshot_rolls_its_own_transaction_back() {
        struct Callee;
        impl<'p> ActionBody<'p> for Callee {
            fn isolation(&self) -> Isolation {
                Isolation::RepeatableRead
            }
            fn caught(&self) -> bool {
                true
            }
            fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
                Box::pin(async move {
                    let _ = ctx.close_snapshot().await;
                    let _ = ctx.transaction("main").await;
                    refused()
                })
            }
        }
        struct Caller;
        impl<'p> ActionBody<'p> for Caller {
            fn isolation(&self) -> Isolation {
                Isolation::RepeatableRead
            }
            fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
                Box::pin(async move {
                    let _ = ctx.read_transaction("main").await;
                    let outcome = action(ctx, &mut Callee).await;
                    assert!(matches!(outcome, Outcome::Failed(_)), "{outcome:?}");
                    let _ = ctx.transaction("main").await;
                    ok()
                })
            }
        }
        let host = Recorded::new(0);
        host.log.savepoints.set(true);
        let places = Places::new();
        let mut ctx = Ctx::new(&places, &host, Env::default());
        assert_eq!(
            block_on(action(&mut ctx, &mut Caller)).expect("nothing waits"),
            ok()
        );
        assert_eq!(
            host.lines(),
            [
                "begin",
                "savepoint",
                "rollback",
                "begin",
                "rollback",
                "begin",
                "commit"
            ],
            "the callee's transaction undone whole, never a savepoint it doesn't hold"
        );
    }

    #[test]
    fn a_failed_savepoint_restores_only_checks_whose_locks_survive() {
        let host = Recorded::new(0);
        let places = Places::new();
        let mut ctx = Ctx::new(&places, &host, Env::default());
        ctx.give_boundary(Some(Boundary::default()));
        block_on(async {
            ctx.transaction("main").await.unwrap();
            let before = ("todo".to_string(), SqlValue::Uuid([1; 16]));
            let inside = ("todo".to_string(), SqlValue::Uuid([2; 16]));
            ctx.boundary_mut().unwrap().checked.insert(before.clone());
            let undo = Undo::take(&mut ctx).await.unwrap();
            ctx.boundary_mut().unwrap().checked.insert(inside);
            let _ = undo.settle(&mut ctx, refused()).await;
            assert_eq!(ctx.boundary().unwrap().checked, BTreeSet::from([before]));
        })
        .unwrap();
    }

    #[test]
    fn a_caught_whole_transaction_rollback_requires_a_new_version_check() {
        for had_snapshot in [false, true] {
            let host = Recorded::new(0);
            let places = Places::new();
            let mut ctx = Ctx::new(&places, &host, Env::default());
            ctx.give_boundary(Some(Boundary::default()));
            block_on(async {
                let key = ("todo".to_string(), SqlValue::Uuid(ROW));
                ctx.boundary_mut()
                    .unwrap()
                    .versions
                    .insert(key.clone(), "first".into());
                if had_snapshot {
                    ctx.read_transaction("main").await.unwrap();
                }
                let undo = Undo::take(&mut ctx).await.unwrap();
                if had_snapshot {
                    ctx.close_snapshot().await.unwrap();
                }
                ctx.transaction("main").await.unwrap();
                ctx.boundary_mut().unwrap().checked.insert(key);
                let _ = undo.settle(&mut ctx, refused()).await;
                assert!(!ctx.transaction_open());
                ctx.transaction("main").await.unwrap();
                assert_eq!(
                    ctx.expected_row(&check(), &SqlValue::Uuid(ROW)),
                    Some("first".into())
                );
                assert!(ctx.boundary().unwrap().checked.is_empty());
            })
            .unwrap();
        }
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
