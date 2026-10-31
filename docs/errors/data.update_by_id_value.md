# `data.update_by_id_value`

An update by `id` sets a value that the database can't work out on its own, such as one read earlier in the action.

## Why Jadpo refuses it

An update by `id` is one statement, worked out by the database at the
moment it writes. Its values may use only the action's parameters,
constants, `clock.now` and the record's stored fields, and a stored
field only with `+` and `-`.

Here `hits` comes from `counter`, a copy loaded earlier. Another request
could add a hit between the read and the write, and this update would
then overwrite it, so a hit would be lost.

## What it looks like

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the new value depends only on the stored one**, name the stored
field, `Counter.hits`, and drop the read:

```jadpo fragment
action hit(counter_id: Counter.id) fails CounterNotFound -> Counter {
    return attempt Counter.update(counter_id) {
        set: {
            hits: Count(Counter.hits + 1)
        }
    }
}
```

```jadpo check project=e-update-by-id-value-fix-1 hidden
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

The database adds 1 to the stored count in one statement, so no hit is
lost. The action loads no record, so it runs at read committed, and a
second request counting at the same time waits for the first and then
adds to its count.

**If the action needs the loaded record anyway**, update that record
rather than its `id`:

```jadpo fragment
    var counter: Counter = attempt Counter.get(counter_id)
    return Counter.update(counter) {
        set: {
            hits: Count(counter.hits + 1)
        }
    }
```

```jadpo check project=e-update-by-id-value-fix-2 hidden
type Count = Int {
    min: 0
}

entity Counter {
    id: Uuid
    hits: Count
}

action hit(counter_id: Counter.id) fails CounterNotFound -> Counter {
    var counter: Counter = attempt Counter.get(counter_id)
    return Counter.update(counter) {
        set: {
            hits: Count(counter.hits + 1)
        }
    }
}
```

The action now runs at repeatable read, because it loads the counter. A
request that changes the same counter at the same time makes it run
again, so the hit still isn't lost.

## Related

- [Counters and other one-step updates](../language/06-reads-and-writes.md#counters-and-other-one-step-updates)
- [Which isolation level an action runs at](../language/06-reads-and-writes.md#which-isolation-level-an-action-runs-at)
- [The operations every entity gets](../language/06-reads-and-writes.md#the-operations-every-entity-gets)
