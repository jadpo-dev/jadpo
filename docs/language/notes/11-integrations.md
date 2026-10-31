---
title: External services, design notes
area: 11
---

# External services: design notes

Why [External services](../11-integrations.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Status

The area page's sections from "How the package format is read" to "Enum
values and failures through a package" settle what the topic sections
leave open, taking the most conservative reading.

## Consistency with the rest of the language

What this area sets in other areas:

| Area | Rule |
|---|---|
| 1 | Connections are in `app.jadpo`; installed packages in a root `packages/` folder and `jadpo.lock`, the app's own integrations in `shared/providers/`; a domain's ownership of a provider is the connection's `domains:` list. There is no `integration` keyword or `integrations/` folder |
| 1 | A package's public members are its interface, so area 1's error for a `public` declaration nothing outside uses doesn't apply to them; everything reachable from a public declaration must be public, in packages and domains alike |
| 5 | `connections:` holds every store and provider, each `driver: Driver.x` or `package: X.vN`; the cache runs in memory in development |
| 7 | Webhook routes are generated from the package, never written by hand, and the runtime drops repeats |
| 7 | Each API's OpenAPI document is also served at `<prefix>/openapi.json` with `x-jadpo` fields, which the importer reads |
| 7 | Readiness doesn't probe providers, so a provider outage doesn't take the app out of service |
| 8 | The generated webhook route checks the signature, so no route names a signature as its authentication; another Jadpo app is called with a service key |
| 8 | Google, Microsoft and other OpenID Connect providers are `Driver.oidc` with an issuer; GitHub and Apple are the built-in `Driver.github` and `Driver.apple` |
| 6, 10 | Once a multi-step record exists, every rerun uses the time stored in it |
| 12 | Up to 8 keyed changing calls in subscribers and keyed routes; a run cut off with a record resumes |
| 13 | Unscripted calls answer from the generated fake; scripted lists keep area 13's leftover check; fakes keep keys and take `lost` and `in_use`; crash-point reruns |
| 17 | Provider call payloads are shown in full in development only |

Other points:

- **Two dots in a call.** `couriers.bookings.create` is the first call with
  two dots. The dot still names the owner: the connection owns its
  resources and actions, a resource its operations. A library package is
  called through its own name (`Postcodes.uk.parse`), as the standard
  library is.
- **No anonymous objects** (area 2). A call's input is a named package
  type, `AcmeCouriers.CreateBooking { ... }`.
- **Area 4's mapping form** holds: `condition: Failure` in the call's block,
  kind checked, the provider failure kept as the internal cause.
- **Areas 3, 9 and 12 hold:** calls only from actions, the changing calls
  before the writes (area 3 cites question 7), whoever may run the action
  may call (area 9).
- **Area 16** sends email through the `email` capability: SMTP is one
  built-in driver, a mail package another. There are no `EmailBounced` or
  `EmailComplained` events: bounces are the package's own events.
- **`public` on libraries.** Area 1 makes a library public as a whole and
  refuses `public` on a function; packages follow it, so a package's public
  functions are its public libraries' functions, and its decoders live in
  a library that is not public.
- **Twilio** signs webhooks with its own HMAC-SHA1 scheme, not Standard
  Webhooks (question 25).
- **Mail templates** read `translations.<domain>.<message>` (areas 10
  and 16).

## Open questions

- **May a package's code read the app's `shared/`?** The pages say a
  package is checked as `shared/` is without saying. It reads only its own
  declarations and the built-ins; allowing `shared/` later breaks no
  package.
- **Official copies of providers' own packages.** When a provider
  publishes its own Jadpo package in its repository, should the registry
  mark it official and serve a cached, digest-pinned copy?
- **Checking for updates.** Should `jadpo packages outdated` check every
  package, or one, for updates? For a chosen version, or a downgrade, the
  upgrade plan would fetch the changelog, compile the app against it, and
  list the calls and types affected, the compile errors that would follow
  and the behaviour changes (new or removed failures, fields, events), for
  a person to decide.

## Decisions

Numbered as the questions they settled, which other pages cite.

1. **Request and response types** come from the package, namespaced
   (`Stripe.PaymentIntent`), and are usable anywhere in code; the app
   writes none and needs no narrowed or derived types, as with TypeScript
   SDKs and Smithy clients. A provider package's object and enum types, its
   library types included, can't be an entity field (that would tie stored
   data to the provider's version) or part of a route's `Input` or
   `Output` (that would make the provider's schema part of the app's API);
   its ids and plain values can. A library package's types may be stored
   and used in route inputs and outputs, and a changed stored type appears
   in the upgrade plan and the migration. A package input may take any of
   the app's object types, sent as JSON. At runtime a response is checked
   only for the fields the code reads.
2. **An operation is called** as `connection.resource.operation(input)`,
   only from actions, with `attempt` or `match`, reading like the
   provider's own SDK, so agents already know it. A provider package's
   public actions are called the same way, `connection.action(input)`,
   sharing the connection's namespace with its resources; a library
   package's functions through the package's name
   (`Postcodes.uk.parse(text)`), with no connection.
3. **Provider versions.** One package per version; two versions are two
   connections run side by side; an upgrade adds the next, moves callers
   and deletes the old, as Stripe advises. Two versions of a library
   package can also sit side by side while callers move, as can a
   candidate version while the upgrade plan compiles against it; the
   approval report shows the surface diff.
4. **Pinning.** Installed packages are committed under
   `packages/<name>/<version>/`, read-only and checked against their
   digest in `jadpo.lock`, which records each one's source (`registry`,
   `git`, `path` or `link`); nothing is fetched during a build, as with
   Cargo's and npm's lock files. `jadpo package add` and `update`
   regenerate the surface and refuse one that differs from the committed
   `surface.jadpo_record`. The app's own integrations live in
   `shared/providers/` with no lock entry.
5. **Failures.** A package's failures are mapped by code at the call to
   any declarable failure of a fitting kind; `timed_out` maps to an
   `unavailable` failure only on idempotent operations; anything unmapped
   is a fault. A package failure is never in an app's `fails`, so a
   provider's codes never become part of the app's API. The provider's
   "different request" answer to a key is `diverged`, never
   `misconfigured`, and its "key in use" answer is waited on and retried,
   never a failure; a package action's failures are mapped twice, by the
   package and then at the app's call, and replayed failures are mapped
   again by the current code.
6. **The idempotency key** is the runtime's; the operation only says where
   it goes. Its inputs are the delivery key in subscribers and schedules
   and the client's key or request id in routes, never sent as they are:
   the key sent is `jd1_` and 32 base32 characters of a SHA-256 over the
   application, environment, connection, caller scope, action key, step
   number and key generation, cut to the package's `max_length:`, because
   Stripe and others scope keys to the account, not the caller. The
   package gives its key's `window:`.
7. **A call and a write in one action.** Allowed when the call comes first:
   the writes commit after it returns, and a failed commit retries the
   call with the same key. A write before the call is an error; that case
   emits an event. An action may make up to 8 changing calls, each keyed or
   safe to repeat, only where its key survives a retry (subscribers, and
   routes whose API requires `Idempotency-Key`), recorded step by step and
   replayed on a rerun; a changing call with no key stays the only one in
   its action ("Several changing calls in one action").
8. **Retry settings.** `timeout:` (default 3s) and `retry: { attempts:
   within: }` (default 3 within 30s), the same words as area 12's
   subscriber retry, with exponential backoff and full jitter fixed;
   Temporal's five knobs are mostly left at their defaults. A call's 3
   attempts of 3 seconds fit a route's 10-second default deadline; at 10
   seconds a call could take 30, and the deadline sum
   (`route.calls_past_deadline`) would refuse almost every route that
   calls out. `ai.generate`'s timeout defaults to 25s and `ai.embed`'s to
   5s (area 21). `max_concurrent:` in `provider.jadpo` caps calls at once
   per app instance, and a connection may lower it; a step in doubt is
   sent again only within the provider's key window.
9. **The outbound host.** Outbound network access is denied by default, as
   Deno's permissions are: Jadpo code has no raw `fetch`, and the runtime
   refuses any host not listed by a declared connection's package. Where an
   API is comes from exactly one place: `hosts:` (live and sandbox, the
   connection's `environment:` picking one), `runs:`, or the connection's
   `host:`. HTTPS only, with no redirects or proxies. The build writes a
   network report of every reachable host, its connection and the domains
   using it; a new connection, a changed host or a new `outbound:` grant to
   a service appears in the approval report. Connections are the
   permission list, with no second list of hosts. Outgoing webhooks are the
   one runtime-owned network exception (area 16).
10. **Credentials.** The package declares the scheme and the connection
    gives only `credential: config.x`: bearer, named header and basic;
    AWS signing with the first AWS package; OAuth client credentials when a
    real app needs them. A connection's credential may also be an
    `OAuthGrant` owned by one of the app's records, with the provider's
    addresses in `oauth:` and every call naming its owner with
    `on_behalf_of:` ("Acting for each user").
11. **Webhooks** are received by a route generated from the package when
    the connection has a webhook secret: the signature checked with a
    five-minute tolerance, repeats dropped by event id, each event written
    to the outbox, public or not as the package marks it, with the
    provider's `occurred_at`. No webhook route is written by hand, and
    area 7 follows this. A package may subscribe to and enrich its own
    webhooks; webhooks that need a reply name the app's answering action;
    handshakes are built-in rules; queues arrive through `receiving`
    ("Receiving from outside").
12. **Fakes.** A fake generated per package answers with valid defaults; a
    test scripts answers with `fakes:`, checked against the package's
    types; `jadpo test --kind contract` runs against the sandbox in
    scheduled CI and covers every unconfirmed written operation. Fakes
    keep idempotency keys as the provider does and take `lost` and
    `in_use`; `jadpo test` reruns multi-step tests with a crash at each
    step boundary; `jadpo dev` fakes every provider call by default.
13. **Where a package comes from.** `jadpo package add` follows a fixed
    order and records which step it used, so an agent never improvises
    where to look: the registry, official package first; the provider's
    published specification (OpenAPI, Smithy, GraphQL), given a URL or
    found in its known places; an importer from the official SDK's type
    definitions, a Postman collection or a HAR capture; or written from
    the provider's documentation, in exactly the format an importer
    produces, with only the operations the app uses. The project's agent
    instructions (generated by `jadpo new`) say the same. It also takes a
    git repository and another Jadpo app's OpenAPI. Imports and
    hand-written packages are the app's own integrations in
    `shared/providers/`. One format means one compiler path and one
    review.
14. **Provenance and confirmation.** Every operation has `source:
    PackageSource.imported` or `.written` (with `docs:`); `jadpo package
    record` saves a sandbox answer with its values replaced by
    placeholders, which the output type is checked against, and every
    decoder must decode every recording. Unconfirmed written operations
    are flagged in the approval report and need a passing scheduled
    contract run, but don't block a build: a sandbox outage shouldn't stop
    a release.
15. **The registry** is Jadpo's own. It holds Jadpo packages of both kinds
    as Jadpo source, never native code or install scripts, pinned by
    digest and never fetched during a build; it repeats the install checks
    on upload and adds discovery, curated packages marked official,
    publisher verification, signing and a documentation website rendered
    from surfaces. Curated packages (Stripe, Twilio, GitHub) start in the
    Jadpo repository; the hosted registry, and opening publishing with
    verified namespaces, come when outside demand appears. Private
    packages come from git, with no private registry. Another Jadpo app's
    package is imported from its served OpenAPI. Upgrade plans are area
    18's review of the surface diff. The importers other than OpenAPI come
    each when first needed: Smithy with the first AWS package, then the
    SDK-types, Postman and HAR importers. Registry names are scoped by
    publisher, `@publisher/name`, with the code name unchanged (Richard,
    2026-10-31): every registry with flat names has fought squatting and
    typosquats, and a scope tells a reader who stands behind a package.
    Releases need a releaser's passkey by default, with an opt-in 24-hour
    automatic release for routine releases, a seven-day wait for a new
    releaser and successors named in advance (Richard, 2026-10-31).
    Publishing whenever a tag appears would make a package as safe as the
    weakest credential with write access to its repository, which is what
    the 2025 and 2026 npm worms stole; a passkey can't be phished or copied
    off a disk. The wait for a new releaser answers the xz-utils takeover,
    where a stranger earned release rights; refusing to hand over a package
    with no named successor answers event-stream.
    The rest of the registry was decided the same day (Richard, "I agree
    with all of the suggestions"), from a review of registry incidents
    (left-pad, event-stream, ua-parser-js, colors and node-ipc, manifest
    confusion, xz-utils, Ultralytics, tj-actions, polyfill.io, revival
    hijacking, boltdb-go, the Shai-Hulud and ChainDrop worms, axios and the
    2026 crates.io compromise, slopsquatting) and of what developers like
    and dislike in npm, crates.io, PyPI, Go and JSR. Releases are pulled
    from a tag, never uploaded, so nothing a publisher sends can disagree
    with the source. Facts from the graph replace a score, because one
    number hides what a reviewer needs and invites gaming; there are no
    ratings for the same reason. The three-day wait is in the client, not
    the registry, because every hijacked release in that review was found
    within hours, and an urgent fix must never wait. Search reads text
    first because people describe a need in words without knowing its
    types. Copies are tracked because packages copy instead of depending,
    so a fix would otherwise never reach them. The verifier is the `jadpo`
    tool in a container, reached as a provider, so the registry is an
    ordinary Jadpo application.
16. **Package names** are UpperCamelCase with the package's own version
    label: `Stripe.v2024_06_20`, `AcmeCouriers.v1`, `Postcodes.v2`. Types,
    failures and events are named through the package without the version
    (`Stripe.PaymentIntent`, `Postcodes.UkPostcode`), so two connections to
    one version share types; while two versions are held, names carry the
    version (`Stripe.v2025_01_01.PaymentIntent`). Curated provider brand
    names are reserved, and installing a package whose name the app's own
    code uses stops until the app's own is renamed.
17. **Connection names** are lower_snake_case, chosen by the app (`stripe`,
    `couriers`), sharing one namespace with domains and libraries, so a
    clash is a compile error. Resource and operation names come from the
    package, following the provider's SDK where the importer can map them.
    A package's resources and public actions share the connection's
    namespace, so a clash between them is an error in the package.
18. **Calling domains.** Each connection lists them, `domains: [payments]`,
    required; other domains go through that domain's public actions. It
    keeps area 1's ownership, and a new caller is a one-line diff in
    review. The list covers the package's own operations on a connection a
    capability uses too (`email:` uses the connection; only the listed
    domains call it); a library package needs no connection and any domain
    may use it.
19. **Capabilities.** A capability exists only for a job Jadpo performs
    itself and covers it whole, with no optional parts: `database`,
    `cache`, `files`, `email`, `publishing`, `receiving`, `schedules`,
    `search`, `bot_check`, `documents`, and `scanning` when an app needs
    it. Nothing in the language sends SMS, so there is no `sms`; Twilio is
    a package. Event delivery stays in the runtime; telemetry is
    OpenTelemetry; push notifications come later. Adding a capability is a
    language change, always with the same parts: a native API, standard
    types, a local driver, a fake and a conformance suite. Packages can't
    invent capabilities: Dapr and Encore own a fixed list too, and Dapr's
    generic "bindings" show that an escape hatch undoes the point.
    Everything provider-specific (hosted templates, bounces, suppression
    lists) is the package's own operations and events.
20. **One `connections:` block.** Each entry is `driver: Driver.x` (built
    in) or `package: X.vN`, with typed settings the driver or package
    declares, and the use site says which capability it serves. One
    connection is one resource (a bucket, a queue), not one vendor
    account, as Cloudflare's bindings are: R2 and Queues need different
    credentials, and cron none. There are no presets as packages: anything
    Jadpo has a built-in driver for is configured in `app.jadpo` (R2 is
    `Driver.s3` with its host and `region: "auto"`), with vendor settings
    documented on each driver's page; a vendor with storage and an API is a
    driver connection plus a package.
21. **Providing a capability.** A file `provides/<capability>.jadpo`, found
    by name, maps each standard operation onto the package's own
    operations, as data only: field paths, literals, lists, nested blocks,
    enum tables, and `each:` with `as:`. Provider features come through
    typed connection settings and the package's own API, never through the
    capability. Protocol drivers, signing and webhook checks are built into
    the runtime. Bounces are the package's own events, and erasure is
    `erasure:` in `provider.jadpo` with the connection's key.
22. **Proving and faking a capability package.** One conformance suite per
    capability, run by `jadpo package conform` against the sandbox or an
    emulator; the registry lists a capability only with a passing run
    pinned to the digest, and the app's own integrations run it in
    scheduled CI. Tests get one fake per capability (`email.sent`,
    `published`), whatever the provider; a package's own operations keep
    their generated fake.
23. **Development.** Nothing leaves the machine: mail to the local inbox,
    files to local disk with local signed links, published events to a log
    in the dev console, schedules on the local timer, search on
    PostgreSQL, and every provider operation faked from recordings and
    generated defaults unless `development: { sandbox: [...] }` lets it
    through to the sandbox, every call logged with its payload in full.
    The database runs its real engine; the cache runs in memory, which
    narrows area 5's "Stores" (every environment runs the same engine) safely, since a
    cache can't change what a request returns (Encore and Rails do the
    same). `jadpo dev` starts the declared services.
24. **Targets.** Every driver and package lists the targets it runs on;
    `jadpo check` refuses a deployable that uses one elsewhere and names
    the alternative ("Targets"). The schedule tick is the target's: an
    in-process timer natively, Cloudflare cron on Workers (generated into
    the Wrangler configuration, also sweeping the outbox), or an external
    tick. A package that calls an HTTP API runs everywhere; a service
    (`runs:`) runs only beside a native deployable, so on Workers the
    connection gives its `host:`. On Workers, TCP works but port 25 is
    blocked, so SMTP needs port 587; there is no local disk; and an
    in-memory cache is an error.
25. **Gaps the modelled packages found**, each closed when the first
    package needs it: a sandbox that is a test credential is the same host
    twice in `hosts:` (Postmark, Twilio); a package host may hold one
    placeholder filled from a typed setting (a Supabase project's), shown
    as a pattern in the network report and checked at runtime; the
    credential scheme decides the credential's shape (SigV4 takes a key id
    and a secret); the webhook checks include basic authentication
    (Postmark, which signs nothing), Twilio's HMAC-SHA1, AWS SNS
    certificates and `handshake:`; and `connection.webhook_url` is a
    mapping value (Twilio's status callback).
26. **A write refused after the call took effect.** The call comes before
    the writes, so a write refused afterwards for good (a unique rule, a
    restricted reference) would roll back the database's only record of a
    change the provider kept. `jadpo check` warns at every point after the
    first changing step that can end the action for good ("The warning",
    under "Several changing calls in one action"), for single calls too.
    The run ends with its record kept, so an operator can retry once the
    cause is fixed, replaying every answer without contacting the
    provider.
27. **An example in a description** is a ```` ```jadpo ```` block in the
    `///` description, holding statements as a test's body does (`assert
    uk.parse("sw1a1aa") == "SW1A 1AA"`, `assert uk.parse("x") fails
    InvalidPostcode`). `jadpo package check` runs each as a test of the
    package, named after its function, and a failure counts as shown when
    an example or a test asserts it with `fails`. It reads as Markdown in
    the registry's page and `jadpo package show`, and uses nothing new.
28. **The changelog.** `jadpo package check` writes a `## <version>`
    section at the top of `CHANGELOG.md` for a new version, its list of the
    surface's changes under it (`added type OutwardCode`), and keeps the
    author's text beneath; the bar refuses a release whose section has no
    text of the author's.

Carried from other areas, and the runtime's own limits:

| Topic | Decision |
|---|---|
| Mapping form | From area 4: `condition: Failure`, such as `timed_out: ChargeUnavailable`; time limits are connection settings |
| Faults | From area 4: faults are never listed in `fails`; provider problems are mapped to declared failures at the call |
| Calls | From areas 1 and 3: the dot names the owner; `attempt` only on calls that can fail; actions have no settings |
| Setting options | From area 2: fixed options are enum values written `Type.value`; every member in braces is `name: value` |
| Calling providers | From area 9: whoever may run the action may call; there is no separate grant |
| Third-party services | From area 8: the language owns a feature rather than relying on a third-party service where it reasonably can |
| SMTP to this machine | The SMTP driver speaks to a server on this machine (`localhost`, `127.0.0.1`, `::1`) without `STARTTLS`, whether it offers it or not, as PostgreSQL's driver does, since a stock Postfix offers it with a self-signed certificate. A server elsewhere is upgraded and its certificate checked, and port 465 speaks TLS from the start |
| Addresses the runtime reaches | Plain HTTP's "this machine" is the host read as an address (`127.0.0.0/8`, `::1`) or `localhost`, never a name's first label; the runtime's own fetches judge an IPv4 address inside NAT64's `64:ff9b::/96`, 6to4's `2002::/16` or Teredo's `2001::/32` (its server's and its client's) as itself, and refuse site-local `fec0::/10`, NAT64's local-use `64:ff9b:1::/48` and the rest of `2001::/23` |

## Moved from the area page

When [External services](../11-integrations.md) was rewritten to the
[writing guide](../../writing.md), its references to the questions above,
its comparisons with other tools and the details of how some rules are
kept moved here. Nothing in this list changed a rule.

- **No feature packages**: where agents keep getting a feature wrong, the
  fix is a worked reference in `examples/`, not a package.
- **Provider types stay out of stored data and route contracts** (question
  1), because the next version of the API is a new package.
- **The golden rule is kept by process.** Every future package feature is
  designed with how the app writes it, and a proposal that only works
  inside a package is rejected. The gate checks it: each package example on
  the area page also compiles placed as the app's own integration in
  `shared/providers/`.
- **No `emails/` folder in a package** is consistent with the golden rule,
  which runs one way: a package doing less than the app is allowed.
- **The declaration is called `provider`**, matching provider packages and
  `shared/providers/`.
- **Names through the package**, `AcmeCouriers.Booking`, are question 16.
- **A vendor with storage and an API** is a driver connection plus a
  package, so there is no hybrid case.
- **No dependencies removes** version resolution, clashes between
  packages, "required by" tracking and clean-up on removal. It is also the
  reversible choice: requirements can be added later without breaking
  anything published, if real packages keep copying something substantial
  that doesn't belong in the standard library.
- **The surface's screens** (`jadpo package show`) follow the CLI design
  language and are designed before they are built.
- **The surface decides the version** as Elm's package manager does: a
  breaking change needs a new major version.
- **Private packages from git, with no private registry**: packages are
  committed into apps, so a registry's caching adds nothing; Go is the
  precedent. The lock's source field leaves room for a private registry
  later.
- **Descriptions alone are not the publishing bar**, because the compiler
  can't judge whether one says anything; examples are, because they must
  run.
- **Examples in descriptions run as tests** (question 27), and **the
  changelog's list is written from the surface diff** (question 28).
- **Where `jadpo package add` looks** follows a fixed order and records the
  step it used (question 13).
- **The registry is Jadpo's own**, in line with Identity's rule that the
  language owns a feature where it reasonably can. The hosted registry is
  question 15.
- **Operation provenance** (`source:`) is question 14.
- **Connections are infrastructure, declared once** in `connections:`
  (question 20), and name the domains that may call them (question 18).
  HTTPS only, redirects and proxies refused, is question 9; code never
  reading the credential is question 10; the default timeout and retries
  are question 8; two versions side by side is question 3; a connection's
  name sharing the domains' namespace is question 17.
- **Calling.** Types coming from the package, the app writing none, is
  question 1; a package input taking any of the app's object types is a
  general addition Values and types designs. Mapping failures at the call
  is question 5, idempotency keys question 6, retries question 8, and the
  order of calls and writes question 7.
- **Package subscribers are tested against live PostgreSQL** in
  `crates/jadpo-cli/tests/package_subscribers.rs`: a signed `invoice.paid`
  reaches the package's subscriber once and the renewal it emits reaches
  the app's subscriber through the outbox.
- **Full package permissions wait**: a package that owns tables or runs
  its own schedules waits until a real app needs package-owned data;
  adding grants later breaks nothing.
- **Package polls are tested against live PostgreSQL** in
  `crates/jadpo-cli/tests/package_polls.rs`, `max_concurrent: 1` included.
- **Services and published ports**: whether a network that blocks
  outbound traffic can publish a port to the machine is checked when
  services are built. Services come in stages, each useful alone.
- **Several changing calls.** The warning holding for single calls is
  question 26, and the deadline sum checking a single call is question 8.
  The 8-step limit is confirmed by measurement, like Jobs and events'
  other limits.
- **No webhook route is written by hand** (question 11): a connection's
  webhook secret generates `POST /webhooks/<connection>`, and the package
  declares the events and how they are signed.
- **The built-in signature schemes** (Stripe, Standard Webhooks, HMAC-SHA256
  with a named header, basic authentication, Twilio's HMAC-SHA1 and AWS SNS
  certificates) are question 25's.
- **Where receiving is proved.** `crates/jadpo-cli/tests/webhooks.rs` shows
  a webhook event reaching an app's subscriber, and
  `crates/jadpo-cli/tests/webhook_replies.rs` a reply and its repeat, both
  against live PostgreSQL. `crates/jadpo-cli/tests/receiving.rs` runs the
  SQS package, which is in this repository at `packages/amazon_sqs`,
  against a local fake of SQS's JSON API that checks each signature.
- **One `connections:` block**, where each connection's use says which
  capability it serves, is question 20's decision.
- **`provides/<capability>.jadpo`**, a data-only mapping of each standard
  operation onto one of the package's, is question 21's.
- **One fake per capability**, whatever the provider (`email.sent`,
  `published`), is question 22's.
- **Nothing leaves the machine in `jadpo dev`**: published events to a log,
  search to PostgreSQL, the cache in memory (question 23).
- **Each driver and package says which targets it runs on**, and
  `jadpo check` refuses a deployable that uses one elsewhere (question 24).
- **Every written operation that isn't confirmed** must have a case in the
  contract run (question 12).
- **A package's examples** in its `///` descriptions, run as tests in the
  scratch app, are question 27's; its `CHANGELOG.md` is question 28's.
- **`domains:` is always required** on a connection to a package
  (question 18).
- **A connection named like a domain or library** is
  `semantic.duplicate_declaration` (question 17).
- **How a version is worked out** compares a package's surface with its
  newest release's entry by entry, as Elm's `bump` does.
- **Why the importer writes `wire_name:` per field** rather than a
  provider-wide naming setting: most APIs other than Stripe's are
  camelCase, and a provider-wide setting would hide the exceptions.
- **A package action calls no other action.** The pages let a package
  action call its operations and its pure functions, and keeping it to one
  body keeps each way through it one sequence of steps. This can be
  loosened later without breaking a package.
- **How the runtime is built for provider calls.** The package driver is
  `jadpo_drivers::package::JsonHttp`, over the `Fetch` its host gives:
  natively `jadpo_host_native::fetch::NativeFetch`, hyper's HTTP/1.1
  client on the worker's thread, with rustls checking certificates. The
  executor runs a package's decoders at the call's site through
  `provider::Functions`; the driver tells a timeout apart with
  `Answer::TimedOut`; the test host's fake is
  `jadpo_runtime::fakes::ProviderFake`.
- **How the Cloudflare Queues driver is built.** The Workers host hands
  each batch to the runtime's seam, `receiving::consume`; the Wrangler
  consumers are written by `jadpo_std::receiving::wrangler_queues`, and
  Wrangler's `deploy --dry-run`, pinned in `tools/fixtures/workers/`,
  accepts them without a warning.
- **Lazy loading** of an installed package's resources is
  docs/compiler-runtime.md §2.3; how a process wakes the calls waiting for
  a `max_concurrent:` slot is §2.6, "Six connections an invocation".
- **Cross-references the area page dropped.** A queue message whose event
  the store refuses as a statement is parked `unreadable`; the page used
  to point at area 12's "Queue messages the store refuses", which is no
  longer a heading there. `Wire.day_month_year` is the encoding areas 12
  and 19 name for file rows, here built for providers.
- **"Errors still to build"** is now the "Mistakes not reported yet"
  section of the area page's "What is built", and its topic ID,
  `errors-still-to-build`, an old ID of `external-services-built`.
