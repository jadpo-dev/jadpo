# `type.named_argument_unsupported`

A call to your own function or action names its arguments, which only the standard library's operations take.

## Why Jadpo refuses it

Your own functions and actions are called with their arguments in the
order of their parameters, so every call to one reads the same way.
Named arguments are kept for standard-library operations, as in
`temporal.resolve(date: d, zone: z, ...)`, whose many arguments read
more clearly by name.

## What it looks like

main.jadpo:

```jadpo check project=e-named-argument expect=type.named_argument_unsupported
type Number = Int {}

function add(left: Number, right: Number) -> Number {
    return Number(left + right)
}

function three() -> Number {
    return add(left: 1, right: 2)
}
```

```text output
error: `add` takes its arguments in order, without names
 --> main.jadpo:8:16
  |
8 |     return add(left: 1, right: 2)
  |                ^^^^ named
  reason: Named arguments exist only for standard-library operations; your own callables are called positionally, in the order of their parameters.
  next: Remove the argument names (agent)
  rule: type.named_argument_unsupported
  help: https://jadpo.dev/docs/errors/type.named_argument_unsupported

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the arguments are already in order**, remove the names:

```jadpo fragment
function three() -> Number {
    return add(1, 2)
}
```

```jadpo check project=e-named-argument-fix hidden
type Number = Int {}

function add(left: Number, right: Number) -> Number {
    return Number(left + right)
}

function three() -> Number {
    return add(1, 2)
}
```

`add` takes `1` as `left` and `2` as `right`, by their order.

## Related

- [Calling](../language/03-logic.md#calling)
- [Signatures and settings](../language/03-logic.md#signatures-and-settings)
