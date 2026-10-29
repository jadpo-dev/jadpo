# `query.repeated_section`

```jadpo check project=e-repeated-section expect=query.repeated_section
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
    limit: 50
}
```

```text output
error: `limit:` is given twice
 --> main.jadpo:17:5
   |
16 |     limit: 20
   |     ----- first given here
17 |     limit: 50
   |     ^^^^^ given again
  reason: A query has one `limit:`; only `where:` and `where_present:` lines stack, each one more condition.
  next: Remove one `limit:` (agent)
  rule: query.repeated_section
  help: docs/errors/query.repeated_section.md

check failed: 1 error, 1 file
```
