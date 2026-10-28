# `type.temporal_format_requires_time`

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
  help: docs/errors/type.temporal_format_requires_time.md

check failed: 1 error, 1 file
```
