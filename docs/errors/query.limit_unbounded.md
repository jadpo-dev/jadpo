# `query.limit_unbounded`

```jadpo check project=e-limit-unbounded expect=query.limit_unbounded
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type Count = Int {
    min: 1
}

query shelf(count: Count) -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: count
}
```

```text output
error: `count` has no maximum, so the limit has no bound
 --> main.jadpo:19:12
   |
19 |     limit: count
   |            ^^^^^ `Count` has no `max:`
  reason: Every list is bounded: `limit:` is a number, or a value whose type has a `max:`, such as the built-in `PageSize` (1 to 100).
  next: Type the value as `PageSize`, or as an `Int` type of your own with a `max:` (agent)
  rule: query.limit_unbounded
  help: docs/language/06-reads-and-writes.html#query-limit-unbounded

check failed: 1 error, 1 file
```
