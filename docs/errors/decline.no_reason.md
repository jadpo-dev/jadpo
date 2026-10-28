# `decline.no_reason`

A decline says why the finding is right to leave, so a reviewer can agree
or not.

```jadpo check project=e-decline-reason expect=decline.no_reason
type Stock = Int {
    min: 0
}

library rounding {
    /// The amount, capped at a thousand.
    function capped(amount: Stock) -> Stock {
        if (amount > Stock(1000)) {
            return Stock(1000)
        }
        return amount
    }
}

decline library.similar_exists: ""
function cap_total(total: Stock) -> Stock {
    if (total > Stock(1000)) {
        return Stock(1000)
    }
    return total
}
```

```text output
error: This decline gives no reason
 --> main.jadpo:15:1
   |
15 | decline library.similar_exists: ""
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ reason needed
  reason: A decline is reviewed like the code it sits on: its reason says why the finding is right to leave, so a reviewer can agree or not.
  next: Write why in the quotes: `decline library.similar_exists: "<why>"` (agent)
  rule: decline.no_reason
  help: docs/errors/decline.no_reason.md

check failed: 1 error, 1 file
```
