use jadpo_core::{
    analyze_sources, derive_approval_subject, derive_artifacts, derive_target,
    validate_approval_export,
};
use jadpo_syntax::SourceFile;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

const POLICY: &str = "compiler_guarded_bounded_default";
const SOURCE: &str = r#"
failure Missing { kind: NotFound code: "missing" }
failure AuditConflict { kind: Conflict code: "conflict" }
entity Customer {
    id: Uuid
    email: Email
    identity: id
    persistence { store: primary role: authority unique: email }
    action change_email(id: Customer.id, email: Customer.email)
        fails Missing, AuditConflict -> Customer {
        return attempt update required Customer {
            where: id == id set: { email: email }
            missing: Missing conflict: AuditConflict
        }
    }
}
"#;

fn project() -> jadpo_core::AnalyzedProject {
    analyze_sources(vec![SourceFile::new(
        "/project/app.jadpo".into(),
        SOURCE.into(),
    )])
    .unwrap()
}

fn subject(project: &jadpo_core::AnalyzedProject) -> Value {
    serde_json::from_str(
        &derive_approval_subject(Path::new("/project/app.jadpo"), project, None, None, None)
            .expect("checked transaction fixture"),
    )
    .unwrap()
}

fn hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

// serde_json's default Map sorts object keys; arrays keep semantic order.
// Verify this transport rehash against the real producer before forging.
fn rehash(subject: &mut Value) {
    subject["subject_digest"] = json!(hash(&serde_json::to_vec(&subject["canonical"]).unwrap()));
}

fn cohere_false_export(subject: &mut Value) {
    assert!(subject["canonical"]["before"].is_null());
    let state = &mut subject["canonical"]["after"];
    for (category, value) in state["impact"].as_object().unwrap().clone() {
        state["facts"][format!("impact:{category}")] = value;
    }
    let outputs = state["provenance"]["generated_artifacts"].clone();
    state["facts"]["provenance:generated_artifacts"] = outputs.clone();
    for category in ["metadata_and_audit", "target"] {
        for (path, pin) in outputs[category]["files"].as_object().unwrap() {
            state["facts"][format!("artifact_pin:{path}")] = pin.clone();
        }
    }
    state["behavior_digest"] = json!(hash(&serde_json::to_vec(&state["facts"]).unwrap()));
    state.as_object_mut().unwrap().remove("state_digest");
    state["state_digest"] = json!(hash(&serde_json::to_vec(state).unwrap()));
    let facts = state["facts"].clone();
    for decision in subject["canonical"]["decisions"].as_array_mut().unwrap() {
        let key = decision["subject"].as_str().unwrap().to_owned();
        decision["after"] = facts.get(&key).unwrap().clone();
    }
    rehash(subject);
}

#[test]
fn global_retry_policy_is_the_same_bounded_descriptor_in_audit_and_approval() {
    let project = project();
    let artifacts = derive_artifacts(Path::new("/project/app.jadpo"), &project);
    let output = artifacts
        .iter()
        .find(|a| a.relative_path == "audit/transactions.json")
        .unwrap();
    let audit: Value = serde_json::from_str(&output.contents).unwrap();
    assert_eq!(audit["schema_version"], 1);
    assert_eq!(audit["model"], "TX-001");
    assert_eq!(audit["actions"].as_array().unwrap().len(), 1);
    assert_eq!(audit["actions"][0]["retry"], POLICY);
    assert_eq!(
        audit["actions"][0],
        json!({
            "action": "Customer.change_email", "mutation_owners": ["Customer"],
            "disposition": "entity_action", "domain": "primary",
            "nested_actions": "join_declared_boundary", "handled_failure": "savepoint_rollback",
            "commit": "outer_success", "rollback": "propagated_failure",
            "postgres": {"isolation": "read_committed", "concurrency": "row_lock_or_checked_revision", "deadlock_order": "stable_entity_identity"},
            "sqlite": {"isolation": "immediate_serialized", "concurrency": "single_immediate_writer"},
            "retry": POLICY,
        })
    );
    let reviewed = subject(&project);
    let state = &reviewed["canonical"]["after"];
    assert_eq!(state["facts"]["audit:transactions"], audit);
    assert_eq!(state["impact"]["transactions"]["facts"], audit);
    assert_eq!(
        state["impact"]["retries"],
        json!({
            "status": "compiler_checked_contract_only", "facts": audit,
            "runtime_conformance": "not_established",
        })
    );
    let descriptor = &state["provenance"]["generated_artifacts"]["metadata_and_audit"]["files"]
        ["audit/transactions.json"];
    assert_eq!(descriptor["byte_digest"], hash(output.contents.as_bytes()));
    assert_eq!(descriptor["byte_length"], output.contents.len());
    assert_eq!(
        state["facts"]["artifact_pin:audit/transactions.json"],
        *descriptor
    );
    assert_eq!(reviewed["canonical"]["release_authority"], false);
    assert_eq!(
        reviewed["canonical"]["evidence"]["runtime_validated"],
        json!([])
    );
    let text = serde_json::to_string(&reviewed).unwrap();
    assert!(validate_approval_export(&text, &text).is_ok());
    let targets = derive_target(Path::new("/project/app.jadpo"), &project).unwrap();
    let app = &targets
        .iter()
        .find(|a| a.relative_path == "target/app.ts")
        .unwrap()
        .contents;
    assert!(
        !app.contains(POLICY),
        "metadata descriptor never authorizes runtime replay"
    );
    assert!(app.contains("replayable: true && __operation.retryAllowed"));
    let persistence = &targets
        .iter()
        .find(|a| a.relative_path == "target/persistence.ts")
        .unwrap()
        .contents;
    for guard in [
        "attempt < 2",
        "evidence.outcome === \"no_commit\"",
        "evidence.rollbackProven",
        "retryOptions.startedAt",
        "retryOptions.signal?.aborted",
        "Math.random() * Math.min(250, 50 * (2 ** attempt), remainingMs)",
    ] {
        assert!(
            persistence.contains(guard),
            "missing unchanged runtime guard: {guard}"
        );
    }
}

#[test]
fn actual_fresh_export_validator_refuses_coherently_rehashed_false_retry_facts() {
    let project = project();
    let expected = subject(&project);
    let text = serde_json::to_string(&expected).unwrap();
    let artifacts = derive_artifacts(Path::new("/project/app.jadpo"), &project);
    let audit_bytes = &artifacts
        .iter()
        .find(|a| a.relative_path == "audit/transactions.json")
        .unwrap()
        .contents;
    let mut reference = expected.clone();
    cohere_false_export(&mut reference);
    assert_eq!(
        reference, expected,
        "forge rehash uses the actual canonical contract"
    );
    for false_policy in ["explicit_idempotent_only", "all_actions_retryable"] {
        let mut forged = expected.clone();
        for pointer in [
            "/canonical/after/facts/audit:transactions",
            "/canonical/after/impact/transactions/facts",
            "/canonical/after/impact/retries/facts",
        ] {
            forged.pointer_mut(pointer).unwrap()["actions"][0]["retry"] = json!(false_policy);
        }
        let bytes = audit_bytes.replace(POLICY, false_policy).into_bytes();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            forged["canonical"]["after"]["facts"]["audit:transactions"]
        );
        // Rebuild every duplicated projection, pin, decision and enclosing digest.
        for pointer in [
            "/canonical/after/facts/artifact_pin:audit~1transactions.json",
            "/canonical/after/provenance/generated_artifacts/metadata_and_audit/files/audit~1transactions.json",
        ] {
            let pin = forged.pointer_mut(pointer).unwrap();
            pin["byte_digest"] = json!(hash(&bytes));
            pin["byte_length"] = json!(bytes.len());
        }
        cohere_false_export(&mut forged);
        assert_eq!(
            forged["canonical"]["after"]["facts"]["impact:transactions"],
            forged["canonical"]["after"]["impact"]["transactions"]
        );
        assert_eq!(
            forged["canonical"]["after"]["facts"]["impact:retries"],
            forged["canonical"]["after"]["impact"]["retries"]
        );
        assert_ne!(
            forged["canonical"]["after"]["behavior_digest"],
            expected["canonical"]["after"]["behavior_digest"]
        );
        assert_ne!(
            forged["canonical"]["after"]["state_digest"],
            expected["canonical"]["after"]["state_digest"]
        );
        assert_ne!(forged["subject_digest"], expected["subject_digest"]);
        let exported = serde_json::to_string(&forged).unwrap();
        assert!(
            validate_approval_export(&exported, &exported).is_ok(),
            "well-formed JSON/digest self-control is not fresh-source agreement"
        );
        assert!(validate_approval_export(&text, &exported).is_err());
    }
    let mut certified = expected.clone();
    certified["canonical"]["after"]["impact"]["retries"]["runtime_conformance"] =
        json!("certified");
    cohere_false_export(&mut certified);
    assert_eq!(
        certified["canonical"]["after"]["facts"]["impact:retries"]["runtime_conformance"],
        "certified"
    );
    let exported = serde_json::to_string(&certified).unwrap();
    assert!(validate_approval_export(&exported, &exported).is_ok());
    assert!(validate_approval_export(&text, &exported).is_err());
}

#[test]
fn same_compiler_before_source_recomputes_policy_without_fabricating_permission_delta() {
    let project = project();
    let reviewed: Value = serde_json::from_str(
        &derive_approval_subject(
            Path::new("/project/app.jadpo"),
            &project,
            Some((Path::new("/project/app.jadpo"), &project)),
            None,
            None,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        reviewed["canonical"]["before"]["facts"]["audit:transactions"]["actions"][0]["retry"],
        POLICY
    );
    assert_eq!(
        reviewed["canonical"]["before"],
        reviewed["canonical"]["after"]
    );
    assert_eq!(reviewed["canonical"]["decisions"], json!([]));
}
