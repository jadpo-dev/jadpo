# `query.aggregate_argument`

An aggregate in a query is missing an argument it needs, or is given one it doesn't take.

## Why Jadpo refuses it

Each aggregate is worked out by the database over the records of a row, so
its arguments say exactly what to work out: `count()` takes none, `sum`,
`min` and `max` take one field of the entity, and `average` takes the field
with `scale:` and `rounding:` as well.

An average rarely comes out exact. Jadpo won't choose how many places it
keeps or how the last one is rounded, because that decides the number people
read.

## What it looks like

main.jadpo:

```jadpo check project=e-aggregate-argument expect=query.aggregate_argument
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
        mean_leaves: average(Book.leaves)
    }
}
```

```text output
error: `average` needs `scale:` and `rounding:`
 --> main.jadpo:17:22
   |
17 |         mean_leaves: average(Book.leaves)
   |                      ^^^^^^^^^^^^^^^^^^^^ wrong arguments
  reason: An average rarely comes out exact, so it says how many places it keeps and how the last one is rounded, both written out: `average(Book.<field>, scale: 2, rounding: Rounding.half_even)`.
  next: Add `scale:` and `rounding:` (agent)
  rule: query.aggregate_argument
  help: https://jadpo.dev/docs/errors/query.aggregate_argument

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`count()` counts records, whatever their fields hold, so it takes no
arguments:

main.jadpo:

```jadpo check project=e-aggregate-argument-count expect=query.aggregate_argument
type LeafCount = Int {
    min: 1
}

entity Book {
    id: Uuid
    leaves: LeafCount
}

type ShelfCount = Output {
    books: Int
}

query shelf_count() -> ShelfCount {
    from: Book
    aggregate: {
        books: count(Book.leaves)
    }
}
```

```text output
error: `count` takes no arguments
 --> main.jadpo:17:16
   |
17 |         books: count(Book.leaves)
   |                ^^^^^^^^^^^^^^^^^^ wrong arguments
  reason: `count()` counts the records in each row, whatever their fields hold; to count only the records whose field has a value, add `where: Book.<field> != none`.
  next: Write `count()` (agent)
  rule: query.aggregate_argument
  help: https://jadpo.dev/docs/errors/query.aggregate_argument

check failed: 1 error, 1 file
```

`sum`, `min` and `max` work out over one field, named with its entity:

main.jadpo:

```jadpo check project=e-aggregate-argument-field expect=query.aggregate_argument
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
        total_leaves: sum(leaves)
    }
}
```

```text output
error: `sum` takes one field of `Book`
 --> main.jadpo:17:23
   |
17 |         total_leaves: sum(leaves)
   |                       ^^^^^^^^^^^ wrong arguments
  reason: `sum` works out over one field of the records, named with its entity: `sum(Book.<field>)`.
  next: Write `sum(Book.<field>)` (agent)
  rule: query.aggregate_argument
  help: https://jadpo.dev/docs/errors/query.aggregate_argument

check failed: 1 error, 1 file
```

`scale:` is a whole number written out, from 0 to 38, the most digits a
decimal holds:

main.jadpo:

```jadpo check project=e-aggregate-argument-scale expect=query.aggregate_argument
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
        mean_leaves: average(Book.leaves, scale: 40, rounding: Rounding.half_even)
    }
}
```

```text output
error: `scale: 40` isn't a number of places
 --> main.jadpo:17:50
   |
17 |         mean_leaves: average(Book.leaves, scale: 40, rounding: Rounding.half_even)
   |                                                  ^^ not 0 to 38
  reason: `scale:` is how many places the average keeps: a whole number written out, from 0 to 38, as a decimal holds at most 38 digits.
  next: Write `scale: 2`, or the places the report needs (agent)
  rule: query.aggregate_argument
  help: https://jadpo.dev/docs/errors/query.aggregate_argument

check failed: 1 error, 1 file
```

`rounding:` is a `Rounding` value written out:

main.jadpo:

```jadpo check project=e-aggregate-argument-rounding expect=query.aggregate_argument
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
        mean_leaves: average(Book.leaves, scale: 1, rounding: "half_even")
    }
}
```

```text output
error: `rounding: "half_even"` isn't a rounding
 --> main.jadpo:17:63
   |
17 |         mean_leaves: average(Book.leaves, scale: 1, rounding: "half_even")
   |                                                               ^^^^^^^^^^^ not a `Rounding`
  reason: `rounding:` is a `Rounding` written out: `Rounding.half_even`, `Rounding.half_up`, `Rounding.up` or `Rounding.down`.
  next: Write `rounding: Rounding.half_even`, or the rounding the report needs (agent)
  rule: query.aggregate_argument
  help: https://jadpo.dev/docs/errors/query.aggregate_argument

check failed: 1 error, 1 file
```

## How to fix it

**If it is an average**, say how many places it keeps and how the last one is rounded:

```jadpo fragment
    aggregate: {
        mean_leaves: average(Book.leaves, scale: 1, rounding: Rounding.half_even)
    }
```

```jadpo check project=e-aggregate-argument-fix-1 hidden
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
        mean_leaves: average(Book.leaves, scale: 1, rounding: Rounding.half_even)
    }
}
```

The query now returns the mean to one place, with a tie going to the even digit.

**If it counts records**, write `count()` with nothing inside:

```jadpo fragment
    aggregate: {
        books: count()
    }
```

```jadpo check project=e-aggregate-argument-fix-2 hidden
type LeafCount = Int {
    min: 1
}

entity Book {
    id: Uuid
    leaves: LeafCount
}

type ShelfCount = Output {
    books: Int
}

query shelf_count() -> ShelfCount {
    from: Book
    aggregate: {
        books: count()
    }
}
```

Every record in the row is counted, whatever its fields hold. To count only those whose field has a value, add `where: Book.<field> != none`.

**If it adds up a field**, name the field with its entity:

```jadpo fragment
    aggregate: {
        total_leaves: sum(Book.leaves)
    }
```

```jadpo check project=e-aggregate-argument-fix-3 hidden
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

`min` and `max` take a field the same way.

## Related

- [The aggregates](../language/06-reads-and-writes.md#the-aggregates)
- [Counts, sums and groups](../language/06-reads-and-writes.md#counts-sums-and-groups)
