# `query.unknown_field`

```jadpo check project=e-unknown-field expect=query.unknown_field
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
    where: Book.titel == title
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `Book` has no field `titel`
 --> main.jadpo:14:17
   |
14 |     where: Book.titel == title
   |                 ^^^^^ not declared
  reason: `Book`'s fields are `id`, `title`, `subtitle` and `available`. Did you mean `title`?
  next: Use `title` instead of `titel` (agent)
  rule: query.unknown_field
  help: docs/errors/query.unknown_field.md

check failed: 1 error, 1 file
```
