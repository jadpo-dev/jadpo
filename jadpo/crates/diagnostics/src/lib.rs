use std::fmt;

include!(concat!(env!("OUT_DIR"), "/catalogue_codes.rs"));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl fmt::Display for Severity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error => formatter.write_str("error"),
            Self::Warning => formatter.write_str("warning"),
            Self::Note => formatter.write_str("note"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    pub source: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairKind {
    AutomaticFix,
    GuidedChoice,
    HumanDecision,
}

impl RepairKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AutomaticFix => "automatic_fix",
            Self::GuidedChoice => "guided_choice",
            Self::HumanDecision => "human_decision",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecisionOwner {
    Compiler,
    Agent,
    Human,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiagnosticFact {
    Callable(String),
    DeclaredProblems(String),
    ReachableProblems(String),
    EventRevision(String),
    LocalRevision(String),
    Route(String),
    FoundValue(String),
    Expected(String),
    Name(String),
    ActualKind(String),
    Examples(String),
    SuggestedName(String),
    Usage(String),
    Received(String),
    Subject(String),
    Field(String),
    ExpectedCount(String),
    ReceivedCount(String),
    Constraint(String),
    Failure(String),
    OtherFailure(String),
    CodeValue(String),
    Scope(String),
    Operation(String),
    Failures(String),
}

impl DiagnosticFact {
    fn into_pair(self) -> (&'static str, String) {
        match self {
            Self::Callable(value) => ("callable", value),
            Self::DeclaredProblems(value) => ("declared", value),
            Self::ReachableProblems(value) => ("reachable", value),
            Self::EventRevision(value) => ("eventRevision", value),
            Self::LocalRevision(value) => ("localRevision", value),
            Self::Route(value) => ("route", value),
            Self::FoundValue(value) => ("found", value),
            Self::Expected(value) => ("expected", value),
            Self::Name(value) => ("name", value),
            Self::ActualKind(value) => ("actualKind", value),
            Self::Examples(value) => ("examples", value),
            Self::SuggestedName(value) => ("suggestedName", value),
            Self::Usage(value) => ("usage", value),
            Self::Received(value) => ("received", value),
            Self::Subject(value) => ("subject", value),
            Self::Field(value) => ("field", value),
            Self::ExpectedCount(value) => ("expectedCount", value),
            Self::ReceivedCount(value) => ("receivedCount", value),
            Self::Constraint(value) => ("constraint", value),
            Self::Failure(value) => ("failure", value),
            Self::OtherFailure(value) => ("otherFailure", value),
            Self::CodeValue(value) => ("codeValue", value),
            Self::Scope(value) => ("scope", value),
            Self::Operation(value) => ("operation", value),
            Self::Failures(value) => ("failures", value),
        }
    }
}

impl DecisionOwner {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Compiler => "compiler",
            Self::Agent => "agent",
            Self::Human => "human",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextEdit {
    pub source: String,
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepairStep {
    pub kind: RepairKind,
    pub title: String,
    pub reason: String,
    pub decision_owner: DecisionOwner,
    pub preferred: bool,
    pub edits: Vec<TextEdit>,
    pub behavioral_effect: String,
    pub public_contract_effect: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticImpact {
    pub behavioral: String,
    pub public_contract: String,
    pub affected: Vec<String>,
    pub query_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogueDefinition {
    pub rule_id: String,
    pub category: String,
    pub summary: String,
    pub reason: String,
    pub recommended_title: String,
    pub repair_kind: RepairKind,
    pub decision_owner: DecisionOwner,
    pub help_id: String,
    pub context_keys: Vec<&'static str>,
    pub fixtures: Vec<&'static str>,
    /// True only when the public copy is deliberately authored for this rule.
    /// Structural fallbacks remain visible as catalogue debt and cannot satisfy
    /// the strict diagnostic-copy conformance gate.
    pub authored_copy: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilerDiagnostic {
    pub code: &'static str,
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub reason: String,
    pub recommended_next_step: Box<RepairStep>,
    pub alternatives: Vec<RepairStep>,
    pub decision_owner: DecisionOwner,
    pub help_id: String,
    pub source_revision: String,
    pub context: Vec<(String, String)>,
    pub impact: Box<DiagnosticImpact>,
    pub primary: Option<SourceSpan>,
    pub related: Vec<SourceSpan>,
    pub notes: Vec<String>,
}

/// Compatibility name used by the existing compiler crates.
pub type Diagnostic = CompilerDiagnostic;

pub fn diagnostic_help_url(help_id: &str) -> String {
    format!("https://jadpo.dev/docs/{help_id}")
}

impl CompilerDiagnostic {
    pub fn error(code: &'static str) -> Self {
        Self::new(code, Severity::Error)
    }

    fn new(code: &'static str, severity: Severity) -> Self {
        let definition = catalogue_definition(code);
        let (behavioral_impact, public_contract_impact) = match severity {
            Severity::Error => (
                "Compilation is blocked until this problem is fixed.",
                "No public-contract change has been applied.",
            ),
            Severity::Warning => (
                "The project can still build; this recommendation is advisory.",
                "No public-contract change has been applied.",
            ),
            Severity::Note => (
                "This information does not block compilation.",
                "No public-contract change has been applied.",
            ),
        };
        let recommended_next_step = RepairStep {
            kind: definition.repair_kind,
            title: definition.recommended_title.clone(),
            reason: definition.reason.clone(),
            decision_owner: definition.decision_owner,
            preferred: definition.repair_kind == RepairKind::AutomaticFix,
            edits: Vec::new(),
            behavioral_effect:
                "No behavioural change is applied until the proposed edit is validated.".to_owned(),
            public_contract_effect: "Review generated contract impact before applying the edit."
                .to_owned(),
        };
        let alternatives = default_alternatives(code, &definition);
        let mut diagnostic = Self {
            code,
            rule_id: definition.rule_id,
            severity,
            message: definition.summary.clone(),
            reason: definition.reason,
            recommended_next_step: Box::new(recommended_next_step),
            alternatives,
            decision_owner: definition.decision_owner,
            help_id: definition.help_id,
            source_revision: "unversioned".to_owned(),
            context: Vec::new(),
            impact: Box::new(DiagnosticImpact {
                behavioral: behavioral_impact.to_owned(),
                public_contract: public_contract_impact.to_owned(),
                affected: Vec::new(),
                query_id: None,
            }),
            primary: None,
            related: Vec::new(),
            notes: Vec::new(),
        };
        if code == "ROUTE_AUTH_VALUE_INVALID" {
            "Compilation is blocked until the route authentication boundary is chosen."
                .clone_into(&mut diagnostic.impact.behavioral);
            "The public route security boundary is unresolved; the compiler will not guess."
                .clone_into(&mut diagnostic.impact.public_contract);
            "No authentication behavior changes until a human chooses a valid boundary."
                .clone_into(&mut diagnostic.recommended_next_step.behavioral_effect);
            "The route's public security contract remains unchanged while compilation is blocked."
                .clone_into(&mut diagnostic.recommended_next_step.public_contract_effect);
        }
        diagnostic
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn warning(code: &'static str) -> Self {
        Self::new(code, Severity::Warning)
    }

    pub fn with_source_revision(mut self, revision: impl Into<String>) -> Self {
        self.source_revision = revision.into();
        self
    }

    pub fn with_fact(mut self, fact: DiagnosticFact) -> Self {
        let (key, value) = fact.into_pair();
        let definition = catalogue_definition(self.code);
        if definition.context_keys.contains(&key) {
            if let Some((_, current)) = self.context.iter_mut().find(|(current, _)| current == key)
            {
                *current = value;
            } else if self.context.len() < 16 {
                self.context.push((key.to_owned(), value));
            }
            self.refresh_catalogue_copy();
        }
        self
    }

    fn refresh_catalogue_copy(&mut self) {
        let definition = catalogue_definition(self.code);
        self.message = render_catalogue_text(&definition.summary, &self.context);
        self.reason = render_catalogue_text(&definition.reason, &self.context);
        self.recommended_next_step.title =
            render_catalogue_text(&definition.recommended_title, &self.context);
        self.recommended_next_step.reason.clone_from(&self.reason);
        self.refresh_semantic_name_copy();
        self.refresh_type_copy();
        self.refresh_failure_copy();
    }

    fn refresh_semantic_name_copy(&mut self) {
        let value = |key: &str| {
            self.context
                .iter()
                .find_map(|(candidate, value)| (candidate == key).then_some(value.as_str()))
        };
        let Some(name) = value("name") else {
            return;
        };
        let expected = value("expected").unwrap_or("declaration");
        let suggestion = value("suggestedName");
        let examples = value("examples");
        let usage = value("usage");

        match self.code {
            "SEM_DUPLICATE_DECLARATION" => {
                self.message = format!("`{name}` is already defined");
                self.reason = format!(
                    "There is already a visible declaration named `{name}`, so references would not know which one to use."
                );
                "Rename this declaration or remove the duplicate"
                    .clone_into(&mut self.recommended_next_step.title);
            }
            "SEM_UNKNOWN_NAME" => {
                self.message = format!("`{name}` isn't defined");
                self.reason = semantic_expected_reason(expected, examples, suggestion, usage);
                self.recommended_next_step.title =
                    semantic_unknown_name_guidance(name, expected, suggestion);
            }
            "SEM_WRONG_NAME_KIND" => {
                self.message = format!("`{name}` can't be used here");
                self.reason = semantic_expected_reason(expected, examples, None, usage);
                self.recommended_next_step.title = semantic_replacement_guidance(expected);
            }
            "SEM_UNKNOWN_CALLEE" => {
                self.message = format!("`{name}` isn't defined");
                self.reason =
                    semantic_expected_reason("function or action", None, suggestion, usage);
                self.recommended_next_step.title = suggestion.map_or_else(
                    || format!("Define `{name}` as a function or action, or import it"),
                    |suggested| {
                        format!(
                            "Use `{suggested}`, define `{name}` as a function or action, or import it"
                        )
                    },
                );
            }
            "SEM_NOT_CALLABLE" => {
                self.message = format!("`{name}` can't be called");
                "Only a function or action can be followed by parentheses and called."
                    .clone_into(&mut self.reason);
                "Call a function or action instead, or remove the parentheses"
                    .clone_into(&mut self.recommended_next_step.title);
            }
            _ => return,
        }
        self.recommended_next_step.reason.clone_from(&self.reason);
    }

    fn refresh_type_copy(&mut self) {
        let value = |key: &str| {
            self.context
                .iter()
                .find_map(|(candidate, value)| (candidate == key).then_some(value.as_str()))
        };
        let name = value("name");
        let subject = value("subject");
        let field = value("field");
        let expected = value("expected");
        let received = value("received");
        let suggestion = value("suggestedName");

        match self.code {
            "TYPE_MISMATCH" | "TYPE_SIBLING_MISMATCH" => {
                let (Some(expected), Some(received)) = (expected, received) else {
                    return;
                };
                self.message = format!("`{received}` can't be used where `{expected}` is required");
                self.reason = if self.code == "TYPE_SIBLING_MISMATCH" {
                    format!(
                        "`{received}` and `{expected}` are separate named types, even if they build on the same base type."
                    )
                } else {
                    format!("This value has type `{received}`, but this part of the source needs `{expected}`.")
                };
                self.recommended_next_step.title = format!("Use a `{expected}` value here");
            }
            "TYPE_ARGUMENT_COUNT" | "TYPE_CONSTRUCTOR_ARGUMENT_COUNT" => {
                let (Some(expected_count), Some(received_count)) =
                    (value("expectedCount"), value("receivedCount"))
                else {
                    return;
                };
                let subject = name.unwrap_or("This call");
                self.message = format!(
                    "`{subject}` expects {expected_count} {}, but received {received_count}",
                    plural("argument", expected_count)
                );
                "Each declared parameter needs one value in the same order."
                    .clone_into(&mut self.reason);
                self.recommended_next_step.title = format!(
                    "Pass {expected_count} {} to `{subject}`",
                    plural("argument", expected_count)
                );
            }
            "TYPE_CONSTRUCTOR_INPUT" => {
                let (Some(expected), Some(received)) = (expected, received) else {
                    return;
                };
                let constructor = name.unwrap_or("This type");
                self.message = format!("`{constructor}` can't be constructed from `{received}`");
                self.reason = format!("`{constructor}` builds on `{expected}`, so its value must be compatible with `{expected}`.");
                self.recommended_next_step.title =
                    format!("Pass a `{expected}` value to `{constructor}`");
            }
            "TYPE_MISSING_FIELD" | "TYPE_MISSING_VARIANT_FIELD" => {
                let (Some(subject), Some(field)) = (subject, field) else {
                    return;
                };
                self.message = format!("`{subject}` is missing the required `{field}` field");
                self.reason = format!("`{field}` is required by the `{subject}` declaration.");
                self.recommended_next_step.title = format!("Add `{field}: ...` to `{subject}`");
            }
            "TYPE_UNKNOWN_FIELD" | "TYPE_UNKNOWN_VARIANT_FIELD" => {
                let (Some(subject), Some(field)) = (subject, field) else {
                    return;
                };
                self.message = format!("`{subject}` has no `{field}` field");
                self.reason = suggestion.map_or_else(
                    || format!("`{field}` is not declared on `{subject}`."),
                    |suggested| {
                        format!(
                            "`{field}` is not declared on `{subject}`. Did you mean `{suggested}`?"
                        )
                    },
                );
                self.recommended_next_step.title = suggestion.map_or_else(
                    || format!("Use a field declared on `{subject}`"),
                    |suggested| format!("Use `{suggested}` instead of `{field}`"),
                );
            }
            "TYPE_UNKNOWN_VALUE" => {
                let Some(name) = name else { return };
                self.message = format!("`{name}` isn't defined in this scope");
                self.reason = suggestion.map_or_else(
                    || "Jadpo could not find a parameter or local value with this name.".to_owned(),
                    |suggested| {
                        format!("Jadpo could not find this name. Did you mean `{suggested}`?")
                    },
                );
                self.recommended_next_step.title = suggestion.map_or_else(
                    || format!("Define `{name}` before using it"),
                    |suggested| format!("Use `{suggested}` instead of `{name}`"),
                );
            }
            "TYPE_UNKNOWN_ENUM_VARIANT" => {
                let (Some(subject), Some(name)) = (subject, name) else {
                    return;
                };
                self.message = format!("`{subject}` has no `{name}` variant");
                self.reason = suggestion.map_or_else(
                    || format!("`{name}` is not one of the variants declared by `{subject}`."),
                    |suggested| {
                        format!(
                            "`{name}` is not declared by `{subject}`. Did you mean `{suggested}`?"
                        )
                    },
                );
                self.recommended_next_step.title = suggestion.map_or_else(
                    || format!("Use a variant declared by `{subject}`"),
                    |suggested| format!("Use `{subject}.{suggested}`"),
                );
            }
            "TYPE_FIELD_ON_NON_RECORD" => {
                let (Some(received), Some(field)) = (received, field) else {
                    return;
                };
                self.message = format!("`{received}` values have no `{field}` field");
                self.reason = format!("`{received}` is not an object with named fields.");
                "Use an object value before `.`, or remove the field access"
                    .clone_into(&mut self.recommended_next_step.title);
            }
            "TYPE_REDUNDANT_NULLABILITY" => {
                let Some(name) = name else { return };
                self.message = format!("`{name}` already permits `none`");
                self.reason = format!("`{name}` inherits nullability from its declared type. A second `?` does not create a distinct kind of absence.");
                self.recommended_next_step.title =
                    format!("Remove the redundant `?` after `{name}`");
            }
            "TYPE_NULLABLE_SELECTION" => {
                let (Some(received), Some(field)) = (received, field) else {
                    return;
                };
                self.message =
                    format!("`{received}` may be `none`, so `{field}` can't be read yet");
                "Jadpo needs the `some(...)` case before it can safely read this field."
                    .clone_into(&mut self.reason);
                "Match the nullable value and read the field inside `some(...)`"
                    .clone_into(&mut self.recommended_next_step.title);
            }
            "TYPE_PRIMITIVE_SIGNATURE" => {
                let (Some(received), Some(usage)) = (received, value("usage")) else {
                    return;
                };
                self.message = format!("This {usage} uses `{received}` directly");
                self.reason = format!("A {usage} needs a named Jadpo type so its meaning and validation rules are explicit.");
                self.recommended_next_step.title =
                    format!("Define a named type based on `{received}` and use it here");
            }
            "TYPE_INVALID_LITERAL" => {
                let Some(subject) = subject else { return };
                self.message = format!("This value doesn't meet `{subject}`'s rules");
                if let Some(constraint) = value("constraint") {
                    self.reason = format!(
                        "The value does not satisfy `{subject}`'s `{constraint}` constraint."
                    );
                    self.recommended_next_step.title =
                        format!("Change the value so it satisfies `{constraint}`");
                }
            }
            _ => return,
        }
        self.recommended_next_step.reason.clone_from(&self.reason);
    }

    fn refresh_failure_copy(&mut self) {
        let value = |key: &str| {
            self.context
                .iter()
                .find_map(|(candidate, value)| (candidate == key).then_some(value.as_str()))
        };
        let callable = value("callable");
        let failure = value("failure");
        let field = value("field");

        match self.code {
            "EFFECT_FUNCTION_CALLS_ACTION" => {
                let (Some(function), Some(action)) = (callable, value("name")) else {
                    return;
                };
                self.message = format!("Function `{function}` can't call action `{action}`");
                self.reason = format!("`{function}` is a function, so it cannot run persistence or other action effects through `{action}`.");
                self.recommended_next_step.title = format!(
                    "Move this call into an action, or call a function instead of `{action}`"
                );
            }
            "EFFECT_FUNCTION_PERSISTENCE" => {
                let Some(function) = callable else { return };
                self.message = format!("Function `{function}` can't use persistence");
                "Create, query, update, and delete operations are available only inside actions."
                    .clone_into(&mut self.reason);
                "Move this persistence operation into an action"
                    .clone_into(&mut self.recommended_next_step.title);
            }
            "FAIL_ATTEMPT_REQUIRED" => {
                let usage = value("usage").unwrap_or("operation");
                self.message = format!("This {usage} can fail and requires `attempt`");
                "`attempt` makes it clear that a declared failure may leave the current function or action."
                    .clone_into(&mut self.reason);
                self.recommended_next_step.title = format!("Add `attempt` before this {usage}");
            }
            "FAIL_CONTEXT_FIELD_OVERLAP" => {
                let (Some(failure), Some(field)) = (failure, field) else {
                    return;
                };
                self.message = format!("`{failure}.{field}` is both public and internal");
                "The same field cannot be returned to callers and hidden from them at the same time."
                    .clone_into(&mut self.reason);
                self.recommended_next_step.title =
                    format!("Keep `{field}` in either `public` or `internal`, not both");
            }
            "FAIL_DUPLICATE_CODE" => {
                let (Some(code), Some(first), Some(second)) =
                    (value("codeValue"), value("otherFailure"), failure)
                else {
                    return;
                };
                self.message = format!("Failure code `{code}` is already used by `{first}`");
                self.reason = format!("`{second}` and `{first}` cannot share a public code because callers use it to identify one failure.");
                self.recommended_next_step.title =
                    format!("Give `{second}` a different stable public code");
            }
            "FAIL_DUPLICATE_CONTEXT_FIELD" => {
                let (Some(failure), Some(field)) = (failure, field) else {
                    return;
                };
                self.message = format!("`{field}` is supplied more than once for `{failure}`");
                self.reason =
                    format!("A rejected `{failure}` value can contain one `{field}` value.");
                self.recommended_next_step.title =
                    format!("Keep one `{field}: ...` entry in this rejection");
            }
            "FAIL_DUPLICATE_DECLARATION" => {
                let (Some(callable), Some(failure)) = (callable, failure) else {
                    return;
                };
                self.message = format!("`{failure}` is listed more than once after `fails`");
                self.reason =
                    format!("Listing `{failure}` twice does not change what `{callable}` can do.");
                self.recommended_next_step.title =
                    format!("Keep one `{failure}` entry after `fails`");
            }
            "FAIL_MISSING_CONTEXT_FIELD" => {
                let (Some(failure), Some(field)) = (failure, field) else {
                    return;
                };
                let scope = value("scope").unwrap_or("required");
                self.message = format!("`{failure}` is missing the {scope} `{field}` field");
                self.reason = format!("`{field}` is required whenever `{failure}` is rejected.");
                self.recommended_next_step.title =
                    format!("Add `{field}: ...` to this `{failure}` rejection");
            }
            "FAIL_MUTATION_CONFLICT_NOT_CONFLICT"
            | "FAIL_PATCH_EMPTY_NOT_INVALID_VALUE"
            | "FAIL_REQUIRED_MUTATION_NOT_NOT_FOUND"
            | "FAIL_REQUIRED_QUERY_NOT_NOT_FOUND" => {
                let (Some(failure), Some(expected), Some(received)) =
                    (failure, value("expected"), value("received"))
                else {
                    return;
                };
                let usage = value("usage").unwrap_or("this failure binding");
                self.message =
                    format!("`{failure}` uses `{received}`, but {usage} requires `{expected}`");
                self.reason =
                    format!("`{failure}` must use the predefined `{expected}` category here.");
                self.recommended_next_step.title =
                    format!("Use a failure declared with `kind {expected}`");
            }
            "FAIL_STALE_DECLARATION" => {
                let (Some(callable), Some(failures)) = (callable, value("failures")) else {
                    return;
                };
                self.message = format!("`{failures}` can never leave `{callable}`");
                self.reason = format!("`{callable}` lists this after `fails`, but no call or `reject` can produce it.");
                self.recommended_next_step.title =
                    format!("Remove `{failures}` from `{callable}`'s `fails` list");
            }
            "FAIL_UNDECLARED_PROPAGATION" => {
                let (Some(callable), Some(failure)) = (callable, failure) else {
                    return;
                };
                self.message =
                    format!("`{failure}` can leave `{callable}` but isn't listed after `fails`");
                self.reason =
                    format!("Callers of `{callable}` need to know that `{failure}` can happen.");
                self.recommended_next_step.title =
                    format!("Add `{failure}` after `fails`, or handle it inside `{callable}`");
            }
            "FAIL_UNKNOWN_CONTEXT_FIELD" => {
                let (Some(failure), Some(field)) = (failure, field) else {
                    return;
                };
                self.message = format!("`{failure}` has no `{field}` field");
                self.reason = format!(
                    "`{field}` is not declared in the public or internal fields of `{failure}`."
                );
                self.recommended_next_step.title =
                    format!("Remove `{field}`, or declare it on `{failure}`");
            }
            _ => return,
        }
        self.recommended_next_step.reason.clone_from(&self.reason);
    }

    pub fn with_impact(mut self, affected: impl Into<String>) -> Self {
        if self.impact.affected.len() < 16 {
            self.impact.affected.push(affected.into());
        }
        self
    }

    pub fn with_related(mut self, span: SourceSpan) -> Self {
        if self.related.len() < 16 {
            self.related.push(span);
        }
        self
    }

    pub fn with_edit(mut self, edit: TextEdit) -> Self {
        self.recommended_next_step.edits.push(edit);
        self
    }

    pub fn to_json(&self) -> String {
        let related = self
            .related
            .iter()
            .map(|span| {
                format!(
                    "{{\"source\":{},\"range\":{{\"start\":{},\"end\":{}}}}}",
                    json_string(&span.source),
                    span.start,
                    span.end
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let location = self.primary.as_ref().map_or_else(
            || "null".to_owned(),
            |primary| {
                format!(
                    "{{\"source\":{},\"range\":{{\"start\":{},\"end\":{}}},\"related\":[{related}]}}",
                    json_string(&primary.source),
                    primary.start,
                    primary.end
                )
            },
        );
        let diagnostic_id = self.primary.as_ref().map_or_else(
            || format!("{}@global", self.rule_id),
            |primary| {
                format!(
                    "{}@{}:{}:{}",
                    self.rule_id, primary.source, primary.start, primary.end
                )
            },
        );
        let context = self
            .context
            .iter()
            .map(|(key, value)| format!("{}:{}", json_string(key), json_string(value)))
            .collect::<Vec<_>>()
            .join(",");
        let affected = self
            .impact
            .affected
            .iter()
            .map(|value| json_string(value))
            .collect::<Vec<_>>()
            .join(",");
        let alternatives = self
            .alternatives
            .iter()
            .map(repair_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"schemaVersion\":2,\"diagnosticId\":{},\"sourceRevision\":{},\"summary\":{},\"reason\":{},\"recommendedNextStep\":{},\"alternatives\":[{alternatives}],\"ruleId\":{},\"severity\":{},\"location\":{location},\"context\":{{{context}}},\"impact\":{{\"behavioral\":{},\"publicContract\":{},\"affected\":[{affected}],\"queryId\":{}}},\"helpId\":{},\"decisionOwner\":{},\"legacyAliases\":[{}]}}",
            json_string(&diagnostic_id),
            json_string(&self.source_revision),
            json_string(&self.message),
            json_string(&self.reason),
            repair_json(&self.recommended_next_step),
            json_string(&self.rule_id),
            json_string(&self.severity.to_string()),
            json_string(&self.impact.behavioral),
            json_string(&self.impact.public_contract),
            self.impact.query_id.as_deref().map(json_string).unwrap_or_else(|| "null".to_owned()),
            json_string(&self.help_id),
            json_string(self.decision_owner.as_str()),
            json_string(self.code),
        )
    }
}

fn repair_json(repair: &RepairStep) -> String {
    let edits = repair
        .edits
        .iter()
        .map(|edit| {
            format!(
                "{{\"source\":{},\"range\":{{\"start\":{},\"end\":{}}},\"replacement\":{}}}",
                json_string(&edit.source),
                edit.start,
                edit.end,
                json_string(&edit.replacement)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"kind\":{},\"title\":{},\"reason\":{},\"decisionOwner\":{},\"preferred\":{},\"edits\":[{edits}],\"preview\":{{\"behavioral\":{},\"publicContract\":{}}}}}",
        json_string(repair.kind.as_str()),
        json_string(&repair.title),
        json_string(&repair.reason),
        json_string(repair.decision_owner.as_str()),
        repair.preferred,
        json_string(&repair.behavioral_effect),
        json_string(&repair.public_contract_effect),
    )
}

#[derive(Clone, Copy)]
struct AuthoredCopy {
    summary: &'static str,
    reason: &'static str,
    next: &'static str,
}

fn job_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "SYN_JOB_EVERY_REQUIRED" => ("Job requires a fixed interval", "A scheduled job uses `job name every duration` before its closed body.", "Write `every` before the interval"),
        "SYN_JOB_DURATION_REQUIRED" | "TYPE_JOB_INTERVAL_INVALID" => ("Job interval must be a positive finite duration", "This slice accepts representable integral milliseconds with ms, s, m or h units, not bare numbers, strings, zero, negative values or cron expressions.", "Use a supported positive duration such as `15m`"),
        "SYN_JOB_ITEM_DUPLICATE" => ("Job clause is duplicated", "Every job body declares concurrency, run and retry exactly once; a later value cannot replace an earlier clause.", "Remove the duplicate clause"),
        "SYN_JOB_CLAUSE_VALUE_INVALID" => ("Unsupported job clause value", "This checked schedule slice supports only concurrency: singleton and retry: next_schedule. The retry clause selects a wake-up, not a retry budget.", "Use the closed supported clause value"),
        "SYN_JOB_RUN_INVOCATION_REQUIRED" => ("Job run requires an action invocation", "A job run binds a checked action with explicit argument mapping, not a bare name or inline body.", "Write `run: action_name(JobRunAt(clock.now))` with the action's exact nominal snapshot type"),
        "SYN_JOB_ITEM_UNKNOWN" => ("Unsupported item inside job", "A job body accepts concurrency:, run:, retry: and an optional closed delivery: reminder_v1 descriptor. Delivery analysis remains unsupported; principal, intent, fences, outcomes and receipts remain compiler-owned.", "Remove the unsupported job item"),
        "SYN_JOB_CLAUSE_REQUIRED" => ("Job is missing a required clause", "The closed schedule entry needs one valid concurrency: singleton, run invocation and retry: next_schedule clause.", "Supply the missing supported clause"),
        "SYN_JOB_EXPORT_INVALID" => ("Scheduled jobs cannot be exported", "A job is an application schedule entry, not a callable or public module value.", "Remove public from the job declaration"),
        "SYN_JOB_DELIVERY_MODE_INVALID" => ("Unsupported delivery descriptor mode", "Only the closed reminder_v1 source descriptor is parsed; it grants no execution or authority.", "Use the reviewed reminder_v1 descriptor"),
        "SYN_JOB_DELIVERY_KEY_UNKNOWN" => ("Unknown delivery descriptor key", "Each descriptor section accepts only its closed key catalogue, not arbitrary expressions or configuration.", "Remove the unknown key"),
        "SYN_JOB_DELIVERY_KEY_DUPLICATE" => ("Duplicate delivery descriptor key", "Every required section and key occurs exactly once; later values cannot replace earlier ones.", "Remove the duplicate key"),
        "SYN_JOB_DELIVERY_KEY_REQUIRED" => ("Missing valid delivery descriptor key", "All five sections and all their closed keys require valid source values; a partial descriptor is not checked.", "Supply each required valid key"),
        "SYN_JOB_DELIVERY_TERM_INVALID" => ("Unsupported delivery descriptor term", "Compiler-origin descriptor terms are closed and position-specific, not ordinary values or caller-provided authority.", "Use the reviewed term for this slot"),
        "SYN_JOB_DELIVERY_REFERENCE_INVALID" => ("Delivery slot requires a direct qualified reference", "This slot accepts a checked field/configuration/operation reference, not an invocation, raw record or generated intent value.", "Use a direct qualified source reference"),
        "SYN_JOB_DELIVERY_LIMIT_INVALID" => ("Delivery scan requires literal 500", "This closed descriptor has a fixed finite scan bound; dynamic, larger and unbounded limits are not supported.", "Write limit: 500"),
        "SYN_JOB_DELIVERY_VERSION_INVALID" => ("Unsupported delivery payload version", "The immutable closed payload uses the reviewed reminder.v1 version.", "Write payload_version: \"reminder.v1\""),
        "SYN_JOB_DELIVERY_DIRECTION_INVALID" => ("Delivery cursor requires ascending order", "The two-field ordered keyset retains due then identity order; each direction is asc.", "Use the reviewed ordered ascending fields"),
        "SYN_JOB_DELIVERY_SYNTAX_INVALID" => ("Malformed closed delivery descriptor", "Sections, references, open enum predicate and two-field cursor use the exact closed descriptor grammar.", "Restore the reviewed delivery descriptor syntax"),
        "TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED" => ("Delivery binding is not yet checked", "Parsing this descriptor does not establish sealed intent origins, mutation hooks or phase-qualified authority. Checked delivery analysis and worker lowering remain unimplemented.", "Keep worker execution disabled until the binding and profile gates pass"),
        "TYPE_JOB_DELIVERY_SELECTOR_INVALID" => ("Delivery selector does not match its entity types", "Selection requires authoritative same-store entities with visibility contracts, an exact nonnullable Uuid identity, distinct mutable nullable Instant due/sent fields, an exact enum variant, due-then-identity cursor and a required relationship to the recipient identity. These necessary type checks do not establish a checked delivery binding or runtime authority.", "Match the selector to the checked entity identity, fields and required recipient reference"),
        "TYPE_JOB_DELIVERY_SERVICE_INVALID" => ("Delivery service does not match its closed payload types", "The selected checked operation must use the distinct direct nominal Uuid intent and exact closed value request/receipt, with required nonsecret mapped configuration and recipient fields. Resolved candidates do not grant delivery authority or execution.", "Match the selected imported operation, nominal intent and exact payload/receipt field contracts"),
        "TYPE_JOB_DELIVERY_SEALED_USE" => ("Compiler-owned delivery value cannot be authored or exposed", "The selected intent, request and receipt types are confined to their exact service contract declarations. Ordinary aliases, containment, signatures, constructors, values and selected provider calls cannot manufacture or carry delivery authority; unbound service types remain ordinary.", "Remove the ordinary use of the selected delivery contract or provider operation"),
        "TYPE_JOB_DELIVERY_HOOK_INVALID" => ("Delivery hooks or competing writes cannot be proved", "A delivery binding requires one exact direct create and optional-due patch site, an effect-free run action, and a complete source census with no other creation or due/sent-field writes. Unknown patch provenance is not a disjoint write proof. These structural checks do not establish transaction or runtime behavior.", "Use the exact compatible mutation sites and remove competing or unproved writes"),
        "TYPE_JOB_DELIVERY_AUTHORITY_INVALID" => ("Delivery authority does not compose to its selected service identity", "The selected API-key credential reference, active service resolution and required principal identity mapping must compose to the exact Service identity and selected authoritative application membership/role. Shared primitive types or another membership's role union do not prove this binding. Static composition is not a live grant or worker admission.", "Match the exact service identity, credential, resolution and selected same-store membership without competing bindings"),
        "TYPE_JOB_RUN_SIGNATURE_INVALID" => ("Job run requires a nominal snapshot-to-Unit action", "The statically named action must have one nonnullable, unconstrained named type directly based on Instant, and builtin Unit result. The ordinary primitive-signature ban remains; functions, queries, service operations and jobs do not qualify.", "Bind a checked action taking a direct unconstrained Instant wrapper"),
        "TYPE_JOB_RUN_ARGUMENT_INVALID" => ("Job run requires an explicit nominal clock snapshot", "The first schedule slice passes the action's exact nominal constructor containing only intrinsic clock.now. Bare clocks, sibling wrappers, named, extra, arbitrary nested or principal arguments reject.", "Pass the exact snapshot type's constructor around clock.now"),
        "JADPO_TARGET_JOB_NOT_IMPLEMENTED" => ("Scheduled job `{name}` cannot execute yet", "The frontend checks this schedule, but a reviewed finite execution profile, durable failure dispositions and checked worker lowering are not implemented. Build stops rather than emitting a no-op or startup call.", "Retain the checked schedule while completing its worker binding and execution gates"),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn syntax_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "SYN_CONSTRAINT_NON_ENTITY" => (
            "Persistence constraints require a persisted object",
            "This constraint describes stored data, so its object type needs a separate `persist Type { ... }` declaration.",
            "Move the constraint into the object's `persist` declaration, or remove it",
        ),
        "SYN_CONSTRAINT_COLON_REQUIRED" => (
            "Type constraint requires a `:` separator",
            "Every type constraint uses the explicit `name: value` form. The old `format email` spelling is not valid.",
            "Insert `:` after the constraint name",
        ),
        "SYN_DUPLICATE_FIELD_MODIFIER" => (
            "Field modifier is repeated",
            "Each field modifier may be written once. Repeating a modifier makes the field's authored storage or validation contract ambiguous.",
            "Keep one occurrence of the repeated field modifier",
        ),
        "SYN_EMPTY_IMPORT" => (
            "Import selects no declarations",
            "A selective import must name at least one declaration from the target module; an empty selection introduces no usable name.",
            "Add at least one imported declaration name, or remove the import",
        ),
        "SYN_ENUM_EMPTY" => (
            "Enum declares no variants",
            "An enum is a closed set of named values and must contain at least one variant so construction and matching have a defined domain.",
            "Add at least one enum variant",
        ),
        "SYN_EXPECTED_CONSTRAINT" => (
            "Expected a type constraint",
            "A type constraint block accepts only supported constraint names such as length, numeric bound, pattern, or format constraints.",
            "Use a supported constraint name or remove the invalid item",
        ),
        "SYN_EXPECTED_CONSTRAINT_VALUE" => (
            "Constraint requires a value",
            "The preceding constraint name is incomplete without the literal or name that defines its bound, pattern, or format.",
            "Add a value of the kind required by this constraint",
        ),
        "SYN_EXPECTED_DECLARATION" => (
            "`{found}` cannot start a top-level declaration",
            "Jadpo files accept `type`, `persist`, `failure`, `function`, `action`, `test`, and `route` declarations at the top level. Optional `module` and `import` headers come before those declarations. Route items such as `path:` belong inside a `route` block, so `{found}` cannot be parsed here.",
            "Move `{found}` into its owning declaration, or replace it with a top-level declaration",
        ),
        "SYN_EXPECTED_DELETE_ACTION" => (
            "Delete requires a persisted object type",
            "A `delete required` expression must name the persisted object whose stored row will be removed before its predicate and failure bindings can be parsed.",
            "Add the persisted object type after `delete required`",
        ),
        "SYN_EXPECTED_EXPRESSION" => (
            "Expected an expression",
            "Jadpo needs a value here, such as a literal, name, function call, constructed value, match, or persistence operation.",
            "Add the value or operation you intended",
        ),
        "SYN_EXPECTED_FAILURE_ITEM" => (
            "Failure declaration contains an unsupported item",
            "A failure body accepts `kind`, `code`, `message`, `public`, and `internal` items only.",
            "Replace this item with a supported failure member or remove it",
        ),
        "SYN_EXPECTED_FIELD" => (
            "Expected a field declaration",
            "Object and field blocks contain `name: Type` declarations. The current source cannot begin a field and is not a valid block terminator.",
            "Write a `name: Type` field or remove the stray source",
        ),
        "SYN_FAILURE_ITEM_COLON_REQUIRED" => (
            "Failure member requires a `:` separator",
            "Failure metadata uses the same explicit `name: value` form as other named settings, such as `kind: NotFound` and `code: \"customer_not_found\"`.",
            "Insert `:` after the failure member name",
        ),
        "SYN_EXPECTED_FIELD_INITIALISER" => (
            "Expected a field value",
            "A constructor or object body assigns fields with `name: expression`; each item needs a field name before its value.",
            "Add a named field initialiser or remove the stray item",
        ),
        "SYN_EXPECTED_HTTP_METHOD" => (
            "Route requires an HTTP method",
            "A route declaration starts with exactly one of `GET`, `POST`, `PUT`, `PATCH`, or `DELETE` before its path.",
            "Add a supported HTTP method before the route path",
        ),
        "SYN_EXPECTED_INVERSE_CARDINALITY" => (
            "Inverse relationship requires `optional` or `many`",
            "Jadpo needs to know whether this relationship can return at most one record or a list of records.",
            "Add `optional` or `many` to the inverse relationship",
        ),
        "SYN_EXPECTED_INVOCATION" => (
            "Route `run:` requires an action invocation",
            "Named route behavior calls an action with parentheses. A bare name or another expression does not define the route's argument mapping.",
            "Call the action using `name(arguments)` after `run:`",
        ),
        "SYN_EXPECTED_LITERAL" => (
            "Expected a literal value",
            "Jadpo needs a value written directly here, such as text, a number, `true`, `false`, or `none`.",
            "Replace this with the required literal value",
        ),
        "SYN_EXPECTED_NAME" => (
            "Expected a name",
            "Jadpo needs the name of a declaration, field, local value, or qualified path here.",
            "Add a valid name beginning with a letter or underscore",
        ),
        "SYN_EXPECTED_ORDER_DIRECTION" => (
            "Query ordering requires `asc` or `desc`",
            "Every `order_by` field needs an explicit direction so result ordering is deterministic across storage adapters.",
            "Add `asc` or `desc` after the ordering field",
        ),
        "SYN_EXPECTED_PATCH_INPUT" => (
            "Patch update requires an omission-aware object",
            "The `patch:` item must name the input value whose supplied fields will be applied to the persisted object.",
            "Add the patch input value after `patch:`",
        ),
        "SYN_EXPECTED_ROUTE_ITEM" => (
            "Unsupported item inside route",
            "A route body accepts `auth:`, `path:`, `query:`, `headers:`, `deadline:`, `input:`, `output:`, `success:`, `run:`, and `action:` items only.",
            "Use a supported route item or move this source outside the route",
        ),
        "SYN_EXPECTED_STATEMENT" => (
            "Expected a statement",
            "Callable and test blocks contain supported statements such as variable declarations, return, reject, conditionals, matches, assertions, and persistence operations.",
            "Add a supported statement or remove the stray source",
        ),
        "SYN_EXPECTED_SUPPLIED" => (
            "Patch condition requires `supplied`",
            "A conditional patch assignment must explicitly test whether an optional input field was supplied before using its value.",
            "Add `supplied` with the optional patch field",
        ),
        "SYN_EXPECTED_SUPPLIED_FIELD" => (
            "`supplied` requires an input field",
            "The `supplied` condition must name the optional input field whose presence controls the conditional patch assignment.",
            "Add the optional input field after `supplied`",
        ),
        "SYN_EXPECTED_UPDATE_BODY" => (
            "Update requires `set:` or `patch:`",
            "After its target and predicate, an update must choose either fixed authored changes with `set:` or omission-aware input changes with `patch:`.",
            "Add the intended `set:` or `patch:` update body",
        ),
        "SYN_FAILURE_CODE_REQUIRED" => (
            "Failure requires a stable public code",
            "Every failure declaration needs one string `code` so generated transports can identify the failure without exposing internal details.",
            "Add a unique string `code` to the failure declaration",
        ),
        "SYN_FAILURE_KIND_REQUIRED" => (
            "Failure requires a predefined category",
            "Every failure needs a `kind`, such as `InvalidValue`, `NotFound`, `Conflict`, or `Unavailable`, so Jadpo knows how to present it at a route boundary.",
            "Add a predefined category with `kind <Category>`",
        ),
        "SYN_IMPORT_REQUIRES_MODULE" => (
            "Imports require a module declaration",
            "A file that imports names must declare its own logical module first; otherwise visibility and dependency identity are undefined.",
            "Add a `module` header before the imports",
        ),
        "SYN_INVERSE_NON_ENTITY" => (
            "Inverse relationships require a persisted object",
            "An inverse relationship is derived from stored owning references, so it belongs in the object's `persist` declaration.",
            "Move the inverse relationship into `persist`, or remove it",
        ),
        "SYN_INVALID_ESCAPE" => (
            "String contains an unsupported escape sequence",
            "Jadpo strings support escapes for quotes, backslashes, newline, carriage return, and tab; this backslash sequence has no defined value.",
            "Use a supported string escape or remove the backslash",
        ),
        "SYN_MATCH_PATTERN" => (
            "Invalid match pattern",
            "A match arm must use a pattern supported by the matched value, such as a variant, Boolean, nullable pattern, or wildcard.",
            "Replace this source with a supported match pattern",
        ),
        "SYN_GENERATED_ON_REQUIRED" => (
            "Generated field requires an `on` lifecycle role",
            "Compiler-owned lifecycle timestamps are explicit. The generated block must state whether the value is set on creation or on creation and semantic change.",
            "Add `on: create` or `on: create_or_change`",
        ),
        "SYN_GENERATED_ROLE_INVALID" => (
            "Generated field lifecycle role is not supported",
            "The initial lifecycle contract has exactly two roles so timestamp ownership and update behavior remain predictable.",
            "Use `create` or `create_or_change`",
        ),
        "SYN_LIFECYCLE_DUPLICATE" => (
            "Entity declares lifecycle rules more than once",
            "An entity has one lifecycle contract so initial state, visibility, transitions, and retention share a single source of truth.",
            "Combine the lifecycle settings into one block",
        ),
        "SYN_LIFECYCLE_INITIAL_DUPLICATE" => (
            "Lifecycle initial state is declared more than once",
            "The entity lifecycle has one initial field map; multiple blocks could disagree about compiler-owned creation state.",
            "Keep one `initial` block containing every lifecycle-owned field",
        ),
        "SYN_LIFECYCLE_NON_ENTITY" => (
            "Lifecycle rules require an entity",
            "Lifecycle visibility and transitions are enforced against authoritative stored rows, so transient input and value records cannot own them.",
            "Move this lifecycle block to a persistent entity",
        ),
        "SYN_LIFECYCLE_PURGE_AFTER_REQUIRED" => (
            "Purge clause requires `after`",
            "Retention is expressed as a minimum duration after the lifecycle timestamp, using the fixed `purge after ... from ...` form.",
            "Write `purge after duration from timestamp_field`",
        ),
        "SYN_LIFECYCLE_PURGE_DUPLICATE" => (
            "Entity declares more than one lifecycle purge",
            "One retention rule determines the bounded physical-removal authority for an entity.",
            "Keep one `purge after ... from ...` clause",
        ),
        "SYN_LIFECYCLE_PURGE_FROM_REQUIRED" => (
            "Purge clause requires `from`",
            "Retention must identify the lifecycle timestamp field that anchors eligibility.",
            "Add `from timestamp_field` after the retention duration",
        ),
        "SYN_LIFECYCLE_SETTING_INVALID" => (
            "Lifecycle block contains an unsupported setting",
            "Lifecycle blocks accept only initial state, visibility, named transitions, and an optional retention purge clause.",
            "Replace this item with a supported lifecycle setting",
        ),
        "SYN_LIFECYCLE_TRANSITION_DUPLICATE" => (
            "Lifecycle transition setting or name is repeated",
            "A transition has one source predicate and one fixed assignment map, and transition names identify exactly one state change.",
            "Keep one `from`, one `set`, and one declaration for this transition name",
        ),
        "SYN_LIFECYCLE_TRANSITION_FROM_REQUIRED" => (
            "Lifecycle transition requires a source predicate",
            "The transition guard limits which current rows may enter its fixed state change.",
            "Add `from: predicate` to the transition",
        ),
        "SYN_LIFECYCLE_TRANSITION_SETTING_INVALID" => (
            "Lifecycle transition contains an unsupported setting",
            "A transition declares only its current-state `from` predicate and compiler-owned `set` values.",
            "Use `from:` or `set:` in the transition",
        ),
        "SYN_LIFECYCLE_TRANSITION_SET_REQUIRED" => (
            "Lifecycle transition requires fixed assignments",
            "The compiler must know the complete state change before an action invokes a named transition.",
            "Add `set: { field: value }` to the transition",
        ),
        "SYN_LIFECYCLE_VISIBLE_DUPLICATE" => (
            "Lifecycle visibility is declared more than once",
            "One visibility predicate must govern reads and mutations consistently for every ordinary caller.",
            "Keep one `visible when` predicate",
        ),
        "SYN_LIFECYCLE_VISIBLE_WHEN_REQUIRED" => (
            "Lifecycle visibility requires `when`",
            "Visibility is a same-row predicate and uses the explicit `visible when predicate` form.",
            "Write `visible when predicate`",
        ),
        "SYN_LOCALES_DEFAULT_REQUIRED" => (
            "Locale declaration requires one default locale",
            "Formatting fallback must be deterministic, so every locale set names one supported default BCP 47 locale.",
            "Add `default: \"...\"` to the locales block",
        ),
        "SYN_LOCALES_SUPPORTED_REQUIRED" => (
            "Locale declaration requires a supported locale list",
            "The generated `Locale` enum is closed over an explicit list rather than accepting host locale strings.",
            "Add `supported: [\"...\"]` to the locales block",
        ),
        "SYN_LOCALES_UNSUPPORTED_REQUIRED" => (
            "Locale declaration requires an unsupported-locale policy",
            "An application must decide whether an unsupported requested locale rejects or uses its declared default.",
            "Add `unsupported: reject` or `unsupported: fallback_to_default`",
        ),
        "SYN_LOCALES_UNSUPPORTED_INVALID" => (
            "Unsupported-locale policy is not recognised",
            "Only rejection or deterministic fallback to the declared default has defined behavior.",
            "Use `reject` or `fallback_to_default`",
        ),
        "SYN_MUTATION_CONFLICT_REQUIRED" => (
            "Mutation requires a conflict failure binding",
            "Required create, update, and delete operations must map storage conflicts into an authored domain failure rather than exposing adapter errors.",
            "Add at least one `conflict:` failure binding",
        ),
        "SYN_NAMED_ARGUMENT_DUPLICATE" => (
            "Named argument is repeated",
            "Each named argument identifies one parameter or option. Repeating a name would make the selected value ambiguous.",
            "Keep one value for this named argument",
        ),
        "SYN_POSITIONAL_AFTER_NAMED" => (
            "Positional argument follows a named argument",
            "Calls put positional values first and policy or option values after them by name. Mixing the order makes signatures harder to read and evolve safely.",
            "Move positional arguments before every named argument",
        ),
        "SYN_ROUTE_EXPORT_INVALID" => (
            "Routes cannot be exported with `public`",
            "Routes already define an external transport boundary. Module `public` exports apply to named declarations and cannot export an unnamed route.",
            "Remove `public` from the route declaration",
        ),
        "SYN_TYPE_PARENT_REQUIRED" => (
            "Refined type requires a parent type",
            "A type definition builds on an existing type, such as `Text`, `Uuid`, or another named type.",
            "Add the type it builds on after `=`",
        ),
        "SYN_UNEXPECTED_TOKEN" => (
            "Expected {expected}",
            "Found `{found}` while parsing this construct. Jadpo requires {expected} at this location, so parsing stops rather than guessing the authored structure.",
            "Provide {expected}",
        ),
        "SYN_UNEXPECTED_CHARACTER" => (
            "Character is not valid Jadpo syntax",
            "This character is not part of any Jadpo name, value, keyword, or punctuation.",
            "Remove the character or replace it with valid Jadpo syntax",
        ),
        "SYN_UNSUPPORTED_THROW" => (
            "Arbitrary `throw` is not supported",
            "Jadpo failures are declared, typed, and propagated through `reject` and `attempt`; arbitrary exceptions would escape that checked boundary.",
            "Replace `throw` with a declared failure and `reject`",
        ),
        "SYN_UNTERMINATED_STRING" => (
            "String literal is not closed",
            "The lexer reached the line or file boundary before finding the quote that ends this string literal.",
            "Add the closing quote, or remove the opening quote",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn semantic_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "SEM_NAME_CASE" => (
            "`{name}` must use {expected}",
            "This name's semantic owner determines one language-wide spelling. Type-like declarations use UpperCamelCase, while runtime names use lower_snake_case.",
            "Rename it to `{suggestedName}`",
        ),
        "SEM_STANDARD_NAMESPACE_RESERVED" => (
            "`{name}` is a compiler-owned standard namespace",
            "Standard-library namespaces are always available without imports and cannot be declared, shadowed, opened, or aliased by application source.",
            "Choose an application-owned declaration name",
        ),
        "SEM_STANDARD_OPERATION_RESERVED" => (
            "`{name}` is reserved for a standard-library operation",
            "A free callable cannot duplicate a compiler-owned operation because that would create qualified and unqualified spellings for the same concept.",
            "Choose a domain-specific callable name and use the standard operation through its namespace",
        ),
        "SEM_STANDARD_OPERATION_QUALIFICATION" => (
            "Standard-library operation requires its owning namespace",
            "Standard operations have one canonical source spelling and cannot be called as free functions or receiver methods.",
            "Use `{suggestedName}`",
        ),
        "SEM_SERVICE_IMPORT_PIN_INVALID" => (
            "Service import must pin one supported contract snapshot",
            "This service slice accepts only the checked repository snapshot at its exact version and SHA-256. Missing or changed import metadata cannot establish which provider contract was reviewed.",
            "Restore the pinned service contract import",
        ),
        "SEM_SERVICE_IMPORT_NOT_FOUND" => (
            "Pinned service contract file is unavailable",
            "The service declaration names a repository snapshot that the compiler cannot read, so it cannot verify the imported contract.",
            "Restore the pinned contract file inside the project",
        ),
        "SEM_SERVICE_IMPORT_DIGEST_MISMATCH" => (
            "Pinned service contract bytes have changed",
            "The imported file does not match its declared SHA-256. The compiler will not accept a mutable or stale provider contract.",
            "Restore the reviewed snapshot or plan a separately reviewed contract successor",
        ),
        "SEM_SERVICE_EGRESS_INVALID" => (
            "Service egress must match its fixed authority",
            "The first service slice admits one statically declared HTTPS host and port. Dynamic or mismatched egress could send credentials or payloads to an undeclared destination.",
            "Use the declared mail provider authority or plan a reviewed provider contract",
        ),
        "SEM_SERVICE_SECRET_SINK_INVALID" => (
            "Service credential sink is not permitted",
            "Only the declared bearer credential from the secret mail configuration slot may enter the fixed Authorization header. Other secret paths or sinks are rejected.",
            "Restore the declared credential slot and Authorization header",
        ),
        "SEM_SERVICE_IDEMPOTENCY_INVALID" => (
            "Service operation lacks its stable delivery identity",
            "The operation must bind its request and provider idempotency header to the persisted nominal ReminderIntentId. An attempt identity or missing key could duplicate a delivery.",
            "Use ReminderIntentId for the operation idempotency key",
        ),
        "SEM_SERVICE_CONTRACT_INVALID" => (
            "Service declaration is outside the checked contract slice",
            "This compiler slice accepts one closed operation and its reviewed request, response, retry and outcome rules. Unsupported additions are rejected instead of being silently ignored.",
            "Restore the checked ReminderMail operation or plan a reviewed contract expansion",
        ),
        "SEM_SERVICE_CONFIGURATION_MISSING" => (
            "Service credential slot requires a project configuration declaration",
            "A service must resolve its typed secret slot against the project's declared configuration field.",
            "Declare the mail API key as a secret configuration field",
        ),
        "SEM_SERVICE_CALL_CONTEXT" => (
            "External service operation requires an effectful caller",
            "This checked operation performs network I/O and may produce uncertain outcomes, so pure functions and read-only queries cannot invoke it.",
            "Move the call into an action with its complete declared failure set",
        ),
        "SEM_TEST_SERVICE_FAKE_REQUIRED" => (
            "Authored tests must fake external service calls",
            "A generated authored test cannot dispatch to a real external provider. Every service operation reachable from the test must be supplied by its typed fixture fake.",
            "Add the service's declared fake to the test fixture",
        ),
        "SEM_SERVICE_ATOMIC_EFFECT" => (
            "External service effect cannot share an uncommitted write",
            "A network request cannot participate in the database write's commit or rollback. A durable intent must commit before dispatch so a crash cannot lose or duplicate the external operation.",
            "Move dispatch behind a committed durable intent or separate it from the database write",
        ),
        "SEM_MULTIPLE_APPLICATIONS" => (
            "Project declares more than one application",
            "One application declaration owns the project-wide authentication default and other application capabilities.",
            "Keep one application declaration and combine its settings",
        ),
        "SEM_MULTIPLE_LOCALE_DECLARATIONS" => (
            "Project declares more than one locale set",
            "One closed locale declaration owns formatting support, fallback behavior, and the generated `Locale` enum for the whole application.",
            "Keep one locales declaration and combine its supported values",
        ),
        "SEM_APPLICATION_PRINCIPAL_SHAPE" => (
            "Application principal must name one declaration",
            "The authentication default names the closed principal declaration directly; it cannot be nullable, generic, or a field path.",
            "Use the plain name of the project principal declaration",
        ),
        "SEM_MULTIPLE_PRINCIPALS" => (
            "Project declares more than one principal",
            "Authentication strategies must converge on one closed provider-independent principal declaration.",
            "Keep one principal declaration and combine its variants",
        ),
        "SEM_PRINCIPAL_VARIANT_DUPLICATE" => (
            "Principal variant is declared more than once",
            "The closed principal has exactly one user variant and one service variant.",
            "Keep one declaration of this principal variant",
        ),
        "SEM_PRINCIPAL_VARIANT_REQUIRED" => (
            "Principal is missing a required variant",
            "The v0.1 principal contract is closed over both user and service identities, even when one client class is not configured yet.",
            "Declare both the user and service principal variants",
        ),
        "SEM_REVOCATION_DELAY_REQUIRED" => (
            "Bounded revocation requires a maximum delay",
            "A bounded credential must state the longest interval before authority changes are guaranteed to take effect.",
            "Add a positive duration as `maximum_delay`",
        ),
        "SEM_REVOCATION_DELAY_FORBIDDEN" => (
            "Immediate revocation cannot declare a bounded delay",
            "Immediate mode performs an authoritative check for each authenticated request and therefore has no bounded-delay contract.",
            "Remove `maximum_delay` or select bounded revocation",
        ),
        "SEM_REVOCATION_DELAY_INVALID" => (
            "Revocation delay must be positive",
            "A zero or negative interval cannot express a meaningful bounded revocation guarantee.",
            "Use a positive duration for `maximum_delay`",
        ),
        "SEM_AUTH_STRATEGY_DUPLICATE" => (
            "Authentication strategy name is already in use",
            "Each named authentication strategy identifies one credential transport, validation mode, and principal variant.",
            "Rename or combine the duplicate authentication strategy",
        ),
        "SEM_AUTH_VALIDATOR_REQUIRED" => (
            "Authentication strategy has no validators",
            "A configured credential location must have at least one named validator that resolves to one principal variant.",
            "Add a named validator to the authentication strategy",
        ),
        "SEM_AUTH_VALIDATOR_DUPLICATE" => (
            "Authentication validator name is already in use",
            "Validator names identify one validation mode and principal mapping within their authentication strategy.",
            "Rename or combine the duplicate authentication validator",
        ),
        "SEM_AUTH_CREDENTIAL_SLOT_DUPLICATE" => (
            "Credential location is configured more than once",
            "A request credential must select at most one strategy. Reusing a cookie or bearer location would make selection ambiguous.",
            "Give each authentication strategy one distinct credential location",
        ),
        "SEM_AUTH_BEARER_LOCATION_FORBIDDEN" => (
            "Bearer credential location is not allowed",
            "Bearer credentials are accepted only from the reserved authorization header and never from a path or query value.",
            "Use `authorization_header` for the bearer transport",
        ),
        "SEM_AUTH_VALIDATION_MODE_UNKNOWN" => (
            "Authentication validation mode is not supported",
            "The initial authentication contract supports signed, opaque, API-key, and JWT validation with compiler-owned security policy.",
            "Choose `signed`, `opaque`, `api_key`, or `jwt`",
        ),
        "SEM_AUTH_PRINCIPAL_VARIANT_UNKNOWN" => (
            "Authentication strategy principal variant is not supported",
            "Every strategy must resolve to exactly one variant of the closed provider-independent principal: user or service.",
            "Choose the `user` or `service` principal variant",
        ),
        "SEM_AUTH_MAPPING_DUPLICATE" => (
            "Authentication mapping writes the same principal field twice",
            "Each principal field has one provenance. Competing claim or authority mappings would make identity construction ambiguous.",
            "Keep one mapping for the principal field",
        ),
        "SEM_AUTH_MAPPING_TARGET_INVALID" => (
            "Authentication mapping target is not a matching principal field",
            "Authentication mappings may populate only a field of the declared closed user or service principal variant.",
            "Map to a field such as `Principal.user.subject`",
        ),
        "SEM_AUTH_MAPPING_REQUIRED" => (
            "Principal resolution has no field mappings",
            "Authoritative resolution must construct the declared principal variant from explicit validated authority fields.",
            "Add at least one authority-to-principal field mapping",
        ),
        "SEM_AUTH_RESOLUTION_DUPLICATE" => (
            "Principal variant has more than one resolution declaration",
            "Each strategy has one unambiguous authoritative resolution path for each principal variant it can produce.",
            "Combine the duplicate principal resolution declarations",
        ),
        "SEM_AUTH_RESOLUTION_AUTHORITY_INVALID" => (
            "Principal resolution authority must name one entity field",
            "Authoritative resolution starts from one inventoried entity field such as `User.authentication_subject`.",
            "Name the entity field that resolves this principal",
        ),
        "SEM_DUPLICATE_DECLARATION" => (
            "A name is defined more than once",
            "Every visible name must identify one thing. Two declarations with the same name would make later references ambiguous.",
            "Rename one of the declarations or remove the duplicate",
        ),
        "SEM_UNKNOWN_NAME" => (
            "This name isn't defined",
            "Jadpo could not find this name among local values, declarations, imports, or built-in types.",
            "Correct the name, define it, or import it",
        ),
        "SEM_WRONG_NAME_KIND" => (
            "This name can't be used here",
            "The name exists, but this part of the source needs something different, such as a type, failure, function, or predefined category.",
            "Use a name that is valid here",
        ),
        "SEM_UNKNOWN_CALLEE" => (
            "Called function or action isn't defined",
            "Jadpo could not find a visible function or action with this name.",
            "Correct the name, define the function or action, or import it",
        ),
        "SEM_NOT_CALLABLE" => (
            "This name can't be called",
            "Only a function or action can be followed by parentheses and called.",
            "Call a function or action instead, or remove the parentheses",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn configuration_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "CONFIG_EXPECTED_FIELD" => (
            "Expected a configuration field",
            "A configuration body contains structured `name: Type { ... }` fields only.",
            "Add a typed configuration field or remove the invalid item",
        ),
        "CONFIG_UNKNOWN_OPTION" => (
            "Configuration option is not supported",
            "A v0.1 configuration field accepts only `binding`, `secret`, and `default` options.",
            "Correct the option name or remove it",
        ),
        "CONFIG_DUPLICATE_OPTION" => (
            "Configuration option is repeated",
            "Each structured configuration option has exactly one value.",
            "Keep one occurrence of the option",
        ),
        "CONFIG_DEFAULT_LITERAL_REQUIRED" => (
            "Configuration default requires a literal",
            "Source defaults must be deterministic checked literals rather than runtime expressions.",
            "Provide a string, number, Boolean, or duration literal",
        ),
        "CONFIG_MULTIPLE_DECLARATIONS" => (
            "Application declares configuration more than once",
            "One closed configuration declaration owns every application binding.",
            "Combine the fields into one configuration declaration",
        ),
        "CONFIG_BINDING_REQUIRED" => (
            "Configuration field has no deployment binding",
            "Every field needs one explicit stable environment binding beside its type.",
            "Add `binding: \"NAME\"` to the field options",
        ),
        "CONFIG_BINDING_EMPTY" => (
            "Configuration binding is empty",
            "An empty binding cannot identify a local or deployment environment value.",
            "Use a non-empty stable environment binding name",
        ),
        "CONFIG_BINDING_INVALID" => (
            "Configuration binding is not a valid environment name",
            "Bindings use portable environment identifiers beginning with a letter or underscore and containing only letters, digits, and underscores.",
            "Replace the binding with a portable environment name",
        ),
        "CONFIG_DUPLICATE_BINDING" => (
            "Configuration binding is used by more than one field",
            "A binding must resolve to one typed field so decoding and diagnostics stay unambiguous.",
            "Give each field a distinct binding name",
        ),
        "CONFIG_SECRET_DEFAULT" => (
            "Secret configuration cannot have a source default",
            "Embedding a secret fallback in authored source would expose it in review and generated artifacts.",
            "Remove the default and supply the secret through the local or deployment environment",
        ),
        "CONFIG_SECRET_FLOW" => (
            "Secret configuration reaches an ordinary value boundary",
            "Secret-classified values may flow only into compiler-owned declared secret sinks, never ordinary returns, arguments, records, output, or diagnostics.",
            "Keep the secret inside its generated adapter boundary",
        ),
        "CONFIG_FIXTURE_DUPLICATE" => (
            "Fixture supplies a configuration field more than once",
            "A typed test fixture must provide one unambiguous value for each configured field.",
            "Keep one value for the repeated fixture field",
        ),
        "CONFIG_FIXTURE_SECRET_REQUIRED" => (
            "Secret fixture value must use the secret boundary",
            "Secret configuration is marked explicitly in fixture source so generated reports and traces can redact it before values enter the harness.",
            "Wrap the fixture value in `secret(...)`",
        ),
        "CONFIG_FIXTURE_SECRET_UNEXPECTED" => (
            "Ordinary configuration cannot use the secret fixture boundary",
            "The fixture classification must match the configuration declaration so test behavior cannot change a field's information-flow contract.",
            "Remove `secret(...)` or classify the declared configuration field as secret",
        ),
        "CONFIG_FIXTURE_VALUE_INVALID" => (
            "Fixture configuration value violates its declared type",
            "Harness-provided configuration passes the same nominal validation as deployment configuration before application behavior can read it.",
            "Supply a value that satisfies the configured field type",
        ),
        "CONFIG_FIXTURE_VALUE_MISSING" => (
            "Fixture omits required configuration",
            "Tests receive configuration only from their typed fixture; every field without a checked source default must be supplied explicitly.",
            "Add the missing field to the fixture's `config` block",
        ),
        "CONFIG_DEFAULT_TYPE" => (
            "Configuration default has the wrong type",
            "The literal representation does not match the field's declared type.",
            "Use a literal matching the field type or remove the default",
        ),
        "CONFIG_TYPE_UNSUPPORTED" => (
            "Configuration field type cannot be decoded from one environment value",
            "v0.1 configuration supports named scalar types; structured collections and records need a separately designed source contract.",
            "Use a supported named scalar type or move the structured data behind an adapter",
        ),
        "CONFIG_DEFAULT_INVALID" => (
            "Configuration default violates its type constraints",
            "Defaults pass the same nominal constraints as values supplied at startup.",
            "Change the default so it satisfies the declared constraints",
        ),
        "CONFIG_FIELD_UNKNOWN" => (
            "Configuration field is not declared",
            "Local configuration commands accept the authored field name rather than an arbitrary environment key.",
            "Use a field declared in the application's configuration block",
        ),
        "CONFIG_VALUE_MISSING" => (
            "Required local configuration is missing",
            "The field has no `.env.local` value and no checked source default, so the service cannot start safely.",
            "Run `jadpo config set <field>` or edit `.env.local` locally",
        ),
        "CONFIG_VALUE_INVALID" => (
            "Local configuration value is invalid",
            "The supplied value could not be decoded into the field's declared type and constraints.",
            "Enter a value that satisfies the declared field type",
        ),
        "CONFIG_LOCAL_CHECK_FAILED" => (
            "Local configuration is incomplete or invalid",
            "At least one declared field is missing or failed validation; no value has been printed.",
            "Configure the reported fields and run `jadpo config check` again",
        ),
        "CONFIG_LOCAL_READ_FAILED" => (
            "Local configuration could not be read",
            "Jadpo could not safely read `.env.local` from the project directory.",
            "Restore project read access and retry",
        ),
        "CONFIG_LOCAL_INVALID_ENCODING" => (
            "Local configuration is not UTF-8 text",
            "Jadpo preserves `.env.local` structurally and cannot safely update an unknown byte encoding.",
            "Convert `.env.local` to UTF-8 without sharing its values",
        ),
        "CONFIG_LOCAL_SYNTAX" => (
            "Local configuration syntax is invalid",
            "Jadpo found an invalid or duplicate assignment and refused to guess which value should win.",
            "Correct the `.env.local` assignment structure and retry",
        ),
        "CONFIG_LOCAL_CHANGED" => (
            "Local configuration changed during the update",
            "Another process modified `.env.local`, so Jadpo refused to overwrite the newer contents.",
            "Run the configuration command again against the latest file",
        ),
        "CONFIG_LOCAL_WRITE_FAILED" => (
            "Local configuration could not be saved",
            "The atomic owner-only `.env.local` update could not be completed.",
            "Restore project write access and retry",
        ),
        "CONFIG_LOCAL_UNSAFE_TARGET" => (
            "Local configuration target is unsafe",
            "`.env.local` is a symbolic link or non-file target, so updating it could write outside the project.",
            "Replace it with a regular project-local file and retry",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn policy_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "POLICY_MEMBERSHIP_DUPLICATE" => (
            "Entity declares membership more than once",
            "One membership declaration must identify the authoritative scope, member, and qualified role fields without competing bindings.",
            "Combine the role binding into one membership declaration",
        ),
        "POLICY_DECLARATION_DUPLICATE" => (
            "Protected item declares policy more than once",
            "An entity or exceptional field has one role-first policy matrix so permission sources cannot drift.",
            "Combine the rules into one policy block",
        ),
        "POLICY_MEMBERSHIP_SCOPE_REQUIRED" => (
            "Membership has no scope field",
            "A scoped role fact must identify the authoritative resource or tenant that bounds it.",
            "Add `scope: <field>` to the membership declaration",
        ),
        "POLICY_MEMBERSHIP_MEMBER_REQUIRED" => (
            "Membership has no member field",
            "A membership must identify the user or service principal that receives its role fact.",
            "Add `member: <field>` to the membership declaration",
        ),
        "POLICY_MEMBERSHIP_ROLE_REQUIRED" => (
            "Membership has no role field",
            "A membership must name the closed enum field containing its qualified scoped role.",
            "Add `role: <field>` to the membership declaration",
        ),
        "POLICY_SCOPE_DUPLICATE" => (
            "Policy selects scope more than once",
            "A protected entity needs one unambiguous scope path for each scoped role namespace.",
            "Keep one explicit scope selection",
        ),
        "POLICY_EFFECT_DUPLICATE" => (
            "Policy effect is repeated",
            "Each subject-to-effect grant has one canonical entry in its role-first matrix.",
            "Remove the repeated effect",
        ),
        "POLICY_EFFECT_UNKNOWN" => (
            "Policy effect is not supported",
            "Entity policy uses only `create`, `read`, `update`, and `delete`; independently protected non-entity operations use `invoke`.",
            "Use the semantic effect derived from the protected operation",
        ),
        "POLICY_BINDING_DUPLICATE" => (
            "Field declares more than one role binding",
            "A direct principal relationship establishes at most one fixed qualified role fact.",
            "Keep one qualified field role",
        ),
        "POLICY_BINDING_MUTABLE" => (
            "Role-binding field cannot be mutable",
            "Changing a role source changes future authority and requires a separately reviewed transfer operation.",
            "Declare the role-binding field immutable",
        ),
        "POLICY_BINDING_INVALID" => (
            "Role binding is not authoritative or type-compatible",
            "A role fact must come from a checked principal reference or persistent membership with compatible scope, member, and closed role-enum fields.",
            "Correct the binding fields and use a qualified declared role",
        ),
        "POLICY_ROLE_FIELD_UPDATE_FORBIDDEN" => (
            "Role-binding field is writable by an ordinary update",
            "Ownership, tenant scope, and membership role fields cannot change through general update permission.",
            "Make the field immutable or design a named reviewed transfer operation",
        ),
        "POLICY_ROLE_FIELD_CREATE_FORBIDDEN" => (
            "Role-binding field is not derived from the authenticated principal",
            "A caller-controlled owner relationship could manufacture authority even when its identifier has the correct declared type.",
            "Construct the role-binding field from the matching typed `principal` identity",
        ),
        "POLICY_ROLE_UNBOUND" => (
            "Policy role has no authoritative binding",
            "Declaring a role enum does not grant authority; the compiler needs a direct principal relationship or persistent membership source for its scope.",
            "Add an authoritative direct or membership role binding",
        ),
        "POLICY_SCOPE_MISSING" => (
            "Protected entity has no path to the role scope",
            "A scoped role can authorize only rows connected to its authoritative resource or tenant scope.",
            "Add the missing typed scope relationship or correct the explicit scope field",
        ),
        "POLICY_SCOPE_AMBIGUOUS" => (
            "Protected entity has more than one possible role scope",
            "Choosing a scope by field name, order, route parameter, or caller input could authorize the wrong tenant.",
            "Add one explicit `scope:` selection to the entity policy",
        ),
        "POLICY_FIELD_WIDENS_ENTITY" => (
            "Field policy would widen entity permission",
            "Exceptional field policy is an additional narrowing gate and cannot grant a role or effect absent from the containing entity policy.",
            "Narrow the field rule or deliberately review the entity-level grant",
        ),
        "POLICY_SUBJECT_DUPLICATE" => (
            "Policy subject is repeated",
            "One canonical role-first entry prevents grants for the same qualified subject from being split or drifting.",
            "Combine the effects into one subject entry",
        ),
        "POLICY_OPERATION_UNKNOWN" => (
            "Policy exception names an unknown entity operation",
            "A named exception can replace inherited permissions only for one checked operation owned by the protected entity.",
            "Correct the operation name or remove the unused exception",
        ),
        "POLICY_OPERATION_EFFECT_MISMATCH" => (
            "Policy exception names an effect the operation does not perform",
            "Operation exceptions are checked against the derived persistence graph and cannot invent or omit reached entity effects.",
            "Match the exception to the operation's derived effects",
        ),
        "POLICY_INVOKE_CONTEXT" => (
            "Invoke policy is attached to the wrong kind of declaration",
            "Only a non-entity action or query can own a local `invoke` policy; entity matrices use create, read, update, and delete.",
            "Move the invoke rule into the independently protected operation or use the entity's derived effect",
        ),
        "POLICY_EFFECT_UNGRANTED" => (
            "Reachable entity effect has no policy grant",
            "Policy is default-deny: every derived create, read, update, or delete effect needs at least one permitted authoritative subject.",
            "Declare the intended qualified role or access subject at the entity policy",
        ),
        "POLICY_PUBLIC_GRANT_MISSING" => (
            "Public route reaches an entity without public permission",
            "`auth: none` removes authentication only at the transport boundary; protected entity access also requires an explicit `Access.public` grant.",
            "Protect the route or deliberately grant `Access.public` for the reached effect",
        ),
        "POLICY_OUTPUT_FIELD_UNPROVED" => (
            "Operation output exposes a field to a broader role set",
            "A narrowing field policy must admit every subject that can reach the operation output; output serialization never removes a forbidden field silently.",
            "Return a projection without the field or narrow the named operation to subjects allowed to read it",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn failure_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "EFFECT_AUTHORED_ASYNC" => (
            "Authored `async` is not part of Jadpo",
            "Actions may suspend internally, but the compiler derives target-level scheduling without changing the authored callable contract.",
            "Remove `async`; declare a function or action with its ordinary Jadpo signature",
        ),
        "EFFECT_AUTHORED_AWAIT" => (
            "Authored `await` is not part of Jadpo",
            "Every ordinary action call completes before the next statement, so the compiler inserts any target-level suspension mechanism internally.",
            "Remove `await` and keep the ordinary call expression",
        ),
        "EFFECT_FUNCTION_CALLS_ACTION" => (
            "Function cannot call an action",
            "Functions are deterministic and effect-free. Calling an action would allow persistence or other effects to escape through a function boundary.",
            "Move the call into an action, or call a function instead",
        ),
        "EFFECT_FUNCTION_PERSISTENCE" => (
            "Function cannot perform persistence",
            "Create, query, update, and delete expressions access stored state and are permitted only inside actions; functions remain pure and deterministic.",
            "Move the persistence expression into an action",
        ),
        "EFFECT_QUERY_MUTATION" => (
            "Query cannot mutate an entity",
            "Named queries are read-only operations. Entity-owned actions are the only declarations allowed to create, update, or delete entity state.",
            "Move the mutation into an action owned by the entity",
        ),
        "EFFECT_QUERY_CALLS_ACTION" => (
            "Query cannot call an action",
            "Calling an action would let a read-only query perform a mutation or another effect through its call graph.",
            "Call another query or pure function, or move this orchestration into an action",
        ),
        "FAIL_ATTEMPT_REQUIRED" => (
            "This operation can fail and requires `attempt`",
            "`attempt` makes it clear that a declared failure may leave the current function or action.",
            "Add `attempt` before this operation",
        ),
        "FAIL_CONTEXT_FIELD_OVERLAP" => (
            "Failure field is both public and internal",
            "The same field cannot be returned to callers and hidden from them at the same time.",
            "Keep the field in either `public` or `internal`, not both",
        ),
        "FAIL_OUTCOME_DUPLICATE_ARM" => (
            "Outcome match repeats a failure arm",
            "Each declared failure must have exactly one explicit handling decision in an outcome match.",
            "Keep one arm for this failure",
        ),
        "FAIL_OUTCOME_INFALLIBLE" => (
            "Outcome match requires a fallible call",
            "An infallible call has no failure outcome to handle and should be used as an ordinary expression.",
            "Remove the outcome match and use the call directly",
        ),
        "FAIL_OUTCOME_MISSING_ARM" => (
            "Outcome match does not handle every failure",
            "Every failure declared by the called function or action needs one explicit recovery, mapping, or propagation arm.",
            "Add an arm for the missing failure",
        ),
        "FAIL_OUTCOME_PATTERN_INVALID" => (
            "Outcome match arm has an invalid pattern",
            "Outcome matches accept only `success(value)` and exact `failure FailureName` patterns.",
            "Replace the pattern with `success(value)` or an exact failure arm",
        ),
        "FAIL_OUTCOME_SUCCESS_DUPLICATE" => (
            "Outcome match repeats its success arm",
            "An outcome has one successful value, so exactly one success arm is allowed.",
            "Keep one `success(value)` arm",
        ),
        "FAIL_OUTCOME_SUCCESS_MISSING" => (
            "Outcome match has no success arm",
            "Every outcome match must decide what value to produce when the call succeeds.",
            "Add one `success(value)` arm",
        ),
        "FAIL_OUTCOME_SUBJECT_REQUIRED" => (
            "Outcome match requires one direct call",
            "The compiler needs the called function or action's exact failure surface to check exhaustive outcome arms.",
            "Match a direct fallible function or action call",
        ),
        "FAIL_OUTCOME_SUCCESS_VALUE_REQUIRED" => (
            "Success arm must produce a value",
            "The success arm defines the successful value of the whole outcome expression; it cannot reject or propagate.",
            "Return a compatible value from the success arm",
        ),
        "FAIL_OUTCOME_UNKNOWN_ARM" => (
            "Outcome arm names a failure the call cannot produce",
            "Failure arms must exactly match the called function or action's declared failure surface.",
            "Remove the arm or use a failure declared by the call",
        ),
        "FAIL_OUTCOME_WILDCARD" => (
            "Outcome failure wildcard is not allowed",
            "Exact failure arms force a new decision when a called function or action gains another failure.",
            "Replace the wildcard with one exact arm for each declared failure",
        ),
        "FAIL_DUPLICATE_CODE" => (
            "Failure code is already in use",
            "Callers use this code to identify a failure, so every public failure code must be unique.",
            "Give one failure a distinct stable public code",
        ),
        "FAIL_DUPLICATE_CONTEXT_FIELD" => (
            "Failure context field is supplied more than once",
            "A rejected failure has one flat context value; assigning a field twice would make its disclosed and internal value ambiguous.",
            "Keep one value for the repeated context field",
        ),
        "FAIL_DUPLICATE_DECLARATION" => (
            "Failure is repeated after `fails`",
            "Listing the same failure twice does not change what can happen and makes the declaration harder to review.",
            "Remove the repeated failure from the `fails` list",
        ),
        "FAIL_MISSING_CONTEXT_FIELD" => (
            "Rejected failure is missing required context",
            "Every field declared by a failure must be supplied when that failure is rejected so its public and internal values are complete and typed.",
            "Add the missing field to the `reject` value",
        ),
        "FAIL_MUTATION_CONFLICT_NOT_CONFLICT" => (
            "`conflict:` must use a `Conflict` failure",
            "A uniqueness or write conflict needs a failure declared with the predefined `Conflict` category.",
            "Bind the conflict to a failure declared with `kind: Conflict`",
        ),
        "FAIL_PATCH_EMPTY_NOT_INVALID_VALUE" => (
            "`empty:` must use an `InvalidValue` failure",
            "A patch with no supplied changes is invalid input, so its failure must use the predefined `InvalidValue` category.",
            "Bind `empty:` to a failure declared with `kind: InvalidValue`",
        ),
        "FAIL_REQUIRED_MUTATION_NOT_NOT_FOUND" => (
            "`missing:` must use a `NotFound` failure",
            "When a required update or delete finds no entity, its failure must use the predefined `NotFound` category.",
            "Bind `missing:` to a failure declared with `kind: NotFound`",
        ),
        "FAIL_REQUIRED_QUERY_NOT_NOT_FOUND" => (
            "`missing:` must use a `NotFound` failure",
            "When a required query finds no entity, its failure must use the predefined `NotFound` category.",
            "Bind `missing:` to a failure declared with `kind: NotFound`",
        ),
        "FAIL_STALE_DECLARATION" => (
            "A listed failure can never happen",
            "Every failure after `fails` must be able to leave the function or action through a call or `reject`.",
            "Remove the stale `fails` entry",
        ),
        "FAIL_UNDECLARED_PROPAGATION" => (
            "A possible failure is missing after `fails`",
            "A function or action must list every failure that can leave its body so callers know what to handle.",
            "Add the failure to `fails`, or handle it before it leaves",
        ),
        "FAIL_UNKNOWN_CONTEXT_FIELD" => (
            "Rejected failure contains an unknown context field",
            "A `reject` value may assign only fields declared by that failure; extra fields have no disclosure classification or generated schema.",
            "Remove the extra field or declare it on the failure",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn route_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "ROUTE_AUTH_VALUE_INVALID" => (
            "Route authentication value `{found}` is not valid",
            "Routes require authentication by default. The only explicit opt-out is the exact form `auth: none`, so the compiler cannot infer whether `{found}` was meant to retain authentication or disable it.",
            "Choose the authentication boundary for `{route}`",
        ),
        "ROUTE_BEHAVIOUR_CONFLICT" => (
            "Route declares two behavior forms",
            "A route must select exactly one local inline `action:` or one named `run:` invocation; keeping both would create two competing handlers.",
            "Keep either `run:` or the inline `action:`",
        ),
        "ROUTE_BEHAVIOUR_REQUIRED" => (
            "Route has no behavior",
            "Every route needs exactly one executable behavior, supplied either by a named `run:` action invocation or an inline `action:` block.",
            "Add `run:` or an inline `action:`",
        ),
        "ROUTE_ITEM_COLON_REQUIRED" => (
            "Route item requires a `:` separator",
            "Every route item uses the explicit `name: value` form, including block-valued `path:` and `action:` items.",
            "Insert `:` after the route item name",
        ),
        "ROUTE_ITEM_DUPLICATE" => (
            "Route item is declared more than once",
            "A route has one authentication setting, path schema, input, output, success mode, and behavior selection. Repeating an item would silently replace part of its public contract.",
            "Keep one occurrence of this route item",
        ),
        "ROUTE_DEADLINE_INVALID" => (
            "Route deadline must be a finite positive duration",
            "A route deadline is a fixed duration written in whole milliseconds using `ms`, `s`, `m`, or `h`. Zero, fractional milliseconds, days, and values outside the portable runtime range are not supported.",
            "Use a positive route deadline such as `deadline: 2s`",
        ),
        "ROUTE_HEADER_FROM_REQUIRED" => (
            "Route header binding needs an explicit wire name",
            "Each ordinary header binding uses `name: Type from \"HTTP-Name\"` so source names never imply transport casing or collide with compiler-owned headers.",
            "Add `from` followed by the exact HTTP header name",
        ),
        "ROUTE_PATH_BINDING_DUPLICATE" => (
            "Route path binding name is repeated",
            "Each typed field in `path:` must bind one distinct placeholder; duplicate names cannot identify separate decoded path segments.",
            "Rename or remove the repeated path field",
        ),
        "ROUTE_PATH_BINDING_EXTRA" => (
            "Typed path binding has no placeholder",
            "Every typed field in `path:` must correspond to one same-named `{placeholder}` in the route path so callers actually supply its value.",
            "Remove the extra path field or add its matching placeholder",
        ),
        "ROUTE_PATH_BINDING_MISSING" => (
            "Route placeholder has no typed binding",
            "Every `{placeholder}` in a route path needs one same-named typed field in `path:` so the runtime can decode and validate it before the handler runs.",
            "Add a typed path field for the route placeholder",
        ),
        "ROUTE_PATH_FIELD_MODIFIER_INVALID" => (
            "Route path field has an unsupported modifier",
            "A value inside the URL path is always required and can declare only its type. It cannot be nullable, optional, stored, indexed, or a relationship.",
            "Remove the modifier and keep only `name: Type`",
        ),
        "ROUTE_PATH_PLACEHOLDER_DUPLICATE" => (
            "Route path placeholder is repeated",
            "A placeholder name may occur once in a route path; repeating it would map one typed binding to multiple URL segments ambiguously.",
            "Give each route placeholder a distinct name",
        ),
        "ROUTE_PATH_PLACEHOLDER_INVALID" => (
            "Route path contains an invalid placeholder",
            "A placeholder uses `{name}` with a non-empty Jadpo name beginning with a letter or underscore and ending with a closing brace.",
            "Correct the placeholder to the `{name}` form",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn data_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "DATA_ATOMIC_DOMAIN_MISMATCH" => (
            "Atomic action spans incompatible transaction domains",
            "One atomic boundary can include only participants that share a compiler-supported transaction and recovery domain.",
            "Use one transaction domain or choose an explicit durable workflow",
        ),
        "DATA_CONSISTENCY_INVALID" => (
            "Consistency disposition is not recognised",
            "A multi-entity action must select a supported atomic or durable-workflow contract explicitly.",
            "Use `atomic` or `durable_workflow`",
        ),
        "DATA_CONSISTENCY_REQUIRED" => (
            "Multi-entity action needs an explicit consistency disposition",
            "Calling mutations owned by more than one entity must not silently widen or split transaction boundaries.",
            "Declare `consistency: atomic` or `consistency: durable_workflow`",
        ),
        "DATA_DELIVERY_MODE_INVALID" => (
            "Derived representation delivery mode is not supported",
            "A derived store must use durable delivery so an authority change cannot be committed without a replayable update obligation.",
            "Use `delivery: durable`",
        ),
        "DATA_ENTITY_NOT_PERSISTENT" => (
            "Entity has no persistence capability",
            "Entity identity does not imply storage. Create, query, update, and delete require an explicit persistence capability.",
            "Add an accepted persistence capability or remove the storage operation",
        ),
        "DATA_FRESHNESS_INVALID" => (
            "Query freshness requirement is not recognised",
            "Queries must choose one of the accepted authority, read-your-writes, bounded-staleness, or eventual guarantees.",
            "Use a supported freshness requirement",
        ),
        "DATA_IDENTITY_FIELD_UNKNOWN" => (
            "Entity identity names an unknown field",
            "An entity reference is derived from one field declared in the same authoritative entity dossier.",
            "Correct the identity field name or add the missing field",
        ),
        "DATA_MUTATION_GUARD_INVALID" => (
            "Mutation guard is not recognised",
            "A value-receiver mutation must establish current authority state by reloading it or checking a revision.",
            "Use `guard: reload` or `guard: revision`",
        ),
        "DATA_MUTATION_OWNER_REQUIRED" => (
            "Entity mutation is outside its owning action",
            "Only an action owned by an entity may directly create, update, or delete that entity, so invariants and policy cannot be bypassed.",
            "Move the mutation into an action declared by the affected entity",
        ),
        "DATA_PERSISTENCE_ROLE_INVALID" => (
            "Persistence role is not recognised",
            "The initial entity model gives each mutable fact one declared authority; derived stores use cache or projection declarations.",
            "Use `role: authority`",
        ),
        "DATA_PERSISTENCE_ROLE_REQUIRED" => (
            "Persistence capability needs an authority role",
            "A stored entity must say which store owns its mutable facts rather than implying multiple writable copies.",
            "Add `role: authority`",
        ),
        "DATA_PERSISTENCE_STORE_REQUIRED" => (
            "Persistence capability needs a store",
            "The compiler needs a named transaction domain for schema, mutation, and consistency planning.",
            "Add `store: <name>`",
        ),
        "DATA_PROJECT_ROLE_INVALID" => (
            "Source file does not match its project role",
            "Recognised entity, value, query, workflow, and route directories have bounded declaration roles so authority and effects remain reviewable.",
            "Move the declaration to its recognised role or correct the file contents",
        ),
        "DATA_QUERY_FRESHNESS_REQUIRED" => (
            "Named query needs a freshness requirement",
            "The compiler cannot choose a potentially stale representation without knowing the weakest observation contract the caller permits.",
            "Declare the query's freshness requirement",
        ),
        "DATA_RAW_QUERY_OUTSIDE_NAMED_QUERY" => (
            "Raw query expression is outside a named query",
            "Routes, actions, workflows, jobs, and functions call named queries so every read shape, policy, index, and freshness contract is inventoried.",
            "Move the read into a named entity or top-level query",
        ),
        "DATA_RECEIVER_KIND_INVALID" => (
            "Entity operation receiver is not recognised",
            "An entity operation explicitly requires either its stable reference or a complete immutable value snapshot.",
            "Use `self: ref` or `self: value`",
        ),
        "DATA_REPRESENTATION_AUTHORITY_INVALID" => (
            "Derived representation has an invalid authority link",
            "A cache or projection must derive from the entity's one authority and cannot name that same store as another representation.",
            "Point `from` at the authority and use a distinct derived store",
        ),
        "DATA_REPRESENTATION_DUPLICATE" => (
            "Derived representation name is repeated",
            "Each cache or projection has one stable identity for delivery, replay, watermarks, and reconciliation.",
            "Keep one representation with this name or rename it",
        ),
        "DATA_REPRESENTATION_SETTING_INVALID" => (
            "Derived representation setting is not recognised",
            "Cache and projection declarations accept only their bounded store, authority, strategy, and delivery contract settings.",
            "Use a supported representation setting",
        ),
        "DATA_REPRESENTATION_SETTING_REQUIRED" => (
            "Derived representation is missing a required setting",
            "A generated projection contract needs its store, authority source, and durable delivery mode to be explicit.",
            "Add the missing representation setting",
        ),
        "DATA_VALUE_RECEIVER_GUARD_REQUIRED" => (
            "Value-receiver mutation needs a stale-write guard",
            "A complete entity value is an immutable observation and cannot overwrite newer authority state without a reload or checked revision.",
            "Add `guard: reload` or `guard: revision`",
        ),
        "DATA_COMPOUND_CONSTRAINT_FIELDS" => (
            "Compound constraint needs at least two fields",
            "A compound identity, uniqueness rule, or index joins multiple fields. A rule for one field belongs directly on that field.",
            "Add the other participating fields, or use a field-level modifier",
        ),
        "DATA_CONSTRAINT_DUPLICATE_FIELD" => (
            "Compound constraint repeats a field",
            "Each field can appear once in a compound constraint. Repeating it does not add anything and makes the intended key unclear.",
            "Remove the repeated field from the constraint",
        ),
        "DATA_CONSTRAINT_NULLABLE_FIELD" => (
            "Compound constraint contains a nullable field",
            "Jadpo does not define cross-adapter uniqueness or identity semantics for null inside compound keys, so constrained fields must be non-nullable.",
            "Use non-nullable fields, or remove the field from the constraint",
        ),
        "DATA_CONSTRAINT_UNKNOWN_FIELD" => (
            "Compound constraint names an unknown field",
            "Every member of a compound constraint must resolve to a stored field on the same persisted object.",
            "Correct the field name or add the missing object field",
        ),
        "DATA_DUPLICATE_CONSTRAINT_SHAPE" => (
            "Persisted object repeats the same constraint field set",
            "Two persistence constraints over the same fields would create duplicate or conflicting database rules.",
            "Keep one constraint for this field set",
        ),
        "DATA_IDENTITY_NULLABLE" => (
            "Persisted object identity cannot be nullable",
            "Every stored object needs a present, stable identity. A nullable identity could not address or reference every stored row.",
            "Make the identity field non-nullable",
        ),
        "DATA_INVERSE_DUPLICATE_NAME" => (
            "Inverse relationship name is already in use",
            "Fields and inverse relationships share an object member namespace so selection and generated output paths resolve unambiguously.",
            "Rename the inverse relationship or conflicting field",
        ),
        "DATA_INVERSE_NOT_OWNING_REFERENCE" => (
            "Inverse relationship does not target an owning reference",
            "An inverse is derived from a stored reference field on the related persisted object; the named `via` field must be that owning reference.",
            "Point `via` at the related object's owning reference field",
        ),
        "DATA_INVERSE_OPTIONAL_NOT_UNIQUE" => (
            "Optional inverse relationship is not unique",
            "An optional one-to-one inverse may return at most one record, so its owning reference must be protected by a unique constraint.",
            "Make the owning reference unique, or declare a many-valued inverse",
        ),
        "DATA_INVERSE_VIA_FIELD" => (
            "Inverse relationship names an unknown `via` field",
            "The `via` member must resolve to a stored field on the related object before the compiler can derive the reverse relationship.",
            "Correct the `via` field name on the inverse relationship",
        ),
        "DATA_MODIFIER_NON_ENTITY" => (
            "Persistence setting requires a persisted object",
            "Identity, uniqueness, indexing, and references describe stored columns and belong in `persist Type { ... }`.",
            "Move the setting into the object's `persist` declaration, or remove it",
        ),
        "DATA_MULTIPLE_IDENTITIES" => (
            "Persisted object declares more than one identity",
            "A persisted object has one identity used by references, stored rows, and generated identifiers.",
            "Keep one identity field or one named identity constraint",
        ),
        "DATA_RELATIONSHIP_CYCLE" => (
            "Required cascading references form a delete cycle",
            "A cycle of required references with cascading deletion has no safe starting point and can recursively delete the same dependency graph.",
            "Break the cycle by changing one reference or its delete behavior",
        ),
        "DATA_RELATIONSHIP_SET_NULL_REQUIRED" => (
            "Required reference cannot use `set_null` on delete",
            "`set_null` clears the stored reference when its target is deleted, which is impossible for a non-nullable required field.",
            "Make the reference nullable or choose `restrict` or `cascade`",
        ),
        "DATA_RELATIONSHIP_TARGET_FIELD" => (
            "Reference target field does not exist",
            "A stored reference must name a field that is actually declared on the target object.",
            "Correct the referenced target field",
        ),
        "DATA_RELATIONSHIP_TARGET_NOT_KEY" => (
            "Reference target is not an identity or unique key",
            "A relationship must point to a stable identity or unique field set; otherwise one stored reference could resolve to multiple target records.",
            "Reference the target identity or another unique key",
        ),
        "DATA_RELATIONSHIP_TYPE_MISMATCH" => (
            "Reference field type does not match its target",
            "The reference and its target must use the same named type. For example, an `OrderId` cannot point to a `CustomerId` field.",
            "Use the referenced field's named type for this reference",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn core_type_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "TYPE_ROUTE_BINDING_COLLISION" => (
            "Route input name is used by more than one boundary",
            "Path, query, header, and body fields must have distinct top-level names so reviews and generated adapters cannot confuse their transport source.",
            "Rename one of the colliding route input fields",
        ),
        "TYPE_ROUTE_HEADER_NAME_INVALID" => (
            "HTTP header name is not valid ASCII",
            "An explicit header wire name may contain only the ASCII token characters allowed for HTTP field names.",
            "Use a valid explicit HTTP field name",
        ),
        "TYPE_ROUTE_HEADER_SCALAR_REQUIRED" => (
            "Route header binding requires a non-null scalar type",
            "One HTTP field value can bind only a supported scalar, refinement, or enum. Structured and nullable values need a different reviewed encoding.",
            "Use a supported non-null scalar header type",
        ),
        "TYPE_ROUTE_HEADER_WIRE_DUPLICATE" => (
            "HTTP header wire name is bound more than once",
            "HTTP field names are case-insensitive, so two bindings that differ only in casing would read the same transport value.",
            "Keep one binding for this HTTP header name",
        ),
        "TYPE_ROUTE_QUERY_FIELD_UNSUPPORTED" => (
            "Query Object contains an unsupported field type",
            "Typed query fields may use supported non-null scalars, enums, refinements, or one JSON-encoded closed Object. Collections and nullable sentinels have no selected query encoding.",
            "Use a supported query field type or make absence optional",
        ),
        "TYPE_ROUTE_QUERY_OBJECT_REQUIRED" => (
            "Route query binding must reference a closed Object",
            "A route query is a named field set so the compiler can reject unknown keys, apply defaults, and generate one OpenAPI parameter per declared field.",
            "Reference a named closed Object after `query:`",
        ),
        "TYPE_AUTH_EXCHANGE_BINDING" => (
            "Exchange must bind a service key and service signed validator",
            "The exchange endpoint uses one bearer strategy with a declared service API-key authority and a service signed validator. A user validator or a validator from another strategy cannot supply either role.",
            "Name the checked service key and signed validators in this bearer strategy",
        ),
        "TYPE_AUTH_EXCHANGE_PATH" => (
            "Exchange path must be a fixed absolute path",
            "A compiler-owned exchange endpoint needs a nonempty absolute path of ASCII letters, digits, hyphens and underscores without parameters, query, fragment or escapes so its transport and artifact inventory are unambiguous.",
            "Use a fixed path such as `/auth/exchange`",
        ),
        "TYPE_AUTH_EXCHANGE_ROUTE_COLLISION" => (
            "Exchange path overlaps an authored POST route",
            "The compiler-owned exchange endpoint must own its POST path. An authored route at the same literal path or a matching path parameter would make dispatch ambiguous.",
            "Move the exchange endpoint or the overlapping POST route",
        ),
        "TYPE_AUTH_JWT_TRANSPORT" => (
            "JWT validation requires bearer transport",
            "The compiler-owned external JWT adapter accepts only the reserved Authorization bearer credential slot. Cookie authentication requires a first-party adapter with the browser CSRF boundary.",
            "Configure JWT validation under the bearer strategy without changing protected route requirements",
        ),
        "TYPE_AUTH_ADAPTER_SETTING" => (
            "Authentication adapter setting is invalid",
            "First-party validators accept unique secret, previous_secret, audience and origin settings. Keys reference secret textual configuration. JWT validators accept only non-secret issuer, audience and jwks_uri text or URL settings. Service API keys may bind owner to a required persistent entity reference on their authority.",
            "Use the settings belonging to the selected validator and explicit configuration or authority references",
        ),
        "TYPE_AUTH_RESERVED_ROUTE_INPUT" => (
            "Credential transport cannot be ordinary route input",
            "Configured authentication cookies and authorization headers belong exclusively to the generated authentication boundary; accepting the same name as application input would create a second untrusted credential path.",
            "Remove the reserved credential field from the route input",
        ),
        "TYPE_AUTH_PRINCIPAL_INPUT" => (
            "A principal cannot come from request input",
            "The authenticated principal is constructed by the compiler-owned credential and authority pipeline. Request bodies and paths cannot assert an identity.",
            "Remove the principal type from the route input",
        ),
        "TYPE_AUTH_PUBLIC_PRINCIPAL" => (
            "Public behavior requires an authenticated principal",
            "An `auth: none` route never creates a principal, so it cannot directly or transitively reach behavior that reads one.",
            "Keep the route authenticated or remove the principal-dependent behavior",
        ),
        "TYPE_AUTH_PRINCIPAL_OUTPUT" => (
            "Principal values cannot cross an ordinary output boundary",
            "A principal contains authentication identity and provenance owned by the security boundary. Public outputs must project an explicitly declared non-principal value.",
            "Return a reviewed output projection instead of the principal",
        ),
        "TYPE_AUTH_PRINCIPAL_FAILURE_CONTEXT" => (
            "Principal values cannot enter failure context",
            "Failure context may be logged or disclosed according to its classification. Carrying the principal object would risk exposing authentication identity or provenance.",
            "Store only the specific non-secret identifier required by the failure",
        ),
        "TYPE_AUTH_CLAIM_UNKNOWN" => (
            "Authentication claim is not in the constrained identity schema",
            "Provider and credential data is untrusted. Only the compiler-owned subject and authentication-strength claims may enter the intermediate identity contract.",
            "Map an allowlisted claim or obtain the field from authority resolution",
        ),
        "TYPE_AUTH_CLAIM_AUTHORITATIVE_FIELD" => (
            "Credential claim cannot populate an authoritative principal field",
            "Credential claims may carry authentication facts such as subject or authentication strength, but application identity fields come from authoritative resolution.",
            "Populate this principal field from an authority entity resolution",
        ),
        "TYPE_AUTH_AUTHORITY_NOT_UNIQUE" => (
            "Principal resolution authority is not unique",
            "An authentication identity must resolve to at most one authoritative entity row; a non-key field could construct ambiguous principals.",
            "Resolve through an identity or unique entity field",
        ),
        "TYPE_AUTH_AUTHORITY_NOT_PERSISTENT" => (
            "Principal authority is not a persistent entity",
            "Authentication resolution must query an authoritative stored entity. A transient value or non-persistent entity cannot prove current identity or lifecycle state.",
            "Resolve the principal through a persistent authority entity",
        ),
        "TYPE_AUTH_LIFECYCLE_ACTIVE_FIELD" => (
            "Lifecycle authentication check reads a private field",
            "Authentication may inspect only the authority subject, identity and lifecycle-owned state of a hidden lifecycle entity.",
            "Base the active check on lifecycle-owned state fields",
        ),
        "TYPE_AUTH_LIFECYCLE_MAPPING_SCOPE" => (
            "Lifecycle authentication mapping reads an unrelated field",
            "A lifecycle authority lookup may map its subject and identity values, but must not read unrelated business or private fields.",
            "Map the authority subject or an identity field",
        ),
        "TYPE_AUTH_INACTIVE_FAILURE_KIND" => (
            "Inactive-principal failure has the wrong category",
            "A valid credential for a disabled user or service is an explicit application rejection, not absence, conflict, or an internal fault.",
            "Declare the inactive failure with `kind: NotPermitted` (403) or `kind: Rejected` (422) to match the application contract",
        ),
        "TYPE_AUTH_RESOLUTION_REQUIRED" => (
            "Authentication validator has no authority resolution",
            "Every configured user or service validator needs one checked authority path so immediate and fresh-authority requests can resolve exactly one active principal.",
            "Add one resolution for this principal variant",
        ),
        "TYPE_AUTH_MAPPING_MISMATCH" => (
            "Authority field does not match the principal field type",
            "Principal construction keeps separately declared types distinct; resolution cannot copy an incompatible authority value into a principal field.",
            "Map a compatible authoritative field to this principal field",
        ),
        "TYPE_AUTH_RESOLUTION_INCOMPLETE" => (
            "Principal resolution does not construct every required field",
            "A strategy must produce one complete closed principal variant rather than a partial identity with missing authoritative fields.",
            "Map every required principal field from a validated claim or authority field",
        ),
        "TYPE_ARGUMENT_COUNT" => (
            "Call has the wrong number of arguments",
            "A function or action invocation must supply exactly one value for each declared parameter in declaration order.",
            "Add or remove arguments to match the function or action parameters",
        ),
        "TYPE_NAMED_ARGUMENT_UNSUPPORTED" => (
            "This callable does not accept named arguments",
            "Named arguments are reserved for compiler-owned standard-library options whose names and policies are checked as part of the language contract.",
            "Pass this callable's declared parameters in order without names",
        ),
        "TYPE_CLOCK_CONTEXT" => (
            "The operation clock is unavailable in this context",
            "`clock.now` is an effect owned by actions and boundary operations. Pure functions and named queries stay deterministic by receiving an `Instant` parameter.",
            "Move the clock read into an action, or pass an `Instant` into this callable",
        ),
        "TYPE_TEST_CLOCK_CONTEXT" => (
            "Test clock control is unavailable in application behavior",
            "Advancing time is a compiler-owned test-harness operation. Allowing it in functions, actions, queries, or routes would make production behavior depend on a test capability.",
            "Move `advance clock` into an authored test",
        ),
        "TYPE_TEST_CALL_CONTEXT" => (
            "Callable boundary invocation is available only in tests",
            "`call` starts a fresh compiler-owned operation with fixture capabilities and records callable-boundary evidence. Application behavior uses an ordinary checked invocation instead.",
            "Use an ordinary invocation here, or move this boundary test into a `test` declaration",
        ),
        "TYPE_GENERATED_FIELD_ASSIGNMENT" => (
            "Compiler-generated field cannot be assigned",
            "A lifecycle field has one owner: the compiler supplies the stable operation time on creation or semantic change, so authored code cannot override it.",
            "Remove the generated field from this assignment",
        ),
        "TYPE_FIELD_UPDATE_FORBIDDEN" => (
            "Field cannot be changed by an ordinary update",
            "Identity, immutable, scope, membership, and direct role-binding fields have compiler-owned write rules so an update cannot move a record or manufacture authority.",
            "Remove the field from the update or use a separately reviewed transfer operation when that capability is available",
        ),
        "TYPE_LIFECYCLE_REQUIRES_PERSISTENT_ENTITY" => (
            "Lifecycle contract requires a persistent entity",
            "Lifecycle visibility, transitions, and retention are enforced against authoritative stored rows.",
            "Add the entity's persistence authority or remove its lifecycle contract",
        ),
        "TYPE_LIFECYCLE_INITIAL_REQUIRED" => (
            "Lifecycle initial state is required",
            "Every lifecycle-owned field needs a compiler-owned initial value on creation.",
            "Declare `initial` with a value for every field used by a lifecycle transition",
        ),
        "TYPE_LIFECYCLE_VISIBLE_REQUIRED" => (
            "Lifecycle visibility predicate is required",
            "Lifecycle-owned rows must have one declared visibility rule so reads and mutations conceal inactive rows consistently.",
            "Add `visible when` with a same-row lifecycle predicate",
        ),
        "TYPE_LIFECYCLE_INITIAL_DUPLICATE" => (
            "Lifecycle initial field is repeated",
            "Each compiler-owned initial field must be assigned exactly once.",
            "Remove the duplicate initial field assignment",
        ),
        "TYPE_LIFECYCLE_FIELD_UNKNOWN" => (
            "Lifecycle refers to an unknown entity field",
            "Lifecycle ownership must bind to a field declared by the same entity.",
            "Use a field declared on this entity",
        ),
        "TYPE_LIFECYCLE_FIELD_INELIGIBLE" => (
            "Field cannot be lifecycle-owned",
            "Generated, immutable, identity, and role-binding fields have separate compiler-owned write rules.",
            "Choose a mutable non-identity field for lifecycle state",
        ),
        "TYPE_LIFECYCLE_VALUE_UNSUPPORTED" => (
            "Lifecycle value is outside the supported closed form",
            "Initial and transition values are closed literals, typed field constructors over enum variants, `none`, or the operation clock; arbitrary input and effects are not allowed.",
            "Replace this value with a literal, typed field constructor, `none`, or `clock.now`",
        ),
        "TYPE_LIFECYCLE_TRANSITION_FIELD_DUPLICATE" => (
            "Transition assigns a field more than once",
            "A transition has one unambiguous value for each lifecycle-owned field.",
            "Keep one assignment for this field",
        ),
        "TYPE_LIFECYCLE_RESTORE_FORBIDDEN" => (
            "Lifecycle transition cannot restore a hidden row",
            "The first lifecycle contract is one-way: a transition cannot start from hidden state and make the row visible again.",
            "Remove this transition or keep its resulting state hidden",
        ),
        "TYPE_LIFECYCLE_TRANSITION_SOURCE_INVALID" => (
            "Lifecycle transition must start from visible state",
            "Ordinary lifecycle transitions are available only to rows that satisfy the entity's current visibility predicate.",
            "Make the transition guard imply the declared lifecycle visibility predicate",
        ),
        "TYPE_LIFECYCLE_FIELD_UPDATE_FORBIDDEN" => (
            "Lifecycle-owned field cannot be changed by an ordinary update",
            "Lifecycle-owned state changes only through the entity's declared fixed transition, which adds its visibility and source-state guards to the write.",
            "Use the declared lifecycle transition instead of assigning this field directly",
        ),
        "TYPE_LIFECYCLE_INITIAL_FIELD_REQUIRED" => (
            "Transition or purge field has no initial value",
            "A field changed by a transition or used for retention must be initialized by the lifecycle contract.",
            "Add this field once to the entity lifecycle `initial` block",
        ),
        "TYPE_LIFECYCLE_PURGE_FIELD_INVALID" => (
            "Purge field must be a nullable instant",
            "Retention purge eligibility is anchored to a nullable deletion timestamp.",
            "Use a nullable `Instant` lifecycle timestamp field",
        ),
        "TYPE_LIFECYCLE_PURGE_INITIAL_INVALID" => (
            "Purge timestamp must start empty",
            "Only a soft-deleted row with a timestamp can become eligible for retention purge.",
            "Initialize the purge timestamp to `none`",
        ),
        "TYPE_LIFECYCLE_PURGE_TRANSITION_INVALID" => (
            "Purge timestamp must come from a declared soft-delete transition",
            "The worker can purge only a hidden post-transition state whose lifecycle-owned timestamp was set from the operation clock.",
            "Set the purge timestamp to `clock.now` in a lifecycle transition",
        ),
        "TYPE_LIFECYCLE_PURGE_RETENTION_INVALID" => (
            "Purge retention must bind to configuration",
            "Retention is a checked startup configuration value rather than an authored per-row expression.",
            "Use a configured duration such as `config.soft_delete_retention`",
        ),
        "TYPE_LIFECYCLE_PREDICATE_UNSUPPORTED" => (
            "Lifecycle predicate uses an unsupported expression",
            "Lifecycle visibility and transition guards are limited to typed same-row equality, absence, and conjunction.",
            "Rewrite this predicate using entity fields, enum variants, `none`, equality, and `and`",
        ),
        "TYPE_LIFECYCLE_TRANSITION_UNKNOWN" => (
            "Lifecycle transition is not declared on this entity",
            "A transition marker selects one fixed state change declared by the target entity.",
            "Use a transition declared in this entity's lifecycle block",
        ),
        "TYPE_LIFECYCLE_TRANSITION_IDENTITY_REQUIRED" => (
            "Lifecycle transition requires an identity-bounded target",
            "A transition changes one authoritative row selected by its declared identity field.",
            "Use the entity identity field in the update predicate",
        ),
        "TYPE_LIFECYCLE_READ_LOWERING_UNSUPPORTED" => (
            "Lifecycle reads are not yet available in the selected backend",
            "A lifecycle entity may be read only when visibility is conjoined before projection and pagination on every adapter.",
            "Keep reads off lifecycle entities until guarded lifecycle read lowering is supported",
        ),
        "TYPE_LIFECYCLE_MUTATION_LOWERING_UNSUPPORTED" => (
            "Lifecycle mutations are not yet available in the selected backend",
            "Every lifecycle mutation must evaluate visibility and transition invariants with its write in one authoritative transaction.",
            "Keep mutations off lifecycle entities until guarded mutation lowering is supported",
        ),
        "TYPE_LIFECYCLE_HARD_DELETE_FORBIDDEN" => (
            "Lifecycle entity cannot be physically deleted by authored code",
            "Lifecycle deletion is a state transition; physical removal belongs only to the bounded compiler-owned retention worker.",
            "Use the declared delete transition, or rely on the generated retention worker after its configured period",
        ),
        "TYPE_LIFECYCLE_TRANSITION_BODY_INVALID" => (
            "Lifecycle transition cannot be combined with direct field writes",
            "A transition is the sole write marker for lifecycle-owned fields and its field values are fixed by the entity contract.",
            "Remove `set` and `patch` and keep only `transition: name`",
        ),
        "TYPE_LIFECYCLE_FIELD_CREATE_FORBIDDEN" => (
            "Lifecycle-owned field cannot be supplied by a constructor",
            "The compiler applies the entity lifecycle's initial values during creation.",
            "Remove this field from the constructor",
        ),
        "TYPE_GENERATED_FIELD_CONTEXT" => (
            "Generated lifecycle role requires a persistent entity field",
            "Lifecycle timestamps describe database creation and change events and therefore have no defined meaning on transient input, output, or value objects.",
            "Move this field to a persistent entity or remove its generated block",
        ),
        "TYPE_GENERATED_IDENTITY_INVALID" => (
            "Generated identity must be a required UUID identity field",
            "A compiler-generated identity belongs only to the persistent entity's declared identity field, uses the UUID representation, and is always present.",
            "Mark the required UUID identity field as `generated: identity`, or supply the identity explicitly",
        ),
        "TYPE_GENERATED_FIELD_INPUT" => (
            "Generated lifecycle field cannot be accepted from input",
            "Copying a generated field through an input or patch would let callers choose a compiler-owned timestamp.",
            "Remove the generated field from the input or patch type",
        ),
        "TYPE_GENERATED_FIELD_TYPE" => (
            "Generated lifecycle field must be a required `Instant`",
            "Creation and change timestamps are always present absolute timeline values. Nullable, optional, date-only, and resolved display values cannot represent that role.",
            "Declare this field as required `Instant`",
        ),
        "TYPE_LOCALE_DEFAULT_UNSUPPORTED" => (
            "Default locale is not in the supported locale set",
            "Fallback can only select a locale whose data and generated enum value are part of the application contract.",
            "Add the default locale to `supported`, or choose a supported default",
        ),
        "TYPE_LOCALE_DUPLICATE" => (
            "Supported locale is repeated",
            "Each canonical BCP 47 locale creates one generated enum value and may appear only once.",
            "Keep one occurrence of this locale",
        ),
        "TYPE_LOCALE_INVALID" => (
            "Locale is not a valid supported BCP 47 tag",
            "Locale values use canonical language tags such as `en-GB`; arbitrary display names and host-specific locale strings are not portable.",
            "Use a valid BCP 47 language tag",
        ),
        "TYPE_PRESENTATION_TEXT_PERSISTENCE" => (
            "Human-formatted time text cannot be persisted as authority",
            "`PresentationText` is locale-specific output. Persisting it instead of the underlying `Instant`, `CalendarDate`, or resolved `Time` would lose authoritative temporal meaning.",
            "Persist the typed temporal value and format it only at the presentation boundary",
        ),
        "TYPE_TEMPORAL_CLOCK_INVALID" => (
            "Clock text is not in the strict temporal format",
            "Local clock input must use `HH:MM` with optional seconds and milliseconds before it can be resolved with a date and zone.",
            "Use a clock value such as `09:30` or `09:30:15.250`",
        ),
        "TYPE_TEMPORAL_FORMAT_CHOICE" => (
            "Temporal formatting needs exactly one format choice",
            "Absolute formatting uses either one named style or one structured component object; supplying both or neither would make output policy unclear.",
            "Supply exactly one of `style:` or `components:`",
        ),
        "TYPE_TEMPORAL_FORMAT_COMPONENT_DUPLICATE" => (
            "Temporal format component is repeated",
            "Each structured component controls one part of the formatted value and may be specified once.",
            "Keep one value for this format component",
        ),
        "TYPE_TEMPORAL_FORMAT_COMPONENT_INVALID" => (
            "Temporal format component value is not supported",
            "Structured formatting uses a closed, portable set of checked component values rather than host-specific formatter options.",
            "Choose one of the supported values for this component",
        ),
        "TYPE_TEMPORAL_FORMAT_COMPONENTS_EMPTY" => (
            "Temporal component format is empty",
            "A structured format must select at least one date, time, or zone component to produce meaningful presentation text.",
            "Add at least one supported format component",
        ),
        "TYPE_TEMPORAL_FORMAT_REQUIRES_TIME" => (
            "Formatting an instant requires an explicit zone",
            "An `Instant` has no human-local calendar representation. It must become a resolved `Time` before locale formatting can choose dates, hours, or zone names.",
            "Call `temporal.in_zone` before formatting this instant",
        ),
        "TYPE_TEMPORAL_LOCALES_REQUIRED" => (
            "Temporal formatting requires an application locale declaration",
            "A closed supported locale set prevents deployment hosts from silently accepting arbitrary locale strings or changing fallback behavior.",
            "Add one project-level `locales` declaration",
        ),
        "TYPE_ARITHMETIC_OPERAND" => (
            "Arithmetic requires numbers",
            "Operators such as `+`, `-`, `*`, and `/` accept compatible numeric values, not text, Boolean values, records, or unrelated named types.",
            "Use numbers on both sides, or choose an operation supported by these values",
        ),
        "TYPE_ASSIGN_IMMUTABLE" => (
            "Binding cannot be reassigned",
            "Parameters and ordinary `var` bindings are immutable. Only a local explicitly declared with `var mut` may receive a new value.",
            "Declare a mutable local when rebinding is intended, or create a new value",
        ),
        "TYPE_ASSIGN_UNKNOWN" => (
            "Assignment target does not exist",
            "Reassignment must target a visible local binding; declarations, fields, and unknown names cannot be assigned as locals.",
            "Correct the local name or declare it before assigning",
        ),
        "TYPE_CONSTRUCTOR_ARGUMENT_COUNT" => (
            "Type constructor needs exactly one value",
            "A named scalar type, such as `CustomerId(...)`, wraps one compatible value and then checks its constraints.",
            "Pass exactly one value between the parentheses",
        ),
        "TYPE_CONSTRUCTOR_INPUT" => (
            "Type constructor received the wrong type of value",
            "The value between the parentheses must match the type after `=` in this type's definition.",
            "Pass a value that matches the type definition",
        ),
        "TYPE_DUPLICATE_VARIANT_FIELD" => (
            "Enum variant payload field is repeated",
            "A data-carrying enum variant has one exact payload record; duplicate field names would make construction and pattern binding ambiguous.",
            "Keep one declaration of the variant payload field",
        ),
        "TYPE_FIELD_ON_NON_RECORD" => (
            "This value has no fields",
            "Only an object, failure context, or enum payload has named fields that can be selected with `.`.",
            "Use an object value before `.`, or remove the field access",
        ),
        "TYPE_INCOMPARABLE" => (
            "Values cannot be compared for equality",
            "Both sides of `==` or `!=` must have compatible types. Separately named types are not interchangeable just because they share the same base type.",
            "Compare values with the same type",
        ),
        "TYPE_INVALID_LITERAL" => (
            "Value does not meet this type's rules",
            "The written value breaks a constraint on the named type, such as length, range, pattern, or format.",
            "Change the value so it meets the type's constraints",
        ),
        "TYPE_LOGICAL_OPERAND" => (
            "Logical operator requires Boolean operands",
            "`and` and `or` combine Boolean conditions only; Jadpo does not coerce numbers, text, records, or optional values to truthiness.",
            "Use Boolean expressions on both sides of the logical operator",
        ),
        "TYPE_MISMATCH" => (
            "Value has the wrong type",
            "Jadpo keeps separately named types distinct and converts them only where the type definitions explicitly allow it.",
            "Use a value of the required type, or construct that type explicitly",
        ),
        "TYPE_MISSING_FIELD" => (
            "Object construction is missing a required field",
            "A constructed object must provide every required field from its declaration exactly once.",
            "Add the missing required field to the constructor",
        ),
        "TYPE_MISSING_VARIANT_FIELD" => (
            "Enum variant construction is missing a payload field",
            "This enum variant carries data and must provide every required field declared for that variant.",
            "Add the missing field to the variant payload",
        ),
        "TYPE_NOT_RECORD" => (
            "This type cannot be constructed with `{ ... }`",
            "Braces construct an object, failure context, or enum variant. A scalar named type uses parentheses instead.",
            "Use an object type with braces, or construct a scalar type with parentheses",
        ),
        "TYPE_REDUNDANT_NULLABILITY" => (
            "This type already permits `none`",
            "Field references inherit nullability from their declared type. Adding another `?` does not create a distinct kind of absence.",
            "Remove the redundant `?`; the referenced type remains nullable",
        ),
        "TYPE_NULLABLE_SELECTION" => (
            "Value may be `none`, so its field cannot be read yet",
            "A nullable object may contain no value. Jadpo needs the `some(...)` case before it can safely read a field.",
            "Use a nullable match and select the field inside the `some(...)` arm",
        ),
        "TYPE_ORDERING_OPERAND" => (
            "These values cannot be ordered",
            "`<`, `<=`, `>`, and `>=` require compatible ordered values. Records, Boolean values, lists, and unrelated named types have no defined order.",
            "Compare values of the same ordered type",
        ),
        "TYPE_PRIMITIVE_SIGNATURE" => (
            "Function or action signature uses a built-in type directly",
            "Parameters and return values must use a named Jadpo type so their meaning and validation rules are explicit.",
            "Define a named type and use it in this signature",
        ),
        "TYPE_SIBLING_MISMATCH" => (
            "Value has a different named type",
            "Two named types remain separate even when both build on the same base type. For example, an `OrderId` is not a `CustomerId`.",
            "Use the required named type, or construct it explicitly from an allowed value",
        ),
        "TYPE_UNARY_OPERAND" => (
            "Operator cannot be used with this value",
            "`not` requires a Boolean value, while numeric negation requires a supported number.",
            "Use a Boolean after `not`, or a number after `-`",
        ),
        "TYPE_UNKNOWN_ENUM_VARIANT" => (
            "Enum variant does not exist",
            "The selected variant is not declared by this closed enum, so it cannot be constructed or matched.",
            "Use one of the enum's declared variants",
        ),
        "TYPE_UNKNOWN_FIELD" => (
            "Record field does not exist",
            "This field is not declared on the record. Jadpo does not add fields dynamically at runtime.",
            "Correct the field name or add it to the record declaration",
        ),
        "TYPE_UNKNOWN_NAME" => (
            "Type name does not exist",
            "The type reference does not resolve to a visible built-in, local declaration, or selected module import.",
            "Correct, declare, or import the referenced type",
        ),
        "TYPE_UNKNOWN_VALUE" => (
            "Value name does not exist in this scope",
            "The expression does not resolve to a local binding, parameter, supported literal, or visible value declaration.",
            "Correct the value name or declare it in the current scope",
        ),
        "TYPE_UNKNOWN_VARIANT_FIELD" => (
            "Enum variant payload field does not exist",
            "This field is not declared on the selected enum variant.",
            "Correct the payload field name or declare it on the variant",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn match_type_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "TYPE_MATCH_DUPLICATE_BINDING" => (
            "Match pattern names the same field more than once",
            "Each enum payload field creates one local value inside its match arm. Repeating it would create two local values for the same field.",
            "Keep one local name for each payload field",
        ),
        "TYPE_MATCH_DUPLICATE_PATTERN" => (
            "Match pattern is already covered",
            "An earlier arm already handles this literal, enum variant, optional case, or wildcard, so this arm cannot select a new case.",
            "Remove the duplicate arm or change it to an uncovered case",
        ),
        "TYPE_MATCH_NON_EXHAUSTIVE" => (
            "Match does not cover every possible value",
            "Every enum variant, Boolean value, or nullable case must select a match arm.",
            "Add arms for every missing case",
        ),
        "TYPE_MATCH_PATTERN_TYPE" => (
            "Pattern cannot match this value",
            "This pattern belongs to a different enum or built-in type than the value being matched.",
            "Use a pattern from the value's type",
        ),
        "TYPE_MATCH_SOME_NON_OPTIONAL" => (
            "`some` pattern requires a nullable value",
            "`some(name)` handles the present case of a nullable value, but this value is already guaranteed to be present.",
            "Match the value directly, or make it nullable if absence is intended",
        ),
        "TYPE_MATCH_UNKNOWN_BINDING" => (
            "Pattern names a field this enum variant does not have",
            "The local name in this pattern must match a field declared on the selected enum variant.",
            "Use a field declared by the selected variant",
        ),
        "TYPE_MATCH_UNKNOWN_VARIANT" => (
            "Pattern names an unknown enum variant",
            "The variant is not part of the matched enum's closed set, so no value of the subject type can select this arm.",
            "Use one of the matched enum's declared variants",
        ),
        "TYPE_MATCH_UNREACHABLE_PATTERN" => (
            "Match arm is unreachable after the wildcard",
            "A wildcard arm accepts every value not selected earlier, so no later pattern can ever run.",
            "Move the wildcard to the final arm or remove the unreachable arm",
        ),
        "TYPE_MATCH_VARIANT_BINDINGS_REQUIRED" => (
            "Enum variant pattern must name its fields",
            "This enum variant carries fields. The pattern must name them so the match arm can use their values.",
            "Add the variant's field names in parentheses",
        ),
        "TYPE_MATCH_WILDCARD_REQUIRED" => (
            "Open value space requires a wildcard arm",
            "This value can have more values than the listed literal patterns cover.",
            "Add a final `_` arm for all remaining values",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn persistence_type_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "TYPE_CONFLICT_DUPLICATE_BINDING" => (
            "Constraint failure is mapped more than once",
            "Each named persistence constraint, and the optional fallback, may have only one conflict mapping so a database violation selects one domain failure.",
            "Keep one conflict mapping for this constraint",
        ),
        "TYPE_CONFLICT_UNKNOWN_CONSTRAINT" => (
            "Conflict mapping names an unknown constraint",
            "A specific conflict mapping must refer to a compiler-owned identity, unique, or named compound-unique constraint on the mutated entity.",
            "Use a declared persistence constraint from the target entity",
        ),
        "TYPE_CREATE_NOT_ENTITY" => (
            "Create target is not persisted",
            "`create` stores an object type that has a `persist` declaration. Ordinary object, enum, failure, and scalar types have no storage contract.",
            "Add an appropriate `persist` declaration, or use ordinary object construction",
        ),
        "TYPE_DELETE_NOT_ENTITY" => (
            "Delete target is not persisted",
            "`delete required` operates on rows for an object type with a `persist` declaration.",
            "Delete a persisted object type",
        ),
        "TYPE_INCLUDE_DUPLICATE_RELATIONSHIP" => (
            "Relationship is included more than once",
            "A to-many query result has one field per included relationship; repeating a relationship would make that output field and its loading plan ambiguous.",
            "Keep one include for each relationship",
        ),
        "TYPE_INCLUDE_MIXED_CARDINALITY" => (
            "To-many include set contains a non-many relationship",
            "The batched child-include form combines only `many` inverse relationships. Owning-parent and optional inverse loads use their dedicated single-include forms.",
            "Use `many` for every include in this set or split out the single-related load",
        ),
        "TYPE_INCLUDE_ORDER_NOT_DETERMINISTIC" => (
            "Included children are not ordered by a stable key",
            "A bounded to-many include must order by an identity or unique child field so generated results cannot change arbitrarily between executions.",
            "Order the included relationship by an identity or unique field",
        ),
        "TYPE_INCLUDE_PARENT_PAGINATION_REQUIRED" => (
            "Many-parent include requires parent pagination",
            "Including children for an unbounded many-parent query could expand an unlimited parent set before child loading.",
            "Add a literal parent `limit` and optional non-negative `offset`",
        ),
        "TYPE_INCLUDE_REQUIRED_PARENT" => (
            "Optional parent query cannot include children",
            "This output requires a parent value, but an optional query may find no parent.",
            "Use a required or many-parent query for this include",
        ),
        "TYPE_INCLUDE_RESULT_MISMATCH" => (
            "Includes use different result object types",
            "All relationships loaded by one query are returned together, so every include must name the same `into` object type.",
            "Use the same result object type for every include",
        ),
        "TYPE_INCLUDE_RESULT_NOT_OUTPUT" => (
            "Include result is not an object type",
            "An include needs a named `type ... = Object { ... }` whose fields exactly describe the parent and loaded relationships.",
            "Use an object type with the required parent and relationship fields",
        ),
        "TYPE_INCLUDE_RESULT_SHAPE" => (
            "Result fields do not match the included relationships",
            "The result object must contain exactly `parent` with the queried persisted type and one non-nullable `List<Child>` field for each included inverse relationship.",
            "Make the result fields exactly match the parent and included child lists",
        ),
        "TYPE_INCLUDE_UNKNOWN_RELATIONSHIP" => (
            "Included to-many relationship does not exist",
            "A to-many include must name a declared `inverse ... many` relationship on the queried parent object.",
            "Use a declared many-valued inverse relationship",
        ),
        "TYPE_INVERSE_ONE_CARDINALITY" => (
            "Inverse-one include must be optional",
            "A zero-or-one inverse can have no related child, so its include cardinality and output field must preserve that possibility.",
            "Mark the inverse-one include `optional`",
        ),
        "TYPE_INVERSE_ONE_REQUIRED_QUERY" => (
            "Inverse-one include requires a parent value",
            "The inverse-one output contains a concrete parent and is only defined for a required parent query.",
            "Use `query required` for this inverse-one include",
        ),
        "TYPE_INVERSE_ONE_RESULT_SHAPE" => (
            "Result fields do not match the optional inverse relationship",
            "The result object must contain exactly `parent` with the queried persisted type and a nullable child field named for the optional inverse relationship.",
            "Make the result object contain the exact parent and nullable inverse child fields",
        ),
        "TYPE_INVERSE_ONE_SINGLE" => (
            "Inverse-one query has more than one include",
            "The bounded zero-or-one loading plan supports exactly one optional inverse relationship per required parent query.",
            "Keep only one inverse-one include in this query",
        ),
        "TYPE_MUTATION_NULLABLE_FIELD_UNSUPPORTED" => (
            "Mutation predicate field is nullable",
            "The current required update and delete slice accepts a single equality predicate on a non-nullable persisted-object field; nullable equality semantics are not implicit.",
            "Use a non-nullable predicate field",
        ),
        "TYPE_MUTATION_UNKNOWN_PREDICATE_FIELD" => (
            "Mutation predicate field does not exist",
            "The field after `where:` must be declared on the persisted object being updated or deleted.",
            "Use a declared field from the target object",
        ),
        "TYPE_NESTED_INCLUDE_CARDINALITY" => (
            "Nested include must be optional",
            "The supported second hop is a unique-backed optional inverse, so the final related value may be absent.",
            "Mark the nested include `optional`",
        ),
        "TYPE_NESTED_INCLUDE_FIRST_HOP" => (
            "Nested include first hop is not an owning reference",
            "The bounded two-hop form must first follow a declared owning reference from the queried child to its parent.",
            "Use a declared owning relationship for the first hop",
        ),
        "TYPE_NESTED_INCLUDE_NULLABLE_FIRST_HOP" => (
            "Nested include first hop is nullable",
            "The supported two-hop plan requires its intermediate parent to exist; a nullable owning reference cannot guarantee a value for the second lookup.",
            "Use a non-nullable owning reference for the first hop",
        ),
        "TYPE_NESTED_INCLUDE_REQUIRED_QUERY" => (
            "Nested include requires a parent value",
            "The bounded two-hop output contains the queried entity as a concrete `parent`, so it is defined only for a required query.",
            "Use `query required` for this nested include",
        ),
        "TYPE_NESTED_INCLUDE_RESULT_SHAPE" => (
            "Result fields do not match the nested relationship",
            "The outer result object must contain exactly the queried `parent` and the named first-hop field whose object, in turn, contains its parent and nullable second-hop child.",
            "Make both object layers match the exact two-hop result shape",
        ),
        "TYPE_NESTED_INCLUDE_SECOND_HOP" => (
            "Nested include second hop is not an optional inverse",
            "After the owning-reference hop, the supported bounded plan may follow only a declared unique-backed optional inverse relationship.",
            "Use an optional inverse relationship for the second hop",
        ),
        "TYPE_NESTED_INCLUDE_SINGLE" => (
            "Nested query has more than one include",
            "The bounded two-hop loading plan supports one explicit nested path and does not combine it with additional includes.",
            "Keep only the nested include in this query",
        ),
        "TYPE_PARENT_INCLUDE_NULLABLE_REFERENCE" => (
            "Required parent include follows a nullable reference",
            "A nullable owning reference may contain `none`, so the related parent cannot be promised as a non-nullable required output field.",
            "Mark the include optional or use a non-nullable reference",
        ),
        "TYPE_PARENT_INCLUDE_REQUIRED_QUERY" => (
            "Owning-parent include requires a child value",
            "The owning-parent output contains the queried child as a concrete `parent`, so it is defined only for a required child query.",
            "Use `query required` for this owning-parent include",
        ),
        "TYPE_PARENT_INCLUDE_RESULT_SHAPE" => (
            "Result fields do not match the parent relationship",
            "The result object must contain exactly `parent` with the queried child type and a relationship field whose nullability matches the include cardinality.",
            "Make the result object contain the exact child and related-parent fields",
        ),
        "TYPE_PARENT_INCLUDE_SINGLE" => (
            "Owning-parent query has more than one include",
            "The bounded owning-reference loading plan supports exactly one related parent per required child query.",
            "Keep only one owning-parent include in this query",
        ),
        "TYPE_PARENT_INCLUDE_UNKNOWN_REFERENCE" => (
            "Included parent relationship is not an owning reference",
            "An owning-parent include must name a stored reference declared on the queried child object, using its explicit relationship name when present.",
            "Use a declared owning relationship from the queried object",
        ),
        "TYPE_PATCH_CONDITION_BINDING" => (
            "Patch supplied-condition uses the wrong binding",
            "A `when ... supplied` condition must select one field directly from the same patch input binding named by `patch:`.",
            "Use `<patch_binding>.<field> supplied` in the condition",
        ),
        "TYPE_PATCH_CONDITION_UNKNOWN_FIELD" => (
            "Patch supplied-condition names an unknown field",
            "A `supplied` check can name only a field declared on the patch input.",
            "Use a field declared on the patch input",
        ),
        "TYPE_PATCH_DERIVED_OVERLAP" => (
            "Patch and derived update write the same field",
            "A field cannot be written by both `patch:` and `set:` because the result would depend on an implicit update order.",
            "Remove the field from either the patch input or the derived `set` block",
        ),
        "TYPE_PATCH_FIELD_NOT_OPTIONAL" => (
            "Patch input field is not omission-aware",
            "Every patch input field must use the `optional` presence modifier so omission can be distinguished from supplying a value, including `none` for nullable fields.",
            "Mark every patch input field `optional`",
        ),
        "TYPE_PATCH_FIELD_REQUIRED" => (
            "Patch input has no fields",
            "A patch object must declare at least one persisted-object field; an empty object can never describe a change.",
            "Add at least one optional persisted-object field to the patch object",
        ),
        "TYPE_PATCH_INPUT_BINDING" => (
            "Patch must name a direct input binding",
            "`patch:` accepts one local or parameter name so the compiler can track each field's supplied flag; field selections and longer paths are not patch bindings.",
            "Pass the patch input binding directly",
        ),
        "TYPE_PATCH_NOT_INPUT" => (
            "Patch binding is not an omission-aware object",
            "`patch:` needs a non-persistent object type whose fields use `optional`, so Jadpo can distinguish an omitted field from a supplied value.",
            "Use a non-persistent object type with optional patch fields",
        ),
        "TYPE_PATCH_UNKNOWN_FIELD" => (
            "Patch object contains a field not on the persisted object",
            "Every patch field must map statically to a declared field on the target object so the generated update remains fixed and type checked.",
            "Remove the field or declare the corresponding field on the target object",
        ),
        "TYPE_QUERY_NOT_ENTITY" => (
            "Query target is not persisted",
            "`query` reads rows for an object type with a `persist` declaration; ordinary objects and scalar types have no storage contract.",
            "Query a persisted object type",
        ),
        "SYN_QUERY_PAGE_AFTER_REQUIRED" => (
            "Page query requires an explicit cursor boundary",
            "A page query must state how an optional continuation cursor is applied so the generated lookup never silently switches to offset pagination.",
            "Add `after: optional cursor_value` before the page limit",
        ),
        "SYN_QUERY_PAGE_PROJECT_REQUIRED" => (
            "Page query requires a named projection",
            "A bounded page must declare the output projection that the compiler selects and validates.",
            "Add `project: OutputShape` before the cursor declaration",
        ),
        "SYN_QUERY_PAGE_CURSOR_REQUIRED" => (
            "Page query requires a typed cursor shape",
            "The continuation token needs a named value shape and explicit ordered fields so the compiler can validate its keyset predicate.",
            "Add `cursor: CursorShape(field, ...)` matching `order_by`",
        ),
        "SYN_QUERY_PAGE_PREDICATE_OPERATOR" => (
            "Unsupported page predicate operator",
            "Page predicates support equality, optional equality filters and optional less-than-or-equal filters with statically bound values.",
            "Use `==`, `matches optional` or `<= optional`",
        ),
        "TYPE_QUERY_NULLABLE_FIELD_UNSUPPORTED" => (
            "Query predicate field is nullable",
            "The current query slice accepts a single equality predicate on a non-nullable field; nullable equality and `none` matching are not implicit.",
            "Use a non-nullable predicate field",
        ),
        "TYPE_QUERY_ORDER_NOT_DETERMINISTIC" => (
            "Query ordering field is not a stable key",
            "A many-result query must order by an identity or unique field so its row order and pagination boundary are deterministic.",
            "Order by an identity or unique field",
        ),
        "TYPE_QUERY_PAGINATION_CONSTANT_REQUIRED" => (
            "Pagination bound must be a literal integer",
            "Compile-time literal bounds keep the current query plan explicitly and statically bounded; dynamic limits and offsets are not part of this language slice.",
            "Use an integer literal for `limit` or `offset`",
        ),
        "TYPE_QUERY_PAGINATION_RANGE" => (
            "Pagination bound is outside its allowed range",
            "`limit` must be a positive integer and `offset` must be a non-negative integer.",
            "Use `limit` of at least 1 and `offset` of at least 0",
        ),
        "TYPE_QUERY_UNKNOWN_FIELD" => (
            "Query predicate field does not exist",
            "The field after `where:` must be declared on the persisted object being queried.",
            "Use a declared field from the target object",
        ),
        "TYPE_QUERY_UNKNOWN_ORDER_FIELD" => (
            "Query ordering field does not exist",
            "`order_by` must name a declared field on the queried persisted object before its stability can be checked.",
            "Use a declared field from the target object",
        ),
        "TYPE_QUERY_PAGE_SHAPE" => (
            "Page query shapes do not match",
            "A keyset page's output, projection and cursor must map to declared fields on the queried entity, and the cursor field order must match the stable sort order.",
            "Align the output page items, projection fields and cursor fields with the query ordering",
        ),
        "TYPE_UPDATE_DUPLICATE_FIELD" => (
            "Update writes the same field more than once",
            "Each persisted-object field may have only one fixed or conditional `set` write so the mutation has a single deterministic value for that column.",
            "Keep one `set` write for this field",
        ),
        "TYPE_UPDATE_FIELD_REQUIRED" => (
            "Update does not write any fields",
            "A required update must contain at least one fixed, conditional, or patch-derived field change; a predicate alone performs no mutation.",
            "Add a `set` change or a `patch` binding",
        ),
        "TYPE_UPDATE_NOT_ENTITY" => (
            "Update target is not persisted",
            "`update required` mutates rows for an object type with a `persist` declaration.",
            "Update a persisted object type",
        ),
        "TYPE_UPDATE_UNKNOWN_FIELD" => (
            "Updated field does not exist on the persisted object",
            "Every field after `set:` must be declared on the object being updated.",
            "Use a declared field from the target object",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn module_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "MOD_DUPLICATE_IMPORT" => (
            "Declaration is imported more than once",
            "A module scope may bind an imported declaration name only once; repeated imports do not create a second distinct value or type.",
            "Keep one import of this declaration",
        ),
        "MOD_DUPLICATE_MODULE" => (
            "Module name is declared by more than one file",
            "Each explicit module has one source file so its exports, imports, and declaration scope have a single owner.",
            "Give each module file a unique module name",
        ),
        "MOD_IMPORT_CONFLICT" => (
            "Imported name conflicts with a local declaration",
            "The module already declares this name locally, so importing another declaration under the same name would make references ambiguous.",
            "Remove the import or rename one of the declarations",
        ),
        "MOD_IMPORT_CYCLE" => (
            "Module imports form a cycle",
            "Modules must have an acyclic dependency order so declarations can be resolved and generated deterministically.",
            "Move shared declarations or remove an import to break the cycle",
        ),
        "MOD_IMPORT_REQUIRED" => (
            "Declaration is used without being imported",
            "In explicit-module mode, declarations from another file are visible only when selected by an import in the current module.",
            "Import the declaration from its module",
        ),
        "MOD_MODULE_REQUIRED" => (
            "Source file is missing its module declaration",
            "Once any source uses explicit modules, every source file must declare its module so cross-file visibility is deterministic.",
            "Add a module declaration to this file",
        ),
        "MOD_STANDARD_NAMESPACE_IMPORT" => (
            "Standard-library namespace cannot be imported",
            "Compiler-owned namespaces are already in scope and must remain qualified; selective imports would create a second unqualified call form.",
            "Remove this import and call the operation through its standard namespace",
        ),
        "MOD_STANDARD_NAMESPACE_RESERVED" => (
            "Module name conflicts with a standard-library namespace",
            "The first module segment cannot be `temporal`, `collection`, or another compiler-owned namespace because authored modules are not runtime namespace objects.",
            "Choose an application-owned lower_snake_case module name",
        ),
        "MOD_PRIVATE_IMPORT" => (
            "Imported declaration is private to its module",
            "The target declaration exists but is not listed as public by its owning module, so another module cannot select it.",
            "Export the declaration from its module or stop importing it",
        ),
        "MOD_PUBLIC_REQUIRES_MODULE" => (
            "Public export requires explicit modules",
            "A public declaration needs an owning module boundary; export markers cannot be used while the project is in implicit single-scope mode.",
            "Declare modules for the project or remove the public export",
        ),
        "MOD_SELF_IMPORT" => (
            "Module imports itself",
            "Declarations in the current module are already in scope, and a self-import would create a dependency cycle without adding visibility.",
            "Remove the self-import",
        ),
        "MOD_UNKNOWN_EXPORT" => (
            "Imported declaration does not exist in the target module",
            "The selected name is neither a public export nor a private declaration in the module being imported.",
            "Correct the imported name or add and export that declaration",
        ),
        "MOD_UNKNOWN_MODULE" => (
            "Imported module does not exist",
            "No source file declares the module named by this import, so its selected declarations cannot be resolved.",
            "Correct the module path or add the missing module file",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn tooling_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "FMT_CHANGES_REQUIRED" => (
            "Source files are not canonically formatted",
            "The formatter check found source whose canonical Jadpo layout differs from the checked-in text.",
            "Run `jadpo fmt` and review the resulting source changes",
        ),
        "FMT_WRITE_FAILED" => (
            "Formatted source could not be written",
            "The formatter produced canonical text but the source file could not be replaced, commonly because of filesystem permissions or an I/O failure.",
            "Make the source writable and run the formatter again",
        ),
        "LSP_CONTENT_LENGTH_MISSING" => (
            "Language-server message has no Content-Length header",
            "The editor connection sent an LSP frame without the byte length required to read exactly one JSON-RPC message.",
            "Restart the editor connection and check the LSP client transport",
        ),
        "LSP_JSON_INVALID" => (
            "Language-server message is not valid JSON",
            "The editor connection delivered a complete frame whose body could not be decoded as a JSON-RPC message.",
            "Restart the editor connection and inspect the client message payload",
        ),
        "LSP_JSON_WRITE_FAILED" => (
            "Language-server response could not be encoded",
            "The server produced a response that could not be serialized as JSON for the editor connection.",
            "Restart the language server and report the request that triggered this response",
        ),
        "LSP_READ_FAILED" => (
            "Language-server input could not be read",
            "The editor connection closed or failed while the server was reading an LSP header or message body.",
            "Restart the editor connection; if it recurs, inspect the client transport logs",
        ),
        "LSP_ROOT_MISSING" => (
            "Language server has no workspace root",
            "The editor did not provide a workspace folder or root URI, so the server cannot discover the Jadpo project and its source files.",
            "Open the Jadpo project folder as an editor workspace and restart the server",
        ),
        "LSP_WRITE_FAILED" => (
            "Language-server response could not be written",
            "The editor connection closed or failed while the server was sending a framed JSON-RPC response.",
            "Restart the editor connection; if it recurs, inspect the client transport logs",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn index_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "INDEX_ACCEPT_CHECK_FAILED" => (
            "Index edit did not pass project checks",
            "After inserting the recommended `index` modifier, the compiler found diagnostics and refused to retain an unchecked source change.",
            "Review the project diagnostics before accepting the recommendation again",
        ),
        "INDEX_ACCEPT_IDENTITY_COUNT" => (
            "Index acceptance changed an unexpected number of schema identities",
            "Accepting one recommendation must register exactly one new index identity; any other count means the source and identity registry are not changing atomically as expected.",
            "Restore schema identity consistency and retry the single recommendation",
        ),
        "INDEX_ACCEPT_RANGE_INVALID" => (
            "Recommended index insertion point is no longer valid",
            "The recorded field range does not match the current source text, so inserting `index` could edit the wrong bytes.",
            "Re-run the index recommendation command against the current sources",
        ),
        "INDEX_ACCEPT_REGISTRY_MISSING" => (
            "Schema identity registry is required before accepting an index",
            "The accepted index becomes durable schema identity and cannot be written safely until the project registry has been initialized.",
            "Run `jadpo schema init` for this project, then accept the index again",
        ),
        "INDEX_ACCEPT_ROLLBACK_FAILED" => (
            "Failed index edit could not be rolled back",
            "The compiler rejected the edited project but could not restore the original source file, so manual recovery is required before another attempt.",
            "Restore the source from version control and inspect filesystem permissions",
        ),
        "INDEX_ACCEPT_SOURCE_READ_FAILED" => (
            "Source file for the recommended index could not be read",
            "The recommendation resolved to a source file that became unavailable before the compiler could prepare its checked edit.",
            "Restore read access to the source and re-run the recommendation",
        ),
        "INDEX_ACCEPT_SOURCE_WRITE_FAILED" => (
            "Recommended index could not be written to source",
            "The compiler prepared the checked `index` insertion but the source file could not be updated.",
            "Make the source writable and accept the recommendation again",
        ),
        "INDEX_RECOMMENDATION_AVAILABLE" => (
            "A query field could benefit from an index",
            "Static query analysis found a predicate or ordering field without a matching persistence index; this is advice and does not change source automatically.",
            "Inspect the recommendation and explicitly accept it if the storage trade-off is appropriate",
        ),
        "INDEX_RECOMMENDATION_FIELD_MISSING" => (
            "Recommended field no longer exists",
            "The selected recommendation refers to an entity field that is absent from the currently analysed sources.",
            "Re-run index recommendation and choose an item from the current results",
        ),
        "INDEX_RECOMMENDATION_UNKNOWN" => (
            "Index recommendation is not current",
            "The requested entity-field recommendation is not present in the latest static query evidence for this project.",
            "List current index recommendations and choose one of their identifiers",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn toolchain_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "JADPO_ARTIFACT_CLEANUP_FAILED" => (
            "Previous build backup could not be removed",
            "The new artifact set was promoted successfully, but cleanup of the temporary backup directory failed.",
            "Inspect permissions on the project build directories and remove the stale backup safely",
        ),
        "JADPO_ARTIFACT_PROMOTE_FAILED" => (
            "Staged build artifacts could not be promoted",
            "The compiler generated a complete staging directory but could not atomically replace the project build output; the previous complete output was preserved when possible.",
            "Restore write and rename access to the project directory, then build again",
        ),
        "JADPO_ARTIFACT_STAGE_FAILED" => (
            "Build staging directory could not be created",
            "Artifact generation writes to a project-local staging directory before promotion so an incomplete build never replaces the last complete output.",
            "Make the project directory writable and build again",
        ),
        "JADPO_ARTIFACT_WRITE_FAILED" => (
            "Generated artifact could not be written",
            "The compiler could not create a directory or file inside its isolated build staging area, so the staged output was discarded.",
            "Check available space and project-directory permissions, then build again",
        ),
        "JADPO_EMPTY_PROJECT" => (
            "Project contains no declarations",
            "Jadpo source files were found and parsed without syntax errors, but none declares a type, enum, record, failure, callable, test, or route to compile.",
            "Add at least one top-level Jadpo declaration",
        ),
        "JADPO_NO_SOURCES" => (
            "Project contains no Jadpo source files",
            "Source discovery found no files ending in `.jadpo` beneath the requested project path, excluding generated build output.",
            "Add a `.jadpo` source file or select the correct project path",
        ),
        "JADPO_PROJECT_NOT_FOUND" => (
            "Project path does not exist",
            "The requested file or directory was not present when Jadpo began source discovery.",
            "Correct the project path or create the project first",
        ),
        "JADPO_PROJECT_READ_FAILED" => (
            "Project directory could not be read",
            "Source discovery could not enumerate part of the project tree, so it cannot know which Jadpo files belong to the build.",
            "Restore directory read access and try again",
        ),
        "JADPO_SCAFFOLD_DESTINATION_EXISTS" => (
            "Scaffold destination is an existing file",
            "A new Jadpo project needs a directory destination; the selected path already belongs to a non-directory filesystem entry.",
            "Choose a new directory path for the project",
        ),
        "JADPO_SCAFFOLD_DESTINATION_NOT_EMPTY" => (
            "Scaffold destination directory is not empty",
            "Project creation refuses to overwrite or mix generated starter files with existing directory contents.",
            "Choose an empty directory or a new destination",
        ),
        "JADPO_SCAFFOLD_NAME_INVALID" => (
            "Project destination has no valid name",
            "The scaffold manifest derives its project name from the final destination path component, which is missing or not valid text.",
            "Choose a destination path with a valid final directory name",
        ),
        "JADPO_SCAFFOLD_READ_FAILED" => (
            "Scaffold destination could not be inspected",
            "The project creator could not read the destination directory to prove that writing starter files would not overwrite existing content.",
            "Restore read access or choose another destination",
        ),
        "JADPO_SCAFFOLD_WRITE_FAILED" => (
            "Project scaffold file could not be written",
            "The project creator could not create one of the required starter directories or files.",
            "Restore write access and available space, then create the project again",
        ),
        "JADPO_SOURCE_READ_FAILED" => (
            "Jadpo source file could not be read",
            "Source discovery found the file, but its text became unavailable before parsing and checking could begin.",
            "Restore read access to the source file and retry",
        ),
        "JADPO_TARGET_AUTH_NOT_IMPLEMENTED" => (
            "Protected route `{route}` cannot be generated yet",
            "The application's selected authentication adapters, configuration or principal mapping are not fully supported by the generated runtime for `{route}`. Generation stops rather than weakening access control.",
            "Complete the supported authentication adapter configuration and principal mapping for `{route}`",
        ),
        "JADPO_TARGET_DURABLE_WORKFLOW_NOT_IMPLEMENTED" => (
            "Durable workflow target execution is not implemented",
            "The compiler can check and audit the accepted durable-workflow disposition, but persisted step state, compensation, reconciliation, and operator intervention still require their parked runtime contract.",
            "Keep the checked workflow contract and wait for the durable-workflow runtime decision",
        ),
        "JADPO_TARGET_DEPENDENCY_MANIFEST" => (
            "Generated target attempted to add a dependency manifest",
            "Jadpo's generated runtime must remain self-contained and may not emit package manifests, lockfiles, or vendored dependency directories.",
            "Remove the generated dependency artifact from the target generator",
        ),
        "JADPO_TARGET_EXTERNAL_MODULE" => (
            "Generated target imports an external module",
            "Generated TypeScript may use the Bun runtime and compiler-emitted relative modules only; undeclared external packages would make the output non-hermetic.",
            "Replace the import with supported runtime functionality or a generated relative module",
        ),
        "JADPO_TARGET_LIVENESS_NOT_LOCAL" => (
            "Authored liveness route is not dependency-free",
            "GET /health/live must remain public and local before readiness or authentication. This target supports only an input-free inline return of an output record containing literals or declared enum constants.",
            "Use auth: none and a single constant output return without inputs, calls, clocks, configuration or declared failures",
        ),
        "JADPO_TARGET_STORE_NOT_IMPLEMENTED" => (
            "Authority store target adapter is not implemented",
            "The current executable target supports the `primary` PostgreSQL or SQLite authority domain; physical adapters for other named authority stores remain parked.",
            "Use `store: primary` for this target or wait for the named adapter contract",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn cli_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "CLI_BUILD_ARGUMENTS" => (
            "Build command has invalid arguments",
            "Build accepts one optional project and one target: bun, native (also rust), or wasm. The default Bun target requires a project.",
            "Use `jadpo build <project>` or `jadpo build --target native|wasm` in the experiment checkout",
        ),
        "CLI_BUILD_EXPERIMENT_SCOPE" => (
            "Experimental target only builds qualified fixtures",
            "The native and WASM adapters are qualified only for the local conformance and auth-policy fixtures; general project compilation is not enabled.",
            "Run `jadpo build --target native|wasm` from the repository checkout to build the fixture",
        ),
        "CLI_BUILD_EXPERIMENT_UNAVAILABLE" => (
            "Shared-Rust experiment checkout was not found",
            "The experimental build needs the checked-in generator, fixture, and adapter sources; they are not bundled into the installed CLI.",
            "Run this build from the Jadpo repository checkout",
        ),
        "CLI_BUILD_TOOL_FAILED" => (
            "Experimental target build failed",
            "The local build tool could not start or exited unsuccessfully; no successful build is claimed.",
            "Inspect the build output and the prerequisites in experiments/native-conformance/README.md",
        ),
        "TEST_NO_TESTS" => (
            "Project has no executable tests",
            "The project builds, but it declares no tests for the test command to execute.",
            "Add a test declaration and run `jadpo test` again",
        ),
        "TEST_RUNTIME_START_FAILED" => (
            "Test runtime could not start",
            "Jadpo built the tests but could not launch the Bun executable.",
            "Make Bun available on PATH and run `jadpo test` again",
        ),
        "TEST_FAILED" => (
            "Test run failed",
            "The generated test process exited unsuccessfully; its report identifies the failing test or startup error.",
            "Inspect the test report, correct the failure, and run `jadpo test` again",
        ),
        "CLI_CONFIG_ARGUMENTS" => (
            "Config command has invalid arguments",
            "`config` accepts `set <field>` or `check` from inside the current project.",
            "Use `jadpo config set <field>` or `jadpo config check`",
        ),
        "CLI_CONFIG_TTY_REQUIRED" => (
            "Configuration entry requires an interactive terminal",
            "Secret-safe prompting cannot read from a pipe, command argument, or captured agent stream.",
            "Run the command yourself in an interactive terminal",
        ),
        "CLI_CONFIG_PROMPT_FAILED" => (
            "Secure configuration prompt failed",
            "Jadpo could not read the terminal value or safely control terminal echo.",
            "Restore terminal access and run the command again",
        ),
        "CLI_CHECK_ARGUMENTS" => (
            "Check command has invalid arguments",
            "`check` accepts one project path plus the optional supported diagnostic presentation flags.",
            "Use `jadpo check <project>` with any presentation flags",
        ),
        "CLI_DEV_ARGUMENTS" => (
            "Dev command has invalid arguments",
            "`dev` accepts one project path and an optional JSON diagnostic format; other positional arguments are not defined.",
            "Use `jadpo dev <project> [--diagnostic-format=json]`",
        ),
        "CLI_DEV_BUN_START_FAILED" => (
            "Bun could not start the generated runtime",
            "The dev command built the target but could not launch `bun --no-install`, so no runtime process is available.",
            "Confirm Bun is installed and executable, then run dev again",
        ),
        "CLI_DEV_PORT_INVALID" => (
            "Development port is invalid",
            "The `PORT` environment value must be an integer from 1 through 65535 before the generated runtime can bind it.",
            "Set `PORT` to an available non-zero TCP port",
        ),
        "CLI_DEV_READINESS_TIMEOUT" => (
            "Generated runtime did not become ready",
            "The runtime process stayed alive but its local `/health` endpoint did not report readiness within the bounded startup window.",
            "Inspect the runtime startup output and verify the configured port is available",
        ),
        "CLI_DEV_ROLLBACK_CLEANUP_FAILED" => (
            "Previous runtime revision could not be cleaned up",
            "The replacement runtime became ready, but its temporary last-known-good artifact backup could not be removed.",
            "Remove the reported `.jadpo-runtime-backup` directory after confirming the ready runtime is healthy",
        ),
        "CLI_DEV_ROLLBACK_PREPARE_FAILED" => (
            "Last-known-good runtime revision could not be preserved",
            "The dev supervisor could not move the current checked build aside before compiling a replacement, so it refused to risk losing rollback continuity.",
            "Restore project-directory write access and let dev retry the edit",
        ),
        "CLI_DEV_ROLLBACK_RESTORE_FAILED" => (
            "Last-known-good runtime revision could not be restored",
            "A replacement runtime failed readiness and the dev supervisor could not atomically restore the preserved checked build.",
            "Stop dev, restore the `.jadpo-runtime-backup` directory as `build`, and restart the session",
        ),
        "CLI_DEV_RUNTIME_EXITED" => (
            "Generated runtime exited unexpectedly",
            "The dev supervisor observed the Bun process terminate before or after readiness, so it can no longer serve the checked build.",
            "Inspect the runtime output, correct the reported fault, and let dev rebuild",
        ),
        "CLI_DEV_RUNTIME_STATUS_FAILED" => (
            "Generated runtime status could not be read",
            "The dev supervisor could not determine whether the child runtime was still running, so it cannot safely report readiness.",
            "Stop the dev session and start a fresh runtime process",
        ),
        "CLI_DEV_TARGET_MISSING" => (
            "Generated runtime entrypoint is missing",
            "The dev command could not resolve `build/target/app.ts`, so there is no checked artifact to execute.",
            "Run a successful build for the project and start dev again",
        ),
        "CLI_FMT_ARGUMENTS" => (
            "Format command has invalid arguments",
            "`fmt` accepts one project path and only the optional `--check` mode.",
            "Use `jadpo fmt <project> [--check]`",
        ),
        "CLI_INCIDENT_ARGUMENTS" => (
            "Incident command has invalid arguments",
            "Local enrichment requires exactly the checked project and one secret-safe runtime event JSON file.",
            "Use `jadpo incident <project> <event-json-file>`",
        ),
        "CLI_SIGNAL_HANDLER_FAILED" => (
            "Shutdown handler could not be installed",
            "The watch or dev command cannot guarantee structured interruption and child-process cleanup without the platform signal handler.",
            "Stop any existing watcher in this process and start a fresh watch or dev command",
        ),
        "CLI_INCIDENT_INVALID" => (
            "Runtime event is not a valid enrichment input",
            "The file must be a bounded `operational_log_event` with a supported runtime classification, semantic operation identifier, and generated request identifier.",
            "Provide an unchanged secret-safe event emitted by the generated runtime",
        ),
        "CLI_INCIDENT_MANIFEST_INVALID" => (
            "Local build manifest is not valid JSON",
            "Incident enrichment cannot safely map an operation to source when `build/app.meta.json` is malformed.",
            "Rebuild the exact project revision to regenerate the manifest",
        ),
        "CLI_INCIDENT_MANIFEST_MISSING" => (
            "Local build manifest is missing",
            "Incident enrichment needs the checked operation-to-source map from `build/app.meta.json` and never infers source locations from event payloads.",
            "Run `jadpo build` for the exact source revision before enrichment",
        ),
        "CLI_INCIDENT_MANIFEST_STALE" => (
            "Local build manifest does not match current sources",
            "The manifest's checked source revision differs from the project now on disk, so its operation locations are not trustworthy.",
            "Build the current sources or restore the source revision that produced the event",
        ),
        "CLI_INCIDENT_OPERATION_UNKNOWN" => (
            "Runtime operation is absent from the local manifest",
            "The event names a semantic operation that the matching local build metadata does not contain.",
            "Use the manifest and sources from the build that emitted this event",
        ),
        "CLI_INCIDENT_READ_FAILED" => (
            "Runtime event file could not be read",
            "The incident command could not load the selected local event JSON file.",
            "Restore read access and pass the correct event file path",
        ),
        "CLI_LSP_ARGUMENTS" => (
            "Language-server command has invalid arguments",
            "`jadpo lsp` runs over standard input and output and does not accept a project path or positional options.",
            "Run `jadpo lsp` without additional arguments",
        ),
        "CLI_PROJECT_REQUIRED" => (
            "Command requires a project path",
            "This command needs a Jadpo file or project directory before it can discover, check, or generate sources.",
            "Provide the project path after the command",
        ),
        "CLI_SCHEMA_COMMAND_REQUIRED" => (
            "Schema command requires a subcommand",
            "`schema` groups explicit identity, decision, plan, SQL-review, and index operations; it does not perform a default mutation.",
            "Choose a supported `jadpo schema` subcommand and project",
        ),
        "CLI_SCHEMA_DECISION_ARGUMENTS" => (
            "Schema decision command has invalid arguments",
            "Decision templates and validation require the current project, an `--against` snapshot, and the appropriate output or decisions file.",
            "Use the documented `schema decision-template` or `schema decision-check` form",
        ),
        "CLI_SCHEMA_DIFF_ARGUMENTS" => (
            "Schema diff command has invalid arguments",
            "A schema diff compares the current project against one explicit immutable snapshot.",
            "Use `jadpo schema diff <project> --against <snapshot>`",
        ),
        "CLI_SCHEMA_INDEX_ACCEPT_ARGUMENTS" => (
            "Index acceptance requires a recommendation path",
            "The checked edit must identify one current recommendation by its `Entity.field` path.",
            "Use `jadpo schema index-accept <project> <Entity.field>`",
        ),
        "CLI_SCHEMA_PLAN_ARGUMENTS" => (
            "Schema plan command has invalid arguments",
            "A migration plan requires an earlier snapshot, reviewed decisions, an explicit adapter, and an output file.",
            "Use `jadpo schema plan <project> --against <snapshot> --decisions <file> --adapter <adapter> <output>`",
        ),
        "CLI_SCHEMA_RENAME_ARGUMENTS" => (
            "Schema rename command has invalid arguments",
            "Identity-preserving rename requires a kind plus exact old and new semantic paths.",
            "Use `jadpo schema rename <project> <entity|field> <old> <new>`",
        ),
        "CLI_SCHEMA_SNAPSHOT_ARGUMENTS" => (
            "Schema snapshot command requires an output path",
            "The immutable identity snapshot is written only to an explicit destination and never overwrites the registry implicitly.",
            "Use `jadpo schema snapshot <project> <output>`",
        ),
        "CLI_SCHEMA_SQL_ARGUMENTS" => (
            "Schema SQL command has invalid arguments",
            "A SQL review requires an earlier snapshot, reviewed decisions, an explicit adapter, and an output file.",
            "Use `jadpo schema sql <project> --against <snapshot> --decisions <file> --adapter <adapter> <output>`",
        ),
        "CLI_UNKNOWN_COMMAND" => (
            "Unknown Jadpo command",
            "The first argument does not name a supported top-level Jadpo operation.",
            "Run `jadpo help` and choose a listed command",
        ),
        "CLI_UNKNOWN_SCHEMA_COMMAND" => (
            "Unknown schema subcommand",
            "The selected name is not one of the explicit schema identity, decision, planning, SQL-review, or index operations.",
            "Run `jadpo help` and choose a supported schema subcommand",
        ),
        "CLI_WATCH_ARGUMENTS" => (
            "Watch command has invalid arguments",
            "`watch` accepts one project path and an optional JSON diagnostic format; other positional arguments are not defined.",
            "Use `jadpo watch <project> [--diagnostic-format=json]`",
        ),
        "CLI_WATCH_INPUT_READ_FAILED" => (
            "Watched project inputs could not be read",
            "The watcher could not enumerate the project or read a tracked source while creating a stable change snapshot.",
            "Restore project read access; watching will retry on the next change",
        ),
        "CLI_WATCH_OUTPUT_FAILED" => (
            "Watch lifecycle event could not be written",
            "The watcher could not send its human or JSON event to the output stream, so consumers may have an incomplete revision sequence.",
            "Restart the watch process with a writable output stream",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn migration_workflow_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "MIG_DECISION_ARTIFACT_EXISTS" => (
            "Migration decision file already exists",
            "Decision templates are immutable review inputs and are never overwritten because doing so could erase authored approval evidence.",
            "Choose a new output path or review the existing decision file",
        ),
        "MIG_DECISION_ARTIFACT_INVALID" => (
            "Migration decision file is invalid",
            "The decision JSON does not match the versioned schema required to bind strategies and evidence to exact schema changes.",
            "Regenerate a decision template and transfer reviewed decisions into its schema",
        ),
        "MIG_DECISION_ARTIFACT_READ_FAILED" => (
            "Migration decision file could not be read",
            "The selected reviewed decision artifact was unavailable before validation could bind it to the current change set.",
            "Restore read access and retry with the reviewed decision file",
        ),
        "MIG_DECISION_ARTIFACT_WRITE_FAILED" => (
            "Migration decision template could not be written",
            "The compiler derived the required decisions but could not write the immutable review artifact.",
            "Choose a writable output path and generate the template again",
        ),
        "MIG_DECISION_CHANGE_SET_STALE" => (
            "Migration decisions refer to a different change set",
            "The decision file contains a fingerprint of the exact schema changes it approved. It cannot be reused after those changes differ.",
            "Regenerate the template and review every decision for the current change set",
        ),
        "MIG_DECISION_DUPLICATE" => (
            "Schema change has more than one migration decision",
            "Each identity and change pair must select one strategy so planning has a single reviewed instruction.",
            "Keep one decision for this schema change",
        ),
        "MIG_DECISION_EVIDENCE_DUPLICATE" => (
            "Migration decision repeats an evidence kind",
            "Each strategy-specific evidence kind has one value; duplicates make the reviewed justification ambiguous.",
            "Keep one value for each required evidence kind",
        ),
        "MIG_DECISION_EVIDENCE_EMPTY" => (
            "Migration decision contains empty evidence",
            "A required evidence field must contain a meaningful reviewed value rather than an empty or whitespace-only placeholder.",
            "Provide the reviewed evidence value",
        ),
        "MIG_DECISION_EVIDENCE_MISSING" => (
            "Migration decision is missing required evidence",
            "The selected strategy requires specific evidence before the compiler can record it as an approved migration instruction.",
            "Supply every evidence kind required by the selected strategy",
        ),
        "MIG_DECISION_EVIDENCE_UNEXPECTED" => (
            "Migration decision contains unsupported evidence",
            "The supplied evidence kind is not part of the selected strategy's review contract and cannot broaden that contract implicitly.",
            "Remove the unsupported evidence or choose the matching strategy",
        ),
        "MIG_DECISION_MISSING" => (
            "Schema change has no migration decision",
            "This change crosses an approval boundary and cannot be planned until a human selects a strategy and supplies its required evidence.",
            "Review the generated template and add the missing human-owned decision",
        ),
        "MIG_DECISION_STRATEGY_INVALID" => (
            "Migration strategy is not valid for this change",
            "Allowed strategies depend on the exact change and disposition; an unrelated strategy cannot be applied by name alone.",
            "Choose one of the strategies generated for this schema change",
        ),
        "MIG_DECISION_UNEXPECTED" => (
            "Decision file contains an unrelated schema change",
            "The identity and change pair is not among the decisions required by the bound current change set.",
            "Remove the unrelated decision or regenerate the template for the intended change set",
        ),
        "MIG_DECISION_UNRESOLVED" => (
            "Migration decision still needs a strategy",
            "A generated `null` strategy is an explicit review placeholder and cannot authorize planning or SQL generation.",
            "Have the responsible human select and evidence an allowed strategy",
        ),
        "MIG_IDENTITY_ADDITIONS_HAVE_REMOVAL" => (
            "Schema additions cannot be registered while identities are missing",
            "The live schema removed or renamed a registered identity; an additions-only command cannot decide that lifecycle change.",
            "Resolve renames or reviewed removals before registering additions",
        ),
        "MIG_IDENTITY_DUPLICATE_ID" => (
            "Schema registry repeats a stable identity",
            "One stable identifier cannot belong to multiple schema entries because diffs would no longer track a single object across renames.",
            "Restore the compiler-owned registry from a valid version or reinitialize before deployment",
        ),
        "MIG_IDENTITY_DUPLICATE_PATH" => (
            "Schema registry repeats a semantic path",
            "Each entity, field, constraint, reference, and index path must map to exactly one stable identity.",
            "Restore the compiler-owned registry or resolve the duplicate through schema commands",
        ),
        "MIG_IDENTITY_PHYSICAL_NAME" => (
            "Schema identity has no physical storage name",
            "Every persistent identity needs a stable physical name so migration SQL can address the same database object after logical renames.",
            "Restore a valid compiler-owned registry entry",
        ),
        "MIG_IDENTITY_REGISTRY_DRIFT" => (
            "Schema identities differ from checked source",
            "A persistent identity was added, removed, or renamed outside the explicit schema registry workflow, so intent cannot be inferred safely.",
            "Use the matching schema add or rename command, or restore the source declaration",
        ),
        "MIG_IDENTITY_REGISTRY_EXISTS" => (
            "Schema identity registry already exists",
            "Initialization never overwrites persistent identities because regeneration could silently assign new identities to deployed objects.",
            "Validate and use the existing registry",
        ),
        "MIG_IDENTITY_REGISTRY_INVALID" => (
            "Schema identity registry is invalid",
            "The registry JSON does not match the versioned compiler-owned schema required for stable migration identity.",
            "Restore the registry from version control or a verified backup",
        ),
        "MIG_IDENTITY_REGISTRY_MISSING" => (
            "Schema identity registry is missing",
            "This schema operation requires persistent identities, but the project has not initialized its compiler-owned registry.",
            "Run `jadpo schema init <project>` before this operation",
        ),
        "MIG_IDENTITY_REGISTRY_NOT_CANONICAL" => (
            "Schema identity registry was edited outside Jadpo",
            "The parsed entries do not serialize to the exact canonical compiler-owned representation, so manual edits cannot be trusted as lifecycle operations.",
            "Restore the canonical file and use schema commands for changes",
        ),
        "MIG_IDENTITY_REGISTRY_OWNER" => (
            "Schema identity has the wrong entity owner",
            "A field, constraint, reference, or index entry must carry the stable identity of the entity that owns its semantic path.",
            "Restore a valid registry or reapply the change through schema commands",
        ),
        "MIG_IDENTITY_REGISTRY_READ_FAILED" => (
            "Schema identity file could not be read",
            "The registry or snapshot path was unavailable, so the compiler cannot validate persistent identity before changing schema artifacts.",
            "Restore read access and retry the schema operation",
        ),
        "MIG_IDENTITY_REGISTRY_WRITE_FAILED" => (
            "Schema identity registry could not be written",
            "A checked initialization, addition, or rename could not persist its canonical identity update.",
            "Restore write access and retry the same schema operation",
        ),
        "MIG_IDENTITY_RENAME_KIND" => (
            "Schema rename kind is not supported",
            "Identity-preserving rename currently accepts only an `entity` or `field` semantic path.",
            "Choose `entity` or `field` as the rename kind",
        ),
        "MIG_IDENTITY_RENAME_SOURCE_UNKNOWN" => (
            "Schema rename source identity does not exist",
            "The old semantic path is not registered under the selected kind, so there is no stable identity to carry forward.",
            "Use the registered old path and matching kind",
        ),
        "MIG_IDENTITY_RENAME_TARGET_EXISTS" => (
            "Schema rename target already has an identity",
            "Moving the old stable identity onto an already registered target would merge two distinct persistent objects.",
            "Choose the unregistered renamed target or resolve the existing identity first",
        ),
        "MIG_IDENTITY_RENAME_TARGET_UNKNOWN" => (
            "Schema rename target is absent from checked source",
            "The new semantic path must already exist in the source before its old stable identity can be reassigned.",
            "Rename the source declaration first, then update its registry identity",
        ),
        "MIG_IDENTITY_SNAPSHOT_EXISTS" => (
            "Schema snapshot output already exists",
            "Snapshots are immutable comparison inputs and are never overwritten after review or archival.",
            "Choose a new output path or use the existing snapshot",
        ),
        "MIG_IDENTITY_SNAPSHOT_INVALID" => (
            "Schema snapshot is invalid",
            "The snapshot JSON does not match the versioned format Jadpo needs to compare stored schema definitions.",
            "Create a new snapshot with `jadpo schema snapshot`",
        ),
        "MIG_IDENTITY_SNAPSHOT_MISSING" => (
            "Previous schema snapshot is missing",
            "The requested comparison needs an immutable earlier snapshot, but the selected path does not exist.",
            "Provide the verified snapshot path",
        ),
        "MIG_IDENTITY_SNAPSHOT_NOT_CANONICAL" => (
            "Previous schema snapshot is not canonical",
            "The parsed snapshot differs from Jadpo's exact serialization, indicating it was edited or produced by an incompatible process.",
            "Recreate the snapshot using `jadpo schema snapshot`",
        ),
        "MIG_IDENTITY_SNAPSHOT_SHAPE_MISSING" => (
            "Snapshot entry is missing its saved definition",
            "Each snapshot entry must include its previous fields and storage rules so Jadpo can identify what changed and plan a review.",
            "Regenerate the snapshot from a valid identity registry",
        ),
        "MIG_IDENTITY_SNAPSHOT_WRITE_FAILED" => (
            "Schema snapshot could not be written",
            "The compiler derived a canonical immutable snapshot but could not create the selected output file.",
            "Choose a writable output path and take the snapshot again",
        ),
        "MIG_PLAN_ADAPTER_INVALID" => (
            "Migration adapter is not supported",
            "Migration planning and SQL review currently require an explicit `postgres` or `sqlite` adapter because their operations differ.",
            "Choose `postgres` or `sqlite`",
        ),
        "MIG_PLAN_DECISION_REJECTS_CHANGE" => (
            "Reviewed decision rejects this schema change",
            "The human-selected `reject` strategy intentionally denies this change, so the compiler will not turn it into a migration plan.",
            "Restore the previous schema definition or obtain a new reviewed decision for another allowed strategy",
        ),
        "MIG_PLAN_EXISTS" => (
            "Migration plan output already exists",
            "Plans are immutable review artifacts and are never overwritten because replacement could invalidate an approval trail.",
            "Choose a new output path or review the existing plan",
        ),
        "MIG_PLAN_WRITE_FAILED" => (
            "Migration plan could not be written",
            "The compiler validated the bound decisions and derived the non-executable plan but could not create its output file.",
            "Choose a writable output path and generate the plan again",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

fn migration_sql_catalogue_copy(code: &str) -> Option<AuthoredCopy> {
    let (summary, reason, next) = match code {
        "MIG_SQL_CHANGE_UNSUPPORTED" => (
            "Schema change has no SQL review generator",
            "The non-executable migration plan can describe this reviewed change, but the selected adapter cannot yet derive safe forward and rollback SQL for it.",
            "Review the migration plan and author the unsupported operation outside generated SQL",
        ),
        "MIG_SQL_EXPRESSION_UNSUPPORTED" => (
            "Backfill expression cannot be compiled to SQL",
            "Generated migration SQL currently accepts only the explicit `literal(...)` evidence form; arbitrary application expressions are not executed during migration.",
            "Use a typed `literal(...)` backfill value or keep the step manual",
        ),
        "MIG_SQL_FIELD_OWNER_MISSING" => (
            "Migration field has no resolvable owning table",
            "The field snapshot or identity registry does not link this persistent field to a registered entity table, so SQL cannot address it safely.",
            "Restore and validate the schema identity registry before regenerating SQL",
        ),
        "MIG_SQL_FIELD_SHAPE_INVALID" => (
            "Saved field definition is incomplete",
            "The snapshot does not contain the field type and nullability Jadpo needs to choose the correct SQL operation.",
            "Regenerate valid schema identities and snapshots before planning this change",
        ),
        "MIG_SQL_LITERAL_INVALID" => (
            "Backfill literal is invalid for the field type",
            "The reviewed literal cannot be parsed as the target Text, UUID, Instant, CalendarDate, Int, Decimal, or Boolean value without changing its meaning.",
            "Provide a valid typed literal for the target field",
        ),
        "MIG_SQL_PREDICATE_UNSUPPORTED" => (
            "Validation check cannot be compiled to SQL",
            "Making a nullable field required needs the exact `not_null` check. Jadpo cannot treat another check as proof that existing rows are safe.",
            "Use `not_null` evidence or keep the narrowing outside generated SQL",
        ),
        "MIG_SQL_REVIEW_EXISTS" => (
            "Migration SQL review output already exists",
            "SQL reviews are immutable, non-executable approval artifacts and are never overwritten after generation.",
            "Choose a new output path or review the existing SQL artifact",
        ),
        "MIG_SQL_REVIEW_WRITE_FAILED" => (
            "Migration SQL review could not be written",
            "The compiler derived forward and rollback statements but could not create the immutable review output.",
            "Choose a writable output path and generate the SQL review again",
        ),
        "MIG_SQL_SQLITE_CONSTRAINT_MISSING" => (
            "SQLite rebuild cannot resolve a constraint identity",
            "Recreating the table requires the stable physical name of each primary, unique, or compound-unique constraint, and one is absent from the registry.",
            "Restore and validate the identity registry before regenerating the SQLite review",
        ),
        "MIG_SQL_SQLITE_ENTITY_MISSING" => (
            "SQLite rebuild cannot find the current entity",
            "The registered table does not match a checked entity declaration with all fields needed to recreate it.",
            "Restore consistency between source and the identity registry",
        ),
        "MIG_SQL_SQLITE_FIELD_IDENTITY_MISSING" => (
            "SQLite rebuild cannot resolve a field identity",
            "Copying rows into a rebuilt table requires each retained field's stable physical column name, and one is absent from the registry.",
            "Restore and validate the field identities before regenerating SQL",
        ),
        "MIG_SQL_SQLITE_FIELD_MISSING" => (
            "SQLite rebuild cannot find the current field",
            "A changed snapshot field does not resolve to a checked field on the entity being rebuilt, so its source and destination columns cannot be proven.",
            "Restore consistency between the snapshot, source, and identity registry",
        ),
        "MIG_SQL_SQLITE_INDEX_MISSING" => (
            "SQLite rebuild cannot resolve an index identity",
            "Recreating the table requires the stable physical name of every retained index, and one is absent from the registry.",
            "Restore and validate the index identities before regenerating SQL",
        ),
        "MIG_SQL_SQLITE_REBUILD_REQUIRED" => (
            "SQLite change requires a table rebuild",
            "SQLite cannot express this field alteration as the direct `ALTER TABLE` operation used by the general path; it must be grouped into a checked rebuild.",
            "Generate this change as a supported isolated SQLite rebuild set",
        ),
        "MIG_SQL_SQLITE_REBUILD_UNSUPPORTED" => (
            "This combination of SQLite table changes is not supported",
            "The reviewed changes combine fields, ownership, modifiers, or constraints that Jadpo cannot yet reproduce safely in one table rebuild.",
            "Split the change into supported steps or author and review the rebuild manually",
        ),
        "MIG_SQL_SQLITE_REFERENCE_MISSING" => (
            "SQLite rebuild cannot resolve a reference target",
            "Recreating a foreign key requires registered physical identities for its target table and column, and that mapping is incomplete.",
            "Restore and validate relationship identities before regenerating SQL",
        ),
        "MIG_SQL_SQLITE_RENAMED_TABLE_UNSUPPORTED" => (
            "SQLite rebuild cannot target a logically renamed table",
            "The bounded rebuild generator does not combine table-history resolution with a physical table recreation in one reviewed step.",
            "Separate the rename lifecycle from the rebuild or author the reviewed SQL manually",
        ),
        "MIG_SQL_STRATEGY_UNSUPPORTED" => (
            "Reviewed migration strategy has no SQL implementation",
            "The selected strategy is valid for planning but does not match the bounded adapter operation required for this SQL review.",
            "Choose a SQL-supported strategy through a new review or keep the step manual",
        ),
        "MIG_SQL_TYPE_UNSUPPORTED" => (
            "Field type has no migration SQL mapping",
            "Jadpo does not know which database column type safely represents this named field type for the selected database.",
            "Use a supported storage representation or author and review the type migration manually",
        ),
        _ => return None,
    };
    Some(AuthoredCopy {
        summary,
        reason,
        next,
    })
}

pub fn catalogue_definition(code: &str) -> CatalogueDefinition {
    let (category, remainder) = code.split_once('_').unwrap_or(("diagnostic", code));
    let category = match category {
        "SYN" => "syntax",
        "SEM" => "semantic",
        "TYPE" => "type",
        "FAIL" => "failure",
        "EFFECT" => "effect",
        "ROUTE" => "route",
        "DATA" => "data",
        "MOD" => "module",
        "MIG" => "migration",
        "INDEX" => "index",
        "CLI" => "cli",
        "LSP" => "lsp",
        "FMT" => "format",
        "RUNTIME" => "runtime",
        "TEST" => "test",
        "JADPO" => "toolchain",
        "CONFIG" => "configuration",
        other => other,
    }
    .to_ascii_lowercase();
    let rule_id = format!("{category}.{}", remainder.to_ascii_lowercase());
    let authored = job_catalogue_copy(code)
        .or_else(|| syntax_catalogue_copy(code))
        .or_else(|| semantic_catalogue_copy(code))
        .or_else(|| configuration_catalogue_copy(code))
        .or_else(|| policy_catalogue_copy(code))
        .or_else(|| failure_catalogue_copy(code))
        .or_else(|| route_catalogue_copy(code))
        .or_else(|| data_catalogue_copy(code))
        .or_else(|| core_type_catalogue_copy(code))
        .or_else(|| match_type_catalogue_copy(code))
        .or_else(|| persistence_type_catalogue_copy(code))
        .or_else(|| module_catalogue_copy(code))
        .or_else(|| tooling_catalogue_copy(code))
        .or_else(|| index_catalogue_copy(code))
        .or_else(|| toolchain_catalogue_copy(code))
        .or_else(|| cli_catalogue_copy(code))
        .or_else(|| migration_workflow_catalogue_copy(code))
        .or_else(|| migration_sql_catalogue_copy(code));
    let human_owned = matches!(
        code,
        "JADPO_TARGET_AUTH_NOT_IMPLEMENTED"
            | "JADPO_TARGET_DURABLE_WORKFLOW_NOT_IMPLEMENTED"
            | "JADPO_TARGET_STORE_NOT_IMPLEMENTED"
            | "JADPO_TARGET_LIVENESS_NOT_LOCAL"
            | "ROUTE_AUTH_VALUE_INVALID"
            | "MIG_DECISION_UNRESOLVED"
            | "MIG_DECISION_MISSING"
            | "MIG_DECISION_EVIDENCE_MISSING"
            | "MIG_PLAN_DECISION_REJECTS_CHANGE"
    );
    let automatic = matches!(
        code,
        "FAIL_ATTEMPT_REQUIRED"
            | "FAIL_STALE_DECLARATION"
            | "ROUTE_ITEM_COLON_REQUIRED"
            | "SYN_CONSTRAINT_COLON_REQUIRED"
            | "SYN_FAILURE_ITEM_COLON_REQUIRED"
    );
    let (repair_kind, decision_owner, recommended_title) = if human_owned {
        (
            RepairKind::HumanDecision,
            DecisionOwner::Human,
            match code {
                "ROUTE_AUTH_VALUE_INVALID" => "Choose the authentication boundary for `{route}`",
                "JADPO_TARGET_LIVENESS_NOT_LOCAL" => "Choose a dependency-free public liveness response",
                "JADPO_TARGET_AUTH_NOT_IMPLEMENTED" => {
                    "Choose and implement the authentication boundary"
                }
                "JADPO_TARGET_DURABLE_WORKFLOW_NOT_IMPLEMENTED" => {
                    "Resolve the durable-workflow runtime contract"
                }
                "JADPO_TARGET_STORE_NOT_IMPLEMENTED" => {
                    "Resolve the named authority-store adapter contract"
                }
                _ => "Request the named human-owned decision",
            }
            .to_owned(),
        )
    } else if automatic {
        (
            RepairKind::AutomaticFix,
            DecisionOwner::Compiler,
            match code {
                "FAIL_ATTEMPT_REQUIRED" => "Prefix the fallible expression with `attempt`",
                "FAIL_STALE_DECLARATION" => "Remove the stale `fails` entry",
                "ROUTE_ITEM_COLON_REQUIRED" => "Insert `:` after the route item name",
                "SYN_CONSTRAINT_COLON_REQUIRED" => "Insert `:` after the constraint name",
                "SYN_FAILURE_ITEM_COLON_REQUIRED" => "Insert `:` after the failure member name",
                _ => "Apply the compiler-verified edit",
            }
            .to_owned(),
        )
    } else {
        (
            RepairKind::GuidedChoice,
            DecisionOwner::Agent,
            match code {
                "FAIL_CONTEXT_FIELD_OVERLAP" => {
                    "Choose one disclosure class for the failure context field"
                }
                "ROUTE_PATH_BINDING_MISSING" => "Add a typed path field for the route placeholder",
                "ROUTE_PATH_BINDING_EXTRA" => "Remove the typed path field that has no placeholder",
                "ROUTE_BEHAVIOUR_CONFLICT" => "Keep either `run:` or the inline `action:`",
                "ROUTE_BEHAVIOUR_REQUIRED" => "Add `run:` or an inline `action:`",
                "ROUTE_DEADLINE_INVALID" => "Use a positive `deadline:` duration in `ms`, `s`, `m`, or `h`",
                "CLI_INCIDENT_REVISION_MISMATCH" => {
                    "Use the checked sources that produced the runtime event"
                }
                "CLI_PRESENTATION_ARGUMENTS" => {
                    "Choose one supported diagnostic format and colour mode"
                }
                "RUNTIME_UNHANDLED_FAULT" => {
                    "Inspect the matching local incident using its request identifier"
                }
                "RUNTIME_OUTCOME_UNKNOWN" => {
                    "Inspect the matching uncertain operation before attempting reconciliation"
                }
                "RUNTIME_STARTUP_FAILED" => {
                    "Inspect the generated runtime startup event before retrying"
                }
                "SYN_EXPECTED_DECLARATION" => {
                    "Move `{found}` into its owning declaration, or replace it with a top-level declaration"
                }
                "SYN_UNEXPECTED_TOKEN" => "Provide {expected}",
                _ => authored
                    .map(|copy| copy.next)
                    .unwrap_or("Update the source to satisfy this rule"),
            }
            .to_owned(),
        )
    };
    let summary = match code {
        "FAIL_ATTEMPT_REQUIRED" => "This operation can fail and requires `attempt`".to_owned(),
        "FAIL_STALE_DECLARATION" => "A listed failure can never happen".to_owned(),
        "FAIL_CONTEXT_FIELD_OVERLAP" => {
            "Failure context field has conflicting disclosure".to_owned()
        }
        "ROUTE_PATH_BINDING_MISSING" => "Route placeholder has no typed binding".to_owned(),
        "ROUTE_PATH_BINDING_EXTRA" => "Typed path binding has no placeholder".to_owned(),
        "ROUTE_BEHAVIOUR_CONFLICT" => "Route declares two behaviour forms".to_owned(),
        "ROUTE_BEHAVIOUR_REQUIRED" => "Route has no behaviour".to_owned(),
        "ROUTE_ITEM_COLON_REQUIRED" => "Route item requires a `:` separator".to_owned(),
        "ROUTE_AUTH_VALUE_INVALID" => {
            "Route authentication value `{found}` is not valid".to_owned()
        }
        "CLI_INCIDENT_REVISION_MISMATCH" => {
            "Runtime event and local source revisions differ".to_owned()
        }
        "CLI_PRESENTATION_ARGUMENTS" => "Terminal presentation option is invalid".to_owned(),
        "RUNTIME_UNHANDLED_FAULT" => "Generated runtime contained an unexpected fault".to_owned(),
        "RUNTIME_OUTCOME_UNKNOWN" => {
            "Generated runtime could not confirm whether an effect committed".to_owned()
        }
        "RUNTIME_STARTUP_FAILED" => "Generated runtime failed during startup".to_owned(),
        "SYN_EXPECTED_DECLARATION" => "`{found}` cannot start a top-level declaration".to_owned(),
        "SYN_UNEXPECTED_TOKEN" => "Expected {expected}".to_owned(),
        _ => authored
            .map(|copy| copy.summary.to_owned())
            .unwrap_or_else(|| sentence_case_identifier(remainder)),
    };
    let reason = match code {
        "FAIL_ATTEMPT_REQUIRED" => "`attempt` makes it clear that a declared failure may leave the current function or action.".to_owned(),
        "FAIL_STALE_DECLARATION" => "Every failure after `fails` must be able to leave the function or action through a call or `reject`.".to_owned(),
        "FAIL_CONTEXT_FIELD_OVERLAP" => "The same field cannot be returned to callers and hidden from them at the same time.".to_owned(),
        "ROUTE_PATH_BINDING_MISSING" | "ROUTE_PATH_BINDING_EXTRA" => "Route placeholders and typed `path` fields must correspond exactly one-to-one.".to_owned(),
        "ROUTE_BEHAVIOUR_CONFLICT" | "ROUTE_BEHAVIOUR_REQUIRED" => "A route must select exactly one local inline action or one named `run:` invocation.".to_owned(),
        "ROUTE_ITEM_COLON_REQUIRED" => "Every route item uses the same explicit `name: value` separator, including block-valued path and action items.".to_owned(),
        "ROUTE_AUTH_VALUE_INVALID" => "Routes require authentication by default. The only explicit opt-out is the exact form `auth: none`, so the compiler cannot infer whether `{found}` was meant to retain authentication or disable it.".to_owned(),
        "CLI_INCIDENT_REVISION_MISMATCH" => "Local enrichment is trustworthy only when the runtime event and compiler graph identify the same checked source revision.".to_owned(),
        "CLI_PRESENTATION_ARGUMENTS" => "Diagnostic format and colour flags must select one supported presentation without changing the semantic diagnostic payload.".to_owned(),
        "RUNTIME_UNHANDLED_FAULT" => "An exception outside the declared domain-failure boundary was contained by the generated runtime.".to_owned(),
        "RUNTIME_OUTCOME_UNKNOWN" => "The generated runtime contained an operation whose external effect may have committed, so it returned a safe uncertainty response without retrying.".to_owned(),
        "RUNTIME_STARTUP_FAILED" => "The generated runtime could not establish its startup contract and did not report readiness.".to_owned(),
        "SYN_EXPECTED_DECLARATION" => "Jadpo files accept `type`, `persist`, `failure`, `function`, `action`, `test`, and `route` declarations at the top level. Optional `module` and `import` headers come before those declarations. Route items such as `path:` belong inside a `route` block, so `{found}` cannot be parsed here.".to_owned(),
        "SYN_UNEXPECTED_TOKEN" => "Found `{found}` while parsing this construct. Jadpo requires {expected} at this location, so parsing stops rather than guessing the authored structure.".to_owned(),
        _ => authored.map_or_else(
            || format!("The compiler-enforced `{code}` invariant is not satisfied at this location."),
            |copy| copy.reason.to_owned(),
        ),
    };
    let mut fixtures = CATALOGUE_FIXTURES
        .iter()
        .filter_map(|(fixture_code, path)| (*fixture_code == code).then_some(*path))
        .collect::<Vec<_>>();
    if fixtures.is_empty() {
        fixtures.push("jadpo/crates/diagnostics/src/lib.rs#every_catalogue_entry_is_renderable");
    }
    let authored_copy = authored.is_some()
        || matches!(
            code,
            "FAIL_ATTEMPT_REQUIRED"
                | "FAIL_STALE_DECLARATION"
                | "FAIL_CONTEXT_FIELD_OVERLAP"
                | "ROUTE_PATH_BINDING_MISSING"
                | "ROUTE_PATH_BINDING_EXTRA"
                | "ROUTE_BEHAVIOUR_CONFLICT"
                | "ROUTE_BEHAVIOUR_REQUIRED"
                | "ROUTE_ITEM_COLON_REQUIRED"
                | "ROUTE_AUTH_VALUE_INVALID"
                | "ROUTE_DEADLINE_INVALID"
                | "CLI_INCIDENT_REVISION_MISMATCH"
                | "CLI_PRESENTATION_ARGUMENTS"
                | "RUNTIME_UNHANDLED_FAULT"
                | "RUNTIME_OUTCOME_UNKNOWN"
                | "RUNTIME_STARTUP_FAILED"
                | "SYN_EXPECTED_DECLARATION"
                | "SYN_UNEXPECTED_TOKEN"
        );
    CatalogueDefinition {
        help_id: format!("diagnostics/{rule_id}"),
        rule_id,
        category,
        summary,
        reason,
        recommended_title,
        repair_kind,
        decision_owner,
        context_keys: match code {
            "EFFECT_FUNCTION_CALLS_ACTION" => vec!["callable", "name"],
            "EFFECT_FUNCTION_PERSISTENCE" => vec!["callable"],
            "FAIL_ATTEMPT_REQUIRED" => vec!["usage", "operation"],
            "FAIL_CONTEXT_FIELD_OVERLAP" => vec!["failure", "field"],
            "FAIL_OUTCOME_DUPLICATE_ARM"
            | "FAIL_OUTCOME_MISSING_ARM"
            | "FAIL_OUTCOME_UNKNOWN_ARM" => vec!["operation", "failure"],
            "FAIL_OUTCOME_INFALLIBLE"
            | "FAIL_OUTCOME_PATTERN_INVALID"
            | "FAIL_OUTCOME_SUCCESS_DUPLICATE"
            | "FAIL_OUTCOME_SUCCESS_MISSING"
            | "FAIL_OUTCOME_SUCCESS_VALUE_REQUIRED"
            | "FAIL_OUTCOME_SUBJECT_REQUIRED"
            | "FAIL_OUTCOME_WILDCARD" => vec!["operation"],
            "FAIL_DUPLICATE_CODE" => vec!["failure", "otherFailure", "codeValue"],
            "FAIL_DUPLICATE_CONTEXT_FIELD" | "FAIL_UNKNOWN_CONTEXT_FIELD" => {
                vec!["failure", "field"]
            }
            "FAIL_DUPLICATE_DECLARATION" | "FAIL_UNDECLARED_PROPAGATION" => {
                vec!["callable", "failure"]
            }
            "FAIL_MISSING_CONTEXT_FIELD" => vec!["failure", "field", "scope"],
            "FAIL_MUTATION_CONFLICT_NOT_CONFLICT"
            | "FAIL_PATCH_EMPTY_NOT_INVALID_VALUE"
            | "FAIL_REQUIRED_MUTATION_NOT_NOT_FOUND"
            | "FAIL_REQUIRED_QUERY_NOT_NOT_FOUND" => {
                vec!["failure", "expected", "received", "usage"]
            }
            "FAIL_STALE_DECLARATION" => {
                vec!["callable", "declared", "reachable", "failures"]
            }
            "CLI_INCIDENT_REVISION_MISMATCH" => vec!["eventRevision", "localRevision"],
            "JADPO_TARGET_AUTH_NOT_IMPLEMENTED" | "JADPO_TARGET_LIVENESS_NOT_LOCAL" => vec!["route"],
            "JADPO_TARGET_DURABLE_WORKFLOW_NOT_IMPLEMENTED" => vec!["callable"],
            "JADPO_TARGET_STORE_NOT_IMPLEMENTED" => vec!["name"],
            "JADPO_TARGET_JOB_NOT_IMPLEMENTED" => vec!["name"],
            "ROUTE_AUTH_VALUE_INVALID" => vec!["route", "found"],
            "SEM_DUPLICATE_DECLARATION" => vec!["name"],
            "SEM_NAME_CASE" => vec!["name", "expected", "suggestedName"],
            "SEM_STANDARD_NAMESPACE_RESERVED" | "SEM_STANDARD_OPERATION_RESERVED" => {
                vec!["name"]
            }
            "SEM_STANDARD_OPERATION_QUALIFICATION" => vec!["name", "suggestedName"],
            "MOD_STANDARD_NAMESPACE_IMPORT" | "MOD_STANDARD_NAMESPACE_RESERVED" => vec!["name"],
            "SEM_UNKNOWN_NAME" | "SEM_WRONG_NAME_KIND" => vec![
                "name",
                "expected",
                "actualKind",
                "examples",
                "suggestedName",
                "usage",
            ],
            "SEM_UNKNOWN_CALLEE" => vec!["name", "expected", "suggestedName", "usage"],
            "SEM_NOT_CALLABLE" => vec!["name", "expected", "actualKind"],
            "TYPE_MISMATCH" | "TYPE_SIBLING_MISMATCH" => vec!["expected", "received"],
            "TYPE_ARGUMENT_COUNT" | "TYPE_CONSTRUCTOR_ARGUMENT_COUNT" => {
                vec!["name", "expectedCount", "receivedCount"]
            }
            "TYPE_CONSTRUCTOR_INPUT" => vec!["name", "expected", "received"],
            "TYPE_REDUNDANT_NULLABILITY" => vec!["name"],
            "TYPE_MISSING_FIELD" | "TYPE_MISSING_VARIANT_FIELD" => {
                vec!["subject", "field"]
            }
            "TYPE_UNKNOWN_FIELD" | "TYPE_UNKNOWN_VARIANT_FIELD" => {
                vec!["subject", "field", "suggestedName"]
            }
            "TYPE_UNKNOWN_VALUE" => vec!["name", "suggestedName"],
            "TYPE_UNKNOWN_ENUM_VARIANT" => vec!["subject", "name", "suggestedName"],
            "TYPE_FIELD_ON_NON_RECORD" | "TYPE_NULLABLE_SELECTION" => {
                vec!["received", "field"]
            }
            "TYPE_PRIMITIVE_SIGNATURE" => vec!["received", "usage"],
            "TYPE_INVALID_LITERAL" => vec!["subject", "constraint"],
            "SYN_EXPECTED_DECLARATION" => vec!["found"],
            "SYN_CONSTRAINT_COLON_REQUIRED" => vec!["name"],
            "SYN_FAILURE_ITEM_COLON_REQUIRED" => vec!["name"],
            "SYN_UNEXPECTED_TOKEN" => vec!["expected", "found"],
            _ => Vec::new(),
        },
        fixtures,
        authored_copy,
    }
}

fn render_catalogue_text(template: &str, context: &[(String, String)]) -> String {
    context
        .iter()
        .fold(template.to_owned(), |rendered, (key, value)| {
            rendered.replace(&format!("{{{key}}}"), value)
        })
}

fn semantic_expected_reason(
    expected: &str,
    examples: Option<&str>,
    suggestion: Option<&str>,
    usage: Option<&str>,
) -> String {
    let mut reason = match expected {
        "predefined category" => examples.map_or_else(
            || "`kind` must be a predefined category.".to_owned(),
            |values| format!("`kind` must be a predefined category, such as {values}."),
        ),
        other => usage.map_or_else(
            || format!("Jadpo expected a {other} here."),
            |usage| format!("This {usage} needs a {other}."),
        ),
    };
    if let Some(suggestion) = suggestion {
        reason.push_str(&format!(" Did you mean `{suggestion}`?"));
    }
    reason
}

fn semantic_unknown_name_guidance(name: &str, expected: &str, suggestion: Option<&str>) -> String {
    match expected {
        "type" => suggestion.map_or_else(
            || format!("Define `{name}` as a type or import the type"),
            |suggested| format!("Use `{suggested}`, define `{name}` as a type, or import the type"),
        ),
        "declared failure" => suggestion.map_or_else(
            || format!("Declare `{name}` as a failure or import the failure"),
            |suggested| format!("Use `{suggested}`, declare `{name}` as a failure, or import it"),
        ),
        "field" => suggestion.map_or_else(
            || format!("Add `{name}: Type` to the object, or use an existing field name"),
            |suggested| format!("Use the existing `{suggested}` field, or add `{name}: Type` to the object"),
        ),
        "object type" => suggestion.map_or_else(
            || format!("Define `{name}` as `type {name} = Object {{ ... }}`, or use an existing object type"),
            |suggested| format!("Use `{suggested}`, or define `{name}` as `type {name} = Object {{ ... }}`"),
        ),
        "predefined category" => "Use one of Jadpo's predefined categories".to_owned(),
        _ => format!("Define `{name}` as a {expected} or import it"),
    }
}

fn semantic_replacement_guidance(expected: &str) -> String {
    match expected {
        "predefined category" => "Use one of Jadpo's predefined categories".to_owned(),
        "type" => "Use a type here".to_owned(),
        "declared failure" => "Use a declared failure here".to_owned(),
        "function or action" => "Use a function or action here".to_owned(),
        other => format!("Use a {other} here"),
    }
}

fn plural<'a>(singular: &'a str, count: &str) -> &'a str {
    if count == "1" {
        singular
    } else {
        match singular {
            "argument" => "arguments",
            other => other,
        }
    }
}

fn sentence_case_identifier(value: &str) -> String {
    let mut words = value
        .split('_')
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    if let Some(first) = words.first_mut() {
        if let Some(initial) = first.get_mut(0..1) {
            initial.make_ascii_uppercase();
        }
    }
    words.join(" ")
}

fn default_alternatives(code: &str, _definition: &CatalogueDefinition) -> Vec<RepairStep> {
    if code == "ROUTE_AUTH_VALUE_INVALID" {
        return vec![
            RepairStep {
                kind: RepairKind::HumanDecision,
                title: "Keep authentication required for this route".to_owned(),
                reason: "Remove the invalid `auth:` item so the route keeps Jadpo's authenticated default.".to_owned(),
                decision_owner: DecisionOwner::Human,
                preferred: false,
                edits: Vec::new(),
                behavioral_effect: "The route continues to require authentication.".to_owned(),
                public_contract_effect: "The public route remains authenticated. Target generation may still be blocked until authentication runtime support is configured.".to_owned(),
            },
            RepairStep {
                kind: RepairKind::HumanDecision,
                title: "Make this route explicitly unauthenticated".to_owned(),
                reason: "Replace the invalid value with the exact opt-out `none` only when unauthenticated access is intentional.".to_owned(),
                decision_owner: DecisionOwner::Human,
                preferred: false,
                edits: Vec::new(),
                behavioral_effect: "Requests may reach the route without an authenticated identity.".to_owned(),
                public_contract_effect: "The public security contract changes: this route becomes callable without authentication.".to_owned(),
            },
        ];
    }
    if code == "SYN_EXPECTED_DECLARATION" {
        return vec![RepairStep {
            kind: RepairKind::GuidedChoice,
            title: "Remove the stray top-level construct".to_owned(),
            reason: "Delete it only when it is leftover text rather than content intended for a declaration or route.".to_owned(),
            decision_owner: DecisionOwner::Agent,
            preferred: false,
            edits: Vec::new(),
            behavioral_effect: "The stray construct no longer contributes authored behavior.".to_owned(),
            public_contract_effect: "Review whether the removed construct was intended to change a route or another public declaration.".to_owned(),
        }];
    }
    let (title, reason) = match code {
        "FAIL_STALE_DECLARATION" => (
            "Add the missing reachable behavior intentionally",
            "Keep the declared problem only by adding a reviewed path that can actually produce it.",
        ),
        "ROUTE_PATH_BINDING_MISSING" => (
            "Remove or rename the route placeholder",
            "Change the public path template when that segment is not intended to be a typed input.",
        ),
        "ROUTE_PATH_BINDING_EXTRA" => (
            "Add the matching route placeholder",
            "Change the public path template when the typed field is intended to be externally supplied.",
        ),
        _ => return Vec::new(),
    };
    vec![RepairStep {
        kind: RepairKind::GuidedChoice,
        title: title.to_owned(),
        reason: reason.to_owned(),
        decision_owner: DecisionOwner::Agent,
        preferred: false,
        edits: Vec::new(),
        behavioral_effect: "This alternative changes authored behavior and requires validation."
            .to_owned(),
        public_contract_effect: if code.starts_with("ROUTE_") {
            "This alternative changes the public route contract.".to_owned()
        } else {
            "Review generated contract impact before applying this alternative.".to_owned()
        },
    }]
}

pub fn catalogue_manifest_json() -> String {
    let entries = CATALOGUE_CODES
        .iter()
        .map(|code| {
            let entry = catalogue_definition(code);
            let allowed_keys = entry
                .context_keys
                .iter()
                .map(|key| json_string(key))
                .collect::<Vec<_>>()
                .join(",");
            let fixtures = entry
                .fixtures
                .iter()
                .map(|fixture| json_string(fixture))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"ruleId\":{},\"category\":{},\"summary\":{},\"reason\":{},\"recommendedNextStep\":{},\"repairKind\":{},\"decisionOwner\":{},\"copyStatus\":{},\"contextSchema\":{{\"allowedKeys\":[{allowed_keys}],\"additionalProperties\":false}},\"helpId\":{},\"fixtures\":[{fixtures}],\"legacyAliases\":[{}]}}",
                json_string(&entry.rule_id),
                json_string(&entry.category),
                json_string(&entry.summary),
                json_string(&entry.reason),
                json_string(&entry.recommended_title),
                json_string(entry.repair_kind.as_str()),
                json_string(entry.decision_owner.as_str()),
                json_string(if entry.authored_copy { "authored" } else { "placeholder" }),
                json_string(&entry.help_id),
                json_string(code),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"schemaVersion\":1,\"diagnostics\":[{entries}]}}")
}

pub fn catalogue_reference_markdown() -> String {
    let mut output = String::from(
        "# Compiler diagnostic catalogue\n\nGenerated from the compiler-owned catalogue.\n\n",
    );
    for code in CATALOGUE_CODES {
        let entry = catalogue_definition(code);
        output.push_str(&format!(
            "## `{}`\n\n{}\n\n{}\n\nRecommended next step: {} (`{}`, owner `{}`).\n\nHelp ID: `{}`. Legacy alias: `{}`. Fixtures: {}.\n\n",
            entry.rule_id,
            entry.summary,
            entry.reason,
            entry.recommended_title,
            entry.repair_kind.as_str(),
            entry.decision_owner.as_str(),
            entry.help_id,
            code,
            if entry.fixtures.is_empty() { "none".to_owned() } else { entry.fixtures.join(", ") }
        ));
    }
    output
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublicFailureValue {
    Text(String),
    Integer(i64),
    Boolean(bool),
    None,
}

impl PublicFailureValue {
    fn to_json(&self) -> String {
        match self {
            Self::Text(value) => json_string(value),
            Self::Integer(value) => value.to_string(),
            Self::Boolean(value) => value.to_string(),
            Self::None => "null".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafeIdentifier(String);

impl SafeIdentifier {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.is_empty()
            && value.len() <= 256
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(byte, b'_' | b'-' | b':' | b'/' | b'{' | b'}' | b'.')
            }))
        .then_some(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationalValue {
    StaticText(&'static str),
    Identifier(SafeIdentifier),
    Integer(i64),
    Boolean(bool),
    None,
}

impl OperationalValue {
    fn to_json(&self) -> String {
        match self {
            Self::StaticText(value) => json_string(value),
            Self::Identifier(value) => json_string(value.as_str()),
            Self::Integer(value) => value.to_string(),
            Self::Boolean(value) => value.to_string(),
            Self::None => "null".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentContextValue {
    CompilerText(String),
    Identifier(SafeIdentifier),
    Integer(i64),
    Boolean(bool),
    None,
}

pub struct Secret<T>(T);

impl<T> Secret<T> {
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    pub fn expose_to_boundary(&self) -> &T {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicFailureResponse {
    pub code: String,
    pub message: String,
    pub request_id: String,
    pub details: Vec<(String, PublicFailureValue)>,
}

impl PublicFailureResponse {
    pub fn to_json(&self) -> String {
        let details = self
            .details
            .iter()
            .map(|(key, value)| format!("{}:{}", json_string(key), value.to_json()))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"error\":{{\"code\":{},\"message\":{},\"request_id\":{},\"details\":{{{details}}}}}}}",
            json_string(&self.code),
            json_string(&self.message),
            json_string(&self.request_id),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationalLogEvent {
    pub event_name: String,
    pub classification: String,
    pub request_id: String,
    pub trace_id: Option<String>,
    pub semantic_operation_id: String,
    pub source_revision: String,
    pub safe_attributes: Vec<(String, OperationalValue)>,
}

impl OperationalLogEvent {
    pub fn to_json(&self) -> String {
        let attributes = self
            .safe_attributes
            .iter()
            .take(16)
            .map(|(key, value)| format!("{}:{}", json_string(key), value.to_json()))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"schemaVersion\":1,\"kind\":\"operational_log_event\",\"eventName\":{},\"classification\":{},\"requestId\":{},\"traceId\":{},\"semanticOperationId\":{},\"sourceRevision\":{},\"attributes\":{{{attributes}}}}}",
            json_string(&self.event_name),
            json_string(&self.classification),
            json_string(&self.request_id),
            self.trace_id.as_deref().map(json_string).unwrap_or_else(|| "null".to_owned()),
            json_string(&self.semantic_operation_id),
            json_string(&self.source_revision),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentIncidentPacket {
    pub source_revision: String,
    pub semantic_operation_id: String,
    pub occurrence_summary: String,
    pub source: SourceSpan,
    pub rule_id: String,
    pub context: Vec<(String, AgentContextValue)>,
    pub affected: Vec<String>,
    pub recommended_next_step: RepairStep,
    pub alternatives: Vec<RepairStep>,
}

pub fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character <= '\u{001f}' => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

impl fmt::Display for CompilerDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "{}[{}]: {}",
            self.severity, self.code, self.message
        )?;

        if let Some(primary) = &self.primary {
            writeln!(
                formatter,
                "  at {}:{}..{}",
                primary.source, primary.start, primary.end
            )?;
        }

        for note in &self.notes {
            writeln!(formatter, "  note: {note}")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        catalogue_definition, catalogue_manifest_json, diagnostic_help_url, json_string,
        DecisionOwner, Diagnostic, DiagnosticFact, OperationalLogEvent, OperationalValue,
        PublicFailureResponse, PublicFailureValue, RepairKind, SafeIdentifier, Secret, SourceSpan,
        CATALOGUE_CODES,
    };
    use serde_json::Value;
    use std::collections::BTreeSet;

    #[test]
    fn emits_stable_json_with_explicit_nullable_location_fields() {
        let mut diagnostic = Diagnostic::error("TEST_CODE").with_note("fix\\path");
        diagnostic.primary = Some(SourceSpan {
            source: "example.jadpo".to_owned(),
            start: 4,
            end: 9,
        });
        diagnostic.related.push(SourceSpan {
            source: "other.jadpo".to_owned(),
            start: 1,
            end: 3,
        });

        let json = diagnostic.to_json();
        assert!(json.contains("\"schemaVersion\":2"));
        assert!(json.contains("\"ruleId\":\"test.code\""));
        assert!(json.contains("\"summary\":\"Code\""));
        assert!(json.contains("\"location\":{\"source\":\"example.jadpo\""));
        assert!(json.contains("\"related\":[{\"source\":\"other.jadpo\""));
        assert!(Diagnostic::warning("TEST_WARNING")
            .to_json()
            .contains("\"location\":null"));
    }

    #[test]
    fn audience_schemas_do_not_share_secret_or_internal_fields() {
        let secret = Secret::new("credential-canary");
        assert_eq!(secret.expose_to_boundary(), &"credential-canary");

        let public = PublicFailureResponse {
            code: "not_found".to_owned(),
            message: "Not found.".to_owned(),
            request_id: "req_1".to_owned(),
            details: vec![(
                "resource".to_owned(),
                PublicFailureValue::Text("public-id".to_owned()),
            )],
        };
        let event = OperationalLogEvent {
            event_name: "operation.failed".to_owned(),
            classification: "unavailable".to_owned(),
            request_id: "req_1".to_owned(),
            trace_id: Some("trace_1".to_owned()),
            semantic_operation_id: "route:items:get".to_owned(),
            source_revision: "src_1".to_owned(),
            safe_attributes: vec![("attempt".to_owned(), OperationalValue::Integer(1))],
        };
        assert!(SafeIdentifier::new("request\ncredential-canary").is_none());
        let public_json = public.to_json();
        assert!(public_json.contains("\"error\":{\"code\":\"not_found\""));
        assert!(public_json.contains("\"request_id\":\"req_1\""));
        for json in [public_json, event.to_json()] {
            assert!(!json.contains("credential-canary"));
            assert!(!json.contains("stack"));
            assert!(!json.contains("provider"));
            assert!(!json.contains("sql"));
        }
    }

    #[test]
    fn catalogue_distinguishes_automatic_guided_and_human_repairs() {
        let automatic = catalogue_definition("FAIL_ATTEMPT_REQUIRED");
        assert_eq!(automatic.repair_kind, RepairKind::AutomaticFix);
        assert_eq!(automatic.decision_owner, DecisionOwner::Compiler);

        let guided = catalogue_definition("ROUTE_PATH_BINDING_MISSING");
        assert_eq!(guided.repair_kind, RepairKind::GuidedChoice);
        assert_eq!(guided.decision_owner, DecisionOwner::Agent);

        let human = catalogue_definition("JADPO_TARGET_AUTH_NOT_IMPLEMENTED");
        assert_eq!(human.repair_kind, RepairKind::HumanDecision);
        assert_eq!(human.decision_owner, DecisionOwner::Human);

        let diagnostic = Diagnostic::error("ROUTE_PATH_BINDING_MISSING");
        assert_eq!(diagnostic.alternatives.len(), 1);
        assert!(diagnostic.to_json().contains("\"alternatives\":[{"));
    }

    #[test]
    fn semantic_name_errors_explain_the_authored_source_in_plain_language() {
        let diagnostic = Diagnostic::error("SEM_UNKNOWN_NAME")
            .with_fact(DiagnosticFact::Name("CustomerID".to_owned()))
            .with_fact(DiagnosticFact::Expected("type".to_owned()))
            .with_fact(DiagnosticFact::SuggestedName("CustomerId".to_owned()))
            .with_fact(DiagnosticFact::Usage("field".to_owned()));

        assert_eq!(diagnostic.message, "`CustomerID` isn't defined");
        assert_eq!(
            diagnostic.reason,
            "This field needs a type. Did you mean `CustomerId`?"
        );
        assert_eq!(
            diagnostic.recommended_next_step.title,
            "Use `CustomerId`, define `CustomerID` as a type, or import the type"
        );
        assert_eq!(
            diagnostic_help_url(&diagnostic.help_id),
            "https://jadpo.dev/docs/diagnostics/semantic.unknown_name"
        );
    }

    #[test]
    fn type_errors_name_the_received_and_required_types() {
        let diagnostic = Diagnostic::error("TYPE_SIBLING_MISMATCH")
            .with_fact(DiagnosticFact::Expected("CustomerId".to_owned()))
            .with_fact(DiagnosticFact::Received("OrderId".to_owned()));

        assert_eq!(
            diagnostic.message,
            "`OrderId` can't be used where `CustomerId` is required"
        );
        assert_eq!(
            diagnostic.reason,
            "`OrderId` and `CustomerId` are separate named types, even if they build on the same base type."
        );
        assert_eq!(
            diagnostic.recommended_next_step.title,
            "Use a `CustomerId` value here"
        );
        assert_eq!(
            diagnostic.context,
            vec![
                ("expected".to_owned(), "CustomerId".to_owned()),
                ("received".to_owned(), "OrderId".to_owned())
            ]
        );
    }

    #[test]
    fn failure_errors_name_the_function_and_escaping_failure() {
        let diagnostic = Diagnostic::error("FAIL_UNDECLARED_PROPAGATION")
            .with_fact(DiagnosticFact::Callable("register".to_owned()))
            .with_fact(DiagnosticFact::Failure("RegistrationClosed".to_owned()));

        assert_eq!(
            diagnostic.message,
            "`RegistrationClosed` can leave `register` but isn't listed after `fails`"
        );
        assert_eq!(
            diagnostic.reason,
            "Callers of `register` need to know that `RegistrationClosed` can happen."
        );
        assert_eq!(
            diagnostic.recommended_next_step.title,
            "Add `RegistrationClosed` after `fails`, or handle it inside `register`"
        );
    }

    #[test]
    fn generated_catalogue_contains_every_representative_compiler_family() {
        let manifest = catalogue_manifest_json();
        for alias in [
            "SYN_UNEXPECTED_TOKEN",
            "TYPE_UNKNOWN_VALUE",
            "FAIL_ATTEMPT_REQUIRED",
            "ROUTE_PATH_BINDING_MISSING",
            "CLI_INCIDENT_INVALID",
            "RUNTIME_UNHANDLED_FAULT",
        ] {
            assert!(manifest.contains(alias), "missing {alias}");
        }
        assert!(manifest.contains("\"schemaVersion\":1"));
        assert!(manifest.contains("\"ruleId\":"));
        assert!(manifest.contains("\"summary\":"));
        assert!(
            manifest.contains("tests/compile/fail/60_fallible_call_requires_attempt.expect.json")
        );
        assert!(manifest.contains("\"legacyAliases\":["));
    }

    #[test]
    fn every_catalogue_entry_is_renderable() {
        for code in CATALOGUE_CODES {
            let definition = catalogue_definition(code);
            assert!(!definition.summary.is_empty(), "{code}");
            assert!(!definition.reason.is_empty(), "{code}");
            assert!(!definition.recommended_title.is_empty(), "{code}");
            assert!(!definition.fixtures.is_empty(), "{code}");

            let json = Diagnostic::error(code).to_json();
            assert!(json.contains("\"schemaVersion\":2"), "{code}");
            assert!(
                json.contains(&format!("\"ruleId\":{}", json_string(&definition.rule_id))),
                "{code}"
            );
            assert!(json.contains("\"recommendedNextStep\":"), "{code}");
        }
    }

    #[test]
    fn warnings_do_not_claim_that_they_block_compilation() {
        let warning = Diagnostic::warning("INDEX_RECOMMENDATION_AVAILABLE");

        assert_eq!(
            warning.impact.behavioral,
            "The project can still build; this recommendation is advisory."
        );
        assert!(!warning.impact.behavioral.contains("blocked"));
    }

    #[test]
    fn every_agent_diagnostic_obeys_the_complete_v2_schema() {
        let expected_top_level = BTreeSet::from([
            "alternatives",
            "context",
            "decisionOwner",
            "diagnosticId",
            "helpId",
            "impact",
            "legacyAliases",
            "location",
            "reason",
            "recommendedNextStep",
            "ruleId",
            "schemaVersion",
            "severity",
            "sourceRevision",
            "summary",
        ]);
        let expected_repair = BTreeSet::from([
            "decisionOwner",
            "edits",
            "kind",
            "preferred",
            "preview",
            "reason",
            "title",
        ]);
        let expected_preview = BTreeSet::from(["behavioral", "publicContract"]);
        let expected_impact =
            BTreeSet::from(["affected", "behavioral", "publicContract", "queryId"]);

        for code in CATALOGUE_CODES {
            let mut diagnostic = Diagnostic::error(code).with_source_revision("src_conformance");
            diagnostic.primary = Some(SourceSpan {
                source: "conformance.jadpo".to_owned(),
                start: 2,
                end: 5,
            });
            let value: Value = serde_json::from_str(&diagnostic.to_json())
                .unwrap_or_else(|error| panic!("{code}: invalid JSON: {error}"));
            assert_eq!(value["schemaVersion"], 2, "{code}");
            assert_eq!(object_keys(&value), expected_top_level, "{code}");
            assert_nonempty_string(&value, "summary", code);
            assert_nonempty_string(&value, "reason", code);
            assert_nonempty_string(&value, "ruleId", code);
            assert_nonempty_string(&value, "helpId", code);
            assert_eq!(value["sourceRevision"], "src_conformance", "{code}");
            assert_eq!(value["legacyAliases"], serde_json::json!([code]), "{code}");
            assert_eq!(value["location"]["range"]["start"], 2, "{code}");
            assert_eq!(value["location"]["range"]["end"], 5, "{code}");
            assert!(value["location"]["related"].as_array().is_some(), "{code}");
            assert!(
                value["context"]
                    .as_object()
                    .is_some_and(|items| items.len() <= 16),
                "{code}"
            );
            assert!(
                value["alternatives"]
                    .as_array()
                    .is_some_and(|items| items.len() <= 16),
                "{code}"
            );
            assert_eq!(object_keys(&value["impact"]), expected_impact, "{code}");
            assert!(
                value["impact"]["affected"]
                    .as_array()
                    .is_some_and(|items| items.len() <= 16),
                "{code}"
            );

            let mut repairs = vec![&value["recommendedNextStep"]];
            repairs.extend(value["alternatives"].as_array().unwrap().iter());
            for repair in repairs {
                assert_eq!(object_keys(repair), expected_repair, "{code}");
                assert_eq!(object_keys(&repair["preview"]), expected_preview, "{code}");
                assert_nonempty_string(repair, "title", code);
                assert_nonempty_string(repair, "reason", code);
                assert!(repair["edits"].as_array().is_some(), "{code}");
                assert!(repair["preferred"].is_boolean(), "{code}");
            }

            let packet = diagnostic.to_json();
            for forbidden in [
                "\u{1b}[",
                "credential-canary",
                "provider-canary",
                "stack-canary",
                "sql-canary",
            ] {
                assert!(!packet.contains(forbidden), "{code}: leaked {forbidden}");
            }
        }
    }

    #[test]
    fn every_public_diagnostic_has_authored_human_readable_copy() {
        let mut placeholders = Vec::new();
        for code in CATALOGUE_CODES {
            let definition = catalogue_definition(code);
            if definition.authored_copy {
                for forbidden in [
                    "compiler-enforced",
                    "Update the source to satisfy this rule",
                    "Unexpected token",
                ] {
                    assert!(
                        !format!(
                            "{} {} {}",
                            definition.summary, definition.reason, definition.recommended_title
                        )
                        .contains(forbidden),
                        "{code}: authored copy contains forbidden fallback `{forbidden}`"
                    );
                }
            } else {
                placeholders.push(*code);
                assert!(
                    definition.reason.contains("compiler-enforced")
                        || definition.recommended_title == "Update the source to satisfy this rule",
                    "{code}: unclassified catalogue copy"
                );
            }
        }
        assert!(
            placeholders.is_empty(),
            "placeholder catalogue copy remains: {}",
            placeholders.join(", ")
        );
    }

    #[test]
    fn every_public_diagnostic_uses_source_level_plain_language() {
        let forbidden = [
            "This position",
            "grammar position",
            "semantic declaration kind",
            "wrong failure kind",
            "recoverable problem set",
            "non-callable declaration",
            "semantic representation",
            "operand shapes",
            "wrong shape",
            "Declared problem",
            "Reachable problem",
            "mathematical set",
            "callee",
            "nominal siblings",
            "nominal type",
            "semantic type domain",
        ];

        for code in CATALOGUE_CODES {
            let definition = catalogue_definition(code);
            let copy = format!(
                "{} {} {}",
                definition.summary, definition.reason, definition.recommended_title
            );
            for phrase in forbidden {
                assert!(
                    !copy.contains(phrase),
                    "{code}: public copy contains compiler-oriented phrase `{phrase}`"
                );
            }
        }
    }

    #[test]
    fn every_syntax_diagnostic_has_rule_specific_public_copy() {
        for code in CATALOGUE_CODES
            .iter()
            .filter(|code| code.starts_with("SYN_"))
        {
            let definition = catalogue_definition(code);
            assert!(definition.authored_copy, "{code}");
            assert!(!definition.summary.contains("invariant"), "{code}");
            assert!(!definition.reason.contains(code), "{code}");
            assert_ne!(
                definition.recommended_title, "Update the source to satisfy this rule",
                "{code}"
            );
        }
    }

    #[test]
    fn every_semantic_name_diagnostic_has_rule_specific_public_copy() {
        for code in CATALOGUE_CODES
            .iter()
            .filter(|code| code.starts_with("SEM_"))
        {
            let definition = catalogue_definition(code);
            assert!(definition.authored_copy, "{code}");
            assert!(!definition.reason.contains(code), "{code}");
            assert_ne!(
                definition.recommended_title, "Update the source to satisfy this rule",
                "{code}"
            );
        }
    }

    #[test]
    fn every_effect_and_failure_diagnostic_has_rule_specific_public_copy() {
        for code in CATALOGUE_CODES
            .iter()
            .filter(|code| code.starts_with("EFFECT_") || code.starts_with("FAIL_"))
        {
            let definition = catalogue_definition(code);
            assert!(definition.authored_copy, "{code}");
            assert!(!definition.reason.contains(code), "{code}");
            assert_ne!(
                definition.recommended_title, "Update the source to satisfy this rule",
                "{code}"
            );
        }
    }

    #[test]
    fn every_route_diagnostic_has_rule_specific_public_copy() {
        for code in CATALOGUE_CODES
            .iter()
            .filter(|code| code.starts_with("ROUTE_"))
        {
            let definition = catalogue_definition(code);
            assert!(definition.authored_copy, "{code}");
            assert!(!definition.reason.contains(code), "{code}");
            assert_ne!(
                definition.recommended_title, "Update the source to satisfy this rule",
                "{code}"
            );
        }
    }

    #[test]
    fn every_data_model_diagnostic_has_rule_specific_public_copy() {
        for code in CATALOGUE_CODES
            .iter()
            .filter(|code| code.starts_with("DATA_"))
        {
            let definition = catalogue_definition(code);
            assert!(definition.authored_copy, "{code}");
            assert!(!definition.reason.contains(code), "{code}");
            assert_ne!(
                definition.recommended_title, "Update the source to satisfy this rule",
                "{code}"
            );
        }
    }

    #[test]
    fn every_core_type_diagnostic_has_rule_specific_public_copy() {
        const CORE_TYPE_CODES: &[&str] = &[
            "TYPE_ARGUMENT_COUNT",
            "TYPE_ARITHMETIC_OPERAND",
            "TYPE_ASSIGN_IMMUTABLE",
            "TYPE_ASSIGN_UNKNOWN",
            "TYPE_CONSTRUCTOR_ARGUMENT_COUNT",
            "TYPE_CONSTRUCTOR_INPUT",
            "TYPE_DUPLICATE_VARIANT_FIELD",
            "TYPE_FIELD_ON_NON_RECORD",
            "TYPE_INCOMPARABLE",
            "TYPE_INVALID_LITERAL",
            "TYPE_LOGICAL_OPERAND",
            "TYPE_MISMATCH",
            "TYPE_MISSING_FIELD",
            "TYPE_MISSING_VARIANT_FIELD",
            "TYPE_NAMED_ARGUMENT_UNSUPPORTED",
            "TYPE_NOT_RECORD",
            "TYPE_NULLABLE_SELECTION",
            "TYPE_ORDERING_OPERAND",
            "TYPE_PRIMITIVE_SIGNATURE",
            "TYPE_SIBLING_MISMATCH",
            "TYPE_UNARY_OPERAND",
            "TYPE_UNKNOWN_ENUM_VARIANT",
            "TYPE_UNKNOWN_FIELD",
            "TYPE_UNKNOWN_VALUE",
            "TYPE_UNKNOWN_VARIANT_FIELD",
        ];

        for code in CORE_TYPE_CODES {
            assert!(CATALOGUE_CODES.contains(code), "{code}");
            let definition = catalogue_definition(code);
            assert!(definition.authored_copy, "{code}");
            assert!(!definition.reason.contains(code), "{code}");
            assert_ne!(
                definition.recommended_title, "Update the source to satisfy this rule",
                "{code}"
            );
        }
    }

    #[test]
    fn every_match_type_diagnostic_has_rule_specific_public_copy() {
        const MATCH_TYPE_CODES: &[&str] = &[
            "TYPE_MATCH_DUPLICATE_BINDING",
            "TYPE_MATCH_DUPLICATE_PATTERN",
            "TYPE_MATCH_NON_EXHAUSTIVE",
            "TYPE_MATCH_PATTERN_TYPE",
            "TYPE_MATCH_SOME_NON_OPTIONAL",
            "TYPE_MATCH_UNKNOWN_BINDING",
            "TYPE_MATCH_UNKNOWN_VARIANT",
            "TYPE_MATCH_UNREACHABLE_PATTERN",
            "TYPE_MATCH_VARIANT_BINDINGS_REQUIRED",
            "TYPE_MATCH_WILDCARD_REQUIRED",
        ];

        for code in MATCH_TYPE_CODES {
            assert!(CATALOGUE_CODES.contains(code), "{code}");
            let definition = catalogue_definition(code);
            assert!(definition.authored_copy, "{code}");
            assert!(!definition.reason.contains(code), "{code}");
            assert_ne!(
                definition.recommended_title, "Update the source to satisfy this rule",
                "{code}"
            );
        }
    }

    #[test]
    fn every_type_diagnostic_has_rule_specific_public_copy() {
        for code in CATALOGUE_CODES
            .iter()
            .filter(|code| code.starts_with("TYPE_"))
        {
            let definition = catalogue_definition(code);
            assert!(definition.authored_copy, "{code}");
            assert!(!definition.reason.contains(code), "{code}");
            assert_ne!(
                definition.recommended_title, "Update the source to satisfy this rule",
                "{code}"
            );
        }
    }

    #[test]
    fn every_module_format_and_lsp_diagnostic_has_rule_specific_public_copy() {
        for code in CATALOGUE_CODES.iter().filter(|code| {
            code.starts_with("MOD_") || code.starts_with("FMT_") || code.starts_with("LSP_")
        }) {
            let definition = catalogue_definition(code);
            assert!(definition.authored_copy, "{code}");
            assert!(!definition.reason.contains(code), "{code}");
            assert_ne!(
                definition.recommended_title, "Update the source to satisfy this rule",
                "{code}"
            );
        }
    }

    #[test]
    fn every_index_and_toolchain_diagnostic_has_rule_specific_public_copy() {
        for code in CATALOGUE_CODES
            .iter()
            .filter(|code| code.starts_with("INDEX_") || code.starts_with("JADPO_"))
        {
            let definition = catalogue_definition(code);
            assert!(definition.authored_copy, "{code}");
            assert!(!definition.reason.contains(code), "{code}");
            assert_ne!(
                definition.recommended_title, "Update the source to satisfy this rule",
                "{code}"
            );
        }
    }

    #[test]
    fn every_cli_diagnostic_has_rule_specific_public_copy() {
        for code in CATALOGUE_CODES
            .iter()
            .filter(|code| code.starts_with("CLI_"))
        {
            let definition = catalogue_definition(code);
            assert!(definition.authored_copy, "{code}");
            assert!(!definition.reason.contains(code), "{code}");
            assert_ne!(
                definition.recommended_title, "Update the source to satisfy this rule",
                "{code}"
            );
        }
    }

    #[test]
    fn every_migration_identity_decision_and_plan_diagnostic_has_rule_specific_public_copy() {
        for code in CATALOGUE_CODES.iter().filter(|code| {
            code.strip_prefix("MIG_")
                .is_some_and(|remainder| !remainder.starts_with("SQL_"))
        }) {
            let definition = catalogue_definition(code);
            assert!(definition.authored_copy, "{code}");
            assert!(!definition.reason.contains(code), "{code}");
            assert_ne!(
                definition.recommended_title, "Update the source to satisfy this rule",
                "{code}"
            );
        }
    }

    #[test]
    fn every_migration_diagnostic_has_rule_specific_public_copy() {
        for code in CATALOGUE_CODES
            .iter()
            .filter(|code| code.starts_with("MIG_"))
        {
            let definition = catalogue_definition(code);
            assert!(definition.authored_copy, "{code}");
            assert!(!definition.reason.contains(code), "{code}");
            assert_ne!(
                definition.recommended_title, "Update the source to satisfy this rule",
                "{code}"
            );
        }
    }

    #[test]
    fn every_public_diagnostic_is_authored_and_has_conformance_evidence() {
        let failures = CATALOGUE_CODES
            .iter()
            .filter_map(|code| {
                let definition = catalogue_definition(code);
                let has_conformance_evidence = definition.fixtures.iter().any(|fixture| {
                    *fixture
                        != "jadpo/crates/diagnostics/src/lib.rs#every_catalogue_entry_is_renderable"
                });
                (!definition.authored_copy || !has_conformance_evidence).then_some(format!(
                    "{code}: copy={}, evidence={}",
                    if definition.authored_copy {
                        "authored"
                    } else {
                        "placeholder"
                    },
                    if has_conformance_evidence {
                        "present"
                    } else {
                        "missing"
                    }
                ))
            })
            .collect::<Vec<_>>();
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    fn object_keys(value: &Value) -> BTreeSet<&str> {
        value
            .as_object()
            .expect("expected object")
            .keys()
            .map(String::as_str)
            .collect()
    }

    fn assert_nonempty_string(value: &Value, key: &str, code: &str) {
        assert!(
            value[key]
                .as_str()
                .is_some_and(|text| !text.trim().is_empty()),
            "{code}: `{key}` must be a non-empty string"
        );
    }
}
