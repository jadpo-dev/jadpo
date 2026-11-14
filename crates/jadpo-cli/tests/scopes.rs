//! Tokens narrowed to their scopes (area 9, "Scopes"; area 8,
//! "`external_token`"; chunk N2), against live PostgreSQL: a notebook whose
//! external tokens carry `notes:read` and `notes.write`, each effect of a
//! note naming the scope it needs, and delete naming none.
//!
//! Its route tests run through `jadpo test` here, and compiled with the
//! interpreter's records in the gate's compiled step (`compiled.rs`,
//! "tokens narrowed to their scopes"). Every refusal is one of them: a
//! token that doesn't carry an effect's scope, an effect no scope names, a
//! scope that never reaches past the person's own grants, an upload slot
//! without its record's create or update scope, an entity with
//! no `scopes:`, a token carrying none, a narrowed token repeating its
//! person's request by its idempotency key, and a narrowed token starting
//! a session or issuing an API token. A browser session isn't narrowed.

use std::fs;
use std::path::{Path, PathBuf};

pub(super) const NOTEBOOK: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use notes/entities { Writer }

application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/notebook/sheets"
        }
    }
    apis: {
        main: {
            prefix: /
            authentication: [
                AuthenticationMethod.browser_session,
                AuthenticationMethod.external_token
            ]
        }
    }
}

principal {
    user: {
        subject: Writer.handle
        active: Writer.active == true
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    audience: Text {
        default: "notebook"
    }
    issuer: Url {
        default: "https://id.notebook.example/"
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    browser_session: {
        cookie: "notebook_session"
    }
    api_token: {}
    external_token: {
        issuer: config.issuer
        audience: config.audience
        scopes: {
            notes_read: "notes:read"
            notes_write: "notes.write"
        }
    }
}
"#,
    ),
    (
        "domains/notes/entities/writer.jadpo",
        r#"/// Someone who keeps notes, known by their handle.
entity Writer {
    id: Uuid
    handle: Text {
        unique: true
    }
    active: Bool

    policy: {
        create: [Access.public]
        read: [id]
        update: [id]
        scopes: {
            read: OAuthScope.notes_read
        }
    }
}
"#,
    ),
    (
        "domains/notes/entities/note.jadpo",
        r#"use notes/entities { Writer }

/// A note its writer keeps.
entity Note {
    id: Uuid
    writer_id: Writer.id {
        on_delete: OnDelete.cascade
    }
    text: Text

    policy: {
        create: [writer_id]
        read: [writer_id]
        update: [writer_id]
        delete: [writer_id]
        scopes: {
            create: OAuthScope.notes_write
            read: OAuthScope.notes_read
            update: OAuthScope.notes_write
        }
    }
}
"#,
    ),
    (
        "domains/notes/entities/draft.jadpo",
        r#"use notes/entities { Writer }

/// A draft, which no token issued to another application may touch.
entity Draft {
    id: Uuid
    writer_id: Writer.id {
        on_delete: OnDelete.cascade
    }
    text: Text

    policy: {
        create: [writer_id]
        read: [writer_id]
    }
}
"#,
    ),
    (
        "domains/notes/entities/attachment.jadpo",
        r#"use notes/entities { Writer }

/// A sheet attached to a writer's notebook.
entity Attachment {
    id: Uuid
    writer_id: Writer.id {
        on_delete: OnDelete.cascade
    }
    sheet: File {
        store: sheets
        max_size: 1mb
        types: [FileType.csv]
    }

    policy: {
        create: [writer_id]
        read: [writer_id]
        scopes: {
            create: OAuthScope.notes_write
            read: OAuthScope.notes_read
        }
    }
}
"#,
    ),
    (
        "domains/notes/actions/attachment_slot.jadpo",
        r#"use notes/entities { Attachment }

/// Where a writer sends a sheet to attach.
action attachment_slot() -> UploadSlot {
    return Attachment.sheet_upload()
}
"#,
    ),
    (
        "domains/notes/types/new_note.jadpo",
        r#"use notes/entities { Note }

/// What a new note says.
type NewNote = Input {
    text: Note.text
}
"#,
    ),
    (
        "domains/notes/types/note_view.jadpo",
        r#"use notes/entities { Note }

/// A note as its writer sees it.
type NoteView = Output {
    pick: [Note.id, Note.text]
}
"#,
    ),
    (
        "domains/notes/types/draft_view.jadpo",
        r#"use notes/entities { Draft }

/// A draft as its writer sees it.
type DraftView = Output {
    pick: [Draft.id, Draft.text]
}
"#,
    ),
    (
        "domains/notes/types/device.jadpo",
        r#"/// A new device's API token.
type Device = Output {
    token: ApiToken
}
"#,
    ),
    (
        "domains/notes/failures/note_missing.jadpo",
        r#"type NoteMissing = Failure {
    kind: FailureKind.not_found
}
"#,
    ),
    (
        "domains/notes/failures/no_writer.jadpo",
        r#"type NoWriter = Failure {
    kind: FailureKind.not_found
}
"#,
    ),
    (
        "domains/notes/failures/handle_taken.jadpo",
        r#"type HandleTaken = Failure {
    kind: FailureKind.conflict
}
"#,
    ),
    (
        "domains/notes/actions/join.jadpo",
        r#"use notes/entities { Writer }
use notes/failures { HandleTaken }

/// Registers a writer.
action join(handle: Writer.handle) fails HandleTaken -> Writer {
    return attempt Writer.create {
        set: {
            handle: handle
            active: true
        }
        conflict: {
            Writer.handle: HandleTaken
        }
    }
}
"#,
    ),
    (
        "domains/notes/actions/write_note.jadpo",
        r#"use notes/entities { Note }
use notes/types { NewNote, NoteView }

/// Keeps a new note for its writer.
action write_note(input: NewNote, writer: Principal.user) -> NoteView {
    var note: Note = Note.create {
        set: {
            writer_id: writer.id
            text: input.text
        }
    }
    return NoteView {
        id: note.id
        text: note.text
    }
}
"#,
    ),
    (
        "domains/notes/actions/keep_draft.jadpo",
        r#"use notes/entities { Draft }
use notes/types { NewNote, DraftView }

/// Keeps a draft for its writer.
action keep_draft(input: NewNote, writer: Principal.user) -> DraftView {
    var draft: Draft = Draft.create {
        set: {
            writer_id: writer.id
            text: input.text
        }
    }
    return DraftView {
        id: draft.id
        text: draft.text
    }
}
"#,
    ),
    (
        "domains/notes/actions/throw_away.jadpo",
        r#"use notes/entities { Note, NoteNotFound }

/// Throws a note away.
action throw_away(note_id: Note.id) fails NoteNotFound -> Unit {
    attempt Note.delete(note_id)
}
"#,
    ),
    (
        "domains/notes/actions/add_device.jadpo",
        r#"use notes/entities { Writer }
use notes/types { Device }
use notes/failures { NoWriter }
use notes/queries { this_writer }

/// Gives the signed-in writer's new device an API token.
action add_device(writer: Principal.user) fails NoWriter -> Device {
    var found: Writer = attempt this_writer(writer)
    return Device {
        token: sessions.issue_token(found.id)
    }
}
"#,
    ),
    (
        "domains/notes/actions/sign_browser_in.jadpo",
        r#"use notes/entities { Writer }
use notes/failures { NoWriter }
use notes/queries { this_writer }

/// Signs this browser in as the signed-in writer.
action sign_browser_in(writer: Principal.user) fails NoWriter -> Unit {
    var found: Writer = attempt this_writer(writer)
    sessions.start(found.id)
}
"#,
    ),
    (
        "domains/notes/queries/one_note.jadpo",
        r#"use notes/entities { Note }
use notes/types { NoteView }
use notes/failures { NoteMissing }

/// One note.
query one_note(note_id: Note.id) fails NoteMissing -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteMissing
}
"#,
    ),
    (
        "domains/notes/queries/all_notes.jadpo",
        r#"use notes/entities { Note }
use notes/types { NoteView }

/// Every note the caller may read.
query all_notes() -> List<NoteView> {
    from: Note
    order: [Note.text asc]
    limit: 50
}
"#,
    ),
    (
        "domains/notes/queries/drafts.jadpo",
        r#"use notes/entities { Draft }
use notes/types { DraftView }

/// Every draft the caller may read.
query drafts() -> List<DraftView> {
    from: Draft
    order: [Draft.text asc]
    limit: 50
}
"#,
    ),
    (
        "domains/notes/queries/this_writer.jadpo",
        r#"use notes/entities { Writer }
use notes/failures { NoWriter }

/// The writer signed in.
query this_writer(writer: Principal.user) fails NoWriter -> Writer {
    from: Writer
    where: Writer.id == writer.id
    missing: NoWriter
}
"#,
    ),
    (
        "domains/notes/routes/notebook.jadpo",
        r#"use notes/entities { Note }
use notes/types { Device, DraftView, NewNote, NoteView }
use notes/actions { add_device, attachment_slot, keep_draft, sign_browser_in, throw_away, write_note }
use notes/queries { all_notes, drafts, one_note }

routes Notebook {
    prefix: /notebook

    GET / {
        output: List<NoteView>
        run: all_notes()
    }

    GET /{note_id: Note.id} {
        output: NoteView
        run: one_note(note_id)
    }

    POST / {
        input: NewNote
        output: NoteView
        status: SuccessStatus.created
        run: write_note(input, current_principal.user)
    }

    DELETE /{note_id: Note.id} {
        status: SuccessStatus.no_content
        run: throw_away(note_id)
    }

    GET /drafts {
        output: List<DraftView>
        run: drafts()
    }

    POST /drafts {
        input: NewNote
        output: DraftView
        status: SuccessStatus.created
        run: keep_draft(input, current_principal.user)
    }

    POST /devices {
        output: Device
        status: SuccessStatus.created
        run: add_device(current_principal.user)
    }

    POST /sessions {
        status: SuccessStatus.no_content
        run: sign_browser_in(current_principal.user)
    }

    POST /slots {
        output: UploadSlot
        run: attachment_slot()
    }
}
"#,
    ),
    (
        "domains/notes/routes/notebook.test.jadpo",
        r#"use notes/entities { Writer }
use notes/actions { join, write_note }
use notes/types { NewNote, NoteView }

test "a token reads notes only with the read scope" {
    var ann: Writer = attempt join("ann")
    var kept: NoteView = write_note(NewNote { text: "milk" }, Principal.user(ann))
    var read: Response = request {
        method: HttpMethod.get
        path: "/notebook/{note_id}"
        params: { note_id: kept.id }
        as: Principal.user(ann)
        scopes: [OAuthScope.notes_read]
    }
    assert read.status == 200
    var unscoped: Response = request {
        method: HttpMethod.get
        path: "/notebook/{note_id}"
        params: { note_id: kept.id }
        as: Principal.user(ann)
        scopes: [OAuthScope.notes_write]
    }
    assert unscoped.status == 404
    assert unscoped.code == "note_missing"
    var listed: Response = request {
        method: HttpMethod.get
        path: "/notebook"
        as: Principal.user(ann)
        scopes: [OAuthScope.notes_read]
    }
    assert listed.status == 200
    assert listed.output == [kept]
    var hidden: Response = request {
        method: HttpMethod.get
        path: "/notebook"
        as: Principal.user(ann)
        scopes: [OAuthScope.notes_write]
    }
    assert hidden.status == 200
    assert hidden.output == []
}

test "a token writes notes only with the write scope" {
    var bo: Writer = attempt join("bo")
    var refused: Response = request {
        method: HttpMethod.post
        path: "/notebook"
        input: NewNote { text: "eggs" }
        as: Principal.user(bo)
        scopes: [OAuthScope.notes_read]
    }
    assert refused.status == 403
    assert refused.code == "not_permitted"
    var written: Response = request {
        method: HttpMethod.post
        path: "/notebook"
        input: NewNote { text: "eggs" }
        as: Principal.user(bo)
        scopes: [OAuthScope.notes_write]
    }
    assert written.status == 201
}

test "an effect no scope names is refused to every token, and a session is not narrowed" {
    var cy: Writer = attempt join("cy")
    var kept: NoteView = write_note(NewNote { text: "bread" }, Principal.user(cy))
    var by_token: Response = request {
        method: HttpMethod.delete
        path: "/notebook/{note_id}"
        params: { note_id: kept.id }
        as: Principal.user(cy)
        scopes: [OAuthScope.notes_read, OAuthScope.notes_write]
    }
    assert by_token.status == 404
    var by_session: Response = request {
        method: HttpMethod.delete
        path: "/notebook/{note_id}"
        params: { note_id: kept.id }
        as: Principal.user(cy)
    }
    assert by_session.status == 204
}

test "an entity with no scopes is refused to every token" {
    var di: Writer = attempt join("di")
    var refused: Response = request {
        method: HttpMethod.post
        path: "/notebook/drafts"
        input: NewNote { text: "plan" }
        as: Principal.user(di)
        scopes: [OAuthScope.notes_read, OAuthScope.notes_write]
    }
    assert refused.status == 403
    var kept: Response = request {
        method: HttpMethod.post
        path: "/notebook/drafts"
        input: NewNote { text: "plan" }
        idempotency_key: "draft-1"
        as: Principal.user(di)
    }
    assert kept.status == 201
    var repeated: Response = request {
        method: HttpMethod.post
        path: "/notebook/drafts"
        input: NewNote { text: "plan" }
        idempotency_key: "draft-1"
        as: Principal.user(di)
        scopes: [OAuthScope.notes_read, OAuthScope.notes_write]
    }
    assert repeated.status == 403
    var listed: Response = request {
        method: HttpMethod.get
        path: "/notebook/drafts"
        as: Principal.user(di)
        scopes: [OAuthScope.notes_read, OAuthScope.notes_write]
    }
    assert listed.output == []
}

test "a slot for an upload needs the scope its record's create or update needs" {
    var jo: Writer = attempt join("jo")
    var refused: Response = request {
        method: HttpMethod.post
        path: "/notebook/slots"
        as: Principal.user(jo)
        scopes: [OAuthScope.notes_read]
    }
    assert refused.status == 403
    assert refused.code == "not_permitted"
    var given: Response = request {
        method: HttpMethod.post
        path: "/notebook/slots"
        as: Principal.user(jo)
        scopes: [OAuthScope.notes_write]
    }
    assert given.status == 200
}

test "a scope never reaches past the person's own grants" {
    var ed: Writer = attempt join("ed")
    var fi: Writer = attempt join("fi")
    var theirs: NoteView = write_note(NewNote { text: "secret" }, Principal.user(fi))
    var read: Response = request {
        method: HttpMethod.get
        path: "/notebook/{note_id}"
        params: { note_id: theirs.id }
        as: Principal.user(ed)
        scopes: [OAuthScope.notes_read, OAuthScope.notes_write]
    }
    assert read.status == 404
}

test "a token carrying no scope does nothing" {
    var gu: Writer = attempt join("gu")
    var kept: NoteView = write_note(NewNote { text: "tea" }, Principal.user(gu))
    var read: Response = request {
        method: HttpMethod.get
        path: "/notebook/{note_id}"
        params: { note_id: kept.id }
        as: Principal.user(gu)
        scopes: []
    }
    assert read.status == 404
}

test "a narrowed token never starts a session or issues a token of the person's whole authority" {
    var hal: Writer = attempt join("hal")
    var refused: Response = request {
        method: HttpMethod.post
        path: "/notebook/devices"
        as: Principal.user(hal)
        scopes: [OAuthScope.notes_read, OAuthScope.notes_write]
    }
    assert refused.status == 403
    assert refused.code == "not_permitted"
    var by_session: Response = request {
        method: HttpMethod.post
        path: "/notebook/devices"
        as: Principal.user(hal)
    }
    assert by_session.status == 201
    var started: Response = request {
        method: HttpMethod.post
        path: "/notebook/sessions"
        as: Principal.user(hal)
        scopes: [OAuthScope.notes_read, OAuthScope.notes_write]
    }
    assert started.status == 403
    assert started.code == "not_permitted"
    var by_browser: Response = request {
        method: HttpMethod.post
        path: "/notebook/sessions"
        as: Principal.user(hal)
    }
    assert by_browser.status == 204
}
"#,
    ),
];

/// The notebook, as the compiled step's corpus takes it.
pub(crate) fn notebook() -> Vec<(String, String)> {
    NOTEBOOK
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect()
}

fn jadpo(cwd: &Path, args: &[&str]) -> jadpo_cli::Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = jadpo_term::Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    jadpo_cli::run(&args, &env.with_cwd(cwd))
}

fn project(name: &str, files: &[(String, String)]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("scopes-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    root
}

#[test]
fn every_scope_only_narrows_what_a_token_may_do() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("notebook", &notebook());
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("8 route tests"), "{}", tested.stdout);
}

#[test]
fn the_access_record_and_report_show_each_scope() {
    let root = project("records", &notebook());
    let record = fs::read_to_string(root.join("access.jadpo_record")).expect("access record");
    assert!(
        record.contains(
            "scopes external_token\n    scope notes_read \"notes:read\"\n    scope notes_write \"notes.write\"\n"
        ),
        "{record}"
    );
    assert!(
        record.contains("    scope read OAuthScope.notes_read\n"),
        "{record}"
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let report =
        fs::read_to_string(root.join("build/report/permissions.md")).expect("permissions report");
    assert!(
        report.contains(
            "| scope | `OAuthScope.notes_write` | `OAuthScope.notes_read` | `OAuthScope.notes_write` | refused |"
        ),
        "{report}"
    );
}
