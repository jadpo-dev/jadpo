//! Policy against live PostgreSQL (area 9): through `jadpo test`, a team
//! application whose tickets are read by anyone signed in, within the
//! teams where they hold a membership; opened by a team's members; changed
//! only by their assignee, whose own listed action hands them over; an
//! audit only an application-wide auditor may run; tokens revoked and keys
//! issued only by someone who may change their holder. Then, through the
//! runtime's entry point with a caller, a change policy refuses reports the
//! code's own not-found failure, which the log marks as concealed by
//! policy, and the `jadpo sql` and permissions report views.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use desk/entities { Person, Robot }

application {}

principal {
    user: {
        subject: Person.handle
    }
    service: {
        subject: Robot.name
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    browser_session: {
        cookie: "desk_session"
    }
    api_token: {}
    service_key: {}
}
"#,
    ),
    (
        "policy.jadpo",
        r#"use desk
use desk/entities { Team, Ticket }
use desk/types { StaffRole }

policy {
    tenant: Team
    actions: {
        desk.found_team: [Access.authenticated]
        desk.hand_over: [Ticket.assignee_id]
        desk.audit: [StaffRole.auditor]
    }
}
"#,
    ),
    (
        "domains/desk/types/handle.jadpo",
        r#"type Handle = Text {
    min_length: 3
    max_length: 40
}
"#,
    ),
    (
        "domains/desk/types/title.jadpo",
        r#"type Title = Text {
    min_length: 1
    max_length: 80
}
"#,
    ),
    (
        "domains/desk/types/team_role.jadpo",
        r#"type TeamRole = Enum {
    lead
    member
}
"#,
    ),
    (
        "domains/desk/types/staff_role.jadpo",
        r#"type StaffRole = Enum {
    auditor
}
"#,
    ),
    (
        "domains/desk/types/ticket_view.jadpo",
        r#"use desk/entities { Ticket }

type TicketView = Output {
    pick: [Ticket.id, Ticket.title]
}
"#,
    ),
    (
        "domains/desk/types/audit_view.jadpo",
        r#"type AuditView = Output {
    ok: Bool
}
"#,
    ),
    (
        "domains/desk/entities/person.jadpo",
        r#"use desk/types { Handle }

entity Person {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
        update: [id]
        tenant: Tenancy.shared
    }
}
"#,
    ),
    (
        "domains/desk/entities/robot.jadpo",
        r#"use desk/entities { Person }
use desk/types { Handle }

entity Robot {
    id: Uuid
    owner_id: Person.id {
        on_delete: OnDelete.restricted
    }
    name: Handle {
        unique: true
    }

    policy: {
        create: [owner_id]
        read: [id, owner_id]
        update: [owner_id]
        tenant: Tenancy.shared
    }
}
"#,
    ),
    (
        "domains/desk/entities/team.jadpo",
        r#"use desk/types { TeamRole, Title }

entity Team {
    id: Uuid
    name: Title

    policy: {
        read: [TeamRole.lead, TeamRole.member]
    }
}
"#,
    ),
    (
        "domains/desk/entities/team_membership.jadpo",
        r#"use desk/entities { Person, Team }
use desk/types { TeamRole }

entity TeamMembership {
    id: Uuid
    team_id: Team.id {
        on_delete: OnDelete.cascade
    }
    person_id: Person.id {
        on_delete: OnDelete.cascade
    }
    role: TeamRole

    membership: {
        scope: team_id
        member: person_id
        role: role
    }

    policy: {
        create: [TeamRole.lead]
        read: [TeamRole.lead, person_id]
    }
}
"#,
    ),
    (
        "domains/desk/entities/staff_membership.jadpo",
        r#"use desk/entities { Person }
use desk/types { StaffRole }

entity StaffMembership {
    id: Uuid
    person_id: Person.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        member: person_id
        role: role
    }

    policy: {
        read: [person_id]
        tenant: Tenancy.shared
    }
}
"#,
    ),
    (
        "domains/desk/entities/ticket.jadpo",
        r#"use desk/entities { Person, Team }
use desk/types { TeamRole, Title }

entity Ticket {
    id: Uuid
    team_id: Team.id {
        on_delete: OnDelete.cascade
    }
    assignee_id: Person.id {
        on_delete: OnDelete.restricted
    }
    title: Title

    policy: {
        create: [TeamRole.lead, TeamRole.member]
        read: [Access.authenticated]
        update: [assignee_id]
        delete: [TeamRole.lead]
    }
}
"#,
    ),
    (
        "domains/desk/failures/handle_taken.jadpo",
        r#"type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
"#,
    ),
    (
        "domains/desk/actions/join.jadpo",
        r#"use desk/entities { Person }
use desk/types { Handle }
use desk/failures { HandleTaken }

action join(handle: Handle) fails HandleTaken -> Person {
    return attempt Person.create {
        set: {
            handle: handle
        }
        conflict: {
            Person.handle: HandleTaken
        }
    }
}
"#,
    ),
    (
        "domains/desk/actions/found_team.jadpo",
        r#"use desk/entities { Team, TeamMembership }
use desk/types { TeamRole, Title }

action found_team(name: Title, lead: Principal.user) -> Team {
    var team: Team = Team.create {
        set: {
            name: name
        }
    }
    var held: TeamMembership = TeamMembership.create {
        set: {
            team_id: team.id
            person_id: lead.id
            role: TeamRole.lead
        }
    }
    assert held.role == TeamRole.lead
    return team
}
"#,
    ),
    (
        "domains/desk/actions/enrol.jadpo",
        r#"use desk/entities { Person, PersonNotFound, Team, TeamMembership, TeamNotFound }
use desk/types { TeamRole }

action enrol(team_id: Team.id, person_id: Person.id) fails PersonNotFound, TeamNotFound -> TeamMembership {
    return attempt TeamMembership.create {
        set: {
            team_id: team_id
            person_id: person_id
            role: TeamRole.member
        }
    }
}
"#,
    ),
    (
        "domains/desk/actions/make_auditor.jadpo",
        r#"use desk/entities { Person, PersonNotFound, StaffMembership }
use desk/types { StaffRole }

action make_auditor(person_id: Person.id) fails PersonNotFound -> StaffMembership {
    return attempt StaffMembership.create {
        set: {
            person_id: person_id
            role: StaffRole.auditor
        }
    }
}
"#,
    ),
    (
        "domains/desk/actions/open_ticket.jadpo",
        r#"use desk/entities { Person, PersonNotFound, Team, TeamNotFound, Ticket }
use desk/types { TicketView, Title }

action open_ticket(input: OpenTicket) fails PersonNotFound, TeamNotFound -> TicketView {
    var ticket: Ticket = attempt Ticket.create {
        set: {
            team_id: input.team_id
            assignee_id: input.assignee_id
            title: input.title
        }
    }
    return TicketView {
        id: ticket.id
        title: ticket.title
    }
}

type OpenTicket = Input {
    team_id: Team.id
    assignee_id: Person.id
    title: Title
}
"#,
    ),
    (
        "domains/desk/actions/retitle.jadpo",
        r#"use desk/entities { Ticket, TicketNotFound }
use desk/types { TicketView, Title }

action retitle(ticket_id: Ticket.id, input: Retitle) fails TicketNotFound -> TicketView {
    var ticket: Ticket = attempt Ticket.update(ticket_id) {
        set: {
            title: input.title
        }
    }
    return TicketView {
        id: ticket.id
        title: ticket.title
    }
}

type Retitle = Input {
    title: Title
}
"#,
    ),
    (
        "domains/desk/actions/hand_over.jadpo",
        r#"use desk/entities { Person, PersonNotFound, Ticket, TicketNotFound }
use desk/types { TicketView }

action hand_over(ticket_id: Ticket.id, input: HandOver) fails PersonNotFound, TicketNotFound -> TicketView {
    var ticket: Ticket = attempt Ticket.update(ticket_id) {
        set: {
            assignee_id: input.to
        }
    }
    return TicketView {
        id: ticket.id
        title: ticket.title
    }
}

type HandOver = Input {
    to: Person.id
}
"#,
    ),
    (
        "domains/desk/actions/audit.jadpo",
        r#"use desk/types { AuditView }

action audit() -> AuditView {
    return AuditView {
        ok: true
    }
}
"#,
    ),
    (
        "domains/desk/types/token_id.jadpo",
        r#"type TokenId = Uuid {}
"#,
    ),
    (
        "domains/desk/types/tokens_issued.jadpo",
        r#"type TokensIssued = Int {}
"#,
    ),
    (
        "domains/desk/actions/give_token.jadpo",
        r#"action give_token(person: Principal.user) -> Unit {
    var token: ApiToken = sessions.issue_token(person.id)
    assert token != ""
}
"#,
    ),
    (
        "domains/desk/actions/list_token_ids.jadpo",
        r#"use desk/types { TokenId }

action list_token_ids(person: Principal.user) -> List<TokenId> {
    return list.map(sessions.tokens(person.id), t -> TokenId(t.id))
}
"#,
    ),
    (
        "domains/desk/actions/count_tokens.jadpo",
        r#"use desk/types { TokensIssued }

action count_tokens(person: Principal.user) -> TokensIssued {
    return TokensIssued(list.count(sessions.tokens(person.id)))
}
"#,
    ),
    (
        "domains/desk/actions/forget.jadpo",
        r#"use desk/types { TokenId }

action forget(token_id: TokenId, by: Principal.user) -> Unit {
    assert by.id == by.id
    sessions.revoke_token(token_id)
}
"#,
    ),
    (
        "domains/desk/types/expiry.jadpo",
        r#"type Expiry = Instant {}
"#,
    ),
    (
        "domains/desk/actions/build_robot.jadpo",
        r#"use desk/entities { Robot }
use desk/types { Handle }
use desk/failures { HandleTaken }

action build_robot(name: Handle, owner: Principal.user) fails HandleTaken -> Robot {
    return attempt Robot.create {
        set: {
            owner_id: owner.id
            name: name
        }
        conflict: {
            Robot.name: HandleTaken
        }
    }
}
"#,
    ),
    (
        "domains/desk/actions/add_key.jadpo",
        r#"use desk/entities { Robot }
use desk/types { Expiry }

action add_key(robot_id: Robot.id, input: KeyRequest, by: Principal.user) -> Unit {
    assert by.id == by.id
    var key: ServiceKey = Robot.issue_key(robot_id, input.until)
    assert key != ""
}

type KeyRequest = Input {
    until: Expiry
}
"#,
    ),
    (
        "domains/desk/queries/tickets.jadpo",
        r#"use desk/entities { Ticket }
use desk/types { TicketView }

query tickets() -> List<TicketView> {
    from: Ticket
    order: [Ticket.title asc]
    limit: 50
}
"#,
    ),
    (
        "domains/desk/routes/desk.jadpo",
        r#"use desk/entities { Robot, Ticket }
use desk/types { AuditView, TicketView }
use desk/actions { HandOver, KeyRequest, OpenTicket, Retitle, add_key, audit, hand_over, open_ticket, retitle }
use desk/queries { tickets }

routes Desk {
    prefix: /desk

    GET /tickets {
        output: List<TicketView>
        run: tickets()
    }

    POST /tickets {
        input: OpenTicket
        output: TicketView
        status: SuccessStatus.created
        run: open_ticket(input)
    }

    PATCH /tickets/{ticket_id: Ticket.id} {
        input: Retitle
        output: TicketView
        run: retitle(ticket_id, input)
    }

    POST /tickets/{ticket_id: Ticket.id}/hand-over {
        input: HandOver
        output: TicketView
        run: hand_over(ticket_id, input)
    }

    POST /audit {
        output: AuditView
        run: audit()
    }

    POST /robots/{robot_id: Robot.id}/keys {
        input: KeyRequest
        status: SuccessStatus.no_content
        run: add_key(robot_id, input, current_principal.user)
    }
}
"#,
    ),
    (
        "api/main.jadpo_record",
        r#"// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication api_token
    authentication browser_session
    authentication service_key

POST /desk/audit
    output AuditView
    status 200

POST /desk/robots/{robot_id: Uuid}/keys
    caller user
    input KeyRequest (body)
    status 204

GET /desk/tickets
    output List<TicketView>
    status 200

POST /desk/tickets
    input OpenTicket (body)
    output TicketView
    status 201
    fails person_not_found
    fails team_not_found

PATCH /desk/tickets/{ticket_id: Uuid}
    input Retitle (body)
    output TicketView
    status 200
    fails ticket_not_found

POST /desk/tickets/{ticket_id: Uuid}/hand-over
    input HandOver (body)
    output TicketView
    status 200
    fails person_not_found
    fails ticket_not_found

output AuditView
    ok: Bool

input HandOver
    to: Uuid

input KeyRequest
    until: Instant

input OpenTicket
    assignee_id: Uuid
    team_id: Uuid
    title: Text { min_length: 1, max_length: 80 }

input Retitle
    title: Text { min_length: 1, max_length: 80 }

output TicketView
    id: Uuid
    title: Text { min_length: 1, max_length: 80 }

failure person_not_found
    status 404

failure team_not_found
    status 404

failure ticket_not_found
    status 404
"#,
    ),
];

const TESTS: &str = r#"use desk/entities { Person, Robot, StaffMembership, Team, TeamMembership }
use desk/types { TicketView, TokenId }
use desk/actions { HandOver, KeyRequest, OpenTicket, Retitle, build_robot, count_tokens, enrol, forget, found_team, give_token, join, list_token_ids, make_auditor, open_ticket }

test "anyone signed in reads tickets only in their own teams" {
    var ann: Person = attempt join("ann")
    var bob: Person = attempt join("bob")
    var red: Team = found_team("Red", Principal.user(ann))
    var blue: Team = found_team("Blue", Principal.user(bob))
    var mine: TicketView = attempt open_ticket(OpenTicket {
        team_id: red.id
        assignee_id: ann.id
        title: "Fix the lift"
    })
    var theirs: TicketView = attempt open_ticket(OpenTicket {
        team_id: blue.id
        assignee_id: bob.id
        title: "Paint the hall"
    })
    var seen: Response = request {
        method: HttpMethod.get
        path: "/desk/tickets"
        as: Principal.user(ann)
    }
    assert seen.output == [mine]
    var other: Response = request {
        method: HttpMethod.get
        path: "/desk/tickets"
        as: Principal.user(bob)
    }
    assert other.output == [theirs]
}

test "a team's members open its tickets, and nobody else can" {
    var ann: Person = attempt join("ann")
    var cyd: Person = attempt join("cyd")
    var dan: Person = attempt join("dan")
    var red: Team = found_team("Red", Principal.user(ann))
    var held: TeamMembership = attempt enrol(red.id, cyd.id)
    assert held.person_id == cyd.id
    var opened: Response = request {
        method: HttpMethod.post
        path: "/desk/tickets"
        as: Principal.user(cyd)
        input: OpenTicket {
            team_id: red.id
            assignee_id: cyd.id
            title: "Order chairs"
        }
    }
    assert opened.status == 201
    var refused: Response = request {
        method: HttpMethod.post
        path: "/desk/tickets"
        as: Principal.user(dan)
        input: OpenTicket {
            team_id: red.id
            assignee_id: dan.id
            title: "Take over"
        }
    }
    assert refused.status == 403
    assert refused.code == "not_permitted"
}

test "only a ticket's assignee changes it, and others are told it isn't there" {
    var ann: Person = attempt join("ann")
    var bob: Person = attempt join("bob")
    var red: Team = found_team("Red", Principal.user(ann))
    var held: TeamMembership = attempt enrol(red.id, bob.id)
    assert held.person_id == bob.id
    var ticket: TicketView = attempt open_ticket(OpenTicket {
        team_id: red.id
        assignee_id: ann.id
        title: "Fix the lift"
    })
    var hidden: Response = request {
        method: HttpMethod.patch
        path: "/desk/tickets/{ticket_id}"
        params: {
            ticket_id: ticket.id
        }
        as: Principal.user(bob)
        input: Retitle {
            title: "Mine now"
        }
    }
    assert hidden.status == 404
    assert hidden.code == "ticket_not_found"
    var changed: Response = request {
        method: HttpMethod.patch
        path: "/desk/tickets/{ticket_id}"
        params: {
            ticket_id: ticket.id
        }
        as: Principal.user(ann)
        input: Retitle {
            title: "Fix both lifts"
        }
    }
    assert changed.status == 200
}

test "handing a ticket over is its assignee's listed action" {
    var ann: Person = attempt join("ann")
    var bob: Person = attempt join("bob")
    var red: Team = found_team("Red", Principal.user(ann))
    var held: TeamMembership = attempt enrol(red.id, bob.id)
    assert held.person_id == bob.id
    var ticket: TicketView = attempt open_ticket(OpenTicket {
        team_id: red.id
        assignee_id: ann.id
        title: "Fix the lift"
    })
    var handed: Response = request {
        method: HttpMethod.post
        path: "/desk/tickets/{ticket_id}/hand-over"
        params: {
            ticket_id: ticket.id
        }
        as: Principal.user(ann)
        input: HandOver {
            to: bob.id
        }
    }
    assert handed.status == 200
    var again: Response = request {
        method: HttpMethod.post
        path: "/desk/tickets/{ticket_id}/hand-over"
        params: {
            ticket_id: ticket.id
        }
        as: Principal.user(ann)
        input: HandOver {
            to: ann.id
        }
    }
    assert again.status == 404
}

test "only an auditor may run the audit" {
    var ann: Person = attempt join("ann")
    var eve: Person = attempt join("eve")
    var staff: StaffMembership = attempt make_auditor(eve.id)
    assert staff.person_id == eve.id
    var allowed: Response = request {
        method: HttpMethod.post
        path: "/desk/audit"
        as: Principal.user(eve)
    }
    assert allowed.status == 200
    var refused: Response = request {
        method: HttpMethod.post
        path: "/desk/audit"
        as: Principal.user(ann)
    }
    assert refused.status == 403
    assert refused.code == "not_permitted"
}

test "a token is revoked only by someone who may change its person" {
    var ann: Person = attempt join("ann")
    var bob: Person = attempt join("bob")
    give_token(Principal.user(ann))
    var ids: List<TokenId> = list_token_ids(Principal.user(ann))
    for id in ids {
        forget(id, Principal.user(bob))
    }
    assert count_tokens(Principal.user(ann)) == 1
    for id in ids {
        forget(id, Principal.user(ann))
    }
    assert count_tokens(Principal.user(ann)) == 0
}

test "only a robot's owner issues it a key" {
    var ann: Person = attempt join("ann")
    var cyd: Person = attempt join("cyd")
    var robot: Robot = attempt build_robot("sweeper", Principal.user(ann))
    var refused: Response = request {
        method: HttpMethod.post
        path: "/desk/robots/{robot_id}/keys"
        params: {
            robot_id: robot.id
        }
        as: Principal.user(cyd)
        input: KeyRequest {
            until: "2027-01-01T00:00:00Z"
        }
    }
    assert refused.status == 403
    assert refused.code == "not_permitted"
    var issued: Response = request {
        method: HttpMethod.post
        path: "/desk/robots/{robot_id}/keys"
        params: {
            robot_id: robot.id
        }
        as: Principal.user(ann)
        input: KeyRequest {
            until: "2027-01-01T00:00:00Z"
        }
    }
    assert issued.status == 204
}
"#;

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("policy-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

#[test]
fn policy_decides_what_each_caller_reads_writes_and_revokes() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("desk");
    write(&root, "domains/desk/routes/desk.test.jadpo", TESTS);
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("7 passed"), "{}", tested.stdout);
}

// --- through the runtime ---------------------------------------------------------

use jadpo_host_native::{Postgres, Runtime, postgres};
use jadpo_interp::Program;
use jadpo_runtime::{Caller, Clock, Ctx, Database, Env, Executor, Host, Outcome};
use jadpo_std::http::PrincipalKind;
use jadpo_value::Value;

struct Store(Postgres, std::cell::Cell<u64>);

impl Host for Store {
    fn database(&self, _store: &str) -> Option<&dyn Database> {
        Some(&self.0)
    }

    fn random(&self, _bytes: &mut [u8]) -> bool {
        false
    }
}

/// Runs `action` with `args` as `caller`, signed in, or as no one.
async fn call(
    program: &Program,
    host: &Store,
    action: &str,
    args: Vec<Value>,
    caller: Option<&Value>,
) -> Outcome {
    let id = program.plan().find(action).expect("the action");
    host.1.set(host.1.get() + 1);
    let env = Env {
        seed: Some(host.1.get()),
        clock: Clock::TEST,
    };
    let mut ctx = Ctx::new(program.places(), host, env);
    ctx.set_caller(caller.map(|person| Caller {
        value: person.clone(),
        key: "user".to_string(),
        session: None,
    }));
    program.run(&mut ctx, id, args).await
}

fn value(outcome: Outcome) -> Value {
    match outcome {
        Outcome::Value(value) => value,
        other => panic!("expected a value, got {other:?}"),
    }
}

fn field(record: &Value, name: &str) -> Value {
    match record {
        Value::Record(record) => record.get(name).cloned().expect("the field"),
        other => panic!("not a record: {other:?}"),
    }
}

/// The caller a person signs in as.
fn signed_in(person: &Value) -> Value {
    jadpo_runtime::http::identity::principal(
        PrincipalKind::User,
        field(person, "id"),
        field(person, "handle"),
    )
}

#[test]
fn a_change_policy_refuses_reads_as_missing_and_is_logged_as_concealed() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("concealed");
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Store(postgres(&url).expect("store"), std::cell::Cell::new(0));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let ann = value(call(&program, &host, "join", vec![Value::text("ann")], None).await);
        let bob = value(call(&program, &host, "join", vec![Value::text("bob")], None).await);
        let (ann_in, bob_in) = (signed_in(&ann), signed_in(&bob));
        // Founding a team is listed for anyone signed in, and no one else.
        let refused = call(
            &program,
            &host,
            "found_team",
            vec![Value::text("Red"), ann_in.clone()],
            None,
        )
        .await;
        assert!(
            matches!(&refused, Outcome::Failed(failure) if failure.code == "not_permitted"),
            "{refused:?}"
        );
        let red = value(
            call(
                &program,
                &host,
                "found_team",
                vec![Value::text("Red"), ann_in.clone()],
                Some(&ann_in),
            )
            .await,
        );
        value(
            call(
                &program,
                &host,
                "enrol",
                vec![field(&red, "id"), field(&bob, "id")],
                Some(&ann_in),
            )
            .await,
        );
        let input = Value::record(
            "OpenTicket",
            vec![
                ("team_id".into(), field(&red, "id")),
                ("assignee_id".into(), field(&ann, "id")),
                ("title".into(), Value::text("Fix the lift")),
            ],
        );
        let ticket = value(call(&program, &host, "open_ticket", vec![input], Some(&ann_in)).await);
        let retitle = Value::record("Retitle", vec![("title".into(), Value::text("Mine"))]);
        // Bob may read the ticket but not change it: the code's own
        // not-found failure, which the log says policy concealed.
        let hidden = call(
            &program,
            &host,
            "retitle",
            vec![field(&ticket, "id"), retitle.clone()],
            Some(&bob_in),
        )
        .await;
        let Outcome::Failed(failure) = hidden else {
            panic!("expected a failure, got {hidden:?}");
        };
        assert_eq!(failure.code, "ticket_not_found");
        assert!(
            failure
                .internal
                .iter()
                .any(|(name, value)| name == "concealed_by" && *value == Value::text("policy")),
            "{failure:?}"
        );
        // A ticket that isn't there is only missing.
        let missing = call(
            &program,
            &host,
            "retitle",
            vec![field(&ann, "id"), retitle],
            Some(&bob_in),
        )
        .await;
        let Outcome::Failed(failure) = missing else {
            panic!("expected a failure, got {missing:?}");
        };
        assert_eq!(failure.code, "ticket_not_found");
        assert!(
            failure
                .internal
                .iter()
                .all(|(name, _)| name != "concealed_by"),
            "{failure:?}"
        );
    });
}

#[test]
fn the_report_and_jadpo_sql_show_the_policy() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("policy-views");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    jadpo(&root, &["migrate", "new", "initial"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    let report = fs::read_to_string(root.join("build/report/permissions.md")).expect("the report");
    for line in [
        "| `desk.Ticket` | create | read | update | delete |",
        "| the record | `TeamRole.lead`, `TeamRole.member` | `Access.authenticated` | `assignee_id` | `TeamRole.lead` |",
        "Belongs to a `Team` through `team_id`. Only an action listed under `actions:` changes `team_id` and `assignee_id`.",
        "| `desk.hand_over` | `Ticket.assignee_id` |",
    ] {
        assert!(report.contains(line), "{line}\n\n{report}");
    }
    let json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("build/report/permissions.json")).expect("the twin"),
    )
    .expect("JSON");
    assert_eq!(json["tenant"], "Team");
    let shown = jadpo(&root, &["sql", "retitle"]);
    assert_eq!(shown.code, 0, "{}{}", shown.stdout, shown.stderr);
    assert!(
        shown.stdout.contains("the signed-in user's id, or none"),
        "{}",
        shown.stdout
    );
    assert!(
        shown.stdout.contains("when policy may have refused"),
        "{}",
        shown.stdout
    );
}

// --- field restrictions (chunk 4.4b) ---------------------------------------------

/// A company's projects, whose budget only editors read and change and
/// whose import reference no client may write (area 9, "Field
/// restrictions").
const LEDGER: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use ledger/entities { Member }

application {}

principal {
    user: {
        subject: Member.handle
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    browser_session: {
        cookie: "ledger_session"
    }
}
"#,
    ),
    (
        "policy.jadpo",
        r#"use ledger
use ledger/entities { Company }

policy {
    tenant: Company
    actions: {
        ledger.found_company: [Access.authenticated]
    }
}
"#,
    ),
    (
        "domains/ledger/types/handle.jadpo",
        r#"type Handle = Text {
    min_length: 3
    max_length: 40
}
"#,
    ),
    (
        "domains/ledger/types/label.jadpo",
        r#"type Label = Text {
    min_length: 1
    max_length: 80
}
"#,
    ),
    (
        "domains/ledger/types/amount.jadpo",
        r#"type Amount = Int {
    min: 0
}
"#,
    ),
    (
        "domains/ledger/types/company_role.jadpo",
        r#"type CompanyRole = Enum {
    owner
    editor
    viewer
}
"#,
    ),
    (
        "domains/ledger/types/project_card.jadpo",
        r#"use ledger/entities { Project }

type ProjectCard = Output {
    pick: [Project.id, Project.name]
}
"#,
    ),
    (
        "domains/ledger/types/budget_view.jadpo",
        r#"use ledger/entities { Project }

type BudgetView = Output {
    pick: [Project.id, Project.budget]
}
"#,
    ),
    (
        "domains/ledger/entities/member.jadpo",
        r#"use ledger/types { Handle }

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
        tenant: Tenancy.shared
    }
}
"#,
    ),
    (
        "domains/ledger/entities/company.jadpo",
        r#"use ledger/types { CompanyRole, Label }

entity Company {
    id: Uuid
    name: Label

    policy: {
        read: [CompanyRole.owner, CompanyRole.editor, CompanyRole.viewer]
    }
}
"#,
    ),
    (
        "domains/ledger/entities/company_membership.jadpo",
        r#"use ledger/entities { Company, Member }
use ledger/types { CompanyRole }

entity CompanyMembership {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    member_id: Member.id {
        on_delete: OnDelete.cascade
    }
    role: CompanyRole

    membership: {
        scope: company_id
        member: member_id
        role: role
    }

    policy: {
        create: [CompanyRole.owner]
        read: [CompanyRole.owner, member_id]
    }
}
"#,
    ),
    (
        "domains/ledger/entities/project.jadpo",
        r#"use ledger/entities { Company }
use ledger/types { Amount, CompanyRole, Label }

entity Project {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    name: Label
    budget: Amount {
        policy: {
            read: [CompanyRole.editor]
            update: [CompanyRole.editor]
        }
    }
    import_reference: Label? {
        policy: FieldAccess.server_only
    }

    policy: {
        create: [CompanyRole.owner, CompanyRole.editor]
        read: [CompanyRole.owner, CompanyRole.editor, CompanyRole.viewer]
        update: [CompanyRole.owner, CompanyRole.editor]
    }
}
"#,
    ),
    (
        "domains/ledger/failures/handle_taken.jadpo",
        r#"type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
"#,
    ),
    (
        "domains/ledger/actions/register.jadpo",
        r#"use ledger/entities { Member }
use ledger/types { Handle }
use ledger/failures { HandleTaken }

action register(handle: Handle) fails HandleTaken -> Member {
    return attempt Member.create {
        set: {
            handle: handle
        }
        conflict: {
            Member.handle: HandleTaken
        }
    }
}
"#,
    ),
    (
        "domains/ledger/actions/found_company.jadpo",
        r#"use ledger/entities { Company, CompanyMembership }
use ledger/types { CompanyRole, Label }

action found_company(name: Label, founder: Principal.user) -> Company {
    var company: Company = Company.create {
        set: {
            name: name
        }
    }
    var held: CompanyMembership = CompanyMembership.create {
        set: {
            company_id: company.id
            member_id: founder.id
            role: CompanyRole.owner
        }
    }
    assert held.role == CompanyRole.owner
    return company
}
"#,
    ),
    (
        "domains/ledger/actions/add_member.jadpo",
        r#"use ledger/entities { Company, CompanyMembership, CompanyNotFound, Member, MemberNotFound }
use ledger/types { CompanyRole }

action add_member(
    company_id: Company.id,
    member_id: Member.id,
    role: CompanyRole
) fails CompanyNotFound, MemberNotFound -> CompanyMembership {
    return attempt CompanyMembership.create {
        set: {
            company_id: company_id
            member_id: member_id
            role: role
        }
    }
}
"#,
    ),
    (
        "domains/ledger/actions/start_project.jadpo",
        r#"use ledger/entities { Company, CompanyNotFound, Project }
use ledger/types { Label, ProjectCard }

action start_project(input: StartProject) fails CompanyNotFound -> ProjectCard {
    var project: Project = attempt Project.create {
        set: {
            company_id: input.company_id
            name: input.name
            budget: 0
            import_reference: input.reference
        }
    }
    return ProjectCard {
        id: project.id
        name: project.name
    }
}

type StartProject = Input {
    company_id: Company.id
    name: Label
    reference: Label?
}
"#,
    ),
    (
        "domains/ledger/actions/rename_project.jadpo",
        r#"use ledger/entities { Project, ProjectNotFound }
use ledger/types { Label, ProjectCard }

action rename_project(project_id: Project.id, input: Rename) fails ProjectNotFound -> ProjectCard {
    var project: Project = attempt Project.update(project_id) {
        set: {
            name: input.name
        }
    }
    return ProjectCard {
        id: project.id
        name: project.name
    }
}

type Rename = Input {
    name: Label
}
"#,
    ),
    (
        "domains/ledger/actions/set_budget.jadpo",
        r#"use ledger/entities { Project, ProjectNotFound }
use ledger/types { Amount, BudgetView }

action set_budget(project_id: Project.id, input: SetBudget) fails ProjectNotFound -> BudgetView {
    var project: Project = attempt Project.update(project_id) {
        set: {
            budget: input.budget
        }
    }
    return BudgetView {
        id: project.id
        budget: project.budget
    }
}

type SetBudget = Input {
    budget: Amount
}
"#,
    ),
    (
        "domains/ledger/queries/company_projects.jadpo",
        r#"use ledger/entities { Company, Project }
use ledger/types { ProjectCard }

query company_projects(company_id: Company.id) -> List<ProjectCard> {
    from: Project
    where: Project.company_id == company_id
    order: [Project.name asc]
    limit: 100
}
"#,
    ),
    (
        "domains/ledger/types/task_card.jadpo",
        r#"use ledger/entities { Task }

type TaskCard = Output {
    pick: [Task.id, Task.title]
}
"#,
    ),
    (
        "domains/ledger/types/task_view.jadpo",
        r#"use ledger/entities { Task }
use ledger/types { ProjectCard }

type TaskView = Output {
    pick: [Task.id, Task.title]
    project: ProjectCard
}
"#,
    ),
    (
        "domains/ledger/entities/task.jadpo",
        r#"use ledger/entities { Company, Project }
use ledger/types { CompanyRole, Label }

entity Task {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    project_id: Project.id {
        on_delete: OnDelete.cascade
    }
    title: Label

    policy: {
        create: [CompanyRole.owner, CompanyRole.editor]
        read: [CompanyRole.owner, CompanyRole.editor, CompanyRole.viewer]
        update: [CompanyRole.owner, CompanyRole.editor]
    }
}
"#,
    ),
    (
        "domains/ledger/actions/add_task.jadpo",
        r#"use ledger/entities { Company, CompanyNotFound, Project, ProjectNotFound, Task }
use ledger/types { Label, TaskCard }

action add_task(input: AddTask) fails CompanyNotFound, ProjectNotFound -> TaskCard {
    var task: Task = attempt Task.create {
        set: {
            company_id: input.company_id
            project_id: input.project_id
            title: input.title
        }
    }
    return TaskCard {
        id: task.id
        title: task.title
    }
}

type AddTask = Input {
    company_id: Company.id
    project_id: Project.id
    title: Label
}
"#,
    ),
    (
        "domains/ledger/actions/move_task.jadpo",
        r#"use ledger/entities { Project, ProjectNotFound, Task, TaskNotFound }
use ledger/types { TaskCard }

action move_task(task_id: Task.id, input: MoveTask) fails ProjectNotFound, TaskNotFound -> TaskCard {
    var task: Task = attempt Task.update(task_id) {
        set: {
            project_id: input.project_id
        }
    }
    return TaskCard {
        id: task.id
        title: task.title
    }
}

type MoveTask = Input {
    project_id: Project.id
}
"#,
    ),
    (
        "domains/ledger/queries/company_tasks.jadpo",
        r#"use ledger/entities { Company, Task }
use ledger/types { TaskView }

query company_tasks(company_id: Company.id) -> List<TaskView> {
    from: Task
    where: Task.company_id == company_id
    order: [Task.title asc]
    limit: 100
    include: {
        project: Task.project_id
    }
}
"#,
    ),
    (
        "domains/ledger/routes/tasks.jadpo",
        r#"use ledger/entities { Company, Task }
use ledger/types { TaskCard, TaskView }
use ledger/actions { AddTask, MoveTask, add_task, move_task }
use ledger/queries { company_tasks }

routes Tasks {
    prefix: /tasks

    POST / {
        input: AddTask
        output: TaskCard
        status: SuccessStatus.created
        run: add_task(input)
    }

    PATCH /{task_id: Task.id} {
        input: MoveTask
        output: TaskCard
        run: move_task(task_id, input)
    }

    GET /{company_id: Company.id} {
        output: List<TaskView>
        run: company_tasks(company_id)
    }
}
"#,
    ),
    (
        "domains/ledger/routes/projects.jadpo",
        r#"use ledger/entities { Company, Project }
use ledger/types { BudgetView, ProjectCard }
use ledger/actions { Rename, SetBudget, StartProject, rename_project, set_budget, start_project }
use ledger/queries { company_projects }

routes Projects {
    prefix: /projects

    POST / {
        input: StartProject
        output: ProjectCard
        status: SuccessStatus.created
        run: start_project(input)
    }

    GET /{company_id: Company.id} {
        output: List<ProjectCard>
        run: company_projects(company_id)
    }

    PATCH /{project_id: Project.id} {
        input: Rename
        output: ProjectCard
        run: rename_project(project_id, input)
    }

    PUT /{project_id: Project.id}/budget {
        input: SetBudget
        output: BudgetView
        run: set_budget(project_id, input)
    }
}
"#,
    ),
];

const LEDGER_TESTS: &str = r#"use ledger/entities { Company, CompanyMembership, Member }
use ledger/types { BudgetView, CompanyRole, ProjectCard }
use ledger/actions { Rename, SetBudget, StartProject, add_member, found_company, register, start_project }

test "only an editor changes a budget, and the whole write is refused for anyone else" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var cyd: Member = attempt register("cyd")
    var acme: Company = found_company("Acme", Principal.user(ann))
    var editor: CompanyMembership = attempt add_member(acme.id, bob.id, CompanyRole.editor)
    var viewer: CompanyMembership = attempt add_member(acme.id, cyd.id, CompanyRole.viewer)
    assert editor.role == CompanyRole.editor
    assert viewer.role == CompanyRole.viewer
    var launch: ProjectCard = attempt start_project(
        StartProject {
            company_id: acme.id
            name: "Launch"
            reference: none
        }
    )
    var by_owner: Response = request {
        method: HttpMethod.put
        path: "/projects/{project_id}/budget"
        params: {
            project_id: launch.id
        }
        as: Principal.user(ann)
        input: SetBudget {
            budget: 500
        }
    }
    assert by_owner.status == 403
    assert by_owner.code == "not_permitted"
    var by_viewer: Response = request {
        method: HttpMethod.put
        path: "/projects/{project_id}/budget"
        params: {
            project_id: launch.id
        }
        as: Principal.user(cyd)
        input: SetBudget {
            budget: 500
        }
    }
    assert by_viewer.status == 404
    assert by_viewer.code == "project_not_found"
    var renamed: Response = request {
        method: HttpMethod.patch
        path: "/projects/{project_id}"
        params: {
            project_id: launch.id
        }
        as: Principal.user(ann)
        input: Rename {
            name: "Liftoff"
        }
    }
    assert renamed.status == 200
    var by_editor: Response = request {
        method: HttpMethod.put
        path: "/projects/{project_id}/budget"
        params: {
            project_id: launch.id
        }
        as: Principal.user(bob)
        input: SetBudget {
            budget: 500
        }
    }
    assert by_editor.status == 200
    assert by_editor.output == BudgetView {
        id: launch.id
        budget: 500
    }
    var listed: Response = request {
        method: HttpMethod.get
        path: "/projects/{company_id}"
        params: {
            company_id: acme.id
        }
        as: Principal.user(cyd)
    }
    assert listed.output == [
        ProjectCard {
            id: launch.id
            name: "Liftoff"
        }
    ]
}

test "a request can't write a field only the server writes" {
    var ann: Member = attempt register("ann")
    var acme: Company = found_company("Acme", Principal.user(ann))
    var tagged: Response = request {
        method: HttpMethod.post
        path: "/projects"
        as: Principal.user(ann)
        input: StartProject {
            company_id: acme.id
            name: "Imported"
            reference: "X-1"
        }
    }
    assert tagged.status == 403
    assert tagged.code == "not_permitted"
    var plain: Response = request {
        method: HttpMethod.post
        path: "/projects"
        as: Principal.user(ann)
        input: StartProject {
            company_id: acme.id
            name: "Plain"
            reference: none
        }
    }
    assert plain.status == 201
}
"#;

const TASK_TESTS: &str = r#"use ledger/entities { Company, CompanyMembership, Member }
use ledger/types { CompanyRole, ProjectCard, TaskCard, TaskView }
use ledger/actions { AddTask, MoveTask, StartProject, add_member, add_task, found_company, register, start_project }

test "a task refers only to a project of its own company" {
    var ann: Member = attempt register("ann")
    var acme: Company = found_company("Acme", Principal.user(ann))
    var globex: Company = found_company("Globex", Principal.user(ann))
    var launch: ProjectCard = attempt start_project(
        StartProject {
            company_id: acme.id
            name: "Launch"
            reference: none
        }
    )
    var rival: ProjectCard = attempt start_project(
        StartProject {
            company_id: globex.id
            name: "Rival"
            reference: none
        }
    )
    var across: Response = request {
        method: HttpMethod.post
        path: "/tasks"
        as: Principal.user(ann)
        input: AddTask {
            company_id: acme.id
            project_id: rival.id
            title: "Borrow"
        }
    }
    assert across.status == 403
    assert across.code == "not_permitted"
    var inside: Response = request {
        method: HttpMethod.post
        path: "/tasks"
        as: Principal.user(ann)
        input: AddTask {
            company_id: acme.id
            project_id: launch.id
            title: "Plan"
        }
    }
    assert inside.status == 201
    var task: TaskCard = attempt add_task(
        AddTask {
            company_id: acme.id
            project_id: launch.id
            title: "Ship"
        }
    )
    var moved_out: Response = request {
        method: HttpMethod.patch
        path: "/tasks/{task_id}"
        params: {
            task_id: task.id
        }
        as: Principal.user(ann)
        input: MoveTask {
            project_id: rival.id
        }
    }
    assert moved_out.status == 403
    assert moved_out.code == "not_permitted"
    var missing: Response = request {
        method: HttpMethod.post
        path: "/tasks"
        as: Principal.user(ann)
        input: AddTask {
            company_id: acme.id
            project_id: "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e"
            title: "Nowhere"
        }
    }
    assert missing.status == 404
    assert missing.code == "project_not_found"
}

test "a viewer reads each task with its project, only in their company" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var acme: Company = found_company("Acme", Principal.user(ann))
    var globex: Company = found_company("Globex", Principal.user(ann))
    var viewer: CompanyMembership = attempt add_member(acme.id, bob.id, CompanyRole.viewer)
    assert viewer.role == CompanyRole.viewer
    var launch: ProjectCard = attempt start_project(
        StartProject {
            company_id: acme.id
            name: "Launch"
            reference: none
        }
    )
    var rival: ProjectCard = attempt start_project(
        StartProject {
            company_id: globex.id
            name: "Rival"
            reference: none
        }
    )
    var plan: TaskCard = attempt add_task(
        AddTask {
            company_id: acme.id
            project_id: launch.id
            title: "Plan"
        }
    )
    var other: TaskCard = attempt add_task(
        AddTask {
            company_id: globex.id
            project_id: rival.id
            title: "Copy"
        }
    )
    assert other.title == "Copy"
    var seen: Response = request {
        method: HttpMethod.get
        path: "/tasks/{company_id}"
        params: {
            company_id: acme.id
        }
        as: Principal.user(bob)
    }
    assert seen.status == 200
    assert seen.output == [
        TaskView {
            id: plan.id
            title: "Plan"
            project: launch
        }
    ]
    var theirs: Response = request {
        method: HttpMethod.get
        path: "/tasks/{company_id}"
        params: {
            company_id: globex.id
        }
        as: Principal.user(bob)
    }
    assert theirs.output == []
}
"#;

fn ledger(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("policy-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in LEDGER {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["api", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    root
}

#[test]
fn a_field_the_caller_may_not_change_refuses_the_whole_write() {
    if !jadpo_test::live() {
        return;
    }
    let root = ledger("ledger");
    write(
        &root,
        "domains/ledger/routes/projects.test.jadpo",
        LEDGER_TESTS,
    );
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("2 passed"), "{}", tested.stdout);
}

#[test]
fn a_reference_stays_inside_its_tenant_so_an_include_of_its_parent_proves() {
    if !jadpo_test::live() {
        return;
    }
    let root = ledger("ledger-tenant");
    write(&root, "domains/ledger/routes/tasks.test.jadpo", TASK_TESTS);
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("2 passed"), "{}", tested.stdout);
    // `jadpo sql` shows the check in the create's statement.
    let shown = jadpo(&root, &["sql", "add_task"]);
    assert_eq!(shown.code, 0, "{}{}", shown.stdout, shown.stderr);
    assert!(
        shown.stdout.contains("IS DISTINCT FROM"),
        "{}",
        shown.stdout
    );
}

#[test]
fn a_reference_into_another_tenant_is_refused_without_policy_too() {
    if !jadpo_test::live() {
        return;
    }
    let root = ledger("ledger-unpoliced");
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Store(postgres(&url).expect("store"), std::cell::Cell::new(0));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        // A test's or a seed's call without a caller runs without policy,
        // but what the data is still holds.
        let plain = |action: &'static str, args: Vec<Value>| {
            let program = &program;
            let host = &host;
            async move {
                let id = program.plan().find(action).expect("the action");
                host.1.set(host.1.get() + 1);
                let env = Env {
                    seed: Some(host.1.get()),
                    clock: Clock::TEST,
                };
                let mut ctx = Ctx::new(program.places(), host, env);
                ctx.set_unpoliced();
                program.run(&mut ctx, id, args).await
            }
        };
        let ann = value(plain("register", vec![Value::text("ann")]).await);
        let founder = signed_in(&ann);
        let acme = value(
            call(
                &program,
                &host,
                "found_company",
                vec![Value::text("Acme"), founder.clone()],
                Some(&founder),
            )
            .await,
        );
        let globex = value(
            call(
                &program,
                &host,
                "found_company",
                vec![Value::text("Globex"), founder.clone()],
                Some(&founder),
            )
            .await,
        );
        let start = |company: &Value, name: &str| {
            Value::record(
                "StartProject",
                vec![
                    ("company_id".into(), field(company, "id")),
                    ("name".into(), Value::text(name)),
                    ("reference".into(), Value::None),
                ],
            )
        };
        let launch = value(plain("start_project", vec![start(&acme, "Launch")]).await);
        let rival = value(plain("start_project", vec![start(&globex, "Rival")]).await);
        let add = |project: &Value| {
            Value::record(
                "AddTask",
                vec![
                    ("company_id".into(), field(&acme, "id")),
                    ("project_id".into(), field(project, "id")),
                    ("title".into(), Value::text("Plan")),
                ],
            )
        };
        let across = plain("add_task", vec![add(&rival)]).await;
        assert!(
            matches!(&across, Outcome::Failed(failure) if failure.code == "not_permitted"),
            "{across:?}"
        );
        let task = value(plain("add_task", vec![add(&launch)]).await);
        let moved = plain(
            "move_task",
            vec![
                field(&task, "id"),
                Value::record("MoveTask", vec![("project_id".into(), field(&rival, "id"))]),
            ],
        )
        .await;
        assert!(
            matches!(&moved, Outcome::Failed(failure) if failure.code == "not_permitted"),
            "{moved:?}"
        );
        // A task that isn't there is only missing.
        let missing = plain(
            "move_task",
            vec![
                field(&launch, "id"),
                Value::record(
                    "MoveTask",
                    vec![("project_id".into(), field(&launch, "id"))],
                ),
            ],
        )
        .await;
        assert!(
            matches!(&missing, Outcome::Failed(failure) if failure.code == "task_not_found"),
            "{missing:?}"
        );
    });
}

#[test]
fn an_output_never_carries_a_field_not_every_caller_may_read() {
    let root = ledger("ledger-views");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    // The report's field rows.
    let report = fs::read_to_string(root.join("build/report/permissions.md")).expect("the report");
    for line in [
        "| `budget` | as the record | `CompanyRole.editor` | `CompanyRole.editor` | as the record |",
        "| `import_reference` | server only | server only | server only | as the record |",
    ] {
        assert!(report.contains(line), "{line}\n\n{report}");
    }
    let json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("build/report/permissions.json")).expect("the twin"),
    )
    .expect("JSON");
    let project = json["entities"]
        .as_array()
        .and_then(|all| all.iter().find(|e| e["entity"] == "ledger.Project"))
        .expect("the project");
    assert_eq!(
        project["fields"],
        serde_json::json!([
            { "field": "budget", "grants": { "read": ["CompanyRole.editor"], "update": ["CompanyRole.editor"] } },
            { "field": "import_reference", "serverOnly": true }
        ])
    );
    // Policy in the manifest: its grants, field restrictions, tenancy and
    // listed action, each with the edge to what it decides.
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("build/manifest.json")).expect("the manifest"),
    )
    .expect("JSON");
    let node = |id: &str| {
        manifest["nodes"]
            .as_array()
            .and_then(|nodes| nodes.iter().find(|n| n["id"] == id))
            .cloned()
            .unwrap_or_else(|| panic!("no node {id}"))
    };
    let read = node("grant Project.read");
    assert_eq!(read["effect"], "read");
    assert_eq!(read["grants"], serde_json::json!(["entity Project"]));
    let budget = node("field_restriction Project.budget");
    assert_eq!(
        budget["grants"],
        serde_json::json!(["field Project.budget"])
    );
    let tenancy = node("tenancy Project");
    assert_eq!(tenancy["via"], "company_id");
    assert_eq!(tenancy["scopes"], serde_json::json!(["entity Project"]));
    let listed = node("action_exception ledger.found_company");
    assert_eq!(listed["who"], serde_json::json!(["Access.authenticated"]));
    // `jadpo sql` shows how a field's refusal is told from the record's.
    let shown = jadpo(&root, &["sql", "set_budget"]);
    assert_eq!(shown.code, 0, "{}{}", shown.stdout, shown.stderr);
    assert!(
        shown.stdout.contains("when a field may have refused"),
        "{}",
        shown.stdout
    );
    // A query sending the budget to everyone who may read a project.
    write(
        &root,
        "domains/ledger/queries/company_projects.jadpo",
        r#"use ledger/entities { Company, Project }
use ledger/types { BudgetView }

query company_projects(company_id: Company.id) -> List<BudgetView> {
    from: Project
    where: Project.company_id == company_id
    order: [Project.name asc]
    limit: 100
}
"#,
    );
    let refused = jadpo(&root, &["check"]);
    assert_eq!(refused.code, 1, "{}", refused.stdout);
    assert!(
        refused
            .stdout
            .contains("rule: policy.output_field_unproved")
            && refused
                .stdout
                .contains("`company_projects` sends `Project.budget`"),
        "{}",
        refused.stdout
    );
    // A query filtering on the budget shows each budget to viewers too.
    write(
        &root,
        "domains/ledger/queries/company_projects.jadpo",
        r#"use ledger/entities { Company, Project }
use ledger/types { ProjectCard }

query company_projects(company_id: Company.id) -> List<ProjectCard> {
    from: Project
    where: Project.company_id == company_id
    where: Project.budget > 100
    order: [Project.name asc]
    limit: 100
}
"#,
    );
    let refused = jadpo(&root, &["check"]);
    assert_eq!(refused.code, 1, "{}", refused.stdout);
    assert!(
        refused.stdout.contains(
            "`company_projects` filters on `Project.budget`, which not every caller may read"
        ) && refused.stdout.contains("tests `budget`"),
        "{}",
        refused.stdout
    );
}

// --- chunk 4.8: tenancy, moves, transitions and listed lines ------------------

/// A guild application: anyone signed in founds a guild and takes a seat
/// in it, or gives one to someone else in a guild they sit in; a job is
/// closed by a master, since only masters set when it closed; a master
/// moves a job between guilds; and a tally is listed for masters, though
/// it changes no record a master's role could be checked on.
const GUILD: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use craft/entities { Member }

application {}

principal {
    user: {
        subject: Member.handle
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    browser_session: {
        cookie: "guild_session"
    }
}
"#,
    ),
    (
        "policy.jadpo",
        r#"use craft
use craft/entities { Guild }
use craft/types { GuildRole }

policy {
    tenant: Guild
    actions: {
        craft.move_job: [GuildRole.master]
        craft.tally: [GuildRole.master]
    }
}
"#,
    ),
    (
        "domains/craft/types/handle.jadpo",
        r#"type Handle = Text {
    min_length: 3
    max_length: 40
}
"#,
    ),
    (
        "domains/craft/types/label.jadpo",
        r#"type Label = Text {
    min_length: 1
    max_length: 80
}
"#,
    ),
    (
        "domains/craft/types/guild_role.jadpo",
        r#"type GuildRole = Enum {
    master
    hand
}
"#,
    ),
    (
        "domains/craft/types/job_status.jadpo",
        r#"type JobStatus = Enum {
    open
    closed
}
"#,
    ),
    (
        "domains/craft/types/guild_card.jadpo",
        r#"use craft/entities { Guild }

type GuildCard = Output {
    pick: [Guild.id, Guild.name]
}
"#,
    ),
    (
        "domains/craft/types/seat_view.jadpo",
        r#"use craft/entities { GuildSeat }

type SeatView = Output {
    pick: [GuildSeat.id, GuildSeat.role]
}
"#,
    ),
    (
        "domains/craft/types/job_card.jadpo",
        r#"use craft/entities { Job }

type JobCard = Output {
    pick: [Job.id, Job.title]
}
"#,
    ),
    (
        "domains/craft/types/tally_view.jadpo",
        r#"type TallyView = Output {
    ok: Bool
}
"#,
    ),
    (
        "domains/craft/entities/member.jadpo",
        r#"use craft/types { Handle }

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
        tenant: Tenancy.shared
    }
}
"#,
    ),
    (
        "domains/craft/entities/guild.jadpo",
        r#"use craft/types { GuildRole, Label }

entity Guild {
    id: Uuid
    name: Label

    policy: {
        create: [Access.authenticated]
        read: [GuildRole.master, GuildRole.hand]
    }
}
"#,
    ),
    (
        "domains/craft/entities/guild_seat.jadpo",
        r#"use craft/entities { Guild, Member }
use craft/types { GuildRole }

entity GuildSeat {
    id: Uuid
    guild_id: Guild.id {
        on_delete: OnDelete.cascade
    }
    member_id: Member.id {
        on_delete: OnDelete.cascade
    }
    role: GuildRole

    membership: {
        scope: guild_id
        member: member_id
        role: role
    }

    policy: {
        create: [Access.authenticated]
        read: [GuildRole.master, member_id]
    }
}
"#,
    ),
    (
        "domains/craft/entities/job.jadpo",
        r#"use craft/entities { Guild }
use craft/types { GuildRole, JobStatus, Label }

entity Job {
    id: Uuid
    guild_id: Guild.id {
        on_delete: OnDelete.cascade
    }
    title: Label
    status: JobStatus
    closed_at: Instant? {
        policy: {
            update: [GuildRole.master]
        }
    }

    lifecycle: {
        initial: {
            status: JobStatus.open
        }
        transitions: {
            close: {
                when: status == JobStatus.open
                set: {
                    status: JobStatus.closed
                    closed_at: clock.now
                }
            }
        }
    }

    policy: {
        create: [GuildRole.master, GuildRole.hand]
        read: [GuildRole.master, GuildRole.hand]
        update: [GuildRole.master, GuildRole.hand]
    }
}
"#,
    ),
    (
        "domains/craft/failures/handle_taken.jadpo",
        r#"type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
"#,
    ),
    (
        "domains/craft/failures/job_closed.jadpo",
        r#"type JobClosed = Failure {
    kind: FailureKind.conflict
    message: "That job is closed."
}
"#,
    ),
    (
        "domains/craft/actions/register.jadpo",
        r#"use craft/entities { Member }
use craft/types { Handle }
use craft/failures { HandleTaken }

action register(handle: Handle) fails HandleTaken -> Member {
    return attempt Member.create {
        set: {
            handle: handle
        }
        conflict: {
            Member.handle: HandleTaken
        }
    }
}
"#,
    ),
    (
        "domains/craft/actions/found_guild.jadpo",
        r#"use craft/entities { Guild, GuildSeat }
use craft/types { GuildCard, GuildRole, Label }

action found_guild(name: Label, founder: Principal.user) -> GuildCard {
    var guild: Guild = Guild.create {
        set: {
            name: name
        }
    }
    var seat: GuildSeat = GuildSeat.create {
        set: {
            guild_id: guild.id
            member_id: founder.id
            role: GuildRole.master
        }
    }
    assert seat.role == GuildRole.master
    return GuildCard {
        id: guild.id
        name: guild.name
    }
}
"#,
    ),
    (
        "domains/craft/actions/take_seat.jadpo",
        r#"use craft/entities { Guild, GuildNotFound, GuildSeat, Member, MemberNotFound }
use craft/types { GuildRole, SeatView }

action take_seat(guild_id: Guild.id, input: NewSeat) fails GuildNotFound, MemberNotFound -> SeatView {
    var seat: GuildSeat = attempt GuildSeat.create {
        set: {
            guild_id: guild_id
            member_id: input.member_id
            role: input.role
        }
    }
    return SeatView {
        id: seat.id
        role: seat.role
    }
}

type NewSeat = Input {
    member_id: Member.id
    role: GuildRole
}
"#,
    ),
    (
        "domains/craft/actions/open_job.jadpo",
        r#"use craft/entities { Guild, GuildNotFound, Job }
use craft/types { JobCard, Label }

action open_job(input: OpenJob) fails GuildNotFound -> JobCard {
    var job: Job = attempt Job.create {
        set: {
            guild_id: input.guild_id
            title: input.title
        }
    }
    return JobCard {
        id: job.id
        title: job.title
    }
}

type OpenJob = Input {
    guild_id: Guild.id
    title: Label
}
"#,
    ),
    (
        "domains/craft/actions/close_job.jadpo",
        r#"use craft/entities { Job, JobNotFound }
use craft/failures { JobClosed }
use craft/types { JobCard }

action close_job(job_id: Job.id) fails JobNotFound, JobClosed -> JobCard {
    var job: Job = attempt Job.close(job_id) {
        conflict: {
            Job.close: JobClosed
        }
    }
    return JobCard {
        id: job.id
        title: job.title
    }
}
"#,
    ),
    (
        "domains/craft/actions/move_job.jadpo",
        r#"use craft/entities { Guild, GuildNotFound, Job, JobNotFound }
use craft/types { JobCard }

action move_job(job_id: Job.id, input: MoveJob) fails JobNotFound, GuildNotFound -> JobCard {
    var job: Job = attempt Job.update(job_id) {
        set: {
            guild_id: input.to
        }
    }
    return JobCard {
        id: job.id
        title: job.title
    }
}

type MoveJob = Input {
    to: Guild.id
}
"#,
    ),
    (
        "domains/craft/actions/tally.jadpo",
        r#"use craft/types { TallyView }

action tally() -> TallyView {
    return TallyView {
        ok: true
    }
}
"#,
    ),
    (
        "domains/craft/queries/guild_jobs.jadpo",
        r#"use craft/entities { Guild, Job }
use craft/types { JobCard }

query guild_jobs(guild_id: Guild.id) -> List<JobCard> {
    from: Job
    where: Job.guild_id == guild_id
    order: [Job.title asc]
    limit: 100
}
"#,
    ),
    (
        "domains/craft/routes/guilds.jadpo",
        r#"use craft/entities { Guild }
use craft/types { SeatView }
use craft/actions { NewSeat, take_seat }

routes Guilds {
    prefix: /guilds

    POST /{guild_id: Guild.id}/seats {
        input: NewSeat
        output: SeatView
        status: SuccessStatus.created
        run: take_seat(guild_id, input)
    }
}
"#,
    ),
    (
        "domains/craft/routes/jobs.jadpo",
        r#"use craft/entities { Guild, Job }
use craft/types { JobCard }
use craft/actions { MoveJob, OpenJob, close_job, move_job, open_job }
use craft/queries { guild_jobs }

routes Jobs {
    prefix: /jobs

    POST / {
        input: OpenJob
        output: JobCard
        status: SuccessStatus.created
        run: open_job(input)
    }

    GET /in/{guild_id: Guild.id} {
        output: List<JobCard>
        run: guild_jobs(guild_id)
    }

    POST /{job_id: Job.id}/close {
        output: JobCard
        run: close_job(job_id)
    }

    POST /{job_id: Job.id}/move {
        input: MoveJob
        output: JobCard
        run: move_job(job_id, input)
    }
}
"#,
    ),
];

const GUILD_TESTS: &str = r#"use craft/entities { Member }
use craft/types { GuildCard, GuildRole, JobCard, SeatView }
use craft/actions { MoveJob, NewSeat, OpenJob, found_guild, open_job, register, take_seat }

test "anyone founds a guild, and nobody seats themselves in someone else's" {
    var ann: Member = attempt register("ann")
    var eve: Member = attempt register("eve")
    var forge: GuildCard = found_guild("Forge", Principal.user(ann))
    var seized: Response = request {
        method: HttpMethod.post
        path: "/guilds/{guild_id}/seats"
        params: {
            guild_id: forge.id
        }
        as: Principal.user(eve)
        input: NewSeat {
            member_id: eve.id
            role: GuildRole.master
        }
    }
    assert seized.status == 403
    assert seized.code == "not_permitted"
    var own: GuildCard = found_guild("Anvil", Principal.user(eve))
    assert own.name == "Anvil"
    var given: Response = request {
        method: HttpMethod.post
        path: "/guilds/{guild_id}/seats"
        params: {
            guild_id: forge.id
        }
        as: Principal.user(ann)
        input: NewSeat {
            member_id: eve.id
            role: GuildRole.hand
        }
    }
    assert given.status == 201
}

test "moving a job to another guild needs a seat in it" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var forge: GuildCard = found_guild("Forge", Principal.user(ann))
    var anvil: GuildCard = found_guild("Anvil", Principal.user(bob))
    var job: JobCard = attempt open_job(
        OpenJob {
            guild_id: forge.id
            title: "Hinge"
        }
    )
    var away: Response = request {
        method: HttpMethod.post
        path: "/jobs/{job_id}/move"
        params: {
            job_id: job.id
        }
        as: Principal.user(ann)
        input: MoveJob {
            to: anvil.id
        }
    }
    assert away.status == 403
    assert away.code == "not_permitted"
    var hand: SeatView = attempt take_seat(
        anvil.id,
        NewSeat {
            member_id: ann.id
            role: GuildRole.hand
        }
    )
    assert hand.role == GuildRole.hand
    var moved: Response = request {
        method: HttpMethod.post
        path: "/jobs/{job_id}/move"
        params: {
            job_id: job.id
        }
        as: Principal.user(ann)
        input: MoveJob {
            to: anvil.id
        }
    }
    assert moved.status == 200
    var seen: Response = request {
        method: HttpMethod.get
        path: "/jobs/in/{guild_id}"
        params: {
            guild_id: anvil.id
        }
        as: Principal.user(bob)
    }
    assert seen.output == [job]
}

test "a hand can't close a job only a master may mark closed, and closing twice conflicts" {
    var ann: Member = attempt register("ann")
    var cyd: Member = attempt register("cyd")
    var forge: GuildCard = found_guild("Forge", Principal.user(ann))
    var hand: SeatView = attempt take_seat(
        forge.id,
        NewSeat {
            member_id: cyd.id
            role: GuildRole.hand
        }
    )
    assert hand.role == GuildRole.hand
    var job: JobCard = attempt open_job(
        OpenJob {
            guild_id: forge.id
            title: "Hinge"
        }
    )
    var by_hand: Response = request {
        method: HttpMethod.post
        path: "/jobs/{job_id}/close"
        params: {
            job_id: job.id
        }
        as: Principal.user(cyd)
    }
    assert by_hand.status == 403
    assert by_hand.code == "not_permitted"
    var by_master: Response = request {
        method: HttpMethod.post
        path: "/jobs/{job_id}/close"
        params: {
            job_id: job.id
        }
        as: Principal.user(ann)
    }
    assert by_master.status == 200
    var again: Response = request {
        method: HttpMethod.post
        path: "/jobs/{job_id}/close"
        params: {
            job_id: job.id
        }
        as: Principal.user(ann)
    }
    assert again.status == 409
    assert again.code == "job_closed"
    var hand_again: Response = request {
        method: HttpMethod.post
        path: "/jobs/{job_id}/close"
        params: {
            job_id: job.id
        }
        as: Principal.user(cyd)
    }
    assert hand_again.status == 403
}
"#;

fn guild(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("policy-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in GUILD {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["api", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    root
}

#[test]
fn tenancy_seats_only_founders_moves_need_a_seat_and_a_field_refusal_comes_first() {
    if !jadpo_test::live() {
        return;
    }
    let root = guild("guild");
    write(&root, "domains/craft/routes/guilds.test.jadpo", GUILD_TESTS);
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("3 passed"), "{}", tested.stdout);
}

#[test]
fn a_listed_line_held_in_a_record_refuses_an_action_that_changes_none() {
    if !jadpo_test::live() {
        return;
    }
    let root = guild("guild-tally");
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Store(postgres(&url).expect("store"), std::cell::Cell::new(0));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let ann = value(call(&program, &host, "register", vec![Value::text("ann")], None).await);
        let ann_in = signed_in(&ann);
        value(
            call(
                &program,
                &host,
                "found_guild",
                vec![Value::text("Forge"), ann_in.clone()],
                Some(&ann_in),
            )
            .await,
        );
        // Ann is a master of a guild, but `tally` changes no record her
        // role could be checked on: its line lets nobody call it.
        let refused = call(&program, &host, "tally", Vec::new(), Some(&ann_in)).await;
        assert!(
            matches!(&refused, Outcome::Failed(failure) if failure.code == "not_permitted"),
            "{refused:?}"
        );
    });
}
