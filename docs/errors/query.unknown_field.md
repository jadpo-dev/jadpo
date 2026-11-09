# `query.unknown_field`

A query names a field its entity doesn't have.

## Why Jadpo refuses it

A query's conditions, order and groups are about the fields of the entity it
reads, written with its name. A name that isn't one of them would have no
column to read, so it is refused rather than guessed at.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/query.unknown_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A field of another entity isn't one this query reads. A related entity's
records come in through `include:`:

main.jadpo:

```jadpo check project=e-unknown-field-other expect=query.unknown_field
type Title = Text {
    max_length: 80
}

entity Author {
    id: Uuid
    name: Title
}

entity Book {
    id: Uuid
    title: Title
}

query books_titled(name: Title) -> List<Book> {
    from: Book
    where: Author.name == name
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `Author.name` isn't a field of `Book`
 --> main.jadpo:17:12
   |
17 |     where: Author.name == name
   |            ^^^^^^ not `Book`
  reason: This part of the query reads `Book` records, and `Author` is another entity; a related entity's records come in through `include:`.
  next: Name a field of `Book` (agent)
  rule: query.unknown_field
  help: https://jadpo.dev/docs/errors/query.unknown_field

check failed: 1 error, 1 file
```

## How to fix it

**If the name is misspelt**, use the field's own name:

```jadpo fragment
    where: Book.title == title
```

```jadpo check project=e-unknown-field-fix-1 hidden
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

The query now tests `Book`'s `title`.

**If you meant a field of another entity**, read that entity in a query of its own:

```jadpo fragment
query authors_named(name: Author.name) -> List<Author> {
    from: Author
    where: Author.name == name
    ...
}
```

```jadpo check project=e-unknown-field-fix-2 hidden
type Title = Text {
    max_length: 80
}

entity Author {
    id: Uuid
    name: Title
}

query authors_named(name: Author.name) -> List<Author> {
    from: Author
    where: Author.name == name
    order: [Author.id asc]
    limit: 20
}
```

Each author's `id` can then be passed to a query of `Book` that tests a reference to it, or an `include:` can bring the books in under a field of the result.

## Related

- [Filters](../language/06-reads-and-writes.md#filters)
- [Children and parents](../language/06-reads-and-writes.md#children-and-parents)
