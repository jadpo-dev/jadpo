# `query.value_unsupported`

```jadpo check project=e-value-unsupported expect=query.value_unsupported
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type PageCount = Int {
    min: 1
    max: 5000
}

entity Manual {
    id: Uuid
    page_count: PageCount
}

query longer_manuals(manual_pages: Manual.page_count) -> List<Manual> {
    from: Manual
    where: Manual.page_count > manual_pages * 2
    order: [Manual.page_count asc]
    limit: 20
}
```

```text output
error: `manual_pages * 2` is worked out, and a query computes nothing
 --> main.jadpo:24:32
   |
24 |     where: Manual.page_count > manual_pages * 2
   |                                ^^^^^^^^^^^^^^^^ computed
  reason: A query's values are its parameters and their fields, literals, constants, enum values, lists of those and `clock.now`; anything worked out from them is computed before the call.
  next: Work the value out in the action, and pass it as a parameter (agent)
  rule: query.value_unsupported
  help: https://jadpo.dev/docs/errors/query.value_unsupported

check failed: 1 error, 1 file
```

A constant counts as a literal here, so this is accepted. Arithmetic on a constant inside the query is
still worked out, and still this error: give the result a constant of its
own.

```jadpo check project=query-constant
type PageCount = Int {
    min: 1
    max: 5000
}

entity Manual {
    id: Uuid
    page_count: PageCount
}

constant longest_manual: PageCount = 900

query long_manuals() -> List<Manual> {
    from: Manual
    where: Manual.page_count > longest_manual
    order: [Manual.page_count asc]
    limit: 20
}
```
