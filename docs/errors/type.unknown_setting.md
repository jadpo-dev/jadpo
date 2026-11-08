# `type.unknown_setting`

A block holds a setting it doesn't have, often a misspelling of one it does, such as `maxlength` for `max_length`.

## Why Jadpo refuses it

Each block has a fixed set of settings, so a misspelling can't be quietly
ignored. A `maxlength` that did nothing would leave the text with no limit
at all, with nothing to say so.

## What it looks like

main.jadpo:

```jadpo check project=e-unknown-setting expect=type.unknown_setting
type ProductName = Text {
    min_length: 1
    maxlength: 120
}
```

```text output
error: `maxlength` isn't a setting Jadpo knows
 --> main.jadpo:3:5
  |
3 |     maxlength: 120
  |     ^^^^^^^^^ unknown setting
  reason: A type's block holds the constraints `min`, `max`, `min_length`, `max_length`, `pattern`, `format`, `scale`, `version`, `currency`, `rounding`, `from`, `to`, `dimensions` and `distance`. Did you mean `max_length`?
  next: Use `max_length` instead of `maxlength` (agent)
  rule: type.unknown_setting
  help: https://jadpo.dev/docs/errors/type.unknown_setting

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`personal:` is a setting of a field where Jadpo keeps or sends its value:
an entity's, an event's or an email's, or an object's, an input's or an
output's that a package declares. An app's own type holds what a marked
field gave it, and the value is followed back to that field, so it is
refused there.

main.jadpo:

```jadpo check project=e-unknown-setting-personal expect=type.unknown_setting
/// A card shown to someone.
type Card = Output {
    name: Text {
        personal: PersonalData.identifying
    }
}
```

```text output
error: `personal` isn't a setting of `Card`'s fields
 --> main.jadpo:4:9
  |
4 |         personal: PersonalData.identifying
  |         ^^^^^^^^ unknown setting
  reason: `personal:` marks a field of an entity, an event or an email, or of an object, input or output a package declares, where Jadpo keeps or sends its value. `Card` is none of these: a value in it is followed back to the marked field it came from, so it needs no mark of its own.
  next: Remove `personal`, and mark the field its value is read from (agent)
  rule: type.unknown_setting
  help: https://jadpo.dev/docs/errors/type.unknown_setting

check failed: 1 error, 1 file
```

## How to fix it

**If the setting is misspelt**, use the name the error gives. `main.jadpo`
becomes:

```jadpo check project=e-unknown-setting-fix-1
type ProductName = Text {
    min_length: 1
    max_length: 120
}
```

A `ProductName` over 120 characters is now refused when one is made.

**If it marks a field as personal on your own type**, remove the setting.
`main.jadpo` becomes:

```jadpo check project=e-unknown-setting-personal-fix
/// A card shown to someone.
type Card = Output {
    name: Text
}
```

Mark the field the value is read from, on its entity, instead. A value
read from that field into a `Card` is followed back to it.

## Related

- [Constraints](../language/02-values-and-types.md#constraints)
- [Marking personal fields](../language/15-personal-data.md#marking-personal-fields)
