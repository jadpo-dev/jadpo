//! Webhooks that need a reply, handshakes, and Twilio's, Slack's and AWS
//! SNS's schemes, against live PostgreSQL (area 11, "Receiving from
//! outside"; chunk 7.7b).
//!
//! A help desk answers calls through Acme Voice, which signs as Twilio
//! does over a form body and waits for a reply in the response, answered
//! by the app's `support.answer_call`; takes messages from Acme Chat,
//! which signs as Slack does and checks its address with Slack's
//! challenge; hears of files changing from Acme Drive, which validates its
//! address as Microsoft Graph does; and follows its orders on an AWS SNS
//! topic, whose messages are signed with a certificate the runtime fetches
//! from AWS's SNS host. Nothing reaches a network: each request is handed
//! to the runtime as a host would, and AWS's host is a server on this
//! machine reached through the native host's test-only stand-in, serving
//! a certificate made for these tests, whose key signed the messages
//! below.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::Outward;
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::SqlValue;
use jadpo_runtime::http::{Request, Response, handle};
use jadpo_runtime::runner::Deferred;
use jadpo_term::Environment;
use jadpo_test::host::Harness;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use support

application {
    connections: {
        voice: {
            package: AcmeVoice.v1
            domains: [support]
            credential: config.voice_key
            host: config.voice_address
            webhooks: {
                secret: config.voice_auth_token
                replies: {
                    incoming_call: support.answer_call
                }
            }
        }
        chat: {
            package: AcmeChat.v1
            domains: [support]
            credential: config.chat_key
            host: config.chat_address
            webhooks: {
                secret: config.chat_signing_secret
            }
        }
        drive: {
            package: AcmeDrive.v1
            domains: [support]
            credential: config.drive_key
            host: config.drive_address
            webhooks: {
                secret: config.drive_secret
            }
        }
        orders: {
            package: AcmeTopics.v1
            domains: [support]
            credential: config.orders_key
            host: config.orders_address
            webhooks: {
                secret: config.orders_topic
            }
        }
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    voice_key: Text {
        secret: true
    }
    voice_address: Url
    voice_auth_token: Text {
        secret: true
    }
    chat_key: Text {
        secret: true
    }
    chat_address: Url
    chat_signing_secret: Text {
        secret: true
    }
    drive_key: Text {
        secret: true
    }
    drive_address: Url
    drive_secret: Text {
        secret: true
    }
    orders_key: Text {
        secret: true
    }
    orders_address: Url
    orders_topic: Text {
        secret: true
    }
}
"#,
    ),
    (
        "shared/providers/acme_voice/v1/package.jadpo",
        "/// Acme Voice's calls API.\npackage AcmeVoice.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
    ),
    (
        "shared/providers/acme_voice/v1/provider.jadpo",
        r#"/// Acme Voice's API, which signs its webhooks as Twilio does.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.bearer
    webhooks: {
        check: SignatureScheme.twilio
        event_id: "CallSid"
        event_type: "CallStatus"
    }
}
"#,
    ),
    (
        "shared/providers/acme_voice/v1/types/call_reply.jadpo",
        r#"/// What a caller hears.
public type CallReply = Object {
    say: Text
    hang_up: Bool {
        wire_name: "hangUp"
    }
}
"#,
    ),
    (
        "shared/providers/acme_voice/v1/events/incoming_call.jadpo",
        r#"/// A call is ringing, and waits to be told what to do.
public type IncomingCall = Event {
    wire_name: "ringing"
    reply: CallReply
    occurred_at: Instant
    call_sid: Text {
        wire_name: "CallSid"
    }
    from: Text {
        wire_name: "From"
    }
}
"#,
    ),
    (
        "shared/providers/acme_chat/v1/package.jadpo",
        "/// Acme Chat's messaging API.\npackage AcmeChat.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
    ),
    (
        "shared/providers/acme_chat/v1/provider.jadpo",
        r#"/// Acme Chat's API, which signs as Slack does and checks an address with Slack's challenge.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.bearer
    webhooks: {
        check: SignatureScheme.slack
        event_id: "event_id"
        event_type: "event.type"
        handshake: WebhookHandshake.slack
    }
}
"#,
    ),
    (
        "shared/providers/acme_chat/v1/types/posted.jadpo",
        "/// What was posted.\npublic type Posted = Object {\n    text: Text\n}\n",
    ),
    (
        "shared/providers/acme_chat/v1/events/message_posted.jadpo",
        r#"/// A message was posted in a channel.
public type MessagePosted = Event {
    wire_name: "message"
    occurred_at: Instant
    posted: Posted {
        wire_name: "event"
    }
}
"#,
    ),
    (
        "shared/providers/acme_drive/v1/package.jadpo",
        "/// Acme Drive's files API.\npackage AcmeDrive.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
    ),
    (
        "shared/providers/acme_drive/v1/provider.jadpo",
        r#"/// Acme Drive's API, which validates an address as Microsoft Graph does.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.bearer
    webhooks: {
        check: SignatureScheme.hmac_sha256
        header: "X-Drive-Signature"
        event_id: "id"
        event_type: "type"
        handshake: WebhookHandshake.microsoft_graph
    }
}
"#,
    ),
    (
        "shared/providers/acme_drive/v1/events/file_changed.jadpo",
        "/// A file changed.\npublic type FileChanged = Event {\n    wire_name: \"file.changed\"\n    occurred_at: Instant\n    path: Text\n}\n",
    ),
    (
        "shared/providers/acme_topics/v1/package.jadpo",
        "/// Acme's order topic, on AWS SNS.\npackage AcmeTopics.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
    ),
    (
        "shared/providers/acme_topics/v1/provider.jadpo",
        r#"/// AWS SNS, which signs each message with a certificate of AWS's own.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.bearer
    webhooks: {
        check: SignatureScheme.sns
        event_id: "MessageId"
        event_type: "Message.type"
        handshake: WebhookHandshake.sns
    }
}
"#,
    ),
    (
        "shared/providers/acme_topics/v1/types/shipment.jadpo",
        "/// An order that went out.\npublic type Shipment = Object {\n    order_id: Text\n}\n",
    ),
    (
        "shared/providers/acme_topics/v1/events/order_shipped.jadpo",
        r#"/// An order was shipped.
public type OrderShipped = Event {
    wire_name: "order.shipped"
    occurred_at: Instant
    shipment: Shipment {
        wire_name: "Message"
    }
}
"#,
    ),
    (
        "domains/support/entities/call.jadpo",
        r#"/// A call the desk answered.
entity Call {
    id: Uuid
    sid: Text
    caller: Text
    rang_at: Instant

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
"#,
    ),
    (
        "domains/support/failures/caller_blocked.jadpo",
        "/// The caller is one the desk doesn't take calls from.\ntype CallerBlocked = Failure {\n    kind: FailureKind.rejected\n    message: \"This caller is blocked.\"\n}\n",
    ),
    (
        "domains/support/actions/answer_call.jadpo",
        r#"use support/entities { Call }
use support/failures { CallerBlocked }
use shared/providers/acme_voice

/// Answers a call Acme Voice rings through, noting who rang.
action answer_call(event: AcmeVoice.IncomingCall) fails CallerBlocked -> AcmeVoice.CallReply {
    if (event.from == "+447700900999") {
        reject CallerBlocked
    }
    var call: Call = Call.create {
        set: {
            sid: event.call_sid
            caller: event.from
            rang_at: event.occurred_at
        }
    }
    return AcmeVoice.CallReply {
        say: "Thanks for calling."
        hang_up: call.caller != ""
    }
}
"#,
    ),
];

/// When the test starts: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;
/// Twilio's example auth token, Slack's example signing secret.
const VOICE_TOKEN: &str = "12345";
const CHAT_SECRET: &str = "8f742231b10e8888abcd99yyyzzz85a5";
const DRIVE_SECRET: &str = "drive-secret";
const TOPIC: &str = "arn:aws:sns:eu-west-2:123456789012:orders";

/// AWS's SNS host, as the messages below name it.
const SNS_HOST: &str = "sns.eu-west-2.amazonaws.com";

/// The certificate made for these tests (`openssl req -x509`, RSA 2048,
/// valid from 2026 to 2036), served as AWS's.
const CERTIFICATE: &str = "-----BEGIN CERTIFICATE-----\nMIIDVzCCAj+gAwIBAgIUN21O0o8ipzh6VaTx2KqMYqoHM4MwDQYJKoZIhvcNAQEL\nBQAwOzEkMCIGA1UEAwwbc25zLmV1LXdlc3QtMi5hbWF6b25hd3MuY29tMRMwEQYD\nVQQKDApKYWRwbyB0ZXN0MB4XDTI2MDEwMTAwMDAwMFoXDTM2MDEwMTAwMDAwMFow\nOzEkMCIGA1UEAwwbc25zLmV1LXdlc3QtMi5hbWF6b25hd3MuY29tMRMwEQYDVQQK\nDApKYWRwbyB0ZXN0MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAkH0+\n4SSdjM3HKlfbW/dqdckWiyKn9X2eKowlYACC9vOJQRS32C7J13LpHo8XlCugfopl\nl+PKQ+Pb7U/gNbqxSIDTpYG7xZ9YRX8ePLqcXVcxN1zr404qp1FOEKClFX6rNUQQ\nq3ewqaQN6aBCOLmTJt/7NQEcD/TsekaZtfsHJ2rsL7OOcu9RiulGtqFThswlcejJ\nY+SPG2TyZVNp3Nbq6e5x+Cuip9BKSz1HVKwlJFpNtTK0CC8MjIa6BTKFhKRnK0MT\nhsebBQU4XbrGcbfu3dz+PwY+bhWDPo5yx1F/9BdeQj5iNPcd0mTbs34gPbi5a3Zm\necF4w56lTWI2w0eM0QIDAQABo1MwUTAdBgNVHQ4EFgQUfAdv5cm6hhGFKK1T8yvY\nPkBMLkUwHwYDVR0jBBgwFoAUfAdv5cm6hhGFKK1T8yvYPkBMLkUwDwYDVR0TAQH/\nBAUwAwEB/zANBgkqhkiG9w0BAQsFAAOCAQEAgo6eeXEvqNsjnGmokGsjASyyziYe\nLD86ajh+5sr/MT7mjKTFdSyZS0D60tDyqeWxnMMO97xW/wPLxxx0ne+ToAOikJ6C\neqTW7NehJdeH3kIV5oCTN1Y7DIq3XX0nzEdDEc5VVpxAZh5DYzpFIthTr3rNvvMh\nbKeZ7nRe3Z+1Q8t/FpM6Pf6goTSNvkoTUfA/YP2shX58qCPxaMlpdITyWLTXeURf\n9qsJnEO5ay6AztvCCjP18og9Sc5OxFo6Gcjn0s3xWv9OFmhPXhlLXvThu/W5KTLO\nzJWvZL7GPptBVGdzooLSeYi+laU5AVilDKqXDNR45h5N3CTVVwTU/CuZ/w==\n-----END CERTIFICATE-----\n";

// Messages as SNS sends them, signed with the certificate's key over
// what SNS signs: two notifications (signature versions 1 and 2), one from
// someone else's topic, and a subscription confirmation.
const NOTIFIED_V1: &str = r#"{"Type":"Notification","MessageId":"m-1","TopicArn":"arn:aws:sns:eu-west-2:123456789012:orders","Message":"{\"type\":\"order.shipped\",\"order_id\":\"o-1\"}","Timestamp":"2026-10-19T08:59:00.000Z","SignatureVersion":"1","SigningCertURL":"https://sns.eu-west-2.amazonaws.com/SimpleNotificationService-7b2test.pem","UnsubscribeURL":"https://sns.eu-west-2.amazonaws.com/?Action=Unsubscribe&SubscriptionArn=x","Signature":"BfVdEgEQxdP2VDLnVffjIx662jPhamN7WcLgz+b5XKbgzzwCSkGuBteQxPq0gA9O51AjUPsmedb4ralSkk+wYxEt+yM4srPtHIAMclezed2qfM0JG/jD+HOgPS5amB2zPMl1HkXW0zD0eNvW5XTSOSONTBYhbbK9oNVoMSX4v8gY6wC+9u2yYnwWJIqcfPDL0k5PR/aGlETILvx/HV0P5x40qJSroFxFBNQ8/MjAkaKJ3Utt2e0zOHFGzqTpTWID54+bXC71iUwJylj0eC0Rawwi1tY4GOlIqJOCHX9QviwNbfv0ZciATMmX2ejltq9+1vr/m9JqJudutVDPG1Vraw=="}"#;
const NOTIFIED_V2: &str = r#"{"Type":"Notification","MessageId":"m-2","TopicArn":"arn:aws:sns:eu-west-2:123456789012:orders","Message":"{\"type\":\"order.shipped\",\"order_id\":\"o-2\"}","Timestamp":"2026-10-19T08:59:30.000Z","SignatureVersion":"2","SigningCertURL":"https://sns.eu-west-2.amazonaws.com/SimpleNotificationService-7b2test.pem","UnsubscribeURL":"https://sns.eu-west-2.amazonaws.com/?Action=Unsubscribe&SubscriptionArn=x","Signature":"G7sXzhCLEe9L+K68j/wCHcI7P3f90oxP2+1cCZMYoF0SCtus19W266W5hFkO64Wmg/+Up5wKONoBxzHJQNKmu8Rk7qo1IIXARys3IL1S3a1XQyQEVRBicORVfGmP8qWeDCU4S1I2XcEWj5vqIX30nrffXclVRmsyIO9idpw0y3q8EEHvsdGaoIYWrXHIN4+gn/yQx3tVEegOK4BkgF7MlcDNXIsCtUnqSH+7EhOtn0vCkuiRhM/VD+lSQXuYxibblmiz5FNHbcF9YMhnd7iWW3vX4YqCQBOxW8VeCyAIb8GSfoKVjsxCsoeXXxmDqeFrNlW+aRXyzVlCpLaxg+jcjQ=="}"#;
const FROM_ELSEWHERE: &str = r#"{"Type":"Notification","MessageId":"m-3","TopicArn":"arn:aws:sns:eu-west-2:999999999999:theirs","Message":"{\"type\":\"order.shipped\",\"order_id\":\"o-3\"}","Timestamp":"2026-10-19T08:59:30.000Z","SignatureVersion":"1","SigningCertURL":"https://sns.eu-west-2.amazonaws.com/SimpleNotificationService-7b2test.pem","UnsubscribeURL":"https://sns.eu-west-2.amazonaws.com/?Action=Unsubscribe&SubscriptionArn=x","Signature":"KOreZPu8KDUOv4mzaPzqOvJOnOGRUBYP3mjsNxd6ydH1h7PWFR0iDK4r8M2Zffjl68J9doVs5FAkW3o3r2yeG5eq2n5cnsLcvcxrhn0egufyfO2uc87apAN8GFCtEgV/JC7o2kpwQ3dtYjG73CNElBa+MaVJTCmnu2PZKfGMeLRvzWxCn5SwQJKuGfvC6C3ZjKkmxoGiitsDKqcGcah6SQlwIIodpAC8ZeMGZ8KyfgIsXbrpDgOjWvVCWNdTuC8tBDSkAEaD6PhvkY5sM5XCiucmH0qXj3HjJvNOsIkmH/M85skGovDwtZbrDvOnNNNl55ascZCX+rZoupuhQaHezw=="}"#;
const CONFIRMATION: &str = r#"{"Type":"SubscriptionConfirmation","MessageId":"m-4","Token":"tok-1","TopicArn":"arn:aws:sns:eu-west-2:123456789012:orders","Message":"You have chosen to subscribe to the topic.","SubscribeURL":"https://sns.eu-west-2.amazonaws.com/?Action=ConfirmSubscription&TopicArn=arn:aws:sns:eu-west-2:123456789012:orders&Token=tok-1","Timestamp":"2026-10-19T08:58:00.000Z","SignatureVersion":"1","SigningCertURL":"https://sns.eu-west-2.amazonaws.com/SimpleNotificationService-7b2test.pem","Signature":"Wxs3nvMdgtvh2zH6B5bO/LtofRsPjThwsaPbXhbhoNSsQymSyRNQcUkcbZMukFz9UnTCb/XnJfPXBvEcA1eujWWe5P9ZdyfJjEoLhbRI/wmn0ljECIjqau6HYUE9drPVUvaW6LArsBKC5ly7AOmwbtQ3jLR9I5JV0ogjVj/ubL3rl6g5iMu74UHe+n77sJwrhWy1BpvRWBDqszRJP0YBfSL7LbIfrc+0wX5AimZbFfeNbsCNWnBVXxQvhqd7h+/mirk4Exf9pnFEHdSOcMXyuid+j9rsbL6zI0XZw0atoQS8gqiVWX8JhGeZNXhBZoG9tETuIpsD612toGjMSzAHDw=="}"#;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", &(T0 / 1000).to_string())
        .with_var("NO_COLOR", "1");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

struct Loaded {
    program: Program,
    _databases: jadpo_test::database::TestDatabases,
    url: String,
}

/// The project, checked, with its migration, and a test database of its
/// own; `None` when live tests are skipped.
fn load(name: &str) -> Option<Loaded> {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("webhooks-7.7b-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    for args in [&["migrate", "new", "initial"][..], &["check"]] {
        let out = jadpo(&root, args);
        assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    }
    if !jadpo_test::live() {
        return None;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let prepared = Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|problems| {
        panic!(
            "the project checks: {:?}",
            problems.iter().map(|p| &p.message).collect::<Vec<_>>()
        )
    });
    let variables = |name: &str| -> Option<String> {
        match name {
            "VOICE_KEY" | "CHAT_KEY" | "DRIVE_KEY" | "ORDERS_KEY" => Some("key_test".into()),
            "VOICE_ADDRESS" | "CHAT_ADDRESS" | "DRIVE_ADDRESS" | "ORDERS_ADDRESS" => {
                Some("https://api.example".into())
            }
            "VOICE_AUTH_TOKEN" => Some(VOICE_TOKEN.into()),
            "CHAT_SIGNING_SECRET" => Some(CHAT_SECRET.into()),
            "DRIVE_SECRET" => Some(DRIVE_SECRET.into()),
            "ORDERS_TOPIC" => Some(TOPIC.into()),
            _ => None,
        }
    };
    let values = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .unwrap_or_else(|problems| panic!("the settings read: {problems:?}"));
    let program = Program::new(prepared).with_config(&values);
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    Some(Loaded {
        program,
        _databases: databases,
        url,
    })
}

/// An HMAC of `message` under `key`, by OpenSSL, so the test signs as a
/// provider does without the runtime's own code.
fn hmac(digest: openssl::hash::MessageDigest, key: &[u8], message: &[u8]) -> Vec<u8> {
    let key = openssl::pkey::PKey::hmac(key).expect("a key");
    let mut signer = openssl::sign::Signer::new(digest, &key).expect("a signer");
    signer.update(message).expect("signed");
    signer.sign_to_vec().expect("a signature")
}

/// A call ringing, as Acme Voice sends it: a form, signed as Twilio signs
/// over `https://<host>/webhooks/voice` and the form's members by name.
fn ringing(sid: &str, from: &str, token: &str) -> Request {
    let members = [("CallSid", sid), ("CallStatus", "ringing"), ("From", from)];
    let body: Vec<String> = members
        .iter()
        .map(|(name, value)| format!("{name}={}", value.replace('+', "%2B")))
        .collect();
    let mut sorted = members.to_vec();
    sorted.sort();
    let mut signed = String::from("https://desk.example/webhooks/voice");
    for (name, value) in sorted {
        signed.push_str(name);
        signed.push_str(value);
    }
    let mac = hmac(
        openssl::hash::MessageDigest::sha1(),
        token.as_bytes(),
        signed.as_bytes(),
    );
    Request {
        method: "POST".into(),
        path: "/webhooks/voice".into(),
        headers: vec![
            (
                "content-type".into(),
                "application/x-www-form-urlencoded".into(),
            ),
            ("host".into(), "desk.example".into()),
            (
                "x-twilio-signature".into(),
                jadpo_value::encoding::to_base64(&mac),
            ),
        ],
        body: body.join("&").into_bytes(),
        address: Some("192.0.2.20".into()),
        ..Request::default()
    }
}

/// A request to Acme Chat's webhook with `body`, signed as Slack signs at
/// `at` with `secret`.
fn chat(body: &str, secret: &str, at: i64) -> Request {
    let t = (at / 1000).to_string();
    let mac = hmac(
        openssl::hash::MessageDigest::sha256(),
        secret.as_bytes(),
        format!("v0:{t}:{body}").as_bytes(),
    );
    let hex: String = mac.iter().map(|b| format!("{b:02x}")).collect();
    Request {
        method: "POST".into(),
        path: "/webhooks/chat".into(),
        headers: vec![
            ("content-type".into(), "application/json".into()),
            ("x-slack-request-timestamp".into(), t),
            ("x-slack-signature".into(), format!("v0={hex}")),
        ],
        body: body.as_bytes().to_vec(),
        address: Some("192.0.2.21".into()),
        ..Request::default()
    }
}

/// An SNS message posted to the orders topic's webhook.
fn sns(body: &str) -> Request {
    Request {
        method: "POST".into(),
        path: "/webhooks/orders".into(),
        headers: vec![
            ("content-type".into(), "text/plain; charset=UTF-8".into()),
            ("x-amz-sns-message-type".into(), "Notification".into()),
        ],
        body: body.as_bytes().to_vec(),
        address: Some("192.0.2.22".into()),
        ..Request::default()
    }
}

/// Answers `request` on `harness` as a host would, giving the response and
/// the work it left.
async fn send(
    harness: &Harness<jadpo_host_native::Postgres>,
    program: &Program,
    request: &Request,
) -> (Response, Vec<Deferred>) {
    let app = program.app();
    let mut ctx = harness.ctx(program);
    let response = handle(&app, program, &mut ctx, request, "req_test").await;
    (response, ctx.take_deferred())
}

async fn count(harness: &Harness<jadpo_host_native::Postgres>, table: &str) -> i64 {
    harness
        .count(&format!("SELECT count(*) FROM \"{table}\""))
        .await
}

fn body(response: &Response) -> serde_json::Value {
    serde_json::from_slice(&response.body)
        .unwrap_or_else(|_| panic!("JSON: {}", String::from_utf8_lossy(&response.body)))
}

/// A call ringing is answered with the reply `support.answer_call` gives,
/// encoded as Acme Voice spells it, after its Twilio signature is checked;
/// the call is noted, and nothing is kept for subscribers. Twilio asking
/// again is answered with the reply kept, without running the action
/// again.
#[test]
fn a_webhook_that_needs_a_reply_is_answered_by_its_action_and_a_repeat_by_its_reply() {
    let Some(loaded) = load("reply") else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let program = &loaded.program;
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let first = ringing("CA100", "+447700900123", VOICE_TOKEN);
        let (response, _) = send(&harness, program, &first).await;
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        assert_eq!(response.header("content-type"), Some("application/json"));
        assert_eq!(response.route.as_deref(), Some("POST /webhooks/voice"));
        let reply = body(&response);
        assert_eq!(
            reply,
            serde_json::json!({"say": "Thanks for calling.", "hangUp": true})
        );
        let calls = harness
            .rows("SELECT \"sid\", \"caller\", \"rang_at\" FROM \"call\"")
            .await;
        // Twilio signs no time, so the call rang when it arrived.
        assert_eq!(
            calls,
            vec![vec![
                SqlValue::Text("CA100".into()),
                SqlValue::Text("+447700900123".into()),
                SqlValue::Timestamp(T0 * 1000),
            ]]
        );
        assert_eq!(count(&harness, "jadpo_outbox").await, 0);
        let kept = harness
            .rows("SELECT \"source\", \"event_id\", \"reply\" FROM \"jadpo_received_events\"")
            .await;
        assert_eq!(
            kept,
            vec![vec![
                SqlValue::Text("webhook voice".into()),
                SqlValue::Text("CA100".into()),
                SqlValue::Text(String::from_utf8(response.body.clone()).expect("text")),
            ]]
        );
        // Twilio asks again: the same reply, the call noted once.
        harness.advance(5_000);
        let (again, _) = send(&harness, program, &first).await;
        assert_eq!(again.status, 200);
        assert_eq!(again.body, response.body);
        assert_eq!(count(&harness, "call").await, 1);
        // Signed with another token: refused before anything runs.
        let forged = ringing("CA101", "+447700900123", "54321");
        let (refused, _) = send(&harness, program, &forged).await;
        assert_eq!(refused.status, 401);
        assert_eq!(body(&refused)["error"]["code"], "unauthenticated");
        assert_eq!(count(&harness, "call").await, 1);
        assert_eq!(count(&harness, "jadpo_received_events").await, 1);
    });
}

/// A reply the action refuses is answered as a route answers its failure
/// and keeps nothing, so Twilio's retry runs it again; a call whose first
/// request is still being answered is told so, and one a stopped process
/// left claimed is taken over once its claim lapses.
#[test]
fn a_reply_that_fails_or_is_still_being_answered_keeps_no_reply() {
    let Some(loaded) = load("reply-refused") else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let program = &loaded.program;
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let blocked = ringing("CA200", "+447700900999", VOICE_TOKEN);
        let (response, _) = send(&harness, program, &blocked).await;
        assert_eq!(
            response.status,
            422,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        assert_eq!(body(&response)["error"]["code"], "caller_blocked");
        assert_eq!(count(&harness, "jadpo_received_events").await, 0);
        // Another request holds the call's claim.
        let claimed = format!(
            "INSERT INTO \"jadpo_received_events\" (\"source\", \"event_id\", \"received_at\") VALUES ('webhook voice', 'CA201', to_timestamp({})) RETURNING 1",
            T0 / 1000
        );
        assert_eq!(harness.rows(&claimed).await.len(), 1);
        let (busy, _) = send(
            &harness,
            program,
            &ringing("CA201", "+447700900124", VOICE_TOKEN),
        )
        .await;
        assert_eq!(busy.status, 409);
        assert_eq!(
            body(&busy)["error"]["message"],
            "The webhook is still being answered."
        );
        assert_eq!(count(&harness, "call").await, 0);
        // Two minutes on, that claim has lapsed: taken over and answered.
        harness.advance(120_000);
        let (taken, _) = send(
            &harness,
            program,
            &ringing("CA201", "+447700900124", VOICE_TOKEN),
        )
        .await;
        assert_eq!(
            taken.status,
            200,
            "{}",
            String::from_utf8_lossy(&taken.body)
        );
        assert_eq!(count(&harness, "call").await, 1);
    });
}

/// Acme Chat's address check is answered with its challenge once it is
/// signed, and refused when it isn't; Acme Drive's validation, which
/// Microsoft Graph doesn't sign, is answered with its token as plain text.
#[test]
fn slack_and_microsoft_graph_handshakes_are_answered() {
    let Some(loaded) = load("handshakes") else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let program = &loaded.program;
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let challenge = r#"{"token":"Jhj5dZrVaK7ZwHHjRyZWjbDl","challenge":"3eZbrw1aBm2rZgRNFdxV2595E9CY3gmdALWMmHkvFXO7tYXAYM8P","type":"url_verification"}"#;
        let (response, _) = send(&harness, program, &chat(challenge, CHAT_SECRET, T0)).await;
        assert_eq!(
            response.status,
            200,
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        assert_eq!(
            body(&response),
            serde_json::json!({"challenge": "3eZbrw1aBm2rZgRNFdxV2595E9CY3gmdALWMmHkvFXO7tYXAYM8P"})
        );
        let (refused, _) = send(&harness, program, &chat(challenge, "not-the-secret", T0)).await;
        assert_eq!(refused.status, 401);
        // A signed message is an event like any other.
        let posted = r#"{"event_id":"Ev1","type":"event_callback","event":{"type":"message","text":"Hello"}}"#;
        let (kept, _) = send(&harness, program, &chat(posted, CHAT_SECRET, T0)).await;
        assert_eq!(kept.status, 204, "{}", String::from_utf8_lossy(&kept.body));
        assert_eq!(count(&harness, "jadpo_outbox").await, 1);
        // Microsoft Graph's validation: unsigned, its token back as text.
        let validation = Request {
            method: "POST".into(),
            path: "/webhooks/drive".into(),
            query: Some("validationToken=Validation%3A%20Testing%20client%20application%20reachability".into()),
            headers: vec![("content-type".into(), "text/plain".into())],
            address: Some("192.0.2.23".into()),
            ..Request::default()
        };
        let (validated, _) = send(&harness, program, &validation).await;
        assert_eq!(validated.status, 200);
        assert_eq!(
            validated.header("content-type"),
            Some("text/plain; charset=utf-8")
        );
        assert_eq!(validated.header("x-content-type-options"), Some("nosniff"));
        assert_eq!(
            validated.body,
            b"Validation: Testing client application reachability"
        );
        // Without the token, Acme Drive's own signature is checked.
        let mut unsigned = validation.clone();
        unsigned.query = None;
        unsigned.body = br#"{"id":"f1","type":"file.changed","path":"/a"}"#.to_vec();
        let (refused, _) = send(&harness, program, &unsigned).await;
        assert_eq!(refused.status, 401);
        assert_eq!(count(&harness, "jadpo_outbox").await, 1);
    });
}

/// What AWS's SNS host served, by request line.
type Served = Arc<Mutex<Vec<String>>>;

/// A server on this machine standing in for AWS's SNS host: the
/// certificate at any `.pem` path, `200` to a subscription's confirmation,
/// `404` to anything else; keeping each request line it was sent.
fn aws() -> (std::net::SocketAddr, Served) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let address = listener.local_addr().expect("its address");
    let served: Served = Arc::default();
    let seen = Arc::clone(&served);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().expect("a stream"));
            let mut first = String::new();
            let _ = reader.read_line(&mut first);
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
            }
            let first = first.trim_end().to_string();
            let path = first.split(' ').nth(1).unwrap_or_default().to_string();
            seen.lock().expect("held").push(first);
            let (status, body) = if path.ends_with(".pem") {
                ("200 OK", CERTIFICATE.to_string())
            } else if path.starts_with("/?Action=ConfirmSubscription") {
                ("200 OK", "<ConfirmSubscriptionResponse/>".to_string())
            } else {
                ("404 Not Found", String::new())
            };
            let mut stream = stream;
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (address, served)
}

/// An SNS notification from the orders topic is checked with the
/// certificate fetched from AWS's host, once for both signature versions,
/// and kept as its event, its time the one SNS signed; a message from
/// another topic, or one changed after it was signed, is refused without
/// fetching anything more; a subscription's confirmation is confirmed by
/// visiting its address on AWS's host; and with AWS out of reach a message
/// is answered `503`, so SNS sends it again.
#[test]
fn an_sns_message_is_checked_with_the_certificate_fetched_from_aws() {
    let Some(loaded) = load("sns") else {
        return;
    };
    let (address, served) = aws();
    let harness = Harness::postgres(&loaded.url)
        .at(T0)
        .random_from(0)
        .outbound(Rc::new(Outward::reaching(SNS_HOST, address)));
    let program = &loaded.program;
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let (response, _) = send(&harness, program, &sns(NOTIFIED_V1)).await;
        assert_eq!(
            response.status,
            204,
            "{:?} {}",
            response.code,
            String::from_utf8_lossy(&response.body)
        );
        let (response, _) = send(&harness, program, &sns(NOTIFIED_V2)).await;
        assert_eq!(response.status, 204, "{:?}", response.code);
        assert_eq!(
            served.lock().expect("held").as_slice(),
            ["GET /SimpleNotificationService-7b2test.pem HTTP/1.1"]
        );
        let payloads = harness
            .rows("SELECT \"payload\"::text FROM \"jadpo_outbox\" ORDER BY \"payload\"::text")
            .await;
        let payload = |at: usize| -> serde_json::Value {
            let [SqlValue::Text(text)] = payloads[at].as_slice() else {
                panic!("a payload: {payloads:?}");
            };
            serde_json::from_str(text).expect("JSON")
        };
        assert_eq!(payload(0)["Message"]["order_id"], "o-1");
        assert_eq!(payload(0)["occurred_at"], "2026-10-19T08:59:00Z");
        assert_eq!(payload(1)["Message"]["order_id"], "o-2");
        // Sent again: a repeat by its `MessageId`.
        let (response, _) = send(&harness, program, &sns(NOTIFIED_V1)).await;
        assert_eq!(response.status, 204);
        assert_eq!(count(&harness, "jadpo_outbox").await, 2);
        // Someone else's topic, and a message changed after signing.
        let changed = NOTIFIED_V1.replace("m-1", "m-9");
        for (why, message) in [
            ("another topic", FROM_ELSEWHERE),
            ("changed", changed.as_str()),
        ] {
            let (response, _) = send(&harness, program, &sns(message)).await;
            assert_eq!(response.status, 401, "{why}");
            assert_eq!(
                body(&response)["error"]["message"],
                "The webhook's signature doesn't match.",
                "{why}"
            );
        }
        // A certificate anywhere but AWS's SNS host isn't fetched.
        let elsewhere = NOTIFIED_V1
            .replace("m-1", "m-8")
            .replace(SNS_HOST, "sns.eu-west-2.amazonaws.com.evil.example");
        let (response, _) = send(&harness, program, &sns(&elsewhere)).await;
        assert_eq!(response.status, 401);
        assert_eq!(served.lock().expect("held").len(), 1);
        assert_eq!(count(&harness, "jadpo_outbox").await, 2);
        // The subscription is confirmed on AWS's host.
        let (response, _) = send(&harness, program, &sns(CONFIRMATION)).await;
        assert_eq!(response.status, 204, "{:?}", response.code);
        let served_now = served.lock().expect("held").clone();
        assert_eq!(served_now.len(), 2, "{served_now:?}");
        assert!(
            served_now[1].starts_with(&format!(
                "GET /?Action=ConfirmSubscription&TopicArn={TOPIC}&Token=tok-1 "
            )),
            "{served_now:?}"
        );
        assert_eq!(count(&harness, "jadpo_outbox").await, 2);
    });
    // With nothing to fetch the certificate through, SNS is asked to send
    // it again later.
    let unreached = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        // A certificate this thread hasn't fetched.
        let other = NOTIFIED_V2.replace("7b2test.pem", "7b2other.pem");
        let (response, _) = send(&unreached, program, &sns(&other)).await;
        assert_eq!(response.status, 503, "{:?}", response.code);
        assert_eq!(body(&response)["error"]["code"], "unavailable");
    });
}

/// `jadpo check` on the project with `changes`, each a file's path and
/// its text in place of the project's (`None` to remove it): every
/// problem's headline and rule, as `rule: headline` lines, but for the
/// migrations and records the change puts out of date.
fn check_with(name: &str, changes: &[(&str, Option<&str>)]) -> Vec<String> {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("webhooks-7.7b-check-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        if changes.iter().all(|(changed, _)| changed != path) {
            write(&root, path, text);
        }
    }
    for (path, text) in changes {
        if let Some(text) = text {
            write(&root, path, text);
        }
    }
    let out = jadpo(&root, &["check", "--json"]);
    let report: serde_json::Value = serde_json::from_str(&out.stdout)
        .unwrap_or_else(|_| panic!("JSON: {}{}", out.stdout, out.stderr));
    report["diagnostics"]
        .as_array()
        .map(|all| {
            all.iter()
                .map(|d| {
                    format!(
                        "{}: {}",
                        d["rule"].as_str().unwrap_or_default(),
                        d["message"].as_str().unwrap_or_default()
                    )
                })
                .filter(|line| !line.starts_with("migration.") && !line.starts_with("record."))
                .collect()
        })
        .unwrap_or_default()
}

/// The project checks as written.
#[test]
fn the_help_desk_checks() {
    assert_eq!(check_with("clean", &[]), Vec::<String>::new());
}

/// A case of the replies' checks: its name, the files it changes, and
/// the problems expected.
type ReplyCase = (&'static str, Vec<(&'static str, String)>, Vec<&'static str>);

/// Each event that needs a reply is answered by an action of the app's
/// taking the event and returning its reply, named in `replies:` by the
/// event's lower_snake name; a reply is an object of the package's own; no
/// other event names one; and no subscriber takes an event that is
/// answered.
#[test]
fn replies_are_checked() {
    let app = |replies: &str| {
        FILES[0].1.replace(
            "                replies: {\n                    incoming_call: support.answer_call\n                }\n",
            replies,
        )
    };
    const ACTION: &str = "domains/support/actions/answer_call.jadpo";
    let cases: Vec<ReplyCase> = vec![
        (
            "no-replies",
            vec![("app.jadpo", app("").replace("use support\n\n", ""))],
            vec!["connection.key_missing: `voice`'s `webhooks:` doesn't answer `IncomingCall`"],
        ),
        (
            "unknown-event",
            vec![(
                "app.jadpo",
                app("                replies: {\n                    incoming_call: support.answer_call\n                    call_ended: support.answer_call\n                }\n"),
            )],
            vec!["connection.key_invalid: `voice`'s `replies:` has no event `call_ended`"],
        ),
        (
            "not-an-action",
            vec![(
                "app.jadpo",
                app("                replies: {\n                    incoming_call: support.pick_up\n                }\n"),
            )],
            vec!["connection.value_invalid: `voice`'s reply to `incoming_call` isn't an action answering it"],
        ),
        (
            "wrong-result",
            vec![(
                ACTION,
                "use shared/providers/acme_voice\n\n/// Answers a call with the call.\naction answer_call(event: AcmeVoice.IncomingCall) -> AcmeVoice.IncomingCall {\n    return event\n}\n".to_string(),
            )],
            vec!["connection.value_invalid: `voice`'s reply to `incoming_call` isn't an action answering it"],
        ),
        (
            "reply-not-object",
            vec![(
                "shared/providers/acme_voice/v1/events/incoming_call.jadpo",
                "/// A call is ringing.\npublic type IncomingCall = Event {\n    wire_name: \"ringing\"\n    reply: Text\n    occurred_at: Instant\n    call_sid: Text {\n        wire_name: \"CallSid\"\n    }\n    from: Text {\n        wire_name: \"From\"\n    }\n}\n".to_string(),
            )],
            vec![
                "package.wire_invalid: Webhook event `IncomingCall`'s `reply:` isn't an object type of `AcmeVoice.v1`",
            ],
        ),
        (
            "app-event-reply",
            vec![(
                "domains/support/events/call_noted.jadpo",
                "/// A call was noted.\ntype CallNoted = Event {\n    reply: Text\n    sid: Text\n}\n".to_string(),
            )],
            vec!["type.reserved_field_name: `reply` can't be a field's name"],
        ),
        (
            "subscribed",
            vec![(
                "domains/support/subscribers/call_rang.jadpo",
                "use support/actions { answer_call }\nuse shared/providers/acme_voice\n\n/// Answers each call.\nsubscriber call_rang {\n    on: AcmeVoice.IncomingCall\n    run: answer_call(event)\n}\n".to_string(),
            )],
            vec!["subscriber.event_invalid: `AcmeVoice.IncomingCall` is answered in the provider's request"],
        ),
    ];
    for (name, changes, expected) in cases {
        let changes: Vec<(&str, Option<&str>)> = changes
            .iter()
            .map(|(path, text)| (*path, Some(text.as_str())))
            .collect();
        let found = check_with(name, &changes);
        assert_eq!(found, expected, "{name}");
    }
}

/// A handshake is one of the built-in ones, signed as its provider signs.
#[test]
fn handshakes_are_checked() {
    const PROVIDER: &str = "shared/providers/acme_chat/v1/provider.jadpo";
    let provider = |webhooks: &str| {
        format!(
            "/// Acme Chat's API.\nprovider {{\n    protocol: Protocol.json_http\n    credential: CredentialScheme.bearer\n    webhooks: {{\n{webhooks}\n    }}\n}}\n"
        )
    };
    let cases: &[(&str, &str, &[&str])] = &[
        (
            "unknown",
            "        check: SignatureScheme.slack\n        event_id: \"event_id\"\n        event_type: \"event.type\"\n        handshake: WebhookHandshake.discord",
            &["package.provider_invalid: `WebhookHandshake.discord` isn't a webhook handshake"],
        ),
        (
            "signed-otherwise",
            "        check: SignatureScheme.hmac_sha256\n        header: \"X-Signature\"\n        event_id: \"event_id\"\n        event_type: \"event.type\"\n        handshake: WebhookHandshake.slack",
            &["package.provider_invalid: `WebhookHandshake.slack` needs `SignatureScheme.slack`"],
        ),
    ];
    for (name, webhooks, expected) in cases {
        let text = provider(webhooks);
        let found = check_with(name, &[(PROVIDER, Some(&text))]);
        assert_eq!(&found, expected, "{name}");
    }
}
