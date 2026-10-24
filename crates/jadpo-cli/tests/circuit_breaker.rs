//! The outbox's circuit breaker, against live PostgreSQL and a provider
//! started in the test (area 12, question 18; chunk 7.2b): after a run of
//! `unavailable` failures from one provider, the deliveries that call it
//! through its connection are held rather than attempted; after a pause
//! one probe delivery is tried, a failed probe holding the rest for a
//! longer pause and a successful one releasing them; held deliveries keep
//! their retry budget, their attempts and their `within:`; the tick raises
//! an alert once; and `jadpo deliveries` shows what is held and why.
//!
//! A shipping app: `ship` records a shipment and emits `Shipped`, which
//! `track_shipped` delivers to `track`, an action that looks the parcel up
//! through the `couriers` connection: a safe call, so a `503` is
//! `unavailable` and the delivery retried. The courier is a local HTTP
//! server answering what each test scripts.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::runner::{Deferred, run_deferred};
use jadpo_runtime::schedules::tick;
use jadpo_runtime::schedules::watch::AlertKind;
use jadpo_runtime::{Executor, Host, Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

use crate::support::fake_provider::FakeProvider;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn files(sandbox: &str) -> Vec<(&'static str, String)> {
    vec![
        (
            "app.jadpo",
            r#"application {
    events: {
        store: main
    }
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
            timeout: 5s
        }
    }
}
"#
            .to_string(),
        ),
        (
            "config.jadpo",
            r#"config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/package.jadpo",
            r#"/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/provider.jadpo",
            format!(
                r#"/// Acme Couriers' API.
provider {{
    protocol: Protocol.json_http
    hosts: {{
        live: "https://api.acme-couriers.example"
        sandbox: "{sandbox}"
    }}
    credential: CredentialScheme.bearer
    error_code: "error.code"
}}
"#
            ),
        ),
        (
            "shared/providers/acme_couriers/v1/resources/parcels.jadpo",
            r#"public resource parcels {
    get: {
        method: HttpMethod.get
        path: "/v1/parcels/{parcelId}"
        input: ParcelLookup
        output: Parcel
        changes: false
        failures: [ParcelUnknown]
        source: PackageSource.written
    }
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo",
            r#"/// Which parcel to look up.
public type ParcelLookup = Object {
    parcel_id: ParcelId {
        wire_name: "parcelId"
    }
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/types/parcel_id.jadpo",
            "/// The courier's parcel reference.\npublic type ParcelId = Text {\n    max_length: 40\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/types/parcel.jadpo",
            r#"/// A parcel and where it is.
public type Parcel = Object {
    id: Text
    status: Text
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/failures/parcel_unknown.jadpo",
            r#"/// No parcel has that reference.
public type ParcelUnknown = Failure {
    kind: FailureKind.not_found
    message: "No parcel has that reference."
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/failures/parcel_missing.jadpo",
            "type ParcelMissing = Failure {\n    kind: FailureKind.not_found\n    message: \"We can't find that parcel.\"\n}\n"
                .to_string(),
        ),
        (
            "domains/shipping/entities/shipment.jadpo",
            r#"use shared/providers/acme_couriers

entity Shipment {
    id: Uuid
    parcel: AcmeCouriers.ParcelId

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/events/shipped.jadpo",
            r#"use shared/providers/acme_couriers

/// A parcel was handed to the courier.
type Shipped = Event {
    parcel: AcmeCouriers.ParcelId
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/ship.jadpo",
            r#"use shipping/entities { Shipment }
use shipping/events { Shipped }
use shared/providers/acme_couriers

action ship(parcel: AcmeCouriers.ParcelId) -> Shipment {
    var shipment: Shipment = Shipment.create {
        set: {
            parcel: parcel
        }
    }
    emit Shipped {
        parcel: parcel
    }
    return shipment
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/track.jadpo",
            r#"use app { couriers }
use shipping/failures { ParcelMissing }
use shared/providers/acme_couriers

/// Where a shipped parcel is.
action track(parcel: AcmeCouriers.ParcelId) fails ParcelMissing -> AcmeCouriers.Parcel {
    return attempt couriers.parcels.get(AcmeCouriers.ParcelLookup {
        parcel_id: parcel
    }) {
        parcel_unknown: ParcelMissing
    }
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/subscribers/track_shipped.jadpo",
            r#"use shipping/actions { track }
use shipping/events { Shipped }

/// Looks each shipped parcel up.
subscriber track_shipped {
    on: Shipped
    run: track(event.parcel)
    retry: {
        attempts: 10
        within: 5m
    }
}
"#
            .to_string(),
        ),
    ]
}

fn jadpo(cwd: &Path, at: i64, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", &(at / 1000).to_string())
        .with_var("USER", "sam")
        .with_terminals(true, true, true)
        .with_var("TERM", "xterm-256color")
        .with_var("NO_COLOR", "1");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    let mut output = run(&args, &env.with_cwd(cwd));
    output.stdout = output.stdout.replace("\x1b[1m", "").replace("\x1b[0m", "");
    output.stderr = output.stderr.replace("\x1b[1m", "").replace("\x1b[0m", "");
    output
}

/// When the test starts: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;
const SECOND: i64 = 1_000;
/// How many times a refused call is sent in one delivery attempt: the
/// connection's default `retry:`, 3 attempts (area 11, question 8).
const TRIES: usize = 3;

struct Loaded {
    root: PathBuf,
    program: Program,
    _databases: jadpo_test::database::TestDatabases,
    url: String,
}

fn load(name: &str, sandbox: &str) -> Option<Loaded> {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("circuit-breaker-7.2b-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files(sandbox) {
        write(&root, path, &text);
    }
    for args in [
        &["migrate", "new", "initial"][..],
        &["records", "update"],
        &["check"],
    ] {
        let out = jadpo(&root, T0, args);
        assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    }
    if !jadpo_test::live() {
        return None;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let prepared = Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|problems| {
        panic!(
            "the project checks: {:?}",
            problems.iter().map(|p| &p.message).collect::<Vec<_>>()
        )
    });
    let variables = |name: &str| -> Option<String> {
        match name {
            "COURIERS_API_KEY" => Some("ck_test_123".into()),
            "COURIERS_ENVIRONMENT" => Some("sandbox".into()),
            _ => None,
        }
    };
    let values = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .unwrap_or_else(|problems| panic!("the settings read: {problems:?}"));
    let program = Program::new(prepared).with_config(&values);
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    Some(Loaded {
        root,
        program,
        _databases: databases,
        url,
    })
}

/// The courier's answer for a parcel it knows.
const FOUND: &str = r#"{"id": "p-1", "status": "out_for_delivery"}"#;

/// The package driver, over the native host's HTTP, for `couriers`.
fn faked<D: jadpo_runtime::Database>(harness: &Harness<D>) -> Faked<&Harness<D>> {
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    Faked::new(harness).with("couriers", driver)
}

/// Ships parcel `parcel` on `host`, giving the work it left for after
/// its response.
async fn ship<D: jadpo_runtime::Database>(
    harness: &Harness<D>,
    host: &dyn Host,
    program: &Program,
    parcel: &str,
) -> Vec<Deferred> {
    let id = program.plan().find("ship").expect("the action");
    let mut ctx = harness.ctx_on(program, host);
    match program.run(&mut ctx, id, vec![Value::text(parcel)]).await {
        Outcome::Value(_) => {}
        other => panic!("the parcel is shipped: {other:?}"),
    }
    ctx.take_deferred()
}

/// Each delivery's parcel, state and attempts, by parcel.
async fn deliveries<D: jadpo_runtime::Database>(
    harness: &Harness<D>,
) -> Vec<(String, String, i32)> {
    harness
        .rows(
            "SELECT \"o\".\"payload\" ->> 'parcel', \"d\".\"status\", \"d\".\"attempts\" FROM \"jadpo_deliveries\" AS \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" ORDER BY 1",
        )
        .await
        .into_iter()
        .map(|row| match row.as_slice() {
            [SqlValue::Text(parcel), SqlValue::Text(status), SqlValue::Int4(attempts)] => {
                (parcel.clone(), status.clone(), *attempts)
            }
            other => panic!("not a delivery: {other:?}"),
        })
        .collect()
}

/// How many deliveries are in `status`.
fn in_state(rows: &[(String, String, i32)], status: &str) -> usize {
    rows.iter().filter(|(_, s, _)| s == status).count()
}

/// The `couriers` breaker's state, failures in a row and next probe.
async fn breaker(
    harness: &Harness<jadpo_host_native::Postgres>,
) -> Option<(String, i32, Option<i64>)> {
    harness
        .rows(
            "SELECT \"state\", \"failures\", \"probe_at\" FROM \"jadpo_circuit_breakers\" WHERE \"connection\" = 'couriers'",
        )
        .await
        .into_iter()
        .next()
        .map(|row| match row.as_slice() {
            [SqlValue::Text(state), SqlValue::Int4(failures), probe] => (
                state.clone(),
                *failures,
                match probe {
                    SqlValue::Timestamp(micros) => Some(micros.div_euclid(1000)),
                    _ => None,
                },
            ),
            other => panic!("not a breaker: {other:?}"),
        })
}

/// Five deliveries in a row find the courier unavailable, so the next are
/// held, never sent; a tick raises one alert; a probe after the pause
/// that fails holds them for a longer pause, and one that succeeds
/// releases them all, each delivered with no attempt spent on being held
/// and its `within:` not counting the time it was held. The connection's
/// own `retry:` (3 attempts by default) sends each refused call three
/// times; a delivery attempt counts once, however many it sent.
#[test]
fn deliveries_to_a_provider_that_keeps_failing_are_held_until_a_probe_succeeds() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("held", &provider.url) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let host = faked(&harness);
    let program = &loaded.program;
    let runtime = Runtime::new().expect("runtime");
    // Six refused delivery attempts, each the call and its two retries.
    for _ in 0..6 * TRIES {
        provider.answer(503, "");
    }
    for _ in 0..8 {
        provider.answer(200, FOUND);
    }
    runtime.block_on(async {
        // Seven parcels shipped: the first five are sent and refused, the
        // run that opens the breaker; the last two are held, never sent.
        for n in 1..=7 {
            let work = ship(&harness, &host, program, &format!("p-{n}")).await;
            run_deferred(program, &host, work).await;
        }
        assert_eq!(provider.requests().len(), 5 * TRIES, "only five sent");
        let rows = deliveries(&harness).await;
        assert_eq!(in_state(&rows, "pending"), 5, "{rows:?}");
        assert_eq!(in_state(&rows, "held"), 2, "{rows:?}");
        for (parcel, status, attempts) in &rows {
            let expected = if status == "held" { 0 } else { 1 };
            assert_eq!(*attempts, expected, "{parcel}: {rows:?}");
        }
        assert_eq!(
            breaker(&harness).await,
            Some(("open".to_string(), 5, Some(T0 + 60 * SECOND)))
        );

        // Thirty seconds on, the retries are due: held at their claim,
        // none sent, their attempts given back. The tick raises the alert
        // once, and the next tick raises nothing.
        harness.set_now(T0 + 30 * SECOND);
        let ticked = tick(program, &host, T0 + 30 * SECOND).await;
        assert!(ticked.problems.is_empty(), "{:?}", ticked.problems);
        assert_eq!(provider.requests().len(), 5 * TRIES, "none sent while open");
        let rows = deliveries(&harness).await;
        assert_eq!(in_state(&rows, "held"), 7, "{rows:?}");
        let alerts: Vec<_> = ticked
            .raised
            .iter()
            .filter(|alert| matches!(alert.kind, AlertKind::ConnectionUnavailable { .. }))
            .collect();
        assert_eq!(alerts.len(), 1, "{:?}", ticked.raised);
        match &alerts[0].kind {
            AlertKind::ConnectionUnavailable {
                connection,
                since,
                held,
            } => {
                assert_eq!(connection, "couriers");
                assert_eq!(*since, T0);
                assert_eq!(*held, 7);
            }
            other => panic!("{other:?}"),
        }
        let event = alerts[0].event();
        assert_eq!(event["event"], "delivery.alert");
        assert_eq!(event["alert"], "connection_unavailable");
        assert_eq!(event["connection"], "couriers");
        harness.set_now(T0 + 31 * SECOND);
        let again = tick(program, &host, T0 + 31 * SECOND).await;
        assert!(again.raised.is_empty(), "{:?}", again.raised);
    });
    {
        // An operator sees what is held, and why.
        let listed = jadpo(
            &loaded.root,
            T0 + 31 * SECOND,
            &["deliveries", "--url", &loaded.url],
        );
        assert_eq!(listed.code, 0, "{}{}", listed.stdout, listed.stderr);
        // The alert's line wraps at 80 columns.
        assert!(
            listed
                .stdout
                .replace("\n  ", " ")
                .contains("`couriers` has been unavailable since 09:00:00: 7 deliveries held, one tried again at 09:01:00"),
            "{}",
            listed.stdout
        );
        assert!(listed.stdout.contains("7 held"), "{}", listed.stdout);
        let held = jadpo(
            &loaded.root,
            T0 + 31 * SECOND,
            &["deliveries", "--state", "held", "--url", &loaded.url],
        );
        assert_eq!(held.code, 0, "{}{}", held.stdout, held.stderr);
        assert!(held.stdout.contains("held since (UTC)"), "{}", held.stdout);
        assert!(held.stdout.contains("‖ held"), "{}", held.stdout);
        assert!(held.stdout.contains("· couriers"), "{}", held.stdout);
        assert!(held.stdout.contains("✓ 7 held"), "{}", held.stdout);
        let json = jadpo(
            &loaded.root,
            T0 + 31 * SECOND,
            &[
                "deliveries",
                "--state",
                "held",
                "--json",
                "--url",
                &loaded.url,
            ],
        );
        assert_eq!(json.code, 0, "{}{}", json.stdout, json.stderr);
        let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
        let items = document["data"]["items"].as_array().expect("items");
        assert_eq!(items.len(), 7);
        assert!(
            items.iter().all(|item| item["heldFor"] == "couriers"),
            "{items:?}"
        );
        assert_eq!(document["data"]["unavailable"][0]["connection"], "couriers");
    }
    runtime.block_on(async {

        // After the pause, one held delivery is the probe: refused, so the
        // rest stay held for twice as long.
        harness.set_now(T0 + 61 * SECOND);
        tick(program, &host, T0 + 61 * SECOND).await;
        assert_eq!(provider.requests().len(), 6 * TRIES, "one probe sent");
        assert_eq!(
            breaker(&harness).await,
            Some(("open".to_string(), 6, Some(T0 + 181 * SECOND)))
        );
        // The probe's retry comes due while the breaker is still open:
        // held too.
        harness.set_now(T0 + 130 * SECOND);
        tick(program, &host, T0 + 130 * SECOND).await;
        assert_eq!(
            provider.requests().len(),
            6 * TRIES,
            "none sent before the pause ends"
        );
        assert_eq!(in_state(&deliveries(&harness).await, "held"), 7);

        // Long after `within:`'s five minutes from shipping, a probe
        // succeeds: the rest are released and delivered, none given up,
        // since the time they were held doesn't count.
        harness.set_now(T0 + 661 * SECOND);
        tick(program, &host, T0 + 661 * SECOND).await;
        assert_eq!(provider.requests().len(), 6 * TRIES + 1, "a second probe");
        assert_eq!(breaker(&harness).await, Some(("closed".to_string(), 0, None)));
        let rows = deliveries(&harness).await;
        assert_eq!(in_state(&rows, "succeeded"), 1, "{rows:?}");
        assert_eq!(in_state(&rows, "pending"), 6, "{rows:?}");
        harness.set_now(T0 + 662 * SECOND);
        tick(program, &host, T0 + 662 * SECOND).await;
        let rows = deliveries(&harness).await;
        assert_eq!(in_state(&rows, "succeeded"), 7, "{rows:?}");
        // Every attempt counted went out: none was spent on being held.
        // Six were refused, each sent three times, and seven answered.
        let attempts: i32 = rows.iter().map(|(_, _, attempts)| attempts).sum();
        assert_eq!(attempts, 13, "{rows:?}");
        assert_eq!(provider.requests().len(), 6 * TRIES + 7);
        // Closed, so a later outage raises the alert again.
        assert_eq!(
            harness
                .count("SELECT count(*) FROM \"jadpo_circuit_breakers\" WHERE \"alerted_at\" IS NOT NULL")
                .await,
            0
        );
    });
}

/// A call the provider answers between failures ends the run: fewer than
/// five in a row hold nothing. A delivery whose call a connection retry
/// got through counts as answered.
#[test]
fn an_answer_between_failures_holds_nothing() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("answered", &provider.url) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let host = faked(&harness);
    let program = &loaded.program;
    for _ in 0..4 * TRIES {
        provider.answer(503, "");
    }
    // Refused once, then answered on the connection's retry.
    provider.answer(503, "");
    provider.answer(200, FOUND);
    for _ in 0..4 * TRIES {
        provider.answer(503, "");
    }
    Runtime::new().expect("runtime").block_on(async {
        for n in 1..=9 {
            let work = ship(&harness, &host, program, &format!("q-{n}")).await;
            run_deferred(program, &host, work).await;
        }
        assert_eq!(provider.requests().len(), 8 * TRIES + 2, "every one sent");
        let rows = deliveries(&harness).await;
        assert_eq!(in_state(&rows, "held"), 0, "{rows:?}");
        assert_eq!(in_state(&rows, "succeeded"), 1, "{rows:?}");
        assert_eq!(
            breaker(&harness).await,
            Some(("closed".to_string(), 4, None))
        );
    });
}

/// Observes production runtime statements, retaining the real database and
/// provider. A failed combined claim must never reach the provider.
struct Observed {
    store: jadpo_host_native::Postgres,
    statements: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    refuse_claim: bool,
}

impl jadpo_runtime::Database for Observed {
    fn begin_at(
        &self,
        isolation: jadpo_runtime::Isolation,
    ) -> jadpo_runtime::LocalFuture<
        '_,
        Result<Box<dyn jadpo_runtime::Transaction>, jadpo_runtime::StoreError>,
    > {
        self.store.begin_at(isolation)
    }
    fn query_once<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> jadpo_runtime::LocalFuture<'a, Result<Vec<jadpo_runtime::Row>, jadpo_runtime::StoreError>>
    {
        self.statements.borrow_mut().push(sql.into());
        if self.refuse_claim && sql.contains("RETURNING \"attempts\"") {
            Box::pin(async { Err(jadpo_runtime::StoreError::Unavailable) })
        } else {
            self.store.query_once(sql, params)
        }
    }
}

#[test]
fn a_connected_production_delivery_reads_breakers_only_in_its_claim() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("claim-snapshot", &provider.url) else {
        return;
    };
    let runtime = Runtime::new().unwrap();
    let statements = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    for refuse_claim in [false, true] {
        let harness = Harness::new(Observed {
            store: jadpo_host_native::postgres(&loaded.url).unwrap(),
            statements: std::rc::Rc::clone(&statements),
            refuse_claim,
        })
        .at(T0 + i64::from(refuse_claim))
        .random_from(if refuse_claim { 100 } else { 0 });
        let host = faked(&harness);
        provider.answer(200, FOUND);
        runtime.block_on(async {
            let work = ship(
                &harness,
                &host,
                &loaded.program,
                if refuse_claim { "refused" } else { "answered" },
            )
            .await;
            statements.borrow_mut().clear();
            run_deferred(&loaded.program, &host, work).await;
            let sql = statements.borrow();
            assert!(
                !sql.iter().any(|s| s == jadpo_runtime::breaker::READ),
                "a second breaker read: {sql:?}"
            );
            assert_eq!(
                sql.iter()
                    .filter(|s| s.contains("RETURNING \"attempts\""))
                    .count(),
                1
            );
            assert!(
                sql.iter()
                    .any(|s| s == &jadpo_runtime::outbox::claim_breakers_sql())
            );
        });
    }
    assert_eq!(
        provider.requests().len(),
        1,
        "the failed claim sent nothing"
    );
}

/// Both competitors must be visible waiting for PostgreSQL's real row lock
/// before it is released. Each owns and joins its thread and connection.
fn competing_statements(
    url: &str,
    lock: &str,
    sql: &str,
    params: [Vec<SqlValue>; 2],
    observed_query: &str,
) -> Vec<Vec<jadpo_runtime::Row>> {
    use jadpo_runtime::Database;
    let runtime = Runtime::new().unwrap();
    runtime.block_on(async {
        let store = jadpo_host_native::postgres(url).unwrap();
        let mut holder = store
            .begin_at(jadpo_runtime::Isolation::ReadCommitted)
            .await
            .unwrap();
        holder.execute(lock, &[]).await.unwrap();
        let children: Vec<_> = params
            .into_iter()
            .map(|params| {
                let url = url.to_string();
                let sql = sql.to_string();
                std::thread::spawn(move || {
                    let rt = Runtime::new().unwrap();
                    let own = jadpo_host_native::postgres(&url).unwrap();
                    rt.block_on(own.query_once(&sql, &params)).unwrap()
                })
            })
            .collect();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut observed = false;
        while std::time::Instant::now() < deadline {
            let rows = store.query_once(observed_query, &[]).await.unwrap();
            if rows == vec![vec![SqlValue::Int4(2)]] {
                observed = true;
                break;
            }
            std::thread::yield_now();
        }
        // Always release/reap, even if the observation failed.
        holder.commit().await.unwrap();
        let result = children
            .into_iter()
            .map(|child| child.join().unwrap())
            .collect();
        assert!(observed, "did not observe both row-lock waiters");
        result
    })
}

#[test]
fn joined_claims_and_probe_leases_have_one_winner_under_observed_contention() {
    use jadpo_runtime::Database;
    let provider = FakeProvider::start();
    let Some(loaded) = load("claim-contention", &provider.url) else {
        return;
    };
    let runtime = Runtime::new().unwrap();
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let host = faked(&harness);
    runtime.block_on(async {
        drop(ship(&harness, &host, &loaded.program, "claim").await);
        harness.rows("INSERT INTO jadpo_circuit_breakers(connection,state,failures,pauses,probe_at) VALUES('a','closed',0,0,NULL),('b','open',5,1,'2026-10-19T08:59:00Z') RETURNING connection").await;
    });
    let id = runtime
        .block_on(
            harness
                .store()
                .query_once("SELECT id FROM jadpo_deliveries", &[]),
        )
        .unwrap()[0][0]
        .clone();
    let names = SqlValue::Array(
        jadpo_runtime::SqlType::Text,
        vec![
            SqlValue::Text("a".into()),
            SqlValue::Text("b".into()),
            SqlValue::Text("absent".into()),
        ],
    );
    let params = vec![
        id.clone(),
        SqlValue::Timestamp(T0 * 1000),
        SqlValue::Timestamp((T0 + 60000) * 1000),
        SqlValue::Json(r#"{"code":"lease_expired"}"#.into()),
        names,
    ];
    let rows = competing_statements(
        &loaded.url,
        "UPDATE jadpo_deliveries SET attempts=attempts",
        &jadpo_runtime::outbox::claim_breakers_sql(),
        [params.clone(), params],
        "SELECT count(*)::int4 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'WITH \"claimed\"%'",
    );
    assert_eq!(rows.iter().filter(|r| !r.is_empty()).count(), 1);
    let won = rows.iter().find(|r| !r.is_empty()).unwrap();
    assert_eq!(won.len(), 2, "all existing breakers, no multiplied claim");
    assert_eq!(won[0][0], SqlValue::Int4(1));
    assert_eq!(won[0][1], SqlValue::Text("a".into()));
    assert_eq!(won[1][1], SqlValue::Text("b".into()));
    let probe = |id| {
        vec![
            SqlValue::Text("b".into()),
            SqlValue::Uuid(id),
            SqlValue::Timestamp((T0 + 60000) * 1000),
            SqlValue::Timestamp(T0 * 1000),
        ]
    };
    let rows = competing_statements(
        &loaded.url,
        "UPDATE jadpo_circuit_breakers SET failures=failures WHERE connection='b'",
        jadpo_runtime::breaker::TAKE_PROBE,
        [probe([1; 16]), probe([2; 16])],
        "SELECT count(*)::int4 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'UPDATE \"jadpo_circuit_breakers\" SET \"state\" = ''probing''%'",
    );
    assert_eq!(
        rows.iter().filter(|r| !r.is_empty()).count(),
        1,
        "exactly one probe"
    );
    assert_eq!(
        runtime
            .block_on(
                harness
                    .store()
                    .query_once("SELECT status,attempts FROM jadpo_deliveries", &[])
            )
            .unwrap(),
        vec![vec![SqlValue::Text("running".into()), SqlValue::Int4(1)]]
    );
}

#[test]
fn a_release_racing_a_waiting_hold_is_recovered_without_spending_an_attempt() {
    use jadpo_runtime::Database;
    let provider = FakeProvider::start();
    let Some(loaded) = load("hold-release", &provider.url) else {
        return;
    };
    let runtime = Runtime::new().unwrap();
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let host = faked(&harness);
    runtime.block_on(async {
        drop(ship(&harness, &host, &loaded.program, "held").await);
        harness.rows("UPDATE jadpo_deliveries SET status='running',attempts=1 RETURNING id").await;
        harness.rows("INSERT INTO jadpo_circuit_breakers(connection,state,failures,pauses,probe_at) VALUES('couriers','open',5,1,'2026-10-19T09:01:00Z') RETURNING connection").await;
        let id = harness.rows("SELECT id FROM jadpo_deliveries").await[0][0].clone();
        let mut holder = harness.store().begin_at(jadpo_runtime::Isolation::ReadCommitted).await.unwrap();
        holder.execute("UPDATE jadpo_deliveries SET attempts=attempts",&[]).await.unwrap();
        let url = loaded.url.clone();
        let child = std::thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            let store = jadpo_host_native::postgres(&url).unwrap();
            rt.block_on(store.query_once(jadpo_runtime::breaker::HOLD,&[id,SqlValue::Int4(1),SqlValue::Text("couriers".into()),SqlValue::Timestamp(T0*1000)])).unwrap()
        });
        let deadline = std::time::Instant::now()+std::time::Duration::from_secs(20);
        let mut observed = false;
        while std::time::Instant::now()<deadline {
            let rows = harness.rows("SELECT count(*)::int4 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'UPDATE \"jadpo_deliveries\" SET \"status\" = ''held''%'").await;
            if rows == vec![vec![SqlValue::Int4(1)]] { observed = true; break; }
            std::thread::yield_now();
        }
        let release = harness.store().query_once(&jadpo_runtime::breaker::succeeded_sql(),&[SqlValue::Text("couriers".into()),SqlValue::Timestamp((T0+1000)*1000)]).await.unwrap();
        holder.commit().await.unwrap();
        let held = child.join().unwrap();
        assert!(observed,"waiting HOLD was never observed");
        assert!(!held.is_empty(),"the old snapshot leaves this delivery for the wake sweep");
        assert_eq!(release,vec![vec![SqlValue::Int8(1),SqlValue::Int8(0)]]);
        assert_eq!(harness.rows("SELECT status,attempts,held_for FROM jadpo_deliveries").await,vec![vec![SqlValue::Text("held".into()),SqlValue::Int4(0),SqlValue::Text("couriers".into())]]);
        let wake = jadpo_runtime::breaker::wake_sql();
        harness.store().query_once(&wake,&[SqlValue::Timestamp((T0+5000)*1000)]).await.unwrap();
        assert_eq!(harness.rows("SELECT status,attempts,held_ms,held_for,held_since FROM jadpo_deliveries").await,
            vec![vec![SqlValue::Text("pending".into()),SqlValue::Int4(0),SqlValue::Int8(5000),SqlValue::Null(jadpo_runtime::SqlType::Text),SqlValue::Null(jadpo_runtime::SqlType::Timestamp)]]);
    });
}
