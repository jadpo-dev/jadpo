//! The S3 driver against a stand-in S3 server (area 16, "The S3 driver";
//! `support::fake_s3`), which verifies every signature, presigned ones
//! included: a store's files on `Driver.s3` behave as the disk driver's
//! tests say they should. The scenario runs interpreted here, compiled in
//! `deployable.rs`, and on workerd in `workers/s3.rs`; what only a real
//! client can see of a signed address is here.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_drivers::s3::S3Store;
use jadpo_host_native::{NativeHost, Runtime};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::Host;
use jadpo_runtime::capability::{Answer, Call, Capability};
use jadpo_runtime::http::{Request, Worker, serve_request};
use jadpo_std::files::key_for;
use jadpo_std::subscriber::Repeat;
use jadpo_term::Environment;
use jadpo_value::Value;

use crate::support::fake_s3::{FakeS3, exchange};

pub(crate) const KEY_ID: &str = "AKIAJADPOSTANDIN";
pub(crate) const SECRET: &str = "stand-in-secret-access-key";
pub(crate) const BUCKET: &str = "sheets";

pub(crate) type Got = (u16, Vec<(String, String)>, Vec<u8>);

/// A request to the app: method, path, headers and body.
pub(crate) type Send<'a> = &'a dyn Fn(&str, &str, &[(&str, &str)], &[u8]) -> Got;

/// The disk driver's bakery (`disk_files.rs`) with its two stores on the
/// S3 driver, their bucket and credential read from settings.
pub(crate) fn project_files() -> Vec<(String, String)> {
    let mut files =
        crate::disk_files::bakery(Path::new("/unused/sheets"), Path::new("/unused/full"));
    let connection = |name: &str, bucket: &str| {
        format!(
            "        {name}: {{\n            driver: Driver.s3\n            host: config.s3_host\n            region: \"auto\"\n            bucket: \"{bucket}\"\n            credential: {{\n                key_id: config.s3_key_id\n                secret: config.s3_secret\n            }}\n        }}\n"
        )
    };
    for (path, text) in &mut files {
        if path == "app.jadpo" {
            let start = text
                .find("        sheets: {")
                .expect("the sheets connection");
            let tail = "    }\n}\n";
            let end = text.rfind(tail).expect("the connections' end");
            text.replace_range(
                start..end,
                &format!(
                    "{}{}",
                    connection("sheets", BUCKET),
                    connection("full", "full")
                ),
            );
        } else if path == "config.jadpo" {
            *text = "config {\n    /// The bakery's database.\n    database_url: PostgresUrl\n    /// The S3 service's address.\n    s3_host: Text\n    /// The S3 credential's key id.\n    s3_key_id: Text\n    /// The S3 credential's secret access key.\n    s3_secret: Text {\n        secret: true\n    }\n}\n".to_string();
        }
    }
    files
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    let out = run(&args, &env.with_cwd(cwd));
    assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    out
}

/// `files` written under a fresh folder of `name` and checked.
pub(crate) fn written(name: &str, files: &[(String, String)]) -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = target
        .parent()
        .unwrap_or(target)
        .join("jadpo-s3")
        .join(name);
    let _ = std::fs::remove_dir_all(root.join("database"));
    crate::disk_files::write_all(&root, files);
    jadpo(&root, &["migrate", "new", "initial"]);
    jadpo(&root, &["records", "update"]);
    jadpo(&root, &["check"]);
    root
}

/// A header of `got`, or nothing.
fn header<'a>(got: &'a Got, name: &str) -> &'a str {
    got.1
        .iter()
        .find(|(header, _)| header == name)
        .map_or("", |(_, value)| value)
}

/// The whole of what the disk driver's tests ask of a store, over `send`,
/// real HTTP to the app (or the interpreter's own requests), with `s3` the
/// stand-in the app's signed addresses point at. `key_before` is every
/// key the stand-in held when it began.
pub(crate) fn runs_on_s3(send: Send<'_>, s3: &FakeS3, key_before: &[String]) {
    const BYTES: &[u8] = b"code\nA1\nB2\n";
    let store = s3.address();

    // A file the app makes: kept under its key, its bytes the same.
    let kept = send("POST", "/sheets", &[], crate::disk_files::SHEET.as_bytes());
    assert_eq!(kept.0, 201, "{kept:?}");
    let new_keys: Vec<String> = s3
        .keys()
        .into_iter()
        .filter(|key| !key_before.contains(key))
        .collect();
    assert_eq!(new_keys.len(), 1, "{new_keys:?}");
    assert_eq!(s3.object(&new_keys[0]).expect("kept").bytes, BYTES);

    // A signed link for a file the app makes, downloaded from the store.
    let linked = send(
        "POST",
        "/sheets/linked",
        &[],
        crate::disk_files::SHEET.as_bytes(),
    );
    assert_eq!(linked.0, 201, "{linked:?}");
    let answer: serde_json::Value = serde_json::from_slice(&linked.2).expect("JSON");
    let url = answer["sheet"]["url"].as_str().expect("a link").to_string();
    assert!(url.starts_with(&format!("{store}/sheets/")), "{url}");
    assert!(url.contains("X-Amz-Signature="), "{url}");
    let back = exchange(&url, "GET", &[], b"");
    assert_eq!(back.0, 200, "{back:?}");
    assert_eq!(back.2, BYTES);
    assert_eq!(header(&back, "content-type"), "text/csv", "{back:?}");
    assert_eq!(
        header(&back, "content-disposition"),
        "attachment; filename*=UTF-8''download.csv",
        "{back:?}"
    );
    let part = exchange(&url, "GET", &[("Range", "bytes=0-3")], b"");
    assert_eq!(part.0, 206, "{part:?}");
    assert_eq!(part.2, b"code");
    // An altered link is refused by the store itself.
    let altered = exchange(
        &url.replace("X-Amz-Signature=", "X-Amz-Signature=0"),
        "GET",
        &[],
        b"",
    );
    assert_eq!(altered.0, 403, "{altered:?}");

    // An upload straight to the store, attached, then downloaded under its
    // name.
    let asked = send("POST", "/sheets/slot", &[], b"");
    assert_eq!(asked.0, 200, "{asked:?}");
    let slot: serde_json::Value = serde_json::from_slice(&asked.2).expect("a slot");
    assert_eq!(slot["max_size"], serde_json::json!(64), "{slot}");
    let url = slot["url"].as_str().expect("a url").to_string();
    let token = slot["token"].as_str().expect("a token").to_string();
    assert!(
        url.starts_with(&format!("{store}/sheets/uploads/")),
        "{url}"
    );
    assert!(url.contains("X-Amz-Signature="), "{url}");
    let sent = exchange(
        &url,
        "PUT",
        &[(
            "Content-Disposition",
            "attachment; filename*=UTF-8''prices.csv",
        )],
        BYTES,
    );
    assert_eq!(sent.0, 200, "{sent:?}");
    let body = format!(r#"{{"supplier":"Millers","sheet":"{token}"}}"#);
    let attached = send(
        "POST",
        "/sheets/attached",
        &[("Content-Type", "application/json")],
        body.as_bytes(),
    );
    assert_eq!(attached.0, 201, "{attached:?}");
    let answer: serde_json::Value = serde_json::from_slice(&attached.2).expect("JSON");
    assert_eq!(
        answer["sheet"]["size"],
        serde_json::json!(BYTES.len()),
        "{answer}"
    );
    assert_eq!(answer["sheet"]["name"], "prices.csv", "{answer}");
    let link = answer["sheet"]["url"].as_str().expect("a link").to_string();
    let back = exchange(&link, "GET", &[], b"");
    assert_eq!(back.0, 200, "{back:?}");
    assert_eq!(back.2, BYTES);
    assert_eq!(
        header(&back, "content-disposition"),
        "attachment; filename*=UTF-8''prices.csv",
        "{back:?}"
    );

    // The slot's address still signs for the upload, but it can't change
    // the attached file: the bytes were copied out of its reach.
    let again = exchange(&url, "PUT", &[], b"code\nC3\n");
    assert_eq!(again.0, 200, "{again:?}");
    let back = exchange(&link, "GET", &[], b"");
    assert_eq!(back.2, BYTES, "an attached file doesn't change");

    // Over the field's limit, or not a type it takes: refused at the
    // attach, with the bytes left where only the sweep reaches them.
    let asked = send("POST", "/sheets/slot", &[], b"");
    let slot: serde_json::Value = serde_json::from_slice(&asked.2).expect("a slot");
    let url = slot["url"].as_str().expect("a url").to_string();
    let token = slot["token"].as_str().expect("a token").to_string();
    let too_big = vec![b'a'; 200];
    assert_eq!(exchange(&url, "PUT", &[], &too_big).0, 200);
    let body = format!(r#"{{"supplier":"Millers","sheet":"{token}"}}"#);
    let refused = send(
        "POST",
        "/sheets/attached",
        &[("Content-Type", "application/json")],
        body.as_bytes(),
    );
    assert_eq!(refused.0, 422, "{refused:?}");
    let asked = send("POST", "/sheets/slot", &[], b"");
    let slot: serde_json::Value = serde_json::from_slice(&asked.2).expect("a slot");
    let url = slot["url"].as_str().expect("a url").to_string();
    let token = slot["token"].as_str().expect("a token").to_string();
    assert_eq!(exchange(&url, "PUT", &[], b"\xff\xfe not a sheet").0, 200);
    let body = format!(r#"{{"supplier":"Millers","sheet":"{token}"}}"#);
    let refused = send(
        "POST",
        "/sheets/attached",
        &[("Content-Type", "application/json")],
        body.as_bytes(),
    );
    assert_eq!(refused.0, 422, "{refused:?}");
    // An upload that never came.
    let asked = send("POST", "/sheets/slot", &[], b"");
    let slot: serde_json::Value = serde_json::from_slice(&asked.2).expect("a slot");
    let token = slot["token"].as_str().expect("a token").to_string();
    let body = format!(r#"{{"supplier":"Millers","sheet":"{token}"}}"#);
    let refused = send(
        "POST",
        "/sheets/attached",
        &[("Content-Type", "application/json")],
        body.as_bytes(),
    );
    assert_eq!(refused.0, 422, "{refused:?}");

    // A large file in parts: each part's address redirects to a signed
    // `UploadPart`, what the first part said of the file is kept, and the
    // attach joins them.
    const PART: usize = 16 * 1024 * 1024;
    const LAST: &[u8] = b"end,of,sheet\n";
    let asked = send("POST", "/sheets/archive-slot", &[], b"");
    assert_eq!(asked.0, 200, "{asked:?}");
    let slot: serde_json::Value = serde_json::from_slice(&asked.2).expect("a slot");
    assert_eq!(slot["part_size"], serde_json::json!(PART), "{slot}");
    assert!(slot.get("url").is_none(), "{slot}");
    let parts = slot["parts_url"].as_str().expect("parts_url").to_string();
    let token = slot["token"].as_str().expect("a token").to_string();
    assert!(parts.starts_with("/files/sheets/uploads/"), "{parts}");

    let first = vec![b'a'; PART];
    let redirected = send(
        "PUT",
        &format!("{parts}/1"),
        &[(
            "Content-Disposition",
            "attachment; filename*=UTF-8''archive.csv",
        )],
        &first,
    );
    assert_eq!(redirected.0, 307, "{:?}", (redirected.0, &redirected.1));
    let location = header(&redirected, "location").to_string();
    assert!(
        location.starts_with(&format!("{store}/sheets/")),
        "{location}"
    );
    assert!(location.contains("partNumber=1"), "{location}");
    assert!(location.contains("uploadId="), "{location}");
    let uploaded = exchange(&location, "PUT", &[], &first);
    assert_eq!(uploaded.0, 200, "{:?}", uploaded.0);

    // A second part, its address asked for again: the upload is one.
    let redirected = send("PUT", &format!("{parts}/2"), &[], LAST);
    assert_eq!(redirected.0, 307, "{redirected:?}");
    let location = header(&redirected, "location").to_string();
    let uploaded = exchange(&location, "PUT", &[], LAST);
    assert_eq!(uploaded.0, 200, "{uploaded:?}");
    assert_eq!(s3.open_uploads(), 1);

    let body = format!(r#"{{"supplier":"Millers","sheet":"{token}"}}"#);
    let attached = send(
        "POST",
        "/sheets/archived",
        &[("Content-Type", "application/json")],
        body.as_bytes(),
    );
    assert_eq!(
        attached.0,
        201,
        "{:?}",
        (attached.0, String::from_utf8_lossy(&attached.2))
    );
    let answer: serde_json::Value = serde_json::from_slice(&attached.2).expect("JSON");
    assert_eq!(
        answer["sheet"]["size"],
        serde_json::json!(PART + LAST.len()),
        "{answer}"
    );
    assert_eq!(answer["sheet"]["name"], "archive.csv", "{answer}");
    assert_eq!(s3.open_uploads(), 0, "the upload was completed");

    let link = answer["sheet"]["url"].as_str().expect("a link").to_string();
    let back = exchange(&link, "GET", &[], b"");
    assert_eq!(back.0, 200, "{:?}", back.0);
    assert_eq!(back.2.len(), PART + LAST.len());
    assert!(back.2.starts_with(&first) && back.2.ends_with(LAST));

    // A part sent once its file is joined.
    let late = send("PUT", &format!("{parts}/2"), &[], b"late\n");
    assert_eq!(late.0, 409, "{late:?}");
    assert!(
        String::from_utf8_lossy(&late.2).contains("upload_already_sent"),
        "{late:?}"
    );

    assert_eq!(
        s3.unverified(),
        1,
        "only the altered link failed its signature"
    );
}

/// The bakery run by the interpreter on the real S3 driver, over the
/// native host's HTTP, with the stand-in as its service: the scenario, and
/// a slot recorded as held by none the moment it is asked for.
#[test]
fn interpreted_files_on_s3_pass_the_disk_drivers_tests() {
    if !jadpo_test::live() {
        return;
    }
    let s3 = FakeS3::start(BUCKET, KEY_ID, SECRET);
    let root = written("interpreted", &project_files());
    let loaded = jadpo_db::load(&root);
    let prepared = Prepared::load(&loaded.db, loaded.project).expect("checks");
    let set = jadpo_check::data::migrations::migrations(&loaded.db, loaded.project);
    let steps = jadpo_check::data::migrations::store_steps(set);
    let server = jadpo_test::database::server().expect("a PostgreSQL server");
    let url = server
        .create_database("s3_interpreted")
        .expect("a database");
    let store = jadpo_host_native::Postgres::new(&url, Rc::new(jadpo_host_native::NativeTransport))
        .expect("a store");
    let runtime = Runtime::new().expect("a runtime");
    runtime
        .block_on(jadpo_runtime::migrate(
            &store,
            steps.get("main").expect("main's migrations"),
        ))
        .expect("migrates");
    let address = s3.address();
    let database_url = crate::deployable::as_url(&url);
    let variables = |name: &str| match name {
        "DATABASE_URL" => Some(database_url.clone()),
        "S3_HOST" => Some(address.clone()),
        "S3_KEY_ID" => Some(KEY_ID.to_string()),
        "S3_SECRET" => Some(SECRET.to_string()),
        _ => None,
    };
    let values = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|bytes: &mut [u8]| NativeHost::new().random(bytes),
        )
        .expect("the settings check");
    let program = Program::new(prepared).with_config(&values);
    let drivers = jadpo_host_native::Drivers::build(
        program.runtime_plan().connections,
        program.started(),
        &jadpo_host_native::deployable_supplies(),
    );
    assert!(drivers.unbuilt().is_empty(), "{:?}", drivers.unbuilt());
    let host = NativeHost::new()
        .with_drivers(drivers)
        .with_store("main", store);
    let worker = Worker::default();
    let app = program.app();
    let send = |method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]| -> Got {
        let mut sent: Vec<(String, String)> = headers
            .iter()
            .map(|(name, value)| (name.to_ascii_lowercase(), value.to_string()))
            .collect();
        sent.push(("content-length".to_string(), body.len().to_string()));
        if method == "POST"
            && !body.is_empty()
            && !sent.iter().any(|(name, _)| name == "content-type")
        {
            sent.push(("content-type".to_string(), "application/json".to_string()));
        }
        let request = Request {
            method: method.into(),
            path: path.into(),
            headers: sent,
            body: body.to_vec(),
            address: Some("10.0.0.1".into()),
            ..Request::default()
        };
        let response = runtime.block_on(serve_request(&app, &program, &host, &worker, &request));
        let work = response.deferred.clone();
        runtime.block_on(jadpo_runtime::runner::run_deferred(&program, &host, work));
        (
            response.status,
            response.headers.clone(),
            response.body.clone(),
        )
    };
    runs_on_s3(&send, &s3, &[]);
    // A slot asked for leaves a row saying its file is held by none, for
    // the sweep to find if it is never attached.
    let asked = send("POST", "/sheets/slot", &[], b"");
    let slot: serde_json::Value = serde_json::from_slice(&asked.2).expect("a slot");
    let url_text = slot["url"].as_str().expect("a url");
    let key = url_text
        .split("/uploads/")
        .nth(1)
        .and_then(|rest| rest.split('?').next())
        .expect("a key")
        .to_string();
    let rows = crate::deployable::rows(
        &url,
        &format!("SELECT holders FROM jadpo_files WHERE key = '{key}'"),
    );
    assert_eq!(rows, vec![vec!["Int4(0)".to_string()]], "{key}");
}

/// The driver itself against the stand-in: everything the runtime sends
/// through the seam. Runs without PostgreSQL.
#[test]
fn the_s3_driver_answers_the_files_capability() {
    let s3 = FakeS3::start(BUCKET, KEY_ID, SECRET);
    let clock_now = Rc::new(Cell::new(1_792_504_800_000i64));
    let clock = {
        let clock_now = Rc::clone(&clock_now);
        Rc::new(move || clock_now.get()) as Rc<dyn Fn() -> i64>
    };
    let store = S3Store::new(
        Rc::new(jadpo_host_native::fetch::NativeFetch::new()),
        clock,
        (&s3.address(), "auto", BUCKET),
        (KEY_ID, SECRET),
    )
    .expect("built");
    assert!(!format!("{store:?}").contains(SECRET), "never the secret");
    assert_eq!(
        store.link_key("sheets", false),
        store.link_key("sheets", true)
    );
    assert!(
        S3Store::new(
            Rc::new(jadpo_host_native::fetch::NativeFetch::new()),
            Rc::new(|| 0),
            ("http://files.example.com", "auto", BUCKET),
            (KEY_ID, SECRET),
        )
        .is_err()
    );

    let runtime = Runtime::new().expect("a runtime");
    let key = key_for(&[1; 16]);
    let call = |operation: &str, input: Vec<(&str, Value)>| {
        let fields = input
            .into_iter()
            .map(|(name, value)| (Rc::from(name), value))
            .collect::<Vec<_>>();
        let record = Value::record("FileCall", fields);
        runtime.block_on(store.call(&Call::new("sheets", operation, record, Repeat::Safe)))
    };
    let int = |value: i64| Value::Int(value);
    let text = |value: &str| Value::text(value);
    let bytes = |value: Vec<u8>| Value::Bytes(Rc::from(value));

    assert!(matches!(
        call(
            jadpo_std::files::PUT,
            vec![
                ("key", text(&key)),
                ("type", text("text/csv")),
                ("bytes", bytes(b"code\nA1\n".to_vec()))
            ]
        ),
        Answer::Value(_)
    ));
    assert_eq!(
        s3.object(&key).expect("kept").content_type.as_deref(),
        Some("text/csv")
    );
    let Answer::Value(Value::Bytes(read)) = call(
        jadpo_std::files::READ,
        vec![("key", text(&key)), ("from", int(5)), ("length", int(2))],
    ) else {
        panic!("a read")
    };
    assert_eq!(&*read, b"A1");
    // Fewer than asked at the end; nothing past it; a file it doesn't hold.
    let Answer::Value(Value::Bytes(read)) = call(
        jadpo_std::files::READ,
        vec![("key", text(&key)), ("from", int(5)), ("length", int(100))],
    ) else {
        panic!("a read")
    };
    assert_eq!(&*read, b"A1\n");
    let Answer::Value(Value::Bytes(read)) = call(
        jadpo_std::files::READ,
        vec![("key", text(&key)), ("from", int(500)), ("length", int(10))],
    ) else {
        panic!("a read")
    };
    assert!(read.is_empty());
    assert!(matches!(
        call(
            jadpo_std::files::READ,
            vec![("key", text(&key_for(&[2; 16]))), ("from", int(0)), ("length", int(10))]
        ),
        Answer::Failed(code) if code == jadpo_std::files::NOT_FOUND
    ));

    // An export's pieces, joined past one part's size so they are re-cut
    // into equal parts, and joined again doing nothing.
    let id = [3u8; 16];
    let joined = key_for(&id);
    let first_piece = vec![b'x'; 10 * 1024 * 1024];
    let second_piece = vec![b'y'; 7 * 1024 * 1024 + 11];
    for (n, piece) in [first_piece.clone(), second_piece.clone()]
        .into_iter()
        .enumerate()
    {
        let piece_key = jadpo_std::files::piece_key(&id, n as u64);
        let put = call(
            jadpo_std::files::PUT,
            vec![
                ("key", text(&piece_key)),
                ("type", text("text/csv")),
                ("bytes", bytes(piece)),
            ],
        );
        assert!(matches!(put, Answer::Value(_)), "{put:?}");
    }
    let Answer::Value(Value::Text(hash)) = call(
        jadpo_std::files::JOIN,
        vec![
            ("key", text(&joined)),
            ("pieces", int(2)),
            ("type", text("text/csv")),
        ],
    ) else {
        panic!("joined")
    };
    assert!(!hash.is_empty());
    let whole = s3.object(&joined).expect("joined").bytes;
    assert_eq!(whole.len(), first_piece.len() + second_piece.len());
    assert!(whole.starts_with(&first_piece) && whole.ends_with(&second_piece));
    assert!(
        s3.object(&jadpo_std::files::piece_key(&id, 0)).is_none(),
        "its pieces went"
    );
    assert!(matches!(
        call(
            jadpo_std::files::JOIN,
            vec![
                ("key", text(&joined)),
                ("pieces", int(2)),
                ("type", text("text/csv"))
            ],
        ),
        Answer::Value(Value::Unit)
    ));
    // A missing piece, and no file.
    let other = key_for(&[4; 16]);
    assert!(matches!(
        call(
            jadpo_std::files::JOIN,
            vec![("key", text(&other)), ("pieces", int(1)), ("type", text("text/csv"))],
        ),
        Answer::Failed(code) if code == jadpo_std::files::NOT_FOUND
    ));
    // A small export joined whole, in one put.
    let small_id = [5u8; 16];
    let small = key_for(&small_id);
    let piece_key = jadpo_std::files::piece_key(&small_id, 0);
    call(
        jadpo_std::files::PUT,
        vec![
            ("key", text(&piece_key)),
            ("type", text("text/csv")),
            ("bytes", bytes(b"tiny".to_vec())),
        ],
    );
    assert!(matches!(
        call(
            jadpo_std::files::JOIN,
            vec![
                ("key", text(&small)),
                ("pieces", int(1)),
                ("type", text("text/csv"))
            ],
        ),
        Answer::Value(Value::Text(_))
    ));
    assert_eq!(s3.object(&small).expect("joined").bytes, b"tiny");

    // A cancelled export's pieces are dropped, and the file if joined.
    assert!(matches!(
        call(
            jadpo_std::files::DISCARD,
            vec![("key", text(&small)), ("pieces", int(1))]
        ),
        Answer::Value(_)
    ));
    assert!(s3.object(&small).is_none());

    // The erasure log's keys.
    let log = jadpo_std::erasure::log_key("Member", "3fa2c1d0-0000-4000-8000-000000000001");
    let put = call(
        jadpo_std::files::PUT,
        vec![
            ("key", text(&log)),
            ("type", text("text/plain")),
            ("bytes", bytes(b"{}".to_vec())),
        ],
    );
    assert!(matches!(put, Answer::Value(_)), "{put:?}");
    let Answer::Value(Value::List(keys)) = call(jadpo_std::files::LIST, vec![]) else {
        panic!("a list")
    };
    assert_eq!(keys.len(), 1, "{keys:?}");

    // A delete: the file, and what an upload left of it; a repeat is fine.
    assert!(matches!(
        call(jadpo_std::files::DELETE, vec![("key", text(&key))]),
        Answer::Value(_)
    ));
    assert!(s3.object(&key).is_none());
    assert!(matches!(
        call(jadpo_std::files::DELETE, vec![("key", text(&key))]),
        Answer::Value(_)
    ));

    assert_eq!(s3.unverified(), 0, "every request's signature verified");
}

/// The bakery as a native deployable on the S3 driver, against the
/// stand-in: built, migrated and served over real HTTP.
#[cfg(unix)]
#[test]
fn a_compiled_deployable_passes_the_disk_drivers_tests_on_s3() {
    if !jadpo_test::live() {
        return;
    }
    let s3 = FakeS3::start(BUCKET, KEY_ID, SECRET);
    let root = written("compiled", &project_files());
    let output = run(
        &["build".to_string(), "--json".to_string()],
        &crate::deployable::at(&root, &crate::deployable::shared_cache()),
    );
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("build/manifest.json")).expect("read"),
    )
    .expect("JSON");
    assert!(
        manifest["deployables"]["app"].get("one_server").is_none(),
        "an S3 store doesn't keep a deployable on one server: {manifest}"
    );

    let server = jadpo_test::postgres::Server::start().expect("a server");
    let connection = server.create_database("s3_compiled").expect("a database");
    crate::deployable::apply(&connection, &crate::deployable::migrations(&root));
    let url = crate::deployable::as_url(&connection);
    let host = s3.address();
    let settings = [
        ("DATABASE_URL", url.as_str()),
        ("HOST", "127.0.0.1"),
        ("PORT", "0"),
        ("S3_HOST", host.as_str()),
        ("S3_KEY_ID", KEY_ID),
        ("S3_SECRET", SECRET),
    ];
    let binary = root.join("build/target/app/app");
    let (mut child, lines) = crate::deployable::start(&binary, &settings);
    let started = crate::deployable::next(&lines, 60);
    assert_eq!(started["event"], "runtime.started", "{started}");
    assert_eq!(started["unbuilt"], serde_json::json!([]), "{started}");
    let address = started["address"]
        .as_str()
        .expect("its address")
        .to_string();
    let send = |method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]| -> Got {
        let (status, head, body) = crate::deployable::exchanged_with(
            &address,
            method,
            path,
            headers,
            &String::from_utf8_lossy(body),
        );
        let all = head
            .lines()
            .skip(1)
            .filter_map(|line| line.split_once(": "))
            .map(|(name, value)| (name.to_ascii_lowercase(), value.to_string()))
            .collect();
        (status, all, body.into_bytes())
    };
    runs_on_s3(&send, &s3, &[]);
    let stopped = std::process::Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(stopped.success());
    assert_eq!(crate::deployable::ended(&mut child, 30), Some(0));
    // The credential is nowhere in what the deployable logged.
    while let Ok(line) = lines.try_recv() {
        assert!(!line.to_string().contains(SECRET), "{line}");
    }
}
