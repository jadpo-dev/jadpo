# `type.temporal_format_components_empty`

```jadpo check project=e-components-empty expect=type.temporal_format_components_empty
type Blank = TimeFormat {}
```

```text output
error: `Blank` shows nothing
 --> main.jadpo:1:6
  |
1 | type Blank = TimeFormat {}
  |      ^^^^^ no parts
  reason: A format lists the parts of a date or a time it shows, each with a `ComponentStyle`, and the locale arranges them; with none listed there is nothing to show.
  next: List the parts to show, such as `day: ComponentStyle.numeric`, or use a built-in format such as `TimeFormat.date_long` (agent)
  rule: type.temporal_format_components_empty
  help: docs/errors/type.temporal_format_components_empty.md

check failed: 1 error, 1 file
```
