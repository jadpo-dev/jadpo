//! Closed first-match inference under explicit paired host/request assumptions.
//! Renderer conformance never supplies runtime observations or release authority.
use super::super::{digest, normalized_source, project_root, AnalyzedProject, Value};
use jadpo_syntax::{CredentialLocation, Declaration, HttpMethod, RouteDeclaration, TokenKind};
use serde_json::json;
use std::path::Path;

fn ascii_segment(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
fn canonical_witness(path: &str) -> bool {
    path.starts_with('/')
        && !matches!(path, "/health" | "/health/live" | "/health/ready")
        && path.split('/').skip(1).all(ascii_segment)
}
// Exactly the native slash/whole-parameter matcher domain. Parameter types do
// not establish disjointness: native dispatch selects before parsing them.
fn closed_disjoint(prior: &RouteDeclaration, witness: &str) -> Option<bool> {
    if !prior.path.starts_with('/') {
        return None;
    }
    let expected: Vec<_> = prior.path.split('/').collect();
    let received: Vec<_> = witness.split('/').collect();
    for segment in expected.iter().skip(1) {
        if let Some(name) = segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            let mut chars = name.bytes();
            if !chars
                .next()
                .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
                || !chars.all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || !prior.path_fields.iter().any(|f| f.name.text == name)
            {
                return None;
            }
        } else if !segment.is_empty() && !ascii_segment(segment) {
            return None;
        }
    }
    if expected.len() != received.len() {
        return Some(true);
    }
    Some(
        expected
            .iter()
            .zip(received)
            .any(|(e, r)| !(e.starts_with('{') && e.ends_with('}')) && *e != r),
    )
}

pub(super) fn derive(
    path: &Path,
    project: &AnalyzedProject,
    file_index: usize,
    declaration_index: usize,
    route: &RouteDeclaration,
) -> Value {
    let unavailable = || json!({"status":"not_analyzed","facts":null});
    if !canonical_witness(&route.path) {
        return unavailable();
    }
    let all = project
        .syntax
        .sources
        .iter()
        .enumerate()
        .flat_map(|(si, s)| {
            s.file
                .declarations
                .iter()
                .enumerate()
                .filter_map(move |(di, d)| {
                    if let Declaration::Route(r) = d {
                        Some((si, di, r))
                    } else {
                        None
                    }
                })
        })
        .collect::<Vec<_>>();
    if all
        .iter()
        .filter(|(_, _, r)| r.method == HttpMethod::Get && r.path == route.path)
        .count()
        != 1
    {
        return unavailable();
    }
    let mut prior = Vec::new();
    for (si, di, r) in &all {
        if (*si, *di) == (file_index, declaration_index) {
            break;
        }
        if r.method != HttpMethod::Get {
            continue;
        }
        if closed_disjoint(r, &route.path) != Some(true) {
            return unavailable();
        }
        prior.push(json!({"source":normalized_source(project_root(path),&project.syntax.sources[*si].source_name),"declaration":di,"template":r.path,"proof":"closed_slash_segments_disjoint"}));
    }
    let mut context = Vec::new();
    for (si, source) in project.syntax.sources.iter().enumerate() {
        let mut tokens = Vec::new();
        let nontrivia = source
            .tokens
            .iter()
            .filter(|t| !t.kind.is_trivia())
            .collect::<Vec<_>>();
        let mut i = 0;
        let mut depth = 0;
        let mut auth_count = 0;
        while i < nontrivia.len() {
            let t = nontrivia[i];
            let in_candidate = si == file_index
                && t.range.start >= route.range.start
                && t.range.end <= route.range.end;
            if in_candidate && depth == 1 && t.kind == TokenKind::Auth {
                if nontrivia.get(i + 1).map(|t| t.kind) != Some(TokenKind::Colon)
                    || nontrivia.get(i + 2).map(|t| t.kind) != Some(TokenKind::NoneLiteral)
                {
                    return unavailable();
                }
                auth_count += 1;
                i += 3;
                continue;
            }
            if in_candidate {
                match t.kind {
                    TokenKind::LeftBrace => depth += 1,
                    TokenKind::RightBrace => depth -= 1,
                    _ => {}
                }
            }
            tokens.push(t.text(&source.source_text));
            i += 1;
        }
        if si == file_index && auth_count != usize::from(route.public) {
            return unavailable();
        }
        context.push(json!({"source":normalized_source(project_root(path),&source.source_name),"tokens":tokens}));
    }
    let credential_slots=project.syntax.sources.iter().flat_map(|source|source.file.declarations.iter().filter_map(move |declaration|{
        let Declaration::AuthenticationStrategy(strategy)=declaration else{return None;};
        let (wire_header,location)=match &strategy.transport.location{
            CredentialLocation::Cookie(cookie)=>("cookie",json!({"cookie_name_token":cookie.text})),
            CredentialLocation::Bearer(name)=>("authorization",json!({"bearer_location":name.text})),
        };
        Some(json!({"strategy":strategy.name.text,"wire_header":wire_header,"declared_location":location,"absence":"entire_header_absent"}))
    })).collect::<Vec<_>>();
    json!({"status":"closed_selected_source_context","facts":{
        "selected":{"source":normalized_source(project_root(path),&project.syntax.sources[file_index].source_name),"source_ordinal":file_index,"declaration":declaration_index,"method":"GET","path":route.path},
        "credential_slots":credential_slots,
        "prior_get_routes":prior,"context_digest":digest(serde_json::to_vec(&context).unwrap().as_slice()),
        "ordered_sources":context.iter().map(|v|v["source"].clone()).collect::<Vec<_>>(),
        "matcher":"native_first_match_slash_segments_whole_parameter_v1"}})
}

pub(super) fn compare(before: &Value, after: &Value, old: &Value, new: &Value) -> Value {
    let a = &old["conditional_source"];
    let b = &new["conditional_source"];
    if a["status"] != "closed_selected_source_context"
        || a != b
        || before["provenance"]["compiler"] != after["provenance"]["compiler"]
        || before["provenance"]["schema_registry"] != after["provenance"]["schema_registry"]
    {
        return json!({"status":"not_generated","facts":null});
    }
    json!({"status":"conditional_feasible_under_assumptions",
        "request":{"method":"GET","path":new["facts"]["path"],"query_body_authored_headers":"absent","credentials":"all_declared_credential_slots_absent","credential_slots":b["facts"]["credential_slots"],"cancellation":"uncancelled"},
        "dispatch":{"status":"selected_by_closed_first_match_rule","source_facts":b["facts"],"paired_context":"identical_ordered_source_tokens_except_selected_auth_none","paired_compiler":"identical_exact_input_provenance"},
        "assumptions":{"configuration":"both_snapshots_valid_checked_configuration_and_supported_authentication_runtime","application":"both_generated_hosts_initialized","database":"both_native_database_initialization_and_readiness_healthy","resources":"ordinary_transport_resource_validation_policy_audit_boundaries_permit_request"},
        "conditional_outcomes":{"before":{"status":401,"code":"authentication_required"},"after":{"status":200,"body":"unchanged_checked_plain_Text_literal","literal_token":new["facts"]["literal_token"]}},
        "evidence":"conditional_source_inference_from_closed_dispatch_and_accepted_authentication_contract","execution":"not_proved","release_authority":false})
}
