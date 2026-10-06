//! Supported private selection contracts; never execution, provider IO or permission.
use jadpo_core::{
    analyze_sources, derive_approval_subject, derive_approval_subject_with_state_pin,
    derive_artifacts, derive_target, discover_sources, validate_approval_export, AnalyzedProject,
};
use jadpo_syntax::SourceFile;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
const FIXTURE: &str =
    include_str!("../../../../tests/validation/fixtures/reminder-delivery-v1.jadpo");
const BINDING: &str = "job.overdue_reminders.delivery.reminder_v1";
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_owned()
}
fn project(fixture: &str) -> AnalyzedProject {
    let mut sources = discover_sources(&root().join("examples/golden-todo-migration")).unwrap();
    sources.retain(|s| !s.path.ends_with("jobs/reminders.jadpo"));
    sources.push(SourceFile::new(
        root().join("tests/validation/fixtures/reminder-delivery-v1.jadpo"),
        fixture.to_owned(),
    ));
    analyze_sources(sources).unwrap()
}
fn subject(project: &AnalyzedProject, before: Option<&AnalyzedProject>) -> Value {
    assert!(
        !project
            .syntax
            .diagnostics()
            .chain(project.semantics.diagnostics.iter())
            .chain(project.entity_model.diagnostics.iter())
            .chain(project.policy.diagnostics.iter())
            .chain(project.typing.diagnostics.iter())
            .chain(project.failures.diagnostics.iter())
            .any(|d| d.severity == jadpo_diagnostics::Severity::Error),
        "{:#?} {:#?} {:#?} {:#?} {:#?}",
        project.syntax.diagnostics().collect::<Vec<_>>(),
        project.typing.diagnostics,
        project.failures.diagnostics,
        project.entity_model.diagnostics,
        project.policy.diagnostics
    );
    serde_json::from_str(
        &derive_approval_subject(
            &root(),
            project,
            before
                .map(|p| (root(), p))
                .as_ref()
                .map(|(p, b)| (p.as_path(), *b)),
            None,
            None,
        )
        .unwrap(),
    )
    .unwrap()
}
fn hash(value: &Value) -> String {
    format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(value).unwrap())
    )
}
fn key(binding: &str) -> String {
    format!("implicit_delivery_emission:{binding}.selection")
}
fn row(subject: &Value) -> &Value {
    &subject["canonical"]["after"]["facts"][key(BINDING)]
}
#[test]
fn checked_selection_enrollment_contract_has_independent_closed_expectations() {
    let p = project(FIXTURE);
    assert_eq!(p.delivery_model().bindings().len(), 1);
    let s = subject(&p, None);
    let r = row(&s);
    assert_eq!(
        r["classification"],
        "compiler_checked_conditional_may_enroll_durable_intent"
    );
    assert_eq!(r["contract_version"], 1);
    assert_eq!(r["job"], "overdue_reminders");
    assert_eq!(r["phase"], "selection");
    assert_eq!(r["private_phase_identity"], format!("{BINDING}.selection"));
    assert_eq!(
        r["selection_contract"],
        json!({"name":"reminder_candidates","entity":"Todo","identity":"Todo.id","due":"Todo.due_at","before":"operation_time","open_field":"Todo.status","open_variant":"TodoStatus.open","visible":"Todo","unsent":"Todo.reminder_sent_at","required_owner":"Todo.owner","owner_visible":"User"})
    );
    assert_eq!(
        r["enrollment_contract"],
        json!({"job":"overdue_reminders","source_operation":format!("{BINDING}.selection"),"source_entity":"Todo","source_entity_id_field":"Todo.id","source_revision_identity":format!("{BINDING}.todo_schedule_revision"),"ordering_key_field":"Todo.id","payload_version":"reminder.v1","dedup_identity_fields":["job","source_operation","source_entity","source_entity_id","source_revision"],"enrollment":"same_selection_transaction","duplicate":"retain_original_immutable_payload_and_sequence"})
    );
    assert_eq!(
        r["conditions"]["selection_phase"]["unproved_requirements"],
        json!([
            "non_null_strictly_overdue_open_unsent",
            "todo_visible",
            "owner_active_visible",
            "recipient_authorized_before_limit",
            "ordered_due_identity_limit_500",
            "due_identity_keyset_continuation",
            "transactional_unique_revision_intent",
            "immutable_payload"
        ])
    );
    assert_eq!(
        r["conditions"]["selection_phase"]["invoke_target"],
        Value::Null
    );
    assert_eq!(
        r["conditions"]["selection_phase"]["private_write_targets"],
        json!([format!("{BINDING}.revision_intent")])
    );
    assert_eq!(r["conditions"]["order"], json!(["Todo.due_at", "Todo.id"]));
    assert_eq!(r["conditions"]["limit"], 500);
    assert_eq!(
        r["conditions"]["authority_contract"]["role"],
        "ReminderRole.sender"
    );
    assert_eq!(r["payload_sources"]["to"], "User.email");
    assert_eq!(r["payload_sources"]["idempotency_key"], "generated_intent");
    assert_eq!(r["hooks"]["create"]["callable"], "Todo.create_todo");
    assert_eq!(
        r["hook_effect"],
        "establish_or_advance_schedule_revision_only"
    );
    assert_eq!(
        r["external_operation_contract"],
        "ReminderMail.send_overdue_reminder"
    );
    assert_eq!(r["provider_invoked_by_enrollment"], false);
    assert_eq!(r["live_authority_established"], false);
    assert_eq!(r["runtime_conformance"], "not_established");
    assert!(r["execution_profile"].is_null());
    assert_eq!(
        r["unqualified_runtime_details"],
        json!([
            "capture_and_locked_result_consistency",
            "native_transaction_success",
            "actual_current_credential_and_rows"
        ])
    );
}
#[test]
fn private_job_effect_is_not_an_ordinary_hook_or_route_call_path() {
    let s = subject(&project(FIXTURE), None);
    let state = &s["canonical"]["after"];
    let id = key(BINDING);
    let effect = &state["effect_paths"]["overdue_reminders"][&id];
    assert_eq!(effect["kind"], "durable_intent_enrollment");
    assert_eq!(
        effect["path"],
        json!(["overdue_reminders", format!("{BINDING}.selection")])
    );
    assert_eq!(effect["path_kind"], "checked_binding_private_phase");
    assert_eq!(effect["ordinary_call_path"], false);
    assert_eq!(effect["contract"], *row(&s));
    assert_eq!(
        state["facts"]["reachability:overdue_reminders"]["private_phase_effects"][&id],
        *effect
    );
    for (owner, paths) in state["effect_paths"].as_object().unwrap() {
        if owner != "overdue_reminders" {
            assert!(paths.get(&id).is_none(), "fabricated edge for {owner}");
        }
    }
    assert_eq!(state["impact"]["emissions"]["facts"][&id], *row(&s));
    assert_eq!(
        state["impact"]["emissions"]["generic_event_analysis"],
        "unsupported"
    );
    assert!(state["graph"]["calls"]
        .as_array()
        .unwrap()
        .iter()
        .all(|p| !p.to_string().contains(".selection")));
    assert_eq!(s["canonical"]["release_authority"], false);
}
#[test]
fn absence_ordinary_job_and_mixed_job_preserve_unsupported_generic_scope() {
    let prefix = FIXTURE.split("job overdue_reminders").next().unwrap();
    for fixture in [prefix.to_owned(), format!("{prefix}\njob ordinary every 15m {{ concurrency: singleton run:reminder_tick(ReminderRunAt(clock.now)) retry: next_schedule }}")] {
        let p=project(&fixture); let s=subject(&p,None); let e=&s["canonical"]["after"]["impact"]["emissions"];
        assert_eq!(e["supported_subset"],"absent"); assert_eq!(e["facts"],json!({})); assert_eq!(e["generic_event_analysis"],"unsupported");
    }
    let mixed=project(&format!("{FIXTURE}\njob ordinary every 15m {{ concurrency: singleton run:reminder_tick(ReminderRunAt(clock.now)) retry: next_schedule }}"));
    let s = subject(&mixed, None);
    assert_eq!(
        s["canonical"]["after"]["impact"]["emissions"]["facts"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
    assert!(s["canonical"]["after"]["effect_paths"]["ordinary"]
        .get(key(BINDING))
        .is_none());
    assert!(
        derive_target(&root(), &mixed).is_err(),
        "unsupported ordinary job gate must remain"
    );
}
#[test]
fn legal_selection_and_schedule_pairs_change_decisions_without_call_topology_changes() {
    let p = project(FIXTURE);
    let old = subject(&p, None);
    for changed in [
        FIXTURE.replace("reminder_candidates", "selected_candidates"),
        FIXTURE.replace("every 15m", "every 20m"),
    ] {
        let n = project(&changed);
        let s = subject(&n, Some(&p));
        assert_eq!(
            s["canonical"]["before"]["graph"],
            s["canonical"]["after"]["graph"]
        );
        assert_ne!(row(&s), row(&old));
        assert!(s["canonical"]["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["subject"] == key(BINDING) && d["change"] == "changed"));
    }
    let renamed = project(&FIXTURE.replace("overdue_reminders", "renamed_reminders"));
    let s = subject(&renamed, Some(&p));
    assert!(s["canonical"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["subject"] == key(BINDING) && d["change"] == "removed"));
    assert!(s["canonical"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |d| d["subject"] == key("job.renamed_reminders.delivery.reminder_v1")
                && d["change"] == "added"
        ));
    assert_eq!(old, subject(&p, None));
    assert!(derive_approval_subject_with_state_pin(
        &root(),
        &p,
        Some((&root(), &p)),
        None,
        None,
        Some("sha256:obsolete_state")
    )
    .is_err());
}
fn remove_omission(value: &mut Value, id: &str, condition: bool) {
    match value {
        Value::Object(map) => {
            if !condition {
                map.remove(id);
            }
            for v in map.values_mut() {
                remove_omission(v, id, condition);
            }
        }
        Value::Array(array) => {
            if condition {
                array.retain(|v| v != "due_identity_keyset_continuation");
            }
            for v in array {
                remove_omission(v, id, condition);
            }
        }
        _ => {}
    }
}
fn cohere(s: &mut Value) {
    let state = &mut s["canonical"]["after"];
    for (category, value) in state["impact"].as_object().unwrap().clone() {
        state["facts"][format!("impact:{category}")] = value;
    }
    let outputs = state["provenance"]["generated_artifacts"].clone();
    state["facts"]["provenance:generated_artifacts"] = outputs.clone();
    for category in ["metadata_and_audit", "target"] {
        for (name, pin) in outputs[category]["files"].as_object().unwrap() {
            state["facts"][format!("artifact_pin:{name}")] = pin.clone();
        }
    }
    state["behavior_digest"] = json!(hash(&state["facts"]));
    state.as_object_mut().unwrap().remove("state_digest");
    state["state_digest"] = json!(hash(state));
    let facts = state["facts"].clone();
    s["canonical"]["decisions"]
        .as_array_mut()
        .unwrap()
        .retain_mut(|d| {
            let k = d["subject"].as_str().unwrap().to_owned();
            if let Some(v) = facts.get(&k) {
                d["after"] = v.clone();
                true
            } else {
                false
            }
        });
    s["subject_digest"] = json!(hash(&s["canonical"]));
}
#[test]
fn coherent_all_copy_emission_and_condition_omissions_self_validate_but_fail_fresh_source() {
    let p = project(FIXTURE);
    let original = subject(&p, None);
    assert!(!row(&original).is_null());
    let mut baseline = original.clone();
    cohere(&mut baseline);
    assert_eq!(
        baseline, original,
        "coherence recipe must first reproduce actual complete subject"
    );
    for condition in [false, true] {
        let mut forged = original.clone();
        remove_omission(&mut forged["canonical"]["after"], &key(BINDING), condition);
        // The condition also occurs in actual generated metadata; synchronize all changed file pins.
        if condition {
            for artifact in derive_artifacts(&root(), &p) {
                let Ok(mut value) = serde_json::from_str::<Value>(&artifact.contents) else {
                    continue;
                };
                let before = value.clone();
                remove_omission(&mut value, &key(BINDING), true);
                if value == before {
                    continue;
                }
                let bytes = format!("{}\n", serde_json::to_string_pretty(&value).unwrap());
                let file = &mut forged["canonical"]["after"]["provenance"]["generated_artifacts"]
                    ["metadata_and_audit"]["files"][&artifact.relative_path];
                if !file.is_null() {
                    file["byte_digest"] =
                        json!(format!("sha256:{:x}", Sha256::digest(bytes.as_bytes())));
                    file["byte_length"] = json!(bytes.len());
                }
            }
        }
        cohere(&mut forged);
        let raw = forged.to_string();
        assert!(
            validate_approval_export(&raw, &raw).is_ok(),
            "fully rehashed generic self-validation is not source truth"
        );
        assert!(validate_approval_export(&original.to_string(), &raw).is_err());
        assert_eq!(
            forged["canonical"]["after"]["behavior_digest"],
            hash(&forged["canonical"]["after"]["facts"])
        );
        let mut state = forged["canonical"]["after"].clone();
        let pin = state
            .as_object_mut()
            .unwrap()
            .remove("state_digest")
            .unwrap();
        assert_eq!(pin, hash(&state));
    }
}
#[test]
fn incomplete_candidate_and_stale_source_do_not_certify_emissions() {
    let invalid = project(&FIXTURE.replace("limit: 500", "limit: 501"));
    assert!(invalid.delivery_model().bindings().is_empty());
    assert!(derive_approval_subject(&root(), &invalid, None, None, None).is_err());
    let mut stale = project(FIXTURE);
    for source in &mut stale.syntax.sources {
        for declaration in &mut source.file.declarations {
            if let jadpo_syntax::Declaration::Job(job) = declaration {
                job.name.text = "changed_without_recheck".into();
            }
        }
    }
    assert!(derive_approval_subject(&root(), &stale, None, None, None).is_err());
}
