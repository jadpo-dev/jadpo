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
fn the_apps_own_email_and_sending_are_never_together() {
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

/// A project in domains whose `magic_link:` holds `setting`, with the
/// application `application` and the domain's files `extra`.
fn project(application: &str, setting: &str, extra: &[(&str, &str)]) -> Vec<String> {
    let config = r#"config {
    browser_origin: Url
    mail_sender: Email
    smtp_host: Hostname
}
"#;
    let app = format!(
        r#"use accounts/entities {{ Member }}

application {{
{application}
}}

principal {{
    user: {{
        subject: Member.email
    }}
}}

locales {{
    default: Locale.en_gb
    supported: [Locale.en_gb]
}}
"#
    );
    let authentication = format!(
        r#"use accounts

authentication {{
    browser_session: {{
        cookie: "session"
        origin: config.browser_origin
    }}
    sign_in: {{
        magic_link: {{
            link: "/sign-in/confirm"
            {setting}
        }}
    }}
}}
"#
    );
    let member = r#"entity Member {
    id: Uuid
    email: Email {
        unique: true
    }
}
"#;
    let mut files: Vec<(&str, &str)> = vec![
        ("config.jadpo", config),
        ("app.jadpo", app.as_str()),
        ("authentication.jadpo", authentication.as_str()),
        ("domains/accounts/entities/member.jadpo", member),
    ];
    files.extend_from_slice(extra);
    // The routes' API record, which `jadpo records update` writes, isn't
    // what these tests are about.
    support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| !problem.contains("`api/main.jadpo_record` is missing"))
        .collect()
}

const SIGN_IN_EMAIL: (&str, &str) = (
    "domains/accounts/emails/sign_in_email.jadpo",
    r#"/// The sign-in email, in the application's own words.
type SignInEmail = EmailMessage {
    pick: [MagicLinkEmail.sign_in_link, MagicLinkEmail.code, MagicLinkEmail.expires_in]
}
"#,
);

const SIGN_IN_HTML: (&str, &str) = (
    "domains/accounts/emails/sign_in_email.html",
    "<p><a href=\"{{ sign_in_link }}\">Open Notes</a></p>\n<p>Or type {{ code }}. Both work for {{ expires_in }}.</p>\n",
);

const SIGN_IN_TEXT: (&str, &str) = (
    "domains/accounts/emails/sign_in_email.txt",
    "Open Notes: {{ sign_in_link }}\n\nOr type {{ code }}. Both work for {{ expires_in }}.\n",
);

const SIGN_IN_WORDS: (&str, &str) = (
    "domains/accounts/translations/en_gb.jadpo",
    "translations {\n    sign_in_email: \"Your Notes sign-in link\"\n}\n",
);

#[test]
fn the_apps_own_email_picks_from_magic_link_email() {
    let found = project(
        SENDS,
        "email: accounts.SignInEmail",
        &[SIGN_IN_EMAIL, SIGN_IN_HTML, SIGN_IN_TEXT, SIGN_IN_WORDS],
    );
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn the_apps_own_email_has_only_fields_jadpo_fills() {
    let written = (
        "domains/accounts/emails/sign_in_email.jadpo",
        r#"/// The sign-in email, with a greeting Jadpo can't know.
type SignInEmail = EmailMessage {
    pick: [MagicLinkEmail.sign_in_link, MagicLinkEmail.code]
    greeting: Text
}
"#,
    );
    let html = (
        "domains/accounts/emails/sign_in_email.html",
        "<p>{{ greeting }} <a href=\"{{ sign_in_link }}\">{{ code }}</a></p>\n",
    );
    let found = project(
        SENDS,
        "email: accounts.SignInEmail",
        &[written, html, SIGN_IN_WORDS],
    );
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `accounts.SignInEmail` has fields Jadpo can't fill"
        ),
        "{found:?}"
    );
    let found = project(SENDS, "email: accounts.Nothing", &[]);
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `accounts.Nothing` isn't an email"
        ),
        "{found:?}"
    );
    let object = (
        "domains/accounts/types/sign_in_email.jadpo",
        "type SignInEmail = Object {\n    pick: [MagicLinkEmail.code]\n}\n",
    );
    let found = project(SENDS, "email: accounts.SignInEmail", &[object]);
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `accounts.SignInEmail` isn't an email"
        ),
        "{found:?}"
    );
    let unknown = (
        "domains/accounts/emails/sign_in_email.jadpo",
        "type SignInEmail = EmailMessage {\n    pick: [MagicLinkEmail.token]\n}\n",
    );
    let found = project(
        SENDS,
        "email: accounts.SignInEmail",
        &[unknown, SIGN_IN_HTML, SIGN_IN_WORDS],
    );
    assert!(
        has(
            &found,
            "type.unknown_field: `MagicLinkEmail` has no field `token`"
        ),
        "{found:?}"
    );
}

#[test]
fn code_never_emits_the_sign_in_email() {
    let action = (
        "domains/accounts/actions/fake_sign_in.jadpo",
        r#"use accounts/emails { SignInEmail }

/// Sends a sign-in email of its own making.
action fake_sign_in(address: Email, link: MagicLinkEmail.sign_in_link, code: MagicLinkEmail.code, expires_in: MagicLinkEmail.expires_in) -> Unit {
    emit SignInEmail {
        to: address
        locale: Locale.en_gb
        sign_in_link: link
        code: code
        expires_in: expires_in
    }
}
"#,
    );
    let found = project(
        SENDS,
        "email: accounts.SignInEmail",
        &[SIGN_IN_EMAIL, SIGN_IN_HTML, SIGN_IN_WORDS, action],
    );
    assert!(
        has(
            &found,
            "event.emit_invalid: `SignInEmail` is sent only by signing in"
        ),
        "{found:?}"
    );
}

/// The application with its name and no email connection.
const NAMED: &str = "    display_name: \"Notes\"";

const SEND_ACTION: (&str, &str) = (
    "domains/accounts/actions/send_sign_in_link.jadpo",
    r#"/// Hands the sign-in link and code to the application's own sender.
action send_sign_in_link(request: MagicLinkRequest) -> Unit {
    assert request.expires_at > clock.now
    assert request.locale == Locale.en_gb
    assert request.address != "nobody@notes.example"
}
"#,
);

#[test]
fn the_apps_own_sending_needs_no_email_connection() {
    let found = project(NAMED, "send: accounts.send_sign_in_link", &[SEND_ACTION]);
    assert_eq!(found, Vec::<String>::new());
    let found = project(NAMED, "", &[]);
    assert!(
        has(
            &found,
            "email.connection_required: `magic_link:` sends email, but nothing sends email"
        ),
        "{found:?}"
    );
}

#[test]
fn the_apps_own_sending_takes_a_magic_link_request() {
    let takes_text = (
        "domains/accounts/actions/send_sign_in_link.jadpo",
        "action send_sign_in_link(address: Email) -> Unit {\n    assert address != \"nobody@notes.example\"\n}\n",
    );
    let found = project(NAMED, "send: accounts.send_sign_in_link", &[takes_text]);
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `accounts.send_sign_in_link` can't send the sign-in email"
        ),
        "{found:?}"
    );
    let found = project(NAMED, "send: accounts.nobody", &[]);
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `accounts.nobody` isn't an action"
        ),
        "{found:?}"
    );
    let found = project(NAMED, "send: \"somewhere\"", &[]);
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `send:` doesn't name an action"
        ),
        "{found:?}"
    );
}

#[test]
fn a_sign_in_code_is_never_kept() {
    let kept = (
        "domains/accounts/entities/sent_code.jadpo",
        "entity SentCode {\n    id: Uuid\n    code: SignInCode\n}\n",
    );
    let found = project(SENDS, "", &[kept]);
    assert!(
        has(
            &found,
            "type.auth_credential_stored: `SentCode.code` keeps a sign-in code"
        ),
        "{found:?}"
    );
    // Nor answered, put in a failure or an event, nor copied from
    // `MagicLinkEmail` into anything but an email.
    let shown = (
        "domains/accounts/types/code_view.jadpo",
        "type CodeView = Output {\n    code: SignInCode\n}\n",
    );
    let failed = (
        "domains/accounts/failures/wrong_code.jadpo",
        "type WrongCode = Failure {\n    kind: FailureKind.rejected\n    client: {\n        code: SignInCode\n    }\n}\n",
    );
    let event = (
        "domains/accounts/events/code_sent.jadpo",
        "type CodeSent = Event {\n    codes: List<SignInCode>\n}\n",
    );
    let copied = (
        "domains/accounts/events/link_sent.jadpo",
        "type LinkSent = Event {\n    pick: [MagicLinkEmail.sign_in_link]\n}\n",
    );
    let found = project(SENDS, "", &[shown, failed, event, copied]);
    for wanted in [
        "type.secret_exposed: `CodeView.code` holds a `SignInCode`",
        "type.secret_exposed: `WrongCode.code` holds a `SignInCode`",
        "type.secret_exposed: `CodeSent.codes` holds a `SignInCode`",
        "type.not_record: `LinkSent` can't copy from `MagicLinkEmail`",
    ] {
        assert!(has(&found, wanted), "{wanted}: {found:?}");
    }
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

// --- The application's own routes (area 8, "Writing the routes yourself";
// chunk 7.3b3) ---------------------------------------------------------------

/// The application's own sign-in routes, over magic links' operations.
const OWN_ROUTES: (&str, &str) = (
    "domains/accounts/routes/sign_in.jadpo",
    r#"use accounts/actions { ask_for_link, confirm_code, confirm_link }
use accounts/types { CodeGiven, LinkAsked, LinkGiven, SignedInView }

routes SignIn {
    prefix: /sign-in

    POST /ask {
        authentication: RouteAuthentication.none
        input: LinkAsked
        status: SuccessStatus.no_content
        run: ask_for_link(input)
    }

    POST /link {
        authentication: RouteAuthentication.none
        input: LinkGiven
        output: SignedInView
        run: confirm_link(input)
    }

    POST /code {
        authentication: RouteAuthentication.none
        input: CodeGiven
        output: SignedInView
        run: confirm_code(input)
    }
}
"#,
);

const OWN_TYPES: [(&str, &str); 5] = [
    (
        "domains/accounts/types/return_path.jadpo",
        "type ReturnPath = Text {\n    max_length: 2000\n}\n",
    ),
    (
        "domains/accounts/types/link_asked.jadpo",
        "use accounts/types { ReturnPath }\n\ntype LinkAsked = Input {\n    address: Email\n    locale: Locale\n    return_to: ReturnPath?\n}\n",
    ),
    (
        "domains/accounts/types/link_given.jadpo",
        "type LinkGiven = Input {\n    token: SignInToken\n}\n",
    ),
    (
        "domains/accounts/types/code_given.jadpo",
        "type CodeGiven = Input {\n    address: Email\n    code: SignInCode\n}\n",
    ),
    (
        "domains/accounts/types/signed_in_view.jadpo",
        "type SignedInView = Output {\n    return_to: Text\n}\n",
    ),
];

const OWN_ACTIONS: [(&str, &str); 3] = [
    (
        "domains/accounts/actions/ask_for_link.jadpo",
        r#"use accounts/types { LinkAsked }

/// Sends a link and a code to the address asked for.
action ask_for_link(input: LinkAsked) -> Unit {
    magic_links.request(input.address, input.locale, input.return_to)
}
"#,
    ),
    (
        "domains/accounts/actions/confirm_link.jadpo",
        r#"use accounts/types { LinkGiven, SignedInView }

/// Signs this browser in with a link's token.
action confirm_link(input: LinkGiven) -> SignedInView {
    var redeemed: MagicLinkRedeemed = magic_links.redeem(input.token)
    sessions.start(redeemed.user_id)
    return SignedInView {
        return_to: redeemed.return_to
    }
}
"#,
    ),
    (
        "domains/accounts/actions/confirm_code.jadpo",
        r#"use accounts/types { CodeGiven, SignedInView }

/// Signs this browser in with a code.
action confirm_code(input: CodeGiven) -> SignedInView {
    var redeemed: MagicLinkRedeemed = magic_links.redeem_code(input.address, input.code)
    sessions.start(redeemed.user_id)
    return SignedInView {
        return_to: redeemed.return_to
    }
}
"#,
    ),
];

const OWN_POLICY: (&str, &str) = (
    "policy.jadpo",
    r#"use accounts

policy {
    actions: {
        accounts.ask_for_link: [Access.public]
        accounts.confirm_link: [Access.public]
        accounts.confirm_code: [Access.public]
    }
}
"#,
);

fn own(setting: &str, extra: &[(&str, &str)]) -> Vec<String> {
    let mut files = vec![OWN_ROUTES, OWN_POLICY];
    files.extend_from_slice(&OWN_TYPES);
    files.extend_from_slice(&OWN_ACTIONS);
    files.extend_from_slice(extra);
    project(SENDS, setting, &files)
        .into_iter()
        .filter(|problem| !problem.contains("import.unused: `accounts` is imported"))
        .collect()
}

#[test]
fn the_apps_own_routes_call_magic_links_operations() {
    assert_eq!(own("generate_routes: false", &[]), Vec::<String>::new());
}

#[test]
fn without_generated_routes_their_paths_are_the_apps() {
    // The application's own route at the generated path overlaps nothing.
    let at_generated = (
        "domains/accounts/routes/auth.jadpo",
        r#"use accounts/actions { ask_for_link }
use accounts/types { LinkAsked }

routes Auth {
    prefix: /auth

    POST /magic-link {
        authentication: RouteAuthentication.none
        input: LinkAsked
        status: SuccessStatus.no_content
        run: ask_for_link(input)
    }
}
"#,
    );
    let found = own("generate_routes: false", &[at_generated]);
    assert!(!has(&found, "route.path_overlap"), "{found:?}");
}

#[test]
fn generate_routes_is_true_or_false_and_never_beside_path() {
    let found = magic("            generate_routes: 1");
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `generate_routes:` must be `true` or `false`"
        ),
        "{found:?}"
    );
    let found = magic("            generate_routes: false\n            path: \"/login\"");
    assert!(
        has(
            &found,
            "semantic.auth_setting_invalid: `magic_link:` has both `path:` and `generate_routes: false`"
        ),
        "{found:?}"
    );
    let found = magic("            routes: false");
    assert!(
        !has(&found, "semantic.auth_setting_unknown: `generate_routes`"),
        "{found:?}"
    );
}

#[test]
fn the_operations_need_generate_routes_false() {
    let found = own("", &[]);
    for wanted in [
        "semantic.auth_setting_required: `magic_links.request` needs `generate_routes: false`",
        "semantic.auth_setting_required: `magic_links.redeem` needs `generate_routes: false`",
        "semantic.auth_setting_required: `magic_links.redeem_code` needs `generate_routes: false`",
    ] {
        assert!(has(&found, wanted), "{wanted}: {found:?}");
    }
}

#[test]
fn the_operations_need_magic_links() {
    let text = r#"principal {
    user: {
        subject: Member.email
    }
}

authentication {
    browser_session: {
        cookie: "session"
    }
}

entity Member {
    id: Uuid
    email: Email {
        unique: true
    }
}

type AskForLink = Input {
    address: Email
}

action ask_for_link(input: AskForLink) -> Unit {
    magic_links.request(input.address, Locale.en_gb, none)
}
"#;
    let found = support::problems(Layout::Loose, &[("app.jadpo", text)]);
    assert!(
        has(
            &found,
            "semantic.auth_setting_required: `magic_links.request` needs magic links"
        ),
        "{found:?}"
    );
}

#[test]
fn only_an_action_calls_the_operations() {
    let function = (
        "domains/accounts/functions/redeem_here.jadpo",
        r#"function redeem_here(token: SignInToken) -> MagicLinkRedeemed {
    return magic_links.redeem(token)
}
"#,
    );
    let found = own("generate_routes: false", &[function]);
    assert!(
        has(
            &found,
            "type.auth_sessions_context: A function can't call `magic_links.redeem`"
        ),
        "{found:?}"
    );
}

#[test]
fn the_operations_take_their_arguments_in_order() {
    let short = (
        "domains/accounts/actions/ask_short.jadpo",
        r#"use accounts/types { LinkAsked }

action ask_short(input: LinkAsked) -> Unit {
    magic_links.request(input.address, input.locale)
}
"#,
    );
    let text = (
        "domains/accounts/actions/redeem_address.jadpo",
        r#"use accounts/types { LinkAsked }

action redeem_address(input: LinkAsked) -> Unit {
    var redeemed: MagicLinkRedeemed = magic_links.redeem(input.address)
}
"#,
    );
    let found = own("generate_routes: false", &[short, text]);
    for wanted in [
        "type.argument_count: `magic_links.request` takes the address, an `Email`, the email's `Locale` and the path to go back to, a `Text?`",
        "type.mismatch",
    ] {
        assert!(has(&found, wanted), "{wanted}: {found:?}");
    }
}

#[test]
fn a_route_runs_no_magic_link_operation_itself() {
    let route = (
        "domains/accounts/routes/direct.jadpo",
        r#"use accounts/types { LinkGiven }

routes Direct {
    prefix: /direct

    POST /link {
        authentication: RouteAuthentication.none
        input: LinkGiven
        status: SuccessStatus.no_content
        run: magic_links.redeem(input.token)
    }
}
"#,
    );
    let found = own("generate_routes: false", &[route]);
    assert!(
        has(
            &found,
            "route.run_invalid: `magic_links.redeem` writes, and a route doesn't run it"
        ),
        "{found:?}"
    );
}

#[test]
fn a_sign_in_token_is_never_kept_or_given_out() {
    let kept = (
        "domains/accounts/entities/kept_token.jadpo",
        "entity KeptToken {\n    id: Uuid\n    token: SignInToken\n}\n",
    );
    let shown = (
        "domains/accounts/types/token_view.jadpo",
        "type TokenView = Output {\n    token: SignInToken\n}\n",
    );
    let failed = (
        "domains/accounts/failures/wrong_token.jadpo",
        "type WrongToken = Failure {\n    kind: FailureKind.rejected\n    client: {\n        token: SignInToken\n    }\n}\n",
    );
    let event = (
        "domains/accounts/events/token_used.jadpo",
        "type TokenUsed = Event {\n    tokens: List<SignInToken>\n}\n",
    );
    let mailed = (
        "domains/accounts/emails/token_mail.jadpo",
        "type TokenMail = EmailMessage {\n    token: SignInToken\n}\n",
    );
    let found = own(
        "generate_routes: false",
        &[kept, shown, failed, event, mailed],
    );
    for wanted in [
        "type.auth_credential_stored: `KeptToken.token` keeps a sign-in link's token",
        "type.secret_exposed: `TokenView.token` holds a `SignInToken`",
        "type.secret_exposed: `WrongToken.token` holds a `SignInToken`",
        "type.secret_exposed: `TokenUsed.tokens` holds a `SignInToken`",
        "type.secret_exposed: `TokenMail.token` holds a `SignInToken`",
    ] {
        assert!(has(&found, wanted), "{wanted}: {found:?}");
    }
}

#[test]
fn only_the_runtime_makes_a_magic_link_redeemed() {
    let made = (
        "domains/accounts/actions/pretend.jadpo",
        r#"use accounts/entities { Member }
use accounts/types { LinkGiven }

action pretend(input: LinkGiven, member: Member) -> Unit {
    var redeemed: MagicLinkRedeemed = MagicLinkRedeemed {
        user_id: member.id
        return_to: "/"
    }
    sessions.start(redeemed.user_id)
}
"#,
    );
    let found = own("generate_routes: false", &[made]);
    assert!(
        has(
            &found,
            "type.not_record: `MagicLinkRedeemed` isn't built with `{ ... }`"
        ),
        "{found:?}"
    );
}

// --- the events every sign-in publishes (chunk 7.3b4) -------------------------

/// A subscriber of each sign-in event, passing the event or its fields.
const LISTENING: &str = r#"entity SignInNote {
    id: Uuid
    member_id: Member.id {
        on_delete: OnDelete.cascade
    }
}

entity WayIn {
    id: Uuid
    by_link: Bool
}

/// Notes who signed in.
action note_sign_in(member_id: Member.id) fails MemberNotFound -> SignInNote {
    return attempt SignInNote.create {
        set: {
            member_id: member_id
        }
    }
}

/// Notes how a person signed in.
action note_method(event: SignedIn) -> WayIn {
    return WayIn.create {
        set: {
            by_link: event.method == SignInMethod.magic_link and event.first_time
        }
    }
}

/// Notes a refusal.
action note_refusal(event: SignInRefused) -> WayIn {
    return WayIn.create {
        set: {
            by_link: event.reason == SignInRefusal.link_expired and event.user_id == none
        }
    }
}

/// Notes an address whose codes stopped.
action note_codes_disabled(event: SignInCodesDisabled) -> WayIn {
    return WayIn.create {
        set: {
            by_link: event.until > clock.now
        }
    }
}

subscriber note_sign_ins {
    on: SignedIn
    run: note_sign_in(event.user_id)
}

subscriber note_methods {
    on: SignedIn
    run: note_method(event)
}

subscriber note_refusals {
    on: SignInRefused
    run: note_refusal(event)
}

subscriber note_disabled_codes {
    on: SignInCodesDisabled
    run: note_codes_disabled(event)
}
"#;

#[test]
fn subscribers_listen_for_the_sign_in_events() {
    assert_eq!(with(SENDS, LINK, "", LISTENING), Vec::<String>::new());
}

#[test]
fn a_sign_in_events_fields_are_its_own() {
    let code = LISTENING.replace(
        "run: note_sign_in(event.user_id)",
        "run: note_sign_in(event.address)",
    );
    let found = with(SENDS, LINK, "", &code);
    assert!(
        has(
            &found,
            "type.unknown_field: `SignedIn` has no field `address`"
        ),
        "{found:?}"
    );
    // `user_id` is the user entity's `id`, so it goes where one goes.
    let code = LISTENING
        .replace(
            "run: note_disabled_codes(event)",
            "run: note_sign_in(event.user_id)",
        )
        .replace(
            "run: note_codes_disabled(event)",
            "run: note_sign_in(event.user_id)",
        );
    let found = with(SENDS, LINK, "", &code);
    assert!(has(&found, "type.mismatch"), "{found:?}");
}

#[test]
fn nothing_publishes_a_sign_in_event_without_a_magic_link() {
    let text = format!(
        r#"config {{
    browser_origin: Url
}}

application {{
    display_name: "Notes"
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

{LISTENING}"#
    );
    let found = support::problems(Layout::Loose, &[("app.jadpo", text.as_str())]);
    assert!(
        has(
            &found,
            "subscriber.event_invalid: Nothing publishes `SignedIn`"
        ),
        "{found:?}"
    );
}

#[test]
fn only_the_runtime_emits_a_sign_in_event() {
    let code = r#"test "a sign-in is announced" {
    emit SignInCodesDisabled {
        user_id: none
        until: clock.now
    }
}
"#;
    let found = with(SENDS, LINK, "", code);
    assert!(
        has(
            &found,
            "event.emit_invalid: `SignInCodesDisabled` is published only by signing in"
        ),
        "{found:?}"
    );
    let code = r#"action announce() -> Unit {
    var refused: SignInRefused = SignInRefused {
        method: SignInMethod.magic_link
        reason: SignInRefusal.code_invalid
        user_id: none
    }
}
"#;
    let found = with(SENDS, LINK, "", code);
    assert!(
        has(
            &found,
            "type.not_record: `SignInRefused` isn't built with `{ ... }`"
        ),
        "{found:?}"
    );
}

/// `on_first_sign_in:`'s action, and a test calling it directly.
const FIRST_TESTED: &str = r#"type SignUpClosed = Failure {
    kind: FailureKind.conflict
}

/// Signs a new member up, unless the address wasn't proved.
action create_member(first: FirstSignIn) fails SignUpClosed -> Member.id {
    if (not first.address_verified) {
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

test "an unproved address is refused" {
    assert create_member(FirstSignIn {
        method: SignInMethod.magic_link
        address: "ada@example.com"
        address_verified: false
        subject: none
        name: none
    }) fails SignUpClosed
}
"#;

#[test]
fn a_test_builds_a_first_sign_in_to_call_the_action() {
    assert_eq!(with(SENDS, LINK, FIRST, FIRST_TESTED), Vec::<String>::new());
    // Every field is given.
    let code = FIRST_TESTED.replace("        name: none\n", "");
    let found = with(SENDS, LINK, FIRST, &code);
    assert!(has(&found, "`name`"), "{found:?}");
}

#[test]
fn only_a_test_builds_a_first_sign_in() {
    let code = format!(
        "{FIRST_TESTED}\naction sign_up_ada() fails SignUpClosed -> Member.id {{\n    return attempt create_member(FirstSignIn {{\n        method: SignInMethod.magic_link\n        address: \"ada@example.com\"\n        address_verified: true\n        subject: none\n        name: none\n    }})\n}}\n"
    );
    let found = with(SENDS, LINK, FIRST, &code);
    assert!(
        has(
            &found,
            "type.not_record: `FirstSignIn` isn't built with `{ ... }`"
        ),
        "{found:?}"
    );
}

/// A record only anyone at all may create, which the system never may.
const SIGN_UP: &str = r#"entity SignUp {
    id: Uuid
    address: Email

    policy: {
        create: [GRANTS]
        read: [Access.system]
    }
}
"#;

/// `on_first_sign_in:`'s action runs as the system, which a grant
/// naming only `Access.public` never lets in (chunk 7.9b).
#[test]
fn on_first_sign_in_runs_as_the_system() {
    let code = |grants: &str| {
        format!(
            "{}\n/// Signs a new member up, noting the address.\naction create_member(first: FirstSignIn) -> Member.id {{\n    var address: Email = match first.address {{\n        some(given) -> given\n        none -> \"nobody@notes.example\"\n    }}\n    var noted: SignUp = SignUp.create {{\n        set: {{\n            address: address\n        }}\n    }}\n    var member: Member = Member.create {{\n        set: {{\n            email: address\n            nickname: \"new\"\n            backup: noted.address\n        }}\n    }}\n    return member.id\n}}\n",
            SIGN_UP.replace("GRANTS", grants)
        )
    };
    let found = with(SENDS, LINK, FIRST, &code("Access.public"));
    assert!(
        has(
            &found,
            "policy.system_grant_missing: `create_member` runs as the system, which only `Access.public` lets create `SignUp`"
        ),
        "{found:?}"
    );
    let found = with(SENDS, LINK, FIRST, &code("Access.public, Access.system"));
    assert!(!has(&found, "policy.system_grant_missing"), "{found:?}");
}

/// The magic-link `send:` action runs as the system too.
#[test]
fn the_apps_own_sending_runs_as_the_system() {
    let entity = |grants: &str| {
        (
            "domains/accounts/entities/sign_up.jadpo",
            SIGN_UP.replace("GRANTS", grants),
        )
    };
    let send = (
        "domains/accounts/actions/send_sign_in_link.jadpo",
        "use accounts/entities { SignUp }\n\n/// Notes who asked, then hands the link to the application's own sender.\naction send_sign_in_link(request: MagicLinkRequest) -> Unit {\n    var noted: SignUp = SignUp.create {\n        set: {\n            address: request.address\n        }\n    }\n}\n",
    );
    let public = entity("Access.public");
    let found = project(
        NAMED,
        "send: accounts.send_sign_in_link",
        &[send, (public.0, public.1.as_str())],
    );
    assert!(
        has(
            &found,
            "policy.system_grant_missing: `accounts.send_sign_in_link` runs as the system, which only `Access.public` lets create `SignUp`"
        ),
        "{found:?}"
    );
    let both = entity("Access.public, Access.system");
    let found = project(
        NAMED,
        "send: accounts.send_sign_in_link",
        &[send, (both.0, both.1.as_str())],
    );
    assert!(!has(&found, "policy.system_grant_missing"), "{found:?}");
}
