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
    let updated = jadpo(&root, &["records", "update"]);
    assert_eq!(updated.code, 0, "{}", updated.stdout);
    jadpo(&root, &["records", "update"]);
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
    let imported = jadpo(
        &root,
        &["package", "import", "openapi", "orders.openapi.json"],
    );
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

To review
  Once the app calls its operations, go through each type they reach against
  the provider's documentation: use the standard type wherever one fits, add a
  decoding wherever a format is undocumented (seconds, cents, comma-separated
  lists), and mark which operations change something and which are idempotent;
  then add the type to `reviewed:` in `imported:`. `jadpo check` lists them
  (agent)

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
    assert!(
        !root
            .join(format!("{folder}/types/exchanged_token.jadpo"))
            .exists()
    );
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    // Importing the same document again changes nothing.
    let again = jadpo(
        &root,
        &[
            "package",
            "import",
            "openapi",
            "orders.openapi.json",
            "--json",
        ],
    );
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
    let imported = jadpo(
        &root,
        &["package", "import", "openapi", "acme.json", "--json"],
    );
    assert_eq!(imported.code, 0, "{}", imported.stdout);
    let report = json(&imported);
    assert_eq!(report["data"]["package"], "AcmeCouriers.v1");
    assert_eq!(report["data"]["exact"], false);
    let folder = "shared/providers/acme_couriers/v1";
    assert_eq!(
        read(&root, &format!("{folder}/types/booking.jadpo")),
        "/// A booked collection.
public type Booking = Object {
    collect_at: Instant? {
        wire_name: \"collectAt\"
    }
    country: Country?
    currency: Currency?
    id: Uuid
    parcel: BookingParcel?
    status: BookingStatus
    tags: List<BookingTagsElement>?
    tracking_url: Url? {
        wire_name: \"trackingUrl\"
    }
    type_value: Text? {
        wire_name: \"type\"
    }
}
"
    );
    // A variant the provider spells otherwise says so too (area 11,
    // decided 2026-10-20).
    let slot = read(&root, &format!("{folder}/types/create_booking_slot.jadpo"));
    assert!(
        slot.contains("    after_noon: {\n        wire_name: \"after-noon\"\n    }\n"),
        "{slot}"
    );
    assert_eq!(
        read(&root, &format!("{folder}/types/booking_tags_element.jadpo")),
        "public type BookingTagsElement = Text {\n    max_length: 20\n}\n"
    );
    assert_eq!(
        read(&root, &format!("{folder}/types/create_booking.jadpo")),
        "public type CreateBooking = Object {\n    contact_email: Email? {\n        wire_name: \"contactEmail\"\n    }\n    postcode: Text {\n        max_length: 8\n    }\n    slot: CreateBookingSlot\n}\n"
    );
    let provider = read(&root, &format!("{folder}/provider.jadpo"));
    assert!(
        provider.contains("    hosts: {\n        live: \"https://api.acme-couriers.example\"\n    }\n    credential: CredentialScheme.bearer\n"),
        "{provider}"
    );
    let resource = read(&root, &format!("{folder}/resources/bookings.jadpo"));
    for name in [
        "    create: {",
        "    get: {",
        "    list: {",
        "    print_label: {",
    ] {
        assert!(resource.contains(name), "{name} in {resource}");
    }
    // What isn't reached isn't imported.
    assert!(!root.join(format!("{folder}/types/unused.jadpo")).exists());
    let findings: Vec<String> = report["data"]["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| {
            format!(
                "{}: {}",
                finding["at"].as_str().unwrap_or(""),
                finding["next"].as_str().unwrap_or("")
            )
        })
        .collect();
    // A spelling on the wire is written, no longer left to check.
    assert!(
        !findings
            .iter()
            .any(|finding| finding.contains("The provider names it")
                || finding.contains("The provider sends `")),
        "{findings:#?}"
    );
    for expected in [
        "Booking.extra: Left out: it is `oneOf` of 2 shapes",
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
            .all(|finding| finding["decides"] == "agent")
    );
    jadpo(&root, &["records", "update"]);
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
        again
            .stdout
            .starts_with("Merged acme.json into shared/providers/acme_couriers/v1\n"),
        "{}",
        again.stdout
    );
    assert!(
        again.stdout.contains("  + bookings.get_proof\n"),
        "{}",
        again.stdout
    );
    assert!(
        again.stdout.contains("  + Booking.note\n"),
        "{}",
        again.stdout
    );
    let merged = read(&root, &resource);
    assert!(
        merged
            .contains("        output: Booking\n        changes: true\n        idempotent: true\n"),
        "{merged}"
    );
    assert!(merged.contains("    get_proof: {"), "{merged}");
    assert!(read(&root, &format!("{folder}/types/booking.jadpo")).contains("    note: Text?\n"));
    let provider = read(&root, &format!("{folder}/provider.jadpo"));
    let digest = {
        use sha2::Digest;
        let bytes = sha2::Sha256::digest(newer.as_bytes());
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    assert!(provider.contains(&format!("sha256:{digest}")), "{provider}");
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
}

#[test]
fn what_the_importer_cant_read_is_refused() {
    let dir = tempdir::Dir::new("import-refused");
    let root = caller(&dir);
    write(&root, "spec.yaml", "openapi: 3.1.0\n");
    write(
        &root,
        "swagger.json",
        "{ \"swagger\": \"2.0\", \"paths\": {} }",
    );
    let cases: [(&[&str], &str); 7] = [
        (
            &["package", "import", "openapi", "missing.json"],
            "cli.package_unreadable",
        ),
        (
            &["package", "import", "openapi", "spec.yaml"],
            "cli.package_unreadable",
        ),
        (
            &["package", "import", "openapi", "swagger.json"],
            "cli.package_not_openapi",
        ),
        (
            &[
                "package",
                "import",
                "openapi",
                "swagger.json",
                "--as",
                "orders",
            ],
            "cli.package_arguments",
        ),
        (
            &["package", "import", "smithy", "s3.json"],
            "toolchain.not_built_yet",
        ),
        (
            &["package", "import", "raml", "s3.json"],
            "cli.package_arguments",
        ),
        (&["package", "unpack"], "cli.package_arguments"),
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

/// A payment provider's document, as Stripe's reads: an amount beside its
/// currency, more amounts sharing it, times as whole seconds, a shop's
/// latitude and longitude, and objects that are a standard type whole.
const PAYMENTS: &str = r##"{
  "openapi": "3.1.0",
  "info": { "title": "Tidepay", "version": "2024-06-20" },
  "servers": [{ "url": "https://api.tidepay.example" }],
  "paths": {
    "/v1/charges/{id}": {
      "get": {
        "tags": ["charges"],
        "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }],
        "responses": { "200": { "description": "ok", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/charge" } } } } }
      }
    }
  },
  "components": {
    "schemas": {
      "charge": {
        "type": "object",
        "description": "A charge to a card.",
        "required": ["id", "amount", "amount_refunded", "currency", "created", "shop", "fee", "window"],
        "properties": {
          "id": { "type": "string" },
          "amount": { "type": "integer" },
          "amount_refunded": { "type": "integer" },
          "currency": { "type": "string" },
          "created": { "type": "integer" },
          "shop": { "$ref": "#/components/schemas/shop" },
          "fee": { "$ref": "#/components/schemas/fee" },
          "window": { "$ref": "#/components/schemas/window" }
        }
      },
      "shop": {
        "type": "object",
        "required": ["name", "lat", "lng"],
        "properties": {
          "name": { "type": "string" },
          "lat": { "type": "number" },
          "lng": { "type": "number" }
        }
      },
      "fee": {
        "type": "object",
        "required": ["amount", "currency"],
        "properties": {
          "amount": { "type": "integer" },
          "currency": { "type": "string" }
        }
      },
      "window": {
        "type": "object",
        "required": ["start", "end"],
        "properties": {
          "start": { "type": "string", "format": "date-time" },
          "end": { "type": "string", "format": "date-time" }
        }
      }
    }
  }
}
"##;

/// Shape and name detection (area 11, "Mapping imported types to
/// standard types"; chunk 7.2e): an amount beside its currency becomes
/// `Money` read from both, a latitude beside a longitude a `Point`, an
/// object that is a standard type whole that type; what the importer
/// isn't sure of is a declinable error on the field, until it is swapped
/// or declined; and the summary lists both.
#[test]
fn shapes_become_standard_types_and_guesses_are_left_to_check() {
    let dir = tempdir::Dir::new("import-shapes");
    let root = caller(&dir);
    write(&root, "tidepay.json", PAYMENTS);
    let imported = jadpo(&root, &["package", "import", "openapi", "tidepay.json"]);
    assert_eq!(imported.code, 0, "{}{}", imported.stdout, imported.stderr);
    assert_eq!(
        imported.stdout,
        "Imported tidepay.json into shared/providers/tidepay/v2024_06_20
    package     Tidepay.v2024_06_20
    read        from its schemas
    resources   charges
    operations  1
    types       3
    standard    4 by shape

Mapped by shape
  Charge.amount  Money, read from `amount` and `currency`
  Charge.fee     Money, sent as the standard type
  Charge.window  InstantRange, sent as the standard type
  Shop.location  Point, read from `lat` and `lng`

To check
  Add `hosts: { sandbox: }` from the provider's documentation, or the live
  host again when the sandbox is a test credential (agent)
    provider

  Add the provider's failures and `error_code:`, where its error code is in a
  failed response, and `idempotency:` if it takes a key (agent)
    provider

  May be `Money` in minor units, its currency in `currency`: check the
  provider's documentation, then make it `Money` read from both with
  `wire_parts:`, or decline `package.standard_candidate` with the reason
  (agent)
    Charge.amount_refunded

  May be an `Instant` sent as whole seconds: check the provider's
  documentation, then make it `Instant { wire: Wire.unix_seconds }`, or
  decline `package.standard_candidate` with the reason (agent)
    Charge.created

To review
  Once the app calls its operations, go through each type they reach against
  the provider's documentation: use the standard type wherever one fits, add a
  decoding wherever a format is undocumented (seconds, cents, comma-separated
  lists), and mark which operations change something and which are idempotent;
  then add the type to `reviewed:` in `imported:`. `jadpo check` lists them
  (agent)

imported: 1 operation, 3 types, 4 to check

next: jadpo check  checks the package with the app
"
    );
    let folder = "shared/providers/tidepay/v2024_06_20";
    assert_eq!(
        read(&root, &format!("{folder}/types/charge.jadpo")),
        "/// A charge to a card.
public type Charge = Object {
    amount: Money {
        wire_parts: {
            amount: \"amount\"
            currency: \"currency\"
        }
    }
    amount_refunded: Int
    created: Int
    fee: Money
    id: Text
    shop: Shop
    window: InstantRange
}
"
    );
    assert_eq!(
        read(&root, &format!("{folder}/types/shop.jadpo")),
        "public type Shop = Object {
    location: Point {
        wire_parts: {
            latitude: \"lat\"
            longitude: \"lng\"
        }
    }
    name: Text
}
"
    );
    // A type that is a standard one whole isn't declared.
    assert!(!root.join(format!("{folder}/types/fee.jadpo")).exists());
    assert!(!root.join(format!("{folder}/types/window.jadpo")).exists());
    // What the importer wasn't sure of stops `check` until it is swapped
    // or declined.
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check", "--json"]);
    assert_ne!(checked.code, 0, "{}", checked.stdout);
    let rules: Vec<String> = json(&checked)["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|d| d["rule"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(
        rules,
        ["package.standard_candidate", "package.standard_candidate"],
        "{}",
        checked.stdout
    );
    // The agent checks the documentation: `created` is seconds, and the
    // refund is money in the charge's currency.
    let charge = format!("{folder}/types/charge.jadpo");
    let swapped = read(&root, &charge)
        .replace(
            "    amount_refunded: Int\n",
            "    amount_refunded: Money {\n        wire_parts: {\n            amount: \"amount_refunded\"\n            currency: \"currency\"\n        }\n    }\n",
        )
        .replace(
            "    created: Int\n",
            "    created: Instant {\n        wire: Wire.unix_seconds\n    }\n",
        );
    write(&root, &charge, &swapped);
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    // Importing again keeps the swaps, and has nothing more to flag.
    let again = jadpo(
        &root,
        &["package", "import", "openapi", "tidepay.json", "--json"],
    );
    assert_eq!(again.code, 0, "{}", again.stdout);
    assert_eq!(read(&root, &charge), swapped);
    let report = json(&again);
    assert_eq!(report["data"]["mapped"][0]["at"], "Charge.amount");
    assert_eq!(report["data"]["mapped"][0]["standard"], "Money");
    assert_eq!(report["data"]["mapped"][0]["by"], "shape");
    assert_eq!(
        report["data"]["mapped"][0]["from"],
        serde_json::json!(["amount", "currency"])
    );
}

/// A guess declined with a reason stays declined: `jadpo check` passes,
/// and the decline is listed for review.
#[test]
fn a_guess_declined_with_a_reason_passes() {
    let dir = tempdir::Dir::new("import-declined");
    let root = caller(&dir);
    write(&root, "tidepay.json", PAYMENTS);
    let imported = jadpo(&root, &["package", "import", "openapi", "tidepay.json"]);
    assert_eq!(imported.code, 0, "{}", imported.stdout);
    let charge = "shared/providers/tidepay/v2024_06_20/types/charge.jadpo";
    let declined = read(&root, charge)
        .replace(
            "    amount_refunded: Int\n",
            "    decline package.standard_candidate: \"A count of refunds, not money\"\n    amount_refunded: Int\n",
        )
        .replace(
            "    created: Int\n",
            "    decline package.standard_candidate: \"A sequence number, not a time\"\n    created: Int\n",
        );
    write(&root, charge, &declined);
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    let listed = jadpo(&root, &["declines", "list"]);
    assert!(
        listed.stdout.contains("A count of refunds, not money"),
        "{}",
        listed.stdout
    );
}

/// The scoped review and the unconfirmed operations (area 11, "Mapping
/// imported types to standard types", layers 4 and 5; chunk 7.2e2): the
/// import ends with one review instruction for the types the app's calls
/// reach, none until it calls something; `jadpo check` lists the types
/// still to review and the calls no recording confirms, writes both to
/// `build/report/providers.json` for `jadpo approval`, and grows the
/// review when the app calls something new; an agent's `reviewed:` list
/// survives importing again, less any type the description added to.
#[test]
fn the_review_is_scoped_to_what_the_app_calls() {
    let dir = tempdir::Dir::new("import-review");
    let root = caller(&dir);
    write(&root, "tidepay.json", PAYMENTS);
    let imported = jadpo(&root, &["package", "import", "openapi", "tidepay.json"]);
    assert_eq!(imported.code, 0, "{}", imported.stdout);
    assert!(
        imported.stdout.contains(
            "To review
  Once the app calls its operations, go through each type they reach against
  the provider's documentation: use the standard type wherever one fits, add a
  decoding wherever a format is undocumented (seconds, cents, comma-separated
  lists), and mark which operations change something and which are idempotent;
  then add the type to `reviewed:` in `imported:`. `jadpo check` lists them
  (agent)

imported: 1 operation, 3 types, 4 to check
"
        ),
        "{}",
        imported.stdout
    );
    let folder = "shared/providers/tidepay/v2024_06_20";
    let charge = format!("{folder}/types/charge.jadpo");
    let declined = read(&root, &charge)
        .replace(
            "    amount_refunded: Int\n",
            "    decline package.standard_candidate: \"A count of refunds, not money\"\n    amount_refunded: Int\n",
        )
        .replace(
            "    created: Int\n",
            "    decline package.standard_candidate: \"A sequence number, not a time\"\n    created: Int\n",
        );
    write(&root, &charge, &declined);
    // Nothing called, nothing to review.
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.stdout, "checked: 7 files\n");
    // The app calls one operation: the types it reaches are to review,
    // and the call to confirm.
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        payments: {\n            package: Tidepay.v2024_06_20\n            domains: [till]\n            environment: config.payments_environment\n        }\n    }\n}\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    payments_environment: ProviderEnvironment {\n        default: ProviderEnvironment.sandbox\n    }\n}\n",
    );
    write(
        &root,
        "domains/till/actions/look_up.jadpo",
        "use app { payments }\nuse shared/providers/tidepay\n\n/// Looks a charge up.\naction look_up(input: Tidepay.ChargesGetChargesInput) -> Tidepay.Charge {\n    return payments.charges.get_charges(input)\n}\n",
    );
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    assert_eq!(
        checked.stdout,
        "To review
  Go through each type against the provider's documentation, then add it to
  `reviewed:` in `imported:` in its package's `provider.jadpo` (agent)
    Tidepay.v2024_06_20  Charge, ChargesGetChargesInput, Shop; 0 of 3 reviewed

Not yet confirmed
  Record each with `jadpo package record`, which checks a sandbox answer
  against its types (agent)
    payments.charges.get_charges

checked: 3 to review, 1 unconfirmed, 9 files
"
    );
    let report: serde_json::Value =
        serde_json::from_str(&read(&root, "build/report/providers.json")).expect("JSON");
    assert_eq!(
        report["providers"][0]["calls"][0],
        serde_json::json!({
            "call": "payments.charges.get_charges",
            "confirmed": false,
            "connection": "payments",
            "operation": "charges.get_charges"
        })
    );
    assert_eq!(
        report["providers"][0]["types"][2],
        serde_json::json!({ "name": "Shop", "reviewed": false })
    );
    // A name that isn't one of the package's types.
    let provider = format!("{folder}/provider.jadpo");
    let original = read(&root, &provider);
    let listed = |names: &str| {
        original.replace(
            "        digest:",
            &format!("        reviewed: [{names}]\n        digest:"),
        )
    };
    write(&root, &provider, &listed("Chrage"));
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check", "--json"]);
    assert_ne!(checked.code, 0);
    let diagnostics = &json(&checked)["diagnostics"];
    assert_eq!(diagnostics[0]["rule"], "package.provider_invalid");
    assert_eq!(
        diagnostics[0]["message"],
        "`Chrage` in `reviewed:` isn't one of the package's types"
    );
    // The agent goes through each and lists it; a recording confirms the
    // call. Then the check is one line again.
    write(
        &root,
        &provider,
        &listed("Charge, ChargesGetChargesInput, Shop"),
    );
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(
        checked.stdout,
        "Not yet confirmed
  Record each with `jadpo package record`, which checks a sandbox answer
  against its types (agent)
    payments.charges.get_charges

checked: 1 unconfirmed, 9 files
"
    );
    write(
        &root,
        &format!("{folder}/recordings/charges.get_charges.json"),
        "{}\n",
    );
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.stdout, "checked: 9 files\n");
    // Importing again keeps the list, less `Shop`, which the description
    // added to and so is to review again.
    write(
        &root,
        "tidepay.json",
        &PAYMENTS.replace(
            "\"name\": { \"type\": \"string\" },",
            "\"name\": { \"type\": \"string\" },\n          \"phone\": { \"type\": \"string\" },",
        ),
    );
    let again = jadpo(&root, &["package", "import", "openapi", "tidepay.json"]);
    assert_eq!(again.code, 0, "{}", again.stdout);
    assert!(
        again.stdout.contains(
            "  To review again, as the description added to them
    ~ Shop
"
        ),
        "{}",
        again.stdout
    );
    assert!(
        again.stdout.contains(
            "  and mark which operations change something and which are idempotent; then
  add the type to `reviewed:` in `imported:` (agent)
    Shop
"
        ),
        "{}",
        again.stdout
    );
    assert!(
        read(&root, &provider).contains("        reviewed: [Charge, ChargesGetChargesInput]\n"),
        "{}",
        read(&root, &provider)
    );
}
