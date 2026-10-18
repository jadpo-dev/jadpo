//! Area 19, "Hashing and cryptography", "Encodings" and "Data formats"
//! (chunk 5.5g), run: a digest is the SHA-256 of bytes or of text's
//! UTF-8; every encoding round-trips and decoding fails `malformed`;
//! `json.read` reads a document as its type with an `Input`'s rules,
//! failing `malformed` or `mismatch` with the first problem in its
//! report, and `json.write` gives the wire's compact text.

use std::rc::Rc;

use crate::support;

use jadpo_interp::Value;
use support::{failure, main, report, text, value};

const SOURCE: &str = "type Note = Text {\n    max_length: 2000\n}\n\ntype Body = Bytes {\n    max_length: 2000\n}\n\ntype Encoded = Text {}\n\ntype HashBytes = Bytes {}\n\ntype StockCode = Text {\n    max_length: 20\n}\n\ntype Quantity = Int {\n    min: 1\n    max: 1000\n}\n\ntype ItemPrice = Decimal {\n    scale: 2\n    min: 0.00\n}\n\ntype Colour = Enum {\n    white\n    brown\n}\n\ntype ImportLine = Input {\n    sku: StockCode\n    quantity: Quantity\n    price: ItemPrice\n}\n\ntype OrderImport = Input {\n    reference: StockCode\n    lines: List<ImportLine>\n    colours: Set<Colour>\n    rush: Bool {\n        default: false\n    }\n    note: Note {\n        optional: true\n    }\n}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype Invalid = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction etag(body: Body) -> Encoded {\n    return Encoded(encoding.to_hex(hash.sha256(body)))\n}\n\nfunction text_digest(note: Note) -> Encoded {\n    return Encoded(encoding.to_hex(hash.sha256(note)))\n}\n\nfunction written(body: Body) -> List<Encoded> {\n    return [Encoded(encoding.to_base64(body)), Encoded(encoding.to_base64url(body)), Encoded(encoding.to_hex(body))]\n}\n\nfunction link(note: Note) -> Encoded {\n    return Encoded(encoding.to_url_component(note))\n}\n\nfunction from_base64(note: Note) fails Unreadable -> HashBytes {\n    var bytes: Bytes = attempt encoding.from_base64(note) {\n        malformed: Unreadable\n    }\n    return HashBytes(bytes)\n}\n\nfunction from_base64url(note: Note) fails Unreadable -> HashBytes {\n    var bytes: Bytes = attempt encoding.from_base64url(note) {\n        malformed: Unreadable\n    }\n    return HashBytes(bytes)\n}\n\nfunction from_hex(note: Note) fails Unreadable -> HashBytes {\n    var bytes: Bytes = attempt encoding.from_hex(note) {\n        malformed: Unreadable\n    }\n    return HashBytes(bytes)\n}\n\nfunction utf8(note: Note) fails Unreadable -> Encoded {\n    var shown: Text = attempt encoding.from_utf8(encoding.to_utf8(note)) {\n        malformed: Unreadable\n    }\n    return Encoded(shown)\n}\n\nfunction from_utf8(body: Body) fails Unreadable -> Encoded {\n    var shown: Text = attempt encoding.from_utf8(body) {\n        malformed: Unreadable\n    }\n    return Encoded(shown)\n}\n\nfunction imported(message: Note) fails Unreadable, Invalid -> OrderImport {\n    return attempt json.read(message, as: OrderImport) {\n        malformed: Unreadable\n        mismatch: Invalid\n    }\n}\n\nfunction imported_bytes(body: Body) fails Unreadable, Invalid -> OrderImport {\n    return attempt json.read(body, as: OrderImport) {\n        malformed: Unreadable\n        mismatch: Invalid\n    }\n}\n\nfunction rewritten(message: Note) fails Unreadable, Invalid -> Encoded {\n    var order: OrderImport = attempt json.read(message, as: OrderImport) {\n        malformed: Unreadable\n        mismatch: Invalid\n    }\n    return Encoded(json.write(order))\n}\n\nfunction quantities(message: Note) fails Invalid -> Encoded {\n    var read: List<Quantity> = attempt json.read(message, as: List<Quantity>) {\n        invalid: Invalid\n    }\n    return Encoded(json.write(read))\n}\n";

fn bytes(bytes: &[u8]) -> Value {
    Value::Bytes(Rc::from(bytes))
}

#[test]
fn a_digest_is_sha_256_of_bytes_or_of_text_s_utf_8() {
    let program = main(SOURCE);
    // NIST's FIPS 180-4 example, "abc".
    let abc = text("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(value(&program, "etag", vec![bytes(b"abc")]), abc);
    assert_eq!(value(&program, "text_digest", vec![text("abc")]), abc);
    assert_eq!(
        value(&program, "etag", vec![bytes(b"")]),
        text("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
    );
}

#[test]
fn every_encoding_round_trips_and_decoding_fails_malformed() {
    let program = main(SOURCE);
    assert_eq!(
        value(&program, "written", vec![bytes(&[0xfb, 0xff, 0x00])]),
        Value::list(vec![text("+/8A"), text("-_8A"), text("fbff00")])
    );
    assert_eq!(
        value(&program, "written", vec![bytes(b"f")]),
        Value::list(vec![text("Zg=="), text("Zg"), text("66")])
    );
    assert_eq!(
        value(&program, "link", vec![text("rye & spelt loaf")]),
        text("rye%20%26%20spelt%20loaf")
    );
    for (name, good, wanted) in [
        ("from_base64", "Zm9v", &b"foo"[..]),
        ("from_base64url", "-_8", &[0xfb, 0xff][..]),
        ("from_hex", "FBff", &[0xfb, 0xff][..]),
    ] {
        assert_eq!(
            value(&program, name, vec![text(good)]),
            bytes(wanted),
            "{name}"
        );
    }
    for (name, bad) in [
        ("from_base64", "Zm9"),
        ("from_base64", "Zg"),
        ("from_base64url", "Zg=="),
        ("from_hex", "abc"),
    ] {
        assert_eq!(
            failure(&program, name, vec![text(bad)]).name,
            "Unreadable",
            "{name}({bad:?})"
        );
    }
    assert_eq!(value(&program, "utf8", vec![text("crème")]), text("crème"));
    assert_eq!(
        failure(&program, "from_utf8", vec![bytes(&[0x63, 0xff])]).name,
        "Unreadable",
        "invalid UTF-8 is refused, never replaced"
    );
}

const ORDER: &str = r#"{"reference": "A-12", "lines": [{"sku": "rye", "quantity": 2, "price": "3.50"}, {"sku": "spelt", "quantity": 1, "price": "4.00"}], "colours": ["brown", "white"]}"#;

#[test]
fn json_read_gives_the_type_with_an_input_s_rules_and_write_gives_the_wire_s_text() {
    let program = main(SOURCE);
    // Fields in declared order, the default filled in, the optional field
    // left out, the set in its canonical (declared) order.
    let canonical = r#"{"reference":"A-12","lines":[{"sku":"rye","quantity":2,"price":"3.50"},{"sku":"spelt","quantity":1,"price":"4.00"}],"colours":["white","brown"],"rush":false}"#;
    assert_eq!(
        value(&program, "rewritten", vec![text(ORDER)]),
        text(canonical)
    );
    // What `json.write` gives reads back to the same text.
    assert_eq!(
        value(&program, "rewritten", vec![text(canonical)]),
        text(canonical)
    );
    // Bytes in UTF-8 read as text does.
    assert_eq!(
        value(&program, "imported_bytes", vec![bytes(ORDER.as_bytes())]),
        value(&program, "imported", vec![text(ORDER)])
    );
    assert_eq!(
        value(&program, "quantities", vec![text("[3, 1, 2]")]),
        text("[3,1,2]")
    );
}

#[test]
fn a_document_that_isn_t_json_is_malformed_and_one_that_isn_t_the_type_a_mismatch() {
    let program = main(SOURCE);
    for document in ["", "{", "{\"reference\": }", "[1, 2] 3", "'A-12'"] {
        assert_eq!(
            failure(&program, "imported", vec![text(document)]).name,
            "Unreadable",
            "{document:?}"
        );
    }
    assert_eq!(
        failure(&program, "imported_bytes", vec![bytes(&[0x7b, 0xff, 0x7d])]).name,
        "Unreadable",
        "bytes that aren't UTF-8"
    );
    let broken = ORDER.replace("\"quantity\": 1,", "\"quantity\": 1001,");
    for (document, problem) in [
        (broken.as_str(), "`mismatch` (`max` on `lines.1.quantity`)"),
        (
            r#"{"reference": "A-12", "lines": [], "colours": [], "extra": 1}"#,
            "`mismatch` (`unknown_field` on `extra`)",
        ),
        (
            r#"{"reference": "A-12", "colours": []}"#,
            "`mismatch` (`required` on `lines`)",
        ),
        (
            r#"{"reference": "A-12", "lines": [], "colours": ["white", "white"]}"#,
            "`mismatch` (`duplicate` on `colours`)",
        ),
        ("[]", "`mismatch` (`type`)"),
    ] {
        assert_eq!(
            failure(&program, "imported", vec![text(document)]).name,
            "Invalid",
            "{document}"
        );
        let shown = report(&program, "imported", vec![text(document)]);
        assert!(shown.contains(problem), "{document}: {shown}");
    }
    // The first problem is named, never the value.
    let shown = report(&program, "imported", vec![text(&broken)]);
    assert!(!shown.contains("1001"), "{shown}");
}
