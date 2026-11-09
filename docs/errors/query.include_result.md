# `query.include_result`

A query's result type has no field of the right shape to hold what an include reads.

## Why Jadpo refuses it

An include puts related records under a field of the result, by name, so the
field has to exist and be the shape the reference gives: one record for a
reference the record holds, and a list for records that point back at it. An
entity itself holds only its own fields, so a result with an include is an
`Output` type of its own.

## What it looks like

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

The entity itself has no room for an include, so the result needs a type
made for it:

main.jadpo:

```jadpo check project=e-include-result-entity expect=query.include_result
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

query cards() -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: 20
    include: {
        author: Book.author_id
    }
}
```

```text output
error: `Book` has no field `author` to hold the include
 --> main.jadpo:23:9
   |
23 |         author: Book.author_id
   |         ^^^^^^ nowhere to put it
  reason: The result is the entity `Book` itself, which holds only its own fields; included records go under fields of a result type made for them.
  next: Return a type with `author` and the fields you need, such as `type BookView = Output { pick: [...] author: ... }` (agent)
  rule: query.include_result
  help: https://jadpo.dev/docs/errors/query.include_result

check failed: 1 error, 1 file
```

Each include fills the field of its own name, so a type with no field of
that name has nowhere to put it:

main.jadpo:

```jadpo check project=e-include-result-name expect=query.include_result
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
error: `BookCard` has no field `author` to hold the include
 --> main.jadpo:27:9
   |
27 |         author: Book.author_id
   |         ^^^^^^ nowhere to put it
  reason: Each include fills the result's field of the same name, and `BookCard` declares none called `author`.
  next: Add `author: ...` to `BookCard`, or rename the include (agent)
  rule: query.include_result
  help: https://jadpo.dev/docs/errors/query.include_result

check failed: 1 error, 1 file
```

## How to fix it

**If the field has the wrong shape**, declare it as the reference gives it:

```jadpo fragment
    author: Author
```

```jadpo check project=e-include-result-fix-1 hidden
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
    author: Author
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

`author` now holds the book's one author, which is always there because the reference is required.

## Related

- [Children and parents](../language/06-reads-and-writes.md#children-and-parents)
- [`query.include_unknown`](query.include_unknown.md)
