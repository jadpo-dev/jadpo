# `query.aggregate_unknown`

```jadpo check project=e-aggregate-unknown expect=query.aggregate_unknown
type LeafCount = Int {
    min: 1
}

entity Book {
    id: Uuid
    leaves: LeafCount
}

type ShelfLength = Output {
    mean_leaves: Decimal?
}

query shelf_length() -> ShelfLength {
    from: Book
    aggregate: {
        mean_leaves: avg(Book.leaves, scale: 1, rounding: Rounding.half_up)
    }
}
```

```text output
error: `avg` isn't an aggregate
 --> main.jadpo:17:22
   |
17 |         mean_leaves: avg(Book.leaves, scale: 1, rounding: Rounding.half_up)
   |                      ^^^ not an aggregate
  reason: An aggregate is `count()`, `sum(F)`, `min(F)`, `max(F)` or `average(F, scale:, rounding:)`, each `F` a field of `Book`. Did you mean `average`?
  next: Use `average` (agent)
  rule: query.aggregate_unknown
  help: https://jadpo.dev/docs/errors/query.aggregate_unknown

check failed: 1 error, 1 file
```
