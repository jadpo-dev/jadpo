//! Area 19's hashing, encodings and JSON (chunk 5.5g) against fixed
//! answers: SHA-256 against NIST's examples for FIPS 180-4, Base64
//! against RFC 4648's test vectors, and `json.write` against the exact
//! text one document gives. Shared by this example, which the gate runs
//! under WebAssembly, and the native test
//! (crates/jadpo-runtime/tests/formats.rs), so both targets are held to
//! the same bytes, each call made as both executors make it: through
//! `jadpo_value::run`, and for JSON through the wire's codecs.

use std::rc::Rc;

use jadpo_std::http::{Codec, CodecId, VariantCodec};
use jadpo_std::types::{FieldPlan, ShapeId, ShapePlan, ValidatorId};
use jadpo_std::{Builtin, Constant, Num, Op, RuleKey, Rules};
use jadpo_value::json::{Codecs, Problem, Unread};
use jadpo_value::shape::Shape;
use jadpo_value::{Fault, Validator, Value, run};

/// NIST's examples for SHA-256 (FIPS 180-4; csrc.nist.gov, "Example
/// Values"): the message and its digest. The million `a`s is built.
const SHA256: &[(&str, &str)] = &[
    (
        "",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    ),
    (
        "abc",
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    ),
    (
        "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
    ),
    (
        "abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
        "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
    ),
];
const MILLION_A: &str = "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0";

/// RFC 4648, section 10: the bytes, Base64 and hexadecimal (section 8).
const BASE64: &[(&str, &str, &str)] = &[
    ("", "", ""),
    ("f", "Zg==", "66"),
    ("fo", "Zm8=", "666f"),
    ("foo", "Zm9v", "666f6f"),
    ("foob", "Zm9vYg==", "666f6f62"),
    ("fooba", "Zm9vYmE=", "666f6f6261"),
    ("foobar", "Zm9vYmFy", "666f6f626172"),
];

/// One document, written out of order with white space and escapes, and
/// the one text `json.write` gives for it: fields in declared order, the
/// set and the map's keys in the canonical order, an unbounded `Int` and
/// every `Decimal` as text, non-ASCII text as itself, no white space.
const DOCUMENT: &str = r#"{
  "note": "crème \"brûlée\"\n\ttab 🍞",
  "made": "2026-10-25",
  "quantity": 7,
  "id": "6F9619FF-8B86-D011-B42D-00CF4FC964FF",
  "total": "12.5",
  "stock": 9007199254740993,
  "paid": true,
  "body": "aGVsbG8=",
  "at": "2026-10-25T09:30:00.250Z",
  "proving": "PT1H30M",
  "opens": {"zone": "Europe/London", "instant": "2026-10-25T08:30:00Z"},
  "colours": ["brown", "white"],
  "shelves": {"rye": 2, "barley": 1, "Émile": 3, "apple": 4},
  "tags": [3, 1],
  "lines": [{"sku": "rye", "quantity": 2}, {"quantity": 1, "sku": "spelt"}],
  "left": null
}"#;

const WRITTEN: &str = "{\"id\":\"6f9619ff-8b86-d011-b42d-00cf4fc964ff\",\"note\":\"crème \\\"brûlée\\\"\\n\\ttab 🍞\",\"quantity\":7,\"stock\":\"9007199254740993\",\"total\":\"12.50\",\"paid\":true,\"body\":\"aGVsbG8=\",\"at\":\"2026-10-25T09:30:00.250Z\",\"made\":\"2026-10-25\",\"proving\":\"PT1H30M\",\"opens\":{\"instant\":\"2026-10-25T08:30:00Z\",\"zone\":\"Europe/London\"},\"colours\":[\"white\",\"brown\"],\"shelves\":{\"apple\":4,\"barley\":1,\"rye\":2,\"Émile\":3},\"tags\":[1,3],\"lines\":[{\"sku\":\"rye\",\"quantity\":2},{\"sku\":\"spelt\",\"quantity\":1}],\"left\":null}";

/// The codecs, validators and shapes of the document's type, as the plan
/// builds them for a `json.read(text, as: Delivery)` call.
struct Table {
    codecs: Vec<Codec>,
    validators: Vec<Validator>,
    shapes: Vec<Shape>,
}

impl Table {
    fn codecs(&self) -> Codecs<'_> {
        Codecs {
            codecs: &self.codecs,
            validators: &self.validators,
            shapes: &self.shapes,
        }
    }
}

/// The document's type and its codecs: the root is codec 0.
fn table() -> Table {
    let mut validators = Vec::new();
    let mut validator = |builtin: Builtin, rules: Rules| {
        validators.push(Validator::new(Some(builtin), vec![rules]));
        ValidatorId(validators.len() as u32 - 1)
    };
    let quantity = Rules {
        min: Num::parse("1"),
        max: Num::parse("1000"),
        ..Rules::default()
    };
    let scalar = |builtin, validator, int_as_number| Codec::Scalar {
        builtin,
        validator,
        int_as_number,
    };
    let v_uuid = validator(Builtin::Uuid, Rules::default());
    let v_text = validator(
        Builtin::Text,
        Rules {
            max_length: Some(200),
            ..Rules::default()
        },
    );
    let v_quantity = validator(Builtin::Int, quantity);
    let v_stock = validator(Builtin::Int, Rules::default());
    let v_total = validator(
        Builtin::Decimal,
        Rules {
            scale: Some(2),
            ..Rules::default()
        },
    );
    let v_bool = validator(Builtin::Bool, Rules::default());
    let v_bytes = validator(Builtin::Bytes, Rules::default());
    let v_instant = validator(Builtin::Instant, Rules::default());
    let v_date = validator(Builtin::CalendarDate, Rules::default());
    let v_duration = validator(Builtin::Duration, Rules::default());
    let v_zoned = validator(Builtin::ZonedTime, Rules::default());
    let v_collection = validator(Builtin::Text, Rules::default());
    let field =
        |owner: &str, name: &str, validator: ValidatorId, absent: Option<Constant>| FieldPlan {
            key: format!("field {owner}.{name}"),
            name: name.to_string(),
            validator,
            omittable: false,
            absent,
            none_left_out: false,
            section: None,
        };
    let delivery = ShapePlan {
        ty: "Delivery".to_string(),
        variant: None,
        fields: vec![
            field("Delivery", "id", v_uuid, None),
            field("Delivery", "note", v_text, None),
            field("Delivery", "quantity", v_quantity, None),
            field("Delivery", "stock", v_stock, None),
            field("Delivery", "total", v_total, None),
            field("Delivery", "paid", v_bool, None),
            field("Delivery", "body", v_bytes, None),
            field("Delivery", "at", v_instant, None),
            field("Delivery", "made", v_date, None),
            field("Delivery", "proving", v_duration, None),
            field("Delivery", "opens", v_zoned, None),
            field("Delivery", "colours", v_collection, None),
            field("Delivery", "shelves", v_collection, None),
            field("Delivery", "tags", v_collection, None),
            field("Delivery", "lines", v_collection, None),
            field("Delivery", "left", v_text, Some(Constant::None)),
        ],
    };
    let line = ShapePlan {
        ty: "Line".to_string(),
        variant: None,
        fields: vec![
            field("Line", "sku", v_text, None),
            field("Line", "quantity", v_quantity, None),
        ],
    };
    let colour = |name: &str, ordinal| VariantCodec {
        name: name.to_string(),
        wire: name.to_string(),
        ordinal,
        fields: None,
    };
    let codecs = vec![
        // 0: Delivery
        Codec::Object {
            shape: ShapeId(0),
            fields: (1..=16).map(CodecId).collect(),
        },
        scalar(Builtin::Uuid, v_uuid, true),
        scalar(Builtin::Text, v_text, true),
        scalar(Builtin::Int, v_quantity, true),
        scalar(Builtin::Int, v_stock, false),
        scalar(Builtin::Decimal, v_total, true),
        scalar(Builtin::Bool, v_bool, true),
        scalar(Builtin::Bytes, v_bytes, true),
        scalar(Builtin::Instant, v_instant, true),
        scalar(Builtin::CalendarDate, v_date, true),
        scalar(Builtin::Duration, v_duration, true),
        scalar(Builtin::ZonedTime, v_zoned, true),
        // 12: Set<Colour>
        Codec::Set {
            item: CodecId(17),
            validator: v_collection,
        },
        // 13: Map<Text, Quantity>
        Codec::Map {
            key: CodecId(2),
            value: CodecId(3),
            validator: v_collection,
        },
        // 14: Set<Quantity>
        Codec::Set {
            item: CodecId(3),
            validator: v_collection,
        },
        // 15: List<Line>
        Codec::List {
            item: CodecId(18),
            validator: v_collection,
        },
        // 16: Text?
        Codec::Nullable(CodecId(2)),
        // 17: Colour
        Codec::Enum {
            ty: "Colour".to_string(),
            variants: vec![colour("white", 0), colour("brown", 1)],
        },
        // 18: Line
        Codec::Object {
            shape: ShapeId(1),
            fields: vec![CodecId(2), CodecId(3)],
        },
    ];
    Table {
        codecs,
        validators,
        shapes: vec![Shape::new(&delivery), Shape::new(&line)],
    }
}

fn hex(value: &Value) -> String {
    match run(
        Op::EncodingToHex,
        std::slice::from_ref(value),
        None,
        &mut |_| None,
    ) {
        Ok(Value::Text(text)) => text.to_string(),
        other => format!("{other:?}"),
    }
}

fn bytes(bytes: &[u8]) -> Value {
    Value::Bytes(Rc::from(bytes))
}

/// Every check that disagrees, written out, and how many were made.
pub fn mismatches() -> (Vec<String>, usize) {
    let mut wrong = Vec::new();
    let mut count = 0;
    let mut expect = |what: String, got: String, wanted: &str| {
        count += 1;
        if got != wanted {
            wrong.push(format!("{what}: gave {got}, expected {wanted}"));
        }
    };
    let call = |op: Op, value: Value| run(op, &[value], None, &mut |_| None);
    for (message, digest) in SHA256 {
        let of_text = call(Op::HashSha256, Value::text(message));
        let of_bytes = call(Op::HashSha256, bytes(message.as_bytes()));
        for (how, given) in [("text", of_text), ("bytes", of_bytes)] {
            let got = given.as_ref().map_or_else(|f| format!("{f:?}"), hex);
            expect(format!("sha256 of {how} {message:?}"), got, digest);
        }
    }
    let million = call(Op::HashSha256, bytes(&vec![b'a'; 1_000_000]));
    expect(
        "sha256 of a million `a`s".to_string(),
        million.as_ref().map_or_else(|f| format!("{f:?}"), hex),
        MILLION_A,
    );
    let shown = |result: Result<Value, Fault>| match result {
        Ok(Value::Text(text)) => text.to_string(),
        Ok(Value::Bytes(bytes)) => format!("bytes {}", String::from_utf8_lossy(&bytes)),
        other => format!("{other:?}"),
    };
    for (plain, base64, hexadecimal) in BASE64 {
        let given = bytes(plain.as_bytes());
        expect(
            format!("to_base64({plain:?})"),
            shown(call(Op::EncodingToBase64, given.clone())),
            base64,
        );
        expect(
            format!("to_base64url({plain:?})"),
            shown(call(Op::EncodingToBase64url, given.clone())),
            base64.trim_end_matches('='),
        );
        expect(
            format!("to_hex({plain:?})"),
            shown(call(Op::EncodingToHex, given)),
            hexadecimal,
        );
        let back = format!("bytes {plain}");
        expect(
            format!("from_base64({base64:?})"),
            shown(call(Op::EncodingFromBase64, Value::text(base64))),
            &back,
        );
        expect(
            format!("from_base64url({:?})", base64.trim_end_matches('=')),
            shown(call(
                Op::EncodingFromBase64url,
                Value::text(base64.trim_end_matches('=')),
            )),
            &back,
        );
        expect(
            format!("from_hex({hexadecimal:?})"),
            shown(call(
                Op::EncodingFromHex,
                Value::text(&hexadecimal.to_uppercase()),
            )),
            &back,
        );
    }
    let malformed = format!("{:?}", Err::<Value, _>(Fault::Broke(RuleKey::Malformed)));
    for (op, text) in [
        (Op::EncodingFromBase64, "Zg"),
        (Op::EncodingFromBase64url, "Zg=="),
        (Op::EncodingFromHex, "6"),
    ] {
        expect(
            format!("{op:?}({text:?})"),
            shown(call(op, Value::text(text))),
            &malformed,
        );
    }
    expect(
        "from_utf8 of invalid UTF-8".to_string(),
        shown(call(Op::EncodingFromUtf8, bytes(&[0xc3, 0x28]))),
        &malformed,
    );
    expect(
        "to_utf8 then from_utf8".to_string(),
        shown(
            call(Op::EncodingToUtf8, Value::text("crème 🍞"))
                .and_then(|utf8| call(Op::EncodingFromUtf8, utf8)),
        ),
        "crème 🍞",
    );
    expect(
        "to_url_component".to_string(),
        shown(call(Op::EncodingToUrlComponent, Value::text("a b/ç~"))),
        "a%20b%2F%C3%A7~",
    );
    // JSON: the document reads as its type and writes as the one text,
    // which reads back to the same value and writes the same again.
    let table = table();
    let codecs = table.codecs();
    match codecs.read_value(CodecId(0), &Value::text(DOCUMENT)) {
        Ok(value) => {
            let written = codecs.write_value(CodecId(0), &value);
            expect("json.write".to_string(), written.clone(), WRITTEN);
            let again = codecs
                .read_value(CodecId(0), &Value::text(&written))
                .map(|read| (read == value, codecs.write_value(CodecId(0), &read)));
            expect(
                "json.read of what json.write gave".to_string(),
                format!("{again:?}"),
                &format!("{:?}", Ok::<_, Unread>((true, WRITTEN.to_string()))),
            );
            let utf8 = codecs.read_value(CodecId(0), &bytes(DOCUMENT.as_bytes()));
            expect(
                "json.read of UTF-8 bytes".to_string(),
                format!("{}", utf8 == Ok(value)),
                "true",
            );
        }
        Err(unread) => expect("json.read".to_string(), format!("{unread:?}"), "a value"),
    }
    for (document, wanted) in [
        ("{\"id\": ", Unread::Malformed),
        ("[1] [2]", Unread::Malformed),
        (
            &DOCUMENT.replace("\"quantity\": 1,", "\"quantity\": 0,") as &str,
            Unread::Mismatch(Problem {
                field: "lines.1.quantity".to_string(),
                rule: "min".to_string(),
            }),
        ),
        (
            &DOCUMENT.replace("\"left\": null", "\"right\": null") as &str,
            Unread::Mismatch(Problem {
                field: "right".to_string(),
                rule: "unknown_field".to_string(),
            }),
        ),
    ] {
        let got = codecs.read_value(CodecId(0), &Value::text(document));
        expect(
            format!("json.read of {:?}", document.get(..20).unwrap_or(document)),
            format!("{got:?}"),
            &format!("{:?}", Err::<Value, _>(wanted)),
        );
    }
    let invalid = codecs.read_value(CodecId(0), &bytes(&[0x7b, 0xff, 0x7d]));
    expect(
        "json.read of bytes that aren't UTF-8".to_string(),
        format!("{invalid:?}"),
        &format!("{:?}", Err::<Value, _>(Unread::Malformed)),
    );
    (wrong, count)
}
