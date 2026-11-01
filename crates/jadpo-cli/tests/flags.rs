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

test "a shopper gets the wide layout a test's flags give" {
    flags: {
        checkout_layout: CheckoutLayout.wide
    }
    var ann: Shopper = attempt join("ann")
    var response: Response = request {
        method: HttpMethod.post
        path: "/checkouts"
        as: Principal.user(ann)
    }
    var wide: Bool = match response.output {
        some(view) -> view.layout == CheckoutLayout.wide
        none -> false
    }
    assert wide
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
    assert!(tested.stdout.contains("3 passed"), "{}", tested.stdout);
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

test "one step when it is on" {{
    flags: {{
        new_checkout: true
    }}
    assert steps() == 1
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

// --- chunk B10: changing, listing, removing and `GET /flags` -------------------

/// `jadpo` as `sam`, the name the operator's machine gives.
fn operator(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let env = Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", "1792504800")
        .with_var("USER", "sam");
    run(&args, &env.with_cwd(cwd))
}

/// A one-file project reading `new_checkout` in an action, with `tests`.
fn reading(name: &str, tests: &str) -> PathBuf {
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
        expires: "2099-01-31"
    }}
}}

action steps() -> Steps {{
    if (flags.new_checkout) {{
        return 1
    }}
    return 3
}}

action shown() -> Steps {{
    if (flags.new_checkout) {{
        return 1
    }}
    return 3
}}
{tests}"#
        ),
    );
    root
}

#[test]
fn check_fails_until_the_tests_that_reach_each_read_give_every_value() {
    let one = reading(
        "loose-untested",
        r#"
test "three steps by default" {
    assert steps() == 3
}
"#,
    );
    let checked = jadpo(&one, &["check", "."]);
    assert_ne!(checked.code, 0);
    let shown = format!("{}{}", checked.stdout, checked.stderr);
    assert!(shown.contains("rule: flag.value_untested"), "{shown}");
    assert!(
        shown.contains("No test that reaches `flags.new_checkout` gives it `true`"),
        "{shown}"
    );
    assert!(
        shown.contains("No test reaches this read of `flags.new_checkout`"),
        "{shown}"
    );
    let rule = jadpo_diag::CATALOGUE
        .iter()
        .find(|rule| rule.id == "flag.value_untested")
        .expect("the rule");
    assert!(!rule.declinable);
    let both = reading(
        "loose-tested",
        r#"
test "three steps by default" {
    assert steps() == 3
    assert shown() == 3
}

test "one step when it is on" {
    flags: {
        new_checkout: true
    }
    assert steps() == 1
    assert shown() == 1
}
"#,
    );
    let checked = jadpo(&both, &["check", "."]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let tested = jadpo(&both, &["test", "."]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
}

#[test]
fn a_routes_run_read_needs_the_requests_that_reach_it_to_give_every_value() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("flags-route-untested");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        // Without the wide layout's test.
        let text = match text.find("test \"a shopper gets the wide layout") {
            Some(at) if path.ends_with("checkouts.test.jadpo") => &text[..at],
            _ => text,
        };
        write(&root, path, text);
    }
    let checked = jadpo(&root, &["check"]);
    let shown = format!("{}{}", checked.stdout, checked.stderr);
    assert!(
        shown.contains(
            "No test that reaches `flags.checkout_layout` gives it `CheckoutLayout.wide`"
        ),
        "{shown}"
    );
    assert!(shown.contains("checkouts.jadpo"), "{shown}");
}

/// The state `jadpo_flags` holds for `flag`, as JSON text, and how many
/// changes were logged for it, with the last one's operator and role.
async fn kept(host: &Harness, flag: &str) -> (Option<Vec<SqlValue>>, Vec<Vec<SqlValue>>) {
    let row = host
        .store()
        .query_once(
            "SELECT \"value\", \"users\"::text, \"shares\"::text FROM \"jadpo_flags\" WHERE \"flag\" = $1",
            &[SqlValue::Text(flag.to_string())],
        )
        .await
        .expect("the flags read")
        .into_iter()
        .next();
    let log = host
        .store()
        .query_once(
            "SELECT \"action\", \"operator\", \"role\", \"was\"::text, \"now\"::text FROM \"jadpo_flag_changes\" WHERE \"flag\" = $1 ORDER BY \"made_at\", \"id\"",
            &[SqlValue::Text(flag.to_string())],
        )
        .await
        .expect("the log read");
    (row, log)
}

fn text_of(value: &SqlValue) -> String {
    match value {
        SqlValue::Text(text) | SqlValue::Json(text) => text.clone(),
        SqlValue::Null(_) => "null".to_string(),
        other => format!("{other:?}"),
    }
}

#[test]
fn set_list_and_reset_change_the_table_and_log_who_made_each_change() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("operate");
    let loaded = jadpo_db::load(&root);
    let steps_of = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let databases = jadpo_test::database::TestDatabases::new(steps_of, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url);
    let user = "0192F1C4-7A3E-7C1D-9B2A-5E6F7A8B9C0D";
    let runtime = Runtime::new().expect("runtime");

    // Before any change, every flag is at its default.
    let listed = operator(&root, &["flags", "--url", &url]);
    assert_eq!(listed.code, 0, "{}{}", listed.stdout, listed.stderr);
    assert!(listed.stdout.contains("default"), "{}", listed.stdout);

    // A named user and a share, as area 16's example sets them.
    let set = operator(
        &root,
        &[
            "flags",
            "set",
            "new_checkout",
            "--users",
            user,
            "--percent",
            "10",
            "--url",
            &url,
        ],
    );
    assert_eq!(set.code, 0, "{}{}", set.stdout, set.stderr);
    assert!(set.stdout.contains("Set `new_checkout`"), "{}", set.stdout);
    assert!(
        set.stdout.contains("false for everyone (the default)"),
        "{}",
        set.stdout
    );
    assert!(
        set.stdout.contains(
            "true for 1 named user; true for 10% of users; false (the default) for everyone else"
        ),
        "{}",
        set.stdout
    );
    assert!(set.stdout.contains("sam · role "), "{}", set.stdout);
    runtime.block_on(async {
        let (row, log) = kept(&host, "new_checkout").await;
        let row = row.expect("a row");
        assert_eq!(text_of(&row[0]), "null");
        assert!(
            text_of(&row[1]).contains(&user.to_ascii_lowercase()),
            "{row:?}"
        );
        assert_eq!(text_of(&row[2]).replace(' ', ""), r#"[["true",10]]"#);
        assert_eq!(log.len(), 1);
        assert_eq!(text_of(&log[0][0]), "set");
        assert_eq!(text_of(&log[0][1]), "sam");
        assert!(!text_of(&log[0][2]).is_empty());
        assert_eq!(text_of(&log[0][3]), "null");
    });

    // An enum's split, and a value written as its type writes it.
    let split = operator(
        &root,
        &[
            "flags",
            "set",
            "checkout_layout",
            "--split",
            "classic=50,compact=50",
            "--url",
            &url,
        ],
    );
    assert_eq!(split.code, 0, "{}{}", split.stdout, split.stderr);
    let everyone = operator(
        &root,
        &[
            "flags",
            "set",
            "checkout_layout",
            "CheckoutLayout.wide",
            "--url",
            &url,
        ],
    );
    assert_eq!(everyone.code, 0, "{}{}", everyone.stdout, everyone.stderr);
    runtime.block_on(async {
        let (row, log) = kept(&host, "checkout_layout").await;
        let row = row.expect("a row");
        assert_eq!(text_of(&row[0]), "wide");
        assert_eq!(
            text_of(&row[2]).replace(' ', ""),
            r#"[["classic",50],["compact",50]]"#
        );
        assert_eq!(log.len(), 2);
    });

    // Refused: a value the flag doesn't have, an id that isn't one, a
    // split over 100%, and a flag the project doesn't declare.
    for (args, said) in [
        (
            vec!["flags", "set", "checkout_layout", "narrow"],
            "`narrow` isn't a value of `checkout_layout`",
        ),
        (
            vec!["flags", "set", "new_checkout", "--users", "ann"],
            "`ann` isn't a user's id",
        ),
        (
            vec![
                "flags",
                "set",
                "checkout_layout",
                "--split",
                "classic=60,wide=60",
            ],
            "`--split` gives 120% of users",
        ),
        (
            vec!["flags", "set", "new_chekout", "true"],
            "Did you mean `new_checkout`?",
        ),
        (
            vec!["flags", "reset", "new_checkout", "--keep", "true"],
            "`jadpo flags reset` takes no `--keep`",
        ),
    ] {
        let mut args = args;
        args.extend(["--url", url.as_str()]);
        let refused = operator(&root, &args);
        assert_ne!(refused.code, 0, "{args:?}");
        let shown = format!("{}{}", refused.stdout, refused.stderr);
        assert!(shown.contains("rule: cli.flags_arguments"), "{shown}");
        assert!(shown.contains(said), "{shown}");
    }

    let route = operator(
        &root,
        &["flags", "remove", "checkout_layout", "--keep", "wide"],
    );
    assert_ne!(route.code, 0);
    let shown = format!("{}{}", route.stdout, route.stderr);
    assert!(shown.contains("passed on by the route"), "{shown}");

    // Everyone on: one value for every caller, ready to remove once it
    // has been so for seven days.
    let on = operator(
        &root,
        &["flags", "set", "new_checkout", "true", "--url", &url],
    );
    assert_eq!(on.code, 0, "{}{}", on.stdout, on.stderr);
    let listed = operator(&root, &["flags", "--url", &url]);
    assert!(
        listed.stdout.contains("true for everyone"),
        "{}",
        listed.stdout
    );
    assert!(
        !listed.stdout.contains("ready to remove"),
        "{}",
        listed.stdout
    );
    runtime.block_on(async {
        host.store()
            .query_once(
                "UPDATE \"jadpo_flags\" SET \"set_at\" = \"set_at\" - interval '8 days' WHERE \"flag\" = 'new_checkout'",
                &[],
            )
            .await
            .expect("the change made older");
    });
    let listed = operator(&root, &["flags", "list", "--url", &url]);
    assert_eq!(listed.code, 0, "{}{}", listed.stdout, listed.stderr);
    assert!(
        listed.stdout.contains("ready to remove"),
        "{}",
        listed.stdout
    );
    assert!(
        listed
            .stdout
            .contains("jadpo flags remove new_checkout --keep true"),
        "{}",
        listed.stdout
    );
    let json = operator(&root, &["flags", "--url", &url, "--json"]);
    assert!(
        json.stdout.contains("\"readyToRemove\": true"),
        "{}",
        json.stdout
    );

    // Setting what it already gives changes nothing, and logs nothing.
    let again = operator(
        &root,
        &["flags", "set", "new_checkout", "true", "--url", &url],
    );
    assert!(again.stdout.contains("already gives"), "{}", again.stdout);

    // Reset: the row goes, and everyone gets the default.
    let reset = operator(&root, &["flags", "reset", "new_checkout", "--url", &url]);
    assert_eq!(reset.code, 0, "{}{}", reset.stdout, reset.stderr);
    assert!(
        reset.stdout.contains("everyone gets its default"),
        "{}",
        reset.stdout
    );
    runtime.block_on(async {
        let (row, log) = kept(&host, "new_checkout").await;
        assert!(row.is_none());
        assert_eq!(log.len(), 3, "the repeated set logged nothing");
        let reset = log
            .iter()
            .find(|row| text_of(&row[0]) == "reset")
            .expect("the reset");
        assert_eq!(text_of(&reset[4]), "null");
        assert!(text_of(&reset[3]).contains("\"value\""), "{reset:?}");
    });
}

const REMOVING: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
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
        "flags.jadpo",
        r#"use shop/types { Layout }

flags {
    /// The one-page checkout.
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
    }

    /// The layout.
    layout: Layout {
        default: Layout.classic
        owner: "payments"
        expires: "2099-01-31"
    }
}
"#,
    ),
    (
        "domains/shop/types/layout.jadpo",
        r#"type Layout = Enum {
    classic
    compact
}
"#,
    ),
    (
        "domains/shop/types/step_count.jadpo",
        r#"type StepCount = Int {}
"#,
    ),
    (
        "domains/shop/actions/steps.jadpo",
        r#"use shop/types { Layout, StepCount }

action steps() -> StepCount {
    if (flags.new_checkout) {
        if (flags.layout == Layout.compact) {
            return 2
        }
        return 1
    }
    return 3
}
"#,
    ),
    (
        "domains/shop/actions/steps.test.jadpo",
        r#"use shop/types { Layout }

test "three steps by default" {
    assert steps() == 3
}

test "one step in the classic layout" {
    flags: {
        new_checkout: true
    }
    assert steps() == 1
}

test "two steps in the compact layout" {
    flags: {
        new_checkout: true
        layout: Layout.compact
    }
    assert steps() == 2
}
"#,
    ),
];

fn removing(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("flags-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in REMOVING {
        write(&root, path, text);
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    root
}

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path)).unwrap_or_default()
}

#[test]
fn removing_a_flag_keeps_its_branch_and_the_project_checks() {
    let root = removing("remove-kept");
    let removed = jadpo(
        &root,
        &["flags", "remove", "new_checkout", "--keep", "true"],
    );
    assert_eq!(removed.code, 0, "{}{}", removed.stdout, removed.stderr);
    assert!(
        removed.stdout.contains("Removed `new_checkout`"),
        "{}",
        removed.stdout
    );
    assert!(
        removed.stdout.contains("- test \"three steps by default\""),
        "{}",
        removed.stdout
    );
    assert_eq!(
        read(&root, "domains/shop/actions/steps.jadpo"),
        r#"use shop/types { Layout, StepCount }

action steps() -> StepCount {
    if (flags.layout == Layout.compact) {
        return 2
    }
    return 1
}
"#
    );
    let tests = read(&root, "domains/shop/actions/steps.test.jadpo");
    assert!(!tests.contains("three steps by default"), "{tests}");
    assert!(!tests.contains("new_checkout"), "{tests}");
    assert!(tests.contains("layout: Layout.compact"), "{tests}");
    let flags = read(&root, "flags.jadpo");
    assert!(!flags.contains("new_checkout"), "{flags}");
    assert!(flags.contains("layout: Layout"), "{flags}");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    if jadpo_test::live() {
        let tested = jadpo(&root, &["test", "--seed", "1"]);
        assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
        assert!(tested.stdout.contains("2 passed"), "{}", tested.stdout);
    }
}

#[test]
fn removing_an_enum_flag_drops_the_branches_it_rules_out_and_its_enum() {
    let root = removing("remove-enum");
    let removed = jadpo(&root, &["flags", "remove", "layout", "--keep", "classic"]);
    assert_eq!(removed.code, 0, "{}{}", removed.stdout, removed.stderr);
    assert_eq!(
        read(&root, "domains/shop/actions/steps.jadpo"),
        r#"use shop/types { StepCount }

action steps() -> StepCount {
    if (flags.new_checkout) {
        return 1
    }
    return 3
}
"#
    );
    assert!(!root.join("domains/shop/types/layout.jadpo").exists());
    let tests = read(&root, "domains/shop/actions/steps.test.jadpo");
    assert!(
        !tests.contains("two steps in the compact layout"),
        "{tests}"
    );
    assert!(!tests.contains("use shop/types"), "{tests}");
    // `flags { }` keeps its other flag, and loses the import.
    let flags = read(&root, "flags.jadpo");
    assert!(flags.starts_with("flags {"), "{flags}");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    if jadpo_test::live() {
        let tested = jadpo(&root, &["test", "--seed", "1"]);
        assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    }
}

#[test]
fn removing_the_last_flag_leaves_flags_empty_and_its_tables() {
    let root = removing("remove-last");
    for (flag, keep) in [("layout", "compact"), ("new_checkout", "false")] {
        let removed = jadpo(&root, &["flags", "remove", flag, "--keep", keep]);
        assert_eq!(removed.code, 0, "{}{}", removed.stdout, removed.stderr);
    }
    assert_eq!(read(&root, "flags.jadpo"), "flags {}\n");
    assert!(
        read(&root, "domains/shop/actions/steps.jadpo").contains("return 3\n}"),
        "{}",
        read(&root, "domains/shop/actions/steps.jadpo")
    );
    // The flags' tables stay, so no migration is needed.
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
}

#[test]
fn a_removal_needs_a_value_of_the_flag_and_a_project_that_checks() {
    let root = removing("remove-refused");
    let without = jadpo(&root, &["flags", "remove", "new_checkout"]);
    assert_ne!(without.code, 0);
    assert!(
        format!("{}{}", without.stdout, without.stderr).contains("needs `--keep`"),
        "{}",
        without.stdout
    );
    let wrong = jadpo(&root, &["flags", "remove", "layout", "--keep", "wide"]);
    assert_ne!(wrong.code, 0);
    assert!(
        format!("{}{}", wrong.stdout, wrong.stderr).contains("`wide` isn't a value of `layout`"),
        "{}",
        wrong.stdout
    );
    assert!(read(&root, "flags.jadpo").contains("new_checkout"));
}

const CLIENT: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
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
        "flags.jadpo",
        r#"use shop/types { Layout }

flags {
    /// The one-page checkout, shown by the browser.
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
        client: true
    }

    /// The layout, which stays on the server.
    layout: Layout {
        default: Layout.classic
        owner: "payments"
        expires: "2099-01-31"
    }
}
"#,
    ),
    (
        "domains/shop/types/layout.jadpo",
        r#"type Layout = Enum {
    classic
    compact
}
"#,
    ),
    (
        "domains/shop/types/step_count.jadpo",
        r#"use shop/types { Layout }

type StepCount = Output {
    steps: Int
    layout: Layout
}
"#,
    ),
    (
        "domains/shop/actions/steps.jadpo",
        r#"use shop/types { Layout, StepCount }

action steps(layout: Layout) -> StepCount {
    if (flags.new_checkout) {
        return StepCount {
            steps: 1
            layout: layout
        }
    }
    return StepCount {
        steps: 3
        layout: layout
    }
}
"#,
    ),
    (
        "domains/shop/actions/steps.test.jadpo",
        r#"use shop/types { Layout }

test "three steps by default" {
    assert steps(Layout.classic).steps == 3
}

test "one step when it is on" {
    flags: {
        new_checkout: true
    }
    assert steps(Layout.classic).steps == 1
}
"#,
    ),
    (
        "domains/shop/routes/checkouts.jadpo",
        r#"use shop/actions { steps }
use shop/types { StepCount }

/// The checkout.
routes Checkouts {
    prefix: /checkouts

    /// Starts a checkout.
    POST / {
        output: StepCount
        status: SuccessStatus.created
        authentication: RouteAuthentication.none
        run: steps(flags.layout)
    }
}
"#,
    ),
    (
        "domains/shop/routes/checkouts.test.jadpo",
        r#"use shop/types { Layout }

test "the classic layout by default" {
    var response: Response = request {
        method: HttpMethod.post
        path: "/checkouts"
    }
    assert response.status == 201
}

test "the compact layout when the flag says" {
    flags: {
        layout: Layout.compact
    }
    var response: Response = request {
        method: HttpMethod.post
        path: "/checkouts"
    }
    assert response.status == 201
}
"#,
    ),
    (
        "policy.jadpo",
        r#"use shop

policy {
    actions: {
        shop.steps: [Access.public]
    }
}
"#,
    ),
];

#[test]
fn get_flags_answers_each_client_flag_and_the_openapi_document_lists_it() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("flags-client");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in CLIENT {
        write(&root, path, text);
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let document = read(&root, "build/openapi/main.json");
    assert!(document.contains("\"/flags\""), "{document}");
    assert!(document.contains("\"new_checkout\""), "{document}");
    assert!(!document.contains("\"layout\": {\n                      \"type\": \"boolean\""));
    let routes = jadpo(&root, &["routes"]);
    assert!(routes.stdout.contains("/flags"), "{}", routes.stdout);
    if !jadpo_test::live() {
        return;
    }
    let loaded = jadpo_db::load(&root);
    let steps_of = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps_of, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url);
    let get = |now: i64| {
        let runtime = Runtime::new().expect("runtime");
        runtime.block_on(async {
            refresh(&program, &host, now).await;
            let mut ctx = host.ctx(&program);
            let request = jadpo_runtime::http::Request {
                method: "GET".to_string(),
                path: "/flags".to_string(),
                ..Default::default()
            };
            jadpo_runtime::http::handle(&program.app(), &program, &mut ctx, &request, "req_1").await
        })
    };
    let start = 1_792_504_800_000;
    let answered = get(start);
    assert_eq!(answered.status, 200);
    assert_eq!(
        String::from_utf8_lossy(&answered.body),
        r#"{"new_checkout":false}"#
    );
    assert!(
        answered
            .headers
            .iter()
            .any(|(name, value)| name == "cache-control" && value == "no-store")
    );
    let set = operator(
        &root,
        &["flags", "set", "new_checkout", "true", "--url", &url],
    );
    assert_eq!(set.code, 0, "{}{}", set.stdout, set.stderr);
    let answered = get(start + REFRESH_MS);
    assert_eq!(
        String::from_utf8_lossy(&answered.body),
        r#"{"new_checkout":true}"#
    );
}
