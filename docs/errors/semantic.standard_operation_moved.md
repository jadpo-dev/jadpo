# `semantic.standard_operation_moved`

An operation the standard library has moved to another name: its edit
writes the new name, and the call is checked as the new operation.
`temporal.format` has moved to the time types' `text.format`:

```jadpo check project=e-operation-moved expect=semantic.standard_operation_moved
type OrderedAt = Instant {}

function placed(at: OrderedAt) -> PresentationText {
    return temporal.format(
        at,
        zone: Zone.europe_london,
        locale: Locale.en_gb,
        format: TimeFormat.date_long
    )
}
```

```text output
error: `temporal.format` is now `text.format`
 --> main.jadpo:4:12
  |
4 |     return temporal.format(
  |            ^^^^^^^^^^^^^^^ moved
  reason: The standard library has one name for each operation; `temporal.format` moved to `text.format`, which takes the same values.
  next: Write `text.format` (agent)
  rule: semantic.standard_operation_moved
  help: https://jadpo.dev/docs/errors/semantic.standard_operation_moved

check failed: 1 error, 1 file
```

`collection.` has moved to `list.`, `set.` and `map.`: a call
through it is reported with an edit to the namespace for its first value,
a list unless it is a set or a map, and checked as that operation where
it is built.

```jadpo check project=e-collection-moved expect=semantic.standard_operation_moved
type BatchName = Text {
    min_length: 1
}

type BatchCount = Int {
    min: 0
}

function batches_baked(bakings: List<BatchName>) -> BatchCount {
    return BatchCount(collection.count(bakings))
}
```

```text output
error: `collection.count` is now `list.count`
 --> main.jadpo:10:23
   |
10 |     return BatchCount(collection.count(bakings))
   |                       ^^^^^^^^^^^^^^^^ moved
  reason: The standard library has one namespace for each type an operation works on, so `collection.` became `list.`, `set.` and `map.`; `bakings` is a list.
  next: Write `list.count` (agent)
  rule: semantic.standard_operation_moved
  help: https://jadpo.dev/docs/errors/semantic.standard_operation_moved

check failed: 1 error, 1 file
```
