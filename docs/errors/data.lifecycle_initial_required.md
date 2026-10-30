# `data.lifecycle_initial_required`

A required field only transitions change gets its first value from `initial:`:

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
