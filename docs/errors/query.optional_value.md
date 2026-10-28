# `query.optional_value`

```jadpo check project=e-optional-value expect=query.optional_value
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type FindBooks = Input {
    title: Title {
        optional: true
    }
}

query find_books(filter: FindBooks) -> List<Book> {
    from: Book
    where: Book.title == filter.title
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `filter.title` may be absent, so this `where:` line can't always be met
 --> main.jadpo:20:26
   |
20 |     where: Book.title == filter.title
   |                          ^^^^^^^^^^^^ may be `none`
  reason: A `where:` line applies to every call; a condition that should apply only when its value is there, as a request's optional filter, is written with `where_present:`.
  next: Write `where_present:` for this condition (agent)
  rule: query.optional_value
  help: docs/language/06-reads-and-writes.html#query-optional-value

check failed: 1 error, 1 file
```
