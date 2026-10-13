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
    CLI_CHECK_ARGUMENTS: "cli.check_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Check command has invalid arguments";
    CLI_COMPLETIONS_ARGUMENTS: "cli.completions_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Completions command has invalid arguments";
    CLI_FMT_ARGUMENTS: "cli.fmt_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Format command has invalid arguments";
    CLI_HELP_ARGUMENTS: "cli.help_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Help command has invalid arguments";
    CLI_LSP_ARGUMENTS: "cli.lsp_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Language-server command has invalid arguments";
    CLI_PRESENTATION_ARGUMENTS: "cli.presentation_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Terminal presentation option is invalid";
    CLI_UNKNOWN_COMMAND: "cli.unknown_command", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Unknown Jadpo command";
    CLI_UNKNOWN_FLAG: "cli.unknown_flag", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Unknown flag";
    CLI_VERSION_ARGUMENTS: "cli.version_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Version command has invalid arguments";
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
    FAILURE_INVALID_MAPPING_REQUIRED: "failure.invalid_mapping_required", area 4, Error, Agent,
        "04-failures.html#failure-invalid-mapping-required", "Construction that can fail does not name its failure";
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
    FORMAT_WRITE_FAILED: "format.write_failed", area 14, Error, Human,
        "14-tooling.html#formatting", "Formatted source could not be written";
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
    NAMING_FOLDED_COLLISION: "naming.folded_collision", area 1, Error, Agent,
        "01-project-layout-and-modules.html#naming-folded-collision", "Two names fold to the same file name";
    NAMING_GENERIC_NAME: "naming.generic_name", area 1, Error, Human,
        "01-project-layout-and-modules.html#naming-generic-name", "Domain or library has a generic name";
    NAMING_ROUND_TRIP: "naming.round_trip", area 1, Error, Agent,
        "01-project-layout-and-modules.html#naming-round-trip", "Name does not survive conversion to its file name";
    SEMANTIC_DUPLICATE_DECLARATION: "semantic.duplicate_declaration", area 1, Error, Agent,
        "01-project-layout-and-modules.html#semantic-duplicate-declaration", "Name is declared more than once";
    SEMANTIC_MISSING_RETURN: "semantic.missing_return", area 3, Error, Agent,
        "03-logic.html#semantic-missing-return", "Callable can end without returning its result";
    SEMANTIC_NAME_CASE: "semantic.name_case", area 1, Error, Agent,
        "01-project-layout-and-modules.html#semantic-name-case", "Name has the wrong case for its kind";
    SEMANTIC_NOT_CALLABLE: "semantic.not_callable", area 1, Error, Agent,
        "01-project-layout-and-modules.html#semantic-not-callable", "Name cannot be called";
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
    SYNTAX_ATTEMPT_PLACEMENT: "syntax.attempt_placement", area 3, Error, Agent,
        "03-logic.html#syntax-attempt-placement", "Attempt is used inside an expression";
    SYNTAX_COLON_REQUIRED: "syntax.colon_required", area 3, Error, Agent,
        "03-logic.html#syntax-colon-required", "Member has no colon before its value";
    SYNTAX_CONDITION_BRACKETS: "syntax.condition_brackets", area 3, Error, Agent,
        "03-logic.html#syntax-condition-brackets", "Condition is not in brackets";
    SYNTAX_EXPECTED_DECLARATION: "syntax.expected_declaration", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-expected-declaration", "Expected a declaration";
    SYNTAX_EXPECTED_EXPRESSION: "syntax.expected_expression", area 3, Error, Agent,
        "03-logic.html#syntax-expected-expression", "Expected a value";
    SYNTAX_EXPECTED_NAME: "syntax.expected_name", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-expected-name", "Expected a name";
    SYNTAX_EXPECTED_STATEMENT: "syntax.expected_statement", area 3, Error, Agent,
        "03-logic.html#syntax-expected-statement", "Expected a statement";
    SYNTAX_FAILS_AFTER_RESULT: "syntax.fails_after_result", area 4, Error, Agent,
        "04-failures.html#syntax-fails-after-result", "Fails list comes after the result type";
    SYNTAX_INVALID_ESCAPE: "syntax.invalid_escape", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-invalid-escape", "Unknown escape in a string";
    SYNTAX_MATCH_ARROW: "syntax.match_arrow", area 3, Error, Agent,
        "03-logic.html#syntax-match-arrow", "Match arm uses `=>`";
    SYNTAX_MATCH_PATTERN: "syntax.match_pattern", area 3, Error, Agent,
        "03-logic.html#syntax-match-pattern", "Expected a match pattern";
    SYNTAX_NAMED_ARGUMENT_DUPLICATE: "syntax.named_argument_duplicate", area 3, Error, Agent,
        "03-logic.html#syntax-named-argument-duplicate", "Named argument is given twice";
    SYNTAX_POSITIONAL_AFTER_NAMED: "syntax.positional_after_named", area 3, Error, Agent,
        "03-logic.html#syntax-positional-after-named", "Positional argument follows a named one";
    SYNTAX_TYPE_BLOCK_REQUIRED: "syntax.type_block_required", area 2, Error, Agent,
        "02-values-and-types.html#syntax-type-block-required", "Type has no block";
    SYNTAX_TYPE_PARENT_REQUIRED: "syntax.type_parent_required", area 2, Error, Agent,
        "02-values-and-types.html#syntax-type-parent-required", "Type has no base";
    SYNTAX_UNEXPECTED_CHARACTER: "syntax.unexpected_character", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-unexpected-character", "Character is not part of Jadpo";
    SYNTAX_UNEXPECTED_TOKEN: "syntax.unexpected_token", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-unexpected-token", "Something else was expected here";
    SYNTAX_UNSUPPORTED_THROW: "syntax.unsupported_throw", area 3, Error, Agent,
        "03-logic.html#syntax-unsupported-throw", "Jadpo has no `throw`";
    SYNTAX_UNTERMINATED_STRING: "syntax.unterminated_string", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-unterminated-string", "String is not closed";
    SYNTAX_VARIABLE_TYPE_REQUIRED: "syntax.variable_type_required", area 2, Error, Agent,
        "02-values-and-types.html#syntax-variable-type-required", "Variable has no type";
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
    TYPE_BASE_OUTSIDE_DECLARATION: "type.base_outside_declaration", area 2, Error, Agent,
        "02-values-and-types.html#type-base-outside-declaration", "Base is used outside a type declaration";
    TYPE_CONSTRAINT_BASE: "type.constraint_base", area 2, Error, Agent,
        "02-values-and-types.html#type-constraint-base", "Constraint does not fit the type's base";
    TYPE_CONSTRAINT_CONTRADICTION: "type.constraint_contradiction", area 2, Error, Agent,
        "02-values-and-types.html#type-constraint-contradiction", "Constraints contradict each other";
    TYPE_CONSTRAINT_REPEATED: "type.constraint_repeated", area 2, Error, Agent,
        "02-values-and-types.html#type-constraint-repeated", "Constraint is given twice";
    TYPE_CONSTRUCTOR_ARGUMENT_COUNT: "type.constructor_argument_count", area 2, Error, Agent,
        "02-values-and-types.html#type-constructor-argument-count", "Constructor is not given exactly one value";
    TYPE_CONSTRUCTOR_INPUT: "type.constructor_input", area 2, Error, Agent,
        "02-values-and-types.html#type-constructor-input", "Value cannot be converted to this type";
    TYPE_COPY_CLASH: "type.copy_clash", area 2, Error, Agent,
        "02-values-and-types.html#type-copy-clash", "Copied field would replace another field";
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
    TYPE_INCOMPARABLE: "type.incomparable", area 3, Error, Agent,
        "03-logic.html#type-incomparable", "Values cannot be compared for equality";
    TYPE_INLINE_FUNCTION_PLACEMENT: "type.inline_function_placement", area 3, Error, Agent,
        "03-logic.html#type-inline-function-placement", "Inline function is not an argument to a collection operation";
    TYPE_INVALID_LITERAL: "type.invalid_literal", area 2, Error, Agent,
        "02-values-and-types.html#type-invalid-literal", "Literal breaks its type's rules";
    TYPE_INVALID_PATTERN: "type.invalid_pattern", area 2, Error, Agent,
        "02-values-and-types.html#type-invalid-pattern", "Pattern is not a supported regular expression";
    TYPE_LOGICAL_OPERAND: "type.logical_operand", area 3, Error, Agent,
        "03-logic.html#type-logical-operand", "Logical operator needs Bool operands";
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
    TYPE_NOT_RECORD: "type.not_record", area 2, Error, Agent,
        "02-values-and-types.html#type-not-record", "Braces build a type that has no fields";
    TYPE_NULLABLE_SELECTION: "type.nullable_selection", area 2, Error, Agent,
        "02-values-and-types.html#type-nullable-selection", "Field is read from a value that may be none";
    TYPE_OPTIONAL_OUTSIDE_INPUT: "type.optional_outside_input", area 2, Error, Agent,
        "02-values-and-types.html#type-optional-outside-input", "Optional or default field outside an Input";
    TYPE_ORDERING_OPERAND: "type.ordering_operand", area 3, Error, Agent,
        "03-logic.html#type-ordering-operand", "Values cannot be ordered";
    TYPE_PRIMITIVE_SIGNATURE: "type.primitive_signature", area 2, Error, Agent,
        "02-values-and-types.html#type-primitive-signature", "Signature uses a built-in type directly";
    TYPE_REDUNDANT_NULLABILITY: "type.redundant_nullability", area 2, Error, Agent,
        "02-values-and-types.html#type-redundant-nullability", "Type is already nullable";
    TYPE_RESERVED_FIELD_NAME: "type.reserved_field_name", area 2, Error, Agent,
        "02-values-and-types.html#type-reserved-field-name", "Field uses a reserved name";
    TYPE_SETTING_VALUE: "type.setting_value", area 2, Error, Agent,
        "02-values-and-types.html#type-setting-value", "Setting has the wrong kind of value";
    TYPE_SIBLING_MISMATCH: "type.sibling_mismatch", area 2, Error, Agent,
        "02-values-and-types.html#type-sibling-mismatch", "Field type is used for a sibling field type";
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
        }
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
