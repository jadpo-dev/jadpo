# `query.optional_value`

A query uses a value that may be absent where it needs one, or `where_present:` with one that is always there.

## Why Jadpo refuses it

A `where:` line applies to every call, so the value it compares with has to
be there every time. `where_present:` is the other way round: it applies
only when its value is there, which is how a request's optional filters are
written.

An absent value compared with `==` would match nothing rather than leave the
filter off, and that is rarely what a caller meant, so Jadpo asks you to say
which you want.

## What it looks like

main.jadpo:

```jadpo check project=e-optional-value expect=query.optional_value
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type FindBooks = Input {
    title: Title {
        optional: true
    }
}

query find_books(filter: FindBooks) -> List<Book> {
    from: Book
    where: Book.title == filter.title
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `filter.title` may be absent, so this `where:` line can't always be met
 --> main.jadpo:20:26
   |
20 |     where: Book.title == filter.title
   |                          ^^^^^^^^^^^^ may be `none`
  reason: A `where:` line applies to every call; a condition that should apply only when its value is there, as a request's optional filter, is written with `where_present:`.
  next: Write `where_present:` for this condition (agent)
  rule: query.optional_value
  help: https://jadpo.dev/docs/errors/query.optional_value

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`where_present:` with a value that is always there would always apply, so it
is a `where:` line:

main.jadpo:

```jadpo check project=e-optional-value-present expect=query.optional_value
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
    where_present: Book.title == title
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `title` is always there, so `where_present:` would always apply
 --> main.jadpo:14:34
   |
14 |     where_present: Book.title == title
   |                                  ^^^^^ never absent
  reason: `where_present:` is for a value that may be absent, such as an optional field of a request's `Input`; a condition that always applies is a `where:` line.
  next: Write `where:` instead (agent)
  rule: query.optional_value
  help: https://jadpo.dev/docs/errors/query.optional_value

check failed: 1 error, 1 file
```

An item of a list a field is tested `in` is a value, so it has to be there
too:

main.jadpo:

```jadpo check project=e-optional-value-list expect=query.optional_value
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query books_titled(first: Title, second: Title?) -> List<Book> {
    from: Book
    where: Book.title in [first, second]
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `second` may be absent, and a list's items can't be
 --> main.jadpo:14:34
   |
14 |     where: Book.title in [first, second]
   |                                  ^^^^^^ may be `none`
  reason: Each item of a list a field is tested `in` is a value; an absent one would match nothing.
  next: Use a value that is always there (agent)
  rule: query.optional_value
  help: https://jadpo.dev/docs/errors/query.optional_value

check failed: 1 error, 1 file
```

A `limit:` or a page `size:` bounds every call, so it is always there too:

main.jadpo:

```jadpo check project=e-optional-value-limit expect=query.optional_value
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf(count: PageSize?) -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: count
}
```

```text output
error: `count` may be absent, and `limit:` needs a value on every call
 --> main.jadpo:15:12
   |
15 |     limit: count
   |            ^^^^^ may be `none`
  reason: Every list is bounded on every call, so what bounds it is always there; `where_present:` is for a filter a call may leave out.
  next: Pass a value that is always there (agent)
  rule: query.optional_value
  help: https://jadpo.dev/docs/errors/query.optional_value

check failed: 1 error, 1 file
```

`nearest` measures from a vector that is always there:

main.jadpo:

```jadpo check project=e-optional-value-nearest expect=query.optional_value
type ArticleMeaning = Vector {
    dimensions: 1536
}

entity HelpArticle {
    id: Uuid
    body_meaning: ArticleMeaning?
}

query closest_articles(question: ArticleMeaning?) -> List<Nearest<HelpArticle>> {
    from: HelpArticle
    order: [HelpArticle.body_meaning nearest question]
    limit: 5
}
```

```text output
error: `question` may be absent, and `nearest` needs a vector to measure from
 --> main.jadpo:12:46
   |
12 |     order: [HelpArticle.body_meaning nearest question]
   |                                              ^^^^^^^^ may be `none`
  reason: `nearest` orders the records by their distance from a vector the call supplies, so that vector is there on every call; `where_present:` is for a filter a call may leave out.
  next: Pass a vector that is always there (agent)
  rule: query.optional_value
  help: https://jadpo.dev/docs/errors/query.optional_value

check failed: 1 error, 1 file
```

## How to fix it

**If the filter is optional**, write `where_present:`:

```jadpo fragment
    where_present: Book.title == filter.title
```

```jadpo check project=e-optional-value-fix-1 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type FindBooks = Input {
    title: Title {
        optional: true
    }
}

query find_books(filter: FindBooks) -> List<Book> {
    from: Book
    where_present: Book.title == filter.title
    order: [Book.id asc]
    limit: 20
}
```

A caller who leaves `title` out gets every book, and one who gives it gets those books alone.

**If the value is always there**, write `where:`:

```jadpo fragment
    where: Book.title == title
```

```jadpo check project=e-optional-value-fix-2 hidden
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
    order: [Book.id asc]
    limit: 20
}
```

The condition now always applies. For a `limit:`, a page `size:`, a list or a `nearest` vector, pass a value that is always there.

## Related

- [Filters](../language/06-reads-and-writes.md#filters)
- [`query.condition_unsupported`](query.condition_unsupported.md)
