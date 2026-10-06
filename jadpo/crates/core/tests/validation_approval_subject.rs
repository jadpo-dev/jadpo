use jadpo_core::{
    analyze_project, analyze_sources, approval_text, derive_approval_subject,
    derive_approval_subject_with_state_pin, AnalyzedProject,
};
use jadpo_syntax::SourceFile;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

const SOURCE: &str = r#"
entity Customer {
    id: Uuid
    policy { Access.public: [read] }
}
input FindCustomer { id: Customer.id }
failure MissingCustomer { kind: NotFound code: "missing_customer" }
action leaf(input: FindCustomer) fails MissingCustomer -> Customer {
    policy { Access.public: [invoke] }
    return attempt query required Customer { where: id == input.id missing: MissingCustomer }
}
action middle(input: FindCustomer) fails MissingCustomer -> Customer {
    return attempt leaf(input)
}
action outer(input: FindCustomer) fails MissingCustomer -> Customer {
    return attempt middle(input)
}
route POST /customer { auth: none input: FindCustomer output: Customer run: outer(input) }
"#;

fn compile_fixture(name: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    std::fs::read_to_string(root.join("tests/compile/pass").join(name)).unwrap()
}

fn field_contracts(subject: &Value) -> &Value {
    &subject["canonical"]["after"]["impact"]["field_source_contracts"]
}

fn source_rows<'a>(subject: &'a Value, owner: &str, kind: &str) -> Vec<&'a Value> {
    field_contracts(subject)["operations"][owner]["contracts"]
        .as_object()
        .unwrap()
        .values()
        .filter(|row| row["kind"] == kind)
        .collect()
}

#[test]
fn field_projection_contracts_keep_nominal_targets_values_and_policy_not_row_origins() {
    let source = compile_fixture("134_policy_safe_projection.jadpo");
    let reviewed = subject(&source, None);
    let summary = source_rows(&reviewed, "PrivateNote.summary", "constructor_mapping")[0];
    assert_eq!(summary["contract"]["target"]["name"], "NoteSummary");
    let fields = summary["contract"]["fields"].as_array().unwrap();
    assert_eq!(
        fields
            .iter()
            .map(|field| field["field"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["id", "title"]
    );
    assert_eq!(
        fields[1]["target_field"]["contract"]["type"]["symbol"]["name"],
        "PrivateNote.title"
    );
    assert_eq!(
        fields[1]["value"]["tokens"],
        serde_json::json!(["note", ".", "title"])
    );
    assert_eq!(fields[1]["value"]["data_origin"]["status"], "not_resolved");
    let private = source_rows(
        &reviewed,
        "PrivateNote.read_private_label",
        "constructor_mapping",
    )[0];
    assert_eq!(
        private["contract"]["fields"][0]["target_field"]["contract"]["type"]["symbol"]["name"],
        "PrivateNote.private_label"
    );
    assert!(
        field_contracts(&reviewed)["operations"]["PrivateNote.read_private_label"]
            ["restricted_field_reads"]
            .as_array()
            .unwrap()
            .iter()
            .any(|read| read["field"] == "private_label" && read["source"] == "typed_expression")
    );
    assert_eq!(
        field_contracts(&reviewed)["operations"]["PrivateNote.summary"]["entry_point_status"],
        "no_checked_route_link"
    );
    let changed = source.replace(
        "title: note.title",
        "title: PrivateNote.title(\"constant\")",
    );
    let delta = subject(&changed, Some(&source));
    assert_ne!(
        source_rows(&delta, "PrivateNote.summary", "constructor_mapping")[0],
        summary
    );
    assert_eq!(
        delta["canonical"]["before"]["graph_digest"],
        delta["canonical"]["after"]["graph_digest"]
    );
}

#[test]
fn field_patch_mappings_cover_input_value_nullable_omission_and_derived_supplied_fields() {
    let input = compile_fixture("40_omission_aware_patch.jadpo");
    for source in [
        input.clone(),
        input.replace("input PatchCustomer {", "type PatchCustomer = Object {"),
    ] {
        let reviewed = subject(&source, None);
        let update = source_rows(&reviewed, "patch_customer", "update_fields")[0];
        let patch = &update["contract"]["patch"];
        assert_eq!(patch["status"], "checked_patch_record_mapping");
        assert_eq!(patch["mappings"].as_array().unwrap().len(), 2);
        let nickname = patch["mappings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|mapping| mapping["target_field"]["contract"]["field"] == "nickname")
            .unwrap();
        assert_eq!(
            nickname["condition"]["path"],
            serde_json::json!(["input", "nickname"])
        );
        assert_eq!(nickname["omission"], "no_assignment");
        assert_eq!(nickname["present_null"], "assignment_of_null");
        assert_eq!(
            nickname["source_patch_field"]["type"]["symbol"]["name"],
            "Customer.nickname"
        );
        assert_eq!(update["contract"]["changes"], serde_json::json!([]));
        let triggers = source_rows(&reviewed, "patch_customer", "failure_boundary")
            .iter()
            .filter_map(|row| row["contract"]["trigger"].as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            triggers,
            std::collections::BTreeSet::from([
                "patch_empty",
                "update_missing",
                "constraint_conflict"
            ])
        );
    }
    let derived = subject(&compile_fixture("44_patch_derived_change.jadpo"), None);
    let change = &source_rows(&derived, "patch_todo", "update_fields")[0]["contract"]
        ["conditional_changes"][0];
    assert_eq!(change["field"]["contract"]["field"], "reminder_sent_at");
    assert_eq!(change["condition"]["path"], "input.due_at");
    assert_eq!(change["value"]["tokens"], serde_json::json!(["none"]));
    let changed = compile_fixture("44_patch_derived_change.jadpo").replace(
        "reminder_sent_at: none",
        "reminder_sent_at: Todo.reminder_sent_at(clock.now)",
    );
    let delta = subject(
        &changed,
        Some(&compile_fixture("44_patch_derived_change.jadpo")),
    );
    assert_ne!(
        delta["canonical"]["before"]["facts"]["operation_field_contracts:patch_todo"],
        delta["canonical"]["after"]["facts"]["operation_field_contracts:patch_todo"]
    );
}

#[test]
fn field_plain_patch_mapping_set_is_stable_under_input_and_value_declaration_order() {
    let original = compile_fixture("40_omission_aware_patch.jadpo");
    for source in [
        original.clone(),
        original.replace("input PatchCustomer {", "type PatchCustomer = Object {"),
    ] {
        let reversed = source.replace(
            "    email: Customer.email optional\n    nickname: Customer.nickname optional",
            "    nickname: Customer.nickname optional\n    email: Customer.email optional",
        );
        assert_ne!(source, reversed);
        let before = subject(&source, None);
        let delta = subject(&reversed, Some(&source));
        let original = source_rows(&before, "patch_customer", "update_fields")[0];
        let reordered = source_rows(&delta, "patch_customer", "update_fields")[0];
        assert_eq!(original, reordered);
        assert_eq!(
            reordered["contract"]["patch"]["mappings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|mapping| mapping["source_patch_field"]["field"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["email", "nickname"]
        );
        assert_ne!(
            delta["canonical"]["before"]["source_digest"],
            delta["canonical"]["after"]["source_digest"]
        );
    }
}

#[test]
fn field_owned_receiver_guard_and_inline_call_witnesses_are_exact_not_double_qualified() {
    let original = compile_fixture("120_entity_dossier_queries.jadpo");
    let source=format!("{original}\ninput Rename {{ customer: Customer email: Customer.email }}\nroute POST /rename {{ auth: none input: Rename output: Customer action: fails CustomerNotFound, EmailAlreadyUsed {{ return attempt rename_customer(input.customer,input.email) }} }}");
    let reviewed = subject(&source, None);
    let surface =
        &source_rows(&reviewed, "Customer.change_email", "operation_surface")[0]["contract"];
    assert_eq!(surface["receiver"], "value");
    assert_eq!(surface["mutation_guard"], "revision");
    assert!(field_contracts(&reviewed)["operations"]
        .get("Customer.Customer.change_email")
        .is_none());
    let route = &field_contracts(&reviewed)["routes"]["POST /rename"];
    assert_eq!(
        route["operations"]["Customer.change_email"]["path"],
        serde_json::json!(["POST /rename", "rename_customer", "Customer.change_email"])
    );
    let changed = source.replace("guard: revision", "guard: reload");
    let delta = subject(&changed, Some(&source));
    assert_eq!(
        source_rows(&delta, "Customer.change_email", "operation_surface")[0]["contract"]
            ["mutation_guard"],
        "reload"
    );
    assert_ne!(
        delta["canonical"]["before"]["facts"]["route_field_contracts:POST /rename"],
        delta["canonical"]["after"]["facts"]["route_field_contracts:POST /rename"]
    );
}

#[test]
fn field_page_contract_keeps_ordered_cursor_and_missing_outer_inference() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let path = root.join("examples/golden-todo-migration");
    let project = analyze_project(&path).unwrap();
    let raw = derive_approval_subject(&path, &project, None, None, None).unwrap();
    let reviewed: Value = serde_json::from_str(&raw).unwrap();
    let query = source_rows(&reviewed, "Todo.page_todos", "query_fields")[0];
    let page = &query["contract"]["page"];
    assert!(query["contract"]["predicate"].is_null());
    assert!(query["contract"]["order"].is_null());
    assert_eq!(page["result"]["symbol"]["name"], "TodoPage");
    assert_eq!(page["projection"]["symbol"]["name"], "TodoView");
    assert_eq!(page["cursor"]["symbol"]["name"], "TodoCursor");
    assert_eq!(
        page["order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| (
                row["field"]["contract"]["field"].as_str().unwrap(),
                row["direction"].as_str().unwrap()
            ))
            .collect::<Vec<_>>(),
        [("created_at", "descending"), ("id", "descending")]
    );
    assert_eq!(
        page["cursor_fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| field["contract"]["field"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["created_at", "id"]
    );
    assert_eq!(
        page["predicates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|predicate| predicate["operator"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["Equal", "Equal", "OptionalEqual", "OptionalLessEqual"]
    );
    assert_eq!(page["after_optional"], true);
    let child = source_rows(&reviewed, "Todo.page_todos", "expression")
        .into_iter()
        .find(|row| {
            row["contract"]["tokens"].as_array().unwrap().first()
                == Some(&serde_json::json!("query"))
        })
        .unwrap();
    assert_eq!(
        child["contract"]["type_evidence"]["status"],
        "observation_missing"
    );
}

#[test]
fn field_nested_include_has_no_fabricated_authored_pagination() {
    let reviewed = subject(&compile_fixture("49_bounded_nested_include.jadpo"), None);
    let include = &source_rows(&reviewed, "get_todo_owner_profile", "query_fields")[0]["contract"]
        ["includes"][0];
    assert_eq!(include["relationship"], "owner");
    assert_eq!(include["nested_relationship"], "profile");
    assert_eq!(include["cardinality"], "optional");
    assert_eq!(include["result"]["symbol"]["name"], "TodoOwnerProfile");
    assert!(include["order"].is_null());
    assert!(include["pagination"].is_null());
    assert_eq!(
        include["bounds_evidence"],
        "no_authored_order_or_pagination"
    );
    let many=subject("entity User { id: Uuid identity inverse todos: many Todo via Todo.owner_id }\nentity Todo { id: Uuid identity owner_id: User.id references User.id on_delete cascade }\noutput UserTodos { parent: User todos: List<Todo> }\nfailure Missing { kind: NotFound code: \"missing\" }\naction load(id: User.id) fails Missing -> UserTodos { return attempt query required User { where: id == id include: todos into: UserTodos order_by: id desc limit: 7 offset: 2 missing: Missing } }",None);
    let include = &source_rows(&many, "load", "query_fields")[0]["contract"]["includes"][0];
    assert_eq!(include["cardinality"], "many");
    assert_eq!(include["order"]["direction"], "descending");
    assert_eq!(
        include["pagination"]["limit"]["tokens"],
        serde_json::json!(["7"])
    );
    assert_eq!(
        include["pagination"]["offset"]["tokens"],
        serde_json::json!(["2"])
    );
}

#[test]
fn field_enum_payload_and_symbolic_match_outcome_triggers_are_not_flattened() {
    let reviewed = subject(&compile_fixture("56_tagged_sum_match.jadpo"), None);
    let catalogue = field_contracts(&reviewed)["catalogue"].as_object().unwrap();
    assert!(catalogue
        .values()
        .any(|row| row["owner"]["name"] == "PaymentOutcome.paid"
            && row["contract"]["field"] == "receipt_id"
            && row["record_kind"] == "enum_payload"));
    let constructors = source_rows(&reviewed, "describe_outcome", "constructor_mapping");
    assert_eq!(constructors.len(), 3);
    assert!(constructors
        .iter()
        .all(|row| row["conditions"][0]["kind"] == "match_arm"
            && row["condition_status"] == "symbolic_not_proved"));
    assert_eq!(
        source_rows(&reviewed, "make_paid", "constructor_mapping")[0]["contract"]["fields"][0]
            ["target_field"]["contract"]["type"]["symbol"]["name"],
        "ReceiptId"
    );
    let outcomes = subject(
        &compile_fixture("113_outcome_match_recovery_mapping_propagation.jadpo"),
        None,
    );
    let triggers = field_contracts(&outcomes)["operations"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|operation| operation["contracts"].as_object().unwrap().values())
        .filter(|row| row["kind"] == "failure_boundary")
        .filter_map(|row| row["contract"]["trigger"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(triggers.contains("outcome_propagate"));
    assert!(triggers.contains("outcome_reject"));
}

#[test]
fn field_pure_response_delta_is_not_actor_admission_or_terminal_effect() {
    let before="output Envelope { value: Text }\naction response() -> Envelope { return Envelope { value: \"v1\" } }\nroute GET /health { auth: none output: Envelope run: response() }";
    let after = before.replace("\"v1\"", "\"v2\"");
    let delta = subject(&after, Some(before));
    assert_eq!(delta["canonical"]["scenarios"], serde_json::json!([]));
    assert_ne!(
        delta["canonical"]["before"]["facts"]["route_field_contracts:GET /health"],
        delta["canonical"]["after"]["facts"]["route_field_contracts:GET /health"]
    );
    assert!(delta["canonical"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["subject"] == "route_field_contracts:GET /health"
            && row["change"] == "changed"));
    let modes = subject(&compile_fixture("68_route_success_modes.jadpo"), None);
    assert_eq!(
        field_contracts(&modes)["routes"]["POST /items"]["surface"]["success"],
        "created"
    );
    assert_eq!(
        field_contracts(&modes)["routes"]["DELETE /items"]["surface"]["success"],
        "nocontent"
    );
    assert!(field_contracts(&modes)["routes"]["DELETE /items"]["surface"]["output"].is_null());
    assert_eq!(
        source_rows(&modes, "remove_item", "operation_surface")[0]["contract"]["return"]["symbol"]
            ["name"],
        "Unit"
    );
}

#[test]
fn field_source_and_route_mapping_omissions_fail_even_when_rehashed() {
    let source = compile_fixture("165_route_query_headers.jadpo");
    let expected = subject(&source, None);
    let raw = serde_json::to_string(&expected).unwrap();
    assert_eq!(
        field_contracts(&expected)["routes"]["GET /items"]["surface"]["headers"][0]["wire_name"],
        "\"X-Trace\""
    );
    for route in [false, true] {
        let mut omitted = expected.clone();
        if route {
            omitted["canonical"]["after"]["impact"]["field_source_contracts"]["routes"]
                ["GET /items"]["operations"] = serde_json::json!({});
        } else {
            let operation = &mut omitted["canonical"]["after"]["impact"]["field_source_contracts"]
                ["operations"]["GET /items"]["contracts"];
            let key = operation
                .as_object()
                .unwrap()
                .iter()
                .find(|(_, row)| row["kind"] == "constructor_mapping")
                .map(|(id, _)| id.clone())
                .unwrap();
            operation.as_object_mut().unwrap().remove(&key);
        }
        omitted["subject_digest"] = serde_json::json!(format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&omitted["canonical"]).unwrap())
        ));
        assert!(jadpo_core::validate_approval_export(
            &raw,
            &serde_json::to_string(&omitted).unwrap()
        )
        .is_err());
    }
}

#[test]
fn field_nested_constructors_admin_projection_whole_aliases_and_invalid_patches_remain_explicit() {
    let source="output Inner { value: Text }\noutput Outer { nested: Inner }\naction nested() -> Outer { return Outer { nested: Inner { value: \"x\" } } }\naction alias() -> Outer { var value = nested() return value }\nroute GET /nested { auth: none output: Outer run: alias() }";
    let reviewed = subject(source, None);
    let inner = source_rows(&reviewed, "nested", "constructor_mapping")
        .into_iter()
        .find(|row| row["contract"]["target"]["name"] == "Inner")
        .unwrap();
    assert_eq!(
        inner["contract"]["target_path"],
        serde_json::json!(["Outer", "nested"])
    );
    assert_eq!(
        inner["contract"]["fields"][0]["target_field"]["contract"]["type"]["symbol"]["name"],
        "Text"
    );
    let alias = source_rows(&reviewed, "alias", "return_boundary")[0];
    assert_eq!(alias["contract"]["boundary"], "whole_value");
    assert_eq!(alias["contract"]["origin_resolution"], "not_performed");
    assert_eq!(
        field_contracts(&reviewed)["routes"]["GET /nested"]["operations"]["nested"]["path"],
        serde_json::json!(["GET /nested", "alias", "nested"])
    );
    let admin = subject(&policy_binding_source(), None);
    let private = source_rows(&admin, "User.administrative_details", "constructor_mapping")[0]
        ["contract"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|field| field["field"] == "private_email")
        .unwrap()
        .clone();
    assert_eq!(
        private["target_field"]["contract"]["type"]["symbol"]["name"],
        "User.private_email"
    );
    assert_eq!(private["value"]["data_origin"]["status"], "not_resolved");
    let original = compile_fixture("40_omission_aware_patch.jadpo");
    for invalid in [
        original.replace("input PatchCustomer {", "output PatchCustomer {"),
        original.replace("input PatchCustomer {", "entity PatchCustomer {"),
        original.replace("Customer.email optional", "Customer.email"),
    ] {
        let project = analyzed("/project", &invalid);
        assert!(derive_approval_subject(
            Path::new("/project/app.jadpo"),
            &project,
            None,
            None,
            None
        )
        .is_err());
    }
}

fn analyzed(root: &str, source: &str) -> AnalyzedProject {
    analyze_sources(vec![SourceFile::new(
        Path::new(root).join("app.jadpo"),
        source.into(),
    )])
    .unwrap()
}
fn subject(source: &str, before: Option<&str>) -> Value {
    let after = analyzed("/project", source);
    let baseline = before.map(|s| analyzed("/baseline", s));
    let raw = derive_approval_subject(
        Path::new("/project/app.jadpo"),
        &after,
        baseline
            .as_ref()
            .map(|b| (Path::new("/baseline/app.jadpo"), b)),
        Some("Review the exact behavioral change"),
        None,
    )
    .unwrap_or_else(|e| {
        panic!(
            "{e}: {:?} {:?} {:?} policy={:?} entity={:?}",
            after.syntax.diagnostics().collect::<Vec<_>>(),
            after.typing.diagnostics,
            after.failures.diagnostics,
            after.policy.diagnostics,
            after.entity_model.diagnostics
        )
    });
    serde_json::from_str(&raw).unwrap()
}

const JOB_SOURCE: &str = r#"
type JobRunAt = Instant {}
action scan(at: JobRunAt) -> Unit {}
action other_scan(at: JobRunAt) -> Unit {}
job overdue_reminders every 15m {
    concurrency: singleton
    run: scan(JobRunAt(clock.now))
    retry: next_schedule
}
"#;

fn policy_binding_source() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    std::fs::read_to_string(root.join("examples/policy-runtime/app.jadpo")).unwrap()
}

fn principal_source() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    std::fs::read_to_string(
        root.join("tests/compile/pass/128_authentication_strategy_topology.jadpo"),
    )
    .unwrap()
}

fn admin_route_source() -> String {
    format!(
        "{}\n{}",
        policy_binding_source(),
        r#"
action wrapper_health() -> Health { return administrative_health() }
route GET /admin-health { output: Health run: wrapper_health() }
"#
    )
}

fn route_sources<'a>(subject: &'a Value, route: &str) -> &'a Value {
    &subject["canonical"]["after"]["impact"]["actor_sources"]["routes"][route]
}

#[test]
fn effectless_nested_invoke_preserves_application_membership_and_changed_admission() {
    let original = admin_route_source();
    let changed = original
        .replace("member: user_id", "member: alternate_user_id")
        .replace(
            "    user_id: User.id\n",
            "    user_id: User.id\n    alternate_user_id: User.id\n",
        );
    let reviewed = subject(&changed, Some(&original));
    let route = route_sources(&reviewed, "GET /admin-health");
    let nested = route["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["operation"] == "administrative_health")
        .unwrap();
    assert_eq!(
        nested["path"],
        serde_json::json!([
            "GET /admin-health",
            "wrapper_health",
            "administrative_health"
        ])
    );
    let surface = &nested["contract"]["surfaces"][0];
    assert_eq!(surface["provenance"], "operation");
    assert_eq!(surface["scope"], "application");
    assert_eq!(surface["subject_composition"], "alternatives_or");
    assert_eq!(surface["actors"][0]["subject"], "ApplicationRole.operator");
    let sources = surface["actors"][0]["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 2);
    assert!(sources.iter().any(
        |row| row["contract_id"] == "membership:ApplicationMembership"
            && row["contract"]["member_field"] == "alternate_user_id"
            && row["conditions"]["scope_equality"] == "application_wide"
    ));
    assert!(sources.iter().any(|row| row["contract_id"]
        == "membership:ApplicationServiceMembership"
        && row["contract"]["principal_entity"] == "Service"));
    let admission = reviewed["canonical"]["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["route"] == "GET /admin-health")
        .unwrap();
    assert_eq!(admission["kind"], "route_admission");
    assert!(admission["effect"].is_null());
    assert_eq!(
        admission["status"],
        "route_actor_constraints_changed_admission_contract"
    );
    assert_eq!(admission["certainty"], "compiler_checked_route_contract");
    assert_eq!(admission["feasibility"], "not_analyzed");
    assert_ne!(admission["before"], admission["after"]);
    assert_eq!(
        reviewed["canonical"]["before"]["graph"]["calls"],
        reviewed["canonical"]["after"]["graph"]["calls"]
    );
}

#[test]
fn effectless_public_route_add_remove_auth_and_unchanged_have_distinct_admission_rows() {
    let original = policy_binding_source();
    let protected = original.replace("    auth: none\n", "");
    let changed = subject(&protected, Some(&original));
    let row = changed["canonical"]["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["route"] == "GET /public-health")
        .unwrap();
    assert_eq!(row["kind"], "route_admission");
    assert_eq!(row["actor_access"]["before_route_auth"], "none");
    assert_ne!(row["actor_access"]["after_route_auth"], "none");
    let added_source = format!(
        "{original}\nroute GET /more-health {{ auth: none output: Health run: public_health() }}\n"
    );
    let added = subject(&added_source, Some(&original));
    let removed = subject(&original, Some(&added_source));
    let lookup = |value: &Value| {
        value["canonical"]["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["route"] == "GET /more-health")
            .unwrap()
            .clone()
    };
    assert_eq!(lookup(&added)["status"], "route_added_admission_contract");
    assert_eq!(
        lookup(&removed)["status"],
        "route_removed_admission_contract"
    );
    assert_eq!(lookup(&added)["id"], lookup(&removed)["id"]);
    assert_eq!(
        subject(&original, Some(&original))["canonical"]["scenarios"],
        serde_json::json!([])
    );
    assert!(subject(&original, None)["canonical"]["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["status"] == "after_only_without_baseline"));
}

#[test]
fn nested_restricted_field_and_role_sources_retain_scope_and_original_provenance() {
    let source = format!(
        "{}\n{}",
        policy_binding_source(),
        r#"
action admin_details(id: User.id) fails UserNotFound -> Unit {
    var details = attempt User.administrative_details(id)
}
route GET /admin-details { path: { id: User.id } success: no_content run: admin_details(path.id) }
"#
    );
    // The route path binding must match its declared URL capture.
    let source = source.replace("GET /admin-details {", "GET /admin-details/{id} {");
    let reviewed = subject(&source, None);
    let route = route_sources(&reviewed, "GET /admin-details/{id}");
    let nested = route["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["operation"] == "User.administrative_details")
        .unwrap();
    assert_eq!(
        nested["path"],
        serde_json::json!([
            "GET /admin-details/{id}",
            "admin_details",
            "User.administrative_details"
        ])
    );
    let surfaces = nested["contract"]["surfaces"].as_array().unwrap();
    assert!(surfaces
        .iter()
        .any(|row| row["provenance"] == "operation_exception" && row["entity"] == "User"));
    assert!(surfaces
        .iter()
        .any(|row| row["kind"] == "restricted_field_read"
            && row["field"] == "private_email"
            && row["provenance"] == "typed_expression"));
    let changed = source.replace(
        "        role: CompanyRole.owner\n",
        "        role: CompanyRole.viewer\n",
    );
    let delta = subject(&changed, Some(&source));
    // Unrelated role source must not manufacture a changed admin-details link.
    assert_eq!(
        route_sources(&delta, "GET /admin-details/{id}"),
        route_sources(&reviewed, "GET /admin-details/{id}")
    );
}

#[test]
fn rehashed_actor_link_and_effectless_admission_omissions_fail_fresh_export() {
    let source = admin_route_source();
    let expected = subject(&source, None);
    let raw = serde_json::to_string(&expected).unwrap();
    for omit_scenarios in [false, true] {
        let mut exported = expected.clone();
        if omit_scenarios {
            exported["canonical"]["scenarios"] = serde_json::json!([]);
        } else {
            exported["canonical"]["after"]["impact"]["actor_sources"]["routes"]
                ["GET /admin-health"]["operations"] = serde_json::json!([]);
        }
        exported["subject_digest"] = serde_json::json!(format!(
            "sha256:{:x}",
            Sha256::digest(
                serde_json::to_string(&exported["canonical"])
                    .unwrap()
                    .as_bytes()
            )
        ));
        assert!(jadpo_core::validate_approval_export(
            &raw,
            &serde_json::to_string(&exported).unwrap()
        )
        .is_err());
    }
}

#[test]
fn resource_actor_sources_keep_exact_role_scope_and_direct_vs_membership_conditions() {
    let source = format!(
        "{}\n{}",
        policy_binding_source(),
        r#"
failure ArticleMissing { kind: NotFound code: "article_missing" }
output ArticleTitle { title: Article.title }
action read_article(id: Article.id) fails ArticleMissing -> ArticleTitle {
    var article = attempt query required Article { where: id == id missing: ArticleMissing }
    return ArticleTitle { title: article.title }
}
route GET /articles/{id} { path: { id: Article.id } output: ArticleTitle run: read_article(path.id) }
"#
    );
    let source = source.replace(
        "        CompanyRole.editor: [read, update]\n",
        "        CompanyRole.editor: [read, update]\n        ApplicationRole.operator: [read]\n",
    );
    let reviewed = subject(&source, None);
    let route = route_sources(&reviewed, "GET /articles/{id}");
    let operation = route["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["operation"] == "read_article")
        .unwrap();
    let surface = operation["contract"]["surfaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["entity"] == "Article" && row["effect"] == "read")
        .unwrap();
    assert_eq!(surface["scope"], "Company");
    assert_eq!(surface["scope_field"], "company_id");
    assert_eq!(surface["target_identity_field"], "id");
    assert_eq!(surface["subject_composition"], "alternatives_or");
    let actors = surface["actors"].as_array().unwrap();
    assert_eq!(actors.len(), 4);
    for actor in actors {
        let sources = actor["sources"].as_array().unwrap();
        if actor["subject"] == "ApplicationRole.operator" {
            assert_eq!(sources.len(), 2);
            assert!(sources
                .iter()
                .all(|row| row["conditions"]["scope_equality"] == "application_wide"));
            assert!(sources
                .iter()
                .any(|row| row["contract"]["principal_entity"] == "User"));
            assert!(sources
                .iter()
                .any(|row| row["contract"]["principal_entity"] == "Service"));
            continue;
        }
        let member = sources
            .iter()
            .find(|row| row["contract_id"] == "membership:CompanyMembership")
            .unwrap();
        assert_eq!(member["contract"]["principal_entity"], "User");
        assert_eq!(member["contract"]["scope_field"], "company_id");
        assert_eq!(
            member["conditions"]["scope_equality"],
            "membership_scope_key_equals_protected_scope_key"
        );
        assert_eq!(member["conditions"]["current_row"], "not_proved");
        assert_eq!(member["principal_candidates"], serde_json::json!([]));
        if actor["subject"] == "CompanyRole.owner" {
            assert_eq!(sources.len(), 2);
            let direct = sources
                .iter()
                .find(|row| row["contract_id"] == "role_binding:Company.owner_id")
                .unwrap();
            assert_eq!(direct["scope_identity_field"], "id");
            assert_eq!(direct["contract"]["principal_entity"], "User");
            assert_eq!(
                direct["conditions"]["principal_equality"],
                "validated_principal_identity_equals_binding_field"
            );
        } else {
            assert_eq!(
                sources.len(),
                1,
                "a different role cannot borrow the owner binding"
            );
        }
        assert!(!sources
            .iter()
            .any(|row| row["contract_id"] == "membership:ApplicationMembership"));
    }
    assert_eq!(route["operation_composition"], "requirements_and");
    assert_eq!(route["live_authority"], "not_proved");
    // A single role enum with conflicting scope contracts is not checked source.
    let invalid = format!(
        "{source}\n{}",
        r#"
entity GlobalCompanyMembership {
    id: Uuid identity
    service_id: Service.id
    role: CompanyRole
    membership { scope: application member: service_id role: role }
}
"#
    );
    let project = analyzed("/invalid", &invalid);
    assert!(project
        .policy
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "POLICY_BINDING_INVALID"));
    assert!(
        derive_approval_subject(Path::new("/invalid/app.jadpo"), &project, None, None, None)
            .is_err()
    );
}

fn principal_admin_suffix() -> &'static str {
    r#"
enum ApplicationRole { operator }
entity ApplicationMembership {
    id: Uuid identity
    user_id: User.id
    role: ApplicationRole
    membership { scope: application member: user_id role: role }
}
entity ApplicationServiceMembership {
    id: Uuid identity
    service_id: Service.id
    role: ApplicationRole
    membership { scope: application member: service_id role: role }
}
output AdminHealth { ok: Bool }
action admin_health() -> AdminHealth {
    policy { ApplicationRole.operator: [invoke] }
    return AdminHealth { ok: true }
}
route GET /identity-admin { output: AdminHealth run: admin_health() }
"#
}

fn admin_membership_candidates(reviewed: &Value, membership: &str) -> Value {
    let route = route_sources(reviewed, "GET /identity-admin");
    let operation = route["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["operation"] == "admin_health")
        .unwrap();
    operation["contract"]["surfaces"][0]["actors"][0]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["contract_id"] == membership)
        .unwrap()["principal_candidates"]
        .clone()
}

#[test]
fn actor_identity_candidates_distinguish_nominal_plain_uuid_and_resolution_identity() {
    let base = principal_source();
    let base = base.split("output PrincipalIdentity").next().unwrap();
    let prefix = base.split("authentication browser_session").next().unwrap();
    let plain = subject(&format!("{prefix}{}", principal_admin_suffix()), None);
    assert_eq!(
        admin_membership_candidates(&plain, "membership:ApplicationMembership"),
        serde_json::json!([])
    );
    assert_eq!(
        admin_membership_candidates(&plain, "membership:ApplicationServiceMembership"),
        serde_json::json!([])
    );
    let nominal = prefix
        .replace("user_id: Uuid", "actor_key: User.id")
        .replace("service_id: Uuid", "agent_key: Service.id");
    let nominal = subject(&format!("{nominal}{}", principal_admin_suffix()), None);
    for (membership, target, kind) in [
        (
            "membership:ApplicationMembership",
            "Principal.user.actor_key",
            "user",
        ),
        (
            "membership:ApplicationServiceMembership",
            "Principal.service.agent_key",
            "service",
        ),
    ] {
        let candidates = admin_membership_candidates(&nominal, membership);
        assert_eq!(candidates.as_array().unwrap().len(), 1);
        assert_eq!(candidates[0]["kind"], "nominal_identity_field");
        assert_eq!(candidates[0]["target"], target);
        assert_eq!(candidates[0]["principal_kind"], kind);
        assert_eq!(
            candidates[0]["identity_composition"],
            "explicit_nominal_identity_reference"
        );
    }
    let renamed_identity = prefix
        .replace("id: Uuid identity", "key: Uuid identity")
        .replace("user_id: Uuid", "actor_key: User.key")
        .replace("service_id: Uuid", "agent_key: Service.key");
    let renamed_suffix = principal_admin_suffix()
        .replace("User.id", "User.key")
        .replace("Service.id", "Service.key");
    let renamed = subject(&format!("{renamed_identity}{renamed_suffix}"), None);
    for membership in [
        "membership:ApplicationMembership",
        "membership:ApplicationServiceMembership",
    ] {
        let candidates = admin_membership_candidates(&renamed, membership);
        assert_eq!(candidates.as_array().unwrap().len(), 1);
        assert_eq!(candidates[0]["identity_field"], "key");
        assert_eq!(candidates[0]["kind"], "nominal_identity_field");
    }
    let resolved = format!("{base}{}", principal_admin_suffix());
    let original = subject(&resolved, None);
    let candidates = admin_membership_candidates(&original, "membership:ApplicationMembership");
    assert!(candidates
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["kind"] == "authority_resolution"));
    assert!(candidates
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["source_field"] == "id"
            && row["identity_composition"] == "explicit_identity_mapping"));
    assert!(candidates
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["source_field"] == "authentication_subject"
            && row["identity_composition"] == "not_present"));
    let changed = resolved
        .replace(
            "    enabled: Bool\n",
            "    enabled: Bool\n    alternate_id: Uuid\n",
        )
        .replace(
            "            id -> Principal.user.user_id",
            "            alternate_id -> Principal.user.user_id",
        );
    let changed = subject(&changed, Some(&resolved));
    let candidates = admin_membership_candidates(&changed, "membership:ApplicationMembership");
    assert!(candidates
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["source_field"] == "alternate_id"
            && row["identity_composition"] == "not_present"));
    assert!(!candidates
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["identity_composition"] == "explicit_identity_mapping"));
    assert!(changed["canonical"]["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["route"] == "GET /identity-admin"
            && row["kind"] == "route_admission"
            && row["status"] == "route_actor_constraints_changed_admission_contract"));
}

#[test]
fn authentication_projection_changes_bind_effectless_entry_without_inventing_validator_selection() {
    let source = format!(
        "{}\n{}",
        principal_source(),
        r#"
output SimpleHealth { ok: Bool }
action simple_health() -> SimpleHealth { return SimpleHealth { ok: true } }
route GET /protected-health { output: SimpleHealth run: simple_health() }
route GET /open-health { auth: none output: SimpleHealth run: simple_health() }
"#
    );
    let changed = source.replace("active: enabled == true", "active: enabled == false");
    let reviewed = subject(&changed, Some(&source));
    let protected = route_sources(&reviewed, "GET /protected-health");
    assert!(!protected["entry_authentication_candidates"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(protected["route_validator_selection"], "not_resolved");
    assert_eq!(
        route_sources(&reviewed, "GET /open-health")["entry_authentication_candidates"],
        serde_json::json!([])
    );
    let rows = reviewed["canonical"]["scenarios"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["route"], "GET /protected-health");
    assert_eq!(rows[0]["kind"], "route_admission");
    assert_eq!(rows[0]["certainty"], "compiler_checked_route_contract");
    assert_eq!(rows[0]["execution"], "not_proved");
}

#[test]
fn inline_actor_route_walk_is_cycle_safe_with_deterministic_alternate_witnesses() {
    let alpha = "action alpha() -> Unit { var health = administrative_health() var next = beta() }";
    let beta = "action beta() -> Unit { var health = administrative_health() var next = alpha() }";
    let route = r#"route POST /inline-health/{id} {
        path: { id: User.id }
        success: no_content
        action: fails UserNotFound {
            var user = attempt query required User { where: id == path.id missing: UserNotFound }
            var first = beta()
            var second = alpha()
        }
    }"#;
    let source = format!("{}\n{alpha}\n{beta}\n{route}", policy_binding_source());
    let reordered = format!("{}\n{route}\n{beta}\n{alpha}", policy_binding_source());
    let reviewed = subject(&source, None);
    let route = route_sources(&reviewed, "POST /inline-health/{id}");
    let operations = route["operations"].as_array().unwrap();
    // Traverse the effectless wrappers but emit only checked policy operations.
    assert_eq!(operations.len(), 2);
    let nested = operations
        .iter()
        .find(|row| row["operation"] == "administrative_health")
        .unwrap();
    assert_eq!(
        nested["path"],
        serde_json::json!(["POST /inline-health/{id}", "alpha", "administrative_health"])
    );
    assert_eq!(nested["contract"]["surfaces"][0]["scope"], "application");
    let direct = operations
        .iter()
        .find(|row| row["operation"] == "route:POST:/inline-health/{id}")
        .unwrap();
    assert_eq!(
        direct["path"],
        serde_json::json!(["POST /inline-health/{id}"])
    );
    assert!(direct["contract"]["surfaces"]
        .as_array()
        .unwrap()
        .iter()
        .any(|surface| surface["entity"] == "User" && surface["provenance"] == "route_inline"));
    assert_eq!(
        route,
        route_sources(&subject(&reordered, None), "POST /inline-health/{id}")
    );
    assert_eq!(route["feasibility"], "not_analyzed");
    let calls = reviewed["canonical"]["after"]["graph"]["calls"]
        .as_array()
        .unwrap();
    assert!(calls
        .iter()
        .any(|edge| edge == &serde_json::json!(["alpha", "beta"])));
    assert!(calls
        .iter()
        .any(|edge| edge == &serde_json::json!(["beta", "alpha"])));
}

#[test]
fn copied_route_actor_groups_remain_distinct_conjunctive_requirements() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let base =
        std::fs::read_to_string(root.join("tests/compile/pass/134_policy_safe_projection.jadpo"))
            .unwrap();
    let route = |calls: &str| {
        format!(
            r#"
route GET /both/{{id}} {{
    path: {{ id: PrivateNote.id }} success: no_content
    action: fails NoteNotFound {{ {calls} }}
}}
"#
        )
    };
    let summary = "var summary = attempt PrivateNote.summary(path.id)";
    let label = "var label = attempt PrivateNote.read_private_label(path.id)";
    let source = format!("{base}{}", route(&format!("{summary}\n{label}")));
    let expected = subject(&source, None);
    let sources = route_sources(&expected, "GET /both/{id}");
    let operations = sources["operations"].as_array().unwrap();
    let copied = operations
        .iter()
        .find(|row| row["operation"] == "route:GET:/both/{id}")
        .unwrap();
    assert_eq!(
        copied["contract"]["surface_composition"],
        "requirements_and"
    );
    let groups = copied["contract"]["surfaces"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|surface| {
            surface["kind"] == "operation_obligation"
                && surface["entity"] == "PrivateNote"
                && surface["effect"] == "read"
                && surface["provenance"] == "transitive_operation"
        })
        .collect::<Vec<_>>();
    assert_eq!(groups.len(), 2);
    assert_ne!(groups[0]["id"], groups[1]["id"]);
    assert!(groups.iter().any(|surface| surface["subject_group"]
        == serde_json::json!(["Access.authenticated", "NoteRole.owner"])));
    assert!(groups
        .iter()
        .any(|surface| surface["subject_group"] == serde_json::json!(["NoteRole.owner"])));
    for operation in ["PrivateNote.summary", "PrivateNote.read_private_label"] {
        let original = operations
            .iter()
            .find(|row| row["operation"] == operation)
            .unwrap();
        assert_eq!(
            original["path"],
            serde_json::json!(["GET /both/{id}", operation])
        );
        assert!(original["contract"]["surfaces"]
            .as_array()
            .unwrap()
            .iter()
            .any(|surface| surface["surface_origin"] == "checked_operation_surface"));
    }
    let reordered = format!("{base}{}", route(&format!("{label}\n{summary}")));
    assert_eq!(
        sources,
        route_sources(&subject(&reordered, None), "GET /both/{id}")
    );
    let narrowed = source.replace("        Access.authenticated: [read]\n", "");
    let delta = subject(&narrowed, Some(&source));
    assert_ne!(
        &delta["canonical"]["before"]["impact"]["actor_sources"]["routes"]["GET /both/{id}"],
        route_sources(&delta, "GET /both/{id}")
    );
    let mut omitted = expected.clone();
    let route =
        &mut omitted["canonical"]["after"]["impact"]["actor_sources"]["routes"]["GET /both/{id}"];
    let copied = route["operations"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["operation"] == "route:GET:/both/{id}")
        .unwrap();
    let surfaces = copied["contract"]["surfaces"].as_array_mut().unwrap();
    let index = surfaces
        .iter()
        .position(|surface| {
            surface["subject_group"]
                == serde_json::json!(["Access.authenticated", "NoteRole.owner"])
        })
        .unwrap();
    surfaces.remove(index);
    omitted["subject_digest"] = serde_json::json!(format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&omitted["canonical"]).unwrap())
    ));
    assert!(jadpo_core::validate_approval_export(
        &serde_json::to_string(&expected).unwrap(),
        &serde_json::to_string(&omitted).unwrap()
    )
    .is_err());
}

fn source_candidate(
    reviewed: &Value,
    route: &str,
    operation: &str,
    subject: &str,
    contract: &str,
) -> Value {
    let route = route_sources(reviewed, route);
    let operation = route["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["operation"] == operation)
        .unwrap();
    operation["contract"]["surfaces"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|surface| surface["actors"].as_array().unwrap())
        .find(|actor| actor["subject"] == subject)
        .unwrap()["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["contract_id"] == contract)
        .unwrap()
        .clone()
}

#[test]
fn actor_source_references_bind_same_root_nonidentity_and_scope_changes_without_identity_proof() {
    let source = format!(
        "{}\n{}",
        policy_binding_source(),
        r#"
failure CompanyMissing { kind: NotFound code: "company_missing" }
action read_company(id: Company.id) fails CompanyMissing -> Unit {
    var company = attempt query required Company { where: id == id missing: CompanyMissing }
}
route GET /company/{id} { path: { id: Company.id } success: no_content run: read_company(path.id) }
action wrapper_health() -> Health { return administrative_health() }
route GET /admin-health { output: Health run: wrapper_health() }
"#
    )
    .replace(
        "    id: Uuid identity\n",
        "    id: Uuid identity\n    alternate_id: Uuid\n",
    );
    let original = subject(&source, None);
    let direct = source_candidate(
        &original,
        "GET /company/{id}",
        "read_company",
        "CompanyRole.owner",
        "role_binding:Company.owner_id",
    );
    assert_eq!(direct["source_value_reference"]["reference"], "User.id");
    assert_eq!(
        direct["source_value_reference_composition"],
        "checked_identity_reference"
    );
    assert_eq!(direct["conditions"]["status"], "required_not_proved");
    for (target, expected_tokens) in [
        (
            "User.alternate_id",
            serde_json::json!(["User", ".", "alternate_id"]),
        ),
        (
            "User.display_name",
            serde_json::json!(["User", ".", "display_name"]),
        ),
    ] {
        let changed = source.replace("owner_id: User.id", &format!("owner_id: {target}"));
        let reviewed = subject(&changed, Some(&source));
        let changed = source_candidate(
            &reviewed,
            "GET /company/{id}",
            "read_company",
            "CompanyRole.owner",
            "role_binding:Company.owner_id",
        );
        assert_eq!(
            changed["contract"], direct["contract"],
            "root-only policy source is unchanged"
        );
        assert_eq!(changed["source_value_reference"]["reference"], target);
        assert_eq!(
            changed["source_value_reference"]["declared_type_tokens"],
            expected_tokens
        );
        assert_eq!(
            changed["source_value_reference_composition"],
            "not_identity_reference"
        );
        assert_ne!(
            route_sources(&original, "GET /company/{id}"),
            route_sources(&reviewed, "GET /company/{id}")
        );
        assert!(reviewed["canonical"]["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |row| row["subject"].as_str().unwrap().starts_with("actor_link:")
                    && row["change"] == "changed"
            ));
    }
    for target in ["User.alternate_id", "User.display_name"] {
        let changed = source.replace(
            "    user_id: User.id\n",
            &format!("    user_id: {target}\n"),
        );
        let reviewed = subject(&changed, Some(&source));
        let member = source_candidate(
            &reviewed,
            "GET /admin-health",
            "administrative_health",
            "ApplicationRole.operator",
            "membership:ApplicationMembership",
        );
        assert_eq!(member["source_value_reference"]["reference"], target);
        assert_eq!(
            member["source_value_reference_composition"],
            "not_identity_reference"
        );
        assert!(member["scope_value_reference"].is_null());
        assert_eq!(
            member["scope_reference_requirement"],
            "application_wide_no_value_reference"
        );
        assert!(reviewed["canonical"]["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["route"] == "GET /admin-health"
                && row["status"] == "route_actor_constraints_changed_admission_contract"));
    }
    let no_identity = source.replace("id: Uuid identity", "id: Uuid");
    let reviewed = subject(&no_identity, Some(&source));
    let direct = source_candidate(
        &reviewed,
        "GET /company/{id}",
        "read_company",
        "CompanyRole.owner",
        "role_binding:Company.owner_id",
    );
    assert!(direct["source_value_reference"]["expected_identity_reference"].is_null());
    assert_eq!(direct["source_value_reference_composition"], "unresolved");
    assert_eq!(direct["source_value_reference"]["reference"], "User.id");
    let changed = source.replace(
        "    company_id: Company.id\n",
        "    company_id: Company.alternate_id\n",
    );
    let reviewed = subject(&changed, Some(&source));
    let member = source_candidate(
        &reviewed,
        "GET /company/{id}",
        "read_company",
        "CompanyRole.owner",
        "membership:CompanyMembership",
    );
    assert_eq!(
        member["scope_value_reference"]["reference"],
        "Company.alternate_id"
    );
    assert_eq!(
        member["scope_value_reference"]["composition"],
        "not_identity_reference"
    );
    assert_ne!(
        route_sources(&original, "GET /company/{id}"),
        route_sources(&reviewed, "GET /company/{id}")
    );
}

#[test]
fn actor_source_reference_preserves_explicit_target_precedence_and_direct_self_identity() {
    let explicit = admin_route_source().replace(
        "    user_id: User.id\n",
        "    user_id: User.id references User.id on_delete restrict\n",
    );
    let reviewed = subject(&explicit, None);
    let member = source_candidate(
        &reviewed,
        "GET /admin-health",
        "administrative_health",
        "ApplicationRole.operator",
        "membership:ApplicationMembership",
    );
    assert_eq!(
        member["source_value_reference"]["declared_type_path"],
        "User.id"
    );
    assert_eq!(member["source_value_reference"]["reference"], "User.id");
    assert_eq!(
        member["source_value_reference"]["reference_origin"],
        "explicit_reference"
    );
    assert_eq!(
        member["source_value_reference_composition"],
        "checked_identity_reference"
    );
    let mismatched = explicit.replace(
        "user_id: User.id references User.id",
        "user_id: Uuid references User.id",
    );
    let project = analyzed("/invalid", &mismatched);
    assert!(project
        .semantics
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "DATA_RELATIONSHIP_TYPE_MISMATCH"));
    assert!(
        derive_approval_subject(Path::new("/invalid/app.jadpo"), &project, None, None, None)
            .is_err()
    );
    let self_identity = r#"
enum PersonRole { self }
failure PersonMissing { kind: NotFound code: "person_missing" }
output PersonKey { key: Person.key }
entity Person {
    key: Uuid { role: PersonRole.self immutable: true }
    identity: key
    persistence { store: primary role: authority }
    policy { PersonRole.self: [read] }
    query own(key: Person.key) freshness: authoritative fails PersonMissing -> PersonKey {
        var person = attempt query required Person { where: key == key missing: PersonMissing }
        return PersonKey { key: person.key }
    }
}
route GET /person/{key} { path: { key: Person.key } output: PersonKey run: Person.own(path.key) }
"#;
    let reviewed = subject(self_identity, None);
    let direct = source_candidate(
        &reviewed,
        "GET /person/{key}",
        "Person.own",
        "PersonRole.self",
        "role_binding:Person.key",
    );
    assert_eq!(
        direct["source_value_reference"]["reference_origin"],
        "direct_self_identity"
    );
    assert_eq!(direct["source_value_reference"]["reference"], "Person.key");
    assert_eq!(
        direct["source_value_reference_composition"],
        "checked_identity_reference"
    );
    assert_eq!(direct["scope_identity_field"], "key");
}

#[test]
fn checked_principal_contracts_expose_exact_fields_claims_and_resolution_not_live_authority() {
    let absent = subject(SOURCE, None);
    let contracts = &absent["canonical"]["after"]["impact"]["actors"]["principal_contracts"];
    for category in [
        "applications",
        "variants",
        "transports",
        "validators",
        "claims",
        "resolutions",
    ] {
        assert_eq!(contracts[category], serde_json::json!([]));
    }
    let reviewed = subject(&principal_source(), None);
    let facts = &reviewed["canonical"]["after"]["facts"];
    assert_eq!(
        facts["application_auth:TodoApplication"]["principal"],
        "Principal"
    );
    assert_eq!(
        facts["application_auth:TodoApplication"]["revocation"],
        serde_json::json!({"mode":"bounded","maximum_delay":"5m"})
    );
    assert_eq!(facts["principal_variant:Principal.user"]["kind"], "user");
    assert_eq!(
        facts["principal_variant:Principal.service"]["fields"]["service_id"]["type"],
        "Uuid"
    );
    assert_eq!(
        facts["auth_validator:api_bearer.service_key"]["principal_variant"],
        "service"
    );
    assert_eq!(
        facts["auth_claim:browser_session:Principal.user.subject"],
        serde_json::json!({"strategy":"browser_session","source":"subject","target":"Principal.user.subject"})
    );
    assert_eq!(
        facts["auth_resolution:browser_session.user"],
        serde_json::json!({
            "strategy":"browser_session","principal_variant":"user","authority":"User.authentication_subject",
            "active_tokens":["enabled","==","true"],"inactive_failure":"PrincipalInactive",
            "mappings":{"Principal.user.subject":"authentication_subject","Principal.user.user_id":"id"}
        })
    );
    assert_eq!(
        facts["auth_projection:browser_session.user:Principal.user.user_id"]["source_field"],
        "id"
    );
    let contracts = &reviewed["canonical"]["after"]["impact"]["actors"]["principal_contracts"];
    assert_eq!(
        contracts["status"],
        "compiler_checked_contract_not_live_authority"
    );
    assert_eq!(
        reviewed["canonical"]["after"]["impact"]["authentication"]["status"],
        "opaque_generated_boundary"
    );
    assert_eq!(reviewed["canonical"]["release_authority"], false);
    assert_eq!(
        reviewed["canonical"]["evidence"]["runtime_validated"],
        serde_json::json!([])
    );
}

#[test]
fn principal_projection_active_and_declared_setting_changes_have_exact_decisions() {
    let original = principal_source().replace(
        "    enabled: Bool\n",
        "    enabled: Bool\n    alternate_subject: Text\n",
    );
    for (changed, identity, field, expected) in [
        (
            original.replace(
                "authentication_subject -> Principal.user.subject",
                "alternate_subject -> Principal.user.subject",
            ),
            "auth_projection:browser_session.user:Principal.user.subject",
            "source_field",
            serde_json::json!("alternate_subject"),
        ),
        (
            original.replace("active: enabled == true", "active: enabled == false"),
            "auth_resolution:browser_session.user",
            "active_tokens",
            serde_json::json!(["enabled", "==", "false"]),
        ),
        (
            original.replace("maximum_delay: 5m", "maximum_delay: 4m"),
            "application_auth:TodoApplication",
            "revocation",
            serde_json::json!({"mode":"bounded","maximum_delay":"4m"}),
        ),
        (
            original.replace(
                "mode: signed\n            principal: user\n",
                "mode: signed\n            principal: user\n            audience: \"reviewed-audience\"\n",
            ),
            "auth_validator:browser_session.signed_session",
            "settings",
            serde_json::json!({"audience":["\"reviewed-audience\""]}),
        ),
    ] {
        let reviewed = subject(&changed, Some(&original));
        let canonical = &reviewed["canonical"];
        let decision = canonical["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["subject"] == identity)
            .unwrap();
        assert_eq!(decision["change"], "changed");
        assert_eq!(decision["after"][field], expected);
        assert!(decision["approval"].is_null());
        assert_eq!(
            canonical["before"]["facts"]["semantic_graph"]["calls"],
            canonical["after"]["facts"]["semantic_graph"]["calls"]
        );
        assert_ne!(
            canonical["before"]["behavior_digest"],
            canonical["after"]["behavior_digest"]
        );
    }
    let removed = subject(
        &original.replace("        subject -> Principal.user.subject\n", ""),
        Some(&original),
    );
    assert!(removed["canonical"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |row| row["subject"] == "auth_claim:browser_session:Principal.user.subject"
                && row["change"] == "removed"
        ));
}

#[test]
fn invalid_principal_projection_cannot_export_checked_facts_and_rehashed_omissions_reject() {
    let original = principal_source();
    for changed in [
        original.replace("active: enabled == true", "active: 42"),
        original.replace(
            "id -> Principal.user.user_id",
            "enabled -> Principal.user.user_id",
        ),
        original.replace(
            "id -> Principal.user.user_id",
            "id -> Principal.user.user_id\n            id -> Principal.user.user_id",
        ),
        original.replace(
            "authentication_subject -> Principal.user.subject",
            "missing_field -> Principal.user.subject",
        ),
    ] {
        let analyzed = analyzed("/invalid", &changed);
        assert!(derive_approval_subject(
            Path::new("/invalid/app.jadpo"),
            &analyzed,
            None,
            None,
            None
        )
        .is_err());
    }
    let expected = subject(&original, None);
    let raw = serde_json::to_string(&expected).unwrap();
    for category in ["variants", "claims", "resolutions", "validators"] {
        let mut exported = expected.clone();
        exported["canonical"]["after"]["impact"]["actors"]["principal_contracts"][category] =
            serde_json::json!([]);
        exported["subject_digest"] = serde_json::json!(format!(
            "sha256:{:x}",
            Sha256::digest(
                serde_json::to_string(&exported["canonical"])
                    .unwrap()
                    .as_bytes()
            )
        ));
        assert!(jadpo_core::validate_approval_export(
            &raw,
            &serde_json::to_string(&exported).unwrap()
        )
        .is_err());
    }
    let rebuilt = subject(
        &original.replace("active: enabled == true", "active: enabled  ==  true"),
        None,
    );
    assert_eq!(
        expected["canonical"]["after"]["impact"]["actors"]["principal_contracts"],
        rebuilt["canonical"]["after"]["impact"]["actors"]["principal_contracts"]
    );
}

#[test]
fn principal_contracts_preserve_checked_credential_slots_and_semantic_field_order() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let path = root.join("examples/golden-todo-migration");
    let project = analyze_project(&path).unwrap();
    let raw = derive_approval_subject(&path, &project, None, None, None).unwrap();
    let reviewed: Value = serde_json::from_str(&raw).unwrap();
    let validator =
        &reviewed["canonical"]["after"]["facts"]["auth_validator:api_bearer.service_key"];
    assert_eq!(
        validator["settings"]["secret"],
        serde_json::json!(["config", ".", "session_signing_key"])
    );
    assert_eq!(
        validator["settings"]["owner"],
        serde_json::json!(["Service", ".", "owner_id"])
    );
    assert_eq!(
        validator["credentials"],
        serde_json::json!({
            "identity":"ServiceCredential.id","principal":"ServiceCredential.service_id",
            "verifier":"ServiceCredential.verifier","active_tokens":["status","==","CredentialStatus",".","active"],
            "expires":"ServiceCredential.expires_at","revoked":"ServiceCredential.revoked_at"
        })
    );
    let source = principal_source();
    let reordered = source.replace(
        "        subject: Text\n        user_id: Uuid",
        "        user_id: Uuid\n        subject: Text",
    );
    assert_ne!(source, reordered);
    let before = subject(&source, None);
    let after = subject(&reordered, None);
    assert_eq!(
        before["canonical"]["after"]["impact"]["actors"]["principal_contracts"],
        after["canonical"]["after"]["impact"]["actors"]["principal_contracts"]
    );
    assert!(before["canonical"]["after"]["facts"]["semantic_graph"]["calls"].is_array());
}

#[test]
fn actor_bindings_expose_checked_source_contracts_and_explicit_absence_not_live_roles() {
    let absent = subject(SOURCE, None);
    let inventory = &absent["canonical"]["after"]["impact"]["actors"];
    assert_eq!(inventory["role_bindings"], serde_json::json!([]));
    assert_eq!(inventory["memberships"], serde_json::json!([]));
    assert_eq!(inventory["status"], "compiler_checked_inventory");
    assert!(inventory["facts"].is_array());
    let reviewed = subject(&policy_binding_source(), None);
    let actors = &reviewed["canonical"]["after"]["impact"]["actors"];
    assert_eq!(
        actors["binding_evidence"],
        "compiler_checked_contract_not_live_authority"
    );
    assert_eq!(
        actors["role_bindings"][0],
        serde_json::json!({
            "entity":"Company","field":"owner_id","role":"CompanyRole.owner",
            "scope":"Company","principal_entity":"User"
        })
    );
    let memberships = actors["memberships"].as_array().unwrap();
    assert!(memberships
        .iter()
        .any(|row| row["entity"] == "CompanyMembership"
            && row["scope"] == "Company"
            && row["scope_field"] == "company_id"
            && row["principal_entity"] == "User"
            && row["member_field"] == "user_id"
            && row["role_field"] == "role"
            && row["role_type"] == "CompanyRole"));
    assert!(memberships.iter().any(|row| row["scope"] == "application"
        && row["scope_field"].is_null()
        && row["principal_entity"] == "Service"));
    assert_eq!(reviewed["canonical"]["release_authority"], false);
    assert_eq!(
        reviewed["canonical"]["evidence"]["runtime_validated"],
        serde_json::json!([])
    );
}

#[test]
fn role_binding_change_and_removal_have_individual_exact_decisions() {
    // Retain a second valid direct scope binding while removing the selected
    // one. Removing the sole scope source must remain a compiler rejection.
    let original = policy_binding_source().replace("entity Company {\n", "entity Company {\n    backup_owner_id: User.id { role: CompanyRole.owner immutable: true }\n");
    for replacement in ["        role: CompanyRole.viewer\n", ""] {
        let changed = original.replace("        role: CompanyRole.owner\n", replacement);
        assert_ne!(changed, original);
        let reviewed = subject(&changed, Some(&original));
        let canonical = &reviewed["canonical"];
        let decision = canonical["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["subject"] == "role_binding:Company.owner_id")
            .expect("direct binding needs an individual human decision");
        assert!(decision["approval"].is_null());
        assert_ne!(
            canonical["before"]["behavior_digest"],
            canonical["after"]["behavior_digest"]
        );
        assert!(canonical["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["subject"] == "impact:actors"));
        assert_eq!(
            canonical["before"]["facts"]["role_binding:Company.owner_id"]["role"],
            "CompanyRole.owner"
        );
        if replacement.is_empty() {
            assert_eq!(decision["change"], "removed");
            assert!(canonical["after"]["facts"]
                .get("role_binding:Company.owner_id")
                .is_none());
        } else {
            assert_eq!(decision["change"], "changed");
            assert_eq!(
                canonical["after"]["facts"]["role_binding:Company.owner_id"]["role"],
                "CompanyRole.viewer"
            );
        }
    }
}

#[test]
fn membership_contract_change_and_rehashed_binding_omissions_fail_export_conformance() {
    let source = policy_binding_source();
    let changed = source
        .replace("member: user_id", "member: alternate_user_id")
        .replace(
            "    user_id: User.id\n",
            "    user_id: User.id\n    alternate_user_id: User.id\n",
        );
    let reviewed = subject(&changed, Some(&source));
    assert!(reviewed["canonical"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["subject"] == "membership:CompanyMembership" && row["change"] == "changed"));
    assert_eq!(
        reviewed["canonical"]["after"]["facts"]["membership:CompanyMembership"]["member_field"],
        "alternate_user_id"
    );
    let expected = serde_json::to_string(&reviewed).unwrap();
    for omitted in ["role_bindings", "memberships"] {
        let mut exported = reviewed.clone();
        exported["canonical"]["after"]["impact"]["actors"][omitted] = serde_json::json!([]);
        let canonical = serde_json::to_string(&exported["canonical"]).unwrap();
        exported["subject_digest"] =
            serde_json::json!(format!("sha256:{:x}", Sha256::digest(canonical.as_bytes())));
        assert!(jadpo_core::validate_approval_export(
            &expected,
            &serde_json::to_string(&exported).unwrap()
        )
        .is_err());
    }
}

#[test]
fn checked_job_impact_binds_exact_nonexecuting_interval_and_absence() {
    let without_job = JOB_SOURCE.split("job overdue_reminders").next().unwrap();
    let reviewed = subject(JOB_SOURCE, Some(without_job));
    let canonical = &reviewed["canonical"];
    let impact = &canonical["after"]["impact"]["jobs"];
    assert_eq!(impact["status"], "compiler_checked_nonexecuting_contract");
    assert_eq!(impact["runtime_conformance"], "not_established");
    assert_eq!(impact["delivery_authority"], "not_resolved");
    assert_eq!(impact["feasible_job_scenarios"], "not_generated");
    assert_eq!(
        canonical["before"]["impact"]["jobs"]["facts"]["jobs"],
        serde_json::json!([])
    );
    let job = &impact["facts"]["jobs"][0];
    assert_eq!(job["job"], "overdue_reminders");
    assert_eq!(job["interval_ms"], 900_000);
    assert_eq!(job["run"], "scan");
    assert_eq!(job["argument"]["constructor"], "JobRunAt");
    assert_eq!(job["argument"]["value"], "clock.now");
    assert!(job["execution_profile"].is_null());
    assert_eq!(job["runtime_lowering_supported"], false);
    assert_eq!(
        job["durable_failure_dispositions"],
        "pending_checked_worker_binding"
    );
    assert_eq!(
        job["effect_evidence"],
        "static_may_call_not_execution_or_authority"
    );
    assert_eq!(
        canonical["after"]["impact"]["emissions"]["status"],
        "unsupported_analysis"
    );
    assert!(canonical["after"]["impact"]["emissions"]["facts"].is_null());
    assert_eq!(canonical["release_authority"], false);
    assert_eq!(
        canonical["evidence"]["runtime_validated"],
        serde_json::json!([])
    );
    assert!(canonical["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|decision| {
            decision["subject"] == "impact:jobs" && decision["approval"].is_null()
        }));
}

#[test]
fn job_contract_changes_cannot_hide_behind_unchanged_call_graph() {
    for (source, field, expected) in [
        (JOB_SOURCE.replace("15m", "30m"), "interval_ms", serde_json::json!(1_800_000)),
        (JOB_SOURCE.replace("run: scan", "run: other_scan"), "run", serde_json::json!("other_scan")),
        (
            format!("failure ScanUnavailable {{ kind: Unavailable code: \"scan_unavailable\" }}\n{}", JOB_SOURCE.replace("action scan(at: JobRunAt) -> Unit {}", "action scan(at: JobRunAt) fails ScanUnavailable -> Unit { reject ScanUnavailable }")),
            "failures", serde_json::json!(["ScanUnavailable"]),
        ),
    ] {
        let reviewed = subject(&source, Some(JOB_SOURCE));
        let canonical = &reviewed["canonical"];
        assert_eq!(canonical["after"]["impact"]["jobs"]["facts"]["jobs"][0][field], expected);
        assert_ne!(canonical["before"]["behavior_digest"], canonical["after"]["behavior_digest"]);
        assert!(canonical["decisions"].as_array().unwrap().iter().any(|decision| {
            decision["subject"] == "impact:jobs" && decision["change"] == "changed"
        }));
        if field == "interval_ms" {
            assert_eq!(canonical["before"]["semantic_graph_digest"], canonical["after"]["semantic_graph_digest"]);
        }
    }
}

#[test]
fn job_impact_is_sorted_by_semantic_identity_not_source_order() {
    let declarations = JOB_SOURCE.split("job overdue_reminders").next().unwrap();
    let first = JOB_SOURCE[declarations.len()..].replace("overdue_reminders", "alpha_job");
    let second = JOB_SOURCE[declarations.len()..].replace("overdue_reminders", "zeta_job");
    let a = subject(&format!("{declarations}{second}{first}"), None);
    let b = subject(&format!("{declarations}{first}{second}"), None);
    assert_eq!(
        a["canonical"]["after"]["impact"]["jobs"],
        b["canonical"]["after"]["impact"]["jobs"]
    );
    assert_eq!(
        a["canonical"]["after"]["impact"]["jobs"]["facts"]["jobs"][0]["job"],
        "alpha_job"
    );
}

#[test]
fn job_transitive_service_effect_is_bound_and_rehashed_omission_rejects() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let path = root.join("tests/compile/pass/170_checked_service_operation.jadpo");
    let fixture = std::fs::read_to_string(&path).unwrap();
    let source = format!(
        r#"{fixture}
type JobRunAt = Instant {{}}
failure JobIdentityMissing {{ kind: NotFound code: "job_identity_missing" }}
query find_job_identity()
    freshness: authoritative
    fails JobIdentityMissing
    -> ServiceFakeIdentity
{{
    return attempt query required ServiceFakeIdentity {{
        where: label == ServiceFakeIdentity.label("fixture-only")
        missing: JobIdentityMissing
    }}
}}
action scan(at: JobRunAt)
    fails JobIdentityMissing, ReminderRecipientRejected, ReminderTemporarilyUnavailable, Misconfigured, Unavailable, OutcomeUnknown
    -> Unit
{{
    var source_record = attempt find_job_identity()
    var mail = ReminderMessage {{
        idempotency_key: ReminderMessage.idempotency_key(source_record.id)
        from: ReminderMessage.from("sender@example.test")
        to: ReminderMessage.to("recipient@example.test")
        todo_title: ReminderMessage.todo_title("Job approval only")
        due_at: none
    }}
    var receipt = attempt deliver(mail)
}}
job overdue_reminders every 15m {{
    concurrency: singleton
    run: scan(JobRunAt(clock.now))
    retry: next_schedule
}}
"#
    );
    let analyzed = analyze_sources(vec![SourceFile::new(path.clone(), source)]).unwrap();
    let raw = derive_approval_subject(&path, &analyzed, None, None, None).unwrap();
    let reviewed: Value = serde_json::from_str(&raw).unwrap();
    let after = &reviewed["canonical"]["after"];
    let effect = "ReminderMail.send_overdue_reminder";
    assert_eq!(
        after["impact"]["jobs"]["facts"]["jobs"][0]["external_service_effects"],
        serde_json::json!([effect])
    );
    assert_eq!(
        after["effect_paths"]["overdue_reminders"][format!("{effect}:external_service:{effect}")]
            ["path"],
        serde_json::json!(["overdue_reminders", "scan", "deliver", effect])
    );
    let mut omitted = reviewed.clone();
    omitted["canonical"]["after"]["impact"]["jobs"]["facts"]["jobs"][0]
        ["external_service_effects"] = serde_json::json!([]);
    omitted["subject_digest"] = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&omitted["canonical"]).unwrap())
    )
    .into();
    assert!(
        jadpo_core::validate_approval_export(&raw, &serde_json::to_string(&omitted).unwrap())
            .is_err()
    );
    let mut missing = reviewed.clone();
    missing["canonical"]["after"]["impact"]
        .as_object_mut()
        .unwrap()
        .remove("jobs");
    missing["subject_digest"] = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&missing["canonical"]).unwrap())
    )
    .into();
    assert!(
        jadpo_core::validate_approval_export(&raw, &serde_json::to_string(&missing).unwrap())
            .is_err()
    );
}

#[test]
fn nested_source_read_has_a_complete_route_witness() {
    let subject = subject(SOURCE, None);
    let effects = &subject["canonical"]["after"]["effect_paths"]["POST /customer"];
    let read = &effects["leaf:read:Customer"];
    assert_eq!(read["target"], "Customer");
    assert_eq!(
        read["path"],
        serde_json::json!(["POST /customer", "outer", "middle", "leaf"])
    );
    assert_eq!(
        subject["canonical"]["after"]["impact"]["services"]["status"],
        "compiler_checked_contract"
    );
    assert_eq!(
        subject["canonical"]["after"]["impact"]["services"]["facts"]["effects"],
        serde_json::json!([])
    );
    assert!(subject["canonical"]["before"].is_null());
    assert_eq!(subject["canonical"]["release_authority"], false);
    let scenario = subject["canonical"]["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|scenario| scenario["route"] == "POST /customer")
        .expect("a route effect without a baseline should be visible");
    assert_eq!(scenario["status"], "after_only_without_baseline");
    assert_eq!(scenario["execution"], "not_proved");
    assert_eq!(scenario["counterexample"]["status"], "not_generated");
}

#[test]
fn route_auth_change_retains_actor_scope_scenario_when_effect_paths_are_unchanged() {
    let before = SOURCE.replace("auth: none", "");
    let subject = subject(SOURCE, Some(&before));
    let canonical = &subject["canonical"];
    assert_eq!(canonical["canonicalization"], "jadpo.approval-subject.v5");
    assert_eq!(subject["schema_version"], 5);
    assert_eq!(
        canonical["before"]["effect_paths"]["POST /customer"],
        canonical["after"]["effect_paths"]["POST /customer"]
    );
    let scenario = canonical["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|scenario| scenario["route"] == "POST /customer")
        .expect("route actor-scope change should remain visible without a graph change");
    assert_eq!(
        scenario["status"],
        "route_actor_constraints_changed_may_effect"
    );
    assert_eq!(
        scenario["actor_access"]["before_route_auth"],
        "authenticated_default"
    );
    assert_eq!(scenario["actor_access"]["after_route_auth"], "none");
    assert_eq!(
        scenario["actor_access"]["unauthenticated_caller_before"],
        false
    );
    assert_eq!(
        scenario["actor_access"]["unauthenticated_caller_after"],
        true
    );
    assert_eq!(
        scenario["counterexample"]["status"],
        "actor_scope_candidate_not_feasibility_proof"
    );
    assert_eq!(
        scenario["counterexample"]["facts"]["actor"],
        "unauthenticated_public_caller"
    );
    assert_eq!(scenario["feasibility"], "not_analyzed");
    assert_eq!(
        scenario["actor_policy"]["status"],
        "compiler_checked_route_policy"
    );
    let obligations = scenario["actor_policy"]["after"]["obligations"]
        .as_array()
        .expect("route policy requirements");
    assert!(obligations.iter().any(|obligation| {
        obligation["effect"] == "read"
            && obligation["actors"]
                .as_array()
                .is_some_and(|actors| actors.iter().any(|actor| actor == "Access.public"))
    }));
}

#[test]
fn route_policy_scenario_lists_exact_added_and_removed_actor_subjects() {
    let after = SOURCE.replace("auth: none", "");
    let before = after.replace("Access.public: [read]", "Access.authenticated: [read]");
    let subject = subject(&after, Some(&before));
    let scenario = subject["canonical"]["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|scenario| scenario["route"] == "POST /customer")
        .expect("policy subject change should produce a route scenario");
    let changes = &scenario["actor_policy"]["subject_changes"];
    assert_eq!(
        changes["status"],
        "compiler_checked_subject_delta_not_principal_or_feasibility_proof"
    );
    assert!(changes["added"].as_array().unwrap().iter().any(|change| {
        change["actor_subject"] == "Access.public"
            && change["surface"]["kind"] == "operation_obligation"
            && change["surface"]["entity"] == "Customer"
            && change["surface"]["effect"] == "read"
    }));
    assert!(changes["removed"].as_array().unwrap().iter().any(|change| {
        change["actor_subject"] == "Access.authenticated"
            && change["surface"]["kind"] == "operation_obligation"
            && change["surface"]["entity"] == "Customer"
            && change["surface"]["effect"] == "read"
    }));
    assert_eq!(scenario["feasibility"], "not_analyzed");
    assert_eq!(scenario["execution"], "not_proved");
}

#[test]
fn nested_checked_service_effect_is_bound_to_route_impact_and_cannot_be_omitted() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let path = root.join("tests/compile/pass/170_checked_service_operation.jadpo");
    let original = std::fs::read_to_string(&path).unwrap();
    let mut source = original.clone();
    source.push_str(
        r#"
action send_via_nested(mail: ReminderMessage)
    fails ReminderRecipientRejected, ReminderTemporarilyUnavailable, Misconfigured, Unavailable, OutcomeUnknown
    -> ReminderReceipt
{
    return attempt deliver(mail)
}
route POST /send-mail {
    auth: none
    input: ReminderMessage
    output: ReminderReceipt
    run: send_via_nested(input)
}
"#,
    );
    let baseline = analyze_sources(vec![SourceFile::new(path.clone(), original)]).unwrap();
    let project = analyze_sources(vec![SourceFile::new(path.clone(), source)]).unwrap();
    let raw = derive_approval_subject(
        &path,
        &project,
        Some((&path, &baseline)),
        Some("Review mail delivery"),
        None,
    )
    .expect("checked service source should produce a review subject");
    let subject: Value = serde_json::from_str(&raw).unwrap();
    let canonical = &subject["canonical"];
    let after = &canonical["after"];
    let service_effect =
        "ReminderMail.send_overdue_reminder:external_service:ReminderMail.send_overdue_reminder";

    assert_eq!(
        after["impact"]["services"]["status"],
        "compiler_checked_contract"
    );
    assert_eq!(
        after["impact"]["services"]["facts"]["runtime_conformance"],
        "not_established"
    );
    assert_eq!(
        after["effect_paths"]["POST /send-mail"][service_effect]["path"],
        serde_json::json!([
            "POST /send-mail",
            "send_via_nested",
            "deliver",
            "ReminderMail.send_overdue_reminder"
        ])
    );
    assert!(
        after["impact"]["external_effects"]["facts"]["POST /send-mail"]
            .get(service_effect)
            .is_some()
    );
    let scenario = canonical["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|scenario| scenario["route"] == "POST /send-mail")
        .expect("a newly reachable route effect should produce a scenario");
    assert_eq!(scenario["status"], "newly_reachable_may_effect");
    assert_eq!(scenario["certainty"], "compiler_checked_may_call");
    assert_eq!(scenario["execution"], "not_proved");
    assert_eq!(scenario["feasibility"], "not_analyzed");
    assert_eq!(
        scenario["effect"]["path"],
        serde_json::json!([
            "POST /send-mail",
            "send_via_nested",
            "deliver",
            "ReminderMail.send_overdue_reminder"
        ])
    );

    let expected = serde_json::to_string(&subject).unwrap();
    let mut omitted = subject.clone();
    omitted["canonical"]["after"]["impact"]["external_effects"]["facts"]
        .as_object_mut()
        .unwrap()
        .remove("POST /send-mail");
    omitted["subject_digest"] = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&omitted["canonical"]).unwrap())
    )
    .into();
    assert!(jadpo_core::validate_approval_export(
        &expected,
        &serde_json::to_string(&omitted).unwrap()
    )
    .is_err());
}

#[test]
fn lifecycle_impact_exposes_distinct_policy_nodes_generated_fields_and_logical_effect() {
    let source = r#"
enum TodoStatus { active deleted }
failure TodoMissing { kind: NotFound code: "todo_missing" }
failure TodoConflict { kind: Conflict code: "todo_conflict" }
entity Todo {
    id: Uuid { generated: identity }
    status: TodoStatus
    deleted_at: Instant?
    updated_at: Instant generated { on: create_or_change }
    identity: id
    persistence { store: primary role: authority }
    policy { Access.authenticated: [create, read, update, delete] }
    lifecycle {
        initial: {
            status: Todo.status(TodoStatus.active)
            deleted_at: none
        }
        visible when status == TodoStatus.active
        transition delete {
            from: status == TodoStatus.active
            set: {
                status: Todo.status(TodoStatus.deleted)
                deleted_at: clock.now
            }
        }
    }
    action delete_todo(id: Todo.id) -> Todo fails TodoMissing, TodoConflict {
        return attempt update required Todo {
            where: id == id
            transition: delete
            missing: TodoMissing
            conflict: TodoConflict
        }
    }
}
"#;
    let analyzed = analyzed("/project", source);
    assert!(
        analyzed.typing.diagnostics.is_empty(),
        "{:?}",
        analyzed.typing.diagnostics
    );
    let raw = derive_approval_subject(Path::new("/project/app.jadpo"), &analyzed, None, None, None)
        .expect("lifecycle approval subject should derive");
    let subject: Value = serde_json::from_str(&raw).unwrap();
    let facts = &subject["canonical"]["after"]["impact"]["lifecycles"]["facts"][0];
    let transition =
        &source_rows(&subject, "Todo.delete_todo", "update_fields")[0]["contract"]["transition"];
    assert_eq!(
        transition["checked_lifecycle_path"],
        "Todo.lifecycle.transition.delete"
    );
    assert_eq!(transition["sql_writes"], "not_inferred");
    assert_eq!(facts["entity"], "Todo");
    assert!(facts["lifecycle_nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| {
            node["kind"] == "lifecycle_transition"
                && node["name"] == "Todo.lifecycle.transition.delete"
        }));
    assert!(facts["policy_nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| { node["kind"] == "policy" && node["name"] == "Todo.policy" }));
    assert!(facts["generated_fields"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| { field["name"] == "updated_at" && field["node_id"].is_number() }));
    assert_eq!(facts["transitions"][0]["policy_effect"], "delete");
    assert_eq!(facts["transitions"][0]["sql_verb"], "UPDATE");
    assert_eq!(
        facts["transitions"][0]["guarded_sql"][1],
        "lifecycle_visibility"
    );
}

#[test]
fn body_value_change_is_visible_even_with_identical_graph_topology() {
    let before =
        "type Greeting = Text {}\nfunction greeting() -> Greeting { return Greeting(\"before\") }";
    let after =
        "type Greeting = Text {}\nfunction greeting() -> Greeting { return Greeting(\"after\") }";
    let s = subject(after, Some(before));
    let c = &s["canonical"];
    assert_eq!(
        c["before"]["semantic_graph_digest"],
        c["after"]["semantic_graph_digest"]
    );
    assert_ne!(
        c["before"]["behavior_digest"],
        c["after"]["behavior_digest"]
    );
    assert!(c["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["subject"] == "callable:greeting" && d["change"] == "changed"));
}
#[test]
fn new_reachability_of_unchanged_declaration_is_a_separate_decision() {
    let before = SOURCE.replace("return attempt middle(input)", "return attempt leaf(input)");
    let s = subject(SOURCE, Some(&before));
    let c = &s["canonical"];
    assert_eq!(
        c["before"]["facts"]["callable:middle"],
        c["after"]["facts"]["callable:middle"]
    );
    assert!(c["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["subject"] == "reachability:POST /customer"));
}
#[test]
fn two_new_permission_grants_have_two_distinct_decisions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let source =
        std::fs::read_to_string(root.join("tests/compile/pass/134_policy_safe_projection.jadpo"))
            .unwrap();
    let changed = source.replacen(
        "Access.authenticated: [read]",
        "Access.authenticated: [read, create, delete]",
        1,
    );
    let s = subject(&changed, Some(&source));
    let ds = s["canonical"]["decisions"].as_array().unwrap();
    let grants: Vec<_> = ds
        .iter()
        .filter(|d| {
            d["subject"]
                .as_str()
                .unwrap()
                .starts_with("grant:PrivateNote:Access.authenticated:")
        })
        .collect();
    assert_eq!(grants.len(), 2);
    assert_ne!(grants[0]["id"], grants[1]["id"]);
    assert!(grants.iter().all(|d| d["change"] == "added"));
}
#[test]
fn canonical_digest_binds_intent_and_text_contains_identical_facts() {
    let s = subject(SOURCE, None);
    let bytes = serde_json::to_vec(&s["canonical"]).unwrap();
    assert_eq!(
        s["subject_digest"],
        format!("sha256:{:x}", Sha256::digest(bytes))
    );
    let raw = serde_json::to_string(&s).unwrap();
    let text = approval_text(&raw);
    let exported: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(exported, s);
    let project = analyzed("/project", SOURCE);
    let other: Value = serde_json::from_str(
        &derive_approval_subject(
            Path::new("/project/app.jadpo"),
            &project,
            None,
            Some("Different request"),
            None,
        )
        .unwrap(),
    )
    .unwrap();
    assert_ne!(s["subject_digest"], other["subject_digest"]);
}
#[test]
fn source_baseline_is_pinned_and_rebuilds_across_roots_are_identical() {
    let a = analyzed("/one", SOURCE);
    let b = analyzed("/two", SOURCE);
    let emit = |root, project| {
        derive_approval_subject(Path::new(root), project, None, None, None).unwrap()
    };
    let first = emit("/one/app.jadpo", &a);
    assert_eq!(first, emit("/two/app.jadpo", &b));
    let v: Value = serde_json::from_str(&first).unwrap();
    let pin = v["canonical"]["after"]["source_digest"].as_str().unwrap();
    assert!(derive_approval_subject(
        Path::new("/two/app.jadpo"),
        &b,
        Some((Path::new("/one/app.jadpo"), &a)),
        None,
        Some(pin)
    )
    .is_ok());
    assert!(derive_approval_subject(
        Path::new("/two/app.jadpo"),
        &b,
        Some((Path::new("/one/app.jadpo"), &a)),
        None,
        Some("sha256:stale")
    )
    .is_err());
    assert!(
        derive_approval_subject(Path::new("/two/app.jadpo"), &b, None, None, Some(pin)).is_err()
    );
    let same = subject(SOURCE, Some(SOURCE));
    assert_eq!(same["canonical"]["decisions"], serde_json::json!([]));
    assert_eq!(same["canonical"]["scenarios"], serde_json::json!([]));
}
#[test]
fn invalid_source_cannot_masquerade_as_an_empty_effect_graph() {
    let p = analyzed(
        "/invalid",
        "action broken() -> Unit { unsupported_effect() }",
    );
    assert!(
        derive_approval_subject(Path::new("/invalid/app.jadpo"), &p, None, None, None).is_err()
    );
}
#[test]
fn supported_examples_all_emit_checked_subjects() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    for example in [
        "jadpo-seed",
        "first-party-authentication",
        "policy-runtime",
        "persistence-seed",
    ] {
        let path = root.join("examples").join(example);
        let p = analyze_project(&path).unwrap();
        let s = derive_approval_subject(&path, &p, None, None, None).unwrap();
        assert!(s.contains("sha256:"));
    }
}

#[test]
fn omission_of_a_transitive_effect_fails_export_conformance_even_when_rehashed() {
    let s = subject(SOURCE, None);
    let expected = serde_json::to_string(&s).unwrap();
    assert!(jadpo_core::validate_approval_export(&expected, &expected).is_ok());
    let mut omitted = s.clone();
    omitted["canonical"]["after"]["effect_paths"]["POST /customer"]
        .as_object_mut()
        .unwrap()
        .remove("leaf:read:Customer");
    omitted["subject_digest"] = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&omitted["canonical"]).unwrap())
    )
    .into();
    assert!(jadpo_core::validate_approval_export(
        &expected,
        &serde_json::to_string(&omitted).unwrap()
    )
    .is_err());
}

#[test]
fn compiler_input_provenance_is_bound_and_has_no_checkout_paths() {
    let s = subject(SOURCE, None);
    assert_eq!(s["schema_version"], 5);
    let state = &s["canonical"]["after"];
    let compiler = &state["provenance"]["compiler"];
    assert_eq!(compiler["status"], "build_time_source_manifest");
    assert!(compiler["binary_attestation"].is_null());
    let files = compiler["files"].as_object().unwrap();
    for name in [
        "Cargo.lock",
        "crates/core/build.rs",
        "crates/semantic/src/typecheck.rs",
        "crates/core/src/runtime/first_party_authentication.ts",
        "data/iana-zones-2026c.txt",
    ] {
        assert!(files[name].as_str().unwrap().starts_with("sha256:"));
    }
    assert!(files.keys().all(|name| !Path::new(name).is_absolute()));
    assert_eq!(
        compiler["manifest_digest"],
        format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&compiler["files"]).unwrap())
        )
    );
    let mut without_digest = state.clone();
    without_digest
        .as_object_mut()
        .unwrap()
        .remove("state_digest");
    assert_eq!(
        state["state_digest"],
        format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&without_digest).unwrap())
        )
    );
    assert_eq!(state["provenance"]["schema_registry"]["status"], "absent");
    assert!(state["impact"]["authentication"]["details"].is_object());
}

#[test]
fn registry_only_change_invalidates_state_pin_and_is_a_separate_decision() {
    let root = std::env::temp_dir().join(format!("jadpo-approval-registry-{}", std::process::id()));
    let before = root.join("before");
    let after = root.join("after");
    for path in [&before, &after] {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("app.jadpo"), SOURCE).unwrap();
    }
    let b = analyze_project(&before).unwrap();
    let a = analyze_project(&after).unwrap();
    let emit = |pin| {
        derive_approval_subject_with_state_pin(&after, &a, Some((&before, &b)), None, None, pin)
    };
    let absent: Value = serde_json::from_str(&emit(None).unwrap()).unwrap();
    let absent_pin = absent["canonical"]["before"]["state_digest"]
        .as_str()
        .unwrap();
    let registry_path = jadpo_core::initialize_schema_identities(&before, &b).unwrap();
    assert!(
        emit(Some(absent_pin)).is_err(),
        "adding a registry changes the pinned state"
    );
    let registered: Value = serde_json::from_str(&emit(None).unwrap()).unwrap();
    let baseline = &registered["canonical"]["before"];
    let pin = baseline["state_digest"].as_str().unwrap();
    assert_eq!(
        baseline["source_digest"],
        absent["canonical"]["before"]["source_digest"]
    );
    assert_eq!(
        baseline["provenance"]["schema_registry"]["byte_digest"],
        format!(
            "sha256:{:x}",
            Sha256::digest(std::fs::read(&registry_path).unwrap())
        )
    );
    assert!(emit(Some(pin)).is_ok());
    let bytes = std::fs::read_to_string(&registry_path).unwrap();
    let changed = bytes.replace(
        "\"physical_name\":\"customer\"",
        "\"physical_name\":\"archived_customer\"",
    );
    assert_ne!(bytes, changed);
    std::fs::write(&registry_path, changed).unwrap();
    assert!(
        emit(Some(pin)).is_err(),
        "unchanged source cannot hide a physical mapping change"
    );
    let changed: Value = serde_json::from_str(&emit(None).unwrap()).unwrap();
    assert_eq!(
        changed["canonical"]["before"]["source_digest"],
        baseline["source_digest"]
    );
    assert!(changed["canonical"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|decision| decision["subject"] == "provenance:schema_registry"));
    std::fs::write(&registry_path, "{}\n").unwrap();
    assert!(emit(None)
        .unwrap_err()
        .contains("registry validation failed"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn state_pin_requires_a_baseline_and_binds_compiler_provenance() {
    let p = analyzed("/project", SOURCE);
    let raw =
        derive_approval_subject(Path::new("/project/app.jadpo"), &p, None, None, None).unwrap();
    let mut s: Value = serde_json::from_str(&raw).unwrap();
    let state = &mut s["canonical"]["after"];
    state.as_object_mut().unwrap().remove("state_digest");
    state["provenance"]["compiler"]["manifest_digest"] = "sha256:other-build".into();
    let stale_pin = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(state).unwrap())
    );
    assert!(derive_approval_subject_with_state_pin(
        Path::new("/project/app.jadpo"),
        &p,
        Some((Path::new("/project/app.jadpo"), &p)),
        None,
        None,
        Some(&stale_pin)
    )
    .is_err());
    assert!(derive_approval_subject_with_state_pin(
        Path::new("/project/app.jadpo"),
        &p,
        None,
        None,
        None,
        Some(&stale_pin)
    )
    .is_err());
}

fn protected_request(entity: &str, field: &str, callable: &str) -> String {
    serde_json::json!({"entity":entity,"field":field,"callable":callable,
        "row":{"parameter":"row_id","nominal_type":format!("{entity}.id"),"alias":"t1"},
        "from":{"nominal_type":"User.id","alias":"alice"},
        "to":{"parameter":"next_owner","nominal_type":"User.id","alias":"bob"}})
    .to_string()
}
fn protected_fixture() -> (String, String) {
    let baseline = format!("{}\nfailure Missing {{ kind: NotFound code: \"missing\" }}\nfailure Conflicted {{ kind: Conflict code: \"conflicted\" }}\n",compile_fixture("132_scoped_role_policy.jadpo"));
    let attempt = format!("{baseline}\naction move_owner(row_id: Company.id, next_owner: User.id) fails Missing, Conflicted -> Company {{\n return attempt update required Company {{ where: id == row_id set: {{ owner_id: Company.owner_id(next_owner) }} missing: Missing conflict: Conflicted }}\n}}\n");
    (baseline, attempt)
}
fn protected_subject(before: &str, after: &str, request: &str) -> Result<String, String> {
    jadpo_core::derive_protected_attempt(
        Path::new("/attempt/app.jadpo"),
        &analyzed("/attempt", after),
        Path::new("/baseline/app.jadpo"),
        &analyzed("/baseline", before),
        request,
        None,
        None,
    )
}
#[test]
fn protected_attempt_is_exact_native_refusal_with_unverified_intent_and_no_checked_after() {
    let (baseline, attempt) = protected_fixture();
    let request = protected_request("Company", "owner_id", "move_owner");
    let raw = protected_subject(&baseline, &attempt, &request).unwrap();
    let artifact: Value = serde_json::from_str(&raw).unwrap();
    let c = &artifact["canonical"];
    assert_eq!(c["policy_rule"], "Company.fields.owner_id");
    assert_eq!(c["disposition"], "requires_human_decision");
    assert_eq!(c["release_equivalent_success"], false);
    assert_eq!(c["release_authority"], false);
    assert_eq!(c["protected_field"]["direct_role"], "CompanyRole.owner");
    assert_eq!(c["request"]["classification"], "supplied_unverified");
    assert_eq!(c["request"]["live_identity_validation"], "unavailable");
    assert_eq!(c["attempt"]["status"], "frontend_rejected_unexecuted");
    assert!(c["attempt"]["checked_after"].is_null());
    assert!(c["attempt"]["semantic_graph"].is_null());
    assert_eq!(
        c["attempt"]["refusal_diagnostic"]["code"],
        "TYPE_FIELD_UPDATE_FORBIDDEN"
    );
    assert_eq!(
        c["attempt"]["refusal_diagnostic"]["primary"],
        c["attempt"]["assignment"]["field_location"]
    );
    let span = &c["attempt"]["assignment"]["field_location"];
    assert_eq!(
        &attempt[span["start"].as_u64().unwrap() as usize..span["end"].as_u64().unwrap() as usize],
        "owner_id"
    );
    jadpo_core::validate_protected_attempt_export(&raw, &raw).unwrap();
    let renamed_before = baseline
        .replace("Company", "Workspace")
        .replace("owner_id", "custodian_id");
    let renamed_attempt = attempt
        .replace("Company", "Workspace")
        .replace("owner_id", "custodian_id");
    let renamed: Value = serde_json::from_str(
        &protected_subject(
            &renamed_before,
            &renamed_attempt,
            &protected_request("Workspace", "custodian_id", "move_owner"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        renamed["canonical"]["policy_rule"],
        "Workspace.fields.custodian_id"
    );
    assert_ne!(renamed["subject_digest"], artifact["subject_digest"]);
}
#[test]
fn protected_attempt_rejects_unrelated_errors_projection_changed_protection_and_mismatched_request()
{
    let (baseline, attempt) = protected_fixture();
    let request = protected_request("Company", "owner_id", "move_owner");
    for invalid in [
        baseline.clone(),
        format!("{attempt}\nfunction broken() -> Text {{ return 12 }}"),
        format!("{attempt}\ninput broken {{"),
        attempt.replace("Company.owner_id(next_owner)", "Company.owner_id(row_id)"),
        attempt.replace("Company.owner_id(next_owner)", "next_owner"),
        attempt.replace("where: id == row_id", "where: owner_id == row_id"),
        attempt.replace("owner_id: Company.owner_id(next_owner)", "name: \"benign\""),
        attempt.replace("immutable: true", "immutable: false"),
        attempt.replace("next_owner: User.id", "next_owner: Uuid"),
        attempt.replace(
            "return attempt update required Company",
            "return attempt update required Article",
        ),
        attempt.replace("role: CompanyRole.owner", "role: CompanyRole.editor"),
        attempt.replace(
            "return attempt update",
            "var ignored = 1\n return attempt update",
        ),
    ] {
        assert!(
            protected_subject(&baseline, &invalid, &request).is_err(),
            "must refuse {invalid}"
        );
    }
    for invalid in [
        request.replace("owner_id", "name"),
        request.replace("move_owner", "absent"),
        request.replace("next_owner", "row_id"),
        request.replace("Company.id", "User.id"),
        request.replace("User.id", "Uuid"),
        request.replace("\"bob\"", "null"),
        request.replace("\"field\":", "\"unexpected\":true,\"field\":"),
        request.replace("\"entity\":", "\"entity\":\"Other\",\"entity\":"),
    ] {
        assert!(
            protected_subject(&baseline, &attempt, &invalid).is_err(),
            "must refuse {invalid}"
        );
    }
    let unrelated = baseline.replace("immutable: true", "immutable: false");
    assert!(protected_subject(&unrelated, &attempt, &request).is_err());
}
#[test]
fn protected_attempt_rederivation_refuses_coherently_rehashed_foreign_or_omitted_exports() {
    let (baseline, attempt) = protected_fixture();
    let request = protected_request("Company", "owner_id", "move_owner");
    let raw = protected_subject(&baseline, &attempt, &request).unwrap();
    let original: Value = serde_json::from_str(&raw).unwrap();
    for (pointer, value) in [
        (
            "/canonical/request/value/to/alias",
            serde_json::json!("mallory"),
        ),
        (
            "/canonical/attempt/refusal_diagnostic/primary/start",
            serde_json::json!(0),
        ),
        (
            "/canonical/policy_rule",
            serde_json::json!("Other.fields.owner_id"),
        ),
        ("/canonical/disposition", serde_json::json!("approved")),
        (
            "/canonical/before/state_digest",
            serde_json::json!("sha256:foreign"),
        ),
        ("/canonical/before/policy", serde_json::json!({})),
        ("/canonical/before/graph", serde_json::json!({})),
        ("/canonical/compiler_version", serde_json::json!("foreign")),
        ("/canonical/attempt/callable_tokens", serde_json::json!([])),
        ("/canonical/release_authority", serde_json::json!(true)),
    ] {
        let mut forged = original.clone();
        *forged.pointer_mut(pointer).unwrap() = value;
        let canonical = serde_json::to_string(&forged["canonical"]).unwrap();
        forged["subject_digest"] =
            Value::String(format!("sha256:{:x}", Sha256::digest(canonical.as_bytes())));
        assert!(
            jadpo_core::validate_protected_attempt_export(&raw, &forged.to_string()).is_err(),
            "must refuse {pointer}"
        );
    }
    let mut self_asserted = original.clone();
    self_asserted["canonical"]["release_authority"] = serde_json::json!(true);
    self_asserted["subject_digest"] = Value::String(format!(
        "sha256:{:x}",
        Sha256::digest(
            serde_json::to_string(&self_asserted["canonical"])
                .unwrap()
                .as_bytes()
        )
    ));
    assert!(jadpo_core::validate_protected_attempt_export(
        &self_asserted.to_string(),
        &self_asserted.to_string()
    )
    .is_err());
    let changed = protected_subject(
        &baseline,
        &attempt,
        &request.replace("\"bob\"", "\"other\""),
    )
    .unwrap();
    assert_ne!(raw, changed);
    assert!(jadpo_core::validate_protected_attempt_export(&changed, &raw).is_err());
    let changed = protected_subject(
        &baseline,
        &format!("{attempt}\n// exact bytes changed"),
        &request,
    )
    .unwrap();
    assert!(jadpo_core::validate_protected_attempt_export(&changed, &raw).is_err());
    let b = analyzed("/baseline", &baseline);
    let a = analyzed("/attempt", &attempt);
    let c = &original["canonical"];
    assert!(jadpo_core::derive_protected_attempt(
        Path::new("/attempt/app.jadpo"),
        &a,
        Path::new("/baseline/app.jadpo"),
        &b,
        &request,
        c["before"]["source_digest"].as_str(),
        c["before"]["state_digest"].as_str()
    )
    .is_ok());
    assert!(jadpo_core::derive_protected_attempt(
        Path::new("/attempt/app.jadpo"),
        &a,
        Path::new("/baseline/app.jadpo"),
        &b,
        &request,
        Some("sha256:foreign"),
        None
    )
    .is_err());
    assert!(jadpo_core::derive_protected_attempt(
        Path::new("/attempt/app.jadpo"),
        &a,
        Path::new("/baseline/app.jadpo"),
        &b,
        &request,
        None,
        Some("sha256:foreign")
    )
    .is_err());
}
#[test]
fn canonical_checked_delivery_baseline_and_exact_owner_attempt_retain_all_diagnostic_streams() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let path = root.join("examples/golden-todo-migration");
    let baseline = analyze_project(&path).unwrap();
    let action = r#"
    action checkpoint_reassign_owner(todo_id: Todo.id, owner_id: User.id)
        fails TodoNotFound, TodoMutationConflict -> Todo
    {
        var changed = attempt update required Todo {
            where: id == todo_id
            set: { owner_id: Todo.owner_id(owner_id) }
            missing: TodoNotFound { todo_id: todo_id }
            conflict: TodoMutationConflict
        }
        return changed
    }
"#;
    let sources = baseline
        .syntax
        .sources
        .iter()
        .map(|s| {
            let mut text = s.source_text.clone();
            if s.source_name.ends_with("entities/todo.jadpo") {
                let end = text.rfind('}').unwrap();
                text.insert_str(end, action);
            }
            SourceFile::new((&s.source_name).into(), text)
        })
        .collect();
    let attempt = analyze_sources(sources).unwrap();
    let request=serde_json::json!({"entity":"Todo","field":"owner_id","callable":"Todo.checkpoint_reassign_owner",
        "row":{"parameter":"todo_id","nominal_type":"Todo.id","alias":"t1"},
        "from":{"nominal_type":"User.id","alias":"alice"},
        "to":{"parameter":"owner_id","nominal_type":"User.id","alias":"bob"}}).to_string();
    let raw = jadpo_core::derive_protected_attempt(
        &path, &attempt, &path, &baseline, &request, None, None,
    )
    .unwrap();
    let subject: Value = serde_json::from_str(&raw).unwrap();
    let c = &subject["canonical"];
    assert_eq!(c["policy_rule"], "Todo.fields.owner_id");
    assert_eq!(c["disposition"], "requires_human_decision");
    assert_eq!(
        c["before"]["impact"]["jobs"]["status"],
        "compiler_checked_nonexecuting_contract"
    );
    let diagnostics = c["attempt"]["diagnostics"].as_array().unwrap();
    assert!(diagnostics
        .iter()
        .any(|d| d["stage"] == "typing" && d["code"] == "TYPE_FIELD_UPDATE_FORBIDDEN"));
    assert!(diagnostics
        .iter()
        .any(|d| d["stage"] == "semantics"
            && d["code"] == "TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED"));
    assert!(jadpo_core::derive_approval_subject(&path, &attempt, None, None, None).is_err());
    assert!(attempt.delivery_model().bindings().is_empty());
}
