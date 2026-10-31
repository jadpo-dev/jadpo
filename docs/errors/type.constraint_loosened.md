# `type.constraint_loosened`

A type that builds on another sets a looser bound than the one it inherits, such as a higher `max_length`.

## Why Jadpo refuses it

A refinement, a type that builds on another, can only add to the rules
it inherits. Every rule along the chain still holds, so a value of
`ShortProductName` still needs `ProductName`'s `max_length: 120`. A
looser bound would never apply, and a reader would believe a value can
be longer than it can.

## What it looks like

main.jadpo:

```jadpo check project=e-constraint-loosened expect=type.constraint_loosened
type ProductName = Text {
    min_length: 1
    max_length: 120
}

type ShortProductName = ProductName {
    max_length: 150
}
```

```text output
error: `max_length: 150` loosens `ProductName`'s `max_length: 120`
 --> main.jadpo:7:5
  |
7 |     max_length: 150
  |     ^^^^^^^^^^ looser than `max_length: 120`
  reason: A refinement only adds to the rules of the type it builds on, and every rule along the way still holds, so a value still needs `max_length: 120` and this one would never apply.
  next: Make `max_length` at most 120, or remove it (agent)
  rule: type.constraint_loosened
  help: https://jadpo.dev/docs/errors/type.constraint_loosened

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the new type should be stricter**, give it a bound within the
inherited one:

```jadpo fragment
    max_length: 40
```

```jadpo check project=e-constraint-loosened-fix-1 hidden
type ProductName = Text {
    min_length: 1
    max_length: 120
}

type ShortProductName = ProductName {
    max_length: 40
}
```

A `ShortProductName` is now at most 40 characters, and it can still be
used wherever a `ProductName` is expected.

**If the inherited bound is the one you want**, remove the line:

```jadpo check project=e-constraint-loosened-fix-2
type ProductName = Text {
    min_length: 1
    max_length: 120
}

type ShortProductName = ProductName {}
```

`ShortProductName` keeps `ProductName`'s rules, 1 to 120 characters, and
is still its own type.

When the new type really needs longer values, build it on `Text` itself
rather than on `ProductName`. It then can't be passed where a
`ProductName` is expected without a construction, which checks the
shorter bound.

## Related

- [Named types](../language/02-values-and-types.md#named-types)
- [Constraints](../language/02-values-and-types.md#constraints)
- [When one type can stand in for another](../language/02-values-and-types.md#when-one-type-can-stand-in-for-another)
