//! Closed reminder selection effects, not ordinary calls or live delivery authority.
//! Only the finished opaque binding reaches this extractor after source freshness.
use crate::AnalyzedProject;
use jadpo_semantic::NodeKind;
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn path(value: &jadpo_syntax::NameExpression) -> String {
    value
        .path
        .iter()
        .map(|part| part.text.as_str())
        .collect::<Vec<_>>()
        .join(".")
}
pub(super) fn derive(
    project: &AnalyzedProject,
    facts: &mut BTreeMap<String, Value>,
    paths: &mut BTreeMap<String, BTreeMap<String, Value>>,
) -> Value {
    let mut rows = BTreeMap::new();
    for binding in project.delivery_model().bindings() {
        let descriptor = binding.descriptor();
        let selected = &descriptor.selection;
        let source = binding.source_facts();
        let obligations = &source["obligations"];
        let selection = obligations["phases"]
            .as_array()
            .expect("finished phase table")
            .iter()
            .find(|phase| phase["phase"] == "selection")
            .expect("finished selection phase");
        let identity = binding
            .private_graph()
            .nodes()
            .iter()
            .find(|node| node.kind() == NodeKind::DeliverySelection)
            .expect("finished binding owns a typed selection node")
            .identity()
            .to_owned();
        let key = format!("implicit_delivery_emission:{identity}");
        let row = json!({
            "contract_version":1,
            "classification":"compiler_checked_conditional_may_enroll_durable_intent",
            "job":binding.schedule().job,"binding":binding.binding_identity(),
            "phase":"selection","private_phase_identity":identity,
            "schedule_contract":source["schedule"],
            "selection_contract":{
                "name":selected.name.text,"entity":selected.entity.text,
                "identity":path(&selected.identity),"due":path(&selected.due),
                "before":"operation_time","open_field":path(&selected.open_field),
                "open_variant":path(&selected.open_variant),"visible":selected.visible.text,
                "unsent":path(&selected.unsent),"required_owner":path(&selected.required_owner),
                "owner_visible":selected.owner_visible.text
            },
            "conditions":{
                "status":"compiler_checked_required_not_proved",
                "selection_phase":selection,"authority_contract":obligations["authority"],
                "order":obligations["order"],"limit":obligations["limit"],
                "owner_active":obligations["owner_active"],
                "nominal_operation_snapshot":binding.schedule().snapshot_type
            },
            "enrollment_contract":{
                "job":binding.schedule().job,"source_operation":identity,
                "source_entity":selected.entity.text,"source_entity_id_field":path(&selected.identity),
                "source_revision_identity":format!("{}.todo_schedule_revision",binding.binding_identity()),
                "ordering_key_field":path(&selected.identity),"payload_version":obligations["payload_version"],
                "dedup_identity_fields":["job","source_operation","source_entity","source_entity_id","source_revision"],
                "enrollment":"same_selection_transaction",
                "duplicate":"retain_original_immutable_payload_and_sequence"
            },
            "payload_sources":obligations["payload"],
            "hooks":source["hooks"],"hook_effect":"establish_or_advance_schedule_revision_only",
            "external_operation_contract":source["service_contract"]["operation"],
            "provider_invoked_by_enrollment":false,
            "unqualified_runtime_details":["capture_and_locked_result_consistency","native_transaction_success","actual_current_credential_and_rows"],
            "runtime_conformance":"not_established","live_authority_established":false,"execution_profile":null
        });
        let effect = json!({"kind":"durable_intent_enrollment","target":identity,
            "path":[binding.schedule().job,identity],"path_kind":"checked_binding_private_phase",
            "ordinary_call_path":false,"classification":"compiler_checked_conditional_may_enroll_durable_intent",
            "contract":row});
        // Deliberately outside ordinary-call BFS. Hook edges in the private
        // graph are provenance associations, never route/action call edges.
        paths
            .entry(binding.schedule().job.clone())
            .or_default()
            .insert(key.clone(), effect.clone());
        facts
            .get_mut(&format!("reachability:{}", binding.schedule().job))
            .expect("finished job has ordinary reachability fact")["private_phase_effects"][&key] =
            effect;
        facts.insert(key.clone(), row.clone());
        rows.insert(key, row);
    }
    json!({"contract_version":1,"status":"compiler_checked_nonexecuting_implicit_delivery_contract",
        "supported_subset":if rows.is_empty(){"absent"}else{"present"},"facts":rows,
        "generic_event_analysis":"unsupported","runtime_conformance":"not_established",
        "live_authority_established":false,"execution_profile":null})
}
