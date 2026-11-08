# `data.field_without_json`

A field kept as JSON holds a type with no stored form, such as an event.

## Why Jadpo refuses it

A field whose type is an object, a list, a set, a map or an enum with data
is kept whole in one `jsonb` column, holding its type's JSON. Everything
inside it therefore needs a stored form. An event, a failure, an email, a
message, a prompt, an `Input` and an `Output` are values that pass through
your code and have none, and neither does a map keyed by a type with no
single text form.

## What it looks like

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the record needs to keep what the value says**, hold it in an
`Object` of your own:

```jadpo fragment
/// One parcel that was sent, as it is kept.
type SentParcel = Object {
    label: Tag
}

/// What a parcel's shipments hold.
type Shipments = Object {
    sent: List<SentParcel>
}
```

```jadpo check project=e-field-without-json-fix-1 hidden
type Tag = Text {
    max_length: 20
}

/// One parcel that was sent, as it is kept.
type SentParcel = Object {
    label: Tag
}

/// What a parcel's shipments hold.
type Shipments = Object {
    sent: List<SentParcel>
}

entity Parcel {
    id: Uuid
    shipments: Shipments
}
```

`Parcel.shipments` is now stored as JSON, and the event stays a value your
code emits.

**If one plain value is enough**, keep it in a field of its own:

```jadpo fragment
entity Parcel {
    id: Uuid
    last_label: Tag?
}
```

```jadpo check project=e-field-without-json-fix-2 hidden
type Tag = Text {
    max_length: 20
}

entity Parcel {
    id: Uuid
    last_label: Tag?
}
```

The label now has a column of its own, which queries can read.

## Related

- [Structured values](../language/05-data.md#structured-values)
- [Storage types](../language/05-data.md#storage-types)
