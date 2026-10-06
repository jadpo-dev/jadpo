use crate::{checked_source_revision, AnalyzedProject};
use jadpo_diagnostics::{catalogue_manifest_json, catalogue_reference_markdown, Diagnostic};
use jadpo_semantic::checked_manifest_json;
use jadpo_syntax::{
    CallableKind, ConfigDeclaration, Constraint, ConstraintKind, Declaration, EnumDeclaration,
    FailureDeclaration, FieldDeclaration, HttpMethod, LiteralKind, PersistenceModifier,
    RecordDeclaration, RecordKind, RouteSuccess, TypeDeclaration, TypeReference,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

mod approval;
mod artifact_provenance;
mod http_boundary_failures;
pub(crate) mod operational_failures;
pub use approval::{
    approval_text, derive_approval_subject, derive_approval_subject_with_state_pin,
    derive_protected_attempt, validate_approval_export, validate_protected_attempt_export,
};

static OUTPUT_REVISION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedArtifact {
    pub relative_path: &'static str,
    pub contents: String,
}

pub fn derive_artifacts(project_path: &Path, project: &AnalyzedProject) -> Vec<GeneratedArtifact> {
    let subject = derive_approval_subject(project_path, project, None, None, None)
        .expect("artifact generation requires a checked project");
    let mut outputs = derive_non_approval_artifacts(project_path, project);
    // Preserve the established build order while sharing the exact no-approval
    // producer with fresh output pinning. Approval cannot hash itself.
    outputs.insert(8, artifact("approval/subject.json", subject.clone()));
    outputs.insert(9, artifact("approval/subject.txt", approval_text(&subject)));
    outputs
}

fn derive_non_approval_artifacts(
    project_path: &Path,
    project: &AnalyzedProject,
) -> Vec<GeneratedArtifact> {
    let model = ArtifactModel::new(project_path, project);
    let metadata = normalized_manifest(project_path, project);
    let mut outputs = vec![
        artifact("app.meta.json", metadata),
        artifact("inventory/routes.json", model.routes_json()),
        artifact("inventory/callables.json", model.callables_json()),
        artifact("audit/failures.json", model.failure_audit_json()),
        artifact("audit/entities.json", model.entity_audit_json()),
        artifact("audit/transactions.json", model.transaction_audit_json()),
        artifact("audit/configuration.json", model.configuration_audit_json()),
        artifact("audit/policy.json", model.policy_audit_json()),
        artifact("validators/plan.json", model.validator_plan_json()),
        artifact(
            "compatibility/public-failure-codes.json",
            model.public_failure_compatibility_json(),
        ),
        artifact("openapi/openapi.json", model.openapi_json()),
        artifact("diagnostics/catalogue.json", catalogue_manifest_json()),
        artifact("diagnostics/reference.md", catalogue_reference_markdown()),
    ];
    if project.syntax.sources.iter().any(|s| {
        s.file
            .declarations
            .iter()
            .any(|d| matches!(d, Declaration::AuthenticationStrategy(_)))
    }) {
        outputs.push(artifact(
            "audit/authentication.json",
            model.authentication_audit_json(project_path),
        ));
    }
    if !project.semantics.external_effects.is_empty() {
        outputs.push(artifact(
            "audit/services.json",
            model.service_audit_json(project_path),
        ));
    }
    if !project.failures.jobs.is_empty() {
        outputs.push(artifact("audit/jobs.json", model.job_audit_json()));
    }
    outputs
}

fn normalized_manifest(project_path: &Path, project: &AnalyzedProject) -> String {
    let project_root = project_root(project_path);
    let graph_json = normalized_graph_json(project_path, project);
    let operations = project
        .syntax
        .sources
        .iter()
        .flat_map(|source| {
            source.file.declarations.iter().filter_map(|declaration| {
                let Declaration::Route(route) = declaration else {
                    return None;
                };
                Some(format!(
                    "{{\"id\":{},\"source\":{},\"range\":{{\"start\":{},\"end\":{}}},\"route\":{}}}",
                    json_string(&format!("route:{}:{}", method_name(route.method), route.path)),
                    json_string(&normalized_source(project_root, &source.source_name)),
                    route.range.start,
                    route.range.end,
                    json_string(&format!("{} {}", method_name(route.method), route.path)),
                ))
            })
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema_version\":2,\"source_revision\":{},\"operations\":[{operations}],\"semantic_graph\":{graph_json}}}",
        json_string(&checked_source_revision(project_path, project))
    )
}

pub(super) fn project_root(project_path: &Path) -> &Path {
    if project_path.is_dir() {
        project_path
    } else {
        project_path.parent().unwrap_or_else(|| Path::new("."))
    }
}

fn normalized_graph_json(project_path: &Path, project: &AnalyzedProject) -> String {
    let project_root = project_root(project_path);
    let mut graph = project.semantics.clone();
    for node in &mut graph.nodes {
        node.source = normalized_source(project_root, &node.source);
    }
    for effect in &mut graph.external_effects {
        effect.source = normalized_source(project_root, &effect.source);
    }
    let mut typing = project.typing.clone();
    for expression in &mut typing.expressions {
        expression.source = normalized_source(project_root, &expression.source);
    }
    for read in &mut typing.clock_reads {
        read.source = normalized_source(project_root, &read.source);
    }
    let manifest = checked_manifest_json(&graph, &typing, &project.failures);
    if project.delivery_model().bindings().is_empty() {
        return manifest;
    }
    let mut manifest: serde_json::Value =
        serde_json::from_str(&manifest).expect("checked manifest JSON");
    manifest["delivery_bindings"] = serde_json::json!(project
        .delivery_model()
        .bindings()
        .iter()
        .map(|binding| binding.source_facts())
        .collect::<Vec<_>>());
    manifest.to_string()
}

pub(super) fn normalized_source(project_root: &Path, source: &str) -> String {
    if source.starts_with('<') {
        return source.to_owned();
    }
    Path::new(source)
        .strip_prefix(project_root)
        .unwrap_or_else(|_| Path::new(source))
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn write_artifacts(
    project_path: &Path,
    artifacts: &[GeneratedArtifact],
) -> Result<PathBuf, Diagnostic> {
    let project_root = if project_path.is_dir() {
        project_path
    } else {
        project_path.parent().unwrap_or_else(|| Path::new("."))
    };
    let output_root = project_root.join("build");
    let revision = OUTPUT_REVISION.fetch_add(1, Ordering::Relaxed);
    let suffix = format!("{}.{}", std::process::id(), revision);
    let staging_root = project_root.join(format!(".jadpo-build-stage.{suffix}"));
    let backup_root = project_root.join(format!(".jadpo-build-backup.{suffix}"));

    fs::create_dir(&staging_root)
        .map_err(|_error| Diagnostic::error("JADPO_ARTIFACT_STAGE_FAILED"))?;

    for artifact in artifacts {
        let destination = staging_root.join(artifact.relative_path);
        if let Some(parent) = destination.parent() {
            if let Err(_error) = fs::create_dir_all(parent) {
                let _ = fs::remove_dir_all(&staging_root);
                return Err(Diagnostic::error("JADPO_ARTIFACT_WRITE_FAILED"));
            }
        }
        if let Err(_error) = fs::write(&destination, &artifact.contents) {
            let _ = fs::remove_dir_all(&staging_root);
            return Err(Diagnostic::error("JADPO_ARTIFACT_WRITE_FAILED"));
        }
    }

    let had_previous = output_root.exists();
    if had_previous {
        if let Err(_error) = fs::rename(&output_root, &backup_root) {
            let _ = fs::remove_dir_all(&staging_root);
            return Err(Diagnostic::error("JADPO_ARTIFACT_PROMOTE_FAILED"));
        }
    }

    if let Err(_error) = fs::rename(&staging_root, &output_root) {
        if had_previous {
            let _ = fs::rename(&backup_root, &output_root);
        }
        let _ = fs::remove_dir_all(&staging_root);
        return Err(Diagnostic::error("JADPO_ARTIFACT_PROMOTE_FAILED"));
    }

    if had_previous {
        fs::remove_dir_all(&backup_root)
            .map_err(|_error| Diagnostic::error("JADPO_ARTIFACT_CLEANUP_FAILED"))?;
    }

    Ok(output_root)
}

fn artifact(relative_path: &'static str, mut contents: String) -> GeneratedArtifact {
    contents.push('\n');
    GeneratedArtifact {
        relative_path,
        contents,
    }
}

struct ArtifactModel<'project> {
    project: &'project AnalyzedProject,
    types: BTreeMap<String, &'project TypeDeclaration>,
    enums: BTreeMap<String, &'project EnumDeclaration>,
    records: BTreeMap<String, &'project RecordDeclaration>,
    fields: BTreeMap<String, &'project FieldDeclaration>,
    failure_declarations: BTreeMap<String, &'project FailureDeclaration>,
    routes: Vec<RouteModel>,
    callables: Vec<CallableModel>,
    configurations: Vec<&'project ConfigDeclaration>,
    relationships: operational_failures::RelationshipAnalysis,
    http_boundaries: http_boundary_failures::Analysis,
}

#[derive(Clone)]
struct RouteModel {
    key: String,
    method: &'static str,
    path: String,
    public: bool,
    fresh_authority: bool,
    path_fields: Vec<(String, String)>,
    query: Option<String>,
    headers: Vec<(String, String, String, bool)>,
    input: Option<String>,
    output: Option<String>,
    success: RouteSuccess,
    deadline_ms: Option<u64>,
    callable: Option<String>,
    behavior: &'static str,
}

#[derive(Clone)]
struct CallableModel {
    name: String,
    kind: &'static str,
    parameters: Vec<(String, String)>,
    output: String,
    failures: Vec<String>,
    may_suspend: bool,
}

impl<'project> ArtifactModel<'project> {
    fn has_persistence_entities(&self) -> bool {
        // Match the native target's emitted persistence boundary, including
        // checked entities without the optional normalized dossier model.
        self.records
            .values()
            .any(|record| record.is_persistent_entity())
    }

    fn job_audit_json(&self) -> String {
        let jobs = self.project.syntax.sources.iter().flat_map(|source| {
            source.file.declarations.iter().filter(|declaration| matches!(declaration, Declaration::Job(_))).map(|declaration| {
                let Declaration::Job(job) = declaration else { unreachable!() };
                // Explicit union: delivery never enters ordinary typing.jobs.
                let delivery = self.project.delivery_model().bindings().iter().find(|entry| entry.schedule().job == job.name.text);
                let contract = match delivery {
                    Some(delivery) => delivery.run_failure_contract(),
                    None => self.project.failures.jobs.iter().find(|entry| entry.job == job.name.text).expect("checked ordinary job failure completeness"),
                };
                let binding = match delivery {
                    Some(delivery) => delivery.schedule(),
                    None => self.project.typing.jobs.iter().find(|entry| entry.job == job.name.text).expect("checked ordinary job completeness"),
                };
                let mut reachable = BTreeSet::from([job.name.text.clone()]);
                loop {
                    let old_len = reachable.len();
                    for edge in &self.project.semantics.calls {
                        let caller = &self.project.semantics.nodes[edge.caller.0 as usize].name;
                        if reachable.contains(caller) {
                            reachable.insert(self.project.semantics.nodes[edge.callee.0 as usize].name.clone());
                        }
                    }
                    if old_len == reachable.len() { break; }
                }
                let effects = self.project.semantics.external_effects.iter().filter_map(|effect| {
                    let name = format!("{}.{}", effect.service, effect.operation);
                    reachable.contains(&name).then_some(name)
                }).collect::<Vec<_>>();
                let mut row = serde_json::json!({
                    "job": job.name.text, "interval_ms": binding.interval_ms,
                    "concurrency": "singleton", "retry_wakeup": "next_schedule",
                    "run": contract.callee, "argument": {"constructor": binding.snapshot_type, "value": "clock.now"}, "output": "Unit",
                    "constructor_validation_proof": binding.constructor_proof,
                    "failures": contract.failures, "may_suspend": contract.may_suspend,
                    "reachable_names": reachable, "external_service_effects": effects,
                    "effect_evidence": "static_may_call_not_execution_or_authority",
                    "durable_failure_dispositions": "pending_checked_worker_binding",
                    "execution_profile": null, "runtime_lowering_supported": false
                });
                if let Some(delivery) = delivery {
                    row["delivery"] = delivery.source_facts();
                    row["delivery_service_effects"] = serde_json::json!([delivery.source_facts()["service_contract"]["operation"]]);
                    // Static source/implementation support is distinct from a
                    // successful target build, live host authority or runtime tests.
                    let source_bound = delivery.matches_source_job(job);
                    let lowering_supported = source_bound && row["delivery"]["runtime_lowering_supported"] == true;
                    row["binding_matches_source_job"] = serde_json::json!(source_bound);
                    row["runtime_lowering_supported"] = serde_json::json!(lowering_supported);
                    row["runtime_conformance"] = serde_json::json!("not_certified_by_static_audit");
                    if lowering_supported {
                        row["durable_failure_dispositions"] = serde_json::json!("emitted_checked_delivery_protocol");
                    }
                }
                row
            })
        }).collect::<Vec<_>>();
        serde_json::json!({"schema_version": if self.project.delivery_model().bindings().is_empty() { 1 } else { 2 }, "scope": "checked_nonexecuting_schedule_entries", "jobs": jobs}).to_string()
    }

    fn new(project_path: &Path, project: &'project AnalyzedProject) -> Self {
        let mut types = BTreeMap::new();
        let mut enums = BTreeMap::new();
        let mut records = BTreeMap::new();
        let mut fields = BTreeMap::new();
        let mut failure_declarations = BTreeMap::new();
        let mut routes = Vec::new();
        let mut callables = Vec::new();
        let mut configurations = Vec::new();

        for source in &project.syntax.sources {
            for declaration in &source.file.declarations {
                match declaration {
                    Declaration::Config(declaration) => configurations.push(declaration),
                    Declaration::Type(declaration) => {
                        types.insert(declaration.name.text.clone(), declaration);
                    }
                    Declaration::Enum(declaration) => {
                        for variant in &declaration.variants {
                            for field in &variant.fields {
                                fields.insert(
                                    format!(
                                        "{}.{}.{}",
                                        declaration.name.text, variant.name.text, field.name.text
                                    ),
                                    field,
                                );
                            }
                        }
                        enums.insert(declaration.name.text.clone(), declaration);
                    }
                    Declaration::Record(declaration) => {
                        for field in &declaration.fields {
                            fields.insert(
                                format!("{}.{}", declaration.name.text, field.name.text),
                                field,
                            );
                        }
                        records.insert(declaration.name.text.clone(), declaration);
                    }
                    Declaration::Callable(declaration) => {
                        let mut failures = declaration
                            .failures
                            .iter()
                            .map(|failure| failure.text.clone())
                            .collect::<Vec<_>>();
                        failures.sort();
                        callables.push(CallableModel {
                            name: declaration.name.text.clone(),
                            kind: match declaration.kind {
                                CallableKind::Function => "function",
                                CallableKind::Action => "action",
                                CallableKind::Query => "query",
                            },
                            parameters: declaration
                                .parameters
                                .iter()
                                .map(|parameter| {
                                    (
                                        parameter.name.text.clone(),
                                        type_name(&parameter.parameter_type),
                                    )
                                })
                                .collect(),
                            output: type_name(&declaration.return_type),
                            failures,
                            may_suspend: project
                                .failures
                                .callables
                                .iter()
                                .find(|callable| callable.callable == declaration.name.text)
                                .is_some_and(|callable| callable.may_suspend),
                        });
                    }
                    Declaration::Route(declaration) => {
                        let method = method_name(declaration.method);
                        routes.push(RouteModel {
                            key: format!("{method} {}", declaration.path),
                            method,
                            path: declaration.path.clone(),
                            public: declaration.public,
                            fresh_authority: declaration.fresh_authority,
                            path_fields: declaration
                                .path_fields
                                .iter()
                                .map(|field| {
                                    (field.name.text.clone(), type_name(&field.field_type))
                                })
                                .collect(),
                            query: declaration.query.as_ref().map(type_name),
                            headers: declaration
                                .headers
                                .iter()
                                .map(|header| {
                                    (
                                        header.name.text.clone(),
                                        unquote(&header.wire_name.text).to_owned(),
                                        type_name(&header.field_type),
                                        header.optional,
                                    )
                                })
                                .collect(),
                            input: declaration.input.as_ref().map(type_name),
                            output: declaration.output.as_ref().map(type_name),
                            success: declaration.success,
                            deadline_ms: declaration.deadline.as_ref().map(|deadline| {
                                deadline.milliseconds().expect("checked route deadline")
                            }),
                            callable: declaration.run.as_ref().map(|run| {
                                run.callee
                                    .path
                                    .iter()
                                    .map(|part| part.text.as_str())
                                    .collect::<Vec<_>>()
                                    .join(".")
                            }),
                            behavior: if declaration.inline_action.is_some() {
                                "inline_action"
                            } else {
                                "run"
                            },
                        });
                    }
                    Declaration::Failure(declaration) => {
                        failure_declarations.insert(declaration.name.text.clone(), declaration);
                        for (scope, declarations) in [
                            ("public", &declaration.public_fields),
                            ("internal", &declaration.internal_fields),
                        ] {
                            for field in declarations {
                                fields.insert(
                                    format!(
                                        "{}.{}.{}",
                                        declaration.name.text, scope, field.name.text
                                    ),
                                    field,
                                );
                            }
                        }
                    }
                    Declaration::AuthenticationStrategy(declaration) => {
                        if let Some(exchange) = &declaration.exchange {
                            let path = unquote(&exchange.path.text).to_owned();
                            routes.push(RouteModel {
                                key: format!("POST {path}"),
                                method: "POST",
                                path,
                                public: false,
                                fresh_authority: false,
                                path_fields: Vec::new(),
                                query: None,
                                headers: Vec::new(),
                                input: None,
                                output: None,
                                success: RouteSuccess::Ok,
                                deadline_ms: None,
                                callable: None,
                                behavior: "service_credential_exchange",
                            });
                        }
                    }
                    Declaration::Application(_)
                    | Declaration::Locales(_)
                    | Declaration::Principal(_)
                    | Declaration::Fixture(_)
                    | Declaration::Test(_)
                    | Declaration::Job(_) => {}
                }
            }
        }
        routes.sort_by(|left, right| left.key.cmp(&right.key));
        callables.sort_by(|left, right| left.name.cmp(&right.name));

        Self {
            project,
            types,
            enums,
            records,
            fields,
            failure_declarations,
            routes,
            callables,
            configurations,
            relationships: operational_failures::RelationshipAnalysis::new(project),
            http_boundaries: http_boundary_failures::Analysis::new(project_path, project),
        }
    }

    fn configuration_audit_json(&self) -> String {
        let fields = self
            .configurations
            .iter()
            .flat_map(|configuration| {
                configuration.fields.iter().map(|field| {
                    let binding = field
                        .binding
                        .as_ref()
                        .map(|binding| json_string(unquote(&binding.text)))
                        .unwrap_or_else(|| "null".to_owned());
                    format!(
                        "{{\"configuration\":{},\"name\":{},\"type\":{},\"binding\":{binding},\"secret\":{},\"required\":{},\"lifecycle\":\"startup\",\"source_default\":{}}}",
                        json_string(&configuration.name.text),
                        json_string(&field.name.text),
                        json_string(&type_name(&field.field_type)),
                        field.secret,
                        field.default.is_none(),
                        field.default.as_ref().map_or("false", |_| "true"),
                    )
                })
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("{{\"schema_version\":1,\"fields\":[{fields}]}}")
    }

    fn policy_audit_json(&self) -> String {
        let bindings = self.project.policy.bindings.iter().map(|binding| {
            format!(
                "{{\"kind\":\"direct\",\"entity\":{},\"field\":{},\"role\":{},\"scope\":{},\"principal\":{},\"authoritative\":true}}",
                json_string(&binding.entity),
                json_string(&binding.field),
                json_string(&binding.role),
                json_string(&binding.scope),
                json_string(&binding.principal),
            )
        });
        let memberships = self.project.policy.memberships.iter().map(|membership| {
            format!(
                "{{\"kind\":\"membership\",\"entity\":{},\"scope_field\":{},\"scope\":{},\"member_field\":{},\"principal\":{},\"role_field\":{},\"role_type\":{},\"authoritative\":true}}",
                json_string(&membership.entity),
                membership
                    .scope_field
                    .as_ref()
                    .map_or_else(|| "null".to_owned(), |field| json_string(field)),
                json_string(&membership.scope),
                json_string(&membership.member_field),
                json_string(&membership.principal),
                json_string(&membership.role_field),
                json_string(&membership.role_type),
            )
        });
        let entities = self.project.policy.entities.iter().map(|entity| {
            let rules = entity.rules.iter().map(|rule| {
                format!(
                    "{{\"subject\":{},\"effects\":{}}}",
                    json_string(&rule.subject),
                    json_string_array(rule.effects.iter().map(String::as_str)),
                )
            });
            let fields = entity.fields.iter().map(|field| {
                let rules = field.rules.iter().map(|rule| {
                    format!(
                        "{{\"subject\":{},\"effects\":{}}}",
                        json_string(&rule.subject),
                        json_string_array(rule.effects.iter().map(String::as_str)),
                    )
                });
                format!(
                    "{{\"field\":{},\"mode\":\"narrowing\",\"rules\":{}}}",
                    json_string(&field.field),
                    json_array(rules),
                )
            });
            let mut effect_first = BTreeMap::<&str, Vec<&str>>::new();
            for rule in &entity.rules {
                for effect in &rule.effects {
                    effect_first
                        .entry(effect)
                        .or_default()
                        .push(&rule.subject);
                }
            }
            let effect_first = effect_first.into_iter().map(|(effect, subjects)| {
                format!(
                    "{{\"effect\":{},\"subjects\":{}}}",
                    json_string(effect),
                    json_string_array(subjects),
                )
            });
            format!(
                "{{\"entity\":{},\"default\":\"deny\",\"scope\":{},\"scope_field\":{},\"role_first\":{},\"effect_first\":{},\"fields\":{}}}",
                json_string(&entity.entity),
                json_optional_string(entity.scope.as_deref()),
                json_optional_string(entity.scope_field.as_deref()),
                json_array(rules),
                json_array(effect_first),
                json_array(fields),
            )
        });
        let operations = self.project.policy.operations.iter().map(|operation| {
            let obligations = operation.obligations.iter().map(|obligation| {
                format!(
                    "{{\"entity\":{},\"effect\":{},\"subjects\":{},\"policy_source\":{},\"judgement\":{}}}",
                    json_string(&obligation.entity),
                    json_string(&obligation.effect),
                    json_string_array(obligation.subjects.iter().map(String::as_str)),
                    json_string(obligation.source),
                    json_string(if obligation.subjects.is_empty() { "rejected" } else { "proved" }),
                )
            });
            let field_reads = operation.field_reads.iter().map(|read| {
                format!(
                    "{{\"entity\":{},\"field\":{},\"subjects\":{},\"source\":{},\"judgement\":\"proved\"}}",
                    json_string(&read.entity),
                    json_string(&read.field),
                    json_string_array(read.subjects.iter().map(String::as_str)),
                    json_string(read.source),
                )
            });
            format!(
                "{{\"operation\":{},\"obligations\":{},\"restricted_field_reads\":{}}}",
                json_string(&operation.operation),
                json_array(obligations),
                json_array(field_reads),
            )
        });
        format!(
            "{{\"schema_version\":1,\"active\":{},\"bindings\":{},\"memberships\":{},\"entities\":{},\"operations\":{}}}",
            self.project.policy.active,
            json_array(bindings),
            json_array(memberships),
            json_array(entities),
            json_array(operations),
        )
    }

    fn routes_json(&self) -> String {
        let routes = self
            .routes
            .iter()
            .map(|route| {
                let failures = self.route_failures(&route.key);
                let relationships = self.relationships.route(&route.key);
                let boundary = self.http_boundaries.route(&route.key);
                let path_fields = route.path_fields.iter().map(|(name, field_type)| {
                    format!(
                        "{{\"name\":{},\"type\":{}}}",
                        json_string(name),
                        json_string(field_type)
                    )
                });
                let headers = route.headers.iter().map(|(name, wire_name, field_type, optional)| {
                    format!(
                        "{{\"name\":{},\"wire_name\":{},\"type\":{},\"optional\":{}}}",
                        json_string(name),
                        json_string(wire_name),
                        json_string(field_type),
                        optional,
                    )
                });
                format!(
                    "{{\"route\":{},\"method\":{},\"path\":{},\"auth\":{},\"path_fields\":{},\"query\":{},\"headers\":{},\"input\":{},\"output\":{},\"success\":{{\"kind\":{},\"http_status\":{}}},\"deadline_ms\":{},\"behavior\":{},\"callable\":{},\"failures\":{},\"operational_boundary\":\"bun_http\",\"operational_failures\":{},\"relationship_failure_analysis\":{},\"http_boundary_failures\":{},\"http_boundary_analysis\":{}}}",
                    json_string(&route.key),
                    json_string(route.method),
                    json_string(&route.path),
                    json_string(if route.public {
                        "none"
                    } else if route.fresh_authority {
                        "fresh_authority"
                    } else {
                        "authenticated_default"
                    }),
                    json_array(path_fields),
                    json_optional_string(route.query.as_deref()),
                    json_array(headers),
                    json_optional_string(route.input.as_deref()),
                    json_optional_string(route.output.as_deref()),
                    json_string(match route.success {
                        RouteSuccess::Ok => "ok",
                        RouteSuccess::Created => "created",
                        RouteSuccess::NoContent => "no_content",
                    }),
                    match route.success {
                        RouteSuccess::Ok => 200,
                        RouteSuccess::Created => 201,
                        RouteSuccess::NoContent => 204,
                    },
                    route.deadline_ms.map_or_else(
                        || "null".to_owned(),
                        |milliseconds| milliseconds.to_string(),
                    ),
                    json_string(route.behavior),
                    json_optional_string(route.callable.as_deref()),
                    json_array(failures.into_iter().map(|failure| {
                        format!(
                            "{{\"name\":{},\"code\":{},\"http_status\":{},\"derived\":true}}",
                            json_string(failure.name),
                            json_string(failure.code),
                            failure.http_status
                        )
                    })),
                    bun_operational_failures_json(relationships.may_fail),
                    relationships.json(),
                    boundary.failures_json(),
                    boundary.json()
                )
            });
        format!("{{\"schema_version\":2,\"routes\":{}}}", json_array(routes))
    }

    fn callables_json(&self) -> String {
        let callables = self.callables.iter().map(|callable| {
            let parameters = callable.parameters.iter().map(|(name, parameter_type)| {
                format!(
                    "{{\"name\":{},\"type\":{}}}",
                    json_string(name),
                    json_string(parameter_type)
                )
            });
            format!(
                "{{\"name\":{},\"kind\":{},\"parameters\":{},\"output\":{},\"failures\":{},\"execution\":{{\"may_suspend\":{},\"completion\":\"before_caller_continues\"}}}}",
                json_string(&callable.name),
                json_string(callable.kind),
                json_array(parameters),
                json_string(&callable.output),
                json_string_array(callable.failures.iter().map(String::as_str)),
                callable.may_suspend
            )
        });
        format!(
            "{{\"schema_version\":2,\"callables\":{}}}",
            json_array(callables)
        )
    }

    fn failure_audit_json(&self) -> String {
        let contracts = self.project.failures.contracts.iter().map(|contract| {
            let routes = self
                .project
                .failures
                .routes
                .iter()
                .filter(|route| route.failure == contract.name)
                .map(|route| route.route.as_str());
            format!(
                "{{\"failure\":{},\"code\":{},\"kind\":{},\"http_status\":{},\"message\":{},\"disclosure\":{{\"public_fields\":{},\"internal_fields\":{},\"internal_to_client\":false}},\"routes\":{}}}",
                json_string(&contract.name),
                json_string(&contract.code),
                json_string(&contract.kind),
                contract.http_status.map_or_else(|| "null".to_owned(), |status| status.to_string()),
                json_optional_string(contract.message.as_deref()),
                json_string_array(contract.public_fields.iter().map(String::as_str)),
                json_string_array(contract.internal_fields.iter().map(String::as_str)),
                json_string_array(routes)
            )
        });
        format!(
            "{{\"schema_version\":2,\"policy\":{{\"closed_public_payloads\":true,\"internal_context_disclosed\":false}},\"failures\":{},\"operational_boundary\":\"bun_http\",\"operational_failures\":{},\"http_boundary\":{}}}",
            json_array(contracts), bun_operational_failures_json(true), self.http_boundaries.audit()
        )
    }

    fn entity_audit_json(&self) -> String {
        let entities = self.project.entity_model.entities.iter().map(|entity| {
            let representations = entity.representations.iter().map(|representation| {
                format!(
                    "{{\"kind\":{},\"name\":{},\"store\":{},\"from_authority\":{},\"strategy\":{},\"required_delivery\":\"durable\",\"change_record\":\"same_local_commit\",\"ordering\":\"per_entity_revision\",\"change_id\":\"unique\",\"delivery_runtime\":\"adapter_pending\",\"replay\":false,\"rebuild\":false,\"watermark\":false,\"reconciliation\":false}}",
                    json_string(representation.kind),
                    json_string(&representation.name),
                    json_string(&representation.store),
                    json_string(&representation.authority),
                    json_optional_string(representation.strategy.as_deref())
                )
            });
            format!(
                "{{\"name\":{},\"identity\":{},\"persistent\":{},\"authority_store\":{},\"reference\":{},\"operations\":{},\"representations\":{}}}",
                json_string(&entity.name),
                json_string(&entity.identity),
                entity.persistent,
                json_optional_string(entity.authority_store.as_deref()),
                json_string(&format!("{}.Ref", entity.name)),
                json_string_array(entity.operations.iter().map(String::as_str)),
                json_array(representations)
            )
        });
        let queries = self.project.entity_model.queries.iter().map(|query| {
            format!(
                "{{\"name\":{},\"owner\":{},\"read_only\":true,\"freshness\":{},\"plan\":{},\"reads\":{},\"predicate_fields\":{},\"policy_source\":\"authority\"}}",
                json_string(&query.name),
                json_optional_string(query.owner.as_deref()),
                json_string(query.freshness),
                json_string(query.plan),
                json_string_array(query.reads.iter().map(String::as_str)),
                json_string_array(query.predicate_fields.iter().map(String::as_str))
            )
        });
        format!(
            "{{\"schema_version\":1,\"model\":\"DATA-007/CONSISTENCY-001\",\"entities\":{},\"queries\":{}}}",
            json_array(entities),
            json_array(queries)
        )
    }

    fn transaction_audit_json(&self) -> String {
        let actions = self.project.entity_model.transactions.iter().map(|transaction| {
            format!(
                "{{\"action\":{},\"mutation_owners\":{},\"disposition\":{},\"domain\":{},\"nested_actions\":{},\"handled_failure\":{},\"commit\":\"outer_success\",\"rollback\":\"propagated_failure\",\"postgres\":{{\"isolation\":{},\"concurrency\":{},\"deadlock_order\":\"stable_entity_identity\"}},\"sqlite\":{{\"isolation\":{},\"concurrency\":\"single_immediate_writer\"}},\"retry\":{}}}",
                json_string(&transaction.action),
                json_string_array(transaction.owners.iter().map(String::as_str)),
                json_string(transaction.disposition),
                json_optional_string(transaction.domain.as_deref()),
                json_string(transaction.nested),
                json_string(transaction.handled_failure),
                json_string(transaction.postgres_isolation),
                json_string(transaction.postgres_concurrency),
                json_string(transaction.sqlite_isolation),
                json_string(transaction.retry)
            )
        });
        format!(
            "{{\"schema_version\":1,\"model\":\"TX-001\",\"actions\":{}}}",
            json_array(actions)
        )
    }

    fn service_audit_json(&self, project_path: &Path) -> String {
        let effects = self
            .project
            .semantics
            .external_effects
            .iter()
            .map(|effect| {
                let mappings = effect
                    .outcome_mappings
                    .iter()
                    .map(|(provider, failure)| {
                        format!(
                            "{{\"provider\":{},\"failure\":{}}}",
                            json_string(provider),
                            json_string(failure)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let schema_parity = effect.schema_parity_json();
                format!(
                    "{{\"service\":{},\"operation\":{},\"method\":{},\"path\":{},\"input\":{},\"output\":{},\"idempotency_type\":{},\"egress\":{},\"credential_slot\":{},\"credential_header\":{},\"imported_contract\":{},\"import_version\":{},\"import_sha256\":{},\"timeout_ms\":{},\"retry\":{{\"max_attempts\":{},\"max_elapsed_ms\":{},\"jitter\":{}}},\"proxy_allowed\":{},\"redirects_allowed\":{},\"outcomes\":{},\"outcome_mappings\":[{}],\"schema_parity\":{},\"source\":{},\"range\":{{\"start\":{},\"end\":{}}}}}",
                    json_string(&effect.service),
                    json_string(&effect.operation),
                    json_string(&effect.method),
                    json_string(&effect.path),
                    json_string(&effect.input),
                    json_string(&effect.output),
                    json_string(&effect.idempotency_type),
                    json_string(&effect.egress),
                    json_string(&effect.credential_slot),
                    json_string(&effect.credential_header),
                    json_string(&effect.imported_contract),
                    json_string(&effect.import_version),
                    json_string(&effect.import_sha256),
                    effect.timeout_ms,
                    effect.max_attempts,
                    effect.max_elapsed_ms,
                    json_string(&effect.jitter),
                    effect.proxy_allowed,
                    effect.redirects_allowed,
                    json_string_array(effect.outcomes.iter().map(String::as_str)),
                    mappings,
                    schema_parity,
                    json_string(&normalized_source(project_root(project_path), &effect.source)),
                    effect.range.start,
                    effect.range.end
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"schema_version\":1,\"kind\":\"service_effect_contract\",\"runtime_conformance\":\"not_established\",\"effects\":[{effects}]}}"
        )
    }

    fn validator_plan_json(&self) -> String {
        let named_types = self.types.iter().map(|(name, declaration)| {
            format!(
                "{{\"name\":{},\"representation\":{},\"constraints\":{}}}",
                json_string(name),
                json_string(&type_name(&declaration.parent)),
                constraints_json(&declaration.constraints)
            )
        });
        let records = self.records.iter().map(|(name, declaration)| {
            let fields = declaration
                .fields
                .iter()
                .map(|field| field_plan_json(field, self.reference_is_nullable(&field.field_type)));
            format!(
                "{{\"name\":{},\"kind\":{},\"persistent\":{},\"closed_shape\":true,\"fields\":{}}}",
                json_string(name),
                json_string(record_kind(declaration.kind)),
                declaration.is_persistent_entity(),
                json_array(fields)
            )
        });
        let enums = self.enums.iter().map(|(name, declaration)| {
            let payloads = declaration.variants.iter().map(|variant| {
                format!(
                    "{{\"name\":{},\"fields\":{}}}",
                    json_string(&variant.name.text),
                    json_array(variant.fields.iter().map(|field| {
                        field_plan_json(field, self.reference_is_nullable(&field.field_type))
                    }))
                )
            });
            format!(
                "{{\"name\":{},\"tagged\":{},\"variants\":{},\"payloads\":{}}}",
                json_string(name),
                declaration
                    .variants
                    .iter()
                    .any(|variant| !variant.fields.is_empty()),
                json_string_array(
                    declaration
                        .variants
                        .iter()
                        .map(|variant| variant.name.text.as_str())
                ),
                json_array(payloads)
            )
        });
        let boundaries = self.routes.iter().map(|route| {
            format!(
                "{{\"route\":{},\"query\":{},\"headers\":{},\"input\":{},\"output\":{},\"reject_unknown_input_fields\":true,\"validate_output\":true}}",
                json_string(&route.key),
                json_optional_string(route.query.as_deref()),
                json_array(route.headers.iter().map(|(name, wire_name, field_type, optional)| format!(
                    "{{\"name\":{},\"wire_name\":{},\"type\":{},\"optional\":{}}}",
                    json_string(name), json_string(wire_name), json_string(field_type), optional
                ))),
                json_optional_string(route.input.as_deref()),
                json_optional_string(route.output.as_deref())
            )
        });
        format!(
            "{{\"schema_version\":1,\"named_types\":{},\"enums\":{},\"records\":{},\"boundaries\":{}}}",
            json_array(named_types),
            json_array(enums),
            json_array(records),
            json_array(boundaries)
        )
    }

    fn public_failure_compatibility_json(&self) -> String {
        let contracts = self.project.failures.contracts.iter().map(|contract| {
            format!(
                "{{\"code\":{},\"failure\":{},\"http_status\":{},\"message\":{},\"public_fields\":{}}}",
                json_string(&contract.code),
                json_string(&contract.name),
                crate::target::bun_failure_http_status(&contract.kind, contract.http_status),
                json_optional_string(contract.message.as_deref()),
                json_string_array(contract.public_fields.iter().map(String::as_str))
            )
        });
        format!(
            "{{\"schema_version\":1,\"baseline\":null,\"status\":\"baseline_not_configured\",\"breaking_change_keys\":[\"code\",\"http_status\",\"message\",\"public_fields\"],\"current\":{}}}",
            json_array(contracts)
        )
    }

    fn authentication_strategies(&self) -> Vec<&jadpo_syntax::AuthenticationStrategyDeclaration> {
        self.project
            .syntax
            .sources
            .iter()
            .flat_map(|s| &s.file.declarations)
            .filter_map(|d| match d {
                Declaration::AuthenticationStrategy(a) => Some(a),
                _ => None,
            })
            .collect()
    }

    fn authentication_audit_json(&self, project_path: &Path) -> String {
        let supported =
            crate::target::first_party_authentication_supported(project_path, self.project);
        let jwt = crate::target::jwt_authentication_supported(project_path, self.project);
        let dependencies = if jwt {
            format!("[{}]", include_str!("runtime/jwt/dependency.json"))
        } else {
            "[]".to_owned()
        };
        let unsupported = if supported {
            "[\"profile_bearing_principals\",\"public_login_endpoints\"]"
        } else {
            "[\"configured_runtime_pending\",\"profile_bearing_principals\",\"public_login_endpoints\"]"
        };
        let jwt_authority = if jwt {
            "\"verified_subject_resolves_once_on_every_request\""
        } else {
            "null"
        };
        let strategies = self.authentication_strategies().into_iter().map(|strategy| {
            let (transport, location) = match &strategy.transport.location {
                jadpo_syntax::CredentialLocation::Cookie(cookie) => ("cookie", cookie.text.trim_matches('"')),
                jadpo_syntax::CredentialLocation::Bearer(_) => ("bearer", "authorization"),
            };
            let validators = strategy.validators.iter().map(|v| {
                let credentials = v.credentials.as_ref().map(|binding| {
                    let field = |reference: &jadpo_syntax::NameExpression| json_string(&reference.path.iter().map(|n| n.text.as_str()).collect::<Vec<_>>().join("."));
                    let active = self.project.syntax.sources.iter().find(|source| source.file.declarations.iter().any(|declaration| matches!(declaration, jadpo_syntax::Declaration::AuthenticationStrategy(item) if std::ptr::eq(item, strategy))))
                        .and_then(|source| source.source_text.get(binding.active.range().start..binding.active.range().end)).unwrap_or_default();
                    format!("{{\"identity\":{},\"principal\":{},\"verifier\":{},\"active\":{},\"expires\":{},\"revoked\":{}}}", field(&binding.identity), field(&binding.principal), field(&binding.verifier), json_string(active), field(&binding.expires), field(&binding.revoked))
                }).unwrap_or_else(|| "null".to_owned());
                format!("{{\"name\":{},\"mode\":{},\"principal\":{},\"settings\":{},\"credentials\":{}}}", json_string(&v.name.text), json_string(&v.mode.text), json_string(&v.principal.text), json_array(v.settings.iter().map(|s| json_string(&s.name.text))), credentials)
            });
            let resolutions = strategy.resolutions.iter().map(|r| format!("{{\"authority\":{},\"inactive_failure\":{}}}", json_string(&r.authority.path.iter().map(|n| n.text.as_str()).collect::<Vec<_>>().join(".")), json_string(&r.inactive.text)));
            let exchange = strategy.exchange.as_ref().map_or_else(
                || "null".to_owned(),
                |exchange| {
                    format!(
                        "{{\"method\":\"POST\",\"path\":{},\"key_validator\":{},\"signed_validator\":{},\"credential_selection\":\"exactly_one\",\"raw_credential\":\"compiler_private_only\",\"success\":{{\"status\":200,\"response_fields\":[\"access_token\",\"token_type\",\"expires_at\"],\"expiry\":\"min(declared_key_expires_at, now + application.revocation.maximum_delay)\"}},\"failures\":{{\"authentication_required\":401,\"invalid_credentials\":401,\"ambiguous_credentials\":401,\"service_disabled\":403,\"authentication_unavailable\":503}}}}",
                        json_string(unquote(&exchange.path.text)),
                        json_string(&exchange.key.text),
                        json_string(&exchange.signed.text),
                    )
                },
            );
            format!("{{\"name\":{},\"transport\":{},\"location\":{},\"validators\":{},\"resolutions\":{},\"exchange\":{},\"source_range\":{{\"start\":{},\"end\":{}}}}}", json_string(&strategy.name.text), json_string(transport), json_string(location), json_array(validators), json_array(resolutions), exchange, strategy.range.start, strategy.range.end)
        });
        let revocation = self
            .project
            .syntax
            .sources
            .iter()
            .flat_map(|s| &s.file.declarations)
            .find_map(|d| match d {
                Declaration::Application(a) => Some(format!(
                    "{{\"mode\":{},\"maximum_delay\":{}}}",
                    json_string(
                        if a.authentication.revocation.mode
                            == jadpo_syntax::RevocationMode::Immediate
                        {
                            "immediate"
                        } else {
                            "bounded"
                        }
                    ),
                    json_optional_string(
                        a.authentication
                            .revocation
                            .maximum_delay
                            .as_ref()
                            .map(|d| d.text.as_str())
                    )
                )),
                _ => None,
            })
            .unwrap_or("null".to_owned());
        format!("{{\"schema_version\":1,\"runtime\":{},\"revocation\":{},\"strategies\":{},\"routes\":{},\"credential_selection\":\"exactly_one\",\"verification\":\"runtime_validation\",\"package_dependencies\":{dependencies},\"csrf\":\"cookie_mutations_require_configured_origin_and_session_bound_header\",\"unsupported\":{unsupported},\"jwt_authority\":{jwt_authority},\"query_accounting\":\"principal_lookup_separate_from_credential_checks\",\"assurance\":\"exploratory; not an independent security review\"}}", json_string(if supported { "first_party" } else { "adapter_pending" }), revocation, json_array(strategies), self.routes_json())
    }

    fn openapi_json(&self) -> String {
        let strategies = self.authentication_strategies();
        let security_schemes = strategies
            .iter()
            .map(|strategy| {
                let scheme = match &strategy.transport.location {
                    jadpo_syntax::CredentialLocation::Cookie(cookie) => format!(
                        "{{\"type\":\"apiKey\",\"in\":\"cookie\",\"name\":{}}}",
                        json_string(cookie.text.trim_matches('"'))
                    ),
                    jadpo_syntax::CredentialLocation::Bearer(_) => {
                        "{\"type\":\"http\",\"scheme\":\"bearer\"}".to_owned()
                    }
                };
                format!("{}:{}", json_string(&strategy.name.text), scheme)
            })
            .collect::<Vec<_>>()
            .join(",");
        let mut paths = BTreeMap::<String, Vec<String>>::new();
        for route in &self.routes {
            let mut responses = Vec::new();
            let (success_status, success_description) = match route.success {
                RouteSuccess::Ok => (200, "Success"),
                RouteSuccess::Created => (201, "Created"),
                RouteSuccess::NoContent => (204, "No content"),
            };
            if route.success == RouteSuccess::NoContent {
                responses.push(format!(
                    "\"{success_status}\":{{\"description\":{}}}",
                    json_string(success_description)
                ));
            } else if route.behavior == "service_credential_exchange" {
                responses.push(r#""200":{"description":"Bounded service bearer issued","content":{"application/json":{"schema":{"type":"object","additionalProperties":false,"properties":{"access_token":{"type":"string"},"token_type":{"type":"string","enum":["Bearer"]},"expires_at":{"type":"string","format":"date-time"}},"required":["access_token","token_type","expires_at"]}}}}"#.to_owned());
            } else if let Some(output) = &route.output {
                responses.push(format!(
                    "\"{success_status}\":{{\"description\":{},\"content\":{{\"application/json\":{{\"schema\":{}}}}}}}",
                    json_string(success_description),
                    self.openapi_type_schema(output)
                ));
            } else {
                responses.push(format!(
                    "\"{success_status}\":{{\"description\":{}}}",
                    json_string(success_description)
                ));
            }
            if route.deadline_ms.is_some() {
                responses.push("\"504\":{\"description\":\"The route operation exceeded its declared deadline\"}".to_owned());
            }
            let boundary = self.http_boundaries.route(&route.key);
            let mut failure_groups = BTreeMap::<u16, Vec<&str>>::new();
            for failure in &boundary.failures {
                failure_groups.entry(failure.status).or_default();
            }
            for failure in self.route_failures(&route.key) {
                failure_groups
                    .entry(failure.http_status)
                    .or_default()
                    .push(failure.name);
            }
            let relationships = self.relationships.route(&route.key);
            if relationships.may_fail {
                failure_groups.entry(409).or_default();
            }
            failure_groups.entry(500).or_default();
            if self.has_persistence_entities() {
                failure_groups.entry(503).or_default();
            }
            for (status, names) in failure_groups {
                let mut references = names
                    .iter()
                    .map(|name| {
                        format!(
                            "{{\"$ref\":{}}}",
                            json_string(&format!("#/components/schemas/{name}"))
                        )
                    })
                    .collect::<Vec<_>>();
                let native = boundary
                    .failures
                    .iter()
                    .filter(|failure| failure.status == status)
                    .collect::<Vec<_>>();
                // The wire cannot distinguish two causes emitting the same
                // closed envelope. Exclude native alternatives from colliding
                // domain references so oneOf never double-matches that body.
                for (reference, name) in references.iter_mut().zip(&names) {
                    if let Some(contract) = self
                        .project
                        .failures
                        .contracts
                        .iter()
                        .find(|contract| contract.name == *name)
                    {
                        let mut overlap = native
                            .iter()
                            .filter(|failure| failure.code == contract.code)
                            .map(|failure| failure.schema())
                            .collect::<Vec<_>>();
                        // An ordinary declared conflict and the normalized FK
                        // boundary may have the same closed public envelope.
                        // Keep custom messages/details on their own reference.
                        if status == 409
                            && relationships.may_fail
                            && contract.code == "relationship_conflict"
                            && contract.public_fields.is_empty()
                            && contract.message.as_deref().map_or(true, |message| {
                                message == "The operation conflicts with a relationship."
                            })
                        {
                            overlap.push(
                                serde_json::from_str::<serde_json::Value>(
                                    &bun_operational_schema_json(409),
                                )
                                .unwrap(),
                            );
                        }
                        if matches!(status, 500 | 503)
                            && (status == 500 || self.has_persistence_entities())
                            && contract.public_fields.is_empty()
                            && operational_wire_pairs(status)
                                .iter()
                                .any(|(code, message)| {
                                    contract.code == *code
                                        && contract
                                            .message
                                            .as_deref()
                                            .map_or(true, |value| value == *message)
                                })
                        {
                            overlap.push(
                                serde_json::from_str::<serde_json::Value>(
                                    &bun_operational_schema_json(status),
                                )
                                .unwrap(),
                            );
                        }
                        if !overlap.is_empty() {
                            *reference = serde_json::json!({"allOf":[serde_json::from_str::<serde_json::Value>(reference).unwrap(),{"not":{"anyOf":overlap}}]}).to_string();
                        }
                    }
                }
                // Distinct native conditions can emit identical wire bodies.
                // Retain those conditions in the inventory/audit, but include
                // their shared schema once so oneOf still accepts the body.
                let mut native_schemas = BTreeSet::new();
                references.extend(native.iter().filter_map(|failure| {
                    let schema = failure.schema().to_string();
                    native_schemas.insert(schema.clone()).then_some(schema)
                }));
                if status == 409 && relationships.may_fail
                    || status == 500
                    || status == 503 && self.has_persistence_entities()
                {
                    references.push(bun_operational_schema_json(status));
                }
                let failure_schema = if references.len() == 1 {
                    references[0].clone()
                } else {
                    format!("{{\"oneOf\":{}}}", json_array(references))
                };
                responses.push(format!(
                    "{}:{{\"description\":{},\"content\":{{\"application/json\":{{\"schema\":{}}}}}}}",
                    json_string(&status.to_string()),
                    json_string(&match status {
                        409 if relationships.may_fail => "Bun relationship constraint conflict or declared failure".to_owned(),
                        500 => "Bun operational failure or declared failure; uncertain effects are not automatically retried".to_owned(),
                        503 if self.has_persistence_entities() => "Bun storage unavailable or declared failure".to_owned(),
                        _ if !native.is_empty() => "Native HTTP boundary failure or declared failure".to_owned(),
                        _ => names.join(" or "),
                    }),
                    failure_schema
                ));
            }
            responses.sort();
            let request_body = route.input.as_ref().map_or_else(
                || "null".to_owned(),
                |input| {
                    format!(
                        "{{\"required\":true,\"content\":{{\"application/json\":{{\"schema\":{}}}}}}}",
                        self.openapi_type_schema(input)
                    )
                },
            );
            let mut parameters = route
                .path_fields
                .iter()
                .map(|(name, field_type)| {
                    format!(
                        "{{\"name\":{},\"in\":\"path\",\"required\":true,\"schema\":{}}}",
                        json_string(name),
                        self.openapi_type_schema(field_type)
                    )
                })
                .collect::<Vec<_>>();
            if let Some(query_name) = &route.query {
                if let Some(query) = self.records.get(query_name) {
                    for field in &query.fields {
                        let mut schema = self.openapi_field_schema(field);
                        if let Some(default) = &field.default {
                            let value = if default.kind == jadpo_syntax::LiteralKind::None {
                                "null".to_owned()
                            } else {
                                default.text.clone()
                            };
                            schema = format!("{{\"allOf\":[{schema}],\"default\":{value}}}");
                        }
                        let required = !field.optional && field.default.is_none();
                        let field_type = type_name(&field.field_type);
                        if self.records.contains_key(&field_type) {
                            parameters.push(format!(
                                "{{\"name\":{},\"in\":\"query\",\"required\":{required},\"content\":{{\"application/json\":{{\"schema\":{schema}}}}}}}",
                                json_string(&field.name.text),
                            ));
                        } else {
                            parameters.push(format!(
                                "{{\"name\":{},\"in\":\"query\",\"required\":{required},\"schema\":{schema}}}",
                                json_string(&field.name.text),
                            ));
                        }
                    }
                }
            }
            parameters.extend(
                route
                    .headers
                    .iter()
                    .map(|(_, wire_name, field_type, optional)| {
                        format!(
                            "{{\"name\":{},\"in\":\"header\",\"required\":{},\"schema\":{}}}",
                            json_string(wire_name),
                            !optional,
                            self.openapi_type_schema(field_type),
                        )
                    }),
            );
            let parameters = json_array(parameters);
            let security = if route.public {
                "[]".to_owned()
            } else {
                json_array(
                    strategies
                        .iter()
                        .filter(|s| {
                            route.behavior != "service_credential_exchange"
                                || s.exchange.as_ref().is_some_and(|exchange| {
                                    unquote(&exchange.path.text) == route.path
                                })
                        })
                        .map(|s| format!("{{{}:[]}}", json_string(&s.name.text))),
                )
            };
            let deadline_extension = route.deadline_ms.map_or_else(String::new, |milliseconds| {
                format!(",\"x-jadpo-deadline-ms\":{milliseconds}")
            });
            let operation = format!(
                "{}:{{\"operationId\":{},\"parameters\":{},\"requestBody\":{},\"responses\":{{{}}},\"security\":{},\"x-jadpo-fresh-authority\":{}{}}}",
                json_string(&route.method.to_ascii_lowercase()),
                json_string(&if route.behavior == "service_credential_exchange" {
                    strategies.iter().find(|strategy| strategy.exchange.as_ref().is_some_and(|exchange| unquote(&exchange.path.text)==route.path)).map(|strategy|format!("exchange_service_credential_{}",strategy.name.text)).unwrap_or_else(||route.key.clone())
                } else {route.callable.clone().unwrap_or_else(||route.key.clone())}),
                parameters,
                request_body,
                responses.join(","),
                security,
                route.fresh_authority,
                deadline_extension
            );
            paths.entry(route.path.clone()).or_default().push(operation);
        }

        let mut schemas = Vec::new();
        for (name, declaration) in &self.types {
            schemas.push(format!(
                "{}:{}",
                json_string(name),
                self.openapi_scalar_schema(declaration)
            ));
        }
        for (name, declaration) in &self.enums {
            schemas.push(format!(
                "{}:{}",
                json_string(name),
                self.openapi_enum_schema(declaration)
            ));
        }
        for (name, declaration) in self
            .records
            .iter()
            .filter(|(name, _)| !name.starts_with("__jadpo_"))
        {
            schemas.push(format!(
                "{}:{}",
                json_string(name),
                self.openapi_record_schema(declaration)
            ));
        }
        for name in self.public_field_schema_names() {
            let field = self.fields[&name];
            schemas.push(format!(
                "{}:{}",
                json_string(&name),
                self.openapi_field_schema(field)
            ));
        }
        for contract in &self.project.failures.contracts {
            schemas.push(format!(
                "{}:{}",
                json_string(&contract.name),
                self.openapi_failure_schema(contract)
            ));
        }
        schemas.sort();

        let components = format!(
            "{{\"schemas\":{{{}}},\"securitySchemes\":{{{}}}}}",
            schemas.join(","),
            security_schemes
        );
        format!(
            "{{\"openapi\":\"3.1.0\",\"info\":{{\"title\":\"Application API\",\"version\":\"0.0.0\"}},\"paths\":{{{}}},\"components\":{}}}",
            paths.iter().map(|(path, operations)| format!("{}:{{{}}}", json_string(path), operations.join(","))).collect::<Vec<_>>().join(","), components
        )
    }

    fn route_failures(&self, route_key: &str) -> Vec<RouteFailureView<'_>> {
        self.project
            .failures
            .routes
            .iter()
            .filter(|route| route.route == route_key)
            .filter_map(|route| {
                self.project
                    .failures
                    .contracts
                    .iter()
                    .find(|contract| contract.name == route.failure)
                    .map(|contract| RouteFailureView {
                        name: &route.failure,
                        code: &contract.code,
                        http_status: crate::target::bun_failure_http_status(
                            &contract.kind,
                            route.http_status,
                        ),
                    })
            })
            .collect()
    }

    fn openapi_field_schema(&self, field: &FieldDeclaration) -> String {
        let parent = self.openapi_type_schema(&type_name(&field.field_type));
        let constraints = openapi_constraint_parts(&field.constraints);
        if constraints.is_empty() {
            parent
        } else {
            format!("{{\"allOf\":[{parent},{{{}}}]}}", constraints.join(","))
        }
    }

    fn public_field_schema_names(&self) -> BTreeSet<String> {
        let is_internal = |name: &str| {
            self.failure_declarations
                .keys()
                .any(|failure| name.starts_with(&format!("{failure}.internal.")))
        };
        let mut names = self
            .fields
            .keys()
            .filter(|name| !is_internal(name) && !name.starts_with("__jadpo_"))
            .cloned()
            .collect::<BTreeSet<_>>();
        for declaration in self.types.values() {
            self.collect_field_schema_names(&type_name(&declaration.parent), &mut names);
        }
        for route in &self.routes {
            for reference in route
                .query
                .iter()
                .chain(route.input.iter())
                .chain(route.output.iter())
                .chain(route.path_fields.iter().map(|(_, reference)| reference))
                .chain(route.headers.iter().map(|(_, _, reference, _)| reference))
            {
                self.collect_field_schema_names(reference, &mut names);
            }
        }
        // Include internal value contracts only when a public declaration
        // explicitly references them. Follow chains to keep every $ref valid.
        loop {
            let before = names.len();
            for name in names.clone() {
                if let Some(field) = self.fields.get(&name) {
                    self.collect_field_schema_names(&type_name(&field.field_type), &mut names);
                }
            }
            if names.len() == before {
                return names;
            }
        }
    }

    fn collect_field_schema_names(&self, name: &str, output: &mut BTreeSet<String>) {
        let base = name.trim_end_matches('?');
        if self.fields.contains_key(base) && !base.starts_with("__jadpo_") {
            output.insert(base.to_owned());
        }
        if let Some((_, arguments)) = base.split_once('<') {
            if let Some(arguments) = arguments.strip_suffix('>') {
                for argument in split_generic_arguments(arguments) {
                    self.collect_field_schema_names(argument, output);
                }
            }
        }
    }

    fn openapi_scalar_schema(&self, declaration: &TypeDeclaration) -> String {
        let parent = self.openapi_type_schema(&type_name(&declaration.parent));
        let constraints = openapi_constraint_parts(&declaration.constraints);
        if constraints.is_empty() {
            parent
        } else {
            format!("{{\"allOf\":[{parent},{{{}}}]}}", constraints.join(","))
        }
    }

    fn openapi_enum_schema(&self, declaration: &EnumDeclaration) -> String {
        if declaration
            .variants
            .iter()
            .any(|variant| !variant.fields.is_empty())
        {
            let variants = declaration.variants.iter().map(|variant| {
                let properties = std::iter::once(format!(
                    "\"tag\":{{\"type\":\"string\",\"const\":{}}}",
                    json_string(&variant.name.text)
                ))
                .chain(variant.fields.iter().map(|field| {
                    format!(
                        "{}:{}",
                        json_string(&field.name.text),
                        self.openapi_field_schema(field)
                    )
                }))
                .collect::<Vec<_>>()
                .join(",");
                let required = std::iter::once("tag")
                    .chain(
                        variant
                            .fields
                            .iter()
                            .filter(|field| !field.optional)
                            .map(|field| field.name.text.as_str()),
                    );
                format!(
                    "{{\"type\":\"object\",\"additionalProperties\":false,\"properties\":{{{properties}}},\"required\":{}}}",
                    json_string_array(required)
                )
            });
            return format!(
                "{{\"oneOf\":{},\"discriminator\":{{\"propertyName\":\"tag\"}}}}",
                json_array(variants)
            );
        }
        format!(
            "{{\"type\":\"string\",\"enum\":{}}}",
            json_string_array(
                declaration
                    .variants
                    .iter()
                    .map(|variant| variant.name.text.as_str())
            )
        )
    }

    fn openapi_record_schema(&self, declaration: &RecordDeclaration) -> String {
        let properties = declaration.fields.iter().map(|field| {
            format!(
                "{}:{}",
                json_string(&field.name.text),
                self.openapi_field_schema(field)
            )
        });
        let required = declaration
            .fields
            .iter()
            .filter(|field| !field.optional && field.default.is_none())
            .map(|field| field.name.text.as_str());
        format!(
            "{{\"type\":\"object\",\"additionalProperties\":false,\"properties\":{{{}}},\"required\":{}}}",
            properties.collect::<Vec<_>>().join(","),
            json_string_array(required)
        )
    }

    fn openapi_failure_schema(&self, contract: &jadpo_semantic::FailureContract) -> String {
        let mut error_properties = vec![
            format!(
                "\"code\":{{\"type\":\"string\",\"const\":{}}}",
                json_string(&contract.code)
            ),
            "\"request_id\":{\"type\":\"string\"}".to_owned(),
        ];
        let mut required = vec!["code", "request_id"];
        if let Some(message) = &contract.message {
            error_properties.push(format!(
                "\"message\":{{\"type\":\"string\",\"const\":{}}}",
                json_string(message)
            ));
            required.push("message");
        } else {
            error_properties.push("\"message\":{\"type\":\"string\"}".to_owned());
            required.push("message");
        }
        if !contract.public_fields.is_empty() {
            let declaration = self
                .failure_declarations
                .get(&contract.name)
                .expect("checked failure contract has its authored declaration");
            let properties = declaration
                .public_fields
                .iter()
                .map(|field| {
                    format!(
                        "{}:{}",
                        json_string(&field.name.text),
                        self.openapi_field_schema(field)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let names = json_string_array(
                declaration
                    .public_fields
                    .iter()
                    .map(|field| field.name.text.as_str()),
            );
            error_properties.push(format!("\"details\":{{\"type\":\"object\",\"additionalProperties\":false,\"properties\":{{{properties}}},\"required\":{names}}}"));
            required.push("details");
        }
        error_properties.sort();
        required.sort();
        format!(
            "{{\"type\":\"object\",\"additionalProperties\":false,\"properties\":{{\"error\":{{\"type\":\"object\",\"additionalProperties\":false,\"properties\":{{{}}},\"required\":{}}}}},\"required\":[\"error\"]}}",
            error_properties.join(","),
            json_string_array(required.into_iter())
        )
    }

    fn openapi_type_schema(&self, name: &str) -> String {
        let base = name.trim_end_matches('?');
        let nullable =
            base.len() != name.len() || self.project.semantics.nullable_types.contains(base);
        let schema = if let Some(field) = self
            .fields
            .get(base)
            .filter(|_| base.starts_with("__jadpo_"))
        {
            self.openapi_field_schema(field)
        } else if self.fields.contains_key(base) {
            format!(
                "{{\"$ref\":{}}}",
                json_string(&format!("#/components/schemas/{base}"))
            )
        } else if let Some(record) = base
            .strip_suffix(".Ref")
            .and_then(|owner| self.records.get(owner))
        {
            let identity = record
                .dossier
                .as_ref()
                .map(|dossier| dossier.identity.text.as_str())
                .or_else(|| {
                    record
                        .fields
                        .iter()
                        .find(|field| field.persistence.contains(&PersistenceModifier::Identity))
                        .map(|field| field.name.text.as_str())
                });
            identity
                .and_then(|identity| {
                    record
                        .fields
                        .iter()
                        .find(|field| field.name.text == identity)
                })
                .map_or_else(|| "{}".to_owned(), |field| self.openapi_field_schema(field))
        } else if let Some(item) = base
            .strip_prefix("List<")
            .and_then(|value| value.strip_suffix('>'))
        {
            format!(
                "{{\"type\":\"array\",\"items\":{}}}",
                self.openapi_type_schema(item)
            )
        } else if let Some(item) = base
            .strip_prefix("Set<")
            .and_then(|value| value.strip_suffix('>'))
        {
            format!(
                "{{\"type\":\"array\",\"uniqueItems\":true,\"items\":{}}}",
                self.openapi_type_schema(item)
            )
        } else if let Some(arguments) = base
            .strip_prefix("Map<")
            .and_then(|value| value.strip_suffix('>'))
        {
            let value = split_generic_arguments(arguments)
                .get(1)
                .map_or_else(|| "{}".to_owned(), |value| self.openapi_type_schema(value));
            format!("{{\"type\":\"object\",\"additionalProperties\":{value}}}")
        } else {
            let resolved = self.schema_declaration(base);
            if let Some(record) = self
                .records
                .get(&resolved)
                .filter(|_| resolved.starts_with("__jadpo_"))
            {
                self.openapi_record_schema(record)
            } else if self.types.contains_key(&resolved)
                || self.enums.contains_key(&resolved)
                || self.records.contains_key(&resolved)
            {
                format!(
                    "{{\"$ref\":{}}}",
                    json_string(&format!("#/components/schemas/{resolved}"))
                )
            } else {
                format!("{{{}}}", scalar_schema_parts(&resolved).join(","))
            }
        };
        if nullable {
            format!("{{\"anyOf\":[{schema},{{\"type\":\"null\"}}]}}")
        } else {
            schema
        }
    }

    fn reference_is_nullable(&self, reference: &TypeReference) -> bool {
        reference.nullable
            || self.project.semantics.nullable_types.contains(
                &reference
                    .path
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
            )
    }

    fn schema_declaration(&self, name: &str) -> String {
        if matches!(name, "Email" | "Url" | "IpAddress") {
            return name.to_owned();
        }
        let mut current = name.to_owned();
        let mut visited = BTreeSet::new();
        while visited.insert(current.clone()) {
            if matches!(current.as_str(), "Email" | "Url" | "IpAddress") {
                return current;
            }
            if self.types.contains_key(&current)
                || self.enums.contains_key(&current)
                || self.records.contains_key(&current)
            {
                return current;
            }
            let Some(node) = self.project.semantics.node(&current) else {
                break;
            };
            let Some(edge) = self
                .project
                .semantics
                .refinements
                .iter()
                .find(|edge| edge.refined == node.id)
            else {
                break;
            };
            current.clone_from(&self.project.semantics.nodes[edge.parent.0 as usize].name);
        }
        current
    }
}

struct RouteFailureView<'a> {
    name: &'a str,
    code: &'a str,
    http_status: u16,
}

fn field_plan_json(field: &FieldDeclaration, nullable: bool) -> String {
    format!(
        "{{\"name\":{},\"type\":{},\"optional\":{},\"nullable\":{},\"constraints\":{}}}",
        json_string(&field.name.text),
        json_string(&type_name(&field.field_type)),
        field.optional,
        nullable,
        constraints_json(&field.constraints)
    )
}

fn constraints_json(constraints: &[Constraint]) -> String {
    json_array(constraints.iter().map(|constraint| {
        format!(
            "{{\"kind\":{},\"value\":{}}}",
            json_string(constraint_name(constraint.kind)),
            literal_json(constraint)
        )
    }))
}

fn literal_json(constraint: &Constraint) -> String {
    match constraint.value.kind {
        LiteralKind::Integer | LiteralKind::Decimal | LiteralKind::Boolean => {
            constraint.value.text.clone()
        }
        LiteralKind::String => json_string(unquote(&constraint.value.text)),
        LiteralKind::None => "null".to_owned(),
    }
}

fn openapi_constraint_parts(constraints: &[Constraint]) -> Vec<String> {
    constraints
        .iter()
        .map(|constraint| {
            let key = match constraint.kind {
                ConstraintKind::Min => "minimum",
                ConstraintKind::Max => "maximum",
                ConstraintKind::MinLength => "minLength",
                ConstraintKind::MaxLength => "maxLength",
                ConstraintKind::Pattern => "pattern",
                ConstraintKind::Format => "format",
            };
            format!("{}:{}", json_string(key), literal_json(constraint))
        })
        .collect()
}

fn scalar_schema_parts(name: &str) -> Vec<String> {
    match name {
        "Bool" => vec!["\"type\":\"boolean\"".to_owned()],
        "Int" => vec!["\"type\":\"integer\"".to_owned()],
        "Decimal" => vec!["\"type\":\"number\"".to_owned()],
        "Uuid" => vec![
            "\"type\":\"string\"".to_owned(),
            "\"format\":\"uuid\"".to_owned(),
        ],
        "Instant" => vec![
            "\"type\":\"string\"".to_owned(),
            "\"format\":\"date-time\"".to_owned(),
        ],
        "CalendarDate" => vec![
            "\"type\":\"string\"".to_owned(),
            "\"format\":\"date\"".to_owned(),
        ],
        "Time" => vec![
            "\"type\":\"object\"".to_owned(),
            "\"additionalProperties\":false".to_owned(),
            "\"required\":[\"instant\",\"zone\"]".to_owned(),
            "\"properties\":{\"instant\":{\"type\":\"string\",\"format\":\"date-time\"},\"zone\":{\"type\":\"string\"}}".to_owned(),
        ],
        "Zone" | "Locale" | "PresentationText" => vec!["\"type\":\"string\"".to_owned()],
        "Duration" => vec![
            "\"type\":\"string\"".to_owned(),
            "\"format\":\"duration\"".to_owned(),
        ],
        "Bytes" => vec![
            "\"type\":\"string\"".to_owned(),
            "\"contentEncoding\":\"base64\"".to_owned(),
        ],
        "Email" => vec![
            "\"type\":\"string\"".to_owned(),
            "\"format\":\"email\"".to_owned(),
            "\"maxLength\":254".to_owned(),
        ],
        "Url" => vec![
            "\"type\":\"string\"".to_owned(),
            "\"format\":\"uri\"".to_owned(),
        ],
        "IpAddress" => vec![
            "\"type\":\"string\"".to_owned(),
            "\"format\":\"ip\"".to_owned(),
        ],
        _ => vec!["\"type\":\"string\"".to_owned()],
    }
}

fn split_generic_arguments(arguments: &str) -> Vec<&str> {
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut values = Vec::new();
    for (index, character) in arguments.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                values.push(arguments[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    values.push(arguments[start..].trim());
    values
}

fn type_name(reference: &TypeReference) -> String {
    let mut name = reference
        .path
        .iter()
        .map(|part| part.text.as_str())
        .collect::<Vec<_>>()
        .join(".");
    if !reference.arguments.is_empty() {
        name.push('<');
        name.push_str(
            &reference
                .arguments
                .iter()
                .map(type_name)
                .collect::<Vec<_>>()
                .join(","),
        );
        name.push('>');
    }
    if reference.nullable {
        name.push('?');
    }
    name
}

fn method_name(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Delete => "DELETE",
    }
}

fn record_kind(kind: RecordKind) -> &'static str {
    match kind {
        RecordKind::Entity | RecordKind::Value | RecordKind::Input | RecordKind::Output => "object",
    }
}

fn constraint_name(kind: ConstraintKind) -> &'static str {
    match kind {
        ConstraintKind::Min => "min",
        ConstraintKind::Max => "max",
        ConstraintKind::MinLength => "min_length",
        ConstraintKind::MaxLength => "max_length",
        ConstraintKind::Pattern => "pattern",
        ConstraintKind::Format => "format",
    }
}

fn json_optional_string(value: Option<&str>) -> String {
    value.map(json_string).unwrap_or_else(|| "null".to_owned())
}

fn json_string_array<'a>(values: impl IntoIterator<Item = &'a str>) -> String {
    json_array(values.into_iter().map(json_string))
}

fn json_array(values: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", values.into_iter().collect::<Vec<_>>().join(","))
}

fn json_string(value: &str) -> String {
    format!("\"{}\"", escape_json(value))
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::{derive_artifacts, write_artifacts, GeneratedArtifact};
    use crate::{analyze_project, analyze_sources};
    use jadpo_syntax::SourceFile;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn repository_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("compiler crate should be inside the repository")
    }

    #[test]
    fn derives_the_complete_seed_artifact_set() {
        let seed = repository_root().join("examples/jadpo-seed");
        let analyzed = analyze_project(&seed).expect("seed should be analyzable");
        let artifacts = derive_artifacts(&seed, &analyzed);
        let paths = artifacts
            .iter()
            .map(|artifact| artifact.relative_path)
            .collect::<Vec<_>>();

        assert_eq!(
            paths,
            vec![
                "app.meta.json",
                "inventory/routes.json",
                "inventory/callables.json",
                "audit/failures.json",
                "audit/entities.json",
                "audit/transactions.json",
                "audit/configuration.json",
                "audit/policy.json",
                "approval/subject.json",
                "approval/subject.txt",
                "validators/plan.json",
                "compatibility/public-failure-codes.json",
                "openapi/openapi.json",
                "diagnostics/catalogue.json",
                "diagnostics/reference.md",
            ]
        );
        assert!(artifact(&artifacts, "inventory/routes.json").contains("\"auth\":\"none\""));
        let subject = artifact(&artifacts, "approval/subject.json");
        let text_subject = artifact(&artifacts, "approval/subject.txt");
        assert!(subject.contains("\"kind\":\"approval_subject\""));
        assert!(subject.contains("\"before\":null"));
        assert!(subject.contains("\"subject_digest\":\"sha256:"));
        assert!(text_subject.starts_with("Jadpo behavioral review v5"));
        assert!(text_subject.contains(&subject[..subject.len() - 1]));
        assert!(artifact(&artifacts, "diagnostics/catalogue.json")
            .contains("\"ruleId\":\"failure.attempt_required\""));
        assert!(
            artifact(&artifacts, "audit/failures.json").contains("\"internal_to_client\":false")
        );
        assert!(artifact(&artifacts, "openapi/openapi.json").contains("\"422\""));
    }

    #[test]
    fn emits_a_scoped_audit_for_checked_service_effects() {
        let path = repository_root().join("tests/compile/pass/170_checked_service_operation.jadpo");
        let source = fs::read_to_string(&path).expect("service fixture should be readable");
        let analyzed = analyze_sources(vec![SourceFile::new(path.clone(), source)])
            .expect("service fixture should analyze");
        assert!(analyzed
            .semantics
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != jadpo_diagnostics::Severity::Error));

        let artifacts = derive_artifacts(&path, &analyzed);
        let service_audit = artifacts
            .iter()
            .find(|artifact| artifact.relative_path == "audit/services.json")
            .expect("checked service should have an audit artifact");
        let json: serde_json::Value =
            serde_json::from_str(&service_audit.contents).expect("service audit should be JSON");
        assert_eq!(json["kind"], "service_effect_contract");
        assert_eq!(json["runtime_conformance"], "not_established");
        assert_eq!(json["effects"][0]["egress"], "mail.example.invalid:443");
        assert_eq!(json["effects"][0]["retry"]["max_attempts"], 3);
        assert_eq!(
            json["effects"][0]["outcome_mappings"]
                .as_array()
                .unwrap()
                .len(),
            10
        );
        assert_eq!(
            json["effects"][0]["schema_parity"]["input"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
        let title_parity = json["effects"][0]["schema_parity"]["input"]
            .as_array()
            .unwrap()
            .iter()
            .find(|field| field["field"] == "todo_title")
            .expect("audit should identify the checked title field");
        assert_eq!(
            title_parity["imported_shape"],
            "string,minLength=1,maxLength=200"
        );
    }

    #[test]
    fn approval_subject_digest_is_stable_and_changes_with_checked_graph() {
        let seed = repository_root().join("examples/jadpo-seed");
        let analyzed = analyze_project(&seed).expect("seed should be analyzable");
        let first =
            artifact(&derive_artifacts(&seed, &analyzed), "approval/subject.json").to_owned();
        let second =
            artifact(&derive_artifacts(&seed, &analyzed), "approval/subject.json").to_owned();
        assert_eq!(first, second);
        assert!(first.contains("\"before\":null"));

        let other_path = repository_root().join("tests/compile/pass/59_p106_failure_route.jadpo");
        let other = analyze_project(&other_path).expect("route fixture should be analyzable");
        let other_artifacts = derive_artifacts(&other_path, &other);
        let other_subject = artifact(&other_artifacts, "approval/subject.json");
        assert_ne!(first, other_subject);
    }

    #[test]
    fn exposes_enum_variants_to_validators_and_openapi() {
        let fixture =
            repository_root().join("tests/compile/pass/53_plain_enum_exhaustive_match.jadpo");
        let analyzed = analyze_project(&fixture).expect("enum fixture should be analyzable");
        let artifacts = derive_artifacts(&fixture, &analyzed);

        assert!(artifact(&artifacts, "validators/plan.json").contains(
            "\"enums\":[{\"name\":\"DeliveryState\",\"tagged\":false,\"variants\":[\"pending\",\"sent\",\"failed\"]"
        ));
        assert!(artifact(&artifacts, "openapi/openapi.json").contains(
            "\"DeliveryState\":{\"type\":\"string\",\"enum\":[\"pending\",\"sent\",\"failed\"]}"
        ));
    }

    #[test]
    fn exposes_proved_restricted_field_reads_in_policy_audit() {
        let fixture = repository_root().join("tests/compile/pass/134_policy_safe_projection.jadpo");
        let analyzed = analyze_project(&fixture).expect("policy fixture should be analyzable");
        let artifacts = derive_artifacts(&fixture, &analyzed);
        let policy = artifact(&artifacts, "audit/policy.json");

        assert!(policy.contains("\"operation\":\"PrivateNote.read_private_label\""));
        assert!(policy.contains("\"restricted_field_reads\":[{\"entity\":\"PrivateNote\",\"field\":\"private_label\",\"subjects\":[\"NoteRole.owner\"]"));
        assert!(policy.contains("\"judgement\":\"proved\""));
    }

    #[test]
    fn exposes_nested_objects_collections_and_validated_prelude_types() {
        let fixture =
            repository_root().join("tests/compile/pass/108_unified_types_and_persistence.jadpo");
        let analyzed = analyze_project(&fixture).expect("unified type fixture should analyze");
        let artifacts = derive_artifacts(&fixture, &analyzed);
        let openapi = artifact(&artifacts, "openapi/openapi.json");
        let validators = artifact(&artifacts, "validators/plan.json");

        assert!(openapi.contains("\"addresses\":{\"type\":\"array\""));
        assert!(openapi.contains("\"format\":\"email\""));
        assert!(openapi.contains("\"format\":\"uri\""));
        assert!(openapi.contains("\"format\":\"ip\""));
        assert!(!openapi.contains("__jadpo_"));
        assert!(validators.contains("\"kind\":\"object\",\"persistent\":true"));
        assert!(!validators.contains("\"kind\":\"output\""));
    }

    #[test]
    fn exposes_typed_route_paths_and_behavior_in_inventory_and_openapi() {
        let fixture = repository_root().join("tests/compile/pass/59_p106_failure_route.jadpo");
        let analyzed = analyze_project(&fixture).expect("route fixture should be analyzable");
        let artifacts = derive_artifacts(&fixture, &analyzed);
        let inventory = artifact(&artifacts, "inventory/routes.json");
        assert!(inventory
            .contains("\"path_fields\":[{\"name\":\"customer_id\",\"type\":\"Customer.id\"}]"));
        assert!(inventory.contains("\"behavior\":\"run\""));

        let openapi = artifact(&artifacts, "openapi/openapi.json");
        assert!(openapi.contains("\"name\":\"customer_id\",\"in\":\"path\",\"required\":true"));
        assert!(openapi.contains("\"format\":\"uuid\""));

        let inline_fixture =
            repository_root().join("tests/compile/pass/66_inline_action_failure_surface.jadpo");
        let inline = analyze_project(&inline_fixture).expect("inline route should be analyzable");
        let inline_artifacts = derive_artifacts(&inline_fixture, &inline);
        assert!(artifact(&inline_artifacts, "inventory/routes.json")
            .contains("\"name\":\"Refused\",\"code\":\"refused\",\"http_status\":422"));
        assert!(artifact(&inline_artifacts, "openapi/openapi.json")
            .contains("\"422\":{\"description\":\"Refused\""));
    }

    #[test]
    fn exposes_route_deadline_in_inventory_and_openapi() {
        let fixture = repository_root().join("tests/compile/pass/171_route_deadline.jadpo");
        let analyzed = analyze_project(&fixture).expect("deadline route fixture should analyze");
        let artifacts = derive_artifacts(&fixture, &analyzed);
        let inventory = artifact(&artifacts, "inventory/routes.json");
        assert!(inventory.contains("\"deadline_ms\":1500"));

        let openapi = artifact(&artifacts, "openapi/openapi.json");
        assert!(openapi.contains("\"x-jadpo-deadline-ms\":1500"));
        assert!(openapi.contains(
            "\"504\":{\"description\":\"The route operation exceeded its declared deadline\"}"
        ));
    }

    #[test]
    fn openapi_input_schema_allows_omission_for_defaulted_nullable_field() {
        let source_path = Path::new("input-default.jadpo");
        let source = r#"input CreateTodo { title: Text due_at: Instant? default none }
output TodoView { due_at: Instant? }
route POST /todos {
    auth: none
    input: CreateTodo
    output: TodoView
    action: { return TodoView { due_at: input.due_at } }
}
"#;
        let project = analyze_sources(vec![jadpo_syntax::SourceFile::new(
            source_path.to_path_buf(),
            source.to_owned(),
        )])
        .expect("input default source should analyze");
        assert!(
            project.typing.diagnostics.is_empty(),
            "{:?}",
            project.typing.diagnostics
        );

        let artifacts = derive_artifacts(source_path, &project);
        let openapi = artifact(&artifacts, "openapi/openapi.json");
        assert!(openapi.contains("\"required\":[\"title\"]"));
        assert!(!openapi.contains("\"required\":[\"title\",\"due_at\"]"));
    }

    #[test]
    fn exposes_tagged_sum_payloads_and_discriminator_to_openapi() {
        let fixture = repository_root().join("tests/compile/pass/56_tagged_sum_match.jadpo");
        let analyzed = analyze_project(&fixture).expect("tagged-sum fixture should analyze");
        let artifacts = derive_artifacts(&fixture, &analyzed);

        assert!(artifact(&artifacts, "validators/plan.json")
            .contains("\"name\":\"PaymentOutcome\",\"tagged\":true"));
        let openapi = artifact(&artifacts, "openapi/openapi.json");
        assert!(openapi.contains("\"discriminator\":{\"propertyName\":\"tag\"}"));
        assert!(openapi.contains("\"const\":\"paid\""));
        assert!(openapi.contains("\"receipt_id\""));
    }

    #[test]
    fn repeated_emission_is_byte_identical() {
        let temp = temporary_project("repeat");
        fs::create_dir_all(&temp).expect("temp project should be created");
        fs::copy(
            repository_root().join("examples/jadpo-seed/app.jadpo"),
            temp.join("app.jadpo"),
        )
        .expect("seed source should be copied");

        let analyzed = analyze_project(&temp).expect("temp seed should be analyzable");
        let artifacts = derive_artifacts(&temp, &analyzed);
        let first_root = write_artifacts(&temp, &artifacts).expect("first emission should work");
        let first = read_outputs(&first_root, &artifacts);
        let second_root = write_artifacts(&temp, &artifacts).expect("second emission should work");
        let second = read_outputs(&second_root, &artifacts);

        assert_eq!(first, second);
        fs::remove_dir_all(&temp).expect("temp project should be removable");
    }

    #[test]
    fn failed_staging_preserves_the_last_complete_build() {
        let temp = temporary_project("failure");
        fs::create_dir_all(&temp).expect("temp project should be created");
        let baseline = [GeneratedArtifact {
            relative_path: "target/app.ts",
            contents: "baseline\n".to_owned(),
        }];
        let output = write_artifacts(&temp, &baseline).expect("baseline should be written");

        let invalid = [
            GeneratedArtifact {
                relative_path: "conflict",
                contents: "file blocks directory\n".to_owned(),
            },
            GeneratedArtifact {
                relative_path: "conflict/child",
                contents: "cannot be written\n".to_owned(),
            },
        ];
        let diagnostic = write_artifacts(&temp, &invalid).expect_err("staging should fail");

        assert_eq!(diagnostic.code, "JADPO_ARTIFACT_WRITE_FAILED");
        assert_eq!(
            fs::read_to_string(output.join("target/app.ts"))
                .expect("last complete build should remain"),
            "baseline\n"
        );
        assert!(!output.join("conflict").exists());
        assert!(fs::read_dir(&temp)
            .expect("temp project should be readable")
            .all(|entry| !entry
                .expect("entry should be readable")
                .file_name()
                .to_string_lossy()
                .starts_with(".jadpo-build-stage.")));
        fs::remove_dir_all(&temp).expect("temp project should be removable");
    }

    #[test]
    fn metadata_is_independent_of_relative_or_absolute_invocation() {
        let seed = repository_root().join("examples/jadpo-seed");
        let absolute = analyze_project(&seed).expect("absolute seed should be analyzable");
        let absolute_artifacts = derive_artifacts(&seed, &absolute);

        let current = std::env::current_dir().expect("current directory should be available");
        let relative_path = relative_path(&current, &seed);
        let relative = analyze_project(&relative_path).expect("relative seed should be analyzable");
        let relative_artifacts = derive_artifacts(&relative_path, &relative);

        assert_eq!(
            artifact(&absolute_artifacts, "app.meta.json"),
            artifact(&relative_artifacts, "app.meta.json")
        );
    }

    fn artifact<'a>(artifacts: &'a [super::GeneratedArtifact], path: &str) -> &'a str {
        &artifacts
            .iter()
            .find(|artifact| artifact.relative_path == path)
            .expect("artifact should exist")
            .contents
    }

    fn read_outputs(root: &Path, artifacts: &[super::GeneratedArtifact]) -> Vec<(String, Vec<u8>)> {
        artifacts
            .iter()
            .map(|artifact| {
                (
                    artifact.relative_path.to_owned(),
                    fs::read(root.join(artifact.relative_path))
                        .expect("artifact should be readable"),
                )
            })
            .collect()
    }

    fn temporary_project(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "jadpo-compiler-artifacts-{}-{name}",
            std::process::id(),
        ))
    }

    fn relative_path(from: &Path, to: &Path) -> PathBuf {
        let from = from
            .canonicalize()
            .expect("working directory should canonicalize");
        let to = to.canonicalize().expect("seed path should canonicalize");
        let from = from.components().collect::<Vec<_>>();
        let to = to.components().collect::<Vec<_>>();
        let shared = from
            .iter()
            .zip(&to)
            .take_while(|(left, right)| left == right)
            .count();
        let mut relative = PathBuf::new();
        for _ in shared..from.len() {
            relative.push("..");
        }
        for component in &to[shared..] {
            relative.push(component.as_os_str());
        }
        relative
    }
}

fn bun_operational_failures_json(relationships: bool) -> String {
    let base = r#"[{"code":"internal_fault","http_status":500,"message":"An internal error occurred.","condition":"contained_runtime_defect"},{"code":"transaction_unavailable","semantic_http_default":503,"http_status":503,"message":"A temporary storage issue prevented the operation.","condition":"retryable_no_commit_not_retried_or_exhausted","automatic_retry":false},{"code":"read_unavailable","semantic_http_default":503,"http_status":503,"message":"A temporary storage issue prevented the read.","condition":"checked_query_only_operation_with_listed_structured_driver_cause","automatic_retry":false},{"code":"outcome_unknown","semantic_http_default":null,"http_status":500,"message":"The operation may have completed.","condition":"effect_may_have_occurred","automatic_retry":false}]"#;
    if relationships {
        format!(
            "{},{}]",
            &base[..base.len() - 1],
            operational_failures::RELATIONSHIP_FAILURE
        )
    } else {
        base.to_owned()
    }
}

fn operational_wire_pairs(status: u16) -> Vec<(&'static str, &'static str)> {
    match status {
        409 => vec![(
            "relationship_conflict",
            "The operation conflicts with a relationship.",
        )],
        503 => vec![
            (
                "transaction_unavailable",
                "A temporary storage issue prevented the operation.",
            ),
            (
                "read_unavailable",
                "A temporary storage issue prevented the read.",
            ),
        ],
        _ => vec![
            ("internal_fault", "An internal error occurred."),
            ("outcome_unknown", "The operation may have completed."),
        ],
    }
}
fn bun_operational_schema_json(status: u16) -> String {
    let pairs = operational_wire_pairs(status);
    let codes = pairs.iter().map(|(code, _)| *code).collect::<Vec<_>>();
    let mut schema = serde_json::json!({"type":"object","additionalProperties":false,"required":["error"],"properties":{
        "error":{"type":"object","additionalProperties":false,"required":["code","message","request_id"],"properties":{
            "code":{"type":"string","enum":codes},"message": if status == 409 { serde_json::json!({"type":"string","enum":["The operation conflicts with a relationship."]}) } else { serde_json::json!({"type":"string"}) },"request_id":{"type":"string"}
        }}
    }});
    if status != 409 {
        // Preserve code.enum while binding native codes to their actual static
        // safe messages. Custom declared messages keep their own references.
        schema["properties"]["error"]["oneOf"] = serde_json::Value::Array(
            pairs
                .iter()
                .map(|(code, message)| {
                    serde_json::json!({"properties":{
                        "code":{"const":code},"message":{"const":message}
                    }})
                })
                .collect(),
        );
    }
    schema.to_string()
}
