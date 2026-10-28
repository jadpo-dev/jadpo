# `query.not_unique`

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
  help: docs/language/06-reads-and-writes.html#query-not-unique

check failed: 1 error, 1 file
```
