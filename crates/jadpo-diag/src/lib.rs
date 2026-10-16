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

/// How serious a rule is. The severity word in a headline comes from here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    /// The lower-case word used in headlines and JSON.
    pub fn word(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
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
    /// The language area (1–18) whose page documents the rule.
    pub area: u8,
    pub severity: Severity,
    /// Who usually decides the next step.
    pub decides: Decider,
    /// The rule's entry on the language pages, relative to `docs/language/`.
    pub anchor: &'static str,
    /// One line, sentence case, no full stop.
    pub summary: &'static str,
}

impl Rule {
    /// The help link printed on the `help` line.
    pub fn help(&self) -> String {
        format!("docs/language/{}", self.anchor)
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id)
    }
}

macro_rules! rules {
    ($($name:ident: $id:literal, area $area:literal, $severity:ident, $decides:ident, $anchor:literal, $summary:literal;)*) => {
        /// Typed constants for every rule in the catalogue.
        pub mod rule {
            use super::{Decider, Rule, Severity};
            $(
                #[doc = $summary]
                pub const $name: Rule = Rule {
                    id: $id,
                    area: $area,
                    severity: Severity::$severity,
                    decides: Decider::$decides,
                    anchor: $anchor,
                    summary: $summary,
                };
            )*
        }

        /// Every rule, sorted by id.
        pub static CATALOGUE: &[Rule] = &[$(rule::$name),*];
    };
}

rules! {
    API_AUTHENTICATION_INVALID: "api.authentication_invalid", area 7, Error, Agent,
        "07-http.html#api-authentication-invalid", "API authentication is not a list of declared sign-in methods";
    API_IDEMPOTENCY_INVALID: "api.idempotency_invalid", area 7, Error, Agent,
        "07-http.html#api-idempotency-invalid", "API idempotency is not an Idempotency";
    API_PREFIX_OVERLAP: "api.prefix_overlap", area 7, Error, Agent,
        "07-http.html#api-prefix-overlap", "Two APIs share a prefix";
    API_PREFIX_REQUIRED: "api.prefix_required", area 7, Error, Agent,
        "07-http.html#api-prefix-required", "Declared API has no prefix";
    API_RECORD_OUT_OF_DATE: "api.record_out_of_date", area 7, Error, Agent,
        "07-http.html#api-record-out-of-date", "API record disagrees with the routes";
    API_SETTING_UNKNOWN: "api.setting_unknown", area 7, Error, Agent,
        "07-http.html#api-setting-unknown", "API setting is not one an API takes";
    CLI_API_ARGUMENTS: "cli.api_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Api command has invalid arguments";
    CLI_CHECK_ARGUMENTS: "cli.check_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Check command has invalid arguments";
    CLI_COMPLETIONS_ARGUMENTS: "cli.completions_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Completions command has invalid arguments";
    CLI_CONFIG_ARGUMENTS: "cli.config_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Config command has invalid arguments";
    CLI_CONFIG_PROMPT_FAILED: "cli.config_prompt_failed", area 14, Error, Human,
        "14-tooling.html#command-errors", "Secure configuration prompt failed";
    CLI_CONFIG_TTY_REQUIRED: "cli.config_tty_required", area 14, Error, Human,
        "14-tooling.html#command-errors", "Configuration entry requires an interactive terminal";
    CLI_DEV_ARGUMENTS: "cli.dev_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Dev command has invalid arguments";
    CLI_DEV_PORT_INVALID: "cli.dev_port_invalid", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Development port is invalid";
    CLI_DEV_START_FAILED: "cli.dev_start_failed", area 14, Error, Human,
        "14-tooling.html#command-errors", "Development server could not start";
    CLI_FMT_ARGUMENTS: "cli.fmt_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Format command has invalid arguments";
    CLI_HELP_ARGUMENTS: "cli.help_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Help command has invalid arguments";
    CLI_LSP_ARGUMENTS: "cli.lsp_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Language-server command has invalid arguments";
    CLI_MIGRATE_ARGUMENTS: "cli.migrate_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Migrate command has invalid arguments";
    CLI_PRESENTATION_ARGUMENTS: "cli.presentation_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Terminal presentation option is invalid";
    CLI_ROUTES_ARGUMENTS: "cli.routes_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Routes command has invalid arguments";
    CLI_SQL_ARGUMENTS: "cli.sql_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Sql command has invalid arguments";
    CLI_TEST_ARGUMENTS: "cli.test_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Test command has invalid arguments";
    CLI_UNKNOWN_COMMAND: "cli.unknown_command", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Unknown Jadpo command";
    CLI_UNKNOWN_FLAG: "cli.unknown_flag", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Unknown flag";
    CLI_VERSION_ARGUMENTS: "cli.version_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Version command has invalid arguments";
    CONFIGURATION_BINDING_EMPTY: "configuration.binding_empty", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-binding-empty", "Setting names an empty environment variable";
    CONFIGURATION_BINDING_INVALID: "configuration.binding_invalid", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-binding-invalid", "Setting names a variable that is not in capitals";
    CONFIGURATION_CLIENT_ADDRESS_DUPLICATE: "configuration.client_address_duplicate", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-client-address-duplicate", "Client address is declared twice";
    CONFIGURATION_CLIENT_ADDRESS_HOPS_INVALID: "configuration.client_address_hops_invalid", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-client-address-hops-invalid", "Client address counts proxies with something that is not a count from 1";
    CONFIGURATION_CLIENT_ADDRESS_HOPS_UNUSED: "configuration.client_address_hops_unused", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-client-address-hops-unused", "Client address gives a count of proxies to a source that counts none";
    CONFIGURATION_CLIENT_ADDRESS_INVALID: "configuration.client_address_invalid", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-client-address-invalid", "Client address is not a block of a source and a count of proxies";
    CONFIGURATION_CLIENT_ADDRESS_SOURCE_UNKNOWN: "configuration.client_address_source_unknown", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-client-address-source-unknown", "Client address names a source that is not one of the four";
    CONFIGURATION_DEFAULT_INVALID: "configuration.default_invalid", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-default-invalid", "Setting value breaks its type's rules";
    CONFIGURATION_DEFAULT_LITERAL_REQUIRED: "configuration.default_literal_required", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-default-literal-required", "Setting value is not a literal";
    CONFIGURATION_DEFAULT_TYPE: "configuration.default_type", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-default-type", "Setting value is the wrong kind of value for its type";
    CONFIGURATION_DUPLICATE_BINDING: "configuration.duplicate_binding", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-duplicate-binding", "Two settings read the same environment variable";
    CONFIGURATION_DUPLICATE_OPTION: "configuration.duplicate_option", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-duplicate-option", "Setting option is given twice";
    CONFIGURATION_EXPECTED_FIELD: "configuration.expected_field", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-expected-field", "Configuration holds something that is not a setting";
    CONFIGURATION_FIELD_UNKNOWN: "configuration.field_unknown", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-field-unknown", "Configuration has no setting with this name";
    CONFIGURATION_LOCAL_CHANGED: "configuration.local_changed", area 14, Error, Human,
        "14-tooling.html#command-errors", "Local configuration changed during the update";
    CONFIGURATION_LOCAL_INVALID_ENCODING: "configuration.local_invalid_encoding", area 14, Error, Human,
        "14-tooling.html#command-errors", "Local configuration is not UTF-8 text";
    CONFIGURATION_LOCAL_READ_FAILED: "configuration.local_read_failed", area 14, Error, Human,
        "14-tooling.html#command-errors", "Local configuration could not be read";
    CONFIGURATION_LOCAL_SYNTAX: "configuration.local_syntax", area 14, Error, Human,
        "14-tooling.html#command-errors", "Local configuration syntax is invalid";
    CONFIGURATION_LOCAL_UNSAFE_TARGET: "configuration.local_unsafe_target", area 14, Error, Human,
        "14-tooling.html#command-errors", "Local configuration target is unsafe";
    CONFIGURATION_LOCAL_WRITE_FAILED: "configuration.local_write_failed", area 14, Error, Human,
        "14-tooling.html#command-errors", "Local configuration could not be saved";
    CONFIGURATION_MULTIPLE_DECLARATIONS: "configuration.multiple_declarations", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-multiple-declarations", "Configuration is declared more than once";
    CONFIGURATION_SECRET_DEFAULT: "configuration.secret_default", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-secret-default", "Secret setting has a value in source";
    CONFIGURATION_SECRET_FLOW: "configuration.secret_flow", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-secret-flow", "Secret setting is used where it could leave the server";
    CONFIGURATION_SECRET_INVALID: "configuration.secret_invalid", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-secret-invalid", "Secret option is not true or false";
    CONFIGURATION_TEST_VALUE_MISSING: "configuration.test_value_missing", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-test-value-missing", "Setting read by a test has no value in tests";
    CONFIGURATION_TYPE_UNSUPPORTED: "configuration.type_unsupported", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-type-unsupported", "Setting type cannot be read from one variable";
    CONFIGURATION_UNKNOWN_OPTION: "configuration.unknown_option", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-unknown-option", "Setting option is not one a setting takes";
    CONFIGURATION_URL_TYPE: "configuration.url_type", area 10, Error, Agent,
        "10-configuration-and-time.html#configuration-url-type", "Connection address is not a setting of the type its driver needs";
    CONFIGURATION_VALUE_INVALID: "configuration.value_invalid", area 14, Error, Human,
        "14-tooling.html#command-errors", "Configuration value is invalid";
    CONFIGURATION_VALUE_MISSING: "configuration.value_missing", area 14, Error, Human,
        "14-tooling.html#command-errors", "Required configuration is missing";
    DATA_CLEARED_REQUIRED_REFERENCE: "data.cleared_required_reference", area 5, Error, Agent,
        "05-data.html#data-cleared-required-reference", "A reference that can never be `none` is cleared on delete";
    DATA_CONFLICT_CANNOT_BREAK: "data.conflict_cannot_break", area 6, Error, Agent,
        "06-reads-and-writes.html#data-conflict-cannot-break", "Conflict mapping for a rule the write can't break";
    DATA_CONFLICT_UNKNOWN: "data.conflict_unknown", area 6, Error, Agent,
        "06-reads-and-writes.html#data-conflict-unknown", "Conflict mapping names no unique rule or reference";
    DATA_CONFLICT_UNMAPPED: "data.conflict_unmapped", area 6, Error, Agent,
        "06-reads-and-writes.html#data-conflict-unmapped", "Write can break a rule its `conflict:` doesn't map";
    DATA_DUPLICATE_RULE: "data.duplicate_rule", area 5, Error, Agent,
        "05-data.html#data-duplicate-rule", "Unique rule or index repeats another";
    DATA_ENTITY_IN_FIELD: "data.entity_in_field", area 5, Error, Agent,
        "05-data.html#data-entity-in-field", "Field holds an entity instead of a reference to one";
    DATA_FIELD_LIST_REQUIRED: "data.field_list_required", area 5, Error, Agent,
        "05-data.html#data-field-list-required", "Unique rule or index is not a list of fields";
    DATA_GENERATED_TYPE: "data.generated_type", area 5, Error, Agent,
        "05-data.html#data-generated-type", "Generated field is not a required `Instant`";
    DATA_IDENTITY_MISSING: "data.identity_missing", area 5, Error, Agent,
        "05-data.html#data-identity-missing", "Entity has no `id` field";
    DATA_IDENTITY_TYPE: "data.identity_type", area 5, Error, Agent,
        "05-data.html#data-identity-type", "Entity's `id` is not a required `Uuid`";
    DATA_LIFECYCLE_CONDITION: "data.lifecycle_condition", area 5, Error, Agent,
        "05-data.html#data-lifecycle-condition", "Lifecycle condition isn't a test of the record's own fields";
    DATA_LIFECYCLE_HARD_DELETE: "data.lifecycle_hard_delete", area 6, Error, Human,
        "06-reads-and-writes.html#data-lifecycle-hard-delete", "Delete would remove a record its lifecycle keeps";
    DATA_LIFECYCLE_INITIAL_REQUIRED: "data.lifecycle_initial_required", area 5, Error, Agent,
        "05-data.html#data-lifecycle-initial-required", "Required field a transition sets has no starting value";
    DATA_LIFECYCLE_OWNED_FIELD: "data.lifecycle_owned_field", area 6, Error, Agent,
        "06-reads-and-writes.html#data-lifecycle-owned-field", "Write sets a field only the lifecycle changes";
    DATA_LIFECYCLE_PURGE_AFTER: "data.lifecycle_purge_after", area 5, Error, Agent,
        "05-data.html#data-lifecycle-purge-after", "Purge's `after:` isn't a positive duration";
    DATA_LIFECYCLE_PURGE_SINCE: "data.lifecycle_purge_since", area 5, Error, Agent,
        "05-data.html#data-lifecycle-purge-since", "Purge's `since:` isn't a time field of the record";
    DATA_LIFECYCLE_PURGE_VISIBLE: "data.lifecycle_purge_visible", area 5, Error, Agent,
        "05-data.html#data-lifecycle-purge-visible", "Lifecycle purges records but hides none";
    DATA_LIFECYCLE_SECTION: "data.lifecycle_section", area 5, Error, Agent,
        "05-data.html#data-lifecycle-section", "Lifecycle has a section it doesn't take, or lacks one it needs";
    DATA_LIFECYCLE_TRANSITION_NAME: "data.lifecycle_transition_name", area 5, Error, Agent,
        "05-data.html#data-lifecycle-transition-name", "Transition is named like another operation or a field";
    DATA_LIFECYCLE_UNKNOWN_FIELD: "data.lifecycle_unknown_field", area 5, Error, Agent,
        "05-data.html#data-lifecycle-unknown-field", "Lifecycle names a field the entity doesn't have";
    DATA_LIFECYCLE_VALUE: "data.lifecycle_value", area 5, Error, Agent,
        "05-data.html#data-lifecycle-value", "Lifecycle sets a value that isn't fixed";
    DATA_NAME_TOO_LONG: "data.name_too_long", area 5, Error, Agent,
        "05-data.html#data-name-too-long", "Name is too long for a table or column";
    DATA_ON_DELETE_NOT_REFERENCE: "data.on_delete_not_reference", area 5, Error, Agent,
        "05-data.html#data-on-delete-not-reference", "Field that is not a reference sets `on_delete`";
    DATA_ON_DELETE_REQUIRED: "data.on_delete_required", area 5, Error, Human,
        "05-data.html#data-on-delete-required", "Reference does not say what deleting its record does";
    DATA_PATCH_FIELD_REQUIRED: "data.patch_field_required", area 6, Error, Agent,
        "06-reads-and-writes.html#data-patch-field-required", "Patch input has a field that isn't optional";
    DATA_PATCH_FIELD_UNKNOWN: "data.patch_field_unknown", area 6, Error, Agent,
        "06-reads-and-writes.html#data-patch-field-unknown", "Patch names a field the entity or its input doesn't have";
    DATA_PATCH_NOT_INPUT: "data.patch_not_input", area 6, Error, Agent,
        "06-reads-and-writes.html#data-patch-not-input", "Patch given something other than an `Input`";
    DATA_PATCH_OVERLAP: "data.patch_overlap", area 6, Error, Agent,
        "06-reads-and-writes.html#data-patch-overlap", "Supplied-field block sets a field the patch already sets";
    DATA_READ_IN_LOOP: "data.read_in_loop", area 6, Error, Agent,
        "06-reads-and-writes.html#data-read-in-loop", "Query or `get` inside a `for` loop";
    DATA_REFERENCE_ACROSS_STORES: "data.reference_across_stores", area 5, Error, Human,
        "05-data.html#data-reference-across-stores", "Reference points at an entity in another store";
    DATA_REFERENCE_CYCLE: "data.reference_cycle", area 5, Error, Human,
        "05-data.html#data-reference-cycle", "Required references form a cycle";
    DATA_REPEATED_FIELD: "data.repeated_field", area 5, Error, Agent,
        "05-data.html#data-repeated-field", "Unique rule or index lists a field twice";
    DATA_RESERVED_TABLE_NAME: "data.reserved_table_name", area 5, Error, Agent,
        "05-data.html#data-reserved-table-name", "Entity's table name starts with `jadpo_`";
    DATA_SET_GENERATED_FIELD: "data.set_generated_field", area 6, Error, Agent,
        "06-reads-and-writes.html#data-set-generated-field", "Write sets a field the store generates";
    DATA_STORE_REQUIRED: "data.store_required", area 5, Error, Human,
        "05-data.html#data-store-required", "Entity names no store and there is no `main`";
    DATA_STORE_UNKNOWN: "data.store_unknown", area 5, Error, Agent,
        "05-data.html#data-store-unknown", "Entity's store is not a database connection";
    DATA_STORED_DECIMAL_SCALE: "data.stored_decimal_scale", area 6, Error, Agent,
        "06-reads-and-writes.html#data-stored-decimal-scale", "Update by `id` adds a decimal with more places than its field keeps";
    DATA_STORED_VALUE_OUTSIDE_UPDATE: "data.stored_value_outside_update", area 6, Error, Agent,
        "06-reads-and-writes.html#data-stored-value-outside-update", "Stored field read outside an update by `id`";
    DATA_STORED_VALUE_UNPROVEN: "data.stored_value_unproven", area 6, Error, Human,
        "06-reads-and-writes.html#data-stored-value-unproven", "Update by `id` builds a value from a stored field that could break its type's rules";
    DATA_UNIQUE_NULLABLE_FIELD: "data.unique_nullable_field", area 5, Error, Agent,
        "05-data.html#data-unique-nullable-field", "Unique rule includes a field that can be `none`";
    DATA_UNIQUE_SINGLE_FIELD: "data.unique_single_field", area 5, Error, Agent,
        "05-data.html#data-unique-single-field", "Unique rule has only one field";
    DATA_UNKNOWN_FIELD: "data.unknown_field", area 5, Error, Agent,
        "05-data.html#data-unknown-field", "Unique rule or index names a field the entity does not have";
    DATA_UNKNOWN_SECTION: "data.unknown_section", area 5, Error, Agent,
        "05-data.html#data-unknown-section", "Entity has a section Jadpo does not know";
    DATA_UPDATE_BY_ID_VALUE: "data.update_by_id_value", area 6, Error, Agent,
        "06-reads-and-writes.html#data-update-by-id-value", "Update by `id` sets a value from something it can't use";
    DATA_WRITE_NOT_ENTITY: "data.write_not_entity", area 6, Error, Agent,
        "06-reads-and-writes.html#data-write-not-entity", "Write names a type that isn't an entity";
    DATA_WRITE_SECTION_REQUIRED: "data.write_section_required", area 6, Error, Agent,
        "06-reads-and-writes.html#data-write-section-required", "Write doesn't say what it changes";
    DATA_WRITE_SECTION_UNEXPECTED: "data.write_section_unexpected", area 6, Error, Agent,
        "06-reads-and-writes.html#data-write-section-unexpected", "Write has a section it doesn't take";
    EFFECT_FUNCTION_CALLS_ACTION: "effect.function_calls_action", area 3, Error, Agent,
        "03-logic.html#effect-function-calls-action", "Function calls an action or query";
    EFFECT_FUNCTION_PERSISTENCE: "effect.function_persistence", area 3, Error, Agent,
        "03-logic.html#effect-function-persistence", "Function reads or writes stored data";
    EFFECT_RECURSIVE_ACTION: "effect.recursive_action", area 3, Error, Human,
        "03-logic.html#effect-recursive-action", "Action is part of a call cycle";
    FAILURE_ATTEMPT_INFALLIBLE: "failure.attempt_infallible", area 3, Error, Agent,
        "03-logic.html#failure-attempt-infallible", "Attempt is used on something that cannot fail";
    FAILURE_ATTEMPT_REQUIRED: "failure.attempt_required", area 3, Error, Agent,
        "03-logic.html#failure-attempt-required", "Call that can fail is not handled";
    FAILURE_CODE_FORMAT: "failure.code_format", area 4, Error, Agent,
        "04-failures.html#failure-code-format", "Failure code is not lower_snake_case";
    FAILURE_CONTEXT_FIELD_OVERLAP: "failure.context_field_overlap", area 4, Error, Agent,
        "04-failures.html#failure-context-field-overlap", "Field is declared for both the client and the log";
    FAILURE_DUPLICATE_CODE: "failure.duplicate_code", area 4, Error, Human,
        "04-failures.html#failure-duplicate-code", "Failure code is already in use";
    FAILURE_DUPLICATE_CONTEXT_FIELD: "failure.duplicate_context_field", area 4, Error, Agent,
        "04-failures.html#failure-duplicate-context-field", "Failure field is given twice";
    FAILURE_DUPLICATE_DECLARATION: "failure.duplicate_declaration", area 4, Error, Agent,
        "04-failures.html#failure-duplicate-declaration", "Failure is listed twice after `fails`";
    FAILURE_DUPLICATE_MAPPING: "failure.duplicate_mapping", area 4, Error, Agent,
        "04-failures.html#failure-duplicate-mapping", "Condition is mapped twice";
    FAILURE_INVALID_COVERS_NOTHING: "failure.invalid_covers_nothing", area 4, Error, Agent,
        "04-failures.html#failure-invalid-covers-nothing", "Catch-all covers no rule, because every rule that can break has an arm";
    FAILURE_INVALID_OUTSIDE_CONSTRUCTION: "failure.invalid_outside_construction", area 4, Error, Agent,
        "04-failures.html#failure-invalid-outside-construction", "Catch-all for rules is used on something that is not a construction";
    FAILURE_KIND_REQUIRED: "failure.kind_required", area 4, Error, Agent,
        "04-failures.html#failure-kind-required", "Failure has no kind";
    FAILURE_KIND_RUNTIME_ONLY: "failure.kind_runtime_only", area 4, Error, Agent,
        "04-failures.html#failure-kind-runtime-only", "Failure kind is raised only by the runtime";
    FAILURE_MAPPING_KIND: "failure.mapping_kind", area 4, Error, Agent,
        "04-failures.html#failure-mapping-kind", "Mapped failure's kind does not fit its condition";
    FAILURE_MISSING_CONTEXT_FIELD: "failure.missing_context_field", area 4, Error, Agent,
        "04-failures.html#failure-missing-context-field", "Rejected failure is missing a field";
    FAILURE_OUTCOME_DUPLICATE_ARM: "failure.outcome_duplicate_arm", area 3, Error, Agent,
        "03-logic.html#failure-outcome-duplicate-arm", "Outcome match handles a failure twice";
    FAILURE_OUTCOME_INFALLIBLE: "failure.outcome_infallible", area 3, Error, Agent,
        "03-logic.html#failure-outcome-infallible", "Outcome match is on a call that cannot fail";
    FAILURE_OUTCOME_MISSING_ARM: "failure.outcome_missing_arm", area 3, Error, Agent,
        "03-logic.html#failure-outcome-missing-arm", "Outcome match does not handle every failure";
    FAILURE_OUTCOME_PATTERN_INVALID: "failure.outcome_pattern_invalid", area 3, Error, Agent,
        "03-logic.html#failure-outcome-pattern-invalid", "Outcome match arm has a pattern for values";
    FAILURE_OUTCOME_SUBJECT_REQUIRED: "failure.outcome_subject_required", area 3, Error, Agent,
        "03-logic.html#failure-outcome-subject-required", "Outcome match is not on a call";
    FAILURE_OUTCOME_SUCCESS_DUPLICATE: "failure.outcome_success_duplicate", area 3, Error, Agent,
        "03-logic.html#failure-outcome-success-duplicate", "Outcome match repeats its success arm";
    FAILURE_OUTCOME_SUCCESS_MISSING: "failure.outcome_success_missing", area 3, Error, Agent,
        "03-logic.html#failure-outcome-success-missing", "Outcome match has no success arm";
    FAILURE_OUTCOME_SUCCESS_VALUE_REQUIRED: "failure.outcome_success_value_required", area 3, Error, Agent,
        "03-logic.html#failure-outcome-success-value-required", "Success arm passes on a failure";
    FAILURE_OUTCOME_UNKNOWN_ARM: "failure.outcome_unknown_arm", area 3, Error, Agent,
        "03-logic.html#failure-outcome-unknown-arm", "Outcome arm names a failure the call cannot produce";
    FAILURE_OUTCOME_WILDCARD: "failure.outcome_wildcard", area 3, Error, Agent,
        "03-logic.html#failure-outcome-wildcard", "Outcome match uses a wildcard";
    FAILURE_PROPAGATE_PLACEMENT: "failure.propagate_placement", area 3, Error, Agent,
        "03-logic.html#failure-propagate-placement", "Propagate is used outside a failure arm";
    FAILURE_REJECT_PLACEMENT: "failure.reject_placement", area 3, Error, Agent,
        "03-logic.html#failure-reject-placement", "Reject is used inside an expression";
    FAILURE_RULE_CANNOT_BREAK: "failure.rule_cannot_break", area 4, Error, Agent,
        "04-failures.html#failure-rule-cannot-break", "Arm maps a rule the value cannot break";
    FAILURE_RULE_UNMAPPED: "failure.rule_unmapped", area 4, Error, Agent,
        "04-failures.html#failure-rule-unmapped", "Rule that can break has no arm and no catch-all";
    FAILURE_STALE_DECLARATION: "failure.stale_declaration", area 4, Error, Agent,
        "04-failures.html#failure-stale-declaration", "Listed failure can never happen";
    FAILURE_UNDECLARED_PROPAGATION: "failure.undeclared_propagation", area 4, Error, Agent,
        "04-failures.html#failure-undeclared-propagation", "Failure that can leave is not listed after `fails`";
    FAILURE_UNKNOWN_CONTEXT_FIELD: "failure.unknown_context_field", area 4, Error, Agent,
        "04-failures.html#failure-unknown-context-field", "Rejected failure has no such field";
    FAILURE_UNKNOWN_MAPPING: "failure.unknown_mapping", area 4, Error, Agent,
        "04-failures.html#failure-unknown-mapping", "Mapping names a condition the operation does not have";
    FORMAT_CHANGES_REQUIRED: "format.changes_required", area 14, Error, Agent,
        "14-tooling.html#formatting", "Source file is not formatted";
    FORMAT_UNSAFE: "format.unsafe", area 14, Error, Human,
        "14-tooling.html#format-unsafe", "Formatting would change the file's meaning";
    FORMAT_WRITE_FAILED: "format.write_failed", area 14, Error, Human,
        "14-tooling.html#formatting", "Formatted source could not be written";
    IMPORT_DUPLICATE: "import.duplicate", area 1, Error, Agent,
        "01-project-layout-and-modules.html#import-duplicate", "Name or folder imported twice";
    IMPORT_MISSING: "import.missing", area 1, Error, Agent,
        "01-project-layout-and-modules.html#import-missing", "Name used without its import";
    IMPORT_UNKNOWN: "import.unknown", area 1, Error, Agent,
        "01-project-layout-and-modules.html#import-unknown", "Import names nothing in its folder";
    IMPORT_UNNEEDED: "import.unneeded", area 1, Error, Agent,
        "01-project-layout-and-modules.html#import-unneeded", "Import of a name the file sees without one";
    IMPORT_UNUSED: "import.unused", area 1, Error, Agent,
        "01-project-layout-and-modules.html#import-unused", "Imported name the file does not use";
    LAYOUT_CALL_DIRECTION: "layout.call_direction", area 1, Error, Human,
        "01-project-layout-and-modules.html#layout-call-direction", "Shared code depends on a domain";
    LAYOUT_FILE_NAME_MISMATCH: "layout.file_name_mismatch", area 1, Error, Agent,
        "01-project-layout-and-modules.html#layout-file-name-mismatch", "File name does not match its declaration";
    LAYOUT_PROMOTION_REQUIRED: "layout.promotion_required", area 1, Error, Agent,
        "01-project-layout-and-modules.html#layout-promotion-required", "Declaration is used beyond the file that holds it";
    LAYOUT_ROUTE_PREFIX_MISMATCH: "layout.route_prefix_mismatch", area 1, Error, Agent,
        "01-project-layout-and-modules.html#layout-route-prefix-mismatch", "Route group prefix does not match its file name";
    LAYOUT_TEST_WITHOUT_SUBJECT: "layout.test_without_subject", area 1, Error, Agent,
        "01-project-layout-and-modules.html#layout-test-without-subject", "Test file has no subject beside it";
    LAYOUT_UNKNOWN_LOCATION: "layout.unknown_location", area 1, Error, Agent,
        "01-project-layout-and-modules.html#layout-unknown-location", "Jadpo file is outside the fixed layout";
    LAYOUT_UNUSED_DECLARATION: "layout.unused_declaration", area 1, Error, Agent,
        "01-project-layout-and-modules.html#layout-unused-declaration", "Shared declaration is not used";
    LAYOUT_WRONG_FOLDER: "layout.wrong_folder", area 1, Error, Agent,
        "01-project-layout-and-modules.html#layout-wrong-folder", "Declaration is in the wrong folder";
    LIBRARY_SIMILAR_EXISTS: "library.similar_exists", area 1, Warning, Agent,
        "01-project-layout-and-modules.html#library-similar-exists", "A library function already does the same";
    MIGRATION_APPLY_FAILED: "migration.apply_failed", area 14, Error, Human,
        "14-tooling.html#command-errors", "Migrations could not be applied";
    MIGRATION_DECISION_INVALID: "migration.decision_invalid", area 5, Error, Agent,
        "05-data.html#migration-decision-invalid", "Migration decision does not fit its change";
    MIGRATION_DECISION_REQUIRED: "migration.decision_required", area 5, Error, Human,
        "05-data.html#migration-decision-required", "Migration needs a decision";
    MIGRATION_DECISION_UNKNOWN: "migration.decision_unknown", area 5, Error, Agent,
        "05-data.html#migration-decision-unknown", "Migration decides about something that is not changing";
    MIGRATION_OUT_OF_DATE: "migration.out_of_date", area 5, Error, Agent,
        "05-data.html#migration-out-of-date", "Entities and the latest migration disagree";
    MIGRATION_RECORD_INVALID: "migration.record_invalid", area 5, Error, Human,
        "05-data.html#migration-record-invalid", "Migration folder cannot be read";
    MIGRATION_STORE_DIVERGED: "migration.store_diverged", area 14, Error, Human,
        "14-tooling.html#command-errors", "Database does not match the project's migrations";
    MIGRATION_UNFINISHED: "migration.unfinished", area 5, Error, Agent,
        "05-data.html#migration-unfinished", "Latest migration is not finished";
    NAMING_FOLDED_COLLISION: "naming.folded_collision", area 1, Error, Agent,
        "01-project-layout-and-modules.html#naming-folded-collision", "Two names fold to the same file name";
    NAMING_GENERIC_NAME: "naming.generic_name", area 1, Error, Human,
        "01-project-layout-and-modules.html#naming-generic-name", "Domain or library has a generic name";
    NAMING_ROUND_TRIP: "naming.round_trip", area 1, Error, Agent,
        "01-project-layout-and-modules.html#naming-round-trip", "Name does not survive conversion to its file name";
    POLICY_BINDING_INVALID: "policy.binding_invalid", area 9, Error, Agent,
        "09-policy.html#policy-binding-invalid", "Policy names something that can't say who";
    POLICY_DECLARATION_DUPLICATE: "policy.declaration_duplicate", area 9, Error, Agent,
        "09-policy.html#policy-declaration-duplicate", "Policy is declared twice";
    POLICY_EFFECT_DUPLICATE: "policy.effect_duplicate", area 9, Error, Agent,
        "09-policy.html#policy-effect-duplicate", "Policy gives the same list twice";
    POLICY_EFFECT_UNGRANTED: "policy.effect_ungranted", area 9, Error, Human,
        "09-policy.html#policy-effect-ungranted", "Route reaches something no grant allows";
    POLICY_EFFECT_UNKNOWN: "policy.effect_unknown", area 9, Error, Agent,
        "09-policy.html#policy-effect-unknown", "Policy holds something that isn't a grant or a setting";
    POLICY_FIELD_WIDENS_ENTITY: "policy.field_widens_entity", area 9, Error, Agent,
        "09-policy.html#policy-field-widens-entity", "Field policy lets in someone its record's grants don't";
    POLICY_MEMBERSHIP_DUPLICATE: "policy.membership_duplicate", area 9, Error, Agent,
        "09-policy.html#policy-membership-duplicate", "Entity declares a membership twice, or gives one of its parts twice";
    POLICY_MEMBERSHIP_MEMBER_REQUIRED: "policy.membership_member_required", area 9, Error, Agent,
        "09-policy.html#policy-membership-member-required", "Membership doesn't say who holds it";
    POLICY_MEMBERSHIP_ROLE_REQUIRED: "policy.membership_role_required", area 9, Error, Agent,
        "09-policy.html#policy-membership-role-required", "Membership doesn't say which field holds the role";
    POLICY_MEMBERSHIP_SCOPE_REQUIRED: "policy.membership_scope_required", area 9, Error, Agent,
        "09-policy.html#policy-membership-scope-required", "Membership doesn't say what it is held in";
    POLICY_OUTPUT_FIELD_UNPROVED: "policy.output_field_unproved", area 9, Error, Agent,
        "09-policy.html#policy-output-field-unproved", "Output carries a value not every caller may read";
    POLICY_PUBLIC_GRANT_MISSING: "policy.public_grant_missing", area 9, Error, Human,
        "09-policy.html#policy-public-grant-missing", "Route anyone may call reaches what not everyone may do";
    POLICY_ROLE_FIELD_CREATE_FORBIDDEN: "policy.role_field_create_forbidden", area 9, Error, Agent,
        "09-policy.html#policy-role-field-create-forbidden", "Create sets the user field its grant needs from the request, not the caller";
    POLICY_ROLE_UNBOUND: "policy.role_unbound", area 9, Error, Agent,
        "09-policy.html#policy-role-unbound", "Grant names a role no membership holds";
    POLICY_SCOPE_AMBIGUOUS: "policy.scope_ambiguous", area 9, Error, Agent,
        "09-policy.html#policy-scope-ambiguous", "Grant doesn't say which reference a role or the tenant applies through";
    POLICY_SCOPE_DUPLICATE: "policy.scope_duplicate", area 9, Error, Agent,
        "09-policy.html#policy-scope-duplicate", "A scope or tenant is given twice";
    POLICY_SCOPE_MISSING: "policy.scope_missing", area 9, Error, Agent,
        "09-policy.html#policy-scope-missing", "Entity has no reference a role or the tenant applies through";
    POLICY_SUBJECT_DUPLICATE: "policy.subject_duplicate", area 9, Error, Agent,
        "09-policy.html#policy-subject-duplicate", "Grant names the same caller twice";
    QUERY_CONDITION_UNSUPPORTED: "query.condition_unsupported", area 6, Error, Agent,
        "06-reads-and-writes.html#query-condition-unsupported", "Query condition is not one an index can serve";
    QUERY_FROM_NOT_ENTITY: "query.from_not_entity", area 6, Error, Agent,
        "06-reads-and-writes.html#query-from-not-entity", "Query reads something that is not an entity";
    QUERY_INCLUDE_DEPTH: "query.include_depth", area 6, Error, Human,
        "06-reads-and-writes.html#query-include-depth", "Includes nest more than two levels";
    QUERY_INCLUDE_RESULT: "query.include_result", area 6, Error, Agent,
        "06-reads-and-writes.html#query-include-result", "Result type has no field of the right shape for an include";
    QUERY_INCLUDE_UNKNOWN: "query.include_unknown", area 6, Error, Agent,
        "06-reads-and-writes.html#query-include-unknown", "Include names no reference to or from the entity";
    QUERY_LIMIT_RANGE: "query.limit_range", area 6, Error, Agent,
        "06-reads-and-writes.html#query-limit-range", "Query limit or page size is below 1";
    QUERY_LIMIT_UNBOUNDED: "query.limit_unbounded", area 6, Error, Agent,
        "06-reads-and-writes.html#query-limit-unbounded", "Query limit or page size comes from a value with no maximum";
    QUERY_NOT_UNIQUE: "query.not_unique", area 6, Error, Agent,
        "06-reads-and-writes.html#query-not-unique", "Query for one record can match several";
    QUERY_OPTIONAL_VALUE: "query.optional_value", area 6, Error, Agent,
        "06-reads-and-writes.html#query-optional-value", "Condition compares with a value that may be absent, or `where_present:` with one that is always there";
    QUERY_ORDER_DIRECTION: "query.order_direction", area 6, Error, Agent,
        "06-reads-and-writes.html#query-order-direction", "Order field has no `asc` or `desc`";
    QUERY_ORDER_FIELD: "query.order_field", area 6, Error, Agent,
        "06-reads-and-writes.html#query-order-field", "Field cannot be in a query's order";
    QUERY_OTHER_DOMAIN: "query.other_domain", area 6, Error, Human,
        "06-reads-and-writes.html#query-other-domain", "Query reads another domain's entity";
    QUERY_PAGE_SETTING: "query.page_setting", area 6, Error, Agent,
        "06-reads-and-writes.html#query-page-setting", "Page block is missing a setting or has a wrong one";
    QUERY_REPEATED_SECTION: "query.repeated_section", area 6, Error, Agent,
        "06-reads-and-writes.html#query-repeated-section", "Query section is given twice";
    QUERY_RESULT_FIELD: "query.result_field", area 6, Error, Agent,
        "06-reads-and-writes.html#query-result-field", "Result type has a field the entity cannot fill";
    QUERY_RESULT_TYPE: "query.result_type", area 6, Error, Agent,
        "06-reads-and-writes.html#query-result-type", "Query result is not the entity or a type derived from it";
    QUERY_SECTION_REQUIRED: "query.section_required", area 6, Error, Agent,
        "06-reads-and-writes.html#query-section-required", "Query is missing a section its result needs";
    QUERY_SECTION_UNEXPECTED: "query.section_unexpected", area 6, Error, Agent,
        "06-reads-and-writes.html#query-section-unexpected", "Query has a section its result does not take";
    QUERY_UNKNOWN_FIELD: "query.unknown_field", area 6, Error, Agent,
        "06-reads-and-writes.html#query-unknown-field", "Query names a field its entity does not have";
    QUERY_UNKNOWN_SECTION: "query.unknown_section", area 6, Error, Agent,
        "06-reads-and-writes.html#query-unknown-section", "Query has a section Jadpo does not know";
    QUERY_VALUE_UNSUPPORTED: "query.value_unsupported", area 6, Error, Agent,
        "06-reads-and-writes.html#query-value-unsupported", "Query value is computed rather than a parameter, literal or enum value";
    ROUTE_API_UNKNOWN: "route.api_unknown", area 7, Error, Agent,
        "07-http.html#route-api-unknown", "Route group names an API the application doesn't declare";
    ROUTE_ARGUMENT_INVALID: "route.argument_invalid", area 7, Error, Agent,
        "07-http.html#route-argument-invalid", "Route passes something other than a placeholder, the input or their fields";
    ROUTE_AUTHENTICATION_INVALID: "route.authentication_invalid", area 7, Error, Human,
        "07-http.html#route-authentication-invalid", "Route authentication is not RouteAuthentication.none or .fresh";
    ROUTE_BINDING_UNUSED: "route.binding_unused", area 7, Error, Agent,
        "07-http.html#route-binding-unused", "Route reads a placeholder or input it never passes on";
    ROUTE_DEADLINE_INVALID: "route.deadline_invalid", area 7, Error, Agent,
        "07-http.html#route-deadline-invalid", "Route deadline is not a duration up to 60 seconds";
    ROUTE_HEADER_CREDENTIAL: "route.header_credential", area 7, Error, Agent,
        "07-http.html#route-header-credential", "Input field reads a header authentication owns";
    ROUTE_HEADER_DUPLICATE: "route.header_duplicate", area 7, Error, Agent,
        "07-http.html#route-header-duplicate", "Two input fields read the same header";
    ROUTE_HEADER_NAME_INVALID: "route.header_name_invalid", area 7, Error, Agent,
        "07-http.html#route-header-name-invalid", "Header name is not a valid HTTP header name";
    ROUTE_HEADER_NOT_SCALAR: "route.header_not_scalar", area 7, Error, Agent,
        "07-http.html#route-header-not-scalar", "Header field is not a single value";
    ROUTE_INPUT_NOT_INPUT: "route.input_not_input", area 7, Error, Agent,
        "07-http.html#route-input-not-input", "Route input is not an `Input` type";
    ROUTE_ITEM_DUPLICATE: "route.item_duplicate", area 7, Error, Agent,
        "07-http.html#route-item-duplicate", "Route item is given twice";
    ROUTE_ITEM_UNKNOWN: "route.item_unknown", area 7, Error, Agent,
        "07-http.html#route-item-unknown", "Route or route group item is not one it takes";
    ROUTE_OUTPUT_MISMATCH: "route.output_mismatch", area 7, Error, Agent,
        "07-http.html#route-output-mismatch", "Route output is not exactly what it runs returns";
    ROUTE_OUTPUT_NOT_OUTPUT: "route.output_not_output", area 7, Error, Agent,
        "07-http.html#route-output-not-output", "Route output is not an `Output` type";
    ROUTE_OUTPUT_REQUIRED: "route.output_required", area 7, Error, Agent,
        "07-http.html#route-output-required", "Route has no `output:`";
    ROUTE_OUTPUT_UNEXPECTED: "route.output_unexpected", area 7, Error, Agent,
        "07-http.html#route-output-unexpected", "A `no_content` route has an output";
    ROUTE_PATH_OVERLAP: "route.path_overlap", area 7, Error, Agent,
        "07-http.html#route-path-overlap", "Two routes could answer the same request";
    ROUTE_PLACEHOLDER_DUPLICATE: "route.placeholder_duplicate", area 7, Error, Agent,
        "07-http.html#route-placeholder-duplicate", "Placeholder name is used twice in a route's path";
    ROUTE_PLACEHOLDER_TYPE: "route.placeholder_type", area 7, Error, Agent,
        "07-http.html#route-placeholder-type", "Placeholder is not typed with a named scalar or an `X.id`";
    ROUTE_PREFIX_API: "route.prefix_api", area 7, Error, Agent,
        "07-http.html#route-prefix-api", "Route group prefix does not start with its API's prefix";
    ROUTE_PREFIX_REQUIRED: "route.prefix_required", area 7, Error, Agent,
        "07-http.html#route-prefix-required", "Route group has no prefix";
    ROUTE_QUERY_FIELD_UNSUPPORTED: "route.query_field_unsupported", area 7, Error, Agent,
        "07-http.html#route-query-field-unsupported", "Query-string input field is not a scalar, an enum or a list of them";
    ROUTE_RUN_INVALID: "route.run_invalid", area 7, Error, Agent,
        "07-http.html#route-run-invalid", "Route `run:` is not a call of a named action or query";
    ROUTE_RUN_KIND: "route.run_kind", area 7, Error, Agent,
        "07-http.html#route-run-kind", "A `GET` route runs a query and other methods run an action";
    ROUTE_RUN_REQUIRED: "route.run_required", area 7, Error, Agent,
        "07-http.html#route-run-required", "Route does not say what it runs";
    ROUTE_STATUS_INVALID: "route.status_invalid", area 7, Error, Agent,
        "07-http.html#route-status-invalid", "Route status is not a `SuccessStatus`";
    SEMANTIC_AUTH_METHOD_DUPLICATE: "semantic.auth_method_duplicate", area 8, Error, Agent,
        "08-identity.html#semantic-auth-method-duplicate", "Sign-in method is declared twice";
    SEMANTIC_AUTH_METHOD_UNKNOWN: "semantic.auth_method_unknown", area 8, Error, Agent,
        "08-identity.html#semantic-auth-method-unknown", "Sign-in method is not one the language has";
    SEMANTIC_AUTH_NOT_SET_UP: "semantic.auth_not_set_up", area 8, Error, Human,
        "08-identity.html#semantic-auth-not-set-up", "Routes need a signed-in caller, but nothing signs anyone in";
    SEMANTIC_AUTH_PRINCIPAL_REQUIRED: "semantic.auth_principal_required", area 8, Error, Agent,
        "08-identity.html#semantic-auth-principal-required", "Sign-in method has no principal variant to sign in";
    SEMANTIC_AUTH_SETTING_INVALID: "semantic.auth_setting_invalid", area 8, Error, Agent,
        "08-identity.html#semantic-auth-setting-invalid", "Sign-in setting has a value it can't take";
    SEMANTIC_AUTH_SETTING_REQUIRED: "semantic.auth_setting_required", area 8, Error, Agent,
        "08-identity.html#semantic-auth-setting-required", "Sign-in method is missing a setting it needs";
    SEMANTIC_AUTH_SETTING_UNKNOWN: "semantic.auth_setting_unknown", area 8, Error, Agent,
        "08-identity.html#semantic-auth-setting-unknown", "Sign-in setting is not one the method takes";
    SEMANTIC_DUPLICATE_DECLARATION: "semantic.duplicate_declaration", area 1, Error, Agent,
        "01-project-layout-and-modules.html#semantic-duplicate-declaration", "Name is declared more than once";
    SEMANTIC_MISSING_RETURN: "semantic.missing_return", area 3, Error, Agent,
        "03-logic.html#semantic-missing-return", "Callable can end without returning its result";
    SEMANTIC_MULTIPLE_APPLICATIONS: "semantic.multiple_applications", area 8, Error, Agent,
        "08-identity.html#semantic-multiple-applications", "Project declares more than one application";
    SEMANTIC_MULTIPLE_AUTHENTICATION: "semantic.multiple_authentication", area 8, Error, Agent,
        "08-identity.html#semantic-multiple-authentication", "Project declares more than one authentication block";
    SEMANTIC_MULTIPLE_LOCALE_DECLARATIONS: "semantic.multiple_locale_declarations", area 10, Error, Agent,
        "10-configuration-and-time.html#semantic-multiple-locale-declarations", "Locales are declared more than once";
    SEMANTIC_MULTIPLE_PRINCIPALS: "semantic.multiple_principals", area 8, Error, Agent,
        "08-identity.html#semantic-multiple-principals", "Project declares more than one principal";
    SEMANTIC_NAME_CASE: "semantic.name_case", area 1, Error, Agent,
        "01-project-layout-and-modules.html#semantic-name-case", "Name has the wrong case for its kind";
    SEMANTIC_NOT_CALLABLE: "semantic.not_callable", area 1, Error, Agent,
        "01-project-layout-and-modules.html#semantic-not-callable", "Name cannot be called";
    SEMANTIC_OWN_DOMAIN_QUALIFICATION: "semantic.own_domain_qualification", area 1, Error, Agent,
        "01-project-layout-and-modules.html#semantic-own-domain-qualification", "Own-domain call is qualified";
    SEMANTIC_PRINCIPAL_VARIANT_DUPLICATE: "semantic.principal_variant_duplicate", area 8, Error, Agent,
        "08-identity.html#semantic-principal-variant-duplicate", "Principal declares a variant twice";
    SEMANTIC_PRINCIPAL_VARIANT_INVALID: "semantic.principal_variant_invalid", area 8, Error, Agent,
        "08-identity.html#semantic-principal-variant-invalid", "Principal variant is not a subject and an optional active condition";
    SEMANTIC_PRINCIPAL_VARIANT_REQUIRED: "semantic.principal_variant_required", area 8, Error, Agent,
        "08-identity.html#semantic-principal-variant-required", "Principal declares neither a user nor a service";
    SEMANTIC_PRINCIPAL_VARIANT_UNKNOWN: "semantic.principal_variant_unknown", area 8, Error, Agent,
        "08-identity.html#semantic-principal-variant-unknown", "Principal variant is not user or service";
    SEMANTIC_REQUEST_INVALID: "semantic.request_invalid", area 13, Error, Agent,
        "13-testing.html#semantic-request-invalid", "Route test request doesn't fit its route";
    SEMANTIC_REQUEST_ROUTE_UNKNOWN: "semantic.request_route_unknown", area 13, Error, Agent,
        "13-testing.html#semantic-request-route-unknown", "No route answers a route test's request";
    SEMANTIC_REVOCATION_DELAY_INVALID: "semantic.revocation_delay_invalid", area 8, Error, Agent,
        "08-identity.html#semantic-revocation-delay-invalid", "Revocation delay is not a positive duration";
    SEMANTIC_STANDARD_NAMESPACE_RESERVED: "semantic.standard_namespace_reserved", area 1, Error, Agent,
        "01-project-layout-and-modules.html#semantic-standard-namespace-reserved", "Name belongs to a standard-library namespace";
    SEMANTIC_STANDARD_OPERATION_QUALIFICATION: "semantic.standard_operation_qualification", area 1, Error, Agent,
        "01-project-layout-and-modules.html#semantic-standard-operation-qualification", "Standard-library operation is called without its namespace";
    SEMANTIC_UNKNOWN_CALLEE: "semantic.unknown_callee", area 1, Error, Agent,
        "01-project-layout-and-modules.html#semantic-unknown-callee", "Called name is not declared";
    SEMANTIC_UNKNOWN_NAME: "semantic.unknown_name", area 1, Error, Agent,
        "01-project-layout-and-modules.html#semantic-unknown-name", "Name is not declared";
    SEMANTIC_UNREACHABLE_CODE: "semantic.unreachable_code", area 3, Error, Agent,
        "03-logic.html#semantic-unreachable-code", "Code can never run";
    SEMANTIC_UNUSED_RESULT: "semantic.unused_result", area 3, Error, Agent,
        "03-logic.html#semantic-unused-result", "Value is computed and thrown away";
    SEMANTIC_UNUSED_VARIABLE: "semantic.unused_variable", area 3, Error, Agent,
        "03-logic.html#semantic-unused-variable", "Variable is never used";
    SEMANTIC_WRONG_NAME_KIND: "semantic.wrong_name_kind", area 1, Error, Agent,
        "01-project-layout-and-modules.html#semantic-wrong-name-kind", "Name is the wrong kind of declaration here";
    STORE_ACTION_SPANS_STORES: "store.action_spans_stores", area 6, Error, Human,
        "06-reads-and-writes.html#store-action-spans-stores", "Action writes more than one store";
    SYNTAX_ASSERTION_FORM: "syntax.assertion_form", area 13, Error, Agent,
        "13-testing.html#syntax-assertion-form", "Assertion written in another language's form";
    SYNTAX_ASYNC_AWAIT: "syntax.async_await", area 3, Error, Agent,
        "03-logic.html#syntax-async-await", "Code marked `async` or `await`";
    SYNTAX_ATTEMPT_PLACEMENT: "syntax.attempt_placement", area 3, Error, Agent,
        "03-logic.html#syntax-attempt-placement", "Attempt is used inside an expression";
    SYNTAX_COLLECTION_TYPE: "syntax.collection_type", area 2, Error, Agent,
        "02-values-and-types.html#syntax-collection-type", "Collection type written in another language's form";
    SYNTAX_COLON_REQUIRED: "syntax.colon_required", area 3, Error, Agent,
        "03-logic.html#syntax-colon-required", "Member has no colon before its value";
    SYNTAX_COMMENT_STYLE: "syntax.comment_style", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-comment-style", "Comment written with `#` or `/* */`";
    SYNTAX_CONDITION_BRACKETS: "syntax.condition_brackets", area 3, Error, Agent,
        "03-logic.html#syntax-condition-brackets", "Condition is not in brackets";
    SYNTAX_ELSE_IF: "syntax.else_if", area 3, Error, Agent,
        "03-logic.html#syntax-else-if", "Else-if written as `elif` or `elsif`";
    SYNTAX_EQUALITY_OPERATOR: "syntax.equality_operator", area 3, Error, Agent,
        "03-logic.html#syntax-equality-operator", "Comparison written with another language's operator";
    SYNTAX_EXPECTED_DECLARATION: "syntax.expected_declaration", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-expected-declaration", "Expected a declaration";
    SYNTAX_EXPECTED_EXPRESSION: "syntax.expected_expression", area 3, Error, Agent,
        "03-logic.html#syntax-expected-expression", "Expected a value";
    SYNTAX_EXPECTED_HTTP_METHOD: "syntax.expected_http_method", area 7, Error, Agent,
        "07-http.html#syntax-expected-http-method", "Route method is not one a route is written with";
    SYNTAX_EXPECTED_NAME: "syntax.expected_name", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-expected-name", "Expected a name";
    SYNTAX_EXPECTED_STATEMENT: "syntax.expected_statement", area 3, Error, Agent,
        "03-logic.html#syntax-expected-statement", "Expected a statement";
    SYNTAX_EXPORT_KEYWORD: "syntax.export_keyword", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-export-keyword", "Declaration marked `export` or `pub`";
    SYNTAX_FAILS_AFTER_RESULT: "syntax.fails_after_result", area 4, Error, Agent,
        "04-failures.html#syntax-fails-after-result", "Fails list comes after the result type";
    SYNTAX_FOR_LOOP: "syntax.for_loop", area 3, Error, Agent,
        "03-logic.html#syntax-for-loop", "Loop written in another language's form";
    SYNTAX_FUNCTION_KEYWORD: "syntax.function_keyword", area 3, Error, Agent,
        "03-logic.html#syntax-function-keyword", "Function declared with `fn`, `func`, `def` or `fun`";
    SYNTAX_IMPORT_STATEMENT: "syntax.import_statement", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-import-statement", "Import written in another language's form";
    SYNTAX_INCREMENT_OPERATOR: "syntax.increment_operator", area 3, Error, Agent,
        "03-logic.html#syntax-increment-operator", "Variable changed with `++`, `--` or `+=`";
    SYNTAX_INLINE_FUNCTION_ARROW: "syntax.inline_function_arrow", area 3, Error, Agent,
        "03-logic.html#syntax-inline-function-arrow", "Inline function written in another language's form";
    SYNTAX_INVALID_ESCAPE: "syntax.invalid_escape", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-invalid-escape", "Unknown escape in a string";
    SYNTAX_LOCALES_DEFAULT_REQUIRED: "syntax.locales_default_required", area 10, Error, Agent,
        "10-configuration-and-time.html#syntax-locales-default-required", "Locales declaration has no default";
    SYNTAX_LOCALES_DUPLICATE_OPTION: "syntax.locales_duplicate_option", area 10, Error, Agent,
        "10-configuration-and-time.html#syntax-locales-duplicate-option", "Locales option is given twice";
    SYNTAX_LOCALES_SUPPORTED_REQUIRED: "syntax.locales_supported_required", area 10, Error, Agent,
        "10-configuration-and-time.html#syntax-locales-supported-required", "Locales declaration lists no supported locales";
    SYNTAX_LOCALES_UNKNOWN_OPTION: "syntax.locales_unknown_option", area 10, Error, Agent,
        "10-configuration-and-time.html#syntax-locales-unknown-option", "Locales declaration has no such option";
    SYNTAX_LOGICAL_OPERATOR: "syntax.logical_operator", area 3, Error, Agent,
        "03-logic.html#syntax-logical-operator", "Logical operator written in another language's form";
    SYNTAX_MAPPING_BLOCK_REQUIRED: "syntax.mapping_block_required", area 4, Error, Agent,
        "04-failures.html#syntax-mapping-block-required", "Mappings are written after the call instead of in a block";
    SYNTAX_MATCH_ARROW: "syntax.match_arrow", area 3, Error, Agent,
        "03-logic.html#syntax-match-arrow", "Match arm uses `=>`";
    SYNTAX_MATCH_PATTERN: "syntax.match_pattern", area 3, Error, Agent,
        "03-logic.html#syntax-match-pattern", "Expected a match pattern";
    SYNTAX_MEMBER_COMMA: "syntax.member_comma", area 3, Error, Agent,
        "03-logic.html#syntax-member-comma", "Members separated by commas";
    SYNTAX_NAMED_ARGUMENT_DUPLICATE: "syntax.named_argument_duplicate", area 3, Error, Agent,
        "03-logic.html#syntax-named-argument-duplicate", "Named argument is given twice";
    SYNTAX_NAMED_ARGUMENT_EQUALS: "syntax.named_argument_equals", area 3, Error, Agent,
        "03-logic.html#syntax-named-argument-equals", "Named argument written with `=`";
    SYNTAX_NEW_KEYWORD: "syntax.new_keyword", area 2, Error, Agent,
        "02-values-and-types.html#syntax-new-keyword", "Value created with `new`";
    SYNTAX_NULL_COALESCING: "syntax.null_coalescing", area 2, Error, Agent,
        "02-values-and-types.html#syntax-null-coalescing", "Default for a missing value written with `??`";
    SYNTAX_NULLABLE_TYPE: "syntax.nullable_type", area 2, Error, Agent,
        "02-values-and-types.html#syntax-nullable-type", "Nullable type written in another language's form";
    SYNTAX_OPTIONAL_CHAINING: "syntax.optional_chaining", area 2, Error, Agent,
        "02-values-and-types.html#syntax-optional-chaining", "Field of a missing value read with `?.`";
    SYNTAX_POSITIONAL_AFTER_NAMED: "syntax.positional_after_named", area 3, Error, Agent,
        "03-logic.html#syntax-positional-after-named", "Positional argument follows a named one";
    SYNTAX_RETURN_TYPE_ARROW: "syntax.return_type_arrow", area 3, Error, Agent,
        "03-logic.html#syntax-return-type-arrow", "Result type written after `:`";
    SYNTAX_SEMICOLON: "syntax.semicolon", area 3, Error, Agent,
        "03-logic.html#syntax-semicolon", "Statement or member ends with `;`";
    SYNTAX_STRING_QUOTES: "syntax.string_quotes", area 2, Error, Agent,
        "02-values-and-types.html#syntax-string-quotes", "String in single quotes or backticks";
    SYNTAX_SWITCH_STATEMENT: "syntax.switch_statement", area 3, Error, Agent,
        "03-logic.html#syntax-switch-statement", "Choice written with `switch` or `when`";
    SYNTAX_TERNARY_OPERATOR: "syntax.ternary_operator", area 3, Error, Agent,
        "03-logic.html#syntax-ternary-operator", "Value chosen with `? :`";
    SYNTAX_TRY_CATCH: "syntax.try_catch", area 3, Error, Agent,
        "03-logic.html#syntax-try-catch", "Failure handled with `try`, `catch` or `?`";
    SYNTAX_TYPE_BLOCK_REQUIRED: "syntax.type_block_required", area 2, Error, Agent,
        "02-values-and-types.html#syntax-type-block-required", "Type has no block";
    SYNTAX_TYPE_KEYWORD: "syntax.type_keyword", area 2, Error, Agent,
        "02-values-and-types.html#syntax-type-keyword", "Type declared with `struct`, `interface` or `enum`";
    SYNTAX_TYPE_PARENT_REQUIRED: "syntax.type_parent_required", area 2, Error, Agent,
        "02-values-and-types.html#syntax-type-parent-required", "Type has no base";
    SYNTAX_UNEXPECTED_CHARACTER: "syntax.unexpected_character", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-unexpected-character", "Character is not part of Jadpo";
    SYNTAX_UNEXPECTED_TOKEN: "syntax.unexpected_token", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-unexpected-token", "Something else was expected here";
    SYNTAX_UNSUPPORTED_THROW: "syntax.unsupported_throw", area 3, Error, Agent,
        "03-logic.html#syntax-unsupported-throw", "Failure raised with `throw` or `raise`";
    SYNTAX_UNTERMINATED_STRING: "syntax.unterminated_string", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-unterminated-string", "String is not closed";
    SYNTAX_VARIABLE_KEYWORD: "syntax.variable_keyword", area 3, Error, Agent,
        "03-logic.html#syntax-variable-keyword", "Variable declared with `let`, `const` or `val`";
    SYNTAX_VARIABLE_TYPE_REQUIRED: "syntax.variable_type_required", area 2, Error, Agent,
        "02-values-and-types.html#syntax-variable-type-required", "Variable has no type";
    TEST_ASSERTION_FAILED: "test.assertion_failed", area 13, Error, Agent,
        "13-testing.html#test-assertion-failed", "Test assertion does not hold";
    TEST_FAILED: "test.failed", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Test run failed";
    TEST_FAULT: "test.fault", area 13, Error, Agent,
        "13-testing.html#test-fault", "Test stopped on a fault";
    TEST_NO_TESTS: "test.no_tests", area 14, Error, Agent,
        "14-tooling.html#command-errors", "No tests to run";
    TEST_RUNTIME_START_FAILED: "test.runtime_start_failed", area 14, Error, Human,
        "14-tooling.html#command-errors", "Test runtime could not start";
    TEST_UNEXPECTED_FAILURE: "test.unexpected_failure", area 13, Error, Agent,
        "13-testing.html#test-unexpected-failure", "Test ended in a failure it did not expect";
    TOOLCHAIN_ARTIFACT_WRITE_FAILED: "toolchain.artifact_write_failed", area 14, Error, Human,
        "14-tooling.html#command-errors", "Generated file could not be written";
    TOOLCHAIN_EMPTY_PROJECT: "toolchain.empty_project", area 1, Error, Agent,
        "01-project-layout-and-modules.html#toolchain-empty-project", "Project declares nothing";
    TOOLCHAIN_NOT_BUILT_YET: "toolchain.not_built_yet", area 3, Error, Agent,
        "03-logic.html#toolchain-not-built-yet", "Operation is declared but not built yet";
    TOOLCHAIN_PROJECT_NOT_FOUND: "toolchain.project_not_found", area 14, Error, Agent,
        "14-tooling.html#the-jadpo-command", "Project could not be found";
    TOOLCHAIN_SOURCE_READ_FAILED: "toolchain.source_read_failed", area 14, Error, Human,
        "14-tooling.html#the-jadpo-command", "Jadpo source file could not be read";
    TYPE_ABSENT_FIELD: "type.absent_field", area 2, Error, Agent,
        "02-values-and-types.html#type-absent-field", "Omitted or optional field is not present";
    TYPE_ARGUMENT_COUNT: "type.argument_count", area 3, Error, Agent,
        "03-logic.html#type-argument-count", "Call has the wrong number of arguments";
    TYPE_ARITHMETIC_OPERAND: "type.arithmetic_operand", area 3, Error, Agent,
        "03-logic.html#type-arithmetic-operand", "Arithmetic operand has the wrong type";
    TYPE_ASSIGN_IMMUTABLE: "type.assign_immutable", area 3, Error, Agent,
        "03-logic.html#type-assign-immutable", "Value that never changes is assigned";
    TYPE_ASSIGN_UNKNOWN: "type.assign_unknown", area 3, Error, Agent,
        "03-logic.html#type-assign-unknown", "Assignment target is not declared";
    TYPE_AUTH_CREDENTIAL_STORED: "type.auth_credential_stored", area 8, Error, Agent,
        "08-identity.html#type-auth-credential-stored", "Entity field holds an API token or a service key";
    TYPE_AUTH_PRINCIPAL_CONSTRUCTION: "type.auth_principal_construction", area 8, Error, Agent,
        "08-identity.html#type-auth-principal-construction", "Caller is made from a record outside tests, fixtures and seeds";
    TYPE_AUTH_PRINCIPAL_FAILURE_CONTEXT: "type.auth_principal_failure_context", area 8, Error, Agent,
        "08-identity.html#type-auth-principal-failure-context", "Failure field holds a caller";
    TYPE_AUTH_PRINCIPAL_INPUT: "type.auth_principal_input", area 8, Error, Agent,
        "08-identity.html#type-auth-principal-input", "Input field holds a caller";
    TYPE_AUTH_PRINCIPAL_OUTPUT: "type.auth_principal_output", area 8, Error, Agent,
        "08-identity.html#type-auth-principal-output", "Output field holds a caller";
    TYPE_AUTH_PRINCIPAL_VARIANT_UNKNOWN: "type.auth_principal_variant_unknown", area 8, Error, Agent,
        "08-identity.html#type-auth-principal-variant-unknown", "Caller variant is not one the principal declares";
    TYPE_AUTH_PUBLIC_PRINCIPAL: "type.auth_public_principal", area 8, Error, Human,
        "08-identity.html#type-auth-public-principal", "Public route passes a caller it never reads";
    TYPE_AUTH_SESSIONS_CONTEXT: "type.auth_sessions_context", area 8, Error, Agent,
        "08-identity.html#type-auth-sessions-context", "Session, API token or service key operation outside an action, or without its sign-in method";
    TYPE_BASE_OUTSIDE_DECLARATION: "type.base_outside_declaration", area 2, Error, Agent,
        "02-values-and-types.html#type-base-outside-declaration", "Base is used outside a type declaration";
    TYPE_CLOCK_CONTEXT: "type.clock_context", area 10, Error, Agent,
        "10-configuration-and-time.html#type-clock-context", "The operation clock is unavailable in this context";
    TYPE_CONSTRAINT_BASE: "type.constraint_base", area 2, Error, Agent,
        "02-values-and-types.html#type-constraint-base", "Constraint does not fit the type's base";
    TYPE_CONSTRAINT_CONTRADICTION: "type.constraint_contradiction", area 2, Error, Agent,
        "02-values-and-types.html#type-constraint-contradiction", "Constraints contradict each other";
    TYPE_CONSTRAINT_LOOSENED: "type.constraint_loosened", area 2, Error, Agent,
        "02-values-and-types.html#type-constraint-loosened", "Refinement loosens a rule it inherits";
    TYPE_CONSTRAINT_REPEATED: "type.constraint_repeated", area 2, Error, Agent,
        "02-values-and-types.html#type-constraint-repeated", "Constraint is given twice";
    TYPE_CONSTRUCTOR_ARGUMENT_COUNT: "type.constructor_argument_count", area 2, Error, Agent,
        "02-values-and-types.html#type-constructor-argument-count", "Constructor is not given exactly one value";
    TYPE_CONSTRUCTOR_INPUT: "type.constructor_input", area 2, Error, Agent,
        "02-values-and-types.html#type-constructor-input", "Value cannot be converted to this type";
    TYPE_COPY_CLASH: "type.copy_clash", area 2, Error, Agent,
        "02-values-and-types.html#type-copy-clash", "Copied field would replace another field";
    TYPE_COPY_CYCLE: "type.copy_cycle", area 2, Error, Agent,
        "02-values-and-types.html#type-copy-cycle", "Type includes its own fields";
    TYPE_COPY_INTO_INPUT: "type.copy_into_input", area 2, Error, Human,
        "02-values-and-types.html#type-copy-into-input", "Input copies every field of another type";
    TYPE_DECIMAL_SCALE_REQUIRED: "type.decimal_scale_required", area 2, Error, Agent,
        "02-values-and-types.html#type-decimal-scale-required", "Decimal type has no scale";
    TYPE_DIVISOR_ZERO: "type.divisor_zero", area 3, Error, Agent,
        "03-logic.html#type-divisor-zero", "Divisor may be zero";
    TYPE_DUPLICATE_FIELD: "type.duplicate_field", area 2, Error, Agent,
        "02-values-and-types.html#type-duplicate-field", "Field is given twice";
    TYPE_DUPLICATE_VARIANT: "type.duplicate_variant", area 2, Error, Agent,
        "02-values-and-types.html#type-duplicate-variant", "Enum variant is declared twice";
    TYPE_DUPLICATE_VARIANT_FIELD: "type.duplicate_variant_field", area 2, Error, Agent,
        "02-values-and-types.html#type-duplicate-variant-field", "Variant field is given twice";
    TYPE_ENUM_EMPTY: "type.enum_empty", area 2, Error, Agent,
        "02-values-and-types.html#type-enum-empty", "Enum declares no variants";
    TYPE_FIELD_ON_NON_RECORD: "type.field_on_non_record", area 2, Error, Agent,
        "02-values-and-types.html#type-field-on-non-record", "Field is read from a value without fields";
    TYPE_FIELD_UNPROVED: "type.field_unproved", area 2, Error, Agent,
        "02-values-and-types.html#type-field-unproved", "Object field value may break the field's rules";
    TYPE_FIELD_UPDATE_FORBIDDEN: "type.field_update_forbidden", area 9, Error, Agent,
        "09-policy.html#type-field-update-forbidden", "Update changes a field that never changes";
    TYPE_FOREIGN_TYPE: "type.foreign_type", area 2, Error, Agent,
        "02-values-and-types.html#type-foreign-type", "Type named as another language names it";
    TYPE_FOREIGN_VALUE: "type.foreign_value", area 2, Error, Agent,
        "02-values-and-types.html#type-foreign-value", "Value written as another language writes it";
    TYPE_INCOMPARABLE: "type.incomparable", area 3, Error, Agent,
        "03-logic.html#type-incomparable", "Values cannot be compared for equality";
    TYPE_INLINE_FUNCTION_PLACEMENT: "type.inline_function_placement", area 3, Error, Agent,
        "03-logic.html#type-inline-function-placement", "Inline function is not an argument to a collection operation";
    TYPE_INVALID_LITERAL: "type.invalid_literal", area 2, Error, Agent,
        "02-values-and-types.html#type-invalid-literal", "Literal breaks its type's rules";
    TYPE_INVALID_PATTERN: "type.invalid_pattern", area 2, Error, Agent,
        "02-values-and-types.html#type-invalid-pattern", "Pattern is not a supported regular expression";
    TYPE_LOCALE_DEFAULT_UNSUPPORTED: "type.locale_default_unsupported", area 10, Error, Agent,
        "10-configuration-and-time.html#type-locale-default-unsupported", "Default locale is not a supported one";
    TYPE_LOCALE_DUPLICATE: "type.locale_duplicate", area 10, Error, Agent,
        "10-configuration-and-time.html#type-locale-duplicate", "Locale is listed twice";
    TYPE_LOCALE_INVALID: "type.locale_invalid", area 10, Error, Agent,
        "10-configuration-and-time.html#type-locale-invalid", "Locale is not written as a Locale";
    TYPE_LOCALE_UNSUPPORTED: "type.locale_unsupported", area 10, Error, Agent,
        "10-configuration-and-time.html#type-locale-unsupported", "Locale is not one the application supports";
    TYPE_LOGICAL_OPERAND: "type.logical_operand", area 3, Error, Agent,
        "03-logic.html#type-logical-operand", "Logical operator needs Bool operands";
    TYPE_MAP_KEY_BOUNDARY: "type.map_key_boundary", area 2, Error, Agent,
        "02-values-and-types.html#type-map-key-boundary", "Map key at a boundary isn't text";
    TYPE_MATCH_ARM_VALUE: "type.match_arm_value", area 3, Error, Agent,
        "03-logic.html#type-match-arm-value", "Match arm gives no value";
    TYPE_MATCH_DUPLICATE_PATTERN: "type.match_duplicate_pattern", area 3, Error, Agent,
        "03-logic.html#type-match-duplicate-pattern", "Match pattern is already covered";
    TYPE_MATCH_NON_EXHAUSTIVE: "type.match_non_exhaustive", area 3, Error, Agent,
        "03-logic.html#type-match-non-exhaustive", "Match does not cover every case";
    TYPE_MATCH_PATTERN_TYPE: "type.match_pattern_type", area 3, Error, Agent,
        "03-logic.html#type-match-pattern-type", "Pattern cannot match this value";
    TYPE_MATCH_SOME_NON_OPTIONAL: "type.match_some_non_optional", area 3, Error, Agent,
        "03-logic.html#type-match-some-non-optional", "Nullable pattern on a value that is never none";
    TYPE_MATCH_UNKNOWN_BINDING: "type.match_unknown_binding", area 3, Error, Agent,
        "03-logic.html#type-match-unknown-binding", "Pattern names data a case does not carry";
    TYPE_MATCH_UNKNOWN_VARIANT: "type.match_unknown_variant", area 3, Error, Agent,
        "03-logic.html#type-match-unknown-variant", "Pattern names an unknown enum variant";
    TYPE_MATCH_UNREACHABLE_PATTERN: "type.match_unreachable_pattern", area 3, Error, Agent,
        "03-logic.html#type-match-unreachable-pattern", "Match arm is unreachable after the wildcard";
    TYPE_MATCH_WILDCARD_CLOSED: "type.match_wildcard_closed", area 3, Error, Agent,
        "03-logic.html#type-match-wildcard-closed", "Wildcard on a value whose cases can be listed";
    TYPE_MATCH_WILDCARD_REQUIRED: "type.match_wildcard_required", area 3, Error, Agent,
        "03-logic.html#type-match-wildcard-required", "Open value space needs a wildcard arm";
    TYPE_MISMATCH: "type.mismatch", area 2, Error, Agent,
        "02-values-and-types.html#type-mismatch", "Value has the wrong type";
    TYPE_MISSING_FIELD: "type.missing_field", area 2, Error, Agent,
        "02-values-and-types.html#type-missing-field", "Object is missing a field";
    TYPE_MISSING_VARIANT_FIELD: "type.missing_variant_field", area 2, Error, Agent,
        "02-values-and-types.html#type-missing-variant-field", "Variant is missing a field";
    TYPE_NAMED_ARGUMENT_UNKNOWN: "type.named_argument_unknown", area 3, Error, Agent,
        "03-logic.html#type-named-argument-unknown", "Named argument is not a parameter";
    TYPE_NAMED_ARGUMENT_UNSUPPORTED: "type.named_argument_unsupported", area 3, Error, Agent,
        "03-logic.html#type-named-argument-unsupported", "Callable does not take named arguments";
    TYPE_NOT_A_VALUE: "type.not_a_value", area 3, Error, Agent,
        "03-logic.html#type-not-a-value", "Declaration named where a value is read";
    TYPE_NOT_RECORD: "type.not_record", area 2, Error, Agent,
        "02-values-and-types.html#type-not-record", "Braces build a type that has no fields";
    TYPE_NULLABLE_SELECTION: "type.nullable_selection", area 2, Error, Agent,
        "02-values-and-types.html#type-nullable-selection", "Field is read from a value that may be none";
    TYPE_OPTIONAL_OUTSIDE_INPUT: "type.optional_outside_input", area 2, Error, Agent,
        "02-values-and-types.html#type-optional-outside-input", "Optional or default field outside an Input";
    TYPE_ORDERING_OPERAND: "type.ordering_operand", area 3, Error, Agent,
        "03-logic.html#type-ordering-operand", "Values cannot be ordered";
    TYPE_PRESENTATION_TEXT_PERSISTENCE: "type.presentation_text_persistence", area 10, Error, Agent,
        "10-configuration-and-time.html#type-presentation-text-persistence", "Formatted text is stored";
    TYPE_PRIMITIVE_SIGNATURE: "type.primitive_signature", area 2, Error, Agent,
        "02-values-and-types.html#type-primitive-signature", "Signature uses a built-in type directly";
    TYPE_PRINCIPAL_ACTIVE_INVALID: "type.principal_active_invalid", area 8, Error, Agent,
        "08-identity.html#type-principal-active-invalid", "Principal's active condition is not a test of the record's own fields";
    TYPE_PRINCIPAL_SUBJECT_INVALID: "type.principal_subject_invalid", area 8, Error, Agent,
        "08-identity.html#type-principal-subject-invalid", "Principal's subject is not a field of a stored entity";
    TYPE_PRINCIPAL_SUBJECT_NOT_UNIQUE: "type.principal_subject_not_unique", area 8, Error, Agent,
        "08-identity.html#type-principal-subject-not-unique", "Principal's subject field is not unique";
    TYPE_REDUNDANT_NULLABILITY: "type.redundant_nullability", area 2, Error, Agent,
        "02-values-and-types.html#type-redundant-nullability", "Type is already nullable";
    TYPE_RESERVED_FIELD_NAME: "type.reserved_field_name", area 2, Error, Agent,
        "02-values-and-types.html#type-reserved-field-name", "Field uses a reserved name";
    TYPE_SETTING_VALUE: "type.setting_value", area 2, Error, Agent,
        "02-values-and-types.html#type-setting-value", "Setting has the wrong kind of value";
    TYPE_SIBLING_MISMATCH: "type.sibling_mismatch", area 2, Error, Agent,
        "02-values-and-types.html#type-sibling-mismatch", "Field type is used for a sibling field type";
    TYPE_TEMPORAL_CLOCK_INVALID: "type.temporal_clock_invalid", area 10, Error, Agent,
        "10-configuration-and-time.html#type-temporal-clock-invalid", "Clock time is not written `HH:MM`";
    TYPE_TEMPORAL_FORMAT_COMPONENT_DUPLICATE: "type.temporal_format_component_duplicate", area 10, Error, Agent,
        "10-configuration-and-time.html#type-temporal-format-component-duplicate", "Format component is given twice";
    TYPE_TEMPORAL_FORMAT_COMPONENT_INVALID: "type.temporal_format_component_invalid", area 10, Error, Agent,
        "10-configuration-and-time.html#type-temporal-format-component-invalid", "Format component takes no such style";
    TYPE_TEMPORAL_FORMAT_COMPONENTS_EMPTY: "type.temporal_format_components_empty", area 10, Error, Agent,
        "10-configuration-and-time.html#type-temporal-format-components-empty", "Declared format shows nothing";
    TYPE_TEMPORAL_FORMAT_INVALID: "type.temporal_format_invalid", area 10, Error, Agent,
        "10-configuration-and-time.html#type-temporal-format-invalid", "Format is not a written-out time format";
    TYPE_TEMPORAL_FORMAT_REQUIRES_TIME: "type.temporal_format_requires_time", area 10, Error, Agent,
        "10-configuration-and-time.html#type-temporal-format-requires-time", "Format shows a clock time a date doesn't have";
    TYPE_TEMPORAL_LOCALES_REQUIRED: "type.temporal_locales_required", area 10, Error, Agent,
        "10-configuration-and-time.html#type-temporal-locales-required", "Formatting needs declared locales";
    TYPE_TEMPORAL_OPTION_UNUSED: "type.temporal_option_unused", area 10, Error, Agent,
        "10-configuration-and-time.html#type-temporal-option-unused", "Option means nothing for this value";
    TYPE_TEMPORAL_ZONE_REQUIRED: "type.temporal_zone_required", area 10, Error, Agent,
        "10-configuration-and-time.html#type-temporal-zone-required", "Formatting an instant needs a zone";
    TYPE_TEST_CLOCK_CONTEXT: "type.test_clock_context", area 13, Error, Agent,
        "13-testing.html#type-test-clock-context", "The clock is moved outside a test";
    TYPE_TYPE_ARGUMENT_COUNT: "type.type_argument_count", area 2, Error, Agent,
        "02-values-and-types.html#type-type-argument-count", "Type has the wrong number of type arguments";
    TYPE_UNARY_OPERAND: "type.unary_operand", area 3, Error, Agent,
        "03-logic.html#type-unary-operand", "Operator cannot be used with this value";
    TYPE_UNKNOWN_ENUM_VARIANT: "type.unknown_enum_variant", area 2, Error, Agent,
        "02-values-and-types.html#type-unknown-enum-variant", "Enum has no such variant";
    TYPE_UNKNOWN_FIELD: "type.unknown_field", area 2, Error, Agent,
        "02-values-and-types.html#type-unknown-field", "Type has no such field";
    TYPE_UNKNOWN_SETTING: "type.unknown_setting", area 2, Error, Agent,
        "02-values-and-types.html#type-unknown-setting", "Setting is not one Jadpo knows";
    TYPE_UNKNOWN_VALUE: "type.unknown_value", area 3, Error, Agent,
        "03-logic.html#type-unknown-value", "Name is not defined in this scope";
    TYPE_UNKNOWN_VARIANT_FIELD: "type.unknown_variant_field", area 2, Error, Agent,
        "02-values-and-types.html#type-unknown-variant-field", "Variant has no such field";
    VISIBILITY_PRIVATE_CROSS_DOMAIN: "visibility.private_cross_domain", area 1, Error, Human,
        "01-project-layout-and-modules.html#visibility-private-cross-domain", "Another domain uses a private declaration";
    VISIBILITY_PUBLIC_NOT_ALLOWED: "visibility.public_not_allowed", area 1, Error, Agent,
        "01-project-layout-and-modules.html#visibility-public-not-allowed", "Declaration can't be public";
    VISIBILITY_PUBLIC_UNUSED: "visibility.public_unused", area 1, Error, Agent,
        "01-project-layout-and-modules.html#visibility-public-unused", "Public declaration no other domain uses";
}

/// Looks a rule up by id, for tools that read ids back from JSON.
pub fn find(id: &str) -> Option<&'static Rule> {
    CATALOGUE.iter().find(|rule| rule.id == id)
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
    /// A page section that says more than the rule's own entry, relative
    /// to `docs/language/`: for a rule many areas share, such as
    /// `toolchain.not_built_yet`, the section that decides what isn't
    /// built.
    pub help: Option<String>,
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
            help: None,
        }
    }

    /// The help link printed on the `help` line: the section given with
    /// [`Diagnostic::help_at`], or the rule's entry.
    pub fn help(&self) -> String {
        match &self.help {
            Some(anchor) => format!("docs/language/{anchor}"),
            None => self.rule.help(),
        }
    }

    /// Points the help link at `anchor` (`13-testing.html#kinds-of-test`).
    pub fn help_at(mut self, anchor: impl Into<String>) -> Diagnostic {
        self.help = Some(anchor.into());
        self
    }

    pub fn severity(&self) -> Severity {
        self.rule.severity
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

    /// The next step, decided by whoever the rule names.
    pub fn next(mut self, text: impl Into<String>) -> Diagnostic {
        self.next = Some(Next {
            text: text.into(),
            decides: self.rule.decides,
            edits: Vec::new(),
        });
        self
    }

    /// The next step, decided by someone other than the rule's default.
    pub fn next_for(mut self, text: impl Into<String>, decides: Decider) -> Diagnostic {
        self.next = Some(Next {
            text: text.into(),
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
    fn rule_ids_summaries_and_anchors_are_well_formed() {
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
                (1..=18).contains(&rule.area),
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
            assert!(
                rule.anchor.starts_with(&format!("{:02}-", rule.area))
                    && rule.anchor.contains(".html#"),
                "{}",
                rule.id
            );
        }
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
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(
            diagnostic.next.as_ref().map(|next| next.decides),
            Some(Decider::Agent)
        );
        assert_eq!(
            diagnostic.rule.help(),
            "docs/language/14-tooling.html#command-errors"
        );
    }
}
