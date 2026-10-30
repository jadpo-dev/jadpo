# `type.type_argument_count`

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
