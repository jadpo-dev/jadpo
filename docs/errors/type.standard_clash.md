# `type.standard_clash`

A declaration named like a standard type: a Jadpo release added the
standard type after the application named its own. Its fixes are the swap
and the rename, each made by `jadpo fix`.

```jadpo check project=e-standard-clash expect=type.standard_clash
/// A room booking's reference, from before Jadpo had a `Uuid` of its own.
type Uuid = Text {
    max_length: 36
}
```

```text output
error: `Uuid` is the name of a standard type
 --> main.jadpo:2:6
  |
2 | type Uuid = Text {
  |      ^^^^ standard since Jadpo 0.1.0
  reason: Jadpo 0.1.0 added the standard type `Uuid`, and the application's own type can't share its name, or `Uuid` would mean two things. An upgrade never changes what code means silently, so `check` stops here until this type is swapped for the standard type or renamed.
  next: Run `jadpo fix Uuid swap` to use the standard `Uuid` in its place, or `jadpo fix Uuid rename <NewName>` to keep this one under a name of its own (agent)
  rule: type.standard_clash
  help: docs/errors/type.standard_clash.md

check failed: 1 error, 1 file
```
