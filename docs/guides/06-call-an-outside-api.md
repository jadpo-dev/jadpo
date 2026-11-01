# Call an outside API
<!-- topic: guide-call-an-outside-api -->

Jadpo calls another service through a typed description of its API,
called a provider package, and a connection that says where to reach it
and with which key. Your code calls the service like any other function,
with typed input and output, and each way it can fail is a failure you
handle. This guide looks up a book's details at a book-data service,
Open Shelf, by its ISBN.

You start from an app with one domain, `books`. Open Shelf is made up
for this guide; for a real service, use its documentation, or import its
OpenAPI document (below).

## Describe the service

The app's own description of a service is a package in
`shared/providers/<name>/v<version>/`, one folder per version of the
service's API.

`shared/providers/open_shelf/v1/package.jadpo`:

```jadpo check project=guide-outside-api file=shared/providers/open_shelf/v1/package.jadpo
/// Details of published books, from Open Shelf.
package OpenShelf.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

`provider.jadpo` says how the API is reached: its protocol, its live and
sandbox addresses, how the key is sent, and where a refusal's code is
in the error body it sends back.

`shared/providers/open_shelf/v1/provider.jadpo`:

```jadpo check project=guide-outside-api file=shared/providers/open_shelf/v1/provider.jadpo
/// How Open Shelf's API is reached and called.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.open-shelf.example"
        sandbox: "https://sandbox.open-shelf.example"
    }
    credential: CredentialScheme.bearer
    error_code: "error.code"
}
```

The types the API takes and gives, each `public` so the app can use
them:

`shared/providers/open_shelf/v1/types/isbn_lookup.jadpo`:

```jadpo check project=guide-outside-api file=shared/providers/open_shelf/v1/types/isbn_lookup.jadpo
/// Which book to look up.
public type IsbnLookup = Object {
    isbn: Text {
        min_length: 10
        max_length: 13
    }
}
```

`shared/providers/open_shelf/v1/types/published_book.jadpo`:

```jadpo check project=guide-outside-api file=shared/providers/open_shelf/v1/types/published_book.jadpo
/// A published book, as Open Shelf describes it.
public type PublishedBook = Object {
    title: Text {
        max_length: 500
    }
    author: Text {
        max_length: 200
    }
    pages: Int? {
        min: 1
    }
}
```

Each error code the service documents is a failure, named so that its
code, `not_published`, is the one the service sends:

`shared/providers/open_shelf/v1/failures/not_published.jadpo`:

```jadpo check project=guide-outside-api file=shared/providers/open_shelf/v1/failures/not_published.jadpo
/// Open Shelf knows no book with that ISBN.
public type NotPublished = Failure {
    kind: FailureKind.not_found
    message: "No book has that ISBN."
}
```

An operation is one call: its method and path, its input and output,
and the failures it can give. `{isbn}` in the path is filled from the
input's field of that name.

`shared/providers/open_shelf/v1/resources/books.jadpo`:

```jadpo check project=guide-outside-api file=shared/providers/open_shelf/v1/resources/books.jadpo
public resource books {
    /// One book, by its ISBN.
    retrieve: {
        method: HttpMethod.get
        path: "/v1/books/{isbn}"
        input: IsbnLookup
        output: PublishedBook
        failures: [NotPublished]
        source: PackageSource.written
        docs: "https://docs.open-shelf.example/books"
    }
}
```

`source: PackageSource.written` and `docs:` say it was written from the
service's documentation, and where, so a reviewer can check it. An
operation that changes something at the service also says
`changes: true`, and whether it is safe to repeat.

## Connect to it

`app.jadpo`:

```jadpo check project=guide-outside-api file=app.jadpo
application {
    jadpo: "0.2"
    connections: {
        shelf: {
            package: OpenShelf.v1
            domains: [books]
            credential: config.open_shelf_api_key
            environment: config.open_shelf_environment
        }
    }
}
```

`domains:` lists the domains that may call it. The key and the choice
of sandbox or live service are settings:

`config.jadpo`:

```jadpo check project=guide-outside-api file=config.jadpo
config {
    /// Open Shelf's API key.
    open_shelf_api_key: Text {
        secret: true
    }
    /// Open Shelf's sandbox or its live API.
    open_shelf_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
```

The key is a secret: only the connection reads it, and it never appears
in a log or an error. On your machine, `jadpo config set
open_shelf_api_key` asks for it at a prompt.

## Call it

`domains/books/failures/unknown_isbn.jadpo`:

```jadpo check project=guide-outside-api file=domains/books/failures/unknown_isbn.jadpo
/// No published book has the ISBN given.
type UnknownIsbn = Failure {
    kind: FailureKind.not_found
    message: "No published book has that ISBN."
}
```

`domains/books/types/book_details.jadpo`:

```jadpo check project=guide-outside-api file=domains/books/types/book_details.jadpo
/// What the library shows about a book before adding it.
type BookDetails = Output {
    title: Text {
        max_length: 500
    }
    author: Text {
        max_length: 200
    }
}
```

`domains/books/actions/look_up_book.jadpo`:

```jadpo check project=guide-outside-api file=domains/books/actions/look_up_book.jadpo
use app { shelf }
use books/types { BookDetails }
use books/failures { UnknownIsbn }
use shared/providers/open_shelf

/// Looks a book up by its ISBN, to fill in the form for adding it.
action look_up_book(isbn: OpenShelf.IsbnLookup.isbn) fails UnknownIsbn -> BookDetails {
    var found: OpenShelf.PublishedBook = attempt shelf.books.retrieve(
        OpenShelf.IsbnLookup {
            isbn: isbn
        }
    ) {
        not_published: UnknownIsbn
    }
    return BookDetails {
        title: found.title
        author: found.author
    }
}
```

- `use app { shelf }` brings in the connection, and
  `shelf.books.retrieve` calls the operation through it.
- The block after the call turns each failure the operation gives into
  one of your own, keyed by its code. The compiler makes you handle every
  one, so a refusal you didn't plan for can't reach a caller by
  accident.
- A read like this one is retried for you when the service is briefly
  unavailable, and no call is made while the action holds a database
  transaction open.

Run `jadpo records update`. It writes `network.jadpo_record`, the
committed list of every outside address the app can reach, so adding a
host shows in review:

`network.jadpo_record` (written by `jadpo records update`):

```text
// Generated by Jadpo from the hosts the app can reach; update it with `jadpo records update`, never by hand.
record network 1

connection shelf
    for OpenShelf.v1
    host https://api.open-shelf.example
    host https://sandbox.open-shelf.example
```

Then check:

```sh
jadpo check
```

```text
Not yet confirmed
  Record each with `jadpo package record`, which checks a sandbox answer
  against its types (agent)
    shelf.books.retrieve
checked: 1 unconfirmed, 12 files
```

The check passes. "Not yet confirmed" means no real answer from the
service has been checked against the types you wrote. With a sandbox key
in `.env.local`, `jadpo package record shelf books.retrieve --input
'{"isbn": "9780141439549"}'` calls the sandbox once, checks the answer,
and keeps it, with its values replaced by placeholders, as a recording
in the package. It refuses a connection whose environment is live.

## Test it

A test never calls the service. `fakes:` gives the answers each call
gets, in order: a value of the operation's output type, or a failure's
code:

`domains/books/actions/look_up_book.test.jadpo`:

```jadpo check project=guide-outside-api file=domains/books/actions/look_up_book.test.jadpo
use app { shelf }
use books/types { BookDetails }
use books/failures { UnknownIsbn }
use shared/providers/open_shelf

test "a published book's details come back" {
    fakes: {
        shelf.books.retrieve: [
            OpenShelf.PublishedBook {
                title: "Middlemarch"
                author: "George Eliot"
                pages: 880
            }
        ]
    }
    var details: BookDetails = attempt look_up_book("9780141439549")
    assert details.title == "Middlemarch"
}

test "an ISBN nobody published is the library's own failure" {
    fakes: {
        shelf.books.retrieve: [not_published]
    }
    assert look_up_book("9780000000000") fails UnknownIsbn
}
```

```sh
jadpo test
```

```text
tested: 2 passed, 2 action tests
```

In `jadpo dev`, every call is faked too, so nothing leaves your machine
and you need no key: a call gets its recording, or else a made-up value
of its output type. List operations in `development: { sandbox: [...] }`
on the connection to send them to the service's sandbox instead.

## When the service publishes an OpenAPI document

`jadpo package import openapi <file> --as OpenShelf.v1` writes the
package from it, mapping fields to Jadpo's standard types where they
fit. `jadpo check` then lists each type the app's calls reach for you
to review against the service's documentation, and you mark each one
reviewed in `provider.jadpo`.

## Read more

[Kinds of package](../language/11-integrations.md#kinds-of-package),
[Connecting in app.jadpo](../language/11-integrations.md#connecting-in-appjadpo),
[Calling](../language/11-integrations.md#calling),
[Tests and fakes](../language/11-integrations.md#tests-and-fakes),
[Provider calls in development](../language/11-integrations.md#provider-calls-in-development)
and [Several changing calls in one action](../language/11-integrations.md#several-changing-calls-in-one-action).
To call the service from a route, see [Build an API](01-build-an-api.md);
an action that reads no stored record needs a line in `policy.jadpo`
saying who may run it.
