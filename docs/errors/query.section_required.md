# `query.section_required`

A query leaves out a section its result type needs, such as `from:`, or the `limit:` that bounds a list.

## Why Jadpo refuses it

A query's result type says how many records come back, and each kind of
result needs sections of its own. Every query names the entity it reads
in `from:`. A list needs `order:` and `limit:`, because every list is
bounded: a query that could read a whole table would grow slower as the
table grows, with nothing in the code to show it.

## What it looks like

main.jadpo:

```jadpo check project=e-section-required expect=query.section_required
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
}
```

```text output
error: `books_titled` returns `List<Book>` but has no `limit:`
 --> main.jadpo:12:7
   |
12 | query books_titled(title: Book.title) -> List<Book> {
   |       ^^^^^^^^^^^^ no `limit:`
  reason: Every list is bounded: `limit:` is a number, or a value whose type has a `max:`.
  next: Add `limit: <number>`, or return `Page<T>` to read it a page at a time (agent)
  rule: query.section_required
  help: https://jadpo.dev/docs/errors/query.section_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A query with no `from:` is refused in the same way, since nothing says
which entity its conditions and order are about:

main.jadpo:

```jadpo check project=e-section-required-from expect=query.section_required
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
    where: Book.title == title
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `books_titled` doesn't say what it reads
 --> main.jadpo:12:7
   |
12 | query books_titled(title: Book.title) -> List<Book> {
   |       ^^^^^^^^^^^^ no `from:`
  reason: A query reads one entity, named in `from:`; its conditions, order and includes are about that entity's records.
  next: Add `from: <Entity>` as the query's first line (agent)
  rule: query.section_required
  help: https://jadpo.dev/docs/errors/query.section_required

check failed: 1 error, 1 file
```

## How to fix it

**If a fixed number of books is enough**, add `limit:`:

```jadpo fragment
    order: [Book.id asc]
    limit: 20
```

```jadpo check project=e-section-required-fix-1 hidden
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

The query gives up to 20 books with that title.

**If the caller should be able to read every one**, return a `Page<Book>`
and read it a page at a time. The caller passes the page size and the
cursor from the page before:

```jadpo fragment
query books_titled(title: Book.title, size: PageSize, after: PageCursor?) -> Page<Book> {
    from: Book
    where: Book.title == title
    order: [Book.id asc]
    page: {
        size: size
        after: after
    }
}
```

```jadpo check project=e-section-required-fix-2 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query books_titled(title: Book.title, size: PageSize, after: PageCursor?) -> Page<Book> {
    from: Book
    where: Book.title == title
    order: [Book.id asc]
    page: {
        size: size
        after: after
    }
}
```

Each call gives up to `size` books, at most 100, and a `next` cursor
while more remain. Passing that cursor back as `after` gives the next
page.

**If the query has no `from:`**, add it as the first line, naming the
entity it reads:

```jadpo fragment
    from: Book
```

```jadpo check project=e-section-required-from-fix-3 hidden
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

The query reads `Book` records, and its `where:` and `order:` test their
fields.

## Related

- [Named queries](../language/06-reads-and-writes.md#named-queries)
- [How many records come back](../language/06-reads-and-writes.md#how-many-records-come-back)
- [Order and pages](../language/06-reads-and-writes.md#order-and-pages)
- [Pages](../language/06-reads-and-writes.md#pages)
