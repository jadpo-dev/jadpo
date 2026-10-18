//! Area 19's hashing, encodings and JSON (chunk 5.5g) against fixed
//! answers: SHA-256 against NIST's examples for FIPS 180-4, Base64
//! against RFC 4648's test vectors, and `json.write` against the exact
//! text one document gives; and `crypto` (chunk 5.5q2) against its
//! standards' known answers: XChaCha20-Poly1305's draft, RFC 4231's
//! HMAC-SHA-256, RFC 8032's Ed25519 and OpenSSL's Argon2id, and a JWT
//! `verify_jwt` checks (chunk 5.5r), and RFC 7515's RS256 and ES256 tokens
//! (chunk 5.5r4). Shared by this example, which the gate runs
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
            validator: None,
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
            validator: None,
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
    crypto_checks(&mut expect);
    (wrong, count)
}

/// draft-irtf-cfrg-xchacha-03, appendix A.3.1: the key (as an
/// `EncryptionKey` setting's text), the nonce, the plaintext and the
/// ciphertext, which the associated data doesn't change.
const XCHACHA_KEY: &str = "gIGCg4SFhoeIiYqLjI2Oj5CRkpOUlZaXmJmam5ydnp8";
const XCHACHA_NONCE: &str = "404142434445464748494a4b4c4d4e4f5051525354555657";
const SUNSCREEN: &str = "Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
const XCHACHA_CIPHERTEXT: &str = "bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb731c7f1b0b4aa6440bf3a82f4eda7e39ae64c6708c54c216cb96b72e1213b4522f8c9ba40db5d945b11b69b982c1bb9e3f3fac2bc369488f76b2383565d3fff921f9664c97637da9768812f615c68b13b52e";

/// `crypto`'s known answers, each made as both executors make the call.
fn crypto_checks(expect: &mut impl FnMut(String, String, &str)) {
    use jadpo_value::crypto::{self, KeyRing, PasswordCheck};
    use jadpo_value::encoding::{from_hex, to_hex};
    let hex_of = |text: &str| from_hex(text).unwrap_or_default();
    // A sealed value's ciphertext is XChaCha20-Poly1305's, with the nonce
    // the invocation's random bytes gave.
    let keys = KeyRing::from_key_text(XCHACHA_KEY, None);
    let nonce = hex_of(XCHACHA_NONCE);
    let sealed = keys.as_ref().map(|keys| {
        let mut given = nonce.clone().into_iter();
        crypto::encrypt(
            SUNSCREEN.as_bytes(),
            keys,
            "row",
            &mut |out: &mut [u8]| {
                out.iter_mut()
                    .for_each(|byte| *byte = given.next().unwrap_or(0));
                true
            },
        )
    });
    match (keys.as_ref(), sealed) {
        (Some(keys), Some(Ok(sealed))) => {
            let body = sealed
                .get(30..sealed.len().saturating_sub(16))
                .unwrap_or(&[]);
            expect(
                "XChaCha20-Poly1305's ciphertext".to_string(),
                to_hex(body),
                XCHACHA_CIPHERTEXT,
            );
            expect(
                "a sealed value opens for its context".to_string(),
                format!(
                    "{:?}",
                    crypto::decrypt(&sealed, keys, "row").map(|v| v.len())
                ),
                &format!("{:?}", Ok::<_, Fault>(SUNSCREEN.len())),
            );
            expect(
                "a sealed value fails for another context".to_string(),
                format!("{:?}", crypto::decrypt(&sealed, keys, "other")),
                &format!("{:?}", Err::<Vec<u8>, _>(Fault::Broke(RuleKey::Unreadable))),
            );
        }
        other => expect(
            "XChaCha20-Poly1305".to_string(),
            format!("{:?}", other.1),
            "sealed",
        ),
    }
    // RFC 4231's first two cases.
    for (key, data, mac) in [
        (
            vec![0x0b; 20],
            "Hi There",
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
        ),
        (
            b"Jefe".to_vec(),
            "what do ya want for nothing?",
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
        ),
    ] {
        let keys = KeyRing::from_signing_text(&String::from_utf8_lossy(&key), None);
        let made = crypto::hmac(data.as_bytes(), &keys).unwrap_or_default();
        expect(format!("HMAC-SHA-256 of {data:?}"), to_hex(&made), mac);
        expect(
            format!("HMAC-SHA-256 of {data:?} checks"),
            crypto::verify_hmac(data.as_bytes(), &made, &keys).to_string(),
            "true",
        );
    }
    // RFC 8032's second test: the message 0x72.
    let seed = "TM0Imyj_ltqdtsNG7BFOD1uKMZ81q6Yk2oz27U-4pvs";
    let public = "PUAXw-hDiVqStwqnTRt-vJyYLM8uxJaMwM1V8Sr0Zgw";
    let signature = KeyRing::from_key_text(seed, None)
        .ok_or(Fault::Shape)
        .and_then(|keys| crypto::sign(b"r", &keys));
    expect(
        "Ed25519's signature".to_string(),
        signature
            .as_ref()
            .map_or_else(|f| format!("{f:?}"), |s| to_hex(s)),
        "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
    );
    if let Ok(signature) = signature {
        expect(
            "Ed25519's signature checks".to_string(),
            crypto::verify_signature(b"r", &signature, public).to_string(),
            "true",
        );
    }
    // OpenSSL 3.6's Argon2id at the release's parameters, with the salt
    // 00 01 ... 0f.
    let mut salt = 0u8..16;
    let hash = crypto::hash_password("correct horse battery staple", &mut |out: &mut [u8]| {
        out.iter_mut()
            .for_each(|byte| *byte = salt.next().unwrap_or(0));
        true
    });
    expect(
        "Argon2id's PHC string".to_string(),
        hash.clone().unwrap_or_else(|f| format!("{f:?}")),
        "$argon2id$v=19$m=19456,t=2,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU",
    );
    if let Ok(hash) = hash {
        expect(
            "a password checks against its hash".to_string(),
            format!(
                "{:?}",
                crypto::verify_password("correct horse battery staple", &hash)
            ),
            &format!("{:?}", PasswordCheck::Correct),
        );
    }
    // `verify_jwt` (chunk 5.5r): a token signed with RFC 8032's first test
    // key gives its subject at 2026-10-18T09:00:00Z, and the same claims
    // under `alg: none` are refused.
    let key = jwk(&[
        ("kty", Value::text("OKP")),
        ("crv", Value::text("Ed25519")),
        (
            "x",
            Value::text("11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo"),
        ),
        ("kid", Value::text("2026-10")),
    ]);
    let now = 1_792_314_000_000;
    let subject = jadpo_value::jwt::verify_jwt(
        JWT_GOOD,
        std::slice::from_ref(&key),
        None,
        Some("clinic"),
        now,
    )
    .map(|claims| match claims {
        Value::Record(record) => format!("{:?}", record.get("sub")),
        other => format!("{other:?}"),
    });
    expect(
        "an Ed25519 token's subject".to_string(),
        subject.unwrap_or_else(|fault| format!("{fault:?}")),
        &format!("{:?}", Some(&Value::text("member-42"))),
    );
    expect(
        "a token under `alg: none` is refused".to_string(),
        format!(
            "{:?}",
            jadpo_value::jwt::verify_jwt(JWT_NONE, &[key], None, Some("clinic"), now)
        ),
        &format!(
            "{:?}",
            Err::<Value, _>(Fault::Broke(RuleKey::InvalidSignature))
        ),
    );
    // RS256 and ES256 (chunk 5.5r4): RFC 7515 §A.2's and §A.3's tokens
    // under their keys, a minute before they expire, and each refused
    // under the other's.
    let rsa = jwk(&[
        ("kty", Value::text("RSA")),
        ("n", Value::text(RFC_7515_RSA_N)),
        ("e", Value::text("AQAB")),
    ]);
    let ec = jwk(&[
        ("kty", Value::text("EC")),
        ("crv", Value::text("P-256")),
        (
            "x",
            Value::text("f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU"),
        ),
        (
            "y",
            Value::text("x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0"),
        ),
    ]);
    let then = 1_300_819_320_000;
    for (name, token, own, other) in [
        ("RS256", RFC_7515_RS256, &rsa, &ec),
        ("ES256", RFC_7515_ES256, &ec, &rsa),
    ] {
        let issuer =
            jadpo_value::jwt::verify_jwt(token, std::slice::from_ref(own), None, None, then).map(
                |claims| match claims {
                    Value::Record(record) => format!("{:?}", record.get("iss")),
                    other => format!("{other:?}"),
                },
            );
        expect(
            format!("RFC 7515's {name} token's issuer"),
            issuer.unwrap_or_else(|fault| format!("{fault:?}")),
            &format!("{:?}", Some(&Value::text("joe"))),
        );
        expect(
            format!("RFC 7515's {name} token under the other key is refused"),
            format!(
                "{:?}",
                jadpo_value::jwt::verify_jwt(token, std::slice::from_ref(other), None, None, then)
            ),
            &format!(
                "{:?}",
                Err::<Value, _>(Fault::Broke(RuleKey::InvalidSignature))
            ),
        );
    }
}

/// A `Jwk` value with these parts, each other part `none`.
fn jwk(parts: &[(&str, Value)]) -> Value {
    Value::record(
        "Jwk",
        jadpo_std::standard::security::JWK_FIELDS
            .iter()
            .map(|(name, _)| {
                let value = parts
                    .iter()
                    .find(|(given, _)| given == name)
                    .map_or(Value::None, |(_, value)| value.clone());
                (Rc::from(*name), value)
            })
            .collect(),
    )
}

/// RFC 7515 §A.2's RSA modulus and its RS256 token, and §A.3's ES256
/// token (chunk 5.5r4).
const RFC_7515_RSA_N: &str = "ofgWCuLjybRlzo0tZWJjNiuSfb4p4fAkd_wWJcyQoTbji9k0l8W26mPddxHmfHQp-Vaw-4qPCJrcS2mJPMEzP1Pt0Bm4d4QlL-yRT-SFd2lZS-pCgNMsD1W_YpRPEwOWvG6b32690r2jZ47soMZo9wGzjb_7OMg0LOL-bSf63kpaSHSXndS5z5rexMdbBYUsLA9e-KXBdQOS-UTo7WTBEMa2R2CapHg665xsmtdVMTBQY4uDZlxvb3qCo5ZwKh9kG4LT6_I5IhlJH7aGhyxXFvUK-DWNmoudF8NAco9_h9iaGNj8q2ethFkMLs91kzk2PAcDTW9gb54h4FRWyuXpoQ";
const RFC_7515_RS256: &str = "eyJhbGciOiJSUzI1NiJ9.eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ.cC4hiUPoj9Eetdgtv3hF80EGrhuB__dzERat0XF9g2VtQgr9PJbu3XOiZj5RZmh7AAuHIm4Bh-0Qc_lF5YKt_O8W2Fp5jujGbds9uJdbF9CUAr7t1dnZcAcQjbKBYNX4BAynRFdiuB--f_nZLgrnbyTyWzO75vRK5h6xBArLIARNPvkSjtQBMHlb1L07Qe7K0GarZRmB_eSN9383LcOLn6_dO--xi12jzDwusC-eOkHWEsqtFZESc6BfI7noOPqvhJ1phCnvWh6IeYI2w9QOYEUipUTI8np6LbgGY9Fs98rqVt5AXLIhWkWywlVmtVrBp0igcN_IoypGlUPQGe77Rw";
const RFC_7515_ES256: &str = "eyJhbGciOiJFUzI1NiJ9.eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ.DtEhU3ljbEg8L38VWAfUAqOyKAM6-Xx-F4GawxaepmXFCgfTjDxw5djxLa8ISlSApmWQxfKTUJqPP3-Kg6NU1Q";

/// A token signed with RFC 8032's first test key, and the same claims
/// under `alg: none` with that signature (chunk 5.5r).
const JWT_GOOD: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.nOqN7320O3T8e_AyF7XNt32jmgjE8VCU7GVHL89ZUeiRcU3gGM3EgiNxVp3_y422De1PZWd2tZ0LuSqdfm6QCg";
const JWT_NONE: &str = "eyJhbGciOiJub25lIiwia2lkIjoiMjAyNi0xMCJ9.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.bUBJ-HYfcv-x-XVebqotpXUryxrUsu15W2sejqXFebmY2OB3-XXAQIWG71XLjOBQDrJnDyfN4RTsPW20OqqBDA";
