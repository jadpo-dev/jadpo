---
title: Data
area: 5
status: in-review
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - data.compound_constraint_fields
  - data.constraint_duplicate_field
  - data.constraint_nullable_field
  - data.constraint_unknown_field
  - data.delivery_mode_invalid
  - data.duplicate_constraint_shape
  - data.entity_not_persistent
  - data.identity_field_unknown
  - data.identity_nullable
  - data.inverse_duplicate_name
  - data.inverse_not_owning_reference
  - data.inverse_optional_not_unique
  - data.inverse_via_field
  - data.modifier_non_entity
  - data.multiple_identities
  - data.persistence_role_invalid
  - data.persistence_role_required
  - data.persistence_store_required
  - data.receiver_kind_invalid
  - data.relationship_cycle
  - data.relationship_set_null_required
  - data.relationship_target_field
  - data.relationship_target_not_key
  - data.relationship_type_mismatch
  - data.representation_authority_invalid
  - data.representation_duplicate
  - data.representation_setting_invalid
  - data.representation_setting_required
  - index.recommendation_available
  - migration.identity_duplicate_id
  - migration.identity_duplicate_path
  - migration.identity_physical_name
  - migration.identity_registry_drift
  - migration.identity_registry_invalid
  - migration.identity_registry_not_canonical
  - migration.identity_registry_owner
  - syntax.constraint_non_entity
  - syntax.duplicate_field_modifier
  - syntax.expected_inverse_cardinality
  - syntax.generated_*
  - syntax.inverse_non_entity
  - syntax.lifecycle_*
  - type.generated_*
  - type.lifecycle_*
---

# Data

How a Jadpo program declares the records it keeps: entities and their
identity, fields the compiler fills in, where records are stored, uniqueness
and indexes, relationships and what happens on delete, soft deletion and
purging, the operations an entity owns, how each type is stored in
PostgreSQL and SQLite, and how the schema changes over time through
migrations and seeds.

Reading and writing records (`query`, `create`, `update`, includes,
transactions) is area 6; who may read or change a record is area 9.

> In review. Code outside the collapsed boxes applies the decisions from areas 1 to 4 and this page's recommendations; each recommendation names its open question. Today's compiled syntax is collapsed beneath each section that changes.

## Entities

An **entity** is a record with a lasting identity that the application stores
and changes: a `Todo`, a `User`, an `Order`. Two todos with the same title are
still two todos. Everything else is a value (`type X = Object { ... }`, area
2): equal values are interchangeable and have no identity.

```jadpo fragment
/// Something a user has to do.
entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: restrict
    }
    title: TodoTitle
    status: TodoStatus
    due_at: Instant?
    created_at: Instant {
        generated: create
    }
    updated_at: Instant {
        generated: create_or_change
    }
    deleted_at: Instant?

    lifecycle: {
        initial: {
            deleted_at: none
        }
        visible: deleted_at == none
        transitions: {
            delete: {
                when: deleted_at == none
                set: {
                    deleted_at: clock.now
                }
            }
        }
        purge: {
            after: config.soft_delete_retention
            since: deleted_at
        }
    }

    policy: {
        TodoRole.owner: [create, read, update, delete]
    }

    action rename(todo_id: Todo.id, title: TodoTitle) fails TodoNotFound -> Todo {
        ...
    }
}
```

- An entity lives in its own file under its domain's `entities/` folder
  (decided in area 1), together with its actions, queries, policy and the
  types only it uses. It keeps the `entity` keyword because it has its own
  folder; anything in `types/` is a `type` (decided in area 4).
- Fields come first. Each field's own settings go in a block after its type,
  one per line (decided in area 2): `generated:`, `unique:`, `index:` and
  `on_delete:` all live there, so every fact about a field is in one place.
- Sections follow the colon rule (decided in area 3): `lifecycle: { }` and
  `policy: { }`. There is no `identity:` item and no `persistence` block
  (questions 1 and 2).
- An entity value is a complete, immutable snapshot. Reading a field never
  touches the database, and nothing is loaded or saved behind your back.

::: today

The identity is a separate `identity:` item, storage is a `persistence` block
that repeats the only values the compiler accepts, relationships are
sentences inside that block, and the lifecycle uses four different grammars:

```jadpo project=entity
config TodoConfiguration {
    soft_delete_retention: Duration {
        binding: "SOFT_DELETE_RETENTION"
        default: 30d
    }
}

type TodoTitle = Text {
    max_length: 200
}

enum TodoStatus {
    open
    done
}

failure TodoNotFound {
    kind: NotFound
    code: "todo_not_found"
    message: "Todo not found."
}

failure TodoConflict {
    kind: Conflict
    code: "todo_conflict"
    message: "The todo could not be changed."
}

entity User {
    id: Uuid
    email: Email

    identity: id

    persistence {
        store: primary
        role: authority
        unique: email
        inverse todos: many Todo via: Todo.owner_id
    }
}

entity Todo {
    id: Uuid { generated: identity }
    owner_id: User.id
    title: TodoTitle
    status: TodoStatus
    due_at: Instant?
    created_at: Instant generated { on: create }
    updated_at: Instant generated { on: create_or_change }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
        references owner_id: User.id as: owner on_delete: restrict
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
        purge after config.soft_delete_retention from deleted_at
    }

    action delete_todo(todo_id: Todo.id)
        fails TodoNotFound, TodoConflict
        -> Todo
    {
        return attempt update required Todo {
            where: id == todo_id
            transition: delete
            missing: TodoNotFound
            conflict: TodoConflict
        }
    }
}
```

The older `type X = Object { }` plus a separate `persist X { }` declaration
also still compiles (see `examples/persistence-seed`); it is removed
(syntax review #23).

:::

### Identity

Every entity is identified by one field called `id`, based on `Uuid`. The
compiler generates it as a version 7 UUID when the record is created, and
callers never supply it (question 1). A named type can pin another version,
as area 2 decided.

```jadpo fragment
entity Customer {
    id: Uuid                    // generated as a version 7 UUID on create
    email: Email {
        unique: true
    }
}

type PartnerRef = Uuid {
    version: v4
}

entity Partner {
    id: PartnerRef              // generated as version 4
    name: PartnerName
}

action register(email: Email) fails EmailTaken -> Customer {
    var customer: Customer = attempt create Customer {
        email: email            // no id: the compiler supplies it
        ...
    }
    return customer
}
```

A record that needs a second key, such as a join between two entities, has
its own `id` plus a compound unique rule (see
[Uniqueness and indexes](#uniqueness-and-indexes)). Other code refers to a
record by its id's type, `Customer.id`.

::: today

The identity is declared four ways, and generation is opt-in. Without
`{ generated: identity }`, every `create` must supply `id`; with it, the
generated code calls `crypto.randomUUID()`, which makes version 4 UUIDs (read
from generated code).

```jadpo project=identity
entity Customer {
    id: Uuid { generated: identity }
    email: Email

    identity: id

    persistence {
        store: primary
        role: authority
        unique: email
    }
}

entity Partner {
    id: Uuid identity
    name: Text
}
```

The other two spellings are `persist Partner { identity: id }` and the
trailing `identity` combined with the item. Whether an entity is stored at
all depends on which spelling it uses (see Status).

:::

### Fields the compiler fills in

```jadpo fragment
entity Note {
    id: Uuid
    body: NoteBody
    created_at: Instant {
        generated: create             // set once, when the record is created
    }
    updated_at: Instant {
        generated: create_or_change   // set on create and on every change
    }
}
```

- A generated field is a required `Instant`, filled from the operation's
  clock, and never assigned by code or accepted in an `Input` (decided in
  area 2).
- Entity fields have no `default:`; area 2 limited `optional` and `default`
  to `Input`. A starting value the compiler owns, such as `deleted_at: none`
  or `status: active`, is the lifecycle's `initial:` (see
  [Lifecycle](#lifecycle-soft-delete-and-purge)).

::: today

`generated` is written after the type and its block, with its own `on:`
setting, while a generated identity is written inside the block:

```jadpo project=generated
entity Note {
    id: Uuid { generated: identity }
    body: Text
    created_at: Instant generated { on: create }
    updated_at: Instant generated { on: create_or_change }

    identity: id

    persistence {
        store: primary
        role: authority
    }
}
```

`created_at: Instant { generated: create }` is a syntax error today.

:::

## Where entities are stored

Every entity is stored in the application's database, and nothing in the
entity says so (question 2). The same project runs on SQLite locally and
PostgreSQL in production.

```jadpo fragment
entity Customer {
    id: Uuid
    email: Email {
        unique: true
    }
}
```

A cache or read model in another store (Redis, a search index, a graph) is a
copy kept up to date from the database, never a second place to write. Its
syntax returns when the first such store can actually run (question 3); area
6 decides how a query asks for one.

::: today

An entity opts in with a `persistence` block whose two settings each accept
one value, and `cache` and `projection` blocks declare copies in other
stores:

```jadpo project=stores
entity Customer {
    id: Uuid
    email: Email

    identity: id

    persistence {
        store: primary
        role: authority
        unique: email
    }

    cache hot {
        store: redis
        from: primary
        strategy: invalidate
        delivery: durable
    }
}
```

This builds, but nothing for the cache appears in the generated code, and a
`store:` other than `primary` fails only when the target is generated
(`toolchain.target_store_not_implemented`).

:::

## Uniqueness and indexes

```jadpo fragment
entity Account {
    id: Uuid
    handle: Handle {
        unique: true
    }
    tenant: TenantName
    owner_email: Email
    region: RegionCode {
        index: true
    }

    unique: {
        tenant_owner: [tenant, owner_email]
    }
}
```

- `unique: true` on a field makes it unique; `index: true` adds an index for
  lookups that no query declares (question 9).
- A rule over several fields is named in the entity's `unique: { }` section,
  with its fields as a list (question 4). Its fields must exist, be distinct
  and be required, so both databases agree on what a duplicate is.
- The name is what a write's conflict mapping refers to (area 6):

```jadpo fragment
var account: Account = attempt create Account {
    ...
    conflict: {
        Account.handle: HandleTaken
        Account.tenant_owner: OwnerAlreadyHasAccount
        other: AccountConflict
    }
}
```

The compiler also creates the indexes its named queries need, because every
read is a named query it can see: a page of a user's visible todos, newest
first, gets an index on `(owner_id, created_at desc, id desc)` limited to
rows where `deleted_at` is empty. The build lists each index and the query
that needs it (question 9).

::: today

A field can say `unique` and `index` after its type, the `persistence` block
can say `unique: name` and `index: name`, and compound rules use a call
shape:

```jadpo project=unique
entity Account {
    id: Uuid
    handle: Text unique
    tenant: Text
    owner_email: Email
    region: Text index

    identity: id

    persistence {
        store: primary
        role: authority
        constraint tenant_owner: unique (tenant, owner_email)
    }
}
```

The formatter writes `unique (tenant, owner_email)` with a space, unlike any
call. The compiler already derives indexes from query shapes: the golden todo
schema has `todo_page_owner_id_created_at_id_where_deleted_at_is_null_idx`,
which no source line declares. For other queries `jadpo check` warns
`index.recommendation_available`, and `jadpo schema index-accept` edits the
source to add `index`.

:::

## Relationships

A field whose type is another entity's id is a reference to that entity
(question 5). The compiler creates the foreign key and an index for it, and
the field states what happens to it when the referenced record is deleted:

```jadpo fragment
entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: restrict         // a user with todos cannot be deleted
    }
    reviewer_id: User.id? {
        on_delete: set_null         // the todo stays; the reviewer is cleared
    }
    ...
}

entity Comment {
    id: Uuid
    todo_id: Todo.id {
        on_delete: cascade          // comments go with their todo
    }
    ...
}
```

| `on_delete:` | When the referenced record is deleted | Allowed on |
|---|---|---|
| `restrict` | The delete fails with a conflict while any record refers to it | any reference |
| `cascade` | Records that refer to it are deleted in the same transaction | any reference |
| `set_null` | The reference is set to `none` | an optional reference (`User.id?`) |

There is no default: every reference says which one (question 5). A cycle of
required references (a team needs a lead, the lead needs a team) is an error,
because neither record could be created first.

The parent does not declare its children. A query that wants a user's todos
names the child's reference, and whether it gets a list or at most one record
follows from whether that field is unique (question 6; the include syntax is
area 6):

```jadpo fragment
// area 6 decides the exact shape
include: {
    todos: Todo.owner_id          // many: owner_id is not unique
    profile: Profile.user_id      // zero or one: user_id is unique
}
```

A many-to-many relationship is an entity of its own, so its lifecycle,
policy and extra fields stay visible:

```jadpo fragment
entity Membership {
    id: Uuid
    team_id: Team.id {
        on_delete: cascade
    }
    user_id: User.id {
        on_delete: cascade
    }
    role: MemberRole

    unique: {
        one_membership: [team_id, user_id]
    }
}
```

::: today

A field typed `User.id` has no foreign key unless the `persistence` block
also says `references`, which repeats the target and adds a logical name with
`as:`. The parent declares each inverse with `many` or `optional`:

```jadpo project=relationships
entity User {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
        inverse todos: many Todo via: Todo.owner_id
        inverse profile: optional Profile via: Profile.user_id
    }
}

entity Profile {
    id: Uuid
    user_id: User.id

    identity: id

    persistence {
        store: primary
        role: authority
        unique: user_id
        references user_id: User.id on_delete: cascade
    }
}

entity Todo {
    id: Uuid
    owner_id: User.id
    reviewer_id: User.id?

    identity: id

    persistence {
        store: primary
        role: authority
        references owner_id: User.id as: owner on_delete: restrict
        references reviewer_id: User.id on_delete: set_null
    }
}
```

A third spelling puts the reference after the field:
`owner_id: User.id references User.id as owner on_delete cascade`.

:::

## Lifecycle: soft delete and purge

A lifecycle says which records are visible, what state a new record starts
in, the named transitions that change that state, and when hidden records are
physically removed. The compiler enforces it on every read and write.

```jadpo fragment
entity User {
    id: Uuid
    status: UserStatus
    disabled_at: Instant?

    lifecycle: {
        initial: {
            status: UserStatus.active
            disabled_at: none
        }
        visible: status == UserStatus.active
        transitions: {
            disable: {
                when: status == UserStatus.active
                set: {
                    status: UserStatus.disabled
                    disabled_at: clock.now
                }
            }
        }
    }
}
```

- `initial:` gives every field the lifecycle owns its starting value. Code
  cannot supply those fields when it creates a record, nor change them with
  an ordinary update.
- `visible:` is a condition on the record's own fields. Hidden records are
  left out of every read before limits are applied, so a page never shrinks.
  A hidden record reads as missing, with the `not_found` failure the code
  already maps (decided in area 4).
- Each transition has a `when:` condition on the visible record and a fixed
  `set:`. An update names the transition (area 6); it may only target one
  record by `id`, and it cannot bring a hidden record back.
- `purge:` lets the runtime physically delete records hidden for longer than
  a configured duration, counted from the timestamp in `since:`. Only that
  timestamp's transition can start the clock, the duration must come from
  configuration, and nothing in the application can call the purge. The
  runtime deletes in batches of at most 500 and records each batch in the
  audit log; scheduling it is area 12.
- `when:` replaces `from:` and `since:` replaces the second `from`, so
  `from` keeps one meaning: "built from" (question 7).

Code cannot hard-delete an entity that has a lifecycle; only the purge can.

::: today

```jadpo project=lifecycle
enum UserStatus {
    active
    disabled
}

entity User {
    id: Uuid { generated: identity }
    status: UserStatus
    disabled_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            status: User.status(UserStatus.active)
            disabled_at: none
        }
        visible when status == UserStatus.active
        transition disable {
            from: status == UserStatus.active
            set: {
                status: User.status(UserStatus.disabled)
                disabled_at: clock.now
            }
        }
    }
}
```

Enum values in `initial:` and `set:` need the field constructor
(`User.status(UserStatus.active)`); area 2 decided that a literal takes the
expected type, so the plain variant is enough.

:::

## Operations an entity owns

An entity's file holds the actions and queries that create, change and read
it. They are ordinary actions and queries (area 3) whose parameters say what
they need, and they are called through the entity (decided in area 1):

```jadpo fragment
entity Todo {
    ...

    query visible_for(owner_id: User.id, page: TodoPageRequest) -> TodoPage { ... }

    action complete(todo_id: Todo.id) fails TodoNotFound -> Todo { ... }

    function display_title(todo: Todo) -> TodoTitle {
        ...
    }
}

var todo: Todo = attempt Todo.complete(todo_id)
var title: TodoTitle = Todo.display_title(todo)
```

There is no `self` receiver (question 10). A parameter of type `Todo` is a
complete snapshot; one of type `Todo.id` is a reference. Only an entity's own
actions may create, update or delete it; another domain calls those actions
(area 6 checks this, and area 3 decided that every write checks for a
concurrent change, so no `guard:` setting is needed).

::: today

Operations declare a receiver, `self: ref` or `self: value`, and a mutating
action with a value receiver needs `guard: reload` or `guard: revision`.
`self: Todo` itself is rejected:

```jadpo project=receivers
type NoteBody = Text {}

entity Note {
    id: Uuid
    body: NoteBody

    identity: id

    persistence {
        store: primary
        role: authority
    }

    function summary(self: value) -> NoteBody {
        return self.body
    }
}
```

:::

## How types are stored

The compiler picks each column from the field's type; nothing in the source
names a column type. A field's rules (`max_length`, `min`, `pattern`) are
checked by the application on every write and again when a row is read back;
the database enforces types, required fields, keys and references
(question 8).

| Field type | PostgreSQL | SQLite | Today (PostgreSQL / SQLite) |
|---|---|---|---|
| `Uuid` and types based on it | `uuid` | `text`, lowercase | `TEXT` / `TEXT` |
| `Int` | `smallint`, `integer` or `bigint`, picked from the declared range (area 2) | `integer` | `BIGINT` / `BIGINT` |
| `Decimal { scale: 2 }` | `numeric(p, 2)` (area 2) | `integer` holding the scaled value (area 2) | `NUMERIC` / `REAL` |
| `Bool` | `boolean` | `integer` 0 or 1 | same |
| `Text` | `text` | `text` | same |
| `Instant` | `timestamptz(3)` | `integer` milliseconds | same |
| `CalendarDate` | `date` | `text` `YYYY-MM-DD` | same |
| `Bytes` | `bytea` | `blob` | `BLOB` / `BLOB` |
| `Enum` without data | `text` holding the variant name | `text` | same |
| `Enum` with data, `Object`, `List`, `Set`, `Map` | `jsonb`, in the same JSON as the HTTP boundary | `text` holding that JSON | `TEXT` or `BLOB` / same |

A structured field is stored whole as JSON (question 8). That includes
recursive types such as a `Comment` whose `replies` are `List<Comment>`,
which area 3 left to this area. A query cannot filter on what is inside a
JSON field; data you need to filter or join on belongs in its own entity.

Renaming an enum variant changes stored values, so it is a schema change that
needs a migration, like renaming a field.

::: today

From `build/sql/postgres/schema.sql` for an entity with an `Object` field,
a `List<Text>` field and a `Bytes` field:

```text
CREATE TABLE IF NOT EXISTS "order" (
  "id" TEXT NOT NULL,
  "quantity" BIGINT NOT NULL,
  "price" NUMERIC NOT NULL,
  "address" BLOB NOT NULL,
  "tags" BLOB NOT NULL,
  "data" BLOB NOT NULL,
  CONSTRAINT "order_identity" PRIMARY KEY ("id")
);
```

PostgreSQL has no `BLOB` type (see Status).

:::

## Schema identity and renames

`database/identities.json` gives every table, column, unique rule and index
a stable identity (decided in area 1). It is how the compiler tells a rename
from a removal plus an addition: a renamed field keeps its identity and its
column, so no data moves.

```json
{"id":"entity/Todo","kind":"entity","path":"Todo","physical_name":"todo"}
{"id":"field/Todo.id","kind":"field","path":"Todo.id","physical_name":"id","owner":"entity/Todo"}
{"id":"field/Todo.title","kind":"field","path":"Todo.title","physical_name":"body","owner":"entity/Todo","previous_paths":["Todo.body"]}
```

The file is written by the compiler, committed and reviewed, with one sorted
entry per line so two branches that change the schema merge readably
(decided in area 1). The exact fields of an entry are an implementation
detail.

A rename is never guessed. Renaming a field with the language server's
rename, or with `jadpo rename field Todo.body Todo.title`, changes the source
and records the rename in the registry together (question 11). Renaming it by
hand fails `jadpo check` with `migration.identity_registry_drift`, which
names the field that disappeared and the one that appeared.

::: today

The registry is `schema.identities.json` at the project root, written as one
JSON line. `jadpo schema init` creates it; `jadpo schema add` registers new
declarations; `jadpo schema rename field Todo.body Todo.title` records a
rename and keeps the column name. Every `jadpo check` validates it when it
exists. The golden todo app has none, and there is no command that records a
removal (see Status).

:::

## Migrations

Each change to the stored schema is a folder under `database/migrations/`,
named by date, time and purpose (decided in area 1):

```text
database/
  identities.json
  migrations/
    20261012T0930_initial/
      schema.json            the schema after this migration, written by the compiler
      postgres.sql
      sqlite.sql
    20261020T1400_add_priority/
      schema.json
      changes.jadpo          your decisions, and backfills written in Jadpo
      postgres.sql
      sqlite.sql
  seeds/
    local_demo.jadpo
```

`jadpo migrate new add_priority` compares the source with the latest
migration's `schema.json` and writes the next folder with the SQL for both
databases (question 11). `jadpo check` fails while the source and the latest
migration disagree, so a schema change cannot ship without its migration.
Additions that cannot lose data (a new entity, an optional field, an index)
need nothing from you. Anything else stops at a decision in `changes.jadpo`,
which the generated SQL then follows:

```jadpo fragment
/// Adds a priority to every todo and retires the legacy flag.
migration {
    Todo.priority: {
        backfill: Priority.normal          // a new required field needs a value for existing rows
    }
    Todo.legacy_flag: retire               // the code stops using it; the column stays for now
    User.phone: {
        backfill: "unknown"                // was optional, now required
    }
}
```

| Change | What it needs |
|---|---|
| New entity, optional field, index, relaxed rule | Nothing |
| Rename | Done with the rename command; no data moves |
| New required field, optional field made required | `backfill:` with a typed value or expression |
| Stricter rule (shorter `max_length`, new `unique`) | The migration checks existing rows and stops if any break it |
| Changed type | A backfill expression from the old value |
| Removed field or entity | `retire`, then `drop` in a later migration (question 12) |
| Changed `on_delete` | An explicit decision |

The compiler never invents a default and never treats an empty development
database as evidence about production.

Removing a field takes two migrations (question 12). The first retires it:
the code no longer reads it and the column stays, so servers still running
the previous build keep working during a rolling deploy. A later migration
drops it, and a `drop` appears in the approval review like a policy change.

```jadpo fragment
migration {
    Todo.legacy_flag: drop                 // shown in the approval review
}
```

Migrations are applied by `jadpo migrate apply` as a deploy step before the
new build starts, each in one transaction where the database allows it, and
recorded in a table the runtime owns. A build refuses to start against a
database whose migrations do not match its own (question 11).

::: today

Nothing is ever applied. At startup the generated runtime runs
`CREATE TABLE IF NOT EXISTS` for every entity, so a new column never reaches
an existing database (read from generated code). The schema commands produce
review artifacts only:

```text
jadpo schema snapshot <project> before.json
jadpo schema diff <project> --against before.json
jadpo schema decision-template <project> --against before.json decisions.json
jadpo schema decision-check <project> --against before.json decisions.json
jadpo schema plan <project> --against before.json --decisions decisions.json --adapter postgres plan.json
jadpo schema sql <project> --against before.json --decisions decisions.json --adapter postgres review.json
```

Decisions are a JSON file with evidence strings, and every plan and SQL
review is marked `executable: false`. SQL is generated for additions,
backfills from literals, and nullability changes; removals, type changes and
`on_delete` changes fail closed.

:::

## Seeds

A seed creates demonstration or development data by calling the
application's own actions, so validation, lifecycle and policy apply exactly
as they do for a user (question 13):

```jadpo fragment
/// Two users with a few todos each, for local development.
seed {
    var alice: User = attempt User.provision("alice@example.com")
    attempt Todo.create_for(alice.id, "Buy milk")
    attempt Todo.create_for(alice.id, "Book dentist")
}
```

`jadpo seed local_demo` runs `database/seeds/local_demo.jadpo` against a
local or preview database; it refuses a production database. Data that
production needs, such as a list of countries, is a migration backfill
instead. Test fixtures are area 13.

There are no seeds today.

## Errors

> The examples in this section use today's syntax, because they show the compiler's real output; they are rewritten as the rework lands.

Every error this area can produce, with an example that triggers it and the
compiler's actual output.

### Identity

### `data.identity_field_unknown`

```jadpo project=e-identity-unknown expect=data.identity_field_unknown
entity Note {
    id: Uuid

    identity: key

    persistence {
        store: primary
        role: authority
    }
}
```

### `data.identity_nullable`

```jadpo project=e-identity-nullable expect=data.identity_nullable
entity Note {
    id: Uuid?

    identity: id

    persistence {
        store: primary
        role: authority
    }
}
```

### `data.multiple_identities`

The `identity:` item and a trailing `identity` name different fields:

```jadpo project=e-multiple-identities expect=data.multiple_identities
entity Note {
    id: Uuid
    key: Uuid identity

    identity: id

    persistence {
        store: primary
        role: authority
    }
}
```

### `data.modifier_non_entity`

```jadpo project=e-modifier-non-entity expect=data.modifier_non_entity
type Contact = Object {
    email: Text unique
}
```

### `syntax.duplicate_field_modifier`

```jadpo project=e-duplicate-modifier expect=syntax.duplicate_field_modifier
entity Account {
    id: Uuid identity
    handle: Text unique unique
}
```

### Generated fields

### `type.generated_identity_invalid`

```jadpo project=e-generated-identity expect=type.generated_identity_invalid
entity Note {
    id: Text { generated: identity }

    identity: id

    persistence {
        store: primary
        role: authority
    }
}
```

### `syntax.generated_on_required`

```jadpo project=e-generated-on expect=syntax.generated_on_required
entity Note {
    id: Uuid identity
    created_at: Instant generated { when: create }
}
```

### `syntax.generated_role_invalid`

```jadpo project=e-generated-role expect=syntax.generated_role_invalid
entity Note {
    id: Uuid identity
    created_at: Instant generated { on: delete }
}
```

### `type.generated_field_type`

```jadpo project=e-generated-type expect=type.generated_field_type
entity Note {
    id: Uuid { generated: identity }
    created_on: CalendarDate generated { on: create }

    identity: id

    persistence {
        store: primary
        role: authority
    }
}
```

### `type.generated_field_context`

```jadpo project=e-generated-context expect=type.generated_field_context
type Draft = Object {
    saved_at: Instant generated { on: create }
}
```

### `type.generated_field_assignment`

```jadpo project=e-generated-assignment expect=type.generated_field_assignment
entity Note {
    id: Uuid { generated: identity }
    body: Text
    created_at: Instant generated { on: create }

    identity: id

    persistence {
        store: primary
        role: authority
    }

    action add(body: Note.body, at: Note.created_at) -> Note {
        return attempt create Note {
            body: body
            created_at: at
        }
    }
}
```

### `type.generated_field_input`

Only a patch input is checked; an ordinary `input` with a generated field
compiles (see Status):

```jadpo project=e-generated-input expect=type.generated_field_input
entity Note {
    id: Uuid identity
    body: Text
    changed_at: Instant generated { on: create_or_change }
}

input PatchNote {
    changed_at: Note.changed_at optional
}

failure EmptyNotePatch {
    kind: InvalidValue
    code: "empty_note_patch"
}

failure NoteNotFound {
    kind: NotFound
    code: "note_not_found"
}

failure NoteConflict {
    kind: Conflict
    code: "note_conflict"
}

action patch_note(id: Note.id, patch: PatchNote)
    fails EmptyNotePatch, NoteNotFound, NoteConflict
    -> Note
{
    return attempt update required Note {
        where: id == id
        patch: patch
        empty: EmptyNotePatch
        missing: NoteNotFound
        conflict: NoteConflict
    }
}
```

### Storage

### `data.entity_not_persistent`

An entity with an `identity:` item and no `persistence` block cannot be
written, although one with a trailing `identity` and no block can:

```jadpo project=e-not-persistent expect=data.entity_not_persistent
entity Visit {
    id: Uuid { generated: identity }
    path: Text

    identity: id

    action record(path: Visit.path) -> Visit {
        return attempt create Visit {
            path: path
        }
    }
}
```

### `data.persistence_store_required`

```jadpo project=e-store-required expect=data.persistence_store_required
entity Note {
    id: Uuid

    identity: id

    persistence {
        role: authority
    }
}
```

### `data.persistence_role_required`

```jadpo project=e-role-required expect=data.persistence_role_required
entity Note {
    id: Uuid

    identity: id

    persistence {
        store: primary
    }
}
```

### `data.persistence_role_invalid`

```jadpo project=e-role-invalid expect=data.persistence_role_invalid
entity Note {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: replica
    }
}
```

### `data.representation_duplicate`

```jadpo project=e-representation-duplicate expect=data.representation_duplicate
entity Customer {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
    }

    cache hot {
        store: redis
        from: primary
        delivery: durable
    }

    cache hot {
        store: memcached
        from: primary
        delivery: durable
    }
}
```

### `data.representation_authority_invalid`

```jadpo project=e-representation-authority expect=data.representation_authority_invalid
entity Customer {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
    }

    cache hot {
        store: redis
        from: warehouse
        delivery: durable
    }
}
```

### `data.representation_setting_required`

```jadpo project=e-representation-required expect=data.representation_setting_required
entity Customer {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
    }

    cache hot {
        from: primary
        delivery: durable
    }
}
```

### `data.representation_setting_invalid`

```jadpo project=e-representation-setting expect=data.representation_setting_invalid
entity Customer {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
    }

    cache hot {
        store: redis
        from: primary
        delivery: durable
        expiry: short
    }
}
```

### `data.delivery_mode_invalid`

```jadpo project=e-delivery-mode expect=data.delivery_mode_invalid
entity Customer {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
    }

    cache hot {
        store: redis
        from: primary
        delivery: eventual
    }
}
```

### Uniqueness and indexes

### `syntax.constraint_non_entity`

```jadpo project=e-constraint-non-entity expect=syntax.constraint_non_entity
type Team = Object {
    name: Text
    constraint named: unique(name)
}
```

### `data.compound_constraint_fields`

```jadpo project=e-compound-fields expect=data.compound_constraint_fields
entity Account {
    id: Uuid
    handle: Text

    identity: id

    persistence {
        store: primary
        role: authority
        constraint only_handle: unique(handle)
    }
}
```

### `data.constraint_unknown_field`

```jadpo project=e-constraint-unknown expect=data.constraint_unknown_field
entity Account {
    id: Uuid
    tenant: Text

    identity: id

    persistence {
        store: primary
        role: authority
        constraint tenant_handle: unique(tenant, handle)
    }
}
```

### `data.constraint_duplicate_field`

```jadpo project=e-constraint-duplicate expect=data.constraint_duplicate_field
entity Account {
    id: Uuid
    handle: Text

    identity: id

    persistence {
        store: primary
        role: authority
        constraint pair: unique(handle, handle)
    }
}
```

### `data.constraint_nullable_field`

```jadpo project=e-constraint-nullable expect=data.constraint_nullable_field
entity Account {
    id: Uuid
    tenant: Text
    handle: Text?

    identity: id

    persistence {
        store: primary
        role: authority
        constraint tenant_handle: unique(tenant, handle)
    }
}
```

### `data.duplicate_constraint_shape`

The same two fields in a different order are the same rule:

```jadpo project=e-duplicate-shape expect=data.duplicate_constraint_shape
entity Account {
    id: Uuid
    tenant: Text
    handle: Text

    identity: id

    persistence {
        store: primary
        role: authority
        constraint tenant_handle: unique(tenant, handle)
        constraint handle_tenant: unique(handle, tenant)
    }
}
```

### `index.recommendation_available`

A warning: the build still succeeds.

```jadpo project=e-index-recommendation expect=index.recommendation_available
type GroupName = Text {}

entity Member {
    id: Uuid { generated: identity }
    group: GroupName

    identity: id

    persistence {
        store: primary
        role: authority
    }

    query in_group(group: Member.group)
    freshness: authoritative
        -> List<Member>
    {
        return attempt query many Member {
            where: group == group
            order_by: id asc
            limit: 50
            offset: 0
        }
    }
}
```

### References

### `data.relationship_target_field`

```jadpo project=e-reference-target-field expect=data.relationship_target_field
entity User {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
    }
}

entity Todo {
    id: Uuid
    owner_id: User.id

    identity: id

    persistence {
        store: primary
        role: authority
        references owner_id: User.key on_delete: cascade
    }
}
```

### `data.relationship_target_not_key`

```jadpo project=e-reference-not-key expect=data.relationship_target_not_key
entity User {
    id: Uuid
    email: Text

    identity: id

    persistence {
        store: primary
        role: authority
    }
}

entity Todo {
    id: Uuid
    owner_email: User.email

    identity: id

    persistence {
        store: primary
        role: authority
        references owner_email: User.email on_delete: cascade
    }
}
```

### `data.relationship_type_mismatch`

```jadpo project=e-reference-type expect=data.relationship_type_mismatch
entity User {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
    }
}

entity Todo {
    id: Uuid
    owner_id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
        references owner_id: User.id on_delete: cascade
    }
}
```

### `data.relationship_set_null_required`

```jadpo project=e-reference-set-null expect=data.relationship_set_null_required
entity User {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
    }
}

entity Todo {
    id: Uuid
    owner_id: User.id

    identity: id

    persistence {
        store: primary
        role: authority
        references owner_id: User.id on_delete: set_null
    }
}
```

### `data.relationship_cycle`

The message talks about cascading deletes, but the same error appears with
`restrict`; the real problem is that neither record can be created first
(see Status):

```jadpo project=e-reference-cycle expect=data.relationship_cycle
entity Team {
    id: Uuid
    lead_id: Member.id

    identity: id

    persistence {
        store: primary
        role: authority
        references lead_id: Member.id on_delete: restrict
    }
}

entity Member {
    id: Uuid
    team_id: Team.id

    identity: id

    persistence {
        store: primary
        role: authority
        references team_id: Team.id on_delete: restrict
    }
}
```

### Inverses

### `syntax.inverse_non_entity`

```jadpo project=e-inverse-non-entity expect=syntax.inverse_non_entity
input Team {
    name: Text
    inverse members: many Member via: Member.team_id
}
```

### `syntax.expected_inverse_cardinality`

```jadpo project=e-inverse-cardinality expect=syntax.expected_inverse_cardinality
entity User {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
        inverse todos: several Todo via: Todo.owner_id
    }
}

entity Todo {
    id: Uuid
    owner_id: User.id

    identity: id

    persistence {
        store: primary
        role: authority
        references owner_id: User.id on_delete: cascade
    }
}
```

### `data.inverse_via_field`

```jadpo project=e-inverse-via expect=data.inverse_via_field
entity User {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
        inverse todos: many Todo via: Todo.user_id
    }
}

entity Todo {
    id: Uuid
    owner_id: User.id

    identity: id

    persistence {
        store: primary
        role: authority
        references owner_id: User.id on_delete: cascade
    }
}
```

### `data.inverse_not_owning_reference`

```jadpo project=e-inverse-not-owning expect=data.inverse_not_owning_reference
entity User {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
        inverse todos: many Todo via: Todo.owner_id
    }
}

entity Todo {
    id: Uuid
    owner_id: User.id

    identity: id

    persistence {
        store: primary
        role: authority
    }
}
```

### `data.inverse_optional_not_unique`

```jadpo project=e-inverse-optional expect=data.inverse_optional_not_unique
entity User {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
        inverse profile: optional Profile via: Profile.user_id
    }
}

entity Profile {
    id: Uuid
    user_id: User.id

    identity: id

    persistence {
        store: primary
        role: authority
        references user_id: User.id on_delete: cascade
    }
}
```

### `data.inverse_duplicate_name`

```jadpo project=e-inverse-duplicate expect=data.inverse_duplicate_name
entity User {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
        inverse todos: many Todo via: Todo.owner_id
        inverse todos: many Todo via: Todo.owner_id
    }
}

entity Todo {
    id: Uuid
    owner_id: User.id

    identity: id

    persistence {
        store: primary
        role: authority
        references owner_id: User.id on_delete: cascade
    }
}
```

### Lifecycle syntax

The examples from here to the end of the lifecycle errors change one thing in
this valid entity:

```jadpo project=e-lifecycle-base
config RetentionConfiguration {
    retention: Duration {
        binding: "RETENTION"
    }
}

type TodoTitle = Text {}

entity Todo {
    id: Uuid { generated: identity }
    title: TodoTitle
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
        purge after config.retention from deleted_at
    }
}
```

### `syntax.lifecycle_non_entity`

```jadpo project=e-lc-non-entity expect=syntax.lifecycle_non_entity
type Draft = Object {
    deleted_at: Instant?

    lifecycle {
        initial: {
            deleted_at: none
        }
    }
}
```

### `syntax.lifecycle_duplicate`

```jadpo project=e-lc-duplicate expect=syntax.lifecycle_duplicate
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
    }
}
```

### `syntax.lifecycle_setting_invalid`

```jadpo project=e-lc-setting expect=syntax.lifecycle_setting_invalid
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        restore: true
        visible when deleted_at == none
    }
}
```

### `syntax.lifecycle_initial_duplicate`

```jadpo project=e-lc-initial-duplicate expect=syntax.lifecycle_initial_duplicate
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
    }
}
```

### `syntax.lifecycle_visible_when_required`

```jadpo project=e-lc-visible-when expect=syntax.lifecycle_visible_when_required
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible if deleted_at == none
    }
}
```

### `syntax.lifecycle_visible_duplicate`

```jadpo project=e-lc-visible-duplicate expect=syntax.lifecycle_visible_duplicate
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        visible when deleted_at == none
    }
}
```

### `syntax.lifecycle_transition_from_required`

```jadpo project=e-lc-transition-from expect=syntax.lifecycle_transition_from_required
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            set: {
                deleted_at: clock.now
            }
        }
    }
}
```

### `syntax.lifecycle_transition_set_required`

```jadpo project=e-lc-transition-set expect=syntax.lifecycle_transition_set_required
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
        }
    }
}
```

### `syntax.lifecycle_transition_setting_invalid`

`when:` is this page's proposed spelling; today it is rejected:

```jadpo project=e-lc-transition-setting expect=syntax.lifecycle_transition_setting_invalid
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            when: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
    }
}
```

### `syntax.lifecycle_transition_duplicate`

```jadpo project=e-lc-transition-duplicate expect=syntax.lifecycle_transition_duplicate
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
    }
}
```

### `syntax.lifecycle_purge_after_required`

```jadpo project=e-lc-purge-after expect=syntax.lifecycle_purge_after_required
config RetentionConfiguration {
    retention: Duration {
        binding: "RETENTION"
    }
}

entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
        purge once config.retention from deleted_at
    }
}
```

### `syntax.lifecycle_purge_from_required`

`since` is this page's proposed word; today it is rejected:

```jadpo project=e-lc-purge-from expect=syntax.lifecycle_purge_from_required
config RetentionConfiguration {
    retention: Duration {
        binding: "RETENTION"
    }
}

entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
        purge after config.retention since deleted_at
    }
}
```

### `syntax.lifecycle_purge_duplicate`

```jadpo project=e-lc-purge-duplicate expect=syntax.lifecycle_purge_duplicate
config RetentionConfiguration {
    retention: Duration {
        binding: "RETENTION"
    }
}

entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
        purge after config.retention from deleted_at
        purge after config.retention from deleted_at
    }
}
```

### Lifecycle rules

### `type.lifecycle_requires_persistent_entity`

```jadpo project=e-lc-requires-persistent expect=type.lifecycle_requires_persistent_entity
entity Todo {
    id: Uuid
    deleted_at: Instant?

    identity: id

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
    }
}
```

### `type.lifecycle_initial_required`

```jadpo project=e-lc-initial-required expect=type.lifecycle_initial_required
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
    }
}
```

### `type.lifecycle_visible_required`

```jadpo project=e-lc-visible-required expect=type.lifecycle_visible_required
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
    }
}
```

### `type.lifecycle_field_unknown`

```jadpo project=e-lc-field-unknown expect=type.lifecycle_field_unknown
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
            archived_at: none
        }
        visible when deleted_at == none
    }
}
```

### `type.lifecycle_field_ineligible`

The identity cannot be owned by the lifecycle:

```jadpo project=e-lc-field-ineligible expect=type.lifecycle_field_ineligible
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
            id: none
        }
        visible when deleted_at == none
    }
}
```

### `type.lifecycle_initial_duplicate`

```jadpo project=e-lc-initial-field-duplicate expect=type.lifecycle_initial_duplicate
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
            deleted_at: none
        }
        visible when deleted_at == none
    }
}
```

### `type.lifecycle_initial_field_required`

A transition changes `deleted_at`, so `initial:` must give it a starting
value:

```jadpo project=e-lc-initial-field expect=type.lifecycle_initial_field_required
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
    }
}
```

### `type.lifecycle_predicate_unsupported`

Visibility may only compare lifecycle fields with fixed values:

```jadpo project=e-lc-predicate expect=type.lifecycle_predicate_unsupported
type TodoTitle = Text {}

entity Todo {
    id: Uuid { generated: identity }
    title: TodoTitle
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none or title == "x"
    }
}
```

### `type.lifecycle_transition_source_invalid`

```jadpo project=e-lc-transition-source expect=type.lifecycle_transition_source_invalid
type TodoTitle = Text {}

entity Todo {
    id: Uuid { generated: identity }
    title: TodoTitle
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: title == "x"
            set: {
                deleted_at: clock.now
            }
        }
    }
}
```

### `type.lifecycle_restore_forbidden`

```jadpo project=e-lc-restore expect=type.lifecycle_restore_forbidden
enum AccountStatus {
    active
    disabled
}

entity Account {
    id: Uuid { generated: identity }
    status: AccountStatus

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            status: Account.status(AccountStatus.active)
        }
        visible when status == AccountStatus.active
        transition restore {
            from: status == AccountStatus.disabled
            set: {
                status: Account.status(AccountStatus.active)
            }
        }
    }
}
```

### `type.lifecycle_transition_field_duplicate`

```jadpo project=e-lc-transition-field expect=type.lifecycle_transition_field_duplicate
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
                deleted_at: clock.now
            }
        }
    }
}
```

### `type.lifecycle_value_unsupported`

A transition can set fixed values, `none` and `clock.now`, nothing computed:

```jadpo project=e-lc-value expect=type.lifecycle_value_unsupported
type TodoTitle = Text {}

entity Todo {
    id: Uuid { generated: identity }
    title: TodoTitle
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
                title: TodoTitle("gone")
            }
        }
    }
}
```

### `type.lifecycle_purge_field_invalid`

```jadpo project=e-lc-purge-field expect=type.lifecycle_purge_field_invalid
config RetentionConfiguration {
    retention: Duration {
        binding: "RETENTION"
    }
}

type TodoTitle = Text {}

entity Todo {
    id: Uuid { generated: identity }
    title: TodoTitle
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
        purge after config.retention from title
    }
}
```

### `type.lifecycle_purge_initial_invalid`

```jadpo project=e-lc-purge-initial expect=type.lifecycle_purge_initial_invalid
config RetentionConfiguration {
    retention: Duration {
        binding: "RETENTION"
    }
}

entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: clock.now
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
        purge after config.retention from deleted_at
    }
}
```

### `type.lifecycle_purge_transition_invalid`

```jadpo project=e-lc-purge-transition expect=type.lifecycle_purge_transition_invalid
config RetentionConfiguration {
    retention: Duration {
        binding: "RETENTION"
    }
}

entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: none
            }
        }
        purge after config.retention from deleted_at
    }
}
```

### `type.lifecycle_purge_retention_invalid`

```jadpo project=e-lc-purge-retention expect=type.lifecycle_purge_retention_invalid
entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
        purge after Duration("PT720H") from deleted_at
    }
}
```

### Writes that a lifecycle forbids

The update and delete forms are area 6; these errors protect the lifecycle.

### `type.lifecycle_field_create_forbidden`

```jadpo project=e-lc-create expect=type.lifecycle_field_create_forbidden
type TodoTitle = Text {}

entity Todo {
    id: Uuid { generated: identity }
    title: TodoTitle
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
    }

    action add(title: Todo.title) -> Todo {
        return attempt create Todo {
            title: title
            deleted_at: none
        }
    }
}
```

### `type.lifecycle_field_update_forbidden`

```jadpo project=e-lc-update expect=type.lifecycle_field_update_forbidden
failure TodoMissing {
    kind: NotFound
    code: "todo_missing"
}

failure TodoConflict {
    kind: Conflict
    code: "todo_conflict"
}

entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
    }

    action restore(id: Todo.id)
        fails TodoMissing, TodoConflict
        -> Todo
    {
        return attempt update required Todo {
            where: id == id
            set: {
                deleted_at: none
            }
            missing: TodoMissing
            conflict: TodoConflict
        }
    }
}
```

### `type.lifecycle_hard_delete_forbidden`

```jadpo project=e-lc-hard-delete expect=type.lifecycle_hard_delete_forbidden
failure TodoMissing {
    kind: NotFound
    code: "todo_missing"
}

failure TodoConflict {
    kind: Conflict
    code: "todo_conflict"
}

entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
    }

    action remove(id: Todo.id)
        fails TodoMissing, TodoConflict
        -> Todo
    {
        return attempt delete required Todo {
            where: id == id
            missing: TodoMissing
            conflict: TodoConflict
        }
    }
}
```

### `type.lifecycle_transition_unknown`

```jadpo project=e-lc-transition-unknown expect=type.lifecycle_transition_unknown
failure TodoMissing {
    kind: NotFound
    code: "todo_missing"
}

failure TodoConflict {
    kind: Conflict
    code: "todo_conflict"
}

entity Todo {
    id: Uuid { generated: identity }
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
    }

    action archive(id: Todo.id)
        fails TodoMissing, TodoConflict
        -> Todo
    {
        return attempt update required Todo {
            where: id == id
            transition: archive
            missing: TodoMissing
            conflict: TodoConflict
        }
    }
}
```

### `type.lifecycle_transition_identity_required`

```jadpo project=e-lc-transition-identity expect=type.lifecycle_transition_identity_required
type TodoTitle = Text {}

failure TodoMissing {
    kind: NotFound
    code: "todo_missing"
}

failure TodoConflict {
    kind: Conflict
    code: "todo_conflict"
}

entity Todo {
    id: Uuid { generated: identity }
    title: TodoTitle
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
    }

    action delete_titled(title: Todo.title)
        fails TodoMissing, TodoConflict
        -> Todo
    {
        return attempt update required Todo {
            where: title == title
            transition: delete
            missing: TodoMissing
            conflict: TodoConflict
        }
    }
}
```

### `type.lifecycle_transition_body_invalid`

```jadpo project=e-lc-transition-body expect=type.lifecycle_transition_body_invalid
type TodoTitle = Text {}

failure TodoMissing {
    kind: NotFound
    code: "todo_missing"
}

failure TodoConflict {
    kind: Conflict
    code: "todo_conflict"
}

entity Todo {
    id: Uuid { generated: identity }
    title: TodoTitle
    deleted_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    lifecycle {
        initial: {
            deleted_at: none
        }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: {
                deleted_at: clock.now
            }
        }
    }

    action delete_renamed(id: Todo.id, title: Todo.title)
        fails TodoMissing, TodoConflict
        -> Todo
    {
        return attempt update required Todo {
            where: id == id
            transition: delete
            set: {
                title: title
            }
            missing: TodoMissing
            conflict: TodoConflict
        }
    }
}
```

### Operations

### `data.receiver_kind_invalid`

```jadpo project=e-receiver expect=data.receiver_kind_invalid
entity Note {
    id: Uuid
    body: Text

    identity: id

    persistence {
        store: primary
        role: authority
    }

    function summary(self: snapshot) -> Text {
        return self.body
    }
}
```

### Schema identity registry

These errors come from `jadpo check` on a project with a registry. Each
example shows the registry file beside the source; the registry is written
with one entry per line, the layout area 1 decided, except where that layout
is itself the error.

### `migration.identity_registry_not_canonical`

The decided one-entry-per-line layout is rejected today, because only the
single-line form counts as compiler-written:

```jadpo project=e-registry-canonical expect=migration.identity_registry_not_canonical
entity Note {
    id: Uuid
    body: Text

    identity: id

    persistence {
        store: primary
        role: authority
    }
}
```

```jadpo project=e-registry-canonical file=schema.identities.json
{"schema_version":1,"entries":[
{"id":"schema-v1/constraint/Note.id#primary_key","kind":"primary_key","path":"Note.id#primary_key","physical_name":"note_pkey","owner_id":"schema-v1/entity/Note","previous_paths":[]},
{"id":"schema-v1/entity/Note","kind":"entity","path":"Note","physical_name":"note","owner_id":null,"previous_paths":[]},
{"id":"schema-v1/field/Note.body","kind":"field","path":"Note.body","physical_name":"body","owner_id":"schema-v1/entity/Note","previous_paths":[]},
{"id":"schema-v1/field/Note.id","kind":"field","path":"Note.id","physical_name":"id","owner_id":"schema-v1/entity/Note","previous_paths":[]}
]}
```

### `migration.identity_registry_drift`

The field was renamed from `body` to `text` by hand, without
`jadpo schema rename`:

```jadpo project=e-registry-drift expect=migration.identity_registry_drift
entity Note {
    id: Uuid
    text: Text

    identity: id

    persistence {
        store: primary
        role: authority
    }
}
```

```jadpo project=e-registry-drift file=schema.identities.json
{"schema_version":1,"entries":[{"id":"schema-v1/constraint/Note.id#primary_key","kind":"primary_key","path":"Note.id#primary_key","physical_name":"note_pkey","owner_id":"schema-v1/entity/Note","previous_paths":[]},{"id":"schema-v1/entity/Note","kind":"entity","path":"Note","physical_name":"note","owner_id":null,"previous_paths":[]},{"id":"schema-v1/field/Note.body","kind":"field","path":"Note.body","physical_name":"body","owner_id":"schema-v1/entity/Note","previous_paths":[]},{"id":"schema-v1/field/Note.id","kind":"field","path":"Note.id","physical_name":"id","owner_id":"schema-v1/entity/Note","previous_paths":[]}]}
```

### `migration.identity_registry_invalid`

```jadpo project=e-registry-invalid expect=migration.identity_registry_invalid
entity Note {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
    }
}
```

```jadpo project=e-registry-invalid file=schema.identities.json
{"schema_version":1}
```

### `migration.identity_duplicate_id`

```jadpo project=e-registry-duplicate-id expect=migration.identity_duplicate_id
entity Note {
    id: Uuid
    body: Text

    identity: id

    persistence {
        store: primary
        role: authority
    }
}
```

```jadpo project=e-registry-duplicate-id file=schema.identities.json
{"schema_version":1,"entries":[
{"id":"schema-v1/constraint/Note.id#primary_key","kind":"primary_key","path":"Note.id#primary_key","physical_name":"note_pkey","owner_id":"schema-v1/entity/Note","previous_paths":[]},
{"id":"schema-v1/entity/Note","kind":"entity","path":"Note","physical_name":"note","owner_id":null,"previous_paths":[]},
{"id":"schema-v1/field/Note.body","kind":"field","path":"Note.body","physical_name":"body","owner_id":"schema-v1/entity/Note","previous_paths":[]},
{"id":"schema-v1/field/Note.body","kind":"field","path":"Note.title","physical_name":"title","owner_id":"schema-v1/entity/Note","previous_paths":[]},
{"id":"schema-v1/field/Note.id","kind":"field","path":"Note.id","physical_name":"id","owner_id":"schema-v1/entity/Note","previous_paths":[]}
]}
```

### `migration.identity_duplicate_path`

```jadpo project=e-registry-duplicate-path expect=migration.identity_duplicate_path
entity Note {
    id: Uuid
    body: Text

    identity: id

    persistence {
        store: primary
        role: authority
    }
}
```

```jadpo project=e-registry-duplicate-path file=schema.identities.json
{"schema_version":1,"entries":[
{"id":"schema-v1/constraint/Note.id#primary_key","kind":"primary_key","path":"Note.id#primary_key","physical_name":"note_pkey","owner_id":"schema-v1/entity/Note","previous_paths":[]},
{"id":"schema-v1/entity/Note","kind":"entity","path":"Note","physical_name":"note","owner_id":null,"previous_paths":[]},
{"id":"schema-v1/field/Note.body","kind":"field","path":"Note.body","physical_name":"body","owner_id":"schema-v1/entity/Note","previous_paths":[]},
{"id":"schema-v1/field/Note.copy","kind":"field","path":"Note.body","physical_name":"body","owner_id":"schema-v1/entity/Note","previous_paths":[]},
{"id":"schema-v1/field/Note.id","kind":"field","path":"Note.id","physical_name":"id","owner_id":"schema-v1/entity/Note","previous_paths":[]}
]}
```

### `migration.identity_registry_owner`

`Note.body` names the wrong owning entity:

```jadpo project=e-registry-owner expect=migration.identity_registry_owner
entity Note {
    id: Uuid
    body: Text

    identity: id

    persistence {
        store: primary
        role: authority
    }
}

entity Tag {
    id: Uuid

    identity: id

    persistence {
        store: primary
        role: authority
    }
}
```

```jadpo project=e-registry-owner file=schema.identities.json
{"schema_version":1,"entries":[
{"id":"schema-v1/constraint/Note.id#primary_key","kind":"primary_key","path":"Note.id#primary_key","physical_name":"note_pkey","owner_id":"schema-v1/entity/Note","previous_paths":[]},
{"id":"schema-v1/constraint/Tag.id#primary_key","kind":"primary_key","path":"Tag.id#primary_key","physical_name":"tag_pkey","owner_id":"schema-v1/entity/Tag","previous_paths":[]},
{"id":"schema-v1/entity/Note","kind":"entity","path":"Note","physical_name":"note","owner_id":null,"previous_paths":[]},
{"id":"schema-v1/entity/Tag","kind":"entity","path":"Tag","physical_name":"tag","owner_id":null,"previous_paths":[]},
{"id":"schema-v1/field/Note.body","kind":"field","path":"Note.body","physical_name":"body","owner_id":"schema-v1/entity/Tag","previous_paths":[]},
{"id":"schema-v1/field/Note.id","kind":"field","path":"Note.id","physical_name":"id","owner_id":"schema-v1/entity/Note","previous_paths":[]},
{"id":"schema-v1/field/Tag.id","kind":"field","path":"Tag.id","physical_name":"id","owner_id":"schema-v1/entity/Tag","previous_paths":[]}
]}
```

### `migration.identity_physical_name`

```jadpo project=e-registry-physical expect=migration.identity_physical_name
entity Note {
    id: Uuid
    body: Text

    identity: id

    persistence {
        store: primary
        role: authority
    }
}
```

```jadpo project=e-registry-physical file=schema.identities.json
{"schema_version":1,"entries":[
{"id":"schema-v1/constraint/Note.id#primary_key","kind":"primary_key","path":"Note.id#primary_key","physical_name":"note_pkey","owner_id":"schema-v1/entity/Note","previous_paths":[]},
{"id":"schema-v1/entity/Note","kind":"entity","path":"Note","physical_name":"note","owner_id":null,"previous_paths":[]},
{"id":"schema-v1/field/Note.body","kind":"field","path":"Note.body","physical_name":"","owner_id":"schema-v1/entity/Note","previous_paths":[]},
{"id":"schema-v1/field/Note.id","kind":"field","path":"Note.id","physical_name":"id","owner_id":"schema-v1/entity/Note","previous_paths":[]}
]}
```

## Review

### Status

Each row says how it was checked: compiled with `jadpo check`, built with
`jadpo build` and the generated SQL or TypeScript read, or run through the
`jadpo schema` commands. Nothing on this page was run against a live
database; the lifecycle runtime evidence is RM-206's record, not re-run.

| Feature | Status |
|---|---|
| Entity declaration | Works: fields, an identity, an optional `persistence` block, `cache`/`projection`, `lifecycle`, `policy` and owned operations in one file (compiled). The legacy `type` plus `persist` form still compiles. |
| Whether an entity is stored | **Bug (compiled and built):** it depends on how the identity is spelled. An entity with no identity at all compiles, can be created, and gets a table with no primary key. An entity without the `identity:` item has its `persistence { }` block silently ignored (`store: warehouse` passes `jadpo build`) and is stored anyway. An entity with `identity: id` and no `persistence` block cannot be created (`data.entity_not_persistent`). |
| Identity | Four spellings (compiled). Generation is opt-in with `{ generated: identity }`; without it every `create` must supply `id` (`type.missing_field`). **Gap (read from generated code):** generated ids are `crypto.randomUUID()`, version 4; area 2 decided version 7. |
| Generated timestamps | Works: `create` and `create_or_change`, required `Instant` only, never assigned (compiled). **Gap (compiled):** an `input` may contain a generated field (`created_at: Note.created_at`); only its use as a patch is rejected. `created_at: Instant { generated: create }` is a syntax error. |
| Uniqueness | Single-field and named compound rules work, with stable constraint names (compiled, generated SQL). **Gap (compiled):** `unique: handle` twice, or `handle: Text unique` plus `unique: handle`, is accepted silently. `constraint` is accepted inside an entity's `persistence` block, which grammar §4.2 does not list. |
| Indexes | `index` on a field or `index:` in the block (compiled). The compiler already adds indexes from query shapes, including a partial composite index for the golden todo page (generated SQL), and `jadpo check` warns `index.recommendation_available` for other query fields. |
| References | Works: foreign key plus index, `restrict`, `cascade` and `set_null`, target must be an identity or unique field of the same type (compiled, generated SQL). A field typed `User.id` without `references` gets no foreign key. **Wording bug (compiled):** a cycle of `restrict` references reports "Required cascading references form a delete cycle"; the cycle is wrong because neither row can be inserted first, whatever the delete rule. |
| Inverses | Works: `many` and unique-backed `optional`, checked against the child's reference (compiled). |
| Lifecycle | Works per RM-206: initial values, visibility before limits, guarded transitions, no restore, no hard delete, bounded audited purge, 13 runtime tests on both databases (recorded, not re-run). All 35 lifecycle errors compile as documented above. **Bug (compiled):** `purge after 30d from deleted_at` is misread and reports `syntax.lifecycle_purge_from_required`, although `30d` is the duration literal used in configuration defaults. |
| Caches and projections | Parsed and checked. **Gap (built):** a `cache` builds and nothing for it appears in the generated code; `strategy:` accepts any word (`strategy: whenever` compiles). |
| Receivers | `self: ref` and `self: value` work; `self: Todo` is rejected (compiled). |
| Storage types | **Bug (generated SQL):** `Object`, `List` and `Bytes` fields become `BLOB` columns in the PostgreSQL schema, and PostgreSQL has no `BLOB` type, so that schema cannot be created (inferred from the SQL, not run against PostgreSQL). **Gaps (generated SQL):** UUIDs are `TEXT` on PostgreSQL; every `Int` is `BIGINT` whatever its range; `Decimal` is `REAL`, a float, on SQLite. Each conflicts with area 2's decisions. |
| Schema registry | Works: `jadpo check` validates it; rename keeps the column's physical name (run). **Bug (run):** there is no way to record a removal. After deleting a field, `jadpo check` fails with `migration.identity_registry_drift`, `jadpo schema add` refuses with `migration.identity_additions_have_removal`, and no `schema` subcommand removes an entry. A rename also fails when any other change is pending, so renames must be done one at a time first. The registry is one JSON line. The golden todo app has no registry. |
| Migrations | Snapshot, diff, decisions, plans and SQL review work as review artifacts, all `executable: false` (DATA-003). **Gap (read from generated code):** nothing applies a migration; the runtime runs `CREATE TABLE IF NOT EXISTS` at startup, so an existing database never gains a new column. |
| Seeds | None. |

### Consistency with the rest of the language

- **One fact, several spellings.** Identity has four, uniqueness four,
   references three, and `generated` two (syntax review #16–#21). Area 2
   decided that a field's settings go in its block; entities are the largest
   remaining exception.
- **Settings with one legal value.** `store: primary`, `role: authority` and
   `delivery: durable` are required and accept nothing else that runs, the
   pattern area 3 removed from actions (`consistency: atomic`).
- **`role` means two things:** a storage role (`role: authority`) and an
   access role (`role: TodoRole.owner`). **`from` means six** (syntax review
   #26), three of them on this page: transition precondition, purge
   timestamp and cache source.
- **Sections and fields share a shape** under the colon rule:
   `lifecycle: { }`, `policy: { }` and `unique: { }` read like fields. Their
   names become reserved inside an entity, so no field can be called
   `lifecycle`, `policy` or `unique`.
- **Receivers duplicate area 3.** `self: ref` and `self: value` exist so that
   `todo.change_email(x)` works, but area 1 decided that the dot names the
   owner (`Todo.change_email(todo, x)`), and `value` is also a removed
   keyword. `guard:` is an action setting, which area 3 removed.
- **`Todo.Ref` and `Todo.id`** both mean "a reference to a todo".
- **Enum fields need a constructor** in `initial:` and `set:`
   (`User.status(UserStatus.active)`), which area 2's literal typing removes.
- **Storage contradicts area 2.** Number columns ignore ranges and `Decimal`
   is a float on SQLite, so the exactness area 2 decided does not survive a
   round trip through SQLite.
- **The registry is ahead of the workflow.** It is validated on every check,
   but nothing applies a migration and nothing records a removal, so it can
   block a change it has no way to accept.

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Reading and writing records: `query`, `create`, `update`, `patch`, `delete`, includes by the child's reference (question 6), `transition:` in an update, conflict mapping by unique rule name, mutation ownership (`data.mutation_owner_required`), `freshness`, `consistency` and `guard` (`data.*_freshness*`, `data.consistency_*`, `data.mutation_guard_invalid`, `data.value_receiver_guard_required`, `data.raw_query_outside_named_query`, `data.atomic_domain_mismatch`) | 6 | Input to area 6 |
| Reading from a cache or projection, once one can run (question 3) | 6 | Depends on question 3 |
| Whether children are hidden when their parent is soft-deleted; today they stay visible and a required include of a hidden parent reads as missing | 6, 9 | Open |
| Field-level policy in the field's block (`role:`, `immutable:`, `policy: { }`), entity `policy: { }`, `membership`, and purge as a compiler-only authority | 9 | Input to area 9 |
| Persisting presentation text (`type.presentation_text_persistence`) and `Instant` precision | 10 | Input to area 10 |
| Scheduling the purge worker | 12 | Input to area 12 |
| Test fixtures versus seeds | 13 | Input to area 13 |
| `migration.*` rules raised only by `jadpo schema` commands (snapshot, diff, decision, plan and SQL errors, registry read and write failures, `migration.identity_registry_missing`, `migration.identity_registry_exists`, `migration.identity_additions_have_removal`, rename errors), `index.accept_*` and `index.recommendation_*` other than the check warning, `cli.schema_*`, and `toolchain.target_store_not_implemented` (raised by `jadpo build`); none can be triggered by `jadpo check` on source | 14 | Input to area 14 |
| The `jadpo migrate` and `jadpo rename` commands themselves (question 11) | 14 | Depends on question 11 |

### Open questions

1. **Is the identity always one generated field called `id`?** Today it is
   declared four ways and generation is opt-in. *Recommendation:* yes:
   every entity has `id`, based on `Uuid`, generated as version 7 on create
   (or the version its named type pins), never supplied by a caller; only
   seeds and migration backfills may set one. No compound identity: a join
   entity has its own `id` and a compound unique rule. This deletes
   `identity:`, `generated: identity` and trailing `identity`. Rails,
   Django and Ecto all default to a single `id`; Prisma allows compound
   `@@id`, which every reference to that table must then repeat.
2. **Is every entity stored in the application's database?** Today storage
   depends on the identity spelling, and `persistence { store: primary
   role: authority }` is required ritual. *Recommendation:* yes: `entity`
   means a stored record, and the `persistence` block goes. Something with an
   identity that is not stored (a request-scoped actor) is a value; the
   entity model's "persistence is optional" (DATA-007) is reversed, because
   nothing in the repository uses a non-stored entity and the split creates
   the Status bug. Rails, Django, Ecto and Prisma models are all stored by
   definition.
3. **Keep `cache` and `projection` before any store can run them?** Today
   they parse and check, and the build emits nothing for them.
   *Recommendation:* remove their syntax now and design it with the first
   real adapter, keeping the rule that a copy is never written directly.
   Declarations that do nothing mislead both agents and reviewers; area 3's
   "reads are current unless they read a declared cache" still holds once
   one exists.
4. **How are compound unique rules written?** Today
   `constraint tenant_owner: unique(tenant, owner_email)`, the language's
   only call-shaped declaration. *Decided 2026-10-12:* a named list in the
   entity's `unique: { }` section, `tenant_owner: [tenant, owner_email]`,
   following the colon and list rules. The name stays because conflict
   mapping refers to it. Rails (`index ..., unique: true, name:`), Django
   (`UniqueConstraint(fields=[...], name=...)`) and Prisma (`@@unique([a, b],
   name: "...")`) all name compound rules for the same reason.
5. **Is every field typed `X.id` a reference, with `on_delete:` required?**
   Today a field typed `User.id` has no foreign key unless a separate
   `references` line repeats the target. *Decided 2026-10-12:* yes: the type
   already says what is referenced, so the compiler creates the foreign key
   and index, and `on_delete:` goes in the field's block with no default.
   Django made `on_delete` mandatory in 2.0 because its silent cascade
   default deleted data; Prisma and Ecto default to restrict-like behaviour,
   which hides the choice. The `as: owner` name goes too; includes name the
   field.
6. **Should a parent declare its children?** Today
   `inverse todos: many Todo via: Todo.owner_id` restates the child's
   reference. *Decided 2026-10-12:* no: a query includes children by naming the
   child's reference field, and it gets a list, or at most one record when
   that field is unique. One place states each relationship; nothing on the
   parent can drift from the child. Django derives the reverse relation the
   same way; Rails, Ecto and Prisma require both sides and report
   mismatches. Area 6 settles the include syntax.
7. **Keep the lifecycle general, in the colon form?** Most lifecycles are
   soft delete, which takes about ten lines. *Decided 2026-10-12:* keep the
   general form (User's disable uses it too), written `visible:`,
   `transitions: { delete: { when: ... set: { } } }` and
   `purge: { after: ... since: ... }`, with no soft-delete shorthand for now.
   A reviewer sees exactly what is hidden and when it goes; a shorthand can
   be added later without changing stored data. Rails and Django leave soft
   delete to libraries (`discard`, `django-safedelete`), each with its own
   rules.
8. **How are structured fields stored, and which column types?** Today
   objects and lists become `BLOB`, which PostgreSQL rejects.
   *Recommendation:* native types where the database has one (`uuid`,
   `boolean`, `numeric`, `date`, `timestamptz`, `bytea`), integer widths
   from the declared range, and structured fields (enums with data,
   objects, lists, sets, maps, recursive types) as `jsonb` on PostgreSQL and
   JSON text on SQLite, in the HTTP boundary's JSON. Field rules are checked
   by the application on write and on read rather than copied into `CHECK`
   constraints. Ecto's `embeds_many` and Prisma's `Json` store embedded data
   this way; Rails and Django would need a separate table per list.
9. **Who decides indexes?** Today the compiler derives some from queries,
   warns about others, and `schema index-accept` edits the source.
   *Recommendation:* the compiler creates every index its named queries
   need and lists each with the query that needs it in the build output;
   `index: true` remains for lookups outside the application, and the
   warning and source-editing command go. Jadpo can do this because raw
   queries are not allowed, so it sees every read; Rails, Django, Ecto and
   Prisma leave indexes to the author because they cannot.
10. **Do entity operations need a `self` receiver?** *Decided 2026-10-12:* no.
    Parameters are ordinary (`todo: Todo` for a snapshot, `todo_id: Todo.id`
    for a reference) and calls are `Todo.complete(todo_id)`, as area 1
    decided. `self: ref`, `self: value`, `Todo.Ref` and `guard:` go; area 3
    already made every write check for concurrent change. Ecto and Django's
    functions taking the record as an argument read the same way.
11. **How are migrations created and applied?** Today nothing is applied,
    and decisions are a JSON evidence file. *Recommendation:*
    `jadpo migrate new <purpose>` writes the next folder (schema, SQL for
    both databases, and `changes.jadpo` when a decision is needed);
    `jadpo check` fails while the source and the latest migration disagree;
    `jadpo migrate apply` runs pending migrations as a deploy step, each in a
    transaction, and a build refuses to start against a database that does
    not match. Renames go through the language server or `jadpo rename`,
    which change source and registry together. This is the shape of Django's
    `makemigrations`/`migrate`, Prisma's `migrate dev`/`migrate deploy` and
    Rails' generated migrations, with Jadpo-typed backfills instead of raw
    code. Applying at app startup was rejected because several instances
    would race.
12. **What does removing data need?** Today removals fail closed and cannot
    even be recorded. *Decided 2026-10-12:* two migrations. The first `retire`s
    the field: the code stops using it and the column stays, so servers on
    the previous build keep working during a rolling deploy. A later
    migration `drop`s it, and a drop appears in the approval review like a
    policy change. Removing an entity works the same way. Rails'
    `ignored_columns` followed by a drop is the same two-step, done by hand.
13. **What is a seed, and can it run in production?** *Decided 2026-10-12:* a
    `seed { }` file in `database/seeds/` that creates data by calling the
    application's own actions, so validation, lifecycle and policy apply;
    `jadpo seed <name>` refuses a production database. Reference data that
    production needs is a migration backfill. Django's JSON fixtures bypass
    validation and Rails' `seeds.rb` runs anything; going through actions
    keeps seeds from creating records the application could never create.

14. **Where is the database itself declared?** Today nowhere: the generated
    app uses PostgreSQL when a `DATABASE_URL` environment variable exists and
    SQLite otherwise, the `primary` store entities name is never declared, and
    `config.jadpo`'s `database_url` is ignored in favour of the raw
    environment variable. *Recommendation:* declare it once in `app.jadpo`,
    `database: { engine: postgres, url: config.database_url }` with `engine` a
    named option (`postgres` or `sqlite`); one engine for the application in
    development and production, avoiding the familiar "works locally, breaks
    live" gap; the connection always comes from configuration; tables,
    columns and indexes still come from entities. Fast throwaway databases for
    tests are area 13's question.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Layout | Carried from area 1: an entity lives in its own file under its domain's `entities/` folder; `database/` holds `identities.json` (one sorted entry per line), timestamped migration folders and `seeds/` | 2026-10-12 |
| Keyword | Carried from area 4: a declaration with its own folder keeps its keyword (`entity`); anything in `types/` is a `type` | 2026-10-12 |
| Field settings | Carried from area 2: a field's settings go in a block after its type, one per line, including entity facts (`unique`, `generated`, the reference); only `Input` has `optional` and `default` | 2026-10-12 |
| Colon rule | Carried from area 3: entity sections are `name: { ... }` (`lifecycle: { }`, `policy: { }`) | 2026-10-12 |
| Setting options and lists | Carried from area 2: closed options are lower_snake_case names (`on_delete: restrict`); lists are `[ ]` | 2026-10-12 |
| UUID versions | Carried from area 2: new identities are version 7; a named type can pin another version | 2026-10-12 |
| Derived types | Carried from area 2: `type X = Output from Todo { pick: [...] }`; generated fields never in an `Input` | 2026-10-12 |
| Calls and settings | Carried from areas 1 and 3: the dot names the owner (`Todo.get(id)`); actions have no settings and every write checks for concurrent change | 2026-10-12 |
| Mapping form | Carried from area 4: write conflicts map with `condition: Failure`, a hidden record reports the same `not_found` failure | 2026-10-12 |
| Identity | | |
| Stored entities | | |
| Caches and projections | | |
| Compound unique rules | A named list in the entity's `unique: { }` section, `tenant_owner: [tenant, owner_email]`; the name is used by failure mapping | 2026-10-12 |
| References | Any field typed `X.id` is a reference: the compiler creates the foreign key and index; `on_delete:` is required in the field's block with no default; no `as:` name | 2026-10-12 |
| Inverses | Parents do not declare children; a query includes children by naming the child's reference field (syntax in area 6) | 2026-10-12 |
| Lifecycle form | The general lifecycle stays, in the colon form (`visible:`, `transitions:`, `purge: { after: since: }`); no soft-delete shorthand for now | 2026-10-12 |
| Storage types | | |
| Indexes | | |
| Receivers | No `self`; ordinary parameters (`todo: Todo`, `todo_id: Todo.id`) and calls such as `Todo.complete(todo_id)`; `Todo.Ref` and `guard:` go | 2026-10-12 |
| Migration workflow | | |
| Removing data | Two migrations: retire (code stops using it, column stays), then drop; a drop needs explicit acceptance (the shared mechanism carried to area 7) | 2026-10-12 |
| Seeds | `database/seeds/` files create data through the application's own actions and refuse to run against production; reference data production needs is a migration backfill | 2026-10-12 |
| Database declaration | | |
