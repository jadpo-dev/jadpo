---
title: Policy
area: 9
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Policy
<!-- topic: policy -->

Policy says who may do what to which stored records. Each entity, a kind
of stored record such as a todo, lists who may create, read, change and
delete its records. The compiler applies those lists to every read and
write your code makes, so you never write a permission check yourself.

This todo may be created, read, changed and deleted only by its owner:

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

`owner_id` is a reference to a `User`, so the person it points to is the
one let in. A caller no list names is refused, and a todo they may not
read behaves as if it didn't exist ([When policy says no](#when-policy-says-no)).

Each entry in a list names who it lets in, in one of four ways:

| Who | Written | Lets in |
|---|---|---|
| [A user field on the record](#a-user-field-on-the-record) | `owner_id` | the user that field points to |
| [A membership role](#a-membership-role) | `CompanyRole.editor` | a user with that role in the record's company |
| [Anyone signed in, or anyone](#anyone-signed-in-or-anyone) | `Access.authenticated`, `Access.public` | anyone signed in; anyone at all |
| [Background work](#background-work) | `Access.system` | schedules and subscribers, limited to the tenant their event came from |

When your app is shared by many customer companies, [Tenancy](#tenancy)
keeps each company's records to its own members, whatever the lists say.

## Grants
<!-- topic: grants -->

A grant says who may do one thing to an entity's records. An entity's
`policy:` holds up to four lists of them, one for each thing: `create`,
`read`, `update` and `delete`. Anyone a list names is enough to let a
caller in, and anything not listed is refused. So "who can delete a
project?" is one line in the project's file. In the `Todo` at the top of
[Policy](#policy), `delete: [owner_id]` lets the todo's owner delete it,
and nobody else.

Policy is always on. From the first line of code, every create, read,
update or delete your code can reach needs a grant.

### Which grant an action needs

You never write which grant an action needs. The compiler sees what each
query and action reads and writes. Every action that updates a `Todo`
needs `update` on it, so ten actions that edit todos share one line of
policy. An action that reaches several entities needs a grant on each.
When an action calls another domain's action, the called action is
checked against its own entities, with the same caller.

### Transitions and deletes

A lifecycle transition, a named change of a record's state such as
archiving it ([Lifecycle](05-data.md#lifecycle)), is an update. The one
exception is deleting: `Todo.delete(todo)` needs `delete`, whether it
removes the record or runs the lifecycle's `delete` transition
([The operations every entity gets](06-reads-and-writes.md#the-operations-every-entity-gets)).
So a soft delete, which keeps the record but hides it, still needs
`delete`.

## A user field on the record
<!-- topic: a-user-field-on-the-record -->

Use a user field when a record belongs to one person, such as a todo and
its owner. A user field is a reference to the user entity, the entity your
app's [principal](08-identity.md#the-principal) names for the people who
sign in. Naming it in a list, such as `read: [owner_id]`, lets in the user
it points to. A reference to the principal's service entity lets in that
service in the same way.

On the user entity itself, `id` names the caller's own record:
`read: [id]` lets each person read only their own.

### On a create

A create is checked against the new record. So `create: [owner_id]` means
a caller can create a todo only if they are its owner. A request that
names someone else as the owner is refused.

### The field can't be changed by an update

A user field that any list names can't be changed by an ordinary update,
since changing it would hand the record to someone else. You don't mark it
`immutable:` for that. To hand a record to someone else, write a named
action and list it in root `policy.jadpo`, as [Actions](#actions) shows.

## A membership role
<!-- topic: a-membership-role -->

Use a role when people get access by belonging to something, such as the
editors of a company. A role is stored in a record of its own: a
membership entity, whose `membership:` section names three of its fields:

- `scope`: the record the role is held in, such as the company;
- `member`: who holds it, a reference to the user entity;
- `role`: the field that holds the role, typed with an enum.

The role enum lives beside the membership in the same domain, and other
domains use it by its full name. There is no global roles file.

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

Here only a company's owners may add, change or remove members. Each
member may also read their own membership, and delete it to leave.

### Using a role on other records

A role applies to another entity through that entity's reference to the
scope, the same reference [tenancy](#tenancy) uses. A `Project` with a
`company_id` takes its grants from the roles held in its company:

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

### How roles combine

- Roles have no ranking. A role lets in only where a list names it, so an
  owner who may also edit is listed under both.
- Only current memberships count. A membership its lifecycle hides grants
  nothing, and removing one takes effect on the next request.
- A role held across the whole application, such as staff, has a
  membership with `scope: MembershipScope.application`.
- An entity can have two references to the scope, such as a handover with
  a `from_team_id` and a `to_team_id`. Then each grant names the reference
  it means: `TeamRole.lead(from_team_id)` is the lead of the from-team.

## Anyone signed in, or anyone
<!-- topic: anyone-signed-in-or-anyone -->

Use these for records that everyone signed in, or the whole world, may
see, such as published articles. `Access.authenticated` lets in anyone who
is signed in. `Access.public` lets in anyone at all, signed in or not.

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

Anyone may read an article. Only the company's editors may create or
change one, and only its owners may delete one.

A public read also needs a route that skips sign-in, one that says
`authentication: RouteAuthentication.none`
([Routes: signed in by default](08-identity.md#routes-signed-in-by-default)).
Either alone isn't enough: the route lets in callers who aren't signed in,
and the grant lets them read the record.

There is no administrator who bypasses policy.

## Background work
<!-- topic: background-work -->

Schedules and event subscribers are the work your app does without a
request ([Jobs and events](12-jobs-and-events.md)). They run as the system
caller, which gets exactly what `Access.system` grants. To let them read or
change an entity's records, name `Access.system` in its lists, such as
`update: [owner_id, Access.system]`.

The system is nobody signed in, and it holds no membership. So:

- a user field, a role and `Access.authenticated` never let it in;
- a grant list, a field's own list or an `actions:` line lets it in only
  by naming `Access.system`.

### `Access.public` doesn't let background work in

An entity grants background work explicitly. A schedule, a subscriber or
a batch that reaches a statement only `Access.public` grants is a compile
error, `policy.system_grant_missing`, in the same way that
`policy.effect_ungranted` refuses such a route.

### Which tenant background work reaches

On a tenant's records ([Tenancy](#tenancy)), a grant naming `Access.system`
lets in:

- a schedule's run in every tenant, since no tenant's event starts it;
- a subscriber within the tenant its event names, in the event's field
  of the tenant's `id` type;
- a batch's run within the tenant its saved values of that type name.

A subscriber or a batch whose value names no tenant reaches no tenant's
records, only those every tenant shares. A value names no tenant when
there is none, when it is `none`, or when its values name different
tenants. So "no tenant" means every tenant for a schedule, which names
none by design, and none for work that names its tenant by a value. [Jobs
and events](12-jobs-and-events.md) covers this under "Background work and
tenants".

The system never moves a record to another tenant, which needs a
membership there, and never founds a tenant's first membership.

### The system and `actions:` lines

A line under `actions:` that names `Access.system` lets the system in
beside whoever else the line names. A line naming only `Access.system`
still lets in no request.

No request is ever the system. Only the runtime's runner makes the system
caller. Every request, and every call a test makes, with a caller or
without one, runs as someone else.

### Actions the system runs inside a request

The runtime runs three of your app's actions as the system, even inside a
request:

- the magic-link `send:` action ([Magic links](08-identity.md#magic-links)),
  delivered as a subscriber of `jadpo.MagicLinkEmail` with its delivery's
  tenant, which is none;
- a webhook's reply action
  ([Webhooks that need a reply](11-integrations.md#webhooks-that-need-a-reply));
- the `on_first_sign_in:` action, which [Identity](08-identity.md) runs
  for a person the app hasn't seen before.

None of them reaches a tenant's records, only those every tenant shares,
since no tenant's event starts them and "no tenant" means every tenant
only for a schedule. Each is let in only by `Access.system`. So when one
of them reaches a grant, or a line under `actions:`, that names
`Access.public` and not `Access.system`, that is
`policy.system_grant_missing`, reported where the action is named.

### Proving which tenant work is for

A subscriber or a batch's run works in the tenant a value names: the
event's field of the tenant's `id` type, or what the run saves of it. A
caller could pass the id of a tenant they don't belong to. So that value
must come from a record of that tenant the action read or wrote under its
caller's policy. Otherwise it is `policy.event_tenant_unproved`, reported
at the value.

The usual fix is to read the tenant first,
`attempt Household.get(household_id)`, or to name the tenant of a record
the action changes. In full, a value proves its tenant when it is one of
these:

- the tenant's own `id`, or the tenancy reference of one of the tenant's
  records, that the action, or an action or query it calls, read, created
  or changed. The tenancy reference is the field that ties a record to
  its tenant, such as `company_id`.
- the same, of a record the action is handed whole, which counts as read.
  A route passes path placeholders, `input` and the caller, and a
  subscriber passes its event's fields. So only a body that read a record
  with the same caller, or a run limited to its tenant, can hand one to an
  action. For example, a batch's action
  `invoice_round(rent_round: RentRound)` names its run's society from
  `rent_round.society_id`.
- the value a create sets a record's tenancy reference from, when the
  create runs whenever the action does. The create needs the caller's
  membership in that tenant, and the action fails with it. A create
  inside an `if`, a `match` or a loop, or after a `return` that may leave
  first, proves nothing.

Anything else is refused, including a parameter that holds an id. Only
where the value came from counts, not what decided it.

The check runs in the action whose `emit` or `batches.each` uses the
value, never at its callers. An action that emits from a parameter is
refused even when every action calling it passes a proved id, since a
route may call it too. Pass it the record instead. The same rule holds
for every action, so a subscriber's action that passes its event's tenant
on, to another `emit` or a batch, reads a record of that tenant first, as
[Declaring an event](12-jobs-and-events.md#declaring-an-event) advises
for anything more than ids.

What a batch's run stores, from its items, what it saved and its
summary, is proved as a call's arguments are, by the tenant the run is
limited to. What `batches.status` gives is held to what decides every
batch's failures. [Jobs and events](12-jobs-and-events.md) covers this
under "What a run reaches under policy".

## Tenancy
<!-- topic: tenancy -->

Use tenancy when your app is shared by many customer companies, so that
one company never sees another's data. Each customer is a tenant: one
record of an entity you choose, such as `Company`.

People belong to a company through a membership. A membership is a
stored record, such as a `CompanyMembership`, that says which user holds
which role in which company. Its entity has a `membership:` section naming
those three fields: `scope`, the company; `member`, the user; and `role`,
such as `CompanyRole.owner` ([A membership role](#a-membership-role)
shows one in full). Grants then name roles: an entry such as
`CompanyRole.viewer` in a record's `read:` list lets in the users who hold
that role in the record's company.

### Naming the tenant

The project's root `policy.jadpo`, the policy file at the top of the
project beside `app.jadpo`, names the tenant entity once:

```jadpo fragment
// policy.jadpo
policy {
    tenant: Company
}
```

This file holds the rules for the whole app: the tenant, and the
[actions](#actions) that need a permission of their own. Each entity's own
`policy:` section still lists who may create, read, change and delete its
records. Naming a tenant adds these rules to every entity:

- Every entity needs one reference to `Company`, its tenancy reference,
  such as `company_id`. A task carries its own `company_id`, not only its
  project's.
- Every grant on a tenant's records applies only within companies where
  the caller has a membership. A grant is an entry in one of an entity's
  lists, such as `read: [CompanyRole.viewer]` ([Grants](#grants)). This
  holds for `Access.authenticated`, which lets in anyone signed in, too:
  on a tenant's records it lets in anyone signed in who belongs to the
  record's company.
- An entity every tenant shares, such as a list of countries or `User`
  itself, says `tenant: Tenancy.shared` in its `policy:`. Its records
  belong to no company, and its grants apply as they would without
  tenancy.

A request has no current tenant of its own, and the signed-in caller
carries no company. Each record's tenant is the company its tenancy
reference names, and a caller who belongs to several companies is let in
to each one's records by their membership there. A route says which
company it works in through its input, such as a `company_id` in its path
or body, and policy checks the caller's membership in the company of
each record the route reads or changes.

An entity with two references to `Company` names, in its `policy:`, the
one that decides where it belongs, such as `tenant: source_company_id`.

An entity you add belongs to a tenant unless a person accepts otherwise,
as every change that widens access needs
([Protecting policy](#protecting-policy)).

A company's first membership founds it, in the action that creates the
company ([Founding a tenant](#founding-a-tenant)). Every other membership
is created by someone who belongs already, as the membership's own
`create:` says. So nobody makes themselves a member, or an owner, of
someone else's company.

### References stay inside the tenant

A reference from one of a tenant's records to another always points at a
record of the same tenant: a chore of Northwind's names one of Northwind's
projects, never Contoso's. Every create and update that sets such a
reference checks it as part of that same write, whether or not the
caller's grants would allow it. One that points into another tenant is
refused with 403 `not_permitted`. A reference to a record that doesn't
exist at all still gets that entity's own not-found failure, such as
`ProjectNotFound`.

Because a chore and its project are always in the same company, anyone
who may read a chore has their role in the project's company too. So a
query of chores may include each chore's project without showing anyone
another company's data. Here a chore belongs to a company and to one of
its projects:

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

In the first test, Ann owns both companies, so only the check on the
project reference refuses her. In the second, Bob, a viewer of Northwind,
sees its chores with their projects:

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

A reference straight to `Company` isn't checked this way. Either it is
the record's tenancy reference, which is what puts the record in its
company, or the entity has two references to `Company`, and the one its
`tenant:` doesn't name points at another company on purpose.

### A unique rule holds within its tenant

A `unique:` rule on a tenant's records holds within each tenant. Northwind
and Contoso may each have a project called Launch, but a second Launch in
Northwind is the failure the create maps it to under `conflict:`, here
`ProjectNameTaken`. So a conflict never tells one company what another
stores.

The compiler does this by adding the record's tenancy reference to the
rule's key, the set of fields that must be unique together, so `name`
becomes unique per `company_id`. A rule that already includes the tenancy
reference is left as it is. An entity every tenant shares, and the tenant
entity itself, keep one key across the whole application. Here `Member` is
shared, so two members can never have the same `handle`.

```jadpo check project=tenant-keys file=app.jadpo hidden
use work/entities { Member }

application {}

principal {
    user: {
        subject: Member.handle
    }
}
```

```jadpo check project=tenant-keys file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "work_session"
    }
}
```

```jadpo check project=tenant-keys file=policy.jadpo hidden
use work/entities { Company }

policy {
    tenant: Company
}
```

```jadpo check project=tenant-keys file=domains/work/types/handle.jadpo hidden
type Handle = Text {
    min_length: 3
    max_length: 40
}
```

```jadpo check project=tenant-keys file=domains/work/types/label.jadpo hidden
type Label = Text {
    min_length: 1
    max_length: 80
}
```

```jadpo check project=tenant-keys file=domains/work/types/company_role.jadpo hidden
/// What a member may do in a company.
type CompanyRole = Enum {
    owner
    viewer
}
```

```jadpo check project=tenant-keys file=domains/work/types/project_card.jadpo hidden
use work/entities { Project }

/// A project as members see it.
type ProjectCard = Output {
    pick: [Project.id, Project.name]
}
```

```jadpo check project=tenant-keys file=domains/work/types/seat_view.jadpo hidden
use work/entities { CompanyMembership }

/// A member's seat in a company.
type SeatView = Output {
    pick: [CompanyMembership.id, CompanyMembership.company_id, CompanyMembership.role]
}
```

```jadpo check project=tenant-keys file=domains/work/failures/handle_taken.jadpo hidden
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
```

```jadpo check project=tenant-keys file=domains/work/failures/project_name_taken.jadpo hidden
type ProjectNameTaken = Failure {
    kind: FailureKind.conflict
    message: "The company already has a project with that name."
}
```

```jadpo check project=tenant-keys file=domains/work/entities/member.jadpo hidden
use work/types { Handle }

/// Someone who signs in; every company shares them, so a handle is unique
/// across the application.
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

```jadpo check project=tenant-keys file=domains/work/entities/company.jadpo hidden
use work/types { CompanyRole, Label }

/// A customer: the tenant every other record belongs to.
entity Company {
    id: Uuid
    name: Label

    policy: {
        create: [Access.authenticated]
        read: [CompanyRole.owner, CompanyRole.viewer]
    }
}
```

```jadpo check project=tenant-keys file=domains/work/actions/register.jadpo hidden
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

```jadpo check project=tenant-keys file=domains/work/actions/start_project.jadpo hidden
use work/entities { Company, CompanyNotFound, Project }
use work/types { Label, ProjectCard }
use work/failures { ProjectNameTaken }

/// What starting a project takes.
type StartProject = Input {
    company_id: Company.id
    name: Label
}

/// Starts a project in a company.
action start_project(input: StartProject) fails CompanyNotFound, ProjectNameTaken -> ProjectCard {
    var project: Project = attempt Project.create {
        set: {
            company_id: input.company_id
            name: input.name
        }
        conflict: {
            Project.name: ProjectNameTaken
        }
    }
    return ProjectCard {
        id: project.id
        name: project.name
    }
}
```

```jadpo check project=tenant-keys file=domains/work/routes/projects.jadpo hidden
use work/types { ProjectCard }
use work/actions { StartProject, start_project }

/// A company's projects.
routes Projects {
    prefix: /projects

    /// Starts a project.
    POST / {
        input: StartProject
        output: ProjectCard
        status: SuccessStatus.created
        run: start_project(input)
    }
}
```

```jadpo check project=tenant-keys file=domains/work/entities/project.jadpo
use work/entities { Company }
use work/types { CompanyRole, Label }

/// A piece of work in a company.
entity Project {
    id: Uuid
    company_id: Company.id {
        on_delete: OnDelete.cascade
    }
    name: Label {
        unique: true  // within its company
    }

    policy: {
        create: [CompanyRole.owner]
        read: [CompanyRole.owner, CompanyRole.viewer]
    }
}
```

```jadpo check project=tenant-keys file=domains/work/routes/projects.test.jadpo
use work/entities { Member }
use work/types { SeatView }
use work/actions { NewCompany, StartProject, found_company, register }

test "two companies may each have a project called Launch, but not two in one" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var northwind: SeatView = found_company(
        NewCompany {
            name: "Northwind"
        },
        Principal.user(ann)
    )
    var contoso: SeatView = found_company(
        NewCompany {
            name: "Contoso"
        },
        Principal.user(bob)
    )
    var first: Response = request {
        method: HttpMethod.post
        path: "/projects"
        as: Principal.user(ann)
        input: StartProject {
            company_id: northwind.company_id
            name: "Launch"
        }
    }
    assert first.status == 201
    var second: Response = request {
        method: HttpMethod.post
        path: "/projects"
        as: Principal.user(bob)
        input: StartProject {
            company_id: contoso.company_id
            name: "Launch"
        }
    }
    assert second.status == 201
    var refused: Response = request {
        method: HttpMethod.post
        path: "/projects"
        as: Principal.user(ann)
        input: StartProject {
            company_id: northwind.company_id
            name: "Launch"
        }
    }
    assert refused.status == 409
    assert refused.code == "project_name_taken"
}
```

Since the key holds the tenancy reference, a query that finds one record
by a unique rule
([How many records come back](06-reads-and-writes.md#how-many-records-come-back))
must test the tenancy reference with `==` beside the rule's fields, such
as `Project.company_id == company_id`. It needn't when it tests a
reference to another of the tenant's records, since that record already
names the tenant.

The field a person signs in by, their subject, such as an email address,
is on an entity every tenant shares, so it is unique across the app
([The principal](08-identity.md#the-principal)).

When you add tenancy to an existing app, `jadpo migrate new` drops each
unique key that didn't hold the tenancy reference and makes it again with
the tenancy column, `UNIQUE ("name", "company_id")`. The migration needs
no decision from you, since every set of rows the old key allowed, the new
one allows too.

### Founding a tenant

A new company has no members, so nobody holds a role that could let
them create its first membership. That first membership founds the
company, and it is allowed in one place only: the action that creates the
company's record. There, the membership's scope is the `id` that the
company's create gave back, its member is the caller, and nobody belongs
to the company yet.

Any other new membership needs someone who belongs already, even one that
names its own caller as the member. In a grant list, a user field such as
`member_id` lets in the user it points to. So
`create: [CompanyRole.owner, member_id]` lets an owner add anyone, and
anyone else only themselves, and only in a company they already belong
to. A stranger can't add themselves to a company whose members have all
left: that is 403 `not_permitted`. Founding a new company still makes its
founder a member:

```jadpo check project=tenant-keys file=domains/work/entities/company_membership.jadpo
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
        create: [CompanyRole.owner, member_id]
        read: [CompanyRole.owner, member_id]
        delete: [CompanyRole.owner, member_id]
    }
}
```

```jadpo check project=tenant-keys file=domains/work/actions/found_company.jadpo
use work/entities { Company, CompanyMembership }
use work/types { CompanyRole, Label, SeatView }

/// What founding a company takes.
type NewCompany = Input {
    name: Label
}

/// Starts a company with its founder as its owner.
action found_company(input: NewCompany, founder: Principal.user) -> SeatView {
    var company: Company = Company.create {
        set: {
            name: input.name
        }
    }
    var seat: CompanyMembership = CompanyMembership.create {
        set: {
            company_id: company.id
            member_id: founder.id
            role: CompanyRole.owner
        }
    }
    return SeatView {
        id: seat.id
        company_id: seat.company_id
        role: seat.role
    }
}
```

```jadpo check project=tenant-keys file=domains/work/actions/add_member.jadpo hidden
use work/entities { Company, CompanyMembership, CompanyNotFound, Member, MemberNotFound }
use work/types { CompanyRole, SeatView }

/// Who joins a company, and as what.
type NewMember = Input {
    member_id: Member.id
    role: CompanyRole
}

/// Gives a member a role in a company.
action add_member(
    company_id: Company.id,
    input: NewMember
) fails CompanyNotFound, MemberNotFound -> SeatView {
    var seat: CompanyMembership = attempt CompanyMembership.create {
        set: {
            company_id: company_id
            member_id: input.member_id
            role: input.role
        }
    }
    return SeatView {
        id: seat.id
        company_id: seat.company_id
        role: seat.role
    }
}
```

```jadpo check project=tenant-keys file=domains/work/actions/leave.jadpo hidden
use work/entities { CompanyMembership, CompanyMembershipNotFound }

/// Gives up a seat in a company.
action leave(seat_id: CompanyMembership.id) fails CompanyMembershipNotFound -> Unit {
    attempt CompanyMembership.delete(seat_id)
}
```

```jadpo check project=tenant-keys file=domains/work/routes/companies.jadpo migrate=initial hidden
use work/entities { Company }
use work/types { SeatView }
use work/actions { NewCompany, NewMember, add_member, found_company }

/// Companies and their members.
routes Companies {
    prefix: /companies

    /// Founds a company, its caller its owner.
    POST / {
        input: NewCompany
        output: SeatView
        status: SuccessStatus.created
        run: found_company(input, current_principal.user)
    }

    /// Gives a member a role in a company.
    POST /{company_id: Company.id}/members {
        input: NewMember
        output: SeatView
        status: SuccessStatus.created
        run: add_member(company_id, input)
    }
}
```

```jadpo check project=tenant-keys file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    authentication browser_session

POST /companies
    caller user
    input NewCompany (body)
    output SeatView
    status 201

POST /companies/{company_id: Uuid}/members
    input NewMember (body)
    output SeatView
    status 201
    fails company_not_found
    fails member_not_found

GET /openapi.json
    generated openapi
    status 200

POST /projects
    input StartProject (body)
    output ProjectCard
    status 201
    fails company_not_found
    fails project_name_taken

enum CompanyRole
    variant owner
    variant viewer

input NewCompany
    name: Text { min_length: 1, max_length: 80 }

input NewMember
    member_id: Uuid
    role: CompanyRole

output ProjectCard
    id: Uuid
    name: Text { min_length: 1, max_length: 80 }

output SeatView
    company_id: Uuid
    id: Uuid
    role: CompanyRole

input StartProject
    company_id: Uuid
    name: Text { min_length: 1, max_length: 80 }

failure company_not_found
    status 404

failure member_not_found
    status 404

failure project_name_taken
    status 409
    message "The company already has a project with that name."
```

```jadpo check project=tenant-keys file=domains/work/routes/companies.test.jadpo
use work/entities { Member }
use work/types { CompanyRole, SeatView }
use work/actions { NewCompany, NewMember, found_company, leave, register }

test "nobody seats themselves in a company everyone has left" {
    var ann: Member = attempt register("ann")
    var eve: Member = attempt register("eve")
    var seat: SeatView = found_company(
        NewCompany {
            name: "Northwind"
        },
        Principal.user(ann)
    )
    attempt leave(seat.id)
    var seized: Response = request {
        method: HttpMethod.post
        path: "/companies/{company_id}/members"
        params: {
            company_id: seat.company_id
        }
        as: Principal.user(eve)
        input: NewMember {
            member_id: eve.id
            role: CompanyRole.owner
        }
    }
    assert seized.status == 403
    assert seized.code == "not_permitted"
    var founded: Response = request {
        method: HttpMethod.post
        path: "/companies"
        as: Principal.user(eve)
        input: NewCompany {
            name: "Contoso"
        }
    }
    assert founded.status == 201
}
```

### Moving a record to another tenant

Moving a record to another company is rare, and it needs an action of its
own. Only an action listed under `actions:` in root `policy.jadpo` may do
it ([Actions](#actions) shows the list). The write checks every reference
the record keeps, and that no record of the old company refers to it. In
the new company, it needs what creating the moved record there would: the
entity's own `create:` and the caller's own membership there.

A membership moved in from another company holds no role its mover
couldn't give there. So a viewer can't move in an owner's membership they
hold elsewhere.

## Actions
<!-- topic: actions -->

Most actions need no permission of their own: an action takes its
permission from the entities it touches. Root `policy.jadpo` lists the few
that can't, under `actions:`:

- an action that touches no entity, such as an operator's health report,
  which is otherwise refused;
- an action that needs different people from its entity's grants, such as
  handing a todo to someone else, or creating a company together with its
  first owner's membership.

Each line names an action by its domain and name, and lists who may run
it, with the same kinds of entry as a grant:

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

`Todo.owner_id` means the current owner of the todo the action changes.
For that action, the listed people replace the entity grants on what it
reaches. [Field restrictions](#field-restrictions) and
[tenancy](#tenancy) still apply.

Calling an integration, an outside service such as a payment provider,
needs nothing extra: permission to run the action covers it.

### Lines that depend on a record

A user field such as `Todo.owner_id`, or a role held in a record such as
`CompanyRole.owner`, is checked on the records the action itself creates,
changes or deletes. On an action that changes none of them, such as a
report, it lets nobody in. The action is then checked as it starts against
the line's other entries: anyone signed in, or a role held across the
whole application. With none left, it refuses every caller, so its route
is `policy.effect_ungranted`.

## Field restrictions
<!-- topic: field-restrictions -->

A field follows its entity's grants unless it says otherwise. Use a
field's own `policy:` when some of a record's fields are more sensitive
than the rest, such as a project's budget. It has the same shape as the
entity's, and it can only take away: a field can't grant anyone more than
its entity does (`policy.field_widens_entity`). A field no client may
read or write says so by name.

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

Here viewers may read a project, but only editors may read or change its
`budget`. A field's `policy:` narrows `create`, `read` and `update`. It
has no `delete`, since a field is deleted with its record.

### A field no request may touch: `FieldAccess.server_only`

`FieldAccess.server_only`, as on `import_reference` above, lets no request
read or write the field. A test's or a seed's call without a caller runs
without policy, so it may. [Jobs and events](12-jobs-and-events.md) says
how schedules and subscribers write one.

### Writes

If a request changes a field the caller may not change, the whole write
is refused; nothing is dropped silently. A caller who may change the
record but not the field gets 403 `not_permitted`. A create that leaves an
optional field `none` sets nothing, so the field's policy never refuses
it.

### Outputs

An output type has one shape for everyone. If anyone allowed to call a
query can't read one of its output fields, the query doesn't compile
(`policy.output_field_unproved`). The fix is a second query with a
narrower output. The same holds for a record a query includes through a
reference: everyone who may read the record must be able to read what it
refers to.

Who may read a value is proved from the grants alone, not from the data.
Two records that each refer to a company may refer to different
companies, so a role held in one says nothing about the other.

### Webhook replies

A webhook's reply
([Webhooks that need a reply](11-integrations.md#webhooks-that-need-a-reply))
goes back to whoever sent the webhook, so it is an output to
`Access.public`. Every field it carries, or that decided it, must be one
anyone at all may read, though its action runs as the system.

The system finds every record its grants name. So a reply that counts or
tests for records shows whether they are there, and they must be records
anyone at all may read. A failure is answered to the sender too. So what
decides whether the action fails (a condition, a `get` or a change by
`id` finding nothing, a `conflict:`) and a failure's client fields are
held to the same proof.

### What else shows a field

A field's value can reach a caller in other ways than an output, and the
compiler checks each like an output:

- **Conditions and orders.** Which records pass a condition, and the order
  they come in, show the fields they test. So a query, `get`, include or
  write may filter or order on a field only when everyone who may run it
  may read that field. Otherwise `policy.output_field_unproved` names the
  condition or order. A transition's `when:` is a write's condition:
  refusing a record shows what it tests. A lifecycle's `visible:` is a
  condition on every read of its entity, so it tests only fields everyone
  the entity's `read:` lets in may read.
- **Copies.** Passing a value through a variable, another type, a
  function or the record a write stores it in doesn't escape the field's
  rules, because the compiler tracks where it came from. A migration's
  [computed backfill](05-data.md#computed-backfills) is a copy too: each
  field it reads must be one everyone who may read the field it fills may
  read.
- **What decides a value.** A value made under an `if`, a `match` or a
  loop, or kept by a filter, shows what they test. So it carries their
  fields' rules too. So do a count of what a filter kept, a failure raised
  on such a condition, and what comes after a `return` or `reject` it may
  have taken.
- **Finding records.** Which record a `get`, a query or a write finds
  shows the value it found it by, and an include shows the reference it
  follows. So each is proved like an output.
- **What a write gives back.** The record an update returns is proved
  against its entity's `read:` as well as its fields', since a caller may
  change a record they may not read. What its `set:` gave a field, and a
  created record's values, are the caller's own.
- **What a write stores.** A value stored in a field is read later, from
  any request, by everyone who may read that field. So a create or update
  stores a value in a field only when every one of them may read it,
  proved as an output is. Otherwise `policy.output_field_unproved` names
  the value in the write. A copy between fields the same callers read
  stays allowed: within a record, into a record that refers to the one it
  came from, or into one of the same tenant. Who reads a field is known
  only of the record it is stored in, so a copy into any other record
  proves only what holds of anyone signed in. A patch, `when_supplied:`, a
  counter and a transition store values too, and so does an action a
  value is passed to. What decided a value is stored with it. Since an
  action is all-or-nothing, so is whatever a `reject` in it tests.

### Events

A restricted field read only in an event isn't an output. An event's
subscribers are server work, so `emit` stores nothing a caller reads.
What a subscriber stores from an event is checked where it stores it
([Subscribers](12-jobs-and-events.md#subscribers)). What a log holds is
covered by log redaction in [Personal data](15-personal-data.md).

## The permissions report
<!-- topic: the-permissions-report -->

Policy is written beside each entity, so nobody has to keep a central
copy. To see the whole app at once, read the report `jadpo check` writes:
`build/report/permissions.md`, with a `.json` twin. It says who can do
what to every entity and field, and which actions are listed separately.
Each list is shown as written, and an effect nobody is granted says so.

Here is part of the report for [Policy, end to end](#policy-end-to-end):

| `work.Project` | create | read | update | delete |
|---|---|---|---|---|
| the record | `CompanyRole.owner`, `CompanyRole.editor` | `CompanyRole.owner`, `CompanyRole.editor`, `CompanyRole.viewer` | `CompanyRole.owner`, `CompanyRole.editor` | `CompanyRole.owner` |

Belongs to a `Company` through `company_id`. Only an action listed under `actions:` changes `company_id`.

A field that narrows its entity's grants gets a row of its own beside "the
record". For the `Project` under [Field restrictions](#field-restrictions):

| `work.Project` | create | read | update | delete |
|---|---|---|---|---|
| the record | `CompanyRole.editor` | `CompanyRole.viewer`, `CompanyRole.editor` | `CompanyRole.editor` | `CompanyRole.owner` |
| `budget` | as the record | `CompanyRole.editor` | `CompanyRole.editor` | as the record |
| `import_reference` | server only | server only | server only | as the record |

When a change alters this table, the approval review shows it in plain
terms, such as "viewers can now read `budget`"
([Review and sign-off](14-tooling.md#review-and-sign-off)).

Policy adds no index of its own. Every field a condition filters on is a
reference (a user field, a role's or the tenant's reference, a
membership's scope and member), and each already has its own
[index](05-data.md#indexes). Keeping a reference inside the tenant finds
the record it points at by `id`, its primary key. A move to another tenant
finds the records referring to it by their reference, which has its
index. So neither needs an index or a migration of its own.

## Protecting policy
<!-- topic: protecting-policy -->

An agent can edit policy, but the change can't ship without a person's
approval, wherever it is written. The compiler compares the whole
application before and after, because access also widens when an output
gains a field, a route becomes public or an action starts touching another
entity.

Each widening needs an acceptance: a file in `acceptances/` in which a
person accepts that change
([Acceptances](18-records-and-manifest.md#acceptances)). Breaking API
changes and database drops are accepted the same way.

Schedules and subscribers act as the system
([Background work](#background-work)), and what `Access.system` is granted
shows in the permissions report. The compiler's own purge of soft-deleted
records ([The purge schedule](12-jobs-and-events.md#the-purge-schedule)) is
the only operation outside policy.

Acceptances for a change that widens access aren't built yet
([What is built](#what-is-built)).

## When policy says no
<!-- topic: when-policy-says-no -->

Your code never handles a policy refusal itself. The compiler adds policy
to the SQL `WHERE` of every query and write, and indexes every field a
policy condition filters on. `jadpo sql` shows the policy each statement
carries. What a refused caller sees depends on what they asked for:

| Situation | The client sees |
|---|---|
| Not signed in, on a route that needs it | 401 `unauthenticated` ([Routes: signed in by default](08-identity.md#routes-signed-in-by-default)) |
| A record the caller may not see, read or changed by id | the code's own `not_found` failure, as if it did not exist |
| A list | only the records the caller may see, filtered before paging |
| A create, or an action they may not call | 403 `not_permitted` |

A hidden record reports the same `not_found` failure the code already
maps, so a caller can't tell it from one that doesn't exist. The log
records that policy concealed it
([Concealing records](04-failures.md#concealing-records)).

`not_permitted` is raised by the runtime and never listed in `fails`. The
generated API description lists the 403 wherever it can happen.

Writes are checked in the same statement as the change, so a membership
removed a moment earlier can't slip through.

A token issued to another application ([Identity](08-identity.md))
carries scopes, which say what that application may do. Scopes only
narrow: the token may do what both the person's grants and its scopes
allow.

### Policy, end to end

A whole application whose customers are companies. Members sign up, found
a company and give each other roles in it, and a company's projects are
seen and changed only by its members.

First, the principal, its one sign-in method, and root `policy.jadpo`.
The policy names the tenant, and the one action that needs other people
than its entities' grants: founding a company, since a company with no
members yet has no owner to grant anything:

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

A member may read only their own record, and anyone may sign up. Members
are shared by every company. The membership says where its role is held,
who holds it and which field holds it. A company's records follow the
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
of a membership, so only the company's owners may do it. The code says
nothing about who may do either:

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
caller, which runs them without policy. `found_company` takes its founder,
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

A [cache](05-data.md#caches) never lets a caller around policy. A record
cache applies policy on every read. A query-result cache is keyed by the
tenant and every input policy used. Policy itself always reads the
database.

[Personal data](15-personal-data.md) takes its permissions from policy:
erasing a person's data needs `delete` on their record, and exporting it
needs `read`.

## What is built
<!-- topic: policy-built -->

Everything on this page is built except the two features below. Each is
`toolchain.not_built_yet` where you write it.

| Feature | Status |
|---|---|
| Grants, user fields, membership roles, `Access.authenticated`, `Access.public` and `Access.system`, tenancy, `actions:`, field restrictions, the permissions report, and the refusals under "When policy says no" | Built. |
| `scopes:`, with the tokens issued to other applications (`oauth_token`, `external_token`) | Not built yet. Browser sessions and service keys work without them. |
| An acceptance for a change that widens access, and the approval review that shows the report's changes ([Review and sign-off](14-tooling.md#review-and-sign-off), [Acceptances](18-records-and-manifest.md#acceptances)) | Not built yet. Review policy changes in the permissions report's diff instead. |

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
