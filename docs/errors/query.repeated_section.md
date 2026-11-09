# `query.repeated_section`

A query gives one of its sections twice, or includes the same field twice.

## Why Jadpo refuses it

Each section of a query says one thing about it, so two would disagree, and
nothing says which wins. Only `where:` and `where_present:` lines stack,
each one more condition. An include fills one field of the result, so a
field is included once.

## What it looks like

main.jadpo:

```jadpo check project=e-repeated-section expect=query.repeated_section
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
    where: Book.title == title
    order: [Book.id asc]
    limit: 20
    limit: 50
}
```

```text output
error: `limit:` is given twice
 --> main.jadpo:17:5
   |
16 |     limit: 20
   |     ----- first given here
17 |     limit: 50
   |     ^^^^^ given again
  reason: A query has one `limit:`; only `where:` and `where_present:` lines stack, each one more condition.
  next: Remove one `limit:` (agent)
  rule: query.repeated_section
  help: https://jadpo.dev/docs/errors/query.repeated_section

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An include is named after the field it fills, so a field is included once:

main.jadpo:

```jadpo check project=e-repeated-include expect=query.repeated_section
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
    editor_id: Author.id {
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
        author: Book.editor_id
    }
}
```

```text output
error: `author` is included twice
 --> main.jadpo:32:9
   |
31 |         author: Book.author_id
   |         ------ first here
32 |         author: Book.editor_id
   |         ^^^^^^ again
  reason: Each include fills one field of the result, so a field is included once.
  next: Remove one `author` (agent)
  rule: query.repeated_section
  help: https://jadpo.dev/docs/errors/query.repeated_section

check failed: 1 error, 1 file
```

## How to fix it

**If one of them is left over**, remove it:

```jadpo fragment
    limit: 20
```

```jadpo check project=e-repeated-section-fix-1 hidden
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
    where: Book.title == title
    order: [Book.id asc]
    limit: 20
}
```

The query has one `limit:`, and reads at most 20 records.

**If two includes both matter**, give each a field of its own, and name it in the type too:

```jadpo fragment
    include: {
        author: Book.author_id
        editor: Book.editor_id
    }
```

```jadpo check project=e-repeated-include-fix-1 hidden
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
    editor_id: Author.id {
        on_delete: OnDelete.cascade
    }
    title: Name
}

type BookCard = Output {
    pick: [Book.id, Book.title]
    author: Author
    editor: Author
}

query cards() -> List<BookCard> {
    from: Book
    order: [Book.title asc]
    limit: 20
    include: {
        author: Book.author_id
        editor: Book.editor_id
    }
}
```

Each card now holds both people.

## Related

- [Named queries](../language/06-reads-and-writes.md#named-queries)
- [Children and parents](../language/06-reads-and-writes.md#children-and-parents)
- [`query.unknown_section`](query.unknown_section.md)
