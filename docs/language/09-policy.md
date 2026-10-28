---
title: Policy
area: 9
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Policy
<!-- topic: policy -->

Who may do what to which records: grants on entities and fields, tenancy,
actions that need their own permission, the generated permissions report, and
what a caller sees when policy says no.

> Not built yet: `scopes:`, and acceptances for a change that widens
> access. "What is built" below lists each.

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
| Background work | `Access.system` | schedules and subscribers, limited to the tenant their event came from |

## Grants
<!-- topic: grants -->

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
<!-- topic: a-user-field-on-the-record -->

Naming a reference to the user entity, such as `owner_id`, grants the user it
points to. Creating checks the new record, so `create: [owner_id]` means a
caller can only create a todo they own; a request naming someone else is
refused.

A user field named anywhere in a policy cannot be changed by an ordinary
update, so it needs no `immutable:`. Handing a record to someone else is a
named action listed in root `policy.jadpo` (see Actions).

## A membership role
<!-- topic: a-membership-role -->

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
<!-- topic: anyone-signed-in-or-anyone -->

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
<!-- topic: background-work -->

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

On a tenant's records, a grant naming `Access.system` lets a schedule's
run, which no tenant's event starts, in every tenant; a subscriber within
the tenant its event's field of the tenant's `id` type names; and a
batch's run within the tenant its saved values of that type name. A
subscriber or a batch whose value names no tenant (none, `none`, or
values naming different tenants) reaches no tenant's records, only those
every tenant shares: "no tenant" means every tenant for a schedule, which
names none by design, and none for work that names its tenant by a value
(area 12, "Background work and tenants"). The system never moves a record to
another tenant, which needs a membership there, and founds no tenant's
first membership. A listed action's line naming `Access.system` lets the
system in beside whoever else it names; a line naming only `Access.system`
still lets in no request. No request is ever the system: only the
runtime's runner makes the system caller, and every request, and every
call a test makes with a caller or without one, runs as someone else.

The runtime runs three of the application's actions as the system inside
a request too: the magic-link `send:` action,
delivered as a subscriber of `jadpo.MagicLinkEmail` with its delivery's
tenant, which is none; a webhook's reply action; and `on_first_sign_in:`'s.
Each reaches no tenant's records, only those every tenant shares, since
no tenant's event starts it and "no tenant" means every tenant only for
a schedule; and each is let in only by `Access.system`, so a grant or a
line under `actions:` naming `Access.public` and not `Access.system` that
one reaches is `policy.system_grant_missing`, where the action is named.

Which tenant a subscriber or a batch's run works in is named by a value:
the event's field of the tenant's `id` type, or what the run saves of it.
A caller could pass the id of a tenant they don't belong to, so that value
must come from a record of that tenant the action read or wrote under its
caller's policy; otherwise it is `policy.event_tenant_unproved` at the
value. Reading the tenant first,
`attempt Household.get(household_id)`, or naming the tenant of a record
the action changes, proves it. In full, a value proves its tenant when it
is:

- the tenant's own `id`, or the tenancy reference of a record of it, that
  the action, or an action or query it calls, read, created or changed;
- the same of a record the action is handed whole, which counts as read:
  a route passes placeholders, `input` and the caller, and a subscriber
  its event's fields, so only a body that read a record, with the same
  caller, or a run limited to its tenant, can hand one to an action
  (example 25's `invoice_round(rent_round: RentRound)` names its run's
  society from `rent_round.society_id`);
- or what a create that runs whenever the action does set a record's
  tenancy reference from, since the create needs the caller's membership
  there and the action fails with it. A create inside an `if`, a `match`,
  a loop or after a `return` that may leave first proves nothing.

Anything else, a parameter holding an id among them, is refused; what
decided the value is no part of it. It is checked in the action whose
`emit` or `batches.each` it is, never at its callers: an action that emits
from a parameter is refused even when every action calling it passes a
proved id, since a route may call it too, so pass it the record instead.
One rule holds for every action, so a subscriber's action passing its
event's tenant on, to another `emit` or a batch, reads a record of it
first, as area 12's "Declaring an event" advises for anything more than
ids.

What a batch's run stores, from its items, what it saved and its summary,
is proved as a call's arguments are, by the tenant it is limited to, and
what `batches.status` gives is held to what decides every batch's failures
(area 12, "What a run reaches under policy").

## Tenancy
<!-- topic: tenancy -->

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
an owner, of someone else's company.

### References stay inside the tenant

A reference from one of a tenant's records to another always points at a
record of the same tenant: a chore of Northwind's names one of Northwind's
projects, never Contoso's. Every create and update that sets such a
reference checks it in its own statement, with or without policy, and one
that points into another tenant is refused with 403 `not_permitted`; a
record that isn't there is still the code's own not-found failure. So whoever may read a chore
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

GET /openapi.json
    generated openapi
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
membership in the new tenant, as a create there would.

## Actions
<!-- topic: actions -->

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
`policy.effect_ungranted`.

## Field restrictions
<!-- topic: field-restrictions -->

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
  it refers to. A webhook's reply (area 11) goes back to whoever sent the
  webhook, so it is an output to `Access.public`: every field it carries,
  or that decided it, is one anyone at all may read, though its action
  runs as the system. Since the system finds every
  record its grants name, a reply that counts or tests for records shows
  whether they are there, so they must be records anyone at all may
  read; and a failure is answered to the sender too, so what decides
  whether the action fails (a condition, a `get` or a change by `id`
  finding nothing, a `conflict:`) and a failure's client fields are held
  to the same proof.
- **Conditions and orders.** Which records pass a condition, and the order
  they come in, show the fields they test, so a query, `get`, include or
  write may filter or order on a field only when everyone who may run it
  may read that field; otherwise `policy.output_field_unproved` names the
  condition or order. A
  transition's `when:` is a write's condition: refusing a record shows what
  it tests.
- **Copies.** Passing a value through a variable, another type, a
  function or the record a write stores it in does not escape the field's
  rules; the compiler tracks where it came from.
- **What decides a value.** A value made under an `if`, a `match` or a
  loop, or kept by a filter, shows what they test, so it carries their
  fields' rules too, as does a count of what a filter kept, a failure
  raised on such a condition, and what comes after a `return` or `reject`
  it may have taken.
- **Finding records.** Which record a `get`, a query or a write finds shows
  the value it found it by, and an include shows the reference it follows,
  so each is proved like an output.
- **What a write gives back.** The record an update returns is proved
  against its `read:` as well as its fields', since a caller may change a
  record they may not read; what its `set:` gave a field, and a created
  record's values, are the caller's own.
- **What a write stores.** A value stored in a field is read later, from
  any request, by everyone who may read that field, so a create or update
  stores a value in a field only when every one of them may read it,
  proved as an output is; otherwise `policy.output_field_unproved` names
  the value in the write. A
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
  so it may; area 12 says how schedules and subscribers write one.

A field's `policy:` narrows `create`, `read` and `update`; it is deleted
with its record. Who may read a value is proved from the grants alone, not
from the data, because two records that each refer to a company may refer
to different companies: a role held in one says nothing about the other.

A restricted field read only in an event isn't an output: an event's
subscribers (area 12) are server work, so `emit` stores nothing a caller
reads; what a subscriber stores from an event is checked where it stores
it (area 12, "Subscribers"). Area 15's log redaction covers what a log
holds.

## The permissions report
<!-- topic: the-permissions-report -->

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

Policy adds no index of its own: every field a condition filters on is a
reference (a user field, a role's or the tenant's reference, a
membership's scope and member), and each already has its own index (area
5). Keeping a reference inside the tenant finds the record it points at by
`id`, its primary key, and a move to another tenant finds the records
referring to it by their reference, which has its index, so it needs no
index or migration of its own.

## Protecting policy
<!-- topic: protecting-policy -->

Decided in area 1: an agent can edit policy, but the change cannot ship
without a person's approval, wherever it is written. The compiler compares
the whole application before and after, because access also widens when an
output gains a field, a route becomes public or an action starts touching
another entity. Each widening needs an acceptance file in `acceptances/`, the
form area 7 decided for breaking API changes and database drops.

Schedules and subscribers act as the system ("Background work"), and what
`Access.system` is granted shows in the permissions report. The compiler's own purge of
soft-deleted records (area 5) is the only operation outside policy.

## When policy says no
<!-- topic: when-policy-says-no -->

The compiler adds policy to the SQL `WHERE` of every query and write (area
6), and indexes every field a policy condition filters on; `jadpo sql`
shows the policy each statement carries.

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

GET /openapi.json
    generated openapi
    status 200

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
<!-- topic: caches-and-personal-data -->

Decided in area 5: a record cache applies policy on every read, a
query-result cache is keyed by the tenant and every input policy used, and
policy itself always reads the database.

Area 15 owns personal data. Policy supplies its permissions: erasing a
person's data needs `delete` on their record and exporting it needs `read`.

## What is built
<!-- topic: policy-built -->

Each feature not built yet is `toolchain.not_built_yet` where it is written.

| Feature | Status |
|---|---|
| Everything else on this page | Built. |
| `scopes:`, with the tokens issued to other applications (`oauth_token`, `external_token`) | Not built yet. |
| An acceptance for a change that widens access, and the approval review that shows the report's changes | Not built yet (areas 14 and 18). |

## Errors

Every error this area's checks produce, with an example that triggers it.
The examples are one file each, so the principal and root `policy { }` sit
beside the entities they name; a project keeps them in `app.jadpo` and
`policy.jadpo`.

### Owner fields and memberships

- [`policy.role_unbound`](../errors/policy.role_unbound.md)
- [`policy.binding_invalid`](../errors/policy.binding_invalid.md)
- [`policy.membership_scope_required`](../errors/policy.membership_scope_required.md)
- [`policy.membership_member_required`](../errors/policy.membership_member_required.md)
- [`policy.membership_role_required`](../errors/policy.membership_role_required.md)
- [`policy.membership_duplicate`](../errors/policy.membership_duplicate.md)
### Scope

- [`policy.scope_missing`](../errors/policy.scope_missing.md)
- [`policy.scope_ambiguous`](../errors/policy.scope_ambiguous.md)
- [`policy.scope_duplicate`](../errors/policy.scope_duplicate.md)
### Grants

- [`policy.effect_ungranted`](../errors/policy.effect_ungranted.md)
- [`policy.effect_unknown`](../errors/policy.effect_unknown.md)
- [`policy.effect_duplicate`](../errors/policy.effect_duplicate.md)
- [`policy.subject_duplicate`](../errors/policy.subject_duplicate.md)
- [`policy.declaration_duplicate`](../errors/policy.declaration_duplicate.md)
- [`policy.system_grant_missing`](../errors/policy.system_grant_missing.md)
- [`policy.event_tenant_unproved`](../errors/policy.event_tenant_unproved.md)
- [`policy.public_grant_missing`](../errors/policy.public_grant_missing.md)
### Fields

- [`type.field_update_forbidden`](../errors/type.field_update_forbidden.md)
- [`policy.field_widens_entity`](../errors/policy.field_widens_entity.md)
- [`policy.output_field_unproved`](../errors/policy.output_field_unproved.md)
- [`policy.role_field_create_forbidden`](../errors/policy.role_field_create_forbidden.md)
