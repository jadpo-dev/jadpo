# Capability providers: options

Status: research, 2026-10-16, for
[packages and extension points](packages-and-extension-proposals.md) and
[area 11's capabilities](../11-integrations.md#capabilities). Nothing here is
decided. Sketches are illustrative; vendor details marked "(to check)" have
not been verified against the vendor's documentation.

## The question

A capability is a job several providers do, with one API in application
code, standard types, a local driver, one fake and a conformance suite. A
connection fills it with a built-in driver (SMTP, S3) or a provider
package's `provides/<capability>.jadpo` mapping (Postmark). Three problems
are open:

1. **Partial support.** SMTP never reports bounces; a cheap mail API may not
   take attachments.
2. **How far a mapping goes.** Area 11 question 21 allows field paths,
   literals, lists, nested blocks and enum tables. Should a mapping call the
   package's pure functions?
3. **Who renders an email.** Postmark, SendGrid, SES, Mandrill and
   Customer.io host templates: the app sends a template name and variables,
   and non-developers change the wording without a deploy. Jadpo's decided
   design renders everything itself from typed templates in `emails/`.

Richard's direction (2026-10-16): capabilities may be doing too much; much
of this could be plain package calls, as payments are. His requirements for
email, which every design below must meet:

- **R1.** Jadpo's typed templates, rendered by Jadpo, sent through the
  built-in SMTP driver or through an API service (SES, Postmark), chosen in
  `app.jadpo`.
- **R2.** The option to skip Jadpo's templates and send straight to a
  provider (its hosted templates with variables), in the same app as R1:
  sign-in and privacy mail rendered by Jadpo through SES, marketing mail
  through the provider's own templates.

## What the reading settled before any design

These hold whichever design is chosen.

**Erasure is not a capability matter.** Area 15 question 15 (decided) lets
*any* provider package declare its deletion operation, Stripe's included,
and requires a deleting subscriber or an accepted reason for every
connection that receives marked fields. Area 11 question 21's `erase` key in
a capability mapping duplicates that. Every design below drops it from the
mapping and uses the package-level declaration; SMTP, which keeps nothing,
needs the accepted reason.

**Mappings do not need function calls.** Every computation found in the
modelled providers is either decoding or encoding of the provider's own
types, which is decided (`wire:`, `encode:`, `decode:` from the package's
`libraries/`), or a lookup the app should own:

| Case | Where it goes |
|---|---|
| SES's `TemplateData`, a JSON object inside a string | Built-in encoding on the package type (`wire: Wire.json_text`) |
| Postmark's `To` as `"a@x, b@y"` | Built-in encoding (`Wire.joined`) |
| `From: "Name <address>"` built from two fields | The package's `Mailbox` type with `encode: mailboxes.format` |
| SES bounces through SNS: an envelope holding a JSON string holding a list | `decode:` on the webhook event type (decided) |
| A Cloudinary transformation path, `w_200,h_200,c_fill` | The package's `Transformation` type with `encode: transforms.to_path` |
| SendGrid's generated template ids per locale | A table the app gives, never package code |
| Attachment bytes as base64 | Built-in encoding |

So a mapping keeps question 21's rule (data only, checked on both sides),
and the target of a mapping is always the package's clean type, whose
encoders make the wire shape. One addition is needed by every design that
sends attachments: an **element mapping**, a list mapped item by item, in
the same shape as the decided enum table:

```text
Attachments: {
    each: message.attachments
    as: {
        Name: attachment.name
        Content: attachment.bytes
        ContentType: attachment.type
    }
}
```

It is a loop over a list with no conditions and no calls, so both sides are
still checked; it changes question 21's wording ("no loops").

**Hosted templates are provider-bound.** Moving from Postmark to SES with
Postmark-hosted templates means recreating every template in SES (a
different template language, different names or ids). "Swap the provider
with no code change" saves the code edit and none of the real work. This
weighs against putting hosted templates inside a capability.

**Which jobs does the language itself perform?** Richard's test: a
capability is needed only when Jadpo, from code it generates, does the job
without the app writing a call. Checked against the area pages:

| Job | Does the language do it itself? | Source |
|---|---|---|
| `database` | Yes: entities, queries, the outbox | Areas 5, 6, 12 |
| `cache` | Yes: `caches:`, rate-limit counters | Areas 5, 16 |
| `schedules` | Yes: `schedule` declarations, the outbox sweep on Workers | Areas 11, 12 |
| `files` | Yes: upload slots, signed links in outputs, deleting bytes on delete and erasure, export links | Areas 15, 16 |
| `email` | Yes: the privacy flow's generated mail (verification link, export link, erasure notices) | Area 15 question 13 |
| sign-in (proposed) | Yes: generated `/auth/{provider}/start` and `/callback` | Area 8 question 11 |
| `publishing` | Not yet. Today it is "a subscriber that calls a queue's package", which an app can write. Outgoing webhooks (area 16 question 8, decided, later) would be its built-in user | Areas 11, 16 |
| `sms` | No. No area generates an SMS; `SignInCodeSms` is an app example | Areas 8, 11 |

Password-reset and magic-link mail are not built in (area 8 leaves magic
links to integrations), so area 15's privacy mail is the only verified
built-in email today.

## 1. The concepts each design needs

| Concept | What it is | A: parts | B: split | C: minimal |
|---|---|---|---|---|
| Capability | A job with one API in code; a fixed list | 7, plus sign-in | 12 or more | 6 (email, files, database, cache, schedules, publishing), plus sign-in |
| Connection, built-in driver, provider package | As decided | yes | yes | yes |
| Mapping (`provides/<capability>.jadpo`) | Data only, element mapping added | yes | yes | yes |
| Core | What every connection filling the capability must do | yes | yes (the whole capability) | yes (the whole capability) |
| Part | An optional key of a mapping; using one the connection lacks is a compile error | yes | no | no |
| Standard events | `EmailBounced`, `EmailComplained`, `SmsDelivered`, mapped from webhooks | yes, as parts | yes, as their own capability | no: the package's own webhook events |
| Rendering choice | Where a message's content comes from | `rendered_by:` on the message type | a second message base, `TemplatedEmail` | none: hosted templates are a package call |
| Template id and variables | `template.id` (name and locale), `template.variables`, an id table in `app.jadpo` | yes | yes | no |
| Template source check | The contract run reads the provider's templates and compares their variables with the fields | yes | yes | no (an app may write a contract case) |
| Free-form object input | A package input that takes any of the app's object types (Postmark's `TemplateModel`) | no | no | yes |
| Local driver, fake, conformance suite | One each per capability | yes, aware of parts | yes, one per split capability | yes |
| Deletion declaration | Area 15's, on any package | yes | yes | yes |
| **New concepts beyond the decided set** | | **4** (part, rendering choice, template id and variables, template check) | **4** (five or more extra capabilities, templated base, template id and variables, template check) | **1** (free-form object input), and two removed (standard events, `sms`) |

The count is of concepts an agent or a reviewer must hold; the element
mapping is shared by all three and left out.

## 2. The designs

### Design A: one capability with optional parts

**Rule.** Each capability has a core and named optional parts. A part is an
optional key of the capability's mapping; a built-in driver's parts are
fixed by Jadpo (SMTP: `send`, `attachments`). Using a part the chosen
connection lacks is a compile error: a `File` field on a message needs
`attachments`, a subscriber on `EmailBounced` needs `bounced`, a message
`rendered_by: Rendering.provider` needs `templates`. The local driver and
the fake take on exactly the parts of the connection they stand in for, so
a test cannot script a bounce production would never produce. Conformance
runs only the claimed parts.

| Capability | Core | Parts |
|---|---|---|
| `email` | `send` | `attachments`, `templates`, `bounced`, `complained` |
| `files` | `put`, `get`, `delete`, `link`, `upload_slot` | `variants` |
| `sms` | `send` | `templates`, `delivered`, `failed` |

#### A, scenario 2: Postmark, Jadpo-rendered, with bounces and streams

```text
// packages/postmark/v1/provides/email.jadpo
provide email {
    send: {
        call: email.send
        input: {
            From: message.from
            To: message.to
            Subject: message.subject
            HtmlBody: message.html
            TextBody: message.text
            MessageStream: settings.message_stream
        }
        receipt: response.MessageID
        failures: {
            inactive_recipient: EmailFailure.recipient_rejected
        }
    }
    attachments: { ... }                      // scenario 5
    templates: { ... }                        // scenario 3
    bounced: {
        on: BounceWebhook
        recipient: event.Email
        receipt: event.MessageID
        kind: {
            from: event.Type
            values: {
                hard_bounce: BounceKind.permanent
                soft_bounce: BounceKind.temporary
            }
        }
    }
    complained: {
        on: SpamComplaintWebhook
        recipient: event.Email
        receipt: event.MessageID
    }
}

// app.jadpo
postmark: {
    package: Postmark.v1
    credential: config.postmark_token
    environment: config.postmark_environment
    message_stream: "outbound"
    webhooks: {
        secret: {
            username: config.postmark_hook_user
            password: config.postmark_hook_password
        }
    }
}
email: {
    connection: postmark
    from: config.mail_sender
}

// domains/accounts/subscribers/note_bounce.jadpo
subscriber note_bounce {
    on: EmailBounced
    run: mark_address_undeliverable(event)
}
```

With `email: { connection: gmail }` (SMTP), `note_bounce` is a compile
error naming the connection and the part, which turns area 16's "with SMTP
there are none" into a check.

#### A, scenario 3: Postmark renders, variables only, with locales

```text
// domains/marketing/emails/spring_offer.jadpo
/// The spring offer. Marketing edits its wording in the provider's editor.
type SpringOffer = EmailMessage {
    rendered_by: Rendering.provider
    first_name: Text
    offer_link: Url
    ends_on: Date
}
// No spring_offer.html, .txt or translations: two sources of wording is an error.

// packages/postmark/v1/provides/email.jadpo, the templates part
templates: {
    call: email.send_with_template
    input: {
        From: message.from
        To: message.to
        TemplateAlias: template.id
        TemplateModel: template.variables
        MessageStream: settings.message_stream
    }
    receipt: response.MessageID
    failures: {
        template_not_found: EmailFailure.template_missing
    }
    source: {
        call: templates.get
        input: {
            TemplateIdOrAlias: template.id
        }
        syntax: TemplateSyntax.mustache
        subject: response.Subject
        html: response.HtmlBody
        text: response.TextBody
    }
}

// use, unchanged from a Jadpo-rendered message
emit SpringOffer {
    to: customer.email
    locale: customer.locale
    first_name: customer.first_name
    offer_link: offer.link
    ends_on: offer.ends_on
}
```

- `template.id` is the message name and locale, `spring_offer.en_gb`, which
  Postmark allows as an alias (to check). A provider with generated ids
  takes a table on the capability: `email: { connection: sendgrid
  template_ids: { spring_offer.en_gb: "d-8f3…" } }`; the compiler checks
  the table covers every provider-rendered message in every locale.
- `template.variables` sends each field as it would print in the message's
  locale (`16 October 2026`, `£12.50`), because a hosted template cannot
  format; lists and nested objects stay structured.
- **Checking lost:** placeholders against fields, translations, the health
  score. **Recovered:** the connection's contract run (scheduled CI, decided)
  reads each template through `source`, checks it exists for every locale,
  parses its variables in the declared syntax (Mustache, Handlebars: a fixed
  list in Jadpo's tooling) and compares them with the fields, then runs the
  health checks on the provider's content. Drift after a marketing edit is
  caught on the next scheduled run, not at compile time; a send that fails
  meanwhile is `template_missing`, parked for the operator.
- **Dev inbox** shows the message name, template id, locale and variables,
  with `Url` fields as working links (so `jadpo email links` still follows a
  flow), and says the content is the provider's. `email.sent` holds the
  message value in both modes; `rendered` exists only on Jadpo-rendered
  types.

#### A, scenario 5: an invoice PDF

```text
// domains/billing/emails/invoice_email.jadpo
type InvoiceEmail = EmailMessage {
    invoice_number: Text
    invoice: File {                           // a File field on a message is an attachment
        max_size: 5mb
    }
}

// packages/postmark/v1/provides/email.jadpo, the attachments part
attachments: {
    max_total: 10mb
    input: {
        Attachments: {
            each: message.attachments
            as: {
                Name: attachment.name
                Content: attachment.bytes
                ContentType: attachment.type
            }
        }
    }
}

// a package whose mapping has no attachments:
// error: InvoiceEmail has an attachment (invoice), and the email connection
//        `cheapmail` cannot send attachments. Send through a connection that
//        can, or send a link (a Url field) instead.
```

The bytes are read from the files store at send time, so the outbox holds
the file's key, not the PDF. A `max_size` above the part's `max_total` is a
compile error.

#### A, files: S3 against a media service

```text
// domains/accounts/entities/user.jadpo
avatar: File? {
    store: avatars
    max_size: 5mb
    variants: {
        thumb: { width: 200  height: 200  fit: Fit.cover }
    }
}

// app.jadpo, S3 (R2): core only
avatars: { driver: Driver.s3  host: config.r2_host  region: "auto"  bucket: "avatars"  credential: { ... } }
// error: User.avatar asks for variants, and the files connection `avatars`
//        (S3) cannot make them.

// packages/cloudinary/v1/provides/files.jadpo
provide files {
    put:         { call: uploads.upload  input: { file: content  public_id: file.key }  receipt: response.public_id }
    get:         { call: delivery.original  input: { public_id: file.key } }
    delete:      { call: uploads.destroy  input: { public_id: file.key } }
    link:        { call: delivery.signed  input: { public_id: file.key  expires: link.expires } }
    upload_slot: { call: uploads.signed_slot  input: { public_id: file.key  max_size: slot.max_size } }
    variants: {
        link: {
            call: delivery.signed
            input: {
                public_id: file.key
                transformation: variant           // Transformation, encoded by transforms.to_path
                expires: link.expires
            }
        }
    }
}
```

Cloudinary signs uploads and delivery links with a hash over the
parameters and the API secret (to check). Packages cannot read secrets, so
this needs a credential scheme in a Jadpo release, as Twilio's webhook check
did (area 11 question 25). Area 16 also lists files as "S3 protocol only";
a media service filling `files` reopens that.

#### A, SMS: Twilio with status callbacks and hosted templates

```text
// packages/twilio/v2010_04_01/provides/sms.jadpo
provide sms {
    send: {
        call: messages.create
        input: { To: message.to  From: message.from  Body: message.text  StatusCallback: connection.webhook_url }
        receipt: response.sid
        failures: { invalid_to_number: SmsFailure.recipient_rejected }
    }
    templates: {                                  // Twilio Content API; WhatsApp requires it (to check)
        call: messages.create
        input: { To: message.to  From: message.from  ContentSid: template.id  ContentVariables: template.variables  StatusCallback: connection.webhook_url }
        receipt: response.sid
    }
    delivered: { on: StatusCallback  receipt: event.MessageSid  when: { from: event.MessageStatus  values: { delivered: true } } }
    failed:    { on: StatusCallback  receipt: event.MessageSid  when: { from: event.MessageStatus  values: { failed: true  undelivered: true } } }
}

// app.jadpo
twilio: { package: Twilio.v2010_04_01  account_sid: config.twilio_sid  credential: config.twilio_auth  webhooks: { secret: config.twilio_auth } }
sms: { connection: twilio  from: config.sms_sender  template_ids: { appointment_reminder.en_gb: "HX…" } }
```

The same part names and rules as email: the pattern is general.

#### A, the other scenarios

| # | Scenario | Design A |
|---|---|---|
| 1 | SMTP (Gmail), Jadpo-rendered | Driver connection (decided); parts `send`, `attachments`; subscribing to bounces is an error |
| 4 | SendGrid dynamic templates | `templates` part; ids from `template_ids:`; `syntax: TemplateSyntax.handlebars`; personalisations as nested blocks |
| 4 | Amazon SES | `templates` part (`Content.Template`, `TemplateData` with `Wire.json_text`); no `idempotency:`, so a timed-out send is `outcome_unknown`; bounces from SNS through `decode:` on the event type |
| 4 | Resend | Core and attachments; `templates` only if its template API fits (to check); idempotency key (to check) |
| 6 | Provider extras | Settings (`configuration_set:`) or the package's own API through `domains:` (decided) |
| 7 | Erasing a person | The package's deletion declaration (area 15), not the capability; SMTP needs an accepted reason |
| 8 | Dev and tests | Jadpo-rendered: inbox, health score, `rendered` on `email.sent`. Provider-rendered: inbox shows variables; health from the contract run |
| 9 | Two kinds of mail | `kind: EmailKind.bulk` on the message now (the health check already needs it), mapped to a stream by an enum table; a second connection later |

### Design B: whole capabilities, extras split out

**Rule.** There are no optional parts. A connection provides a capability
entirely or not at all, and whatever not every provider does is a separate
capability, wired separately in `app.jadpo`. Using an unwired capability is
already an error, so no new rule is needed for partial support.
Attachments are too small to split, so they are core: a mail API without
them cannot fill `email`.

| Capability | Holds |
|---|---|
| `email` | Jadpo-rendered `EmailMessage`, attachments included |
| `email_templates` | `TemplatedEmail` messages, rendered by the provider |
| `email_events` | `EmailBounced`, `EmailComplained` |
| `files` | Core file operations |
| `image_variants` | Variants of files in any store |
| `sms`, `sms_templates`, `sms_events` | As email |

#### B, scenarios 2, 3 and 5

```text
// packages/postmark/v1/provides/email.jadpo            core send, attachments required
// packages/postmark/v1/provides/email_templates.jadpo  the send_with_template mapping and its source
// packages/postmark/v1/provides/email_events.jadpo     bounced and complained, as in A
// (the mappings themselves are A's, one file each)

// app.jadpo
email:           { connection: postmark  from: config.mail_sender }
email_templates: { connection: postmark  from: config.marketing_sender }
email_events:    { connection: postmark }

// domains/marketing/emails/spring_offer.jadpo
type SpringOffer = TemplatedEmail {
    first_name: Text
    offer_link: Url
    ends_on: Date
}

// domains/billing/emails/invoice_email.jadpo: as A
// `email: { connection: cheapmail }` is an error at the wiring:
// Cheapmail.v1 does not provide email (its mapping has no attachments).
```

Composition is the gain B claims: `email` through an SMTP relay and
`email_events` from the same vendor's webhooks. It is mostly illusory,
because events only describe mail that provider sent; the one real case is
`image_variants` from an image service (imgproxy, Cloudinary's fetch
delivery, to check) over an S3 store.

#### B, files and SMS

```text
// app.jadpo
avatars:        { driver: Driver.s3  ... }
images:         { package: Imgproxy.v3  ... }           // a container service (decided runs:)
image_variants: { connection: images  source: avatars }

twilio:         { package: Twilio.v2010_04_01  ... }
sms:            { connection: twilio  from: config.sms_sender }
sms_templates:  { connection: twilio  from: config.sms_sender  template_ids: { ... } }
sms_events:     { connection: twilio }
```

#### B, the other scenarios

| # | Scenario | Design B |
|---|---|---|
| 1 | SMTP | `email` only; `email_events` cannot be wired to it |
| 4 | SendGrid, SES, Resend | As A, each extra as its own capability file |
| 6 | Provider extras | As A |
| 7 | Erasing a person | As A |
| 8 | Dev and tests | As A, but `TemplatedEmail` is a second fake (`email_templates.sent`) |
| 9 | Two kinds of mail | As A |

### Design C: minimal capabilities

**Rule.** A capability exists only for a job the language performs itself,
and it covers exactly the language's own construct for that job, whole:
for email, a Jadpo-rendered `EmailMessage` sent after commit, attachments
included because the type can carry a `File`; for files, what a `File`
field needs. Every connection that fills a capability does all of it, so
partial support cannot arise. Everything provider-specific (hosted
templates, bounces, suppression lists, marketing sends) is the provider
package's own operations and webhook events, called from an action or
subscriber in a domain the connection lists, exactly as payments are. One
connection can serve both: the capability uses it, and so do the listed
domains.

| Capability | What it covers |
|---|---|
| `email` | Send a Jadpo-rendered message, attachments included; the dev inbox; `email.sent`; area 15's privacy mail |
| `files` | `put`, `get`, `delete`, `link`, `upload_slot` |
| `sms` | Not a capability: Twilio is a package |

#### C, scenario 2: Postmark, Jadpo-rendered, with bounces and streams

```text
// packages/postmark/v1/provides/email.jadpo: the core only
provide email {
    send: {
        call: email.send
        input: {
            From: message.from
            To: message.to
            Subject: message.subject
            HtmlBody: message.html
            TextBody: message.text
            MessageStream: settings.message_stream
            Attachments: { each: message.attachments  as: { ... } }   // scenario 5
        }
        receipt: response.MessageID
        failures: {
            inactive_recipient: EmailFailure.recipient_rejected
        }
    }
}

// app.jadpo
postmark: {
    package: Postmark.v1
    domains: [accounts, marketing]               // only for the package's own operations
    credential: config.postmark_token
    environment: config.postmark_environment
    message_stream: "outbound"
    webhooks: {
        secret: { username: config.postmark_hook_user  password: config.postmark_hook_password }
    }
}
email: {
    connection: postmark
    from: config.mail_sender
}

// domains/accounts/subscribers/note_bounce.jadpo
subscriber note_bounce {
    on: Postmark.BounceWebhook                   // the package's own event (decided webhooks)
    run: mark_address_undeliverable(event.Email)
}
```

Bounces are the package's webhook events because nothing in the language
consumes them. Moving to SES rewrites this one subscriber to
`on: AmazonSes.BounceNotification`; that is the cost, and it is visible in
the diff.

#### C, scenario 3: Postmark renders, variables only, with locales

```text
// domains/marketing/types/spring_offer_model.jadpo
/// The variables the spring-offer template in Postmark reads.
type SpringOfferModel = Object {
    first_name: Text
    offer_link: Url
    ends_on: Text                                // formatted here: a hosted template cannot format
}

// domains/marketing/actions/send_spring_offer.jadpo
action send_spring_offer(due: SpringOfferDue) fails OfferRecipientInactive -> Void {
    var request: Postmark.TemplatedEmail = Postmark.TemplatedEmail {
        From: config.marketing_sender
        To: due.email
        TemplateAlias: offer_templates.alias(due.locale)    // the app's own pure function
        TemplateModel: SpringOfferModel {
            first_name: due.first_name
            offer_link: due.offer_link
            ends_on: temporal.format(due.ends_on, style: TimeFormat.date_long, locale: due.locale)
        }
        MessageStream: "broadcast"
    }
    attempt postmark.email.send_with_template(request) {
        inactive_recipient: OfferRecipientInactive
    }
}

// domains/marketing/subscribers/send_spring_offer.jadpo
subscriber send_spring_offer {
    on: SpringOfferDue
    run: send_spring_offer(event)
}
```

- **After commit** comes from the event and subscriber, as for any provider
  call; the decided one-changing-call rule holds.
- **`TemplateModel`** is free-form in Postmark's API. Typing it as
  `Map<Text, Text>` would lose nesting and the app's types, so C needs one
  general addition: a package input field that takes any of the app's
  object types, serialised in its JSON form (Stripe's metadata, SendGrid's
  `dynamic_template_data` and Customer.io's attributes need the same).
- **Checking lost:** that the alias exists and its variables match
  `SpringOfferModel`, the health score, the dev inbox. **Recovered:** only if
  the app writes a contract case (scheduled, decided) that calls Postmark's
  template validation (to check). Jadpo cannot check content it does not
  hold in any design; A recovers it automatically, C leaves it to the app.
- **Tests** read `postmark.email.send_with_template.requests` (area 13's
  form) as typed values. **Development** fakes the call by default and logs
  it with its input and answer ([section 4](#4-provider-calls-in-development)),
  which stands in for the dev inbox.
- **Locales** are the app's choice of alias, in its own code.

#### C, scenario 5: an invoice PDF

```text
// domains/billing/emails/invoice_email.jadpo: as A
type InvoiceEmail = EmailMessage {
    invoice_number: Text
    invoice: File { max_size: 5mb }
}

// The mapping above carries Attachments; the email conformance suite sends one.
// app.jadpo: email: { connection: cheapmail }
// error: `cheapmail` cannot be the email connection: Cheapmail.v1 does not
//        provide email (its mapping sends no attachments). Its own operations
//        are still callable from the domains its connection lists.
```

Every mainstream API (Postmark, SendGrid, Mailgun, Resend, SES through raw
or simple content) takes attachments (to check for SES's simple content), so
requiring them excludes little. The package declares its size limit; a
`max_size` above it is a compile error, as in A.

#### C, files: S3 against a media service

```text
// app.jadpo
avatars: { driver: Driver.s3  host: config.r2_host  region: "auto"  bucket: "avatars"  credential: { ... } }
// or
avatars: { package: Cloudinary.v1  domains: [accounts]  credential: { ... }  cloud_name: "acme" }

// packages/cloudinary/v1/provides/files.jadpo: put, get, delete, link, upload_slot as in A; no variants

// domains/accounts/libraries/avatars.jadpo, using the package's pure function
var thumb: Url = Cloudinary.urls.image(user.avatar.key, Cloudinary.Transformation { width: 200  height: 200  crop: Crop.fill })
```

A store only stores. Transformations are the media service's own feature,
reached through the package's pure functions (unsigned delivery URLs) or its
API. If area 16's proposed `variants:` on `File` is built, the runtime
makes the variants (an image service it runs, decided `runs:`) over any
store, so no store needs a variants part. Cloudinary filling `files` still
needs its signing scheme in a Jadpo release.

#### C, SMS: Twilio as a package

```text
// app.jadpo
twilio: {
    package: Twilio.v2010_04_01
    domains: [appointments]
    account_sid: config.twilio_sid
    credential: config.twilio_auth
    webhooks: { secret: config.twilio_auth }
}

// domains/appointments/actions/send_reminder_sms.jadpo
action send_reminder_sms(due: ReminderDue) fails ReminderNumberInvalid -> Void {
    var request: Twilio.CreateMessage = Twilio.CreateMessage {
        To: due.phone
        From: config.sms_sender
        Body: translations.appointment_reminder(locale: due.locale, starts_at: due.starts_at_text)
        StatusCallback: twilio.webhook_url
    }
    attempt twilio.messages.create(request) {
        invalid_to_number: ReminderNumberInvalid
    }
}

// domains/appointments/subscribers/note_sms_failed.jadpo
subscriber note_sms_failed {
    on: Twilio.StatusCallback
    run: note_reminder_status(event)
}
```

The text is still typed against translations (area 10's generated
`translations.key(...)`). Lost against a capability: the SMS dev inbox and
`sms.sent`. If a built-in feature ever sends SMS (sign-in codes in area 8),
`sms` returns as a capability on the same rule.

#### C, the other scenarios

| # | Scenario | Design C |
|---|---|---|
| 1 | SMTP (Gmail), Jadpo-rendered | Driver connection (decided); attachments over MIME |
| 4 | SendGrid dynamic templates | Jadpo-rendered mail through `email`; dynamic templates by `sendgrid.mail.send(...)` with the template id in the app's code |
| 4 | Amazon SES | `email` through SES's simple content; templates by `ses.outbound_emails.send(...)` with `Content.Template`; bounces as `AmazonSes.BounceNotification` (SNS, decoded); no idempotency, so a timed-out send is `outcome_unknown` |
| 4 | Resend | `email` mapping; its templates and idempotency (to check) are its own API |
| 6 | Provider extras | The package's own API through `domains:`; per-send values such as SES's configuration set as settings (decided) |
| 7 | Erasing a person | The package's deletion declaration (area 15): SES's suppressed-destination delete; Postmark's data removal uses an account token, a second API and so a second package (to check), or an accepted reason |
| 8 | Dev and tests | Jadpo-rendered mail: inbox, health score, `email.sent`, unchanged. Package calls: the trace and `.requests` |
| 9 | Two kinds of mail | Marketing through hosted templates is already package calls, on any connection; Jadpo-rendered bulk through a second connection waits for an app that asks |

## 3. Comparison

| | A: parts | B: split | C: minimal |
|---|---|---|---|
| R1: Jadpo templates through SMTP, SES or Postmark, chosen in `app.jadpo` | Yes | Yes | Yes |
| R2: send straight to a provider's hosted templates, in the same app | Yes, through the capability (`rendered_by:`) | Yes, through `email_templates` | Yes, as a package call from an action |
| One connection for both | Yes: the parts of one mapping | Yes: wired twice | Yes: `email:` uses it, `domains:` call it |
| New concepts | 4 | 4, plus five or more capabilities, each with a local driver, fake and suite | 1, and two removed |
| What an agent must remember | Which parts each connection has; `rendered_by:`; template ids and locales; which events are standard | Which capability each job is; wiring each; two message bases | One rule: if Jadpo renders it, `emit` it; anything else is the package, as for Stripe |
| What a reviewer sees | One `emit` whatever the mode; the mode in the type; parts invisible until an error | Every capability wired in `app.jadpo` | Provider calls named where they happen, in listed domains; `app.jadpo` shows one `email:` line |
| Checking kept | Everything for Jadpo-rendered mail; parts against connections at compile time; provider templates by the contract run | As A | Everything for Jadpo-rendered mail; package calls checked against package types |
| Checking lost, and how recovered | Provider templates' variables at compile time, recovered by the scheduled contract run | As A | Provider templates' variables and the health score, recoverable only by an app contract case; the dev inbox for hosted mail, replaced by the development call log (section 4) |
| Swapping provider | `app.jadpo` only, except hosted templates must be recreated at the new provider | As A | `app.jadpo` for Jadpo-rendered mail; package calls and bounce subscribers rewritten |
| Partial support | Parts | Splitting, attachments required | Does not arise: the capability is the language's construct, whole |
| Decided items changed | Q21 (erase out, element mapping, parts); area 16's emails (a message without templates); area 13's one fake (part-aware) | Q19's list grows; Q21 as A; area 16 as A | Q19 (`sms` removed); Q21 (erase and standard events out, element mapping); area 16 (no `EmailBounced`, `EmailComplained`) |

## 4. Provider calls in development

This applies to every provider operation, whichever design is chosen.
Capabilities already go to local drivers in `jadpo dev` (area 11 question
23). The question is a package's own operations: `stripe.payment_intents`,
`postmark.email.send_with_template`, `twilio.messages.create`. Richard's
requirement: development can stop outside calls per connection and per
operation, and logs every call for debugging.

**Recommendation.**

- **Faked by default.** In `jadpo dev` every provider operation is answered
  locally, so a fresh checkout, an agent or a laptop on a train runs every
  flow with no credential and nothing leaving the machine (the proposed
  rule 6). The answer comes from the package's recording of that operation
  where there is one, its placeholders filled with generated values of the
  right types, so optional fields and list lengths look like the
  provider's; otherwise from the generated fake's valid defaults (decided,
  area 11 question 12). Tests are unchanged: they always fake (area 13).
- **Let through by name, to the sandbox only.** A connection lists the
  resources or operations that reach the provider in development:

  ```text
  // app.jadpo
  stripe: {
      package: Stripe.v2024_06_20
      domains: [payments]
      credential: config.stripe_secret_key
      environment: config.stripe_environment
      development: {
          sandbox: [payment_intents.retrieve, customers]   // an operation, or a whole resource
      }
  }
  postmark: {
      package: Postmark.v1
      ...                                                   // no development: every call faked
  }
  ```

  One shape: a list of names the compiler checks against the package. A
  whole connection is its resources listed, which the compiler can suggest
  from the ones the app calls, so a reviewer always sees exactly which
  calls leave the machine. `jadpo dev` refuses a let-through connection
  whose environment is `live`, as `jadpo package record` does, and a
  connection whose sandbox credential is missing from `.env.local` stays
  faked with one line saying which setting to run `jadpo config set` for.
- **Written in `app.jadpo`, overridable for one session.** Which outside
  calls development may make is an outbound-network decision, and area 11
  makes connections the one permission list, reviewed in the approval
  report; a per-developer file would be a second, unreviewed list, and an
  agent would not know it existed. A developer chasing one problem passes
  `jadpo dev --fake stripe` or `--sandbox stripe.refunds.create` for that
  run only; nothing persists. The credential is the only per-developer part,
  and it already lives in `.env.local` (area 10).
- **Scripting a failure.** `jadpo dev` answers from fakes that never fail,
  so `jadpo calls script stripe.payment_intents.create card_declined` sets
  the next answer, in area 13's `fakes:` form, letting an agent walk a
  failure path by hand. Fakes keep no state: a flow that creates and then
  reads the same object either lets both operations through or scripts the
  read.
- **Every call logged.** Faked, scripted or real, each call records the
  connection, resource and operation, the input, the answer or failure,
  where the answer came from (`recording`, `generated`, `scripted`,
  `sandbox`), attempts and retries, duration, and the request id and span,
  so it appears in `jadpo trace <request>` (area 17) beside the queries.
  `jadpo calls` lists them (`--connection`, `--operation`, `--request`) and
  `jadpo calls show <id>` prints one; both take `--json`. They are kept with
  the last 500 timelines in `build/dev/`, never committed, and appear in
  the developer console. Credentials and `secret: true` values are never
  shown. The screens follow the CLI design language and are designed with
  the design skills before they are built.
- **Values in the log.** Area 17's dev timeline hashes text parameters
  (area 4's log rule), which would make a payload log useless for
  debugging. *Recommendation:* the development call log shows inputs and
  answers in full, since it holds local data in a git-ignored folder;
  production keeps area 16's spans with no values. This changes area 17's
  rule for call payloads in development only.

**Alternatives weighed.** Let-through by default (sandbox unless faked)
needs every developer and agent to hold sandbox credentials before
anything runs, and sends local data to third parties silently. A
per-developer settings file keeps `app.jadpo` clean but hides the decision
from review and from agents. A single blanket switch fails Richard's
requirement (fake Postmark's send, let Stripe's reads through).

## 5. Recommendation

**Design C.** The decisive reason: a capability defined by the language's
own construct is whole by definition, so the partial-support problem
disappears instead of needing a parts mechanism, and everything
provider-specific goes through the one mechanism already decided for
payments. What A abstracts in addition (hosted templates, bounces) is
provider-bound anyway: moving hosted templates means rebuilding them at the
new provider, so the code edit C asks for is the smaller part of that
work. C meets R1 and R2 on one connection, with one rule an agent can apply
without looking anything up.

What C costs, plainly: hosted-template mail gets no dev inbox entry, no
health score and no automatic template check (an app can add a contract
case), and bounce handling names the provider. A is the fallback if hosted
templates turn out to be common in Jadpo apps, and C does not block it:
parts can be added to a whole capability later without breaking a
connection.

### Open questions for Richard

1. **Adopt design C: capabilities only for jobs the language does itself,
   each covering its construct whole?** *Recommendation:* yes, for the
   reason above.
2. **Do `EmailBounced` and `EmailComplained` leave the capability?**
   *Recommendation:* yes; bounces are the package's webhook events until a
   built-in feature consumes them (for example, Jadpo refusing to send to
   an address that bounced permanently), at which point they return as part
   of the core and every email connection must report them.
3. **Is `sms` removed from the capability list?** *Recommendation:* yes; no
   area sends SMS itself. Twilio is a package, text still comes from typed
   translations, and `sms` returns when a built-in feature (SMS sign-in
   codes) needs it. This changes area 11 question 19.
4. **Does `publishing` stay?** *Recommendation:* yes, as the capability
   outgoing webhooks (decided, later) are built on; until then it is the one
   capability whose built-in user is not yet built, and a subscriber calling
   a queue's package does the same job.
5. **Does `erase` leave capability mappings for area 15's package-level
   deletion declaration?** *Recommendation:* yes, in every design: one way to
   erase at any provider, Stripe and Postmark alike.
6. **Do mappings stay data, with no function calls?** *Recommendation:* yes;
   computation lives in the decided encoders and decoders on the package's
   clean types. Add the element mapping (`each:` and `as:`), which changes
   question 21's "no loops".
7. **Are attachments core: a `File` field on an `EmailMessage` is an
   attachment, read from its store at send time and checked against the
   package's declared size limit?** *Recommendation:* yes; a provider
   without attachments stays a package but cannot be the email connection.
8. **Can a package input take any of the app's object types?** (Postmark's
   `TemplateModel`.) *Recommendation:* yes, as a general area 2 addition
   (syntax to be chosen there); hosted templates, Stripe metadata and CRM
   attributes all need it, and `Map<Text, Text>` loses the app's types.
9. **Do email sends without an idempotency key keep area 12's parking on
   `outcome_unknown`?** Postmark, SES, SendGrid and Mailgun take no key, so
   a provider blip parks mail for an operator. *Recommendation:* keep the
   decided rule for now; a duplicate mail is not always harmless (a payment
   receipt), and `jadpo deliveries` can resend in bulk. Revisit if chunk 7.3
   shows parking is common.
10. **Two kinds of mail through two connections (scenario 9): now or
    later?** *Recommendation:* later. Marketing mail through hosted templates
    is already package calls in C; Jadpo-rendered bulk mail through a second
    connection is one key on `email:` when an app asks.
11. **Image variants:** when area 16's `variants:` is built, does the
    runtime make them over any store, rather than stores offering them?
    *Recommendation:* yes, so `files` stays whole; a media service filling
    `files` waits for an app that needs it and for its signing scheme in a
    Jadpo release.
12. **Are provider operations faked by default in `jadpo dev`, answered
    from recordings and then generated defaults?** *Recommendation:* yes
    (section 4): it runs with no credentials and keeps local data local.
13. **Is the let-through list written on the connection in `app.jadpo`
    (`development: { sandbox: [...] }`), with session-only `--fake` and
    `--sandbox` flags and no per-developer file?** *Recommendation:* yes;
    which calls leave the machine is a reviewed network decision, and only
    the credential is per developer.
14. **Does the development call log show payloads in full, changing area
    17's hashing of text for calls in development only?**
    *Recommendation:* yes; secrets are never shown and production is
    unchanged.
