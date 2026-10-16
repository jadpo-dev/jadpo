# Packages and extension points

Status: proposal, 2026-10-16, for [area 11](../11-integrations.md), with
changes to areas 1, 8, 14 and 16. Nothing here is decided until Richard
answers the questions at the end; decisions then move to the area pages.

Richard asked (2026-10-16) for one design that every later chunk follows
when it builds something an app might want to swap, extend or add from a
package: how packages are installed, where they sit, how they reach the
program, and how a feature is turned on, off or replaced. Email is the
example: chunk 7.3 builds SMTP, but an app may want to send through a
provider's HTTP API instead.

## What is already decided

Most of the pattern exists, in area 11's provider packages (questions 1–18)
and capabilities (questions 19–25), all decided 2026-10-13. The email case
is answered there:

1. `jadpo provider add Postmark.v1` copies the package into the local
   cache and pins its digest in `jadpo.lock`.
2. `app.jadpo` declares a connection to it, beside the database.
3. `email: { connection: postmark }` in `application { }` sends every
   `EmailMessage` through it. Replacing `postmark` with an SMTP connection
   is the only change needed to go back; no domain code changes.

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

There is nothing to disable. SMTP is a built-in driver that does nothing
unless a connection names it; chunk 7.3 builds the `email` capability with
SMTP as its first driver, and the Postmark package plugs into the same
capability. What makes that work:

- **Capabilities** are a fixed, language-owned list of jobs (database,
  cache, files, email, SMS, publishing, schedules). Each has one API in
  application code, standard types and events, a local driver for
  development, one fake for tests and a conformance suite.
- **A package provides a capability** with a `provide email { }` file that
  maps the standard operations onto its own calls, declaratively.
- **Provider features** come through typed connection settings, standard
  events back (`EmailBounced`), or the package's own operations, called
  directly from the domains the connection lists.
- **Development and tests** swap every driver for a local one (inbox, disk,
  in-memory cache); the local drivers do not exist in production builds.

What is not designed is everything outside "a package describes another
company's API": packages of Jadpo code, how packages depend on each other,
how far a capability mapping can go, choosing a provider at deploy time,
and which other features should become capabilities. That is this page.

## The model

Four layers, each extended in exactly one way.

| Layer | Holds | Extended by | Who can extend it |
|---|---|---|---|
| Runtime | Drivers (PostgreSQL, Redis, SMTP, S3, OIDC), wire formats, credential and signature schemes, schedule ticks | A Jadpo release | Jadpo only |
| Capabilities | The swap points: one API, types, events, fake, conformance suite each | A language change through review | Jadpo only |
| Packages | Jadpo source: API descriptions, capability mappings, types, failures, library functions | `jadpo package add` or a local folder | Anyone |
| Wiring | `connections:` and the capability settings in `app.jadpo` | Editing `app.jadpo` | The app |

### The rules every chunk follows

1. **Nothing is ambient.** Every backend a feature uses is a named
   connection in `app.jadpo`, and the feature names the connection it uses
   (`store: main`, `email: { connection: postmark }`). Turning a feature
   off is removing its wiring; replacing a provider is changing a name.
   There are no enable or disable switches, and no defaults that pick a
   backend silently. This is area 16's rule for flags and rate limits,
   applied everywhere.
2. **Native code only comes from a Jadpo release.** A package is Jadpo
   source, checked by the compiler like the app's own code. It has no
   install scripts and no native code, and it reaches the network only
   through connections the app declares, so a package cannot phone home or
   read a secret the app did not hand to its connection.
3. **A swap point is a capability.** Where an app should be able to change
   provider without changing code, the language adds a capability with all
   five parts. Packages fill capabilities; they never invent them (area 11
   question 19). Anything else from a provider is called as that provider.
4. **Extend by events, override by name.** A package, or a built-in
   feature, offers extension through events an app subscribes to and
   settings it declares. An app overrides a template or translation by
   writing its own file at the same key. Nothing patches another
   declaration's behaviour.
5. **Everything a package brings is reported.** The manifest, the network
   report, the personal-data report and the approval report show what each
   package adds, with an upgrade shown as a diff of what the app uses.
6. **Development never leaves the machine.** Every capability has a local
   driver that `jadpo dev` and `jadpo test` use in place of the wired one.

A new feature that needs a backend asks, in order: is it state the app
already keeps (then it names a store), is it a job several providers do and
the compiler gains from knowing it (then it is a capability), or is it one
provider's API (then it is a provider package).

## Package kinds

Decided in discussion with Richard, 2026-10-16.

| Kind | Contains | Used as |
|---|---|---|
| Provider | One API at one version: resources, operations, types, failures, events, hosts; optionally `provide` mappings and library functions | A connection; `connection.resource.operation(input)` (area 11) |
| Library | Types, failures and library functions under `shared/`'s rules: pure, no entities, no routes, no network | `use Postcodes`, then `Postcodes.uk.parse(text)`, `Postcodes.UkPostcode` |

**No feature packages.** A whole slice of an app (invitations, billing)
is not a package, installed or copied in. An agent writes it from the
language pages, with the compiler catching the dangerous mistakes, and
most features other frameworks take from packages (sign-in, flags, audit,
files, email) are built in. Where agents keep getting a feature wrong
(chunk 10.2 measures this), the fix is a worked reference in `examples/`
that the agent reads, not a package system.

**No driver packages.** Anything that needs native code (a new database,
a signing scheme) comes only through a Jadpo release. Packages never carry
native or WebAssembly code.

**Library package types may be stored** in entity fields and used in
route inputs and outputs, unlike provider types. An upgrade that changes a
stored type appears in the upgrade plan and the migration, as a change to
the app's own type does.

## Inside a package

Decided 2026-10-16: the layout is `shared/`'s, so one set of rules covers
the app and every package, and the compiler checks a package as it checks
`shared/`.

```text
postcodes/v2/
    package.jadpo                   the index: name, version, description, licence, requirements
    types/uk_postcode.jadpo         public type UkPostcode = Text { ... }
    failures/invalid_postcode.jadpo
    libraries/uk.jadpo              library uk { public parse(...)  normalise(...) }
    libraries/uk.test.jadpo         tests beside the file, as in an app
    CHANGELOG.md                    read by the upgrade plan
    surface.json                    generated: the package's public surface (proposed below)
```

- **`public` marks the package's API**, as it marks what a domain offers
  other domains. Everything else is internal and may change freely.
- **A package's tests run when it is published**, not in every app's
  `jadpo test`; the digest guarantees the code is the code that passed.

## Where packages live

Decided 2026-10-16: in the repository, committed. This changes where area
11 put registry packages (a cache on the developer's machine); the pin by
digest and "never fetched during a build" stand.

```text
app.jadpo
domains/
shared/
packages/
    postcodes/v2/           from the registry: digest in jadpo.lock, read-only
    stripe/v2024_06_20/     a provider package, same place
    acme_couriers/v1/       a local package: not in the lock, the app's to edit
jadpo.lock
```

- Agents read packages with the tools they already use, reviewers see an
  upgrade as an ordinary diff, and CI needs no fetch step.
- The compiler refuses a registry package whose files do not match its
  digest. Changing one means taking it out of the lock, and it becomes a
  local package the app owns.
- Folders nest by version, so two versions sit side by side: a provider
  during an upgrade (area 11 question 3), a library while an upgrade plan
  compiles the app against the candidate, or while callers move across.
  While two versions are in use, names carry the version
  (`Postcodes.v3.UkPostcode`), as area 11 question 16 decided for
  providers.

## The package surface

Proposed 2026-10-16 (Richard's idea). The compiler works out a package's
public surface and writes it to `surface.json`: every public type with its
fields and constraints, every failure with its kind and code, every
function's signature, failures and `///` description, the requirements and
licence, and for a provider its operations, hosts, credential scheme,
settings and the capabilities it provides.

- **One source for documentation.** The registry renders each package's
  pages from its surface, so every package is documented the same way,
  and an agent reads one file instead of the package's code (also as
  `jadpo package show <name> --json`).
- **Generated, never written.** `jadpo package publish` writes it, and the
  registry refuses a surface that does not match the code. A local
  package's surface is generated into `build/`.
- **It decides the version.** Publishing compares the new surface with the
  last release's and refuses a breaking change (a public name removed, a
  field retyped, a new failure) without a new major version, as Elm's
  package manager does. The upgrade plan diffs the same two files.
- **Examples in descriptions are tested.** An example in a function's
  `///` description is compiled and run at publish, so the documentation
  cannot drift from the code.

## Publishing

Decided 2026-10-16 (Richard). The surface above is generated as proposed,
and `jadpo package publish` refuses a package unless:

1. it compiles with no errors and no warnings;
2. every public type, failure and function has a `///` description;
3. every public function has at least one example in its description,
   compiled and run at publish, and every failure it can raise is shown by
   an example or a test;
4. its tests pass;
5. its version agrees with the surface diff: a breaking change needs a new
   major version;
6. its changelog's list of changes is generated from the surface diff, and
   the author writes only why;
7. it declares a licence from a fixed list and the lowest Jadpo version it
   needs.

Local packages skip the bar until they are published. Descriptions alone
are not the bar, because the compiler cannot judge whether one says
anything; examples are, because they must run.

## Built in or a library package

Agreed 2026-10-16 (Richard) as input to chunk 5.4, which decides it. A
thing is built in when the compiler gains from knowing it: a type that
turns a mistake into a compile error, carries personal-data marking, a SQL
or OpenAPI form or generated test values, or a security boundary the
language should own. Saving an install is not enough.

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
language. With the common cases built in, public library packages are the
long tail, and the largest use is likely private: types and rules shared
between one company's own apps. Open for 5.4: where fixed reference data
(a country table, phone formats) can be declared at all.

## How far a capability mapping goes

Area 11 question 21 limits a `provide` mapping to field paths, literals,
lists, nested blocks and enum tables, with real code in the runtime. The
eight packages modelled in the capabilities research needed nothing more
for mapping beyond a map over a list for batches, which the first build
leaves out; the other gaps they found (hosts with a placeholder, two-part
credentials, new webhook checks) are runtime parts, already decided.

When a provider does need a value computed (an address list joined into
one string, a body encoded), the choice is between a Jadpo release that
adds a runtime feature and letting the mapping call a pure function
(question 3). The safety that matters is rule 2, no native code and no
network beyond declared connections, and a pure Jadpo function keeps it:
the compiler checks it and it has no effects. Wire formats, signing and
protocols stay in the runtime either way.

## Choosing a provider when deploying

`email: { connection: postmark }` is fixed when the app is built. That
suits a team that writes and runs its own app. It does not suit an app
that other people deploy, such as an open-source product run by many
operators, each with their own mail provider, who should not have to edit
source.

The fit with the rules (question 5): the app declares every connection it
supports, and a setting chooses among them when the app starts.

```text
// app.jadpo
email: {
    connection: config.mail_provider    // MailProvider.smtp or .postmark
    from: config.mail_sender
}
```

Every candidate is compiled, checked and listed in the network report,
and the configuration report shows which settings each one needs, so
nothing is wired at runtime that review has not seen. A connection that is
not chosen needs no credential.

Two related needs are later, each when an app asks: sending different
messages through different connections (marketing mail through one,
password resets through another), and falling back to a second provider.
Fallback is only safe when the first surely did nothing, which area 11's
`unavailable` fault already tells apart from `outcome_unknown`.

## Every extension point

Everything in the language pages that an app might swap, extend or get
from a package, and which way it goes.

| Thing | Way | Today | Change proposed |
|---|---|---|---|
| Database | Capability, built-in drivers only | PostgreSQL; D1 later | None |
| Cache | Capability, built-in drivers only | Redis; in memory | None |
| Files | Capability | S3 driver; R2 and S3 as preset packages; local disk | None |
| Email | Capability | SMTP driver (7.3); mail packages; local inbox | Deploy-time choice (question 5) |
| SMS | Capability | Packages; local inbox | None |
| Publishing | Capability | Packages; local log; outgoing webhooks as a driver later | None |
| Schedules | Capability, built-in ticks only | Timer; Cloudflare cron; external tick | None |
| Sign-in providers | Built-in presets plus generic OIDC (area 8) | Not built | Become a `sign_in` capability (question 6) |
| Search | Entity's own store, PostgreSQL full-text | Not built | Candidate capability when an app needs typo tolerance (question 7) |
| Push notifications | Not yet | — | Candidate capability (question 7) |
| Bot protection | Not yet | — | Candidate capability (question 7) |
| Rate-limit counters | Names a cache connection | Redis or bounded memory | None: a store, not a capability |
| Flags, sessions, idempotency keys, outbox, audit | Name a database connection | Main store | None: a store, not a capability |
| Event delivery | Runtime only | Outbox in the main store | None: external queues are publishing targets |
| Telemetry | OpenTelemetry settings | Any vendor's endpoint | None |
| Secrets | Environment variables | `config.jadpo` | None: a secrets manager fills the environment |
| Payments, AI models, geocoding, tax, any other API | Provider package | — | None: one provider's API each, called as that provider |
| Shared types and helpers | Library package | — | New kind (decided 2026-10-16) |
| Whole features (billing, CMS) | Written by the agent; worked references in `examples/` | — | Not a package (decided 2026-10-16) |

**Sign-in providers** are the one built-in list that should follow the
pattern and does not. Area 8 decided presets (Google, GitHub) plus generic
OpenID Connect, built after the first build. As a `sign_in` capability,
generic OpenID Connect and OAuth 2.0 become built-in drivers and each
preset becomes a package that sets the issuer, the scopes and where the
email and its verified flag come from. Adding "Sign in with Apple" is then
a package, not a compiler release; anything that needs real code (Apple
signs its client secret as a JWT) is a credential scheme in the runtime,
as for any provider.

**What stays a provider package and not a capability.** Payments (money
moves at the provider, and switching provider is a migration of customer
records, never a configuration change), AI models (each provider's options
differ in ways the compiler gains nothing from hiding), and any API one app
uses. An app that wants to swap one of these writes one action per job,
and that action is the swap point. A job that several apps keep wrapping
the same way is the signal to propose a capability through review.

## What each coming chunk follows

| Chunk | Follows |
|---|---|
| 5.1 Configuration | Connection settings come from `config.jadpo`; an enum setting can choose a connection if question 5 is agreed |
| 7.1 Packages and connections | `packages/<name>/<version>/` committed and checked against `jadpo.lock`, and `jadpo package` commands; library packages laid out as `shared/` and resolved with `use`; the index's requirements (question 4); the surface (question 8) |
| 7.2 Importing and faking | Imports write into `packages/`; unchanged otherwise |
| 7.3 Email | The `email` capability with SMTP as one driver and a mail package as the second, proving the swap; the local inbox as the development driver; templates overridable by name |
| After the first build | Sign-in providers as a capability (question 6); search, push and bot protection only when an app needs them (question 7) |

## Questions for Richard

Questions 1, 2 and 4 change what chunk 7.1 builds. The rest change nothing
until later, but answering them now fixes the pattern.

1. **Which kinds of package exist?** *Decided 2026-10-16 (Richard):*
   provider and library packages; no feature or driver packages (see
   "Package kinds").
2. **Where do packages live, and how are they laid out?** *Decided
   2026-10-16 (Richard):* committed in `packages/<name>/<version>/`, laid
   out as `shared/`, `public` marking the API, `jadpo package …` commands
   (see "Inside a package" and "Where packages live").
3. **May a capability mapping call pure functions?** *Recommendation:* yes,
   functions from the package's own libraries and the standard library,
   added with the first package that needs one, not before. Rule 2 still
   holds, and the alternative is a Jadpo release for each provider's quirk.
4. **How do packages depend on each other?** *Recommendation:* a package
   lists its requirements; one version of each in use, two only while
   callers move across; every requirement written into `jadpo.lock`, so
   nothing arrives that the lock does not show.
5. **Can the provider be chosen when deploying?** This depends on whether
   Jadpo apps will be run by people who did not write them.
   *Recommendation:* yes, when the first such app needs it: the app
   declares every connection it supports and an enum setting picks one at
   start, so every candidate is still compiled, checked and reported.
6. **Do sign-in providers become a `sign_in` capability?**
   *Recommendation:* yes: generic OpenID Connect and OAuth 2.0 as built-in
   drivers, Google, GitHub, Microsoft and Apple as packages. Area 8's
   generated routes, linked accounts and `on_first_sign_in:` stay as
   decided. Nothing of it is built yet, so the change costs nothing now.
7. **Which other capabilities are candidates?** *Recommendation:* search
   (an external index such as Meilisearch or Typesense behind the decided
   `search:` section), push notifications and bot protection, each added
   through review only when an app needs it. Payments and AI models stay
   provider packages.
8. **Does the compiler generate a package's surface?** Richard's idea,
   2026-10-16. *Decided 2026-10-16 (Richard):* yes, as "The package surface" describes:
   `surface.json` generated at publish and checked by the registry, the
   registry's documentation rendered from it, publishing refused when the
   surface breaks without a new major version, and examples in `///`
   descriptions run at publish.
