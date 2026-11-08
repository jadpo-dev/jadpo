# `type.presentation_text_persistence`

An entity's field holds `PresentationText`, text already formatted for one reader.

## Why Jadpo refuses it

`PresentationText` is text formatted for one reader, in their language and
their zone. Another reader, a change of locale or a correction to the zone
database would show the same moment differently, and the text can't be
read back as a time. So a record keeps the value itself. An entity's field
is refused whether it holds `PresentationText` directly or through a named
type, a list, an object or a variant's data.

## What it looks like

main.jadpo:

```jadpo check project=e-presentation-persist expect=type.presentation_text_persistence
entity Receipt {
    id: Uuid
    issued_label: PresentationText
}
```

```text output
error: `Receipt.issued_label` would store text formatted for a person
 --> main.jadpo:3:19
  |
3 |     issued_label: PresentationText
  |                   ^^^^^^^^^^^^^^^^ `PresentationText`, stored
  reason: `Receipt.issued_label` holds `PresentationText`: text in one reader's language and zone, which another reader, a change of locale or a correction to the zone database would show differently, and which can't be read back as a time. A record keeps the value itself and formats it when it is shown.
  next: Store the `Instant`, `CalendarDate` or `ZonedTime` instead, and format `Receipt.issued_label` in the output that shows it (agent)
  rule: type.presentation_text_persistence
  help: https://jadpo.dev/docs/errors/type.presentation_text_persistence

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the field records a moment**, store the moment, and format it in the
output that shows it:

```jadpo fragment
entity Receipt {
    id: Uuid
    issued_at: Instant
}
```

```jadpo check project=e-presentation-persist-fix hidden
entity Receipt {
    id: Uuid
    issued_at: Instant
}
```

The record now holds the moment itself, and each reader can be shown it in
their own language and zone. A `CalendarDate` or a `ZonedTime` is stored
the same way.

## Related

- [`PresentationText`](../language/10-configuration-and-time.md#presentationtext)
- [Showing dates and times to people](../language/10-configuration-and-time.md#showing-dates-and-times-to-people)
- [Time types](../language/10-configuration-and-time.md#time-types)
