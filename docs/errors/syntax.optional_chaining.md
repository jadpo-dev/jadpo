# `syntax.optional_chaining`

A field of a value that may be `none` is read with `?.` instead of a `match`.

## Why Jadpo refuses it

`?.` reads a field of a value that may be missing in JavaScript, TypeScript,
C#, Kotlin and Swift, and quietly gives nothing when it is. Jadpo reads a
value that may be `none` with a `match` that handles both cases, so the
missing case is something the code says rather than something it passes on.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-chaining expect=syntax.optional_chaining
type ShippingCode = Text {
    max_length: 10
}

type Address = Object {
    postal_code: ShippingCode
}

type Customer = Object {
    address: Address?
}

function postal_code(customer: Customer) -> ShippingCode? {
    return customer.address?.postal_code
}
```

```text output
error: Jadpo reads a value that may be `none` with `match`, not `?.`
 --> main.jadpo:14:28
   |
14 |     return customer.address?.postal_code
   |                            ^^ write a `match`
  reason: `?.` reads a field of a value that may be missing in JavaScript, TypeScript, C#, Kotlin and Swift; Jadpo reads a value that may be `none` with a `match` that handles both cases.
  next: Write `match customer.address` with the arms `some(address) -> address.postal_code` and `none -> none` (agent)
  rule: syntax.optional_chaining
  help: https://jadpo.dev/docs/errors/syntax.optional_chaining

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the result may be missing too**, write the `match` the error names:

```jadpo fragment
function postal_code(customer: Customer) -> ShippingCode? {
    return match customer.address {
        some(address) -> address.postal_code
        none -> none
    }
}
```

```jadpo check project=e-habit-chaining-fix hidden
type ShippingCode = Text {
    max_length: 10
}

type Address = Object {
    postal_code: ShippingCode
}

type Customer = Object {
    address: Address?
}

function postal_code(customer: Customer) -> ShippingCode? {
    return match customer.address {
        some(address) -> address.postal_code
        none -> none
    }
}
```

`postal_code` now gives the code when the customer has an address, and
`none` when they don't.

## Related

- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)
- [Match](../language/03-logic.md#match)
