//! Confirming one request with a passkey (area 8, "Confirm one request
//! with a passkey"; chunk R1): a route's `confirm: Confirmation.passkey`,
//! what it needs (a signed-in person, and passkeys), the `…/confirmation`
//! route the runtime answers beside it, what the API record and OpenAPI
//! say of both, and `test_passkeys.confirm`, which only tests call.

use jadpo_db::{Database, Layout};

use crate::support;

/// An application signing members in with a browser session, magic links
/// and, when `passkeys`, passkeys, with `code` after.
fn files(passkeys: bool, code: &str) -> Vec<(&'static str, String)> {
    let passkey = if passkeys {
        "        passkey: {}\n"
    } else {
        ""
    };
    let text = format!(
        r#"config {{
    browser_origin: Url
    mail_sender: Email
    smtp_host: Hostname
}}

application {{
    display_name: "Notes"
    connections: {{
        mail: {{
            driver: Driver.smtp
            host: config.smtp_host
        }}
    }}
    email: {{
        connection: mail
        from: config.mail_sender
    }}
}}

principal {{
    user: {{
        subject: Member.email
    }}
}}

authentication {{
    browser_session: {{
        cookie: "session"
        origin: config.browser_origin
    }}
    sign_in: {{
        magic_link: {{
            link: "/sign-in/confirm"
        }}
{passkey}    }}
}}

entity Member {{
    id: Uuid
    email: Email {{
        unique: true
    }}
}}

{code}"#
    );
    vec![("app.jadpo", text)]
}

fn problems(passkeys: bool, code: &str) -> Vec<String> {
    let files = files(passkeys, code);
    let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    support::problems(Layout::Loose, &files)
}

fn has(found: &[String], wanted: &str) -> bool {
    found.iter().any(|problem| problem.contains(wanted))
}

/// Closing a member's account, which a route confirms with `confirm`.
fn closing(confirm: &str) -> String {
    format!(
        r#"type Reason = Text {{
    max_length: 200
}}

type Closing = Input {{
    reason: Reason
}}

/// Closes the signed-in member's account.
action close_account(input: Closing) -> Unit {{
    var reason: Reason = input.reason
    assert reason != ""
}}

routes Account {{
    prefix: /account

    POST /close {{
{confirm}        input: Closing
        status: SuccessStatus.no_content
        run: close_account(input)
    }}
}}

policy {{
    actions: {{
        close_account: [Access.authenticated]
    }}
}}
"#
    )
}

const CONFIRMED: &str = "        confirm: Confirmation.passkey\n";

#[test]
fn a_route_confirmed_with_a_passkey_checks() {
    assert_eq!(problems(true, &closing(CONFIRMED)), Vec::<String>::new());
}

/// The record says the route is confirmed, and lists the `…/confirmation`
/// the runtime answers beside it, reading the same input.
#[test]
fn the_record_lists_the_confirmation() {
    let files = files(true, &closing(CONFIRMED));
    let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &files);
    let records = jadpo_check::routes::record::record_texts(&db, project);
    let record = records.get("main").expect("a record");
    assert!(
        record.contains(
            "POST /account/close\n    confirm passkey\n    input Closing (body)\n    status 204\n"
        ),
        "{record}"
    );
    assert!(
        record.contains(
            "POST /account/close/confirmation\n    generated confirmation\n    caller user\n    input Closing (body)\n    output PasskeyRequestOptions\n    status 200\n"
        ),
        "{record}"
    );
    // OpenAPI says the same: the answer's header, the refusal, and the
    // generated options.
    let document = jadpo_check::routes::openapi::document(&db, project, "main", "notes", "1")
        .expect("a document");
    let paths = document.get("paths").expect("paths");
    let confirmed = paths
        .get("/account/close")
        .and_then(|path| path.get("post"))
        .expect("the route");
    let shown = format!("{confirmed:?}");
    for part in [
        "x-jadpo-confirm",
        "X-Jadpo-Confirmation",
        "confirmation_required",
    ] {
        assert!(shown.contains(part), "{part}: {shown}");
    }
    let options = paths
        .get("/account/close/confirmation")
        .and_then(|path| path.get("post"))
        .expect("the confirmation");
    let shown = format!("{options:?}");
    for part in [
        "x-jadpo-generated",
        "post_account_close_confirmation",
        "requestBody",
        "confirmation_required",
    ] {
        assert!(shown.contains(part), "{part}: {shown}");
    }
}

#[test]
fn only_a_passkey_confirms_a_request() {
    let found = problems(true, &closing("        confirm: passkey\n"));
    assert!(
        has(
            &found,
            "route.confirm_invalid: `passkey` isn't a `Confirmation`"
        ),
        "{found:?}"
    );
    let found = problems(true, &closing("        confirm: Confirmation.password\n"));
    assert!(
        has(
            &found,
            "route.confirm_invalid: `Confirmation.password` isn't a `Confirmation`"
        ),
        "{found:?}"
    );
}

#[test]
fn a_route_anyone_may_call_has_no_one_to_confirm_it() {
    let code = closing(&format!(
        "        authentication: RouteAuthentication.none\n{CONFIRMED}"
    ))
    .replace("[Access.authenticated]", "[Access.public]");
    let found = problems(true, &code);
    assert!(
        has(
            &found,
            "route.confirm_invalid: `POST /account/close` is open to anyone, so no one signed in can confirm it"
        ),
        "{found:?}"
    );
}

#[test]
fn confirming_needs_passkeys() {
    let found = problems(false, &closing(CONFIRMED));
    assert!(
        has(
            &found,
            "semantic.auth_setting_required: `POST /account/close`'s `confirm:` needs passkeys"
        ),
        "{found:?}"
    );
}

/// Two confirmed routes at one path each have their own, asked with their
/// method, but a `GET`'s is a `POST`, which a `POST` beside it would share.
#[test]
fn each_confirmed_route_has_its_own_confirmation() {
    let beside = |method: &str| {
        closing(CONFIRMED).replace(
            "}\n\npolicy {",
            &format!(
                "\n    {method} /close {{\n        confirm: Confirmation.passkey\n        input: Closing\n        status: SuccessStatus.no_content\n        run: close_account(input)\n    }}\n}}\n\npolicy {{"
            ),
        )
    };
    let code = beside("PUT");
    assert_eq!(problems(true, &code), Vec::<String>::new());
    let files = files(true, &code);
    let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &files);
    let records = jadpo_check::routes::record::record_texts(&db, project);
    let record = records.get("main").expect("a record");
    assert!(
        record.contains("POST /account/close/confirmation\n"),
        "{record}"
    );
    assert!(
        record.contains("PUT /account/close/confirmation\n"),
        "{record}"
    );
    let query = r#"type MemberCard = Output {
    email: Email
}

/// Everyone's addresses.
query closing_preview() -> List<MemberCard> {
    from: Member
    order: [Member.email asc]
}
"#;
    let code = closing(CONFIRMED).replace(
        "}\n\npolicy {",
        "\n    GET /close {\n        confirm: Confirmation.passkey\n        output: List<MemberCard>\n        run: closing_preview()\n    }\n}\n\npolicy {",
    ) + query;
    let found = problems(true, &code);
    assert!(
        has(
            &found,
            "route.path_overlap: `GET /account/close` and `POST /account/close` would be confirmed through one route"
        ),
        "{found:?}"
    );
}

#[test]
fn a_route_at_a_confirmation_path_overlaps() {
    let code = closing(CONFIRMED).replace(
        "}\n\npolicy {",
        "\n    POST /close/confirmation {\n        input: Closing\n        status: SuccessStatus.no_content\n        run: close_account(input)\n    }\n}\n\npolicy {",
    );
    let found = problems(true, &code);
    assert!(
        has(
            &found,
            "route.path_overlap: `POST /account/close/confirmation` is where the runtime confirms `POST /account/close`"
        ),
        "{found:?}"
    );
}

/// A test confirming the next request with a test passkey.
const ROUTE_TESTED: &str = r#"type MemberAddress = Input {
    email: Email
}

type EmailTaken = Failure {
    kind: FailureKind.conflict
}

/// Adds a member.
action add_member(input: MemberAddress) fails EmailTaken -> Member {
    return attempt Member.create {
        set: {
            email: input.email
        }
        conflict: {
            Member.email: EmailTaken
        }
    }
}

test "Ada confirms closing her account with her passkey" {
    var member: Member = attempt add_member(MemberAddress {
        email: "ada@example.com"
    })
    var ada: Principal.user = Principal.user(member)
    var phone: TestPasskey = test_passkeys.register(ada, "Phone")
    test_passkeys.confirm(phone)
    var response: Response = request {
        method: HttpMethod.post
        path: "/account/close"
        as: ada
        input: Closing {
            reason: "Moving on"
        }
    }
    assert response.status == 204
}
"#;

fn route_tested(test: &str) -> String {
    closing(CONFIRMED).replace(
        "        close_account: [Access.authenticated]\n",
        "        close_account: [Access.authenticated]\n        add_member: [Access.public]\n",
    ) + "\n"
        + test
}

#[test]
fn a_route_test_confirms_with_test_passkeys() {
    assert_eq!(
        problems(true, &route_tested(ROUTE_TESTED)),
        Vec::<String>::new()
    );
    let code = route_tested(ROUTE_TESTED).replace(
        "test_passkeys.confirm(phone)",
        "test_passkeys.confirm(phone, ada)",
    );
    let found = problems(true, &code);
    assert!(
        has(
            &found,
            "type.argument_count: `test_passkeys.confirm` takes the device's `TestPasskey`"
        ),
        "{found:?}"
    );
    let code = route_tested(ROUTE_TESTED)
        .replace("test_passkeys.confirm(phone)", "test_passkeys.confirm(ada)");
    let found = problems(true, &code);
    assert!(has(&found, "type.mismatch"), "{found:?}");
}

#[test]
fn only_a_test_confirms_with_a_test_passkey() {
    let code = closing(CONFIRMED).replace(
        "    var reason: Reason = input.reason\n",
        "    var reason: Reason = input.reason\n    test_passkeys.confirm(none)\n",
    );
    let found = problems(true, &code);
    assert!(
        has(
            &found,
            "semantic.request_invalid: `test_passkeys.confirm` is only for route tests"
        ),
        "{found:?}"
    );
}

/// The wording of `confirmation_required` is asked of an app's locales only
/// once a route confirms, so passkeys alone need no new translation.
#[test]
fn its_wording_is_on_only_with_a_confirmed_route() {
    let on = |code: &str| {
        let files = files(true, code);
        let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
        let db = Database::default();
        let project = support::project(&db, Layout::Loose, &files);
        jadpo_check::translations::jadpo::messages_on(&db, project)
            .iter()
            .any(|message| message.key == "confirmation_required")
    };
    assert!(on(&closing(CONFIRMED)));
    assert!(!on(&closing("")));
}
