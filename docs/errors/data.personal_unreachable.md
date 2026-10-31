# `data.personal_unreachable`

A field marked as personal data is on an entity that isn't a person and doesn't refer to one, so erasing a person could never find it.

## Why Jadpo refuses it

Erasing a person, and exporting what is held about them, reach the
records that refer to them, directly or through records that do. Here a
`LossReport` holds a passenger's name and email, both marked
`personal:`, but it isn't a person and refers to no one. A passenger who
asked to be forgotten would stay in every report they made. Taking
`personal:` off would silence the error and leave the data where nothing
erases it.

Jadpo won't choose for you: who the data is about, and so what erasing
them reaches, is a decision about your users.

## What it looks like

main.jadpo:

```jadpo check project=e-personal-unreachable expect=data.personal_unreachable
type PassengerName = Text {
    max_length: 120
}

/// A loss a passenger reported.
entity LossReport {
    id: Uuid
    passenger_name: PassengerName {
        personal: PersonalData.identifying
    }
    passenger_email: Email {
        personal: PersonalData.contact
    }
}
```

```text output
error: Erasing a person can't find `LossReport.passenger_name`
 --> main.jadpo:8:5
   |
 8 |     passenger_name: PassengerName {
   |     ^^^^^^^^^^^^^^ personal data that belongs to no person
11 |     passenger_email: Email {
   |     --------------- personal data too
  reason: Erasing or exporting a person reaches the records that refer to them, directly or through records that do. `LossReport` isn't a person (`person: true`), and none of its references leads to one, so nothing could ever erase or export what it holds.
  next: Refer to the person `LossReport.passenger_name` is about, or mark `LossReport` `person: true` if each record is one (human)
  rule: data.personal_unreachable
  help: https://jadpo.dev/docs/errors/data.personal_unreachable

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whose data a report is.

## How to fix it

The question is who the report's personal data is about.

**If a report is all you hold of the person who made it**, mark the
entity as a person:

```jadpo fragment
entity LossReport {
    ...
    person: true
}
```

```jadpo check project=e-personal-unreachable-person hidden
type PassengerName = Text {
    max_length: 120
}

/// A loss a passenger reported.
entity LossReport {
    id: Uuid
    passenger_name: PassengerName {
        personal: PersonalData.identifying
    }
    passenger_email: Email {
        personal: PersonalData.contact
    }

    person: true
}
```

Each report is now a person, so its name and email are that person's
own.

**If the app keeps the passenger already**, such as a `Passenger` who
signs in, refer to them from the report instead:

```jadpo fragment
entity LossReport {
    id: Uuid
    passenger_id: Passenger.id {
        on_delete: OnDelete.cascade
    }
    ...
}
```

```jadpo check project=e-personal-unreachable-passenger hidden
type PassengerName = Text {
    max_length: 120
}

/// Someone who travels with us and signs in.
entity Passenger {
    id: Uuid
    email: Email {
        unique: true
        personal: PersonalData.contact
    }

    person: true
}

/// A loss a passenger reported.
entity LossReport {
    id: Uuid
    passenger_id: Passenger.id {
        on_delete: OnDelete.cascade
    }
    passenger_name: PassengerName {
        personal: PersonalData.identifying
    }
    passenger_email: Email {
        personal: PersonalData.contact
    }
}
```

Each report now belongs to its passenger: the build's manifest lists
`LossReport.passenger_id` under the `person Passenger` node, among the
references a passenger's records belong through.

## Related

- [The person](../language/15-personal-data.md#the-person)
- [What the compiler checks](../language/15-personal-data.md#what-the-compiler-checks)
- [Marking personal fields](../language/15-personal-data.md#marking-personal-fields)
- [References and `on_delete`](../language/05-data.md#references-and-on_delete)
