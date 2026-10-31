# `type.temporal_zone_required`

Your code formats an `Instant` for people to read without saying which time zone to show it in.

## Why Jadpo refuses it

An `Instant` is one moment for everyone. Its date and clock time depend
on where it is read: late evening in London is already the next morning
in Tokyo. The server's own zone is never used, because it says nothing
about the reader, so `zone:` says which zone to show the moment in.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.temporal_zone_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If readers are in different zones**, take the reader's zone as a
parameter and give it as `zone:`. `main.jadpo` becomes:

```jadpo check project=e-zone-required-reader
type OrderedAt = Instant {}

function placed_label(
    ordered_at: OrderedAt,
    reader_zone: Zone,
    reader_locale: Locale
) -> PresentationText {
    return text.format(
        ordered_at,
        zone: reader_zone,
        locale: reader_locale,
        format: TimeFormat.date_long
    )
}
```

Each reader sees the date as it was where they are.

**If every reader is in one place**, such as the customers of a shop in
London, give that zone. `main.jadpo` becomes:

```jadpo check project=e-zone-required-fixed
type OrderedAt = Instant {}

function placed_label(ordered_at: OrderedAt, reader_locale: Locale) -> PresentationText {
    return text.format(
        ordered_at,
        zone: Zone.europe_london,
        locale: reader_locale,
        format: TimeFormat.date_long
    )
}
```

Every reader sees the date as it was in London, wherever they are.

## Related

- [`zone:` and `locale:`](../language/10-configuration-and-time.md#zone-and-locale)
- [Time types](../language/10-configuration-and-time.md#time-types)
- [Showing dates and times to people](../language/10-configuration-and-time.md#showing-dates-and-times-to-people)
