//! The `telemetry:` section over OTLP (area 16, "Telemetry"; area 17,
//! "Traces, metrics and logs in development"; chunk O1). One project, a
//! greeter that counts its greetings by whether they were warm, notes a
//! fact on its trace and refuses to greet nobody, sends the same traces,
//! metrics and logs to a collector double on this machine from `jadpo dev
//! --export-telemetry` here, interpreted, and from its compiled native
//! deployable in `deployable.rs`: a span for the route and the action,
//! the request's duration, the action's and the app's own counter, and the
//! failure's log event, carrying no value from the app's data. Nothing is
//! sent anywhere else.
//!
//! `jadpo dev` keeps them on the machine besides, which `jadpo traces`,
//! `jadpo metrics` and `jadpo logs` read; and a personal field given as a
//! label or an attribute is refused by `jadpo check`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jadpo_cli::run;
use jadpo_term::Environment;
use serde_json::Value;

use super::http::{Dev, tempdir};

/// What the visitor's name is: a value no payload may carry.
pub(crate) const NAME: &str = "Ada Lovelace";

/// The project's files: `application` given, the rest fixed.
pub(crate) fn project_at(root: &Path, application: &str) {
    let files = [
        ("app.jadpo", application),
        ("config.jadpo", "config {\n    collector: Url\n}\n"),
        (
            "policy.jadpo",
            "use greeting\n\npolicy {\n    actions: {\n        greeting.greet: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/greeting/types/words.jadpo",
            "/// What the greeting says.\ntype Words = Text {}\n",
        ),
        (
            "domains/greeting/types/hello.jadpo",
            "use greeting/types { Words }\n\n/// A greeting.\ntype Hello = Output {\n    words: Words\n}\n",
        ),
        (
            "domains/greeting/types/visit.jadpo",
            "use greeting/types { Words }\n\n/// Who is visiting.\ntype Visit = Input {\n    name: Words\n}\n",
        ),
        (
            "domains/greeting/failures/nobody_to_greet.jadpo",
            "/// There is no one to greet.\ntype NobodyToGreet = Failure {\n    kind: FailureKind.invalid_value\n    message: \"There is no one to greet.\"\n}\n",
        ),
        (
            "domains/greeting/monitoring.jadpo",
            "/// Greetings given, by whether they were warm.\nmetric greetings = Counter {\n    by: [warm]\n}\n",
        ),
        (
            "domains/greeting/actions/greet.jadpo",
            "use greeting/types { Hello, Visit }\nuse greeting/failures { NobodyToGreet }\n\n/// Greets whoever asks.\naction greet(visit: Visit) fails NobodyToGreet -> Hello {\n    if (visit.name == \"Nemo\") {\n        reject NobodyToGreet\n    }\n    telemetry.greetings.add(1, warm: true)\n    telemetry.note(greeted: 1)\n    return Hello {\n        words: visit.name\n    }\n}\n",
        ),
        (
            "domains/greeting/routes/greeting.jadpo",
            "use greeting/types { Hello, Visit }\nuse greeting/actions { greet }\n\n/// The greeting.\nroutes Greeting {\n    prefix: /greeting\n\n    /// Greets.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: Visit\n        output: Hello\n        run: greet(input)\n    }\n}\n",
        ),
    ];
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("folders");
        if std::fs::read_to_string(&path).ok().as_deref() != Some(text) {
            std::fs::write(&path, text).expect("written");
        }
    }
}

/// The application `jadpo dev` and a native deployable run: its endpoint
/// a setting, its logs from `LogLevel.info`.
pub(crate) const APPLICATION: &str = "application {\n    telemetry: {\n        endpoint: config.collector\n        send: {\n            logs: LogLevel.info\n        }\n    }\n}\n";

/// One request the collector was sent.
#[derive(Clone, Debug)]
pub(crate) struct Received {
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// A collector double on this machine: it answers every `POST` 200 and
/// keeps what it was sent.
pub(crate) struct Collector {
    pub address: String,
    received: Arc<Mutex<Vec<Received>>>,
}

impl Collector {
    pub(crate) fn start() -> Collector {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let address = format!("http://{}", listener.local_addr().expect("an address"));
        let received = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&received);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let kept = Arc::clone(&kept);
                std::thread::spawn(move || answer(stream, &kept));
            }
        });
        Collector { address, received }
    }

    pub(crate) fn received(&self) -> Vec<Received> {
        self.received.lock().expect("kept").clone()
    }

    /// Waits up to 30 seconds until `done` holds of what it was sent.
    pub(crate) fn wait_for(&self, done: impl Fn(&[Received]) -> bool) -> Vec<Received> {
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            let received = self.received();
            if done(&received) {
                return received;
            }
            assert!(
                Instant::now() < until,
                "the collector wasn't sent what was waited for: {:#?}",
                received
                    .iter()
                    .map(|one| one.path.as_str())
                    .collect::<Vec<_>>()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Reads each request on `stream`, keeping it, and answers 200.
fn answer(stream: TcpStream, kept: &Mutex<Vec<Received>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut writer = stream.try_clone().expect("a stream");
    let mut reader = BufReader::new(stream);
    loop {
        let mut first = String::new();
        if reader.read_line(&mut first).unwrap_or(0) == 0 {
            return;
        }
        let path = first.split(' ').nth(1).unwrap_or_default().to_string();
        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                headers.push((name.trim().to_lowercase(), value.trim().to_string()));
            }
        }
        let length: usize = headers
            .iter()
            .find(|(name, _)| name == "content-length")
            .and_then(|(_, value)| value.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0; length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        kept.lock().expect("kept").push(Received {
            path,
            headers,
            body: String::from_utf8_lossy(&body).to_string(),
        });
        let _ = writer.write_all(
            b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\n\r\n{}",
        );
        let _ = writer.flush();
    }
}

/// Every span, metric or log record in what was sent on `path`.
fn parts(received: &[Received], path: &str) -> Vec<Value> {
    received
        .iter()
        .filter(|one| one.path == path)
        .filter_map(|one| serde_json::from_str::<Value>(&one.body).ok())
        .collect()
}

fn spans(payloads: &[Value]) -> Vec<Value> {
    payloads
        .iter()
        .flat_map(|payload| {
            payload["resourceSpans"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .flat_map(|resource| {
            resource["scopeSpans"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .flat_map(|scope| scope["spans"].as_array().cloned().unwrap_or_default())
        .collect()
}

fn metrics(payloads: &[Value]) -> Vec<Value> {
    payloads
        .iter()
        .flat_map(|payload| {
            payload["resourceMetrics"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .flat_map(|resource| {
            resource["scopeMetrics"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .flat_map(|scope| scope["metrics"].as_array().cloned().unwrap_or_default())
        .collect()
}

fn records(payloads: &[Value]) -> Vec<Value> {
    payloads
        .iter()
        .flat_map(|payload| {
            payload["resourceLogs"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .flat_map(|resource| {
            resource["scopeLogs"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .flat_map(|scope| scope["logRecords"].as_array().cloned().unwrap_or_default())
        .collect()
}

/// An attribute's value in an OTLP attribute list.
fn attribute<'a>(list: &'a Value, key: &str) -> Option<&'a Value> {
    list.as_array()?
        .iter()
        .find(|attribute| attribute["key"] == key)
        .map(|attribute| &attribute["value"])
}

/// Sends a greeting and a refusal through `send` (method, path, body: the
/// status), then checks what the collector was sent: on every host the
/// same traces, metrics and logs, none carrying the visitor's name.
pub(crate) fn reaches_the_collector(collector: &Collector, send: impl Fn(&str, &str, &str) -> u16) {
    let greeted = send("POST", "/greeting", &format!("{{\"name\": \"{NAME}\"}}"));
    assert_eq!(greeted, 200);
    let refused = send("POST", "/greeting", "{\"name\": \"Nemo\"}");
    assert_eq!(refused, 422);
    let received = collector.wait_for(|received| {
        let traces = spans(&parts(received, "/v1/traces"));
        let logged = records(&parts(received, "/v1/logs"));
        traces.iter().filter(|span| span["kind"] == 2).count() >= 2
            && parts(received, "/v1/metrics").len() >= 2
            && !logged.is_empty()
    });
    for one in &received {
        assert!(
            one.headers
                .iter()
                .any(|(name, value)| name == "content-type" && value == "application/json"),
            "{one:?}"
        );
        assert!(!one.body.contains(NAME), "a value was sent: {}", one.body);
        assert!(!one.body.contains("Nemo"), "a value was sent: {}", one.body);
    }
    // A span for the route and one for its action under it.
    let traces = spans(&parts(&received, "/v1/traces"));
    let roots: Vec<&Value> = traces.iter().filter(|span| span["kind"] == 2).collect();
    let greeting = roots
        .iter()
        .find(|span| {
            attribute(&span["attributes"], "http.response.status_code")
                == Some(&serde_json::json!({ "intValue": "200" }))
        })
        .expect("the greeting's trace");
    assert_eq!(greeting["name"], "POST /greeting");
    assert_eq!(
        attribute(&greeting["attributes"], "greeted"),
        Some(&serde_json::json!({ "intValue": "1" }))
    );
    let action = traces
        .iter()
        .find(|span| span["name"] == "greet" && span["traceId"] == greeting["traceId"])
        .expect("the action's span");
    assert_eq!(action["parentSpanId"], greeting["spanId"]);
    assert_eq!(
        attribute(&action["attributes"], "jadpo.name"),
        Some(&serde_json::json!({ "stringValue": "greeting.greet" }))
    );
    let refusal = roots
        .iter()
        .find(|span| {
            attribute(&span["attributes"], "jadpo.failure")
                == Some(&serde_json::json!({ "stringValue": "nobody_to_greet" }))
        })
        .expect("the refusal's trace, sent whole as it failed");
    assert_eq!(refusal["name"], "POST /greeting");
    // The request's duration, the action's and the app's own counter.
    let metrics = metrics(&parts(&received, "/v1/metrics"));
    for name in [
        "http.server.request.duration",
        "jadpo.run.duration",
        "greeting.greetings",
        "jadpo.failures",
    ] {
        assert!(
            metrics.iter().any(|metric| metric["name"] == name),
            "{name} wasn't sent"
        );
    }
    let counted = metrics
        .iter()
        .find(|metric| metric["name"] == "greeting.greetings")
        .expect("the app's counter");
    assert_eq!(counted["sum"]["isMonotonic"], true);
    let point = &counted["sum"]["dataPoints"][0];
    assert_eq!(point["asInt"], "1");
    assert_eq!(
        attribute(&point["attributes"], "warm"),
        Some(&serde_json::json!({ "boolValue": true }))
    );
    // The refusal's log event, with its code and never a value.
    let logged = records(&parts(&received, "/v1/logs"));
    let rejected = logged
        .iter()
        .find(|record| record["body"]["stringValue"] == "failure.rejected")
        .expect("the refusal logged");
    assert_eq!(rejected["severityText"], "INFO");
    assert_eq!(
        attribute(&rejected["attributes"], "code"),
        Some(&serde_json::json!({ "stringValue": "nobody_to_greet" }))
    );
}

/// `jadpo dev --export-telemetry`, interpreted, sends to the collector
/// what a deployable sends, and keeps it on the machine, where `jadpo
/// traces`, `metrics` and `logs` read it.
#[test]
fn jadpo_dev_sends_spans_metrics_and_logs_and_keeps_them() {
    let collector = Collector::start();
    let dir = tempdir::Dir::new("telemetry-dev");
    project_at(dir.path(), APPLICATION);
    std::fs::write(
        dir.path().join(".env.local"),
        format!("COLLECTOR={}\n", collector.address),
    )
    .expect("written");
    let env = Environment::empty().with_cwd(dir.path());
    let records = run(&["records".to_string(), "update".to_string()], &env);
    assert_eq!(records.code, 0, "{}{}", records.stdout, records.stderr);
    let dev = Dev::start_exporting(dir.path());
    reaches_the_collector(&collector, |method, path, body| {
        dev.send(method, path, &[], body).status
    });
    let jadpo = |args: &[&str]| {
        let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        let output = run(&args, &env);
        assert_eq!(
            output.code, 0,
            "{args:?}: {}{}",
            output.stdout, output.stderr
        );
        output.stdout
    };
    let traces = jadpo(&["traces"]);
    assert!(traces.contains("POST /greeting"), "{traces}");
    assert!(traces.contains("nobody_to_greet"), "{traces}");
    assert!(traces.contains("2 requests"), "{traces}");
    let failed = jadpo(&["traces", "--failure", "nobody_to_greet", "--json"]);
    let failed: Value = serde_json::from_str(&failed).expect("JSON");
    assert_eq!(failed["data"]["traces"].as_array().map(Vec::len), Some(1));
    let none = jadpo(&["traces", "--action", "nothing_like_it"]);
    assert!(none.contains("No traces match"), "{none}");
    let metrics = jadpo(&["metrics", "--action", "greet"]);
    assert!(metrics.contains("greeting.greetings"), "{metrics}");
    assert!(metrics.contains("warm=true"), "{metrics}");
    let logs = jadpo(&["logs", "--failure", "nobody_to_greet"]);
    assert!(logs.contains("failure.rejected"), "{logs}");
    assert!(logs.contains("code=nobody_to_greet"), "{logs}");
    for kept in ["traces", "metrics", "logs"] {
        let text =
            std::fs::read_to_string(dir.path().join(format!("build/dev/telemetry/{kept}.jsonl")))
                .expect("kept");
        assert!(!text.contains(NAME), "{kept} kept a value");
    }
    drop(dev);
}

/// With nowhere to send, `jadpo dev --export-telemetry` doesn't start, and
/// says which variable to set.
#[test]
fn exporting_with_nowhere_to_send_stops_jadpo_dev() {
    let dir = tempdir::Dir::new("telemetry-nowhere");
    project_at(
        dir.path(),
        "application {\n    telemetry: {\n        send: {\n            metrics: false\n        }\n    }\n}\n",
    );
    std::fs::write(
        dir.path().join(".env.local"),
        "COLLECTOR=http://127.0.0.1:9\n",
    )
    .expect("written");
    let args = jadpo_cli::dev::DevArgs {
        cwd: dir.path().to_path_buf(),
        port: 0,
        url: None,
        no_run: false,
        json: false,
        choice: jadpo_term::ColorChoice::Never,
        fake: Vec::new(),
        sandbox: Vec::new(),
        export_telemetry: true,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = jadpo_cli::dev::serve_to(&args, &Environment::empty(), &mut out, &mut err, &|| true);
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out),
        String::from_utf8_lossy(&err)
    );
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("OTEL_EXPORTER_OTLP_ENDPOINT"), "{said}");
}

/// A personal field given as a metric's label or a note's attribute is
/// refused where it is written; so is a label that isn't an enum or a
/// `Bool`.
#[test]
fn a_personal_field_given_as_a_label_is_refused() {
    let dir = tempdir::Dir::new("telemetry-personal");
    project_at(dir.path(), APPLICATION);
    std::fs::write(
        dir.path().join("domains/greeting/actions/greet.jadpo"),
        "use greeting/types { Hello, Visit }\n\n/// Greets whoever asks.\naction greet(visit: Visit) -> Hello {\n    telemetry.greetings.add(1, warm: visit.name)\n    return Hello {\n        words: visit.name\n    }\n}\n",
    )
    .expect("written");
    std::fs::write(
        dir.path().join("domains/greeting/types/visit.jadpo"),
        "/// Who is visiting.\ntype Visit = Input {\n    name: PersonName\n}\n",
    )
    .expect("written");
    std::fs::write(
        dir.path().join("domains/greeting/types/hello.jadpo"),
        "/// A greeting.\ntype Hello = Output {\n    words: PersonName\n}\n",
    )
    .expect("written");
    let env = Environment::empty().with_cwd(dir.path());
    let checked = run(&["check".to_string(), "--json".to_string()], &env);
    assert_ne!(checked.code, 0, "{}", checked.stdout);
    let report: Value = serde_json::from_str(&checked.stdout).expect("JSON");
    let rules: Vec<&str> = report["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter_map(|diagnostic| diagnostic["rule"].as_str())
        .collect();
    assert!(rules.contains(&"telemetry.personal_data"), "{rules:?}");
}
