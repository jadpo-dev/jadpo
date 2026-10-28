# `type.presentation_text_persistence`

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
  help: docs/language/10-configuration-and-time.html#type-presentation-text-persistence

check failed: 1 error, 1 file
```
