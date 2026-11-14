# `telemetry.personal_data`

A personal field is given as a metric's label or a note's attribute, which would send it to the monitoring service.

## Why Jadpo refuses it

A metric's labels and a note's attributes are the only values from your
app's data that telemetry sends, and they go to the monitoring service as
they are, with nothing hashed. The people a personal field is about never
agreed to it leaving the app that way, and a monitoring service keeps
what it is sent for as long as it likes. So a field marked `personal:`,
or a value of a type that is personal data whatever holds it, such as an
`Email` or a `PersonName`, is never one.

## What it looks like

`count_visit` counts visits by the visitor's plan, and gives the plan
field, which is marked as personal data, as the label:

```jadpo check project=e-telemetry-personal-data expect=telemetry.personal_data
/// What a visitor pays for.
type Plan = Enum {
    free
    paid
}

/// Someone who visits.
entity Visitor {
    id: Uuid
    plan: Plan {
        personal: PersonalData.financial
    }
    person: true
}

/// Visits, by plan.
metric visits = Counter {
    by: [plan]
}

/// Counts a visit by `visitor`.
action count_visit(visitor: Visitor) {
    telemetry.visits.add(1, plan: visitor.plan)
}
```

```text output
error: `telemetry.visits.add` would send `Visitor.plan`, marked `personal:`, as a label
 --> main.jadpo:23:35
   |
23 |     telemetry.visits.add(1, plan: visitor.plan)
   |                                   ^^^^^^^^^^^^ personal data
  reason: A metric's labels and a note's attributes go to the monitoring service as they are, with nothing hashed, so a personal field is never one: the people it is about never agreed to it leaving the app this way.
  next: Give `plan:` something that isn't personal data, or remove it (agent)
  rule: telemetry.personal_data
  help: https://jadpo.dev/docs/errors/telemetry.personal_data

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you want the counts split**, split them by something that isn't
personal data, which tells visits apart without saying anything about a
person:

```jadpo fragment
/// Visits, by whether they came back.
metric visits = Counter {
    by: [returning]
}
```

```jadpo check project=e-telemetry-personal-data-fix hidden
/// Someone who visits.
entity Visitor {
    id: Uuid
    returning: Bool
}

/// Visits, by whether they came back.
metric visits = Counter {
    by: [returning]
}

/// Counts a visit by `visitor`.
action count_visit(visitor: Visitor) {
    telemetry.visits.add(1, returning: visitor.returning)
}
```

The metric now counts visits by whether they came back, which is no
one's personal data.

## Related

- [Custom metrics](../language/16-built-in-services.md#custom-metrics)
- [Telemetry](../language/16-built-in-services.md#telemetry)
- [Personal data](../language/15-personal-data.md)
