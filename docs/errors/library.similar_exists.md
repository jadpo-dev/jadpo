# `library.similar_exists`

A declinable error: call the library function, or decline with a reason
when the two differ on purpose ([Declining a judgement call](../language/01-project-layout-and-modules.md#declining-a-judgement-call)).
A function is like a library function
when its parameter types, result, `fails` list and body are the same once
each parameter and variable is renamed by its position: a copy, however it
is formatted or named. Of two alike library functions, the later one in
file order is reported.

```jadpo check project=e-similar expect=library.similar_exists
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

function cap_total(total: Stock) -> Stock {
    if (total > Stock(1000)) {
        return Stock(1000)
    }
    return total
}
```

```text output
error: A similar library function exists: `rounding.capped`
 --> main.jadpo:15:10
   |
15 | function cap_total(total: Stock) -> Stock {
   |          ^^^^^^^^^ same as `rounding.capped`
  reason: `cap_total` takes the same parameters, gives the same result and has the same body as `rounding.capped` in `main.jadpo`, apart from the names of its variables.
  next: Call `rounding.capped` instead, and remove `cap_total`, or write `decline library.similar_exists: "<why>"` above it if this is deliberate (agent)
  rule: library.similar_exists
  help: https://jadpo.dev/docs/errors/library.similar_exists

check failed: 1 error, 1 file
```
