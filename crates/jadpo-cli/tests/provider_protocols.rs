//! What else a provider call can be (area 11, "How provider calls are
//! made"; chunk K2): a body form-encoded (`Protocol.form_http`) or sent as
//! parts (`Protocol.multipart_http`), and a host whose placeholder a
//! setting fills, the value held to one DNS label.
//!
//! Each test compiles one project whose `forms` connection reaches a
//! provider started in the test on this machine. Nothing reaches a
//! network.

use std::path::Path;
use std::rc::Rc;

use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Executor, Prepared, Program};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::{Clock, Ctx, Env, MemoryHost};
use jadpo_value::Value;

use crate::support::fake_provider::{FakeProvider, Request};

const PACKAGE: &str = "shared/providers/acme_forms/v1";

/// The project, its provider sending with `protocol` to `sandbox`, its
/// `settings:` block `settings`, and `connection` added to its connection.
fn files(protocol: &str, sandbox: &str, settings: &str, connection: &str) -> Vec<(String, String)> {
    let package = |path: &str, text: &str| (format!("{PACKAGE}/{path}"), text.to_string());
    vec![
        (
            "app.jadpo".into(),
            format!(
                r#"application {{
    connections: {{
        forms: {{
            package: AcmeForms.v1
            domains: [filing]
            credential: config.forms_api_key
            environment: config.forms_environment
{connection}        }}
    }}
}}
"#
            ),
        ),
        (
            "config.jadpo".into(),
            r#"config {
    forms_api_key: Text {
        secret: true
    }
    forms_environment: ProviderEnvironment
    forms_octet: Text
}
"#
            .into(),
        ),
        package(
            "package.jadpo",
            "/// Acme Forms' API.\npackage AcmeForms.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
        ),
        package(
            "provider.jadpo",
            &format!(
                r#"/// Acme Forms' API.
provider {{
    protocol: Protocol.{protocol}
    hosts: {{
        live: "https://api.acme-forms.example"
        sandbox: "{sandbox}"
    }}
    credential: CredentialScheme.bearer
{settings}}}
"#
            ),
        ),
        package(
            "resources/filings.jadpo",
            r#"public resource filings {
    create: {
        method: HttpMethod.post
        path: "/v1/filings"
        input: NewFiling
        output: Filing
        changes: true
        idempotent: false
        source: PackageSource.written
    }
    find: {
        method: HttpMethod.get
        path: "/v1/filings"
        input: FilingSearch
        output: Filing
        changes: false
        source: PackageSource.written
    }
}
"#,
        ),
        package(
            "types/new_filing.jadpo",
            r#"/// A filing to make.
public type NewFiling = Object {
    title: Title
    pence: Int
    note: Text?
    tags: List<Text>
    card: Card
    scan: Scan?
}
"#,
        ),
        package(
            "types/card.jadpo",
            r#"/// A card on a filing.
public type Card = Object {
    number: Text {
        wire_name: "num"
    }
    exp_month: Int
}
"#,
        ),
        package(
            "types/filing_search.jadpo",
            "/// Which filings.\npublic type FilingSearch = Object {\n    title: Title\n}\n",
        ),
        package(
            "types/title.jadpo",
            "/// A filing's title.\npublic type Title = Text {\n    max_length: 80\n}\n",
        ),
        package(
            "types/scan.jadpo",
            "/// A scanned page.\npublic type Scan = Bytes {}\n",
        ),
        package(
            "types/filing.jadpo",
            "/// A filing made.\npublic type Filing = Object {\n    id: Text\n}\n",
        ),
        (
            "domains/filing/actions/file_one.jadpo".into(),
            r#"use app { forms }
use shared/providers/acme_forms

/// Files a titled filing, with a scan when there is one.
action file_one(title: AcmeForms.Title, scan: AcmeForms.Scan?) -> AcmeForms.Filing {
    return forms.filings.create(AcmeForms.NewFiling {
        title: title
        pence: 1250
        note: none
        tags: ["a", "b c"]
        card: AcmeForms.Card {
            number: "4242"
            exp_month: 12
        }
        scan: scan
    })
}
"#
            .into(),
        ),
        (
            "domains/filing/actions/find_one.jadpo".into(),
            r#"use app { forms }
use shared/providers/acme_forms

/// Finds a filing by its title.
action find_one(title: AcmeForms.Title) -> AcmeForms.Filing {
    return forms.filings.find(AcmeForms.FilingSearch {
        title: title
    })
}
"#
            .into(),
        ),
    ]
}

fn program(root: &Path, files: Vec<(String, String)>, octet: &str) -> Program {
    let _ = std::fs::remove_dir_all(root);
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    let loaded = jadpo_db::load(root);
    let prepared = Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|problems| {
        panic!(
            "the project checks: {:?}",
            problems.iter().map(|p| &p.message).collect::<Vec<_>>()
        )
    });
    let octet = octet.to_string();
    let variables = move |name: &str| -> Option<String> {
        match name {
            "FORMS_API_KEY" => Some("sk_test_k2".into()),
            "FORMS_ENVIRONMENT" => Some("sandbox".into()),
            "FORMS_OCTET" => Some(octet.clone()),
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
    Program::new(prepared).with_config(&values)
}

/// Runs `action` with `args` in the project `files` makes of the fake
/// provider's address: what the provider was sent, and how it ended.
fn called(
    name: &str,
    files: &dyn Fn(&str) -> Vec<(String, String)>,
    octet: &str,
    action: &str,
    args: Vec<Value>,
) -> (Vec<Request>, String) {
    let provider = FakeProvider::start();
    let root = std::env::temp_dir().join(format!(
        "jadpo-provider-protocols-{name}-{}",
        std::process::id()
    ));
    let program = program(&root, files(&provider.url), octet);
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(200, r#"{"id": "f-1"}"#);
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    let host = Faked::new(MemoryHost::new()).with("forms", driver);
    let mut ctx = Ctx::of(
        &program,
        &host,
        Env {
            seed: Some(7),
            clock: Clock::TEST,
        },
    );
    let id = program.plan().find(action).expect("the action");
    let outcome = runtime.block_on(program.run(&mut ctx, id, args));
    let sent = provider.requests();
    let _ = std::fs::remove_dir_all(&root);
    (sent, format!("{outcome:?}"))
}

/// A form-encoded body is the input's members as Stripe and every Rack,
/// PHP and `qs` reader take them, in the order its JSON holds them: an
/// object's members in brackets, a list's items by place, `none` left
/// out, each escaped; the answer is JSON. A `GET` sends its query as any
/// protocol does.
#[test]
fn a_form_body_is_the_inputs_members_in_brackets() {
    let files = |sandbox: &str| files("form_http", sandbox, "", "");
    let (sent, outcome) = called(
        "form",
        &files,
        "1",
        "file_one",
        vec![Value::text("Q3 return"), Value::None],
    );
    assert!(outcome.contains("f-1"), "{outcome}");
    let [request] = sent.as_slice() else {
        panic!("one request: {sent:?}");
    };
    assert_eq!(request.method, "POST");
    assert_eq!(
        request.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(
        request.body,
        "card%5Bexp_month%5D=12&card%5Bnum%5D=4242&pence=1250&tags%5B0%5D=a&tags%5B1%5D=b%20c&title=Q3%20return"
    );
    let (sent, outcome) = called("form-get", &files, "1", "find_one", vec![Value::text("Q3")]);
    assert!(outcome.contains("f-1"), "{outcome}");
    assert_eq!(sent[0].path, "/v1/filings?title=Q3");
    assert_eq!(sent[0].body, "");
}

/// A body of parts sends each member as a part under its spelling, a
/// `Bytes` member as a file part holding its bytes, the rest as text parts
/// written as a form's members are; the boundary is drawn from what the
/// parts hold, so the same call is the same bytes.
#[test]
fn a_body_of_parts_sends_bytes_as_a_file() {
    let files = |sandbox: &str| files("multipart_http", sandbox, "", "");
    let (sent, outcome) = called(
        "parts",
        &files,
        "1",
        "file_one",
        vec![
            Value::text("Q3"),
            Value::Bytes(b"%PDF-1.7 scan".as_slice().into()),
        ],
    );
    assert!(outcome.contains("f-1"), "{outcome}");
    let [request] = sent.as_slice() else {
        panic!("one request: {sent:?}");
    };
    let content_type = request.header("content-type").expect("a content type");
    let boundary = content_type
        .strip_prefix("multipart/form-data; boundary=")
        .expect("multipart");
    assert!(boundary.starts_with("jadpo-"), "{boundary}");
    let part = |name: &str, value: &str| {
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
        )
    };
    // The members in the order the input's JSON holds them, its files
    // last.
    let expected = [
        part("card[exp_month]", "12"),
        part("card[num]", "4242"),
        part("pence", "1250"),
        part("tags[0]", "a"),
        part("tags[1]", "b c"),
        part("title", "Q3"),
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"scan\"; filename=\"scan\"\r\nContent-Type: application/octet-stream\r\n\r\n%PDF-1.7 scan\r\n"
        ),
        format!("--{boundary}--\r\n"),
    ]
    .concat();
    assert_eq!(request.body, expected);
    // The same call again is the same bytes.
    let (again, _) = called(
        "parts-again",
        &files,
        "1",
        "file_one",
        vec![
            Value::text("Q3"),
            Value::Bytes(b"%PDF-1.7 scan".as_slice().into()),
        ],
    );
    assert_eq!(again[0].body, request.body);
}

/// A host's placeholder takes its setting's value (chunk K2): read from
/// configuration, written in `app.jadpo`, or the setting's default. A
/// value that isn't one DNS label would move the call, so nothing is sent.
#[test]
fn a_hosts_placeholder_is_filled_from_its_setting() {
    let pattern = |sandbox: &str| sandbox.replace("127.0.0.1", "127.0.0.{octet}");
    let settings = "    settings: {\n        octet: Text\n    }\n";
    let read = |sandbox: &str| {
        files(
            "json_http",
            &pattern(sandbox),
            settings,
            "            octet: config.forms_octet\n",
        )
    };
    let (sent, outcome) = called("fill-read", &read, "1", "find_one", vec![Value::text("Q3")]);
    assert_eq!(sent.len(), 1, "{outcome}");
    assert!(outcome.contains("f-1"), "{outcome}");
    for bad in ["1:9", "1/x", "-1", "1.0", "1@x"] {
        let (sent, outcome) = called("fill-bad", &read, bad, "find_one", vec![Value::text("Q3")]);
        assert!(sent.is_empty(), "`{bad}` was sent: {sent:?}");
        assert!(outcome.contains("misconfigured"), "`{bad}`: {outcome}");
    }
    let written = |sandbox: &str| {
        files(
            "json_http",
            &pattern(sandbox),
            settings,
            "            octet: \"1\"\n",
        )
    };
    let (sent, outcome) = called(
        "fill-written",
        &written,
        "9",
        "find_one",
        vec![Value::text("Q3")],
    );
    assert_eq!(sent.len(), 1, "{outcome}");
    let defaulted = |sandbox: &str| {
        files(
            "json_http",
            &pattern(sandbox),
            "    settings: {\n        octet: Text {\n            default: \"1\"\n        }\n    }\n",
            "",
        )
    };
    let (sent, outcome) = called(
        "fill-default",
        &defaulted,
        "9",
        "find_one",
        vec![Value::text("Q3")],
    );
    assert_eq!(sent.len(), 1, "{outcome}");
}
