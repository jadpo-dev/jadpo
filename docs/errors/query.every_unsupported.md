# `query.every_unsupported`

A query says `limit: every` where it can't read every record that matches: a page's `size:`, an include, or a list ordered by `nearest`.

## Why Jadpo refuses it

`limit: every` reads every record that matches, up to 10,000, in one
list: what a request such as "export all my todos" asks for. It goes on a
query that returns a `List<T>`, a map layer or a list of groups. The other
places a list is bounded take a number:

- A page holds `size:` records at a time, and its cursor reaches the
  rest, so a page of every record would be the whole list.
- An include's `limit:` bounds the records it reads for each record it
  is included in.
- A list ordered by `nearest` returns at most 100 records, since its
  index searches only a few candidates beyond its limit.

## What it looks like

main.jadpo:

```jadpo check project=e-every-page expect=query.every_unsupported
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
}

query shelf(after: PageCursor?) -> Page<Book> {
    from: Book
    order: [Book.title asc]
    page: {
        size: every
        after: after
    }
}
```

```text output
error: A page can't hold `every` record
 --> main.jadpo:14:15
   |
14 |         size: every
   |               ^^^^^ a page's size
  reason: A page holds `size:` records at a time, and its cursor reaches the rest; every record that matches, at once, is a query returning `List<T>` with `limit: every`.
  next: Give `size:` a number or a `PageSize`, or return `List<T>` with `limit: every` (agent)
  rule: query.every_unsupported
  help: https://jadpo.dev/docs/errors/query.every_unsupported

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An include's `limit:` is refused the same way:

main.jadpo:

```jadpo check project=e-every-include expect=query.every_unsupported
type Title = Text {
    max_length: 80
}

entity Shelf {
    id: Uuid
    name: Title
}

entity Book {
    id: Uuid
    shelf_id: Shelf.id {
        on_delete: OnDelete.cascade
    }
    title: Title
}

type ShelfView = Output {
    pick: [Shelf.id, Shelf.name]
    books: List<Book>
}

query shelf_with_books(shelf_id: Shelf.id) -> ShelfView? {
    from: Shelf
    where: Shelf.id == shelf_id
    include: {
        books: {
            via: Book.shelf_id
            order: [Book.title asc]
            limit: every
        }
    }
}
```

```text output
error: An include can't read `every` record
 --> main.jadpo:30:20
   |
30 |             limit: every
   |                    ^^^^^ an include's limit
  reason: An include's `limit:` bounds the records it reads for each record it is included in, so it is a number; every record that matches is a query of its own, with `limit: every`.
  next: Give `limit:` a number, or read the records with a query of their own (agent)
  rule: query.every_unsupported
  help: https://jadpo.dev/docs/errors/query.every_unsupported

check failed: 1 error, 1 file
```

## How to fix it

**If the caller should get every book at once**, return a `List<Book>`
with `limit: every`:

```jadpo check project=e-every-page-fix-1
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
}

query shelf() -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: every
}
```

`shelf` gives every book, in title order. When more than 10,000 match it
gives none of them, and the action that called it stops with a fault.

**If the caller reads them a page at a time**, give `size:` a number or a
`PageSize`:

```jadpo fragment
    page: {
        size: 50
        after: after
    }
```

```jadpo check project=e-every-page-fix-2 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
}

query shelf(after: PageCursor?) -> Page<Book> {
    from: Book
    order: [Book.title asc]
    page: {
        size: 50
        after: after
    }
}
```

Each call gives up to 50 books and a `next` cursor while more remain.

**For an include**, give its `limit:` a number, or read every book of the
shelf with a query of its own:

```jadpo check project=e-every-include-fix
type Title = Text {
    max_length: 80
}

entity Shelf {
    id: Uuid
    name: Title
}

entity Book {
    id: Uuid
    shelf_id: Shelf.id {
        on_delete: OnDelete.cascade
    }
    title: Title
}

query books_on_shelf(shelf_id: Shelf.id) -> List<Book> {
    from: Book
    where: Book.shelf_id == shelf_id
    order: [Book.title asc]
    limit: every
}
```

`books_on_shelf` gives every book on the shelf, up to 10,000, in title
order.

## Related

- [Every record that matches](../language/06-reads-and-writes.md#every-record-that-matches)
- [Pages](../language/06-reads-and-writes.md#pages)
- [`query.limit_unbounded`](query.limit_unbounded.md)
