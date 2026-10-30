//! Structured values stored as JSON against live PostgreSQL (area 5,
//! "Storage types"; chunk 5.9b): an entity storing an object with a rule
//! across its fields, a list, a map, a set and an enum with data, each in
//! one `jsonb` column holding its type's contract JSON, written and read
//! by the route codec. The runtime answers routes as a deployable does
//! (`http::serve_request`) on the native host:
//!
//! - what a request sends is kept and read back unchanged, a set and a
//!   map's keys in their canonical order, `none` as the column's `NULL`;
//! - the column holds exactly the JSON a response sends;
//! - a row edited outside Jadpo to break any of the type's rules, or the
//!   field's own, is a fault when read, named by the rule and where it
//!   broke, never the value; so are JSON's own `null` for `none`, a value
//!   of another shape, and one past the size a value kept as JSON may be.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live tests.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use jadpo_host_native::{NativeHost, Postgres, Runtime};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::http::{Request, Response, Worker, serve_request};
use jadpo_runtime::{Host, SqlValue};

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/shipping/types/tag.jadpo",
        "type Tag = Text {\n    min_length: 1\n    max_length: 20\n}\n",
    ),
    (
        "domains/shipping/types/tags.jadpo",
        "use shipping/types { Tag }\n\ntype Tags = List<Tag> {\n    max_length: 3\n}\n",
    ),
    (
        "domains/shipping/types/grams.jadpo",
        "type Grams = Int {\n    min: 0\n    max: 100000\n}\n",
    ),
    (
        "domains/shipping/types/shelf.jadpo",
        "type Shelf = Text {\n    pattern: \"[A-Z][0-9]\"\n}\n",
    ),
    (
        "domains/shipping/types/label.jadpo",
        r#"use shipping/types { Grams, Tag }

/// What is printed on a parcel.
type Label = Object {
    line: Tag
    note: Tag?
    weight: Grams
    limit: Grams
    price: Money
    rule within_limit: weight <= limit
}
"#,
    ),
    (
        "domains/shipping/types/delivery.jadpo",
        r#"use shipping/types { Grams, Tag }

/// How a parcel leaves.
type Delivery = Enum {
    collect
    courier: {
        company: Tag
        weight: Grams
    }
}
"#,
    ),
    (
        "domains/shipping/entities/parcel.jadpo",
        r#"use shipping/types { Delivery, Grams, Label, Shelf, Tag, Tags }

entity Parcel {
    id: Uuid
    label: Label
    tags: Tags
    weights: Map<Tag, Grams> {
        max_length: 4
    }
    shelves: Set<Shelf>
    delivery: Delivery
    spare: Label?

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
"#,
    ),
    (
        "domains/shipping/types/parcel_view.jadpo",
        r#"use shipping/entities { Parcel }

/// A parcel as clients see it.
type ParcelView = Output {
    pick: [
        Parcel.id,
        Parcel.label,
        Parcel.tags,
        Parcel.weights,
        Parcel.shelves,
        Parcel.delivery,
        Parcel.spare
    ]
}
"#,
    ),
    (
        "domains/shipping/actions/send_parcel.jadpo",
        r#"use shipping/entities { Parcel }
use shipping/types { Delivery, Grams, Label, ParcelView, Shelf, Tag, Tags }

/// What sending a parcel takes.
type SendParcel = Input {
    label: Label
    tags: Tags
    weights: Map<Tag, Grams> {
        max_length: 4
    }
    shelves: Set<Shelf>
    delivery: Delivery
    spare: Label {
        optional: true
    }
}

/// Keeps a parcel.
action send_parcel(input: SendParcel) -> ParcelView {
    var parcel: Parcel = Parcel.create {
        set: {
            label: input.label
            tags: input.tags
            weights: input.weights
            shelves: input.shelves
            delivery: input.delivery
            spare: input.spare
        }
    }
    return ParcelView {
        id: parcel.id
        label: parcel.label
        tags: parcel.tags
        weights: parcel.weights
        shelves: parcel.shelves
        delivery: parcel.delivery
        spare: parcel.spare
    }
}
"#,
    ),
    (
        "domains/shipping/failures/parcel_not_found.jadpo",
        r#"use shipping/entities { Parcel }

type ParcelNotFound = Failure {
    kind: FailureKind.not_found
    message: "Parcel not found."
    client: {
        parcel_id: Parcel.id
    }
}
"#,
    ),
    (
        "domains/shipping/queries/find_parcel.jadpo",
        r#"use shipping/entities { Parcel }
use shipping/types { ParcelView }
use shipping/failures { ParcelNotFound }

query find_parcel(parcel_id: Parcel.id) fails ParcelNotFound -> ParcelView {
    from: Parcel
    where: Parcel.id == parcel_id
    missing: ParcelNotFound {
        parcel_id: parcel_id
    }
}
"#,
    ),
    (
        "domains/shipping/routes/parcels.jadpo",
        r#"use shipping/entities { Parcel }
use shipping/types { ParcelView }
use shipping/actions { SendParcel, send_parcel }
use shipping/queries { find_parcel }

/// Parcels: keeping one and reading it back.
routes Parcels {
    prefix: /parcels

    /// Keeps a parcel.
    POST / {
        authentication: RouteAuthentication.none
        input: SendParcel
        output: ParcelView
        status: SuccessStatus.created
        run: send_parcel(input)
    }

    /// One parcel.
    GET /{parcel_id: Parcel.id} {
        authentication: RouteAuthentication.none
        output: ParcelView
        run: find_parcel(parcel_id)
    }
}
"#,
    ),
];

/// A parcel with every structured value given: a set and a map written
/// out of their canonical order, and a field that is `none` in the spare
/// label.
const SENT: &str = r#"{
    "label": {"line": "Rye loaf", "note": "Keep dry", "weight": 800, "limit": 1000, "price": {"amount": 1050, "currency": "GBP"}},
    "tags": ["bread", "fragile"],
    "weights": {"loaf": 800, "box": 120},
    "shelves": ["B2", "A1"],
    "delivery": {"tag": "courier", "company": "Swift", "weight": 920},
    "spare": {"line": "Spare", "note": null, "weight": 0, "limit": 0, "price": {"amount": 0, "currency": "EUR"}}
}"#;

fn jadpo(cwd: &std::path::Path, args: &[&str]) {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let env = jadpo_term::Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", "1792504800")
        .with_cwd(cwd);
    let done = jadpo_cli::run(&args, &env);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
}

/// The project, migrated into a database of its own and served by the
/// runtime.
struct Live {
    dir: PathBuf,
    program: Program,
    host: NativeHost,
    runtime: Runtime,
    worker: Worker,
}

impl Live {
    fn new(name: &str) -> Live {
        let dir = std::env::temp_dir().join(format!(
            "jadpo-json-columns-5.9b-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("shipping");
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
        let set = jadpo_check::data::migrations::migrations(&loaded.db, loaded.project);
        let steps = jadpo_check::data::migrations::store_steps(set);
        let server = jadpo_test::database::server().expect("a PostgreSQL server");
        let url = server
            .create_database(&format!("json_columns_{name}"))
            .expect("a database");
        let store =
            Postgres::new(&url, Rc::new(jadpo_host_native::NativeTransport)).expect("a store");
        let runtime = Runtime::new().expect("a runtime");
        runtime
            .block_on(jadpo_runtime::migrate(
                &store,
                steps.get("main").expect("main's migrations"),
            ))
            .expect("migrates");
        let values = prepared
            .config(
                &BTreeMap::new(),
                jadpo_runtime::config::Mode::Environment,
                &|bytes: &mut [u8]| NativeHost::new().random(bytes),
            )
            .expect("the settings check");
        let program = Program::new(Arc::clone(&prepared)).with_config(&values);
        Live {
            dir,
            program,
            host: NativeHost::new().with_store("main", store),
            runtime,
            worker: Worker::default(),
        }
    }

    fn send(&self, request: &Request) -> Response {
        let app = self.program.app();
        self.runtime.block_on(serve_request(
            &app,
            &self.program,
            &self.host,
            &self.worker,
            request,
        ))
    }

    fn rows(&self, statement: &str) -> Vec<jadpo_runtime::Row> {
        let database = self.host.database("main").expect("main");
        self.runtime
            .block_on(database.query_once(statement, &[]))
            .expect("runs")
    }

    /// Keeps a parcel from `body`, giving its id.
    fn keep(&self, body: &str) -> String {
        let response = self.send(&request("POST", "/parcels", body));
        assert_eq!(response.status, 201, "{}", text(&response));
        json(&response)["id"].as_str().expect("an id").to_string()
    }

    fn read(&self, id: &str) -> Response {
        self.send(&request("GET", &format!("/parcels/{id}"), ""))
    }

    /// The stored column `column` of parcel `id`, as JSON.
    fn stored(&self, id: &str, column: &str) -> serde_json::Value {
        let rows = self.rows(&format!(
            "SELECT \"{column}\"::text FROM \"parcel\" WHERE \"id\" = '{id}'"
        ));
        match rows.first().and_then(|row| row.first()) {
            Some(SqlValue::Text(text)) => serde_json::from_str(text).expect("json"),
            Some(SqlValue::Null(_)) => serde_json::Value::Null,
            other => panic!("{other:?}"),
        }
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn request(method: &str, path: &str, body: &str) -> Request {
    let mut headers = vec![("origin".to_string(), "https://shipping.example".to_string())];
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

fn text(response: &Response) -> String {
    String::from_utf8_lossy(&response.body).into_owned()
}

fn json(response: &Response) -> serde_json::Value {
    serde_json::from_slice(&response.body)
        .unwrap_or_else(|_| panic!("not JSON ({}): {}", response.status, text(response)))
}

#[test]
fn an_entity_stores_an_object_a_list_a_map_and_an_enum_with_data_and_reads_each_back_unchanged() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("round_trip");
    let id = live.keep(SENT);
    let read = live.read(&id);
    assert_eq!(read.status, 200, "{}", text(&read));
    let read = json(&read);
    let mut sent: serde_json::Value = serde_json::from_str(SENT).expect("json");
    // A set is read in its canonical order; a map compares by its entries.
    sent["shelves"] = serde_json::json!(["A1", "B2"]);
    for field in ["label", "tags", "weights", "shelves", "delivery", "spare"] {
        assert_eq!(read[field], sent[field], "{field}");
        // The column holds the contract's JSON a response sends.
        assert_eq!(live.stored(&id, field), read[field], "{field} as stored");
    }
    // The set and the map are written in their canonical order.
    let rows = live.rows(&format!(
        "SELECT \"shelves\"::text, \"weights\"::text FROM \"parcel\" WHERE \"id\" = '{id}'"
    ));
    assert_eq!(
        rows[0],
        vec![
            SqlValue::Text(r#"["A1", "B2"]"#.into()),
            SqlValue::Text(r#"{"box": 120, "loaf": 800}"#.into())
        ]
    );

    // Another, with a variant that carries no data and no spare label,
    // which is the column's `NULL`, never JSON's `null`.
    let plain = SENT
        .replace(
            r#"{"tag": "courier", "company": "Swift", "weight": 920}"#,
            r#""collect""#,
        )
        .replace(
            r#",
    "spare": {"line": "Spare", "note": null, "weight": 0, "limit": 0, "price": {"amount": 0, "currency": "EUR"}}"#,
            "",
        );
    let other = live.keep(&plain);
    let read = json(&live.read(&other));
    assert_eq!(read["delivery"], serde_json::json!("collect"));
    assert_eq!(read["spare"], serde_json::Value::Null);
    let rows = live.rows(&format!(
        "SELECT \"spare\" IS NULL FROM \"parcel\" WHERE \"id\" = '{other}'"
    ));
    assert_eq!(rows[0], vec![SqlValue::Bool(true)]);
}

#[test]
fn a_row_edited_outside_jadpo_to_break_a_rule_is_a_fault_when_read() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("rules_on_read");
    let id = live.keep(SENT);
    // Each edit, what the fault names, and a part of the edit the fault
    // never shows.
    let label = r#"{"line": "Rye loaf", "note": null, "weight": 800, "limit": 1000, "price": {"amount": 1050, "currency": "GBP"}}"#;
    let cases = [
        // The object's rule across its fields.
        (
            "label",
            label.replace("\"weight\": 800", "\"weight\": 4321"),
            "within_limit",
            "4321",
        ),
        // A field of the object.
        (
            "label",
            label.replace("Rye loaf", "Wholemeal sourdough rye loaf"),
            "`max_length` on `line`",
            "sourdough",
        ),
        // Money inside it.
        ("label", label.replace("GBP", "QQQ"), "currency", "QQQ"),
        // The list's own length, and an item's.
        (
            "tags",
            r#"["bread", "fragile", "heavy", "secret-tag"]"#.to_string(),
            "`max_length`",
            "secret-tag",
        ),
        (
            "tags",
            r#"["bread", ""]"#.to_string(),
            "`min_length`",
            "bread",
        ),
        // A map's value, and the field's own rule on its entries.
        ("weights", r#"{"loaf": -77}"#.to_string(), "`min`", "-77"),
        (
            "weights",
            r#"{"a": 1, "b": 2, "c": 3, "d": 4, "secret-key": 5}"#.to_string(),
            "`max_length`",
            "secret-key",
        ),
        // A set's item.
        ("shelves", r#"["zz9"]"#.to_string(), "`pattern`", "zz9"),
        // A variant's data, and a variant that isn't one.
        (
            "delivery",
            r#"{"tag": "courier", "company": "Swift", "weight": 123456}"#.to_string(),
            "`max` on `weight`",
            "123456",
        ),
        (
            "delivery",
            r#"{"tag": "pigeon"}"#.to_string(),
            "isn't its type",
            "pigeon",
        ),
        // Another shape altogether.
        (
            "tags",
            r#"{"bread": true}"#.to_string(),
            "isn't its type",
            "bread",
        ),
        // JSON's own `null` is never kept for `none`.
        ("spare", "null".to_string(), "`null`", "Spare"),
        // Past the size a value kept as JSON may be.
        (
            "weights",
            format!(
                "{{{}}}",
                (0..80_000)
                    .map(|n| format!("\"k{n}\": {n}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "more than",
            "k79999",
        ),
    ];
    for (column, edit, names, hidden) in cases {
        let kept = live.stored(&id, column);
        live.rows(&format!(
            "UPDATE \"parcel\" SET \"{column}\" = '{edit}'::jsonb WHERE \"id\" = '{id}'"
        ));
        let read = live.read(&id);
        assert_eq!(read.status, 500, "{column}: {}", text(&read));
        let fault = read.fault.as_ref().expect("the fault behind the 500");
        let why = format!("{:?}", fault.kind);
        assert!(why.contains(names), "{column} names {names}: {why}");
        assert!(
            !why.contains(hidden),
            "{column} never shows {hidden}: {why}"
        );
        assert!(!text(&read).contains(hidden), "{}", text(&read));
        // Put back, it reads again.
        live.rows(&format!(
            "UPDATE \"parcel\" SET \"{column}\" = '{kept}'::jsonb WHERE \"id\" = '{id}'"
        ));
        assert_eq!(live.read(&id).status, 200, "{column} put back");
    }
}
