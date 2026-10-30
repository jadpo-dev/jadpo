# `query.include_result`

```jadpo check project=e-include-result expect=query.include_result
type Name = Text {
    max_length: 80
}

entity Author {
    id: Uuid
    name: Name
}

entity Book {
    id: Uuid
    author_id: Author.id {
        on_delete: OnDelete.cascade
    }
    title: Name
}

type BookCard = Output {
    pick: [Book.id, Book.title]
    author: List<Author>
}

query cards() -> List<BookCard> {
    from: Book
    order: [Book.title asc]
    limit: 20
    include: {
        author: Book.author_id
    }
}
```

```text output
error: `BookCard.author` is `List<Author>`, which the include `author` can't fill
 --> main.jadpo:28:9
   |
28 |         author: Book.author_id
   |         ^^^^^^ needs `Author`
  reason: `author` follows a required reference, so the one record it points at is always there. Its field is `Author`, where `Author` may also be an object type derived from it.
  next: Declare `author: Author` in `BookCard` (agent)
  rule: query.include_result
  help: https://jadpo.dev/docs/errors/query.include_result

check failed: 1 error, 1 file
```
