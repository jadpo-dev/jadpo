# `query.unknown_section`

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
  reason: A query's block holds `from:`, `geometry:`, `where:`, `where_present:`, `group:`, `aggregate:`, `order:`, `limit:`, `page:`, `include:` and `missing:`. Did you mean `order:`?
  next: Use `order:` (agent)
  rule: query.unknown_section
  help: docs/errors/query.unknown_section.md

check failed: 1 error, 1 file
```
