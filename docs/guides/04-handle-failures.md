# Handle failures
<!-- topic: guide-handle-failures -->

In Jadpo, a way an action can fail is a type of its own, listed in the
action's signature, and the compiler makes every caller deal with it.
There are no exceptions to forget. This guide refuses to lend a book
that is already out, shows the client a `409`, and lets one caller turn
the failure into an ordinary answer.

You start from an app with a `Book` entity that has an `on_loan` field,
a `LoanView` output type, and a fixture that stores a book for tests;
[Store and change a record](02-store-and-change-a-record.md) and
[Write tests](08-write-tests.md) show how to write those.

```jadpo check project=guide-failures file=app.jadpo hidden
application {}
```

```jadpo check project=guide-failures file=domains/books/entities/book.jadpo hidden
/// A book the library lends.
entity Book {
    id: Uuid
    title: Text {
        min_length: 1
        max_length: 200
    }
    on_loan: Bool

    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
    }
}
```

```jadpo check project=guide-failures file=domains/books/types/loan_view.jadpo hidden
use books/entities { Book }

/// A book's loan, as the API sends it.
type LoanView = Output {
    pick: [Book.id, Book.on_loan]
}
```

```jadpo check project=guide-failures file=domains/books/fixtures/shelved_book.jadpo hidden
use books/entities { Book }

/// A book on the shelf.
fixture shelved_book() -> Book {
    return Book.create {
        set: {
            title: "Middlemarch"
            on_loan: false
        }
    }
}
```

## Declare the failure

A failure is a type built on `Failure`, in a domain's `failures/`
folder:

`domains/books/failures/book_on_loan.jadpo`:

```jadpo check project=guide-failures file=domains/books/failures/book_on_loan.jadpo
/// The book is out on loan already.
type BookOnLoan = Failure {
    kind: FailureKind.conflict
    message: "That book is out on loan."
}
```

- `kind:` says what sort of failure it is, and so the HTTP status a
  client gets. A failure never names a status itself.
- `message:` is fixed text that is safe to show anyone.
- The code clients see is the name in lower snake case,
  `book_on_loan`, and it stays the same so that clients can rely on it.

These are the kinds you declare:

| Kind | Status | Use it when |
|---|---:|---|
| `invalid_value` | 422 | a well-formed value breaks a rule |
| `rejected` | 422 | a business rule refuses the operation |
| `not_found` | 404 | the record doesn't exist |
| `conflict` | 409 | the current state conflicts with the request |
| `not_permitted` | 403 | the caller is known but may not do this |
| `rate_limited` | 429 | the caller must wait before trying again |
| `unavailable` | 503 | something it depends on is briefly unavailable |

The runtime raises the rest itself, such as `401` when nobody is signed
in, or a `500` for a bug.

## Raise it, and say that you can

`reject` ends the action with a failure. `fails` lists every failure
that can leave it, before the result type:

`domains/books/actions/lend_book.jadpo`:

```jadpo check project=guide-failures file=domains/books/actions/lend_book.jadpo
use books/entities { Book, BookNotFound }
use books/types { LoanView }
use books/failures { BookOnLoan }

/// Lends a book that is on the shelf.
action lend_book(book_id: Book.id) fails BookNotFound, BookOnLoan -> LoanView {
    var book: Book = attempt Book.get(book_id)
    if (book.on_loan) {
        reject BookOnLoan
    }
    var lent: Book = Book.update(book) {
        set: {
            on_loan: true
        }
    }
    return LoanView {
        id: lent.id
        on_loan: lent.on_loan
    }
}
```

`Book.get` can fail with `BookNotFound`, and `attempt` passes that on,
so it is in the list too. The compiler checks the list both ways: a
failure that can leave but isn't listed is an error, and so is one that
is listed but can never happen.

A route that runs `lend_book` needs nothing more. Each failure reaches
the client as its status, in the same error body every failure has:
`{"error": {"code": "book_on_loan", "message": "That book is out on loan.", "request_id": "…"}}`.
The route's OpenAPI document lists both failures under their statuses.

## Handle it with `match`

A caller that wants to answer a failure itself, rather than pass it on,
uses `match`, with one arm for success and one for each failure. The
library's self-service kiosk shows a message when the book is out, so
being out isn't an error there:

`domains/books/types/lend_outcome.jadpo`:

```jadpo check project=guide-failures file=domains/books/types/lend_outcome.jadpo
/// What happened when a kiosk asked to lend a book.
type LendOutcome = Enum {
    lent
    already_out
}
```

`domains/books/actions/lend_at_kiosk.jadpo`:

```jadpo check project=guide-failures file=domains/books/actions/lend_at_kiosk.jadpo
use books/entities { Book, BookNotFound }
use books/types { LendOutcome }
use books/failures { BookOnLoan }
use books/actions { lend_book }

/// Lends a book at the self-service kiosk, which shows a message rather
/// than an error when the book is out.
action lend_at_kiosk(book_id: Book.id) fails BookNotFound -> LendOutcome {
    return match lend_book(book_id) {
        success -> LendOutcome.lent
        BookOnLoan -> LendOutcome.already_out
        BookNotFound -> propagate
    }
}
```

Every failure needs an arm, and there is no catch-all, so a failure
added to `lend_book` later stops this file compiling until someone
decides what the kiosk does with it. An arm can recover with a value,
as `BookOnLoan` does here; turn it into another failure, with
`reject SomeOtherFailure`; or pass it on, with `propagate`.

## Serve it and test it

`domains/books/routes/books.jadpo`:

```jadpo check project=guide-failures file=domains/books/routes/books.jadpo
use books/entities { Book }
use books/types { LoanView }
use books/actions { lend_book }

/// The library's books.
routes Books {
    prefix: /books

    /// Lends a book.
    POST /{book_id: Book.id}/loan {
        authentication: RouteAuthentication.none
        output: LoanView
        run: lend_book(book_id)
    }
}
```

`domains/books/actions/lend_book.test.jadpo`:

```jadpo check project=guide-failures file=domains/books/actions/lend_book.test.jadpo
use books/entities { Book }
use books/types { LendOutcome, LoanView }
use books/failures { BookOnLoan }
use books/actions { lend_at_kiosk }
use books/fixtures { shelved_book }

test "a book out on loan can't be lent again" {
    var book: Book = shelved_book()
    var lent: LoanView = attempt lend_book(book.id)
    assert lent.on_loan
    assert lend_book(book.id) fails BookOnLoan
}

test "the kiosk says the book is out" {
    var book: Book = shelved_book()
    var first: LendOutcome = attempt lend_at_kiosk(book.id)
    var second: LendOutcome = attempt lend_at_kiosk(book.id)
    assert first == LendOutcome.lent
    assert second == LendOutcome.already_out
}

test "lending a book on loan answers 409" {
    var book: Book = shelved_book()
    var first: Response = request {
        method: HttpMethod.post
        path: "/books/{book_id}/loan"
        params: {
            book_id: book.id
        }
    }
    assert first.status == 200
    var second: Response = request {
        method: HttpMethod.post
        path: "/books/{book_id}/loan"
        params: {
            book_id: book.id
        }
    }
    assert second.status == 409
    assert second.code == "book_on_loan"
}
```

`assert ... fails BookOnLoan` passes when the call fails with exactly
that failure. Run `jadpo api update` for the API record, then the
tests:

```sh
jadpo test
```

```text
tested: 3 passed, 2 action tests, 1 route test
```

## Other places a failure comes from

The compiler knows what else can go wrong, and makes you name a failure
for each:

- a query for one record names one in `missing:`
  ([Query records](03-query-records.md));
- a write that can break a unique rule names one per rule in
  `conflict:` ([Store and change a record](02-store-and-change-a-record.md));
- a value worked out at run time that may break its type's rules, such
  as `attempt LoanDays(days) { max: LoanTooLong }`.

A failure can also carry fields: `client:` ones the caller sees under
`details`, and `internal:` ones that only reach your logs.

## Read more

[Declaring a failure](../language/04-failures.md#declaring-a-failure),
[Kinds](../language/04-failures.md#kinds),
[What the client sees](../language/04-failures.md#what-the-client-sees),
[Saying what can fail](../language/04-failures.md#saying-what-can-fail-fails),
[Passing a failure on](../language/04-failures.md#passing-a-failure-on-attempt),
[Turning one failure into another](../language/04-failures.md#turning-one-failure-into-another) and
[Handling a failure in match](../language/04-failures.md#handling-a-failure-in-match).
