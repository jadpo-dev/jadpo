//! After the response (chunk 8.2h) is proved on a news desk of its own,
//! whose model is a server this test starts speaking OpenAI's Chat
//! Completions API: a draft streamed from the model reaches the client
//! piece by piece, its first piece before the model has written the rest,
//! and a client that goes away closes the model's connection; stories
//! filed together are each summarised by a subscriber after the answer,
//! in `waitUntil`, and a walk through the archive's clippings is a batch
//! run, both taking longer than an invocation's 30 seconds, so each hands
//! what it didn't reach on to a fresh invocation of the Worker, a request
//! to itself through the loopback `ctx.exports` gives its `JadpoHandOn`
//! entrypoint, which carries it on to its end. Since chunk 8.4a the desk
//! answers its limits on the module's own stack and V8's: a thousand
//! frames of a plain recursion and of a heavy one, four hundred inline
//! functions running at once, and a level more of each the
//! `recursion_limit` fault, the isolate serving on.
//!

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::deployable::{apply, migrations, rows};
use crate::support::probes;

use super::{Database, Workerd, jadpo, linked, names, now_ms, stamped};

/// The news desk, built for Workers: stories filed are summarised by a
/// subscriber, the archive's clippings by a batch run, and a draft is
/// streamed from the model as it is written.
const NEWSROOM: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"application {
    connections: {
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 1001
        }
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        local_model: {
            driver: Driver.openai_compatible
            url: config.model_url
        }
    }
    events: {
        store: main
    }
    rate_limits: {
        store: limits
    }
    ai: {
        connection: local_model
        models: {
            default: config.drafting_model
        }
        limits: {
            per_day: {
                tokens: 2000000
            }
        }
    }
    deployables: {
        desk: {
            serves: [main]
            runs: [Trigger.subscribers, Trigger.schedules]
            target: Target.wasm
        }
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    database_url: PostgresUrl
    model_url: Url
    drafting_model: ModelId {
        default: "local/llama3.2"
    }
}
"#,
    ),
    (
        "domains/newsroom/actions/draft.jadpo",
        r#"use newsroom/failures { ModelUnavailable }
use newsroom/prompts { DraftStory }
use newsroom/types { DraftAsked }

/// Drafts a story as it is written.
action draft(asked: DraftAsked) fails ModelUnavailable -> AiStream {
    return attempt ai.stream(
        DraftStory {
            headline: asked.headline
        },
        max_output: 300,
        timeout: 20s
    ) {
        refused: ModelUnavailable
        timed_out: ModelUnavailable
        over_budget: ModelUnavailable
        too_long: ModelUnavailable
    }
}
"#,
    ),
    (
        "domains/newsroom/actions/file_stories.jadpo",
        r#"use newsroom/entities { Story }
use newsroom/events { StoryFiled }
use newsroom/types { Filed, FiledStories }

/// Files each story, each summarised after the answer.
action file_stories(filed: Filed) -> FiledStories {
    for headline in filed.headlines {
        var story: Story = Story.create {
            set: {
                headline: headline
                summary: none
            }
        }
        emit StoryFiled {
            story_id: story.id
        }
    }
    return FiledStories {
        filed: list.count(filed.headlines)
    }
}
"#,
    ),
    (
        "domains/newsroom/actions/resummarise_clippings.jadpo",
        r#"use newsroom/actions { summarise_clipping }
use newsroom/entities { ArchiveWalk }
use newsroom/queries { clippings_page }
use newsroom/types { ClippingPage, Resummarising }

/// Tells every clipping in one line again, one after another.
action resummarise_clippings() -> Resummarising {
    var run: BatchRun = batches.each(
        from: clippings_page(ClippingPage {}),
        each: clipping -> summarise_clipping(clipping)
    )
    var walk: ArchiveWalk = ArchiveWalk.create {
        set: {
            run: run
        }
    }
    return Resummarising {
        walk_id: walk.id
    }
}
"#,
    ),
    (
        "domains/newsroom/actions/summarise_clipping.jadpo",
        r#"use newsroom/entities { Clipping }
use newsroom/failures { ModelUnavailable }
use newsroom/prompts { Summarise }
use newsroom/types { Summary }

/// Tells a clipping in one line.
action summarise_clipping(clipping: Clipping) fails ModelUnavailable -> Unit {
    var summary: Summary = attempt ai.generate(
        Summarise {
            headline: clipping.headline
        },
        as: Summary,
        max_output: 100
    ) {
        refused: ModelUnavailable
        malformed: ModelUnavailable
        timed_out: ModelUnavailable
        over_budget: ModelUnavailable
        too_long: ModelUnavailable
    }
    var summarised: Clipping = Clipping.update(clipping) {
        set: {
            summary: summary.line
        }
    }
    assert summarised.summary == summary.line
}
"#,
    ),
    (
        "domains/newsroom/actions/summarise_story.jadpo",
        r#"use newsroom/entities { Story, StoryNotFound }
use newsroom/failures { ModelUnavailable }
use newsroom/prompts { Summarise }
use newsroom/types { Summary }

/// Tells a story filed in one line.
action summarise_story(story_id: Story.id) fails StoryNotFound, ModelUnavailable -> Unit {
    var story: Story = attempt Story.get(story_id)
    var summary: Summary = attempt ai.generate(
        Summarise {
            headline: story.headline
        },
        as: Summary,
        max_output: 100
    ) {
        refused: ModelUnavailable
        malformed: ModelUnavailable
        timed_out: ModelUnavailable
        over_budget: ModelUnavailable
        too_long: ModelUnavailable
    }
    var summarised: Story = Story.update(story) {
        set: {
            summary: summary.line
        }
    }
    assert summarised.summary == summary.line
}
"#,
    ),
    (
        "domains/newsroom/entities/archive_walk.jadpo",
        r#"/// A walk through the archive, telling each clipping again.
entity ArchiveWalk {
    id: Uuid
    run: BatchRun

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}
"#,
    ),
    (
        "domains/newsroom/entities/clipping.jadpo",
        r#"use newsroom/types { Headline, SummaryLine }

/// A story kept in the archive.
entity Clipping {
    id: Uuid
    headline: Headline
    summary: SummaryLine?

    policy: {
        read: [Access.system]
        update: [Access.system]
    }
}
"#,
    ),
    (
        "domains/newsroom/entities/story.jadpo",
        r#"use newsroom/types { Headline, SummaryLine }

/// A story filed.
entity Story {
    id: Uuid
    headline: Headline
    summary: SummaryLine?

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}
"#,
    ),
    (
        "domains/newsroom/events/story_filed.jadpo",
        r#"use newsroom/entities { Story }

/// A story was filed.
type StoryFiled = Event {
    story_id: Story.id
}
"#,
    ),
    (
        "domains/newsroom/failures/model_unavailable.jadpo",
        r#"/// The model can't answer now.
type ModelUnavailable = Failure {
    kind: FailureKind.unavailable
    message: "The model can't answer now."
}
"#,
    ),
    (
        "domains/newsroom/prompts/draft_story.jadpo",
        r#"use newsroom/types { Headline }

/// Drafts a story under a headline.
type DraftStory = Prompt {
    headline: Headline
}
"#,
    ),
    (
        "domains/newsroom/prompts/draft_story.prompt",
        r#"# Instructions

You draft a newspaper's stories.

# Request

{headline}
"#,
    ),
    (
        "domains/newsroom/prompts/summarise.jadpo",
        r#"use newsroom/types { Headline }

/// Tells a story in one line.
type Summarise = Prompt {
    headline: Headline
}
"#,
    ),
    (
        "domains/newsroom/prompts/summarise.prompt",
        r#"# Instructions

You tell a newspaper's stories in one line.

# Request

{headline}
"#,
    ),
    (
        "domains/newsroom/queries/clippings_page.jadpo",
        r#"use newsroom/entities { Clipping }
use newsroom/types { ClippingPage }

/// The archive, a page at a time.
query clippings_page(page: ClippingPage) -> Page<Clipping> {
    from: Clipping
    order: [Clipping.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
"#,
    ),
    (
        "domains/newsroom/routes/desk.jadpo",
        r#"use newsroom/actions { draft, file_stories, resummarise_clippings }
use newsroom/types { DraftAsked, Filed, FiledStories, Resummarising }

/// The news desk.
routes Desk {
    prefix: /desk

    /// Files stories, each summarised after the answer.
    POST /stories {
        authentication: RouteAuthentication.none
        input: Filed
        output: FiledStories
        status: SuccessStatus.created
        run: file_stories(input)
    }

    /// Tells every clipping in the archive in one line again.
    POST /clippings/summaries {
        authentication: RouteAuthentication.none
        output: Resummarising
        status: SuccessStatus.accepted
        run: resummarise_clippings()
    }

    /// Drafts a story, sent as it is written.
    POST /drafts {
        authentication: RouteAuthentication.none
        input: DraftAsked
        output: AiStream
        deadline: 30s
        run: draft(input)
    }
}
"#,
    ),
    (
        "domains/newsroom/subscribers/summarise_filed.jadpo",
        r#"use newsroom/actions { summarise_story }
use newsroom/events { StoryFiled }

/// Tells each story filed in one line.
subscriber summarise_filed {
    on: StoryFiled
    run: summarise_story(event.story_id)
}
"#,
    ),
    (
        "domains/newsroom/types/clipping_page.jadpo",
        r#"/// A page of clippings.
type ClippingPage = Input {
    page_size: PageSize {
        default: 20
    }
    after: PageCursor {
        optional: true
    }
}
"#,
    ),
    (
        "domains/newsroom/types/draft_asked.jadpo",
        r#"use newsroom/types { Headline }

/// A headline to draft a story for.
type DraftAsked = Input {
    headline: Headline
}
"#,
    ),
    (
        "domains/newsroom/types/filed.jadpo",
        r#"use newsroom/types { Headline }

/// Stories filed together.
type Filed = Input {
    headlines: List<Headline>
}
"#,
    ),
    (
        "domains/newsroom/types/filed_stories.jadpo",
        r#"/// How many stories were filed.
type FiledStories = Output {
    filed: Int
}
"#,
    ),
    (
        "domains/newsroom/types/headline.jadpo",
        r#"/// A story's headline.
type Headline = Text {
    min_length: 1
    max_length: 120
}
"#,
    ),
    (
        "domains/newsroom/types/resummarising.jadpo",
        r#"use newsroom/entities { ArchiveWalk }

/// The walk telling the archive's clippings again.
type Resummarising = Output {
    walk_id: ArchiveWalk.id
}
"#,
    ),
    (
        "domains/newsroom/types/summary.jadpo",
        r#"use newsroom/types { SummaryLine }

/// What the model says a story is.
type Summary = Output {
    line: SummaryLine
}
"#,
    ),
    (
        "domains/newsroom/types/summary_line.jadpo",
        r#"/// A story told in one line.
type SummaryLine = Text {
    max_length: 200
}
"#,
    ),
];

/// The news desk, written where its build is kept between runs, a file
/// left as it was keeping its time, with its migration and records.
fn newsroom() -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled/workers/newsroom");
    // Beside the desk's own, the routes that probe a deployable's limits
    // (chunk 8.4a), its policy opening both to anyone.
    let policy = format!(
        "use newsroom\nuse probe\n\npolicy {{\n    actions: {{\n        newsroom.draft: [Access.public]\n{}    }}\n}}\n",
        probes::POLICY
    );
    let files = NEWSROOM
        .iter()
        .chain(probes::FILES)
        .copied()
        .chain([("policy.jadpo", policy.as_str())]);
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("a folder")).expect("folders");
        if fs::read_to_string(&path).ok().as_deref() != Some(text) {
            fs::write(&path, text).expect("written");
        }
    }
    jadpo(&root, &["api", "update"]);
    if !root.join("database/migrations").exists() {
        jadpo(&root, &["migrate", "new", "initial"]);
    }
    jadpo(&root, &["records", "update"]);
    root
}

/// How long the desk's model takes to summarise a story.
const SUMMARY_MS: u64 = 2_000;

/// How many stories are filed together, and how many clippings the
/// archive holds: each summarised in [`SUMMARY_MS`], more than one
/// invocation's 30 seconds of either.
const STORIES: usize = 18;

/// The desk's model: a server on this machine speaking OpenAI's Chat
/// Completions API. A call for a summary is answered after
/// [`SUMMARY_MS`] with the line `Told: <headline>`, and when it was asked
/// for which headline noted; a call for a stream is sent its head and its
/// first piece, `First `, and then handed to the test, which writes the
/// rest or watches it close.
struct Model {
    port: u16,
    asked: Arc<Mutex<Vec<(Instant, String)>>>,
    streams: mpsc::Receiver<TcpStream>,
}

impl Model {
    fn start() -> Model {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("an address").port();
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (streamed, streams) = mpsc::channel();
        let noted = Arc::clone(&asked);
        std::thread::spawn(move || {
            for connection in listener.incoming() {
                let Ok(stream) = connection else {
                    return;
                };
                let noted = Arc::clone(&noted);
                let streamed = streamed.clone();
                std::thread::spawn(move || Model::answer(stream, &noted, &streamed));
            }
        });
        Model {
            port,
            asked,
            streams,
        }
    }

    fn answer(
        mut stream: TcpStream,
        asked: &Mutex<Vec<(Instant, String)>>,
        streamed: &mpsc::Sender<TcpStream>,
    ) {
        let Some(request) = model_request(&stream) else {
            return;
        };
        let body: Value = request
            .split_once("\r\n\r\n")
            .and_then(|(_, body)| serde_json::from_str(body).ok())
            .unwrap_or(Value::Null);
        if body["stream"] == true {
            let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
            if stream.write_all(head.as_bytes()).is_ok()
                && model_chunk(&mut stream, &completion_chunk("First ", None)).is_ok()
            {
                let _ = streamed.send(stream);
            }
            return;
        }
        let headline = regex::Regex::new(r"(Story|Clipping) \d\d")
            .expect("a pattern")
            .find(&request)
            .map(|found| found.as_str().to_string())
            .unwrap_or_default();
        asked
            .lock()
            .expect("noted")
            .push((Instant::now(), headline.clone()));
        std::thread::sleep(Duration::from_millis(SUMMARY_MS));
        let answer = serde_json::json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "model": "local/llama3.2",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": serde_json::json!({ "line": format!("Told: {headline}") }).to_string(),
                },
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 40, "completion_tokens": 12, "total_tokens": 52}
        })
        .to_string();
        // A call the Worker gave up on finds its connection gone.
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
            answer.len()
        );
    }

    /// When each headline starting with `kind` was asked for, from the
    /// first.
    fn asked(&self, kind: &str) -> Vec<(Duration, String)> {
        let asked = self.asked.lock().expect("noted");
        let mine: Vec<&(Instant, String)> = asked
            .iter()
            .filter(|(_, headline)| headline.starts_with(kind))
            .collect();
        let Some(first) = mine.iter().map(|(at, _)| *at).min() else {
            return Vec::new();
        };
        mine.iter()
            .map(|(at, headline)| (at.duration_since(first), headline.clone()))
            .collect()
    }
}

/// One request the model read: its head and body.
fn model_request(stream: &TcpStream) -> Option<String> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut seen = String::new();
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().ok()?;
        }
        seen.push_str(&line);
        if line == "\r\n" {
            break;
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    seen.push_str(&String::from_utf8_lossy(&body));
    Some(seen)
}

/// A chunk of OpenAI's streamed chat completion.
fn completion_chunk(content: &str, finish: Option<&str>) -> String {
    let event = serde_json::json!({
        "id": "chatcmpl-1",
        "object": "chat.completion.chunk",
        "model": "local/llama3.2",
        "choices": [{"index": 0, "delta": {"content": content}, "finish_reason": finish}]
    });
    format!("data: {event}\n\n")
}

/// Writes one part of a chunked body.
fn model_chunk(stream: &mut TcpStream, text: &str) -> std::io::Result<()> {
    stream.write_all(format!("{:x}\r\n{text}\r\n", text.len()).as_bytes())?;
    stream.flush()
}

/// A client of the Worker reading a streamed answer as it comes.
struct Reader {
    reader: BufReader<TcpStream>,
}

impl Reader {
    /// Asks the desk for a draft under `headline`.
    fn draft(port: u16, headline: &str) -> Reader {
        let body = serde_json::json!({ "headline": headline }).to_string();
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("workerd listens");
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .expect("a timeout");
        write!(
            stream,
            "POST /desk/drafts HTTP/1.1\r\nHost: desk\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .expect("sent");
        Reader {
            reader: BufReader::new(stream),
        }
    }

    /// The answer's head, lowercased.
    fn head(&mut self) -> String {
        let mut head = String::new();
        loop {
            let mut line = String::new();
            self.reader.read_line(&mut line).expect("a line");
            head.push_str(&line.to_ascii_lowercase());
            if line == "\r\n" || line.is_empty() {
                return head;
            }
        }
    }

    /// What the chunked body holds from here until `until` ends it, or
    /// its end.
    fn until(&mut self, until: &str) -> String {
        let mut text = String::new();
        while !text.ends_with(until) {
            let mut size = String::new();
            self.reader.read_line(&mut size).expect("a size");
            let size = usize::from_str_radix(size.trim(), 16).expect("a chunk's size");
            if size == 0 {
                break;
            }
            let mut data = vec![0; size + 2];
            self.reader.read_exact(&mut data).expect("a chunk");
            data.truncate(size);
            text.push_str(&String::from_utf8_lossy(&data));
        }
        text
    }
}

/// After the response (chunk 8.2h), on Workers: a draft streamed as it is
/// written, its first piece before the model writes the rest, and a
/// client going away closing the model's connection; stories filed
/// together summarised by a subscriber after the answer, and a batch run
/// through the archive, each taking longer than one invocation's 30
/// seconds, handing what is left on to a fresh invocation of the Worker,
/// which carries it to its end; and the minute's cron sweeping what the
/// budget's end left.
#[test]
fn a_worker_streams_answers_and_hands_on_what_one_invocation_leaves() {
    let root = newsroom();
    let output = jadpo(&root, &["build", "--json"]);
    let document: Value = serde_json::from_str(&output.stdout).expect("JSON");
    let built = &document["data"];
    let desk = &built["deployables"][0];
    assert_eq!(desk["deployable"], "desk", "{built}");
    let out = root.join("build/target/desk");
    let wrangler: Value = serde_json::from_str(
        &fs::read_to_string(out.join("wrangler.json")).expect("the Wrangler file"),
    )
    .expect("JSON");
    // It runs background work, handed on through the loopback binding
    // `ctx.exports` gives the entrypoint its shim exports, so no binding
    // names the Worker, whatever name `wrangler deploy` gives it (chunk
    // 8.4b).
    assert!(wrangler.get("services").is_none(), "{wrangler}");
    // Beside its database's Hyperdrive binding, the minute's cron and its
    // rate-limiting bindings.
    assert_eq!(
        wrangler["hyperdrive"][0]["binding"], "DATABASE_URL",
        "{wrangler}"
    );
    assert_eq!(
        wrangler["triggers"]["crons"],
        serde_json::json!(["* * * * *"]),
        "{wrangler}"
    );
    assert!(
        wrangler["ratelimits"]
            .as_array()
            .is_some_and(|limits| !limits.is_empty()),
        "{wrangler}"
    );
    let shim = fs::read_to_string(out.join("worker.mjs")).expect("the shim");
    assert!(
        shim.contains("export class JadpoHandOn extends WorkerEntrypoint {"),
        "{shim}"
    );
    assert!(
        names(
            &linked(&root, "desk"),
            "jadpo_host_workers",
            "deployable::hand_on"
        ),
        "it lacks the hand-on handler"
    );

    if !jadpo_test::live() {
        return;
    }
    let database = Database::on_tcp();
    apply(&database.native, &migrations(&root));
    for at in 1..=STORIES {
        rows(
            &database.native,
            &format!(
                "INSERT INTO clipping (id, headline) VALUES (gen_random_uuid(), 'Clipping {at:02}') RETURNING 1"
            ),
        );
    }
    let model = Model::start();
    let model_url = format!("http://127.0.0.1:{}/v1", model.port);
    let worker = Workerd::start(
        &out,
        &wrangler,
        &[
            ("DATABASE_URL", &database.worker),
            ("MODEL_URL", &model_url),
        ],
    );

    // A draft reaches the client as the model writes it: its first piece
    // before the model has written the rest.
    let mut reader = Reader::draft(worker.port, "Harbour wall repaired");
    let mut written = model
        .streams
        .recv_timeout(Duration::from_secs(30))
        .unwrap_or_else(|_| panic!("the model wasn't asked:\n{}", worker.log()));
    let head = reader.head();
    assert!(head.starts_with("http/1.1 200 ok\r\n"), "{head}");
    assert!(
        head.contains("content-type: text/event-stream\r\n"),
        "{head}"
    );
    assert!(head.contains("cache-control: no-store\r\n"), "{head}");
    assert!(!head.contains("content-length"), "{head}");
    assert_eq!(
        reader.until("\n\n"),
        "event: piece\ndata: {\"text\":\"First \"}\n\n",
        "{}",
        worker.log()
    );
    model_chunk(
        &mut written,
        &completion_chunk("then the rest.", Some("stop")),
    )
    .expect("a piece");
    model_chunk(&mut written, "data: [DONE]\n\n").expect("done");
    model_chunk(&mut written, "").expect("the end");
    drop(written);
    assert_eq!(
        reader.until("event: done\ndata: {}\n\n"),
        "event: piece\ndata: {\"text\":\"then the rest.\"}\n\nevent: done\ndata: {}\n\n"
    );
    assert_eq!(reader.until("\n\n"), "", "the stream ends after `done`");

    // A client that goes away ends the request, and with it the model's
    // call: workerd finds the client gone when it next sends it a piece,
    // so the model writes on until its connection closes.
    let mut reader = Reader::draft(worker.port, "Ferry timetable changes");
    let mut written = model
        .streams
        .recv_timeout(Duration::from_secs(30))
        .expect("the model is asked");
    reader.head();
    assert_eq!(
        reader.until("\n\n"),
        "event: piece\ndata: {\"text\":\"First \"}\n\n"
    );
    drop(reader);
    written
        .set_read_timeout(Some(Duration::from_millis(200)))
        .expect("a timeout");
    let until = Instant::now() + Duration::from_secs(10);
    let mut closed = false;
    while !closed && Instant::now() < until {
        if model_chunk(&mut written, &completion_chunk("more ", None)).is_err() {
            closed = true;
            break;
        }
        let mut rest = [0u8; 16];
        closed = match written.read(&mut rest) {
            Ok(0) => true,
            Err(error) => !matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ),
            Ok(_) => false,
        };
    }
    assert!(
        closed,
        "the model's connection stayed open:\n{}",
        worker.log()
    );

    // Stories filed together, and a walk through the archive, each more
    // work than one invocation's 30 seconds.
    let headlines: Vec<String> = (1..=STORIES).map(|at| format!("Story {at:02}")).collect();
    let began = Instant::now();
    let (status, body) = worker.send(
        "POST",
        "/desk/stories",
        &[],
        &serde_json::json!({ "headlines": headlines }).to_string(),
    );
    assert_eq!(status, 201, "{body}\n{}", worker.log());
    let (status, body) = worker.send("POST", "/desk/clippings/summaries", &[], "");
    assert_eq!(status, 202, "{body}\n{}", worker.log());
    assert!(
        began.elapsed() < Duration::from_secs(10),
        "answered after {:?}",
        began.elapsed()
    );
    let summarised = |table: &str| {
        count_where(
            &database.native,
            &format!("SELECT count(*) FROM {table} WHERE summary = 'Told: ' || headline"),
        )
    };
    let finished = || {
        count_where(
            &database.native,
            "SELECT count(*) FROM jadpo_batch_runs WHERE state = 'finished'",
        )
    };
    let until = Instant::now() + Duration::from_secs(120);
    while (summarised("story") < STORIES as i64 - 1
        || summarised("clipping") < STORIES as i64
        || finished() < 1)
        && Instant::now() < until
    {
        std::thread::sleep(Duration::from_millis(250));
    }
    // Every clipping is told by the run, which finished, its last items
    // asked for after the first invocation's budget was spent: in a fresh
    // invocation it was handed on to.
    assert_eq!(summarised("clipping"), STORIES as i64, "{}", worker.log());
    assert_eq!(
        rows(
            &database.native,
            "SELECT state, items_done::text FROM jadpo_batch_runs"
        ),
        vec![vec!["finished".to_string(), STORIES.to_string()]],
        "{}",
        worker.log()
    );
    let clippings = model.asked("Clipping");
    assert!(
        clippings
            .iter()
            .any(|(after, _)| *after > Duration::from_secs(28)),
        "every clipping was asked for within one invocation: {clippings:?}"
    );
    // Every story but at most one, whose summary the budget's end cut
    // short and which waits to be tried again, is told by the subscriber
    // after the answer, the last of them in a fresh invocation.
    assert!(
        summarised("story") >= STORIES as i64 - 1,
        "{} of {STORIES} stories told:\n{}",
        summarised("story"),
        worker.log()
    );
    let stories = model.asked("Story");
    assert!(
        stories
            .iter()
            .any(|(after, _)| *after > Duration::from_secs(28)),
        "every story was asked for within one invocation: {stories:?}"
    );
    let waiting = rows(
        &database.native,
        "SELECT status, attempts::text, last_failure ->> 'code' FROM jadpo_deliveries WHERE status <> 'succeeded'",
    );
    assert!(
        waiting.is_empty()
            || waiting
                == vec![vec![
                    "pending".to_string(),
                    "1".to_string(),
                    "deadline_exceeded".to_string()
                ]],
        "{waiting:?}"
    );
    // The minute's cron sweeps what an invocation left: due now, a summary
    // waiting to be tried again is told, and every delivery has succeeded.
    rows(
        &database.native,
        "UPDATE jadpo_deliveries SET next_attempt_at = now() WHERE status = 'pending' RETURNING 1",
    );
    assert_eq!(worker.scheduled(now_ms()), "ok", "{}", worker.log());
    assert_eq!(summarised("story"), STORIES as i64, "{}", worker.log());
    assert_eq!(
        rows(
            &database.native,
            "SELECT status FROM jadpo_deliveries WHERE status <> 'succeeded'"
        ),
        Vec::<Vec<String>>::new(),
        "{}",
        worker.log()
    );
    // Its limits, probed on the module's own stack and V8's (chunk 8.4a):
    // a thousand frames of any of the walks, four hundred inline
    // functions, and a level more of each the `recursion_limit` fault,
    // logged as such, the isolate serving on, where a trap would leave it
    // hung.
    for (path, depth, expected) in probes::WALKS {
        let (status, body) = worker.send("POST", path, &[], &probes::walk(*depth));
        match expected {
            Ok(()) => assert_eq!(
                (status, body.as_str()),
                (200, format!(r#"{{"depth":"{depth}"}}"#).as_str()),
                "{path} {depth}\n{}",
                worker.log()
            ),
            Err(code) => {
                assert_eq!(status, 500, "{path} {depth}: {body}\n{}", worker.log());
                assert!(body.contains(code), "{path} {depth}: {body}");
            }
        }
    }
    assert_eq!(
        worker
            .events("operation.failed")
            .iter()
            .filter(|event| event["classification"] == "recursion_limit")
            .count(),
        3,
        "{}",
        worker.log()
    );
    let started = worker.events("runtime.started");
    assert_eq!(started.len(), 1, "{}", worker.log());
    for event in worker.events("") {
        stamped(&event, built, desk);
    }
}

/// The one number `sql` counts.
fn count_where(connection: &str, sql: &str) -> i64 {
    rows(connection, sql)
        .first()
        .and_then(|row| row.first())
        .and_then(|count| count.parse().ok())
        .unwrap_or(0)
}
