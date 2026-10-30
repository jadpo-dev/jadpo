# `syntax.collection_type`

A collection names its types in angle brackets: `List<Tag>`, `Set<Tag>`, `Map<Tag, Count>`.

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
