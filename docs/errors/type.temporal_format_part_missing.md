# `type.temporal_format_part_missing`

A `LocalTime` is a clock time with no date or zone, and a
`LocalDateTime` a date and a clock time with no zone, so `text.format`
shows each only in a format of the parts it has: a format showing a
date is refused for a `LocalTime`, and one showing a zone for either, as
`type.temporal_format_requires_time` refuses a clock time for a date
(area 10). Nothing is made up to fill the part.

```jadpo check project=e-temporal-format-part-missing expect=type.temporal_format_part_missing
type OpeningSign = PresentationText {}

/// When the shop opens each day, as its sign shows it.
function opening_sign(doors: LocalTime, sign_locale: Locale) -> OpeningSign {
    return OpeningSign(text.format(doors, locale: sign_locale, format: TimeFormat.date_time_short))
}
```

```text output
error: `TimeFormat.date_time_short` shows a part `LocalTime` doesn't have
 --> main.jadpo:5:72
  |
5 |     return OpeningSign(text.format(doors, locale: sign_locale, format: TimeFormat.date_time_short))
  |                                                                        ^^^^^^^^^^^^^^^^^^^^^^^^^^ shows a date
  reason: `LocalTime` is a clock time, with no date or zone, so `text.format` can show only the parts it has.
  next: Use a time format such as `TimeFormat.time_short` (agent)
  rule: type.temporal_format_part_missing
  help: docs/errors/type.temporal_format_part_missing.md

check failed: 1 error, 1 file
```
