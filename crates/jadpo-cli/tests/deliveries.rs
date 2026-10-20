//! `jadpo deliveries` against live PostgreSQL (area 12, "What an operator
//! sees"; area 14, question 13; chunk 6.12): the deliveries and schedule
//! runs that need an operator are listed, and retried, dismissed,
//! cancelled and resolved, each only from the state it applies to and
//! each logged with who did it; a retried delivery gets a fresh budget,
//! and a schedule's failed run is retried by the app's own tick.
//!
//! The household application of `schedules.rs`: `chore_census` emits
//! `CensusTaken` each 15 minutes, which `ring_bell` fails to deliver
//! (`bell_missing`, one attempt within an hour) until a bell is hung; and
//! `bell_check` fails each hour the same way.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::Program;
use jadpo_runtime::SqlValue;
use jadpo_runtime::schedules::tick;
use jadpo_term::Environment;

use jadpo_test::host::Harness;

use crate::schedules::{BELL_CHECK, FILES, MINUTE, T0, jadpo, until_count, write};

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("deliveries-6.12-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES.iter().chain([&BELL_CHECK]) {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    root
}

/// `jadpo <args>` as the operator `sam` at `at` (milliseconds), against
/// `url` when one is given, as a terminal with colour off shows it, bold
/// taken out.
fn operating(root: &Path, url: Option<&str>, at: i64, args: &[&str]) -> Output {
    let mut args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    if let Some(url) = url {
        args.extend(["--url".to_string(), url.to_string()]);
    }
    let env = Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", &(at / 1000).to_string())
        .with_var("USER", "sam")
        .with_terminals(true, true, true)
        .with_var("TERM", "xterm-256color")
        .with_var("NO_COLOR", "1");
    let mut output = run(&args, &env.with_cwd(root));
    output.stdout = output.stdout.replace("\x1b[1m", "").replace("\x1b[0m", "");
    output.stderr = output.stderr.replace("\x1b[1m", "").replace("\x1b[0m", "");
    output
}

/// An id as the list shows it: its last 8 hex digits.
fn short(row: &[SqlValue]) -> String {
    match row.first() {
        Some(SqlValue::Uuid(id)) => jadpo_runtime::deliveries::short(id),
        other => panic!("not an id: {other:?}"),
    }
}

#[test]
fn mistakes_on_the_command_line_are_refused_before_any_database_is_read() {
    let root = project("arguments");
    let cases: &[(&[&str], &str)] = &[
        (
            &["deliveries", "frobnicate"],
            "`frobnicate` isn't something `jadpo deliveries` does",
        ),
        (
            &["deliveries", "retry"],
            "`jadpo deliveries retry` needs an id",
        ),
        (
            &["deliveries", "retry", "abc"],
            "`abc` is too short to be an id",
        ),
        (
            &["deliveries", "retry", "zz12ab34"],
            "`zz12ab34` isn't an id",
        ),
        (
            &["deliveries", "resolve", "4fa2c09e"],
            "`jadpo deliveries resolve` needs `--done` or `--not-done`",
        ),
        (
            &["deliveries", "resolve", "4fa2c09e", "--done", "--not-done"],
            "`jadpo deliveries resolve` takes `--done` or `--not-done`, not both",
        ),
        (
            &["deliveries", "retry", "4fa2c09e", "--done"],
            "`--done` is for `jadpo deliveries resolve`, not `retry`",
        ),
        (
            &["deliveries", "dismiss", "4fa2c09e", "--page", "2"],
            "`--page` is for listing, not `dismiss`",
        ),
        (&["deliveries", "--state", "stuck"], "`stuck` isn't a state"),
        (
            &["deliveries", "--state", "pendng"],
            "`pendng` isn't a state",
        ),
    ];
    for (args, headline) in cases {
        let out = operating(&root, None, T0, args);
        assert_eq!(out.code, 2, "{args:?}: {}{}", out.stdout, out.stderr);
        assert!(
            out.stderr.contains(&format!("✗ error  {headline}\n")),
            "{args:?}: {}",
            out.stderr
        );
        assert!(
            out.stderr.contains("rule      cli.deliveries_arguments"),
            "{args:?}: {}",
            out.stderr
        );
    }
    // The nearest state is offered, and `delivery` is the same command.
    let near = operating(&root, None, T0, &["deliveries", "--state", "pendng"]);
    assert!(
        near.stderr.contains("Use `--state pending`"),
        "{}",
        near.stderr
    );
    let synonym = operating(&root, None, T0, &["delivery", "frobnicate"]);
    assert!(
        synonym
            .stderr
            .contains("isn't something `jadpo deliveries` does"),
        "{}",
        synonym.stderr
    );
}

#[test]
fn deliveries_and_schedule_runs_are_listed_retried_dismissed_cancelled_and_resolved() {
    let root = project("operate");
    if !jadpo_test::live() {
        return;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url).at(T0).random_from(0);
    let at = |minutes: i64, seconds: i64| T0 + minutes * MINUTE + seconds * 1000;
    let runtime = Runtime::new().expect("runtime");
    let tick_at = |minutes: i64| {
        host.set_now(at(minutes, 0));
        runtime.block_on(tick(&program, &host, at(minutes, 0)))
    };
    let op = |at: i64, args: &[&str]| operating(&root, Some(&url), at, args);
    let role = match runtime
        .block_on(host.rows("SELECT session_user::text"))
        .first()
        .and_then(|row| row.first())
    {
        Some(SqlValue::Text(role)) => role.clone(),
        other => panic!("no role: {other:?}"),
    };

    // Nothing has run yet.
    let empty = op(at(0, 0), &["deliveries"]);
    assert_eq!(empty.code, 0, "{}{}", empty.stdout, empty.stderr);
    assert_eq!(empty.stdout, "✓ Nothing needs an operator\n");

    // Three censuses, each `CensusTaken` failing to reach `ring_bell`;
    // the bell check failing at 10:00 while the census is paused; and a
    // fourth census at 10:15.
    for minutes in [0, 15, 30, 45] {
        tick_at(minutes);
    }
    let paused = op(at(50, 0), &["schedules", "pause", "chore_census"]);
    assert_eq!(paused.code, 0, "{}{}", paused.stdout, paused.stderr);
    tick_at(60);
    let resumed = op(at(61, 0), &["schedules", "resume", "chore_census"]);
    assert_eq!(resumed.code, 0, "{}{}", resumed.stdout, resumed.stderr);
    tick_at(75);
    let paused = op(at(76, 0), &["schedules", "pause", "chore_census"]);
    assert_eq!(paused.code, 0, "{}{}", paused.stdout, paused.stderr);
    let deliveries = runtime.block_on(host.rows(
        "SELECT \"d\".\"id\" FROM \"jadpo_deliveries\" AS \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" ORDER BY \"o\".\"emitted_at\"",
    ));
    let [d1, d2, d3, d4] = deliveries.as_slice() else {
        panic!("four deliveries: {deliveries:?}");
    };
    let (d1, d2, d3, d4) = (short(d1), short(d2), short(d3), short(d4));
    let x1 = short(
        runtime
            .block_on(host.rows("SELECT \"id\" FROM \"jadpo_schedule_runs\" WHERE \"schedule\" = 'schedule bell_check'"))
            .first()
            .expect("the bell check's run"),
    );
    // The first's outcome became unknown, as an outside call that may
    // have happened leaves one (chunk 7.0c).
    runtime.block_on(host.rows(&format!("UPDATE \"jadpo_deliveries\" SET \"status\" = 'outcome_unknown' WHERE replace(\"id\"::text, '-', '') LIKE '%{d1}' RETURNING 1")));

    let listed = op(at(75, 20), &["deliveries"]);
    assert_eq!(listed.code, 0, "{}{}", listed.stdout, listed.stderr);
    assert_eq!(
        listed.stdout,
        format!(
            "  id        item                     state              last tried (UTC)
  {d4}  CensusTaken → ring_bell  ✗ failed           10:15:00 · bell_missing
  {x1}  schedule bell_check      ✗ failed           10:00:00 · bell_missing
  {d3}  CensusTaken → ring_bell  ✗ failed           09:45:00 · bell_missing
  {d2}  CensusTaken → ring_bell  ✗ failed           09:30:00 · bell_missing
  {d1}  CensusTaken → ring_bell  ◆ outcome unknown  09:15:00 · bell_missing

✗ 4 failed  1 outcome unknown

→ jadpo deliveries retry {d4}  sends it again, once the cause is fixed
"
        )
    );
    let json = op(at(75, 20), &["deliveries", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    assert_eq!(document["data"]["total"], 5);
    assert_eq!(document["data"]["counts"]["failed"], 4);
    assert_eq!(document["data"]["items"][0]["kind"], "delivery");
    assert_eq!(
        document["data"]["items"][0]["subscriber"],
        "subscriber ring_bell"
    );
    assert_eq!(document["data"]["items"][1]["schedule"], "bell_check");

    // Refused: each action only from the state it applies to.
    let refusals: &[(Vec<&str>, String, &str)] = &[
        (
            vec!["deliveries", "resolve", &d2, "--done"],
            format!("`{d2}` isn't in doubt, so there is nothing to resolve"),
            "deliveries.wrong_state",
        ),
        (
            vec!["deliveries", "retry", &d1],
            format!("`{d1}` hasn't failed, so there is nothing to retry"),
            "deliveries.wrong_state",
        ),
        (
            vec!["deliveries", "cancel", &d2],
            format!("`{d2}` isn't pending, so there is nothing to cancel"),
            "deliveries.wrong_state",
        ),
        (
            vec!["deliveries", "retry", "0000abcd"],
            "No delivery or schedule run has the id `0000abcd`".to_string(),
            "deliveries.not_found",
        ),
    ];
    for (args, headline, rule) in refusals {
        let out = op(at(75, 30), args);
        assert_eq!(out.code, 1, "{args:?}: {}{}", out.stdout, out.stderr);
        assert!(
            out.stderr.contains(&format!("✗ error  {headline}\n")),
            "{args:?}: {}",
            out.stderr
        );
        assert!(
            out.stderr.contains(&format!("rule      {rule}")),
            "{args:?}: {}",
            out.stderr
        );
    }
    let blind = op(at(75, 30), &["deliveries", "retry", &d1]);
    assert!(
        blind.stderr.contains("could repeat the outside call"),
        "{}",
        blind.stderr
    );
    assert!(
        blind
            .stderr
            .contains(&format!("`jadpo deliveries resolve {d1} --done`")),
        "{}",
        blind.stderr
    );

    // Resolved as done: it succeeded, and isn't retried.
    let resolved = op(at(76, 0), &["deliveries", "resolve", &d1, "--done"]);
    assert_eq!(resolved.code, 0, "{}{}", resolved.stdout, resolved.stderr);
    assert_eq!(
        resolved.stdout,
        format!(
            "✓ Resolved `{d1}` as done: `ring_bell` took effect, so it isn't retried
    logged  sam · role {role}
"
        )
    );

    // Dismissed: set aside, and listed with who did it.
    let dismissed = op(at(77, 0), &["deliveries", "dismiss", &d4]);
    assert_eq!(
        dismissed.code, 0,
        "{}{}",
        dismissed.stdout, dismissed.stderr
    );
    assert_eq!(
        dismissed.stdout,
        format!(
            "✓ Dismissed `{d4}`: `ring_bell` won't receive `CensusTaken`
    logged  sam · role {role}
"
        )
    );
    let again = op(at(77, 10), &["deliveries", "dismiss", &d4]);
    assert_eq!(again.code, 1);
    assert!(
        again.stderr.contains(&format!(
            "✗ error  `{d4}` hasn't failed, so there is nothing to dismiss"
        )),
        "{}",
        again.stderr
    );
    let listed = op(at(77, 20), &["deliveries", "--state", "dismissed"]);
    assert_eq!(
        listed.stdout,
        format!(
            "  id        item                     state        dismissed (UTC)
  {d4}  CensusTaken → ring_bell  – dismissed  10:17:00 · sam

✓ 1 dismissed
"
        )
    );

    // Retried, then cancelled before the tick delivered it.
    let retried = op(at(78, 0), &["deliveries", "retry", &d3]);
    assert_eq!(retried.code, 0, "{}{}", retried.stdout, retried.stderr);
    let cancelled = op(at(79, 0), &["deliveries", "cancel", &d3]);
    assert_eq!(
        cancelled.code, 0,
        "{}{}",
        cancelled.stdout, cancelled.stderr
    );
    assert_eq!(
        cancelled.stdout,
        format!(
            "✓ Cancelled `{d3}`: `ring_bell` won't receive `CensusTaken`
    logged  sam · role {role}
"
        )
    );
    let listed = op(at(79, 10), &["deliveries", "--state", "cancelled"]);
    assert_eq!(
        listed.stdout,
        format!(
            "  id        item                     state        cancelled (UTC)
  {d3}  CensusTaken → ring_bell  – cancelled  10:19:00 · sam

✓ 1 cancelled
"
        )
    );

    // Retried after its hour from the event had passed: a fresh budget,
    // so once a bell is hung the tick's sweep delivers it.
    let retried = op(at(100, 0), &["deliveries", "retry", &d2]);
    assert_eq!(retried.code, 0, "{}{}", retried.stdout, retried.stderr);
    assert_eq!(
        retried.stdout,
        format!(
            "✓ Queued `{d2}` for retry: the app's next tick sends `CensusTaken` to `ring_bell` again, with a fresh retry budget
    logged  sam · role {role}
"
        )
    );
    let pending = op(at(100, 10), &["deliveries", "--state", "pending"]);
    assert_eq!(
        pending.stdout,
        format!(
            "  id        item                     state      next attempt (UTC)
  {d2}  CensusTaken → ring_bell  ○ pending  10:40:00 · after bell_missing

✓ 1 pending

→ jadpo deliveries cancel {d2}  stops it running
"
        )
    );
    runtime.block_on(async {
        assert_eq!(
            host.count(&format!("SELECT \"attempts\"::int8 FROM \"jadpo_deliveries\" WHERE replace(\"id\"::text, '-', '') LIKE '%{d2}'")).await,
            0
        );
        host.rows("INSERT INTO \"bell\" (\"id\") VALUES ('00000000-0000-7000-8000-0000000000be') RETURNING 1")
            .await;
    });
    tick_at(101);
    runtime.block_on(async {
        assert_eq!(
            host.rows(&format!("SELECT \"status\" FROM \"jadpo_deliveries\" WHERE replace(\"id\"::text, '-', '') LIKE '%{d2}'")).await,
            [vec![SqlValue::Text("succeeded".into())]]
        );
    });

    // A schedule's failed run, retried: the app's own tick runs it, by
    // hand, and the command waits for it as `jadpo schedules run` does.
    let requested = "SELECT count(*) FROM \"jadpo_schedules\" WHERE \"run_requested\" IS NOT NULL";
    let waiting = {
        let (root, url, x1) = (root.clone(), url.clone(), x1.clone());
        std::thread::spawn(move || {
            operating(&root, Some(&url), at(102, 0), &["deliveries", "retry", &x1])
        })
    };
    until_count(&runtime, &host, requested, 1);
    tick_at(103);
    let ran = waiting.join().expect("the command");
    assert_eq!(ran.code, 0, "{}{}", ran.stdout, ran.stderr);
    assert_eq!(
        ran.stdout,
        "✓ The app's tick ran `bell_check` in 0 ms, emitting 0 events\n"
    );
    let empty = op(at(103, 10), &["deliveries"]);
    assert_eq!(empty.stdout, "✓ Nothing needs an operator\n");

    // A schedule's latest run that failed, dismissed: its log row.
    runtime.block_on(host.rows("INSERT INTO \"jadpo_schedule_runs\" (\"id\", \"schedule\", \"due_slot\", \"started_at\", \"ended_at\", \"outcome\", \"failure\") VALUES ('00000000-0000-7000-8000-0000000000c1', 'schedule bell_check', '2026-10-19T10:50:00Z', '2026-10-19T10:50:00Z', '2026-10-19T10:50:00Z', 'failed', '{\"code\": \"bell_missing\", \"kind\": \"unavailable\"}') RETURNING 1"));
    let dismissed = op(at(111, 0), &["deliveries", "dismiss", "000000c1"]);
    assert_eq!(
        dismissed.code, 0,
        "{}{}",
        dismissed.stdout, dismissed.stderr
    );
    assert_eq!(
        dismissed.stdout,
        format!(
            "✓ Dismissed `000000c1`: `bell_check`'s failed run from 10:50:00
    logged  sam · role {role}
"
        )
    );
    let empty = op(at(111, 10), &["deliveries"]);
    assert_eq!(empty.stdout, "✓ Nothing needs an operator\n");

    // A run asked for by hand, waiting for a tick, is pending until it is
    // cancelled, which withdraws it.
    let declared = jadpo_runtime::schedules::watch::Declared {
        key: "schedule chore_census".into(),
        name: "chore_census".into(),
        every_ms: 15 * MINUTE as u64,
    };
    let run = match runtime
        .block_on(jadpo_runtime::schedules::watch::request_run(
            host.store(),
            &host,
            &declared,
            at(112, 0),
        ))
        .expect("a request")
    {
        jadpo_runtime::schedules::watch::Requested::Waiting { run, .. } => {
            jadpo_runtime::deliveries::short(&run)
        }
        other => panic!("not a request: {other:?}"),
    };
    let pending = op(at(112, 10), &["deliveries", "--state", "pending"]);
    assert_eq!(
        pending.stdout,
        format!(
            "  id        item                   state      next attempt (UTC)
  {run}  schedule chore_census  ○ pending  next tick · asked by hand

✓ 1 pending

→ jadpo deliveries cancel {run}  stops it running
"
        )
    );
    let cancelled = op(at(113, 0), &["deliveries", "cancel", &run]);
    assert_eq!(
        cancelled.code, 0,
        "{}{}",
        cancelled.stdout, cancelled.stderr
    );
    assert_eq!(
        cancelled.stdout,
        format!(
            "✓ Cancelled `{run}`: the run asked of `chore_census` won't happen
    logged  sam · role {role}
"
        )
    );
    assert_eq!(runtime.block_on(host.count(requested)), 0);

    // Every action is logged once, by whom, as which role, from which
    // state, in order.
    let logged = runtime.block_on(host.rows(
        "SELECT \"action\", \"kind\", \"was\", \"operator\", \"role\", to_char(\"acted_at\" AT TIME ZONE 'UTC', 'HH24:MI') FROM \"jadpo_operator_actions\" ORDER BY \"acted_at\"",
    ));
    let text = |value: &SqlValue| match value {
        SqlValue::Text(text) => text.clone(),
        other => panic!("not text: {other:?}"),
    };
    let logged: Vec<Vec<String>> = logged
        .iter()
        .map(|row| row.iter().map(text).collect())
        .collect();
    let row = |action: &str, kind: &str, was: &str, time: &str| {
        vec![
            action.to_string(),
            kind.to_string(),
            was.to_string(),
            "sam".to_string(),
            role.clone(),
            time.to_string(),
        ]
    };
    assert_eq!(
        logged,
        [
            row("resolve_done", "delivery", "outcome_unknown", "10:16"),
            row("dismiss", "delivery", "failed", "10:17"),
            row("retry", "delivery", "failed", "10:18"),
            row("cancel", "delivery", "pending", "10:19"),
            row("retry", "delivery", "failed", "10:40"),
            row("retry", "schedule_run", "failed", "10:42"),
            row("dismiss", "schedule_run", "failed", "10:51"),
            row("cancel", "schedule_run", "pending", "10:53"),
        ]
    );

    drop(host);
    databases.release(copy, false);
}
