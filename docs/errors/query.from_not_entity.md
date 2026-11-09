# `query.from_not_entity`

A query's `from:` names something that isn't an entity.

## Why Jadpo refuses it

A query reads stored records, and only an entity has records: a type
describes a value, and a name with nothing behind it has no table to read.

## What it looks like

main.jadpo:

```jadpo check project=e-from-not-entity expect=query.from_not_entity
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query titles(title: Book.title) -> List<Book> {
    from: Title
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `from:` names `Title`, which isn't an entity
 --> main.jadpo:13:11
   |
13 |     from: Title
   |           ^^^^^ not an entity
  reason: `Title` is a type; a query reads stored records, so `from:` names an entity declared in the domain's `entities/` folder.
  next: Name the entity the query reads (agent)
  rule: query.from_not_entity
  help: https://jadpo.dev/docs/errors/query.from_not_entity

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A name that isn't declared is refused the same way, and the error suggests
the nearest entity:

main.jadpo:

```jadpo check project=e-from-not-declared expect=query.from_not_entity
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query titles() -> List<Book> {
    from: Bok
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `from:` names `Bok`, which isn't an entity
 --> main.jadpo:13:11
   |
13 |     from: Bok
   |           ^^^ not an entity
  reason: Nothing called `Bok` is declared; a query reads stored records, so `from:` names an entity declared in the domain's `entities/` folder.
  next: Use `from: Book` (agent)
  rule: query.from_not_entity
  help: https://jadpo.dev/docs/errors/query.from_not_entity

check failed: 1 error, 1 file
```

## How to fix it

**If you meant an entity**, name it:

```jadpo fragment
    from: Book
```

```jadpo check project=e-from-not-entity-fix-1 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query titles(title: Book.title) -> List<Book> {
    from: Book
    where: Book.title == title
    order: [Book.id asc]
    limit: 20
}
```

The query now reads `Book` records, and its conditions and order are about their fields.

## Related

- [Named queries](../language/06-reads-and-writes.md#named-queries)
- [What a result holds](../language/06-reads-and-writes.md#what-a-result-holds)
