//! Sign-in providers (area 8, "Providers"; chunk A7b): `Driver.oidc`
//! connections named by `sign_in: { providers: [...] }`, the routes they
//! generate, `SignInMethod`'s value for each, and `test_providers`.

use jadpo_db::Layout;

use crate::support;

const CONNECTIONS: &str = r#"application {
    display_name: "Lending desk"
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        google: {
            driver: Driver.oidc
            issuer: config.google_issuer
            client_id: config.google_client_id
            credential: config.google_client_secret
        }
        microsoft: {
            driver: Driver.oidc
            issuer: config.microsoft_issuer
            client_id: config.microsoft_client_id
            credential: config.microsoft_client_secret
        }
    }
}
config {
    database_url: PostgresUrl
    site: Url
    google_issuer: Url
    google_client_id: Text
    google_client_secret: Text {
        secret: true
    }
    microsoft_issuer: Url
    microsoft_client_id: Text
    microsoft_client_secret: Text {
        secret: true
    }
}
principal {
    user: {
        subject: Librarian.email
    }
}
"#;

const AUTHENTICATION: &str = r#"authentication {
    browser_session: {
        cookie: "desk_session"
        origin: config.site
    }
    sign_in: {
        providers: [google, microsoft]
    }
    on_first_sign_in: link_staff
}
"#;

const CODE: &str = r#"entity Librarian {
    id: Uuid
    email: Email {
        unique: true
    }
    name: Text

    policy: {
        create: [Access.system]
        read: [id]
    }
}

type NotOnTheStaff = Failure {
    kind: FailureKind.not_found
}

type AlreadyOnTheStaff = Failure {
    kind: FailureKind.conflict
}

/// Takes on someone Google vouches for by a verified address.
action link_staff(first: FirstSignIn) fails NotOnTheStaff, AlreadyOnTheStaff -> Librarian.id {
    if (first.method != SignInMethod.google or not first.address_verified) {
        reject NotOnTheStaff {}
    }
    match first.address {
        none -> {
            reject NotOnTheStaff {}
        }
        some(address) -> {
            var librarian: Librarian = attempt Librarian.create {
                set: {
                    email: address
                    name: "New starter"
                }
                conflict: {
                    Librarian.email: AlreadyOnTheStaff
                }
            }
            return librarian.id
        }
    }
}
"#;

fn source(connections: &str, authentication: &str, code: &str) -> String {
    format!("{connections}\n{authentication}\n{code}")
}

fn problems(text: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("main.jadpo", text)])
}

fn has(found: &[String], rule: &str, words: &str) -> bool {
    found
        .iter()
        .any(|problem| problem.contains(&format!("{rule}:")) && problem.contains(words))
}

#[test]
fn google_and_microsoft_sign_in_through_their_connections() {
    let text = source(CONNECTIONS, AUTHENTICATION, CODE);
    let found = problems(&text);
    assert_eq!(found, Vec::<String>::new(), "{found:?}");
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", text.as_str())]);
    // Each provider's two routes, in the API that takes browser sessions.
    let routes: Vec<String> = jadpo_check::routes::generated::generating(&db, project)
        .values()
        .flatten()
        .flat_map(jadpo_check::routes::generated::Generating::routes)
        .filter(|route| route.generated == "provider")
        .map(|route| route.label())
        .collect();
    assert_eq!(
        routes,
        [
            "POST /auth/google/start",
            "GET /auth/google/callback",
            "POST /auth/microsoft/start",
            "GET /auth/microsoft/callback",
        ]
    );
    // The links a first sign-in makes are kept beside the users.
    let schema = jadpo_check::data::schema(&db, project);
    assert!(
        schema.stores["main"]
            .tables
            .contains_key("jadpo_sign_in_links")
    );
}

#[test]
fn a_provider_the_app_doesnt_declare_is_no_sign_in_method() {
    let code = CODE.replace("SignInMethod.google", "SignInMethod.github");
    let found = problems(&source(CONNECTIONS, AUTHENTICATION, &code));
    assert!(
        has(&found, "type.unknown_enum_variant", "github"),
        "{found:?}"
    );
}

#[test]
fn providers_need_the_first_sign_in_action_and_a_browser_sessions_origin() {
    let without = AUTHENTICATION.replace("    on_first_sign_in: link_staff\n", "");
    let found = problems(&source(CONNECTIONS, &without, ""));
    assert!(
        has(&found, "semantic.auth_setting_required", "on_first_sign_in"),
        "{found:?}"
    );
    let no_origin = AUTHENTICATION.replace("        origin: config.site\n", "");
    let found = problems(&source(CONNECTIONS, &no_origin, CODE));
    assert!(
        has(&found, "semantic.auth_setting_required", "origin"),
        "{found:?}"
    );
}

#[test]
fn a_provider_is_a_connection_on_the_oidc_driver() {
    for (named, words) in [("main", "main"), ("nowhere", "nowhere")] {
        let authentication =
            AUTHENTICATION.replace("[google, microsoft]", &format!("[google, {named}]"));
        let found = problems(&source(CONNECTIONS, &authentication, CODE));
        assert!(
            has(&found, "semantic.auth_setting_invalid", words),
            "{named}: {found:?}"
        );
    }
}

#[test]
fn an_oidc_connection_reads_its_settings_from_configuration() {
    let no_client = CONNECTIONS.replace("            client_id: config.google_client_id\n", "");
    let found = problems(&source(&no_client, AUTHENTICATION, CODE));
    assert!(
        has(&found, "connection.key_missing", "client_id"),
        "{found:?}"
    );
    let plain = CONNECTIONS.replace(
        "    google_client_secret: Text {\n        secret: true\n    }\n",
        "    google_client_secret: Text\n",
    );
    let found = problems(&source(&plain, AUTHENTICATION, CODE));
    assert!(
        has(&found, "connection.value_invalid", "secret"),
        "{found:?}"
    );
    let written = CONNECTIONS.replace(
        "issuer: config.google_issuer",
        "issuer: \"https://accounts.google.com\"",
    );
    let found = problems(&source(&written, AUTHENTICATION, CODE));
    assert!(
        has(&found, "connection.value_invalid", "issuer"),
        "{found:?}"
    );
    let unknown = CONNECTIONS.replace(
        "            client_id: config.google_client_id\n",
        "            client_id: config.google_client_id\n            scopes: config.google_client_id\n",
    );
    let found = problems(&source(&unknown, AUTHENTICATION, CODE));
    assert!(has(&found, "connection.key_invalid", "scopes"), "{found:?}");
}

#[test]
fn only_a_test_signs_in_through_a_test_provider() {
    let code = format!(
        "{CODE}\naction pretend() {{\n    var answer: Response = test_providers.sign_in(SignInMethod.google, TestAccount {{\n        subject: \"1\"\n        email: \"ann@example.com\"\n        email_verified: true\n        name: none\n    }})\n}}\n"
    );
    let found = problems(&source(CONNECTIONS, AUTHENTICATION, &code));
    assert!(
        has(&found, "semantic.request_invalid", "test_providers"),
        "{found:?}"
    );
    assert!(has(&found, "type.not_record", "TestAccount"), "{found:?}");
}

/// A test that signs in only through `test_providers.sign_in` goes
/// through the provider's generated routes, so it calls them, and is a
/// route test with a database, not a pure test (chunk A6, found by the
/// lending desk example).
#[test]
fn a_test_signing_in_through_a_provider_calls_its_routes() {
    let code = format!(
        "{CODE}\ntest \"a stranger is refused\" {{\n    var answer: Response = test_providers.sign_in(\n        SignInMethod.google,\n        TestAccount {{\n            subject: \"1\"\n            email: \"ann@example.com\"\n            email_verified: true\n            name: none\n        }}\n    )\n    assert answer.code == \"sign_in_refused\"\n}}\n"
    );
    let text = source(CONNECTIONS, AUTHENTICATION, &code);
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &text)]);
    let graph = jadpo_check::graph::graph(&db, project);
    let test = graph
        .ids()
        .find(|id| graph.node(*id).kind == jadpo_graph::Kind::Test)
        .expect("the test");
    let called: Vec<String> = graph
        .outgoing(test, jadpo_graph::EdgeKind::Calls)
        .filter(|callee| graph.node(*callee).kind == jadpo_graph::Kind::GeneratedRoute)
        .map(|callee| graph.node(callee).name.clone())
        .collect();
    assert!(
        called.iter().any(|name| name == "POST /auth/google/start")
            && called
                .iter()
                .any(|name| name == "GET /auth/google/callback"),
        "{called:?}"
    );
}

/// GitHub's connection, beside Google's (chunk N1).
fn with_github(connections: &str) -> String {
    connections
        .replace(
            "        microsoft: {\n            driver: Driver.oidc\n            issuer: config.microsoft_issuer\n            client_id: config.microsoft_client_id\n            credential: config.microsoft_client_secret\n        }\n",
            "        github: {\n            driver: Driver.github\n            client_id: config.github_client_id\n            credential: config.github_client_secret\n        }\n",
        )
        .replace(
            "    microsoft_issuer: Url\n    microsoft_client_id: Text\n    microsoft_client_secret: Text {\n        secret: true\n    }\n",
            "    github_client_id: Text\n    github_client_secret: Text {\n        secret: true\n    }\n",
        )
}

#[test]
fn github_signs_in_through_its_own_driver() {
    let connections = with_github(CONNECTIONS);
    let authentication = AUTHENTICATION.replace("[google, microsoft]", "[google, github]");
    let code = CODE.replace(
        "first.method != SignInMethod.google",
        "first.method != SignInMethod.github",
    );
    let text = source(&connections, &authentication, &code);
    let found = problems(&text);
    assert_eq!(found, Vec::<String>::new(), "{found:?}");
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", text.as_str())]);
    let routes: Vec<String> = jadpo_check::routes::generated::generating(&db, project)
        .values()
        .flatten()
        .flat_map(jadpo_check::routes::generated::Generating::routes)
        .filter(|route| route.generated == "provider")
        .map(|route| route.label())
        .collect();
    assert_eq!(
        routes,
        [
            "POST /auth/google/start",
            "GET /auth/google/callback",
            "POST /auth/github/start",
            "GET /auth/github/callback",
        ]
    );
    let identity = jadpo_check::identity::identity(&db, project);
    let github = identity
        .providers
        .iter()
        .find(|provider| provider.name == "github")
        .expect("GitHub");
    assert_eq!(github.driver, jadpo_std::http::ProviderDriver::Github);
    assert_eq!(github.issuer, None);
    // GitHub's own hosts are in the network report, and nothing else of
    // its.
    let hosts: Vec<String> = jadpo_check::connections::hosts(&db, project, false)
        .into_iter()
        .filter(|row| row.connection == "github")
        .filter_map(|row| row.address)
        .collect();
    assert_eq!(hosts, ["https://api.github.com", "https://github.com"]);
}

#[test]
fn a_github_connection_takes_its_client_id_and_secret_alone() {
    let connections = with_github(CONNECTIONS);
    let authentication = AUTHENTICATION.replace("[google, microsoft]", "[google, github]");
    let with_issuer = connections.replace(
        "            driver: Driver.github\n",
        "            driver: Driver.github\n            issuer: config.google_issuer\n",
    );
    let found = problems(&source(&with_issuer, &authentication, CODE));
    assert!(has(&found, "connection.key_invalid", "issuer"), "{found:?}");
    let with_company = connections.replace(
        "            driver: Driver.github\n",
        "            driver: Driver.github\n            organisation: config.github_client_id\n",
    );
    let found = problems(&source(&with_company, &authentication, CODE));
    assert!(
        has(&found, "connection.key_invalid", "organisation"),
        "{found:?}"
    );
    let no_secret =
        connections.replace("            credential: config.github_client_secret\n", "");
    let found = problems(&source(&no_secret, &authentication, CODE));
    assert!(
        has(&found, "connection.key_missing", "credential"),
        "{found:?}"
    );
    // Neither is it reported as an unknown provider beside its own error.
    assert!(
        !has(&found, "semantic.auth_setting_invalid", "github"),
        "{found:?}"
    );
    let plain = connections.replace(
        "    github_client_secret: Text {\n        secret: true\n    }\n",
        "    github_client_secret: Text\n",
    );
    let found = problems(&source(&plain, &authentication, CODE));
    assert!(
        has(&found, "connection.value_invalid", "secret"),
        "{found:?}"
    );
}

#[test]
fn sign_in_with_apple_is_not_built_yet() {
    let connections = with_github(CONNECTIONS).replace("Driver.github", "Driver.apple");
    let authentication = AUTHENTICATION.replace("[google, microsoft]", "[google, github]");
    let found = problems(&source(&connections, &authentication, CODE));
    assert!(has(&found, "toolchain.not_built_yet", "Apple"), "{found:?}");
    assert!(
        !has(&found, "semantic.auth_setting_invalid", "github"),
        "{found:?}"
    );
}

/// GitHub never says how a person signed in, so a role that needs a
/// second factor through it can't be held (chunk N1).
#[test]
fn a_second_factor_through_github_can_never_be_met() {
    let connections = with_github(CONNECTIONS);
    let authentication = AUTHENTICATION.replace("[google, microsoft]", "[google, github]");
    let text = source(&connections, &authentication, CODE);
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", text.as_str())]);
    let ways = jadpo_check::policy::requires::Ways::of(&db, project);
    assert!(ways.required.contains("github"));
    assert!(!ways.second_factor.contains("github"));
    assert!(ways.second_factor.contains("google"));
}
