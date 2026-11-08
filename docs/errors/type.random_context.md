# `type.random_context`

A function, an inline function or a constant calls `random`, or an operation that needs random bytes such as `crypto.encrypt`, which only an action, a test or a fixture may do.

## Why Jadpo refuses it

A function gives the same result for the same arguments, so a test, a
query or a reviewer can rely on it without knowing when it ran. A random
value is different each time, so only actions, tests and fixtures call
`random`. An inline function and a constant are pure too. `crypto.encrypt`
and `crypto.hash_password` need random bytes, for a nonce and a salt, so
the same goes for them.

## What it looks like

main.jadpo:

```jadpo check project=e-random-context expect=type.random_context
type OrderReference = Text {}

/// An order's reference, which a customer reads out on the phone.
function order_reference() -> OrderReference {
    return OrderReference(random.code(8))
}
```

```text output
error: A function can't call `random.code`
 --> main.jadpo:5:27
  |
5 |     return OrderReference(random.code(8))
  |                           ^^^^^^^^^^^^^^ reads the generator
  reason: A function is pure: it gives the same result for the same arguments, so a random value it needs arrives as a parameter; only actions read the generator, which a test seeds so that it repeats.
  next: Pass the value in as a parameter, made in the action that calls this (agent)
  rule: type.random_context
  help: https://jadpo.dev/docs/errors/type.random_context

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If making the value is all it does**, make it an action:

```jadpo fragment
action order_reference() -> OrderReference {
    return OrderReference(random.code(8))
}
```

```jadpo check project=e-random-context-fix-1 hidden
type OrderReference = Text {}

/// An order's reference, which a customer reads out on the phone.
action order_reference() -> OrderReference {
    return OrderReference(random.code(8))
}
```

Each call now gives a new reference, and a test gets the same values on
every run.

**If the function does more with the value**, pass the value in, made in
the action that calls it:

```jadpo fragment
function order_reference(code: ReferenceCode) -> OrderReference {
    return OrderReference(code)
}

action new_order_reference() -> OrderReference {
    return order_reference(ReferenceCode(random.code(8)))
}
```

```jadpo check project=e-random-context-fix-2 hidden
type OrderReference = Text {}

type ReferenceCode = Text {}

/// An order's reference, which a customer reads out on the phone.
function order_reference(code: ReferenceCode) -> OrderReference {
    return OrderReference(code)
}

/// Makes a new reference for an order.
action new_order_reference() -> OrderReference {
    return order_reference(ReferenceCode(random.code(8)))
}
```

`order_reference` is pure again, and a test can call it with any code.

## Related

- [Randomness](../language/19-standard-library.md#randomness)
- [Functions and actions](../language/03-logic.md#functions-and-actions)
