# `package.standard_candidate`

> This flags, in a provider package's public object, a field the
> provider sends in its own shape that looks like a standard type the
> package doesn't use ("Mapping imported types to standard types"). A whole number named like a time (`created`, `expires`,
> `paid_at`) may be an `Instant` sent as seconds; one named like an amount
> (`amount`, `amount_refunded`, `total_amount`) beside a currency, `Money`
> in minor units; a latitude beside a longitude, a `Point`. Text is read
> the same way: an email's `to` and `from`, or text named like an email
> address, a first name, a surname or a phone number, may be `Email`,
> `GivenName`, `FamilyName` or `PhoneNumber`, which are personal data; free
> text a person writes, such as an email's subject and body or a prompt, is
> marked `personal:`. A field that says how it is sent (`wire:`,
> `wire_parts:`, `decode:`) or is marked `personal:` is never one, and an
> object that is a standard type whole is `type.standard_exists` instead.
> It is declinable: a field that means something else keeps its type with a
> reason on the field.

An imported description often says only `integer`, so the importer leaves
the field as the provider types it and the error asks for the
documentation to be checked:

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

Swapping it reads the provider's number as the `Instant` it is; a field
that really is a number keeps its type with a reason, which `jadpo
declines list` shows:

```jadpo check project=e72e-candidate-declined file=app.jadpo hidden
application {}
```

```jadpo check project=e72e-candidate-declined file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72e-candidate-declined file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
}
```

```jadpo check project=e72e-candidate-declined file=shared/providers/acme_couriers/v1/types/delivery.jadpo
/// A parcel delivered.
public type Delivery = Object {
    id: Text
    delivered_at: Instant {
        wire: Wire.unix_seconds
    }
    decline package.standard_candidate: "The courier numbers its collection slots 1 to 6; this is that number"
    slot_time: Int
}
```

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

The subject and the body of an email, a message's content and a prompt are
free text a person may write, so each is flagged until it is marked
`personal:`, as `subject` is here. Swapped and marked, the package's facts
list each as the personal data it sends:

```jadpo check project=e72e-candidate-text-fixed file=app.jadpo hidden
application {}
```

```jadpo check project=e72e-candidate-text-fixed file=shared/providers/acme_mail/v1/package.jadpo hidden
/// Acme Mail's sending API.
package AcmeMail.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72e-candidate-text-fixed file=shared/providers/acme_mail/v1/provider.jadpo hidden
/// Acme Mail's sending API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-mail.example"
    }
}
```

```jadpo check project=e72e-candidate-text-fixed file=shared/providers/acme_mail/v1/types/outgoing_email.jadpo
/// One email to send.
public type OutgoingEmail = Object {
    from: Email
    to: List<Email>
    subject: Text {
        personal: PersonalData.identifying
    }
}
```
