# `type.optional_outside_input`

A field has `optional:` or `default:` in a type that isn't an `Input`.

## Why Jadpo refuses it

An `Input` describes a request arriving from outside, which may leave a
field out or have Jadpo fill it in. An object built inside your app states
every field each time, so `optional:` and `default:` have nothing to do
there, and nobody reading the code has to wonder what a missing field
would hold.

## What it looks like

main.jadpo:

```jadpo check project=e-optional expect=type.optional_outside_input
type Nickname = Text {
    max_length: 30
}

type Profile = Object {
    nickname: Nickname {
        optional: true
    }
}
```

```text output
error: `Profile` isn't an `Input`, so its fields are always given
 --> main.jadpo:7:9
  |
7 |         optional: true
  |         ^^^^^^^^ only for an `Input`
  reason: Only a request arriving from outside may leave fields out or have them filled by default; an object built inside the application states every field, using `none` for a field declared `T?`.
  next: Remove the setting, or make the field's type `T?` if it may be absent (agent)
  rule: type.optional_outside_input
  help: https://jadpo.dev/docs/errors/type.optional_outside_input

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the field may have no value**, put `?` on its type, and write `none`
where a value is built without one:

```jadpo fragment
type Profile = Object {
    nickname: Nickname?
}
```

```jadpo check project=e-optional-fix-1 hidden
type Nickname = Text {
    max_length: 30
}

type Profile = Object {
    nickname: Nickname?
}
```

Every `Profile` now states its `nickname`, as a `Nickname` or as `none`.

**If the type is what a request sends**, make it an `Input`:

```jadpo fragment
type Profile = Input {
    nickname: Nickname {
        optional: true
    }
}
```

```jadpo check project=e-optional-fix-2 hidden
type Nickname = Text {
    max_length: 30
}

type Profile = Input {
    nickname: Nickname {
        optional: true
    }
}
```

A request can now leave `nickname` out.

## Related

- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)
- [Objects](../language/02-values-and-types.md#objects)
