//! Policy's checks (area 9, chunk 4.4) beyond the one example per rule on
//! the page: which fields an ordinary update and a listed action may
//! change, what a route reaches through the actions it calls, when the
//! route checks wait for the routes to read cleanly, tenancy's reference,
//! and what isn't built yet.

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
fn a_fields_own_policy_and_scopes_wait_for_their_chunks() {
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
    assert_eq!(
        rules,
        ["toolchain.not_built_yet:", "toolchain.not_built_yet:"],
        "{found:#?}"
    );
}
