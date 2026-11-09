# `query.unknown_section`

A query's block holds a section Jadpo doesn't know.

## Why Jadpo refuses it

A query's block holds a fixed list of sections, so a misspelt section can't
quietly do nothing. Anything else in the block is a mistake to correct, and
the error lists the sections that can be there.

## What it looks like

main.jadpo:

```jadpo check project=e-unknown-section expect=query.unknown_section
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query books_titled(title: Book.title) -> List<Book> {
    from: Book
    where: Book.title == title
    order_by: [Book.id asc]
    limit: 20
}
```

```text output
error: `order_by` isn't something a query holds
 --> main.jadpo:15:5
   |
15 |     order_by: [Book.id asc]
   |     ^^^^^^^^ not a section
  reason: A query's block holds `from:`, `geometry:`, `where:`, `where_present:`, `group:`, `aggregate:`, `order:`, `limit:`, `page:`, `include:`, `missing:` and `include_erased:`. Did you mean `order:`?
  next: Use `order:` (agent)
  rule: query.unknown_section
  help: https://jadpo.dev/docs/errors/query.unknown_section

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A section Jadpo has no equivalent for is refused the same way, with no
suggestion:

main.jadpo:

```jadpo check project=e-unknown-section-other expect=query.unknown_section
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query books_titled(title: Book.title) -> List<Book> {
    from: Book
    where: Book.title == title
    order: [Book.id asc]
    distinct: true
    limit: 20
}
```

```text output
error: `distinct` isn't something a query holds
 --> main.jadpo:16:5
   |
16 |     distinct: true
   |     ^^^^^^^^ not a section
  reason: A query's block holds `from:`, `geometry:`, `where:`, `where_present:`, `group:`, `aggregate:`, `order:`, `limit:`, `page:`, `include:`, `missing:` and `include_erased:`.
  next: Remove it from `books_titled` (agent)
  rule: query.unknown_section
  help: https://jadpo.dev/docs/errors/query.unknown_section

check failed: 1 error, 1 file
```

## How to fix it

**If it is misspelt**, use the section's own name:

```jadpo fragment
    order: [Book.id asc]
```

```jadpo check project=e-unknown-section-fix-1 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query books_titled(title: Book.title) -> List<Book> {
    from: Book
    where: Book.title == title
    order: [Book.id asc]
    limit: 20
}
```

The query's records now come back in `id` order.

**If you meant to remove duplicates**, read one record per value with a group:

```jadpo fragment
    group: {
        title: Book.title
    }
```

```jadpo check project=e-unknown-section-fix-2 hidden
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

query titles() -> List<TitleRow> {
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

The query returns one row for each title, with how many records have it.

## Related

- [Named queries](../language/06-reads-and-writes.md#named-queries)
- [`query.repeated_section`](query.repeated_section.md)
