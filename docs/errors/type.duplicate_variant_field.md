# `type.duplicate_variant_field`

An enum variant's value is built with the same field given twice, or a variant declares a field twice.

## Why Jadpo refuses it

A variant with fields, such as `PaymentOutcome.declined`, has one value
for each of its fields. Two `reason:` lines leave a reader to work out
which one counts. Jadpo won't pick the first or the last, because the
two may differ and either choice would quietly drop the other.

## What it looks like

main.jadpo:

```jadpo check project=e-variant-duplicate expect=type.duplicate_variant_field
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
        reason: reason
    }
}
```

```text output
error: `reason` is given twice
 --> main.jadpo:14:9
   |
13 |         reason: reason
   |         ------ first given here
14 |         reason: reason
   |         ^^^^^^ given again
  reason: Each field of `PaymentOutcome.declined` is given once, so there is never a question of which value counts.
  next: Remove one `reason` (agent)
  rule: type.duplicate_variant_field
  help: https://jadpo.dev/docs/errors/type.duplicate_variant_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If both lines give the same field**, keep the one with the value you
mean and remove the other:

```jadpo fragment
    return PaymentOutcome.declined {
        reason: reason
    }
```

```jadpo check project=e-variant-duplicate-fix hidden
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

The `declined` value is built with one `reason`. If the second line was
meant for another field, give it that field's name, and declare the
field in the variant's block if it has none.

## Related

- [Enums](../language/02-values-and-types.md#enums)
- [Creating values](../language/02-values-and-types.md#creating-values)
