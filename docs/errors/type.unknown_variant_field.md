# `type.unknown_variant_field`

An enum variant's value is built with a field the variant doesn't declare.

## Why Jadpo refuses it

A variant lists the fields it carries, the same way an object does, so a
name that isn't among them can never be one. Jadpo reports it when you
check, and lists the fields there are.

## What it looks like

main.jadpo:

```jadpo check project=e-variant-unknown expect=type.unknown_variant_field
type DeclineReason = Text {
    max_length: 200
}

type PaymentOutcome = Enum {
    declined: {
        reason: DeclineReason
    }
}

function declined_payment(reason: DeclineReason) -> PaymentOutcome {
    return PaymentOutcome.declined {
        reason: reason
        note: reason
    }
}
```

```text output
error: `PaymentOutcome.declined` has no field `note`
 --> main.jadpo:14:9
   |
14 |         note: reason
   |         ^^^^ not declared
  reason: `PaymentOutcome.declined`'s fields are `reason`.
  next: Remove `note`, or declare it on `PaymentOutcome.declined` (agent)
  rule: type.unknown_variant_field
  help: https://jadpo.dev/docs/errors/type.unknown_variant_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the variant should have this field.

**If it shouldn't**, remove the line:

```jadpo fragment
    return PaymentOutcome.declined {
        reason: reason
    }
```

```jadpo check project=e-variant-unknown-fix-1 hidden
type DeclineReason = Text {
    max_length: 200
}

type PaymentOutcome = Enum {
    declined: {
        reason: DeclineReason
    }
}

function declined_payment(reason: DeclineReason) -> PaymentOutcome {
    return PaymentOutcome.declined {
        reason: reason
    }
}
```

The variant is now built from the one field it has.

**If it should**, add the field to the variant:

```jadpo fragment
    declined: {
        reason: DeclineReason
        note: DeclineReason
    }
```

```jadpo check project=e-variant-unknown-fix-2 hidden
type DeclineReason = Text {
    max_length: 200
}

type PaymentOutcome = Enum {
    declined: {
        reason: DeclineReason
        note: DeclineReason
    }
}

function declined_payment(reason: DeclineReason) -> PaymentOutcome {
    return PaymentOutcome.declined {
        reason: reason
        note: reason
    }
}
```

Every `PaymentOutcome.declined` now has a `note`, so other places that
build one have to give it.

## Related

- [Enums](../language/02-values-and-types.md#enums)
