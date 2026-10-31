# `query.other_domain`

A query in one domain reads the records of an entity that belongs to another domain.

## Why Jadpo refuses it

A domain owns its entities, and other domains see its data only through
what it makes public. A query reads its own domain's entities. Here
`shelf`, in `lending`, reads `Book`, which belongs to `catalogue`. If
that were allowed, `catalogue` could no longer change how it stores books
without breaking `lending`, and it would have no say in what it shares.

Jadpo won't choose for you between moving the query and asking
`catalogue` for the data, because that decides which domain owns the
shelf and what `catalogue` offers to others.

## What it looks like

`Book` is a public entity in the `catalogue` domain, and `lending` uses
it:

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

domains/lending/queries/shelf.jadpo:

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

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which domain owns the shelf, and what `catalogue` shares with
other domains.

## How to fix it

The question is whether the shelf belongs to `catalogue` or to `lending`.

**If it belongs to `catalogue`**, move the file to
`domains/catalogue/queries/shelf.jadpo`. Its contents stay the same:

```jadpo check project=e-other-domain-fix-1 file=domains/catalogue/queries/shelf.jadpo
use catalogue/entities { Book }

/// Every copy.
query shelf() -> List<Book> {
    from: Book
    order: [Book.id asc]
    limit: 20
}
```

```jadpo check project=e-other-domain-fix-1 file=app.jadpo hidden
application {}
```

```jadpo check project=e-other-domain-fix-1 file=domains/lending/actions/check_out.jadpo hidden
use catalogue/entities { Book }

/// Lends a copy.
action check_out(book: Book) -> Unit {}
```

```jadpo check project=e-other-domain-fix-1 file=domains/catalogue/entities/book.jadpo hidden migrate=initial
/// A copy on the shelves.
public entity Book {
    id: Uuid
}
```

`shelf` reads `Book` from inside its own domain. Only `catalogue`'s own
code can call it.

**If `lending` needs the list**, put the query in `catalogue`, mark it
`public`, and call it from `lending` through the domain's name. In
`domains/catalogue/queries/shelf.jadpo`:

```jadpo fragment
public query shelf() -> List<Book> {
```

```jadpo check project=e-other-domain-fix-2 file=domains/catalogue/queries/shelf.jadpo hidden
use catalogue/entities { Book }

/// Every copy.
public query shelf() -> List<Book> {
    from: Book
    order: [Book.id asc]
    limit: 20
}
```

And in `domains/lending/actions/browse_shelf.jadpo`:

```jadpo fragment
use catalogue/entities { Book }
use catalogue

/// The copies a borrower can choose from.
action browse_shelf() -> List<Book> {
    return catalogue.shelf()
}
```

```jadpo check project=e-other-domain-fix-2 file=domains/lending/actions/browse_shelf.jadpo hidden
use catalogue/entities { Book }
use catalogue

/// The copies a borrower can choose from.
action browse_shelf() -> List<Book> {
    return catalogue.shelf()
}
```

```jadpo check project=e-other-domain-fix-2 file=app.jadpo hidden
application {}
```

```jadpo check project=e-other-domain-fix-2 file=domains/lending/actions/check_out.jadpo hidden
use catalogue/entities { Book }

/// Lends a copy.
action check_out(book: Book) -> Unit {}
```

```jadpo check project=e-other-domain-fix-2 file=domains/catalogue/entities/book.jadpo hidden migrate=initial
/// A copy on the shelves.
public entity Book {
    id: Uuid
}
```

`catalogue` now offers `shelf` to other domains, and `lending` gets the
books by calling `catalogue.shelf()`. `catalogue` can change how it
stores books as long as `shelf` gives the same result.

## Related

- [Reads across domains](../language/06-reads-and-writes.md#reads-across-domains)
- [What can be public](../language/01-project-layout-and-modules.md#what-can-be-public)
- [Calls name their owner](../language/01-project-layout-and-modules.md#calls-name-their-owner)
