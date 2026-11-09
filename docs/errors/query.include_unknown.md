# `query.include_unknown`

A query's `include:` isn't a block of references to or from its entity, in the same store.

## Why Jadpo refuses it

An include brings in related records by following a reference, so that the
database can read them with the record, in one statement. A field that isn't
a reference leads nowhere, and a record kept in another store is in a
database this statement can't read.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/query.include_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`include:` is always a block, naming each result field and the reference
that fills it:

main.jadpo:

```jadpo check project=e-include-unknown-block expect=query.include_unknown
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
    include: Book.author_id
}
```

```text output
error: `include:` is a block of named includes
 --> main.jadpo:26:5
   |
26 |     include: Book.author_id
   |     ^^^^^^^ not a block
  reason: Each include puts related records under a field of the result: `include: { <field>: <Entity>.<reference> }`.
  next: Write `include: { <field>: <Entity>.<reference> }` (agent)
  rule: query.include_unknown
  help: https://jadpo.dev/docs/errors/query.include_unknown

check failed: 1 error, 1 file
```

## How to fix it

**If the field belongs to the entity**, pick it, and leave it out of `include:`:

```jadpo fragment
type ShelfCard = Output {
    pick: [Book.id, Book.title]
}
```

```jadpo check project=e-include-unknown-fix-1 hidden
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
    pick: [Book.id, Book.title]
}

query cards() -> List<ShelfCard> {
    from: Book
    order: [Book.title asc]
    limit: 20
}
```

The result now has the book's own `title`, with no include.

**If it is a related record**, name the reference, in a block:

```jadpo fragment
    include: {
        author: Book.author_id
    }
```

```jadpo check project=e-include-unknown-fix-2 hidden
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

Each book now comes back with its author, and the query is still one statement. Records in another store are read with a query of their own, by the `id` this one gives.

## Related

- [Children and parents](../language/06-reads-and-writes.md#children-and-parents)
- [One store per action](../language/06-reads-and-writes.md#one-store-per-action)
- [`query.include_result`](query.include_result.md)
