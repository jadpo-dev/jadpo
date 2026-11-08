# `type.missing_variant_field`

An enum variant's value is built without one of the fields the variant declares.

## Why Jadpo refuses it

A variant with fields carries every one of them, so code that matches it
can read each field without checking it is there. Building one without a
field would leave that code reading nothing. Jadpo won't fill in a value
for you, because it can't know the right one; a field that may be absent
is declared `T?` and given as `none`.

## What it looks like

main.jadpo:

```jadpo check project=e-variant-missing expect=type.missing_variant_field
type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

function paid() -> PaymentOutcome {
    return PaymentOutcome.paid {}
}
```

```text output
error: `PaymentOutcome.paid` is missing `receipt_id`
 --> main.jadpo:11:12
   |
11 |     return PaymentOutcome.paid {}
   |            ^^^^^^^^^^^^^^^^^^^ fields missing
  reason: Every field of `PaymentOutcome.paid` is given when a value is built; a field that may be absent is declared `T?` and given as `none`.
  next: Add `receipt_id: ...` to the block (agent)
  rule: type.missing_variant_field
  help: https://jadpo.dev/docs/errors/type.missing_variant_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the field always has a value.

**If it always does**, give it in the block:

```jadpo fragment
function paid(receipt_id: ReceiptId) -> PaymentOutcome {
    return PaymentOutcome.paid {
        receipt_id: receipt_id
    }
}
```

```jadpo check project=e-variant-missing-fix-1 hidden
type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

function paid(receipt_id: ReceiptId) -> PaymentOutcome {
    return PaymentOutcome.paid {
        receipt_id: receipt_id
    }
}
```

A paid outcome now carries the receipt it was paid with.

**If it can be absent**, declare it `T?` and give it as `none`:

```jadpo fragment
type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId?
    }
}

function paid() -> PaymentOutcome {
    return PaymentOutcome.paid {
        receipt_id: none
    }
}
```

```jadpo check project=e-variant-missing-fix-2 hidden
type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId?
    }
}

function paid() -> PaymentOutcome {
    return PaymentOutcome.paid {
        receipt_id: none
    }
}
```

Code that reads a paid outcome's `receipt_id` now matches `some` and
`none`.

## Related

- [Enums](../language/02-values-and-types.md#enums)
- [Creating values](../language/02-values-and-types.md#creating-values)
- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)
