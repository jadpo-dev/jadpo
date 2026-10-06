//! Complete static boundary, not generated worker or live permission evidence.
use jadpo_core::{
    analyze_sources, derive_approval_subject, derive_artifacts, derive_target, discover_sources,
    validate_approval_export, AnalyzedProject,
};
use jadpo_diagnostics::Severity;
use jadpo_semantic::NodeKind;
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
fn analyze(fixture: &str, changes: &[(&str, &str, &str)]) -> AnalyzedProject {
    let root = root();
    let mut sources = discover_sources(&root.join("examples/golden-todo-migration")).unwrap();
    // Candidate fixture owns its delivery declarations; canonical checked
    // source is excluded to avoid duplicate names while retaining contracts.
    sources.retain(|source| !source.path.ends_with("jobs/reminders.jadpo"));
    for (file, before, after) in changes {
        let source = sources.iter_mut().find(|s| s.path.ends_with(file)).unwrap();
        assert!(source.text.contains(before), "missing {before} in {file}");
        source.text = source.text.replacen(before, after, 1);
    }
    sources.push(SourceFile::new(
        root.join("tests/validation/fixtures/reminder-delivery-v1.jadpo"),
        fixture.to_owned(),
    ));
    analyze_sources(sources).unwrap()
}
fn errors(project: &AnalyzedProject) -> Vec<&str> {
    project
        .syntax
        .diagnostics()
        .chain(project.semantics.diagnostics.iter())
        .chain(project.typing.diagnostics.iter())
        .chain(project.failures.diagnostics.iter())
        .chain(project.entity_model.diagnostics.iter())
        .chain(project.policy.diagnostics.iter())
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}
fn checked(project: &AnalyzedProject) {
    assert!(errors(project).is_empty(), "{:?}", errors(project));
    assert_eq!(project.delivery_model().bindings().len(), 1);
    assert_eq!(project.typing.delivery_candidates.len(), 1);
    assert!(project
        .typing
        .jobs
        .iter()
        .all(|job| job.job != "overdue_reminders"));
    assert!(derive_approval_subject(&root(), project, None, None, None).is_ok());
    if project.typing.jobs.is_empty() {
        assert!(derive_target(&root(), project).is_ok());
    } else {
        assert_eq!(
            derive_target(&root(), project).unwrap_err().code,
            "JADPO_TARGET_JOB_NOT_IMPLEMENTED"
        );
    }
}
fn expected_graph() -> Value {
    // Independent closed expectation: do not obtain it from the phase model.
    let mut edges: Vec<(&str, &str, String, bool)> = Vec::new();
    let mut add = |phase, kind, targets: &[&str], private| {
        for target in targets {
            edges.push((
                phase,
                kind,
                if private {
                    format!("{BINDING}.{target}")
                } else {
                    (*target).to_owned()
                },
                private,
            ));
        }
    };
    let selected = [
        "Todo.id",
        "Todo.due_at",
        "Todo.reminder_sent_at",
        "Todo.status",
        "Todo.title",
        "Todo.owner",
        "User.email",
        "User.status",
    ];
    add("selection", "read", &selected, false);
    add("admission", "read", &selected, false);
    add(
        "admission",
        "read",
        &[
            "ServiceCredential.id",
            "ServiceCredential.service_id",
            "ServiceCredential.verifier",
            "ServiceCredential.status",
            "ServiceCredential.expires_at",
            "ServiceCredential.revoked_at",
            "Service.id",
            "Service.name",
            "Service.status",
            "ReminderServiceMembership.id",
            "ReminderServiceMembership.service_id",
            "ReminderServiceMembership.role",
        ],
        false,
    );
    for phase in ["selection", "admission"] {
        add(
            phase,
            "predicate",
            &["Todo.lifecycle.visibility", "User.lifecycle.visibility"],
            false,
        );
    }
    add(
        "completion",
        "read",
        &["Todo.id", "Todo.reminder_sent_at"],
        false,
    );
    add("completion", "write", &["Todo.reminder_sent_at"], false);
    add(
        "selection",
        "private_read",
        &["revision_intent", "todo_schedule_revision"],
        true,
    );
    add("selection", "private_write", &["revision_intent"], true);
    add(
        "admission",
        "private_read",
        &[
            "revision_intent",
            "claim_fence",
            "todo_schedule_revision",
            "invocation_budget",
        ],
        true,
    );
    add(
        "admission",
        "private_write",
        &["revision_intent", "claim_fence", "invocation_budget"],
        true,
    );
    add(
        "dispatch",
        "private_read",
        &["revision_intent", "claim_fence", "invocation_budget"],
        true,
    );
    add(
        "completion",
        "private_read",
        &[
            "revision_intent",
            "claim_fence",
            "receipt",
            "todo_schedule_revision",
        ],
        true,
    );
    add(
        "completion",
        "private_write",
        &["revision_intent", "receipt"],
        true,
    );
    add(
        "dispatch",
        "invoke",
        &["ReminderMail.send_overdue_reminder"],
        false,
    );
    for phase in ["selection", "admission", "dispatch", "completion"] {
        add(
            phase,
            "selected_validator",
            &["authentication.api_bearer.validator.service_key.api_key"],
            false,
        );
        add(
            phase,
            "selected_membership",
            &["ReminderServiceMembership"],
            false,
        );
        add(phase, "selected_role", &["ReminderRole.sender"], false);
        add(
            phase,
            "selected_service_principal",
            &["Principal.service"],
            false,
        );
    }
    add("selection", "create_hook", &["Todo.create_todo"], false);
    add("selection", "patch_hook", &["Todo.patch_todo"], false);
    edges.sort();
    json!({"schema_version":1, "binding":BINDING,
        "nodes":[{"identity":format!("{BINDING}.selection"),"kind":"delivery_selection"},
            {"identity":format!("{BINDING}.completion"),"kind":"delivery_completion"}],
        "edges":edges.iter().map(|(phase,kind,target,private)| json!({"phase":phase,"kind":kind,"target":target,"private_target":private})).collect::<Vec<_>>(),
        "ordinary_call_edges":false,"live_authority_established":false})
}

#[test]
fn complete_binding_publishes_exact_private_graph_without_ordinary_authority() {
    let project = analyze(FIXTURE, &[]);
    checked(&project);
    let binding = &project.delivery_model().bindings()[0];
    assert_eq!(binding.binding_identity(), BINDING);
    assert_eq!(binding.schedule().callee, "reminder_tick");
    assert_eq!(binding.schedule().interval_ms, 900_000);
    assert_eq!(binding.private_graph().source_facts(), expected_graph());
    let private = project
        .semantics
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.kind,
                NodeKind::DeliverySelection | NodeKind::DeliveryCompletion
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(private.len(), 2);
    assert!(private.iter().all(|node| project
        .semantics
        .calls
        .iter()
        .all(|call| call.caller != node.id && call.callee != node.id)));
    assert_eq!(
        project
            .semantics
            .calls
            .iter()
            .filter(
                |call| project.semantics.nodes[call.caller.0 as usize].name == "overdue_reminders"
            )
            .count(),
        1
    );
    let facts = binding.source_facts();
    assert_eq!(facts["status"], "checked_nonexecuting_binding");
    assert_eq!(facts["execution_profile"], Value::Null);
    assert_eq!(facts["runtime_lowering_supported"], true);
    assert_eq!(
        facts["run_failure_contract"],
        json!({"job":"overdue_reminders","callee":"reminder_tick","failures":[],"may_suspend":false})
    );
    assert_eq!(facts["live_authority_established"], false);
    assert_eq!(
        facts["hooks"]["create"]["parameter_identity"],
        "Todo.create_todo.parameter.0"
    );
    assert_eq!(
        facts["hooks"]["patch"]["parameter_identity"],
        "Todo.patch_todo.parameter.1"
    );
    assert_eq!(
        facts["obligations"]["order"],
        json!(["Todo.due_at", "Todo.id"])
    );
    assert_eq!(
        facts["obligations"]["payload"]["idempotency_key"],
        "generated_intent"
    );
    assert_eq!(
        facts["service_contract"]["import"],
        json!({"file":"tests/assurance/service-reference-mail-v0.1.json", "version":"0.1.0", "sha256":"c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"})
    );
    assert_eq!(facts["service_contract"]["max_attempts"], 3);
    assert_eq!(facts["service_contract"]["max_elapsed_ms"], 30_000);
    assert_eq!(
        facts["service_contract"]["credential_slot"],
        "config.mail_api_key"
    );
    let plain = format!(
        "{} }}\n",
        &FIXTURE[..FIXTURE.find("    delivery:").unwrap()]
    );
    let ordinary = analyze(&plain, &[]);
    assert!(errors(&ordinary).is_empty());
    assert_eq!(project.policy, ordinary.policy);
}

#[test]
fn mixed_jobs_are_an_explicit_complete_audit_union_with_private_outcomes_separate() {
    let project = analyze(&format!("{FIXTURE}\njob plain_job every 1m {{ concurrency: singleton run: reminder_tick(ReminderRunAt(clock.now)) retry: next_schedule }}"), &[]);
    checked(&project);
    assert_eq!(project.typing.jobs.len(), 1);
    assert_eq!(project.failures.jobs.len(), 2);
    let artifacts = derive_artifacts(&root(), &project);
    let audit: Value = serde_json::from_str(
        &artifacts
            .iter()
            .find(|a| a.relative_path == "audit/jobs.json")
            .unwrap()
            .contents,
    )
    .unwrap();
    assert_eq!(audit["schema_version"], 2);
    let rows = audit["jobs"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let delivery = rows
        .iter()
        .find(|r| r["job"] == "overdue_reminders")
        .unwrap();
    let plain = rows.iter().find(|r| r["job"] == "plain_job").unwrap();
    assert!(plain.get("delivery").is_none());
    assert_eq!(plain["runtime_lowering_supported"], false);
    assert_eq!(plain["durable_failure_dispositions"], "pending_checked_worker_binding");
    assert_eq!(plain["execution_profile"], Value::Null);
    assert_eq!(delivery["runtime_lowering_supported"], true);
    assert_eq!(delivery["binding_matches_source_job"], true);
    assert_eq!(delivery["runtime_conformance"], "not_certified_by_static_audit");
    assert_eq!(delivery["durable_failure_dispositions"], "emitted_checked_delivery_protocol");
    assert_eq!(delivery["execution_profile"], Value::Null);
    assert_eq!(delivery["delivery"]["live_authority_established"], false);
    assert_eq!(delivery["delivery"]["obligations"]["runtime_supported"], false);
    assert_eq!(
        delivery["delivery"],
        project.delivery_model().bindings()[0].source_facts()
    );
    assert_eq!(
        delivery["delivery_service_effects"],
        json!(["ReminderMail.send_overdue_reminder"])
    );
    assert_eq!(delivery["external_service_effects"], json!([]));
    assert_eq!(delivery["failures"], json!([]));
    let meta: Value = serde_json::from_str(
        &artifacts
            .iter()
            .find(|a| a.relative_path == "app.meta.json")
            .unwrap()
            .contents,
    )
    .unwrap();
    assert_eq!(
        meta["semantic_graph"]["delivery_bindings"],
        json!([project.delivery_model().bindings()[0].source_facts()])
    );
    let subject: Value = serde_json::from_str(
        &artifacts
            .iter()
            .find(|a| a.relative_path == "approval/subject.json")
            .unwrap()
            .contents,
    )
    .unwrap();
    assert_eq!(
        subject["canonical"]["after"]["impact"]["jobs"]["facts"],
        audit
    );
    assert_eq!(
        subject["canonical"]["after"]["facts"][format!("delivery_binding:{BINDING}")],
        delivery["delivery"]
    );
}

#[test]
fn rehashed_omissions_of_each_delivery_surface_are_rejected() {
    let project = analyze(FIXTURE, &[]);
    checked(&project);
    let raw = derive_approval_subject(&root(), &project, None, None, None).unwrap();
    let reviewed: Value = serde_json::from_str(&raw).unwrap();
    let key = format!("delivery_binding:{BINDING}");
    for omission in 0..11 {
        let mut omitted = reviewed.clone();
        let after = &mut omitted["canonical"]["after"];
        match omission {
            0 => {
                after["facts"]
                    .as_object_mut()
                    .unwrap()
                    .remove(&key)
                    .unwrap();
            }
            1 => {
                after["facts"][&key]["hooks"]
                    .as_object_mut()
                    .unwrap()
                    .remove("patch")
                    .unwrap();
            }
            2 => {
                after["facts"][&key]["obligations"]["phases"]
                    .as_array_mut()
                    .unwrap()
                    .remove(1);
            }
            3 => {
                after["facts"][&key]["private_graph"]["edges"]
                    .as_array_mut()
                    .unwrap()
                    .pop()
                    .unwrap();
            }
            4 => {
                after["facts"][&key]["service_contract"]
                    .as_object_mut()
                    .unwrap()
                    .remove("import")
                    .unwrap();
            }
            5 => {
                after["facts"][&key]["obligations"]["payload"]
                    .as_object_mut()
                    .unwrap()
                    .remove("to")
                    .unwrap();
            }
            6 => {
                after["facts"][&key]["obligations"]["phases"][1]["unproved_predicates"]
                    .as_array_mut()
                    .unwrap()
                    .remove(5);
            }
            7 => {
                after["facts"]["semantic_graph"]
                    .as_object_mut()
                    .unwrap()
                    .remove("private_delivery_graphs")
                    .unwrap();
            }
            8 => {
                after["impact"]["jobs"]["facts"]["jobs"]
                    .as_array_mut()
                    .unwrap()
                    .clear();
            }
            9 => {
                after["facts"][&key]["execution_profile"] =
                    json!({"execution_ms":60_000,"lease_ms":40_000});
            }
            10 => {
                after["facts"][&key]
                    .as_object_mut()
                    .unwrap()
                    .remove("run_failure_contract")
                    .unwrap();
            }
            _ => unreachable!(),
        }
        omitted["subject_digest"] = json!(format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&omitted["canonical"]).unwrap())
        ));
        assert!(
            validate_approval_export(&raw, &omitted.to_string()).is_err(),
            "omission {omission}"
        );
    }
}

#[test]
fn stable_facts_ignore_offsets_and_node_allocation_but_bind_resolved_changes() {
    let baseline = analyze(FIXTURE, &[]);
    checked(&baseline);
    let spaced = analyze(
        &format!("\n// offsets and allocation\nenum AaaFirst {{ one }}\n{FIXTURE}"),
        &[],
    );
    checked(&spaced);
    assert_eq!(
        baseline.delivery_model().bindings()[0].source_facts(),
        spaced.delivery_model().bindings()[0].source_facts()
    );
    assert_ne!(
        baseline
            .semantics
            .node("ReminderServiceMembership")
            .unwrap()
            .id,
        spaced
            .semantics
            .node("ReminderServiceMembership")
            .unwrap()
            .id
    );
    let renamed = analyze(
        &FIXTURE.replace("ReminderServiceMembership", "MailAuthorityMembership"),
        &[],
    );
    checked(&renamed);
    let old: Value = serde_json::from_str(
        &derive_approval_subject(&root(), &baseline, None, None, None).unwrap(),
    )
    .unwrap();
    let new: Value = serde_json::from_str(
        &derive_approval_subject(&root(), &renamed, None, None, None).unwrap(),
    )
    .unwrap();
    assert_ne!(
        old["canonical"]["after"]["state_digest"],
        new["canonical"]["after"]["state_digest"]
    );
    assert_ne!(old["subject_digest"], new["subject_digest"]);
    assert_eq!(
        renamed.delivery_model().bindings()[0].source_facts()["obligations"]["authority"]
            ["membership"],
        "MailAuthorityMembership"
    );
}

#[test]
fn every_source_error_refuses_all_checked_bindings_and_private_nodes() {
    let invalid = [
        analyze(&FIXTURE.replacen("identity: Todo.id", "identity: Todo.title", 1), &[]),
        analyze(&FIXTURE.replacen("membership: ReminderServiceMembership", "membership: ServiceCredential", 1), &[]),
        analyze(&FIXTURE.replacen("patch: Todo.patch_todo", "patch: Todo.delete_todo", 1), &[]),
        analyze(&format!("{FIXTURE}\nfunction unrelated() -> MissingType {{}}"), &[]),
        analyze(&format!("{FIXTURE}\nfunction forge() -> Unit {{ job.overdue_reminders.delivery.reminder_v1.selection() }}"), &[]),
        analyze(&format!("{FIXTURE}\nfunction forge() -> Unit {{ reminder_candidates() }}"), &[]),
        analyze(&format!("{FIXTURE}\nfunction forge() -> Unit {{ reminder_sent() }}"), &[]),
        analyze(&format!("{FIXTURE}\n{}", FIXTURE[FIXTURE.find("job overdue_reminders").unwrap()..].replace("overdue_reminders", "competing_job")), &[]),
    ];
    for project in invalid {
        assert!(!errors(&project).is_empty());
        assert!(errors(&project).contains(&"TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED"));
        assert!(project.delivery_model().bindings().is_empty());
        assert!(!project.semantics.nodes.iter().any(|n| matches!(
            n.kind,
            NodeKind::DeliverySelection | NodeKind::DeliveryCompletion
        )));
        assert!(derive_approval_subject(&root(), &project, None, None, None).is_err());
    }
}

#[test]
fn incomplete_export_cannot_hide_authored_delivery_or_ordinary_schedule_rows() {
    let baseline = analyze(FIXTURE, &[]);
    checked(&baseline);
    let mut changed = baseline.clone();
    let descriptor = changed
        .syntax
        .sources
        .iter_mut()
        .flat_map(|file| &mut file.file.declarations)
        .find_map(|declaration| {
            let jadpo_syntax::Declaration::Job(job) = declaration else {
                return None;
            };
            job.delivery.as_mut()
        })
        .unwrap();
    descriptor.selection.identity.path[1].text = "title".into();
    assert!(derive_approval_subject(&root(), &changed, None, None, None).is_err());
    for mutation in 0..5 {
        let mut mutated = baseline.clone();
        match mutation {
            0 => mutated.failures.jobs[0].job = "unrelated_job".into(),
            1 => mutated.failures.jobs[0].callee = "unrelated_callee".into(),
            2 => mutated.failures.jobs[0].may_suspend = true,
            3 => mutated.failures.jobs[0]
                .failures
                .push("ForgedFailure".into()),
            4 => mutated.failures.jobs.push(mutated.failures.jobs[0].clone()),
            _ => unreachable!(),
        }
        assert!(
            derive_approval_subject(&root(), &mutated, None, None, None).is_err(),
            "mutation {mutation}"
        );
    }
    let mixed = analyze(&format!("{FIXTURE}\njob plain_job every 1m {{ concurrency: singleton run: reminder_tick(ReminderRunAt(clock.now)) retry: next_schedule }}"), &[]);
    checked(&mixed);
    let mut replaced = mixed.clone();
    let ordinary_failure = replaced
        .failures
        .jobs
        .iter()
        .find(|failure| failure.job == "plain_job")
        .unwrap()
        .clone();
    *replaced
        .failures
        .jobs
        .iter_mut()
        .find(|failure| failure.job == "overdue_reminders")
        .unwrap() = ordinary_failure;
    assert!(derive_approval_subject(&root(), &replaced, None, None, None).is_err());
    let mut missing = mixed.clone();
    missing.typing.jobs.clear();
    assert!(derive_approval_subject(&root(), &missing, None, None, None).is_err());
    let mut duplicated = mixed.clone();
    duplicated
        .typing
        .jobs
        .push(duplicated.typing.jobs[0].clone());
    assert!(derive_approval_subject(&root(), &duplicated, None, None, None).is_err());
    let mut missing_failure = mixed;
    missing_failure.failures.jobs.clear();
    assert!(derive_approval_subject(&root(), &missing_failure, None, None, None).is_err());
}

fn job_audit(project: &AnalyzedProject) -> Value {
    let artifacts = derive_artifacts(&root(), project);
    serde_json::from_str(&artifacts.iter().find(|artifact| artifact.relative_path == "audit/jobs.json").unwrap().contents).unwrap()
}

#[test]
fn canonical_and_checked_renamed_delivery_audits_match_emitted_lowering_without_execution_claims() {
    let canonical = jadpo_core::analyze_project(&root().join("examples/golden-todo-migration")).unwrap();
    let renamed = analyze(&FIXTURE.replace("overdue_reminders", "renamed_reminders"), &[]);
    for (project, name) in [(&canonical, "overdue_reminders"), (&renamed, "renamed_reminders")] {
        assert!(errors(project).is_empty());
        let target = derive_target(&root(), project).expect("checked exact source should emit closed native host");
        assert!(target.iter().any(|artifact| artifact.relative_path == "target/app.ts" && artifact.contents.contains("createDeliveryJobHost")));
        let audit = job_audit(project); assert_eq!(audit["scope"], "checked_nonexecuting_schedule_entries");
        let row = audit["jobs"].as_array().unwrap().iter().find(|row| row["job"] == name).unwrap();
        assert_eq!(row["runtime_lowering_supported"], true);
        assert_eq!(row["binding_matches_source_job"], true);
        assert_eq!(row["durable_failure_dispositions"], "emitted_checked_delivery_protocol");
        assert_eq!(row["runtime_conformance"], "not_certified_by_static_audit");
        assert_eq!(row["execution_profile"], Value::Null);
        assert_eq!(row["delivery"]["execution_profile"], Value::Null);
        assert_eq!(row["delivery"]["live_authority_established"], false);
        assert_eq!(row["delivery"]["obligations"]["runtime_supported"], false);
    }
}

#[test]
fn an_analyzed_binding_cannot_certify_a_changed_job_ast_in_audit_or_target() {
    let mut project = analyze(FIXTURE, &[]); checked(&project);
    let original = project.delivery_model().bindings()[0].source_facts();
    let job = project.syntax.sources.iter_mut().flat_map(|source| source.file.declarations.iter_mut()).find_map(|declaration| match declaration { jadpo_syntax::Declaration::Job(job) if job.name.text == "overdue_reminders" => Some(job), _ => None }).unwrap();
    job.every.text = "1m".to_owned();
    // The whole artifact boundary already refuses stale checked projects;
    // it must not publish even a partially supported audit row.
    assert_eq!(project.delivery_model().bindings()[0].source_facts(), original);
    assert!(std::panic::catch_unwind(|| derive_artifacts(&root(), &project)).is_err());
    assert!(derive_target(&root(), &project).is_err());
}
