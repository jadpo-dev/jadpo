//! Conditional private phase associations, never ordinary calls or live authority.
use crate::AnalyzedProject;
use jadpo_semantic::NodeKind;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
type Rows = BTreeMap<String, BTreeMap<String, Value>>;
fn phase<'a>(source: &'a Value, name: &str) -> &'a Value {
    source["obligations"]["phases"]
        .as_array()
        .expect("finished phase table")
        .iter()
        .find(|row| row["phase"] == name)
        .expect("finished named phase")
}
pub(super) fn derive(
    project: &AnalyzedProject,
    facts: &mut BTreeMap<String, Value>,
    paths: &mut BTreeMap<String, BTreeMap<String, Value>>,
) -> Rows {
    let mut rows = Rows::new();
    for binding in project.delivery_model().bindings() {
        let source = binding.source_facts();
        let graph = binding.private_graph();
        let job = &binding.schedule().job;
        let name = binding.binding_identity();
        let invoke = graph
            .edges()
            .iter()
            .find(|edge| edge.phase() == "dispatch" && edge.kind() == "invoke")
            .expect("finished binding owns an exact dispatch invoke edge");
        for (selected_phase, node_kind) in [
            ("dispatch", None),
            ("completion", Some(NodeKind::DeliveryCompletion)),
            ("acceptance_proof", Some(NodeKind::DeliveryAcceptanceProof)),
            ("reconciliation", Some(NodeKind::DeliveryReconciliation)),
        ] {
            let id = format!("private_delivery_effect:{name}:{selected_phase}");
            let mut row = json!({
                "contract_version":2,"job":job,"binding":name,"phase":selected_phase,
                "schedule_contract":source["schedule"],
                "payload_version":source["obligations"]["payload_version"],
                "ordinary_call_path":false,
                "conditions":{"status":"compiler_checked_required_not_proved"},
                "runtime_conformance":"not_established","live_authority_established":false,"execution_profile":null
            });
            let path;
            let kind;
            if selected_phase == "dispatch" {
                row["classification"] =
                    json!("compiler_checked_conditional_private_dispatch_may_effect");
                row["invoke_target"] = json!(invoke.target_identity());
                row["witness"] = json!({"edge":{"phase":invoke.phase(),"kind":invoke.kind(),"target":invoke.target_identity()},"private_node":null});
                row["conditions"]["admission_phase"] = phase(&source, "admission").clone();
                row["conditions"]["dispatch_phase"] = phase(&source, "dispatch").clone();
                row["service_contract"] = source["service_contract"].clone();
                path = json!([job, name, invoke.target_identity()]);
                kind = "external_service";
            } else {
                let node = graph
                    .nodes()
                    .iter()
                    .find(|node| Some(node.kind()) == node_kind)
                    .expect("finished binding owns typed private phase node");
                row["classification"] = json!(format!(
                    "compiler_checked_conditional_private_{selected_phase}_may_effect"
                ));
                row["public_write_targets"] = json!(graph
                    .edges()
                    .iter()
                    .filter(|edge| edge.phase() == selected_phase && edge.kind() == "write")
                    .map(|edge| edge.target_identity())
                    .collect::<Vec<_>>());
                row["private_write_targets"] = json!(graph
                    .edges()
                    .iter()
                    .filter(|edge| edge.phase() == selected_phase && edge.kind() == "private_write")
                    .map(|edge| edge.target_identity())
                    .collect::<Vec<_>>());
                row["witness"] = json!({"private_node":node.identity(),"edges":graph.edges().iter()
                    .filter(|edge|edge.phase()==selected_phase && matches!(edge.kind(),"write"|"private_write"))
                    .map(|edge|json!({"phase":edge.phase(),"kind":edge.kind(),"target":edge.target_identity()})).collect::<Vec<_>>()});
                row["conditions"][format!("{selected_phase}_phase")] =
                    phase(&source, selected_phase).clone();
                if selected_phase != "completion" {
                    row["reconciliation_contract"] = source["reconciliation_contract"].clone();
                }
                path = json!([job, node.identity()]);
                kind = match selected_phase {
                    "completion" => "delivery_completion",
                    "acceptance_proof" => "delivery_acceptance_proof",
                    "reconciliation" => "delivery_reconciliation",
                    _ => unreachable!(),
                };
            }
            let effect = json!({"kind":kind,"path":path,"path_kind":"checked_binding_private_delivery_effect",
                "ordinary_call_path":false,"classification":row["classification"],"contract":row});
            paths
                .entry(job.clone())
                .or_default()
                .insert(id.clone(), effect.clone());
            facts
                .get_mut(&format!("reachability:{job}"))
                .expect("finished job has ordinary reachability fact")["private_phase_effects"]
                [&id] = effect;
            facts.insert(id.clone(), row.clone());
            rows.entry(job.clone()).or_default().insert(id, row);
        }
    }
    rows
}
pub(super) fn projection(rows: &Rows, selected_phase: &str) -> Value {
    json!(rows
        .iter()
        .map(|(job, effects)| (
            job,
            effects
                .iter()
                .filter(|(_, row)| row["phase"] == selected_phase
                    || selected_phase == "private_writes" && row["phase"] != "dispatch")
                .collect::<BTreeMap<_, _>>()
        ))
        .collect::<BTreeMap<_, _>>())
}
// Private associations are not processed by HTTP may-call scenario traversal.
pub(super) fn scenarios(before: Option<&Value>, after: &Value) -> Vec<Value> {
    let old = before.and_then(|state| state["effect_paths"].as_object());
    let new = after["effect_paths"]
        .as_object()
        .expect("checked effect paths");
    let owners: BTreeSet<_> = old
        .into_iter()
        .flat_map(|paths| paths.keys())
        .chain(new.keys())
        .collect();
    let mut output = Vec::new();
    for owner in owners {
        let previous = old
            .and_then(|paths| paths.get(owner))
            .and_then(Value::as_object);
        let proposed = new.get(owner).and_then(Value::as_object);
        let ids: BTreeSet<_> = previous
            .into_iter()
            .flat_map(|effects| effects.keys())
            .chain(proposed.into_iter().flat_map(|effects| effects.keys()))
            .collect();
        for id in ids {
            let old = previous.and_then(|effects| effects.get(id));
            let new = proposed.and_then(|effects| effects.get(id));
            let selected = new.or(old).expect("selected union effect");
            if selected["path_kind"] != "checked_binding_private_delivery_effect" || old == new {
                continue;
            }
            let status = match (old.is_some(), new.is_some(), before.is_some()) {
                (false, true, false) => "after_only_without_baseline",
                (false, true, true) => "newly_reachable_private_may_effect",
                (true, false, _) => "no_longer_reachable_private_may_effect",
                (true, true, _) => "private_phase_contract_changed_may_effect",
                (false, false, _) => unreachable!(),
            };
            output.push(json!({
                "id":super::digest(format!("private.delivery.scenario.v1:{owner}:{id}").as_bytes()),
                "kind":"private_delivery_phase_may_effect","job":owner,
                "binding":selected["contract"]["binding"],"phase":selected["contract"]["phase"],
                "effect":selected,"status":status,"before":old,"after":new,
                "certainty":"compiler_checked_conditional_private_phase",
                "ordinary_call_path":false,"execution":"not_proved","feasibility":"not_analyzed",
                "counterexample":{"status":"not_generated","facts":null}
            }));
        }
    }
    output
}
