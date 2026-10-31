//! Moving a record to another tenant (area 9, "Moving a record to another
//! tenant"), against live PostgreSQL through `jadpo test`: a listed
//! action's line is checked on the record as it was, and in its new
//! tenant the caller needs what a create of it there would, the entity's
//! own `create:`. So a viewer of a company can't move an owner's
//! membership they hold elsewhere into it to become its owner, nor a
//! project into it they couldn't start there (security audit S1).
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::Path;

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
        r#"use work/entities { Member }

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
        cookie: "work_session"
    }
}
"#,
    ),
    (
        "policy.jadpo",
        r#"use work
use work/entities { Company }
use work/types { CompanyRole }

policy {
    tenant: Company
    actions: {
        work.found_with: [Access.authenticated]
        work.move_membership: [CompanyRole.owner]
        work.move_project: [CompanyRole.owner]
    }
}
"#,
    ),
    (
        "domains/work/types/company_role.jadpo",
        "/// What a member may do in a company.\ntype CompanyRole = Enum {\n    owner\n    editor\n    viewer\n}\n",
    ),
    (
        "domains/work/types/handle.jadpo",
        "type Handle = Text {\n    min_length: 3\n    max_length: 40\n}\n",
    ),
    (
        "domains/work/types/label.jadpo",
        "type Label = Text {\n    min_length: 1\n    max_length: 80\n}\n",
    ),
    (
        "domains/work/types/membership_view.jadpo",
        "use work/entities { CompanyMembership }\n\n/// A membership as its holder sees it.\ntype MembershipView = Output {\n    pick: [CompanyMembership.id, CompanyMembership.company_id, CompanyMembership.role]\n}\n",
    ),
    (
        "domains/work/types/project_view.jadpo",
        "use work/entities { Project }\n\n/// A project as members see it.\ntype ProjectView = Output {\n    pick: [Project.id, Project.name]\n}\n",
    ),
    (
        "domains/work/types/move_to.jadpo",
        "use work/entities { Company }\n\n/// Where a record moves to.\ntype MoveTo = Input {\n    to: Company.id\n}\n",
    ),
    (
        "domains/work/failures/handle_taken.jadpo",
        "type HandleTaken = Failure {\n    kind: FailureKind.conflict\n    message: \"That handle is taken.\"\n}\n",
    ),
    (
        "domains/work/entities/member.jadpo",
        r#"use work/types { Handle }

/// Someone who signs in.
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
        "domains/work/entities/company.jadpo",
        r#"use work/types { CompanyRole, Label }

/// A customer: the tenant every other record belongs to.
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
        "domains/work/entities/company_membership.jadpo",
        r#"use work/entities { Company, Member }
use work/types { CompanyRole }

/// A member's role in a company.
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
        "domains/work/entities/project.jadpo",
        r#"use work/entities { Company }
use work/types { CompanyRole, Label }

/// A piece of work in a company.
entity Project {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    name: Label

    policy: {
        create: [CompanyRole.owner, CompanyRole.editor]
        read: [CompanyRole.owner, CompanyRole.editor, CompanyRole.viewer]
    }
}
"#,
    ),
    (
        "domains/work/actions/register.jadpo",
        r#"use work/entities { Member }
use work/types { Handle }
use work/failures { HandleTaken }

/// Signs a new member up; anyone may.
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
        "domains/work/actions/found_with.jadpo",
        r#"use work/entities { Company, CompanyMembership }
use work/types { CompanyRole, Label }

/// Starts a company with its founder as its owner, giving the membership.
action found_with(name: Label, founder: Principal.user) -> CompanyMembership {
    var company: Company = Company.create {
        set: {
            name: name
        }
    }
    return CompanyMembership.create {
        set: {
            company_id: company.id
            member_id: founder.id
            role: CompanyRole.owner
        }
    }
}
"#,
    ),
    (
        "domains/work/actions/add_member.jadpo",
        r#"use work/entities { Company, CompanyMembership, CompanyNotFound, Member, MemberNotFound }
use work/types { CompanyRole }

/// Gives a member a role in a company; only its owners may.
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
        "domains/work/actions/start_project.jadpo",
        r#"use work/entities { Company, CompanyNotFound, Project }
use work/types { Label, ProjectView }

/// Starts a project in a company.
action start_project(company_id: Company.id, name: Label) fails CompanyNotFound -> ProjectView {
    var project: Project = attempt Project.create {
        set: {
            company_id: company_id
            name: name
        }
    }
    return ProjectView {
        id: project.id
        name: project.name
    }
}
"#,
    ),
    (
        "domains/work/actions/move_membership.jadpo",
        r#"use work/entities { CompanyMembership, CompanyMembershipNotFound, CompanyNotFound }
use work/types { MembershipView, MoveTo }

/// Moves a membership to another company; its company's owners may.
action move_membership(
    membership_id: CompanyMembership.id,
    input: MoveTo
) fails CompanyMembershipNotFound, CompanyNotFound -> MembershipView {
    var moved: CompanyMembership = attempt CompanyMembership.update(membership_id) {
        set: {
            company_id: input.to
        }
    }
    return MembershipView {
        id: moved.id
        company_id: moved.company_id
        role: moved.role
    }
}
"#,
    ),
    (
        "domains/work/actions/move_project.jadpo",
        r#"use work/entities { CompanyNotFound, Project, ProjectNotFound }
use work/types { MoveTo, ProjectView }

/// Moves a project to another company; its company's owners may.
action move_project(project_id: Project.id, input: MoveTo) fails ProjectNotFound, CompanyNotFound -> ProjectView {
    var moved: Project = attempt Project.update(project_id) {
        set: {
            company_id: input.to
        }
    }
    return ProjectView {
        id: moved.id
        name: moved.name
    }
}
"#,
    ),
    (
        "domains/work/routes/moves.jadpo",
        r#"use work/entities { CompanyMembership, Project }
use work/types { MembershipView, MoveTo, ProjectView }
use work/actions { move_membership, move_project }

/// Records moved between companies.
routes Moves {
    prefix: /moves

    /// Moves a membership.
    PATCH /memberships/{membership_id: CompanyMembership.id} {
        input: MoveTo
        output: MembershipView
        run: move_membership(membership_id, input)
    }

    /// Moves a project.
    PATCH /projects/{project_id: Project.id} {
        input: MoveTo
        output: ProjectView
        run: move_project(project_id, input)
    }
}
"#,
    ),
];

const TESTS: &str = r#"use work/entities { CompanyMembership, Member }
use work/types { CompanyRole, MoveTo, ProjectView }
use work/actions { add_member, found_with, register, start_project }

test "a membership moved in from another company gives no role there" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var northwind: CompanyMembership = found_with("Northwind", Principal.user(ann))
    var contoso: CompanyMembership = found_with("Contoso", Principal.user(bob))
    var viewer: CompanyMembership = attempt add_member(northwind.company_id, bob.id, CompanyRole.viewer)
    assert viewer.role == CompanyRole.viewer
    var moved: Response = request {
        method: HttpMethod.patch
        path: "/moves/memberships/{membership_id}"
        params: {
            membership_id: contoso.id
        }
        as: Principal.user(bob)
        input: MoveTo {
            to: northwind.company_id
        }
    }
    assert moved.status == 403
    assert moved.code == "not_permitted"
}

test "an owner of both companies moves a membership between them" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var northwind: CompanyMembership = found_with("Northwind", Principal.user(ann))
    var contoso: CompanyMembership = found_with("Contoso", Principal.user(ann))
    var held: CompanyMembership = attempt add_member(contoso.company_id, bob.id, CompanyRole.viewer)
    var moved: Response = request {
        method: HttpMethod.patch
        path: "/moves/memberships/{membership_id}"
        params: {
            membership_id: held.id
        }
        as: Principal.user(ann)
        input: MoveTo {
            to: northwind.company_id
        }
    }
    assert moved.status == 200
}

test "a project moves only to a company where the mover may start one" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var northwind: CompanyMembership = found_with("Northwind", Principal.user(ann))
    var contoso: CompanyMembership = found_with("Contoso", Principal.user(bob))
    var viewer: CompanyMembership = attempt add_member(northwind.company_id, bob.id, CompanyRole.viewer)
    assert viewer.role == CompanyRole.viewer
    var launch: ProjectView = attempt start_project(contoso.company_id, "Launch")
    var as_viewer: Response = request {
        method: HttpMethod.patch
        path: "/moves/projects/{project_id}"
        params: {
            project_id: launch.id
        }
        as: Principal.user(bob)
        input: MoveTo {
            to: northwind.company_id
        }
    }
    assert as_viewer.status == 403
    var editor: CompanyMembership = attempt add_member(northwind.company_id, bob.id, CompanyRole.editor)
    assert editor.role == CompanyRole.editor
    var as_editor: Response = request {
        method: HttpMethod.patch
        path: "/moves/projects/{project_id}"
        params: {
            project_id: launch.id
        }
        as: Principal.user(bob)
        input: MoveTo {
            to: northwind.company_id
        }
    }
    assert as_editor.status == 200
}
"#;

#[test]
fn a_move_needs_what_a_create_in_the_new_tenant_would() {
    if !jadpo_test::live() {
        return;
    }
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tenant-moves");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    write(&root, "domains/work/routes/moves.test.jadpo", TESTS);
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("3 passed"), "{}", tested.stdout);
}
