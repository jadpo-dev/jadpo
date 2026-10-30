# `query.result_type`

```jadpo check project=e-result-type expect=query.result_type
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query titles(title: Book.title) -> List<Title> {
    from: Book
    where: Book.title == title
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `titles` reads `Book` records but returns `List<Title>`
 --> main.jadpo:12:36
   |
12 | query titles(title: Book.title) -> List<Title> {
   |                                    ^^^^^^^^^^^ not records of the entity
  reason: A query's result is `Book` or an object type its fields fill (`type BookView = Output { pick: [...] }`), as `T` for exactly one record, `T?` for at most one, `List<T>` or `Page<T>`, or a map layer, `FeatureCollection<G, P>`.
  next: Return `Book`, or an `Output` type that picks the fields you need (agent)
  rule: query.result_type
  help: https://jadpo.dev/docs/errors/query.result_type

check failed: 1 error, 1 file
```
