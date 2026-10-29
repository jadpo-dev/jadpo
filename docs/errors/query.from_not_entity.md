# `query.from_not_entity`

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
  help: docs/errors/query.from_not_entity.md

check failed: 1 error, 1 file
```
