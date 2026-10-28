# `query.order_field`

```jadpo check project=e-order-field expect=query.order_field
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
    order: [Book.available desc]
    limit: 20
}
```

```text output
error: Records can't be sorted by `Book.available`
 --> main.jadpo:14:13
   |
14 |     order: [Book.available desc]
   |             ^^^^^^^^^^^^^^^^^^^ `Bool`
  reason: A query sorts by numbers, times, dates, durations, quantities, percentages, text and identities, which the database and Jadpo order the same way; `Book.available` is `Bool`.
  next: Sort by another field (agent)
  rule: query.order_field
  help: docs/language/06-reads-and-writes.html#query-order-field

check failed: 1 error, 1 file
```
