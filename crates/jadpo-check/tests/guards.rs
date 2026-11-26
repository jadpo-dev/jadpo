//! A guard is one decision (chunk 10.11b, area 9 "What else shows a
//! field"): a restricted field that only decides whether an action goes
//! ahead, by a `reject` its callers see, shows them one fact, that it
//! refused. The values stored or returned on the way past it are not
//! copies of the field, unless they are worked out from it.

use jadpo_db::{Database, Layout};
use jadpo_diag::{Decider, Diagnostic};

use crate::support;

const APP: &str = "use desk/entities { User }

application {}

principal {
    user: {
        subject: User.handle
    }
}
";

const AUTHENTICATION: &str = "authentication {
    api_token: {}
}
";

const POLICY: &str = "use desk/entities { Workspace }

policy {
    tenant: Workspace
}
";

const ROLE: &str = "/// What a member may do.
type Role = Enum {
    owner
    agent
    viewer
}
";

const BODY: &str = "/// A short text.
type Body = Text {
    max_length: 200
}
";

const USER: &str = "use desk/types { Body }

/// Someone who signs in.
entity User {
    id: Uuid
    handle: Body {
        unique: true
    }

    policy: {
        read: [id]
        tenant: Tenancy.shared
    }
}
";

const WORKSPACE: &str = "use desk/types { Body, Role }

/// The tenant.
entity Workspace {
    id: Uuid
    name: Body

    policy: {
        read: [Role.owner, Role.agent, Role.viewer]
    }
}
";

const MEMBERSHIP: &str = "use desk/entities { User, Workspace }
use desk/types { Role }

/// A role in a workspace.
entity Membership {
    id: Uuid
    workspace_id: Workspace.id {
        on_delete: OnDelete.cascade
    }
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: Role

    membership: {
        scope: workspace_id
        member: user_id
        role: role
    }

    policy: {
        read: [Role.owner, user_id]
    }
}
";

/// Staff read customers; viewers never do, and only owners read whether
/// one is flagged.
const CUSTOMER: &str = "use desk/entities { Workspace }
use desk/types { Body, Role }

/// Someone the desk helps.
entity Customer {
    id: Uuid
    workspace_id: Workspace.id {
        on_delete: OnDelete.cascade
    }
    name: Body
    blocked: Bool
    flagged: Bool {
        policy: {
            read: [Role.owner]
        }
    }
    note: Body

    policy: {
        create: [Role.owner, Role.agent]
        read: [Role.owner, Role.agent]
    }
}
";

/// Every member reads a ticket.
const TICKET: &str = "use desk/entities { Customer, Workspace }
use desk/types { Body, Role }

/// A request for help.
entity Ticket {
    id: Uuid
    workspace_id: Workspace.id {
        on_delete: OnDelete.cascade
    }
    customer_id: Customer.id {
        on_delete: OnDelete.cascade
    }
    subject: Body
    body: Body
    channel: Body

    policy: {
        create: [Role.owner, Role.agent]
        read: [Role.owner, Role.agent, Role.viewer]
    }
}
";

const CUSTOMER_BLOCKED: &str = "/// The customer may not open tickets.
type CustomerBlocked = Failure {
    kind: FailureKind.rejected
    message: \"This customer can't open tickets.\"
}
";

/// Opens a ticket for a customer unless `guard` (a condition on
/// `customer`) holds, the ticket's channel `channel`, after `before`.
fn guarded(name: &str, guard: &str, before: &str, channel: &str) -> String {
    format!(
        "use desk/entities {{ Customer, CustomerNotFound, Ticket, Workspace, WorkspaceNotFound }}
use desk/failures {{ CustomerBlocked }}
use desk/types {{ Body }}

/// Opens a ticket for a customer who may open one.
action {name}(workspace_id: Workspace.id, customer_id: Customer.id, subject: Body, body: Body) fails CustomerNotFound, WorkspaceNotFound, CustomerBlocked -> Ticket {{
    var customer: Customer = attempt Customer.get(customer_id)
{before}    if ({guard}) {{
        reject CustomerBlocked
    }}
    return attempt Ticket.create {{
        set: {{
            workspace_id: workspace_id
            customer_id: customer.id
            subject: subject
            body: body
            channel: {channel}
        }}
    }}
}}
"
    )
}

/// What checking the project finds, its root policy `policy`.
fn diagnostics(policy: &str, extra: &[(&str, &str)]) -> Vec<Diagnostic> {
    let mut files: Vec<(&str, &str)> = vec![
        ("app.jadpo", APP),
        ("authentication.jadpo", AUTHENTICATION),
        ("policy.jadpo", policy),
        ("domains/desk/types/role.jadpo", ROLE),
        ("domains/desk/types/body.jadpo", BODY),
        (
            "domains/desk/failures/customer_blocked.jadpo",
            CUSTOMER_BLOCKED,
        ),
        ("domains/desk/entities/user.jadpo", USER),
        ("domains/desk/entities/workspace.jadpo", WORKSPACE),
        ("domains/desk/entities/membership.jadpo", MEMBERSHIP),
        ("domains/desk/entities/customer.jadpo", CUSTOMER),
        ("domains/desk/entities/ticket.jadpo", TICKET),
    ];
    // A file given again replaces the one here.
    files.retain(|(path, _)| !extra.iter().any(|(given, _)| given == path));
    files.extend_from_slice(extra);
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files);
    jadpo_check::check(&db, project)
        .iter()
        .filter(|d| d.rule.id != "data.personal_unread" && d.rule.id != "record.out_of_date")
        .cloned()
        .collect()
}

fn shown(found: &[Diagnostic]) -> String {
    found
        .iter()
        .map(|d| {
            format!(
                "{}: {}\n  reason: {}\n  next: {}",
                d.rule.id,
                d.message,
                d.reason.as_deref().unwrap_or(""),
                d.next.as_ref().map_or("", |next| next.text.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The one `policy.output_field_unproved` found, asking a person to
/// accept `subject`.
fn asks_for(found: &[Diagnostic], subject: &str) -> Diagnostic {
    let all = shown(found);
    assert_eq!(found.len(), 1, "{all}");
    let d = &found[0];
    assert_eq!(d.rule.id, "policy.output_field_unproved", "{all}");
    let next = d.next.as_ref().expect("a next step");
    assert_eq!(next.decides, Decider::Human, "{all}");
    assert!(
        next.text
            .contains(&format!("{subject}: PolicyChange.field_copied")),
        "{all}"
    );
    d.clone()
}

const ACTION: &str = "domains/desk/actions/open_ticket.jadpo";

/// An acceptance listing `subject` as an approved copy.
fn accepting(subject: &str) -> String {
    format!(
        "/// Sam Okafor (support lead), in chat on 2026-11-26: \"Staff may learn that a customer is flagged.\"
acceptance {{
    changes: {{
        {subject}: PolicyChange.field_copied
    }}
}}
"
    )
}

#[test]
fn a_guard_every_caller_may_read_stores_the_ticket_with_no_acceptance() {
    // Viewers read every field of the ticket but not `blocked`; the
    // ticket's fields are the request's and the customer's, not copies
    // of `blocked`, and everyone who reaches the customer may read it.
    let action = guarded("open_ticket", "customer.blocked", "", "\"web\"");
    let found = diagnostics(POLICY, &[(ACTION, &action)]);
    assert!(found.is_empty(), "{}", shown(&found));
}

#[test]
fn a_guard_testing_two_fields_every_caller_reads_needs_no_acceptance() {
    let action = guarded(
        "open_ticket",
        "customer.blocked or text.length(customer.note) > 150",
        "",
        "\"web\"",
    );
    let found = diagnostics(POLICY, &[(ACTION, &action)]);
    assert!(found.is_empty(), "{}", shown(&found));
}

#[test]
fn a_guard_whose_failure_reaches_callers_who_cant_read_the_field_asks_for_one_acceptance() {
    // Agents may not read `flagged`, so whether the action refuses shows
    // them one fact about it: one acceptance, not one per stored field.
    let action = guarded("open_ticket", "customer.flagged", "", "\"web\"");
    let found = diagnostics(POLICY, &[(ACTION, &action)]);
    let all = shown(&found);
    assert_eq!(found.len(), 1, "{all}");
    let d = &found[0];
    assert_eq!(d.rule.id, "policy.output_field_unproved", "{all}");
    assert_eq!(
        d.message,
        "`open_ticket` raises `desk.CustomerBlocked` on a condition that tests `Customer.flagged`, which not every caller may read"
    );
    let next = d.next.as_ref().expect("a next step");
    assert_eq!(next.decides, Decider::Human, "{all}");
    assert!(
        next.text.contains(
            "`\"desk.Customer.flagged -> desk.CustomerBlocked\": PolicyChange.field_copied`"
        ),
        "{all}"
    );
}

#[test]
fn one_acceptance_of_the_guard_lets_the_action_compile() {
    let action = guarded("open_ticket", "customer.flagged", "", "\"web\"");
    let accepted = accepting("\"desk.Customer.flagged -> desk.CustomerBlocked\"");
    let found = diagnostics(
        POLICY,
        &[
            (ACTION, &action),
            (
                "acceptances/20261126T1000_flagged_customers.jadpo",
                &accepted,
            ),
        ],
    );
    assert!(found.is_empty(), "{}", shown(&found));
}

#[test]
fn a_value_worked_out_from_the_guarded_field_is_still_a_copy() {
    // The channel viewers read is decided by `blocked`, not only guarded
    // by it.
    let action = guarded(
        "open_ticket",
        "customer.blocked",
        "    var mut channel: Body = \"web\"\n    if (text.length(customer.note) > 150) {\n        channel = \"long\"\n    }\n",
        "channel",
    );
    let found = diagnostics(POLICY, &[(ACTION, &action)]);
    let d = asks_for(&found, "\"desk.Customer.note -> desk.Ticket.channel\"");
    assert_eq!(
        d.message,
        "`open_ticket` stores a value decided by `Customer.note` in `Ticket.channel`, which not everyone who may read `channel` may read"
    );
}

#[test]
fn a_guard_that_may_also_return_still_decides_what_comes_after() {
    // A `return` shows the caller a different value, so what it tested
    // decides what comes after it, as before.
    let action = guarded("open_ticket", "customer.blocked", "", "\"web\"").replace(
        "        reject CustomerBlocked\n",
        "        if (subject == body) {\n            return attempt Ticket.create {\n                set: {\n                    workspace_id: workspace_id\n                    customer_id: customer.id\n                    subject: subject\n                    body: body\n                    channel: \"copy\"\n                }\n            }\n        }\n        reject CustomerBlocked\n",
    );
    let found = diagnostics(POLICY, &[(ACTION, &action)]);
    let all = shown(&found);
    assert!(
        found
            .iter()
            .any(|d| d.rule.id == "policy.output_field_unproved"
                && d.message.contains("decided by `Customer.blocked`")),
        "{all}"
    );
}

/// An action guarding on a flag it is passed, and one passing it a
/// field only owners read.
const PASSED_GUARD: &str = "use desk/entities { Customer, CustomerNotFound, Ticket, Workspace, WorkspaceNotFound }
use desk/failures { CustomerBlocked }
use desk/types { Body, Held }

/// Opens a ticket unless told the customer is held.
action open_unless_held(workspace_id: Workspace.id, customer_id: Customer.id, held: Held, subject: Body) fails CustomerNotFound, WorkspaceNotFound, CustomerBlocked -> Ticket {
    var customer: Customer = attempt Customer.get(customer_id)
    if (held) {
        reject CustomerBlocked
    }
    return attempt Ticket.create {
        set: {
            workspace_id: workspace_id
            customer_id: customer.id
            subject: subject
            body: subject
            channel: \"web\"
        }
    }
}
";

const HELD: &str = "/// Whether a customer is held back from opening tickets.
type Held = Bool {}
";

const PASSING: &str = "use desk/entities { Customer, CustomerNotFound, Ticket, Workspace, WorkspaceNotFound }
use desk/failures { CustomerBlocked }
use desk/types { Body }
use desk/actions { open_unless_held }

/// Opens a ticket unless the customer is flagged.
action open_ticket(workspace_id: Workspace.id, customer_id: Customer.id, subject: Body, body: Body) fails CustomerNotFound, WorkspaceNotFound, CustomerBlocked -> Ticket {
    var customer: Customer = attempt Customer.get(customer_id)
    return attempt open_unless_held(workspace_id, customer_id, customer.flagged, subject)
}
";

#[test]
fn a_guard_on_a_parameter_is_judged_where_it_is_called_as_before() {
    // The called action can't tell a guard on its parameter from a copy
    // of it, so its caller passing `flagged` stores a value decided by
    // it, as it did before (chunk 10.9c's rule across calls).
    let found = diagnostics(
        POLICY,
        &[
            ("domains/desk/types/held.jadpo", HELD),
            ("domains/desk/actions/open_unless_held.jadpo", PASSED_GUARD),
            (ACTION, PASSING),
        ],
    );
    let all = shown(&found);
    assert!(
        found
            .iter()
            .any(|d| d.rule.id == "policy.output_field_unproved"
                && d.message.contains("decided by `Customer.flagged`")
                && d.message.contains("`Ticket.")),
        "{all}"
    );
    assert!(
        !all.contains("desk.Customer.flagged -> desk.CustomerBlocked"),
        "{all}"
    );
}

/// A customer's portal password hash, which only owners may read.
const HASHED_CUSTOMER: &str = "use desk/entities { Workspace }
use desk/types { Body, Role }

/// Someone the desk helps, with a portal password.
entity Customer {
    id: Uuid
    workspace_id: Workspace.id {
        on_delete: OnDelete.cascade
    }
    name: Body
    blocked: Bool
    flagged: Bool {
        policy: {
            read: [Role.owner]
        }
    }
    note: Body
    portal_hash: PasswordHash {
        policy: {
            read: [Role.owner]
        }
    }

    policy: {
        create: [Role.owner, Role.agent]
        read: [Role.owner, Role.agent]
    }
}
";

/// Opens a ticket only with the customer's portal password.
const PASSWORD_GUARD: &str = "use desk/entities { Customer, CustomerNotFound, Ticket, Workspace, WorkspaceNotFound }
use desk/failures { CustomerBlocked }
use desk/types { Body }

/// Opens a ticket for a customer who gives their portal password.
action open_ticket(workspace_id: Workspace.id, customer_id: Customer.id, password: Password, subject: Body) fails CustomerNotFound, WorkspaceNotFound, CustomerBlocked -> Ticket {
    var customer: Customer = attempt Customer.get(customer_id)
    if (crypto.verify_password(password, customer.portal_hash) == PasswordCheck.wrong) {
        reject CustomerBlocked
    }
    return attempt Ticket.create {
        set: {
            workspace_id: workspace_id
            customer_id: customer.id
            subject: subject
            body: subject
            channel: \"web\"
        }
    }
}
";

/// What a project with the hashed customer and the password guard
/// finds, with `extra` files.
fn password_guarded(extra: &[(&str, &str)]) -> Vec<Diagnostic> {
    let mut files = vec![
        ("domains/desk/entities/customer.jadpo", HASHED_CUSTOMER),
        (ACTION, PASSWORD_GUARD),
    ];
    files.extend_from_slice(extra);
    diagnostics(POLICY, &files)
}

#[test]
fn a_guard_on_a_password_check_lets_nothing_past_it() {
    // A secret is shown to nobody: the guard's failure and every value
    // stored after it still carry the hash, so the action is refused.
    let found = password_guarded(&[]);
    let all = shown(&found);
    let raised = found
        .iter()
        .find(|d| d.message.contains("raises `desk.CustomerBlocked`"))
        .unwrap_or_else(|| panic!("{all}"));
    assert!(raised.message.contains("`Customer.portal_hash`"), "{all}");
    let next = raised.next.as_ref().expect("a next step");
    assert_ne!(next.decides, Decider::Human, "{all}");
    assert!(!next.text.contains("PolicyChange.field_copied"), "{all}");
    assert!(
        found.iter().any(|d| d
            .message
            .contains("stores a value decided by `Customer.portal_hash`")),
        "{all}"
    );
}

#[test]
fn no_acceptance_lets_a_password_check_past_a_guard() {
    let accepted = accepting("\"desk.Customer.portal_hash -> desk.CustomerBlocked\"");
    let found = password_guarded(&[("acceptances/20261126T1000_portal_password.jadpo", &accepted)]);
    let all = shown(&found);
    assert!(
        found
            .iter()
            .any(|d| d.message.contains("raises `desk.CustomerBlocked`")),
        "{all}"
    );
}
