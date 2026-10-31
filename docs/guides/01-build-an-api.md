# Build an API
<!-- topic: guide-build-an-api -->

An API in Jadpo is a set of routes, each saying what it takes in, what
it sends back and what it runs, and Jadpo does the rest. It checks every
request against the input type, turns failures into HTTP errors, keeps
a committed record of the API and writes an OpenAPI document for it. This guide adds two routes to a book catalogue: one that adds a
book and one that reads it back.

You start from an app with a `Book` entity, an action `add_book` that
stores one, and a query `book_by_id` that reads one.
[Store and change a record](02-store-and-change-a-record.md) and
[Query records](03-query-records.md) show how to write those.

```jadpo check project=guide-api file=app.jadpo hidden
application {
    jadpo: "0.1"
}
```

```jadpo check project=guide-api file=domains/books/entities/book.jadpo hidden
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
    added_at: Instant {
        generated: Generated.on_create
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=guide-api file=domains/books/actions/add_book.jadpo hidden
use books/entities { Book }
use books/types { BookView, NewBook }

/// Adds a book to the catalogue.
action add_book(input: NewBook) -> BookView {
    var book: Book = Book.create {
        set: {
            title: input.title
            author: input.author
        }
    }
    return BookView {
        id: book.id
        title: book.title
        author: book.author
        added_at: book.added_at
    }
}
```

```jadpo check project=guide-api file=domains/books/queries/book_by_id.jadpo hidden
use books/entities { Book, BookNotFound }
use books/types { BookView }

/// One book.
query book_by_id(book_id: Book.id) fails BookNotFound -> BookView {
    from: Book
    where: Book.id == book_id
    missing: BookNotFound
}
```

## Say what comes in and what goes out

A route's input is a type built on `Input`, and its output a type built
on `Output`. Each lists its fields; a field can take its type, rules
included, from an entity's field, so an input refuses the same values
the database would.

`domains/books/types/new_book.jadpo`:

```jadpo check project=guide-api file=domains/books/types/new_book.jadpo
use books/entities { Book }

/// What a caller sends to add a book.
type NewBook = Input {
    title: Book.title
    author: Book.author
}
```

`domains/books/types/book_view.jadpo`:

```jadpo check project=guide-api file=domains/books/types/book_view.jadpo
use books/entities { Book }

/// A book, as the API sends it.
type BookView = Output {
    pick: [Book.id, Book.title, Book.author, Book.added_at]
}
```

`pick:` sends exactly the entity's fields it names, with their types. A
field you add to `Book` later stays private until you pick it.

## Add the routes

Routes live in a route group, in a domain's `routes/` folder. The
group's `prefix:` starts every path in it.

`domains/books/routes/books.jadpo`:

```jadpo check project=guide-api file=domains/books/routes/books.jadpo
use books/entities { Book }
use books/types { BookView, NewBook }
use books/actions { add_book }
use books/queries { book_by_id }

/// The library's catalogue.
routes Books {
    prefix: /books

    /// Adds a book.
    POST / {
        authentication: RouteAuthentication.none
        input: NewBook
        output: BookView
        status: SuccessStatus.created
        run: add_book(input)
    }

    /// One book.
    GET /{book_id: Book.id} {
        authentication: RouteAuthentication.none
        output: BookView
        run: book_by_id(book_id)
    }
}
```

- `input:` is read from the request's JSON body for `POST`. A body that
  doesn't fit `NewBook` is answered `422 invalid_input`, listing each
  field's problem, and never reaches the action.
- `status: SuccessStatus.created` answers `201`. Without it a route
  answers `200`.
- `/{book_id: Book.id}` is a path placeholder with its type. A value
  that isn't a valid id is a `422` too.
- `run:` names the action or query the route runs, with the values it
  passes.
- Every route needs a signed-in caller unless it says otherwise.
  `authentication: RouteAuthentication.none` lets anyone call these two;
  [Add sign-in](05-add-sign-in.md) takes it away again.

`book_by_id` can fail with `BookNotFound`, the failure Jadpo declares
for every entity, so the route answers `404 book_not_found` for a book
that isn't there. [Handle failures](04-handle-failures.md) declares
failures of your own.

## Write the API record

Each API keeps a committed record of its routes, fields and failure
codes, so every change a client would notice shows as a diff of one
file. You never write it: `jadpo check` fails while it disagrees with
the routes, and `jadpo api update` rewrites it.

```sh
jadpo api update
```

```text
Wrote api/main.jadpo_record
    version  9c0c5d256937

next: git diff api/  shows what changed for the API's clients
```

`api/main.jadpo_record` (written by `jadpo api update`, never edited by
hand):

```text
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

POST /books
    authentication none
    input NewBook (body)
    output BookView
    status 201

GET /books/{book_id: Uuid}
    authentication none
    output BookView
    status 200
    fails book_not_found

GET /openapi.json
    generated openapi
    authentication none
    status 200

output BookView
    added_at: Instant
    author: Text { min_length: 1, max_length: 100 }
    id: Uuid
    title: Text { min_length: 1, max_length: 200 }

input NewBook
    author: Text { min_length: 1, max_length: 100 }
    title: Text { min_length: 1, max_length: 200 }

failure book_not_found
    status 404
```

The app declares no API of its own, so it has one, called `main`.
`GET /openapi.json` is Jadpo's: it serves the API's OpenAPI document.
Run `jadpo api update` again after every change to a route or a type it
uses, and commit the record with the code. A change that breaks
clients, such as removing a field, also needs a person's sign-off;
[Get a person's sign-off](09-get-a-sign-off.md) covers that.

## Call it

```sh
jadpo dev
```

```text
jadpo dev, started 10:46:31
listening on http://localhost:3000
    routes    3 routes
    database  disposable PostgreSQL, gone when dev stops
serving: saves reload, press ctrl-c to quit
```

In another terminal, add a book:

```sh
curl -i -X POST http://localhost:3000/books \
  -H 'content-type: application/json' \
  -d '{"title": "Middlemarch", "author": "George Eliot"}'
```

```text
HTTP/1.1 201 Created
content-type: application/json
x-request-id: req_449f27296665cedd
content-length: 129
date: Sat, 31 Oct 2026 10:46:33 GMT

{"id":"01a19191-7b3e-7b55-a9a4-9b723e457c37","title":"Middlemarch","author":"George Eliot","added_at":"2026-10-31T10:46:33.022Z"}
```

Read it back with the id it gave you; yours differs:

```sh
curl http://localhost:3000/books/01a19191-7b3e-7b55-a9a4-9b723e457c37
```

```text
{"id":"01a19191-7b3e-7b55-a9a4-9b723e457c37","title":"Middlemarch","author":"George Eliot","added_at":"2026-10-31T10:46:33.022Z"}
```

A book that isn't there is a `404`, in the error body every client
gets:

```text
{"error":{"code":"book_not_found","message":"Not found.","request_id":"req_0ba7b3845b9a0a4d"}}
```

## The OpenAPI document

`jadpo check` writes the API's OpenAPI 3.1 document to
`build/openapi/main.json`, and the app serves the same document at
`/openapi.json`. It lists every route with its input, its output, its
success status and the failures it can reach, and each `///` comment
becomes a description. Generate a client from it, or open it in any
OpenAPI viewer.

## Read more

[Route groups](../language/07-http.md#route-groups),
[Paths and placeholders](../language/07-http.md#paths-and-placeholders),
[One input](../language/07-http.md#one-input),
[Output and status](../language/07-http.md#output-and-status),
[How failures reach the client](../language/07-http.md#how-failures-reach-the-client),
[The public API record](../language/07-http.md#the-public-api-record) and
[OpenAPI](../language/07-http.md#openapi).
