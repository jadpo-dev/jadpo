---
title: External services
area: 11
status: in-review
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - semantic.service_atomic_effect
  - semantic.service_call_context
  - semantic.service_configuration_missing
  - semantic.service_contract_invalid
  - semantic.service_egress_invalid
  - semantic.service_idempotency_invalid
  - semantic.service_import_digest_mismatch
  - semantic.service_import_not_found
  - semantic.service_import_pin_invalid
  - semantic.service_secret_sink_invalid
---

# External services

An external service is another company's API (Stripe, a courier, another
Jadpo app) that an action calls. In Jadpo, each one is described once, as a
**provider package**: Jadpo source that lists the API's operations, types,
failures and webhook events, and says which calls change something, which
take an idempotency key, how errors and pages come back and how webhooks are
signed. An app connects to a package in `app.jadpo` and calls its operations
like an SDK. The compiler owns the HTTP request, the secret, the host, the
retries and the reading of the response; application code never builds a
URL, sets a header or sees a raw response.

> Reopened on 2026-10-13. Area 11 was decided on 2026-10-12 around
> per-domain `integration` declarations; Richard agreed to rethink it around
> provider packages ([the rethink](research/11-integrations-rethink.md)).
> Questions 6, 7 and 8 stay decided; the rest are open again, with new
> recommendations below. Today's compiled syntax is collapsed beneath each
> section.

| Topic | In short |
|---|---|
| Provider package | One API at one version, written in Jadpo, used by every app |
| Where packages come from | Jadpo's registry, an importer (OpenAPI, Smithy, Postman, HAR, `.d.ts`), or written by an agent from the docs |
| Provenance | Every operation says whether it was imported or written, and a recorded response confirms it |
| Connecting | `providers: { }` in `app.jadpo`, beside the stores |
| Calling | `couriers.bookings.create(request)` from actions only, failures mapped at the call |
| Retries and keys | From the package and the runtime; the runtime supplies idempotency keys |
| Webhooks | Generated route, signature checked, repeats dropped, published as events |
| Compile time | Only the resources an app calls are loaded; each package checked once per digest |
| Tests | A generated fake per package; scheduled contract runs against the sandbox |
| Your own services | Every Jadpo API publishes a package from its API record |
| Capabilities | Mail, files, SMS and others: one API in code, the provider chosen in `app.jadpo` (proposed) |

## Provider packages

A package is one external API at one version. It is to Stripe what a
TypeScript `.d.ts` file is to a JavaScript library: a description, with no
code of its own. It is Jadpo source, one file per resource plus an index, so
agents and people read it like any other code and the compiler parses only
Jadpo.

It records what the provider's own specification usually leaves out:

- which operations change something (`POST` does not say);
- which take an idempotency key, and where it goes;
- the provider's errors, as failure types with a kind;
- how lists page, and the provider's rate limits;
- webhook events and how they are signed;
- the live and sandbox hosts.

```jadpo fragment
// providers/acme_couriers/package.jadpo: the index, names to files
package AcmeCouriers.v1 {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
    credential: CredentialScheme.bearer
    idempotency: {
        header: "Idempotency-Key"
    }
    error_code: "error.code"
    resources: {
        bookings: "bookings.jadpo"
    }
}

// providers/acme_couriers/bookings.jadpo
resource bookings {
    create: {
        method: HttpMethod.post
        path: "/v1/bookings"
        input: CreateBooking
        output: Booking
        changes: true
        idempotent: true
        failures: [SlotTaken]
        source: PackageSource.written
        docs: "https://docs.acme-couriers.example/api#create-booking"
    }
}

type CreateBooking = Object {
    postcode: Text
    slot: Text
}

type Booking = Object {
    id: Text
    collect_at: Instant
}

type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "The collection slot is no longer available."
}
```

Inside the package, names are bare. An app names them through the package:
`AcmeCouriers.Booking`, `AcmeCouriers.SlotTaken` (question 16). A failure's
code (`slot_taken`, from its name) is the provider's error code, read from
the place `error_code:` names.

::: today

The keyword is `service`, an operation is a run of flat lines, and settings
use one-off grammar: `equals`, `exponential(...)` and `maps ... -> fault`.
Faults are listed in the caller's `fails`. The compiler compares every line
with a fixed list, so this is the only declaration that compiles:

```jadpo project=today-declare file=types.jadpo hidden
config TestConfiguration {
    mail_api_key: Text {
        binding: "MAIL_API_KEY"
        secret: true
    }
}

type ReminderIntentId = Uuid
type TodoTitle = Text {
    min_length: 1
    max_length: 200
}

value ReminderMessage {
    idempotency_key: ReminderIntentId
    from: Email
    to: Email
    todo_title: TodoTitle
    due_at: Instant?
}

value ReminderReceipt {
    accepted_at: Instant
}

failure ReminderRecipientRejected {
    kind: Rejected
    code: "recipient_rejected"
    message: "Recipient rejected."
}

failure ReminderTemporarilyUnavailable {
    kind: Unavailable
    code: "temporarily_unavailable"
    message: "Temporarily unavailable."
}
```

```jadpo project=today-declare file=tests/assurance/service-reference-mail-v0.1.json hidden
{
  "openapi": "3.1.0",
  "info": {
    "title": "Jadpo local reference mail provider",
    "version": "0.1.0"
  },
  "servers": [
    { "url": "https://mail.example.invalid" }
  ],
  "paths": {
    "/v1/messages": {
      "post": {
        "operationId": "send_overdue_reminder",
        "parameters": [
          {
            "name": "Idempotency-Key",
            "in": "header",
            "required": true,
            "schema": { "type": "string", "format": "uuid" }
          }
        ],
        "security": [ { "providerCredential": [] } ],
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["idempotency_key", "from", "to", "todo_title", "due_at"],
                "properties": {
                  "idempotency_key": { "type": "string", "format": "uuid" },
                  "from": { "type": "string", "format": "email" },
                  "to": { "type": "string", "format": "email" },
                  "todo_title": { "type": "string", "minLength": 1, "maxLength": 200 },
                  "due_at": { "type": ["string", "null"], "format": "date-time" }
                }
              }
            }
          }
        },
        "responses": {
          "202": {
            "description": "Accepted or deduplicated by the same key and payload",
            "headers": {
              "Idempotency-Key": {
                "required": true,
                "schema": { "type": "string", "format": "uuid" }
              }
            },
            "content": {
              "application/json": {
                "schema": {
                  "type": "object",
                  "additionalProperties": false,
                  "required": ["accepted_at"],
                  "properties": {
                    "accepted_at": { "type": "string", "format": "date-time" }
                  }
                }
              }
            }
          },
          "400": { "$ref": "#/components/responses/InvalidRecipient" },
          "401": { "$ref": "#/components/responses/Authentication" },
          "409": { "$ref": "#/components/responses/IdempotencyConflict" },
          "429": { "$ref": "#/components/responses/RateLimited" }
        }
      }
    }
  },
  "components": {
    "securitySchemes": {
      "providerCredential": { "type": "http", "scheme": "bearer" }
    },
    "responses": {
      "InvalidRecipient": {
        "description": "Recipient rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/InvalidRecipient" } } }
      },
      "Authentication": {
        "description": "Credential rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Authentication" } } }
      },
      "IdempotencyConflict": {
        "description": "Key reused with a different payload; no new acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/IdempotencyConflict" } } }
      },
      "RateLimited": {
        "description": "Request refused before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/RateLimited" } } }
      }
    },
    "schemas": {
      "InvalidRecipient": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "invalid_recipient" } }
      },
      "Authentication": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "authentication" } }
      },
      "IdempotencyConflict": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "idempotency_conflict" } }
      },
      "RateLimited": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "rate_limited" } }
      }
    }
  }
}
```

```jadpo project=today-declare
service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}

action deliver(mail: ReminderMessage)
    fails ReminderRecipientRejected, ReminderTemporarilyUnavailable, Misconfigured, Unavailable, OutcomeUnknown
    -> ReminderReceipt
{
    return attempt ReminderMail.send_overdue_reminder(mail)
}
```

:::

## Where packages come from

There are three sources, and they all produce the same files.

**The registry.** Jadpo's own registry holds curated packages for common
providers. `jadpo provider add Stripe.v2024_06_20` copies one into a local
cache and pins it in `jadpo.lock` with its digest. A build never fetches
anything (see "The registry" below).

**An importer.** `jadpo provider import <format> <file>` turns a published
description into a package once, offline, in the repository's `providers/`
folder:

| Format | Typical source | What it gives |
|---|---|---|
| `openapi` | Most REST APIs (Stripe, Twilio, GitHub) | Operations, types, error responses |
| `smithy` | AWS, one model per service | The same, plus AWS's protocols and signing |
| `postman` | A provider's published Postman collection | Operations and example bodies; types inferred from examples |
| `har` | A browser or proxy capture of real traffic | Operations and real responses; values stripped, shapes kept |
| `dts` | The provider's official TypeScript SDK (`.d.ts` files) | Exact types and SDK method names; no HTTP method or path, so the agent adds those from the docs |

An agent then fills what the source leaves out: which calls change
something, idempotency, error names. A person reviews that diff.
Corrections to a wrong specification live in the package and survive the
next import, because the importer merges rather than overwrites, as an
[OpenAPI Overlay](https://spec.openapis.org/overlay/v1.0.0) does.

**Written by hand: the no-spec path.** Many APIs publish only HTML docs. A
package is Jadpo source, so for these an agent writes the package files
directly from the docs, in exactly the format an importer produces. Nothing
else changes: the same compiler checks, the same fakes, the same review. A
package needs only the operations the app uses, so it can start with one
operation and grow (question 13).

### Provenance and confirmation

Every operation says where it came from, so a reviewer knows how far to
trust it (question 14):

| `source:` | Meaning | Also records |
|---|---|---|
| `PackageSource.imported` | From a published description (OpenAPI, Smithy, Postman, `.d.ts`, GraphQL introspection later) | The package index names the file and its digest |
| `PackageSource.written` | Written from the provider's docs | `docs:` with the page's URL |

An operation is **confirmed** when a real response has been checked against
it. `jadpo provider record <connection> <operation>` calls the provider's
sandbox once, then checks the operation's output type against the response,
or writes the type if there is none yet. It saves the response, with values
replaced by placeholders, as `providers/<package>/recordings/<resource>.<operation>.json`;
the compiler checks the output type against it on every build. An operation
imported from a HAR capture is confirmed from the start, since the capture
holds real responses. `record` refuses a connection whose environment is
live.

Written operations that are not confirmed are allowed in every build. The
approval report flags them, and CI requires a passing contract run against
the sandbox for each one (see "Tests and fakes").

::: today

There are no packages, importers or registry. The one accepted declaration
is a `service` naming its OpenAPI file, version and SHA-256 in an `import`
block, and the file must be the golden app's fixed contract.

:::

## The registry

The registry is Jadpo's own, not a third party's, in line with area 8's rule
that the language owns a feature where it reasonably can.

- **Declarations only.** A package contains no executable code: the
  compiler generates the client from it. A registry package cannot run an
  install script or ship code into the app, which is how most npm
  supply-chain attacks work. This is a real advantage, though not a complete
  one: a malicious package could still name the wrong host or mark an
  unsafe call idempotent. That is why hosts and every package change appear
  in the approval report.
- **Signed and pinned.** Packages are signed by the registry and pinned by
  digest in `jadpo.lock`; a changed digest is a package upgrade, reviewed as
  one.
- **Never fetched during a build.** `jadpo provider add` and `upgrade` fetch;
  `jadpo check` and `jadpo build` read the cache and the lock only.
- **Local packages** live in `providers/` and are committed.
- **Jadpo APIs publish their own** (see "Your own services").

To start, a few curated packages (Stripe, Twilio and GitHub, all imported
from their published OpenAPI) live in the Jadpo repository; a hosted
registry, and who may publish to it, come later (question 15).

## Connecting in `app.jadpo`

A connection is infrastructure, declared once beside the stores. It names
the package, the domains whose actions may call it (question 18), the
credential from `config.jadpo`, and which of the package's hosts to use.

```jadpo fragment
// app.jadpo
application {
    stores: {
        ...
    }
    providers: {
        stripe: {
            package: Stripe.v2024_06_20
            domains: [payments]
            credential: config.stripe_secret_key
            environment: config.stripe_environment
            webhooks: {
                secret: config.stripe_webhook_secret
            }
        }
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
            timeout: 5s
        }
    }
}
```

- `environment:` is a `ProviderEnvironment` setting, `.sandbox` or `.live`,
  choosing between the package's hosts. HTTPS only; redirects and proxies are
  always refused (question 9).
- The credential is a `secret: true` setting (area 10). The package says how
  it is sent (bearer, a named header, basic); code can never read it
  (question 10).
- `timeout:` and `retry:` override the defaults (question 8).
- Two versions of one package are two connections, used side by side while
  callers move across (question 3).

::: today

No connections exist. The `service` declaration itself holds the host
(`base_url` plus a restated `egress:`), a credential that must be
`config.mail_api_key`, and `proxy: prohibited` and `redirects: prohibited`,
each with one legal value.

:::

## Calling

An operation is called through its connection:
`connection.resource.operation(input)`. Only actions may call one. It can
fail, so it needs `attempt` or a `match`, and the package's failures are
mapped to the app's own at the call, in area 4's form `condition: Failure`.

```jadpo fragment
// domains/shipping/actions/book_collection.jadpo
action book_collection(shipment: Shipment) fails CollectionSlotGone -> Shipment {
    var request: AcmeCouriers.CreateBooking = AcmeCouriers.CreateBooking {
        postcode: shipment.postcode
        slot: shipment.slot
    }
    var booking: AcmeCouriers.Booking = attempt couriers.bookings.create(request) {
        slot_taken: CollectionSlotGone
    }
    return attempt Shipment.update(shipment) {
        set: {
            booking_id: booking.id
        }
    }
}
```

- **Types** come from the package; the app writes none (question 1). The
  checker compares what you send with what the operation takes. At runtime
  a response is checked only for the fields your code reads, so a provider
  adding or changing other fields breaks nothing.
- **Failures** (question 5). A package failure is mapped at the call or it
  becomes a fault. `timed_out` may map to an `unavailable` failure only on an
  idempotent operation. Unmapped problems are faults, never in `fails`:
  `misconfigured` for a rejected credential, `unavailable` when the provider
  surely did nothing, `outcome_unknown` when it may have acted.
- **Idempotency** (question 6, decided). The package says where the key goes;
  the runtime supplies it: the delivery key in subscribers and schedules, the
  client's `Idempotency-Key` or the request id in routes.
- **Retries** (question 8, decided). `timeout:` (default 10s) and
  `retry: { attempts: within: }` (default 3 within 30s), exponential backoff
  with full jitter. Only safe repeats are retried: the request never left,
  the provider said it did nothing (honouring `Retry-After`), or the call is
  idempotent. The package's rate limits space calls out before the provider
  refuses them.
- **Pagination.** A list operation returns area 6's `Page<T>`; the package
  says how the provider pages, and `page.next` asks for the next one. Work
  over many pages runs one page per subscriber delivery (area 12).
- **Order with writes** (question 7, decided). A call that changes something
  comes before any write; the writes commit after it returns, and a failed
  commit retries the call with the same key. An action makes at most one
  such call (area 12). Work that must follow a write emits an event.

::: today

Called as `ReminderMail.send_overdue_reminder(mail)`. A call from a function
or query is `semantic.service_call_context`, and any write in the same
action, before or after the call, is `semantic.service_atomic_effect`.
Callers must list the faults `Misconfigured`, `Unavailable` and
`OutcomeUnknown`, which area 4 removes. Retries are fixed at `timeout: 5s`
and `retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)`.

:::

## Webhooks as events

A package declares the provider's events and how they are signed. Giving a
connection a webhook secret makes the runtime:

1. generate `POST /webhooks/<connection>` in area 7's `webhooks` API;
2. check the signature over the raw body, rejecting timestamps older than
   five minutes;
3. drop repeats by the provider's event id;
4. publish each event as an ordinary Jadpo event (area 12).

```jadpo fragment
// domains/payments/subscribers/payment_succeeded.jadpo
subscriber payment_succeeded {
    on: Stripe.PaymentIntentSucceeded
    run: mark_order_paid(event)
}
```

No webhook route is written by hand (question 11). Signature schemes are
built in: `SignatureScheme.stripe`, `SignatureScheme.standard_webhooks`
(Svix and others) and `SignatureScheme.hmac_sha256` with a named
header for the rest.

Today nothing supports webhooks.

## Compile time

Provider descriptions are large. Sizes checked on 2026-10-13 by
`Content-Length` of the raw files:

| File | Size |
|---|---|
| Stripe `openapi/spec3.json` | 8.3 MB |
| AWS S3 Smithy model | 3.0 MB |
| AWS EC2 Smithy model | 6.7 MB |

AWS publishes about 400 such models, one per service. Parsing 8 MB takes
milliseconds; the cost is resolving, checking and generating thousands of
schemas on every build and every keystroke. So the compiler never reads a
provider's description: an importer reads it once. On a build, the compiler
reads each package's index, loads only the resources the app calls, and
caches the check per package digest, so an unchanged package costs nothing
after the first build. An app using S3 and SES loads two small packages and
a handful of files from each.

## Tests and fakes

Every package has a generated fake. In `jadpo test`, an operation answers
with a valid default built from its output type, and a test scripts a
response or a failure for one call with area 13's `fakes:`. Responses still
pass through the real retries and mapping.

```jadpo fragment
test "a taken slot is reported" {
    fakes: {
        couriers.bookings.create: [slot_taken]
    }
    ...
}
```

The compiler checks every scripted response against the package's types, so
a fake cannot return a shape the provider never would. `jadpo test --kind
contract` runs the same cases against the provider's sandbox, in CI on a
schedule. Every written operation that is not confirmed must have a case in
that run (question 12).

## Review

The approval report (area 17) lists, for each change:

- operations newly called, and the domains calling them;
- data newly sent to a provider, with personal fields flagged (area 15);
- package upgrades, as a diff of the operations the app uses, hosts
  included;
- written operations not yet confirmed.

## Your own services

Every Jadpo API (area 7) publishes a package from its API record, with its
types, failures, idempotency and events. App A connects to app B's package
like any other and calls `orders.orders.create(request)`, with B's types for
free. A package with no fixed host (your own services, self-hosted
products) takes `host: config.orders_url` on the connection, and the
approval report shows it. Internal services and third parties work the same
way.

## Capabilities

> Proposed 2026-10-13 (questions 19–25). The research, and eight real
> providers modelled as packages, are in
> [the capabilities proposals](research/capabilities-proposals.md).

A **capability** is a job several providers do: sending mail, storing files,
sending SMS. Jadpo owns a short, fixed list, and each one has a single API
that application code uses, never the provider's. A package says which
capabilities it provides and maps the standard operations onto its own
calls, so changing from Postmark to SES is a change to `app.jadpo`, not to
code (question 19).

| Capability | Code uses | Built-in drivers | Packages? |
|---|---|---|---|
| `database` | Entities (area 5) | PostgreSQL; D1 later | No |
| `cache` | `caches:` (area 5) | Redis; in memory | No, at first |
| `files` | `File` fields (area 16) | S3; local disk | Yes, as S3 presets |
| `email` | `type X = EmailMessage`, emitted (area 16) | SMTP; local inbox | Yes |
| `sms` | `type X = Sms`, emitted | Local inbox | Yes |
| `publishing` | Chosen public events sent to other systems | Local log | Yes |
| `schedules` | `schedule` (area 12) | Timer; Cloudflare cron; external tick | No |

Event delivery is not a capability: it stays inside the runtime, from the
outbox in the main store, so logs and traces stay in one place and no queue
service is needed. A queue such as Cloudflare Queues or SQS is a
`publishing` target instead: chosen public events go out to it after
commit, at least once, with their trace context. Telemetry is not one
either: every monitoring vendor accepts OpenTelemetry (area 16).

**One `connections:` block.** `stores:` and `providers:` merge. Each entry
is a built-in driver or a package; where it is used says which capability
it serves (question 20). One connection is one resource (a bucket, a
queue), not one vendor account, as Cloudflare's own bindings are.

```jadpo fragment
// app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        cache: {
            driver: Driver.redis
            url: config.redis_url
        }
        avatars: {
            package: CloudflareR2.v1
            account_id: config.cloudflare_account
            bucket: "avatars"
            credential: config.r2_credential
        }
        postmark: {
            package: Postmark.v1
            credential: config.postmark_token
            message_stream: "outbound"
        }
        stripe: {
            package: Stripe.v2024_06_20
            domains: [payments]
            credential: config.stripe_secret_key
        }
    }
    email: {
        connection: postmark
        from: config.mail_sender
    }
}
```

Entities, caches and `File` fields name a connection with `store:`;
`email:`, `sms:` and `publishing:` name one in `application { }`; schedules
use the target's own tick unless told otherwise. Using a connection for a
capability its package does not provide is a compile error.

**Provider features without breaking the abstraction.** Three ways in:
typed settings the package declares (`message_stream:` above); standard
events back (`EmailBounced`, `SmsFailed`) that the package maps its webhooks
onto; and the package's own API, still callable as
`connection.resource.operation(input)` from the domains the connection
lists.

**How a package provides one.** The package index says `provides: { email:
"email.jadpo" }`, and that file maps each standard operation onto one of the
package's operations (question 21):

```jadpo fragment
// providers/postmark/email.jadpo
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
            inactive_recipient: MailFailure.recipient_rejected
        }
    }
}
```

A mapping holds only field paths, literals, lists, nested blocks and enum
tables: no expressions or calls, so the compiler checks both sides and a
package stays declarations only. What needs real code is built into the
runtime: the PostgreSQL, Redis, SMTP and S3 drivers, Workers bindings, wire
formats, credential schemes, webhook checks and schedule ticks.

**Proving a package works.** Each capability has a conformance suite;
`jadpo provider conform <package> <capability>` runs it against the
provider's sandbox. The registry lists a capability only with a passing
run pinned to the package's digest. Tests use one fake per capability,
whatever the provider: `email.sent`, `sms.sent`, `published` (question 22).

**Development.** In `jadpo dev` nothing leaves the machine: mail and SMS go
to the local inbox, files to local disk, published events to a log in the
dev console. The database runs its real engine; the cache runs in memory
(question 23).

**Reports.** The manifest records each connection's capabilities, hosts and
targets; the network report labels each host with its capability; the
personal-data report lists mail, SMS, file and publishing connections as
receiving the marked fields of what they carry. A capability can map the
provider's delete operation, and then area 15's `PersonErased` subscriber
is generated.

**Targets.** Each driver and package says which targets it runs on, and
`jadpo check` refuses a deployable that uses one elsewhere, naming the
alternative (question 24). Cloudflare Workers can open TCP sockets but block
port 25, so SMTP works only on port 587; Workers has no timer of its own, so
its schedule tick is Cloudflare cron; R2 and Queues bindings exist only on
Workers.

## Errors

Today's rules check the one fixed declaration. They carry over to the general
compiler, with wording that no longer names the mail provider. Every example
below is today's syntax.

### `semantic.service_import_pin_invalid`

The contract import loses its digest:

```jadpo project=err-pin expect=semantic.service_import_pin_invalid
service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

### `semantic.service_import_not_found`

The declaration is exact, but the contract file is missing from the project:

```jadpo project=err-not-found expect=semantic.service_import_not_found
service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

### `semantic.service_import_digest_mismatch`

The contract file exists but its bytes have changed (here it holds `{}`):

```jadpo project=err-digest file=tests/assurance/service-reference-mail-v0.1.json hidden
{}
```

```jadpo project=err-digest expect=semantic.service_import_digest_mismatch
service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

### `semantic.service_egress_invalid`

The allowed host no longer matches `base_url`:

```jadpo project=err-egress expect=semantic.service_egress_invalid
service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.attacker.example:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

### `semantic.service_secret_sink_invalid`

The credential moves to another header:

```jadpo project=err-secret expect=semantic.service_secret_sink_invalid
service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "X-Api-Key"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

### `semantic.service_idempotency_invalid`

The idempotency key is a plain `Uuid` instead of the named intent type:

```jadpo project=err-idempotency expect=semantic.service_idempotency_invalid
service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: Uuid
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

### `semantic.service_contract_invalid`

Any other difference, here only the name, is outside the checked slice:

```jadpo project=err-contract expect=semantic.service_contract_invalid
service StripeV2024 {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

### `semantic.service_configuration_missing`

The project has no `config` declaration for the credential:

```jadpo project=err-config file=tests/assurance/service-reference-mail-v0.1.json hidden
{
  "openapi": "3.1.0",
  "info": {
    "title": "Jadpo local reference mail provider",
    "version": "0.1.0"
  },
  "servers": [
    { "url": "https://mail.example.invalid" }
  ],
  "paths": {
    "/v1/messages": {
      "post": {
        "operationId": "send_overdue_reminder",
        "parameters": [
          {
            "name": "Idempotency-Key",
            "in": "header",
            "required": true,
            "schema": { "type": "string", "format": "uuid" }
          }
        ],
        "security": [ { "providerCredential": [] } ],
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["idempotency_key", "from", "to", "todo_title", "due_at"],
                "properties": {
                  "idempotency_key": { "type": "string", "format": "uuid" },
                  "from": { "type": "string", "format": "email" },
                  "to": { "type": "string", "format": "email" },
                  "todo_title": { "type": "string", "minLength": 1, "maxLength": 200 },
                  "due_at": { "type": ["string", "null"], "format": "date-time" }
                }
              }
            }
          }
        },
        "responses": {
          "202": {
            "description": "Accepted or deduplicated by the same key and payload",
            "headers": {
              "Idempotency-Key": {
                "required": true,
                "schema": { "type": "string", "format": "uuid" }
              }
            },
            "content": {
              "application/json": {
                "schema": {
                  "type": "object",
                  "additionalProperties": false,
                  "required": ["accepted_at"],
                  "properties": {
                    "accepted_at": { "type": "string", "format": "date-time" }
                  }
                }
              }
            }
          },
          "400": { "$ref": "#/components/responses/InvalidRecipient" },
          "401": { "$ref": "#/components/responses/Authentication" },
          "409": { "$ref": "#/components/responses/IdempotencyConflict" },
          "429": { "$ref": "#/components/responses/RateLimited" }
        }
      }
    }
  },
  "components": {
    "securitySchemes": {
      "providerCredential": { "type": "http", "scheme": "bearer" }
    },
    "responses": {
      "InvalidRecipient": {
        "description": "Recipient rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/InvalidRecipient" } } }
      },
      "Authentication": {
        "description": "Credential rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Authentication" } } }
      },
      "IdempotencyConflict": {
        "description": "Key reused with a different payload; no new acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/IdempotencyConflict" } } }
      },
      "RateLimited": {
        "description": "Request refused before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/RateLimited" } } }
      }
    },
    "schemas": {
      "InvalidRecipient": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "invalid_recipient" } }
      },
      "Authentication": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "authentication" } }
      },
      "IdempotencyConflict": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "idempotency_conflict" } }
      },
      "RateLimited": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "rate_limited" } }
      }
    }
  }
}
```

```jadpo project=err-config expect=semantic.service_configuration_missing
service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

### `semantic.service_call_context`

A function calls the operation:

```jadpo project=err-context file=types.jadpo hidden
config TestConfiguration {
    mail_api_key: Text {
        binding: "MAIL_API_KEY"
        secret: true
    }
}

type ReminderIntentId = Uuid
type TodoTitle = Text {
    min_length: 1
    max_length: 200
}

value ReminderMessage {
    idempotency_key: ReminderIntentId
    from: Email
    to: Email
    todo_title: TodoTitle
    due_at: Instant?
}

value ReminderReceipt {
    accepted_at: Instant
}

failure ReminderRecipientRejected {
    kind: Rejected
    code: "recipient_rejected"
    message: "Recipient rejected."
}

failure ReminderTemporarilyUnavailable {
    kind: Unavailable
    code: "temporarily_unavailable"
    message: "Temporarily unavailable."
}

service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

```jadpo project=err-context file=tests/assurance/service-reference-mail-v0.1.json hidden
{
  "openapi": "3.1.0",
  "info": {
    "title": "Jadpo local reference mail provider",
    "version": "0.1.0"
  },
  "servers": [
    { "url": "https://mail.example.invalid" }
  ],
  "paths": {
    "/v1/messages": {
      "post": {
        "operationId": "send_overdue_reminder",
        "parameters": [
          {
            "name": "Idempotency-Key",
            "in": "header",
            "required": true,
            "schema": { "type": "string", "format": "uuid" }
          }
        ],
        "security": [ { "providerCredential": [] } ],
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["idempotency_key", "from", "to", "todo_title", "due_at"],
                "properties": {
                  "idempotency_key": { "type": "string", "format": "uuid" },
                  "from": { "type": "string", "format": "email" },
                  "to": { "type": "string", "format": "email" },
                  "todo_title": { "type": "string", "minLength": 1, "maxLength": 200 },
                  "due_at": { "type": ["string", "null"], "format": "date-time" }
                }
              }
            }
          }
        },
        "responses": {
          "202": {
            "description": "Accepted or deduplicated by the same key and payload",
            "headers": {
              "Idempotency-Key": {
                "required": true,
                "schema": { "type": "string", "format": "uuid" }
              }
            },
            "content": {
              "application/json": {
                "schema": {
                  "type": "object",
                  "additionalProperties": false,
                  "required": ["accepted_at"],
                  "properties": {
                    "accepted_at": { "type": "string", "format": "date-time" }
                  }
                }
              }
            }
          },
          "400": { "$ref": "#/components/responses/InvalidRecipient" },
          "401": { "$ref": "#/components/responses/Authentication" },
          "409": { "$ref": "#/components/responses/IdempotencyConflict" },
          "429": { "$ref": "#/components/responses/RateLimited" }
        }
      }
    }
  },
  "components": {
    "securitySchemes": {
      "providerCredential": { "type": "http", "scheme": "bearer" }
    },
    "responses": {
      "InvalidRecipient": {
        "description": "Recipient rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/InvalidRecipient" } } }
      },
      "Authentication": {
        "description": "Credential rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Authentication" } } }
      },
      "IdempotencyConflict": {
        "description": "Key reused with a different payload; no new acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/IdempotencyConflict" } } }
      },
      "RateLimited": {
        "description": "Request refused before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/RateLimited" } } }
      }
    },
    "schemas": {
      "InvalidRecipient": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "invalid_recipient" } }
      },
      "Authentication": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "authentication" } }
      },
      "IdempotencyConflict": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "idempotency_conflict" } }
      },
      "RateLimited": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "rate_limited" } }
      }
    }
  }
}
```

```jadpo project=err-context expect=semantic.service_call_context
function deliver(mail: ReminderMessage) -> ReminderReceipt {
    return attempt ReminderMail.send_overdue_reminder(mail)
}
```

### `semantic.service_atomic_effect`

An action calls the operation and then writes the receipt:

```jadpo project=err-atomic file=types.jadpo hidden
config TestConfiguration {
    mail_api_key: Text {
        binding: "MAIL_API_KEY"
        secret: true
    }
}

type ReminderIntentId = Uuid
type TodoTitle = Text {
    min_length: 1
    max_length: 200
}

value ReminderMessage {
    idempotency_key: ReminderIntentId
    from: Email
    to: Email
    todo_title: TodoTitle
    due_at: Instant?
}

value ReminderReceipt {
    accepted_at: Instant
}

failure ReminderRecipientRejected {
    kind: Rejected
    code: "recipient_rejected"
    message: "Recipient rejected."
}

failure ReminderTemporarilyUnavailable {
    kind: Unavailable
    code: "temporarily_unavailable"
    message: "Temporarily unavailable."
}

service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

```jadpo project=err-atomic file=tests/assurance/service-reference-mail-v0.1.json hidden
{
  "openapi": "3.1.0",
  "info": {
    "title": "Jadpo local reference mail provider",
    "version": "0.1.0"
  },
  "servers": [
    { "url": "https://mail.example.invalid" }
  ],
  "paths": {
    "/v1/messages": {
      "post": {
        "operationId": "send_overdue_reminder",
        "parameters": [
          {
            "name": "Idempotency-Key",
            "in": "header",
            "required": true,
            "schema": { "type": "string", "format": "uuid" }
          }
        ],
        "security": [ { "providerCredential": [] } ],
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["idempotency_key", "from", "to", "todo_title", "due_at"],
                "properties": {
                  "idempotency_key": { "type": "string", "format": "uuid" },
                  "from": { "type": "string", "format": "email" },
                  "to": { "type": "string", "format": "email" },
                  "todo_title": { "type": "string", "minLength": 1, "maxLength": 200 },
                  "due_at": { "type": ["string", "null"], "format": "date-time" }
                }
              }
            }
          }
        },
        "responses": {
          "202": {
            "description": "Accepted or deduplicated by the same key and payload",
            "headers": {
              "Idempotency-Key": {
                "required": true,
                "schema": { "type": "string", "format": "uuid" }
              }
            },
            "content": {
              "application/json": {
                "schema": {
                  "type": "object",
                  "additionalProperties": false,
                  "required": ["accepted_at"],
                  "properties": {
                    "accepted_at": { "type": "string", "format": "date-time" }
                  }
                }
              }
            }
          },
          "400": { "$ref": "#/components/responses/InvalidRecipient" },
          "401": { "$ref": "#/components/responses/Authentication" },
          "409": { "$ref": "#/components/responses/IdempotencyConflict" },
          "429": { "$ref": "#/components/responses/RateLimited" }
        }
      }
    }
  },
  "components": {
    "securitySchemes": {
      "providerCredential": { "type": "http", "scheme": "bearer" }
    },
    "responses": {
      "InvalidRecipient": {
        "description": "Recipient rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/InvalidRecipient" } } }
      },
      "Authentication": {
        "description": "Credential rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Authentication" } } }
      },
      "IdempotencyConflict": {
        "description": "Key reused with a different payload; no new acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/IdempotencyConflict" } } }
      },
      "RateLimited": {
        "description": "Request refused before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/RateLimited" } } }
      }
    },
    "schemas": {
      "InvalidRecipient": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "invalid_recipient" } }
      },
      "Authentication": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "authentication" } }
      },
      "IdempotencyConflict": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "idempotency_conflict" } }
      },
      "RateLimited": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "rate_limited" } }
      }
    }
  }
}
```

```jadpo project=err-atomic expect=semantic.service_atomic_effect
entity Reminder {
    id: Uuid { generated: identity }
    accepted_at: Instant

    identity: id

    persistence {
        store: primary
        role: authority
    }

    action deliver(mail: ReminderMessage)
        fails ReminderRecipientRejected, ReminderTemporarilyUnavailable, Misconfigured, Unavailable, OutcomeUnknown
        -> ReminderReceipt
    {
        var receipt = attempt ReminderMail.send_overdue_reminder(mail)
        var saved = attempt create Reminder {
            accepted_at: receipt.accepted_at
        }
        return receipt
    }
}
```

## Review

### Status

Each row says how it was checked: compiled with `jadpo check`, built with
`jadpo build`, or read from the compiler source.

| Feature | Status |
|---|---|
| Declaring | **Gap:** one hard-coded declaration. `checked_service_effect` compares every line with a fixed list of 29 and the name with `ReminderMail` (read); renaming it, or adding a second service, is `semantic.service_contract_invalid` (compiled). The operation, method, path, types, host and limits in the result are constants, not read from the source (read). |
| Contract | Must be the fixed path and digest; a file outside the project is refused (compiled). The hand-written input and output types are checked field by field against the contract (read). |
| Calls | A function caller and any write in the same action are rejected (compiled); an action that only calls passes (compiled). |
| Generated adapter | `jadpo build` writes `build/target/service-adapter.ts` and `build/audit/services.json` (built). The service plan records 19 passing loopback HTTP tests; not rerun here. |
| Failures | Callers must list the faults `Misconfigured`, `Unavailable` and `OutcomeUnknown` (compiled), which area 4 forbids. |
| Credential | **Gap:** the setting must be named `mail_api_key` (read). |
| Messages | **Bug:** every rule's advice names the mail provider ("Declare the mail API key", "Use ReminderIntentId"), whatever the service (read from the catalogue). |
| Packages, importers, registry, recordings | None. |
| Webhooks, versions, other schemes | None. |
| Fakes | Work (area 13). |

### Consistency with the rest of the language

The package design changes decisions made in other areas:

| Area | Decided | Change |
|---|---|---|
| 1 | `integration` keyword in each domain's `integrations/` folder, contract beside it ("Keywords match folders") | Connections in `app.jadpo`; packages in a root `providers/` folder and `jadpo.lock`, two entries area 1's fixed root does not list; a domain's ownership becomes the connection's `domains:` list |
| 7 | Question 8: webhooks are hand-written routes in the `webhooks` API with a unique rule on the event id | Generated from the package; repeats dropped by the runtime |
| 8 | A webhook route's authentication is `StripeV2024.signature` | No longer needed; the generated route checks the signature |
| 13 | Question 7 (open): every reachable operation needs a fake, or the test does not compile | Unscripted calls answer from the generated fake; scripted lists keep area 13's leftover check |

Other points:

- **Two dots in a call.** `couriers.bookings.create` is the first call with
  two dots. The dot still names the owner: the connection owns its
  resources, a resource its operations.
- **No anonymous objects** (area 2). The rethink's `create({ amount: ... })`
  becomes a named package type, `AcmeCouriers.CreateBooking { ... }`.
- **Area 4's mapping form** holds: `condition: Failure` in the call's block,
  kind checked, the provider failure kept as the internal cause.
- **Areas 3, 9 and 12 hold:** calls only from actions, the changing call
  before the writes (area 3's amendment cites question 7, which keeps its
  number), whoever may run the action may call (area 9), one changing call
  per action (area 12).
- **Area 16** sends email through the `email` capability: SMTP is one
  built-in driver, a mail package another (question 19). Outgoing webhooks
  could reuse `SignatureScheme`.
- **Capabilities (proposed) change decided text.** Area 5's `stores:` and
  `engine:` and this page's `providers:` become one `connections:` block
  with `driver:` or `package:` (question 20); area 5's "same engine in every
  environment" would no longer hold for caches in development (question
  23); area 12's schedule adapters become built-in tick drivers chosen by
  the target (question 24); questions 9 and 10 need sandboxes that are
  credentials, host placeholders and two-part credentials (question 25).
- **Correction.** Twilio signs webhooks with its own HMAC-SHA1 scheme, not
  Standard Webhooks as the webhooks section said (checked 2026-10-13); see
  question 25.
- **Rule IDs.** The `semantic.service_*` rules below check today's
  declaration. Under packages, the pin, digest and egress rules become
  `jadpo.lock` and package checks (area 14).

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Root `providers/` folder and `jadpo.lock`; `integrations/` folder and keyword removed | 1 | Input to area 1 |
| Generated webhook routes in the `webhooks` API, replacing question 8's hand-written routes; every Jadpo API publishing a package from its API record | 7 | Input to area 7 |
| Webhook signature as the generated route's authentication; service keys as the credential for calling another Jadpo app | 8 | Input to area 8 |
| Integration time limits inside a route's deadline: the compiler warns when `timeout` times `attempts` exceeds a reaching route's deadline | 7 | Answered here |
| The delivery key as the idempotency key; one changing call per action; `outcome_unknown` parked; provider events published as events | 12 | Answered here, agreeing with area 12 |
| Fakes named `connection.resource.operation`; generated defaults; contract runs required for unconfirmed operations | 13 | Input to area 13 |
| `jadpo provider add`, `upgrade`, `import <format>`, `record`; the registry client; `jadpo.lock`; contract runs in CI | 14 | Input to area 14 (its question 15) |
| Connections that receive marked fields, listed in the personal-data report | 15 | Input to area 15 |
| Approval report entries: operations, data sent, package diffs, unconfirmed operations, calling domains | 17 | Input to area 17 |
| Readiness does not probe providers; a provider outage should not take the app out of service | 7 | Answered here |
| Magic links and OIDC code exchange (area 8) are ordinary providers; no special feature | 8 | Answered here |
| Mail templates and translations for message bodies | 10 | Deferred |
| `connections:` replacing `stores:`; `driver: Driver.x` replacing `engine:`; cache in memory in development | 5 | Questions 20 and 23 |
| `Sms` type base and a `PhoneNumber` type | 2 | Question 19 |
| Event delivery stays in the runtime; publishing targets fed from the outbox; schedule ticks chosen by target | 12 | Questions 19 and 24 |
| Mail and files sections use capabilities; outgoing webhooks as a built-in publishing driver | 16 | Questions 19 and 21 |
| One fake per capability (`mail.sent`, `sms.sent`, `published`) | 13 | Question 22 |
| `jadpo provider conform`; local inbox for SMS and published events; Wrangler config generated per target | 14 | Questions 22–24 |
| Capabilities in the manifest, network and personal-data reports; `erase` mappings generating `PersonErased` subscribers | 15, 18 | Question 21 |
| `privacy: { self_service: { mail: } }` names the app's mail connection | 15 | Question 20 |

### Open questions

Questions keep their numbers, because other pages cite 6 and 7. Questions
1 to 5 and 9 to 12 were decided on 2026-10-12 and reopened on 2026-10-13;
13 to 18 are new; 19 to 25 (capabilities) were raised on 2026-10-13 and
are open.

1. **How are request and response types written?** *Reopened 2026-10-13;*
   first answered as hand-written types checked against the contract.
   *Recommendation:* they come from the package and the app writes none; a
   type built from one (`pick: [Stripe.PaymentIntent.id, ...]`) narrows it. At runtime a response is checked only for the fields the
   code reads. TypeScript SDKs and Smithy clients work the same way.
   *Decided 2026-10-13 (Richard):* package types come in namespaced
   (`Stripe.PaymentIntent`) and are usable anywhere in code; no narrowing or
   derived types are needed. Two limits: a provider object or enum type
   cannot be an entity field (it would tie stored data to the provider's
   version) or part of a route's `Input` or `Output` (it would make the
   provider's schema part of the app's API); ids and plain values can.
2. **How is an operation called?** *Reopened 2026-10-13;* first answered as
   `StripeV2024.create_payment_intent(...)`. *Recommendation:*
   `connection.resource.operation(input)`, only from actions, with `attempt`
   or `match`, which reads like the provider's own SDK, so agents already
   know it. *Decided 2026-10-13:* `connection.resource.operation(input)`, as recommended.
3. **How are provider versions handled?** *Reopened 2026-10-13;* first
   answered as one integration per version. *Recommendation:* one package per
   version; two versions are two connections run side by side; upgrade by
   adding the next, moving callers, and deleting the old. The approval
   report shows the diff of the operations the app uses. This is still
   Stripe's own advice. *Decided 2026-10-13:* yes, as recommended.
4. **How is a package pinned?** *Reopened 2026-10-13;* first answered as a
   contract file whose digest goes in the approval record.
   *Recommendation:* registry packages by digest in `jadpo.lock`, local ones
   by being committed; never fetched during a build. Cargo and npm lock
   files work this way. *Decided 2026-10-13:* yes, as recommended.
5. **Which failures can be mapped, and to what?** *Reopened 2026-10-13;*
   first answered with the contract's error responses. *Recommendation:* the
   package's failures, by code, at the call, to any declarable failure of a
   fitting kind; `timed_out` to an `unavailable` failure only on idempotent
   operations; anything unmapped is a fault. A package failure is never
   listed in an app's `fails`, so a provider's codes never become part of
   the app's API. *Decided 2026-10-13:* yes, as recommended.
6. **Who supplies the idempotency key?** *Recommendation:* the runtime: the
   delivery key in subscribers and schedules, the client's key or request id
   in routes; the operation only says where it goes. Today's
   `ReminderIntentId` and its two clauses go. *Decided 2026-10-12:* yes, as recommended.
7. **May one action call a provider and also write?** Today no write at all
   is allowed. *Recommendation:* yes, if the call comes first; the writes
   commit after it returns, and a failed commit retries the call with the
   same key. Writes before the call stay an error; that case emits an event. *Decided 2026-10-12:* yes, as recommended.
8. **Which retry settings exist?** *Recommendation:* `timeout:` (default
   10s) and `retry: { attempts: within: }` (default 3 within 30s), the same
   words as area 12's subscriber retry; exponential backoff with full jitter
   fixed. Temporal exposes five knobs, mostly left at defaults. *Decided 2026-10-12:* yes, as recommended.
9. **How is the outbound host controlled?** *Reopened 2026-10-13;* first
   answered as derived from `base_url`. *Recommendation:* the package lists
   its live and sandbox hosts and the connection's `environment:` picks one;
   a package with no fixed host takes `host: config.x`. HTTPS only, no
   redirects or proxies, and a host change in an upgrade is flagged in the
   approval report.
   *Decided 2026-10-13 (Richard, after Deno's permissions):* outbound
   network access is denied by default. Jadpo code has no raw `fetch`; the
   runtime refuses any host not listed by a declared connection's package;
   the build writes a network report of every reachable host, its connection
   and the domains using it; a new connection or a changed host appears in
   the approval report. Connections are the permission list, with no second
   list of hosts.
10. **Which credential schemes first?** *Reopened 2026-10-13;* first
    answered on the integration. *Recommendation:* the package declares the
    scheme and the connection gives only `credential: config.x`. Bearer,
    named header and basic first; AWS signing arrives with the Smithy
    importer and the first AWS package; OAuth client credentials when a real
    app needs them. *Decided 2026-10-13:* yes, as recommended.
11. **How are webhooks received?** *Reopened 2026-10-13;* first answered as
    a hand-written route using the integration's signature check.
    *Recommendation:* generated from the package when the connection has a
    webhook secret: signature checked with a five-minute tolerance, repeats
    dropped by event id, each event published as a Jadpo event. No webhook
    route is written by hand. Area 7's question 8 is amended. *Decided 2026-10-13:* yes, as recommended.
12. **How are fakes made and checked?** *Reopened 2026-10-13;* first
    answered as fakes checked against the contract file. *Recommendation:*
    a fake generated per package answers with valid defaults; a test
    scripts responses with `fakes:`; the compiler checks scripted responses
    against the package's types; `jadpo test --kind contract` runs against
    the sandbox in scheduled CI and must cover every unconfirmed written
    operation. *Decided 2026-10-13:* yes, as recommended.
13. **How is a package made for an API with no published specification?**
    *Recommendation:* an agent writes the package files from the provider's
    docs, in exactly the format an importer produces, with only the
    operations the app uses. Importers for a Postman collection, a HAR
    capture and the official SDK's `.d.ts` files cover the common cases with
    something to start from. One format means one compiler path and one
    review.
    *Richard asked (2026-10-13)* whether agents should work out where to
    look. *Revised recommendation:* no; `jadpo provider add <name>` follows a
    fixed order and records which step it used: (1) the registry, official
    package first; (2) the provider's published specification (OpenAPI,
    Smithy, GraphQL), given a URL or found in its known places; (3) an
    importer from the official SDK's type definitions, a Postman collection
    or a HAR capture; (4) written from the provider's documentation. The
    project's agent instructions (generated by `jadpo new`) say the same, so
    an agent never improvises the order. *Decided 2026-10-13:* the fixed order in the revised recommendation.
14. **How is an operation's origin recorded and confirmed, and do
    production builds refuse unconfirmed ones?** *Recommendation:* `source:
    PackageSource.imported` or `.written` (with `docs:`) on every operation;
    `jadpo provider record` saves a placeholder-valued sandbox response that
    the output type is checked against. Unconfirmed written operations are
    flagged in the approval report and need a passing scheduled contract run,
    but do not block a build: a sandbox outage should not stop a release. *Decided 2026-10-13:* yes, as recommended.
15. **What is the registry, and who publishes to it?** *Recommendation:*
    Jadpo's own registry, declarations only, signed, pinned by digest, never
    fetched during a build. Start with curated packages (Stripe, Twilio,
    GitHub) in the Jadpo repository and local packages in `providers/`;
    host a registry, and open publishing with verified namespaces, only when
    outside demand appears. Jadpo APIs publish their own packages from the
    API record.
    *Ideas recorded (Richard, 2026-10-13), for later; not decided:*
    - The registry holds Jadpo packages in general, not only providers.
    - A package generated from a provider's OpenAPI file is ordinary Jadpo
      source. When a provider publishes its own Jadpo package in its
      repository, the registry marks it official and serves a cached,
      digest-pinned copy instead.
    - `jadpo packages outdated` checks every package, or one, for updates.
      For a chosen version, the upgrade plan fetches the changelog and
      compiles the app against it, then lists the calls and types affected,
      the compile errors that would follow, and behaviour changes (new or
      removed failures, fields, events). The same works for a downgrade. An
      agent shows the plan to a person, who decides.
    *Staging (recommendation, 2026-10-13):* first build: the package format,
    connections, generated fakes, the OpenAPI importer, hand-written packages
    and `jadpo provider record`, with curated packages kept in the Jadpo
    repository and copied into `providers/`. The golden todo's mail provider
    is fictional, so its package is hand-written and exercises the no-spec
    path. Later, each when first needed: the Smithy importer (first AWS
    package), the SDK-types, Postman and HAR importers, the hosted registry,
    then upgrade plans. *Decided 2026-10-13 (Richard: "sounds like a plan"):* Jadpo's own registry of declarations, staged as above; the recorded ideas stay ideas.
16. **How are packages and their types named?** *Recommendation:* the
    provider in UpperCamelCase and its own version label, `Stripe.v2024_06_20`,
    `AcmeCouriers.v1`. Types, failures and events are named by the package
    without the version (`Stripe.PaymentIntent`), so two connections to one
    version share types; while two versions are connected during an upgrade,
    names carry the version (`Stripe.v2025_01_01.PaymentIntent`). *Decided 2026-10-13:* yes, as recommended.
17. **How are connections named?** *Recommendation:* lower_snake_case,
    chosen by the app (`stripe`, `couriers`); they share one namespace with
    domains and libraries, so a clash is a compile error. Resource and
    operation names come from the package, following the provider's SDK where
    the importer can map them. *Decided 2026-10-13:* yes, as recommended.
18. **Which domains may call a provider?** Area 1 put integrations in a
    domain, so only that domain called them. *Recommendation:* the
    connection lists them, `domains: [payments]`, required; other domains go
    through that domain's public actions. It keeps area 1's ownership, and a
    new caller is a one-line diff in review. *Decided 2026-10-13:* yes, as recommended.
19. **Which capabilities exist, and how is one added?** Raised 2026-10-13
    from Richard's idea that a package states its purpose. *Recommendation:*
    seven in the first build: `database` and `cache` (built-in drivers
    only), `files`, `mail`, `sms`, `publishing` (chosen public events sent
    outward from the outbox) and `schedules` (built-in ticks only). Event
    delivery stays in the runtime; telemetry is OpenTelemetry; push
    notifications come later. Adding one is a language change through the
    review, always with the same parts: a native API, standard types, a
    local driver, a fake and a conformance suite. Packages cannot invent
    capabilities. Dapr and Encore also own a fixed list; Dapr shows that a
    generic escape hatch (its "bindings") undoes the point.
20. **Do `stores:` and `providers:` become one `connections:` block?**
    Raised 2026-10-13. *Recommendation:* yes. Each entry is
    `driver: Driver.x` (built in) or `package: X.vN`, with typed settings
    the driver or package declares. The use site says which capability it
    serves: `store:` on entities, caches and `File` fields; `email:`, `sms:`
    and `publishing:` in `application { }`. One connection per resource
    (bucket, queue), not per vendor account, following Cloudflare's
    bindings: R2 and Queues need different credentials, and cron none.
21. **How does a package provide a capability?** Raised 2026-10-13.
    *Recommendation:* `provides:` in the index names a `provide <capability>`
    file mapping each standard operation, standard event and optional
    `erase` onto the package's own operations, with field paths, literals,
    lists, nested blocks and enum tables only. Provider features come
    through typed connection settings and the package's own API, never
    through the capability. Protocol drivers, signing and webhook checks are
    built into the runtime, so packages stay declarations only. An `erase`
    mapping generates area 15's `PersonErased` subscriber.
22. **How is a capability package proven, and how is it faked?** Raised
    2026-10-13. *Recommendation:* one conformance suite per capability, run
    by `jadpo provider conform` against the sandbox or an emulator; the
    registry lists a capability only with a passing run pinned to the
    digest, and local packages run it in scheduled CI. Tests get one fake
    per capability (`email.sent`, `sms.sent`, `published`), whatever the
    provider; a package's own operations keep their generated fake.
23. **What runs in development?** Raised 2026-10-13. *Recommendation:*
    nothing leaves the machine: mail and SMS to the local inbox, files to
    local disk with local signed links, published events to a log in the
    dev console, schedules on the local timer. The database runs its real
    engine; the cache runs in memory. That narrows area 5's "same engine in
    every environment" for caches, which is safe because a cache cannot
    change what a request returns. Encore and Rails do the same.
24. **How do targets limit drivers?** Raised 2026-10-13 with the native
    and WebAssembly targets. *Recommendation:* every driver and package
    lists the targets it runs on; `jadpo check` refuses a deployable that
    uses one elsewhere and names the alternative. The schedule tick is the
    target's: an in-process timer natively, Cloudflare cron on Workers
    (generated into the Wrangler config, also sweeping the outbox), or an
    external tick. On Workers: TCP works but port 25 is blocked, so SMTP
    needs port 587; there is no local disk; bindings exist only there.
25. **How are the gaps the modelled packages found closed?** Raised
    2026-10-13. *Recommendation:* five small additions, each built with the
    first package that needs it:
    - a package's sandbox may be a test credential rather than a host
      (Postmark, Twilio);
    - a host may hold one placeholder filled from a typed setting
      (`{account_id}.r2.cloudflarestorage.com`), shown as a pattern in the
      network report and checked at runtime;
    - the credential scheme decides the credential's shape (SigV4 takes a
      key id and a secret);
    - new webhook checks: basic auth (Postmark, which signs nothing),
      Twilio's HMAC-SHA1 and AWS SNS certificates;
    - `connection.webhook_url` as a mapping value (Twilio's status
      callback).

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Keyword and folder | Carried from area 1: `integration` in `integrations/`, one declaration per provider version, contract beside it (Superseded 2026-10-13: reopened by questions 15 and 18.) | 2026-10-12 |
| Mapping form | Carried from area 4: `condition: Failure`, e.g. `timed_out: ChargeUnavailable`; time limits are integration settings | 2026-10-12 |
| Faults | Carried from area 4: faults are never listed in `fails`; integrations map provider problems to declared failures | 2026-10-12 |
| Calls | Carried from areas 1 and 3: the dot names the owner; `attempt` only on calls that can fail; actions have no settings | 2026-10-12 |
| Colon rule and options | Carried from areas 2 and 3: members are `name: value`; closed options are lower_snake_case names (Superseded 2026-10-12: options are written `Type.value`; see area 2 question 17.) | 2026-10-12 |
| Setting options | Carried from area 2: fixed options are enum values written `Type.value`; every member in braces is `name: value` | 2026-10-12 |
| Calling integrations | Carried from area 9: whoever may run the action; no separate grant | 2026-10-12 |
| Third-party services | Carried from area 8: the language owns a feature rather than relying on a third-party service where it reasonably can | 2026-10-12 |
| Types and fields | Written by hand and checked against the contract; may leave out unused output fields; one type may serve two versions (Superseded 2026-10-13: reopened as question 1.) | 2026-10-12 |
| Naming and calls | UpperCamelCase with the version (`StripeV2024`), called `StripeV2024.operation(...)` only from actions (Superseded 2026-10-13: reopened as questions 2, 16 and 17.) | 2026-10-12 |
| Provider versions | One integration per version, both runnable at once; upgrade by adding the next and moving callers; `version_header:` (Superseded 2026-10-13: reopened as question 3.) | 2026-10-12 |
| Contract pin | `contract: "file"` only; the file's digest goes in the approval report (Superseded 2026-10-13: reopened as question 4.) | 2026-10-12 |
| Failure conditions | Contract error responses map to declarable failures; `timed_out` to `unavailable` only with an idempotency key; unmapped is a fault (Superseded 2026-10-13: reopened as question 5.) | 2026-10-12 |
| Idempotency keys | Supplied by the runtime: delivery key in subscribers and schedules, the client's `Idempotency-Key` or request id in routes | 2026-10-12 |
| Call then write | Allowed when the call comes first; writes commit after it; a failed commit retries the call with the same key; area 3's text amended | 2026-10-12 |
| Retry settings | `timeout:` (10s) and `retry: { attempts: within: }` (3 within 30s); backoff fixed | 2026-10-12 |
| Outbound host | From `base_url`, HTTPS only, no redirects or proxies; a sandbox host chosen by configuration (Superseded 2026-10-13: reopened as question 9.) | 2026-10-12 |
| Credential schemes | `CredentialScheme.bearer`, `.header`, `.basic` first (Superseded 2026-10-13: reopened as question 10.) | 2026-10-12 |
| Webhook signatures | On the integration (`webhooks: { signature: SignatureScheme.x secret: }`), used as `authentication: StripeV2024.signature`; five-minute tolerance (Superseded 2026-10-13: reopened as question 11.) | 2026-10-12 |
| Checking fakes | Fakes checked against the contract by the compiler; `jadpo test --kind contract` against the sandbox in scheduled CI (Superseded 2026-10-13: reopened as question 12.) | 2026-10-12 |
| Types from the package | Namespaced package types usable anywhere in code; no narrowing needed; provider object and enum types barred from entity fields and route `Input`/`Output`; ids and plain values allowed | 2026-10-13 |
| Calling an operation | `connection.resource.operation(input)`: the connection from `app.jadpo`, the resource group and operation from the package | 2026-10-13 |
| Versions as connections | One package per version; two versions are two connections side by side; the approval report shows the diff of operations used | 2026-10-13 |
| Package pin | Registry packages by digest in `jadpo.lock`, local ones committed; never fetched during a build | 2026-10-13 |
| Failures at the call | Package failures mapped by code at the call to declarable failures; `timed_out` to `unavailable` only when idempotent; unmapped is a fault; never in the app's `fails` | 2026-10-13 |
| Hosts | Outbound access denied by default; only hosts of declared connections; no raw `fetch`; generated network report; changes in the approval report; no separate host list | 2026-10-13 |
| Credentials | The package declares the scheme; the connection gives `credential: config.x`; bearer, header, basic first; AWS signing with the first AWS package | 2026-10-13 |
| Webhooks as events | Generated when the connection has a webhook secret: signature checked, repeats dropped by event id, each published as an event; no hand-written route | 2026-10-13 |
| Fakes and contract runs | Generated fake per package; tests script responses checked against the package; scheduled sandbox run covers unconfirmed operations | 2026-10-13 |
| The no-spec path | `jadpo provider add` tries, in order: registry (official first), published specification, SDK types / Postman / HAR importers, written from docs; records the step used | 2026-10-13 |
| Provenance and confirmation | `source: PackageSource.imported` / `.written` (with `docs:`) per operation; `jadpo provider record` confirms; unconfirmed flagged in the approval report and covered by the contract run; builds not blocked | 2026-10-13 |
| The registry | Jadpo's own, declarations only, signed and digest-pinned; first build: package format, connections, fakes, OpenAPI importer, hand-written packages, `jadpo provider record`, curated packages in the Jadpo repo; hosted registry, other importers and upgrade plans later | 2026-10-13 |
| Package naming | `Stripe.v2024_06_20`; types `Stripe.PaymentIntent`, versioned names only while two versions are connected | 2026-10-13 |
| Connection naming | lower_snake_case, chosen by the app, sharing a namespace with domains and libraries | 2026-10-13 |
| Calling domains | Each connection lists `domains: [...]`; other domains go through that domain's public actions | 2026-10-13 |
| Capabilities | | |
| Connections | | |
| Providing a capability | | |
| Conformance and fakes | | |
| Development drivers | | |
| Targets | | |
| Gaps from the modelled packages | | |
