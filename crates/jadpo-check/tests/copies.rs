//! Copying a restricted field into a record more people read (chunk
//! 10.7c, area 9 "What else shows a field"): refused unless an acceptance
//! names that field flow, followed through a query's `Output` as through
//! a direct read, and the error names a reader who really can't read the
//! source field, says whether the role, the tenant or the record is the
//! cause, and points at the acceptance when that is the way through.

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

const BODY: &str = "/// A message's text.
type Body = Text {
    max_length: 500
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

/// Staff read saved answers; viewers don't.
const SAVED: &str = "use desk/entities { Workspace }
use desk/types { Body, Role }

/// A saved answer staff reuse.
entity SavedAnswer {
    id: Uuid
    workspace_id: Workspace.id {
        on_delete: OnDelete.cascade
    }
    body: Body

    policy: {
        create: [Role.owner]
        read: [Role.owner, Role.agent]
    }
}
";

/// Every member reads an answer.
const ANSWER: &str = "use desk/entities { Workspace }
use desk/types { Body, Role }

/// An answer every member reads.
entity Answer {
    id: Uuid
    workspace_id: Workspace.id {
        on_delete: OnDelete.cascade
    }
    body: Body

    policy: {
        create: [Role.owner, Role.agent]
        read: [Role.owner, Role.agent, Role.viewer]
    }
}
";

/// Staff read drafts, as they read saved answers.
const DRAFT: &str = "use desk/entities { Workspace }
use desk/types { Body, Role }

/// A draft only staff read.
entity Draft {
    id: Uuid
    workspace_id: Workspace.id {
        on_delete: OnDelete.cascade
    }
    body: Body

    policy: {
        create: [Role.owner, Role.agent]
        read: [Role.owner, Role.agent]
    }
}
";

const SAVED_VIEW: &str = "use desk/entities { SavedAnswer }

/// A saved answer.
type SavedView = Output {
    pick: [SavedAnswer.id, SavedAnswer.workspace_id, SavedAnswer.body]
}
";

const SAVED_BY_ID: &str = "use desk/entities { SavedAnswer, SavedAnswerNotFound }
use desk/types { SavedView }

/// One saved answer.
query saved_by_id(saved_id: SavedAnswer.id) fails SavedAnswerNotFound -> SavedView {
    from: SavedAnswer
    where: SavedAnswer.id == saved_id
    missing: SavedAnswerNotFound
}
";

/// An action storing the saved answer's `body`, read by `read` (a
/// statement giving `saved`), in `target`'s `body`, in the workspace
/// `workspace`.
fn copying(name: &str, read: &str, target: &str, workspace: &str) -> String {
    // A workspace read off the saved answer is one that is there, so the
    // create can't fail for it.
    let (named, workspace_param, fails, attempt) = if workspace.starts_with("saved.") {
        ("", "", "", "")
    } else {
        (
            ", Workspace, WorkspaceNotFound",
            "workspace_id: Workspace.id, ",
            ", WorkspaceNotFound",
            "attempt ",
        )
    };
    format!(
        "use desk/entities {{ {target}, SavedAnswer, SavedAnswerNotFound{named} }}
{view}
/// Writes a saved answer's text.
action {name}({workspace_param}saved_id: SavedAnswer.id) fails SavedAnswerNotFound{fails} -> {target} {{
    {read}
    return {attempt}{target}.create {{
        set: {{
            workspace_id: {workspace}
            body: saved.body
        }}
    }}
}}
",
        view = if read.contains("saved_by_id") {
            "use desk/queries { saved_by_id }\nuse desk/types { SavedView }\n"
        } else {
            ""
        },
    )
}

const DIRECT: &str = "var saved: SavedAnswer = attempt SavedAnswer.get(saved_id)";
const VIEW: &str = "var saved: SavedView = attempt saved_by_id(saved_id)";

fn diagnostics(extra: &[(&str, &str)]) -> Vec<Diagnostic> {
    let mut files: Vec<(&str, &str)> = vec![
        ("app.jadpo", APP),
        ("authentication.jadpo", AUTHENTICATION),
        ("policy.jadpo", POLICY),
        ("domains/desk/types/role.jadpo", ROLE),
        ("domains/desk/types/body.jadpo", BODY),
        ("domains/desk/types/saved_view.jadpo", SAVED_VIEW),
        ("domains/desk/entities/user.jadpo", USER),
        ("domains/desk/entities/workspace.jadpo", WORKSPACE),
        ("domains/desk/entities/membership.jadpo", MEMBERSHIP),
        ("domains/desk/entities/saved_answer.jadpo", SAVED),
        ("domains/desk/entities/answer.jadpo", ANSWER),
        ("domains/desk/entities/draft.jadpo", DRAFT),
        ("domains/desk/queries/saved_by_id.jadpo", SAVED_BY_ID),
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

const ACCEPTED: &str = "/// Sam Okafor (support lead), in chat on 2026-11-25: \"Viewers may see a saved answer's text once it is sent as an answer.\"
acceptance {
    changes: {
        \"desk.SavedAnswer.body -> desk.Answer.body\": PolicyChange.field_copied
    }
}
";

#[test]
fn a_copy_into_a_record_more_people_read_names_the_role_that_cannot_read_it() {
    let action = copying("answer_from_saved", DIRECT, "Answer", "workspace_id");
    let found = diagnostics(&[("domains/desk/actions/answer_from_saved.jadpo", &action)]);
    let all = shown(&found);
    assert_eq!(found.len(), 1, "{all}");
    let d = &found[0];
    assert_eq!(d.rule.id, "policy.output_field_unproved", "{all}");
    assert_eq!(
        d.message,
        "`answer_from_saved` stores `SavedAnswer.body` in `Answer.body`, which not everyone who may read `body` may read"
    );
    let reason = d.reason.as_deref().unwrap_or("");
    assert!(
        reason.contains("`Role.viewer` may read `Answer.body` but not `SavedAnswer.body`"),
        "{all}"
    );
    assert!(!reason.contains("`Role.owner` among them"), "{all}");
    assert!(reason.contains("The cause is the role"), "{all}");
    let next = d.next.as_ref().expect("a next step");
    assert_eq!(next.decides, Decider::Human, "{all}");
    assert!(
        next.text
            .contains("\"desk.SavedAnswer.body -> desk.Answer.body\": PolicyChange.field_copied"),
        "{all}"
    );
    assert!(next.text.contains("ask your user"), "{all}");
}

#[test]
fn a_copy_read_through_a_querys_output_is_proved_as_a_direct_read_is() {
    let action = copying("answer_through_view", VIEW, "Answer", "workspace_id");
    let found = diagnostics(&[("domains/desk/actions/answer_through_view.jadpo", &action)]);
    let all = shown(&found);
    assert_eq!(found.len(), 1, "{all}");
    assert_eq!(
        found[0].message,
        "`answer_through_view` stores `SavedAnswer.body` in `Answer.body`, which not everyone who may read `body` may read",
        "{all}"
    );
    assert!(
        found[0]
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("`Role.viewer` may read `Answer.body` but not `SavedAnswer.body`"),
        "{all}"
    );
}

#[test]
fn an_acceptance_naming_the_field_flow_allows_the_copy() {
    let direct = copying("answer_from_saved", DIRECT, "Answer", "workspace_id");
    let view = copying("answer_through_view", VIEW, "Answer", "workspace_id");
    let found = diagnostics(&[
        ("domains/desk/actions/answer_from_saved.jadpo", &direct),
        ("domains/desk/actions/answer_through_view.jadpo", &view),
        (
            "acceptances/20261125T1000_send_saved_answers.jadpo",
            ACCEPTED,
        ),
    ]);
    let unproved: Vec<&Diagnostic> = found
        .iter()
        .filter(|d| d.rule.id == "policy.output_field_unproved")
        .collect();
    assert!(unproved.is_empty(), "{}", shown(&found));
}

#[test]
fn an_acceptance_allows_only_the_flow_it_names() {
    // The same source into another field is still refused: here a
    // draft's, whose readers hold the source's roles in a workspace the
    // request named.
    let action = copying("draft_from_saved", DIRECT, "Draft", "workspace_id");
    let found = diagnostics(&[
        ("domains/desk/actions/draft_from_saved.jadpo", &action),
        (
            "acceptances/20261125T1000_send_saved_answers.jadpo",
            ACCEPTED,
        ),
    ]);
    let unproved: Vec<&Diagnostic> = found
        .iter()
        .filter(|d| d.rule.id == "policy.output_field_unproved")
        .collect();
    assert_eq!(unproved.len(), 1, "{}", shown(&found));
    assert_eq!(
        unproved[0].message,
        "`draft_from_saved` stores `SavedAnswer.body` in `Draft.body`, which not everyone who may read `body` may read"
    );
}

#[test]
fn a_copy_whose_readers_hold_the_role_in_another_tenant_says_the_tenant_is_the_cause() {
    let action = copying("draft_from_saved", DIRECT, "Draft", "workspace_id");
    let found = diagnostics(&[("domains/desk/actions/draft_from_saved.jadpo", &action)]);
    let all = shown(&found);
    assert_eq!(found.len(), 1, "{all}");
    let d = &found[0];
    let reason = d.reason.as_deref().unwrap_or("");
    assert!(reason.contains("The cause is the tenant"), "{all}");
    let next = d.next.as_ref().expect("a next step");
    assert_eq!(next.decides, Decider::Agent, "{all}");
    assert!(
        next.text
            .contains("Set `Draft.workspace_id` from the `SavedAnswer` that `body` is read from"),
        "{all}"
    );
    // Set so, it compiles.
    let fixed = copying("draft_from_saved", DIRECT, "Draft", "saved.workspace_id");
    let found = diagnostics(&[("domains/desk/actions/draft_from_saved.jadpo", &fixed)]);
    assert!(found.is_empty(), "{}", shown(&found));
}
