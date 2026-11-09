# `configuration.type_unsupported`

A setting's type can't be read from one environment variable, or a secret setting's type isn't text.

## Why Jadpo refuses it

A setting is read from one environment variable, so its type has to be
something one piece of text can hold. A list holds any number of values and
a variable holds one. A secret also has to be text, because `jadpo test`
makes a throwaway value for each one on every run, and it can't make up a
number or a flag.

## What it looks like

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
  reason: A setting is read from one environment variable, so its type must be decodable from one piece of text: text-based types, `Int`, `Decimal`, `Bool`, `Uuid`, `Email`, `Url`, `IpAddress`, `Duration`, `Instant`, `CalendarDate`, `ModelId`, `Money`, `Certificate` and enums. A list holds any number of values, and a variable holds one; make each value a setting of its own.
  next: Split `admin_emails` into settings of those types (agent)
  rule: configuration.type_unsupported
  help: https://jadpo.dev/docs/errors/configuration.type_unsupported

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A secret that isn't text is refused the same way:

```jadpo check project=e-type-unsupported-secret expect=configuration.type_unsupported
config {
    retry_limit: Int {
        secret: true
    }
}
```

```text output
error: `retry_limit` is secret, and `Int` isn't text
 --> main.jadpo:2:18
  |
2 |     retry_limit: Int {
  |                  ^^^ a secret that isn't text
  reason: A secret is a key, a token or an address, which is text; `jadpo test` generates a throwaway one for every run, and cannot make a number or a flag.
  next: Make `retry_limit` text, or leave out `secret: true` (agent)
  rule: configuration.type_unsupported
  help: https://jadpo.dev/docs/errors/configuration.type_unsupported

check failed: 1 error, 1 file
```

## How to fix it

**If it holds several values**, make each one a setting of its own:

```jadpo check project=e-type-unsupported-split
config {
    billing_admin_email: Email
    support_admin_email: Email
}
```

Each deployment now sets `BILLING_ADMIN_EMAIL` and `SUPPORT_ADMIN_EMAIL`,
and each is checked as an email address when the app starts.

**If a secret isn't text**, leave out `secret: true`:

```jadpo check project=e-type-unsupported-open
config {
    retry_limit: Int
}
```

The setting is now read as a number, and it can appear in logs and answers
like any other value.

## Related

- [What a setting can hold](../language/10-configuration-and-time.md#what-a-setting-can-hold)
- [Secrets](../language/10-configuration-and-time.md#secrets)
