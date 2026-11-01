//! Feature flags against live PostgreSQL (area 16, "Feature flags";
//! chunk B9): a shop whose checkout reads two flags, a `Bool` in an action
//! and an enum in a route's `run:`. Through `jadpo test`, a test reads each
//! flag's `default:` and the values its `flags:` gives. Then, through the
//! runtime's entry point with a caller, each value comes from the flags'
//! table, `jadpo_flags`: a named user's fixed value before any share, a
//! stable share of the other users chosen by the runtime's hash, and
//! everyone's value for the rest; a change read again only once the state
//! is due, fixed for an operation already under way, and the last values
//! kept when the table can't be read.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::Program;
use jadpo_runtime::flags::{REFRESH_MS, bucket, refresh};
use jadpo_runtime::{Caller, Database, Executor, Outcome, SqlValue};
use jadpo_std::http::PrincipalKind;
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use shop/entities { Shopper }

application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}

principal {
    user: {
        subject: Shopper.handle
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    api_token: {}
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    database_url: PostgresUrl
}
"#,
    ),
    (
        "domains/shop/actions/join.jadpo",
        r#"use shop/entities { Shopper }
use shop/failures { HandleTaken }
use shop/types { Handle }

action join(handle: Handle) fails HandleTaken -> Shopper {
    return attempt Shopper.create {
        set: {
            handle: handle
        }
        conflict: {
            Shopper.handle: HandleTaken
        }
    }
}
"#,
    ),
    (
        "domains/shop/actions/start_checkout.jadpo",
        r#"use shop/types { CheckoutLayout, CheckoutView }

action start_checkout(layout: CheckoutLayout) -> CheckoutView {
    if (flags.new_checkout) {
        return CheckoutView {
            steps: 1
            layout: layout
        }
    }
    return CheckoutView {
        steps: 3
        layout: layout
    }
}
"#,
    ),
    (
        "domains/shop/entities/shopper.jadpo",
        r#"use shop/types { Handle }

entity Shopper {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }
}
"#,
    ),
    (
        "domains/shop/failures/handle_taken.jadpo",
        r#"/// Another shopper already has this handle.
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "Someone already has that handle."
}
"#,
    ),
    (
        "domains/shop/routes/checkouts.jadpo",
        r#"use shop/types { CheckoutView }
use shop/actions { start_checkout }

/// The checkout.
routes Checkouts {
    prefix: /checkouts

    /// Starts a checkout in the layout this shopper is given.
    POST / {
        output: CheckoutView
        status: SuccessStatus.created
        run: start_checkout(flags.checkout_layout)
    }
}
"#,
    ),
    (
        "domains/shop/routes/checkouts.test.jadpo",
        r#"use shop/actions { join }
use shop/entities { Shopper }
use shop/types { CheckoutLayout }

test "a shopper gets the classic three-step checkout by default" {
    var ann: Shopper = attempt join("ann")
    var response: Response = request {
        method: HttpMethod.post
        path: "/checkouts"
        as: Principal.user(ann)
    }
    assert response.status == 201
    var classic: Bool = match response.output {
        some(view) -> view.steps == 3 and view.layout == CheckoutLayout.classic
        none -> false
    }
    assert classic
}

test "a shopper gets the layout and the one step a test's flags give" {
    flags: {
        new_checkout: true
        checkout_layout: CheckoutLayout.compact
    }
    var ann: Shopper = attempt join("ann")
    var response: Response = request {
        method: HttpMethod.post
        path: "/checkouts"
        as: Principal.user(ann)
    }
    var compact: Bool = match response.output {
        some(view) -> view.steps == 1 and view.layout == CheckoutLayout.compact
        none -> false
    }
    assert compact
}
"#,
    ),
    (
        "domains/shop/types/checkout_layout.jadpo",
        r#"type CheckoutLayout = Enum {
    classic
    compact
    wide
}
"#,
    ),
    (
        "domains/shop/types/checkout_view.jadpo",
        r#"use shop/types { CheckoutLayout, Steps }

type CheckoutView = Output {
    steps: Steps
    layout: CheckoutLayout
}
"#,
    ),
    (
        "domains/shop/types/handle.jadpo",
        r#"type Handle = Text {
    min_length: 3
    max_length: 40
}
"#,
    ),
    (
        "domains/shop/types/steps.jadpo",
        r#"type Steps = Int {
    min: 1
}
"#,
    ),
    (
        "flags.jadpo",
        r#"use shop/types { CheckoutLayout }

flags {
    /// The one-page checkout replacing the three-step form.
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
    }

    /// Which layout the checkout uses while we compare them.
    checkout_layout: CheckoutLayout {
        default: CheckoutLayout.classic
        owner: "payments"
        expires: "2099-12-15"
    }
}
"#,
    ),
    (
        "policy.jadpo",
        r#"use shop

policy {
    actions: {
        shop.start_checkout: [Access.authenticated]
    }
}
"#,
    ),
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("flags-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    root
}

#[test]
fn a_test_reads_each_flags_default_and_the_values_its_flags_give() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("tested");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("2 passed"), "{}", tested.stdout);
}

/// Runs `action` with `args` as the user `person`, signed in.
async fn call(
    program: &Program,
    host: &Harness,
    action: &str,
    args: Vec<Value>,
    person: Option<&Value>,
) -> Outcome {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = host.ctx(program);
    ctx.set_caller(person.map(signed_in));
    program.run(&mut ctx, id, args).await
}

/// The caller a shopper signs in as.
fn signed_in(person: &Value) -> Caller {
    Caller {
        value: jadpo_runtime::http::identity::principal(
            PrincipalKind::User,
            field(person, "id"),
            field(person, "handle"),
        ),
        key: "user".to_string(),
        session: None,
        sign_in: None,
        multi_factor: false,
    }
}

fn value(outcome: Outcome) -> Value {
    match outcome {
        Outcome::Value(value) => value,
        other => panic!("expected a value, got {other:?}"),
    }
}

fn field(record: &Value, name: &str) -> Value {
    match record {
        Value::Record(record) => record.get(name).cloned().expect("the field"),
        other => panic!("not a record: {other:?}"),
    }
}

fn id(person: &Value) -> String {
    match field(person, "id") {
        Value::Uuid(id) => id.to_string(),
        other => panic!("not an id: {other:?}"),
    }
}

/// How many steps the checkout `person` starts has.
async fn steps(program: &Program, host: &Harness, person: &Value) -> i64 {
    let layout = Value::variant("CheckoutLayout", 0, "classic", Vec::new());
    let view = value(call(program, host, "start_checkout", vec![layout], Some(person)).await);
    match field(&view, "steps") {
        Value::Int(steps) => steps,
        other => panic!("not a number: {other:?}"),
    }
}

/// Keeps `flag`'s state in `jadpo_flags`, replacing what was there.
async fn set(
    host: &Harness,
    flag: &str,
    everyone: Option<&str>,
    users: Option<String>,
    shares: Option<&str>,
) {
    let text = |text: Option<&str>| match text {
        Some(text) => SqlValue::Text(text.to_string()),
        None => SqlValue::Null(jadpo_std::SqlType::Text),
    };
    let json = |text: Option<&str>| match text {
        Some(text) => SqlValue::Json(text.to_string()),
        None => SqlValue::Null(jadpo_std::SqlType::Json),
    };
    host.store()
        .query_once(
            "INSERT INTO \"jadpo_flags\" (\"flag\", \"value\", \"users\", \"shares\", \"set_at\") VALUES ($1, $2, $3, $4, now()) ON CONFLICT (\"flag\") DO UPDATE SET \"value\" = $2, \"users\" = $3, \"shares\" = $4, \"set_at\" = now()",
            &[SqlValue::Text(flag.to_string()), text(everyone), json(users.as_deref()), json(shares)],
        )
        .await
        .expect("the flag's state is kept");
}

#[test]
fn each_value_comes_from_the_table_for_its_user_and_is_read_again_when_due() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("table");
    let loaded = jadpo_db::load(&root);
    let steps_of = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps_of, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url);
    let flags = program.plan().runtime().flags.clone();
    let (layout_at, layout) = flags.flag("checkout_layout").expect("checkout_layout");
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut shoppers = Vec::new();
        for n in 0..120 {
            let handle = Value::text(&format!("shopper{n:03}"));
            shoppers.push(value(
                call(&program, &host, "join", vec![handle], None).await,
            ));
        }
        let ann = shoppers[0].clone();
        // Before any state is read, every flag gives its `default:`.
        assert_eq!(steps(&program, &host, &ann).await, 3);
        // Ann is named; half the other shoppers are in a share.
        set(
            &host,
            "new_checkout",
            Some("false"),
            Some(format!(r#"{{"{}": "true"}}"#, id(&ann))),
            Some(r#"[["true", 50]]"#),
        )
        .await;
        set(
            &host,
            "checkout_layout",
            Some("wide"),
            None,
            Some(r#"[["compact", 30]]"#),
        )
        .await;
        let start = 1_792_504_800_000;
        refresh(&program, &host, start).await;
        assert_eq!(steps(&program, &host, &ann).await, 1, "the named user");
        let mut in_share = 0;
        for shopper in &shoppers[1..] {
            let expected = if bucket("new_checkout", &id(shopper)) < 50 {
                1
            } else {
                3
            };
            if expected == 1 {
                in_share += 1;
            }
            assert_eq!(
                steps(&program, &host, shopper).await,
                expected,
                "{}",
                id(shopper)
            );
        }
        assert!(
            (35..85).contains(&in_share),
            "{in_share} of 119 in a 50% share"
        );
        // The enum's split, read as a route's `run:` reads it.
        for shopper in &shoppers {
            let mut ctx = host.ctx(&program);
            ctx.set_caller(Some(signed_in(shopper)));
            let expected = if bucket("checkout_layout", &id(shopper)) < 30 {
                1
            } else {
                2
            };
            assert_eq!(
                ctx.flag(layout_at),
                Some(jadpo_runtime::flags::value(layout, expected)),
                "{}",
                id(shopper)
            );
        }
        // A change is read again only once the state is due, and an
        // operation under way keeps the value it read first.
        let bob = shoppers[1..]
            .iter()
            .find(|s| bucket("new_checkout", &id(s)) >= 50)
            .expect("one outside the share")
            .clone();
        let mut under_way = host.ctx(&program);
        under_way.set_caller(Some(signed_in(&bob)));
        let (new_at, _) = flags.flag("new_checkout").expect("new_checkout");
        assert_eq!(under_way.flag(new_at), Some(Value::Bool(false)));
        set(&host, "new_checkout", Some("true"), None, None).await;
        refresh(&program, &host, start + REFRESH_MS - 1).await;
        assert_eq!(steps(&program, &host, &bob).await, 3, "not due yet");
        refresh(&program, &host, start + REFRESH_MS).await;
        assert_eq!(steps(&program, &host, &bob).await, 1, "everyone");
        assert_eq!(
            under_way.flag(new_at),
            Some(Value::Bool(false)),
            "fixed for the operation"
        );
        // A table that can't be read keeps the last values.
        host.store()
            .query_once(
                "ALTER TABLE \"jadpo_flags\" RENAME TO \"jadpo_flags_gone\"",
                &[],
            )
            .await
            .expect("the table renamed");
        refresh(&program, &host, start + 2 * REFRESH_MS).await;
        assert_eq!(
            steps(&program, &host, &bob).await,
            1,
            "the last values kept"
        );
    });
}

/// A one-file project whose flag expires on `expires`.
fn expiring(name: &str, expires: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("flags-{name}"));
    let _ = fs::remove_dir_all(&root);
    write(
        &root,
        "main.jadpo",
        &format!(
            r#"type Steps = Int {{}}

flags {{
    new_checkout: Bool {{
        default: false
        owner: "payments"
        expires: "{expires}"
    }}
}}

action steps() -> Steps {{
    if (flags.new_checkout) {{
        return 1
    }}
    return 3
}}

test "three steps by default" {{
    assert steps() == 3
}}
"#
        ),
    );
    root
}

#[test]
fn check_fails_once_a_flags_date_has_passed_and_the_rule_cant_be_declined() {
    // `SOURCE_DATE_EPOCH` is 2026-10-20: a flag expiring that day still
    // checks, and one that expired the day before doesn't.
    let on_the_day = expiring("on-the-day", "2026-10-20");
    let checked = jadpo(&on_the_day, &["check", "."]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let past = expiring("past", "2026-10-19");
    let checked = jadpo(&past, &["check", "."]);
    assert_ne!(checked.code, 0);
    let shown = format!("{}{}", checked.stdout, checked.stderr);
    assert!(shown.contains("rule: flag.expired"), "{shown}");
    assert!(shown.contains("owned by \"payments\""), "{shown}");
    assert!(!shown.contains("decline flag.expired"), "{shown}");
    let rule = jadpo_diag::CATALOGUE
        .iter()
        .find(|rule| rule.id == "flag.expired")
        .expect("the rule");
    assert!(!rule.declinable);
}
