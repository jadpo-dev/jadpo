# Query records
<!-- topic: guide-query-records -->

Every read of stored records is a named query: one file that says which
entity it reads, which records it keeps, in what order and how many. You
call it by name, from a route or an action, and Jadpo writes the SQL.
This guide reads books: one by its id, and a list a page at a time that
a caller can narrow.

You start from an app with a `Book` entity that anyone may read, and a
`BookView` output type; [Store and change a record](02-store-and-change-a-record.md)
shows how to write them.

```jadpo check project=guide-query file=app.jadpo hidden
application {
    jadpo: "0.1"
}
```

```jadpo check project=guide-query file=domains/books/entities/book.jadpo hidden
/// A book the library lends.
entity Book {
    id: Uuid
    title: Text {
        min_length: 1
        max_length: 200
    }
    author: Text {
        min_length: 1
        max_length: 100
    }
    on_loan: Bool
    added_at: Instant {
        generated: Generated.on_create
    }

    policy: {
        read: [Access.public]
    }
}
```

```jadpo check project=guide-query file=domains/books/types/book_view.jadpo hidden
use books/entities { Book }

/// A book, as the API sends it.
type BookView = Output {
    pick: [Book.id, Book.title, Book.author, Book.on_loan]
}
```

## Read one record

Queries live in a domain's `queries/` folder, one to a file.

`domains/books/queries/book_by_id.jadpo`:

```jadpo check project=guide-query file=domains/books/queries/book_by_id.jadpo
use books/entities { Book, BookNotFound }
use books/types { BookView }

/// One book.
query book_by_id(book_id: Book.id) fails BookNotFound -> BookView {
    from: Book
    where: Book.id == book_id
    missing: BookNotFound
}
```

- `from:` names the entity.
- `where:` keeps the records that match. Fields are named with their
  entity, as `Book.id`.
- The result type says how many records come back. A plain `BookView`
  is exactly one, so the query needs `missing:`, the failure it gives
  when nothing matches. `BookNotFound` is the one Jadpo declares for
  every entity, and a route answers it with `404`.
- The query reads only the fields `BookView` needs.

## Read a list, a page at a time

A caller narrows the list with optional values, and pages through it
with a cursor. Give those as an `Input` type:

`domains/books/types/book_filter.jadpo`:

```jadpo check project=guide-query file=domains/books/types/book_filter.jadpo
use books/entities { Book }

/// What a caller may ask for when listing books.
type BookFilter = Input {
    author: Book.author {
        optional: true
    }
    on_loan: Book.on_loan {
        optional: true
    }
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}
```

`PageSize` is a standard type for how many records a page holds, from 1
to 100, and `PageCursor` the opaque cursor a page hands back for the
next one.

`domains/books/queries/list_books.jadpo`:

```jadpo check project=guide-query file=domains/books/queries/list_books.jadpo
use books/entities { Book }
use books/types { BookFilter, BookView }

/// Books, newest first, a page at a time, narrowed by what the caller asks.
query list_books(filter: BookFilter) -> Page<BookView> {
    from: Book
    where_present: Book.author == filter.author
    where_present: Book.on_loan == filter.on_loan
    order: [Book.added_at desc]
    page: {
        size: filter.page_size
        after: filter.after
    }
}
```

- `where_present:` applies its condition only when the value on the
  right is there: leave out `author` and every author comes back.
- `order:` is required for a list, so the same request always gives the
  same order.
- `Page<BookView>` comes back as one page and a cursor for the next.
  For a fixed number of records with no cursor, give
  `List<BookView>` and `limit:` instead of `page:`.

`jadpo sql` shows the statement it sends. A missing filter and the
first page's missing cursor are written `IS NULL OR`, so the query
always sends the same statement:

```sh
jadpo sql list_books
```

```text
query list_books
  SELECT "id", "title", "author", "on_loan", "added_at"
  FROM "book"
  WHERE ($1 IS NULL OR "author" = $1)
    AND ($2 IS NULL OR "on_loan" = $2)
    AND ($3 IS NULL OR ("added_at", "id") < ($3, $4))
  ORDER BY "added_at" DESC, "id" DESC
  LIMIT $5

  reads: Book, a page of BookView
  file: domains/books/queries/list_books.jadpo:5
  sends: 1 statement
  $1: filter.author · text
  $2: filter.on_loan · boolean
  $3: after: the cursor's added_at, or none · timestamptz
  $4: after: the cursor's id, or none · uuid
  $5: the page size + 1, to see if more follow · bigint
```

## Serve them

`domains/books/routes/books.jadpo`:

```jadpo check project=guide-query file=domains/books/routes/books.jadpo
use books/entities { Book }
use books/types { BookFilter, BookView }
use books/queries { book_by_id, list_books }

/// The library's catalogue.
routes Books {
    prefix: /books

    /// Lists books, newest first.
    GET / {
        authentication: RouteAuthentication.none
        input: BookFilter
        output: Page<BookView>
        run: list_books(input)
    }

    /// One book.
    GET /{book_id: Book.id} {
        authentication: RouteAuthentication.none
        output: BookView
        run: book_by_id(book_id)
    }
}
```

A `GET` route reads its input from the query string. Run
`jadpo api update` for the API record, then `jadpo dev`, and ask for
one author's books, two at a time:

```sh
curl 'http://localhost:3000/books?author=George%20Eliot&page_size=2'
```

```text
{"items":[],"next":null}
```

The catalogue is empty until something adds books; `next` holds the
cursor to send as `after` for the next page, and is `null` on the last.
A value the input type refuses never reaches the query:

```sh
curl 'http://localhost:3000/books?page_size=500'
```

```text
{"error":{"code":"invalid_input","message":"Request input is invalid.","request_id":"req_50d5ebd886cc877d","details":{"problems":[{"field":"page_size","rule":"max"}]}}}
```

## Who sees what

A query only ever returns records the caller may read: the entity's
`read:` rule is part of the SQL, so a record someone may not see is
never fetched. Here anyone may read a book. When records belong to
people, [Add sign-in](05-add-sign-in.md) and
[A user field on the record](../language/09-policy.md#a-user-field-on-the-record)
narrow it.

## Read more

[Named queries](../language/06-reads-and-writes.md#named-queries),
[Filters](../language/06-reads-and-writes.md#filters),
[Order and pages](../language/06-reads-and-writes.md#order-and-pages) and
[Counts, sums and groups](../language/06-reads-and-writes.md#counts-sums-and-groups).
