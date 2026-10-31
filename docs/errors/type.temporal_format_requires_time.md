# `type.temporal_format_requires_time`

Your code formats a date with a format that also shows a clock time, and a date has no time to show.

## Why Jadpo refuses it

A `CalendarDate` is a date with no clock time and no zone, such as a
birthday or an invoice date. A format such as `TimeFormat.date_time_long`
shows a time of day as well, and so does a declared format that shows
`hour`, `minute`, `second` or `zone_name`. A date has no time to show,
and Jadpo won't make one up, such as midnight.

## What it looks like

main.jadpo:

```jadpo check project=e-requires-time expect=type.temporal_format_requires_time
type InvoiceDate = CalendarDate {}

function invoice_dated(day: InvoiceDate, reader_locale: Locale) -> PresentationText {
    return text.format(day, locale: reader_locale, format: TimeFormat.date_time_long)
}
```

```text output
error: `TimeFormat.date_time_long` shows a clock time, which `InvoiceDate` doesn't have
 --> main.jadpo:4:60
  |
4 |     return text.format(day, locale: reader_locale, format: TimeFormat.date_time_long)
  |                                                            ^^^^^^^^^^^^^^^^^^^^^^^^^ shows a clock time
  reason: `InvoiceDate` is a date, with no clock time or zone, so `text.format` can show only its weekday, day, month and year.
  next: Use a date format such as `TimeFormat.date_long`, or format an `Instant` or a `ZonedTime` instead (agent)
  rule: type.temporal_format_requires_time
  help: https://jadpo.dev/docs/errors/type.temporal_format_requires_time

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If only the date matters**, use a date format:

```jadpo fragment
    return text.format(day, locale: reader_locale, format: TimeFormat.date_long)
```

```jadpo check project=e-requires-time-date hidden
type InvoiceDate = CalendarDate {}

function invoice_dated(day: InvoiceDate, reader_locale: Locale) -> PresentationText {
    return text.format(day, locale: reader_locale, format: TimeFormat.date_long)
}
```

The text shows the day, month and year, as "27 September 2026" in
`en_gb`.

**If the moment has a time**, keep it as an `Instant` and give the zone to
show it in. `main.jadpo` becomes:

```jadpo check project=e-requires-time-instant
type InvoicedAt = Instant {}

function invoice_dated(
    invoiced_at: InvoicedAt,
    reader_zone: Zone,
    reader_locale: Locale
) -> PresentationText {
    return text.format(
        invoiced_at,
        zone: reader_zone,
        locale: reader_locale,
        format: TimeFormat.date_time_long
    )
}
```

The text now shows the date and the clock time in the reader's zone, as
"27 September 2026 at 14:30:05 BST" in `en_gb` for London. Callers now
pass a moment and a zone rather than a date.

## Related

- [Time types](../language/10-configuration-and-time.md#time-types)
- [Built-in formats](../language/10-configuration-and-time.md#built-in-formats)
- [`zone:` and `locale:`](../language/10-configuration-and-time.md#zone-and-locale)
