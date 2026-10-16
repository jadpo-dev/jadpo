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

| Kind | Contains | Used as | Status |
|---|---|---|---|
| Provider | One API at one version: resources, operations, types, failures, events, hosts; optionally `provide` mappings | A connection; `connection.resource.operation(input)` | Decided (area 11) |
| Library | Types, failures and library functions, under `shared/`'s rules: pure, no entities, no effects | `Money.Amount`, `Money.rounding.half_even(x)` | Proposed (question 1) |
| Domain | Entities, actions, queries, routes, events, emails: a whole feature such as billing | Mounted as a domain by the app | Later (question 1) |

A provider package may also hold library functions, for helpers that
belong with an API (formatting Stripe's amounts), and those functions are
what a capability mapping may call (question 3).

**Library packages** are a publishable `shared/`. Names follow area 11's
rule: the package's own name, then the declaration (`Money.Amount`), with
the version only while two versions are in use. A library function is
owned by its library, so a call reads package, library, function. They
need no connection, because they do nothing but compute.

**Domain packages** are the hard kind, and no app needs one yet. They
raise questions about migrations, policy, the principal and overriding
that deserve a design of their own when a real case exists. The
constraints that keep the door open, which nothing built before then may
break:

- The app mounts one under a domain name it chooses, and its entities
  migrate in the app's own snapshots, like the app's own entities.
- Policy stays the app's: a package may ship suggested grants, but none
  apply until the app writes or accepts them, and the permissions report
  shows them as the app's.
- It is extended through its public events and public actions, and its
  emails and translations are overridden by name (rule 4).
- It reaches providers only through connections the app gives it.

Jadpo's own built-in features (sign-in, flags, audit) already behave like
first-party domain packages compiled into the language. They stay built
in; domain packages, when they come, follow the same shape.

## Where packages live

Area 11 decided: registry packages are fetched into a local cache by a
command, pinned by digest in `jadpo.lock`, and never fetched during a
build; local packages are committed in the repository's `providers/`
folder. With library packages, the folder and the commands should not be
named after providers (question 2):

```text
packages/                 committed: local and imported packages
    acme_couriers/
        package.jadpo     the index: name, version, parts, requirements
        bookings.jadpo
        email.jadpo       a provide mapping, when it fills a capability
jadpo.lock                every registry package, version and digest
```

| Command | Does |
|---|---|
| `jadpo package add <name>` | Fetches into the cache, pins in `jadpo.lock`; for a provider, the fixed order of area 11 question 13 |
| `jadpo package import <format> <file>` | Writes a provider package into `packages/` |
| `jadpo package record` / `conform` | Confirms an operation; runs a capability's conformance suite |
| `jadpo package update <name>` | Upgrades, with the upgrade plan of area 18 |
| `jadpo package remove <name>` | Removes it, refused while code or a connection still uses it |

**Requirements between packages** (question 4). A package may require
others, by name and version, in its index. One version of each package is
used per app (two only as two connections during a provider upgrade), and
`jadpo package add` writes every requirement into `jadpo.lock`, so the
lock is the full list a reviewer reads and nothing arrives unseen. Npm's
nested copies of one package at several versions are not possible.

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
| Shared types and helpers | Library package | — | New kind (question 1) |
| Whole features (billing, CMS) | Domain package | — | Later, under the constraints above |

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
| 7.1 Packages and connections | `packages/` and `jadpo package` (question 2); the index lists its parts and requirements (question 4); library packages parse and resolve (question 1) |
| 7.2 Importing and faking | Imports write into `packages/`; unchanged otherwise |
| 7.3 Email | The `email` capability with SMTP as one driver and a mail package as the second, proving the swap; the local inbox as the development driver; templates overridable by name |
| After the first build | Sign-in providers as a capability (question 6); search, push and bot protection only when an app needs them (question 7); domain packages with their own design |

## Questions for Richard

Questions 1, 2 and 4 change what chunk 7.1 builds. The rest change nothing
until later, but answering them now fixes the pattern.

1. **Which kinds of package exist?** *Recommendation:* provider packages as
   decided, plus library packages (types, failures, pure functions, under
   `shared/`'s rules) from chunk 7.1. Domain packages later, designed when
   a real app needs one, under the four constraints above. Library packages
   cost little because `shared/` already has the rules; domain packages
   carry migrations and policy that no current app tests.
2. **Is it `packages/` and `jadpo package …`, not `providers/` and
   `jadpo provider …`?** *Recommendation:* yes, renamed now, before 7.1
   builds either. Library packages would otherwise sit in a folder called
   `providers/`, and renaming a command after people use it costs more than
   renaming it on paper.
3. **May a capability mapping call pure functions?** *Recommendation:* yes,
   functions from the package's own libraries and the standard library,
   added with the first package that needs one, not before. Rule 2 still
   holds, and the alternative is a Jadpo release for each provider's quirk.
4. **How do packages depend on each other?** *Recommendation:* a package
   lists its requirements; one version of each per app; every requirement
   written into `jadpo.lock`, so nothing arrives that the lock does not
   show.
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
