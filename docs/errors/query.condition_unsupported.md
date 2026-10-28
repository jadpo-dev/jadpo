# `query.condition_unsupported`

```jadpo check project=e-condition-unsupported expect=query.condition_unsupported
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query books_after(title: Book.title) -> List<Book> {
    from: Book
    where: Book.title > title
    order: [Book.title asc]
    limit: 20
}
```

```text output
error: `Book.title > title` isn't a condition a query can use
 --> main.jadpo:14:12
   |
14 |     where: Book.title > title
   |            ^^^^^^^^^^^^^^^^^^ not an indexable condition
  reason: `>` orders numbers, times, dates, durations and quantities, as it does in a body, and `Book.title` is `Title`.
  next: Compare `Book.title` with `==`, `!=` or `in` (agent)
  rule: query.condition_unsupported
  help: docs/language/06-reads-and-writes.html#query-condition-unsupported

check failed: 1 error, 1 file
```

A standard operation stands where a field does only when it has an exact
SQL form ("Operations in conditions"). `text.lower` has none,
since PostgreSQL's would follow the database's collation:

```jadpo check project=e-condition-no-form expect=query.condition_unsupported
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
}

query books_titled(title: Book.title) -> List<Book> {
    from: Book
    where: text.lower(Book.title) == title
    order: [Book.title asc]
    limit: 20
}
```

```text output
error: `text.lower(Book.title) == title` isn't a condition a query can use
 --> main.jadpo:12:12
   |
12 |     where: text.lower(Book.title) == title
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not an indexable condition
  reason: `text.lower` has no SQL form, an expression PostgreSQL works out exactly as the code does (area 19, "SQL forms"), so the database can't decide this condition.
  next: Compare `Book.title` itself, or keep what the condition needs in a field of its own (agent)
  rule: query.condition_unsupported
  help: docs/language/06-reads-and-writes.html#query-condition-unsupported

check failed: 1 error, 1 file
```
