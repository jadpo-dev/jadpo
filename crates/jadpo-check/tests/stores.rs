//! Restricted values stored elsewhere (area 9, "Restricted values stored
//! in another record", roadmap chunk 5.1d): a create or update may store a
//! value in a field only when everyone who may read that field may read
//! the value, followed as an output's is; otherwise
//! `policy.output_field_unproved` names the write. A copy between fields
//! the same callers read stays allowed: in the same record, in a record
//! that refers to the one it came from, or in one of the same tenant.

use jadpo_db::Layout;

use crate::support;

/// A tenant, its members' roles, projects whose budget only editors may
/// read, the tasks of a project, shared public rates, and whatever
/// follows.
fn stores(code: &str) -> Vec<String> {
    let text = format!(
        r#"principal {{
    user: {{
        subject: Person.handle
    }}
}}

authentication {{
    api_token: {{}}
}}

policy {{
    tenant: Company
}}

type Handle = Text {{
    max_length: 40
}}

type Label = Text {{
    max_length: 80
}}

type Amount = Int {{}}

entity Person {{
    id: Uuid
    handle: Handle {{
        unique: true
    }}

    policy: {{
        read: [id]
        tenant: Tenancy.shared
    }}
}}

type Role = Enum {{
    owner
    editor
    viewer
}}

entity Company {{
    id: Uuid
    name: Label

    policy: {{
        read: [Role.owner, Role.editor, Role.viewer]
    }}
}}

entity Membership {{
    id: Uuid
    company_id: Company.id {{
        on_delete: OnDelete.cascade
    }}
    person_id: Person.id {{
        on_delete: OnDelete.cascade
    }}
    role: Role

    membership: {{
        scope: company_id
        member: person_id
        role: role
    }}

    policy: {{
        read: [Role.owner, person_id]
    }}
}}

entity Project {{
    id: Uuid
    company_id: Company.id {{
        on_delete: OnDelete.cascade
    }}
    name: Label
    budget: Amount {{
        policy: {{
            read: [Role.editor]
            update: [Role.editor]
        }}
    }}
    forecast: Amount {{
        policy: {{
            read: [Role.editor]
            update: [Role.editor]
        }}
    }}
    spent: Amount
    audit: Amount? {{
        policy: FieldAccess.server_only
    }}

    policy: {{
        create: [Role.owner, Role.editor]
        read: [Role.owner, Role.editor, Role.viewer]
        update: [Role.owner, Role.editor]
    }}
}}

entity Task {{
    id: Uuid
    company_id: Company.id {{
        on_delete: OnDelete.cascade
    }}
    project_id: Project.id {{
        on_delete: OnDelete.cascade
    }}
    title: Label
    cost: Amount
    closed: Bool

    lifecycle: {{
        initial: {{
            closed: false
        }}
        transitions: {{
            close: {{
                set: {{
                    closed: true
                }}
            }}
        }}
    }}

    policy: {{
        create: [Role.owner, Role.editor]
        read: [Role.owner, Role.editor, Role.viewer]
        update: [Role.owner, Role.editor]
    }}
}}

entity Rate {{
    id: Uuid
    amount: Amount

    policy: {{
        read: [Access.public]
        tenant: Tenancy.shared
    }}
}}

query company_projects(company_id: Company.id) -> List<Project> {{
    from: Project
    where: Project.company_id == company_id
    order: [Project.name asc]
    limit: 100
}}

{code}"#
    );
    support::problems(Layout::Loose, &[("main.jadpo", text.as_str())])
}

/// Each problem's message, one per line.
fn messages(found: &[String]) -> String {
    found.join("\n")
}

/// Every problem is `policy.output_field_unproved`, each `expected` is
/// among them, there are no more than those, and none is about an action
/// in `clean`.
fn assert_stores(found: &[String], expected: &[&str], clean: &[&str]) {
    let all = messages(found);
    for problem in found {
        assert!(
            problem.contains("policy.output_field_unproved"),
            "{problem}\n{all}"
        );
    }
    for message in expected {
        assert!(all.contains(message), "{message}\n{all}");
    }
    assert_eq!(found.len(), expected.len(), "{all}");
    for name in clean {
        for problem in found {
            assert!(!problem.contains(&format!(": `{name}` ")), "{name}\n{all}");
        }
    }
}

#[test]
fn a_copy_within_a_record_is_refused_when_its_new_field_has_more_readers() {
    let found = stores(
        r#"action share_budget(project_id: Project.id) fails ProjectNotFound -> Project {
    var project: Project = attempt Project.get(project_id)
    return Project.update(project) {
        set: {
            spent: project.budget
        }
    }
}

action copy_budget(project_id: Project.id) fails ProjectNotFound -> Project {
    var project: Project = attempt Project.get(project_id)
    return Project.update(project) {
        set: {
            forecast: project.budget
        }
    }
}

action narrow_spent(project_id: Project.id) fails ProjectNotFound -> Project {
    var project: Project = attempt Project.get(project_id)
    return Project.update(project) {
        set: {
            forecast: project.spent
        }
    }
}

action audit_budget(project_id: Project.id) fails ProjectNotFound -> Project {
    var project: Project = attempt Project.get(project_id)
    return Project.update(project) {
        set: {
            audit: project.budget
        }
    }
}
"#,
    );
    assert_stores(
        &found,
        &[
            "`share_budget` stores `Project.budget` in `Project.spent`, which not everyone who may read `spent` may read",
        ],
        &["copy_budget", "narrow_spent", "audit_budget"],
    );
}

#[test]
fn a_copy_into_another_record_is_proved_through_the_reference_it_sets() {
    let found = stores(
        r#"action plan_task(project_id: Project.id) fails ProjectNotFound -> Task {
    var project: Project = attempt Project.get(project_id)
    return Task.create {
        set: {
            company_id: project.company_id
            project_id: project.id
            title: project.name
            cost: project.spent
        }
    }
}

action plan_found_task(company_id: Company.id, project_id: Project.id) fails CompanyNotFound, ProjectNotFound -> Task {
    var project: Project = attempt Project.get(project_id)
    return attempt Task.create {
        set: {
            company_id: company_id
            project_id: project_id
            title: project.name
            cost: 0
        }
    }
}

action plan_tenant_task(company_id: Company.id, project_id: Project.id, other_id: Project.id) fails ProjectNotFound -> Task {
    var other: Project = attempt Project.get(other_id)
    return attempt Task.create {
        set: {
            company_id: other.company_id
            project_id: project_id
            title: other.name
            cost: 0
        }
    }
}

action price_task(company_id: Company.id, project_id: Project.id, rate_id: Rate.id) fails CompanyNotFound, ProjectNotFound, RateNotFound -> Task {
    var rate: Rate = attempt Rate.get(rate_id)
    return attempt Task.create {
        set: {
            company_id: company_id
            project_id: project_id
            title: "Priced"
            cost: rate.amount
        }
    }
}

action budget_task(project_id: Project.id) fails ProjectNotFound -> Task {
    var project: Project = attempt Project.get(project_id)
    return Task.create {
        set: {
            company_id: project.company_id
            project_id: project.id
            title: project.name
            cost: project.budget
        }
    }
}

action borrow_name(company_id: Company.id, project_id: Project.id, other_id: Project.id) fails CompanyNotFound, ProjectNotFound -> Task {
    var other: Project = attempt Project.get(other_id)
    return attempt Task.create {
        set: {
            company_id: company_id
            project_id: project_id
            title: other.name
            cost: 0
        }
    }
}
"#,
    );
    assert_stores(
        &found,
        &[
            "`budget_task` stores `Project.budget` in `Task.cost`, which not everyone who may read `cost` may read",
            "`borrow_name` stores `Project.name` in `Task.title`, which not everyone who may read `title` may read",
        ],
        &[
            "plan_task",
            "plan_found_task",
            "plan_tenant_task",
            "price_task",
        ],
    );
}

#[test]
fn a_counter_reads_the_record_it_changes() {
    let found = stores(
        r#"action spend(project_id: Project.id) fails ProjectNotFound -> Project {
    return attempt Project.update(project_id) {
        set: {
            spent: Amount(Project.spent + 1)
        }
    }
}

action spend_budget(project_id: Project.id) fails ProjectNotFound -> Project {
    return attempt Project.update(project_id) {
        set: {
            spent: Amount(Project.budget + 1)
        }
    }
}

action forecast_budget(project_id: Project.id) fails ProjectNotFound -> Project {
    return attempt Project.update(project_id) {
        set: {
            forecast: Amount(Project.budget + 1)
        }
    }
}
"#,
    );
    assert_stores(
        &found,
        &[
            "`spend_budget` stores `Project.budget` in `Project.spent`, which not everyone who may read `spent` may read",
        ],
        &["spend", "forecast_budget"],
    );
}

#[test]
fn what_decides_a_stored_value_is_stored_with_it() {
    // An action is all-or-nothing, so what a `reject` tests decides every
    // value it stores, before it or after.
    let found = stores(
        r#"type OverBudget = Failure {
    kind: FailureKind.conflict
    message: "The project is over budget."
}

action flag_budget(project_id: Project.id) fails ProjectNotFound -> Project {
    var project: Project = attempt Project.get(project_id)
    if (project.budget > 100) {
        return Project.update(project) {
            set: {
                spent: 1
            }
        }
    }
    return project
}

action flag_spent(project_id: Project.id) fails ProjectNotFound -> Project {
    var project: Project = attempt Project.get(project_id)
    if (project.spent > 100) {
        return Project.update(project) {
            set: {
                spent: 1
            }
        }
    }
    return project
}

action close_over_budget(task_id: Task.id) fails TaskNotFound, ProjectNotFound -> Task {
    var task: Task = attempt Task.get(task_id)
    var project: Project = attempt Project.get(task.project_id)
    if (project.budget > 100) {
        return Task.close(task)
    }
    return task
}

action spend_unless_over(project_id: Project.id) fails ProjectNotFound, OverBudget -> Project {
    var project: Project = attempt Project.get(project_id)
    var spent: Project = Project.update(project) {
        set: {
            spent: project.spent
        }
    }
    if (project.budget > 100) {
        reject OverBudget
    }
    return spent
}

action forecast_unless_over(project_id: Project.id) fails ProjectNotFound, OverBudget -> Project {
    var project: Project = attempt Project.get(project_id)
    var forecast: Project = Project.update(project) {
        set: {
            forecast: project.forecast
        }
    }
    if (project.budget > 100) {
        reject OverBudget
    }
    return forecast
}

action close_over_spent(task_id: Task.id) fails TaskNotFound, ProjectNotFound -> Task {
    var task: Task = attempt Task.get(task_id)
    var project: Project = attempt Project.get(task.project_id)
    if (project.spent > 100) {
        return Task.close(task)
    }
    return task
}
"#,
    );
    assert_stores(
        &found,
        &[
            "`flag_budget` stores a value decided by `Project.budget` in `Project.spent`, which not everyone who may read `spent` may read",
            "`close_over_budget` stores a value decided by `Project.budget` in `Task.closed`, which not everyone who may read `closed` may read",
            "`spend_unless_over` stores a value decided by `Project.budget` in `Project.spent`, which not everyone who may read `spent` may read",
            "`spend_unless_over` raises `OverBudget` on a condition that tests `Project.budget`, which not every caller may read",
            "`forecast_unless_over` raises `OverBudget` on a condition that tests `Project.budget`, which not every caller may read",
        ],
        &["flag_spent", "close_over_spent"],
    );
}

#[test]
fn a_patch_stores_each_field_its_input_supplies() {
    let found = stores(
        r#"type ProjectChanges = Input {
    spent: Amount {
        optional: true
    }
    forecast: Amount {
        optional: true
    }
}

action amend(project: Project, changes: ProjectChanges) -> Project {
    return Project.update(project) {
        patch: changes
    }
}

action amend_from_budget(project_id: Project.id) fails ProjectNotFound -> Project {
    var project: Project = attempt Project.get(project_id)
    var changes: ProjectChanges = ProjectChanges {
        spent: project.budget
    }
    return Project.update(project) {
        patch: changes
    }
}

action forecast_from_budget(project_id: Project.id) fails ProjectNotFound -> Project {
    var project: Project = attempt Project.get(project_id)
    var changes: ProjectChanges = ProjectChanges {
        forecast: project.budget
    }
    return Project.update(project) {
        patch: changes
    }
}
"#,
    );
    assert_stores(
        &found,
        &[
            "`amend_from_budget` stores `Project.budget` in `Project.spent`, which not everyone who may read `spent` may read",
        ],
        &["amend", "forecast_from_budget"],
    );
}

#[test]
fn a_value_passed_to_an_action_that_stores_it_is_checked_where_it_is_passed() {
    let found = stores(
        r#"action set_cost(task: Task, cost: Amount) -> Task {
    return Task.update(task) {
        set: {
            cost: cost
        }
    }
}

action cost_from(task: Task, cost: Amount) -> Task {
    return set_cost(task, cost)
}

action cost_from_budget(task_id: Task.id) fails TaskNotFound, ProjectNotFound -> Task {
    var task: Task = attempt Task.get(task_id)
    var project: Project = attempt Project.get(task.project_id)
    return set_cost(task, project.budget)
}

action cost_from_spent(task_id: Task.id) fails TaskNotFound, ProjectNotFound -> Task {
    var task: Task = attempt Task.get(task_id)
    var project: Project = attempt Project.get(task.project_id)
    return set_cost(task, project.spent)
}

action cost_through(task_id: Task.id) fails TaskNotFound, ProjectNotFound -> Task {
    var task: Task = attempt Task.get(task_id)
    var project: Project = attempt Project.get(task.project_id)
    return cost_from(task, project.budget)
}

action cost_from_other(task_id: Task.id, other_id: Project.id) fails TaskNotFound, ProjectNotFound -> Task {
    var task: Task = attempt Task.get(task_id)
    var other: Project = attempt Project.get(other_id)
    return set_cost(task, other.spent)
}
"#,
    );
    assert_stores(
        &found,
        &[
            "`cost_from_budget` passes `Project.budget` to `set_cost`, which stores it in `Task.cost`, though not everyone who may read `cost` may read it",
            "`cost_through` passes `Project.budget` to `cost_from`, which stores it in `Task.cost`, though not everyone who may read `cost` may read it",
            "`cost_from_other` passes `Project.spent` to `set_cost`, which stores it in `Task.cost`, though not everyone who may read `cost` may read it",
        ],
        &["set_cost", "cost_from", "cost_from_spent"],
    );
}

#[test]
fn a_create_in_a_loop_stores_each_item_and_what_decided_it() {
    let found = stores(
        r#"action plan_all(company_id: Company.id) {
    var projects: List<Project> = company_projects(company_id)
    for project in projects {
        var task: Task = Task.create {
            set: {
                company_id: project.company_id
                project_id: project.id
                title: project.name
                cost: project.spent
            }
        }
        assert task.closed == false
    }
}

action cost_all(company_id: Company.id) {
    var projects: List<Project> = company_projects(company_id)
    for project in projects {
        var task: Task = Task.create {
            set: {
                company_id: project.company_id
                project_id: project.id
                title: project.name
                cost: project.budget
            }
        }
        assert task.closed == false
    }
}

action plan_funded(company_id: Company.id) {
    var projects: List<Project> = company_projects(company_id)
    for project in projects {
        if (project.budget > 0) {
            var task: Task = Task.create {
                set: {
                    company_id: project.company_id
                    project_id: project.id
                    title: project.name
                    cost: 0
                }
            }
            assert task.closed == false
        }
    }
}

action plan_spent(company_id: Company.id) {
    var projects: List<Project> = company_projects(company_id)
    for project in projects {
        if (project.spent > 0) {
            var task: Task = Task.create {
                set: {
                    company_id: project.company_id
                    project_id: project.id
                    title: project.name
                    cost: 0
                }
            }
            assert task.closed == false
        }
    }
}

action plan_crossed(company_id: Company.id) {
    var projects: List<Project> = company_projects(company_id)
    for first in projects {
        for second in projects {
            var task: Task = Task.create {
                set: {
                    company_id: second.company_id
                    project_id: second.id
                    title: "Crossed"
                    cost: first.spent
                }
            }
            assert task.closed == false
        }
    }
}

action plan_carried(company_id: Company.id) {
    var projects: List<Project> = company_projects(company_id)
    var mut title: Label = "First"
    for project in projects {
        var task: Task = Task.create {
            set: {
                company_id: project.company_id
                project_id: project.id
                title: title
                cost: 0
            }
        }
        assert task.closed == false
        title = project.name
    }
}

action plan_until_funded(company_id: Company.id) -> Amount {
    var projects: List<Project> = company_projects(company_id)
    for project in projects {
        var task: Task = Task.create {
            set: {
                company_id: project.company_id
                project_id: project.id
                title: project.name
                cost: 0
            }
        }
        assert task.closed == false
        if (project.budget > 100) {
            return 1
        }
    }
    return 0
}
"#,
    );
    assert_stores(
        &found,
        &[
            "`cost_all` stores `Project.budget` in `Task.cost`, which not everyone who may read `cost` may read",
            "`plan_funded` stores a value decided by `Project.budget` in `Task.company_id`, which not everyone who may read `company_id` may read",
            "`plan_crossed` stores `Project.spent` in `Task.cost`, which not everyone who may read `cost` may read",
            "`plan_carried` stores `Project.name` in `Task.title`, which not everyone who may read `title` may read",
            "`plan_until_funded` stores a value decided by `Project.budget` in `Task.company_id`, which not everyone who may read `company_id` may read",
        ],
        &["plan_all", "plan_spent"],
    );
}

#[test]
fn an_event_is_server_work_so_emitting_a_restricted_value_stores_nothing_a_caller_reads() {
    let found = stores(
        r#"type BudgetSeen = Event {
    budget: Amount
}

action announce_budget(project_id: Project.id) fails ProjectNotFound -> Project {
    var project: Project = attempt Project.get(project_id)
    emit BudgetSeen {
        budget: project.budget
    }
    return project
}
"#,
    );
    assert_stores(&found, &[], &["announce_budget"]);
}
