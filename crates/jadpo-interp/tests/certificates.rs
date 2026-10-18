//! Area 19, "Security records" (chunk 5.5r5), run: a `Certificate` is
//! read from its PEM by `text.parse` and `json.read`, read through by its
//! parts, written back as the PEM it was read from, and compared; anything
//! but one certificate's block fails `malformed`.

use crate::support;

use jadpo_interp::{Outcome, Value};
use jadpo_std::standard::certificates as fixtures;
use support::{main, text, value};

const SOURCE: &str = "type Pem = Text {\n    max_length: 100000\n}\n\ntype Wire = Text {}\n\ntype Name = Text {}\n\ntype Same = Bool {}\n\ntype At = Instant {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype Partner = Object {\n    name: Name\n    certificate: Certificate\n}\n\nfunction read(raw: Pem) fails Unreadable -> Certificate {\n    return attempt text.parse(raw, as: Certificate) {\n        malformed: Unreadable\n    }\n}\n\nfunction subject(raw: Pem) fails Unreadable -> Name {\n    var offered: Certificate = attempt read(raw)\n    return Name(offered.subject)\n}\n\nfunction valid_at(raw: Pem, at: At) fails Unreadable -> Same {\n    var offered: Certificate = attempt read(raw)\n    return Same(offered.not_before <= at and at <= offered.not_after)\n}\n\nfunction written(raw: Pem) fails Unreadable -> Wire {\n    var offered: Certificate = attempt read(raw)\n    return Wire(text.from(offered))\n}\n\nfunction same(a: Pem, b: Pem) fails Unreadable -> Same {\n    var first: Certificate = attempt read(a)\n    var second: Certificate = attempt read(b)\n    return Same(first == second)\n}\n\nfunction sent(name: Name, raw: Pem) fails Unreadable -> Wire {\n    var offered: Certificate = attempt read(raw)\n    return Wire(json.write(Partner {\n        name: name\n        certificate: offered\n    }))\n}\n\nfunction received(raw: Wire) fails Unreadable -> Name {\n    var partner: Partner = attempt json.read(raw, as: Partner) {\n        malformed: Unreadable\n        mismatch: Unreadable\n    }\n    return Name(partner.certificate.issuer)\n}\n";

#[test]
fn a_certificate_is_read_through_and_written_back() {
    let program = main(SOURCE);
    assert_eq!(
        value(&program, "subject", vec![text(fixtures::ISRG_ROOT_X1)]),
        text("CN=ISRG Root X1,O=Internet Security Research Group,C=US")
    );
    for (at, valid) in [
        ("2026-10-18T09:00:00Z", true),
        ("2036-10-01T00:00:00Z", true),
        ("2036-10-01T00:00:00.001Z", false),
        ("2026-09-30T23:59:59Z", false),
    ] {
        let at = Value::Instant(jadpo_std::instant_millis(at).unwrap());
        assert_eq!(
            value(&program, "valid_at", vec![text(fixtures::SELF_SIGNED), at]),
            Value::Bool(valid)
        );
    }
    // Written back as the PEM it was read from, however its lines ended.
    let crlf = fixtures::SELF_SIGNED.replace('\n', "\r\n");
    assert_eq!(
        value(&program, "written", vec![text(&crlf)]),
        text(fixtures::SELF_SIGNED)
    );
    assert_eq!(
        value(
            &program,
            "same",
            vec![text(&crlf), text(fixtures::SELF_SIGNED)]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        value(
            &program,
            "same",
            vec![text(fixtures::ISRG_ROOT_X1), text(fixtures::SELF_SIGNED)]
        ),
        Value::Bool(false)
    );
}

#[test]
fn json_carries_a_certificate_as_its_pem() {
    let program = main(SOURCE);
    let sent = value(
        &program,
        "sent",
        vec![text("Lab"), text(fixtures::ISRG_ROOT_X1)],
    );
    let Value::Text(json) = &sent else {
        panic!("{sent:?}");
    };
    // The PEM as a JSON string, its line feeds escaped.
    let pem = fixtures::ISRG_ROOT_X1.replace('\n', "\\n");
    assert_eq!(
        **json,
        format!("{{\"name\":\"Lab\",\"certificate\":\"{pem}\"}}")
    );
    assert_eq!(
        value(&program, "received", vec![sent.clone()]),
        text("CN=ISRG Root X1,O=Internet Security Research Group,C=US")
    );
    // A forged part is never read: JSON gives the PEM alone.
    let forged = r#"{"name":"Lab","certificate":{"subject":"CN=bank.example","der":"AA=="}}"#;
    assert!(matches!(
        support::run(&program, "received", vec![text(forged)]),
        Outcome::Failed(_)
    ));
}

#[test]
fn anything_but_one_certificate_is_malformed() {
    let program = main(SOURCE);
    for raw in fixtures::CERTIFICATE_ENTRY.examples.malformed {
        assert!(
            matches!(
                support::run(&program, "read", vec![text(raw)]),
                Outcome::Failed(_)
            ),
            "{raw}"
        );
    }
}
