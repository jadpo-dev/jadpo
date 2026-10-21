//! One run of one test (area 13, "Writing a test"; docs/compiler-runtime.md
//! §2.5, "The test runner"; chunk 8.2e): what `jadpo test` sets a test up
//! with, runs it under and keeps of it, whichever executor runs it and on
//! whichever host, so the interpreter's run natively and compiled code's
//! inside workerd (a Worker's test handler, `jadpo_host_workers::testing`)
//! are the same run, compared by their records.
//!
//! [`once`] is that run: the test's seed from its key and its clock from
//! its `clock:` ([`env`]), every provider package's connection answered by
//! a package fake and the connection `ai:` names by the `ai` fake, each
//! outside call kept in the record when the run keeps one, the test's
//! settings, and, for a test with a database, what each step emits
//! delivered before the next ([`delivery_order`]) and each step it reaches
//! counted or crashed at; then its outcome kept, and the batch runs it
//! started run to their end at its clock ([`AtClock`]) when asked. The
//! host it is given holds the test's stores; what a test's host gives
//! besides them is [`Caught`]. [`rows`] reads what a test's database holds
//! when it ends, for its record.
//!
//! The test runner (`jadpo-test`) chooses the tests, makes each one's
//! databases, crashes a run at each step it reached, and reports.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::ai::AiFake;
use crate::budget::Invocation;
use crate::capability::{Capability, Faked};
use crate::crashes::{Boundary, Reached};
use crate::fakes::{ProviderFake, Usage};
use crate::files::MemoryFiles;
use crate::record::{Entry, Record};
use crate::runner::{Deferred, run_deferred};
use crate::test_steps::Undelivered;
use crate::{
    Clock, Ctx, Database, Env, Executor, Fault, FaultKind, Host, LocalFuture, Outcome, Places,
    Plan, SqlValue,
};

/// How many of a file's rows a test's host reads into one batch, so every
/// file a test reads resumes between batches (area 13, "Provider calls
/// and capabilities in tests").
pub const ROWS_PER_BATCH: u32 = 2;

/// One step of SplitMix64.
pub fn splitmix(state: u64) -> u64 {
    let mut z = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A stable digest of `text` (FNV-1a), the same on every platform.
fn digest(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The seed of the test with key `key`: from the key alone, so every run
/// gives the test the same identities, tokens, codes and shuffles wherever
/// it falls in the order (area 13, "Controlling time", amended by area 19,
/// question 19).
pub fn seed(key: &str) -> u64 {
    splitmix(digest(key))
}

/// The order a test's deliveries are drawn in between its steps (area 13,
/// "Caches and events"; chunk 6.11): from the run's order seed, which the
/// report prints and `--seed` gives again, and the test's key, so tests of
/// one run are shuffled apart and rerunning one alone keeps its order.
pub fn delivery_order(order_seed: u64, key: &str) -> u64 {
    splitmix(order_seed ^ digest(key))
}

/// What the test with key `key`, callable `callable` of `plan`, runs under:
/// its seed, and its clock at the fixed default, 2000-01-01T00:00:00Z, or
/// where its `clock:` starts it.
pub fn env(plan: Plan<'_>, key: &str, callable: u32) -> Env {
    Env {
        seed: Some(seed(key)),
        clock: plan.test_clock(callable).map_or(Clock::TEST, Clock::fixed),
    }
}

/// One run of a test.
#[derive(Debug)]
pub struct Once<'a> {
    /// The test's identity key.
    pub key: &'a str,
    /// Its callable in the program's plan.
    pub callable: u32,
    /// For a test with a database, the run's order seed: what each step
    /// emits is delivered before the next in the order it and the key
    /// give, and each step the test reaches under a record is counted.
    /// `None` for a test without one.
    pub order: Option<u64>,
    /// The step, from 1, and the boundary to crash the run at (chunk
    /// 7.5d), for a test with a database.
    pub crash: Option<(usize, Boundary)>,
    /// Each batch run the test started run to its end once it has ended
    /// (chunk 8.1h).
    pub batches: bool,
    /// The record each statement, event and outside call is kept in, when
    /// the run keeps one; the host's stores keep theirs in it too.
    pub record: Option<Rc<Record>>,
}

/// How one run of a test went.
#[derive(Debug)]
pub struct Ran {
    pub outcome: Outcome,
    /// The delivery between its steps that failed for good, if one did.
    pub undelivered: Option<Undelivered>,
    /// The steps it reached under a record, in order.
    pub reached: Vec<Reached>,
    /// Whether the crash it was asked to make happened.
    pub crashed: bool,
    /// How each scripted operation and prompt was used.
    pub usage: Vec<Usage>,
    /// What its keyed provider calls did, by operation.
    pub effects: Vec<(String, usize)>,
}

/// Runs one test on `executor` over `host`, which holds its stores, as
/// `once` says.
pub async fn once(executor: &dyn Executor, host: &dyn Host, once: Once<'_>) -> Ran {
    let plan = executor.plan();
    // Every provider package's connection is answered by its fake (area
    // 13; chunk 7.2d), but the email connection's, whose mail the host
    // catches whole; every model asked answers through the `ai` fake
    // (area 21, "Tests"), first, so it answers a package's connection
    // that fills `ai` too (chunk 5.5v3b).
    let fake = Rc::new(ProviderFake::new());
    let ai = Rc::new(AiFake::new());
    let mail = plan.mail_connection();
    let faked = match plan.ai_connection() {
        Some(models) => Faked::new(host).with(models, Rc::clone(&ai) as Rc<dyn Capability>),
        None => Faked::new(host),
    };
    let faked = plan
        .provider_connections()
        .into_iter()
        .filter(|connection| Some(*connection) != mail)
        .fold(faked, |faked, connection| {
            faked.with(connection, Rc::clone(&fake) as Rc<dyn Capability>)
        });
    let faked = match &once.record {
        Some(record) => faked.with_record(Rc::clone(record)),
        None => faked,
    };
    let mut ctx = Ctx::of(executor, &faked, env(plan, once.key, once.callable));
    ctx.set_settings(plan.test_settings(once.callable));
    if let Some(order) = once.order {
        // What each step emits is delivered before the next statement
        // (chunk 6.11); each step reached is counted, or crashed at.
        ctx.deliver_between_steps(delivery_order(order, once.key));
        match once.crash {
            Some((at, boundary)) => ctx.crash_at(at, boundary),
            None => ctx.track_steps(),
        }
    }
    let outcome = executor.run(&mut ctx, once.callable, Vec::new()).await;
    if let Some(record) = &once.record {
        record.push(Entry::Outcome(crate::record::outcome(
            &outcome,
            executor.places(),
        )));
    }
    if once.batches {
        let started: Vec<Deferred> = ctx
            .take_deferred()
            .into_iter()
            .filter(|work| matches!(work, Deferred::Batch(_)))
            .collect();
        if !started.is_empty() {
            let clocked = AtClock::new(&faked, ctx.now());
            run_deferred(executor, &clocked, started).await;
        }
    }
    let mut usage = fake.usage();
    usage.extend(ai.usage());
    Ran {
        outcome,
        undelivered: ctx.undelivered().cloned(),
        reached: ctx.steps_reached().to_vec(),
        crashed: ctx.crashed(),
        usage,
        effects: fake.effects(),
    }
}

/// How a run went whose wait nothing would end, so its executor gave up:
/// a defect, kept in `record` as its outcome.
pub fn stalled(places: &Places, record: Option<&Record>) -> Ran {
    let outcome = Outcome::Fault(Box::new(Fault {
        kind: FaultKind::Defect("a wait nothing would end".into()),
        frames: Vec::new(),
    }));
    if let Some(record) = record {
        record.push(Entry::Outcome(crate::record::outcome(&outcome, places)));
    }
    Ran {
        outcome,
        undelivered: None,
        reached: Vec::new(),
        crashed: false,
        usage: Vec::new(),
        effects: Vec::new(),
    }
}

/// What a test's host gives besides its stores: every store of files in
/// memory (area 13, "Provider calls and capabilities in tests"; chunks
/// 6.4b and 7.1c), for the connections that keep files and no other, and
/// every email sent through the connection `email:` names caught rather
/// than sent (area 16, "Email"; chunk 7.3), which `email.sent` reads.
#[derive(Debug, Default)]
pub struct Caught {
    files: MemoryFiles,
    file_stores: Vec<String>,
    mail: crate::capability::Fake,
    mail_connection: Option<String>,
}

impl Caught {
    /// What `plan`'s tests are given: its stores of files and its email
    /// connection.
    pub fn of(plan: Plan<'_>) -> Caught {
        Caught::default()
            .files_for(plan.file_stores())
            .mail_for(plan.mail_connection().map(str::to_string))
    }

    /// Keeps files for the connections in `stores`.
    pub fn files_for<'s>(mut self, stores: impl IntoIterator<Item = &'s str>) -> Caught {
        self.file_stores = stores.into_iter().map(str::to_string).collect();
        self
    }

    /// Catches the mail sent through `connection`.
    pub fn mail_for(mut self, connection: Option<String>) -> Caught {
        self.mail_connection = connection;
        self
    }

    /// What fills `connection`: the caught mail, or the files in memory.
    pub fn capability(&self, connection: &str) -> Option<&dyn Capability> {
        if self.mail_connection.as_deref() == Some(connection) {
            return Some(&self.mail);
        }
        self.file_stores
            .iter()
            .any(|store| store == connection)
            .then_some(&self.files as &dyn Capability)
    }
}

/// What a test's database holds when it ends (chunk 8.1c): the rows of
/// each of the application's and the runtime's tables, but the migrations'
/// own and an extension's, by table and in column order, as JSON; a
/// `bytea` column by its length only, since it holds what is sealed, hashed
/// or encrypted. The tables in name order, each table's rows in their
/// text's order, so the same data always reads the same.
const TABLES: &str = "SELECT c.relname::text, a.attname::text, format_type(a.atttypid, a.atttypmod)::text \
    FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
    JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped \
    WHERE n.nspname = 'public' AND c.relkind IN ('r', 'p') AND NOT c.relispartition \
    AND c.relname <> 'jadpo_migrations' \
    AND NOT EXISTS (SELECT 1 FROM pg_depend d WHERE d.classid = 'pg_class'::regclass \
    AND d.objid = c.oid AND d.deptype = 'e') \
    ORDER BY c.relname, a.attnum";

/// Each row `database`, store `store` of a test, holds, as [`TABLES`]
/// reads them: `<store>.<table> <row as JSON>`; or why it couldn't be read.
pub async fn rows(store: &str, database: &dyn Database) -> Result<Vec<String>, String> {
    let mut transaction = database.begin().await.map_err(|e| format!("{e:?}"))?;
    // Times read the same whatever zone the server is in.
    transaction
        .batch("SET LOCAL TIME ZONE 'UTC'")
        .await
        .map_err(|e| format!("{e:?}"))?;
    let columns = transaction
        .query(TABLES, &[])
        .await
        .map_err(|e| format!("{e:?}"))?;
    let mut tables: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for column in &columns {
        let [
            SqlValue::Text(table),
            SqlValue::Text(name),
            SqlValue::Text(ty),
        ] = column.as_slice()
        else {
            continue;
        };
        let quoted = format!("\"{}\"", name.replace('"', "\"\""));
        let read = if ty == "bytea" {
            format!("'<' || octet_length({quoted}) || ' bytes>' AS {quoted}")
        } else {
            quoted
        };
        tables.entry(table.clone()).or_default().push(read);
    }
    let mut out = Vec::new();
    for (table, columns) in &tables {
        let sql = format!(
            "SELECT row_to_json(r)::text FROM (SELECT {} FROM \"{}\") r",
            columns.join(", "),
            table.replace('"', "\"\"")
        );
        let read = transaction
            .query(&sql, &[])
            .await
            .map_err(|e| format!("{e:?}"))?;
        let mut rows: Vec<String> = read
            .into_iter()
            .filter_map(|row| match row.into_iter().next() {
                Some(SqlValue::Text(json)) => Some(json),
                _ => None,
            })
            .collect();
        rows.sort();
        out.extend(rows.into_iter().map(|row| format!("{store}.{table} {row}")));
    }
    let _ = transaction.rollback().await;
    Ok(out)
}

/// The rows of each of `stores`, in name order, as [`rows`] reads them; a
/// store that can't be read says why in a row of its own.
pub async fn all_rows<'s>(
    stores: impl IntoIterator<Item = (&'s str, &'s dyn Database)>,
) -> Vec<String> {
    let mut out = Vec::new();
    for (name, database) in stores {
        match rows(name, database).await {
            Ok(rows) => out.extend(rows),
            Err(error) => out.push(format!("{name} unread {error}")),
        }
    }
    out
}

/// A test's host, `host`, with the test's clock as it ended, `now`, and
/// random bytes drawn from a counter, for running what the test left to
/// run after it (chunk 8.1h): the differential run's batch runs, which a
/// runner runs as a deployable's does, under its host's clock and
/// randomness, where a test's host has neither. The same bytes every run,
/// so both executors' runs make the same identities.
pub struct AtClock<H> {
    host: H,
    now: i64,
    drawn: Cell<u64>,
}

impl<H> std::fmt::Debug for AtClock<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AtClock")
            .field("now", &self.now)
            .field("drawn", &self.drawn.get())
            .finish_non_exhaustive()
    }
}

impl<H: Host> AtClock<H> {
    pub fn new(host: H, now: i64) -> AtClock<H> {
        AtClock {
            host,
            now,
            drawn: Cell::new(0),
        }
    }
}

impl<H: Host> Host for AtClock<H> {
    fn capability(&self, connection: &str) -> Option<&dyn Capability> {
        self.host.capability(connection)
    }

    fn call_log(&self) -> Option<&dyn crate::call_log::CallLog> {
        self.host.call_log()
    }

    fn record(&self) -> Option<&Record> {
        self.host.record()
    }

    fn passkeys(&self) -> Option<&dyn crate::Passkeys> {
        self.host.passkeys()
    }

    fn test_authenticators(&self) -> Option<&dyn crate::TestAuthenticators> {
        self.host.test_authenticators()
    }

    fn database(&self, store: &str) -> Option<&dyn Database> {
        self.host.database(store)
    }

    fn outbound(&self) -> Option<&dyn crate::Outbound> {
        self.host.outbound()
    }

    fn defer(&self, work: LocalFuture<'static, ()>) {
        self.host.defer(work);
    }

    fn budget_ms(&self, invocation: Invocation) -> u64 {
        self.host.budget_ms(invocation)
    }

    fn rows_per_batch(&self) -> u32 {
        self.host.rows_per_batch()
    }

    fn stopping(&self) -> LocalFuture<'_, ()> {
        self.host.stopping()
    }

    fn hand_on(&self, work: LocalFuture<'static, ()>) {
        self.host.hand_on(work);
    }

    fn variable(&self, name: &str) -> Option<String> {
        self.host.variable(name)
    }

    fn random(&self, bytes: &mut [u8]) -> bool {
        for chunk in bytes.chunks_mut(8) {
            self.drawn.set(self.drawn.get() + 1);
            let mut z = self.drawn.get().wrapping_mul(0x9E37_79B9_7F4A_7C15);
            z = (z ^ (z >> 31)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            for (to, from) in chunk.iter_mut().zip(z.to_be_bytes()) {
                *to = from;
            }
        }
        true
    }

    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        self.host.sleep(millis)
    }

    fn yield_now(&self) -> LocalFuture<'_, ()> {
        self.host.yield_now()
    }

    fn monotonic_ms(&self) -> Option<u64> {
        self.host.monotonic_ms()
    }

    fn now_ms(&self) -> Option<i64> {
        Some(self.now)
    }

    fn rate_limit<'a>(
        &'a self,
        key: &'a str,
        limit: jadpo_std::http::RateLimit,
        now: i64,
    ) -> LocalFuture<'a, crate::Allowed> {
        self.host.rate_limit(key, limit, now)
    }

    fn slots(&self) -> &crate::slots::Slots {
        self.host.slots()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test's seed and delivery order follow its key and the run's
    /// order seed, the same on every platform: these are the numbers
    /// `jadpo test` has drawn since chunk 6.11, so records made before
    /// the run moved here read the same.
    #[test]
    fn a_tests_seed_and_order_follow_its_key() {
        assert_eq!(seed("test sizes \"a\""), seed("test sizes \"a\""));
        assert_ne!(seed("test sizes \"a\""), seed("test sizes \"b\""));
        assert_eq!(splitmix(0), 0xe220_a839_7b1d_cdaf);
        assert_eq!(
            delivery_order(1, "test sizes \"a\""),
            splitmix(1 ^ digest("test sizes \"a\""))
        );
        assert_eq!(digest(""), 0xcbf2_9ce4_8422_2325);
    }

    /// A test's host catches the email connection's mail and keeps the
    /// stores of files in memory, and gives nothing for any other
    /// connection.
    #[test]
    fn a_tests_host_catches_mail_and_keeps_files_for_their_connections() {
        let caught = Caught::default()
            .files_for(["uploads"])
            .mail_for(Some("postmark".into()));
        assert!(caught.capability("uploads").is_some());
        assert!(caught.capability("postmark").is_some());
        assert!(caught.capability("payments").is_none());
    }
}
