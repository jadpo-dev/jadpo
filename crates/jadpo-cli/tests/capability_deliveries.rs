//! Deliveries that hand their event to a capability, and parking an
//! outside call whose outcome is unknown, against live PostgreSQL (area
//! 12, "Retries and giving up"; area 16, "Email"; chunk 7.0c, from the
//! Phase 6 architecture review): a send that may have acted and has no
//! key is parked as `outcome_unknown` for an operator, never sent again
//! blindly, whether its driver said so, its run was cut off while it was
//! out, or its worker was lost; one that surely did nothing, or has a key,
//! is retried.
//!
//! No capability exists yet (email is chunk 7.3), so a test capability
//! stands in: the `mail` connection's `send`, a fake on the shared test
//! host. A notice board's `relay_note` subscriber is pointed at it, as an
//! `EmailMessage`'s delivery will be, or its action made to send through
//! it first, as an action calling a provider will (chunk 7.2).
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::Program;
use jadpo_runtime::budget::RUN_LIMIT_MS;
use jadpo_runtime::capability::{Answer, Call, Capability, Fake, Faked, Repeat, Scripted};
use jadpo_runtime::runner::{Deferred, run_deferred, sweep};
use jadpo_runtime::{Ctx, Executor, Host, LocalFuture, Outcome, Places, Plan, SqlValue};
use jadpo_std::subscriber::{EventsPlan, Target};
use jadpo_term::Environment;
use jadpo_test::host::{Harness, Timer};
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
        r#"application {
    events: {
        store: main
    }
}
"#,
    ),
    (
        "domains/board/types/words.jadpo",
        r#"type Words = Text {
    min_length: 1
    max_length: 200
}
"#,
    ),
    (
        "domains/board/entities/note.jadpo",
        r#"use board/types { Words }

entity Note {
    id: Uuid
    words: Words

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}
"#,
    ),
    (
        "domains/board/events/note_posted.jadpo",
        r#"use board/entities { Note }

/// A note was posted.
type NotePosted = Event {
    note_id: Note.id
}
"#,
    ),
    (
        "domains/board/events/note_relayed.jadpo",
        r#"use board/entities { Note }

/// A posted note was passed on.
type NoteRelayed = Event {
    note_id: Note.id
}
"#,
    ),
    (
        "domains/board/actions/post_note.jadpo",
        r#"use board/entities { Note }
use board/events { NotePosted }
use board/types { Words }

action post_note(words: Words) -> Note {
    var note: Note = Note.create {
        set: {
            words: words
        }
    }
    emit NotePosted {
        note_id: note.id
    }
    return note
}
"#,
    ),
    (
        "domains/board/actions/relay.jadpo",
        r#"use board/entities { Note }
use board/events { NoteRelayed }

action relay(note_id: Note.id) -> Unit {
    emit NoteRelayed {
        note_id: note_id
    }
}
"#,
    ),
    (
        "domains/board/subscribers/relay_note.jadpo",
        r#"use board/actions { relay }
use board/events { NotePosted }

/// Passes each posted note on.
subscriber relay_note {
    on: NotePosted
    run: relay(event.note_id)
    retry: {
        attempts: 3
        within: 1d
    }
}
"#,
    ),
];

fn project(name: &str) -> PathBuf {
    let root =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("capability-deliveries-7.0c-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    root
}

/// When the test starts: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;
/// An hour, past any retry's first wait.
const HOUR: i64 = 3_600_000;

struct Loaded {
    _root: PathBuf,
    program: Program,
    _databases: jadpo_test::database::TestDatabases,
    url: String,
}

fn load(name: &str) -> Option<Loaded> {
    let root = project(name);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    if !jadpo_test::live() {
        return None;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    Some(Loaded {
        _root: root,
        program,
        _databases: databases,
        url,
    })
}

/// How `relay_note`'s deliveries go, which no source can write yet.
#[derive(Clone)]
enum Relay {
    /// Handed whole to the `mail` connection's `send`, as an
    /// `EmailMessage`'s delivery will be (chunk 7.3).
    Sent(Repeat),
    /// Its action sends through `mail` first, unkeyed, then ends as
    /// given, as an action calling a provider will (chunk 7.2).
    CallsThen(Outcome),
}

/// The program, with `relay_note`'s deliveries going as `relay` says.
struct Stand<'a> {
    program: &'a Program,
    events: EventsPlan,
    relay: Relay,
    relay_action: u32,
}

impl<'a> Stand<'a> {
    fn new(program: &'a Program, relay: Relay) -> Stand<'a> {
        let mut events = program.plan().events().clone();
        let subscriber = events
            .subscribers
            .iter_mut()
            .find(|subscriber| subscriber.key.ends_with("relay_note"))
            .expect("relay_note");
        let relay_action = match &subscriber.target {
            Target::Action { callable, .. } => *callable,
            other => panic!("not an action: {other:?}"),
        };
        if let Relay::Sent(repeat) = &relay {
            subscriber.target = Target::Capability {
                connection: "mail".into(),
                operation: "send".into(),
                repeat: *repeat,
            };
            subscriber.store = None;
        }
        Stand {
            program,
            events,
            relay,
            relay_action,
        }
    }
}

impl Executor for Stand<'_> {
    fn places(&self) -> &Places {
        self.program.places()
    }

    fn started(&self) -> Option<std::rc::Rc<jadpo_runtime::Started>> {
        Executor::started(self.program)
    }

    fn plan(&self) -> Plan<'_> {
        let program = Executor::plan(self.program);
        Plan {
            json: program.json,
            events: Some(&self.events),
            event_numbers: program.event_numbers,
            callables: program.callables,
            ..Plan::NONE
        }
    }

    fn run<'a, 'p: 'a>(
        &'a self,
        ctx: &'a mut Ctx<'p>,
        callable: u32,
        args: Vec<Value>,
    ) -> LocalFuture<'a, Outcome> {
        Box::pin(async move {
            if let Relay::CallsThen(then) = &self.relay
                && callable == self.relay_action
            {
                let send = Call::new("mail", "send", Value::Unit, Repeat::Unkeyed);
                let _ = jadpo_runtime::effects::call(ctx, send).await;
                return then.clone();
            }
            self.program.run(ctx, callable, args).await
        })
    }
}

/// Posts a note saying `words` on `host`, giving the work it left for
/// after its response.
async fn post(harness: &Harness, host: &dyn Host, program: &Program, words: &str) -> Vec<Deferred> {
    let id = program.plan().find("post_note").expect("the action");
    let mut ctx = harness.ctx_on(program, host);
    match program.run(&mut ctx, id, vec![Value::text(words)]).await {
        Outcome::Value(_) => {}
        other => panic!("the note is posted: {other:?}"),
    }
    ctx.take_deferred()
}

/// Each delivery of the note saying `words`: its state, attempts, last
/// failure's code and its cause.
async fn deliveries(
    harness: &Harness,
    words: &str,
) -> Vec<(String, i32, Option<String>, Option<String>)> {
    let sql = format!(
        "SELECT \"d\".\"status\", \"d\".\"attempts\", \"d\".\"last_failure\" ->> 'code', \"d\".\"last_failure\" ->> 'cause' FROM \"jadpo_deliveries\" AS \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" JOIN \"note\" AS \"n\" ON \"n\".\"id\"::text = \"o\".\"payload\" ->> 'note_id' WHERE \"d\".\"subscriber\" LIKE '%relay_note' AND \"n\".\"words\" = '{words}'"
    );
    let text = |value: &SqlValue| match value {
        SqlValue::Text(text) => Some(text.clone()),
        _ => None,
    };
    harness
        .rows(&sql)
        .await
        .into_iter()
        .map(|row| match row.as_slice() {
            [
                SqlValue::Text(status),
                SqlValue::Int4(attempts),
                code,
                cause,
            ] => (status.clone(), *attempts, text(code), text(cause)),
            other => panic!("not a delivery: {other:?}"),
        })
        .collect()
}

fn state(
    status: &str,
    attempts: i32,
    code: Option<&str>,
    cause: Option<&str>,
) -> Vec<(String, i32, Option<String>, Option<String>)> {
    vec![(
        status.to_string(),
        attempts,
        code.map(str::to_string),
        cause.map(str::to_string),
    )]
}

/// The test capability, on a harness over `url` whose clock reads `T0`.
fn stage(url: &str) -> (Rc<Fake>, Harness) {
    (
        Rc::new(Fake::new()),
        Harness::postgres(url).at(T0).random_from(0),
    )
}

fn faked<'b>(harness: &'b Harness, fake: &Rc<Fake>) -> Faked<&'b Harness> {
    Faked::new(harness).with("mail", Rc::clone(fake) as Rc<dyn Capability>)
}

/// A delivery that targets a capability hands it the event whole, through
/// the seam, and its record alone says it was sent: succeeded once, and
/// never again on a later sweep.
#[test]
fn a_delivery_hands_its_event_to_a_capability_once() {
    let Some(loaded) = load("sent") else {
        return;
    };
    let (fake, harness) = stage(&loaded.url);
    let host = faked(&harness, &fake);
    let stand = Stand::new(&loaded.program, Relay::Sent(Repeat::Unkeyed));
    Runtime::new().expect("runtime").block_on(async {
        let work = post(&harness, &host, &loaded.program, "hello").await;
        let ran = run_deferred(&stand, &host, work).await;
        assert!(matches!(ran.as_slice(), [Outcome::Value(_)]), "{ran:?}");
        assert_eq!(
            deliveries(&harness, "hello").await,
            state("succeeded", 1, None, None)
        );
        let sent = fake.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].name(), "mail.send");
        assert!(
            matches!(&sent[0].input, Value::Record(event) if event.get("note_id").is_some()),
            "the event, whole"
        );
        assert_eq!(
            harness
                .count("SELECT count(*) FROM \"jadpo_handled_deliveries\"")
                .await,
            0,
            "no action, so no handled record"
        );
        harness.set_now(T0 + HOUR);
        sweep(&stand, &host, "main", T0 + HOUR)
            .await
            .expect("the sweep runs");
        assert_eq!(fake.sent().len(), 1, "sent once");
    });
}

/// An unkeyed send whose outcome its driver doesn't know is parked as
/// `outcome_unknown` for an operator, never sent again by itself; a
/// refused one fails for good; one that surely did nothing is retried.
#[test]
fn an_unkeyed_send_whose_outcome_is_unknown_is_parked_never_sent_again() {
    let Some(loaded) = load("unknown") else {
        return;
    };
    let (fake, harness) = stage(&loaded.url);
    let host = faked(&harness, &fake);
    let stand = Stand::new(&loaded.program, Relay::Sent(Repeat::Unkeyed));
    Runtime::new().expect("runtime").block_on(async {
        for (words, answer) in [
            ("lost", Answer::Unknown),
            ("refused", Answer::Failed("recipient_rejected".into())),
            ("down", Answer::Unavailable),
        ] {
            fake.script("send", Scripted::Answer(answer));
            let work = post(&harness, &host, &loaded.program, words).await;
            run_deferred(&stand, &host, work).await;
        }
        assert_eq!(
            deliveries(&harness, "lost").await,
            state("outcome_unknown", 1, Some("outcome_unknown"), None)
        );
        assert_eq!(
            deliveries(&harness, "refused").await,
            state("failed", 1, Some("recipient_rejected"), None)
        );
        assert_eq!(
            deliveries(&harness, "down").await,
            state("pending", 1, Some("unavailable"), None)
        );
        // An hour on, only the one that surely did nothing is sent again.
        harness.set_now(T0 + HOUR);
        sweep(&stand, &host, "main", T0 + HOUR)
            .await
            .expect("the sweep runs");
        assert_eq!(fake.sent().len(), 4, "three sends, and the retry");
        assert_eq!(
            deliveries(&harness, "down").await,
            state("succeeded", 2, None, None)
        );
        assert_eq!(
            deliveries(&harness, "lost").await,
            state("outcome_unknown", 1, Some("outcome_unknown"), None),
            "parked, not sent again"
        );
    });
}

/// A keyed send whose outcome is unknown is sent again under its key,
/// which its provider keeps, so it acts once.
#[test]
fn a_keyed_send_whose_outcome_is_unknown_is_sent_again() {
    let Some(loaded) = load("keyed") else {
        return;
    };
    let (fake, harness) = stage(&loaded.url);
    let host = faked(&harness, &fake);
    let stand = Stand::new(&loaded.program, Relay::Sent(Repeat::Keyed));
    Runtime::new().expect("runtime").block_on(async {
        fake.script("send", Scripted::Answer(Answer::Unknown));
        let work = post(&harness, &host, &loaded.program, "keyed").await;
        run_deferred(&stand, &host, work).await;
        assert_eq!(
            deliveries(&harness, "keyed").await,
            state("pending", 1, Some("unavailable"), None)
        );
        harness.set_now(T0 + HOUR);
        sweep(&stand, &host, "main", T0 + HOUR)
            .await
            .expect("the sweep runs");
        assert_eq!(
            deliveries(&harness, "keyed").await,
            state("succeeded", 2, None, None)
        );
        assert_eq!(fake.sent().len(), 2);
    });
}

/// An unkeyed send still out when its run's time runs out may have acted:
/// parked, never retried as a run that timed out otherwise is.
#[test]
fn an_unkeyed_send_cut_off_while_it_is_out_is_parked() {
    let Some(loaded) = load("cut") else {
        return;
    };
    let (fake, harness) = stage(&loaded.url);
    let host = faked(&harness, &fake);
    let stand = Stand::new(&loaded.program, Relay::Sent(Repeat::Unkeyed));
    Runtime::new().expect("runtime").block_on(async {
        fake.script("send", Scripted::Hang);
        let work = post(&harness, &host, &loaded.program, "slow").await;
        harness.set_timer(RUN_LIMIT_MS, Timer::Now);
        run_deferred(&stand, &host, work).await;
        harness.set_timer(RUN_LIMIT_MS, Timer::Never);
        assert_eq!(
            deliveries(&harness, "slow").await,
            state(
                "outcome_unknown",
                1,
                Some("outcome_unknown"),
                Some("deadline_exceeded")
            )
        );
        harness.set_now(T0 + HOUR);
        sweep(&stand, &host, "main", T0 + HOUR)
            .await
            .expect("the sweep runs");
        assert_eq!(fake.sent().len(), 1, "never sent again");
    });
}

/// An unkeyed send is marked `sending` before it goes out, so one whose
/// worker was lost meanwhile is parked by the sweep once its lease has
/// passed, never claimed and sent again.
#[test]
fn an_unkeyed_send_whose_worker_was_lost_is_parked_not_sent_again() {
    let Some(loaded) = load("lost-worker") else {
        return;
    };
    let (fake, harness) = stage(&loaded.url);
    let host = faked(&harness, &fake);
    let stand = Stand::new(&loaded.program, Relay::Sent(Repeat::Unkeyed));
    Runtime::new().expect("runtime").block_on(async {
        // Its work dropped, then its record left as a worker lost while
        // its send was out leaves it: `sending`, its lease a minute on.
        drop(post(&harness, &host, &loaded.program, "crashed").await);
        let lease = (T0 + 60_000) / 1000;
        harness
            .rows(&format!(
                "UPDATE \"jadpo_deliveries\" SET \"status\" = 'sending', \"attempts\" = 1, \"next_attempt_at\" = to_timestamp({lease}) RETURNING \"id\""
            ))
            .await;
        // Before its lease passes, the sweep leaves it alone.
        harness.set_now(T0 + 30_000);
        sweep(&stand, &host, "main", T0 + 30_000).await.expect("the sweep runs");
        assert_eq!(deliveries(&harness, "crashed").await, state("sending", 1, None, None));
        harness.set_now(T0 + HOUR);
        sweep(&stand, &host, "main", T0 + HOUR).await.expect("the sweep runs");
        assert_eq!(
            deliveries(&harness, "crashed").await,
            state(
                "outcome_unknown",
                1,
                Some("outcome_unknown"),
                Some("lease_expired")
            )
        );
        assert!(fake.sent().is_empty(), "never sent again");
    });
}

/// A subscriber's action that sent an unkeyed call and then ended in a way
/// that is otherwise retried is parked instead, since a retry would send
/// the call again.
#[test]
fn an_action_that_sent_an_unkeyed_call_then_failed_is_parked_not_retried() {
    let Some(loaded) = load("action") else {
        return;
    };
    let (fake, harness) = stage(&loaded.url);
    let host = faked(&harness, &fake);
    let failed = Outcome::Failed(Box::new(jadpo_runtime::unavailable()));
    let stand = Stand::new(&loaded.program, Relay::CallsThen(failed));
    Runtime::new().expect("runtime").block_on(async {
        let work = post(&harness, &host, &loaded.program, "half").await;
        run_deferred(&stand, &host, work).await;
        assert_eq!(
            deliveries(&harness, "half").await,
            state(
                "outcome_unknown",
                1,
                Some("outcome_unknown"),
                Some("unavailable")
            )
        );
        harness.set_now(T0 + HOUR);
        sweep(&stand, &host, "main", T0 + HOUR)
            .await
            .expect("the sweep runs");
        assert_eq!(fake.sent().len(), 1, "never sent again");
    });
}
