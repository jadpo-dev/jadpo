use jadpo_core::{
    analyze_project, analyze_sources, derive_approval_subject, validate_approval_export,
};
use jadpo_syntax::SourceFile;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
const SOURCE: &str = include_str!(
    "../../../../tests/validation/evidence/rm601-route-transport-source-links/fixture.jadpo"
);
const INLINE: &str = "POST /transport/{key}";
fn subject(source: &str, before: Option<&str>) -> Value {
    let make = |root: &str, text: &str| {
        analyze_sources(vec![SourceFile::new(
            Path::new(root).join("app.jadpo"),
            text.into(),
        )])
        .unwrap()
    };
    let after = make("/project", source);
    let baseline = before.map(|s| make("/baseline", s));
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
            "{e}: syntax={:?} types={:?}",
            after.syntax.diagnostics().collect::<Vec<_>>(),
            after.typing.diagnostics
        )
    });
    serde_json::from_str(&raw).unwrap()
}
fn operations(s: &Value) -> &Value {
    &s["canonical"]["after"]["impact"]["field_source_contracts"]["operations"]
}
fn rows<'a>(s: &'a Value, owner: &str) -> Vec<&'a Value> {
    operations(s)[owner]["contracts"]
        .as_object()
        .unwrap()
        .values()
        .collect()
}
fn named<'a>(s: &'a Value, owner: &str, tokens: Value) -> Vec<&'a Value> {
    rows(s, owner)
        .into_iter()
        .filter(|r| r["kind"] == "expression" && r["contract"]["tokens"] == tokens)
        .collect()
}

#[test]
fn inline_transport_uses_link_only_exact_authored_route_declarations() {
    let s = subject(SOURCE, None);
    for (root, field) in [
        ("input", "body"),
        ("query", "search"),
        ("path", "key"),
        ("headers", "trace"),
    ] {
        let uses = named(&s, INLINE, json!([root, ".", field]));
        assert!(!uses.is_empty());
        for row in uses {
            let link = &row["contract"]["source_binding_use"];
            assert_eq!(link["status"], "linked_route_transport_source_use");
            assert_eq!(link["declaration"]["address"], json!(["route", root]));
            assert_eq!(link["declaration"]["owner"]["name"], INLINE);
            assert_eq!(link["access_path"], json!([field]));
            assert_eq!(link["value_origin"], "not_resolved");
            assert_eq!(link["caller_identity"], "not_established");
            assert_eq!(link["branch_execution"], "not_proved");
            assert_eq!(link["alias_value_expansion"], "not_performed");
            assert_eq!(row["contract"]["data_origin"]["status"], "not_resolved");
        }
    }
}

#[test]
fn path_and_header_members_bind_authored_field_addresses_not_generated_shape_offsets() {
    let s = subject(SOURCE, None);
    for (root, field) in [("path", "key"), ("headers", "trace")] {
        let row = named(&s, INLINE, json!([root, ".", field]))[0];
        let member = &row["contract"]["source_binding_use"]["member_declaration"];
        assert_eq!(member["kind"], "authored_route_transport_field");
        assert_eq!(member["address"], json!(["route", root, "field", field]));
        assert_eq!(member["owner"]["name"], INLINE);
    }
    for (root, field) in [("input", "body"), ("query", "search")] {
        assert!(
            named(&s, INLINE, json!([root, ".", field]))[0]["contract"]["source_binding_use"]
                ["member_declaration"]
                .is_null()
        );
    }
    let surface = rows(&s, INLINE)
        .into_iter()
        .find(|r| r["kind"] == "operation_surface")
        .unwrap();
    assert_eq!(surface["contract"]["path_fields"][0]["field"], "key");
    assert_eq!(surface["contract"]["headers"][0]["name"], "trace");
}

#[test]
fn checked_run_argument_uses_have_same_route_contract_scope_without_value_expansion() {
    let s = subject(SOURCE, None);
    let uses = named(&s, "POST /run/{key}", json!(["input", ".", "body"]));
    assert_eq!(uses.len(), 1);
    assert_eq!(
        uses[0]["contract"]["source_binding_use"]["declaration"]["address"],
        json!(["route", "input"])
    );
    assert_eq!(
        uses[0]["contract"]["source_binding_use"]["caller_identity"],
        "not_established"
    );
    let call = rows(&s, "POST /run/{key}")
        .into_iter()
        .find(|r| r["kind"] == "call_boundary" && r["contract"]["callee"] == "echo")
        .unwrap();
    assert_eq!(call["contract"]["result_origin"], "not_resolved");
}

#[test]
fn local_shadow_takes_precedence_then_outer_transport_scope_is_restored() {
    let s = subject(SOURCE, None);
    let shadow = named(&s, INLINE, json!(["input", ".", "search"]));
    assert_eq!(shadow.len(), 1);
    let link = &shadow[0]["contract"]["source_binding_use"];
    assert_eq!(link["declaration"]["kind"], "immutable_local");
    assert_eq!(
        link["declaration"]["address"],
        json!([
            "body",
            "statement",
            "1",
            "if",
            "then",
            "statement",
            "0",
            "binding"
        ])
    );
    assert_eq!(link["branch_execution"], "not_proved");
    let alias = named(&s, INLINE, json!(["alias", ".", "body"]));
    assert_eq!(alias.len(), 1);
    assert_eq!(
        alias[0]["contract"]["source_binding_use"]["declaration"]["kind"],
        "immutable_local"
    );
    assert_eq!(
        alias[0]["contract"]["source_binding_use"]["value_origin"],
        "not_resolved"
    );
    let restored = named(&s, INLINE, json!(["input", ".", "body"]));
    assert!(!restored.is_empty());
    assert!(restored.iter().all(
        |r| r["contract"]["source_binding_use"]["declaration"]["kind"]
            == "authored_route_transport"
    ));
}

#[test]
fn missing_transport_and_implicit_runtime_principal_clock_are_not_source_links() {
    let s = subject(SOURCE, None);
    let surface = rows(&s, "GET /empty")
        .into_iter()
        .find(|r| r["kind"] == "operation_surface")
        .unwrap();
    assert!(surface["contract"]["input"].is_null());
    assert!(surface["contract"]["query"].is_null());
    assert_eq!(surface["contract"]["path_fields"], json!([]));
    assert_eq!(surface["contract"]["headers"], json!([]));
    assert!(rows(&s, "GET /empty")
        .iter()
        .all(|r| r["contract"].get("source_binding_use").is_none()));
    let clock = named(&s, INLINE, json!(["clock", ".", "now"]));
    assert_eq!(clock.len(), 1);
    assert_eq!(
        clock[0]["contract"]["source_binding_use"]["status"],
        "unresolved"
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let path = root.join("examples/golden-todo-migration");
    let analyzed = analyze_project(&path).unwrap();
    let golden: Value =
        serde_json::from_str(&derive_approval_subject(&path, &analyzed, None, None, None).unwrap())
            .unwrap();
    let principal = rows(&golden, "PATCH /todos/{todo_id}")
        .into_iter()
        .filter(|r| {
            r["kind"] == "expression"
                && r["contract"]["tokens"] == json!(["current_principal", ".", "user"])
        })
        .collect::<Vec<_>>();
    assert_eq!(principal.len(), 1);
    assert_eq!(
        principal[0]["contract"]["source_binding_use"]["status"],
        "unresolved"
    );
}

#[test]
fn renamed_authored_header_changes_exact_source_decisions_without_graph_reachability_proof() {
    let changed = SOURCE
        .replace("trace:", "audit:")
        .replace("headers.trace", "headers.audit");
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
        .any(
            |d| d["subject"] == format!("operation_field_contracts:{INLINE}")
                && d["change"] == "changed"
        ));
    let audit = named(&s, INLINE, json!(["headers", ".", "audit"]));
    assert!(!audit.is_empty());
    assert_eq!(
        audit[0]["contract"]["source_binding_use"]["member_declaration"]["address"],
        json!(["route", "headers", "field", "audit"])
    );
}

#[test]
fn coherent_all_copy_header_link_forgery_and_omission_refuse_fresh_export() {
    let expected = subject(SOURCE, None);
    let raw = serde_json::to_string(&expected).unwrap();
    assert!(validate_approval_export(&raw, &raw).is_ok());
    fn alter(v: &mut Value, omit: bool) -> usize {
        match v {
            Value::Object(o) => {
                let mut n = 0;
                if let Some(link) = o.get_mut("source_binding_use") {
                    if link["declaration"]["kind"] == "authored_route_transport" {
                        if omit {
                            o.remove("source_binding_use");
                        } else {
                            link["declaration"]["owner"]["name"] = json!("GET /empty");
                            if !link["member_declaration"].is_null() {
                                link["member_declaration"]["address"] =
                                    json!(["route", "headers", "field", "foreign"]);
                            }
                        }
                        n += 1;
                    }
                }
                n + o.values_mut().map(|v| alter(v, omit)).sum::<usize>()
            }
            Value::Array(a) => a.iter_mut().map(|v| alter(v, omit)).sum(),
            _ => 0,
        }
    }
    for omit in [false, true] {
        let mut forged = expected.clone();
        assert!(alter(&mut forged["canonical"], omit) > 1);
        forged["subject_digest"] = json!(format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&forged["canonical"]).unwrap())
        ));
        assert!(validate_approval_export(&raw, &serde_json::to_string(&forged).unwrap()).is_err());
    }
}

#[test]
fn mutable_route_alias_and_pattern_shadow_remain_explicitly_unresolved() {
    let changed = SOURCE.replace("var alias = input", "var mut alias = input");
    let s = subject(&changed, None);
    let alias = named(&s, INLINE, json!(["alias", ".", "body"]));
    assert_eq!(alias.len(), 1);
    assert_eq!(
        alias[0]["contract"]["source_binding_use"]["reason"],
        "mutable_binding_value_not_linked"
    );
    let pattern=SOURCE.replace("input Body { body: Choice }","input Body { body: Choice maybe: Choice? }").replace("var now = clock.now","match input.maybe { none => {} some(input) => { assert input == input } }\n        var now = clock.now");
    let s = subject(&pattern, None);
    let arm_uses = named(&s, INLINE, json!(["input"]))
        .into_iter()
        .filter(|r| {
            r["conditions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["kind"] == "match_arm")
        })
        .collect::<Vec<_>>();
    assert_eq!(arm_uses.len(), 2);
    assert!(arm_uses
        .iter()
        .all(|r| r["contract"]["source_binding_use"]["reason"] == "expression_scope_not_analyzed"));
}
