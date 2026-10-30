# `email.attachment_limit`

Through a mail package, an email's `File` fields go as attachments to a
provider that accepts so much in all, its `limits: { attachments: }`, so
each field says the most it may hold and together they fit:

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
    from: Text
    to: Text
    subject: Text
    html: Text?
    text: Text?
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
