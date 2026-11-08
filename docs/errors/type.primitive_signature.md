# `type.primitive_signature`

A function, an action, a query or a constant uses a built-in type such as `Text` directly, where it needs a named type.

## Why Jadpo refuses it

A named type says what a value means and carries its rules, such as a
maximum length. A bare `Text` says neither, so a title, a postcode and a
name would all pass for each other in a call. A function, an action, a
query or a constant is where one part of an app hands a value to another,
so each uses named types.

## What it looks like

main.jadpo:

```jadpo check project=e-primitive expect=type.primitive_signature
type Title = Text {
    max_length: 200
}

function normalise(value: Text) -> Title {
    return Title(value)
}
```

```text output
error: This parameter uses `Text` directly
 --> main.jadpo:5:27
  |
5 | function normalise(value: Text) -> Title {
  |                           ^^^^ a built-in type
  reason: Functions, actions and queries take and return named types, so each value says what it means and carries its rules; `Text` says neither.
  next: Declare a named type built on `Text` and use it here (agent)
  rule: type.primitive_signature
  help: https://jadpo.dev/docs/errors/type.primitive_signature

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the value already follows the rules of a type you
have.

**If it does**, take that type:

```jadpo fragment
function normalise(value: Title) -> Title {
    return value
}
```

```jadpo check project=e-primitive-fix-1 hidden
type Title = Text {
    max_length: 200
}

function normalise(value: Title) -> Title {
    return value
}
```

A caller now has to pass a `Title`, so the value is known to be 200
characters or fewer.

**If it doesn't yet**, declare a named type for what it is, built on the
same type:

```jadpo fragment
type TitleText = Text {}

function normalise(value: TitleText) fails TitleTooLong -> Title {
    return attempt Title(value) {
        max_length: TitleTooLong
    }
}
```

```jadpo check project=e-primitive-fix-2 hidden
type Title = Text {
    max_length: 200
}

type TitleText = Text {}

type TitleTooLong = Failure {
    kind: FailureKind.invalid_value
}

function normalise(value: TitleText) fails TitleTooLong -> Title {
    return attempt Title(value) {
        max_length: TitleTooLong
    }
}
```

The signature now says what the value is, and a value that is too long
gives a failure the caller has to handle.

## Related

- [Signatures use named types](../language/02-values-and-types.md#signatures-use-named-types)
- [Named types](../language/02-values-and-types.md#named-types)
