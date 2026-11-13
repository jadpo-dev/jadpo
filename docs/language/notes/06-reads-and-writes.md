---
title: Reads and writes, design notes
area: 6
---

# Reads and writes: design notes

Why [Reads and writes](../06-reads-and-writes.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Consistency with the rest of the language

- **Qualified fields.** This page writes `Todo.status` in every condition,
  while area 5's lifecycle writes `visible: deleted_at == none` and
  `when: deleted_at == none` with bare fields. One rule for every condition
  on a record would mean qualifying those too.
- **`set:` everywhere.** Create, update, counters and lifecycle transitions
  all give field values in a `set:` block.
- **Two not-found mappings.** `Todo.get` reports the entity's own failure,
  while a query maps `missing:` itself, because a query can be about
  something other than one record (`ProjectBoardView`).
- **`Page<T>` is a standard generic type,** like `List<T>`; area 3 rules
  out generics only in your own code.
- **Row versions as other tools use them.** The check that a record loaded
  before an outside call is unchanged compares PostgreSQL's row version,
  `xmin`, which Entity Framework's PostgreSQL provider uses the same way
  for optimistic concurrency.

## Carried to later areas

| Item | Area | Status |
|---|---|---|
| A route running a query for a display read; the write revision header and cookie; `If-Match` checked against a record's revision | 7 | Answered by area 7: a `GET` runs a query, and every write returns a `Jadpo-Revision` header; `If-Match` is deferred there |
| Policy filters in queries, the cache policy key, concealment of hidden and unpermitted records alike | 9 | Answered by area 9: policy is compiled into each query's `WHERE` |
| Children of a hidden parent: the policy side, such as whether a hidden owner's todos stay readable by others | 9 | Input to area 9 |
| Exports and reports that read a large set, and whether they need a consistent snapshot read | 12 | Answered by area 12: no snapshot reads; keyset pages |
| Asserting in tests how many records and database queries a query can cost | 13 | Input to area 13 |
| The build report: each query call's source (store or cache), its row bound and query count, and the indexes it needs | 14 | Area 14's query report |
| Counts and sums in queries, and totals kept on an entity in the same transaction | 6 | Aggregate queries are built ("Counts, sums and groups"); totals kept on an entity are deferred |
| `expiry:` on lossy-store entities hiding expired records from queries and updates like `visible:` | 5 | Answered by area 5: `expires_after:` |
| Several changing calls: the record, replay and `diverged` | 12 | Answered: "Several outside calls" |

## Decisions

Rows starting "From area" were decided in that area and bind this page.

| Topic | Decision |
|---|---|
| Query placement | From areas 1 and 5: every query lives in its domain's `queries/` folder, one per file; generated operations stay on the entity |
| Action placement | From areas 1 and 5: every action lives in `actions/`, one per file; written actions are called by name |
| Inverses | From area 5: parents do not declare children; a query includes children by naming the child's reference field |
| Receivers | From area 5: no `self`; `todo: Todo` is a loaded record and `todo_id: Todo.id` a reference; there is no `guard:` |
| Callable settings | From area 3: none; every action is all-or-nothing, writes check for concurrent change, reads are current unless they read a declared cache |
| Mapping form | From area 4: `condition: Failure` inside the operation's block; the original failure kept as the internal cause |
| Stores and caches | From area 5: an action writes at most one store; caches are copies for display reads only, with a required `keep:` |
| Loops | From area 3: `for` over a `List` only |
| Query shape | One declarative read: `from:`, stacked `where:`, `order:`, `limit:` or `page:`, `include:`, `missing:`; the result type sets how many records come back; composition happens in actions |
| Filters | Fields qualified with their entity; every `where:` line must hold; comparisons, `and`, `or`, `not`, `== none`, `in`; `where_present:` for optional filters; only conditions an index can serve |
| Selected columns | The generated SQL selects only the columns the result type needs, plus `order:` fields for the cursor and fields a policy checks outside the database |
| Page types | `Page<T>` (items and next), `PageCursor` and `PageSize` (1 to 100) are built in; an app may declare its own `Int` type with a `max:` for a different limit |
| Row-level security | Not now; later an opt-in setting on a PostgreSQL store, generated from policy. The compiled `WHERE` is the protection |
| Order and pages | `order: [field direction]` with `id` added as the tie-break; every list bounded by a literal or a type's `max:`; keyset pages only, no offset |
| Includes | `include:` names a reference field: own reference gives the parent, another entity's gives children (ordered and limited); two levels, within the domain, across stores only by `id` |
| Entity operations | `get`, `create`, `update` (`set:` or `patch:`), `delete` and one per lifecycle transition; only actions in the owning domain call the writing ones |
| Write failures | Missing record gives `<Entity>NotFound` (provided unless declared); every unique rule and restricted reference a write can break mapped in `conflict:`, no catch-all, since the compiler knows which rules each write can break |
| Empty patch | Succeeds and changes nothing (RFC 7396); conditional writes use `when_supplied: { }` |
| Concurrent change | The runtime reruns the whole action within the retry budget, then fails `unavailable`; repeatable-read isolation on PostgreSQL |
| Counters | An update by `id` using `Entity.field` for the stored value (`Post.view_count + 1`), done by the database in one step |
| Hidden parents | Visibility never spreads; an include of a hidden parent returns it; the action that hides a parent runs its children's transitions when they should go too |
| Reads in loops | A query or `get` inside `for` is a compile error; the fix is one query with `in` |
| Cache reads | Only a query a route runs directly may be served from a cache; reads in actions, policy and authentication go to the store; a client that just wrote skips caches |
| Policy in query plans | Policy compiled into each query's `WHERE`; fields a policy condition filters on get indexes; row-level security later, opt-in |
| Caches in tests | `jadpo test` supplies an in-memory cache that behaves like Redis, emptied between tests, so tests run production's code paths; a test checks a cached read by reading, writing elsewhere and reading again |
| Write targets | `update` and `delete` take the record loaded earlier or its `id`; by `id` no record is the entity's not-found failure; a delete gives nothing |
| Update sections | `set:` or `patch:`, never both; `when_supplied:` only with `patch:`, never setting a field the patch has or one another entry sets |
| Provided not-found failure | Kind `not_found`, public, no fields, placed at its entity |
| References that hold | A reference's value read from a record loaded earlier (its `id`, or its reference to the same entity) or `none` names a record that is there; any other value may not, and the write can fail with that entity's not-found failure |
| Stored values | An update by `id` works a stored field out only with `+` and `-`, under constructions the declared ranges prove |
| A one-step update that refuses (decided 2026-11-11, Richard accepting the recommendation) | A stored-value construction in `set:` may map the rules it can break, as any construction does (area 2): `stock: Stock(Product.stock - quantity) { min: OutOfStock }`. The database checks the condition in the same statement (`WHERE "stock" >= $1`), and no row updated is the mapped failure, so the update stays one step and never reruns. Without it, taking stock can't be a one-step update, because the declared ranges can't prove the result stays at 0 or above, so the action loads the product first, and buyers racing on one product rerun each other; in a flash sale that uses up the three-attempt budget. Built in chunk Q2b |
| A refusal's kind (2026-11-12, chunk Q2b, the obvious recommendation) | Each arm of a construction an update by `id` works out from stored fields names a `conflict` failure, never `invalid_value` (`failure.mapping_kind`): what refuses the update is the value stored, as a unique rule or a transition's `when:` refuses a write, and the same request may succeed once the record changes (409, as area 4's `OutOfStock` is). `invalid:` covers every rule without an arm, as on any construction, and the write fails with each failure the arms name, passed on by its `attempt` |
| Rules the statement checks (2026-11-12, chunk Q2b, the obvious recommendation) | `min` and `max` of an `Int` or a `Decimal`, of an amount of money in its minor units and of a `Duration` in its milliseconds, and `min_length` and `max_length` of text with `char_length`, which counts Unicode characters as Jadpo does; each bound along the type's chain, written into the statement as a number fixed while compiling. Any other rule a construction can break, a `pattern` or a `format`, is `data.stored_value_unproven` whether or not an arm maps it, since PostgreSQL's regular expressions and formats aren't Jadpo's; so is a rule no arm maps. The error's `next:` names the block first, since it keeps the update one step |
| Telling a refusal from a missing record (2026-11-12, chunk Q2b, the obvious recommendation) | No row updated sends a second statement, `SELECT` of whether each rule holds of the value worked out now, a column each, from the record by `id` under its lifecycle and policy (and the fields' grants, as a transition's guard asks): no row is the not-found failure or policy's refusal as for any update by `id`, and the first column that doesn't hold, in the order `set:` gives the constructions and each one's rules, is that arm's failure. A row whose every rule holds changed between the two statements, so it is a concurrent change and the action runs again, which only a change that undoes the refusal (a restock) meanwhile can cause. Since the probe asks again rather than reading the update's snapshot, the update loads no record and the action stays at read committed, unlike a guarded transition: two buyers racing on the last item both end at their first attempt, the second refused. Reversible: a later chunk may ask both in one statement |
| `none` to a field that can't be cleared | `none` given to an optional `Input` field whose type isn't nullable is the same as leaving it out, so a patch keeps the stored value |
| Refusals in a called action | An action whose failure its caller handles with `match` runs inside a savepoint, so its failure undoes all it wrote, a refusal among it, and the caller carries on |
| Transitions as operations | `Entity.<transition>(record or id)`, an update of the fields its `set:` gives, giving back the record (nothing through `delete`); its block takes only `conflict:`; it reaches only visible records |
| A transition's `when:` refusing | A `when:` that asks more than `visible:` can refuse a record that is there, a conflict mapped in `conflict:` keyed by the transition (`User.disable: UserDisabled`), with no catch-all; one that asks only what `visible:` asks can't, since a hidden record is not found |
| Counter decimals | A decimal added to or subtracted from a stored field in a counter update may not have more decimal places than the field's `scale` (`data.stored_decimal_scale`), a value whose type sets no `scale` counting as too many |
| Isolation by what an action does | An action that loads no record, itself or through any action it calls, runs at read committed, so concurrent counters on one row both commit without a rerun; any other action runs at repeatable read |
| What loads a record | Reading one (`get`, a query and its includes); being handed one (a parameter whose type holds a record, which area 5 calls a loaded record, so a record authentication loads for an action counts too); writing one back (an `update`, `delete` or transition of a record rather than its `id`, wherever the record came from); and a transition whose `when:` can refuse a visible record, whose second statement must share the first one's snapshot. A create (its references checked by the store in the same statement), an update, delete or unguarded transition by `id`, a counter and an update that refuses load nothing |
| Isolation of fixtures and tests | A fixture is decided as an action is; each action or fixture a test calls begins its own transaction at its own level; a read or write a test runs itself, and whatever else runs outside an action (migrations, purge), runs at repeatable read |
| Outside calls and the transaction | No transaction is open during an outside call: reads before it run in a read-only snapshot closed before the call, the transaction opens after it, and a write to a record loaded before the call checks its row version, a change rerunning the action |
| Several outside calls | Writes after the last call; no transaction during calls; the step record in the runtime's own short transactions, deleted in the commit; a rerun replays with the record's `clock.now` and stops as `diverged` when a fresh read changes a request; a write refusable after a step took effect is warned (area 12) |
| Writes before calls | A write placed before a later outside call, on any way through the body, directly or through an action it calls, is `data.write_before_call`, not declinable, at the write and naming the last call it comes before; starting or cancelling a run writes, and a run's `each:` and `finished:` are the run's |
| Entities in Redis | Get, create, update and delete by `id`; lookup by a `unique:` field; equality on one indexed field, paged in creation order, and its count; revision checks; simple-equality grants checked after reading; everything else a compile error naming PostgreSQL; expired records hidden |
| Redis counters and lifecycles | Counter updates by `id` allowed as one Redis `increment`; no lifecycles on a Redis entity |
| Constants in queries and updates | A `constant` (area 3) is a value fixed while compiling, so a query accepts one wherever it accepts a literal (`query.value_unsupported` names constants among a query's values), and the "constants" an update by `id` may use are these. Arithmetic in a query stays refused, on constants too |
| Aggregate queries | `group:` and `aggregate:` blocks shaped like `include:`; `count()`, `sum`, `min`, `max` and `average(F, scale:, rounding:)`, each what code gives; groups of exactly-comparing fields and time buckets of dates; `List<T>` with `group:`, `T` without; one statement, as "Counts, sums and groups" sets out |
| Standard operations in `where:` | Allowed where the operation has an exact SQL form, with a derived expression index, as "Operations in conditions" sets out |
| A query giving a map layer | A query may return its records as a `FeatureCollection<G, P>`, naming the geometry field and the properties type, so a `GET` route can send a layer; nothing is computed beyond picking fields |
| Values kept in several columns | Decided 2026-11-11, recommended for a maintainer to confirm: a standard type in named columns is equal when every column is, a part that may be `none` compared with `IS NOT DISTINCT FROM`; ordered only where the contract orders it part by part and every column is a number, a date or a time that is always there (`YearMonth`, `LocalTime`, `CardExpiry`), by a row comparison; never a `SemanticVersion`, whose pre-release text doesn't sort by precedence; a type holding a geometry or a vector isn't compared, since PostGIS's `=` compares bounding boxes. `in` a list given as a parameter sends each part as an array read back with `unnest`, so it takes only types whose parts are always there and can be sent as arrays; a list written out takes any |
| Money in a query | Decided 2026-11-11, recommended for a maintainer to confirm: `==` and `!=` compare both columns, so amounts in two currencies are unequal, as code's `==` says; `<` and `order:` compare minor units only where the field's type fixes its currency (the value is of that type, since two types of money are compared only where one widens to the other), the proof code needs without `attempt`, since a query can't map `currency_mismatch` for a record. Conditions as proof (`money.currency(F) == ...` beside the ordering) were left out: `money.currency` of the field is a condition of its own, and the cheaper rule covers every app that fixes its currency |
| Values kept as JSON | Decided 2026-11-11, recommended for a maintainer to confirm: compared by `jsonb`'s `=` with the JSON the field's own codec writes for a parameter or an enum value when the query runs, which is exactly code's `==` because the codec writes one spelling for each value; never ordered. A type holding a decimal, a percentage or a float whose places its type doesn't fix is refused, since `1.5` and `1.50` are `==` but differ as JSON; so is a list written out, which only the codec could write. A row written outside Jadpo in another spelling of the same value (a set out of its canonical order) is compared as written, as the area page's rows written around Jadpo are |
| Indexes over held values | Decided 2026-11-11: a derived index holds every column of a value in named columns a condition tests with `==` or `in`, in its layout's order, and the columns its order reads of one it sorts by (an amount's minor units), and no column of one kept as JSON, whose up to 1 MiB a B-tree's entry of about 2.7 kB can't hold; an expression index over an amount's operation records both its columns as what it reads |
