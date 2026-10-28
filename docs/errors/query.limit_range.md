# `query.limit_range`

```jadpo check project=e-limit-range expect=query.limit_range
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
    order: [Book.title asc]
    limit: 0
}
```

```text output
error: `limit: 0` would read nothing
 --> main.jadpo:15:12
   |
15 |     limit: 0
   |            ^ below 1
  reason: `limit:` is how many records come back at most, so it is at least 1.
  next: Give `limit:` a number from 1 (agent)
  rule: query.limit_range
  help: docs/errors/query.limit_range.md

check failed: 1 error, 1 file
```

A value's type must keep it at 1 or more too, since PostgreSQL refuses a
negative `LIMIT` and 0 reads nothing:

```jadpo check project=e-limit-range-type expect=query.limit_range
type Title = Text {
    max_length: 80
}

type ShelfSize = Int {
    max: 50
}

entity Book {
    id: Uuid
    title: Title
}

query shelf(size: ShelfSize) -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: size
}
```

```text output
error: `size` can be below 1, so the limit could read nothing
 --> main.jadpo:17:12
   |
17 |     limit: size
   |            ^^^^ `ShelfSize` has no `min:`
  reason: `limit:` is how many records come back at most, so it is at least 1, and a value's type must say so with `min: 1` or more, as the built-in `PageSize` (1 to 100) does.
  next: Give `ShelfSize` `min: 1`, or type the value as `PageSize` (agent)
  rule: query.limit_range
  help: docs/errors/query.limit_range.md

check failed: 1 error, 1 file
```
