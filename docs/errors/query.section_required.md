# `query.section_required`

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
  help: docs/language/06-reads-and-writes.html#query-section-required

check failed: 1 error, 1 file
```
