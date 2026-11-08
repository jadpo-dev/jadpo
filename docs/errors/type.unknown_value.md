# `type.unknown_value`

Code reads a name that isn't defined where it is used, often a misspelling of one that is.

## Why Jadpo refuses it

A name has to be declared where it is read: as a parameter, a variable or
something a `use` line brings in. Jadpo can't tell what a name it can't
find was meant to be, so it stops and names the nearest one, if there is
one.

## What it looks like

main.jadpo:

```jadpo check project=e-unknown-value expect=type.unknown_value
type Name = Text {}

function echo(name: Name) -> Name {
    return nmae
}
```

```text output
error: `nmae` isn't defined in this scope
 --> main.jadpo:4:12
  |
4 |     return nmae
  |            ^^^^ not defined
  reason: Jadpo could not find this name. Did you mean `name`?
  next: Use `name` instead of `nmae` (agent)
  rule: type.unknown_value
  help: https://jadpo.dev/docs/errors/type.unknown_value

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the name is misspelt**, use the name the error gives:

```jadpo fragment
    return name
```

```jadpo check project=e-unknown-value-fix hidden
type Name = Text {}

function echo(name: Name) -> Name {
    return name
}
```

`echo` now gives back what it was passed.

## Related

- [Functions and actions](../language/03-logic.md#functions-and-actions)
- [Variables](../language/03-logic.md#variables)
