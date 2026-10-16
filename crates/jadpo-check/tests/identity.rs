//! API tokens and service keys (area 8, chunk 4.3b): the generated
//! operations' contexts, the methods each needs, the entity `issue_key`
//! belongs to, and signing in as a service from a route test.

use jadpo_db::Layout;

use crate::support;

/// A user and a service, with whatever `authentication` and code follow.
fn with(authentication: &str, code: &str) -> Vec<String> {
    let text = format!(
        r#"principal {{
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

const BOTH: &str = "authentication {\n    api_token: {}\n    service_key: {}\n}";

#[test]
fn every_operation_checks_in_an_action() {
    let found = with(
        BOTH,
        r#"type Issued = Output {
    token: ApiToken
    key: ServiceKey
}

type Until = Instant {}

action issue(member: Member, robot: Robot, until: Until) -> Issued {
    for token in sessions.tokens(member.id) {
        sessions.revoke_token(token.id)
    }
    for key in Robot.keys(robot.id) {
        var ends: Instant = key.ends_at
        assert ends > until
        Robot.revoke_key(key.id)
    }
    sessions.end()
    return Issued {
        token: sessions.issue_token(member.id)
        key: Robot.issue_key(robot.id, until)
    }
}
"#,
    );
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn only_an_action_calls_them() {
    let found = with(
        BOTH,
        r#"test "listing keys" {
    var robot: Robot = Robot.create {
        set: {
            name: "r2"
        }
    }
    var keys: List<IssuedKey> = Robot.keys(robot.id)
    assert collection.count(keys) == 0
}
"#,
    );
    assert!(
        found
            .iter()
            .any(|p| p.contains("type.auth_sessions_context: A test can't call `Robot.keys`")),
        "{found:?}"
    );
}

#[test]
fn each_operation_needs_its_method() {
    let found = with(
        "authentication {\n    service_key: {}\n}",
        r#"action issue(member: Member) -> Unit {
    sessions.revoke_token(member.id)
}
"#,
    );
    assert!(
        found
            .iter()
            .any(|p| p
                .contains("type.auth_sessions_context: `sessions.revoke_token` needs API tokens")),
        "{found:?}"
    );
    let found = with(
        "authentication {\n    api_token: {}\n}",
        r#"action issue(robot: Robot) -> Unit {
    Robot.revoke_key(robot.id)
}
"#,
    );
    assert!(
        found.iter().any(
            |p| p.contains("type.auth_sessions_context: `Robot.revoke_key` needs service keys")
        ),
        "{found:?}"
    );
}

#[test]
fn keys_belong_to_the_service_entity_only() {
    let found = with(
        BOTH,
        r#"action issue(member: Member, until: Instant) -> ServiceKey {
    return Member.issue_key(member.id, until)
}
"#,
    );
    assert!(
        found
            .iter()
            .any(|p| p.contains("semantic.unknown_callee: `Member` has no operation `issue_key`")),
        "{found:?}"
    );
}

#[test]
fn a_service_key_needs_a_service_to_sign_in() {
    let text = r#"principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    service_key: {
        cookie: "x"
    }
    api_token: {}
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}
"#;
    let found = support::problems(Layout::Loose, &[("main.jadpo", text)]);
    assert!(
        found
            .iter()
            .any(|p| p
                .contains("semantic.auth_setting_unknown: `cookie` isn't a `service_key` setting")),
        "{found:?}"
    );
    let text = text.replace("        cookie: \"x\"\n", "");
    let found = support::problems(Layout::Loose, &[("main.jadpo", text.as_str())]);
    assert!(
        found.iter().any(|p| p.contains(
            "semantic.auth_principal_required: `service_key:` signs in services, but no `service:` is declared"
        )),
        "{found:?}"
    );
}

#[test]
fn a_route_test_signs_in_as_a_service_only_with_service_keys() {
    let code = r#"type RobotView = Output {
    name: Handle
}

type RobotNotFound = Failure {
    kind: FailureKind.not_found
}

query whoami(robot: Principal.service) fails RobotNotFound -> RobotView {
    from: Robot
    where: Robot.id == robot.id
    missing: RobotNotFound
}

routes Robots {
    prefix: /robots

    GET /me {
        output: RobotView
        run: whoami(current_principal.service)
    }
}

test "a robot reads itself" {
    var robot: Robot = Robot.create {
        set: {
            name: "r2"
        }
    }
    var response: Response = request {
        method: HttpMethod.get
        path: "/robots/me"
        as: Principal.service(robot)
    }
    assert response.status == 200
}
"#;
    let found = with("authentication {\n    api_token: {}\n}", code);
    assert!(
        found
            .iter()
            .any(|p| p.contains("semantic.request_invalid: Nothing signs a service in for `as:`")),
        "{found:?}"
    );
    let found = with(BOTH, code);
    assert!(
        !found.iter().any(|p| p.contains("semantic.request_invalid")),
        "{found:?}"
    );
}
