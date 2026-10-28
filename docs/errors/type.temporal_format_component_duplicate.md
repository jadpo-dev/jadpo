# `type.temporal_format_component_duplicate`

```jadpo check project=e-component-duplicate expect=type.temporal_format_component_duplicate
type ShortDate = TimeFormat {
    day: ComponentStyle.numeric
    month: ComponentStyle.short
    day: ComponentStyle.two_digit
}
```

```text output
error: `day` is given twice
 --> main.jadpo:4:5
  |
2 |     day: ComponentStyle.numeric
  |     --- first given here
4 |     day: ComponentStyle.two_digit
  |     ^^^ given again
  reason: A format shows each part once, in one style, so there is never a question of which `day` counts.
  next: Remove one `day` (agent)
  rule: type.temporal_format_component_duplicate
  help: docs/language/10-configuration-and-time.html#type-temporal-format-component-duplicate

check failed: 1 error, 1 file
```
