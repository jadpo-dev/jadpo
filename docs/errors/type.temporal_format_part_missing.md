# `type.temporal_format_part_missing`

Your code formats a `LocalTime` or a `LocalDateTime` with a format that shows a part it doesn't have, such as a date or a zone.

## Why Jadpo refuses it

A `LocalTime` is a clock time with no date or zone, and a `LocalDateTime`
a date and a clock time with no zone, so `text.format` shows each only in
a format of the parts it has. A format showing a date is refused for a
`LocalTime`, and one showing a zone for either. Nothing is made up to fill
the part, as
[`type.temporal_format_requires_time`](../errors/type.temporal_format_requires_time.md)
refuses a clock time for a date.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.temporal_format_part_missing

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If only the parts it has matter**, use a format of those parts:

```jadpo fragment
    return OpeningSign(text.format(doors, locale: sign_locale, format: TimeFormat.time_short))
```

```jadpo check project=e-temporal-format-part-missing-fix-1 hidden
type OpeningSign = PresentationText {}

/// When the shop opens each day, as its sign shows it.
function opening_sign(doors: LocalTime, sign_locale: Locale) -> OpeningSign {
    return OpeningSign(text.format(doors, locale: sign_locale, format: TimeFormat.time_short))
}
```

The sign now shows only the time.

**If the format's date is what you want**, pass a value that has one:

```jadpo fragment
function opening_sign(doors: LocalDateTime, sign_locale: Locale) -> OpeningSign {
    ...
}
```

```jadpo check project=e-temporal-format-part-missing-fix-2 hidden
type OpeningSign = PresentationText {}

/// When the shop opens next, as its sign shows it.
function opening_sign(doors: LocalDateTime, sign_locale: Locale) -> OpeningSign {
    return OpeningSign(text.format(doors, locale: sign_locale, format: TimeFormat.date_time_short))
}
```

The sign now shows the date and the time, and callers pass both.

## Related

- [Time and calendar additions](../language/19-standard-library.md#time-and-calendar-additions)
- [Built-in formats](../language/10-configuration-and-time.md#built-in-formats)
