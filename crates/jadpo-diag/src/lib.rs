//! The rule catalogue and the diagnostic model (docs/compiler-runtime.md §2.2).
//!
//! Every rule is declared once in the `rules!` table below and used through
//! its typed constant in [`rule`], so a misspelt rule does not compile. A
//! [`Diagnostic`] carries the rule, the spans it points at, the reason, the
//! recommended next step with any edits, and who decides. `jadpo-term`
//! renders it; nothing here knows about terminals.

mod suggest;

use std::fmt;

pub use suggest::nearest;

/// Whether a rule can be declined (area 1, "Declining a judgement call"):
/// every finding is an error, and a judgement call may be answered by a
/// `decline <rule>: "<reason>"` line above the declaration instead of a fix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A plain mistake: only a fix answers it.
    Error,
    /// A judgement call: a fix, or a decline with a reason.
    Declinable,
    /// A judgement call the catalogue marks as a security rule: its
    /// decline also needs a person's acceptance (area 1, "Writing a
    /// decline"; area 18, "Acceptances"), since an agent can write a
    /// decline's reason itself.
    Security,
}

/// Who acts on a diagnostic's next step (area 14): a coding agent can make
/// the change, or a person has to decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Decider {
    Agent,
    Human,
}

impl Decider {
    pub fn word(self) -> &'static str {
        match self {
            Decider::Agent => "agent",
            Decider::Human => "human",
        }
    }
}

/// One entry in the rule catalogue.
#[derive(Debug, PartialEq, Eq)]
pub struct Rule {
    /// `area.name`, lower case, for example `cli.unknown_command`.
    pub id: &'static str,
    /// The language area (1–21) whose page documents the rule.
    pub area: u8,
    /// Whether a `decline` line may answer it instead of a fix.
    pub declinable: bool,
    /// Whether the catalogue marks it as a security rule, whose decline a
    /// person accepts (`PolicyChange.<rule name>_declined`, area 18).
    pub security: bool,
    /// Who usually decides the next step.
    pub decides: Decider,
    /// One line, sentence case, no full stop.
    pub summary: &'static str,
}

/// Where the site serves each rule's error page: the page of
/// `docs/errors/<id>.md` is this address followed by the rule's ID.
pub const ERROR_PAGES: &str = "https://jadpo.dev/docs/errors/";

impl Rule {
    /// The help link printed on the `help` line: the address of the
    /// rule's own error page on the site, which a reader can open
    /// wherever they are, `https://jadpo.dev/docs/errors/<id>`.
    pub fn help(&self) -> String {
        format!("{ERROR_PAGES}{}", self.id)
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id)
    }
}

macro_rules! rules {
    ($($name:ident: $id:literal, area $area:literal, $kind:ident, $decides:ident, $summary:literal;)*) => {
        /// Typed constants for every rule in the catalogue.
        pub mod rule {
            use super::{Decider, Kind, Rule};
            $(
                #[doc = $summary]
                pub const $name: Rule = Rule {
                    id: $id,
                    area: $area,
                    declinable: matches!(Kind::$kind, Kind::Declinable | Kind::Security),
                    security: matches!(Kind::$kind, Kind::Security),
                    decides: Decider::$decides,
                    summary: $summary,
                };
            )*
        }

        /// Every rule, sorted by id.
        pub static CATALOGUE: &[Rule] = &[$(rule::$name),*];
    };
}

rules! {
    ACCEPTANCE_BASE_MISSING: "acceptance.base_missing", area 14, Error, Agent,
        "Base commit the change is compared with isn't in this clone";
    ACCEPTANCE_EDITED: "acceptance.edited", area 14, Error, Agent,
        "Acceptance already on the before side was edited";
    ACCEPTANCE_INVALID: "acceptance.invalid", area 14, Error, Agent,
        "Acceptance file isn't written as an acceptance";
    ACCEPTANCE_MISSING: "acceptance.missing", area 14, Error, Human,
        "A change could break someone else or widen who can do what, and no acceptance this change adds says a person agreed to it";
    ACCEPTANCE_NO_REASON: "acceptance.no_reason", area 14, Error, Human,
        "An acceptance this change adds has no `///` line above `acceptance` saying why a person accepted the changes it lists";
    ACCEPTANCE_UNMATCHED: "acceptance.unmatched", area 14, Error, Agent,
        "Acceptance lists a change this change doesn't make";
    AI_CONNECTION_REQUIRED: "ai.connection_required", area 21, Error, Agent,
        "Action asks a model but the application names no `ai:` connection";
    AI_SETTINGS_INVALID: "ai.settings_invalid", area 21, Error, Agent,
        "Application's `ai:` holds something it doesn't take, or leaves out what it needs";
    AI_USAGE_UNAVAILABLE: "ai.usage_unavailable", area 14, Error, Human,
        "The app's AI usage couldn't be read from its database, because it can't be reached or has no table for it yet";
    API_AUTHENTICATION_INVALID: "api.authentication_invalid", area 7, Error, Agent,
        "API authentication is not a list of declared sign-in methods";
    API_IDEMPOTENCY_INVALID: "api.idempotency_invalid", area 7, Error, Agent,
        "API idempotency is not an Idempotency";
    API_PREFIX_OVERLAP: "api.prefix_overlap", area 7, Error, Agent,
        "Two APIs share a prefix";
    API_PREFIX_REQUIRED: "api.prefix_required", area 7, Error, Agent,
        "Declared API has no prefix";
    API_SERVE_OPENAPI_INVALID: "api.serve_openapi_invalid", area 7, Error, Agent,
        "An API's `serve_openapi:` setting holds something other than `true` or `false`";
    API_SETTING_UNKNOWN: "api.setting_unknown", area 7, Error, Agent,
        "API setting is not one an API takes";
    CLI_ACCEPTANCE_ARGUMENTS: "cli.acceptance_arguments", area 14, Error, Agent,
        "A `jadpo acceptance` command names no action, one it doesn't have, or no purpose for `new`";
    CLI_AI_ARGUMENTS: "cli.ai_arguments", area 14, Error, Agent,
        "A `jadpo ai` command names no action, one it doesn't have, or a prompt the project doesn't declare";
    CLI_API_ARGUMENTS: "cli.api_arguments", area 14, Error, Agent,
        "A `jadpo api` command names no action, one it doesn't have, or more than `update`";
    CLI_BUILD_ARGUMENTS: "cli.build_arguments", area 14, Error, Agent,
        "A `jadpo build` command names a deployable the app doesn't have, or more than one";
    CLI_CALLS_ARGUMENTS: "cli.calls_arguments", area 14, Error, Agent,
        "A `jadpo calls` command names an action it doesn't have, a call the log doesn't hold, or an answer without its operation";
    CLI_CHECK_ARGUMENTS: "cli.check_arguments", area 14, Error, Agent,
        "A `jadpo check` command names a path that isn't there or isn't a `.jadpo` file, or a `--before` git doesn't know";
    CLI_COMPLETIONS_ARGUMENTS: "cli.completions_arguments", area 14, Error, Agent,
        "A `jadpo completions` command names no shell, or one Jadpo writes no completions for";
    CLI_CONFIG_ARGUMENTS: "cli.config_arguments", area 14, Error, Agent,
        "A `jadpo config` command names no action, an action it doesn't have, or a setting `config.jadpo` doesn't declare";
    CLI_CONFIG_PROMPT_FAILED: "cli.config_prompt_failed", area 14, Error, Human,
        "Nothing was typed at the prompt of `jadpo config set`, or the terminal couldn't hide what you type there";
    CLI_CONFIG_TTY_REQUIRED: "cli.config_tty_required", area 14, Error, Human,
        "You ran `jadpo config set` where there is no terminal to type the value at, such as from a coding agent or a script";
    CLI_DECLINES_ARGUMENTS: "cli.declines_arguments", area 14, Error, Agent,
        "A `jadpo declines` command names no action, one it doesn't have, or a rule or path that isn't the project's";
    CLI_DELIVERIES_ARGUMENTS: "cli.deliveries_arguments", area 14, Error, Agent,
        "A `jadpo deliveries` command names an action it doesn't have, no id for one that needs it, or a state or page it doesn't list";
    CLI_DEV_ARGUMENTS: "cli.dev_arguments", area 14, Error, Agent,
        "A `jadpo dev` command names an action it doesn't have, `token` without a user's id, or a `--url` the project's several stores can't share";
    CLI_DEV_PORT_INVALID: "cli.dev_port_invalid", area 14, Error, Agent,
        "A `jadpo dev --port` flag holds something other than a whole number from 0 to 65535";
    CLI_DEV_START_FAILED: "cli.dev_start_failed", area 14, Error, Human,
        "The app couldn't start under `jadpo dev`, usually because another program is using its port, it has no database to run on, or a connection would call a provider's live API";
    CLI_EMAIL_ARGUMENTS: "cli.email_arguments", area 14, Error, Agent,
        "A `jadpo email` command names an action it doesn't have, or a mail or link the dev inbox doesn't hold";
    CLI_FIX_ARGUMENTS: "cli.fix_arguments", area 14, Error, Agent,
        "A `jadpo fix` command names a declaration with nothing to fix, or a fix it doesn't have";
    CLI_FMT_ARGUMENTS: "cli.fmt_arguments", area 14, Error, Agent,
        "A `jadpo fmt` command names a path that isn't there or isn't a `.jadpo` file";
    CLI_HELP_ARGUMENTS: "cli.help_arguments", area 14, Error, Agent,
        "A `jadpo help` command names more than one command or rule";
    CLI_LSP_ARGUMENTS: "cli.lsp_arguments", area 14, Error, Agent,
        "A `jadpo lsp` command is given an argument, which it doesn't take";
    CLI_MIGRATE_ARGUMENTS: "cli.migrate_arguments", area 14, Error, Agent,
        "A `jadpo migrate` command names no action, one it doesn't have, or a purpose for an action that takes none";
    CLI_NEW_ARGUMENTS: "cli.new_arguments", area 14, Error, Agent,
        "A `jadpo new` command names no project, or more than one";
    CLI_PACKAGE_ARGUMENTS: "cli.package_arguments", area 14, Error, Agent,
        "A `jadpo package` command names no action, one it doesn't have, or a package the project doesn't hold";
    CLI_PACKAGE_NOT_OPENAPI: "cli.package_not_openapi", area 14, Error, Agent,
        "A file given to `jadpo package import` is JSON but not an OpenAPI 3.0 or 3.1 document";
    CLI_PACKAGE_UNREADABLE: "cli.package_unreadable", area 14, Error, Agent,
        "A file given to `jadpo package import` isn't there, can't be read, or isn't JSON";
    CLI_PRESENTATION_ARGUMENTS: "cli.presentation_arguments", area 14, Error, Agent,
        "A `--color` flag holds something other than `auto`, `always` or `never`, or nothing";
    CLI_RECORDS_ARGUMENTS: "cli.records_arguments", area 14, Error, Agent,
        "A `jadpo records` command names an action other than `update`";
    CLI_ROUTES_ARGUMENTS: "cli.routes_arguments", area 14, Error, Agent,
        "A `jadpo routes` command is given an argument, which it doesn't take";
    CLI_RUNS_ARGUMENTS: "cli.runs_arguments", area 14, Error, Agent,
        "A `jadpo runs` command names an action other than `unfinished`, or leaves out its `--build`";
    CLI_SCHEDULES_ARGUMENTS: "cli.schedules_arguments", area 14, Error, Agent,
        "A `jadpo schedules` command names an action it doesn't have, or a schedule the project doesn't declare";
    CLI_SEED_ARGUMENTS: "cli.seed_arguments", area 14, Error, Agent,
        "A `jadpo seed` command names no seed, one the project doesn't have, or more than one";
    CLI_SQL_ARGUMENTS: "cli.sql_arguments", area 14, Error, Agent,
        "A `jadpo sql` command names no query or action, one the project doesn't declare, or an action that writes nothing";
    CLI_SUGGEST_ARGUMENTS: "cli.suggest_arguments", area 14, Error, Agent,
        "A `jadpo suggest` command names an action it doesn't have, or a suggestion the last scan didn't find";
    CLI_TEST_ARGUMENTS: "cli.test_arguments", area 14, Error, Agent,
        "A `jadpo test` command names a path that isn't there, or a seed, a kind or a `--name` it can't use";
    CLI_TOOLCHAIN_ARGUMENTS: "cli.toolchain_arguments", area 14, Error, Agent,
        "A `jadpo toolchain` command names an action it doesn't have, or a release that isn't kept";
    CLI_TRANSLATIONS_ARGUMENTS: "cli.translations_arguments", area 14, Error, Agent,
        "A `jadpo translations` command names no action, one it doesn't have, or a locale the project doesn't translate into";
    CLI_UNKNOWN_COMMAND: "cli.unknown_command", area 14, Error, Agent,
        "A `jadpo` command line starts with a command Jadpo doesn't have, or asks `jadpo help` about a command or rule it doesn't have";
    CLI_UNKNOWN_FLAG: "cli.unknown_flag", area 14, Error, Agent,
        "A `jadpo` command line has a flag the command doesn't take";
    CLI_VERSION_ARGUMENTS: "cli.version_arguments", area 14, Error, Agent,
        "A `jadpo version` command is given an argument, which it doesn't take";
    CLI_WHY_ARGUMENTS: "cli.why_arguments", area 14, Error, Agent,
        "A `jadpo why` command names no schedule, one the project doesn't declare, or more than one";
    CONFIGURATION_BINDING_EMPTY: "configuration.binding_empty", area 10, Error, Agent,
        "Setting names an empty environment variable";
    CONFIGURATION_BINDING_INVALID: "configuration.binding_invalid", area 10, Error, Agent,
        "Setting names a variable that is not in capitals";
    CONFIGURATION_CLIENT_ADDRESS_DUPLICATE: "configuration.client_address_duplicate", area 10, Error, Agent,
        "Client address is declared twice";
    CONFIGURATION_CLIENT_ADDRESS_HOPS_INVALID: "configuration.client_address_hops_invalid", area 10, Error, Agent,
        "Client address counts proxies with something that is not a count from 1";
    CONFIGURATION_CLIENT_ADDRESS_HOPS_UNUSED: "configuration.client_address_hops_unused", area 10, Error, Agent,
        "Client address gives a count of proxies to a source that counts none";
    CONFIGURATION_CLIENT_ADDRESS_INVALID: "configuration.client_address_invalid", area 10, Error, Agent,
        "Client address is not a block of a source and a count of proxies";
    CONFIGURATION_CLIENT_ADDRESS_SOURCE_UNKNOWN: "configuration.client_address_source_unknown", area 10, Error, Agent,
        "Client address names a source that is not one of the four";
    CONFIGURATION_DEFAULT_INVALID: "configuration.default_invalid", area 10, Error, Agent,
        "Setting value breaks its type's rules";
    CONFIGURATION_DEFAULT_LITERAL_REQUIRED: "configuration.default_literal_required", area 10, Error, Agent,
        "Setting value is not a literal";
    CONFIGURATION_DEFAULT_TYPE: "configuration.default_type", area 10, Error, Agent,
        "Setting value is the wrong kind of value for its type";
    CONFIGURATION_DUPLICATE_BINDING: "configuration.duplicate_binding", area 10, Error, Agent,
        "Two settings read the same environment variable";
    CONFIGURATION_DUPLICATE_OPTION: "configuration.duplicate_option", area 10, Error, Agent,
        "Setting option is given twice";
    CONFIGURATION_EXPECTED_FIELD: "configuration.expected_field", area 10, Error, Agent,
        "Configuration holds something that is not a setting";
    CONFIGURATION_FIELD_UNKNOWN: "configuration.field_unknown", area 10, Error, Agent,
        "Configuration has no setting with this name";
    CONFIGURATION_LOCAL_CHANGED: "configuration.local_changed", area 14, Error, Human,
        "Something else saved `.env.local` while `jadpo config set` was saving a value in it";
    CONFIGURATION_LOCAL_INVALID_ENCODING: "configuration.local_invalid_encoding", area 14, Error, Human,
        "The `.env.local` file holds bytes that aren't UTF-8 text";
    CONFIGURATION_LOCAL_READ_FAILED: "configuration.local_read_failed", area 14, Error, Human,
        "The `.env.local` file is there, but it couldn't be read: it is a folder, or you aren't allowed to read it";
    CONFIGURATION_LOCAL_SYNTAX: "configuration.local_syntax", area 14, Error, Human,
        "A line of `.env.local` isn't blank, a `#` comment or `NAME=value`, or it sets a variable a line above it already sets";
    CONFIGURATION_LOCAL_UNSAFE_TARGET: "configuration.local_unsafe_target", area 14, Error, Human,
        "The `.env.local` file that `jadpo config set` would save a value in is a link, not an ordinary file";
    CONFIGURATION_LOCAL_WRITE_FAILED: "configuration.local_write_failed", area 14, Error, Human,
        "Saving a value in `.env.local` failed, usually because the project's folder can't be written";
    CONFIGURATION_MOVE_NAME_TAKEN: "configuration.move_name_taken", area 14, Error, Agent,
        "A constant `jadpo config move` was asked to make a setting has the name of a setting `config { }` already declares";
    CONFIGURATION_MOVE_READ_UNSUPPORTED: "configuration.move_read_unsupported", area 14, Error, Agent,
        "A constant `jadpo config move` was asked to make a setting is read where a setting can't be: in a query, an update by `id`, a backfill or another constant's value";
    CONFIGURATION_MOVE_TYPE_UNSUPPORTED: "configuration.move_type_unsupported", area 14, Error, Agent,
        "A constant `jadpo config move` was asked to make a setting has a type one environment variable can't hold, such as a list or an object";
    CONFIGURATION_MULTIPLE_DECLARATIONS: "configuration.multiple_declarations", area 10, Error, Agent,
        "Configuration is declared more than once";
    CONFIGURATION_SECRET_DEFAULT: "configuration.secret_default", area 10, Error, Agent,
        "Secret setting has a value in source";
    CONFIGURATION_SECRET_FLOW: "configuration.secret_flow", area 10, Error, Agent,
        "Your code reads a secret setting somewhere other than the few places where Jadpo keeps it on the server";
    CONFIGURATION_SECRET_INVALID: "configuration.secret_invalid", area 10, Error, Agent,
        "Secret option is not true or false";
    CONFIGURATION_TEST_VALUE_MISSING: "configuration.test_value_missing", area 10, Error, Agent,
        "A test reaches code that reads a setting, and the setting has no value for tests to use";
    CONFIGURATION_TYPE_UNSUPPORTED: "configuration.type_unsupported", area 10, Error, Agent,
        "Setting type cannot be read from one variable";
    CONFIGURATION_UNKNOWN_OPTION: "configuration.unknown_option", area 10, Error, Agent,
        "Setting option is not one a setting takes";
    CONFIGURATION_URL_TYPE: "configuration.url_type", area 10, Error, Agent,
        "Connection address is not a setting of the type its driver needs";
    CONFIGURATION_VALUE_INVALID: "configuration.value_invalid", area 14, Error, Human,
        "A setting's value on this machine, from the environment or `.env.local`, doesn't fit the type `config.jadpo` gives it";
    CONFIGURATION_VALUE_MISSING: "configuration.value_missing", area 14, Error, Human,
        "A setting `config.jadpo` declares without a `default:` has no value on this machine, in the environment or `.env.local`";
    CONFORM_FAILED: "conform.failed", area 14, Error, Agent,
        "A package's answers to `jadpo package conform` don't fit the capability it fills, or the suite couldn't run";
    CONFORM_REFUSED: "conform.refused", area 14, Error, Human,
        "The conformance suite wasn't sent to the provider's live API, because the app has no connection to the package, nothing sets the connection's credential or address, or `ai:` doesn't ask its models through it";
    CONNECTION_DISK_PATH_INVALID: "connection.disk_path_invalid", area 16, Error, Human,
        "A disk store's `path:` isn't an absolute folder outside the project";
    CONNECTION_DOMAIN_NOT_LISTED: "connection.domain_not_listed", area 11, Error, Human,
        "An action calls an outside service through a connection whose `domains:` list doesn't include the action's domain";
    CONNECTION_KEY_INVALID: "connection.key_invalid", area 11, Error, Agent,
        "Connection gives a key its package doesn't take";
    CONNECTION_KEY_MISSING: "connection.key_missing", area 11, Error, Agent,
        "Connection leaves out a key its package needs";
    CONNECTION_PACKAGE_UNKNOWN: "connection.package_unknown", area 11, Error, Agent,
        "Connection names no provider package the app holds";
    CONNECTION_VALUE_INVALID: "connection.value_invalid", area 11, Error, Agent,
        "Connection's key holds a value it can't";
    CONSTANT_CYCLE: "constant.cycle", area 3, Error, Agent,
        "Constant's value depends on itself";
    CONSTANT_SIMILAR_EXISTS: "constant.similar_exists", area 3, Declinable, Agent,
        "Another constant of the same type holds the same value";
    CONSTANT_VALUE_NOT_FIXED: "constant.value_not_fixed", area 3, Error, Agent,
        "Constant's value is only known while the app runs";
    DATA_CLEARED_REQUIRED_REFERENCE: "data.cleared_required_reference", area 5, Error, Agent,
        "A reference that can never be `none` is cleared on delete";
    DATA_CONFLICT_CANNOT_BREAK: "data.conflict_cannot_break", area 6, Error, Agent,
        "A write's `conflict:` block maps a rule that the write can never break";
    DATA_CONFLICT_UNKNOWN: "data.conflict_unknown", area 6, Error, Agent,
        "Conflict mapping names no unique rule or reference";
    DATA_CONFLICT_UNMAPPED: "data.conflict_unmapped", area 6, Error, Agent,
        "A write could be refused by a unique rule or a reference, and its `conflict:` block doesn't say which failure the caller gets then";
    DATA_DUPLICATE_RULE: "data.duplicate_rule", area 5, Error, Agent,
        "Unique rule or index repeats another";
    DATA_ENTITY_IN_FIELD: "data.entity_in_field", area 5, Error, Agent,
        "Field holds an entity instead of a reference to one";
    DATA_ERASE_FIELD_REQUIRED: "data.erase_field_required", area 15, Error, Human,
        "Erasing a person would clear a field that can't be `none`: a reference whose record erasure keeps, or a marked field of a record it anonymises";
    DATA_ERASE_REQUIRED: "data.erase_required", area 15, Error, Human,
        "A `restricted` reference into records that erasing a person deletes doesn't say what erasure does to the record holding it";
    DATA_ERASE_RETENTION_REQUIRED: "data.erase_retention_required", area 15, Error, Human,
        "Erasing a person keeps records with `Erasure.retained`, but their entity has no `retention:` saying when they go";
    DATA_ERASE_UNREACHED: "data.erase_unreached", area 15, Error, Agent,
        "A field's `erase:` is on a field erasing a person never reaches: one that isn't a reference, or a reference to records that belong to no person";
    DATA_FIELD_LIST_REQUIRED: "data.field_list_required", area 5, Error, Agent,
        "Unique rule or index is not a list of fields";
    DATA_FIELD_WITHOUT_JSON: "data.field_without_json", area 5, Error, Agent,
        "Structured field holds a type with no stored JSON form";
    DATA_FILE_FIELD_INVALID: "data.file_field_invalid", area 16, Error, Agent,
        "A `File` field's store, size or types are missing or wrong";
    DATA_GENERATED_TYPE: "data.generated_type", area 5, Error, Agent,
        "Generated field is not a required `Instant`";
    DATA_IDENTITY_MISSING: "data.identity_missing", area 5, Error, Agent,
        "Entity has no `id` field";
    DATA_IDENTITY_TYPE: "data.identity_type", area 5, Error, Agent,
        "Entity's `id` is not a required `Uuid`";
    DATA_LIFECYCLE_CONDITION: "data.lifecycle_condition", area 5, Error, Agent,
        "A lifecycle's `visible:` or `when:` condition isn't a plain test of the record's own fields against fixed values";
    DATA_LIFECYCLE_HARD_DELETE: "data.lifecycle_hard_delete", area 6, Error, Human,
        "Code would remove a record that has a lifecycle for good, which only the lifecycle's own purge may do";
    DATA_LIFECYCLE_INITIAL_REQUIRED: "data.lifecycle_initial_required", area 5, Error, Agent,
        "Required field a transition sets has no starting value";
    DATA_LIFECYCLE_OWNED_FIELD: "data.lifecycle_owned_field", area 6, Error, Agent,
        "A create, update or patch sets a field that only the entity's lifecycle may change";
    DATA_LIFECYCLE_PURGE_AFTER: "data.lifecycle_purge_after", area 5, Error, Agent,
        "Purge's `after:` isn't a positive duration";
    DATA_LIFECYCLE_PURGE_SINCE: "data.lifecycle_purge_since", area 5, Error, Agent,
        "Purge's `since:` isn't a time field of the record";
    DATA_LIFECYCLE_PURGE_VISIBLE: "data.lifecycle_purge_visible", area 5, Error, Agent,
        "Lifecycle purges records but hides none";
    DATA_LIFECYCLE_SECTION: "data.lifecycle_section", area 5, Error, Agent,
        "Lifecycle has a section it doesn't take, or lacks one it needs";
    DATA_LIFECYCLE_TRANSITION_NAME: "data.lifecycle_transition_name", area 5, Error, Agent,
        "Transition is named like another operation or a field";
    DATA_LIFECYCLE_UNKNOWN_FIELD: "data.lifecycle_unknown_field", area 5, Error, Agent,
        "Lifecycle names a field the entity doesn't have";
    DATA_LIFECYCLE_VALUE: "data.lifecycle_value", area 5, Error, Agent,
        "Lifecycle sets a value that isn't fixed";
    DATA_NAME_TOO_LONG: "data.name_too_long", area 5, Error, Agent,
        "Name is too long for a table or column";
    DATA_ON_DELETE_NOT_REFERENCE: "data.on_delete_not_reference", area 5, Error, Agent,
        "Field that is not a reference sets `on_delete`";
    DATA_ON_DELETE_REQUIRED: "data.on_delete_required", area 5, Error, Human,
        "A field refers to a record of another entity but doesn't say what happens to it when that record is deleted";
    DATA_PATCH_FIELD_REQUIRED: "data.patch_field_required", area 6, Error, Agent,
        "Patch input has a field that isn't optional";
    DATA_PATCH_FIELD_UNKNOWN: "data.patch_field_unknown", area 6, Error, Agent,
        "Patch names a field the entity or its input doesn't have";
    DATA_PATCH_NOT_INPUT: "data.patch_not_input", area 6, Error, Agent,
        "Patch given something other than an `Input`";
    DATA_PATCH_OVERLAP: "data.patch_overlap", area 6, Error, Agent,
        "Supplied-field block sets a field the patch already sets";
    DATA_PERSONAL_UNREACHABLE: "data.personal_unreachable", area 15, Error, Human,
        "A field marked as personal data is on an entity that isn't a person and doesn't refer to one, so erasing a person could never find it";
    DATA_READ_IN_LOOP: "data.read_in_loop", area 6, Error, Agent,
        "Query or `get` inside a `for` loop";
    DATA_REFERENCE_ACROSS_STORES: "data.reference_across_stores", area 5, Error, Human,
        "A field refers to a record of an entity that is kept in a different database";
    DATA_REFERENCE_CYCLE: "data.reference_cycle", area 5, Error, Human,
        "Two or more entities refer to each other through required fields, so none of their records could ever be created first";
    DATA_REPEATED_FIELD: "data.repeated_field", area 5, Error, Agent,
        "Unique rule or index lists a field twice";
    DATA_RESERVED_TABLE_NAME: "data.reserved_table_name", area 5, Error, Agent,
        "Entity's table name starts with `jadpo_`";
    DATA_RETENTION_FIELD_REQUIRED: "data.retention_field_required", area 15, Error, Agent,
        "A field's `retention:` is on a field that can't be `none`, so its value could never be cleared";
    DATA_RETENTION_HELD: "data.retention_held", area 15, Error, Human,
        "An entity's `retention:` would delete records that a `restricted` reference keeps from being deleted";
    DATA_RETENTION_LIFECYCLE_FIELD: "data.retention_lifecycle_field", area 15, Error, Human,
        "A field's `retention:` would clear a field the entity's lifecycle owns or reads, changing which records are hidden or undoing what a transition set";
    DATA_RETENTION_PERIOD: "data.retention_period", area 15, Error, Agent,
        "A retention's `after:` isn't a `Duration` setting in `config.jadpo`";
    DATA_RETENTION_SINCE: "data.retention_since", area 15, Error, Agent,
        "A retention's `since:` isn't one of the record's `Instant` fields";
    DATA_SET_GENERATED_FIELD: "data.set_generated_field", area 6, Error, Agent,
        "Write sets a field the store generates";
    DATA_STORE_REQUIRED: "data.store_required", area 5, Error, Human,
        "An entity names no database to live in, and the app declares databases but none called `main`";
    DATA_STORE_UNKNOWN: "data.store_unknown", area 5, Error, Agent,
        "Entity's store is not a database connection";
    DATA_STORED_DECIMAL_SCALE: "data.stored_decimal_scale", area 6, Error, Agent,
        "Update by `id` adds a decimal with more places than its field keeps";
    DATA_STORED_VALUE_OUTSIDE_UPDATE: "data.stored_value_outside_update", area 6, Error, Agent,
        "Stored field read outside an update by `id`";
    DATA_STORED_VALUE_UNPROVEN: "data.stored_value_unproven", area 6, Error, Human,
        "An update by `id` works out a new value from a stored field, and the value could break its type's rules";
    DATA_UNIQUE_NULLABLE_FIELD: "data.unique_nullable_field", area 5, Error, Agent,
        "Unique rule includes a field that can be `none`";
    DATA_UNIQUE_SINGLE_FIELD: "data.unique_single_field", area 5, Error, Agent,
        "Unique rule has only one field";
    DATA_UNKNOWN_FIELD: "data.unknown_field", area 5, Error, Agent,
        "Unique rule or index names a field the entity does not have";
    DATA_UNKNOWN_SECTION: "data.unknown_section", area 5, Error, Agent,
        "Entity has a section Jadpo does not know";
    DATA_UPDATE_BY_ID_VALUE: "data.update_by_id_value", area 6, Error, Agent,
        "An update by `id` sets a value that the database can't work out on its own, such as one read earlier in the action";
    DATA_VECTOR_STORE_UNSUPPORTED: "data.vector_store_unsupported", area 21, Error, Agent,
        "Vector field on a store that isn't PostgreSQL";
    DATA_WRITE_BEFORE_CALL: "data.write_before_call", area 6, Error, Agent,
        "Write comes before a later outside call";
    DATA_WRITE_NOT_ENTITY: "data.write_not_entity", area 6, Error, Agent,
        "Write names a type that isn't an entity";
    DATA_WRITE_SECTION_REQUIRED: "data.write_section_required", area 6, Error, Agent,
        "Write doesn't say what it changes";
    DATA_WRITE_SECTION_UNEXPECTED: "data.write_section_unexpected", area 6, Error, Agent,
        "Write has a section it doesn't take";
    DECLINE_NO_REASON: "decline.no_reason", area 1, Error, Agent,
        "Decline gives no reason";
    DECLINE_NOT_DECLINABLE: "decline.not_declinable", area 1, Error, Agent,
        "Decline names a rule that can't be declined";
    DECLINE_UNUSED: "decline.unused", area 1, Error, Agent,
        "Decline's rule no longer fires on its declaration";
    DELIVERIES_NOT_FOUND: "deliveries.not_found", area 14, Error, Human,
        "The id given to `jadpo deliveries` matches no delivery or schedule run in the database";
    DELIVERIES_UNAVAILABLE: "deliveries.unavailable", area 14, Error, Human,
        "The app's deliveries couldn't be read from their database, because it can't be reached or has no table for them yet";
    DELIVERIES_WRONG_STATE: "deliveries.wrong_state", area 14, Error, Human,
        "A delivery or schedule run was asked to do something its state rules out, such as cancelling one that has already failed";
    DEPLOYABLE_API_UNKNOWN: "deployable.api_unknown", area 14, Error, Agent,
        "Deployable serves an API the app doesn't have";
    DEPLOYABLE_EMPTY: "deployable.empty", area 14, Error, Agent,
        "Deployable serves nothing and runs nothing";
    DEPLOYABLE_INVALID: "deployable.invalid", area 14, Error, Agent,
        "Deployable is written in a form `deployables:` doesn't take";
    DEPLOYABLE_SETTLE_UNRUN: "deployable.settle_unrun", area 14, Error, Agent,
        "Route waits for follow-on work its deployable doesn't run";
    DEPLOYABLE_TARGET_UNKNOWN: "deployable.target_unknown", area 14, Error, Agent,
        "Deployable's target isn't one Jadpo builds for";
    DEPLOYABLE_TARGET_UNSUPPORTED: "deployable.target_unsupported", area 14, Error, Agent,
        "Deployable reaches a connection its target can't run";
    DEPLOYABLE_TRIGGER_UNKNOWN: "deployable.trigger_unknown", area 14, Error, Agent,
        "Deployable runs something that isn't a trigger";
    DEPLOYABLE_UNSERVED: "deployable.unserved", area 14, Error, Agent,
        "No deployable serves an API, or runs the background work, the app has";
    EFFECT_CALL_IN_LOOP: "effect.call_in_loop", area 3, Error, Agent,
        "Outside call inside a `for` loop";
    EFFECT_FUNCTION_CALLS_ACTION: "effect.function_calls_action", area 3, Error, Agent,
        "Function calls an action or query";
    EFFECT_FUNCTION_PERSISTENCE: "effect.function_persistence", area 3, Error, Agent,
        "Function reads or writes stored data";
    EFFECT_RECURSIVE_ACTION: "effect.recursive_action", area 3, Error, Human,
        "An action calls itself, or calls other actions that end up calling it again";
    EMAIL_ATTACHMENT_LIMIT: "email.attachment_limit", area 16, Error, Agent,
        "Email's attachments may exceed what its mail package accepts";
    EMAIL_BROWSER_FAILED: "email.browser_failed", area 14, Error, Human,
        "A `jadpo email open` command couldn't start the browser to open a mail's link";
    EMAIL_CONNECTION_REQUIRED: "email.connection_required", area 16, Error, Agent,
        "Action emits an email but the application names no email connection";
    EMAIL_FIELD_UNUSED: "email.field_unused", area 16, Declinable, Agent,
        "Email's field is written by no template and no subject";
    EMAIL_HEALTH_PROBLEM: "email.health_problem", area 16, Declinable, Agent,
        "Email's template fails a check of the mail health score";
    EMAIL_PARTIAL_INVALID: "email.partial_invalid", area 16, Error, Agent,
        "An email template includes a partial that doesn't exist, can't be reached from it, or is given the wrong fields";
    EMAIL_SENT_CONTEXT: "email.sent_context", area 16, Error, Agent,
        "Reads `email.sent` outside a test";
    EMAIL_SETTINGS_INVALID: "email.settings_invalid", area 16, Error, Agent,
        "Application's `email:` or `display_name:` is invalid";
    EMAIL_TEMPLATE_MISSING: "email.template_missing", area 16, Error, Agent,
        "Email has no template, or a template no email";
    EMAIL_TEMPLATE_NOT_ALLOWED: "email.template_not_allowed", area 16, Error, Agent,
        "An email template calls a function, does arithmetic or builds a value, when it may only read its fields, translate and format";
    EMAIL_TEMPLATE_VALUE_INVALID: "email.template_value_invalid", area 16, Error, Agent,
        "An email template writes out a value that isn't text, a number or an enum with wording, such as a date";
    EVENT_EMIT_INVALID: "event.emit_invalid", area 12, Error, Agent,
        "Emit is given something that isn't an event, or is used outside an action";
    EVENT_SETTINGS_INVALID: "event.settings_invalid", area 12, Error, Agent,
        "Events settings in `app.jadpo` hold something other than a database's `store:`";
    EVENT_STORE_REQUIRED: "event.store_required", area 12, Error, Human,
        "An action emits an event but writes no database, and the app names no database to keep its events in";
    FAILURE_ATTEMPT_INFALLIBLE: "failure.attempt_infallible", area 3, Error, Agent,
        "Attempt is used on something that cannot fail";
    FAILURE_ATTEMPT_REQUIRED: "failure.attempt_required", area 3, Error, Agent,
        "A call that can fail is made as if it can't, with nothing passing its failure on or handling it";
    FAILURE_CODE_FORMAT: "failure.code_format", area 4, Error, Agent,
        "Failure code is not lower_snake_case";
    FAILURE_CONTEXT_FIELD_OVERLAP: "failure.context_field_overlap", area 4, Error, Agent,
        "Field is declared for both the client and the log";
    FAILURE_DUPLICATE_CODE: "failure.duplicate_code", area 4, Error, Human,
        "Two failures have the same code, or a failure has a code the runtime already answers with itself";
    FAILURE_DUPLICATE_CONTEXT_FIELD: "failure.duplicate_context_field", area 4, Error, Agent,
        "Failure field is given twice";
    FAILURE_DUPLICATE_DECLARATION: "failure.duplicate_declaration", area 4, Error, Agent,
        "Failure is listed twice after `fails`";
    FAILURE_DUPLICATE_MAPPING: "failure.duplicate_mapping", area 4, Error, Agent,
        "Condition is mapped twice";
    FAILURE_INVALID_COVERS_NOTHING: "failure.invalid_covers_nothing", area 4, Error, Agent,
        "Catch-all covers no rule, because every rule that can break has an arm";
    FAILURE_INVALID_OUTSIDE_CONSTRUCTION: "failure.invalid_outside_construction", area 4, Error, Agent,
        "Catch-all for rules is used on something that is not a construction";
    FAILURE_KIND_REQUIRED: "failure.kind_required", area 4, Error, Agent,
        "A failure is declared without a `kind:` saying what sort of failure it is";
    FAILURE_KIND_RUNTIME_ONLY: "failure.kind_runtime_only", area 4, Error, Agent,
        "Failure kind is raised only by the runtime";
    FAILURE_MAPPING_KIND: "failure.mapping_kind", area 4, Error, Agent,
        "Mapped failure's kind does not fit its condition";
    FAILURE_MISSING_CONTEXT_FIELD: "failure.missing_context_field", area 4, Error, Agent,
        "Rejected failure is missing a field";
    FAILURE_OUTCOME_DUPLICATE_ARM: "failure.outcome_duplicate_arm", area 3, Error, Agent,
        "Outcome match handles a failure twice";
    FAILURE_OUTCOME_INFALLIBLE: "failure.outcome_infallible", area 3, Error, Agent,
        "Outcome match is on a call that cannot fail";
    FAILURE_OUTCOME_MISSING_ARM: "failure.outcome_missing_arm", area 3, Error, Agent,
        "Outcome match does not handle every failure";
    FAILURE_OUTCOME_PATTERN_INVALID: "failure.outcome_pattern_invalid", area 3, Error, Agent,
        "Outcome match arm has a pattern for values";
    FAILURE_OUTCOME_SUBJECT_REQUIRED: "failure.outcome_subject_required", area 3, Error, Agent,
        "Outcome match is not on a call";
    FAILURE_OUTCOME_SUCCESS_DUPLICATE: "failure.outcome_success_duplicate", area 3, Error, Agent,
        "Outcome match repeats its success arm";
    FAILURE_OUTCOME_SUCCESS_MISSING: "failure.outcome_success_missing", area 3, Error, Agent,
        "Outcome match has no success arm";
    FAILURE_OUTCOME_SUCCESS_VALUE_REQUIRED: "failure.outcome_success_value_required", area 3, Error, Agent,
        "Success arm passes on a failure";
    FAILURE_OUTCOME_UNKNOWN_ARM: "failure.outcome_unknown_arm", area 3, Error, Agent,
        "Outcome arm names a failure the call cannot produce";
    FAILURE_OUTCOME_WILDCARD: "failure.outcome_wildcard", area 3, Error, Agent,
        "Outcome match uses a wildcard";
    FAILURE_PROPAGATE_PLACEMENT: "failure.propagate_placement", area 3, Error, Agent,
        "Propagate is used outside a failure arm";
    FAILURE_REJECT_PLACEMENT: "failure.reject_placement", area 3, Error, Agent,
        "Reject is used inside an expression";
    FAILURE_RULE_CANNOT_BREAK: "failure.rule_cannot_break", area 4, Error, Agent,
        "An `attempt` block maps a rule to a failure, but the value being built can never break that rule";
    FAILURE_RULE_UNMAPPED: "failure.rule_unmapped", area 4, Error, Agent,
        "An `attempt` block builds a value that can break one of its type's rules, and no arm says which failure that gives";
    FAILURE_STALE_DECLARATION: "failure.stale_declaration", area 4, Error, Agent,
        "A function or action lists a failure after `fails` that nothing in it can ever raise or pass on";
    FAILURE_UNDECLARED_PROPAGATION: "failure.undeclared_propagation", area 4, Error, Agent,
        "A function or action can end with a failure that its `fails` list doesn't name";
    FAILURE_UNKNOWN_CONTEXT_FIELD: "failure.unknown_context_field", area 4, Error, Agent,
        "Rejected failure has no such field";
    FAILURE_UNKNOWN_MAPPING: "failure.unknown_mapping", area 4, Error, Agent,
        "Mapping names a condition the operation does not have";
    FORMAT_CHANGES_REQUIRED: "format.changes_required", area 14, Error, Agent,
        "A file `jadpo fmt --check` read isn't laid out as `jadpo fmt` would write it";
    FORMAT_UNSAFE: "format.unsafe", area 14, Error, Human,
        "A file was left as it was, because what the formatter made of it didn't read back as the same code, which is a mistake in Jadpo's formatter";
    FORMAT_WRITE_FAILED: "format.write_failed", area 14, Error, Human,
        "A file `jadpo fmt` formatted couldn't be written, usually because it is read-only";
    IMPORT_DUPLICATE: "import.duplicate", area 1, Error, Agent,
        "Name or folder imported twice";
    IMPORT_MISSING: "import.missing", area 1, Error, Agent,
        "A file uses a name declared elsewhere in the project without bringing it in on a `use` line";
    IMPORT_UNKNOWN: "import.unknown", area 1, Error, Agent,
        "Import names nothing in its folder";
    IMPORT_UNNEEDED: "import.unneeded", area 1, Error, Agent,
        "A file imports a name it already sees without a `use` line, such as a built-in or something its test subject declares";
    IMPORT_UNUSED: "import.unused", area 1, Error, Agent,
        "Imported name the file does not use";
    INSTALL_FETCH_FAILED: "install.fetch_failed", area 14, Error, Human,
        "Installing a package couldn't read its git repository, or found no release there at the tag you gave";
    INSTALL_IN_USE: "install.in_use", area 14, Error, Agent,
        "Package to remove is still used by the app";
    INSTALL_LINK_REFUSED: "install.link_refused", area 14, Error, Agent,
        "Linked package in a CI or production build";
    INSTALL_SURFACE_MISMATCH: "install.surface_mismatch", area 14, Error, Human,
        "The package you are installing has code that offers something different from its committed `surface.jadpo_record`";
    LAYOUT_CALL_DIRECTION: "layout.call_direction", area 1, Error, Human,
        "Code in `shared/` uses a declaration that belongs to one domain";
    LAYOUT_FILE_NAME_MISMATCH: "layout.file_name_mismatch", area 1, Error, Agent,
        "File name does not match its declaration";
    LAYOUT_PROMOTION_REQUIRED: "layout.promotion_required", area 1, Error, Agent,
        "Declaration is used beyond the file that holds it";
    LAYOUT_ROUTE_PREFIX_MISMATCH: "layout.route_prefix_mismatch", area 1, Error, Agent,
        "Route group prefix does not match its file name";
    LAYOUT_TEST_WITHOUT_SUBJECT: "layout.test_without_subject", area 1, Error, Agent,
        "Test file has no subject beside it";
    LAYOUT_UNKNOWN_LOCATION: "layout.unknown_location", area 1, Error, Agent,
        "Jadpo file is outside the fixed layout";
    LAYOUT_UNUSED_DECLARATION: "layout.unused_declaration", area 1, Error, Agent,
        "Shared declaration is not used";
    LAYOUT_WRONG_FOLDER: "layout.wrong_folder", area 1, Error, Agent,
        "Declaration is in the wrong folder";
    LIBRARY_SIMILAR_EXISTS: "library.similar_exists", area 1, Declinable, Agent,
        "A library function already does the same";
    MIGRATION_APPLY_FAILED: "migration.apply_failed", area 14, Error, Human,
        "Applying the project's migrations to a database stopped at a step that failed";
    MIGRATION_BACKFILL_CAN_FAIL: "migration.backfill_can_fail", area 5, Error, Agent,
        "Computed backfill can fail";
    MIGRATION_BACKFILL_OUTSIDE_ROW: "migration.backfill_outside_row", area 5, Error, Agent,
        "Computed backfill reads more than its row";
    MIGRATION_DECISION_INVALID: "migration.decision_invalid", area 5, Error, Agent,
        "Migration decision does not fit its change";
    MIGRATION_DECISION_REQUIRED: "migration.decision_required", area 5, Error, Human,
        "A migration needs your decision, such as a value for records already stored, and its `changes.jadpo` doesn't give one yet";
    MIGRATION_DECISION_UNKNOWN: "migration.decision_unknown", area 5, Error, Agent,
        "Migration decides about something that is not changing";
    MIGRATION_EXTENSION_MISSING: "migration.extension_missing", area 14, Error, Human,
        "The project's migrations need a PostgreSQL extension, such as pgvector, that the database server doesn't offer or that the role applying them can't create";
    MIGRATION_EXTENSION_OUTDATED: "migration.extension_outdated", area 14, Error, Human,
        "The database server's PostgreSQL extension, such as pgvector, is older than the project's migrations and queries need";
    MIGRATION_OUT_OF_DATE: "migration.out_of_date", area 5, Error, Agent,
        "The entities ask for a database schema that no migration has written yet, because they are new or have changed since the latest migration";
    MIGRATION_RECORD_INVALID: "migration.record_invalid", area 5, Error, Human,
        "A file in a migration's folder is missing, can't be read, or was changed by hand, so Jadpo can't use the migration";
    MIGRATION_STORE_DIVERGED: "migration.store_diverged", area 14, Error, Human,
        "The migrations a database has applied don't match the project's: one has changed since it ran, the project doesn't have one, or one comes out of order";
    MIGRATION_UNFINISHED: "migration.unfinished", area 5, Error, Agent,
        "Latest migration is not finished";
    NAMING_FOLDED_COLLISION: "naming.folded_collision", area 1, Error, Agent,
        "Two names fold to the same file name";
    NAMING_GENERIC_NAME: "naming.generic_name", area 1, Error, Agent,
        "A domain or library is called something generic, such as `helpers` or `utils`, that says nothing about what it holds";
    NAMING_ROUND_TRIP: "naming.round_trip", area 1, Error, Agent,
        "Name does not survive conversion to its file name";
    PACKAGE_ACTION_INVALID: "package.action_invalid", area 11, Error, Agent,
        "Package action does what package code may not";
    PACKAGE_DIGEST_MISMATCH: "package.digest_mismatch", area 11, Error, Human,
        "The files of an installed package in `packages/` have been changed, added or removed since it was installed";
    PACKAGE_EMAILS_FOLDER: "package.emails_folder", area 11, Error, Agent,
        "Package holds email templates";
    PACKAGE_INDEX_INVALID: "package.index_invalid", area 11, Error, Agent,
        "Package's index is missing, or holds what an index can't";
    PACKAGE_LICENCE_NOT_ALLOWED: "package.licence_not_allowed", area 11, Error, Human,
        "An installed package says its licence is proprietary, but it wasn't installed from a git repository";
    PACKAGE_NAME_MISMATCH: "package.name_mismatch", area 11, Error, Agent,
        "Package's index names another package than its folder";
    PACKAGE_NAME_TAKEN: "package.name_taken", area 11, Error, Human,
        "A package has the same name as one of the app's domains, libraries, declarations or other packages";
    PACKAGE_NOT_INSTALLED: "package.not_installed", area 11, Error, Human,
        "The app's lock file, `jadpo.lock`, lists a package whose folder in `packages/` isn't there";
    PACKAGE_NOT_LOCKED: "package.not_locked", area 11, Error, Human,
        "A package sits in `packages/`, but `jadpo.lock` has no line for it, so it was copied in rather than installed";
    PACKAGE_NOT_PUBLIC: "package.not_public", area 11, Error, Agent,
        "Code uses a package's declaration that isn't public";
    PACKAGE_OPERATION_INVALID: "package.operation_invalid", area 11, Error, Agent,
        "Provider operation holds what it can't, or lacks what it needs";
    PACKAGE_PROVIDER_INVALID: "package.provider_invalid", area 11, Error, Agent,
        "Provider file holds what it can't, or lacks `protocol:`";
    PACKAGE_PROVIDER_MISSING: "package.provider_missing", area 11, Error, Agent,
        "Package has resources but no `provider.jadpo`";
    PACKAGE_PROVIDER_TRANSLATIONS: "package.provider_translations", area 11, Error, Agent,
        "Provider package carries translations";
    PACKAGE_RESERVED_NAME: "package.reserved_name", area 11, Error, Agent,
        "App's own integration takes a provider's reserved name";
    PACKAGE_STANDARD_CANDIDATE: "package.standard_candidate", area 11, Declinable, Agent,
        "Provider field looks like a standard type the package doesn't use";
    PACKAGE_VERSION_NEEDED: "package.version_needed", area 11, Error, Agent,
        "Your code names something through a package that the app holds in two versions, without saying which version it means";
    PACKAGE_VERSION_UNNEEDED: "package.version_unneeded", area 11, Error, Agent,
        "Name written through a package held in one version carries a version";
    PACKAGE_WIRE_INVALID: "package.wire_invalid", area 11, Error, Agent,
        "Field says how it is sent in a way it can't";
    PACKAGE_WIRE_NAME_INVALID: "package.wire_name_invalid", area 11, Error, Agent,
        "Wire name outside a provider package, or one the type already uses";
    POLICY_BINDING_INVALID: "policy.binding_invalid", area 9, Error, Agent,
        "Policy names something that can't say who";
    POLICY_CALL_UNGRANTED: "policy.call_ungranted", area 9, Security, Human,
        "An action listed under `actions:` calls another action that refuses some of the people the listed action's line lets in";
    POLICY_DECLARATION_DUPLICATE: "policy.declaration_duplicate", area 9, Error, Agent,
        "Policy is declared twice";
    POLICY_EFFECT_DUPLICATE: "policy.effect_duplicate", area 9, Error, Agent,
        "Policy gives the same list twice";
    POLICY_EFFECT_UNGRANTED: "policy.effect_ungranted", area 9, Error, Human,
        "A route runs an action that nobody is allowed to call, because nothing in the app says who may";
    POLICY_EFFECT_UNKNOWN: "policy.effect_unknown", area 9, Error, Agent,
        "Policy holds something that isn't a grant or a setting";
    POLICY_EVENT_TENANT_UNPROVED: "policy.event_tenant_unproved", area 9, Error, Agent,
        "An event or a batch names a tenant its action never reached under its caller's policy";
    POLICY_FIELD_WIDENS_ENTITY: "policy.field_widens_entity", area 9, Error, Agent,
        "A field's own `policy:` lets in someone its record's grants don't";
    POLICY_MEMBERSHIP_DUPLICATE: "policy.membership_duplicate", area 9, Error, Agent,
        "Entity declares a membership twice, or gives one of its parts twice";
    POLICY_MEMBERSHIP_MEMBER_REQUIRED: "policy.membership_member_required", area 9, Error, Agent,
        "Membership doesn't say who holds it";
    POLICY_MEMBERSHIP_ROLE_REQUIRED: "policy.membership_role_required", area 9, Error, Agent,
        "Membership doesn't say which field holds the role";
    POLICY_MEMBERSHIP_SCOPE_REQUIRED: "policy.membership_scope_required", area 9, Error, Agent,
        "Membership doesn't say what it is held in";
    POLICY_OUTPUT_FIELD_UNPROVED: "policy.output_field_unproved", area 9, Error, Agent,
        "A query or action lets a caller see a field's value, directly or through what it decides, though not every caller may read that field";
    POLICY_PUBLIC_GRANT_MISSING: "policy.public_grant_missing", area 9, Error, Human,
        "A route that anyone may call, with no sign-in, reads or changes a record that only signed-in people are allowed to see";
    POLICY_REQUIREMENT_INVALID: "policy.requirement_invalid", area 9, Error, Agent,
        "A role's line in `requires:` holds something a requirement can't";
    POLICY_REQUIREMENT_UNSATISFIABLE: "policy.requirement_unsatisfiable", area 9, Error, Human,
        "A role's requirement asks for a sign-in that no way in the app can give";
    POLICY_REQUIREMENTS_INCOMPLETE: "policy.requirements_incomplete", area 9, Error, Human,
        "A membership's `requires:` leaves out one of its roles";
    POLICY_ROLE_FIELD_CREATE_FORBIDDEN: "policy.role_field_create_forbidden", area 9, Error, Agent,
        "Create sets the user field its grant needs from the request, not the caller";
    POLICY_ROLE_UNBOUND: "policy.role_unbound", area 9, Error, Agent,
        "Grant names a role no membership holds";
    POLICY_SCOPE_AMBIGUOUS: "policy.scope_ambiguous", area 9, Error, Agent,
        "Grant doesn't say which reference a role or the tenant applies through";
    POLICY_SCOPE_DUPLICATE: "policy.scope_duplicate", area 9, Error, Agent,
        "A scope or tenant is given twice";
    POLICY_SCOPE_MISSING: "policy.scope_missing", area 9, Error, Agent,
        "An entity names a role held in another record, or belongs to the app's tenant, but has no reference to that record";
    POLICY_SUBJECT_DUPLICATE: "policy.subject_duplicate", area 9, Error, Agent,
        "Grant names the same caller twice";
    POLICY_SYSTEM_GRANT_MISSING: "policy.system_grant_missing", area 9, Error, Human,
        "A schedule or subscriber reaches a grant that names `Access.public` but not `Access.system`, so the background work would always be refused";
    PROMPT_FREE_TEXT_IN_INSTRUCTIONS: "prompt.free_text_in_instructions", area 21, Error, Agent,
        "A prompt writes a free-text value, such as text a user typed, in its instructions rather than in its request";
    PROMPT_PLACEHOLDER_TYPE: "prompt.placeholder_type", area 21, Error, Agent,
        "A prompt's field is of a type a prompt can't send";
    PROMPT_PLACEHOLDER_UNKNOWN: "prompt.placeholder_unknown", area 21, Error, Agent,
        "A prompt's text writes a field its type does not have";
    PROMPT_PLACEHOLDER_UNUSED: "prompt.placeholder_unused", area 21, Error, Agent,
        "A prompt's type has a field that its prompt file never writes";
    PROMPT_SECTION_MISSING: "prompt.section_missing", area 21, Error, Agent,
        "A prompt and its file don't make its two sections, each once and in order";
    PUBLISH_BREAKING_CHANGE: "publish.breaking_change", area 14, Error, Human,
        "A package's new version changes or removes something its last release offered, within the same major version";
    PUBLISH_CHANGELOG_MISSING: "publish.changelog_missing", area 14, Error, Agent,
        "Package's changelog doesn't say why its new version is released";
    PUBLISH_DESCRIPTION_MISSING: "publish.description_missing", area 14, Error, Agent,
        "Package's public declaration has no description";
    PUBLISH_EXAMPLE_FAILED: "publish.example_failed", area 14, Error, Agent,
        "Example in a package's description doesn't compile or doesn't hold";
    PUBLISH_EXAMPLE_MISSING: "publish.example_missing", area 14, Error, Agent,
        "Package's function has no example, or a failure it raises is shown by none";
    PUBLISH_SAMPLE_MISSING: "publish.sample_missing", area 14, Error, Agent,
        "Package's recipes have no sample app with one domain to compile in";
    QUERY_AGGREGATE_ARGUMENT: "query.aggregate_argument", area 6, Error, Agent,
        "Aggregate is given the wrong arguments";
    QUERY_AGGREGATE_FIELD: "query.aggregate_field", area 6, Error, Agent,
        "Aggregate cannot be worked out over this field";
    QUERY_AGGREGATE_UNKNOWN: "query.aggregate_unknown", area 6, Error, Agent,
        "Aggregate entry is not count, sum, min, max or average";
    QUERY_CONDITION_UNSUPPORTED: "query.condition_unsupported", area 6, Error, Agent,
        "Query condition is not one an index can serve";
    QUERY_FROM_NOT_ENTITY: "query.from_not_entity", area 6, Error, Agent,
        "Query reads something that is not an entity";
    QUERY_GEOMETRY_FIELD: "query.geometry_field", area 6, Error, Agent,
        "A map-layer query's `geometry:` names a field whose type isn't the shape the layer's result type says each feature holds";
    QUERY_GROUP_KEY: "query.group_key", area 6, Error, Agent,
        "Records cannot be grouped by this";
    QUERY_INCLUDE_DEPTH: "query.include_depth", area 6, Error, Human,
        "A query's includes nest more than two levels deep: records inside records inside records";
    QUERY_INCLUDE_RESULT: "query.include_result", area 6, Error, Agent,
        "Result type has no field of the right shape for an include";
    QUERY_INCLUDE_UNKNOWN: "query.include_unknown", area 6, Error, Agent,
        "Include names no reference to or from the entity";
    QUERY_LIMIT_RANGE: "query.limit_range", area 6, Error, Agent,
        "A query's `limit:` could be less than 1, either as written or because its value's type allows it";
    QUERY_LIMIT_UNBOUNDED: "query.limit_unbounded", area 6, Error, Agent,
        "Query limit or page size comes from a value with no maximum";
    QUERY_NEAREST_EXACT: "query.nearest_exact", area 21, Declinable, Agent,
        "Nearest-neighbour order on a vector too long for an index reads every record";
    QUERY_NEAREST_PAGE: "query.nearest_page", area 21, Error, Agent,
        "Nearest-neighbour query returns a page";
    QUERY_NEAREST_TYPE: "query.nearest_type", area 21, Error, Agent,
        "Nearest-neighbour order measures from something it can't";
    QUERY_NEAREST_UNINDEXED: "query.nearest_unindexed", area 21, Error, Agent,
        "Nearest-neighbour order on a vector with no fixed length";
    QUERY_NOT_UNIQUE: "query.not_unique", area 6, Error, Agent,
        "A query that returns one record, or none, picks it by a test that several records could match";
    QUERY_OPTIONAL_VALUE: "query.optional_value", area 6, Error, Agent,
        "Condition compares with a value that may be absent, or `where_present:` with one that is always there";
    QUERY_ORDER_DIRECTION: "query.order_direction", area 6, Error, Agent,
        "Order field has no `asc` or `desc`";
    QUERY_ORDER_FIELD: "query.order_field", area 6, Error, Agent,
        "A query sorts records by a field, such as a `Bool` or an enum, that the database and Jadpo don't order the same way";
    QUERY_OTHER_DOMAIN: "query.other_domain", area 6, Error, Human,
        "A query in one domain reads the records of an entity that belongs to another domain";
    QUERY_PAGE_SETTING: "query.page_setting", area 6, Error, Agent,
        "Page block is missing a setting or has a wrong one";
    QUERY_REPEATED_SECTION: "query.repeated_section", area 6, Error, Agent,
        "Query section is given twice";
    QUERY_RESULT_FIELD: "query.result_field", area 6, Error, Agent,
        "Result type has a field the entity cannot fill";
    QUERY_RESULT_RULE: "query.result_rule", area 6, Error, Agent,
        "Query result type has rules across its fields";
    QUERY_RESULT_TYPE: "query.result_type", area 6, Error, Agent,
        "Query result is not the entity or a type derived from it";
    QUERY_SECTION_REQUIRED: "query.section_required", area 6, Error, Agent,
        "A query leaves out a section its result type needs, such as `from:`, or the `limit:` that bounds a list";
    QUERY_SECTION_UNEXPECTED: "query.section_unexpected", area 6, Error, Agent,
        "Query has a section its result does not take";
    QUERY_UNKNOWN_FIELD: "query.unknown_field", area 6, Error, Agent,
        "Query names a field its entity does not have";
    QUERY_UNKNOWN_SECTION: "query.unknown_section", area 6, Error, Agent,
        "Query has a section Jadpo does not know";
    QUERY_VALUE_UNSUPPORTED: "query.value_unsupported", area 6, Error, Agent,
        "Query value is computed rather than a parameter, literal or enum value";
    RATE_LIMITS_COUNTED_IN_MEMORY: "rate_limits.counted_in_memory", area 16, Declinable, Agent,
        "Deployable built for Workers would count its rate limits in each instance's memory";
    RATE_LIMITS_STORE_INVALID: "rate_limits.store_invalid", area 16, Error, Agent,
        "Rate limits' store isn't a connection that can count them";
    RATE_LIMITS_STORE_REQUIRED: "rate_limits.store_required", area 16, Error, Agent,
        "Deployable built for Workers has nowhere to count its rate limits";
    RECEIVING_INVALID: "receiving.invalid", area 12, Error, Agent,
        "A queue in `receiving:` doesn't say how its messages are read";
    RECORD_EDITED: "record.edited", area 18, Error, Agent,
        "Generated record was edited by hand";
    RECORD_FORMAT_NEWER: "record.format_newer", area 18, Error, Human,
        "A record file in the project was written by a newer Jadpo, in a format this one can't read";
    RECORD_OUT_OF_DATE: "record.out_of_date", area 18, Error, Agent,
        "A record file that Jadpo writes from your code is missing, or no longer matches the code";
    RECORDING_FAILED: "recording.failed", area 14, Error, Agent,
        "A recording by `jadpo package record` or `jadpo ai record` got no answer worth keeping from the sandbox or the model, or its call couldn't be made";
    RECORDING_REFUSED: "recording.refused", area 14, Error, Human,
        "A recording was refused before anything was sent, because its connection would call a live API or has nothing set for its sandbox, its package is installed, or there is nothing to ask its prompt with";
    ROUTE_API_UNKNOWN: "route.api_unknown", area 7, Error, Agent,
        "Route group names an API the application doesn't declare";
    ROUTE_ARGUMENT_INVALID: "route.argument_invalid", area 7, Error, Agent,
        "Route passes something other than a placeholder, the input or their fields";
    ROUTE_AUTHENTICATION_INVALID: "route.authentication_invalid", area 7, Error, Human,
        "A route's `authentication:` line holds something other than `RouteAuthentication.none` or `RouteAuthentication.fresh`";
    ROUTE_BINDING_UNUSED: "route.binding_unused", area 7, Error, Agent,
        "Route reads a placeholder or input it never passes on";
    ROUTE_CALLS_PAST_DEADLINE: "route.calls_past_deadline", area 11, Error, Agent,
        "The outside calls a route makes could, at their slowest, take longer than the route is allowed to run";
    ROUTE_DEADLINE_INVALID: "route.deadline_invalid", area 7, Error, Agent,
        "Route deadline is not a duration up to 60 seconds";
    ROUTE_HEADER_CREDENTIAL: "route.header_credential", area 7, Error, Agent,
        "A route's input reads a header that sign-in or the runtime reads first, such as `Authorization`, a cookie or `Idempotency-Key`";
    ROUTE_HEADER_DUPLICATE: "route.header_duplicate", area 7, Error, Agent,
        "Two input fields read the same header";
    ROUTE_HEADER_NAME_INVALID: "route.header_name_invalid", area 7, Error, Agent,
        "Header name is not a valid HTTP header name";
    ROUTE_HEADER_NOT_SCALAR: "route.header_not_scalar", area 7, Error, Agent,
        "Header field is not a single value";
    ROUTE_INPUT_NOT_INPUT: "route.input_not_input", area 7, Error, Agent,
        "Route input is not an `Input` type";
    ROUTE_ITEM_DUPLICATE: "route.item_duplicate", area 7, Error, Agent,
        "Route item is given twice";
    ROUTE_ITEM_UNKNOWN: "route.item_unknown", area 7, Error, Agent,
        "Route or route group item is not one it takes";
    ROUTE_OUTPUT_MISMATCH: "route.output_mismatch", area 7, Error, Agent,
        "Route output is not exactly what it runs returns";
    ROUTE_OUTPUT_NOT_OUTPUT: "route.output_not_output", area 7, Error, Agent,
        "Route output is not an `Output` type";
    ROUTE_OUTPUT_REQUIRED: "route.output_required", area 7, Error, Agent,
        "Route has no `output:`";
    ROUTE_OUTPUT_UNEXPECTED: "route.output_unexpected", area 7, Error, Agent,
        "A `no_content` route has an output";
    ROUTE_PATH_OVERLAP: "route.path_overlap", area 7, Error, Agent,
        "Two routes could answer the same request";
    ROUTE_PLACEHOLDER_DUPLICATE: "route.placeholder_duplicate", area 7, Error, Agent,
        "Placeholder name is used twice in a route's path";
    ROUTE_PLACEHOLDER_TYPE: "route.placeholder_type", area 7, Error, Agent,
        "Placeholder is not typed with a named scalar or an `X.id`";
    ROUTE_PREFIX_API: "route.prefix_api", area 7, Error, Agent,
        "Route group prefix does not start with its API's prefix";
    ROUTE_PREFIX_REQUIRED: "route.prefix_required", area 7, Error, Agent,
        "Route group has no prefix";
    ROUTE_QUERY_FIELD_UNSUPPORTED: "route.query_field_unsupported", area 7, Error, Agent,
        "Query-string input field is not a scalar, an enum or a list of them";
    ROUTE_RUN_INVALID: "route.run_invalid", area 7, Error, Agent,
        "Route `run:` is not a call of a named action or query";
    ROUTE_RUN_KIND: "route.run_kind", area 7, Error, Agent,
        "A `GET` route runs an action, or a route of another method runs a query";
    ROUTE_RUN_REQUIRED: "route.run_required", area 7, Error, Agent,
        "Route does not say what it runs";
    ROUTE_SETTLE_INVALID: "route.settle_invalid", area 7, Error, Agent,
        "Route `settle:` is not an answer query and a wait inside its deadline";
    ROUTE_STATUS_INVALID: "route.status_invalid", area 7, Error, Agent,
        "Route status is not a `SuccessStatus`";
    ROUTE_STREAM_INVALID: "route.stream_invalid", area 7, Error, Agent,
        "Streaming route answers with another status or waits with `settle:`";
    RUNS_UNAVAILABLE: "runs.unavailable", area 14, Error, Human,
        "The app's unfinished runs couldn't be read from their database, because it can't be reached or has no table for them yet";
    SCHEDULE_EVERY_INVALID: "schedule.every_invalid", area 12, Error, Agent,
        "A schedule's `every:` says how often it runs in something other than a whole number of seconds, minutes, hours or days";
    SCHEDULE_ITEM_INVALID: "schedule.item_invalid", area 12, Error, Agent,
        "Schedule holds an item it doesn't take, gives one twice or leaves out `every:` or `run:`";
    SCHEDULE_RUN_INVALID: "schedule.run_invalid", area 12, Error, Agent,
        "Schedule's `run:` isn't a call of an action in its own domain with no arguments";
    SCHEDULE_SETTINGS_INVALID: "schedule.settings_invalid", area 12, Error, Agent,
        "Schedules settings in `app.jadpo` hold something other than a database's `store:`";
    SCHEDULE_STORE_REQUIRED: "schedule.store_required", area 12, Error, Human,
        "An app has a schedule but no database named to keep its state in, because it has several databases or none";
    SCHEDULES_LEASE_HELD: "schedules.lease_held", area 14, Error, Human,
        "A schedule was run by hand while another run of it still holds its lease, so it wasn't run again";
    SCHEDULES_NO_TICK: "schedules.no_tick", area 14, Error, Human,
        "A schedule was run by hand against a deployment, and no tick of the app picked the run up in time, so nothing ran";
    SCHEDULES_RUN_FAILED: "schedules.run_failed", area 14, Error, Human,
        "A schedule run by hand ended in a failure, or its run on the app never ended";
    SCHEDULES_UNAVAILABLE: "schedules.unavailable", area 14, Error, Human,
        "The app's schedules couldn't be read from their database, because it can't be reached or has no table for them yet";
    SEARCH_SETTINGS_INVALID: "search.settings_invalid", area 16, Error, Agent,
        "An entity's `search:` holds something it doesn't take, or leaves out what it needs";
    SEARCH_UNDECLARED: "search.undeclared", area 16, Error, Agent,
        "A query searches an entity by words that has no `search:` section";
    SEED_FAILED: "seed.failed", area 14, Error, Agent,
        "A seed `jadpo seed` ran stopped at a call that failed or faulted, and the calls before it stay in the database";
    SEED_PRODUCTION_REFUSED: "seed.production_refused", area 14, Error, Human,
        "A seed was run against a database that isn't on this machine, which may be production";
    SEED_UNAVAILABLE: "seed.unavailable", area 14, Error, Human,
        "The database a seed runs against couldn't be reached, so nothing was run";
    SEMANTIC_AI_BUDGET_MISSING: "semantic.ai_budget_missing", area 21, Error, Agent,
        "Application's `ai:` sets no daily limit";
    SEMANTIC_AI_DIMENSIONS_MISMATCH: "semantic.ai_dimensions_mismatch", area 21, Error, Agent,
        "Vector type an embedding is asked for as has a length other than `embedding_dimensions:`";
    SEMANTIC_AI_FIELD_REFUSED: "semantic.ai_field_refused", area 21, Error, Agent,
        "A value from a field marked `to_models: false`, which must never be sent to a model, reaches a prompt or text sent for an embedding";
    SEMANTIC_AI_FIELD_UNMARKED: "semantic.ai_field_unmarked", area 21, Error, Agent,
        "Code stores a value from a field marked as personal data, or `to_models: false`, in a field that doesn't carry the same marks";
    SEMANTIC_AI_MODEL_LITERAL: "semantic.ai_model_literal", area 21, Error, Agent,
        "A `ModelId` is written out anywhere but a setting's `default:`";
    SEMANTIC_AI_OUTPUT_TYPE: "semantic.ai_output_type", area 21, Error, Agent,
        "Type a model is asked to answer as is one no model's answer can be";
    SEMANTIC_AI_PERSONAL_DATA_NOT_ALLOWED: "semantic.ai_personal_data_not_allowed", area 21, Error, Human,
        "Your code sends a model personal data of a kind, such as health records, that the app's `ai:` settings don't allow it to send";
    SEMANTIC_APPLICATION_SECTION_UNKNOWN: "semantic.application_section_unknown", area 1, Error, Agent,
        "Application has a section Jadpo does not know";
    SEMANTIC_AUTH_METHOD_DUPLICATE: "semantic.auth_method_duplicate", area 8, Error, Agent,
        "Sign-in method is declared twice";
    SEMANTIC_AUTH_METHOD_UNKNOWN: "semantic.auth_method_unknown", area 8, Error, Agent,
        "Sign-in method is not one the language has";
    SEMANTIC_AUTH_NOT_SET_UP: "semantic.auth_not_set_up", area 8, Error, Human,
        "A route needs a signed-in caller, but the app declares no way for anyone to sign in";
    SEMANTIC_AUTH_PRINCIPAL_REQUIRED: "semantic.auth_principal_required", area 8, Error, Agent,
        "Sign-in method has no principal variant to sign in";
    SEMANTIC_AUTH_SETTING_INVALID: "semantic.auth_setting_invalid", area 8, Error, Agent,
        "Sign-in setting has a value it can't take";
    SEMANTIC_AUTH_SETTING_REQUIRED: "semantic.auth_setting_required", area 8, Error, Agent,
        "Sign-in method is missing a setting it needs";
    SEMANTIC_AUTH_SETTING_UNKNOWN: "semantic.auth_setting_unknown", area 8, Error, Agent,
        "Sign-in setting is not one the method takes";
    SEMANTIC_DUPLICATE_DECLARATION: "semantic.duplicate_declaration", area 1, Error, Agent,
        "Name is declared more than once";
    SEMANTIC_FAKE_INVALID: "semantic.fake_invalid", area 13, Error, Agent,
        "Test's fakes script something no provider operation answers";
    SEMANTIC_MISSING_RETURN: "semantic.missing_return", area 3, Error, Agent,
        "Callable can end without returning its result";
    SEMANTIC_MULTIPLE_APPLICATIONS: "semantic.multiple_applications", area 8, Error, Agent,
        "Project declares more than one application";
    SEMANTIC_MULTIPLE_AUTHENTICATION: "semantic.multiple_authentication", area 8, Error, Agent,
        "Project declares more than one authentication block";
    SEMANTIC_MULTIPLE_LOCALE_DECLARATIONS: "semantic.multiple_locale_declarations", area 10, Error, Agent,
        "Locales are declared more than once";
    SEMANTIC_MULTIPLE_PRINCIPALS: "semantic.multiple_principals", area 8, Error, Agent,
        "Project declares more than one principal";
    SEMANTIC_MULTIPLE_SUGGESTIONS: "semantic.multiple_suggestions", area 14, Error, Agent,
        "Suggestions settings are declared more than once";
    SEMANTIC_NAME_CASE: "semantic.name_case", area 1, Error, Agent,
        "Name has the wrong case for its kind";
    SEMANTIC_NOT_CALLABLE: "semantic.not_callable", area 1, Error, Agent,
        "Name cannot be called";
    SEMANTIC_OWN_DOMAIN_QUALIFICATION: "semantic.own_domain_qualification", area 1, Error, Agent,
        "Own-domain call is qualified";
    SEMANTIC_PRINCIPAL_VARIANT_DUPLICATE: "semantic.principal_variant_duplicate", area 8, Error, Agent,
        "Principal declares a variant twice";
    SEMANTIC_PRINCIPAL_VARIANT_INVALID: "semantic.principal_variant_invalid", area 8, Error, Agent,
        "Principal variant is not a subject and an optional active condition";
    SEMANTIC_PRINCIPAL_VARIANT_REQUIRED: "semantic.principal_variant_required", area 8, Error, Agent,
        "Principal declares neither a user nor a service";
    SEMANTIC_PRINCIPAL_VARIANT_UNKNOWN: "semantic.principal_variant_unknown", area 8, Error, Agent,
        "Principal variant is not user or service";
    SEMANTIC_REQUEST_INVALID: "semantic.request_invalid", area 13, Error, Agent,
        "Route test request doesn't fit its route";
    SEMANTIC_REQUEST_ITEM_DUPLICATE: "semantic.request_item_duplicate", area 13, Error, Agent,
        "A route test's `request { }` gives an item, or a placeholder in `params:`, twice";
    SEMANTIC_REQUEST_ROUTE_UNKNOWN: "semantic.request_route_unknown", area 13, Error, Agent,
        "A route test's `request { }` names a method and path that no route in the app answers";
    SEMANTIC_REVOCATION_DELAY_INVALID: "semantic.revocation_delay_invalid", area 8, Error, Agent,
        "Revocation delay is not a positive duration";
    SEMANTIC_STANDARD_NAMESPACE_RESERVED: "semantic.standard_namespace_reserved", area 1, Error, Agent,
        "Name belongs to a standard-library namespace";
    SEMANTIC_STANDARD_OPERATION_MOVED: "semantic.standard_operation_moved", area 19, Error, Agent,
        "Standard-library operation has moved to another name";
    SEMANTIC_STANDARD_OPERATION_QUALIFICATION: "semantic.standard_operation_qualification", area 1, Error, Agent,
        "Standard-library operation is called without its namespace";
    SEMANTIC_TEST_SETTING_DUPLICATE: "semantic.test_setting_duplicate", area 13, Error, Agent,
        "A test gives one of its settings, `clock:`, `config:` or `fakes:`, twice";
    SEMANTIC_UNKNOWN_CALLEE: "semantic.unknown_callee", area 1, Error, Agent,
        "Called name is not declared";
    SEMANTIC_UNKNOWN_NAME: "semantic.unknown_name", area 1, Error, Agent,
        "The code uses a name that nothing in the project or the standard library declares";
    SEMANTIC_UNREACHABLE_CODE: "semantic.unreachable_code", area 3, Error, Agent,
        "Code can never run";
    SEMANTIC_UNUSED_RESULT: "semantic.unused_result", area 3, Error, Agent,
        "Value is computed and thrown away";
    SEMANTIC_UNUSED_VARIABLE: "semantic.unused_variable", area 3, Error, Agent,
        "A variable is given a value that nothing ever reads";
    SEMANTIC_WRONG_NAME_KIND: "semantic.wrong_name_kind", area 1, Error, Agent,
        "Name is the wrong kind of declaration here";
    STEP_CONTEXT: "step.context", area 12, Error, Human,
        "An action makes several changing calls from somewhere a retry can't carry on where it stopped, such as a route whose API doesn't require `Idempotency-Key`";
    STEP_ENCRYPTION_KEY_REQUIRED: "step.encryption_key_required", area 12, Error, Human,
        "An action makes several changing calls, but the app names no key to encrypt the answers it keeps from them";
    STEP_LARGE_OUTPUT: "step.large_output", area 12, Error, Agent,
        "Operation with a large answer among several changing calls";
    STEP_LATE_FAILURE: "step.late_failure", area 12, Declinable, Agent,
        "An action can fail for good after a call that changes something outside the app took effect, leaving that change behind";
    STEP_SETTINGS_INVALID: "step.settings_invalid", area 12, Error, Agent,
        "Steps setting in `application { }` doesn't read";
    STEP_STORE_REQUIRED: "step.store_required", area 12, Error, Human,
        "An action makes several changing calls but writes no database, and the app names no database to keep the record of its calls in";
    STEP_TOO_MANY: "step.too_many", area 12, Error, Agent,
        "More than 8 changing calls on one way through an action";
    STEP_UNKEYED: "step.unkeyed", area 12, Error, Agent,
        "Changing call with no idempotency key beside another changing call";
    STORE_ACTION_SPANS_STORES: "store.action_spans_stores", area 6, Error, Human,
        "An action writes to two databases, itself or through an action it calls, but an action's writes all commit in one database";
    SUBSCRIBER_ARGUMENT_INVALID: "subscriber.argument_invalid", area 12, Error, Agent,
        "Subscriber passes something other than its event or a field of it";
    SUBSCRIBER_EVENT_INVALID: "subscriber.event_invalid", area 12, Error, Agent,
        "A subscriber's `on:` names something other than an event, such as an entity or a plain type";
    SUBSCRIBER_ITEM_INVALID: "subscriber.item_invalid", area 12, Error, Agent,
        "Subscriber holds an item it doesn't take, gives one twice or leaves out `on:` or `run:`";
    SUBSCRIBER_RETRY_INVALID: "subscriber.retry_invalid", area 12, Error, Agent,
        "A subscriber's `retry:` holds something other than 1 to 100 tries and up to 30 days to keep trying";
    SUBSCRIBER_RUN_INVALID: "subscriber.run_invalid", area 12, Error, Agent,
        "Subscriber's `run:` isn't a call of an action in its own domain";
    SUGGEST_RECORD_INVALID: "suggest.record_invalid", area 14, Error, Agent,
        "A suggestions record `jadpo suggest` reads, `ignored_suggestions.jadpo_record` or `taught_suggestions.jadpo_record`, isn't as Jadpo writes it";
    SUGGEST_SHARE_FAILED: "suggest.share_failed", area 14, Error, Human,
        "Jadpo HQ couldn't be reached, or refused what `jadpo suggest share` sent, so the entries wait to be shared again";
    SYNTAX_ASSERTION_FORM: "syntax.assertion_form", area 13, Error, Agent,
        "Assertion written in another language's form";
    SYNTAX_ASYNC_AWAIT: "syntax.async_await", area 3, Error, Agent,
        "Code marked `async` or `await`";
    SYNTAX_ATTEMPT_PLACEMENT: "syntax.attempt_placement", area 3, Error, Agent,
        "Attempt is used inside an expression";
    SYNTAX_COLLECTION_TYPE: "syntax.collection_type", area 2, Error, Agent,
        "Collection type written in another language's form";
    SYNTAX_COLON_REQUIRED: "syntax.colon_required", area 3, Error, Agent,
        "Member has no colon before its value";
    SYNTAX_COMMENT_STYLE: "syntax.comment_style", area 1, Error, Agent,
        "Comment written with `#` or `/* */`";
    SYNTAX_CONDITION_BRACKETS: "syntax.condition_brackets", area 3, Error, Agent,
        "Condition is not in brackets";
    SYNTAX_ELSE_IF: "syntax.else_if", area 3, Error, Agent,
        "Else-if written as `elif` or `elsif`";
    SYNTAX_EQUALITY_OPERATOR: "syntax.equality_operator", area 3, Error, Agent,
        "Comparison written with another language's operator";
    SYNTAX_EXPECTED_DECLARATION: "syntax.expected_declaration", area 1, Error, Agent,
        "Expected a declaration";
    SYNTAX_EXPECTED_EXPRESSION: "syntax.expected_expression", area 3, Error, Agent,
        "Expected a value";
    SYNTAX_EXPECTED_HTTP_METHOD: "syntax.expected_http_method", area 7, Error, Agent,
        "Route method is not one a route is written with";
    SYNTAX_EXPECTED_NAME: "syntax.expected_name", area 1, Error, Agent,
        "Expected a name";
    SYNTAX_EXPECTED_STATEMENT: "syntax.expected_statement", area 3, Error, Agent,
        "Expected a statement";
    SYNTAX_EXPORT_KEYWORD: "syntax.export_keyword", area 1, Error, Agent,
        "Declaration marked `export` or `pub`";
    SYNTAX_FAILS_AFTER_RESULT: "syntax.fails_after_result", area 4, Error, Agent,
        "Fails list comes after the result type";
    SYNTAX_FOR_LOOP: "syntax.for_loop", area 3, Error, Agent,
        "Loop written in another language's form";
    SYNTAX_FUNCTION_KEYWORD: "syntax.function_keyword", area 3, Error, Agent,
        "Function declared with `fn`, `func`, `def` or `fun`";
    SYNTAX_IMPORT_STATEMENT: "syntax.import_statement", area 1, Error, Agent,
        "Import written in another language's form";
    SYNTAX_INCREMENT_OPERATOR: "syntax.increment_operator", area 3, Error, Agent,
        "Variable changed with `++`, `--` or `+=`";
    SYNTAX_INLINE_FUNCTION_ARROW: "syntax.inline_function_arrow", area 3, Error, Agent,
        "Inline function written in another language's form";
    SYNTAX_INVALID_ESCAPE: "syntax.invalid_escape", area 1, Error, Agent,
        "Unknown escape in a string";
    SYNTAX_LOCALES_DEFAULT_REQUIRED: "syntax.locales_default_required", area 10, Error, Agent,
        "Locales declaration has no default";
    SYNTAX_LOCALES_DUPLICATE_OPTION: "syntax.locales_duplicate_option", area 10, Error, Agent,
        "Locales option is given twice";
    SYNTAX_LOCALES_SUPPORTED_REQUIRED: "syntax.locales_supported_required", area 10, Error, Agent,
        "Locales declaration lists no supported locales";
    SYNTAX_LOCALES_UNKNOWN_OPTION: "syntax.locales_unknown_option", area 10, Error, Agent,
        "Locales declaration has no such option";
    SYNTAX_LOGICAL_OPERATOR: "syntax.logical_operator", area 3, Error, Agent,
        "Logical operator written in another language's form";
    SYNTAX_MAPPING_BLOCK_REQUIRED: "syntax.mapping_block_required", area 4, Error, Agent,
        "Mappings are written after the call instead of in a block";
    SYNTAX_MATCH_ARROW: "syntax.match_arrow", area 3, Error, Agent,
        "Match arm uses `=>`";
    SYNTAX_MATCH_PATTERN: "syntax.match_pattern", area 3, Error, Agent,
        "Expected a match pattern";
    SYNTAX_MEMBER_COMMA: "syntax.member_comma", area 3, Error, Agent,
        "Members separated by commas";
    SYNTAX_NAMED_ARGUMENT_DUPLICATE: "syntax.named_argument_duplicate", area 3, Error, Agent,
        "Named argument is given twice";
    SYNTAX_NAMED_ARGUMENT_EQUALS: "syntax.named_argument_equals", area 3, Error, Agent,
        "Named argument written with `=`";
    SYNTAX_NEW_KEYWORD: "syntax.new_keyword", area 2, Error, Agent,
        "Value created with `new`";
    SYNTAX_NULL_COALESCING: "syntax.null_coalescing", area 2, Error, Agent,
        "Default for a missing value written with `??`";
    SYNTAX_NULLABLE_TYPE: "syntax.nullable_type", area 2, Error, Agent,
        "Nullable type written in another language's form";
    SYNTAX_OPTIONAL_CHAINING: "syntax.optional_chaining", area 2, Error, Agent,
        "Field of a missing value read with `?.`";
    SYNTAX_PERCENT_AMBIGUOUS: "syntax.percent_ambiguous", area 19, Error, Agent,
        "A number, `%` and a value with no spaces could be a percentage or a remainder";
    SYNTAX_POSITIONAL_AFTER_NAMED: "syntax.positional_after_named", area 3, Error, Agent,
        "Positional argument follows a named one";
    SYNTAX_RETURN_TYPE_ARROW: "syntax.return_type_arrow", area 3, Error, Agent,
        "Result type written after `:`";
    SYNTAX_SEMICOLON: "syntax.semicolon", area 3, Error, Agent,
        "Statement or member ends with `;`";
    SYNTAX_STRING_QUOTES: "syntax.string_quotes", area 2, Error, Agent,
        "String in single quotes or backticks";
    SYNTAX_SUGGESTIONS_DUPLICATE_OPTION: "syntax.suggestions_duplicate_option", area 14, Error, Agent,
        "Suggestions setting is given twice";
    SYNTAX_SUGGESTIONS_UNKNOWN_OPTION: "syntax.suggestions_unknown_option", area 14, Error, Agent,
        "Suggestions settings have no such option";
    SYNTAX_SWITCH_STATEMENT: "syntax.switch_statement", area 3, Error, Agent,
        "Choice written with `switch` or `when`";
    SYNTAX_TERNARY_OPERATOR: "syntax.ternary_operator", area 3, Error, Agent,
        "Value chosen with `? :`";
    SYNTAX_TRY_CATCH: "syntax.try_catch", area 3, Error, Agent,
        "Failure handled with `try`, `catch` or `?`";
    SYNTAX_TYPE_BLOCK_REQUIRED: "syntax.type_block_required", area 2, Error, Agent,
        "Type has no block";
    SYNTAX_TYPE_KEYWORD: "syntax.type_keyword", area 2, Error, Agent,
        "Type declared with `struct`, `interface` or `enum`";
    SYNTAX_TYPE_PARENT_REQUIRED: "syntax.type_parent_required", area 2, Error, Agent,
        "Type has no base";
    SYNTAX_UNEXPECTED_CHARACTER: "syntax.unexpected_character", area 1, Error, Agent,
        "Character is not part of Jadpo";
    SYNTAX_UNEXPECTED_TOKEN: "syntax.unexpected_token", area 1, Error, Agent,
        "Something else was expected here";
    SYNTAX_UNSUPPORTED_THROW: "syntax.unsupported_throw", area 3, Error, Agent,
        "Failure raised with `throw` or `raise`";
    SYNTAX_UNTERMINATED_STRING: "syntax.unterminated_string", area 1, Error, Agent,
        "String is not closed";
    SYNTAX_VARIABLE_KEYWORD: "syntax.variable_keyword", area 3, Error, Agent,
        "A variable is declared with `let`, `const` or `val`, words from other languages, where Jadpo writes `var`";
    SYNTAX_VARIABLE_TYPE_REQUIRED: "syntax.variable_type_required", area 2, Error, Agent,
        "Variable has no type";
    TEST_ASSERTION_FAILED: "test.assertion_failed", area 13, Error, Agent,
        "A test's `assert` doesn't hold: its condition is false, or a call checked with `assert … fails` returned or failed differently";
    TEST_FAILED: "test.failed", area 14, Error, Agent,
        "A test failed under `jadpo test`, or among a package's own tests under `jadpo package check`";
    TEST_FAKE_EXHAUSTED: "test.fake_exhausted", area 13, Error, Agent,
        "Test called a faked operation more times than its fakes script";
    TEST_FAKE_UNUSED: "test.fake_unused", area 13, Error, Agent,
        "A test passed otherwise, but its `fakes:` scripted more answers for a provider operation than the code called it for";
    TEST_FAULT: "test.fault", area 13, Error, Agent,
        "Test stopped on a fault";
    TEST_NO_TESTS: "test.no_tests", area 14, Error, Agent,
        "A `jadpo test` command found no test to run: the project has none, or none matches the path, line, name, kind or `--failed` it gives";
    TEST_RECORDING_UNDECODABLE: "test.recording_undecodable", area 11, Error, Agent,
        "A saved answer from an outside service doesn't fit the types your integration reads that answer as";
    TEST_ROUND_TRIP_FAILED: "test.round_trip_failed", area 11, Error, Agent,
        "Field's decode and encode don't round-trip a recorded value";
    TEST_RUNTIME_START_FAILED: "test.runtime_start_failed", area 14, Error, Human,
        "The tests couldn't start, because the machine couldn't give `jadpo test` the thread with a large stack they run on, usually for lack of memory";
    TEST_SUBSCRIBER_FAILED: "test.subscriber_failed", area 13, Error, Agent,
        "Subscriber failed for good during a test";
    TEST_UNEXPECTED_FAILURE: "test.unexpected_failure", area 13, Error, Agent,
        "Test ended in a failure it did not expect";
    TOOLCHAIN_ARTIFACT_WRITE_FAILED: "toolchain.artifact_write_failed", area 14, Error, Human,
        "A file Jadpo writes for you, such as a record, couldn't be written, usually because it or its folder is read-only";
    TOOLCHAIN_BUILD_FAILED: "toolchain.build_failed", area 14, Error, Human,
        "The Rust `jadpo build` wrote for a deployable didn't compile, because the machine lacks something the build runs, such as a C compiler, or because of a mistake in Jadpo";
    TOOLCHAIN_EMPTY_PROJECT: "toolchain.empty_project", area 1, Error, Agent,
        "Project declares nothing";
    TOOLCHAIN_FETCH_FAILED: "toolchain.fetch_failed", area 14, Error, Human,
        "The Rust toolchain or the crates `jadpo build` compiles with couldn't be fetched, usually because the server they come from, or the mirror in its place, can't be reached";
    TOOLCHAIN_NOT_BUILT_YET: "toolchain.not_built_yet", area 3, Error, Agent,
        "Operation is declared but not built yet";
    TOOLCHAIN_PROJECT_NOT_FOUND: "toolchain.project_not_found", area 14, Error, Agent,
        "You ran a command that works on a project in a folder with no `app.jadpo`, in it or in any folder above it";
    TOOLCHAIN_SCAFFOLD_DESTINATION_EXISTS: "toolchain.scaffold_destination_exists", area 14, Error, Human,
        "You ran `jadpo new` with the name of a file that already exists";
    TOOLCHAIN_SCAFFOLD_DESTINATION_NOT_EMPTY: "toolchain.scaffold_destination_not_empty", area 14, Error, Human,
        "You ran `jadpo new` on a folder that already has files in it, hidden files included";
    TOOLCHAIN_SCAFFOLD_NAME_INVALID: "toolchain.scaffold_name_invalid", area 14, Error, Agent,
        "You ran `jadpo new` with a path that doesn't end in a folder's name, such as `..` or `/`";
    TOOLCHAIN_SCAFFOLD_READ_FAILED: "toolchain.scaffold_read_failed", area 14, Error, Human,
        "You ran `jadpo new` on a path Jadpo can't look into, such as a folder you can't read or a path through a file";
    TOOLCHAIN_SCAFFOLD_WRITE_FAILED: "toolchain.scaffold_write_failed", area 14, Error, Human,
        "You ran `jadpo new`, and writing the project's files failed, usually because the folder it goes in can't be written";
    TOOLCHAIN_SOURCE_READ_FAILED: "toolchain.source_read_failed", area 14, Error, Human,
        "A `.jadpo` file in the project couldn't be read, because it isn't UTF-8 text or you aren't allowed to read it";
    TOOLCHAIN_VERSION_INVALID: "toolchain.version_invalid", area 1, Error, Agent,
        "Application's `jadpo:` is not a major and minor version as text";
    TOOLCHAIN_VERSION_NEWER: "toolchain.version_newer", area 1, Error, Human,
        "The project is written for a later Jadpo release line than the `jadpo` checking it, and that `jadpo` can't run the later release instead";
    TOOLCHAIN_VERSION_OLDER: "toolchain.version_older", area 1, Error, Human,
        "The project is written for an earlier Jadpo release line than the `jadpo` checking it, and that `jadpo` can't run the earlier release instead";
    TRANSLATION_EXCHANGE_FILE_INVALID: "translation.exchange_file_invalid", area 14, Error, Human,
        "A file given to `jadpo translations import` isn't XLIFF 2.x or JSON in the shape `export` writes, or can't be read as one";
    TRANSLATION_EXCHANGE_UNIT_UNKNOWN: "translation.exchange_unit_unknown", area 14, Error, Agent,
        "A file given to `jadpo translations import` holds a unit of wording the project doesn't have";
    TRANSLATION_FEATURE_OFF: "translation.feature_off", area 10, Error, Agent,
        "A language file words a Jadpo message of a feature the application has off";
    TRANSLATION_FIELD_TYPE: "translation.field_type", area 10, Error, Agent,
        "A message field is a type a wording can't hold";
    TRANSLATION_FIELD_UNUSED: "translation.field_unused", area 10, Declinable, Agent,
        "No wording in a language uses one of a message's fields";
    TRANSLATION_FILE_MISSING: "translation.file_missing", area 10, Error, Agent,
        "The app supports a locale, but a folder of messages has no language file for it";
    TRANSLATION_FILE_NAME_INVALID: "translation.file_name_invalid", area 10, Error, Agent,
        "A language file is not named for a locale";
    TRANSLATION_GROUP_UNKNOWN: "translation.group_unknown", area 10, Error, Agent,
        "A group in the root language file is not one Jadpo reads";
    TRANSLATION_JADPO_MISSING: "translation.jadpo_missing", area 10, Error, Agent,
        "The app supports a locale that Jadpo doesn't ship its own messages in, and the app hasn't worded them itself";
    TRANSLATION_LOCALE_UNSUPPORTED: "translation.locale_unsupported", area 10, Error, Agent,
        "A language file is for a locale the application does not support";
    TRANSLATION_LOCALES_REQUIRED: "translation.locales_required", area 10, Error, Agent,
        "Translations without declared locales";
    TRANSLATION_MEMBER_DUPLICATE: "translation.member_duplicate", area 10, Error, Agent,
        "A member is written twice in a language file";
    TRANSLATION_MEMBER_UNKNOWN: "translation.member_unknown", area 10, Error, Agent,
        "A language file words something that is no message, enum or email";
    TRANSLATION_MESSAGE_UNKNOWN: "translation.message_unknown", area 10, Error, Agent,
        "A translation call names no message or enum";
    TRANSLATION_OPTION_UNKNOWN: "translation.option_unknown", area 10, Error, Agent,
        "An option of translations in app.jadpo is not one it takes";
    TRANSLATION_PLACEHOLDER_STYLE: "translation.placeholder_style", area 10, Error, Agent,
        "A wording writes an amount of money without its style, or a style on another field";
    TRANSLATION_PLACEHOLDER_UNKNOWN: "translation.placeholder_unknown", area 10, Error, Agent,
        "A wording writes a placeholder, such as `{nom}`, that isn't one of its message's fields";
    TRANSLATION_REVIEW_INVALID: "translation.review_invalid", area 10, Error, Agent,
        "Translation review is not a TranslationReview";
    TRANSLATION_SOURCES_INVALID: "translation.sources_invalid", area 10, Error, Agent,
        "A translations folder's sources record cannot be read";
    TRANSLATION_STALE: "translation.stale", area 10, Error, Agent,
        "A translation was made from default wording that has changed";
    TRANSLATION_TOO_MANY_SELECTORS: "translation.too_many_selectors", area 10, Error, Agent,
        "A message has more than one enum or plural field";
    TRANSLATION_VARIANT_MISSING: "translation.variant_missing", area 10, Error, Agent,
        "A wording that changes with a count or with an enum's value leaves out one of the plural forms or values";
    TRANSLATION_VARIANT_UNKNOWN: "translation.variant_unknown", area 10, Error, Agent,
        "A wording is for a value or plural form that does not exist";
    TRANSLATION_WORDING_INVALID: "translation.wording_invalid", area 10, Error, Agent,
        "A wording is not a plain string with {field} placeholders";
    TRANSLATION_WORDING_MISSING: "translation.wording_missing", area 10, Error, Agent,
        "A language file of a supported locale leaves out one of the messages, or failures, that its folder has to word";
    TRANSLATION_WORDING_SHAPE: "translation.wording_shape", area 10, Error, Agent,
        "A member is written in the wrong shape for what it words";
    TYPE_ABSENT_FIELD: "type.absent_field", area 2, Error, Agent,
        "Omitted or optional field is not present";
    TYPE_ARGUMENT_COUNT: "type.argument_count", area 3, Error, Agent,
        "A call passes more or fewer values than the function or action it calls has parameters";
    TYPE_ARGUMENT_EMPTY: "type.argument_empty", area 19, Error, Agent,
        "Text that may be empty where it never is";
    TYPE_ARGUMENT_NEGATIVE: "type.argument_negative", area 19, Error, Agent,
        "A list or text operation is given a count or a position that might be negative";
    TYPE_ARITHMETIC_OPERAND: "type.arithmetic_operand", area 3, Error, Agent,
        "A sum, difference or division mixes two different named types, or uses a value that isn't a number";
    TYPE_ASSIGN_IMMUTABLE: "type.assign_immutable", area 3, Error, Agent,
        "A variable declared without `mut`, or a parameter, is given a new value";
    TYPE_ASSIGN_UNKNOWN: "type.assign_unknown", area 3, Error, Agent,
        "Assignment target is not declared";
    TYPE_AUTH_CREDENTIAL_STORED: "type.auth_credential_stored", area 8, Error, Agent,
        "Entity field holds an API token or a service key";
    TYPE_AUTH_PRINCIPAL_CONSTRUCTION: "type.auth_principal_construction", area 8, Error, Agent,
        "Caller is made from a record outside tests, fixtures and seeds";
    TYPE_AUTH_PRINCIPAL_FAILURE_CONTEXT: "type.auth_principal_failure_context", area 8, Error, Agent,
        "Failure field holds a caller";
    TYPE_AUTH_PRINCIPAL_INPUT: "type.auth_principal_input", area 8, Error, Agent,
        "Input field holds a caller";
    TYPE_AUTH_PRINCIPAL_OUTPUT: "type.auth_principal_output", area 8, Error, Agent,
        "Output field holds a caller";
    TYPE_AUTH_PRINCIPAL_VARIANT_UNKNOWN: "type.auth_principal_variant_unknown", area 8, Error, Agent,
        "Caller variant is not one the principal declares";
    TYPE_AUTH_PUBLIC_PRINCIPAL: "type.auth_public_principal", area 8, Error, Human,
        "A route that anyone may call, with `authentication: RouteAuthentication.none`, passes on the signed-in caller, though it never looks for one";
    TYPE_AUTH_SESSIONS_CONTEXT: "type.auth_sessions_context", area 8, Error, Agent,
        "Session, API token or service key operation outside an action, or without its sign-in method";
    TYPE_BASE_OUTSIDE_DECLARATION: "type.base_outside_declaration", area 2, Error, Agent,
        "Base is used outside a type declaration";
    TYPE_BATCH_CAPTURE_NOT_STORABLE: "type.batch_capture_not_storable", area 12, Error, Agent,
        "A batch saves a value that can't be stored";
    TYPE_BATCH_CONTEXT: "type.batch_context", area 12, Error, Agent,
        "Batch operation called outside an action";
    TYPE_BATCH_EACH_INVALID: "type.batch_each_invalid", area 12, Error, Agent,
        "A batch's `each:` or `finished:` is not one call of an action in its domain, or a write's `each:` no inline function giving the row";
    TYPE_BATCH_FROM_INVALID: "type.batch_from_invalid", area 12, Error, Agent,
        "A batch's `from:` is something other than a call of a named query that gives a `Page`, such as a query that gives a `List`";
    TYPE_BATCH_INPUT: "type.batch_input", area 12, Error, Agent,
        "An `Input` or a route placeholder holds a batch type, which only the runtime builds";
    TYPE_BATCH_ORDER_UPDATABLE: "type.batch_order_updatable", area 12, Declinable, Agent,
        "A batch walks a query ordered by a field an update can change";
    TYPE_BATCH_ROW_INVALID: "type.batch_row_invalid", area 12, Error, Agent,
        "A file helper's rows can't be read as the type its `as:` names or from where its `items_at:` says, its `max_unpacked_size:` isn't a size, or its rows can't be written as the row its `each:` gives";
    TYPE_CLOCK_CONTEXT: "type.clock_context", area 10, Error, Agent,
        "The operation clock is unavailable in this context";
    TYPE_CONSTRAINT_BASE: "type.constraint_base", area 2, Error, Agent,
        "Constraint does not fit the type's base";
    TYPE_CONSTRAINT_CONTRADICTION: "type.constraint_contradiction", area 2, Error, Agent,
        "Constraints contradict each other";
    TYPE_CONSTRAINT_LOOSENED: "type.constraint_loosened", area 2, Error, Agent,
        "A type that builds on another sets a looser bound than the one it inherits, such as a higher `max_length`";
    TYPE_CONSTRAINT_REPEATED: "type.constraint_repeated", area 2, Error, Agent,
        "Constraint is given twice";
    TYPE_CONSTRUCTOR_ARGUMENT_COUNT: "type.constructor_argument_count", area 2, Error, Agent,
        "Constructor is not given exactly one value";
    TYPE_CONSTRUCTOR_INPUT: "type.constructor_input", area 2, Error, Agent,
        "A type's constructor is given a value it can't convert, because the value is built on a different kind of value or may be `none`";
    TYPE_COPY_CLASH: "type.copy_clash", area 2, Error, Agent,
        "Copied field would replace another field";
    TYPE_COPY_CYCLE: "type.copy_cycle", area 2, Error, Agent,
        "Type includes its own fields";
    TYPE_COPY_INTO_INPUT: "type.copy_into_input", area 2, Error, Human,
        "An `Input` type copies every field of another type with `include`, so a request could set any of them";
    TYPE_DECIMAL_SCALE_REQUIRED: "type.decimal_scale_required", area 2, Error, Agent,
        "Decimal type has no scale";
    TYPE_DIVISOR_ZERO: "type.divisor_zero", area 3, Error, Agent,
        "A division's divisor has a type that allows zero, so the division could fail when the program runs";
    TYPE_DUPLICATE_FIELD: "type.duplicate_field", area 2, Error, Agent,
        "Field is given twice";
    TYPE_DUPLICATE_VARIANT: "type.duplicate_variant", area 2, Error, Agent,
        "Enum variant is declared twice";
    TYPE_DUPLICATE_VARIANT_FIELD: "type.duplicate_variant_field", area 2, Error, Agent,
        "Variant field is given twice";
    TYPE_ENUM_EMPTY: "type.enum_empty", area 2, Error, Agent,
        "Enum declares no variants";
    TYPE_ENUM_LABEL: "type.enum_label", area 2, Error, Agent,
        "Enum variant is given a label or a value";
    TYPE_FIELD_ON_NON_RECORD: "type.field_on_non_record", area 2, Error, Agent,
        "Field is read from a value without fields";
    TYPE_FIELD_UNPROVED: "type.field_unproved", area 2, Error, Agent,
        "Object field value may break the field's rules";
    TYPE_FIELD_UPDATE_FORBIDDEN: "type.field_update_forbidden", area 9, Error, Agent,
        "An update changes a field that stays fixed once its record exists, such as a user field a grant names or a field marked `immutable: true`";
    TYPE_FILE_READ_TOO_LARGE: "type.file_read_too_large", area 16, Error, Agent,
        "Code reads a whole file into memory from a file field that can hold more than the 10 MB a whole read allows";
    TYPE_FOREIGN_TYPE: "type.foreign_type", area 2, Error, Agent,
        "Type named as another language names it";
    TYPE_FOREIGN_VALUE: "type.foreign_value", area 2, Error, Agent,
        "Value written as another language writes it";
    TYPE_INCOMPARABLE: "type.incomparable", area 3, Error, Agent,
        "Values cannot be compared for equality";
    TYPE_INLINE_FUNCTION_PARAMETERS: "type.inline_function_parameters", area 3, Error, Agent,
        "Inline function has a different number of parameters than its operation gives";
    TYPE_INLINE_FUNCTION_PLACEMENT: "type.inline_function_placement", area 3, Error, Agent,
        "Inline function is not an argument to a collection operation";
    TYPE_INVALID_LITERAL: "type.invalid_literal", area 2, Error, Agent,
        "A literal value breaks a rule of the type it goes into, such as a number below the type's `min`";
    TYPE_INVALID_PATTERN: "type.invalid_pattern", area 2, Error, Agent,
        "Pattern is not a supported regular expression";
    TYPE_LOCALE_DEFAULT_UNSUPPORTED: "type.locale_default_unsupported", area 10, Error, Agent,
        "Default locale is not a supported one";
    TYPE_LOCALE_DUPLICATE: "type.locale_duplicate", area 10, Error, Agent,
        "Locale is listed twice";
    TYPE_LOCALE_IN_PROGRESS_DEFAULT: "type.locale_in_progress_default", area 10, Error, Agent,
        "The default locale is listed as in progress";
    TYPE_LOCALE_IN_PROGRESS_UNSUPPORTED: "type.locale_in_progress_unsupported", area 10, Error, Agent,
        "An in-progress locale is not a supported locale";
    TYPE_LOCALE_INVALID: "type.locale_invalid", area 10, Error, Agent,
        "Locale is not written as a Locale";
    TYPE_LOCALE_UNSUPPORTED: "type.locale_unsupported", area 10, Error, Agent,
        "Your code writes out a locale, such as `Locale.fr_fr`, that isn't in the app's list of supported locales";
    TYPE_LOGICAL_OPERAND: "type.logical_operand", area 3, Error, Agent,
        "Logical operator needs Bool operands";
    TYPE_MAP_KEY_BOUNDARY: "type.map_key_boundary", area 2, Error, Agent,
        "Map key at a boundary isn't text";
    TYPE_MATCH_ARM_VALUE: "type.match_arm_value", area 3, Error, Agent,
        "Match arm gives no value";
    TYPE_MATCH_DUPLICATE_PATTERN: "type.match_duplicate_pattern", area 3, Error, Agent,
        "Match pattern is already covered";
    TYPE_MATCH_NON_EXHAUSTIVE: "type.match_non_exhaustive", area 3, Error, Agent,
        "A `match` on an enum, a `Bool` or a value that may be `none` leaves out one of its cases";
    TYPE_MATCH_PATTERN_TYPE: "type.match_pattern_type", area 3, Error, Agent,
        "Pattern cannot match this value";
    TYPE_MATCH_SOME_NON_OPTIONAL: "type.match_some_non_optional", area 3, Error, Agent,
        "Nullable pattern on a value that is never none";
    TYPE_MATCH_UNKNOWN_BINDING: "type.match_unknown_binding", area 3, Error, Agent,
        "Pattern names data a case does not carry";
    TYPE_MATCH_UNKNOWN_VARIANT: "type.match_unknown_variant", area 3, Error, Agent,
        "Pattern names an unknown enum variant";
    TYPE_MATCH_UNREACHABLE_PATTERN: "type.match_unreachable_pattern", area 3, Error, Agent,
        "Match arm is unreachable after the wildcard";
    TYPE_MATCH_WILDCARD_CLOSED: "type.match_wildcard_closed", area 3, Error, Agent,
        "Wildcard on a value whose cases can be listed";
    TYPE_MATCH_WILDCARD_REQUIRED: "type.match_wildcard_required", area 3, Error, Agent,
        "A `match` on text or a number lists some values but has no `_` arm for all the others";
    TYPE_MISMATCH: "type.mismatch", area 2, Error, Agent,
        "A value of one type is used where a different type is required";
    TYPE_MISSING_FIELD: "type.missing_field", area 2, Error, Agent,
        "An object value is built without one of the fields its type declares";
    TYPE_MISSING_VARIANT_FIELD: "type.missing_variant_field", area 2, Error, Agent,
        "Variant is missing a field";
    TYPE_MONEY_SPLIT: "type.money_split", area 19, Error, Agent,
        "Money is divided by a count";
    TYPE_NAMED_ARGUMENT_UNKNOWN: "type.named_argument_unknown", area 3, Error, Agent,
        "Named argument is not a parameter";
    TYPE_NAMED_ARGUMENT_UNSUPPORTED: "type.named_argument_unsupported", area 3, Error, Agent,
        "Callable does not take named arguments";
    TYPE_NOT_A_TYPE: "type.not_a_type", area 2, Error, Agent,
        "Field's type is written as a value or a call";
    TYPE_NOT_A_VALUE: "type.not_a_value", area 3, Error, Agent,
        "Declaration named where a value is read";
    TYPE_NOT_RECORD: "type.not_record", area 2, Error, Agent,
        "Braces build a type that has no fields";
    TYPE_NULLABLE_SELECTION: "type.nullable_selection", area 2, Error, Agent,
        "Field is read from a value that may be none";
    TYPE_OPTIONAL_OUTSIDE_INPUT: "type.optional_outside_input", area 2, Error, Agent,
        "Optional or default field outside an Input";
    TYPE_ORDERING_OPERAND: "type.ordering_operand", area 3, Error, Agent,
        "A comparison such as `<` or `>` is used on a value that has no order, such as text or a value that may be `none`";
    TYPE_PASSWORD_HASH_EXPOSED: "type.password_hash_exposed", area 19, Error, Agent,
        "Password hash is sent, read from a request or put in a failure";
    TYPE_PRESENTATION_TEXT_PERSISTENCE: "type.presentation_text_persistence", area 10, Error, Agent,
        "Formatted text is stored";
    TYPE_PRIMITIVE_SIGNATURE: "type.primitive_signature", area 2, Error, Agent,
        "Signature uses a built-in type directly";
    TYPE_PRINCIPAL_ACTIVE_INVALID: "type.principal_active_invalid", area 8, Error, Agent,
        "Principal's active condition is not a test of the record's own fields";
    TYPE_PRINCIPAL_SUBJECT_INVALID: "type.principal_subject_invalid", area 8, Error, Agent,
        "Principal's subject is not a field of a stored entity";
    TYPE_PRINCIPAL_SUBJECT_NOT_UNIQUE: "type.principal_subject_not_unique", area 8, Error, Agent,
        "Principal's subject field is not unique";
    TYPE_RANDOM_CONTEXT: "type.random_context", area 19, Error, Agent,
        "Randomness is read outside an action";
    TYPE_REDUNDANT_NULLABILITY: "type.redundant_nullability", area 2, Error, Agent,
        "Type is already nullable";
    TYPE_RESERVED_FIELD_NAME: "type.reserved_field_name", area 2, Error, Agent,
        "Field uses a reserved name";
    TYPE_RESERVED_VARIANT_NAME: "type.reserved_variant_name", area 19, Error, Agent,
        "Variant is called `all`, which lists an enum's variants";
    TYPE_RULE_NAME_TAKEN: "type.rule_name_taken", area 2, Error, Agent,
        "Rule is named like a field, another rule or `invalid`";
    TYPE_RULE_NOT_CONDITION: "type.rule_not_condition", area 2, Error, Agent,
        "Rule's condition isn't a `Bool`";
    TYPE_RULE_OUTSIDE_FIELDS: "type.rule_outside_fields", area 2, Error, Agent,
        "Rule reads something besides its type's fields";
    TYPE_RULE_OUTSIDE_OBJECT: "type.rule_outside_object", area 2, Error, Agent,
        "Rule is written outside an object type";
    TYPE_SECRET_EXPOSED: "type.secret_exposed", area 19, Error, Agent,
        "Secret value is sent, stored, logged or read as text";
    TYPE_SETTING_VALUE: "type.setting_value", area 2, Error, Agent,
        "Setting has the wrong kind of value";
    TYPE_SIBLING_MISMATCH: "type.sibling_mismatch", area 2, Error, Agent,
        "Field type is used for a sibling field type";
    TYPE_STANDARD_CLASH: "type.standard_clash", area 19, Error, Agent,
        "Declaration has a standard type's name";
    TYPE_STANDARD_EXISTS: "type.standard_exists", area 19, Declinable, Agent,
        "You declared a type with the same fields as a standard type, such as a start and an end, under a name of your own";
    TYPE_SUGGESTIONS_VALUE_INVALID: "type.suggestions_value_invalid", area 14, Error, Agent,
        "Suggestions setting has a value it can't take";
    TYPE_TEMPORAL_CLOCK_INVALID: "type.temporal_clock_invalid", area 10, Error, Agent,
        "Clock time is not written `HH:MM`";
    TYPE_TEMPORAL_FORMAT_COMPONENT_DUPLICATE: "type.temporal_format_component_duplicate", area 10, Error, Agent,
        "Format component is given twice";
    TYPE_TEMPORAL_FORMAT_COMPONENT_INVALID: "type.temporal_format_component_invalid", area 10, Error, Agent,
        "Format component takes no such style";
    TYPE_TEMPORAL_FORMAT_COMPONENTS_EMPTY: "type.temporal_format_components_empty", area 10, Error, Agent,
        "Declared format shows nothing";
    TYPE_TEMPORAL_FORMAT_INVALID: "type.temporal_format_invalid", area 10, Error, Agent,
        "Format is not a written-out time format";
    TYPE_TEMPORAL_FORMAT_PART_MISSING: "type.temporal_format_part_missing", area 19, Error, Agent,
        "Format shows a part a local time doesn't have";
    TYPE_TEMPORAL_FORMAT_REQUIRES_TIME: "type.temporal_format_requires_time", area 10, Error, Agent,
        "Your code formats a date with a format that also shows a clock time, and a date has no time to show";
    TYPE_TEMPORAL_LOCALES_REQUIRED: "type.temporal_locales_required", area 10, Error, Agent,
        "Formatting needs declared locales";
    TYPE_TEMPORAL_OPTION_UNUSED: "type.temporal_option_unused", area 10, Error, Agent,
        "Option means nothing for this value";
    TYPE_TEMPORAL_ZONE_REQUIRED: "type.temporal_zone_required", area 10, Error, Agent,
        "Your code formats an `Instant` for people to read without saying which time zone to show it in";
    TYPE_TEST_CLOCK_CONTEXT: "type.test_clock_context", area 13, Error, Agent,
        "The clock is moved outside a test";
    TYPE_TYPE_ARGUMENT_COUNT: "type.type_argument_count", area 2, Error, Agent,
        "Type has the wrong number of type arguments";
    TYPE_UNARY_OPERAND: "type.unary_operand", area 3, Error, Agent,
        "Operator cannot be used with this value";
    TYPE_UNIT_AS_NUMBER: "type.unit_as_number", area 2, Declinable, Agent,
        "You declared a type named for a unit, such as `Grams`, `Pence` or `Percent`, on a plain number";
    TYPE_UNIT_REQUIRED: "type.unit_required", area 19, Error, Agent,
        "A plain number is given where a physical quantity, such as a length or a mass, needs its unit";
    TYPE_UNIT_UNKNOWN: "type.unit_unknown", area 19, Error, Agent,
        "Number is written with a suffix that is no unit";
    TYPE_UNKNOWN_ENUM_VARIANT: "type.unknown_enum_variant", area 2, Error, Agent,
        "The code names a variant that the enum doesn't have, often a misspelling of one it does";
    TYPE_UNKNOWN_FIELD: "type.unknown_field", area 2, Error, Agent,
        "Type has no such field";
    TYPE_UNKNOWN_SETTING: "type.unknown_setting", area 2, Error, Agent,
        "Setting is not one Jadpo knows";
    TYPE_UNKNOWN_VALUE: "type.unknown_value", area 3, Error, Agent,
        "Name is not defined in this scope";
    TYPE_UNKNOWN_VARIANT_FIELD: "type.unknown_variant_field", area 2, Error, Agent,
        "Variant has no such field";
    TYPE_VARIANT_RETIRED: "type.variant_retired", area 19, Declinable, Agent,
        "Code writes a retired code";
    VISIBILITY_PRIVATE_CROSS_DOMAIN: "visibility.private_cross_domain", area 1, Error, Human,
        "Code in one domain uses an action, type or other declaration that another domain keeps private, because it isn't marked `public`";
    VISIBILITY_PUBLIC_NOT_ALLOWED: "visibility.public_not_allowed", area 1, Error, Agent,
        "Declaration can't be public";
    VISIBILITY_PUBLIC_REACHES_PRIVATE: "visibility.public_reaches_private", area 1, Error, Agent,
        "Public declaration's signature names one that isn't public";
    VISIBILITY_PUBLIC_UNUSED: "visibility.public_unused", area 1, Error, Agent,
        "Public declaration no other domain uses";
}

/// Looks a rule up by id, for tools that read ids back from JSON.
pub fn find(id: &str) -> Option<&'static Rule> {
    CATALOGUE.iter().find(|rule| rule.id == id)
}

/// What a declinable rule's next step adds: the line that declines it,
/// and for a security rule that a person accepts the decline.
pub fn decline_offer(rule: &Rule) -> String {
    let accepted = if rule.security {
        ", which a person then accepts"
    } else {
        ""
    };
    format!(
        ", or write `decline {}: \"<why>\"` above it if this is deliberate{accepted}",
        rule.id
    )
}

/// A byte range in one project file. Paths are project-relative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub path: String,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(path: impl Into<String>, start: u32, end: u32) -> Span {
        Span {
            path: path.into(),
            start,
            end,
        }
    }
}

/// A span with the few words printed beside its carets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub span: Span,
    pub text: String,
}

/// A text replacement an editor or agent can apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub span: Span,
    pub replacement: String,
}

/// The recommended next step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Next {
    pub text: String,
    pub decides: Decider,
    pub edits: Vec<Edit>,
}

/// One problem, in the shape every Jadpo error shares (area 14).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub rule: &'static Rule,
    /// The headline's subject: what is wrong, in the reader's terms.
    pub message: String,
    pub primary: Option<Label>,
    pub secondary: Vec<Label>,
    pub reason: Option<String>,
    pub next: Option<Next>,
    /// Printed only when it says something the rest does not.
    pub impact: Option<String>,
    /// The names in use that a rename should know about, each with its
    /// file (`semantic.duplicate_declaration`, area 1): printed as the `in
    /// use` field after `next`.
    pub in_use: Vec<(String, String)>,
}

impl Diagnostic {
    pub fn new(rule: &'static Rule, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            rule,
            message: message.into(),
            primary: None,
            secondary: Vec::new(),
            reason: None,
            next: None,
            impact: None,
            in_use: Vec::new(),
        }
    }

    /// The help link printed on the `help` line: the rule's error page on
    /// the site ([`Rule::help`]).
    pub fn help(&self) -> String {
        self.rule.help()
    }

    pub fn at(mut self, span: Span, label: impl Into<String>) -> Diagnostic {
        self.primary = Some(Label {
            span,
            text: label.into(),
        });
        self
    }

    pub fn also(mut self, span: Span, label: impl Into<String>) -> Diagnostic {
        self.secondary.push(Label {
            span,
            text: label.into(),
        });
        self
    }

    pub fn reason(mut self, reason: impl Into<String>) -> Diagnostic {
        self.reason = Some(reason.into());
        self
    }

    /// The next step, decided by whoever the rule names. A declinable
    /// rule's next step also offers the decline line.
    pub fn next(self, text: impl Into<String>) -> Diagnostic {
        let decides = self.rule.decides;
        self.next_for(text, decides)
    }

    /// The next step, decided by someone other than the rule's default.
    pub fn next_for(mut self, text: impl Into<String>, decides: Decider) -> Diagnostic {
        let mut text = text.into();
        if self.rule.declinable {
            text.push_str(&decline_offer(self.rule));
        }
        self.next = Some(Next {
            text,
            decides,
            edits: Vec::new(),
        });
        self
    }

    /// Adds an edit to the next step. Call after [`Diagnostic::next`].
    pub fn edit(mut self, span: Span, replacement: impl Into<String>) -> Diagnostic {
        if let Some(next) = self.next.as_mut() {
            next.edits.push(Edit {
                span,
                replacement: replacement.into(),
            });
        }
        self
    }

    /// The names in use, each with its file, printed as the `in use` field.
    pub fn in_use(mut self, names: Vec<(String, String)>) -> Diagnostic {
        self.in_use = names;
        self
    }

    pub fn impact(mut self, impact: impl Into<String>) -> Diagnostic {
        self.impact = Some(impact.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_is_sorted_and_unique() {
        let ids: Vec<&str> = CATALOGUE.iter().map(|rule| rule.id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            ids, sorted,
            "keep the rules! table sorted by id, with no repeats"
        );
    }

    #[test]
    fn rule_ids_and_summaries_are_well_formed() {
        for rule in CATALOGUE {
            let (area, name) = rule.id.split_once('.').expect("ids are area.name");
            for part in [area, name] {
                assert!(
                    !part.is_empty() && part.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                    "{}",
                    rule.id
                );
            }
            assert!(
                (1..=21).contains(&rule.area),
                "{}: area {}",
                rule.id,
                rule.area
            );
            let first = rule.summary.chars().next().expect("summary");
            assert!(
                first.is_uppercase() && !rule.summary.ends_with('.'),
                "{}: {}",
                rule.id,
                rule.summary
            );
        }
    }

    #[test]
    fn a_declinable_rules_next_step_offers_the_decline_line() {
        let diagnostic = Diagnostic::new(&rule::LIBRARY_SIMILAR_EXISTS, "A similar one exists")
            .next("Call `rounding.capped` instead");
        assert_eq!(
            diagnostic.next.map(|next| next.text).as_deref(),
            Some(
                "Call `rounding.capped` instead, or write `decline library.similar_exists: \"<why>\"` above it if this is deliberate"
            )
        );
        let declinable: Vec<&str> = CATALOGUE
            .iter()
            .filter(|rule| rule.declinable)
            .map(|rule| rule.id)
            .collect();
        assert_eq!(
            declinable,
            [
                "constant.similar_exists",
                "email.field_unused",
                "email.health_problem",
                "library.similar_exists",
                "package.standard_candidate",
                "policy.call_ungranted",
                "query.nearest_exact",
                "rate_limits.counted_in_memory",
                "step.late_failure",
                "translation.field_unused",
                "type.batch_order_updatable",
                "type.standard_exists",
                "type.unit_as_number",
                "type.variant_retired"
            ]
        );
    }

    #[test]
    fn a_security_rules_decline_is_accepted_by_a_person() {
        let security: Vec<&str> = CATALOGUE
            .iter()
            .filter(|rule| rule.security)
            .map(|rule| rule.id)
            .collect();
        assert_eq!(security, ["policy.call_ungranted"]);
        const { assert!(rule::POLICY_CALL_UNGRANTED.declinable) };
        let diagnostic = Diagnostic::new(&rule::POLICY_CALL_UNGRANTED, "A call refuses")
            .next("List the called action for them");
        assert_eq!(
            diagnostic.next.map(|next| next.text).as_deref(),
            Some(
                "List the called action for them, or write `decline policy.call_ungranted: \"<why>\"` above it if this is deliberate, which a person then accepts"
            )
        );
    }

    #[test]
    fn find_returns_the_typed_constant() {
        assert_eq!(
            find("cli.unknown_command"),
            Some(&rule::CLI_UNKNOWN_COMMAND)
        );
        assert_eq!(find("cli.no_such_rule"), None);
    }

    #[test]
    fn builder_fills_the_shared_shape() {
        let diagnostic = Diagnostic::new(&rule::CLI_UNKNOWN_COMMAND, "`x` isn't a jadpo command")
            .reason("Jadpo has no command with this name.")
            .next("Run `jadpo help` to see every command");
        assert!(!diagnostic.rule.declinable);
        assert_eq!(
            diagnostic.next.as_ref().map(|next| next.decides),
            Some(Decider::Agent)
        );
        assert_eq!(
            diagnostic.rule.help(),
            "https://jadpo.dev/docs/errors/cli.unknown_command"
        );
    }
}
