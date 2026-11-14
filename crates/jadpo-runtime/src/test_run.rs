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
//! delivered before the next ([`delivery_order`]), with the batch runs it
//! starts run to their end at its clock ([`AtClock`]), and each step it
//! reaches counted or crashed at; then its outcome kept. The
//! host it is given holds the test's stores; what a test's host gives
//! besides them is [`Caught`]. [`rows`] reads what a test's database holds
//! when it ends, for its record.
//!
//! A test the compiler generates runs here in place of a body (area 13,
//! "Tests generated for each route"): a route's requests, made into
//! exchanges from plain data by [`exchanges`], so a test Worker makes its
//! own; and a person entity's erasure (chunk W2b), the records the plan's
//! test section says to make, those [`erasure_makes`] finds a value can be
//! made for, then the erasure.
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
use crate::test_steps::Undelivered;
use crate::{
    Clock, Ctx, Database, Env, Executor, Fault, FaultKind, Host, LocalFuture, Outcome, Places,
    Plan, SqlValue,
};

/// How many items a test's host puts in one batch, a file's rows or a
/// query's, so every run a test starts resumes between batches (area 13,
/// "Batches").
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
/// "Randomness").
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
    /// The record each statement, event and outside call is kept in, when
    /// the run keeps one; the host's stores keep theirs in it too.
    pub record: Option<Rc<Record>>,
    /// For a generated test (area 13, "Tests generated for each route"), the requests it
    /// sends in place of running `callable`, which is then not a test's;
    /// empty for every other test.
    pub exchanges: &'a [Exchange],
    /// For the test generated for a person entity (area 13, "Tests
    /// generated for each route"; chunk W2b), the entity's name: the
    /// records its plan makes and the erasure it runs, in place of running
    /// `callable`, which is then not a test's.
    pub erasing: Option<&'a str>,
}

/// One request a generated test sends (area 13, "Tests generated for each route"),
/// through the HTTP component in the test's own context as a route test's
/// is, answered as `generated` says: as its caller, and with what it
/// forces in place of what the route runs.
#[derive(Clone, Debug)]
pub struct Exchange {
    pub request: crate::http::Request,
    pub generated: crate::Generated,
}

/// What a generated test's request was answered with, as a client reads
/// it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Answer {
    /// The first header called `name`, compared without case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
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
    /// What each of a generated test's requests was answered with, in the
    /// order they were sent; empty for every other test.
    pub answers: Vec<Answer>,
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
    ctx.set_test_flags(plan.test_flags(once.callable));
    // The paths its bodies take are kept with the rest of its record
    // (chunk 9.3).
    if once.record.is_some() {
        ctx.track_paths();
    }
    if let Some(order) = once.order {
        // What each step emits is delivered before the next statement
        // (chunk 6.11); each step reached is counted, or crashed at.
        ctx.deliver_between_steps(delivery_order(order, once.key));
        match once.crash {
            Some((at, boundary)) => ctx.crash_at(at, boundary),
            None => ctx.track_steps(),
        }
    }
    let mut answers = Vec::new();
    let outcome = if let Some(person) = once.erasing {
        erasure(executor, &mut ctx, person).await
    } else if once.exchanges.is_empty() {
        executor.run(&mut ctx, once.callable, Vec::new()).await
    } else {
        exchanged(executor, &mut ctx, once.exchanges, &mut answers).await
    };
    let outcome = export_after_erasure(&mut ctx, plan, once.order, outcome).await;
    if let Some(record) = &once.record {
        record.push(Entry::Outcome(crate::record::outcome(
            &outcome,
            executor.places(),
        )));
        for span in crate::record::spans(ctx.spans(), executor.places()) {
            record.push(span);
        }
        for paths in crate::record::paths(&ctx.paths_taken(), executor.places()) {
            record.push(paths);
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
        answers,
    }
}

/// Sends a generated test's requests, each through the HTTP component in
/// the test's context, keeping what each was answered with in `answers`:
/// a `Unit` value, or the fault behind a 500, which ends the test as a
/// route test's does.
async fn exchanged(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    exchanges: &[Exchange],
    answers: &mut Vec<Answer>,
) -> Outcome {
    let app = executor.app();
    for exchange in exchanges {
        ctx.set_generated(exchange.generated.clone());
        let id = crate::http::request_id(ctx, u64::from(ctx.actions()));
        let mut response = crate::http::handle(&app, executor, ctx, &exchange.request, &id).await;
        ctx.set_generated(crate::Generated::default());
        if let Some(fault) = response.fault.take() {
            return Outcome::Fault(fault);
        }
        answers.push(Answer {
            status: response.status,
            headers: response.headers,
            body: String::from_utf8_lossy(&response.body).into_owned(),
        });
    }
    Outcome::Value(jadpo_value::Value::Unit)
}

/// One request a generated route test sends (area 13, "Tests generated
/// for each route"), as plain data a test Worker is sent too: the route
/// it goes to, by its number in the route table, the request, and what it
/// is answered with in place of what the route runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requested {
    pub route: u32,
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub forcing: Forcing,
}

/// What a generated route test's request is answered with in place of
/// what its route runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Forcing {
    Nothing,
    /// The declared failure with this code, its client fields' values as
    /// JSON.
    Failure {
        code: String,
        client: Vec<(String, serde_json::Value)>,
    },
    /// A value of the route's output, as JSON.
    Output(serde_json::Value),
}

/// The exchanges a generated route test with key `key` sends, made for
/// `executor`'s program on this thread from `requests`: each answered as
/// a caller of the kind its route takes, with a new identity from the
/// test's key, and with what it forces read as the program's values.
/// `Err` names what the program lacks that the request needs.
pub fn exchanges(
    executor: &dyn Executor,
    key: &str,
    requests: &[Requested],
) -> Result<Vec<Exchange>, String> {
    let app = executor.app();
    let mut out = Vec::new();
    for requested in requests {
        let route = app
            .table
            .routes
            .get(requested.route as usize)
            .ok_or_else(|| format!("route {} isn't in the program's routes", requested.route))?;
        let caller = route
            .authentication
            .needs_caller()
            .then(|| generated_caller(&app, route, key))
            .flatten();
        let forced = match &requested.forcing {
            Forcing::Nothing => None,
            Forcing::Failure { code, client } => Some(crate::Forced::Failure(Box::new(
                generated_failure(&app, code, client)
                    .ok_or_else(|| format!("the program declares no failure `{code}`"))?,
            ))),
            Forcing::Output(json) => {
                let codec = route.output.ok_or("the route sends no output")?;
                let mut problems = Vec::new();
                let value = app
                    .codecs
                    .read(
                        codec,
                        json,
                        &jadpo_value::json::Path::default(),
                        &mut problems,
                    )
                    .ok_or("its output's value doesn't read")?;
                Some(crate::Forced::Value(value))
            }
        };
        out.push(Exchange {
            request: crate::http::Request {
                method: requested.method.clone(),
                path: requested.path.clone(),
                query: requested.query.clone(),
                headers: requested.headers.clone(),
                body: requested.body.clone().into_bytes(),
                address: Some("generated test".to_string()),
                ..crate::http::Request::default()
            },
            generated: crate::Generated { caller, forced },
        });
    }
    Ok(out)
}

/// A caller of the kind `route` takes, with an identity made from `key`.
fn generated_caller(
    app: &crate::http::App<'_>,
    route: &jadpo_std::http::RoutePlan,
    key: &str,
) -> Option<crate::Caller> {
    use jadpo_std::http::PrincipalKind;
    let identity = app.table.identity.as_ref()?;
    let kind = route.caller.or_else(|| {
        if identity.user.is_some() {
            Some(PrincipalKind::User)
        } else if identity.service.is_some() {
            Some(PrincipalKind::Service)
        } else {
            None
        }
    })?;
    // A new identity from the test's key, read as a stored id is.
    let seed = seed(key);
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&seed.to_be_bytes());
    bytes[8..].copy_from_slice(&splitmix(seed).to_be_bytes());
    let id = crate::from_sql(&SqlValue::Uuid(bytes), &jadpo_std::read::Column::Uuid).ok()?;
    Some(crate::Caller {
        key: format!("{} {id}", kind.name()),
        value: crate::http::identity::principal_of(kind, id, jadpo_value::Value::None, true),
        session: None,
        sign_in: None,
        multi_factor: true,
    })
}

/// The declared failure with code `code`, made as the program raises it,
/// its client fields holding `client`'s values.
fn generated_failure(
    app: &crate::http::App<'_>,
    code: &str,
    client: &[(String, serde_json::Value)],
) -> Option<crate::Failure> {
    let plan = app.failures.iter().find(|plan| plan.code == code)?;
    let codecs = app.table.failure(&plan.key);
    let values = plan
        .fields
        .iter()
        .map(|field| {
            let json = client.iter().find(|(name, _)| *name == field.name);
            let codec = codecs.and_then(|codecs| {
                codecs
                    .client
                    .iter()
                    .find(|(name, _)| *name == field.name)
                    .map(|(_, id)| *id)
            });
            match (json, codec) {
                (Some((_, json)), Some(codec)) => {
                    let mut problems = Vec::new();
                    app.codecs
                        .read(
                            codec,
                            json,
                            &jadpo_value::json::Path::default(),
                            &mut problems,
                        )
                        .unwrap_or(jadpo_value::Value::None)
                }
                _ => jadpo_value::Value::None,
            }
        })
        .collect();
    let slots: Vec<Option<u32>> = (0..plan.fields.len())
        .map(|at| u32::try_from(at).ok())
        .collect();
    crate::Failure::of(plan, &slots, values)
}

/// Which of the records `test` plans can be made, by their places in its
/// `makes`, in order (area 13, "Tests generated for each route"; chunk
/// W2b): the person's own and the records it needs, and each kind of
/// theirs whose values can all be made, with what it needs. A value is
/// made as a fake's generated default is; a type whose rules no such
/// value keeps leaves its kind out, as a route whose types no value can
/// be made for gets only its bad-input test. `None` when the person's own
/// record can't be made, or their export has no plan to check it by.
pub fn erasure_makes(plan: Plan<'_>, test: &jadpo_std::testing::ErasureTest) -> Option<Vec<usize>> {
    use jadpo_std::testing::{Fill, Role};
    if !plan
        .exports
        .iter()
        .any(|export| export.entity == test.person)
    {
        return None;
    }
    let codecs = plan.json.codecs;
    let reads = |json: u32, distinct: bool| {
        let Some(codec) = plan.json.codec_of(json) else {
            return false;
        };
        let mut uuid = || "00000000-0000-4000-8000-000000000000".to_string();
        let mut making = crate::fakes::Making {
            now: Clock::TEST.now(),
            uuid: &mut uuid,
        };
        let mark = distinct.then_some("0a1b2c3d");
        let made = crate::fakes::generated_marked(&codecs, codec, &mut making, mark);
        codecs
            .read_value(codec, &jadpo_value::Value::text(&made.to_string()))
            .is_ok()
    };
    // Whether each can be made, its own values and the records it needs.
    let mut makeable: Vec<bool> = Vec::with_capacity(test.makes.len());
    for making in &test.makes {
        let ok = making.inputs.iter().all(|fill| match fill {
            Fill::Absent => true,
            Fill::Made { json, distinct, .. } => reads(*json, *distinct),
            Fill::Record(at) => makeable.get(*at as usize).copied().unwrap_or(false),
        });
        makeable.push(ok);
    }
    // The person's own, and each kind of theirs that can be made, with
    // everything they need: each record a needed one names was made
    // before it, so one walk back from the last finds them all.
    let own = test
        .makes
        .iter()
        .position(|making| making.role == Role::Person)?;
    if makeable.get(own) != Some(&true) {
        return None;
    }
    let mut used: Vec<bool> = test
        .makes
        .iter()
        .zip(&makeable)
        .map(|(making, ok)| *ok && making.role != Role::Needed)
        .collect();
    for at in (0..test.makes.len()).rev() {
        if used.get(at) != Some(&true) {
            continue;
        }
        let Some(making) = test.makes.get(at) else {
            continue;
        };
        for fill in &making.inputs {
            if let Fill::Record(needed) = fill
                && let Some(slot) = used.get_mut(*needed as usize)
            {
                *slot = true;
            }
        }
    }
    Some(
        used.iter()
            .enumerate()
            .filter(|(_, used)| **used)
            .map(|(at, _)| at)
            .collect(),
    )
}

/// The test generated for person entity `person` (area 13, "Tests
/// generated for each route"; area 15, "Export"; chunk W2b): builders the
/// runner calls make the person's record, one record of each kind that
/// belongs to them and what those need, each a step of its own, as a
/// builder's create in a test is; their export then holds the person and
/// each record of theirs. The person is erased, as a step too, and
/// [`export_after_erasure`] holds their export to nothing once its
/// deliveries are done. A record a builder can't make, or an export that
/// doesn't hold what was made, is a fault naming the entity, never a
/// value.
async fn erasure(executor: &dyn Executor, ctx: &mut Ctx<'_>, person: &str) -> Outcome {
    use jadpo_std::testing::{Fill, Role};
    use jadpo_value::Value;
    let defect = |what: String| {
        Outcome::Fault(Box::new(Fault {
            kind: FaultKind::Defect(what),
            frames: Vec::new(),
        }))
    };
    let plan = executor.plan();
    let Some(test) = plan
        .testing()
        .erasures
        .iter()
        .find(|test| test.person == person)
    else {
        return defect(format!(
            "a generated test erasing a `{person}` with no plan"
        ));
    };
    let Some(chosen) = erasure_makes(plan, test) else {
        return defect(format!(
            "a generated test erasing a `{person}` no builder can make"
        ));
    };
    let mut ids: Vec<Option<Value>> = vec![None; test.makes.len()];
    let mut theirs: i64 = 0;
    for at in chosen {
        let Some(making) = test.makes.get(at) else {
            continue;
        };
        let entity = &making.entity;
        let mut inputs = Vec::with_capacity(making.inputs.len());
        for fill in &making.inputs {
            let value = match fill {
                Fill::Absent => Value::None,
                Fill::Record(made) => match ids.get(*made as usize).cloned().flatten() {
                    Some(id) => id,
                    None => return defect(format!("a `{entity}` made before what it needs")),
                },
                Fill::Made {
                    json,
                    distinct,
                    validator,
                } => {
                    let value = match crate::effects::made_value(ctx, plan.json, *json, *distinct) {
                        Ok(value) => value,
                        Err(why) => return defect(format!("{why}, making a `{entity}`")),
                    };
                    // Kept as its field keeps it, as a builder's value is.
                    let kept = validator
                        .and_then(|number| plan.json.codecs.validators.get(number as usize));
                    match kept {
                        Some(validator) => match validator.store(value) {
                            Some(value) => value,
                            None => {
                                return Outcome::Fault(Box::new(Fault {
                                    kind: FaultKind::DecimalOverflow,
                                    frames: Vec::new(),
                                }));
                            }
                        },
                        None => value,
                    }
                }
            };
            inputs.push(value);
        }
        // With no caller, as a builder's create in a test runs.
        crate::call_as(ctx, None);
        let made = crate::store(
            ctx,
            crate::StoreOp::Write(&making.create),
            plan.failures,
            plan.json,
            &inputs,
            crate::Frame::ENTRY,
        )
        .await;
        let record = match made {
            crate::Stored::Value(Value::Record(record)) => record,
            crate::Stored::Value(_) => {
                return defect(format!("a `{entity}` made without a record"));
            }
            crate::Stored::Failed(failure) | crate::Stored::Conflict { cause: failure, .. } => {
                return defect(format!(
                    "a `{entity}` a builder couldn't make, refused with `{}`",
                    failure.code
                ));
            }
            crate::Stored::Fault(fault) => return Outcome::Fault(fault),
        };
        if let Some(slot) = ids.get_mut(at) {
            *slot = record.get("id").cloned();
        }
        if making.role == Role::Theirs {
            theirs += 1;
        }
        if let Err(ended) =
            crate::test_steps::after_step(executor, ctx, false, crate::Frame::ENTRY).await
        {
            return ended;
        }
    }
    let own = test
        .makes
        .iter()
        .position(|making| making.role == Role::Person)
        .and_then(|at| ids.get(at).cloned().flatten());
    let Some(id) = own else {
        return defect(format!("a generated test with no `{person}` made to erase"));
    };
    let Ok(Value::Text(text)) = jadpo_value::standard::from(&id) else {
        return defect(format!("a `{person}` whose `id` isn't one"));
    };
    // Before the erasure, the export holds them and each record of theirs
    // the test made, so the check after it proves something.
    if let Some(export) = plan.exports.iter().find(|export| export.entity == person) {
        let wanted = 1 + theirs;
        match crate::export::records_held(ctx, export, plan.json, &text).await {
            Ok(held) if held >= wanted => {}
            Ok(held) => {
                return defect(format!(
                    "`{person}.export` holding {held} record(s) before `{person}.erase`, fewer than the {wanted} the test made of theirs"
                ));
            }
            Err(crate::read::Interrupt::Fault(fault)) => return Outcome::Fault(fault),
            Err(_) => {
                return defect(format!(
                    "a read of `{person}.export` before `{person}.erase` that failed"
                ));
            }
        }
    }
    crate::call_as(ctx, None);
    match crate::store(
        ctx,
        crate::StoreOp::Write(&test.erase),
        plan.failures,
        plan.json,
        std::slice::from_ref(&id),
        crate::Frame::ENTRY,
    )
    .await
    {
        crate::Stored::Value(_) => {}
        crate::Stored::Failed(failure) | crate::Stored::Conflict { cause: failure, .. } => {
            return defect(format!("`{person}.erase` refused with `{}`", failure.code));
        }
        crate::Stored::Fault(fault) => return Outcome::Fault(fault),
    }
    if let Err(ended) =
        crate::test_steps::after_step(executor, ctx, false, crate::Frame::ENTRY).await
    {
        return ended;
    }
    Outcome::Value(Value::Unit)
}

/// The check generated for each person a test erases (area 15, "Export"):
/// once its erasure's deliveries are done, an export of them must hold
/// nothing. A test that passed but left records behind is a fault, naming
/// the person's entity, never the data.
async fn export_after_erasure(
    ctx: &mut Ctx<'_>,
    plan: Plan<'_>,
    order: Option<u64>,
    outcome: Outcome,
) -> Outcome {
    if order.is_none() || !matches!(outcome, Outcome::Value(_)) || ctx.undelivered().is_some() {
        return outcome;
    }
    for (entity, id) in ctx.take_erased() {
        let Some(export) = plan.exports.iter().find(|export| export.entity == entity) else {
            continue;
        };
        match crate::export::records_held(ctx, export, plan.json, &id).await {
            Ok(0) => {}
            Ok(records) => {
                return Outcome::Fault(Box::new(Fault {
                    kind: FaultKind::Defect(format!(
                        "`{entity}.export` still holding {records} record(s) after `{entity}.erase`"
                    )),
                    frames: Vec::new(),
                }));
            }
            Err(crate::read::Interrupt::Fault(fault)) => return Outcome::Fault(fault),
            Err(_) => {
                return Outcome::Fault(Box::new(Fault {
                    kind: FaultKind::Defect(format!(
                        "a read of `{entity}.export` after `{entity}.erase` that failed"
                    )),
                    frames: Vec::new(),
                }));
            }
        }
    }
    outcome
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
        answers: Vec::new(),
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
    /// Every event published outward, caught rather than sent (area 12,
    /// "Where events travel"), through the connections in `publishing`.
    published: Published,
    publishing: Vec<String>,
}

/// What a test's host catches in place of each event published outward:
/// every one delivered, nothing sent (area 13, "One fake per
/// capability").
#[derive(Debug, Default)]
struct Published(crate::capability::Fake);

impl Capability for Published {
    fn call<'a>(
        &'a self,
        call: &'a crate::capability::Call,
    ) -> crate::LocalFuture<'a, crate::capability::Answer> {
        self.0.call(call)
    }

    fn source(&self) -> crate::capability::Source {
        crate::capability::Source::Fake
    }
}

impl Caught {
    /// What `plan`'s tests are given: its stores of files and its email
    /// connection.
    pub fn of(plan: Plan<'_>) -> Caught {
        Caught::default()
            .files_for(plan.file_stores())
            .mail_for(plan.mail_connection().map(str::to_string))
            .published_for(plan.publishing_connections())
    }

    /// Catches the events published through the connections in
    /// `connections`.
    pub fn published_for<'s>(mut self, connections: impl IntoIterator<Item = &'s str>) -> Caught {
        self.publishing = connections.into_iter().map(str::to_string).collect();
        self
    }

    /// Keeps files for the connections in `stores`.
    pub fn files_for<'s>(mut self, stores: impl IntoIterator<Item = &'s str>) -> Caught {
        self.file_stores = stores.into_iter().map(str::to_string).collect();
        self
    }

    /// Reads a test's fixture files from the project at `root` (area 13,
    /// "Files").
    pub fn fixtures_under(mut self, root: impl Into<std::path::PathBuf>) -> Caught {
        self.files = std::mem::take(&mut self.files).fixtures_under(root);
        self
    }

    /// Catches the mail sent through `connection`.
    pub fn mail_for(mut self, connection: Option<String>) -> Caught {
        self.mail_connection = connection;
        self
    }

    /// Each file its stores of files keep, `files <key> <n> bytes`, in
    /// key order: what a test's record keeps beside its rows, so both
    /// executors are seen to keep and delete the same bytes (chunk B3).
    pub fn files_kept(&self) -> Vec<String> {
        self.files
            .keys()
            .into_iter()
            .map(|key| {
                let size = self.files.bytes(&key).map_or(0, |bytes| bytes.len());
                format!("files {key} {size} bytes")
            })
            .collect()
    }

    /// What fills `connection`: the caught mail, or the files in memory.
    pub fn capability(&self, connection: &str) -> Option<&dyn Capability> {
        if self.mail_connection.as_deref() == Some(connection) {
            return Some(&self.mail);
        }
        if self.publishing.iter().any(|known| known == connection) {
            return Some(&self.published);
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

/// A test's host, `host`, with the test's clock at a step, `now`, and
/// random bytes drawn from a counter, for the batch runs a test's steps
/// start (area 13, "Batches"), which a runner runs as a deployable's does,
/// under its host's clock and randomness, where a test's host has
/// neither. The same bytes every run, so both executors' runs make the
/// same identities.
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

    /// The host, its randomness carrying on after `drawn` draws: so runs
    /// between a test's steps never draw the same bytes twice.
    pub fn after(self, drawn: u64) -> AtClock<H> {
        self.drawn.set(drawn);
        self
    }

    /// How many draws its randomness has made.
    pub fn drawn(&self) -> u64 {
        self.drawn.get()
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

    fn local_issuer(&self) -> Option<&crate::http::local_issuer::LocalIssuer> {
        self.host.local_issuer()
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

    fn items_per_batch(&self) -> u32 {
        self.host.items_per_batch()
    }

    fn stopping(&self) -> LocalFuture<'_, ()> {
        self.host.stopping()
    }

    fn hand_on(&self, work: crate::runner::HandOn) {
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
