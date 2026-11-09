# `query.page_setting`

A query's `page:` isn't a block holding a `size:` and a `PageCursor` in `after:`.

## Why Jadpo refuses it

A page has a `size:` and a cursor to continue `after:`, and nothing else.
There is no offset, because a large offset is slow and it skips or repeats
records when rows are added. `after:` is the cursor a previous page gave in
its `next`, so it is a `PageCursor`, absent on the first call.

## What it looks like

main.jadpo:

```jadpo check project=e-page-setting expect=query.page_setting
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf() -> Page<Book> {
    from: Book
    order: [Book.title asc]
    page: {
        size: 20
    }
}
```

```text output
error: `page:` has no `after:`
 --> main.jadpo:15:5
   |
15 |     page: {
   |     ^^^^ no `after:`
  reason: A page continues `after:` the cursor the previous page gave in its `next`; on the first call that cursor is absent.
  next: Add `after: ...` to the block (agent)
  rule: query.page_setting
  help: https://jadpo.dev/docs/errors/query.page_setting

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A page has a `size:` too, the most records it holds:

main.jadpo:

```jadpo check project=e-page-setting-size expect=query.page_setting
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf(after: PageCursor?) -> Page<Book> {
    from: Book
    order: [Book.title asc]
    page: {
        after: after
    }
}
```

```text output
error: `page:` has no `size:`
 --> main.jadpo:15:5
   |
15 |     page: {
   |     ^^^^ no `size:`
  reason: A page's `size:` is how many records it holds: a number, or a value whose type has a `max:`, such as `PageSize`.
  next: Add `size: ...` to the block (agent)
  rule: query.page_setting
  help: https://jadpo.dev/docs/errors/query.page_setting

check failed: 1 error, 1 file
```

Anything else isn't a page setting. An offset is refused for the reason
above:

main.jadpo:

```jadpo check project=e-page-setting-unknown expect=query.page_setting
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf(after: PageCursor?) -> Page<Book> {
    from: Book
    order: [Book.title asc]
    page: {
        size: 20
        after: after
        offset: 40
    }
}
```

```text output
error: `offset` isn't a page setting
 --> main.jadpo:18:9
   |
18 |         offset: 40
   |         ^^^^^^ not a setting
  reason: A page has a `size:` and an `after:` cursor; there is no offset, because a large offset is slow and skips or repeats records when rows are added.
  next: Remove `offset:` (agent)
  rule: query.page_setting
  help: https://jadpo.dev/docs/errors/query.page_setting

check failed: 1 error, 1 file
```

`after:` is a `PageCursor`, not a value of another type:

main.jadpo:

```jadpo check project=e-page-setting-after expect=query.page_setting
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf(after: Title?) -> Page<Book> {
    from: Book
    order: [Book.title asc]
    page: {
        size: 20
        after: after
    }
}
```

```text output
error: `after:` takes a `PageCursor`, and `after` is `Title?`
 --> main.jadpo:17:16
   |
17 |         after: after
   |                ^^^^^ a `Title?`
  reason: `after:` is the cursor a previous page gave in its `next`, which a client sends back: a `PageCursor`, usually an optional field of the request's `Input`.
  next: Pass a `PageCursor` here (agent)
  rule: query.page_setting
  help: https://jadpo.dev/docs/errors/query.page_setting

check failed: 1 error, 1 file
```

`page:` is always a block:

main.jadpo:

```jadpo check project=e-page-setting-block expect=query.page_setting
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf() -> Page<Book> {
    from: Book
    order: [Book.title asc]
    page: 20
}
```

```text output
error: `page:` is a block
 --> main.jadpo:15:5
   |
15 |     page: 20
   |     ^^^^ not a block
  reason: `page:` gives the page's `size:` and the cursor to continue `after:` in a block.
  next: Write `page: { size: ... after: ... }` (agent)
  rule: query.page_setting
  help: https://jadpo.dev/docs/errors/query.page_setting

check failed: 1 error, 1 file
```

## How to fix it

**If a setting is missing or wrong**, write both, with `after:` a `PageCursor` parameter:

```jadpo fragment
query shelf(after: PageCursor?) -> Page<Book> {
    ...
    page: {
        size: 20
        after: after
    }
}
```

```jadpo check project=e-page-setting-fix-1 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf(after: PageCursor?) -> Page<Book> {
    from: Book
    order: [Book.title asc]
    page: {
        size: 20
        after: after
    }
}
```

A caller gets the first twenty books with no cursor, then passes each page's `next` to get the page after it.

## Related

- [Pages](../language/06-reads-and-writes.md#pages)
- [Order and pages](../language/06-reads-and-writes.md#order-and-pages)
