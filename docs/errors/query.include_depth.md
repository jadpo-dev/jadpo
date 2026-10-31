# `query.include_depth`

A query's includes nest more than two levels deep: records inside records inside records.

## Why Jadpo refuses it

Each include is one more database query, however many records the
result holds. Keeping includes to two levels keeps a query's cost easy
to see from its shape. A deeper shape is read by a second query instead.

Jadpo won't split the query for you, because the split decides what the
result holds and how its readers get the rest.

## What it looks like

main.jadpo:

```jadpo check project=e-include-depth expect=query.include_depth
type Name = Text {
    max_length: 80
}

entity Territory {
    id: Uuid
    name: Name
}

entity Publisher {
    id: Uuid
    territory_id: Territory.id {
        on_delete: OnDelete.restricted
    }
}

entity Author {
    id: Uuid
    publisher_id: Publisher.id {
        on_delete: OnDelete.restricted
    }
}

entity Book {
    id: Uuid
    author_id: Author.id {
        on_delete: OnDelete.cascade
    }
}

type TerritoryView = Output {
    pick: [Territory.name]
}

type PublisherView = Output {
    territory: TerritoryView
}

type AuthorView = Output {
    publisher: PublisherView
}

type BookView = Output {
    pick: [Book.id]
    author: AuthorView
}

query books() -> List<BookView> {
    from: Book
    order: [Book.id asc]
    limit: 20
    include: {
        author: {
            via: Book.author_id
            include: {
                publisher: {
                    via: Author.publisher_id
                    include: {
                        territory: Publisher.territory_id
                    }
                }
            }
        }
    }
}
```

```text output
error: Includes nest at most two levels
 --> main.jadpo:58:21
   |
58 |                     include: {
   |                     ^^^^^^^ a third level
  reason: Each include is one more database query, and a query's includes nest up to two levels so its cost stays easy to see; a deeper shape is a second query called from the action.
  next: Read the deeper records with a query of their own (human)
  rule: query.include_depth
  help: https://jadpo.dev/docs/errors/query.include_depth

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides what each book in the list carries, and how a reader finds its
publisher's territory.

## How to fix it

The question is whether whoever reads the list needs each publisher's
territory.

**If they do**, stop the includes at the publisher, give its
`territory_id`, and read the territory with a query of its own:

```jadpo fragment
type PublisherView = Output {
    pick: [Publisher.territory_id]
}
```

```jadpo fragment
    include: {
        author: {
            via: Book.author_id
            include: {
                publisher: Author.publisher_id
            }
        }
    }
```

```jadpo fragment
query territory_by_id(territory_id: Territory.id) fails TerritoryNotFound -> TerritoryView {
    from: Territory
    where: Territory.id == territory_id
    missing: TerritoryNotFound
}
```

```jadpo check project=e-include-depth-fix-1 hidden
type Name = Text {
    max_length: 80
}

entity Territory {
    id: Uuid
    name: Name
}

entity Publisher {
    id: Uuid
    territory_id: Territory.id {
        on_delete: OnDelete.restricted
    }
}

entity Author {
    id: Uuid
    publisher_id: Publisher.id {
        on_delete: OnDelete.restricted
    }
}

entity Book {
    id: Uuid
    author_id: Author.id {
        on_delete: OnDelete.cascade
    }
}

type TerritoryView = Output {
    pick: [Territory.name]
}

type PublisherView = Output {
    pick: [Publisher.territory_id]
}

type AuthorView = Output {
    publisher: PublisherView
}

type BookView = Output {
    pick: [Book.id]
    author: AuthorView
}

type TerritoryNotFound = Failure {
    kind: FailureKind.not_found
}

query books() -> List<BookView> {
    from: Book
    order: [Book.id asc]
    limit: 20
    include: {
        author: {
            via: Book.author_id
            include: {
                publisher: Author.publisher_id
            }
        }
    }
}

query territory_by_id(territory_id: Territory.id) fails TerritoryNotFound -> TerritoryView {
    from: Territory
    where: Territory.id == territory_id
    missing: TerritoryNotFound
}
```

`books` now costs three database queries: the books, their authors and
their publishers. Each publisher carries its `territory_id`, and
`territory_by_id` reads that territory when it is needed, as one more
query.

**If they don't**, stop the includes at the publisher and leave the
territory out:

```jadpo fragment
type PublisherView = Output {
    pick: [Publisher.id]
}
```

```jadpo check project=e-include-depth-fix-2 hidden
type Name = Text {
    max_length: 80
}

entity Territory {
    id: Uuid
    name: Name
}

entity Publisher {
    id: Uuid
    territory_id: Territory.id {
        on_delete: OnDelete.restricted
    }
}

entity Author {
    id: Uuid
    publisher_id: Publisher.id {
        on_delete: OnDelete.restricted
    }
}

entity Book {
    id: Uuid
    author_id: Author.id {
        on_delete: OnDelete.cascade
    }
}

type PublisherView = Output {
    pick: [Publisher.id]
}

type AuthorView = Output {
    publisher: PublisherView
}

type BookView = Output {
    pick: [Book.id]
    author: AuthorView
}

query books() -> List<BookView> {
    from: Book
    order: [Book.id asc]
    limit: 20
    include: {
        author: {
            via: Book.author_id
            include: {
                publisher: Author.publisher_id
            }
        }
    }
}
```

`books` costs three database queries, and its results no longer say
anything about territories.

## Related

- [Children and parents](../language/06-reads-and-writes.md#children-and-parents)
- [Named queries](../language/06-reads-and-writes.md#named-queries)
