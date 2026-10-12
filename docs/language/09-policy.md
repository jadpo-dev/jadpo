---
title: Policy
area: 9
status: decided
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - policy.*
  - type.field_update_forbidden
---

# Policy

Who may do what to which records: grants on entities and fields, tenancy,
actions that need their own permission, the generated permissions report, and
what a caller sees when policy says no.

> Decided on 2026-10-12. Code outside the collapsed boxes applies the decisions from areas 1 to 9. Today's compiled syntax is collapsed beneath each section that changes.

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

"Who" is always one of three things:

| Who | Written | Means |
|---|---|---|
| A user field on the record | `owner_id` | the user that field points to |
| A membership role | `CompanyRole.editor` | a user with that role in the record's company |
| Everyone | `Access.authenticated`, `Access.public` | anyone signed in; anyone at all |

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

::: today

Grants are keyed by role, `TodoRole.owner: [create, read, update, delete]`,
and each role needs an enum and a field or membership that gives it out
(`policy.role_unbound`). Policy only starts once something in the project
declares one, so a project with none compiles every read and write
unprotected.

:::

## A user field on the record

Naming a reference to the user entity, such as `owner_id`, grants the user it
points to. Creating checks the new record, so `create: [owner_id]` means a
caller can only create a todo they own; a request naming someone else is
refused.

A user field named anywhere in a policy cannot be changed by an ordinary
update, so it needs no `immutable:`. Handing a record to someone else is a
named action listed in root `policy.jadpo` (see Actions).

::: today

The field carries a role, and `immutable: true` must be written, the only
value allowed (`policy.role_field_update_forbidden` without it,
`policy.binding_mutable` with `false`):

```text
owner_id: User.id {
    role: TodoRole.owner
    immutable: true
}
```

:::

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

::: today

The same, with role enums declared as `enum`, no colons, and an owner role
given out by a field:

```jadpo project=today-membership
enum CompanyRole {
    owner
    editor
}

entity User {
    id: Uuid identity
}

entity Company {
    id: Uuid identity
    owner_id: User.id {
        role: CompanyRole.owner
        immutable: true
    }

    policy {
        CompanyRole.owner: [read, update]
        CompanyRole.editor: [read]
    }
}

entity CompanyMembership {
    id: Uuid identity
    company_id: Company.id
    user_id: User.id
    role: CompanyRole

    membership {
        scope: company_id
        member: user_id
        role: role
    }

    policy {
        CompanyRole.owner: [create, read, update, delete]
    }
}
```

:::

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

::: today

There is no tenancy declaration; each entity is scoped only through the roles
its grants name. The golden todo `policy.jadpo` is design-only: a second policy
language of sentences (`read when principal.user.user_id == User.id`) that
does not compile (`syntax.expected_declaration`). It is retired.

:::

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

::: today

A non-entity action states `policy { Role: [invoke] }` as the first item in
its body, and an exception sits in the entity's `policy { operations { } }`
block: two places for one idea, the first a setting area 3 removed.

```jadpo project=today-invoke
output Health {
    ok: Bool
}

action health_report() -> Health {
    policy {
        Access.authenticated: [invoke]
    }
    return Health { ok: true }
}
```

:::

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
  whole write is refused; nothing is dropped silently.
- **Outputs.** An output type has one shape for everyone. If anyone allowed
  to call a query cannot read one of its output fields, the query does not
  compile (`policy.output_field_unproved`); the fix is a second query with a
  narrower output.
- **Copies.** Passing a value through a variable or another type does not
  escape the field's rules; the compiler tracks where it came from.

::: today

`policy {}` with nothing inside marks a server-only field:

```jadpo project=today-fields
enum UserRole {
    self
}

entity User {
    id: Uuid {
        role: UserRole.self
        immutable: true
    }
    email: Text {
        policy {}
    }
    display_name: Text

    identity: id

    policy {
        UserRole.self: [read]
    }
}
```

:::

## The permissions report

Policy is written beside each entity, so nobody has to keep a central copy.
The compiler writes the global view, `build/report/permissions.md` with a
`.json` twin: who can do what to every entity and field, and which actions
are listed separately.

Part of the report for `Project`, with `CompanyRole` shortened:

| `projects.Project` | create | read | update | delete |
|---|---|---|---|---|
| the record | editor | viewer, editor | editor | owner |
| `budget` | | editor | editor | |
| `import_reference` | | server only | server only | |

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

::: today

`build/approval/subject.json` lists every grant, binding, scope and
reachable effect, and `jadpo approval --before` compares two versions.
Nothing stops an unapproved release.

:::

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

::: today

Concealment and refusals work (read from generated code), but nothing is
logged: a concealed record and a missing one leave the same trace. The golden
todo actions still filter by owner by hand, repeating what policy adds:

```text
UPDATE "todo" SET ... WHERE "todo"."id" = $1 AND ("todo"."owner_id" = $2)
```

:::

## Caches and personal data

Decided in area 5: a record cache applies policy on every read, a
query-result cache is keyed by the tenant and every input policy used, and
policy itself always reads the database.

Area 15 owns personal data. Policy supplies its permissions: erasing a
person's data needs `delete` on their record and exporting it needs `read`.

## Errors

> The examples in this section use today's syntax, because they show the compiler's real output; they are rewritten as the rework lands. Under the decided model, `policy.binding_duplicate`, `policy.binding_mutable`, `policy.role_field_update_forbidden`, `policy.invoke_context`, `policy.operation_unknown` and `policy.operation_effect_mismatch` disappear; the rest keep their meaning in the new vocabulary.

Every error this area can produce, with an example that triggers it and the
compiler's actual output.

### Owner fields and memberships

### `policy.role_unbound`

A role used in a grant must have a field or membership that gives it out; declaring the enum is not enough:

```jadpo project=e-role-unbound expect=policy.role_unbound
enum NoteRole {
    owner
}

entity User {
    id: Uuid identity
}

entity Note {
    id: Uuid identity

    policy {
        NoteRole.owner: [read]
    }
}
```

### `policy.binding_invalid`

A role field must reference a principal entity:

```jadpo project=e-binding-invalid expect=policy.binding_invalid
enum NoteRole {
    owner
}

entity User {
    id: Uuid identity
}

entity Note {
    id: Uuid identity
    owner_id: Text {
        role: NoteRole.owner
        immutable: true
    }

    policy {
        NoteRole.owner: [read]
    }
}
```

### `policy.binding_duplicate`

```jadpo project=e-binding-duplicate expect=policy.binding_duplicate
enum NoteRole {
    owner
}

entity User {
    id: Uuid identity
}

entity Note {
    id: Uuid identity
    owner_id: User.id {
        role: NoteRole.owner
        role: NoteRole.owner
        immutable: true
    }

    policy {
        NoteRole.owner: [read]
    }
}
```

### `policy.binding_mutable`

```jadpo project=e-binding-mutable expect=policy.binding_mutable
enum NoteRole {
    owner
}

entity User {
    id: Uuid identity
}

entity Note {
    id: Uuid identity
    owner_id: User.id {
        role: NoteRole.owner
        immutable: false
    }

    policy {
        NoteRole.owner: [read]
    }
}
```

### `policy.role_field_update_forbidden`

A role field that could be changed by an ordinary update:

```jadpo project=e-role-field-update-forbidden expect=policy.role_field_update_forbidden
enum NoteRole {
    owner
}

entity User {
    id: Uuid identity
}

entity Note {
    id: Uuid identity
    owner_id: User.id {
        role: NoteRole.owner
    }

    policy {
        NoteRole.owner: [read]
    }
}
```

### `policy.role_field_create_forbidden`

The owner comes from the request instead of the signed-in principal:

```jadpo project=e-role-field-create-forbidden expect=policy.role_field_create_forbidden
enum NoteRole {
    owner
}

entity User {
    id: Uuid identity
}

entity PrivateNote {
    id: Uuid identity
    owner_id: User.id {
        role: NoteRole.owner
        immutable: true
    }
    title: Text

    policy {
        NoteRole.owner: [create, read, update, delete]
    }
}

input CreatePrivateNote {
    id: PrivateNote.id
    owner_id: PrivateNote.owner_id
    title: PrivateNote.title
}

action create_private_note(input: CreatePrivateNote) -> PrivateNote {
    return create PrivateNote {
        id: input.id
        owner_id: input.owner_id
        title: input.title
    }
}
```

### `policy.membership_scope_required`

```jadpo project=e-membership-scope-required expect=policy.membership_scope_required
enum AppRole {
    member
}

entity User {
    id: Uuid identity
}

entity Membership {
    id: Uuid identity
    user_id: User.id
    role: AppRole

    membership {
        member: user_id
        role: role
    }
}
```

### `policy.membership_member_required`

```jadpo project=e-membership-member-required expect=policy.membership_member_required
enum AppRole {
    member
}

entity User {
    id: Uuid identity
}

entity Membership {
    id: Uuid identity
    user_id: User.id
    role: AppRole

    membership {
        scope: application
        role: role
    }
}
```

### `policy.membership_role_required`

```jadpo project=e-membership-role-required expect=policy.membership_role_required
enum AppRole {
    member
}

entity User {
    id: Uuid identity
}

entity Membership {
    id: Uuid identity
    user_id: User.id
    role: AppRole

    membership {
        scope: application
        member: user_id
    }
}
```

### `policy.membership_duplicate`

```jadpo project=e-membership-duplicate expect=policy.membership_duplicate
enum AppRole {
    member
}

entity User {
    id: Uuid identity
}

entity Membership {
    id: Uuid identity
    user_id: User.id
    role: AppRole

    membership {
        scope: application
        member: user_id
        role: role
    }

    membership {
        scope: application
        member: user_id
        role: role
    }
}
```

### Scope

### `policy.scope_missing`

`Attachment` uses a note role but has no reference to `Note`:

```jadpo project=e-scope-missing expect=policy.scope_missing
enum NoteRole {
    owner
}

entity User {
    id: Uuid identity
}

entity Note {
    id: Uuid identity
    owner_id: User.id {
        role: NoteRole.owner
        immutable: true
    }

    policy {
        NoteRole.owner: [read]
    }
}

entity Attachment {
    id: Uuid identity

    policy {
        NoteRole.owner: [read]
    }
}
```

### `policy.scope_ambiguous`

Two references to `Company`, and the policy does not say which one decides:

```jadpo project=e-scope-ambiguous expect=policy.scope_ambiguous
enum CompanyRole {
    owner
}

entity User {
    id: Uuid identity
}

entity Company {
    id: Uuid identity
    owner_id: User.id {
        role: CompanyRole.owner
        immutable: true
    }

    policy {
        CompanyRole.owner: [read]
    }
}

entity InvoiceTransfer {
    id: Uuid identity
    source_company_id: Company.id
    destination_company_id: Company.id

    policy {
        CompanyRole.owner: [read]
    }
}
```

### `policy.scope_duplicate`

```jadpo project=e-scope-duplicate expect=policy.scope_duplicate
enum NoteRole {
    owner
}

entity User {
    id: Uuid identity
}

entity Note {
    id: Uuid identity
    owner_id: User.id {
        role: NoteRole.owner
        immutable: true
    }

    policy {
        scope: owner_id
        scope: owner_id
        NoteRole.owner: [read]
    }
}
```

### Grants

### `policy.effect_ungranted`

Once any policy exists, an action that touches no entity needs its own grant:

```jadpo project=e-effect-ungranted expect=policy.effect_ungranted
entity Article {
    id: Uuid identity

    policy {
        Access.authenticated: [read]
    }
}

output Health {
    ok: Bool
}

action administrative_health() -> Health {
    return Health { ok: true }
}
```

### `policy.effect_unknown`

```jadpo project=e-effect-unknown expect=policy.effect_unknown
enum NoteRole {
    owner
}

entity User {
    id: Uuid identity
}

entity Note {
    id: Uuid identity
    owner_id: User.id {
        role: NoteRole.owner
        immutable: true
    }

    policy {
        NoteRole.owner: [publish]
    }
}
```

### `policy.effect_duplicate`

```jadpo project=e-effect-duplicate expect=policy.effect_duplicate
enum NoteRole {
    owner
}

entity User {
    id: Uuid identity
}

entity Note {
    id: Uuid identity
    owner_id: User.id {
        role: NoteRole.owner
        immutable: true
    }

    policy {
        NoteRole.owner: [read, read]
    }
}
```

### `policy.subject_duplicate`

One line per role:

```jadpo project=e-subject-duplicate expect=policy.subject_duplicate
enum NoteRole {
    owner
}

entity User {
    id: Uuid identity
}

entity Note {
    id: Uuid identity
    owner_id: User.id {
        role: NoteRole.owner
        immutable: true
    }

    policy {
        NoteRole.owner: [read]
        NoteRole.owner: [update]
    }
}
```

### `policy.declaration_duplicate`

```jadpo project=e-declaration-duplicate expect=policy.declaration_duplicate
enum NoteRole {
    owner
}

entity User {
    id: Uuid identity
}

entity Note {
    id: Uuid identity
    owner_id: User.id {
        role: NoteRole.owner
        immutable: true
    }

    policy {
        NoteRole.owner: [read]
    }

    policy {
        NoteRole.owner: [update]
    }
}
```

### `policy.invoke_context`

An action's grant can only be `invoke`:

```jadpo project=e-invoke-context expect=policy.invoke_context
output Health {
    ok: Bool
}

action public_health() -> Health {
    policy {
        Access.public: [read]
    }
    return Health { ok: true }
}
```

### `policy.public_grant_missing`

A route that needs no sign-in reaches an entity that does not grant `Access.public`:

```jadpo project=e-public-grant-missing expect=policy.public_grant_missing
entity Article {
    id: Uuid
    title: Text
    identity: id

    persistence {
        store: primary
        role: authority
    }

    policy {
        Access.authenticated: [read]
    }

    query by_id(id: Article.id)
        freshness: authoritative
        -> Article?
    {
        return attempt query optional Article {
            where: id == id
        }
    }
}

route GET /articles/{article_id} {
    auth: none
    path: {
        article_id: Article.id
    }
    output: Article?
    run: Article.by_id(path.article_id)
}
```

### Action exceptions

### `policy.operation_unknown`

```jadpo project=e-operation-unknown expect=policy.operation_unknown
enum NoteRole {
    owner
}

entity User {
    id: Uuid identity
}

entity Note {
    id: Uuid identity
    owner_id: User.id {
        role: NoteRole.owner
        immutable: true
    }

    policy {
        NoteRole.owner: [read]

        operations {
            archive {
                NoteRole.owner: [update]
            }
        }
    }
}
```

### `policy.operation_effect_mismatch`

`label` touches no data, so there is nothing for the exception to replace:

```jadpo project=e-operation-effect-mismatch expect=policy.operation_effect_mismatch
type Label = Text {}

enum NoteRole {
    owner
}

entity User {
    id: Uuid identity
}

entity Note {
    id: Uuid identity
    owner_id: User.id {
        role: NoteRole.owner
        immutable: true
    }

    function label() -> Label {
        return Label("label")
    }

    policy {
        NoteRole.owner: [read]

        operations {
            label {
                NoteRole.owner: [read]
            }
        }
    }
}
```

### Fields

### `policy.field_widens_entity`

A field cannot grant more than its entity does:

```jadpo project=e-field-widens-entity expect=policy.field_widens_entity
enum NoteRole {
    owner
}

entity User {
    id: Uuid identity
}

entity PrivateNote {
    id: Uuid identity
    owner_id: User.id {
        role: NoteRole.owner
        immutable: true
    }
    private_label: Text {
        policy {
            NoteRole.owner: [read, update]
        }
    }

    policy {
        NoteRole.owner: [read]
    }
}
```

### `policy.output_field_unproved`

Every signed-in caller may read `Note`, but only its owner may read `private_label`, so a query returning the whole `Note` is refused:

```jadpo project=e-output-field-unproved expect=policy.output_field_unproved
enum NoteRole {
    owner
}

entity User {
    id: Uuid identity
}

failure NoteNotFound {
    kind: NotFound
    code: "note_not_found"
}

entity Note {
    id: Uuid identity
    owner_id: User.id {
        role: NoteRole.owner
        immutable: true
    }
    private_label: Text {
        policy {
            NoteRole.owner: [read]
        }
    }

    policy {
        Access.authenticated: [read]
        NoteRole.owner: [read]
    }

    query by_id(id: Note.id)
        freshness: authoritative
        fails NoteNotFound
        -> Note
    {
        return attempt query required Note {
            where: id == id
            missing: NoteNotFound
        }
    }
}
```

### `type.field_update_forbidden`

```jadpo project=e-field-update-forbidden expect=type.field_update_forbidden
entity Article {
    id: Uuid identity
    company_id: Uuid {
        immutable: true
    }
    title: Text
}

failure ArticleNotFound {
    kind: NotFound
    code: "article_not_found"
}

failure ArticleConflict {
    kind: Conflict
    code: "article_conflict"
}

action move_article(id: Article.id, company_id: Article.company_id)
    fails ArticleNotFound, ArticleConflict
    -> Article
{
    return attempt update required Article {
        where: id == id
        set: { company_id: company_id }
        missing: ArticleNotFound
        conflict: ArticleConflict
    }
}
```

## Review

### Status

Each row says how it was checked: compiled with `jadpo check`, or read from
the code generated for `examples/golden-todo-migration` and
`examples/policy-runtime`.

| Feature | Status |
|---|---|
| Roles, owner fields, memberships, grants | Work in today's role-keyed form, with the errors above (compiled). `Access.public` and `Access.authenticated` work. The decided model is not built. |
| Policy on by default | **Gap (compiled):** policy only starts once something declares one. A project with no policy compiles a public `GET /notes/{id}` route that reads any note; adding one `policy` anywhere makes the same read `policy.effect_ungranted`. |
| Scoping reads and writes | Works: the owner or membership condition is added to the SQL of reads, updates and deletes, and a create is checked against the new record (read from generated code). |
| Hand-written filters | The golden todo actions take `principal: Principal.user` and filter `where: owner_id == Todo.owner_id(principal.user_id)` by hand, repeating policy (read from source). |
| Concealment | A hidden record fails with the query's `missing:` failure (read from generated code). **Gap:** nothing is logged. |
| Refusals | 403 `not_permitted`, "This operation is not permitted.", for a refused create or action (read from generated code). |
| Fields | Narrowing, server-only (`policy {}`), whole-write refusal and output checks work (compiled). |
| Action grants | Two places: `policy { Role: [invoke] }` inside an action's body, and `operations { }` inside an entity's policy (compiled). |
| Root `policy.jadpo` | Design-only sentence language; `syntax.expected_declaration` (compiled). No tenancy declaration exists. |
| Permissions report | Not built. `build/approval/subject.json` lists grants for the approval review; nothing blocks a release. |

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
| Event and subscriber policy (today's candidate `policy { scope: f  Role: [invoke, read] }`, syntax review #52) | 12 | Input to area 12 |
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
   `authentication.jadpo`. A caller whose token carries scopes needs both a
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
