//! Signed sessions (area 8, chunk 5.1b): `revocation_delay:` with each
//! method's `signing_key:` read from a `SigningKey` setting, a browser
//! session's `origin:` from a `Url` setting, the generated `POST
//! /auth/exchange` keeping its path, and its place in the API record and
//! OpenAPI.

use jadpo_db::Layout;

use crate::support;

/// A user and a service, the settings, and whatever `application`,
/// `authentication` and code follow.
fn with(settings: &str, application: &str, authentication: &str, code: &str) -> Vec<String> {
    let text = format!(
        r#"config {{
{settings}}}

{application}

principal {{
    user: {{
        subject: Member.handle
    }}
    service: {{
        subject: Robot.name
    }}
}}

{authentication}

type Handle = Text {{
    max_length: 40
}}

entity Member {{
    id: Uuid
    handle: Handle {{
        unique: true
    }}
}}

entity Robot {{
    id: Uuid
    name: Handle {{
        unique: true
    }}
}}

{code}"#
    );
    support::problems(Layout::Loose, &[("main.jadpo", text.as_str())])
}

const SETTINGS: &str =
    "    session_signing_key: SigningKey\n    browser_origin: Url\n    greeting: Text\n";

const DELAY: &str = "application {\n    revocation_delay: 5m\n}";

const SIGNED: &str = r#"authentication {
    browser_session: {
        cookie: "session"
        signing_key: config.session_signing_key
        origin: config.browser_origin
    }
    api_token: {
        signing_key: config.session_signing_key
    }
    service_key: {
        signing_key: config.session_signing_key
    }
}"#;

#[test]
fn a_revocation_delay_signs_with_each_methods_key() {
    assert_eq!(with(SETTINGS, DELAY, SIGNED, ""), Vec::<String>::new());
}

#[test]
fn signing_keys_and_an_origin_need_no_delay() {
    assert_eq!(
        with(SETTINGS, "application {}", SIGNED, ""),
        Vec::<String>::new()
    );
}

#[test]
fn with_a_delay_every_method_needs_a_signing_key() {
    let found = with(
        SETTINGS,
        DELAY,
        "authentication {\n    browser_session: {\n        cookie: \"session\"\n    }\n    api_token: {}\n    service_key: {}\n}",
        "",
    );
    assert_eq!(
        found,
        [
            "main.jadpo semantic.auth_setting_required: `browser_session:` has no `signing_key:`",
            "main.jadpo semantic.auth_setting_required: `api_token:` has no `signing_key:`",
            "main.jadpo semantic.auth_setting_required: `service_key:` has no `signing_key:`",
        ]
    );
}

#[test]
fn a_signing_key_and_an_origin_read_settings_of_their_types() {
    let found = with(
        SETTINGS,
        DELAY,
        r#"authentication {
    browser_session: {
        cookie: "session"
        signing_key: config.greeting
        origin: config.greeting
    }
    api_token: {
        signing_key: "0123456789abcdef0123456789abcdef"
    }
    service_key: {
        signing_key: session_signing_key
    }
}"#,
        "",
    );
    assert_eq!(
        found,
        [
            "main.jadpo semantic.auth_setting_invalid: `signing_key:` reads `greeting`, which isn't a `SigningKey`",
            "main.jadpo semantic.auth_setting_invalid: `origin:` reads `greeting`, which isn't a `Url`",
            "main.jadpo semantic.auth_setting_invalid: A signing key is written in the source",
            "main.jadpo semantic.auth_setting_invalid: `signing_key:` doesn't read a setting",
        ]
    );
}

#[test]
fn an_origin_is_never_written_in_source() {
    let found = with(
        SETTINGS,
        "application {}",
        "authentication {\n    browser_session: {\n        cookie: \"session\"\n        origin: \"https://app.example\"\n    }\n}",
        "",
    );
    assert_eq!(
        found,
        [
            "main.jadpo semantic.auth_setting_invalid: A browser session's origin is written in the source"
        ]
    );
}

#[test]
fn a_setting_no_one_declares_is_reported_where_it_is_read() {
    let found = with(
        SETTINGS,
        DELAY,
        "authentication {\n    api_token: {\n        signing_key: config.missing_key\n    }\n}",
        "",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("configuration.field_unknown"),
        "{found:?}"
    );
}

const ROUTE: &str = r#"type Done = Output {
    ok: Bool
}

action trade(robot: Principal.service) -> Done {
    var name: Handle = robot.subject
    return Done {
        ok: name != ""
    }
}

policy {
    actions: {
        trade: [Access.authenticated]
    }
}

routes Auth {
    prefix: /auth

    POST /exchange {
        output: Done
        run: trade(current_principal.service)
    }
}
"#;

#[test]
fn the_exchange_keeps_its_path_with_a_delay_and_service_keys() {
    let found = with(SETTINGS, DELAY, SIGNED, ROUTE);
    assert_eq!(
        found,
        ["main.jadpo route.path_overlap: `POST /auth/exchange` is the runtime's key exchange"]
    );
    // With no delay there is no exchange, and the path is free.
    assert_eq!(
        with(SETTINGS, "application {}", SIGNED, ROUTE),
        Vec::<String>::new()
    );
}

// --- the record and OpenAPI -------------------------------------------------------

const PROJECT: &[(&str, &str)] = &[
    (
        "config.jadpo",
        "config {\n    session_signing_key: SigningKey\n}\n",
    ),
    (
        "app.jadpo",
        r#"use robots/entities { Robot }

application {
    revocation_delay: 5m
}

principal {
    service: {
        subject: Robot.name
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        "authentication {\n    service_key: {\n        signing_key: config.session_signing_key\n    }\n}\n",
    ),
    (
        "domains/robots/types/robot_name.jadpo",
        "type RobotName = Text {\n    max_length: 40\n}\n",
    ),
    (
        "domains/robots/entities/robot.jadpo",
        r#"use robots/types { RobotName }

entity Robot {
    id: Uuid
    name: RobotName {
        unique: true
    }

    policy: {
        read: [id]
    }
}
"#,
    ),
    (
        "domains/robots/types/robot_view.jadpo",
        "use robots/entities { Robot }\n\ntype RobotView = Output {\n    pick: [Robot.name]\n}\n",
    ),
    (
        "domains/robots/queries/this_robot.jadpo",
        r#"use robots/entities { Robot, RobotNotFound }
use robots/types { RobotView }

query this_robot(robot: Principal.service) fails RobotNotFound -> RobotView {
    from: Robot
    where: Robot.id == robot.id
    missing: RobotNotFound
}
"#,
    ),
    (
        "domains/robots/routes/robots.jadpo",
        r#"use robots/queries { this_robot }
use robots/types { RobotView }

routes Robots {
    prefix: /robots

    GET /me {
        output: RobotView
        run: this_robot(current_principal.service)
    }
}
"#,
    ),
];

#[test]
fn the_record_and_openapi_list_the_exchange() {
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, PROJECT);
    let bodies = jadpo_check::routes::record::record_bodies(&db, project);
    assert_eq!(
        bodies.get("main").map(String::as_str),
        Some(
            r#"authentication: service_key

POST /auth/exchange
    generated: exchange
    caller: service
    output: ExchangedToken
    status: 200

GET /robots/me
    caller: service
    output: RobotView
    status: 200
    fails: robot_not_found 404

output ExchangedToken
    token: ServiceToken
    expires_at: Instant

output RobotView
    name: Text { max_length: 40 }

failure robot_not_found 404
"#
        )
    );
    let document = jadpo_check::routes::openapi::document(&db, project, "main", "robots", 1)
        .expect("a document");
    use jadpo_graph::json::Value;
    let exchange = document
        .get("paths")
        .and_then(|paths| paths.get("/auth/exchange"))
        .and_then(|path| path.get("post"))
        .expect("the exchange");
    assert_eq!(exchange.get("x-jadpo-generated"), Some(&Value::Bool(true)));
    let security = exchange.get("security").and_then(Value::as_array);
    assert_eq!(
        security.and_then(|all| all.first()?.get("service_key")),
        Some(&Value::Array(Vec::new()))
    );
    let success = exchange
        .get("responses")
        .and_then(|responses| responses.get("200"))
        .and_then(|ok| {
            ok.get("content")?
                .get("application/json")?
                .get("schema")?
                .get("$ref")
        })
        .and_then(Value::as_str);
    assert_eq!(success, Some("#/components/schemas/ExchangedToken"));
    let refused = exchange
        .get("responses")
        .and_then(|responses| responses.get("403"))
        .map(Value::to_document)
        .unwrap_or_default();
    assert!(refused.contains("service_disabled"), "{refused}");
    let schema = document
        .get("components")
        .and_then(|components| components.get("schemas")?.get("ExchangedToken"))
        .expect("its schema");
    assert_eq!(
        schema.get("required"),
        Some(&Value::Array(vec![
            Value::from("token"),
            Value::from("expires_at")
        ]))
    );
}
