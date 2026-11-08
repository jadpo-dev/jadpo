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

A variant that declares the same field twice in its block is the same
error:

```jadpo check project=e-variant-duplicate-declared expect=type.duplicate_variant_field
type DeclineReason = Text {
    max_length: 200
}

type PaymentOutcome = Enum {
    declined: {
        reason: DeclineReason
        reason: DeclineReason
    }
}
```

```text output
error: `PaymentOutcome.declined` declares `reason` twice
 --> main.jadpo:8:9
  |
7 |         reason: DeclineReason
  |         ------ first declared here
8 |         reason: DeclineReason
  |         ^^^^^^ declared again
  reason: Each field is written once; a second line with the same name would leave its reader guessing which one counts.
  next: Remove one `reason`, or rename it (agent)
  rule: type.duplicate_variant_field
  help: https://jadpo.dev/docs/errors/type.duplicate_variant_field

check failed: 1 error, 1 file
```

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

**If the variant declares it twice**, remove one line from its block,
or rename it if it was meant to be another field:

```jadpo fragment
    declined: {
        reason: DeclineReason
    }
```

```jadpo check project=e-variant-duplicate-declared-fix hidden
type DeclineReason = Text {
    max_length: 200
}

type PaymentOutcome = Enum {
    declined: {
        reason: DeclineReason
    }
}
```

The variant now has one `reason`, so a value of it has one too.

## Related

- [Enums](../language/02-values-and-types.md#enums)
- [Creating values](../language/02-values-and-types.md#creating-values)
