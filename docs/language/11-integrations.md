---
title: External services
area: 11
status: decided
checked-with: this checkout's jadpo on every gate run
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

There are two kinds, in one format:

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
  are built in. Where agents keep getting one wrong, the
  fix is a worked reference in `examples/`.
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
- **Built in or a library package** is decided by one rule:
  a thing is built in when the compiler gains from knowing it (a type that
  turns a mistake into a compile error, carries personal-data marking, has
  a SQL or OpenAPI form or generated test values, or a security boundary
  the language should own). Money, countries, currencies, languages, phone
  numbers, safe HTML and CSV are built in; postcodes, IBAN, VAT numbers,
  public holidays, repeating dates and QR codes are packages.

### The golden rule

**A package can do nothing the app cannot, and the app can do everything a
package can**. A package is a folder of an app's own
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
  placed as the app's own integration in `shared/providers/`.
- The rule runs one way only: an app can do far more than a package
  (entities, actions over its data, routes, policy, emails).

## The package format

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
- **No `emails/` folder**: email templates, partials and layouts are the
  app's own, and an `emails/` folder in a package is a compile error. The
  golden rule runs one way, so a package doing less than the app is
  consistent.
- **Translations** only in library packages, for their own messages and
  failures, laid out as a domain's (area 10, "Packages' text: groups, calls and the surface"); an
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
`AcmeCouriers.Booking`, `AcmeCouriers.SlotTaken` (question 16), in a
`match` arm too (`AcmeCouriers.SlotTaken -> ...`), and an enum's values
the same way, `AcmeCouriers.Window.morning`. A failure's
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

Two versions
of one package, both installed or both the app's own, sit side by side,
each its own package in its own folder, and every name written through
either carries its version. One `use` line brings in both, since it names
the package's folder. Here the app's own `Postcodes` is held at `v2` and
`v3` while callers move across:

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
`package.version_needed`; once the old version goes, a name that still
carries the version is `package.version_unneeded`, whose edit drops it
(both under "Package errors"). While two versions of a library package
that carries translations are held, the app words each under a key
carrying the version, `postcodes_v3: { }`, as its names carry it (area 10,
"Jadpo's and packages' text").

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
  library or any declaration.
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
- A few curated packages (Stripe, Twilio and GitHub, imported from their
  published OpenAPI) live in the Jadpo repository; the hosted registry is
  not built yet (question 15).

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

Only `openapi` is built, from a local file (`jadpo package import
openapi ../orders/build/openapi/main.json`, `--as Orders.v1` to name it);
"How the OpenAPI importer reads a document" below says how it reads one. An agent then fills
what the source leaves out: which calls change something, idempotency,
error names, and decoding (OpenAPI often says only `integer` for Unix
seconds). A person reviews that diff. Corrections to a
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
> It writes the same to `build/report/providers.md` and `.json` for `jadpo
> approval` (not built yet) to show a person what ships unreviewed
> or unconfirmed. The import ends with the scoped instruction: the types
> the app's calls reach still to review, or, while it calls none of the
> package's operations, when the review starts; and the calls not yet
> confirmed. Importing again keeps `reviewed:`, less any type the
> description added to, which the summary lists under "To review again".
> The choices are in "The scoped review and unconfirmed operations"
> below.

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
>   help: docs/language/14-tooling.html#command-errors
> ```
>
> Writing an operation's output type from the answer when it has none,
> and a HAR capture's operations, are not built yet. The choices are in
> "Recordings" below.

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

> `jadpo package add` from a folder (`../postcodes`)
> or from a git repository at a release tag
> (`github.com/acme/postcodes@v2.1.0`): the package is compiled as the
> app's own integration would be, the surface its code gives has to be its
> committed `surface.jadpo_record` byte for byte (`install.surface_mismatch`),
> and only then is it copied into `packages/<name>/<version>/` and locked
> with its version, digest and source. `update` moves one to its newest
> release with area 18's review of what changed, `remove` takes out one the
> app no longer uses (`install.in_use`), and `link` and `unlink` are under
> "Developing and publishing a package". The registry, another Jadpo app's
> OpenAPI and publisher trust are not built yet; the choices are in "How
> packages are installed and linked" below.
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
locale, with a `///` description on every public message (area 10):
the bar's `wording` row, shown when the package carries translations,
holds every language it ships to completeness, and item 2 asks the
description of every public message.

> `jadpo package check`, run in a package's own
> repository, checks the package placed as the app's own integration in a
> scratch app (the golden rule) and shows each problem at its place in the
> package's folder. Item 1 is the check itself, `type.standard_exists` on
> the package's public types among it (area 19, "`type.standard_exists`"); item 2 is
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
libraries (question 17), so `billing.<name>` means one thing. A
connection named like a domain or a library is reported at the connection, the name `app.jadpo` can change without moving code:

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
  help: docs/language/01-project-layout-and-modules.html#semantic-duplicate-declaration

check failed: 1 error, 4 files
```

### The network report

`jadpo check` writes `build/report/network.md`, with a `.json` twin, once
the connections read cleanly: every host the app can reach, the connection
that reaches it, when (a package's live and sandbox hosts are chosen by its
environment setting), what for (a package, or a driver's capability) and
the domains that use it. It is built for small diffs, as the permissions
report is (area 9): rows in order of connection and host, each host as
written, no times or totals, so a new connection or a changed host is a
one-line change in review. For the app above:

| Connection | Host | When | For | Domains |
|---|---|---|---|---|
| `couriers` | `https://api.acme-couriers.example` | `config.couriers_environment` is `live` | `AcmeCouriers.v1` | `shipping` |
| `couriers` | `https://sandbox.acme-couriers.example` | `config.couriers_environment` is `sandbox` | `AcmeCouriers.v1` | `shipping` |
| `ledger` | the address in `config.ledger_address` | always | `Ledger.v1` | `billing` |

A database or cache connection is listed at the address in its `url:`
setting, for the domains whose entities it stores, a database's `For`
saying `without TLS` when its connection says `tls: false` and the setting
its server's certificate is checked against when it names an `authority:`
(area 5, "PostgreSQL over TLS"); a store of files on
the disk driver reaches no host and isn't listed. A package Jadpo runs
(`runs:`) is listed as the service it runs from its image; the hosts a
service asks for join the report with services, which are not built yet.

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
- **Retries** (question 8). `timeout:` (default 3s) and
  `retry: { attempts: within: }` (default 3 within 30s), exponential backoff
  with full jitter. The defaults' 9 seconds fit a route's 10-second default
  deadline, so a route making one call needs no `deadline:` of its own.
  Only safe repeats are retried: the request never left,
  the provider said it did nothing (honouring `Retry-After`), or the call
  is idempotent. The package's rate limits space calls out before the
  provider refuses them, and `max_concurrent:` caps how many run at once
  per app instance: further calls wait, and waiting subscriber deliveries
  retry later (under "How polls and `max_concurrent:` run"). Heavy
  work such as PDF rendering runs in subscribers, never in the route.
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
  unchanged (area 6, "Outside calls hold no transaction"). A
  changing call with no idempotency key is the only changing call in its
  action; several keyed calls follow "Several changing calls in one
  action" below. Work that must follow a write emits an event.

Here an action of a
domain the connection lists calls an operation through it, given the
package's own input type, built through the package's name, and its
package failure mapped at the call by its code to the app's failure of
the same kind. The package driver sends it as JSON over HTTP; how it is
sent and read is under "Decoding" below.

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
  code of one of the operation's `failures:` and names the app's failure
  of the same kind, which leaves the action with the provider's failure
  as its internal cause (area 4). One no arm maps is a fault when it
  comes, never passed on, so a package failure is never in the app's
  `fails` list. A `match` on the call handles the package's failures by
  name, as for a library function.
- **The runtime's own failures** come as they would for any outside call:
  `misconfigured` for a rejected credential or an address it won't send
  to, `unavailable` when the provider surely did nothing (it answered
  `408`, `429` or that the key is still in use, or the call never left)
  and for a keyed call that may have acted, `outcome_unknown` for an
  unkeyed one that may have (a server's error, or no answer in time).
- **Sent once no transaction is open.** The call goes through the
  capability seam. What the action has only read before it is a snapshot,
  closed before the call goes out, and its writes run in a transaction
  opened after it, holding each record loaded before the call to the row
  version it was loaded at (area 6, "Outside calls hold no transaction").
  A call after the action's first write is still the fault
  `call_in_transaction`, sent to nobody, and a change made while the call
  is out runs the action again, the call repeated under its key. `jadpo
  check` refuses a write placed before a later call
  (area 6's `data.write_before_call`), so checked code never meets the
  fault, and an item of a batch whose `each:` action calls is a batch of
  its own (area 12, "How a run is kept").

A connection's `retry:`, and `timed_out:` mapped at the call of an
operation safe to send again: here the courier is tried up to four times
within 20 seconds, and a lookup it doesn't answer within five seconds
is the shop's own `CourierUnavailable`:

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
  ends the retrying, and the last answer stands. Each
  attempt is logged as a call of its own.
- **`timed_out:`** maps the runtime's `timed_out` (area 4) on a call of
  an operation that changes nothing or is `idempotent: true`, to one of
  the app's `unavailable` failures, with `timed_out` as its cause, once
  every attempt has gone unanswered in time. Unmapped, a call that timed
  out is `unavailable` where it was safe to send again, and
  `outcome_unknown` where it wasn't; mapping it on an operation that
  changes something with no key is `failure.unknown_mapping`.

## Decoding

Two kinds of conversion, split by one test:
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
  uses one, since every boundary goes through the contract.
- **Byte formats stay in the runtime**: JSON, form-encoded, multipart, XML
  later. Package functions reshape values, never bytes.
- **Decoding is lazy where it can be.** A field decoder runs only when code
  reads the field. A whole-response decoder reads what it reads, so "a
  provider adding fields breaks nothing" holds for fields it ignores.
- **A value that will not decode is a fault** naming the operation and
  field, as a field that does not match its type is; contract runs
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

`wire_name:` on a provider package's field or enum variant is written
only where the provider's spelling differs from the Jadpo name, and the
runtime sends and reads that spelling; `failed_when:` reads the
failure's code from the provider's `error_code:`. A field or variant
gives its spelling in its own block:

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

The built-in encodings a field names with `wire:` are these ten:

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

A field's own pair of functions is for anything else: `decode:` takes the
value as the provider sends it and gives the field's type, failing when
it can't, and `encode:` takes the field's type back without failing.
Both are in the package's own `libraries/`, so they may be internal:

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

An operation's `wire_output:` is the whole answer as sent, read from
inside `output_at:`, and its `decode:` turns it into `output:`:

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

An action reading one field of an answer decodes that field alone, so
`{"id": "p-1", "status": "held", "collectAt": "soon", "slot": "NOON"}` is
read for its `status` though its time and its window would never decode:

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

A standard
type whose parts the provider sends as members of the object that holds
it, as Stripe sends `"amount": 1050, "currency": "gbp"` beside a charge's
other members, is one field whose `wire_parts:` spells the member each part
is read from and written to. The field has no member of its own on the
provider's wire, and fields may share one part's member, as a charge's
amounts share its currency:

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
"gbp", "lat": 51.5, "lng": -0.125}` is read as a `Charge` of £10.50 with
£2.50 refunded at a merchant at 51.5° N, 0.125° W, and a `Charge` is sent back
as those six members, the currency once; two amounts in different
currencies can't be sent, since they share one member, and the call stops
as one that doesn't encode. A `Point` is read from its `latitude` and
`longitude`, which GeoJSON would send as a pair; any other standard type
made of named parts from those parts (`Money`'s `amount`, in minor units,
and `currency`; `InstantRange`'s `start` and `end`). A field none of whose
members is sent is `none` where it may be, and one with only some is a
fault naming the missing member. Everywhere else, the field is written as
the contract writes its type.

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

The point of a package is expertise written
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

Here a package's action calls its own operations, one its package keeps
to itself, each answer passed to the next, and the app calls it through
its connection and maps its failure at the call. A package action's
changing calls are steps of the action that calls it (area 12, "Several
changing calls in one action"). The choices are in "Package actions and
recipes". The card can be declined after the customer was
created, so the package's surface lists `after_effect DeclinedCard` for
`start_subscription`, and the app's call of it is warned as a point that
can end `subscribe` for good once something took effect (area 12's
`step.late_failure`); here the empty customer left behind is
harmless, so the warning is declined with that reason.

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

Here a package's subscriber hears an event its webhooks bring, kept to
the package, and runs the package's action that looks the subscription
up through the connection the webhook came in on and emits the package's
own public event, which the app's subscriber keeps (choices under
"Package subscribers and recipes in `sample/`"). The event the action emits gives
no `wire_name:`: that marks one the provider sends.

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

A signed `invoice.paid` reaches `renewal` once, its lookup sent through
`payments` with that connection's credential, and the renewal it emits
reaches `renewed` through the app's outbox, as
`crates/jadpo-cli/tests/package_subscribers.rs` shows against live
PostgreSQL.

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

Here a poll of Acme Fuel's price list, whose operation takes nothing and
changes nothing, emits the package's own public `PricesPublished` made
from its answer; the connection polls it every six hours instead of once
a day, and the app keeps each change (choices under "How polls and
`max_concurrent:` run"). `fetched_at`, which the
event doesn't carry, never counts as a change.

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

The tick calls `pump_prices.latest` through `fuel` with its credential once
it is due, keeps `PricesPublished` and its deliveries only when the
prices changed, and tries a failed call again a minute later; without
the subscriber the poll never runs. Under `max_concurrent: 1` a call
waits for the connection's slot and a delivery that would call through
it waits in the outbox, as `crates/jadpo-cli/tests/package_polls.rs`
shows against live PostgreSQL.

### Services

A package's own API may be a container Jadpo runs (`runs:`), which its
actions call; it cannot call another package or connection. Services
are not built yet.

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

> `jadpo package check`'s `recipes` row compiles each recipe on its own,
> copied into the one domain of the package's `sample/` app as `jadpo
> package recipe` copies it, the package placed as the sample's own
> integration; a recipe that doesn't compile is shown at its own path,
> `recipes/<recipe>/...`, and a package with recipes and no such sample
> is `publish.sample_missing`. Running a recipe's tests in `sample/`
> waits for a recipe that has some. The choices are under "Package
> subscribers and recipes in `sample/`".

## Several changing calls in one action

These rules apply to an app's action and to the package actions it calls, which are one
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
  first run's start) for every rerun, area 6's rerun included; a resumed
  action writes timestamps from when it began, which is also when the
  provider acted. Before the first changing step a rerun takes a fresh
  time. Areas 6 and 10 hold the same.
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
  follow an effect, so the warning reaches through package actions. It
  holds for single calls too (question 26).
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
  encrypted with `application { steps: { encryption_key: } }` (area 12); nothing links them to a person, so the
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

`jadpo check` keeps these rules with the errors under area 12's "Errors"
(`step.context`, `step.large_output`, `step.store_required`,
`step.too_many`, `step.unkeyed`,
[`step.late_failure`](12-jobs-and-events.md#step-late-failure)), area
3's `effect.call_in_loop` and
[`route.calls_past_deadline`](#route-calls-past-deadline) below, and
reads an operation's `large_output:`. A provider's `idempotency:`
`window:`, `replays_failures:` and `new_key_on:` are read into the plan,
an operation's `changes:` marks a step, and the provider call records,
replays and keys each step through the capability seam, whose exchange
writes a step before it is sent. The operator's side is area 12's "What
an operator sees" and area 14's `jadpo runs unfinished --build`; the
fakes and crash points are area 13's "Provider calls and capabilities in
tests"; each step's state under its request's or delivery's line in
`jadpo dev`, and the metric and the alert on unfinished records, are
area 12's "Several changing calls in one action".

The warning counts, after an action's first changing call, each point
that can end it for good (an `attempt` passing on a failure that isn't
`unavailable` or `rate_limited`, `reject`, `propagate`) as one finding
for the action, declinable, unless handled with `match`; a package's
action lists on its surface which failures can follow its own changing
call (`after_effect`), and the app's call of it is the point. Each
route's action sums the `timeout:` times the attempts of every outside
call on its longest way, a provider's operation and an `ai` call alike,
against the route's `deadline:`, which is how question 8's single call is
checked too. The choices are in area 12's decisions.

## Receiving from outside

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

> A package declares `webhooks:` in `provider.jadpo`
> (`SignatureScheme.stripe`, `.standard_webhooks`, `.hmac_sha256` with
> `header:`, `.basic`, `.twilio`, `.slack`, `.sns`, and `handshake:`) and
> its `events/`, each with its `wire_name:` and `occurred_at: Instant`; a
> connection gives `webhooks: { secret: }`; the runtime answers the
> generated route and keeps its events in the outbox for the app's
> subscribers. The choices are under "How webhooks are received" and
> "Webhook replies, handshakes and further signature schemes" below. A
> package that receives webhooks, its events and how its provider signs
> them:

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

A connection to it with `webhooks: { secret: config.payments_webhook_secret }`
has `POST /webhooks/payments`, listed by `jadpo routes` as `generated
webhook` with `signature` for its authentication, and an app's subscriber
takes its events with `on: AcmePay.PaymentSucceeded`, as
`crates/jadpo-cli/tests/webhooks.rs` shows against live PostgreSQL. Its
body's `paymentId` and `pence` are the event's fields; its `id`, `type` and
`created` are read from the places `webhooks:` names, and every other
member is left out, as a provider's answer's are.

### Webhooks that need a reply

Some providers wait for an answer in the webhook's own response (Twilio's
call instructions, Slack's messages). The package's event type names its
reply type with `reply:`, the reply type living in the package's `types/`;
the connection's `webhooks: { replies: { } }` names the app's action that
answers, keyed by the event's name in lower_snake_case. The action takes
the event and returns the reply type, which runs inside the request after the signature
check and may read and write like any action. A repeat gets the stored
reply, and the package encodes the reply (Twilio's XML waits for an XML
byte format in a Jadpo release). The action runs as the system with no
tenant, and its reply goes back to whoever sent the webhook, so it is
held to area 9's output proof as an output to `Access.public`: what it
carries, or what decided it, is what anyone at all may read. That includes whether records are there, since the system
finds every one: a reply that counts or tests for records only the
system may read, or a failure raised because one is there or not, is
refused, and a failure, sent back the same way, is held to the proof as
the reply is.

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

> An event's `reply:` is an object of the package's own `types/`, and a
> connection's `webhooks: { replies: }` answers each event needing a
> reply with an app's action that takes the event and returns the reply,
> run as the system once the signature is checked (choices under "Webhook
> replies, handshakes and further signature schemes" below); the reply is sent as
> the package's JSON and kept with the event's id, so a repeat is answered
> with it. The event isn't kept in the outbox, so subscribing to it is
> `subscriber.event_invalid`. Acme Voice signs as Twilio does, over a form
> body, and waits for what a caller hears:

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

A ringing call is answered `200` with `{"spoken":"Thanks for calling.","hangUp":true}`,
and Twilio asking again gets the same reply without the action running
again, as `crates/jadpo-cli/tests/webhook_replies.rs` shows against live
PostgreSQL.

### Handshakes

Some providers check an address before sending to it (AWS SNS, Slack,
Microsoft Graph). These are built-in rules the package names in
`webhooks:` (`handshake: WebhookHandshake.sns`, `.slack`,
`.microsoft_graph`), answered by the runtime and extended by Jadpo
releases.

> The handshakes (choices under "Webhook replies, handshakes and further
> signature schemes" below) are SNS's subscription confirmation, signed as its messages are and
> from the connection's topic, confirmed by visiting its address on AWS's
> SNS host; Slack's challenge, answered once its signature is checked
> (`SignatureScheme.slack`); and Microsoft Graph's validation, which Graph
> doesn't sign, its token answered as plain text. SNS's own messages are
> checked against the certificate the runtime fetches from AWS's SNS host
> over HTTPS, never from a private address, and keeps. A handshake signed
> otherwise than its provider signs is `package.provider_invalid`. A
> package receiving an SNS topic's messages:

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

The connection's secret is the topic's ARN, and SNS's `Message`, which
holds JSON, is read as the JSON it holds, so `event_type:` and the
event's fields reach inside it.

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

> `receiving:` takes `type_at:`, `id_at:` and `events:`, an app event
> gives `wire_name:`, and a package provides the capability in
> `provides/receiving.jadpo`. Each message is kept once as its event and
> then acknowledged, and the ones that can't be kept are parked for
> `jadpo deliveries` (choices under "How queues are received" below). The
> SQS package (`packages/amazon_sqs`) makes its calls in AWS's JSON
> protocol, signed with Signature Version 4. The Cloudflare Queues driver
> is a connection with `driver: Driver.cloudflare_queues`, `queue:` and
> `dead_letter_queue:`, its consumer handed batches through the runtime's
> seam and written into the Wrangler file (choices under "The Cloudflare
> Queues driver").

### Senders with no package

A system that signs webhooks and has no package gets the app's own
integration in `shared/providers/`, by the golden rule. A partner with an
agreement is a `Service` with a service key calling an ordinary route
(area 8).

## Acting for each user: OAuth grants

For apps acting on another service for each user or organisation: a
user's Google Calendar, an organisation's Xero, a Slack workspace
install. Not built yet.

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

- **Jadpo's OpenAPI documents carry `x-jadpo` fields** saying what plain
  OpenAPI lacks: `x-jadpo-changes` (the operation changes something),
  `x-jadpo-idempotency`, `x-jadpo-failure` (each failure's `code` and
  `kind`), `x-jadpo-constraints` and `x-jadpo-events` (area 7). Other tools ignore them. Every
  API serves its document at `<prefix>/openapi.json` by default (area 7).
- **The OpenAPI importer uses them whenever it finds them**, so importing a
  Jadpo app's document gives an exact package needing no corrections or
  recordings; a provider that is not Jadpo is imported from its plain OpenAPI. There is
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
  breaks, before it merges. Not built yet.

## Capabilities

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
  keep area 12's parking on `outcome_unknown`. Two kinds of mail through two connections wait for an
  app that asks.
- **Image variants**, when built, are made by the runtime over any store,
  so `files` stays whole; media services (Cloudinary, Uploadcare) stay
  ordinary packages, since they cannot do the whole `files` job.

> For `email` (choices in area 16), `provides/email.jadpo` is read and checked
> against the package's operation and settings
> (`package.provider_invalid`), its `input:` keys the input's field names
> as `provide receiving`'s are, the wire names coming from the types
> rather than written as the example above writes them. `email:` may name
> the connection, which then needs no `domains:`, and each delivery calls
> the mapped operation with the rendered message, its failures mapped
> onto `EmailFailure`'s. `packages/resend` (Resend's `POST /emails`) is
> the second connection beside SMTP: the swap changes `app.jadpo` and
> its settings only.

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

> For `ai` (choices in area 21), `jadpo package conform <package> ai` runs the
> suite in an app, by default against the package's recordings and a
> local fake on this machine's loopback address, and with `--live`
> against the provider's API through the app's own connection, refused
> (`conform.refused`) without one or without its credential set. Another
> capability's suite, the run pinned to the digest for the registry and
> the scheduled CI run are not built yet.

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

> A deployable (area 14, "Declared deployables") that declares a target is refused each connection it reaches whose
> driver that target can't run, `deployable.target_unsupported`, naming
> what to use instead: on Workers the disk, a service Jadpo runs without
> `host:`, and SMTP on port 25; natively a Cloudflare queue, which a
> deployable reaches when it runs `Trigger.subscribers`, as the queues
> `receiving:` reads are. What a deployable reaches is followed from the
> routes of the APIs it serves and the background work it runs, through
> every callable they call. An app that declares no deployables, and a
> deployable with no `target:`, has no target, so nothing is refused for
> it (area 14, Decisions). An in-memory cache is checked once caches are
> built.

### Choosing a provider when deploying

Not built yet: it is built when the first app other people run needs it.
An app that other people deploy (an open-source product run by
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

> All of this is built but for `jadpo trace` and the timelines: every
> provider call `jadpo dev` makes is answered by the package's fake, with
> what `jadpo calls script` scripted, else from the operation's recording
> with its placeholders filled (logged as `recording`), else
> the operation's generated default ("Tests and fakes"), and a provider
> connection's setting that nothing gives (its credential, environment or
> address) gets a stand-in, so the app starts with no credential set.
> `development: { sandbox: }` names operations, `resource.operation`, or
> whole resources, checked against the package (`connection.value_invalid`):

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

> They go to the sandbox once its credential is set; until then the
> connection stays faked and `jadpo dev` names the variable to set. A
> connection whose environment is `live` stops `jadpo dev` starting, or a
> save reloading, with `cli.dev_start_failed`. `--sandbox
> couriers.pickups.create` (or a resource) and `--fake couriers` change one
> session. Each request's line lists its calls, and `jadpo calls` lists
> them all (area 17):
>
> ```text
> jadpo dev, started 07:34:33
> listening on http://localhost:3918
>     routes     2 routes
>     settings   2 checked
>     providers  couriers faked
> serving: saves reload, press ctrl-c to quit
> req_58d4  POST /pickups                      201   9 ms   127.0.0.1
>   └─ couriers.pickups.create · generated · answered
> req_9588  POST /pickups                      409 pickup_slot_gone   1 ms   127.0.0.1
>   └─ couriers.pickups.create · scripted · slot_taken
> ```
>
> The choices are in "Fakes and the development call log" below.

## Compile time

Provider descriptions are large, by the `Content-Length` of the raw
files:

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

> As area 13's "Fakes for integrations" shows, the test host gives one
> fake in every provider connection's place, and `fakes:` scripts each
> operation's answers in order. Grants as fixtures and the contract run
> are not built yet.

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

## Errors still to build

These mistakes are specified but not reported yet:

| Mistake | Reported by |
|---|---|
| A connection that receives personal data, with no `erasure:` and no acceptance | `jadpo check` |
| An operation whose scope the grant does not request; a call to a grant connection without `on_behalf_of:` | `jadpo check` |
| A service image not yet accepted; an `asks:` host not granted by `outbound:` | `jadpo check`, naming the line that grants it |
| A third-party package from git with no acceptance | `jadpo check` |
| A capability's connection whose package does not provide it, or a function call or expression in a `provide` mapping, for a capability other than `receiving` and `email` | `jadpo check` |

The rest of the area's mistakes have examples below: the digest and the
lock, an `emails/` folder, the index's keys, `provider.jadpo`'s keys,
`hosts:` with `runs:`, reserved and taken names, a resource and an action
of one name and a public action that emits (`package.action_invalid`)
and subscribing to a non-public event (`package.not_public`) under
"Package errors"; a surface that differs at install and a link in CI
under "Installing errors"; a decoder that fails on a recording and a
field's `decode:` and `encode:` that don't round-trip under "Recording
errors"; a connection's keys under "Connection errors". The
reachable-from-public rule is area 1's `visibility.public_reaches_private`,
and several changing calls in the wrong place are area 12's `step.`
errors and area 3's `effect.call_in_loop`.

## How the package format is read

- **A package's name is its folder's.** `packages/acme_couriers/v1/` is
  `AcmeCouriers.v1`: the folder's name in `UpperCamelCase` and its version
  folder, which the index has to give (`package.name_mismatch`). A version
  label is `v` and digits in groups joined by single underscores, `v2` or
  `v2024_06_20`; the lock sorts the groups as numbers.
- **What a package's code sees.** Only its own declarations, bare, and the
  built-ins: not the app's `shared/`, domains, connections or other
  packages, since an installed package is published apart from any app and
  the app's own integration is written so that it could be. Its files need
  no `use` lines. It doesn't read the app's `shared/` (an open question
  below); allowing it later breaks no package.
- **Bringing a package in.** `use packages/<name>` for an installed one and
  `use shared/providers/<name>` for the app's own bring the whole package
  in, as `use <domain>` does; there is no `{ ... }` form, since its names
  are always written through it. `jadpo fmt` sorts these lines after the
  domains'.
- **Names through a package.** A type (`Postcodes.UkPostcode`), a failure
  (`fails Postcodes.InvalidPostcode`), a type built from a value
  (`Postcodes.UkPostcode(text)`) and a public library's function
  (`Postcodes.uk.parse(text)`). A capability mapping other than
  `receiving`'s and `email`'s is `toolchain.not_built_yet` where it is
  declared.
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

## How the surface and the publishing bar work

- **The surface's entries.** In the record format's order: the package;
  for a provider, `provider`; each operation of a `public resource`; each
  function of a `public library`; each `public` type (`type Name =
  Base`), then each `public` failure; each folder in `recipes/`. Each
  `public` action follows the operations ("Package actions and recipes");
  its public messages, `message translations.<message>` with their
  fields, follow the functions, and the package's entry has a `locale`
  line for each language it words (area 10). Polls and capability
  mappings aren't in the surface yet. A library itself has no entry: its
  functions do. Declarations without `public`, and test files,
  are left out.
- **The package's entry.** `kind` is `provider` when the package has
  `provider.jadpo`; `licence` is the `Licence` variant's name (`mit`), as
  `protocol json_http` and `credential bearer` are written; `jadpo` is the
  index's text; `guide GUIDE.md` when the package carries one; `version`
  when it is known (below), and left out of an own integration's surface.
- **The provider's entry** holds `protocol`, `host live` and `host
  sandbox`, `runs image`, `port` and each `asks` host, `credential`, and
  `idempotency header` and `window`. Settings, webhooks, polls and
  erasure aren't in it yet.
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
- **Not built yet.** `jadpo package new` and `publish` say
  `toolchain.not_built_yet`.

## How packages are installed and linked

- **Where `add` installs from.** A folder holding `package.jadpo` is a
  `path` source, locked relative to the app's root (`../postcodes`);
  `<repository>@v<version>` is a `git` source. A repository written
  `host/owner/name` is fetched over HTTPS, a URL (`https://`, `ssh://`,
  `file://`, `git@host:`) as written, and a folder on this machine
  (starting `.` or `/`) from its path, locked relative to the root. Git
  runs with the developer's or CI's own access and never prompts. A Jadpo
  app's folder (another app's OpenAPI) and a bare name (the registry) say
  `toolchain.not_built_yet`. A
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
  failed copy leaves the old one, and its digest is worked out as "How the
  package format is read" says.
- **What `add` refuses.** A package already in the lock (`update` moves
  it); a package whose name the app's own code uses, which is installed,
  checked and undone, showing `package.name_taken`. A third party's git
  package installs without an acceptance until `jadpo approval` reports
  publisher trust (area 18), which is not built yet.
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
  `jadpo check`, `link`, `test` and `dev` show its problems at the
  checkout's paths. A checkout with no
  `package.jadpo` can't be read. `link` itself shows the app's check
  against it. `jadpo dev` and `jadpo test --watch` watch the checkout as
  they run ("A package's names, and watching a linked checkout", below).
- **Where a link is refused.** CI is the `CI` variable set to anything but
  empty, as CI services set it and as `jadpo suggest share` and the
  terminal's animation already read it; there `jadpo check` and `jadpo
  test` refuse each link (`install.link_refused`). Area 14 names no
  release or production profile, and every production build goes through
  `jadpo build`, which refuses a link whatever the environment. Not built
  yet: outside CI, `jadpo build` still builds against a link.
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

## How connections to packages are read

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
  is used; `outbound:` waits for services. Driver connections keep their
  own keys.
- **What a package needs.** `domains:` always, a list of the app's
  domains by name, each once and at least one (question 18).
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
  it. There are no defaults yet: a setting is given on every connection.
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
  `domains`. The manifest's hosts, and the approval report's changes to
  them, are not built yet (area 18).
- **How the seam finds a connection's driver.** The capability seam
  asks the host for `capability(connection)`; a host gives a driver only
  for a connection whose declared driver it carries. Natively and in
  `jadpo dev` that is the disk driver for the stores of files the running
  program's plan lists (re-read after every reload); in `jadpo test` the
  in-memory store for the same list, which the executor gives the test's
  host. A package's connection is given the package driver ("How provider
  calls are made"), or in `jadpo dev` and `jadpo test` the package's fake
  ("Fakes and the development call log").

## A package's names, and watching a linked checkout

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
  `Postcodes.v2` and `Postcodes.v3` (area 18 sorts them by name, then
  major version); a connection names one version in `package:`, so two
  versions of a provider are two connections.
- **The commands.** A package's folder or code name (`postcodes`,
  `Postcodes`) names both versions, so `jadpo package show`, `update`,
  `remove` and `unlink` refuse it and ask for the version
  (`cli.package_arguments`). `add` installs a second major version beside
  the first. `remove` refuses a version while the app's code writes a
  name through it (`install.in_use`), not while the `use` line stays for
  the other version. `update`'s review marks an entry `used by the app`
  when the code writes it through that version.
- **Translations side by side.** The app words a library package's
  messages under its folder's name (`postcodes: { }`, area 10), which two
  versions would share, so while two are held the version joins the key
  (`postcodes_v3: { }`), as it joins the names; with one held, the key
  stays the folder's name.
- **Lazy loading.** A package's index and `provider.jadpo` are always
  read; an installed package's `resources/<resource>.jadpo` is compiled
  only once code outside every package writes `<connection>.<resource>`
  for a connection to it (docs/compiler-runtime.md §2.3). Its bytes still
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
- **Connection names (question 17).** A connection, to a package or a
  driver, named like one of the app's domains or libraries is
  `semantic.duplicate_declaration` at the connection in `app.jadpo`,
  under "Connecting in `app.jadpo`". A package's library isn't one of the
  app's, so it never clashes.

## How the OpenAPI importer reads a document

The importer, `jadpo package import openapi <file>`:

- **Where it writes.** Always the app's own integration,
  `shared/providers/<name>/<version>/`; installing another Jadpo app's
  package into `packages/` is `jadpo package add`. The name
  and version are `--as AcmeCouriers.v1`, or the document's title in
  `UpperCamelCase` and its version's major number (`1.4.0` is `v1`), or
  its date (`2024-06-20` is `v2024_06_20`), else `v1`, as for a Jadpo
  application, whose document's version is a hash of its record.
- **What it reads.** OpenAPI 3.0 and 3.1 as JSON, from a local file; YAML
  and Swagger 2.0 are refused with what to do (`cli.package_unreadable` and `cli.package_not_openapi`,
  area 14), and Smithy, Postman, HAR and `.d.ts` are
  `toolchain.not_built_yet`. Nothing is fetched.
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
- **Types.** Area 11's exact mappings (`date-time` `Instant`, `date`
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
  `wire_name:` ("Decoding"). Most APIs other than Stripe's are camelCase,
  and a provider-wide setting would hide the exceptions.

## How provider calls are made

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
  member of its spelling, as text, escaped; the rest is a `GET`'s or
  `DELETE`'s query (a list as the key repeated, `none` left out, an
  object as its JSON) and every other method's JSON body.
- **Who calls.** Only an action, of a domain the connection's `domains:`
  lists (`connection.domain_not_listed` otherwise); never a function, and
  never a test, which reaches a provider through an action with the
  provider faked. A connection's protocol other than `Protocol.json_http`
  and `.aws_json`, and a credential scheme other than
  `CredentialScheme.bearer`, `.basic`, `.header` (area 21) and
  `.aws_sigv4`, is `toolchain.not_built_yet` at the call. A `basic` credential is the setting's `user:password`.
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
- **The key** (area 12, "Keys") is `jd1_` and 32 characters of RFC 4648
  base32 in lower case, the first 160 bits of a SHA-256 over the parts,
  each given by its length so two sets of parts never run together, and
  cut to `max_length:`. The application is its name (its folder's until
  `app.jadpo` names it); the environment the host its environment setting
  chose, `live` or `sandbox`, or the address a `host:` setting gives. In
  a route, the caller's idempotency scope and the client's
  `Idempotency-Key`, whether or not a store keeps it, or `request` and the
  request's id; in a subscriber, `delivery` and the delivery's id, but a
  sign-in email's attempt a key of its own (area 16, "Built-in emails");
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
  A host holding a setting's placeholder (`{project_ref}`) isn't filled
  yet. The call waits `timeout:` (3 seconds by default), a duration
  written out (`connection.value_invalid` otherwise). It sends `Accept`
  and `Content-Type: application/json`, `User-Agent: jadpo`, the
  credential as `Authorization`, and the key in the package's header.
- **The package driver** is one driver for every package connection,
  `jadpo_drivers::package::JsonHttp`, over the `Fetch` its host gives:
  natively `jadpo_host_native::fetch::NativeFetch`, hyper's HTTP/1.1
  client on the worker's thread with its connections kept per host,
  rustls checking the certificate and the host against Mozilla's roots
  for HTTPS. It classifies what came back: a `2xx` as sent, unless
  `failed_when:` marks it, then the code at `error_code:`; a redirect,
  `401` or `403` `misconfigured`; `408`, `429` or the package's `in_use:`
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

## Wire encodings, decoders and retries

- **The built-in encodings** are ten, by these names: `unix_seconds` and
  `unix_milliseconds` (an `Instant` as a whole number), `iso_8601` (the
  page's "ISO dates": an `Instant` with any offset from UTC, which the
  contract refuses, since it writes only `Z`), `day_month_year` (a
  `CalendarDate`, the encoding areas 12 and 19 name for file rows, here
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
  package function where the call is made (`provider::Functions`); a
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
  `timeout:` apart from other missing answers (`Answer::TimedOut`). Where
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

- **One fake per test, in every provider connection's place.** The test
  host gives a package's fake (`jadpo_runtime::fakes::ProviderFake`) for
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
  the operation, at its first scripted answer. `.requests` is not built
  yet.
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
  it ended with a failure's code. It keeps the last 500 calls rather than
  timelines, which aren't built. A call `retry:` sends again is one
  entry, its last attempt's, where the invocation's own log
  keeps each attempt. Only provider calls are logged, a host's other capabilities'
  calls (files) are not, and a delivery's or a schedule's call has no
  request. Each request's line in `jadpo dev` lists its calls, and its
  `--json` event gives them.

## Shape and name detection

Layers 2, 3 and 6 of "Mapping imported types to standard types", for
fields; layers 4 and 5 are under "The scoped review and unconfirmed
operations".

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
  is a test of a fifth kind, `recording` (area 13), named
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
  keyed operation takes a key derived as any call's (area 12, "Keys"),
  from a scope of its own and this run's randomness, so recording again
  is a new request. A refusal, no answer, or an answer that doesn't
  decode keeps nothing (`recording.failed`); recording the same answer
  again leaves the file as it was. An operation with no `output:` is
  `toolchain.not_built_yet`: writing the type from the answer is not
  built yet.

## The scoped review and unconfirmed operations

Layers 4 and 5 of "Mapping imported types to standard types":

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

Webhooks as events; replies, handshakes and the further schemes are
under "Webhook replies, handshakes and further signature schemes", and
queues under "How queues are received".

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
  again once the package is fixed. A field read by the package's own
  `decode:` and `encode:` is `toolchain.not_built_yet` on a webhook's
  event, since those run at a call's site.
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
  whose payload is over 64 KB, every event's limit (area 12), is `413
  request_too_large`. It is counted
  against its address's rate limit, whatever credential it carries. A
  non-public package event is kept and reaches no subscriber, subscribing
  to one being `package.not_public`; the package's own subscribers hear
  it ("Package subscribers and recipes in `sample/`").

## The Cloudflare Queues driver

The Cloudflare Queues driver on Workers:

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
  runtime's seam (`receiving::consume`): the queue's name and each
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
  retries, Cloudflare's most, and no dead-letter queue of its own
  (`jadpo_std::receiving::wrangler_queues`, as JSON). The setting names
  are Wrangler's own: `jadpo build` writes them into the
  Wrangler file, and Wrangler's `deploy --dry-run`, pinned in
  `tools/workers/`, accepts it without a warning.
- **Natively.** A server off Workers can't receive the binding: it reads
  no message from the queue, only the parked ones an operator retried.
  A native deployable that runs `Trigger.subscribers`, where the app's
  queues are received, is refused the driver ("Targets"); a native server reads a Cloudflare queue
  through a package over its HTTP pull API instead.

## How queues are received

Queues through `receiving`; the Cloudflare Queues driver is under "The
Cloudflare Queues driver".

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
  as a statement, as it would every time, is parked `unreadable` (area
  12, "Queue messages the store refuses").
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
  in the mapping's file. Mappings for capabilities other than `receiving`
  and `email` are `toolchain.not_built_yet`. An installed package's resources a mapping
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
- **The SQS package**, `AmazonSqs.v2012_11_05`, is in this repository at
  `packages/amazon_sqs` and is installed with `jadpo package add`; it maps
  `ReceiveMessage` and `DeleteMessage`, marking `ReceiveMessage` safe to
  send again, since repeating it only hides other messages for a while.
  Its tests run against a local fake of SQS's JSON API that checks each
  signature (`crates/jadpo-cli/tests/receiving.rs`).

## Webhook replies, handshakes and further signature schemes

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
  nosniff` (`400 invalid_request` for one that isn't). Graph's own
  notification check, its `clientState` in the body, isn't a scheme yet.
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
  the claim lapses runs the action again. Twilio's TwiML waits for an XML
  byte format.
- **What a reply may show.** What only `Access.public` lets the reply
  action in to is `policy.system_grant_missing` at its `replies:` entry.
  The reply goes back to whoever sent the webhook, so each field it
  carries, or that decided it, is one anyone at all may read
  (`policy.output_field_unproved`), a record the action created holding
  what it set. Whether records are there reaches the sender too: a reply
  counting or testing for records only the system may read, or a failure
  decided by it, is refused the same way, and the action's failure,
  answered to the sender as its reply is, is held to the same proof, what
  decides it and its client fields (area 9).

## Package actions and recipes

- **Calling its own API.** In a package's action, `<resource>.<operation>(input)`
  calls one of its package's operations, public or not, by its bare names;
  a function or a test of the package calling one is
  `effect.function_persistence`. The operation's failures are the
  package's own, so they pass on to the action's `fails` with `attempt`
  as a library function's do, or are handled with `match`. At run time
  the call goes through the connection the app called the action
  through, with its credential, host, timeout and fake.
- **No other action.** A package's action calling another action, its
  package's or any other, is `package.action_invalid`: the pages let a
  package action call its operations and its pure functions, and keeping
  it to one body keeps each way through it one sequence of steps. This
  can be loosened later without breaking a package.
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
  process's waiters are woken by the call that gives a slot back
  (compiler-runtime §2.6, "Six connections an invocation").
- **The surface.** A poll's event is one of the package's public types
  and in its surface as any is; the polls themselves and
  `max_concurrent:` join the provider's entry once the surface tells an
  addition inside an entry from a breaking change, since any change to
  the provider's entry counts as breaking.

## Enum values and failures through a package

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

Built: the package format, names and the lock; the surface, `jadpo
package show` and `jadpo package check` with the publishing bar;
installing (`add` from a folder or git, `update`, `remove`, `link` and
`unlink`); connections to packages and the network report; lazy loading,
the package cache and two versions side by side; the OpenAPI importer;
provider calls, decoding, `wire_parts:` and retries; fakes, recordings
and provider calls faked in development; the scoped review of an
import's types; the `email` capability with SMTP and Resend; no
transaction open during a call; package actions, subscribers, recipes and
polls; several changing calls in one action; webhooks as events, replies,
handshakes, queues through a package and the Cloudflare Queues driver.

Not built yet: services (`runs:`); OAuth grants; publisher trust for
third-party git packages; the hosted registry; importers other than
OpenAPI; choosing a provider when deploying; erasure at providers, which
comes with personal data (area 15). [The roadmap](../roadmap.md) says
when.

Each section below settles what the topic sections leave open, taking
the most conservative reading.

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
> so area 14 lists them with its command errors.

- [`install.surface_mismatch`](../errors/install.surface_mismatch.md)
- [`install.fetch_failed`](../errors/install.fetch_failed.md)
- [`install.in_use`](../errors/install.in_use.md)
- [`install.link_refused`](../errors/install.link_refused.md)


### Recording errors

> `jadpo test` reads every recording of the app's
> own integrations as its operation's output, its placeholders filled,
> through the package's own decoders, each as a test of the `recording`
> kind (area 13), and the publishing bar does the same as its
> `decoders` row. `jadpo package record` reports `recording.refused` and
> `recording.failed`, which area 14 lists with its command errors.

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
