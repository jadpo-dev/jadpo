//! `jadpo package import openapi <file>` (area 11, "Where packages come
//! from"; chunk 7.2): a local OpenAPI document read into the app's own
//! integration in `shared/providers/<name>/<version>/`, exact from another
//! Jadpo application's `x-jadpo` fields, mapped by format from any other,
//! and merged, not overwritten, when imported again. Nothing here touches
//! the network.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::http::tempdir;
use crate::support;

/// The application whose API is imported: memos, a failure, a query with
/// a placeholder, and services that sign in with keys.
const ORDERS: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use memos/entities { Memo }

application {
    apis: {
        main: {
            prefix: /v1
        }
    }
}

principal {
    service: {
        subject: Memo.text
    }
}
"#,
    ),
    (
        "config.jadpo",
        "config {\n    session_signing_key: SigningKey\n}\n",
    ),
    (
        "authentication.jadpo",
        "authentication {\n    service_key: {\n        signing_key: config.session_signing_key\n    }\n}\n",
    ),
    (
        "domains/memos/types/memo_text.jadpo",
        "type MemoText = Text {\n    min_length: 1\n    max_length: 200\n}\n",
    ),
    (
        "domains/memos/entities/memo.jadpo",
        r#"use memos/types { MemoText }

entity Memo {
    id: Uuid
    text: MemoText {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
"#,
    ),
    (
        "domains/memos/types/memo_view.jadpo",
        "use memos/entities { Memo }\n\n/// A memo as clients see it.\ntype MemoView = Output {\n    pick: [Memo.id, Memo.text]\n}\n",
    ),
    (
        "domains/memos/failures/memo_not_found.jadpo",
        "/// No memo has that id.\ntype MemoNotFound = Failure {\n    kind: FailureKind.not_found\n    message: \"Memo not found.\"\n}\n",
    ),
    (
        "domains/memos/failures/memo_taken.jadpo",
        "type MemoTaken = Failure {\n    kind: FailureKind.conflict\n    message: \"That memo is kept already.\"\n}\n",
    ),
    (
        "domains/memos/queries/find_memo.jadpo",
        r#"use memos/entities { Memo }
use memos/types { MemoView }
use memos/failures { MemoNotFound }

query find_memo(memo_id: Memo.id) fails MemoNotFound -> MemoView {
    from: Memo
    where: Memo.id == memo_id
    missing: MemoNotFound
}
"#,
    ),
    (
        "domains/memos/actions/add_memo.jadpo",
        r#"use memos/entities { Memo }
use memos/types { MemoText, MemoView }
use memos/failures { MemoTaken }

/// What adding a memo takes.
type AddMemo = Input {
    text: MemoText
}

action add_memo(input: AddMemo) fails MemoTaken -> MemoView {
    var memo: Memo = attempt Memo.create {
        set: {
            text: input.text
        }
        conflict: {
            Memo.text: MemoTaken
        }
    }
    return MemoView {
        id: memo.id
        text: memo.text
    }
}
"#,
    ),
    (
        "domains/memos/routes/v1_memos.jadpo",
        r#"use memos/entities { Memo }
use memos/types { MemoView }
use memos/actions { AddMemo, add_memo }
use memos/queries { find_memo }

/// Memos: adding one and reading one back.
routes V1Memos {
    prefix: /v1/memos

    /// Adds a memo.
    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        status: SuccessStatus.created
        run: add_memo(input)
    }

    /// One memo.
    GET /{memo_id: Memo.id} {
        authentication: RouteAuthentication.none
        output: MemoView
        run: find_memo(memo_id)
    }
}
"#,
    ),
];

/// Another provider's document: camelCase names, formats, an enumeration
/// of currencies, an object inside an object, a list with rules on its
/// items, a `oneOf`, a header parameter and a body that isn't JSON.
const COURIERS: &str = r##"{
  "openapi": "3.0.3",
  "info": { "title": "Acme Couriers", "version": "1.4.0", "description": "Acme Couriers' booking and tracking API.\n\nMore." },
  "servers": [{ "url": "https://api.acme-couriers.example/" }],
  "tags": [{ "name": "bookings", "description": "Collections booked with a courier." }],
  "paths": {
    "/v1/bookings": {
      "get": {
        "tags": ["bookings"],
        "operationId": "listBookings",
        "summary": "Lists bookings.",
        "parameters": [
          { "name": "limit", "in": "query", "schema": { "type": "integer", "minimum": 1, "maximum": 100 } },
          { "name": "X-Request-Tag", "in": "header", "schema": { "type": "string" } }
        ],
        "responses": { "200": { "description": "ok", "content": { "application/json": { "schema": { "type": "array", "items": { "$ref": "#/components/schemas/Booking" } } } } } }
      },
      "post": {
        "tags": ["bookings"],
        "operationId": "createBooking",
        "summary": "Books a collection.",
        "requestBody": { "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CreateBooking" } } } },
        "responses": { "201": { "description": "made", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Booking" } } } } }
      }
    },
    "/v1/bookings/{bookingId}": {
      "get": {
        "tags": ["bookings"],
        "operationId": "getBooking",
        "parameters": [{ "name": "bookingId", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
        "responses": { "200": { "description": "ok", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Booking" } } } } }
      }
    },
    "/v1/bookings/{bookingId}/label": {
      "post": {
        "tags": ["bookings"],
        "operationId": "printLabel",
        "parameters": [{ "name": "bookingId", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
        "requestBody": { "content": { "application/pdf": { "schema": { "type": "string", "format": "binary" } } } },
        "responses": { "200": { "description": "ok" } }
      }
    }
  },
  "components": {
    "securitySchemes": { "token": { "type": "http", "scheme": "bearer" } },
    "schemas": {
      "CreateBooking": {
        "type": "object",
        "required": ["postcode", "slot"],
        "properties": {
          "postcode": { "type": "string", "maxLength": 8 },
          "slot": { "type": "string", "enum": ["morning", "after-noon"] },
          "contactEmail": { "type": "string", "format": "email" }
        }
      },
      "Booking": {
        "type": "object",
        "description": "A booked collection.",
        "required": ["id", "status"],
        "properties": {
          "id": { "type": "string", "format": "uuid" },
          "status": { "type": "string", "enum": ["booked", "collected", "cancelled"] },
          "collectAt": { "type": "string", "format": "date-time" },
          "trackingUrl": { "type": "string", "format": "uri", "nullable": true },
          "currency": { "type": "string", "enum": ["GBP", "EUR"] },
          "country": { "type": "string", "enum": ["GB", "FR"] },
          "parcel": { "type": "object", "properties": { "length": { "type": "integer" }, "fragile": { "type": "boolean" } } },
          "tags": { "type": "array", "items": { "type": "string", "maxLength": 20 } },
          "extra": { "oneOf": [{ "type": "string" }, { "type": "integer" }] },
          "type": { "type": "string" }
        }
      },
      "Unused": { "type": "object", "properties": { "a": { "type": "string" } } }
    }
  }
}
"##;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let env = Environment::empty().with_var("JADPO_PLAIN", "1");
    let mut output = run(&args, &env.with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_str(&output.stdout).expect("a JSON report")
}

/// An empty application to import into.
fn caller(dir: &tempdir::Dir) -> PathBuf {
    let root = dir.path().join("shop");
    write(&root, "app.jadpo", "application {}\n");
    root
}

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path)).unwrap_or_else(|error| panic!("{path}: {error}"))
}

/// The orders application's document, as its `jadpo check` writes it.
fn orders_document(dir: &tempdir::Dir) -> PathBuf {
    let root = dir.path().join("orders");
    for (path, text) in ORDERS {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}", migrated.stdout);
    let updated = jadpo(&root, &["api", "update"]);
    assert_eq!(updated.code, 0, "{}", updated.stdout);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    root.join("build/openapi/main.json")
}

#[test]
fn a_jadpo_applications_document_imports_exactly() {
    let dir = tempdir::Dir::new("import-exact");
    let document = orders_document(&dir);
    let root = caller(&dir);
    fs::copy(&document, root.join("orders.openapi.json")).expect("copy");
    let imported = jadpo(&root, &["package", "import", "openapi", "orders.openapi.json"]);
    assert_eq!(imported.code, 0, "{}{}", imported.stdout, imported.stderr);
    assert_eq!(
        imported.stdout,
        "Imported orders.openapi.json into shared/providers/orders/v1
    package     Orders.v1
    read        exactly, from its x-jadpo fields
    resources   v1_memos
    operations  2, 1 changes something
    types       3
    failures    2

imported: 2 operations, 3 types, 2 failures

next: jadpo check  checks the package with the app
"
    );
    let folder = "shared/providers/orders/v1";
    assert_eq!(
        read(&root, &format!("{folder}/package.jadpo")),
        "/// orders, imported from its OpenAPI document.\npackage Orders.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n"
    );
    let provider = read(&root, &format!("{folder}/provider.jadpo"));
    assert!(
        provider.starts_with(
            "/// How orders is reached and called.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.bearer
    idempotency: {
        header: \"Idempotency-Key\"
        window: 1d
        max_length: 255
        in_use: \"conflict\"
        reused: \"idempotency_key_reused\"
    }
    error_code: \"error.code\"
    imported: {
        file: \"orders.openapi.json\"
        digest: \"sha256:"
        ),
        "{provider}"
    );
    assert_eq!(
        read(&root, &format!("{folder}/resources/v1_memos.jadpo")),
        "/// Memos: adding one and reading one back.
public resource v1_memos {
    /// Adds a memo.
    create: {
        method: HttpMethod.post
        path: \"/v1/memos\"
        input: AddMemo
        output: MemoView
        changes: true
        idempotent: true
        failures: [MemoTaken]
        source: PackageSource.imported
    }

    /// One memo.
    get: {
        method: HttpMethod.get
        path: \"/v1/memos/{memo_id}\"
        input: V1MemosGetInput
        output: MemoView
        changes: false
        failures: [MemoNotFound]
        source: PackageSource.imported
    }
}
"
    );
    assert_eq!(
        read(&root, &format!("{folder}/types/add_memo.jadpo")),
        "/// What adding a memo takes.\npublic type AddMemo = Object {\n    text: Text {\n        min_length: 1\n        max_length: 200\n    }\n}\n"
    );
    assert_eq!(
        read(&root, &format!("{folder}/failures/memo_not_found.jadpo")),
        "/// Memo not found.\npublic type MemoNotFound = Failure {\n    kind: FailureKind.not_found\n    message: \"Memo not found.\"\n}\n"
    );
    // The runtime's own routes, the served document among them, aren't
    // the package's.
    assert!(!root.join(format!("{folder}/types/exchanged_token.jadpo")).exists());
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    // Importing the same document again changes nothing.
    let again = jadpo(&root, &["package", "import", "openapi", "orders.openapi.json", "--json"]);
    assert_eq!(again.code, 0, "{}", again.stdout);
    let report = json(&again);
    assert!(
        report["data"]["files"]
            .as_array()
            .expect("files")
            .iter()
            .all(|file| file["change"] == "unchanged"),
        "{report}"
    );
    assert_eq!(report["data"]["findings"], serde_json::json!([]));
}

#[test]
fn another_providers_document_maps_its_formats_and_lists_what_to_check() {
    let dir = tempdir::Dir::new("import-mapped");
    let root = caller(&dir);
    write(&root, "acme.json", COURIERS);
    let imported = jadpo(&root, &["package", "import", "openapi", "acme.json", "--json"]);
    assert_eq!(imported.code, 0, "{}", imported.stdout);
    let report = json(&imported);
    assert_eq!(report["data"]["package"], "AcmeCouriers.v1");
    assert_eq!(report["data"]["exact"], false);
    let folder = "shared/providers/acme_couriers/v1";
    assert_eq!(
        read(&root, &format!("{folder}/types/booking.jadpo")),
        "/// A booked collection.
public type Booking = Object {
    collect_at: Instant?
    country: Country?
    currency: Currency?
    id: Uuid
    parcel: BookingParcel?
    status: BookingStatus
    tags: List<BookingTagsItem>?
    tracking_url: Url?
    type_value: Text?
}
"
    );
    assert_eq!(
        read(&root, &format!("{folder}/types/booking_tags_item.jadpo")),
        "public type BookingTagsItem = Text {\n    max_length: 20\n}\n"
    );
    assert_eq!(
        read(&root, &format!("{folder}/types/create_booking.jadpo")),
        "public type CreateBooking = Object {\n    contact_email: Email?\n    postcode: Text {\n        max_length: 8\n    }\n    slot: CreateBookingSlot\n}\n"
    );
    let provider = read(&root, &format!("{folder}/provider.jadpo"));
    assert!(
        provider.contains("    hosts: {\n        live: \"https://api.acme-couriers.example\"\n    }\n    credential: CredentialScheme.bearer\n"),
        "{provider}"
    );
    let resource = read(&root, &format!("{folder}/resources/bookings.jadpo"));
    for name in ["    create: {", "    get: {", "    list: {", "    print_label: {"] {
        assert!(resource.contains(name), "{name} in {resource}");
    }
    // What isn't reached isn't imported.
    assert!(!root.join(format!("{folder}/types/unused.jadpo")).exists());
    let findings: Vec<String> = report["data"]["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| format!("{}: {}", finding["at"].as_str().unwrap_or(""), finding["next"].as_str().unwrap_or("")))
        .collect();
    for expected in [
        "Booking.collect_at: The provider names it `collectAt`",
        "Booking.extra: Left out: it is `oneOf` of 2 shapes",
        "Booking.type_value: The provider names it `type`",
        "CreateBookingSlot.after_noon: The provider sends `after-noon`",
        "bookings.list: Left out its header parameter `X-Request-Tag`",
        "bookings.print_label: Left out its body, sent as `application/pdf`",
        "bookings.create: Check against the provider's documentation that it changes something",
        "provider: Add `hosts: { sandbox: }`",
        "provider: Add the provider's failures and `error_code:`",
    ] {
        assert!(
            findings.iter().any(|finding| finding.starts_with(expected)),
            "no finding {expected:?} in {findings:#?}"
        );
    }
    assert!(
        report["data"]["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .all(|finding| finding["owner"] == "agent")
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
}

#[test]
fn importing_again_keeps_corrections_and_adds_what_is_new() {
    let dir = tempdir::Dir::new("import-merge");
    let root = caller(&dir);
    write(&root, "acme.json", COURIERS);
    let first = jadpo(&root, &["package", "import", "openapi", "acme.json"]);
    assert_eq!(first.code, 0, "{}", first.stdout);
    let folder = "shared/providers/acme_couriers/v1";
    // An agent corrects what the description left out.
    let resource = format!("{folder}/resources/bookings.jadpo");
    let corrected = read(&root, &resource).replace(
        "        path: \"/v1/bookings\"\n        input: CreateBooking\n        output: Booking\n        changes: true\n",
        "        path: \"/v1/bookings\"\n        input: CreateBooking\n        output: Booking\n        changes: true\n        idempotent: true\n",
    );
    write(&root, &resource, &corrected);
    // The provider adds an operation and a field.
    let newer = COURIERS
        .replace(
            "\"/v1/bookings/{bookingId}/label\": {",
            "\"/v1/bookings/{bookingId}/proof\": {\n      \"get\": { \"tags\": [\"bookings\"], \"operationId\": \"getProof\", \"parameters\": [{ \"name\": \"bookingId\", \"in\": \"path\", \"required\": true, \"schema\": { \"type\": \"string\", \"format\": \"uuid\" } }], \"responses\": { \"200\": { \"description\": \"ok\" } } }\n    },\n    \"/v1/bookings/{bookingId}/label\": {",
        )
        .replace(
            "\"id\": { \"type\": \"string\", \"format\": \"uuid\" },",
            "\"id\": { \"type\": \"string\", \"format\": \"uuid\" },\n          \"note\": { \"type\": \"string\" },",
        );
    write(&root, "acme.json", &newer);
    let again = jadpo(&root, &["package", "import", "openapi", "acme.json"]);
    assert_eq!(again.code, 0, "{}", again.stdout);
    assert!(
        again.stdout.starts_with("Merged acme.json into shared/providers/acme_couriers/v1\n"),
        "{}",
        again.stdout
    );
    assert!(again.stdout.contains("  + bookings.get_proof\n"), "{}", again.stdout);
    assert!(again.stdout.contains("  + Booking.note\n"), "{}", again.stdout);
    let merged = read(&root, &resource);
    assert!(
        merged.contains("        output: Booking\n        changes: true\n        idempotent: true\n"),
        "{merged}"
    );
    assert!(merged.contains("    get_proof: {"), "{merged}");
    assert!(read(&root, &format!("{folder}/types/booking.jadpo")).contains("    note: Text?\n"));
    let provider = read(&root, &format!("{folder}/provider.jadpo"));
    let digest = {
        use sha2::Digest;
        let bytes = sha2::Sha256::digest(newer.as_bytes());
        bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
    };
    assert!(provider.contains(&format!("sha256:{digest}")), "{provider}");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
}

#[test]
fn what_the_importer_cant_read_is_refused() {
    let dir = tempdir::Dir::new("import-refused");
    let root = caller(&dir);
    write(&root, "spec.yaml", "openapi: 3.1.0\n");
    write(&root, "swagger.json", "{ \"swagger\": \"2.0\", \"paths\": {} }");
    let cases: [(&[&str], &str); 7] = [
        (&["package", "import", "openapi", "missing.json"], "cli.package_import_unreadable"),
        (&["package", "import", "openapi", "spec.yaml"], "cli.package_import_unreadable"),
        (&["package", "import", "openapi", "swagger.json"], "cli.package_import_invalid"),
        (&["package", "import", "openapi", "swagger.json", "--as", "orders"], "cli.package_arguments"),
        (&["package", "import", "smithy", "s3.json"], "toolchain.not_built_yet"),
        (&["package", "import", "raml", "s3.json"], "cli.package_arguments"),
        (&["package", "publish"], "cli.package_arguments"),
    ];
    for (args, rule) in cases {
        let mut with_json = args.to_vec();
        with_json.push("--json");
        let refused = jadpo(&root, &with_json);
        assert_ne!(refused.code, 0, "{args:?}: {}", refused.stdout);
        assert_eq!(
            json(&refused)["diagnostics"][0]["rule"],
            rule,
            "{args:?}: {}",
            refused.stdout
        );
    }
    assert!(!root.join("shared").exists());
}
