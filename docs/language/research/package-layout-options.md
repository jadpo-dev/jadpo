# Package layout options

Status: research, 2026-10-16, for
[packages and extension points](packages-and-extension-proposals.md).
Nothing here is decided. Sketches are illustrative; vendor details marked
"(to check)" have not been verified against the vendor's documentation.

Each package scenario was designed separately, and the `package` index grew
a different set of keys for each kind: `driver:`, `fixed:` and `settings:`
for a preset; `protocol:`, `hosts:`, `idempotency:`, `error_code:` and
`webhooks:` for an API; `service:` for a container; `provides:` naming
files. A vendor with both storage and an API (Cloudflare) fitted none of
them. Making everything a driver, one per package, kept one set of keys but
split one vendor across several packages and kept the `fixed:`/`settings:`
split.

This page sets out three layouts, each shown on the same fifteen scenarios,
and recommends one.

## What all three keep

These parts are the same in every candidate, so the candidates differ only
where it matters:

- **The index is the same in every package.** `package.jadpo` holds one
  declaration, `package Name.vN { }`, with a `///` description and three
  keys: `licence:`, `jadpo:` (the lowest version it needs) and
  `requires:`. Nothing about hosts, drivers or containers goes in it, so a
  library's index and Stripe's index look the same.
- **Code is laid out as `shared/` is**: `types/`, `failures/`, `libraries/`,
  plus `emails/` and `translations/` for a library's extras, `public`
  marking the API (decided).
- **An API's parts** are `resources/`, `events/` and `recordings/` (decided).
- **A capability mapping** is a file `provides/<capability>.jadpo` holding
  `provide <capability> { }`, found by name. This replaces area 11's
  `provides: { email: "email.jadpo" }` in the index, which already breaks
  the decided "the index lists no files".
- **Use sites are unchanged**: `stripe.payment_intents.create(request)`,
  `store: avatars`, `email: { connection: postmark }`,
  `on: Stripe.PaymentIntentSucceeded`.
- **A sandbox is a host.** `hosts: { live: sandbox: }` may name the same host
  twice, which is what a sandbox that is a test credential is (Stripe,
  Postmark, Twilio): the connection's `environment:` says which kind of
  credential it holds. Area 11 question 25's `Sandbox.test_credential` is no
  longer needed.
- **Webhook checks and missing idempotency** are values, not shapes:
  `webhooks: { check: SignatureScheme.basic_auth }`, and an API with no
  idempotency key leaves out `idempotency:`, so every changing call that
  times out is `outcome_unknown` and the review lists it.
- **Requirements, versions side by side and raw drivers** (scenarios 12, 13
  and 15) work the same everywhere; each candidate's table shows them.

## The concepts each candidate needs

| Concept | What it is | A | B | C |
|---|---|---|---|---|
| Package index | `package.jadpo`: name, version, description, licence, Jadpo version, requirements | yes | yes | yes |
| Code folders | `types/`, `failures/`, `libraries/`, `emails/`, `translations/`, as `shared/` | yes | yes | yes |
| Resource | A group of an API's operations | yes | yes | yes |
| Event | A webhook payload published as a Jadpo event | yes | yes | yes |
| Capability mapping | `provides/<capability>.jadpo` | yes | yes | yes |
| Built-in driver | Native code from a Jadpo release: PostgreSQL, Redis, S3, SMTP, OpenID Connect, HTTP | yes | yes | yes |
| Connection | An entry in `app.jadpo`'s `connections:` | yes | yes | yes |
| Settings | Typed values the app gives a connection (`message_stream:`) | yes | yes | yes |
| Host placeholder | `{account_id}` in a host, filled from a setting | presets and APIs | APIs only (SES's `{region}`) | APIs only |
| Service | `runs:` a container the app's connection reaches | yes | yes | yes |
| Connection template | `connections/<name>.jadpo`: one per thing the package reaches, in any driver's keys | yes | no | no |
| Template reference | `from: Package.vN.template` in the app, `connection:` on every resource | yes | no | no |
| Provider declaration | `provider.jadpo`: at most one per package, one fixed set of keys | no | yes | yes |
| Vendor driver | A built-in driver for one vendor (`Driver.cloudflare_r2`) | no | no | yes |
| **Total** | | **12** | **11** | **12** |

The count hides the larger difference: in A a template is written in the
keys of whichever driver it uses, so a package author learns one key set
per driver; in B and C a package has one key set.

## Candidate A: connection templates

**Rule:** each thing a package connects to is a template in
`connections/<name>.jadpo`, written as the connection would be written in
`app.jadpo`. The keys it writes are fixed; the driver's keys it leaves out,
and its own `settings:`, are the app's to give. An app connects with
`from: Package.vN.template`. Every resource names the template it is
called through.

```text
cloudflare/v4/
    package.jadpo
    connections/r2.jadpo            connection r2 { driver: Driver.s3 ... }
    connections/api.jadpo           connection api { driver: Driver.json_http ... }
    resources/zones.jadpo           resource zones { connection: api ... }
    types/  failures/  events/  libraries/  provides/  recordings/
```

### A3: Stripe, imported from OpenAPI, with webhooks

```text
// packages/stripe/v2024_06_20/package.jadpo
/// Stripe's API at version 2024-06-20, imported from Stripe's OpenAPI file.
package Stripe.v2024_06_20 {
    licence: Licence.mit
    jadpo: "1.0"
    requires: []
}

// packages/stripe/v2024_06_20/connections/api.jadpo
/// Stripe's REST API. Test mode is a test key on the live host.
connection api {
    driver: Driver.form_http
    hosts: {
        live: "https://api.stripe.com"
        sandbox: "https://api.stripe.com"
    }
    credential: CredentialScheme.bearer
    idempotency: {
        header: "Idempotency-Key"
    }
    error_code: "error.code"
    webhooks: {
        check: SignatureScheme.stripe
        event_id: "id"
        event_type: "type"
    }
    imported: {
        file: "openapi/spec3.json"
        digest: "sha256:…"
    }
}

// packages/stripe/v2024_06_20/resources/payment_intents.jadpo
resource payment_intents {
    connection: api
    create: {
        method: HttpMethod.post
        path: "/v1/payment_intents"
        input: CreatePaymentIntent
        output: PaymentIntent
        changes: true
        idempotent: true
        failures: [CardDeclined]
        source: PackageSource.imported
    }
}

// packages/stripe/v2024_06_20/events/payment_intent_succeeded.jadpo
/// A payment intent was paid.
public type PaymentIntentSucceeded = Event {
    wire_name: "payment_intent.succeeded"
    object: PaymentIntent
}

// app.jadpo
stripe: {
    from: Stripe.v2024_06_20.api
    domains: [payments]
    credential: config.stripe_secret_key
    environment: config.stripe_environment
    webhooks: {
        secret: config.stripe_webhook_secret
    }
}

// use
var intent: Stripe.PaymentIntent = attempt stripe.payment_intents.create(request) {
    card_declined: PaymentDeclined
}
```

### A4: a courier with awkward wire formats

```text
// shared/providers/acme_couriers/v1/package.jadpo
/// Acme Couriers' booking and tracking API, written from its HTML docs.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
    requires: []
}

// shared/providers/acme_couriers/v1/connections/api.jadpo
connection api {
    driver: Driver.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
    credential: CredentialScheme.bearer
    error_code: "error.code"
}

// shared/providers/acme_couriers/v1/types/booking.jadpo
public type Booking = Object {
    id: Text
    collect_at: Instant {
        wire: Wire.unix_seconds
    }
    price: Money {
        wire: Wire.minor_units
        currency: Currency.gbp
    }
    weight: Decimal {
        scale: 2
        wire: Wire.text
    }
}

// shared/providers/acme_couriers/v1/resources/tracking.jadpo
resource tracking {
    connection: api
    get: {
        method: HttpMethod.get
        path: "/v1/parcels/{id}/tracking"
        input: TrackingLookup
        wire_output: TrackingTable
        output: List<TrackingEvent>
        decode: tables.to_events
        output_at: "data"
        failed_when: { path: "ok"  equals: false }
        changes: false
        source: PackageSource.written
        docs: "https://docs.acme-couriers.example/api#tracking"
    }
}

// shared/providers/acme_couriers/v1/libraries/tables.jadpo
library tables {
    /// Turns the columnar tracking table into one event per row.
    function to_events(table: TrackingTable) -> List<TrackingEvent> { ... }
}

// app.jadpo
couriers: {
    from: AcmeCouriers.v1.api
    domains: [shipping]
    credential: config.couriers_api_key
    environment: config.couriers_environment
}

// use
var events: List<AcmeCouriers.TrackingEvent> = attempt couriers.tracking.get(lookup) {
    parcel_not_found: ParcelUnknown
}
```

### A5: Postmark filling `email`

```text
// packages/postmark/v1/connections/api.jadpo
/// Postmark's email API. The sandbox is Postmark's test token on the live host.
connection api {
    driver: Driver.json_http
    hosts: {
        live: "https://api.postmarkapp.com"
        sandbox: "https://api.postmarkapp.com"
    }
    credential: {
        scheme: CredentialScheme.header
        header: "X-Postmark-Server-Token"
    }
    error_code: "ErrorCode"
    webhooks: {
        check: SignatureScheme.basic_auth
        event_id: "ID"
        event_type: "RecordType"
    }
    settings: {
        message_stream: Text {
            default: "outbound"
        }
    }
}
// No `idempotency:`: Postmark's send takes no key.

// packages/postmark/v1/provides/email.jadpo
// The template is the one its calls belong to (`email` is on `api`).
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
    events: {
        bounced: {
            on: BounceWebhook
            recipient: event.Email
            kind: {
                from: event.Type
                values: {
                    hard_bounce: BounceKind.permanent
                    soft_bounce: BounceKind.temporary
                }
            }
        }
    }
}

// app.jadpo
postmark: {
    from: Postmark.v1.api
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

// use
emit WelcomeEmail { to: user.email  locale: user.locale  display_name: user.display_name }
```

### A6: presets over built-in drivers

```text
// packages/cloudflare/v4/connections/r2.jadpo
/// An R2 bucket, through R2's S3-compatible API.
connection r2 {
    driver: Driver.s3
    host: "https://{account_id}.r2.cloudflarestorage.com"
    region: "auto"
    binding: Binding.r2             // on Workers, reads and writes use the bucket binding
    settings: {
        account_id: Text
    }
}
// `bucket:` and `credential:` are S3 keys the template leaves out: the app gives them.

// packages/neon/v1/connections/database.jadpo
/// A Neon database through its pooled endpoint.
connection database {
    driver: Driver.postgres
    tls: TlsMode.verify_full
    pooling: Pooling.transaction    // Neon's pooler runs PgBouncer in transaction mode (to check)
}

// packages/gmail/v1/connections/smtp.jadpo
/// Gmail's SMTP submission server, signed in with an app password.
connection smtp {
    driver: Driver.smtp
    host: "smtp.gmail.com"
    port: 587
    tls: TlsMode.starttls
}

// app.jadpo
avatars: {
    from: Cloudflare.v4.r2
    account_id: config.cloudflare_account
    bucket: "avatars"
    credential: {
        key_id: config.r2_key_id
        secret: config.r2_secret
    }
}
main: {
    from: Neon.v1.database
    url: config.database_url
}
gmail: {
    from: Gmail.v1.smtp
    credential: {
        username: config.gmail_user
        password: config.gmail_app_password
    }
}

// use
avatar: File? { store: avatars  max_size: 5mb }
```

The Neon and Gmail packages each hold three or four fixed values; the
index, version, lock entry and registry entry around them are larger than
what they carry.

### A7: one vendor, several connectable things

```text
cloudflare/v4/
    package.jadpo
    connections/r2.jadpo            as A6
    connections/api.jadpo           connection api { driver: Driver.json_http ... }
    resources/zones.jadpo           resource zones { connection: api ... }

// packages/cloudflare/v4/connections/api.jadpo
/// Cloudflare's management API.
connection api {
    driver: Driver.json_http
    hosts: {
        live: "https://api.cloudflare.com/client/v4"
    }
    credential: CredentialScheme.bearer
    error_code: "errors.0.code"
}

supabase/v2/
    package.jadpo
    connections/database.jadpo      connection database { driver: Driver.postgres  pooling: Pooling.transaction }
    connections/storage.jadpo       connection storage { driver: Driver.s3  host: "https://{project_ref}.supabase.co/storage/v1/s3" ... } (to check)
    connections/api.jadpo           connection api { driver: Driver.json_http  hosts: { live: "https://{project_ref}.supabase.co" } ... }
    resources/auth_users.jadpo      resource auth_users { connection: api ... }

// app.jadpo
avatars: {
    from: Cloudflare.v4.r2
    account_id: config.cloudflare_account
    bucket: "avatars"
    credential: { key_id: config.r2_key_id  secret: config.r2_secret }
}
cloudflare: {
    from: Cloudflare.v4.api
    domains: [publishing]
    credential: config.cloudflare_token
}

// use
attempt cloudflare.zones.purge_cache(request)
```

### A8: Gotenberg as a container service

```text
// packages/gotenberg/v8/connections/api.jadpo
/// Gotenberg's HTTP API, run by Jadpo as a container beside the app.
connection api {
    driver: Driver.multipart_http
    runs: {
        image: "gotenberg/gotenberg:8@sha256:…"
        port: 3000
        asks: ["fonts.googleapis.com", "fonts.gstatic.com"]
    }
}

// packages/gotenberg/v8/resources/chromium.jadpo
resource chromium {
    connection: api
    convert_html: {
        method: HttpMethod.post
        path: "/forms/chromium/convert/html"
        input: HtmlDocument
        output: Bytes
        changes: false
        source: PackageSource.written
        docs: "https://gotenberg.dev/docs/routes"
    }
}

// app.jadpo
pdfs: {
    from: Gotenberg.v8.api
    domains: [billing]
    outbound: ["fonts.googleapis.com", "fonts.gstatic.com"]
}

// use
var pdf: Bytes = attempt pdfs.chromium.convert_html(document)
```

`runs:` takes the place of `hosts:`. The app's `outbound:` grants what the
package asks; on Workers, which runs no containers, the app writes
`host: config.pdf_url` instead and nothing runs.

### A: the other scenarios

| # | Package | `app.jadpo` | Use |
|---|---|---|---|
| 1 Library | `postcodes/v2/`: index, `types/uk_postcode.jadpo`, `failures/invalid_postcode.jadpo`, `libraries/uk.jadpo`; no `connections/` | nothing | `use packages/postcodes`, then `Postcodes.uk.parse(text)`; `postcode: Postcodes.UkPostcode` on an entity |
| 2 Library with extras | `brand/v1/`: as 1, plus `emails/button.jadpo` with `button.html` (a typed partial) and `translations/en_gb.json` for its failure messages | nothing | `{{ include Brand.button(label: t.open_app, url: app_url) }}` |
| 9 From a Jadpo app | `jadpo package import jadpo ../orders/api/main.api` writes `connections/api.jadpo` with no `hosts:`, bearer, `Idempotency-Key`, Jadpo's error shape, one resource per route group | `orders: { from: Orders.v1.api  host: config.orders_url  credential: config.orders_key  domains: [fulfilment] }` | `attempt orders.orders.create(request)` |
| 10 Own integration | `shared/providers/acme_couriers/v1/`, the same files as installed | `from: AcmeCouriers.v1.api` | `use shared/providers/acme_couriers` |
| 11 Quirks | Same-host `sandbox:`; `check: SignatureScheme.basic_auth` (Postmark) or `.twilio`; no `idempotency:` | `environment:` picks test or live credential | unchanged |
| 12 Two versions | Two folders, `stripe/v2024_06_20/` and `stripe/v2025_01_01/` | `stripe: { from: Stripe.v2024_06_20.api }`, `stripe_next: { from: Stripe.v2025_01_01.api }` | `Stripe.v2025_01_01.PaymentIntent` while both are used |
| 13 Requirement | `getaddress/v1/package.jadpo` has `requires: [Postcodes.v2]`; its input type uses `Postcodes.UkPostcode` | `jadpo package add` locks both | `attempt addresses.lookup.by_postcode(postcode)` |
| 14 Sign-in preset | `google/v1/connections/sign_in.jadpo`: `connection sign_in { driver: Driver.oidc  issuer: "https://accounts.google.com" }` | `google: { from: Google.v1.sign_in  client_id: config.google_client_id  credential: config.google_client_secret }` and `sign_in: { connections: [google] }` | `/auth/google/start`, generated |
| 15 Raw driver | none | `minio: { driver: Driver.s3  host: config.minio_url  region: "us-east-1"  path_style: true  bucket: "uploads"  credential: { ... } }` | `store: minio` |

## Candidate B: packages describe APIs; drivers are configured in the app

**Rule:** a package is `shared/`-style code under an index. If it
describes an API, it has one `provider.jadpo` saying how to reach and call
it, with `resources/` and `events/`; if it fills a capability, it has
`provides/<capability>.jadpo`. Anything spoken in a protocol Jadpo has a
built-in driver for (PostgreSQL, Redis, S3, SMTP, OpenID Connect) is never
a package: the app writes it as a `driver:` connection in `app.jadpo`.

So there are two kinds of package, as decided: a library, and a provider,
which is a library plus one API. A package describes at most one API because
one API is one connection (one set of hosts and credentials, area 11
question 20) and is versioned as one; a vendor with two APIs under
different credentials (Shopify's Admin and Storefront APIs) is two packages.

```text
stripe/v2024_06_20/
    package.jadpo                   same three keys as every package
    provider.jadpo                  provider { protocol hosts credential idempotency error_code webhooks settings runs imported }
    resources/payment_intents.jadpo
    types/  failures/  events/  libraries/  provides/  recordings/
```

The `provider` declaration has one fixed set of keys, every one optional
except `protocol:`:

| Key | Says |
|---|---|
| `protocol:` | How bodies are sent: `Protocol.json_http`, `.form_http`, `.multipart_http`, `.aws_json` |
| `hosts:` | `live:` and `sandbox:`; a host may hold one placeholder from a setting. Left out, the app gives `host:` |
| `runs:` | A container Jadpo runs instead of a fixed host: `image:`, `port:`, `asks:` |
| `credential:` | The scheme, which decides the shape the app gives |
| `idempotency:` | Where the key goes; left out, the API takes none |
| `error_code:` | Where the provider's error code is |
| `webhooks:` | `check:`, `event_id:`, `event_type:` |
| `settings:` | Typed values the app gives (`message_stream:`) |
| `imported:` | The description it was imported from, and its digest |

### B3: Stripe, imported from OpenAPI, with webhooks

```text
// packages/stripe/v2024_06_20/package.jadpo
/// Stripe's API at version 2024-06-20, imported from Stripe's OpenAPI file.
package Stripe.v2024_06_20 {
    licence: Licence.mit
    jadpo: "1.0"
    requires: []
}

// packages/stripe/v2024_06_20/provider.jadpo
/// Stripe's REST API. Test mode is a test key on the live host.
provider {
    protocol: Protocol.form_http
    hosts: {
        live: "https://api.stripe.com"
        sandbox: "https://api.stripe.com"
    }
    credential: CredentialScheme.bearer
    idempotency: {
        header: "Idempotency-Key"
    }
    error_code: "error.code"
    webhooks: {
        check: SignatureScheme.stripe
        event_id: "id"
        event_type: "type"
    }
    imported: {
        file: "openapi/spec3.json"
        digest: "sha256:…"
    }
}

// packages/stripe/v2024_06_20/resources/payment_intents.jadpo
resource payment_intents {
    create: {
        method: HttpMethod.post
        path: "/v1/payment_intents"
        input: CreatePaymentIntent
        output: PaymentIntent
        changes: true
        idempotent: true
        failures: [CardDeclined]
        source: PackageSource.imported
    }
}

// packages/stripe/v2024_06_20/events/payment_intent_succeeded.jadpo
/// A payment intent was paid.
public type PaymentIntentSucceeded = Event {
    wire_name: "payment_intent.succeeded"
    object: PaymentIntent
}

// app.jadpo: unchanged from area 11
stripe: {
    package: Stripe.v2024_06_20
    domains: [payments]
    credential: config.stripe_secret_key
    environment: config.stripe_environment
    webhooks: {
        secret: config.stripe_webhook_secret
    }
}

// use
var intent: Stripe.PaymentIntent = attempt stripe.payment_intents.create(request) {
    card_declined: PaymentDeclined
}
```

### B4: a courier with awkward wire formats

```text
// shared/providers/acme_couriers/v1/provider.jadpo
/// Acme Couriers' booking and tracking API, written from its HTML docs.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
    credential: CredentialScheme.bearer
    error_code: "error.code"
}

// shared/providers/acme_couriers/v1/types/booking.jadpo
public type Booking = Object {
    id: Text
    collect_at: Instant {
        wire: Wire.unix_seconds
    }
    price: Money {
        wire: Wire.minor_units
        currency: Currency.gbp
    }
    weight: Decimal {
        scale: 2
        wire: Wire.text
    }
}

// shared/providers/acme_couriers/v1/resources/tracking.jadpo
resource tracking {
    get: {
        method: HttpMethod.get
        path: "/v1/parcels/{id}/tracking"
        input: TrackingLookup
        wire_output: TrackingTable
        output: List<TrackingEvent>
        decode: tables.to_events
        output_at: "data"
        failed_when: { path: "ok"  equals: false }
        changes: false
        source: PackageSource.written
        docs: "https://docs.acme-couriers.example/api#tracking"
    }
}

// shared/providers/acme_couriers/v1/libraries/tables.jadpo
library tables {
    /// Turns the columnar tracking table into one event per row.
    function to_events(table: TrackingTable) -> List<TrackingEvent> { ... }
}

// app.jadpo
couriers: {
    package: AcmeCouriers.v1
    domains: [shipping]
    credential: config.couriers_api_key
    environment: config.couriers_environment
}

// use
var events: List<AcmeCouriers.TrackingEvent> = attempt couriers.tracking.get(lookup) {
    parcel_not_found: ParcelUnknown
}
```

### B5: Postmark filling `email`

```text
// packages/postmark/v1/provider.jadpo
/// Postmark's email API. The sandbox is Postmark's test token on the live host.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.postmarkapp.com"
        sandbox: "https://api.postmarkapp.com"
    }
    credential: {
        scheme: CredentialScheme.header
        header: "X-Postmark-Server-Token"
    }
    error_code: "ErrorCode"
    webhooks: {
        check: SignatureScheme.basic_auth
        event_id: "ID"
        event_type: "RecordType"
    }
    settings: {
        message_stream: Text {
            default: "outbound"
        }
    }
}
// No `idempotency:`: Postmark's send takes no key.

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
    events: {
        bounced: {
            on: BounceWebhook
            recipient: event.Email
            kind: {
                from: event.Type
                values: {
                    hard_bounce: BounceKind.permanent
                    soft_bounce: BounceKind.temporary
                }
            }
        }
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

// use
emit WelcomeEmail { to: user.email  locale: user.locale  display_name: user.display_name }
subscriber note_bounce { on: EmailBounced  run: mark_address_bad(event) }
```

### B6: what were presets are driver connections

No package. The app writes the driver's keys, as it already does for a
self-hosted MinIO (scenario 15):

```text
// app.jadpo
avatars: {
    driver: Driver.s3
    host: "https://3f9c0d….r2.cloudflarestorage.com"    // or config.r2_host when it varies by environment
    region: "auto"
    binding: Binding.r2                                  // on Workers, reads and writes use the bucket binding
    bucket: "avatars"
    credential: {
        key_id: config.r2_key_id
        secret: config.r2_secret
    }
}
main: {
    driver: Driver.postgres
    url: config.database_url
    tls: TlsMode.verify_full
    pooling: Pooling.transaction                         // Neon's pooled endpoint (to check)
}
gmail: {
    driver: Driver.smtp
    host: "smtp.gmail.com"
    port: 587
    tls: TlsMode.starttls
    credential: {
        username: config.gmail_user
        password: config.gmail_app_password
    }
}
email: {
    connection: gmail
    from: config.mail_sender
}

// use
avatar: File? { store: avatars  max_size: 5mb }
```

What the presets carried (a host pattern, `region: "auto"`, a TLS mode, a
pooling mode, a port) is two to four lines per connection, and the host is
now written where the reviewer reads it. No placeholder is needed for these:
the app writes the real host or a setting.

### B7: one vendor, several connectable things

The vendor's API is a package; its storage and database are driver
connections. Nothing new is needed.

```text
// packages/cloudflare/v4/provider.jadpo
/// Cloudflare's management API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.cloudflare.com/client/v4"
    }
    credential: CredentialScheme.bearer
    error_code: "errors.0.code"
}

// packages/supabase/v2/provider.jadpo
/// A Supabase project's own API: auth administration and storage buckets.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://{project_ref}.supabase.co"
    }
    credential: {
        scheme: CredentialScheme.header
        header: "apikey"                                 // (to check)
    }
    settings: {
        project_ref: Text
    }
}

// app.jadpo
avatars: {
    driver: Driver.s3
    host: "https://3f9c0d….r2.cloudflarestorage.com"
    region: "auto"
    binding: Binding.r2
    bucket: "avatars"
    credential: { key_id: config.r2_key_id  secret: config.r2_secret }
}
cloudflare: {
    package: Cloudflare.v4
    domains: [publishing]
    credential: config.cloudflare_token
}
main: {
    driver: Driver.postgres
    url: config.supabase_db_url
    pooling: Pooling.transaction
}
uploads: {
    driver: Driver.s3
    host: "https://abcd1234.supabase.co/storage/v1/s3"    // (to check)
    region: "eu-west-2"
    bucket: "uploads"
    credential: { key_id: config.supabase_s3_key_id  secret: config.supabase_s3_secret }
}
supabase: {
    package: Supabase.v2
    project_ref: "abcd1234"
    domains: [accounts]
    credential: config.supabase_service_key
}

// use
attempt cloudflare.zones.purge_cache(request)
```

### B8: Gotenberg as a container service

```text
// packages/gotenberg/v8/provider.jadpo
/// Gotenberg's HTTP API, run by Jadpo as a container beside the app.
provider {
    protocol: Protocol.multipart_http
    runs: {
        image: "gotenberg/gotenberg:8@sha256:…"
        port: 3000
        asks: ["fonts.googleapis.com", "fonts.gstatic.com"]
    }
}

// packages/gotenberg/v8/resources/chromium.jadpo
resource chromium {
    convert_html: {
        method: HttpMethod.post
        path: "/forms/chromium/convert/html"
        input: HtmlDocument
        output: Bytes
        changes: false
        source: PackageSource.written
        docs: "https://gotenberg.dev/docs/routes"
    }
}

// app.jadpo
pdfs: {
    package: Gotenberg.v8
    domains: [billing]
    outbound: ["fonts.googleapis.com", "fonts.gstatic.com"]
}

// use
var pdf: Bytes = attempt pdfs.chromium.convert_html(document)
```

Where the API is comes from one of three places, and only one:
`hosts:` (it is on the internet), `runs:` (Jadpo runs it), or neither, when
the app gives `host:` (another Jadpo app, a self-hosted product, or a
service on Workers, which runs no containers).

### B: the other scenarios

| # | Package | `app.jadpo` | Use |
|---|---|---|---|
| 1 Library | `postcodes/v2/`: index, `types/uk_postcode.jadpo`, `failures/invalid_postcode.jadpo`, `libraries/uk.jadpo`; no `provider.jadpo` | nothing | `use packages/postcodes`, then `Postcodes.uk.parse(text)`; `postcode: Postcodes.UkPostcode` on an entity |
| 2 Library with extras | `brand/v1/`: as 1, plus `emails/button.jadpo` with `button.html` (a typed partial) and `translations/en_gb.json` for its failure messages | nothing | `{{ include Brand.button(label: t.open_app, url: app_url) }}` |
| 9 From a Jadpo app | `jadpo package import jadpo ../orders/api/main.api` writes `provider.jadpo` with no `hosts:`, bearer, `Idempotency-Key`, Jadpo's error shape, one resource per route group | `orders: { package: Orders.v1  host: config.orders_url  credential: config.orders_key  domains: [fulfilment] }` | `attempt orders.orders.create(request)` |
| 10 Own integration | `shared/providers/acme_couriers/v1/`, the same files as installed | `package: AcmeCouriers.v1` | `use shared/providers/acme_couriers` |
| 11 Quirks | Same-host `sandbox:`; `check: SignatureScheme.basic_auth` (Postmark) or `.twilio`; no `idempotency:` | `environment:` picks test or live credential | unchanged |
| 12 Two versions | Two folders, `stripe/v2024_06_20/` and `stripe/v2025_01_01/` | `stripe: { package: Stripe.v2024_06_20 }`, `stripe_next: { package: Stripe.v2025_01_01 }` | `Stripe.v2025_01_01.PaymentIntent` while both are used |
| 13 Requirement | `getaddress/v1/package.jadpo` has `requires: [Postcodes.v2]`; its input type uses `Postcodes.UkPostcode` | `jadpo package add` locks both | `attempt addresses.lookup.by_postcode(postcode)` |
| 14 Sign-in | No package: Google speaks OpenID Connect, which is a built-in driver, and its discovery document gives the rest | `google: { driver: Driver.oidc  issuer: "https://accounts.google.com"  client_id: config.google_client_id  credential: config.google_client_secret }` and `sign_in: { connections: [google] }` | `/auth/google/start`, generated |
| 15 Raw driver | none | `minio: { driver: Driver.s3  host: config.minio_url  region: "us-east-1"  path_style: true  bucket: "uploads"  credential: { ... } }` | `store: minio` |

## Candidate C: vendor drivers ship with Jadpo

**Rule:** as B for packages. A vendor whose storage, database, mail server
or sign-in needs set-up beyond the generic driver gets a driver of its own
in a Jadpo release (`Driver.cloudflare_r2`, `Driver.neon`, `Driver.gmail`,
`Driver.google`), which fixes those values and declares the vendor's own
settings. This extends area 8's decided built-in sign-in presets to every
driver.

C3, C4, C5, C8 and the table rows for 1, 2, 9 to 13 and 15 are B's
unchanged. Where C differs:

### C6: presets are vendor drivers

```text
// app.jadpo
avatars: {
    driver: Driver.cloudflare_r2
    account_id: config.cloudflare_account
    bucket: "avatars"
    credential: {
        key_id: config.r2_key_id
        secret: config.r2_secret
    }
}
main: {
    driver: Driver.neon
    url: config.database_url
}
gmail: {
    driver: Driver.gmail
    credential: {
        username: config.gmail_user
        password: config.gmail_app_password
    }
}

// use
avatar: File? { store: avatars  max_size: 5mb }
```

### C7: one vendor, several connectable things

```text
// app.jadpo
avatars: {
    driver: Driver.cloudflare_r2
    account_id: config.cloudflare_account
    bucket: "avatars"
    credential: { key_id: config.r2_key_id  secret: config.r2_secret }
}
cloudflare: {
    package: Cloudflare.v4                               // B7's package
    domains: [publishing]
    credential: config.cloudflare_token
}
main: {
    driver: Driver.postgres                              // no Supabase driver: a URL is enough
    url: config.supabase_db_url
    pooling: Pooling.transaction
}
uploads: {
    driver: Driver.s3                                    // or Driver.supabase_storage, if one ships
    host: "https://abcd1234.supabase.co/storage/v1/s3"
    region: "eu-west-2"
    bucket: "uploads"
    credential: { key_id: config.supabase_s3_key_id  secret: config.supabase_s3_secret }
}
supabase: {
    package: Supabase.v2
    project_ref: "abcd1234"
    domains: [accounts]
    credential: config.supabase_service_key
}
```

### C14: sign-in

`google: { driver: Driver.google  client_id: config.google_client_id
credential: config.google_client_secret }`, with `Driver.oidc` for any
other OpenID Connect provider.

Every vendor driver also exists as the generic driver with settings, so C
has two ways to write R2, and someone decides per vendor whether it earns a
driver. A new vendor driver waits for a Jadpo release.

## Comparison

| | A: connection templates | B: APIs only; drivers in the app | C: vendor drivers in releases |
|---|---|---|---|
| Concepts | 12 | 11 | 12 |
| Index keys | 3 (`licence`, `jadpo`, `requires`) | 3 | 3 |
| Key sets a package author learns | One per driver used in a template (HTTP, S3, PostgreSQL, SMTP, OpenID Connect) | One: `provider`'s nine keys | One: `provider`'s nine keys |
| Kinds of package | Library, API, preset, several of these in one, service | Library, provider (one API, which may run in a container) | As B |
| App's connection keys | `from: Package.vN.template` or `driver:` | `package:` or `driver:` (area 11 as decided) | As B |
| Hybrid (Cloudflare, Supabase) | One package, several templates; each resource names its template | The API is a package; storage and database are driver connections | As B, with vendor drivers where they exist |
| Service (Gotenberg) | `runs:` on a template | `runs:` on `provider` | As B |
| Sign-in (Google) | A package with an `oidc` template | `Driver.oidc` with an issuer | `Driver.google` |
| Where vendor know-how lives | In packages | In each app's `app.jadpo` | In Jadpo releases |
| What an agent remembers | Templates and their two kinds of key (fixed, left to the app); `from:`; `connection:` on every resource | "Built-in protocol: a driver in `app.jadpo`. Otherwise an API: a package with `provider.jadpo`." | B's rule, plus checking the vendor-driver list first |
| What a reviewer sees | A package's `connections/` lists what it reaches; the app shows template names, and the host is in the package or the network report | Every driver host written in `app.jadpo`; a package reaches at most one API, named in its `provider.jadpo` | Vendor driver names in `app.jadpo`; their hosts only in the network report |
| Cost | Most rules; presets are packages with versions, lock and registry entries around three lines | Two to four more lines per vendor connection; the agent must know a vendor's settings | Releases for every new vendor; two ways to write each vendor |

**Decided items each would change.** All three: area 11 question 21's
`provides:` key in the index becomes the `provides/` folder, question 25's
test-credential sandbox becomes a same-host `sandbox:`, and the index keys
of area 11's example move out of the index.

- **A** also changes the app's `package:` key to `from:` (area 11 question
  20), adds `connection:` to every resource, and redefines "a package with
  `resources/` is a provider package" (2026-10-16) as "a package with
  `connections/`".
- **B** also drops packages as presets: area 11 question 19's table ("files:
  yes, as S3 presets") and area 16's Files example (`package:
  CloudflareR2.v1`) become an S3 driver connection. Area 8 question 11's
  built-in presets narrow to providers that are not OpenID Connect (GitHub,
  Apple), which need a driver anyway. The proposed "sign-in presets as
  packages" goes.
- **C** changes what B changes, and extends area 8's presets to every
  driver, against the proposal that a new sign-in provider should be a
  package rather than a release.

## Recommendation

**B: a package is code plus at most one API, and every built-in driver is
configured in `app.jadpo`.**

The decisive reason: every scenario that made the index messy was a
package being used to configure a built-in driver. Presets (R2, Neon,
Gmail, Google sign-in) carry two to four fixed values; wrapping them in a
package costs a second shape of package, template names, settings that only
feed a host placeholder, and the fixed-or-supplied split. Without them every
package has the same index and at most one `provider` declaration with one
fixed set of keys, and the hybrid stops being a case: a vendor's storage or
database is a driver connection, its API is a package. The price is two to
four lines per vendor connection in `app.jadpo`, which is also where a
reviewer wants the host.

Open questions for Richard, each with the recommended answer:

1. **Drop presets as packages?** This is the change B rests on. It reverses
   area 11 question 19's "files: as S3 presets" and area 16's R2 example.
   *Recommended:* yes.
2. **Where does an agent learn a vendor's driver settings** (R2's
   `region: "auto"`, Neon's pooling mode)? *Recommended:* each driver's
   language page lists the settings for the common services, and
   `jadpo check` warns when a host it recognises is missing one. Add vendor
   drivers (C) only for a vendor whose flow needs native code, as Apple's
   signed client secret does, or when chunk 10.2 shows agents keep getting
   one wrong.
3. **Sign-in:** Google, Microsoft and any other OpenID Connect provider
   through `Driver.oidc` and an issuer; area 8's built-in presets only for
   GitHub and Apple, which are not plain OpenID Connect. *Recommended:* yes.
4. **A vendor with two APIs under different credentials** (Shopify Admin
   and Storefront, Supabase's management and project APIs) is two
   packages. *Recommended:* yes; each is one connection and is versioned on
   its own.
5. **The declaration's name.** `provider` matches "provider package" and
   `shared/providers/`, but sits beside the `provide` keyword; `api` is
   taken by area 7's APIs. *Recommended:* `provider`.
6. **The container image's digest** goes in the package source
   (`image: "…@sha256:…"`) rather than `jadpo.lock`: the package's own
   digest already pins it, and an own integration in `shared/providers/`
   has no lock entry. *Recommended:* yes.
7. **A connection's `host:` beside a package's `runs:`** means "it is
   already running there; don't start it", which is how Workers reaches a
   service. A package with fixed `hosts:` refuses an app's `host:`.
   *Recommended:* yes.
8. **The grant for a service's outbound hosts** is `outbound: [...]` on the
   connection, listed in the approval report. *Recommended:* yes.
