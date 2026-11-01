//! A test Worker (roadmap chunk 8.2e; docs/compiler-runtime.md §2.5,
//! "Differential test", and §2.7, "The differential run"): compiled
//! programs built for wasm32 with the `testing` feature, behind a handler
//! that runs one test by its key, so `jadpo-test` runs a project's route
//! tests inside workerd and compares each one's record with the
//! interpreter's.
//!
//! Its crate exports `fetch`, which hands [`fetch`] the programs it holds
//! and each request. A request is one run of one test ([`Asked`], JSON
//! posted to the Worker): which program, the test's key, the run's order
//! seed for a test with a database, the address of a copy of each store's
//! database on the run's server, which the Worker reaches over
//! `connect()`, a crash to make and whether to keep a record. The Worker runs it as `jadpo test` runs one
//! ([`jadpo_runtime::test_run::once`]): on a host of its own holding those
//! stores, their statements kept in the record, with every file kept in
//! memory and every email caught, as a test's host has them, no randomness
//! and no timer; then reads the rows each store holds when a store was
//! used. It answers what the run did ([`Answered`]): the record's lines,
//! the rows, how it ended, the fakes' use, the steps it reached and what
//! its keyed calls did, so the runner judges it and crashes it again as it
//! judges and crashes a run of its own.
//!
//! The program is made once per isolate, with the settings a test run
//! reads (`test:` values, defaults, and throwaway secrets from Web Crypto),
//! as a Workers deployable reads its own (`Target::Workers`). Its passkey
//! ceremonies and the software authenticator `test_passkeys` drives them
//! with are the runtime's, as natively, so a route test that makes
//! passkeys runs here too.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use jadpo_runtime::compiled::Program;
use jadpo_runtime::config::{self, Mode};
use jadpo_runtime::crashes::{Boundary, Reached};
use jadpo_runtime::http::address::Target;
use jadpo_runtime::record::{Entry, Record, Recorded};
use jadpo_runtime::test_authenticator::SoftwareAuthenticators;
use jadpo_runtime::test_run::{self, Caught, Once};
use jadpo_runtime::{Database, Executor, Host, LocalFuture, MemoryHost, Outcome, StoreError};
use serde_json::{Value, json};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::ResponseInit;

use crate::{PASSKEYS, WorkersTransport, web_crypto_random};

/// What a crate of compiled programs gives for each: its program on this
/// thread.
pub type MakeProgram = fn() -> Result<Program, jadpo_std::Unreadable>;

/// One run of one test, as a test Worker is asked for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    /// The program's place among those the Worker holds.
    pub program: usize,
    /// The test's identity key.
    pub key: String,
    /// For a test with a database, the run's order seed; `None` for one
    /// without, which runs with no store.
    pub order: Option<u64>,
    /// Each store's database, a copy made for this run, by its address
    /// over TCP.
    pub stores: BTreeMap<String, String>,
    /// The step, from 1, and the boundary to crash the run at.
    pub crash: Option<(usize, Boundary)>,
    /// Whether to keep the run's record, with the key its digests are
    /// keyed with: the asking process's own, so they are compared with its
    /// records' (chunk 8.4a).
    pub record: Option<[u8; 32]>,
}

impl Asked {
    /// As JSON, the body posted to the Worker.
    pub fn to_json(&self) -> String {
        json!({
            "program": self.program,
            "key": self.key,
            "order": self.order,
            "stores": self.stores,
            "crash": self.crash.map(|(at, boundary)| json!({
                "at": at,
                "boundary": boundary.word(),
            })),
            "record": self.record.map(|key| key.iter().map(|byte| format!("{byte:02x}")).collect::<String>()),
        })
        .to_string()
    }

    /// Read from the JSON [`Asked::to_json`] writes.
    pub fn read(text: &str) -> Result<Asked, String> {
        let value: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
        let crash = match at(&value, "crash") {
            Value::Null => None,
            crash => {
                let step = number(at(crash, "at"), "crash.at")?;
                let boundary = Boundary::ALL
                    .into_iter()
                    .find(|boundary| at(crash, "boundary").as_str() == Some(boundary.word()))
                    .ok_or("`crash.boundary` isn't a boundary")?;
                Some((step, boundary))
            }
        };
        let stores = match at(&value, "stores") {
            Value::Object(stores) => stores
                .iter()
                .map(|(store, url)| {
                    url.as_str()
                        .map(|url| (store.clone(), url.to_string()))
                        .ok_or_else(|| format!("store `{store}`'s address isn't text"))
                })
                .collect::<Result<_, _>>()?,
            _ => return Err("`stores` isn't an object".into()),
        };
        Ok(Asked {
            program: number(at(&value, "program"), "program")?,
            key: text_of(at(&value, "key"), "key")?,
            order: at(&value, "order").as_u64(),
            stores,
            crash,
            record: match at(&value, "record") {
                Value::Null => None,
                key => Some(digest_key(key)?),
            },
        })
    }
}

/// A record's key from its 64 hex digits.
fn digest_key(value: &Value) -> Result<[u8; 32], String> {
    let text = value.as_str().ok_or("`record` isn't text")?;
    let mut key = [0; 32];
    if text.len() != 64 {
        return Err("`record` isn't a key".into());
    }
    for (at, byte) in key.iter_mut().enumerate() {
        *byte = text
            .get(at * 2..at * 2 + 2)
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            .ok_or("`record` isn't a key")?;
    }
    Ok(key)
}

/// What one run of a test did, as a test Worker answers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answered {
    /// The record's lines ([`Entry::line`]), its outcome's last, when the
    /// run kept one.
    pub entries: Vec<String>,
    /// What its stores held when it ended, when it used one.
    pub rows: Vec<String>,
    /// How it ended: `value`, `assertion`, `failed` or `fault`.
    pub ending: String,
    /// Whether a delivery between its steps failed for good.
    pub undelivered: bool,
    /// Each scripted operation's and prompt's use: called past its
    /// script, and with answers left.
    pub fakes: Vec<(bool, bool)>,
    /// The steps it reached under a record.
    pub reached: Vec<Reached>,
    /// Whether the crash it was asked for happened.
    pub crashed: bool,
    /// What its keyed provider calls did, by operation.
    pub effects: Vec<(String, usize)>,
}

impl Answered {
    /// As JSON, the Worker's answer.
    pub fn to_json(&self) -> String {
        json!({
            "entries": self.entries,
            "rows": self.rows,
            "ending": self.ending,
            "undelivered": self.undelivered,
            "fakes": self.fakes.iter().map(|(exhausted, unused)| json!({
                "exhausted": exhausted,
                "unused": unused,
            })).collect::<Vec<_>>(),
            "reached": self.reached.iter().map(|step| json!({
                "action": step.action,
                "position": step.position,
                "operation": step.operation,
            })).collect::<Vec<_>>(),
            "crashed": self.crashed,
            "effects": self.effects.iter().map(|(operation, times)| json!([operation, times])).collect::<Vec<_>>(),
        })
        .to_string()
    }

    /// Read from the JSON [`Answered::to_json`] writes.
    pub fn read(text: &str) -> Result<Answered, String> {
        let value: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
        let texts = |name: &str| -> Result<Vec<String>, String> {
            list(at(&value, name), name)?
                .iter()
                .map(|line| text_of(line, name))
                .collect()
        };
        Ok(Answered {
            entries: texts("entries")?,
            rows: texts("rows")?,
            ending: text_of(at(&value, "ending"), "ending")?,
            undelivered: at(&value, "undelivered").as_bool().unwrap_or(false),
            fakes: list(at(&value, "fakes"), "fakes")?
                .iter()
                .map(|used| {
                    (
                        at(used, "exhausted").as_bool().unwrap_or(false),
                        at(used, "unused").as_bool().unwrap_or(false),
                    )
                })
                .collect(),
            reached: list(at(&value, "reached"), "reached")?
                .iter()
                .map(|step| {
                    Ok(Reached {
                        action: text_of(at(step, "action"), "reached.action")?,
                        position: u32::try_from(number(at(step, "position"), "reached.position")?)
                            .map_err(|error| error.to_string())?,
                        operation: text_of(at(step, "operation"), "reached.operation")?,
                    })
                })
                .collect::<Result<_, String>>()?,
            crashed: at(&value, "crashed").as_bool().unwrap_or(false),
            effects: list(at(&value, "effects"), "effects")?
                .iter()
                .map(|effect| {
                    Ok((
                        text_of(at(effect, 0), "effects")?,
                        number(at(effect, 1), "effects")?,
                    ))
                })
                .collect::<Result<_, String>>()?,
        })
    }
}

/// `value`'s member or item `key`, or `null` when it has none.
fn at<I: serde_json::value::Index>(value: &Value, key: I) -> &Value {
    value.get(key).unwrap_or(&Value::Null)
}

fn number(value: &Value, name: &str) -> Result<usize, String> {
    value
        .as_u64()
        .and_then(|number| usize::try_from(number).ok())
        .ok_or_else(|| format!("`{name}` isn't a whole number"))
}

fn text_of(value: &Value, name: &str) -> Result<String, String> {
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("`{name}` isn't text"))
}

fn list<'v>(value: &'v Value, name: &str) -> Result<&'v Vec<Value>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("`{name}` isn't a list"))
}

/// The test Worker's `fetch` for the programs `programs`: a promise of the
/// `Response` to `request`, a run of a test ([`Asked`]) answered with what
/// it did ([`Answered`]), or 400 or 500 with why it couldn't run.
pub fn fetch(
    programs: &'static [MakeProgram],
    request: JsValue,
    env: JsValue,
    ctx: JsValue,
) -> JsValue {
    let _ = (env, ctx);
    wasm_bindgen_futures::future_to_promise(async move {
        let (status, body) = match asked(request).await {
            Err(why) => (400, json!({ "error": why }).to_string()),
            Ok(asked) => match ran(programs, &asked).await {
                Ok(answered) => (200, answered.to_json()),
                Err(why) => (500, json!({ "error": why }).to_string()),
            },
        };
        let init = ResponseInit::new();
        init.set_status(status);
        web_sys::Response::new_with_opt_str_and_init(Some(&body), &init).map(JsValue::from)
    })
    .into()
}

/// The run `request` asks for.
async fn asked(request: JsValue) -> Result<Asked, String> {
    let request = request
        .dyn_into::<web_sys::Request>()
        .map_err(|_| "not a request".to_string())?;
    let text = request.text().map_err(|error| crate::js::said(&error))?;
    let text = JsFuture::from(text)
        .await
        .map_err(|error| crate::js::said(&error))?
        .as_string()
        .ok_or("the body isn't text")?;
    Asked::read(&text)
}

thread_local! {
    /// Each program the isolate has made, by its place.
    static MADE: RefCell<BTreeMap<usize, Rc<Program>>> = const { RefCell::new(BTreeMap::new()) };
}

/// The program at `place`, made once per isolate with the settings a test
/// run reads: each field's `test:` value, its default, and throwaway
/// secrets (area 10).
fn program(programs: &[MakeProgram], place: usize) -> Result<Rc<Program>, String> {
    if let Some(made) = MADE.with(|made| made.borrow().get(&place).cloned()) {
        return Ok(made);
    }
    let make = programs
        .get(place)
        .ok_or_else(|| format!("the Worker holds no program {place}"))?;
    let program = make().map_err(|error| format!("its plan can't be read: {error}"))?;
    let no_variables = |_: &str| -> Option<String> { None };
    let values = config::load(
        &program.runtime().config,
        program.validators(),
        &no_variables,
        Mode::Test,
        &web_crypto_random,
    )
    .ok();
    let program = Rc::new(match values {
        Some(values) => program.with_config_on(&values, Target::Workers),
        None => program,
    });
    MADE.with(|made| made.borrow_mut().insert(place, Rc::clone(&program)));
    Ok(program)
}

/// Runs the test `asked` names, as `jadpo test` runs one, and what it did.
async fn ran(programs: &[MakeProgram], asked: &Asked) -> Result<Answered, String> {
    let program = program(programs, asked.program)?;
    let executor: &dyn Executor = &*program;
    let callable = executor
        .plan()
        .callable(&asked.key)
        .ok_or_else(|| format!("the program has no test {}", asked.key))?;
    let record = asked.record.map(|key| Rc::new(Record::new(key)));
    let once = Once {
        key: &asked.key,
        callable,
        order: asked.order,
        crash: asked.crash,
        record: record.clone(),
    };
    // A test without a database runs on a host with no store, as natively.
    let (ran, rows) = if asked.order.is_some() {
        let host = TestHost::new(&asked.stores, record.as_ref(), executor);
        let ran = test_run::once(executor, &host, once).await;
        let rows = host.rows().await;
        (ran, rows)
    } else {
        (
            test_run::once(executor, &MemoryHost::new(), once).await,
            Vec::new(),
        )
    };
    Ok(Answered {
        entries: record.map_or_else(Vec::new, |record| {
            record.entries().iter().map(Entry::line).collect()
        }),
        rows,
        ending: match ran.outcome {
            Outcome::Value(_) => "value",
            Outcome::Assertion(_) => "assertion",
            Outcome::Failed(_) => "failed",
            Outcome::Fault(_) => "fault",
        }
        .to_string(),
        undelivered: ran.undelivered.is_some(),
        fakes: ran
            .usage
            .iter()
            .map(|usage| (usage.exhausted(), usage.unused()))
            .collect(),
        reached: ran.reached,
        crashed: ran.crashed,
        effects: ran.effects,
    })
}

/// One of a test's stores: its copy, whose statements the run's record
/// keeps when it keeps one, or why it can't be reached.
enum Store {
    #[cfg(feature = "postgres")]
    Plain(jadpo_drivers::postgres::Postgres),
    #[cfg(feature = "postgres")]
    Recorded(Recorded<jadpo_drivers::postgres::Postgres>),
    Unready(Unready),
}

impl Store {
    /// The copy at `url` over `connect()`, kept in `record` when given.
    fn at(store: &str, url: &str, record: Option<&Rc<Record>>) -> Store {
        #[cfg(feature = "postgres")]
        {
            match jadpo_drivers::postgres::Postgres::new(url, Rc::new(WorkersTransport::default()))
            {
                Ok(database) => match record {
                    Some(record) => Store::Recorded(Recorded::new(database, Rc::clone(record))),
                    None => Store::Plain(database),
                },
                // Never the address, which may hold a password.
                Err(_) => Store::Unready(Unready(format!(
                    "the test database for `{store}` can't be used"
                ))),
            }
        }
        #[cfg(not(feature = "postgres"))]
        {
            let _ = (url, record);
            Store::Unready(Unready(format!(
                "the test database for `{store}` needs the `postgres` feature"
            )))
        }
    }

    fn database(&self) -> &dyn Database {
        match self {
            #[cfg(feature = "postgres")]
            Store::Plain(database) => database,
            #[cfg(feature = "postgres")]
            Store::Recorded(recorded) => recorded,
            Store::Unready(why) => why,
        }
    }

    /// The copy itself, whose statements no record keeps.
    fn raw(&self) -> &dyn Database {
        match self {
            #[cfg(feature = "postgres")]
            Store::Plain(database) => database,
            #[cfg(feature = "postgres")]
            Store::Recorded(recorded) => recorded.inner(),
            Store::Unready(why) => why,
        }
    }
}

/// A store that refuses with why the test's database can't be had, so the
/// test reports it as a fault where it used it.
struct Unready(String);

impl Database for Unready {
    fn begin_at(
        &self,
        _isolation: jadpo_runtime::Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn jadpo_runtime::Transaction>, StoreError>> {
        let why = self.0.clone();
        Box::pin(async move { Err(StoreError::Fault(why)) })
    }
}

/// A test's host on Workers: a copy of each store's database, reached over
/// `connect()`; every file in memory and every email caught; the passkey
/// ceremonies, their challenges from Web Crypto, and the test's software
/// authenticators; no randomness, since a test runs under its seed, and
/// no timer, so nothing runs out of time.
struct TestHost {
    stores: BTreeMap<String, Store>,
    /// Whether the test used a store, whose rows its record then holds.
    used: Cell<bool>,
    caught: Caught,
    /// The authenticators `test_passkeys.register` made, each holding one
    /// passkey.
    authenticators: SoftwareAuthenticators,
    /// The local issuer in the configured external-token issuer's place,
    /// its key from Web Crypto the first time it is asked for, as
    /// natively.
    issuer: std::cell::OnceCell<Option<jadpo_runtime::http::local_issuer::LocalIssuer>>,
}

impl TestHost {
    fn new(
        stores: &BTreeMap<String, String>,
        record: Option<&Rc<Record>>,
        executor: &dyn Executor,
    ) -> TestHost {
        TestHost {
            stores: stores
                .iter()
                .map(|(store, url)| (store.clone(), Store::at(store, url, record)))
                .collect(),
            used: Cell::new(false),
            caught: Caught::of(executor.plan()),
            authenticators: SoftwareAuthenticators::default(),
            issuer: std::cell::OnceCell::new(),
        }
    }

    /// The rows its stores hold now, when the test used one.
    async fn rows(&self) -> Vec<String> {
        if !self.used.get() {
            return self.caught.files_kept();
        }
        let mut rows = test_run::all_rows(
            self.stores
                .iter()
                .map(|(name, store)| (name.as_str(), store.raw())),
        )
        .await;
        // The bytes its stores of files keep, after the rows (chunk B3).
        rows.extend(self.caught.files_kept());
        rows
    }
}

impl Host for TestHost {
    fn passkeys(&self) -> Option<&dyn jadpo_runtime::Passkeys> {
        Some(&PASSKEYS)
    }

    /// The authenticators `test_passkeys` makes, as `jadpo test`'s host
    /// has them.
    fn test_authenticators(&self) -> Option<&dyn jadpo_runtime::TestAuthenticators> {
        Some(&self.authenticators)
    }

    fn local_issuer(&self) -> Option<&jadpo_runtime::http::local_issuer::LocalIssuer> {
        self.issuer
            .get_or_init(|| jadpo_runtime::http::local_issuer::LocalIssuer::new(&web_crypto_random))
            .as_ref()
    }

    fn database(&self, store: &str) -> Option<&dyn Database> {
        self.used.set(true);
        self.stores.get(store).map(Store::database)
    }

    /// None: a test runs under its seed, so it reproduces.
    fn random(&self, _bytes: &mut [u8]) -> bool {
        false
    }

    fn capability(&self, connection: &str) -> Option<&dyn jadpo_runtime::capability::Capability> {
        self.caught.capability(connection)
    }

    fn rows_per_batch(&self) -> u32 {
        test_run::ROWS_PER_BATCH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run asked for and what it did read back from the JSON each is
    /// sent as.
    #[test]
    fn a_run_and_its_answer_read_back_from_their_json() {
        let asked = Asked {
            program: 1,
            key: "test \"a todo is created\"".into(),
            order: Some(u64::MAX),
            stores: BTreeMap::from([("main".into(), "postgresql://jadpo@127.0.0.1:5432/t".into())]),
            crash: Some((2, Boundary::AfterSend)),
            record: Some(core::array::from_fn(|at| at as u8 * 7)),
        };
        assert_eq!(Asked::read(&asked.to_json()), Ok(asked));
        let without = Asked {
            program: 0,
            key: "test \"x\"".into(),
            order: None,
            stores: BTreeMap::new(),
            crash: None,
            record: None,
        };
        assert_eq!(Asked::read(&without.to_json()), Ok(without));
        let answered = Answered {
            entries: vec![
                "sql SELECT 1 | () | read 1".into(),
                "outcome value ()".into(),
            ],
            rows: vec!["main.todo {\"id\":\"x\"}".into()],
            ending: "assertion".into(),
            undelivered: true,
            fakes: vec![(false, true), (true, false)],
            reached: vec![Reached {
                action: "pay_invoice".into(),
                position: 1,
                operation: "pay.payments.create".into(),
            }],
            crashed: true,
            effects: vec![("pay.payments.create".into(), 1)],
        };
        assert_eq!(Answered::read(&answered.to_json()), Ok(answered));
        assert!(Asked::read("{\"program\": \"one\"}").is_err());
    }
}
