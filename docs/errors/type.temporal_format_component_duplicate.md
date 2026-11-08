# `type.temporal_format_component_duplicate`

A `TimeFormat` gives the same part twice, such as two `day:` lines.

## Why Jadpo refuses it

A format shows each part once, in one style, so there is never a question
of which `day` counts. The two styles would show different text, and Jadpo
won't pick one for you.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.temporal_format_component_duplicate

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If a part is given twice**, keep the line with the style you want:

```jadpo fragment
type ShortDate = TimeFormat {
    day: ComponentStyle.two_digit
    month: ComponentStyle.short
}
```

```jadpo check project=e-component-duplicate-fix hidden
type ShortDate = TimeFormat {
    day: ComponentStyle.two_digit
    month: ComponentStyle.short
}
```

The format now shows the day once, in at least two digits.

## Related

- [Declared formats](../language/10-configuration-and-time.md#declared-formats)
