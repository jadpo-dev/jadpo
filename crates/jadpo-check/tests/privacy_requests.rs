//! Area 15, "Data requests": what `privacy:`'s routes need, what a setting
//! refuses, the generated operations and who may call them, and the
//! tables they leave behind.

use jadpo_db::{Database, Layout};

use crate::support;

const APP: &str = "use accounts/entities { Member }

application {
    display_name: \"Notes\"
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
    privacy: {
        self_service: {}
    }
}

principal {
    user: {
        subject: Member.email
    }
}
";

const AUTH: &str = "authentication {
    browser_session: {
        cookie: \"notes_session\"
        origin: config.site
    }
    sign_in: {
        magic_link: {
            link: \"/sign-in/confirm\"
        }
    }
}
";

const CONFIG: &str = "config {
    site: Url {
        default: \"https://notes.example\"
    }
    mail_sender: Email {
        default: \"hello@notes.example\"
    }
    smtp_host: Hostname {
        default: \"localhost\"
    }
}
";

const MEMBER: &str = "/// Someone with notes.
entity Member {
    id: Uuid
    email: Email {
        unique: true
        personal: PersonalData.contact
    }

    person: true
    policy: {
        create: [Access.public]
        read: [id]
    }
}
";

const MEMBER_PATH: &str = "domains/accounts/entities/member.jadpo";

fn files<'a>(changed: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut out: Vec<(&str, &str)> = vec![
        ("app.jadpo", APP),
        ("authentication.jadpo", AUTH),
        ("config.jadpo", CONFIG),
        (MEMBER_PATH, MEMBER),
    ];
    for (path, text) in changed {
        out.retain(|(at, _)| at != path);
        if !text.is_empty() {
            out.push((path, text));
        }
    }
    out
}

/// The rules that fired, without the ones a test project written from
/// strings always trips.
fn rules(changed: &[(&str, &str)]) -> Vec<String> {
    support::rules(Layout::Project, &files(changed))
        .into_iter()
        .filter(|rule| !rule.starts_with("record.") && !rule.starts_with("migration."))
        .collect()
}

#[test]
fn a_project_with_self_service_checks() {
    assert_eq!(rules(&[]), Vec::<String>::new());
}

#[test]
fn the_routes_need_a_user_who_is_a_person() {
    let no_principal = APP.replace(
        "principal {\n    user: {\n        subject: Member.email\n    }\n}\n",
        "",
    );
    let found = rules(&[("app.jadpo", no_principal.as_str())]);
    assert!(
        found.contains(&"data.privacy_person_required".to_string()),
        "{found:?}"
    );
    let not_a_person = MEMBER.replace("    person: true\n", "");
    let found = rules(&[(MEMBER_PATH, not_a_person.as_str())]);
    assert!(
        found.contains(&"data.privacy_person_required".to_string()),
        "{found:?}"
    );
}

#[test]
fn a_person_who_cannot_be_found_by_address_is_refused() {
    let username = MEMBER.replace(
        "    email: Email {\n        unique: true\n        personal: PersonalData.contact\n    }\n",
        "    name: Text {\n        unique: true\n        personal: PersonalData.contact\n        max_length: 40\n    }\n",
    );
    let app = APP.replace("Member.email", "Member.name");
    let found = rules(&[
        ("app.jadpo", app.as_str()),
        (MEMBER_PATH, username.as_str()),
        (
            "authentication.jadpo",
            "authentication {\n    browser_session: {\n        cookie: \"notes_session\"\n        origin: config.site\n    }\n}\n",
        ),
    ]);
    assert!(
        found.contains(&"data.privacy_person_required".to_string()),
        "{found:?}"
    );
}

#[test]
fn self_service_needs_an_origin_a_connection_and_a_name() {
    let auth = "authentication {\n    api_token: {}\n    sign_in: {\n        magic_link: {\n            link: \"/sign-in/confirm\"\n        }\n    }\n}\n";
    let found = rules(&[("authentication.jadpo", auth)]);
    assert!(
        found.contains(&"semantic.auth_setting_required".to_string()),
        "{found:?}"
    );
    let no_mail = APP
        .replace(
            "    connections: {\n        mail: {\n            driver: Driver.smtp\n            host: config.smtp_host\n        }\n    }\n    email: {\n        connection: mail\n        from: config.mail_sender\n    }\n",
            "",
        );
    let found = rules(&[("app.jadpo", no_mail.as_str())]);
    assert!(
        found.contains(&"email.connection_required".to_string()),
        "{found:?}"
    );
    let no_name = APP.replace("    display_name: \"Notes\"\n", "");
    let found = rules(&[("app.jadpo", no_name.as_str())]);
    assert!(
        found.contains(&"semantic.auth_setting_required".to_string()),
        "{found:?}"
    );
}

#[test]
fn a_setting_it_does_not_hold_is_refused_and_what_comes_later_is_not_built_yet() {
    for (written, rule) in [
        ("export_link: \"/data\"", "data.privacy_setting_invalid"),
        ("path: \"privacy\"", "data.privacy_setting_invalid"),
        ("generate_routes: 1", "data.privacy_setting_invalid"),
        (
            "self_service: { link: \"no\" }",
            "data.privacy_setting_invalid",
        ),
        (
            "emails: { address_check: { email: x.Y } }",
            "toolchain.not_built_yet",
        ),
        ("emails: { export_ready: {} }", "toolchain.not_built_yet"),
        ("exports: {}", "toolchain.not_built_yet"),
    ] {
        let app = APP.replace(
            "    privacy: {\n",
            &format!("    privacy: {{\n        {written}\n"),
        );
        let found = rules(&[("app.jadpo", app.as_str())]);
        assert!(found.contains(&rule.to_string()), "{written}: {found:?}");
    }
}

#[test]
fn a_route_may_not_answer_where_the_runtime_does() {
    let routes = "routes Own {\n    prefix: /privacy\n\n    POST /export {\n        authentication: RouteAuthentication.none\n        status: SuccessStatus.no_content\n        run: nothing()\n    }\n}\n";
    let action = "action nothing() -> Unit {\n    return\n}\n";
    let found = rules(&[
        ("domains/accounts/routes/own.jadpo", routes),
        ("domains/accounts/actions/nothing.jadpo", action),
    ]);
    assert!(
        found.contains(&"route.path_overlap".to_string()),
        "{found:?}"
    );
}

#[test]
fn the_operations_need_their_routes_off_and_a_grant_for_staff() {
    let action = "/// A request of the application's own.\naction ask(address: Email, locale: Locale) -> Unit {\n    privacy.request(address, locale)\n}\n";
    let path = "domains/accounts/actions/ask.jadpo";
    let found = rules(&[(path, action)]);
    assert!(
        found.contains(&"semantic.auth_setting_required".to_string()),
        "{found:?}"
    );
    let off = APP.replace(
        "    privacy: {\n",
        "    privacy: {\n        generate_routes: false\n",
    );
    let found = rules(&[("app.jadpo", off.as_str()), (path, action)]);
    assert!(
        !found.contains(&"semantic.auth_setting_required".to_string()),
        "{found:?}"
    );
    let staff = "use accounts/entities { Member }\n\n/// Staff make a request.\naction make(member: Member.id) -> Unit {\n    var made: PrivacyRequestAccepted = privacy.create_request(member, PrivacyRequestKind.export, \"Passport\")\n    return\n}\n";
    let path = "domains/accounts/actions/make.jadpo";
    let found = rules(&[(path, staff)]);
    assert!(
        found.contains(&"policy.binding_invalid".to_string()),
        "{found:?}"
    );
}

#[test]
fn only_a_role_held_across_the_application_can_be_granted_it() {
    let role = "type StaffRole = Enum {\n    support\n}\n";
    let staff = "use accounts/entities { Member }\nuse accounts/types { StaffRole }\n\nentity StaffMember {\n    id: Uuid\n    member_id: Member.id {\n        on_delete: OnDelete.cascade\n    }\n    role: StaffRole\n\n    membership: {\n        scope: MembershipScope.application\n        member: member_id\n        role: role\n    }\n\n    policy: {\n        read: [member_id]\n    }\n}\n";
    let base = [
        ("domains/accounts/types/staff_role.jadpo", role),
        ("domains/accounts/entities/staff_member.jadpo", staff),
    ];
    let line = |who: &str| {
        format!(
            "use accounts/types {{ StaffRole }}\n\npolicy {{\n    actions: {{\n        privacy.create_request: [{who}]\n    }}\n}}\n"
        )
    };
    let granted = line("StaffRole.support");
    let mut changed = base.to_vec();
    changed.push(("policy.jadpo", granted.as_str()));
    assert_eq!(rules(&changed), Vec::<String>::new());
    let anyone = line("Access.authenticated");
    let mut changed = base.to_vec();
    changed.push(("policy.jadpo", anyone.as_str()));
    let found = rules(&changed);
    assert!(
        found.contains(&"policy.binding_invalid".to_string()),
        "{found:?}"
    );
    let no_privacy = "policy {\n    actions: {\n        privacy.create_request: [Access.authenticated]\n    }\n}\n";
    let app = APP.replace("    privacy: {\n        self_service: {}\n    }\n", "");
    let found = rules(&[("app.jadpo", app.as_str()), ("policy.jadpo", no_privacy)]);
    assert!(
        found.contains(&"policy.binding_invalid".to_string()),
        "{found:?}"
    );
}

#[test]
fn the_tables_are_in_the_people_store_while_requests_are_on() {
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files(&[]));
    let schema = jadpo_check::data::schema(&db, project);
    let main = schema.stores.get("main").expect("the main store");
    assert!(
        main.tables
            .contains_key(jadpo_std::system::PRIVACY_REQUESTS.table)
    );
    assert!(
        main.tables
            .contains_key(jadpo_std::system::PRIVACY_ADDRESS_CHECKS.table)
    );
}
