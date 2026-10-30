# `configuration.type_unsupported`

```jadpo check project=e-type-unsupported expect=configuration.type_unsupported
config {
    admin_emails: List<Email>
}
```

```text output
error: `admin_emails` can't be a `List<Email>`
 --> main.jadpo:2:19
  |
2 |     admin_emails: List<Email>
  |                   ^^^^^^^^^^^ not read from one variable
  reason: A setting is read from one environment variable, so its type must be decodable from one piece of text: text-based types, `Int`, `Decimal`, `Bool`, `Uuid`, `Email`, `Url`, `IpAddress`, `Duration`, `Instant`, `CalendarDate`, `ModelId`, `Money`, `Certificate` and enums. A list is one value in many variables or one variable holding a format; split it into settings.
  next: Split `admin_emails` into settings of those types (agent)
  rule: configuration.type_unsupported
  help: https://jadpo.dev/docs/errors/configuration.type_unsupported

check failed: 1 error, 1 file
```
