//! Area 12's schedules in the checker (chunk 6.2): a schedule says how
//! often it runs with `every:` and runs an action of its own domain that
//! takes nothing; its state and lease go in `schedules: { store: }`, or
//! the only database, and with several and none named it is
//! `schedule.store_required`; and a schedule, which runs as the system,
//! reaching what only `Access.public` grants is refused (area 9, decided
//! 2026-10-19).

use crate::support;

use jadpo_db::{Database, Layout};

const TODO: &str = r#"entity Todo {
    id: Uuid
    done: Bool

    policy: {
        create: [Access.system]
        read: [Access.system]
        update: [Access.system]
    }
}
"#;

const TIDY: &str = r#"use planning/entities { Todo }

action tidy() -> Unit {
    var todo: Todo = Todo.create {
        set: {
            done: false
        }
    }
    assert todo.done == false
}
"#;

const SCHEDULE: &str = r#"use planning/actions { tidy }

/// Tidies the todos.
schedule tidy_up {
    every: 15m
    run: tidy()
}
"#;

const PATH: &str = "domains/planning/schedules/tidy_up.jadpo";

/// The application's files, each of `changed` given instead of its own.
fn files<'a>(changed: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut out: Vec<(&str, &str)> = vec![
        ("app.jadpo", "application {}\n"),
        ("domains/planning/entities/todo.jadpo", TODO),
        ("domains/planning/actions/tidy.jadpo", TIDY),
        (PATH, SCHEDULE),
    ];
    for (path, text) in changed {
        match out.iter_mut().find(|(known, _)| known == path) {
            Some(file) => file.1 = text,
            None => out.push((path, text)),
        }
    }
    out
}

fn problems(changed: &[(&str, &str)]) -> Vec<String> {
    support::problems(Layout::Project, &files(changed))
}

fn rules(changed: &[(&str, &str)]) -> Vec<String> {
    support::rules(Layout::Project, &files(changed))
}

/// `tidy_up` with its body's items `items`.
fn schedule(items: &str) -> String {
    format!(
        "use planning/actions {{ tidy }}\n\n/// Tidies the todos.\nschedule tidy_up {{\n{items}}}\n"
    )
}

#[test]
fn a_schedule_reads_into_how_often_it_runs_and_the_action_it_runs() {
    assert_eq!(problems(&[]), Vec::<String>::new());
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files(&[]));
    let schedules = jadpo_check::schedules::schedules(&db, project);
    let [found] = schedules.as_slice() else {
        panic!("one schedule: {schedules:?}");
    };
    assert_eq!(found.name, "tidy_up");
    assert_eq!(found.every_ms, 15 * 60 * 1000);
    assert_eq!(found.run.action().map(|key| key.name()), Some("tidy"));
    assert_eq!(found.path, PATH);
    // Its state goes in the only database, beside the entities.
    assert_eq!(
        jadpo_check::schedules::schedules_settings(&db, project)
            .store
            .as_deref(),
        Some("main")
    );
    let schema = jadpo_check::data::schema(&db, project);
    let main = schema.stores.get("main").expect("main");
    for table in ["jadpo_schedules", "jadpo_schedule_runs"] {
        assert!(main.tables.contains_key(table), "{table}");
    }
}

#[test]
fn an_application_without_schedules_keeps_no_schedule_state() {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Project,
        &files(&[])
            .into_iter()
            .filter(|(path, _)| *path != PATH)
            .collect::<Vec<_>>(),
    );
    let schema = jadpo_check::data::schema(&db, project);
    let main = schema.stores.get("main").expect("main");
    assert!(!main.tables.contains_key("jadpo_schedules"));
}

#[test]
fn every_takes_whole_seconds_minutes_hours_or_days() {
    for good in ["30s", "15m", "1h", "1d", "7d"] {
        let text = schedule(&format!("    every: {good}\n    run: tidy()\n"));
        assert_eq!(
            rules(&[(PATH, text.as_str())]),
            Vec::<String>::new(),
            "{good}"
        );
    }
    for bad in ["500ms", "0m", "15", "soon"] {
        let text = schedule(&format!("    every: {bad}\n    run: tidy()\n"));
        assert_eq!(
            rules(&[(PATH, text.as_str())]),
            ["schedule.every_invalid"],
            "{bad}"
        );
    }
}

#[test]
fn a_schedule_holds_every_and_run_once_each_and_nothing_else() {
    let unknown = schedule("    every: 15m\n    run: tidy()\n    retry: next_schedule\n");
    assert_eq!(
        problems(&[(PATH, unknown.as_str())]),
        [format!(
            "{PATH} schedule.item_invalid: `retry` isn't something a schedule holds"
        )]
    );
    let missing = schedule("    run: tidy()\n");
    assert_eq!(
        problems(&[(PATH, missing.as_str())]),
        [format!(
            "{PATH} schedule.item_invalid: `tidy_up` has no `every:`"
        )]
    );
    let twice = schedule("    every: 15m\n    every: 1h\n    run: tidy()\n");
    assert_eq!(
        problems(&[(PATH, twice.as_str())]),
        [format!(
            "{PATH} schedule.item_invalid: `every:` is given twice"
        )]
    );
}

#[test]
fn a_schedule_runs_its_own_domains_action_with_nothing() {
    let taking = r#"use planning/entities { Todo }

action tidy_one(todo_id: Todo.id) -> Unit {
    var todo: Todo = Todo.create {
        set: {
            done: false
        }
    }
    assert todo.done == false
}
"#;
    let action = ("domains/planning/actions/tidy_one.jadpo", taking);
    let passing = r#"use planning/actions { tidy }

/// Tidies the todos.
schedule tidy_up {
    every: 15m
    run: tidy(1)
}
"#;
    assert_eq!(
        problems(&[(PATH, passing)]),
        [format!(
            "{PATH} schedule.run_invalid: A schedule passes nothing to `tidy`"
        )]
    );
    let calls_it =
        schedule("    every: 15m\n    run: tidy_one()\n").replace("{ tidy }", "{ tidy_one }");
    assert_eq!(
        problems(&[action, (PATH, calls_it.as_str())]),
        [format!(
            "{PATH} schedule.run_invalid: `tidy_one` takes parameters, which a schedule has nothing to pass"
        )]
    );
    let not_a_call = schedule("    every: 15m\n    run: tidy\n");
    // `tidy` named but not called isn't a use of its import either.
    assert_eq!(
        rules(&[(PATH, not_a_call.as_str())]),
        ["import.unused", "schedule.run_invalid"]
    );
    let other = r#"use billing

/// Tidies the todos.
schedule tidy_up {
    every: 15m
    run: billing.invoice()
}
"#;
    let invoice = "public action invoice() -> Unit {}\n";
    let found = rules(&[
        ("domains/billing/actions/invoice.jadpo", invoice),
        (PATH, other),
    ]);
    assert_eq!(found, ["schedule.run_invalid"], "{found:?}");
}

#[test]
fn several_databases_need_schedules_store_named() {
    let config = "config {\n    main_url: PostgresUrl\n    archive_url: PostgresUrl\n}\n";
    let two = |extra: &str| {
        format!(
            "application {{\n    connections: {{\n        archive: {{\n            driver: Driver.postgres\n            url: config.archive_url\n        }}\n        main: {{\n            driver: Driver.postgres\n            url: config.main_url\n        }}\n    }}\n{extra}}}\n"
        )
    };
    let unnamed = two("");
    assert_eq!(
        problems(&[("app.jadpo", unnamed.as_str()), ("config.jadpo", config)]),
        [format!(
            "{PATH} schedule.store_required: `tidy_up` has no database named to keep its state in"
        )]
    );
    let named = two("    schedules: {\n        store: archive\n    }\n");
    let changed = [("app.jadpo", named.as_str()), ("config.jadpo", config)];
    assert_eq!(problems(&changed), Vec::<String>::new());
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files(&changed));
    let schema = jadpo_check::data::schema(&db, project);
    assert!(
        schema.stores["archive"]
            .tables
            .contains_key("jadpo_schedules"),
        "in the store named"
    );
    assert!(!schema.stores["main"].tables.contains_key("jadpo_schedules"));
    let wrong = two("    schedules: {\n        store: archives\n    }\n");
    assert_eq!(
        problems(&[("app.jadpo", wrong.as_str()), ("config.jadpo", config)]),
        ["app.jadpo schedule.settings_invalid: `store: archives` isn't a database"]
    );
    let unknown = "application {\n    schedules: {\n        tick: native\n    }\n}\n";
    assert_eq!(
        problems(&[("app.jadpo", unknown)]),
        ["app.jadpo schedule.settings_invalid: `tick` isn't a schedules setting"]
    );
}

/// `schedules: { trigger: }` (area 12, question 16; chunk 8.2g): the
/// server's own timer by default, or `ScheduleTrigger.external`, the
/// native deployable's `tick` command; anything else is refused with the
/// nearest.
#[test]
fn a_schedule_trigger_is_the_servers_timer_or_an_external_tick() {
    use jadpo_std::ScheduleTrigger;
    let trigger = |db: &Database, app: &str| {
        let project = support::project(db, Layout::Project, &files(&[("app.jadpo", app)]));
        jadpo_check::schedules::schedules_settings(db, project).trigger
    };
    let db = Database::default();
    assert_eq!(trigger(&db, "application {}\n"), ScheduleTrigger::Native);
    let external =
        "application {\n    schedules: {\n        trigger: ScheduleTrigger.external\n    }\n}\n";
    assert_eq!(problems(&[("app.jadpo", external)]), Vec::<String>::new());
    let db = Database::default();
    assert_eq!(trigger(&db, external), ScheduleTrigger::External);
    let native = "application {\n    schedules: {\n        store: main\n        trigger: ScheduleTrigger.native\n    }\n}\n";
    assert_eq!(problems(&[("app.jadpo", native)]), Vec::<String>::new());
    let db = Database::default();
    assert_eq!(trigger(&db, native), ScheduleTrigger::Native);
    for (written, near) in [
        ("ScheduleTrigger.externl", "ScheduleTrigger.external"),
        ("native", "ScheduleTrigger.native"),
        ("ScheduleTrigger.cron", "ScheduleTrigger.external"),
    ] {
        let app =
            format!("application {{\n    schedules: {{\n        trigger: {written}\n    }}\n}}\n");
        let db = Database::default();
        let project =
            support::project(&db, Layout::Project, &files(&[("app.jadpo", app.as_str())]));
        let found = jadpo_check::check(&db, project);
        let [problem] = found.as_slice() else {
            panic!("one problem for `{written}`: {found:?}");
        };
        assert_eq!(problem.rule.id, "schedule.settings_invalid");
        assert_eq!(
            problem.message,
            format!("`{written}` isn't a schedule trigger")
        );
        assert!(
            problem
                .next
                .as_ref()
                .is_some_and(|next| next.text.contains(&format!("`{near}`"))),
            "{problem:?}"
        );
    }
}

#[test]
fn a_schedule_reaching_what_only_access_public_grants_is_refused() {
    let public = TODO.replace("create: [Access.system]", "create: [Access.public]");
    assert_eq!(
        problems(&[("domains/planning/entities/todo.jadpo", public.as_str())]),
        [format!(
            "{PATH} policy.system_grant_missing: `tidy_up` runs as the system, which only `Access.public` lets create `Todo`"
        )]
    );
    // Naming `Access.system` beside it lets background work in on purpose.
    let both = TODO.replace(
        "create: [Access.system]",
        "create: [Access.public, Access.system]",
    );
    assert_eq!(
        problems(&[("domains/planning/entities/todo.jadpo", both.as_str())]),
        Vec::<String>::new()
    );
}
