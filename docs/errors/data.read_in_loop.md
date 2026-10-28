# `data.read_in_loop`

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
  help: docs/language/06-reads-and-writes.html#data-read-in-loop

check failed: 1 error, 1 file
```
