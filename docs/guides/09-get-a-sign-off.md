# Get a person's sign-off
<!-- topic: guide-get-a-sign-off -->

Some changes can hurt someone outside the code: removing a field a
client reads, letting more people see a record, or sending data to a
new outside host. `jadpo check` fails on each of them
until a person accepts it in an acceptance, a small file that says what
was decided and why. A coding agent drafts the file and then stops,
because the reason has to come from you. This guide removes a field
from an API's output and gets it accepted.

You start from the book catalogue in [Build an API](01-build-an-api.md),
committed to git, with its `BookView` sending each book's `author`. You
work on a branch, as for any change you would open a pull request for.

```jadpo check project=guide-sign-off file=app.jadpo hidden
application {}
```

```jadpo check project=guide-sign-off file=domains/books/entities/book.jadpo hidden
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

```jadpo check project=guide-sign-off file=domains/books/types/new_book.jadpo hidden
use books/entities { Book }

/// What a caller sends to add a book.
type NewBook = Input {
    title: Book.title
    author: Book.author
}
```

```jadpo check project=guide-sign-off file=domains/books/queries/book_by_id.jadpo hidden
use books/entities { Book, BookNotFound }
use books/types { BookView }

/// One book.
query book_by_id(book_id: Book.id) fails BookNotFound -> BookView {
    from: Book
    where: Book.id == book_id
    missing: BookNotFound
}
```

```jadpo check project=guide-sign-off file=domains/books/routes/books.jadpo hidden
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

## Make the change

Take `author` out of what the API sends:

`domains/books/types/book_view.jadpo`, changed. `Book.author` is gone from `pick:`.

```jadpo check project=guide-sign-off file=domains/books/types/book_view.jadpo
use books/entities { Book }

/// A book, as the API sends it.
type BookView = Output {
    pick: [Book.id, Book.title, Book.added_at]
}
```

`domains/books/actions/add_book.jadpo`, changed. The `author:` line is gone.

```jadpo check project=guide-sign-off file=domains/books/actions/add_book.jadpo
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
        added_at: book.added_at
    }
}
```

Run `jadpo api update`, which rewrites the API record. Its diff shows
the field gone:

```sh
git diff api/
```

```text
diff --git a/api/main.jadpo_record b/api/main.jadpo_record
index 7cb91b2..ec32289 100644
--- a/api/main.jadpo_record
+++ b/api/main.jadpo_record
@@ -20,7 +20,6 @@ GET /openapi.json
 
 output BookView
     added_at: Instant
-    author: Text { min_length: 1, max_length: 100 }
     id: Uuid
     title: Text { min_length: 1, max_length: 200 }
```

## The check stops for a person

```sh
jadpo check
```

```text
error: removing an output field can break clients: main.BookView.author
 --> api/main.jadpo_record:21:1
   |
21 | output BookView
   | ^^^^^^^^^^^^^^^ ApiChange.output_field_removed
  reason: Clients that read it find it gone. So a person accepts it: no acceptance this change adds lists `main.BookView.author: ApiChange.output_field_removed`.
  next: A person accepts it: `jadpo acceptance new <purpose>` writes the draft for them (human)
  rule: acceptance.missing
  help: https://jadpo.dev/docs/errors/acceptance.missing

check failed: 1 error, 7 files
```

`jadpo check` compares the change with your default branch where this
branch left it, and finds a client-facing field removed. That's
`ApiChange.output_field_removed`: any client that reads `author` would
find it gone.

`(human)` at the end of an error's `next:` line tells your coding agent
to stop and ask you which fix you want, because the choice is yours to
make (who can see data, what a setting holds). `(agent)` means your
agent can make the fix itself. Here the choice decides whether your
API's clients lose a field: put it back, or accept that it goes.

## Draft the acceptance

```sh
jadpo acceptance new remove_book_author
```

```text
Wrote acceptances/20261020T1400_remove_book_author.jadpo
    main.BookView.author  ApiChange.output_field_removed

decision: 1 change waits for a person's reason
 --> acceptances/20261020T1400_remove_book_author.jadpo:1
  next: Stop and hand over: a person reads the changes and writes why they are accepted on a `///` line above `acceptance`. (human)
```

It writes one file listing every change this branch makes that needs a
person, each as `subject: Kind.change`. An agent may run this; then it
stops, and `jadpo check` keeps failing:

```text
error: `acceptances/20261020T1400_remove_book_author.jadpo` has no reason
 --> acceptances/20261020T1400_remove_book_author.jadpo:1:1
  |
1 | acceptance {
  | ^^^^^^^^^^ no `///` reason above
  reason: An acceptance records a person's decision, so it says why on a `///` line above `acceptance`; an agent writes the draft and stops there.
  next: A person writes why these changes are accepted on a `///` line above `acceptance` (human)
  rule: acceptance.no_reason
  help: https://jadpo.dev/docs/errors/acceptance.no_reason

check failed: 1 error, 8 files
```

## Write the reason

Only a person writes the `///` line above `acceptance`, saying why the
change is safe:

`acceptances/20261020T1400_remove_book_author.jadpo`:

```jadpo check project=guide-sign-off file=acceptances/20261020T1400_remove_book_author.jadpo
/// No client reads a book's author: the catalogue pages stopped showing it in version 2.
acceptance {
    changes: {
        main.BookView.author: ApiChange.output_field_removed
    }
}
```

`jadpo check` passes. Commit the acceptance with the change. It counts
only in the change that adds it: once merged, it is history, and
editing it later is an error.

## Who approves it

The pull request is where the decision is reviewed. `jadpo new` writes
a `CODEOWNERS` file naming `acceptances/`, `api/`,
`access.jadpo_record` and `network.jadpo_record`, for you to put your
project's owners after. With your git host's branch protection, an
owner's approval of the pull request is what lets an acceptance in, and
the host records who gave it. CI runs `jadpo check` with the whole
history, so it judges the acceptances against the same base.

## What needs a sign-off

| Change | Kind |
|---|---|
| A route removed, a required input field added, a rule tightened, an output field removed or retyped, a failure code changed; an event removed or its fields changed the same ways | `ApiChange` |
| Anyone added to who may read or change something, a route opened to callers who aren't signed in, a sign-in setting loosened past its limit | `PolicyChange` |
| A git location added to, or widened in, `packages:`'s `own_sources:` | `PolicyChange` |
| A new domain | `DomainChange` |
| A connection reaching an outside host it didn't | `NetworkChange` |
| A third party's package installed from git | `PackageChange` |

Changes that only add need none: a new route, a new optional input
field, a new output field, or tighter policy. The record diffs still
show them.

## Read more

[Acceptances](../language/18-records-and-manifest.md#acceptances),
[The public API record](../language/07-http.md#the-public-api-record),
[Review and sign-off](../language/14-tooling.md#review-and-sign-off) and
the error pages [acceptance.missing](../errors/acceptance.missing.md) and
[acceptance.no_reason](../errors/acceptance.no_reason.md).

## Not built yet

- `jadpo approval`, the report of what a branch changes in routes,
  policy, failures and data for a reviewer to read, isn't built yet. Read
  the diffs of `api/`, `access.jadpo_record` and `network.jadpo_record`
  instead.
- Acceptances for dropping a column or table, for a package upgrade
  plan, and for raising a monitoring threshold aren't built yet; they
  come with the commands that make those changes.
