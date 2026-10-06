use jadpo_core::{analyze_sources, derive_approval_subject, derive_artifacts, derive_target};
use jadpo_syntax::SourceFile;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

const SOURCE: &str = "output Health { ok: Bool }\naction health() -> Health { return Health { ok: true } }\nroute GET /health { auth: none output: Health run: health() }";

fn project(root: &str, source: &str) -> jadpo_core::AnalyzedProject {
    analyze_sources(vec![SourceFile::new(
        Path::new(root).join("app.jadpo"),
        source.into(),
    )])
    .unwrap()
}

fn subject(source: &str, before: Option<&str>) -> Value {
    let after = project("/project", source);
    let before = before.map(|source| project("/baseline", source));
    serde_json::from_str(
        &derive_approval_subject(
            Path::new("/project/app.jadpo"),
            &after,
            before
                .as_ref()
                .map(|project| (Path::new("/baseline/app.jadpo"), project)),
            None,
            None,
        )
        .unwrap(),
    )
    .unwrap()
}

fn pins(subject: &Value) -> &Value {
    &subject["canonical"]["after"]["provenance"]["generated_artifacts"]
}

#[test]
fn approval_pins_every_actual_generated_output_except_explicit_self_exclusions() {
    let project = project("/project", SOURCE);
    let path = Path::new("/project/app.jadpo");
    let expected = subject(SOURCE, None);
    assert_eq!(pins(&expected)["status"], "fresh_expected_generator_bytes");
    assert_eq!(
        pins(&expected)["target"]["result"]["status"],
        "fresh_generator_output"
    );
    let mut count = 0;
    for (category, outputs) in [
        ("metadata_and_audit", derive_artifacts(path, &project)),
        ("target", derive_target(path, &project).unwrap()),
    ] {
        for output in outputs
            .iter()
            .filter(|output| !output.relative_path.starts_with("approval/"))
        {
            let descriptor = &pins(&expected)[category]["files"][output.relative_path];
            assert_eq!(
                descriptor["byte_digest"],
                format!("sha256:{:x}", Sha256::digest(output.contents.as_bytes()))
            );
            assert_eq!(descriptor["byte_length"], output.contents.len());
            assert_eq!(
                &expected["canonical"]["after"]["facts"]
                    [format!("artifact_pin:{}", output.relative_path)],
                descriptor
            );
            count += 1;
        }
    }
    assert_eq!(
        count,
        pins(&expected)["metadata_and_audit"]["files"]
            .as_object()
            .unwrap()
            .len()
            + pins(&expected)["target"]["files"]
                .as_object()
                .unwrap()
                .len()
    );
    assert_eq!(
        pins(&expected)["excluded"],
        json!(["approval/subject.json", "approval/subject.txt"])
    );
    assert_eq!(pins(&expected)["written_file_conformance"], "not_checked");
    assert_eq!(
        expected["canonical"]["after"]["impact"]["deployment"]["status"],
        "compiler_supported_expected_native_contract"
    );
    assert_eq!(expected["canonical"]["after"]["impact"]["deployment"]["runtime_conformance"], "not_established");
    assert!(expected["canonical"]["after"]["impact"]["deployment"]["live_listener"].is_null());
    assert!(
        expected["canonical"]["after"]["provenance"]["compiler"]["binary_attestation"].is_null()
    );
}

#[test]
fn generated_target_rejection_is_distinct_from_supported_empty_output() {
    let reviewed=subject("type ScanTime = Instant {}\naction scan(at: ScanTime) -> Unit {}\njob reminders every 15m { concurrency: singleton run: scan(ScanTime(clock.now)) retry: next_schedule }",None);
    assert_eq!(
        pins(&reviewed)["target"]["result"]["status"],
        "generation_rejected"
    );
    assert_eq!(
        pins(&reviewed)["target"]["result"]["diagnostic"]["code"],
        "JADPO_TARGET_JOB_NOT_IMPLEMENTED"
    );
    assert!(pins(&reviewed)["target"]["files"]
        .as_object()
        .unwrap()
        .is_empty());
    assert!(pins(&reviewed)["metadata_and_audit"]["files"]
        .get("audit/jobs.json")
        .is_some());
    assert!(reviewed["canonical"]["after"]["facts"]
        .get("artifact_pin:target/app.ts")
        .is_none());
}

#[test]
fn changed_bytes_have_individual_pin_decisions_without_fake_effect_scenarios() {
    let changed = SOURCE.replace("ok: true", "ok: false");
    let reviewed = subject(&changed, Some(SOURCE));
    assert_ne!(
        reviewed["canonical"]["before"]["facts"]["artifact_pin:target/app.ts"],
        reviewed["canonical"]["after"]["facts"]["artifact_pin:target/app.ts"]
    );
    assert_eq!(
        reviewed["canonical"]["before"]["facts"]["artifact_pin:inventory/routes.json"],
        reviewed["canonical"]["after"]["facts"]["artifact_pin:inventory/routes.json"]
    );
    assert!(reviewed["canonical"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["subject"] == "artifact_pin:target/app.ts" && row["change"] == "changed"));
    assert_eq!(reviewed["canonical"]["scenarios"], json!([]));
    assert_ne!(
        reviewed["canonical"]["before"]["state_digest"],
        reviewed["canonical"]["after"]["state_digest"]
    );
}

#[test]
fn byte_pins_rebuild_identically_across_roots_and_no_change_has_no_decisions() {
    let first = subject(SOURCE, None);
    let project = project("/elsewhere", SOURCE);
    let second: Value = serde_json::from_str(
        &derive_approval_subject(
            Path::new("/elsewhere/app.jadpo"),
            &project,
            None,
            None,
            None,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(pins(&first), pins(&second));
    let unchanged = subject(SOURCE, Some(SOURCE));
    assert_eq!(unchanged["canonical"]["decisions"], json!([]));
    assert_eq!(unchanged["canonical"]["scenarios"], json!([]));
}

#[test]
fn service_audit_bytes_do_not_depend_on_checkout_location() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let text = std::fs::read_to_string(
        root.join("tests/compile/pass/170_checked_service_operation.jadpo"),
    )
    .unwrap();
    // Keep imported bytes/name/pin and authored source identical. Both source
    // directories really exist, so ordinary pinned-import checking still runs.
    let temporary = std::env::temp_dir().join(format!(
        "jadpo-service-output-pins-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut subjects = Vec::new();
    let mut projects = Vec::new();
    for name in ["one", "two"] {
        let directory = temporary.join(name);
        std::fs::create_dir_all(directory.join("tests/assurance")).unwrap();
        std::fs::copy(
            root.join("tests/assurance/service-reference-mail-v0.1.json"),
            directory.join("tests/assurance/service-reference-mail-v0.1.json"),
        )
        .unwrap();
        let input = directory.join("app.jadpo");
        let project = analyze_sources(vec![SourceFile::new(input.clone(), text.clone())]).unwrap();
        assert!(
            !project.typing.clock_reads.is_empty(),
            "full pinned fixture must retain checked clock reads"
        );
        let raw = derive_approval_subject(&input, &project, None, None, None).unwrap();
        subjects.push(serde_json::from_str::<Value>(&raw).unwrap());
        projects.push((input, project));
    }
    let first = &subjects[0];
    let second = &subjects[1];
    assert_eq!(
        pins(&first)["metadata_and_audit"]["files"]["audit/services.json"],
        pins(&second)["metadata_and_audit"]["files"]["audit/services.json"]
    );
    assert_eq!(pins(&first), pins(&second));
    let unchanged: Value = serde_json::from_str(
        &derive_approval_subject(
            &projects[1].0,
            &projects[1].1,
            Some((&projects[0].0, &projects[0].1)),
            None,
            None,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(unchanged["canonical"]["decisions"], json!([]));
    assert_eq!(unchanged["canonical"]["scenarios"], json!([]));
    std::fs::remove_dir_all(temporary).unwrap();
}

#[test]
fn actual_clock_and_lifecycle_outputs_match_relative_and_absolute_invocations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    // These are real existing roots/imports, not a rewritten unchecked graph.
    let current = std::env::current_dir().unwrap();
    let shared = root
        .components()
        .zip(current.components())
        .take_while(|(a, b)| a == b)
        .count();
    let mut relative_root = std::path::PathBuf::new();
    for _ in shared..current.components().count() {
        relative_root.push("..");
    }
    for component in root.components().skip(shared) {
        relative_root.push(component.as_os_str());
    }
    for fixture in [
        "tests/compile/pass/170_checked_service_operation.jadpo",
        "examples/golden-todo-migration",
    ] {
        let relative = relative_root.join(fixture);
        let absolute = root.join(fixture);
        assert_ne!(
            relative, absolute,
            "exercise actual relative/absolute source paths"
        );
        let before = jadpo_core::analyze_project(&absolute).unwrap();
        let after = jadpo_core::analyze_project(&relative).unwrap();
        let before_outputs = derive_artifacts(&absolute, &before);
        let after_outputs = derive_artifacts(&relative, &after);
        assert_eq!(
            before_outputs
                .iter()
                .find(|p| p.relative_path == "app.meta.json")
                .unwrap()
                .contents,
            after_outputs
                .iter()
                .find(|p| p.relative_path == "app.meta.json")
                .unwrap()
                .contents
        );
        let before_target = derive_target(&absolute, &before).unwrap();
        let after_target = derive_target(&relative, &after).unwrap();
        assert_eq!(
            before_target, after_target,
            "actual target-owned bytes including lifecycle audit"
        );
        let compared: Value = serde_json::from_str(
            &derive_approval_subject(&relative, &after, Some((&absolute, &before)), None, None)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(compared["canonical"]["decisions"], json!([]));
        assert_eq!(compared["canonical"]["scenarios"], json!([]));
        assert!(compared["canonical"]["after"]["source_digest"].is_string());
        assert_eq!(
            compared["canonical"]["before"]["source_digest"],
            compared["canonical"]["after"]["source_digest"]
        );
    }
}

#[test]
fn fresh_export_rejects_rehashed_output_pin_omission_or_replacement() {
    let expected = subject(SOURCE, None);
    let raw = serde_json::to_string(&expected).unwrap();
    for remove in [true, false] {
        let mut altered = expected.clone();
        if remove {
            altered["canonical"]["after"]["provenance"]["generated_artifacts"]["target"]["files"]
                .as_object_mut()
                .unwrap()
                .remove("target/app.ts");
        } else {
            altered["canonical"]["after"]["provenance"]["generated_artifacts"]["target"]["files"]
                ["target/app.ts"]["byte_digest"] =
                json!("sha256:0000000000000000000000000000000000000000000000000000000000000000");
        }
        altered["subject_digest"] = json!(format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&altered["canonical"]).unwrap())
        ));
        assert!(jadpo_core::validate_approval_export(
            &raw,
            &serde_json::to_string(&altered).unwrap()
        )
        .is_err());
    }
}
