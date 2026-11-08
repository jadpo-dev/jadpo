# `semantic.standard_namespace_reserved`

A declaration, domain, library or variable takes the name of a standard-library namespace.

## Why Jadpo refuses it

A dot in a call names who owns the operation, so `temporal.` has to mean
the standard library wherever it appears. A declaration of the same name
would make `temporal.in_zone(...)` mean something different in one file.

## What it looks like

main.jadpo:

```jadpo check project=e-std-namespace expect=semantic.standard_namespace_reserved
type Count = Int {
    min: 0
}

function temporal(x: Count) -> Count {
    return x
}
```

```text output
error: `temporal` is a standard-library namespace
 --> main.jadpo:5:10
  |
5 | function temporal(x: Count) -> Count {
  |          ^^^^^^^^ reserved
  reason: Standard-library namespaces can't name a declaration, domain, library or variable, so `temporal.` always means the standard library.
  next: Choose another name than `temporal` (agent)
  rule: semantic.standard_namespace_reserved
  help: https://jadpo.dev/docs/errors/semantic.standard_namespace_reserved

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it's your own declaration**, name it something else, here and where
it's used:

```jadpo fragment
function time_passed(x: Count) -> Count {
    return x
}
```

```jadpo check project=e-std-namespace-fix hidden
type Count = Int {
    min: 0
}

function time_passed(x: Count) -> Count {
    return x
}
```

`temporal.` still means the standard library, everywhere.

## Related

- [Words that are names everywhere else](../language/01-project-layout-and-modules.md#words-that-are-names-everywhere-else)
- [Calls name their owner](../language/01-project-layout-and-modules.md#calls-name-their-owner)
