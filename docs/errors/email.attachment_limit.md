# `email.attachment_limit`

An email's attachments may add up to more than its mail package accepts.

## Why Jadpo refuses it

A mail package says how much it accepts in all, in its `limits: { attachments: }`,
and each `File` field says the most it may hold. If those could add up to more
than the package accepts, the largest email would be refused when it was
sent, at a time when no one is there to fix it.

## What it looks like

`Postbox` accepts 10 MB of attachments in all, and the invoice may be 12 MB.

```jadpo check project=e-email-attachment-limit file=app.jadpo hidden
application {
    display_name: "Notes"
    connections: {
        postbox: {
            package: Postbox.v1
            credential: config.postbox_token
            host: config.postbox_host
        }
    }
    email: {
        connection: postbox
        from: "hello@notes.example"
    }
    events: {
        store: main
    }
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-email-attachment-limit file=config.jadpo hidden
config {
    postbox_host: Url
    postbox_token: Text {
        secret: true
    }
}
```

```jadpo check project=e-email-attachment-limit file=shared/providers/postbox/v1/package.jadpo hidden
/// Postbox: sends the app's email.
package Postbox.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-email-attachment-limit file=shared/providers/postbox/v1/provider.jadpo hidden
/// Postbox's JSON API.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.bearer
}
```

```jadpo check project=e-email-attachment-limit file=shared/providers/postbox/v1/resources/transmissions.jadpo hidden
public resource transmissions {
    send: {
        method: HttpMethod.post
        path: "/emails"
        input: OutgoingEmail
        changes: true
        source: PackageSource.written
    }
}
```

```jadpo check project=e-email-attachment-limit file=shared/providers/postbox/v1/types/outgoing_email.jadpo hidden
/// One email to send.
public type OutgoingEmail = Object {
    from: Text {
        personal: PersonalData.contact
    }
    to: Text {
        personal: PersonalData.contact
    }
    subject: Text {
        personal: PersonalData.identifying
    }
    html: Text? {
        personal: PersonalData.identifying
    }
    text: Text? {
        personal: PersonalData.identifying
    }
    attachments: List<OutgoingAttachment>
}
```

```jadpo check project=e-email-attachment-limit file=shared/providers/postbox/v1/types/outgoing_attachment.jadpo hidden
/// One file sent with an email.
public type OutgoingAttachment = Object {
    filename: Text
    content: Bytes
}
```

`shared/providers/postbox/v1/provides/email.jadpo`:

```jadpo check project=e-email-attachment-limit file=shared/providers/postbox/v1/provides/email.jadpo
/// Sends each email through `transmissions.send`.
provide email {
    send: {
        call: transmissions.send
        input: {
            from: message.from
            to: message.to
            subject: message.subject
            html: message.html
            text: message.text
            attachments: {
                each: message.attachments
                as: {
                    filename: item.name
                    content: item.content
                }
            }
        }
    }
    limits: {
        attachments: 10mb
    }
}
```

```jadpo check project=e-email-attachment-limit file=domains/billing/translations/en_gb.jadpo hidden
translations {
    invoice_email: "Your invoice, {name}"
}
```

```jadpo check project=e-email-attachment-limit file=domains/billing/emails/invoice_email.txt hidden
Hello {{ name }}, your invoice is attached.
```

`domains/billing/emails/invoice_email.jadpo`:

```jadpo check project=e-email-attachment-limit file=domains/billing/emails/invoice_email.jadpo expect=email.attachment_limit
/// Sends a customer their invoice.
type InvoiceEmail = EmailMessage {
    name: Text
    invoice: File {
        max_size: 12mb
    }
}
```

```text output
error: `InvoiceEmail`'s attachments may be 12 MB, over the 10 MB `postbox` accepts
 --> domains/billing/emails/invoice_email.jadpo:2:6
  |
2 | type InvoiceEmail = EmailMessage {
  |      ^^^^^^^^^^^^ too large to send
  reason: An email's `File` fields are sent as attachments, each at most its `max_size:`, and `postbox`'s package accepts 10 MB of them in all, so an email at its largest would be refused.
  next: Lower the `max_size:` of `InvoiceEmail`'s `File` fields to 10 MB in all, or send the biggest files as links (agent)
  rule: email.attachment_limit
  help: https://jadpo.dev/docs/errors/email.attachment_limit

check failed: 1 error, 10 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Each `File` field has to say how large it may be, or Jadpo can't add them up:

```jadpo check project=e-email-attachment-limit-size file=app.jadpo hidden
application {
    display_name: "Notes"
    connections: {
        postbox: {
            package: Postbox.v1
            credential: config.postbox_token
            host: config.postbox_host
        }
    }
    email: {
        connection: postbox
        from: "hello@notes.example"
    }
    events: {
        store: main
    }
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-email-attachment-limit-size file=config.jadpo hidden
config {
    postbox_host: Url
    postbox_token: Text {
        secret: true
    }
}
```

```jadpo check project=e-email-attachment-limit-size file=shared/providers/postbox/v1/package.jadpo hidden
/// Postbox: sends the app's email.
package Postbox.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-email-attachment-limit-size file=shared/providers/postbox/v1/provider.jadpo hidden
/// Postbox's JSON API.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.bearer
}
```

```jadpo check project=e-email-attachment-limit-size file=shared/providers/postbox/v1/resources/transmissions.jadpo hidden
public resource transmissions {
    send: {
        method: HttpMethod.post
        path: "/emails"
        input: OutgoingEmail
        changes: true
        source: PackageSource.written
    }
}
```

```jadpo check project=e-email-attachment-limit-size file=shared/providers/postbox/v1/types/outgoing_email.jadpo hidden
/// One email to send.
public type OutgoingEmail = Object {
    from: Text {
        personal: PersonalData.contact
    }
    to: Text {
        personal: PersonalData.contact
    }
    subject: Text {
        personal: PersonalData.identifying
    }
    html: Text? {
        personal: PersonalData.identifying
    }
    text: Text? {
        personal: PersonalData.identifying
    }
    attachments: List<OutgoingAttachment>
}
```

```jadpo check project=e-email-attachment-limit-size file=shared/providers/postbox/v1/types/outgoing_attachment.jadpo hidden
/// One file sent with an email.
public type OutgoingAttachment = Object {
    filename: Text
    content: Bytes
}
```

```jadpo check project=e-email-attachment-limit-size file=shared/providers/postbox/v1/provides/email.jadpo hidden
/// Sends each email through `transmissions.send`.
provide email {
    send: {
        call: transmissions.send
        input: {
            from: message.from
            to: message.to
            subject: message.subject
            html: message.html
            text: message.text
            attachments: {
                each: message.attachments
                as: {
                    filename: item.name
                    content: item.content
                }
            }
        }
    }
    limits: {
        attachments: 10mb
    }
}
```

```jadpo check project=e-email-attachment-limit-size file=domains/billing/translations/en_gb.jadpo hidden
translations {
    invoice_email: "Your invoice, {name}"
}
```

```jadpo check project=e-email-attachment-limit-size file=domains/billing/emails/invoice_email.txt hidden
Hello {{ name }}, your invoice is attached.
```

`domains/billing/emails/invoice_email.jadpo`:

```jadpo check project=e-email-attachment-limit-size file=domains/billing/emails/invoice_email.jadpo expect=email.attachment_limit
/// Sends a customer their invoice.
type InvoiceEmail = EmailMessage {
    name: Text
    invoice: File
}
```

```text output
error: `InvoiceEmail.invoice` doesn't say how large it may be
 --> domains/billing/emails/invoice_email.jadpo:4:5
  |
4 |     invoice: File
  |     ^^^^^^^ no `max_size:`
  reason: An email's `File` fields are sent as attachments, and `postbox` accepts 10 MB of them in all, so each says the most it may hold, and their sum fits.
  next: Give `invoice` a `max_size:`, such as `invoice: File { max_size: 5mb }`, keeping the email's total within 10 MB (agent)
  rule: email.attachment_limit
  help: https://jadpo.dev/docs/errors/email.attachment_limit

check failed: 1 error, 10 files
```

## How to fix it

**If the files are never that large**, lower the `max_size:` so the fields
fit. In `domains/billing/emails/invoice_email.jadpo`:

```jadpo fragment
    invoice: File {
        max_size: 8mb
    }
```

```jadpo check project=e-email-attachment-limit-fix file=domains/billing/emails/invoice_email.jadpo hidden
/// Sends a customer their invoice.
type InvoiceEmail = EmailMessage {
    name: Text
    invoice: File {
        max_size: 8mb
    }
}
```

```jadpo check project=e-email-attachment-limit-fix file=app.jadpo hidden
application {
    display_name: "Notes"
    connections: {
        postbox: {
            package: Postbox.v1
            credential: config.postbox_token
            host: config.postbox_host
        }
    }
    email: {
        connection: postbox
        from: "hello@notes.example"
    }
    events: {
        store: main
    }
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-email-attachment-limit-fix file=config.jadpo hidden
config {
    postbox_host: Url
    postbox_token: Text {
        secret: true
    }
}
```

```jadpo check project=e-email-attachment-limit-fix file=shared/providers/postbox/v1/package.jadpo hidden
/// Postbox: sends the app's email.
package Postbox.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-email-attachment-limit-fix file=shared/providers/postbox/v1/provider.jadpo hidden
/// Postbox's JSON API.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.bearer
}
```

```jadpo check project=e-email-attachment-limit-fix file=shared/providers/postbox/v1/resources/transmissions.jadpo hidden
public resource transmissions {
    send: {
        method: HttpMethod.post
        path: "/emails"
        input: OutgoingEmail
        changes: true
        source: PackageSource.written
    }
}
```

```jadpo check project=e-email-attachment-limit-fix file=shared/providers/postbox/v1/types/outgoing_email.jadpo hidden
/// One email to send.
public type OutgoingEmail = Object {
    from: Text {
        personal: PersonalData.contact
    }
    to: Text {
        personal: PersonalData.contact
    }
    subject: Text {
        personal: PersonalData.identifying
    }
    html: Text? {
        personal: PersonalData.identifying
    }
    text: Text? {
        personal: PersonalData.identifying
    }
    attachments: List<OutgoingAttachment>
}
```

```jadpo check project=e-email-attachment-limit-fix file=shared/providers/postbox/v1/types/outgoing_attachment.jadpo hidden
/// One file sent with an email.
public type OutgoingAttachment = Object {
    filename: Text
    content: Bytes
}
```

```jadpo check project=e-email-attachment-limit-fix file=shared/providers/postbox/v1/provides/email.jadpo hidden
/// Sends each email through `transmissions.send`.
provide email {
    send: {
        call: transmissions.send
        input: {
            from: message.from
            to: message.to
            subject: message.subject
            html: message.html
            text: message.text
            attachments: {
                each: message.attachments
                as: {
                    filename: item.name
                    content: item.content
                }
            }
        }
    }
    limits: {
        attachments: 10mb
    }
}
```

```jadpo check project=e-email-attachment-limit-fix file=domains/billing/translations/en_gb.jadpo hidden
translations {
    invoice_email: "Your invoice, {name}"
}
```

```jadpo check project=e-email-attachment-limit-fix file=domains/billing/emails/invoice_email.txt hidden
Hello {{ name }}, your invoice is attached.
```

The email can't be larger than `postbox` accepts, so no invoice is refused
for its size.

**If a field gives no size**, say the most it may hold.

```jadpo check project=e-email-attachment-limit-size-fix file=domains/billing/emails/invoice_email.jadpo
/// Sends a customer their invoice.
type InvoiceEmail = EmailMessage {
    name: Text
    invoice: File {
        max_size: 5mb
    }
}
```

```jadpo check project=e-email-attachment-limit-size-fix file=app.jadpo hidden
application {
    display_name: "Notes"
    connections: {
        postbox: {
            package: Postbox.v1
            credential: config.postbox_token
            host: config.postbox_host
        }
    }
    email: {
        connection: postbox
        from: "hello@notes.example"
    }
    events: {
        store: main
    }
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=e-email-attachment-limit-size-fix file=config.jadpo hidden
config {
    postbox_host: Url
    postbox_token: Text {
        secret: true
    }
}
```

```jadpo check project=e-email-attachment-limit-size-fix file=shared/providers/postbox/v1/package.jadpo hidden
/// Postbox: sends the app's email.
package Postbox.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-email-attachment-limit-size-fix file=shared/providers/postbox/v1/provider.jadpo hidden
/// Postbox's JSON API.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.bearer
}
```

```jadpo check project=e-email-attachment-limit-size-fix file=shared/providers/postbox/v1/resources/transmissions.jadpo hidden
public resource transmissions {
    send: {
        method: HttpMethod.post
        path: "/emails"
        input: OutgoingEmail
        changes: true
        source: PackageSource.written
    }
}
```

```jadpo check project=e-email-attachment-limit-size-fix file=shared/providers/postbox/v1/types/outgoing_email.jadpo hidden
/// One email to send.
public type OutgoingEmail = Object {
    from: Text {
        personal: PersonalData.contact
    }
    to: Text {
        personal: PersonalData.contact
    }
    subject: Text {
        personal: PersonalData.identifying
    }
    html: Text? {
        personal: PersonalData.identifying
    }
    text: Text? {
        personal: PersonalData.identifying
    }
    attachments: List<OutgoingAttachment>
}
```

```jadpo check project=e-email-attachment-limit-size-fix file=shared/providers/postbox/v1/types/outgoing_attachment.jadpo hidden
/// One file sent with an email.
public type OutgoingAttachment = Object {
    filename: Text
    content: Bytes
}
```

```jadpo check project=e-email-attachment-limit-size-fix file=shared/providers/postbox/v1/provides/email.jadpo hidden
/// Sends each email through `transmissions.send`.
provide email {
    send: {
        call: transmissions.send
        input: {
            from: message.from
            to: message.to
            subject: message.subject
            html: message.html
            text: message.text
            attachments: {
                each: message.attachments
                as: {
                    filename: item.name
                    content: item.content
                }
            }
        }
    }
    limits: {
        attachments: 10mb
    }
}
```

```jadpo check project=e-email-attachment-limit-size-fix file=domains/billing/translations/en_gb.jadpo hidden
translations {
    invoice_email: "Your invoice, {name}"
}
```

```jadpo check project=e-email-attachment-limit-size-fix file=domains/billing/emails/invoice_email.txt hidden
Hello {{ name }}, your invoice is attached.
```

Jadpo can now add the attachments up and check them against the package's
limit.

## Related

- [Email](../language/16-built-in-services.md#email)
- [Sending](../language/16-built-in-services.md#sending)
