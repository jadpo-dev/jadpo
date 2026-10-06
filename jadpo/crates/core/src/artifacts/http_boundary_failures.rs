//! Native HTTP boundary contracts derived from checked route and authentication
//! declarations. Conditional emitted outcomes, not feasibility or authority.
use crate::AnalyzedProject;
use jadpo_syntax::{CredentialLocation, Declaration, HttpMethod};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Clone)]
pub(crate) struct Failure {
    pub code: String,
    pub status: u16,
    pub message: String,
    pub condition: String,
    pub declared_failure: Option<String>,
}
impl Failure {
    fn native(code: &str, status: u16, message: &str, condition: &str) -> Self {
        Self {
            code: code.into(),
            status,
            message: message.into(),
            condition: condition.into(),
            declared_failure: None,
        }
    }
    pub fn schema(&self) -> Value {
        json!({"type":"object","additionalProperties":false,"required":["error"],"properties":{
            "error":{"type":"object","additionalProperties":false,"required":["code","message","request_id"],"properties":{
                "code":{"type":"string","const":self.code},"message":{"type":"string","const":self.message},"request_id":{"type":"string"}
            }}
        }})
    }
    pub fn json(&self) -> Value {
        json!({"code":self.code,"http_status":self.status,"message":self.message,"condition":self.condition,
            "declared_failure":self.declared_failure,"envelope":"closed_native_error","internal_context_disclosed":false})
    }
}
#[derive(Clone, Default)]
pub(crate) struct Facts {
    pub failures: Vec<Failure>,
    pub unresolved: BTreeSet<String>,
}
impl Facts {
    pub fn failures_json(&self) -> String {
        serde_json::to_string(&self.failures.iter().map(Failure::json).collect::<Vec<_>>()).unwrap()
    }
    pub fn json(&self) -> String {
        json!({"qualified":self.unresolved.is_empty(),"status":if self.unresolved.is_empty(){"checked_native_boundary"}else{"unqualified_native_mapping"},
            "evidence":"checked_routes_strategies_resolutions_and_native_http_lowering","scope":"conditional_emitted_outcomes_not_feasibility_or_authority","unresolved":self.unresolved}).to_string()
    }
    fn parser(&mut self, layer: &str, syntax: bool, code: &str) {
        if syntax {
            self.failures.push(Failure::native(
                "invalid_request",
                400,
                &format!("Request {layer} syntax is invalid."),
                &format!("checked_{layer}_syntax_error"),
            ));
        }
        let message_layer = if layer == "JSON" { "body" } else { layer };
        self.failures.push(Failure::native(
            code,
            422,
            &format!("Request {message_layer} value is invalid."),
            &format!("checked_{message_layer}_validation_error"),
        ));
    }
}
pub(crate) struct Analysis {
    routes: BTreeMap<String, Facts>,
}
impl Analysis {
    pub fn new(path: &Path, project: &AnalyzedProject) -> Self {
        let supported = crate::target::first_party_authentication_supported(path, project);
        let strategies = project
            .syntax
            .sources
            .iter()
            .flat_map(|s| &s.file.declarations)
            .filter_map(|d| match d {
                Declaration::AuthenticationStrategy(s) => Some(s),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut auth = Facts::default();
        if supported {
            for (code, status, condition) in [
                ("authentication_required", 401, "no_presented_credential"),
                (
                    "ambiguous_credentials",
                    401,
                    "multiple_presented_credentials",
                ),
                (
                    "invalid_credentials",
                    401,
                    "invalid_or_missing_authority_credential",
                ),
                (
                    "authentication_unavailable",
                    503,
                    "credential_or_authority_dependency_unavailable",
                ),
                (
                    "authentication_misconfigured",
                    503,
                    "native_authentication_configuration_unavailable",
                ),
                (
                    "authority_invariant",
                    500,
                    "native_authentication_contract_invariant",
                ),
            ] {
                auth.failures.push(Failure::native(
                    code,
                    status,
                    "Authentication failed.",
                    condition,
                ));
            }
            // The native external JWT validator can return this distinct
            // outcome. Local credential verification does not return it.
            if strategies
                .iter()
                .any(|s| s.validators.iter().any(|v| v.mode.text == "jwt"))
            {
                auth.failures.push(Failure::native(
                    "authentication_misconfigured",
                    500,
                    "Authentication failed.",
                    "checked_jwt_validator_returns_misconfigured",
                ));
            }
            let mut inactive = BTreeSet::new();
            for resolution in strategies.iter().flat_map(|s| &s.resolutions) {
                if !inactive.insert(resolution.inactive.text.clone()) {
                    continue;
                }
                if let Some(contract) = project
                    .failures
                    .contracts
                    .iter()
                    .find(|f| f.name == resolution.inactive.text)
                {
                    if let Some(status) = contract.http_status {
                        let mut failure = Failure::native(
                            &contract.code,
                            status,
                            contract
                                .message
                                .as_deref()
                                .unwrap_or("Authentication failed."),
                            "checked_selected_principal_inactive",
                        );
                        failure.declared_failure = Some(contract.name.clone());
                        auth.failures.push(failure);
                    } else {
                        auth.unresolved
                            .insert(format!("inactive_http_status:{}", contract.name));
                    }
                } else {
                    auth.unresolved
                        .insert(format!("inactive_contract:{}", resolution.inactive.text));
                }
            }
        } else {
            auth.unresolved
                .insert("first_party_authentication_not_lowered".into());
        }
        let cookie = strategies
            .iter()
            .any(|s| matches!(s.transport.location, CredentialLocation::Cookie(_)));
        let mut mutative = project
            .entity_model
            .transactions
            .iter()
            .filter(|t| !t.owners.is_empty())
            .map(|t| t.action.clone())
            .collect::<BTreeSet<_>>();
        loop {
            let before = mutative.len();
            for edge in &project.semantics.calls {
                let caller = &project.semantics.nodes[edge.caller.0 as usize].name;
                let callee = &project.semantics.nodes[edge.callee.0 as usize].name;
                if mutative.contains(callee) {
                    mutative.insert(caller.clone());
                }
            }
            if mutative.len() == before {
                break;
            }
        }
        let mut routes = BTreeMap::new();
        for declaration in project
            .syntax
            .sources
            .iter()
            .flat_map(|s| &s.file.declarations)
        {
            if let Declaration::Route(route) = declaration {
                let key = format!("{} {}", super::method_name(route.method), route.path);
                // Native liveness returns before authentication and route input.
                if route.method == HttpMethod::Get && route.path == "/health/live" {
                    routes.insert(key, Facts::default());
                    continue;
                }
                let mut facts = if route.public {
                    Facts::default()
                } else {
                    auth.clone()
                };
                if !route.public
                    && supported
                    && cookie
                    && (route.method != HttpMethod::Get || mutative.contains(&key))
                {
                    facts.failures.push(Failure::native("csrf_rejected",403,"Authentication failed.","selected_cookie_unsafe_method_or_checked_mutation_requires_origin_and_session_csrf"));
                }
                if !route.public
                    && supported
                    && cookie
                    && route.method == HttpMethod::Get
                    && route.inline_action.is_some()
                {
                    facts
                        .unresolved
                        .insert("safe_method_inline_cookie_mutation_mapping_unqualified".into());
                }
                if route.query.is_some() {
                    facts.parser("query", true, "invalid_value");
                }
                if !route.headers.is_empty() {
                    facts.parser("header", true, "invalid_value");
                }
                if !route.path_fields.is_empty() {
                    facts.parser("path", false, "invalid_value");
                }
                if route.input.is_some() {
                    facts.parser("JSON", true, "invalid_input");
                }
                routes.insert(key, facts);
            }
        }
        for strategy in &strategies {
            if let Some(exchange) = &strategy.exchange {
                let mut facts = auth.clone();
                facts.failures.retain(|f| {
                    if f.code == "authentication_misconfigured" && f.status == 500 {
                        return false;
                    }
                    f.declared_failure.as_ref().map_or(true, |name| {
                        strategy
                            .resolutions
                            .iter()
                            .any(|r| &r.inactive.text == name)
                    })
                });
                routes.insert(
                    format!("POST {}", super::unquote(&exchange.path.text)),
                    facts,
                );
            }
        }
        Self { routes }
    }
    pub fn route(&self, name: &str) -> Facts {
        self.routes.get(name).cloned().unwrap_or_else(|| {
            let mut f = Facts::default();
            f.unresolved.insert("route_not_mapped".into());
            f
        })
    }
    pub fn audit(&self) -> String {
        let mut rows = BTreeMap::<String, (Failure, Vec<String>)>::new();
        for (route, facts) in &self.routes {
            for failure in &facts.failures {
                let key = failure.json().to_string();
                let row = rows
                    .entry(key)
                    .or_insert_with(|| (failure.clone(), Vec::new()));
                row.1.push(route.clone());
            }
        }
        let failures = rows
            .values()
            .map(|(failure, routes)| {
                let mut row = failure.json();
                row["routes"] = json!(routes);
                row
            })
            .collect::<Vec<_>>();
        json!({"failures":failures,"routes":self.routes.iter().map(|(route,facts)|json!({"route":route,"analysis":serde_json::from_str::<Value>(&facts.json()).unwrap()})).collect::<Vec<_>>()}).to_string()
    }
}
