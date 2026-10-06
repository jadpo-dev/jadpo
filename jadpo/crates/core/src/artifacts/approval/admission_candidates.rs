//! A closed source-only anonymous literal-route candidate, never execution proof.
mod finite_witness;
use super::{contract_tokens, method_name, AnalyzedProject, Value};
use jadpo_syntax::{Declaration, Expression, HttpMethod, LiteralKind, RouteSuccess, Statement};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

pub(super) fn derive(path: &Path, project: &AnalyzedProject, deployment: &Value) -> Value {
    let supported = deployment["status"] == "compiler_supported_expected_native_contract"
        && crate::target::first_party_authentication_supported(path, project);
    let mut routes = BTreeMap::new();
    for (source_index, source) in project.syntax.sources.iter().enumerate() {
        for (declaration_index, declaration) in source.file.declarations.iter().enumerate() {
            let Declaration::Route(route) = declaration else {
                continue;
            };
            let key = format!("{} {}", method_name(route.method), route.path);
            // Only literal URL paths whose native router and URL.pathname agree.
            // The deployment/liveness plane bypasses ordinary application auth.
            let static_path = route.path.starts_with('/')
                && route
                    .path
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"/-_".contains(&c))
                && !matches!(route.path.as_str(), "/health/ready" | "/health/live");
            let plain_text = route.output.as_ref().is_some_and(|output| {
                output.path.len() == 1
                    && output.path[0].text == "Text"
                    && output.arguments.is_empty()
                    && !output.nullable
            });
            let literal = route.inline_action.as_ref().and_then(|action| {
                if !action.failures.is_empty() {
                    return None;
                }
                let [Statement::Return(statement)] = action.body.statements.as_slice() else {
                    return None;
                };
                let Expression::Literal(literal) = &statement.value else {
                    return None;
                };
                (literal.kind == LiteralKind::String).then_some((literal, &action.body))
            });
            let eligible = supported
                && route.method == HttpMethod::Get
                && static_path
                && plain_text
                && route.success == RouteSuccess::Ok
                && !route.fresh_authority
                && route.path_fields.is_empty()
                && route.query.is_none()
                && route.input.is_none()
                && route.headers.is_empty()
                && route.deadline.is_none()
                && route.run.is_none();
            let mut entry = if let Some((literal, body)) = literal.filter(|_| eligible) {
                json!({"status":"checked_plain_text_literal_route",
                    "facts":{"method":"GET","path":route.path,"output":"Text",
                        "literal_token":literal.text,"body_tokens":contract_tokens(source,body.range),
                        "authored_transport_values":"none","authored_principal_or_effect_dependencies":"none"},
                    "evidence":"checked_closed_ast_and_supported_native_authentication_target",
                    "execution":"not_proved","feasibility":"not_analyzed"})
            } else {
                json!({"status":"unsupported_analysis","facts":null,
                    "reason":"outside_closed_native_plain_text_literal_route_scope"})
            };
            if entry["status"] == "checked_plain_text_literal_route" {
                entry.as_object_mut().unwrap().insert(
                    "conditional_source".into(),
                    finite_witness::derive(path, project, source_index, declaration_index, route),
                );
            }
            routes.insert(key, entry);
        }
    }
    json!({"schema_version":1,"status":"bounded_checked_source_candidates",
        "routes":routes,"runtime_observations":null,"release_authority":false})
}

pub(super) fn compare(before: Option<&Value>, after: &Value, route: &str) -> Option<Value> {
    let before = before?;
    let old = &before["impact"]["anonymous_admission_candidates"]["routes"][route];
    let new = &after["impact"]["anonymous_admission_candidates"]["routes"][route];
    if old["status"] != "checked_plain_text_literal_route"
        || new["status"] != "checked_plain_text_literal_route"
        || old["facts"] != new["facts"]
        || super::route_auth_mode(before, route) != Some("authenticated_default")
        || super::route_auth_mode(after, route) != Some("none")
    {
        return None;
    }
    Some(
        json!({"status":"source_derived_anonymous_admission_candidate",
        "conditional_witness":finite_witness::compare(before,after,old,new),
        "facts":{"actor":"unauthenticated_public_caller",
            "request":{"method":"GET","path":new["facts"]["path"],"credentials":"absent","authored_transport_values":"none"},
            "checked_literal_return":new["facts"],
            "admission":{"before":"credential_required_by_native_route_contract","after":"anonymous_route_contract"},
            "assumptions":{"application":"initialized_with_valid_configuration",
                "database":"native_readiness_available_if_persistent_entities",
                "host":"supported_native_application_boundary"},
            "evidence":"compiler_checked_source_candidate_not_execution_or_feasibility_proof",
            "execution":"not_proved","feasibility":"not_analyzed","release_authority":false}}),
    )
}
