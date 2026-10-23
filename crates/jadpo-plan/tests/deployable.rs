//! A deployable's plan holds only what it reaches (area 14, "Declared
//! deployables"; roadmap chunk 8.1o3): the callables its routes, the
//! background work it runs and what the runtime names by key reach, with
//! only those callables' sites, the routes of the APIs it serves, the
//! schedules and subscribers it runs, the connections it reaches and the
//! settings they and its callables read. A subscriber another deployable
//! runs stays listed, so an event it emits is still left for it, but
//! without its action.

use std::collections::BTreeSet;

use jadpo_db::{Database, File, Layout, Project};
use jadpo_plan::Plan;
use jadpo_std::RuntimePlan;
use jadpo_std::subscriber::Target;

/// The café: the kiosk serves `main`, where a note is added, emitting
/// `NoteAdded`, and runs the subscriber that pins it up; the office
/// serves `staff`, keeping sheets on the disk, and runs the nightly tidy.
/// Each reads a setting of its own.
const CAFE: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "application {\n    apis: {\n        main: {\n            prefix: /\n        }\n        staff: {\n            prefix: /sheets\n        }\n    }\n    connections: {\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n        sheets: {\n            driver: Driver.disk\n            path: \"/var/lib/cafe/sheets\"\n        }\n    }\n    events: {\n        store: main\n    }\n    deployables: {\n        kiosk: {\n            serves: [main]\n            runs: [Trigger.subscribers]\n        }\n        office: {\n            serves: [staff]\n            runs: [Trigger.schedules]\n        }\n    }\n}\n",
    ),
    (
        "config.jadpo",
        "use cafe/types { NoteText }\n\nconfig {\n    database_url: PostgresUrl\n    greeting: NoteText {\n        default: \"Welcome\"\n    }\n    sheet_heading: NoteText {\n        default: \"Sheet\"\n    }\n}\n",
    ),
    (
        "policy.jadpo",
        "use cafe\n\npolicy {\n    actions: {\n        cafe.keep_sheet: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/cafe/types/note_text.jadpo",
        "type NoteText = Text {\n    max_length: 200\n}\n",
    ),
    (
        "domains/cafe/types/new_note.jadpo",
        "use cafe/types { NoteText }\n\ntype NewNote = Input {\n    text: NoteText\n}\n",
    ),
    (
        "domains/cafe/types/note_view.jadpo",
        "use cafe/entities { Note }\nuse cafe/types { NoteText }\n\ntype NoteView = Output {\n    id: Note.id\n    text: NoteText\n}\n",
    ),
    (
        "domains/cafe/types/sheet_view.jadpo",
        "use cafe/types { NoteText }\n\ntype SheetView = Output {\n    text: NoteText\n}\n",
    ),
    (
        "domains/cafe/entities/note.jadpo",
        "use cafe/types { NoteText }\n\nentity Note {\n    id: Uuid\n    text: NoteText\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public, Access.system]\n    }\n}\n",
    ),
    (
        "domains/cafe/failures/no_room.jadpo",
        "type NoRoom = Failure {\n    kind: FailureKind.unavailable\n}\n",
    ),
    (
        "domains/cafe/events/note_added.jadpo",
        "use cafe/types { NoteText }\n\n/// A note was added.\ntype NoteAdded = Event {\n    text: NoteText\n}\n",
    ),
    (
        "domains/cafe/actions/add_note.jadpo",
        "use cafe/entities { Note }\nuse cafe/events { NoteAdded }\nuse cafe/types { NewNote, NoteText, NoteView }\n\n/// The kiosk's greeting.\nfunction greeted() -> NoteText {\n    return config.greeting\n}\n\naction add_note(input: NewNote) -> NoteView {\n    var note: Note = Note.create {\n        set: {\n            text: input.text\n        }\n    }\n    emit NoteAdded {\n        text: input.text\n    }\n    return NoteView {\n        id: note.id\n        text: greeted()\n    }\n}\n",
    ),
    (
        "domains/cafe/actions/pin_note.jadpo",
        "use cafe/types { NoteText }\n\naction pin_note(pinned: NoteText) -> Unit {\n    assert pinned == pinned\n}\n",
    ),
    (
        "domains/cafe/actions/keep_sheet.jadpo",
        "use app { sheets }\nuse cafe/failures { NoRoom }\nuse cafe/types { NewNote, NoteText, SheetView }\n\n/// The sheet's heading.\nfunction headed() -> NoteText {\n    return config.sheet_heading\n}\n\naction keep_sheet(input: NewNote) fails NoRoom -> SheetView {\n    var sheet: File = attempt sheets.put(encoding.to_utf8(input.text), FileType.csv) {\n        storage_full: NoRoom\n    }\n    return SheetView {\n        text: headed()\n    }\n}\n",
    ),
    (
        "domains/cafe/actions/tidy.jadpo",
        "action tidy() -> Unit {\n    assert 1 == 1\n}\n",
    ),
    (
        "domains/cafe/subscribers/pin_it.jadpo",
        "use cafe/actions { pin_note }\nuse cafe/events { NoteAdded }\n\n/// Pins each note up.\nsubscriber pin_it {\n    on: NoteAdded\n    run: pin_note(event.text)\n}\n",
    ),
    (
        "domains/cafe/schedules/nightly.jadpo",
        "use cafe/actions { tidy }\n\n/// Tidies up each night.\nschedule nightly {\n    every: 1d\n    run: tidy()\n}\n",
    ),
    (
        "domains/cafe/queries/note_for.jadpo",
        "use cafe/entities { Note, NoteNotFound }\nuse cafe/types { NoteView }\n\nquery note_for(note_id: Note.id) fails NoteNotFound -> NoteView {\n    from: Note\n    where: Note.id == note_id\n    missing: NoteNotFound\n}\n",
    ),
    (
        "domains/cafe/routes/notes.jadpo",
        "use cafe/types { NewNote, NoteView }\nuse cafe/actions { add_note }\n\n/// The notes board.\nroutes Notes {\n    prefix: /notes\n\n    /// Adds a note.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: NewNote\n        output: NoteView\n        status: SuccessStatus.created\n        run: add_note(input)\n    }\n}\n",
    ),
    (
        "domains/cafe/routes/sheets.jadpo",
        "use cafe/entities { Note }\nuse cafe/types { NewNote, NoteView, SheetView }\nuse cafe/actions { keep_sheet }\nuse cafe/queries { note_for }\n\n/// The staff's sheets.\nroutes Sheets {\n    prefix: /sheets\n    api: staff\n\n    /// Keeps a sheet.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: NewNote\n        output: SheetView\n        status: SuccessStatus.created\n        run: keep_sheet(input)\n    }\n\n    /// A note, as the staff see it.\n    GET /{note_id: Note.id} {\n        authentication: RouteAuthentication.none\n        output: NoteView\n        run: note_for(note_id)\n    }\n}\n",
    ),
    (
        "domains/cafe/actions/add_note.test.jadpo",
        "use cafe/types { NewNote, NoteView }\n\ntest \"a note is added\" {\n    var added: NoteView = add_note(NewNote {\n        text: \"Hello\"\n    })\n    assert added.text == \"Welcome\"\n}\n",
    ),
];

/// The café's project, checked.
fn cafe(db: &Database) -> Project {
    let files: Vec<(String, String)> = CAFE
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect();
    checked(db, &files)
}

/// The project of `files`, checked: the errors that stop a run fail the
/// test.
fn checked(db: &Database, files: &[(String, String)]) -> Project {
    let inputs: Vec<File> = files
        .iter()
        .map(|(path, text)| File::new(db, path.clone(), text.clone()))
        .collect();
    let project = Project::new(
        db,
        inputs,
        Vec::new(),
        Layout::Project,
        Default::default(),
        Vec::new(),
    );
    // But for what these projects don't keep: migrations and records.
    let problems: Vec<String> = jadpo_check::check(db, project)
        .iter()
        .filter(|diagnostic| jadpo_check::blocks_running(diagnostic))
        .map(|diagnostic| format!("{}: {}", diagnostic.rule.id, diagnostic.message))
        .filter(|problem| {
            !problem.starts_with("migration.")
                && !problem.starts_with("record.")
                && !problem.starts_with("semantic.unused_variable")
        })
        .collect();
    assert!(problems.is_empty(), "{problems:#?}");
    project
}

/// The plan of the café's deployable `name`.
fn planned(db: &Database, project: Project, name: &str) -> Plan {
    let deployables = jadpo_check::deployables::deployables(db, project);
    let deployable = deployables.get(name).expect("declared");
    Plan::for_deployable(db, project, deployable).expect("everything it names is kept")
}

/// The keys of the callables a plan holds.
fn callables(plan: &Plan) -> BTreeSet<String> {
    plan.callables()
        .iter()
        .map(|callable| callable.key.as_str().to_string())
        .collect()
}

/// The paths of the routes of the app's own a plan serves.
fn paths(plan: &Plan) -> Vec<String> {
    plan.runtime()
        .routes
        .routes
        .iter()
        .filter(|route| route.file != "generated")
        .map(|route| format!("{} {}", route.method.word(), route.path()))
        .collect()
}

fn names<'a>(list: impl Iterator<Item = &'a str>) -> Vec<String> {
    list.map(str::to_string).collect()
}

#[test]
fn each_deployable_holds_only_the_callables_routes_and_work_it_reaches() {
    let db = Database::default();
    let project = cafe(&db);
    let kiosk = planned(&db, project, "kiosk");
    assert_eq!(
        callables(&kiosk),
        BTreeSet::from([
            "action add_note".to_string(),
            "action pin_note".to_string(),
            "function cafe.greeted".to_string(),
        ])
    );
    assert_eq!(paths(&kiosk), ["POST /notes"]);
    assert!(kiosk.queries().is_empty(), "{:?}", kiosk.queries());
    let runtime = kiosk.runtime();
    assert!(runtime.schedules.schedules.is_empty());
    assert_eq!(runtime.routes.apis.len(), 1, "{:?}", runtime.routes.apis);
    let [pin_it] = runtime.events.subscribers.as_slice() else {
        panic!("{:?}", runtime.events.subscribers)
    };
    assert!(matches!(pin_it.target, Target::Action { .. }), "{pin_it:?}");
    assert_eq!(
        names(runtime.json.events.iter().map(|event| event.key.as_str())),
        ["type cafe.NoteAdded"]
    );
    assert_eq!(
        names(
            runtime
                .config
                .fields
                .iter()
                .map(|field| field.name.as_str())
        ),
        ["database_url", "greeting"]
    );
    assert_eq!(
        names(
            runtime
                .connections
                .iter()
                .map(|connection| connection.name.as_str())
        ),
        ["main"]
    );
    assert_eq!(
        runtime.runs,
        jadpo_std::Runs {
            schedules: false,
            subscribers: true
        }
    );

    let office = planned(&db, project, "office");
    assert_eq!(
        callables(&office),
        BTreeSet::from([
            "action keep_sheet".to_string(),
            "action tidy".to_string(),
            "function cafe.headed".to_string(),
        ])
    );
    assert_eq!(paths(&office), ["POST /sheets", "GET /sheets/{note_id}"]);
    assert_eq!(
        names(office.queries().iter().map(|query| query.name.as_str())),
        ["note_for"]
    );
    let runtime = office.runtime();
    assert_eq!(
        names(
            runtime
                .schedules
                .schedules
                .iter()
                .map(|schedule| schedule.name.as_str())
        ),
        ["nightly"]
    );
    // The office runs no subscriber, but one it emitted for would still
    // be left for the kiosk: it is listed, without its action.
    let [pin_it] = runtime.events.subscribers.as_slice() else {
        panic!("{:?}", runtime.events.subscribers)
    };
    assert_eq!(pin_it.target, Target::Elsewhere, "{pin_it:?}");
    // It neither emits nor hears an event.
    assert!(runtime.json.events.is_empty(), "{:?}", runtime.json.events);
    assert_eq!(
        names(
            runtime
                .config
                .fields
                .iter()
                .map(|field| field.name.as_str())
        ),
        ["database_url", "sheet_heading"]
    );
    assert_eq!(
        names(
            runtime
                .connections
                .iter()
                .map(|connection| connection.name.as_str())
        ),
        ["main", "sheets"]
    );

    // Each reads back from its bytes.
    for plan in [&kiosk, &office] {
        let bytes = plan.runtime().encode_deployable().expect("encodes");
        let read = RuntimePlan::decode(&bytes).expect("reads back");
        assert_eq!(read.encode_deployable().expect("encodes"), bytes);
    }
}

#[test]
fn the_one_deployable_of_an_app_that_declares_none_leaves_out_only_its_tests() {
    let db = Database::default();
    let files: Vec<(String, String)> = CAFE
        .iter()
        .map(|(path, text)| {
            let text = if *path == "app.jadpo" {
                let at = text.find("    deployables:").expect("declared");
                format!("{}}}\n", &text[..at])
            } else {
                text.to_string()
            };
            (path.to_string(), text)
        })
        .collect();
    let project = checked(&db, &files);
    let app = planned(&db, project, "app");
    let whole = Plan::build(&db, project);
    let mut kept = callables(&whole);
    kept.retain(|key| !key.starts_with("test "));
    assert_eq!(callables(&app), kept);
    assert_eq!(paths(&app), paths(&whole));
    let subscribers = |plan: &Plan| {
        names(
            plan.runtime()
                .events
                .subscribers
                .iter()
                .map(|subscriber| subscriber.key.as_str()),
        )
    };
    assert_eq!(subscribers(&app), subscribers(&whole));
    let schedules = |plan: &Plan| {
        names(
            plan.runtime()
                .schedules
                .schedules
                .iter()
                .map(|schedule| schedule.key.as_str()),
        )
    };
    assert_eq!(schedules(&app), schedules(&whole));
    let settings = |plan: &Plan| {
        names(
            plan.runtime()
                .config
                .fields
                .iter()
                .map(|field| field.name.as_str()),
        )
    };
    assert_eq!(settings(&app), settings(&whole));
    assert_eq!(app.runtime().connections, whole.runtime().connections);
}

/// The café signing people in: browsers on `main`, with the kiosk's
/// session key and the site's origin, and robots on `staff`, with their
/// own key; a pinner serving nothing runs the subscribers.
fn signed_cafe(db: &Database) -> Project {
    configured_cafe(db, false)
}

fn configured_cafe(db: &Database, external: bool) -> Project {
    let mut files: Vec<(String, String)> = CAFE
        .iter()
        .map(|(path, text)| {
            let text = match *path {
                "app.jadpo" => "use cafe/entities { Member, Robot }\n\napplication {\n    revocation_delay: 5m\n    apis: {\n        main: {\n            prefix: /\n            authentication: [AuthenticationMethod.browser_session]\n        }\n        staff: {\n            prefix: /sheets\n            authentication: [AuthenticationMethod.service_key]\n        }\n    }\n    connections: {\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n        sheets: {\n            driver: Driver.disk\n            path: \"/var/lib/cafe/sheets\"\n        }\n    }\n    events: {\n        store: main\n    }\n    deployables: {\n        kiosk: {\n            serves: [main]\n        }\n        office: {\n            serves: [staff]\n            runs: [Trigger.schedules]\n        }\n        pinner: {\n            runs: [Trigger.subscribers]\n        }\n    }\n}\n\nprincipal {\n    user: {\n        subject: Member.handle\n    }\n    service: {\n        subject: Robot.name\n    }\n}\n".to_string(),
                "config.jadpo" => text.replace(
                    "    database_url: PostgresUrl\n",
                    "    database_url: PostgresUrl\n    session_key: SigningKey\n    robot_key: SigningKey\n    site_origin: Url\n",
                ),
                _ => text.to_string(),
            };
            (path.to_string(), text)
        })
        .collect();
    files.push((
        "authentication.jadpo".to_string(),
        "authentication {\n    browser_session: {\n        cookie: \"cafe_session\"\n        signing_key: config.session_key\n        origin: config.site_origin\n    }\n    service_key: {\n        signing_key: config.robot_key\n    }\n}\n".to_string(),
    ));
    files.push((
        "domains/cafe/types/handle.jadpo".to_string(),
        "type Handle = Text {\n    max_length: 40\n}\n".to_string(),
    ));
    files.push((
        "domains/cafe/entities/member.jadpo".to_string(),
        "use cafe/types { Handle }\n\nentity Member {\n    id: Uuid\n    handle: Handle {\n        unique: true\n    }\n\n    policy: {\n        read: [Access.system]\n    }\n}\n".to_string(),
    ));
    files.push((
        "domains/cafe/entities/robot.jadpo".to_string(),
        "use cafe/types { Handle }\n\nentity Robot {\n    id: Uuid\n    name: Handle {\n        unique: true\n    }\n\n    policy: {\n        read: [Access.system]\n    }\n}\n".to_string(),
    ));
    if external {
        for (path, text) in &mut files {
            if path == "app.jadpo" {
                *text = text.replace(
                    "AuthenticationMethod.browser_session",
                    "AuthenticationMethod.external_token",
                );
            } else if path == "config.jadpo" {
                *text = text.replacen(
                    "config {",
                    "config {\n    issuer: Url\n    audience: Text",
                    1,
                );
            } else if path == "authentication.jadpo" {
                *text = text.replacen("browser_session: {\n        cookie: \"cafe_session\"\n        signing_key: config.session_key\n        origin: config.site_origin\n    }", "external_token: {\n        issuer: config.issuer\n        audience: config.audience\n    }", 1);
            }
        }
    }
    checked(db, &files)
}

/// The settings a plan reads, by name.
fn settings_of(plan: &Plan) -> Vec<String> {
    names(
        plan.runtime()
            .config
            .fields
            .iter()
            .map(|field| field.name.as_str()),
    )
}

/// Sign-in's keys and origin are kept only where an API that takes them
/// is served (chunk 8.4c): the kiosk keeps the browser session's key and
/// the origin, the office the robots' key and the exchange, and the
/// pinner, serving nothing, neither, so it starts without them.
#[test]
fn sign_in_keys_and_origin_are_kept_only_where_an_api_takes_them() {
    let db = Database::default();
    let project = signed_cafe(&db);
    let methods = |plan: &Plan| -> Vec<&'static str> {
        plan.runtime()
            .routes
            .identity
            .as_ref()
            .map(|identity| {
                identity
                    .signing_keys
                    .iter()
                    .map(|(method, _)| method.name())
                    .collect()
            })
            .unwrap_or_default()
    };
    let origin = |plan: &Plan| {
        plan.runtime()
            .routes
            .identity
            .as_ref()
            .and_then(|identity| identity.origin.clone())
    };
    let generated = |plan: &Plan| -> Vec<String> {
        plan.runtime()
            .routes
            .routes
            .iter()
            .filter(|route| {
                route.file == "generated" && !matches!(route.run, jadpo_std::http::Run::OpenApi(_))
            })
            .map(|route| route.key.clone())
            .collect()
    };

    let kiosk = planned(&db, project, "kiosk");
    assert_eq!(methods(&kiosk), ["browser_session"]);
    assert_eq!(origin(&kiosk).as_deref(), Some("site_origin"));
    assert_eq!(
        settings_of(&kiosk),
        ["database_url", "session_key", "site_origin", "greeting"]
    );

    let office = planned(&db, project, "office");
    assert_eq!(methods(&office), ["service_key"]);
    assert_eq!(origin(&office), None);
    assert_eq!(
        settings_of(&office),
        ["database_url", "robot_key", "sheet_heading"]
    );
    assert_eq!(generated(&office), ["route POST /auth/exchange"]);

    let pinner = planned(&db, project, "pinner");
    assert_eq!(methods(&pinner), Vec::<&str>::new());
    assert_eq!(origin(&pinner), None);
    assert_eq!(settings_of(&pinner), ["database_url"]);
    assert_eq!(generated(&pinner), Vec::<String>::new());
    // It still knows who signs in, for the credentials an action issues
    // and the sessions every tick sweeps.
    assert!(pinner.runtime().routes.identity.is_some());
}

/// What a plan runs, besides its callables, by name: each route with
/// what it runs and whether it waits for follow-on work, each subscriber,
/// schedule, batch, purge, webhook, poll and queue, each email, and
/// whether it asks a model or seals records of several changing calls.
/// The event types it writes and reads are those it emits and hears.
fn runs(plan: &Plan) -> Vec<String> {
    let runtime = plan.runtime();
    let mut out: Vec<String> = Vec::new();
    for route in &runtime.routes.routes {
        let run = match &route.run {
            jadpo_std::http::Run::Action(_) => "action".to_string(),
            other => format!("{other:?}").chars().take(12).collect(),
        };
        out.push(format!(
            "route {} {} {run} settles {}",
            runtime.routes.apis[route.api as usize].name,
            route.key,
            route.settle.is_some()
        ));
    }
    for subscriber in &runtime.events.subscribers {
        out.push(format!("subscriber {}", subscriber.key));
    }
    for schedule in &runtime.schedules.schedules {
        out.push(format!("schedule {}", schedule.key));
    }
    for batch in &runtime.batches {
        out.push(format!("batch {}", batch.name));
    }
    for purge in &runtime.purges {
        out.push(format!("purge {}", purge.entity));
    }
    for webhook in &runtime.providers.webhooks {
        out.push(format!("webhook {}", webhook.connection));
    }
    for poll in &runtime.providers.polls {
        out.push(format!("poll {}", poll.name));
    }
    for queue in &runtime.providers.queues {
        out.push(format!("queue {}", queue.connection));
    }
    for email in &runtime.events.emails.emails {
        out.push(format!(
            "email {} {} {}",
            email.key,
            email.html.is_some(),
            email.text.is_some()
        ));
    }
    out.push(format!("ai {}", runtime.ai.is_some()));
    out.push(format!("steps {}", runtime.steps_key.is_some()));
    out
}

/// Every project of the specification corpus that checks, the pages' and
/// the teaching examples': each of its deployables planned from what it
/// reaches names nothing it doesn't hold, reads back from its bytes, and
/// for an app that declares none runs everything the whole plan runs, its
/// connections, settings and event types among the whole's.
#[test]
fn every_corpus_projects_deployables_hold_what_they_name() {
    let mut projects: Vec<(String, Database, Project)> = Vec::new();
    for (name, files) in crate::syntax_examples::projects() {
        if name.starts_with("examples ") {
            continue;
        }
        let db = Database::default();
        let mut sorted = files.clone();
        sorted.sort();
        let inputs: Vec<File> = sorted
            .iter()
            .map(|(path, text)| File::new(&db, path.clone(), text.clone()))
            .collect();
        let layout = if files.iter().any(|(path, _)| path == "app.jadpo") {
            Layout::Project
        } else {
            Layout::Loose
        };
        let project = Project::new(
            &db,
            inputs,
            Vec::new(),
            layout,
            Default::default(),
            Vec::new(),
        );
        projects.push((name, db, project));
    }
    let root = crate::syntax_examples::root().join("examples");
    let mut folders: Vec<_> = std::fs::read_dir(&root)
        .expect("examples/")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.join("app.jadpo").is_file())
        .collect();
    folders.sort();
    for folder in folders {
        let loaded = jadpo_db::load(&folder);
        projects.push((folder.display().to_string(), loaded.db, loaded.project));
    }
    let mut problems: Vec<String> = Vec::new();
    let mut planned_count = 0;
    for (name, db, project) in &projects {
        let blocked = jadpo_check::check(db, *project)
            .iter()
            .any(jadpo_check::blocks_running);
        if blocked {
            continue;
        }
        let whole = Plan::build(db, *project);
        let deployables = jadpo_check::deployables::deployables(db, *project);
        for deployable in &deployables.list {
            let plan = match Plan::for_deployable(db, *project, deployable) {
                Ok(plan) => plan,
                Err(unkept) => {
                    problems.push(format!("{name} {}: {unkept}", deployable.name));
                    continue;
                }
            };
            planned_count += 1;
            let bytes = plan.runtime().encode_deployable().expect("encodes");
            let read = RuntimePlan::decode(&bytes).expect("reads back");
            if read.encode_deployable().expect("encodes") != bytes {
                problems.push(format!("{name} {}: written again differs", deployable.name));
            }
            if deployables.declared {
                continue;
            }
            if runs(&plan) != runs(&whole) {
                problems.push(format!(
                    "{name}: runs {:#?}, the whole plan {:#?}",
                    runs(&plan),
                    runs(&whole)
                ));
            }
            // What it doesn't reach, an action only a test calls among
            // them, it leaves out, and nothing else.
            let connections = |plan: &Plan| -> BTreeSet<String> {
                plan.runtime()
                    .connections
                    .iter()
                    .map(|connection| connection.name.clone())
                    .collect()
            };
            if !connections(&plan).is_subset(&connections(&whole)) {
                problems.push(format!("{name}: connections the whole plan lacks"));
            }
            let settings = |plan: &Plan| -> BTreeSet<String> {
                plan.runtime()
                    .config
                    .fields
                    .iter()
                    .map(|field| field.name.clone())
                    .collect()
            };
            if !settings(&plan).is_subset(&settings(&whole)) {
                problems.push(format!("{name}: settings the whole plan lacks"));
            }
            let events = |plan: &Plan| -> BTreeSet<String> {
                plan.runtime()
                    .json
                    .events
                    .iter()
                    .map(|event| event.key.clone())
                    .collect()
            };
            if !events(&plan).is_subset(&events(&whole)) {
                problems.push(format!("{name}: events the whole plan lacks"));
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
    assert!(planned_count > 150, "the corpus was read: {planned_count}");
}

#[test]
fn external_issuer_and_audience_are_kept_only_where_the_method_is_served() {
    let db = Database::default();
    let project = configured_cafe(&db, true);
    let kiosk = planned(&db, project, "kiosk");
    assert!(
        kiosk
            .runtime()
            .routes
            .identity
            .as_ref()
            .expect("identity")
            .external_token
            .is_some()
    );
    assert_eq!(
        settings_of(&kiosk),
        ["issuer", "audience", "database_url", "greeting"]
    );
    for name in ["office", "pinner"] {
        let plan = planned(&db, project, name);
        assert!(
            plan.runtime()
                .routes
                .identity
                .as_ref()
                .is_none_or(|identity| identity.external_token.is_none())
        );
        assert!(
            !settings_of(&plan)
                .iter()
                .any(|name| name == "issuer" || name == "audience")
        );
    }
}
