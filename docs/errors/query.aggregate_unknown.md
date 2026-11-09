# `query.aggregate_unknown`

An `aggregate:` section isn't a block of `count`, `sum`, `min`, `max` or `average` entries.

## Why Jadpo refuses it

A query's `aggregate:` block names each total after the result field it
fills, and each total is one of five aggregates the database can work out
exactly: `count()`, `sum`, `min`, `max` and `average`. Anything else, such
as a name another language uses or a field written on its own, has no agreed
meaning over a group of records.

## What it looks like

main.jadpo:

```jadpo check project=e-aggregate-unknown expect=query.aggregate_unknown
type LeafCount = Int {
    min: 1
}

entity Book {
    id: Uuid
    leaves: LeafCount
}

type ShelfLength = Output {
    mean_leaves: Decimal?
}

query shelf_length() -> ShelfLength {
    from: Book
    aggregate: {
        mean_leaves: avg(Book.leaves, scale: 1, rounding: Rounding.half_up)
    }
}
```

```text output
error: `avg` isn't an aggregate
 --> main.jadpo:17:22
   |
17 |         mean_leaves: avg(Book.leaves, scale: 1, rounding: Rounding.half_up)
   |                      ^^^ not an aggregate
  reason: An aggregate is `count()`, `sum(F)`, `min(F)`, `max(F)` or `average(F, scale:, rounding:)`, each `F` a field of `Book`. Did you mean `average`?
  next: Use `average` (agent)
  rule: query.aggregate_unknown
  help: https://jadpo.dev/docs/errors/query.aggregate_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A field on its own holds a different value on each record of a row, so it
isn't a total:

main.jadpo:

```jadpo check project=e-aggregate-unknown-field expect=query.aggregate_unknown
type LeafCount = Int {
    min: 1
}

entity Book {
    id: Uuid
    leaves: LeafCount
}

type ShelfTotal = Output {
    total_leaves: Int?
}

query shelf_total() -> ShelfTotal {
    from: Book
    aggregate: {
        total_leaves: Book.leaves
    }
}
```

```text output
error: `Book.leaves` isn't an aggregate
 --> main.jadpo:17:23
   |
17 |         total_leaves: Book.leaves
   |                       ^^^^^^^^^^^ not an aggregate
  reason: An aggregate is `count()`, `sum(F)`, `min(F)`, `max(F)` or `average(F, scale:, rounding:)`, each `F` a field of `Book`. A field's own value, the same for every record of a row, is a group.
  next: Write `sum(Book.leaves)`, or move it to `group:` (agent)
  rule: query.aggregate_unknown
  help: https://jadpo.dev/docs/errors/query.aggregate_unknown

check failed: 1 error, 1 file
```

`aggregate:` is always a block, with a name for each total:

main.jadpo:

```jadpo check project=e-aggregate-unknown-block expect=query.aggregate_unknown
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
}

type TitleRow = Output {
    title: Title
}

query title_rows() -> List<TitleRow> {
    from: Book
    group: {
        title: Book.title
    }
    aggregate: count()
    order: [title asc]
    limit: 20
}
```

```text output
error: `aggregate:` is a block
 --> main.jadpo:19:5
   |
19 |     aggregate: count()
   |     ^^^^^^^^^ not a block
  reason: `aggregate:` names each of its entries after the result field it fills, in a block: `aggregate: { <name>: count() }`.
  next: Write `aggregate: { <name>: count() }` (agent)
  rule: query.aggregate_unknown
  help: https://jadpo.dev/docs/errors/query.aggregate_unknown

check failed: 1 error, 1 file
```

## How to fix it

**If the name is misspelt**, use the aggregate's own name:

```jadpo fragment
        mean_leaves: average(Book.leaves, scale: 1, rounding: Rounding.half_up)
```

```jadpo check project=e-aggregate-unknown-fix-1 hidden
type LeafCount = Int {
    min: 1
}

entity Book {
    id: Uuid
    leaves: LeafCount
}

type ShelfLength = Output {
    mean_leaves: Decimal?
}

query shelf_length() -> ShelfLength {
    from: Book
    aggregate: {
        mean_leaves: average(Book.leaves, scale: 1, rounding: Rounding.half_up)
    }
}
```

The query now returns the mean, to one place.

**If you meant the field's total**, name an aggregate around it:

```jadpo fragment
        total_leaves: sum(Book.leaves)
```

```jadpo check project=e-aggregate-unknown-fix-2 hidden
type LeafCount = Int {
    min: 1
}

entity Book {
    id: Uuid
    leaves: LeafCount
}

type ShelfTotal = Output {
    total_leaves: Int?
}

query shelf_total() -> ShelfTotal {
    from: Book
    aggregate: {
        total_leaves: sum(Book.leaves)
    }
}
```

To get one row for each value of the field instead, move it to `group:`.

**If `aggregate:` isn't a block**, name the total after the result's field:

```jadpo fragment
    aggregate: {
        copies: count()
    }
```

```jadpo check project=e-aggregate-unknown-fix-3 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
}

type TitleRow = Output {
    title: Title
    copies: Int
}

query title_rows() -> List<TitleRow> {
    from: Book
    group: {
        title: Book.title
    }
    aggregate: {
        copies: count()
    }
    order: [title asc]
    limit: 20
}
```

`copies` now fills the field of that name in `TitleRow`, which needs one for it.

## Related

- [The aggregates](../language/06-reads-and-writes.md#the-aggregates)
- [The groups](../language/06-reads-and-writes.md#the-groups)
