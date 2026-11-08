# `type.temporal_format_invalid`

A `format:` is missing or isn't a built-in `TimeFormat` or the name of a declared one, or a format is used as a type or a value.

## Why Jadpo refuses it

A format holds no value, so `format:` names one written out: a built-in
format, `TimeFormat.` followed by `date_`, `time_` or `date_time_` and
`full`, `long`, `medium` or `short`, or the name of a format you declared.
Anything else isn't a format, and Jadpo can't tell what parts to show.

## What it looks like

main.jadpo:

```jadpo check project=e-format-invalid expect=type.temporal_format_invalid
type OrderedAt = Instant {}

function placed_label(
    ordered_at: OrderedAt,
    zone: Zone,
    reader_locale: Locale
) -> PresentationText {
    return text.format(
        ordered_at,
        zone: zone,
        locale: reader_locale,
        format: TimeFormat.date_longest
    )
}
```

```text output
error: `TimeFormat.date_longest` isn't a time format
 --> main.jadpo:12:17
   |
12 |         format: TimeFormat.date_longest
   |                 ^^^^^^^^^^^^^^^^^^^^^^^ not a format
  reason: `text.format`'s `format:` is written out: a built-in format, `TimeFormat.` followed by `date_`, `time_` or `date_time_` and `full`, `long`, `medium` or `short`, or the name of a format declared as `type Name = TimeFormat { ... }`.
  next: Write a built-in format such as `TimeFormat.date_long`, or declare one with `type Name = TimeFormat { ... }` (agent)
  rule: type.temporal_format_invalid
  help: https://jadpo.dev/docs/errors/type.temporal_format_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A format named as the type of a field, a parameter or a variable, or used
as a value, is reported the same way, and so is a `format:` left out.

## How to fix it

**If you meant a built-in format**, write one of their names:

```jadpo fragment
    return text.format(ordered_at, zone: zone, locale: reader_locale, format: TimeFormat.date_long)
```

```jadpo check project=e-format-invalid-fix-1 hidden
type OrderedAt = Instant {}

function placed_label(
    ordered_at: OrderedAt,
    zone: Zone,
    reader_locale: Locale
) -> PresentationText {
    return text.format(ordered_at, zone: zone, locale: reader_locale, format: TimeFormat.date_long)
}
```

The text now shows the date in the locale's long form.

**If no built-in format shows what you want**, declare one and name it in
`format:`:

```jadpo fragment
type OrderedOn = TimeFormat {
    day: ComponentStyle.numeric
    month: ComponentStyle.short
}
```

```jadpo fragment
    return text.format(ordered_at, zone: zone, locale: reader_locale, format: OrderedOn)
```

```jadpo check project=e-format-invalid-fix-2 hidden
type OrderedAt = Instant {}

type OrderedOn = TimeFormat {
    day: ComponentStyle.numeric
    month: ComponentStyle.short
}

function placed_label(
    ordered_at: OrderedAt,
    zone: Zone,
    reader_locale: Locale
) -> PresentationText {
    return text.format(ordered_at, zone: zone, locale: reader_locale, format: OrderedOn)
}
```

With `format: OrderedOn`, the text shows just the day and the short month.

## Related

- [Built-in formats](../language/10-configuration-and-time.md#built-in-formats)
- [Declared formats](../language/10-configuration-and-time.md#declared-formats)
