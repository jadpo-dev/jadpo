# `translation.field_type`

A message's fields are the placeholders of its wording: text, numbers, amounts of money, a plural count, or an enum that picks the wording. A moment is formatted in code and passed in as `PresentationText`.

```jadpo check project=e-tr-field-type file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=e-tr-field-type file=domains/shipping/translations/left_at.jadpo expect=translation.field_type
/// When a parcel left, on the tracking page.
type LeftAt = Message {
    when: Instant
}
```

```text output
error: `LeftAt.when` is typed `Instant`, which a wording can't write
 --> domains/shipping/translations/left_at.jadpo:3:5
  |
3 |     when: Instant
  |     ^^^^ not text, a number, money, a percentage or a choice
  reason: A message's fields are the placeholders of its wording, and a placeholder is text (`Text`, `PresentationText`), a number (`Int`, `Decimal`), an amount of money (`Money`, written `{field, number, currency}`), a percentage (`Percentage`, written `{field, number, percent}`) or a plural count (`PluralCount`), or a type built on one of them; one `enum` field picks which wording is written. Dates and times are formatted in code, with `text.format`, and passed in as `PresentationText`.
  next: Make `when` a `PresentationText` and format the value where the message is called (agent)
  rule: translation.field_type
  help: https://jadpo.dev/docs/errors/translation.field_type

check failed: 1 error, 2 files
```

A failure's `client:` fields are placeholders too, filled by the client app, which formats a date, a time, a number or a duration for its own language. A choice has no such format.

```jadpo check project=e-tr-field-type-failure file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-tr-field-type-failure file=domains/shipping/types/hold_reason.jadpo hidden
type HoldReason = Enum {
    customs
    payment
}
```

```jadpo check project=e-tr-field-type-failure file=domains/shipping/translations/en_gb.jadpo hidden
translations {}
```

```jadpo check project=e-tr-field-type-failure file=domains/shipping/failures/parcel_held.jadpo expect=translation.field_type
use shipping/types { HoldReason }

type ParcelHeld = Failure {
    kind: FailureKind.rejected
    client: {
        reason: HoldReason
    }
}
```

```text output
error: `ParcelHeld.reason` is typed `HoldReason`, which a client can't write into a wording
 --> domains/shipping/failures/parcel_held.jadpo:6:9
  |
6 |         reason: HoldReason
  |         ^^^^^^ not text, a number, a date, a time or a duration
  reason: A failure's `client:` fields are the placeholders of its wording, and the client app fills each one: text as it is, and a number, a date, a time or a duration formatted for its own language. A choice, an object, a list or an optional value has no such format.
  next: Make `reason` text, or send the text in a `PresentationText` field (agent)
  rule: translation.field_type
  help: https://jadpo.dev/docs/errors/translation.field_type

check failed: 1 error, 4 files
```
