//! Grounded static prerequisites; never live delivery authority.
use jadpo_core::{
    analyze_sources, derive_approval_subject, derive_target, discover_sources, AnalyzedProject,
};
use jadpo_syntax::SourceFile;
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

fn candidate(source: &str) -> AnalyzedProject {
    candidate_with_change(source, None)
}

fn candidate_with_change(source: &str, change: Option<(&str, &str, &str)>) -> AnalyzedProject {
    let root = root();
    let mut sources = discover_sources(&root.join("examples/golden-todo-migration")).unwrap();
    // Candidate fixture owns its delivery declarations; canonical checked
    // source is excluded to avoid duplicate names while retaining contracts.
    sources.retain(|source| !source.path.ends_with("jobs/reminders.jadpo"));
    if let Some((file, before, after)) = change {
        let changed = sources
            .iter_mut()
            .find(|source| source.path.ends_with(file))
            .unwrap();
        assert!(changed.text.contains(before), "missing {before} in {file}");
        changed.text = changed.text.replacen(before, after, 1);
    }
    sources.push(SourceFile::new(
        root.join("tests/validation/fixtures/reminder-delivery-v1.jadpo"),
        source.to_owned(),
    ));
    analyze_sources(sources).unwrap()
}

fn selector_rejects(project: AnalyzedProject) {
    let codes = codes(&project);
    assert!(
        codes.contains(&"TYPE_JOB_DELIVERY_SELECTOR_INVALID"),
        "{codes:?}"
    );
    assert!(codes.contains(&"TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED"));
    assert!(project.typing.jobs.is_empty());
    assert!(derive_approval_subject(&root(), &project, None, None, None).is_err());
}

#[test]
fn selector_rejects_wrong_identity_fields_variants_relationships_and_cursor() {
    for (before, after) in [
        ("identity: Todo.id", "identity: Todo.title"),
        ("identity: Todo.id", "identity: User.id"),
        ("due: Todo.due_at", "due: Todo.title"),
        ("due: Todo.due_at", "due: Todo.reminder_sent_at"),
        ("unsent: Todo.reminder_sent_at", "unsent: Todo.updated_at"),
        (
            "open: Todo.status(TodoStatus.open)",
            "open: Todo.due_at(TodoStatus.open)",
        ),
        (
            "open: Todo.status(TodoStatus.open)",
            "open: Todo.status(UserStatus.active)",
        ),
        (
            "open: Todo.status(TodoStatus.open)",
            "open: Todo.status(TodoStatus.missing)",
        ),
        ("visible: Todo", "visible: User"),
        ("owner_visible: User", "owner_visible: Service"),
        (
            "required_owner: Todo.owner",
            "required_owner: Todo.owner_id",
        ),
        ("required_owner: Todo.owner", "required_owner: User.todos"),
        (
            "order_by: Todo.due_at asc, Todo.id asc",
            "order_by: Todo.id asc, Todo.due_at asc",
        ),
        ("field: Todo.reminder_sent_at", "field: Todo.due_at"),
        ("entity: Todo", "entity: MissingTodo"),
    ] {
        assert!(FIXTURE.contains(before));
        let project = candidate(&FIXTURE.replacen(before, after, 1));
        // Keep the exact public diagnostic assertion in an executable #[test]
        // body, so the generated diagnostic catalogue indexes this evidence.
        assert!(project
            .typing
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "TYPE_JOB_DELIVERY_SELECTOR_INVALID" }));
        selector_rejects(project);
    }
}

#[test]
fn selector_uses_effective_nullability_identity_and_authoritative_store() {
    for change in [
        ("entities/todo.jadpo", "due_at: Instant?", "due_at: Instant"),
        (
            "entities/todo.jadpo",
            "reminder_sent_at: Instant?",
            "reminder_sent_at: Instant",
        ),
        (
            "entities/todo.jadpo",
            "status: TodoStatus",
            "status: TodoStatus?",
        ),
        (
            "entities/todo.jadpo",
            "status: TodoStatus\n",
            "status: TodoStatus optional\n",
        ),
        (
            "entities/todo.jadpo",
            "due_at: Instant?",
            "due_at: Instant? optional",
        ),
        (
            "entities/todo.jadpo",
            "reminder_sent_at: Instant?",
            "reminder_sent_at: Instant? optional",
        ),
        (
            "entities/todo.jadpo",
            "owner_id: User.id {\n        role: TodoRole.owner\n        immutable: true\n    }",
            "owner_id: User.id {\n        role: TodoRole.owner\n        immutable: true\n    } optional",
        ),
        (
            "entities/todo.jadpo",
            "owner_id: User.id {",
            "owner_id: User.id? {",
        ),
        ("entities/user.jadpo", "store: primary", "store: replica"),
        (
            "entities/user.jadpo",
            "visible when status == UserStatus.active",
            "",
        ),
        ("entities/todo.jadpo", "visible when deleted_at == none", ""),
        (
            "entities/todo.jadpo",
            "references owner_id: User.id as: owner",
            "references owner_id: User.email as: owner",
        ),
        (
            "entities/todo.jadpo",
            "due_at: Instant?",
            "due_at: Instant? { immutable: true }",
        ),
        (
            "entities/todo.jadpo",
            "reminder_sent_at: Instant?",
            "reminder_sent_at: Instant? { immutable: true }",
        ),
    ] {
        let project = candidate_with_change(FIXTURE, Some(change));
        if change.2.contains("optional") {
            assert!(project.syntax.diagnostics().next().is_none(), "{:?}", codes(&project));
        }
        selector_rejects(project);
    }
    let inherited_due = candidate_with_change(
        FIXTURE,
        Some((
            "entities/todo.jadpo",
            "due_at: Instant?",
            "due_at: NullableDue",
        )),
    );
    // A missing alias is not sufficient proof even if the name sounds nullable.
    selector_rejects(inherited_due);
    let source = format!("type NullableDue = Instant? {{}}\n{FIXTURE}");
    let inherited_due = candidate_with_change(
        &source,
        Some((
            "entities/todo.jadpo",
            "due_at: Instant?",
            "due_at: NullableDue",
        )),
    );
    assert!(codes(&inherited_due).is_empty());
    assert_eq!(inherited_due.delivery_model().bindings().len(), 1);
    let source = format!("type OptionalOwner = User.id? {{}}\n{FIXTURE}");
    selector_rejects(candidate_with_change(
        &source,
        Some((
            "entities/todo.jadpo",
            "owner_id: User.id {",
            "owner_id: OptionalOwner {",
        )),
    ));
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

#[test]
fn actual_migration_contracts_finish_nonexecuting_binding_not_worker() {
    let project = candidate(FIXTURE);
    assert!(codes(&project).is_empty(), "{:?}", codes(&project));
    assert_eq!(project.delivery_model().bindings().len(), 1);
    assert!(project.typing.jobs.is_empty());
    assert_eq!(project.typing.delivery_candidates.len(), 1);
    let facts = &project.typing.delivery_candidates[0];
    assert_eq!(facts.schedule_candidate().interval_ms, 900_000);
    assert_eq!(
        facts.service_operation_candidate(),
        "ReminderMail.send_overdue_reminder"
    );
    let membership = project
        .policy
        .memberships
        .iter()
        .find(|membership| membership.entity == "ReminderServiceMembership")
        .unwrap();
    assert_eq!(membership.principal, "Service");
    assert_eq!(membership.member_field, "service_id");
    assert_eq!(membership.role_type, "ReminderRole");
    assert_eq!(membership.scope, "application");
    for name in [
        "Todo.create_todo",
        "Todo.patch_todo",
        "Todo.owner",
        "ReminderMail.send_overdue_reminder",
        "authentication.api_bearer.validator.service_key.api_key",
    ] {
        assert!(project.semantics.node(name).is_some(), "missing {name}");
    }
    let root = root();
    assert!(derive_approval_subject(&root, &project, None, None, None).is_ok());
    assert!(derive_target(&root, &project).is_ok());
    // Removing the descriptor leaves genuinely supported ordinary declarations,
    // including the exact imported service, not a rewritten unchecked graph.
    let plain = FIXTURE[..FIXTURE.find("    delivery:").unwrap()].to_owned() + "}\n";
    let project = candidate(&plain);
    assert!(codes(&project).is_empty(), "{:?}", codes(&project));
    assert_eq!(project.typing.jobs.len(), 1);
    assert_eq!(project.typing.jobs[0].interval_ms, 900_000);
}

#[test]
fn delivery_service_requires_exact_checked_contract_and_payload_types() {
    for (before, after) in [
        ("intent: ReminderIntentId", "intent: ReminderRunAt"),
        ("input: ReminderMessage", "input: TodoView"),
        ("output: ReminderReceipt", "output: PublicHealth"),
        (
            "operation: ReminderMail.send_overdue_reminder",
            "operation: Todo.create_todo",
        ),
        ("from: config.mail_sender", "from: config.mail_api_key"),
        ("to: Todo.owner.email", "to: User.email"),
        ("todo_title: Todo.title", "todo_title: Todo.id"),
        ("due_at: Todo.due_at", "due_at: Todo.reminder_sent_at"),
        (
            "type ReminderIntentId = Uuid {}",
            "type ReminderIntentId = Uuid? {}",
        ),
    ] {
        let project = candidate(&FIXTURE.replacen(before, after, 1));
        assert!(
            codes(&project).contains(&"TYPE_JOB_DELIVERY_SERVICE_INVALID"),
            "{before}: {:?}",
            codes(&project)
        );
        assert!(project.typing.delivery_candidates.is_empty());
        assert!(project.typing.jobs.is_empty());
    }
    for change in [
        ("values/contracts.jadpo", "from: Email", "from: Text"),
        ("values/contracts.jadpo", "to: User.email", "to: Email"),
        (
            "values/contracts.jadpo",
            "todo_title: Todo.title",
            "todo_title: Text",
        ),
        (
            "values/contracts.jadpo",
            "todo_title: Todo.title\n    due_at: Todo.due_at",
            "todo_title: Todo.title\n    due_at: Instant?",
        ),
        (
            "values/contracts.jadpo",
            "accepted_at: Instant",
            "accepted_at: Instant?",
        ),
        (
            "values/contracts.jadpo",
            "accepted_at: Instant",
            "accepted_at: Instant optional",
        ),
        (
            "values/contracts.jadpo",
            "accepted_at: Instant",
            "accepted_at: Instant\n    extra: Text",
        ),
    ] {
        let project = candidate_with_change(FIXTURE, Some(change));
        assert!(
            codes(&project).contains(&"TYPE_JOB_DELIVERY_SERVICE_INVALID"),
            "{change:?}: {:?}",
            codes(&project)
        );
        assert!(project.typing.delivery_candidates.is_empty());
    }
}

#[test]
fn sealed_delivery_types_and_operation_cannot_be_authored_as_ordinary_values() {
    for extra in [
        "type ForgedIntent = ReminderIntentId {}",
        "value WrappedReceipt { receipt: ReminderReceipt }",
        "value WrappedIntent { intents: List<ReminderIntentId> }",
        "value RecursiveA { b: RecursiveB? }\nvalue RecursiveB { a: RecursiveA? receipt: ReminderReceipt? }",
        "enum ForgedOutcome { accepted { receipt: ReminderReceipt } }",
        "action forged(receipt: ReminderReceipt) -> Unit {}",
        "action forged() -> ReminderReceipt { return ReminderReceipt { accepted_at: clock.now } }",
        "action forged() -> Unit { var intent = ReminderIntentId(Uuid(\"00000000-0000-0000-0000-000000000001\")) }",
        "action forged() -> Unit { var receipt = ReminderReceipt { accepted_at: clock.now } }",
        "action forged() -> Unit { if true { var hidden: ReminderIntentId? = none } }",
        "action forged() -> Unit { var receipt = ReminderMail.send_overdue_reminder() }",
    ] {
        let project = candidate(&format!("{FIXTURE}\n{extra}\n"));
        assert!(project.syntax.diagnostics().next().is_none(), "{extra}: {:?}", codes(&project));
        assert!(codes(&project).contains(&"TYPE_JOB_DELIVERY_SEALED_USE"), "{extra}: {:?}", codes(&project));
        assert!(codes(&project).contains(&"TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED"));
        assert!(project.typing.jobs.is_empty());
        assert!(derive_approval_subject(&root(), &project, None, None, None).is_err());
    }
    // A shape is sealed by a selected descriptor, not by its spelling globally.
    let plain = FIXTURE[..FIXTURE.find("    delivery:").unwrap()].to_owned() + "}\n";
    let project = candidate(&format!(
        "{plain}\nvalue OrdinaryReceiptWrapper {{ receipt: ReminderReceipt }}"
    ));
    assert!(codes(&project).is_empty(), "{:?}", codes(&project));
    assert!(project.typing.delivery_candidates.is_empty());
}

#[test]
fn sealed_origins_cover_unused_paths_and_invalid_declaration_expressions() {
    for extra in [
        "route GET /sealed-path/{id} { auth: none path: { id: ReminderIntentId } success: no_content action: {} }",
        "route GET /sealed-constructor { auth: none success: no_content run: reminder_tick(ReminderRunAt(clock.now), ReminderIntentId(Uuid(\"00000000-0000-0000-0000-000000000001\"))) }",
        "route GET /sealed-provider { auth: none success: no_content run: reminder_tick(ReminderRunAt(clock.now), ReminderMail.send_overdue_reminder()) }",
        "job hidden every 15m { concurrency: singleton run: reminder_tick(ReminderRunAt(clock.now), ReminderIntentId(Uuid(\"00000000-0000-0000-0000-000000000001\"))) retry: next_schedule }",
        "fixture forged { clock: fixed Instant(\"2026-10-05T00:00:00Z\", ReminderIntentId(Uuid(\"00000000-0000-0000-0000-000000000001\"))) }",
        "fixture forged { config { mail_sender: Email(\"x\", ReminderIntentId(Uuid(\"00000000-0000-0000-0000-000000000001\"))) } }",
        "fixture forged { service ReminderMail: fake { send_overdue_reminder => accept Text(\"x\", ReminderReceipt { accepted_at: Instant(\"2026-10-05T00:00:00Z\") }) } }",
    ] {
        let project = candidate(&format!("{FIXTURE}\n{extra}\n"));
        assert!(project.syntax.diagnostics().next().is_none(), "{extra}: {:?}", codes(&project));
        assert!(codes(&project).contains(&"TYPE_JOB_DELIVERY_SEALED_USE"), "{extra}: {:?}", codes(&project));
        assert!(codes(&project).contains(&"TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED"));
        assert!(project.typing.jobs.is_empty());
        assert!(derive_approval_subject(&root(), &project, None, None, None).is_err());
    }
}

#[test]
fn delivery_uses_common_nominal_schedule_checks_without_publishing_a_plain_job() {
    for (old, new, expected) in [
        ("every 15m", "every 0s", "TYPE_JOB_INTERVAL_INVALID"),
        (
            "at: ReminderRunAt",
            "at: Instant",
            "TYPE_JOB_RUN_SIGNATURE_INVALID",
        ),
        (
            "reminder_tick(ReminderRunAt(clock.now))",
            "reminder_tick(clock.now)",
            "TYPE_JOB_RUN_ARGUMENT_INVALID",
        ),
        (
            "reminder_tick(ReminderRunAt(clock.now))",
            "reminder_tick(ReminderRunAt(clock.now), ReminderRunAt(clock.now))",
            "TYPE_JOB_RUN_ARGUMENT_INVALID",
        ),
        (
            "type ReminderRunAt = Instant {}",
            "type ReminderRunAt = Instant { min: Instant(\"2000-01-01T00:00:00Z\") }",
            "TYPE_JOB_RUN_SIGNATURE_INVALID",
        ),
    ] {
        assert!(FIXTURE.contains(old));
        let project = candidate(&FIXTURE.replace(old, new));
        let codes = codes(&project);
        assert!(
            codes.contains(&expected),
            "expected {expected}, got {codes:?}"
        );
        assert!(codes.contains(&"TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED"));
        assert!(project.typing.jobs.is_empty());
        assert!(derive_approval_subject(&root(), &project, None, None, None).is_err());
    }
}
