# `type.random_context`

A `random` operation reads the generator, so only actions, tests and
fixtures call it; a function is pure, and so is an inline function or a
constant, as for area 10's `type.clock_context`. So do `crypto.encrypt`,
for its nonce, and `crypto.hash_password`, for its salt.

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
