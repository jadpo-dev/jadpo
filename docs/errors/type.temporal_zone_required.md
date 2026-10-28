# `type.temporal_zone_required`

```jadpo check project=e-zone-required expect=type.temporal_zone_required
type OrderedAt = Instant {}

function placed_label(ordered_at: OrderedAt, reader_locale: Locale) -> PresentationText {
    return text.format(ordered_at, locale: reader_locale, format: TimeFormat.date_long)
}
```

```text output
error: `text.format` needs the zone to show `OrderedAt` in
 --> main.jadpo:4:12
  |
4 |     return text.format(ordered_at, locale: reader_locale, format: TimeFormat.date_long)
  |            ^^^^^^^^^^^ no `zone:`
  reason: An `Instant` is one moment for everyone, and its date and clock time depend on where it is read; the server never guesses a zone, so `zone:` says which.
  next: Add `zone: ...` with the reader's zone, such as `zone: Zone.europe_london` (agent)
  rule: type.temporal_zone_required
  help: docs/errors/type.temporal_zone_required.md

check failed: 1 error, 1 file
```
