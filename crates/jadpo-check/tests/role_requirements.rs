//! Roles that need a way in (area 9, "Roles that need a way in"; chunk
//! A10): a membership's `requires:`, saying for each role how its holder
//! must have signed in for it to count, and a sign-in provider's
//! `organisation:` (area 8, "Company accounts").

use jadpo_db::Layout;

use crate::support;

const APP: &str = r#"application {
    display_name: "Lending desk"
    apis: {
        main: {
            prefix: /v1
        }
        tokens: {
            prefix: /tokens
            authentication: [AuthenticationMethod.api_token]
        }
    }
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
            organisation: config.google_domain
        }
    }
}
config {
    database_url: PostgresUrl
    site: Url
    google_issuer: Url {
        default: "https://accounts.google.com"
    }
    google_client_id: Text
    google_client_secret: Text {
        secret: true
    }
    google_domain: Text
}
principal {
    user: {
        subject: Patron.email
    }
}
authentication {
    browser_session: {
        cookie: "desk_session"
        origin: config.site
    }
    api_token: {}
    sign_in: {
        providers: [google]
    }
    on_first_sign_in: meet
}
"#;

const CODE: &str = r#"type StaffRole = Enum {
    admin
    front_desk
}

entity Patron {
    id: Uuid
    email: Email {
        unique: true
    }

    policy: {
        create: [Access.system]
        read: [id, StaffRole.admin]
    }
}

entity StaffMember {
    id: Uuid
    patron_id: Patron.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        member: patron_id
        role: role
        requires: {
            StaffRole.admin: {
                sign_in: [SignInMethod.google]
                strength: AuthenticationStrength.multi_factor
            }
            StaffRole.front_desk: {
                sign_in: [SignInMethod.google]
            }
        }
    }

    policy: {
        create: [StaffRole.admin]
        read: [patron_id, StaffRole.admin]
    }
}

type NotKnown = Failure {
    kind: FailureKind.not_found
}

action meet(first: FirstSignIn) fails NotKnown -> Patron.id {
    reject NotKnown {}
}

type PatronCard = Output {
    pick: [Patron.id, Patron.email]
}

type AlreadyKnown = Failure {
    kind: FailureKind.conflict
}

action add_patron(email: Patron.email) fails AlreadyKnown -> Patron {
    return attempt Patron.create {
        set: {
            email: email
        }
        conflict: {
            Patron.email: AlreadyKnown
        }
    }
}

query patrons() -> List<PatronCard> {
    from: Patron
    order: [Patron.email asc]
    limit: 100
}

routes Desk {
    prefix: /v1/desk
    GET /patrons {
        output: List<PatronCard>
        run: patrons()
    }
}
"#;

fn problems(code: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("app.jadpo", &format!("{APP}\n{code}"))])
}

fn with(app: &str, code: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("app.jadpo", &format!("{app}\n{code}"))])
}

fn has(found: &[String], rule: &str, words: &str) -> bool {
    found
        .iter()
        .any(|problem| problem.contains(&format!("{rule}:")) && problem.contains(words))
}

fn requires(lines: &str) -> String {
    let start = CODE.find("        requires: {").expect("requires");
    let end = start + CODE[start..].find("\n        }\n").expect("its end") + 11;
    format!(
        "{}        requires: {{\n{lines}        }}\n{}",
        &CODE[..start],
        &CODE[end..]
    )
}

#[test]
fn each_role_says_how_it_must_be_signed_in_to() {
    let found = problems(CODE);
    assert_eq!(found, Vec::<String>::new(), "{found:?}");
    let db = jadpo_db::Database::default();
    let text = format!("{APP}\n{CODE}");
    let project = support::project(&db, Layout::Loose, &[("app.jadpo", &text)]);
    let access = jadpo_check::policy::access(&db, project);
    let staff = access.membership("StaffMember").expect("the membership");
    let admin = staff.requirement("admin").expect("admin needs a way in");
    assert_eq!(admin.sign_in.as_deref(), Some(&["google".to_string()][..]));
    assert!(admin.multi_factor);
    let front = staff.requirement("front_desk").expect("front desk too");
    assert!(!front.multi_factor);
    // The permissions report gives each role's requirement a row.
    let report = jadpo_check::policy::report::report(&db, project);
    assert!(
        report
            .markdown
            .contains("| `StaffRole.admin` | `google` | a second factor |"),
        "{}",
        report.markdown
    );
    assert!(
        report
            .markdown
            .contains("| `StaffRole.front_desk` | `google` | any |"),
        "{}",
        report.markdown
    );
}

#[test]
fn a_role_or_a_way_in_the_app_hasnt_is_unknown() {
    let code = requires(
        "            StaffRole.admin: { sign_in: [SignInMethod.github] }\n            StaffRole.front_desk: {}\n            StaffRole.porter: {}\n",
    );
    let found = problems(&code);
    assert!(
        has(&found, "type.unknown_enum_variant", "github"),
        "{found:?}"
    );
    assert!(
        has(&found, "type.unknown_enum_variant", "porter"),
        "{found:?}"
    );
}

#[test]
fn once_one_role_says_how_every_role_does() {
    let code = requires("            StaffRole.admin: { sign_in: [SignInMethod.google] }\n");
    let found = problems(&code);
    assert!(
        has(
            &found,
            "policy.requirements_incomplete",
            "`StaffRole.front_desk`"
        ),
        "{found:?}"
    );
    // `{}` says a role needs nothing.
    let code = requires(
        "            StaffRole.admin: { sign_in: [SignInMethod.google] }\n            StaffRole.front_desk: {}\n",
    );
    let found = problems(&code);
    assert_eq!(found, Vec::<String>::new(), "{found:?}");
}

#[test]
fn a_requirement_says_only_what_a_requirement_can() {
    for (lines, words) in [
        (
            "            StaffRole.admin: { sign_in: [] }\n            StaffRole.front_desk: {}\n",
            "lists no way in",
        ),
        (
            "            StaffRole.admin: { strength: AuthenticationStrength.primary }\n            StaffRole.front_desk: {}\n",
            "requires nothing",
        ),
        (
            "            StaffRole.admin: { device: SignInMethod.google }\n            StaffRole.front_desk: {}\n",
            "isn't something a role can require",
        ),
        (
            "            StaffRole.admin: {}\n            StaffRole.admin: {}\n            StaffRole.front_desk: {}\n",
            "given twice",
        ),
        (
            "            StaffRole.admin: { sign_in: [SignInMethod.magic_link] }\n            StaffRole.front_desk: {}\n",
            "can't require",
        ),
    ] {
        let found = problems(&requires(lines));
        assert!(
            has(&found, "policy.requirement_invalid", words),
            "{lines}: {found:?}"
        );
    }
}

#[test]
fn a_requirement_no_way_in_can_meet_is_refused() {
    let code = requires(
        "            StaffRole.admin: { sign_in: [SignInMethod.external_token] }\n            StaffRole.front_desk: {}\n",
    );
    let found = problems(&code);
    assert!(
        has(
            &found,
            "policy.requirement_unsatisfiable",
            "`StaffRole.admin`"
        ),
        "{found:?}"
    );
}

#[test]
fn a_route_whose_api_cant_carry_the_way_in_is_refused() {
    // On an API that takes only API tokens, no Google session ever
    // arrives, so the admin role, the one grant on patrons, can't count.
    let code = CODE.replace(
        "routes Desk {\n    prefix: /v1/desk",
        "routes Desk {\n    prefix: /tokens/desk\n    api: tokens",
    );
    let code = code.replace("read: [id, StaffRole.admin]", "read: [StaffRole.admin]");
    let found = problems(&code);
    assert!(
        has(
            &found,
            "policy.effect_ungranted",
            "no one signed in to API `tokens`"
        ),
        "{found:?}"
    );
    // On the browser API it can.
    let found = problems(&CODE.replace("read: [id, StaffRole.admin]", "read: [StaffRole.admin]"));
    assert_eq!(found, Vec::<String>::new(), "{found:?}");
}

#[test]
fn only_a_provider_that_names_the_company_keeps_to_it() {
    let other = APP.replace(
        "default: \"https://accounts.google.com\"",
        "default: \"https://id.example.com\"",
    );
    let found = with(&other, CODE);
    assert!(
        has(&found, "connection.key_invalid", "one company's accounts"),
        "{found:?}"
    );
    let microsoft = APP.replace(
        "default: \"https://accounts.google.com\"",
        "default: \"https://login.microsoftonline.com/7f1d3a5e-8c2b-4f6a-9d0e-2b4c6a8e0f13/v2.0\"",
    );
    let found = with(&microsoft, CODE);
    assert_eq!(found, Vec::<String>::new(), "{found:?}");
    let not_text = APP.replace("    google_domain: Text\n", "    google_domain: Url\n");
    let found = with(&not_text, CODE);
    assert!(
        has(&found, "connection.value_invalid", "organisation"),
        "{found:?}"
    );
}

#[test]
fn a_route_test_says_how_its_user_signed_in() {
    let test = r#"
test "an admin signed in through Google" {
    var ann: Patron = attempt add_patron("ann@desk.example")
    var answer: Response = request {
        method: HttpMethod.get
        path: "/v1/desk/patrons"
        as: Principal.user(ann)
        sign_in: SignInMethod.google
        strength: AuthenticationStrength.multi_factor
    }
    assert answer.status == 200
}
"#;
    let found = problems(&format!("{CODE}{test}"));
    assert_eq!(found, Vec::<String>::new(), "{found:?}");
    let unknown = test.replace("SignInMethod.google", "SignInMethod.github");
    let found = problems(&format!("{CODE}{unknown}"));
    assert!(
        has(&found, "type.unknown_enum_variant", "github"),
        "{found:?}"
    );
    let nobody = test.replace("        as: Principal.user(ann)\n", "");
    let found = problems(&format!("{CODE}{nobody}"));
    assert!(
        has(&found, "semantic.request_invalid", "signs no user in"),
        "{found:?}"
    );
}

#[test]
fn a_test_account_may_say_its_company_and_strength() {
    let test = r#"
test "a company account" {
    var answer: Response = test_providers.sign_in(
        SignInMethod.google,
        TestAccount {
            subject: "1"
            email: "ann@desk.example"
            email_verified: true
            name: none
            organisation: "desk.example"
            strength: AuthenticationStrength.multi_factor
        }
    )
    assert answer.code == none
}
"#;
    let found = problems(&format!("{CODE}{test}"));
    assert_eq!(found, Vec::<String>::new(), "{found:?}");
}

#[test]
fn a_route_test_says_each_way_in_once() {
    let test = r#"
test "an admin signed in through Google" {
    config: {
        google_domain: "desk.example"
    }

    var ann: Patron = attempt add_patron("ann@desk.example")
    var answer: Response = request {
        method: HttpMethod.get
        path: "/v1/desk/patrons"
        as: Principal.user(ann)
        sign_in: SignInMethod.google
        strength: AuthenticationStrength.multi_factor
    }
    assert answer.status == 200
}
"#;
    let found = problems(&format!("{CODE}{test}"));
    assert_eq!(found, Vec::<String>::new(), "{found:?}");
    for (line, rule, words) in [
        (
            "        sign_in: SignInMethod.google\n",
            "semantic.request_item_duplicate",
            "`sign_in:` is given twice",
        ),
        (
            "        strength: AuthenticationStrength.multi_factor\n",
            "semantic.request_item_duplicate",
            "`strength:` is given twice",
        ),
        (
            "        as: Principal.user(ann)\n",
            "semantic.request_item_duplicate",
            "`as:` is given twice",
        ),
        (
            "    config: {\n        google_domain: \"desk.example\"\n    }\n",
            "semantic.test_setting_duplicate",
            "`config:` is given twice",
        ),
    ] {
        let twice = test.replacen(line, &format!("{line}{line}"), 1);
        let found = problems(&format!("{CODE}{twice}"));
        assert!(has(&found, rule, words), "{line}: {found:?}");
        assert_eq!(found.len(), 1, "{line}: {found:?}");
    }
}

#[test]
fn a_test_account_gives_each_field_once() {
    let test = r#"
test "a company account" {
    var answer: Response = test_providers.sign_in(
        SignInMethod.google,
        TestAccount {
            subject: "1"
            subject: "2"
            email: "ann@desk.example"
            email_verified: true
            name: none
        }
    )
    assert answer.code == none
}
"#;
    let found = problems(&format!("{CODE}{test}"));
    assert!(
        has(&found, "type.duplicate_field", "`subject` is given twice"),
        "{found:?}"
    );
}
