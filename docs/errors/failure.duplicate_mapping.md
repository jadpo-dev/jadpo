# `failure.duplicate_mapping`

An `attempt` block or a `conflict:` block maps the same condition twice.

## Why Jadpo refuses it

Each condition gives the caller one failure. Mapped twice, there would be a
question of which one the caller gets, and the second arm would never run.

## What it looks like

main.jadpo:

```jadpo check project=e-duplicate-mapping expect=failure.duplicate_mapping
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityTooLarge
        max: QuantityTooLarge
    }
    return result
}
```

```text output
error: `max:` is mapped twice
 --> main.jadpo:13:9
   |
12 |         max: QuantityTooLarge
   |         --- first mapped here
13 |         max: QuantityTooLarge
   |         ^^^ mapped again
  reason: Each condition is mapped once, so there is never a question of which failure the caller gets.
  next: Remove one `max:` (agent)
  rule: failure.duplicate_mapping
  help: https://jadpo.dev/docs/errors/failure.duplicate_mapping

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

The same goes for a write's `conflict:`, which maps each unique rule once:

main.jadpo:

```jadpo check project=e-duplicate-mapping-conflict expect=failure.duplicate_mapping
type Handle = Text {
    max_length: 40
}

type HandleTaken = Failure {
    kind: FailureKind.conflict
}

entity Account {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
    }
}

action claim(handle: Handle) fails HandleTaken -> Account {
    var account: Account = attempt Account.create {
        set: {
            handle: handle
        }
        conflict: {
            Account.handle: HandleTaken
            Account.handle: HandleTaken
        }
    }
    return account
}
```

```text output
error: `Account.handle:` is mapped twice
 --> main.jadpo:27:13
   |
26 |             Account.handle: HandleTaken
   |             -------------- first mapped here
27 |             Account.handle: HandleTaken
   |             ^^^^^^^^^^^^^^ mapped again
  reason: Each condition is mapped once, so there is never a question of which failure the caller gets.
  next: Remove one `Account.handle:` (agent)
  rule: failure.duplicate_mapping
  help: https://jadpo.dev/docs/errors/failure.duplicate_mapping

check failed: 1 error, 1 file
```

## How to fix it

**If it is a repeat**, remove one:

```jadpo fragment
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityTooLarge
    }
```

```jadpo check project=e-duplicate-mapping-fix-1 hidden
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityTooLarge
    }
    return result
}
```

A doubled quantity over 1000 now ends with `QuantityTooLarge`.

## Related

- [Turning one failure into another](../language/04-failures.md#turning-one-failure-into-another)
- [A value built at run time: rule keys and `invalid:`](../language/04-failures.md#a-value-built-at-run-time-rule-keys-and-invalid)
- [A write that breaks a unique rule: `conflict:`](../language/04-failures.md#a-write-that-breaks-a-unique-rule-conflict)
