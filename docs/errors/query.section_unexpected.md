# `query.section_unexpected`

A query or one of its includes holds a section its result doesn't take.

## Why Jadpo refuses it

A query's result type says what kind of read it is, and each kind takes only
the sections that mean something for it. One record has nothing to order, a
list isn't paged, and a total over every record is always there. A section
that can't apply would read as though it did something, so Jadpo refuses it
rather than ignore it.

## What it looks like

main.jadpo:

```jadpo check project=e-section-unexpected expect=query.section_unexpected
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query book_by_id(book_id: Book.id) -> Book? {
    from: Book
    where: Book.id == book_id
    order: [Book.title asc]
}
```

```text output
error: A query returning `Book?` takes no `order:`
 --> main.jadpo:15:5
   |
15 |     order: [Book.title asc]
   |     ^^^^^ not for this result
  reason: `Book?` is at most one record, found by a unique field, so there is nothing to order or limit.
  next: Remove `order:` (agent)
  rule: query.section_unexpected
  help: https://jadpo.dev/docs/errors/query.section_unexpected

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`missing:` names the failure for a query returning exactly one record. A
`Book?` says the record may be absent, so the caller gets `none`:

main.jadpo:

```jadpo check project=e-section-unexpected-missing expect=query.section_unexpected
type BookGone = Failure {
    kind: FailureKind.not_found
}

type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query book_by_id(book_id: Book.id) -> Book? {
    from: Book
    where: Book.id == book_id
    missing: BookGone
}
```

```text output
error: A query returning `Book?` takes no `missing:`
 --> main.jadpo:19:5
   |
19 |     missing: BookGone
   |     ^^^^^^^ not for this result
  reason: `Book?` already says the record may be absent: the caller gets `none`, so there is no failure to name.
  next: Remove `missing:` (agent)
  rule: query.section_unexpected
  help: https://jadpo.dev/docs/errors/query.section_unexpected

check failed: 1 error, 1 file
```

A `limit:` bounds a list, and a page's size is in its `page:` block:

main.jadpo:

```jadpo check project=e-section-unexpected-limit expect=query.section_unexpected
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf(after: PageCursor?) -> Page<Book> {
    from: Book
    order: [Book.title asc]
    page: {
        size: 20
        after: after
    }
    limit: 20
}
```

```text output
error: A query returning `Page<Book>` takes no `limit:`
 --> main.jadpo:19:5
   |
19 |     limit: 20
   |     ^^^^^ not for this result
  reason: A page's size is its `page:` block's `size:`; `limit:` bounds a `List<T>`.
  next: Remove `limit:` (agent)
  rule: query.section_unexpected
  help: https://jadpo.dev/docs/errors/query.section_unexpected

check failed: 1 error, 1 file
```

`page:` belongs to a query returning `Page<T>`:

main.jadpo:

```jadpo check project=e-section-unexpected-page expect=query.section_unexpected
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf(after: PageCursor?) -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: 20
    page: {
        size: 20
        after: after
    }
}
```

```text output
error: A query returning `List<Book>` takes no `page:`
 --> main.jadpo:16:5
   |
16 |     page: {
   |     ^^^^ not for this result
  reason: `page:` belongs to a query returning `Page<T>`; `List<Book>` is one bounded list, sized by `limit:`.
  next: Remove `page:` (agent)
  rule: query.section_unexpected
  help: https://jadpo.dev/docs/errors/query.section_unexpected

check failed: 1 error, 1 file
```

`geometry:` names the field each feature of a map layer holds, and a list of
records holds no features:

main.jadpo:

```jadpo check project=e-section-unexpected-geometry expect=query.section_unexpected
entity Shop {
    id: Uuid
    spot: Point
}

query shops() -> List<Shop> {
    from: Shop
    geometry: Shop.spot
    order: [Shop.id asc]
    limit: 20
}
```

```text output
error: A query returning `List<Shop>` takes no `geometry:`
 --> main.jadpo:8:5
  |
8 |     geometry: Shop.spot
  |     ^^^^^^^^ not for this result
  reason: `geometry:` names the field each feature of a map layer holds, for a query returning `FeatureCollection<G, P>`; `List<Shop>` holds records, not features.
  next: Remove `geometry:` (agent)
  rule: query.section_unexpected
  help: https://jadpo.dev/docs/errors/query.section_unexpected

check failed: 1 error, 1 file
```

A `group:` gives one row for each group, so the query returns them as a
list:

main.jadpo:

```jadpo check project=e-section-unexpected-group expect=query.section_unexpected
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

query title_rows() -> TitleRow {
    from: Book
    group: {
        title: Book.title
    }
    aggregate: {
        copies: count()
    }
}
```

```text output
error: A query returning `TitleRow` takes no `group:`
 --> main.jadpo:17:5
   |
17 |     group: {
   |     ^^^^^ not for this result
  reason: `group:` gives one row for each group of records, so the query returns them as a list, ordered and limited: `List<T>`.
  next: Return `List<TitleRow>` (agent)
  rule: query.section_unexpected
  help: https://jadpo.dev/docs/errors/query.section_unexpected

check failed: 1 error, 1 file
```

An aggregate query with no `group:` is one row over every record, so it has
nothing to order or limit:

main.jadpo:

```jadpo check project=e-section-unexpected-aggregate expect=query.section_unexpected
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
}

type ShelfCount = Output {
    books: Int
}

query shelf_count() -> ShelfCount {
    from: Book
    aggregate: {
        books: count()
    }
    limit: 20
}
```

```text output
error: An aggregate query takes no `limit:`
 --> main.jadpo:19:5
   |
19 |     limit: 20
   |     ^^^^^ not for this query
  reason: Without `group:`, an aggregate query is one row over every record it finds, so there is nothing to order or limit.
  next: Remove `limit:` (agent)
  rule: query.section_unexpected
  help: https://jadpo.dev/docs/errors/query.section_unexpected

check failed: 1 error, 1 file
```

An include of a parent record is the one record a reference points at, which
is there whenever the reference is, so it takes no order or limit:

main.jadpo:

```jadpo check project=e-section-unexpected-include expect=query.section_unexpected
type Name = Text {
    max_length: 80
}

entity Author {
    id: Uuid
    name: Name
}

entity Book {
    id: Uuid
    author_id: Author.id {
        on_delete: OnDelete.cascade
    }
    title: Name
}

type BookCard = Output {
    pick: [Book.id, Book.title]
    author: Author
}

query cards() -> List<BookCard> {
    from: Book
    order: [Book.title asc]
    limit: 20
    include: {
        author: {
            via: Book.author_id
            order: [Author.name asc]
        }
    }
}
```

```text output
error: This include takes no `order:`
 --> main.jadpo:30:13
   |
30 |             order: [Author.name asc]
   |             ^^^^^ not for this include
  reason: This include is the one `Author` record a reference points at, which is there whenever the reference is, so it takes no conditions, order or limit.
  next: Remove `order:` (agent)
  rule: query.section_unexpected
  help: https://jadpo.dev/docs/errors/query.section_unexpected

check failed: 1 error, 1 file
```

## How to fix it

**If the section isn't needed**, remove it:

```jadpo fragment
query book_by_id(book_id: Book.id) -> Book? {
    from: Book
    where: Book.id == book_id
}
```

```jadpo check project=e-section-unexpected-fix-1 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query book_by_id(book_id: Book.id) -> Book? {
    from: Book
    where: Book.id == book_id
}
```

A unique field picks at most one record, so there is nothing to order.

**If the section is the one you want**, return what it applies to:

```jadpo fragment
query shelf() -> List<Book> {
    ...
    order: [Book.title asc]
    limit: 20
}
```

```jadpo check project=e-section-unexpected-fix-2 hidden
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

The query is now a bounded list, which takes `order:` and `limit:`. A query with `missing:` returns exactly one record, `Book` rather than `Book?`.

## Related

- [How many records come back](../language/06-reads-and-writes.md#how-many-records-come-back)
- [What an aggregate query doesn't take](../language/06-reads-and-writes.md#what-an-aggregate-query-doesnt-take)
- [`query.section_required`](query.section_required.md)
