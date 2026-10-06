use jadpo_core::{analyze_project, analyze_sources, derive_artifacts, AnalyzedProject};
use jadpo_syntax::SourceFile;
use serde_json::Value;
use std::path::{Path, PathBuf};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn outputs(path: &Path, project: &AnalyzedProject) -> (Value, Value, Value) {
    let artifacts = derive_artifacts(path, project);
    let get = |path| {
        serde_json::from_str(
            &artifacts
                .iter()
                .find(|a| a.relative_path == path)
                .unwrap()
                .contents,
        )
        .unwrap()
    };
    (
        get("inventory/routes.json"),
        get("openapi/openapi.json"),
        get("audit/failures.json"),
    )
}
fn row<'a>(routes: &'a Value, route: &str) -> &'a Value {
    routes["routes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["route"] == route)
        .unwrap()
}
fn failures<'a>(routes: &'a Value, route: &str) -> &'a Vec<Value> {
    row(routes, route)["http_boundary_failures"]
        .as_array()
        .unwrap()
}
fn schema<'a>(api: &'a Value, path: &str, method: &str, status: &str) -> &'a Value {
    &api["paths"][path][method]["responses"][status]["content"]["application/json"]["schema"]
}
fn clean(source: &str) -> AnalyzedProject {
    let p = analyze_sources(vec![SourceFile::new(
        "http-boundary.jadpo".into(),
        source.into(),
    )])
    .unwrap();
    let d = p
        .syntax
        .diagnostics()
        .chain(p.semantics.diagnostics.iter())
        .chain(p.typing.diagnostics.iter())
        .chain(p.failures.diagnostics.iter())
        .chain(p.entity_model.diagnostics.iter())
        .chain(p.policy.diagnostics.iter())
        .collect::<Vec<_>>();
    assert!(d.is_empty(), "{d:#?}");
    p
}
#[test]
fn canonical_authentication_and_inactive_contracts_are_distinct_from_domain_failures() {
    let path = root().join("examples/golden-todo-migration");
    let p = analyze_project(&path).unwrap();
    let (routes, api, audit) = outputs(&path, &p);
    let fs = failures(&routes, "GET /todos");
    for code in [
        "authentication_required",
        "invalid_credentials",
        "ambiguous_credentials",
    ] {
        assert!(fs.iter().any(|f| f["code"] == code
            && f["http_status"] == 401
            && f["message"] == "Authentication failed."));
    }
    let disabled = fs
        .iter()
        .find(|f| f["declared_failure"] == "UserDisabled")
        .unwrap();
    assert_eq!(disabled["http_status"], 403);
    assert_eq!(disabled["code"], "user_disabled");
    assert_eq!(disabled["message"], "This account is not available.");
    assert_eq!(
        row(&routes, "GET /todos")["http_boundary_analysis"]["qualified"],
        true
    );
    let alternatives = schema(&api, "/todos", "get", "401")["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(alternatives.len(), 3);
    for s in alternatives {
        assert_eq!(s["additionalProperties"], false);
        assert_eq!(s["properties"]["error"]["additionalProperties"], false);
        assert_eq!(
            s["properties"]["error"]["required"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
    }
    assert!(!failures(&routes, "GET /todos")
        .iter()
        .any(|f| f["code"] == "csrf_rejected"));
    assert!(failures(&routes, "DELETE /users/{user_id}")
        .iter()
        .any(|f| f["code"] == "csrf_rejected"));
    assert!(failures(&routes, "GET /health/live").is_empty());
    assert!(audit["http_boundary"]["failures"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["declared_failure"] == "UserDisabled"
            && f["routes"]
                .as_array()
                .unwrap()
                .contains(&Value::from("DELETE /users/{user_id}"))));
}
#[test]
fn canonical_exchange_is_single_operation_with_selected_strategy_and_closed_success() {
    let path = root().join("examples/golden-todo-migration");
    let p = analyze_project(&path).unwrap();
    let (_, api, _) = outputs(&path, &p);
    let op = &api["paths"]["/auth/exchange"]["post"];
    assert_eq!(op["operationId"], "exchange_service_credential_api_bearer");
    assert_eq!(op["security"], serde_json::json!([{"api_bearer":[]} ]));
    assert_eq!(
        schema(&api, "/auth/exchange", "post", "200")["additionalProperties"],
        false
    );
    assert!(schema(&api, "/auth/exchange", "post", "200")["properties"]
        .get("access_token")
        .is_some());
    assert!(schema(&api, "/auth/exchange", "post", "401")["oneOf"].is_array());
    assert!(!api["paths"]["/auth/exchange"]["post"]["responses"]
        .get("422")
        .is_some());
}
const PUBLIC: &str = r#"
type Positive=Int {min:1}
input Body {n:Positive}
type Params=Object {n:Positive}
output Result {ok:Bool}
route GET /plain {auth:none output:Result action:{return Result {ok:true}}}
route GET /value/{id} {auth:none path:{id:Uuid} output:Result action:{return Result {ok:true}}}
route GET /shaped {auth:none query:Params headers:{h:Positive from "X-N"} output:Result action:{return Result {ok:true}}}
route POST /body {auth:none input:Body output:Result action:{return Result {ok:true}}}
"#;
#[test]
fn parser_families_follow_actual_shapes_and_do_not_invent_public_authentication() {
    let p = clean(PUBLIC);
    let (routes, api, audit) = outputs(Path::new("http-boundary.jadpo"), &p);
    assert!(failures(&routes, "GET /plain").is_empty());
    assert!(api["paths"]["/plain"]["get"]["responses"]
        .get("400")
        .is_none());
    assert!(api["paths"]["/plain"]["get"]["responses"]
        .get("401")
        .is_none());
    assert!(api["paths"]["/plain"]["get"]["responses"]
        .get("422")
        .is_none());
    assert_eq!(failures(&routes, "GET /value/{id}").len(), 1);
    assert!(api["paths"]["/value/{id}"]["get"]["responses"]
        .get("400")
        .is_none());
    assert_eq!(failures(&routes, "GET /shaped").len(), 4);
    for status in ["400", "422"] {
        assert_eq!(
            schema(&api, "/shaped", "get", status)["oneOf"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
    assert_eq!(
        schema(&api, "/body", "post", "422")["properties"]["error"]["properties"]["code"]["const"],
        "invalid_input"
    );
    assert_eq!(
        schema(&api, "/body", "post", "400")["properties"]["error"]["properties"]["message"]
            ["const"],
        "Request JSON syntax is invalid."
    );
    assert!(audit["http_boundary"]["failures"]
        .as_array()
        .unwrap()
        .iter()
        .all(|f| !f["code"].as_str().unwrap().starts_with("authentication")));
}
#[test]
fn unsupported_protected_application_has_explicit_unknown_without_fabricated_auth_family() {
    let p = clean(&PUBLIC.replace("route GET /plain {auth:none", "route GET /plain {"));
    let (routes, api, _) = outputs(Path::new("http-boundary.jadpo"), &p);
    assert!(failures(&routes, "GET /plain").is_empty());
    assert_eq!(
        row(&routes, "GET /plain")["http_boundary_analysis"]["qualified"],
        false
    );
    assert_eq!(
        row(&routes, "GET /plain")["http_boundary_analysis"]["unresolved"][0],
        "first_party_authentication_not_lowered"
    );
    assert!(api["paths"]["/plain"]["get"]["responses"]
        .get("401")
        .is_none());
}
#[test]
fn declared_same_status_code_keeps_domain_reference_without_oneof_double_match() {
    let src = PUBLIC.to_owned()
        + r#"
failure Bad {kind:InvalidValue code:"invalid_input" message:"Request body value is invalid."}
route POST /collision {auth:none input:Body output:Result action:fails Bad {reject Bad}}
"#;
    let p = clean(&src);
    let (routes, api, _) = outputs(Path::new("http-boundary.jadpo"), &p);
    assert_eq!(
        row(&routes, "POST /collision")["failures"][0]["name"],
        "Bad"
    );
    let alternatives = schema(&api, "/collision", "post", "422")["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(alternatives.len(), 2);
    assert_eq!(
        alternatives[0]["allOf"][0]["$ref"],
        "#/components/schemas/Bad"
    );
    assert!(alternatives[0]["allOf"][1]["not"]["anyOf"].is_array());
    assert_eq!(
        alternatives[1]["properties"]["error"]["properties"]["code"]["const"],
        "invalid_input"
    );
}

#[test]
fn identical_native_envelopes_share_one_wire_schema_but_keep_both_conditions() {
    let path = root().join("examples/golden-todo-migration");
    let mut sources = jadpo_core::discover_sources(&path).unwrap();
    let before = "failure UserDisabled {\n    kind: NotPermitted\n    code: \"user_disabled\"\n    message: \"This account is not available.\"\n}";
    let after = "failure UserDisabled {\n    kind: Rejected\n    code: \"invalid_value\"\n    message: \"Request path value is invalid.\"\n}";
    let source = sources
        .iter_mut()
        .find(|s| s.text.contains(before))
        .unwrap();
    source.text = source.text.replace(before, after);
    let project = analyze_sources(sources).unwrap();
    assert!(project
        .syntax
        .diagnostics()
        .chain(project.semantics.diagnostics.iter())
        .chain(project.typing.diagnostics.iter())
        .chain(project.failures.diagnostics.iter())
        .chain(project.entity_model.diagnostics.iter())
        .chain(project.policy.diagnostics.iter())
        .all(|d| d.severity != jadpo_diagnostics::Severity::Error));
    let (routes, api, audit) = outputs(&path, &project);
    let matching = failures(&routes, "GET /todos/{todo_id}")
        .iter()
        .filter(|f| f["code"] == "invalid_value" && f["http_status"] == 422)
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 2);
    assert_ne!(matching[0]["condition"], matching[1]["condition"]);
    let wire = schema(&api, "/todos/{todo_id}", "get", "422");
    assert!(
        wire.get("oneOf").is_none(),
        "Identical native envelopes must not double-match oneOf"
    );
    assert_eq!(
        wire["properties"]["error"]["properties"]["code"]["const"],
        "invalid_value"
    );
    for fact in matching {
        assert!(audit["http_boundary"]["failures"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["code"] == fact["code"] && f["condition"] == fact["condition"]));
    }
}

const RELATIONSHIP_COLLISION: &str = r#"
entity Parent {id:Uuid identity}
entity Child {id:Uuid identity parent_id:Parent.id references Parent.id on_delete restrict note:Text unique}
input NewChild {id:Child.id parent_id:Parent.id note:Child.note}
output Result {ok:Bool}
failure MutationRejected {kind:Conflict code:"relationship_conflict" message:"The operation conflicts with a relationship."}
action create_child(input:NewChild) ->Child fails MutationRejected {return attempt create Child {id:input.id parent_id:Child.parent_id(input.parent_id) note:input.note} conflict:MutationRejected}
route POST /child {auth:none input:NewChild output:Child run:create_child(input)}
route GET /declared {auth:none output:Result action:fails MutationRejected {reject MutationRejected}}
"#;
#[test]
fn relationship_native_wire_is_excluded_from_colliding_domain_reference() {
    let (routes, api, audit) = outputs(
        Path::new("relationship-wire.jadpo"),
        &clean(RELATIONSHIP_COLLISION),
    );
    assert_eq!(
        row(&routes, "POST /child")["failures"][0]["name"],
        "MutationRejected"
    );
    assert!(row(&routes, "POST /child")["operational_failures"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["code"] == "relationship_conflict"));
    let choices = schema(&api, "/child", "post", "409")["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(choices.len(), 2);
    assert_eq!(
        choices[0]["allOf"][0]["$ref"],
        "#/components/schemas/MutationRejected"
    );
    assert_eq!(
        choices[0]["allOf"][1]["not"]["anyOf"][0]["properties"]["error"]["properties"]["code"]
            ["enum"][0],
        "relationship_conflict"
    );
    assert_eq!(
        schema(&api, "/declared", "get", "409")["$ref"],
        "#/components/schemas/MutationRejected"
    );
    assert!(audit["operational_failures"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["code"] == "relationship_conflict"));
    // A different static public message is disjoint; preserve its exact reference.
    let (_, different, _) = outputs(
        Path::new("relationship-wire.jadpo"),
        &clean(&RELATIONSHIP_COLLISION.replace(
            "The operation conflicts with a relationship.",
            "Authored relationship conflict.",
        )),
    );
    assert_eq!(
        schema(&different, "/child", "post", "409")["oneOf"][0]["$ref"],
        "#/components/schemas/MutationRejected"
    );
}

#[test]
fn inactive_resolution_rejects_operational_status_categories() {
    let auth = include_str!("../../../../tests/compile/pass/164_mixed_service_exchange.jadpo");
    for kind in ["Conflict", "Unavailable"] {
        let source = auth.replace(
            "kind: NotPermitted code: \"user_disabled\"",
            &format!("kind: {kind} code: \"relationship_conflict\""),
        );
        let project =
            analyze_sources(vec![SourceFile::new("inactive-wire.jadpo".into(), source)]).unwrap();
        assert!(project
            .typing
            .diagnostics
            .iter()
            .any(|d| d.code == "TYPE_AUTH_INACTIVE_FAILURE_KIND"
                && d.severity == jadpo_diagnostics::Severity::Error));
    }
}
