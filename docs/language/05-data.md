---
title: Data
area: 5
status: decided
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

How a Jadpo program declares the records it keeps and how their schema
changes. Reading and writing records is area 6; who may read or change one
is area 9.

> Decided on 2026-10-12. Code on this page shows the language after the rework, applying the decisions from areas 1 to 5; none of it compiles yet. Today's compiled syntax is collapsed beneath each section that changes. The reasoning behind the store and cache rules is in [the stores and caching mapping](research/data-stores-and-caching-mapping.md); where it differs, this page wins.

Examples use the golden todo app's `planning` domain, plus a shop where Redis
is needed.

## Stores

Stores are declared in `app.jadpo`; with none declared, there is one called
`main` on PostgreSQL. Each store's engine is fixed in source, so every
environment runs the same engine (D1 is on the roadmap). Every entity lives
in exactly one store, `main` unless its file says `store: name`; anything
never saved is a `type`. An entity in Redis must declare an expiry. An action
writes to at most one store; work across stores goes through an event and a
subscriber.

```jadpo fragment
// app.jadpo in a shop that caches its catalogue
application {
    stores: {
        main: {
            engine: Engine.postgres
            url: config.database_url
        }
        cache: {
            engine: Engine.redis
            url: config.redis_url
        }
    }
}
```

::: today

A `persistence` block repeats the only values that run, `cache` builds
nothing, and the engine depends on whether `DATABASE_URL` is set.

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

:::

## Entities and the `id`

An entity is a stored record with a lasting identity, in its own file under
its domain's `entities/` folder. Its identity is one field, `id`, based on
`Uuid`, generated as version 7 on create (or the version its named type
pins) and never supplied by callers; other code refers to it as `Todo.id`.
There is no compound identity: a link between two entities is an entity with
its own `id` and a compound unique rule. An entity value is an immutable
snapshot; reading a field never touches the database.

```jadpo fragment
// domains/planning/entities/project.jadpo
/// A group of todos with a shared goal.
entity Project {
    id: Uuid                    // version 7, generated on create
    owner_id: User.id {
        on_delete: OnDelete.restricted
    }
    name: ProjectName
}
```

::: today

The identity is declared four ways (`identity: id`, `{ generated: identity }`,
a trailing `identity`, `persist X { identity: id }`); generation is opt-in and
makes version 4 ids.

:::

## Fields and their settings

A field's settings (`generated:`, `unique:`, `on_delete:`) go in a block
after its type, one per line. A nullable field is `T?`; only `Input` has
`optional:` and `default:`, and a starting value the compiler owns is the
lifecycle's `initial:`. A generated field is a required `Instant` set from
the operation's clock, on create or on every change; code never assigns it
and it is never in an `Input`.

```jadpo fragment
entity Todo {
    ...
    due_at: Instant?
    created_at: Instant {
        generated: Generated.on_create   // set once, when the record is created
    }
    updated_at: Instant {
        generated: Generated.on_change   // set on create and on every change
    }
}
```

::: today

`generated` is written after the type with its own `on:` setting, and
`created_at: Instant { generated: create }` is a syntax error:

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

:::

## Unique rules

`unique: true` in a field's block makes that field unique. A rule over
several fields is named in the entity's `unique: { }` section with its fields
as a list; its fields must exist, be distinct and be required, so every
engine agrees on what a duplicate is. The name is what a write's conflict
mapping refers to (area 6).

```jadpo fragment
entity Project {
    ...
    name: ProjectName

    unique: {
        name_per_owner: [owner_id, name]
    }
}
```

::: today

A field can say `unique` after its type, the `persistence` block can say
`unique: name`, and compound rules use a call shape:

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

:::

## References and `on_delete`

Any field typed `X.id` is a reference, with a foreign key and index. Its
block must say what happens when the referenced record is deleted; there is
no default. A cycle of required references is an error, because neither
record could be created first.

| `on_delete:` | When the referenced record is deleted | Allowed on |
|---|---|---|
| `OnDelete.restricted` | The delete fails with a conflict while any record refers to it | any reference |
| `OnDelete.cascade` | Records that refer to it are deleted in the same transaction | any reference |
| `OnDelete.cleared` | The reference is set to `none` | an optional reference (`Project.id?`) |

```jadpo fragment
entity Todo {
    ...
    owner_id: User.id {
        on_delete: OnDelete.restricted  // a user with todos cannot be deleted
    }
    project_id: Project.id? {
        on_delete: OnDelete.cleared     // the todo stays; it leaves the project
    }
}
```

::: today

A field typed `User.id` has no foreign key unless the `persistence` block
repeats it with `references`, and the parent declares each child with
`inverse`:

```jadpo project=relationships
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
        references owner_id: User.id as: owner on_delete: restrict
    }
}
```

:::

## Children

A parent does not declare its children; the relationship is stated once, on
the child's reference. A query wanting a project's todos names that field and
gets a list, or at most one record when the field is unique. A many-to-many
link, such as project members, is an entity with two references and a unique
rule over both.

```jadpo fragment
// in a query: area 6 settles the include syntax
include: {
    todos: Todo.project_id          // a list: project_id is not unique
}
```

## Lifecycle

A lifecycle describes the stages a record goes through and when it drops
out of normal view. The common case is soft delete: deleting a todo hides it
everywhere at once but keeps it for 30 days, so it can be investigated or
restored, and then removes it for good.

```jadpo fragment
entity Todo {
    ...
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none          // which records normal reads can see
        transitions: {
            delete: {                        // a named, allowed stage change
                when: deleted_at == none     // only allowed when this is true
                set: {
                    deleted_at: clock.now    // what the change sets
                }
            }
        }
        purge: {                             // permanent deletion of hidden records
            after: config.soft_delete_retention
            since: deleted_at
        }
    }
}
```

- **`visible:`** Every read, list and count leaves out records that are not
  visible, and reading one by `id` reports `not_found`. Nothing else needs to
  remember to filter them.
- **`transitions:`** The only way to change the fields a lifecycle owns. Each
  has a name, an optional `when:` and a fixed `set:`. The generated
  operation `Todo.delete(todo_id)` runs it.
- **`purge:`** The application deletes hidden records once they have been
  hidden for the configured time, in small audited batches. Code cannot
  hard-delete a record that has a lifecycle.

`visible:` and `when:` are conditions on the record's own fields: comparisons
(`==`, `!=`, `<`, `<=`, `>`, `>=`), `and`, `or`, `none` checks and
`clock.now`, so scheduled publishing works:

```jadpo fragment
lifecycle: {
    visible: published_at != none and published_at <= clock.now
}
```

They cannot mention other records or who is asking: that is policy (area 9),
which keeps lifecycle conditions simple enough to become database indexes.

::: today

`visible when`, `transition x { from: }` and `purge after ... from ...` are
three different grammars, and enum values in `initial:` and `set:` need the
field constructor, `User.status(UserStatus.active)`.

:::

## Indexes

The compiler sees every read, so it creates every index its queries need,
multi-column and partial (for `visible:`) included; a page of a user's
visible todos, newest first, gets `(owner_id, created_at desc, id desc)`
where `deleted_at` is empty. An `indexes:` section declares lookups made from
outside the application; special kinds (full-text, JSON, geographic) take a
`kind:` option where the store supports it. Every index change appears in its
migration and in the build's schema report, beside the query that needed it.

```jadpo fragment
entity Todo {
    ...
    indexes: {
        by_due_at: [due_at]         // for the reporting export; no query needs it
    }
}
```

::: today

The compiler already derives indexes from query shapes, warns
`index.recommendation_available` for others, and `jadpo schema index-accept`
edits the source to add `index` to a field.

:::

## Caches

A cache keeps a copy of data in a Redis store so busy pages read it quickly.
Nothing is cached unless you add it, on an entity (single records) or on a
query (its results):

```jadpo fragment
entity Product {
    ...
    caches: {
        records: {                     // single products, looked up by id
            store: cache               // a Redis store declared in app.jadpo
            keep: 1h                   // the oldest an entry can be
        }
    }
}

query category_page(category_id: Category.id, page: PageRequest) -> Page<ProductCard> {
    from: Product
    where: Product.category_id == category_id
    cache: {                           // this query's results
        store: cache
        keep: 10m
    }
}
```

- **Writes always go to the database**, never to the cache; code never names
  cache keys.
- **The running application keeps caches current.** The compiler generates
  the code that, after every committed change, clears each cached entry the
  change could affect. By default a change to a `Product` clears every cached
  result of every query that reads products: some extra database reads, but
  never a stale list. Clearing only the affected entries (for example the
  category pages a product moved out of and into) is a later optimisation.
- **Only pages read caches.** Reads inside actions, policy checks and
  authentication always read the database, so a decision is never made from
  a stale copy. If Redis is down, pages read the database instead.
- **Permissions are applied on every read**, and cached results are keyed by
  tenant and the details policy depends on, so one user's results are never
  served to another. Erasing personal data clears cached copies immediately.
- **Each entry is stored as one JSON value**, the same JSON the API sends, so
  every Jadpo type maps cleanly: money as `"12.50"`, large integers as
  strings, sets as arrays, enums tagged.

A record that lives in Redis rather than being copied there (a session, a
cart) uses `store: sessions` on the entity instead, and can only be looked up
by `id` or a unique field.

## Storage types

Each column is the store engine's native type for the field's type; the
source never names one. Structured fields, recursive ones included, are
stored whole as `jsonb` in the API's JSON, which caches and Redis entities
hold too. The database enforces types, required fields, keys and references;
the application checks each field's own rules. Renaming an enum variant
changes stored values, so it needs a migration.

| Field type | PostgreSQL |
|---|---|
| `Uuid` and types based on it | `uuid` |
| `Int` | `smallint`, `integer` or `bigint`, from the declared range |
| `Decimal { scale: 2 }` | `numeric(p, 2)` |
| `Bool`, `Text`, `Bytes` | `boolean`, `text`, `bytea` |
| `Instant`, `CalendarDate` | `timestamptz(3)`, `date` |
| `Enum` without data | `text` holding the variant name |
| `Enum` with data, `Object`, `List`, `Set`, `Map` | `jsonb` |

::: today

UUIDs are `TEXT`, every `Int` is `BIGINT`, `Decimal` is `REAL` on SQLite, and
structured and `Bytes` fields are `BLOB` columns, a type PostgreSQL does not
have.

:::

## Migrations

Entities describe the schema you want; a migration records each step from
one version of the database to the next. `jadpo migrate new add_priority`
compares the entities with the previous migration's snapshot and writes the
next folder:

```text
database/migrations/20261020T1400_add_priority/
  snapshot.json        generated: the whole schema at this point
  postgres.sql         generated: the difference from the previous snapshot
  changes.jadpo        only when a decision is needed
```

```jadpo fragment
// changes.jadpo
/// Adds a priority to every todo and retires the legacy flag.
migration {
    Todo.priority: {
        backfill: Priority.normal       // existing rows need a value
    }
    Todo.legacy_flag: FieldChange.retired
}
```

- **Renames keep their data**: `database/identities.json` gives each table
  and column a stable identity, so renaming a field renames its column.
- **Decisions only where needed**: a value for existing rows, or retiring a
  field. Removing a field takes two migrations, `FieldChange.retired` (code
  stops using it, the column stays for servers still running the old build)
  and later `FieldChange.dropped`, which needs explicit acceptance.
- **Forgetting is caught at once**: `jadpo check` fails while the entities
  and the latest snapshot disagree, naming what changed and the command to
  run, so the editor, the build and CI all stop.
- **`jadpo migrate apply`** runs pending migrations as a deploy step, each in
  a transaction, one per store; the application refuses to start against a
  database that does not match.

::: today

Nothing is applied: the runtime runs `CREATE TABLE IF NOT EXISTS` at startup.
The `jadpo schema` commands produce review artifacts marked
`executable: false`, and the registry is `schema.identities.json`.

:::

## Seeds

A seed in `database/seeds/` creates development data by calling the
application's own actions, so validation, lifecycle and policy apply.
`jadpo seed <name>` refuses a production database; data production needs is
a migration backfill.

```jadpo fragment
// database/seeds/local_demo.jadpo
/// A user with a few todos, for local development.
seed {
    var alice: User = attempt planning.register_user("alice@example.com")
    attempt planning.create_todo(alice.id, "Buy milk")
    attempt planning.create_todo(alice.id, "Book dentist")
}
```

There are no seeds today.

## Where things live

An entity's file holds only its data definition (store, fields, unique
rules, indexes, lifecycle, policy, record cache) and gets generated
operations: `Todo.get`, create, update and delete. Every query you write is
in the domain's `queries/` folder, every action in `actions/` and every
failure in `failures/`, one per file. Written actions are called by name,
`complete_todo(todo_id)`, or `planning.complete_todo(todo_id)` from another
domain when public. There is no `self`: `todo: Todo` is a snapshot,
`todo_id: Todo.id` a reference.

```jadpo fragment
// domains/planning/failures/todo_not_found.jadpo
type TodoNotFound = Failure {
    kind: FailureKind.not_found
    message: "Todo not found."
}

// domains/planning/actions/complete_todo.jadpo
action complete_todo(todo_id: Todo.id) fails TodoNotFound -> Todo {
    var todo: Todo = attempt Todo.get(todo_id)      // generated operation
    ...
}
```

::: today

Operations sit inside the entity with a `self: ref` or `self: value`
receiver, and a value receiver that writes needs `guard:`.

:::

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

Bugs and gaps in today's compiler. Each was found by compiling with
`jadpo check`, building and reading the generated SQL or TypeScript, or
running the `jadpo schema` commands; nothing was run against a live database.

- **Whether an entity is stored depends on how its identity is spelled
  (compiled, built).** An entity with no identity gets a table with no
  primary key; one without `identity:` has its `persistence { }` block
  silently ignored (`store: warehouse` passes `jadpo build`); one with
  `identity: id` and no `persistence` block cannot be created.
- **Generated ids are version 4** (`crypto.randomUUID()`, read from generated
  code); area 2 decided version 7.
- **An `input` may contain a generated field** (compiled); only its use as a
  patch is rejected.
- **Duplicate unique rules are accepted silently** (compiled), and
  `constraint` is accepted inside a `persistence` block, which the grammar
  does not list.
- **A cycle of `restrict` references reports "Required cascading references
  form a delete cycle"** (compiled); the cycle is wrong because neither row
  can be inserted first, whatever the delete rule.
- **`purge after 30d from deleted_at` is misread** as
  `syntax.lifecycle_purge_from_required` (compiled).
- **A `cache` builds and generates nothing, and `strategy:` accepts any word**
  (built).
- **The PostgreSQL schema cannot be created** when an entity has an `Object`,
  `List` or `Bytes` field, which become `BLOB` (generated SQL, not run). UUIDs
  are `TEXT`, every `Int` is `BIGINT`, and `Decimal` is a float on SQLite.
- **The identity registry cannot record a removal** (run): after deleting a
  field, `jadpo check` fails with `migration.identity_registry_drift` and no
  command accepts it. A rename fails while any other change is pending. The
  golden todo app has no registry.
- **Nothing applies a migration** (read from generated code): the runtime runs
  `CREATE TABLE IF NOT EXISTS` at startup, so an existing database never gains
  a new column.

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Reading and writing records: `query`, `create`, `update`, `patch`, `delete`, includes by the child's reference (question 6), `transition:` in an update, conflict mapping by unique rule name, mutation ownership (`data.mutation_owner_required`), `freshness`, `consistency` and `guard` (`data.*_freshness*`, `data.consistency_*`, `data.mutation_guard_invalid`, `data.value_receiver_guard_required`, `data.raw_query_outside_named_query`, `data.atomic_domain_mismatch`) | 6 | Input to area 6 |
| Reading from a cache or projection (question 3) | 6 | Input to area 6 |
| Whether children are hidden when their parent is soft-deleted; today they stay visible and a required include of a hidden parent reads as missing | 6, 9 | Open |
| Field-level policy in the field's block (`role:`, `immutable:`, `policy: { }`), entity `policy: { }`, `membership`, and purge as a compiler-only authority | 9 | Input to area 9 |
| Persisting presentation text (`type.presentation_text_persistence`) and `Instant` precision | 10 | Input to area 10 |
| Scheduling the purge worker | 12 | Input to area 12 |
| Test fixtures versus seeds | 13 | Input to area 13 |
| `migration.*` rules raised only by `jadpo schema` commands (snapshot, diff, decision, plan and SQL errors, registry read and write failures, `migration.identity_registry_missing`, `migration.identity_registry_exists`, `migration.identity_additions_have_removal`, rename errors), `index.accept_*` and `index.recommendation_*` other than the check warning, `cli.schema_*`, and `toolchain.target_store_not_implemented` (raised by `jadpo build`); none can be triggered by `jadpo check` on source | 14 | Input to area 14 |
| The `jadpo migrate` and rename commands themselves (question 11) | 14 | Input to area 14 |
| GDPR support as its own review: personal-data marking on fields, erasure across every store and cache, export of everything held about a person, retention, and a build report of where personal data lives (Richard: strong out-of-the-box compliance helps adoption) | 5, 9, 10 | Raised |

### Open questions

1. **Is the identity always one generated field called `id`?** *Decided
   2026-10-12:* yes: `id`, based on `Uuid`, generated as version 7 on create
   (or the version its named type pins), never supplied by a caller; only
   seeds and migration backfills may set one. No compound identity: a
   many-to-many link is its own entity with a compound unique rule. Mapping
   an existing database with composite keys is out of scope.
2. **Is every entity stored in the application's database?** *Decided
   2026-10-12:* every entity lives in exactly one declared store, `main`
   unless its file says `store: name`; the `persistence` block goes; anything
   never saved is a `type`; an entity in an engine that can lose data (Redis)
   must declare an expiry. An action writes to at most one store; work across
   stores goes through an event and a subscriber.
3. **Keep `cache` and `projection` before any store can run them?** *Decided
   2026-10-12:* today's syntax is removed. Caches return with the Redis
   adapter: record, query result and computed caches beside what they copy,
   derived keys including tenant and policy inputs, compiler-driven
   invalidation, a required `keep:` of at most a day. Projections return with
   the first search, graph or edge engine. Display reads from routes may use
   caches; reads inside actions, policy and authentication never do. Erasing
   personal data wipes its cached copies immediately.
4. **How are compound unique rules written?** *Decided 2026-10-12:* a named
   list in the entity's `unique: { }` section, `tenant_owner: [tenant,
   owner_email]`; the name stays because conflict mapping refers to it.
5. **Is every field typed `X.id` a reference, with `on_delete:` required?**
   *Decided 2026-10-12:* yes: the compiler creates the foreign key and index,
   `on_delete:` goes in the field's block with no default, and the `as:` name
   goes.
6. **Should a parent declare its children?** *Decided 2026-10-12:* no: a
   query includes children by naming the child's reference field, and gets a
   list, or at most one record when that field is unique. Area 6 settles the
   include syntax.
7. **Keep the lifecycle general, in the colon form?** *Decided 2026-10-12:*
   yes: `visible:`, `transitions: { delete: { when: ... set: { } } }` and
   `purge: { after: ... since: ... }`, with no soft-delete shorthand for now.
8. **How are structured fields stored, and which column types?** *Decided
   2026-10-12:* each store uses its engine's native column types (on
   PostgreSQL `uuid`, `numeric`, `timestamptz`, `jsonb`, integer widths from
   declared ranges); structured fields are `jsonb`; caches and Redis entities
   store the same JSON as API responses.
9. **Who decides indexes?** *Decided 2026-10-12:* the compiler creates every
   index its named queries need, multi-column ones included, and lists each
   with the query that needs it. Lookups outside the application use
   `indexes: { by_tenant_created: [tenant, created_at] }`; special kinds
   (full-text, JSON, partial, geographic) take a `kind:` option where the
   store supports it; a partial index for `visible:` is derived. Every index
   change appears in its migration, and the build includes a schema report.
   The recommendation warning and the source-editing command go.
10. **Do entity operations need a `self` receiver?** *Decided 2026-10-12:* no.
    Parameters are ordinary (`todo: Todo` for a snapshot, `todo_id: Todo.id`
    for a reference) and written actions are called by name,
    `complete_todo(todo_id)`. `self: ref`, `self: value`, `Todo.Ref` and
    `guard:` go.
11. **How are migrations created and applied?** *Decided 2026-10-12:*
    `jadpo migrate new <purpose>` writes the next migration, one per store;
    `jadpo check` fails while source and the latest migration disagree;
    `jadpo migrate apply` runs pending migrations as a deploy step, each in a
    transaction, and the application refuses to start against a database
    that does not match; moving an entity between stores needs explicit
    acceptance.
12. **What does removing data need?** *Decided 2026-10-12:* two migrations:
    `retire` (the code stops using it, the column stays), then `drop` in a
    later one, shown in the approval review. Removing an entity works the
    same way.
13. **What is a seed, and can it run in production?** *Decided 2026-10-12:* a
    `seed { }` file in `database/seeds/` that creates data by calling the
    application's own actions; `jadpo seed <name>` refuses a production
    database. Reference data production needs is a migration backfill.
14. **Where is the database itself declared?** *Decided 2026-10-12:* in
    `app.jadpo`, `application { stores: { main: { engine: Engine.postgres url:
    config.database_url } } }`, with `main` on PostgreSQL implied when nothing
    is declared; each store's engine is fixed in source, so every environment
    uses the same engine. D1 is on the roadmap.
15. **How is a Redis entity's expiry written?** (carried from area 6) An
    entity in Redis must declare an expiry, and area 6 has expired records
    hidden from queries and updates like `visible:`, but this page gives the
    `expiry:` setting no syntax. What does it look like, and where does it go?
16. **A `lifecycle { }` block on an entity written `id: Uuid identity` is
    silently ignored today** (carried from area 8). Does it need its own fix?
    *Recommendation:* no; it is moot once the rework lands, because the
    trailing `identity` form goes.

### Decisions

Decided by Richard on 2026-10-12. Nothing here is built yet; the rework is listed in the roadmap.

| Topic | Decision | Date |
|---|---|---|
| Layout | Carried from area 1: an entity lives in its own file under its domain's `entities/` folder; `database/` holds `identities.json` (one sorted entry per line), timestamped migration folders and `seeds/` | 2026-10-12 |
| Keyword | Carried from area 4: a declaration with its own folder keeps its keyword (`entity`); anything in `types/` is a `type` | 2026-10-12 |
| Field settings | Carried from area 2: a field's settings go in a block after its type, one per line, including entity facts (`unique`, `generated`, the reference); only `Input` has `optional` and `default` | 2026-10-12 |
| Colon rule | Carried from area 3: entity sections are `name: { ... }` (`lifecycle: { }`, `policy: { }`) | 2026-10-12 |
| Setting options and lists | Carried from area 2: closed options are lower_snake_case names (`on_delete: restrict`); lists are `[ ]` (Superseded 2026-10-12: options are written `Type.value`; see area 2 question 17.) | 2026-10-12 |
| UUID versions | Carried from area 2: new identities are version 7; a named type can pin another version | 2026-10-12 |
| Derived types | Carried from area 2: `type X = Output from Todo { pick: [...] }`; generated fields never in an `Input` | 2026-10-12 |
| Calls and settings | Carried from areas 1 and 3: the dot names the owner (`Todo.get(id)`); actions have no settings and every write checks for concurrent change | 2026-10-12 |
| Mapping form | Carried from area 4: write conflicts map with `condition: Failure`, a hidden record reports the same `not_found` failure | 2026-10-12 |
| Identity | One field `id`, `Uuid`, generated as version 7, never supplied by callers; no composite identity: many-to-many links are entities with their own `id` and a compound unique rule | 2026-10-12 |
| Stores | Every entity lives in one declared store (`main` by default); no `persistence` block; never-saved things are types; Redis entities declare an expiry; an action writes at most one store | 2026-10-12 |
| Caches | Today's syntax removed; record, query-result and computed caches return with Redis, beside what they copy; derived keys including tenant and policy inputs; compiler-driven invalidation; required `keep:` up to a day; display reads only; erasure wipes cached copies immediately | 2026-10-12 |
| Compound unique rules | A named list in the entity's `unique: { }` section, `tenant_owner: [tenant, owner_email]`; the name is used by failure mapping | 2026-10-12 |
| References | Any field typed `X.id` is a reference: the compiler creates the foreign key and index; `on_delete:` is required in the field's block with no default; no `as:` name | 2026-10-12 |
| Inverses | Parents do not declare children; a query includes children by naming the child's reference field (syntax in area 6) | 2026-10-12 |
| Lifecycle form | The general lifecycle stays, in the colon form (`visible:`, `transitions:`, `purge: { after: since: }`); no soft-delete shorthand for now | 2026-10-12 |
| Storage types | Native column types per engine; structured fields as `jsonb`; caches use API JSON | 2026-10-12 |
| Indexes | Derived from named queries, including multi-column; `indexes: { name: [fields] }` for outside lookups; special kinds via a `kind:` option when the store supports it; indexes shown in migrations and in a generated schema report | 2026-10-12 |
| Receivers | No `self`; ordinary parameters (`todo: Todo`, `todo_id: Todo.id`) and written actions called by name, such as `complete_todo(todo_id)`; `Todo.Ref` and `guard:` go | 2026-10-12 |
| Migration workflow | `jadpo migrate new` / `apply`, one migration per store, check fails on drift, app refuses a mismatched database, moving stores needs acceptance | 2026-10-12 |
| Removing data | Two migrations: retire (code stops using it, column stays), then drop; a drop needs explicit acceptance (the shared mechanism carried to area 7) | 2026-10-12 |
| Seeds | `database/seeds/` files create data through the application's own actions and refuse to run against production; reference data production needs is a migration backfill | 2026-10-12 |
| Store declaration | Stores declared in `app.jadpo`; PostgreSQL `main` implied; same engine in every environment; D1 on the roadmap | 2026-10-12 |
| Query placement | Every query lives in its domain's `queries/` folder, one per file, whatever it reads; entity files hold the data definition; generated operations (`Todo.get`, create, update, delete) stay on the entity; a query's cache sits in its file (Richard, after scenarios) | 2026-10-12 |
| Action placement | Every action lives in its domain's `actions/` folder, one per file; written actions are called by name (`complete_todo(todo_id)`); generated operations stay on the entity (Richard) | 2026-10-12 |
| Generated fields | `generated: Generated.on_create` (set once when created) or `Generated.on_change` (set when created and on every change) (Richard) | 2026-10-12 |
| Redis entity expiry syntax | | |
| `lifecycle { }` ignored on `id: Uuid identity` | | |
| Lifecycle conditions | `visible:` and `when:` may use comparisons, `and`, `or`, `none` checks and `clock.now` on the record's own fields; never other records or the caller (Richard) | 2026-10-12 |
| Cache configuration | Opt-in: `caches: { records: { store: keep: } }` on an entity, `cache: { store: keep: }` on a query; by default a change clears every cached result reading that entity; precise clearing later; entries are one JSON value each (Richard) | 2026-10-12 |
| Migration snapshots | Each migration folder holds a generated `snapshot.json`, generated SQL and `changes.jadpo` only for decisions; the compiler diffs consecutive snapshots; `jadpo check` fails with the command to run when entities and the latest snapshot disagree (Richard) | 2026-10-12 |
