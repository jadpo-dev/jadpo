//! Area 12's events and subscribers in the checker (chunk 6.1b): a
//! subscriber names one event and runs an action of its own domain with
//! the event or its fields; `emit` is an action's, of an event; an action
//! that emits keeps its events in the store it writes, or the one
//! `events: { store: }` names; and a subscriber, which runs as the
//! system, reaching what only `Access.public` grants is refused (area 9,
//! decided 2026-10-19).

use crate::support;

use jadpo_db::{Database, Layout};

const TODO: &str = r#"entity Todo {
    id: Uuid
    done: Bool

    policy: {
        create: [Access.authenticated]
        read: [Access.authenticated, Access.system]
        update: [Access.system]
    }
}
"#;

const EVENT: &str = r#"use planning/entities { Todo }

/// A todo was added.
type TodoAdded = Event {
    todo_id: Todo.id
}
"#;

const ADD: &str = r#"use planning/entities { Todo }
use planning/events { TodoAdded }

action add() -> Todo {
    var todo: Todo = Todo.create {
        set: {
            done: false
        }
    }
    emit TodoAdded {
        todo_id: todo.id
    }
    return todo
}
"#;

const MARK: &str = r#"use planning/entities { Todo, TodoNotFound }

action mark(todo_id: Todo.id) fails TodoNotFound -> Unit {
    attempt Todo.update(todo_id) {
        set: {
            done: true
        }
    }
}
"#;

const SUBSCRIBER: &str = r#"use planning/actions { mark }
use planning/events { TodoAdded }

/// Marks each new todo done.
subscriber mark_added {
    on: TodoAdded
    run: mark(event.todo_id)
}
"#;

const APP: &str = "application {}\n";

const AUTH: &str = r#"use planning/entities { Person }

principal {
    user: {
        subject: Person.handle
    }
}

authentication {
    browser_session: {
        cookie: "todo_session"
    }
}
"#;

const PERSON: &str = r#"entity Person {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }
}

type Handle = Text {
    min_length: 3
    max_length: 40
}
"#;

/// The application's files, each of `changed` given instead of its own.
fn files<'a>(changed: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut out: Vec<(&str, &str)> = vec![
        ("app.jadpo", APP),
        ("authentication.jadpo", AUTH),
        ("domains/planning/entities/person.jadpo", PERSON),
        ("domains/planning/entities/todo.jadpo", TODO),
        ("domains/planning/events/todo_added.jadpo", EVENT),
        ("domains/planning/actions/add.jadpo", ADD),
        ("domains/planning/actions/mark.jadpo", MARK),
        ("domains/planning/subscribers/mark_added.jadpo", SUBSCRIBER),
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

#[test]
fn a_subscriber_reads_into_the_event_it_listens_for_and_the_action_it_runs() {
    assert_eq!(problems(&[]), Vec::<String>::new());
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files(&[]));
    let subscribers = jadpo_check::events::subscribers(&db, project);
    let [subscriber] = subscribers.as_slice() else {
        panic!("one subscriber: {subscribers:?}");
    };
    assert_eq!(subscriber.name, "mark_added");
    assert!(
        subscriber.event.as_str().ends_with("TodoAdded"),
        "{:?}",
        subscriber.event
    );
    assert_eq!(subscriber.run.name(), "mark");
    assert_eq!(subscriber.args, [vec!["todo_id".to_string()]]);
    assert_eq!(subscriber.retry, jadpo_check::events::DEFAULT_RETRY);
    assert_eq!(subscriber.tenant, jadpo_check::events::Tenant::Untenanted);
    // The action that emits keeps its events in the store it writes.
    assert_eq!(
        jadpo_check::events::outbox_store(&db, project, "add").as_deref(),
        Some("main")
    );
    assert_eq!(
        jadpo_check::events::outbox_store(&db, project, "mark"),
        None
    );
}

#[test]
fn an_action_that_writes_no_store_names_where_its_events_are_kept() {
    let ping = r#"use planning/entities { Todo }
use planning/events { TodoAdded }

action ping(todo_id: Todo.id) -> Unit {
    emit TodoAdded {
        todo_id: todo_id
    }
}
"#;
    let found = problems(&[("domains/planning/actions/ping.jadpo", ping)]);
    assert_eq!(
        found,
        [
            "domains/planning/actions/ping.jadpo event.store_required: `ping` emits `TodoAdded` but writes no store to keep it in"
        ]
    );
    let named = "application {\n    events: {\n        store: main\n    }\n}\n";
    assert_eq!(
        problems(&[
            ("domains/planning/actions/ping.jadpo", ping),
            ("app.jadpo", named)
        ]),
        Vec::<String>::new()
    );
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Project,
        &files(&[
            ("domains/planning/actions/ping.jadpo", ping),
            ("app.jadpo", named),
        ]),
    );
    assert_eq!(
        jadpo_check::events::outbox_store(&db, project, "ping").as_deref(),
        Some("main")
    );
    // `events:` names a database, and holds nothing else.
    for wrong in [
        "application {\n    events: {\n        store: archive\n    }\n}\n",
        "application {\n    events: {\n        keep: main\n    }\n}\n",
    ] {
        assert_eq!(
            rules(&[
                ("domains/planning/actions/ping.jadpo", ping),
                ("app.jadpo", wrong)
            ]),
            ["event.settings_invalid", "event.store_required"],
            "{wrong}"
        );
    }
}

#[test]
fn only_an_action_emits_and_only_an_event() {
    let not_event = r#"use planning/entities { Todo }

action add() -> Todo {
    var todo: Todo = Todo.create {
        set: {
            done: false
        }
    }
    emit todo
    return todo
}
"#;
    assert_eq!(
        rules(&[("domains/planning/actions/add.jadpo", not_event)]),
        ["event.emit_invalid"]
    );
    let in_function = r#"use planning/entities { Todo }
use planning/events { TodoAdded }

function announce(todo_id: Todo.id) -> TodoAdded {
    emit TodoAdded {
        todo_id: todo_id
    }
    return TodoAdded {
        todo_id: todo_id
    }
}
"#;
    let found = rules(&[("domains/planning/actions/announce.jadpo", in_function)]);
    assert!(
        found.contains(&"event.emit_invalid".to_string()),
        "{found:?}"
    );
}

#[test]
fn a_subscriber_names_one_event_and_runs_its_own_domains_action_with_it() {
    let subscriber = |body: &str| {
        format!(
            "use planning/actions {{ mark }}\nuse planning/entities {{ Todo }}\nuse planning/events {{ TodoAdded }}\n\nsubscriber mark_added {{\n{body}}}\n"
        )
    };
    let cases = [
        (
            "    on: Todo\n    run: mark(event.todo_id)\n",
            "subscriber.event_invalid",
        ),
        ("    on: TodoAdded\n", "subscriber.item_invalid"),
        (
            "    on: TodoAdded\n    run: mark(event.todo_id)\n    when: TodoAdded\n",
            "subscriber.item_invalid",
        ),
        (
            "    on: TodoAdded\n    run: mark\n",
            "subscriber.run_invalid",
        ),
        (
            "    on: TodoAdded\n    run: mark(Todo.id)\n",
            "subscriber.argument_invalid",
        ),
        ("    on: TodoAdded\n    run: mark(event)\n", "type.mismatch"),
        (
            "    on: TodoAdded\n    run: mark(event.todo_id)\n    retry: {\n        attempts: 0\n    }\n",
            "subscriber.retry_invalid",
        ),
        (
            "    on: TodoAdded\n    run: mark(event.todo_id)\n    retry: {\n        within: 60d\n    }\n",
            "subscriber.retry_invalid",
        ),
    ];
    for (body, rule) in cases {
        let text = subscriber(body);
        let found = rules(&[(
            "domains/planning/subscribers/mark_added.jadpo",
            text.as_str(),
        )]);
        assert!(found.contains(&rule.to_string()), "{body}: {found:?}");
    }
    // A budget of its own.
    let text = subscriber(
        "    on: TodoAdded\n    run: mark(event.todo_id)\n    retry: {\n        attempts: 20\n        within: 3d\n    }\n",
    );
    let changed = [(
        "domains/planning/subscribers/mark_added.jadpo",
        text.as_str(),
    )];
    let found: Vec<String> = problems(&changed)
        .into_iter()
        .filter(|problem| !problem.contains("import.unused"))
        .collect();
    assert_eq!(found, Vec::<String>::new());
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files(&changed));
    let subscribers = jadpo_check::events::subscribers(&db, project);
    assert_eq!(
        subscribers[0].retry,
        jadpo_check::events::Retry {
            attempts: 20,
            within_ms: 3 * 24 * 60 * 60 * 1000
        }
    );
}

#[test]
fn a_subscriber_runs_only_its_own_domains_action() {
    let other = "public action tally() -> Unit {}\n";
    let subscriber = r#"use filing
use planning/events { TodoAdded }

subscriber mark_added {
    on: TodoAdded
    run: filing.tally()
}
"#;
    let found = rules(&[
        ("domains/filing/actions/tally.jadpo", other),
        ("domains/planning/subscribers/mark_added.jadpo", subscriber),
    ]);
    assert!(
        found.contains(&"subscriber.run_invalid".to_string()),
        "{found:?}"
    );
}

#[test]
fn a_subscriber_reaching_what_only_access_public_grants_is_refused() {
    let public = TODO.replace("update: [Access.system]", "update: [Access.public]");
    assert_eq!(
        problems(&[("domains/planning/entities/todo.jadpo", public.as_str())]),
        [
            "domains/planning/subscribers/mark_added.jadpo policy.system_grant_missing: `mark_added` runs as the system, which only `Access.public` lets update `Todo`"
        ]
    );
    // Naming `Access.system` beside it lets background work in on purpose.
    let both = TODO.replace(
        "update: [Access.system]",
        "update: [Access.public, Access.system]",
    );
    assert_eq!(
        problems(&[("domains/planning/entities/todo.jadpo", both.as_str())]),
        Vec::<String>::new()
    );
}

#[test]
fn an_event_naming_the_tenant_limits_its_subscribers_to_it() {
    let policy = "use planning/entities { Team }\n\npolicy {\n    tenant: Team\n}\n";
    let team = r#"use planning/entities { Person }

entity Team {
    id: Uuid
    owner_id: Person.id

    policy: {
        create: [Access.authenticated]
        read: [owner_id]
    }
}
"#;
    let todo = r#"use planning/entities { Team }

entity Todo {
    id: Uuid
    team_id: Team.id
    done: Bool

    policy: {
        create: [Access.authenticated]
        read: [Access.system]
        update: [Access.system]
    }
}
"#;
    let event = r#"use planning/entities { Team, Todo }

/// A todo was added.
type TodoAdded = Event {
    todo_id: Todo.id
    team_id: Team.id
}
"#;
    let add = r#"use planning/entities { Team, Todo }
use planning/events { TodoAdded }

action add(team_id: Team.id) -> Todo {
    var todo: Todo = Todo.create {
        set: {
            team_id: team_id
            done: false
        }
    }
    emit TodoAdded {
        todo_id: todo.id
        team_id: todo.team_id
    }
    return todo
}
"#;
    let person = PERSON.replace(
        "        read: [id]\n",
        "        read: [id]\n        tenant: Tenancy.shared\n",
    );
    let changed = [
        ("policy.jadpo", policy),
        ("domains/planning/entities/team.jadpo", team),
        ("domains/planning/entities/todo.jadpo", todo),
        ("domains/planning/entities/person.jadpo", person.as_str()),
        ("domains/planning/events/todo_added.jadpo", event),
        ("domains/planning/actions/add.jadpo", add),
    ];
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files(&changed));
    let subscribers = jadpo_check::events::subscribers(&db, project);
    assert_eq!(
        subscribers[0].tenant,
        jadpo_check::events::Tenant::Field("team_id".into())
    );
    // Without the tenant's id, the event limits its subscribers to none.
    let bare = "use planning/entities { Todo }\n\n/// A todo was added.\ntype TodoAdded = Event {\n    todo_id: Todo.id\n}\n";
    let add_bare = add.replace("        team_id: todo.team_id\n", "");
    let mut changed = changed.to_vec();
    changed[4] = ("domains/planning/events/todo_added.jadpo", bare);
    changed[5] = ("domains/planning/actions/add.jadpo", add_bare.as_str());
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files(&changed));
    let subscribers = jadpo_check::events::subscribers(&db, project);
    assert_eq!(subscribers[0].tenant, jadpo_check::events::Tenant::None);
}
