---
title: External services
area: 11
status: decided
checked-with: this checkout's jadpo on every gate run
---

# External services
<!-- topic: external-services -->

An external service is another company's API, such as Stripe, a courier
or another Jadpo app, that your actions call. You call it like an SDK:
`couriers.bookings.create(request)`. Jadpo makes the HTTP request, adds the
secret, picks the host, retries and reads the response. Your code never
builds a URL, sets a header or sees a raw response.

Each API is described once, in a **provider package**: Jadpo source that
lists the API's operations, types, failures and webhook events. It also
says which calls change something, which take an idempotency key (a value
that lets the provider spot a repeated request), how values are written on
the wire, how errors and pages of results come back and how webhooks are
signed. Your app connects to the package in `app.jadpo`, and then calls
its operations.

The other kind of package, a **library package**, holds types, failures
and pure functions that several apps share, such as a postcode parser.

The next topic says how packages, providers, connections, drivers,
capabilities, built-in services and the services Jadpo runs fit together,
and where each runs.

| Topic | In short |
|---|---|
| How the pieces fit | Package, provider, connection, driver, capability, built-in service and service run by Jadpo: what each is, where it runs, what is built; Docker or Podman only for a service run by Jadpo |
| Kinds of package | Library (types, failures, pure functions) and provider (one API); the kind follows from what is inside |
| The golden rule | A package can do nothing the app cannot, and the app can do everything a package can |
| The format | `package.jadpo` (`licence:`, `jadpo:`), an optional `provider.jadpo`, folders laid out as `shared/` is, `public` as the API |
| Where packages live | Installed in `packages/<name>/<version>/`, committed, digest-checked, read-only; the app's own in `shared/providers/` |
| Where they come from | Jadpo's registry, a git repository, an importer, another Jadpo app's OpenAPI, or written by an agent from the docs |
| The surface | `surface.jadpo_record`, generated and readable, pointing at each file; `jadpo package show` |
| Connecting | `connections:` in `app.jadpo`; built-in drivers are configured there, never packaged |
| Calling | `couriers.bookings.create(request)` and package actions such as `stripe.start_subscription(...)`, from actions only, failures mapped at the call |
| Decoding | The package declares how values are written on the wire; the runtime decodes; the app translates |
| Package code | No state; package actions call only their own API; events only about the outside world; polls; services |
| Several calls in order | Each step recorded before it is sent, replayed on a rerun, stopped as `diverged` on any difference |
| Receiving | Webhooks, replies, handshakes and queues, each arriving as an event |
| Acting for each user | OAuth grants owned by one of the app's records; `on_behalf_of:` on every call |
| Capabilities | Only jobs Jadpo itself does, each whole (email, files, search and the rest); the provider chosen in `app.jadpo` |
| Development | Provider calls faked by default, let through to the sandbox by name, every call logged |
| Tests | A generated fake per package; scheduled contract runs against the sandbox |

## How packages, connections, capabilities and services fit together
<!-- topic: how-integrations-fit-together -->

Seven terms name the ways an app reaches past its own code: package,
provider, connection, driver, capability, built-in service and service
run by Jadpo. This topic says what each one is and where it runs. The
topics it links hold the rules.

Most apps use only two of them. A *connection* in `app.jadpo` opens a
database or a provider's API, and your actions call through it. You
don't need Docker or Podman for any of this: only a service run by Jadpo
uses a container.

### What each one is

| Term | What it is | Declared in | Runs |
|---|---|---|---|
| **Package** | Jadpo source shared between apps, laid out as `shared/` is. A *library package* holds types, failures and pure functions (`Postcodes.uk.parse`); a *provider package* describes one outside API: its operations, types, failures, webhook events and how it is reached ([Kinds of package](#kinds-of-package), [The package format](#the-package-format)) | `packages/<name>/<version>/` when installed; the app's own in `shared/providers/` | Compiled into the app and run in its process. A package is never a process, a container or native code of its own |
| **Provider** | The outside API a provider package describes: Stripe, a courier, another Jadpo app, Gotenberg. The package's `provider.jadpo` says how it is reached | Nowhere in the app: it is the other end of the call | Wherever its owner runs it: on the internet (`hosts:`), at an address a setting gives (`host:` on the connection), or in a container Jadpo starts (`runs:`) |
| **Connection** | One named entry in `connections:` in `app.jadpo`: a built-in driver (`driver: Driver.x`) or a provider package (`package: X.vN`), with its settings, its credential and the domains whose actions may call it. One connection is one resource: a database, a bucket, one account at a provider ([Connecting in `app.jadpo`](#connecting-in-appjadpo)) | `app.jadpo`, its values read from `config.jadpo` settings | Opened by the runtime, inside the app |
| **Driver** | Native code in Jadpo's runtime that speaks one protocol: PostgreSQL, SMTP, the disk, Cloudflare Queues, OpenAI-compatible model servers; Redis, S3 and OpenID Connect to come. Drivers arrive only with a Jadpo release, never in a package ([No presets, no dependencies](#no-presets-no-dependencies)) | `driver:` on a connection | Inside the app, talking to a server the operator runs |
| **Capability** | A job Jadpo itself does, with one API in app code whatever fills it: `database`, `cache`, `files`, `email`, `search`, `documents`, `ai` and the rest. One connection fills it: a driver, or a provider package whose `provides/<capability>.jadpo` maps the job onto its operations ([Capabilities](#capabilities)) | Chosen in `app.jadpo`: `email: { connection: mail }`, `store: sheets` on a `File` field | Wherever the connection that fills it runs |
| **Built-in service** | A backend feature Jadpo provides, so the app installs nothing: feature flags, rate limits, files, email, audit, telemetry, search, documents, bot protection ([Built-in services](16-built-in-services.md)). Email, files, search and documents do their outside work through a capability; flags, rate limits and audit keep their state in the app's own stores | Where [Built-in services](16-built-in-services.md) says: `flags.jadpo`, `rate_limit:`, `type X = EmailMessage`, `File` fields | Inside the app's runtime. It is a feature, not a process or a container |
| **Service run by Jadpo** | A provider whose API is a stateless container image, declared by `runs:` in its package's `provider.jadpo` (Gotenberg turning HTML into PDFs, ClamAV scanning uploads). Jadpo starts and supervises the container through Docker or Podman and hands its connection the address ([Services](#services)) | `runs:` in the package, named by a connection | A container beside the app on the same machine |

Two words mean two things each:

- **"Service".** A built-in service is part of Jadpo's runtime, inside
  the app's process; a service run by Jadpo is a container beside it. No
  built-in service starts a container; one can use a service run by Jadpo
  through its capability, as documents can use Gotenberg.
- **`runs:`.** In a package's `provider.jadpo` it declares a container;
  on a deployable in `app.jadpo` it lists the background work that
  deployable runs, `runs: [Trigger.schedules]`
  ([Declared deployables](14-tooling.md#declared-deployables)).

### How they relate

```text
the app's process: jadpo dev's interpreter, or a native or Workers deployable
│
├── domain code ── calls ──► library package             pure functions, in process
│
├── domain code ── calls ──► connection: provider package ── HTTPS ──► provider's API
│                                                    on the internet (hosts:), at host:,
│                                                    or in a container Jadpo runs (runs:)
│
└── built-in services: flags, rate limits, email, files, search, documents, ...
       │ outside work goes through a capability
       └──► capability: email, files, database, documents, ai, ...
              filled by one connection, either
              ├── a built-in driver ──────────── its protocol ──► a server the operator runs:
              │                                                  PostgreSQL, SMTP, S3
              └── a provider package's provides/ ── HTTPS ──────► provider's API:
                                                                 Resend, Postmark, Gotenberg
```

- **Code calls a provider's operations only through a connection**, from
  an action in a domain the connection lists: `couriers.bookings.create(...)`
  ([Calling](#calling)).
- **Code never names what fills a capability.** It writes an
  `EmailMessage` type or a `File` field, and `app.jadpo` picks the
  connection, so changing provider changes `app.jadpo` and its settings,
  never domain code.
- **One provider package can be both.** Postmark fills `email` through
  `provides/email.jadpo`, and the domains its connection lists call its
  own operations (hosted templates, suppression lists) as any provider's.
- **Packages fill capabilities and never invent them**, never bring a
  driver and never depend on another package.
- **Anything that keeps data is the operator's.** PostgreSQL, Redis, an
  S3 bucket and an SMTP server are reached through a driver at the
  address a setting gives; Jadpo never runs them. A service run by Jadpo
  holds no data, and each app instance runs its own copy.

### Where each runs

| | `jadpo dev` | `jadpo test` | Native deployable | Workers deployable |
|---|---|---|---|---|
| Library package | In process | In process | Compiled in | Compiled in |
| Provider package's calls | Faked by default, from its recordings or generated answers; operations named in `development: { sandbox: }` go to the sandbox, never live ([Provider calls in development](#provider-calls-in-development)) | Always faked; a test scripts answers with `fakes:` ([Tests and fakes](#tests-and-fakes)) | HTTPS to the live or sandbox host the connection's `environment:` picks, or to its `host:` | The same, through the Worker's `fetch` |
| Database, `Driver.postgres` | The address its setting gives, else a disposable PostgreSQL started from the machine's PostgreSQL tools | A copy per test on a disposable PostgreSQL, or on `JADPO_TEST_DATABASE_URL`'s server | The operator's PostgreSQL | The operator's PostgreSQL, through Hyperdrive |
| Email, by SMTP or a mail package | Caught in the dev inbox | Caught in `email.sent` | The SMTP server, or the mail package's API | The same; SMTP on port 587 only |
| Files | The disk driver, in `.jadpo/files/` | An in-memory store | The disk (one server) or S3 | S3, R2 included |
| Service run by Jadpo, `runs:` | Started as a container by `jadpo dev` | Never started: the package's fake answers | A container beside the app: `jadpo build` writes the configuration that runs it (Compose first), then the deployable supervises it itself | None: Workers runs no containers, so the connection gives `host:` where the service is hosted elsewhere |

`jadpo check` refuses a deployable whose target can't run a connection
it reaches, naming what to use instead: on Workers the disk, a service
run by Jadpo without `host:`, and SMTP on port 25
([Capabilities](#capabilities), "Targets").

### How Docker fits

- **An app needs no Docker or Podman** to be checked, tested, run in
  `jadpo dev` or built. Nothing above but a service run by Jadpo uses a
  container: the disposable PostgreSQL comes from the machine's own
  PostgreSQL tools, a native deployable is one binary
  ([A native deployable](14-tooling.md#a-native-deployable)) and a
  Workers deployable is a Worker. Neither is an image.
- **Docker or Podman comes in only for a service run by Jadpo.** Jadpo
  talks to whichever engine the machine has, through the
  Docker-compatible API both serve. It creates each container itself. A
  container has no outbound network until its connection grants it, is
  never privileged and can't reach the host's disk. A new or changed image
  runs only once a person accepts it in the approval report.
  `jadpo services` lists, starts and stops each one and shows its logs
  ([Tooling](14-tooling.md)). You never write a Dockerfile or a Compose
  file, and an app with no such connection never needs an engine
  ([Services](#services)).
- **The operator's infrastructure is their own.** They may run
  PostgreSQL or an SMTP server in Docker, or anywhere else; the app sees
  only the address its setting gives.

### In an app

One app with each kind of connection: a database on the PostgreSQL
driver, mail on the SMTP driver filling the `email` capability, and the
app's own provider package for a courier, which the `shipping` domain
calls:

```jadpo check project=pieces-fit file=app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
            credential: {
                username: config.smtp_username
                password: config.smtp_password
            }
        }
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
}
```

```jadpo check project=pieces-fit file=config.jadpo hidden
config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
    database_url: PostgresUrl
    mail_sender: Email
    smtp_host: Hostname
    smtp_password: Text {
        secret: true
    }
    smtp_username: Text
}
```

```jadpo check project=pieces-fit file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=pieces-fit file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
    credential: CredentialScheme.bearer
    error_code: "error.code"
}
```

```jadpo check project=pieces-fit file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
public resource bookings {
    create: {
        method: HttpMethod.post
        path: "/v1/bookings"
        input: CreateBooking
        output: Booking
        changes: true
        idempotent: true
        failures: [SlotTaken]
        source: PackageSource.written
    }
}
```

```jadpo check project=pieces-fit file=shared/providers/acme_couriers/v1/types/postcode.jadpo hidden
/// Where a collection is.
public type Postcode = Text {
    max_length: 8
}
```

```jadpo check project=pieces-fit file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Postcode
}
```

```jadpo check project=pieces-fit file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: Text
}
```

```jadpo check project=pieces-fit file=shared/providers/acme_couriers/v1/failures/slot_taken.jadpo hidden
/// The slot went before the booking arrived.
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "That slot is taken."
}
```

```jadpo check project=pieces-fit file=domains/shipping/failures/slot_gone.jadpo hidden
/// The slot the customer chose has gone.
type SlotGone = Failure {
    kind: FailureKind.conflict
    message: "That slot has gone; choose another."
}
```

```jadpo check project=pieces-fit file=domains/shipping/actions/book.jadpo
use app { couriers }
use shipping/failures { SlotGone }
use shared/providers/acme_couriers

/// Books a collection with the courier, through its connection.
action book(postcode: AcmeCouriers.Postcode) fails SlotGone -> AcmeCouriers.Booking {
    return attempt couriers.bookings.create(
        AcmeCouriers.CreateBooking {
            postcode: postcode
        }
    ) {
        slot_taken: SlotGone
    }
}
```

A service run by Jadpo: Gotenberg's package declares its container, and a
connection to it fills the `documents` capability. In `jadpo dev` and a
native deployable, Jadpo starts the container. On Workers the connection
would give `host:` instead:

```jadpo fragment
// packages/gotenberg/v8/provider.jadpo
/// Gotenberg's HTTP API, run by Jadpo as a container beside the app.
provider {
    protocol: Protocol.multipart_http
    runs: {
        image: "gotenberg/gotenberg:8@sha256:…"
        port: 3000
        asks: ["fonts.googleapis.com"]
    }
}

// app.jadpo
application {
    connections: {
        pdfs: {
            package: Gotenberg.v8
            domains: [billing]
            outbound: ["fonts.googleapis.com"]
        }
    }
    documents: {
        connection: pdfs
    }
}
```

### What is built of these

Built: library and provider packages, installed from a folder or git
and locked; connections and the network report; provider calls with
decoding, retries and idempotency keys; fakes in development and tests,
recordings and sandbox calls; webhooks, queues, polls and package
actions; the drivers for PostgreSQL, SMTP, the disk (in `jadpo dev` and
tests), Cloudflare Queues and OpenAI-compatible servers; the `database`,
`email` (SMTP, Resend and other mail packages), `receiving`,
`schedules` and `ai` capabilities; and the built-in services email,
rate limits and the first part of files.

Not built yet: **services run by Jadpo** (`runs:`, `jadpo services`,
and with them every use of Docker or Podman); the Redis and S3 drivers
and a deployable's disk; OpenID Connect sign-in providers and OAuth
grants; the `cache`, `publishing`, `search`, `documents`, `bot_check`
and `scanning` capabilities; the built-in services flags, audit,
telemetry's section, search, documents and bot protection; the hosted
registry; importers other than `openapi`; and choosing a provider when
deploying. Each page's "What is built" table is the record.

## Kinds of package
<!-- topic: kinds-of-package -->

A package is Jadpo code shared between apps. There are two kinds, written
in the same format:

| Kind | Contains | Used as |
|---|---|---|
| Provider | One API at one version: resources, operations, types, failures, events and how the API is reached; optionally package actions, subscribers, capability mappings and library functions | A connection: `stripe.payment_intents.create(...)` |
| Library | Types, failures and library functions under `shared/`'s rules: pure, no entities, no routes, no network | `Postcodes.uk.parse(text)`, `Postcodes.UkPostcode` |

A *resource* is a group of an API's operations, such as Stripe's
`payment_intents`. A *library* is a named group of pure functions.

### Which kind a package is

You don't declare the kind: it follows from what is inside. A package
with a `resources/` folder is a provider package, and you need a
connection to call its operations. One without is a library package. A
provider package's library functions can be called without a connection.

### What can be stored

A library package's types may be stored in entity fields and used in a
route's input and output. When an upgrade changes a stored type, the
change shows in the upgrade plan and the migration, as a change to one of
your own types does.

Everything in a provider package follows the provider rules, its library
types included. You may store only ids and plain values from it, and no
provider object or enum type may be part of a route's `Input` or
`Output`. The next version of the API is a new package, so your stored
data and your API must not depend on this one. Keep what you need from a
provider as ids, plain values and built-in types (money, countries, phone
numbers), or as your own types.

### What isn't a package

- **A feature isn't a package.** A whole slice of an app, such as
  invitations or billing, is never installed or copied in as a package.
  Your coding agent writes it from these pages, and the compiler catches
  the dangerous mistakes. Most of what other frameworks take from packages,
  such as sign-in, flags, audit, files and email, is built into Jadpo.
- **A driver isn't a package.** Anything that needs native code, such as a
  new database, a signing scheme or a wire format, comes only in a Jadpo
  release. Packages never carry native or WebAssembly code.

### Built in, or a library package

A thing is built into Jadpo when the compiler gains from knowing it: a
type that turns a mistake into a compile error, carries personal-data
marking, has a SQL or OpenAPI form or generated test values, or is a
security boundary the language should own. Money, countries, currencies,
languages, phone numbers, safe HTML and CSV are built in. Postcodes,
IBAN, VAT numbers, public holidays, repeating dates and QR codes are
packages.

### The golden rule

**A package can do nothing the app can't, and the app can do everything a
package can.** A package is a folder of an app's own code, published.

- Your app's own integrations in `shared/providers/` use the whole package
  format: `provider.jadpo`, `resources/`, `events/`, decoding, `provides/`
  and `runs:`. Its own types, failures, libraries, emails and translations
  live in `shared/` and its domains.
- The compiler treats `packages/` and `shared/providers/` alike. They
  differ only in where the code came from: the lock entry, the digest
  check, being read-only, and the bar a package passes to be published.
- The rule runs one way only. An app can do far more than a package:
  entities, actions over its data, routes, policy and emails.

## The package format
<!-- topic: the-package-format -->

A package is laid out as `shared/` is, so one set of rules covers the app
and every package, and the compiler checks a package as it checks
`shared/`.
A library package:

```text
postcodes/v2/
    package.jadpo                   the index: description, licence, lowest Jadpo version
    types/uk_postcode.jadpo         public type UkPostcode = Text { ... }
    failures/invalid_postcode.jadpo
    libraries/uk.jadpo              public library uk { parse(...)  normalise(...) }
    libraries/uk.test.jadpo         tests beside the file, as in an app
    translations/                   its own messages and failure wording
    GUIDE.md                        optional: how to use it well
    CHANGELOG.md
    surface.jadpo_record            generated
```

A provider package adds `provider.jadpo` and the folders an API needs:

```text
stripe/v2024_06_20/
    package.jadpo                          the same two keys as every package
    provider.jadpo                         how the API is reached and called
    resources/payment_intents.jadpo        imported, corrections merged in
    types/payment_intent.jadpo             imported
    failures/card_declined.jadpo
    events/payment_intent_succeeded.jadpo
    actions/start_subscription.jadpo       written: several of its own calls in order
    subscribers/renewal.jadpo              written: enriches its own webhook
    libraries/tables.jadpo                 written: decoders and helpers
    provides/email.jadpo                   only when it fills a capability
    recipes/track_parcels/                 wiring examples an app copies in
    recordings/payment_intents.create.json real responses that confirm the types
    surface.jadpo_record
```

- **The index lists no files.** A file is named after what it declares, so
  the compiler finds a resource by name and loads only what the app uses.
- **A re-import rewrites only imported files.** Written libraries, actions
  and corrections survive.
- **A package's tests run when it is published**, not in every app's
  `jadpo test`; the digest guarantees the code is the code that passed.
- **No `emails/` folder.** Email templates, partials and layouts are the
  app's own, and an `emails/` folder in a package is a compile error.
- **Translations** are only in library packages, for their own messages
  and failures, laid out as a domain's
  ([Packages' text: groups, calls and the surface](10-configuration-and-time.md#packages-text-groups-calls-and-the-surface)).
  An app calls them as `Postcodes.translations.<message>(...)` and rewords
  them in its root `translations/`. Provider packages show users no text,
  because their failures are always mapped to the app's own, so they
  carry none.
- **`GUIDE.md`** is optional, and says how to use the package well. Every
  code example in it is compiled and run when the package is published,
  and `jadpo package show <name> --guide` prints it.

### The index: `package.jadpo`

Every package's index has the same form: `package Name.vN { }`, a `///`
description above it, and two keys. `licence:` names its licence, and
`jadpo:` the lowest Jadpo version it needs.

The licence is one of `Licence.mit`, `.apache_2_0`, `.bsd_2_clause`,
`.bsd_3_clause`, `.isc`, `.mpl_2_0`, `.unlicense` and `.proprietary`.
Only a package installed from git may be `.proprietary`; the public
registry refuses it. There is no `requires:`, because packages have no
dependencies, and no list of files.

```jadpo fragment
// packages/postcodes/v2/package.jadpo
/// UK postcodes: parsing, normalising and checking the outward code.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

### How the API is reached: `provider.jadpo`

A package that describes an API has one `provider.jadpo`: a
`provider { }` declaration with a fixed set of keys. A package without
one is a library.

One package describes at most one API, because one API is one connection
with one set of hosts and credentials. A vendor with two APIs under
different credentials, such as Shopify's Admin and Storefront APIs, is two
packages.

| Key | Says |
|---|---|
| `protocol:` | How bodies are sent: `Protocol.json_http`, `.form_http`, `.multipart_http`, `.aws_json`. Required |
| `hosts:` | `live:` and `sandbox:`; the same host twice when the sandbox is a test credential; a host may hold one placeholder filled from a setting (`"https://{project_ref}.supabase.co"`) |
| `runs:` | A container Jadpo runs instead of a fixed host: `image:` pinned by digest, `port:` it listens on inside the container, `asks:` the outbound hosts it needs (see "Services") |
| `credential:` | The scheme (`CredentialScheme.bearer`, a named header, basic, AWS signing), which decides the shape of credential the app gives |
| `credential_header:` | For `CredentialScheme.header`, the named header the credential goes in alone (`"x-api-key"`); required for it and only for it |
| `headers:` | The fixed headers every request carries, each `"Name: value"` (`["anthropic-version: 2023-06-01"]`), never one the driver writes itself |
| `oauth:` | The provider's authorize, token and revoke addresses, for a connection whose credential is an `OAuthGrant` (see "Acting for each user") |
| `idempotency:` | `header:` (where the key goes), `window:` (how long the provider keeps a key; required), `max_length:`, the provider's error codes `in_use:` (the key's first request is still running), `reused:` (the key came with a different request) and `new_key_on:` (the first attempt failed and the key cannot be used again), and `replays_failures: true` (default false) for a provider that replays a failed answer under the same key. Left out when the API takes no key |
| `error_code:` | Where the provider's error code is in a failed response |
| `webhooks:` | `check:` (the signature scheme), `event_id:`, `event_type:`, `occurred_at:` where the body says when the event happened, `header:` for `SignatureScheme.hmac_sha256`, and `handshake:` where the provider needs one |
| `polls:` | Operations the runtime calls on a schedule, each with a default interval and the event emitted when the answer changes |
| `erasure:` | The operation that deletes a person at the provider and the key it takes (`by:`) |
| `max_concurrent:` | How many of its calls run at once per app instance |
| `settings:` | Typed values the app gives on the connection (`message_stream:`) |
| `imported:` | The description it was imported from, `file:`, that file's digest, `digest:`, and `reviewed:`, the package's types an agent has reviewed against the provider's documentation |

**Where the API is comes from exactly one place:**

- `hosts:` when it is on the internet;
- `runs:` when Jadpo runs it;
- neither, when the app gives `host:` on the connection: another Jadpo
  app, a self-hosted product, or a service on Workers.

A connection's `host:` beside `runs:` means the service is already
running there, so Jadpo doesn't start it. A package with fixed `hosts:`
refuses a `host:`.

```jadpo fragment
// packages/acme_couriers/v1/provider.jadpo
/// Acme Couriers' booking and tracking API, written from its HTML docs.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
    credential: CredentialScheme.bearer
    idempotency: {
        header: "Idempotency-Key"
        window: 24h
        max_length: 255
        in_use: "idempotency_key_in_use"
        reused: "idempotency_key_reused"
    }
    error_code: "error.code"
}

// packages/acme_couriers/v1/resources/bookings.jadpo
public resource bookings {
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

// packages/acme_couriers/v1/types/create_booking.jadpo
public type CreateBooking = Object {
    postcode: Text
    slot: Text
}

// packages/acme_couriers/v1/types/booking.jadpo
public type Booking = Object {
    id: Text
    collect_at: Instant {
        wire: Wire.unix_seconds
    }
}

// packages/acme_couriers/v1/failures/slot_taken.jadpo
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "The collection slot is no longer available."
}
```

`idempotent: true` on an operation means it is safe to send again: with
the package's `idempotency:` block, because it takes the key; without
one, because repeating it as it is changes nothing (a `PUT` that
replaces, a `DELETE`). An operation of a connection whose credential is
an OAuth grant also names the scope it needs, `scope: "calendar.events"`.

Inside the package, its names are written bare. An app names them through
the package: `AcmeCouriers.Booking`, `AcmeCouriers.SlotTaken`. That holds
in a `match` arm too (`AcmeCouriers.SlotTaken -> ...`), and for an enum's
values, `AcmeCouriers.Window.morning`.

A failure's code, `slot_taken` from its name, is the provider's error
code. Jadpo reads it from the place in the response that `error_code:`
names.

### What is public

- **`public` marks the package's API**, as it marks what a domain offers
  other domains. A `public resource` and a `public action` can be called
  through the connection. Everything else can be called only by the
  package's own actions, subscribers and polls. As in an app, `public`
  goes on a library, never on a function: every function of a
  `public library` is public, and internal helpers such as decoders live
  in a library without `public`
  ([Project layout and modules](01-project-layout-and-modules.md)).
  Internal declarations may change freely between versions. An importer
  marks every resource public.
- **Everything reachable from the public surface is public**, and every
  compile checks it. A failure a public library's function `fails` with,
  a provider operation's `failures:` and the types in a public signature
  must themselves be public. Internal failures are mapped or handled
  before they leave a public declaration. A domain's public actions,
  queries and libraries follow the same rule.
- **A package's resources and actions share the connection's namespace**
  (`stripe.customers`, `stripe.start_subscription`), so a clash between
  them is an error in the package.
- **Package events are public or not.** The app may subscribe to a
  `public` event; one without `public` reaches only the package's own
  subscribers, so a raw webhook a package enriches can stay internal while
  its enriched event is public. An importer marks webhook events public;
  subscribing to a package's non-public event is an error.

### No presets, no dependencies

**Built-in drivers are configured in `app.jadpo`, never packaged.** A
driver is native code in Jadpo's runtime that speaks one protocol.
PostgreSQL, Redis, S3, SMTP and OpenID Connect are drivers. A vendor that
speaks one of them is that driver with the vendor's settings: Cloudflare
R2 is `driver: Driver.s3` with its host and `region: "auto"`, Neon is the
PostgreSQL driver with its pooling mode, and Gmail is the SMTP driver
with its host and port. A vendor with storage and an API is a driver
connection plus a package. A driver for one vendor comes in a Jadpo
release only where a flow needs native code.

```jadpo fragment
// app.jadpo
avatars: {
    driver: Driver.s3
    host: config.r2_host
    region: "auto"
    bucket: "avatars"
    credential: {
        key_id: config.r2_key_id
        secret: config.r2_secret
    }
}
```

- **Each driver's page lists the settings for common services**, so an
  agent looks them up rather than guessing: R2, Backblaze B2,
  DigitalOcean Spaces, MinIO and Amazon S3; Neon, Supabase and Amazon
  RDS; Gmail, Microsoft 365 and Amazon SES's SMTP; Google and Microsoft.
  Each is a working connection, checked with the pages' other examples.
  `jadpo check` warns when it recognises a host that is missing one of
  its settings.
- **Sign-in providers are connections.** Google, Microsoft and other
  OpenID Connect providers are `google: { driver: Driver.oidc  issuer:
  client_id:  credential: }` in `connections:`, named by
  `sign_in: { providers: [google, microsoft] }`. Google's and Microsoft's
  settings are in [Providers](08-identity.md#providers). GitHub and Apple
  aren't plain OpenID Connect, so they are the built-in `Driver.github`
  and `Driver.apple`, which aren't built yet.

**Packages have no dependencies**: a package can't require another.

- The standard library holds the shared vocabulary, such as money,
  countries, currencies and phone numbers, which is what packages
  elsewhere mostly depend on each other for.
- Anything else is copied. A package that needs another's code includes a
  copy, checked and tested by the publishing bar like the rest of it.
- Two packages' types for the same thing are converted at the call,
  usually in one line.
- Installing a package is one folder and one line in the lock.

## Where packages live
<!-- topic: where-packages-live -->

`packages/` holds only packages you installed. Code your app's authors
wrote lives where the rest of their code lives, whatever kind it is:

```text
app.jadpo
domains/
    shipping/libraries/...           the app's helpers for one domain
shared/
    libraries/...                    the app's helpers for several domains
    providers/
        acme_couriers/v1/            the app's own integration, in the package format
packages/                            installed only: in jadpo.lock, read-only
    stripe/v2024_06_20/
    postcodes/v2/
jadpo.lock
```

- **Installed packages are committed** under `packages/<name>/<version>/`.
  Agents read them with the tools they already use, reviewers see an
  upgrade as an ordinary diff (its surface diff included), and CI needs no
  fetch step. A build never fetches anything.
- **Installed packages are read-only.** The compiler refuses one whose
  files don't match its digest in `jadpo.lock`. A digest is a fingerprint
  of the files, so any change to them shows.
- **Your app's own integrations live in
  `shared/providers/<name>/<version>/`**, in exactly the package format.
  They can have everything an installed provider can: types, failures,
  pure functions, decoding, recordings, fakes, `provide` mappings, actions,
  subscribers and services. Who may call one is still decided by the
  connection's `domains:` list.
- **A `use` line shows where a name comes from.** `use packages/postcodes`
  is always installed code, and `use shared/providers/acme_couriers` is
  the app's own. The only differences between the two are where the code
  sits, whether you can edit it and whether the publishing bar applies. So
  replacing your own integration with an official package later changes
  `jadpo.lock`, the folder and the `use` lines, and not the code that
  calls it.
- **Publishing moves code out of the app.** A package other apps depend on
  gets its own repository and versions. Every app installs it into
  `packages/`, the one it started in included.

### Two versions side by side

Two versions of one package can be held at once: a provider during an
upgrade, a candidate version while the upgrade plan compiles the app
against it, or two library versions while callers move across. Each is
its own package in its own folder, both installed or both the app's own.
While two are held, every name written through either carries its
version, such as `Postcodes.v3.UkPostcode`. One `use` line brings in both,
because it names the package's folder.

Here the app's own `Postcodes` is held at `v2` and `v3` while callers move
across:

```jadpo check project=p71c2-side file=app.jadpo hidden
application {}
```

```jadpo check project=p71c2-side file=shared/providers/postcodes/v2/package.jadpo hidden
/// UK postcodes: reading them as people write them.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71c2-side file=shared/providers/postcodes/v2/types/uk_postcode.jadpo hidden
/// A UK postcode in standard form, such as "SW1A 1AA".
public type UkPostcode = Text {
    min_length: 5
    max_length: 8
}
```

```jadpo check project=p71c2-side file=shared/providers/postcodes/v2/types/written_postcode.jadpo hidden
/// A postcode as someone typed it.
public type WrittenPostcode = Text {
    max_length: 20
}
```

```jadpo check project=p71c2-side file=shared/providers/postcodes/v2/failures/invalid_postcode.jadpo hidden
/// The text isn't a UK postcode.
public type InvalidPostcode = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=p71c2-side file=shared/providers/postcodes/v2/libraries/uk.jadpo hidden
/// Reading UK postcodes.
public library uk {
    /// Reads a UK postcode as it is written.
    function standard_form(postcode_text: WrittenPostcode) fails InvalidPostcode -> UkPostcode {
        return attempt UkPostcode(text.upper(postcode_text)) {
            invalid: InvalidPostcode
        }
    }
}
```

```jadpo check project=p71c2-side file=shared/providers/postcodes/v3/package.jadpo hidden
/// UK postcodes: reading them as people write them.
package Postcodes.v3 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71c2-side file=shared/providers/postcodes/v3/types/uk_postcode.jadpo hidden
/// A UK postcode in standard form, such as "SW1A 1AA".
public type UkPostcode = Text {
    min_length: 5
    max_length: 8
}
```

```jadpo check project=p71c2-side file=shared/providers/postcodes/v3/types/written_postcode.jadpo hidden
/// A postcode as someone typed it.
public type WrittenPostcode = Text {
    max_length: 20
}
```

```jadpo check project=p71c2-side file=shared/providers/postcodes/v3/failures/invalid_postcode.jadpo hidden
/// The text isn't a UK postcode.
public type InvalidPostcode = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=p71c2-side file=shared/providers/postcodes/v3/libraries/uk.jadpo
/// Reading UK postcodes.
public library uk {
    /// Reads a UK postcode as it is written, spaces around it left out.
    function standard_form(postcode_text: WrittenPostcode) fails InvalidPostcode -> UkPostcode {
        return attempt UkPostcode(text.upper(text.trim(postcode_text))) {
            invalid: InvalidPostcode
        }
    }
}
```

```jadpo check project=p71c2-side file=domains/addresses/actions/read_postcode.jadpo
use shared/providers/postcodes

/// Reads a customer's postcode, as callers did before the upgrade.
action read_postcode(
    written: Postcodes.v2.WrittenPostcode
) fails Postcodes.v2.InvalidPostcode -> Postcodes.v2.UkPostcode {
    return attempt Postcodes.v2.uk.standard_form(written)
}
```

```jadpo check project=p71c2-side file=domains/addresses/actions/read_typed_postcode.jadpo
use shared/providers/postcodes

/// Reads a customer's postcode with the next version.
action read_typed_postcode(
    written: Postcodes.v3.WrittenPostcode
) fails Postcodes.v3.InvalidPostcode -> Postcodes.v3.UkPostcode {
    return attempt Postcodes.v3.uk.standard_form(written)
}
```

A name without its version while two are held is
`package.version_needed`. Once the old version goes, a name that still
carries the version is `package.version_unneeded`, whose suggested edit
drops it. Both are listed under "Package errors".

While two versions of a library package that carries translations are
held, the app words each under a key that carries the version,
`postcodes_v3: { }`, as its names do
([Jadpo's and packages' text](10-configuration-and-time.md#jadpos-and-packages-text)).

### Names

- **Provider brand names are reserved.** Jadpo ships the list of curated
  provider names, and an app's own integration cannot take one, so `Stripe`
  in code always means the official package.
- **Two packages never share a name in code.** Installing a registry package
  whose name one of the app's own integrations or libraries already uses
  stops and asks for the app's own to be renamed first (`jadpo rename`).
- **A package's name can't clash with any other name.** Domain, library
  and standard-library names are unique among themselves, and a package's
  name joins them. Installing a package refuses one whose name the app
  already uses for a domain, a library or any declaration.
- **Registry names are scoped by publisher**: a package's identity in the
  registry is `@publisher/name`, such as `@acme/postcodes`, and Jadpo's
  official packages are `@jadpo/…`. In code a package keeps its own name
  and version label (`Postcodes.v2`), so two publishers' packages with one
  code name can't be installed in the same app. `jadpo package add` takes
  the scoped name, and the lock's `registry` source records it
  ([The registry protocol](#the-registry-protocol)).
- Creating an integration checks the registry and warns when the name is
  taken.
- A package is named in UpperCamelCase with its own version label, such
  as `Stripe.v2024_06_20` or `Postcodes.v2`. Its folder is the name in
  lower_snake_case with the version below it.

## Where packages come from
<!-- topic: where-packages-come-from -->

`jadpo package add <name>` looks for a package in a fixed order and
records which step it used, so an agent never improvises where to look:

1. **The registry**, the official package first.
2. **The provider's published specification** (OpenAPI, Smithy, GraphQL),
   given a URL or found in its known places, imported once.
3. **An importer** from the official SDK's type definitions, a Postman
   collection or a HAR capture.
4. **Written from the provider's documentation** by an agent.

It also takes a git repository (`jadpo package add
github.com/acme/acme-pricing@v2.1.0`, below) and another Jadpo app's
OpenAPI document (`jadpo package add ../orders`, see
[Packages from other Jadpo apps](#packages-from-other-jadpo-apps)). Each
source is read once, when the package is added or updated. Then it is
pinned in `jadpo.lock` with its source and digest.

### The registry

The registry at jadpo.dev holds public Jadpo packages of both kinds, as
Jadpo source with no native code and no install script; nothing in a
package runs when it is installed, and its code runs only when started
([What package code may do](#what-package-code-may-do)). A malicious
package could still name the wrong host or mark an unsafe call
idempotent, so hosts, services and every package change appear in the
approval report, the summary a person signs off before a release. How it
works is in [The public registry](#the-public-registry).

A few curated packages, Stripe, Twilio and GitHub, imported from their
published OpenAPI, live in the Jadpo repository until then.

### An importer

`jadpo package import <format> <file>` turns a published description into
the app's own integration once, offline, in
`shared/providers/<name>/<version>/`:

| Format | Typical source | What it gives |
|---|---|---|
| `openapi` | Most REST APIs (Stripe, Twilio, GitHub), and every Jadpo API | Operations, types, error responses; from a Jadpo app, exact (its `x-jadpo` fields) |
| `smithy` | AWS, one model per service | The same, plus AWS's protocols and signing |
| `postman` | A provider's published Postman collection | Operations and example bodies; types inferred from examples |
| `har` | A browser or proxy capture of real traffic | Operations and real responses; values stripped, shapes kept |
| `dts` | The provider's official TypeScript SDK (`.d.ts` files) | Exact types and SDK method names; no HTTP method or path, so the agent adds those from the docs |

`openapi` reads a local file: `jadpo package import openapi
../orders/build/openapi/main.json`, with `--as Orders.v1` to name the
package. "How the OpenAPI importer reads a document", further down this
page, says how it reads one.

An agent then fills in what the source leaves out: which calls change
something, idempotency, error names, and decoding (OpenAPI often says only
`integer` for Unix seconds). A person reviews that diff. Corrections to a
wrong specification live in the package and survive the next import,
because the importer merges rather than overwrites, as an
[OpenAPI Overlay](https://spec.openapis.org/overlay/v1.0.0) does.

### Written by hand, when there is no specification

Many APIs publish only HTML docs. A package is Jadpo source, so for these
an agent writes the package files directly from the docs, in exactly the
format an importer produces. Nothing else changes: the same compiler
checks, the same fakes, the same review. A package needs only the
operations the app uses, so it can start with one operation and grow.

### Mapping imported types to standard types

An imported description says little about what a field *is*: OpenAPI's
`integer` may be pence, Unix seconds or a count. A package's public types
use the standard types, such as `Money` and `Instant`, where one fits
(`type.standard_exists`, [Standard library](19-standard-library.md)). So
an import maps fields to them in six layers, and nothing is guessed
silently:

1. **Exact mappings.** A description's own format hints map without
   judgement: OpenAPI's `date-time` to `Instant`, `date` to `CalendarDate`,
   `email` to `Email`, `uri` to `Url`, `uuid` to `Uuid`, `ipv4` and `ipv6` to
   `IpAddress`, ISO 4217 and ISO 3166 enumerations to `Currency` and
   `Country`.
2. **Shape and name detection,** with the same matcher as
   `type.standard_exists`: `amount` with a sibling `currency` to `Money`,
   `lat`/`lng` to `Point`, an integer named like a time (`created`,
   `*_at`) as a candidate `Instant` from seconds. The provider's own shape
   becomes a named decoding at the edge (Stripe's `amount: 1050, currency:
   "gbp"` decodes to `Money`, with a field's `wire_parts:` under
   "Decoding"), so the app only ever sees the standard type.
3. **Findings on what was flagged.** Every candidate the importer is not
   certain of is a declinable error on that field,
   `package.standard_candidate`, whose `next:` says what
   to check, marked `(agent)`: confirm against the docs and swap, or decline
   with a reason that `jadpo approval` shows.
4. **One scoped review of the whole package.** The import ends with one
   broad instruction to the agent: go through every imported type the app
   reaches against the provider's documentation, use the standard type
   wherever one fits, add a decoding wherever a format is undocumented
   (seconds, cents, comma-separated lists), and mark which operations change
   something and which are idempotent. The review covers the operations the
   app calls and every type they reach, never the whole description
   (Stripe's has 1,180 types), and grows when the app calls something new.
   Each imported type starts unreviewed; the agent marks it reviewed when it
   has gone through it; `jadpo check` lists reviewed and outstanding types
   for the operations the app calls, and `jadpo approval` shows a person
   any type shipped unreviewed.
5. **Confirmation by real responses.** An operation the app calls stays
   unconfirmed until `jadpo package record` has checked a sandbox response
   against it ("Provenance and confirmation", below).
6. **The safety net.** The package is Jadpo source, so
   `type.standard_exists` runs on it as on any code: a field that looks like
   a standard type and was not mapped blocks `check` until it is swapped or
   declined.

The import's own output lists what it mapped, the findings, the scoped
review instruction and the operations still unconfirmed, with `--json` for
agents. Corrections made in review survive the next import, because the
importer merges rather than overwrites.

> The agent marks a type reviewed by listing it in
> `provider.jadpo`'s `imported:`, `reviewed: [Charge, Shop]`, each one of
> the package's own types, once (`package.provider_invalid`). The review's
> scope is worked out from the calls on every check, so it grows when the
> app calls something new: the package's types each operation the app
> calls takes and gives reach, through fields, list items, payload
> variants and refinements. `jadpo check`, once it passes, lists the types
> still to review and the calls no recording confirms, counts both in its
> footer, and gives every call and type in `--json` as `providers`;
> neither stops it. With nothing left a passing check is one line; with
> something left it reads:
>
> ```text
> $ jadpo check
> To review
>   Go through each type against the provider's documentation, then add it to
>   `reviewed:` in `imported:` in its package's `provider.jadpo` (agent)
>     Tidepay.v2024_06_20  Charge, ChargesGetChargesInput, Shop; 0 of 3 reviewed
>
> Not yet confirmed
>   Record each with `jadpo package record`, which checks a sandbox answer
>   against its types (agent)
>     payments.charges.get_charges
>
> ✓ checked  3 to review · 1 unconfirmed · 9 files
> ```
>
> It writes the same to `build/report/providers.md` and `.json`, for
> `jadpo approval` to show a person what ships unreviewed or unconfirmed.
> The import ends with the scoped instruction: the types
> the app's calls reach still to review, or, while it calls none of the
> package's operations, when the review starts; and the calls not yet
> confirmed. Importing again keeps `reviewed:`, less any type the
> description added to, which the summary lists under "To review again".
> The choices are in "The scoped review and unconfirmed operations"
> below.

### Provenance and confirmation

Every operation says where it came from, so a reviewer knows how far to
trust it:

| `source:` | Meaning | Also records |
|---|---|---|
| `PackageSource.imported` | From a published description (OpenAPI, Smithy, Postman, `.d.ts`, GraphQL introspection later) | `provider.jadpo`'s `imported:` names the file and its digest |
| `PackageSource.written` | Written from the provider's docs | `docs:` with the page's URL |

An operation is **confirmed** when a real response has been checked against
it. `jadpo package record <connection> <operation>` calls the provider's
sandbox once, then checks the operation's output type against the response,
or writes the type if there is none yet. It saves the response, with values
replaced by placeholders, as `recordings/<resource>.<operation>.json` in the
package; every decoder must decode every recorded response, on every build
of the app's own integration and at publish. An operation imported from a
HAR capture is confirmed from the start. `record` refuses a connection whose
environment is live.

Written operations that are not confirmed are allowed in every build. The
approval report flags them, and CI requires a passing contract run against
the sandbox for each one (see "Tests and fakes").

> `jadpo package record <connection>
> <resource>.<operation>` calls the sandbox once through the connection,
> with its credential from `.env.local` and the operation's input, written
> as a route's body is, from `--input`. It checks the answer against the
> operation's output type and keeps it as the recording: along
> `output_at:`, only the members the type declares, each plain text or
> number replaced by a placeholder naming its built-in type (`"{Text}"`),
> and what the package's own encodings and decoders read kept as sent.
> The surface then marks the operation `confirmed`, and `jadpo test`
> decodes the recording ("Recording errors"):
>
> ```text
> $ jadpo package record couriers bookings.create --input '{"postcode": "N1 9GU"}'
> ✓ Recorded `couriers.bookings.create` from the sandbox
>     sent    POST https://sandbox.acme-couriers.example/v1/bookings
>     answer  decodes as Booking
>     values  2 placeholders, 2 kept as sent
>     file    shared/providers/acme_couriers/v1/recordings/bookings.create.json
>
> ✓ operation recorded  AcmeCouriers.v1 bookings.create · confirmed
>
> → jadpo test  decodes the recording with every decoder
> ```
>
> ```json
> {
>   "data": {
>     "collectAt": "{Instant}",
>     "id": "{Text}",
>     "status": "booked",
>     "window": "PM"
>   }
> }
> ```
>
> It refuses before anything is sent a connection whose environment is
> live, or whose credential nothing sets (`recording.refused`), and keeps
> nothing from an answer that doesn't decode or a refusal
> (`recording.failed`):
>
> ```text
> $ jadpo package record couriers bookings.create --input '{"postcode": "N1 9GU"}'
> error: The sandbox's answer to `couriers.bookings.create` doesn't decode as `Booking`
>   reason: Reading it found `variant` on `status`: the operation's types and the provider disagree, so a real call would fault. Nothing was recorded.
>   next: Correct `Booking` against the answer and the provider's documentation, then run it again (agent)
>   rule: recording.failed
>   help: https://jadpo.dev/docs/errors/recording.failed
> ```
>
> The choices are in "Recordings" below.

### Installing safely

- **Every package is verified at install, wherever it came from**:
  `jadpo package add` and `update` regenerate the surface from the
  package's code and refuse one that differs from its committed
  `surface.jadpo_record`.
- **The lock** records each package's version, digest and source:
  `registry`, `git` (the repository, the tag and the tag's commit), `path`
  (a folder in the same repository, such as another Jadpo app) or `link`
  (a live checkout during development, below). A public package can never
  stand in for a git one. `jadpo check` and `jadpo build` read the lock and
  the committed folders only.
- **Publisher trust.** A third party's package from git, rather than the
  registry, is unverified, and needs a person's acceptance in the change
  that installs it: `AcmeCouriers.v1: PackageChange.git_source`
  ([Acceptances](18-records-and-manifest.md#acceptances)). Registry
  packages carry their verified publisher, and curated ones are marked
  official.
- **Your own company's repositories** are listed in `application { }`'s
  `packages:` section, each a host and path ending in `/`:

  ```text
  packages: {
      own_sources: ["github.com/acme/", "git.acme.example/platform/"]
  }
  ```

  A package locked from a repository under one of them needs no
  `PackageChange.git_source`; everything else about it is reviewed as for
  any package. Adding an entry, or widening one, is a decision:
  `"github.com/acme/": PolicyChange.own_source_added`, so an agent can't
  let a stranger's repository in. Removing one needs nothing. The list is
  kept in `access.jadpo_record` as `own_source` lines, which gives the
  check its before side (Richard, 2026-10-31).
- **Prose is unverified wherever it comes from**, such as `///`
  descriptions and `GUIDE.md`. Nothing safety-relevant relies on it.

> `jadpo package add` from a folder (`../postcodes`)
> or from a git repository at a release tag
> (`github.com/acme/postcodes@v2.1.0`): the package is compiled as the
> app's own integration would be, the surface its code gives has to be its
> committed `surface.jadpo_record` byte for byte (`install.surface_mismatch`),
> and only then is it copied into `packages/<name>/<version>/` and locked
> with its version, digest and source. `update` moves one to its newest
> release and shows what changed for review
> ([Records and manifest](18-records-and-manifest.md)). `remove` takes out
> one the app no longer uses (`install.in_use`), and `link` and `unlink`
> are under [Developing and publishing a package](#developing-and-publishing-a-package).
> The choices are in "How packages are installed and linked" below.
>
> ```text
> $ jadpo package add github.com/acme/postcodes@v2.1.0
> ✓ Added Postcodes.v2 2.1.0
>     folder   packages/postcodes/v2
>     source   git github.com/acme/postcodes tag v2.1.0
>     surface  matches its code
>
> ✓ package added  Postcodes.v2 2.1.0 · 1 function · 2 types · 1 failure
>
> → jadpo package show postcodes  shows what it offers
> ```
>
> ```text
> $ jadpo package update postcodes
> ✓ Updated Postcodes.v2 from 2.0.0 to 2.1.0
>     folder   packages/postcodes/v2
>     source   git github.com/acme/postcodes tag v2.1.0
>     surface  matches its code
>
>   Changes since 2.0.0
>     + type OutwardCode
>
> ✓ package updated  Postcodes.v2 2.1.0 · 1 added
>
> → jadpo check  checks the app against 2.1.0
> ```

Not built yet: the hosted registry; importers other than `openapi`;
adding another Jadpo app's OpenAPI document with `jadpo package add`;
`packages:`'s `own_sources:`; writing an operation's output type from
the sandbox's answer when it has none; and a HAR capture's operations.

## The surface
<!-- topic: the-surface -->

A package's surface is everything it offers an app, in one file you can
read. To see a package's surface, run `jadpo package show <name>`. The
compiler works it out from the package's code and writes it to
`surface.jadpo_record`, beside `package.jadpo`. It lists:

- the package's description, licence and version;
- every public type with its fields and constraints, and every failure
  with its kind and code;
- every public library's functions, with their signatures, failures and
  descriptions;
- every recipe;
- for a provider: its operations (wire and output types both); each
  public action with the operations it calls on each path, which of them
  change something and which failures can follow an effect; its hosts or
  container, credential scheme, settings and polls; and the capabilities
  it provides.

### Read as a diff

The surface is plain text in the format of
[the public API record](07-http.md#the-public-api-record), not JSON. Like
every `.jadpo_record` file, it starts with a line saying what wrote it
([Records and manifest](18-records-and-manifest.md)). Installed packages
are committed, so every upgrade's pull request carries the surface diff,
and a reviewer reads "removed `bookings.cancel`, new failure
`PostcodeNotServed`" in plain text.

Every entry points at its file, such as `types/booking.jadpo`, so an agent
reads the surface first and opens only what it needs. A file is named
after what it declares, so the pointer never goes stale.

### Generated, never written by hand

`jadpo package check` writes the surface. `add`, `update` and the registry
refuse a surface that doesn't match the code. The surface of an app's own
integration is generated into `build/`.

The surface is kept out of `package.jadpo`, which a person writes. The
descriptions stay in the source as `///` comments, where the author edits
them.

### `jadpo package show`

`jadpo package show <name>` prints the description, licence, version,
kind and public surface. `jadpo package show <name> <declaration>` prints
one operation, type or function with its description and file. `--guide`
prints `GUIDE.md`, and `--json` gives tools and agents the same. It works
for installed packages and the app's own integrations alike.

### It decides the version

Publishing compares the new surface with the last release's, and refuses
a breaking change without a new major version. A breaking change is a
public name removed, a field retyped or a new failure. The upgrade plan
diffs the same two files.

> The surface is `surface.jadpo_record` (`record
> surface 1`): the package's entry (description, file, kind, version,
> licence, Jadpo version, guide), a provider's `provider` entry (protocol,
> hosts or container, credential scheme, idempotency key), each public
> resource's operations, each public library's functions, each public
> type and failure, and each recipe, every entry with its file. `jadpo
> package show` prints it, one declaration in full or the guide, for
> installed packages and the app's own integrations alike, and `jadpo
> check` writes each own integration's surface to
> `build/providers/<name>/<version>/surface.jadpo_record`. The
> `postcodes` library package's, as `jadpo package check` writes it:
>
> ```text
> // Generated by Jadpo from this package's code; update it with `jadpo package check`, never by hand.
> record surface 1
>
> package Postcodes.v2
>     /// UK postcodes: reading them as people write them.
>     file package.jadpo
>     kind library
>     version 2.0.0
>     licence mit
>     jadpo 1.0
>
> function uk.tidy
>     /// Reads a postcode in upper case.
>     file libraries/uk.jadpo
>     signature (entered: EnteredPostcode) -> UkPostcode
>     fails InvalidPostcode
>
> type EnteredPostcode = Text
>     /// A postcode as someone typed it.
>     file types/entered_postcode.jadpo
>     max_length: 20
>
> type UkPostcode = Text
>     /// A UK postcode in standard form, such as "SW1A 1AA".
>     file types/uk_postcode.jadpo
>     max_length: 8
>     min_length: 5
>
> failure InvalidPostcode
>     /// The text isn't a UK postcode.
>     file failures/invalid_postcode.jadpo
>     kind invalid_value
>     code invalid_postcode
> ```
>
> and `jadpo package show` in an app that installed it:
>
> ```text
> $ jadpo package show postcodes
> UK postcodes: reading them as people write them.
>   package   Postcodes.v2
>   kind      library
>   version   2.1.0
>   source    path ../postcodes
>   licence   mit
>   jadpo     1.0
>   folder    packages/postcodes/v2
>
> Functions
>   uk.tidy          (entered: EnteredPostcode) fails InvalidPostcode -> UkPost…
>
> Types
>   EnteredPostcode  Text { max_length: 20 }
>   UkPostcode       Text { min_length: 5, max_length: 8 }
>
> Failures
>   InvalidPostcode  invalid_value
>
> ✓ Postcodes.v2  1 function · 2 types · 1 failure
>
> → jadpo package show postcodes uk.tidy  shows one in full, with its file
> ```
>
> `add` and `update` verify the surface at install (under "Installing
> safely"); the choices are in "How the surface and the publishing bar
> work" below.

## Developing and publishing a package
<!-- topic: developing-and-publishing-a-package -->

A package lives in its own repository. To start one, run
`jadpo package new <Name> --library` or `--provider`. It writes the
package format, plus `recipes/`, a `sample/` app that the recipes and
tests run against, and `GUIDE.md`.

### Developing it

- `jadpo test` runs the package's tests, the examples in its
  descriptions, its decoders against its recordings, and its recipes
  compiled into `sample/`.
- `jadpo package record` and `conform` work against the provider's
  sandbox.
- `jadpo package check` runs the whole publishing bar locally, writes
  `surface.jadpo_record`, and compares it with the surface at the previous
  tag.

### Trying it in an app before publishing

`jadpo package link ../acme-pricing` compiles the app against the live
checkout and records the link in the lock. `jadpo build` for production,
and CI, refuse a link, so it can never ship. `jadpo package unlink`
restores the installed version.

### Private packages come from git

`jadpo package add github.com/acme/acme-pricing@v2.1.0` installs a
private package. A version is a git tag, which the package's CI adds only
after `jadpo package check` passes. Access is the developer's or CI's own
git access. There is no `jadpo login` and no private registry: packages
are committed into apps, so a registry's caching would add nothing.

### Public packages

Public packages go to the registry with `jadpo package publish`, from a
tag ([Releasing](#releasing)).

> `jadpo package link <folder>` writes `package
> Postcodes.v2 link ../postcodes` in the lock and keeps the line it
> replaced in `.jadpo/links`; from then on every `jadpo check`, `test` and
> `dev` reads the checkout's files in place of `packages/postcodes/v2/`,
> which stays as installed, and shows its problems at the checkout's paths.
> With `CI` set, `jadpo check` and `jadpo test` refuse the link
> (`install.link_refused`), and `jadpo build` will always. `jadpo package
> unlink postcodes` puts the installed line back:
>
> ```text
> $ jadpo package link ../postcodes
> ✓ Linked Postcodes.v2 to ../postcodes
>     checkout   ../postcodes
>     installed  2.1.0, restored by jadpo package unlink postcodes
>
> ✓ package linked  Postcodes.v2 · the app checks against it
>
> → jadpo package unlink postcodes  goes back to the installed version before you push
> ```

### What publishing requires

`jadpo package publish`, and `jadpo package check` before a tag, refuse a
package unless:

1. it compiles with no errors and no warnings;
2. every public type, failure, library and library function has a `///`
   description;
3. every function of a public library has at least one example in its
   description,
   compiled and run at publish, and every failure it can raise is shown by
   an example or a test;
4. its tests pass, every decoder decodes every recorded response, a
   field's decode and encode pair round-trips, and every recipe compiles
   and passes its tests in `sample/`;
5. its version agrees with the surface diff;
6. its changelog's list of changes is generated from the surface diff, and
   the author writes only why;
7. it declares a licence from the fixed list (not `.proprietary`, for the
   public registry) and the lowest Jadpo version it needs.

The bar applies only to what is published: the app's own integrations and
libraries skip it. Examples are part of the bar because they must run,
which the compiler can check.

A library package also ships complete wording in its own default locale,
with a `///` description on every public message
([Configuration and time](10-configuration-and-time.md)). The bar's
`wording` row, shown when the package carries translations, holds every
language the package ships to completeness, and item 2 asks for the
description of every public message.

> `jadpo package check`, run in a package's own
> repository, checks the package placed as the app's own integration in a
> scratch app (the golden rule) and shows each problem at its place in the
> package's folder. Item 1 is the check itself, `type.standard_exists` on
> the package's public types among it
> ([Standard library](19-standard-library.md)); item 2 is
> `publish.description_missing`; item 4's tests are the package's own
> `.test.jadpo` files; item 5 works out the version from the surface at
> the last tag and refuses a breaking change (`publish.breaking_change`);
> item 7 is the index's own check. Item 4's decoders are the `decoders`
> row, every decoder over every recording with each field's `decode:` and
> `encode:` round-tripping ("Recording errors"), and its recipes the
> `recipes` row (under "Recipes"). The surface is written whenever the
> package compiles and its version is allowed.
>
> Item 3: each ```` ```jadpo ```` block in a
> public declaration's `///` description runs as a test of the package,
> a function of a public library without one is
> `publish.example_missing`, as is a failure it can raise that no example
> or test asserts with `fails`, and an example that doesn't compile or hold
> is `publish.example_failed` at its line in the description. Item 6: the
> new version's section of `CHANGELOG.md` is written from the surface's
> changes, and a release whose section says nothing beneath the list is
> `publish.changelog_missing`:
>
> ```text
> $ jadpo package check
> Publishing bar
> ✓ compiles      passed                        no errors
> ✓ described     passed                        every public declaration
> ✓ examples      passed                        2 examples, every failure shown
> ✓ tests         passed                        3 tests
> ✓ version       passed                        2.1.0, 1 added since 2.0.0
> ✓ changelog     passed                        2.1.0 says why
> ✓ licence       passed                        mit, from Jadpo 1.0
>
> ✓ Wrote surface.jadpo_record and CHANGELOG.md
>
> ✓ package check passed  Postcodes.v2 2.1.0 · 7 passed
>
> → git tag v2.1.0  tags the release once surface.jadpo_record is committed
> ```
>
> An example is a test's body in the description; the changelog's list is
> the first list under its version, and the author writes why beneath it:
>
> ```text
> /// Reads a postcode in upper case.
> ///
> /// ```jadpo
> /// var postcode: UkPostcode = attempt uk.tidy("sw1a 1aa")
> /// assert postcode == "SW1A 1AA"
> /// assert uk.tidy("x") fails InvalidPostcode
> /// ```
> function tidy(entered: EnteredPostcode) fails InvalidPostcode -> UkPostcode {
> ```
>
> ```text
> # Changelog
>
> ## 2.1.0
>
> - added type `OutwardCode`
>
> Reads the outward code, for delivery zones.
>
> ## 2.0.0
>
> - first release
>
> UK postcodes, read as people type them.
> ```

## The public registry
<!-- topic: the-public-registry -->

The registry at jadpo.dev holds public Jadpo packages of both kinds. It is
itself a Jadpo application, deployed to Cloudflare Workers, and calls one
service that isn't Jadpo code: the verifier, the `jadpo` tool in a
container, which compiles and checks what is published. Private packages
still come from git ([Private packages come from git](#private-packages-come-from-git)).

Four things the language already does make most attacks on package
registries impossible here: nothing in a package runs at install or build,
packages have no dependencies, an app commits the exact source it runs, and
every host a package reaches is declared. The rules below cover what is
left: a hijacked or hostile publisher releasing code that returns wrong
values or sends data to a host it may already reach.

### Names and publishers

- **A package's registry name is `@publisher/name`**, such as
  `@acme/postcodes`; Jadpo's official packages are `@jadpo/…`. In code it
  keeps its own name and version label (`Postcodes.v2`). `package.jadpo`
  names its publisher, so a fork can't be published as the original.
- **A name is never reused**, after a yank, a takedown or a publisher
  leaving. A name close to a popular one under another publisher, or to a
  reserved brand, needs Jadpo's approval.
- **A publisher is a person or an organisation.** An organisation's owners
  manage its members and packages; its releasers confirm releases; other
  members see pending releases. An organisation with two owners needs both
  owners' passkeys to change its owners.
- **Accounts sign in by magic link or passkey**, and later with Google or
  GitHub. Anything that publishes, confirms, cancels, yanks, marks a
  release urgent or changes members needs a passkey; an email address
  alone never grants it. Each person can keep several passkeys.
- **A package with one releaser says so** on its page.

### Releasing

A release is made from a tag in any public git repository reached over
HTTPS. The registry never accepts an upload, and no publish token exists.

1. `jadpo package publish` (or the jadpo.dev page) asks the registry to
   publish a repository at a tag. It sends only those two, and needs no
   sign-in, because nothing goes live without a releaser.
2. The verifier resolves the tag to its commit, fetches that commit, and
   then, with no network, checks it: only Jadpo source and the package's
   text files (`package.jadpo`, `provider.jadpo`, `GUIDE.md`,
   `CHANGELOG.md`, recordings and translations), within the size limit,
   and the whole publishing bar. Everything the registry shows about the
   release is worked out from that source, never from anything the
   publisher sends.
3. **The release is pending until a releaser confirms it with a passkey**,
   on a page showing its version, surface diff, changelog and facts.
4. It goes live, its digest is added to the public log, and its pages are
   written.

- **Routine releases may release themselves.** A package can opt into
  automatic release 24 hours after a release goes pending, unless a member
  cancels it; every member is told when it goes pending. Only a release
  that isn't a new major and adds no host, no new place personal data is
  sent and no new call that changes something qualifies. Any other release
  always needs the passkey.
- **Any releaser can confirm**, with their own passkey.
- **A new releaser waits seven days** before they can confirm a release,
  and every member is told they were added. Removing a releaser takes
  effect at once.
- **A tag that moves is refused.** A tag that later points at another
  commit is reported to the publisher, and the registry keeps the source
  of the commit that was confirmed.
- **Succession is named in advance.** An owner may name a successor, who
  can claim the package once the owner hasn't signed in for six months,
  after the owner is emailed and has 30 days to refuse. A package moving to
  another publisher is shown in every app's next update and restarts the
  three-day wait below. Without a named successor Jadpo never hands a
  package to anyone: it can be marked unmaintained, and a fork is
  published under its own name.

### What a release shows

The verifier works out a release's **facts** from the compiled graph:

| Fact | From |
|---|---|
| Kind, and whether it is pure | The package's folders and what its code does |
| Each host, and the operations that reach it | `provider.jadpo` and each operation |
| Which calls change something at the provider | Each operation |
| Which personal data reaches which host | `personal:` marking followed to each operation that sends it ([Personal data](15-personal-data.md)) |
| Whether it declares an `erasure:` call | `provider.jadpo` ([Erasure at providers](15-personal-data.md#erasure-at-providers)) |
| Types an app may store | Its library types |
| Failures, per operation and function | The surface |
| Confirmed operations, and the last passing contract run | Recordings and scheduled runs |
| The Jadpo releases it compiles and passes its tests with | The compatibility runs |
| Standard types it could use and doesn't | `package.standard_candidate` |

- **Facts and checks, never one score.** Each line is a fact or a check that
  passes or fails. Downloads and stars aren't shown in ranking, and there
  are no ratings or reviews.
- **A written account of the package**, by a model, from its source,
  descriptions and examples: what it is for, and each public function in
  plain words with the ideas it involves. It is labelled as written by a
  model, and is checked against the surface and the facts: an account that
  names a declaration the package doesn't have, or an effect the facts
  don't show, is written again. A model also reads each release's source
  diff for what the facts can't show, such as a new literal that looks
  like an account number, and flags it to Jadpo; it never decides
  anything.
- **The same facts everywhere**: `jadpo package check` prints them while
  the package is written, and the pending release, the package's page,
  `jadpo package show` and `--json` show the same ones.
- **Reports, not reviews.** A security report goes privately to the
  publisher and Jadpo; suspected malware goes to Jadpo. An app may opt in
  to telling the registry when its scheduled contract run gets a
  response a package can no longer decode, sending the shape that failed
  and never the values. Issues stay in the package's repository.

### The changelog

`CHANGELOG.md` is written from the release's **change record**. Each entry
names a declaration and what happened to it: added, removed, retyped, a new
failure, a new host, personal data sent somewhere new, or its behaviour
changed. The author writes only why.

- **Behaviour the types don't show.** The verifier runs the previous
  release's examples and tests against the new code. Each one that now
  fails is a "behaviour changed" entry. A minor or patch release with one
  is refused unless the author marks it a fix and says why.

### Search

People and agents describe what they need in words, usually without
knowing its types, so search reads text first.

- **What is searched**: each package's written account, its descriptions
  and examples, and each public function's own account, by words (for
  exact terms such as "H3") and by meaning (embeddings, with `ai.embed`
  and a `Vector` field, [AI](21-ai.md)). A result names the function that
  answers the query.
- **The standard library is in the same index and comes first**, so an
  agent asking for what Jadpo has built in is told so.
- **Shape, when the asker knows it.** Each public function also has a
  fingerprint of its structure: inputs and outputs reduced to standard
  types and their constraints, its failures and its effects. `--from` and
  `--to` filter by it, reading the shapes from the app; the tool sends
  shapes, never source.
- Results are ranked by fit, then by the facts.

### Installing and updating from the registry

The rules in [Installing safely](#installing-safely) all apply.

- **`jadpo package add @acme/postcodes` installs that exact name or
  stops.** It never picks a near match, and it shows the publisher,
  whether they are verified, the package's age and its facts.
- **Every digest is checked against the public log**, so the registry
  can't give one app different code from another.
- **Releases are available as soon as they are live.** `jadpo package
  update <name>` with no version takes the newest release at least three
  days old, and names the newer ones it skipped. `--to <version>` takes
  that release at once, after showing its facts.
- **Urgent releases skip the wait.** A releaser marks a release urgent, with
  a reason and a passkey, even under automatic release. The registry
  publishes an advisory against the versions it fixes, and `update` takes
  it at once and says why. The urgent mark is shown on the release.
- **Before an update** the review ([Package upgrades](18-records-and-manifest.md#package-upgrades))
  also shows a new publisher or owner since the installed version, and the
  change record.
- **An app can always fix a package itself**: an installed package is in
  the format of the app's own integrations, so it can be copied into
  `shared/providers/` and changed there.

### Yanks, takedowns and advisories

- **A release is never changed or deleted by its publisher.** A releaser
  may yank one: new installs refuse it, and apps that have it keep
  working. Jadpo removes a release only for malware or a legal order,
  leaving a record of why.
- **An advisory names declarations and versions** (`Postcodes.uk.parse`,
  2.0.0 to 2.1.2), not just a package. `jadpo package audit` lists the
  advisories against the app's packages and says whether the app reaches
  each affected declaration. It is the one check that goes online; `jadpo
  check` and `build` never do. Run it in CI.
- **Copies are tracked.** Packages copy code instead of depending on each
  other, so the verifier fingerprints every public declaration's
  normalised source. An advisory also names the packages carrying a copy,
  and their publishers are told.

### Compatibility with Jadpo releases

Every package is compiled and tested against each Jadpo pre-release, so a
release that would break the ecosystem is caught before it ships, and again
when it ships. A package's page lists the Jadpo releases it works with,
with nothing for its author to keep up. The lowest Jadpo version its
`package.jadpo` declares is checked against that list. When a release
breaks a package, its publisher is told the errors and what `jadpo fix`
would change.

### Developing for the registry

Publishing should hold no surprises:

- `jadpo package check` prints the facts, the version the surface diff
  gives and the change record, exactly as the registry will show them.
- `jadpo package preview` serves the package's registry page locally.
- `jadpo package publish --dry-run` runs the verifier on the pushed tag's
  commit and publishes nothing.
- The verifier checks with the Jadpo release the package declares and the
  newest, so a local pass with either is a pass at the registry.

### The registry protocol

The protocol is what the `jadpo` tool and the registry application are each
built against. It is served at `https://jadpo.dev/registry/v1/` as JSON,
with failures in the [failure envelope](07-http.md), and is versioned by
its path. Reads need no sign-in. The `jadpo` tool never signs in: nothing
it can send makes a release live.

| Request | Answers |
|---|---|
| `GET packages/@{publisher}/{name}` | The package: description, kind, code name and label, publisher (verified, official), maintained or its successor, and each release's version, digest, time, and whether it is yanked or urgent |
| `GET packages/@{publisher}/{name}/releases/{version}` | One release: repository, tag, commit, digest, its `surface.jadpo_record`, facts, change record, written account, compatible Jadpo releases and its index in the log |
| `GET archives/{digest}` | The release's files as a tar archive whose files, unpacked, have that digest, as the lock computes it |
| `GET log/head` | The log's size and root hash |
| `GET log/proof/{digest}?size=` | The release's log entry, its index and the hashes that prove it is in the log of that size |
| `GET log/consistency?from=&to=` | The hashes that prove the log of size `to` extends the log of size `from` |
| `GET search?q=&from=&to=&kind=` | Ranked results, the standard library's first, each naming a package and declaration, with why it matched |
| `POST advisories/query` | Given installed packages and versions, the advisories against them, each naming its declarations, versions and the packages carrying copies |
| `POST releases` | Given a repository, a tag and `dry_run`, starts a release (or a dry run) and answers its id |
| `GET releases/{id}` | Its state (`verifying`, `failed` with the bar's rows, `pending`, `cancelled`, `live`), with its facts and change record once checked |

- **The log** is an append-only Merkle tree of releases, hashed as RFC
  6962 does with SHA-256. Each leaf is one line, `@acme/postcodes 2.1.0
  sha256:…`, so a logged digest can't stand for another package's or
  version's release. `add` and `update` keep the last head they saw in
  `.jadpo/registry_head`, ask `log/consistency` from its size to the new
  head's and refuse a head the proof doesn't show extends it, then ask
  `log/proof` at the new head's size and refuse a release it doesn't show
  is in the log. Proofs are made by RFC 6962's definitions and checked by
  RFC 9162's algorithms; the root is never sent with a proof, since a
  client checks it against the head it holds.
- **A placeholder fills a whole segment**, so `{publisher}` is the handle
  with its `@`: `GET packages/@acme/postcodes` is `packages/{publisher}/{name}`
  with `publisher` `@acme`. Each part of a scoped name is lower-case
  letters and digits, starting with a letter, with single `-`s between
  them, at most 39 characters.
- **Every answer's field is always sent**, `null` where it has no value,
  as a Jadpo application writes an output; sizes and indexes are JSON
  numbers. A release's facts are the table in [What a release
  shows](#what-a-release-shows), less the Jadpo releases it works with,
  which are the release's own field since they grow after it is checked.
- **Its failures** are `package_not_found`, `release_not_found`,
  `archive_not_found` and `release_request_not_found` (404), `not_in_log`
  (404, also for a release logged after the size asked about),
  `size_past_head` (422, a size past the head or sizes the wrong way
  round) and `repository_refused` (422, not a repository reached over
  HTTPS). `POST releases` answers 202, the others 200.
- **The lock's `registry` source** records the scoped name:
  `package Postcodes.v2 version 2.1.0 registry @acme/postcodes digest
  sha256:…`.
- **The registry application's served OpenAPI is checked against this
  table** by a conformance test in the `jadpo` repository, run against a
  local registry built from the same table, so neither side drifts. Every
  row must be served below `/registry/v1/` with its placeholders, query
  members, body, status, answer and failures, open to a caller who hasn't
  signed in, and nothing else but `openapi.json`; shapes are compared
  through any `$ref`, and what a Jadpo application adds that the table
  doesn't constrain (the runtime's shared failures, `x-jadpo-` keywords,
  an `Idempotency-Key` header) is left alone. The registry application's
  CI runs it with `JADPO_REGISTRY_OPENAPI` set to its served document
  ([compiler/runtime §2.1](../compiler-runtime.md#21-workspace-and-crates)).

### The verifier

The verifier is `jadpo package verify`, the same tool authors run, in a
container image published with each Jadpo release.

- **`jadpo package verify <folder> --previous <folder> --json`** runs the
  publishing bar and writes everything a release shows: the bar's rows,
  surface, facts, change record, the fingerprints of every public function
  and declaration, the previous release's examples and tests run against
  the new code, and the Jadpo release it was checked with.
- **`--serve`** answers the registry over HTTPS with a credential the
  registry holds. Each request names a repository, a commit and the
  previous release's digest. It fetches over git with network to the git
  host only, then verifies in a process with no network at all.
- It is reached from the registry as an ordinary connection to a
  provider package, so the registry needs no feature an app doesn't have.

## Connecting in `app.jadpo`
<!-- topic: connecting-in-app-jadpo -->

A connection is how your app reaches a database, a mail server or a
provider's API. You declare each one once, in `connections:` in
`app.jadpo`. Each entry is either a built-in driver (`driver: Driver.x`)
or a package (`package: X.vN`), with the typed settings that driver or
package declares. A connection to a package also names the domains whose
actions may call it, the credential from `config.jadpo`, and which of the
package's hosts to use.

```jadpo fragment
// app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        stripe: {
            package: Stripe.v2024_06_20
            domains: [payments]
            credential: config.stripe_secret_key
            environment: config.stripe_environment
            webhooks: {
                secret: config.stripe_webhook_secret
            }
            erasure: {
                key: User.stripe_customer_id
            }
            development: {
                sandbox: [payment_intents.retrieve, customers]
            }
        }
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
            timeout: 5s
        }
        rates: {
            package: ExchangeRates.v1
            domains: [finance]
            credential: config.rates_key
            polls: {
                latest: 6h
            }
        }
        pdfs: {
            package: Gotenberg.v8
            domains: [billing]
            outbound: ["fonts.googleapis.com"]
        }
    }
}
```

- **`domains:`** lists the domains whose actions may call the connection.
  No other domain can.
- **`environment:`** is a `ProviderEnvironment` setting, `.sandbox` or
  `.live`, which picks between the package's hosts. Calls are HTTPS only,
  and redirects and proxies are always refused.
- **The credential** is a `secret: true` setting
  ([Configuration and time](10-configuration-and-time.md)). The package
  says how it is sent, and code can never read it. For a provider that
  acts for each user, the credential is an `OAuthGrant` instead.
- **`host:`** gives the address of a package with no fixed host: another
  Jadpo app, a self-hosted product, or a service already running
  elsewhere.
- **`timeout:` and `retry:`** override the default time limit and retries.
  `max_concurrent:` may lower the package's limit on calls at once.
- **`webhooks:`** gives the webhook secret and, for webhooks that need a
  reply, `replies:` (see "Receiving from outside", further down this page).
- **`polls:`** changes a poll's interval, **`outbound:`** grants a service
  the hosts its package asks for, **`erasure:`** names the field holding
  the person's key at this provider, and **`development:`** lists what
  reaches the sandbox in `jadpo dev`. Each has its own section below.
- **A package's own `settings:`** are given by name, such as
  `message_stream: "outbound"`.

Two versions of one package are two connections, used side by side while
callers move across. A library package needs no connection: any domain
brings it in with a `use` line.

Two connections: one to the app's own integration with fixed live and
sandbox hosts, and one to an integration with no fixed host, reached at
the address a setting gives and taking a setting of its own. The
`package:` line names the package by its name and version, as the lock
does, with no `use` line.

```jadpo check project=p71c-connect file=app.jadpo
application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
            timeout: 5s
        }
        ledger: {
            package: Ledger.v1
            domains: [billing]
            credential: config.ledger_token
            host: config.ledger_address
            account: "shop"
        }
    }
}
```

```jadpo check project=p71c-connect file=config.jadpo
config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment
    ledger_address: Url
    ledger_token: Text {
        secret: true
    }
}
```

```jadpo check project=p71c-connect file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API, written from its documentation.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=p71c-connect file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking and tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
    credential: CredentialScheme.bearer
}
```

```jadpo check project=p71c-connect file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
public resource bookings {
    create: {
        method: HttpMethod.post
        path: "/v1/bookings"
        input: CreateBooking
        output: Booking
        changes: true
        idempotent: true
        source: PackageSource.written
    }
}
```

```jadpo check project=p71c-connect file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Text
    slot: Text
}
```

```jadpo check project=p71c-connect file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: Text
}
```

```jadpo check project=p71c-connect file=shared/providers/ledger/v1/package.jadpo hidden
/// The shop's ledger service, run by its finance team.
package Ledger.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=p71c-connect file=shared/providers/ledger/v1/provider.jadpo
/// The ledger's API, at the address each deployment gives.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.bearer
    settings: {
        account: Text
    }
}
```

```jadpo check project=p71c-connect file=shared/providers/ledger/v1/resources/postings.jadpo hidden
public resource postings {
    create: {
        method: HttpMethod.post
        path: "/v1/postings"
        input: NewPosting
        output: Posting
        changes: true
        idempotent: false
        source: PackageSource.written
    }
}
```

```jadpo check project=p71c-connect file=shared/providers/ledger/v1/types/new_posting.jadpo hidden
/// An amount to record.
public type NewPosting = Object {
    reference: Text
    pence: Int
}
```

```jadpo check project=p71c-connect file=shared/providers/ledger/v1/types/posting.jadpo hidden
/// A recorded amount.
public type Posting = Object {
    id: Text
}
```

```jadpo check project=p71c-connect file=domains/shipping/types/parcel_reference.jadpo hidden
/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
```

```jadpo check project=p71c-connect file=domains/shipping/actions/read_reference.jadpo hidden
use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
```

```jadpo check project=p71c-connect file=domains/billing/types/invoice_code.jadpo hidden
/// The code an invoice is filed under.
type InvoiceCode = Text {
    max_length: 20
}
```

```jadpo check project=p71c-connect file=domains/billing/actions/file_invoice.jadpo hidden
use billing/types { InvoiceCode }

/// Files an invoice under its code.
action file_invoice(code: InvoiceCode) -> InvoiceCode {
    return code
}
```

A package declares its own settings in `provider.jadpo`'s `settings:`, each
a plain value (`Text`, `Int`, `Decimal`, `Bool` or `Duration`), and every
connection to it gives each one by name, written out or read from a
setting of that type (`account: config.ledger_account`).

A connection's name shares one namespace with the app's domains and
libraries, so `billing.<name>` always means one thing. A connection named
like a domain or a library is an error, reported at the connection,
because that is the name you can change in `app.jadpo` without moving any
code:

```jadpo check project=p71c2-clash file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=p71c2-clash file=domains/billing/types/invoice_code.jadpo hidden
/// The code an invoice is filed under.
type InvoiceCode = Text {
    max_length: 20
}
```

```jadpo check project=p71c2-clash file=domains/billing/actions/file_invoice.jadpo hidden
use billing/types { InvoiceCode }

/// Files an invoice under its code.
action file_invoice(code: InvoiceCode) -> InvoiceCode {
    return code
}
```

```jadpo check project=p71c2-clash file=app.jadpo expect=semantic.duplicate_declaration
application {
    connections: {
        billing: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```text output
error: Connection `billing` is named like domain `billing`
 --> app.jadpo:3:9
  |
3 |         billing: {
  |         ^^^^^^^ already a domain
  reason: Connections share one namespace with domains and libraries, so `billing.<name>` means one thing; it is already the domain `billing`.
  next: Rename the connection, such as `billing_api`, and the calls through it (agent)
  rule: semantic.duplicate_declaration
  help: https://jadpo.dev/docs/errors/semantic.duplicate_declaration

check failed: 1 error, 4 files
```

### The network report

The network report lists every host your app can reach, so a reviewer
sees a new one before it ships. `jadpo check` writes it to
`build/report/network.md`, with a `.json` twin, once the connections
read cleanly. Each row gives a host, the connection that reaches it, when
(a package's environment setting picks its live or sandbox host), what
for (a package, or a driver's capability) and the domains that use it.

A new host needs a person: `jadpo check` keeps the same hosts, by
connection, in `network.jadpo_record` at the root, and a host a
connection reaches that it didn't on the default branch, a new
connection's included, needs an acceptance,
`couriers: NetworkChange.host_added`
([Acceptances](18-records-and-manifest.md#acceptances)).

The report is built for small diffs, as
[the permissions report](09-policy.md#the-permissions-report) is. Rows are
in order of connection and host, each host is shown as written, and there
are no times or totals. So a new connection or a changed host is a
one-line change in review. For the app above:

| Connection | Host | When | For | Domains |
|---|---|---|---|---|
| `couriers` | `https://api.acme-couriers.example` | `config.couriers_environment` is `live` | `AcmeCouriers.v1` | `shipping` |
| `couriers` | `https://sandbox.acme-couriers.example` | `config.couriers_environment` is `sandbox` | `AcmeCouriers.v1` | `shipping` |
| `ledger` | the address in `config.ledger_address` | always | `Ledger.v1` | `billing` |

A database or cache connection is listed at the address in its `url:`
setting, for the domains whose entities it stores. A database's `For`
says `without TLS` when its connection says `tls: false`, and names the
setting its server's certificate is checked against when it names an
`authority:` ([PostgreSQL over TLS](05-data.md#postgresql-over-tls)). A
store of files on the disk driver reaches no host and isn't listed. A
package Jadpo runs (`runs:`) is listed as the service it runs from its
image.

Not built yet: the hosts a service run by Jadpo asks for aren't in the
report until services are built.

## Calling
<!-- topic: calling -->

Your app reaches an outside service, such as a courier's API, through a
**connection**: a named entry under `connections:` in `app.jadpo` that
says which package to use, which of your domains may use it and which
credential it sends
([Connecting in `app.jadpo`](#connecting-in-appjadpo)). A domain is one
business area of your app, in its own folder under `domains/`. The
example below declares a connection named `couriers`.

There are two kinds of package
([Kinds of package](#kinds-of-package)):

- A **provider package** describes one outside API at one version: its
  operations, grouped into resources such as `bookings`, and their types
  and failures. You call it through a connection.
- A **library package** is plain shared code with no service behind it,
  such as a postcode parser. You call it through the package's name.

### Calling a provider

You call a provider's operation as `connection.resource.operation(input)`.
`couriers.bookings.create(request)` calls the `create` operation of the
`bookings` resource, through the connection `couriers`. A file brings the
connection in with `use app { couriers }`. A provider package's public
action is called the same way, as `connection.action(input)`.

Only actions may call either. An action is code that may change things,
such as writing records or calling outside, unlike a function, which only
computes a value ([Functions and actions](03-logic.md#functions-and-actions)).

A call can fail, so you write it with `attempt` or a `match`. `attempt`
passes a failure on to whoever called your action
([Passing a failure on: `attempt`](04-failures.md#passing-a-failure-on-attempt)).
After the call you list how each of the package's failures becomes one of
your app's own, written `condition: Failure`, as in
[Turning one failure into another](04-failures.md#turning-one-failure-into-another).

This sketch books a collection for a `Shipment`, a stored record of your
app that isn't shown, and saves the booking's id on it. The complete
example under [A call, step by step](#a-call-step-by-step) compiles.

```jadpo fragment
// domains/shipping/actions/book_collection.jadpo
action book_collection(shipment: Shipment) fails CollectionSlotGone -> Shipment {
    var request: AcmeCouriers.CreateBooking = AcmeCouriers.CreateBooking {
        postcode: shipment.postcode
        slot_id: shipment.slot
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

`slot_taken` is the code of one of the failures the package declares for
the operation, under its `failures:`.

### Calling a library package

A library package's functions are called through the package's name, with
no connection, from anywhere a library may be called:

```jadpo fragment
// domains/accounts/actions/save_address.jadpo
use packages/postcodes

var postcode: Postcodes.UkPostcode = attempt Postcodes.uk.parse(input.postcode) {
    invalid_postcode: AddressInvalid
}
```

### Types

The types come from the package, and your app writes none. The checker
compares what you send with what the operation takes. At runtime a
response is checked only for the fields your code reads, so a provider
adding or changing other fields breaks nothing. A package input may take
any of your app's object types, sent as JSON, such as Postmark's template
variables or Stripe's metadata.

### Failures

Map a package's failure at the call to give it your app's meaning. A
package failure you don't map becomes a fault: a problem outside your
code that the runtime raises, which never appears in your `fails` list
([Faults](04-failures.md#faults)). To change a package failure's code, not
just its message, map it to your app's own failure at the call.

The runtime also raises failures of its own on any outside call, whatever
the package declares:

- `misconfigured` when the provider rejects the credential;
- `unavailable` when the provider surely did nothing;
- `outcome_unknown` when the provider may have acted.

A call that gets no answer in time is `timed_out`. You may map
`timed_out` to one of your own `unavailable` failures only on an
operation that is safe to send again
([Retrying a call, and timing out](#retrying-a-call-and-timing-out)).

### Idempotency keys

An idempotency key lets a provider recognise a request sent twice and act
on it once. The package says where the key goes, and the runtime supplies
it. You don't choose the key yourself.

Each call's key is built from what identifies this call. The action's key
is the client's `Idempotency-Key` header or the request's id in a route
([Idempotent writes](07-http.md#idempotent-writes)), and the delivery key in
subscribers and schedules, which names the delivery being handled
([Delivery: at least once, safe to repeat](12-jobs-and-events.md#delivery-at-least-once-safe-to-repeat)).

The runtime never sends that key as it is. It hashes it with the
application, the environment, the connection, the caller's scope (who is
calling, so one person's key never matches another's), the call's step
number and a key generation. The result is `jd1_` followed by 32 base32
characters of a SHA-256, 36 characters in all. Where the provider takes
fewer, it is cut to the package's `max_length:`.

So when two callers send the same `Idempotency-Key`, a provider that keys
requests by account never takes one for a repeat of the other.

### Time limits and retries

By default a call may take 3 seconds (`timeout:`) and is tried up to 3
times within 30 seconds (`retry: { attempts: within: }`). The wait between
tries doubles each time, with some randomness (exponential backoff with
full jitter): at most 200 milliseconds before the second try and 400
before the third.

A route has 10 seconds by default, its deadline, which the route's
`deadline:` changes ([Time limits](07-http.md#time-limits)). Three
3-second tries and their waits take less than 10 seconds, so a route
making one call needs no `deadline:` of its own. Retrying never runs past
the route's deadline: a wait that would end after it stops the retrying,
and the last answer stands. In background work, retrying stops at
`within:` or when the work's own time budget runs out, whichever comes
first.

Only safe repeats are retried: the request never left, the provider said
it did nothing (honouring `Retry-After`), or the call is idempotent.
[Retrying a call, and timing out](#retrying-a-call-and-timing-out) has
the details.

The package's rate limits space calls out before the provider refuses
them. `max_concurrent:` caps how many calls run at once per app
instance. Further calls wait, and a subscriber delivery that is waiting
is retried later
([How polls and `max_concurrent:` run](#how-polls-and-max_concurrent-run)).
Heavy work such as PDF rendering runs in subscribers, never in the
route.

### Pages of results

A list operation returns a `Page<T>`
([Pages](06-reads-and-writes.md#pages)). The package says how the
provider pages, and `page.next` asks for the next page. Work over many
pages uses `batches.each(from:, each:, finished:, key:)`, which pages
through in the background and resumes where it stopped
([Batches: work over many items](12-jobs-and-events.md#batches-work-over-many-items)).

### Calls and writes

In an action, calls that change something come before any write. The
writes commit after the last call returns. If the commit fails, the
action runs again, and so do its calls, under the same keys. No
transaction is open while a call is out. Reads before the call use a
snapshot closed before the call, and a write to a record loaded before it
checks the row is unchanged
([Outside calls hold no transaction](06-reads-and-writes.md#outside-calls-hold-no-transaction)).

A changing call with no idempotency key must be the only changing call
in its action.
Several keyed calls follow
[Several changing calls in one action](#several-changing-calls-in-one-action).
Work that must follow a write emits an event
([Emitting an event](12-jobs-and-events.md#emitting-an-event)).

### A call, step by step

Here an action calls an operation through the connection `couriers`,
declared in `app.jadpo`. The action is in `shipping`, a domain the
connection lists. It builds the package's own input type through the
package's name. It maps the package's `slot_taken` failure to the app's
`CollectionSlotGone`, a failure of the same kind (`conflict`). The
runtime's driver for the package's protocol, native code that speaks
JSON over HTTP, sends the call; how it is sent and read is under
[Decoding](#decoding).

```jadpo check project=p72c-call file=domains/shipping/actions/book_collection.jadpo
use app { couriers }
use shipping/failures { CollectionSlotGone }
use shared/providers/acme_couriers

/// Books a collection, mapping a slot the courier has given away to the
/// shop's own failure.
action book_collection(
    postcode: AcmeCouriers.Postcode,
    slot: AcmeCouriers.SlotId
) fails CollectionSlotGone -> AcmeCouriers.Booking {
    var request: AcmeCouriers.CreateBooking = AcmeCouriers.CreateBooking {
        postcode: postcode
        slot_id: slot
    }
    return attempt couriers.bookings.create(request) {
        slot_taken: CollectionSlotGone
    }
}
```

```jadpo check project=p72c-call file=domains/shipping/failures/collection_slot_gone.jadpo hidden
type CollectionSlotGone = Failure {
    kind: FailureKind.conflict
    message: "That collection slot has gone."
}
```

```jadpo check project=p72c-call file=app.jadpo hidden
application {
    connections: {
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

```jadpo check project=p72c-call file=config.jadpo hidden
config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=p72c-call file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API, written from its documentation.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=p72c-call file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking and tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
    credential: CredentialScheme.bearer
    idempotency: {
        header: "Idempotency-Key"
        window: 24h
        max_length: 255
    }
    error_code: "error.code"
}
```

```jadpo check project=p72c-call file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Postcode
    slot_id: SlotId {
        wire_name: "slotId"
    }
}
```

```jadpo check project=p72c-call file=shared/providers/acme_couriers/v1/types/postcode.jadpo hidden
/// A postcode as the courier takes it.
public type Postcode = Text {
    max_length: 10
}
```

```jadpo check project=p72c-call file=shared/providers/acme_couriers/v1/types/slot_id.jadpo hidden
/// A collection slot's code.
public type SlotId = Text {
    max_length: 10
}
```

```jadpo check project=p72c-call file=shared/providers/acme_couriers/v1/failures/slot_taken.jadpo hidden
/// The slot was taken before the booking reached the courier.
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "The collection slot is no longer available."
}
```

- **Mapped at the call.** Each arm of the call's `attempt` is keyed by the
  code of one of the operation's `failures:`, and names the app's failure
  of the same kind. The action fails with the app's failure, and the
  provider's failure is kept as its internal cause, for the logs. A
  failure no arm maps is a fault when it comes, and is never passed on, so
  a package failure is never in the app's `fails` list. A `match` on the
  call handles the package's failures by name, as for a library function.
- **The runtime's own failures** come as they would for any outside call.
  `misconfigured` means the provider rejected the credential, or the
  address is one the runtime won't send to.
- **`unavailable`** means the call can safely be made again. The provider
  surely did nothing: it answered `408` or `429`, or said the key is
  still in use, or the call never left. A call with an idempotency key
  that may have acted is `unavailable` too, since sending it again under
  the same key is safe.
- **`outcome_unknown`** means a call with no key may have acted: the
  provider answered with a server error, or didn't answer in time.
- **Sent once no transaction is open.** The call goes out through the
  capability seam, the one place in the runtime that makes outside calls,
  where each call is logged and keyed. What the action has only read
  before the call is a snapshot, closed before the call goes out. Its
  writes run in a transaction opened after the call, which holds each
  record loaded before the call to the row version it was loaded at
  ([Outside calls hold no transaction](06-reads-and-writes.md#outside-calls-hold-no-transaction)).
- **A call after a write.** A call made after the action's first write is
  never sent: it is the fault `call_in_transaction`. `jadpo check`
  refuses a write placed before a later call (`data.write_before_call`),
  so checked code never meets the fault. If a record changes while the
  call is out, the action runs again, and the call is repeated under its
  key. An item of a batch whose `each:` action calls a provider is a
  batch of its own ([How a run is kept](12-jobs-and-events.md#how-a-run-is-kept)).

### Retrying a call, and timing out

Here the connection sets `timeout: 5s`, so each try may take five
seconds, and `retry:`, which tries the courier up to four times within 20
seconds. Looking up a parcel changes nothing, so it is safe to send
again, and the action maps `timed_out:` at the call. A lookup the courier
never answers in time, on any try, is the shop's own
`CourierUnavailable`.

```jadpo check project=p72c2-retry file=app.jadpo
application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
            timeout: 5s
            retry: {
                attempts: 4
                within: 20s
            }
        }
    }
}
```

```jadpo check project=p72c2-retry file=domains/shipping/actions/parcel_status.jadpo
use app { couriers }
use shipping/failures { CourierUnavailable }
use shared/providers/acme_couriers

/// Where a parcel is, or the shop's own failure when the courier doesn't
/// answer in time.
action parcel_status(
    parcel: AcmeCouriers.ParcelRef
) fails CourierUnavailable -> AcmeCouriers.Status {
    var tracked_parcel: AcmeCouriers.Parcel = attempt couriers.parcels.get(
        AcmeCouriers.ParcelLookup {
            id: parcel
        }
    ) {
        timed_out: CourierUnavailable
    }
    return tracked_parcel.status
}
```

```jadpo check project=p72c2-retry file=domains/shipping/failures/courier_unavailable.jadpo hidden
/// The courier isn't answering.
type CourierUnavailable = Failure {
    kind: FailureKind.unavailable
    message: "The courier isn't answering just now."
}
```

```jadpo check project=p72c2-retry file=config.jadpo hidden
config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=p72c2-retry file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=p72c2-retry file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
    credential: CredentialScheme.bearer
}
```

```jadpo check project=p72c2-retry file=shared/providers/acme_couriers/v1/resources/parcels.jadpo hidden
public resource parcels {
    get: {
        method: HttpMethod.get
        path: "/v1/parcels/{id}"
        input: ParcelLookup
        output: Parcel
        changes: false
        source: PackageSource.written
    }
}
```

```jadpo check project=p72c2-retry file=shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo hidden
/// Which parcel to look up.
public type ParcelLookup = Object {
    id: ParcelRef
}
```

```jadpo check project=p72c2-retry file=shared/providers/acme_couriers/v1/types/parcel_ref.jadpo hidden
/// The courier's reference for a parcel.
public type ParcelRef = Text {
    max_length: 40
}
```

```jadpo check project=p72c2-retry file=shared/providers/acme_couriers/v1/types/status.jadpo hidden
/// Where a parcel is, in the courier's words.
public type Status = Text {
    max_length: 40
}
```

```jadpo check project=p72c2-retry file=shared/providers/acme_couriers/v1/types/parcel.jadpo hidden
/// A parcel and where it is.
public type Parcel = Object {
    id: ParcelRef
    status: Status
}
```

- **What is sent again.** A call that never left, or that the provider
  said did nothing (`408`, `429`, its key still in use), whatever it
  changes; and one that may have acted (a server's error, no answer in
  time) only when it changes nothing or takes its package's key, sent
  again unchanged under the same key. A change with no key that may have
  acted is never sent again: it is `outcome_unknown`.
- **How long it waits between.** 200 milliseconds before the second
  attempt, doubled before each after it, each wait drawn evenly from zero
  to that (full jitter); a provider's `Retry-After` in seconds instead,
  where it gives one. A wait that would end after `within:`, or after
  what is left of the route's deadline or of background work's budget,
  ends the retrying, and the last answer stands. Each attempt is logged
  as a call of its own.
- **`timed_out:`** turns the runtime's `timed_out` into one of the app's
  own failures of kind `unavailable`, once every attempt has gone
  unanswered in time. The runtime's `timed_out` is kept as its cause. You
  can map it only on an operation that changes nothing or is
  `idempotent: true`. Mapping it on an operation that changes something
  with no key is the error `failure.unknown_mapping`.
- **A timeout you don't map** is `unavailable` where the call was safe to
  send again, and `outcome_unknown` where it wasn't.

## Decoding
<!-- topic: decoding -->

Providers often send values in odd forms: a date as Unix seconds, a
number as text, money as a whole number of pence. Decoding turns these
into proper typed values, so your code only ever sees an `Instant`, a
`Decimal` or `Money`. The package declares how each value is written on
the wire, and the runtime decodes it.

Decoding is one of two kinds of conversion. To tell which one you need,
ask: is it needed to make the value mean what the provider's
documentation says it is?

- **Yes: decoding.** Dates as Unix seconds, numbers as text, lists as
  `"a,b,c"`, an envelope around the real object, an error inside a `200`,
  a columnar table that is really a list of rows, or Stripe's
  `amount: 1250, currency: "gbp"`, which is really money. **The package
  declares it and the runtime does it.**
- **No: translation.** Turning a correct value into the shape one app
  prefers. **Your app's action does it, with an explicit call.**

Decoding is declared on fields and on operations:

```jadpo fragment
// packages/acme_couriers/v1/types/parcel.jadpo
public type Parcel = Object {
    collect_at: Instant {
        wire: Wire.unix_seconds
    }
    weight: Decimal {
        scale: 2
        wire: Wire.text
    }
    price: Money {
        wire: Wire.minor_units
        currency: Currency.gbp
    }
    slot: SlotWindow {
        decode: slots.parse_window
        encode: slots.format_window
    }
}

// packages/acme_couriers/v1/resources/tracking.jadpo
public resource tracking {
    get: {
        method: HttpMethod.get
        path: "/v1/parcels/{id}/tracking"
        input: TrackingLookup
        wire_output: TrackingTable
        output: List<TrackingEvent>
        decode: tables.to_events
        output_at: "data"
        failed_when: {
            path: "ok"
            equals: false
        }
        changes: false
        source: PackageSource.written
        docs: "https://docs.acme-couriers.example/api#tracking"
    }
}
```

| Written | On | Means |
|---|---|---|
| `wire: Wire.x` | A field | One of the built-in encodings: Unix seconds and milliseconds, ISO dates, numbers as text, joined lists, base64, JSON inside a string, money in minor units. A fixed list extended by Jadpo releases |
| `decode:` and `encode:` | A field | A pure function pair from the package's own `libraries/`, for anything else; checked to round-trip at publish |
| `wire_output:` and `decode:` | An operation | The response as sent, and a pure function turning it into `output:`; run over every recorded response at publish |
| `output_at:` | An operation | The path of the real object inside an envelope |
| `failed_when:` | An operation | A `200` answer that is really a failure, by a path and the value that marks it (`ok` is `false`); the failure's code is then read from `error_code:` |

- **A field without `wire:` is written as Jadpo's contract writes it.**
  The contract is the one JSON form each standard type has at every
  boundary ([Standard library](19-standard-library.md)). A standard type
  in a provider's request, response or webhook is encoded that way, by
  the same codec a route's request body goes through. So an amount is
  `{"amount": 1250, "currency": "GBP"}`, and a quantity keeps its unit.
  Write `wire:` only where the provider uses another encoding.
- **Byte formats stay in the runtime**: JSON, form-encoded, multipart,
  and XML later. Package functions reshape values, never bytes.
- **Decoding is lazy where it can be.** A field decoder runs only when
  code reads the field. A whole-response decoder reads what it reads, so a
  provider adding fields it ignores breaks nothing.
- **A value that won't decode is a fault** naming the operation and
  field, as a field that doesn't match its type is. Contract runs against
  the sandbox catch it before production does.
- **App tests script the clean type**, never the wire shape. The
  package's own tests and recordings cover its decoders. Webhook payloads
  decode the same way.
- **The importer can't see most of this**, so the agent adds it with the
  other corrections, and `jadpo package record` confirms it against a
  real response.
- **The compiler can't police the line between decoding and
  translation.** The surface shows both the wire type and the output type,
  so the published documentation and the review make it visible.

### Spellings: `wire_name:`

Write `wire_name:` on a provider package's field or enum variant only
where the provider's spelling differs from the Jadpo name. The runtime
then sends and reads that spelling. A field or variant gives its spelling
in its own block. Here `failed_when:` also reads the failure's code from
the provider's `error_code:`:

```jadpo check project=p72c-call file=shared/providers/acme_couriers/v1/types/booking.jadpo
/// A booked collection.
public type Booking = Object {
    id: Text
    collect_at: Instant {
        wire_name: "collectAt"
    }
    part_of_day: SlotWindow {
        wire_name: "partOfDay"
    }
}
```

```jadpo check project=p72c-call file=shared/providers/acme_couriers/v1/types/slot_window.jadpo
/// When in the day a collection comes.
public type SlotWindow = Enum {
    morning
    after_noon: {
        wire_name: "after-noon"
    }
}
```

```jadpo check project=p72c-call file=shared/providers/acme_couriers/v1/resources/bookings.jadpo
public resource bookings {
    create: {
        method: HttpMethod.post
        path: "/v1/bookings"
        input: CreateBooking
        output: Booking
        output_at: "data"
        failed_when: {
            path: "ok"
            equals: false
        }
        changes: true
        idempotent: true
        failures: [SlotTaken]
        source: PackageSource.written
    }
}
```

So `{"ok": true, "data": {"id": "b-1", "collectAt": "2026-10-20T09:00:00Z",
"partOfDay": "after-noon"}}` is read as a `Booking` with `collect_at`
and `part_of_day: SlotWindow.after_noon`, and `{"ok": false, "error": {"code":
"slot_taken"}}` as the operation's `SlotTaken`.

### The built-in encodings: `wire:`

A field names one of these ten built-in encodings with `wire:`:

| `wire:` | On a field of | The provider sends |
|---|---|---|
| `Wire.unix_seconds` | `Instant` | A whole number of seconds since 1970: `1792400400` |
| `Wire.unix_milliseconds` | `Instant` | A whole number of milliseconds since 1970 |
| `Wire.iso_8601` | `Instant` | ISO 8601 text with any offset from UTC, `"2026-10-19T10:00:00+01:00"`; written in UTC |
| `Wire.day_month_year` | `CalendarDate` | `"19/10/2026"` |
| `Wire.text` | `Int`, `Decimal` | The number as text: `"3"`, `"2.50"` |
| `Wire.number` | `Int`, `Decimal` | A JSON number, every place written kept: `2.50` |
| `Wire.comma_separated` | A list of text, numbers or an enum's plain variants | One text, joined by commas: `"fragile,heavy"` |
| `Wire.base64` | `Text` | The Base64 of its UTF-8 |
| `Wire.json_text` | Anything | Its JSON, inside a string |
| `Wire.minor_units` | `Money` whose type fixes its `currency:` | A whole number of the currency's minor units: `1250` |

### A field's own functions: `decode:` and `encode:`

For anything the built-in encodings don't cover, a field names a pair of
functions. `decode:` takes the value as the provider sends it and gives
the field's type, failing when it can't. `encode:` turns the field's type
back, without failing. Both live in the package's own `libraries/`, so
they may be internal:

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/types/parcel.jadpo
/// A parcel and where it is.
public type Parcel = Object {
    id: ParcelRef
    status: Status
    collect_at: CollectionTime {
        wire: Wire.unix_seconds
        wire_name: "collectAt"
    }
    weight: Weight {
        wire: Wire.number
    }
    price: ParcelPrice {
        wire: Wire.minor_units
    }
    slot: SlotWindow {
        decode: courier_windows.parse_window
        encode: courier_windows.format_window
    }
}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/libraries/courier_windows.jadpo
/// The courier's collection windows, which it writes as "AM" and "PM".
library courier_windows {
    /// The window the courier's code names.
    function parse_window(code: WindowCode) fails WindowUnknown -> SlotWindow {
        if (code == "AM") {
            return SlotWindow.morning
        }
        if (code == "PM") {
            return SlotWindow.afternoon
        }
        reject WindowUnknown
    }

    /// The courier's code for a window.
    function format_window(pickup_window: SlotWindow) -> WindowCode {
        if (pickup_window == SlotWindow.morning) {
            return "AM"
        }
        return "PM"
    }
}
```

### A whole answer: `wire_output:`

An operation's `wire_output:` is the whole answer as sent, read from
inside `output_at:`. Its `decode:` turns it into `output:`:

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/resources/tracking.jadpo
public resource tracking {
    get: {
        method: HttpMethod.get
        path: "/v1/parcels/{id}/tracking"
        input: ParcelLookup
        wire_output: TrackingTable
        output: List<TrackingEvent>
        decode: tracking_sheets.to_events
        output_at: "data"
        changes: false
        source: PackageSource.written
    }
}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/libraries/tracking_sheets.jadpo
/// The courier's tracking history, as a list of where a parcel was seen.
library tracking_sheets {
    /// Each sighting as an event.
    function to_events(history_sheet: TrackingTable) -> List<TrackingEvent> {
        return list.map(
            history_sheet.sightings,
            sighting -> TrackingEvent {
                hub: sighting.hub
            }
        )
    }
}
```

An action that reads one field of an answer decodes that field alone. So
`{"id": "p-1", "status": "held", "collectAt": "soon", "slot": "NOON"}` is
read for its `status`, even though its time and its window would never
decode:

```jadpo check project=p72c2-wire file=domains/shipping/actions/parcel_status.jadpo
use app { couriers }
use shared/providers/acme_couriers

/// Where a parcel is.
action parcel_status(parcel: AcmeCouriers.ParcelRef) -> AcmeCouriers.Status {
    var tracked_parcel: AcmeCouriers.Parcel = couriers.parcels.get(
        AcmeCouriers.ParcelLookup {
            id: parcel
        }
    )
    return tracked_parcel.status
}
```

```jadpo check project=p72c2-wire file=domains/shipping/actions/parcel_history.jadpo hidden
use app { couriers }
use shared/providers/acme_couriers

/// Where a parcel has been.
action parcel_history(parcel: AcmeCouriers.ParcelRef) -> List<AcmeCouriers.TrackingEvent> {
    return couriers.tracking.get(
        AcmeCouriers.ParcelLookup {
            id: parcel
        }
    )
}
```

```jadpo check project=p72c2-wire file=app.jadpo hidden
application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
        }
    }
}
```

```jadpo check project=p72c2-wire file=config.jadpo hidden
config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
    credential: CredentialScheme.bearer
}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/resources/parcels.jadpo hidden
public resource parcels {
    get: {
        method: HttpMethod.get
        path: "/v1/parcels/{id}"
        input: ParcelLookup
        output: Parcel
        changes: false
        source: PackageSource.written
    }
}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo hidden
/// Which parcel to look up.
public type ParcelLookup = Object {
    id: ParcelRef
}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/types/parcel_ref.jadpo hidden
/// The courier's reference for a parcel.
public type ParcelRef = Text {
    max_length: 40
}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/types/status.jadpo hidden
/// Where a parcel is, in the courier's words.
public type Status = Text {
    max_length: 40
}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/types/collection_time.jadpo hidden
/// When a parcel is collected.
public type CollectionTime = Instant {}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/types/weight.jadpo hidden
/// A parcel's weight in kilograms.
public type Weight = Decimal {
    scale: 2
    min: 0.0
    max: 1000.0
}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/types/parcel_price.jadpo hidden
/// What sending a parcel costs, in pounds.
public type ParcelPrice = Money {
    currency: Currency.gbp
}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/types/slot_window.jadpo hidden
/// When in the day a collection comes.
public type SlotWindow = Enum {
    morning
    afternoon
}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/types/window_code.jadpo hidden
/// A collection window as the courier writes it.
public type WindowCode = Text {
    max_length: 2
}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/types/tracking_table.jadpo hidden
/// The courier's tracking history as it sends it.
public type TrackingTable = Object {
    sightings: List<Scan>
}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/types/scan.jadpo hidden
/// One sighting as the courier sends it.
public type Scan = Object {
    hub: Text {
        wire_name: "where"
    }
}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/types/tracking_event.jadpo hidden
/// Where a parcel was seen.
public type TrackingEvent = Object {
    hub: Text
}
```

```jadpo check project=p72c2-wire file=shared/providers/acme_couriers/v1/failures/window_unknown.jadpo hidden
/// A collection window this package doesn't know.
type WindowUnknown = Failure {
    kind: FailureKind.invalid_value
    message: "The courier named a collection window this package doesn't know."
}
```

### Parts sent separately: `wire_parts:`

Some providers send a standard type's parts as separate members of the
object that holds it. Stripe sends `"amount": 1050, "currency": "gbp"`
beside a charge's other members. In Jadpo that is one field, whose
`wire_parts:` names the member each part is read from and written to. The
field has no member of its own on the provider's wire. Fields may share
one part's member, as a charge's amounts share its currency:

```jadpo check project=p72e-parts file=shared/providers/tidepay/v1/types/charge.jadpo
/// A charge to a card.
public type Charge = Object {
    id: ChargeId
    amount: Amount {
        wire_parts: {
            amount: "amount"
            currency: "currency"
        }
    }
    amount_refunded: Amount {
        wire_parts: {
            amount: "amount_refunded"
            currency: "currency"
        }
    }
    merchant: Point? {
        wire_parts: {
            latitude: "lat"
            longitude: "lng"
        }
    }
}
```

```jadpo check project=p72e-parts file=shared/providers/tidepay/v1/resources/charges.jadpo
public resource charges {
    get: {
        method: HttpMethod.get
        path: "/v1/charges/{id}"
        input: ChargeLookup
        output: Charge
        changes: false
        source: PackageSource.written
    }
}
```

```jadpo check project=p72e-parts file=domains/billing/actions/refunded_so_far.jadpo
use app { payments }
use shared/providers/tidepay

/// How much of a charge has been refunded.
action refunded_so_far(charge: Tidepay.ChargeId) -> Tidepay.Amount {
    var paid_charge: Tidepay.Charge = payments.charges.get(
        Tidepay.ChargeLookup {
            id: charge
        }
    )
    return paid_charge.amount_refunded
}
```

```jadpo check project=p72e-parts file=app.jadpo hidden
application {
    connections: {
        payments: {
            package: Tidepay.v1
            domains: [billing]
            credential: config.payments_api_key
            environment: config.payments_environment
        }
    }
}
```

```jadpo check project=p72e-parts file=config.jadpo hidden
config {
    payments_api_key: Text {
        secret: true
    }
    payments_environment: ProviderEnvironment
}
```

```jadpo check project=p72e-parts file=shared/providers/tidepay/v1/package.jadpo hidden
/// Tidepay's card payments.
package Tidepay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=p72e-parts file=shared/providers/tidepay/v1/provider.jadpo hidden
/// How Tidepay is reached and called.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.tidepay.example"
        sandbox: "https://sandbox.tidepay.example"
    }
    credential: CredentialScheme.bearer
}
```

```jadpo check project=p72e-parts file=shared/providers/tidepay/v1/types/charge_lookup.jadpo hidden
/// Which charge.
public type ChargeLookup = Object {
    id: ChargeId
}
```

```jadpo check project=p72e-parts file=shared/providers/tidepay/v1/types/charge_id.jadpo hidden
/// Tidepay's reference for a charge.
public type ChargeId = Text {
    max_length: 40
}
```

```jadpo check project=p72e-parts file=shared/providers/tidepay/v1/types/amount.jadpo hidden
/// An amount charged.
public type Amount = Money {}
```

So `{"id": "ch_1", "amount": 1050, "amount_refunded": 250, "currency":
"gbp", "lat": 51.5, "lng": -0.125}` is read as a `Charge` of £10.50, with
£2.50 refunded, at a merchant at 51.5° N, 0.125° W. A `Charge` is sent
back as those six members, the currency once. Two amounts in different
currencies can't be sent, since they share one member, and the call
stops as one that doesn't encode.

A `Point` is read from its `latitude` and `longitude`, which GeoJSON
would send as a pair. Any other standard type made of named parts is read
from those parts: `Money`'s `amount`, in minor units, and `currency`;
`InstantRange`'s `start` and `end`. A field none of whose members is sent
is `none` where it may be, and one with only some is a fault naming the
missing member. Everywhere else, the field is written as the contract
writes its type.

### How decoding runs

- **Where they apply.** On the provider's wire: an operation's input,
  its answer and its webhooks. Everywhere else, a
  `json.write`, a route's answer or an event's payload, the field is
  written as the contract writes its type, so the app's clients never see
  a provider's encoding.
- **Run at the call's site.** A field's `decode:` and `encode:`, and an
  operation's `decode:`, run on the executor where the call is made, as
  any function the action calls would; one that fails, or gives no value,
  is a fault naming the operation and the field, never the value.
- **Only the fields read.** The answer to a call held in a variable that
  code only reads fields of (`tracked_parcel.status`) is decoded for those fields
  alone; returning it, passing it on, or anything else uses it whole.
  Fields further in are decoded with the field that holds them. An
  answer decoded whole by an operation's `decode:` is read whole.

## What package code may do
<!-- topic: what-package-code-may-do -->

A provider package may act on its own API, but never on your app's data.
That way a package can carry what someone who knows the API has learned,
written once and tested, without reaching into the apps that install it.

- **No state and no database.** A package owns no entities or tables and
  never reads or writes the app's data. Data a package fetches reaches the
  app as a return value or an event, and the app's own action writes it.
- **Package actions call only their own API.** A provider package may
  hold `public action`s in `actions/`. They call the package's own
  operations in any order, pass answers between them, use its pure
  functions, and return a result or fail. They can't emit app events, send
  email, call any other connection, or read configuration or secrets. The
  app calls one as `connection.action(input)` inside its own action. One
  with several changing calls follows
  [Several changing calls in one action](#several-changing-calls-in-one-action).
  Inside a package, operations are named without the connection:
  `customers.create`.
- **Package subscribers on its own webhooks.** A package may subscribe to
  its own webhook events, call its own API to enrich them, and emit its
  own declared events, such as `Stripe.SubscriptionRenewed`.
- **Events only about the outside world.** A package emits events for
  webhooks, enriched webhooks and polls, never as a side effect of the app
  calling it. Where the app wants an event after a call, the app's own
  action emits it, where a reviewer sees it. So **an action that emits
  can't be `public`**: only the package's subscribers and polls run it.
  Package events travel through the app's outbox like any other.
- **Package code runs only when started**: by the app calling it, or by
  the outside world through a webhook or a poll the app subscribes to.

In this sketch, Stripe's package holds an action that makes several calls
in the order Stripe requires. Its subscriber `renewal` hears the raw
`InvoicePaid` webhook event, which isn't public, and runs
`announce_renewal`, which emits the public `SubscriptionRenewed`. The app
calls the action through its `stripe` connection:

```jadpo fragment
// packages/stripe/v2024_06_20/actions/start_subscription.jadpo
/// Creates the customer, attaches the card, starts the subscription and
/// finalises its first invoice, in the order Stripe requires.
public action start_subscription(input: StartSubscription) fails CardDeclined -> StartedSubscription { ... }

// packages/stripe/v2024_06_20/subscribers/renewal.jadpo
subscriber renewal {
    on: InvoicePaid
    run: announce_renewal(event)
}

// domains/billing/actions/subscribe.jadpo
var started: Stripe.StartedSubscription = attempt stripe.start_subscription(request) {
    card_declined: PaymentDeclined
}
```

### A package action, compiled

Here a package's action calls its own operations, one of which the
package keeps to itself, and passes each answer to the next. The app calls
the action through its connection and maps its failure at the call. A
package action's changing calls are steps of the action that calls it
([Several changing calls in one action](#several-changing-calls-in-one-action)).
The choices are in "Package actions and recipes", further down this page.

The card can be declined after the customer was created. So the
package's surface lists `after_effect DeclinedCard` for
`start_subscription`, and the app's call of it gets the warning
`step.late_failure`: a point that can end `subscribe` for good once
something took effect ([Jobs and events](12-jobs-and-events.md)). Here
the empty customer left behind is harmless, so the warning is declined
with that reason.

```jadpo check project=p76-actions file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        pay: {
            package: AcmePay.v1
            domains: [billing]
            credential: config.pay_api_key
            environment: config.pay_environment
        }
    }
    events: {
        store: main
    }
    steps: {
        encryption_key: config.step_encryption_key
    }
}
```

```jadpo check project=p76-actions file=config.jadpo hidden
config {
    pay_api_key: Text {
        secret: true
    }
    pay_environment: ProviderEnvironment
    step_encryption_key: SigningKey
}
```

```jadpo check project=p76-actions file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=p76-actions file=shared/providers/acme_pay/v1/provider.jadpo hidden
/// Acme Pay's payments API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-pay.example"
        sandbox: "https://sandbox.acme-pay.example"
    }
    credential: CredentialScheme.bearer
    idempotency: {
        header: "Idempotency-Key"
        window: 24h
    }
    error_code: "error.code"
}
```

```jadpo check project=p76-actions file=shared/providers/acme_pay/v1/types/pay_request.jadpo hidden
/// What each call takes.
public type PayRequest = Object {
    reference: Reference
}
```

```jadpo check project=p76-actions file=shared/providers/acme_pay/v1/types/pay_reply.jadpo hidden
/// What each call gives.
public type PayReply = Object {
    id: Reference
}
```

```jadpo check project=p76-actions file=shared/providers/acme_pay/v1/types/reference.jadpo hidden
/// Acme Pay's reference for a record.
public type Reference = Text {
    max_length: 40
}
```

```jadpo check project=p76-actions file=shared/providers/acme_pay/v1/failures/declined_card.jadpo hidden
/// The card was declined.
public type DeclinedCard = Failure {
    kind: FailureKind.rejected
    message: "The card was declined."
}
```

```jadpo check project=p76-actions file=shared/providers/acme_pay/v1/resources/customers.jadpo hidden
resource customers {
    create: {
        method: HttpMethod.post
        path: "/v1/customers"
        input: PayRequest
        output: PayReply
        changes: true
        idempotent: true
        source: PackageSource.written
    }
}
```

```jadpo check project=p76-actions file=shared/providers/acme_pay/v1/resources/cards.jadpo hidden
public resource cards {
    attach: {
        method: HttpMethod.post
        path: "/v1/cards"
        input: PayRequest
        output: PayReply
        changes: true
        idempotent: true
        failures: [DeclinedCard]
        source: PackageSource.written
    }
}
```

```jadpo check project=p76-actions file=domains/billing/failures/payment_refused.jadpo hidden
/// The payment provider refused the card.
type PaymentRefused = Failure {
    kind: FailureKind.rejected
    message: "The card was refused."
}
```

```jadpo check project=p76-actions file=domains/billing/events/plan_chosen.jadpo hidden
use shared/providers/acme_pay

/// A customer chose a plan.
type PlanChosen = Event {
    tier: AcmePay.Reference
}
```

```jadpo check project=p76-actions file=domains/billing/events/events.jadpo_record hidden
// Generated by Jadpo from the events of domain billing; update it with `jadpo records update`, never by hand.
record events 1

event PlanChosen
    tier: Text { max_length: 40 }
```

```jadpo check project=p76-actions file=domains/billing/subscribers/open_subscription.jadpo hidden
use billing/actions { subscribe }
use billing/events { PlanChosen }

/// Starts the subscription a customer chose.
subscriber open_subscription {
    on: PlanChosen
    run: subscribe(event.tier)
}
```

```jadpo check project=p76-actions file=shared/providers/acme_pay/v1/actions/start_subscription.jadpo
/// Creates the customer, then attaches the card to it.
public action start_subscription(input: PayRequest) fails DeclinedCard -> PayReply {
    var customer: PayReply = customers.create(input)
    return attempt cards.attach(
        PayRequest {
            reference: customer.id
        }
    )
}
```

```jadpo check project=p76-actions file=domains/billing/actions/subscribe.jadpo
use app { pay }
use billing/failures { PaymentRefused }
use shared/providers/acme_pay

/// Starts a subscription: two steps, run by a subscriber.
decline step.late_failure: "A declined card leaves an empty customer at Acme Pay, which costs nothing"
action subscribe(tier: AcmePay.Reference) fails PaymentRefused -> AcmePay.PayReply {
    return attempt pay.start_subscription(
        AcmePay.PayRequest {
            reference: tier
        }
    ) {
        declined_card: PaymentRefused
    }
}
```

### A package subscriber, compiled

Here a package's subscriber hears an event its webhooks bring, which the
package keeps to itself. It runs the package's action, which looks the
subscription up through the connection the webhook came in on and emits
the package's own public event. The app's subscriber keeps that event.
The choices are under "Package subscribers and recipes in `sample/`",
further down this page. The event the action emits gives no `wire_name:`,
because `wire_name:` marks an event the provider sends.

```jadpo check project=p76b-subscribers file=app.jadpo hidden
application {
    connections: {
        payments: {
            package: AcmePay.v1
            domains: [billing]
            credential: config.payments_key
            environment: config.payments_environment
            webhooks: {
                secret: config.payments_webhook_secret
            }
        }
    }
}
```

```jadpo check project=p76b-subscribers file=config.jadpo hidden
config {
    payments_environment: ProviderEnvironment
    payments_key: Text {
        secret: true
    }
    payments_webhook_secret: Text {
        secret: true
    }
}
```

```jadpo check project=p76b-subscribers file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=p76b-subscribers file=shared/providers/acme_pay/v1/provider.jadpo hidden
/// Acme Pay's API, which signs its webhooks as Stripe does.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-pay.example"
        sandbox: "https://sandbox.acme-pay.example"
    }
    credential: CredentialScheme.bearer
    webhooks: {
        check: SignatureScheme.stripe
        event_id: "id"
        event_type: "type"
        occurred_at: "created"
    }
}
```

```jadpo check project=p76b-subscribers file=shared/providers/acme_pay/v1/types/reference.jadpo hidden
/// Acme Pay's reference for a record.
public type Reference = Text {
    max_length: 40
}
```

```jadpo check project=p76b-subscribers file=shared/providers/acme_pay/v1/types/subscription_asked.jadpo hidden
/// Which subscription to look up.
type SubscriptionAsked = Object {
    id: Reference
}
```

```jadpo check project=p76b-subscribers file=shared/providers/acme_pay/v1/types/subscription.jadpo hidden
/// A subscription, as Acme Pay keeps it.
type Subscription = Object {
    id: Reference
    tier: Reference
}
```

```jadpo check project=p76b-subscribers file=shared/providers/acme_pay/v1/resources/subscriptions.jadpo hidden
resource subscriptions {
    retrieve: {
        method: HttpMethod.post
        path: "/v1/subscriptions/lookup"
        input: SubscriptionAsked
        output: Subscription
        source: PackageSource.written
    }
}
```

```jadpo check project=p76b-subscribers file=shared/providers/acme_pay/v1/events/invoice_paid.jadpo
/// An invoice was paid: all Acme Pay's webhook says, kept to the package.
type InvoicePaid = Event {
    wire_name: "invoice.paid"
    occurred_at: Instant
    subscription: Reference
}
```

```jadpo check project=p76b-subscribers file=shared/providers/acme_pay/v1/events/subscription_renewed.jadpo
/// A subscription was renewed, with its tier: the package's own event.
public type SubscriptionRenewed = Event {
    subscription: Reference
    tier: Reference
    paid_at: Instant
}
```

```jadpo check project=p76b-subscribers file=shared/providers/acme_pay/v1/actions/announce_renewal.jadpo
/// Looks up the subscription an invoice paid for, and announces it.
action announce_renewal(event: InvoicePaid) -> Unit {
    var renewed_subscription: Subscription = subscriptions.retrieve(
        SubscriptionAsked {
            id: event.subscription
        }
    )
    emit SubscriptionRenewed {
        subscription: renewed_subscription.id
        tier: renewed_subscription.tier
        paid_at: event.occurred_at
    }
}
```

```jadpo check project=p76b-subscribers file=shared/providers/acme_pay/v1/subscribers/renewal.jadpo
/// Turns a paid invoice into a renewal.
subscriber renewal {
    on: InvoicePaid
    run: announce_renewal(event)
}
```

```jadpo check project=p76b-subscribers file=domains/billing/entities/renewal.jadpo hidden
use shared/providers/acme_pay

/// A subscription renewed at Acme Pay.
entity Renewal {
    id: Uuid
    subscription: AcmePay.Reference
    tier: AcmePay.Reference
    paid_at: Instant

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
```

```jadpo check project=p76b-subscribers file=domains/billing/actions/record_renewal.jadpo hidden
use billing/entities { Renewal }
use shared/providers/acme_pay

/// Keeps a renewal Acme Pay announced.
action record_renewal(event: AcmePay.SubscriptionRenewed) -> Renewal {
    return Renewal.create {
        set: {
            subscription: event.subscription
            tier: event.tier
            paid_at: event.paid_at
        }
    }
}
```

```jadpo check project=p76b-subscribers file=domains/billing/subscribers/renewed.jadpo migrate=initial
use billing/actions { record_renewal }
use shared/providers/acme_pay

/// Keeps each renewal Acme Pay announces.
subscriber renewed {
    on: AcmePay.SubscriptionRenewed
    run: record_renewal(event)
}
```

A signed `invoice.paid` reaches `renewal` once, and its lookup is sent
through `payments` with that connection's credential. The renewal it
emits reaches `renewed` through the app's outbox.

### Polls

**Polling turns an API without webhooks into events.** `polls:` in
`provider.jadpo` names an operation, a default interval and the event
emitted when the answer changes. The runtime calls the operation on that
interval and keeps what it last saw, as runtime state, like webhook
de-duplication. It emits an event only when the answer changes. A poll
runs only while something subscribes to its event, and the connection can
change the interval: `polls: { latest: 6h }`.

```jadpo fragment
// packages/exchange_rates/v1/provider.jadpo
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.rates.example"
    }
    credential: CredentialScheme.bearer
    polls: {
        latest: {
            call: rates.latest
            every: 1d
            emits: RatesPublished
        }
    }
}
```

Polls are for feeds that don't depend on your app's data, such as
exchange rates or a price list. To check each of your own records, such
as each parcel still in transit, write a schedule in your app that calls
the package, one event per record. A package can give that as a recipe.

Here a poll of Acme Fuel's price list emits the package's own public
`PricesPublished`, made from its answer. The operation takes nothing and
changes nothing. The connection polls it every six hours instead of once
a day, and the app keeps each change. `fetched_at`, which the event
doesn't carry, never counts as a change. The choices are under "How polls
and `max_concurrent:` run", further down this page.

```jadpo check project=p76c-polls file=app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        fuel: {
            package: FuelPrices.v1
            domains: [pricing]
            credential: config.fuel_key
            environment: config.fuel_environment
            max_concurrent: 2
            polls: {
                latest: 6h
            }
        }
    }
    events: {
        store: main
    }
}
```

```jadpo check project=p76c-polls file=config.jadpo hidden
config {
    fuel_environment: ProviderEnvironment
    fuel_key: Text {
        secret: true
    }
}
```

```jadpo check project=p76c-polls file=shared/providers/fuel_prices/v1/package.jadpo hidden
/// Acme Fuel's price list.
package FuelPrices.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=p76c-polls file=shared/providers/fuel_prices/v1/provider.jadpo
/// Acme Fuel's price list, which sends no webhooks.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.fuel-prices.example"
        sandbox: "https://sandbox.fuel-prices.example"
    }
    credential: CredentialScheme.bearer
    max_concurrent: 4
    polls: {
        latest: {
            call: pump_prices.latest
            every: 1d
            emits: PricesPublished
        }
    }
}
```

```jadpo check project=p76c-polls file=shared/providers/fuel_prices/v1/types/price_list.jadpo hidden
/// The day's prices, as the API gives them.
type PriceList = Object {
    region: Text
    diesel_pence: Int
    petrol_pence: Int
    fetched_at: Instant
}
```

```jadpo check project=p76c-polls file=shared/providers/fuel_prices/v1/resources/pump_prices.jadpo hidden
resource pump_prices {
    latest: {
        method: HttpMethod.get
        path: "/v1/prices/latest"
        output: PriceList
        source: PackageSource.written
    }
}
```

```jadpo check project=p76c-polls file=shared/providers/fuel_prices/v1/events/prices_published.jadpo
/// The day's pump prices, published.
public type PricesPublished = Event {
    region: Text
    diesel_pence: Int
    petrol_pence: Int
}
```

```jadpo check project=p76c-polls file=domains/pricing/entities/pump_price.jadpo hidden
/// A region's pump prices, as Acme Fuel published them.
entity PumpPrice {
    id: Uuid
    region: Text
    diesel_pence: Int
    petrol_pence: Int

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
```

```jadpo check project=p76c-polls file=domains/pricing/actions/keep_prices.jadpo hidden
use pricing/entities { PumpPrice }
use shared/providers/fuel_prices

/// Keeps the prices Acme Fuel published.
action keep_prices(event: FuelPrices.PricesPublished) -> PumpPrice {
    return PumpPrice.create {
        set: {
            region: event.region
            diesel_pence: event.diesel_pence
            petrol_pence: event.petrol_pence
        }
    }
}
```

```jadpo check project=p76c-polls file=domains/pricing/subscribers/prices_published.jadpo migrate=initial
use pricing/actions { keep_prices }
use shared/providers/fuel_prices

/// Keeps each change of price.
subscriber prices_published {
    on: FuelPrices.PricesPublished
    run: keep_prices(event)
}
```

The tick, the runtime's regular check for background work that is due,
calls `pump_prices.latest` through `fuel` with its credential once the
poll is due. It keeps `PricesPublished` and its deliveries only when the
prices changed, and tries a failed call again a minute later. Without the
subscriber the poll never runs. Under `max_concurrent: 1` a call waits for
the connection's slot, and a delivery that would call through it waits in
the outbox.

### Services

A package's own API may be a container Jadpo runs, declared with `runs:`.
The package's actions call it. A service can't call another package or
connection.

```jadpo fragment
// packages/gotenberg/v8/provider.jadpo
/// Gotenberg's HTTP API, run by Jadpo as a container beside the app.
provider {
    protocol: Protocol.multipart_http
    runs: {
        image: "gotenberg/gotenberg:8@sha256:…"
        port: 3000
        asks: ["fonts.googleapis.com"]
    }
}
```

- **Only stateless services.** A service holds no data and has no volume,
  and each app instance runs its own copy, so many servers need nothing
  shared. Anything that holds data (PostgreSQL, Redis) is infrastructure
  the operator provides and the app reaches through a driver connection.
- **A service never starts until approved.** Installing shows the image, its
  digest and the hosts it asks for; the app names it on a connection, and a
  new or changed image needs a person's acceptance in the approval report
  before it runs, so an upgrade cannot swap in a different image unseen. The
  image's digest is written in the package source, which the package's own
  digest pins; an own integration in `shared/providers/` has no lock entry,
  and its image is listed the same way.
- **Outbound network needs the app's permission.** A package only asks
  (`asks:`); the service gets none of those hosts until the connection
  grants them (`outbound: [...]`), and a new grant is a new outside host,
  which needs a person's acceptance (`NetworkChange.host_added`). Until
  then `jadpo check` names the hosts asked for and the line that grants
  them.
- **Three settings Jadpo always applies** when it creates a container: no
  outbound network unless granted, never privileged, and no access to the
  host's disk. Jadpo creates the containers itself, so there is no
  configuration to audit.
- **Reaching a service.** A package says only the port its service listens
  on inside its container, so two packages can both use 3000. Jadpo gives
  each app a private network that only it and its services are on, and
  hands each connection the address: the service's name on that network
  when the app runs in a container, or a loopback port the engine picked
  when it runs as a plain process. Nobody writes an address.
- **Jadpo is a client, never a container runtime.** It talks to whatever
  engine the machine has through the Docker-compatible API, which Docker and
  Podman both serve, and leaves pulling, isolation and restart policies to
  it. An app with no service never needs an engine; when one is needed and
  none is found, `jadpo dev` and the deployable say so and how to install
  one.
- **Docker stays behind the scenes.** `jadpo services` lists each service
  (package, image, state, health, CPU and memory, restarts, uptime);
  `jadpo services start`, `stop` and `restart <name>`, `logs <name>`
  (`--follow`, `--since`) and `shell <name>` (development only) manage them,
  each with `--json`. In production the commands talk to the running app,
  which supervises its services. A service's output joins the app's
  OpenTelemetry logs under its name, and a call to it appears in the
  request's trace as a provider call.
- **Where services run.** `jadpo dev` starts the declared services, and
  tests keep using fakes. `jadpo build` writes the configuration that runs
  each one beside the app, Compose first. The native deployable supervises
  its services itself. On Workers, which runs no containers, a service is
  hosted elsewhere and the connection gives `host:`.

### Recipes

A package may ship `recipes/`: complete wiring examples (schedules, actions,
entities) that `jadpo package recipe <package> <recipe> --into <domain>`
copies into a domain, after which they are the app's own code. Each is
compiled and tested against the package's `sample/` app at publish and
listed in the surface. Library packages may ship recipes too.

> `jadpo package check`'s `recipes` row compiles each recipe on its own,
> copied into the one domain of the package's `sample/` app as `jadpo
> package recipe` copies it, the package placed as the sample's own
> integration; a recipe that doesn't compile is shown at its own path,
> `recipes/<recipe>/...`, and a package with recipes and no such sample
> is `publish.sample_missing`. The choices are under "Package
> subscribers and recipes in `sample/`".

Not built yet: services run by Jadpo (`runs:`), and running a recipe's
tests in `sample/`.

## Several changing calls in one action
<!-- topic: several-changing-provider-calls -->

Some jobs need several calls that each change something at a provider,
in order: create a customer, attach a card, start a subscription. If the
action stops halfway, a rerun must not do the first steps again. Jadpo
records each step before it is sent, replays the recorded answers on a
rerun, and stops the action as `diverged` if anything differs.

These rules apply to an app's action and to the package actions it
calls, which count as one sequence. "Several changing calls in one
action" in [Jobs and events](12-jobs-and-events.md) describes the same
rules from the side of the event delivery that runs the action.

- **Words.** A *step* is one outside call made by an action from its first
  changing call on, numbered along the path the action runs, package
  actions' calls included. The *record* is the rows holding an action's
  steps, keyed by the action's key. A step is *in doubt* when it was
  recorded as sent with no answer recorded.
- **How many, and which.** An action may make up to 8 changing calls in
  order, counted on its longest path with package actions' calls included.
  Each must be `idempotent: true` in its package: it takes the key, or it
  is safe to repeat as it is (a `PUT` that replaces, a `DELETE`). A changing call with
  neither stays the only changing call in its action. A package action over
  the limit cannot be published.
- **Where.** Only in actions run from subscribers, or from routes whose API
  requires `Idempotency-Key`. Reaching such an action from a schedule, or
  from a route whose API does not require the key, is a compile error naming
  the two fixes: require the key, or answer 202 and do the steps in a
  subscriber.
- **No outside call inside `for`** in such an action, so the compiler knows
  every path's length; batch work stays one event per item.
- **The record.** Before a step is sent the runtime records its position,
  operation, a fingerprint of its request (over the typed value, not the
  bytes) and the time sent; when the answer comes, success or failure, it
  records the answer as the provider sent it, encrypted, at most 1 MB (a
  larger one is a fault naming the step). Provider reads after the first
  changing step are recorded the same way. Database reads are not, and
  reads before the first changing step are not.
- **Replay.** A rerun replays answered steps in order without sending them,
  decoding each stored answer with the current package. At each step the
  operation and the fingerprint must match, and at the end every recorded
  step must have been replayed; otherwise the action stops as `diverged`
  before anything is sent, naming the step, its operation and the top-level
  request fields that differ. A diverged action is held for an operator,
  not failed: deploying matching code and retrying finishes it.
- **Time.** Once a record exists, `clock.now` is the time stored in it (the
  first run's start) for every rerun, including the rerun after a failed
  commit ([Reads and writes](06-reads-and-writes.md)). A resumed action
  writes timestamps from when it began, which is also when the provider
  acted. Before the first changing step a rerun takes a fresh time.
  [Configuration and time](10-configuration-and-time.md) holds the same.
- **Keys and windows.** Each step's key is derived as "Calling" says, the
  step number and key generation included. A keyed step in doubt inside its
  provider's `window:` (less a margin, the smaller of a tenth of the window
  and one hour) ends the run retry-later, and the next attempt sends it again
  under the same key. It parks as `outcome_unknown` when it has no key, when
  its window would pass, or when the package says `replays_failures: true`
  and the provider answered 5xx. Only the package's `new_key_on:` answer
  makes the runtime send again under the next key generation by itself. The window rule holds for single calls too.
  A resumable single call with a store already available
  keeps only its operation, first dispatch time, key generation and runner
  generation in the step tables, with no request fingerprint or answer and
  no steps encryption setting. It keeps a fresh code clock and its existing
  provider key; host time bounds retries. A saved record cannot switch
  between encrypted replay and dispatch metadata: its saved mode disagrees
  with the new build's, so it stops as `diverged` before sending anything,
  with the problem kept for the operator. A route with no store keeps no
  durable history across independent requests.
- **One runner per record.** Routes claim the client's key before the
  action runs ([HTTP](07-http.md)). A delivery's lease carries a generation that every record
  write checks, so a runner whose lease lapsed stops before sending another
  step. The provider's "key in use" answer means wait: retried within the
  call's budget, then retry-later, never a failure. The provider's
  "different request" answer is `diverged`, never `misconfigured`.
- **Transactions.** Database writes still come after the last call, with
  no transaction open during calls
  ([Outside calls hold no transaction](06-reads-and-writes.md#outside-calls-hold-no-transaction)).
  Record writes are the runtime's own, each in a short transaction.
- **The warning.** After the first changing step, every point that can end
  the action for good (a step's failure, a write that can be refused,
  `reject`, a called action's failure) is warned unless handled with
  `match`, naming three fixes: check the condition before the first step;
  record the answers and finish in a subscriber of an event the write emits;
  undo as another step. A package's surface lists which of its failures
  follow an effect, so the warning reaches through package actions. It
  holds for single calls too.
- **Undo is code.** There is no built-in compensation. An undo is a step
  like any other: it needs a key, gets the same warning, and a failure
  mapped as "already undone" (Stripe's `resource_missing` on a delete)
  counts as handled.
- **Ending.** The record is removed in the action's commit transaction,
  with the delivery's handled marker or the route's stored response. A
  record that ends any other way (failed, diverged, parked, or a route's
  key lapsing after 24 hours) becomes an operator item listing each step
  and its answer, with retry (`--not-done` to send the step in doubt under
  a new key once the operator has checked it did not happen) and abandon
  (end the action as failed, keeping the list of effects for tidying up by
  hand). It is kept at most 30 days, after which the answers are dropped
  and the step list stays.
- **Where records live.** In the store the action writes, beside the
  outbox; an action that writes no store needs `events: { store: }`, or it
  is a compile error.
- **Personal data.** Requests are kept only as fingerprints. Answers are
  encrypted with `application { steps: { encryption_key: } }`
  ([Jobs and events](12-jobs-and-events.md)). Nothing links them to a person, so the
  personal-data report lists "provider answers for unfinished actions, at
  most 30 days", as it does for logs. A metric and a no-setup alert watch
  the number of unfinished records and the age of the oldest.
- **Deploys.** Each record keeps the build id it started under and a digest
  of its action's step shape (the operations on each path, package steps
  included). No deploy command is involved: a rerun under another build
  sees the different build id in the manifest, compares the step shape,
  and stops as `diverged` before sending anything if it changed. Code after
  the last recorded step, new steps after it, writes and failure mappings
  are not changes to the shape.
- **Run limits.** A run cut off with a record (a Workers limit, a deploy, a
  crash) resumes; only its step in doubt follows the window rule. The
  deadline warning sums the timeout times the attempts of every step on the
  longest path, against the route's deadline.
- **Tests and tools.** Fakes keep keys as the provider does (the same key
  and request return the first answer; a different request returns the
  package's mismatch answer) and gain two scripted answers, `lost` (the
  provider acted and the answer never arrived) and `in_use`. `jadpo test`
  reruns every test that reaches a multi-step action with a crash at each
  step boundary (before sending, after sending but before recording, after
  recording) and checks that the end state and the fake's effects match the
  run without one; this is on by default. `jadpo dev`, `jadpo watch` and
  `jadpo deliveries show` list each step as sent (with its key), answered,
  replayed or in doubt.

```text
$ jadpo deliveries show d_7Q2
billing.subscribe, from subscriber start_subscription
Outcome unknown at step 3 of 4

  1  stripe.customers.create         answered     10:02:11  cus_P4x…
  2  stripe.payment_methods.attach   answered     10:02:12  pm_1N…
  3  stripe.subscriptions.create     no answer    10:02:12  Stripe answered 500 and repeats it for this key until 10:02 tomorrow
  4  stripe.invoices.finalize        not reached

Check in Stripe whether customer cus_P4x… has a subscription.
  It doesn't:  jadpo deliveries retry d_7Q2 --not-done
  It does:     jadpo deliveries abandon d_7Q2, then tidy up in Stripe
```

Calls without idempotency keys (GitHub, Twilio Messages, Postmark) stay
one per action. A receipt sent after a charge belongs in a subscriber of
the action's event, not as an unkeyed last step.

### How the compiler keeps these rules

`jadpo check` keeps these rules with these errors:
[`step.context`](../errors/step.context.md),
[`step.large_output`](../errors/step.large_output.md),
[`step.store_required`](../errors/step.store_required.md),
[`step.too_many`](../errors/step.too_many.md),
[`step.unkeyed`](../errors/step.unkeyed.md),
[`step.late_failure`](../errors/step.late_failure.md),
[`effect.call_in_loop`](../errors/effect.call_in_loop.md) and
[`route.calls_past_deadline`](../errors/route.calls_past_deadline.md). It
reads an operation's `large_output:`. A provider's `idempotency:`
`window:`, `replays_failures:` and `new_key_on:` are read into the plan,
and an operation's `changes:` marks a step. The provider call records,
replays and keys each step through the capability seam, whose exchange
writes a step before it is sent.

The warning counts each point that can end the action for good after its
first changing call as one finding for the action, which you can
decline, unless it is handled with `match`. Those points are an `attempt`
passing on a failure that isn't `unavailable` or `rate_limited`,
`reject`, and `propagate`. A package's action lists on its surface which
failures can follow its own changing call (`after_effect`), and the app's
call of it is the point.

Each route's action sums the `timeout:` times the attempts of every
outside call on its longest path, a provider's operation and an `ai` call
alike, against the route's `deadline:`. A single call is checked the same
way.

### Where else this is covered

- What an operator sees, and the metric and the alert on unfinished
  records: "What an operator sees" and "Several changing calls in one
  action" in [Jobs and events](12-jobs-and-events.md). Each step's state
  shows under its request's or delivery's line in `jadpo dev`.
- `jadpo runs unfinished --build`: [Tooling](14-tooling.md).
- The fakes and crash points: "Provider calls and capabilities in tests"
  in [Testing](13-testing.md).
- The choices behind these rules:
  [the design notes for Jobs and events](notes/12-jobs-and-events.md).

## Receiving from outside
<!-- topic: receiving-from-outside -->

A provider tells your app that something happened in one of two ways: it
sends a webhook, or it puts a message on a queue. Either way, the message
reaches your app as an event, and your subscribers handle it as they
handle your app's own events
([Subscribers](12-jobs-and-events.md#subscribers)). You don't write a
route for either.

### Webhooks as events

A webhook is a request a provider such as Stripe sends your app when
something happens there, such as a payment going through. The package
declares the provider's events and how it signs them, so your app only
gives the connection a webhook secret. With a secret set, Jadpo:

1. generates the route `POST /webhooks/<connection>` in the app's
   `webhooks` API ([Webhooks](07-http.md#webhooks)). You can change its
   path, but you can't switch it off;
2. checks the signature over the raw body, and refuses a timestamp more
   than five minutes old;
3. drops repeats, by the provider's event id;
4. writes each event to the app's outbox, the table that holds events
   until their subscribers run, so subscribers run after the transaction
   commits. The package marks whether each event is public, which decides
   whether your app's subscribers hear it.

A subscriber then takes the event as it takes one of your own:

```jadpo fragment
// domains/payments/subscribers/payment_succeeded.jadpo
subscriber payment_succeeded {
    on: Stripe.PaymentIntentSucceeded
    run: mark_order_paid(event)
}
```

The signature schemes are built in:

- `SignatureScheme.stripe`;
- `SignatureScheme.standard_webhooks`, used by Svix and others;
- `SignatureScheme.hmac_sha256`, with the header that holds the signature
  named;
- basic authentication, for Postmark, which signs nothing;
- Twilio's HMAC-SHA1;
- AWS SNS's certificates.

Every event carries `occurred_at`, when the provider says it happened.
Providers can deliver events out of order. Your app handles that by
ignoring an event older than what it has stored, and a recipe shows how.

A package declares what its provider sends in `provider.jadpo`'s
`webhooks:` block. `check:` names the signature scheme
(`SignatureScheme.stripe`, `.standard_webhooks`, `.hmac_sha256` with
`header:`, `.basic`, `.twilio`, `.slack` or `.sns`). `event_id:`,
`event_type:` and `occurred_at:` say where in the body each of those is,
and `handshake:` names a [handshake](#handshakes) when the provider needs
one. Each event is a type in the package's `events/` folder, with its
`wire_name:` (the name the provider gives that kind of event) and
`occurred_at: Instant`. A connection gives `webhooks: { secret: }`. The
runtime answers the generated route and keeps the events in the outbox for
your subscribers. The details are under
[How webhooks are received](#how-webhooks-are-received) and
[Webhook replies, handshakes and further signature schemes](#webhook-replies-handshakes-and-further-signature-schemes).

Here is a package for Acme Pay, which signs its webhooks as Stripe does,
with one event:

```jadpo check project=e77-webhooks file=app.jadpo hidden
application {}
```

```jadpo check project=e77-webhooks file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e77-webhooks file=shared/providers/acme_pay/v1/provider.jadpo
/// Acme Pay's API, which signs its webhooks as Stripe does.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-pay.example"
        sandbox: "https://sandbox.acme-pay.example"
    }
    credential: CredentialScheme.bearer
    webhooks: {
        check: SignatureScheme.stripe
        event_id: "id"
        event_type: "type"
        occurred_at: "created"
    }
}
```

```jadpo check project=e77-webhooks file=shared/providers/acme_pay/v1/types/payment_id.jadpo hidden
/// Acme Pay's reference for a payment.
public type PaymentId = Text {
    max_length: 40
}
```

```jadpo check project=e77-webhooks file=shared/providers/acme_pay/v1/events/payment_succeeded.jadpo
/// A payment was taken.
public type PaymentSucceeded = Event {
    wire_name: "payment.succeeded"
    occurred_at: Instant
    payment_id: PaymentId {
        wire_name: "paymentId"
    }
    pence: Int {
        min: 0
    }
}
```

A connection to it with
`webhooks: { secret: config.payments_webhook_secret }` gets the route
`POST /webhooks/payments`. `jadpo routes` lists it as `generated webhook`,
with `signature` as its authentication. A subscriber takes its events with
`on: AcmePay.PaymentSucceeded`. The body's `paymentId` and `pence` become
the event's fields, and its `id`, `type` and `created` are read from the
places `webhooks:` names. Every other member of the body is left out, as
it is from a provider's answer to a call.

### Webhooks that need a reply

Some providers wait for an answer in the webhook's own response: Twilio
waits for call instructions, and Slack for a message. For these, the
package's event names its reply type with `reply:`, a type in the
package's `types/` folder. The connection's `webhooks: { replies: { } }`
names the app's action that answers, keyed by the event's name in
lower_snake_case:

```jadpo fragment
// packages/twilio/v2010/events/incoming_call.jadpo
public type IncomingCall = Event {
    wire_name: "voice.incoming"
    reply: CallReply
    from: Text
    to: Text
}

// app.jadpo
phone: {
    package: Twilio.v2010
    domains: [support]
    credential: config.twilio_credential
    webhooks: {
        secret: config.twilio_auth_token
        replies: {
            incoming_call: support.answer_call
        }
    }
}
```

The action takes the event and returns the reply type. It runs inside the
webhook's request, once the signature is checked, and it can read and
write like any action. Its reply is sent as the package's JSON and kept
with the event's id, so a repeat of the same webhook gets the stored reply
and the action doesn't run again. Twilio's own XML format waits for an XML
byte format in a later Jadpo release.

An event that needs a reply isn't kept in the outbox, so no subscriber
hears it: subscribing to one is `subscriber.event_invalid`.

**What a reply may show.** The action runs as the system, with no tenant,
and its reply goes back to whoever sent the webhook. So
[Policy](09-policy.md#field-restrictions) treats the reply as an output to
`Access.public`: everything it carries, or that decided it, must be
something anyone at all may read. That includes whether records are
there, because the system can find every record. A reply that counts or
tests for records only the system may read is refused, and so is a
failure raised because a record is there or not. A failure is sent back
the same way as a reply, so it is held to the same rule. The details are
under
[Webhook replies, handshakes and further signature schemes](#webhook-replies-handshakes-and-further-signature-schemes).

Here Acme Voice signs its webhooks as Twilio does, over a form body, and
waits to be told what a caller hears:

```jadpo check project=e77b-replies file=app.jadpo
use support

application {
    connections: {
        phone: {
            package: AcmeVoice.v1
            domains: [support]
            credential: config.phone_key
            host: config.phone_address
            webhooks: {
                secret: config.phone_auth_token
                replies: {
                    incoming_call: support.answer_call
                }
            }
        }
    }
}
```

```jadpo check project=e77b-replies file=config.jadpo hidden
config {
    phone_address: Url
    phone_auth_token: Text {
        secret: true
    }
    phone_key: Text {
        secret: true
    }
}
```

```jadpo check project=e77b-replies file=shared/providers/acme_voice/v1/package.jadpo hidden
/// Acme Voice's calls API.
package AcmeVoice.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e77b-replies file=shared/providers/acme_voice/v1/provider.jadpo
/// Acme Voice's API, which signs its webhooks as Twilio does.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.bearer
    webhooks: {
        check: SignatureScheme.twilio
        event_id: "CallSid"
        event_type: "CallStatus"
    }
}
```

```jadpo check project=e77b-replies file=shared/providers/acme_voice/v1/types/call_reply.jadpo
/// What a caller hears.
public type CallReply = Object {
    spoken: Text
    hang_up: Bool {
        wire_name: "hangUp"
    }
}
```

```jadpo check project=e77b-replies file=shared/providers/acme_voice/v1/events/incoming_call.jadpo
/// A call is ringing, and waits to be told what to do.
public type IncomingCall = Event {
    wire_name: "ringing"
    reply: CallReply
    occurred_at: Instant
    call_sid: Text {
        wire_name: "CallSid"
    }
    from: Text {
        wire_name: "From"
    }
}
```

```jadpo check project=e77b-replies file=domains/support/entities/phone_log.jadpo hidden
/// A call the desk answered.
entity PhoneLog {
    id: Uuid
    call_sid: Text
    caller: Text
    rang_at: Instant

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
```

```jadpo check project=e77b-replies file=domains/support/actions/answer_call.jadpo migrate=initial
use support/entities { PhoneLog }
use shared/providers/acme_voice

/// Answers a call Acme Voice rings through, noting who rang.
action answer_call(event: AcmeVoice.IncomingCall) -> AcmeVoice.CallReply {
    var noted_call: PhoneLog = PhoneLog.create {
        set: {
            call_sid: event.call_sid
            caller: event.from
            rang_at: event.occurred_at
        }
    }
    return AcmeVoice.CallReply {
        spoken: "Thanks for calling."
        hang_up: noted_call.caller != ""
    }
}
```

A ringing call is answered `200` with
`{"spoken":"Thanks for calling.","hangUp":true}`. If Acme Voice sends the
same webhook again, it gets the same reply without the action running
again.

### Handshakes

Some providers check an address before they send to it: AWS SNS, Slack
and Microsoft Graph. The package names the handshake in `webhooks:`
(`handshake: WebhookHandshake.sns`, `.slack` or `.microsoft_graph`), and
the runtime answers it, so you write nothing for it. New handshakes come
with Jadpo releases.

- **SNS** sends a subscription confirmation, signed as its messages are
  and from the connection's topic. The runtime confirms it by visiting
  its address on AWS's SNS host.
- **Slack** sends a challenge, answered once its signature is checked
  (`SignatureScheme.slack`).
- **Microsoft Graph** sends a validation, which Graph doesn't sign. Its
  token is answered as plain text.

SNS's own messages are checked against the certificate the runtime
fetches from AWS's SNS host over HTTPS, never from a private address, and
keeps. A handshake signed in a way its provider doesn't sign is
`package.provider_invalid`. The details are under
[Webhook replies, handshakes and further signature schemes](#webhook-replies-handshakes-and-further-signature-schemes).

A package receiving an SNS topic's messages:

```jadpo check project=e77b-sns file=app.jadpo hidden
application {
    connections: {
        orders: {
            package: AcmeTopics.v1
            domains: [dispatch]
            credential: config.orders_key
            host: config.orders_address
            webhooks: {
                secret: config.orders_topic
            }
        }
    }
}
```

```jadpo check project=e77b-sns file=config.jadpo hidden
config {
    orders_address: Url
    orders_key: Text {
        secret: true
    }
    orders_topic: Text {
        secret: true
    }
}
```

```jadpo check project=e77b-sns file=domains/dispatch/entities/parcel.jadpo hidden
/// A parcel sent out.
entity Parcel {
    id: Uuid
    reference: Text

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
```

```jadpo check project=e77b-sns file=shared/providers/acme_topics/v1/package.jadpo hidden
/// Acme's order topic, on AWS SNS.
package AcmeTopics.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e77b-sns file=shared/providers/acme_topics/v1/provider.jadpo
/// AWS SNS, which signs each message with a certificate of AWS's own.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.bearer
    webhooks: {
        check: SignatureScheme.sns
        event_id: "MessageId"
        event_type: "Message.type"
        handshake: WebhookHandshake.sns
    }
}
```

```jadpo check project=e77b-sns file=shared/providers/acme_topics/v1/types/shipment.jadpo hidden
/// An order that went out.
public type Shipment = Object {
    order_id: Text
}
```

```jadpo check project=e77b-sns file=shared/providers/acme_topics/v1/events/order_shipped.jadpo migrate=initial
/// An order was shipped.
public type OrderShipped = Event {
    wire_name: "order.shipped"
    occurred_at: Instant
    shipment: Shipment {
        wire_name: "Message"
    }
}
```

The connection's secret is the topic's ARN, the name AWS gives it. SNS's
`Message` member holds JSON written as text, and is read as the JSON it
holds, so `event_type:` and the event's fields can reach inside it.

### Queues

A queue is the other way a provider can send your app messages. Your app
reads a queue directly, through the `receiving` capability, the inbound
twin of `publishing` ([Capabilities](#capabilities)). A queue names the
field that gives each message's type, the field that gives its id, and
the events it may carry. A queue that carries one kind of message lists
one event and no `type_at:`.

```jadpo fragment
// app.jadpo
receiving: {
    warehouse_queue: {
        type_at: "type"
        id_at: "message_id"
        events: [Warehouse.OrderShipped, Warehouse.StockLow]
    }
}
```

Each message becomes its event for your app's subscribers:

- The events come from another Jadpo app's generated package, or your
  app declares them, in a domain's `events/` or its own integration, each
  with its `wire_name:`. The queue's package or driver only knows how to
  fetch messages.
- A message is acknowledged only once the transaction that keeps its
  event, and hands it to the subscribers, has committed. Repeats are
  dropped by message id, which is stored in the database.
- A message whose type isn't listed is parked for an operator, never
  dropped. A message that keeps failing is left to the queue's
  dead-letter queue (a second queue that holds messages that couldn't be
  delivered) and parked. `jadpo deliveries` lists what is parked.

`receiving:` takes `type_at:`, `id_at:` and `events:`. An event of your
app's gives a `wire_name:`, and a package provides the capability in
`provides/receiving.jadpo`. The details are under
[How queues are received](#how-queues-are-received).

**Which queues are packages.** The usual rule decides: a service reached
over an HTTP API is a package, and anything that needs its own protocol or
a platform binding is a built-in driver. So:

- Amazon SQS and Google Pub/Sub pull are packages. The SQS package makes
  its calls in AWS's JSON protocol, signed with AWS Signature Version 4.
- SNS and Pub/Sub push are webhooks, through a package with a handshake.
- Cloudflare Queues is a built-in driver on Workers, a binding whose
  consumer is generated into the Wrangler configuration. From a native
  server it is a package, over its HTTP pull API. The driver is a
  connection with `driver: Driver.cloudflare_queues`, `queue:` and
  `dead_letter_queue:` ([The Cloudflare Queues driver](#the-cloudflare-queues-driver)).
- Kafka becomes a built-in driver in a later release.

### Senders with no package

A system that signs its webhooks but has no package gets your app's own
integration in `shared/providers/`, written in the package format, by
[the golden rule](#the-golden-rule). A partner your company has an
agreement with is a `Service`, calling an ordinary route with a service
key ([Service credentials](08-identity.md#service-credentials)).


## Acting for each user: OAuth grants
<!-- topic: acting-for-each-user-oauth-grants -->

Use a grant when your app acts at another service for each of its users
or organisations: a user's Google Calendar, an organisation's Xero, a
Slack workspace install. Each owner connects their own account, and your
app calls the service with that owner's permission.

```jadpo fragment
// app.jadpo
calendar: {
    package: GoogleCalendar.v3
    domains: [scheduling]
    credential: OAuthGrant {
        owner: User
        client_id: config.google_client_id
        client_secret: config.google_client_secret
        scopes: ["calendar.events"]
        return_to: "/settings/integrations"
    }
}

// domains/scheduling/actions/add_to_calendar.jadpo
attempt calendar.events.insert(event, on_behalf_of: booking.organiser_id) {
    grant_missing: CalendarNotConnected
    grant_revoked: CalendarNotConnected
}
```

### The grant is the connection's credential

A connection's credential may be a grant owned by one of the app's
records. The owner is `User` or any entity, such as `Organisation`. The
provider's OAuth facts (its authorize, token and revoke addresses) are
`oauth:` in the package's `provider.jadpo`, and each operation declares
the scope it needs (`scope: "calendar.events"`). Nothing is specific to
one provider, and the connection's name is the app's.

### Connecting

Connecting is two generated endpoints, not pages:

1. `POST /connections/<name>/start`, called by the signed-in caller,
   returns the provider's consent address. It stores the OAuth `state` on
   the server, against the caller's session or API token.
2. The provider sends the person back to the generated
   `GET /connections/<name>/callback`. It finds the `state`, exchanges the
   code with PKCE, stores the grant, and redirects to the frontend's
   `return_to` with success or failure.

The callback needs no cookie, so the app's own API-token clients connect
the same way.
[Acting for a user at another service: OAuth grants](08-identity.md#acting-for-a-user-at-another-service-oauth-grants)
covers how this touches sign-in.

### Where grants are kept

The runtime holds grants, as it holds sessions, in a `jadpo_grants`
system table in the owner entity's store, so grants need a database.
Their tokens are encrypted with
`application { grants: { encryption_key: config.grant_encryption_key } }`.
That key is a secret, read and rotated as signing keys are, with its
previous key kept ([Signing keys](10-configuration-and-time.md#signing-keys)).
No code can read a grant.

### Calling with a grant

Every call names whose grant it uses: `on_behalf_of: owner_id`.
`grant_missing` and `grant_revoked` are failures the app maps, as the
example maps both to `CalendarNotConnected`.

Tokens are refreshed for you: renewed before they expire, once when
several calls race, and a rotated refresh token kept. A revoked grant
gives `grant_revoked` and a `GrantRevoked` event, so the app can ask the
owner to connect again.

Scopes are checked when you compile: calling an operation whose scope the
connection doesn't request is an error.

### Who may connect

[Policy](09-policy.md) decides who may connect for an owner. Connecting
and disconnecting need the `update` grant on the owner's record, so
grants bring in no new kind of permission.

### Disconnecting

The generated `POST /connections/<name>/disconnect`, and its operation,
disconnect a grant, revoking it at the provider when the package declares
how. Erasing a person or a record ([Erasure](15-personal-data.md#erasure))
deletes and revokes its grants.

### In development and tests

Calls are faked by default. `jadpo dev grants add <connection> --owner <id>`
makes a fake grant, and tests declare grants as fixtures.

### Later

Each of these comes when an app needs it: several accounts per owner,
scopes added after connecting, provider notifications per grant, and
GitHub App installation tokens (a credential scheme in the runtime).

> OAuth grants aren't built yet ([What is built](#what-is-built)).

## Packages from other Jadpo apps
<!-- topic: packages-from-other-jadpo-apps -->

Your app can call another Jadpo app's API through a package made from
that app's OpenAPI document, exactly, with nothing to correct by hand.

### What the document carries

Jadpo's OpenAPI documents carry `x-jadpo` fields that say what plain
OpenAPI can't: `x-jadpo-changes` (the operation changes something),
`x-jadpo-idempotency`, `x-jadpo-failure` (each failure's `code` and
`kind`), `x-jadpo-constraints` and `x-jadpo-events`
([OpenAPI](07-http.md#openapi)). Other tools ignore them. Every API serves
its document at `<prefix>/openapi.json` by default.

The OpenAPI importer uses these fields whenever it finds them, so
importing a Jadpo app's document gives an exact package that needs no
corrections or recordings. A provider that isn't a Jadpo app is imported
from its plain OpenAPI. There is one importer and one package format.

### Getting the package

- **In one repository**, `jadpo package add ../orders` reads the other
  app's generated OpenAPI.
- **Across repositories**, the other app's package comes from git, like
  any private package.
- **For another company's public API**,
  `jadpo package add https://api.orders.example/v1` fetches the document
  it serves.

Each is read once, when the package is added or updated, then pinned in
the lock with its source and digest.

### Connecting to it

The result is an ordinary installed provider package in
`packages/<name>/<version>/`. It has no fixed host, so the connection
gives `host:`, and its credential is a service key
([Service credentials](08-identity.md#service-credentials)):

```jadpo fragment
// app.jadpo
orders: {
    package: Orders.v1
    domains: [fulfilment]
    host: config.orders_url
    credential: config.orders_service_key
}
```

At runtime the two apps meet only over HTTP, and the called app can't
tell what made the call.

### When the other app changes

An additive change to the other API regenerates as a minor version
through `jadpo package update`, and `jadpo check` says when yours is out
of date. A breaking change, which already needs an acceptance in the
other app ([The public API record](07-http.md#the-public-api-record)),
becomes a new major version, installed side by side while callers move.

In one repository, `jadpo check` in the called app reads the other apps'
locks and reports which of their calls a change breaks, before it merges.

> Reporting breaks across apps isn't built yet.


## Capabilities
<!-- topic: capabilities -->

A **capability** is a job Jadpo does itself, such as storing files or
sending email, done through a connection you choose. Your code uses the
capability's one API, never the provider's, so changing provider changes
`app.jadpo` and never your domain code.

A capability exists only for a job Jadpo performs itself, and it covers
that job whole. Jadpo owns the list. Every connection that fills a
capability does all of it, so there are no optional parts and partial
support can't arise.

| Capability | Code uses | Built-in drivers | Packages |
|---|---|---|---|
| `database` | Entities ([Data](05-data.md)) | PostgreSQL; D1 later | No |
| `cache` | `caches:` ([Caches](05-data.md#caches)) | Redis (Upstash, Valkey and ElastiCache through it, on Workers too); in memory, a warning in production and an error on Workers | No |
| `files` | `File` fields, `store.read` and `store.put` ([Files](16-built-in-services.md#files)) | S3 (R2, B2, MinIO and the rest through its settings); disk, native only, one server | Only one that does the whole job |
| `email` | `type X = EmailMessage`, emitted, and Jadpo's own emails ([Email](16-built-in-services.md#email)) | SMTP; the local inbox | Yes |
| `publishing` | Chosen public events sent outward after commit; outgoing webhooks | The local log; outgoing webhooks | Yes |
| `receiving` | Queue messages arriving as events | Cloudflare Queues on Workers; Kafka later | Yes (SQS, Pub/Sub pull) |
| `schedules` | `schedule` ([Schedules](12-jobs-and-events.md#schedules)) | Timer; Cloudflare cron; external tick | No |
| `search` | `search:` on an entity ([Search](16-built-in-services.md#search)) | PostgreSQL full-text search, the default | Yes (hosted engines) |
| `bot_check` | `bot_check: true` on a route or generated feature ([Bot protection](16-built-in-services.md#bot-protection)) | A local driver passing every check in development and tests | Yes (Turnstile, hCaptcha, reCAPTCHA) |
| `documents` | `type X = Document` rendered to HTML or PDF ([Documents](16-built-in-services.md#documents)) | None: tests fake it | Yes (Gotenberg as a service, DocRaptor, PDFShift) |
| `scanning` | Uploads scanned before they can be attached ([Built-in services](16-built-in-services.md)) | None | Yes (a ClamAV service, a scanning API); built when an app needs it |
| `ai` | Prompts and `ai.generate`, embeddings ([AI](21-ai.md)) | OpenAI-compatible servers, for local models | Yes (`provides/ai.jadpo`: Anthropic, OpenAI and the rest) |

Some things are not capabilities:

- **Event delivery** stays inside the runtime, from the outbox in the main
  store, so logs and traces stay in one place and no queue service is
  needed.
- **Telemetry** has its own section in `application { }` and one
  protocol, OpenTelemetry ([Telemetry](16-built-in-services.md#telemetry)).
- **Feature flags, rate limits, sessions and idempotency keys** each name
  a store.
- **SMS**, because nothing in the language sends SMS. Twilio is a
  package, and its text is still typed against translations. `sms` comes
  back as a capability if SMS sign-in codes are built.

### One `connections:` block

Every connection is an entry in one `connections:` block, and each entry
is a built-in driver or a package. Where a connection is used says which
capability it serves. One connection is one resource, such as a bucket or
a queue, not one vendor account, as Cloudflare's own bindings are.

- Entities, caches and `File` fields name a connection with `store:`.
- `email:`, `publishing:`, `receiving:`, `documents:` and `bot_check:`
  name one in `application { }`.
- `search:` names a search engine's connection, and PostgreSQL when it's
  left out.
- Schedules use the target's own tick unless told otherwise.

Using a connection for a capability its package doesn't provide is a
compile error.

```jadpo fragment
// app.jadpo
application {
    connections: {
        postmark: {
            package: Postmark.v1
            domains: [marketing]
            credential: config.postmark_token
            environment: config.postmark_environment
            message_stream: "outbound"
        }
    }
    email: {
        connection: postmark
        from: config.mail_sender
    }
}
```

### What a provider does beyond the capability

Everything specific to one provider is the package's own operations,
called from an action or subscriber in a domain the connection lists, as
payments are. Examples are hosted templates with variables
(`postmark.email.send_with_template(...)`), suppression lists and
marketing sends. One connection serves both: `email:` uses it, and the
listed domains call it.

Bounces and complaints are the package's own webhook events
(`Postmark.BounceWebhook`), so moving to another provider means rewriting
the subscribers that read them. They become part of the capability only
when a built-in feature uses them.

Mail sent through a hosted template gives up the dev inbox entry, the
health score and the automatic template check. An app can add a contract
case to check it instead.

### How a package provides a capability

A file `provides/<capability>.jadpo`, found by its name, maps each
standard operation of the capability onto one of the package's
operations:

```jadpo fragment
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
            Attachments: {
                each: message.attachments
                as: {
                    Name: item.name
                    Content: item.content
                    ContentType: item.content_type
                }
            }
        }
        receipt: response.MessageID
        failures: {
            inactive_recipient: EmailFailure.recipient_rejected
        }
    }
    limits: {
        attachments: 10mb
    }
}
```

- **Mappings stay data**: field paths, literals, lists, nested blocks,
  enum tables, and `each:` with `as:` to map a list element by element,
  the current element named `item`. A mapping has no expressions or
  calls. Any computing is done in the package's decoders and encoders.
  The compiler checks both sides of a mapping.
- **Email attachments are part of the capability.** A `File` field on an
  `EmailMessage` is an attachment. It is read from its store when the
  message is sent, and checked against the mapping's
  `limits: { attachments: }`, the total size the provider accepts. A
  provider that can't send attachments stays a package, but can't be the
  email connection.
- **Emails without an idempotency key.** Postmark, SES, SendGrid and
  Mailgun take no idempotency key, so an email whose sending ends in
  `outcome_unknown` is parked rather than sent again
  ([Delivery: at least once, safe to repeat](12-jobs-and-events.md#delivery-at-least-once-safe-to-repeat)).
  Sending two kinds of mail through two connections waits for an app that
  asks for it.
- **Image variants**, once built, are made by the runtime over any store,
  so `files` stays whole. Media services such as Cloudinary and Uploadcare
  stay ordinary packages, since they can't do the whole `files` job.

For `email` ([Email](16-built-in-services.md#email)),
`provides/email.jadpo` is read and checked against the package's
operation and settings (`package.provider_invalid`). Its `input:` keys
are the operation's input field names, as `provide receiving`'s are, with
the wire names coming from the types rather than written out as the
example above writes them. `email:` may name the connection, which then
needs no `domains:`. Each delivery calls the mapped operation with the
rendered message, and its failures are mapped onto `EmailFailure`'s. The
Resend package (Resend's `POST /emails`) is the second email connection
beside SMTP: moving between them changes `app.jadpo` and its settings
only.

### Erasing a person at a provider

When a person is erased, a provider that holds their data has to erase
it too. This isn't a capability mapping:

1. The package declares its erasure operation in `provider.jadpo`:
   `erasure: { call: customers.delete  by: CustomerId }`.
2. The connection names the app's field that holds the provider's key for
   the person: `erasure: { key: User.stripe_customer_id }`.
3. Jadpo copies each key into the erasure event, and the app erases the
   person in one transaction.
4. After commit, a generated subscriber calls each provider, retried
   until the provider confirms.

The erasure-completed email ([Erasure](15-personal-data.md#erasure)) goes
only once every provider has confirmed. A delivery that gives up is
parked for an operator and shown, never left quietly incomplete. A copied
key that is personal data, such as an email address, stays only in the
outbox until it is delivered, and is listed in the personal-data report.

A connection that receives personal data but can't erase it is a compile
error, unless an acceptance ([Acceptances](18-records-and-manifest.md#acceptances)),
`ErasureNotPossible`, states why the data stays, such as tax records. It
is listed in the personal-data report. The app may add its own
`PersonErased` subscribers.

> Erasure at providers isn't built yet: it comes with
> [Personal data](15-personal-data.md).

### Proving a package works

Each capability has a conformance suite, a set of checks any package
filling it must pass. `jadpo package conform <package> <capability>` runs
it against the provider's sandbox or an emulator. The registry lists a
package for a capability only with a passing run pinned to the package's
digest, and the app's own integrations run the suite in scheduled CI.
Tests use one fake per capability, whatever the provider: `email.sent`
for email, `published` for publishing.

For `ai` ([AI](21-ai.md)), `jadpo package conform <package> ai` runs the
suite in an app. By default it runs against the package's recordings and
a local fake on this machine's loopback address. With `--live` it runs
against the provider's API through the app's own connection, and it is
refused (`conform.refused`) when there is no connection or its credential
isn't set.

> Only the `ai` suite is built. Other capabilities' suites, the run pinned
> to the digest for the registry, and the scheduled CI run aren't built
> yet.

### In development

In `jadpo dev` nothing leaves the machine: published events go to a log
in the dev console, search goes to PostgreSQL whatever production uses,
and the cache runs in memory. Where the other connections go in
`jadpo dev`, in tests and on each target is the table in
[How packages, connections, capabilities and services fit together](#how-packages-connections-capabilities-and-services-fit-together).

### Reports

- The manifest records each connection's capabilities, hosts, services
  and targets.
- The network report labels each host with its connection and
  capability.
- The personal-data report lists the connections that receive marked
  fields ([Where personal data goes](15-personal-data.md#where-personal-data-goes)),
  which of them can erase, and the acceptances for those that can't. Which
  can erase isn't built yet.

### Targets

A target is where a deployable runs, natively or on Cloudflare Workers.
Each driver and package says which targets it runs on, and `jadpo check`
refuses a deployable that uses one elsewhere, naming what to use instead.

- A package that calls an HTTP API runs on every target.
- A service (`runs:`) runs only beside a native deployable, so on Workers
  its connection gives `host:`.
- Cloudflare Workers can open TCP sockets but block port 25, so SMTP
  works there only on port 587.
- Workers has no timer of its own, so its schedule tick is Cloudflare
  cron.
- R2 and Queues bindings exist only on Workers.
- The disk driver and an in-memory cache don't run on Workers.

A deployable ([Declared deployables](14-tooling.md#declared-deployables))
that declares a target is refused each connection it reaches whose driver
that target can't run, with `deployable.target_unsupported` naming what
to use instead. On Workers that is the disk, a service Jadpo runs without
`host:`, and SMTP on port 25. Natively it is a Cloudflare queue, which a
deployable reaches when it runs `Trigger.subscribers`, where the queues
`receiving:` reads are received.

What a deployable reaches is followed from the routes of the APIs it
serves and the background work it runs, through everything they call. An
app that declares no deployables, and a deployable with no `target:`,
have no target, so nothing is refused for them
([Tooling's design notes](notes/14-tooling.md)).

> An in-memory cache is checked against the target once caches are built.

### Choosing a provider when deploying

An app that other people deploy, such as an open-source product run by
many operators, declares every connection it supports, and a setting
picks one when the app starts:

```jadpo fragment
// app.jadpo
email: {
    connection: config.mail_provider
    from: config.mail_sender
}
```

Every candidate connection is compiled, checked and reported, and one
that isn't chosen needs no credential. Sending different messages through
different connections, and falling back to a second provider, come later,
and only when an app asks for them.

> Choosing a provider when deploying isn't built yet. It is built when
> the first app that other people run needs it.


## Provider calls in development
<!-- topic: provider-calls-in-development -->

In `jadpo dev`, every call to a provider is faked by default, so nothing
leaves your machine and you need no credential. You let chosen calls
through to the provider's sandbox when you want real answers, and never
to the live service.

### Faked answers

A faked call is answered, in this order, by:

1. what `jadpo calls script` set for it (below);
2. the operation's recording, when the package keeps one, its
   placeholders filled with generated values of the right types (logged
   as `recording`);
3. the operation's generated default, a valid value of its output type
   ([Tests and fakes](#tests-and-fakes)).

A provider connection's setting that nothing gives (its credential,
environment or address) gets a stand-in value, so the app starts with no
credential set.

### Letting calls through to the sandbox

Name the operations or whole resources that go to the provider's sandbox
in `development: { sandbox: }` on the connection, where review and agents
see it. Each name is an operation, `resource.operation`, or a whole
resource, and is checked against the package
(`connection.value_invalid`):

```jadpo check project=e72d-development file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
```

```jadpo check project=e72d-development file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72d-development file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
    error_code: "error.code"
}
```

```jadpo check project=e72d-development file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
public resource bookings {
    create: {
        method: HttpMethod.post
        path: "/v1/bookings"
        input: CreateBooking
        output: Booking
        changes: true
        idempotent: true
        failures: [SlotTaken]
        source: PackageSource.written
        docs: "https://docs.acme-couriers.example/bookings"
    }
}
```

```jadpo check project=e72d-development file=shared/providers/acme_couriers/v1/types/postcode.jadpo hidden
/// Where a collection is.
public type Postcode = Text {
    max_length: 8
}
```

```jadpo check project=e72d-development file=shared/providers/acme_couriers/v1/types/booking_id.jadpo hidden
/// The courier's reference for a booking.
public type BookingId = Text {
    max_length: 40
}
```

```jadpo check project=e72d-development file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Postcode
}
```

```jadpo check project=e72d-development file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: BookingId
}
```

```jadpo check project=e72d-development file=shared/providers/acme_couriers/v1/failures/slot_taken.jadpo hidden
/// The slot went before the booking arrived.
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "That slot is taken."
}
```

```jadpo check project=e72d-development file=domains/shipping/failures/slot_gone.jadpo hidden
/// The slot the customer chose has gone.
type SlotGone = Failure {
    kind: FailureKind.conflict
    message: "That slot has gone; choose another."
}
```

```jadpo check project=e72d-development file=domains/shipping/actions/book.jadpo hidden
use app { couriers }
use shipping/failures { SlotGone }
use shared/providers/acme_couriers

/// Books a collection with the courier.
action book(postcode: AcmeCouriers.Postcode) fails SlotGone -> AcmeCouriers.Booking {
    return attempt couriers.bookings.create(
        AcmeCouriers.CreateBooking {
            postcode: postcode
        }
    ) {
        slot_taken: SlotGone
    }
}
```

```jadpo check project=e72d-development file=app.jadpo
application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            environment: config.couriers_environment
            development: {
                sandbox: [bookings.create]
            }
        }
    }
}
```

The named calls go to the sandbox once the sandbox credential is set in
`.env.local`. Until then the connection stays faked, and `jadpo dev`
prints one line naming the setting to set. Only the credential is per
developer; everything else about where calls go is in `app.jadpo`.

A connection whose environment is `live` stops `jadpo dev` starting, or
a save reloading, with `cli.dev_start_failed`. Calls never go to the live
service from `jadpo dev`.

`--sandbox` and `--fake` on `jadpo dev` change one session only, and
nothing about them is kept: `--sandbox couriers.pickups.create` (or a
resource) lets a call through, and `--fake couriers` fakes a connection.

### Scripting the next answer

`jadpo calls script <operation> <failure>` sets the next answer for an
operation, as a test's `fakes:` does
([Fakes for integrations](13-testing.md#fakes-for-integrations)), so an
agent can walk a failure path by hand. Fakes keep no state: a flow that
creates an object and then reads it either lets both calls through or
scripts the read.

### The call log

Every call is logged, faked or real, with its connection, operation,
inputs and answer in full (in development only, and never a secret), its
source (`recording`, `generated`, `scripted` or `sandbox`), attempts,
duration and request id. Read it with `jadpo calls` (`--connection`,
`--operation`, `--request`), `jadpo calls show <id>`, `jadpo trace` and
`--json`. Calls are kept with the last 500 timelines in `build/dev/`,
never committed. [Debugging and review](17-debugging-and-review.md)
otherwise hashes the text it shows; call payloads in development are the
one exception. Production keeps telemetry spans with no values
([Telemetry](16-built-in-services.md#telemetry)).

Each request's line in `jadpo dev` lists its calls:

```text
jadpo dev, started 07:34:33
listening on http://localhost:3918
    routes     2 routes
    settings   2 checked
    providers  couriers faked
serving: saves reload, press ctrl-c to quit
req_58d4  POST /pickups                      201   9 ms   127.0.0.1
  └─ couriers.pickups.create · generated · answered
req_9588  POST /pickups                      409 pickup_slot_gone   1 ms   127.0.0.1
  └─ couriers.pickups.create · scripted · slot_taken
```

How each part works is under
[Fakes and the development call log](#fakes-and-the-development-call-log).

> `jadpo trace` and the timelines aren't built yet. The call log keeps
> the last 500 calls.

## Compile time
<!-- topic: compile-time -->

A provider's description doesn't slow your build, however large it is,
because the compiler never reads it. Descriptions are large, by the
`Content-Length` of the raw files:

| File | Size |
|---|---|
| Stripe `openapi/spec3.json` | 8.3 MB |
| AWS S3 Smithy model | 3.0 MB |
| AWS EC2 Smithy model | 6.7 MB |

AWS publishes about 400 such models, one per service. Parsing 8 MB takes
milliseconds. The cost is resolving, checking and generating thousands of
schemas on every build and every keystroke.

So an importer reads a description once, and writes the package. On a
build, the compiler reads each package's index and `provider.jadpo`,
finds each resource the app calls by its file name, and loads only those.
It caches the check per package digest, so an unchanged package costs
nothing after the first build. An app using S3 and SES loads one driver
connection, one small package and a handful of files from it.

## Tests and fakes
<!-- topic: tests-and-fakes -->

In `jadpo test`, every package has a generated fake, so tests never call
a provider. An operation answers with a valid default built from its
output type. A test scripts a response or a failure for one call with
`fakes:` ([Fakes for integrations](13-testing.md#fakes-for-integrations)),
and the test host puts one fake in every provider connection's place,
answering each operation's scripted answers in order:

```jadpo fragment
test "a taken slot is reported" {
    fakes: {
        couriers.bookings.create: [slot_taken]
    }
    ...
}
```

Responses still pass through the real retries, decoding and mapping, and
package actions run for real over the faked operations.

The compiler checks every scripted response against the package's types,
so a fake can't return a shape the provider never would. Tests script the
clean type, never the shape on the wire. Fakes keep idempotency keys as
the provider does, and accept `lost` and `in_use` as scripted answers
([Several changing calls in one action](#several-changing-calls-in-one-action)).
Grants are declared as fixtures.

`jadpo test --kind contract` runs the same cases against the provider's
sandbox, in CI on a schedule. Every written operation that isn't
confirmed by a recording must have a case in that run.

> Grants as fixtures and the contract run aren't built yet.

## What reviewers see
<!-- topic: what-reviewers-see -->

The approval report
([Reviewing a change](17-debugging-and-review.md#reviewing-a-change))
lists, for each change:

- operations newly called, and the domains calling them;
- data newly sent to a provider, with personal fields flagged
  ([Marking personal fields](15-personal-data.md#marking-personal-fields));
- package installs and upgrades, as the surface diff of what the app
  uses, hosts and credential schemes included
  ([Package upgrades](18-records-and-manifest.md#package-upgrades) says
  which need a person);
- services: a new or changed image, which needs an acceptance before it
  runs, and new `outbound:` grants, which need one too;
- new outside hosts, each needing an acceptance;
- third-party packages from git, marked unverified, which need an
  acceptance to install;
- connections that receive personal data but can't erase it, with their
  acceptance;
- written operations not yet confirmed.


## How the package format is read
<!-- topic: how-the-package-format-is-read -->

This is the detail behind [The package format](#the-package-format):
exactly how Jadpo reads a package's folder, its names and the lock. Read
it when you write a package by hand or a check refuses one.

- **A package's name is its folder's.** `packages/acme_couriers/v1/` is
  `AcmeCouriers.v1`: the folder's name in `UpperCamelCase` and its version
  folder, which the index has to give (`package.name_mismatch`). A version
  label is `v` and digits in groups joined by single underscores, `v2` or
  `v2024_06_20`; the lock sorts the groups as numbers.
- **What a package's code sees.** Only its own declarations, bare, and the
  built-ins: not the app's `shared/`, domains, connections or other
  packages, since an installed package is published apart from any app and
  the app's own integration is written so that it could be. Its files need
  no `use` lines. It doesn't read the app's `shared/`; whether it should
  is an open question in the design notes, and allowing it later breaks
  no package.
- **Bringing a package in.** `use packages/<name>` for an installed one and
  `use shared/providers/<name>` for the app's own bring the whole package
  in, as `use <domain>` does; there is no `{ ... }` form, since its names
  are always written through it. `jadpo fmt` sorts these lines after the
  domains'.
- **Names through a package.** A type (`Postcodes.UkPostcode`), a failure
  (`fails Postcodes.InvalidPostcode`), a type built from a value
  (`Postcodes.UkPostcode(text)`) and a public library's function
  (`Postcodes.uk.parse(text)`) are all written through the package's
  name.
- **Its failures.** A package's failure is known through its package in
  every `fails` list and outcome, and its code is the package's, not the
  app's: it isn't checked against the app's failure codes.
- **The index.** A `///` description is required, as are `licence:` and
  `jadpo:`; `jadpo:` is a major and minor number as text, compared with
  nothing until Jadpo has a language version to compare it with.
  `Licence.proprietary` is refused for an installed package whose lock line
  isn't a `git` source; the app's own integrations may use it.
- **`provider.jadpo`.** Its top-level keys are checked, with `hosts:`'
  (`live:`, `sandbox:`), `idempotency:`'s (with `window:` required) and
  `runs:`'s keys; the others' values are read where they are used.
  `protocol:` is required, and `hosts:` with `runs:` is
  `package.provider_invalid`.
- **The kind.** A package with `resources/` needs `provider.jadpo`
  (`package.provider_missing`); a provider package carrying
  `translations/` is `package.provider_translations`.
- **The digest.** `sha256:` and the SHA-256 of one line per file,
  `<the SHA-256 of its bytes>  <its path>`, paths relative to the package's
  folder in byte order, as Go's `dirhash` does; every file in the folder
  counts, hidden ones left out, so a changed, added or removed file is
  `package.digest_mismatch`.
- **What isn't compiled.** An installed package's tests (run when it is
  published, not in the app's `jadpo test`), its `recipes/` and `sample/`
  are read only for its digest. The app's own integration's tests run with
  the app's.
- **Reserved names.** `Stripe`, `Twilio` and `GitHub`, the curated packages
  this page names, compared by folder without underscores; only the app's
  own integration is refused one.
- **A name the app uses.** A package's folder name may not be a domain's or
  a library's, its name in code may not be a declaration's, and an
  installed package and the app's own integration never share a name
  (`package.name_taken`).
- **The lock.** `jadpo check` reads `jadpo.lock` and the committed folders
  only: each installed package needs its line (`package.not_locked`), each
  line other than a `link` its package (`package.not_installed`). A `link`
  line names a checkout, which every check compiles in the installed
  folder's place ("How packages are installed and linked").

> Capability mappings other than `receiving`'s and `email`'s aren't built
> yet: one is `toolchain.not_built_yet` where it is declared.

## How the surface and the publishing bar work
<!-- topic: how-the-surface-and-the-publishing-bar-work -->

This is the detail behind [The surface](#the-surface) and
[Developing and publishing a package](#developing-and-publishing-a-package):
what a package's surface record holds, how its version is worked out, and
what `jadpo package check` and `jadpo package show` do.

- **The surface's entries.** In the record format's order: the package;
  for a provider, `provider`; each operation of a `public resource`; each
  function of a `public library`; each `public` type (`type Name =
  Base`), then each `public` failure; each folder in `recipes/`. Each
  `public` action follows the operations ("Package actions and recipes");
  its public messages, `message translations.<message>` with their
  fields, follow the functions, and the package's entry has a `locale`
  line for each language it words
  ([Packages' text in an app](10-configuration-and-time.md#packages-text-in-an-app)).
  A library itself has no entry: its functions do. Declarations without
  `public`, and test files, are left out.
- **The package's entry.** `kind` is `provider` when the package has
  `provider.jadpo`; `licence` is the `Licence` variant's name (`mit`), as
  `protocol json_http` and `credential bearer` are written; `jadpo` is the
  index's text; `guide GUIDE.md` when the package carries one; `version`
  when it is known (below), and left out of an own integration's surface.
- **The provider's entry** holds `protocol`, `host live` and `host
  sandbox`, `runs image`, `port` and each `asks` host, `credential`, and
  `idempotency header` and `window`.
- **Facts written as Jadpo writes them.** A type's own rules are fields
  sorted by name (`max_length: 8`); an object's fields are `name: Type`
  sorted by name, a field's own rules in braces after its type (`Text {
  max_length: 4 }`) and `default` after it; an enum's variants are
  `variant` facts sorted, a variant's fields indented under it; a
  function's `signature` is its parameters and result without its
  failures, each failure a `fails` line; an operation's `request` is its
  method and path, then `input`, `output`, `output_at`, `changes`,
  `idempotent`, `fails`, `source` (`written` or `imported`, with its
  documentation page) and `confirmed` when `recordings/<resource>.<operation>.json`
  exists. Names are written as the package writes them, bare.
- **Where surfaces live.** `jadpo package check` writes an installed
  package's, in its own repository, beside `package.jadpo`; in an app it
  is held by the package's digest, so `jadpo check` doesn't regenerate it
  and `jadpo records` doesn't list it (a newer Jadpo writing surfaces
  differently would otherwise fail every app on code it can't change). `jadpo
  check` writes each own integration's to
  `build/providers/<name>/<version>/surface.jadpo_record`.
- **`jadpo package check`** takes the package's folder, by default the one
  it runs in, which must hold `package.jadpo`. It copies the package into a
  scratch app as its own integration (the golden rule), so the curated
  packages' reserved names are allowed there, and shows every path
  relative to the package's folder. Its bar is a row per item, each passed,
  failed or not checked yet; the surface is written when the package
  compiles and its version is allowed, even while another item fails, so
  the surface diff is there to read.
- **The version.** A release is a git tag `v<version>` whose committed
  `surface.jadpo_record` names the package. With none, the first release
  is the major version the label names (`v2` is 2.0.0, `v0` is 0.1.0, a
  dated label such as `v2024_06_20` 1.0.0). When nothing in the package's
  folder changed since the newest release, its version is that release's.
  Otherwise the surface is compared with that release's, entry by entry:
  a removed entry, or an entry whose facts changed (a new `fails`, a
  field retyped or added, a variant added, a licence changed), breaks an
  app and is refused; only new entries, or a new `jadpo` or locale in the
  package's entry, is the next minor version; nothing but descriptions,
  files, sources and recordings is the next patch. A breaking change is a
  new package (`Postcodes.v3`), not a new number. Outside git, every
  check is a first release.
- **The bar's tests** are the package's own `.test.jadpo` files, run in
  the interpreter in the scratch app.
- **`type.standard_exists` in a package** is held to its public types
  only: a type the package keeps to itself is what it decodes the
  provider's shape into before a value leaves. The words are a package's:
  convert at the edge, in the package.
- **`jadpo package show`** finds a package by its folder's name
  (`postcodes`), its name in code (`Postcodes`) or with its version
  (`Postcodes.v2`), and a declaration by the name the app writes after
  the package's (`uk.parse`, `UkPostcode`, `bookings.create`). An
  installed package's version and source come from its lock line; the
  app's own integration's source is "the app's own". Lists cut a long
  line with `…`; the declaration's own screen and `--json` hold it whole.

> Polls and capability mappings aren't in the surface yet, and settings,
> webhooks, polls and erasure aren't in the provider's entry yet.
> `jadpo package new` and `publish` aren't built yet: they say
> `toolchain.not_built_yet`.

## How packages are installed and linked
<!-- topic: how-packages-are-installed-and-linked -->

This is the detail behind [Installing safely](#installing-safely): what
`jadpo package add`, `update`, `remove`, `link` and `unlink` do, and what
each refuses.

- **Where `add` installs from.** A folder holding `package.jadpo` is a
  `path` source, locked relative to the app's root (`../postcodes`);
  `<repository>@v<version>` is a `git` source. A repository written
  `host/owner/name` is fetched over HTTPS, a URL (`https://`, `ssh://`,
  `file://`, `git@host:`) as written, and a folder on this machine
  (starting `.` or `/`) from its path, locked relative to the root. Git
  runs with the developer's or CI's own access and never prompts. A Jadpo
  app's folder (another app's OpenAPI) and a bare name (the registry)
  are in the note at the end. A source whose path has a space is
  refused, since the lock writes each as one word.
- **A git release.** The repository is cloned with nothing checked out,
  the tag's commit is checked out, and the package is at the repository's
  root. The lock records the repository as written, the tag and its
  commit. A tag the repository lacks names the tags it has
  (`install.fetch_failed`).
- **Verification at install.** The package is compiled as the app's own
  integration in a scratch app (the golden rule), and must have no errors;
  the surface its code gives at the version its committed
  `surface.jadpo_record` names has to be that file byte for byte, and for
  git that version has to be the tag's (`install.surface_mismatch`). Only
  then are its files copied, leaving out hidden ones (`.git`) and a
  `build/` at its root, through a hidden folder beside the target so a
  failed copy leaves the old one, and its digest is worked out as "How the
  package format is read" says.
- **What `add` refuses.** A package already in the lock (`update` moves
  it); a package whose name the app's own code uses, which is installed,
  checked and undone, showing `package.name_taken`. A package from git
  installs, and `jadpo check` then needs a person's acceptance of it
  (`PackageChange.git_source`).
- **`update <name>`.** A `path` package is verified again from its folder;
  a `git` one moves to the newest `v<version>` tag whose committed surface
  names the same package, never to an older one, and a folder or tag now
  holding another major version is refused, since that is a new package.
  It prints the review
  [Package upgrades](18-records-and-manifest.md#package-upgrades)
  describes: each surface entry added, changed or removed, `used by the
  app` when the app's code writes its name through the package
  (`Postcodes.uk.tidy`), and `needs a person` for a changed or removed
  one the app uses. A linked package is unlinked first. An installed
  folder changed by hand (`package.digest_mismatch`) is installed again
  from its source, at the release the lock names when there is none
  newer, so `update` is also how a changed package is put back.
- **`remove <name>`.** Refused while a file of the app's has `use
  packages/<name>` (`install.in_use`), naming each; otherwise the folder
  and its lock line go, and the lock stays, possibly empty. A linked
  package is unlinked first.
- **`link <folder>`.** The lock's line becomes `link <path>`, the path
  relative to the root, and the line it replaced is kept in
  `.jadpo/links` (in the lock's format, `.jadpo/` git-ignored) for
  `unlink`. The installed folder stays. Every load reads the checkout's
  files, without hidden ones or its `build/`, in the installed folder's
  place, so `jadpo check`, `test` and `dev` compile the app against it;
  `jadpo check`, `link`, `test` and `dev` show its problems at the
  checkout's paths. A checkout with no
  `package.jadpo` can't be read. `link` itself shows the app's check
  against it. `jadpo dev` and `jadpo test --watch` watch the checkout as
  they run ("A package's names, and watching a linked checkout", below).
- **Where a link is refused.** CI is the `CI` variable set to anything but
  empty, as CI services set it and as `jadpo suggest share` and the
  terminal's animation already read it; there `jadpo check` and `jadpo
  test` refuse each link (`install.link_refused`).
  [Tooling](14-tooling.md) names no release or production profile, and
  every production build goes through `jadpo build`, which refuses a link
  whatever the environment.
- **`unlink <name>`.** Puts back the line `link` kept; with none (a
  package linked before it was installed), the lock no longer names it and
  the hint says to add it.
- **Examples.** A ```` ```jadpo ```` block in the `///`
  description of any public declaration; other fences are prose. Each runs
  as a test named `example of <name>` (then `example 2 of <name>`), placed
  after the package's own tests in the test file beside the declaration's
  file, in the scratch app. One that doesn't compile is shown at its line
  and left out so the rest run. A failure counts as shown when an example
  or one of the package's tests has a line calling the function with
  `fails <Failure>`. The bar's row counts examples; the package's own
  tests stay the `tests` row.
- **The changelog.** `CHANGELOG.md` beside `package.jadpo`,
  part of the package and its digest: `# Changelog`, then a `## <version>`
  section per version, newest first. For a new version `jadpo package
  check` writes the list as the first list under its heading (`- first
  release`, ``- added type `OutwardCode` ``, or `- no change to what it
  offers` for a patch), and everything beneath it is the author's. While
  the top section's version isn't a released tag, its heading and list
  are rewritten and the author's text kept, so a change before the tag
  updates it; a released section is never rewritten. A new version whose
  section has no text of the author's fails the bar
  (`publish.changelog_missing`); an already released one passes.

> Not built yet: adding another Jadpo app's folder, or a package by bare
> name from the registry, says `toolchain.not_built_yet`. Accepting an
> update's changes waits for package upgrade plans
> ([Package upgrades](18-records-and-manifest.md#package-upgrades)).
> Outside CI, `jadpo build` still builds against a link.

## How connections to packages are read
<!-- topic: how-connections-to-packages-are-read -->

This is the detail behind [Connecting in `app.jadpo`](#connecting-in-appjadpo):
which keys a connection to a package takes and needs, and what
`jadpo check` writes about the hosts it reaches.

- **Naming the package.** `package:` names a provider package the app
  holds, installed or its own, by its name and version
  (`AcmeCouriers.v1`), as the lock does; `app.jadpo` needs no `use` line
  for it. A version the app doesn't hold names the ones it does; a library
  package is refused, since it needs no connection
  (`connection.package_unknown`).
- **The keys.** A connection to a package takes `package:`, `domains:`,
  `credential:`, `environment:`, `host:`, `timeout:`, `retry:`,
  `max_concurrent:`, `webhooks:`, `erasure:`, `development:`, `polls:`
  and `outbound:`, and each setting its package declares; anything else is
  `connection.key_invalid`, with the nearest key. Each is read where it
  `connection.key_invalid`, with the nearest key. Each is read where it
  is used. Driver connections keep their own keys.
- **What a package needs.** `domains:` always, a list of the app's
  domains by name, each once and at least one.
  `credential:` exactly when `provider.jadpo` names a `credential:`
  scheme, read from a `secret: true` setting. `environment:` exactly when
  the package has `hosts:`, read from a `ProviderEnvironment` setting, the
  new built-in enum (`.sandbox`, `.live`); a value written in source is
  refused, so each deployment says which it calls. `host:` exactly when
  the package has neither `hosts:` nor `runs:`, read from a `Url` setting
  (or a type built on it); beside `runs:` it may be given, and beside
  `hosts:` it is refused. A missing one is `connection.key_missing`, a
  value of the wrong kind `connection.value_invalid`, a key the package
  doesn't take `connection.key_invalid`. That a host is HTTPS is checked
  when calls go out, since a `Url` setting may hold
  `http://`.
- **A package's settings.** `provider.jadpo`'s `settings:` names each
  setting and its type, one of `Text`, `Int`, `Decimal`, `Bool` and
  `Duration` (`package.provider_invalid` otherwise), and every connection
  gives each, written out or read from a setting whose type is built on
  it. A setting has no default: every connection gives each one.
- **The network report.** `build/report/network.md` and `.json`, written
  by `jadpo check` once the connections and their addresses read cleanly,
  designed with the CLI design language's calm, plain rules: a title, one
  sentence, and one table row per host in order of connection and host,
  with no times or totals, so a change is a one-line diff. A package's
  live and sandbox hosts each get a row, `when` its environment setting
  says so; a package with no fixed host is listed at the address in its
  `host:` setting; a package Jadpo runs, as the service it runs from its
  image. Database and cache connections are listed at their `url:`
  setting, for the domains whose entities they store; the disk driver
  reaches no host and isn't listed. The JSON gives each host's
  `connection`, `address`, `setting` or `service`, `when`, `for` and
  `domains`. The manifest has a `host` node for each row, under its
  connection, and the graph's diff says which changes to them need a
  person ([Package upgrades](18-records-and-manifest.md#package-upgrades)).
- **How the seam finds a connection's driver.** The capability seam
  asks the host for `capability(connection)`; a host gives a driver only
  for a connection whose declared driver it carries. Natively and in
  `jadpo dev` that is the disk driver for the stores of files the running
  program's plan lists (re-read after every reload); in `jadpo test` the
  in-memory store for the same list, which the executor gives the test's
  host. A package's connection is given the package driver ("How provider
  calls are made"), or in `jadpo dev` and `jadpo test` the package's fake
  ("Fakes and the development call log").

> `outbound:` waits for services, which aren't built yet. Package
> settings have no defaults yet. The approval report that shows host
> changes isn't built yet.

## A package's names, and watching a linked checkout
<!-- topic: package-names-and-linked-checkouts (was a-package-s-names-and-watching-a-linked-checkout) -->

How a name written through a package is told apart from the app's own,
and how `jadpo dev` and `jadpo test --watch` follow a package you have
linked to a checkout you are working on.

- **Where a package's name must be unique.** In its package only. Two
  declarations of one package can't share a name
  (`semantic.duplicate_declaration`), but none clashes with the app's,
  of any kind and wherever it is declared (a domain, `shared/`, public or
  not), nor with another package's: `Postcodes.uk.tidy` beside the app's
  action `tidy`, or `Postcodes.UkPostcode` beside the app's
  `UkPostcode` and `ZipCodes.UkPostcode`, are three things. Their file
  names never fold together either, since each package is its own
  folder. A package's library may be named as an app's library or
  domain is, since it is always written through its package.
- **Which one a name means.** In the app, a bare name is always the
  app's and `Postcodes.<name>` always the package's, so returning
  `Postcodes.UkPostcode` where the app's `UkPostcode` is declared is
  `type.mismatch`. Inside a package, a bare name is the package's own. Failures keep their package too: `fails
  Postcodes.InvalidPostcode` and the app's `fails InvalidPostcode` are
  told apart in tests and at runtime.
- **What a watch watches.** The files every load reads: for a linked
  package, its checkout's sources and data, each at its path in the
  package's folder (`packages/postcodes/v2/libraries/uk.jadpo`), and not
  the installed copy, which stays as it was. The watcher reads the lock at
  every look, so `jadpo package link` or `unlink` while `jadpo dev` or
  `jadpo test --watch` runs moves the watch, and `test --watch` reads the
  whole folder again from where it now lives. A save in the checkout gets
  the watcher's guard for a file emptied mid-save. `jadpo dev` loads the
  project afresh on each save, so it needs nothing more.
- **Where its problems show.** At the checkout's paths in `jadpo test`,
  `test --watch` and `jadpo dev`, as `jadpo check` shows them, since
  that is where they are fixed; the rule line still names the save at
  the package's path, as the project reads it.

## Two versions side by side, lazy loading and the package cache
<!-- topic: two-versions-side-by-side-lazy-loading-and-the-package-cache -->

How an app holds two major versions of one package while it moves
between them, and how Jadpo avoids compiling and checking package code
the app doesn't use.

- **When names carry the version.** Exactly while the project holds two
  versions of one package of the same kind, both installed in
  `packages/<name>/` or both the app's own in `shared/providers/<name>/`:
  then every name written through either carries its version
  (`Postcodes.v3.UkPostcode`, `Postcodes.v3.uk.parse(...)`), and one
  without it is `package.version_needed`, naming both. With one version
  held, a name never carries it, so there is one way to write each name;
  one that does is `package.version_unneeded`, with the edit that drops
  it. An installed package and the app's own of one name stay a clash
  (`package.name_taken`), never two versions. Two versions are two
  packages throughout: their declarations never clash, their failures are
  told apart, and the graph and the interpreter know each version's
  declarations by its own name (`Postcodes.v2.uk.parse` beside
  `Postcodes.v3.uk.parse`). A package's own code still writes its names
  bare. A name the package declares that looks like a version (a library
  called `v2`) is the declaration, not a version.
- **The `use` line.** `use packages/postcodes` names the folder, so it
  brings in every version held there; there is no `use` line per version.
- **The lock and connections.** Each version has its own line, as
  `Postcodes.v2` and `Postcodes.v3`, sorted by name, then major version
  ([Records and manifest](18-records-and-manifest.md)). A connection names
  one version in `package:`, so two versions of a provider are two
  connections.
- **The commands.** A package's folder or code name (`postcodes`,
  `Postcodes`) names both versions, so `jadpo package show`, `update`,
  `remove` and `unlink` refuse it and ask for the version
  (`cli.package_arguments`). `add` installs a second major version beside
  the first. `remove` refuses a version while the app's code writes a
  name through it (`install.in_use`), not while the `use` line stays for
  the other version. `update`'s review marks an entry `used by the app`
  when the code writes it through that version.
- **Translations side by side.** The app words a library package's
  messages under its folder's name (`postcodes: { }`,
  [Packages' text in an app](10-configuration-and-time.md#packages-text-in-an-app)),
  which two versions would share, so while two are held the version
  joins the key (`postcodes_v3: { }`), as it joins the names; with one
  held, the key stays the folder's name.
- **Lazy loading.** A package's index and `provider.jadpo` are always
  read; an installed package's `resources/<resource>.jadpo` is compiled
  only once code outside every package writes `<connection>.<resource>`
  for a connection to it. Its bytes still
  count towards the digest, so a changed resource the app doesn't call is
  still `package.digest_mismatch`. The app's own integrations are its
  code, compiled whole, and so are an installed package's types,
  failures and libraries, which the app's names reach directly. Once
  reached, a resource stays compiled for the life of `jadpo dev`, `test
  --watch` or the language server. `jadpo package show` reads a package
  whole.
- **The package cache.** `jadpo check` keeps an installed package's check
  in `~/.cache/jadpo/packages/<digest>/<compiler>` (under
  `$XDG_CACHE_HOME` when it is set): the paths of the package's source
  files it checked with no problem. A later check skips those files' own
  checks; their declarations are still read for the app's. The compiler
  is its version and the size and modification time of the running
  `jadpo`, so a rebuilt compiler never trusts an older one's check. Only a
  package whose files match its lock line's digest is cached, never a
  link; with no home folder, or a cache that can't be read or written,
  every file is checked. The other commands check every file.
- **Connection names.** A connection, to a package or a
  driver, named like one of the app's domains or libraries is
  `semantic.duplicate_declaration` at the connection in `app.jadpo`,
  under "Connecting in `app.jadpo`". A package's library isn't one of the
  app's, so it never clashes.


## How the OpenAPI importer reads a document
<!-- topic: how-the-openapi-importer-reads-a-document -->

This is the detail behind [Where packages come from](#where-packages-come-from):
what `jadpo package import openapi <file>` reads from a provider's OpenAPI
document, what it writes, and what it leaves for you or your agent to
check.

- **Where it writes.** Always the app's own integration,
  `shared/providers/<name>/<version>/`; installing another Jadpo app's
  package into `packages/` is `jadpo package add`. The name
  and version are `--as AcmeCouriers.v1`, or the document's title in
  `UpperCamelCase` and its version's major number (`1.4.0` is `v1`), or
  its date (`2024-06-20` is `v2024_06_20`), else `v1`, as for a Jadpo
  application, whose document's version is a hash of its record.
- **What it reads.** OpenAPI 3.0 and 3.1 as JSON, from a local file. YAML
  and Swagger 2.0 are refused with what to do (`cli.package_unreadable`
  and `cli.package_not_openapi`, listed under
  [Command-line errors](14-tooling.md#command-line-errors)). Nothing is fetched.
- **The files.** `package.jadpo` with `Licence.proprietary` (an own
  integration may use it) and `jadpo: "1.0"`; `provider.jadpo` with
  `protocol: Protocol.json_http`, `imported: { file: digest: }` (the path
  from the project's root and `sha256:` of the file's bytes), `hosts: {
  live: }` from the first absolute server, and `credential:
  CredentialScheme.bearer` or `.basic` from the security schemes; a
  resource per tag, in `resources/`, every resource `public`; the types
  and failures the operations reach, each `public` in its own file, and
  nothing else of the description. Every operation is `source:
  PackageSource.imported`.
- **Operations.** By path, then `get`, `put`, `post`, `delete` and
  `patch`. A resource is the operation's first tag in `lower_snake_case`,
  else its path's first fixed segment that isn't a version. On the
  resource's shortest fixed path an operation is `list` or `create`; with
  one placeholder after it `get`, `update` (`replace` for a `PUT` beside a
  `PATCH`) or `delete`; any other is its `operationId` in
  `lower_snake_case`, or a Jadpo application's method and fixed segments
  (`post_archive`). Its input is its body's type when it has no
  placeholders or query parameters, else `<Resource><Operation>Input`
  holding them and the body's fields; header and cookie parameters and a
  body that isn't JSON are left out and listed. Its output is its lowest
  `2xx` answer's JSON body.
- **What it can't know from another provider** is written the safe way
  and listed: `changes: true` for every method but `GET`, no
  `idempotent:`, no failures, no `idempotency:` or `error_code:`, and no
  sandbox host.
- **Types.** The exact mappings of
  [Mapping imported types to standard types](#mapping-imported-types-to-standard-types)
  (`date-time` `Instant`, `date`
  `CalendarDate`, `email` `Email`, `uri` `Url`, `uuid` `Uuid`, `ipv4` and
  `ipv6` `IpAddress`, `byte` `Bytes`, `duration` `Duration`, and an
  enumeration of ISO 4217 codes `Currency` or of ISO 3166-1 codes
  `Country`), `integer` `Int`, `number`
  `Decimal`, `boolean` `Bool`, an array `List` (`Set` with
  `uniqueItems`), an object of one kind of value `Map<Text, V>`, and their
  rules (`minLength`, `maxLength`, `pattern`, `minimum`, `maximum`,
  `minItems`, `maxItems`). A field not `required`, or `nullable`, is `?`.
  An object or enumeration inside another is a type of its own,
  `<Parent><Field>`; a constrained item of a list is a refined type,
  `<Parent><Field>Element`. A schema named as a built-in type is
  `<Name>Type`, and listed. `oneOf` and `anyOf` of several shapes, and an
  object of any fields, are left out and listed. Shape and name
  detection is under "Shape and name detection".
- **A Jadpo application's document** (one whose operations carry
  `x-jadpo-changes`) is read exactly: each field's type from
  `x-jadpo-constraints`, `changes:` from `x-jadpo-changes`, `idempotent:
  true` where a key keeps the response, the provider's `idempotency:` from
  `x-jadpo-idempotency`, `error_code: "error.code"`, its failures from
  `x-jadpo-failure` (named from the code, with its kind and message), no
  host (the connection gives `host:`), and no finding. Its runtime's own
  routes (`x-jadpo-generated`: the exchange, passkeys, the served
  document) are left out. An API that signs callers in only with a
  browser session gets no credential, and a finding.
- **Importing again merges** one declaration and one member at a time:
  what the description adds is added, what both have is kept as the file
  has it, so corrections survive, and what only the file has is kept and
  listed; only `imported:` is always the description's. A file that
  doesn't read is left alone and listed.
- **The summary** says what was written where and counts what the package
  holds, lists a merge's changes, and groups what is left to check by
  what to do, each `(agent)`, with `--json` for agents.
- **Field and variant names on the wire.** A field or variant is the
  provider's name in `lower_snake_case`; where that differs (`collectAt`,
  `after-noon`, or `type` written `type_value` to avoid a reserved word),
  the importer writes the provider's spelling as the field's or variant's
  `wire_name:` ([Decoding](#decoding)).

> Importing Smithy, Postman, HAR and `.d.ts` isn't built yet: each is
> `toolchain.not_built_yet`.

## How provider calls are made
<!-- topic: how-provider-calls-are-made -->

This is the detail behind [Calling](#calling): which keys an operation
takes, how its input is sent, who may call it, whether it may be sent
again, and how its answer is classified and read.

- **An operation's keys.** `method:` and `path:` are required; `input:`
  is one of the package's object types, and without it the call takes
  nothing; without `output:` it gives nothing and its answer isn't read.
  `changes:` defaults to `true` for every method but `GET`, as the
  importer writes it, and `idempotent:` to `false`. `source:`, `docs:`,
  `scope:` and the flag `large_output:` are read where they are used;
  `wire_output:` and `decode:` are under "Wire encodings, decoders and
  retries". Anything else, a malformed path, a placeholder no input field is
  sent as, or a failure that isn't the package's, is
  `package.operation_invalid`.
- **How the input is sent.** Written as the package's JSON, its members
  by their spellings on the wire; each placeholder in `path:` takes the
  member of its spelling, as text, escaped, as one segment (a value that
  would leave it, empty, `.` or `..`, is a fault, and nothing is sent);
  the rest is a `GET`'s or
  `DELETE`'s query (a list as the key repeated, `none` left out, an
  object as its JSON) and every other method's JSON body.
- **Who calls.** Only an action, of a domain the connection's `domains:`
  lists (`connection.domain_not_listed` otherwise); never a function, and
  never a test, which reaches a provider through an action with the
  provider faked. A `basic` credential is the setting's `user:password`.
- **Its failures are conditions of the call.** An operation that lists
  failures needs `attempt` or `match`. `attempt`'s arms are keyed by the
  failures' codes and name the app's failure of the same kind
  (`failure.mapping_kind`); the provider's failure becomes the app
  failure's internal cause. A code no arm maps, or one the operation
  doesn't declare, is a fault naming the operation and the code, its
  classification `internal_fault`, so a package failure never reaches an
  app's `fails` list. A `match` on the call names the package's failures,
  as a library function's.
- **Whether it may be sent again.** An operation that changes nothing is
  safe; one marked `idempotent: true` is keyed when its package has
  `idempotency:`, and safe without one (repeating it changes nothing);
  any other change is unkeyed, so one whose outcome is unknown is parked
  rather than sent again.
- **The key**, as
  [Several changing calls in one action](#several-changing-calls-in-one-action)
  uses it, is `jd1_` and 32 characters of RFC 4648
  base32 in lower case, the first 160 bits of a SHA-256 over the parts,
  each given by its length so two sets of parts never run together, and
  cut to `max_length:`. The application is its name (its folder's until
  `app.jadpo` names it); the environment the host its environment setting
  chose, `live` or `sandbox`, or the address a `host:` setting gives. In
  a route, the caller's idempotency scope and the client's
  `Idempotency-Key`, whether or not a store keeps it, or `request` and the
  request's id; in a subscriber, `delivery` and the delivery's id, but a
  sign-in email's attempt a key of its own
  ([Built-in emails](16-built-in-services.md#built-in-emails));
  anywhere else (a schedule, a test's call) a key made from the
  invocation's randomness the first time a call needs one. The step
  counts the invocation's keyed calls from 1, a delivery's attempt from 1
  wherever it runs, and an action run again after a concurrent change
  takes the same steps; the generation is 0 until a provider's
  `new_key_on:`.
- **Where it is sent.** HTTPS, or plain HTTP only to this machine
  (`localhost`, an IPv4 address in `127.0.0.0/8`, `[::1]`, the host read
  as an address, so `127.api.example.com` is elsewhere), which is how a test reaches its
  server; any other address, or a connection whose address or credential
  has no value, is `misconfigured` and logged as refused, sent to nobody.
  The call waits `timeout:` (3 seconds by default), a duration
  written out (`connection.value_invalid` otherwise). It sends `Accept`
  and `Content-Type: application/json`, `User-Agent: jadpo`, the
  credential as `Authorization`, and the key in the package's header.
- **The package driver** is one driver for every package connection,
  over the fetch its host gives. Natively that is an HTTP/1.1 client on
  the worker's thread, with its connections kept per host, checking the
  certificate and the host against Mozilla's roots for HTTPS. It
  classifies what came back: a `2xx` as sent, unless
  `failed_when:` marks it, then the code at `error_code:`; a redirect
  `misconfigured`, and a `401` or `403` as the credential's source says:
  `misconfigured` for a setting's, the connection's for every call; `408`, `429` or the package's `in_use:`
  code `unavailable`; any other `4xx` the code at `error_code:`, a fault
  when there is none; a `5xx`, no answer in time, a connection broken
  once the request had left, or an answer over 10 MB as an answer that
  may have acted, which is `unavailable` for a keyed or safe call and
  `outcome_unknown` for an unkeyed one; a request that never left
  `unavailable`. `reused:` is a code like any other, so a key sent with a
  different request is a fault.
- **The answer** is kept as sent on a recording, and decoded again by the
  build that replays it. It is read from `output_at:` (nothing there is a
  fault) with its output type's codec, every field it declares decoded as
  it arrives; members the package's types don't declare are left out, so
  a provider adding fields breaks nothing. Only the fields code reads
  are decoded ("Wire encodings, decoders and retries"), so a changed
  field nobody reads breaks nothing either. A value that doesn't decode is a fault
  naming the operation, the field and the rule, never the value.
- **`wire_name:`** goes in a field's block and in an enum variant's block;
  a variant whose block holds only `wire_name:` carries no data, and one
  with data is tagged with its spelling. It is the spelling wherever the
  type's JSON is written or read: a provider's request and answer,
  `json.read` and `json.write`, an event's payload. Only a provider
  package's types give one, never empty, never the name itself, and never
  two members of one type under one spelling (`package.wire_name_invalid`).
- **A package's object types are built through its name**,
  `AcmeCouriers.CreateBooking { ... }`, as its other types are written.


> Not built yet: a connection's protocol other than `Protocol.json_http`
> and `.aws_json`, and a credential scheme other than
> `CredentialScheme.bearer`, `.basic`, `.header` (which
> [AI](21-ai.md) packages use) and `.aws_sigv4`, is
> `toolchain.not_built_yet` at the call. A host holding a setting's
> placeholder (`{project_ref}`) isn't filled yet.

## Wire encodings, decoders and retries
<!-- topic: wire-encodings-decoders-and-retries -->

This is the detail behind [Decoding](#decoding) and
[Calling](#calling): the encodings a provider's fields can be sent in, a
package's own decoders, which fields are decoded, and how `retry:` and
`timed_out:` work.

- **The built-in encodings** are ten, by these names: `unix_seconds` and
  `unix_milliseconds` (an `Instant` as a whole number), `iso_8601` (an
  `Instant` with any offset from UTC, which the contract refuses, since it
  writes only `Z`), `day_month_year` (a `CalendarDate`, the encoding
  [Jobs and events](12-jobs-and-events.md) and the
  [Standard library](19-standard-library.md) name for file rows, here
  built for providers), `text` (an `Int` or a `Decimal` as text),
  `comma_separated` (a list of single values), `base64` (`Text` as the
  Base64 of its UTF-8), `json_text` (any value's JSON in a string),
  `minor_units` (`Money` in the one currency its type's `currency:`
  fixes, which `Wire.minor_units` needs) and `number` (an `Int` or a
  `Decimal` as a JSON number, its digits kept as written both ways). The
  contract writes a `Decimal` as text and reads no number for one, so
  without `number` a provider sending `2.50` could not be read at all,
  not even by a package's own decoder.
- **Where they apply.** On the provider's wire only: a call's input, its
  answer and a webhook's event. `json.write`, a route's answer
  and an event's payload write the field as the contract writes its
  type, so an app's clients never see a provider's encoding, where a
  field's `wire_name:` is its spelling everywhere. In the
  plan a field that says how it is sent has a codec of its own, holding
  its type's contract codec and how the provider sends it.
- **What each refuses.** `wire:` holds `Wire.<name>` naming one of the
  ten, on a field of a type that encoding holds; outside a provider
  package, beside `decode:` or `encode:`, or naming anything else, it is
  `package.wire_invalid`. A value the provider sends that its encoding
  can't read is a fault naming the operation, the field and the rule
  (`type`, `format`, `max` for an instant past the years an `Instant`
  holds), and a list item holding a comma, which `comma_separated` would
  split, is the fault `comma_separated` when the call is sent.
- **A field's own functions.** `decode:` and `encode:` come together,
  each `library.function` in the package's own `libraries/`, internal or
  public. `decode:` takes one value, the field as the provider sends it,
  and gives the field's type; it may fail, which is a fault. `encode:`
  takes the field's type and gives what `decode:` takes, and may not
  fail. Either named but not the package's, or of another shape, is
  `package.wire_invalid`; they count as the functions' use. That the
  pair round-trips, and that they read every recording, is checked under
  "Recordings".
- **An operation's whole decoder.** `wire_output:` and `decode:` come
  together, with `output:` (`package.operation_invalid` otherwise):
  `decode:` takes the `wire_output:` type and gives the `output:` type.
  The answer is read from `output_at:` as `wire_output:`, its own
  fields' encodings applied, and given whole to `decode:`.
- **Run at the call's site.** The runtime asks the executor to run each
  package function where the call is made; a
  field decoder that fails is the problem `decode` on that field, and an
  operation's decoder that fails names how it ended (its failure's code
  or its fault's classification), never a value.
- **Only the fields read.** Worked out per call site from its action's
  body: when the call's value, or its `attempt`'s, is a variable's and
  every use of that variable is a field read, only those top-level fields
  are decoded and checked; the others hold `none`, which no code can
  read. Fields inside a read field are decoded with it. Rules across the
  type's fields are checked only when every field is read. An operation
  decoded whole is read whole.
- **Numbers as written.** An answer is parsed with Jadpo's own reader,
  so a number keeps the digits the provider wrote and an
  answer naming a member twice isn't JSON; a request's body and query
  are written with each number's digits as the input's codec wrote them.
- **`retry:`** takes `attempts:`, 1 to 10, and `within:`, a duration up
  to 5 minutes, each defaulting to 3 and 30 seconds;
  `connection.value_invalid` otherwise. A call is sent again when it
  never left or the provider said it did nothing (`408`, `429`, its
  `in_use:` code), whatever it changes, and when it may have acted (a
  server's error, no whole answer, no answer in time) only if it changes
  nothing or takes its key, under the same key. Waits are 200
  milliseconds doubled before each further attempt, drawn evenly from
  zero (full jitter, from the host's randomness), or a `408` or `429`'s
  `Retry-After` in seconds; a wait past `within:` stops the retrying. A
  host with no timer, as a test's, sends again at once and measures
  nothing. Each attempt goes through the capability seam and is logged,
  recorded and replayed as a call of its own.
- **`timed_out:`** The driver tells an answer that didn't come within
  `timeout:` apart from other missing answers. Where
  the call's `attempt` maps `timed_out:` and the operation is safe to
  send again, the last attempt's timeout is the runtime's `timed_out`
  failure (kind `timed_out`, 504), mapped to the app's failure with it
  as the cause; anywhere else it is `unavailable` or `outcome_unknown`,
  as "How provider calls are made" says. `timed_out:` on an operation that changes something without
  a key is `failure.unknown_mapping`, to a failure that isn't
  `unavailable` `failure.mapping_kind`; an operation listing no failures
  may still be attempted for `timed_out:` alone.
- **What reviewers see.** The package's surface writes an operation's
  `wire_output X decode Y`, so the answer as sent shows beside its
  output, and `jadpo package show` gives it as "sent as"; the manifest
  gives a field's `wire`.

## Fakes and the development call log
<!-- topic: fakes-and-the-development-call-log -->

This is the detail behind [Tests and fakes](#tests-and-fakes) and
[Provider calls in development](#provider-calls-in-development): what a
fake answers, how a script is used, and what the call log keeps.

- **One fake per test, in every provider connection's place.** The test
  host gives a package's fake for
  each connection to a provider package, so a subscriber's call between
  the test's steps answers from the same script. An unscripted call
  answers with the operation's generated default.
- **The generated default** is the value of the operation's output type a
  provider would send, keeping its rules where a simple value can: text
  of its least length (`example` where that fits), a number at its least,
  the first plain variant, empty lists and maps, `null` for what is
  optional, the invocation's clock for an instant and its date, a fresh
  UUID. It is written as the provider sends it, its `wire_name:`
  spellings and `output_at:` envelope included, and read back through the
  operation's decoding, so a type whose rules no such value keeps (a
  pattern, say) faults where it is read, naming the field and the rule,
  and the test scripts it instead.
- **A scripted answer** is a value of the operation's output type, written
  and read back the same way; one of its failures, by its code; or a
  condition: `unavailable`, `lost` (the request went and no answer came:
  `outcome_unknown` for an unkeyed change, `unavailable` otherwise),
  `in_use` (the provider's key still in use) or `misconfigured`. A bare name is only a code or a condition, never
  a constant; an operation is scripted once, with at least one answer
  (`semantic.fake_invalid`). `fakes:` sits among the test's settings, and
  its answers are given to the fake before the test's first statement.
- **A script's use.** Each call of a scripted operation takes its next
  answer; a call past the last gets the generated default and fails the
  test as `test.fake_exhausted`, which reports in place of whatever the
  test went on to fail with, being the first thing that went wrong. Answers
  left fail a test that otherwise passed as `test.fake_unused`. Both name
  the operation, at its first scripted answer.
- **`jadpo dev`** gives every provider connection of the program it runs
  one driver: the sandbox, through the package driver, for what is
  let through; otherwise the fake, answering what `jadpo calls script`
  scripted, then the generated default. A provider connection's
  credential, environment or address setting that no variable or default
  gives gets a stand-in, so the app starts; such a connection stays faked
  whatever `sandbox:` says, and `jadpo dev` names the variables to set.
  Its environment set to `live` refuses to start, or to reload a save,
  with `cli.dev_start_failed` naming the variable and `--fake`. The
  session's `--sandbox` takes `connection.resource.operation` or
  `connection.resource`, and `--fake` a connection, each checked
  (`cli.dev_arguments`).
- **`development: { sandbox: }`** takes only `sandbox:`, a list of
  `resource.operation` and `resource` names, each once, each one of the
  package's public operations or resources (`connection.key_invalid`,
  `connection.value_invalid`).
- **Scripting by hand.** `jadpo calls script <operation> <answer>` takes
  an operation as the code calls it and one of its failures' codes or a
  condition, checked against the project's plan; a value can't be given on
  the command line. Each answer waits as a file of its own under
  `build/dev/script/`, oldest first, and the call that takes it removes
  it, so each answers one call, whichever worker makes it.
- **The call log** is `build/dev/calls.jsonl`, one call a line: its id
  (`call_31`, counted on from the last the log holds), when it went out,
  its connection and operation, its source (`generated`, `scripted`,
  `sandbox` and `recording`), its request's id, how long it
  took, the request as made (method, address and body, never its headers,
  which hold the credential) and the answer as sent or generated, and how
  it ended with a failure's code. It keeps the last 500 calls. A call
  `retry:` sends again is one
  entry, its last attempt's, where the invocation's own log
  keeps each attempt. Only provider calls are logged, a host's other capabilities'
  calls (files) are not, and a delivery's or a schedule's call has no
  request. Each request's line in `jadpo dev` lists its calls, and its
  `--json` event gives them.

> A script's `.requests` isn't built yet, nor are the timelines the call
> log would keep beside its calls.


## Shape and name detection
<!-- topic: shape-and-name-detection -->

This is the detail behind layers 2, 3 and 6 of
[Mapping imported types to standard types](#mapping-imported-types-to-standard-types),
for fields: how a provider's fields that together make a standard type,
such as an amount and a currency, are read as that type, and which
fields the importer asks you to check. Layers 4 and 5 are under
[The scoped review and unconfirmed operations](#the-scoped-review-and-unconfirmed-operations).

- **How a provider's shape is decoded at the edge: `wire_parts:`.**
  Stripe's `amount: 1050, currency: "gbp"` decodes to `Money`, and no
  other decoding could do it: `wire:` and `decode:`
  read one member, and `Wire.minor_units` needs one fixed currency. A
  field's `wire_parts:` names, for each part of its standard type, the
  member of the object holding the field it is read from and written to;
  the field has no member of its own on the provider's wire, and is
  written as the contract writes it everywhere else. It is for the
  standard types made of named parts (`Money`'s `amount` and `currency`,
  `InstantRange`'s `start` and `end`, and the others sent as an object of
  members) and a `Point`, by `latitude` and `longitude`, which GeoJSON
  sends as a pair. Every part is spelled, as text, once
  (`package.wire_invalid`), never beside `wire:`, `decode:` or `encode:`.
  Fields may share a part's member, as a charge's amounts share its
  currency, but a field of its own can't take one
  (`package.wire_name_invalid`); written, two values that share a member
  and differ stop the call as one that doesn't encode, naming the member.
  None of a field's members sent is `none` where the field may be; some
  missing is a fault naming each. It follows `wire_name:`'s shape.
- **What the importer maps by shape.** Only what the shapes
  `type.standard_exists` matches name, and only where nothing is guessed
  but the units: a whole number named as `Money`'s amount (`amount`,
  `value`, `cents`, `pence`, `minor`, `amount_minor`) beside a currency
  (`currency`, `currency_code`) of the same optionality is `Money` read
  from both, the currency's field and its own enumeration gone; a
  latitude beside a longitude (`lat`, `lng`, `lon`) of the same
  optionality is a `Point` named `location` (`position` or `point` where
  that is taken). An object that is only an amount in whole numbers and a
  currency, or only a `start` and an `end` that are instants, each
  required and spelled as the contract spells it, is `Money` or
  `InstantRange` wherever it is used, and isn't declared, since nothing
  needs decoding. Any other object of a standard shape is left to
  `type.standard_exists`, whose swap needs a decoding an agent writes.
- **What it leaves to check: `package.standard_candidate`.** A whole
  number named like a time (`created`, `updated`, `deleted`, `expires`,
  `timestamp`, `time`, or ending `_at`, `_time` or `_timestamp`) may be an
  `Instant` sent as seconds or milliseconds; one named like an amount
  (`amount_` or `_amount` besides the shape's names) beside a currency
  may be `Money`; neither is swapped, since only the documentation says
  which. Each is a declinable error on the field, in a provider package's
  public objects, until it says how it is sent or is declined with a
  reason; so is a latitude beside a longitude that isn't mapped. A number
  named like a time that is text (`"2026-10-20"`) isn't flagged: the page
  names whole numbers, and a format says when text is a time.
- **Fakes.** A generated default answer writes a field's parts as their
  members, and a `Money` as the contract sends one, a whole number of
  minor units in the currency its type fixes or `GBP`.
- **The summary.** The status grid counts the fields mapped `by format`
  and `by shape`; "Mapped by shape" lists each shape mapping and how it is
  read, since each is a judgement, where a format's is only counted; each
  guess is under "To check" with its `(agent)` step. `--json` gives every
  mapping as `mapped`, with `at`, `standard`, `by` and the members it is
  read `from`.

The import's own output, for a payment provider's document:

```text
$ jadpo package import openapi tidepay.json
✓ Imported tidepay.json into shared/providers/tidepay/v2024_06_20
    package     Tidepay.v2024_06_20
    read        from its schemas
    resources   charges
    operations  1
    types       3
    standard    4 by shape

Mapped by shape
  Charge.amount  Money, read from `amount` and `currency`
  Charge.fee     Money, sent as the standard type
  Charge.window  InstantRange, sent as the standard type
  Shop.location  Point, read from `lat` and `lng`

To check
  May be `Money` in minor units, its currency in `currency`: check the
  provider's documentation, then make it `Money` read from both with
  `wire_parts:`, or decline `package.standard_candidate` with the reason
  (agent)
    Charge.amount_refunded

  May be an `Instant` sent as whole seconds: check the provider's
  documentation, then make it `Instant { wire: Wire.unix_seconds }`, or
  decline `package.standard_candidate` with the reason (agent)
    Charge.created

✓ imported  1 operation · 3 types · 4 to check

→ jadpo check  checks the package with the app
```

(The provider's two findings, its sandbox host and its failures, are left
out above.)

## Recordings
<!-- topic: recordings -->

A recording is a provider's answer to one operation, its plain values
replaced by placeholders, kept in the package as
`recordings/<resource>.<operation>.json` so its decoding can be checked
without calling the provider. This is what a recording keeps,
how it is checked and how you make one.

- **What a recording keeps.** The answer as the provider sent it, only
  along `output_at:`'s path and only the members the operation's output
  type declares, written with two spaces of indentation and members in
  name order. Each text or number the type reads as a plain value is a
  placeholder naming its built-in type in braces, `"{Text}"`,
  `"{Instant}"`, `"{Int}"`, so no personal data or account detail from
  the sandbox is committed. Kept as sent: booleans and `null`, an enum's
  spellings and a map's keys, which say nothing about anyone; a field in
  a built-in encoding or with the package's own `decode:` and `encode:`,
  and the whole answer of an operation with `wire_output:`, which its
  decoders exist to read, numbers with their digits as written; and a
  value whose type's rules no generated value keeps (a pattern), which a
  placeholder couldn't stand in for.
- **Filling a placeholder** gives a generated value of the type at its
  place, as the fake's generated default makes one: the
  invocation's clock, a fresh UUID. Members a hand-written recording adds
  beyond the type are left as they are and ignored by decoding.
- **Which recordings `jadpo test` reads.** Every
  `recordings/<resource>.<operation>.json` of the app's own integrations
  in `shared/providers/`, connected or not; an installed package's were
  read when it was published, and its files are held to its digest. Each
  is a test of a fifth kind, `recording`
  ([Kinds of test](13-testing.md#kinds-of-test)), named
  `<resource>.<operation>'s recording decodes`, run alone by its file;
  it reads the whole output, never only the fields code reads. A
  recording named after no operation of its package fails it too.
- **Round trips** are checked on what a recording holds: each field's
  `decode:` over the value recorded, then its `encode:` over what that
  gave, which must be the value recorded (`test.round_trip_failed`,
  showing both). Built-in encodings round-trip by construction.
- **The publishing bar** counts the recordings as its `decoders` row,
  apart from the package's own tests, and shows each failure's report.
- **`jadpo dev`** answers an unscripted call from its operation's
  recording where its package keeps one, the generated default
  otherwise; a script still comes first. Tests keep the generated
  default, so a test never depends on a recording.
- **`jadpo package record <connection> <resource>.<operation>`** runs in
  an app, through one of its connections, since a package alone has no
  credential or sandbox to call. It records into the app's own
  integration, or into a linked package's checkout, and refuses an
  installed package (`recording.refused`), whose files its digest holds.
  The operation's input is `--input`, JSON as a route's body is written,
  required when the operation takes one. It refuses before sending
  anything a connection whose environment setting chooses `live`, one
  whose credential, environment or address nothing sets (the stand-in
  `jadpo dev` gives it would reach nobody), and one with no address. A
  keyed operation takes a key derived as any call's
  ([Several changing calls in one action](#several-changing-calls-in-one-action)),
  from a scope of its own and this run's randomness, so recording again
  is a new request. A refusal, no answer, or an answer that doesn't
  again leaves the file as it was.

> Writing an operation's output type from a recorded answer isn't built
> yet, so recording an operation with no `output:` is
> `toolchain.not_built_yet`.

## The scoped review and unconfirmed operations
<!-- topic: the-scoped-review-and-unconfirmed-operations -->

This is the detail behind layers 4 and 5 of
[Mapping imported types to standard types](#mapping-imported-types-to-standard-types):
which imported types you review, how a review is marked, and which calls
still need a recording to confirm them.

- **Where a review is marked: `reviewed:` in `imported:`.** It is a list in
  `provider.jadpo`'s `imported:`, beside the description it reviews,
  written by the agent like any source and kept in review's diff; each
  entry is one of the package's own types, in its `types/`, once
  (`package.provider_invalid`), and `imported:` holds only `file:`,
  `digest:` and `reviewed:`. A package with no `imported:` was written,
  not imported, and has no types to review. Every imported type starts
  unreviewed, a Jadpo application's exact import included: the page makes
  no exception.
- **What is in scope.** The package's own types each called operation's
  `input:`, `output:` and `wire_output:` reach, through fields, list,
  set and map items, `?`, payload variants and the type a refinement is
  built on; failures, whose codes and messages aren't a decoding, are
  left out. A call is any provider call in an action outside a
  `.test.jadpo` file. The scope is worked out from the calls on every
  check, so it grows, and shrinks, with them.
- **Only the app's own integrations.** The review and the unconfirmed
  operations cover the packages in `shared/providers/`, which an import
  writes and `jadpo package record` records into. An installed package
  was reviewed and recorded by its publisher, its files held to its
  digest, and its surface already says which operations are `confirmed`,
  so listing its calls would name a step the app can't take.
- **Not a finding.** A type to review and a call to confirm stop nothing:
  the page lets an unreviewed type ship, for `jadpo approval` to show, and
  allows unconfirmed operations in every build. `jadpo check` shows both
  only once it passes, after the errors would be, and counts them, muted,
  in its footer; while it has errors those come first.
- **Recorded for `jadpo approval`.** `jadpo check` writes
  `build/report/providers.md` and `.json` once the connections read
  cleanly and the app has an integration of its own: per package, each
  call and whether a recording confirms it, and each type in scope and
  whether it is reviewed, rows in name order for small diffs.
- **Importing again.** `reviewed:` is kept, as the agent's correction, but
  a type the description adds a member to is new to review: it leaves
  the list, and the summary shows it under "To review again". A type
  whose description changed where the file kept its own version is still
  as reviewed.
- **The import's summary** ends with "To review": the page's one
  instruction, then the types the app's calls reach still to review, or,
  while the app calls none of the package's operations, when the review
  starts; then "Not yet confirmed" with the calls no recording confirms.
  `--json` gives them as `review` and `again`.

## How webhooks are received
<!-- topic: how-webhooks-are-received -->

This is the detail behind [Webhooks as events](#webhooks-as-events): the
generated route, how each signature scheme is checked, and how an event
is read and kept once. Replies, handshakes and the further schemes are
under
[Webhook replies, handshakes and further signature schemes](#webhook-replies-handshakes-and-further-signature-schemes),
and queues under [How queues are received](#how-queues-are-received).

- **Where the route is.** `POST <prefix>/<connection>`, the prefix the
  app's `webhooks` API declares, or `/webhooks` when it declares none, in
  which case the routes belong to no declared API and have no record or
  OpenAPI document of their own; `jadpo routes` lists them under
  `webhooks  /webhooks`. In a declared `webhooks` API each is in its record
  (`generated webhook`, its connection, `authentication signature`,
  `status 204`) and its OpenAPI document. The path changes only with the
  API's prefix. Any method is the runtime's at that path (`OPTIONS` and
  `405` with `Allow: OPTIONS, POST`), so an app route there is
  `route.path_overlap`.
- **The signature.** Checked over the raw body with the connection's
  secret, and the value it replaced while a deployment rotates it, every
  candidate compared in constant time. `stripe`: `Stripe-Signature:
  t=<seconds>,v1=<hex>`, each `v1` tried, over `<t>.<body>` under the
  secret as written. `standard_webhooks`: `webhook-id`,
  `webhook-timestamp` and `webhook-signature` (or Svix's `svix-` headers),
  each `v1,<base64>` tried, over `<id>.<timestamp>.<body>` under the Base64
  after the secret's `whsec_` (the secret as written when it has none).
  `hmac_sha256`: the body's HMAC in the header `header:` names, as 64 hex
  digits or Base64, with or without `sha256=`; it signs no time, so its age
  isn't checked. `basic`: `Authorization: Basic` against the secret as
  `user:password`. A signed time more than five minutes from the host's
  clock, either way, is refused. Any refusal is `401 unauthenticated`,
  "The webhook's signature doesn't match.", saying nothing of why; the
  host's log line says (`unsigned`, `stale`, `mismatch`, `no secret`).
- **Reading the body.** It is JSON (`400 invalid_request` otherwise). The
  event's id is the text or whole number at `event_id:`, its type the text
  at `event_type:` (`400` when either is missing). A type the package
  declares no event for is answered `204` and not kept, since a provider
  sends every type the account subscribes to. The event's fields are read
  from the body's root with its type's codec on the provider's wire, its
  `wire_name:` and `wire:` spellings included, every member it doesn't
  declare left out; a body it doesn't read is `422 invalid_input`, each
  problem by the provider's field, so the provider keeps it and sends it
  again once the package is fixed.
- **`occurred_at`.** Every event of a package with `webhooks:` declares
  `occurred_at: Instant`, with no `wire_name:` or `wire:`
  (`package.wire_invalid`), and the runtime fills it, whatever the body
  holds of that name: from the place `webhooks: { occurred_at: }` names (a
  whole number of seconds since 1970, or RFC 3339 text; `400` when it holds
  neither), else the signature's time, else when the request arrived. Each
  event names the type its provider sends with a text `wire_name:`, no two
  of one package alike (`package.wire_name_invalid`). An app's own event
  gives `wire_name:` for a queue ("How queues are received").
- **Keeping it once.** In one read-committed transaction in the events'
  store, the provider's id is recorded under `webhook <connection>` in
  `jadpo_received_events`, then the event's outbox row and a delivery for
  each subscriber are written, as an action's events are (its trace the
  request's, `traceparent` continued; depth 0; `within:` counted from
  arrival). An id already recorded is a repeat: answered `204` as the
  first was, nothing kept. The tick deletes ids received more than 30 days
  ago, longer than any provider retries. A store that can't be reached is
  `503 unavailable`, nothing kept, so the provider sends it again.
- **The store.** `events: { store: }`, or the only store that holds the
  app's records; with none, or several and none named, the connection's
  `secret:` is `event.store_required`. `jadpo_received_events` is in that
  store's migration only when some connection receives webhooks.
- **The connection.** `webhooks:` takes `secret:`, a secret setting
  (`connection.value_invalid` otherwise, `connection.key_missing` when it
  is left out), and `replies:` ("Webhook replies, handshakes and further signature
  schemes"); given to a package whose
  provider declares no `webhooks:` it is
  `connection.key_invalid`. A package with `webhooks:` and no connection
  giving a secret receives nothing.
- **What answers.** Every request is answered once it is kept or dropped:
  `204` with no body, deliveries running after the response. An event
  whose payload is over 64 KB, every event's limit
  ([Limits and runtime faults](12-jobs-and-events.md#limits-and-runtime-faults)),
  is `413 request_too_large`. It is counted
  against its address's rate limit, whatever credential it carries. A
  non-public package event is kept and reaches no subscriber, subscribing
  to one being `package.not_public`; the package's own subscribers hear
  it ("Package subscribers and recipes in `sample/`").

> A field read by the package's own `decode:` and `encode:` isn't built
> yet on a webhook's event, since those run at a call's site: it is
> `toolchain.not_built_yet`.


## The Cloudflare Queues driver
<!-- topic: the-cloudflare-queues-driver -->

Use the Cloudflare Queues driver to receive a Cloudflare queue in an app
deployed to Workers. Cloudflare pushes each batch of messages to the
Worker, and each message becomes its event for your subscribers, as with
any queue ([Queues](#queues)):

```jadpo fragment
// app.jadpo
connections: {
    warehouse_queue: {
        driver: Driver.cloudflare_queues
        queue: "warehouse-events"
        dead_letter_queue: "warehouse-events-dead"
    }
}
```

- **The connection.** `driver: Driver.cloudflare_queues` names one queue
  by `queue:`, its name in Cloudflare, and `dead_letter_queue:`, another
  queue Cloudflare moves a message to once its retries are spent, so none
  is dropped: both required, both written out, since the consumer is
  written into the Wrangler file when the app is built, each lowercase
  letters, digits and dashes, starting and ending with a letter or digit,
  at most 63 characters (Jadpo's own form, since Wrangler checks no queue
  name and takes any text). The two differ, and no two
  connections name one queue. It takes no `domains:`, since nothing but
  `receiving:` reads it, and reaches no host, so the network report lists
  none. Problems are `connection.key_missing`, `connection.key_invalid`
  and `connection.value_invalid`; `receiving:` takes it as it takes a
  package's connection.
- **A batch.** The Workers host's queue consumer, the Worker's `queue`,
  starts the isolate as `fetch` does and hands each batch to the
  runtime's seam: the queue's name and each
  message's body, as text (a text body as sent, bytes as UTF-8 with an
  invalid sequence replaced, so it parks as unreadable, any other as its
  JSON), with the times it was handed over. An isolate that can't start,
  or whose stores aren't current, retries each message after the delay
  its attempts give, keeping nothing. Each message is read and kept as the
  tick keeps a fetched one ("How queues are received"), and answered in the
  batch's order: acknowledged once its event and id commit, as a repeat,
  or once parked; retried when the store can't take it now, after 10
  seconds doubled for each earlier attempt, at most an hour. The host
  answers each message on its own, never the batch whole, then runs the
  kept events' deliveries after answering, in `waitUntil` and handed on
  past its 30 seconds. Each batch with a message retried logs
  `queue.retried` with the queue's name, how many and a code
  (`not_started`, `not_received`, `store_unavailable`), never a body. A
  batch from a queue the program doesn't receive is retried whole.
- **The tick.** Nothing is fetched from a queue Cloudflare pushes; the
  tick still reads again the parked messages an operator retried with
  `jadpo deliveries retry`, as for any queue.
- **Long retries and the dead-letter queue.** Every message the consumer retries is one the Worker can't take yet:
  its isolate hasn't started, its stores are behind while a deploy
  migrates them, or the store is unavailable; one it can read and can't
  keep is parked, not retried. The delay doubles up to an hour and the consumer has 30
  retries, about 22 hours: long enough to outlast a deploy's migration,
  and short enough that the message reaches the dead-letter queue inside
  the 24 hours a queue keeps a message on Workers' Free plan (4 days by
  default on a paid one, at most 14), every value within Cloudflare's
  limits (100 retries, a delay of at most 24 hours). The Worker consumes
  the dead-letter queue too: each message it hands back is parked whole
  in `jadpo_parked_messages` as `dead_lettered`, with its id and its
  event when they can be read and no event kept, and acknowledged, so
  `jadpo deliveries` lists it and `retry` has the next tick read it again
  with the build running then; one the store can't park now is retried
  on the same schedule, 100 times, since a message whose retries are
  spent there is gone. So one queue's dead-letter queue is no other
  connection's queue or dead-letter queue (`connection.value_invalid`).
  A message whose `retry()` throws isn't acknowledged by default: the
  handler throws once every message is answered, so Cloudflare hands
  again each one not acknowledged.
- **The Wrangler file.** Each such queue is a consumer under `queues: {
  consumers: [] }`: its `queue`, `max_batch_size` 10 (as many as one
  receive asks a package's queue for), `max_batch_timeout` 5 seconds,
  `max_retries` 30 and its `dead_letter_queue`; and
  the dead-letter queue is a consumer too, with the same batch, 100
  retries, Cloudflare's most, and no dead-letter queue of its own. The
  setting names are Wrangler's own: `jadpo build` writes them into the
  Wrangler file as JSON, and Wrangler's `deploy --dry-run` accepts it
  without a warning.
- **Natively.** A server off Workers can't receive the binding: it reads
  no message from the queue, only the parked ones an operator retried.
  A native deployable that runs `Trigger.subscribers`, where the app's
  queues are received, is refused the driver ([Targets](#targets)); a
  native server reads a Cloudflare queue through a package over its HTTP
  pull API instead.

## How queues are received
<!-- topic: how-queues-are-received -->

This is the detail behind [Queues](#queues): when a message is
acknowledged, what is parked, how messages are read and fetched, and how
a package provides `receiving`. The Cloudflare Queues driver is under
[The Cloudflare Queues driver](#the-cloudflare-queues-driver).

- **When a message is acknowledged.** Once the transaction keeping its
  event commits: in one read-committed transaction in the events' store,
  its id is recorded under `queue <connection>` in `jadpo_received_events`
  and its outbox row and a delivery for each subscriber are written, as a
  webhook's are; then the message is deleted from the queue. Its
  subscribers run from the outbox with their usual retries, and one that
  gives up is parked as any delivery is, so "after the subscriber's
  transaction commits" is read as the transaction that hands the event to
  them; tying the message to each subscriber's run would keep a second
  retry budget beside the outbox's. A repeat, its id recorded already, is
  deleted and keeps nothing. Ids are deleted by the tick after 30 days, as
  a webhook's are.
- **Parked, never dropped.** A message whose type the queue doesn't list
  (`unlisted_type`), one that can't be read as its event (`unreadable`:
  not JSON, no id or type where the queue says, a field its event refuses,
  each problem kept by field and rule, never a value) and one over 64 KB as
  its event (`too_large`) are kept whole in `jadpo_parked_messages`, once
  per queue and body, and deleted from the queue. `jadpo deliveries` lists
  each as `failed` (`queue warehouse_queue · OrderShipped`, its reason as
  the code); `retry` makes it `pending` and the app's next tick reads it
  again with the build running then, keeping it or parking it again;
  `dismiss` sets it aside, deleted by the tick 30 days later. A message the
  store can't take now is left on the queue, which sends it again, and the
  queue's own dead-letter queue takes one that keeps failing, which on
  Cloudflare Queues the Worker parks; one whose event the store refuses
  as a statement, as it would every time, is parked `unreadable`.
- **Reading a message.** Its body is JSON; its id is the text or whole
  number at `id_at:`, its type the text at `type_at:`, matched against
  each listed event's `wire_name:`; a queue of one kind has no type. The
  event is read from the body's root, every member it doesn't declare left
  out, the queue's own id and type among them, as a webhook's are.
- **Naming the events.** `events:` names each event as code does: the
  app's own imported on a `use` line in `app.jadpo` (`use stock/events {
  OrderShipped }`), or a package's public one as `Package.Event`. With
  `type_at:` each gives a text `wire_name:`, no two alike; without it the
  queue lists one event. Anything else is `receiving.invalid`, and with no
  store to keep the events in, `event.store_required`.
- **When it is received.** On the tick: each queue is asked for up to ten
  messages at a time, again while a batch comes back full, at most ten
  times a tick, within the tick's budget; up to 20 parked messages an
  operator retried are read first. An app that receives a queue ticks at
  least every 10 seconds. On Workers the queue's consumer is handed
  batches instead ("The Cloudflare Queues driver").
- **How a package provides it.** `provides/receiving.jadpo` holds
  `provide receiving` with `receive:` (`call:`, `input:`, `messages:
  response.<field>`, `body: item.<field>`, `handle: item.<field>`) and
  `acknowledge:` (`call:`, `input:` with `message.handle`). Each input is
  keyed by the operation's input field and given `settings.<name>`, a
  setting the package declares, or text, a whole number or a flag; every
  required field is mapped, the list is a list of objects, and the body and
  the handle are text. An operation whose answer the package's own
  `decode:` reads can't be mapped. Problems are `package.provider_invalid`
  in the mapping's file. An installed package's resources a mapping
  calls are loaded with it.
- **AWS's JSON protocol.** `Protocol.aws_json` sends every call as a
  `POST` with `Content-Type: application/x-amz-json-1.0`, its body the
  input (`{}` without one), naming the operation in `X-Amz-Target:
  <target>.<Operation>`, its name in UpperCamelCase
  (`receive_message` is `ReceiveMessage`). A refusal's code at
  `error_code:` is AWS's error type, read after its `#` in
  lower_snake_case (`com.amazonaws.sqs#QueueDoesNotExist` is
  `queue_does_not_exist`). The provider key `aws: { service: target: }`
  names the service requests are signed for and the operations'
  prefix; `aws_json` needs `target:` and `aws_sigv4` needs `service:`
  (`package.provider_invalid`).
- **AWS Signature Version 4.** `CredentialScheme.aws_sigv4` signs every
  header but `user-agent`, with `Host` and `X-Amz-Date`, over the body's
  SHA-256, as AWS's published test suite and worked example sign, which
  its tests check. The connection gives `credential: { key_id:
  config.<name>  secret: config.<name> }`, the secret a secret setting
  (`connection.value_invalid` otherwise). The region is read from an AWS
  host, `<service>.<region>.amazonaws.com`, and is `us-east-1` for any
  other, as AWS's own tools sign for a local emulator; host placeholders
  wait for a package with fixed regional hosts, so the SQS package's
  connection gives its endpoint with `host:`. The secret appears in no
  header, problem or log line.
- **A member a provider leaves out.** On a provider's wire, a field its
  type may hold `none` in reads as `none` when the provider leaves its
  member out, as SQS leaves `Messages` out of an empty receive; the app's
  own contract still needs the member.
- **The SQS package**, `AmazonSqs.v2012_11_05`, is installed with
  `jadpo package add`. It maps `ReceiveMessage` and `DeleteMessage`, and
  marks `ReceiveMessage` safe to send again, since repeating it only hides
  other messages for a while. Its tests run against a local fake of SQS's
  JSON API that checks each signature.

> Capability mappings other than `receiving`'s and `email`'s aren't built
> yet: one is `toolchain.not_built_yet`.

## Webhook replies, handshakes and further signature schemes
<!-- topic: webhook-replies-handshakes-and-further-signature-schemes -->

This is the detail behind [Webhooks that need a reply](#webhooks-that-need-a-reply)
and [Handshakes](#handshakes): how the Twilio, Slack and SNS signatures
are checked, how each handshake is answered, and how a reply is made,
kept and limited.

- **Twilio.** `SignatureScheme.twilio`: `X-Twilio-Signature`, the Base64
  of the HMAC-SHA1 under the auth token (the connection's secret) of the
  URL Twilio requested followed by each form member's name and value,
  sorted by name and, for a name sent twice, by value. The URL is
  `https://`, the request's `Host` and its path and query, tried with
  `:443` written and without, as Twilio's own libraries do; Twilio sends
  only to HTTPS and a proxy in front of the host ends the TLS. It signs no
  time, so no age is checked; a repeat is dropped by its id. The body is
  a form (`400 invalid_request` otherwise), read as JSON of its members'
  text, a name sent twice a list, so the places and fields name form
  members (`event_id: "CallSid"`).
- **Slack.** `SignatureScheme.slack`, since no other scheme checks
  Slack's events: `X-Slack-Signature: v0=<hex>`, the HMAC-SHA-256 of
  `v0:<X-Slack-Request-Timestamp>:<body>` under the signing secret, with
  the five-minute tolerance.
- **SNS.** `SignatureScheme.sns`: the message's own `Signature`, RSA
  PKCS #1 v1.5 over the members SNS lists for its `Type`, with SHA-1
  (`SignatureVersion` 1) or SHA-256 (2), by the RSA key (2,048 bits or
  more) of the certificate at its `SigningCertURL`, valid at the host's
  time; the chain isn't checked, since the certificate comes over HTTPS
  from AWS's own host. Any AWS account can have SNS sign a message from
  its own topic, so the connection's `secret:` is the topic's ARN, and a
  message from any other topic is refused before anything is fetched.
  The certificate is fetched only from `https://sns.<region>.amazonaws.com`
  (or `.amazonaws.com.cn`), a `.pem` path with no port, credentials or
  query, by the host's guard for the runtime's own requests: HTTPS only,
  no redirect followed, connected only to the public address it checked
  (never loopback, private, link-local, shared, reserved or multicast,
  and an IPv4 address carried inside IPv6, by NAT64, 6to4 or Teredo,
  judged as itself),
  at most 16 KB within five seconds; each certificate is kept for a day.
  SNS signs the time it published and its retries keep it, so the
  five-minute tolerance holds only ahead of the host's clock: a message
  signed 30 days ago or more, as long as a received id is kept, is
  refused, so any replay within that is a repeat by its `MessageId`. A
  certificate that can't be fetched is `503 unavailable`, so SNS sends it
  again; a host that reaches nothing answers so.
  The `Message`, when it holds JSON, is read as that JSON, so places reach
  inside it (`"Message.type"`). Anything but a `Notification` carries no
  event: answered `204`.
- **Handshakes.** `handshake:` names `WebhookHandshake.sns`, `.slack` or
  `.microsoft_graph` (`package.provider_invalid` otherwise); SNS's and
  Slack's need `SignatureScheme.sns` and `.slack`, which sign them
  (`package.provider_invalid` otherwise). SNS's `SubscriptionConfirmation`,
  signed and from the topic, is confirmed by a `GET` of its `SubscribeURL`
  on an SNS host through the same guard (`204` when SNS took it, `503`
  otherwise). Slack's `url_verification`, once signed, is answered
  `200` with `{"challenge": ...}`. Microsoft Graph's validation is the
  only request answered before a signature is looked for, since Graph
  signs none: a `POST` whose query gives one `validationToken` of at most
  1,024 characters, none a control character, answered `200` with the
  token as `text/plain; charset=utf-8` and `X-Content-Type-Options:
  nosniff` (`400 invalid_request` for one that isn't).
- **Replies.** `reply:` in a webhook event a package declares names an
  `Object` of the package's own (`package.wire_invalid` otherwise); on
  any other event `reply` is reserved (`type.reserved_field_name`), so it
  is never a field. The connection's `replies:` keys each event needing a
  reply by its name in lower_snake_case, `incoming_call:`, naming an
  action `<domain>.<action>` (any domain; the app's `use` line for it, as
  `on_first_sign_in:`'s) that takes the one event and returns the reply
  type: a key no such event has is `connection.key_invalid`, a value that
  isn't such an action `connection.value_invalid`, and an event needing a
  reply with none `connection.key_missing`. Subscribing to such an event
  is `subscriber.event_invalid`: it isn't kept in the outbox.
- **Answering.** After the signature and the body are read, the event's
  id is claimed in `jadpo_received_events` (its new `reply` column empty)
  in a statement of its own; the action runs as the system, reaching no
  tenant's records, in its own transaction, as
  `on_first_sign_in:`'s does; its reply is encoded with
  the reply type's codec on the provider's wire and sent `200` as
  `application/json`, and kept over the claim once the action has
  committed. A repeat that finds the reply is answered with it, one that
  finds a claim younger than a minute `409 conflict` ("The webhook is
  still being answered."), and a claim older than that, left by a process
  that stopped, is taken over. A failure gives the claim back, so the
  provider's retry runs the action again: a declared failure answered as
  a route answers it, `unavailable` `503`, a fault `500`. Keeping the
  reply follows the action's commit rather than joining it, since the
  action may write another store: if that statement fails, a retry after
  the claim lapses runs the action again.
- **What a reply may show.** What only `Access.public` lets the reply
  action in to is `policy.system_grant_missing` at its `replies:` entry.
  The reply goes back to whoever sent the webhook, so each field it
  carries, or that decided it, is one anyone at all may read
  (`policy.output_field_unproved`), a record the action created holding
  what it set. Whether records are there reaches the sender too: a reply
  counting or testing for records only the system may read, or a failure
  decided by it, is refused the same way, and the action's failure,
  answered to the sender as its reply is, is held to the same proof, what
  decides it and its client fields
  ([Field restrictions](09-policy.md#field-restrictions)).

> Graph's own notification check, its `clientState` in the body, isn't a
> signature scheme yet. Twilio's TwiML replies wait for an XML byte
> format.


## Package actions and recipes
<!-- topic: package-actions-and-recipes -->

This is the detail behind a package's actions, in
[What package code may do](#what-package-code-may-do), and its
[Recipes](#recipes): what a package action may call, how the app calls
one, and how `jadpo package recipe` copies a recipe into your app.

- **Calling its own API.** In a package's action, `<resource>.<operation>(input)`
  calls one of its package's operations, public or not, by its bare names;
  a function or a test of the package calling one is
  `effect.function_persistence`. The operation's failures are the
  package's own, so they pass on to the action's `fails` with `attempt`
  as a library function's do, or are handled with `match`. At run time
  the call goes through the connection the app called the action
  through, with its credential, host, timeout and fake.
- **No other action.** A package's action calling another action, its
  package's or any other, is `package.action_invalid`. A package action
  calls only its own package's operations and pure functions.
- **No setting and no sign-in.** A `config.<name>` read anywhere in a
  package's code but its tests, a key given to a `crypto` call's `key:`
  included, is `package.action_invalid`: a package reads none of the
  app's settings, so it can neither send one to its API nor sign or
  decrypt with the app's keys; what its provider needs reaches it
  through the connection's `credential:`. So is a call of `sessions.*`,
  `magic_links.*` or `test_passkeys.*` in a package's code: who is
  signed in is the app's own.
- **The app's call.** `connection.<action>(...)` calls a `public` action
  of the connection's package, from an action of a domain its
  `domains:` lists (`connection.domain_not_listed` otherwise), with the
  action's parameters as written; one without `public` is
  `package.not_public`. Its package failures are mapped at the call by
  their codes, as an operation's are (`failure.attempt_required` when
  nothing maps them), and one no arm maps is a fault; its `attempt` may
  not map `timed_out:`, since its calls are several, or may be. An
  unknown name through the connection lists its public operations and
  actions.
- **Steps.** A package action's changing calls count as the calling
  action's on its longest way through, and the keys the runtime derives
  carry on the caller's step numbering, so a second package action, or
  the app's own call after one, continues it. A package action over 8 is
  `step.too_many` where it is declared, which `jadpo package check`'s
  `compiles` row shows, so it can't be published.
- **Emitting.** `emit` in a `public` package action is
  `package.action_invalid`; one without `public`, which only the
  package's subscribers run, emits the package's own events ("Package
  subscribers and recipes in `sample/`").
- **A resource and an action of one name** is `package.action_invalid`
  at the action.
- **The surface.** Each `public` action is an entry after the
  operations, `action <name>` with its file, its `signature` and a `fails`
  line per failure, compared between versions as a function's is.
- **Fakes.** A test's `fakes:` may script any of the package's
  operations, public or not, when the package has actions, since those
  reach the ones it keeps to itself; a package action runs its real code
  in a test, as every package's code does. The provider plan holds every
  operation of a connection's package.
- **`jadpo package recipe <package> <recipe> --into <domain>`** finds the
  package as `jadpo package show` does and copies every file under its
  `recipes/<recipe>/`, hidden ones left out, to the same path under
  `domains/<domain>/`, which must be one of the app's domains. It copies
  all or nothing: a file the domain already has is never written over,
  and naming one refuses the copy with `cli.package_arguments`. The files
  are copied as they are; `jadpo check` then shows what to fit to the app.
  The screen lists each file written, the footer counts them, and the
  hint is `jadpo check`, designed with the CLI design language:

```text
Copied recipe track_parcels into domains/shipping
    domains/shipping/actions/track_parcel.jadpo
    domains/shipping/schedules/track_nightly.jadpo

recipe copied: Couriers.v1, track_parcels, 2 files

next: jadpo check  shows what to fit to the app
```

## Package subscribers and recipes in `sample/`
<!-- topic: package-subscribers-and-recipes-in-sample -->

What a package's own subscribers hear and emit, and how
`jadpo package check` compiles a package's recipes in its `sample/` app.

- **What a package's subscriber hears.** A subscriber in a package's
  `subscribers/` listens with `on:` for an event its webhooks bring: one
  of the package's events with `wire_name:`, its provider declaring
  `webhooks:`, public or not. Any other event, the package's own events
  among them, is `subscriber.event_invalid`, so what a package makes of a
  webhook is for the app's subscribers. Its `run:` calls one of the
  package's own actions with the event or its fields
  (`subscriber.run_invalid` otherwise); `retry:` is as any subscriber's.
- **Through the connection it came in on.** A package's subscriber hears
  each connection that gives its package `webhooks: { secret: }`, and
  only the events that came in on that connection: one delivery each,
  named `subscriber <Package>.<name> <connection>` (`subscriber
  AcmePay.renewal payments`), so a package connected twice runs its
  subscriber once per webhook, through the connection the webhook named.
  Its action's operations go through that connection, with its
  credential, host, timeout and fake; its changing calls are the
  delivery's steps, keyed by the delivery, as a subscriber's action's
  are; and a delivery is held while that connection's circuit breaker is
  open.
- **What a package's action emits.** An action without `public`, which
  only the package's subscribers run, emits the package's own events. An
  event a package's action emits and that gives no `wire_name:` is the
  package's own, and needs neither `wire_name:` nor `occurred_at`;
  `wire_name:` marks an event the provider sends, and emitting one is
  `package.action_invalid`, since only a signed webhook brings it. An
  event with neither `wire_name:` nor an action emitting it is still
  `package.wire_name_invalid` in a package that receives webhooks. What
  it emits is kept where the package's webhooks are, `events: { store:
  }` or the only store of the app's records, and reaches the app's
  subscribers of its public events; a package's subscriber never hears
  it, and one without `public` reaches no one.
- **Recipes in `sample/`.** `jadpo package check` compiles each recipe on
  its own, copied into the one domain of the package's `sample/` app
  (its `app.jadpo` and `domains/`), with the package placed as the
  sample's own integration, as the bar places it for its own compile, so a
  recipe names the package as an app's own integration does; the
  migrations and records a recipe would need aren't asked for, since an
  app writes its own once it copies the recipe. The `recipes` row reads
  `1 recipe compiles in sample/`, or which didn't; a recipe's problem is
  shown at `recipes/<recipe>/<file>` and a problem of the sample's own at
  `sample/<file>`, once. A package with recipes and no `sample/app.jadpo`,
  or a sample with no domain or several, is `publish.sample_missing`.
  The row isn't shown for a package with no recipes, and waits when the
  package has errors. A recipe's own tests run in `sample/` once a recipe
  has some.

## How polls and `max_concurrent:` run
<!-- topic: how-polls-and-max-concurrent-run -->

This is the detail behind [Polls](#polls) and a connection's
`max_concurrent:`: what a poll declares, when it runs and what it keeps,
and how the cap on calls at once is counted.

- **A poll's keys.** Each entry of `polls:` in `provider.jadpo` is a
  block of `call:`, `every:` and `emits:`, each required and nothing
  else (`package.provider_invalid`). `call:` names one of the package's
  operations, public or not, that takes no `input:` and isn't `changes:
  true`, since a poll reads a feed that doesn't depend on the app's data
  (checking each of the app's records is the app's schedule calling the
  package), and whose `output:` is an `Object` read by the package's
  types, with no `decode:` of its own, as a queue's mapping is. `every:`
  is a duration of a minute or more: the tick runs once a minute.
- **What it emits.** `emits:` names an `Event` of the package that is
  `public`, since only the app's subscribers hear it, and gives no
  `wire_name:`, which marks one the provider sends; it is the package's
  own, as one its action emits is, so a package that receives webhooks
  needs no `wire_name:` or `occurred_at` on it. Each of its fields is a
  field of the answer of the same name and type, and only those count
  when the runtime asks whether the answer changed: a time the answer was
  fetched, left out of the event, never does. An event of fields the
  answer hasn't is refused at `emits:`.
- **The connection's interval.** `polls: { <name>: <interval> }` on a
  connection gives one of its package's polls another interval, a
  duration of a minute or more; a name the package's `polls:` hasn't is
  `connection.key_invalid` with the nearest, a shorter or unwritten
  interval `connection.value_invalid`, and `polls:` on a connection to a
  package with none `connection.key_invalid`.
- **Only while subscribed.** A poll runs for each connection to its
  package while one of the app's subscribers listens for its event, and
  is left out of the plan otherwise, so nothing is called and no state
  kept. Its events are kept where the package's webhooks would be, in
  `events: { store: }` or the only store of the app's records
  (`event.store_required` at the connection otherwise), with
  `jadpo_polls`, by `poll <connection>.<poll>`: when it is next due, the
  digest of the event it last kept and when that changed.
- **Running it.** On each tick, after the queues, one statement takes a
  poll that is due, or has never run, and moves it on by its interval, so
  of several instances only one calls it; one due later than a whole new
  interval away, since shortened, is due at once. The row keeps the
  interval it was last claimed under, `jadpo_polls.every_ms`, so only a
  poll claimed under another interval than the connection's now counts
  as shortened, and a second tick at the same moment, or an instance
  whose clock is behind, never calls the provider again within the
  interval. The column may be `NULL`, since a migration adds it to a
  table that may hold rows, and a row without it counts once as claimed
  under another interval. The operation is called
  as the runtime's own work, through the connection, with no request and
  no function of the package's, logged and retried as any call is. The
  event is made from the answer's fields, written with its codec and
  read back, and the SHA-256 of what is written compared with the last
  digest: the same keeps nothing; a change, or the first answer, keeps
  the event with a delivery for each subscriber and the digest in one
  transaction, the digest checked again there. A failed call keeps
  nothing, is reported by the tick under the poll's name, and is due
  again a minute later. A poll's first answer is emitted, so the app
  learns the current prices without waiting for them to change.
- **`max_concurrent:`** in `provider.jadpo` is a whole number of at
  least 1 (`package.provider_invalid`), and a connection's may only lower
  it (`connection.value_invalid` past it); a connection may give one when
  its package gives none. The cap is per app instance: the process's,
  shared by its worker threads, or an isolate's on Workers, and a test's
  host keeps its own. A provider call takes one of its connection's slots
  for each attempt and gives it back once answered, so none is held while
  a retry waits; a further call waits for one, a route's within its
  deadline. A subscriber's delivery calling out through a connection
  whose slots are all out is given back as claimed, its attempt not
  counted, due again a second later for the next sweep; its wait counts
  towards `within:`, as a release after a shutdown does. Polls' calls
  wait as any call does.
- **On Workers** the cap is counted per isolate: Workers runs many
  isolates, and a cap across them would need a Durable Object and a round
  trip per call. So the deploy report and `jadpo check` say "on Workers,
  per instance" (`jadpo check`'s "On Workers" lists each capped
  connection a deployable built for Workers reaches, and `jadpo build`'s
  `calls` line each Worker's); a provider that needs a hard global limit
  enforces it on its side, and its rate-limit answers already make calls
  back off.
- **Between an isolate's invocations** a slot one invocation gives back
  is handed to the call of another that has waited longest, which finds
  it on its own timer every 10 ms rather than being woken by the first,
  since Workers runs each invocation's work in its own context. A call
  holding a slot says so every second, and one not heard from for five
  seconds is forgotten, as is a waiter that stopped looking, so an
  invocation Workers ended mid-call never keeps a slot. Natively the
  process's waiters are woken by the call that gives a slot back.
- **The surface.** A poll's event is one of the package's public types
  and in its surface as any is; the polls themselves and
  `max_concurrent:` join the provider's entry once the surface tells an
  addition inside an entry from a breaking change, since any change to
  the provider's entry counts as breaking.

## Enum values and failures through a package
<!-- topic: enum-values-and-failures-through-a-package -->

How your app writes a package's enum values and failures: through the
package's name, as its types are.

- **One path for every name through a package.** An enum's value is
  written through its package as its type is, `AcmeCouriers.Window.morning`,
  and through the package and its version while two are held,
  `Postcodes.v3.Round.morning`; the version is missing or needless exactly
  as for a type (`package.version_needed`, `package.version_unneeded`), and
  the type must be one the package makes public. The same holds wherever a
  value is written: in a body, a constant, a test's `fakes:`, a variant
  built with its fields (`Postcodes.v3.Round.shared { ... }`) and a
  `match` arm's pattern, whose names resolution checks as a value's; a pattern resolution refuses leaves
  its arm's case unknown, so the `match` isn't also reported as not
  covering it. The enum written alone, `Postcodes.Round`, is
  `type.not_a_value` naming the enum, not the package.

- **A package's failure in an outcome `match`.** An app's `match` on a
  provider call, a package action or a library package's function names
  the package's failure in its arm as `fails` and `attempt` write it,
  through the package, `AcmeCouriers.SlotTaken ->`, and through the
  package and its version while two are held,
  `Postcodes.v3.InvalidPostcode ->`; an arm it misses is named the same
  way. The version is missing or needless exactly as for a type, and an
  arm resolution refuses leaves its failure unknown, so the `match`
  isn't also told it misses one. Inside its own code a package still
  names its failures bare.

## What is built
<!-- topic: external-services-built (was errors-still-to-build) -->

| Feature | Status |
|---|---|
| The package format, names and the lock | Built. |
| The surface, `jadpo package show` and `jadpo package check` with the publishing bar | Built. |
| Installing: `add` from a folder or git, `update`, `remove`, `link` and `unlink` | Built. |
| Connections to packages and the network report | Built. |
| Lazy loading, the package cache and two versions side by side | Built. |
| The OpenAPI importer, and the scoped review of an import's types | Built. |
| Provider calls, decoding, `wire_parts:` and retries | Built. |
| Walking a list operation's pages with `batches.each` ([Pages of results](#pages-of-results)) | Not built yet: a provider's list operation as `batches.each`'s `from:` is `toolchain.not_built_yet` where it is written ([Jobs and events](12-jobs-and-events.md#walking-records-or-a-providers-list-batcheseach)). |
| No transaction open during a call, and several changing calls in one action | Built. |
| Fakes, recordings and provider calls faked in development | Built. |
| The `email` capability with SMTP and Resend | Built. |
| Package actions, subscribers, recipes and polls | Built. |
| Webhooks as events, replies, handshakes, queues through a package and the Cloudflare Queues driver | Built. |
| Services (`runs:`) | Not built yet. |
| OAuth grants | Not built yet: the application's `grants:` is `toolchain.not_built_yet` where it is written. |
| Which personal data a package's operations send, and to which hosts, in `jadpo package check` ([Where personal data goes](15-personal-data.md#where-personal-data-goes)) | Built. The other facts under [What a release shows](#what-a-release-shows) come with the registry. |
| Publisher trust for third-party git packages | Built: every package from git needs an [acceptance](18-records-and-manifest.md#acceptances). Not built yet: `packages:`'s `own_sources:`, which lets your own company's repositories in without one. |
| The registry protocol: its requests and answers, the log and its proofs, a local registry and the conformance test | Built (`jadpo-registry`). |
| The public registry: the verifier, publishing, facts, search, advisories and compatibility runs | Not built yet. |
| Importers other than OpenAPI | Not built yet. |
| Choosing a provider when deploying | Not built yet. |
| Erasure at providers | Not built yet: it comes with [Personal data](15-personal-data.md). |

[The roadmap](../roadmap.md) says when each comes.

### Mistakes not reported yet

These mistakes are specified but not reported yet:

| Mistake | Reported by |
|---|---|
| A connection that receives personal data, with no `erasure:` and no acceptance | `jadpo check` |
| An operation whose scope the grant does not request; a call to a grant connection without `on_behalf_of:` | `jadpo check` |
| A service image not yet accepted; an `asks:` host not granted by `outbound:` | `jadpo check`, naming the line that grants it |
| A capability's connection whose package does not provide it, or a function call or expression in a `provide` mapping, for a capability other than `receiving` and `email` | `jadpo check` |

The rest of the page's mistakes have examples under
[Errors](#errors): the digest and the lock, an `emails/` folder, the
index's keys, `provider.jadpo`'s keys, `hosts:` with `runs:`, reserved
and taken names, a resource and an action of one name and a public
action that emits (`package.action_invalid`), and subscribing to a
non-public event (`package.not_public`) under "Package errors"; a
surface that differs at install and a link in CI under "Installing
errors"; a decoder that fails on a recording and a field's `decode:` and
`encode:` that don't round-trip under "Recording errors"; a connection's
keys under "Connection errors". The rule that nothing public reaches
something private is `visibility.public_reaches_private`
([Project layout and modules](01-project-layout-and-modules.md)), and
several changing calls in the wrong place are the `step.` errors of
[Jobs and events](12-jobs-and-events.md) and `effect.call_in_loop`
([Logic](03-logic.md)).

## Errors

### Package errors

> Mistakes in the package format, its names and the lock. Each example
> is checked as an installed package in `packages/` or as the app's own
> integration in `shared/providers/`.

An installed library package, locked by its digest, used through its name
from the app's own code:

```jadpo check project=p71-library file=app.jadpo
application {}
```

```jadpo check project=p71-library file=packages/postcodes/v2/package.jadpo
/// UK postcodes: reading them as people write them.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-library file=packages/postcodes/v2/types/uk_postcode.jadpo
/// A UK postcode in standard form, such as "SW1A 1AA".
public type UkPostcode = Text {
    min_length: 5
    max_length: 8
}
```

```jadpo check project=p71-library file=packages/postcodes/v2/types/entered_postcode.jadpo
/// A postcode as someone typed it.
public type EnteredPostcode = Text {
    max_length: 20
}
```

```jadpo check project=p71-library file=packages/postcodes/v2/failures/invalid_postcode.jadpo
/// The text isn't a UK postcode.
public type InvalidPostcode = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=p71-library file=packages/postcodes/v2/libraries/uk.jadpo
/// Reading UK postcodes.
public library uk {
    /// Reads a postcode in upper case.
    function tidy(entered: EnteredPostcode) fails InvalidPostcode -> UkPostcode {
        return attempt UkPostcode(text.upper(entered)) {
            invalid: InvalidPostcode
        }
    }
}
```

```jadpo check project=p71-library file=domains/intake/actions/read_postcode.jadpo
use packages/postcodes

/// Reads a customer's postcode.
action read_postcode(
    entered: Postcodes.EnteredPostcode
) fails Postcodes.InvalidPostcode -> Postcodes.UkPostcode {
    return attempt Postcodes.uk.tidy(entered)
}
```

```jadpo check project=p71-library file=domains/intake/actions/read_postcode.test.jadpo
use packages/postcodes

test "a postcode is read in upper case" {
    var postcode: Postcodes.UkPostcode = attempt read_postcode("sw1a 1aa")
    assert postcode == "SW1A 1AA"
    assert read_postcode("x") fails Postcodes.InvalidPostcode
}
```

```jadpo check project=p71-library file=jadpo.lock
// Generated by Jadpo from the installed packages; update it with `jadpo package add`, `update` or `remove`, never by hand.
record lock 1

package Postcodes.v2 version 2.1.0 path ../postcodes digest sha256:090b5925bc82196d1f2d740b45033e73c6c61db327970997997fb30c21d1b939
```

The app's own integration in `shared/providers/`, in exactly the package
format, needs no lock:

```jadpo check project=p71-own file=app.jadpo
application {}
```

```jadpo check project=p71-own file=shared/providers/acme_couriers/v1/package.jadpo
/// Acme Couriers' booking API, written from its documentation.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=p71-own file=shared/providers/acme_couriers/v1/provider.jadpo
/// Acme Couriers' booking and tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
    credential: CredentialScheme.bearer
    idempotency: {
        header: "Idempotency-Key"
        window: 24h
        max_length: 255
        in_use: "idempotency_key_in_use"
        reused: "idempotency_key_reused"
    }
    error_code: "error.code"
}
```

```jadpo check project=p71-own file=shared/providers/acme_couriers/v1/resources/bookings.jadpo
public resource bookings {
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
```

```jadpo check project=p71-own file=shared/providers/acme_couriers/v1/types/create_booking.jadpo
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Text
    slot: Text
}
```

```jadpo check project=p71-own file=shared/providers/acme_couriers/v1/types/booking.jadpo
/// A booked collection.
public type Booking = Object {
    id: Text
    collect_at: Instant
}
```

```jadpo check project=p71-own file=shared/providers/acme_couriers/v1/failures/slot_taken.jadpo
/// The collection slot was taken first.
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "The collection slot is no longer available."
}
```

- [`package.index_invalid`](../errors/package.index_invalid.md)
- [`package.name_mismatch`](../errors/package.name_mismatch.md)
- [`package.emails_folder`](../errors/package.emails_folder.md)
- [`package.provider_invalid`](../errors/package.provider_invalid.md)
- [`package.provider_missing`](../errors/package.provider_missing.md)
- [`package.provider_translations`](../errors/package.provider_translations.md)
- [`package.reserved_name`](../errors/package.reserved_name.md)
- [`package.name_taken`](../errors/package.name_taken.md)
- [`package.not_public`](../errors/package.not_public.md)
- [`package.action_invalid`](../errors/package.action_invalid.md)
- [`package.not_locked`](../errors/package.not_locked.md)
- [`package.not_installed`](../errors/package.not_installed.md)
- [`package.digest_mismatch`](../errors/package.digest_mismatch.md)
- [`package.licence_not_allowed`](../errors/package.licence_not_allowed.md)
- [`package.version_needed`](../errors/package.version_needed.md)
- [`package.version_unneeded`](../errors/package.version_unneeded.md)
- [`package.operation_invalid`](../errors/package.operation_invalid.md)
- [`package.wire_invalid`](../errors/package.wire_invalid.md)
- [`package.wire_name_invalid`](../errors/package.wire_name_invalid.md)
- [`package.standard_candidate`](../errors/package.standard_candidate.md)


### Installing errors

> The installing commands report these, and change
> nothing when they do; `jadpo check` and `jadpo test` report a link in
> CI. Like the publishing errors below they come from running a command,
> so [Command errors](14-tooling.md#command-errors) lists them too.

- [`install.surface_mismatch`](../errors/install.surface_mismatch.md)
- [`install.fetch_failed`](../errors/install.fetch_failed.md)
- [`install.in_use`](../errors/install.in_use.md)
- [`install.link_refused`](../errors/install.link_refused.md)


### Recording errors

> `jadpo test` reads every recording of the app's
> own integrations as its operation's output, its placeholders filled,
> through the package's own decoders, each as a test of the `recording`
> kind ([Kinds of test](13-testing.md#kinds-of-test)), and the publishing
> bar does the same as its `decoders` row. `jadpo package record` reports
> `recording.refused` and `recording.failed`, which
> [Command errors](14-tooling.md#command-errors) lists.

- [`test.recording_undecodable`](../errors/test.recording_undecodable.md)
- [`test.round_trip_failed`](../errors/test.round_trip_failed.md)


### Publishing errors

> `jadpo package check` reports these in a package's own repository, every
> path relative to the package's folder; `jadpo check` never does, since
> the app's own integrations skip the bar.

- [`publish.description_missing`](../errors/publish.description_missing.md)
- [`publish.breaking_change`](../errors/publish.breaking_change.md)
- [`publish.example_missing`](../errors/publish.example_missing.md)
- [`publish.example_failed`](../errors/publish.example_failed.md)
- [`publish.sample_missing`](../errors/publish.sample_missing.md)
- [`publish.changelog_missing`](../errors/publish.changelog_missing.md)


### Connection errors

> A connection to a package is checked against what the package takes
> and needs.

- [`connection.package_unknown`](../errors/connection.package_unknown.md)
- [`connection.key_invalid`](../errors/connection.key_invalid.md)
- [`connection.key_missing`](../errors/connection.key_missing.md)
- [`connection.value_invalid`](../errors/connection.value_invalid.md)
- [`connection.domain_not_listed`](../errors/connection.domain_not_listed.md)


### Time limit errors

- [`route.calls_past_deadline`](../errors/route.calls_past_deadline.md)
