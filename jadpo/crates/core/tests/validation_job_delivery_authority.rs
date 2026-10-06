//! Exact selected identity composition, not live phase permissions/admission.
use jadpo_core::{analyze_sources, derive_approval_subject, discover_sources, AnalyzedProject};
use jadpo_syntax::{Declaration, SourceFile};
use serde_json::json;
use std::path::PathBuf;
const FIXTURE: &str =
    include_str!("../../../../tests/validation/fixtures/reminder-delivery-v1.jadpo");
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_owned()
}
fn analyze(fixture: &str, change: Option<(&str, &str, &str)>) -> AnalyzedProject {
    let root = root();
    let mut sources = discover_sources(&root.join("examples/golden-todo-migration")).unwrap();
    // Candidate fixture owns its delivery declarations; canonical checked
    // source is excluded to avoid duplicate names while retaining contracts.
    sources.retain(|source| !source.path.ends_with("jobs/reminders.jadpo"));
    if let Some((file, before, after)) = change {
        let source = sources
            .iter_mut()
            .find(|source| source.path.ends_with(file))
            .unwrap();
        assert!(source.text.contains(before), "{file}: {before}");
        source.text = source.text.replacen(before, after, 1);
    }
    sources.push(SourceFile::new(
        root.join("tests/validation/fixtures/reminder-delivery-v1.jadpo"),
        fixture.to_owned(),
    ));
    analyze_sources(sources).unwrap()
}
fn codes(project: &AnalyzedProject) -> Vec<&str> {
    project
        .syntax
        .diagnostics()
        .chain(project.semantics.diagnostics.iter())
        .chain(project.typing.diagnostics.iter())
        .chain(project.failures.diagnostics.iter())
        .map(|diagnostic| diagnostic.code)
        .collect()
}
fn refusal(project: &AnalyzedProject) {
    assert!(
        project.syntax.diagnostics().next().is_none(),
        "{:?}",
        codes(project)
    );
    assert!(project.typing.jobs.is_empty());
    assert!(project.delivery_model().bindings().is_empty());
    assert!(codes(project).contains(&"TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED"));
    assert!(derive_approval_subject(&root(), project, None, None, None).is_err());
}
fn checked_nonexecuting(project: &AnalyzedProject) {
    assert!(codes(project).is_empty(), "{:?}", codes(project));
    assert!(project.typing.jobs.is_empty());
    assert_eq!(project.delivery_model().bindings().len(), 1);
    assert!(derive_approval_subject(&root(), project, None, None, None).is_ok());
}

#[test]
fn authority_baseline_retains_actual_selected_membership_dossier_nonexecuting() {
    let project = analyze(FIXTURE, None);
    checked_nonexecuting(&project);
    let membership = project
        .syntax
        .sources
        .iter()
        .flat_map(|file| &file.file.declarations)
        .find_map(|declaration| match declaration {
            Declaration::Record(record) if record.name.text == "ReminderServiceMembership" => {
                Some(record)
            }
            _ => None,
        })
        .unwrap();
    let dossier = membership.dossier.as_ref().unwrap();
    assert_eq!(dossier.identity.text, "id");
    assert_eq!(dossier.persistence.as_ref().unwrap().store.text, "primary");
    assert_eq!(
        membership
            .fields
            .iter()
            .find(|field| field.name.text == "service_id")
            .unwrap()
            .reference
            .as_ref()
            .unwrap()
            .target
            .path
            .iter()
            .map(|part| part.text.as_str())
            .collect::<Vec<_>>(),
        ["Service", "id"]
    );
}

#[test]
fn selected_membership_role_store_and_identity_are_not_global_role_evidence() {
    for (before, after) in [
        (
            "validator: api_bearer.service_key",
            "validator: api_bearer.signed_user",
        ),
        (
            "validator: api_bearer.service_key",
            "validator: api_bearer.service_signed",
        ),
        (
            "validator: api_bearer.service_key",
            "validator: browser_session.service_key",
        ),
        (
            "membership: ReminderServiceMembership",
            "membership: ServiceCredential",
        ),
        ("role: ReminderRole.sender", "role: TodoRole.owner"),
        ("role: ReminderRole.sender", "role: ReminderRole.missing"),
        ("store: primary", "store: replica"),
        ("scope: application", "scope: service_id"),
        ("role: ReminderRole\n", "role: ReminderRole optional\n"),
        ("service_id: Service.id", "service_id: Service.owner_id"),
        ("identity: id\n", ""),
        ("id: Uuid\n", "id: Uuid optional\n"),
    ] {
        let project = analyze(&FIXTURE.replacen(before, after, 1), None);
        refusal(&project);
        assert!(
            codes(&project).contains(&"TYPE_JOB_DELIVERY_AUTHORITY_INVALID"),
            "{before}: {:?}",
            codes(&project)
        );
    }
    let extra = "\nentity WrongMembership { id: Uuid service_id: User.id role: ReminderRole identity: id persistence { store: primary role: authority references service_id: User.id as: user on_delete: restrict } membership { scope: application member: service_id role: role } }\n";
    let fixture = FIXTURE.replacen(
        "membership: ReminderServiceMembership",
        "membership: WrongMembership",
        1,
    ) + extra;
    let project = analyze(&fixture, None);
    refusal(&project);
    assert!(project
        .policy
        .memberships
        .iter()
        .any(
            |membership| membership.entity == "ReminderServiceMembership"
                && membership.principal == "Service"
        ));
    assert!(
        project
            .policy
            .memberships
            .iter()
            .any(|membership| membership.entity == "WrongMembership"
                && membership.principal == "User")
    );
    assert!(codes(&project).contains(&"TYPE_JOB_DELIVERY_AUTHORITY_INVALID"));
}

#[test]
fn credential_reference_and_principal_resolution_must_compose_to_exact_service_id() {
    for change in [
        (
            "authentication.jadpo",
            "mode: api_key\n            principal: service",
            "mode: api_key\n            principal: user",
        ),
        (
            "authentication.jadpo",
            "principal: ServiceCredential.service_id",
            "principal: ServiceCredential.id",
        ),
        (
            "authentication.jadpo",
            "active: status == CredentialStatus.active",
            "active: status == CredentialStatus.revoked",
        ),
        (
            "authentication.jadpo",
            "active: status == ServiceStatus.active",
            "active: status == ServiceStatus.disabled",
        ),
        (
            "authentication.jadpo",
            "authority: Service.name",
            "authority: User.authentication_subject",
        ),
        (
            "authentication.jadpo",
            "id -> Principal.service.service_id",
            "owner_id -> Principal.service.service_id",
        ),
        (
            "entities/service_credential.jadpo",
            "references service_id: Service.id",
            "references service_id: Service.owner_id",
        ),
        (
            "entities/service_credential.jadpo",
            "expires_at: Instant",
            "expires_at: Instant?",
        ),
        (
            "entities/service_credential.jadpo",
            "revoked_at: Instant?",
            "revoked_at: Instant",
        ),
        (
            "entities/service_credential.jadpo",
            "store: primary",
            "store: replica",
        ),
        ("entities/service.jadpo", "store: primary", "store: replica"),
        ("application.jadpo", "service_id: Uuid", "service_id: Uuid?"),
    ] {
        let project = analyze(FIXTURE, Some(change));
        refusal(&project);
        assert!(
            codes(&project).contains(&"TYPE_JOB_DELIVERY_AUTHORITY_INVALID"),
            "{change:?}: {:?}",
            codes(&project)
        );
    }
}

#[test]
fn competing_jobs_cannot_reuse_selected_entity_or_service_binding() {
    let second = FIXTURE[FIXTURE.find("job overdue_reminders").unwrap()..].replacen(
        "overdue_reminders",
        "competing_reminders",
        1,
    );
    let project = analyze(&format!("{FIXTURE}\n{second}"), None);
    refusal(&project);
    assert!(codes(&project).contains(&"TYPE_JOB_DELIVERY_AUTHORITY_INVALID"));
    // Necessary type candidates are explicitly incomplete, never checked rows.
    assert_eq!(project.typing.delivery_candidates.len(), 2);
    assert!(project.delivery_phase_candidates().is_empty());
}

#[test]
fn closed_phase_candidates_have_exact_scoped_targets_and_unproved_requirements() {
    let project = analyze(FIXTURE, None);
    assert_eq!(project.delivery_phase_candidates().len(), 1);
    let facts = project.delivery_phase_candidates()[0].candidate_facts();
    let authority = json!({
        "application": "TodoApplication",
        "validator": "authentication.api_bearer.validator.service_key.api_key",
        "credential_identity": "ServiceCredential.id",
        "credential_principal_reference": {"field": "ServiceCredential.service_id", "target": "Service.id"},
        "credential_verifier": "ServiceCredential.verifier",
        "credential_active": {"field": "ServiceCredential.status", "equals": "CredentialStatus.active"},
        "credential_expires": "ServiceCredential.expires_at",
        "credential_revoked": "ServiceCredential.revoked_at",
        "service_identity": "Service.id",
        "service_resolution": "authentication.api_bearer.resolution.service",
        "resolution_authority": "Service.name",
        "service_active": {"field": "Service.status", "equals": "ServiceStatus.active"},
        "principal_identity_mapping": {"source": "Service.id", "target": "Principal.service.service_id"},
        "membership": "ReminderServiceMembership", "membership_identity": "ReminderServiceMembership.id",
        "member_reference": {"field": "ReminderServiceMembership.service_id", "target": "Service.id"},
        "role_field": "ReminderServiceMembership.role", "role": "ReminderRole.sender",
        "scope": "application", "authority_store": "primary",
    });
    let binding = "job.overdue_reminders.delivery.reminder_v1";
    let sorted = |fields: &[&str]| {
        let mut fields = fields.to_vec();
        fields.sort();
        json!(fields)
    };
    let private = |names: &[&str]| {
        let mut names = names
            .iter()
            .map(|name| format!("{binding}.{name}"))
            .collect::<Vec<_>>();
        names.sort();
        json!(names)
    };
    let reads = [
        "Todo.id",
        "Todo.due_at",
        "Todo.reminder_sent_at",
        "Todo.status",
        "Todo.title",
        "Todo.owner",
        "User.email",
        "User.status",
    ];
    let mut admission_reads = reads.to_vec();
    admission_reads.extend([
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
    ]);
    let expected_rows = [
        ("selection", sorted(&reads), sorted(&["Todo.lifecycle.visibility", "User.lifecycle.visibility"]), json!([]),
            private(&["revision_intent", "todo_schedule_revision"]), private(&["revision_intent"]), json!(null),
            vec!["non_null_strictly_overdue_open_unsent", "todo_visible", "owner_active_visible", "recipient_authorized_before_limit",
                "ordered_due_identity_limit_500", "due_identity_keyset_continuation", "transactional_unique_revision_intent", "immutable_payload"]),
        ("admission", sorted(&admission_reads), sorted(&["Todo.lifecycle.visibility", "User.lifecycle.visibility"]), json!([]),
            private(&["revision_intent", "claim_fence", "todo_schedule_revision", "invocation_budget"]), private(&["revision_intent", "claim_fence", "invocation_budget"]), json!(null),
            vec!["current_revision_and_eligibility", "immutable_recipient_active_authorized", "live_credential_service_membership_role",
                "credential_active", "credential_not_expired", "credential_not_revoked", "selected_service_active",
                "selected_service_identity_resolved", "selected_application_membership_present_current",
                "selected_role_equals_and_authorizes_operation", "payload_equals_enrolled_immutable_payload",
                "exact_external_operation", "remaining_shared_budget", "commit_fence_and_possible_dispatch_before_io"]),
        ("dispatch", json!([]), json!([]), json!([]), private(&["revision_intent", "claim_fence", "invocation_budget"]), json!([]),
            json!("ReminderMail.send_overdue_reminder"), vec!["admitted_authority_only_no_extra_live_cut",
                "consume_shared_admitted_budget_without_reset",
                "frozen_provider_attempts_timeout_egress_outcomes", "no_transaction_across_provider_io", "no_automatic_unknown_replay"]),
        ("completion", sorted(&["Todo.id", "Todo.reminder_sent_at"]), json!([]), json!(["Todo.reminder_sent_at"]),
            private(&["revision_intent", "claim_fence", "receipt", "todo_schedule_revision"]), private(&["revision_intent", "receipt"]), json!(null),
            vec!["exact_key_validated_receipt", "retain_receipt_on_original_intent", "current_fencing_generation",
                "original_schedule_revision_equals_current", "todo_still_unsent", "compiler_receipt_observation_time_only", "same_completion_transaction"]),
    ].into_iter().map(|(phase, read_targets, predicate_targets, write_targets, private_read_targets, private_write_targets, invoke_target, requirements)| {
        json!({"job": "overdue_reminders", "binding": binding, "phase": phase, "authority": authority,
            "read_targets": read_targets, "predicate_targets": predicate_targets, "write_targets": write_targets,
            "private_read_targets": private_read_targets, "private_write_targets": private_write_targets,
            "invoke_target": invoke_target, "unproved_requirements": requirements,
            "unproved_predicates": expected_live_predicates(binding, phase), "live_authority_established": false})
    }).collect::<Vec<_>>();
    assert_eq!(
        facts,
        json!({
            "status": "incomplete_nonexecuting_candidate", "job": "overdue_reminders", "binding": binding,
            "authority": authority, "selection": "reminder_candidates", "completion": "reminder_sent",
            "operation": "ReminderMail.send_overdue_reminder",
            "open": {"field": "Todo.status", "variant": "TodoStatus.open"},
            "owner_active": {"field": "User.status", "equals": "UserStatus.active"},
            "order": ["Todo.due_at", "Todo.id"], "limit": 500, "payload_version": "reminder.v1",
            "payload": {"idempotency_key": "generated_intent", "from": "config.mail_sender", "to": "User.email", "todo_title": "Todo.title", "due_at": "Todo.due_at"},
            "phases": expected_rows, "runtime_supported": false, "execution_profile": null,
        })
    );
    // Static finish must not turn the empty ordinary run into live authority.
    checked_nonexecuting(&project);
    assert_eq!(
        project
            .semantics
            .calls
            .iter()
            .filter(
                |edge| project.semantics.nodes[edge.caller.0 as usize].name == "overdue_reminders"
            )
            .count(),
        1
    );
}

fn expected_live_predicates(binding: &str, phase: &str) -> serde_json::Value {
    match phase {
        "admission" => json!([
            {"requirement":"credential_active", "targets":["ServiceCredential.status"], "equals":"CredentialStatus.active", "established":false},
            {"requirement":"credential_not_expired", "targets":["ServiceCredential.expires_at"], "established":false},
            {"requirement":"credential_not_revoked", "targets":["ServiceCredential.revoked_at"], "established":false},
            {"requirement":"selected_service_active", "targets":["Service.status"], "equals":"ServiceStatus.active", "established":false},
            {"requirement":"selected_service_identity_resolved", "targets":["ServiceCredential.service_id", "Service.id", "Principal.service.service_id"], "established":false},
            {"requirement":"selected_application_membership_present_current", "targets":["ReminderServiceMembership.id", "ReminderServiceMembership.service_id", "Service.id"], "scope":"application", "store":"primary", "established":false},
            {"requirement":"selected_role_equals_and_authorizes_operation", "targets":["ReminderServiceMembership.role", "ReminderMail.send_overdue_reminder"], "equals":"ReminderRole.sender", "established":false},
            {"requirement":"payload_equals_enrolled_immutable_payload", "targets":[format!("{binding}.revision_intent")], "established":false}
        ]),
        "dispatch" => json!([
            {"requirement":"consume_shared_admitted_budget_without_reset", "targets":[format!("{binding}.invocation_budget")], "established":false}
        ]),
        _ => json!([]),
    }
}

#[test]
fn individual_live_predicates_and_shared_dispatch_budget_are_bound_and_unproved() {
    for (fixture, job) in [
        (FIXTURE.to_owned(), "overdue_reminders"),
        (
            FIXTURE.replacen("job overdue_reminders", "job renamed_reminders", 1),
            "renamed_reminders",
        ),
    ] {
        let project = analyze(&fixture, None);
        checked_nonexecuting(&project);
        let facts = project.delivery_phase_candidates()[0].candidate_facts();
        let binding = format!("job.{job}.delivery.reminder_v1");
        let budget = json!(format!("{binding}.invocation_budget"));
        let rows = facts["phases"].as_array().unwrap();
        for row in [&rows[1], &rows[2]] {
            assert!(row["private_read_targets"]
                .as_array()
                .unwrap()
                .contains(&budget));
            assert_eq!(
                row["unproved_predicates"],
                expected_live_predicates(&binding, row["phase"].as_str().unwrap())
            );
        }
        assert_eq!(rows[3]["predicate_targets"], json!([]));
        assert_eq!(rows[3]["write_targets"], json!(["Todo.reminder_sent_at"]));
    }
}

#[test]
fn phase_candidates_bind_selected_job_and_reject_inactive_recipient_visibility() {
    let baseline = analyze(FIXTURE, None);
    let renamed = analyze(
        &FIXTURE.replacen("job overdue_reminders", "job renamed_reminders", 1),
        None,
    );
    let original = baseline.delivery_phase_candidates()[0].candidate_facts();
    let changed = renamed.delivery_phase_candidates()[0].candidate_facts();
    assert_eq!(original["authority"], changed["authority"]);
    assert_eq!(
        changed["binding"],
        "job.renamed_reminders.delivery.reminder_v1"
    );
    for row in changed["phases"].as_array().unwrap() {
        assert_eq!(row["job"], "renamed_reminders");
        for target in row["private_read_targets"]
            .as_array()
            .unwrap()
            .iter()
            .chain(row["private_write_targets"].as_array().unwrap())
        {
            assert!(target
                .as_str()
                .unwrap()
                .starts_with("job.renamed_reminders.delivery.reminder_v1."));
        }
    }
    let invalid = analyze(
        FIXTURE,
        Some((
            "entities/user.jadpo",
            "visible when status == UserStatus.active",
            "visible when status == UserStatus.disabled",
        )),
    );
    assert!(invalid.delivery_phase_candidates().is_empty());
    assert!(codes(&invalid).contains(&"TYPE_JOB_DELIVERY_AUTHORITY_INVALID"));
    refusal(&invalid);
}

#[test]
fn phase_candidate_facts_ignore_source_offsets_and_node_allocation_order() {
    let ordinary = analyze(FIXTURE, None);
    let spaced = analyze(&format!("\n\n// Unrelated nominal type changes graph allocation, not bound identity.\ntype AAUnrelated = Text {{}}\n{FIXTURE}"), None);
    assert_eq!(
        ordinary.delivery_phase_candidates()[0].candidate_facts(),
        spaced.delivery_phase_candidates()[0].candidate_facts()
    );
    assert!(
        ordinary
            .semantics
            .node("ReminderServiceMembership")
            .unwrap()
            .id
            != spaced
                .semantics
                .node("ReminderServiceMembership")
                .unwrap()
                .id
    );
    checked_nonexecuting(&spaced);
}
