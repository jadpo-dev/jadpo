use jadpo_core::{analyze_sources, derive_approval_subject, validate_approval_export};
use jadpo_syntax::SourceFile;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

const SOURCE: &str = include_str!(
    "../../../../tests/validation/evidence/rm601-static-lexical-source-links/fixture.jadpo"
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
fn contracts<'a>(s: &'a Value, owner: &str) -> Vec<&'a Value> {
    s["canonical"]["after"]["impact"]["field_source_contracts"]["operations"][owner]["contracts"]
        .as_object()
        .unwrap()
        .values()
        .collect()
}
fn names<'a>(s: &'a Value, owner: &str, name: &str) -> Vec<&'a Value> {
    contracts(s, owner)
        .into_iter()
        .filter(|r| r["kind"] == "expression" && r["contract"]["tokens"] == json!([name]))
        .collect()
}
fn returned<'a>(s: &'a Value, owner: &str) -> &'a Value {
    &contracts(s, owner)
        .into_iter()
        .find(|r| r["kind"] == "return_boundary")
        .unwrap()["contract"]["value"]["source_binding_use"]
}

#[test]
fn immutable_parameter_and_alias_uses_link_exact_declarations_without_origins() {
    let s = subject(SOURCE, None);
    let input = names(&s, "linked", "input");
    assert!(input.len() >= 2);
    for row in input {
        let link = &row["contract"]["source_binding_use"];
        assert_eq!(link["declaration"]["kind"], "parameter");
        assert_eq!(link["declaration"]["address"], json!(["parameter", "0"]));
        assert_eq!(link["value_origin"], "not_resolved");
        assert_eq!(link["alias_value_expansion"], "not_performed");
    }
    let alias = returned(&s, "linked");
    assert_eq!(
        alias["declaration"]["address"],
        json!(["body", "statement", "0", "binding"])
    );
    let binding = contracts(&s, "linked")
        .into_iter()
        .find(|r| r["kind"] == "binding" && r["address"] == alias["declaration"]["address"])
        .unwrap();
    assert_eq!(alias["declaration"]["contract_id"], binding["id"]);
    assert_eq!(
        alias["identity"],
        "owner_and_build_local_structural_address_not_insertion_stable"
    );
    assert_eq!(binding["contract"]["alias_resolution"], "not_performed");
}

#[test]
fn nested_shadow_initializers_see_outer_scope_and_scope_is_restored() {
    let s = subject(SOURCE, None);
    let rows = names(&s, "linked", "alias");
    let initializer = rows
        .iter()
        .find(|r| {
            r["address"]
                == json!([
                    "body",
                    "statement",
                    "1",
                    "if",
                    "then",
                    "statement",
                    "0",
                    "binding",
                    "value"
                ])
        })
        .unwrap();
    assert_eq!(
        initializer["contract"]["source_binding_use"]["declaration"]["address"],
        json!(["body", "statement", "0", "binding"])
    );
    let inner = rows
        .iter()
        .find(|r| {
            r["contract"]["source_binding_use"]["declaration"]["address"]
                == json!([
                    "body",
                    "statement",
                    "1",
                    "if",
                    "then",
                    "statement",
                    "0",
                    "binding"
                ])
        })
        .unwrap();
    assert_eq!(
        inner["contract"]["source_binding_use"]["branch_execution"],
        "not_proved"
    );
    assert_eq!(inner["condition_status"], "symbolic_not_proved");
    assert_eq!(
        returned(&s, "linked")["declaration"]["address"],
        json!(["body", "statement", "0", "binding"])
    );
}

#[test]
fn mutable_values_remain_unresolved_on_both_sides_of_reassignment() {
    let s = subject(SOURCE, None);
    let uses = names(&s, "mutable", "alias");
    assert_eq!(uses.len(), 2);
    for row in uses {
        let link = &row["contract"]["source_binding_use"];
        assert_eq!(link["status"], "unresolved");
        assert_eq!(link["reason"], "mutable_binding_value_not_linked");
        assert_eq!(link["candidate_declaration"]["kind"], "mutable_local");
    }
}

#[test]
fn call_return_alias_links_local_declaration_without_resolving_call_result() {
    let s = subject(SOURCE, None);
    let alias = returned(&s, "from_call");
    assert_eq!(alias["declaration"]["kind"], "immutable_local");
    assert_eq!(alias["value_origin"], "not_resolved");
    assert_eq!(alias["alias_value_expansion"], "not_performed");
    let call = contracts(&s, "from_call")
        .into_iter()
        .find(|r| r["kind"] == "call_boundary")
        .unwrap();
    assert_eq!(call["contract"]["result_origin"], "not_resolved");
}

#[test]
fn pattern_shadowing_and_route_implicit_names_do_not_fall_through_to_parameters() {
    let s = subject(SOURCE, None);
    let pattern = names(&s, "match_value", "value");
    assert_eq!(pattern.len(), 3);
    for row in pattern
        .iter()
        .filter(|r| !r["conditions"].as_array().unwrap().is_empty())
    {
        assert_eq!(
            row["contract"]["source_binding_use"]["reason"],
            "expression_scope_not_analyzed"
        );
    }
    assert_eq!(
        returned(&s, "match_value")["declaration"]["kind"],
        "parameter"
    );
    let route_names = names(&s, "POST /response", "input");
    assert!(!route_names.is_empty());
    for row in route_names {
        assert_eq!(
            row["contract"]["source_binding_use"]["reason"],
            "expression_scope_not_analyzed"
        );
    }
}

#[test]
fn field_access_links_parameter_root_but_persistence_predicate_scope_stays_unresolved() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let source =
        std::fs::read_to_string(root.join("tests/compile/pass/134_policy_safe_projection.jadpo"))
            .unwrap();
    let s = subject(&source, None);
    let rows = names(&s, "PrivateNote.summary", "id");
    assert!(!rows.is_empty());
    for row in rows {
        assert_eq!(
            row["contract"]["source_binding_use"]["reason"],
            "expression_scope_not_analyzed"
        );
    }
    let title = contracts(&s, "PrivateNote.summary")
        .into_iter()
        .find(|r| {
            r["kind"] == "expression" && r["contract"]["tokens"] == json!(["note", ".", "title"])
        })
        .unwrap();
    assert_eq!(
        title["contract"]["source_binding_use"]["access_path"],
        json!(["title"])
    );
    assert_eq!(
        title["contract"]["source_binding_use"]["declaration"]["name"],
        "note"
    );
    assert_eq!(title["contract"]["data_origin"]["status"], "not_resolved");
}

#[test]
fn initializer_source_delta_changes_exact_contract_decisions_without_graph_or_alias_proof() {
    let changed = SOURCE.replacen("var alias = input", "var alias = other", 1);
    let s = subject(&changed, Some(SOURCE));
    assert!(s["canonical"]["before"]["semantic_graph_digest"].is_string());
    assert_eq!(
        s["canonical"]["before"]["semantic_graph_digest"],
        s["canonical"]["after"]["semantic_graph_digest"]
    );
    assert!(s["canonical"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["subject"] == "operation_field_contracts:linked" && d["change"] == "changed"));
    let binding = contracts(&s, "linked")
        .into_iter()
        .find(|r| {
            r["kind"] == "binding" && r["address"] == json!(["body", "statement", "0", "binding"])
        })
        .unwrap();
    assert_eq!(
        binding["contract"]["value"]["source_binding_use"]["declaration"]["address"],
        json!(["parameter", "1"])
    );
    assert_eq!(returned(&s, "linked")["value_origin"], "not_resolved");
}

#[test]
fn rehashed_link_forgery_and_omission_refuse_fresh_export() {
    let s = subject(SOURCE, None);
    let raw = serde_json::to_string(&s).unwrap();
    assert!(validate_approval_export(&raw, &raw).is_ok());
    for omit in [false, true] {
        let mut forged = s.clone();
        fn mutate(value: &mut Value, omit: bool) -> usize {
            match value {
                Value::Object(object) => {
                    let mut count = 0;
                    if omit {
                        count += usize::from(object.remove("source_binding_use").is_some());
                    } else if let Some(link) = object.get_mut("source_binding_use") {
                        if link["declaration"]["name"] == "alias"
                            && link["declaration"]["owner"]["name"] == "linked"
                        {
                            let declaration = link["declaration"].as_object_mut().unwrap();
                            declaration.insert("address".into(), json!(["parameter", "1"]));
                            declaration.insert("kind".into(), json!("parameter"));
                            declaration.insert("name".into(), json!("other"));
                            declaration.remove("contract_id");
                            count += 1;
                        }
                    }
                    count + object.values_mut().map(|v| mutate(v, omit)).sum::<usize>()
                }
                Value::Array(values) => values.iter_mut().map(|v| mutate(v, omit)).sum(),
                _ => 0,
            }
        }
        assert!(mutate(&mut forged["canonical"], omit) > 1);
        forged["subject_digest"] = json!(format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&forged["canonical"]).unwrap())
        ));
        assert!(validate_approval_export(&raw, &serde_json::to_string(&forged).unwrap()).is_err());
    }
}

#[test]
fn parameter_field_path_and_build_local_address_limits_are_explicit() {
    let s = subject(SOURCE, None);
    let field = returned(&s, "field");
    assert_eq!(field["declaration"]["kind"], "parameter");
    assert_eq!(field["access_path"], json!(["value"]));
    let inserted = SOURCE.replacen(
        "var alias = input",
        "var unused = other\n    var alias = input",
        1,
    );
    let changed = subject(&inserted, None);
    assert_ne!(
        returned(&s, "linked")["declaration"]["contract_id"],
        returned(&changed, "linked")["declaration"]["contract_id"]
    );
    assert_eq!(
        returned(&changed, "linked")["declaration"]["address"],
        json!(["body", "statement", "1", "binding"])
    );
}

#[test]
fn rejected_source_never_produces_checked_lexical_contracts() {
    let invalid = SOURCE.replacen("return alias", "return absent_value", 1);
    let p = analyze_sources(vec![SourceFile::new("/project/app.jadpo".into(), invalid)]).unwrap();
    assert!(
        derive_approval_subject(Path::new("/project/app.jadpo"), &p, None, None, None).is_err()
    );
}
