---
title: Policy
area: 9
status: in-review
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - policy.*
  - type.field_update_forbidden
---

# Policy

Who may do what to which records: roles, where a role comes from, what each
role may do to an entity and its fields, who may call an action, tenancy, and
what a caller sees when policy says no.

> In review. Code outside the collapsed boxes applies the decisions from areas 1 to 5 and this page's recommendations; each recommendation names its open question. Today's compiled syntax is collapsed beneath each section that changes.

The rule behind all of it: **policy is written once, beside the data it
protects, and the compiler applies it to every read and write.** Code never
checks permissions by hand and never filters by owner or tenant itself.

| Concept | What it says | Where it is written |
|---|---|---|
| Role | A named kind of authority, such as `CompanyRole.editor` | an `Enum` in `types/` |
| Role source | Who holds a role: an owner field, or a membership record | the entity's file |
| Grant | What a role may do to an entity: `create`, `read`, `update`, `delete` | the entity's `policy:` |
| Field restriction | A field only some of those roles may read or change | the field's block |
| Application policy | Tenancy and grants for actions that touch no entity | root `policy.jadpo` |

## Roles

A role is a variant of an `Enum`, always written with its type:
`CompanyRole.editor`, never `editor`. The type names where the role means
something: you are an editor *of a company*. A role grants nothing until an
entity's policy says what it may do.

```jadpo fragment
// domains/accounts/types/company_role.jadpo
type CompanyRole = Enum {
    owner
    editor
    viewer
}
```

Two built-in subjects need no role: `Access.authenticated` is anyone signed
in, and `Access.public` is anyone at all (the route must also say
`auth: none`, area 7). There is no role hierarchy and no administrator who
bypasses policy: an owner who may also edit is listed under both.

::: today

Roles are `enum TodoRole { owner }` declarations.

:::

## Where a role comes from

Roles come only from data the application stores, never from a token, a
header or request input. There are two sources.

**An owner field.** `role:` in a reference field's block says that the user it
points to holds that role for this record. A role field cannot be changed by
an ordinary update, so it needs no `immutable:` (question 5); moving
ownership is a named action with its own grant.

```jadpo fragment
entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: restrict
        role: TodoRole.owner
    }
    title: TodoTitle

    policy: {
        TodoRole.owner: [create, read, update, delete]
    }
}
```

**A membership record.** An entity with a `membership:` section gives its
`member` the stored `role` within its `scope`: a company, or `application`
for roles that hold everywhere. Only current records count: a membership the
lifecycle hides (expired, removed) grants nothing, and removing it takes
effect on the next request.

```jadpo fragment
entity CompanyMembership {
    id: Uuid
    company_id: Company.id {
        on_delete: cascade
    }
    user_id: User.id {
        on_delete: cascade
    }
    role: CompanyRole

    membership: {
        scope: company_id
        member: user_id
        role: role
    }

    policy: {
        CompanyRole.owner: [create, read, update, delete]
    }
}
```

::: today

The same, without colons, and `immutable: true` must be written on every role
field (`policy.role_field_update_forbidden` without it,
`policy.binding_mutable` with `false`):

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

## Granting access to an entity

An entity's `policy:` lists each role with what it may do. The four effects
are `create`, `read`, `update` and `delete`; deleting includes a lifecycle
delete such as a soft delete, and `update` cannot set `deleted_at` itself.
Anything not listed is refused. Several roles on one entity are alternatives:
any one of them is enough.

```jadpo fragment
entity Article {
    id: Uuid
    company_id: Company.id {
        on_delete: cascade
    }
    title: ArticleTitle

    policy: {
        CompanyRole.owner: [create, read, update, delete]
        CompanyRole.editor: [read, update]
        CompanyRole.viewer: [read]
    }
}
```

Nobody writes which effect an action needs. The compiler sees what each query
and action reads and writes, and every one of them that updates an `Article`
needs `update` on it. Ten actions that edit articles share one line of policy.
An action that reaches several entities needs a grant on each; a call to
another domain's action carries the caller's identity and that action's own
policy applies.

Policy is always on (question 2). Today it only starts once something in the
project declares a policy, so a project with none compiles every read and
write unprotected, including from a public route.

## Which records a caller sees

The compiler adds policy to every query and write, so a caller only ever
touches records their roles cover. The read below is all an author writes
(its syntax is area 6); the owner or membership condition is added to its
SQL `WHERE`:

```jadpo fragment
var article: Article = attempt query required Article {
    where: Article.id == article_id    // policy adds: and the caller holds a role in its company
    missing: ArticleNotFound {
        article_id: article_id
    }
}
```

- **Scope.** An entity using `CompanyRole` must reach `Company` through
  exactly one reference, which the compiler finds itself (`Article.company_id`
  above). With two references to `Company`, `scope: source_company_id` in the
  `policy:` section picks one.
- **Lists** leave out records the caller may not see, before paging, so page
  sizes and counts never reveal them.
- **Single records** the caller may not see fail with the query's own
  `missing:` failure, exactly like a record that does not exist (area 4).
- **Creating** checks the new record before it is saved: an article in a
  company where the caller has no role is refused.
- **Writes** check in the same statement as the change, so a membership
  removed a moment earlier cannot slip through.
- **Children** are checked on their own: reading a company does not grant
  reading its articles.

::: today

This works (read from generated code). The golden todo application still
passes the principal into every action and filters by owner by hand,
`where: owner_id == Todo.owner_id(principal.user_id)`, which repeats what
policy already adds. The generated condition for `Todo` is:

```text
UPDATE "todo" SET ... WHERE "todo"."id" = $1 AND ("todo"."owner_id" = $2)
```

:::

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

Every entity must then reach `Company` through exactly one reference, and
every grant on it, `Access.authenticated` included, applies only within
companies where the caller holds a role. An entity that is shared by all
tenants (a list of countries, the `User` record itself) says
`tenant: none` in its `policy:`, which shows in the approval review. A new
entity an agent adds is tenant-scoped unless someone approves otherwise.

::: today

There is no tenancy declaration; each entity is scoped only through the roles
its grants name. The golden todo `policy.jadpo` is design-only: a second policy
language of sentences (`read when principal.user.user_id == User.id`) that
does not compile (`syntax.expected_declaration`). It is retired.

:::

## Fields

A field follows its entity's policy unless its block narrows it. A field
restriction can only take away: it cannot give a role an effect the entity
does not (`policy.field_widens_entity`). A field no client may read or write
says so by name (question 6):

```jadpo fragment
entity Article {
    ...
    billing_reference: Text? {
        policy: {
            CompanyRole.owner: [read, update]     // editors may update the article, not this
        }
    }
    authentication_subject: Text {
        policy: server_only                       // set by the server, never read or written by a client
    }
}
```

- **Writes.** If a request changes a field the caller may not change, the
  whole write is refused; nothing is dropped silently and nothing partial is
  saved. `immutable: true` fields, generated fields and lifecycle fields are
  never caller-writable, whatever the grant (area 5).
- **Outputs.** An output type has one shape for everyone. If any role allowed
  to call a query cannot read one of its output fields, the query does not
  compile (`policy.output_field_unproved`); the fix is a second query and
  output for the narrower audience, such as `ArticleBillingView`.
- **Copies.** Passing a value through a variable, an input or a wider type
  does not escape the field's rules; the compiler tracks where it came from.

::: today

`policy {}` with nothing inside marks a server-only field, the same block
that elsewhere lists roles:

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

## Who may call an action

Decided in area 3: an action has no settings, and who may call it is policy.
Most actions need nothing: their permission comes from the entities they
touch. Two cases need a grant, and both go in root `policy.jadpo` under
`actions:` (question 4):

- an action that touches no entity, such as an operator's health report or a
  data export, which is otherwise refused;
- an action that needs different roles from its entity's grants, such as
  `disable_user`, which a user may run on their own account although they may
  not otherwise update it. These are rare and always reviewed.

```jadpo fragment
// policy.jadpo
policy {
    tenant: Company
    actions: {
        monitoring.administrative_health: [ApplicationRole.operator]
        accounts.disable_user: [UserRole.self]
    }
}
```

The roles listed may call the action, and for that action they replace the
entity grants on what it reaches; field rules and scope still apply. A route
with `auth: none` may call an action only when everything it reaches allows
`Access.public`.

::: today

A non-entity action states `policy { Role: [invoke] }` as the first item in its
body, and an exception sits in the entity's `policy { operations { } }`
block. These are two places for one idea, and the first is a setting area 3
removed:

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

## When policy says no

| Situation | The client sees |
|---|---|
| Not signed in, on a route that needs it | 401 `unauthenticated` (area 8) |
| A record the caller may not see, read or changed by id | the code's own `not_found` failure, as if it did not exist |
| A list | the records the caller may see |
| A create, or an action they may not call | 403 `not_permitted` |

Decided in area 4: a hidden record reports the same `not_found` failure the
code already maps, and the log records that policy concealed it. A refusal
that reveals no record is raised by the runtime with the fixed code
`not_permitted`; it is never listed in `fails` (question 7), and the generated
API description lists the 403 for every route that can reach it.

::: today

Both work (read from generated code), but nothing is logged: a concealed
record and a missing one leave the same trace.

:::

## Caches

Decided in area 5: caches hold copies for display reads only. A record cache
holds records before policy and applies policy on every read, exactly as the
database does; a query-result cache is keyed by the tenant and by every input
policy used, so one caller's results never reach another. Policy itself, role
lookups included, always reads the database, so a removed membership takes
effect at once.

## Personal data

GDPR support is its own future review (area 5). Policy needs one fact from
it: which fields hold personal data, marked in the field's block. Policy then
keeps those values out of logs and failure details (area 4 hashes text
today) and out of any output not covered by a grant, and erasure can find
every copy. The marking itself is designed in that review.

## Protecting policy

Decided in area 1: an agent can edit policy, but the change cannot ship
without human approval, wherever the policy is written. The compiler compares
the whole application before and after, not just `policy:` blocks, because
access also widens when an output gains a field, an input reaches a new
field, a route becomes public or an action starts touching another entity.
Each such change is listed for a person to accept, in plain terms ("viewers
can now read `billing_reference`").

Schedules and subscribers act as a declared service principal with its own
roles (areas 8 and 12); being internal bypasses nothing. The one exception is
the compiler's own purge of soft-deleted records past their retention (area
5), which no application code can call.

::: today

`build/approval/subject.json` lists every grant, binding, scope and
reachable effect, and `jadpo approval --before` compares two versions. The
protected sign-off that would stop an unapproved release is not built.

:::

## Errors

> The examples in this section use today's syntax, because they show the compiler's real output; they are rewritten as the rework lands.

Every error this area can produce, with an example that triggers it and the
compiler's actual output.

### Where roles come from

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

A route with `auth: none` reaches an entity that does not grant `Access.public`:

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
| Roles, owner fields, memberships, grants | Work, with the errors below (compiled). `Access.public` and `Access.authenticated` work. |
| Policy on by default | **Gap (compiled):** policy only starts once something declares one. A project with no policy compiles a `GET /notes/{id}` route with `auth: none` that reads any note; adding one `policy` anywhere makes the same read `policy.effect_ungranted`. |
| Scoping reads and writes | Works: the owner or membership condition is added to the SQL of reads, updates and deletes, and a create is checked against the new record (read from generated code). An entity with no grants is refused every create. |
| Hand-written filters | The golden todo actions take `principal: Principal.user` and filter `where: owner_id == Todo.owner_id(principal.user_id)` by hand, repeating policy (read from source). |
| Concealment | A hidden record fails with the query's `missing:` failure, the same as absence (read from generated code). **Gap:** nothing is logged, so concealment cannot be told from absence afterwards. |
| Refusals | 403 `not_permitted`, "This operation is not permitted.", for a refused create or action (read from generated code). |
| Fields | Narrowing, server-only (`policy {}`), whole-write refusal and output checks work (compiled; `policy.output_field_unproved`). |
| Role fields | Must say `immutable: true`, the only value allowed (compiled). |
| Action grants | Two places: `policy { Role: [invoke] }` inside an action's body, and `operations { }` inside an entity's policy (compiled). |
| Root `policy.jadpo` | Design-only sentence language; `syntax.expected_declaration` (compiled). No tenancy declaration exists. |
| Approval | `build/approval/subject.json` and `jadpo approval --before` describe policy changes; the protected sign-off that blocks a release is not built (approval protocol). |
| Caches, personal data | Nothing to check: caches were removed in area 5, and there is no personal-data marking. |

### Consistency with the rest of the language

- **Colon rule.** `policy { }`, `membership { }` and `operations { }` lack
  the colon area 3 decided for sections.
- **An action setting survives.** `policy { }` as the first item of an
  action body is a setting, which area 3 removed from actions.
- **Ritual line.** `immutable: true` on a role field restates the only value
  allowed (syntax review §4, "settings with one legal value").
- **An empty block means something.** `policy {}` on a field means "no
  client", while an absent block means "as the entity" (syntax review #51).
- **Two effect vocabularies.** Entities use `create`, `read`, `update`,
  `delete`; actions use `invoke`. The proposed `actions:` grants list roles
  only, so `invoke` goes.
- **`scope:` beside grants.** `scope: source_company_id` sits in the same
  block as role lines; it is a different kind of member, though the qualified
  role names keep them apart.
- **`role` means two things.** A field setting (`role: CompanyRole.owner`)
  and a membership's stored field (`role: role`); with `persistence` gone
  (area 5) these are the only two.
- **Policy repeated in code.** Hand-written owner filters and principal
  parameters duplicate what policy adds, and drift from it when policy
  changes.

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| How an action reaches the signed-in principal without a `principal` parameter; policy never needs it passed | 8 | Input to area 8 |
| `auth: none` and fresh sign-in on routes; a public route needs both `auth: none` and `Access.public` | 7, 8 | Input to areas 7 and 8 |
| The service principal and roles that schedules and subscribers act as | 8, 12 | Input to areas 8 and 12 |
| Event and subscriber policy (today's candidate `policy { scope: f  Role: [invoke, read] }`, syntax review #52) | 12 | Input to area 12 |
| Includes apply each child's own policy; children of a soft-deleted parent are a lifecycle question, not policy (area 5 carried it to 6 and 9) | 6 | Input to area 6 |
| Generated query plans, indexes for policy conditions, and PostgreSQL row-level security as optional defence in depth generated from the same policy | 6 | Input to area 6 |
| One "accept a reviewed change" mechanism for policy, public API, drops and domains (area 4 carried it to area 7) | 7 | Input to area 7 |
| Testing policy: calling an action as a given user and expecting `not_found` or `not_permitted` | 13 | Input to area 13 |
| The approval command, its report and the protected sign-off | 14 | Input to area 14 |
| Personal-data marking on fields, erasure, export, retention and log redaction (GDPR review) | 5, 9, 10 | Raised |

### Open questions

1. **Keep the role model, tidied, or move to rules written as conditions or
   as relationships?** Today's model has five concepts (role, role source,
   grant, field restriction, action grant) and the compiler proves every
   query and write against them. *Recommendation:* keep it, with the
   simplifications below; see
   [the proposals](research/09-policy-proposals.md). Condition rules
   (Pundit, Supabase row-level security) read naturally but let every rule
   differ; relationship models (OpenFGA, Oso) add inheritance between roles
   that most business applications do not need yet.
2. **Is policy always on?** Today a project with no policy has none.
   *Recommendation:* yes. Every reachable create, read, update or delete
   needs a grant from the first line of code, so an application cannot ship
   with policy forgotten. Ash and Supabase row-level security also refuse by
   default, but Supabase only once a table has it switched on, which is a
   known source of leaks.
3. **How is tenancy declared?** *Recommendation:* `tenant: Company` in root
   `policy.jadpo`. Every entity must then reach the tenant through one
   reference or say `tenant: none`, and every grant applies only inside the
   caller's tenants, `Access.authenticated` included. This is the one setting
   that stops a new entity leaking across customers. Rails' `acts_as_tenant`
   and Supabase policies do the same per model or table, where it can be
   forgotten.
4. **Where does permission to call an action live?** Area 3 removed action
   settings. *Recommendation:* every action takes its permission from the
   entities it touches; the few that touch none, or need different roles,
   are listed in root `policy.jadpo` under
   `actions: { domain.action: [Role, ...] }`. One place answers "who may call
   this?", and `invoke` and entity `operations { }` go. The alternative,
   keeping exceptions in the entity file, splits action grants across two
   kinds of file.
5. **Is a role field immutable without saying so?** *Recommendation:* yes.
   `role:` already makes the field unchangeable by ordinary updates; writing
   `immutable: true` adds nothing, and `immutable: false` is an error today.
   Transferring ownership is a named action granted in `actions:`.
6. **How is a field no client may touch written?** *Recommendation:*
   `policy: server_only`, a named option instead of an empty block, so "no
   access" never looks like an accident (syntax review #51).
7. **Is a policy refusal listed in `fails`?** *Recommendation:* no. Like
   `unauthenticated`, `not_permitted` from policy is raised by the runtime with
   a fixed code, and the API description lists the 403 wherever it can happen.
   Listing it would add the same name to most actions and say nothing a
   reader can act on. A business refusal an action raises itself is still an
   ordinary `kind: not_permitted` failure.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Policy protection | Carried from area 1: through the approval check wherever policy lives, not by file path | 2026-10-12 |
| Root file | Carried from area 1: `policy.jadpo` holds application-wide policy, such as tenancy; it is an unnamed singleton | 2026-10-12 |
| Action settings | Carried from area 3: actions have no settings; who may call an action is decided by policy | 2026-10-12 |
| Colon rule | Carried from area 3: sections are `name: { ... }` (`policy: { }`, `membership: { }`) | 2026-10-12 |
| Setting options and lists | Carried from area 2: closed options are lower_snake_case names; lists are `[ ]` | 2026-10-12 |
| Concealment | Carried from area 4: a record hidden by policy reports the same `not_found` failure; the log records the concealment | 2026-10-12 |
| Caches | Carried from area 5: caches serve display reads only; keys include tenant and policy inputs; reads inside policy never use a cache; erasure wipes cached copies | 2026-10-12 |
| Placement | Carried from areas 1 and 5: an entity's policy is in its file; actions and queries are in their own folders | 2026-10-12 |
| Policy model | | |
| Policy always on | | |
| Tenancy | | |
| Action permission | | |
| Role fields | | |
| Server-only fields | | |
| Refusals in `fails` | | |
