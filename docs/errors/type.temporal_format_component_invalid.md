# `type.temporal_format_component_invalid`

A part of a `TimeFormat` has a style it doesn't take, such as `day: ComponentStyle.long`.

## Why Jadpo refuses it

Each part of a format takes only the styles that mean something for it. A
`day` is a number, so it is `numeric` or `two_digit`, and only a `month`,
a `weekday` or a `zone_name` has a name to show in `long` or `short`. The
locale decides the rest, such as the order and the punctuation.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.temporal_format_component_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A part that isn't one a format shows, such as `days:`, is
[`type.unknown_setting`](../errors/type.unknown_setting.md), with the
nearest part's name.

## How to fix it

**If the style isn't one the part takes**, use one it does:

```jadpo fragment
type ShortDate = TimeFormat {
    day: ComponentStyle.numeric
    month: ComponentStyle.short
}
```

```jadpo check project=e-component-invalid-fix hidden
type ShortDate = TimeFormat {
    day: ComponentStyle.numeric
    month: ComponentStyle.short
}
```

The day now shows as a number, as wide as the locale writes it.

## Related

- [Declared formats](../language/10-configuration-and-time.md#declared-formats)
