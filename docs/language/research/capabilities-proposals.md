# Capabilities

Status: proposal, 2026-10-13, for the [area 11 page](../11-integrations.md#capabilities)
(questions 19–25) and [area 16](../16-built-in-services.md). It builds on the
decided provider packages: Jadpo source, declarations only, connected in
`app.jadpo`, called as `connection.resource.operation(input)`.

## The idea

Richard's idea: a package states its purpose (mail, files, SMS), so it is
installed in the right place, used through an API made for that purpose,
and a third party fills in a known shape. In Jadpo that is a
**capability**: a small, fixed list owned by the language, each with one
native API that application code uses (`emit WelcomeMail { … }`, a `File`
field). A package maps the standard operations onto its own calls, so
swapping provider changes configuration, not code.

## What others do

| System | Shape | Lesson |
|---|---|---|
| Laravel [disks](https://laravel.com/docs/12.x/filesystem), [mail](https://laravel.com/docs/12.x/mail), [queues](https://laravel.com/docs/12.x/queues) | Named instances in config, each with a `driver`; one API; `Storage::fake()` | The model to copy. Custom drivers are PHP closures, which Jadpo cannot have |
| Rails [Active Storage](https://guides.rubyonrails.org/active_storage_overview.html), [Action Mailer](https://guides.rubyonrails.org/action_mailer_basics.html) | `Disk`, `S3`, `Mirror` services per environment; `delivery_method` `:smtp`, `:test` | Local disk in development, S3 in production, same code |
| Django [storages](https://docs.djangoproject.com/en/5.1/ref/files/storage/), [email backends](https://docs.djangoproject.com/en/5.1/topics/email/#email-backends) | Settings name a class; console and in-memory backends | A small interface; backends are code |
| Spring Boot [auto-configuration](https://docs.spring.io/spring-boot/reference/features/developing-auto-configuration.html) | A starter on the classpath configures itself | Too implicit; Jadpo names every connection |
| Encore [primitives](https://encore.dev/docs/ts/primitives) | Databases, Pub/Sub, object storage, caches, cron, secrets in code; local versions in `encore run`; infra config maps them | Closest in spirit: a fixed set, mapped by configuration, local by default |
| [Nitric](https://nitric.io/docs) | Buckets, topics, queues, schedules; code requests permissions; provider plugins deploy | Permissions from code; providers are executable plugins |
| Dapr [building blocks](https://docs.dapr.io/concepts/building-blocks-concept/) | Fixed APIs backed by pluggable components | See below |
| Terraform [providers](https://developer.hashicorp.com/terraform/plugin/how-terraform-works) | Typed schemas; each provider an executable over RPC | Typed schemas a registry can check; but code |
| Cloudflare [bindings](https://developers.cloudflare.com/workers/runtime-apis/bindings/) | One binding per resource in Wrangler config; "a permission and an API in one piece"; no credentials in code | One connection per resource, and the connection is the permission |

**Dapr is the closest prior art.** It gets the core right: a small set of
APIs, the provider chosen in configuration. Its genericity hurts in three
places:

- **Untyped settings.** A component is `spec.type: state.redis` with
  `metadata:` as a list of name/value strings
  ([schema](https://docs.dapr.io/reference/resource-specs/component-schema/)).
  A misspelt setting is found at runtime.
- **Uneven features behind one API.** Most state stores support only basic
  reads and writes; transactions, ETags and TTL vary
  ([matrix](https://docs.dapr.io/reference/components-reference/supported-state-stores/)),
  so code can fail on another store.
- **"Bindings" are a generic escape hatch:** an operation name as a string
  and a free-form payload, where the abstraction is most needed.

Jadpo types every setting, refuses a connection lacking a feature the app
uses, and keeps the typed provider API instead of a generic binding.

## The design

### The capabilities

A capability earns its place when the compiler gains something by knowing
it (area 16's test) and when several providers do the same job. First build:

| Capability | Native API | Built-in drivers | Packages may provide it? |
|---|---|---|---|
| `database` | Entities (area 5) | PostgreSQL; D1 later | No: needs real code |
| `cache` | `caches:` (area 5) | Redis; in memory | No, in the first build |
| `files` | `File` fields; `store.put(bytes, type)` | S3 protocol; local disk | Yes, as S3 presets |
| `mail` | `type X = Mail`, emitted | SMTP; local inbox | Yes |
| `sms` | `type X = Sms`, emitted | Local inbox | Yes |
| `publishing` | Chosen public events sent outward | Local log (outgoing webhooks later) | Yes |
| `schedules` | `schedule` declarations (area 12) | In-process timer; Cloudflare cron; an external tick | No |

Not capabilities:

- **Event delivery** stays inside the runtime, from the outbox in the main
  store (Richard, 2026-10-13): one place for logs and traces, no slower
  requests, no queue service needed. External queues are **publishing**
  targets: chosen public events sent outward, at least once, after commit,
  from the outbox, with the trace context.
- **Telemetry.** OpenTelemetry is already the standard every monitoring
  vendor accepts; a vendor is an endpoint and a header (area 16 question 10).
- **Push notifications.** Later, as their own capability, when an app needs them.

Adding a capability is a language change made through the review, with the
same parts every time: a native API, standard types, a local driver for
development, a fake, and a conformance suite. Jadpo owns the list, as Dapr
and Encore do; packages cannot invent capabilities.

### One `connections:` block

`stores:` (area 5) and `providers:` (area 11) become one `connections:`
block. Each entry is a **built-in driver** (`driver: Driver.redis`) or a
**package** (`package: Postmark.v1`), with its settings. What a connection
*can* serve comes from its driver or package; what it *does* serve is said
where the capability is used:

- `store: name` on entities, caches and `File` fields (database, cache, files);
- `mail:`, `sms:` and `publishing:` in `application { }`;
- schedules default to the target's own tick, so most apps write nothing.

The entry does not repeat its capabilities; a use the package does not
provide is a compile error.

**One connection per resource, not per vendor account.** "One Cloudflare
connection for files, queues and cron" did not hold: natively R2 needs S3
keys and Queues an API token, cron has no credential, and two buckets need
two settings. Cloudflare's bindings are one per resource too.

### Provider-specific features

Three ways in, so the capability API stays small: **typed connection
settings** the package declares (Dapr's metadata, typed); **standard events
back** (`MailBounced`, `SmsFailed`) that a package maps its webhooks onto;
and **the raw API**, callable with `domains:` on the connection.

### How a package provides a capability

The package index gains `provides:`, naming a file per capability. A
`provide` declaration maps each standard operation onto one of the package's
operations, using only field paths (`message.subject`, `settings.x`,
`response.MessageID`), literals, lists (`[message.to]`), nested blocks, and
enum tables. There are no expressions, calls or loops, so a mapping is data
the compiler can check on both sides: every source is a field of the
standard type, every target a field of the provider's type.

What needs real code is built into the runtime, never shipped in a package:

- protocol drivers: PostgreSQL, Redis, SMTP, S3 with presigning;
- Workers bindings (R2, Queues);
- wire formats: JSON, form, AWS;
- credential schemes: bearer, header, basic, AWS SigV4;
- webhook checks: Stripe, Standard Webhooks, HMAC, basic auth, Twilio, AWS SNS;
- schedule ticks.

So packages stay declarations only.

### Conformance and fakes

Each capability has a **conformance suite**: standard cases a provider must
pass. For mail: a receipt for a good message, the mapped failure for a bad
address, a mapped bounce, and the idempotency claim holding. `jadpo provider conform <package>
<capability>` runs it against the provider's sandbox, or an emulator (MinIO
for S3). The registry lists a package as providing a capability only with a
passing report pinned to its digest; a local package runs it in scheduled
CI, like area 11's contract runs.

Tests use **one fake per capability**, whatever the provider: `mail.sent`,
`sms.sent`, an in-memory file store, `published` for outward events. Raw
API calls keep the package's generated fake (area 11 question 12).

### Development

In `jadpo dev` nothing leaves the machine. Mail and SMS go to the local
inbox, files to local disk with locally signed links, and published events
to a local log in the dev console. The database runs its real engine (area
5). The cache runs in memory, which narrows area 5's "same engine in every
environment" for caches only: a cache passing the same conformance suite
cannot change what a request returns, and an unreachable cache already
never fails one.

### Reports and review (area 18)

The manifest records each connection's driver or package, the capabilities
it serves, its hosts or binding, and its targets. The network report labels
hosts by capability. The personal-data report lists mail, SMS, file and
publishing connections as receiving the marked fields of what they carry. A
capability may define an `erase` mapping; with one, area 15's `PersonErased`
subscriber is generated, without one its acceptance applies. Moving a
capability to another package is a reviewed diff, with no new acceptance.

### Targets

Each driver and package transport lists its targets; `jadpo check` checks
every deployable against them and names the alternative. On Cloudflare
Workers (checked 2026-10-13):

- **Raw TCP exists**, with STARTTLS; port 25 is blocked
  ([TCP sockets](https://developers.cloudflare.com/workers/runtime-apis/tcp-sockets/)).
  SMTP submission on 587 can work; port 25 cannot.
- **No in-process timer.** The tick is Cloudflare cron, generated into the
  Wrangler config; it also sweeps the outbox.
- **No local disk**, and bindings exist only on Workers (natively, R2 goes
  over S3 and Queues over HTTP).

## The eight packages modelled

Fragments; package files in `providers/<name>/`, app lines in `app.jadpo`.

**Postmark (mail).**

```text
package Postmark.v1 {
    protocol: Protocol.json_http
    hosts: { live: "https://api.postmarkapp.com" }
    sandbox: Sandbox.test_credential            // Postmark's test token, not a host
    credential: { scheme: CredentialScheme.header  header: "X-Postmark-Server-Token" }
    webhooks: { check: SignatureScheme.basic_auth }
    error_code: "ErrorCode"
    settings: { message_stream: Text { default: "outbound" } }
    resources: { email: "email.jadpo" }
    provides: { mail: "mail.jadpo" }
}

provide mail {
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
        failures: { inactive_recipient: MailFailure.recipient_rejected }
    }
    events: {
        bounced: {
            on: BounceWebhook
            recipient: event.Email
            kind: { from: event.Type  values: { hard_bounce: BounceKind.permanent  soft_bounce: BounceKind.temporary } }
        }
    }
}

// app.jadpo
connections: { postmark: { package: Postmark.v1  credential: config.postmark_token  message_stream: "outbound" } }
mail: { connection: postmark  from: config.mail_sender }

// code
emit WelcomeMail { to: user.email  locale: user.locale  display_name: user.display_name  sign_in_link: config.app_url }
```

**Amazon SES (mail plus raw API).**

```text
package AmazonSes.v2 {
    protocol: Protocol.aws_rest_json
    hosts: { live: "https://email.{region}.amazonaws.com" }
    credential: { scheme: CredentialScheme.aws_sigv4  service: "ses" }
    webhooks: { check: SignatureScheme.aws_sns }
    settings: { region: AwsRegion  configuration_set: Text? }
    resources: { outbound_emails: "outbound_emails.jadpo"  suppressed_destinations: "suppression.jadpo" }
    provides: { mail: "mail.jadpo" }
}

provide mail {
    send: {
        call: outbound_emails.send
        input: {
            FromEmailAddress: message.from
            Destination: { ToAddresses: [message.to] }
            Content: { Simple: { Subject: { Data: message.subject }  Body: { Html: { Data: message.html } } } }
        }
        receipt: response.MessageId
        failures: { message_rejected: MailFailure.rejected }
    }
    erase: { call: suppressed_destinations.delete  input: { EmailAddress: person.email } }
}

// app.jadpo
ses: { package: AmazonSes.v2  domains: [accounts]  credential: { key_id: config.aws_key_id  secret: config.aws_secret }  region: AwsRegion.eu_west_2 }
mail: { connection: ses  from: config.mail_sender }

// code (raw API, from the accounts domain)
var entry: AmazonSes.SuppressedDestination = attempt ses.suppressed_destinations.get(lookup)
```

**Cloudflare R2 (files).** A preset over the built-in S3 driver.

```text
package CloudflareR2.v1 {
    settings: { account_id: Text  bucket: Text }
    provides: {
        files: {
            driver: Driver.s3
            host: "https://{account_id}.r2.cloudflarestorage.com"
            region: "auto"
            binding: Binding.r2                 // Workers: reads and writes through the bucket binding
        }
    }
}

// app.jadpo
avatars: { package: CloudflareR2.v1  account_id: config.cloudflare_account  bucket: "avatars"  credential: { key_id: config.r2_key_id  secret: config.r2_secret } }

// code
avatar: File? { store: avatars  max_size: 5mb  types: [FileType.png, FileType.jpeg] }
```

**Redis (cache).** A built-in driver, no package:
`cache: { driver: Driver.redis  url: config.redis_url }`, used as
`caches: { records: { store: cache  keep: 1h } }`.

**Cloudflare Queues (publishing).**

```text
package CloudflareQueues.v1 {
    protocol: Protocol.json_http
    hosts: { live: "https://api.cloudflare.com/client/v4" }
    credential: { scheme: CredentialScheme.bearer }
    settings: { account_id: Text  queue_id: Text }
    resources: { messages: "messages.jadpo" }
    provides: { publishing: "publishing.jadpo" }
}

provide publishing {
    publish: { call: messages.push  input: { body: envelope } }   // envelope: id, type, time, traceparent, data
    binding: Binding.queue
}

// app.jadpo
analytics_queue: { package: CloudflareQueues.v1  credential: config.cloudflare_token  account_id: config.cloudflare_account  queue_id: config.analytics_queue }
publishing: { analytics_queue: [planning.TodoCompleted, accounts.UserSignedUp] }

// code: unchanged; the event is emitted as usual and also published after commit
emit TodoCompleted { todo_id: todo.id }
```

**Cloudflare cron (schedules).** Not a package: a built-in tick and the
Workers target's default, so `app.jadpo` says nothing and
`schedule overdue_reminders { every: 15m  run: queue_overdue_reminders() }`
just works. Other hosts may name `tick: { driver: Driver.external_tick }`
and `schedules: { connection: tick }`.

**Twilio (SMS).**

```text
package Twilio.v2010_04_01 {
    protocol: Protocol.form_http
    hosts: { live: "https://api.twilio.com" }
    sandbox: Sandbox.test_credential
    credential: { scheme: CredentialScheme.basic }
    webhooks: { check: SignatureScheme.twilio }
    settings: { account_sid: Text }
    resources: { messages: "messages.jadpo" }       // path /2010-04-01/Accounts/{account_sid}/Messages.json
    provides: { sms: "sms.jadpo" }
}

provide sms {
    send: {
        call: messages.create
        input: { To: message.to  From: message.from  Body: message.text  StatusCallback: connection.webhook_url }
        receipt: response.sid
        failures: { invalid_to_number: SmsFailure.recipient_rejected }
    }
    events: {
        delivered: { on: StatusCallback  when: { from: event.MessageStatus  values: { delivered: true } } }
    }
}

// app.jadpo
twilio: { package: Twilio.v2010_04_01  account_sid: config.twilio_sid  credential: config.twilio_auth  environment: config.sms_environment }
sms: { connection: twilio  from: config.sms_sender }

// code
emit SignInCodeSms { to: user.phone  locale: user.locale  code: code }
```

**Stripe (plain API).** No capability, unchanged from area 11: a
connection with `domains: [payments]`, called as
`attempt stripe.payment_intents.create(request) { card_declined: PaymentDeclined }`.

## What the exercise revealed

The design held for mail, SMS, files, cache and publishing, and Stripe
needed nothing. What it could not express cleanly:

1. **Sandboxes that are credentials.** Postmark's sandbox is a test token
   and Twilio's a set of test credentials, on the live host; area 11's
   `environment:` only picks a host.
2. **Hosts with a placeholder.** R2 (`{account_id}`) and SES (`{region}`).
   Proposed: one placeholder in one host label, from a typed setting; the
   network report shows the pattern, the runtime checks the resolved host.
3. **Two-part credentials.** SigV4 needs a key id and a secret; area 11
   decided one `credential: config.x`. The scheme should decide the shape.
4. **Webhook checks area 11 does not list.** Postmark has no signature, only
   basic auth and an IP allowlist
   ([Postmark](https://postmarkapp.com/developer/webhooks/webhooks-overview));
   Twilio signs with HMAC-SHA1 over the URL and sorted parameters
   ([Twilio](https://www.twilio.com/docs/usage/webhooks/webhooks-security)),
   not Standard Webhooks as area 11 said; SES bounces arrive through SNS,
   certificate-signed with a subscription handshake.
5. **The webhook address as a value.** Twilio wants a status callback
   address on every send: mappings need `connection.webhook_url`.
6. **Numeric error codes.** Postmark's `ErrorCode: 406` needs `code: "406"`
   on the failure; area 4's override covers it, clumsily.
7. **No idempotency key.** Neither Postmark's nor SES's send documents one,
   so a timed-out send is `outcome_unknown`, not retried; the review should
   say which mail packages lack keys.
8. **Upload slots need S3 keys even on Workers.** R2 presigned URLs come
   only from the S3 API
   ([R2](https://developers.cloudflare.com/r2/api/s3/presigned-urls/)); the
   binding only speeds up reads and writes.
9. **Cron is not a provider.** No host, credential or operation: it is a
   feature of the Workers target, so schedule ticks are built in.
10. **"One Cloudflare connection" did not fit**: one connection per resource.
11. **Batches.** Publishing many events in one call needs a mapping over a
    list; the first build publishes one at a time.
