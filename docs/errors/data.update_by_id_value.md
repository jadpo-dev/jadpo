# `data.update_by_id_value`

An update by `id` is worked out by the database in one step, so its values
use only parameters, constants, `clock.now` and the record's stored fields,
and a stored field only with `+` and `-`:

```jadpo check project=e-update-by-id-value expect=data.update_by_id_value
type Count = Int {
    min: 0
}

entity Counter {
    id: Uuid
    hits: Count
}

action hit(counter_id: Counter.id) fails CounterNotFound -> Counter {
    var counter: Counter = attempt Counter.get(counter_id)
    return attempt Counter.update(counter_id) {
        set: {
            hits: Count(counter.hits + 1)
        }
    }
}
```

```text output
error: `Counter.update` by `id` sets a value from `counter`, a variable
 --> main.jadpo:14:25
   |
14 |             hits: Count(counter.hits + 1)
   |                         ^^^^^^^ a variable
  reason: An update by `id` is worked out by the database in one step, so its values use only the action's parameters, constants, `clock.now` and `Counter`'s stored fields; a value read earlier could have changed by the time it writes.
  next: Load the record and update it, `Counter.update(counter) { set: { ... } }`, or pass the value in as a parameter (agent)
  rule: data.update_by_id_value
  help: https://jadpo.dev/docs/errors/data.update_by_id_value

check failed: 1 error, 1 file
```
