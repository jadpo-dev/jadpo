# `query.limit_range`

A query's `limit:` could be less than 1, either as written or because its value's type allows it.

## Why Jadpo refuses it

`limit:` is the most records a list query gives back. A limit of 0 would
read nothing, and PostgreSQL refuses a negative one. So a limit is at
least 1: a number written from 1, or a value whose type says `min: 1` or
more.

## What it looks like

main.jadpo:

```jadpo check project=e-limit-range expect=query.limit_range
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf() -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: 0
}
```

```text output
error: `limit: 0` would read nothing
 --> main.jadpo:15:12
   |
15 |     limit: 0
   |            ^ below 1
  reason: `limit:` is how many records come back at most, so it is at least 1.
  next: Give `limit:` a number from 1 (agent)
  rule: query.limit_range
  help: https://jadpo.dev/docs/errors/query.limit_range

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A limit given as a parameter is refused when its type lets it be below 1.
Here `ShelfSize` has a `max:` but no `min:`:

main.jadpo:

```jadpo check project=e-limit-range-type expect=query.limit_range
type Title = Text {
    max_length: 80
}

type ShelfSize = Int {
    max: 50
}

entity Book {
    id: Uuid
    title: Title
}

query shelf(size: ShelfSize) -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: size
}
```

```text output
error: `size` can be below 1, so the limit could read nothing
 --> main.jadpo:17:12
   |
17 |     limit: size
   |            ^^^^ `ShelfSize` has no `min:`
  reason: `limit:` is how many records come back at most, so it is at least 1, and a value's type must say so with `min: 1` or more, as the built-in `PageSize` (1 to 100) does.
  next: Give `ShelfSize` `min: 1`, or type the value as `PageSize` (agent)
  rule: query.limit_range
  help: https://jadpo.dev/docs/errors/query.limit_range

check failed: 1 error, 1 file
```

## How to fix it

**If the limit is written as a number**, write the most records the list
should give, from 1:

```jadpo fragment
    limit: 50
```

```jadpo check project=e-limit-range-fix-1 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf() -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: 50
}
```

`shelf` gives up to 50 books, in title order.

**If the limit is a parameter**, give its type `min: 1`. `main.jadpo`
becomes:

```jadpo check project=e-limit-range-type-fix-2
type Title = Text {
    max_length: 80
}

type ShelfSize = Int {
    min: 1
    max: 50
}

entity Book {
    id: Uuid
    title: Title
}

query shelf(size: ShelfSize) -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: size
}
```

`shelf` gives up to `size` books. A number outside 1 to 50 can't be a
`ShelfSize`, so it never reaches the query.

If 1 to 100 suits the list, the parameter can be typed as the built-in
`PageSize` instead, and `ShelfSize` isn't needed.

## Related

- [Order and pages](../language/06-reads-and-writes.md#order-and-pages)
- [How many records come back](../language/06-reads-and-writes.md#how-many-records-come-back)
