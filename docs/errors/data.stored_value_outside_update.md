# `data.stored_value_outside_update`

A stored field, written `Entity.field`, is read anywhere but an update by `id` of that record.

## Why Jadpo refuses it

`Counter.hits` means the value in the database at the moment an update by
`id` runs, which the database works out in one step. Anywhere else there
is no such moment: a record's field is read from the copy loaded earlier,
as `counter.hits`.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/data.stored_value_outside_update

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An update by `id` of one record reads only that record's stored fields:

```jadpo check project=e-stored-value-other expect=data.stored_value_outside_update
type Count = Int {
    min: 0
}

entity Counter {
    id: Uuid
    hits: Count
}

entity Tally {
    id: Uuid
    total: Count
}

action hit(tally_id: Tally.id) fails TallyNotFound -> Tally {
    return attempt Tally.update(tally_id) {
        set: {
            total: Count(Counter.hits + 1)
        }
    }
}
```

```text output
error: `Counter.hits` is a stored value, read only by an update by `id`
 --> main.jadpo:18:26
   |
18 |             total: Count(Counter.hits + 1)
   |                          ^^^^^^^^^^^^ stored value
  reason: Only the record being updated is read as it is stored: this update by `id` writes `Tally`, not `Counter`.
  next: Read it from a loaded record, `counter.hits`, or use it in `Counter.update(counter_id) { set: { ... } }` (agent)
  rule: data.stored_value_outside_update
  help: https://jadpo.dev/docs/errors/data.stored_value_outside_update

check failed: 1 error, 1 file
```

## How to fix it

**If nothing else needs the record**, update it by its `id`, and let the
database work the value out:

```jadpo fragment
action hit(counter_id: Counter.id) fails CounterNotFound -> Counter {
    return attempt Counter.update(counter_id) {
        set: {
            hits: Count(Counter.hits + 1)
        }
    }
}
```

```jadpo check project=e-stored-value-fix-1 hidden
type Count = Int {
    min: 0
}

entity Counter {
    id: Uuid
    hits: Count
}

action hit(counter_id: Counter.id) fails CounterNotFound -> Counter {
    return attempt Counter.update(counter_id) {
        set: {
            hits: Count(Counter.hits + 1)
        }
    }
}
```

Two calls at the same moment now each add one, and no count is lost.

**If you want the value you loaded**, read it from the record:

```jadpo fragment
action hit(counter: Counter) -> Counter {
    return Counter.update(counter) {
        set: {
            hits: Count(counter.hits + 1)
        }
    }
}
```

```jadpo check project=e-stored-value-fix-2 hidden
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
            hits: Count(counter.hits + 1)
        }
    }
}
```

The new count now comes from the copy the action loaded.

## Related

- [Counters and other one-step updates](../language/06-reads-and-writes.md#counters-and-other-one-step-updates)
- [The operations every entity gets](../language/06-reads-and-writes.md#the-operations-every-entity-gets)
