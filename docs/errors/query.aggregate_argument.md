# `query.aggregate_argument`

```jadpo check project=e-aggregate-argument expect=query.aggregate_argument
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
        mean_leaves: average(Book.leaves)
    }
}
```

```text output
error: `average` needs `scale:` and `rounding:`
 --> main.jadpo:17:22
   |
17 |         mean_leaves: average(Book.leaves)
   |                      ^^^^^^^^^^^^^^^^^^^^ wrong arguments
  reason: An average rarely comes out exact, so it says how many places it keeps and how the last one is rounded, both written out: `average(Book.<field>, scale: 2, rounding: Rounding.half_even)`.
  next: Add `scale:` and `rounding:` (agent)
  rule: query.aggregate_argument
  help: docs/errors/query.aggregate_argument.md

check failed: 1 error, 1 file
```
