# `data.stored_value_outside_update`

`Counter.hits` is the value stored when an update by `id` runs, so only
that update reads it:

```jadpo check project=e-stored-value-outside-update expect=data.stored_value_outside_update
type Count = Int {
    min: 0
}

entity Counter {
    id: Uuid
    hits: Count
}

action hit(counter: Counter) -> Counter {
    return Counter.update(counter) {
        set: {
            hits: Count(Counter.hits + 1)
        }
    }
}
```

```text output
error: `Counter.hits` is a stored value, read only by an update by `id`
 --> main.jadpo:13:25
   |
13 |             hits: Count(Counter.hits + 1)
   |                         ^^^^^^^^^^^^ stored value
  reason: `Counter.hits` is the value stored in the database when an update by `id` runs, which only that update's `set:` reads; elsewhere a record's field is read from the record loaded earlier.
  next: Read it from a loaded record, `counter.hits`, or use it in `Counter.update(counter_id) { set: { ... } }` (agent)
  rule: data.stored_value_outside_update
  help: docs/language/06-reads-and-writes.html#data-stored-value-outside-update

check failed: 1 error, 1 file
```
