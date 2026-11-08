# `type.named_argument_unknown`

A call names an argument that the operation it calls doesn't have, such as a misspelt `itmes:`.

## Why Jadpo refuses it

A standard-library operation's named arguments are its parameters' names,
so a name that isn't one of them gives the value to nothing. It is
usually a typo, or a name from another operation. The error lists the
operation's arguments and the nearest name when there is one.

## What it looks like

main.jadpo:

```jadpo check project=e-named-unknown expect=type.named_argument_unknown
type Stock = Int {
    min: 0
}

function line_count(lines: List<Stock>) -> Stock {
    return Stock(list.count(itmes: lines))
}
```

```text output
error: `list.count` has no argument `itmes`
 --> main.jadpo:6:29
  |
6 |     return Stock(list.count(itmes: lines))
  |                             ^^^^^ not a parameter
  reason: Its one argument is `items`.
  next: Use `items:` instead of `itmes:` (agent)
  rule: type.named_argument_unknown
  help: https://jadpo.dev/docs/errors/type.named_argument_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the name is misspelt**, write the parameter's name:

```jadpo fragment
function line_count(lines: List<Stock>) -> Stock {
    return Stock(list.count(items: lines))
}
```

```jadpo check project=e-named-unknown-fix hidden
type Stock = Int {
    min: 0
}

function line_count(lines: List<Stock>) -> Stock {
    return Stock(list.count(items: lines))
}
```

`line_count` gives the number of items in `lines`.

## Related

- [Calling](../language/03-logic.md#calling)
- [Lists](../language/19-standard-library.md#lists)
