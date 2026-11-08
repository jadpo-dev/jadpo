# `data.read_in_loop`

A query or another database read, such as a `get`, is inside a `for` loop.

## Why Jadpo refuses it

A read inside a loop makes one database query for each item, so the action
gets slower with every item in the list and nothing warns you. One query
can read the whole set at once, by testing a field with `in` a list of
values.

## What it looks like

main.jadpo:

```jadpo check project=e-read-in-loop expect=data.read_in_loop
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type BookNotFound = Failure {
    kind: FailureKind.not_found
}

type Count = Int {
    min: 0
}

action available_count(book_ids: List<Book.id>) fails BookNotFound -> Count {
    var mut count: Count = 0
    for book_id in book_ids {
        var book: Book = attempt Book.get(book_id)
        if (book.available) {
            count = Count(count + 1)
        }
    }
    return count
}
```

```text output
error: `Book.get` reads once for every item of the loop
 --> main.jadpo:23:34
   |
23 |         var book: Book = attempt Book.get(book_id)
   |                                  ^^^^^^^^ inside `for`
  reason: A read inside a loop makes one database query per item. Read the whole set with one query, testing the field with `in` a list of the values, before the loop.
  next: Read the records with one query using `in` before the loop (agent)
  rule: data.read_in_loop
  help: https://jadpo.dev/docs/errors/data.read_in_loop

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you are reading several records by their `id`**, read them with one
query before the loop:

```jadpo fragment
query books_by_ids(book_ids: List<Book.id>) -> List<BookView> {
    from: Book
    where: Book.id in book_ids
    order: [Book.title asc]
    limit: 100
}

action available_count(book_ids: List<Book.id>) -> Count {
    var mut count: Count = 0
    for book in books_by_ids(book_ids) {
        if (book.available) {
            count = Count(count + 1)
        }
    }
    return count
}
```

```jadpo check project=e-read-in-loop-fix hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type BookView = Output {
    pick: [Book.id, Book.available]
}

type Count = Int {
    min: 0
}

query books_by_ids(book_ids: List<Book.id>) -> List<BookView> {
    from: Book
    where: Book.id in book_ids
    order: [Book.title asc]
    limit: 100
}

action available_count(book_ids: List<Book.id>) -> Count {
    var mut count: Count = 0
    for book in books_by_ids(book_ids) {
        if (book.available) {
            count = Count(count + 1)
        }
    }
    return count
}
```

The action now reads the database once, however many books it is given.

## Related

- [Reading in a loop](../language/06-reads-and-writes.md#reading-in-a-loop)
