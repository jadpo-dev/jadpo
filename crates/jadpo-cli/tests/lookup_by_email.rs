//! Looking up one person by their exact email (area 15, "Looking up one
//! person by email"; chunk 10.13b), against live PostgreSQL: the
//! helpdesk's "add a member by email" (benchmark change H42) written with
//! `User.lookup(email:)`, with no special case for it.
//!
//! - An owner adds the person whose account has the address, and adding
//!   them again is the membership's own conflict.
//! - An address no account has is the action's own not-found failure.
//! - An agent of the workspace, and an owner of another one, are refused
//!   with `not_permitted` before any lookup runs, whatever the address,
//!   so no answer tells them whether an address has an account: the
//!   action's line, `[WorkspaceRole.owner]`, is checked as it starts, in
//!   the workspace whose `id` it is given.
//!
//! The tests run through `jadpo test`. The server is the one
//! `JADPO_TEST_DATABASE_URL` names or one started from the machine's
//! PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::Path;

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

const FILES: [(&str, &str); 14] = [
    (
        "app.jadpo",
        r#"use desk/entities { User }

application {}

principal {
    user: {
        subject: User.handle
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    api_token: {}
}
"#,
    ),
    (
        "policy.jadpo",
        r#"use desk/entities { Workspace }
use desk/types { WorkspaceRole }
use desk

policy {
    tenant: Workspace
    actions: {
        desk.add_member_by_email: [WorkspaceRole.owner]
    }
}
"#,
    ),
    (
        "domains/desk/actions/add_member_by_email.jadpo",
        r#"use desk/entities { Membership, User, Workspace, WorkspaceNotFound }
use desk/types { MemberView, WorkspaceRole }
use desk/failures { AlreadyMember, NoOneByThatEmail }

/// Adds the person whose account has an address to a workspace; only its
/// owners may.
action add_member_by_email(workspace_id: Workspace.id, input: NewMemberByEmail) fails NoOneByThatEmail, AlreadyMember, WorkspaceNotFound -> MemberView {
    match User.lookup(email: input.email) {
        none -> {
            reject NoOneByThatEmail {}
        }
        some(person) -> {
            var member: Membership = attempt Membership.create {
                set: {
                    workspace_id: workspace_id
                    user_id: person.id
                    role: input.role
                }
                conflict: {
                    Membership.one_per_person: AlreadyMember
                }
            }
            return MemberView {
                id: member.id
                user_id: member.user_id
                role: member.role
            }
        }
    }
}

/// Who to add, by the address of their account.
type NewMemberByEmail = Input {
    email: User.email
    role: WorkspaceRole
}
"#,
    ),
    (
        "domains/desk/entities/membership.jadpo",
        r#"use desk/entities { User, Workspace }
use desk/types { WorkspaceRole }

/// A person's role in a workspace.
entity Membership {
    id: Uuid
    workspace_id: Workspace.id {
        on_delete: OnDelete.cascade
    }
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: WorkspaceRole

    membership: {
        scope: workspace_id
        member: user_id
        role: role
    }

    unique: {
        one_per_person: [workspace_id, user_id]
    }

    policy: {
        create: [WorkspaceRole.owner]
        read: [WorkspaceRole.owner, WorkspaceRole.agent]
    }
}
"#,
    ),
    (
        "domains/desk/entities/user.jadpo",
        r#"use desk/types { Handle }

/// Someone who signs in. Their address is theirs alone.
entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
    email: Email {
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
        "domains/desk/entities/workspace.jadpo",
        r#"use desk/types { Handle, WorkspaceRole }

/// The tenant.
entity Workspace {
    id: Uuid
    name: Handle

    policy: {
        read: [WorkspaceRole.owner, WorkspaceRole.agent]
    }
}
"#,
    ),
    (
        "domains/desk/failures/already_member.jadpo",
        r#"/// The person is a member already.
type AlreadyMember = Failure {
    kind: FailureKind.conflict
}
"#,
    ),
    (
        "domains/desk/failures/no_one_by_that_email.jadpo",
        r#"/// No one has that address.
type NoOneByThatEmail = Failure {
    kind: FailureKind.not_found
}
"#,
    ),
    (
        "domains/desk/routes/workspaces.jadpo",
        r#"use desk/entities { Workspace }
use desk/actions { NewMemberByEmail, add_member_by_email }
use desk/types { MemberView }

/// A workspace's members.
routes Workspaces {
    prefix: /workspaces

    POST /{workspace_id: Workspace.id}/members {
        input: NewMemberByEmail
        output: MemberView
        run: add_member_by_email(workspace_id, input)
    }
}
"#,
    ),
    (
        "domains/desk/routes/workspaces.test.jadpo",
        r#"use desk/entities { Membership, User, Workspace }
use desk/actions { NewMemberByEmail }
use desk/types { MemberView, WorkspaceRole }

test "an owner adds a person by the address of their account" {
    var ann: User = make.User {
        email: "ann@desk.example"
    }
    var fay: User = make.User {
        email: "fay@desk.example"
    }
    var acme: Workspace = make.Workspace {}
    var owner: Membership = make.Membership {
        workspace_id: acme.id
        user_id: ann.id
        role: WorkspaceRole.owner
    }
    assert owner.role == WorkspaceRole.owner
    var added: Response<MemberView> = request {
        method: HttpMethod.post
        path: "/workspaces/{workspace_id}/members"
        params: {
            workspace_id: acme.id
        }
        input: NewMemberByEmail {
            email: "fay@desk.example"
            role: WorkspaceRole.agent
        }
        as: Principal.user(ann)
    }
    assert added.status == 200
    match added.output {
        none -> {
            assert false
        }
        some(member) -> {
            assert member.user_id == fay.id
        }
    }
    var again: Response<MemberView> = request {
        method: HttpMethod.post
        path: "/workspaces/{workspace_id}/members"
        params: {
            workspace_id: acme.id
        }
        input: NewMemberByEmail {
            email: "fay@desk.example"
            role: WorkspaceRole.agent
        }
        as: Principal.user(ann)
    }
    assert again.status == 409
    var nobody: Response<MemberView> = request {
        method: HttpMethod.post
        path: "/workspaces/{workspace_id}/members"
        params: {
            workspace_id: acme.id
        }
        input: NewMemberByEmail {
            email: "nobody@desk.example"
            role: WorkspaceRole.agent
        }
        as: Principal.user(ann)
    }
    assert nobody.status == 404
    assert nobody.code == "no_one_by_that_email"
}

test "nobody but an owner learns whether an address has an account" {
    var ann: User = make.User {
        email: "ann@desk.example"
    }
    var cat: User = make.User {
        email: "cat@desk.example"
    }
    var fay: User = make.User {
        email: "fay@desk.example"
    }
    var acme: Workspace = make.Workspace {}
    var globex: Workspace = make.Workspace {}
    var owner: Membership = make.Membership {
        workspace_id: acme.id
        user_id: ann.id
        role: WorkspaceRole.owner
    }
    var agent: Membership = make.Membership {
        workspace_id: acme.id
        user_id: cat.id
        role: WorkspaceRole.agent
    }
    assert owner.role != agent.role
    for address in [fay.email, "nobody@desk.example"] {
        var by_agent: Response<MemberView> = request {
            method: HttpMethod.post
            path: "/workspaces/{workspace_id}/members"
            params: {
                workspace_id: acme.id
            }
            input: NewMemberByEmail {
                email: address
                role: WorkspaceRole.agent
            }
            as: Principal.user(cat)
        }
        assert by_agent.status == 403
        assert by_agent.code == "not_permitted"
        var elsewhere: Response<MemberView> = request {
            method: HttpMethod.post
            path: "/workspaces/{workspace_id}/members"
            params: {
                workspace_id: globex.id
            }
            input: NewMemberByEmail {
                email: address
                role: WorkspaceRole.agent
            }
            as: Principal.user(ann)
        }
        assert elsewhere.status == 403
        assert elsewhere.code == "not_permitted"
    }
}
"#,
    ),
    (
        "domains/desk/types/handle.jadpo",
        r#"/// A person's handle.
type Handle = Text {
    max_length: 40
}
"#,
    ),
    (
        "domains/desk/types/member_view.jadpo",
        r#"use desk/entities { Membership }

/// A membership, as members see it.
type MemberView = Output {
    pick: [Membership.id, Membership.user_id, Membership.role]
}
"#,
    ),
    (
        "domains/desk/types/workspace_role.jadpo",
        r#"/// What a member may do.
type WorkspaceRole = Enum {
    owner
    agent
}
"#,
    ),
];

/// The project, for the compiled run (`compiled.rs`).
pub(crate) fn files() -> Vec<(String, String)> {
    FILES
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect()
}

#[test]
fn an_owner_adds_a_person_by_email_and_nobody_else_learns_who_has_an_account() {
    if !jadpo_test::live() {
        return;
    }
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lookup-by-email");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("2 route tests"), "{}", tested.stdout);
}
