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
    assert list.count(keys) == 0
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

/// Like [`with`], with entities whose policy lets each record be read by
/// itself, so a route that lists a holder's tokens or keys has the grant it
/// needs (area 9, question 8).
fn listing(authentication: &str, code: &str) -> Vec<String> {
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

    policy: {{
        read: [id]
    }}
}}

entity Robot {{
    id: Uuid
    name: Handle {{
        unique: true
    }}

    policy: {{
        read: [id]
    }}
}}

{code}"#
    );
    support::problems(Layout::Loose, &[("main.jadpo", text.as_str())])
}

const DEVICES: &str = r#"routes Devices {
    prefix: /devices

    GET / {
        output: List<IssuedToken>
        run: sessions.tokens(current_principal.user.id)
    }

    GET /{member_id: Member.id} {
        output: List<IssuedToken>
        run: sessions.tokens(member_id)
    }
}

routes Robots {
    prefix: /robots

    GET /keys {
        output: List<IssuedKey>
        run: Robot.keys(current_principal.service.id)
    }
}
"#;

#[test]
fn a_get_route_lists_tokens_and_keys() {
    let found = listing(BOTH, DEVICES);
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn an_output_holds_issued_tokens_and_keys() {
    let found = listing(
        BOTH,
        r#"type Credentials = Output {
    tokens: List<IssuedToken>
    keys: List<IssuedKey>
}

action credentials_of(member: Principal.user, robot: Robot) -> Credentials {
    return Credentials {
        tokens: sessions.tokens(member.id)
        keys: Robot.keys(robot.id)
    }
}
"#,
    );
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn only_a_get_route_runs_them() {
    let found = listing(
        BOTH,
        r#"routes Devices {
    prefix: /devices

    POST / {
        output: List<IssuedToken>
        run: sessions.tokens(current_principal.user.id)
    }
}
"#,
    );
    assert!(
        found
            .iter()
            .any(|p| p.contains("route.run_kind: `POST /devices` runs query `sessions.tokens`")),
        "{found:?}"
    );
}

#[test]
fn a_route_runs_only_what_reads() {
    let found = listing(
        BOTH,
        r#"routes Devices {
    prefix: /devices

    GET / {
        output: List<IssuedToken>
        run: sessions.issue_token(current_principal.user.id)
    }
}
"#,
    );
    assert!(
        found
            .iter()
            .any(|p| p.contains("route.run_invalid: `sessions.issue_token` writes")),
        "{found:?}"
    );
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn a_listing_route_needs_its_method_and_its_holders_id() {
    let found = listing(
        "authentication {\n    service_key: {}\n}",
        r#"routes Devices {
    prefix: /devices

    GET / {
        output: List<IssuedToken>
        run: sessions.tokens(current_principal.service.id)
    }
}
"#,
    );
    assert!(
        found
            .iter()
            .any(|p| p.contains("type.auth_sessions_context: `sessions.tokens` needs API tokens")),
        "{found:?}"
    );
    let found = listing(
        BOTH,
        r#"routes Devices {
    prefix: /devices

    GET / {
        output: List<IssuedToken>
        run: sessions.tokens(current_principal.service.id)
    }
}
"#,
    );
    assert!(
        found.iter().any(|p| p.contains("type.mismatch")),
        "{found:?}"
    );
}

#[test]
fn a_listing_route_sends_what_it_lists() {
    let found = listing(
        BOTH,
        r#"routes Devices {
    prefix: /devices

    GET / {
        output: List<IssuedKey>
        run: sessions.tokens(current_principal.user.id)
    }
}
"#,
    );
    assert!(
        found.iter().any(|p| p.contains(
            "route.output_mismatch: `GET /devices` sends `List<IssuedKey>`, but `sessions.tokens` returns `List<IssuedToken>`"
        )),
        "{found:?}"
    );
}

#[test]
fn listing_needs_read_on_the_holders_record() {
    // The holder grants nobody a read.
    let text = DEVICES;
    let found = support::problems(
        Layout::Loose,
        &[(
            "main.jadpo",
            format!(
                r#"principal {{
    user: {{
        subject: Member.handle
    }}
}}

authentication {{
    api_token: {{}}
}}

type Handle = Text {{
    max_length: 40
}}

entity Member {{
    id: Uuid
    handle: Handle {{
        unique: true
    }}
}}

{}"#,
                text.split("routes Robots").next().unwrap_or_default()
            )
            .as_str(),
        )],
    );
    assert!(
        found.iter().any(|p| p.contains(
            "policy.effect_ungranted: `GET /devices` reads `Member` records, which nobody may do"
        )),
        "{found:?}"
    );
}

#[test]
fn a_route_anyone_may_call_lists_only_what_everyone_may_read() {
    let found = listing(
        BOTH,
        r#"routes Devices {
    prefix: /devices

    GET /{member_id: Member.id} {
        authentication: RouteAuthentication.none
        output: List<IssuedToken>
        run: sessions.tokens(member_id)
    }
}
"#,
    );
    assert!(
        found.iter().any(|p| p.contains(
            "policy.public_grant_missing: `GET /devices/{member_id}` lets anyone call it, but not everyone may read `Member`"
        )),
        "{found:?}"
    );
}

/// External JWTs carry an identity provider's subject, never its profile
/// or policy. Issuer and audience are deployment configuration (area 8).
fn external(settings: &str, configuration: &str) -> Vec<String> {
    with(
        &format!("authentication {{\n    external_token: {{\n{settings}\n    }}\n}}"),
        &format!("config {{\n{configuration}\n}}"),
    )
}

#[test]
fn an_external_token_reads_its_issuer_and_audience_from_configuration() {
    let found = external(
        "        issuer: config.issuer\n        audience: config.audience",
        "    issuer: Url\n    audience: Text",
    );
    assert_eq!(found, Vec::<String>::new(), "{found:?}");
}

#[test]
fn an_external_token_requires_both_settings_and_their_declared_types() {
    for (settings, configuration, rule) in [
        (
            "issuer: config.issuer",
            "issuer: Url",
            "semantic.auth_setting_required",
        ),
        (
            "audience: config.audience",
            "audience: Text",
            "semantic.auth_setting_required",
        ),
        (
            "issuer: config.issuer\naudience: config.audience",
            "issuer: Text\naudience: Text",
            "semantic.auth_setting_invalid",
        ),
        (
            "issuer: config.issuer\naudience: config.audience",
            "issuer: Url\naudience: Int",
            "semantic.auth_setting_invalid",
        ),
        (
            "issuer: config.issuer\naudience: config.audience\njwks_uri: config.issuer",
            "issuer: Url\naudience: Text",
            "semantic.auth_setting_unknown",
        ),
        (
            "issuer: \"https://login.example\"\naudience: config.audience",
            "audience: Text",
            "semantic.auth_setting_invalid",
        ),
    ] {
        let found = external(settings, configuration);
        assert!(
            found.iter().any(|p| p.contains(&format!(" {rule}:"))),
            "{settings}: {found:?}"
        );
        assert!(
            !found
                .iter()
                .any(|p| p.contains(" toolchain.not_built_yet:")),
            "{found:?}"
        );
    }
}

#[test]
fn external_provider_signatures_do_not_need_an_application_signing_key() {
    let found = with(
        "authentication {\n    external_token: { issuer: config.issuer audience: config.audience }\n}",
        "application { revocation_delay: 5m }\nconfig { issuer: Url audience: Text }",
    );
    assert_eq!(found, Vec::<String>::new(), "{found:?}");
}

#[test]
fn an_external_token_still_requires_the_application_user_principal() {
    let found = support::problems(
        Layout::Loose,
        &[(
            "main.jadpo",
            r#"config {
    issuer: Url
    audience: Text
}
authentication {
    external_token: {
        issuer: config.issuer
        audience: config.audience
    }
}
"#,
        )],
    );
    assert!(
        found
            .iter()
            .any(|p| p.contains("semantic.auth_principal_required:")),
        "{found:?}"
    );
}

#[test]
fn external_compound_subjects_are_explicitly_not_built() {
    let source = r#"principal { user: { subject: Member.identity } }
config { issuer: Url audience: Text }
authentication { external_token: { issuer: config.issuer audience: config.audience } }
entity Member {
    id: Uuid
    identity: Money { unique: true }
}
"#;
    let found = support::problems(Layout::Loose, &[("main.jadpo", source)]);
    assert!(
        found
            .iter()
            .any(|problem| problem.contains("toolchain.not_built_yet:")
                && problem.contains("subject")),
        "{found:?}"
    );
}
