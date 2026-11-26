//! A lookup isn't a copy (chunk 10.9c, area 9 "What else shows a
//! field"): a restricted field compared by equality with a request value,
//! only to choose the `id` of an existing record, is stored with no
//! acceptance, since its readers learn only which record was chosen. A
//! value worked out from the field's content, a key the readers can know
//! (a literal, or one also stored where they read it), and the field's
//! value itself are still copies a person approves (chunk 10.7c).

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

/// Staff read contacts; viewers never do.
const CONTACT: &str = "use desk/entities { Workspace }
use desk/types { Body, Role }

/// Someone who writes to the desk.
entity Contact {
    id: Uuid
    workspace_id: Workspace.id {
        on_delete: OnDelete.cascade
    }
    address: Body

    policy: {
        create: [Role.owner, Role.agent]
        read: [Role.owner, Role.agent]
    }
}
";

/// Every member reads people, but only staff read their address.
const PERSON: &str = "use desk/entities { Workspace }
use desk/types { Body, Role }

/// Someone every member sees, whose address only staff see.
entity Person {
    id: Uuid
    workspace_id: Workspace.id {
        on_delete: OnDelete.cascade
    }
    address: Body {
        policy: {
            read: [Role.owner, Role.agent]
        }
    }

    policy: {
        create: [Role.owner, Role.agent]
        read: [Role.owner, Role.agent, Role.viewer]
    }
}
";

/// Every member reads a case, and which contact it is for.
const CASE: &str = "use desk/entities { Contact, Person, Workspace }
use desk/types { Body, Role }

/// A request for help.
entity Case {
    id: Uuid
    workspace_id: Workspace.id {
        on_delete: OnDelete.cascade
    }
    contact_id: Contact.id? {
        on_delete: OnDelete.cleared
    }
    person_id: Person.id? {
        on_delete: OnDelete.cleared
    }
    subject: Body

    policy: {
        create: [Role.owner, Role.agent]
        read: [Role.owner, Role.agent, Role.viewer]
    }
}
";

const CONTACT_ROW: &str = "use desk/entities { Contact }

/// A contact to match an address against.
type ContactRow = Output {
    pick: [Contact.id, Contact.address]
}
";

const CONTACTS_OF: &str = "use desk/entities { Contact, Workspace }

/// A workspace's contacts.
query contacts_of(workspace_id: Workspace.id) -> List<Contact> {
    from: Contact
    where: Contact.workspace_id == workspace_id
    order: [Contact.id asc]
    limit: every
}
";

const CONTACT_ROWS: &str = "use desk/entities { Contact, Workspace }
use desk/types { ContactRow }

/// A workspace's contacts, as rows.
query contact_rows(workspace_id: Workspace.id) -> List<ContactRow> {
    from: Contact
    where: Contact.workspace_id == workspace_id
    order: [Contact.id asc]
    limit: every
}
";

const PEOPLE_OF: &str = "use desk/entities { Person, Workspace }

/// A workspace's people.
query people_of(workspace_id: Workspace.id) -> List<Person> {
    from: Person
    where: Person.workspace_id == workspace_id
    order: [Person.id asc]
    limit: every
}
";

/// Finds a contact by address in a loop over rows, ignoring case.
const CONTACT_FOR: &str = "use desk/entities { Contact, Workspace }
use desk/types { Body, ContactRow }
use desk/queries { contact_rows }

/// The contact with an address, ignoring case.
action contact_for(workspace_id: Workspace.id, address: Body) -> Contact.id? {
    var rows: List<ContactRow> = contact_rows(workspace_id)
    for row in rows {
        if (text.lower(row.address) == text.lower(address)) {
            return row.id
        }
    }
    return none
}
";

/// An action opening a case for the contact `predicate` (an inline
/// function of `c`) picks with `list.filter`, its subject `subject`.
fn filtered(name: &str, predicate: &str, subject: &str) -> String {
    format!(
        "use desk/entities {{ Case, Contact, ContactNotFound, Workspace, WorkspaceNotFound }}
use desk/types {{ Body }}
use desk/queries {{ contacts_of }}

/// Opens a case for the contact a filter picks.
action {name}(workspace_id: Workspace.id, address: Body, subject: Body) fails WorkspaceNotFound, ContactNotFound -> Case {{
    var contacts: List<Contact> = contacts_of(workspace_id)
    var matched: List<Contact> = list.filter(contacts, c -> {predicate})
    var mut contact_id: Contact.id? = none
    match list.first(matched) {{
        some(found) -> {{
            contact_id = found.id
        }}
        none -> {{
            contact_id = none
        }}
    }}
    return attempt Case.create {{
        set: {{
            workspace_id: workspace_id
            contact_id: contact_id
            person_id: none
            subject: {subject}
        }}
    }}
}}
"
    )
}

/// Adds a contact with an address.
const ADD_CONTACT: &str = "use desk/entities { Contact, Workspace, WorkspaceNotFound }
use desk/types { Body }

/// Adds a contact.
action add_contact(workspace_id: Workspace.id, address: Body) fails WorkspaceNotFound -> Contact.id {
    var added: Contact = attempt Contact.create {
        set: {
            workspace_id: workspace_id
            address: address
        }
    }
    return added.id
}
";

/// Finds the contact with `list.find`, ignoring case, or adds one.
const FIND_OR_ADD: &str = "use desk/entities { Case, Contact, ContactNotFound, Workspace, WorkspaceNotFound }
use desk/types { Body }
use desk/actions { add_contact }
use desk/queries { contacts_of }

/// Opens a case for the contact with an address, adding one if none has it.
action case_found_or_added(workspace_id: Workspace.id, address: Body, subject: Body) fails WorkspaceNotFound, ContactNotFound -> Case {
    var contacts: List<Contact> = contacts_of(workspace_id)
    var found: Contact? = list.find(contacts, c -> text.equals_ignoring_case(c.address, address))
    var mut contact_id: Contact.id? = none
    match found {
        some(contact) -> {
            contact_id = contact.id
        }
        none -> {
            var added: Contact.id = attempt add_contact(workspace_id, address)
            contact_id = added
        }
    }
    return attempt Case.create {
        set: {
            workspace_id: workspace_id
            contact_id: contact_id
            person_id: none
            subject: subject
        }
    }
}
";

/// Stores what `contact_for` finds for `key`.
fn looped(name: &str, key: &str) -> String {
    format!(
        "use desk/entities {{ Case, Contact, ContactNotFound, Workspace, WorkspaceNotFound }}
use desk/types {{ Body }}
use desk/actions {{ contact_for }}

/// Opens a case for the contact a loop finds.
action {name}(workspace_id: Workspace.id, address: Body, subject: Body) fails WorkspaceNotFound, ContactNotFound -> Case {{
    var contact_id: Contact.id? = contact_for(workspace_id, {key})
    return attempt Case.create {{
        set: {{
            workspace_id: workspace_id
            contact_id: contact_id
            person_id: none
            subject: subject
        }}
    }}
}}
"
    )
}

/// Picks a person by an address only staff may read.
const PERSON_LOOKUP: &str = "use desk/entities { Case, Person, PersonNotFound, Workspace, WorkspaceNotFound }
use desk/types { Body }
use desk/queries { people_of }

/// Opens a case for the person with an address.
action case_for_person(workspace_id: Workspace.id, address: Body, subject: Body) fails WorkspaceNotFound, PersonNotFound -> Case {
    var people: List<Person> = people_of(workspace_id)
    var found: Person? = list.find(people, p -> p.address == address)
    var mut person_id: Person.id? = none
    match found {
        some(person) -> {
            person_id = person.id
        }
        none -> {
            person_id = none
        }
    }
    return attempt Case.create {
        set: {
            workspace_id: workspace_id
            contact_id: none
            person_id: person_id
            subject: subject
        }
    }
}
";

fn diagnostics(extra: &[(&str, &str)]) -> Vec<Diagnostic> {
    let mut files: Vec<(&str, &str)> = vec![
        ("app.jadpo", APP),
        ("authentication.jadpo", AUTHENTICATION),
        ("policy.jadpo", POLICY),
        ("domains/desk/types/role.jadpo", ROLE),
        ("domains/desk/types/body.jadpo", BODY),
        ("domains/desk/types/contact_row.jadpo", CONTACT_ROW),
        ("domains/desk/entities/user.jadpo", USER),
        ("domains/desk/entities/workspace.jadpo", WORKSPACE),
        ("domains/desk/entities/membership.jadpo", MEMBERSHIP),
        ("domains/desk/entities/contact.jadpo", CONTACT),
        ("domains/desk/entities/person.jadpo", PERSON),
        ("domains/desk/entities/case.jadpo", CASE),
        ("domains/desk/queries/contacts_of.jadpo", CONTACTS_OF),
        ("domains/desk/queries/contact_rows.jadpo", CONTACT_ROWS),
        ("domains/desk/queries/people_of.jadpo", PEOPLE_OF),
        ("domains/desk/actions/contact_for.jadpo", CONTACT_FOR),
    ];
    files.extend_from_slice(extra);
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files);
    jadpo_check::check(&db, project)
        .iter()
        .filter(|d| d.rule.id != "data.personal_unread")
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

const CHOSEN: &str = "\"desk.Contact.address -> desk.Case.contact_id\"";

#[test]
fn an_id_chosen_by_an_equal_address_is_stored_with_no_acceptance() {
    let action = filtered("case_by_address", "c.address == address", "subject");
    let found = diagnostics(&[("domains/desk/actions/case_by_address.jadpo", &action)]);
    assert!(found.is_empty(), "{}", shown(&found));
}

#[test]
fn a_lookup_ignoring_case_that_adds_a_contact_when_none_matches_compiles() {
    let found = diagnostics(&[
        ("domains/desk/actions/add_contact.jadpo", ADD_CONTACT),
        (
            "domains/desk/actions/case_found_or_added.jadpo",
            FIND_OR_ADD,
        ),
    ]);
    assert!(found.is_empty(), "{}", shown(&found));
}

#[test]
fn an_id_a_called_action_finds_in_a_loop_is_stored_with_no_acceptance() {
    let action = looped("case_by_loop", "address");
    let found = diagnostics(&[("domains/desk/actions/case_by_loop.jadpo", &action)]);
    assert!(found.is_empty(), "{}", shown(&found));
}

#[test]
fn an_id_chosen_by_what_the_address_holds_is_a_copy() {
    let action = filtered(
        "case_for_vip",
        "text.ends_with(c.address, \"@vip.test\")",
        "subject",
    );
    let found = diagnostics(&[("domains/desk/actions/case_for_vip.jadpo", &action)]);
    let d = asks_for(&found, CHOSEN);
    assert_eq!(
        d.message,
        "`case_for_vip` stores a value decided by `Contact.address` in `Case.contact_id`, which not everyone who may read `contact_id` may read"
    );
}

#[test]
fn a_lookup_by_a_key_written_in_the_code_is_a_copy() {
    let action = filtered(
        "case_for_boss",
        "c.address == \"boss@acme.test\"",
        "subject",
    );
    let found = diagnostics(&[("domains/desk/actions/case_for_boss.jadpo", &action)]);
    asks_for(&found, CHOSEN);
}

#[test]
fn a_called_lookup_passed_a_key_written_in_the_code_is_a_copy() {
    let action = looped("case_for_boss", "\"boss@acme.test\"");
    let found = diagnostics(&[("domains/desk/actions/case_for_boss.jadpo", &action)]);
    asks_for(&found, CHOSEN);
}

#[test]
fn a_lookup_whose_key_is_also_stored_where_the_readers_read_it_is_a_copy() {
    // Viewers read the subject, so they would know which contact has
    // that address.
    let action = filtered("case_named_by_address", "c.address == address", "address");
    let found = diagnostics(&[("domains/desk/actions/case_named_by_address.jadpo", &action)]);
    let d = asks_for(&found, CHOSEN);
    let reason = d.reason.as_deref().unwrap_or("");
    assert!(
        reason.contains("`address`, which it was compared with, is stored in `Case.subject`"),
        "{}",
        shown(&found)
    );
}

#[test]
fn storing_the_found_address_itself_still_needs_an_acceptance() {
    // The subject is the contact's own address: a copy, as in 10.7c.
    let action = filtered("case_titled_by_contact", "c.address == address", "title")
        .replace(
            "    var mut contact_id: Contact.id? = none\n",
            "    var mut contact_id: Contact.id? = none\n    var mut title: Body = subject\n",
        )
        .replace(
            "            contact_id = found.id\n",
            "            contact_id = found.id\n            title = found.address\n",
        );
    let found = diagnostics(&[("domains/desk/actions/case_titled_by_contact.jadpo", &action)]);
    let d = asks_for(&found, "\"desk.Contact.address -> desk.Case.subject\"");
    assert_eq!(
        d.message,
        "`case_titled_by_contact` stores `Contact.address` in `Case.subject`, which not everyone who may read `subject` may read"
    );
}

#[test]
fn a_lookup_by_a_field_not_every_caller_may_read_is_a_copy() {
    // Viewers may reach a person but not their address, so nothing shows
    // the caller who sent the address may read it.
    let found = diagnostics(&[("domains/desk/actions/case_for_person.jadpo", PERSON_LOOKUP)]);
    asks_for(&found, "\"desk.Person.address -> desk.Case.person_id\"");
}

#[test]
fn an_acceptance_allows_an_id_chosen_by_what_the_address_holds() {
    let action = filtered(
        "case_for_vip",
        "text.ends_with(c.address, \"@vip.test\")",
        "subject",
    );
    let accepted = format!(
        "/// Sam Okafor (support lead), in chat on 2026-11-26: \"Viewers may see which contact is a VIP's.\"
acceptance {{
    changes: {{
        {CHOSEN}: PolicyChange.field_copied
    }}
}}
"
    );
    let found = diagnostics(&[
        ("domains/desk/actions/case_for_vip.jadpo", &action),
        ("acceptances/20261126T1000_vip_cases.jadpo", &accepted),
    ]);
    assert!(found.is_empty(), "{}", shown(&found));
}

#[test]
fn which_turn_of_a_loop_chose_a_value_that_is_no_id_is_still_a_copy() {
    // How many contacts match is decided by what each one holds, so a
    // count is no lookup, whatever the loop.
    let action = "use desk/entities { Case, Workspace, WorkspaceNotFound }
use desk/types { Body, ContactRow }
use desk/queries { contact_rows }

/// Opens a case titled by whether a contact has a long address.
action case_for_long(workspace_id: Workspace.id, subject: Body, long: Body) fails WorkspaceNotFound -> Case {
    var rows: List<ContactRow> = contact_rows(workspace_id)
    var mut title: Body = subject
    for row in rows {
        if (text.length(row.address) > 40) {
            title = long
        }
    }
    return attempt Case.create {
        set: {
            workspace_id: workspace_id
            contact_id: none
            person_id: none
            subject: title
        }
    }
}
";
    let found = diagnostics(&[("domains/desk/actions/case_for_long.jadpo", action)]);
    asks_for(&found, "\"desk.Contact.address -> desk.Case.subject\"");
}

/// A page of cases whose items gained the contact's address as a flat
/// field (chunk 10.9d): the error says how an include fills it.
#[test]
fn a_flat_field_of_a_related_record_on_a_pages_items_points_at_the_include() {
    let row = "use desk/entities { Case, Contact }

/// A case in a list.
type CaseRow = Output {
    pick: [Case.id, Case.subject]
    contact_address: Contact.address?
}
";
    let page = "use desk/entities { Case, Workspace }
use desk/types { CaseRow }

/// A page of cases.
query case_page(workspace_id: Workspace.id, after: PageCursor?) -> Page<CaseRow> {
    from: Case
    where: Case.workspace_id == workspace_id
    order: [Case.id desc]
    page: {
        size: 25
        after: after
    }
}
";
    let found = diagnostics(&[
        ("domains/desk/types/case_row.jadpo", row),
        ("domains/desk/queries/case_page.jadpo", page),
    ]);
    let all = shown(&found);
    let d = found
        .iter()
        .find(|d| d.rule.id == "query.result_field")
        .unwrap_or_else(|| panic!("{all}"));
    assert_eq!(
        d.next.as_ref().map(|next| next.text.as_str()),
        Some(
            "Fill it from an include: in place of `contact_address`, give `CaseRow` a field `contact: ContactLabel?`, an `Output` that picks `Contact.address`, and add `contact: Case.contact_id` to the query's `include:` (`jadpo help topic order-and-pages`)"
        ),
        "{all}"
    );
}
