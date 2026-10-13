# Persistence slice v0.1

**Status:** accepted P10 prototype evidence; transaction widening superseded by TX-001
**Runtime adapters:** SQLite local and PostgreSQL  
**Deployment contract:** fresh schema plus parameterised CRUD

**Reading this historical slice:** new source uses the accepted
[entity/query model](entity-query-model.md): identity-bearing data lives in an
`entity` dossier, entity mutations in its actions, and reads in named queries
with declared freshness. Top-level CRUD examples below preserve the earlier
prototype's compatibility and adapter evidence; they are not a second source
model to copy into new applications. The operation-level parameterisation,
cardinality and failure contracts still apply within the accepted owners.

The first persistence operation is the existing canonical language form:

```text
action create_customer(input: CreateCustomer) -> Customer
    fails CustomerMutationConflict
{
    return attempt create Customer {
        id: input.id
        email: input.email
    } conflict: CustomerMutationConflict
}
```

`create` is an expression whose target must be an `entity`. It is permitted in
actions and rejected in functions with `EFFECT_FUNCTION_PERSISTENCE`. Its field
set must be complete and contain no unknown fields. Supplied values must be
compatible with the target's nominal field identities: `Customer.id` and
`Customer.email` in this example. The expression evaluates to a validated
`Customer`, not an insert count or untyped driver row.
Conflict bindings may identify a compiler-known constraint or provide a
fallback:

```text
conflict Account.handle: AccountHandleTaken
conflict Account.tenant_owner: AccountTenantOwnerTaken
conflict: AccountMutationConflict
```

The generated adapter translates native PostgreSQL constraint names and SQLite
unique signatures into compiler-owned identities. Specific bindings are tried
before the fallback; an unmatched constraint remains an operational fault when
no fallback is authored. Raw driver codes, names, and messages remain internal.

The first read operation makes its cardinality explicit:

```text
action find_customer(input: FindCustomer) -> Customer? {
    return attempt query optional Customer {
        where: id == input.id
    }
}
```

`query optional` accepts one equality predicate on a known, non-nullable entity
field. The predicate value must have that field's nominal type. Zero rows
produce `none`, one row produces a validated entity, and two rows prove a
cardinality violation. The adapter requests at most two rows so it can detect
that violation instead of choosing one silently. Reads are action-only in this
slice.

Required-one reads bind absence to a declared domain failure:

```text
action require_customer(input: FindCustomer) -> Customer
    fails CustomerNotFound
{
    return attempt query required Customer {
        where: id == input.id
        missing: CustomerNotFound {
            customer_id: input.id
        }
    }
}
```

The expression has type `Customer`, and its `missing:` failure must be declared
by the action and derive from `NotFound`. Missing rows reject that failure with
fully checked context. Required lookup therefore cannot turn absence into an
untyped exception.

Required mutations use explicit absence and constraint bindings:

```text
action update_customer(input: UpdateCustomer) -> Customer
    fails CustomerNotFound, CustomerMutationConflict
{
    return attempt update required Customer {
        where: id == input.id
        set: {
            email: input.email
        }
        missing: CustomerNotFound
        conflict: CustomerMutationConflict
    }
}
```

`update required` and `delete required` return the validated affected entity.
Zero matches reject the declared `NotFound`-derived failure. Driver constraint
violations reject the declared `Conflict`-derived failure. Multiple matches are
contained as operational cardinality faults. An update may name one or more
distinct, nominally typed fields in `set:`. The compiler lowers that fixed shape
to one parameterised `UPDATE ... SET ... RETURNING` statement, so no field can
commit independently of another. The automatically inferred action transaction
also covers persistence work before or after the update.

Omission-aware `patch:` updates preserve the same cardinality and failure
contracts. The patch input must contain only optional, nominally matched entity
fields and must name an `InvalidValue`-derived `empty:` failure. Both adapters
emit one stable SQL shape using a boolean supplied flag and value parameter per
possible field:

```sql
UPDATE "patch_item"
SET "title" = CASE WHEN $1 THEN $2 ELSE "title" END,
    "note" = CASE WHEN $3 THEN $4 ELSE "note" END,
    "marker" = CASE WHEN $1 THEN $5 ELSE "marker" END
WHERE "id" = $6
RETURNING "id", "title", "note", "marker"
```

Omission binds a false flag and leaves the column untouched. A true flag with a
null value is distinct and clears a nullable column. The persistence manifest
records these plans with `"omission":"supplied_flag"`.
Derived `set:` values share the same statement. Conditional derived writes
reuse the triggering patch field's supplied flag, so they neither add a dynamic
SQL shape nor confuse an omitted field with a supplied null.

## Generated storage contract

For every entity, `jadpo build` now emits:

```text
build/
  persistence/entities.json
  sql/postgres/schema.sql
  sql/sqlite/schema.sql
  target/persistence.ts
```

The manifest records ordered entity fields and the exact parameterised create,
optional-query, required-query, update, and delete statements for each adapter.
Postgres uses `$1`, `$2`, and so on; SQLite uses `?1`, `?2`, and so on. Values
are never interpolated into SQL text.

Entities may declare named compound uniqueness:

```text
constraint tenant_owner: unique(tenant, owner_email)
```

Its fields must exist, be distinct, and be non-nullable so both adapters share
the same semantics. The authored name and ordered fields appear in persistence
metadata and the generated schema uses a stable constraint name.

The generated Bun runtime selects PostgreSQL when `DATABASE_URL` is present and
uses `bun:sqlite` otherwise. It creates the fresh schema, executes
`INSERT ... RETURNING`, `SELECT ... LIMIT 2`, `UPDATE ... RETURNING`, and
`DELETE ... RETURNING`, and sends returned rows back through the generated
entity validator before application code or the HTTP serializer trusts them.
Required mutations preflight up to two rows and execute inside adapter-native
transactions, rolling back ambiguous cardinality. The SQLite path comes from
`SQLITE_PATH`; otherwise it is `build/local.sqlite`.

The P10 prototype infers one transaction for any transitively mutative action.
That behaviour remains implementation evidence, not final language authority:
TX-001 now makes an entity action independently failure-atomic and requires an
enclosing action that reaches multiple mutation scopes to declare atomic or
durable intent. Within a declared atomic boundary, all reads and writes receive
the same transaction-scoped adapter and nested mutative calls reuse it.
Read-only actions do not open a write transaction. PostgreSQL uses its native
async transaction callback; SQLite serialises action transactions on the
generated connection and uses `BEGIN IMMEDIATE`, `COMMIT`, and `ROLLBACK`. The
current manifest records the prototype `transaction_policy: mutative_action`;
future TX-001 artifacts must record the declared consistency and proved plan.

The generated PostgreSQL client currently sets `prepare: false`. Bun still
binds every parameter safely, but disables named prepared statements and query
pipelining. This is a deliberate compatibility boundary: Bun 1.2.20 can roll a
failed prepared transaction back on the server while leaving `SQL.begin()`
unresolved after a constraint error. The non-pipelined mode completes the same
rollback deterministically. Re-enabling prepared statements requires a pinned
runtime upgrade plus the full live rollback suite, not a target-only change.

Every adapter operation catches SQLite/PostgreSQL driver errors and replaces
them with the compiler-owned `PersistenceFault`. The operation name is stable;
the raw driver error is retained only as the internal `cause` and never crosses
the generated HTTP boundary.

Both database clients are Bun built-ins (`SQL` from `bun` and `Database` from
`bun:sqlite`). Persistence adds no package manifest or installation step.

Entity fields may now carry `identity`, `unique`, and `index` after their type.
The compiler rejects persistence modifiers outside entities, nullable identity
fields, and multiple identities on one entity. Fresh SQLite and PostgreSQL
schemas lower `identity` to a stable named primary-key constraint, `unique` to
a stable named unique constraint, and `index` to an idempotent named index. The
persistence manifest exposes the entity identity, unique fields, indexes, and
per-field flags. These properties do not become runtime value refinements.

Owning relationships use
`references Target.field [as relationship] on_delete <action>`. The optional
`as` name is the logical include/output name while the declaring field remains
the stored foreign key; without it, the field name is used for both. Names are
never inferred from storage suffixes. The
target must be a non-nullable identity or unique field, and the source field
must carry that target field's nominal type. `restrict`, `cascade`, and
nullable-only `set_null` are accepted. Fresh schemas create referenced tables
first, emit stable named foreign keys, and automatically index each referencing
field. SQLite foreign-key enforcement is enabled on the generated connection;
cross-entity dependency cycles that cannot be ordered are rejected.

`query many` adds the first collection read. It accepts one bound equality
predicate and requires `order_by: <identity-or-unique-field> asc|desc`. Both
adapters emit parameterised `SELECT ... WHERE ... ORDER BY ...` statements and
the generated application validates every returned entity before constructing
the result list.

Parents declare inverse collections explicitly with
`inverse todos: many Todo via Todo.owner_id`. The compiler verifies that the
named child field is an owning reference back to the parent. A required query
may then opt into one inverse using
`include: todos into: UserTodos order_by: id asc limit: 100 offset: 0`. `UserTodos`
must contain exactly
`parent: User` and `todos: List<Todo>`. The generated adapter executes one
bounded parent query and one ordered child query, validates both result sets,
and emits the nested shape with an empty list for zero children. The manifest
records `strategy: bounded_batch`, `query_count: 2`, cardinalities, ordering,
and both SQL statements. The positive literal limit and non-negative literal
offset are lowered to `$2`/`$3` or `?2`/`?3` parameters. No ordinary entity
access performs a hidden query.

Parents may declare a zero-or-one inverse as
`inverse profile: optional Profile via Profile.user_id`. The owning `via` field
must be identity or unique, so SQLite and PostgreSQL enforce at-most-one child
per parent. A required parent query opts in with
`include: profile optional into: UserProfile`; the generated
`bounded_optional_inverse` plan performs one bounded parent lookup and one
optional child lookup, validates both values, and emits `none` when the child
is absent. The manifest records both fields, cardinalities, statements, and a
query count of two.

`query many User` may also include one inverse when both levels are explicitly
ordered and paginated. The generated `parent_page_join` plan first selects the
parent page in a CTE, ranks each parent's children with `ROW_NUMBER()`, applies
the child window, and only then performs the left join. It executes as one
query, preserves empty child lists, and prevents flattened child rows from
changing parent-page membership. The manifest records the single-query choice,
the parent-before-join guarantee, both adapter statements, and all five
parameter positions.

Queries may repeat `include` to request multiple inverse collections in one
exact output shape. Every include retains independent deterministic ordering and
pagination. Required-parent loads execute one parent query followed by one
bounded child query per relationship. Many-parent loads execute one independent
parent-page join per relationship and merge results by the declared unique
parent ordering key. Query count is proportional to the authored relationships,
never the number of parents, and Cartesian row multiplication is avoided. The
manifest records `parent_then_bounded_children` or
`independent_parent_page_joins`, the exact query count, relationship names, and
`cartesian_product_avoided: true`.

A required child query may explicitly traverse one stored owning reference
with `include: relationship required|optional into: Output`. Nullable references must
use `optional`; their null value is preserved without a parent query. Otherwise
the generated adapter performs one bounded child lookup and one bounded lookup
of the referenced identity or unique field, validates both rows, and constructs
the exact output. The manifest exposes `strategy: bounded_parent_lookup`, a
query count of two, source and target fields, relationship cardinality, and the
representative parameterised SQL. This is explicit eager loading, not lazy
entity access.

One bounded depth-two path is accepted as
`include: owner.profile optional into: TodoOwnerProfile`: the first hop is a
non-nullable owning reference and the second is a unique-backed optional
inverse. Generation performs exactly three bounded queries for the root,
required middle entity, and optional leaf. The manifest exposes
`strategy: bounded_nested_lookup`, `maximum_depth: 2`, the ordered path,
cardinalities, all three statements, and `query_count: 3`. Other nested shapes
remain rejected rather than recursively expanded.

## Relationship-shape disposition

The first bounded relationship core is intentionally smaller than every shape
an ORM can spell:

| Shape | Disposition | Reason |
|---|---|---|
| owning field -> required/optional parent | implemented | the stored foreign key and nullability enforce the declared cardinality |
| parent -> optional inverse-one | implemented | a unique owning field enforces zero-or-one |
| owning field -> optional inverse-one | implemented to depth two | three fixed lookups preserve the same enforceable cardinalities |
| parent -> required inverse-one | rejected for now | a unique child foreign key proves at-most-one, not that every parent has a child; ordinary SQLite/PostgreSQL foreign keys cannot enforce totality in this direction |
| nested to-many | deferred | every collection hop needs explicit ordering, limit, offset/cursor, and parent-before-child pagination semantics |
| many-to-many | explicit join entity | ownership, lifecycle, attributes, and policy stay visible; no invisible join table is inferred |
| recursive/deeper paths | rejected | depth and query count must remain statically bounded |

A future required inverse must identify an enforceable storage construction or
an explicit domain absence path. It cannot turn a missing child into an
undocumented 500 or label an application convention as a database guarantee.

## Deliberate limits

This is a fresh-schema CRUD slice. It does not yet claim:

- defaults, generated IDs, or many-to-many join declarations;
- precise foreign-key/check-constraint mapping beyond the implemented identity,
  single-field unique, and compound-unique mappings;
- compound predicates, nullable predicates, dynamic bounds, nested paths beyond
  the bounded owning-parent/optional-inverse form, enforceable required
  inverse-one declarations, general patch-condition expressions, or projections;
- authored isolation selection, savepoints, or deliberate transaction splitting;
- schema diffs or migrations.

Those omissions are visible rather than simulated. They belong to the focused
P10.5 language-completion pass; the P10 fresh-schema persistence-core exit gate
is complete.

The [persistence seed](../examples/persistence-seed/app.jadpo) and its
[SQLite HTTP test](../tests/runtime/persistence-seed.test.ts) and
[PostgreSQL HTTP test](../tests/runtime/persistence-postgres.test.ts) are
executable evidence. Invalid wire values produce no row. A valid request
inserts with bound parameters, decodes the returned row as `Customer`, and can
be read back, updated, and deleted through typed operations. Both adapters prove
zero-row, one-row, duplicate-row, typed not-found, create/update
constraint-to-conflict,
single- and multi-operation transaction rollback, and raw-driver-failure
behaviour; operational faults are
contained behind the generic fault envelope without leaking driver or invariant
detail. SQLite additionally proves that generated identity and unique
constraints reject duplicates and that the declared lookup index exists. The
`User`–`Todo` pressure case proves invalid-owner rejection, automatic
relationship indexing, cascade deletion, and a validated deterministically
ordered nested parent/child read through a bounded two-query generated adapter.
SQLite executes zero-, one-, and many-child cases, missing parents, multiple
users without cross-user mixing, and a query-plan count assertion. The
PostgreSQL suite carries the equivalent nested-result cases and passed all 22
cases against a fresh Postgres 16 database for the P10 exit run.

The `AtomicItem` pressure case deliberately performs two creates without any
transaction syntax. A unique-key failure on the second create returns the
declared conflict and proves the first create was rolled back; the success case
proves both writes commit together.

The `Account` pressure case updates `handle` and `owner_email` in one authored
`set:` block. The generated adapter emits one statement with three bound
parameters (two replacements and the predicate). A conflicting handle returns
the declared conflict while the previously stored email remains unchanged,
proving fixed-shape multi-field updates are atomic.
