//! Magic links (area 8, "Magic links", chunk 7.3b): `magic_link:` in
//! `sign_in:`, its settings, what it needs beside it (a browser session
//! with its origin, the `email:` connection, the application's
//! `display_name:`), `on_first_sign_in:`, and the routes it generates.

use jadpo_db::Layout;

use crate::support;

/// The application, the user and its browser session, `magic_link:`'s
/// settings `magic`, `extra` inside `authentication { }` and `code`
/// after.
fn with(application: &str, magic: &str, extra: &str, code: &str) -> Vec<String> {
    let text = format!(
        r#"config {{
    browser_origin: Url
    mail_sender: Email
    smtp_host: Hostname
}}

application {{
{application}
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
{magic}
        }}
    }}
{extra}
}}

type Nickname = Text {{
    max_length: 40
}}

entity Member {{
    id: Uuid
    email: Email {{
        unique: true
    }}
    nickname: Nickname
    backup: Email
}}

{code}"#
    );
    support::problems(Layout::Loose, &[("app.jadpo", text.as_str())])
}

/// What an application that sends email declares.
const SENDS: &str = r#"    display_name: "Notes"
    connections: {
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }"#;

const LINK: &str = "            link: \"/sign-in/confirm\"";

fn magic(settings: &str) -> Vec<String> {
    with(SENDS, &format!("{LINK}\n{settings}"), "", "")
}

fn has(found: &[String], wanted: &str) -> bool {
    found.iter().any(|problem| problem.contains(wanted))
}

#[test]
fn a_magic_link_with_its_page_checks() {
    assert_eq!(with(SENDS, LINK, "", ""), Vec::<String>::new());
    assert_eq!(
        magic(
            "            address: Member.email\n            expires_after: 1h\n            code_attempts: 10\n            codes_per_hour: 3\n            failed_codes_per_day: 50\n            resend_after: 2m\n            same_browser: true\n            path: \"/account/sign-in\""
        ),
        Vec::<String>::new()
    );
}

#[test]
fn a_magic_link_needs_the_browser_sessions_origin() {
    let text = r#"config {
    mail_sender: Email
    smtp_host: Hostname
}

application {
    display_name: "Notes"
    connections: {
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
}

principal {
    user: {
        subject: Member.email
    }
}

authentication {
    browser_session: {
        cookie: "session"
    }
    sign_in: {
        magic_link: {
            link: "/sign-in/confirm"
        }
    }
}

entity Member {
    id: Uuid
    email: Email {
        unique: true
    }
}
"#;
    let found = support::problems(Layout::Loose, &[("app.jadpo", text)]);
    assert!(
        has(
            &found,
            "semantic.auth_setting_required: `magic_link:` needs the browser session's `origin:`"
        ),
        "{found:?}"
    );
}

#[test]
fn a_magic_link_needs_email_and_the_applications_name() {
    let found = with("    revocation_delay: 5m", LINK, "", "");
    assert!(
        has(
            &found,
            "email.connection_required: `magic_link:` sends email, but nothing sends email"
        ),
        "{found:?}"
    );
    assert!(
        has(
            &found,
            "semantic.auth_setting_required: `magic_link:` needs the application's `display_name:`"
        ),
        "{found:?}"
    );
}

#[test]
fn the_address_is_a_unique_email_of_the_user() {
    let found = magic("            address: Member.backup");
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `Member.backup` can't be where a link is sent"
        ),
        "{found:?}"
    );
    let found = magic("            address: Member.nickname");
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `Member.nickname` can't be where a link is sent"
        ),
        "{found:?}"
    );
    let found = magic("            address: Member.phone");
    assert!(
        has(&found, "type.unknown_field: `Member` has no field `phone`"),
        "{found:?}"
    );
    let found = magic("            address: Partner.email");
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `Partner` isn't the user entity"
        ),
        "{found:?}"
    );
}

#[test]
fn without_an_email_field_the_address_is_required() {
    let text = format!(
        r#"config {{
    browser_origin: Url
    mail_sender: Email
    smtp_host: Hostname
}}

application {{
{SENDS}
}}

type Handle = Text {{
    max_length: 40
}}

principal {{
    user: {{
        subject: Member.handle
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
    }}
}}

entity Member {{
    id: Uuid
    handle: Handle {{
        unique: true
    }}
}}
"#
    );
    let found = support::problems(Layout::Loose, &[("app.jadpo", text.as_str())]);
    assert!(
        has(
            &found,
            "semantic.auth_setting_required: `magic_link:` doesn't say where the address is kept"
        ),
        "{found:?}"
    );
}

#[test]
fn its_settings_are_checked() {
    let found = magic("            lifetime: 15m");
    assert!(
        has(
            &found,
            "semantic.auth_setting_unknown: `lifetime` isn't a `magic_link` setting"
        ),
        "{found:?}"
    );
    let found = magic("            expires_after: 30s");
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `30s` can't be how long a magic link lasts"
        ),
        "{found:?}"
    );
    let found = magic("            code_attempts: 0");
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `code_attempts:` must be a whole number from 1"
        ),
        "{found:?}"
    );
    let found = magic("            same_browser: \"yes\"");
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `same_browser:` must be `true` or `false`"
        ),
        "{found:?}"
    );
    let found = magic("            path: \"/Sign In\"");
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `path:` isn't a path of fixed segments"
        ),
        "{found:?}"
    );
}

#[test]
fn the_link_is_a_page_on_the_applications_own_site() {
    for link in [
        "\"//evil.example/confirm\"",
        "\"https://evil.example/confirm\"",
        "\"/confirm?next=/\"",
        "\"/\\\\evil.example\"",
        "\"sign-in\"",
    ] {
        let found = with(SENDS, &format!("            link: {link}"), "", "");
        assert!(
            has(
                &found,
                "semantic.auth_setting_invalid: `link:` isn't a path on the application's site"
            ),
            "{link}: {found:?}"
        );
    }
    let found = with(SENDS, "            expires_after: 15m", "", "");
    assert!(
        has(
            &found,
            "semantic.auth_setting_required: `magic_link:` doesn't say which page the link opens"
        ),
        "{found:?}"
    );
}

#[test]
fn loosening_past_the_limits_needs_an_acceptance() {
    for (setting, wanted) in [
        ("expires_after: 2h", "`expires_after:` `2h` isn't built yet"),
        ("code_attempts: 11", "`code_attempts:` `11` isn't built yet"),
        (
            "failed_codes_per_day: 51",
            "`failed_codes_per_day:` `51` isn't built yet",
        ),
    ] {
        let found = magic(&format!("            {setting}"));
        assert!(
            has(&found, &format!("toolchain.not_built_yet: {wanted}")),
            "{setting}: {found:?}"
        );
    }
}

#[test]
fn the_apps_own_email_and_sending_come_later_and_never_together() {
    let found = magic("            email: accounts.SignInEmail");
    assert!(
        has(&found, "toolchain.not_built_yet: `email:` isn't built yet"),
        "{found:?}"
    );
    let found = magic("            send: accounts.send_sign_in_link");
    assert!(
        has(&found, "toolchain.not_built_yet: `send:` isn't built yet"),
        "{found:?}"
    );
    let found = magic(
        "            email: accounts.SignInEmail\n            send: accounts.send_sign_in_link",
    );
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `magic_link:` has both `email:` and `send:`"
        ),
        "{found:?}"
    );
}

#[test]
fn sign_in_providers_is_renamed_sign_in() {
    let text = r#"principal {
    user: {
        subject: Member.email
    }
}

authentication {
    browser_session: {
        cookie: "session"
    }
    sign_in_providers: {}
}

entity Member {
    id: Uuid
    email: Email {
        unique: true
    }
}
"#;
    let found = support::problems(Layout::Loose, &[("app.jadpo", text)]);
    assert!(
        has(
            &found,
            "semantic.auth_method_unknown: `sign_in_providers:` is now `sign_in:`"
        ),
        "{found:?}"
    );
}

const FIRST: &str = "    on_first_sign_in: create_member";

#[test]
fn on_first_sign_in_names_an_action_taking_first_sign_in() {
    let code = r#"type SignUpClosed = Failure {
    kind: FailureKind.conflict
}

/// Signs a new member up, unless the address wasn't proved.
action create_member(first: FirstSignIn) fails SignUpClosed -> Member.id {
    if (first.method != SignInMethod.magic_link or not first.address_verified) {
        reject SignUpClosed {}
    }
    match first.address {
        none -> {
            reject SignUpClosed {}
        }
        some(address) -> {
            var member: Member = attempt Member.create {
                set: {
                    email: address
                    nickname: "new"
                    backup: address
                }
                conflict: {
                    Member.email: SignUpClosed
                }
            }
            return member.id
        }
    }
}
"#;
    assert_eq!(with(SENDS, LINK, FIRST, code), Vec::<String>::new());
}

#[test]
fn on_first_sign_in_with_the_wrong_shape_is_refused() {
    let takes_text = r#"action create_member(address: Email) -> Member.id {
    var member: Member = Member.create {
        set: {
            email: address
            nickname: "new"
            backup: address
        }
    }
    return member.id
}
"#;
    let found = with(SENDS, LINK, FIRST, takes_text);
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `create_member` can't decide a first sign-in"
        ),
        "{found:?}"
    );
    let gives_unit = r#"action create_member(first: FirstSignIn) -> Unit {
    assert first.address_verified
}
"#;
    let found = with(SENDS, LINK, FIRST, gives_unit);
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `create_member` can't decide a first sign-in"
        ),
        "{found:?}"
    );
    let found = with(SENDS, LINK, "    on_first_sign_in: nobody", "");
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `nobody` isn't an action"
        ),
        "{found:?}"
    );
}

#[test]
fn a_route_at_a_generated_path_overlaps() {
    let code = r#"action hello() -> Unit {
}

routes Account {
    prefix: /auth

    POST /magic-link {
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
    let found = with(SENDS, LINK, "", code);
    assert!(
        has(
            &found,
            "route.path_overlap: `POST /auth/magic-link` is the runtime's magic link request"
        ),
        "{found:?}"
    );
}
