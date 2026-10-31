# `type.missing_field`

An object value is built without one of the fields its type declares.

## Why Jadpo refuses it

An object value is written with its type name and every field, so a
value always has what its type promises and nothing reads a field that
was never set. A field that may hold nothing is declared `T?` and still
given, as `none`, so leaving it empty is a choice you can see.

## What it looks like

main.jadpo:

```jadpo check project=e-missing-field expect=type.missing_field
type Receipt = Output {
    email: Email
}

function incomplete_receipt() -> Receipt {
    return Receipt {}
}
```

```text output
error: `Receipt` is missing `email`
 --> main.jadpo:6:12
  |
6 |     return Receipt {}
  |            ^^^^^^^ fields missing
  reason: Every field of `Receipt` is given when a value is built; a field that may be absent is declared `T?` and given as `none`.
  next: Add `email: ...` to the block (agent)
  rule: type.missing_field
  help: https://jadpo.dev/docs/errors/type.missing_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the value has the field**, give it in the block:

```jadpo fragment
function incomplete_receipt(email: Email) -> Receipt {
    return Receipt {
        email: email
    }
}
```

```jadpo check project=e-missing-field-fix-1 hidden
type Receipt = Output {
    email: Email
}

function incomplete_receipt(email: Email) -> Receipt {
    return Receipt {
        email: email
    }
}
```

Every `Receipt` now carries the email it is sent to.

**If a receipt may have no email**, declare the field `Email?` and give
it as `none`:

```jadpo check project=e-missing-field-fix-2
type Receipt = Output {
    email: Email?
}

function incomplete_receipt() -> Receipt {
    return Receipt {
        email: none
    }
}
```

Code that reads `email` now handles both cases, usually with `match`.

## Related

- [Objects](../language/02-values-and-types.md#objects)
- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)
