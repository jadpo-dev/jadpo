# `query.result_field`

The fields of a query's result type don't match what the query reads.

## Why Jadpo refuses it

A query's result type says what each record it returns holds, and every
field has to come from somewhere: a field of the entity with the same name,
an include, or a group or aggregate. A field with no source, or a stored
value that doesn't fit the field's type, couldn't be filled when the query
runs.

An aggregate query fills its fields from its groups and aggregates only,
since a field of the entity has no one value for a row of records.

## What it looks like

main.jadpo:

```jadpo check project=e-result-field expect=query.result_field
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type BookCard = Output {
    pick: [Book.id, Book.title]
    author: Title
}

query cards() -> List<BookCard> {
    from: Book
    order: [Book.title asc]
    limit: 20
}
```

```text output
error: `BookCard.author` isn't a field of `Book`, or an include
 --> main.jadpo:17:18
   |
17 | query cards() -> List<BookCard> {
   |                  ^^^^^^^^^^^^^^ `author` can't be filled
  reason: A query fills its result's fields from `Book`'s fields of the same name (`id`, `title`, `subtitle` and `available`) and from its includes; nothing gives `author`.
  next: Remove `author` from `BookCard`, or include records under it (agent)
  rule: query.result_field
  help: https://jadpo.dev/docs/errors/query.result_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A stored value goes into the result as it is, so the result's field takes
the field's type or one it widens to:

main.jadpo:

```jadpo check project=e-result-field-type expect=query.result_field
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type BookCard = Output {
    pick: [Book.id]
    title: Int
}

query cards() -> List<BookCard> {
    from: Book
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `BookCard.title` is `Int`, which `Book.title` can't fill
 --> main.jadpo:17:18
   |
17 | query cards() -> List<BookCard> {
   |                  ^^^^^^^^^^^^^^ `title` doesn't fit
  reason: `Book.title` is `Title`, and a stored value goes into the result as it is, so the result's field takes that type or one it widens to.
  next: Declare `title` as `Book.title`, or pick it: `pick: [Book.title]` (agent)
  rule: query.result_field
  help: https://jadpo.dev/docs/errors/query.result_field

check failed: 1 error, 1 file
```

Each group and aggregate fills the field of its own name, so the type needs
a field for it:

main.jadpo:

```jadpo check project=e-result-field-aggregate-name expect=query.result_field
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
        titles: count()
    }
}
```

```text output
error: `ShelfCount` has no field `titles` to hold the aggregate
 --> main.jadpo:18:9
   |
18 |         titles: count()
   |         ^^^^^^ nowhere to put it
  reason: Each group and aggregate fills the result's field of its name, and `ShelfCount` declares none called `titles`.
  next: Add `titles: ...` to `ShelfCount`, or rename the aggregate (agent)
  rule: query.result_field
  help: https://jadpo.dev/docs/errors/query.result_field

check failed: 1 error, 1 file
```

Every field of an aggregate query's result is a group or an aggregate:

main.jadpo:

```jadpo check project=e-result-field-aggregate-missing expect=query.result_field
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
}

type ShelfCount = Output {
    books: Int
    title: Title
}

query shelf_count() -> ShelfCount {
    from: Book
    aggregate: {
        books: count()
    }
}
```

```text output
error: `ShelfCount.title` isn't a group or an aggregate of `shelf_count`
 --> main.jadpo:15:24
   |
15 | query shelf_count() -> ShelfCount {
   |                        ^^^^^^^^^^ `title` can't be filled
  reason: An aggregate query fills its result's fields from its groups and aggregates of the same name (`books`); nothing gives `title`, and a field of `Book` has no one value for a row's records.
  next: Remove `title` from `ShelfCount`, or add `title: ...` to `group:` or `aggregate:` (agent)
  rule: query.result_field
  help: https://jadpo.dev/docs/errors/query.result_field

check failed: 1 error, 1 file
```

A total goes into its field as it is, so the field takes the type the
aggregate gives:

main.jadpo:

```jadpo check project=e-result-field-aggregate-type expect=query.result_field
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
}

type ShelfCount = Output {
    books: Title
}

query shelf_count() -> ShelfCount {
    from: Book
    aggregate: {
        books: count()
    }
}
```

```text output
error: `ShelfCount.books` is `Title`, which `count()` can't fill
 --> main.jadpo:17:9
   |
17 |         books: count()
   |         ^^^^^ gives `Int`
  reason: `count()` gives `Int`, and a total goes into the result as it is, so the result's field takes that type or one it widens to.
  next: Declare `books` as `Int` (agent)
  rule: query.result_field
  help: https://jadpo.dev/docs/errors/query.result_field

check failed: 1 error, 1 file
```

A name fills one field, so a group and an aggregate can't share it:

main.jadpo:

```jadpo check project=e-result-field-twice expect=query.result_field
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
    aggregate: {
        title: count()
    }
    order: [title asc]
    limit: 20
}
```

```text output
error: `title` is given twice
 --> main.jadpo:20:9
   |
17 |         title: Book.title
   |         ----- first given here
20 |         title: count()
   |         ^^^^^ given again
  reason: Each group and aggregate fills the result's field of its name, so `title` would fill one field twice.
  next: Rename one of them, and add a field for it to `TitleRow` (agent)
  rule: query.result_field
  help: https://jadpo.dev/docs/errors/query.result_field

check failed: 1 error, 1 file
```

## How to fix it

**If the field doesn't belong**, remove it from the type:

```jadpo fragment
type BookCard = Output {
    pick: [Book.id, Book.title]
}
```

```jadpo check project=e-result-field-fix-1 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type BookCard = Output {
    pick: [Book.id, Book.title]
}

query cards() -> List<BookCard> {
    from: Book
    order: [Book.title asc]
    limit: 20
}
```

Every field of `BookCard` now comes from `Book`.

**If the field is related records**, include them under its name:

```jadpo fragment
    include: {
        author: Book.author_id
    }
```

```jadpo check project=e-result-field-fix-2 hidden
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
        author: Book.author_id
    }
}
```

Each card now comes back with its author.

**If a field's type doesn't fit**, pick the field, so it has the entity's own type:

```jadpo fragment
type BookCard = Output {
    pick: [Book.id, Book.title]
}
```

```jadpo check project=e-result-field-fix-3 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type BookCard = Output {
    pick: [Book.id, Book.title]
}

query cards() -> List<BookCard> {
    from: Book
    order: [Book.id asc]
    limit: 20
}
```

`title` now has the type `Book.title` has.

**If an aggregate query's fields and totals don't match**, give each total the name of a field, and each field a total:

```jadpo fragment
    aggregate: {
        books: count()
    }
```

```jadpo check project=e-result-field-fix-4 hidden
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
}
```

Each total fills the field of its name, and each field is filled by one total.

## Related

- [What a result holds](../language/06-reads-and-writes.md#what-a-result-holds)
- [The result](../language/06-reads-and-writes.md#the-result)
- [`query.include_result`](query.include_result.md)
