# `type.type_argument_count`

A type is given more or fewer type arguments than it takes, such as two for a `List`.

## Why Jadpo refuses it

A `List` holds values of the type in `< >`, so it takes exactly one type
argument, and a type built to hold two kinds of thing names them both.
Jadpo can't tell what an extra one would mean, or what a missing one would
have been.

## What it looks like

main.jadpo:

```jadpo check project=e-type-arguments expect=type.type_argument_count
type Tag = Text {
    max_length: 30
}

type Basket = Object {
    tags: List<Tag, Tag>
}
```

```text output
error: `List` takes one type argument, as in `List<Tag>`
 --> main.jadpo:6:11
  |
6 |     tags: List<Tag, Tag>
  |           ^^^^^^^^^^^^^^ 2 given
  reason: `List` holds values of the type in `< >`, so it needs exactly 1.
  next: Write `List` with one type argument, as in `List<Tag>` (agent)
  rule: type.type_argument_count
  help: https://jadpo.dev/docs/errors/type.type_argument_count

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the list holds one kind of value**, give that one type:

```jadpo fragment
type Basket = Object {
    tags: List<Tag>
}
```

```jadpo check project=e-type-arguments-fix hidden
type Tag = Text {
    max_length: 30
}

type Basket = Object {
    tags: List<Tag>
}
```

`tags` is now a list of `Tag` values.

## Related

- [Collections](../language/02-values-and-types.md#collections)
