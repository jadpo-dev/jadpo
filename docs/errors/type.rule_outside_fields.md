# `type.rule_outside_fields`

A rule reads only its type's fields, so every value of the type follows
it wherever it is made, the same in every run: no function of the
application, constant, setting, clock or record:

```jadpo check project=e-rule-outside-fields expect=type.rule_outside_fields
type Moment = Instant {}

type Booking = Object {
    starts: Moment
    ends: Moment
    rule in_opening_hours: opens_by(starts)
}

type Opens = Bool {}

function opens_by(at: Moment) -> Opens {
    return Opens(true)
}
```

```text output
error: A rule can't read `opens_by(starts)`
 --> main.jadpo:6:28
  |
6 |     rule in_opening_hours: opens_by(starts)
  |                            ^^^^^^^^^^^^^^^^ not one of the type's fields
  reason: A rule reads only `Booking`'s own fields, with literals, variants, operators, `match` and the standard operations that give a result from their arguments alone, so every value of `Booking` follows it wherever it is made, the same in every run.
  next: Write the rule over `Booking`'s fields, or check this where the value is built, before building it (agent)
  rule: type.rule_outside_fields
  help: docs/errors/type.rule_outside_fields.md

check failed: 1 error, 1 file
```
