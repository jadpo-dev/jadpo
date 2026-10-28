# `effect.function_persistence`

```jadpo check project=e-fn-persist expect=effect.function_persistence
entity Customer {
    id: Uuid
}

function find_customer(id: Customer.id) -> Customer {
    return attempt Customer.get(id)
}
```

```text output
error: A function can't call `Customer.get`
 --> main.jadpo:6:20
  |
6 |     return attempt Customer.get(id)
  |                    ^^^^^^^^^^^^ reads or writes stored data
  reason: A function is pure: it computes its result from its arguments alone, so reading and writing entities belongs in an action.
  next: Make the caller an action, or pass the data in as an argument (agent)
  rule: effect.function_persistence
  help: docs/language/03-logic.html#effect-function-persistence

check failed: 1 error, 1 file
```
