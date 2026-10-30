# `query.section_unexpected`

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
