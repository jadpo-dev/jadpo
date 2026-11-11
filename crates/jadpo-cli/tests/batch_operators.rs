//! `jadpo batches` against live PostgreSQL (area 14, "`jadpo batches`";
//! area 12, "What an operator sees"): the batch runs kept are listed,
//! running first, with their state, progress and problems; `show` gives
//! one run's progress, when it should finish, why it failed and its
//! problems, never a value; `cancel` stops a running one after the batch
//! in hand, logged with who asked, and leaves one that ended as it is; and
//! `jadpo deliveries` lists a failed run until it is dismissed.
//!
//! The stock check of `batches.rs`: seven products, one with none in
//! stock, so a check of them all has one problem.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_host_native::{Postgres, Runtime};
use jadpo_interp::{Program, on_large_stack};
use jadpo_runtime::runner::{Deferred, run_deferred};
use jadpo_runtime::{Executor, Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

const MINUTE: i64 = 60_000;
/// 2026-10-19 09:00:00 UTC.
const T0: i64 = 1_792_400_400_000;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("batch-operators-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in crate::batches::FILES {
        write(&root, path, text);
    }
    let migrated = operating(&root, None, T0, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

/// `jadpo <args>` as the operator `sam` at `at`, against `url` when one is
/// given, as a terminal with colour off shows it, bold taken out.
fn operating(root: &Path, url: Option<&str>, at: i64, args: &[&str]) -> Output {
    let mut args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    if let Some(url) = url {
        args.extend(["--url".to_string(), url.to_string()]);
    }
    let mut env = Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", &(at / 1000).to_string())
        .with_var("USER", "sam")
        .with_terminals(true, true, true)
        .with_var("TERM", "xterm-256color")
        .with_var("NO_COLOR", "1");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    let mut output = run(&args, &env.with_cwd(root));
    output.stdout = output.stdout.replace("\x1b[1m", "").replace("\x1b[0m", "");
    output.stderr = output.stderr.replace("\x1b[1m", "").replace("\x1b[0m", "");
    output
}

fn page(size: i64) -> Value {
    Value::record(
        "CheckPage",
        vec![
            (Rc::from("page_size"), Value::Int(size)),
            (Rc::from("after"), Value::None),
        ],
    )
}

/// Runs `action` with `args` as no one signed in: its value and the work
/// it left for after its response.
async fn call(
    program: &Program,
    host: &Harness<Postgres>,
    action: &str,
    args: Vec<Value>,
) -> (Value, Vec<Deferred>) {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = host.ctx(program);
    let outcome = program.run(&mut ctx, id, args).await;
    let work = ctx.take_deferred();
    match outcome {
        Outcome::Value(value) => (value, work),
        other => panic!("{action}: {other:?}"),
    }
}

/// Each run's id as the list shows it, oldest first.
async fn run_ids(host: &Harness<Postgres>) -> Vec<String> {
    host.rows("SELECT \"id\" FROM \"jadpo_batch_runs\" ORDER BY \"started_at\"")
        .await
        .iter()
        .map(|row| match row.first() {
            Some(SqlValue::Uuid(id)) => jadpo_runtime::deliveries::short(id),
            other => panic!("not an id: {other:?}"),
        })
        .collect()
}

#[test]
fn mistakes_on_the_command_line_are_refused_before_any_database_is_read() {
    let root = project("arguments");
    let cases: &[(&[&str], &str)] = &[
        (
            &["batches", "frobnicate"],
            "`frobnicate` isn't something `jadpo batches` does",
        ),
        (&["batches", "show"], "`jadpo batches show` needs an id"),
        (
            &["batches", "cancel", "abc"],
            "`abc` is too short to be an id",
        ),
        (&["batches", "show", "zz12ab34"], "`zz12ab34` isn't an id"),
        (
            &["batches", "show", "4fa2c09e", "9b1e77d2"],
            "`jadpo batches` takes 2 arguments",
        ),
        (
            &["batches", "list", "4fa2c09e"],
            "`jadpo batches` lists them all, not `4fa2c09e`",
        ),
        (&["batches", "--state", "stuck"], "`stuck` isn't a state"),
        (&["batches", "--state", "runing"], "`runing` isn't a state"),
        (&["batches", "--page", "0"], "`0` isn't a page"),
        (
            &["batches", "show", "4fa2c09e", "--state", "failed"],
            "`--state` is for listing, not `show`",
        ),
        (
            &["batches", "cancel", "4fa2c09e", "--page", "2"],
            "`--page` is for listing and `show`, not `cancel`",
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
            out.stderr.contains("rule      cli.batches_arguments"),
            "{args:?}: {}",
            out.stderr
        );
    }
    // The nearest action and state are offered, and `batch` is the same
    // command.
    let near = operating(&root, None, T0, &["batches", "--state", "runing"]);
    assert!(
        near.stderr.contains("Use `--state running`"),
        "{}",
        near.stderr
    );
    let action = operating(&root, None, T0, &["batch", "shwo", "4fa2c09e"]);
    assert!(
        action.stderr.contains("Did you mean `show`?"),
        "{}",
        action.stderr
    );
}

/// A database that can't be reached is `batches.unavailable`.
#[test]
fn a_database_that_cant_be_reached_is_unavailable() {
    let root = project("unavailable");
    let out = operating(
        &root,
        Some("postgresql://localhost:1/stock"),
        T0,
        &["batches"],
    );
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(
        out.stderr
            .contains("✗ error  The database `main` can't be reached"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("rule      batches.unavailable"),
        "{}",
        out.stderr
    );
}

#[test]
fn batch_runs_are_listed_shown_and_cancelled() {
    on_large_stack(batch_runs_are_listed_shown_and_cancelled_on_its_stack).expect("a thread");
}

fn batch_runs_are_listed_shown_and_cancelled_on_its_stack() {
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
    let rt = Runtime::new().expect("runtime");
    let op = |at: i64, args: &[&str]| operating(&root, Some(&url), at, args);
    let role = match rt
        .block_on(host.rows("SELECT session_user::text"))
        .first()
        .and_then(|row| row.first())
    {
        Some(SqlValue::Text(role)) => role.clone(),
        other => panic!("no role: {other:?}"),
    };

    // Nothing has run yet.
    let empty = op(T0, &["batches"]);
    assert_eq!(empty.code, 0, "{}{}", empty.stdout, empty.stderr);
    assert_eq!(empty.stdout, "✓ No batch run is kept\n");

    // Seven products, the fourth with none in stock.
    for (n, code) in ["p1", "p2", "p3", "p4", "p5", "p6", "p7"]
        .iter()
        .enumerate()
    {
        let stock = if n == 3 { 0 } else { 5 };
        rt.block_on(call(
            &program,
            &host,
            "add_product",
            vec![Value::text(code), Value::Int(stock)],
        ));
    }
    // A check of them all, run to its end at 09:00: one problem.
    let (_, work) = rt.block_on(call(&program, &host, "quick_check", vec![page(3)]));
    rt.block_on(run_deferred(&program, &host, work));
    // A second, started at 09:10, its runner yet to go on: three of about
    // nine done by 09:29.
    host.set_now(T0 + 10 * MINUTE);
    let (_, going) = rt.block_on(call(&program, &host, "quick_check", vec![page(3)]));
    // A third, started at 09:20, that failed at 09:25.
    host.set_now(T0 + 20 * MINUTE);
    let (_, _dropped) = rt.block_on(call(&program, &host, "quick_check", vec![page(3)]));
    let ids = rt.block_on(run_ids(&host));
    let [a, b, c] = ids.as_slice() else {
        panic!("three runs: {ids:?}");
    };
    rt.block_on(host.rows(&format!(
        "UPDATE \"jadpo_batch_runs\" SET \"items_done\" = 3, \"estimated_total\" = 9, \"stepped_at\" = to_timestamp({}) WHERE replace(\"id\"::text, '-', '') LIKE '%{b}' RETURNING 1",
        (T0 + 29 * MINUTE) / 1000
    )));
    rt.block_on(host.rows(&format!(
        "UPDATE \"jadpo_batch_runs\" SET \"state\" = 'failed', \"failure\" = 'its saved values were written in encoding 2, which this release doesn''t read', \"items_done\" = 2, \"estimated_total\" = 7, \"stepped_at\" = to_timestamp({0}), \"finished_at\" = to_timestamp({0}) WHERE replace(\"id\"::text, '-', '') LIKE '%{c}' RETURNING 1",
        (T0 + 25 * MINUTE) / 1000
    )));
    let at = T0 + 30 * MINUTE;

    // Those running first, then those that ended, newest first.
    let listed = op(at, &["batches"]);
    assert_eq!(listed.code, 0, "{}{}", listed.stdout, listed.stderr);
    assert_eq!(
        listed.stdout,
        format!(
            "  id        batch                      state       progress      problems
  {b}  quick_check  batches.each  ● running   3 of about 9         0
  {c}  quick_check  batches.each  ✗ failed    2 of about 7         0
  {a}  quick_check  batches.each  ✓ finished  7 of 7               1

✗ 1 failed  1 running · 1 finished

→ jadpo batches show {c}  says why it failed
"
        )
    );
    let running = op(at, &["batches", "--state", "running"]);
    assert!(running.stdout.contains(b.as_str()), "{}", running.stdout);
    assert!(!running.stdout.contains(a.as_str()), "{}", running.stdout);
    assert!(
        running.stdout.contains("● 1 running\n"),
        "{}",
        running.stdout
    );
    let none = op(at, &["batches", "--state", "cancelled"]);
    assert_eq!(none.stdout, "✓ Nothing was cancelled\n");
    let json = op(at, &["batches", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    assert_eq!(document["data"]["total"], 3);
    assert_eq!(document["data"]["counts"]["running"], 1);
    assert_eq!(document["data"]["runs"][0]["id"], b.as_str());
    assert_eq!(document["data"]["runs"][0]["call"], "quick_check");
    assert_eq!(document["data"]["runs"][0]["helper"], "batches.each");
    assert_eq!(document["data"]["runs"][0]["itemsDone"], 3);
    assert_eq!(document["data"]["runs"][0]["estimatedTotal"], 9);
    assert_eq!(
        document["data"]["runs"][0]["estimatedFinishAt"],
        "2026-10-19T10:07:00Z"
    );

    // A running one: how far, and when it should finish at the rate so
    // far, 3 items in 19 minutes leaving 6.
    let shown = op(at, &["batches", "show", b]);
    assert_eq!(shown.code, 0, "{}{}", shown.stdout, shown.stderr);
    assert_eq!(
        shown.stdout,
        format!(
            "  batch     quick_check · batches.each
  progress  3 of about 9 · 33%
  started   09:10:00
  updated   09:29:00
  finishes  about 10:07:00

● running

→ jadpo batches cancel {b}  stops it after the batch in hand
"
        )
    );
    // A finished one: its problem by row and rule, never a value.
    let finished = op(at, &["batches", "show", a]);
    assert_eq!(
        finished.stdout,
        "  batch     quick_check · batches.each
  progress  7 of 7
  started   09:00:00
  ended     09:00:00 · took 0 ms

  row  column  rule
    4  –       out_of_stock

✓ finished  1 problem
"
    );
    // A failed one: why.
    let failed = op(at, &["batches", "show", c]);
    assert!(
        failed
            .stdout
            .contains("  ended     09:25:00 · took 5 min\n"),
        "{}",
        failed.stdout
    );
    assert!(
        failed
            .stdout
            .contains("reason    its saved values were written in encoding 2"),
        "{}",
        failed.stdout
    );
    assert!(failed.stdout.ends_with("✗ failed\n"), "{}", failed.stdout);
    let unknown = op(at, &["batches", "show", "0000abcd"]);
    assert_eq!(unknown.code, 2, "{}{}", unknown.stdout, unknown.stderr);
    assert!(
        unknown
            .stderr
            .contains("✗ error  No batch run has the id `0000abcd`"),
        "{}",
        unknown.stderr
    );

    // `jadpo deliveries` lists the failed run, and shows it as `jadpo
    // batches show` does.
    let deliveries = op(at, &["deliveries"]);
    assert_eq!(
        deliveries.stdout,
        format!(
            "  id        item               state     last tried (UTC)
  {c}  batch quick_check  ✗ failed  09:25:00 · 2 of about 7

✗ 1 failed

→ jadpo batches show {c}  says why it failed
"
        )
    );
    let through = op(at, &["deliveries", "show", c]);
    assert_eq!(through.stdout, failed.stdout);
    let retried = op(at, &["deliveries", "retry", c]);
    assert_eq!(retried.code, 1, "{}{}", retried.stdout, retried.stderr);
    assert!(
        retried.stderr.contains(&format!(
            "`{c}` is a batch run, which is only cancelled or dismissed"
        )),
        "{}",
        retried.stderr
    );
    assert!(
        retried.stderr.contains("rule      deliveries.wrong_state"),
        "{}",
        retried.stderr
    );
    let dismissed = op(at, &["deliveries", "dismiss", c]);
    assert_eq!(
        dismissed.code, 0,
        "{}{}",
        dismissed.stdout, dismissed.stderr
    );
    assert_eq!(
        dismissed.stdout,
        format!(
            "✓ Dismissed `{c}`: the failed run of `quick_check`
    logged  sam · role {role}
"
        )
    );
    let calm = op(at, &["deliveries"]);
    assert_eq!(calm.stdout, "✓ Nothing needs an operator  1 running\n");
    let set_aside = op(at, &["deliveries", "--state", "dismissed"]);
    assert!(
        set_aside.stdout.contains(&format!(
            "{c}  batch quick_check  – dismissed  09:30:00 · sam"
        )),
        "{}",
        set_aside.stdout
    );

    // Cancelled: asked of the run, logged, and the runner stops it at its
    // next batch.
    let cancelled = op(at, &["batches", "cancel", b]);
    assert_eq!(
        cancelled.code, 0,
        "{}{}",
        cancelled.stdout, cancelled.stderr
    );
    assert_eq!(
        cancelled.stdout,
        format!(
            "✓ Asked `{b}` to stop: `quick_check` stops after the batch in hand
    logged  sam · role {role}

→ jadpo batches show {b}  shows how far it got
"
        )
    );
    let logged = rt.block_on(host.rows(
        "SELECT \"action\", \"kind\", \"was\", \"operator\" FROM \"jadpo_operator_actions\" WHERE \"kind\" = 'batch_run' ORDER BY \"action\"",
    ));
    assert_eq!(
        logged,
        [
            vec![
                SqlValue::Text("cancel".into()),
                SqlValue::Text("batch_run".into()),
                SqlValue::Text("running".into()),
                SqlValue::Text("sam".into()),
            ],
            vec![
                SqlValue::Text("dismiss".into()),
                SqlValue::Text("batch_run".into()),
                SqlValue::Text("failed".into()),
                SqlValue::Text("sam".into()),
            ],
        ]
    );
    let stopping = op(at, &["batches", "show", b]);
    assert!(
        stopping
            .stdout
            .contains("  cancel    asked at 09:30:00 by sam; it stops after the batch in hand\n"),
        "{}",
        stopping.stdout
    );
    let again = op(at, &["batches", "cancel", b]);
    assert_eq!(again.code, 0, "{}{}", again.stdout, again.stderr);
    assert_eq!(
        again.stdout,
        format!("✓ `{b}` is already stopping: a cancel was asked at 09:30:00\n")
    );
    host.set_now(at + MINUTE);
    rt.block_on(run_deferred(&program, &host, going));
    let ended = op(at + MINUTE, &["batches", "show", b]);
    assert!(ended.stdout.ends_with("✓ cancelled\n"), "{}", ended.stdout);
    let over = op(at + MINUTE, &["batches", "cancel", b]);
    assert_eq!(over.stdout, format!("✓ `{b}` was already cancelled\n"));
    let done = op(at + MINUTE, &["batches", "cancel", a]);
    assert_eq!(
        done.stdout,
        format!("✓ `{a}` had already finished, so there is nothing to cancel\n")
    );
}
