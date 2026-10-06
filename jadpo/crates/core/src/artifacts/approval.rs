//! Local behavioral review data. This module never accepts an attestation.
use super::{method_name, normalized_source, project_root, ArtifactModel};
use crate::AnalyzedProject;
use jadpo_diagnostics::Severity;
use jadpo_semantic::NodeKind;
use jadpo_syntax::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

const SCHEMA: u32 = 5;
const CANONICALIZATION: &str = "jadpo.approval-subject.v5";

mod actor_links;
mod admission_candidates;
mod protected_attempt;
pub use protected_attempt::{derive_protected_attempt, validate_protected_attempt_export};
mod field_contracts;

/// Both sides are freshly analyzed source, never a self-asserted previous artifact.
/// `expected_before` pins the exact baseline source digest obtained from a prior
/// inspection. It detects accidental/stale selection, not a trusted issuer.
pub fn derive_approval_subject(
    path: &Path,
    project: &AnalyzedProject,
    before: Option<(&Path, &AnalyzedProject)>,
    intent: Option<&str>,
    expected_before: Option<&str>,
) -> Result<String, String> {
    derive_approval_subject_with_state_pin(path, project, before, intent, expected_before, None)
}

/// The state pin additionally binds registry, compiler-input provenance and all
/// extracted facts. The original source-only pin remains available explicitly.
pub fn derive_approval_subject_with_state_pin(
    path: &Path,
    project: &AnalyzedProject,
    before: Option<(&Path, &AnalyzedProject)>,
    intent: Option<&str>,
    expected_before: Option<&str>,
    expected_before_state: Option<&str>,
) -> Result<String, String> {
    checked(project)?;
    if let Some((_, baseline)) = before {
        checked(baseline)?;
    }
    let after = snapshot(path, project)?;
    let before = before
        .map(|(path, project)| snapshot(path, project))
        .transpose()?;
    if let Some(expected) = expected_before {
        if before
            .as_ref()
            .and_then(|value| value["source_digest"].as_str())
            != Some(expected)
        {
            return Err("Baseline source digest does not match --expected-before.".into());
        }
    }
    if let Some(expected) = expected_before_state {
        if before
            .as_ref()
            .and_then(|value| value["state_digest"].as_str())
            != Some(expected)
        {
            return Err("Baseline state digest does not match --expected-before-state.".into());
        }
    }
    let decisions = decisions(before.as_ref(), &after);
    let mut unknowns = vec![
        "Declaration identities are kind plus qualified name; renames are removal/addition, not proven equivalence.",
        "Call reachability is a conservative may-call analysis, not a proof that a branch executes.",
        "Runtime behavior, counterexample feasibility and policy weakening are not proved by this local artifact.",
        "Actor-scope candidates compare route authentication modes and exact compiler-derived policy subjects; role membership, principal-specific access, and input/data feasibility are not analyzed.",
        "Generated authentication is an opaque effect boundary; credential internals are not source-call paths.",
        "Compiler source inputs are recorded at build time; toolchain, executable and protected-build attestation are unavailable.",
    ];
    unknowns.push(if project.delivery_model().bindings().is_empty() {
        "Job facts describe checked nonexecuting schedule entries and static may-call effects only; durable worker authority, delivery/revision bindings, profiles and runtime conformance are not established. Emission analysis remains unsupported."
    } else {
        "Job facts include checked nonexecuting delivery bindings and private phase obligations, separate from ordinary may-call effects; live worker authority, transactions, execution profiles and runtime conformance are not established. Emission analysis remains unsupported."
    });
    if before.is_none() {
        unknowns.push("No before source baseline supplied; decisions compare explicit absence with the after state.");
    }
    if intent.is_none() {
        unknowns.push("No human request/intent supplied.");
    }
    unknowns.push(
        "May-effect scenarios describe compiler-checked call-graph reachability, not feasible inputs or executed behavior.",
    );
    unknowns.push("Closed anonymous literal-route candidates require healthy supported application startup/readiness; they contain no runtime observations or feasible-execution proof.");
    let scenarios = effect_scenarios(before.as_ref(), &after);
    let canonical = json!({
        "schema_version": SCHEMA, "kind": "approval_subject",
        "compiler_version": env!("CARGO_PKG_VERSION"),
        "policy_schema_version": 1, "canonicalization": CANONICALIZATION,
        "status": "local_inspection_non_releasable", "release_authority": false,
        "request": {"intent": intent, "classification": if intent.is_some() { "supplied_unverified" } else { "unknown" }},
        "before": before, "after": after, "decisions": decisions, "scenarios": scenarios,
        "evidence": {
            "proved": ["Both supplied source states pass the compiler frontend checks."],
            "runtime_validated": [], "tested": [], "operational": [], "assumed": [],
            "unsupported": ["policy weakening proof", "feasible counterexample generation", "protected issuer validation"],
            "uncertain": unknowns
        }
    });
    let bytes = canonical_bytes(&canonical);
    Ok(canonical_bytes(&json!({
        "schema_version": SCHEMA, "kind": "approval_subject",
        "subject_digest": digest(bytes.as_bytes()), "canonical": canonical
    })))
}

pub fn approval_text(subject: &str) -> String {
    // Keeping the complete data makes omission by this non-graphical renderer
    // impossible; the prose is explanatory and outside the canonical digest.
    format!("Jadpo behavioral review v5 — local inspection, non-releasable\n\nBefore/after facts and individual decisions follow. Null means absent or unknown; unsupported analysis is explicit. No approval or runtime proof is implied.\n\n{subject}")
}

/// Renderer conformance against a freshly compiler-produced subject, not an
/// attestation verifier. Even a rehashed omission must fail comparison.
pub fn validate_approval_export(expected: &str, exported: &str) -> Result<(), String> {
    let parse = |text: &str| {
        serde_json::from_str::<Value>(text).map_err(|_| "Invalid review JSON.".to_owned())
    };
    let expected = parse(expected)?;
    let exported = parse(exported)?;
    if expected["schema_version"] != SCHEMA || exported != expected {
        return Err(
            "Review export differs from the compiler subject; facts may be missing.".into(),
        );
    }
    let canonical = &expected["canonical"];
    if canonical.is_null()
        || expected["subject_digest"] != digest(canonical_bytes(canonical).as_bytes())
    {
        return Err("Review subject digest is invalid.".into());
    }
    Ok(())
}

fn checked(project: &AnalyzedProject) -> Result<(), String> {
    if project
        .syntax
        .diagnostics()
        .chain(project.semantics.diagnostics.iter())
        .chain(project.typing.diagnostics.iter())
        .chain(project.failures.diagnostics.iter())
        .chain(project.entity_model.diagnostics.iter())
        .chain(project.policy.diagnostics.iter())
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        Err("Approval subjects require checked source with no frontend errors.".into())
    } else {
        let jobs = project
            .syntax
            .sources
            .iter()
            .flat_map(|file| {
                file.file.declarations.iter().filter_map(|declaration| {
                    let Declaration::Job(job) = declaration else {
                        return None;
                    };
                    Some(job)
                })
            })
            .collect::<Vec<_>>();
        let ordinary = jobs
            .iter()
            .filter(|job| job.delivery.is_none())
            .collect::<Vec<_>>();
        if !project
            .delivery_model()
            .covers_authored_delivery(&project.syntax)
            || project.typing.jobs.len() != ordinary.len()
            || project.failures.jobs.len() != jobs.len()
            || project.delivery_model().bindings().iter().any(|binding| {
                let contracts = project
                    .failures
                    .jobs
                    .iter()
                    .filter(|contract| contract.job == binding.schedule().job)
                    .collect::<Vec<_>>();
                let [contract] = contracts.as_slice() else {
                    return true;
                };
                *contract != binding.run_failure_contract()
            })
            || ordinary.iter().any(|job| {
                let bindings = project
                    .typing
                    .jobs
                    .iter()
                    .filter(|binding| binding.job == job.name.text)
                    .collect::<Vec<_>>();
                let [binding] = bindings.as_slice() else {
                    return true;
                };
                project
                    .failures
                    .jobs
                    .iter()
                    .filter(|failure| {
                        failure.job == job.name.text && failure.callee == binding.callee
                    })
                    .count()
                    != 1
            })
        {
            return Err("Every authored job requires exactly one complete checked binding and failure contract.".into());
        }
        Ok(())
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// Recursively sorted object keys, UTF-8 compact JSON, no trailing newline.
/// Integers only in generated numeric values; literal spelling stays in strings.
/// Arrays retain semantic order; semantic sets are sorted before serialization.
fn canonical_bytes(value: &Value) -> String {
    fn ordered(value: &Value) -> Value {
        match value {
            Value::Object(values) => {
                let sorted: BTreeMap<_, _> = values
                    .iter()
                    .map(|(k, v)| (k.clone(), ordered(v)))
                    .collect();
                Value::Object(sorted.into_iter().collect())
            }
            Value::Array(values) => Value::Array(values.iter().map(ordered).collect()),
            value => value.clone(),
        }
    }
    serde_json::to_string(&ordered(value)).expect("JSON values serialize")
}

fn audit(value: String) -> Value {
    serde_json::from_str(&value).expect("compiler audit must be JSON")
}

/// Expression spelling is checked source, not a runtime value or feasibility
/// proof. Omit trivia and offsets while preserving operators and literal bytes.
fn contract_tokens(source: &ParsedSyntax, range: TextRange) -> Vec<&str> {
    source
        .tokens
        .iter()
        .filter(|token| {
            !token.kind.is_trivia()
                && token.range.start >= range.start
                && token.range.end <= range.end
        })
        .map(|token| token.text(&source.source_text))
        .collect()
}

fn principal_contracts(project: &AnalyzedProject, facts: &mut BTreeMap<String, Value>) -> Value {
    let mut categories = BTreeMap::<&str, BTreeMap<String, Value>>::from([
        ("applications", BTreeMap::new()),
        ("variants", BTreeMap::new()),
        ("transports", BTreeMap::new()),
        ("validators", BTreeMap::new()),
        ("claims", BTreeMap::new()),
        ("resolutions", BTreeMap::new()),
    ]);
    let path = |reference: &NameExpression| {
        reference
            .path
            .iter()
            .map(|name| name.text.as_str())
            .collect::<Vec<_>>()
            .join(".")
    };
    for source in &project.syntax.sources {
        for declaration in &source.file.declarations {
            match declaration {
                Declaration::Application(application) => {
                    let authentication = &application.authentication;
                    categories.get_mut("applications").unwrap().insert(
                        format!("application_auth:{}", application.name.text), json!({
                            "application":application.name.text,
                            "principal":super::type_name(&authentication.principal),
                            "revocation":{
                                "mode":match authentication.revocation.mode { RevocationMode::Immediate => "immediate", RevocationMode::Bounded => "bounded" },
                                "maximum_delay":authentication.revocation.maximum_delay.as_ref().map(|value| &value.text)
                            }
                        }));
                }
                Declaration::Principal(principal) => {
                    for variant in &principal.variants {
                        let fields: BTreeMap<_, _> = variant
                            .fields
                            .iter()
                            .map(|field| {
                                (
                                    field.name.text.clone(),
                                    json!({"type":super::type_name(&field.field_type),
                                "optional":field.optional,
                                "declaration_tokens":contract_tokens(source, field.range)}),
                                )
                            })
                            .collect();
                        categories.get_mut("variants").unwrap().insert(
                            format!(
                                "principal_variant:{}.{}",
                                principal.name.text, variant.name.text
                            ),
                            json!({"principal":principal.name.text,"variant":variant.name.text,
                                "kind":variant.kind.as_str(),"fields":fields}),
                        );
                    }
                }
                Declaration::AuthenticationStrategy(strategy) => {
                    categories.get_mut("transports").unwrap().insert(
                        format!("auth_transport:{}", strategy.name.text),
                        json!({"strategy":strategy.name.text,"tokens":contract_tokens(source,strategy.transport.range)}));
                    for validator in &strategy.validators {
                        let settings: BTreeMap<_, _> = validator
                            .settings
                            .iter()
                            .map(|setting| {
                                (
                                    setting.name.text.clone(),
                                    contract_tokens(source, setting.value.range()),
                                )
                            })
                            .collect();
                        let credentials = validator.credentials.as_ref().map(|binding| json!({
                            "identity":path(&binding.identity),"principal":path(&binding.principal),
                            "verifier":path(&binding.verifier),"active_tokens":contract_tokens(source,binding.active.range()),
                            "expires":path(&binding.expires),"revoked":path(&binding.revoked)
                        }));
                        categories.get_mut("validators").unwrap().insert(
                            format!("auth_validator:{}.{}",strategy.name.text,validator.name.text),
                            json!({"strategy":strategy.name.text,"validator":validator.name.text,
                                "mode":validator.mode.text,"principal_variant":validator.principal.text,
                                "settings":settings,"credentials":credentials}));
                    }
                    for mapping in &strategy.claims {
                        let target = path(&mapping.target);
                        // Checked mappings have unique targets. Changing the
                        // source retains identity; changing target is remove/add.
                        categories.get_mut("claims").unwrap().insert(
                            format!("auth_claim:{}:{target}",strategy.name.text),
                            json!({"strategy":strategy.name.text,"source":mapping.source.text,"target":target}));
                    }
                    for resolution in &strategy.resolutions {
                        let mappings: BTreeMap<_, _> = resolution
                            .mappings
                            .iter()
                            .map(|mapping| (path(&mapping.target), mapping.source.text.clone()))
                            .collect();
                        for (target, field) in &mappings {
                            facts.insert(format!("auth_projection:{}.{}:{target}",strategy.name.text,resolution.principal.text),
                                json!({"strategy":strategy.name.text,"principal_variant":resolution.principal.text,
                                    "authority":path(&resolution.authority),"source_field":field,"target":target}));
                        }
                        categories.get_mut("resolutions").unwrap().insert(
                            format!("auth_resolution:{}.{}",strategy.name.text,resolution.principal.text),
                            json!({"strategy":strategy.name.text,"principal_variant":resolution.principal.text,
                                "authority":path(&resolution.authority),"active_tokens":contract_tokens(source,resolution.active.range()),
                                "mappings":mappings,"inactive_failure":resolution.inactive.text}));
                    }
                }
                _ => {}
            }
        }
    }
    for contracts in categories.values() {
        facts.extend(
            contracts
                .iter()
                .map(|(identity, contract)| (identity.clone(), contract.clone())),
        );
    }
    json!({"status":"compiler_checked_contract_not_live_authority",
        "applications":categories["applications"].values().collect::<Vec<_>>(),
        "variants":categories["variants"].values().collect::<Vec<_>>(),
        "transports":categories["transports"].values().collect::<Vec<_>>(),
        "validators":categories["validators"].values().collect::<Vec<_>>(),
        "claims":categories["claims"].values().collect::<Vec<_>>(),
        "resolutions":categories["resolutions"].values().collect::<Vec<_>>()})
}

fn snapshot(path: &Path, project: &AnalyzedProject) -> Result<Value, String> {
    let model = ArtifactModel::new(path, project);
    let mut facts = BTreeMap::<String, Value>::new();
    let mut sources = BTreeMap::new();
    let mut locations = BTreeMap::new();
    for source in &project.syntax.sources {
        let name = normalized_source(project_root(path), &source.source_name);
        sources.insert(name.clone(), digest(source.source_text.as_bytes()));
        for declaration in &source.file.declarations {
            let id = declaration_id(declaration);
            // Token spelling retains body values even when graph topology is
            // unchanged. Whitespace/comments are not behavioral facts.
            let range = declaration.range();
            let tokens: Vec<_> = source
                .tokens
                .iter()
                .filter(|t| {
                    !t.kind.is_trivia() && t.range.start >= range.start && t.range.end <= range.end
                })
                .map(|t| t.text(&source.source_text))
                .collect();
            facts.insert(id.clone(), json!({"tokens": tokens}));
            locations.insert(
                id,
                json!({"source":name,"start":range.start,"end":range.end}),
            );
        }
    }
    for source in &project.syntax.sources {
        for declaration in &source.file.persistence {
            let tokens: Vec<_> = source
                .tokens
                .iter()
                .filter(|t| {
                    !t.kind.is_trivia()
                        && t.range.start >= declaration.range.start
                        && t.range.end <= declaration.range.end
                })
                .map(|t| t.text(&source.source_text))
                .collect();
            facts.insert(
                format!("persistence:{}", declaration.target.text),
                json!({"tokens":tokens}),
            );
        }
    }
    let policy = audit(model.policy_audit_json());
    // Binding source and scope are independent human decisions, not merely a
    // token change hidden inside an entity declaration or aggregate policy hash.
    let role_bindings: BTreeMap<_, _> = project
        .policy
        .bindings
        .iter()
        .map(|binding| {
            let identity = format!("{}.{}", binding.entity, binding.field);
            let contract = json!({"entity":binding.entity,"field":binding.field,
            "role":binding.role,"scope":binding.scope,"principal_entity":binding.principal});
            facts.insert(format!("role_binding:{identity}"), contract.clone());
            (identity, contract)
        })
        .collect();
    let memberships: BTreeMap<_, _> = project
        .policy
        .memberships
        .iter()
        .map(|membership| {
            let contract = json!({"entity":membership.entity,"scope_field":membership.scope_field,
            "scope":membership.scope,"member_field":membership.member_field,
            "principal_entity":membership.principal,"role_field":membership.role_field,
            "role_type":membership.role_type});
            facts.insert(
                format!("membership:{}", membership.entity),
                contract.clone(),
            );
            (membership.entity.clone(), contract)
        })
        .collect();
    // Individual grants stay separate even when they belong to one entity.
    for entity in &project.policy.entities {
        facts.insert(
            format!("policy_scope:{}", entity.entity),
            json!({"scope":entity.scope,"field":entity.scope_field}),
        );
        for rule in &entity.rules {
            for effect in &rule.effects {
                facts.insert(
                    format!("grant:{}:{}:{}", entity.entity, rule.subject, effect),
                    json!({"entity":entity.entity,"actor":rule.subject,"effect":effect}),
                );
            }
        }
        for field in &entity.fields {
            facts.insert(
                format!("field_policy:{}.{}", entity.entity, field.field),
                json!(field
                    .rules
                    .iter()
                    .map(|r| json!({"actor":r.subject,"effects":r.effects}))
                    .collect::<Vec<_>>()),
            );
        }
    }
    for operation in &project.policy.operations {
        facts.insert(format!("operation_policy:{}", operation.operation), json!({
            "obligations": operation.obligations.iter().map(|o| json!({"entity":o.entity,"effect":o.effect,"actors":o.subjects,"source":o.source})).collect::<Vec<_>>(),
            "restricted_field_reads": operation.field_reads.iter().map(|r| json!({"entity":r.entity,"field":r.field,"actors":r.subjects,"source":r.source})).collect::<Vec<_>>()
        }));
    }
    let nodes: BTreeSet<_> = project
        .semantics
        .nodes
        .iter()
        .map(|n| (n.kind.as_str(), n.name.as_str()))
        .collect();
    let calls: BTreeSet<_> = project
        .semantics
        .calls
        .iter()
        .map(|e| {
            (
                project.semantics.nodes[e.caller.0 as usize].name.clone(),
                project.semantics.nodes[e.callee.0 as usize].name.clone(),
            )
        })
        .collect();
    let refinements: BTreeSet<_> = project
        .semantics
        .refinements
        .iter()
        .map(|e| {
            (
                project.semantics.nodes[e.refined.0 as usize].name.clone(),
                project.semantics.nodes[e.parent.0 as usize].name.clone(),
            )
        })
        .collect();
    let resolutions: BTreeSet<_> = project
        .semantics
        .authentication_resolutions
        .iter()
        .map(|e| {
            (
                project.semantics.nodes[e.resolution.0 as usize]
                    .name
                    .clone(),
                project.semantics.nodes[e.authority.0 as usize].name.clone(),
            )
        })
        .collect();
    let mut graph = json!({"nodes":nodes,"calls":calls,"refinements":refinements,"authentication_resolutions":resolutions});
    if !project.delivery_model().bindings().is_empty() {
        graph["private_delivery_graphs"] = json!(project
            .delivery_model()
            .bindings()
            .iter()
            .map(|binding| binding.private_graph().source_facts())
            .collect::<Vec<_>>());
        for binding in project.delivery_model().bindings() {
            facts.insert(
                format!("delivery_binding:{}", binding.binding_identity()),
                binding.source_facts(),
            );
        }
    }
    facts.insert("semantic_graph".into(), graph.clone());
    let mut effects: BTreeMap<String, BTreeSet<(String, String)>> = BTreeMap::new();
    for source in &project.syntax.sources {
        for declaration in &source.file.declarations {
            match declaration {
                Declaration::Callable(c) => {
                    scan_block(&c.body, effects.entry(c.name.text.clone()).or_default())
                }
                Declaration::Route(r) => {
                    if let Some(inline) = &r.inline_action {
                        scan_block(
                            &inline.body,
                            effects
                                .entry(format!("{} {}", method_name(r.method), r.path))
                                .or_default(),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    // Unknown standard-call effects must not quietly disappear. Currently only
    // the explicitly named temporal namespace has a pure contract.
    for node in &project.semantics.nodes {
        if node.kind == NodeKind::StandardFunction && !pure_standard_function(&node.name) {
            effects
                .entry(node.name.clone())
                .or_default()
                .insert(("unclassified_external_boundary".into(), node.name.clone()));
        }
    }
    // Checked service operations are external effects. Attach each contract to
    // its semantic call-graph node so route/action witnesses expose transitive
    // provider effects instead of leaving them only in a separate audit file.
    for service in &project.semantics.external_effects {
        let operation = format!("{}.{}", service.service, service.operation);
        effects
            .entry(operation.clone())
            .or_default()
            .insert(("external_service".into(), operation));
    }
    let mut paths = BTreeMap::new();
    for node in &project.semantics.nodes {
        if matches!(
            node.kind,
            NodeKind::Route
                | NodeKind::Job
                | NodeKind::Action
                | NodeKind::Query
                | NodeKind::Function
        ) {
            let mut queue = VecDeque::from([vec![node.name.clone()]]);
            let mut visited = BTreeSet::new();
            let mut reachable = BTreeMap::new();
            while let Some(path) = queue.pop_front() {
                let leaf = path.last().expect("path is nonempty");
                if !visited.insert(leaf.clone()) {
                    continue;
                }
                if let Some(found) = effects.get(leaf) {
                    for (kind, target) in found {
                        reachable.insert(format!("{leaf}:{kind}:{target}"), json!({"kind":kind,"target":target,"path":path,"classification":"compiler_checked_may_effect"}));
                    }
                }
                for (caller, callee) in &calls {
                    if caller == leaf {
                        let mut next = path.clone();
                        next.push(callee.clone());
                        queue.push_back(next);
                    }
                }
            }
            // One shortest witness per terminal effect; all direct edges are
            // retained in the graph, including alternate paths and cycles.
            facts.insert(
                format!("reachability:{}", node.name),
                json!({"reachable":visited,"effects":reachable}),
            );
            paths.insert(node.name.clone(), reachable);
        }
    }
    let mut impact = BTreeMap::<&str, Value>::new();
    for (category, kinds) in [
        (
            "actors",
            vec![NodeKind::Principal, NodeKind::PrincipalVariant],
        ),
        ("entities", vec![NodeKind::Entity]),
        ("fields", vec![NodeKind::Field]),
        (
            "relationships",
            vec![NodeKind::Relationship, NodeKind::EntityReference],
        ),
    ] {
        let entries: BTreeSet<_> = project
            .semantics
            .nodes
            .iter()
            .filter(|n| kinds.contains(&n.kind) && n.source != "<prelude>")
            .map(|n| &n.name)
            .collect();
        impact.insert(
            category,
            json!({"status":"compiler_checked_inventory","facts":entries}),
        );
    }
    for (category, data) in [
        ("routes", audit(model.routes_json())),
        ("configuration", audit(model.configuration_audit_json())),
        ("transactions", audit(model.transaction_audit_json())),
        ("disclosure", audit(model.failure_audit_json())),
        ("data", audit(model.entity_audit_json())),
        ("types", audit(model.validator_plan_json())),
    ] {
        facts.insert(format!("audit:{category}"), data.clone());
        impact.insert(
            category,
            json!({"status":"compiler_checked_contract","facts":data}),
        );
    }
    // Retain the existing inventory facts shape; these additional source
    // contracts are not effective role facts for any live authenticated caller.
    impact.get_mut("actors").expect("actor inventory exists")["role_bindings"] =
        json!(role_bindings.values().collect::<Vec<_>>());
    impact.get_mut("actors").expect("actor inventory exists")["memberships"] =
        json!(memberships.values().collect::<Vec<_>>());
    impact.get_mut("actors").expect("actor inventory exists")["binding_evidence"] =
        json!("compiler_checked_contract_not_live_authority");
    impact.get_mut("actors").expect("actor inventory exists")["principal_contracts"] =
        principal_contracts(project, &mut facts);
    let secrets: BTreeSet<_> = model
        .configurations
        .iter()
        .flat_map(|c| {
            c.fields
                .iter()
                .filter(|f| f.secret)
                .map(|f| format!("{}.{}", c.name.text, f.name.text))
        })
        .collect();
    impact.insert(
        "secrets",
        json!({"status":"compiler_checked_bindings_only","facts":secrets}),
    );
    let boundaries: BTreeSet<_> = project
        .semantics
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::AuthenticationStrategy)
        .map(|n| &n.name)
        .collect();
    impact.insert("authentication",json!({"status":"opaque_generated_boundary","strategies":boundaries,"details":audit(model.authentication_audit_json(path))}));
    for category in ["reads", "writes", "external_effects"] {
        let entries: BTreeMap<_, _> = paths
            .iter()
            .map(|(owner, effects)| {
                let selected: BTreeMap<_, _> = effects
                    .iter()
                    .filter(|(_, v)| match category {
                        "reads" => v["kind"] == "read",
                        "writes" => {
                            matches!(v["kind"].as_str(), Some("create" | "update" | "delete"))
                        }
                        _ => matches!(
                            v["kind"].as_str(),
                            Some("external_service" | "unclassified_external_boundary")
                        ),
                    })
                    .collect();
                (owner, selected)
            })
            .collect();
        impact.insert(category,json!({"status":"compiler_checked_may_effects","facts":entries,"opaque_authentication_boundaries":boundaries}));
    }
    let lifecycle_entities = project
        .syntax
        .sources
        .iter()
        .flat_map(|source| {
            source.file.declarations.iter().filter_map(move |declaration| {
                let Declaration::Record(record) = declaration else {
                    return None;
                };
                let lifecycle = record.dossier.as_ref()?.lifecycle.as_ref()?;
                let lifecycle_prefix = format!("{}.lifecycle", record.name.text);
                let policy_prefix = format!("{}.policy", record.name.text);
                let transition_rows = lifecycle
                    .transitions
                    .iter()
                    .map(|transition| {
                        let effect = crate::entity_model::lifecycle_transition_policy_effect(
                            &transition.name.text,
                        );
                        json!({
                            "name": transition.name.text,
                            "source": normalized_source(project_root(path), &source.source_name),
                            "start": transition.range.start,
                            "end": transition.range.end,
                            "policy_effect": effect,
                            "sql_verb": "UPDATE",
                            "guarded_sql": ["identity", "lifecycle_visibility", "transition_from"],
                        })
                    })
                    .collect::<Vec<_>>();
                let lifecycle_nodes = project
                    .semantics
                    .nodes
                    .iter()
                    .filter(|node| {
                        node.source == source.source_name
                            && (node.name == lifecycle_prefix
                                || node.name.starts_with(&format!("{lifecycle_prefix}.")))
                    })
                    .map(|node| {
                        json!({
                            "id": node.id.0,
                            "kind": node.kind.as_str(),
                            "name": node.name,
                            "source": normalized_source(project_root(path), &node.source),
                            "start": node.range.start,
                            "end": node.range.end,
                        })
                    })
                    .collect::<Vec<_>>();
                let policy_nodes = project
                    .semantics
                    .nodes
                    .iter()
                    .filter(|node| {
                        node.source == source.source_name
                            && (node.name == policy_prefix
                                || node.name.starts_with(&format!("{policy_prefix}.")))
                    })
                    .map(|node| {
                        json!({
                            "id": node.id.0,
                            "kind": node.kind.as_str(),
                            "name": node.name,
                            "source": normalized_source(project_root(path), &node.source),
                            "start": node.range.start,
                            "end": node.range.end,
                        })
                    })
                    .collect::<Vec<_>>();
                let generated_fields = record
                    .fields
                    .iter()
                    .filter(|field| field.generated.is_some())
                    .map(|field| {
                        let node_name = format!("{}.{}.generated", record.name.text, field.name.text);
                        let node = project.semantics.nodes.iter().find(|node| {
                            node.name == node_name && node.source == source.source_name
                        });
                        json!({
                            "name": field.name.text,
                            "node_id": node.map(|node| node.id.0),
                            "source": normalized_source(project_root(path), &source.source_name),
                            "start": field.name.range.start,
                            "end": field.name.range.end,
                        })
                    })
                    .collect::<Vec<_>>();
                Some(json!({
                    "entity": record.name.text,
                    "source": normalized_source(project_root(path), &source.source_name),
                    "lifecycle_nodes": lifecycle_nodes,
                    "policy_nodes": policy_nodes,
                    "generated_fields": generated_fields,
                    "owned_fields": lifecycle.initial.as_ref().map(|fields| fields.iter().map(|field| field.name.text.as_str()).collect::<Vec<_>>()).unwrap_or_default(),
                    "transitions": transition_rows,
                    "purge": lifecycle.purge.as_ref().map(|purge| json!({"from": purge.from.text, "start": purge.range.start, "end": purge.range.end})),
                }))
            })
        })
        .collect::<Vec<_>>();
    impact.insert(
        "lifecycles",
        json!({"status":"compiler_checked_contract", "facts": lifecycle_entities}),
    );
    impact.insert(
        "services",
        json!({"status":"compiler_checked_contract", "facts":audit(model.service_audit_json(path))}),
    );
    let mut jobs = audit(model.job_audit_json());
    // Schedule entries are a semantic set, not authored execution order.
    jobs["jobs"]
        .as_array_mut()
        .expect("checked job audit contains an array")
        .sort_by(|a, b| a["job"].as_str().cmp(&b["job"].as_str()));
    impact.insert(
        "jobs",
        json!({"status":"compiler_checked_nonexecuting_contract", "facts":jobs,
            "runtime_conformance":"not_established", "delivery_authority": if project.delivery_model().bindings().is_empty() { "not_resolved" } else { "checked_nonexecuting_phase_obligations" },
            "feasible_job_scenarios":"not_generated"}),
    );
    // Native lowering qualification belongs to the complete checked snapshot,
    // never eager route-only metadata construction on an unresolved model.
    let deployment = crate::target::deployment_surface_metadata(path, project);
    if let Some(deployment) = deployment["facts"].as_object() {
        for key in ["listener", "readiness", "application_plane"] {
            facts.insert(format!("deployment_surface:{key}"), deployment[key].clone());
        }
    }
    impact.insert("deployment", deployment);
    for category in ["emissions"] {
        impact.insert(
            category,
            json!({"status":"unsupported_analysis","facts":null}),
        );
    }
    impact.insert("retries",json!({"status":"compiler_checked_contract_only","facts":audit(model.transaction_audit_json()),"runtime_conformance":"not_established"}));
    let actor_sources = actor_links::derive(project, &mut facts);
    impact.insert("actor_sources", actor_sources);
    let candidates = admission_candidates::derive(path, project, &impact["deployment"]);
    impact.insert("anonymous_admission_candidates", candidates);
    impact.insert(
        "field_source_contracts",
        field_contracts::derive(project, &mut facts),
    );
    // Include every impact in the comparison, including auth/config boundaries.
    for (category, value) in &impact {
        facts.insert(format!("impact:{category}"), value.clone());
    }
    let registry = crate::migration_identity::checked_schema_registry_source(path, project)
        .map_err(|error| format!("Approval registry validation failed: {}", error.code))?;
    let registry = match registry {
        Some(bytes) => {
            json!({"status":"compiler_checked_registry", "path":"schema.identities.json",
            "byte_digest":digest(bytes.as_bytes()), "facts":audit(bytes)})
        }
        None => {
            json!({"status":"absent", "path":"schema.identities.json", "byte_digest":null,"facts":null})
        }
    };
    facts.insert("provenance:schema_registry".into(), registry.clone());
    let generated_artifacts = super::artifact_provenance::derive(path, project)?;
    for category in ["metadata_and_audit", "target"] {
        for (path, descriptor) in generated_artifacts[category]["files"].as_object().unwrap() {
            facts.insert(format!("artifact_pin:{path}"), descriptor.clone());
        }
    }
    facts.insert(
        "artifact_target_generation".into(),
        generated_artifacts["target"]["result"].clone(),
    );
    facts.insert(
        "provenance:generated_artifacts".into(),
        generated_artifacts.clone(),
    );
    let compiler: Value = serde_json::from_str(include_str!(concat!(
        env!("OUT_DIR"),
        "/approval-compiler-inputs.json"
    )))
    .expect("build-time compiler provenance is JSON");
    let mut snapshot = json!({"policy_digest":digest(canonical_bytes(&policy).as_bytes()),
        "semantic_graph_digest":digest(canonical_bytes(&graph).as_bytes()),
        "behavior_digest":digest(canonical_bytes(&json!(facts)).as_bytes()),
        "source_digest":digest(canonical_bytes(&json!(sources)).as_bytes()),
        "provenance":{"compiler":compiler,"schema_registry":registry,"generated_artifacts":generated_artifacts,
            "compiler_version":env!("CARGO_PKG_VERSION"),"policy_schema_version":1,
            "approval_schema_version":SCHEMA,"canonicalization":CANONICALIZATION},
        "sources":sources,"locations":locations,"facts":facts,"graph":graph,"policy":policy,"impact":impact,"effect_paths":paths});
    // The state digest binds the complete snapshot before its own field exists.
    snapshot["state_digest"] = digest(canonical_bytes(&snapshot).as_bytes()).into();
    Ok(snapshot)
}

fn pure_standard_function(name: &str) -> bool {
    matches!(
        name,
        "temporal.in_zone"
            | "temporal.resolve"
            | "temporal.add_elapsed"
            | "temporal.between"
            | "temporal.add_days"
            | "temporal.add_weeks"
            | "temporal.add_months"
            | "temporal.add_years"
            | "temporal.add_local_days"
            | "temporal.add_local_weeks"
            | "temporal.add_local_months"
            | "temporal.add_local_years"
            | "temporal.day_bounds"
            | "temporal.week_bounds"
            | "temporal.month_bounds"
            | "temporal.year_bounds"
            | "temporal.calendar_date"
            | "temporal.year"
            | "temporal.month"
            | "temporal.day"
            | "temporal.weekday"
            | "temporal.hour"
            | "temporal.minute"
            | "temporal.second"
            | "temporal.millisecond"
            | "temporal.offset"
            | "temporal.zone"
            | "temporal.same_zone"
            | "temporal.same_local"
            | "temporal.format"
            | "temporal.format_friendly"
    )
}

fn declaration_id(d: &Declaration) -> String {
    let (kind, name) = match d {
        Declaration::Application(x) => ("application", x.name.text.clone()),
        Declaration::Locales(_) => ("locales", "locales".into()),
        Declaration::AuthenticationStrategy(x) => ("authentication", x.name.text.clone()),
        Declaration::Principal(x) => ("principal", x.name.text.clone()),
        Declaration::Config(x) => ("config", x.name.text.clone()),
        Declaration::Type(x) => ("type", x.name.text.clone()),
        Declaration::Enum(x) => ("enum", x.name.text.clone()),
        Declaration::Record(x) => ("record", x.name.text.clone()),
        Declaration::Failure(x) => ("failure", x.name.text.clone()),
        Declaration::Callable(x) => ("callable", x.name.text.clone()),
        Declaration::Fixture(x) => ("fixture", x.name.text.clone()),
        Declaration::Test(x) => ("test", x.name.text.clone()),
        Declaration::Route(x) => ("route", format!("{} {}", method_name(x.method), x.path)),
        Declaration::Job(x) => ("job", x.name.text.clone()),
    };
    format!("{kind}:{name}")
}

fn decisions(before: Option<&Value>, after: &Value) -> Vec<Value> {
    let old = before.and_then(|b| b["facts"].as_object());
    let new = after["facts"].as_object().expect("facts object");
    let keys: BTreeSet<_> = old
        .into_iter()
        .flat_map(|o| o.keys())
        .chain(new.keys())
        .collect();
    keys.into_iter().filter_map(|id| {
        let previous = old.and_then(|o|o.get(id));
        let proposed = new.get(id);
        if previous == proposed { return None; }
        Some(json!({"id":digest(format!("decision.v3:{id}").as_bytes()), "subject":id,
            "change":if previous.is_none(){"added"}else if proposed.is_none(){"removed"}else{"changed"},
            "before":previous,"after":proposed,
            "reason":"Changed behavior or authority needs review; weakening classification is not proved.",
            "alternatives":["retain_before","accept_exact_after","request_clarification"],
            "counterexamples":{"status":"not_generated","facts":null},"approval":null}))
    }).collect()
}

fn effect_scenarios(before: Option<&Value>, after: &Value) -> Vec<Value> {
    let after_paths = after["effect_paths"]
        .as_object()
        .expect("snapshot effect paths are an object");
    let before_paths = before.and_then(|snapshot| snapshot["effect_paths"].as_object());
    let routes: BTreeSet<_> = before_paths
        .into_iter()
        .flat_map(|paths| paths.keys())
        .chain(after_paths.keys())
        .filter(|owner| is_http_route(owner))
        .collect();
    let mut scenarios = Vec::new();
    for route in routes {
        let before_auth = before.and_then(|snapshot| route_auth_mode(snapshot, route));
        let after_auth = route_auth_mode(after, route);
        let route_auth_changed =
            before_auth.is_some() && after_auth.is_some() && before_auth != after_auth;
        let before_policy = before.and_then(|snapshot| route_policy_contract(snapshot, route));
        let after_policy = route_policy_contract(after, route);
        let route_policy_changed = before.is_some() && before_policy != after_policy;
        let before_sources = before.and_then(|snapshot| {
            snapshot["facts"].get(format!("route_actor_sources:{route}").as_str())
        });
        let after_sources = after["facts"].get(format!("route_actor_sources:{route}").as_str());
        let route_sources_changed = before.is_some() && before_sources != after_sources;
        let route_actor_constraints_changed =
            route_auth_changed || route_policy_changed || route_sources_changed;
        let old = before_paths
            .and_then(|paths| paths.get(route))
            .and_then(Value::as_object);
        let new = after_paths.get(route).and_then(Value::as_object);
        let effects: BTreeSet<_> = old
            .into_iter()
            .flat_map(|effects| effects.keys())
            .chain(new.into_iter().flat_map(|effects| effects.keys()))
            .collect();
        if effects.is_empty()
            && (before.is_none()
                || before_auth.is_none() != after_auth.is_none()
                || route_actor_constraints_changed)
        {
            scenarios.push(json!({
                "id":digest(canonical_bytes(&json!(["scenario.admission.v1",route])).as_bytes()),
                "kind":"route_admission","route":route,"effect":null,
                "status":if before.is_none() { "after_only_without_baseline" }
                    else if before_auth.is_none() { "route_added_admission_contract" }
                    else if after_auth.is_none() { "route_removed_admission_contract" }
                    else { "route_actor_constraints_changed_admission_contract" },
                "certainty":"compiler_checked_route_contract","execution":"not_proved","feasibility":"not_analyzed",
                "before":before_sources,"after":after_sources,
                "actor_access":{"before_route_auth":before_auth,"after_route_auth":after_auth},
                "actor_policy":{"before":before_policy,"after":after_policy},
                "counterexample":admission_candidates::compare(before, after, route)
                    .unwrap_or_else(||json!({"status":"not_generated","facts":null}))
            }));
        }
        for effect_id in effects {
            let previous = old.and_then(|effects| effects.get(effect_id));
            let proposed = new.and_then(|effects| effects.get(effect_id));
            if previous == proposed && !route_actor_constraints_changed {
                continue;
            }
            let status = match (previous.is_some(), proposed.is_some(), before.is_some()) {
                (false, true, true) => "newly_reachable_may_effect",
                (true, false, _) => "no_longer_reachable_may_effect",
                (true, true, true) if route_actor_constraints_changed => {
                    "route_actor_constraints_changed_may_effect"
                }
                (true, true, _) => "may_effect_witness_changed",
                (false, true, false) => "after_only_without_baseline",
                (false, false, _) => continue,
            };
            let selected = proposed.or(previous).expect("changed effect is present");
            let public_caller_before = before_auth == Some("none");
            let public_caller_after = after_auth == Some("none");
            let public_caller_scope_changed =
                route_auth_changed && public_caller_before != public_caller_after;
            scenarios.push(json!({
                "id": digest(format!("scenario.v3:{route}:{effect_id}").as_bytes()),
                "route": route,
                "effect": selected,
                "status": status,
                "certainty": "compiler_checked_may_call",
                "execution": "not_proved",
                "feasibility": "not_analyzed",
                "actor_access": {
                    "status": if route_auth_changed { "route_authentication_changed" } else { "unchanged_or_unavailable" },
                    "before_route_auth": before_auth,
                    "after_route_auth": after_auth,
                    "unauthenticated_caller_before": public_caller_before,
                    "unauthenticated_caller_after": public_caller_after
                },
                "actor_policy": {
                    "status": if route_policy_changed { "route_policy_changed" } else if after_policy.is_some() { "compiler_checked_route_policy" } else { "not_available" },
                    "before": before_policy,
                    "after": after_policy,
                    "subject_changes": route_policy_subject_changes(before_policy, after_policy)
                },
                "actor_sources":{"before":before_sources,"after":after_sources,"changed":route_sources_changed},
                "before": previous,
                "after": proposed,
                "counterexample": if public_caller_scope_changed {
                    json!({"status":"actor_scope_candidate_not_feasibility_proof", "facts": {
                        "actor":"unauthenticated_public_caller",
                        "route_auth_before":before_auth,
                        "route_auth_after":after_auth
                    }})
                } else {
                    json!({"status":"not_generated", "facts":null})
                }
            }));
        }
    }
    scenarios
}

fn route_policy_contract<'a>(snapshot: &'a Value, route: &str) -> Option<&'a Value> {
    let (method, path) = route.split_once(' ')?;
    let key = format!("operation_policy:route:{method}:{path}");
    snapshot["facts"].get(&key)
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum PolicySubjectSurface {
    OperationObligation {
        entity: String,
        effect: String,
        source: String,
    },
    RestrictedFieldRead {
        entity: String,
        field: String,
    },
}

impl PolicySubjectSurface {
    fn as_json(&self) -> Value {
        match self {
            Self::OperationObligation {
                entity,
                effect,
                source,
            } => json!({
                "kind":"operation_obligation",
                "entity":entity,
                "effect":effect,
                "source":source
            }),
            Self::RestrictedFieldRead { entity, field } => json!({
                "kind":"restricted_field_read",
                "entity":entity,
                "field":field
            }),
        }
    }
}

fn route_policy_subjects(
    contract: Option<&Value>,
) -> BTreeMap<PolicySubjectSurface, BTreeSet<String>> {
    let mut subjects = BTreeMap::<PolicySubjectSurface, BTreeSet<String>>::new();
    let Some(contract) = contract else {
        return subjects;
    };
    if let Some(obligations) = contract["obligations"].as_array() {
        for obligation in obligations {
            let (Some(entity), Some(effect), Some(source)) = (
                obligation["entity"].as_str(),
                obligation["effect"].as_str(),
                obligation["source"].as_str(),
            ) else {
                continue;
            };
            let selected = subjects
                .entry(PolicySubjectSurface::OperationObligation {
                    entity: entity.to_owned(),
                    effect: effect.to_owned(),
                    source: source.to_owned(),
                })
                .or_default();
            if let Some(actors) = obligation["actors"].as_array() {
                selected.extend(actors.iter().filter_map(Value::as_str).map(str::to_owned));
            }
        }
    }
    if let Some(reads) = contract["restricted_field_reads"].as_array() {
        for read in reads {
            let (Some(entity), Some(field)) = (read["entity"].as_str(), read["field"].as_str())
            else {
                continue;
            };
            let selected = subjects
                .entry(PolicySubjectSurface::RestrictedFieldRead {
                    entity: entity.to_owned(),
                    field: field.to_owned(),
                })
                .or_default();
            if let Some(actors) = read["actors"].as_array() {
                selected.extend(actors.iter().filter_map(Value::as_str).map(str::to_owned));
            }
        }
    }
    subjects
}

fn route_policy_subject_changes(before: Option<&Value>, after: Option<&Value>) -> Value {
    let before = route_policy_subjects(before);
    let after = route_policy_subjects(after);
    let surfaces: BTreeSet<_> = before.keys().chain(after.keys()).cloned().collect();
    let mut added = Vec::new();
    let mut removed = Vec::new();
    for surface in surfaces {
        let before_subjects = before.get(&surface);
        let after_subjects = after.get(&surface);
        let surface_json = surface.as_json();
        for actor in after_subjects
            .into_iter()
            .flat_map(|subjects| subjects.iter())
            .filter(|actor| !before_subjects.is_some_and(|subjects| subjects.contains(*actor)))
        {
            added.push(json!({"surface":surface_json,"actor_subject":actor}));
        }
        for actor in before_subjects
            .into_iter()
            .flat_map(|subjects| subjects.iter())
            .filter(|actor| !after_subjects.is_some_and(|subjects| subjects.contains(*actor)))
        {
            removed.push(json!({"surface":surface_json,"actor_subject":actor}));
        }
    }
    json!({
        "status":"compiler_checked_subject_delta_not_principal_or_feasibility_proof",
        "added":added,
        "removed":removed
    })
}

fn route_auth_mode<'a>(snapshot: &'a Value, route: &str) -> Option<&'a str> {
    snapshot["facts"]["audit:routes"]["routes"]
        .as_array()?
        .iter()
        .find(|entry| entry["route"] == route)?["auth"]
        .as_str()
}

fn is_http_route(owner: &str) -> bool {
    let Some((method, path)) = owner.split_once(' ') else {
        return false;
    };
    matches!(method, "GET" | "POST" | "PUT" | "PATCH" | "DELETE") && path.starts_with('/')
}

fn name(n: &NameExpression) -> String {
    n.path
        .iter()
        .map(|n| n.text.as_str())
        .collect::<Vec<_>>()
        .join(".")
}
type Effects = BTreeSet<(String, String)>;
fn scan_block(block: &Block, out: &mut Effects) {
    for s in &block.statements {
        match s {
            Statement::Binding(s) => scan_expr(&s.value, out),
            Statement::Assignment(s) => scan_expr(&s.value, out),
            Statement::Return(s) => scan_expr(&s.value, out),
            Statement::Reject(s) => scan_fields(&s.values, out),
            Statement::If(s) => {
                scan_expr(&s.condition, out);
                scan_block(&s.then_block, out);
                if let Some(b) = &s.else_block {
                    scan_block(b, out);
                }
            }
            Statement::Match(s) => {
                scan_expr(&s.subject, out);
                for a in &s.arms {
                    scan_block(&a.body, out);
                }
            }
            Statement::Assert(s) => scan_expr(&s.condition, out),
            Statement::AdvanceClock(s) => scan_expr(&s.duration, out),
            Statement::Unsupported(s) => {
                out.insert(("unclassified_external_boundary".into(), s.keyword.clone()));
            }
        }
    }
}
fn scan_fields(fields: &[FieldInitialiser], out: &mut Effects) {
    for f in fields {
        scan_expr(&f.value, out);
    }
}
fn scan_conflicts(conflicts: &[ConflictBinding], out: &mut Effects) {
    for c in conflicts {
        scan_fields(&c.rejection.values, out);
    }
}
fn scan_invocation(e: &InvocationExpression, out: &mut Effects) {
    for a in &e.arguments {
        scan_expr(a, out);
    }
    scan_fields(&e.named_arguments, out);
}
fn scan_expr(e: &Expression, out: &mut Effects) {
    match e {
        Expression::Create(e) => {
            out.insert(("create".into(), name(&e.target)));
            scan_fields(&e.fields, out);
            scan_conflicts(&e.conflicts, out);
        }
        Expression::Query(e) => {
            out.insert(("read".into(), name(&e.target)));
            scan_expr(&e.value, out);
            if let Some(p) = &e.pagination {
                scan_expr(&p.limit, out);
                scan_expr(&p.offset, out);
            }
            if let Some(p) = &e.page {
                for pred in &p.predicates {
                    scan_expr(&pred.value, out);
                }
                scan_expr(&p.after, out);
                scan_expr(&p.limit, out);
            }
            for i in &e.includes {
                out.insert((
                    "read".into(),
                    format!("{}.{}", name(&e.target), i.relationship.text),
                ));
                if let Some(n) = &i.nested_relationship {
                    out.insert((
                        "read".into(),
                        format!("{}.{}.{}", name(&e.target), i.relationship.text, n.text),
                    ));
                }
                scan_expr(&i.pagination.limit, out);
                scan_expr(&i.pagination.offset, out);
            }
            if let Some(r) = &e.missing {
                scan_fields(&r.values, out);
            }
        }
        Expression::Update(e) => {
            out.insert(("update".into(), name(&e.target)));
            scan_expr(&e.value, out);
            scan_fields(&e.changes, out);
            for c in &e.conditional_changes {
                scan_expr(&c.change.value, out);
            }
            scan_fields(&e.missing.values, out);
            if let Some(r) = &e.empty {
                scan_fields(&r.values, out);
            }
            scan_conflicts(&e.conflicts, out);
        }
        Expression::Delete(e) => {
            out.insert(("delete".into(), name(&e.target)));
            scan_expr(&e.value, out);
            scan_fields(&e.missing.values, out);
            scan_conflicts(&e.conflicts, out);
        }
        Expression::Invocation(e) => scan_invocation(e, out),
        Expression::TestCall(e) => scan_invocation(&e.invocation, out),
        Expression::Object(e) => scan_fields(&e.fields, out),
        Expression::Construction(e) => scan_fields(&e.fields, out),
        Expression::Attempt(e) => scan_expr(&e.value, out),
        Expression::Unary(e) => scan_expr(&e.value, out),
        Expression::Grouped(e) => scan_expr(&e.value, out),
        Expression::Binary(e) => {
            scan_expr(&e.left, out);
            scan_expr(&e.right, out);
        }
        Expression::OutcomeMatch(e) => {
            scan_expr(&e.subject, out);
            for a in &e.arms {
                match &a.body {
                    OutcomeMatchArmBody::Value(e) => scan_expr(e, out),
                    OutcomeMatchArmBody::Reject(r) => scan_fields(&r.values, out),
                    OutcomeMatchArmBody::Propagate(_) => {}
                }
            }
        }
        Expression::Name(_) | Expression::Literal(_) => {}
        Expression::Missing(_) => {
            out.insert((
                "unclassified_external_boundary".into(),
                "missing_expression".into(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::route_policy_subject_changes;
    use serde_json::json;

    #[test]
    fn restricted_field_subject_changes_keep_the_field_surface() {
        let before = json!({
            "restricted_field_reads":[
                {"entity":"User","field":"email","actors":["Access.public","UserRole.admin"]},
                {"entity":"User","field":"id","actors":["Access.public"]}
            ]
        });
        let after = json!({
            "restricted_field_reads":[
                {"entity":"User","field":"email","actors":["Access.authenticated","UserRole.admin"]},
                {"entity":"User","field":"id","actors":["Access.public"]}
            ]
        });

        let changes = route_policy_subject_changes(Some(&before), Some(&after));
        assert_eq!(changes["added"].as_array().unwrap().len(), 1);
        assert_eq!(changes["removed"].as_array().unwrap().len(), 1);
        assert_eq!(
            changes["added"][0]["surface"],
            json!({"kind":"restricted_field_read","entity":"User","field":"email"})
        );
        assert_eq!(changes["added"][0]["actor_subject"], "Access.authenticated");
        assert_eq!(changes["removed"][0]["actor_subject"], "Access.public");
    }
}
