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
    company_listing("", code)
}

/// [`company`], with `actions` (the lines under `actions:`) in its root
/// `policy { }`.
fn company_listing(actions: &str, code: &str) -> Vec<String> {
    let actions = if actions.is_empty() {
        String::new()
    } else {
        format!("\n    actions: {{\n{actions}    }}")
    };
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
    tenant: Company{actions}
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
    // task's company is held in its project's. An assignment every company
    // shares says nothing of the company its person belongs to, so its
    // person holds no role the proof can see.
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
        tenant: Tenancy.shared
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
    // The project sent when the spend fits shows that it fits, which the
    // budget decides (chunk 4.8).
    assert_eq!(
        rules_of(&found),
        [
            "policy.output_field_unproved",
            "policy.output_field_unproved"
        ],
        "{found:#?}"
    );
    assert!(
        found[0].contains("`OverBudget.budget` carries `Project.budget`"),
        "{found:#?}"
    );
    assert!(
        found[1].contains("`ProjectCard.id` carries `Project.budget`"),
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

// --- chunk 4.8: the Phase 4 review's policy findings ---------------------------

#[test]
fn a_line_held_in_a_record_lets_nobody_call_an_action_that_changes_none() {
    // `report` changes no record a company's role or a membership's member
    // could be checked on, so its line lets nobody in; anyone signed in, or
    // a line whose role applies to the project `start` creates, does.
    let found = company_listing(
        "        report: [Role.owner]\n        seat_report: [Membership.person_id]\n        open_report: [Role.owner, Access.authenticated]\n        start: [Role.owner]\n",
        r#"type Report = Output {
    ok: Bool
}

action report() -> Report {
    return Report {
        ok: true
    }
}

action seat_report() -> Report {
    return Report {
        ok: true
    }
}

action open_report() -> Report {
    return Report {
        ok: true
    }
}

type Start = Input {
    company_id: Company.id
    name: Label
}

action start(input: Start) fails CompanyNotFound -> ProjectCard {
    var project: Project = attempt Project.create {
        set: {
            company_id: input.company_id
            name: input.name
            budget: 0
            import_reference: none
        }
    }
    return ProjectCard {
        id: project.id
        name: project.name
    }
}

routes Reports {
    prefix: /reports

    POST / {
        output: Report
        run: report()
    }

    POST /seats {
        output: Report
        run: seat_report()
    }

    POST /open {
        output: Report
        run: open_report()
    }

    POST /projects {
        input: Start
        output: ProjectCard
        status: SuccessStatus.created
        run: start(input)
    }
}
"#,
    );
    assert_eq!(
        rules_of(&found),
        ["policy.effect_ungranted", "policy.effect_ungranted"],
        "{found:#?}"
    );
    assert!(
        found[0].contains("`POST /reports` runs `report`, which nobody may call"),
        "{found:#?}"
    );
    assert!(
        found[1].contains("`POST /reports/seats` runs `seat_report`, which nobody may call"),
        "{found:#?}"
    );
}

#[test]
fn an_output_a_function_builds_is_checked_where_it_is_called() {
    let found = company(
        r#"function budget_of(project: Project) -> BudgetView {
    return BudgetView {
        id: project.id
        budget: project.budget
    }
}

action peek(project_id: Project.id) fails ProjectNotFound -> BudgetView {
    var project: Project = attempt Project.get(project_id)
    return budget_of(project)
}

function card_of(project: Project) -> ProjectCard {
    return ProjectCard {
        id: project.id
        name: project.name
    }
}

action glance(project_id: Project.id) fails ProjectNotFound -> ProjectCard {
    var project: Project = attempt Project.get(project_id)
    return card_of(project)
}
"#,
    );
    assert_eq!(
        rules_of(&found),
        ["policy.output_field_unproved"],
        "{found:#?}"
    );
    assert!(
        found[0].contains("`peek` passes `Project.budget` to `budget_of`"),
        "{found:#?}"
    );
}

#[test]
fn a_value_an_if_a_match_a_filter_or_a_count_decides_keeps_what_decided_it() {
    // Each output below is built from constants, but which one, or the
    // number counted, shows whether a budget is over a line, which only
    // editors may read; each action's problem is at the first value the
    // budget decides.
    let found = company(
        r#"type Flag = Output {
    high: Bool
}

type Tally = Output {
    funded: Int
}

query company_records(company_id: Company.id) -> List<Project> {
    from: Project
    where: Project.company_id == company_id
    order: [Project.name asc]
    limit: 100
}

action flag_if(project_id: Project.id) fails ProjectNotFound -> Flag {
    var project: Project = attempt Project.get(project_id)
    if (project.budget > 100) {
        return Flag {
            high: true
        }
    }
    return Flag {
        high: false
    }
}

action flag_match(project_id: Project.id) fails ProjectNotFound -> Flag {
    var project: Project = attempt Project.get(project_id)
    return match project.budget > 100 {
        true -> Flag {
            high: true
        }
        false -> Flag {
            high: false
        }
    }
}

action count_funded(company_id: Company.id) -> Tally {
    var projects: List<Project> = company_records(company_id)
    return Tally {
        funded: list.count(list.filter(projects, project -> project.budget > 0))
    }
}

action funded_cards(company_id: Company.id) -> List<ProjectCard> {
    var projects: List<Project> = company_records(company_id)
    var funded: List<Project> = list.filter(projects, project -> project.budget > 0)
    return list.map(funded, project -> ProjectCard {
        id: project.id
        name: project.name
    })
}

action count_loop(company_id: Company.id) -> Tally {
    var projects: List<Project> = company_records(company_id)
    var mut funded: Int = 0
    for project in projects {
        if (project.budget > 0) {
            funded = funded + 1
        }
    }
    return Tally {
        funded: funded
    }
}

action flag_none(project_id: Project.id) fails ProjectNotFound -> Flag? {
    var project: Project = attempt Project.get(project_id)
    if (project.budget > 100) {
        return none
    }
    return Flag {
        high: false
    }
}

action count_all(company_id: Company.id) -> Tally {
    var projects: List<Project> = company_records(company_id)
    return Tally {
        funded: list.count(projects)
    }
}

query find_record(project_id: Project.id) -> Project? {
    from: Project
    where: Project.id == project_id
}

action flag_found(project_id: Project.id) -> Flag {
    var found: Project? = find_record(project_id)
    if (found == none) {
        return Flag {
            high: false
        }
    }
    return Flag {
        high: true
    }
}
"#,
    );
    let all = found.join("\n");
    assert_eq!(
        rules_of(&found),
        [
            "policy.output_field_unproved",
            "policy.output_field_unproved",
            "policy.output_field_unproved",
            "policy.output_field_unproved",
            "policy.output_field_unproved",
            "policy.output_field_unproved"
        ],
        "{all}"
    );
    for expected in [
        "`Flag.high` carries `Project.budget`, which not every caller of `flag_if` may read",
        "`Flag.high` carries `Project.budget`, which not every caller of `flag_match` may read",
        "`Tally.funded` carries `Project.budget`, which not every caller of `count_funded` may read",
        "`ProjectCard.id` carries `Project.budget`, which not every caller of `funded_cards` may read",
        "`Tally.funded` carries `Project.budget`, which not every caller of `count_loop` may read",
        "`Flag.high` carries `Project.budget`, which not every caller of `flag_none` may read",
    ] {
        assert!(all.contains(expected), "{expected}\n{all}");
    }
    assert!(!all.contains("`count_all`"), "{all}");
    assert!(!all.contains("`flag_found`"), "{all}");
}

#[test]
fn a_failure_raised_under_a_condition_shows_what_it_tests() {
    let found = company(
        r#"type OverBudget = Failure {
    kind: FailureKind.conflict
    message: "That is over budget."
}

action spend(project_id: Project.id, amount: Amount) fails ProjectNotFound, OverBudget -> ProjectCard {
    var project: Project = attempt Project.get(project_id)
    if (amount > project.budget) {
        reject OverBudget
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
        found[0].contains("`spend` raises `OverBudget` on a condition that tests `Project.budget`, which not every caller may read"),
        "{found:#?}"
    );
}

#[test]
fn a_value_stored_with_set_keeps_its_rules_in_the_record_written() {
    let found = company(
        r#"entity Jot {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    amount: Amount

    policy: {
        create: [Role.owner, Role.editor, Role.viewer]
        read: [Role.owner, Role.editor, Role.viewer]
    }
}

type JotView = Output {
    pick: [Jot.id, Jot.amount]
}

action note_budget(project_id: Project.id) fails ProjectNotFound -> JotView {
    var project: Project = attempt Project.get(project_id)
    var jot: Jot = Jot.create {
        set: {
            company_id: project.company_id
            amount: project.budget
        }
    }
    return JotView {
        id: jot.id
        amount: jot.amount
    }
}
"#,
    );
    // Storing it is refused too, since a viewer may read the jot later
    // (chunk 5.1d).
    assert_eq!(
        rules_of(&found),
        [
            "policy.output_field_unproved",
            "policy.output_field_unproved"
        ],
        "{found:#?}"
    );
    assert!(
        found[0].contains("`note_budget` stores `Project.budget` in `Jot.amount`"),
        "{found:#?}"
    );
    assert!(
        found[1].contains(
            "`JotView.amount` carries `Project.budget`, which not every caller of `note_budget`"
        ),
        "{found:#?}"
    );
}

/// A brief whose project only editors may know, and whatever follows.
const BRIEF: &str = r#"entity Brief {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    project_id: Project.id {
        on_delete: OnDelete.cascade
        policy: {
            read: [Role.editor]
        }
    }

    policy: {
        read: [Role.owner, Role.editor, Role.viewer]
    }
}

type BriefCard = Output {
    pick: [Brief.id]
}
"#;

#[test]
fn a_restricted_id_used_to_find_a_record_is_sent_like_an_output() {
    let found = company(&format!(
        r#"{BRIEF}
query project_card(project_id: Project.id) -> ProjectCard? {{
    from: Project
    where: Project.id == project_id
}}

action brief_project(brief_id: Brief.id) fails BriefNotFound, ProjectNotFound -> ProjectCard {{
    var brief: Brief = attempt Brief.get(brief_id)
    var project: Project = attempt Project.get(brief.project_id)
    return ProjectCard {{
        id: project.id
        name: project.name
    }}
}}

action brief_card(brief_id: Brief.id) fails BriefNotFound -> ProjectCard? {{
    var brief: Brief = attempt Brief.get(brief_id)
    return project_card(brief.project_id)
}}
"#
    ));
    assert_eq!(
        rules_of(&found),
        [
            "policy.output_field_unproved",
            "policy.output_field_unproved"
        ],
        "{found:#?}"
    );
    let all = found.join("\n");
    for expected in [
        "`brief_project` finds a `Project` by `Brief.project_id`, which not every caller may read",
        "`brief_card` runs `project_card` with `Brief.project_id`, which not every caller may read",
    ] {
        assert!(all.contains(expected), "{expected}\n{all}");
    }
}

#[test]
fn an_include_through_a_restricted_reference_shows_it() {
    let found = company(&format!(
        r#"{BRIEF}
type BriefView = Output {{
    pick: [Brief.id]
    project: ProjectCard
}}

query brief_view(brief_id: Brief.id) -> BriefView? {{
    from: Brief
    where: Brief.id == brief_id
    include: {{
        project: Brief.project_id
    }}
}}

type ProjectBriefs = Output {{
    pick: [Project.id]
    briefs: List<BriefCard>
}}

query project_briefs(project_id: Project.id) -> ProjectBriefs? {{
    from: Project
    where: Project.id == project_id
    include: {{
        briefs: {{
            via: Brief.project_id
            order: [Brief.id asc]
            limit: 50
        }}
    }}
}}
"#
    ));
    assert_eq!(
        rules_of(&found),
        [
            "policy.output_field_unproved",
            "policy.output_field_unproved"
        ],
        "{found:#?}"
    );
    let all = found.join("\n");
    for expected in [
        "`brief_view` includes the `Project` each `Brief` refers to through `project_id`, which not every caller may read",
        "`project_briefs` includes the `Brief`s that refer to each `Project` through `project_id`, which not every caller may read",
    ] {
        assert!(all.contains(expected), "{expected}\n{all}");
    }
}

#[test]
fn the_record_an_update_returns_is_proved_against_its_read_even_with_no_field_policy() {
    // No field has a policy of its own, and no create is for user fields
    // only: a helper may change a board, but only its owner may read it.
    let found = support::problems(
        Layout::Loose,
        &[(
            "main.jadpo",
            r#"principal {
    user: {
        subject: Person.handle
    }
}

authentication {
    api_token: {}
}

type Handle = Text {
    max_length: 40
}

type Words = Text {
    max_length: 200
}

entity Person {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }
}

entity Board {
    id: Uuid
    owner_id: Person.id {
        on_delete: OnDelete.cascade
    }
    helper_id: Person.id {
        on_delete: OnDelete.cascade
    }
    title: Words
    secret: Words

    policy: {
        create: [Access.authenticated]
        read: [owner_id]
        update: [owner_id, helper_id]
    }
}

type BoardView = Output {
    pick: [Board.id, Board.secret]
}

type BoardTitle = Output {
    pick: [Board.id, Board.title]
}

type Retitle = Input {
    title: Words
}

action bump_board(board_id: Board.id, input: Retitle) fails BoardNotFound -> BoardView {
    var board: Board = attempt Board.update(board_id) {
        set: {
            title: input.title
        }
    }
    return BoardView {
        id: board.id
        secret: board.secret
    }
}

action open_board(input: OpenBoard) fails PersonNotFound -> BoardTitle {
    var board: Board = attempt Board.create {
        set: {
            owner_id: input.owner_id
            helper_id: input.helper_id
            title: input.title
            secret: input.title
        }
    }
    return BoardTitle {
        id: board.id
        title: board.title
    }
}

type OpenBoard = Input {
    owner_id: Person.id
    helper_id: Person.id
    title: Words
}
"#,
        )],
    );
    assert_eq!(
        rules_of(&found),
        ["policy.output_field_unproved"],
        "{found:#?}"
    );
    assert!(
        found[0].contains("`BoardView.secret` carries `Board.secret`, which not every caller of `bump_board` may read"),
        "{found:#?}"
    );
}

#[test]
fn an_update_whose_record_may_come_from_several_places_is_proved_for_each() {
    // Only editors may create a ledger, and only they read its total; anyone
    // in the company reads and changes one.
    let found = company(
        r#"entity Ledger {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    name: Label
    total: Amount {
        policy: {
            read: [Role.editor]
        }
    }

    policy: {
        create: [Role.editor]
        read: [Role.owner, Role.editor, Role.viewer]
        update: [Role.owner, Role.editor, Role.viewer]
    }
}

type TotalView = Output {
    pick: [Ledger.id, Ledger.total]
}

action fresh_total(company_id: Company.id) fails CompanyNotFound -> TotalView {
    var ledger: Ledger = attempt Ledger.create {
        set: {
            company_id: company_id
            name: "New"
            total: 0
        }
    }
    var changed: Ledger = Ledger.update(ledger) {
        set: {
            name: "Renamed"
        }
    }
    return TotalView {
        id: changed.id
        total: changed.total
    }
}

type Fresh = Bool {}

action either_total(ledger_id: Ledger.id, company_id: Company.id, fresh: Fresh) fails LedgerNotFound, CompanyNotFound -> TotalView {
    var mut ledger: Ledger = attempt Ledger.get(ledger_id)
    if (fresh) {
        ledger = attempt Ledger.create {
            set: {
                company_id: company_id
                name: "New"
                total: 0
            }
        }
    }
    var changed: Ledger = Ledger.update(ledger) {
        set: {
            name: "Renamed"
        }
    }
    return TotalView {
        id: changed.id
        total: changed.total
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
        found[0].contains("`TotalView.total` carries `Ledger.total`, which not every caller of `either_total` may read"),
        "{found:#?}"
    );
}

/// A club whose seats hold a role, `optional` or not, and a query of a
/// person's seats with each one's club.
fn club(optional: bool) -> Vec<String> {
    let role = if optional { "ClubRole?" } else { "ClubRole" };
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

type ClubRole = Enum {{
    chair
    member
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

entity Club {{
    id: Uuid
    handle: Handle

    policy: {{
        read: [ClubRole.chair, ClubRole.member]
    }}
}}

entity ClubSeat {{
    id: Uuid
    club_id: Club.id {{
        on_delete: OnDelete.cascade
    }}
    person_id: Person.id {{
        on_delete: OnDelete.cascade
    }}
    role: {role}

    membership: {{
        scope: club_id
        member: person_id
        role: role
    }}

    policy: {{
        read: [person_id]
    }}
}}

type ClubCard = Output {{
    pick: [Club.id, Club.handle]
}}

type SeatView = Output {{
    pick: [ClubSeat.id]
    club: ClubCard
}}

query my_seats(person_id: Person.id) -> List<SeatView> {{
    from: ClubSeat
    where: ClubSeat.person_id == person_id
    order: [ClubSeat.id asc]
    limit: 50
    include: {{
        club: ClubSeat.club_id
    }}
}}
"#
    );
    support::problems(Layout::Loose, &[("main.jadpo", text.as_str())])
}

#[test]
fn a_membership_whose_role_may_be_none_holds_no_role_a_proof_can_count_on() {
    assert_eq!(club(false), Vec::<String>::new());
    let found = club(true);
    assert_eq!(
        rules_of(&found),
        ["policy.output_field_unproved"],
        "{found:#?}"
    );
    assert!(
        found[0].contains("`my_seats` includes the `Club` each `ClubSeat` refers to"),
        "{found:#?}"
    );
}

#[test]
fn a_caller_inside_a_named_type_never_reaches_a_client_or_a_failure() {
    let found = with(
        r#"type Wrapper = Object {
    who: Principal.user
}

type Nested = Object {
    inner: List<Wrapper>
}

type WhoView = Output {
    wrapper: Wrapper
}

type WhoInput = Input {
    nested: Nested
}

type WhoFailed = Failure {
    kind: FailureKind.conflict
    message: "No."
    client: {
        wrapper: Wrapper?
    }
}
"#,
    );
    assert_eq!(
        rules_of(&found),
        [
            "type.auth_principal_output",
            "type.auth_principal_input",
            "type.auth_principal_failure_context"
        ],
        "{found:#?}"
    );
}

#[test]
fn a_create_sets_its_user_field_from_the_caller_on_every_path() {
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

type Mine = Bool {}

action draft_either(me: Principal.user, other: Person.id, mine: Mine, text: NoteText) fails PersonNotFound -> Draft {
    var mut owner: Person.id = me.id
    if (mine == false) {
        owner = other
    }
    return attempt Draft.create {
        set: {
            owner_id: owner
            text: text
        }
    }
}

action draft_maybe(me: Principal.user, mine: Mine, text: NoteText) -> Draft? {
    if (mine) {
        return Draft.create {
            set: {
                owner_id: me.id
                text: text
            }
        }
    }
    return none
}
"#,
    );
    assert_eq!(
        rules_of(&found),
        ["policy.role_field_create_forbidden"],
        "{found:#?}"
    );
    assert!(
        found[0].contains("`draft_either` sets `Draft.owner_id` from the request"),
        "{found:#?}"
    );
}

#[test]
fn a_value_passed_round_a_recursive_call_keeps_its_rules() {
    // `pick` gives back `second` only through its call to itself, `there`
    // only through `back`, which calls it, `show_first` sends `second`
    // only through its call to itself, and `last` gives back a link's
    // amount at any depth: the summary each call reads is worked out until
    // it stops changing (chunk 4.11), where a call back into a function
    // being followed once read as nothing.
    let found = company(
        r#"type Swap = Bool {}

type Shown = Output {
    amount: Amount
}

type Link = Object {
    amount: Amount
    next: Link?
}

function pick(first: Amount, second: Amount, swap: Swap) -> Amount {
    if (swap) {
        return pick(second, first, false)
    }
    return first
}

function last(link: Link) -> Amount {
    return match link.next {
        some(next) -> last(next)
        none -> link.amount
    }
}

function there(first: Amount, second: Amount, swap: Swap) -> Amount {
    if (swap) {
        return back(second, first)
    }
    return first
}

function back(first: Amount, second: Amount) -> Amount {
    return there(first, second, false)
}

function show_first(first: Amount, second: Amount, swap: Swap) -> Shown {
    if (swap) {
        return show_first(second, first, false)
    }
    return Shown {
        amount: first
    }
}

action peek(project_id: Project.id) fails ProjectNotFound -> Shown {
    var project: Project = attempt Project.get(project_id)
    return Shown {
        amount: pick(0, project.budget, true)
    }
}

action tail(project_id: Project.id) fails ProjectNotFound -> Shown {
    var project: Project = attempt Project.get(project_id)
    var link: Link = Link {
        amount: 0
        next: Link {
            amount: project.budget
            next: none
        }
    }
    return Shown {
        amount: last(link)
    }
}

action round(project_id: Project.id) fails ProjectNotFound -> Shown {
    var project: Project = attempt Project.get(project_id)
    return Shown {
        amount: there(0, project.budget, true)
    }
}

action shown(project_id: Project.id) fails ProjectNotFound -> Shown {
    var project: Project = attempt Project.get(project_id)
    return show_first(0, project.budget, true)
}

action safe() -> Shown {
    return Shown {
        amount: pick(0, 1, false)
    }
}
"#,
    );
    let all = found.join("\n");
    assert_eq!(
        rules_of(&found),
        [
            "policy.output_field_unproved",
            "policy.output_field_unproved",
            "policy.output_field_unproved",
            "policy.output_field_unproved"
        ],
        "{all}"
    );
    for expected in [
        "`Shown.amount` carries `Project.budget`, which not every caller of `peek` may read",
        "`Shown.amount` carries `Project.budget`, which not every caller of `tail` may read",
        "`Shown.amount` carries `Project.budget`, which not every caller of `round` may read",
        "`shown` passes `Project.budget` to `show_first`, which sends it",
    ] {
        assert!(all.contains(expected), "{expected}\n{all}");
    }
}
