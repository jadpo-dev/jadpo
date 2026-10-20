//! A model's answer streamed to a route (area 21, `ai.stream`; area 7,
//! "Streaming a model's answer"; chunk 5.5v6): a support desk that sends
//! the assistant's reply as it is written, as server-sent events.
//!
//! - **The fake.** `jadpo test` answers every `ai.stream` through the
//!   `ai` fake: a route test reads a scripted stream piece by piece, a
//!   failure after the first piece arrives as an `error` event, and one
//!   before it is the app's own failure.
//! - **The local driver.** `jadpo dev` streams from a server that speaks
//!   OpenAI's Chat Completions API, started in the test on this machine
//!   (never a real provider, never the network, no key): each piece
//!   reaches the client before the model has finished, a stream that
//!   breaks part-way ends with an `error` event, and a client that goes
//!   away closes the model's connection.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::http::{Dev, wait_for};

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("ai-stream-5.5v6-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &Environment::empty().with_cwd(cwd))
}

/// The desk: a prompt, an action that streams its answer and maps every
/// failure before the first piece, and a route that sends it.
fn desk(name: &str) -> PathBuf {
    let root = folder(name);
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        local_model: {\n            driver: Driver.openai_compatible\n            url: config.model_url\n        }\n    }\n    ai: {\n        connection: local_model\n        models: {\n            default: config.drafting_model\n        }\n        limits: {\n            per_day: {\n                tokens: 2000000\n            }\n        }\n    }\n}\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    model_url: Url {\n        default: \"http://localhost:11434/v1\"\n    }\n    drafting_model: ModelId {\n        default: \"local/llama3.2\"\n    }\n}\n",
    );
    write(
        &root,
        "policy.jadpo",
        "use support\n\npolicy {\n    actions: {\n        support.stream_answer: [Access.public]\n    }\n}\n",
    );
    write(
        &root,
        "domains/support/types/customer_question.jadpo",
        "/// What a customer asked.\ntype CustomerQuestion = Text {\n    max_length: 500\n}\n",
    );
    write(
        &root,
        "domains/support/types/asked.jadpo",
        "use support/types { CustomerQuestion }\n\n/// A customer's question, as the desk sends it.\ntype Asked = Input {\n    question: CustomerQuestion\n}\n",
    );
    write(
        &root,
        "domains/support/failures/assistant_unavailable.jadpo",
        "/// The assistant can't answer now.\ntype AssistantUnavailable = Failure {\n    kind: FailureKind.unavailable\n    message: \"The assistant can't answer now.\"\n}\n",
    );
    write(
        &root,
        "domains/support/prompts/answer_question.jadpo",
        "use support/types { CustomerQuestion }\n\n/// Answers a customer's question.\ntype AnswerQuestion = Prompt {\n    question: CustomerQuestion\n}\n",
    );
    write(
        &root,
        "domains/support/prompts/answer_question.prompt",
        "# Instructions\n\nYou answer questions from customers of an online bakery.\n\n# Request\n\n{question}\n",
    );
    write(
        &root,
        "domains/support/actions/stream_answer.jadpo",
        "use support/failures { AssistantUnavailable }\nuse support/prompts { AnswerQuestion }\nuse support/types { Asked }\n\n/// Streams an answer to a customer's question as it is written.\naction stream_answer(asked: Asked) fails AssistantUnavailable -> AiStream {\n    return attempt ai.stream(\n        AnswerQuestion {\n            question: asked.question\n        },\n        max_output: 600,\n        timeout: 8s\n    ) {\n        refused: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n}\n",
    );
    write(
        &root,
        "domains/support/routes/answers.jadpo",
        "use support/actions { stream_answer }\nuse support/types { Asked }\n\n/// Customers' questions.\nroutes Answers {\n    prefix: /answers\n\n    /// Streams the assistant's reply as it is written.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: Asked\n        output: AiStream\n        run: stream_answer(input)\n    }\n}\n",
    );
    // The counts of what its calls use are kept in `main` (chunk 5.5v5).
    for args in [&["migrate", "new", "initial"][..], &["api", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    root
}

/// The route tests a desk's developer writes.
const TESTS: &str = r#"use support/actions { stream_answer }
use support/failures { AssistantUnavailable }
use support/types { Asked }

test "a reply comes piece by piece" {
    fakes: {
        ai.answer_question: ["Our sourdough is baked beside sesame."]
    }
    var response: Response = request {
        method: HttpMethod.post
        path: "/answers"
        input: Asked {
            question: "Sesame?"
        }
    }
    assert response.status == 200
    assert response.pieces == ["Our sourdough is ", "baked beside sesame."]
    assert response.code == none
}

test "a failure after the first piece arrives as an error event" {
    fakes: {
        ai.answer_question: [partial("Our sourdough is baked", timed_out)]
    }
    var response: Response = request {
        method: HttpMethod.post
        path: "/answers"
        input: Asked {
            question: "Sesame?"
        }
    }
    assert response.status == 200
    assert response.pieces == ["Our sourdough is ", "baked"]
    assert response.code == "unavailable"
}

test "a cut-off after the first piece arrives as an error event" {
    fakes: {
        ai.answer_question: [partial("Our sourdough", over_budget)]
    }
    var response: Response = request {
        method: HttpMethod.post
        path: "/answers"
        input: Asked {
            question: "Sesame?"
        }
    }
    assert response.pieces == ["Our sourdough"]
    assert response.code == "unavailable"
}

test "a refusal before the first piece is the app's own failure" {
    fakes: {
        ai.answer_question: [refused]
    }
    var response: Response = request {
        method: HttpMethod.post
        path: "/answers"
        input: Asked {
            question: "Pick a lock?"
        }
    }
    assert response.status == 503
    assert response.code == "assistant_unavailable"
    assert response.pieces == []
}

test "each failure before the first piece is the app's own" {
    fakes: {
        ai.answer_question: [timed_out, over_budget, too_long]
    }
    assert stream_answer(Asked { question: "Rye?" }) fails AssistantUnavailable
    assert stream_answer(Asked { question: "Rye?" }) fails AssistantUnavailable
    assert stream_answer(Asked { question: "Rye?" }) fails AssistantUnavailable
}

test "an unscripted stream is a generated answer" {
    var response: Response = request {
        method: HttpMethod.post
        path: "/answers"
        input: Asked {
            question: "Rye?"
        }
    }
    assert response.status == 200
    assert response.code == none
    assert list.count(response.pieces) > 0
}
"#;

/// A route test reads a scripted stream piece by piece, and a failure
/// after the first piece arrives as an error event.
#[test]
fn a_route_test_reads_a_scripted_stream_piece_by_piece() {
    let root = desk("fake");
    write(&root, "domains/support/routes/answers.test.jadpo", TESTS);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(
        tested.stdout.contains("6 passed"),
        "{}{}",
        tested.stdout,
        tested.stderr
    );

    // The record and OpenAPI describe the stream.
    let record = fs::read_to_string(root.join("api/main.jadpo_record")).expect("record");
    assert!(
        record.contains("    output AiStream (server-sent events)\n"),
        "{record}"
    );
    let document: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("build/openapi/main.json")).expect("OpenAPI"),
    )
    .expect("JSON");
    let ok = &document["paths"]["/answers"]["post"]["responses"]["200"];
    let content = &ok["content"]["text/event-stream"];
    assert_eq!(content["schema"]["type"], "string", "{ok}");
    assert_eq!(
        content["x-jadpo-stream"]["events"]["piece"]["required"],
        serde_json::json!(["text"])
    );
    // The desk writes no store, so a key keeps nothing.
    assert_eq!(
        document["paths"]["/answers"]["post"]["x-jadpo-idempotency"]["keeps"],
        "nothing"
    );
}

/// A chunk of OpenAI's streamed chat completion.
fn chunk(content: &str, finish: Option<&str>) -> String {
    let event = serde_json::json!({
        "id": "chatcmpl-1",
        "object": "chat.completion.chunk",
        "model": "local/llama3.2",
        "choices": [{"index": 0, "delta": {"content": content}, "finish_reason": finish}]
    });
    format!("data: {event}\n\n")
}

/// One request a model server read: its head and body.
fn read_request(stream: &TcpStream) -> String {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut seen = String::new();
    let mut length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("a line");
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().expect("a length");
        }
        seen.push_str(&line);
        if line == "\r\n" || line.is_empty() {
            break;
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).expect("a body");
    seen.push_str(&String::from_utf8_lossy(&body));
    seen
}

/// Writes one part of a chunked body.
fn send_chunk(stream: &mut TcpStream, text: &str) -> std::io::Result<()> {
    stream.write_all(format!("{:x}\r\n{text}\r\n", text.len()).as_bytes())?;
    stream.flush()
}

/// A client of `jadpo dev` reading a streamed answer as it comes.
struct Client {
    reader: BufReader<TcpStream>,
}

impl Client {
    fn ask(port: u16, question: &str) -> Client {
        let body = serde_json::json!({ "question": question }).to_string();
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connects");
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .expect("timeout");
        write!(
            stream,
            "POST /answers HTTP/1.1\r\nhost: localhost\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        )
        .expect("sends");
        Client {
            reader: BufReader::new(stream),
        }
    }

    /// The response's head, lowercased.
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

    /// The next whole event, `event:` and `data:`, from the chunked body.
    fn event(&mut self) -> String {
        let mut text = String::new();
        while !text.ends_with("\n\n") {
            let mut size = String::new();
            self.reader.read_line(&mut size).expect("a size");
            let size = usize::from_str_radix(size.trim(), 16).expect("a chunk's size");
            if size == 0 {
                return text;
            }
            let mut data = vec![0; size + 2];
            self.reader.read_exact(&mut data).expect("a chunk");
            data.truncate(size);
            text.push_str(&String::from_utf8_lossy(&data));
        }
        text
    }
}

/// `jadpo dev` streams a local model's answer as it comes: the request
/// asks for a stream, the first piece reaches the client while the model
/// is still writing, and `done` follows the last; a stream the model
/// breaks off ends with an `error` event.
#[test]
fn jadpo_dev_streams_a_local_models_answer_as_it_comes() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("address").port();
    let (go_on, carry_on) = mpsc::channel::<()>();
    let (seen_sender, seen) = mpsc::channel::<String>();
    let model = std::thread::spawn(move || {
        // The first answer: a piece, a wait for the test to read it, then
        // the rest.
        let (mut stream, _) = listener.accept().expect("a request");
        seen_sender.send(read_request(&stream)).expect("seen");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n")
            .expect("head");
        send_chunk(&mut stream, &chunk("Our sourdough ", None)).expect("a piece");
        carry_on.recv().expect("told to go on");
        send_chunk(&mut stream, &chunk("is baked beside sesame.", Some("stop"))).expect("a piece");
        send_chunk(&mut stream, "data: [DONE]\n\n").expect("done");
        send_chunk(&mut stream, "").expect("the end");
        drop(stream);
        // The second: a piece, then the connection breaks.
        let (mut stream, _) = listener.accept().expect("a request");
        read_request(&stream);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n")
            .expect("head");
        send_chunk(&mut stream, &chunk("Our rye ", None)).expect("a piece");
        drop(stream);
        // The third: a piece, then it waits; the client goes away, and
        // the connection closes, which a read sees.
        let (mut stream, _) = listener.accept().expect("a request");
        read_request(&stream);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n")
            .expect("head");
        send_chunk(&mut stream, &chunk("Our spelt ", None)).expect("a piece");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("timeout");
        let mut rest = [0u8; 16];
        // Closed: a read ends, where an open connection would wait.
        match stream.read(&mut rest) {
            Ok(0) => true,
            Err(error) => !matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ),
            Ok(_) => false,
        }
    });

    let root = desk("dev");
    write(
        &root,
        ".env.local",
        &format!("MODEL_URL=http://127.0.0.1:{port}/v1\n"),
    );
    let dev = Dev::start_without_database(&root);
    wait_for(&dev, "\"listening\"");

    let mut client = Client::ask(dev.port, "Do your loaves contain sesame?");
    let sent = seen.recv_timeout(Duration::from_secs(20)).expect("asked");
    assert!(
        sent.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"),
        "{sent}"
    );
    let body: serde_json::Value =
        serde_json::from_str(sent.split("\r\n\r\n").nth(1).expect("a body")).expect("JSON");
    assert_eq!(body["stream"], true);
    assert_eq!(body["max_tokens"], 600);
    assert_eq!(body["model"], "local/llama3.2");
    let head = client.head();
    assert!(head.starts_with("http/1.1 200 ok\r\n"), "{head}");
    assert!(
        head.contains("content-type: text/event-stream\r\n"),
        "{head}"
    );
    assert!(head.contains("cache-control: no-store\r\n"), "{head}");
    assert!(head.contains("x-request-id: req_"), "{head}");
    // The first piece comes while the model is still writing.
    assert_eq!(
        client.event(),
        "event: piece\ndata: {\"text\":\"Our sourdough \"}\n\n"
    );
    go_on.send(()).expect("go on");
    assert_eq!(
        client.event(),
        "event: piece\ndata: {\"text\":\"is baked beside sesame.\"}\n\n"
    );
    assert_eq!(client.event(), "event: done\ndata: {}\n\n");
    assert_eq!(client.event(), "", "the stream ends after `done`");

    // Broken off part-way: an error event, with the request's id.
    let mut client = Client::ask(dev.port, "Rye?");
    client.head();
    assert_eq!(
        client.event(),
        "event: piece\ndata: {\"text\":\"Our rye \"}\n\n"
    );
    let failed = client.event();
    assert!(
        failed.starts_with("event: error\ndata: {\"error\":{\"code\":\"unavailable\",\"message\":\"A dependency is temporarily unavailable.\",\"request_id\":\"req_"),
        "{failed}"
    );
    assert!(!failed.contains("rye"), "no answer in an error: {failed}");

    // A client that goes away closes the model's connection.
    let mut client = Client::ask(dev.port, "Spelt?");
    client.head();
    assert_eq!(
        client.event(),
        "event: piece\ndata: {\"text\":\"Our spelt \"}\n\n"
    );
    drop(client);
    assert!(
        model.join().expect("the model's thread"),
        "the model's connection stayed open"
    );
    drop(dev);
}
