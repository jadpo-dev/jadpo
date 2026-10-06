//! Structural census is not transaction/ABA or generated-worker evidence.
use jadpo_core::{analyze_sources, derive_approval_subject, discover_sources, AnalyzedProject};
use jadpo_syntax::SourceFile;
use serde_json::json;
use std::path::PathBuf;
const FIXTURE: &str =
    include_str!("../../../../tests/validation/fixtures/reminder-delivery-v1.jadpo");
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_owned()
}
fn analyze(fixture: &str, changes: &[(&str, &str, &str)], extra: &str) -> AnalyzedProject {
    let root = root();
    let mut sources = discover_sources(&root.join("examples/golden-todo-migration")).unwrap();
    // Candidate fixture owns its delivery declarations; canonical checked
    // source is excluded to avoid duplicate names while retaining contracts.
    sources.retain(|source| !source.path.ends_with("jobs/reminders.jadpo"));
    sources
        .iter_mut()
        .find(|source| source.path.ends_with("values/contracts.jadpo"))
        .unwrap()
        .text
        .push_str(extra);
    for (file, before, after) in changes {
        let source = sources
            .iter_mut()
            .find(|source| source.path.ends_with(file))
            .unwrap();
        assert!(source.text.contains(before), "missing {before} in {file}");
        source.text = source.text.replacen(before, after, 1);
    }
    sources.push(SourceFile::new(
        root.join("tests/validation/fixtures/reminder-delivery-v1.jadpo"),
        fixture.to_owned(),
    ));
    analyze_sources(sources).unwrap()
}
fn codes(project: &AnalyzedProject) -> Vec<&str> {
    project
        .syntax
        .diagnostics()
        .chain(project.semantics.diagnostics.iter())
        .chain(project.typing.diagnostics.iter())
        .chain(project.failures.diagnostics.iter())
        .map(|diagnostic| diagnostic.code)
        .collect()
}
fn refuses(project: &AnalyzedProject) {
    assert!(project.typing.jobs.is_empty());
    assert!(project.delivery_model().bindings().is_empty());
    assert!(codes(project).contains(&"TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED"));
    assert!(derive_approval_subject(&root(), project, None, None, None).is_err());
}
fn checked_nonexecuting(project: &AnalyzedProject) {
    assert!(codes(project).is_empty(), "{:?}", codes(project));
    assert!(project.typing.jobs.is_empty());
    assert_eq!(project.delivery_model().bindings().len(), 1);
    assert!(derive_approval_subject(&root(), project, None, None, None).is_ok());
}

#[test]
fn hook_baseline_and_parameter_rename_finish_nonexecuting_binding() {
    let baseline = analyze(FIXTURE, &[], "");
    checked_nonexecuting(&baseline);
    let original =
        std::fs::read_to_string(root().join("examples/golden-todo-migration/entities/todo.jadpo"))
            .unwrap();
    let renamed = original.replace("input", "payload");
    let project = analyze(FIXTURE, &[("entities/todo.jadpo", &original, &renamed)], "");
    checked_nonexecuting(&project);
}

#[test]
fn exact_hook_facts_come_from_resolved_sites_not_descriptor_or_byte_offsets() {
    let baseline = analyze(FIXTURE, &[], "");
    assert_eq!(baseline.delivery_hook_candidates().len(), 1);
    let facts = baseline.delivery_hook_candidates()[0].candidate_facts();
    assert_eq!(
        facts,
        json!({
            "status":"incomplete_nonexecuting_candidate", "job":"overdue_reminders",
            "create": {
                "site_identity":"Todo.create_todo.direct_create", "callable":"Todo.create_todo", "kind":"direct_create",
                "parameter_identity":"Todo.create_todo.parameter.0", "parameter_name":"input", "parameter_type":"CreateTodo",
                "input_due_field":"CreateTodo.due_at", "bound_due_field":"Todo.due_at", "bound_sent_field":"Todo.reminder_sent_at",
                "open_initial":{"field":"Todo.status", "variant":"TodoStatus.open"}, "sent_initial":"none", "sent_reset":null,
                "transactional_revision_evidence":"unproved", "runtime_supported":false
            },
            "patch": {
                "site_identity":"Todo.patch_todo.direct_patch", "callable":"Todo.patch_todo", "kind":"direct_patch",
                "parameter_identity":"Todo.patch_todo.parameter.1", "parameter_name":"input", "parameter_type":"PatchTodo",
                "input_due_field":"PatchTodo.due_at", "bound_due_field":"Todo.due_at", "bound_sent_field":"Todo.reminder_sent_at",
                "open_initial":null, "sent_initial":null, "sent_reset":"none_when_same_parameter_due_supplied",
                "transactional_revision_evidence":"unproved", "runtime_supported":false
            }
        })
    );
    let original =
        std::fs::read_to_string(root().join("examples/golden-todo-migration/entities/todo.jadpo"))
            .unwrap();
    let renamed = format!(
        "\n\n// Offset and parameter spelling are not site identity.\n{}",
        original.replace("input", "payload")
    );
    let changed = analyze(FIXTURE, &[("entities/todo.jadpo", &original, &renamed)], "");
    let mut changed_facts = changed.delivery_hook_candidates()[0].candidate_facts();
    assert_eq!(changed_facts["create"]["parameter_name"], "payload");
    assert_eq!(changed_facts["patch"]["parameter_name"], "payload");
    changed_facts["create"]["parameter_name"] = json!("input");
    changed_facts["patch"]["parameter_name"] = json!("input");
    assert_eq!(changed_facts, facts);
    checked_nonexecuting(&changed);
    let invalid = analyze(
        &FIXTURE.replacen("patch: Todo.patch_todo", "patch: Todo.delete_todo", 1),
        &[],
        "",
    );
    assert!(invalid.delivery_hook_candidates().is_empty());
    assert!(codes(&invalid).contains(&"TYPE_JOB_DELIVERY_HOOK_INVALID"));
    refuses(&invalid);
}

#[test]
fn hook_contract_rejects_wrong_sites_and_non_exact_supplied_reset() {
    for (before, after) in [
        ("create: Todo.create_todo", "create: Todo.get"),
        ("patch: Todo.patch_todo", "patch: Todo.delete_todo"),
        ("supplied: PatchTodo.due_at", "supplied: CreateTodo.due_at"),
    ] {
        let project = analyze(&FIXTURE.replacen(before, after, 1), &[], "");
        assert!(
            codes(&project).contains(&"TYPE_JOB_DELIVERY_HOOK_INVALID"),
            "{before}: {:?}",
            codes(&project)
        );
        refuses(&project);
    }
    for change in [
        (
            "entities/todo.jadpo",
            "status: Todo.status(TodoStatus.open)",
            "status: Todo.status(TodoStatus.completed)",
        ),
        (
            "entities/todo.jadpo",
            "reminder_sent_at: none\n        } conflict",
            "reminder_sent_at: clock.now\n        } conflict",
        ),
        (
            "entities/todo.jadpo",
            "due_at: input.due_at\n            reminder_sent_at",
            "due_at: none\n            reminder_sent_at",
        ),
        (
            "entities/todo.jadpo",
            "none when input.due_at supplied",
            "none when input.title supplied",
        ),
        (
            "entities/todo.jadpo",
            "none when input.due_at supplied",
            "none",
        ),
        (
            "entities/todo.jadpo",
            "none when input.due_at supplied",
            "clock.now when input.due_at supplied",
        ),
        (
            "values/contracts.jadpo",
            "due_at: Todo.due_at optional",
            "due_at: Todo.due_at",
        ),
    ] {
        let project = analyze(FIXTURE, &[change], "");
        assert!(
            project.syntax.diagnostics().next().is_none(),
            "{change:?}: {:?}",
            codes(&project)
        );
        assert!(
            codes(&project).contains(&"TYPE_JOB_DELIVERY_HOOK_INVALID"),
            "{change:?}: {:?}",
            codes(&project)
        );
        refuses(&project);
    }
}

#[test]
fn census_rejects_extra_creates_due_sent_writes_transitions_and_unknown_patches() {
    const CREATE: &str = "var competing = attempt create Todo { owner_id: Todo.owner_id(principal.user_id) title: input.title status: Todo.status(TodoStatus.open) due_at: input.due_at reminder_sent_at: none } conflict: TodoMutationConflict";
    for injected in [CREATE.to_owned(), format!("if false {{ {CREATE} }}")] {
        let after = format!("{injected}\n        match input.due_at {{");
        let project = analyze(
            FIXTURE,
            &[("entities/todo.jadpo", "match input.due_at {", &after)],
            "",
        );
        assert!(
            project.syntax.diagnostics().next().is_none(),
            "{:?}",
            codes(&project)
        );
        assert!(codes(&project).contains(&"TYPE_JOB_DELIVERY_HOOK_INVALID"));
        refuses(&project);
    }
    for change in [
        ("entities/todo.jadpo", "set: { deleted_at: clock.now }", "set: { deleted_at: clock.now due_at: none }"),
        ("entities/todo.jadpo", "deleted_at: none\n        }", "deleted_at: none\n            reminder_sent_at: none\n        }"),
        ("entities/todo.jadpo", "reminder_sent_at: none when input.due_at supplied", "reminder_sent_at: none when input.due_at supplied\n                due_at: none when input.title supplied"),
        ("entities/todo.jadpo", "var todo = attempt update required Todo {", "var alias = input\n        var todo = attempt update required Todo {"),
    ] {
        let mut project = analyze(FIXTURE, &[change], "");
        // The last probe additionally swaps the actual whole-patch origin.
        if change.2.contains("var alias") {
            project = analyze(FIXTURE, &[change, ("entities/todo.jadpo", "patch: input", "patch: alias")], "");
        }
        assert!(project.syntax.diagnostics().next().is_none(), "{change:?}: {:?}", codes(&project));
        assert!(codes(&project).contains(&"TYPE_JOB_DELIVERY_HOOK_INVALID"), "{change:?}: {:?}", codes(&project));
        refuses(&project);
    }
    let extra = "\naction helper(id: Todo.id) fails TodoNotFound, TodoMutationConflict -> Unit { var todo = attempt update required Todo { where: id == id set: { due_at: none } missing: TodoNotFound { todo_id: id } conflict: TodoMutationConflict } }\n";
    let project = analyze(&format!("{FIXTURE}{extra}"), &[], "");
    assert!(
        project.syntax.diagnostics().next().is_none(),
        "{:?}",
        codes(&project)
    );
    assert!(codes(&project).contains(&"TYPE_JOB_DELIVERY_HOOK_INVALID"));
    refuses(&project);
}

#[test]
fn extra_status_only_patch_has_proven_disjoint_footprint() {
    let extra = "\ninput StatusOnlyPatch { status: Todo.status optional }\n";
    let action = "action status_only(todo_id: Todo.id, changes: StatusOnlyPatch) fails TodoNotFound, EmptyPatch, TodoMutationConflict -> Unit { var todo = attempt update required Todo { where: id == todo_id patch: changes empty: EmptyPatch missing: TodoNotFound { todo_id: todo_id } conflict: TodoMutationConflict } }\n    action delete_todo";
    let project = analyze(
        FIXTURE,
        &[("entities/todo.jadpo", "action delete_todo", action)],
        extra,
    );
    checked_nonexecuting(&project);
}

#[test]
fn outcome_success_binding_cannot_borrow_outer_disjoint_patch_origin() {
    let records = "\ninput StatusOnlyPatch { status: Todo.status optional }\ntype DuePatch = Object { due_at: Todo.due_at optional }\n";
    let helpers = "\nfunction provide_patch(value: DuePatch) fails EmptyPatch -> DuePatch { if false { reject EmptyPatch } return value }\nfunction keep_patch(todo: Todo, value: DuePatch) -> DuePatch { return value }\nfunction after_patch(todo: Todo) -> Todo.due_at { return none }\n";
    let action = "action extra(todo_id: Todo.id, changes: StatusOnlyPatch, incoming: DuePatch) fails TodoNotFound, EmptyPatch, TodoMutationConflict -> Unit { var result = match provide_patch(incoming) { success(changes) => keep_patch(attempt update required Todo { where: id == todo_id patch: changes empty: EmptyPatch missing: TodoNotFound { todo_id: todo_id } conflict: TodoMutationConflict }, changes) failure EmptyPatch => propagate } }\n    action delete_todo";
    let rejected = "failure ExtraPatchRejected { kind: InvalidValue code: \"extra_patch_rejected\" message: \"Extra patch rejected.\" internal { patch: DuePatch } }\n";
    let mutation = "keep_patch(attempt update required Todo { where: id == todo_id patch: changes empty: EmptyPatch missing: TodoNotFound { todo_id: todo_id } conflict: TodoMutationConflict }, changes)";
    let update = "attempt update required Todo { where: id == todo_id patch: changes empty: EmptyPatch missing: TodoNotFound { todo_id: todo_id } conflict: TodoMutationConflict }";
    let constructed = action.replace(
        mutation,
        &format!("DuePatch {{ due_at: after_patch({update}) }}"),
    );
    let returned = action.replace("-> Unit { var result =", "-> DuePatch { return");
    let rejected_arm = action
        .replace(
            "TodoMutationConflict -> Unit",
            "TodoMutationConflict, ExtraPatchRejected -> Unit",
        )
        .replace(
            mutation,
            &format!("reject ExtraPatchRejected {{ patch: {mutation} }}"),
        );
    for (body, rejection_probe) in [
        (action, false),
        (constructed.as_str(), false),
        (returned.as_str(), false),
        (rejected_arm.as_str(), true),
    ] {
        let project = analyze(
            &format!("{FIXTURE}{helpers}{rejected}"),
            &[("entities/todo.jadpo", "action delete_todo", body)],
            records,
        );
        assert!(
            project.syntax.diagnostics().next().is_none(),
            "{:?}",
            codes(&project)
        );
        assert!(
            codes(&project).contains(&"TYPE_JOB_DELIVERY_HOOK_INVALID"),
            "{:?}",
            codes(&project)
        );
        // A success arm cannot reject under the ordinary failure contract.
        // This fourth context checks census traversal, not a clean positive.
        assert_eq!(
            codes(&project).contains(&"FAIL_OUTCOME_SUCCESS_VALUE_REQUIRED"),
            rejection_probe
        );
        assert!(
            codes(&project).iter().all(|code| {
                [
                    "TYPE_JOB_DELIVERY_HOOK_INVALID",
                    "TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED",
                ]
                .contains(code)
                    || (rejection_probe && *code == "FAIL_OUTCOME_SUCCESS_VALUE_REQUIRED")
            }),
            "{body}: {:?}",
            codes(&project)
        );
        refuses(&project);
    }
}
