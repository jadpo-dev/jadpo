# `query.page_setting`

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
  help: docs/errors/query.page_setting.md

check failed: 1 error, 1 file
```
