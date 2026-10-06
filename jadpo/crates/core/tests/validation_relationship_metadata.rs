use jadpo_core::{analyze_project, analyze_sources, derive_artifacts, AnalyzedProject};
use jadpo_syntax::SourceFile;
use serde_json::Value;
use std::path::Path;

fn clean(source: &str) -> AnalyzedProject {
    let project = analyze_sources(vec![SourceFile::new(
        "relationship-metadata.jadpo".into(),
        source.into(),
    )])
    .unwrap();
    let diagnostics = project
        .syntax
        .diagnostics()
        .chain(project.semantics.diagnostics.iter())
        .chain(project.typing.diagnostics.iter())
        .chain(project.failures.diagnostics.iter())
        .chain(project.entity_model.diagnostics.iter())
        .chain(project.policy.diagnostics.iter())
        .collect::<Vec<_>>();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    project
}
fn outputs(project: &AnalyzedProject) -> (Value, Value, Value) {
    let artifacts = derive_artifacts(Path::new("relationship-metadata.jadpo"), project);
    let get = |path| {
        serde_json::from_str(
            &artifacts
                .iter()
                .find(|artifact| artifact.relative_path == path)
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
fn route<'a>(routes: &'a Value, name: &str) -> &'a Value {
    routes["routes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["route"] == name)
        .unwrap()
}
fn may(routes: &Value, name: &str) -> bool {
    let row = route(routes, name);
    assert_eq!(row["relationship_failure_analysis"]["qualified"], true);
    let expected = row["relationship_failure_analysis"]["may_fail"]
        .as_bool()
        .unwrap();
    assert_eq!(
        row["operational_failures"]
            .as_array()
            .unwrap()
            .iter()
            .any(|failure| failure["code"] == "relationship_conflict"),
        expected
    );
    expected
}
const SOURCE: &str = r#"
entity Parent { id: Uuid identity key: Text unique note: Text }
entity Child { id: Uuid identity parent_key: Parent.key references Parent.key on_delete restrict note: Text }
entity Cascade { id: Uuid identity parent_key: Parent.key references Parent.key on_delete cascade }
entity Leaf { id: Uuid identity cascade_id: Cascade.id references Cascade.id on_delete restrict }
entity Lone { id: Uuid identity note: Text }
input ChildInput { id: Child.id key: Parent.key note: Child.note }
input ParentInput { id: Parent.id key: Parent.key note: Parent.note }
input LoneInput { id: Lone.id note: Lone.note }
failure Missing { kind: NotFound code: "missing" }
failure MutationRejected { kind: Conflict code: "conflict" }
action create_child(input: ChildInput) -> Child fails MutationRejected {
 return attempt create Child { id: input.id parent_key: Child.parent_key(input.key) note: input.note } conflict: MutationRejected
}
action change_child(input: ChildInput) -> Child fails Missing, MutationRejected {
 return attempt update required Child { where: id == input.id set: { parent_key: Child.parent_key(input.key) } missing: Missing conflict: MutationRejected }
}
action change_parent_key(input: ParentInput) -> Parent fails Missing, MutationRejected {
 return attempt update required Parent { where: id == input.id set: { key: input.key } missing: Missing conflict: MutationRejected }
}
action rename_parent(input: ParentInput) -> Parent fails Missing, MutationRejected {
 return attempt update required Parent { where: id == input.id set: { note: input.note } missing: Missing conflict: MutationRejected }
}
action remove_parent(input: ParentInput) -> Parent fails Missing, MutationRejected {
 return attempt delete required Parent { where: id == input.id missing: Missing conflict: MutationRejected }
}
action remove_child(input: ChildInput) -> Child fails Missing, MutationRejected {
 return attempt delete required Child { where: id == input.id missing: Missing conflict: MutationRejected }
}
action find_child(input: ChildInput) -> Child fails Missing {
 return attempt query required Child { where: id == input.id missing: Missing }
}
action create_lone(input: LoneInput) -> Lone { return attempt create Lone { id: input.id note: input.note } }
action recursive_a(input: ChildInput) -> Child fails MutationRejected { return attempt recursive_b(input) }
action recursive_b(input: ChildInput) -> Child fails MutationRejected {
 if input.note == Child.note("again") { return attempt recursive_a(input) } else { return attempt create_child(input) }
}
route POST /create { auth: none input: ChildInput output: Child run: create_child(input) }
route POST /change-child { auth: none input: ChildInput output: Child run: change_child(input) }
route POST /change-key { auth: none input: ParentInput output: Parent run: change_parent_key(input) }
route POST /rename { auth: none input: ParentInput output: Parent run: rename_parent(input) }
route POST /remove-parent { auth: none input: ParentInput output: Parent run: remove_parent(input) }
route POST /remove-child { auth: none input: ChildInput output: Child run: remove_child(input) }
route POST /read { auth: none input: ChildInput output: Child run: find_child(input) }
route POST /unrelated { auth: none input: LoneInput output: Lone run: create_lone(input) }
route POST /cycle { auth: none input: ChildInput output: Child run: recursive_a(input) }
route POST /inline { auth: none input: ChildInput output: Child action: fails MutationRejected { return attempt create_child(input) } }
"#;

#[test]
fn checked_mutated_columns_and_physical_delete_are_route_specific() {
    let (routes, api, audit) = outputs(&clean(SOURCE));
    for path in [
        "create",
        "change-child",
        "change-key",
        "remove-parent",
        "cycle",
        "inline",
    ] {
        assert!(may(&routes, &format!("POST /{path}")), "{path}");
        assert!(api["paths"][format!("/{path}")]["post"]["responses"]
            .get("409")
            .is_some());
    }
    for path in ["rename", "remove-child", "read", "unrelated"] {
        assert!(!may(&routes, &format!("POST /{path}")), "{path}");
    }
    assert!(api["paths"]["/read"]["post"]["responses"]
        .get("409")
        .is_none());
    assert!(api["paths"]["/unrelated"]["post"]["responses"]
        .get("409")
        .is_none());
    let descriptor = audit["operational_failures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["code"] == "relationship_conflict")
        .unwrap();
    assert_eq!(descriptor["http_status"], 409);
    assert_eq!(
        descriptor["message"],
        "The operation conflicts with a relationship."
    );
    assert_eq!(descriptor["automatic_retry"], false);
    let schema = &api["paths"]["/create"]["post"]["responses"]["409"]["content"]
        ["application/json"]["schema"];
    let alternatives = schema["oneOf"].as_array().unwrap();
    assert!(alternatives
        .iter()
        .any(|row| row["$ref"] == "#/components/schemas/MutationRejected"));
    let native = alternatives
        .iter()
        .find(|row| {
            row["properties"]["error"]["properties"]["code"]["enum"][0] == "relationship_conflict"
        })
        .unwrap();
    assert_eq!(native["additionalProperties"], false);
    assert_eq!(native["properties"]["error"]["additionalProperties"], false);
}
#[test]
fn delete_cascade_reaches_restrict_but_outgoing_and_cascade_only_are_not_conflicts() {
    let source = SOURCE.replace(
        "parent_key: Parent.key references Parent.key on_delete restrict",
        "parent_key: Parent.key references Parent.key on_delete cascade",
    );
    let (routes, _, _) = outputs(&clean(&source));
    assert!(may(&routes, "POST /remove-parent"));
    let source = source.replace(
        "cascade_id: Cascade.id references Cascade.id on_delete restrict",
        "cascade_id: Cascade.id references Cascade.id on_delete cascade",
    );
    let (routes, api, _) = outputs(&clean(&source));
    assert!(!may(&routes, "POST /remove-parent"));
    // The declared ordinary MutationRejected still owns409; no native alternative added.
    assert_eq!(
        api["paths"]["/remove-parent"]["post"]["responses"]["409"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/MutationRejected"
    );
}
#[test]
fn normalized_separate_specs_and_renamed_fields_do_not_use_nominal_prefix_guesses() {
    let source=SOURCE.replace("entity Child { id: Uuid identity parent_key: Parent.key references Parent.key on_delete restrict note: Text }",
      "type Child = Object { id: Uuid parent_key: Parent.key note: Text } persist Child { identity: id references parent_key: Parent.key on_delete: restrict }")
      .replace("Parent","Account").replace("Child","Entry");
    let (routes, _, _) = outputs(&clean(&source));
    assert!(may(&routes, "POST /create"));
    assert!(may(&routes, "POST /change-key"));
    assert!(!may(&routes, "POST /rename"));
    let source = SOURCE
        .replace(" references Parent.key on_delete restrict", "")
        .replace(" references Parent.key on_delete cascade", "")
        .replace(" references Cascade.id on_delete restrict", "");
    let (routes, _, _) = outputs(&clean(&source));
    for path in ["create", "change-child", "change-key", "remove-parent"] {
        assert!(!may(&routes, &format!("POST /{path}")));
    }
}
#[test]
fn lifecycle_logical_delete_and_reads_remain_actual_nonreference_updates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let project = analyze_project(
        &root.join("tests/compile/pass/lifecycle/170_lifecycle_initial_values.jadpo"),
    )
    .unwrap();
    let (routes, api, _) = outputs(&project);
    assert!(may(&routes, "POST /lifecycle/todos"));
    for path in [
        "/lifecycle/users",
        "/lifecycle/reminders",
        "/lifecycle/todos/{todo_id}/delete",
        "/lifecycle/todos/{todo_id}/rename",
    ] {
        assert!(!may(&routes, &format!("POST {path}")), "{path}");
    }
    assert!(api["paths"]["/lifecycle/reminders"]["post"]["responses"]
        .get("409")
        .is_none());
}

const PATCH: &str = r#"
entity Parent { id: Uuid identity }
entity Child { id: Uuid identity parent_id: Parent.id references Parent.id on_delete restrict note: Text }
input PatchKey { parent_id: Child.parent_id optional }
input PatchNote { note: Child.note optional }
input Both { id: Child.id key: PatchKey note: PatchNote }
failure Empty { kind: InvalidValue code: "empty" }
failure Missing { kind: NotFound code: "missing" }
failure MutationRejected { kind: Conflict code: "conflict" }
action patch_key(id: Child.id, input: PatchKey) -> Child fails Empty,Missing,MutationRejected {
 return attempt update required Child {where: id == id patch: input empty: Empty missing: Missing conflict: MutationRejected}
}
action shadow(id: Child.id,input: PatchKey,other: PatchNote) -> Child fails Empty,Missing,MutationRejected {
 var input=other
 return attempt update required Child {where: id == id patch: input empty: Empty missing: Missing conflict: MutationRejected}
}
route POST /patch-key {auth:none input:Both output:Child run:patch_key(input.id,input.key)}
route POST /shadow {auth:none input:Both output:Child run:shadow(input.id,input.key,input.note)}
"#;
#[test]
fn patch_mapping_uses_checked_lexical_occurrence_not_same_named_parameter() {
    let (routes, _, _) = outputs(&clean(PATCH));
    assert!(may(&routes, "POST /patch-key"));
    assert!(!may(&routes, "POST /shadow"));
}

#[test]
fn checked_nullable_set_null_delete_is_not_a_relationship_conflict() {
    let source = r#"
entity Parent { id: Uuid identity }
entity Child { id: Uuid identity parent_id: Parent.id? references Parent.id on_delete set_null }
input Request { id: Parent.id }
failure Missing {kind:NotFound code:"missing"}
failure MutationRejected {kind:Conflict code:"conflict"}
action remove(id:Parent.id) ->Parent fails Missing,MutationRejected {
 return attempt delete required Parent {where:id==id missing:Missing conflict:MutationRejected}
}
route POST /remove {auth:none input:Request output:Parent run:remove(input.id)}
"#;
    let (routes, api, _) = outputs(&clean(source));
    assert!(!may(&routes, "POST /remove"));
    assert_eq!(
        api["paths"]["/remove"]["post"]["responses"]["409"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/MutationRejected"
    );
}
