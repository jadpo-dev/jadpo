# `query.value_unsupported`

A query uses a value it would have to work out, or reads inside one that may be absent.

## Why Jadpo refuses it

A query is one statement the database runs, and what it compares with is
what the call supplies: parameters and their fields, literals, constants,
enum values, lists of those and `clock.now`. Arithmetic in a condition would
be done by the database, by its rules rather than your code's, so it stays
in your code.

## What it looks like

main.jadpo:

```jadpo check project=e-value-unsupported expect=query.value_unsupported
type PageCount = Int {
    min: 1
    max: 5000
}

entity Manual {
    id: Uuid
    page_count: PageCount
}

query longer_manuals(manual_pages: Manual.page_count) -> List<Manual> {
    from: Manual
    where: Manual.page_count > manual_pages * 2
    order: [Manual.page_count asc]
    limit: 20
}
```

```text output
error: `manual_pages * 2` is worked out, and a query computes nothing
 --> main.jadpo:13:32
   |
13 |     where: Manual.page_count > manual_pages * 2
   |                                ^^^^^^^^^^^^^^^^ computed
  reason: A query's values are its parameters and their fields, literals, constants, enum values, lists of those and `clock.now`; anything worked out from them is computed before the call.
  next: Work the value out in the action, and pass it as a parameter (agent)
  rule: query.value_unsupported
  help: https://jadpo.dev/docs/errors/query.value_unsupported

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A query reads a parameter's fields only through values that are always
there, since a value that may be `none` has no field to read:

main.jadpo:

```jadpo check project=e-value-unsupported-optional expect=query.value_unsupported
type PageCount = Int {
    min: 1
    max: 5000
}

entity Manual {
    id: Uuid
    page_count: PageCount
}

type Search = Input {
    pages: Manual.page_count
}

query long_manuals(search: Search?) -> List<Manual> {
    from: Manual
    where_present: Manual.page_count > search.pages
    order: [Manual.page_count asc]
    limit: 20
}
```

```text output
error: `search.pages` may be `none`, so `pages` can't be read
 --> main.jadpo:17:40
   |
17 |     where_present: Manual.page_count > search.pages
   |                                        ^^^^^^^^^^^^ inside a value that may be `none`
  reason: A query reads a parameter's fields only through values that are always there.
  next: Pass the inner value as a parameter of its own (agent)
  rule: query.value_unsupported
  help: https://jadpo.dev/docs/errors/query.value_unsupported

check failed: 1 error, 1 file
```

## How to fix it

**If the value depends on the call**, work it out in the action, and pass it as a parameter:

```jadpo fragment
query longer_manuals(at_least: Manual.page_count) -> List<Manual> {
    from: Manual
    where: Manual.page_count > at_least
    ...
}
```

```jadpo check project=e-value-unsupported-fix-1 hidden
type PageCount = Int {
    min: 1
    max: 5000
}

entity Manual {
    id: Uuid
    page_count: PageCount
}

query longer_manuals(at_least: Manual.page_count) -> List<Manual> {
    from: Manual
    where: Manual.page_count > at_least
    order: [Manual.page_count asc]
    limit: 20
}
```

The action decides what "longer" means, and the query compares with its answer.

**If the value is fixed**, give it a constant of its own, which counts as a literal:

```jadpo fragment
constant longest_manual: PageCount = 900
```

```jadpo check project=query-constant hidden
type PageCount = Int {
    min: 1
    max: 5000
}

entity Manual {
    id: Uuid
    page_count: PageCount
}

constant longest_manual: PageCount = 900

query long_manuals() -> List<Manual> {
    from: Manual
    where: Manual.page_count > longest_manual
    order: [Manual.page_count asc]
    limit: 20
}
```

Arithmetic on a constant inside the query is still worked out, and still this error, so the constant holds the result.

**If the value may be absent**, pass the inner value as a parameter of its own:

```jadpo fragment
query long_manuals(pages: Manual.page_count?) -> List<Manual> {
    ...
    where_present: Manual.page_count > pages
    ...
}
```

```jadpo check project=e-value-unsupported-fix-3 hidden
type PageCount = Int {
    min: 1
    max: 5000
}

entity Manual {
    id: Uuid
    page_count: PageCount
}

query long_manuals(pages: Manual.page_count?) -> List<Manual> {
    from: Manual
    where_present: Manual.page_count > pages
    order: [Manual.page_count asc]
    limit: 20
}
```

The condition now applies only when a call gives `pages`.

## Related

- [Filters](../language/06-reads-and-writes.md#filters)
- [`query.optional_value`](query.optional_value.md)
