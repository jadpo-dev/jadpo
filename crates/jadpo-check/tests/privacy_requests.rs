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
        vault: {
            driver: Driver.disk
            path: \"/var/lib/notes/vault\"
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
    privacy: {
        erasure_log: {
            store: vault
        }
        exports: {
            store: vault
        }
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
            "        mail: {\n            driver: Driver.smtp\n            host: config.smtp_host\n        }\n",
            "",
        )
        .replace(
            "    email: {\n        connection: mail\n        from: config.mail_sender\n    }\n",
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
        ("emails: { everything: {} }", "data.privacy_setting_invalid"),
        (
            "emails: { export_ready: { template: x.Y } }",
            "data.privacy_setting_invalid",
        ),
        (
            "emails: { export_ready: { email: notes.Nope } }",
            "data.privacy_setting_invalid",
        ),
        (
            "emails: { export_ready: { send: notes.nope } }",
            "data.privacy_setting_invalid",
        ),
        (
            "emails: { export_ready: { email: notes.A  send: notes.b } }",
            "data.privacy_setting_invalid",
        ),
        (
            "erasures: { cooling_off: 2h }",
            "data.privacy_setting_invalid",
        ),
        ("erasures: { wait: 14d }", "data.privacy_setting_invalid"),
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
fn exports_are_kept_in_a_store_of_files_for_a_time_it_allows() {
    for (written, rule) in [
        ("store: mail", "data.privacy_setting_invalid"),
        ("keep: 1h", "data.privacy_setting_invalid"),
        ("keep: 40d", "data.privacy_setting_invalid"),
        (
            "store: vault\n link: \"no\"",
            "data.privacy_setting_invalid",
        ),
        ("store: vault\n colour: 1", "data.privacy_setting_invalid"),
    ] {
        let app = APP.replace(
            "        exports: {\n            store: vault\n        }\n",
            &format!("        exports: {{\n            {written}\n        }}\n"),
        );
        let found = rules(&[("app.jadpo", app.as_str())]);
        assert!(found.contains(&rule.to_string()), "{written}: {found:?}");
    }
    let good = APP.replace(
        "        exports: {\n            store: vault\n        }\n",
        "        exports: {\n            store: vault\n            keep: 3d\n            link: \"/data/download\"\n        }\n",
    );
    assert_eq!(rules(&[("app.jadpo", good.as_str())]), Vec::<String>::new());
}

#[test]
fn taking_requests_needs_a_log_and_somewhere_to_keep_exports() {
    let no_log = APP.replace(
        "        erasure_log: {\n            store: vault\n        }\n",
        "",
    );
    let found = rules(&[("app.jadpo", no_log.as_str())]);
    assert!(
        found.contains(&"data.erasure_log_required".to_string()),
        "{found:?}"
    );
    let no_exports = APP.replace(
        "        exports: {\n            store: vault\n        }\n",
        "",
    );
    let found = rules(&[("app.jadpo", no_exports.as_str())]);
    assert!(
        found.contains(&"data.privacy_setting_invalid".to_string()),
        "{found:?}"
    );
}

const OWN_EXPORT_EMAIL: &str = "domains/accounts/emails/export_email.jadpo";

#[test]
fn an_applications_own_email_takes_only_what_jadpo_fills() {
    let app = format!("use accounts\n{APP}");
    let app = app.replace(
        "        self_service: {}\n",
        "        self_service: {}\n        emails: {\n            export_ready: {\n                email: accounts.ExportEmail\n            }\n        }\n",
    );
    // A field of its own, which Jadpo can't fill.
    let wrong = "/// A person's export.\ntype ExportEmail = EmailMessage {\n    pick: [PrivacyExportEmail.download_link]\n    extra: Text\n}\n";
    let found = rules(&[("app.jadpo", app.as_str()), (OWN_EXPORT_EMAIL, wrong)]);
    assert!(
        found.contains(&"data.privacy_setting_invalid".to_string()),
        "{found:?}"
    );
    // Not an email at all.
    let not_email = "/// A person's export.\ntype ExportEmail = Object {\n    a: Text\n}\n";
    let found = rules(&[("app.jadpo", app.as_str()), (OWN_EXPORT_EMAIL, not_email)]);
    assert!(
        found.contains(&"data.privacy_setting_invalid".to_string()),
        "{found:?}"
    );
}

#[test]
fn an_applications_own_sending_takes_its_request_and_gives_nothing() {
    let app = format!("use accounts\n{APP}");
    let app = app.replace(
        "        self_service: {}\n",
        "        self_service: {}\n        emails: {\n            erasure_confirmed: {\n                send: accounts.tell\n            }\n        }\n",
    );
    let path = "domains/accounts/actions/tell.jadpo";
    let wrong =
        "/// Tells someone.\naction tell(request: MagicLinkRequest) -> Unit {\n    return\n}\n";
    let found = rules(&[("app.jadpo", app.as_str()), (path, wrong)]);
    assert!(
        found.contains(&"data.privacy_setting_invalid".to_string()),
        "{found:?}"
    );
    let right = "/// Tells someone.\naction tell(request: PrivacyErasureConfirmedRequest) -> Unit {\n    return\n}\n";
    assert_eq!(
        rules(&[("app.jadpo", app.as_str()), (path, right)]),
        Vec::<String>::new()
    );
}

#[test]
fn no_mail_connection_needs_a_subscriber_for_the_export() {
    let no_mail = APP
        .replace(
            "        mail: {\n            driver: Driver.smtp\n            host: config.smtp_host\n        }\n",
            "",
        )
        .replace(
            "    email: {\n        connection: mail\n        from: config.mail_sender\n    }\n",
            "",
        )
        .replace("        self_service: {}\n", "");
    let auth = "authentication {\n    browser_session: {\n        cookie: \"notes_session\"\n        origin: config.site\n    }\n}\n";
    let found = rules(&[
        ("app.jadpo", no_mail.as_str()),
        ("authentication.jadpo", auth),
    ]);
    assert!(
        found.contains(&"email.connection_required".to_string()),
        "{found:?}"
    );
    let action = "/// Passes a link on.\naction pass_on(event: PrivacyExportReady) -> Unit {\n    return\n}\n";
    let subscriber = "use accounts/actions { pass_on }\n\n/// Passes each link on.\nsubscriber pass_links_on {\n    on: PrivacyExportReady\n    run: pass_on(event)\n}\n";
    assert_eq!(
        rules(&[
            ("app.jadpo", no_mail.as_str()),
            ("authentication.jadpo", auth),
            ("domains/accounts/actions/pass_on.jadpo", action),
            (
                "domains/accounts/subscribers/pass_links_on.jadpo",
                subscriber
            ),
        ]),
        Vec::<String>::new()
    );
}

const FINE: &str = "use accounts/entities { Member }\n\n/// A fine.\nentity Fine {\n    id: Uuid\n    member_id: Member.id? {\n        on_delete: OnDelete.restricted\n        erase: Erasure.retained\n    }\n    issued_at: Instant {\n        generated: Generated.on_create\n    }\n\n    policy: {\n        read: [Access.public]\n    }\n\n    retention: {\n        after: config.fines_kept\n        since: issued_at\n    }\n}\n";

#[test]
fn a_record_kept_by_law_needs_its_wording() {
    let config = CONFIG.replace(
        "config {\n",
        "config {\n    fines_kept: Duration {\n        default: 2192d\n    }\n",
    );
    let locales = format!(
        "{APP}\nlocales {{\n    default: Locale.en_gb\n    supported: [Locale.en_gb]\n}}\n"
    );
    let path = "domains/accounts/entities/fine.jadpo";
    let found = rules(&[
        ("app.jadpo", locales.as_str()),
        ("config.jadpo", config.as_str()),
        (path, FINE),
        ("translations/en_gb.jadpo", "translations {\n}\n"),
    ]);
    assert!(
        found.iter().any(|rule| rule.starts_with("translation.")),
        "{found:?}"
    );
    let worded = rules(&[
        ("app.jadpo", locales.as_str()),
        ("config.jadpo", config.as_str()),
        (path, FINE),
        ("translations/en_gb.jadpo", "translations {\n}\n"),
        (
            "domains/accounts/translations/en_gb.jadpo",
            "translations {\n    fine: \"A fine\"\n}\n",
        ),
    ]);
    assert_eq!(worded, Vec::<String>::new());
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
    let staff = "use accounts/entities { Member }\n\n/// Staff make a request.\naction built(member: Member.id) -> Unit {\n    var made: PrivacyRequestAccepted = privacy.create_request(member, PrivacyRequestKind.export, \"Passport\")\n    return\n}\n";
    let path = "domains/accounts/actions/built.jadpo";
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
    let app = APP.replace(
        "    privacy: {\n        erasure_log: {\n            store: vault\n        }\n        exports: {\n            store: vault\n        }\n        self_service: {}\n    }\n",
        "",
    );
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
