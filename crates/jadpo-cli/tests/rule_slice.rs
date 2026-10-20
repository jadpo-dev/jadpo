//! Rules across fields where a request or the store makes a value (area 2,
//! "Rules across fields"; chunk 5.8c, from the slice the Phase 5 soundness
//! review couldn't cover), over real HTTP and live PostgreSQL:
//!
//! - a query string that breaks an `Input`'s rule is refused as invalid
//!   input, the problem naming the rule, never a value, as a body is;
//! - a body naming a member twice is no JSON Jadpo reads, refused before
//!   anything is decoded;
//! - a standard object read from the store is made like any other value,
//!   so a row written around Jadpo that breaks its rules is a defect, said
//!   by the rule's name, never kept.

use std::path::Path;

use jadpo_interp::Program;
use jadpo_runtime::{Clock, Ctx, Database, Env, Executor, Outcome, SqlValue};
use jadpo_test::host::Bench;
use jadpo_value::Value;

use super::http::{Dev, jadpo, tempdir, write};

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/stays/actions/find_stay.jadpo",
        r#"use stays/types { StaySearch, StayView }

/// The stay asked for, as it was asked.
action find_stay(input: StaySearch) -> StayView {
    return StayView {
        arrives: input.arrives
        leaves: input.leaves
    }
}
"#,
    ),
    (
        "domains/stays/routes/stays.jadpo",
        r#"use stays/types { StayCard, StaySearch, StayView }
use stays/actions { find_stay }
use stays/queries { stays_from }

routes Stays {
    prefix: /stays

    /// The stays from a night, from the query string.
    GET / {
        authentication: RouteAuthentication.none
        input: StaySearch
        output: List<StayCard>
        run: stays_from(input)
    }

    /// A stay, from the body.
    POST / {
        authentication: RouteAuthentication.none
        input: StaySearch
        output: StayView
        run: find_stay(input)
    }
}
"#,
    ),
    (
        "domains/stays/types/stay_search.jadpo",
        r#"use stays/types { StayDate }

/// The nights a stay is asked for.
type StaySearch = Input {
    arrives: StayDate
    leaves: StayDate
    rule leaves_after_arrival: leaves > arrives
}
"#,
    ),
    (
        "domains/stays/types/stay_view.jadpo",
        r#"use stays/types { StayDate }

/// A stay.
type StayView = Output {
    arrives: StayDate
    leaves: StayDate
}
"#,
    ),
    (
        "domains/stays/entities/stay.jadpo",
        r#"use stays/types { StayDate }

/// A stay booked.
entity Stay {
    id: Uuid
    arrives: StayDate
    leaves: StayDate

    policy: {
        read: [Access.public]
    }
}
"#,
    ),
    (
        "domains/stays/types/stay_card.jadpo",
        r#"use stays/entities { Stay }

/// A stay in a list.
type StayCard = Output {
    pick: [Stay.id, Stay.arrives]
}
"#,
    ),
    (
        "domains/stays/queries/stays_from.jadpo",
        r#"use stays/entities { Stay }
use stays/types { StayCard, StaySearch }

/// The stays arriving from a night.
query stays_from(input: StaySearch) -> List<StayCard> {
    from: Stay
    where: Stay.arrives >= input.arrives
    order: [Stay.id asc]
    limit: 10
}
"#,
    ),
    (
        "domains/stays/types/stay_date.jadpo",
        "/// A day of a stay.\ntype StayDate = CalendarDate {}\n",
    ),
    (
        "policy.jadpo",
        "use stays\n\npolicy {\n    actions: {\n        stays.find_stay: [Access.public]\n    }\n}\n",
    ),
];

fn project() -> (tempdir::Dir, std::path::PathBuf) {
    let dir = tempdir::Dir::new("rule-slice");
    let root = dir.path().join("stays");
    for (path, text) in FILES {
        write(&root, path, text);
    }
    for args in [
        &["migrate", "new", "initial"][..],
        &["api", "update"],
        &["check"],
    ] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    (dir, root)
}

#[test]
fn a_query_string_that_breaks_a_rule_is_refused_by_its_name() {
    if !jadpo_test::live() {
        return;
    }
    let (_dir, root) = project();
    let dev = Dev::start(&root);
    let asked = dev.send(
        "GET",
        "/stays?arrives=2026-03-02&leaves=2026-03-05",
        &[],
        "",
    );
    assert_eq!(asked.status, 200, "{asked:?}");
    let backwards = dev.send(
        "GET",
        "/stays?arrives=2026-03-05&leaves=2026-03-02",
        &[],
        "",
    );
    assert_eq!(backwards.status, 422, "{backwards:?}");
    let shown = backwards.json().to_string();
    assert!(shown.contains("leaves_after_arrival"), "{shown}");
    assert!(!shown.contains("2026"), "{shown}");
    // The same rule in a body.
    let body = dev.send(
        "POST",
        "/stays",
        &[],
        r#"{"arrives": "2026-03-05", "leaves": "2026-03-02"}"#,
    );
    assert_eq!(body.status, 422, "{body:?}");
    assert!(body.json().to_string().contains("leaves_after_arrival"));
}

#[test]
fn a_body_naming_a_member_twice_is_refused() {
    if !jadpo_test::live() {
        return;
    }
    let (_dir, root) = project();
    let dev = Dev::start(&root);
    let once = dev.send(
        "POST",
        "/stays",
        &[],
        r#"{"arrives": "2026-03-02", "leaves": "2026-03-05"}"#,
    );
    assert_eq!(once.status, 200, "{once:?}");
    // Last one winning would make a stay that follows its rule out of one
    // a reader keeping the first would refuse.
    let twice = dev.send(
        "POST",
        "/stays",
        &[],
        r#"{"arrives": "2026-03-05", "leaves": "2026-03-02", "leaves": "2026-03-09"}"#,
    );
    assert_eq!(twice.status, 400, "{twice:?}");
}

#[test]
fn a_stored_value_that_breaks_its_rules_is_never_read_as_one() {
    if !jadpo_test::live() {
        return;
    }
    let dir = tempdir::Dir::new("rule-slice-store");
    let root = dir.path().join("reviews");
    for (path, text) in [
        ("app.jadpo", "application {}\n"),
        (
            "domains/reviews/entities/review.jadpo",
            "/// A review.\nentity Review {\n    id: Uuid\n    stars: Rating\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/reviews/failures/review_not_found.jadpo",
            "/// No such review.\ntype ReviewNotFound = Failure {\n    kind: FailureKind.not_found\n}\n",
        ),
        (
            "domains/reviews/actions/find_review.jadpo",
            "use reviews/entities { Review }\nuse reviews/failures { ReviewNotFound }\n\n/// A review.\naction find_review(review_id: Review.id) fails ReviewNotFound -> Review {\n    return attempt Review.get(review_id)\n}\n",
        ),
    ] {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server
        .create_database("rule_slice_store")
        .expect("database");
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    let id = |n: u8| {
        let mut id = [0u8; 16];
        id[15] = n;
        id[6] = 0x40;
        id[8] = 0x80;
        id
    };
    let store = jadpo_host_native::postgres(&url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        for (n, value) in [(1, "4.5"), (2, "9")] {
            transaction
                .execute(
                    "INSERT INTO review (id, stars_value, stars_best, stars_worst) VALUES ($1, $2, 5, 1)",
                    &[SqlValue::Uuid(id(n)), SqlValue::Numeric(value.to_string())],
                )
                .await
                .expect("insert");
        }
        transaction.commit().await.expect("commit");
    });
    let loaded = jadpo_db::load(&root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let host = Bench::postgres(&url);
    let call = |n: u8| {
        let action = program.plan().find("find_review").expect("the action");
        runtime.block_on(async {
            let env = Env {
                seed: Some(1),
                clock: Clock::TEST,
            };
            let mut ctx = Ctx::new(program.places(), &host, env);
            program
                .run(
                    &mut ctx,
                    action,
                    vec![Value::uuid(&jadpo_runtime::uuid_text(&id(n)))],
                )
                .await
        })
    };
    assert!(matches!(call(1), Outcome::Value(Value::Record(_))));
    // Nine stars out of five, written around Jadpo.
    match call(2) {
        Outcome::Fault(fault) => {
            let shown = format!("{fault:?}");
            assert!(shown.contains("value_on_scale"), "{shown}");
            assert!(!shown.contains('9'), "{shown}");
        }
        other => panic!("read as a rating: {other:?}"),
    }
    let _ = Path::new(&root);
}
