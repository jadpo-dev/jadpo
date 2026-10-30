# `query.include_depth`

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
