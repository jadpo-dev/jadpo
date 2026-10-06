use jadpo_core::{analyze_sources, derive_approval_subject, validate_approval_export};
use jadpo_syntax::SourceFile;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

const SOURCE: &str = include_str!(
    "../../../../tests/validation/evidence/rm601-persistence-source-links/fixture.jadpo"
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
#[test]
fn create_initializers_link_outer_and_shadowed_immutable_declarations() {
    let s = subject(SOURCE, None);
    let aliases = names(&s, "Note.add", "alias");
    assert_eq!(aliases.len(), 2);
    let declarations = aliases
        .iter()
        .map(|r| r["contract"]["source_binding_use"]["declaration"]["address"].clone())
        .collect::<Vec<_>>();
    assert!(declarations.contains(&json!(["body", "statement", "0", "binding"])));
    assert!(declarations.contains(&json!([
        "body",
        "statement",
        "1",
        "if",
        "then",
        "statement",
        "0",
        "binding"
    ])));
    for r in aliases {
        let link = &r["contract"]["source_binding_use"];
        assert_eq!(link["status"], "linked_immutable_source_use");
        assert_eq!(link["value_origin"], "not_resolved");
        assert_eq!(link["alias_value_expansion"], "not_performed");
        assert_eq!(link["branch_execution"], "not_proved");
    }
}
#[test]
fn query_field_and_same_named_rhs_are_separate_checked_subjects() {
    let s = subject(SOURCE, None);
    let ids = names(&s, "Note.lookup", "id");
    assert_eq!(ids.len(), 2);
    assert!(ids
        .iter()
        .any(|r| r["contract"]["source_binding_use"]["declaration"]["kind"] == "parameter"));
    assert!(ids
        .iter()
        .any(|r| r["contract"]["source_binding_use"]["declaration"]["kind"] == "immutable_local"));
    let query = contracts(&s, "Note.lookup")
        .into_iter()
        .find(|r| r["kind"] == "query_fields")
        .unwrap();
    assert_eq!(
        query["contract"]["predicate"]["field"]["contract"]["field"],
        "id"
    );
    assert_eq!(
        query["contract"]["predicate"]["value"]["source_binding_use"]["declaration"]["address"],
        json!(["body", "statement", "0", "binding"])
    );
    assert_eq!(query["contract"]["execution"], "not_proved");
    for r in names(&s, "Note.lookup_optional", "id") {
        assert_eq!(
            r["contract"]["source_binding_use"]["declaration"]["address"],
            json!(["parameter", "0"])
        );
        assert_eq!(r["contract"]["data_origin"]["status"], "not_resolved");
    }
}
#[test]
fn mutable_create_and_query_values_keep_unresolved_value_links() {
    let s = subject(SOURCE, None);
    for owner in ["Note.add_mutable", "Note.lookup_mutable"] {
        let aliases = names(&s, owner, "alias");
        assert!(!aliases.is_empty());
        for r in aliases {
            assert_eq!(
                r["contract"]["source_binding_use"]["reason"],
                "mutable_binding_value_not_linked"
            );
        }
    }
}
#[test]
fn call_arguments_link_without_call_result_or_database_origin_claims() {
    let s = subject(SOURCE, None);
    assert!(!names(&s, "Note.add_from_call", "title").is_empty());
    for r in names(&s, "Note.add_from_call", "title") {
        assert_eq!(
            r["contract"]["source_binding_use"]["declaration"]["address"],
            json!(["parameter", "0"])
        );
    }
    let call = contracts(&s, "Note.add_from_call")
        .into_iter()
        .find(|r| r["kind"] == "call_boundary")
        .unwrap();
    assert_eq!(call["contract"]["result_origin"], "not_resolved");
}
#[test]
fn exact_initializer_and_query_rhs_changes_affect_source_decisions_only() {
    let changed = SOURCE.replacen("title: alias", "title: other", 1).replacen(
        "where: id == id missing",
        "where: id == other missing",
        1,
    );
    let s = subject(&changed, Some(SOURCE));
    assert!(s["canonical"]["before"]["semantic_graph_digest"].is_string());
    assert_eq!(
        s["canonical"]["before"]["semantic_graph_digest"],
        s["canonical"]["after"]["semantic_graph_digest"]
    );
    for owner in ["Note.add", "Note.lookup"] {
        assert!(s["canonical"]["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |d| d["subject"] == format!("operation_field_contracts:{owner}")
                    && d["change"] == "changed"
            ));
    }
}
#[test]
fn coherently_rehashed_all_copy_operand_link_forgery_and_omission_refuse() {
    let s = subject(SOURCE, None);
    let raw = serde_json::to_string(&s).unwrap();
    assert!(validate_approval_export(&raw, &raw).is_ok());
    fn mutate(v: &mut Value, omit: bool) -> usize {
        match v {
            Value::Object(o) => {
                let mut n = 0;
                if let Some(link) = o.get("source_binding_use") {
                    if ["Note.add", "Note.lookup"]
                        .iter()
                        .any(|name| link["declaration"]["owner"]["name"] == *name)
                    {
                        if omit {
                            o.remove("source_binding_use");
                        } else {
                            o["source_binding_use"]["declaration"]["address"] =
                                json!(["parameter", "99"]);
                        }
                        n += 1;
                    }
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
fn many_query_predicate_values_stay_explicitly_unsupported() {
    let s = subject(SOURCE, None);
    let rows = names(&s, "Note.many_titles", "title");
    assert!(!rows.is_empty());
    for row in rows {
        assert_eq!(
            row["contract"]["source_binding_use"]["reason"],
            "expression_scope_not_analyzed"
        );
    }
}
