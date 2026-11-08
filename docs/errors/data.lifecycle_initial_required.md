# `data.lifecycle_initial_required`

A required field that a lifecycle's transition sets has no starting value in `initial:`.

## Why Jadpo refuses it

Only a lifecycle's transitions may change a field they set, so a create
can't give it. A required field needs a value from the moment its record
exists, so something else has to supply the first one, and that is
`initial:`.

## What it looks like

main.jadpo:

```jadpo check project=e-lifecycle-initial-required expect=data.lifecycle_initial_required
type OrderStage = Enum {
    placed
    shipped
}

entity Order {
    id: Uuid
    stage: OrderStage

    lifecycle: {
        transitions: {
            ship: {
                when: stage == OrderStage.placed
                set: {
                    stage: OrderStage.shipped
                }
            }
        }
    }
}
```

```text output
error: `Order.stage` has no starting value
 --> main.jadpo:8:5
   |
 8 |     stage: OrderStage
   |     ^^^^^ required, and only transitions set it
12 |             ship: {
   |             ---- set here
  reason: Only the lifecycle's transitions change `stage`, so a create can't give it; a required field gets its first value from `initial:`, and an optional one starts as `none`.
  next: Add `stage: <value>` to the lifecycle's `initial:` (agent)
  rule: data.lifecycle_initial_required
  help: https://jadpo.dev/docs/errors/data.lifecycle_initial_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If every new record starts at the same value**, give it in `initial:`:

```jadpo fragment
    lifecycle: {
        initial: {
            stage: OrderStage.placed
        }
        ...
    }
```

```jadpo check project=e-lifecycle-initial-required-fix-1 hidden
type OrderStage = Enum {
    placed
    shipped
}

entity Order {
    id: Uuid
    stage: OrderStage

    lifecycle: {
        initial: {
            stage: OrderStage.placed
        }
        transitions: {
            ship: {
                when: stage == OrderStage.placed
                set: {
                    stage: OrderStage.shipped
                }
            }
        }
    }
}
```

A new order now starts as `placed`, and only `ship` changes it.

**If a new record has no value yet**, make the field optional:

```jadpo fragment
    stage: OrderStage?
```

```jadpo check project=e-lifecycle-initial-required-fix-2 hidden
type OrderStage = Enum {
    placed
    shipped
}

entity Order {
    id: Uuid
    stage: OrderStage?

    lifecycle: {
        transitions: {
            ship: {
                when: stage == OrderStage.placed
                set: {
                    stage: OrderStage.shipped
                }
            }
        }
    }
}
```

A new order now starts with `stage` as `none`.

## Related

- [Lifecycle](../language/05-data.md#lifecycle)
- [Transitions](../language/06-reads-and-writes.md#transitions)
