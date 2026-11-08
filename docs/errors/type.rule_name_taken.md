# `type.rule_name_taken`

A `rule` has the name of a field or another rule of its type, or is called `invalid`.

## Why Jadpo refuses it

A rule's name is what an `attempt`'s arm and a failure's report call it,
so `ends:` could only mean one thing if no field or other rule of the type
shares it. `invalid` is taken too, because an `attempt` keeps it for every
rule at once.

## What it looks like

main.jadpo:

```jadpo check project=e-rule-name-taken expect=type.rule_name_taken
type Moment = Instant {}

type Booking = Object {
    starts: Moment
    ends: Moment
    rule ends: ends > starts
}
```

```text output
error: A rule can't be called `ends` here
 --> main.jadpo:6:10
  |
6 |     rule ends: ends > starts
  |          ^^^^ name taken
  reason: `Booking` has a field `ends`; a rule's name is its own, so `ends:` in an `attempt` or a report can only mean one thing.
  next: Give the rule a name of its own that says what it checks (agent)
  rule: type.rule_name_taken
  help: https://jadpo.dev/docs/errors/type.rule_name_taken

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the rule's name is the same as something else in the type**, name the
rule for what it checks:

```jadpo fragment
type Booking = Object {
    ...
    rule ends_after_start: ends > starts
}
```

```jadpo check project=e-rule-name-taken-fix hidden
type Moment = Instant {}

type Booking = Object {
    starts: Moment
    ends: Moment
    rule ends_after_start: ends > starts
}
```

An `attempt` and a failure's report now call it `ends_after_start`.

## Related

- [Rules across fields](../language/02-values-and-types.md#rules-across-fields)
