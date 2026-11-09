# `query.limit_unbounded`

A query's `limit:` or page `size:` has no upper bound.

## Why Jadpo refuses it

Every list is bounded, so that no query can read a whole table as it grows.
A `limit:` or a page `size:` is a number, or a value whose type has a
`max:`. A value with no maximum is a limit in name only: any caller could
ask for everything.

## What it looks like

main.jadpo:

```jadpo check project=e-limit-unbounded expect=query.limit_unbounded
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type Count = Int {
    min: 1
}

query shelf(count: Count) -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: count
}
```

```text output
error: `count` has no maximum, so the limit has no bound
 --> main.jadpo:19:12
   |
19 |     limit: count
   |            ^^^^^ `Count` has no `max:`
  reason: Every list is bounded: `limit:` is a number, or a value whose type has a `max:`, such as the built-in `PageSize` (1 to 100).
  next: Type the value as `PageSize`, or as an `Int` type of your own with a `max:` (agent)
  rule: query.limit_unbounded
  help: https://jadpo.dev/docs/errors/query.limit_unbounded

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A page's `size:` is bounded the same way:

main.jadpo:

```jadpo check project=e-limit-unbounded-page expect=query.limit_unbounded
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type Count = Int {
    min: 1
}

query shelf(count: Count, after: PageCursor?) -> Page<Book> {
    from: Book
    order: [Book.title asc]
    page: {
        size: count
        after: after
    }
}
```

```text output
error: `count` has no maximum, so the size has no bound
 --> main.jadpo:20:15
   |
20 |         size: count
   |               ^^^^^ `Count` has no `max:`
  reason: Every list is bounded: `size:` is a number, or a value whose type has a `max:`, such as the built-in `PageSize` (1 to 100).
  next: Type the value as `PageSize`, or as an `Int` type of your own with a `max:` (agent)
  rule: query.limit_unbounded
  help: https://jadpo.dev/docs/errors/query.limit_unbounded

check failed: 1 error, 1 file
```

## How to fix it

**If callers choose the size**, type the value as `PageSize`, which runs from 1 to 100:

```jadpo fragment
query shelf(count: PageSize) -> List<Book> {
    ...
}
```

```jadpo check project=e-limit-unbounded-fix-1 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf(count: PageSize) -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: count
}
```

The query reads at most 100 records, whatever a caller asks for.

**If you need another bound**, give the type a `max:`:

```jadpo fragment
type Count = Int {
    ...
    max: 50
}
```

```jadpo check project=e-limit-unbounded-fix-2 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type Count = Int {
    min: 1
    max: 50
}

query shelf(count: Count) -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: count
}
```

A value above 50 is refused when the `Count` is made, before the query runs.

## Related

- [Order and pages](../language/06-reads-and-writes.md#order-and-pages)
- [Pages](../language/06-reads-and-writes.md#pages)
- [`query.limit_range`](query.limit_range.md)
