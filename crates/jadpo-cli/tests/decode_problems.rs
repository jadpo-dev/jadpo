//! The decode problem over real HTTP (area 19, "Data formats", chunk
//! 5.5g2): `jadpo dev` runs a project whose actions read a supplier's
//! order with `json.read`, and each test sends a request as a client
//! would:
//!
//! - a failure mapped from `mismatch` with `problem: DecodeProblem` in its
//!   `client` block sends the first problem under `details`, the field's
//!   path with list positions as numbers and the rule;
//! - one with it in `internal` sends no details, and neither sends a value
//!   from the document, nor does `jadpo dev`'s log;
//! - the OpenAPI document describes the problem.
//!
//! The project has no entities, so no database is needed.

use std::path::Path;

use super::http::{Dev, jadpo, tempdir, wait_for, write};

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"application {}
"#,
    ),
    (
        "domains/supply/actions/check_order.jadpo",
        r#"use supply/failures { ImportRefused, ImportUnreadable }
use supply/types { ImportRequest, ImportedOrder, SupplierOrder }

/// Checks a supplier's order, keeping why it was refused to the log.
action check_order(input: ImportRequest) fails ImportUnreadable, ImportRefused -> ImportedOrder {
    var order: SupplierOrder = attempt json.read(input.message, as: SupplierOrder) {
        malformed: ImportUnreadable
        mismatch: ImportRefused
    }
    return ImportedOrder {
        reference: order.reference
    }
}
"#,
    ),
    (
        "domains/supply/actions/import_order.jadpo",
        r#"use supply/failures { ImportInvalid, ImportUnreadable }
use supply/types { ImportRequest, ImportedOrder, SupplierOrder }

/// Reads a supplier's order from its message.
action import_order(input: ImportRequest) fails ImportUnreadable, ImportInvalid -> ImportedOrder {
    var order: SupplierOrder = attempt json.read(input.message, as: SupplierOrder) {
        malformed: ImportUnreadable
        mismatch: ImportInvalid
    }
    return ImportedOrder {
        reference: order.reference
    }
}
"#,
    ),
    (
        "domains/supply/failures/import_invalid.jadpo",
        r#"/// The message isn't an order.
type ImportInvalid = Failure {
    kind: FailureKind.invalid_value
    message: "The order breaks a rule."
    client: {
        problem: DecodeProblem
    }
}
"#,
    ),
    (
        "domains/supply/failures/import_refused.jadpo",
        r#"/// The message isn't an order, and the caller isn't told why.
type ImportRefused = Failure {
    kind: FailureKind.invalid_value
    internal: {
        problem: DecodeProblem
    }
}
"#,
    ),
    (
        "domains/supply/failures/import_unreadable.jadpo",
        r#"/// The message isn't JSON.
type ImportUnreadable = Failure {
    kind: FailureKind.invalid_value
}
"#,
    ),
    (
        "domains/supply/routes/imports.jadpo",
        r#"use supply/types { ImportRequest, ImportedOrder }
use supply/actions { check_order, import_order }

routes Imports {
    prefix: /imports

    POST / {
        authentication: RouteAuthentication.none
        input: ImportRequest
        output: ImportedOrder
        run: import_order(input)
    }

    POST /checks {
        authentication: RouteAuthentication.none
        input: ImportRequest
        output: ImportedOrder
        run: check_order(input)
    }
}
"#,
    ),
    (
        "domains/supply/types/import_request.jadpo",
        r#"use supply/types { SupplierMessage }

/// A supplier's message to import.
type ImportRequest = Input {
    message: SupplierMessage
}
"#,
    ),
    (
        "domains/supply/types/imported_order.jadpo",
        r#"use supply/types { StockCode }

/// An order once imported.
type ImportedOrder = Output {
    reference: StockCode
}
"#,
    ),
    (
        "domains/supply/types/line_quantity.jadpo",
        r#"/// How many of one product a line orders.
type LineQuantity = Int {
    min: 1
    max: 1000
}
"#,
    ),
    (
        "domains/supply/types/stock_code.jadpo",
        r#"/// A product code.
type StockCode = Text {
    max_length: 20
}
"#,
    ),
    (
        "domains/supply/types/supplier_line.jadpo",
        r#"use supply/types { LineQuantity, StockCode }

/// One line of a supplier's order.
type SupplierLine = Input {
    sku: StockCode
    quantity: LineQuantity
}
"#,
    ),
    (
        "domains/supply/types/supplier_message.jadpo",
        r#"/// A supplier's message, as it arrived.
type SupplierMessage = Text {
    max_length: 100000
}
"#,
    ),
    (
        "domains/supply/types/supplier_order.jadpo",
        r#"use supply/types { StockCode, SupplierLine }

/// An order as a supplier's system sends it.
type SupplierOrder = Input {
    reference: StockCode
    lines: List<SupplierLine>
}
"#,
    ),
    (
        "policy.jadpo",
        r#"use supply

policy {
    actions: {
        supply.import_order: [Access.public]
        supply.check_order: [Access.public]
    }
}
"#,
    ),
];

/// The third line's quantity is over its `max`.
const OVER: &str = r#"{"reference": "A-12", "lines": [{"sku": "rye", "quantity": 2}, {"sku": "spelt", "quantity": 1}, {"sku": "oat", "quantity": 1001}]}"#;

/// A secret-looking value too long for a `StockCode`.
const SECRET: &str = "sk_live_51HxQ9eSECRETv4lue";

fn project() -> (tempdir::Dir, std::path::PathBuf) {
    let dir = tempdir::Dir::new("decode-problems");
    let root = dir.path().join("supply");
    for (path, text) in FILES {
        write(&root, path, text);
    }
    for args in [&["api", "update"][..], &["check"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    (dir, root)
}

/// `message` as a request's JSON body.
fn body(message: &str) -> String {
    serde_json::json!({ "message": message }).to_string()
}

#[test]
fn the_first_problem_reaches_the_client_and_never_a_value() {
    let (_dir, root) = project();
    let dev = Dev::start_without_database(&root);

    let read = dev.send(
        "POST",
        "/imports",
        &[],
        &body(r#"{"reference": "A-12", "lines": []}"#),
    );
    assert_eq!(read.status, 200, "{read:?}");
    assert_eq!(read.json(), serde_json::json!({ "reference": "A-12" }));

    let over = dev.send("POST", "/imports", &[], &body(OVER));
    assert_eq!(over.status, 422, "{over:?}");
    let error = &over.json()["error"];
    assert_eq!(error["code"], "import_invalid");
    assert_eq!(error["message"], "The order breaks a rule.");
    assert_eq!(
        error["details"],
        serde_json::json!({ "problem": { "field": "lines.2.quantity", "rule": "max" } })
    );

    let secret = dev.send(
        "POST",
        "/imports",
        &[],
        &body(&format!(r#"{{"reference": "{SECRET}", "lines": []}}"#)),
    );
    assert_eq!(secret.status, 422, "{secret:?}");
    assert_eq!(
        secret.json()["error"]["details"],
        serde_json::json!({ "problem": { "field": "reference", "rule": "max_length" } })
    );
    assert!(!format!("{secret:?}").contains("sk_live"), "{secret:?}");

    // Kept to the server: no details at all.
    let refused = dev.send(
        "POST",
        "/imports/checks",
        &[],
        &body(&format!(r#"{{"reference": "{SECRET}", "lines": []}}"#)),
    );
    assert_eq!(refused.status, 422, "{refused:?}");
    let error = &refused.json()["error"];
    assert_eq!(error["code"], "import_refused");
    assert!(error.get("details").is_none(), "{refused:?}");
    assert!(!format!("{refused:?}").contains("sk_live"), "{refused:?}");

    let unreadable = dev.send("POST", "/imports", &[], &body("{"));
    assert_eq!(unreadable.status, 422, "{unreadable:?}");
    assert_eq!(unreadable.json()["error"]["code"], "import_unreadable");

    // The server writes a request's line after answering it, so the last
    // one is waited for before the log is read.
    wait_for(&dev, "import_unreadable");
    let log = dev.log();
    assert!(log.contains("import_invalid"), "{log}");
    assert!(!log.contains("sk_live"), "{log}");
}

#[test]
fn the_openapi_document_describes_the_problem() {
    let (_dir, root) = project();
    let text = std::fs::read_to_string(Path::new(&root).join("build/openapi/main.json"))
        .expect("written by api update");
    let document: serde_json::Value = serde_json::from_str(&text).expect("json");
    let schemas = &document["components"]["schemas"];
    assert_eq!(
        schemas["ImportInvalid"]["properties"]["error"]["properties"]["details"]["properties"]["problem"],
        serde_json::json!({ "$ref": "#/components/schemas/DecodeProblem" })
    );
    let problem = &schemas["DecodeProblem"];
    assert_eq!(problem["required"], serde_json::json!(["field", "rule"]));
    assert_eq!(problem["properties"]["field"]["type"], "string");
    assert_eq!(problem["additionalProperties"], false);
    assert!(
        schemas["ImportRefused"]["properties"]["error"]["properties"]
            .get("details")
            .is_none(),
        "{}",
        schemas["ImportRefused"]
    );
}
