//! Policy's checks (area 9, chunk 4.4) beyond the one example per rule on
//! the page: which fields an ordinary update and a listed action may
//! change, what a route reaches through the actions it calls, when the
//! route checks wait for the routes to read cleanly, tenancy's reference,
//! and what isn't built yet; and, from chunk 4.4b, a field's own policy: what
//! it may name, the outputs and creates it decides, and the routes that
//! write a field nobody may change.

use jadpo_db::Layout;

use crate::support;

/// A principal, a note its owner keeps, and whatever follows.
fn with(code: &str) -> Vec<String> {
    let text = format!(
        r#"principal {{
    user: {{
        subject: Person.handle
    }}
}}

authentication {{
    api_token: {{}}
}}

type Handle = Text {{
    max_length: 40
}}

type NoteText = Text {{
    max_length: 200
}}

type NoteView = Output {{
    text: NoteText
}}

entity Person {{
    id: Uuid
    handle: Handle {{
        unique: true
    }}

    policy: {{
        read: [id]
    }}
}}

entity Note {{
    id: Uuid
    owner_id: Person.id {{
        on_delete: OnDelete.cascade
    }}
    text: NoteText
    stamp: NoteText {{
        immutable: true
    }}

    policy: {{
        create: [owner_id]
        read: [owner_id]
        update: [owner_id]
    }}
}}

{code}"#
    );
    support::problems(Layout::Loose, &[("main.jadpo", text.as_str())])
}

const GIVE: &str = r#"action give_note(note_id: Note.id, to: Person.id) fails NoteNotFound, PersonNotFound -> NoteView {
    var note: Note = attempt Note.update(note_id) {
        set: {
            owner_id: to
        }
    }
    return NoteView {
        text: note.text
    }
}
"#;

#[test]
fn only_a_listed_action_changes_a_field_policy_reaches_the_caller_through() {
    let refused = with(GIVE);
    assert_eq!(refused.len(), 1, "{refused:#?}");
    assert!(
        refused[0].contains("type.field_update_forbidden"),
        "{refused:#?}"
    );
    let listed = with(&format!(
        "{GIVE}\npolicy {{\n    actions: {{\n        give_note: [Note.owner_id]\n    }}\n}}\n"
    ));
    assert_eq!(listed, Vec::<String>::new());
}

#[test]
fn an_immutable_field_never_changes_even_in_a_listed_action() {
    let found = with(
        r#"action restamp(note_id: Note.id, stamp: NoteText) fails NoteNotFound -> NoteView {
    var note: Note = attempt Note.update(note_id) {
        set: {
            stamp: stamp
        }
    }
    return NoteView {
        text: note.text
    }
}

policy {
    actions: {
        restamp: [Note.owner_id]
    }
}
"#,
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("type.field_update_forbidden"),
        "{found:#?}"
    );
    assert!(found[0].contains("`Note.stamp`"), "{found:#?}");
}

#[test]
fn a_route_reaches_what_the_actions_it_calls_reach() {
    let found = with(
        r#"type Wipe = Input {
    note_id: Note.id
}

action wipe_inner(note_id: Note.id) fails NoteNotFound -> Unit {
    attempt Note.delete(note_id)
}

action wipe(input: Wipe) fails NoteNotFound -> Unit {
    attempt wipe_inner(input.note_id)
}

routes Notes {
    prefix: /notes

    POST /wipe {
        input: Wipe
        status: SuccessStatus.no_content
        run: wipe(input)
    }
}
"#,
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("policy.effect_ungranted") && found[0].contains("deletes `Note` records"),
        "{found:#?}"
    );
}

#[test]
fn signing_in_or_out_reaches_enough_to_need_no_listing() {
    let found = with(
        r#"action sign_out() -> Unit {
    sessions.end()
}

routes Account {
    prefix: /account

    POST /sign-out {
        status: SuccessStatus.no_content
        run: sign_out()
    }
}
"#,
    );
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn the_route_checks_wait_for_the_routes_to_read_cleanly() {
    let found = with(
        r#"type Health = Output {
    ok: Bool
}

action health() -> Health {
    return Health {
        ok: true
    }
}

routes Monitoring {
    prefix: /health

    GET / {
        output: Health
        run: health()
    }
}
"#,
    );
    let rules: Vec<&str> = found
        .iter()
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect();
    assert_eq!(rules, ["route.run_kind:"], "{found:#?}");
}

#[test]
fn a_line_under_actions_names_an_action() {
    let found = with(
        "policy {\n    actions: {\n        nowhere.publish: [Access.authenticated]\n    }\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("policy.binding_invalid") && found[0].contains("isn't an action"),
        "{found:#?}"
    );
}

#[test]
fn with_a_tenant_every_entity_refers_to_it_or_says_it_is_shared() {
    let found = support::problems(
        Layout::Loose,
        &[(
            "main.jadpo",
            r#"principal {
    user: {
        subject: Person.handle
    }
}

type Handle = Text {
    max_length: 40
}

type ClubRole = Enum {
    member
}

entity Person {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        tenant: Tenancy.shared
    }
}

entity Club {
    id: Uuid
}

entity ClubMembership {
    id: Uuid
    club_id: Club.id {
        on_delete: OnDelete.cascade
    }
    person_id: Person.id {
        on_delete: OnDelete.cascade
    }
    role: ClubRole

    membership: {
        scope: club_id
        member: person_id
        role: role
    }
}

entity Notice {
    id: Uuid
}

policy {
    tenant: Club
}
"#,
        )],
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("policy.scope_missing") && found[0].contains("`Notice`"),
        "{found:#?}"
    );
}

#[test]
fn scopes_wait_for_their_chunk_and_a_fields_own_policy_reads() {
    let found = with(
        r#"entity Draft {
    id: Uuid
    owner_id: Person.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText {
        policy: {
            read: [owner_id]
        }
    }

    policy: {
        read: [owner_id]
        scopes: {
            read: OAuthScope.drafts_read
        }
    }
}
"#,
    );
    let rules: Vec<&str> = found
        .iter()
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect();
    assert_eq!(rules, ["toolchain.not_built_yet:"], "{found:#?}");
}

/// The rules each problem in `found` names, in order.
fn rules_of(found: &[String]) -> Vec<&str> {
    found
        .iter()
        .filter_map(|line| line.split_whitespace().nth(1))
        .map(|rule| rule.trim_end_matches(':'))
        .collect()
}

#[test]
fn a_fields_policy_narrows_create_read_and_update_in_the_entitys_shape() {
    let found = with(
        r#"entity Draft {
    id: Uuid
    owner_id: Person.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText {
        policy: {
            delete: [owner_id]
            read: [owner_id]
            read: [owner_id]
        }
    }
    title: NoteText {
        policy: FieldAccess.public
    }
    summary: NoteText {
        policy: [owner_id]
    }

    policy: {
        read: [owner_id]
    }
}
"#,
    );
    assert_eq!(
        rules_of(&found),
        [
            "policy.effect_unknown",
            "policy.effect_duplicate",
            "policy.binding_invalid",
            "policy.binding_invalid"
        ],
        "{found:#?}"
    );
    assert!(found[0].contains("`delete`"), "{found:#?}");
}

#[test]
fn a_field_can_only_take_away_from_its_records_grants() {
    // Anyone signed in includes the owner, so a field may narrow to them;
    // the other way round lets in more, as does a list the record leaves
    // out.
    let found = with(
        r#"entity Draft {
    id: Uuid
    owner_id: Person.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText {
        policy: {
            read: [owner_id]
        }
    }
    title: NoteText {
        policy: {
            update: [owner_id]
        }
    }

    policy: {
        read: [owner_id, Access.authenticated]
    }
}

entity Memo {
    id: Uuid
    owner_id: Person.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText {
        policy: {
            read: [Access.authenticated]
        }
    }

    policy: {
        read: [owner_id]
    }
}
"#,
    );
    assert_eq!(
        rules_of(&found),
        ["policy.field_widens_entity", "policy.field_widens_entity"],
        "{found:#?}"
    );
    assert!(
        found[0].contains("`Draft.title` lets `owner_id` update it"),
        "{found:#?}"
    );
    assert!(
        found[1].contains("`Memo.text` lets `Access.authenticated` read it"),
        "{found:#?}"
    );
}

/// A company-scoped project with members, a budget only editors read and
/// change, a reference no client may touch, and whatever follows.
fn company(code: &str) -> Vec<String> {
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

type Amount = Int {{
    min: 0
}}

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
    import_reference: Label? {{
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

    policy: {{
        read: [Role.owner, Role.editor, Role.viewer]
    }}
}}

type CompanyView = Output {{
    pick: [Company.id, Company.name]
}}

type ProjectCard = Output {{
    pick: [Project.id, Project.name]
}}

type BudgetView = Output {{
    pick: [Project.id, Project.budget]
}}

{code}"#
    );
    support::problems(Layout::Loose, &[("main.jadpo", text.as_str())])
}

#[test]
fn a_query_sends_only_fields_everyone_who_may_call_it_may_read() {
    let found = company(
        r#"query budgets(company_id: Company.id) -> List<BudgetView> {
    from: Project
    where: Project.company_id == company_id
    order: [Project.name asc]
    limit: 100
}

query projects(company_id: Company.id) -> List<ProjectCard> {
    from: Project
    where: Project.company_id == company_id
    order: [Project.name asc]
    limit: 100
}
"#,
    );
    assert_eq!(
        rules_of(&found),
        ["policy.output_field_unproved"],
        "{found:#?}"
    );
    assert!(
        found[0].contains("`budgets` sends `Project.budget`"),
        "{found:#?}"
    );
}

#[test]
fn an_include_reads_a_parent_only_when_every_reader_may() {
    // A company's role applies to the company a project refers to, and a
    // membership's member holds a role in its company; a task refers only
    // to a project of its own company (chunk 4.7), so a role held in the
    // task's company is held in its project's. The person an assignment
    // points at holds no role the proof can see.
    let found = company(
        r#"type ProjectWithCompany = Output {
    pick: [Project.id, Project.name]
    company: CompanyView
}

query project_with_company(project_id: Project.id) -> ProjectWithCompany? {
    from: Project
    where: Project.id == project_id
    include: {
        company: Project.company_id
    }
}

type MembershipView = Output {
    pick: [Membership.id]
    company: CompanyView
}

query my_memberships(person_id: Person.id) -> List<MembershipView> {
    from: Membership
    where: Membership.person_id == person_id
    order: [Membership.id asc]
    limit: 50
    include: {
        company: Membership.company_id
    }
}

type TaskView = Output {
    pick: [Task.id, Task.title]
    project: ProjectCard
}

query task_with_project(task_id: Task.id) -> TaskView? {
    from: Task
    where: Task.id == task_id
    include: {
        project: Task.project_id
    }
}

entity Assignment {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    project_id: Project.id {
        on_delete: OnDelete.cascade
    }
    person_id: Person.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        read: [person_id]
    }
}

type AssignmentView = Output {
    pick: [Assignment.id]
    project: ProjectCard
}

query my_assignment(assignment_id: Assignment.id) -> AssignmentView? {
    from: Assignment
    where: Assignment.id == assignment_id
    include: {
        project: Assignment.project_id
    }
}
"#,
    );
    assert_eq!(
        rules_of(&found),
        ["policy.output_field_unproved"],
        "{found:#?}"
    );
    assert!(
        found[0].contains("`my_assignment` includes the `Project` each `Assignment` refers to"),
        "{found:#?}"
    );
}

#[test]
fn an_include_of_a_tenants_parent_proves_from_the_childs_tenancy() {
    // Anyone signed in, within the tenant, reads a comment, and anyone
    // signed in reads its project: the project is in the comment's tenant.
    // A reference to the tenant itself isn't one inside it: a transfer's
    // second company is another tenant.
    let found = company(
        r#"entity Note {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    task_id: Task.id {
        on_delete: OnDelete.cascade
    }
    text: Label

    policy: {
        read: [Access.authenticated]
    }
}

entity Board {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    note_id: Note.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        read: [Role.viewer]
    }
}

type NoteCard = Output {
    pick: [Note.id, Note.text]
}

type BoardView = Output {
    pick: [Board.id]
    note: NoteCard
}

query board_card(board_id: Board.id) -> BoardView? {
    from: Board
    where: Board.id == board_id
    include: {
        note: Board.note_id
    }
}

entity Transfer {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    to_company_id: Company.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        read: [Role.owner(company_id)]
        tenant: company_id
    }
}

type TransferView = Output {
    pick: [Transfer.id]
    to: CompanyView
}

query transfer_card(transfer_id: Transfer.id) -> TransferView? {
    from: Transfer
    where: Transfer.id == transfer_id
    include: {
        to: Transfer.to_company_id
    }
}
"#,
    );
    assert_eq!(
        rules_of(&found),
        ["policy.output_field_unproved"],
        "{found:#?}"
    );
    assert!(
        found[0].contains("`transfer_card` includes the `Company` each `Transfer` refers to"),
        "{found:#?}"
    );
}

#[test]
fn a_query_filters_and_orders_only_on_fields_every_caller_may_read() {
    // Which projects pass `budget > 0`, and the order budgets put them in,
    // show each budget to viewers, who may not read it.
    let found = company(
        r#"query funded(company_id: Company.id) -> List<ProjectCard> {
    from: Project
    where: Project.company_id == company_id
    where: Project.budget > 0
    order: [Project.name asc]
    limit: 100
}

query by_budget(company_id: Company.id) -> List<ProjectCard> {
    from: Project
    where: Project.company_id == company_id
    order: [Project.budget desc]
    limit: 100
}

type CompanyProjects = Output {
    pick: [Company.id]
    projects: List<ProjectCard>
}

query company_funded(company_id: Company.id) -> CompanyProjects? {
    from: Company
    where: Company.id == company_id
    include: {
        projects: {
            via: Project.company_id
            where: Project.budget > 0
            order: [Project.name asc]
            limit: 50
        }
    }
}

query named(company_id: Company.id, name: Label) -> List<ProjectCard> {
    from: Project
    where: Project.company_id == company_id
    where: Project.name == name
    order: [Project.name asc]
    limit: 100
}
"#,
    );
    assert_eq!(
        rules_of(&found),
        [
            "policy.output_field_unproved",
            "policy.output_field_unproved",
            "policy.output_field_unproved"
        ],
        "{found:#?}"
    );
    let all = found.join("\n");
    for expected in [
        "`funded` filters on `Project.budget`, which not every caller may read",
        "`by_budget` orders by `Project.budget`, which not every caller may read",
        "`company_funded` filters on `Project.budget`, which not every caller may read",
    ] {
        assert!(all.contains(expected), "{expected}\n{all}");
    }
}

#[test]
fn a_value_keeps_its_fields_rules_through_variables_objects_and_calls() {
    let found = company(
        r#"type Kept = Object {
    amount: Project.budget
}

action rename(project_id: Project.id, name: Label) fails ProjectNotFound -> BudgetView {
    var project: Project = attempt Project.update(project_id) {
        set: {
            name: name
        }
    }
    var kept: Kept = Kept {
        amount: project.budget
    }
    var amount: Project.budget = kept.amount
    return BudgetView {
        id: project.id
        budget: amount
    }
}

action set_budget(project_id: Project.id, budget: Amount) fails ProjectNotFound -> BudgetView {
    var project: Project = attempt Project.update(project_id) {
        set: {
            budget: budget
        }
    }
    return BudgetView {
        id: project.id
        budget: project.budget
    }
}

action show(project: Project) -> BudgetView {
    return BudgetView {
        id: project.id
        budget: project.budget
    }
}

action look(project_id: Project.id) fails ProjectNotFound -> BudgetView {
    var project: Project = attempt Project.get(project_id)
    return show(project)
}
"#,
    );
    // `set_budget`'s callers passed the budget's own `update:`, which
    // editors alone pass; `show`'s project is checked where it is called.
    assert_eq!(
        rules_of(&found),
        [
            "policy.output_field_unproved",
            "policy.output_field_unproved"
        ],
        "{found:#?}"
    );
    assert!(
        found[0].contains(
            "`BudgetView.budget` carries `Project.budget`, which not every caller of `rename`"
        ),
        "{found:#?}"
    );
    assert!(
        found[1].contains("`look` passes `Project.budget` to `show`"),
        "{found:#?}"
    );
}

#[test]
fn a_create_sets_the_user_field_its_grant_needs_from_the_caller() {
    let found = with(
        r#"entity Draft {
    id: Uuid
    owner_id: Person.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText

    policy: {
        create: [owner_id]
        read: [owner_id]
    }
}

entity Memo {
    id: Uuid
    owner_id: Person.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText

    policy: {
        create: [owner_id, Access.authenticated]
        read: [owner_id]
    }
}

action start_draft(me: Principal.user, text: NoteText) fails PersonNotFound -> Draft {
    var mine: Person.id = me.id
    return attempt Draft.create {
        set: {
            owner_id: mine
            text: text
        }
    }
}

action draft_for(owner: Person.id, text: NoteText) fails PersonNotFound -> Draft {
    return attempt Draft.create {
        set: {
            owner_id: owner
            text: text
        }
    }
}

action memo_for(owner: Person.id, text: NoteText) fails PersonNotFound -> Memo {
    return attempt Memo.create {
        set: {
            owner_id: owner
            text: text
        }
    }
}
"#,
    );
    assert_eq!(
        rules_of(&found),
        ["policy.role_field_create_forbidden"],
        "{found:#?}"
    );
    assert!(
        found[0].contains("`draft_for` sets `Draft.owner_id` from the request"),
        "{found:#?}"
    );
}

#[test]
fn a_route_that_sets_a_field_no_client_may_change_is_refused() {
    let found = company(
        r#"type Retag = Input {
    reference: Label
}

action retag(project_id: Project.id, input: Retag) fails ProjectNotFound -> ProjectCard {
    var project: Project = attempt Project.update(project_id) {
        set: {
            import_reference: input.reference
        }
    }
    return ProjectCard {
        id: project.id
        name: project.name
    }
}

routes Projects {
    prefix: /projects

    PATCH /{project_id: Project.id} {
        input: Retag
        output: ProjectCard
        run: retag(project_id, input)
    }
}
"#,
    );
    assert_eq!(rules_of(&found), ["policy.effect_ungranted"], "{found:#?}");
    assert!(
        found[0].contains("sets `Project.import_reference`, which nobody may"),
        "{found:#?}"
    );
}

#[test]
fn a_failures_client_fields_keep_their_values_rules_too() {
    let found = company(
        r#"type OverBudget = Failure {
    kind: FailureKind.conflict
    message: "That is over budget."
    client: {
        budget: Project.budget
    }
    internal: {
        spent: Project.budget
    }
}

action spend(project_id: Project.id, amount: Amount) fails ProjectNotFound, OverBudget -> ProjectCard {
    var project: Project = attempt Project.get(project_id)
    if (amount > project.budget) {
        reject OverBudget {
            budget: project.budget
            spent: project.budget
        }
    }
    return ProjectCard {
        id: project.id
        name: project.name
    }
}
"#,
    );
    assert_eq!(
        rules_of(&found),
        ["policy.output_field_unproved"],
        "{found:#?}"
    );
    assert!(
        found[0].contains("`OverBudget.budget` carries `Project.budget`"),
        "{found:#?}"
    );
}

#[test]
fn a_transitions_condition_tests_only_fields_every_caller_may_read() {
    // Closing a ticket that is already closed is refused, which shows an
    // owner its stage, which only editors may read.
    let found = company(
        r#"type Stage = Enum {
    open
    closed
}

type Closed = Failure {
    kind: FailureKind.conflict
    message: "That ticket is closed."
}

entity Ticket {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    stage: Stage {
        policy: {
            read: [Role.editor]
        }
    }

    lifecycle: {
        initial: {
            stage: Stage.open
        }
        transitions: {
            close: {
                when: stage == Stage.open
                set: {
                    stage: Stage.closed
                }
            }
        }
    }

    policy: {
        read: [Role.owner, Role.editor, Role.viewer]
        update: [Role.owner, Role.editor]
    }
}

action close_ticket(ticket_id: Ticket.id) fails TicketNotFound, Closed -> Unit {
    var ticket: Ticket = attempt Ticket.close(ticket_id) {
        conflict: {
            Ticket.close: Closed
        }
    }
    assert ticket.id == ticket_id
}
"#,
    );
    assert_eq!(
        rules_of(&found),
        ["policy.output_field_unproved"],
        "{found:#?}"
    );
    assert!(
        found[0].contains(
            "`close_ticket` runs `Ticket.close`, whose `when:` tests `Ticket.stage`, which not every caller may read"
        ),
        "{found:#?}"
    );
}
