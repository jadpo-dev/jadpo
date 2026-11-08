//! Area 12's events and subscribers in the checker (chunk 6.1b): a
//! subscriber names one event and runs an action of its own domain with
//! the event or its fields; `emit` is an action's, of an event; an action
//! that emits keeps its events in the store it writes, or the one
//! `events: { store: }` names; and a subscriber, which runs as the
//! system, reaching anything whose list doesn't name `Access.system` is
//! refused (area 9, decided 2026-10-19 and chunks A22 and A23).

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

/// What the run does to `Todo`, with `Todo` granting it through no list:
/// the delivery could only ever fail, so it fails at check (chunk A22).
#[test]
fn a_subscriber_reaching_what_no_list_grants_is_refused() {
    let run = |effect: &str, body: &str, fails: &str| {
        let todo = TODO.replace(&format!("        {effect}: [Access.system]\n"), "");
        let todo = if effect == "create" {
            todo.replace("        create: [Access.authenticated]\n", "")
        } else if effect == "read" {
            todo.replace("        read: [Access.authenticated, Access.system]\n", "")
        } else {
            todo
        };
        let mark = format!(
            "use planning/entities {{ Todo{fails} }}\n\naction mark(todo_id: Todo.id){} -> Unit {{\n{body}\n}}\n",
            if fails.is_empty() {
                ""
            } else {
                " fails TodoNotFound"
            }
        );
        problems(&[
            ("domains/planning/entities/todo.jadpo", todo.as_str()),
            ("domains/planning/actions/mark.jadpo", mark.as_str()),
        ])
    };
    let subscriber = "domains/planning/subscribers/mark_added.jadpo policy.system_grant_missing";
    assert_eq!(
        run(
            "update",
            "    attempt Todo.update(todo_id) {\n        set: {\n            done: true\n        }\n    }",
            ", TodoNotFound",
        ),
        [format!(
            "{subscriber}: `mark_added` runs as the system, which nothing lets update `Todo`"
        )]
    );
    assert_eq!(
        run(
            "delete",
            "    attempt Todo.delete(todo_id)",
            ", TodoNotFound",
        ),
        [format!(
            "{subscriber}: `mark_added` runs as the system, which nothing lets delete `Todo`"
        )]
    );
    assert_eq!(
        run(
            "read",
            "    var found: Todo = attempt Todo.get(todo_id)\n    assert found.done == true",
            ", TodoNotFound",
        ),
        [format!(
            "{subscriber}: `mark_added` runs as the system, which nothing lets read `Todo`"
        )]
    );
}

/// The report this came from: a record created, through an action the
/// subscriber's own action calls, on an entity with no `create:`.
#[test]
fn a_subscribers_create_with_no_list_is_refused_however_deep() {
    let line = "/// A line of the log.\nentity Line {\n    id: Uuid\n    done: Bool\n\n    policy: {\n        read: [Access.system]\n    }\n}\n";
    let write = "use planning/entities { Line }\n\naction write_line() -> Line {\n    return Line.create {\n        set: {\n            done: true\n        }\n    }\n}\n";
    let mark = "use planning/actions { write_line }\nuse planning/entities { Line, Todo }\n\naction mark(todo_id: Todo.id) -> Line {\n    return write_line()\n}\n";
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Project,
        &files(&[
            ("domains/planning/entities/line.jadpo", line),
            ("domains/planning/actions/write_line.jadpo", write),
            ("domains/planning/actions/mark.jadpo", mark),
        ]),
    );
    let found = jadpo_check::check(&db, project);
    let refused: Vec<_> = found
        .iter()
        .filter(|d| d.rule.id == "policy.system_grant_missing")
        .collect();
    let [refused] = refused.as_slice() else {
        panic!("one refusal: {found:?}");
    };
    assert_eq!(
        refused.message,
        "`mark_added` runs as the system, which nothing lets create `Line`"
    );
    assert_eq!(
        refused.reason.as_deref(),
        Some(
            "A subscriber runs as the system, which an entity grants background work by naming `Access.system`; `Line`'s policy grants nobody `create`, so every run would be refused with `not_permitted`."
        )
    );
    assert_eq!(
        refused.next.as_ref().map(|next| next.text.as_str()),
        Some("Add `create: [Access.system]` to `Line`'s `policy:`")
    );
}

/// What a list naming only people comes to: refused on what the code could
/// reach, whichever branch runs, as a route anyone may call is.
#[test]
fn a_subscriber_reaching_a_list_naming_only_people_is_refused() {
    let subscriber = "domains/planning/subscribers/mark_added.jadpo policy.system_grant_missing";
    let refused = |who: &str| {
        format!(
            "{subscriber}: `mark_added` runs as the system, which `Todo`'s `{who}:` doesn't let in"
        )
    };
    // Anyone signed in, and the record's own field, for an update.
    let signed = TODO.replace("update: [Access.system]", "update: [Access.authenticated]");
    assert_eq!(
        problems(&[("domains/planning/entities/todo.jadpo", signed.as_str())]),
        [refused("update")]
    );
    let owned = "use planning/entities { Person }\n\nentity Todo {\n    id: Uuid\n    owner_id: Person.id? {\n        on_delete: OnDelete.cleared\n    }\n    done: Bool\n\n    policy: {\n        create: [Access.authenticated]\n        read: [owner_id, Access.system]\n        update: [owner_id]\n    }\n}\n";
    let add = ADD.replace(
        "            done: false\n",
        "            owner_id: none\n            done: false\n",
    );
    assert_eq!(
        problems(&[
            ("domains/planning/entities/todo.jadpo", owned),
            ("domains/planning/actions/add.jadpo", add.as_str()),
        ]),
        [refused("update")]
    );
    // A read is reached the same way, through what the action calls too.
    let read = "use planning/entities { Todo, TodoNotFound }\n\naction peek(todo_id: Todo.id) fails TodoNotFound -> Todo {\n    return attempt Todo.get(todo_id)\n}\n";
    let mark = "use planning/actions { peek }\nuse planning/entities { Todo, TodoNotFound }\n\naction mark(todo_id: Todo.id) fails TodoNotFound -> Unit {\n    var found: Todo = attempt peek(todo_id)\n    assert found.done == false\n}\n";
    let people = TODO.replace(
        "read: [Access.authenticated, Access.system]",
        "read: [Access.authenticated]",
    );
    assert_eq!(
        problems(&[
            ("domains/planning/entities/todo.jadpo", people.as_str()),
            ("domains/planning/actions/peek.jadpo", read),
            ("domains/planning/actions/mark.jadpo", mark),
        ]),
        [refused("read")]
    );
}

/// The action a route and a subscriber share, the people's write on a branch
/// the subscriber never takes, is refused, and the error says to split it.
const SHARED: &str = r#"use planning/entities { Todo, TodoNotFound }

action mark(todo_id: Todo.id) fails TodoNotFound -> Unit {
    var found: Todo = attempt Todo.get(todo_id)
    if (found.done) {
        attempt Todo.delete(todo_id)
    } else {
        attempt Todo.update(todo_id) {
            set: {
                done: true
            }
        }
    }
}
"#;

#[test]
fn an_action_a_route_shares_is_refused_and_told_to_split() {
    let todo = TODO.replace(
        "update: [Access.system]\n",
        "update: [Access.system]\n        delete: [Access.authenticated]\n",
    );
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Project,
        &files(&[
            ("domains/planning/entities/todo.jadpo", todo.as_str()),
            ("domains/planning/actions/mark.jadpo", SHARED),
        ]),
    );
    let found = jadpo_check::check(&db, project);
    let refused: Vec<_> = found
        .iter()
        .filter(|d| d.rule.id == "policy.system_grant_missing")
        .collect();
    let [refused] = refused.as_slice() else {
        panic!("one refusal: {found:?}");
    };
    assert_eq!(
        refused.message,
        "`mark_added` runs as the system, which `Todo`'s `delete:` doesn't let in"
    );
    assert_eq!(
        refused.reason.as_deref(),
        Some(
            "A subscriber runs as the system, which an entity grants background work by naming `Access.system`; `Todo`'s `delete:` names only people, and what the code could reach counts, whichever branch runs."
        )
    );
    assert_eq!(
        refused.next.as_ref().map(|next| next.text.as_str()),
        Some(
            "Add `Access.system` to `Todo`'s `delete:`, or split `mark` so the system's work is an action of its own that doesn't delete `Todo`"
        )
    );
}

/// An action's line, listed or reached however deep, and a listed query's,
/// which can't name the system at all.
#[test]
fn a_subscriber_reaching_a_line_naming_only_people_is_refused() {
    let subscriber = "domains/planning/subscribers/mark_added.jadpo policy.system_grant_missing";
    // A line deciding the action's own write.
    let listed = "use planning\n\npolicy {\n    actions: {\n        planning.mark: [Access.authenticated]\n    }\n}\n";
    assert_eq!(
        problems(&[("policy.jadpo", listed)]),
        [format!(
            "{subscriber}: `mark_added` runs as the system, which `planning.mark`'s line under `actions:` doesn't let in"
        )]
    );
    // A line on an action that writes nothing, called by the one it runs.
    let quiet = "use planning/entities { Todo }\n\naction quiet(todo_id: Todo.id) -> Unit {\n    assert todo_id == todo_id\n}\n";
    let mark = "use planning/actions { quiet }\nuse planning/entities { Todo }\n\naction mark(todo_id: Todo.id) -> Unit {\n    quiet(todo_id)\n}\n";
    let listed = "use planning\n\npolicy {\n    actions: {\n        planning.quiet: [Access.authenticated]\n    }\n}\n";
    assert_eq!(
        problems(&[
            ("domains/planning/actions/quiet.jadpo", quiet),
            ("domains/planning/actions/mark.jadpo", mark),
            ("policy.jadpo", listed),
        ]),
        [format!(
            "{subscriber}: `mark_added` runs as the system, which `planning.quiet`'s line under `actions:` doesn't let in"
        )]
    );
    // A listed query, whose line can't name `Access.system`.
    let open = "use planning/entities { Todo }\n\nquery open_todos() -> List<Todo> {\n    from: Todo\n    order: [Todo.id asc]\n    limit: 10\n}\n";
    let mark = "use planning/entities { Todo }\nuse planning/queries { open_todos }\n\naction mark(todo_id: Todo.id) -> Unit {\n    var open: List<Todo> = open_todos()\n    assert open == open\n}\n";
    let listed = "use planning\n\npolicy {\n    actions: {\n        planning.open_todos: [Access.authenticated]\n    }\n}\n";
    assert_eq!(
        problems(&[
            ("domains/planning/queries/open_todos.jadpo", open),
            ("domains/planning/actions/mark.jadpo", mark),
            ("policy.jadpo", listed),
        ]),
        [format!(
            "{subscriber}: `mark_added` runs as the system, which `planning.open_todos`'s line under `actions:` can't let in"
        )]
    );
}

/// A field's own list is reached as the record's is; one naming the system
/// beside people lets it in.
#[test]
fn a_subscriber_setting_a_field_whose_list_names_only_people_is_refused() {
    let people = TODO.replace(
        "    done: Bool\n",
        "    done: Bool {\n        policy: {\n            update: [Access.authenticated]\n        }\n    }\n",
    );
    let people = people.replace(
        "update: [Access.system]",
        "update: [Access.authenticated, Access.system]",
    );
    assert_eq!(
        problems(&[("domains/planning/entities/todo.jadpo", people.as_str())]),
        [
            "domains/planning/subscribers/mark_added.jadpo policy.system_grant_missing: `mark_added` runs as the system, which `Todo.done`'s own `update:` doesn't let in"
        ]
    );
    let both = people.replace(
        "            update: [Access.authenticated]\n",
        "            update: [Access.authenticated, Access.system]\n",
    );
    assert_eq!(
        problems(&[("domains/planning/entities/todo.jadpo", both.as_str())]),
        Vec::<String>::new()
    );
}

/// `FieldAccess.server_only` keeps every request out and leaves the system's
/// writes to the record's own lists (area 12, "Runs as the system").
#[test]
fn a_subscriber_may_set_a_server_only_field() {
    let server_only = TODO.replace(
        "    done: Bool\n",
        "    done: Bool {\n        policy: FieldAccess.server_only\n    }\n",
    );
    assert_eq!(
        problems(&[("domains/planning/entities/todo.jadpo", server_only.as_str())])
            .iter()
            .filter(|found| found.contains("policy.system_grant_missing"))
            .count(),
        0
    );
    // The record's own list still decides.
    let record = server_only.replace("update: [Access.system]", "update: [Access.authenticated]");
    assert_eq!(
        problems(&[("domains/planning/entities/todo.jadpo", record.as_str())])
            .iter()
            .filter(|found| found.contains("which `Todo`'s `update:` doesn't let in"))
            .count(),
        1
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

/// A household application whose `log_chore` subscriber copies the title
/// of each chore `remind` emits into a digest of its household (chunk
/// 6.1c).
fn household(add: &str) -> Vec<(&str, &str)> {
    vec![
        ("app.jadpo", APP),
        (
            "authentication.jadpo",
            "use home/entities { Person }\n\nprincipal {\n    user: {\n        subject: Person.handle\n    }\n}\n\nauthentication {\n    browser_session: {\n        cookie: \"home_session\"\n    }\n}\n",
        ),
        (
            "policy.jadpo",
            "use home/entities { Household }\n\npolicy {\n    tenant: Household\n}\n",
        ),
        (
            "domains/home/types/home_role.jadpo",
            "type HomeRole = Enum {\n    member\n}\n",
        ),
        (
            "domains/home/types/title.jadpo",
            "type Title = Text {\n    max_length: 80\n}\n",
        ),
        (
            "domains/home/types/handle.jadpo",
            "type Handle = Text {\n    min_length: 3\n    max_length: 40\n}\n",
        ),
        (
            "domains/home/entities/person.jadpo",
            "use home/types { Handle }\n\nentity Person {\n    id: Uuid\n    handle: Handle {\n        unique: true\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [id]\n        tenant: Tenancy.shared\n    }\n}\n",
        ),
        (
            "domains/home/entities/household.jadpo",
            "use home/types { HomeRole, Title }\n\nentity Household {\n    id: Uuid\n    name: Title\n\n    policy: {\n        read: [HomeRole.member]\n    }\n}\n",
        ),
        (
            "domains/home/entities/home_membership.jadpo",
            "use home/entities { Household, Person }\nuse home/types { HomeRole }\n\nentity HomeMembership {\n    id: Uuid\n    household_id: Household.id {\n        on_delete: OnDelete.cascade\n    }\n    person_id: Person.id {\n        on_delete: OnDelete.cascade\n    }\n    role: HomeRole\n\n    membership: {\n        scope: household_id\n        member: person_id\n        role: role\n    }\n\n    policy: {\n        create: [HomeRole.member]\n        read: [person_id]\n    }\n}\n",
        ),
        (
            "domains/home/entities/chore.jadpo",
            "use home/entities { Household }\nuse home/types { HomeRole, Title }\n\nentity Chore {\n    id: Uuid\n    household_id: Household.id {\n        on_delete: OnDelete.cascade\n    }\n    title: Title\n    reminded: Bool\n\n    policy: {\n        create: [HomeRole.member]\n        read: [HomeRole.member]\n        update: [HomeRole.member]\n    }\n}\n",
        ),
        (
            "domains/home/entities/digest_line.jadpo",
            "use home/entities { Household }\nuse home/types { HomeRole, Title }\n\nentity DigestLine {\n    id: Uuid\n    household_id: Household.id {\n        on_delete: OnDelete.cascade\n    }\n    line: Title\n\n    policy: {\n        create: [Access.system]\n        read: [HomeRole.member]\n    }\n}\n",
        ),
        (
            "domains/home/events/chore_added.jadpo",
            "use home/entities { Chore, Household }\nuse home/types { Title }\n\n/// A chore was added to a household.\ntype ChoreAdded = Event {\n    chore_id: Chore.id\n    household_id: Household.id\n    title: Title\n}\n",
        ),
        ("domains/home/actions/remind.jadpo", add),
        (
            "domains/home/actions/log.jadpo",
            "use home/entities { DigestLine, Household, HouseholdNotFound }\nuse home/types { Title }\n\naction log(household_id: Household.id, title: Title) fails HouseholdNotFound -> Unit {\n    var line: DigestLine = attempt DigestLine.create {\n        set: {\n            household_id: household_id\n            line: title\n        }\n    }\n    assert line.line == title\n}\n",
        ),
        (
            "domains/home/subscribers/log_chore.jadpo",
            "use home/actions { log }\nuse home/events { ChoreAdded }\n\nsubscriber log_chore {\n    on: ChoreAdded\n    run: log(event.household_id, event.title)\n}\n",
        ),
    ]
}

const ADD_CHORE: &str = r#"use home/entities { Chore, ChoreNotFound, Household }
use home/events { ChoreAdded }

action remind(household_id: Household.id, chore_id: Chore.id) fails ChoreNotFound -> Unit {
    var chore: Chore = attempt Chore.update(chore_id) {
        set: {
            reminded: true
        }
    }
    emit ChoreAdded {
        chore_id: chore.id
        household_id: chore.household_id
        title: chore.title
    }
}
"#;

#[test]
fn a_subscriber_stores_what_its_event_carries_where_its_tenants_readers_may_read_it() {
    // The chore's title, read by the household's members, goes into a
    // record of the same household, read by its members: the event's
    // tenant field came from the chore itself, and the subscriber is
    // limited to that tenant.
    assert_eq!(
        support::problems(Layout::Project, &household(ADD_CHORE)),
        Vec::<String>::new()
    );
    // With the tenant the request named rather than the chore's, nothing
    // says the title is of the household the subscriber is limited to,
    // and the event names a household the action never reached (chunk
    // 6.6c).
    let found = ADD_CHORE.replace(
        "        household_id: chore.household_id\n",
        "        household_id: household_id\n",
    );
    assert_eq!(
        support::problems(Layout::Project, &household(&found)),
        [
            "domains/home/actions/remind.jadpo policy.event_tenant_unproved: `remind` sends `ChoreAdded` to a `Household` it never reached",
            "domains/home/subscribers/log_chore.jadpo policy.output_field_unproved: `log_chore` passes `Chore.title` to `log`, which stores it in `DigestLine.line`, though not everyone who may read `line` may read it"
        ]
    );
}

// Every action a subscriber reaches is checked against its own line
// under `actions:`, and a listed action's line decides its own body's
// writes, as the plan does (chunk 6.6b).

/// `settle` runs `mark` for the subscriber.
const SETTLE: &str = r#"use planning/actions { mark }
use planning/entities { Todo, TodoNotFound }

action settle(todo_id: Todo.id) fails TodoNotFound -> Unit {
    attempt mark(todo_id)
}
"#;

const SETTLE_SUBSCRIBER: &str = r#"use planning/actions { settle }
use planning/events { TodoAdded }

/// Settles each new todo.
subscriber mark_added {
    on: TodoAdded
    run: settle(event.todo_id)
}
"#;

/// `mark` listed under `actions:` with `line`.
fn listed(line: &str) -> String {
    format!(
        "use planning\n\npolicy {{\n    actions: {{\n        planning.mark: [{line}]\n    }}\n}}\n"
    )
}

#[test]
fn a_called_actions_line_naming_only_access_public_is_refused() {
    let nested = [
        ("domains/planning/actions/settle.jadpo", SETTLE),
        (
            "domains/planning/subscribers/mark_added.jadpo",
            SETTLE_SUBSCRIBER,
        ),
    ];
    assert_eq!(problems(&nested), Vec::<String>::new());
    // `mark`'s line lets anyone at all in, the system among them, though
    // `Todo` grants background work.
    let public = listed("Access.public");
    let mut with = nested.to_vec();
    with.push(("policy.jadpo", public.as_str()));
    assert_eq!(
        problems(&with),
        [
            "domains/planning/subscribers/mark_added.jadpo policy.system_grant_missing: `mark_added` runs as the system, which only `Access.public` lets call `planning.mark`"
        ]
    );
    // The subscriber's own action, listed the same way.
    let direct = [("policy.jadpo", public.as_str())];
    assert_eq!(
        problems(&direct),
        [
            "domains/planning/subscribers/mark_added.jadpo policy.system_grant_missing: `mark_added` runs as the system, which only `Access.public` lets call `planning.mark`"
        ]
    );
}

#[test]
fn a_called_actions_line_decides_its_own_writes() {
    // `Todo` lets anyone at all update it, but `mark`'s line, which
    // decides what `mark` writes, names the system.
    let todo = TODO.replace("update: [Access.system]", "update: [Access.public]");
    let line = listed("Access.authenticated, Access.system");
    let files = [
        ("domains/planning/entities/todo.jadpo", todo.as_str()),
        ("domains/planning/actions/settle.jadpo", SETTLE),
        (
            "domains/planning/subscribers/mark_added.jadpo",
            SETTLE_SUBSCRIBER,
        ),
        ("policy.jadpo", line.as_str()),
    ];
    assert_eq!(problems(&files), Vec::<String>::new());
    // Without the line, `Todo`'s own `update:` decides.
    assert_eq!(
        problems(&files[..3]),
        [
            "domains/planning/subscribers/mark_added.jadpo policy.system_grant_missing: `mark_added` runs as the system, which only `Access.public` lets update `Todo`"
        ]
    );
}
