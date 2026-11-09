# `query.condition_unsupported`

A query's `where:` or `where_present:` line holds a condition the database can't serve.

## Why Jadpo refuses it

A query's conditions become one SQL statement, so each has to be one the
database can decide and an index can serve, and it has to give the same
answer Jadpo's own code would. A condition that needs every record read in
turn, or that PostgreSQL would answer differently from your code, is refused
rather than left to slow down or disagree later.

Some cases are about what each section holds: `where_present:` applies one
comparison only when its value is there, and a query ranks by one search.

## What it looks like

main.jadpo:

```jadpo check project=e-condition-unsupported expect=query.condition_unsupported
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query books_after(title: Book.title) -> List<Book> {
    from: Book
    where: Book.title > title
    order: [Book.title asc]
    limit: 20
}
```

```text output
error: `Book.title > title` isn't a condition a query can use
 --> main.jadpo:14:12
   |
14 |     where: Book.title > title
   |            ^^^^^^^^^^^^^^^^^^ not an indexable condition
  reason: `>` orders numbers, times, dates, durations and quantities, as it does in a body, and `Book.title` is `Title`.
  next: Compare `Book.title` with `==`, `!=` or `in` (agent)
  rule: query.condition_unsupported
  help: https://jadpo.dev/docs/errors/query.condition_unsupported

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A standard operation stands where a field does only when it has an exact SQL
form. `text.lower` has none, since PostgreSQL's would follow the database's
collation:

main.jadpo:

```jadpo check project=e-condition-no-form expect=query.condition_unsupported
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
}

query books_titled(title: Book.title) -> List<Book> {
    from: Book
    where: text.lower(Book.title) == title
    order: [Book.title asc]
    limit: 20
}
```

```text output
error: `text.lower(Book.title) == title` isn't a condition a query can use
 --> main.jadpo:12:12
   |
12 |     where: text.lower(Book.title) == title
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not an indexable condition
  reason: `text.lower` has no SQL form, an expression PostgreSQL works out exactly as the code does (area 19, "SQL forms"), so the database can't decide this condition.
  next: Compare `Book.title` itself, or keep what the condition needs in a field of its own (agent)
  rule: query.condition_unsupported
  help: https://jadpo.dev/docs/errors/query.condition_unsupported

check failed: 1 error, 1 file
```

Comparing two fields of a record can't be served by an index, since no index
holds one field against another:

main.jadpo:

```jadpo check project=e-condition-two-fields expect=query.condition_unsupported
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query same_title() -> List<Book> {
    from: Book
    where: Book.title == Book.subtitle
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `Book.subtitle` compares a field with a field
 --> main.jadpo:14:26
   |
14 |     where: Book.title == Book.subtitle
   |                          ^^^^^^^^^^^^^ a field, not a value
  reason: A condition compares a field with a value the call supplies; comparing two fields of a record can't be served by an index.
  next: Compare the field with a parameter, a literal or an enum value (agent)
  rule: query.condition_unsupported
  help: https://jadpo.dev/docs/errors/query.condition_unsupported

check failed: 1 error, 1 file
```

`where_present:` holds one comparison, the one it applies when the value is
there:

main.jadpo:

```jadpo check project=e-condition-present-comparison expect=query.condition_unsupported
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query available_book_list(title: Title?) -> List<Book> {
    from: Book
    where_present: Book.title == title and Book.available == true
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `where_present:` holds one comparison
 --> main.jadpo:14:20
   |
14 |     where_present: Book.title == title and Book.available == true
   |                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not one comparison
  reason: `where_present:` applies its condition only when the value on the right is there, which is how a request's optional filters are written: one field compared with one optional value, `<Entity>.<field> == filter.<field>`.
  next: Write one comparison, or move the condition to `where:` (agent)
  rule: query.condition_unsupported
  help: https://jadpo.dev/docs/errors/query.condition_unsupported

check failed: 1 error, 1 file
```

A `geo.` condition's value is always given, so it stands on a `where:` line:

main.jadpo:

```jadpo check project=e-condition-present-geo expect=query.condition_unsupported
entity Shop {
    id: Uuid
    spot: Point
}

query shops_in(area: Polygon?) -> List<Shop> {
    from: Shop
    where_present: geo.within(Shop.spot, area)
    order: [Shop.id asc]
    limit: 20
}
```

```text output
error: `where_present:` can't hold a `geo.` condition
 --> main.jadpo:8:20
  |
8 |     where_present: geo.within(Shop.spot, area)
  |                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^ a `geo.` condition
  reason: `where_present:` applies a field's comparison only when its value is there; a `geo.` condition's value is always given, in a `where:` line (area 20, "Queries").
  next: Write the condition in `where:` (agent)
  rule: query.condition_unsupported
  help: https://jadpo.dev/docs/errors/query.condition_unsupported

check failed: 1 error, 1 file
```

A distance is the same, since its vector and its bound are both always
given:

main.jadpo:

```jadpo check project=e-condition-present-distance expect=query.condition_unsupported
type ArticleMeaning = Vector {
    dimensions: 1536
}

entity HelpArticle {
    id: Uuid
    body_meaning: ArticleMeaning?
}

query close_articles(question: ArticleMeaning) -> List<HelpArticle> {
    from: HelpArticle
    where_present: ai.distance(HelpArticle.body_meaning, question) <= 0.6
    order: [HelpArticle.id asc]
    limit: 20
}
```

```text output
error: `where_present:` can't hold a distance
 --> main.jadpo:12:20
   |
12 |     where_present: ai.distance(HelpArticle.body_meaning, question) <= 0.6
   |                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ a distance
  reason: `where_present:` applies a field's comparison only when its value is there; a distance's vector and its bound are both always given, in a `where:` line.
  next: Write the distance in `where:` (agent)
  rule: query.condition_unsupported
  help: https://jadpo.dev/docs/errors/query.condition_unsupported

check failed: 1 error, 1 file
```

A search ranks the records it finds by how well they match one phrase, so a
query holds one `matches`:

main.jadpo:

```jadpo check project=e-condition-two-searches expect=query.condition_unsupported
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title

    search: {
        fields: [title]
        language: SearchLanguage.english
    }
}

query found(first: SearchText, second: SearchText) -> List<Book> {
    from: Book
    where: Book matches first
    where: Book matches second
    limit: 20
}
```

```text output
error: `found` searches by words twice
 --> main.jadpo:15:7
   |
15 | query found(first: SearchText, second: SearchText) -> List<Book> {
   |       ^^^^^ two `matches`
  reason: A search ranks the records it finds by how well they match one phrase, so a query holds one `matches`; one `SearchText` takes several words, quoted phrases and `-word`.
  next: Keep one `matches` line (agent)
  rule: query.condition_unsupported
  help: https://jadpo.dev/docs/errors/query.condition_unsupported

check failed: 1 error, 1 file
```

A search orders its records by how well they match, so it can't share a
query with a `nearest` order:

main.jadpo:

```jadpo check project=e-condition-search-nearest expect=query.condition_unsupported
type Title = Text {
    max_length: 80
}

type BookMeaning = Vector {
    dimensions: 1536
}

entity Book {
    id: Uuid
    title: Title
    meaning: BookMeaning?

    search: {
        fields: [title]
        language: SearchLanguage.english
    }
}

query found(phrase: SearchText, meaning: BookMeaning) -> List<Nearest<Book>> {
    from: Book
    where: Book matches phrase
    order: [Book.meaning nearest meaning]
    limit: 20
}
```

```text output
error: `found` searches by words and orders by nearness
 --> main.jadpo:20:7
   |
20 | query found(phrase: SearchText, meaning: BookMeaning) -> List<Nearest<Book>> {
   |       ^^^^^ `matches` and a nearest order
  reason: A search orders its records by how well they match, or by the fields `order:` lists; a `nearest` or `geo.distance` order, or a map layer, orders them by place, and the two can't be combined.
  next: Search in a query of its own (agent)
  rule: query.condition_unsupported
  help: https://jadpo.dev/docs/errors/query.condition_unsupported

check failed: 1 error, 1 file
```

## How to fix it

**If the condition can compare for equality**, use `==`, `!=` or `in`:

```jadpo fragment
    where: Book.title == title
```

```jadpo check project=e-condition-fix-1 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query books_titled(title: Book.title) -> List<Book> {
    from: Book
    where: Book.title == title
    order: [Book.title asc]
    limit: 20
}
```

An index on `title` serves the query.

**If you need to compare a worked-out value**, keep it in a field of its own, written by the action that saves the record:

```jadpo fragment
    lower_title: Title
```

```jadpo check project=e-condition-fix-2 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    lower_title: Title
}

query books_titled(title: Book.lower_title) -> List<Book> {
    from: Book
    where: Book.lower_title == title
    order: [Book.title asc]
    limit: 20
}
```

The action that saves a book works out the lower-case title with `text.lower`, and the query compares the stored field.

**If the condition belongs on a `where:` line**, move it there:

```jadpo fragment
    where: Book.available == true
    where_present: Book.title == title
```

```jadpo check project=e-condition-fix-3 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query available_book_list(title: Title?) -> List<Book> {
    from: Book
    where: Book.available == true
    where_present: Book.title == title
    order: [Book.id asc]
    limit: 20
}
```

The `where:` line always applies, and `where_present:` applies only when `title` is given. A `geo.` condition or a distance moves to a `where:` line the same way.

**If a query searches twice, or searches and orders by nearness**, keep one `matches` line, which orders the records best first:

```jadpo fragment
    where: Book matches phrase
    limit: 20
```

```jadpo check project=e-condition-fix-4 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title

    search: {
        fields: [title]
        language: SearchLanguage.english
    }
}

query found(phrase: SearchText) -> List<Book> {
    from: Book
    where: Book matches phrase
    limit: 20
}
```

One `SearchText` takes several words, quoted phrases and `-word`. A nearest-neighbour search goes in a query of its own.

## Related

- [Filters](../language/06-reads-and-writes.md#filters)
- [Operations in conditions](../language/06-reads-and-writes.md#operations-in-conditions)
- [`query.optional_value`](query.optional_value.md)
- [Search](../language/16-built-in-services.md#search)
