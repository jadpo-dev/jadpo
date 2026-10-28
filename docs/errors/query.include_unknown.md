# `query.include_unknown`

```jadpo check project=e-include-unknown expect=query.include_unknown
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type ShelfCard = Output {
    pick: [Book.id]
    name: Title
}

query cards() -> List<ShelfCard> {
    from: Book
    order: [Book.title asc]
    limit: 20
    include: {
        name: Book.title
    }
}
```

```text output
error: `Book.title` isn't a reference to or from `Book`
 --> main.jadpo:22:15
   |
22 |         name: Book.title
   |               ^^^^^^^^^^ not a reference
  reason: An include follows a reference: one of `Book`'s own gives the record it points at, and another entity's reference to `Book` gives the records that point here. No reference leads to or from `Book`.
  next: Add a reference field, or read the records with a query of their own (agent)
  rule: query.include_unknown
  help: docs/language/06-reads-and-writes.html#query-include-unknown

check failed: 1 error, 1 file
```
