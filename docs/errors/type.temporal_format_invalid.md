# `type.temporal_format_invalid`

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

A format named as the type of a field, a parameter or a variable, or used
as a value, is reported the same way: it holds no value.
