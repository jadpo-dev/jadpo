# `data.field_without_json`

A structured field is stored as its type's JSON, so everything inside it
has a stored form: no event, failure, email, message, prompt, `Input` or
`Output`, and no map keyed by a type with no single text form.

```jadpo check project=e-field-without-json expect=data.field_without_json
type Tag = Text {
    max_length: 20
}

/// A parcel was sent.
type ParcelSent = Event {
    label: Tag
}

/// What a parcel's shipments hold.
type Shipments = Object {
    sent: List<ParcelSent>
}

entity Parcel {
    id: Uuid
    shipments: Shipments
}
```

```text output
error: `Parcel.shipments` can't be stored as JSON
 --> main.jadpo:17:16
   |
17 |     shipments: Shipments
   |                ^^^^^^^^^ `ParcelSent` has no stored form
  reason: A structured field is stored whole as `jsonb` holding its type's JSON, made of values built on the built-in and standard types, enums, and objects made of them, in lists, sets and maps; `Parcel.shipments` holds `ParcelSent`, an event, which is emitted, not written.
  next: Store what `Parcel.shipments` needs in an `Object` of the application's, or in fields of its own (agent)
  rule: data.field_without_json
  help: https://jadpo.dev/docs/errors/data.field_without_json

check failed: 1 error, 1 file
```
