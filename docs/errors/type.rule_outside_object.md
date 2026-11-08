# `type.rule_outside_object`

A `rule` is written in a type other than an `Object`, an `Input` or an `Output`, such as one built on `Int`.

## Why Jadpo refuses it

A rule relates the fields of one object, so it belongs in the block of an
`Object`, an `Input` or an `Output`, and is checked whenever a value of it
is made. A type built on one value has no fields to relate, and limits its
value with constraints such as `max:`. An entity's records are made by
writes, which have their own conditions. Events, failures, messages and
enums have no rules either.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.rule_outside_object

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the rule limits one value**, use a constraint. `main.jadpo` becomes:

```jadpo check project=e-rule-outside-object-fix-1
type Nights = Int {
    min: 1
    max: 14
}
```

A `Nights` over 14 is now refused when one is made.

**If the rule relates several values**, move it into the object type whose
fields it reads:

```jadpo fragment
type Stay = Object {
    nights: Nights
    free_nights: Nights
    rule free_within_stay: free_nights <= nights
}
```

```jadpo check project=e-rule-outside-object-fix-2 hidden
type Nights = Int {
    min: 1
}

type Stay = Object {
    nights: Nights
    free_nights: Nights
    rule free_within_stay: free_nights <= nights
}
```

Every `Stay` now has to follow the rule, wherever it is made.

## Related

- [Rules across fields](../language/02-values-and-types.md#rules-across-fields)
- [Constraints](../language/02-values-and-types.md#constraints)
