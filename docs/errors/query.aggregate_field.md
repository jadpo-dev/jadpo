# `query.aggregate_field`

```jadpo check project=e-aggregate-field expect=query.aggregate_field
type Fine = Money {}

entity Loan {
    id: Uuid
    fine: Fine
}

type FinesOwed = Output {
    owed: Money
}

query fines_owed() -> FinesOwed {
    from: Loan
    aggregate: {
        owed: sum(Loan.fine)
    }
}
```

```text output
error: `sum` adds up money in one currency, and `Loan.fine`'s type doesn't fix one
 --> main.jadpo:15:19
   |
15 |         owed: sum(Loan.fine)
   |                   ^^^^^^^^^ `Fine`
  reason: Amounts in two currencies don't add up (`currency_mismatch`), and a sum of no records needs a currency for its zero, so the field's type fixes its currency: `Fine` doesn't.
  next: Give `Fine` a `currency:` (agent)
  rule: query.aggregate_field
  help: https://jadpo.dev/docs/errors/query.aggregate_field

check failed: 1 error, 1 file
```
