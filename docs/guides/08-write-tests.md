# Write tests
<!-- topic: guide-write-tests -->

Tests in Jadpo sit beside the code they test, and each one gets a
PostgreSQL database of its own. Test files end `.test.jadpo`, and a
test's database is made from your migrations and thrown away after it,
so tests never see each other's data and can run in any order. This guide tests a reading list:
an action on its own, then its routes, signed in as different people.

You start from an app where each `Reader` signs in and keeps a list of
`Entry` records, which only they may read and change, served at
`POST /entries`, `GET /entries` and `POST /entries/{entry_id}/finish`.
The action it tests is this one:

```jadpo check project=guide-tests file=app.jadpo hidden
use reading/entities { Reader }

application {}

principal {
    user: {
        subject: Reader.email
    }
}
```

```jadpo check project=guide-tests file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "reading_session"
    }
}
```

```jadpo check project=guide-tests file=domains/reading/entities/reader.jadpo hidden
/// Someone keeping a reading list.
entity Reader {
    id: Uuid
    email: Email {
        unique: true
    }

    policy: {
        create: [Access.system]
        read: [id]
    }
}
```

```jadpo check project=guide-tests file=domains/reading/types/entry_title.jadpo hidden
/// A book's title on a reading list.
type EntryTitle = Text {
    min_length: 1
    max_length: 200
}
```

```jadpo check project=guide-tests file=domains/reading/entities/entry.jadpo hidden
use reading/entities { Reader }
use reading/types { EntryTitle }

/// A book on someone's reading list.
entity Entry {
    id: Uuid
    reader_id: Reader.id {
        on_delete: OnDelete.cascade
    }
    title: EntryTitle
    finished_at: Instant?

    policy: {
        create: [reader_id]
        read: [reader_id]
        update: [reader_id]
    }
}
```

```jadpo check project=guide-tests file=domains/reading/types/new_entry.jadpo hidden
use reading/types { EntryTitle }

/// A book to add to the list.
type NewEntry = Input {
    title: EntryTitle
}
```

```jadpo check project=guide-tests file=domains/reading/types/entry_view.jadpo hidden
use reading/entities { Entry }

/// A book on the list, as the API sends it.
type EntryView = Output {
    pick: [Entry.id, Entry.title, Entry.finished_at]
}
```

```jadpo check project=guide-tests file=domains/reading/actions/add_entry.jadpo hidden
use reading/entities { Entry }
use reading/types { EntryView, NewEntry }

/// Adds a book to the caller's list.
action add_entry(input: NewEntry, caller: Principal.user) -> EntryView {
    var entry: Entry = Entry.create {
        set: {
            reader_id: caller.id
            title: input.title
            finished_at: none
        }
    }
    return EntryView {
        id: entry.id
        title: entry.title
        finished_at: entry.finished_at
    }
}
```

```jadpo check project=guide-tests file=domains/reading/queries/list_entries.jadpo hidden
use reading/entities { Entry }
use reading/types { EntryView }

/// The books on the list, in the order they were added.
query list_entries() -> List<EntryView> {
    from: Entry
    order: [Entry.id asc]
    limit: 100
}
```

```jadpo check project=guide-tests file=domains/reading/routes/entries.jadpo hidden
use reading/entities { Entry }
use reading/types { EntryView, NewEntry }
use reading/actions { add_entry, finish_entry }
use reading/queries { list_entries }

/// The caller's reading list.
routes Entries {
    prefix: /entries

    /// Adds a book.
    POST / {
        input: NewEntry
        output: EntryView
        status: SuccessStatus.created
        run: add_entry(input, current_principal.user)
    }

    /// The books on the list.
    GET / {
        output: List<EntryView>
        run: list_entries()
    }

    /// Marks a book as read.
    POST /{entry_id: Entry.id}/finish {
        output: EntryView
        run: finish_entry(entry_id)
    }
}
```

```jadpo check project=guide-tests file=domains/reading/failures/reader_exists.jadpo hidden
/// Someone has that address already.
type ReaderExists = Failure {
    kind: FailureKind.conflict
}
```

`domains/reading/actions/finish_entry.jadpo`:

```jadpo check project=guide-tests file=domains/reading/actions/finish_entry.jadpo
use reading/entities { Entry, EntryNotFound }
use reading/types { EntryView }

/// Marks a book on the list as read, now.
action finish_entry(entry_id: Entry.id) fails EntryNotFound -> EntryView {
    var entry: Entry = attempt Entry.get(entry_id)
    var finished: Entry = Entry.update(entry) {
        set: {
            finished_at: clock.now
        }
    }
    return EntryView {
        id: finished.id
        title: finished.title
        finished_at: finished.finished_at
    }
}
```

## Make the data a test starts from

A fixture builds data for tests, in a domain's `fixtures/` folder. Only
tests call fixtures, and a fixture may write straight to an entity,
which a test can't:

`domains/reading/fixtures/a_reader.jadpo`:

```jadpo check project=guide-tests file=domains/reading/fixtures/a_reader.jadpo
use reading/entities { Reader }
use reading/failures { ReaderExists }

/// A reader with the address given.
fixture a_reader(email: Reader.email) -> Reader {
    return attempt Reader.create {
        set: {
            email: email
        }
        conflict: {
            Reader.email: ReaderExists
        }
    }
}
```

`domains/reading/fixtures/an_entry.jadpo`:

```jadpo check project=guide-tests file=domains/reading/fixtures/an_entry.jadpo
use reading/entities { Entry, Reader }
use reading/types { EntryTitle }

/// A book on the reader's list, not yet read.
fixture an_entry(reader: Reader, title: EntryTitle) -> Entry {
    return Entry.create {
        set: {
            reader_id: reader.id
            title: title
            finished_at: none
        }
    }
}
```

## Test an action

An action test calls the action directly, as code does. `clock:` fixes
the time the test runs at, so a value worked out from `clock.now` can be
checked exactly:

`domains/reading/actions/finish_entry.test.jadpo`:

```jadpo check project=guide-tests file=domains/reading/actions/finish_entry.test.jadpo
use reading/entities { Entry, Reader }
use reading/types { EntryView }
use reading/fixtures { a_reader, an_entry }

test "finishing a book records when" {
    clock: "2099-03-01T09:30:00Z"
    var ann: Reader = a_reader("ann@example.com")
    var entry: Entry = an_entry(ann, "Middlemarch")
    var finished: EntryView = attempt finish_entry(entry.id)
    assert finished.finished_at == "2099-03-01T09:30:00Z"
}
```

- A test's name is a sentence saying what it proves.
- `attempt` passes the action's failures on: a test that hits one fails,
  naming it.
- `assert` takes any condition. `assert finish_entry(id) fails
  EntryNotFound` passes only when the call fails with that failure.

## Test a route

A route test sends a request through the whole HTTP boundary: the input
is decoded, the caller signed in, policy checked and the failure turned
into a status, as for a real client. `as:` makes the request as someone,
without going through sign-in:

`domains/reading/routes/entries.test.jadpo`:

```jadpo check project=guide-tests file=domains/reading/routes/entries.test.jadpo
use reading/entities { Entry, Reader }
use reading/types { NewEntry }
use reading/fixtures { a_reader, an_entry }

test "adding a book answers 201" {
    var ann: Reader = a_reader("ann@example.com")
    var response: Response = request {
        method: HttpMethod.post
        path: "/entries"
        as: Principal.user(ann)
        input: NewEntry {
            title: "Middlemarch"
        }
    }
    assert response.status == 201
}

test "the list needs someone signed in" {
    var response: Response = request {
        method: HttpMethod.get
        path: "/entries"
    }
    assert response.status == 401
    assert response.code == "authentication_required"
}

test "nobody finishes another reader's book" {
    var ann: Reader = a_reader("ann@example.com")
    var bo: Reader = a_reader("bo@example.com")
    var entry: Entry = an_entry(ann, "Middlemarch")
    var response: Response = request {
        method: HttpMethod.post
        path: "/entries/{entry_id}/finish"
        params: {
            entry_id: entry.id
        }
        as: Principal.user(bo)
    }
    assert response.status == 404
    var listed: Response = request {
        method: HttpMethod.get
        path: "/entries"
        as: Principal.user(bo)
    }
    assert listed.output == []
}
```

- `request { }` takes the method, the path with its `params:`, the
  caller in `as:`, and the `input:`. Leave out `as:` to call as nobody.
- The `Response` has the `status`, the error's `code`, and the
  `output`.
- The last test proves that policy keeps one reader's list from
  another: Bo gets a `404` for Ann's entry, as if it weren't there.

## Run them

Run `jadpo migrate new initial` and `jadpo api update` if you haven't,
then:

```sh
jadpo test
```

```text
tested: 4 passed, 1 action test, 3 route tests
```

Jadpo works out each test's kind from what it calls, and counts each
kind apart, since an action test doesn't prove the route decodes its
input or checks who is signed in. When a test fails, it shows each value
in the assertion and the command that reruns just that test:

```text
FAIL  finishing a book records when
 --> domains/reading/actions/finish_entry.test.jadpo:10

  assert finished.finished_at == "2099-03-01T09:31:00Z"
         |        |
         |        "2099-03-01T09:30:00Z"
         EntryView { id: "03b506f3-b9c0-71f9-8d76-e4864446acb7", title: "Middlemarch", finished_at: "2099-03-01T09:30:00Z" }

  expected: "2099-03-01T09:31:00Z"
  actual: "2099-03-01T09:30:00Z"

  rule: test.assertion_failed
  rerun: jadpo test domains/reading/actions/finish_entry.test.jadpo:10 --seed 1

test failed: 1 failed, 3 passed, 1 action test, 3 route tests
```

To run fewer tests, name a file, `jadpo test <file>`, or a test,
`jadpo test <file>:<line>`; or use `--name <text>`, `--kind route`, or
`--failed` for those that failed last time. `jadpo test --watch` reruns
the tests a change reaches each time you save.

## Other things tests do

- **Calls to other services** are never made: `fakes:` gives each call
  its answer ([Call an outside API](06-call-an-outside-api.md)).
- **Emails** are caught, and `email.sent` lists them
  ([Add sign-in](05-add-sign-in.md)).
- **Schedules** run with `.run()`, and the work they start finishes
  before the next line ([Run background work](07-run-background-work.md)).
- **`clock.advance(1d)`** moves a test's clock on.

## Read more

[Where tests live](../language/13-testing.md#where-tests-live),
[Writing a test](../language/13-testing.md#writing-a-test),
[Kinds of test](../language/13-testing.md#kinds-of-test),
[Acting as a user](../language/13-testing.md#acting-as-a-user),
[Controlling time](../language/13-testing.md#controlling-time),
[Shared test data: fixtures](../language/13-testing.md#shared-test-data-fixtures),
[Assertions](../language/13-testing.md#assertions),
[Running tests](../language/13-testing.md#running-tests) and
[The test database](../language/13-testing.md#the-test-database).
