# Packages and extension points

Status: working document, 2026-10-16, for [area 11](../11-integrations.md),
with changes to areas 1, 2, 8, 14 and 16. Richard and Claude are working
through it one part at a time. Each part says whether it is **decided**
(Richard agreed it) or **proposed** (not yet discussed). When every part is
decided, the decisions move to the area pages and this page becomes the
research behind them.

Richard's question (2026-10-16): before chunks build anything an app might
want to swap, extend or get from a package (email in 7.3 is the example),
how are packages installed, where do they sit, how do they reach the
program, and how is a feature turned on, off or replaced?

## At a glance

| Part | State |
|---|---|
| Swapping a provider (the email case) | Decided before, in area 11 (2026-10-13) |
| Which kinds of package exist | Decided |
| Built in or a library package | Agreed as input to chunk 5.4 |
| Layout inside a package | Decided |
| Where packages live, and where your own code lives | Decided |
| Names | Decided |
| The package surface | Decided |
| What publishing requires | Decided |
| No code with side effects in packages | Decided |
| Decoding responses and requests | Decided |
| Dependencies between packages | Next |
| Private packages | Proposed |
| The model and its rules | Proposed |
| Capability mappings calling pure functions | Proposed |
| Choosing a provider when deploying | Proposed |
| Sign-in providers as a capability | Proposed |
| Other candidate capabilities | Proposed |
| What a library package may also carry | Proposed |
| Software and disk | Proposed |
| Services: containers run by Jadpo | Proposed, its own topic |

## Decided before this: swapping a provider

Area 11's provider packages (questions 1–18) and capabilities (questions
19–25), decided 2026-10-13, already answer the email case. Email is a
**capability**: a job several providers do, with one API in application
code. SMTP is one built-in driver for it; a mail package such as Postmark
is another, providing the capability through a declarative `provide email
{ }` mapping. `app.jadpo` names which one is used:

```text
// app.jadpo
application {
    connections: {
        main:     { driver: Driver.postgres  url: config.database_url }
        postmark: { package: Postmark.v1  credential: config.postmark_token
                    message_stream: "outbound" }
    }
    email: { connection: postmark  from: config.mail_sender }
}
```

There is nothing to disable: SMTP does nothing unless a connection names
it, and changing provider changes `app.jadpo`, never domain code. The
capabilities are a fixed list owned by the language (database, cache,
files, email, SMS, publishing, schedules), each with standard types and
events, a local driver for development, one fake for tests and a
conformance suite. Packages fill capabilities and never invent them.

## Decided: which kinds of package exist

| Kind | Contains | Used as |
|---|---|---|
| Provider | One API at one version: resources, operations, types, failures, events, hosts; optionally `provide` mappings and library functions | A connection: `stripe.payment_intents.create(...)` (area 11) |
| Library | Types, failures and library functions under `shared/`'s rules: pure, no entities, no routes, no network | `use Postcodes`, then `Postcodes.uk.parse(text)`, `Postcodes.UkPostcode` |

- **No feature packages.** A whole slice of an app (invitations, billing)
  is not a package, installed or copied in. An agent writes it from the
  language pages, the compiler catches the dangerous mistakes, and most
  features other frameworks take from packages (sign-in, flags, audit,
  files, email) are built in. Where agents keep getting one wrong (chunk
  10.2 measures this), the fix is a worked reference in `examples/`.
- **No driver packages.** Anything that needs native code (a new
  database, a signing scheme, a wire format) comes only through a Jadpo
  release. Packages never carry native or WebAssembly code.
- **Library types may be stored** in entity fields and used in route
  inputs and outputs, unlike provider types. An upgrade that changes a
  stored type appears in the upgrade plan and the migration, as a change
  to the app's own type does.
- **One package format; the kind follows from what is inside.** A package
  with `resources/` is a provider package and needs a connection to call
  its operations; one without is a library package. A provider package's
  library functions are callable without a connection.
- **Everything in a provider package follows the provider rules**, its
  library types included: only ids and plain values may be stored,
  because a provider package is versioned by the API's version and the
  next version is a new package. Types worth storing go in a library
  package that the provider package requires (a `Postcodes` library
  required by a `Getaddress` lookup provider).

### Names

- **Provider brand names are reserved**: Jadpo ships the list of curated
  provider names, and an app's own integration cannot take one, so `Stripe` in
  code always means the official package.
- **Two packages never share a name in code.** Installing a registry
  package whose name one of the app's own integrations or libraries
  already uses stops and asks for the app's own to be renamed first
  (`jadpo rename`).
- Creating an integration checks the registry and warns when the name is
  taken.

## Agreed: built in or a library package

Input to chunk 5.4 (the standard-library plan), which decides it. A thing
is built in when the compiler gains from knowing it: a type that turns a
mistake into a compile error, carries personal-data marking, has a SQL or
OpenAPI form or generated test values, or a security boundary the language
should own. Saving an install is not enough.

| Thing | Where | Why |
|---|---|---|
| Money and currency | Built in | Mixing currencies is a compile error; rounding fixed; one column form |
| Countries, currencies, languages | Built in | Small, slow to change, needed by money and phone numbers |
| Phone numbers | Built in | Already carried to area 2 for SMS; always personal data |
| HTML cleaning and Markdown, as a safe-HTML type | Built in | A security boundary; the parser Jadpo is worst at writing |
| CSV | Built in | Already in 5.4's encodings |
| Postcodes, IBAN, VAT numbers, ISBN | Package | A long tail of per-country rules |
| Public holidays, business days | Package | Data that changes yearly, per country |
| Repeating dates, `.ics`, QR codes | Package | Niche; can move into the standard library later |

Built-in data ships with Jadpo releases, as time-zone data does in every
language. Public library packages are then the long tail, and the largest
use is likely private: types and rules shared between one company's own
apps. Open for 5.4: where fixed reference data (a country table, phone
formats) can be declared at all, since the language has no way to declare
a table of values today.

## Decided: the layout inside a package

A package is laid out as `shared/` is, so one set of rules covers the app
and every package, and the compiler checks a package as it checks
`shared/`. A library package:

```text
postcodes/v2/
    package.jadpo                   the index: name, version, description, licence, requirements
    types/uk_postcode.jadpo         public type UkPostcode = Text { ... }
    failures/invalid_postcode.jadpo
    libraries/uk.jadpo              library uk { public parse(...)  normalise(...) }
    libraries/uk.test.jadpo         tests beside the file, as in an app
    CHANGELOG.md
    surface.json                    generated
```

A provider package adds `resources/`, `events/` and `recordings/`:

```text
stripe/v2024_06_20/
    package.jadpo                          hosts, credential scheme, idempotency, error location
    resources/payment_intents.jadpo        imported, corrections merged in
    types/payment_intent.jadpo             imported
    failures/card_declined.jadpo
    events/payment_intent_succeeded.jadpo
    libraries/tables.jadpo                 written: decoders and translation helpers
    libraries/tables.test.jadpo
    recordings/payment_intents.create.json real responses that confirm the types
    surface.json
```

- **`public` marks the package's API**, as it marks what a domain offers
  other domains. Everything else is internal and may change freely.
- **The index lists no files**: a file is named after what it declares,
  so the compiler finds a resource by name and loads only what the app
  uses.
- **A re-import rewrites only imported files.** Written libraries and
  corrections survive, as area 11 already says for corrections.
- **A package's tests run when it is published**, not in every app's
  `jadpo test`; the digest guarantees the code is the code that passed.

## Decided: where packages live, and where your own code lives

One rule, by who wrote the code rather than what kind it is:
**`packages/` holds only what was installed. Everything the app's authors
wrote lives where their code already lives, whatever kind it is.**

```text
app.jadpo
domains/
    shipping/libraries/...           the app's helpers for one domain (as now)
shared/
    libraries/...                    the app's helpers for several domains (as now)
    providers/
        acme_couriers/v1/            the app's own integration, in the package format
packages/                            installed only: in jadpo.lock, read-only
    stripe/v2024_06_20/
    postcodes/v2/
jadpo.lock
```

- **Installed packages are committed** under `packages/<name>/<version>/`.
  This changes where area 11 put registry packages (a cache on the
  developer's machine); the pin by digest and "never fetched during a
  build" stand. Agents read them with the tools they already use,
  reviewers see an upgrade as an ordinary diff, and CI needs no fetch
  step.
- **Installed packages are read-only.** The compiler refuses one whose
  files do not match its digest in `jadpo.lock`.
- **The app's own integrations live in `shared/providers/<name>/<version>/`**,
  in exactly the package format, with everything an installed provider can
  have: types, failures, pure functions, decoding and encoding, recordings,
  fakes and `provide` mappings. Who may call one is still the connection's
  `domains:` list. This adds `providers/` to what area 1 allows in
  `shared/` (today pure types, failures and libraries only).
- **A `use` line shows where a name comes from**, as area 1 already
  requires: `use packages/…` is always installed code; anything else is
  the app's own.
- **Installed or the app's own, the only differences are where it sits,
  whether it can be edited, and whether the publishing bar applies.** The
  compiler treats both the same, so replacing an own integration with an
  official package later changes `jadpo.lock`, the folder and the `use`
  lines, not the code that calls it.
- **Publishing moves code out of the app.** A package other apps depend on
  gets its own repository and versions, and every app installs it into
  `packages/`, the one it started in included.
- Two versions can sit side by side: a provider during an upgrade (area
  11 question 3), a candidate version while the upgrade plan compiles the
  app against it, or two library versions while callers move across.
  While two are in use, names carry the version
  (`Postcodes.v3.UkPostcode`), as area 11 question 16 decided for
  providers.
- The commands are `jadpo package …` (`add`, `import`, `record`,
  `conform`, `update`, `remove`, `show`, `publish`), replacing area 11's
  `jadpo provider …`.

## Decided: the package surface

Richard's idea. The compiler works out a package's public surface and
writes it to `surface.json`: every public type with its fields and
constraints, every failure with its kind and code, every function's
signature, failures and description, the requirements and licence, and for
a provider its operations (wire and output types both), hosts, credential
scheme, settings and the capabilities it provides.

- **One source for documentation.** The registry renders every package's
  pages from its surface, so every package is documented the same way,
  and an agent reads one file instead of the package's code (also as
  `jadpo package show <name> --json`).
- **Generated, never written.** `jadpo package publish` writes it, and the
  registry refuses a surface that does not match the code. The surface
  of an app's own integration is generated into `build/`.
- **It decides the version.** Publishing compares the new surface with the
  last release's and refuses a breaking change (a public name removed, a
  field retyped, a new failure) without a new major version, as Elm's
  package manager does. The upgrade plan diffs the same two files.

## Decided: what publishing requires

`jadpo package publish` refuses a package unless:

1. it compiles with no errors and no warnings;
2. every public type, failure and function has a `///` description;
3. every public function has at least one example in its description,
   compiled and run at publish, and every failure it can raise is shown by
   an example or a test;
4. its tests pass, and every decoder decodes every recorded response;
5. its version agrees with the surface diff;
6. its changelog's list of changes is generated from the surface diff, and
   the author writes only why;
7. it declares a licence from a fixed list and the lowest Jadpo version it
   needs.

The app's own integrations and libraries skip the bar; it applies only to
what is published. Descriptions alone
are not the bar, because the compiler cannot judge whether one says
anything; examples are, because they must run.

## Decided: no code with side effects in packages

A package holds no actions, subscribers or calls. An operation is a
description the runtime performs, and only the app's actions call one.
That keeps area 11's rules whole: one changing call per action, the call
before the writes, the provider's failures mapped onto the app's at the
call, and every outside call visible where it happens. Multi-step flows
are events and subscribers, which are the app's.

A provider package may hold pure functions in its `libraries/`, for
decoding (below) and for helpers that belong to that API version and
change with it.

## Decided: decoding and translation

Two kinds of conversion, split by one test: is it needed to make the value
mean what the provider's documentation says it is?

- **Decoding** (yes): dates as Unix seconds, numbers as text, lists as
  `"a,b,c"`, an envelope around the real object, an error inside a `200`,
  a columnar table that is really a list of rows, Stripe's
  `amount: 1250, currency: "gbp"` that is really money. **The package
  declares it and the runtime does it**, so the app only ever sees
  correctly typed values.
- **Translation** (no): turning a correct value into the shape one app
  prefers. **The app's action does it, with an explicit call.**

Decoding is declared on fields and on operations (syntax illustrative):

```text
type Booking = Object {
    collect_at: Instant {
        wire: Wire.unix_seconds
    }
    weight: Decimal {
        scale: 2
        wire: Wire.text
    }
    slot: SlotWindow {
        decode: slots.parse_window
        encode: slots.format_window
    }
}

list_rows: {
    method: HttpMethod.get
    path: "/v1/reports/{id}/rows"
    wire_output: ColumnTable
    output: List<ReportRow>
    decode: tables.to_rows
    output_at: "data"
    failed_when: { path: "ok"  equals: false }
}
```

- **Built-in encodings** for the common cases, a fixed list extended by
  Jadpo releases: Unix seconds and milliseconds, ISO dates, numbers as
  text, joined lists, base64, JSON inside a string, money in minor units,
  envelopes, errors inside a `200`.
- **Pure decode and encode functions** from the package's own
  `libraries/` for anything else, on a field or a whole request or
  response. A field's pair is checked to round-trip at publish; a
  whole-response decoder is run over every recorded response.
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
- **The importer cannot see most of this** (OpenAPI often says only
  `integer` for Unix seconds), so the agent adds it with the other
  corrections and `jadpo package record` confirms it against a real
  response.
- The compiler cannot police the line between decoding and translation; the
  surface shows both the wire and the output type, so the published
  documentation and the review make it visible.

## Next: dependencies between packages

Proposed: a package lists its requirements in its index; one version of
each is in use, two only while callers move across; `jadpo package add`
writes every requirement into `jadpo.lock`, so nothing arrives that the
lock does not show. To discuss: whether a library may require a provider
package (helpers on top of someone else's Stripe), how deep requirements
may go, and how an upgrade of a shared requirement is resolved.

## Proposed: private packages

A company's own apps sharing types and rules is likely the largest use of
library packages. Decided so far: such a package lives in its own
repository and every app installs it, the one it started in included. Open:
where apps install it from (a private registry, or a git repository the
company controls) and whether the publishing bar applies in full.

## Proposed: the model and its rules

Four layers, each extended in exactly one way:

| Layer | Holds | Extended by |
|---|---|---|
| Runtime | Drivers, byte formats, built-in encodings, credential and signature schemes, schedule ticks | A Jadpo release |
| Capabilities | The swap points | A language change through review |
| Packages | Provider and library packages: Jadpo source | `jadpo package add`, or the app's own in `shared/providers/` |
| Wiring | `connections:` and capability settings in `app.jadpo` | The app |

The rules every chunk would follow:

1. **Nothing is ambient.** Every backend is a named connection, and each
   feature names the connection it uses. Turning a feature off is removing
   its wiring; replacing a provider is changing a name.
2. **Native code only comes from a Jadpo release**, and a package reaches
   the network only through connections the app declares.
3. **A swap point is a capability**; packages fill capabilities and never
   invent them.
4. **Extend by events, override by name.** Built-in features offer events
   and settings; an app overrides a template or translation by writing
   its own file under the same name.
5. **Everything a package brings is reported**: manifest, network,
   personal-data and approval reports.
6. **Development never leaves the machine.**

A new feature that needs a backend asks, in order: is it state the app
keeps (it names a store), a job several providers do that the compiler
gains from knowing (a capability), or one provider's API (a provider
package)?

## Proposed: capability mappings calling pure functions

Area 11 question 21 limits a `provide` mapping to field paths, literals,
lists, nested blocks and enum tables. Decoding already lets a package's
pure functions run on the response path; the same would let a mapping call
them, added with the first package that needs one.

## Proposed: choosing a provider when deploying

`email: { connection: postmark }` is fixed at build time. An app that
other people deploy (an open-source product run by many operators) would
instead declare every connection it supports and let a setting choose one
at start (`connection: config.mail_provider`), so every candidate is still
compiled, checked and reported, and an unchosen one needs no credential.
Depends on whether Jadpo apps will be run by people who did not write
them. Sending different messages through different connections, and
falling back to a second provider, come later and only when an app asks.

## Proposed: sign-in providers as a capability

Area 8 decided built-in presets (Google, GitHub) plus generic OpenID
Connect, not built yet. As a `sign_in` capability, generic OpenID Connect
and OAuth 2.0 become built-in drivers and each preset becomes a package
that sets the issuer, scopes and where the email and its verified flag
come from, so adding "Sign in with Apple" is a package, not a compiler
release.

## Proposed: other candidate capabilities

Search (an external index such as Meilisearch or Typesense behind the
decided `search:` section), push notifications and bot protection, each
added through review only when an app needs it. Payments and AI models stay
provider packages: one action per job is the app's swap point for those.

## Proposed: what a library package may also carry

- **Email layouts and partials**, so a company's apps share one branded
  layout. Partials are typed and read-only (area 16), so they are pure.
- **Translations** for its own failure messages and partials.
- **A short usage guide** for agents: the surface says what a package
  offers, the guide how to use it well (handling a refund with Stripe).

Not packages: starter templates (`jadpo new --template`), which belong
with the references in `examples/` if ever wanted; company-wide rules
("no app sends personal data to this provider"), which are review tooling
for areas 17 and 18.

## Proposed: software and disk

Jadpo code never runs a program or touches the disk: Workers has neither,
and a package that could run programs is npm's install-script problem
again. Software such as LibreOffice, headless Chrome, ImageMagick, ffmpeg,
ClamAV or Tesseract reaches an app in one of two ways:

- **As a service with a provider package.** Most are already packaged as
  small HTTP services (Gotenberg for PDFs, imgproxy for images, REST
  wrappers for ClamAV). The registry carries curated packages for the
  common ones, and the connection gives the service's address, as for any
  self-hosted product (area 11).
- **As a built-in driver behind a capability**, when the compiler gains
  from knowing it: image variants on a `File` field
  (`variants: { thumb: 200x200 }`, as Rails' Active Storage has), virus
  scanning of every upload before it is attached, and a `documents`
  capability that renders a typed template to PDF exactly as email does.
  Each is built when an app needs it.

Disk is the `files` capability. A disk driver for production on the
native target, refused on Workers by the target rule, would let a
single-server app keep files on its own disk.

## Proposed: services, containers run by Jadpo

Richard, 2026-10-16: commissioning a server that needs custom software
means maintaining Docker Compose files and deploying those boxes
separately. If a package can declare the container it needs, everything
the app depends on is described in the app.

```text
package Gotenberg.v8 {
    service: { image: "gotenberg/gotenberg:8"  port: 3000 }
    ...
}
```

The image is pinned by digest in `jadpo.lock` and listed in the approval
report.

**Only stateless services** (Richard, 2026-10-16, proposed): a service
holds no data and has no volume, and each app instance runs its own copy,
so many servers need nothing shared. Anything that holds data (PostgreSQL,
Redis) is infrastructure the operator provides and the app reaches through
a connection; `jadpo dev` runs those locally as it already does, and
nowhere else.

**Security is the engine's, with three settings Jadpo always applies** when
it creates a container: no outbound network unless the package declares
the hosts it needs (the app's own deny-by-default rule, area 11 question 9,
which Docker's default network does not give: a PDF service rendering an
invoice could otherwise send it anywhere), never privileged, and no access
to the host's disk. Jadpo creates the containers itself, so there is no
configuration to audit.

Proposed stages, each useful alone:

1. **Development.** `jadpo dev` starts the services the app's connections
   declare, as it runs the database; tests keep using fakes.
2. **Deployment config.** `jadpo build` writes the configuration that runs
   each service beside the app (Compose first).
3. **Supervision on the native target.** The deployable starts, checks,
   restarts and upgrades its services on its own server through the
   server's container runtime, and their logs and health join the app's
   telemetry. Kamal's "accessories" and .NET Aspire's app host do parts of
   this.

**How containers run** (proposed): Jadpo is a client, never a container
runtime. It talks to whatever engine the machine has through the
Docker-compatible API, which Docker and Podman both serve (Podman without
a daemon or root), and leaves pulling, isolation and restart policies to
it. Jadpo's own part is the declaration, the digest pin, locked-down
defaults, start order, health and telemetry. On a Mac, containers need a
Linux virtual machine (Docker Desktop, OrbStack, Podman), so `jadpo dev`
says plainly when none is installed.

On Workers, which runs no containers, a service is hosted elsewhere and
the connection gives its address.

## Every extension point

| Thing | Way | State |
|---|---|---|
| Database, cache, schedules | Capability, built-in drivers only | Decided (area 11) |
| Files, email, SMS, publishing | Capability, drivers and packages | Decided (area 11) |
| Sign-in providers | Built-in presets today; a capability if agreed | Proposed |
| Search, push, bot protection | Candidate capabilities | Proposed |
| Rate limits, flags, sessions, idempotency keys, outbox, audit | Name a store; not capabilities | Decided (areas 5, 12, 16) |
| Event delivery | Runtime only; external queues are publishing targets | Decided (area 12) |
| Telemetry | OpenTelemetry settings | Decided (area 16) |
| Secrets | Environment variables through `config.jadpo` | Decided (area 10) |
| Payments, AI models, any other API | Provider package | Decided (area 11) |
| Money, countries, phone numbers, safe HTML, CSV | Built in | Agreed, for 5.4 |
| Shared types and rules | Library package | Decided |
| Whole features | The agent writes them; references in `examples/` | Decided |

## What the coming chunks follow

| Chunk | Follows |
|---|---|
| 5.4 Standard library: the plan | The built-in-or-package table and its rule; where reference data is declared |
| 7.1 Packages and connections | `packages/<name>/<version>/` for installed packages only, committed and checked against `jadpo.lock`; the app's own integrations in `shared/providers/`; reserved brand names; `jadpo package` commands; both kinds laid out as `shared/`; `public` as the API; `use` for library packages; the surface and the publishing bar |
| 7.2 Importing and faking | Imports write into `shared/providers/`; decoding (`wire:`, `decode:`, `wire_output:`) with built-in encodings; recordings checked by decoders; fakes scripting the clean type |
| 7.3 Email | The `email` capability with SMTP as one driver and a mail package as the second, proving the swap |
