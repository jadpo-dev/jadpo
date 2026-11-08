# `type.setting_value`

A setting is given the wrong kind of value, such as a bare `email` where `TextFormat.email` belongs.

## Why Jadpo refuses it

A setting with a fixed set of options takes an enum value, written
`Type.value`, so the checker knows which list the option comes from and a
misspelling can't pass for a name. A setting for a quantity takes a number
of the right kind.

## What it looks like

main.jadpo:

```jadpo check project=e-setting-value expect=type.setting_value
type ContactEmail = Text {
    format: email
}
```

```text output
error: `format` takes a `TextFormat` value, such as `TextFormat.email`
 --> main.jadpo:2:13
  |
2 |     format: email
  |             ^^^^^ a name here
  reason: `format` is a constraint, and its value must be a `TextFormat` value, such as `TextFormat.email`.
  next: Write `TextFormat.email` (agent)
  rule: type.setting_value
  help: https://jadpo.dev/docs/errors/type.setting_value

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the setting takes one of a fixed set of options**, write the option
in full. `main.jadpo` becomes:

```jadpo check project=e-setting-value-fix
type ContactEmail = Text {
    format: TextFormat.email
}
```

A `ContactEmail` now has to be written as an email address.

## Related

- [Constraints](../language/02-values-and-types.md#constraints)
