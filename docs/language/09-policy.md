---
title: Policy
area: 9
status: decided
checked-with: jadpo chunk-5.1d (2026-10-16)
rules:
  - policy.*
  - type.field_update_forbidden
---

# Policy

Who may do what to which records: grants on entities and fields, tenancy,
actions that need their own permission, the generated permissions report, and
what a caller sees when policy says no.

> Decided on 2026-10-12. Code on this page applies the decisions from areas 1 to 9. Built in the new compiler in roadmap chunk 4.4: grants compiled into every read and write, membership roles, tenancy, actions listed under `actions:`, the permissions report, and the grants on revoking tokens and keys. Built in chunk 4.4b: a field's own `policy:` and `FieldAccess.server_only`, the proofs that every output and include is readable by everyone who may call it, writes refused for a field the caller may not change, and policy in the graph and manifest. Built in chunk 4.7: references kept inside the tenant, and conditions and orders proved like outputs. Built in chunk 4.8: founding a tenant, moving a record between tenants, listed lines no record lets in, and values followed through functions, the conditions that decide them, the records writes give back and the ids that find records. Built in chunk 5.1d: a value stored only in a field everyone who may read it may read. Built in chunk 6.1a: the system caller `Access.system` lets in, for schedules and subscribers. Built in chunk 6.1b: a subscriber reaching what only `Access.public` grants refused (`policy.system_grant_missing`). Built in chunk 6.2: a schedule reaching it refused the same way, and a schedule's run reaching every tenant. "Status" below lists the rest.

**Each entity lists, for each thing you can do to it, who may do it, and the
compiler applies that list to every read and write.**

```jadpo fragment
// domains/todos/entities/todo.jadpo
entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.restricted
    }
    title: TodoTitle

    policy: {
        create: [owner_id]
        read: [owner_id]
        update: [owner_id]
        delete: [owner_id]
    }
}
```

"Who" is always one of four things:

| Who | Written | Means |
|---|---|---|
| A user field on the record | `owner_id` | the user that field points to |
| A membership role | `CompanyRole.editor` | a user with that role in the record's company |
| Everyone | `Access.authenticated`, `Access.public` | anyone signed in; anyone at all |
| Background work | `Access.system` | schedules and subscribers, limited to the tenant their event came from (added from area 12, 2026-10-13) |

## Grants

An entity's `policy:` has up to four lists: `create`, `read`, `update` and
`delete`. Each names who may do that; anyone in the list is enough, and
anything not listed is refused. So "who can delete a project?" is one line.

A lifecycle transition is an update, with one exception from area 6:
`Todo.delete(todo)` needs `delete` whether it removes the record or runs the
lifecycle's `delete` transition, so a soft delete is still a delete.

Nobody writes which grant an action needs. The compiler sees what each query
and action reads and writes: every action that updates a `Todo` needs
`update` on it, so ten actions that edit todos share one line of policy. An
action that reaches several entities needs a grant on each, and a call to
another domain's action is checked against that action's entities with the
same caller.

Policy is always on (question 2): from the first line of code, every
reachable create, read, update or delete needs a grant.

## A user field on the record

Naming a reference to the user entity, such as `owner_id`, grants the user it
points to. Creating checks the new record, so `create: [owner_id]` means a
caller can only create a todo they own; a request naming someone else is
refused.

A user field named anywhere in a policy cannot be changed by an ordinary
update, so it needs no `immutable:`. Handing a record to someone else is a
named action listed in root `policy.jadpo` (see Actions).

## A membership role

A role exists only where it is stored: in a membership entity, whose
`membership:` section says which record the role is held in (`scope`), who
holds it (`member`) and which field holds the role. The role enum lives
beside it in the same domain, and other domains use it by its full name. There
is no global roles file.

```jadpo fragment
// domains/accounts/types/company_role.jadpo
type CompanyRole = Enum {
    owner
    editor
    viewer
}

// domains/accounts/entities/company_membership.jadpo
entity CompanyMembership {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: CompanyRole

    membership: {
        scope: company_id
        member: user_id
        role: role
    }

    policy: {
        create: [CompanyRole.owner]
        read: [CompanyRole.owner, user_id]
        update: [CompanyRole.owner]
        delete: [CompanyRole.owner, user_id]
    }
}
```

A role applies to another entity through its reference to the scope, the
same reference tenancy uses:

```jadpo fragment
// domains/projects/entities/project.jadpo
entity Project {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    name: ProjectName

    policy: {
        create: [CompanyRole.editor]
        read: [CompanyRole.viewer, CompanyRole.editor]
        update: [CompanyRole.editor]
        delete: [CompanyRole.owner]
    }
}
```

- There is no hierarchy: an owner who may also edit is listed under both.
- Only current memberships count. One the lifecycle hides grants nothing, and
  removing one takes effect on the next request.
- A role held across the whole application, such as staff, uses
  `scope: MembershipScope.application`.
- If an entity has two references to the scope, each grant names the one it
  means: `TeamRole.lead(from_team_id)` (question 5).

## Anyone signed in, or anyone

`Access.authenticated` is anyone signed in, and `Access.public` is anyone at
all. A public read also needs a route that says
`authentication: RouteAuthentication.none` (area 8); either alone is not
enough. There is no administrator who bypasses policy.

```jadpo fragment
entity Article {
    ...
    policy: {
        read: [Access.public]
        create: [CompanyRole.editor]
        update: [CompanyRole.editor]
        delete: [CompanyRole.owner]
    }
}
```

## Background work

Schedules and subscribers act as the system caller (area 12, question 8),
which gets exactly what `Access.system` grants. The system is nobody
signed in and holds no membership, so a user field, a role and
`Access.authenticated` never let it in; a grant list, a field's own list
or an `actions:` line lets it in only by naming `Access.system`. An
entity grants background work explicitly: a schedule, a subscriber or a
batch that reaches a statement only `Access.public` grants is a compile
error, as `policy.effect_ungranted` refuses a route, so the runtime, where
such a statement tests no caller and runs for the system as for everyone,
never has to tell the two apart.

On a tenant's records, a grant naming `Access.system` lets a subscriber in
within the tenant its event came from, and a schedule's run, which no
tenant's event starts, in every tenant. The system never moves a record to
another tenant, which needs a membership there, and founds no tenant's
first membership. A listed action's line naming `Access.system` lets the
system in beside whoever else it names; a line naming only `Access.system`
still lets in no request. No request is ever the system: only the
runtime's runner makes the system caller, and every request, and every
call a test makes with a caller or without one, runs as someone else.

## Tenancy

Most business applications are shared by many customer companies, and the
worst bug they can have is one company seeing another's data. Root
`policy.jadpo` names the tenant once (question 3):

```jadpo fragment
// policy.jadpo
policy {
    tenant: Company
}
```

Every entity must then have one reference to `Company` (a task carries its
own `company_id`, not only its project's), and every grant on it,
`Access.authenticated` included, applies only within companies where the
caller has a membership. An entity shared by every tenant, such as a list of
countries or `User` itself, says `tenant: Tenancy.shared` in its `policy:`.
A new entity is tenant-scoped unless someone accepts otherwise.

A company's first membership founds it: a new membership makes its member
one of the company's people only while nobody belongs to the company yet.
Every other membership is created by someone who belongs already, as the
membership's own `create:` says, so nobody makes themselves a member, or
an owner, of someone else's company (revised in chunk 4.8).

### References stay inside the tenant

A reference from one of a tenant's records to another always points at a
record of the same tenant: a chore of Northwind's names one of Northwind's
projects, never Contoso's. Every create and update that sets such a
reference checks it in its own statement, with or without policy, and one
that points into another tenant is refused with 403 `not_permitted`; a
record that isn't there is still the code's own not-found failure
(Richard, 2026-10-16; built in chunk 4.7). So whoever may read a chore
holds their role in the project's company too, and a query of chores may
include each chore's project:

```jadpo check project=tenant-references file=app.jadpo hidden
use work/entities { Member }

application {}

principal {
    user: {
        subject: Member.handle
    }
}
```

```jadpo check project=tenant-references file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "work_session"
    }
}
```

```jadpo check project=tenant-references file=policy.jadpo hidden
use work/entities { Company }
use work

policy {
    tenant: Company
    actions: {
        work.found_company: [Access.authenticated]
    }
}
```

```jadpo check project=tenant-references file=domains/work/types/handle.jadpo hidden
type Handle = Text {
    min_length: 3
    max_length: 40
}
```

```jadpo check project=tenant-references file=domains/work/types/label.jadpo hidden
type Label = Text {
    min_length: 1
    max_length: 80
}
```

```jadpo check project=tenant-references file=domains/work/types/company_role.jadpo hidden
/// What a member may do in a company.
type CompanyRole = Enum {
    owner
    viewer
}
```

```jadpo check project=tenant-references file=domains/work/types/project_card.jadpo hidden
use work/entities { Project }

/// A project as members see it.
type ProjectCard = Output {
    pick: [Project.id, Project.name]
}
```

```jadpo check project=tenant-references file=domains/work/types/chore_card.jadpo hidden
use work/entities { Chore }

/// A chore on its own.
type ChoreCard = Output {
    pick: [Chore.id, Chore.title]
}
```

```jadpo check project=tenant-references file=domains/work/types/chore_view.jadpo hidden
use work/entities { Chore }
use work/types { ProjectCard }

/// A chore with the project it belongs to.
type ChoreView = Output {
    pick: [Chore.id, Chore.title]
    project: ProjectCard
}
```

```jadpo check project=tenant-references file=domains/work/failures/handle_taken.jadpo hidden
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
```

```jadpo check project=tenant-references file=domains/work/entities/member.jadpo hidden
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

```jadpo check project=tenant-references file=domains/work/entities/company.jadpo hidden
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

```jadpo check project=tenant-references file=domains/work/entities/company_membership.jadpo hidden
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

```jadpo check project=tenant-references file=domains/work/entities/project.jadpo hidden
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

```jadpo check project=tenant-references file=domains/work/actions/register.jadpo hidden
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

```jadpo check project=tenant-references file=domains/work/actions/found_company.jadpo hidden
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

```jadpo check project=tenant-references file=domains/work/actions/add_member.jadpo hidden
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

```jadpo check project=tenant-references file=domains/work/actions/start_project.jadpo hidden
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

```jadpo check project=tenant-references file=domains/work/actions/add_chore.jadpo hidden
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

```jadpo check project=tenant-references file=domains/work/entities/chore.jadpo
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

```jadpo check project=tenant-references file=domains/work/queries/company_chores.jadpo
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

```jadpo check project=tenant-references file=domains/work/routes/chores.jadpo migrate=initial hidden
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

```jadpo check project=tenant-references file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication browser_session

POST /chores
    input AddChore (body)
    output ChoreCard
    status 201
    fails company_not_found
    fails project_not_found

GET /chores/{company_id: Uuid}
    output List<ChoreView>
    status 200

input AddChore
    company_id: Uuid
    project_id: Uuid
    title: Text { min_length: 1, max_length: 80 }

output ChoreCard
    id: Uuid
    title: Text { min_length: 1, max_length: 80 }

output ChoreView
    id: Uuid
    project: ProjectCard
    title: Text { min_length: 1, max_length: 80 }

output ProjectCard
    id: Uuid
    name: Text { min_length: 1, max_length: 80 }

failure company_not_found
    status 404

failure project_not_found
    status 404
```

Ann owns both companies, so only the reference refuses her; Bob, a viewer
of Northwind, sees its chores with their projects:

```jadpo check project=tenant-references file=domains/work/routes/chores.test.jadpo
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
```

A reference to the tenant itself is the record's tenancy, or, beside the
one an entity names with `tenant:`, another tenant on purpose, so it isn't
checked. A write that moves a record to another tenant, which only an
action listed under `actions:` may, checks every reference it keeps, and
that no record of the old tenant refers to it, and needs the caller's own
membership in the new tenant, as a create there would (chunk 4.8).

## Actions

An action takes its permission from the entities it touches. Root
`policy.jadpo` lists the few that cannot, under `actions:` (question 4):

- an action that touches no entity, such as an operator's health report,
  which is otherwise refused;
- an action that needs different people from its entity's grants, such as
  handing a todo to someone else, or creating a company together with its
  first owner's membership.

```jadpo fragment
// policy.jadpo
policy {
    tenant: Company
    actions: {
        todos.transfer_todo: [Todo.owner_id]
        accounts.create_company: [Access.authenticated]
        monitoring.health_report: [StaffRole.operator]
    }
}
```

`Todo.owner_id` means the current owner of the todo the action changes. For
that action, the listed people replace the entity grants on what it reaches; field restrictions and tenancy still apply. Calling an integration
needs nothing extra: permission to run the action covers it (question 9).

A user field such as `Todo.owner_id`, or a role held in a record such as
`CompanyRole.owner`, is checked on the records the action itself creates,
changes or deletes. On an action that changes none of them, such as a
report, it lets nobody in: the action is checked as it starts against the
line's other entries, anyone signed in or a role held across the whole
application, and with none left it refuses every caller, so its route is
`policy.effect_ungranted` (chunk 4.8).

## Field restrictions

A field follows its entity's grants unless its block narrows them, in the
same shape. A field can only take away: it cannot grant anyone more than the
entity does (`policy.field_widens_entity`). A field no client may read or
write says so by name.

```jadpo fragment
entity Project {
    ...
    budget: ProjectBudget {
        policy: {
            read: [CompanyRole.editor]
            update: [CompanyRole.editor]
        }
    }
    import_reference: Text {
        policy: FieldAccess.server_only
    }
}
```

- **Writes.** If a request changes a field the caller may not change, the
  whole write is refused; nothing is dropped silently. A caller who may
  change the record but not the field gets 403 `not_permitted`; a create
  that leaves an optional field `none` sets nothing, so the field's policy
  never refuses it.
- **Outputs.** An output type has one shape for everyone. If anyone allowed
  to call a query cannot read one of its output fields, the query does not
  compile (`policy.output_field_unproved`); the fix is a second query with a
  narrower output. The same holds for the record a query includes through
  a reference: everyone who may read the record must be able to read what
  it refers to.
- **Conditions and orders.** Which records pass a condition, and the order
  they come in, show the fields they test, so a query, `get`, include or
  write may filter or order on a field only when everyone who may run it
  may read that field; otherwise `policy.output_field_unproved` names the
  condition or order (Richard, 2026-10-16; built in chunk 4.7). A
  transition's `when:` is a write's condition: refusing a record shows what
  it tests.
- **Copies.** Passing a value through a variable, another type, a
  function or the record a write stores it in does not escape the field's
  rules; the compiler tracks where it came from (chunk 4.8 adds functions
  and writes).
- **What decides a value.** A value made under an `if`, a `match` or a
  loop, or kept by a filter, shows what they test, so it carries their
  fields' rules too, as does a count of what a filter kept, a failure
  raised on such a condition, and what comes after a `return` or `reject`
  it may have taken (chunk 4.8).
- **Finding records.** Which record a `get`, a query or a write finds shows
  the value it found it by, and an include shows the reference it follows,
  so each is proved like an output (chunk 4.8).
- **What a write gives back.** The record an update returns is proved
  against its `read:` as well as its fields', since a caller may change a
  record they may not read; what its `set:` gave a field, and a created
  record's values, are the caller's own (chunk 4.8).
- **What a write stores.** A value stored in a field is read later, from
  any request, by everyone who may read that field, so a create or update
  stores a value in a field only when every one of them may read it,
  proved as an output is; otherwise `policy.output_field_unproved` names
  the value in the write (Richard, 2026-10-16; built in chunk 5.1d). A
  copy between fields the same callers read stays allowed: within a
  record, into a record that refers to the one it came from, or into one
  of the same tenant. Who reads a field is known only of the record it is
  stored in, so a copy into any other record proves only what holds of
  anyone signed in. A patch, `when_supplied:`, a counter and a transition
  store values too, and so does an action a value is passed to; what
  decided a value is stored with it, and since an action is
  all-or-nothing, so is whatever a `reject` in it tests.
- **Server only.** `FieldAccess.server_only` lets no request read or write
  the field. A test's or a seed's call without a caller runs without policy,
  so it may; how schedules and subscribers write one comes with area 12.

A field's `policy:` narrows `create`, `read` and `update`; it is deleted
with its record. Who may read a value is proved from the grants alone, not
from the data, because two records that each refer to a company may refer
to different companies: a role held in one says nothing about the other.

## The permissions report

Policy is written beside each entity, so nobody has to keep a central copy.
`jadpo check` writes the global view, `build/report/permissions.md` with a
`.json` twin: who can do what to every entity and field, and which actions
are listed separately. Each list is shown as written, and an effect nobody
is granted says so.

Part of the report for the end-to-end example below:

| `work.Project` | create | read | update | delete |
|---|---|---|---|---|
| the record | `CompanyRole.owner`, `CompanyRole.editor` | `CompanyRole.owner`, `CompanyRole.editor`, `CompanyRole.viewer` | `CompanyRole.owner`, `CompanyRole.editor` | `CompanyRole.owner` |

Belongs to a `Company` through `company_id`. Only an action listed under `actions:` changes `company_id`.

A field that narrows its entity's grants gets a row of its own beside "the
record". For the `Project` under "Field restrictions":

| `work.Project` | create | read | update | delete |
|---|---|---|---|---|
| the record | `CompanyRole.editor` | `CompanyRole.viewer`, `CompanyRole.editor` | `CompanyRole.editor` | `CompanyRole.owner` |
| `budget` | as the record | `CompanyRole.editor` | `CompanyRole.editor` | as the record |
| `import_reference` | server only | server only | server only | as the record |

Changes to this matrix appear in the approval review in plain terms
("viewers can now read `budget`").

## Protecting policy

Decided in area 1: an agent can edit policy, but the change cannot ship
without a person's approval, wherever it is written. The compiler compares
the whole application before and after, because access also widens when an
output gains a field, a route becomes public or an action starts touching
another entity. Each widening needs an acceptance file in `acceptances/`, the
form area 7 decided for breaking API changes and database drops.

Who schedules and subscribers act as is area 12's question; whatever is
decided shows in the permissions report. The compiler's own purge of
soft-deleted records (area 5) is the only operation outside policy.

## When policy says no

The compiler adds policy to the SQL `WHERE` of every query and write (area
6), and indexes every field a policy condition filters on.

| Situation | The client sees |
|---|---|
| Not signed in, on a route that needs it | 401 `unauthenticated` (area 8) |
| A record the caller may not see, read or changed by id | the code's own `not_found` failure, as if it did not exist |
| A list | only the records the caller may see, filtered before paging |
| A create, or an action they may not call | 403 `not_permitted` |

A hidden record reports the same `not_found` failure the code already maps,
and the log records that policy concealed it (area 4). `not_permitted` is
raised by the runtime and never listed in `fails` (question 7); the generated
API description lists the 403 wherever it can happen. Writes are checked in
the same statement as the change, so a membership removed a moment earlier
cannot slip through.

A token issued to another application (area 8) carries scopes, and scopes
only narrow: the token may do what both the person's grants and its scopes
allow (question 6).

### Policy, end to end

A whole application whose customers are companies: members sign up, found
a company, and give each other roles in it, and a company's projects are
seen and changed only by its members. The principal, its one sign-in
method and root `policy.jadpo`, which names the tenant and the one action
that needs other people than its entities' grants: founding a company,
since a company with no members yet has no owner to grant anything:

```jadpo check project=policy-end-to-end file=app.jadpo
use work/entities { Member }

application {}

principal {
    user: {
        subject: Member.handle
    }
}
```

```jadpo check project=policy-end-to-end file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "projects_session"
    }
}
```

```jadpo check project=policy-end-to-end file=policy.jadpo
use work/entities { Company }
use work

policy {
    tenant: Company
    actions: {
        work.found_company: [Access.authenticated]
    }
}
```

```jadpo check project=policy-end-to-end file=domains/work/types/handle.jadpo hidden
type Handle = Text {
    min_length: 3
    max_length: 40
}
```

```jadpo check project=policy-end-to-end file=domains/work/types/company_name.jadpo hidden
type CompanyName = Text {
    min_length: 1
    max_length: 80
}
```

```jadpo check project=policy-end-to-end file=domains/work/types/company_role.jadpo hidden
/// What a member may do in a company.
type CompanyRole = Enum {
    owner
    editor
    viewer
}
```

```jadpo check project=policy-end-to-end file=domains/work/types/project_name.jadpo hidden
type ProjectName = Text {
    min_length: 1
    max_length: 80
}
```

```jadpo check project=policy-end-to-end file=domains/work/types/project_view.jadpo hidden
use work/entities { Project }

/// A project as members see it.
type ProjectView = Output {
    pick: [Project.id, Project.name]
}
```

```jadpo check project=policy-end-to-end file=domains/work/failures/handle_taken.jadpo hidden
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
```

A member may read only their own record, and anyone may sign up; members
are shared by every company. The membership says where its role is held,
who holds it and which field holds it, and a company's records follow the
roles held in it:

```jadpo check project=policy-end-to-end file=domains/work/entities/member.jadpo
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

```jadpo check project=policy-end-to-end file=domains/work/entities/company.jadpo
use work/types { CompanyName, CompanyRole }

/// A customer: the tenant every other record belongs to.
entity Company {
    id: Uuid
    name: CompanyName

    policy: {
        read: [CompanyRole.owner, CompanyRole.editor, CompanyRole.viewer]
    }
}
```

```jadpo check project=policy-end-to-end file=domains/work/entities/company_membership.jadpo
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
        delete: [CompanyRole.owner, member_id]
    }
}
```

```jadpo check project=policy-end-to-end file=domains/work/entities/project.jadpo
use work/entities { Company }
use work/types { CompanyRole, ProjectName }

/// A piece of work in a company.
entity Project {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    name: ProjectName

    policy: {
        create: [CompanyRole.owner, CompanyRole.editor]
        read: [CompanyRole.owner, CompanyRole.editor, CompanyRole.viewer]
        update: [CompanyRole.owner, CompanyRole.editor]
        delete: [CompanyRole.owner]
    }
}
```

Founding a company is the listed action: anyone signed in may call it,
and it makes its founder the owner. Adding a member is an ordinary create
of a membership, so only the company's owners may; the code says nothing
about who may do either:

```jadpo check project=policy-end-to-end file=domains/work/actions/register.jadpo hidden
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

```jadpo check project=policy-end-to-end file=domains/work/actions/found_company.jadpo
use work/entities { Company, CompanyMembership }
use work/types { CompanyName, CompanyRole }

/// Starts a company with its founder as its owner.
action found_company(name: CompanyName, founder: Principal.user) -> Company {
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

```jadpo check project=policy-end-to-end file=domains/work/actions/add_member.jadpo
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

```jadpo check project=policy-end-to-end file=domains/work/actions/start_project.jadpo hidden
use work/entities { Company, CompanyNotFound, Project }
use work/types { ProjectName, ProjectView }

/// What starting a project takes.
type StartProject = Input {
    company_id: Company.id
    name: ProjectName
}

/// Starts a project in a company.
action start_project(input: StartProject) fails CompanyNotFound -> ProjectView {
    var project: Project = attempt Project.create {
        set: {
            company_id: input.company_id
            name: input.name
        }
    }
    return ProjectView {
        id: project.id
        name: project.name
    }
}
```

```jadpo check project=policy-end-to-end file=domains/work/actions/rename_project.jadpo hidden
use work/entities { Project, ProjectNotFound }
use work/types { ProjectName, ProjectView }

/// What renaming a project takes.
type RenameProject = Input {
    name: ProjectName
}

/// Renames a project.
action rename_project(
    project_id: Project.id,
    input: RenameProject
) fails ProjectNotFound -> ProjectView {
    var project: Project = attempt Project.update(project_id) {
        set: {
            name: input.name
        }
    }
    return ProjectView {
        id: project.id
        name: project.name
    }
}
```

The query names no caller and no company check: policy adds both to its
`WHERE`, so a caller sees only the projects of companies where they hold a
role that may read them:

```jadpo check project=policy-end-to-end file=domains/work/queries/company_projects.jadpo
use work/entities { Company, Project }
use work/types { ProjectView }

/// A company's projects, by name.
query company_projects(company_id: Company.id) -> List<ProjectView> {
    from: Project
    where: Project.company_id == company_id
    order: [Project.name asc]
    limit: 100
}
```

```jadpo check project=policy-end-to-end file=domains/work/routes/projects.jadpo migrate=initial
use work/entities { Company, Project }
use work/types { ProjectView }
use work/actions { RenameProject, StartProject, rename_project, start_project }
use work/queries { company_projects }

/// A company's projects.
routes Projects {
    prefix: /projects

    /// Starts a project.
    POST / {
        input: StartProject
        output: ProjectView
        status: SuccessStatus.created
        run: start_project(input)
    }

    /// A company's projects the caller may see.
    GET /{company_id: Company.id} {
        output: List<ProjectView>
        run: company_projects(company_id)
    }

    /// Renames a project.
    PATCH /{project_id: Project.id} {
        input: RenameProject
        output: ProjectView
        run: rename_project(project_id, input)
    }
}
```

```jadpo check project=policy-end-to-end file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication browser_session

POST /projects
    input StartProject (body)
    output ProjectView
    status 201
    fails company_not_found

GET /projects/{company_id: Uuid}
    output List<ProjectView>
    status 200

PATCH /projects/{project_id: Uuid}
    input RenameProject (body)
    output ProjectView
    status 200
    fails project_not_found

output ProjectView
    id: Uuid
    name: Text { min_length: 1, max_length: 80 }

input RenameProject
    name: Text { min_length: 1, max_length: 80 }

input StartProject
    company_id: Uuid
    name: Text { min_length: 1, max_length: 80 }

failure company_not_found
    status 404

failure project_not_found
    status 404
```

A route test signs in as each member, so what it checks is exactly what a
browser would see. The test sets up its data by calling actions without a
caller, which runs them without policy; `found_company` takes its founder,
so it runs as the founder and its grants apply:

```jadpo check project=policy-end-to-end file=domains/work/routes/projects.test.jadpo
use work/entities { Company, CompanyMembership, Member }
use work/types { CompanyRole, ProjectView }
use work/actions { RenameProject, StartProject, add_member, found_company, register, start_project }

test "a company's projects are listed only to its members" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var cy: Member = attempt register("cyd")
    var northwind: Company = found_company("Northwind", Principal.user(ann))
    var viewer: CompanyMembership = attempt add_member(northwind.id, bob.id, CompanyRole.viewer)
    assert viewer.role == CompanyRole.viewer
    var launch: ProjectView = attempt start_project(
        StartProject {
            company_id: northwind.id
            name: "Launch"
        }
    )
    var seen: Response = request {
        method: HttpMethod.get
        path: "/projects/{company_id}"
        params: {
            company_id: northwind.id
        }
        as: Principal.user(bob)
    }
    assert seen.status == 200
    assert seen.output == [launch]
    var outsider: Response = request {
        method: HttpMethod.get
        path: "/projects/{company_id}"
        params: {
            company_id: northwind.id
        }
        as: Principal.user(cy)
    }
    assert outsider.output == []
}

test "a viewer can't rename a project, which reads as missing" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var northwind: Company = found_company("Northwind", Principal.user(ann))
    var viewer: CompanyMembership = attempt add_member(northwind.id, bob.id, CompanyRole.viewer)
    assert viewer.role == CompanyRole.viewer
    var launch: ProjectView = attempt start_project(
        StartProject {
            company_id: northwind.id
            name: "Launch"
        }
    )
    var by_bob: Response = request {
        method: HttpMethod.patch
        path: "/projects/{project_id}"
        params: {
            project_id: launch.id
        }
        as: Principal.user(bob)
        input: RenameProject {
            name: "Plans"
        }
    }
    assert by_bob.status == 404
    assert by_bob.code == "project_not_found"
    var by_ann: Response = request {
        method: HttpMethod.patch
        path: "/projects/{project_id}"
        params: {
            project_id: launch.id
        }
        as: Principal.user(ann)
        input: RenameProject {
            name: "Plans"
        }
    }
    assert by_ann.status == 200
}

test "someone outside a company can't start a project in it" {
    var ann: Member = attempt register("ann")
    var cyd: Member = attempt register("cyd")
    var northwind: Company = found_company("Northwind", Principal.user(ann))
    var response: Response = request {
        method: HttpMethod.post
        path: "/projects"
        as: Principal.user(cyd)
        input: StartProject {
            company_id: northwind.id
            name: "Takeover"
        }
    }
    assert response.status == 403
    assert response.code == "not_permitted"
}
```

## Caches and personal data

Decided in area 5: a record cache applies policy on every read, a
query-result cache is keyed by the tenant and every input policy used, and
policy itself always reads the database.

Area 15 owns personal data. Policy supplies its permissions: erasing a
person's data needs `delete` on their record and exporting it needs `read`.

## Errors

Every error this area's checks produce, with an example that triggers it.
The examples are one file each, so the principal and root `policy { }` sit
beside the entities they name; a project keeps them in `app.jadpo` and
`policy.jadpo`.

### Owner fields and memberships

### `policy.role_unbound`

A role exists only where a membership stores it; declaring the enum gives it to nobody:

```jadpo check project=e-role-unbound expect=policy.role_unbound
type NoteRole = Enum {
    owner
}

entity Note {
    id: Uuid

    policy: {
        read: [NoteRole.owner]
    }
}
```

```text output
error: No membership holds a `NoteRole`
 --> main.jadpo:9:16
  |
9 |         read: [NoteRole.owner]
  |                ^^^^^^^^^^^^^^ a role nobody can hold
  reason: A role exists only where it is stored: in a membership entity whose `membership:` names a field typed `NoteRole` as its `role:`. Declaring the enum alone gives the role to nobody.
  next: Add a membership entity holding `NoteRole`, or name someone else in the grant (agent)
  rule: policy.role_unbound
  help: docs/language/09-policy.html#policy-role-unbound

check failed: 1 error, 1 file
```

### `policy.binding_invalid`

A field names who may do something only when it points at the caller, a reference to the principal's user entity; the same rule covers a membership's `scope:`, `member:` and `role:`, `tenant:`, and the lines under `actions:`:

```jadpo check project=e-binding-invalid expect=policy.binding_invalid
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

entity Note {
    id: Uuid
    owner_id: Uuid

    policy: {
        read: [owner_id]
    }
}
```

```text output
error: `owner_id` doesn't point at a caller
 --> main.jadpo:23:16
   |
23 |         read: [owner_id]
   |                ^^^^^^^^ not a user field
  reason: A field names who may do this when it points at the caller: a reference to `User`, the principal's user entity (area 8), typed `User.id`. `Note.owner_id` isn't.
  next: Name a reference to the user, a membership role, or `Access` (agent)
  rule: policy.binding_invalid
  help: docs/language/09-policy.html#policy-binding-invalid

check failed: 1 error, 1 file
```

### `policy.membership_scope_required`

A membership says where its role is held: a reference, or `MembershipScope.application`:

```jadpo check project=e-membership-scope-required expect=policy.membership_scope_required
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type StaffRole = Enum {
    steward
}

entity StaffMembership {
    id: Uuid
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        member: user_id
        role: role
    }
}
```

```text output
error: `StaffMembership`'s membership has no `scope:`
 --> main.jadpo:29:5
   |
29 |     membership: {
   |     ^^^^^^^^^^ no `scope:`
  reason: A membership says where a role is held (`scope:`), who holds it (`member:`) and which field holds it (`role:`); `scope:` is what the role is held in: a field typed `<Entity>.id`, or `MembershipScope.application` for a role held across the whole application.
  next: Add `scope:` to the membership (agent)
  rule: policy.membership_scope_required
  help: docs/language/09-policy.html#policy-membership-scope-required

check failed: 1 error, 1 file
```

### `policy.membership_member_required`

```jadpo check project=e-membership-member-required expect=policy.membership_member_required
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type StaffRole = Enum {
    steward
}

entity StaffMembership {
    id: Uuid
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        role: role
    }
}
```

```text output
error: `StaffMembership`'s membership has no `member:`
 --> main.jadpo:29:5
   |
29 |     membership: {
   |     ^^^^^^^^^^ no `member:`
  reason: A membership says where a role is held (`scope:`), who holds it (`member:`) and which field holds it (`role:`); `member:` is who holds it: a reference to the user entity.
  next: Add `member:` to the membership (agent)
  rule: policy.membership_member_required
  help: docs/language/09-policy.html#policy-membership-member-required

check failed: 1 error, 1 file
```

### `policy.membership_role_required`

```jadpo check project=e-membership-role-required expect=policy.membership_role_required
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type StaffRole = Enum {
    steward
}

entity StaffMembership {
    id: Uuid
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        member: user_id
    }
}
```

```text output
error: `StaffMembership`'s membership has no `role:`
 --> main.jadpo:29:5
   |
29 |     membership: {
   |     ^^^^^^^^^^ no `role:`
  reason: A membership says where a role is held (`scope:`), who holds it (`member:`) and which field holds it (`role:`); `role:` is which field holds the role, a field typed with the role's enum.
  next: Add `role:` to the membership (agent)
  rule: policy.membership_role_required
  help: docs/language/09-policy.html#policy-membership-role-required

check failed: 1 error, 1 file
```

### `policy.membership_duplicate`

An entity stores one role, held by one member in one place:

```jadpo check project=e-membership-duplicate expect=policy.membership_duplicate
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type StaffRole = Enum {
    steward
}

entity StaffMembership {
    id: Uuid
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        member: user_id
        role: role
    }

    membership: {
        scope: MembershipScope.application
        member: user_id
        role: role
    }
}
```

```text output
error: `StaffMembership` declares `membership:` twice
 --> main.jadpo:35:5
   |
29 |     membership: {
   |     ---------- first declared here
35 |     membership: {
   |     ^^^^^^^^^^ declared again
  reason: A membership entity stores one role, held in one place by one member, so it says so once.
  next: Remove one `membership:`, or keep the second role in an entity of its own (agent)
  rule: policy.membership_duplicate
  help: docs/language/09-policy.html#policy-membership-duplicate

check failed: 1 error, 1 file
```

### Scope

### `policy.scope_missing`

`Ledger` names a company role but has no reference to `Company`, which the role is held in; an entity with no reference to the tenant is refused the same way:

```jadpo check project=e-scope-missing expect=policy.scope_missing
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type CompanyRole = Enum {
    owner
    editor
}

entity Company {
    id: Uuid
}

entity CompanyMembership {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: CompanyRole

    membership: {
        scope: company_id
        member: user_id
        role: role
    }
}

entity Ledger {
    id: Uuid

    policy: {
        read: [CompanyRole.editor]
    }
}
```

```text output
error: `Ledger` has no reference to `Company`
 --> main.jadpo:48:16
   |
48 |         read: [CompanyRole.editor]
   |                ^^^^^^^^^^^^^^^^^^ a role held in a `Company`
  reason: `CompanyRole` is held in a `Company` (`CompanyMembership`'s `scope:`), and a role applies to a record through the record's reference to its scope, so `Ledger` needs a field typed `Company.id`.
  next: Add a reference to `Company` to `Ledger`, or name someone else (agent)
  rule: policy.scope_missing
  help: docs/language/09-policy.html#policy-scope-missing

check failed: 1 error, 1 file
```

### `policy.scope_ambiguous`

Two references to `Company`, and the grant does not say which one the role applies through; `CompanyRole.editor(source_company_id)` would:

```jadpo check project=e-scope-ambiguous expect=policy.scope_ambiguous
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type CompanyRole = Enum {
    owner
    editor
}

entity Company {
    id: Uuid
}

entity CompanyMembership {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: CompanyRole

    membership: {
        scope: company_id
        member: user_id
        role: role
    }
}

entity InvoiceTransfer {
    id: Uuid
    source_company_id: Company.id {
        on_delete: OnDelete.restricted
    }
    destination_company_id: Company.id {
        on_delete: OnDelete.restricted
    }

    policy: {
        read: [CompanyRole.editor]
    }
}
```

```text output
error: `InvoiceTransfer` refers to `Company` more than once
 --> main.jadpo:54:16
   |
54 |         read: [CompanyRole.editor]
   |                ^^^^^^^^^^^^^^^^^^ through which reference?
  reason: `CompanyRole` is held in a `Company`, and `InvoiceTransfer` has 2 references to `Company` (`source_company_id` and `destination_company_id`), so the grant names the one it means.
  next: Write `CompanyRole.editor(source_company_id)` (agent)
  rule: policy.scope_ambiguous
  help: docs/language/09-policy.html#policy-scope-ambiguous

check failed: 1 error, 1 file
```

### `policy.scope_duplicate`

```jadpo check project=e-scope-duplicate expect=policy.scope_duplicate
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type StaffRole = Enum {
    steward
}

entity StaffMembership {
    id: Uuid
    user_id: User.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        scope: MembershipScope.application
        member: user_id
        role: role
    }
}
```

```text output
error: `scope:` is given twice
 --> main.jadpo:31:9
   |
30 |         scope: MembershipScope.application
   |         ----- first given here
31 |         scope: MembershipScope.application
   |         ^^^^^ given again
  reason: A membership says once where its role is held, who holds it and which field holds it, so there is never a question of which counts.
  next: Remove one `scope:` (agent)
  rule: policy.scope_duplicate
  help: docs/language/09-policy.html#policy-scope-duplicate

check failed: 1 error, 1 file
```

### Grants

### `policy.effect_ungranted`

A route that runs an action reaching no entity, which `actions:` in root `policy { }` does not list; a route that reaches a create, read, update or delete no grant names is refused the same way:

```jadpo check project=e-effect-ungranted expect=policy.effect_ungranted
type Health = Output {
    ok: Bool
}

action check_health() -> Health {
    return Health {
        ok: true
    }
}

routes Checkup {
    prefix: /health

    POST / {
        authentication: RouteAuthentication.none
        output: Health
        run: check_health()
    }
}
```

```text output
error: `POST /health` runs `check_health`, which nobody may call
 --> main.jadpo:14:5
   |
14 |     POST / {
   |     ^^^^^^ reaches no entity
  reason: An action takes its permission from the entities it reaches, and `check_health` reaches none, so it is refused to everyone unless root `policy { }` lists who may call it under `actions:`.
  next: List `check_health: [...]` under `actions:` in `policy.jadpo` (human)
  rule: policy.effect_ungranted
  help: docs/language/09-policy.html#policy-effect-ungranted

check failed: 1 error, 1 file
```

A line under `actions:` naming only a user field or a role held in a record lets nobody call an action that changes no such record, such as a summary of the caller's notes, so its route could never succeed:

```jadpo check project=e-listed-line-unheld expect=policy.effect_ungranted
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

authentication {
    api_token: {}
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        read: [owner_id]
    }
}

type Tidings = Output {
    ok: Bool
}

action note_tidings() -> Tidings {
    return Tidings {
        ok: true
    }
}

routes Bulletin {
    prefix: /tidings

    POST / {
        output: Tidings
        run: note_tidings()
    }
}

policy {
    actions: {
        note_tidings: [Note.owner_id]
    }
}
```

```text output
error: `POST /tidings` runs `note_tidings`, which nobody may call
 --> main.jadpo:46:5
   |
46 |     POST / {
   |     ^^^^^^ nobody passes its line
  reason: `note_tidings`'s line under `actions:` names `Note.owner_id`, the caller a `Note` the action changes points at, and `note_tidings` changes no such record itself, so no caller passes the line as the action starts and this route could never succeed.
  next: List `Access.authenticated` or a role held across the whole application for `note_tidings` under `actions:` (human)
  rule: policy.effect_ungranted
  help: docs/language/09-policy.html#policy-effect-ungranted

check failed: 1 error, 1 file
```

### `policy.effect_unknown`

A policy grants `create`, `read`, `update` and `delete`; an action needing other people is listed under `actions:`:

```jadpo check project=e-effect-unknown expect=policy.effect_unknown
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        publish: [owner_id]
    }
}
```

```text output
error: `publish` isn't something a policy grants
 --> main.jadpo:25:9
   |
25 |         publish: [owner_id]
   |         ^^^^^^^ not a grant or a setting
  reason: An entity's `policy:` lists who may `create`, `read`, `update` and `delete` its records, and may say which `tenant:` they belong to. Every action takes its permission from these; one that needs others is listed under `actions:` in `policy.jadpo`.
  next: Remove `publish` (agent)
  rule: policy.effect_unknown
  help: docs/language/09-policy.html#policy-effect-unknown

check failed: 1 error, 1 file
```

### `policy.effect_duplicate`

One list per effect, so "who may read a note?" is one line:

```jadpo check project=e-effect-duplicate expect=policy.effect_duplicate
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        read: [owner_id]
        read: [Access.authenticated]
    }
}
```

```text output
error: `read:` is given twice
 --> main.jadpo:26:9
   |
25 |         read: [owner_id]
   |         ---- first given here
26 |         read: [Access.authenticated]
   |         ^^^^ given again
  reason: `Note`'s policy gives each list once, so "who may read?" is always one line.
  next: Merge the two `read:` lists into one (agent)
  rule: policy.effect_duplicate
  help: docs/language/09-policy.html#policy-effect-duplicate

check failed: 1 error, 1 file
```

### `policy.subject_duplicate`

```jadpo check project=e-subject-duplicate expect=policy.subject_duplicate
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        read: [owner_id, Access.authenticated, owner_id]
    }
}
```

```text output
error: `owner_id` is listed twice
 --> main.jadpo:25:48
   |
25 |         read: [owner_id, Access.authenticated, owner_id]
   |                                                ^^^^^^^^ listed again
  reason: A grant list means "any of these", so naming someone twice adds nothing and hides which line was meant.
  next: Remove the second `owner_id` (agent)
  rule: policy.subject_duplicate
  help: docs/language/09-policy.html#policy-subject-duplicate

check failed: 1 error, 1 file
```

### `policy.declaration_duplicate`

```jadpo check project=e-declaration-duplicate expect=policy.declaration_duplicate
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        read: [owner_id]
    }

    policy: {
        update: [owner_id]
    }
}
```

```text output
error: `Note` declares `policy:` twice
 --> main.jadpo:28:5
   |
24 |     policy: {
   |     ------ first declared here
28 |     policy: {
   |     ^^^^^^ declared again
  reason: An entity's grants are one list per effect in one `policy:`, so "who can delete this?" is always one line.
  next: Move what the second `policy:` holds into the first, and remove it (agent)
  rule: policy.declaration_duplicate
  help: docs/language/09-policy.html#policy-declaration-duplicate

check failed: 1 error, 1 file
```

### `policy.system_grant_missing`

A subscriber runs as the system, and the entity it changes grants that
only to anyone at all (decided 2026-10-19):

```jadpo check project=e-system-grant-missing expect=policy.system_grant_missing
type Note = Text {
    max_length: 200
}

entity Reminder {
    id: Uuid
    note: Note
    sent: Bool

    policy: {
        create: [Access.authenticated]
        read: [Access.public]
        update: [Access.public]
    }
}

type ReminderDue = Event {
    reminder_id: Reminder.id
}

action send_reminder(reminder_id: Reminder.id) fails ReminderNotFound -> Unit {
    attempt Reminder.update(reminder_id) {
        set: {
            sent: true
        }
    }
}

subscriber send_due {
    on: ReminderDue
    run: send_reminder(event.reminder_id)
}
```

```text output
error: `send_due` runs as the system, which only `Access.public` lets update `Reminder`
 --> main.jadpo:29:12
   |
29 | subscriber send_due {
   |            ^^^^^^^^ runs as `Access.system`
  reason: A subscriber runs as the system, which an entity grants background work by naming `Access.system`; `Reminder`'s `update:` names `Access.public` and not `Access.system`, and anyone at all is never how background work is let in.
  next: Add `Access.system` to `Reminder`'s `update:`, or reach `Reminder` from a route instead (human)
  rule: policy.system_grant_missing
  help: docs/language/09-policy.html#policy-system-grant-missing

check failed: 1 error, 1 file
```

### `policy.public_grant_missing`

A route anyone may call reaches an entity that does not grant `Access.public`:

```jadpo check project=e-public-grant-missing expect=policy.public_grant_missing
type ArticleTitle = Text {
    max_length: 120
}

type ArticleView = Output {
    title: ArticleTitle
}

entity Article {
    id: Uuid
    title: ArticleTitle

    policy: {
        read: [Access.authenticated]
    }
}

query find_article(article_id: Article.id) fails ArticleNotFound -> ArticleView {
    from: Article
    where: Article.id == article_id
    missing: ArticleNotFound
}

routes Articles {
    prefix: /articles

    GET /{article_id: Article.id} {
        authentication: RouteAuthentication.none
        output: ArticleView
        run: find_article(article_id)
    }
}
```

```text output
error: `GET /articles/{article_id}` lets anyone call it, but not everyone may read `Article`
 --> main.jadpo:27:5
   |
27 |     GET /{article_id: Article.id} {
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ anyone may call it
  reason: A route with `authentication: RouteAuthentication.none` has no caller, so everything it reaches needs `Access.public`; `Article`'s `read:` doesn't name `Access.public`. Either alone is not enough.
  next: Add `Access.public` to `Article`'s `read:`, or remove `authentication: RouteAuthentication.none` from the route (human)
  rule: policy.public_grant_missing
  help: docs/language/09-policy.html#policy-public-grant-missing

check failed: 1 error, 1 file
```

### Fields

### `type.field_update_forbidden`

A user field named in policy is never changed by an ordinary update; handing the note to someone else is an action listed under `actions:`. A field marked `immutable: true` is refused the same way:

```jadpo check project=e-field-update-forbidden expect=type.field_update_forbidden
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText

    policy: {
        read: [owner_id]
        update: [owner_id]
    }
}

action give_note(note_id: Note.id, to: User.id) fails NoteNotFound, UserNotFound -> Note {
    return attempt Note.update(note_id) {
        set: {
            owner_id: to
        }
    }
}
```

```text output
error: `Note.update` changes `Note.owner_id`, which an update can't change
 --> main.jadpo:38:13
   |
38 |             owner_id: to
   |             ^^^^^^^^ named in policy
  reason: `Note`'s policy reaches the caller or the tenant through `owner_id`, so changing it hands the record to someone else. That is a named action, listed with who may call it under `actions:` in `policy.jadpo`.
  next: Remove `owner_id` from the update, or list `give_note` under `actions:` in `policy.jadpo` (agent)
  rule: type.field_update_forbidden
  help: docs/language/09-policy.html#type-field-update-forbidden

check failed: 1 error, 1 file
```

### `policy.field_widens_entity`

A field's `policy:` only takes away from its record's grants; here anyone signed in could read a note's summary when only its owner may read the note:

```jadpo check project=e-field-widens-entity expect=policy.field_widens_entity
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    summary: NoteText {
        policy: {
            read: [Access.authenticated]
        }
    }

    policy: {
        read: [owner_id]
    }
}
```

```text output
error: `Note.summary` lets `Access.authenticated` read it, which `Note` doesn't
 --> main.jadpo:30:20
   |
30 |             read: [Access.authenticated]
   |                    ^^^^^^^^^^^^^^^^^^^^ not in `Note`'s `read:`
  reason: A field's `policy:` only takes away from its record's grants: it narrows who may read the field, and never lets in anyone `Note`'s `read:` doesn't, which names `owner_id`.
  next: Remove `Access.authenticated` from `summary`'s `read:` (agent)
  rule: policy.field_widens_entity
  help: docs/language/09-policy.html#policy-field-widens-entity

check failed: 1 error, 1 file
```

### `policy.output_field_unproved`

A note's owner may call `find_note`, but only its reviewer may read the appraisal, and an output has one shape for everyone:

```jadpo check project=e-output-field-unproved expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    appraisal: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }

    policy: {
        read: [owner_id, reviewer_id]
    }
}

type NoteView = Output {
    pick: [Note.id, Note.text, Note.appraisal]
}

query find_note(note_id: Note.id) fails NoteNotFound -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}
```

```text output
error: `find_note` sends `Note.appraisal`, which not every caller may read
 --> main.jadpo:46:57
   |
46 | query find_note(note_id: Note.id) fails NoteNotFound -> NoteView {
   |                                                         ^^^^^^^^ carries `appraisal`
  reason: Anyone `Note`'s `read:` lets in may call `find_note`, the user `owner_id` points at among them, and only the user `reviewer_id` points at may read `appraisal`. An output has one shape for everyone, so a field some callers may not read can't be in it.
  next: Return an output without `appraisal` from `find_note` (agent)
  rule: policy.output_field_unproved
  help: docs/language/09-policy.html#policy-output-field-unproved

check failed: 1 error, 1 file
```

A value copied through a variable or another type keeps its field's rules, so the same appraisal built into an output by an action is refused there:

```jadpo check project=e-output-field-copied expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    appraisal: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }

    policy: {
        read: [owner_id, reviewer_id]
    }
}

type Assessment = Object {
    appraisal: Note.appraisal
}

type NoteView = Output {
    pick: [Note.id, Note.appraisal]
}

action show_note(note_id: Note.id) fails NoteNotFound -> NoteView {
    var note: Note = attempt Note.get(note_id)
    var assessment: Assessment = Assessment {
        appraisal: note.appraisal
    }
    return NoteView {
        id: note.id
        appraisal: assessment.appraisal
    }
}
```

```text output
error: `NoteView.appraisal` carries `Note.appraisal`, which not every caller of `show_note` may read
 --> main.jadpo:57:20
   |
57 |         appraisal: assessment.appraisal
   |                    ^^^^^^^^^^^^^^^^^^^^ from `Note.appraisal`
  reason: `show_note` reaches the `Note` it comes from for anyone its grants let in, the user `owner_id` points at among them, and only the user `reviewer_id` points at may read `appraisal`. What a caller is sent has one shape for everyone, and a value passed through a variable or another type keeps its field's rules.
  next: Leave `appraisal` out of `NoteView` (agent)
  rule: policy.output_field_unproved
  help: docs/language/09-policy.html#policy-output-field-unproved

check failed: 1 error, 1 file
```

A condition shows the field it tests, so a list of the notes whose
appraisal is "Excellent" is refused to their owners, who may not read it:

```jadpo check project=e-condition-field-unproved expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    appraisal: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }

    policy: {
        read: [owner_id, reviewer_id]
    }
}

type NoteView = Output {
    pick: [Note.id, Note.text]
}

query praised_notes() -> List<NoteView> {
    from: Note
    where: Note.appraisal == "Excellent"
    order: [Note.text asc]
    limit: 50
}
```

```text output
error: `praised_notes` filters on `Note.appraisal`, which not every caller may read
 --> main.jadpo:48:12
   |
48 |     where: Note.appraisal == "Excellent"
   |            ^^^^^^^^^^^^^^ tests `appraisal`
  reason: Anyone `Note`'s `read:` lets in may see the records `praised_notes` reads, the user `owner_id` points at among them, and only the user `reviewer_id` points at may read `appraisal`. Which records pass a condition shows each one's `appraisal`, so a query filters and orders only on fields every caller may read.
  next: Remove the condition on `Note.appraisal`, or test a field every caller may read (agent)
  rule: policy.output_field_unproved
  help: docs/language/09-policy.html#policy-output-field-unproved

check failed: 1 error, 1 file
```

A value made under a condition shows what the condition tests, so a verdict built from constants is refused when the `if` deciding it tests the appraisal:

```jadpo check project=e-output-field-decided expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    appraisal: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }

    policy: {
        read: [owner_id, reviewer_id]
    }
}

type Laurel = Output {
    praised: Bool
}

action judge_note(note_id: Note.id) fails NoteNotFound -> Laurel {
    var note: Note = attempt Note.get(note_id)
    if (note.appraisal == "Excellent") {
        return Laurel {
            praised: true
        }
    }
    return Laurel {
        praised: false
    }
}
```

```text output
error: `Laurel.praised` carries `Note.appraisal`, which not every caller of `judge_note` may read
 --> main.jadpo:50:22
   |
50 |             praised: true
   |                      ^^^^ decided by `Note.appraisal`
  reason: `judge_note` reaches the `Note` it comes from for anyone its grants let in, the user `owner_id` points at among them, and only the user `reviewer_id` points at may read `appraisal`. Which value is sent shows what decided it, so a value made under an `if`, a `match` or a loop, or kept by a filter, keeps the rules of the fields they test.
  next: Decide `praised` by what every caller of `judge_note` may read (agent)
  rule: policy.output_field_unproved
  help: docs/language/09-policy.html#policy-output-field-unproved

check failed: 1 error, 1 file
```

A failure raised on such a condition shows what it tests the same way:

```jadpo check project=e-failure-decided expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    appraisal: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }

    policy: {
        read: [owner_id, reviewer_id]
    }
}

type NoteView = Output {
    pick: [Note.id, Note.text]
}

type NotPraised = Failure {
    kind: FailureKind.conflict
    message: "Only a praised note can be pinned."
}

action pin_note(note_id: Note.id) fails NoteNotFound, NotPraised -> NoteView {
    var note: Note = attempt Note.get(note_id)
    if (note.appraisal != "Excellent") {
        reject NotPraised
    }
    return NoteView {
        id: note.id
        text: note.text
    }
}
```

```text output
error: `pin_note` raises `NotPraised` on a condition that tests `Note.appraisal`, which not every caller may read
 --> main.jadpo:54:9
   |
54 |         reject NotPraised
   |         ^^^^^^^^^^^^^^^^^ decided by `Note.appraisal`
  reason: `pin_note` reaches the `Note` it comes from for anyone its grants let in, the user `owner_id` points at among them, and only the user `reviewer_id` points at may read `appraisal`. Whether `NotPraised` is raised shows what the condition tested, so a failure is raised only on what every caller may read.
  next: Test a field every caller may read, or raise `NotPraised` only where callers who may read `appraisal` reach (agent)
  rule: policy.output_field_unproved
  help: docs/language/09-policy.html#policy-output-field-unproved

check failed: 1 error, 1 file
```

Which record a `get` finds shows the value it is given, so a citation only the reviewer may know can't find its record for the note's owner:

```jadpo check project=e-lookup-field-unproved expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Citation {
    id: Uuid
    title: NoteText

    policy: {
        read: [Access.authenticated]
    }
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    citation_id: Citation.id {
        on_delete: OnDelete.restricted
        policy: {
            read: [reviewer_id]
        }
    }
    text: NoteText

    policy: {
        read: [owner_id, reviewer_id]
    }
}

type CitationView = Output {
    pick: [Citation.id, Citation.title]
}

action note_citation(note_id: Note.id) fails NoteNotFound, CitationNotFound -> CitationView {
    var note: Note = attempt Note.get(note_id)
    var citation: Citation = attempt Citation.get(note.citation_id)
    return CitationView {
        id: citation.id
        title: citation.title
    }
}
```

```text output
error: `note_citation` finds a `Citation` by `Note.citation_id`, which not every caller may read
 --> main.jadpo:58:51
   |
58 |     var citation: Citation = attempt Citation.get(note.citation_id)
   |                                                   ^^^^^^^^^^^^^^^^ from `Note.citation_id`
  reason: `note_citation` reaches the `Note` it comes from for anyone its grants let in, the user `owner_id` points at among them, and only the user `reviewer_id` points at may read `citation_id`. Whether a `Citation` is found by it, and what it holds, shows what `citation_id` holds, so a value used to find a record keeps its field's rules, as one sent in an output does.
  next: Find it by a value every caller of `note_citation` may read (agent)
  rule: policy.output_field_unproved
  help: docs/language/09-policy.html#policy-output-field-unproved

check failed: 1 error, 1 file
```

An include shows the reference it follows the same way:

```jadpo check project=e-include-reference-unproved expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Citation {
    id: Uuid
    title: NoteText

    policy: {
        read: [Access.authenticated]
    }
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    citation_id: Citation.id {
        on_delete: OnDelete.restricted
        policy: {
            read: [reviewer_id]
        }
    }
    text: NoteText

    policy: {
        read: [owner_id, reviewer_id]
    }
}

type CitationView = Output {
    pick: [Citation.id, Citation.title]
}

type NoteWithCitation = Output {
    pick: [Note.id, Note.text]
    citation: CitationView
}

query note_with_citation(note_id: Note.id) fails NoteNotFound -> NoteWithCitation {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
    include: {
        citation: Note.citation_id
    }
}
```

```text output
error: `note_with_citation` includes the `Citation` each `Note` refers to through `citation_id`, which not every caller may read
 --> main.jadpo:66:9
   |
66 |         citation: Note.citation_id
   |         ^^^^^^^^ through `citation_id`
  reason: Anyone `Note`'s `read:` lets in may see the records `note_with_citation` reads, the user `owner_id` points at among them, and only the user `reviewer_id` points at may read `citation_id`. Which record an include reads shows the reference it follows, so a query includes only through references every caller may read.
  next: Remove the include, or give `Note.citation_id`'s `read:` everyone who may read a `Note` (agent)
  rule: policy.output_field_unproved
  help: docs/language/09-policy.html#policy-output-field-unproved

check failed: 1 error, 1 file
```

A caller may change a record they may not read, so the record an update gives back is proved against its `read:`:

```jadpo check project=e-update-record-unproved expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    editor_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    summary: NoteText

    policy: {
        read: [owner_id]
        update: [owner_id, editor_id]
    }
}

type SummaryView = Output {
    pick: [Note.id, Note.summary]
}

action edit_note(note_id: Note.id, revised: NoteText) fails NoteNotFound -> SummaryView {
    var note: Note = attempt Note.update(note_id) {
        set: {
            text: revised
        }
    }
    return SummaryView {
        id: note.id
        summary: note.summary
    }
}
```

```text output
error: `SummaryView.summary` carries `Note.summary`, which not every caller of `edit_note` may read
 --> main.jadpo:51:18
   |
51 |         summary: note.summary
   |                  ^^^^^^^^^^^^ from `Note.summary`
  reason: `edit_note` reaches the `Note` it comes from for anyone its grants let in, the user `editor_id` points at among them, and only the user `owner_id` points at may read `summary`. What a caller is sent has one shape for everyone, and a value passed through a variable or another type keeps its field's rules.
  next: Leave `summary` out of `SummaryView` (agent)
  rule: policy.output_field_unproved
  help: docs/language/09-policy.html#policy-output-field-unproved

check failed: 1 error, 1 file
```

What a write stores is read later, from any request, by everyone who may read the field it is stored in, so a value is stored only where all of them may read it. Here the reviewer copies the appraisal into the summary, which the note's owner may read too:

```jadpo check project=e-stored-field-unproved expect=policy.output_field_unproved
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    summary: NoteText
    appraisal: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }

    policy: {
        read: [owner_id, reviewer_id]
        update: [reviewer_id]
    }
}

action publish_appraisal(note_id: Note.id) fails NoteNotFound -> Note {
    var note: Note = attempt Note.get(note_id)
    return Note.update(note) {
        set: {
            summary: note.appraisal
        }
    }
}
```

```text output
error: `publish_appraisal` stores `Note.appraisal` in `Note.summary`, which not everyone who may read `summary` may read
 --> main.jadpo:47:22
   |
47 |             summary: note.appraisal
   |                      ^^^^^^^^^^^^^^ from `Note.appraisal`
  reason: Everyone who may read `Note.summary`, the user `owner_id` points at among them, reads what is stored there, from any request, and only the user `reviewer_id` points at may read `appraisal`. A value stored in a field keeps its own field's rules, as one sent in an output does, so it is stored only where everyone who may read it may read the value.
  next: Leave `appraisal` out of `summary`, or store it in a field only those who may read `appraisal` may read (agent)
  rule: policy.output_field_unproved
  help: docs/language/09-policy.html#policy-output-field-unproved

check failed: 1 error, 1 file
```

A copy between fields the same callers read stays allowed: the reviewer's draft, which only the reviewer may read, becomes the appraisal, which only the reviewer may read too. So does a copy into a record that refers to the one it came from, or into one of the same tenant, when everyone who may read the field may read the value there:

```jadpo check project=stored-field-same-readers
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    reviewer_id: User.id {
        on_delete: OnDelete.cascade
    }
    summary: NoteText
    draft: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }
    appraisal: NoteText {
        policy: {
            read: [reviewer_id]
        }
    }

    policy: {
        read: [owner_id, reviewer_id]
        update: [reviewer_id]
    }
}

action finish_review(note_id: Note.id) fails NoteNotFound -> Note {
    var note: Note = attempt Note.get(note_id)
    return Note.update(note) {
        set: {
            appraisal: note.draft
        }
    }
}
```

### `policy.role_field_create_forbidden`

A note's `create:` names only `owner_id`, so a note is only ever created by its owner, and `owner_id` is set from the caller rather than the request:

```jadpo check project=e-role-field-create-forbidden expect=policy.role_field_create_forbidden
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

type NoteText = Text {
    max_length: 200
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText

    policy: {
        create: [owner_id]
        read: [owner_id]
    }
}

type CreateNote = Input {
    owner_id: User.id
    text: NoteText
}

action create_note(input: CreateNote) fails UserNotFound -> Note {
    return attempt Note.create {
        set: {
            owner_id: input.owner_id
            text: input.text
        }
    }
}
```

```text output
error: `create_note` sets `Note.owner_id` from the request, not the caller
 --> main.jadpo:43:23
   |
43 |             owner_id: input.owner_id
   |                       ^^^^^^^^^^^^^^ not the caller
  reason: `Note`'s `create:` names only `owner_id`, so a `Note` is only ever created by the caller it points at. Set from the request, the create would be refused for every value but the caller's own, so it is set from the caller: the `id` of a `Principal.user` the action takes.
  next: Take the caller as a parameter typed `Principal.user`, and set `owner_id` to its `id` (agent)
  rule: policy.role_field_create_forbidden
  help: docs/language/09-policy.html#policy-role-field-create-forbidden

check failed: 1 error, 1 file
```

The fix takes the caller, `create_note(owner: Principal.user, input: CreateNote)`, and sets `owner_id: owner.id`.

## Review

### Status

Built in chunk 4.4. `jadpo check` reads each entity's `policy:` (its
`create`, `read`, `update` and `delete` lists and its `tenant:`), each
membership entity's `membership:`, and root `policy { }` with `tenant:`
and `actions:`, and reports every rule under "Errors". Routes are checked
once they and the policy read cleanly: every create, read, update and
delete a route reaches needs a grant, an action that reaches no entity
needs its line under `actions:`, and a route anyone may call reaches only
what `Access.public` is granted. A field named in policy, a reference a
role or the tenant applies through, a membership's `scope:` and `member:`,
and any field marked `immutable: true` keep their values under an ordinary
update.

The plan compiles every grant and the tenancy into each statement's
`WHERE` (a query, `get`, include, update, delete and transition), and a
create writes its row only when the new record's grants hold, in the same
statement; the runtime binds who is calling. A list holds only what the
caller may see, filtered before paging; a record the caller may not see,
read or change by `id` reports its code's own not-found failure, which the
log marks `concealed_by: policy`; a create, a change to a record the action
loaded, and a call an action's line refuses answer 403 `not_permitted`,
which OpenAPI lists on every route that runs such an action. Listing a
user's tokens or a service's keys needs `read` on its record, whether an
action lists them or a `GET` route runs the listing itself (area 8, chunk
4.4c), and revoking one or issuing a key `update`. `jadpo check` writes
the permissions report to `build/report/permissions.md` and `.json`, and
`jadpo sql` shows the policy each statement carries.

Built in chunk 4.4b: a field's own `policy:` narrows its record's
`create`, `read` and `update`, or says `FieldAccess.server_only`, and can
only take away (`policy.field_widens_entity`). Every query's output, and
every `Output` or failure client field an action builds, is proved readable
by everyone who could have reached the record it comes from, following the
value through variables, objects, lists, `match`, the collection operations
and calls to other actions (`policy.output_field_unproved`); so is the
parent every include reads, which a caller could otherwise be refused. A
create whose `create:` names only user fields sets one from the caller
(`policy.role_field_create_forbidden`). A write compiles each restricted
field it changes into its statement: a create that sets one, an update's
`set:`, a patch's field when supplied, and a transition; the whole write is
refused with 403 `not_permitted`, and a route whose action always changes a
field nobody may change is `policy.effect_ungranted`. The permissions report
gives each such field a row, and the graph and manifest hold every grant,
field restriction, tenancy and listed action, with `grants` and `scopes`
edges.

Built in chunk 4.7: a reference from one of a tenant's records to another
points inside the tenant, checked in the statement of every create and
update that sets it, with or without policy (403 `not_permitted`), so an
include of a tenant's parent proves from the child's own tenancy; and a
field a query's, an include's or a `get`'s condition or order tests, or a
transition's `when:`, is proved readable by everyone who may run it, like
an output (`policy.output_field_unproved`).

Built in chunk 4.8, from the Phase 4 review: a new membership founds only a
tenant nobody belongs to yet, so nobody joins someone else's tenant on
their own say-so, and a listed action moving a record to another tenant
needs the caller's membership there, both in the write's own statement; a
transition a field refuses is 403 `not_permitted` before its condition can
answer with its `conflict:` arm. A listed line naming only user fields and
roles held in records lets nobody call an action that changes none of
those records, which is refused as it starts (`policy.effect_ungranted` on
its route), and a gate or credential statement the plan has no policy for
refuses rather than runs without it. Values are followed through
functions as through actions; what an `if`, a `match`, a loop or a
filter's predicate tests is carried by every value made under it, by a
count, and by a failure raised on it; a `set:` value stays in the record a
write gives back, an update's record is proved against its `read:` for
every record it may be, and ids that find records and includes'
references are proved like outputs. A membership whose role may be `none`,
or one read as an include's parent, proves no role; belonging to a
record's tenant proves a list naming every role held there; a create's
user field comes from the caller on every path.

Built in chunk 5.1d (Richard, 2026-10-16): a value a write stores is
proved readable by everyone who may read the field it is stored in, as an
output is by its caller (`policy.output_field_unproved` at the value): a
create's and an update's `set:`, a patch's fields and what
`when_supplied:` sets, a counter's stored fields, and the fields a
transition sets, with whatever decided the write, any `reject` in the
action, and the transition's `when:`. A value is followed to the record
it came from by the grants' shape: the same record, the record a
reference the write sets points at, or one of the same tenant; a loop's
item is one record within its turn. A value an action stores from its
parameters is checked where it is called, through its summary, which
holds no positions, as 4.11's do.

Not yet:

- A restricted field read only in an event isn't an output: an event's
  subscribers (area 12) are server work, so `emit` stores nothing a caller
  reads. What a subscriber stores from an event is checked in chunk
  6.1c. Area 15's log redaction covers what a log holds.
- `scopes:`, with the tokens issued to other applications (`oauth_token`,
  `external_token`), after the first build.
- An acceptance for a change that widens access, and the approval review
  that shows the report's changes (areas 14 and 18).
- Indexes for policy: every field a condition filters on is a reference
  (a user field, a role's or the tenant's reference, a membership's scope
  and member), and each already has its own index (area 5), so policy adds
  none yet. Keeping a reference inside the tenant finds the record it
  points at by `id`, its primary key, and a move to another tenant finds
  the records referring to it by their reference, which has its index, so
  it needs no index or migration of its own.

### Consistency with the rest of the language

- **Fewer concepts.** The decided model removes role enums that store
  nothing, the `role:` field setting, the "role source", `invoke`,
  `operations { }` and the `Effect` values. `role` now has one meaning: the
  membership field that stores it.
- **Colon rule and enum values.** Every section is `name: { }`, and fixed
  options are `Type.value`: `OnDelete.cascade`, `FieldAccess.server_only`,
  `MembershipScope.application`, `Tenancy.shared`.
- **One setting, two kinds of value.** `scope:` takes a field or
  `MembershipScope.application`; an entity's `tenant:` takes a field (when it
  has two references to the tenant) or `Tenancy.shared`. Both read as "where
  this belongs", so they are kept.
- **Grant lists mean "any of".** That is why an OAuth scope, which must hold
  as well as a grant, is not put in the same list (question 6).
- **Policy repeated in code.** Hand-written owner filters and principal
  parameters duplicate what policy adds. Area 8 passes `current_principal`
  to actions as a parameter for the action's own use; policy never needs it.

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Who schedules and subscribers act as, and how policy applies to them | 12 | Depends on area 12 question 8 |
| How schedules and subscribers write a `FieldAccess.server_only` field, which no request may | 12 | Input to area 12 |
| Event and subscriber policy (today's candidate `policy { scope: f  Role: [invoke, read] }`, syntax review #52) | 12 | Input to area 12 |
| What a subscriber stores from an event's payload, checked against what every `emit` of it put there (chunk 5.1d) | 12 | Input to area 12 (chunk 6.1) |
| Testing policy: calling an action as a given user and expecting `not_found` or `not_permitted` | 13 | Input to area 13 |
| The permissions report (`build/report/permissions.md` and `.json`) and its place in the approval review | 14 | Carried to area 14 |
| The protected sign-off that stops an unaccepted release | 14 | Area 14 question 12 |
| Personal-data marking, erasure, export, retention and log redaction | 15 | Moved to area 15 |

### Open questions

1. **Which permission model?** Today's model has five concepts (role, role
   source, grant, field restriction, action grant). *Recommendation:* keep
   it, tidied; see [the proposals](research/09-policy-proposals.md).
   *Decided 2026-10-12:* a simpler model (Richard). Grants are keyed by what
   you do (`create`, `read`, `update`, `delete`); "who" is a user field on the
   record, a membership role, or `Access.authenticated` / `Access.public`;
   role enums exist only beside the membership that stores them; a user field
   named in policy cannot be changed by an ordinary update; field
   restrictions use the same shape, with `FieldAccess.server_only`; the global
   view is the generated permissions report; root `policy.jadpo` keeps only
   tenancy and `actions:`.
2. **Is policy always on?** Today a project with no policy has none.
   *Recommendation:* yes. Every reachable create, read, update or delete
   needs a grant from the first line of code, so an application cannot ship
   with policy forgotten. Ash refuses by default too; Supabase row-level
   security only once it is switched on per table, a known source of leaks. *Decided 2026-10-12:* yes, as recommended.
3. **How is tenancy declared?** *Recommendation:* `tenant: Company` in root
   `policy.jadpo`. Every entity then needs one reference to the tenant, or
   names it with `tenant: source_company_id` if it has two, or says
   `tenant: Tenancy.shared`; every grant applies only inside the caller's
   tenants, `Access.authenticated` included. This is the one setting that
   stops a new entity leaking across customers. Rails' `acts_as_tenant` and
   Supabase policies do the same per model or table, where it can be
   forgotten. *Decided 2026-10-12:* yes, as recommended.
4. **Where does permission to call an action live?** Area 3 removed action
   settings. *Recommendation:* every action takes its permission from the
   entities it touches; the few that touch none, or need different people,
   are listed in root `policy.jadpo` under
   `actions: { domain.action: [...] }`, with the same three kinds of who. One
   place answers "which actions are exceptions?", and `invoke` and entity
   `operations { }` go. *Decided 2026-10-12:* yes, as recommended.
5. **How does a grant choose between two references to a membership's
   scope?** An entity such as a team handover has `from_team_id` and
   `to_team_id`, both `Team.id`, so `TeamRole.lead` is ambiguous.
   *Recommendation:* name the field in brackets, `TeamRole.lead(from_team_id)`,
   read as "lead of the from-team"; only needed when there are two
   references, and the compiler says so otherwise. It matches how area 3
   names a variant's data in brackets. The alternatives are a `scope:` member
   for the whole policy (cannot grant leads of either team) or an `on`
   keyword like Oso's `"lead" on "from_team"` (a new word for one case). *Decided 2026-10-12:* yes, as recommended.
6. **How does a grant require an OAuth scope?** Area 8 decided that scopes
   from any token narrow what policy allows. *Recommendation:* a `scopes:`
   member in the entity's `policy:`, keyed by the same four names:
   `scopes: { read: OAuthScope.todos_read  update: OAuthScope.todos_write }`,
   where `OAuthScope` is generated from the scopes declared in
   `authentication.jadpo`. *Amended 2026-10-18 (lead session, area 19,
   chunk 5.5r):* `OAuthScope` is now a standard type (RFC 6749's
   scope-token), so the generated enum narrows it to the application's
   declared scopes, a closed subset, rather than reusing the name. Not
   built; it comes with `scopes:`. A caller whose token carries scopes needs both a
   grant and the scope; signed-in browsers and service keys are unaffected.
   An effect with no scope listed is refused to scoped tokens, so a new
   entity is never exposed to other applications by accident; an
   `external_token` method narrows only if it declares the scopes it reads.
   It is a separate member because grant lists mean "any of" and a scope
   must hold as well. GitHub and Google scopes work the same way: the token
   never exceeds the person. *Decided 2026-10-12:* yes, as recommended.
7. **Is a policy refusal listed in `fails`?** *Recommendation:* no. Like
   `unauthenticated`, `not_permitted` from policy is raised by the runtime
   with a fixed code, and the API description lists the 403 wherever it can
   happen. A business refusal an action raises itself is still an ordinary
   `kind: FailureKind.not_permitted` failure. *Decided 2026-10-12:* yes, as recommended.
8. **Who may issue, list and revoke service keys?** (carried from area 8)
   *Recommendation:* the `Service` record's own grants: `update` on the
   service to issue or revoke a key (`Service.issue_key`,
   `Service.revoke_key`), `read` to list them (`Service.keys`). A service
   owned through `owner_id` is then managed by its owner, with no new
   concept. *Decided 2026-10-12:* yes, as recommended.
9. **Who may call an integration?** (carried from area 11) *Recommendation:*
   whoever may run the action that calls it; no separate grant, and today's
   `external_effect` block goes. An action that only calls an integration
   touches no entity, so it is listed under `actions:` anyway. *Decided 2026-10-12:* yes, as recommended.
10. **Is a policy weakening accepted with area 7's shared acceptance
    form?** *Decided 2026-10-12:* yes, already decided in area 7: one
    acceptance file per change in `acceptances/`, the same form as breaking
    API changes, database drops and new domains.
11. **Who owns personal-data marking and the classification of internal
    fields for log redaction?** (carried from areas 4 and 5) *Decided
    2026-10-12:* area 15, Personal data. Erase maps to the `delete` grant and
    export to `read`.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Policy protection | Carried from area 1: through the approval check wherever policy lives, not by file path | 2026-10-12 |
| Root file | Carried from area 1: `policy.jadpo` holds application-wide policy, such as tenancy; it is an unnamed singleton | 2026-10-12 |
| Action settings | Carried from area 3: actions have no settings; who may call an action is decided by policy | 2026-10-12 |
| Colon rule | Carried from area 3: sections are `name: { ... }` (`policy: { }`, `membership: { }`) | 2026-10-12 |
| Setting options and lists | Carried from area 2: fixed options are enum values written `Type.value`; lists are `[ ]` | 2026-10-12 |
| Concealment | Carried from area 4: a record hidden by policy reports the same `not_found` failure; the log records the concealment | 2026-10-12 |
| Caches | Carried from area 5: caches serve display reads only; keys include tenant and policy inputs; reads inside policy never use a cache; erasure wipes cached copies | 2026-10-12 |
| Placement | Carried from areas 1 and 5: an entity's policy is in its file; actions and queries are in their own folders | 2026-10-12 |
| Policy in queries | Carried from area 6: policy is compiled into each query's `WHERE`; fields a policy condition filters on get indexes; row-level security later, opt-in | 2026-10-12 |
| The caller | Carried from area 8: policy gets the caller from the runtime; `current_principal` exists in routes only; principal fields are `id`, `subject`, `strength` | 2026-10-12 |
| Public routes | Carried from area 8: a route opts out with `authentication: RouteAuthentication.none` | 2026-10-12 |
| OAuth scopes | Carried from area 8: scopes from any token (`oauth_token`, `external_token`) narrow what policy allows | 2026-10-12 |
| Policy model | Grants keyed by effect in `policy: { create: read: update: delete: }`; who is a user field on the record, a membership role, or `Access.authenticated` / `Access.public`; role enums only beside the membership that stores them, used elsewhere by full name; a membership role applies through the reference to its scope; user fields named in policy cannot change by ordinary update, transfer is a listed action; field restrictions in the same shape plus `FieldAccess.server_only`; a lifecycle transition is an update except `delete`; generated `build/report/permissions.md` and `.json` matrix shown in the approval review; root `policy.jadpo` keeps tenancy and `actions:` (Richard) | 2026-10-12 |
| Policy always on | Yes: every reachable create, read, update or delete needs a grant from the first line of code | 2026-10-12 |
| Tenancy | `tenant: Company` in root `policy.jadpo`; each entity has one reference to the tenant, names it with `tenant: field` if it has two, or says `tenant: Tenancy.shared`; grants apply only inside the caller's tenants | 2026-10-12 |
| Action permission | From the entities an action touches; exceptions listed under `actions: { domain.action: [...] }` in `policy.jadpo`; `invoke` and `operations { }` go | 2026-10-12 |
| Two references to a scope | Name the field in brackets, `TeamRole.lead(from_team_id)`, only when two references exist | 2026-10-12 |
| OAuth scopes in grants | `scopes: { read: OAuthScope.x ... }` in the entity's `policy:`; scoped tokens need the grant and the scope; an effect with no scope is refused to scoped tokens | 2026-10-12 |
| Refusals in `fails` | Never listed; the runtime raises `not_permitted` and the API description lists the 403 | 2026-10-12 |
| Service key management | Follows the `Service` record's grants: `update` to issue or revoke, `read` to list | 2026-10-12 |
| Calling integrations | Whoever may run the action; no separate grant; `external_effect` goes | 2026-10-12 |
| Accepting policy weakening | Carried from area 7: one acceptance file per weakening in `acceptances/`, the shared form | 2026-10-12 |
| Personal data and log redaction | Designed in area 15 (Personal data); erase needs `delete`, export needs `read` | 2026-10-12 |
| Background work | Carried from area 12: schedules and subscribers act as `Access.system`, which an entity grants explicitly; subscribers stay in their event's tenant | 2026-10-13 |
| Who a grant names (chosen in chunk 4.4) | A field names the caller it points at: a reference to the principal's user entity (or its service entity, which grants that service), or that entity's own `id`, the caller themselves (`read: [id]` on the user entity). A role is `Role.value`, held by a membership whose `role:` field is typed with the enum, or `Role.value(field)` naming the reference it applies through; role enums are imported like any type. Anything else is `policy.binding_invalid` | 2026-10-16 |
| Memberships (chosen in chunk 4.4) | `membership: { scope: member: role: }`: `scope:` a reference or `MembershipScope.application`, `member:` a reference to the user entity, `role:` a field typed with a plain enum. A role applies to a record through its one reference to the membership's scope, through `id` on the scope entity itself, and everywhere for an application-wide membership; with no reference it is `policy.scope_missing`, with two `policy.scope_ambiguous`. A membership the lifecycle hides grants nothing | 2026-10-16 |
| Tenancy (chosen in chunk 4.4) | Every entity has one reference to the tenant, names it with `tenant: field`, says `tenant: Tenancy.shared`, or is the tenant itself (through `id`); otherwise `policy.scope_missing` or `policy.scope_ambiguous`. A record belongs to a caller's tenant when the caller holds a current membership, of any role, in a membership entity whose scope is the tenant; `Access.public` on a tenant's records therefore reaches only members. Creating a tenant's own record checks no tenancy, since nobody belongs to it yet, and a new membership in the tenant makes its member one only while nobody belongs to the tenant yet, founding it (revised in chunk 4.8: before, any new membership did, so anyone a membership's `create:` let in could join any tenant, as its owner too). Root `policy.jadpo` imports what it names, like any root file (`use work/entities { Company }`, `use work` for `work.found_company`); a tenant no membership is held in is `policy.role_unbound` | 2026-10-16 |
| Fields policy fixes (chosen in chunk 4.4) | An ordinary update can't change a field a grant points at the caller through, a reference a role or the tenant applies through, or a membership's `scope:` and `member:`, since each hands the record to someone else (`type.field_update_forbidden`); an action listed under `actions:` may. A field marked `immutable: true` never changes | 2026-10-16 |
| Checking routes (chosen in chunk 4.4) | Once the routes and the policy read cleanly, each route is checked against what it reaches through every action and query it runs: an effect whose list names nobody but `Access.system`, or an action that reaches no entity and signs no one in or out and isn't listed, is `policy.effect_ungranted`; a route anyone may call reaching what `Access.public` isn't granted, or a tenant's records, is `policy.public_grant_missing`. One problem per route, at the first it reaches | 2026-10-16 |
| When policy says no (chosen in chunk 4.4) | A list is filtered in its statement, before paging. A `get`, a query for one record, and an update or delete by `id` that policy hides report the code's own not-found failure, and the log says `concealed_by: policy`: a `get` and a query for one record select the grant in their own statement, so a hidden record costs no second one (amended in chunk 4.10), and an update or delete by `id` asks a second statement without policy. A create policy refuses, a change to a record the action loaded, and a call an action's line refuses are 403 `not_permitted`, raised by the runtime and never in `fails`; a required parent an include can't read is `not_permitted` too until chunk 4.4b proves it can't happen. OpenAPI lists the 403 on every route that runs an action that writes or is listed | 2026-10-16 |
| Listed actions (chosen in chunk 4.4) | An `actions:` line replaces its entities' grants on the creates, updates, deletes and transitions in the action's own body; its reads, and the actions and queries it calls, keep their own entities' grants, with the same caller. `Entity.field` names a field of that entity; a line that depends on no record (anyone signed in, or a role held across the whole application) is also checked as the action starts, wherever it is called from | 2026-10-16 |
| Tokens and keys (chosen in chunk 4.4) | `sessions.start`, `sessions.end` and `sessions.issue_token` need no grant: who may sign in is the application's own check (area 8), and an action that only signs in or out isn't one that reaches nothing. `sessions.tokens` and `Service.keys` list only holders the caller may `read`; `sessions.revoke_token`, `Service.revoke_key` and `Service.issue_key` need `update` on the holder. A refused list is empty, a refused revoke revokes nothing, and a refused issue is 403 `not_permitted`. A `GET` route that runs `sessions.tokens` or `Service.keys` itself reaches the same `read` (chunk 4.4c, area 8) | 2026-10-16 |
| Calling as someone in a test (chosen in chunk 4.4, confirmed by Richard 2026-10-16) | A call a test, fixture or seed makes runs as the `Principal.user` or `Principal.service` it passes, so policy applies as to that caller; a call that passes none runs without policy, so an action test proves behaviour and data and a route test, or a call with a caller, proves policy (area 13). Nothing a request runs leaves policy out | 2026-10-16 |
| A field's own policy (chosen in chunk 4.4b) | `policy: { create: read: update: }` on a field narrows those lists, and an effect it leaves out follows the record's; `delete:` is `policy.effect_unknown`, since a field goes with its record. Its entries resolve like the record's, and each must be one the record's list for that effect lets in: the same entry, any entry under `Access.public`, or a user field or role under `Access.authenticated` (`policy.field_widens_entity`). `FieldAccess.server_only` lets no request read or write the field. A user field or reference a field's grant reaches the caller through joins the fields only a listed action changes | 2026-10-16 |
| Writes to a restricted field (chosen in chunk 4.4b) | The write's statement holds the field's grants beside the record's, with tenancy once: a create's for each field it sets (none in an optional field is never refused), an update's `set:`, a patch's field when the request supplies it, and a transition's fields. Writing no row asks again under the record's grants: found, the field refused the caller and the whole write is 403 `not_permitted`, by `id` or not. A listed action's line replaces the record's grants but never a field's. A route whose action always sets a field nobody may change (an update's `set:`, or a create of a required field) is `policy.effect_ungranted` | 2026-10-16 |
| Proving outputs (chosen in chunk 4.4b) | Who may read a value is proved from the grants alone, never the data. A query's records are reached through its entity's `read:`, a child's through its own, and a parent's both along the reference (the user a field points at is the user it includes; a role applies to the scope its reference points at; a membership's member holds a role in its scope, which proves a list naming every role) and through the parent's `read:`; a listed action whose line depends on no record adds that line, and an update's or create's record the field grants of what it sets. A value is proved when every grant of one way the caller came proves the field's `read:`. Every parent an include reads is proved readable, or the query is `policy.output_field_unproved` at the include; an output field the query's type carries, at its result type. In actions, values are followed through variables, objects, lists, `match`, `list.filter` and `.map`, and calls (an action's parameter that reaches an output it builds is checked where it is called); a function's result carries all its arguments. An `Output` built, and a failure's `client:` field, are checked where the value is given | 2026-10-16 |
| User fields on creates (chosen in chunk 4.4b) | When every entry of an entity's `create:` is a user field, a create in an action that isn't listed under `actions:` sets at least one of them from a `Principal.user` or `Principal.service` the action takes (directly, or through a variable or object); otherwise `policy.role_field_create_forbidden` at the first such field's value. Fixtures, tests and seeds aren't checked: their calls without a caller run without policy | 2026-10-16 |
| Policy in the graph (chosen in chunk 4.4b) | A `grant` node per list an entity's `policy:` writes (`Project.read`, its effect and who as written) with a `grants` edge to the entity; a `field_restriction` node per field's own policy (`Project.budget`) with a `grants` edge to the field; a `tenancy` node per entity that belongs to the tenant (its tenant and reference) with a `scopes` edge to the entity; an `action_exception` node per line under `actions:` (`work.found_company`) with a `grants` edge to the action. The permissions report's JSON lists each field's own lists under `fields` | 2026-10-16 |
| The permissions report (chosen in chunk 4.4) | `jadpo check` writes `build/report/permissions.md` and `.json` once policy reads cleanly: a table per entity, `domain.Entity`, with each list as written or "nobody", a line on its tenancy, its membership and the fields only listed actions change, and a table of the actions listed separately; no times or totals, so a change reads as a one-line diff | 2026-10-16 |
| References stay inside the tenant | A reference from one tenant-owned record to another must point at a record of the same tenant, checked on every create and update that sets it (403 `not_permitted` otherwise), so an include of a tenant-owned parent proves from the child's own tenancy; built in chunk 4.7 (Richard) | 2026-10-16 |
| Restricted fields in conditions and orders | A query, `get`, include or write may filter or order on a field only when every caller who may run it may read that field; otherwise `policy.output_field_unproved` names the condition or order, since which records pass shows the value; built in chunk 4.7 (Richard) | 2026-10-16 |
| Keeping references inside the tenant (chosen in chunk 4.7) | The check is a condition in the write's own statement, `NOT EXISTS` a record with that `id` in another tenant, so it costs no round trip, and its unpoliced twin carries it too: what the data is holds for a test's or a seed's call without a caller as well. A record that isn't there passes, so its foreign key still reports the entity's not-found failure; one in another tenant is 403 `not_permitted`, a create's or, for an update, once the record is found under its grants. It covers references between two entities that belong to the tenant, the tenant itself included as one that belongs to itself; a reference to the tenant is the record's tenancy, or beside the one `tenant:` names, another tenant on purpose, and isn't checked. A write that changes a record's tenancy (a listed action) checks every reference it keeps and that no record of the old tenant refers to it. The record it points at is found by its primary key and the records referring to it by their reference's own index, so no index or migration is added. In proofs, a role held through the record's tenancy, in a membership scoped to the tenant, holds through the parent's, and membership of the tenant moves along the reference | 2026-10-16 |
| Conditions and orders (chosen in chunk 4.7) | Checked like outputs against how a caller reached the records: a query's `where:`, `where_present:` and `order:` (the tie-break the compiler adds isn't written, so isn't checked) through its entity's `read:`, an include's through its own entity's, a `get`'s `id`, and a transition's `when:` through the grants its write checked. A lifecycle's `visible:` isn't one: it decides what exists for every caller alike. One problem per field and section, at the field where the condition or order names it | 2026-10-16 |
| Nearest and distance orders (chosen in chunk 5.8a) | A `nearest` order (area 21) and a `geo.distance` one (area 20) stand in place of `order:` and are orders all the same: the field each measures is checked like any field an `order:` names, `policy.output_field_unproved` at the field where the order names it, so the order nearest records come in can't show a vector or a place a caller may not read. A `geo.distance` order's field is also tested by the `geo.within` line it needs, so it is reported once for each section, as a field both filtered and ordered on is (Phase 5 review) | 2026-10-19 |
| Founding a tenant (chosen in chunk 4.8) | "Nobody belongs to it yet" counts every membership held in the tenant, of every membership entity scoped to it, those its lifecycle hides included: a tenant that has had members isn't founded again. The create's own statement checks it (`NOT EXISTS`), so it costs no round trip; the founder's membership is the one being created, which the membership's `scope:` names as the tenancy's reference | 2026-10-16 |
| Moving a record to another tenant (chosen in chunk 4.8) | A listed action's update that changes a record's tenancy needs the caller's current membership, of any role, in the new tenant, in the same statement, as a create there would; the line's grants are checked on the record as it was. Refused, it is 403 `not_permitted`, as a field the caller may not change is | 2026-10-16 |
| Listed lines no record lets in (chosen in chunk 4.8) | A user field or a role held in a record, on an `actions:` line, lets someone in only on the creates, updates, deletes and transitions of the action's own body whose records it applies to. On an action whose body writes none of them it lets nobody in: the action is checked as it starts against the line's other entries (anyone signed in, a role held across the whole application), and a line left with nobody, or with only `Access.system`, refuses every caller with 403 `not_permitted`. A route running such an action is `policy.effect_ungranted` | 2026-10-16 |
| Failing closed (chosen in chunk 4.8) | A gate the plan can't build for a listed line refuses every caller, and a credential statement whose policed twin is missing is a defect when a caller runs it: neither runs without policy | 2026-10-16 |
| Field-refused transitions (chosen in chunk 4.8) | A transition setting a field the caller may not change is 403 `not_permitted` before its `when:` can answer with its `conflict:` arm: writing no row asks the condition's question with the fields' grants as well as the record's | 2026-10-16 |
| What decides a value (chosen in chunk 4.8) | Functions are followed as actions are, each with a summary of what its result carries and which parts of which parameters reach what a caller is shown. Whatever an `if`'s or `else if`'s condition, a `match`'s subject, a `for`'s list or a `list.filter` predicate tests is carried by every value made under it (assigned, returned, built, or kept by the filter), by `list.count` alone, and by `list.map`'s result, and, when a `return` or `reject` inside it may have left, by every value made after it; whether a record is there carries nothing. Each field is reported once in an action, at the first value it decides. A `reject` of a failure with no client fields is checked against what decided it is raised. A failure an entity operation or another action raises isn't followed | 2026-10-16 |
| Finding records (chosen in chunk 4.8) | A `get`'s `id`, each argument a query is run with, and a write's target are checked like an output field, at the value: a record passed whole finds by its `id`. An include's reference is checked like a field the query reads: a parent's on the record that refers to it, a child's on the child | 2026-10-16 |
| What a write gives back (chosen in chunk 4.8) | An update's record is proved against its entity's `read:` as well as its fields' own, for every record its target may be (a `get`'s, a create's, one passed in or found by `id`); its `id`, and what its `set:` gave each field, are the caller's own, and a stored field an update by `id` reads is the record's. A create's record holds what the caller set and what is new, so only its fields' own `read:` apply. The values a `set:` gives stay in the record given back. The check runs whether or not any field has a policy | 2026-10-16 |
| Proofs from memberships (chosen in chunk 4.8) | A membership whose `role:` may be `none` holds no role a proof counts on, though it still makes its member one of the tenant's people. A membership read as an include's parent, which its lifecycle may hide, proves no role. A caller who belongs to a record's tenant passes a list naming every role of every membership held in the tenant, through the record's tenancy | 2026-10-16 |
| Restricted values stored in another record | A create or update may store a value in a field only when every caller who may read that field may read the value, proved as an output is; otherwise `policy.output_field_unproved` names the `set:`, so copies between fields read by the same callers stay allowed; built in chunk 5.1d (Richard) | 2026-10-16 |
| Storing values (chosen in chunk 5.1d) | Every value a write stores is checked, at the value: a create's and an update's `set:`, each field of a patch's `Input` (at the patch) and what `when_supplied:` sets, which whether the request supplied its field decides, and each field a transition sets (at the write), which what decided that it runs and its `when:` decide. A field's readers are those its record's `read:` and its own `read:` let in, but `Access.system`, which is server work; a field no one else may read, `FieldAccess.server_only` among them, may hold anything. Readers are known only of the record a value is stored in, so the record it came from is reached from there by the grants' shape: the same record (one loaded and changed, one found by the `id` an update by `id` is given, or a stored field a counter reads); the record a reference the write sets points at, by its `id` or the `id` a `get` found it by, or one a `get` found by a reference the stored record holds, which nothing else the action runs, nor any action it calls, may change; a record of the same tenant, when the write sets its tenancy's reference from that record's own; and any other record only as what holds of anyone signed in, or of a role held across the whole application. A `for` loop's item is one record through its turn, and a value carried into another turn, through a variable the loop changes, is any record. A value from a parameter is checked where the action is called, against what is passed, through its summary. One problem per value; a field that only decides is reported once per action. Which record an update changes is checked against its caller (chunk 4.8), not its readers, who see its `id` anyway | 2026-10-16 |
| What fails decides what is stored (chosen in chunk 5.1d) | An action is all-or-nothing, so whatever a `reject` in its body tests decides every value it stores, before the `reject` or after it; a failure an entity operation or another action raises isn't followed, as for outputs. A `return` or `reject` inside a loop decides every turn after the one it may leave, from the top of the loop's body, for outputs and stores alike | 2026-10-16 |
| Events and stores (chosen in chunk 5.1d) | `emit` stores nothing a caller reads: an event's subscribers are server work, as `Access.system` reading a field is, so a restricted value may go in an event's payload. What a subscriber stores from a payload is a store like any other, checked when subscribers run (chunk 6.1) against what every `emit` of the event put in it | 2026-10-16 |
| User fields on creates (chosen in chunk 4.8) | The user field a create sets from the caller is set from the caller on every path: every value it may hold, whatever an `if` or `match` decided, is the caller's `id`; otherwise `policy.role_field_create_forbidden` | 2026-10-16 |
| Background work and `Access.public` (after chunk 6.1a) | A schedule or subscriber that reaches a statement only `Access.public` grants is a compile error: an entity grants background work explicitly with `Access.system`, so the runtime's leniency ("Background work") is never relied on (Richard, as recommended). Built for subscribers in chunk 6.1b as `policy.system_grant_missing`, at the subscriber: a grant list, a field's own list or the action's line under `actions:` that names `Access.public` and not `Access.system`, reached by its action or any it calls | 2026-10-19 |
| Batches and `Access.public` (after chunk 6.4) | A batch whose query or actions reach a statement only `Access.public` grants is a compile error, as a schedule's or subscriber's is (Richard, as recommended) | 2026-10-19 |
