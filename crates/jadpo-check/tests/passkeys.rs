//! Passkeys (area 8, "Passkeys", chunks 5.5z and 5.5z2): `passkey:` in
//! `sign_in:`, its settings, what it needs beside it (a browser session
//! with its origin, and magic links, how a person with no passkey left
//! gets back in), the routes it generates, its events and
//! `test_passkeys`, which only tests call.

use jadpo_db::Layout;

use crate::support;

/// A user signing in with a browser session, `sign_in:` holding magic
/// links and `sign_in`, the browser session's settings `session`, and
/// `code` after.
fn with(session: &str, sign_in: &str, code: &str) -> Vec<String> {
    with_ways(session, &format!("{MAGIC}\n{sign_in}"), code)
}

/// The same, `sign_in:` holding only `sign_in`.
fn with_ways(session: &str, sign_in: &str, code: &str) -> Vec<String> {
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
{session}
    }}
    sign_in: {{
{sign_in}
    }}
}}

entity Member {{
    id: Uuid
    email: Email {{
        unique: true
    }}
}}

{code}"#
    );
    support::problems(Layout::Loose, &[("app.jadpo", text.as_str())])
}

/// Magic links, which every application with passkeys has.
const MAGIC: &str = "        magic_link: {\n            link: \"/sign-in/confirm\"\n        }";

const SESSION: &str = "        cookie: \"session\"\n        origin: config.browser_origin";

fn has(found: &[String], wanted: &str) -> bool {
    found.iter().any(|problem| problem.contains(wanted))
}

#[test]
fn a_passkey_with_an_origin_checks() {
    assert_eq!(
        with(SESSION, "        passkey: {}", ""),
        Vec::<String>::new()
    );
    assert_eq!(
        with(
            SESSION,
            "        passkey: {\n            expires_after: 10m\n            path: \"/account/passkeys\"\n        }",
            ""
        ),
        Vec::<String>::new()
    );
}

#[test]
fn a_passkey_needs_the_browser_sessions_origin() {
    let found = with("        cookie: \"session\"", "        passkey: {}", "");
    assert!(
        has(
            &found,
            "semantic.auth_setting_required: `passkey:` needs the browser session's `origin:`"
        ),
        "{found:?}"
    );
    let text = r#"config {
    browser_origin: Url
}

principal {
    user: {
        subject: Member.email
    }
}

authentication {
    api_token: {}
    sign_in: {
        passkey: {}
    }
}

entity Member {
    id: Uuid
    email: Email {
        unique: true
    }
}
"#;
    let found = support::problems(Layout::Loose, &[("main.jadpo", text)]);
    assert!(
        has(
            &found,
            "semantic.auth_setting_required: `passkey:` needs a browser session"
        ),
        "{found:?}"
    );
}

#[test]
fn its_settings_are_checked() {
    let found = with(
        SESSION,
        "        passkey: {\n            timeout: 5m\n        }",
        "",
    );
    assert!(
        has(
            &found,
            "semantic.auth_setting_unknown: `timeout` isn't a `passkey` setting"
        ),
        "{found:?}"
    );
    for written in ["30m", "30s", "soon"] {
        let found = with(
            SESSION,
            &format!("        passkey: {{\n            expires_after: {written}\n        }}"),
            "",
        );
        assert!(
            has(
                &found,
                &format!(
                    "semantic.auth_setting_invalid: `{written}` can't be how long a passkey's challenge lasts"
                )
            ),
            "{written}: {found:?}"
        );
    }
    for written in ["\"login\"", "\"/login/{id}\"", "\"/\"", "\"/Log In\""] {
        let found = with(
            SESSION,
            &format!("        passkey: {{\n            path: {written}\n        }}"),
            "",
        );
        assert!(
            has(
                &found,
                "semantic.auth_setting_invalid: `path:` isn't a path of fixed segments"
            ),
            "{written}: {found:?}"
        );
    }
}

#[test]
fn only_the_languages_ways_go_in_sign_in() {
    let found = with(SESSION, "        password: {}", "");
    assert!(
        has(
            &found,
            "semantic.auth_method_unknown: `password` isn't a way to check a person"
        ),
        "{found:?}"
    );
    let found = with(
        SESSION,
        "        passkey: {}\n        providers: [google]",
        "",
    );
    assert!(
        has(
            &found,
            "toolchain.not_built_yet: `providers:` isn't built yet"
        ),
        "{found:?}"
    );
    let found = with(SESSION, "        passkey: {}\n        passkey: {}", "");
    assert!(
        has(
            &found,
            "semantic.auth_method_duplicate: `passkey:` is declared twice"
        ),
        "{found:?}"
    );
}

#[test]
fn a_route_at_a_passkey_path_overlaps() {
    let routes = r#"action hello() -> Unit {
}

routes Account {
    prefix: /auth/passkey

    POST /sign-in {
        authentication: RouteAuthentication.none
        status: SuccessStatus.no_content
        run: hello()
    }
}

policy {
    actions: {
        hello: [Access.public]
    }
}
"#;
    let found = with(SESSION, "        passkey: {}", routes);
    assert!(
        has(
            &found,
            "route.path_overlap: `POST /auth/passkey/sign-in` is the runtime's passkey sign-in"
        ),
        "{found:?}"
    );
    // Moved with `path:`, the same route is the application's.
    let found = with(
        SESSION,
        "        passkey: {\n            path: \"/account/passkeys\"\n        }",
        routes,
    );
    assert!(!has(&found, "route.path_overlap"), "{found:?}");
}

// --- recovery, events and route tests (chunk 5.5z2) ---------------------------

#[test]
fn a_passkey_needs_magic_links_beside_it() {
    let found = with_ways(SESSION, "        passkey: {}", "");
    assert!(
        has(
            &found,
            "semantic.auth_setting_required: `passkey:` needs `magic_link:` beside it"
        ),
        "{found:?}"
    );
    // Without an origin there is nothing to bind to, which is said first
    // and alone.
    let found = with_ways("        cookie: \"session\"", "        passkey: {}", "");
    assert!(
        has(&found, "`passkey:` needs the browser session's `origin:`"),
        "{found:?}"
    );
    assert!(!has(&found, "needs `magic_link:`"), "{found:?}");
}

/// A subscriber of each passkey event, and of `SignedIn`.
const LISTENING: &str = r#"entity PasskeyNote {
    id: Uuid
    member_id: Member.id {
        on_delete: OnDelete.cascade
    }
    passkey_id: Uuid
    added: Bool
}

/// Notes a passkey added.
action note_passkey(member_id: Member.id, event: PasskeyAdded) fails MemberNotFound -> PasskeyNote {
    return attempt PasskeyNote.create {
        set: {
            member_id: member_id
            passkey_id: event.passkey_id
            added: true
        }
    }
}

/// Notes a removal.
action note_removed(event: PasskeyRemoved) fails MemberNotFound -> PasskeyNote {
    return attempt PasskeyNote.create {
        set: {
            member_id: event.user_id
            passkey_id: event.passkey_id
            added: false
        }
    }
}

/// Notes a sign-in with a passkey.
action note_signed_in(event: SignedIn) fails MemberNotFound -> PasskeyNote {
    return attempt PasskeyNote.create {
        set: {
            member_id: event.user_id
            passkey_id: "0192d3c4-0000-7000-8000-00000000002a"
            added: event.method == SignInMethod.passkey
        }
    }
}

subscriber note_added {
    on: PasskeyAdded
    run: note_passkey(event.user_id, event)
}

subscriber note_removals {
    on: PasskeyRemoved
    run: note_removed(event)
}

subscriber note_sign_ins {
    on: SignedIn
    run: note_signed_in(event)
}
"#;

#[test]
fn subscribers_listen_for_the_passkey_events() {
    assert_eq!(
        with(SESSION, "        passkey: {}", LISTENING),
        Vec::<String>::new()
    );
    let code = LISTENING.replace(
        "note_passkey(event.user_id, event)",
        "note_passkey(event.credential_id, event)",
    );
    let found = with(SESSION, "        passkey: {}", &code);
    assert!(
        has(
            &found,
            "type.unknown_field: `PasskeyAdded` has no field `credential_id`"
        ),
        "{found:?}"
    );
}

#[test]
fn nothing_publishes_a_passkey_event_without_passkeys() {
    let found = with(SESSION, "", LISTENING);
    assert!(
        has(
            &found,
            "subscriber.event_invalid: Nothing publishes `PasskeyAdded`"
        ),
        "{found:?}"
    );
    assert!(
        has(
            &found,
            "subscriber.event_invalid: Nothing publishes `PasskeyRemoved`"
        ),
        "{found:?}"
    );
    // Magic links publish `SignedIn` without passkeys.
    assert!(!has(&found, "`SignedIn`"), "{found:?}");
}

#[test]
fn only_the_runtime_emits_a_passkey_event() {
    let code = r#"test "a passkey is announced" {
    emit PasskeyAdded {
        user_id: "0192d3c4-0000-7000-8000-00000000002a"
        passkey_id: "0192d3c4-0000-7000-8000-00000000002b"
    }
}
"#;
    let found = with(SESSION, "        passkey: {}", code);
    assert!(
        has(
            &found,
            "event.emit_invalid: `PasskeyAdded` is published only by adding a passkey"
        ),
        "{found:?}"
    );
    let code = r#"action announce() -> Unit {
    var removed: PasskeyRemoved = PasskeyRemoved {
        user_id: "0192d3c4-0000-7000-8000-00000000002a"
        passkey_id: "0192d3c4-0000-7000-8000-00000000002b"
    }
}
"#;
    let found = with(SESSION, "        passkey: {}", code);
    assert!(
        has(
            &found,
            "type.not_record: `PasskeyRemoved` isn't built with `{ ... }`"
        ),
        "{found:?}"
    );
}

/// A member made by an action a test calls, and a test driving the
/// passkey routes with the software authenticator.
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

policy {
    actions: {
        add_member: [Access.public]
    }
}

test "a passkey signs Ada in" {
    var member: Member = attempt add_member(MemberAddress {
        email: "ada@example.com"
    })
    var ada: Principal.user = Principal.user(member)
    var phone: TestPasskey = test_passkeys.register(ada, "Phone")
    var response: Response = test_passkeys.sign_in(phone)
    assert response.status == 204
    assert response.code == none
}
"#;

#[test]
fn a_route_test_drives_the_passkey_routes() {
    assert_eq!(
        with(SESSION, "        passkey: {}", ROUTE_TESTED),
        Vec::<String>::new()
    );
    let code = ROUTE_TESTED.replace(
        "test_passkeys.register(ada, \"Phone\")",
        "test_passkeys.register(member, \"Phone\")",
    );
    let found = with(SESSION, "        passkey: {}", &code);
    assert!(has(&found, "type.mismatch"), "{found:?}");
    let code = ROUTE_TESTED.replace(
        "test_passkeys.sign_in(phone)",
        "test_passkeys.sign_in(phone, ada)",
    );
    let found = with(SESSION, "        passkey: {}", &code);
    assert!(
        has(
            &found,
            "type.argument_count: `test_passkeys.sign_in` takes the device's `TestPasskey`"
        ),
        "{found:?}"
    );
}

#[test]
fn only_a_test_holds_a_test_passkey() {
    let code = r#"type Named = Input {
    name: Text
}

/// Tries to make a device outside a test.
action pretend(input: Named) -> Unit {
    var phone: TestPasskey = test_passkeys.register(current_principal.user, input.name)
}
"#;
    let found = with(SESSION, "        passkey: {}", code);
    assert!(
        has(
            &found,
            "semantic.request_invalid: `test_passkeys.register` is only for route tests"
        ),
        "{found:?}"
    );
    // A test with no passkeys to drive.
    let found = with(SESSION, "", ROUTE_TESTED);
    assert!(
        has(
            &found,
            "semantic.auth_setting_required: `test_passkeys.register` needs passkeys"
        ),
        "{found:?}"
    );
}
