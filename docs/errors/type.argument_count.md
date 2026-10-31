# `type.argument_count`

A call passes more or fewer values than the function or action it calls has parameters.

## Why Jadpo refuses it

Your own functions and actions take their values in order, one for each
parameter, and no parameter has a default. A call that passes too few
leaves a parameter with no value, and one that passes too many has a
value with nowhere to go. Jadpo won't fill in a missing value or drop an
extra one, because either would run the call on values you didn't write.

## What it looks like

main.jadpo:

```jadpo check project=e-argument-count expect=type.argument_count
type Name = Text {}

function choose(first: Name, second: Name) -> Name {
    return first
}

function incomplete(name: Name) -> Name {
    return choose(name)
}
```

```text output
error: `choose` takes 2 values, not 1
 --> main.jadpo:8:18
  |
8 |     return choose(name)
  |                  ^^^^^^ 1 given
  reason: `choose(first: Name, second: Name)` has 2 parameters; each is given, in order, and none has a default.
  next: Pass `second` as well (agent)
  rule: type.argument_count
  help: https://jadpo.dev/docs/errors/type.argument_count

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Passing too many values is the same error, and its `next:` line says to
remove the extra one:

main.jadpo:

```jadpo check project=e-argument-count-extra expect=type.argument_count
type Name = Text {}

function shout(name: Name) -> Name {
    return name
}

function twice(name: Name, other: Name) -> Name {
    return shout(name, other)
}
```

```text output
error: `shout` takes 1 value, not 2
 --> main.jadpo:8:17
  |
8 |     return shout(name, other)
  |                 ^^^^^^^^^^^^^ 2 given
  reason: `shout(name: Name)` has 1 parameter; each is given, in order, and none has a default.
  next: Remove the extra value (agent)
  rule: type.argument_count
  help: https://jadpo.dev/docs/errors/type.argument_count

check failed: 1 error, 1 file
```

## How to fix it

**If a value is missing**, pass one for every parameter, in the order the
signature lists them:

```jadpo fragment
function incomplete(name: Name, fallback: Name) -> Name {
    return choose(name, fallback)
}
```

```jadpo check project=e-argument-count-fix-1 hidden
type Name = Text {}

function choose(first: Name, second: Name) -> Name {
    return first
}

function incomplete(name: Name, fallback: Name) -> Name {
    return choose(name, fallback)
}
```

`choose` gets `name` as `first` and `fallback` as `second`.

**If a value is extra**, remove it:

```jadpo fragment
    return shout(name)
```

```jadpo check project=e-argument-count-fix-2 hidden
type Name = Text {}

function shout(name: Name) -> Name {
    return name
}

function twice(name: Name, other: Name) -> Name {
    return shout(name)
}
```

`shout` gets the one value it takes. If the call needs the extra value,
the function needs a parameter for it: add one to its signature, and pass
it at every call.

## Related

- [Signatures and settings](../language/03-logic.md#signatures-and-settings)
- [Calling](../language/03-logic.md#calling)
