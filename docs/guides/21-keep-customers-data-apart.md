# Keep customers' data apart
<!-- topic: guide-keep-customers-data-apart -->

When many customer companies share your app, one must never see another's data. In
Jadpo you name the entity that is a customer, and every record then belongs to one. A
grant only ever applies inside companies where the caller has a membership, and that
holds for every query and write in the app, with no filter for you to remember. This
guide makes a work tracker where each company's projects and chores stay its own.

You start from an app where people sign in, with an action called `register` that
stores a new `Member`, and with queries and output types for projects and chores.
[Add sign-in](05-add-sign-in.md) and [Query records](03-query-records.md) show how to
write those. Everything else this page uses is shown.

```jadpo check project=guide-tenancy file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "work_session"
    }
}
```

```jadpo check project=guide-tenancy file=domains/work/failures/handle_taken.jadpo hidden
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
```

```jadpo check project=guide-tenancy file=domains/work/actions/register.jadpo hidden
use work/entities { Member }
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
```

```jadpo check project=guide-tenancy file=domains/work/types/project_card.jadpo hidden
use work/entities { Project }

/// A project as members see it.
type ProjectCard = Output {
    pick: [Project.id, Project.name]
}
```

```jadpo check project=guide-tenancy file=domains/work/types/chore_card.jadpo hidden
use work/entities { Chore }

/// A chore on its own.
type ChoreCard = Output {
    pick: [Chore.id, Chore.title]
}
```

```jadpo check project=guide-tenancy file=domains/work/types/chore_view.jadpo hidden
use work/entities { Chore }
use work/types { ProjectCard }

/// A chore with the project it belongs to.
type ChoreView = Output {
    pick: [Chore.id, Chore.title]
    project: ProjectCard
}
```

```jadpo check project=guide-tenancy file=domains/work/actions/start_project.jadpo hidden
use work/entities { Company, CompanyNotFound, Project }
use work/types { Label, ProjectCard }

/// Starts a project in a company.
action start_project(company_id: Company.id, name: Label) fails CompanyNotFound -> ProjectCard {
    var project: Project = attempt Project.create {
        set: {
            company_id: company_id
            name: name
        }
    }
    return ProjectCard {
        id: project.id
        name: project.name
    }
}
```

```jadpo check project=guide-tenancy file=domains/work/queries/company_chores.jadpo hidden
use work/entities { Chore, Company }
use work/types { ChoreView }

/// A company's chores, each with its project.
query company_chores(company_id: Company.id) -> List<ChoreView> {
    from: Chore
    where: Chore.company_id == company_id
    order: [Chore.title asc]
    limit: 100
    include: {
        project: Chore.project_id
    }
}
```

## Name the tenant

A tenant is one record of the entity you choose. The root `policy.jadpo` names that
entity once. An action that isn't reached through an entity's own list, such as starting
a company, is listed under `actions:`.

`policy.jadpo`:

```jadpo check project=guide-tenancy file=policy.jadpo
use work/entities { Company }
use work

policy {
    tenant: Company
    actions: {
        work.found_company: [Access.authenticated]
    }
}
```

Naming a tenant adds a rule to every entity: it needs a reference to `Company`. Leave one out
and `jadpo check` refuses it:

```text
error: `Note` doesn't belong to a `Company`
 --> domains/work/entities/note.jadpo:4:8
  |
4 | entity Note {
  |        ^^^^ no reference to `Company`
  reason: Root `policy { }` names `Company` as the tenant, so every entity carries its own reference to it and its grants apply only within the `Company`s where the caller has a membership; an entity every tenant shares says `tenant: Tenancy.shared`.
  next: Add a field typed `Company.id` to `Note`, or `tenant: Tenancy.shared` to its `policy:` (agent)
  rule: policy.scope_missing
  help: https://jadpo.dev/docs/errors/policy.scope_missing
```

`Member` is the other way out: its records belong to no company, so its `policy:` says
`tenant: Tenancy.shared`. A person reads a note saying so when it goes in. It is who people
sign in as, which `principal` says in `app.jadpo`, by the unique `handle` they sign in with.

`app.jadpo`:

```jadpo check project=guide-tenancy file=app.jadpo
use work/entities { Member }

application {}

principal {
    user: {
        subject: Member.handle
    }
}
```

`domains/work/types/handle.jadpo`:

```jadpo check project=guide-tenancy file=domains/work/types/handle.jadpo
type Handle = Text {
    min_length: 3
    max_length: 40
}
```

`domains/work/entities/member.jadpo`:

```jadpo check project=guide-tenancy file=domains/work/entities/member.jadpo
use work/types { Handle }

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
```

## Say who belongs where

People belong to a company through a membership, a stored record saying which member holds
which role in which company. Its `membership:` section names those three fields. Grants then
name roles: `CompanyRole.viewer` in a `read:` list lets in members who hold that role in the
record's company.

`domains/work/types/company_role.jadpo`:

```jadpo check project=guide-tenancy file=domains/work/types/company_role.jadpo
/// What a member may do in a company.
type CompanyRole = Enum {
    owner
    viewer
}
```

`Label` is the short text a company, a project and a chore are each named with.

`domains/work/types/label.jadpo`:

```jadpo check project=guide-tenancy file=domains/work/types/label.jadpo
type Label = Text {
    min_length: 1
    max_length: 80
}
```

`domains/work/entities/company.jadpo`:

```jadpo check project=guide-tenancy file=domains/work/entities/company.jadpo
use work/types { CompanyRole, Label }

/// A customer: the tenant every other record belongs to.
entity Company {
    id: Uuid
    name: Label

    policy: {
        read: [CompanyRole.owner, CompanyRole.viewer]
    }
}
```

`domains/work/entities/company_membership.jadpo`:

```jadpo check project=guide-tenancy file=domains/work/entities/company_membership.jadpo
use work/entities { Company, Member }
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
```

## Found a company and add people

A new company has no members, so nobody holds a role that could let them create its first
membership. That first one is allowed in one place only: the action that creates the
company, naming the caller as the member. Every other membership needs someone who belongs
already, so nobody can make themselves a member of someone else's company.

`domains/work/actions/found_company.jadpo`:

```jadpo check project=guide-tenancy file=domains/work/actions/found_company.jadpo
use work/entities { Company, CompanyMembership }
use work/types { CompanyRole, Label }

/// Starts a company with its founder as its owner.
action found_company(name: Label, founder: Principal.user) -> Company {
    var company: Company = Company.create {
        set: {
            name: name
        }
    }
    var owner: CompanyMembership = CompanyMembership.create {
        set: {
            company_id: company.id
            member_id: founder.id
            role: CompanyRole.owner
        }
    }
    assert owner.role == CompanyRole.owner
    return company
}
```

`domains/work/actions/add_member.jadpo`:

```jadpo check project=guide-tenancy file=domains/work/actions/add_member.jadpo
use work/entities { Company, CompanyMembership, CompanyNotFound, Member, MemberNotFound }
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
```

## Give each record its tenant

Each entity has its own reference to `Company`, its tenancy reference, even a chore whose
project already has one. A request has no current tenant of its own, and the caller carries
no company. A record's tenant is the one its reference names, and someone who belongs to
several companies is let in to each by their membership there.

`domains/work/entities/project.jadpo`:

```jadpo check project=guide-tenancy file=domains/work/entities/project.jadpo
use work/entities { Company }
use work/types { CompanyRole, Label }

/// A piece of work in a company.
entity Project {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    name: Label

    policy: {
        create: [CompanyRole.owner]
        read: [CompanyRole.owner, CompanyRole.viewer]
    }
}
```

`domains/work/entities/chore.jadpo`:

```jadpo check project=guide-tenancy file=domains/work/entities/chore.jadpo
use work/entities { Company, Project }
use work/types { CompanyRole, Label }

/// One step of a project.
entity Chore {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    project_id: Project.id {
        on_delete: OnDelete.cascade
    }
    title: Label

    policy: {
        create: [CompanyRole.owner]
        read: [CompanyRole.owner, CompanyRole.viewer]
    }
}
```

A route says which company it works in through its input.

`domains/work/actions/add_chore.jadpo`:

```jadpo check project=guide-tenancy file=domains/work/actions/add_chore.jadpo
use work/entities { Chore, Company, CompanyNotFound, Project, ProjectNotFound }
use work/types { ChoreCard, Label }

/// What adding a chore takes.
type AddChore = Input {
    company_id: Company.id
    project_id: Project.id
    title: Label
}

/// Adds a chore to a project.
action add_chore(input: AddChore) fails CompanyNotFound, ProjectNotFound -> ChoreCard {
    var chore: Chore = attempt Chore.create {
        set: {
            company_id: input.company_id
            project_id: input.project_id
            title: input.title
        }
    }
    return ChoreCard {
        id: chore.id
        title: chore.title
    }
}
```

`domains/work/routes/chores.jadpo`:

```jadpo check project=guide-tenancy file=domains/work/routes/chores.jadpo
use work/entities { Company }
use work/types { ChoreCard, ChoreView }
use work/actions { AddChore, add_chore }
use work/queries { company_chores }

/// A company's chores.
routes Chores {
    prefix: /chores

    /// Adds a chore.
    POST / {
        input: AddChore
        output: ChoreCard
        status: SuccessStatus.created
        run: add_chore(input)
    }

    /// A company's chores, each with its project.
    GET /{company_id: Company.id} {
        output: List<ChoreView>
        run: company_chores(company_id)
    }
}
```

Run `jadpo migrate new add_tenants`, then `jadpo records update`.

## Test the walls

Tests make requests as each person, which is what proves one company can't reach another.

`domains/work/routes/chores.test.jadpo`:

```jadpo check project=guide-tenancy file=domains/work/routes/chores.test.jadpo
use work/entities { Company, CompanyMembership, Member }
use work/types { ChoreCard, ChoreView, CompanyRole, ProjectCard }
use work/actions { AddChore, add_chore, add_member, found_company, register, start_project }

test "a chore can't name another company's project" {
    var ann: Member = attempt register("ann")
    var northwind: Company = found_company("Northwind", Principal.user(ann))
    var contoso: Company = found_company("Contoso", Principal.user(ann))
    var rival: ProjectCard = attempt start_project(contoso.id, "Rival")
    var response: Response = request {
        method: HttpMethod.post
        path: "/chores"
        as: Principal.user(ann)
        input: AddChore {
            company_id: northwind.id
            project_id: rival.id
            title: "Borrow"
        }
    }
    assert response.status == 403
    assert response.code == "not_permitted"
}

test "a viewer sees each chore with its project" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var northwind: Company = found_company("Northwind", Principal.user(ann))
    var viewer: CompanyMembership = attempt add_member(northwind.id, bob.id, CompanyRole.viewer)
    assert viewer.role == CompanyRole.viewer
    var launch: ProjectCard = attempt start_project(northwind.id, "Launch")
    var kickoff: ChoreCard = attempt add_chore(
        AddChore {
            company_id: northwind.id
            project_id: launch.id
            title: "Plan"
        }
    )
    var seen: Response = request {
        method: HttpMethod.get
        path: "/chores/{company_id}"
        params: {
            company_id: northwind.id
        }
        as: Principal.user(bob)
    }
    assert seen.status == 200
    assert seen.output == [
        ChoreView {
            id: kickoff.id
            title: "Plan"
            project: launch
        }
    ]
}

test "someone from another company sees none of its chores" {
    var ann: Member = attempt register("ann")
    var eve: Member = attempt register("eve")
    var northwind: Company = found_company("Northwind", Principal.user(ann))
    var launch: ProjectCard = attempt start_project(northwind.id, "Launch")
    var kickoff: ChoreCard = attempt add_chore(
        AddChore {
            company_id: northwind.id
            project_id: launch.id
            title: "Plan"
        }
    )
    assert kickoff.title == "Plan"
    var seen: Response = request {
        method: HttpMethod.get
        path: "/chores/{company_id}"
        params: {
            company_id: northwind.id
        }
        as: Principal.user(eve)
    }
    assert seen.status == 200
    assert seen.output == []
}
```

```sh
jadpo test
```

```text
tested: 3 passed, 3 route tests
```

These three tests show the three things to know:

- **A reference can't cross companies.** Every write that sets a reference checks it points at
  the same tenant, whatever the caller's grants say. Ann belongs to both companies and still gets
  `403 not_permitted`, because a chore of Northwind's can't name Contoso's project.
- **A member reads what their role lets them.** Bob is a viewer, so he sees each chore along with
  its project, which is always in the same company.
- **An outsider finds nothing.** Eve's request answers `200` with an empty list, not `403`, as if
  the chores weren't there, so she can't even tell that they exist. Policy filters in the query's own
  statement.

## See who can do what

`access.jadpo_record` (written by `jadpo records update`) lists who can do what. It is committed,
so a change that widens access shows as a diff, and needs a person's acceptance:

```text
tenant Company

entity work.Chore
    create CompanyRole.owner
    read CompanyRole.owner
    read CompanyRole.viewer
    tenant company_id
```

`jadpo check` also writes `build/report/permissions.md`, for reading the whole app at once.

## Work with no caller

Schedules and subscribers have no caller, so they run as the system, and a grant must name
`Access.system` to let them in:

- A schedule's run reaches every tenant's records, since no tenant's event starts it.
- A subscriber's delivery reaches the tenant named by its event's field of the tenant's `id`
  type, so an event about a company carries a `company_id`.
- A batch's run reaches the tenant its saved values name.

Work whose value names no tenant reaches only what every tenant shares.

## Read more

[Tenancy](../language/09-policy.md#tenancy),
[A membership role](../language/09-policy.md#a-membership-role),
[Grants](../language/09-policy.md#grants),
[The permissions report](../language/09-policy.md#the-permissions-report) and
[Background work and tenants](../language/12-jobs-and-events.md#background-work-and-tenants).
