//! How many times a request waits on PostgreSQL (chunk 4.10): the runtime
//! answers requests as a deployable does (`http::serve_request`) on the
//! native host against live PostgreSQL, through a transport that counts
//! round trips, each write to the server after reading its last answer. A
//! signed-in write with an `Idempotency-Key`, its repeat, a `GET` policy
//! hides the record of, and the readiness check are counted once the
//! store's connections are open.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live tests.

use std::cell::Cell;
use std::path::Path;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use jadpo_drivers::{Endpoint, Stream, Transport};
use jadpo_host_native::{NativeHost, NativeTransport, Postgres, Runtime};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::LocalFuture;
use jadpo_runtime::http::{Request, Response, Worker, serve_request};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Counts round trips across every connection it opens.
#[derive(Clone, Default)]
struct Counting {
    trips: Rc<Cell<u32>>,
}

impl Transport for Counting {
    fn connect(&self, endpoint: &Endpoint) -> LocalFuture<'_, std::io::Result<Box<dyn Stream>>> {
        let endpoint = endpoint.clone();
        let trips = Rc::clone(&self.trips);
        Box::pin(async move {
            let inner = NativeTransport.connect(&endpoint).await?;
            Ok(Box::new(Counted {
                inner,
                trips,
                answered: true,
            }) as Box<dyn Stream>)
        })
    }

    fn spawn(&self, task: LocalFuture<'static, ()>) {
        NativeTransport.spawn(task);
    }

    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        NativeTransport.sleep(millis)
    }
}

/// A connection whose first write after an answer counts one round trip:
/// what is written together, before the server answers, is one.
struct Counted {
    inner: Box<dyn Stream>,
    trips: Rc<Cell<u32>>,
    /// The server has answered since the last write.
    answered: bool,
}

impl Counted {
    fn wrote(&mut self, n: usize) {
        if n > 0 && self.answered {
            self.answered = false;
            self.trips.set(self.trips.get() + 1);
        }
    }
}

impl AsyncRead for Counted {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let read = Pin::new(&mut self.inner).poll_read(cx, buf);
        if matches!(read, Poll::Ready(Ok(()))) && buf.filled().len() > before {
            self.answered = true;
        }
        read
    }
}

impl AsyncWrite for Counted {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let written = Pin::new(&mut self.inner).poll_write(cx, bytes);
        if let Poll::Ready(Ok(n)) = written {
            self.wrote(n);
        }
        written
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        let written = Pin::new(&mut self.inner).poll_write_vectored(cx, bufs);
        if let Poll::Ready(Ok(n)) = written {
            self.wrote(n);
        }
        written
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use notes/entities { Member }
use notes/types { MemberStatus }

application {}

principal {
    user: {
        subject: Member.handle
        active: Member.status == MemberStatus.active
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        "authentication {\n    api_token: {}\n}\n",
    ),
    (
        "domains/notes/entities/member.jadpo",
        r#"use notes/types { Handle, MemberStatus }

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
    status: MemberStatus

    policy: {
        create: [Access.public]
        read: [id]
        update: [id]
        delete: [id]
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
    }
}
"#,
    ),
    (
        "domains/notes/actions/sign_up.jadpo",
        r#"use notes/entities { Member }
use notes/types { Handle, MemberStatus }
use notes/failures { HandleTaken }

/// Registers a member and gives their app a token, shown once.
action sign_up(input: SignUp) fails HandleTaken -> SignedUp {
    var member: Member = attempt Member.create {
        set: {
            handle: input.handle
            status: MemberStatus.active
        }
        conflict: {
            Member.handle: HandleTaken
        }
    }
    return SignedUp {
        token: sessions.issue_token(member.id)
    }
}

type SignUp = Input {
    handle: Handle
}

type SignedUp = Output {
    token: ApiToken
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
        "domains/notes/queries/one_note.jadpo",
        r#"use notes/entities { Note, NoteNotFound }
use notes/types { NoteView }

query one_note(note_id: Note.id) fails NoteNotFound -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}
"#,
    ),
    (
        "domains/notes/routes/account.jadpo",
        r#"use notes/actions { SignUp, SignedUp, sign_up }

routes Account {
    prefix: /account

    POST /sign-up {
        authentication: RouteAuthentication.none
        input: SignUp
        output: SignedUp
        status: SuccessStatus.created
        run: sign_up(input)
    }
}
"#,
    ),
    (
        "domains/notes/routes/notes.jadpo",
        r#"use notes/entities { Note }
use notes/types { NoteView }
use notes/actions { WriteNote, write_note }
use notes/queries { one_note }

routes Notes {
    prefix: /notes

    POST / {
        input: WriteNote
        output: NoteView
        status: SuccessStatus.created
        run: write_note(input, current_principal.user)
    }

    GET /{note_id: Note.id} {
        output: NoteView
        run: one_note(note_id)
    }
}
"#,
    ),
    (
        "domains/notes/failures/handle_taken.jadpo",
        "type HandleTaken = Failure {\n    kind: FailureKind.conflict\n    message: \"That handle is taken.\"\n}\n",
    ),
    (
        "domains/notes/types/handle.jadpo",
        "type Handle = Text {\n    min_length: 3\n    max_length: 40\n}\n",
    ),
    (
        "domains/notes/types/member_status.jadpo",
        "type MemberStatus = Enum {\n    active\n    suspended\n}\n",
    ),
    (
        "domains/notes/types/note_text.jadpo",
        "type NoteText = Text {\n    min_length: 1\n    max_length: 500\n}\n",
    ),
    (
        "domains/notes/types/note_view.jadpo",
        "use notes/entities { Note }\n\ntype NoteView = Output {\n    pick: [Note.id, Note.text]\n}\n",
    ),
];

fn jadpo(cwd: &Path, args: &[&str]) {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let env = jadpo_term::Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", "1792504800")
        .with_cwd(cwd);
    let done = jadpo_cli::run(&args, &env);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
}

/// A request, as the native host hands one over, from one address.
fn request(method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Request {
    let mut headers: Vec<(String, String)> = headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
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
fn a_signed_in_write_and_a_hidden_record_take_the_fewest_round_trips() {
    if !jadpo_test::live() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("jadpo-round-trips-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let root = dir.join("notes");
    for (path, text) in FILES {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    jadpo(&root, &["migrate", "new", "initial"]);
    jadpo(&root, &["api", "update"]);
    jadpo(&root, &["check"]);
    let loaded = jadpo_db::load(&root);
    let prepared = Prepared::load(&loaded.db, loaded.project).expect("checks");
    let set = jadpo_check::data::migrations::migrations(&loaded.db, loaded.project);
    let steps = jadpo_check::data::migrations::store_steps(set);
    let program = Program::new(prepared);
    let server = jadpo_test::database::server().expect("a PostgreSQL server");
    let url = server.create_database("trips").expect("a database");
    let counting = Counting::default();
    let store = Postgres::new(&url, Rc::new(counting.clone())).expect("a store");
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
    let send = |request: &Request| -> (Response, u32) {
        let before = counting.trips.get();
        let response = runtime.block_on(serve_request(&app, &program, &host, &worker, request));
        (response, counting.trips.get() - before)
    };
    let token = |handle: &str| {
        let (signed_up, _) = send(&request(
            "POST",
            "/account/sign-up",
            &[],
            &format!(r#"{{"handle":"{handle}"}}"#),
        ));
        assert_eq!(signed_up.status, 201, "{}", json(&signed_up));
        format!(
            "Bearer {}",
            json(&signed_up)["token"].as_str().expect("a token")
        )
    };
    let alice = token("alice");
    let bob = token("bob");
    let write = |key: &str, text: &str| {
        send(&request(
            "POST",
            "/notes",
            &[("authorization", &alice), ("idempotency-key", key)],
            &format!(r#"{{"text":"{text}"}}"#),
        ))
    };
    let (first, _) = write("k1", "one");
    assert_eq!(first.status, 201, "{}", json(&first));
    // The sign-in lookup, the key's claim, the note's insert with the
    // transaction's `BEGIN`, the response kept over the claim, the commit.
    let (second, trips) = write("k2", "two");
    assert_eq!(second.status, 201, "{}", json(&second));
    assert_eq!(trips, 5, "a signed-in write with a key");
    let (repeat, trips) = write("k2", "two");
    assert_eq!((repeat.status, &repeat.body), (201, &second.body));
    assert_eq!(
        trips, 2,
        "a repeat: the lookup and the claim, which finds the response"
    );
    let id = json(&second)["id"].as_str().expect("an id").to_string();
    let read = |caller: &str| {
        send(&request(
            "GET",
            &format!("/notes/{id}"),
            &[("authorization", caller)],
            "",
        ))
    };
    let (own, _) = read(&alice);
    assert_eq!(own.status, 200, "{}", json(&own));
    // The lookup, then the terminal read's queued BEGIN/read/COMMIT. This
    // proven read-only scope can already have committed when policy hides
    // its row; client interpretation must still return the same safe 404.
    let (hidden, trips) = read(&bob);
    assert_eq!(hidden.status, 404, "{}", json(&hidden));
    assert_eq!(json(&hidden)["error"]["code"], "note_not_found");
    assert_eq!(trips, 2, "a policed 404 with a queued terminal read");
    let (ready, trips) = send(&request("GET", "/health/ready", &[], ""));
    assert_eq!((ready.status, trips), (200, 1), "`SELECT 1`, on its own");
    let (_, trips) = send(&request("GET", "/health/ready", &[], ""));
    assert_eq!(trips, 0, "the worker's last check, within a second");
    drop(host);
    let _ = std::fs::remove_dir_all(&dir);
}
