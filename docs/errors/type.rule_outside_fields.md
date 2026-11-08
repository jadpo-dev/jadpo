# `type.rule_outside_fields`

A `rule` reads something other than its type's own fields, such as a function of your app, a constant, a setting or the clock.

## Why Jadpo refuses it

A rule says what every value of its type is like, wherever the value is
made and in every run. So it reads only the type's own fields, with
literals, variants, operators, `match` and the standard operations that
give a result from their arguments alone. A function of your app, a
constant, a setting, the clock or a record could change, and values made
earlier would no longer follow the rule.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.rule_outside_fields

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the condition can be said from the fields alone.

**If it can**, write it over the fields:

```jadpo fragment
type Booking = Object {
    ...
    rule ends_after_start: ends > starts
}
```

```jadpo check project=e-rule-outside-fields-fix-1 hidden
type Moment = Instant {}

type Booking = Object {
    starts: Moment
    ends: Moment
    rule ends_after_start: ends > starts
}
```

Every `Booking` now has to end after it starts, wherever it is made.

**If it needs something else**, remove the rule and check the condition
where the value is built:

```jadpo fragment
action book(starts: Moment, ends: Moment) fails ClosedThen -> Booking {
    if (not opens_by(starts)) {
        reject ClosedThen
    }
    return Booking {
        starts: starts
        ends: ends
    }
}
```

```jadpo check project=e-rule-outside-fields-fix-2 hidden
type Moment = Instant {}

type Booking = Object {
    starts: Moment
    ends: Moment
}

type Opens = Bool {}

type ClosedThen = Failure {
    kind: FailureKind.invalid_value
}

function opens_by(at: Moment) -> Opens {
    return Opens(true)
}

action book(starts: Moment, ends: Moment) fails ClosedThen -> Booking {
    if (not opens_by(starts)) {
        reject ClosedThen
    }
    return Booking {
        starts: starts
        ends: ends
    }
}
```

`book` now gives `ClosedThen` for a time the function refuses, and a
`Booking` made anywhere else isn't checked.

## Related

- [Rules across fields](../language/02-values-and-types.md#rules-across-fields)
