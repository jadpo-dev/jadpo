# `query.result_type`

A query returns something other than its entity or a type its fields can fill.

## Why Jadpo refuses it

A query reads whole records, and its result says how many come back and what
each holds: the entity itself or an object type its fields fill. A plain
value isn't a record, so it has no fields for the query to fill. An
aggregate query's rows are groups and totals, so they need a type of their
own, and the entity won't do.

## What it looks like

main.jadpo:

```jadpo check project=e-result-type expect=query.result_type
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query titles(title: Book.title) -> List<Title> {
    from: Book
    where: Book.title == title
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `titles` reads `Book` records but returns `List<Title>`
 --> main.jadpo:12:36
   |
12 | query titles(title: Book.title) -> List<Title> {
   |                                    ^^^^^^^^^^^ not records of the entity
  reason: A query's result is `Book` or an object type its fields fill (`type BookView = Output { pick: [...] }`), as `T` for exactly one record, `T?` for at most one, `List<T>` or `Page<T>`, or a map layer, `FeatureCollection<G, P>`.
  next: Return `Book`, or an `Output` type that picks the fields you need (agent)
  rule: query.result_type
  help: https://jadpo.dev/docs/errors/query.result_type

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An aggregate query can't return the entity, since a row of totals isn't a
record:

main.jadpo:

```jadpo check project=e-result-type-aggregate expect=query.result_type
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf_count() -> Book {
    from: Book
    aggregate: {
        books: count()
    }
}
```

```text output
error: `shelf_count` counts `Book` records but returns them
 --> main.jadpo:12:24
   |
12 | query shelf_count() -> Book {
   |                        ^^^^ records, not totals
  reason: An aggregate query's rows are groups and totals, not `Book` records: values of an `Output` type with a field for each group and aggregate, by name.
  next: Return an `Output` type with a field for each group and aggregate, such as `type BookTotals = Output { ... }` (agent)
  rule: query.result_type
  help: https://jadpo.dev/docs/errors/query.result_type

check failed: 1 error, 1 file
```

## How to fix it

**If you want the field's values**, return a type that picks it:

```jadpo fragment
type BookTitle = Output {
    pick: [Book.title]
}

query titles(title: Book.title) -> List<BookTitle> {
    ...
}
```

```jadpo check project=e-result-type-fix-1 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type BookTitle = Output {
    pick: [Book.title]
}

query titles(title: Book.title) -> List<BookTitle> {
    from: Book
    where: Book.title == title
    order: [Book.id asc]
    limit: 20
}
```

The query returns only each book's title.

**If it is an aggregate query**, return an `Output` type with a field for each total:

```jadpo fragment
type ShelfCount = Output {
    books: Int
}

query shelf_count() -> ShelfCount {
    ...
}
```

```jadpo check project=e-result-type-fix-2 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
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

`books` now fills the field of its name.

## Related

- [How many records come back](../language/06-reads-and-writes.md#how-many-records-come-back)
- [What a result holds](../language/06-reads-and-writes.md#what-a-result-holds)
- [The result](../language/06-reads-and-writes.md#the-result)
