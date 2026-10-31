# `query.not_unique`

A query that returns one record, or none, picks it by a test that several records could match.

## Why Jadpo refuses it

A result type of `Book` or `Book?` says at most one record comes back.
Only a `where:` that tests a unique field with `==` can promise that.
Here `title` isn't unique, so two books with the same title would both
match, and the query would have no right answer.

## What it looks like

main.jadpo:

```jadpo check project=e-not-unique expect=query.not_unique
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query book_titled(title: Book.title) -> Book? {
    from: Book
    where: Book.title == title
}
```

```text output
error: A query returning `Book?` could match several records
 --> main.jadpo:14:5
   |
14 |     where: Book.title == title
   |     ^^^^^ not a unique field
  reason: `Book?` is at most one record, so a `where:` line must test a unique field with `==`: `Book.id`.
  next: Test a unique field with `==`, or return `List<T>` (agent)
  rule: query.not_unique
  help: https://jadpo.dev/docs/errors/query.not_unique

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If no two books should share a title**, make `title` unique:

```jadpo fragment
    title: Title {
        unique: true
    }
```

```jadpo check project=e-not-unique-fix-1 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title {
        unique: true
    }
    subtitle: Title?
    available: Bool
}

query book_titled(title: Book.title) -> Book? {
    from: Book
    where: Book.title == title
}
```

The database refuses a second book with a title already taken, so every
write that sets `title` now needs a `conflict:` arm for it.
`book_titled` gives the one book with that title, or `none`.

**If several books may share a title**, return a list, with `order:` and
`limit:`:

```jadpo fragment
query books_titled(title: Book.title) -> List<Book> {
    from: Book
    where: Book.title == title
    order: [Book.id asc]
    limit: 20
}
```

```jadpo check project=e-not-unique-fix-2 hidden
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

The query gives up to 20 books with that title, and callers handle a
list rather than one book.

## Related

- [How many records come back](../language/06-reads-and-writes.md#how-many-records-come-back)
- [Unique rules](../language/05-data.md#unique-rules)
- [`data.conflict_unmapped`](../errors/data.conflict_unmapped.md)
