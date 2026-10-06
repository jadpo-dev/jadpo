use jadpo_core::{analyze_sources, derive_approval_subject, validate_approval_export};
use jadpo_syntax::SourceFile;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

const SOURCE: &str = include_str!(
    "../../../../tests/validation/evidence/rm601-deployment-surface-metadata/fixture.jadpo"
);

fn subject(source: &str, before: Option<&str>) -> Value {
    let analyzed = |root: &str, s: &str| {
        analyze_sources(vec![SourceFile::new(
            Path::new(root).join("app.jadpo"),
            s.into(),
        )])
        .unwrap()
    };
    let after = analyzed("/project", source);
    let baseline = before.map(|s| analyzed("/baseline", s));
    let raw = derive_approval_subject(
        Path::new("/project/app.jadpo"),
        &after,
        baseline
            .as_ref()
            .map(|p| (Path::new("/baseline/app.jadpo"), p)),
        None,
        None,
    )
    .unwrap_or_else(|e| {
        panic!(
            "{e}: syntax={:?} typing={:?} failures={:?}",
            after.syntax.diagnostics().collect::<Vec<_>>(),
            after.typing.diagnostics,
            after.failures.diagnostics
        )
    });
    serde_json::from_str(&raw).unwrap()
}
fn surface(s: &Value) -> &Value {
    &s["canonical"]["after"]["impact"]["deployment"]
}
#[test]
fn pure_native_listener_is_optional_explicit_loopback_and_not_live() {
    let s = subject(SOURCE, None);
    let d = surface(&s);
    assert_eq!(d["status"], "compiler_supported_expected_native_contract");
    let l = &d["facts"]["listener"];
    assert_eq!(l["hostname"], "127.0.0.1");
    assert_eq!(l["port"]["required_api_argument"], true);
    assert_eq!(
        l["port"]["native_main_environment"],
        "JADPO_DEPLOYMENT_PORT"
    );
    assert_eq!(
        l["port"]["native_main_activation"],
        "only_when_explicitly_present"
    );
    assert!(l["port"]["resolved_value"].is_null());
    assert_eq!(
        d["facts"]["readiness"]["database_check"],
        "not_applicable_no_persistent_entities"
    );
    assert_eq!(
        d["facts"]["readiness"]["paths"],
        json!(["/health/ready", "/health"])
    );
    assert_eq!(d["facts"]["readiness"]["service_advisories"], json!({}));
    assert_eq!(d["runtime_conformance"], "not_established");
    for key in [
        "live_listener",
        "current_readiness",
        "worker_execution_profile",
    ] {
        assert!(d[key].is_null());
    }
    assert_eq!(d["release_authority"], false);
    assert_eq!(
        d["facts"]["application_plane"]["authored_routes_at_reserved_path"],
        "excluded_by_native_application_boundary"
    );
}
#[test]
fn persistent_and_nonpersistent_record_predicates_match_native_generator() {
    for entity in [
        "entity Item { id: Uuid identity }",
        "entity Item { id: Uuid { generated: identity } identity: id persistence { store: primary role: authority } }",
    ] {
        let s = subject(&format!("{entity}\n{SOURCE}"), None);
        assert_eq!(
            surface(&s)["facts"]["readiness"]["database_check"],
            "native_database_readiness"
        );
        assert_eq!(
            surface(&s)["facts"]["readiness"]["response_statuses"],
            json!([200, 503])
        );
    }
    let s = subject(&format!("output Item {{ id: Uuid }}\n{SOURCE}"), None);
    assert_eq!(
        surface(&s)["facts"]["readiness"]["database_check"],
        "not_applicable_no_persistent_entities"
    );
}
#[test]
fn configuration_preflight_and_changed_database_readiness_have_exact_decisions() {
    let changed=format!("config Settings {{ region: Text {{ binding: \"REGION\" }} }}\nentity Item {{ id: Uuid identity }}\n{SOURCE}");
    let s = subject(&changed, Some(SOURCE));
    let d = surface(&s);
    assert_eq!(d["facts"]["declared_configuration_preflight"], true);
    assert!(s["canonical"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["subject"] == "deployment_surface:readiness" && r["change"] == "changed"));
    assert!(s["canonical"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["subject"] == "impact:deployment" && r["change"] == "changed"));
}
#[test]
fn unsupported_native_target_has_no_fabricated_deployment_facts() {
    let source="type ScanTime = Instant {}\naction scan(at: ScanTime) -> Unit {}\njob reminders every 15m { concurrency: singleton run: scan(ScanTime(clock.now)) retry: next_schedule }";
    let s = subject(source, None);
    let d = surface(&s);
    assert_eq!(d["status"], "target_generation_rejected");
    assert!(d["facts"].is_null());
    assert_eq!(d["diagnostic"]["code"], "JADPO_TARGET_JOB_NOT_IMPLEMENTED");
    assert_eq!(
        d["diagnostic"],
        s["canonical"]["after"]["provenance"]["generated_artifacts"]["target"]["result"]
            ["diagnostic"]
    );
}
#[test]
fn audit_and_canonical_fact_coverage_share_exact_generator_contract() {
    let p = analyze_sources(vec![SourceFile::new(
        "/project/app.jadpo".into(),
        SOURCE.into(),
    )])
    .unwrap();
    let artifacts = jadpo_core::derive_artifacts(Path::new("/project/app.jadpo"), &p);
    let audit = artifacts
        .iter()
        .find(|a| a.relative_path == "audit/deployment.json")
        .unwrap();
    let v: Value = serde_json::from_str(&audit.contents).unwrap();
    let s = subject(SOURCE, None);
    assert_eq!(&v, surface(&s));
    assert!(
        s["canonical"]["after"]["provenance"]["generated_artifacts"]["metadata_and_audit"]["files"]
            ["audit/deployment.json"]
            .is_object()
    );
}
#[test]
fn coherently_rehashed_all_copy_deployment_omission_and_port_forgery_refuse() {
    let s = subject(SOURCE, None);
    let raw = serde_json::to_string(&s).unwrap();
    assert!(validate_approval_export(&raw, &raw).is_ok());
    fn mutate(v: &mut Value, omit: bool) -> usize {
        match v {
            Value::Object(o) => {
                let mut n = 0;
                if o.get("native_main_environment") == Some(&json!("JADPO_DEPLOYMENT_PORT")) {
                    if omit {
                        o.remove("native_main_environment");
                    } else {
                        o.insert("native_main_environment".into(), json!("FORGED_PORT"));
                    }
                    n += 1;
                }
                n + o.values_mut().map(|v| mutate(v, omit)).sum::<usize>()
            }
            Value::Array(a) => a.iter_mut().map(|v| mutate(v, omit)).sum(),
            _ => 0,
        }
    }
    for omit in [false, true] {
        let mut forged = s.clone();
        assert!(mutate(&mut forged["canonical"], omit) > 1);
        forged["subject_digest"] = json!(format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&forged["canonical"]).unwrap())
        ));
        assert!(validate_approval_export(&raw, &serde_json::to_string(&forged).unwrap()).is_err());
    }
}
#[test]
fn accepted_service_advisories_are_static_unprobed_and_do_not_qualify_provider_health() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let project_path = root.join("examples/golden-todo-migration");
    let project = jadpo_core::analyze_project(&project_path).unwrap();
    let s: Value = serde_json::from_str(
        &derive_approval_subject(&project_path, &project, None, None, None).unwrap(),
    )
    .unwrap();
    let d = surface(&s);
    assert_eq!(
        d["facts"]["readiness"]["service_advisories"],
        json!({"reminder_mail":"unprobed"})
    );
    assert_eq!(
        d["facts"]["readiness"]["advisories_qualify_provider_health"],
        false
    );
    assert_eq!(d["facts"]["declared_configuration_preflight"], true);
    assert_eq!(d["runtime_conformance"], "not_established");
}
#[test]
fn nonlocal_authored_liveness_refusal_remains_target_rejection() {
    let source = SOURCE.replace("action: { return Health { ok: true } }", "run: health()");
    let s = subject(&source, None);
    let d = surface(&s);
    assert_eq!(d["status"], "target_generation_rejected");
    assert!(d["facts"].is_null());
    assert_eq!(d["diagnostic"]["code"], "JADPO_TARGET_LIVENESS_NOT_LOCAL");
}
