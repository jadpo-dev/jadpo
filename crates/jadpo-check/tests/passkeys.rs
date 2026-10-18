//! Passkeys (area 8, "Passkeys", chunk 5.5z): `passkey:` in `sign_in:`,
//! its settings, what it needs beside it, and the routes it generates.

use jadpo_db::Layout;

use crate::support;

/// A user signing in with a browser session, `sign_in:` holding
/// `sign_in`, the browser session's settings `session`, and `code` after.
fn with(session: &str, sign_in: &str, code: &str) -> Vec<String> {
    let text = format!(
        r#"config {{
    browser_origin: Url
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
    support::problems(Layout::Loose, &[("main.jadpo", text.as_str())])
}

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
                &format!("semantic.auth_setting_invalid: `{written}` can't be how long a passkey's challenge lasts")
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
        "        passkey: {}\n        magic_link: {\n            link: \"/sign-in/confirm\"\n        }",
        "",
    );
    assert!(
        has(
            &found,
            "toolchain.not_built_yet: `magic_link:` isn't built yet"
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
