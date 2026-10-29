# `syntax.optional_chaining`

A field of a value that may be `none` is read in a `match` too.

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
  help: docs/errors/syntax.optional_chaining.md

check failed: 1 error, 1 file
```
