//! API tokens issued narrowed to some scopes (area 8, "API tokens"; area
//! 9, "Scopes"; chunk 10.13b), as a deployable answers them
//! (`http::serve_request`) on the native host against live PostgreSQL. A
//! notebook declares its own scopes in `api_token: { scopes: }`, and its
//! sign-up gives a member's app three tokens: one with the member's whole
//! authority, one narrowed to `notes:read`, and one to `notes:read` and
//! `notes:write`.
//!
//! - The whole token reads, writes and deletes the member's notes.
//! - A narrowed token is held to its scopes exactly as a provider's
//!   narrowed token is: the read-only token reads a note and can't write
//!   one, and neither narrowed token deletes, an effect no scope names.
//! - `sessions.tokens` lists each token with the scopes it was issued
//!   with, `null` for the whole one, and never the token itself.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips it.

use std::path::Path;

use jadpo_host_native::{NativeHost, Runtime};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::http::{Request, Response, Worker, serve_request};

const FILES: [(&str, &str); 13] = [
    (
        "app.jadpo",
        r#"use notes/entities { Member }

application {}

principal {
    user: {
        subject: Member.handle
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    api_token: {
        scopes: {
            notes_read: "notes:read"
            notes_write: "notes:write"
        }
    }
}
"#,
    ),
    (
        "domains/notes/types/handle.jadpo",
        r#"/// A member's name.
type Handle = Text {
    max_length: 40
}
"#,
    ),
    (
        "domains/notes/types/note_text.jadpo",
        r#"/// A note's text.
type NoteText = Text {
    max_length: 200
}
"#,
    ),
    (
        "domains/notes/types/note_view.jadpo",
        r#"use notes/entities { Note }

/// A note, as its author sees it.
type NoteView = Output {
    pick: [Note.id, Note.text]
}
"#,
    ),
    (
        "domains/notes/entities/member.jadpo",
        r#"use notes/types { Handle }

/// Someone who keeps notes.
entity Member {
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
        "domains/notes/entities/note.jadpo",
        r#"use notes/entities { Member }
use notes/types { NoteText }

/// A note only its author reads.
entity Note {
    id: Uuid
    author_id: Member.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText

    policy: {
        create: [author_id]
        read: [author_id]
        update: [author_id]
        delete: [author_id]
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
        "domains/notes/failures/handle_taken.jadpo",
        r#"/// Someone has that handle already.
type HandleTaken = Failure {
    kind: FailureKind.conflict
}
"#,
    ),
    (
        "domains/notes/actions/sign_up.jadpo",
        r#"use notes/entities { Member }
use notes/types { Handle }
use notes/failures { HandleTaken }

/// Registers a member and gives their app three tokens, each shown once.
action sign_up(input: SignUp) fails HandleTaken -> SignedUp {
    var member: Member = attempt Member.create {
        set: {
            handle: input.handle
        }
        conflict: {
            Member.handle: HandleTaken
        }
    }
    return SignedUp {
        whole: sessions.issue_token(member.id)
        reading: sessions.issue_token(member.id, scopes: [OAuthScope.notes_read])
        writing: sessions.issue_token(member.id, scopes: [OAuthScope.notes_read, OAuthScope.notes_write])
    }
}

type SignUp = Input {
    handle: Handle
}

type SignedUp = Output {
    whole: ApiToken
    reading: ApiToken
    writing: ApiToken
}
"#,
    ),
    (
        "domains/notes/actions/write_note.jadpo",
        r#"use notes/entities { Note }
use notes/types { NoteText, NoteView }

/// Writes a note as the signed-in member.
action write_note(input: WriteNote, author: Principal.user) -> NoteView {
    var note: Note = Note.create {
        set: {
            author_id: author.id
            text: input.text
        }
    }
    return NoteView {
        id: note.id
        text: note.text
    }
}

type WriteNote = Input {
    text: NoteText
}
"#,
    ),
    (
        "domains/notes/actions/delete_note.jadpo",
        r#"use notes/entities { Note, NoteNotFound }

/// Deletes one of the member's notes.
action delete_note(note_id: Note.id) fails NoteNotFound -> Unit {
    attempt Note.delete(note_id)
}
"#,
    ),
    (
        "domains/notes/queries/one_note.jadpo",
        r#"use notes/entities { Note, NoteNotFound }
use notes/types { NoteView }

/// One of the member's notes.
query one_note(note_id: Note.id) fails NoteNotFound -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}
"#,
    ),
    (
        "domains/notes/routes/notes.jadpo",
        r#"use notes/entities { Note }
use notes/actions { SignUp, SignedUp, WriteNote, delete_note, sign_up, write_note }
use notes/queries { one_note }
use notes/types { NoteView }

/// A member's notes, and their app's tokens.
routes Notes {
    prefix: /

    POST /sign-up {
        authentication: RouteAuthentication.none
        input: SignUp
        output: SignedUp
        run: sign_up(input)
    }

    GET /tokens {
        output: List<IssuedToken>
        run: sessions.tokens(current_principal.user.id)
    }

    POST /notes {
        input: WriteNote
        output: NoteView
        run: write_note(input, current_principal.user)
    }

    GET /notes/{note_id: Note.id} {
        output: NoteView
        run: one_note(note_id)
    }

    DELETE /notes/{note_id: Note.id} {
        status: SuccessStatus.no_content
        run: delete_note(note_id)
    }
}
"#,
    ),
];

fn jadpo(cwd: &Path, args: &[&str]) {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let env = jadpo_term::Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", "1792504800")
        .with_cwd(cwd);
    let done = jadpo_cli::run(&args, &env);
    assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
}

fn request(method: &str, path: &str, token: Option<&str>, body: &str) -> Request {
    let mut headers = Vec::new();
    if let Some(token) = token {
        headers.push(("authorization".to_string(), format!("Bearer {token}")));
    }
    if !body.is_empty() {
        headers.push(("content-type".into(), "application/json".into()));
    }
    Request {
        method: method.into(),
        path: path.into(),
        headers,
        body: body.as_bytes().to_vec(),
        address: Some("10.0.0.1".into()),
        ..Request::default()
    }
}

fn json(response: &Response) -> serde_json::Value {
    serde_json::from_slice(&response.body).unwrap_or_else(|_| {
        panic!(
            "not JSON ({}): {}",
            response.status,
            String::from_utf8_lossy(&response.body)
        )
    })
}

#[test]
fn a_token_issued_with_scopes_does_only_what_they_allow() {
    if !jadpo_test::live() {
        return;
    }
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("issued-scopes");
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in FILES {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    jadpo(&root, &["migrate", "new", "initial"]);
    jadpo(&root, &["records", "update"]);
    jadpo(&root, &["check"]);
    let loaded = jadpo_db::load(&root);
    let prepared = Prepared::load(&loaded.db, loaded.project).expect("checks");
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::new(prepared);
    let server = jadpo_test::database::server().expect("a PostgreSQL server");
    let url = server.create_database("issued_scopes").expect("a database");
    let store = jadpo_host_native::postgres(&url).expect("a store");
    let runtime = Runtime::new().expect("a runtime");
    runtime
        .block_on(jadpo_runtime::migrate(
            &store,
            steps.get("main").expect("main's migrations"),
        ))
        .expect("migrates");
    let host = NativeHost::new().with_store("main", store);
    let worker = Worker::default();
    let app = program.app();
    let send = |request: Request| -> Response {
        runtime.block_on(serve_request(&app, &program, &host, &worker, &request))
    };
    let signed_up = send(request("POST", "/sign-up", None, r#"{"handle":"ann"}"#));
    assert_eq!(signed_up.status, 200, "{}", json(&signed_up));
    let tokens = json(&signed_up);
    let whole = tokens["whole"].as_str().expect("a token").to_string();
    let reading = tokens["reading"].as_str().expect("a token").to_string();
    let writing = tokens["writing"].as_str().expect("a token").to_string();
    let write = |token: &str, text: &str| {
        send(request(
            "POST",
            "/notes",
            Some(token),
            &format!(r#"{{"text":"{text}"}}"#),
        ))
    };
    // The whole token writes; the read-only one can't, the writing one can.
    let first = write(&whole, "one");
    assert_eq!(first.status, 200, "{}", json(&first));
    let note = json(&first)["id"].as_str().expect("an id").to_string();
    let refused = write(&reading, "two");
    assert_eq!(refused.status, 403, "{}", json(&refused));
    assert_eq!(refused.code.as_deref(), Some("not_permitted"));
    let second = write(&writing, "three");
    assert_eq!(second.status, 200, "{}", json(&second));
    // Each reads the member's note.
    for token in [&whole, &reading, &writing] {
        let read = send(request("GET", &format!("/notes/{note}"), Some(token), ""));
        assert_eq!(read.status, 200, "{}", json(&read));
        assert_eq!(json(&read)["text"], "one");
    }
    // No scope names delete, so neither narrowed token deletes; the whole
    // one does.
    for token in [&reading, &writing] {
        let delete = send(request(
            "DELETE",
            &format!("/notes/{note}"),
            Some(token),
            "",
        ));
        assert!(delete.status >= 400, "{}", delete.status);
    }
    let still = send(request("GET", &format!("/notes/{note}"), Some(&whole), ""));
    assert_eq!(still.status, 200);
    let deleted = send(request(
        "DELETE",
        &format!("/notes/{note}"),
        Some(&whole),
        "",
    ));
    assert!(deleted.status < 300, "{}", deleted.status);
    // The listing names each token's scopes, and never a token.
    let listed = send(request("GET", "/tokens", Some(&whole), ""));
    assert_eq!(listed.status, 200, "{}", json(&listed));
    let listed = json(&listed);
    // Issued in the same instant, so compared in any order.
    let mut scopes: Vec<String> = listed
        .as_array()
        .expect("a list")
        .iter()
        .map(|token| token["scopes"].to_string())
        .collect();
    scopes.sort();
    assert_eq!(
        scopes,
        [
            r#"["notes:read","notes:write"]"#,
            r#"["notes:read"]"#,
            "null",
        ]
    );
    let text = listed.to_string();
    for token in [&whole, &reading, &writing] {
        assert!(!text.contains(token.as_str()), "{text}");
    }
}
