# `query.other_domain`

```jadpo check project=e-other-domain file=app.jadpo hidden
application {}
```

```jadpo check project=e-other-domain file=domains/lending/actions/check_out.jadpo hidden
use catalogue/entities { Book }

/// Lends a copy.
action check_out(book: Book) -> Unit {}
```

```jadpo check project=e-other-domain file=domains/catalogue/entities/book.jadpo hidden migrate=initial
/// A copy on the shelves.
public entity Book {
    id: Uuid
}
```

```jadpo check project=e-other-domain file=domains/lending/queries/shelf.jadpo expect=query.other_domain
use catalogue/entities { Book }

/// Every copy.
query shelf() -> List<Book> {
    from: Book
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `shelf` reads `Book`, an entity of domain `catalogue`
 --> domains/lending/queries/shelf.jadpo:5:11
  |
5 |     from: Book
  |           ^^^^ domain `catalogue`
  reason: A query reads its own domain's entities only; another domain's data comes from that domain's public queries, so `catalogue` decides what it shares.
  next: Call a public query of `catalogue`, or move `shelf` into `catalogue` (human)
  rule: query.other_domain
  help: https://jadpo.dev/docs/errors/query.other_domain

check failed: 1 error, 4 files
```
