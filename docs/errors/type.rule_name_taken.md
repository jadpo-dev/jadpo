# `type.rule_name_taken`

A rule's name is what an `attempt`'s arm and a failure's report call it,
so no field or other rule of the type shares it, and it is never
`invalid`:

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
