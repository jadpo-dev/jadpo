# `type.temporal_format_component_invalid`

```jadpo check project=e-component-invalid expect=type.temporal_format_component_invalid
type ShortDate = TimeFormat {
    day: ComponentStyle.long
    month: ComponentStyle.short
}
```

```text output
error: `day` isn't shown `long`
 --> main.jadpo:2:10
  |
2 |     day: ComponentStyle.long
  |          ^^^^^^^^^^^^^^^^^^^ not a style `day` takes
  reason: A format's `day` is shown `ComponentStyle.numeric` or `ComponentStyle.two_digit`; the locale decides the rest, such as the order and the punctuation.
  next: Write `day: ComponentStyle.numeric` (agent)
  rule: type.temporal_format_component_invalid
  help: docs/errors/type.temporal_format_component_invalid.md

check failed: 1 error, 1 file
```

A part that isn't one a format shows, such as `days:`, is
`type.unknown_setting`, with the nearest part's name.
