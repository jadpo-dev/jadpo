# Store and change a record
<!-- topic: guide-store-and-change-a-record -->

A record your app keeps is an entity: think of a table, where each field
is a column with a Jadpo type, and the entity also says who may create,
read, change and delete its records. This guide stores books, then adds,
changes and deletes them from actions. Jadpo writes the SQL and the
migrations; you write neither.

You start from an app with one domain, `books`, holding a type for a
book's title.

```jadpo check project=guide-store file=app.jadpo hidden
application {}
```

`domains/books/types/book_title.jadpo`:

```jadpo check project=guide-store file=domains/books/types/book_title.jadpo
/// What a book is called: at least one character, at most 200.
type BookTitle = Text {
    min_length: 1
    max_length: 200
}
```

## Declare the entity

Entities have their own folder in a domain, `entities/`:

`domains/books/entities/book.jadpo`:

```jadpo check project=guide-store file=domains/books/entities/book.jadpo
use books/types { BookTitle }

/// A book the library lends.
entity Book {
    id: Uuid
    title: BookTitle
    isbn: Text {
        min_length: 10
        max_length: 13
        unique: true
    }
    on_loan: Bool
    added_at: Instant {
        generated: Generated.on_create
    }
    changed_at: Instant {
        generated: Generated.on_change
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }
}
```

- `id: Uuid` is the record's key. Every entity has one, and Jadpo makes
  a new one for each record.
- A field's type carries its rules into the database: `title` is a
  `BookTitle`, and `isbn` is 10 to 13 characters.
- `unique: true` makes the database refuse a second book with the same
  ISBN.
- `generated:` fields are filled by Jadpo, never by your code:
  `on_create` once, `on_change` on every update.
- `policy:` says who may do each thing. `Access.public` means anyone,
  which keeps this guide short; a real app names its signed-in people,
  as [Add sign-in](05-add-sign-in.md) shows.

## Write the migration

A migration is the SQL that changes the database to match your entities.
`jadpo check` fails while they disagree, and `jadpo migrate new` writes
the next one, comparing the entities with the last migration:

```sh
jadpo migrate new initial
```

```text
  compared the entities with an empty database

  Book
    + table  book                      new entity

  System tables
    + table  jadpo_circuit_breakers    system table for circuit breakers
    + table  jadpo_deliveries          system table for deliveries
    + table  jadpo_handled_deliveries  system table for handled deliveries
    + table  jadpo_housekeeping        system table for housekeeping
    + table  jadpo_idempotency_keys    system table for idempotency keys
    + table  jadpo_operator_actions    system table for operator actions
    + table  jadpo_outbox              system table for outbox

Wrote database/migrations/20261020T1400_initial/
    snapshot.json  the whole schema at this point
    postgres.sql   15 statements: create table, add unique rule, add reference, create index

next: jadpo check  confirms the migration matches the entities
```

Commit the folder it writes, and never edit it; yours is named after
the time you ran it. The `jadpo_` tables are Jadpo's own bookkeeping,
kept in your database so that it commits or rolls back with your data.

## Add a record

Every entity gets `create`, `get`, `update` and `delete` without your
writing them. A create gives every field but `id` and the generated
ones in `set:`, and gives back the record as stored.

The unique ISBN means a create can be refused. The compiler knows which
rules each write can break, and makes you map each one to a failure in
`conflict:`, so the caller gets a clear `409` rather than a database
error:

`domains/books/failures/isbn_taken.jadpo`:

```jadpo check project=guide-store file=domains/books/failures/isbn_taken.jadpo
/// Another book already has this ISBN.
type IsbnTaken = Failure {
    kind: FailureKind.conflict
    message: "A book with that ISBN is already in the catalogue."
}
```

`domains/books/actions/add_book.jadpo`:

```jadpo check project=guide-store file=domains/books/actions/add_book.jadpo
use books/entities { Book }
use books/types { BookTitle }
use books/failures { IsbnTaken }

/// Adds a book to the catalogue.
action add_book(title: BookTitle, isbn: Book.isbn) fails IsbnTaken -> Book {
    return attempt Book.create {
        set: {
            title: title
            isbn: isbn
            on_loan: false
        }
        conflict: {
            Book.isbn: IsbnTaken
        }
    }
}
```

`attempt` passes the failure on to whoever called the action, which
`fails IsbnTaken` declares. [Handle failures](04-handle-failures.md)
says more.

## Change a record

To change only the fields a caller sent, give `update` a patch: an
`Input` whose fields are all optional. A field the caller left out
keeps its value.

`domains/books/types/book_changes.jadpo`:

```jadpo check project=guide-store file=domains/books/types/book_changes.jadpo
use books/entities { Book }

/// What a caller may change about a book: any of these, or none.
type BookChanges = Input {
    pick: [Book.title, Book.on_loan]
    optional: all
}
```

`domains/books/actions/change_book.jadpo`:

```jadpo check project=guide-store file=domains/books/actions/change_book.jadpo
use books/entities { Book, BookNotFound }
use books/types { BookChanges }

/// Changes the fields of a book that the caller sent.
action change_book(book_id: Book.id, changes: BookChanges) fails BookNotFound -> Book {
    var book: Book = attempt Book.get(book_id)
    return Book.update(book) {
        patch: changes
    }
}
```

`Book.get` reads one record or fails with `BookNotFound`, the failure
Jadpo declares for every entity; a route turns it into a `404`. To set
fields from code rather than from a request, give `set:` in place of
`patch:`.

`jadpo sql` shows the SQL an action sends:

```sh
jadpo sql change_book
```

```text
error: The entities have changed since the latest migration
 --> domains/books/entities/book.jadpo:4:8
  |
4 | entity Book {
  |        ^^^^ changed
  reason: `jadpo check` compares the database schema the entities ask for with the latest migration's snapshot (`database/migrations/20261020T1500_add_author/snapshot.json`): field `Book.author` removed.
  next: Run `jadpo migrate new retire_author` (agent)
  rule: migration.out_of_date
  help: https://jadpo.dev/docs/errors/migration.out_of_date

sql not shown: 1 error
```

## Delete a record

`domains/books/actions/remove_book.jadpo`:

```jadpo check project=guide-store file=domains/books/actions/remove_book.jadpo
use books/entities { Book, BookNotFound }

/// Takes a book out of the catalogue.
action remove_book(book_id: Book.id) fails BookNotFound -> Unit {
    var book: Book = attempt Book.get(book_id)
    Book.delete(book)
}
```

## All or nothing

Every action is all or nothing: its writes commit together, or none of
them do. Two people changing the same book at once don't overwrite each
other's changes either. A write to a record the action loaded checks
that nobody changed it in between, and if someone did, Jadpo runs the
whole action again with fresh reads. You write nothing for either.

## Change the entity later

Each time you add, change or remove a field, `jadpo check` fails until
you write the next migration, named for what it does. Here an optional
`author: Text?` field was added at the end of `Book`:

```sh
jadpo migrate new add_author
```

```text
  compared the entities with 20261020T1400_initial

  Book
    + author  text  new field

Wrote database/migrations/20261020T1500_add_author/
    snapshot.json  the whole schema at this point
    postgres.sql   1 statement: add column

next: jadpo check  confirms the migration matches the entities
```

## Read more

[Entities and the id](../language/05-data.md#entities-and-the-id),
[Fields and their settings](../language/05-data.md#fields-and-their-settings),
[Unique rules](../language/05-data.md#unique-rules),
[Migrations](../language/05-data.md#migrations),
[The operations every entity gets](../language/06-reads-and-writes.md#the-operations-every-entity-gets),
[When a write is refused](../language/06-reads-and-writes.md#when-a-write-is-refused) and
[All-or-nothing actions and concurrent changes](../language/06-reads-and-writes.md#all-or-nothing-actions-and-concurrent-changes).
To read records back, see [Query records](03-query-records.md).
