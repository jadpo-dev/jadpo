# `syntax.type_parent_required`

```jadpo check project=e-parent expect=syntax.type_parent_required
type BoxSize = {
    width: Int
}
```

```text output
error: `BoxSize` needs a base type
 --> main.jadpo:1:16
  |
1 | type BoxSize = {
  |                ^ expected a base before `{`
  reason: Every type is written `type Name = Base { ... }`; the base says what it is built from, such as `Object`, `Enum`, `Input` or `Text`.
  next: Add the base before `{`, for example `type BoxSize = Object {` (agent)
  rule: syntax.type_parent_required
  help: docs/errors/syntax.type_parent_required.md

check failed: 1 error, 1 file
```
