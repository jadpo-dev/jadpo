# `package.standard_candidate`

A provider package's field looks like a standard type the package doesn't use, or like personal data it doesn't mark.

## Why Jadpo refuses it

A package's public types use the standard types, so an app reads an
`Instant` rather than a number to convert, and the package decodes it at the
edge. An imported description often says only `integer`, so the importer
leaves a field as the provider types it. The field is flagged when it looks
like a standard type the package doesn't use: a whole number named like a
time may be an `Instant` sent as seconds, one named like an amount beside a
currency `Money` in minor units, and a latitude beside a longitude a
`Point`.

Text is read the same way: an email's `to` and `from`, or text named like
an email address, a first name, a surname or a phone number, may be an
`Email`, a `GivenName`, a `FamilyName` or a `PhoneNumber`, which are
personal data. Free text a person writes, such as a subject, a body or a
prompt, is marked `personal:`. A field that says how it is sent (`wire:`,
`wire_parts:`, `decode:`) or is marked `personal:` is never flagged. Jadpo
can't tell from a name alone what a field means, so you can decline it,
with a reason.

## What it looks like

`delivered_at` is a whole number named like a time:

```jadpo check project=e72e-candidate file=app.jadpo hidden
application {}
```

```jadpo check project=e72e-candidate file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72e-candidate file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
}
```

`shared/providers/acme_couriers/v1/types/delivery.jadpo`:

```jadpo check project=e72e-candidate file=shared/providers/acme_couriers/v1/types/delivery.jadpo expect=package.standard_candidate
/// A parcel delivered.
public type Delivery = Object {
    id: Text
    delivered_at: Int
}
```

```text output
error: `Delivery.delivered_at` may be an `Instant` sent as seconds
 --> shared/providers/acme_couriers/v1/types/delivery.jadpo:4:5
  |
4 |     delivered_at: Int
  |     ^^^^^^^^^^^^ a whole number named like a time
  reason: `delivered_at` is a whole number named like a time, which providers usually send as seconds since 1970. A package's public types use the standard types, so an app reads an `Instant` rather than a number to convert, and the package decodes it at the edge.
  next: Check the provider's documentation, then make it `delivered_at: Instant { wire: Wire.unix_seconds }`, or `Wire.unix_milliseconds` for milliseconds, or write `decline package.standard_candidate: "<why>"` above it if this is deliberate (agent)
  rule: package.standard_candidate
  help: https://jadpo.dev/docs/errors/package.standard_candidate

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.


Text is flagged by its name. In an email's object, one with a `subject`,
`cc` or `bcc`, the sender and the recipients are email addresses; a
recipients' list here is plain text:

```jadpo check project=e72e-candidate-text file=app.jadpo hidden
application {}
```

```jadpo check project=e72e-candidate-text file=shared/providers/acme_mail/v1/package.jadpo hidden
/// Acme Mail's sending API.
package AcmeMail.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72e-candidate-text file=shared/providers/acme_mail/v1/provider.jadpo hidden
/// Acme Mail's sending API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-mail.example"
    }
}
```

`shared/providers/acme_mail/v1/types/outgoing_email.jadpo`:

```jadpo check project=e72e-candidate-text file=shared/providers/acme_mail/v1/types/outgoing_email.jadpo expect=package.standard_candidate
/// One email to send.
public type OutgoingEmail = Object {
    from: Email
    to: List<Text>
    subject: Text {
        personal: PersonalData.identifying
    }
}
```

```text output
error: `OutgoingEmail.to` may be an `Email`
 --> shared/providers/acme_mail/v1/types/outgoing_email.jadpo:4:5
  |
4 |     to: List<Text>
  |     ^^ text naming who an email is from or to
  reason: `to` is text beside an email's subject or copies, so it names who the email is from or to: an email address, which is personal data. A package's public types use the standard types, so an app gives one checked as what it is, and the package's facts count it as the personal data it is.
  next: Check the provider's documentation, then make it `to: List<Email>`, or `List<EmailWithName>` where the provider shows a name with the address, with `decode:` and `encode:` if it sends both as one text (`Mary Somerville <mary@example.com>`), or write `decline package.standard_candidate: "<why>"` above it if this is deliberate (agent)
  rule: package.standard_candidate
  help: https://jadpo.dev/docs/errors/package.standard_candidate

check failed: 1 error, 4 files
```

## How to fix it

**If the provider's documentation says it is the standard type**, use it,
and say how it is sent:

```jadpo fragment
public type Delivery = Object {
    ...
    delivered_at: Instant {
        wire: Wire.unix_seconds
    }
}
```

```jadpo check project=e72e-candidate-fix file=shared/providers/acme_couriers/v1/types/delivery.jadpo hidden
/// A parcel delivered.
public type Delivery = Object {
    id: Text
    delivered_at: Instant {
        wire: Wire.unix_seconds
    }
}
```

```jadpo check project=e72e-candidate-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e72e-candidate-fix file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72e-candidate-fix file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
}
```

The package reads the number as an `Instant` at the edge, and an app gets a
time.

**If the field really means something else**, keep its type and say why:

```jadpo check project=e72e-candidate-decline file=shared/providers/acme_couriers/v1/types/delivery.jadpo
/// A parcel delivered.
public type Delivery = Object {
    id: Text
    decline package.standard_candidate: "The courier numbers its collection slots 1 to 6; this is that number"
    delivered_at: Int
}
```

```jadpo check project=e72e-candidate-decline file=app.jadpo hidden
application {}
```

```jadpo check project=e72e-candidate-decline file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72e-candidate-decline file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
}
```

The field stays a number, and `jadpo declines list` shows the reason.

**If a text field names an email address**, make it the standard type:

```jadpo check project=e72e-candidate-text-fix file=shared/providers/acme_mail/v1/types/outgoing_email.jadpo
/// One email to send.
public type OutgoingEmail = Object {
    from: Email
    to: List<Email>
    subject: Text {
        personal: PersonalData.identifying
    }
}
```

```jadpo check project=e72e-candidate-text-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e72e-candidate-text-fix file=shared/providers/acme_mail/v1/package.jadpo hidden
/// Acme Mail's sending API.
package AcmeMail.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72e-candidate-text-fix file=shared/providers/acme_mail/v1/provider.jadpo hidden
/// Acme Mail's sending API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-mail.example"
    }
}
```

The package's facts now count each recipient as the personal data it is.

## Related

- [Mapping imported types to standard types](../language/11-integrations.md#mapping-imported-types-to-standard-types)
- [The built-in encodings: `wire:`](../language/11-integrations.md#the-built-in-encodings-wire)
- [Writing a decline](../language/01-project-layout-and-modules.md#writing-a-decline)
