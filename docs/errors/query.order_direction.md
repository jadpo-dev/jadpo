# `query.order_direction`

```jadpo check project=e-order-direction expect=query.order_direction
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
    order: [Book.title]
    limit: 20
}
```

```text output
error: `Book.title` says no direction
 --> main.jadpo:14:13
   |
14 |     order: [Book.title]
   |             ^^^^^^^^^^ `asc` or `desc`?
  reason: Each field of `order:` says which way it sorts: `asc` from the smallest, `desc` from the largest.
  next: Write `Book.title asc` or `Book.title desc` (agent)
  rule: query.order_direction
  help: https://jadpo.dev/docs/errors/query.order_direction

check failed: 1 error, 1 file
```
