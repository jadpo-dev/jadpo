//! Private checked may-effects; no ordinary calls, execution, or granted authority.
use jadpo_core::{
    analyze_sources, derive_approval_subject, derive_artifacts, discover_sources,
    validate_approval_export, AnalyzedProject,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
const BINDING: &str = "job.overdue_reminders.delivery.reminder_v1";
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_owned()
}
fn project(replacements: &[(&str, &str)]) -> AnalyzedProject {
    let mut sources = discover_sources(&root().join("examples/golden-todo-migration")).unwrap();
    for source in &mut sources {
        for (old, new) in replacements {
            source.text = source.text.replace(old, new);
        }
    }
    analyze_sources(sources).unwrap()
}
fn subject(p: &AnalyzedProject, before: Option<&AnalyzedProject>) -> Value {
    serde_json::from_str(
        &derive_approval_subject(
            &root(),
            p,
            before
                .map(|b| (root(), b))
                .as_ref()
                .map(|(p, b)| (p.as_path(), *b)),
            None,
            None,
        )
        .unwrap(),
    )
    .unwrap()
}
fn id(binding: &str, phase: &str) -> String {
    format!("private_delivery_effect:{binding}:{phase}")
}
fn row<'a>(s: &'a Value, phase: &str) -> &'a Value {
    &s["canonical"]["after"]["facts"][id(BINDING, phase)]
}
#[test]
fn typed_dispatch_edge_and_completion_targets_have_separate_conditional_effects() {
    let s = subject(&project(&[]), None);
    let state = &s["canonical"]["after"];
    let d = row(&s, "dispatch");
    let c = row(&s, "completion");
    assert_eq!(
        d["classification"],
        "compiler_checked_conditional_private_dispatch_may_effect"
    );
    assert_eq!(d["contract_version"], 1);
    assert_eq!(d["invoke_target"], "ReminderMail.send_overdue_reminder");
    assert_eq!(
        d["witness"]["edge"],
        json!({"phase":"dispatch","kind":"invoke","target":"ReminderMail.send_overdue_reminder"})
    );
    assert!(d["witness"]["private_node"].is_null());
    assert_eq!(
        c["classification"],
        "compiler_checked_conditional_private_completion_may_effect"
    );
    assert_eq!(c["public_write_targets"], json!(["Todo.reminder_sent_at"]));
    assert_eq!(
        c["private_write_targets"],
        json!([
            format!("{BINDING}.receipt"),
            format!("{BINDING}.revision_intent")
        ])
    );
    assert_eq!(
        c["witness"]["private_node"],
        format!("{BINDING}.completion")
    );
    for phase in ["dispatch", "completion"] {
        let r = row(&s, phase);
        assert_eq!(
            r["conditions"]["status"],
            "compiler_checked_required_not_proved"
        );
        assert_eq!(r["live_authority_established"], false);
        assert_eq!(r["runtime_conformance"], "not_established");
        assert!(r["execution_profile"].is_null());
        let key = id(BINDING, phase);
        let e = &state["effect_paths"]["overdue_reminders"][&key];
        assert_eq!(e["ordinary_call_path"], false);
        assert_eq!(e["contract"], *r);
        assert_eq!(
            state["facts"]["reachability:overdue_reminders"]["private_phase_effects"][&key],
            *e
        );
        assert_eq!(
            state["impact"]["jobs"]["private_phase_facts"]["overdue_reminders"][&key],
            *r
        );
        for (owner, paths) in state["effect_paths"].as_object().unwrap() {
            if owner != "overdue_reminders" {
                assert!(paths.get(&key).is_none());
            }
        }
    }
    assert_eq!(
        state["facts"]["reachability:overdue_reminders"]["effects"],
        json!({})
    );
    assert_eq!(
        state["impact"]["external_effects"]["facts"]["overdue_reminders"],
        json!({})
    );
    assert_eq!(
        state["impact"]["external_effects"]["private_phase_facts"]["overdue_reminders"]
            [id(BINDING, "dispatch")],
        *d
    );
    assert_eq!(
        state["impact"]["writes"]["private_phase_facts"]["overdue_reminders"]
            [id(BINDING, "completion")],
        *c
    );
    assert_eq!(state["impact"]["emissions"]["contract_version"], 1);
    assert!(state["graph"]["calls"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| !e.to_string().contains("delivery.reminder_v1")));
    assert_eq!(
        state["graph"]["private_delivery_graphs"][0]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn exact_admitted_authority_budget_receipt_and_fence_conditions_remain_unproved() {
    let s = subject(&project(&[]), None);
    let d = row(&s, "dispatch");
    let c = row(&s, "completion");
    assert_eq!(
        d["conditions"]["dispatch_phase"]["unproved_requirements"],
        json!([
            "admitted_authority_only_no_extra_live_cut",
            "consume_shared_admitted_budget_without_reset",
            "frozen_provider_attempts_timeout_egress_outcomes",
            "no_transaction_across_provider_io",
            "no_automatic_unknown_replay"
        ])
    );
    assert_eq!(
        c["conditions"]["completion_phase"]["unproved_requirements"],
        json!([
            "exact_key_validated_receipt",
            "retain_receipt_on_original_intent",
            "current_fencing_generation",
            "original_schedule_revision_equals_current",
            "todo_still_unsent",
            "compiler_receipt_observation_time_only",
            "same_completion_transaction"
        ])
    );
    let b = &s["canonical"]["after"]["facts"][format!("delivery_binding:{BINDING}")];
    let phases = b["obligations"]["phases"].as_array().unwrap();
    for phase in ["admission", "dispatch"] {
        assert_eq!(
            d["conditions"][format!("{phase}_phase")],
            *phases.iter().find(|p| p["phase"] == phase).unwrap()
        );
    }
    assert_eq!(
        c["conditions"]["completion_phase"],
        *phases.iter().find(|p| p["phase"] == "completion").unwrap()
    );
    assert_eq!(d["service_contract"], b["service_contract"]);
    assert_eq!(d["payload_version"], "reminder.v1");
    assert_eq!(
        s["canonical"]["after"]["impact"]["jobs"]["feasible_job_scenarios"],
        "not_generated"
    );
}
#[test]
fn job_phase_scenarios_compare_contract_changes_without_feasibility_or_http_reclassification() {
    let p = project(&[]);
    let after = subject(&p, None);
    let only = |s: &Value| {
        s["canonical"]["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["kind"] == "private_delivery_phase_may_effect")
            .cloned()
            .collect::<Vec<_>>()
    };
    let rows = only(&after);
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(row["job"], "overdue_reminders");
        assert_eq!(row["status"], "after_only_without_baseline");
        assert_eq!(row["execution"], "not_proved");
        assert_eq!(row["feasibility"], "not_analyzed");
        assert!(row.get("route").is_none());
    }
    assert!(only(&subject(&p, Some(&p))).is_empty());
    let q = project(&[("every 15m", "every 10m")]);
    let changed = subject(&q, Some(&p));
    assert_eq!(only(&changed).len(), 2);
    assert_eq!(
        changed["canonical"]["before"]["graph"],
        changed["canonical"]["after"]["graph"]
    );
    for row in only(&changed) {
        assert_eq!(row["status"], "private_phase_contract_changed_may_effect");
    }
    let renamed = subject(
        &project(&[("overdue_reminders", "due_notifications")]),
        Some(&p),
    );
    assert_eq!(only(&renamed).len(), 4);
    let field = subject(
        &project(&[("reminder_sent_at", "reminder_recorded_at")]),
        Some(&p),
    );
    assert_eq!(
        field["canonical"]["after"]["facts"][id(BINDING, "completion")]["public_write_targets"],
        json!(["Todo.reminder_recorded_at"])
    );
}
#[test]
fn missing_or_incomplete_binding_and_stale_sources_do_not_certify_private_effects() {
    let p = project(&[]);
    let mut sources = discover_sources(&root().join("examples/golden-todo-migration")).unwrap();
    for source in &mut sources {
        if source.path.ends_with("jobs/reminders.jadpo") {
            source.text = source
                .text
                .split("job overdue_reminders")
                .next()
                .unwrap()
                .to_owned();
        }
    }
    let none = analyze_sources(sources).unwrap();
    let s = subject(&none, None);
    assert_eq!(
        s["canonical"]["after"]["impact"]["jobs"]["private_phase_facts"],
        json!({})
    );
    let mut stale = p.clone();
    for source in &mut stale.syntax.sources {
        for declaration in &mut source.file.declarations {
            if let jadpo_syntax::Declaration::Job(job) = declaration {
                job.name.text = "changed_without_recheck".into();
            }
        }
    }
    assert!(derive_approval_subject(&root(), &stale, None, None, None).is_err());
    assert!(derive_approval_subject(
        &root(),
        &project(&[("ReminderMail", "ReminderSender")]),
        None,
        None,
        None
    )
    .is_err());
}
fn hash(value: &Value) -> String {
    format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(value).unwrap())
    )
}
fn mutate(value: &mut Value, mode: &str) {
    match value {
        Value::Object(map) => {
            if mode == "dispatch" || mode == "completion" {
                map.remove(&id(BINDING, mode));
            }
            if mode == "authority"
                && map
                    .get("classification")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.starts_with("compiler_checked_conditional_private_"))
            {
                map.insert("live_authority_established".into(), json!(true));
            }
            for v in map.values_mut() {
                mutate(v, mode);
            }
        }
        Value::Array(a) => {
            if mode == "admission-cut" {
                a.retain(|v| v != "admitted_authority_only_no_extra_live_cut");
            }
            if mode == "receipt" {
                a.retain(|v| v != "exact_key_validated_receipt");
            }
            if mode == "budget" {
                a.retain(|v| v != "consume_shared_admitted_budget_without_reset");
            }
            if mode == "fence" {
                a.retain(|v| v != "current_fencing_generation");
            }
            for v in a {
                mutate(v, mode);
            }
        }
        _ => {}
    }
}
fn cohere(s: &mut Value) {
    let state = &mut s["canonical"]["after"];
    for (k, v) in state["impact"].as_object().unwrap().clone() {
        state["facts"][format!("impact:{k}")] = v;
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
            let k = d["subject"].as_str().unwrap();
            if let Some(v) = facts.get(k) {
                d["after"] = v.clone();
                true
            } else {
                false
            }
        });
    s["subject_digest"] = json!(hash(&s["canonical"]));
}
#[test]
fn coherent_all_copy_omissions_and_authority_forgery_fail_actual_fresh_export_comparison() {
    let p = project(&[]);
    let original = subject(&p, None);
    assert!(!row(&original, "dispatch").is_null());
    let mut equal = original.clone();
    cohere(&mut equal);
    assert_eq!(equal, original);
    let artifacts = derive_artifacts(&root(), &p);
    for mode in [
        "dispatch",
        "completion",
        "budget",
        "fence",
        "authority",
        "receipt",
        "admission-cut",
    ] {
        let mut forged = original.clone();
        mutate(&mut forged["canonical"]["after"], mode);
        mutate(&mut forged["canonical"]["scenarios"], mode);
        if mode == "dispatch" || mode == "completion" {
            forged["canonical"]["scenarios"]
                .as_array_mut()
                .unwrap()
                .retain(|v| v["phase"] != mode);
        }
        for artifact in &artifacts {
            if let Ok(mut value) = serde_json::from_str::<Value>(&artifact.contents) {
                let old = value.clone();
                mutate(&mut value, mode);
                if value != old {
                    let bytes = format!("{}\n", serde_json::to_string_pretty(&value).unwrap());
                    let file = &mut forged["canonical"]["after"]["provenance"]
                        ["generated_artifacts"]["metadata_and_audit"]["files"]
                        [&artifact.relative_path];
                    if !file.is_null() {
                        file["byte_digest"] =
                            json!(format!("sha256:{:x}", Sha256::digest(bytes.as_bytes())));
                        file["byte_length"] = json!(bytes.len());
                    }
                }
            }
        }
        cohere(&mut forged);
        let raw = forged.to_string();
        assert!(validate_approval_export(&raw, &raw).is_ok());
        assert!(
            validate_approval_export(&original.to_string(), &raw).is_err(),
            "{mode}"
        );
    }
}

#[test]
fn mixed_ordinary_job_has_no_private_effect_and_text_keeps_exact_canonical_rows() {
    let mut sources = discover_sources(&root().join("examples/golden-todo-migration")).unwrap();
    for source in &mut sources {
        if source.path.ends_with("jobs/reminders.jadpo") {
            source.text.push_str("\njob ordinary every 15m { concurrency: singleton run: reminder_tick(ReminderRunAt(clock.now)) retry: next_schedule }\n");
        }
    }
    let p = analyze_sources(sources).unwrap();
    let s = subject(&p, None);
    assert!(
        s["canonical"]["after"]["impact"]["jobs"]["private_phase_facts"]
            .get("ordinary")
            .is_none()
    );
    assert!(s["canonical"]["after"]["effect_paths"]["ordinary"]
        .as_object()
        .unwrap()
        .is_empty());
    assert_eq!(
        s["canonical"]["after"]["impact"]["jobs"]["private_phase_contract_version"],
        1
    );
    let raw = s.to_string();
    let text = jadpo_core::approval_text(&raw);
    assert!(validate_approval_export(&raw, text.lines().last().unwrap()).is_ok());
}
