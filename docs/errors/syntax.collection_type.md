# `syntax.collection_type`

A collection type is written in another language's form instead of `List<T>`, `Set<T>` or `Map<K, V>`.

## Why Jadpo refuses it

`T[]`, `[]T`, `[T]` and `list[T]` each mean a list in another language.
Jadpo writes every collection the same way, its kind with its types in angle
brackets, so a list, a set and a map read alike.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-collection expect=syntax.collection_type
type Tag = Text {
    max_length: 30
}

type Basket = Object {
    tags: Tag[]
}
```

```text output
error: Jadpo writes `List<Tag>`, not `Tag[]`
 --> main.jadpo:6:14
  |
6 |     tags: Tag[]
  |              ^^ write `List<Tag>`
  reason: `T[]` is a list type in TypeScript, Java and C#, `[]T` in Go, and `list[T]` in Python; Jadpo writes a collection's types in angle brackets, as in `List<T>`, `Set<T>` and `Map<K, V>`.
  next: Write `List<Tag>` (agent)
  rule: syntax.collection_type
  help: https://jadpo.dev/docs/errors/syntax.collection_type

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant a list**, write `List<Tag>`:

```jadpo fragment
type Basket = Object {
    tags: List<Tag>
}
```

```jadpo check project=e-habit-collection-fix hidden
type Tag = Text {
    max_length: 30
}

type Basket = Object {
    tags: List<Tag>
}
```

`tags` now holds a list of `Tag` values.

## Related

- [Collections](../language/02-values-and-types.md#collections)
