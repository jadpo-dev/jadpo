# `type.field_unproved`

An object literal never fails, so a field value the declared ranges don't
prove follows the field's own rules is built first:

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
  help: docs/language/02-values-and-types.html#type-field-unproved

check failed: 1 error, 1 file
```
