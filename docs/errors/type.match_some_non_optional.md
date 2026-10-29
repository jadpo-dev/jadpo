# `type.match_some_non_optional`

```jadpo check project=e-match-some file=shapes.jadpo hidden
type DeliveryState = Enum {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo check project=e-match-some expect=type.match_some_non_optional
function describe(label: Label) -> Label {
    return match label {
        some(name) -> name
        _ -> label
    }
}
```

```text output
error: `label` is never `none`, so `some(...)` can't match it
 --> main.jadpo:3:9
  |
3 |         some(name) -> name
  |         ^^^^^^^^^^ for nullable values
  reason: `some(name)` and `none` match a nullable value, written `T?`; `label` is a `Label`.
  next: Use `label` directly: it always has a value (agent)
  rule: type.match_some_non_optional
  help: docs/errors/type.match_some_non_optional.md

check failed: 1 error, 2 files
```
