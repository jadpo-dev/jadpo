//! Atomic static finish of inspection-only bindings, private graph and facts.
//! Incomplete source remains unsupported; no runtime authority is issued here.
use crate::{DeliveryHookCandidate, DeliveryPhaseCandidate, ParsedProject};
use jadpo_diagnostics::{Diagnostic, Severity, SourceSpan};
use jadpo_semantic::{
    CheckedJobBinding, ExternalServiceEffect, FailureCheckResult, JobFailureSet, NodeId,
    SemanticGraph, SemanticNode, TypeCheckResult,
};
use jadpo_syntax::{Declaration, JobDeclaration, JobReminderDelivery};
use std::collections::{BTreeMap, BTreeSet};
mod graph;
pub use graph::{DeliveryPrivateEdge, DeliveryPrivateGraph, DeliveryPrivateNode};

/// A nonexecuting, statically finished binding. There is no public constructor.
/// Finishing source never establishes live authority or runtime conformance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedReminderDeliveryBinding {
    source_job: JobDeclaration,
    schedule: CheckedJobBinding,
    descriptor: JobReminderDelivery,
    phases: DeliveryPhaseCandidate,
    hooks: DeliveryHookCandidate,
    private_graph: DeliveryPrivateGraph,
    service_effect: ExternalServiceEffect,
    run_failure_contract: JobFailureSet,
}
impl CheckedReminderDeliveryBinding {
    pub(crate) fn matches_source_job(&self, job: &JobDeclaration) -> bool {
        &self.source_job == job
    }
    pub(crate) fn service_effect_for_lowering(&self) -> &ExternalServiceEffect {
        &self.service_effect
    }
    pub(crate) fn selection_for_lowering(
        &self,
    ) -> crate::delivery_phases::DeliverySelectionLowering<'_> {
        self.phases.selection_for_lowering()
    }
    pub(crate) fn authority_for_lowering(
        &self,
    ) -> crate::delivery_phases::DeliveryAuthorityLowering<'_> {
        self.phases.authority_for_lowering()
    }
    pub fn schedule(&self) -> &CheckedJobBinding {
        &self.schedule
    }
    pub fn binding_identity(&self) -> &str {
        self.phases.binding_candidate()
    }
    pub fn descriptor(&self) -> &JobReminderDelivery {
        &self.descriptor
    }
    pub fn run_failure_contract(&self) -> &JobFailureSet {
        &self.run_failure_contract
    }
    pub fn private_graph(&self) -> &DeliveryPrivateGraph {
        &self.private_graph
    }
    pub fn source_facts(&self) -> serde_json::Value {
        let effect = &self.service_effect;
        serde_json::json!({"schema_version":1, "status":"checked_nonexecuting_binding",
            "binding":self.binding_identity(), "job":self.schedule.job,
            "schedule":{"callee":self.schedule.callee,"snapshot_type":self.schedule.snapshot_type,
                "interval_ms":self.schedule.interval_ms,"constructor_proof":self.schedule.constructor_proof},
            "run_failure_contract":{"job":self.run_failure_contract.job,"callee":self.run_failure_contract.callee,
                "failures":self.run_failure_contract.failures,"may_suspend":self.run_failure_contract.may_suspend},
            "hooks":self.hooks.source_facts(), "obligations":self.phases.obligation_source_facts(),
            "private_graph":self.private_graph.source_facts(),
            "service_contract":{"operation":format!("{}.{}",effect.service,effect.operation),
                "input":effect.input,"output":effect.output,"intent_type":effect.idempotency_type,
                "import":{"file":effect.imported_contract,"version":effect.import_version,"sha256":effect.import_sha256},
                "credential_slot":effect.credential_slot,"credential_header":effect.credential_header,
                "egress":effect.egress,"timeout_ms":effect.timeout_ms,"max_attempts":effect.max_attempts,
                "max_elapsed_ms":effect.max_elapsed_ms,"jitter":effect.jitter,"proxy_allowed":effect.proxy_allowed,
                "redirects_allowed":effect.redirects_allowed,"outcomes":effect.outcomes,"mappings":effect.outcome_mappings,
                "schema_parity":serde_json::from_str::<serde_json::Value>(&effect.schema_parity_json()).expect("checked parity JSON")},
            "execution_profile":null,"runtime_lowering_supported":true,"live_authority_established":false})
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeliveryModel {
    bindings: Vec<CheckedReminderDeliveryBinding>,
}
impl DeliveryModel {
    pub fn bindings(&self) -> &[CheckedReminderDeliveryBinding] {
        &self.bindings
    }
    pub(crate) fn covers_authored_delivery(&self, syntax: &ParsedProject) -> bool {
        let declarations = syntax
            .sources
            .iter()
            .flat_map(|file| {
                file.file.declarations.iter().filter_map(|declaration| {
                    let Declaration::Job(job) = declaration else {
                        return None;
                    };
                    job.delivery.as_ref()?;
                    Some((job.name.text.as_str(), job))
                })
            })
            .collect::<Vec<_>>();
        declarations.len() == self.bindings.len()
            && declarations.iter().all(|(name, job)| {
                self.bindings
                    .iter()
                    .filter(|binding| {
                        binding.schedule.job == *name && binding.matches_source_job(job)
                    })
                    .count()
                    == 1
            })
    }
}

/// The sole constructor of completed bindings, called after every source gate.
/// Validate all descriptors/candidates before committing any row. No diagnostics
/// are filtered or suppressed here. Unsupported belongs to incomplete finish.
pub(crate) fn finish_delivery_candidates(
    syntax: &ParsedProject,
    graph: &mut SemanticGraph,
    typing: &TypeCheckResult,
    failures: &FailureCheckResult,
    phase_candidates: &[DeliveryPhaseCandidate],
    hook_candidates: &[DeliveryHookCandidate],
) -> (DeliveryModel, Vec<Diagnostic>) {
    let has_errors = syntax
        .diagnostics()
        .chain(graph.diagnostics.iter())
        .chain(typing.diagnostics.iter())
        .chain(failures.diagnostics.iter())
        .any(|diagnostic| diagnostic.severity == Severity::Error);
    let incomplete = || {
        let mut diagnostics = syntax
            .sources
            .iter()
            .flat_map(|file| {
                file.file
                    .declarations
                    .iter()
                    .filter_map(move |declaration| {
                        let Declaration::Job(job) = declaration else {
                            return None;
                        };
                        let descriptor = job.delivery.as_ref()?;
                        let mut diagnostic =
                            Diagnostic::error("TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED");
                        diagnostic.primary = Some(SourceSpan {
                            source: file.source_name.clone(),
                            start: descriptor.range.start,
                            end: descriptor.range.end,
                        });
                        Some(diagnostic)
                    })
            })
            .collect::<Vec<_>>();
        if diagnostics.is_empty() {
            diagnostics.push(Diagnostic::error(
                "TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED",
            ));
        }
        (DeliveryModel::default(), diagnostics)
    };
    let declarations = syntax
        .sources
        .iter()
        .flat_map(|file| {
            file.file
                .declarations
                .iter()
                .filter_map(move |declaration| {
                    let Declaration::Job(job) = declaration else {
                        return None;
                    };
                    Some((
                        job.name.text.as_str(),
                        (file.source_name.as_str(), {
                            job.delivery.as_ref()?;
                            job
                        }),
                    ))
                })
        })
        .collect::<Vec<_>>();
    let declared = declarations
        .iter()
        .map(|(name, _)| *name)
        .collect::<BTreeSet<_>>();
    if has_errors {
        // Preserve every original diagnostic. Mark authored delivery incomplete
        // as well, without duplicating an existing unsupported diagnostic or
        // adding a delivery error to an ordinary invalid source.
        let already_unsupported = syntax
            .diagnostics()
            .chain(graph.diagnostics.iter())
            .chain(typing.diagnostics.iter())
            .chain(failures.diagnostics.iter())
            .any(|diagnostic| diagnostic.code == "TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED");
        return if declarations.is_empty() || already_unsupported {
            (DeliveryModel::default(), Vec::new())
        } else {
            incomplete()
        };
    }
    let all_jobs = syntax
        .sources
        .iter()
        .flat_map(|file| {
            file.file.declarations.iter().filter_map(|declaration| {
                let Declaration::Job(job) = declaration else {
                    return None;
                };
                Some(job.name.text.as_str())
            })
        })
        .collect::<BTreeSet<_>>();
    if failures.jobs.len() != all_jobs.len()
        || failures
            .jobs
            .iter()
            .map(|failure| failure.job.as_str())
            .collect::<BTreeSet<_>>()
            != all_jobs
    {
        return incomplete();
    }
    let types = typing
        .delivery_candidates
        .iter()
        .map(|candidate| (candidate.schedule_candidate().job.as_str(), candidate))
        .collect::<BTreeMap<_, _>>();
    let phases = phase_candidates
        .iter()
        .map(|candidate| (candidate.job_candidate(), candidate))
        .collect::<BTreeMap<_, _>>();
    let hooks = hook_candidates
        .iter()
        .map(|candidate| (candidate.job_candidate(), candidate))
        .collect::<BTreeMap<_, _>>();
    // Cardinality before keyed lookup prevents duplicate collapse or orphan rows.
    if declared.len() != declarations.len()
        || types.len() != typing.delivery_candidates.len()
        || phases.len() != phase_candidates.len()
        || hooks.len() != hook_candidates.len()
        || types.len() != declared.len()
        || phases.len() != declared.len()
        || hooks.len() != declared.len()
        || types.keys().copied().collect::<BTreeSet<_>>() != declared
        || phases.keys().copied().collect::<BTreeSet<_>>() != declared
        || hooks.keys().copied().collect::<BTreeSet<_>>() != declared
    {
        return incomplete();
    }
    let mut ready = Vec::new();
    for (job, (source, source_job)) in declarations {
        let descriptor = source_job.delivery.as_ref().expect("authored delivery");
        let types = types[job];
        let phases = phases[job];
        let hooks = hooks[job];
        if types.source() != source
            || types.descriptor_candidate() != descriptor
            || hooks.source_candidate() != source
            || phases.binding_candidate() != format!("job.{job}.delivery.reminder_v1")
            || typing.jobs.iter().any(|ordinary| ordinary.job == job)
        {
            return incomplete();
        }
        let contracts = failures
            .jobs
            .iter()
            .filter(|failure| {
                failure.job == job && failure.callee == types.schedule_candidate().callee
            })
            .collect::<Vec<_>>();
        let [run_failure_contract] = contracts.as_slice() else {
            return incomplete();
        };
        let Some(private_graph) = DeliveryPrivateGraph::resolve(phases, hooks, graph) else {
            return incomplete();
        };
        let effects = graph
            .external_effects
            .iter()
            .filter(|effect| {
                format!("{}.{}", effect.service, effect.operation)
                    == types.service_operation_candidate()
            })
            .collect::<Vec<_>>();
        let [service_effect] = effects.as_slice() else {
            return incomplete();
        };
        ready.push(CheckedReminderDeliveryBinding {
            source_job: source_job.clone(),
            schedule: types.schedule_candidate().clone(),
            descriptor: descriptor.clone(),
            phases: phases.clone(),
            hooks: hooks.clone(),
            private_graph,
            service_effect: (*service_effect).clone(),
            run_failure_contract: (*run_failure_contract).clone(),
        });
    }
    ready.sort_by(|left, right| left.schedule.job.cmp(&right.schedule.job));
    // Stage ALL nodes first, then commit once. This never adds ordinary calls.
    let mut staged_nodes = Vec::new();
    for binding in &ready {
        let candidate = types[binding.schedule.job.as_str()];
        for node in binding.private_graph.nodes() {
            let Ok(id) = u32::try_from(graph.nodes.len() + staged_nodes.len()) else {
                return incomplete();
            };
            staged_nodes.push(SemanticNode {
                id: NodeId(id),
                kind: node.kind(),
                name: node.identity().to_owned(),
                source: candidate.source().to_owned(),
                range: binding.descriptor.range,
            });
        }
    }
    graph.nodes.extend(staged_nodes);
    (DeliveryModel { bindings: ready }, Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn any_error_stream_prevents_atomic_finish() {
        let parsed = jadpo_syntax::parse(
            std::path::Path::new("fixture.jadpo"),
            include_str!("../../../../tests/validation/fixtures/reminder-delivery-v1.jadpo"),
        );
        assert!(parsed.diagnostics.is_empty());
        // No type/phase candidates: the error-free input must fail closed,
        // rather than silently drop its authored descriptor.
        for (stream, code) in (0..=4).flat_map(|stream| {
            [
                "TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED",
                "TYPE_NAME_UNRESOLVED",
            ]
            .into_iter()
            .map(move |code| (stream, code))
        }) {
            let mut graph = jadpo_semantic::build_semantic_graph(&[]);
            let mut typing = TypeCheckResult::default();
            let mut failures = FailureCheckResult::default();
            let mut syntax = ParsedProject {
                sources: vec![parsed.clone()],
            };
            let existing = Diagnostic::error(code);
            match stream {
                0 => syntax.sources[0].diagnostics.push(existing),
                1 => graph.diagnostics.push(existing),
                2 => typing.diagnostics.push(existing),
                3 => failures.diagnostics.push(existing),
                _ => {}
            }
            let (model, diagnostics) =
                finish_delivery_candidates(&syntax, &mut graph, &typing, &failures, &[], &[]);
            assert!(model.bindings().is_empty());
            assert_eq!(
                diagnostics.len(),
                usize::from(stream == 4 || code != "TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED")
            );
            if stream < 4 {
                assert!(syntax
                    .diagnostics()
                    .chain(graph.diagnostics.iter())
                    .chain(typing.diagnostics.iter())
                    .chain(failures.diagnostics.iter())
                    .any(|diagnostic| diagnostic.code == code));
            }
        }
    }

    #[test]
    fn grounded_candidate_corruption_never_partially_commits() {
        use jadpo_semantic::NodeKind;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let mut sources =
            crate::discover_sources(&root.join("examples/golden-todo-migration")).unwrap();
        sources.retain(|source| !source.path.ends_with("jobs/reminders.jadpo"));
        sources.push(jadpo_syntax::SourceFile::new(
            root.join("tests/validation/fixtures/reminder-delivery-v1.jadpo"),
            include_str!("../../../../tests/validation/fixtures/reminder-delivery-v1.jadpo")
                .to_owned(),
        ));
        let original = crate::analyze_sources(sources).unwrap();
        assert_eq!(original.delivery_model().bindings().len(), 1);
        // Recreate the exact pre-finish graph from the already checked pipeline:
        // only its two terminal compiler-private nodes are removed. No source
        // diagnostics are cleared to manufacture a successful fixture.
        for mutation in 0..10 {
            let mut graph = original.semantics.clone();
            graph.nodes.retain(|node| {
                !matches!(
                    node.kind,
                    NodeKind::DeliverySelection | NodeKind::DeliveryCompletion
                )
            });
            let before = graph.nodes.clone();
            let mut typing = original.typing.clone();
            let mut failures = original.failures.clone();
            let mut phases = original.delivery_phase_candidates().to_vec();
            let mut hooks = original.delivery_hook_candidates().to_vec();
            match mutation {
                0 => typing.delivery_candidates.clear(),
                1 => typing
                    .delivery_candidates
                    .push(typing.delivery_candidates[0].clone()),
                2 => phases.clear(),
                3 => phases.push(phases[0].clone()),
                4 => hooks.clear(),
                5 => hooks.push(hooks[0].clone()),
                6 => failures.jobs.clear(),
                7 => failures.jobs.push(failures.jobs[0].clone()),
                8 => failures.jobs[0].job = "orphan_job".into(),
                9 => failures.jobs[0].callee = "orphan_callee".into(),
                _ => unreachable!(),
            }
            let (model, diagnostics) = finish_delivery_candidates(
                &original.syntax,
                &mut graph,
                &typing,
                &failures,
                &phases,
                &hooks,
            );
            assert!(model.bindings().is_empty(), "mutation {mutation}");
            assert_eq!(diagnostics.len(), 1, "mutation {mutation}");
            assert_eq!(
                diagnostics[0].code,
                "TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED"
            );
            assert_eq!(graph.nodes, before, "mutation {mutation}");
        }
    }
}
