//! Area 19, "Hashing and cryptography", "Encodings" and "Data formats"
//! (chunk 5.5g): `hash.sha256` takes bytes or text and gives `Bytes`; the
//! `encoding` operations check, decoding failing `malformed`; `json.read`
//! reads a document as a type written out, failing `malformed` and
//! `mismatch`, and `json.write` writes a value an `Output` may hold; a
//! type with no JSON form is refused either way.

use crate::support;

use jadpo_db::Layout;
use support::{problems, rules};

const TYPES: &str = "type Note = Text {\n    max_length: 200\n}\n\ntype Body = Bytes {\n    max_length: 1000\n}\n\ntype Quantity = Int {\n    min: 1\n    max: 1000\n}\n\ntype ImportLine = Input {\n    sku: Note\n    quantity: Quantity\n}\n\ntype OrderImport = Input {\n    reference: Note\n    lines: List<ImportLine>\n    note: Note {\n        optional: true\n    }\n}\n\ntype OrderView = Output {\n    reference: Note\n    lines: List<ImportLine>\n}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype Invalid = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype Encoded = Text {}\n\ntype Digest = Bytes {}\n\nentity Order {\n    id: Uuid\n    reference: Note\n}\n\n";

fn check(source: &str) -> Vec<String> {
    rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{TYPES}{source}"))],
    )
}

fn shown(source: &str) -> Vec<String> {
    problems(
        Layout::Loose,
        &[("main.jadpo", &format!("{TYPES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn every_operation_checks() {
    let source = "function etag(body: Body) -> Encoded {\n    return Encoded(encoding.to_hex(hash.sha256(body)))\n}\n\nfunction key(note: Note) -> Encoded {\n    return Encoded(encoding.to_base64url(hash.sha256(note)))\n}\n\nfunction written(note: Note) -> List<Encoded> {\n    var bytes: Bytes = encoding.to_utf8(note)\n    return [Encoded(encoding.to_base64(bytes)), Encoded(encoding.to_base64url(bytes)), Encoded(encoding.to_hex(bytes)), Encoded(encoding.to_url_component(note))]\n}\n\nfunction read(note: Note, body: Body) fails Unreadable -> List<Digest> {\n    var one: Bytes = attempt encoding.from_base64(note) {\n        malformed: Unreadable\n    }\n    var two: Bytes = attempt encoding.from_base64url(note) {\n        malformed: Unreadable\n    }\n    var three: Bytes = attempt encoding.from_hex(note) {\n        invalid: Unreadable\n    }\n    var shown: Text = attempt encoding.from_utf8(body) {\n        malformed: Unreadable\n    }\n    return [Digest(one), Digest(two), Digest(three), Digest(encoding.to_utf8(shown))]\n}\n\nfunction imported(message: Note) fails Unreadable, Invalid -> OrderImport {\n    var order: OrderImport = attempt json.read(message, as: OrderImport) {\n        malformed: Unreadable\n        mismatch: Invalid\n    }\n    return order\n}\n\nfunction from_bytes(body: Body) fails Invalid -> List<Quantity> {\n    return attempt json.read(body, as: List<Quantity>) {\n        invalid: Invalid\n    }\n}\n\nfunction sent(order: OrderView) -> Encoded {\n    return Encoded(json.write(order))\n}\n\nfunction lines(order: OrderImport) -> Encoded {\n    return Encoded(json.write(order.lines))\n}\n";
    assert_eq!(check(source), NONE);
}

#[test]
fn decoding_can_fail_and_its_attempt_maps_it() {
    assert_eq!(
        check("function f(note: Note) -> Digest {\n    return Digest(encoding.from_hex(note))\n}\n"),
        ["failure.attempt_required"]
    );
    assert_eq!(
        check("function f(note: Note) -> OrderImport {\n    return json.read(note, as: OrderImport)\n}\n"),
        ["failure.attempt_required"]
    );
    // Each condition needs an arm, or `invalid:` for the rest.
    assert_eq!(
        check(
            "function f(note: Note) fails Unreadable -> OrderImport {\n    return attempt json.read(note, as: OrderImport) {\n        malformed: Unreadable\n    }\n}\n"
        ),
        ["failure.rule_unmapped"]
    );
    // Writing never fails, so it takes no `attempt`.
    assert_eq!(
        check(
            "function f(note: Note) fails Unreadable -> Encoded {\n    var hex: Text = attempt encoding.to_hex(encoding.to_utf8(note)) {\n        malformed: Unreadable\n    }\n    return Encoded(hex)\n}\n"
        ),
        ["failure.attempt_infallible"]
    );
    let headline = shown("function f(note: Note) -> Digest {\n    return Digest(encoding.from_base64(note))\n}\n");
    assert_eq!(headline.len(), 1, "{headline:?}");
    assert!(headline[0].contains("can fail at runtime"), "{headline:?}");
}

#[test]
fn a_digest_is_bytes_of_bytes_or_text() {
    assert_eq!(
        check("function f(quantity: Quantity) -> Digest {\n    return Digest(hash.sha256(quantity))\n}\n"),
        ["type.mismatch"]
    );
    assert_eq!(
        check("function f(note: Note) -> Encoded {\n    return hash.sha256(note)\n}\n"),
        ["type.mismatch"]
    );
    assert_eq!(
        check("function f(body: Body) -> Encoded {\n    return Encoded(encoding.to_hex(body))\n}\n"),
        NONE,
        "a type built on `Bytes` is bytes"
    );
    assert_eq!(
        check("function f(note: Note) -> Encoded {\n    return Encoded(encoding.to_hex(note))\n}\n"),
        ["type.mismatch"]
    );
}

#[test]
fn json_read_takes_a_type_written_out_that_has_a_json_form() {
    let read = |what: &str| {
        check(&format!(
            "function f(note: Note) fails Invalid -> Encoded {{\n    var read: {what} = attempt json.read(note, as: {what}) {{\n        invalid: Invalid\n    }}\n    return Encoded(json.write(read))\n}}\n"
        ))
    };
    assert_eq!(read("Note"), NONE);
    assert_eq!(read("Map<Text, Quantity>"), NONE);
    assert_eq!(read("Set<Quantity>"), NONE);
    // `as:` is the one error; writing what it read is another.
    for (refused, what) in [("Invalid", "a failure"), ("Order", "an entity")] {
        let found = check(&format!(
            "function f(note: Note) fails Invalid -> Note {{\n    return attempt json.read(note, as: {refused}) {{\n        invalid: Invalid\n    }}\n}}\n"
        ));
        assert_eq!(found.first().map(String::as_str), Some("type.mismatch"), "{what}");
        assert!(
            !found.iter().any(|rule| rule.starts_with("failure.")),
            "{what}: {found:?}"
        );
    }
    // A value can't say which type to read.
    assert_eq!(
        check(
            "function f(note: Note) fails Invalid -> Note {\n    var read: Note = attempt json.read(note, as: note) {\n        invalid: Invalid\n    }\n    return read\n}\n"
        ),
        ["type.mismatch"]
    );
    // A type that names nothing is resolution's one error.
    assert_eq!(
        check(
            "function f(note: Note) fails Invalid -> Note {\n    return attempt json.read(note, as: Nothing) {\n        invalid: Invalid\n    }\n}\n"
        ),
        ["semantic.unknown_name"]
    );
    // The result is the type `as:` names.
    assert_eq!(
        check(
            "function f(note: Note) fails Invalid -> Quantity {\n    return attempt json.read(note, as: Note) {\n        invalid: Invalid\n    }\n}\n"
        ),
        ["type.mismatch"]
    );
}

#[test]
fn json_write_takes_a_value_an_output_may_hold() {
    assert_eq!(
        check("function f(order: Order) -> Encoded {\n    return Encoded(json.write(order))\n}\n"),
        ["type.mismatch"]
    );
    assert_eq!(
        check("function f() -> Encoded {\n    return Encoded(json.write(Invalid))\n}\n").len(),
        1
    );
    assert_eq!(
        check("function f(quantity: Quantity) -> Encoded {\n    return Encoded(json.write([quantity]))\n}\n"),
        NONE
    );
}

#[test]
fn hash_encoding_and_json_are_reserved() {
    for name in ["hash", "encoding", "json"] {
        assert_eq!(
            check(&format!(
                "function f({name}: Note) -> Note {{\n    return {name}\n}}\n"
            )),
            ["semantic.standard_namespace_reserved"],
            "{name}"
        );
    }
    // There is no `json.decode`: reading is `json.read` (question 14).
    assert_eq!(
        check("function f(note: Note) -> Note {\n    return Note(json.decode(note))\n}\n").len(),
        1
    );
}
