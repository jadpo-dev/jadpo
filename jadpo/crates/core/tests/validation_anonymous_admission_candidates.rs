use jadpo_core::{analyze_sources, derive_approval_subject, validate_approval_export};
use jadpo_syntax::SourceFile;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

const BASE: &str = include_str!("../../../../examples/first-party-authentication/app.jadpo");
const ROUTE: &str = "route GET /witness { output: Text action: { return \"literal witness\" } }";
fn source(route: &str) -> String {
    format!("{BASE}\n{route}\n")
}
fn public(route: &str) -> String {
    route.replace("{ output:", "{ auth: none output:")
}
fn subject(after: &str, before: Option<&str>) -> Value {
    let analyze = |root: &str, source: &str| {
        analyze_sources(vec![SourceFile::new(
            Path::new(root).join("app.jadpo"),
            source.into(),
        )])
        .unwrap()
    };
    let after = analyze("/project", after);
    let before = before.map(|s| analyze("/baseline", s));
    let raw = derive_approval_subject(
        Path::new("/project/app.jadpo"),
        &after,
        before
            .as_ref()
            .map(|p| (Path::new("/baseline/app.jadpo"), p)),
        None,
        None,
    )
    .unwrap_or_else(|e| {
        panic!(
            "{e} syntax={:?} typing={:?} semantic={:?} failure={:?} policy={:?}",
            after.syntax.diagnostics().collect::<Vec<_>>(),
            after.typing.diagnostics,
            after.semantics.diagnostics,
            after.failures.diagnostics,
            after.policy.diagnostics
        )
    });
    serde_json::from_str(&raw).unwrap()
}
fn counterexample<'a>(s: &'a Value, route: &str) -> &'a Value {
    &s["canonical"]["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["route"] == route)
        .unwrap()["counterexample"]
}
#[test]
fn identical_checked_literal_auth_weakening_has_conditional_source_candidate_only() {
    let before = source(ROUTE);
    let after = source(&public(ROUTE));
    let s = subject(&after, Some(&before));
    let c = counterexample(&s, "GET /witness");
    assert_eq!(c["status"], "source_derived_anonymous_admission_candidate");
    assert_eq!(
        c["facts"]["request"],
        json!({"method":"GET","path":"/witness","credentials":"absent","authored_transport_values":"none"})
    );
    assert_eq!(
        c["facts"]["checked_literal_return"]["literal_token"],
        "\"literal witness\""
    );
    assert_eq!(
        c["facts"]["assumptions"]["database"],
        "native_readiness_available_if_persistent_entities"
    );
    assert_eq!(c["facts"]["execution"], "not_proved");
    assert_eq!(c["facts"]["feasibility"], "not_analyzed");
    assert_eq!(c["facts"]["release_authority"], false);
    let scenario = s["canonical"]["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["route"] == "GET /witness")
        .unwrap();
    assert_eq!(scenario["execution"], "not_proved");
    assert_eq!(scenario["feasibility"], "not_analyzed");
    assert!(
        s["canonical"]["after"]["impact"]["anonymous_admission_candidates"]["runtime_observations"]
            .is_null()
    );
    assert_eq!(s["canonical"]["evidence"]["runtime_validated"], json!([]));
}
#[test]
fn absent_baseline_auth_strengthening_and_changed_literal_are_not_candidates() {
    let before = source(ROUTE);
    let after = source(&public(ROUTE));
    assert_eq!(
        counterexample(&subject(&after, None), "GET /witness")["status"],
        "not_generated"
    );
    assert_eq!(
        counterexample(&subject(&before, Some(&after)), "GET /witness")["status"],
        "not_generated"
    );
    let changed = after.replace("literal witness", "different literal");
    assert_eq!(
        counterexample(&subject(&changed, Some(&before)), "GET /witness")["status"],
        "not_generated"
    );
    let unchanged = subject(&after, Some(&after));
    assert!(!unchanged["canonical"]["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["route"] == "GET /witness"));
}
#[test]
fn checked_branch_binding_and_callable_hops_remain_unsupported() {
    for (extra, body, output) in [
        ("", "action: { if true { return \"literal witness\" } else { return \"literal witness\" } }", "Text"),
        ("", "action: { var answer = \"literal witness\" return answer }", "Text"),
        ("type Answer = Text {} action answer() -> Answer { policy { Access.public: [invoke] } return Answer(\"literal witness\") }", "run: answer()", "Answer")
    ] {
        let route = format!("route GET /witness {{ output: {output} {body} }}");
        let before = format!("{extra}\n{}", source(&route));
        let after = format!("{extra}\n{}", source(&public(&route)));
        assert_eq!(counterexample(&subject(&after, Some(&before)), "GET /witness")["status"], "not_generated");
    }
}
#[test]
fn nonget_nominal_transport_and_reserved_native_paths_remain_unsupported() {
    for (extra,route,key) in [("","route POST /witness { output: Text action: { return \"literal witness\" } }","POST /witness"),("type Answer = Text {}","route GET /witness { output: Answer action: { return Answer(\"literal witness\") } }","GET /witness"),("","route GET /witness { output: Text query: IdentityRequest action: { return \"literal witness\" } }","GET /witness"),("","route GET /health/ready { output: Text action: { return \"literal witness\" } }","GET /health/ready")]{
        let before=format!("{extra}\n{}",source(route));let after=format!("{extra}\n{}",source(&public(route)));
        assert_eq!(counterexample(&subject(&after,Some(&before)),key)["status"],"not_generated");
    }
}
#[test]
fn supported_target_and_generated_authentication_boundary_are_required() {
    let before = ROUTE;
    let after = public(ROUTE);
    assert_eq!(
        counterexample(&subject(&after, Some(before)), "GET /witness")["status"],
        "not_generated"
    );
    let unsupported = "output Health { ok: Bool } action health() -> Health { policy { Access.public: [invoke] } return Health { ok: true } } route GET /health/live { auth: none output: Health run: health() }";
    let before = format!("{unsupported}\n{}", source(ROUTE));
    let after = format!("{unsupported}\n{}", source(&public(ROUTE)));
    let s = subject(&after, Some(&before));
    assert_eq!(
        s["canonical"]["after"]["impact"]["deployment"]["diagnostic"]["code"],
        "JADPO_TARGET_LIVENESS_NOT_LOCAL"
    );
    assert_eq!(
        counterexample(&s, "GET /witness")["status"],
        "not_generated"
    );
}
#[test]
fn body_spelling_changes_do_not_hide_behind_unchanged_graph_topology() {
    let before = source(ROUTE);
    let after = source(&public(ROUTE).replace("literal witness", "other witness"));
    let s = subject(&after, Some(&before));
    assert_eq!(
        s["canonical"]["before"]["semantic_graph_digest"],
        s["canonical"]["after"]["semantic_graph_digest"]
    );
    assert_eq!(
        counterexample(&s, "GET /witness")["status"],
        "not_generated"
    );
    assert!(s["canonical"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |d| d["subject"] == "impact:anonymous_admission_candidates" && d["change"] == "changed"
        ));
}
#[test]
fn coherent_candidate_omission_forgery_and_execution_promotion_refuse() {
    let s = subject(&source(&public(ROUTE)), Some(&source(ROUTE)));
    let expected = serde_json::to_string(&s).unwrap();
    assert!(validate_approval_export(&expected, &expected).is_ok());
    for mutation in ["omit", "actor", "execution"] {
        let mut forged = s.clone();
        let rows = forged["canonical"]["scenarios"].as_array_mut().unwrap();
        let row = rows
            .iter_mut()
            .find(|r| r["route"] == "GET /witness")
            .unwrap();
        match mutation {
            "omit" => row["counterexample"] = json!({"status":"not_generated","facts":null}),
            "actor" => {
                row["counterexample"]["facts"]["request"]["credentials"] = json!("valid_user")
            }
            _ => row["execution"] = json!("proved"),
        }
        forged["subject_digest"] = json!(format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&forged["canonical"]).unwrap())
        ));
        assert!(
            validate_approval_export(&expected, &serde_json::to_string(&forged).unwrap()).is_err()
        );
    }
}

#[test]
fn all_copy_rehashed_literal_source_omission_and_forgery_refuse() {
    let s = subject(&source(&public(ROUTE)), Some(&source(ROUTE)));
    let expected = serde_json::to_string(&s).unwrap();
    fn mutate(v: &mut Value, omit: bool) -> usize {
        match v {
            Value::Object(o) => {
                let mut n = 0;
                if o.contains_key("literal_token") {
                    if omit {
                        o.remove("literal_token");
                    } else {
                        o.insert("literal_token".into(), json!("\"foreign literal\""));
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
        assert!(mutate(&mut forged["canonical"], omit) > 4);
        forged["subject_digest"] = json!(format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&forged["canonical"]).unwrap())
        ));
        assert!(
            validate_approval_export(&expected, &serde_json::to_string(&forged).unwrap()).is_err()
        );
    }
}

#[test]
fn authored_header_input_and_path_parsing_are_not_anonymous_literal_candidates() {
    for (route, key) in [
        ("route GET /witness { headers: { trace: Text from \"X-Trace\" } output: Text action: { return \"literal witness\" } }", "GET /witness"),
        ("route GET /witness { input: IdentityRequest output: Text action: { return \"literal witness\" } }", "GET /witness"),
        ("route GET /witness/{id} { path: { id: User.id } output: Text action: { return \"literal witness\" } }", "GET /witness/{id}"),
    ] {
        let after = route.replacen(" { ", " { auth: none ", 1);
        assert_eq!(counterexample(&subject(&source(&after), Some(&source(route))), key)["status"], "not_generated");
    }
}
