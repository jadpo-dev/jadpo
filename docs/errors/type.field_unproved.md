# `type.field_unproved`

An object is built with a field value that might break a rule the field adds, such as a value that can be longer than the field's `max_length`.

## Why Jadpo refuses it

Building an object never fails, so every field has to be given a value
already known to follow the field's rules. A value of a looser type, such
as an 80-character name going into a 12-character badge, might not. Jadpo
won't cut the text or reject the call for you, because what should happen
to a name that is too long is a choice about your app.

## What it looks like

main.jadpo:

```jadpo check project=e-field-unproved expect=type.field_unproved
type Profile = Object {
    display_name: Text {
        max_length: 80
    }
}

type Badge = Object {
    name: Text {
        max_length: 12
    }
}

function make_badge(profile: Profile) -> Badge {
    return Badge {
        name: profile.display_name
    }
}
```

```text output
error: `profile.display_name` might not fit `Badge.name`
 --> main.jadpo:15:15
   |
15 |         name: profile.display_name
   |               ^^^^^^^^^^^^^^^^^^^^ can break `max_length: 12`
  reason: An object literal never fails, so each field gets a value already known to fit; neither the declared ranges nor a condition before it prove this one follows `max_length: 12`, which `Badge.name` adds.
  next: Build it first on its own line, `var name: Badge.name = attempt Badge.name(profile.display_name) { max_length: SomeFailure }`, then write `name: name` in the object (agent)
  rule: type.field_unproved
  help: https://jadpo.dev/docs/errors/type.field_unproved

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is what should happen when the value doesn't fit.

**If a value that doesn't fit is a failure**, build the field's value on
its own line with `attempt`, and give the object the result:

```jadpo fragment
type BadgeNameTooLong = Failure {
    kind: FailureKind.invalid_value
}

function make_badge(profile: Profile) fails BadgeNameTooLong -> Badge {
    var name: Badge.name = attempt Badge.name(profile.display_name) {
        max_length: BadgeNameTooLong
    }
    return Badge {
        name: name
    }
}
```

```jadpo check project=e-field-unproved-fix-1 hidden
type Profile = Object {
    display_name: Text {
        max_length: 80
    }
}

type Badge = Object {
    name: Text {
        max_length: 12
    }
}

type BadgeNameTooLong = Failure {
    kind: FailureKind.invalid_value
}

function make_badge(profile: Profile) fails BadgeNameTooLong -> Badge {
    var name: Badge.name = attempt Badge.name(profile.display_name) {
        max_length: BadgeNameTooLong
    }
    return Badge {
        name: name
    }
}
```

A display name longer than 12 characters now makes `make_badge` fail with
`BadgeNameTooLong`, which its callers handle.

**If the source can never be too long**, give it the same rule, so the
value is known to fit:

profile.jadpo:

```jadpo check project=e-field-unproved-fix-2 file=profile.jadpo
type Profile = Object {
    display_name: Text {
        max_length: 12
    }
}
```

```jadpo check project=e-field-unproved-fix-2 file=main.jadpo hidden
type Badge = Object {
    name: Text {
        max_length: 12
    }
}

function make_badge(profile: Profile) -> Badge {
    return Badge {
        name: profile.display_name
    }
}
```

A profile's display name is now at most 12 characters, so building one
with a longer name fails where the profile is made.

## Related

- [Creating values](../language/02-values-and-types.md#creating-values)
- [Construction failures by rule](../language/02-values-and-types.md#construction-failures-by-rule)
- [Conditions as proof](../language/02-values-and-types.md#conditions-as-proof)
