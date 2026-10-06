//! A rejected requested change is not a checked after state or a release grant.
use super::{
    canonical_bytes, checked, contract_tokens, digest, normalized_source, project_root, snapshot,
};
use crate::AnalyzedProject;
use jadpo_diagnostics::{Diagnostic, Severity};
use jadpo_syntax::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

const CANONICALIZATION: &str = "jadpo.protected-change-attempt.v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    entity: String,
    field: String,
    callable: String,
    row: ParameterAlias,
    from: Alias,
    to: ParameterAlias,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Alias {
    nominal_type: String,
    alias: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParameterAlias {
    parameter: String,
    nominal_type: String,
    alias: String,
}

/// Derives only an exact, single ordinary update of an unchanged immutable
/// direct-role field. All other source tokens must equal the checked baseline.
/// Alias strings describe supplied intent; no principal or persisted row is read.
pub fn derive_protected_attempt(
    path: &Path,
    attempt: &AnalyzedProject,
    before_path: &Path,
    baseline: &AnalyzedProject,
    request_text: &str,
    expected_before: Option<&str>,
    expected_before_state: Option<&str>,
) -> Result<String, String> {
    checked(baseline)?;
    let request: Request = serde_json::from_str(request_text).map_err(|_| {
        "Protected attempt request must match the closed request schema.".to_owned()
    })?;
    let request_value: Value =
        serde_json::from_str(request_text).map_err(|_| "Invalid request JSON.")?;
    let strings = [
        &request.entity,
        &request.field,
        &request.callable,
        &request.row.parameter,
        &request.row.nominal_type,
        &request.row.alias,
        &request.from.nominal_type,
        &request.from.alias,
        &request.to.parameter,
        &request.to.nominal_type,
        &request.to.alias,
    ];
    if strings
        .iter()
        .any(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
    {
        return Err("Request names and aliases must be bounded nonempty strings.".into());
    }
    let before = snapshot(before_path, baseline)?;
    for (expected, field) in [
        (expected_before, "source_digest"),
        (expected_before_state, "state_digest"),
    ] {
        if expected.is_some_and(|expected| before[field].as_str() != Some(expected)) {
            return Err(format!("Baseline {field} does not match its supplied pin."));
        }
    }
    let (field_source, entity, field) = protected_field(baseline, &request)?;
    let (attempt_source, callable) = selected_callable(attempt, &request.callable)?;
    if callable.kind != CallableKind::Action {
        return Err("The supported attempt must be an ordinary action.".into());
    }
    let update = single_update(callable)?;
    let [assignment] = update.changes.as_slice() else {
        return Err("The attempt must assign exactly one protected field.".into());
    };
    if name(&update.target) != request.entity
        || assignment.name.text != request.field
        || update.transition.is_some()
        || !update.conditional_changes.is_empty()
        || update.patch.is_some()
        || update.empty.is_some()
    {
        return Err("The actual update target must match the requested ordinary protected-field assignment.".into());
    }
    let identity = entity
        .dossier
        .as_ref()
        .map(|d| d.identity.text.as_str())
        .or_else(|| {
            entity
                .fields
                .iter()
                .find(|f| f.persistence.contains(&PersistenceModifier::Identity))
                .map(|f| f.name.text.as_str())
        })
        .ok_or("The checked entity must declare its identity field.")?;
    let row_type = format!("{}.{}", request.entity, identity);
    let owner_type = type_name(&field.field_type);
    if update.field.text != identity
        || request.row.nominal_type != row_type
        || request.from.nominal_type != owner_type
        || request.to.nominal_type != owner_type
        || expression_name(&update.value) != Some(request.row.parameter.as_str())
        || request.row.parameter == request.to.parameter
    {
        return Err("The parsed row predicate and supplied nominal bindings do not match the checked field contracts.".into());
    }
    let row_parameter = parameter(callable, &request.row.parameter, &row_type)?;
    let to_parameter = parameter(callable, &request.to.parameter, &owner_type)?;
    let value_form = match &assignment.value {
        Expression::Invocation(value)
            if name(&value.callee) == format!("{}.{}", request.entity, request.field)
                && value.named_arguments.is_empty()
                && value.arguments.len() == 1
                && expression_name(&value.arguments[0]) == Some(request.to.parameter.as_str()) =>
        {
            "field_nominal_constructor"
        }
        _ => {
            return Err(
                "The parsed assignment must use the exact requested destination parameter.".into(),
            )
        }
    };
    if surrounding_tokens(before_path, baseline, &request.callable)
        != surrounding_tokens(path, attempt, &request.callable)
    {
        return Err(
            "Source outside the selected attempted callable differs from the checked baseline."
                .into(),
        );
    }
    let mut diagnostics = Vec::new();
    let mut protected_diagnostic = None;
    let streams: Vec<(&str, Vec<&Diagnostic>)> = vec![
        ("syntax", attempt.syntax.diagnostics().collect()),
        ("semantics", attempt.semantics.diagnostics.iter().collect()),
        ("typing", attempt.typing.diagnostics.iter().collect()),
        ("failures", attempt.failures.diagnostics.iter().collect()),
        (
            "entity_model",
            attempt.entity_model.diagnostics.iter().collect(),
        ),
        ("policy", attempt.policy.diagnostics.iter().collect()),
    ];
    for (stage, rows) in streams {
        for diagnostic in rows {
            if diagnostic.severity == Severity::Error {
                let exact_field = stage == "typing"
                    && diagnostic.code == "TYPE_FIELD_UPDATE_FORBIDDEN"
                    && diagnostic.primary.as_ref().is_some_and(|span| {
                        span.source == attempt_source.source_name
                            && span.start == assignment.name.range.start
                            && span.end == assignment.name.range.end
                    });
                // Delivery completion deliberately refuses any frontend error. Only
                // its exact unchanged authored job span may accompany this refusal.
                let derived_delivery = stage == "semantics" && diagnostic.code == "TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED"
                    && diagnostic.primary.as_ref().is_some_and(|span| attempt.syntax.sources.iter().any(|source|
                        source.source_name == span.source && source.file.declarations.iter().any(|declaration|
                            matches!(declaration, Declaration::Job(job) if job.delivery.as_ref().is_some_and(|d|
                                    d.range.start == span.start && d.range.end == span.end)))));
                if exact_field {
                    if protected_diagnostic.is_some() {
                        return Err("Ambiguous protected-field refusal.".into());
                    }
                    protected_diagnostic = Some(diagnostic_json(path, stage, diagnostic));
                } else if !derived_delivery {
                    return Err("The attempt contains an unrelated frontend error; it cannot derive a protected-change disposition.".into());
                }
            }
            diagnostics.push(diagnostic_json(path, stage, diagnostic));
        }
    }
    let protected_diagnostic = protected_diagnostic
        .ok_or("The exact attempted assignment has no native protected-field refusal.")?;
    let mut sources = BTreeMap::new();
    for source in &attempt.syntax.sources {
        sources.insert(
            normalized_source(project_root(path), &source.source_name),
            digest(source.source_text.as_bytes()),
        );
    }
    let canonical = json!({
        "schema_version":1,"kind":"protected_change_attempt","canonicalization":CANONICALIZATION,
        "compiler_version":env!("CARGO_PKG_VERSION"),"policy_schema_version":1,
        "status":"local_inspection_non_releasable","release_authority":false,
        "release_equivalent_success":false,"disposition":"requires_human_decision",
        "policy_rule":format!("{}.fields.{}",request.entity,request.field),
        "disposition_reason":"The ordinary assignment is compiler-refused; changing the immutable direct-role protection requires a separate human-owned design decision. This record grants no transfer or release.",
        "before":before,
        "request":{"classification":"supplied_unverified","value":request_value,
            "digest":digest(canonical_bytes(&request_value).as_bytes()),"live_identity_validation":"unavailable"},
        "protected_field":{"status":"compiler_checked_baseline_contract",
            "entity":request.entity,"field":request.field,"declared_nominal_type":owner_type,
            "field_nominal_type":format!("{}.{}",request.entity,request.field),
            "immutable":field.immutable,"direct_role":name(&field.role.as_ref().expect("checked role").role),
            "entity_location":location(before_path,field_source,entity.range),
            "field_location":location(before_path,field_source,field.range),
            "type_location":location(before_path,field_source,field.field_type.range),
            "role_location":location(before_path,field_source,field.role.as_ref().expect("checked role").range),
            "tokens":contract_tokens(field_source,field.range)},
        "attempt":{"status":"frontend_rejected_unexecuted","sources":sources,
            "source_digest":digest(canonical_bytes(&json!(sources)).as_bytes()),
            "callable":request.callable,"callable_location":location(path,attempt_source,callable.range),
            "callable_tokens":contract_tokens(attempt_source,callable.range),
            "update_location":location(path,attempt_source,update.range),
            "predicate":{"field":update.field.text,"field_location":location(path,attempt_source,update.field.range),
                "parameter":request.row.parameter,
                "location":location(path,attempt_source,update.value.range()),
                "parameter_location":location(path,attempt_source,row_parameter.range)},
            "assignment":{"field":assignment.name.text,"field_location":location(path,attempt_source,assignment.name.range),
                "location":location(path,attempt_source,assignment.range),"tokens":contract_tokens(attempt_source,assignment.range),
                "value_form":value_form,"value_location":location(path,attempt_source,assignment.value.range()),
                "destination_parameter_location":location(path,attempt_source,to_parameter.range),
                "value_status":"parsed_nominal_spelling_not_checked_value_or_data_origin"},
            "refusal_diagnostic":protected_diagnostic,"diagnostics":diagnostics,
            "checked_after":null,"semantic_graph":null,"feasibility":"unavailable","runtime_authority":"unavailable"},
        "evidence":{"proved":["The baseline passes frontend checks.","The exact parsed ordinary assignment has the native protected-field diagnostic.","Other source tokens equal the baseline."],
            "runtime_validated":[],"tested":[],"unsupported":["live fixture identities","ownership transfer","checked after state","protected issuer or release validation"]}
    });
    Ok(canonical_bytes(
        &json!({"schema_version":1,"kind":"protected_change_attempt",
        "subject_digest":digest(canonical_bytes(&canonical).as_bytes()),"canonical":canonical}),
    ))
}

/// Full equality with a freshly derived subject, including coherent rehashes.
/// This is local renderer conformance, never an issuer/attestation validator.
pub fn validate_protected_attempt_export(expected: &str, exported: &str) -> Result<(), String> {
    let parse = |s: &str| {
        serde_json::from_str::<Value>(s).map_err(|_| "Invalid protected attempt JSON.".to_owned())
    };
    let expected = parse(expected)?;
    let exported = parse(exported)?;
    let c = &expected["canonical"];
    if c["schema_version"] != 1
        || c["kind"] != "protected_change_attempt"
        || c["canonicalization"] != CANONICALIZATION
        || c["status"] != "local_inspection_non_releasable"
        || c["release_authority"] != false
        || c["release_equivalent_success"] != false
        || c["disposition"] != "requires_human_decision"
        || expected["schema_version"] != 1
        || expected["kind"] != "protected_change_attempt"
        || expected != exported
        || expected["subject_digest"] != digest(canonical_bytes(&expected["canonical"]).as_bytes())
    {
        return Err("Export differs from the freshly derived protected attempt.".into());
    }
    Ok(())
}

fn name(value: &NameExpression) -> String {
    value
        .path
        .iter()
        .map(|n| n.text.as_str())
        .collect::<Vec<_>>()
        .join(".")
}
fn type_name(value: &TypeReference) -> String {
    value
        .path
        .iter()
        .map(|n| n.text.as_str())
        .collect::<Vec<_>>()
        .join(".")
}
fn expression_name(value: &Expression) -> Option<&str> {
    match value {
        Expression::Name(n) if n.path.len() == 1 => Some(&n.path[0].text),
        _ => None,
    }
}
fn parameter<'a>(
    callable: &'a CallableDeclaration,
    name: &str,
    nominal: &str,
) -> Result<&'a Parameter, String> {
    let rows = callable
        .parameters
        .iter()
        .filter(|p| p.name.text == name)
        .collect::<Vec<_>>();
    let [parameter] = rows.as_slice() else {
        return Err("The requested parameter is absent or ambiguous.".into());
    };
    if parameter.parameter_type.nullable
        || !parameter.parameter_type.arguments.is_empty()
        || type_name(&parameter.parameter_type) != nominal
    {
        return Err("The requested parameter does not declare the exact nominal type.".into());
    }
    Ok(parameter)
}
fn protected_field<'a>(
    project: &'a AnalyzedProject,
    request: &Request,
) -> Result<
    (
        &'a ParsedSyntax,
        &'a RecordDeclaration,
        &'a FieldDeclaration,
    ),
    String,
> {
    let rows = project
        .syntax
        .sources
        .iter()
        .flat_map(|source| {
            source
                .file
                .declarations
                .iter()
                .filter_map(move |d| match d {
                    Declaration::Record(r)
                        if r.kind == RecordKind::Entity && r.name.text == request.entity =>
                    {
                        Some((source, r))
                    }
                    _ => None,
                })
        })
        .collect::<Vec<_>>();
    let [(source, entity)] = rows.as_slice() else {
        return Err("The checked protected entity is absent or ambiguous.".into());
    };
    let fields = entity
        .fields
        .iter()
        .filter(|f| f.name.text == request.field)
        .collect::<Vec<_>>();
    let [field] = fields.as_slice() else {
        return Err("The checked protected field is absent or ambiguous.".into());
    };
    if !field.immutable
        || field.role.is_none()
        || field.optional
        || field.field_type.nullable
        || !field.field_type.arguments.is_empty()
    {
        return Err("This refusal requires an immutable, required direct-role field in the checked baseline.".into());
    }
    Ok((source, entity, field))
}
fn callable_name(c: &CallableDeclaration) -> String {
    c.name.text.clone()
}
fn selected_callable<'a>(
    project: &'a AnalyzedProject,
    selected: &str,
) -> Result<(&'a ParsedSyntax, &'a CallableDeclaration), String> {
    let rows = project
        .syntax
        .sources
        .iter()
        .flat_map(|source| {
            source
                .file
                .declarations
                .iter()
                .filter_map(move |d| match d {
                    Declaration::Callable(c) if callable_name(c) == selected => Some((source, c)),
                    _ => None,
                })
        })
        .collect::<Vec<_>>();
    let [row] = rows.as_slice() else {
        return Err("The exact attempted callable is absent or ambiguous.".into());
    };
    Ok(*row)
}
fn single_update(callable: &CallableDeclaration) -> Result<&UpdateExpression, String> {
    let value = match callable.body.statements.as_slice() {
        [Statement::Return(r)] => &r.value,
        [Statement::Binding(b), Statement::Return(r)]
            if expression_name(&r.value) == Some(b.name.text.as_str()) =>
        {
            &b.value
        }
        _ => return Err(
            "The supported callable must contain only the single attempted update and its return."
                .into(),
        ),
    };
    match value {
        Expression::Attempt(a) => match a.value.as_ref() {
            Expression::Update(u) => Ok(u),
            _ => Err("The callable does not directly attempt an update.".into()),
        },
        _ => Err("The callable does not directly attempt an update.".into()),
    }
}
fn surrounding_tokens(
    path: &Path,
    project: &AnalyzedProject,
    selected: &str,
) -> BTreeMap<String, Vec<String>> {
    project
        .syntax
        .sources
        .iter()
        .filter_map(|source| {
            let excluded = source
                .file
                .declarations
                .iter()
                .filter_map(|d| match d {
                    Declaration::Callable(c) if callable_name(c) == selected => Some(c.range),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let tokens = source
                .tokens
                .iter()
                .filter(|t| {
                    !t.kind.is_trivia()
                        && !t.text(&source.source_text).is_empty()
                        && !excluded
                            .iter()
                            .any(|r| t.range.start >= r.start && t.range.end <= r.end)
                })
                .map(|t| t.text(&source.source_text).to_owned())
                .collect::<Vec<_>>();
            (!tokens.is_empty()).then(|| {
                (
                    normalized_source(project_root(path), &source.source_name),
                    tokens,
                )
            })
        })
        .collect()
}
fn location(path: &Path, source: &ParsedSyntax, range: TextRange) -> Value {
    json!({"source":normalized_source(project_root(path),&source.source_name),"start":range.start,"end":range.end})
}
fn diagnostic_json(path: &Path, stage: &str, d: &Diagnostic) -> Value {
    let span = |s: &jadpo_diagnostics::SourceSpan| json!({"source":normalized_source(project_root(path),&s.source),"start":s.start,"end":s.end});
    json!({"stage":stage,"code":d.code,"rule_id":d.rule_id,"severity":d.severity.to_string(),
        "message":d.message,"reason":d.reason,"context":d.context,"source_revision":d.source_revision,
        "primary":d.primary.as_ref().map(span),"related":d.related.iter().map(span).collect::<Vec<_>>(),"notes":d.notes})
}
