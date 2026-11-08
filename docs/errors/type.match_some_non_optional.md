# `type.match_some_non_optional`

A `match` uses `some(...)` or `none` on a value whose type says it is never `none`.

## Why Jadpo refuses it

`some(name)` and `none` match a value that may be missing, written `T?`.
A value whose type has no `?` always holds one, so `some(...)` would
always match and `none` never would. The `match` usually comes from code
that expected the value to be optional, and the value can be used as it
is.

## What it looks like

main.jadpo:

```jadpo check project=e-match-some expect=type.match_some_non_optional
type Label = Text {}

function describe(label: Label) -> Label {
    return match label {
        some(name) -> name
        _ -> label
    }
}
```

```text output
error: `label` is never `none`, so `some(...)` can't match it
 --> main.jadpo:5:9
  |
5 |         some(name) -> name
  |         ^^^^^^^^^^ for nullable values
  reason: `some(name)` and `none` match a nullable value, written `T?`; `label` is a `Label`.
  next: Use `label` directly: it always has a value (agent)
  rule: type.match_some_non_optional
  help: https://jadpo.dev/docs/errors/type.match_some_non_optional

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the value can really be missing.

**If it always has a value**, use it without a `match`:

```jadpo fragment
function describe(label: Label) -> Label {
    return label
}
```

```jadpo check project=e-match-some-fix-1 hidden
type Label = Text {}

function describe(label: Label) -> Label {
    return label
}
```

`describe` gives the label it was passed.

**If it can be missing**, make its type nullable, and match `some` and
`none`:

```jadpo fragment
function describe(label: Label?) -> Label {
    return match label {
        some(name) -> name
        none -> "Untitled"
    }
}
```

```jadpo check project=e-match-some-fix-2 hidden
type Label = Text {}

function describe(label: Label?) -> Label {
    return match label {
        some(name) -> name
        none -> "Untitled"
    }
}
```

Callers may now pass `none`, and get "Untitled" back.

## Related

- [Match](../language/03-logic.md#match)
- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)
