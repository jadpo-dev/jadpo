//! Finite conditional source inference, not live execution or global feasibility.
use jadpo_core::{analyze_sources, derive_approval_subject, validate_approval_export};
use jadpo_syntax::SourceFile;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
const BASE: &str = include_str!("../../../../examples/first-party-authentication/app.jadpo");
const ROUTE: &str = "route GET /witness { output: Text action: { return \"literal witness\" } }";
fn public(s: &str) -> String {
    s.replace(ROUTE, &ROUTE.replace("{ output:", "{ auth: none output:"))
}
fn subject(before: Vec<(&str, String)>, after: Vec<(&str, String)>) -> Value {
    let analyze = |root: &str, files: Vec<(&str, String)>| {
        analyze_sources(
            files
                .into_iter()
                .map(|(n, s)| SourceFile::new(Path::new(root).join(n), s))
                .collect(),
        )
        .unwrap()
    };
    let b = analyze("/before", before);
    let a = analyze("/after", after);
    serde_json::from_str(
        &derive_approval_subject(
            Path::new("/after/app.jadpo"),
            &a,
            Some((Path::new("/before/app.jadpo"), &b)),
            None,
            None,
        )
        .unwrap(),
    )
    .unwrap()
}
fn single(text: String) -> Vec<(&'static str, String)> {
    vec![("app.jadpo", text)]
}
fn witness(s: &Value) -> &Value {
    &s["canonical"]["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["route"] == "GET /witness")
        .unwrap()["counterexample"]["conditional_witness"]
}
#[test]
fn unchanged_unshadowed_checked_pair_has_only_conditional_individual_feasibility() {
    let b = format!("{BASE}\n{ROUTE}");
    let s = subject(single(b.clone()), single(public(&b)));
    let w = witness(&s);
    assert_eq!(w["status"], "conditional_feasible_under_assumptions");
    assert_eq!(w["execution"], "not_proved");
    assert_eq!(w["release_authority"], false);
    assert_eq!(
        w["request"]["credentials"],
        "all_declared_credential_slots_absent"
    );
    assert_eq!(
        w["request"]["credential_slots"].as_array().unwrap().len(),
        2
    );
    assert!(w["request"]["credential_slots"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["absence"] == "entire_header_absent"));
    assert_eq!(w["conditional_outcomes"]["before"]["status"], 401);
    assert_eq!(w["conditional_outcomes"]["after"]["status"], 200);
    assert_eq!(
        w["dispatch"]["status"],
        "selected_by_closed_first_match_rule"
    );
    for row in s["canonical"]["scenarios"].as_array().unwrap() {
        assert_eq!(row["feasibility"], "not_analyzed");
        assert_eq!(row["execution"], "not_proved");
    }
}
#[test]
fn earlier_wildcard_and_same_path_block_even_with_nonmatching_nominal_parameter() {
    for ty in ["Text", "Int"] {
        let shadow=format!("route GET /{{slot}} {{ auth: none path: {{ slot: {ty} }} output: Text action: {{ return \"shadow\" }} }}");
        let b = format!("{BASE}\n{shadow}\n{ROUTE}");
        let s = subject(single(b.clone()), single(public(&b)));
        assert_eq!(witness(&s)["status"], "not_generated");
        assert_eq!(
            s["canonical"]["scenarios"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["route"] == "GET /witness")
                .unwrap()["counterexample"]["status"],
            "source_derived_anonymous_admission_candidate"
        );
    }
}
#[test]
fn disjoint_templates_other_methods_and_later_overlap_preserve_selected_order() {
    for (prior,later) in [
  ("route GET /other { auth: none output: Text action: { return \"other\" } }", ""),
  ("route GET /other/{slot} { auth: none path: { slot: Text } output: Text action: { return \"other\" } }", ""),
  ("route POST /{slot} { auth: none path: { slot: Text } output: Text action: { return \"other\" } }", ""),
  ("", "route GET /{slot} { auth: none path: { slot: Text } output: Text action: { return \"later\" } }"),
 ]{
  let b=format!("{BASE}\n{prior}\n{ROUTE}\n{later}");let s=subject(single(b.clone()),single(public(&b)));assert_eq!(witness(&s)["status"],"conditional_feasible_under_assumptions");
 }
}
#[test]
fn cross_file_shadow_and_only_one_side_order_or_context_changes_refuse() {
    let shadow="route GET /{slot} { auth: none path: { slot: Text } output: Text action: { return \"shadow\" } }";
    let app = format!("{BASE}\n{ROUTE}");
    let s = subject(
        vec![
            ("a-shadow.jadpo", shadow.into()),
            ("z-app.jadpo", app.clone()),
        ],
        vec![
            ("a-shadow.jadpo", shadow.into()),
            ("z-app.jadpo", public(&app)),
        ],
    );
    assert_eq!(witness(&s)["status"], "not_generated");
    let s = subject(
        vec![
            ("a-app.jadpo", app.clone()),
            ("z-shadow.jadpo", shadow.into()),
        ],
        vec![
            ("a-app.jadpo", public(&app)),
            ("z-shadow.jadpo", shadow.into()),
        ],
    );
    assert_eq!(
        witness(&s)["status"],
        "conditional_feasible_under_assumptions"
    );
    let changed=format!("{BASE}\n{ROUTE}\nroute GET /other {{ auth: none output: Text action: {{ return \"extra\" }} }}");
    let s = subject(single(app.clone()), single(public(&changed)));
    assert_eq!(witness(&s)["status"], "not_generated");
    let s = subject(
        vec![
            ("a-app.jadpo", app.clone()),
            ("z-shadow.jadpo", shadow.into()),
        ],
        vec![
            ("a-shadow.jadpo", shadow.into()),
            ("z-app.jadpo", public(&app)),
        ],
    );
    assert_eq!(witness(&s)["status"], "not_generated");
}
#[test]
fn rehashed_conditional_dispatch_credential_assumption_and_execution_forgery_refuses() {
    let b = format!("{BASE}\n{ROUTE}");
    let s = subject(single(b.clone()), single(public(&b)));
    let expected = serde_json::to_string(&s).unwrap();
    assert!(validate_approval_export(&expected, &expected).is_ok());
    for key in [
        "dispatch",
        "request",
        "assumptions",
        "conditional_outcomes",
        "execution",
    ] {
        let mut f = s.clone();
        let w = &mut f["canonical"]["scenarios"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["route"] == "GET /witness")
            .unwrap()["counterexample"]["conditional_witness"];
        if key == "execution" {
            w[key] = json!("proved");
        } else {
            w.as_object_mut().unwrap().remove(key);
        }
        f["subject_digest"] = json!(format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&f["canonical"]).unwrap())
        ));
        assert!(validate_approval_export(&expected, &serde_json::to_string(&f).unwrap()).is_err());
    }
}

#[test]
fn unsupported_prior_templates_and_paired_nonroute_source_changes_stay_unknown() {
    let b=format!("{BASE}\nroute GET /other.dot {{ auth: none output: Text action: {{ return \"other\" }} }}\n{ROUTE}");
    assert_eq!(
        witness(&subject(single(b.clone()), single(public(&b))))["status"],
        "not_generated"
    );
    let b = format!("{BASE}\n{ROUTE}");
    let a = public(&b).replace("__Host-jadpo_session", "__Host-other_session");
    assert_ne!(a, public(&b));
    assert_eq!(
        witness(&subject(single(b), single(a)))["status"],
        "not_generated"
    );
}
#[test]
fn duplicate_exact_route_identity_never_establishes_dispatch() {
    let b = format!("{BASE}\n{ROUTE}\n{ROUTE}");
    let analyze = |s: &str| {
        analyze_sources(vec![SourceFile::new("/dupe/app.jadpo".into(), s.into())]).unwrap()
    };
    let before = analyze(&b);
    let after = analyze(&public(&b));
    match derive_approval_subject(
        Path::new("/dupe/app.jadpo"),
        &after,
        Some((Path::new("/dupe/app.jadpo"), &before)),
        None,
        None,
    ) {
        Ok(raw) => {
            let s: Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(witness(&s)["status"], "not_generated");
        }
        Err(_) => assert!(before
            .syntax
            .diagnostics()
            .chain(before.semantics.diagnostics.iter())
            .chain(before.typing.diagnostics.iter())
            .any(|d| d.severity == jadpo_diagnostics::Severity::Error)),
    }
}

#[test]
fn reserved_and_noncanonical_fixed_path_candidates_keep_the_finite_rule_unknown() {
    for path in ["/health", "/witness/", "/witness//nested", "/"] {
        let before = format!("{BASE}\n{}", ROUTE.replace("/witness", path));
        let after = before.replace(
            "{ output: Text action: { return \"literal witness\"",
            "{ auth: none output: Text action: { return \"literal witness\"",
        );
        let s = subject(single(before), single(after));
        let key = format!("GET {path}");
        let row = s["canonical"]["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["route"] == key)
            .unwrap();
        assert_ne!(
            row["counterexample"]["conditional_witness"]["status"],
            "conditional_feasible_under_assumptions"
        );
    }
}
#[test]
fn coherent_all_copy_context_digest_omission_and_foreign_dispatch_refuse() {
    let b = format!("{BASE}\n{ROUTE}");
    let s = subject(single(b.clone()), single(public(&b)));
    let expected = serde_json::to_string(&s).unwrap();
    fn mutate(v: &mut Value, key: &str) -> usize {
        match v {
            Value::Object(o) => {
                let n = usize::from(o.contains_key(key));
                if n > 0 {
                    if key == "context_digest" {
                        o.remove(key);
                    } else {
                        o.insert(key.into(), json!(999));
                    }
                }
                n + o.values_mut().map(|v| mutate(v, key)).sum::<usize>()
            }
            Value::Array(a) => a.iter_mut().map(|v| mutate(v, key)).sum(),
            _ => 0,
        }
    }
    fn rehash_snapshot(v: &mut Value) {
        if let Some(o) = v.as_object_mut() {
            o.remove("state_digest");
            v["state_digest"] = json!(format!(
                "sha256:{:x}",
                Sha256::digest(serde_json::to_vec(v).unwrap())
            ));
        }
    }
    for key in ["context_digest", "declaration"] {
        let mut f = s.clone();
        assert!(mutate(&mut f["canonical"], key) > 2);
        rehash_snapshot(&mut f["canonical"]["before"]);
        rehash_snapshot(&mut f["canonical"]["after"]);
        f["subject_digest"] = json!(format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&f["canonical"]).unwrap())
        ));
        assert!(validate_approval_export(&expected, &serde_json::to_string(&f).unwrap()).is_err());
    }
}
