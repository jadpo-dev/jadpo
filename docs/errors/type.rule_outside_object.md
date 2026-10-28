# `type.rule_outside_object`

Only an `Object`, `Input` or `Output` names rules across its fields: a
refinement's rules are its constraints, and an entity's records are made
by writes, which have their own conditions (area 6):

```jadpo check project=e-rule-outside-object expect=type.rule_outside_object
type Nights = Int {
    min: 1
    rule short_stay: Nights <= 14
}
```

```text output
error: Only an `Object`, `Input` or `Output` names rules across its fields
 --> main.jadpo:3:5
  |
3 |     rule short_stay: Nights <= 14
  |     ^^^^^^^^^^^^^^^ in a type built on `Int`
  reason: `Nights`, built on `Int`, can't hold a rule: a rule relates the fields of one object, so it is written in the block of an `Object`, `Input` or `Output`, and checked whenever a value of it is made.
  next: Move the rule into the object type whose fields it reads, or remove it (agent)
  rule: type.rule_outside_object
  help: docs/errors/type.rule_outside_object.md

check failed: 1 error, 1 file
```
