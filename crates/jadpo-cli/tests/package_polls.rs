//! Polls and `max_concurrent:`, against live PostgreSQL (area 11,
//! "Polls" and "Calling"; area 12, "Events from outside"; chunk 7.6c).
//!
//! Acme Fuel publishes its pump prices with no webhooks, so its package
//! polls them: the app's tick calls `prices.latest` through the `fuel`
//! connection, at the connection's interval, and keeps `PricesPublished`
//! only when the prices it carries changed, for the app's subscriber to
//! keep. A poll nothing subscribes to never runs, and one whose call
//! fails keeps nothing and is tried again a minute later. Under
//! `max_concurrent: 1`, a call waits while the connection's slot is out,
//! and a delivery that would call through it is given back to the outbox
//! to run later.
//!
//! Acme Fuel's API is a provider started in the test on this machine
//! (`support::fake_provider`); nothing reaches a network. The server is
//! the one `JADPO_TEST_DATABASE_URL` names or one started from the
//! machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::rc::Rc;
use std::task::Poll;
use std::time::Duration;

use jadpo_cli::{Output, run};
use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::schedules::tick;
use jadpo_runtime::{Host, Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;

use crate::support::fake_provider::FakeProvider;

const HOME: &str = "shared/providers/fuel_prices/v1";

/// When the test starts: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;
const HOUR: i64 = 60 * 60 * 1000;

/// The project, Acme Fuel's sandbox at `sandbox`, its connection capped
/// at `max_concurrent`, with the app's `more` files.
fn files(sandbox: &str, max_concurrent: u32, more: &[(&str, &str)]) -> Vec<(String, String)> {
    let package = |path: &str, text: &str| (format!("{HOME}/{path}"), text.to_string());
    let mut out = vec![
        (
            "app.jadpo".to_string(),
            format!(
                "application {{\n    connections: {{\n        main: {{\n            driver: Driver.postgres\n        }}\n        fuel: {{\n            package: FuelPrices.v1\n            domains: [pricing]\n            credential: config.fuel_key\n            environment: config.fuel_environment\n            max_concurrent: {max_concurrent}\n            polls: {{\n                latest: 6h\n            }}\n        }}\n    }}\n    events: {{\n        store: main\n    }}\n}}\n"
            ),
        ),
        (
            "config.jadpo".into(),
            "config {\n    fuel_key: Text {\n        secret: true\n    }\n    fuel_environment: ProviderEnvironment\n}\n".into(),
        ),
        package(
            "package.jadpo",
            "/// Acme Fuel's price list.\npackage FuelPrices.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
        ),
        package(
            "provider.jadpo",
            &format!(
                "/// Acme Fuel's price list, which sends no webhooks.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://api.fuel-prices.example\"\n        sandbox: \"{sandbox}\"\n    }}\n    credential: CredentialScheme.bearer\n    max_concurrent: 4\n    polls: {{\n        latest: {{\n            call: prices.latest\n            every: 1d\n            emits: PricesPublished\n        }}\n    }}\n}}\n"
            ),
        ),
        package(
            "types/price_list.jadpo",
            "/// The day's prices, as the API gives them.\npublic type PriceList = Object {\n    region: Text\n    diesel_pence: Int\n    petrol_pence: Int\n    fetched_at: Instant\n}\n",
        ),
        package(
            "resources/prices.jadpo",
            "public resource prices {\n    latest: {\n        method: HttpMethod.get\n        path: \"/v1/prices/latest\"\n        output: PriceList\n        source: PackageSource.written\n    }\n}\n",
        ),
        package(
            "events/prices_published.jadpo",
            "/// The day's pump prices, published.\npublic type PricesPublished = Event {\n    region: Text\n    diesel_pence: Int\n    petrol_pence: Int\n}\n",
        ),
        (
            "domains/pricing/entities/pump_price.jadpo".into(),
            "entity PumpPrice {\n    id: Uuid\n    region: Text\n    diesel_pence: Int\n    petrol_pence: Int\n\n    policy: {\n        create: [Access.system]\n        read: [Access.system]\n    }\n}\n".into(),
        ),
        (
            "domains/pricing/actions/keep_prices.jadpo".into(),
            "use pricing/entities { PumpPrice }\nuse shared/providers/fuel_prices\n\n/// Keeps the prices Acme Fuel published.\naction keep_prices(event: FuelPrices.PricesPublished) -> PumpPrice {\n    return PumpPrice.create {\n        set: {\n            region: event.region\n            diesel_pence: event.diesel_pence\n            petrol_pence: event.petrol_pence\n        }\n    }\n}\n".into(),
        ),
        (
            "domains/pricing/subscribers/prices_published.jadpo".into(),
            "use pricing/actions { keep_prices }\nuse shared/providers/fuel_prices\n\n/// Keeps each change of price.\nsubscriber prices_published {\n    on: FuelPrices.PricesPublished\n    run: keep_prices(event)\n}\n".into(),
        ),
    ];
    for (path, text) in more {
        out.retain(|(written, _)| written != path);
        if !text.is_empty() {
            out.push((path.to_string(), text.to_string()));
        }
    }
    out
}

/// A second subscriber, whose action looks the prices up again through
/// `fuel`: a delivery that calls out through the capped connection.
const CHECKING: &[(&str, &str)] = &[
    (
        "domains/pricing/actions/check_prices.jadpo",
        "use app { fuel }\nuse shared/providers/fuel_prices\n\n/// Looks the prices up again as Acme Fuel gives them now.\naction check_prices(event: FuelPrices.PricesPublished) -> FuelPrices.PriceList {\n    return fuel.prices.latest()\n}\n",
    ),
    (
        "domains/pricing/subscribers/prices_checked.jadpo",
        "use pricing/actions { check_prices }\nuse shared/providers/fuel_prices\n\n/// Checks each change of price against the list.\nsubscriber prices_checked {\n    on: FuelPrices.PricesPublished\n    run: check_prices(event)\n}\n",
    ),
];

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", &(T0 / 1000).to_string())
        .with_var("NO_COLOR", "1");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

struct Loaded {
    program: Program,
    _databases: jadpo_test::database::TestDatabases,
    url: String,
}

/// The project of `files`, checked, with its migration, and a test
/// database of its own; `None` when live tests are skipped.
fn load(name: &str, files: Vec<(String, String)>) -> Option<Loaded> {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("package-polls-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    for args in [&["migrate", "new", "initial"][..], &["check"]] {
        let out = jadpo(&root, args);
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
            "FUEL_KEY" => Some("fk_test_key".into()),
            "FUEL_ENVIRONMENT" => Some("sandbox".into()),
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
        program,
        _databases: databases,
        url,
    })
}

/// The host a tick runs on: the test's database and clock, with the
/// package driver, over this machine's HTTP, for `fuel`.
fn host(url: &str) -> Faked<Harness<jadpo_host_native::Postgres>> {
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch));
    Faked::new(Harness::postgres(url).at(T0).random_from(0)).with("fuel", driver)
}

/// Acme Fuel's price list, fetched at `fetched_at`.
fn prices(diesel: i64, fetched_at: &str) -> String {
    format!(
        r#"{{"region":"uk","diesel_pence":{diesel},"petrol_pence":140,"fetched_at":"{fetched_at}"}}"#
    )
}

/// Ticks at `now`, the host's clock moved there first.
async fn tick_at(
    program: &Program,
    host: &Faked<Harness<jadpo_host_native::Postgres>>,
    now: i64,
) -> jadpo_runtime::schedules::Ticked {
    host.host().set_now(now);
    tick(program, host, now).await
}

/// The prices kept, in the order kept.
async fn kept(host: &Faked<Harness<jadpo_host_native::Postgres>>) -> Vec<Vec<SqlValue>> {
    host.host()
        .rows("SELECT \"region\", \"diesel_pence\" FROM \"pump_price\" ORDER BY \"diesel_pence\"")
        .await
}

/// The first tick calls the poll through `fuel`, with its credential, and
/// keeps the prices; one before the connection's six hours calls nothing;
/// one after calls again and keeps nothing while the prices are the same,
/// whenever the list was fetched; and a change of price is kept again.
#[test]
fn a_poll_keeps_only_the_changes_it_sees() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("changes", files(&provider.url, 2, &[])) else {
        return;
    };
    let program = &loaded.program;
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        provider.answer(200, &prices(150, "2026-10-19T08:59:00Z"));
        let ticked = tick_at(program, &host, T0).await;
        assert!(ticked.problems.is_empty(), "{:?}", ticked.problems);
        assert_eq!(ticked.polled, Some((1, 1)));
        assert!(
            ticked
                .deliveries
                .iter()
                .all(|outcome| matches!(outcome, Outcome::Value(_))),
            "{:?}",
            ticked.deliveries
        );
        let sent = provider.requests();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].method, "GET");
        assert_eq!(sent[0].path, "/v1/prices/latest");
        assert_eq!(sent[0].header("authorization"), Some("Bearer fk_test_key"));
        assert_eq!(
            kept(&host).await,
            vec![vec![SqlValue::Text("uk".into()), SqlValue::Int8(150)]]
        );
        let state = host
            .host()
            .rows("SELECT \"poll\", \"next_at\" FROM \"jadpo_polls\"")
            .await;
        assert_eq!(
            state,
            vec![vec![
                SqlValue::Text("poll fuel.latest".into()),
                SqlValue::Timestamp((T0 + 6 * HOUR) * 1000),
            ]]
        );

        // Not due yet: the connection's interval is six hours.
        let early = tick_at(program, &host, T0 + HOUR).await;
        assert_eq!(early.polled, None);
        assert_eq!(provider.requests().len(), 1);

        // Due: the same prices, fetched later, change nothing.
        provider.answer(200, &prices(150, "2026-10-19T15:00:00Z"));
        let same = tick_at(program, &host, T0 + 6 * HOUR).await;
        assert!(same.problems.is_empty(), "{:?}", same.problems);
        assert_eq!(same.polled, Some((1, 0)));
        assert_eq!(provider.requests().len(), 2);
        assert_eq!(
            host.host()
                .count("SELECT count(*) FROM \"jadpo_outbox\"")
                .await,
            1
        );
        assert_eq!(kept(&host).await.len(), 1);

        // A new price is a change, kept and delivered.
        provider.answer(200, &prices(152, "2026-10-19T21:00:00Z"));
        let changed = tick_at(program, &host, T0 + 12 * HOUR).await;
        assert!(changed.problems.is_empty(), "{:?}", changed.problems);
        assert_eq!(changed.polled, Some((1, 1)));
        assert_eq!(
            kept(&host).await,
            vec![
                vec![SqlValue::Text("uk".into()), SqlValue::Int8(150)],
                vec![SqlValue::Text("uk".into()), SqlValue::Int8(152)],
            ]
        );
    });
}

/// With nothing subscribed to `PricesPublished`, the poll never runs: no
/// call, and no state kept.
#[test]
fn a_poll_nothing_subscribes_to_never_runs() {
    let provider = FakeProvider::start();
    let unheard = [("domains/pricing/subscribers/prices_published.jadpo", "")];
    let Some(loaded) = load("unheard", files(&provider.url, 2, &unheard)) else {
        return;
    };
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let ticked = tick_at(&loaded.program, &host, T0).await;
        assert!(ticked.problems.is_empty(), "{:?}", ticked.problems);
        assert_eq!(ticked.polled, None);
    });
    assert!(provider.requests().is_empty());
}

/// A call the provider fails keeps nothing and names the poll; the poll is
/// tried again a minute later, not on the next tick before then.
#[test]
fn a_failed_poll_keeps_nothing_and_is_tried_again_a_minute_later() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("failed", files(&provider.url, 2, &[])) else {
        return;
    };
    let program = &loaded.program;
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        // Sent three times, as `retry:` allows a call that changes nothing.
        for _ in 0..3 {
            provider.answer(503, r#"{"error":{"code":"down"}}"#);
        }
        let failed = tick_at(program, &host, T0).await;
        let problems: Vec<&str> = failed
            .problems
            .iter()
            .map(|(what, _)| what.as_str())
            .collect();
        assert_eq!(problems, ["poll fuel.latest"]);
        assert_eq!(failed.polled, Some((1, 0)));
        assert_eq!(provider.requests().len(), 3);
        assert!(kept(&host).await.is_empty());

        let soon = tick_at(program, &host, T0 + 30_000).await;
        assert_eq!(soon.polled, None);
        assert_eq!(provider.requests().len(), 3);

        provider.answer(200, &prices(150, "2026-10-19T09:01:00Z"));
        let again = tick_at(program, &host, T0 + 60_000).await;
        assert!(again.problems.is_empty(), "{:?}", again.problems);
        assert_eq!(again.polled, Some((1, 1)));
        assert_eq!(kept(&host).await.len(), 1);
    });
}

/// Polls `future` for `millis` milliseconds: its output if it finished.
async fn for_a_while<F: Future + Unpin>(future: &mut F, millis: u64) -> Option<F::Output> {
    let mut sleep = Box::pin(tokio::time::sleep(Duration::from_millis(millis)));
    std::future::poll_fn(|cx| {
        if let Poll::Ready(output) = Pin::new(&mut *future).poll(cx) {
            return Poll::Ready(Some(output));
        }
        if sleep.as_mut().poll(cx).is_ready() {
            return Poll::Ready(None);
        }
        Poll::Pending
    })
    .await
}

/// Under `max_concurrent: 1`, the poll's call waits while the
/// connection's one slot is out, and goes once it is given back.
#[test]
fn a_call_waits_while_its_connections_slots_are_out() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("waits", files(&provider.url, 1, &[])) else {
        return;
    };
    let program = &loaded.program;
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        provider.answer(200, &prices(150, "2026-10-19T08:59:00Z"));
        let slots = host.host().slots();
        let out = slots.try_take("fuel", 1).expect("the one slot");
        let mut ticking = Box::pin(tick(program, &host, T0));
        assert!(for_a_while(&mut ticking, 500).await.is_none());
        assert!(provider.requests().is_empty());
        // It was claimed, and waits for the slot.
        assert!(slots.full("fuel"));
        drop(out);
        let ticked = ticking.await;
        assert!(ticked.problems.is_empty(), "{:?}", ticked.problems);
        assert_eq!(ticked.polled, Some((1, 1)));
        assert_eq!(provider.requests().len(), 1);
        assert_eq!(slots.busy("fuel"), 0);
    });
}

/// A delivery whose action calls through `fuel` is given back to the
/// outbox, its attempt not counted and nothing sent, while the
/// connection's one slot is out; once it is given back, the next sweep
/// runs it.
#[test]
fn a_delivery_waits_in_the_outbox_while_its_connections_slots_are_out() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("deliveries", files(&provider.url, 1, CHECKING)) else {
        return;
    };
    let program = &loaded.program;
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    let state = || async {
        host.host()
            .rows("SELECT \"status\", \"attempts\" FROM \"jadpo_deliveries\" WHERE \"subscriber\" = 'subscriber prices_checked'")
            .await
    };
    runtime.block_on(async {
        // The poll's call answers; the check's is refused three times, so
        // its delivery waits to be tried again.
        provider.answer(200, &prices(150, "2026-10-19T08:59:00Z"));
        for _ in 0..3 {
            provider.answer(503, r#"{"error":{"code":"down"}}"#);
        }
        let ticked = tick_at(program, &host, T0).await;
        assert_eq!(ticked.polled, Some((1, 1)));
        assert_eq!(provider.requests().len(), 4);
        assert_eq!(
            state().await,
            vec![vec![SqlValue::Text("pending".into()), SqlValue::Int4(1)]]
        );

        let out = host
            .host()
            .slots()
            .try_take("fuel", 1)
            .expect("the one slot");
        let held = tick_at(program, &host, T0 + HOUR).await;
        assert!(held.deliveries.is_empty(), "{:?}", held.deliveries);
        assert_eq!(provider.requests().len(), 4);
        assert_eq!(
            state().await,
            vec![vec![SqlValue::Text("pending".into()), SqlValue::Int4(1)]]
        );
        let due = host
            .host()
            .rows("SELECT \"next_attempt_at\" FROM \"jadpo_deliveries\" WHERE \"subscriber\" = 'subscriber prices_checked'")
            .await;
        assert_eq!(
            due,
            vec![vec![SqlValue::Timestamp((T0 + HOUR + 1_000) * 1000)]]
        );

        drop(out);
        provider.answer(200, &prices(150, "2026-10-19T10:00:00Z"));
        let ran = tick_at(program, &host, T0 + HOUR + 2_000).await;
        assert!(
            ran.deliveries
                .iter()
                .all(|outcome| matches!(outcome, Outcome::Value(_))),
            "{:?}",
            ran.deliveries
        );
        assert_eq!(provider.requests().len(), 5);
        assert_eq!(
            state().await,
            vec![vec![SqlValue::Text("succeeded".into()), SqlValue::Int4(2)]]
        );
    });
}
