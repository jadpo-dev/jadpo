# `query.order_field`

A query sorts records by a field, such as a `Bool` or an enum, that the database and Jadpo don't order the same way.

## Why Jadpo refuses it

A query's `order:` is carried out by the database, and Jadpo compares
the same values itself, so both must order them the same way. Numbers,
times, dates, durations, quantities, percentages, text and identities
sort the same way in both.

An enum, a `Bool` or a structured field doesn't. The database could put
`true` first where Jadpo puts it last, so records would come back in an
order the code can't rely on.

## What it looks like

main.jadpo:

```jadpo check project=e-order-field expect=query.order_field
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
    order: [Book.available desc]
    limit: 20
}
```

```text output
error: Records can't be sorted by `Book.available`
 --> main.jadpo:14:13
   |
14 |     order: [Book.available desc]
   |             ^^^^^^^^^^^^^^^^^^^ `Bool`
  reason: A query sorts by numbers, times, dates, durations, quantities, percentages, text and identities, which the database and Jadpo order the same way; `Book.available` is `Bool`.
  next: Sort by another field (agent)
  rule: query.order_field
  help: https://jadpo.dev/docs/errors/query.order_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the shelf only needs the available books**, keep them with a
`where:` and sort by a field that has an order:

```jadpo fragment
query shelf() -> List<Book> {
    from: Book
    where: Book.available == true
    order: [Book.title asc]
    limit: 20
}
```

```jadpo check project=e-order-field-fix-1 hidden
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
    where: Book.available == true
    order: [Book.title asc]
    limit: 20
}
```

`shelf` gives up to 20 available books, in title order. Books that tie
on title are ordered by `id`, which Jadpo adds as the last field.

**If the shelf shows every book**, sort by another field and leave
`available` to the result:

```jadpo fragment
    order: [Book.title asc]
```

```jadpo check project=e-order-field-fix-2 hidden
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
    limit: 20
}
```

`shelf` gives up to 20 books in title order, each with its `available`
field for the caller to show.

## Related

- [Order and pages](../language/06-reads-and-writes.md#order-and-pages)
- [What can be sorted](../language/06-reads-and-writes.md#what-can-be-sorted)
- [Filters](../language/06-reads-and-writes.md#filters)
