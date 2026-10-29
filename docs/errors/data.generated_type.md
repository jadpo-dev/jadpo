# `data.generated_type`

```jadpo check project=e-generated-type expect=data.generated_type
entity Note {
    id: Uuid
    created_on: CalendarDate {
        generated: Generated.on_create
    }
}
```

```text output
error: `Note.created_on` can't be generated
 --> main.jadpo:4:9
  |
4 |         generated: Generated.on_create
  |         ^^^^^^^^^ not an `Instant`
  reason: A generated field is a required `Instant` set from the operation's clock, on create or on every change; code never assigns it.
  next: Type `created_on` as `Instant`, or remove `generated:` (agent)
  rule: data.generated_type
  help: docs/errors/data.generated_type.md

check failed: 1 error, 1 file
```
