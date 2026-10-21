//! Declared deployables (area 14, question 14; area 11, question 24;
//! roadmap chunk 8.1o2): `application { deployables: { } }` names each
//! program the app is built into, what it serves and runs and its
//! target; an app that declares none has one, `app`, with no target. A
//! deployable that declares a target is refused each connection it
//! reaches whose driver that target can't run, naming the alternative.

use crate::support;

use jadpo_db::{Database, Layout};
use jadpo_std::{DeployTarget, Runs};

/// The café's two APIs, its database and a disk for sheets, and
/// `deployables`, written inside `application { }`.
fn app(deployables: &str, more: &str) -> String {
    format!(
        "application {{\n    apis: {{\n        main: {{\n            prefix: /\n        }}\n        staff: {{\n            prefix: /sheets\n        }}\n    }}\n    connections: {{\n        main: {{\n            driver: Driver.postgres\n        }}\n        sheets: {{\n            driver: Driver.disk\n            path: \"/var/lib/cafe/sheets\"\n        }}\n{more}    }}\n    events: {{\n        store: main\n    }}\n{deployables}}}\n"
    )
}

/// `main` serves notes, which emit `NoteAdded`, heard by a subscriber;
/// `staff` keeps sheets on the disk; a schedule tidies up nightly.
const CAFE: &[(&str, &str)] = &[
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
        "use cafe/entities { Note }\nuse cafe/events { NoteAdded }\nuse cafe/types { NewNote, NoteView }\n\naction add_note(input: NewNote) -> NoteView {\n    var note: Note = Note.create {\n        set: {\n            text: input.text\n        }\n    }\n    emit NoteAdded {\n        text: input.text\n    }\n    return NoteView {\n        id: note.id\n        text: note.text\n    }\n}\n",
    ),
    (
        "domains/cafe/actions/pin_note.jadpo",
        "use cafe/types { NoteText }\n\naction pin_note(pinned: NoteText) -> Unit {\n    assert pinned == pinned\n}\n",
    ),
    (
        "domains/cafe/actions/keep_sheet.jadpo",
        "use app { sheets }\nuse cafe/failures { NoRoom }\nuse cafe/types { NewNote, SheetView }\n\naction keep_sheet(input: NewNote) fails NoRoom -> SheetView {\n    var sheet: File = attempt sheets.put(encoding.to_utf8(input.text), FileType.csv) {\n        storage_full: NoRoom\n    }\n    return SheetView {\n        text: input.text\n    }\n}\n",
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
    ("domains/cafe/routes/notes.jadpo", NOTES),
    (
        "domains/cafe/routes/sheets.jadpo",
        "use cafe/types { NewNote, SheetView }\nuse cafe/actions { keep_sheet }\n\n/// The staff's sheets.\nroutes Sheets {\n    prefix: /sheets\n    api: staff\n\n    /// Keeps a sheet.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: NewNote\n        output: SheetView\n        status: SuccessStatus.created\n        run: keep_sheet(input)\n    }\n}\n",
    ),
];

/// The notes board, its route answering at once.
const NOTES: &str = "use cafe/types { NewNote, NoteView }\nuse cafe/actions { add_note }\n\n/// The notes board.\nroutes Notes {\n    prefix: /notes\n\n    /// Adds a note.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: NewNote\n        output: NoteView\n        status: SuccessStatus.created\n        run: add_note(input)\n    }\n}\n";

/// The kiosk serves `main` and runs the subscribers; the office serves
/// `staff` and runs the schedules.
fn declared(kiosk: &str, office: &str) -> String {
    format!(
        "    deployables: {{\n        kiosk: {{\n            serves: [main]\n            runs: [Trigger.subscribers]\n{kiosk}        }}\n        office: {{\n            serves: [staff]\n            runs: [Trigger.schedules]\n{office}        }}\n    }}\n"
    )
}

const NATIVE: &str = "            target: Target.native\n";
const WASM: &str = "            target: Target.wasm\n";

/// The café's problems with `app.jadpo` as `application`, but for the
/// unused imports and declarations, migrations and records these
/// projects don't keep, and a variable the sheet's action doesn't read.
fn problems_with(application: &str, more: &[(&str, &str)]) -> Vec<String> {
    let mut files: Vec<(&str, &str)> = vec![("app.jadpo", application)];
    files.extend_from_slice(CAFE);
    // A file given again replaces the café's.
    files.retain(|(path, _)| !more.iter().any(|(again, _)| again == path));
    files.extend_from_slice(more);
    let mut found: Vec<String> = support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| {
            ![
                "import.unused",
                "layout.unused_declaration",
                "semantic.unused_variable",
            ]
            .iter()
            .any(|rule| problem.contains(&format!(" {rule}:")))
                && !problem.contains(" migration.")
                && !problem.contains(" record.")
        })
        .collect();
    found.sort();
    found
}

fn problems(deployables: &str) -> Vec<String> {
    problems_with(&app(deployables, ""), &[])
}

const CLEAN: &[&str] = &[];

/// The deployables the café declares with `deployables`.
fn read(deployables: &str) -> jadpo_check::deployables::Deployables {
    let db = Database::default();
    let application = app(deployables, "");
    let mut files: Vec<(&str, &str)> = vec![("app.jadpo", application.as_str())];
    files.extend_from_slice(CAFE);
    let project = support::project(&db, Layout::Project, &files);
    jadpo_check::deployables::deployables(&db, project).clone()
}

/// The connections each of the café's deployables reaches, by name.
fn reached(deployables: &str) -> Vec<(String, Vec<String>)> {
    let db = Database::default();
    let application = app(deployables, "");
    let mut files: Vec<(&str, &str)> = vec![("app.jadpo", application.as_str())];
    files.extend_from_slice(CAFE);
    let project = support::project(&db, Layout::Project, &files);
    let list = &jadpo_check::deployables::deployables(&db, project).list;
    list.iter()
        .zip(jadpo_check::deployables::reaches(&db, project))
        .map(|(deployable, reached)| {
            (
                deployable.name.clone(),
                reached.connections.keys().cloned().collect(),
            )
        })
        .collect()
}

#[test]
fn an_app_that_declares_none_has_one_deployable_with_no_target() {
    assert_eq!(problems(""), CLEAN);
    let read = read("");
    assert!(!read.declared);
    let [app] = read.list.as_slice() else {
        panic!("{read:?}")
    };
    assert_eq!(app.name, "app");
    assert_eq!(app.serves, ["main", "staff"]);
    assert_eq!(app.runs, Runs::EVERYTHING);
    assert_eq!(app.target, None);
    assert_eq!(app.builds_for(), DeployTarget::Native);
    // It reaches the disk, but with no target nothing is refused.
    assert_eq!(
        reached(""),
        [("app".to_string(), vec!["sheets".to_string()])]
    );
}

#[test]
fn each_declared_deployable_serves_and_runs_what_it_says() {
    let deployables = declared(NATIVE, NATIVE);
    assert_eq!(problems(&deployables), CLEAN);
    let read = read(&deployables);
    assert!(read.declared);
    assert_eq!(read.names(), ["kiosk", "office"]);
    let kiosk = read.get("kiosk").expect("kiosk");
    assert_eq!(kiosk.serves, ["main"]);
    assert_eq!(
        kiosk.runs,
        Runs {
            schedules: false,
            subscribers: true
        }
    );
    assert_eq!(
        kiosk.target.as_ref().map(|(target, _)| *target),
        Some(DeployTarget::Native)
    );
    // Only the office reaches the disk, through the staff's route.
    assert_eq!(
        reached(&deployables),
        [
            ("kiosk".to_string(), Vec::<String>::new()),
            ("office".to_string(), vec!["sheets".to_string()])
        ]
    );
}

#[test]
fn a_target_is_checked_only_against_what_its_deployable_reaches() {
    // The kiosk doesn't reach the disk, so it can be built for Workers.
    assert_eq!(problems(&declared(WASM, NATIVE)), CLEAN);
    assert_eq!(
        problems(&declared(NATIVE, WASM)),
        [
            "app.jadpo deployable.target_unsupported: `office` can't use `sheets` on Workers, which has no disk"
        ]
    );
    // Without a target, nothing is held to one.
    assert_eq!(problems(&declared("", "")), CLEAN);
}

#[test]
fn what_a_deployable_lists_is_checked() {
    let one = |kiosk: &str| {
        format!(
            "    deployables: {{\n        kiosk: {{\n{kiosk}        }}\n        office: {{\n            serves: [staff]\n            runs: [Trigger.schedules, Trigger.subscribers]\n        }}\n    }}\n"
        )
    };
    let cases: [(&str, &[&str]); 9] = [
        (
            "            serves: [mian]\n",
            &["app.jadpo deployable.api_unknown: `mian` isn't one of the app's APIs"],
        ),
        (
            "            serves: [main]\n            runs: [Trigger.jobs]\n",
            &[
                "app.jadpo deployable.trigger_unknown: `Trigger.jobs` isn't background work a deployable runs",
            ],
        ),
        (
            "            serves: [main]\n            target: Target.workers\n",
            &["app.jadpo deployable.target_unknown: `Target.workers` isn't a target"],
        ),
        (
            "            serves: [main, main]\n",
            &["app.jadpo deployable.invalid: `kiosk`'s `serves:` lists `main` twice"],
        ),
        (
            "            serves: main\n",
            &["app.jadpo deployable.invalid: `kiosk`'s `serves:` isn't a list"],
        ),
        (
            "            serves: [main]\n            port: 8080\n",
            &["app.jadpo deployable.invalid: `port` isn't a setting of a deployable"],
        ),
        (
            "            serve: [main]\n",
            &["app.jadpo deployable.invalid: `serve` isn't a setting of a deployable"],
        ),
        (
            "",
            &["app.jadpo deployable.empty: `kiosk` serves nothing and runs nothing"],
        ),
        ("            serves: [main]\n", &[]),
    ];
    for (kiosk, expected) in cases {
        assert_eq!(problems(&one(kiosk)), expected, "{kiosk}");
    }
    assert_eq!(
        problems("    deployables: [kiosk]\n"),
        ["app.jadpo deployable.invalid: `deployables:` isn't a block"]
    );
    assert_eq!(
        problems("    deployables: {}\n"),
        ["app.jadpo deployable.invalid: `deployables:` names no deployable"]
    );
}

#[test]
fn every_api_and_all_background_work_needs_a_deployable() {
    let kiosk_only =
        "    deployables: {\n        kiosk: {\n            serves: [main]\n        }\n    }\n";
    assert_eq!(
        problems(kiosk_only),
        [
            "app.jadpo deployable.unserved: No deployable runs the app's schedules",
            "app.jadpo deployable.unserved: No deployable runs the app's subscribers",
            "app.jadpo deployable.unserved: No deployable serves the API `staff`",
        ]
    );
}

#[test]
fn a_route_that_waits_for_follow_on_work_is_served_where_subscribers_run() {
    let settling = NOTES
        .replace("use cafe/actions { add_note }", "use cafe/actions { add_note }\nuse cafe/queries { note_for }")
        .replace(
            "        run: add_note(input)\n",
            "        run: add_note(input)\n        settle: {\n            answer: note_for(result.id)\n            within: 2s\n        }\n",
        );
    let deployables = "    deployables: {\n        kiosk: {\n            serves: [main]\n        }\n        office: {\n            serves: [staff]\n            runs: [Trigger.schedules, Trigger.subscribers]\n        }\n    }\n";
    let found = problems_with(
        &app(deployables, ""),
        &[("domains/cafe/routes/notes.jadpo", settling.as_str())],
    );
    assert!(
        found.contains(
            &"app.jadpo deployable.settle_unrun: `POST /notes` waits for follow-on work `kiosk` doesn't run"
                .to_string()
        ),
        "{found:#?}"
    );
}

#[test]
fn a_cloudflare_queue_is_refused_in_a_native_deployable_that_receives_it() {
    let queue = "        warehouse: {\n            driver: Driver.cloudflare_queues\n            queue: \"warehouse-events\"\n            dead_letter_queue: \"warehouse-events-dead\"\n        }\n";
    let receiving = "    receiving: {\n        warehouse: {\n            type_at: \"type\"\n            id_at: \"message_id\"\n            events: [StockLow]\n        }\n    }\n";
    let low = (
        "domains/cafe/events/stock_low.jadpo",
        "/// A product is running out.\ntype StockLow = Event {\n    wire_name: \"stock.low\"\n    sku: Text {\n        max_length: 40\n    }\n}\n",
    );
    let application = |kiosk: &str| {
        app(&format!("{receiving}{}", declared(kiosk, NATIVE)), queue).replace(
            "application {",
            "use cafe/events { StockLow }\n\napplication {",
        )
    };
    assert_eq!(
        problems_with(&application(NATIVE), &[low]),
        [
            "app.jadpo deployable.target_unsupported: `kiosk` can't receive `warehouse` natively: Cloudflare pushes it only to a Worker"
        ]
    );
    assert_eq!(problems_with(&application(WASM), &[low]), CLEAN);
}

/// A provider package whose API is a service Jadpo runs beside the app
/// (area 11, "Services").
const PDF: &[(&str, &str)] = &[
    (
        "shared/providers/acme_pdf/v1/package.jadpo",
        "/// Acme's PDF service.\npackage AcmePdf.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
    ),
    (
        "shared/providers/acme_pdf/v1/provider.jadpo",
        "/// Acme's PDF service, run beside the app.\nprovider {\n    protocol: Protocol.json_http\n    runs: {\n        image: \"acme/pdf:1\"\n        port: 3000\n    }\n}\n",
    ),
    (
        "shared/providers/acme_pdf/v1/types/page_text.jadpo",
        "/// A page's text.\npublic type PageText = Text {\n    max_length: 200\n}\n",
    ),
    (
        "shared/providers/acme_pdf/v1/types/render_request.jadpo",
        "/// What a render takes.\npublic type RenderRequest = Object {\n    text: PageText\n}\n",
    ),
    (
        "shared/providers/acme_pdf/v1/resources/documents.jadpo",
        "/// Rendered documents.\npublic resource documents {\n    render: {\n        method: HttpMethod.post\n        path: \"/render\"\n        input: RenderRequest\n        output: RenderRequest\n        source: PackageSource.written\n    }\n}\n",
    ),
    (
        "domains/cafe/actions/pin_note.jadpo",
        "use app { pdf }\nuse cafe/types { NoteText }\nuse shared/providers/acme_pdf\n\naction pin_note(pinned: NoteText) -> Unit {\n    var page: AcmePdf.RenderRequest = pdf.documents.render(AcmePdf.RenderRequest {\n        text: AcmePdf.PageText(pinned)\n    })\n}\n",
    ),
];

#[test]
fn a_service_is_refused_on_workers_until_its_connection_gives_its_host() {
    let service =
        "        pdf: {\n            package: AcmePdf.v1\n            domains: [cafe]\n        }\n";
    let kiosk = |target: &str, connection: &str| {
        problems_with(&app(&declared(target, NATIVE), connection), PDF)
    };
    assert_eq!(kiosk(NATIVE, service), CLEAN);
    assert_eq!(
        kiosk(WASM, service),
        [
            "app.jadpo deployable.target_unsupported: `kiosk` can't call `pdf` on Workers, which runs no containers"
        ]
    );
    // Hosted elsewhere, its connection says where, and Workers reaches it
    // as any HTTP API.
    let hosted = "        pdf: {\n            package: AcmePdf.v1\n            domains: [cafe]\n            host: config.pdf_url\n        }\n";
    let config = ("config.jadpo", "config {\n    pdf_url: Url\n}\n");
    let mut files = PDF.to_vec();
    files.push(config);
    assert_eq!(
        problems_with(&app(&declared(WASM, NATIVE), hosted), &files),
        CLEAN
    );
}

#[test]
fn on_workers_a_cap_on_calls_counts_per_instance() {
    let capped = "        pdf: {\n            package: AcmePdf.v1\n            domains: [cafe]\n            host: config.pdf_url\n            max_concurrent: 2\n        }\n";
    let config = ("config.jadpo", "config {\n    pdf_url: Url\n}\n");
    let mut files = PDF.to_vec();
    files.push(config);
    let caps = |kiosk: &str| {
        let db = Database::default();
        let application = app(&declared(kiosk, NATIVE), capped);
        let mut all: Vec<(&str, &str)> = vec![("app.jadpo", application.as_str())];
        all.extend(
            CAFE.iter()
                .filter(|(path, _)| !files.iter().any(|(again, _)| again == path)),
        );
        all.extend_from_slice(&files);
        let project = support::project(&db, Layout::Project, &all);
        jadpo_check::deployables::per_instance(&db, project)
    };
    assert_eq!(
        caps(WASM),
        [jadpo_check::deployables::PerInstance {
            connection: "pdf".to_string(),
            cap: 2,
            deployable: "kiosk".to_string(),
        }]
    );
    assert_eq!(caps(NATIVE), []);
}

#[test]
fn smtp_on_port_25_is_refused_where_workers_sends_the_email() {
    let mail = |port: &str| {
        format!(
            "        mail: {{\n            driver: Driver.smtp\n            host: \"smtp.cafe.example\"\n{port}        }}\n"
        )
    };
    let email =
        "    email: {\n        connection: mail\n        from: \"hello@cafe.example\"\n    }\n";
    let files: &[(&str, &str)] = &[
        (
            "domains/cafe/emails/thanks_email.jadpo",
            "/// Thanks someone for a note.\ntype ThanksEmail = EmailMessage {\n    name: Text\n}\n",
        ),
        (
            "domains/cafe/emails/thanks_email.txt",
            "Thanks, {{ name }}.\n",
        ),
        (
            "domains/cafe/translations/en_gb.jadpo",
            "translations {\n    no_room: \"There's no room for the sheet.\"\n    thanks_email: \"Thanks, {name}\"\n}\n",
        ),
    ];
    let application = |port: &str, kiosk: &str| {
        let text = app(&format!("{email}{}", declared(kiosk, NATIVE)), &mail(port));
        format!(
            "{}\nlocales {{\n    default: Locale.en_gb\n    supported: [Locale.en_gb]\n}}\n",
            text.replace("application {", "application {\n    display_name: \"Cafe\"")
        )
    };
    let port_25 = "            port: 25\n";
    assert_eq!(
        problems_with(&application(port_25, WASM), files),
        [
            "app.jadpo deployable.target_unsupported: `kiosk` can't send email through `mail` on port 25, which Workers blocks"
        ]
    );
    assert_eq!(problems_with(&application("", WASM), files), CLEAN);
    assert_eq!(problems_with(&application(port_25, NATIVE), files), CLEAN);
}
