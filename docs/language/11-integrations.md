---
title: External services
area: 11
status: decided
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
  - package.*
---

# External services

An external service is another company's API (Stripe, a courier, another
Jadpo app) that an action calls. In Jadpo, each one is described once, as a
**provider package**: Jadpo source that lists the API's operations, types,
failures and webhook events, and says which calls change something, which
take an idempotency key, how its values are written on the wire, how errors
and pages come back and how webhooks are signed. An app connects to a
package in `app.jadpo` and calls its operations like an SDK. The compiler
owns the HTTP request, the secret, the host, the retries and the reading of
the response; application code never builds a URL, sets a header or sees a
raw response. The other kind of package, a **library package**, carries
types, failures and pure functions that several apps share.

> Reopened on 2026-10-13. Area 11 was decided on 2026-10-12 around
> per-domain `integration` declarations; Richard agreed to rethink it around
> provider packages ([the rethink](research/11-integrations-rethink.md)).
> Questions 6, 7 and 8 kept their answers; the rest were decided again on
> 2026-10-13.
>
> Revised on 2026-10-16 (Richard), from
> [packages and extension points](research/packages-and-extension-proposals.md)
> and its research ([package layouts](research/package-layout-options.md),
> candidate B; [capability providers](research/capability-providers-options.md),
> design C; [multi-step calls](research/multi-step-calls-review.md), adopted
> with its ten fixes): the two kinds of package, the package format, where
> packages live, the surface and the publishing bar, what package code may
> do, decoding, capabilities as only the jobs the language does itself,
> receiving from outside, several changing calls in one action, OAuth grants
> and packages from other Jadpo apps. The open questions below carry
> *Revised 2026-10-16* notes where an answer changed. Nothing here is built
> yet: "Status" says which chunk builds what.

| Topic | In short |
|---|---|
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

## Kinds of package

Decided 2026-10-16 (Richard). There are two kinds, in one format:

| Kind | Contains | Used as |
|---|---|---|
| Provider | One API at one version: resources, operations, types, failures, events and how the API is reached; optionally package actions, subscribers, capability mappings and library functions | A connection: `stripe.payment_intents.create(...)` |
| Library | Types, failures and library functions under `shared/`'s rules: pure, no entities, no routes, no network | `Postcodes.uk.parse(text)`, `Postcodes.UkPostcode` |

- **The kind follows from what is inside.** A package with `resources/` is
  a provider package and needs a connection to call its operations; one
  without is a library package. A provider package's library functions are
  callable without a connection.
- **No feature packages.** A whole slice of an app (invitations, billing) is
  not a package, installed or copied in. An agent writes it from these
  pages, the compiler catches the dangerous mistakes, and most features
  other frameworks take from packages (sign-in, flags, audit, files, email)
  are built in. Where agents keep getting one wrong (chunk 10.2 measures
  this), the fix is a worked reference in `examples/`.
- **No driver packages.** Anything that needs native code (a new database,
  a signing scheme, a wire format) comes only through a Jadpo release.
  Packages never carry native or WebAssembly code.
- **Library types may be stored** in entity fields and used in route inputs
  and outputs. An upgrade that changes a stored type appears in the
  upgrade plan and the migration, as a change to the app's own type does.
- **Everything in a provider package follows the provider rules**, its
  library types included: only ids and plain values may be stored, and no
  provider object or enum type is part of a route's `Input` or `Output`
  (question 1), because the next version of the API is a new package. What
  an app keeps from a provider it stores as ids, plain values and built-in
  types (money, countries, phone numbers), or as its own types.
- **Built in or a library package** is decided by chunk 5.4 with one rule:
  a thing is built in when the compiler gains from knowing it (a type that
  turns a mistake into a compile error, carries personal-data marking, has
  a SQL or OpenAPI form or generated test values, or a security boundary
  the language should own). Money, countries, currencies, languages, phone
  numbers, safe HTML and CSV are built in; postcodes, IBAN, VAT numbers,
  public holidays, repeating dates and QR codes are packages.

### The golden rule

**A package can do nothing the app cannot, and the app can do everything a
package can** (Richard, 2026-10-16). A package is a folder of an app's own
code, published.

- It holds by construction: the app's own integrations in
  `shared/providers/` use the package format whole (`provider.jadpo`,
  `resources/`, `events/`, decoding, `provides/`, `runs:`), and its own
  types, failures, libraries, emails and translations live in `shared/` and
  its domains.
- The compiler treats `packages/` and `shared/providers/` alike; they differ
  only in provenance: the lock entry, the digest check, being read-only,
  and the publishing bar.
- Every future package feature is designed with how the app writes it; a
  proposal that only works inside a package is rejected.
- The gate checks it: each package example on these pages also compiles
  placed as the app's own integration in `shared/providers/` (from chunk
  7.1, when package examples become checked).
- The rule runs one way only: an app can do far more than a package
  (entities, actions over its data, routes, policy, emails).

## The package format

A package is laid out as `shared/` is, so one set of rules covers the app
and every package, and the compiler checks a package as it checks
`shared/` (candidate B of [the layout comparison](research/package-layout-options.md)).
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
- **No `emails/` folder**: email templates, partials and layouts are the
  app's own, and an `emails/` folder in a package is a compile error. The
  golden rule runs one way, so a package doing less than the app is
  consistent.
- **Translations** only in library packages, for their own messages and
  failures, laid out as a domain's (area 10, "Packages and strictness"); an
  app calls them as `Postcodes.translations.<message>(...)` and rewords
  them in its root `translations/`. Provider packages show users no text,
  since their failures are always mapped to the app's own, so they carry
  none.
- **`GUIDE.md`**, optional, says how to use the package well; every code
  example in it is compiled and run at publish, and
  `jadpo package show <name> --guide` prints it.

### The index: `package.jadpo`

Every index is the same: `package Name.vN { }` with a `///` description and
two keys, `licence:` and `jadpo:` (the lowest Jadpo version it needs).
The licence is one of `Licence.mit`, `.apache_2_0`, `.bsd_2_clause`,
`.bsd_3_clause`, `.isc`, `.mpl_2_0`, `.unlicense` and `.proprietary`;
only a package installed from git may be `.proprietary`, and the public
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

A package that describes an API has one `provider.jadpo`, a `provider { }`
declaration with one fixed set of keys; a package without one is a
library. One package describes at most one API, because one API is one
connection with one set of hosts and credentials: a vendor with two APIs
under different credentials (Shopify's Admin and Storefront APIs) is two
packages. The declaration is called `provider`, matching provider packages
and `shared/providers/`.

| Key | Says |
|---|---|
| `protocol:` | How bodies are sent: `Protocol.json_http`, `.form_http`, `.multipart_http`, `.aws_json`. Required |
| `hosts:` | `live:` and `sandbox:`; the same host twice when the sandbox is a test credential; a host may hold one placeholder filled from a setting (`"https://{project_ref}.supabase.co"`) |
| `runs:` | A container Jadpo runs instead of a fixed host: `image:` pinned by digest, `port:` it listens on inside the container, `asks:` the outbound hosts it needs (see "Services") |
| `credential:` | The scheme (`CredentialScheme.bearer`, a named header, basic, AWS signing), which decides the shape of credential the app gives |
| `oauth:` | The provider's authorize, token and revoke addresses, for a connection whose credential is an `OAuthGrant` (see "Acting for each user") |
| `idempotency:` | `header:` (where the key goes), `window:` (how long the provider keeps a key; required), `max_length:`, the provider's error codes `in_use:` (the key's first request is still running), `reused:` (the key came with a different request) and `new_key_on:` (the first attempt failed and the key cannot be used again), and `replays_failures: true` (default false) for a provider that replays a failed answer under the same key. Left out when the API takes no key |
| `error_code:` | Where the provider's error code is in a failed response |
| `webhooks:` | `check:` (the signature scheme), `event_id:`, `event_type:`, and `handshake:` where the provider needs one |
| `polls:` | Operations the runtime calls on a schedule, each with a default interval and the event emitted when the answer changes |
| `erasure:` | The operation that deletes a person at the provider and the key it takes (`by:`) |
| `max_concurrent:` | How many of its calls run at once per app instance |
| `settings:` | Typed values the app gives on the connection (`message_stream:`) |
| `imported:` | The description it was imported from, and that file's digest |

**Where an API is comes from exactly one place**: `hosts:` (it is on the
internet), `runs:` (Jadpo runs it), or neither, when the app gives `host:`
on the connection (another Jadpo app, a self-hosted product, a service on
Workers). A connection's `host:` beside `runs:` means the service is already
running there and Jadpo does not start it; a package with fixed `hosts:`
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

Inside the package, names are bare. An app names them through the package:
`AcmeCouriers.Booking`, `AcmeCouriers.SlotTaken` (question 16). A failure's
code (`slot_taken`, from its name) is the provider's error code, read from
the place `error_code:` names.

### What is public

- **`public` marks the package's API**, as it marks what a domain offers
  other domains. A `public resource` and a `public action` are callable
  through the connection; everything else only by the package's own
  actions, subscribers and polls. As in an app (area 1), `public` goes on a
  library, never on a function: every function of a `public library` is
  public, and internal helpers such as decoders live in a library without
  `public`. Internal declarations may change freely between versions. An
  importer marks every resource public.
- **Everything reachable from the public surface is public**, checked on
  every compile: a failure a public library's function `fails` with, a
  provider operation's `failures:` and the types in a public signature must
  themselves be public; internal failures are mapped or handled before
  they leave a public declaration. Area 1 applies the same rule to a
  domain's public actions, queries and libraries.
- **A package's resources and actions share the connection's namespace**
  (`stripe.customers`, `stripe.start_subscription`), so a clash between
  them is an error in the package.
- **Package events are public or not.** The app may subscribe to a
  `public` event; one without `public` reaches only the package's own
  subscribers, so a raw webhook a package enriches can stay internal while
  its enriched event is public. An importer marks webhook events public;
  subscribing to a package's non-public event is an error.

### No presets, no dependencies

**Built-in drivers are configured in `app.jadpo`, never packaged.**
PostgreSQL, Redis, S3, SMTP and OpenID Connect are drivers in the runtime.
Cloudflare R2 is `driver: Driver.s3` with its host and `region: "auto"`;
Neon is the PostgreSQL driver with its pooling mode; Gmail is the SMTP
driver with its host and port. A vendor with storage and an API is a driver
connection plus a package, so there is no hybrid case. A vendor driver
comes in a Jadpo release only where a flow needs native code.

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

- **Vendor settings are documented, so agents look them up**: each driver's
  page lists the settings for the common services (R2, Backblaze B2,
  DigitalOcean Spaces, MinIO, Amazon S3; Neon, Supabase, Amazon RDS; Gmail,
  Microsoft 365, Amazon SES's SMTP; Google, Microsoft), each a working
  connection checked with the pages' other examples, and `jadpo check` warns
  when it recognises a host that is missing one.
- **Sign-in providers are connections**: Google, Microsoft and other
  OpenID Connect providers are `google: { driver: Driver.oidc  issuer:
  client_id:  credential: }` in `connections:`, named by area 8's
  `sign_in: { providers: [google, github] }`; GitHub and Apple, which are
  not plain OpenID Connect, are the built-in `Driver.github` and
  `Driver.apple`.

**Packages have no dependencies**: a package cannot require another.

- The standard library carries the shared vocabulary (money, countries,
  currencies, phone numbers), which is what packages elsewhere mostly
  depend on each other for.
- What is left is copied: a package that needs another's code includes a
  copy, checked and tested by the publishing bar like the rest of it. Two
  packages' types for the same thing are converted at the call, usually in
  one line.
- It removes version resolution, clashes between packages, "required by"
  tracking and clean-up on removal: installing is one folder and one line
  in the lock. It is also the reversible choice; requirements can be added
  later without breaking anything published, if real packages keep copying
  something substantial that does not belong in the standard library.

## Where packages live

One rule, by who wrote the code rather than what kind it is: **`packages/`
holds only what was installed; everything the app's authors wrote lives
where their code already lives.**

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
- **Installed packages are read-only.** The compiler refuses one whose files
  do not match its digest in `jadpo.lock`.
- **The app's own integrations live in `shared/providers/<name>/<version>/`**,
  in exactly the package format, with everything an installed provider can
  have: types, failures, pure functions, decoding, recordings, fakes,
  `provide` mappings, actions, subscribers and services. Who may call one is
  still the connection's `domains:` list.
- **A `use` line shows where a name comes from**: `use packages/postcodes`
  is always installed code, and `use shared/providers/acme_couriers` the
  app's own. Installed or the app's own, the only differences are where it
  sits, whether it can be edited and whether the publishing bar applies, so
  replacing an own integration with an official package later changes
  `jadpo.lock`, the folder and the `use` lines, not the code that calls it.
- **Publishing moves code out of the app.** A package other apps depend on
  gets its own repository and versions, and every app installs it into
  `packages/`, the one it started in included.
- **Two versions can sit side by side**: a provider during an upgrade
  (question 3), a candidate version while the upgrade plan compiles the app
  against it, or two library versions while callers move across. While two
  are in use, names carry the version (`Postcodes.v3.UkPostcode`).

### Names

- **Provider brand names are reserved.** Jadpo ships the list of curated
  provider names, and an app's own integration cannot take one, so `Stripe`
  in code always means the official package.
- **Two packages never share a name in code.** Installing a registry package
  whose name one of the app's own integrations or libraries already uses
  stops and asks for the app's own to be renamed first (`jadpo rename`).
- **A package's name joins the domain, library and standard-library
  names,** which are unique among themselves (area 1), so installing a
  package refuses one whose name the app already uses for a domain, a
  library or any declaration (2026-10-16, Richard).
- Creating an integration checks the registry and warns when the name is
  taken.
- A package is named in UpperCamelCase with its own version label
  (`Stripe.v2024_06_20`, `Postcodes.v2`); its folder is the name in
  lower_snake_case with the version below it (question 16).

## Where packages come from

`jadpo package add <name>` follows a fixed order and records which step it
used (question 13), so an agent never improvises where to look:

1. **The registry**, the official package first.
2. **The provider's published specification** (OpenAPI, Smithy, GraphQL),
   given a URL or found in its known places, imported once.
3. **An importer** from the official SDK's type definitions, a Postman
   collection or a HAR capture.
4. **Written from the provider's documentation** by an agent.

It also takes a git repository (`jadpo package add
github.com/acme/acme-pricing@v2.1.0`, below) and another Jadpo app's
OpenAPI document (`jadpo package add ../orders`, see "Packages from other
Jadpo apps"). Each source is read once, when the package is added or
updated, then pinned in `jadpo.lock` with its source and digest.

**The registry** is Jadpo's own, in line with area 8's rule that the language
owns a feature where it reasonably can. It holds Jadpo packages of both
kinds.

- **Jadpo source only.** A package carries no native code and no install
  script, and nothing in it runs when it is installed; its code runs only
  when started (see "What package code may do"). A malicious package could
  still name the wrong host or mark an unsafe call idempotent, which is why
  hosts, services and every package change appear in the approval report.
- **Checked on upload.** The registry repeats every install check, adds
  discovery, curated packages marked official, publisher verification and
  signing, and renders each package's documentation website from its
  surface, so every package is documented the same way.
- To start, a few curated packages (Stripe, Twilio and GitHub, imported
  from their published OpenAPI) live in the Jadpo repository; the hosted
  registry comes later (question 15).

**An importer.** `jadpo package import <format> <file>` turns a published
description into the app's own integration once, offline, in
`shared/providers/<name>/<version>/`:

| Format | Typical source | What it gives |
|---|---|---|
| `openapi` | Most REST APIs (Stripe, Twilio, GitHub), and every Jadpo API | Operations, types, error responses; from a Jadpo app, exact (its `x-jadpo` fields) |
| `smithy` | AWS, one model per service | The same, plus AWS's protocols and signing |
| `postman` | A provider's published Postman collection | Operations and example bodies; types inferred from examples |
| `har` | A browser or proxy capture of real traffic | Operations and real responses; values stripped, shapes kept |
| `dts` | The provider's official TypeScript SDK (`.d.ts` files) | Exact types and SDK method names; no HTTP method or path, so the agent adds those from the docs |

An agent then fills what the source leaves out: which calls change
something, idempotency, error names, and decoding (OpenAPI often says only
`integer` for Unix seconds). A person reviews that diff. Corrections to a
wrong specification live in the package and survive the next import,
because the importer merges rather than overwrites, as an
[OpenAPI Overlay](https://spec.openapis.org/overlay/v1.0.0) does.

**Written by hand: the no-spec path.** Many APIs publish only HTML docs. A
package is Jadpo source, so for these an agent writes the package files
directly from the docs, in exactly the format an importer produces. Nothing
else changes: the same compiler checks, the same fakes, the same review. A
package needs only the operations the app uses, so it can start with one
operation and grow.

### Mapping imported types to standard types

> Decided 2026-10-17 (Richard). Built with the importer (chunk 7.2).

An imported description says little about what a field *is*: OpenAPI's
`integer` may be pence, Unix seconds or a count. Area 19 decides that a
package's public types use the standard types (`type.standard_exists`), so
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
   "gbp"` decodes to `Money`), so the app only ever sees the standard type.
3. **Findings on what was flagged.** Every candidate the importer is not
   certain of is a declinable error on that field, whose `next:` says what
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

### Provenance and confirmation

Every operation says where it came from, so a reviewer knows how far to
trust it (question 14):

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
- **Publisher trust**: the app's own company's packages from git install as
  any other; a third party's package from git, not the registry, is marked
  unverified in the approval report and needs a person's acceptance to
  install; registry packages carry their verified publisher, curated ones
  marked official.
- **Prose is unverified wherever it comes from** (`///` descriptions,
  `GUIDE.md`); nothing safety-relevant relies on it.

> **Built (chunk 7.1b2).** `jadpo package add` from a folder (`../postcodes`)
> or from a git repository at a release tag
> (`github.com/acme/postcodes@v2.1.0`): the package is compiled as the
> app's own integration would be, the surface its code gives has to be its
> committed `surface.jadpo_record` byte for byte (`install.surface_mismatch`),
> and only then is it copied into `packages/<name>/<version>/` and locked
> with its version, digest and source. `update` moves one to its newest
> release with area 18's review of what changed, `remove` takes out one the
> app no longer uses (`install.in_use`), and `link` and `unlink` are under
> "Developing and publishing a package". The registry, another Jadpo app's
> OpenAPI and publisher trust come later; the choices are in "Chosen in
> chunk 7.1b2" below.
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

## The surface

The compiler works out a package's public surface and writes it to
`surface.jadpo_record`, beside `package.jadpo`: the package's description,
licence and version, every public type with its fields and constraints,
every failure with its kind and code, every public library's functions with
their signatures, failures and descriptions, every recipe, and for a provider its operations (wire and
output types both), each public action with the operations it calls on
each path, which of them change something and which failures can follow
an effect, its hosts or container, credential scheme, settings, polls and
the capabilities it provides.

- **Readable, in the format of the API record** (area 7), not JSON, and
  starting with a line saying what wrote it, like every `.jadpo_record`
  (area 18). Installed packages are committed, so every upgrade's pull
  request carries the surface diff, and a reviewer reads "removed
  `bookings.cancel`, new failure `PostcodeNotServed`" in plain text.
- **Every entry points at its file** (`types/booking.jadpo`), so an agent
  reads the surface first and opens only what it needs. A file is named
  after what it declares, so the pointer never goes stale.
- **Kept out of `package.jadpo`**, which a person writes; the descriptions
  stay in the source as `///` comments, where the author edits them.
- **`jadpo package show <name>`** prints the description, licence, version,
  kind and public surface, and `jadpo package show <name> <declaration>`
  one operation, type or function with its description and file; `--guide`
  prints `GUIDE.md` and `--json` gives tools and agents the same. It works
  for installed packages and the app's own integrations alike. Its screens
  follow the CLI design language and are designed before they are built.
- **Generated, never written.** `jadpo package check` writes it; `add`,
  `update` and the registry refuse a surface that does not match the code.
  The surface of an app's own integration is generated into `build/`.
- **It decides the version.** Publishing compares the new surface with the
  last release's and refuses a breaking change (a public name removed, a
  field retyped, a new failure) without a new major version, as Elm's
  package manager does. The upgrade plan diffs the same two files.

> **Built (chunk 7.1b).** The surface as `surface.jadpo_record` (`record
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
> `add` and `update` verify the surface at install (chunk 7.1b2, under
> "Installing safely"); the choices are in "Chosen in chunk 7.1b" below.

## Developing and publishing a package

- **A package lives in its own repository.** `jadpo package new <Name>
  --library` or `--provider` writes the package format plus `recipes/`, a
  `sample/` app the recipes and tests run against, and `GUIDE.md`.
- **Developing**: `jadpo test` runs the package's tests, the examples in its
  descriptions, its decoders against its recordings and its recipes
  compiled into `sample/`; `jadpo package record` and `conform` work
  against the provider's sandbox; `jadpo package check` runs the whole
  publishing bar locally, writes `surface.jadpo_record`, and compares it
  with the surface at the previous tag.
- **Trying it in an app before publishing**: `jadpo package link
  ../acme-pricing` compiles the app against the live checkout and records
  the link in the lock; `jadpo build` for production and CI refuse a link,
  so it can never ship; `jadpo package unlink` restores the installed
  version.
- **Private packages come from git**: `jadpo package add
  github.com/acme/acme-pricing@v2.1.0`, a version being a git tag that the
  package's CI tags only after `jadpo package check` passes. Access is the
  developer's or CI's own git access, with no `jadpo login` and no private
  registry: packages are committed into apps, so a registry's caching adds
  nothing (Go is the precedent). The lock's source field leaves room for a
  private registry later.
- **Public packages** go to the registry with `jadpo package publish`.

> **Built (chunk 7.1b2).** `jadpo package link <folder>` writes `package
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

**What publishing requires.** `jadpo package publish`, and `jadpo package
check` before a tag, refuse a package unless:

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

The app's own integrations and libraries skip the bar; it applies only to
what is published. Descriptions alone are not the bar, because the compiler
cannot judge whether one says anything; examples are, because they must
run. A library package also ships complete wording in its own default
locale, with a `///` description on every public message (area 10).

> **Built (chunk 7.1b).** `jadpo package check`, run in a package's own
> repository, checks the package placed as the app's own integration in a
> scratch app (the golden rule) and shows each problem at its place in the
> package's folder. Item 1 is the check itself, `type.standard_exists` on
> the package's public types among it (area 19, question 1); item 2 is
> `publish.description_missing`; item 4's tests are the package's own
> `.test.jadpo` files; item 5 works out the version from the surface at
> the last tag and refuses a breaking change (`publish.breaking_change`);
> item 7 is the index's own check. Item 4's decoders and recipes come with
> decoding (chunk 7.2) and recipes; the bar says "not checked yet" for
> each. The surface is written whenever the package compiles and its
> version is allowed.
>
> **Built (chunk 7.1b2).** Item 3: each ```` ```jadpo ```` block in a
> public declaration's `///` description runs as a test of the package
> (question 27), a function of a public library without one is
> `publish.example_missing`, as is a failure it can raise that no example
> or test asserts with `fails`, and an example that doesn't compile or hold
> is `publish.example_failed` at its line in the description. Item 6: the
> new version's section of `CHANGELOG.md` is written from the surface's
> changes, and a release whose section says nothing beneath the list is
> `publish.changelog_missing` (question 28):
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

## Connecting in `app.jadpo`

A connection is infrastructure, declared once in `connections:`
(question 20). Each entry is a built-in driver (`driver: Driver.x`) or a
package (`package: X.vN`), with the typed settings the driver or package
declares. For a package it names the domains whose actions may call it
(question 18), the credential from `config.jadpo` and which of the
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

- `environment:` is a `ProviderEnvironment` setting, `.sandbox` or `.live`,
  choosing between the package's hosts. HTTPS only; redirects and proxies
  are always refused (question 9).
- The credential is a `secret: true` setting (area 10). The package says how
  it is sent; code can never read it (question 10). For a provider acting
  for each user, the credential is an `OAuthGrant` instead.
- `host:` gives the address of a package with no fixed host: another Jadpo
  app, a self-hosted product, or a service already running elsewhere.
- `timeout:` and `retry:` override the defaults (question 8);
  `max_concurrent:` may lower the package's.
- `webhooks:` gives the webhook secret and, for webhooks that need a reply,
  `replies:` (see "Receiving from outside").
- `polls:` changes a poll's interval; `outbound:` grants a service the hosts
  its package asks for; `erasure:` names the field holding the person's key
  at this provider; `development:` lists what reaches the sandbox in
  `jadpo dev`. Each is described in its own section below.
- A package's own `settings:` are given by name (`message_stream:
  "outbound"`).
- Two versions of one package are two connections, used side by side while
  callers move across (question 3).
- A library package needs no connection: any domain brings it in with a
  `use` line.

## Calling

An operation is called through its connection:
`connection.resource.operation(input)`, and a provider package's public
action the same way, `connection.action(input)`. Only actions may call
either. A call can fail, so it needs `attempt` or a `match`, and the
package's failures are mapped to the app's own at the call, in area 4's
form `condition: Failure`.

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

A library package's functions are called through the package's name, with
no connection, from anywhere a library may be called:

```jadpo fragment
// domains/accounts/actions/save_address.jadpo
use packages/postcodes

var postcode: Postcodes.UkPostcode = attempt Postcodes.uk.parse(input.postcode) {
    invalid_postcode: AddressInvalid
}
```

- **Types** come from the package; the app writes none (question 1). The
  checker compares what you send with what the operation takes. At runtime
  a response is checked only for the fields your code reads, so a provider
  adding or changing other fields breaks nothing. A package input may take
  any of the app's object types (Postmark's template variables, Stripe's
  metadata), sent as JSON: a general addition area 2 designs.
- **Failures** (question 5). A package failure is mapped at the call or it
  becomes a fault. `timed_out` may map to an `unavailable` failure only on
  an idempotent operation. Unmapped problems are faults, never in `fails`:
  `misconfigured` for a rejected credential, `unavailable` when the
  provider surely did nothing, `outcome_unknown` when it may have acted.
  Changing a package failure's code, not just its words, is mapping it to
  the app's own failure at the call.
- **Idempotency** (question 6). The package says where the key goes; the
  runtime supplies it. The key sent is never the client's or the delivery's
  key as it is: it is `jd1_` and 32 base32 characters of a SHA-256 over the
  application, environment, connection, caller scope, the action's key (the
  delivery key in subscribers and schedules, the client's
  `Idempotency-Key` or the request id in routes), the step number and the
  key generation, 36 characters, cut to the package's `max_length:` where
  the provider allows fewer. Two callers who send the same
  `Idempotency-Key` never meet at a provider that keys by account.
- **Retries** (question 8). `timeout:` (default 10s) and
  `retry: { attempts: within: }` (default 3 within 30s), exponential backoff
  with full jitter. Only safe repeats are retried: the request never left,
  the provider said it did nothing (honouring `Retry-After`), or the call
  is idempotent. The package's rate limits space calls out before the
  provider refuses them, and `max_concurrent:` caps how many run at once
  per app instance: further calls wait, and waiting subscriber deliveries
  retry later. Heavy work such as PDF rendering runs in subscribers, never
  in the route.
- **Pagination.** A list operation returns area 6's `Page<T>`; the package
  says how the provider pages, and `page.next` asks for the next one. Work
  over many pages uses `batches.each(from:, each:, finished:, key:)`
  (area 12, "Batches: work over many items"), which pages through in the
  background and resumes where it stopped.
- **Order with writes** (question 7). Calls that change something come
  before any write; the writes commit after the last call returns, and a
  failed commit retries the action, whose calls replay. No transaction is
  open while a call is out: reads before it use a snapshot closed before the
  call, and a write to a record loaded before it checks the row is
  unchanged (area 6, "Outside calls hold no transaction", 2026-10-15). A
  changing call with no idempotency key is the only changing call in its
  action; several keyed calls follow "Several changing calls in one
  action" below. Work that must follow a write emits an event.

## Decoding

Decided 2026-10-16 (Richard). Two kinds of conversion, split by one test:
is it needed to make the value mean what the provider's documentation says
it is?

- **Decoding** (yes): dates as Unix seconds, numbers as text, lists as
  `"a,b,c"`, an envelope around the real object, an error inside a `200`, a
  columnar table that is really a list of rows, Stripe's
  `amount: 1250, currency: "gbp"` that is really money. **The package
  declares it and the runtime does it**, so the app only ever sees correctly
  typed values.
- **Translation** (no): turning a correct value into the shape one app
  prefers. **The app's action does it, with an explicit call.**

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

- **A field without `wire:` is the contract's JSON.** A standard type in a
  provider's request, response or webhook is encoded as area 19's
  contract writes it, by the route codec a request's body goes through,
  so an amount is `{"amount": 1250, "currency": "GBP"}` and a quantity
  keeps its unit; `wire:` names another encoding only where the provider
  uses one (decided by the lead session, 2026-10-18, under Richard's
  instruction of 2026-10-17 that every boundary goes through the
  contract).
- **Byte formats stay in the runtime**: JSON, form-encoded, multipart, XML
  later. Package functions reshape values, never bytes.
- **Decoding is lazy where it can be.** A field decoder runs only when code
  reads the field. A whole-response decoder reads what it reads, so "a
  provider adding fields breaks nothing" holds for fields it ignores.
- **A value that will not decode is a fault** naming the operation and
  field, as a field that does not match its type is today; contract runs
  against the sandbox catch it before production does.
- **App tests script the clean type**, never the wire shape; the package's
  own tests and recordings cover its decoders. Webhook payloads decode the
  same way.
- **The importer cannot see most of this**, so the agent adds it with the
  other corrections and `jadpo package record` confirms it against a real
  response.
- The compiler cannot police the line between decoding and translation; the
  surface shows both the wire and the output type, so the published
  documentation and the review make it visible.

## What package code may do

Decided 2026-10-16 (Richard). The point of a package is expertise written
once by someone who knows the API and tested properly, so a provider
package may act on its own API, but never on the app's data.

- **No state and no database.** A package owns no entities or tables and
  never reads or writes the app's data. Data a package fetches reaches the
  app as a return value or an event, and the app's own action writes it.
- **Package actions call only their own API.** A provider package may hold
  `public action`s in `actions/` that call its own operations in any order,
  pass answers between them and use its pure functions, and return a result
  or fail. They cannot emit app events, send email, call any other
  connection, or read configuration or secrets. The app calls one as
  `connection.action(input)` inside its own action; one with several
  changing calls follows "Several changing calls in one action". Inside a
  package, operations are named without the connection
  (`customers.create`).
- **Package subscribers on its own webhooks.** A package may subscribe to
  its own webhook events, call its own API to enrich them, and emit its own
  declared events (`Stripe.SubscriptionRenewed`).
- **Events only about the outside world.** A package emits events for
  webhooks, enriched webhooks and polls, never as a side effect of the app
  calling it; the app's action emits its own event there, where a reviewer
  sees it. So **an action that emits cannot be `public`**: only the
  package's subscribers and polls run it. Package events travel through the
  app's outbox like any other.
- **Package code runs only when started**: by the app calling it, or by the
  outside world through a webhook or a poll the app subscribes to.
- **Full permissions are not built**: a package that owns tables or runs
  its own schedules waits until a real app needs package-owned data; adding
  grants later breaks nothing.

```jadpo fragment
// packages/stripe/v2024_06_20/actions/start_subscription.jadpo
/// Creates the customer, attaches the card, starts the subscription and
/// finalises its first invoice, in the order Stripe requires.
public action start_subscription(input: StartSubscription) fails CardDeclined -> StartedSubscription { ... }

// packages/stripe/v2024_06_20/subscribers/renewal.jadpo
subscriber renewal {
    on: InvoicePaid                       // the raw webhook event, not public
    run: announce_renewal(event)          // emits the public SubscriptionRenewed
}

// domains/billing/actions/subscribe.jadpo
var started: Stripe.StartedSubscription = attempt stripe.start_subscription(request) {
    card_declined: PaymentDeclined
}
```

### Polls

**Polling turns an API without webhooks into events.** `polls:` in
`provider.jadpo` names an operation, a default interval and the event
emitted when the answer changes. The runtime runs it, keeps what it last
saw (runtime state, like webhook de-duplication) and emits only changes; a
poll runs only while something subscribes to its event, and the connection
can change the interval (`polls: { latest: 6h }`).

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

Polls are for feeds that do not depend on the app's data (rates, a price
list). Checking each of the app's own records (each parcel still in
transit) is the app's schedule calling the package, one event per record,
and a package gives it as a recipe.

### Services

A package's own API may be a container Jadpo runs (`runs:`), which its
actions call; it cannot call another package or connection.

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
  grants them (`outbound: [...]`), and a new grant appears in the approval
  report. Until then `jadpo check` names the hosts asked for and the line
  that grants them.
- **Three settings Jadpo always applies** when it creates a container: no
  outbound network unless granted, never privileged, and no access to the
  host's disk. Jadpo creates the containers itself, so there is no
  configuration to audit.
- **Reaching a service.** A package says only the port its service listens
  on inside its container, so two packages can both use 3000. Jadpo gives
  each app a private network that only it and its services are on, and
  hands each connection the address: the service's name on that network
  when the app runs in a container, or a loopback port the engine picked
  when it runs as a plain process. Nobody writes an address. Whether a
  network that blocks outbound traffic can publish a port to the machine is
  checked when this is built.
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
- **Stages, each useful alone**: `jadpo dev` starts the declared services
  (tests keep using fakes); `jadpo build` writes the configuration that
  runs each beside the app (Compose first); the native deployable
  supervises its services itself. On Workers, which runs no containers, a
  service is hosted elsewhere and the connection gives `host:`.

### Recipes

A package may ship `recipes/`: complete wiring examples (schedules, actions,
entities) that `jadpo package recipe <package> <recipe> --into <domain>`
copies into a domain, after which they are the app's own code. Each is
compiled and tested against the package's `sample/` app at publish and
listed in the surface. Library packages may ship recipes too.

## Several changing calls in one action

Decided 2026-10-16 (Richard), adopting
[the multi-step review](research/multi-step-calls-review.md) with all ten
of its fixes and its open questions answered as it recommends. It applies
to an app's action and to the package actions it calls, which are one
sequence.

Area 12 describes the same rules from the delivery's side ("Several
changing calls in one action").

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
  records the answer as the provider sent it, encrypted, at most 256 KB (a
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
  first run's start) for every rerun, area 6's rerun included; a resumed
  action writes timestamps from when it began, which is also when the
  provider acted. Before the first changing step a rerun takes a fresh
  time. This amends areas 6 and 10.
- **Keys and windows.** Each step's key is derived as "Calling" says, the
  step number and key generation included. A keyed step in doubt inside its
  provider's `window:` (less a margin, the smaller of a tenth of the window
  and one hour) ends the run retry-later, and the next attempt sends it again
  under the same key. It parks as `outcome_unknown` when it has no key, when
  its window would pass, or when the package says `replays_failures: true`
  and the provider answered 5xx. Only the package's `new_key_on:` answer
  makes the runtime send again under the next key generation by itself. The window rule holds for single calls too.
- **One runner per record.** Routes claim the client's key before the action
  runs (area 7). A delivery's lease carries a generation that every record
  write checks, so a runner whose lease lapsed stops before sending another
  step. The provider's "key in use" answer means wait: retried within the
  call's budget, then retry-later, never a failure. The provider's
  "different request" answer is `diverged`, never `misconfigured`.
- **Transactions.** Database writes still come after the last call, with no
  transaction open during calls (area 6). Record writes are the runtime's
  own, each in a short transaction.
- **The warning.** After the first changing step, every point that can end
  the action for good (a step's failure, a write that can be refused,
  `reject`, a called action's failure) is warned unless handled with
  `match`, naming three fixes: check the condition before the first step;
  record the answers and finish in a subscriber of an event the write emits;
  undo as another step. A package's surface lists which of its failures
  follow an effect, so the warning reaches through package actions. This
  answers question 26, for single calls too.
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
  encrypted with a runtime key; nothing links them to a person, so the
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
  longest path, against the route's deadline. The 8-step limit is
  confirmed by measurement, like area 12's other limits.
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

## Receiving from outside

Decided 2026-10-16 (Richard), extending question 11.

### Webhooks as events

A package declares the provider's events and how they are signed. Giving a
connection a webhook secret makes the runtime:

1. generate `POST /webhooks/<connection>` in area 7's `webhooks` API (its
   path can change, but it cannot be switched off);
2. check the signature over the raw body, rejecting timestamps older than
   five minutes;
3. drop repeats by the provider's event id;
4. write each event, public or not as the package marks it, to the app's
   outbox, so subscribers run after commit (area 12).

```jadpo fragment
// domains/payments/subscribers/payment_succeeded.jadpo
subscriber payment_succeeded {
    on: Stripe.PaymentIntentSucceeded
    run: mark_order_paid(event)
}
```

No webhook route is written by hand (question 11). Signature schemes are
built in: `SignatureScheme.stripe`, `SignatureScheme.standard_webhooks`
(Svix and others), `SignatureScheme.hmac_sha256` with a named header,
basic authentication (Postmark, which signs nothing), Twilio's HMAC-SHA1
and AWS SNS certificates (question 25). Every event carries the provider's
`occurred_at`; out-of-order delivery is the app's to handle, by ignoring
an event older than what it stored, and a recipe shows how.

### Webhooks that need a reply

Some providers wait for an answer in the webhook's own response (Twilio's
call instructions, Slack's messages). The package's event type names its
reply type with `reply:`, the reply type living in the package's `types/`;
the connection's `webhooks: { replies: { } }` names the app's action that
answers, keyed by the event's name in lower_snake_case. The action takes
the event and returns the reply type, which runs inside the request after the signature
check and may read and write like any action. A repeat gets the stored
reply, and the package encodes the reply (Twilio's XML waits for an XML
byte format in a Jadpo release).

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

### Handshakes

Some providers check an address before sending to it (AWS SNS, Slack,
Microsoft Graph). These are built-in rules the package names in
`webhooks:` (`handshake: WebhookHandshake.sns`, `.slack`,
`.microsoft_graph`), answered by the runtime and extended by Jadpo
releases.

### Queues

Queues are read directly through the `receiving` capability, the inbound
twin of `publishing`. A queue names the field giving each message's type,
the field giving its id, and the events it may carry; a queue of one kind
lists one event and no `type_at:`.

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

- The events come from another Jadpo app's generated package, or are
  declared by the app (in a domain's `events/` or its own integration, each
  with its `wire_name:`); the queue's package or driver knows only how to
  fetch.
- Each message becomes its event for the app's subscribers, acknowledged
  only after the subscriber's transaction commits, repeats dropped by
  message id stored in the database. A message whose type is not listed is
  parked for an operator, never dropped; one that keeps failing is left to
  the queue's dead-letter queue and parked.
- **Which are packages**, by the usual rule (an HTTP API is a package;
  anything needing its own protocol or a platform binding is a built-in
  driver): Amazon SQS and Google Pub/Sub pull are packages; SNS and Pub/Sub
  push are webhooks through a package with a handshake; Cloudflare Queues
  is a built-in driver on Workers (a binding, its consumer generated into
  the Wrangler configuration) and a package from a native server (its HTTP
  pull API); Kafka is a built-in driver in a later release.

### Senders with no package

A system that signs webhooks and has no package gets the app's own
integration in `shared/providers/`, by the golden rule. A partner with an
agreement is a `Service` with a service key calling an ordinary route
(area 8).

## Acting for each user: OAuth grants

Decided 2026-10-16 (Richard), for apps acting on another service for each
user or organisation: a user's Google Calendar, an organisation's Xero, a
Slack workspace install.

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

- **A connection's credential may be a grant owned by one of the app's
  records**: the owner is `User` or any entity (`Organisation`). The
  provider's facts (authorize, token and revoke addresses) are `oauth:` in
  the package's `provider.jadpo`, and each operation declares the scope it
  needs (`scope: "calendar.events"`). Nothing is specific to one provider; the connection's name is the
  app's.
- **Connecting is endpoints, not pages**: `POST /connections/<name>/start`
  for the signed-in caller returns the provider's consent address, and
  stores the `state` on the server against the caller's session or API
  token; the provider returns to the generated
  `GET /connections/<name>/callback`, which finds the `state`, exchanges the
  code with PKCE, stores the grant and redirects to the frontend's
  `return_to` with success or failure. The callback needs no cookie, so an
  app's own API-token clients connect the same way. Area 8 covers how this
  touches identity.
- **Grants are held by the runtime**, like sessions, in a `jadpo_grants`
  system table in the owner entity's store (so grants need a database),
  their tokens encrypted with `application { grants: { encryption_key:
  config.grant_encryption_key } }`, a secret read and rotated as signing
  keys are (area 10's `SigningKey`, with its previous key), and never
  readable by code.
- **Every call names whose grant it uses**: `on_behalf_of: owner_id`.
  `grant_missing` and `grant_revoked` are failures the app maps.
- **Refresh is automatic**: renewed before expiry, once when calls race,
  rotated refresh tokens kept. A revoked grant gives `grant_revoked` and a
  `GrantRevoked` event, so the app can ask the owner to reconnect.
- **Scopes are checked at compile time**: calling an operation whose scope
  the connection does not request is an error.
- **Policy decides who may connect** for an owner (area 9): connecting and
  disconnecting need the `update` grant on the owner's record, so no new
  effect is introduced.
- **Disconnecting** is the generated `POST /connections/<name>/disconnect`
  and its operation, revoking at the
  provider when the package declares how; erasing a person or record (area
  15) deletes and revokes its grants.
- **Development and tests**: calls are faked by default;
  `jadpo dev grants add <connection> --owner <id>` makes a fake grant, and
  tests declare grants as fixtures.
- **Later, each when an app needs it**: several accounts per owner,
  incremental scopes, provider notifications per grant, and GitHub App
  installation tokens (a credential scheme in the runtime).

## Packages from other Jadpo apps

Decided 2026-10-16 (Richard), replacing "every Jadpo API publishes a
package from its API record".

- **Jadpo's OpenAPI documents carry `x-jadpo` fields** saying what plain
  OpenAPI lacks: `x-jadpo-changes` (the operation changes something),
  `x-jadpo-idempotency`, `x-jadpo-failure` (each failure's `code` and
  `kind`), `x-jadpo-constraints` and `x-jadpo-events` (area 7). Other tools ignore them. Every
  API serves its document at `<prefix>/openapi.json` by default (area 7).
- **The OpenAPI importer uses them whenever it finds them**, so importing a
  Jadpo app's document gives an exact package needing no corrections or
  recordings; a provider that is not Jadpo is imported as before. There is
  one importer and one format.
- **Getting it**: in one repository, `jadpo package add ../orders` reads the
  other app's generated OpenAPI; across repositories the other app's
  package comes from git like any private package; for another company's
  public API, `jadpo package add https://api.orders.example/v1` fetches the
  served document. Each is read once, when the package is added or updated,
  then pinned in the lock with its source and digest.
- **The result is an ordinary installed provider package** in
  `packages/<name>/<version>/` with no fixed host (the connection gives
  `host:`) and a service-key credential (area 8). At runtime the two apps
  meet only over HTTP, and the called app cannot tell what made the call.

```jadpo fragment
// app.jadpo
orders: {
    package: Orders.v1
    domains: [fulfilment]
    host: config.orders_url
    credential: config.orders_service_key
}
```

- **Versions**: an additive change to the other API regenerates as a minor
  version through `jadpo package update`, and `jadpo check` says when it is
  out of date; a breaking change (already needing an acceptance in that
  app, area 7) becomes a new major version, side by side while callers
  move.
- **Breaks across apps**: in one repository, `jadpo check` in the called app
  reads the other apps' locks and reports which of their calls a change
  breaks, before it merges. Built later.

## Capabilities

Decided 2026-10-13, revised 2026-10-16 (Richard) to design C of
[the capability comparison](research/capability-providers-options.md).

A **capability** exists only for a job Jadpo performs itself, and covers
that job whole. Jadpo owns the list, each capability has one API in
application code, never the provider's, and every connection that fills
one does all of it, so partial support cannot arise and there are no
optional parts. Changing provider changes `app.jadpo`, never domain code.

| Capability | Code uses | Built-in drivers | Packages |
|---|---|---|---|
| `database` | Entities (area 5) | PostgreSQL; D1 later | No |
| `cache` | `caches:` (area 5) | Redis (Upstash, Valkey and ElastiCache through it, on Workers too); in memory, a warning in production and an error on Workers | No |
| `files` | `File` fields, `store.read` and `store.put` (area 16) | S3 (R2, B2, MinIO and the rest through its settings); disk, native only, one server | Only one that does the whole job |
| `email` | `type X = EmailMessage`, emitted, and Jadpo's own emails (area 16) | SMTP; the local inbox | Yes |
| `publishing` | Chosen public events sent outward after commit; outgoing webhooks | The local log; outgoing webhooks | Yes |
| `receiving` | Queue messages arriving as events | Cloudflare Queues on Workers; Kafka later | Yes (SQS, Pub/Sub pull) |
| `schedules` | `schedule` (area 12) | Timer; Cloudflare cron; external tick | No |
| `search` | `search:` on an entity (area 16) | PostgreSQL full-text search, the default | Yes (hosted engines) |
| `bot_check` | `bot_check: true` on a route or generated feature (area 16) | A local driver passing every check in development and tests | Yes (Turnstile, hCaptcha, reCAPTCHA) |
| `documents` | `type X = Document` rendered to HTML or PDF (area 16) | None: tests fake it | Yes (Gotenberg as a service, DocRaptor, PDFShift) |
| `scanning` | Uploads scanned before they can be attached (area 16) | None | Yes (a ClamAV service, a scanning API); built when an app needs it |

Event delivery is not a capability: it stays inside the runtime, from the
outbox in the main store, so logs and traces stay in one place and no
queue service is needed. Telemetry is not one either: it has its own
section in `application { }` and one protocol, OpenTelemetry (area 16).
Feature flags, rate limits, sessions and idempotency keys name a store and
are not capabilities. SMS is not a capability, because nothing in the
language sends SMS: Twilio is a package, its text still typed against
translations, and `sms` returns if SMS sign-in codes are built.

**One `connections:` block.** Each entry is a built-in driver or a package;
where it is used says which capability it serves (question 20). One
connection is one resource (a bucket, a queue), not one vendor account, as
Cloudflare's own bindings are. Entities, caches and `File` fields name a
connection with `store:`; `email:`, `publishing:`, `receiving:`,
`documents:` and `bot_check:` name one in `application { }`; `search:` names
an engine's connection, PostgreSQL when left out; schedules use the
target's own tick unless told otherwise. Using a connection for a
capability its package does not provide is a compile error.

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

**Everything provider-specific is the package's own operations**, called
from an action or subscriber in a domain the connection lists, as payments
are: hosted templates with variables
(`postmark.email.send_with_template(...)`), suppression lists, marketing
sends. One connection serves both: `email:` uses it, and the listed domains
call it. Bounces and complaints are the package's own webhook events
(`Postmark.BounceWebhook`), so moving provider rewrites the subscribers
that read them; they return to the capability only when a built-in feature
consumes them. What hosted-template mail gives up: the dev inbox entry, the
health score and the automatic template check (an app can add a contract
case).

**How a package provides one.** A file `provides/<capability>.jadpo`, found
by name, maps each standard operation onto one of the package's operations
(question 21):

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

- **Mappings stay data**: field paths, literals, lists, nested blocks, enum
  tables, and `each:` with `as:` to map a list element by element, the
  current element named `item`; no
  expressions or calls. Computation lives in the package's decoders and
  encoders. The compiler checks both sides.
- **Email attachments are part of the capability**: a `File` field on an
  `EmailMessage` is an attachment, read from its store at send time and
  checked against the mapping's `limits: { attachments: }`, the total size
  the provider accepts. A provider that cannot
  send attachments stays a package but cannot be the email connection.
- **Emails without an idempotency key** (Postmark, SES, SendGrid, Mailgun)
  keep area 12's parking on `outcome_unknown`; revisited if chunk 7.3 shows
  parking is common. Two kinds of mail through two connections wait for an
  app that asks.
- **Image variants**, when built, are made by the runtime over any store,
  so `files` stays whole; media services (Cloudinary, Uploadcare) stay
  ordinary packages, since they cannot do the whole `files` job.

**Erasing a person at a provider** is not a capability mapping. The package
declares its erasure operation in `provider.jadpo` (`erasure: { call:
customers.delete  by: CustomerId }`), and the connection names the app
field holding the key (`erasure: { key: User.stripe_customer_id }`). Jadpo
copies each key into the erasure event, the app erases the person in one
transaction, then a generated subscriber calls each provider after commit,
retried until it confirms. Area 15's erasure-completed email goes only
once every provider has confirmed, and a delivery that gives up is parked
for an operator and shown, never quietly incomplete. A copied key that is
personal data (an email address) stays only in the outbox until delivered,
listed in the personal-data report. A connection that receives personal
data but cannot erase it is a compile error unless an acceptance
(`ErasureNotPossible`, area 18) states why the data stays (tax records), listed in the personal-data report. The
app may add its own `PersonErased` subscribers.

**Proving a package works.** Each capability has a conformance suite;
`jadpo package conform <package> <capability>` runs it against the
provider's sandbox or an emulator. The registry lists a capability only
with a passing run pinned to the package's digest, and the app's own
integrations run it in scheduled CI. Tests use one fake per capability,
whatever the provider: `email.sent`, `published` (question 22).

**Development.** In `jadpo dev` nothing leaves the machine: mail goes to the
local inbox, files to local disk, published events to a log in the dev
console, and search to PostgreSQL whatever production uses. The database
runs its real engine; the cache runs in memory (question 23). `jadpo dev`
starts the services the app's connections declare.

**Reports.** The manifest records each connection's capabilities, hosts,
services and targets; the network report labels each host with its
connection and capability; the personal-data report lists the connections
that receive marked fields, which of them can erase, and the acceptances
for those that cannot.

**Targets.** Each driver and package says which targets it runs on, and
`jadpo check` refuses a deployable that uses one elsewhere, naming the
alternative (question 24). A package that calls an HTTP API runs on every
target; a service (`runs:`) runs only beside a native deployable, so on
Workers its connection gives `host:`. Cloudflare Workers can open TCP
sockets but block port 25, so SMTP works only on port 587; Workers has no
timer of its own, so its schedule tick is Cloudflare cron; R2 and Queues
bindings exist only on Workers; the disk driver and an in-memory cache do
not run there.

### Choosing a provider when deploying

Designed 2026-10-16 (Richard), built when the first app other people run
needs it. An app that other people deploy (an open-source product run by
many operators) declares every connection it supports and a setting picks
one at start:

```jadpo fragment
// app.jadpo
email: {
    connection: config.mail_provider
    from: config.mail_sender
}
```

Every candidate connection is compiled, checked and reported, and an
unchosen one needs no credential. Sending different messages through
different connections, and falling back to a second provider, come later
and only when an app asks.

## Provider calls in development

Decided 2026-10-16 (Richard), as section 4 of
[the capability comparison](research/capability-providers-options.md)
recommends.

- **Every provider operation is faked by default in `jadpo dev`**, answered
  from the package's recordings where they exist (their placeholders filled
  with generated values of the right types) and from generated defaults
  otherwise, so nothing leaves the machine and no credential is needed.
- **Named operations or resources go to the sandbox, never live**, through
  `development: { sandbox: [payment_intents.retrieve, customers] }` on the
  connection, where review and agents see it; the names are checked against
  the package. `jadpo dev` refuses a let-through connection whose
  environment is live, and one whose sandbox credential is missing from
  `.env.local` stays faked with one line naming the setting to set.
  `--fake` and `--sandbox` on `jadpo dev` change one session only, and
  nothing else persists: only the credential is per developer.
- **`jadpo calls script <operation> <failure>`** sets the next answer, in
  area 13's `fakes:` form, so an agent can walk a failure path by hand.
  Fakes keep no state: a flow that creates and then reads the same object
  either lets both through or scripts the read.
- **Every call is logged**, faked or real, with its connection, operation,
  inputs and answer in full (development only, secrets never), its source
  (`recording`, `generated`, `scripted`, `sandbox`), attempts, duration and
  request id, in `jadpo calls` (`--connection`, `--operation`,
  `--request`), `jadpo calls show <id>`, `jadpo trace` and `--json`. They
  are kept with the last 500 timelines in `build/dev/`, never committed.
  This changes area 17's hashing of text for call payloads in development
  only; production keeps area 16's spans with no values.

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
reads each package's index and `provider.jadpo`, finds each resource the
app calls by its file name, loads only those, and caches the check per
package digest, so an unchanged package costs nothing after the first
build. An app using S3 and SES loads one driver connection, one small
package and a handful of files from it.

## Tests and fakes

Every package has a generated fake. In `jadpo test`, an operation answers
with a valid default built from its output type, and a test scripts a
response or a failure for one call with area 13's `fakes:`. Responses still
pass through the real retries, decoding and mapping. Package actions run
for real over the faked operations.

```jadpo fragment
test "a taken slot is reported" {
    fakes: {
        couriers.bookings.create: [slot_taken]
    }
    ...
}
```

The compiler checks every scripted response against the package's types, so
a fake cannot return a shape the provider never would; tests script the
clean type, never the wire shape. Fakes keep idempotency keys as the
provider does and accept `lost` and `in_use` as scripted answers (see
"Several changing calls in one action"); grants are declared as fixtures.
`jadpo test --kind contract` runs the same cases against the provider's
sandbox, in CI on a schedule. Every written operation that is not
confirmed must have a case in that run (question 12).

## What reviewers see

The approval report (area 17) lists, for each change:

- operations newly called, and the domains calling them;
- data newly sent to a provider, with personal fields flagged (area 15);
- package installs and upgrades, as the surface diff of what the app uses,
  hosts and credential schemes included (area 18 says which need a
  person);
- services: a new or changed image, which needs an acceptance before it
  runs, and new `outbound:` grants;
- third-party packages from git, marked unverified, which need an
  acceptance to install;
- connections that receive personal data but cannot erase it, with their
  acceptance;
- written operations not yet confirmed.

## Errors the coming chunks add

None of these has an example yet; each comes with the chunk named, with its
example and real output under "Errors".

| Mistake | Reported | Chunk |
|---|---|---|
| An installed package whose files do not match its digest in `jadpo.lock`, or whose surface differs from its committed `surface.jadpo_record` | By `jadpo check` and at install | 7.1, the surface at install 7.1b2 |
| A link in the lock when building for production or in CI | By `jadpo check` and `jadpo test` in CI; by `jadpo build` | 7.1b2; `jadpo build` in Phase 8 |
| An own integration or library taking a reserved brand name; installing a package whose name the app's own code uses | By `jadpo check`; by `jadpo package add` | 7.1 |
| An `emails/` folder in a package; `requires:` or any key besides `licence:` and `jadpo:` in `package.jadpo`; an unknown key in `provider.jadpo` | By `jadpo check` | 7.1 |
| A non-public type or failure reachable from a public declaration, in a package or a domain | By `jadpo check` | 7.1 |
| A package's resource and action with the same name; a public action that emits; subscribing to a package's non-public event | By `jadpo check` | 7.1 |
| A connection's `host:` for a package with fixed `hosts:`; a package with both `hosts:` and `runs:` | By `jadpo check` | 7.1 |
| A capability's connection whose package does not provide it | By `jadpo check` | 7.3 |
| A function call or expression in a `provide` mapping | By `jadpo check` | 7.3 |
| A decoder that fails on a recorded response; a field's decode and encode that do not round-trip | By `jadpo test` and the publishing bar | 7.2 |
| A connection that receives personal data, with no `erasure:` and no acceptance | By `jadpo check` | Personal data (after the first build) |
| An operation whose scope the grant does not request; a call to a grant connection without `on_behalf_of:` | By `jadpo check` | Not yet on the roadmap |
| Several changing calls reachable from a schedule or a route whose API does not require `Idempotency-Key`; an outside call inside `for` there; more than 8 steps; a changing call with no key beside another changing call | By `jadpo check` | Not yet on the roadmap |
| A service image not yet accepted; an `asks:` host not granted by `outbound:` | By `jadpo check` (named, with the line that grants it) | Not yet on the roadmap |
| A third-party package from git with no acceptance | By `jadpo check` | Not yet on the roadmap |

> Built in chunk 7.1, under "Package errors" below: the digest and the
> lock, an `emails/` folder, the index's keys, `provider.jadpo`'s keys and
> `hosts:` with `runs:`, a reserved name and a name the app uses, and the
> reachable-from-public rule (area 1's `visibility.public_reaches_private`).
> Chunk 7.1b2 built a surface that differs at install and a link in CI,
> under "Installing errors". Still to come: a resource and an action with one
> name, a public action that emits and subscribing to a non-public event
> (chunk 7.6); a connection's `host:` for a package with fixed `hosts:`
> (chunk 7.1c).

## Package errors

> Built in chunk 7.1: the package format, names and the lock. Each example
> below is checked by this checkout's `jadpo`, as an installed package in
> `packages/` or as the app's own integration in `shared/providers/`.

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

### `package.index_invalid`

A package's index is a `///` description and two keys, `licence:` and
`jadpo:`. Packages have no dependencies, so it has no `requires:`:

```jadpo check project=p71-index file=app.jadpo hidden
application {}
```

```jadpo check project=p71-index file=shared/providers/acme_couriers/v1/package.jadpo expect=package.index_invalid
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
    requires: [Postcodes.v2]
}
```

```text output
error: A package's index can't hold `requires:`
 --> shared/providers/acme_couriers/v1/package.jadpo:5:5
  |
5 |     requires: [Postcodes.v2]
  |     ^^^^^^^^^^^^^^^^^^^^^^^^ not an index key
  reason: Packages have no dependencies: a package that needs another's code carries a copy, so installing one is one folder and one line in the lock.
  next: Remove `requires:`, and copy what the package needs into it (agent)
  rule: package.index_invalid
  help: docs/language/11-integrations.html#package-index-invalid

check failed: 1 error, 2 files
```

### `package.name_mismatch`

A package's folder is its name in `lower_snake_case` and its version, so
its index gives the folder's name in `UpperCamelCase`:

```jadpo check project=p71-name file=app.jadpo hidden
application {}
```

```jadpo check project=p71-name file=shared/providers/acme_couriers/v1/package.jadpo expect=package.name_mismatch
/// Acme Couriers' booking API.
package AcmeCourier.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```text output
error: `AcmeCourier.v1` is in the folder of `AcmeCouriers.v1`
 --> shared/providers/acme_couriers/v1/package.jadpo:2:9
  |
2 | package AcmeCourier.v1 {
  |         ^^^^^^^^^^^^^^ the folder says `AcmeCouriers.v1`
  reason: A package's folder is its name in `lower_snake_case` and its version, `acme_couriers/v1`, so its name in code is `AcmeCouriers.v1`, the folder's in `UpperCamelCase`, and the index gives the same.
  next: Write `package AcmeCouriers.v1` (agent)
  rule: package.name_mismatch
  help: docs/language/11-integrations.html#package-name-mismatch

check failed: 1 error, 2 files
```

### `package.emails_folder`

Email templates are the app's own, so a package has no `emails/`:

```jadpo check project=p71-emails file=app.jadpo hidden
application {}
```

```jadpo check project=p71-emails file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-emails file=shared/providers/acme_couriers/v1/emails/booked.jadpo expect=package.emails_folder
type Booked = EmailMessage {}
```

```text output
error: A package can't hold email templates
 --> shared/providers/acme_couriers/v1/emails/booked.jadpo
  reason: Email templates, partials and layouts are the app's own, in `emails/` of the domain that sends them; a package sends no email itself.
  next: Move the email into a domain's `emails/` folder in the app, and remove it from the package (agent)
  rule: package.emails_folder
  help: docs/language/11-integrations.html#package-emails-folder

check failed: 1 error, 3 files
```

### `package.provider_invalid`

`provider { }` has one fixed set of keys, `protocol:` among them:

```jadpo check project=p71-provider file=app.jadpo hidden
application {}
```

```jadpo check project=p71-provider file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-provider file=shared/providers/acme_couriers/v1/provider.jadpo expect=package.provider_invalid
provider {
    protocol: Protocol.json_http
    host: "https://api.acme-couriers.example"
}
```

```text output
error: `provider` has no key `host:`
 --> shared/providers/acme_couriers/v1/provider.jadpo:3:5
  |
3 |     host: "https://api.acme-couriers.example"
  |     ^^^^ not a provider key
  reason: `provider { }` has one fixed set of keys: `protocol:`, `hosts:`, `runs:`, `credential:`, `oauth:`, `idempotency:`, `error_code:`, `webhooks:`, `polls:`, `erasure:`, `max_concurrent:`, `settings:`, `imported:` and `description:`. Did you mean `hosts:`?
  next: Use `hosts:` instead of `host:` (agent)
  rule: package.provider_invalid
  help: docs/language/11-integrations.html#package-provider-invalid

check failed: 1 error, 3 files
```

### `package.provider_missing`

A package with `resources/` describes an API, so it says how the API is
reached:

```jadpo check project=p71-missing file=app.jadpo hidden
application {}
```

```jadpo check project=p71-missing file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-missing file=shared/providers/acme_couriers/v1/resources/bookings.jadpo expect=package.provider_missing
public resource bookings {}
```

```text output
error: Package `AcmeCouriers.v1` has resources but no `provider.jadpo`
 --> shared/providers/acme_couriers/v1/resources/bookings.jadpo
  reason: A package with `resources/` describes an API, so it says how the API is reached in `provider.jadpo`: its protocol, hosts and credential. A package without resources is a library.
  next: Add `shared/providers/acme_couriers/v1/provider.jadpo` with `provider { protocol: Protocol.json_http ... }`, or move the resources out if this is a library (agent)
  rule: package.provider_missing
  help: docs/language/11-integrations.html#package-provider-missing

check failed: 1 error, 3 files
```

### `package.provider_translations`

A provider package shows people no text, since its failures are mapped to
the app's own:

```jadpo check project=p71-words file=app.jadpo hidden
application {}
```

```jadpo check project=p71-words file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-words file=shared/providers/acme_couriers/v1/provider.jadpo hidden
provider {
    protocol: Protocol.json_http
}
```

```jadpo check project=p71-words file=shared/providers/acme_couriers/v1/translations/booking_hint.jadpo expect=package.provider_translations
/// Shown under a booking.
type BookingHint = Message {}
```

```text output
error: Provider package `AcmeCouriers.v1` carries translations
 --> shared/providers/acme_couriers/v1/translations/booking_hint.jadpo
  reason: A provider package shows no text of its own: its failures are always mapped to the app's own, which the app words. Only a library package carries translations, for its own messages and failures.
  next: Remove the package's `translations/`, and word what users see in the app's own failures (agent)
  rule: package.provider_translations
  help: docs/language/11-integrations.html#package-provider-translations

check failed: 1 error, 4 files
```

### `package.reserved_name`

The names of the providers Jadpo curates packages for are reserved, so
`Stripe` in code always means the official package:

```jadpo check project=p71-reserved file=app.jadpo hidden
application {}
```

```jadpo check project=p71-reserved file=shared/providers/stripe/v1/package.jadpo expect=package.reserved_name
/// The app's own Stripe calls.
package Stripe.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```text output
error: `Stripe` is a reserved provider name
 --> shared/providers/stripe/v1/package.jadpo
  reason: Jadpo reserves the names of the providers it curates packages for, so `Stripe` in code always means the official package. The app's own integration takes a name of its own.
  next: Rename the folder `shared/providers/stripe/v1` to a name of the app's own, such as `stripe_api`, and the index with it (agent)
  rule: package.reserved_name
  help: docs/language/11-integrations.html#package-reserved-name

check failed: 1 error, 2 files
```

### `package.name_taken`

A package's name joins the app's domain and library names:

```jadpo check project=p71-taken file=app.jadpo hidden
application {}
```

```jadpo check project=p71-taken file=domains/couriers/actions/book.jadpo hidden
action book() -> Unit {}
```

```jadpo check project=p71-taken file=shared/providers/couriers/v1/package.jadpo expect=package.name_taken
/// A courier's booking API.
package Couriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```text output
error: Package `Couriers`'s name is already taken
 --> shared/providers/couriers/v1/package.jadpo
  reason: A package's name joins the names of the app's domains and libraries, which are unique among themselves, and two packages never share a name in code, so `Couriers.` means one thing; it is already domain `couriers`.
  next: Rename one of them (human)
  rule: package.name_taken
  help: docs/language/11-integrations.html#package-name-taken

check failed: 1 error, 3 files
```

### `package.not_public`

A package offers only what it marks `public`:

```jadpo check project=p71-private file=app.jadpo hidden
application {}
```

```jadpo check project=p71-private file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-private file=shared/providers/acme_couriers/v1/types/slot_text.jadpo hidden
/// A collection slot as Acme writes it.
public type SlotText = Text {
    max_length: 12
}
```

```jadpo check project=p71-private file=shared/providers/acme_couriers/v1/libraries/timetable.jadpo hidden
/// Reading collection slots.
public library timetable {
    /// A slot in the form Acme expects.
    function normal(slot: SlotText) -> SlotText {
        return tidying.same(slot)
    }
}
```

```jadpo check project=p71-private file=shared/providers/acme_couriers/v1/libraries/tidying.jadpo hidden
library tidying {
    function same(slot: SlotText) -> SlotText {
        return slot
    }
}
```

```jadpo check project=p71-private file=domains/shipping/actions/keep_slot.jadpo expect=package.not_public
use shared/providers/acme_couriers

action keep_slot(slot: AcmeCouriers.SlotText) -> AcmeCouriers.SlotText {
    return AcmeCouriers.tidying.same(slot)
}
```

```text output
error: `AcmeCouriers.tidying` isn't public
 --> domains/shipping/actions/keep_slot.jadpo:4:12
  |
4 |     return AcmeCouriers.tidying.same(slot)
  |            ^^^^^^^^^^^^^^^^^^^^ not part of the package's API
  reason: A package offers only what it marks `public`; `tidying` is a library in `shared/providers/acme_couriers/v1/libraries/tidying.jadpo` that only the package's own code uses, and may change in any version.
  next: Use what `AcmeCouriers` marks `public` instead (agent)
  rule: package.not_public
  help: docs/language/11-integrations.html#package-not-public

check failed: 1 error, 6 files
```

### `package.not_locked`

`packages/` holds only installed packages, each with its line in
`jadpo.lock`:

```jadpo check project=p71-unlocked file=app.jadpo hidden
application {}
```

```jadpo check project=p71-unlocked file=packages/postcodes/v2/package.jadpo expect=package.not_locked
/// UK postcodes.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```text output
error: Package `Postcodes.v2` isn't in `jadpo.lock`
 --> packages/postcodes/v2/package.jadpo
  reason: `packages/` holds only installed packages, each with its line in `jadpo.lock`: its version, digest and source. A package with no line was copied in rather than installed, so nothing says where it came from.
  next: Install it with `jadpo package add`, or, if it is the app's own code, move it to `shared/providers/postcodes/v2/` (human)
  rule: package.not_locked
  help: docs/language/11-integrations.html#package-not-locked

check failed: 1 error, 2 files
```

### `package.not_installed`

Each line of the lock is a package in `packages/`:

```jadpo check project=p71-gone file=app.jadpo hidden
application {}
```

```jadpo check project=p71-gone file=jadpo.lock expect=package.not_installed
// Generated by Jadpo from the installed packages; update it with `jadpo package add`, `update` or `remove`, never by hand.
record lock 1

package Postcodes.v2 version 2.1.0 path ../postcodes digest sha256:0000000000000000000000000000000000000000000000000000000000000000
```

```text output
error: `jadpo.lock` names `Postcodes.v2`, which isn't installed
 --> jadpo.lock
  reason: Each line of the lock is a package in `packages/`; `Postcodes.v2` has none at `packages/postcodes/v2/`, so the lock and the folder disagree, as removing the folder by hand leaves them.
  next: Restore `packages/postcodes/v2/` from version control, or remove the package with `jadpo package remove` (human)
  rule: package.not_installed
  help: docs/language/11-integrations.html#package-not-installed

check failed: 1 error, 1 file
```

### `package.digest_mismatch`

An installed package is read-only: its files are what its digest in the
lock says:

```jadpo check project=p71-edited file=app.jadpo hidden
application {}
```

```jadpo check project=p71-edited file=packages/postcodes/v2/package.jadpo hidden
/// UK postcodes.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-edited file=jadpo.lock expect=package.digest_mismatch
// Generated by Jadpo from the installed packages; update it with `jadpo package add`, `update` or `remove`, never by hand.
record lock 1

package Postcodes.v2 version 2.1.0 path ../postcodes digest sha256:0000000000000000000000000000000000000000000000000000000000000000
```

```text output
error: Package `Postcodes.v2` differs from what was installed
 --> packages/postcodes/v2/package.jadpo
  reason: An installed package is read-only: `jadpo.lock` locks it with digest `sha256:0000000000000000000000000000000000000000000000000000000000000000`, and its files now give `sha256:fc196456d53ad674b39fbab39eca4883d512f52feb90d687d22c2eeb2c9721e7`, so a file in `packages/postcodes/v2/` was changed, added or removed since it was installed.
  next: Restore `packages/postcodes/v2/` from version control, or install the package again with `jadpo package add`; change the app's own code instead (human)
  rule: package.digest_mismatch
  help: docs/language/11-integrations.html#package-digest-mismatch

check failed: 1 error, 2 files
```

### `package.licence_not_allowed`

Only a package installed from git may be proprietary:

```jadpo check project=p71-licence file=app.jadpo hidden
application {}
```

```jadpo check project=p71-licence file=packages/postcodes/v2/package.jadpo hidden
/// UK postcodes.
package Postcodes.v2 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=p71-licence file=jadpo.lock expect=package.licence_not_allowed
// Generated by Jadpo from the installed packages; update it with `jadpo package add`, `update` or `remove`, never by hand.
record lock 1

package Postcodes.v2 version 2.1.0 path ../postcodes digest sha256:3fcf79f4803b01a5f2f4a9ff6550c4ee20fe3a4e93e608faa0c447eabf6a3191
```

```text output
error: Package `Postcodes.v2` is proprietary but wasn't installed from git
 --> packages/postcodes/v2/package.jadpo:3:5
  |
3 |     licence: Licence.proprietary
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ proprietary
  reason: Only a package installed from git, a company's private package, may be `Licence.proprietary`; the registry and other sources carry packages others may use.
  next: Install the package from its git repository, or publish it under one of the open licences (human)
  rule: package.licence_not_allowed
  help: docs/language/11-integrations.html#package-licence-not-allowed

check failed: 1 error, 2 files
```

## Installing errors

> Built in chunk 7.1b2. The installing commands report these, and change
> nothing when they do; `jadpo check` and `jadpo test` report a link in
> CI. Like the publishing errors below they come from running a command,
> so area 14 lists them with its command errors.

### `install.surface_mismatch`

A package is installed only when its committed surface is what its code
offers. Here a type was added after the surface was written:

```text
$ jadpo package add ../postcodes
error: `Postcodes.v2`'s surface.jadpo_record doesn't match its code
  reason: Its code has `type InwardCode`, which its committed surface doesn't. A package is installed only when its surface, what reviewers read, is what its code offers.
  next: Run `jadpo package check` in the package's repository, commit its surface.jadpo_record, and release it again (human)
  rule: install.surface_mismatch
  help: docs/language/14-tooling.html#command-errors
```

### `install.fetch_failed`

A git repository that can't be read, or holds no release to install:

```text
$ jadpo package add github.com/acme/postcodes@v9.0.0
error: `github.com/acme/postcodes` has no tag `v9.0.0`
  reason: Its release tags are `v2.0.0` and `v2.1.0`.
  next: Give one of its release tags, or tag the release in the package's repository after `jadpo package check` passes (human)
  rule: install.fetch_failed
  help: docs/language/14-tooling.html#command-errors
```

### `install.in_use`

A package the app's code still brings in stays installed:

```text
$ jadpo package remove postcodes
error: The app still uses `Postcodes.v2`
  reason: `domains/intake/actions/read_postcode.jadpo` brings it in with `use packages/postcodes`, so removing it would leave that file unable to compile.
  next: Take `use packages/postcodes` and what it brings in out of the app's code, then run `jadpo package remove postcodes` again (agent)
  rule: install.in_use
  help: docs/language/14-tooling.html#command-errors
```

### `install.link_refused`

A link compiles the app against a checkout on one machine, so it never
ships:

```text
$ CI=true jadpo check
error: `Postcodes.v2` is linked to `../postcodes`
 --> jadpo.lock:4:1
  |
4 | package Postcodes.v2 link ../postcodes
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ a live checkout
  reason: A link compiles the app against a checkout on one machine, so CI (the `CI` variable is set) and production builds refuse it, and a link never ships.
  next: Run `jadpo package unlink postcodes`, then commit `jadpo.lock` (agent)
  rule: install.link_refused
  help: docs/language/14-tooling.html#command-errors

check failed: 1 error, 7 files
```

## Publishing errors

> Built in chunk 7.1b, and the examples and the changelog in chunk 7.1b2.
> `jadpo package check` reports these in a package's own repository, every
> path relative to the package's folder; `jadpo check` never does, since
> the app's own integrations skip the bar.

### `publish.description_missing`

Every public type, failure, library and library function of a published
package has a `///` description:

```text
$ jadpo package check
error: Public type `OutwardCode` has no description
 --> types/outward_code.jadpo:1:13
  |
1 | public type OutwardCode = Text {
  |             ^^^^^^^^^^^ no `///` description
  reason: Every public type, failure, library and library function of a published package has a `///` description: it is what `jadpo package show` and the registry's page say of it, and what an agent reads before it opens the file.
  next: Add a `///` line above `public type OutwardCode` saying what it is for (agent)
  rule: publish.description_missing
  help: docs/language/14-tooling.html#command-errors

Publishing bar
  compiles      passed                        no errors
  described     failed                        1 has no description
  examples      passed                        1 example, every failure shown
  tests         passed                        no tests
  version       passed                        2.1.0, 1 added since 2.0.0
  changelog     passed                        2.1.0 says why
  licence       passed                        mit, from Jadpo 1.0

Wrote surface.jadpo_record

package check failed: 1 failed, 6 passed
```

### `publish.breaking_change`

A version keeps every promise of the release before it in the same major
version: here `uk.tidy` takes a `UkPostcode` instead of an
`EnteredPostcode`, which went:

```text
$ jadpo package check
error: `Postcodes.v2` changed in ways that break the apps that use it
  reason: Since 2.0.0, `function uk.tidy` changed and `type EnteredPostcode` was removed, which an app using 2.0.0 may rely on. A breaking change is a new major version: a new package beside this one.
  next: Put back what changed, or publish the change as `Postcodes.v3` in a folder of its own (human)
  rule: publish.breaking_change
  help: docs/language/14-tooling.html#command-errors

Publishing bar
  compiles      passed                        no errors
  described     passed                        every public declaration
  examples      passed                        1 example, every failure shown
  tests         passed                        no tests
  version       failed                        2 breaking changes since 2.0.0
  changelog     not checked yet               waits for an allowed version
  licence       passed                        mit, from Jadpo 1.0

package check failed: 1 failed, 5 passed, 1 not checked yet
```

### `publish.example_missing`

Every function of a public library has an example in its description, and
each failure it can raise is shown by an example or a test:

```text
$ jadpo package check
error: Function `uk.tidy` has no example
 --> libraries/uk.jadpo:6:14
  |
6 |     function tidy(entered: EnteredPostcode) fails InvalidPostcode -> UkPostcode {
  |              ^^^^ no example in its description
  reason: Every function of a published library shows how it is called with an example in its description, a ```jadpo block run as a test, so what the registry's page and `jadpo package show` say of it always runs.
  next: Add a ```jadpo block to `uk.tidy`'s `///` description, holding statements as a test does: `assert uk.tidy(...) == ...` (agent)
  rule: publish.example_missing
  help: docs/language/14-tooling.html#command-errors
```

A failure no example or test asserts with `fails` is reported the same way,
as "`uk.tidy` can fail with `InvalidPostcode`, which no example or test
shows".

### `publish.example_failed`

An example runs as a test, so one that doesn't hold, or doesn't compile,
is shown at its line in the description:

```text
$ jadpo package check
error: Example of `uk.tidy` failed
 --> libraries/uk.jadpo:7:9
  |
7 |     /// assert postcode == "SW1A1AA"
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ this doesn't hold
  reason: Run as a test of the package, it expected "SW1A1AA" and got "SW1A 1AA". An example runs as a test, so a description always shows what the code does.
  next: Correct the example, or the code if the example says what it should do (agent)
  rule: publish.example_failed
  help: docs/language/14-tooling.html#command-errors
```

### `publish.changelog_missing`

`jadpo package check` writes the new version's list of changes into
`CHANGELOG.md`; the author says why beneath it:

```text
$ jadpo package check
error: CHANGELOG.md doesn't say why 2.1.0 is released
 --> CHANGELOG.md:3:1
  |
3 | ## 2.1.0
  | ^^^^^^^^ no why beneath the list
  reason: `jadpo package check` writes what changed from the surface; whoever releases it writes why, beneath it. A release reviewers can't tell the purpose of is one they can't judge.
  next: Write why 2.1.0 is released under `## 2.1.0` in CHANGELOG.md, after the list (agent)
  rule: publish.changelog_missing
  help: docs/language/14-tooling.html#command-errors
```

## Errors

> These examples predate the provider packages above and are rewritten when
> the area is built (roadmap chunks 7.1 and 7.2).

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

A declaration with a different name:

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

Chunk 7.1 built the package format, names and the lock, chunk 7.1b
the surface, `jadpo package show` and `jadpo package check` with the
publishing bar, and chunk 7.1b2 installing (`add` from a folder or git,
`update`, `remove`, `link` and `unlink`) with the bar's examples and
changelog (below); the rest is still to build.

| Part | Chunk |
|---|---|
| The package format (both kinds, `package.jadpo`, `provider.jadpo`, folders laid out as `shared/`, `public` and the reachability rule), `packages/<name>/<version>/` committed and checked against `jadpo.lock`, the app's own integrations in `shared/providers/`, reserved names, `connections:`, lazy loading, the surface and `jadpo package show`, the publishing bar, the network report | 7.1 |
| The OpenAPI importer (writing into `shared/providers/`, reading `x-jadpo` fields), hand-written packages, decoding with the built-in encodings, recordings checked by decoders, generated fakes, `jadpo package record` | 7.2 |
| The `email` capability whole (Jadpo-rendered, attachments) with SMTP as one driver and a mail package as the second, proving the swap; provider calls faked in development and logged | 7.3 |
| No transaction open during a call | 7.4 |
| Package actions, subscribers, polls and recipes; services; several changing calls in one action; receiving (replies, handshakes, queues); OAuth grants; publisher trust (git packages and links are built, chunk 7.1b2); the hosted registry; deploy-time provider choice; erasure at providers | Not yet on the roadmap; erasure comes with personal data (area 15), after the first build |

> **Built (chunk 7.1).** The package format and both kinds:
> `packages/<name>/<version>/` and `shared/providers/<name>/<version>/`
> read as packages, laid out as `shared/` is with a provider's
> `resources/`; `package.jadpo` and `provider.jadpo` checked; names bare
> inside a package and written through it outside (`use packages/postcodes`,
> `Postcodes.UkPostcode`, `Postcodes.uk.parse(...)`), each package's own
> code seeing only its own names; `public` as a package's API and the
> reachable-from-public rule in packages and domains (area 1); reserved
> brand names and a package name the app already uses; and `jadpo check`
> holding each installed package to its line and digest in `jadpo.lock`.
> The errors are under "Package errors" above, the choices in "Chosen in
> chunk 7.1" below. Next: `add`, `update`, `remove`, `link` and `unlink`
> with verification at install (chunk 7.1b2); library packages'
> translations (chunk 7.1b3); `connections:`, lazy loading and the
> network report (chunk 7.1c).
>
> **Built (chunk 7.1b).** The surface, `surface.jadpo_record`, written
> from a package's code and read by `jadpo package show` (under "The
> surface"); `jadpo package check` with the publishing bar, the version
> it allows and `type.standard_exists` on a package's public types (under
> "What publishing requires"); `publish.description_missing` and
> `publish.breaking_change` (under "Publishing errors"). The choices are
> in "Chosen in chunk 7.1b" below, with two questions for Richard (27 and
> 28), decided on 2026-10-20.
>
> **Built (chunk 7.1b2).** Installing: `jadpo package add` from a folder or
> a git repository's release tag, verified against its committed surface;
> `update`, with area 18's review of what changed; `remove`; `link` and
> `unlink`, every check compiling the app against a linked checkout and CI
> refusing it; and the bar's examples (question 27) and changelog
> (question 28). The errors are `install.surface_mismatch`,
> `install.fetch_failed`, `install.in_use` and `install.link_refused`
> under "Installing errors", and `publish.example_missing`,
> `publish.example_failed` and `publish.changelog_missing` under
> "Publishing errors"; the choices are in "Chosen in chunk 7.1b2" below.

### Chosen in chunk 7.1

What the pages leave to the build, each the most conservative reading:

- **A package's name is its folder's.** `packages/acme_couriers/v1/` is
  `AcmeCouriers.v1`: the folder's name in `UpperCamelCase` and its version
  folder, which the index has to give (`package.name_mismatch`). A version
  label is `v` and digits in groups joined by single underscores, `v2` or
  `v2024_06_20`; the lock sorts the groups as numbers.
- **What a package's code sees.** Only its own declarations, bare, and the
  built-ins: not the app's `shared/`, domains, connections or other
  packages, since an installed package is published apart from any app and
  the app's own integration is written so that it could be. Its files need
  no `use` lines. *Flagged for Richard:* the pages say a package is
  "checked as `shared/` is" without saying whether it may read the app's
  `shared/`; this takes no, which can be loosened later without breaking
  a package. Recommendation: keep it.
- **Bringing a package in.** `use packages/<name>` for an installed one and
  `use shared/providers/<name>` for the app's own bring the whole package
  in, as `use <domain>` does; there is no `{ ... }` form, since its names
  are always written through it. `jadpo fmt` sorts these lines after the
  domains'.
- **Names through a package.** A type (`Postcodes.UkPostcode`), a failure
  (`fails Postcodes.InvalidPostcode`), a type built from a value
  (`Postcodes.UkPostcode(text)`) and a public library's function
  (`Postcodes.uk.parse(text)`). A package's actions, subscribers and
  events (chunk 7.6) and capability mappings (chunk 7.3) are
  `toolchain.not_built_yet` where they are declared, as are a library
  package's translations (chunk 7.1b); a resource's
  operations are read with provider calls (chunk 7.2). Two versions of one
  package side by side, names carrying the version, are
  `toolchain.not_built_yet` too.
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
  `runs:`'s keys; the values of the others are read with the chunks that
  use them. `protocol:` is required, and `hosts:` with `runs:` is
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
  line is read but its checkout isn't compiled until `jadpo package link`
  (chunk 7.1b).

### Chosen in chunk 7.1b

What the pages leave to the build, each the most conservative reading:

- **The surface's entries.** In the record format's order: the package;
  for a provider, `provider`; each operation of a `public resource`; each
  function of a `public library`; each `public` type (`type Name =
  Base`), then each `public` failure; each folder in `recipes/`. A
  package's events, actions, polls and capability mappings join with their
  chunks (7.6, 7.3), its messages with 7.1b3. A library itself has no
  entry: its functions do. Declarations without `public`, and test files,
  are left out.
- **The package's entry.** `kind` is `provider` when the package has
  `provider.jadpo`; `licence` is the `Licence` variant's name (`mit`), as
  `protocol json_http` and `credential bearer` are written; `jadpo` is the
  index's text; `guide GUIDE.md` when the package carries one; `version`
  when it is known (below), and left out of an own integration's surface.
- **The provider's entry** holds what chunk 7.1 reads of `provider.jadpo`:
  `protocol`, `host live` and `host sandbox`, `runs image`, `port` and
  each `asks` host, `credential`, and `idempotency header` and `window`.
  Settings, webhooks, polls and erasure join with the chunks that read
  them.
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
  Otherwise the surface is compared with that release's by entry, as Elm's
  `bump` does: a removed entry, or an entry whose facts changed (a new
  `fails`, a field retyped or added, a variant added, a licence changed),
  breaks an app and is refused; only new entries, or a new `jadpo` or
  locale in the package's entry, is the next minor version; nothing but
  descriptions, files, sources and recordings is the next patch. A
  breaking change is a new package (`Postcodes.v3`), not a new number.
  Outside git, every check is a first release.
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
- **Not built yet.** `jadpo package new`, `import`, `record`, `conform`,
  `publish` and `recipe` say `toolchain.not_built_yet` until their
  chunks; `add`, `update`, `remove`, `link` and `unlink` came with chunk
  7.1b2.

### Chosen in chunk 7.1b2

What the pages leave to the build, each the most conservative reading:

- **Where `add` installs from.** A folder holding `package.jadpo` is a
  `path` source, locked relative to the app's root (`../postcodes`);
  `<repository>@v<version>` is a `git` source. A repository written
  `host/owner/name` is fetched over HTTPS, a URL (`https://`, `ssh://`,
  `file://`, `git@host:`) as written, and a folder on this machine
  (starting `.` or `/`) from its path, locked relative to the root. Git
  runs with the developer's or CI's own access and never prompts. A Jadpo
  app's folder (another app's OpenAPI) waits for the importer (chunk 7.2),
  and a bare name for the registry; both say `toolchain.not_built_yet`. A
  source whose path has a space is refused, since the lock writes each as
  one word.
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
  failed copy leaves the old one, and its digest is 7.1's.
- **What `add` refuses.** A package already in the lock (`update` moves
  it); a package whose name the app's own code uses, which is installed,
  checked and undone, showing `package.name_taken`. A third party's git
  package still installs without an acceptance until `jadpo approval`
  reports publisher trust (area 18).
- **`update <name>`.** A `path` package is verified again from its folder;
  a `git` one moves to the newest `v<version>` tag whose committed surface
  names the same package, never to an older one, and a folder or tag now
  holding another major version is refused, since that is a new package.
  It prints area 18's review: each surface entry added, changed or
  removed, `used by the app` when the app's code writes its name through
  the package (`Postcodes.uk.tidy`), and `needs a person` for a changed or
  removed one the app uses. The acceptance itself waits for `jadpo
  approval`. A linked package is unlinked first.
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
  `jadpo check` and `link` show its problems at the checkout's paths. A
  checkout with no `package.jadpo` can't be read. `link` itself shows the
  app's check against it. `jadpo dev` and `jadpo test --watch` read the
  checkout when they start: an edit there shows at their next start.
- **Where a link is refused.** CI is the `CI` variable set to anything but
  empty, as CI services set it and as `jadpo suggest share` and the
  terminal's animation already read it; there `jadpo check` and `jadpo
  test` refuse each link (`install.link_refused`). Area 14 names no
  release or production profile, and every production build goes through
  `jadpo build` (Phase 8), which will refuse a link whatever the
  environment.
- **`unlink <name>`.** Puts back the line `link` kept; with none (a
  package linked before it was installed), the lock no longer names it and
  the hint says to add it.
- **Examples (question 27).** A ```` ```jadpo ```` block in the `///`
  description of any public declaration; other fences are prose. Each runs
  as a test named `example of <name>` (then `example 2 of <name>`), placed
  after the package's own tests in the test file beside the declaration's
  file, in the scratch app. One that doesn't compile is shown at its line
  and left out so the rest run. A failure counts as shown when an example
  or one of the package's tests has a line calling the function with
  `fails <Failure>`. The bar's row counts examples; the package's own
  tests stay the `tests` row.
- **The changelog (question 28).** `CHANGELOG.md` beside `package.jadpo`,
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

### Consistency with the rest of the language

The package design changes decisions made in other areas:

| Area | Decided | Change |
|---|---|---|
| 1 | `integration` keyword in each domain's `integrations/` folder, contract beside it ("Keywords match folders") | Connections in `app.jadpo`; installed packages in a root `packages/` folder and `jadpo.lock`, the app's own integrations in `shared/providers/` (revised 2026-10-16; the root `providers/` of 2026-10-13 goes); a domain's ownership becomes the connection's `domains:` list |
| 1 | `public` on a declaration nothing outside its domain uses is an error | A package's public members are its interface and are exempt; everything reachable from a public declaration must be public, in packages and domains alike (2026-10-16) |
| 7 | Question 8: webhooks are hand-written routes in the `webhooks` API with a unique rule on the event id | Generated from the package; repeats dropped by the runtime |
| 7 | One OpenAPI document per API in `build/openapi/` | Also served at `<prefix>/openapi.json` with `x-jadpo` fields, which the importer reads (2026-10-16) |
| 8 | A webhook route's authentication is `StripeV2024.signature` | No longer needed; the generated route checks the signature |
| 8 | Sign-in provider presets for common providers | Google, Microsoft and other OpenID Connect providers are `Driver.oidc` with an issuer; presets only for GitHub and Apple (2026-10-16) |
| 6, 10 | A rerun takes a fresh `clock.now` | Once a multi-step record exists, every rerun uses the time stored in it (2026-10-16) |
| 12 | One changing call per action; a run cut off after an outside call is `outcome_unknown` | Up to 8 keyed changing calls in subscribers and keyed routes; a run cut off with a record resumes (2026-10-16) |
| 13 | Question 7 (open): every reachable operation needs a fake, or the test does not compile | Unscripted calls answer from the generated fake; scripted lists keep area 13's leftover check; fakes keep keys and gain `lost` and `in_use`; crash-point reruns (2026-10-16) |
| 17 | Text values in development timelines are hashed | Provider call payloads are shown in full in development only (2026-10-16) |

Other points:

- **Two dots in a call.** `couriers.bookings.create` is the first call with
  two dots. The dot still names the owner: the connection owns its
  resources and actions, a resource its operations. A library package is
  called through its own name (`Postcodes.uk.parse`), as the standard
  library is.
- **No anonymous objects** (area 2). The rethink's `create({ amount: ... })`
  becomes a named package type, `AcmeCouriers.CreateBooking { ... }`.
- **Area 4's mapping form** holds: `condition: Failure` in the call's block,
  kind checked, the provider failure kept as the internal cause.
- **Areas 3, 9 and 12 hold:** calls only from actions, the changing calls
  before the writes (area 3's amendment cites question 7, which keeps its
  number), whoever may run the action may call (area 9).
- **Area 16** sends email through the `email` capability: SMTP is one
  built-in driver, a mail package another. `EmailBounced` and
  `EmailComplained` go: bounces are the package's own events.
- **`public` on libraries.** Area 1 makes a library public as a whole and
  refuses `public` on a function; packages follow it, so a package's public
  functions are its public libraries' functions, and its decoders live in
  a library that is not public (2026-10-16).
- **Correction.** Twilio signs webhooks with its own HMAC-SHA1 scheme, not
  Standard Webhooks as the webhooks section said (checked 2026-10-13); see
  question 25.
- **Rule IDs.** The `semantic.service_*` rules below check the `service`
  declaration in the examples. Under packages, the pin, digest and egress
  rules become `jadpo.lock` and package checks (area 14).

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Root `providers/` folder and `jadpo.lock`; `integrations/` folder and keyword removed | 1 | Input to area 1 (Revised 2026-10-16: root `packages/` for installed packages, `shared/providers/` for the app's own) |
| Generated webhook routes in the `webhooks` API, replacing question 8's hand-written routes; every Jadpo API publishing a package from its API record | 7 | Input to area 7 (Revised 2026-10-16: every API serves its OpenAPI with `x-jadpo` fields, and the importer makes the package) |
| Webhook signature as the generated route's authentication; service keys as the credential for calling another Jadpo app | 8 | Input to area 8 |
| Integration time limits inside a route's deadline: the compiler warns when `timeout` times `attempts` exceeds a reaching route's deadline | 7 | Answered here (Revised 2026-10-16: summed over every step on the longest path) |
| The delivery key as the idempotency key; one changing call per action; `outcome_unknown` parked; provider events published as events | 12 | Answered here, agreeing with area 12 (Revised 2026-10-16: keys derived per step; several keyed changing calls; records resume a cut-off run) |
| Fakes named `connection.resource.operation`; generated defaults; contract runs required for unconfirmed operations | 13 | Input to area 13 |
| `jadpo provider add`, `upgrade`, `import <format>`, `record`; the registry client; `jadpo.lock`; contract runs in CI | 14 | Input to area 14 (its question 15) (Revised 2026-10-16: the `jadpo package` commands) |
| Connections that receive marked fields, listed in the personal-data report | 15 | Input to area 15 |
| Approval report entries: operations, data sent, package diffs, unconfirmed operations, calling domains | 17 | Input to area 17 (2026-10-16 adds services, unverified publishers and providers that cannot erase) |
| Readiness does not probe providers; a provider outage should not take the app out of service | 7 | Answered here |
| Magic links and OIDC code exchange (area 8) are ordinary providers; no special feature | 8 | Answered here (Superseded 2026-10-16: magic links and sign-in providers are built in, area 8's `sign_in:`.) |
| Mail templates and translations for message bodies | 10 | Deferred (Decided 2026-10-16: templates read `translations.<domain>.<message>`; see areas 10 and 16.) |
| `connections:` replacing `stores:`; `driver: Driver.x` replacing `engine:`; cache in memory in development | 5 | Questions 20 and 23 |
| `Sms` type base and a `PhoneNumber` type | 2 | Question 19 (Superseded 2026-10-16: SMS is not a capability; phone numbers are built in for chunk 5.4.) |
| Event delivery stays in the runtime; publishing targets fed from the outbox; schedule ticks chosen by target | 12 | Questions 19 and 24 |
| Mail and files sections use capabilities; outgoing webhooks as a built-in publishing driver | 16 | Questions 19 and 21 |
| One fake per capability (`mail.sent`, `sms.sent`, `published`) | 13 | Question 22 (Revised 2026-10-16: `email.sent` and `published`; `sms.sent` goes) |
| `jadpo provider conform`; local inbox for SMS and published events; Wrangler config generated per target | 14 | Questions 22–24 (Revised 2026-10-16: `jadpo package conform`; no SMS inbox) |
| Capabilities in the manifest, network and personal-data reports; `erase` mappings generating `PersonErased` subscribers | 15, 18 | Question 21 (Revised 2026-10-16: erasure is `erasure:` in `provider.jadpo` with the connection's key) |
| `privacy: { self_service: { mail: } }` names the app's mail connection | 15 | Question 20 (Superseded 2026-10-16: every email goes through the one `email:` connection; area 15's `self_service: {}`.) |
| A package input taking any of the app's object types, sent as JSON | 2 | Decided 2026-10-16; syntax for area 2 |
| `clock.now` fixed for the life of a multi-step record | 6, 10 | Decided 2026-10-16 |
| The run-limit row, the operator's retry `--not-done` and abandon for unfinished records, `events: { store: }` for records | 12 | Decided 2026-10-16 |
| Fakes keeping keys, `lost` and `in_use`, crash-point reruns, grants as fixtures | 13 | Decided 2026-10-16 |
| "Provider answers for unfinished actions, at most 30 days" in the personal-data report; erasing a person deletes and revokes their OAuth grants | 15 | Decided 2026-10-16 |
| The `search`, `bot_check`, `documents` and `scanning` capabilities, the disk driver, image variants, telemetry | 16 | Decided 2026-10-16 |
| `jadpo watch`, `jadpo calls` in the developer console; call payloads in full in development | 17 | Decided 2026-10-16 |
| `.jadpo_record` files, the lock's source field, acceptances for services, unverified publishers and providers that cannot erase | 18 | Decided 2026-10-16 |
| `jadpo batches list`, `show` and `cancel` for operators | 14 | Decided 2026-10-16 |

### Open questions

Questions keep their numbers, because other pages cite 6 and 7. Questions
1 to 5 and 9 to 12 were decided on 2026-10-12 and reopened on 2026-10-13;
13 to 25 were decided on 2026-10-13; many were revised on 2026-10-16 by
packages and extension points, as each note says; 26 was decided then.

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
   *Revised 2026-10-16 (Richard):* the limits are a provider package's,
   covering its library types too; a library package's types may be
   stored and used in route inputs and outputs, and a changed stored type
   appears in the upgrade plan and the migration. A package input may take
   any of the app's object types, sent as JSON.
2. **How is an operation called?** *Reopened 2026-10-13;* first answered as
   `StripeV2024.create_payment_intent(...)`. *Recommendation:*
   `connection.resource.operation(input)`, only from actions, with `attempt`
   or `match`, which reads like the provider's own SDK, so agents already
   know it. *Decided 2026-10-13:* `connection.resource.operation(input)`, as recommended.
   *Revised 2026-10-16 (Richard):* a provider package's public actions are
   called the same way, `connection.action(input)`, sharing the
   connection's namespace with its resources; a library package's
   functions are called through the package's name
   (`Postcodes.uk.parse(text)`), with no connection.
3. **How are provider versions handled?** *Reopened 2026-10-13;* first
   answered as one integration per version. *Recommendation:* one package per
   version; two versions are two connections run side by side; upgrade by
   adding the next, moving callers, and deleting the old. The approval
   report shows the diff of the operations the app uses. This is still
   Stripe's own advice. *Decided 2026-10-13:* yes, as recommended.
   *Revised 2026-10-16 (Richard):* two versions of a library package can
   also sit side by side while callers move, as can a candidate version
   while the upgrade plan compiles against it; the diff is the surface
   diff.
4. **How is a package pinned?** *Reopened 2026-10-13;* first answered as a
   contract file whose digest goes in the approval record.
   *Recommendation:* registry packages by digest in `jadpo.lock`, local ones
   by being committed; never fetched during a build. Cargo and npm lock
   files work this way. *Decided 2026-10-13:* yes, as recommended.
   *Revised 2026-10-16 (Richard):* installed packages are committed under
   `packages/<name>/<version>/`, not kept in a cache, read-only and checked
   against their digest in `jadpo.lock`; the lock records each one's
   source (`registry`, `git`, `path` or `link`); `jadpo package add` and
   `update` regenerate the surface and refuse one that differs from the
   committed `surface.jadpo_record`. The app's own integrations live in
   `shared/providers/` with no lock entry.
5. **Which failures can be mapped, and to what?** *Reopened 2026-10-13;*
   first answered with the contract's error responses. *Recommendation:* the
   package's failures, by code, at the call, to any declarable failure of a
   fitting kind; `timed_out` to an `unavailable` failure only on idempotent
   operations; anything unmapped is a fault. A package failure is never
   listed in an app's `fails`, so a provider's codes never become part of
   the app's API. *Decided 2026-10-13:* yes, as recommended.
   *Revised 2026-10-16 (Richard):* the provider's "different request"
   answer to a key is `diverged`, never `misconfigured`, and its "key in
   use" answer is waited on and retried, never a failure; a package
   action's failures are mapped twice, by the package and then at the
   app's call, and replayed failures are mapped again by the current code.
6. **Who supplies the idempotency key?** *Recommendation:* the runtime: the
   delivery key in subscribers and schedules, the client's key or request id
   in routes; the operation only says where it goes. Today's
   `ReminderIntentId` and its two clauses go. *Decided 2026-10-12:* yes, as recommended.
   *Revised 2026-10-16 (Richard):* those keys are inputs, never sent as
   they are: the key sent is `jd1_` and 32 base32 characters of a SHA-256
   over the application, environment, connection, caller scope, action
   key, step number and key generation, cut to the package's
   `max_length:`, because Stripe and others scope keys to the account, not
   the caller. The package must give its key `window:`.
7. **May one action call a provider and also write?** Today no write at all
   is allowed. *Recommendation:* yes, if the call comes first; the writes
   commit after it returns, and a failed commit retries the call with the
   same key. Writes before the call stay an error; that case emits an event. *Decided 2026-10-12:* yes, as recommended.
   *Revised 2026-10-16 (Richard):* an action may make up to 8 changing
   calls, each keyed or safe to repeat, only where its key survives a
   retry (subscribers, and routes whose API requires `Idempotency-Key`),
   recorded step by step and replayed on a rerun; a changing call with no
   key stays the only one in its action. See "Several changing calls in
   one action".
8. **Which retry settings exist?** *Recommendation:* `timeout:` (default
   10s) and `retry: { attempts: within: }` (default 3 within 30s), the same
   words as area 12's subscriber retry; exponential backoff with full jitter
   fixed. Temporal exposes five knobs, mostly left at defaults. *Decided 2026-10-12:* yes, as recommended.
   *Revised 2026-10-16 (Richard):* `max_concurrent:` in `provider.jadpo`
   caps calls at once per app instance, and a connection may lower it; a
   step in doubt is resent only within the provider's key window.
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
   *Revised 2026-10-16 (Richard):* where an API is comes from exactly one
   place, `hosts:`, `runs:` or the connection's `host:`; a service reaches
   only the hosts its connection grants with `outbound:`, each new grant in
   the approval report; outgoing webhooks get the one runtime-owned network
   exception (area 16).
10. **Which credential schemes first?** *Reopened 2026-10-13;* first
    answered on the integration. *Recommendation:* the package declares the
    scheme and the connection gives only `credential: config.x`. Bearer,
    named header and basic first; AWS signing arrives with the Smithy
    importer and the first AWS package; OAuth client credentials when a real
    app needs them. *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* a connection's credential may also be
    an `OAuthGrant` owned by one of the app's records, with the provider's
    addresses in `oauth:` and every call naming its owner with
    `on_behalf_of:`. See "Acting for each user".
11. **How are webhooks received?** *Reopened 2026-10-13;* first answered as
    a hand-written route using the integration's signature check.
    *Recommendation:* generated from the package when the connection has a
    webhook secret: signature checked with a five-minute tolerance, repeats
    dropped by event id, each event published as a Jadpo event. No webhook
    route is written by hand. Area 7's question 8 is amended. *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* events go to the outbox public or not
    as the package marks them, each with the provider's `occurred_at`; a
    package may subscribe to and enrich its own webhooks; webhooks that
    need a reply name the app's answering action; handshakes are built-in
    rules; queues arrive through `receiving`. See "Receiving from outside".
12. **How are fakes made and checked?** *Reopened 2026-10-13;* first
    answered as fakes checked against the contract file. *Recommendation:*
    a fake generated per package answers with valid defaults; a test
    scripts responses with `fakes:`; the compiler checks scripted responses
    against the package's types; `jadpo test --kind contract` runs against
    the sandbox in scheduled CI and must cover every unconfirmed written
    operation. *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* fakes keep idempotency keys as the
    provider does and gain `lost` and `in_use`; `jadpo test` reruns
    multi-step tests with a crash at each step boundary; `jadpo dev` fakes
    every provider call by default too.
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
    *Revised 2026-10-16 (Richard):* the command is `jadpo package add`; it
    also takes a git repository and another Jadpo app's OpenAPI; imports
    and hand-written packages are the app's own integrations in
    `shared/providers/`.
14. **How is an operation's origin recorded and confirmed, and do
    production builds refuse unconfirmed ones?** *Recommendation:* `source:
    PackageSource.imported` or `.written` (with `docs:`) on every operation;
    `jadpo provider record` saves a placeholder-valued sandbox response that
    the output type is checked against. Unconfirmed written operations are
    flagged in the approval report and need a passing scheduled contract run,
    but do not block a build: a sandbox outage should not stop a release. *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* the command is `jadpo package record`,
    and every decoder must decode every recording.
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
    *Revised 2026-10-16 (Richard):* the registry holds Jadpo packages of
    both kinds (the first recorded idea, now decided), which carry Jadpo
    source, not declarations only, and never native code or install
    scripts; it repeats the install checks on upload and adds discovery,
    curated packages marked official, publisher verification, signing and
    a documentation website rendered from surfaces. Private packages come
    from git, with no private registry. Jadpo APIs no longer publish
    packages from their records: another app's package is imported from
    their served OpenAPI. Upgrade plans stay area 18's review of the
    surface diff.
16. **How are packages and their types named?** *Recommendation:* the
    provider in UpperCamelCase and its own version label, `Stripe.v2024_06_20`,
    `AcmeCouriers.v1`. Types, failures and events are named by the package
    without the version (`Stripe.PaymentIntent`), so two connections to one
    version share types; while two versions are connected during an upgrade,
    names carry the version (`Stripe.v2025_01_01.PaymentIntent`). *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* library packages are named the same way
    (`Postcodes.v2`, `Postcodes.UkPostcode`); curated provider brand names
    are reserved, and installing a package whose name the app's own code
    uses stops until the app's own is renamed.
17. **How are connections named?** *Recommendation:* lower_snake_case,
    chosen by the app (`stripe`, `couriers`); they share one namespace with
    domains and libraries, so a clash is a compile error. Resource and
    operation names come from the package, following the provider's SDK where
    the importer can map them. *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* a package's resources and public actions
    share the connection's namespace, so a clash between them is an error
    in the package.
18. **Which domains may call a provider?** Area 1 put integrations in a
    domain, so only that domain called them. *Recommendation:* the
    connection lists them, `domains: [payments]`, required; other domains go
    through that domain's public actions. It keeps area 1's ownership, and a
    new caller is a one-line diff in review. *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* the list also covers the package's own
    operations on a connection a capability uses (`email:` uses the
    connection; only the listed domains call it); a library package needs
    no connection and any domain may use it.
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
    generic escape hatch (its "bindings") undoes the point. *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard, design C):* a capability exists only for a
    job Jadpo performs itself and covers it whole, with no optional parts.
    `sms` goes (nothing in the language sends SMS; Twilio is a package);
    `receiving`, `search`, `bot_check` and `documents` join, and `scanning`
    when an app needs it. Everything provider-specific (hosted templates,
    bounces, suppression lists) is the package's own operations and
    events.
20. **Do `stores:` and `providers:` become one `connections:` block?**
    Raised 2026-10-13. *Recommendation:* yes. Each entry is
    `driver: Driver.x` (built in) or `package: X.vN`, with typed settings
    the driver or package declares. The use site says which capability it
    serves: `store:` on entities, caches and `File` fields; `email:`, `sms:`
    and `publishing:` in `application { }`. One connection per resource
    (bucket, queue), not per vendor account, following Cloudflare's
    bindings: R2 and Queues need different credentials, and cron none. *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* there are no presets as packages:
    anything Jadpo has a built-in driver for is configured in `app.jadpo`
    (R2 is `Driver.s3` with its host and `region: "auto"`), with vendor
    settings documented on each driver's page; a vendor with storage and an
    API is a driver connection plus a package. `sms:` goes; `receiving:`,
    `documents:` and `bot_check:` join the use sites.
21. **How does a package provide a capability?** Raised 2026-10-13.
    *Recommendation:* `provides:` in the index names a `provide <capability>`
    file mapping each standard operation, standard event and optional
    `erase` onto the package's own operations, with field paths, literals,
    lists, nested blocks and enum tables only. Provider features come
    through typed connection settings and the package's own API, never
    through the capability. Protocol drivers, signing and webhook checks are
    built into the runtime, so packages stay declarations only. An `erase`
    mapping generates area 15's `PersonErased` subscriber. *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* the mapping is a file,
    `provides/<capability>.jadpo`, found by name, and the index's
    `provides:` goes; mappings stay data and gain `each:` and `as:`; standard
    events and `erase` leave them: bounces are the package's own events,
    and erasure is `erasure:` in `provider.jadpo` with the connection's key.
22. **How is a capability package proven, and how is it faked?** Raised
    2026-10-13. *Recommendation:* one conformance suite per capability, run
    by `jadpo provider conform` against the sandbox or an emulator; the
    registry lists a capability only with a passing run pinned to the
    digest, and local packages run it in scheduled CI. Tests get one fake
    per capability (`email.sent`, `sms.sent`, `published`), whatever the
    provider; a package's own operations keep their generated fake. *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* the command is `jadpo package conform`;
    `sms.sent` goes with `sms`.
23. **What runs in development?** Raised 2026-10-13. *Recommendation:*
    nothing leaves the machine: mail and SMS to the local inbox, files to
    local disk with local signed links, published events to a log in the
    dev console, schedules on the local timer. The database runs its real
    engine; the cache runs in memory. That narrows area 5's "same engine in
    every environment" for caches, which is safe because a cache cannot
    change what a request returns. Encore and Rails do the same. *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* every provider operation is faked by
    default too, answered from recordings and generated defaults; named
    operations or resources reach the sandbox through `development: {
    sandbox: [...] }` on the connection; every call is logged with its
    payload in full; `jadpo dev` starts the declared services; search runs
    on PostgreSQL.
24. **How do targets limit drivers?** Raised 2026-10-13 with the native
    and WebAssembly targets. *Recommendation:* every driver and package
    lists the targets it runs on; `jadpo check` refuses a deployable that
    uses one elsewhere and names the alternative. The schedule tick is the
    target's: an in-process timer natively, Cloudflare cron on Workers
    (generated into the Wrangler config, also sweeping the outbox), or an
    external tick. On Workers: TCP works but port 25 is blocked, so SMTP
    needs port 587; there is no local disk; bindings exist only there. *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* a package that calls an HTTP API runs
    everywhere; a service (`runs:`) runs only beside a native deployable,
    so on Workers the connection gives its `host:`; the disk driver is
    native only, and an in-memory cache is an error on Workers.
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
      callback). *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* a sandbox that is a test credential is
    written as the same host twice in `hosts:`; R2 is no longer a package,
    so its host is the app's to write, and placeholders remain for package
    hosts such as a Supabase project's. Handshakes join the webhook checks
    as `handshake:`.

26. **What happens when a write is refused after the call took effect?**
    Raised 2026-10-15 by an outside review. The call comes before the
    writes, so a write refused afterwards for good (a unique rule, a
    restricted reference) rolls back the database's only record of a change
    the provider kept, and retrying the request refuses again.
    *Recommendation:* `jadpo check` warns at a write that can be refused
    after a changing call, naming the two fixes: check the condition with a
    read before the call, or record the call's result first and do the
    refusable work in a subscriber of an event that write emits.
    *Decided 2026-10-16 (Richard), with several changing calls in one
    action:* the warning covers every point after the first changing step
    that can end the action for good (a step's failure, a refusable write,
    `reject`, a called action's failure) unless handled with `match`, and
    names three fixes: check first; record the answers and finish in a
    subscriber; undo as another step. The run ends with its record kept, so
    an operator can retry once the cause is fixed, replaying every answer
    without contacting the provider.
27. **How is an example in a description written?** Raised 2026-10-20 by
    chunk 7.1b. The publishing bar requires every function of a public
    library to have an example in its description, compiled and run at
    publish, and every failure it can raise shown by an example or a test,
    but no page says what an example looks like inside `///`.
    *Recommendation:* a fenced block in the description, ```` ```jadpo ````
    to ```` ``` ````, holding statements as a test's body does (`assert
    uk.parse("sw1a1aa") == "SW1A 1AA"`, `assert uk.parse("x") fails
    InvalidPostcode`); `jadpo package check` runs each as a test of the
    package, named after its function, and a failure counts as shown when
    an example or a test asserts it with `fails`. It reads as Markdown in
    the registry's page and `jadpo package show`, and uses nothing new.
    *Decided 2026-10-20 (Richard), as recommended; built in chunk 7.1b2.*
28. **What does the changelog hold, and who writes it?** Raised 2026-10-20
    by chunk 7.1b. The bar says the changelog's list of changes is
    generated from the surface diff and the author writes only why, but no
    page gives `CHANGELOG.md`'s form. *Recommendation:* `jadpo package
    check` writes a `## <version>` section at the top of `CHANGELOG.md` for
    a new version, its generated list of the surface's changes under it
    (`added type OutwardCode`), and keeps the author's text beneath; the
    bar refuses a release whose section has no text of the author's. Until
    decided, the bar says the item is not checked yet. *Decided 2026-10-20
    (Richard), as recommended; built in chunk 7.1b2.*

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
| Idempotency keys | Supplied by the runtime: delivery key in subscribers and schedules, the client's `Idempotency-Key` or request id in routes (Revised 2026-10-16: the key sent is derived from these by hashing, per step) | 2026-10-12 |
| Call then write | Allowed when the call comes first; writes commit after it; a failed commit retries the call with the same key; area 3's text amended (Revised 2026-10-16: up to 8 keyed changing calls, recorded and replayed) | 2026-10-12 |
| Retry settings | `timeout:` (10s) and `retry: { attempts: within: }` (3 within 30s); backoff fixed | 2026-10-12 |
| Outbound host | From `base_url`, HTTPS only, no redirects or proxies; a sandbox host chosen by configuration (Superseded 2026-10-13: reopened as question 9.) | 2026-10-12 |
| Credential schemes | `CredentialScheme.bearer`, `.header`, `.basic` first (Superseded 2026-10-13: reopened as question 10.) | 2026-10-12 |
| Webhook signatures | On the integration (`webhooks: { signature: SignatureScheme.x secret: }`), used as `authentication: StripeV2024.signature`; five-minute tolerance (Superseded 2026-10-13: reopened as question 11.) | 2026-10-12 |
| Checking fakes | Fakes checked against the contract by the compiler; `jadpo test --kind contract` against the sandbox in scheduled CI (Superseded 2026-10-13: reopened as question 12.) | 2026-10-12 |
| Types from the package | Namespaced package types usable anywhere in code; no narrowing needed; provider object and enum types barred from entity fields and route `Input`/`Output`; ids and plain values allowed (Revised 2026-10-16: library package types may be stored and used in routes) | 2026-10-13 |
| Calling an operation | `connection.resource.operation(input)`: the connection from `app.jadpo`, the resource group and operation from the package | 2026-10-13 |
| Versions as connections | One package per version; two versions are two connections side by side; the approval report shows the diff of operations used | 2026-10-13 |
| Package pin | Registry packages by digest in `jadpo.lock`, local ones committed; never fetched during a build (Revised 2026-10-16: installed packages committed in `packages/`, read-only, with their source in the lock) | 2026-10-13 |
| Failures at the call | Package failures mapped by code at the call to declarable failures; `timed_out` to `unavailable` only when idempotent; unmapped is a fault; never in the app's `fails` | 2026-10-13 |
| Hosts | Outbound access denied by default; only hosts of declared connections; no raw `fetch`; generated network report; changes in the approval report; no separate host list | 2026-10-13 |
| Credentials | The package declares the scheme; the connection gives `credential: config.x`; bearer, header, basic first; AWS signing with the first AWS package | 2026-10-13 |
| Webhooks as events | Generated when the connection has a webhook secret: signature checked, repeats dropped by event id, each published as an event; no hand-written route | 2026-10-13 |
| Fakes and contract runs | Generated fake per package; tests script responses checked against the package; scheduled sandbox run covers unconfirmed operations | 2026-10-13 |
| The no-spec path | `jadpo provider add` tries, in order: registry (official first), published specification, SDK types / Postman / HAR importers, written from docs; records the step used (Revised 2026-10-16: `jadpo package add`, which also takes git and another app's OpenAPI) | 2026-10-13 |
| Provenance and confirmation | `source: PackageSource.imported` / `.written` (with `docs:`) per operation; `jadpo provider record` confirms; unconfirmed flagged in the approval report and covered by the contract run; builds not blocked (Revised 2026-10-16: `jadpo package record`) | 2026-10-13 |
| The registry | Jadpo's own, declarations only, signed and digest-pinned; first build: package format, connections, fakes, OpenAPI importer, hand-written packages, `jadpo provider record`, curated packages in the Jadpo repo; hosted registry, other importers and upgrade plans later (Revised 2026-10-16: both kinds, Jadpo source, publisher verification; private packages from git) | 2026-10-13 |
| Package naming | `Stripe.v2024_06_20`; types `Stripe.PaymentIntent`, versioned names only while two versions are connected | 2026-10-13 |
| Connection naming | lower_snake_case, chosen by the app, sharing a namespace with domains and libraries | 2026-10-13 |
| Calling domains | Each connection lists `domains: [...]`; other domains go through that domain's public actions | 2026-10-13 |
| Capabilities | Seven in the first build: `database`, `cache`, `files`, `email`, `sms`, `publishing`, `schedules`; event delivery stays in the runtime; adding one is a language change; packages cannot invent capabilities (Revised 2026-10-16: design C, below) | 2026-10-13 |
| Connections | One `connections:` block replaces `stores:` and `providers:`; built-in `driver:` or `package:`; the use site says which capability it serves; one connection per resource | 2026-10-13 |
| Providing a capability | `provides:` maps standard operations and events declaratively; provider extras through typed settings or the package's own API; protocols, signing and webhook checks built into the runtime (Revised 2026-10-16: `provides/<capability>.jadpo`, operations only, `each:` and `as:`) | 2026-10-13 |
| Conformance and fakes | One conformance suite per capability, required by the registry; one fake per capability in tests | 2026-10-13 |
| Development drivers | Nothing leaves the machine: local inbox, local disk, published-event log, in-memory cache; the database runs its real engine | 2026-10-13 |
| Targets | Drivers list the targets they run on; `jadpo check` refuses mismatches; the schedule tick comes from the target | 2026-10-13 |
| Gaps from the modelled packages | Test-credential sandboxes, placeholder hosts, credential shape by scheme, more webhook checks, `connection.webhook_url`, each built with the first package needing it | 2026-10-13 |
| Transaction during a call | None is open: reads before the call run in a snapshot closed before it, the transaction opens after it, and writes to records loaded before it check the row version (area 6) (Richard, from an outside review) | 2026-10-15 |
| Kinds of package | Library and provider, in one format, the kind following from whether it has `resources/`; no feature or driver packages; library types may be stored; everything in a provider package follows the provider rules (Richard) | 2026-10-16 |
| The golden rule | A package can do nothing the app cannot, and the app can do everything a package can; the app's own integrations use the package format whole; the gate compiles package examples as own integrations too (Richard) | 2026-10-16 |
| The package format | Laid out as `shared/`; `package.jadpo` with a `///` description, `licence:` and `jadpo:` only; an optional `provider.jadpo` with a fixed set of keys; capability mappings in `provides/`; the index lists no files; a re-import rewrites only imported files; tests run at publish; no `emails/`; translations in library packages only; optional `GUIDE.md` (Richard, candidate B) | 2026-10-16 |
| What is public | `public` marks the API; everything reachable from it is public, in packages and domains; resources and actions share the connection's namespace; package events public or not (Richard) | 2026-10-16 |
| No presets | Built-in drivers are configured in `app.jadpo`, never packaged; vendor settings documented per driver and recognised by `jadpo check`; Google and Microsoft sign-in through `Driver.oidc` (Richard) | 2026-10-16 |
| No dependencies | A package cannot require another; shared vocabulary is in the standard library; anything else is copied (Richard) | 2026-10-16 |
| Where packages live | Installed packages committed under `packages/<name>/<version>/`, read-only and digest-checked; the app's own in `shared/providers/<name>/<version>/`; `use packages/…` marks installed code; publishing moves code to its own repository (Richard) | 2026-10-16 |
| Names | Curated provider brand names reserved; a clash with the app's own code stops an install until it is renamed; creating an integration warns on a taken name (Richard) | 2026-10-16 |
| The surface | `surface.jadpo_record`, generated, readable, each entry pointing at its file; `jadpo package show`; it decides the version; the registry renders documentation from it (Richard) | 2026-10-16 |
| Publishing | The seven-point bar, run by `jadpo package check` and `publish`; the app's own code skips it (Richard) | 2026-10-16 |
| Developing and private packages | A package in its own repository from `jadpo package new`; `jadpo package link` and `unlink`, a link refused in production builds and CI; private packages from git tags, no private registry; verification at install; third-party git packages unverified and accepted by a person; prose never trusted (Richard) | 2026-10-16 |
| Package code | No state or database; public package actions call only their own API; package subscribers on its own webhooks; events only about the outside world, so an emitting action is never public; polls for feeds; recipes copied into the app; `max_concurrent:` (Richard) | 2026-10-16 |
| Services | Stateless containers from `runs:`, pinned by digest, started only after a person accepts the image, outbound only as `outbound:` grants, never privileged, no host disk; Jadpo a client of Docker or Podman; `jadpo services` (Richard) | 2026-10-16 |
| Decoding | The package declares decoding (`wire:`, `decode:` and `encode:`, `wire_output:`, `output_at:`, `failed_when:`) and the runtime does it, lazily; translation is the app's; byte formats stay in the runtime; a value that will not decode is a fault (Richard) | 2026-10-16 |
| Capabilities, design C | Only jobs Jadpo performs itself, each whole: `database`, `cache`, `files`, `email` (attachments included), `publishing`, `receiving`, `schedules`, `search`, `bot_check`, `documents`, and `scanning` when needed; `sms` removed; provider extras and bounces are the package's own; mappings stay data with `each:` and `as:` (Richard) | 2026-10-16 |
| Erasure at providers | `erasure: { call: by: }` in `provider.jadpo` and `erasure: { key: }` on the connection; a generated subscriber after commit, retried until confirmed; a connection that cannot erase personal data needs an acceptance (Richard) | 2026-10-16 |
| Provider calls in development | Faked by default from recordings and generated defaults; `development: { sandbox: [...] }` lets named calls through to the sandbox, never live; `jadpo calls script`; every call logged in full in development (Richard) | 2026-10-16 |
| Receiving from outside | Webhook events to the outbox with `occurred_at`; replies through the connection's `replies:`; built-in handshakes; queues through `receiving` with `type_at:`, `id_at:` and `events:`; senders with no package as own integrations, partners as services (Richard) | 2026-10-16 |
| Several changing calls in one action | The multi-step review adopted with its ten fixes: up to 8 keyed steps, only from subscribers and keyed routes, none inside `for`; each step recorded before it is sent and replayed by fingerprint, `diverged` held on any difference; `clock.now` fixed per record; derived keys and provider key windows; one runner; operator retry and abandon; records deleted in the commit, encrypted, capped and reported; a rerun under another build compares the step shape, with no deploy command; crash-point tests (Richard) | 2026-10-16 |
| OAuth grants | A credential may be an `OAuthGrant` owned by a record; connecting through generated endpoints; grants held encrypted by the runtime; `on_behalf_of:` on every call; automatic refresh; scopes checked at compile time; policy decides who connects (Richard) | 2026-10-16 |
| Packages from other Jadpo apps | Served OpenAPI with `x-jadpo` fields; one importer; added from a path, git or a URL, pinned in the lock; an ordinary provider package with `host:` and a service key; minor and major versions follow the other API (Richard) | 2026-10-16 |
| Choosing a provider when deploying | Designed now, built when the first app other people run needs it: every supported connection compiled and checked, a setting picks one at start, an unchosen one needs no credential (Richard) | 2026-10-16 |
| Question 26 | Answered by the multi-step warning, for single calls too (Richard) | 2026-10-16 |
| Settled for the build | Replies: an event's `reply:` names its reply type, keyed in `replies:` by the event's lower_snake name; `idempotency: { header: window: max_length: in_use: reused: new_key_on: replays_failures: }`; `idempotent: true` marks an operation safe to send again; `scope:` per operation; `limits: { attachments: }` in `provides/email.jadpo`; `item` in `as:`; the licence list with `.proprietary` for git only; the `x-jadpo-*` names; `ErasureNotPossible`; `public` on libraries, never functions; sign-in providers as connections (`Driver.oidc`, `Driver.github`, `Driver.apple`); OAuth grant routes, `jadpo_grants`, `grants: { encryption_key: }`, the `update` grant to connect, server-side `state` (decided for Richard by the coordinator) | 2026-10-16 |
| Mapping imported types (Richard) | An import maps fields to standard types in six layers: exact format mappings; shape and name detection with `type.standard_exists`'s matcher, the provider's shape decoded at the edge; declinable findings with `(agent)` steps on flagged fields; one broad review instruction scoped to the types the app's operations reach, tracked per type as reviewed or not in `jadpo check` and `jadpo approval`; confirmation by recorded responses; `type.standard_exists` as the safety net. Built in 7.2 | 2026-10-17 |
| An example in a description (question 27) | A fenced code block in a public declaration's description, written as a test body and run as a test of the package by `jadpo package check` (Richard, as recommended, 2026-10-19 standing instruction; built with 7.1b2) | 2026-10-20 |
| A package's changelog (question 28) | `jadpo package check` writes a section per version holding the list of changes it works out from the surface, and the author adds why beneath it (Richard, as recommended, 2026-10-19 standing instruction; built with 7.1b2) | 2026-10-20 |
