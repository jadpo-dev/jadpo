//! What an invocation did, kept whole enough to read back on the
//! developer's machine (area 17, "The request timeline in `jadpo dev`";
//! chunk E0): beside its spans ([`crate::spans`]), each statement its
//! stores ran, under the span it ran in, with its SQL and its parameters,
//! each transaction's end, each event it emitted, and the failure that
//! ended it with where it was raised.
//!
//! Only a host that keeps timelines asks for one
//! ([`crate::Host::keeps_timelines`]: `jadpo dev`'s), and a test that
//! keeps its trace for `--keep-failed` ([`crate::Ctx::keep_timeline`]), so
//! a deployable pays nothing for it. Its transactions are wrapped as they
//! begin ([`Watch::watched`]), so neither executor adds anything.
//!
//! A parameter is shown by area 4's rule for a failure's internal fields
//! in logs: an identifier, a number, a boolean or a time as its value,
//! text as its keyed hash, so two values can be told apart without either
//! being seen. A statement's parameters carry no type of their own, so an
//! enum's variant, held as text, is hashed as text is.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use serde_json::{Value as Json, json};

use crate::logging::Hasher;
use crate::spans::{Ended, Kind, Spans};
use crate::{Frame, Places};
use crate::{LocalFuture, Row, SqlValue, StoreError, Transaction};

/// The version of the timeline document ([`document`]).
pub const SCHEMA_VERSION: u32 = 1;

/// One thing a timeline keeps, in the order it happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seen {
    /// A statement a store ran, inside span `span` (by its place in the
    /// invocation's list), or `None` outside every span: the runtime's own
    /// bookkeeping, such as an idempotency key.
    Statement {
        span: Option<u32>,
        sql: String,
        /// Each parameter as shown: `0192f0c4-…`, `7`, `true`, a text's
        /// keyed hash.
        params: Vec<String>,
        /// The rows it gave or changed; `None` when the store refused it.
        rows: Option<u64>,
    },
    /// A transaction ended: committed or rolled back, and whether it had
    /// written anything.
    Ended { committed: bool, wrote: bool },
    /// An event written to the outbox, by its type's identity key.
    Emitted { event: String },
}

/// The failure that ended the invocation, and where it was raised.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Raised {
    pub name: String,
    pub code: String,
    pub at: Option<Frame>,
}

/// A timeline as it was kept.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Timeline {
    pub seen: Vec<Seen>,
    /// What each read or write span is of, by its place in the list:
    /// `Todo.get`, `Todo.update`.
    pub names: BTreeMap<u32, String>,
    /// Who it ran as: `user 0192…`, `service …`.
    pub caller: Option<String>,
    pub raised: Option<Raised>,
}

/// A timeline being kept, shared with the transactions it watches.
#[derive(Clone)]
pub struct Watch(Rc<Kept>);

struct Kept {
    /// The innermost span open now, which a statement is kept under.
    current: Cell<Option<u32>>,
    timeline: RefCell<Timeline>,
    hasher: Hasher,
}

/// The key a timeline's hashes are made with when the host's log has
/// none to give: the process's own, so two values compare across its
/// requests.
static KEY: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();

fn key(host: &dyn crate::Host) -> Vec<u8> {
    KEY.get_or_init(|| {
        let mut key = [0u8; 32];
        if !host.random(&mut key) {
            use std::hash::{BuildHasher, Hasher as _};
            for chunk in key.chunks_mut(8) {
                let mut state = std::collections::hash_map::RandomState::new().build_hasher();
                state.write_usize(chunk.as_ptr() as usize);
                for (byte, drawn) in chunk.iter_mut().zip(state.finish().to_le_bytes()) {
                    *byte = drawn;
                }
            }
        }
        key
    })
    .to_vec()
}

impl std::fmt::Debug for Watch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watch").finish_non_exhaustive()
    }
}

impl Watch {
    pub(crate) fn new(host: &dyn crate::Host) -> Watch {
        Watch(Rc::new(Kept {
            current: Cell::new(None),
            timeline: RefCell::new(Timeline::default()),
            hasher: Hasher::of(host).or_key(|| key(host)),
        }))
    }

    /// Statements from now on are kept under span `span`.
    pub(crate) fn set_current(&self, span: Option<u32>) {
        self.0.current.set(span);
    }

    pub(crate) fn name(&self, span: u32, name: String) {
        self.0.timeline.borrow_mut().names.insert(span, name);
    }

    pub(crate) fn set_caller(&self, caller: Option<String>) {
        self.0.timeline.borrow_mut().caller = caller;
    }

    /// Keeps the failure that ended it, unless one already did: the
    /// innermost is the one raised.
    pub(crate) fn raised(&self, failure: &crate::Failure) {
        let mut timeline = self.0.timeline.borrow_mut();
        if timeline.raised.is_none() {
            timeline.raised = Some(Raised {
                name: failure.name.clone(),
                code: failure.code.clone(),
                at: failure.frames.first().copied(),
            });
        }
    }

    /// What it has kept so far.
    pub fn taken(&self) -> Timeline {
        self.0.timeline.borrow().clone()
    }

    /// `transaction`, its statements kept in this timeline.
    pub(crate) fn watched(&self, transaction: Box<dyn Transaction>) -> Box<dyn Transaction> {
        Box::new(Watched {
            inner: transaction,
            watch: self.clone(),
            wrote: false,
        })
    }

    fn statement(&self, sql: &str, params: &[SqlValue], rows: Option<u64>) {
        let mut timeline = self.0.timeline.borrow_mut();
        if sql == crate::outbox::INSERT_OUTBOX {
            if let Some(SqlValue::Array(_, keys)) = params.get(1) {
                for key in keys {
                    if let SqlValue::Text(event) = key {
                        timeline.seen.push(Seen::Emitted {
                            event: event.clone(),
                        });
                    }
                }
            }
            return;
        }
        let params = params
            .iter()
            .map(|param| shown(param, &self.0.hasher))
            .collect();
        timeline.seen.push(Seen::Statement {
            span: self.0.current.get(),
            sql: sql.to_string(),
            params,
            rows,
        });
    }

    fn ended(&self, committed: bool, wrote: bool) {
        self.0
            .timeline
            .borrow_mut()
            .seen
            .push(Seen::Ended { committed, wrote });
    }
}

/// A parameter as a timeline shows it.
fn shown(param: &SqlValue, hasher: &Hasher) -> String {
    let hash = |bytes: &[u8]| hasher.hash(bytes).unwrap_or_else(|| "hidden".to_string());
    match param {
        SqlValue::Null(_) => "none".to_string(),
        SqlValue::Bool(bool) => bool.to_string(),
        SqlValue::Int2(int) => int.to_string(),
        SqlValue::Int4(int) => int.to_string(),
        SqlValue::Int8(int) => int.to_string(),
        SqlValue::Numeric(number) => number.clone(),
        SqlValue::Float8(bits) => f64::from_bits(*bits).to_string(),
        SqlValue::Uuid(bytes) => crate::read::uuid_text(bytes),
        SqlValue::Timestamp(micros) => jadpo_std::format_instant(micros.div_euclid(1000)),
        SqlValue::LocalTimestamp(micros) => {
            let instant = jadpo_std::format_instant(micros.div_euclid(1000));
            instant.trim_end_matches('Z').to_string()
        }
        SqlValue::Date(days) => jadpo_std::format_date(*days),
        SqlValue::Time(micros) => {
            let seconds = micros.div_euclid(1_000_000);
            format!(
                "{:02}:{:02}:{:02}",
                seconds / 3600,
                (seconds / 60) % 60,
                seconds % 60
            )
        }
        SqlValue::Text(text) => hash(text.as_bytes()),
        SqlValue::Json(text) => hash(text.as_bytes()),
        SqlValue::Bytes(bytes) => hash(bytes),
        SqlValue::Array(_, items) => {
            let items: Vec<String> = items.iter().map(|item| shown(item, hasher)).collect();
            format!("[{}]", items.join(", "))
        }
        SqlValue::Range { .. } | SqlValue::Vector(_) | SqlValue::Geography(_) => {
            hash(format!("{param:?}").as_bytes())
        }
    }
}

/// A transaction whose statements a timeline keeps.
struct Watched {
    inner: Box<dyn Transaction>,
    watch: Watch,
    wrote: bool,
}

/// Whether `sql` changes something.
fn writes(sql: &str) -> bool {
    let start = sql.trim_start();
    ["INSERT", "UPDATE", "DELETE"]
        .iter()
        .any(|word| start.starts_with(word))
        || start.starts_with("WITH")
            && [" INSERT ", " UPDATE ", " DELETE "]
                .iter()
                .any(|word| start.contains(word))
}

impl Transaction for Watched {
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        Box::pin(async move {
            let rows = self.inner.query(sql, params).await;
            self.wrote |= writes(sql) && sql != crate::outbox::INSERT_OUTBOX;
            let count = rows.as_ref().ok().map(|rows| rows.len() as u64);
            self.watch.statement(sql, params, count);
            rows
        })
    }

    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<u64, StoreError>> {
        Box::pin(async move {
            let changed = self.inner.execute(sql, params).await;
            self.wrote |= writes(sql) && sql != crate::outbox::INSERT_OUTBOX;
            self.watch
                .statement(sql, params, changed.as_ref().ok().copied());
            changed
        })
    }

    fn batch<'a>(&'a mut self, sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
        self.inner.batch(sql)
    }

    fn savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        self.inner.savepoint()
    }

    fn release_savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        self.inner.release_savepoint()
    }

    fn rollback_to_savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        self.inner.rollback_to_savepoint()
    }

    fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        let Watched {
            inner,
            watch,
            wrote,
        } = *self;
        Box::pin(async move {
            let committed = inner.commit().await;
            watch.ended(committed.is_ok(), wrote);
            committed
        })
    }

    fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        let Watched {
            inner,
            watch,
            wrote,
        } = *self;
        Box::pin(async move {
            watch.ended(false, wrote);
            inner.rollback().await
        })
    }
}

/// What a read or a write is of, for its span: a query's reads are named
/// after the query, from its place, so this names the rest.
pub(crate) fn store_name(op: crate::store::StoreOp<'_>) -> String {
    use crate::store::StoreOp;
    match op {
        StoreOp::Read(plan) => {
            let ty = &plan.select.record.ty;
            match plan.cardinality {
                jadpo_std::read::Cardinality::One | jadpo_std::read::Cardinality::Optional => {
                    format!("{ty}.get")
                }
                _ => ty.clone(),
            }
        }
        StoreOp::Write(plan) => {
            let entity = plan.returns.as_ref().map_or_else(
                || table_of(&plan.sql).unwrap_or_else(|| "record".to_string()),
                |returned| returned.record.ty.clone(),
            );
            format!("{entity}.{}", plan.op.word())
        }
        StoreOp::History(_) => "history".to_string(),
        StoreOp::Export(_) => "export".to_string(),
    }
}

/// The table a `DELETE FROM "todos" …` names.
fn table_of(sql: &str) -> Option<String> {
    let rest = sql.split_once("FROM \"")?.1;
    Some(rest.split('"').next()?.to_string())
}

/// The label of `callable`'s place: `query list_todos`, `action add_todo`.
fn label(places: &Places, callable: Option<u32>) -> Option<&str> {
    callable.map(|callable| places.label(callable))
}

/// A label's name: `list_todos` from `query list_todos`.
fn bare(label: &str) -> &str {
    label.split_once(' ').map_or(label, |(_, name)| name)
}

/// The document a timeline is kept and read as (area 17, "Everything as
/// JSON"): each span with what it is, where, how long, how many rows and
/// how it ended, and under each read and write the statements it ran; the
/// statements no span holds; each transaction's end; the events emitted;
/// and the failure that ended it, with its place. What the invocation was,
/// a request or a test, its host adds.
pub fn document(spans: &Spans, timeline: &Timeline, places: &Places) -> Json {
    let place = |frame: Option<Frame>| {
        frame
            .and_then(|frame| places.place(frame))
            .map_or(Json::Null, |place| {
                Json::from(format!("{}:{}", place.file, place.line))
            })
    };
    let statement = |sql: &str, params: &[String], rows: Option<u64>| json!({ "sql": sql, "params": params, "rows": rows });
    let mut kept: BTreeMap<u32, Vec<Json>> = BTreeMap::new();
    let mut outside = Vec::new();
    let mut transactions = Vec::new();
    let mut emitted = Vec::new();
    for seen in &timeline.seen {
        match seen {
            Seen::Statement {
                span,
                sql,
                params,
                rows,
            } => {
                let one = statement(sql, params, *rows);
                match span {
                    Some(span) => kept.entry(*span).or_default().push(one),
                    None => outside.push(one),
                }
            }
            Seen::Ended { committed, wrote } => {
                transactions.push(json!({ "committed": committed, "wrote": wrote }));
            }
            Seen::Emitted { event } => {
                emitted.push(Json::from(bare(event)));
            }
        }
    }
    let list: Vec<Json> = spans
        .iter()
        .enumerate()
        .map(|(at, span)| {
            let at = at as u32;
            let label = label(places, span.callable);
            let (kind, name) = match &span.kind {
                Kind::Action => ("action", label.map_or("action", bare).to_string()),
                Kind::Read => (
                    "read",
                    match label {
                        Some(label) if label.starts_with("query ") => bare(label).to_string(),
                        _ => timeline
                            .names
                            .get(&at)
                            .cloned()
                            .unwrap_or_else(|| "read".to_string()),
                    },
                ),
                Kind::Write => (
                    "write",
                    timeline
                        .names
                        .get(&at)
                        .cloned()
                        .unwrap_or_else(|| "write".to_string()),
                ),
                Kind::Call {
                    connection,
                    operation,
                } => ("call", format!("{connection}.{operation}")),
            };
            let (ended, code) = match &span.ended {
                Ended::Open => ("open", None),
                Ended::Value => ("value", None),
                Ended::Failed(code) => ("failed", Some(code.clone())),
                Ended::Fault(classification) => ("fault", Some(classification.to_string())),
            };
            // Where in its file the operation is written, so a reader with
            // the source names a read as the code calls it.
            let site = span
                .frame()
                .and_then(|frame| places.site(frame))
                .map(|(file, site)| json!({ "file": file, "start": site.start, "end": site.end }));
            json!({
                "kind": kind,
                "name": name,
                "place": place(span.frame()),
                "site": site,
                "parent": span.parent,
                "ms": span.ms(),
                "rows": span.rows,
                "ended": ended,
                "code": code,
                "statements": kept.remove(&at).unwrap_or_default(),
            })
        })
        .collect();
    json!({
        "schemaVersion": SCHEMA_VERSION,
        "caller": timeline.caller,
        "spans": list,
        "spansDropped": spans.dropped(),
        "runtimeStatements": outside,
        "transactions": transactions,
        "emitted": emitted,
        "failure": timeline.raised.as_ref().map(|raised| json!({
            "name": raised.name,
            "code": raised.code,
            "place": place(raised.at),
        })),
    })
}
