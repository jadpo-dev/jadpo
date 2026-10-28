# `query.result_field`

```jadpo check project=e-result-field expect=query.result_field
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type BookCard = Output {
    pick: [Book.id, Book.title]
    author: Title
}

query cards() -> List<BookCard> {
    from: Book
    order: [Book.title asc]
    limit: 20
}
```

```text output
error: `BookCard.author` isn't a field of `Book`, or an include
 --> main.jadpo:17:18
   |
17 | query cards() -> List<BookCard> {
   |                  ^^^^^^^^^^^^^^ `author` can't be filled
  reason: A query fills its result's fields from `Book`'s fields of the same name (`id`, `title`, `subtitle` and `available`) and from its includes; nothing gives `author`.
  next: Remove `author` from `BookCard`, or include records under it (agent)
  rule: query.result_field
  help: docs/language/06-reads-and-writes.html#query-result-field

check failed: 1 error, 1 file
```
