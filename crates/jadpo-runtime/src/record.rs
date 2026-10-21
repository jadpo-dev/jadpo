//! The outcome-and-effect record (docs/compiler-runtime.md §2.5,
//! "Differential test"; roadmap chunk 8.1c): what one invocation did, in
//! the order it happened, written by the runtime whichever executor runs
//! it, so two runs of a test, interpreted and compiled, can be compared
//! line by line.
//!
//! It keeps each statement a store ran (its SQL text, its parameters'
//! types and how many rows it gave or changed, or how the store refused
//! it), each transaction's end, each event written to the outbox with its
//! payload, each outside call's connection, operation and answer, and the
//! outcome ([`outcome`]). Never a statement's parameters' values nor a
//! call's input, which can hold secrets and personal data; a secret value
//! shows as its kind, as everywhere else.
//!
//! Only a test's host keeps one: [`Recorded`] wraps a store so its
//! transactions write to the record, and a host gives the record to the
//! capability seam through [`crate::Host::record`], which is `None`
//! everywhere else, so a deployable pays one call that answers `None` per
//! outside call and nothing per statement.

use std::cell::RefCell;
use std::rc::Rc;

use jadpo_std::Isolation;

use crate::capability::{Answer, Call};
use crate::outcome::{AssertionDetail, Got, Outcome};
use crate::{
    Database, Failure, FaultKind, Frame, LocalFuture, Places, Row, SqlType, SqlValue, StoreError,
    Transaction,
};

/// What one invocation did, in order.
#[derive(Debug, Default)]
pub struct Record {
    entries: RefCell<Vec<Entry>>,
}

/// One thing an invocation did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A statement a store ran.
    Statement {
        sql: String,
        /// Its parameters' types, in order: `uuid`, `text[]`.
        types: Vec<String>,
        ran: Ran,
    },
    /// A transaction ended: committed or rolled back, and how.
    Ended { committed: bool, ran: Ran },
    /// An event written to the outbox: its type's identity key and its
    /// payload, the contract JSON the outbox keeps.
    Event { event: String, payload: String },
    /// An outside call: its connection, its operation and its answer.
    Call {
        connection: String,
        operation: String,
        answer: String,
    },
    /// How the invocation ended ([`outcome`]).
    Outcome(String),
}

/// How a statement or a transaction's end went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ran {
    /// A query, and how many rows it gave.
    Read(usize),
    /// A statement that changes, and how many rows it changed.
    Changed(u64),
    /// Statements without parameters, or a transaction's end.
    Done,
    /// The store refused it: how every driver classifies it, a conflict
    /// with its constraint, never the server's message.
    Refused(String),
}

impl Record {
    pub fn new() -> Record {
        Record::default()
    }

    /// Everything kept so far, in order.
    pub fn entries(&self) -> Vec<Entry> {
        self.entries.borrow().clone()
    }

    /// Keeps `entry`.
    pub fn push(&self, entry: Entry) {
        self.entries.borrow_mut().push(entry);
    }

    /// Keeps a statement and how it went.
    fn statement(&self, sql: &str, params: &[SqlValue], ran: Ran) {
        self.push(Entry::Statement {
            sql: sql.to_string(),
            types: params.iter().map(|param| type_name(&param.ty())).collect(),
            ran,
        });
        if sql == crate::outbox::INSERT_OUTBOX {
            self.events(params);
        }
    }

    /// Each event an outbox row written by `params` holds.
    fn events(&self, params: &[SqlValue]) {
        let (Some(SqlValue::Array(_, keys)), Some(SqlValue::Array(_, payloads))) =
            (params.get(1), params.get(2))
        else {
            return;
        };
        for (key, payload) in keys.iter().zip(payloads) {
            if let (SqlValue::Text(event), SqlValue::Text(payload)) = (key, payload) {
                self.push(Entry::Event {
                    event: event.clone(),
                    payload: payload.clone(),
                });
            }
        }
    }

    /// Keeps an outside call and its answer.
    pub(crate) fn call(&self, call: &Call, answer: &str) {
        self.push(Entry::Call {
            connection: call.connection.clone(),
            operation: call.operation.clone(),
            answer: answer.to_string(),
        });
    }
}

impl Entry {
    /// The entry as one line of the record's canonical form.
    pub fn line(&self) -> String {
        match self {
            Entry::Statement { sql, types, ran } => {
                format!("sql {} | ({}) | {}", one_line(sql), types.join(", "), ran)
            }
            Entry::Ended { committed, ran } => {
                let ended = if *committed { "commit" } else { "rollback" };
                format!("{ended} | {ran}")
            }
            Entry::Event { event, payload } => format!("event {event} {}", one_line(payload)),
            Entry::Call {
                connection,
                operation,
                answer,
            } => format!("call {connection}.{operation} -> {}", one_line(answer)),
            Entry::Outcome(outcome) => format!("outcome {outcome}"),
        }
    }
}

impl std::fmt::Display for Ran {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ran::Read(rows) => write!(f, "read {rows}"),
            Ran::Changed(rows) => write!(f, "changed {rows}"),
            Ran::Done => f.write_str("done"),
            Ran::Refused(why) => write!(f, "refused {why}"),
        }
    }
}

/// `text` on one line: each line break written `\n`.
fn one_line(text: &str) -> String {
    text.replace('\\', "\\\\").replace('\n', "\\n")
}

/// How a store's refusal is kept: its classification, and a conflict's
/// constraint; never a server's message, which names databases and
/// processes.
fn refused(error: &StoreError) -> Ran {
    Ran::Refused(match error {
        StoreError::Conflict { constraint } => format!("conflict {constraint}"),
        StoreError::ConcurrentChange => "concurrent_change".into(),
        StoreError::Unavailable => "unavailable".into(),
        StoreError::Statement(_) => "statement".into(),
        StoreError::Fault(_) => "fault".into(),
    })
}

/// PostgreSQL's name for a parameter's type.
fn type_name(ty: &SqlType) -> String {
    match ty {
        SqlType::Bool => "bool".into(),
        SqlType::Int2 => "int2".into(),
        SqlType::Int4 => "int4".into(),
        SqlType::Int8 => "int8".into(),
        SqlType::Numeric => "numeric".into(),
        SqlType::Text => "text".into(),
        SqlType::Bytes => "bytea".into(),
        SqlType::Uuid => "uuid".into(),
        SqlType::Timestamp => "timestamptz".into(),
        SqlType::Date => "date".into(),
        SqlType::Time => "time".into(),
        SqlType::LocalTimestamp => "timestamp".into(),
        SqlType::Json => "jsonb".into(),
        SqlType::Array(inner) => format!("{}[]", type_name(inner)),
        SqlType::Range(inner) => format!("range of {}", type_name(inner)),
        SqlType::Vector => "vector".into(),
        SqlType::Geography => "geography".into(),
        SqlType::Float8 => "float8".into(),
    }
}

/// How an answer is kept: what it gives, or how it ended.
pub(crate) fn answer(answer: &Answer) -> String {
    match answer {
        Answer::Value(value) => format!("value {value}"),
        Answer::Wire(text) => format!("wire {text}"),
        Answer::Failed(code) => format!("failed {code}"),
        Answer::Unavailable => "unavailable".into(),
        Answer::Unknown => "unknown".into(),
        Answer::TimedOut => "timed_out".into(),
        Answer::RetryAfter(millis) => format!("retry_after {millis}"),
        Answer::Misconfigured => "misconfigured".into(),
        Answer::Generated => "generated".into(),
        Answer::Recorded => "recorded".into(),
    }
}

/// How `outcome` is kept, its sites named by `places`: a value; a
/// failure's code, its fields and the failures it was mapped from; a
/// fault's kind and site; a failed assertion's site and values.
pub fn outcome(outcome: &Outcome, places: &Places) -> String {
    match outcome {
        Outcome::Value(value) => format!("value {value}"),
        Outcome::Failed(failure) => format!("failed {}", failure_text(failure)),
        Outcome::Fault(fault) => format!(
            "fault {} at {}",
            fault_kind(&fault.kind),
            site(fault.site(), places)
        ),
        Outcome::Assertion(assertion) => {
            let detail = match &assertion.detail {
                AssertionDetail::False { comparison: None } => "false".to_string(),
                AssertionDetail::False {
                    comparison: Some((operator, left, right)),
                } => format!("false: {left} {operator} {right}"),
                AssertionDetail::Fails { expected, got } => match got {
                    Got::Value(value) => format!("expected {expected}, got value {value}"),
                    Got::Failure(failure) => {
                        format!("expected {expected}, got {}", failure_text(failure))
                    }
                },
            };
            let parts: Vec<String> = assertion
                .parts
                .iter()
                .map(|part| format!("{}..{} {}", part.start, part.end, part.value))
                .collect();
            format!(
                "assertion at {} {detail}; parts [{}]",
                site(assertion.frames.first().copied(), places),
                parts.join(", ")
            )
        }
    }
}

/// A failure's code, its client and internal fields, and what it was
/// mapped from.
fn failure_text(failure: &Failure) -> String {
    let fields = |fields: &[(String, jadpo_value::Value)]| {
        fields
            .iter()
            .map(|(name, value)| format!("{name}: {value}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut text = format!(
        "{} {{{}}} internal {{{}}}",
        failure.code,
        fields(&failure.client),
        fields(&failure.internal)
    );
    if let Some(broken) = &failure.broken {
        text.push_str(&format!(" broke {broken}"));
    }
    if let Some(cause) = &failure.cause {
        text.push_str(&format!(" from {}", failure_text(cause)));
    }
    text
}

/// A fault's kind: its classification, and what a defect names, but never
/// a store's reason, which names databases and processes.
fn fault_kind(kind: &FaultKind) -> String {
    match kind {
        FaultKind::NotBuiltYet(what)
        | FaultKind::Defect(what)
        | FaultKind::Provider(what)
        | FaultKind::CallInTransaction(what)
        | FaultKind::Diverged(what) => format!("{} ({what})", kind.classification()),
        FaultKind::Store(_) => "store".into(),
        other => other.classification().into(),
    }
}

/// `file:line  label`, or `entry` for none.
fn site(frame: Option<Frame>, places: &Places) -> String {
    frame
        .and_then(|frame| places.place(frame))
        .map_or_else(|| "entry".to_string(), |place| place.to_string())
}

/// A store whose transactions keep each statement in a [`Record`].
pub struct Recorded<D> {
    inner: D,
    record: Rc<Record>,
}

impl<D> std::fmt::Debug for Recorded<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorded").finish_non_exhaustive()
    }
}

impl<D: Database> Recorded<D> {
    pub fn new(inner: D, record: Rc<Record>) -> Recorded<D> {
        Recorded { inner, record }
    }

    /// The store it wraps, whose statements it doesn't keep.
    pub fn inner(&self) -> &D {
        &self.inner
    }
}

impl<D: Database> Database for Recorded<D> {
    fn begin_at(
        &self,
        isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        Box::pin(async move {
            let inner = self.inner.begin_at(isolation).await?;
            Ok(Box::new(RecordedTransaction {
                inner,
                record: Rc::clone(&self.record),
            }) as Box<dyn Transaction>)
        })
    }

    fn begin(&self) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        Box::pin(async move {
            let inner = self.inner.begin().await?;
            Ok(Box::new(RecordedTransaction {
                inner,
                record: Rc::clone(&self.record),
            }) as Box<dyn Transaction>)
        })
    }

    fn query_once<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        Box::pin(async move {
            let rows = self.inner.query_once(sql, params).await;
            self.record.statement(sql, params, read(&rows));
            rows
        })
    }
}

fn read(rows: &Result<Vec<Row>, StoreError>) -> Ran {
    match rows {
        Ok(rows) => Ran::Read(rows.len()),
        Err(error) => refused(error),
    }
}

fn done(result: &Result<(), StoreError>) -> Ran {
    match result {
        Ok(()) => Ran::Done,
        Err(error) => refused(error),
    }
}

struct RecordedTransaction {
    inner: Box<dyn Transaction>,
    record: Rc<Record>,
}

impl Transaction for RecordedTransaction {
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        Box::pin(async move {
            let rows = self.inner.query(sql, params).await;
            self.record.statement(sql, params, read(&rows));
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
            let ran = match &changed {
                Ok(rows) => Ran::Changed(*rows),
                Err(error) => refused(error),
            };
            self.record.statement(sql, params, ran);
            changed
        })
    }

    fn batch<'a>(&'a mut self, sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
        Box::pin(async move {
            let ran = self.inner.batch(sql).await;
            self.record.statement(sql, &[], done(&ran));
            ran
        })
    }

    fn savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        Box::pin(async move {
            let ran = self.inner.savepoint().await;
            self.record.statement(crate::SAVEPOINT, &[], done(&ran));
            ran
        })
    }

    fn release_savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        Box::pin(async move {
            let ran = self.inner.release_savepoint().await;
            self.record
                .statement(crate::RELEASE_SAVEPOINT, &[], done(&ran));
            ran
        })
    }

    fn rollback_to_savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        Box::pin(async move {
            let ran = self.inner.rollback_to_savepoint().await;
            self.record
                .statement(crate::ROLLBACK_TO_SAVEPOINT, &[], done(&ran));
            ran
        })
    }

    fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        let RecordedTransaction { inner, record } = *self;
        let committing = inner.commit();
        Box::pin(async move {
            let ran = committing.await;
            record.push(Entry::Ended {
                committed: true,
                ran: done(&ran),
            });
            ran
        })
    }

    fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        let RecordedTransaction { inner, record } = *self;
        let rolling_back = inner.rollback();
        Box::pin(async move {
            let ran = rolling_back.await;
            record.push(Entry::Ended {
                committed: false,
                ran: done(&ran),
            });
            ran
        })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use jadpo_value::Value;

    use super::*;
    use crate::capability::{Capability, Fake, Faked, Repeat};
    use crate::outcome::{Assertion, Part};
    use crate::{Clock, Ctx, Env, Fault, Host, MemoryHost, block_on};

    /// A store whose every query gives two rows and every change one, and
    /// that refuses a statement naming `taken`.
    #[derive(Default)]
    struct Store {
        seen: Rc<RefCell<Vec<String>>>,
    }

    struct Open {
        seen: Rc<RefCell<Vec<String>>>,
    }

    impl Transaction for Open {
        fn query<'a>(
            &'a mut self,
            sql: &'a str,
            _params: &'a [SqlValue],
        ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
            self.seen.borrow_mut().push(sql.to_string());
            Box::pin(async { Ok(vec![vec![SqlValue::Int4(1)], vec![SqlValue::Int4(2)]]) })
        }

        fn execute<'a>(
            &'a mut self,
            sql: &'a str,
            _params: &'a [SqlValue],
        ) -> LocalFuture<'a, Result<u64, StoreError>> {
            self.seen.borrow_mut().push(sql.to_string());
            let taken = sql.contains("taken");
            Box::pin(async move {
                if taken {
                    Err(StoreError::Conflict {
                        constraint: "notes_title_key".into(),
                    })
                } else {
                    Ok(1)
                }
            })
        }

        fn batch<'a>(&'a mut self, sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
            self.seen.borrow_mut().push(sql.to_string());
            Box::pin(async { Ok(()) })
        }

        fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
            Box::pin(async { Ok(()) })
        }

        fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
            Box::pin(async { Ok(()) })
        }
    }

    impl Database for Store {
        fn begin_at(
            &self,
            _isolation: Isolation,
        ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
            let seen = Rc::clone(&self.seen);
            Box::pin(async move { Ok(Box::new(Open { seen }) as Box<dyn Transaction>) })
        }
    }

    fn lines(record: &Record) -> Vec<String> {
        record.entries().iter().map(Entry::line).collect()
    }

    /// Each statement is kept in the order it ran, with its SQL text, its
    /// parameters' types and how many rows it gave or changed, a refusal
    /// by its classification and constraint, and each transaction's end;
    /// never a parameter's value. The store underneath runs each as it
    /// would without the record.
    #[test]
    fn a_recorded_store_keeps_each_statement_without_its_values() {
        let record = Rc::new(Record::new());
        let store = Recorded::new(Store::default(), Rc::clone(&record));
        block_on(async {
            let mut transaction = store.begin_at(Isolation::ReadCommitted).await.unwrap();
            let rows = transaction
                .query(
                    "SELECT \"id\" FROM \"notes\" WHERE \"title\" = $1",
                    &[SqlValue::Text("a secret title".into())],
                )
                .await
                .unwrap();
            assert_eq!(rows.len(), 2);
            transaction
                .execute(
                    "INSERT INTO \"notes\" VALUES ($1, $2)",
                    &[
                        SqlValue::Uuid([7; 16]),
                        SqlValue::Array(SqlType::Text, vec![SqlValue::Text("x".into())]),
                    ],
                )
                .await
                .unwrap();
            transaction.savepoint().await.unwrap();
            let refused = transaction
                .execute(
                    "INSERT INTO taken VALUES ($1)",
                    &[SqlValue::Null(SqlType::Int8)],
                )
                .await;
            assert!(refused.is_err());
            transaction.rollback_to_savepoint().await.unwrap();
            transaction.commit().await.unwrap();
        })
        .unwrap();
        assert_eq!(
            lines(&record),
            [
                "sql SELECT \"id\" FROM \"notes\" WHERE \"title\" = $1 | (text) | read 2",
                "sql INSERT INTO \"notes\" VALUES ($1, $2) | (uuid, text[]) | changed 1",
                "sql SAVEPOINT \"jadpo_call\" | () | done",
                "sql INSERT INTO taken VALUES ($1) | (int8) | refused conflict notes_title_key",
                "sql ROLLBACK TO SAVEPOINT \"jadpo_call\"; RELEASE SAVEPOINT \"jadpo_call\" | () | done",
                "commit | done",
            ]
        );
        assert!(
            !lines(&record).concat().contains("secret"),
            "no parameter's value is kept"
        );
        assert_eq!(store.inner().seen.borrow().len(), 5);
    }

    /// An outbox row written keeps each of its events with its payload,
    /// after the statement that wrote them.
    #[test]
    fn each_event_written_to_the_outbox_is_kept_with_its_payload() {
        let record = Rc::new(Record::new());
        let store = Recorded::new(Store::default(), Rc::clone(&record));
        block_on(async {
            let mut transaction = store.begin().await.unwrap();
            let texts = |items: &[&str]| {
                SqlValue::Array(
                    SqlType::Text,
                    items
                        .iter()
                        .map(|item| SqlValue::Text((*item).into()))
                        .collect(),
                )
            };
            transaction
                .execute(
                    crate::outbox::INSERT_OUTBOX,
                    &[
                        SqlValue::Array(SqlType::Uuid, vec![SqlValue::Uuid([1; 16])]),
                        texts(&["type RepairBooked", "type RepairDone"]),
                        texts(&["{\"item\":\"kettle\"}", "{\"item\":\n\"lamp\"}"]),
                        SqlValue::Timestamp(0),
                        SqlValue::Null(SqlType::Text),
                        SqlValue::Text("json".into()),
                        SqlValue::Int4(0),
                    ],
                )
                .await
                .unwrap();
            transaction.rollback().await.unwrap();
        })
        .unwrap();
        let lines = lines(&record);
        assert_eq!(
            lines[1..],
            [
                "event type RepairBooked {\"item\":\"kettle\"}".to_string(),
                "event type RepairDone {\"item\":\\n\"lamp\"}".to_string(),
                "rollback | done".to_string(),
            ]
        );
        assert!(lines[0].starts_with("sql INSERT INTO \"jadpo_outbox\""));
    }

    /// An outside call through the capability seam is kept with its
    /// connection, operation and answer, never its input; a host that
    /// keeps no record keeps nothing.
    #[test]
    fn each_outside_call_is_kept_with_its_answer_never_its_input() {
        let record = Rc::new(Record::new());
        let fake = Rc::new(Fake::answering(Answer::Value(Value::text("receipt-1"))));
        let host = Faked::new(MemoryHost::new())
            .with("mail", Rc::clone(&fake) as Rc<dyn Capability>)
            .with_record(Rc::clone(&record));
        let places = Places::new();
        let env = Env {
            seed: Some(1),
            clock: Clock::TEST,
        };
        let mut ctx = Ctx::new(&places, &host, env);
        let call = Call::new(
            "mail",
            "send",
            Value::text("ada@example.com"),
            Repeat::Unkeyed,
        );
        block_on(crate::effects::call(&mut ctx, call))
            .unwrap()
            .unwrap();
        assert_eq!(lines(&record), ["call mail.send -> value \"receipt-1\""]);

        let unrecorded = Faked::new(MemoryHost::new()).with("mail", fake as Rc<dyn Capability>);
        assert!(unrecorded.record().is_none());
        assert!(MemoryHost::new().record().is_none());
    }

    /// The outcome as the record keeps it: a value; a failure's code, its
    /// fields and its cause; a fault's kind and site; a failed
    /// assertion's site, comparison and parts.
    #[test]
    fn an_outcome_is_kept_with_its_code_fields_site_and_values() {
        let mut places = Places::new();
        let callable = places.push(crate::places::CallablePlace {
            file: "main.test.jadpo".into(),
            line: 3,
            label: "test \"sums\"".into(),
            sites: vec![crate::places::SitePlace::line(5)],
        });
        let at = Frame { callable, site: 0 };
        assert_eq!(outcome(&Outcome::Value(Value::Int(3)), &places), "value 3");

        let mut cause = Failure::new("type Gone", "Gone", "gone");
        cause.internal.push(("why".into(), Value::text("moved")));
        let mut failure = Failure::new("type Missing", "Missing", "missing");
        failure.client.push(("id".into(), Value::Int(4)));
        failure.cause = Some(Box::new(cause));
        assert_eq!(
            outcome(&Outcome::Failed(Box::new(failure)), &places),
            "failed missing {id: 4} internal {} from gone {} internal {why: \"moved\"}"
        );

        let fault = Fault {
            kind: FaultKind::IntOverflow,
            frames: vec![at],
        };
        assert_eq!(
            outcome(&Outcome::Fault(Box::new(fault)), &places),
            "fault int_overflow at main.test.jadpo:5  test \"sums\""
        );
        let store = Fault {
            kind: FaultKind::Store("database jadpo_copy_91 is gone".into()),
            frames: Vec::new(),
        };
        assert_eq!(
            outcome(&Outcome::Fault(Box::new(store)), &places),
            "fault store at entry"
        );

        let assertion = Assertion {
            frames: vec![at],
            detail: AssertionDetail::False {
                comparison: Some(("==", Value::Int(3), Value::Int(4))),
            },
            parts: vec![Part {
                start: 40,
                end: 49,
                anchor: 40,
                value: Value::Int(3),
            }],
        };
        assert_eq!(
            outcome(&Outcome::Assertion(Box::new(assertion)), &places),
            "assertion at main.test.jadpo:5  test \"sums\" false: 3 == 4; parts [40..49 3]"
        );
    }
}
