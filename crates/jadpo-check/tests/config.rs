//! Settings (area 10, chunk 5.1): what `config { }` may declare, the type
//! of `config.<name>` in code, the places a secret may go, and the
//! addresses connections open with.

use jadpo_db::Layout;

use crate::support;

/// Rules of the problems found in a loose project of one file.
fn rules(source: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("main.jadpo", source)])
        .into_iter()
        .map(|found| {
            let rule = found.split(' ').nth(1).unwrap_or_default();
            rule.trim_end_matches(':').to_string()
        })
        .collect()
}

fn found(source: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("main.jadpo", source)])
}

const CLEAN: &[&str] = &[];

// --- what a setting can be ----------------------------------------------------

#[test]
fn the_page_example_declares_cleanly() {
    assert_eq!(
        rules(
            r#"type SessionSigningKey = SigningKey {}

config {
    database_url: PostgresUrl
    session_signing_key: SessionSigningKey {
        secret: true
    }
    browser_origin: Url
    mail_sender: Email
    soft_delete_retention: Duration {
        default: 30d
    }
}
"#
        ),
        CLEAN
    );
}

#[test]
fn every_kind_of_one_piece_of_text_is_a_setting() {
    assert_eq!(
        rules(
            r#"type Verbosity = Enum {
    debug
    info
    warning
}

type Port = Int {
    min: 1
    max: 65535
}

config {
    name: Text {
        default: "app"
    }
    port: Port {
        default: 8080
    }
    ratio: Decimal {
        default: 1.5
    }
    verbose: Bool {
        default: false
    }
    node: Uuid
    contact: Email
    site: Url {
        default: "https://example.test"
    }
    proxy: IpAddress
    timeout: Duration {
        default: 5s
    }
    opened: Instant {
        default: "2026-01-15T12:00:00Z"
    }
    launch: CalendarDate
    level: Verbosity {
        default: Verbosity.info
    }
    business_zone: Zone {
        default: Zone.europe_london
    }
    language: Locale {
        default: Locale.en_gb
    }
}
"#
        ),
        CLEAN
    );
}

#[test]
fn lists_objects_and_optional_settings_are_not_one_piece_of_text() {
    for ty in [
        "List<Email>",
        "Set<Text>",
        "Map<Text, Text>",
        "Email?",
        "Bytes",
    ] {
        assert_eq!(
            rules(&format!("config {{\n    value: {ty}\n}}\n")),
            ["configuration.type_unsupported"],
            "{ty}"
        );
    }
    assert_eq!(
        rules("type Pair = Object {\n    a: Text\n    b: Text\n}\n\nconfig {\n    pair: Pair\n}\n"),
        ["configuration.type_unsupported"]
    );
    assert_eq!(
        rules(
            "type Shade = Enum {\n    plain\n    tinted: {\n        colour: Text\n    }\n}\n\nconfig {\n    shade: Shade\n}\n"
        ),
        ["configuration.type_unsupported"]
    );
}

#[test]
fn a_secret_is_text() {
    assert_eq!(
        rules("config {\n    limit: Int {\n        secret: true\n    }\n}\n"),
        ["configuration.type_unsupported"]
    );
}

#[test]
fn an_unknown_type_is_an_unknown_name() {
    assert_eq!(
        rules("config {\n    sender: Emial\n}\n"),
        ["semantic.unknown_name"]
    );
}

// --- names and variables --------------------------------------------------------

#[test]
fn the_variable_is_the_name_in_capitals_unless_binding_says_otherwise() {
    assert_eq!(
        rules(
            "config {\n    mail_sender: Email\n    reply_to: Email {\n        binding: \"REPLY_ADDRESS\"\n    }\n}\n"
        ),
        CLEAN
    );
    // A setting that takes another's derived name clashes with it.
    assert_eq!(
        rules(
            "config {\n    mail_sender: Email\n    reply_to: Email {\n        binding: \"MAIL_SENDER\"\n    }\n}\n"
        ),
        ["configuration.duplicate_binding"]
    );
}

#[test]
fn bad_bindings_are_each_their_own_error() {
    let with = |binding: &str| {
        rules(&format!(
            "config {{\n    mail_sender: Email {{\n        binding: {binding}\n    }}\n}}\n"
        ))
    };
    assert_eq!(with("\"\""), ["configuration.binding_empty"]);
    assert_eq!(with("\"MAIL-SENDER\""), ["configuration.binding_invalid"]);
    assert_eq!(with("\"mail_sender\""), ["configuration.binding_invalid"]);
    assert_eq!(with("\"1MAIL\""), ["configuration.binding_invalid"]);
    assert_eq!(with("42"), ["configuration.binding_invalid"]);
    assert_eq!(with("\"HEROKU_POSTGRESQL_URL\""), CLEAN);
}

#[test]
fn a_binding_fix_is_offered_as_an_edit() {
    let found =
        found("config {\n    mail_sender: Email {\n        binding: \"MAIL-SENDER\"\n    }\n}\n");
    assert!(
        found[0].contains("`MAIL-SENDER` can't be a variable name"),
        "{found:?}"
    );
}

#[test]
fn a_signing_key_also_takes_its_previous_variable() {
    assert_eq!(
        rules(
            "config {\n    session_signing_key: SigningKey\n    old: Text {\n        binding: \"SESSION_SIGNING_KEY_PREVIOUS\"\n    }\n}\n"
        ),
        ["configuration.duplicate_binding"]
    );
}

#[test]
fn options_are_checked() {
    let with = |options: &str| {
        rules(&format!(
            "config {{\n    mail_sender: Email {{\n{options}    }}\n}}\n"
        ))
    };
    assert_eq!(
        with("        required: true\n"),
        ["configuration.unknown_option"]
    );
    assert_eq!(
        with("        binding: \"MAIL_SENDER\"\n        binding: \"SENDER\"\n"),
        ["configuration.duplicate_option"]
    );
    assert_eq!(
        with("        secret: yes\n"),
        ["configuration.secret_invalid"]
    );
    assert_eq!(with("        secret: false\n"), CLEAN);
}

#[test]
fn a_setting_is_a_name_and_a_type() {
    assert_eq!(
        rules("config {\n    mail_sender\n}\n"),
        ["configuration.expected_field"]
    );
    assert_eq!(
        rules("config {\n    mail_sender:\n}\n")
            .first()
            .map(String::as_str),
        Some("syntax.expected_expression")
    );
}

#[test]
fn there_is_one_config() {
    assert_eq!(
        rules("config {\n    a: Text\n}\n\nconfig {\n    b: Text\n}\n"),
        ["configuration.multiple_declarations"]
    );
}

// --- defaults and test values -----------------------------------------------------

#[test]
fn a_default_is_a_literal_of_the_type() {
    let with = |ty: &str, default: &str| {
        rules(&format!(
            "config {{\n    value: {ty} {{\n        default: {default}\n    }}\n}}\n"
        ))
    };
    assert_eq!(with("Int", "25"), CLEAN);
    assert_eq!(with("Int", "\"many\""), ["configuration.default_type"]);
    assert_eq!(with("Int", "5s"), ["configuration.default_type"]);
    assert_eq!(with("Int", "none"), ["configuration.default_type"]);
    assert_eq!(
        with("Int", "other_value"),
        ["configuration.default_literal_required"]
    );
    assert_eq!(
        with("Int", "1 + 2"),
        ["configuration.default_literal_required"]
    );
    assert_eq!(with("Duration", "5s"), CLEAN);
    assert_eq!(with("Duration", "5"), ["configuration.default_type"]);
    assert_eq!(
        with("Email", "\"nobody\""),
        ["configuration.default_invalid"]
    );
    assert_eq!(
        with("Url", "\"postgres://localhost/app\""),
        ["configuration.default_invalid"]
    );
    assert_eq!(with("Url", "\"https://example.test\""), CLEAN);
}

#[test]
fn a_default_follows_the_rules_of_a_named_type() {
    let with = |default: &str| {
        rules(&format!(
            "type Port = Int {{\n    min: 1\n    max: 65535\n}}\n\nconfig {{\n    port: Port {{\n        default: {default}\n    }}\n}}\n"
        ))
    };
    assert_eq!(with("8080"), CLEAN);
    assert_eq!(with("0"), ["configuration.default_invalid"]);
    assert_eq!(with("70000"), ["configuration.default_invalid"]);
}

#[test]
fn an_enum_setting_takes_its_variants() {
    let with = |default: &str| {
        rules(&format!(
            "type Verbosity = Enum {{\n    debug\n    info\n}}\n\nconfig {{\n    level: Verbosity {{\n        default: {default}\n    }}\n}}\n"
        ))
    };
    assert_eq!(with("Verbosity.info"), CLEAN);
    assert_eq!(with("Verbosity.loud"), ["configuration.default_invalid"]);
    assert_eq!(with("Zone.utc"), ["configuration.default_type"]);
    assert_eq!(with("\"info\""), ["configuration.default_type"]);
}

#[test]
fn a_test_value_is_checked_like_a_default() {
    let with = |value: &str| {
        rules(&format!(
            "config {{\n    mail_sender: Email {{\n        test: {value}\n    }}\n}}\n"
        ))
    };
    assert_eq!(with("\"reminders@example.test\""), CLEAN);
    assert_eq!(with("\"nobody\""), ["configuration.default_invalid"]);
    assert_eq!(with("5"), ["configuration.default_type"]);
}

#[test]
fn a_secret_has_no_value_in_source() {
    for option in ["default", "test"] {
        assert_eq!(
            rules(&format!(
                "config {{\n    api_key: Text {{\n        secret: true\n        {option}: \"development-key\"\n    }}\n}}\n"
            )),
            ["configuration.secret_default"],
            "{option}"
        );
    }
    // A type that is always secret needs no flag to refuse one.
    assert_eq!(
        rules(
            "config {\n    database_url: PostgresUrl {\n        default: \"postgres://localhost/app\"\n    }\n}\n"
        ),
        ["configuration.secret_default"]
    );
}

// --- reading a setting --------------------------------------------------------------

#[test]
fn code_reads_a_setting_with_its_type() {
    assert_eq!(
        rules(
            r#"type PageLimit = Int {}

type Sender = Email {}

config {
    page_size: Int {
        default: 25
    }
    sender: Email {
        test: "reminders@example.test"
    }
}

function page_limit() -> PageLimit {
    return PageLimit(config.page_size)
}

function from_address() -> Sender {
    return Sender(config.sender)
}
"#
        ),
        CLEAN
    );
}

#[test]
fn functions_read_settings_because_a_setting_cannot_change() {
    assert_eq!(
        rules(
            "type Wait = Duration {}\n\nconfig {\n    wait: Duration {\n        default: 5s\n    }\n}\n\nfunction pause() -> Wait {\n    return Wait(config.wait)\n}\n"
        ),
        CLEAN
    );
}

#[test]
fn a_setting_has_its_declared_type_and_not_another() {
    assert_eq!(
        rules(
            "type Label = Text {}\n\nconfig {\n    page_size: Int {\n        default: 25\n    }\n}\n\nfunction label() -> Label {\n    return config.page_size\n}\n"
        ),
        ["type.mismatch"]
    );
}

#[test]
fn a_name_that_is_no_setting_says_which_could_be_meant() {
    let found = found(
        "type Sender = Email {}\n\nconfig {\n    mail_sender: Email\n}\n\nfunction from_address() -> Sender {\n    return Sender(config.mail_sendr)\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("configuration.field_unknown"));
    assert!(found[0].contains("`config.mail_sendr` isn't a setting"));
    // With no `config { }` at all, there are no settings to read.
    assert_eq!(
        rules(
            "type Sender = Email {}\n\nfunction from_address() -> Sender {\n    return Sender(config.mail_sender)\n}\n"
        ),
        ["configuration.field_unknown"]
    );
}

#[test]
fn a_setting_that_did_not_read_is_not_also_unknown() {
    // The mistake is reported where it is written, once.
    assert_eq!(
        rules(
            "type Sender = Email {}\n\nconfig {\n    mail_sender: Email {\n        binding: \"\"\n    }\n}\n\nfunction from_address() -> Sender {\n    return Sender(config.mail_sender)\n}\n"
        ),
        ["configuration.binding_empty"]
    );
}

// --- secrets --------------------------------------------------------------------------

const SECRET: &str = "type PartnerKey = Text {\n    min_length: 8\n}\n\nconfig {\n    api_key: PartnerKey {\n        secret: true\n    }\n}\n\n";

#[test]
fn a_secret_cannot_be_returned() {
    assert_eq!(
        rules(&format!(
            "{SECRET}function expose_secret() -> PartnerKey {{\n    return config.api_key\n}}\n"
        )),
        ["configuration.secret_flow"]
    );
}

#[test]
fn a_secret_cannot_be_compared() {
    let found = found(&format!(
        "{SECRET}type Same = Bool {{}}\n\nfunction guess(key: PartnerKey) -> Same {{\n    return Same(key == config.api_key)\n}}\n"
    ));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("configuration.secret_flow"));
}

#[test]
fn a_secret_cannot_be_stored_or_put_in_a_failure_or_built_into_another_value() {
    for body in [
        // Stored.
        "entity Vault {\n    id: Uuid\n    note: PartnerKey\n}\n\naction keep() -> Unit {\n    var vault: Vault = Vault.create {\n        set: {\n            note: config.api_key\n        }\n    }\n}\n",
        // In a failure.
        "type Leaked = Failure {\n    kind: FailureKind.rejected\n    client: {\n        key: PartnerKey\n    }\n}\n\naction leak() fails Leaked -> Unit {\n    reject Leaked {\n        key: config.api_key\n    }\n}\n",
        // Into another value.
        "type Shown = Output {\n    key: PartnerKey\n}\n\nfunction show() -> Shown {\n    return Shown {\n        key: config.api_key\n    }\n}\n",
        // Through a variable, and a call.
        "function echo(key: PartnerKey) -> PartnerKey {\n    return key\n}\n\nfunction through() -> PartnerKey {\n    var key: PartnerKey = config.api_key\n    return echo(key)\n}\n",
    ] {
        let found = rules(&format!("{SECRET}{body}"));
        assert!(
            found.iter().any(|rule| rule == "configuration.secret_flow"),
            "{body}\n{found:?}"
        );
    }
}

#[test]
fn a_type_that_is_always_secret_needs_no_flag() {
    assert_eq!(
        rules(
            "config {\n    database_url: PostgresUrl\n}\n\nfunction url() -> PostgresUrl {\n    return config.database_url\n}\n"
        ),
        ["configuration.secret_flow"]
    );
    assert_eq!(
        rules(
            "config {\n    key: SigningKey\n}\n\nfunction key() -> SigningKey {\n    return config.key\n}\n"
        ),
        ["configuration.secret_flow"]
    );
}

#[test]
fn a_secret_is_accepted_where_the_compiler_owns_the_place() {
    assert_eq!(
        connections(
            "    database_url: PostgresUrl\n",
            "        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n"
        ),
        CLEAN
    );
}

#[test]
fn a_secret_is_not_accepted_in_any_other_member_of_the_application() {
    let found = rules(
        "config {\n    api_key: Text {\n        secret: true\n    }\n}\n\napplication {\n    mail: {\n        from: config.api_key\n    }\n}\n",
    );
    assert!(
        found.iter().any(|rule| rule == "configuration.secret_flow"),
        "{found:?}"
    );
}

#[test]
fn a_signing_key_in_authentication_is_not_a_leak() {
    // Signing sessions with it comes with chunk 5.1b; reading it there
    // is not what is refused.
    let found = rules(
        "config {\n    session_signing_key: SigningKey\n}\n\nauthentication {\n    api_token: {\n        signing_key: config.session_signing_key\n    }\n}\n",
    );
    assert!(
        !found.iter().any(|rule| rule == "configuration.secret_flow"),
        "{found:?}"
    );
}

// --- connections' addresses -----------------------------------------------------------

fn connections(settings: &str, connection: &str) -> Vec<String> {
    let config = format!("config {{\n{settings}}}\n");
    let app = format!("application {{\n    connections: {{\n{connection}    }}\n}}\n");
    support::problems(
        Layout::Project,
        &[
            ("app.jadpo", app.as_str()),
            ("config.jadpo", config.as_str()),
        ],
    )
    .into_iter()
    .map(|found| {
        let rule = found.split(' ').nth(1).unwrap_or_default();
        rule.trim_end_matches(':').to_string()
    })
    .collect()
}

#[test]
fn a_database_takes_a_postgres_address_and_a_cache_a_redis_one() {
    assert_eq!(
        connections(
            "    database_url: PostgresUrl\n    redis_url: RedisUrl\n",
            "        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n        cache: {\n            driver: Driver.redis\n            url: config.redis_url\n        }\n"
        ),
        CLEAN
    );
    assert_eq!(
        connections(
            "    database_url: PostgresUrl\n    redis_url: RedisUrl\n",
            "        main: {\n            driver: Driver.postgres\n            url: config.redis_url\n        }\n"
        ),
        ["configuration.url_type"]
    );
    assert_eq!(
        connections(
            "    database_url: Url\n",
            "        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n"
        ),
        ["configuration.url_type"]
    );
}

#[test]
fn an_address_is_never_written_in_source() {
    assert_eq!(
        connections(
            "    database_url: PostgresUrl\n",
            "        main: {\n            driver: Driver.postgres\n            url: \"postgres://u:p@localhost/app\"\n        }\n"
        ),
        ["configuration.url_type"]
    );
}

#[test]
fn the_database_a_project_gets_without_declaring_one_reads_database_url() {
    let with = |setting: &str| {
        support::problems(
            Layout::Project,
            &[
                ("app.jadpo", "application {}\n"),
                (
                    "config.jadpo",
                    format!("config {{\n    {setting}\n}}\n").as_str(),
                ),
            ],
        )
        .len()
    };
    assert_eq!(with("database_url: PostgresUrl"), 0);
    assert_eq!(with("database_url: Url"), 1);
}

// --- the graph and the plan ---------------------------------------------------------------

#[test]
fn a_url_is_a_web_address() {
    assert_eq!(
        rules(
            "type Site = Url {}\n\nfunction site() -> Site {\n    return Site(\"postgres://localhost/app\")\n}\n"
        ),
        ["type.invalid_literal"]
    );
    assert_eq!(
        rules(
            "type Site = Url {}\n\nfunction site() -> Site {\n    return Site(\"https://example.test/app\")\n}\n"
        ),
        CLEAN
    );
}

// --- retention ------------------------------------------------------------------------------

#[test]
fn a_purge_period_is_a_duration_setting() {
    let with = |after: &str| {
        rules(&format!(
            "config {{\n    retention: Duration {{\n        default: 30d\n    }}\n    label: Text {{\n        default: \"x\"\n    }}\n    token: Text {{\n        secret: true\n    }}\n}}\n\nentity Note {{\n    id: Uuid\n    hidden_at: Instant?\n\n    lifecycle: {{\n        visible: hidden_at == none\n        purge: {{\n            after: {after}\n            since: hidden_at\n        }}\n    }}\n}}\n"
        ))
    };
    assert_eq!(with("30d"), CLEAN);
    assert_eq!(with("config.retention"), CLEAN);
    assert_eq!(with("config.label"), ["data.lifecycle_purge_after"]);
    assert_eq!(with("config.token"), ["configuration.secret_flow"]);
    assert_eq!(with("config.retentoin"), ["configuration.field_unknown"]);
}
