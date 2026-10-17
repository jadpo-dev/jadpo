# Override scenarios

Status: research, 2026-10-16, for
[packages and extension points](packages-and-extension-proposals.md).
Nothing here is decided. Sketches are illustrative; vendor details marked
"(to check)" have not been verified against the vendor's documentation.
Sketches of `app.jadpo` show entries that sit inside `application { }`, and
connections that sit inside its `connections: { }`.

## Summary

**The question.** Richard and Claude agreed in principle that an app changes
Jadpo's built-in functionality in exactly two ways:

1. **Events.** A built-in feature emits events; any number of the app's
   subscribers react after commit and cannot change what happened.
2. **Typed settings.** Each built-in feature publishes a fixed list of
   settings. A setting's value is a plain value, a connection, one of the
   app's own declarations (a message type, a field), or one of the app's
   actions or functions: a **decision point**, with a fixed signature and
   one handler, written where the feature is configured.

No hooks that intercept built-in steps; no override by naming a file the
same as a built-in one; some things deliberately cannot be overridden; every
backend is a named connection. This page tests that on eighteen scenarios,
each with two to four real providers.

**It holds.** Every scenario is met by a setting, an event, or both, and
nothing needed a hook. Three findings sharpen the model rather than strain
it:

- **Replacing a generated route or page is not an override.** The app turns
  the generated one off with a setting and writes its own route over the
  same generated operations (`User.export`, `sessions.start`,
  `flags.for_caller`). That is ordinary use of a built-in feature, so the
  model needs no third mechanism, only the rule that every generated route
  sits over operations the app can also call.
- **Decision points are rare.** The eighteen scenarios need exactly one,
  `on_first_sign_in`, which also answers "who may self-register". What a
  privacy export includes is a declaration on the data, not a decision.
- **A built-in feature emits an event only for what it does without the
  app's code** (a provider sign-in, a flag change by an operator, a
  rejected upload). Where the app's own action did the work, the app emits
  its own event. This keeps the event list short and predictable.

**Where it strains, and the smallest fix for each:**

| Strain | Scenario | Smallest fix |
|---|---|---|
| Built-in text is looked up by key, so changing it is override by name | 9 | Keys under a reserved `jadpo.` prefix in the app's translations; an unknown `jadpo.` key is an error. The one by-name case, visible in the key itself |
| Replacing a built-in email needs a typed link between the app's message and the built-in one | 9 | A setting names the app's `EmailMessage` type, which `pick:`s its fields from the built-in message (existing syntax) |
| `PersonErased` is delivered after the person's record, holding the provider's id, is gone | 12 | The connection declares its erasure key (`erasure: { key: User.stripe_customer_id }`), read into the event before erasing |
| Some providers' webhooks need a handshake answer (Slack, Microsoft Graph, SNS), which neither an event nor a setting can give | 10 | Built-in handshake rules beside signature schemes: `handshake: WebhookHandshake.x` in `provider.jadpo` |
| Outgoing webhooks send to hosts customers register, against deny-by-default network | 14 | One runtime-owned exception with fixed guards (HTTPS, no private addresses, no redirects), listed as one entry in the network report |
| Telemetry is the one backend that is not a named connection | 16 | A `Driver.otlp` connection whose settings stay bound to the standard `OTEL_*` names |
| An external search index cannot take policy compiled into SQL | 4 | The index returns ranked ids filtered by tenant and equality conditions; the records are then read through the policy-applied query |
| A store that cannot do the whole job (Cloudflare's rate-limit binding, GuardDuty's scanning) | 1, 8 | Already covered by design C's whole-job rule: a compile error naming what it cannot do and the alternative |

**Feature flags: not a capability.** The language performs the flag job
itself, but no vendor can do all of it: flag definitions would live in two
places, targeting rules the compiler cannot see would decide behaviour, and
each vendor's evaluation needs native SDK code. Keep the database store. If
an app ever must use a vendor, the one route is a `Driver.ofrep` store
(OpenFeature's remote evaluation protocol), evaluation only.

**Audit: yes, through `publishing`.** Each audited change gives a built-in
`AuditEntryRecorded` event, emitted only when something listens, which
`publishing:` sends to Datadog, Splunk or an archive like any other event.
The database copy stays the record of truth.

**Decided items this would change:** packages rule 4 (override by file
name), area 15's email replacement and `PersonErased`, area 16's telemetry
and files, area 11's deny-by-default network and webhook settings, area 8's
sign-in settings, and area 7's APIs. Section 5 lists each, with sixteen
open questions for Richard.

## 1. The model, stated for lookup

| Mechanism | When | Can change what happens? | Where written |
|---|---|---|---|
| Event | After the built-in feature's work commits | No | A subscriber in the reacting domain (area 12) |
| Setting: plain value | Configuration | Yes, within the feature's fixed options | Where the feature is configured |
| Setting: connection | Configuration | Chooses the backend | `connections:` plus the feature's `store:` or `connection:` |
| Setting: the app's declaration | Configuration | Replaces a message or names a field | The feature's setting names it |
| Setting: decision point | During the feature's work | Yes, at one named point | An action (may write) or function (only decides) |

Rules that apply to every built-in feature:

- **Settings live where the thing is**: app-wide in `app.jadpo` (or
  `authentication.jadpo`, `policy.jadpo`, `flags.jadpo` for their areas),
  per entity in its section (`audit:`, `search:`, `caches:`), per field in
  its block (`File` fields), per API or route on it.
- **`store:` names where state is kept** (entities, caches, files, flags,
  rate-limit counters, the outbox, a search index); **`connection:` names
  who does a job for the app** (email, publishing, scanning, bot checks,
  telemetry). Today both appear for similar things (`flags: { store: }`,
  `email: { connection: }`); this states the rule they already follow.
- **A decision point has one handler and a signature the feature
  publishes.** Its failures become the feature's refusal, with their codes.
  The compiler checks the signature where the setting names it.
- **A built-in feature emits an event only for what it does without the
  app's code.**
- **Every generated route sits over generated operations** the app can call
  from its own routes, and has an off switch, so a generated route is
  replaced by switching it off and writing one.
- **Things that cannot be overridden say so in `jadpo check`'s error**, with
  the reason and the alternative (scenario 18).

## 2. The scenarios

Each scenario gives the mechanism, a sketch, what the compiler checks, and
what development and tests do. In `jadpo dev`, capabilities go to their
local drivers and every provider operation is faked by default, from the
package's recordings or generated defaults, unless the connection's
`development: { sandbox: [...] }` lets it through to the sandbox (decided).

### 1. Files: S3, R2, local disk, media services, scanning, variants

**Mechanism.** Settings: a connection named by the field's `store:`, plain
values for size, types and variants. Proposed: a `scanning` connection and
one event, `FileRejected`.

**Stores.** S3 and R2 are the built-in S3 driver; local disk is a proposed
native-only driver.

```text
// app.jadpo
uploads: {                                      // Amazon S3
    driver: Driver.s3
    host: "https://s3.eu-west-2.amazonaws.com"   // (to check) virtual-hosted or path style
    region: "eu-west-2"
    bucket: "acme-uploads"
    credential: {
        key_id: config.aws_key_id
        secret: config.aws_secret
    }
}
avatars: {                                      // Cloudflare R2
    driver: Driver.s3
    host: config.r2_host                        // https://<account>.r2.cloudflarestorage.com
    region: "auto"
    binding: Binding.r2                         // on Workers, reads and writes use the bucket binding (to check)
    bucket: "avatars"
    credential: {
        key_id: config.r2_key_id
        secret: config.r2_secret
    }
}
documents: {                                    // local disk on one native server (proposed)
    driver: Driver.disk
    path: config.documents_path
    signing_key: config.file_link_key
}

// domains/accounts/entities/user.jadpo
avatar: File? {
    store: avatars
    max_size: 5mb
    types: [FileType.png, FileType.jpeg, FileType.webp]
    variants: {                                 // proposed: made by the runtime over any store
        thumb: { width: 200  height: 200  fit: Fit.cover }
    }
}

// use, the same whichever store
var slot: UploadSlot = User.avatar_upload()
```

- **Disk** has no outside host to sign links for, so the runtime serves
  them itself: generated `GET /files/{token}` and `PUT /files/uploads/{slot}`
  in the main API, the upload route taking the slot's `max_size` instead of
  the 1 MB body limit. It is refused on Workers by the target rule, and the
  deployable's manifest says the store needs a single server.
- **Variants** are made by the runtime after the file is attached, in a
  generated subscriber, and kept beside the original in the same store. On
  the native target the image work is built into the runtime; on Workers it
  goes through Cloudflare's Images binding (to check). Until a variant is
  ready its link is `none`. No setting chooses an image engine.

**Media services.** Under design C, Cloudinary and Uploadcare are not
`files` connections: a store only stores, and their signed uploads and
delivery would each need a signing scheme in a Jadpo release. The original
stays in S3 or R2, and the media service is a package called from an
action, after an event the app emits where the file is attached:

```text
// app.jadpo
cloudinary: {
    package: Cloudinary.v1_1                    // (to check) version label
    domains: [accounts]
    cloud_name: "acme"
    credential: {
        key_id: config.cloudinary_key
        secret: config.cloudinary_secret
    }
}

// domains/accounts/subscribers/copy_avatar.jadpo
subscriber copy_avatar {
    on: AvatarChanged                           // the app's own event
    run: copy_avatar_to_cloudinary(event.user_id)
}

// domains/accounts/actions/copy_avatar_to_cloudinary.jadpo, the call
var uploaded: Cloudinary.UploadResult = attempt cloudinary.uploads.upload(Cloudinary.Upload {
    file: user.avatar.link(within: 10m)         // proposed: a signed link inside an action; upload from a URL (to check)
    public_id: user.id
})

// display: the package's pure function builds an unsigned delivery URL
var thumb: Url = Cloudinary.urls.image(user.cloudinary_id, Cloudinary.Transformation { width: 200  height: 200  crop: Crop.fill })
```

Uploadcare is the same pattern: its upload-from-URL operation (to check),
and a pure function building CDN operation URLs such as
`https://ucarecdn.com/<uuid>/-/resize/200x200/` (to check).

**Virus scanning** (proposed `scanning` capability). Scanning every upload
before it is used is a job the language would perform itself, and every
scanner does the whole of it: take bytes, answer clean or a threat's name.

```text
// packages/clamav_rest/v1/provider.jadpo
provider {
    protocol: Protocol.multipart_http
    runs: {
        image: "<a ClamAV REST wrapper>@sha256:…"   // (to check) which image
        port: 8080
        asks: ["database.clamav.net"]          // signature updates (to check)
    }
}

// packages/clamav_rest/v1/provides/scanning.jadpo
provide scanning {
    scan: {
        call: files.scan
        input: { file: content }
        clean: response.clean
        threat: response.virus_name
    }
}

// app.jadpo
clamav: {
    package: ClamavRest.v1
    outbound: ["database.clamav.net"]
}
files: {
    scanning: { connection: clamav }
}
```

- A scanning API (Cloudmersive, to check) is the same with fixed `hosts:`
  and a credential. VirusTotal shares submitted files with its community
  (to check), so the personal-data report would list it as receiving every
  uploaded file, which a reviewer should refuse.
- Amazon GuardDuty's malware protection for S3 scans inside AWS and reports
  later through EventBridge (to check). It cannot answer "is this clean?"
  when asked, so it cannot be the scanning connection (design C's whole-job
  rule); an app may still use it through the package's events.
- **The flow is asynchronous**, because a large scan can outlast a route's
  10-second deadline: the upload is attached as `scanning`, outputs leave
  it out until it is clean, a generated subscriber scans it after commit,
  and a threat deletes the bytes, clears the field and emits
  `FileRejected { entity, record_id, field, threat }` for the app to tell
  the person. Files the app writes itself (`store.put`) are not scanned.

**Checks.** `store:` names a connection that fills `files` (the S3 or disk
driver, or a `provides/files.jadpo` mapping); `Driver.disk` is refused on
Workers; `variants:` only on image types; a scanning connection provides
`scanning`; media and scanning connections appear in the network and
personal-data reports.

**Development and tests.** Every store goes to local disk with locally
signed links (decided). The local scanner passes everything except the
EICAR test file, so the rejected path can be walked by hand; tests script
`scanning: [infected]`. Cloudinary calls are faked from recordings and
logged in `jadpo calls`.

### 2. Cache: Redis, Upstash, in memory

**Mechanism.** Settings only: a connection named by `store:` in `caches:` or
`cache:`. No events.

```text
// app.jadpo
cache: {                                        // Redis, self-run or managed
    driver: Driver.redis
    url: config.redis_url                       // RedisUrl, always secret
}
upstash: {                                      // Upstash: the same driver over TLS
    driver: Driver.redis
    url: config.upstash_url                     // rediss://… (to check)
}
local_cache: {                                  // in memory, per instance (proposed in production)
    driver: Driver.memory
    size: 64mb
}

// domains/shop/entities/product.jadpo
caches: {
    records: {
        store: cache
        keep: 1h
    }
}
```

- Upstash also offers a REST API for platforms without TCP (to check). It
  is not needed: Workers can open TCP sockets (decided), so one Redis
  driver serves both, if Upstash accepts those connections (to check).
- In memory in production, each instance clears only its own copies on a
  write, so another instance can serve an entry up to `keep:` old.
  `jadpo check` warns, as it does for in-memory rate limits.

**Checks.** `store:` names a Redis or memory connection; `keep:` at most a
day; sensitive fields left out (decided). **Development and tests:** in
memory whatever the production driver (decided).

### 3. Schedules: in-process timer, Cloudflare cron, an external tick

**Mechanism.** One plain-value setting, decided in area 12. No events: a
failing or silent schedule already raises built-in alerts.

```text
// native server: the default, so nothing need be written
schedules: {
    trigger: ScheduleTrigger.native
}

// Workers: one every-minute cron trigger generated into the Wrangler configuration (to check)
schedules: {
    trigger: ScheduleTrigger.cloudflare_cron
}

// a Kubernetes CronJob or EventBridge Scheduler calls the generated tick
schedules: {
    trigger: ScheduleTrigger.external
    tick_key: config.schedule_tick_key          // proposed: the bearer key the caller sends
}
```

The external tick is a generated `POST /jadpo/tick`, outside every API's
OpenAPI, as health is. The lease stays in the main store, so duplicate
ticks are harmless (decided). **Checks:** `native` refused on Workers;
`external` needs `tick_key:`; a schedule needs a store for its state
(decided). **Development and tests:** the native timer in `jadpo dev`;
`schedule.run()` in tests (decided).

### 4. Search: PostgreSQL built in, or a hosted engine

**Mechanism.** Settings: `store:` on the entity's `search:` section names
the index. Proposed: a `search` capability that packages fill, because
keeping an outside index in step with the entity is a job the language
would then perform itself.

```text
// built in (decided): the entity's own PostgreSQL store
search: {
    fields: [title, notes]
    language: SearchLanguage.english
}

// a hosted engine (proposed)
search: {
    fields: [title, notes]
    language: SearchLanguage.english
    store: meili
}

// app.jadpo
meili: {                                        // Meilisearch Cloud: the project's own host
    package: Meilisearch.v1
    host: config.meili_url
    credential: config.meili_key                // Authorization: Bearer (to check)
}
typesense: {                                    // Typesense Cloud
    package: Typesense.v1                       // (to check) version label
    host: config.typesense_url
    credential: config.typesense_key            // X-TYPESENSE-API-KEY header (to check)
}
algolia: {
    package: Algolia.v1
    application_id: config.algolia_app_id       // a setting: hosts are per application (to check)
    credential: config.algolia_admin_key
}
elastic: {                                      // Elastic Cloud
    package: Elasticsearch.v8
    host: config.elastic_url
    credential: config.elastic_api_key          // Authorization: ApiKey (to check)
}

// packages/meilisearch/v1/provides/search.jadpo
provide search {
    put: {
        call: documents.add_or_replace
        input: { index: index.name  documents: [document] }
    }
    delete: {
        call: documents.delete
        input: { index: index.name  id: document.id }
    }
    query: {
        call: search.query
        input: { index: index.name  q: query.text  filter: query.filter  limit: query.limit  offset: query.offset }
        ids: { each: response.hits  as: hit.id }
    }
}

// use, unchanged from PostgreSQL search
query search_todos(owner_id: User.id, text: SearchText) -> Page<TodoView> {
    from: Todo
    where: Todo.owner_id == owner_id
    where: Todo matches text
}
```

- **Filters differ per engine** (Meilisearch's filter string, Typesense's
  `filter_by`, Algolia's `filters`, Elasticsearch's query JSON; to check).
  The mapping stays data: the package's `Filter` type takes the standard
  list of equality conditions and its decided `encode:` function writes the
  engine's form.
- **How policy holds.** Policy is compiled into SQL, which an outside index
  cannot run. The index holds the listed fields, the `id`, the tenant
  reference and every field a `where:` or a grant compares for equality
  (the compiler knows which). A query sends the words and those equality
  conditions, receives ranked ids, and reads the records through the
  policy-applied SQL `where id in (...)`, keeping the rank; if policy drops
  some, it asks the index for more, up to a fixed bound.
- **Writes reach the index after commit**, through a generated subscriber,
  so a record just written may take a moment to appear. Erasure and
  deletion remove entries the same way.
- **Engine features** (typo tolerance, synonyms, ranking rules) are the
  engine's own: connection settings or its dashboard, never `search:`,
  which stays the same for every engine.

**Checks.** The `store:` connection provides `search`; a `matches` query on
an outside index may only add equality conditions on indexed fields (a
range or a join is an error naming PostgreSQL search as the alternative);
the engine supports the declared `language:`; the personal-data report
lists the engine for every marked field it indexes.

**Development and tests.** The engine holds data, so it is not a container
Jadpo runs (decided: only stateless services); the local driver is
PostgreSQL full-text search over the development database. Rankings differ
from the engine's, so tests assert which records match, not their order;
the conformance suite runs against the engine's sandbox in scheduled CI.

### 5. Sign-in: built in, external tokens, OpenID Connect, GitHub, Apple, passkeys

**Mechanism.** Settings (methods in `authentication.jadpo`, connections in
`app.jadpo`), one decision point (`on_first_sign_in`) and one event
(`SignedInWithProvider`, emitted by the generated callback, which runs no
app code after the first time).

```text
// app.jadpo
google: {
    driver: Driver.oidc
    issuer: "https://accounts.google.com"
    client_id: config.google_client_id
    credential: config.google_client_secret
}
microsoft: {
    driver: Driver.oidc
    issuer: "https://login.microsoftonline.com/common/v2.0"   // the multi-tenant issuer check differs (to check)
    client_id: config.microsoft_client_id
    credential: config.microsoft_client_secret
}
github: {
    driver: Driver.github                       // OAuth 2.0, not OpenID Connect; verified email from its emails API (to check)
    client_id: config.github_client_id
    credential: config.github_client_secret
}
apple: {
    driver: Driver.apple                        // the client secret is a JWT the runtime signs (to check)
    client_id: config.apple_services_id
    team_id: config.apple_team_id
    key_id: config.apple_key_id
    credential: config.apple_private_key
}

// authentication.jadpo
authentication {
    browser_session: {
        cookie: "acme_session"
    }
    sign_in_providers: {
        connections: [google, microsoft, github, apple]
        on_first_sign_in: link_or_create_account
        signed_in: config.app_url               // where the callback sends the browser
        refused: config.sign_in_refused_url     // with ?code=<failure code>
    }
}
```

**External tokens** (Auth0, Clerk, WorkOS, Amazon Cognito) are one method
with plain values; the vendor's issuer and audience are documented on the
method's page, as vendor settings are for drivers (decided):

```text
// authentication.jadpo
external_token: {
    issuer: config.auth_issuer
    audience: config.auth_audience
    on_first_sign_in: link_or_create_account    // proposed
}
```

| Vendor | Issuer | Audience |
|---|---|---|
| Auth0 | `https://<tenant>.auth0.com/` (to check) | The API's identifier |
| Clerk | The instance's frontend API host (to check) | Session tokens carry no `aud` by default; `azp` instead (to check) |
| WorkOS | `https://api.workos.com/user_management/<client_id>` (to check) | (to check) |
| Cognito | `https://cognito-idp.<region>.amazonaws.com/<pool_id>` | Access tokens carry `client_id`, not `aud` (to check) |

Clerk and Cognito need one more plain value, `audience_claim:` (default
`"aud"`). An unknown subject runs `on_first_sign_in`, in its own
transaction before the request's work, as the callback does for providers;
a vendor's user webhooks (Clerk's and WorkOS's user events, to check) are
the package's own events, for keeping profiles current or disabling users.

**The decision point.** One handler for every provider, with a fixed
signature:

```text
// domains/accounts/actions/link_or_create_account.jadpo
/// Runs once per provider account, the first time it signs in.
action link_or_create_account(profile: SignInProfile) fails SignUpClosed -> User.id {
    // profile.verified_email is set only when the provider says it is verified,
    // so linking by email cannot use an unverified address.
    // find a user by profile.verified_email and return its id;
    // otherwise reject SignUpClosed unless config.sign_up_open,
    // or an Invitation exists for the address;
    // otherwise create the user and return its id.
}
```

`SignInProfile` is built in: `connection` (an enum of the listed
connections, plus `external_token`), `subject`, `verified_email: Email?`,
`name: Text?`. Typing the email this way makes area 8's "link by email
only when verified" a property of the type rather than a rule to remember.
**Who may self-register** is this action's refusal; no separate setting.

**Passkeys** are not a provider and need no override: a later built-in
method (`passkey: { relying_party: config.app_domain }`) with generated
operations an app's actions call, as `sessions.start` is, and WebAuthn's
checks in the runtime (native code in a release). Proposed only, as the
answer to "passwords are out of scope".

**Checks.** Listed connections use `Driver.oidc`, `.github` or `.apple`;
`on_first_sign_in` takes a `SignInProfile` and returns a `User.id`, and its
failures' codes are what `refused` receives; `audience_claim` only on
`external_token`; generated routes appear in OpenAPI, marked generated
(decided).

**Development and tests.** `jadpo dev` fakes every provider: `/start` shows
a local page to pick or type a profile, so nothing leaves the machine and
no client secret is needed; `development: { sandbox: [...] }` sends it to
the real provider. Tests script the profile for a callback
(`fakes: { google.sign_in: [SignInProfile { ... }] }`) and sign external
tokens with a test key.

### 6. Feature flags: built in, LaunchDarkly, Unleash, Statsig, OpenFeature

**Mechanism.** Decided: `flags: { store: main }`, flags declared in
`flags.jadpo`, changed with `jadpo flags`. Proposed: one event,
`FlagChanged { flag, environment, by }`, emitted by `jadpo flags set` in the
transaction that changes the state, for an app's Slack message or audit.

What a vendor would have to do, against what the language does today:

| | Jadpo's own | LaunchDarkly | Unleash | Statsig |
|---|---|---|---|---|
| The list of flags | `flags.jadpo`, typed, with owner and expiry | Its dashboard: a second list | Its dashboard or API | Its console: gates, configs, experiments |
| Targeting | Users, tenants, percentage | Attribute rules on contexts Jadpo never sends (to check) | Strategies and constraints (to check) | Rules on user attributes (to check) |
| Changing state | `jadpo flags set`, logged with who | Its dashboard | Its dashboard | Its console |
| Evaluation | In process, from the store | Server SDK streaming rules for local evaluation (to check) | Server SDK polling (to check) | Server SDK (to check) |
| Both-values tests, expiry warnings, `flags remove --keep` | Yes | Source side only | Source side only | Source side only |

The compiler-owned half (declaration, tests, removal) stays whatever the
store. What moves is the state, and with it a second definition of each
flag, behaviour decided by rules the compiler cannot read, and evaluation
that needs vendor SDK code in a Jadpo release. Statsig's main value,
experiment analysis, is product analytics, which area 16 left out.

**If an app must use a vendor**, the one route is a store driver for
OpenFeature's remote evaluation protocol (OFREP; its stability to check):

```text
// app.jadpo (not recommended now)
flag_service: {
    driver: Driver.ofrep
    host: config.flagd_url                      // flagd, a vendor relay, or a vendor's OFREP endpoint (to check which serve one)
    credential: config.ofrep_key
}
flags: {
    store: flag_service
}
```

Evaluation only: Jadpo sends the user id and tenant id, `jadpo flags set`
and `list` refuse with a pointer to the vendor's tool, and the both-values
check and removal still work from source. That is whole if the job is
defined as "evaluate a declared flag for a caller"; it is a driver, not a
capability, because one protocol covers every vendor that speaks it.

**Checks and tests** as decided: unknown flags are errors, `flags:` in
tests, both values covered.

### 7. Audit trail: the database plus Datadog, Splunk or an archive

**Mechanism.** Decided: `audit:` on the entity, entries in the same
transaction. Proposed: a built-in event per entry, `AuditEntryRecorded`,
which `publishing:` sends like any public event.

```text
// domains/billing/entities/invoice.jadpo (decided)
audit: {
    retention: {
        after: config.audit_retention
    }
}

// app.jadpo (proposed)
datadog_logs: {
    package: DatadogLogs.v2                     // HTTP logs intake, DD-API-KEY header (to check)
    site: "datadoghq.eu"                        // chooses the regional host (to check)
    credential: config.datadog_api_key
}
splunk: {
    package: SplunkHec.v1                       // HTTP Event Collector, "Authorization: Splunk <token>" (to check)
    host: config.splunk_hec_url
    credential: config.splunk_hec_token
}
audit_archive: {
    package: AmazonFirehose.v2015_08_04         // batches records into S3 (to check)
    region: "eu-west-2"
    stream: "acme-audit"
    credential: {
        key_id: config.aws_key_id
        secret: config.aws_secret
    }
}
publishing: {
    splunk: [AuditEntryRecorded]
    audit_archive: [AuditEntryRecorded]
}
```

- The event carries what the stored entry holds: entity, record id, action,
  who (user, service or the application), request id, time, and each
  changed field before and after, personal fields as keyed hashes and
  server-only fields left out (decided for entries).
- **It is emitted only when something listens** (a subscriber or a
  `publishing:` entry), so an app that keeps audit in the database pays no
  outbox row per change.
- An S3 archive goes through Firehose rather than the S3 driver, because one
  object per entry is the wrong shape for an archive; batching needs state
  a publishing target should not hold.
- Entries sent out cannot be erased by Jadpo. They hold ids and keyed
  hashes only, and the personal-data report lists each target, which then
  needs an erasing subscriber or an accepted reason ("Splunk keeps audit
  entries for a year; they hold ids and hashes") (area 15, decided).

**Checks.** `AuditEntryRecorded` may be published only when some entity has
`audit:`; the target provides `publishing`. **Development and tests:**
published copies go to the dev console's log and to `published` in tests
(decided).

### 8. Rate limits: Redis, memory, Cloudflare's binding

**Mechanism.** Settings: decided `rate_limits: { store: }` or memory;
`rate_limit:` per API or route. No events: refusals are metrics.

```text
// app.jadpo
rate_limits: {
    store: cache                                // Redis (decided)
}
rate_limits: {
    memory: 16mb                                // no store: per instance, warned (decided)
}

// Workers, Cloudflare's rate-limiting binding (proposed driver)
edge_limits: {
    driver: Driver.cloudflare_rate_limit        // one binding per distinct limit, generated into the Wrangler configuration (to check)
}
rate_limits: {
    store: edge_limits
}
```

Cloudflare's binding counts only over 10 or 60 seconds, per location and
approximately (to check). The defaults (per minute) fit; a route's
`5 within 1h` does not, so the whole-job rule refuses it:

```text
error: `edge_limits` can't count requests within 1h
 --> domains/sales/routes/demo_requests.jadpo:5:9
  |
5 |         within: 1h
  |         ^^^^^^^^^^ longer than the store can count
  reason: Cloudflare's rate-limiting binding counts over 10 or 60 seconds only, so this limit would never be applied as written.
  next: Count this route in Redis (`rate_limits: { store: cache }`), or set a limit within 60s (agent)
  rule: semantic.rate_limit_store_unsupported
  help: docs/language/16-built-in-services.html#rate-limits
```

**Development and tests:** in memory.

### 9. Built-in emails and messages

**Today's built-in mail is the privacy flow only** (area 15): the
verification link, the export link, erasure scheduled with its cancel
link, and erasure completed. Sign-in links and password resets are not
built in: area 8 keeps passwords out of scope and leaves magic links to the
app, so those messages are the app's own `EmailMessage` types. (The packages
page says Jadpo's own mail includes "sign-in, password resets"; that
disagrees with area 8.) If Jadpo later builds magic links or passkey
recovery, their mail follows the same pattern.

**Mechanism.** Settings: the email connection, an app layout, and per
message the app's own message type. The privacy events stay for adding
reactions (decided).

```text
// app.jadpo
email: {
    connection: postmark
    from: config.mail_sender
    layout: shared.emails.brand                 // proposed: every message's layout, built-in ones included
}
privacy: {
    self_service: {
        email: postmark
        messages: {
            export_ready: accounts.PrivacyExportReady   // proposed: the app's own message
        }
    }
}

// domains/accounts/emails/privacy_export_ready.jadpo, with .html and .txt beside it
/// Our wording for "your data is ready".
type PrivacyExportReady = EmailMessage {
    pick: [PrivacyExportReadyEmail.download_link, PrivacyExportReadyEmail.expires_at]
}
```

The built-in message (`PrivacyExportReadyEmail`) publishes its fields; the
app's type may pick any of them and nothing else, so the runtime can always
fill it. Without the setting, the built-in message is sent.

**Built-in page text** (the generated `/privacy` page) is translations.
Jadpo ships its text for a fixed list of locales; an app changes the
wording, or supplies a locale Jadpo does not ship, with keys under the
reserved `jadpo.` prefix:

```json
{
  "jadpo.privacy.page.heading": "Your data at Acme",
  "jadpo.privacy.page.sent": "If an account exists, we have emailed you a link."
}
```

A `jadpo.` key must be one Jadpo defines, with the same placeholders (an
unknown one is an error naming the nearest); a supported locale with no
built-in text needs every `jadpo.` key the app's features use, and the
error lists them.

**Validation and failure text cannot be overridden on the server**
(decided, area 4 question 12): `message` is fixed English for developers
and logs, and clients translate by `code`. Every built-in code
(`invalid_input`, `rate_limited`, `not_permitted`) is in OpenAPI and
`public-failure-codes.json`, and bad input arrives as `{ field, rule }`, so
a client words it.

**Checks.** The picked fields exist on the built-in message; templates are
typed against the app's type and its translations (decided); health score
warnings apply to built-in mail too. **Development and tests:** built-in
mail lands in the dev inbox like any other, rendered with the app's
template where one is set; `email.sent` holds it in tests.

### 10. Generated routes: paths and response shape

**Mechanism.** Settings: which API a feature's routes join (`api:`), so the
API's prefix, accepted methods and failure format apply, and an off switch
where the feature works without its route. Below the prefix, paths are
fixed, so every Jadpo app answers the same URL for the same feature, which
agents and provider dashboards can rely on.

| Feature | Routes, below the API's prefix | API | Answers |
|---|---|---|---|
| Sign-in providers | `GET /auth/{connection}/start`, `/auth/{connection}/callback` (Apple posts its answer, to check) | `api:` on `sign_in_providers`, default `main` | Redirects; refusal to `refused` with `?code=` |
| Token exchange | `POST /auth/exchange` | Each API accepting service keys | Decided |
| Inbound webhooks | `POST /webhooks/{connection}` | `webhooks` when declared, else `main` | 204 once stored; 401 on a bad signature; handshakes below |
| Flags | `GET /flags` | `flags: { api: }`, default `main` | `{ "flags": { "new_checkout": true } }` (proposed shape) |
| Privacy | `GET /privacy`, `POST /privacy/export`, `POST /privacy/erasure` | `main` | Decided |
| Files on disk | `GET /files/{token}`, `PUT /files/uploads/{slot}` | `main` | Bytes |
| Schedule tick | `POST /jadpo/tick` | None, like health | 204 |

**Replacing one**: `flags: { route: false }` and the app's own
`GET /features { run: flags.for_caller() }` over the generated query; the
same for the privacy routes, whose actions (`User.export`, the request
operations) are already generated.

**Webhook handshakes.** Some providers need an answer that is not "stored,
204": Slack echoes a `url_verification` challenge, Microsoft Graph echoes
`validationToken` as plain text, Amazon SNS confirms by fetching a
`SubscribeURL`, Zoom answers `endpoint.url_validation` with an HMAC (all to
check). Neither an event (it must shape the response) nor a decision point
(it is protocol, not a business choice), it is a fixed rule the runtime
applies, extended by releases as signature schemes are:

```text
// packages/slack/v1/provider.jadpo
provider {
    protocol: Protocol.json_http
    hosts: { live: "https://slack.com/api" }
    credential: CredentialScheme.bearer
    webhooks: {
        check: SignatureScheme.slack            // HMAC over "v0:{timestamp}:{body}" (to check)
        handshake: WebhookHandshake.echo_challenge   // proposed
        event_id: "event_id"
        event_type: "event.type"
    }
}
```

**Checks.** `api:` names a declared API; the generated routes' shapes are
in the API record, so a Jadpo release that changes one shows as a record
diff. **Tests** call generated routes with `request { }` like any route.

### 11. Decision points: first sign-in, self-registration, export contents

- **First sign-in** is the decision point: scenario 5's
  `on_first_sign_in`, an action because it may create a user.
- **Who may self-register** is that action refusing; no second setting.
- **What a privacy export includes** is not a decision. `User.export`
  returns everything that belongs to the person, because a partial export
  is worse than none (decided). Two needs remain. Data held by providers:
  the export names them, from the report (decided), and the person asks
  them. A field the law lets an app leave out (another person's data,
  GDPR Article 15(4)): a declaration on the field,
  `export: Export.left_out`, needing an acceptance, built only when an app
  asks (proposed).

A decision point's signature is checked where the setting names it:

```text
error: `on_first_sign_in` must take a `SignInProfile` and return a `User.id`
 --> authentication.jadpo:7:27
  |
7 |         on_first_sign_in: create_member
  |                           ^^^^^^^^^^^^^ takes (input: SignUp) -> Member
  reason: The callback hands the action the provider's profile and signs in the user whose id it returns, so the action's signature is fixed by the feature.
  next: Change `create_member` to `(profile: SignInProfile) -> User.id`, or name an action that has that signature (agent)
  rule: semantic.decision_point_signature
  help: docs/language/08-identity.html#semantic-decision-point-signature
```

**Development and tests.** The action is ordinary code, tested by calling
it with a `SignInProfile`, and through the callback with a scripted profile.

### 12. Erasing a person at providers: Stripe, Postmark, Mailchimp

**Mechanism.** The decided event, `PersonErased`, and area 15's
package-level deletion declaration, which a generated subscriber calls.
The strain: the subscriber runs after the person's record, which held the
provider's id, is gone. Fix: the connection names its erasure key, which
the runtime reads into the event before erasing.

```text
// packages/stripe/v2024_06_20/provider.jadpo
provider {
    ...
    erasure: {                                  // proposed key: the deletion declaration
        call: customers.delete
        input: { customer: key }
    }
}

// packages/mailchimp/v3/provider.jadpo
provider {
    ...
    erasure: {
        call: list_members.delete_permanent    // (to check)
        input: {
            list_id: settings.list_id
            subscriber_hash: key                // SubscriberHash encodes an Email: MD5 of the lower-cased address (to check)
        }
    }
}

// app.jadpo
stripe: {
    package: Stripe.v2024_06_20
    domains: [payments]
    credential: config.stripe_secret_key
    erasure: {
        key: User.stripe_customer_id           // proposed: read before the person is erased
    }
}
mailchimp: {
    package: Mailchimp.v3
    domains: [marketing]
    list_id: config.mailchimp_list_id
    credential: config.mailchimp_key
    erasure: {
        key: User.email
    }
}
```

- **Postmark** keeps sent messages for a limited time and then deletes them
  (to check), and its data-removal API uses an account token, which would
  be a second package (to check). The app accepts instead, in area 18's
  form: "Postmark keeps message content for 45 days, then deletes it."
- Stripe keeps records it must by law after a customer is deleted (to
  check); the personal-data report says so for that connection.
- An erasure key holding a marked field (Mailchimp's email) rides in the
  outbox until delivered, then goes with the delivery record; it is hashed
  in logs like any marked field.

**Checks.** Every connection receiving marked fields has `erasure:` with a
key on a person entity, or an accepted reason (decided, now with the key);
the key's type fits the package's key type. **Development and tests:**
`User.erase(id)` then delivery shows the call in `jadpo calls`, faked;
tests read `stripe.customers.delete.requests`.

### 13. Bot protection: Turnstile, hCaptcha, reCAPTCHA

**Mechanism.** Proposed `bot_check` capability: settings (the connection,
and `bot_check: true` on a route). It qualifies once generated public
routes (the privacy page, sign-up through providers) use it, because then
the language performs the check itself. Every provider does the whole job:
take a token and the caller's address, answer pass or fail.

```text
// packages/turnstile/v0/provider.jadpo
provider {
    protocol: Protocol.form_http
    hosts: { live: "https://challenges.cloudflare.com" }   // /turnstile/v0/siteverify (to check)
    credential: {
        scheme: CredentialScheme.form_field    // proposed scheme: the secret goes in the form body
        field: "secret"
    }
}

// packages/turnstile/v0/provides/bot_check.jadpo
provide bot_check {
    verify: {
        call: siteverify.verify
        input: { response: check.token  remoteip: check.address }
        passed: response.success
    }
    widget: {
        script: "https://challenges.cloudflare.com/turnstile/v0/api.js"   // (to check)
        site_key: settings.site_key
    }
}

// app.jadpo
turnstile: {
    package: Turnstile.v0
    site_key: config.turnstile_site_key
    credential: config.turnstile_secret
}
bot_check: {
    connection: turnstile
}

// a route of the app's own
POST / {
    authentication: RouteAuthentication.none
    bot_check: true                             // proposed
    input: RequestDemo
    status: SuccessStatus.no_content
    run: request_demo(input)
}
```

- hCaptcha is the same shape at its own siteverify host (to check).
  reCAPTCHA v3 answers a score (to check): its mapping turns the score into
  pass or fail with a `min_score:` connection setting, so the capability
  stays pass or fail and every provider fills it whole.
- The token travels in a fixed header the language owns; a refusal is the
  built-in `bot_check_failed` (403). Generated public routes and the
  privacy page use the check, and include the widget, whenever `bot_check:`
  is set.

**Checks.** `bot_check: true` needs `bot_check:` in `app.jadpo`; the
connection provides `bot_check`. **Development and tests:** the local
driver passes every token except `fail`, so the refusal can be walked by
hand; tests script `bot_check: [failed]`.

### 14. Outgoing webhooks: Standard Webhooks and Stripe-style signing

**Mechanism.** Settings for a built-in feature (decided "later", area 16
question 8): which events, whose endpoints, which signing scheme. One
event, `WebhookEndpointDisabled`. Endpoints are stored by the language,
with generated operations, as service keys are.

```text
// app.jadpo (proposed)
outgoing_webhooks: {
    events: [ordering.OrderShipped, ordering.OrderCancelled]
    owner: Company                              // an endpoint receives only its own tenant's events
    signing: SignatureScheme.standard_webhooks  // or .stripe, or .hmac_sha256 with a header
    retry: {
        attempts: 15
        within: 3d
    }
}

// domains/integrations/actions/add_endpoint.jadpo, the call
var created: CreatedWebhookEndpoint = WebhookEndpoint.register(input.company_id, input.url, input.events)
// created.secret is shown once, like a service key
```

| Scheme | Headers | Signed content |
|---|---|---|
| `standard_webhooks` (Svix) | `webhook-id`, `webhook-timestamp`, `webhook-signature: v1,<base64>` | `{id}.{timestamp}.{body}`, HMAC-SHA256, secret `whsec_…` (to check) |
| `stripe` | `Stripe-Signature: t=<time>,v1=<hex>` | `{time}.{body}`, HMAC-SHA256 (to check) |
| `hmac_sha256` | A named header, such as GitHub's `X-Hub-Signature-256: sha256=<hex>` | The body (to check) |

- **The strain: customers' hosts.** Area 11 denies every host no connection
  lists; endpoints are hosts customers register. Fix: one exception the
  runtime owns, with fixed guards: HTTPS only, the address resolved and
  refused when private, loopback, link-local or a cloud metadata address,
  no redirects, a short timeout, nothing read back but the status. The
  network report lists "customer endpoints (outgoing webhooks)" as one
  entry; a change to `events:` appears in the approval report.
- It is not a `publishing` connection: the targets are data, filtered by
  tenant, not a fixed connection. It shares the outbox and the retry rules.

**Checks.** Listed events are public and carry a reference to `owner:`'s
entity (otherwise they cannot be routed to a tenant); the personal-data
report lists customer endpoints as receiving the events' marked fields.
**Development and tests:** nothing leaves the machine; deliveries appear in
the dev console with their signed headers; tests read `webhooks.sent`.

### 15. Publishing targets: SQS, Cloudflare Queues, Kafka, EventBridge

**Mechanism.** Settings: decided `publishing:` lists chosen public events
per connection; the target is a package that provides `publishing`.

```text
// app.jadpo
orders_queue: {                                 // Amazon SQS
    package: AmazonSqs.v2012_11_05
    region: "eu-west-2"
    queue_url: config.orders_queue_url
    credential: { key_id: config.aws_key_id  secret: config.aws_secret }
}
analytics_queue: {                              // Cloudflare Queues
    package: CloudflareQueues.v1
    account_id: config.cloudflare_account
    queue_id: config.analytics_queue_id
    credential: config.cloudflare_token
    binding: Binding.queue                      // on Workers, sent through the binding (to check)
}
bus: {                                          // Amazon EventBridge
    package: AmazonEventBridge.v2015_10_07
    region: "eu-west-2"
    event_bus: "acme"
    credential: { key_id: config.aws_key_id  secret: config.aws_secret }
}
kafka: {                                        // Kafka through an HTTP proxy
    package: ConfluentRestProxy.v3              // (to check); Redpanda's HTTP proxy is similar (to check)
    host: config.kafka_proxy_url
    topic: "acme.orders"
    credential: config.kafka_proxy_key
}
publishing: {
    orders_queue: [ordering.OrderPlaced, ordering.OrderShipped]
    analytics_queue: [accounts.UserSignedUp]
    bus: [ordering.OrderShipped]
}
```

- **One envelope for every target**: CloudEvents 1.0 in its JSON form
  (`id`, `source`, `type`, `time`, `data`, and the trace context through its
  tracing extension; to check). The decided envelope (id, type, time,
  traceparent, data) already is this, nearly; using the standard means
  consumers have libraries for it.
- SQS FIFO queues dedupe by `MessageDeduplicationId: envelope.id` (to
  check). EventBridge's `PutEvents` can refuse single entries inside a
  successful response (to check), which the package's `failed_when:`
  decodes into a failure. Kafka's own protocol is binary over TCP, so a
  native `Driver.kafka` would come only in a Jadpo release; until an app
  needs one, a REST proxy package serves.

**Checks.** Listed events are public; the target provides `publishing`;
marked fields in published events reach the personal-data report.
**Development and tests:** a log in the dev console; `published` in tests
(decided).

### 16. Telemetry: Honeycomb, Datadog, Grafana

**Mechanism.** Settings: decided as `config.jadpo` fields bound to the
standard `OTEL_*` names. Proposed: the exporter becomes a named connection,
so its host is in the network report like every other.

```text
// config.jadpo (decided)
config {
    otel_exporter_otlp_endpoint: Url
    otel_exporter_otlp_headers: OtlpHeaders {
        secret: true
    }
    otel_service_name: Text
}

// app.jadpo (proposed)
otlp: {
    driver: Driver.otlp
    url: config.otel_exporter_otlp_endpoint
    headers: config.otel_exporter_otlp_headers
    service_name: config.otel_service_name
}
telemetry: {
    connection: otlp
}
```

| Vendor | Endpoint | Headers |
|---|---|---|
| Honeycomb | `https://api.honeycomb.io` (to check) | `x-honeycomb-team=<key>` (to check) |
| Datadog | The Datadog Agent's OTLP receiver, or Datadog's own OTLP intake (to check) | An API key (to check) |
| Grafana Cloud | The stack's OTLP gateway (to check) | Basic auth, instance id and token (to check) |

Only values change; no package. **Checks:** without `telemetry:` nothing is
sent (decided). **Development:** traces in the dev console (area 17).

### 17. Failure format: Jadpo's envelope or RFC 9457

**Mechanism.** One plain-value setting per API (proposed). The failure's
content is the same; only the wrapping changes.

```text
// app.jadpo
apis: {
    partners: {
        prefix: /partners
        authentication: [AuthenticationMethod.service_key]
        failure_format: FailureFormat.problem_details   // default FailureFormat.jadpo
        problem_types: "https://api.acme.example/problems/"
    }
}
```

```json
{
  "type": "https://api.acme.example/problems/quantity_unavailable",
  "title": "The requested quantity is unavailable.",
  "status": 409,
  "code": "quantity_unavailable",
  "request_id": "req_0b1e…",
  "available": 5
}
```

Served as `application/problem+json`; `title` is the fixed `message`,
`code` and `request_id` and the client fields are extension members, and
bad input lists `problems` with the same `{ field, rule }` entries (RFC 9457
field rules to check). OpenAPI and the API record show the format.
**Checks:** one format per API. **Tests:** `response.code` reads either.

### 18. Things that cannot be overridden

| Thing | Why not | The alternative |
|---|---|---|
| Policy's authority (OPA, Cedar, SpiceDB) | Policy is compiled into every query's `where`, so lists, counts and pages hold only what the caller may see; an outside authority answers one question at a time | Grants on entities; ask the authority as well from one action and `reject` on a no; keep its relationships current from subscribers |
| The event bus (Kafka, SQS as the delivery path) | An event is saved in the action's transaction and delivered by the runtime, so a rolled-back action announces nothing and one trace covers everything | `publishing:` sends chosen public events to the queue |
| Raw HTTP, and hosts no connection lists | Every host the app can reach is reviewed and in the network report | A package in `shared/providers/` with the one operation needed |
| Server-side failure text | `message` is fixed English; codes are the contract | Clients translate by `code` |
| Cookie flags, CSRF and credential formats | The security boundary the language owns (area 8) | None; methods are a fixed list |
| Authentication reading a cache | A cache outage would become a revocation gap (decided) | `revocation_delay:` |
| Generated paths below an API's prefix | One URL per feature in every app | The API's `prefix:`, or switch the route off and write one |
| Idempotency handling and the outbox | The guarantees areas 7 and 12 rest on | Settings (`idempotency: Idempotency.required`, `retry:`) |

```text
error: `authority:` isn't a policy setting
 --> policy.jadpo:3:5
  |
3 |     authority: spicedb
  |     ^^^^^^^^^ policy can't be handed to a connection
  reason: Policy is compiled into the `where` of every query and write, so a list, a count or a page only ever holds what the caller may see, and a write is checked in the same statement. An outside authority answers one question at a time and can't filter a query.
  next: Keep the grants on the entities. To ask SpiceDB as well about one action, call `spicedb.permissions.check(...)` in that action and `reject` on a no (agent)
  rule: policy.authority_external
  help: docs/language/09-policy.html#policy-authority-external
```

```text
error: events can't travel through `kafka`
 --> app.jadpo:14:9
   |
14 |         connection: kafka
   |         ^^^^^^^^^^ events are delivered by the runtime
  reason: An event is saved in the same transaction as the action that emits it and delivered by the runtime from that store, so a rolled-back action announces nothing and one trace covers the request and everything it set off.
  next: Write `events: { store: main }`, and send chosen public events to Kafka with `publishing: { kafka: [ordering.OrderPlaced] }` (agent)
  rule: semantic.events_store_invalid
  help: docs/language/12-jobs-and-events.html#semantic-events-store-invalid
```

```text
error: `http` isn't a name here
 --> domains/shipping/actions/track_parcel.jadpo:4:30
  |
4 |     var page: Text = attempt http.get("https://api.example.com/track")
  |                              ^^^^ there is no raw network access
  reason: Jadpo code reaches the network only through connections declared in `app.jadpo`, so every host the app can call is reviewed and listed in the network report.
  next: Describe the API as a package in `shared/providers/example_tracking/v1/` with the one operation you need, connect it in `app.jadpo`, and call it as `tracking.parcels.get(...)` (agent)
  rule: semantic.network_access
  help: docs/language/11-integrations.html#semantic-network-access
```

Rule names in these three are proposed.

## 3. Catalogue: every built-in feature's settings and events

Kinds: **value** (a literal or `config.x`), **connection**, **declaration**
(the app's type or field), **action** or **function** (a decision point).
"Decided" marks what the area pages already say; everything else is
proposed here.

### Identity (`authentication.jadpo`, `app.jadpo`)

| Setting | Kind | State |
|---|---|---|
| `application { revocation_delay: }` | value | Decided |
| `principal { user: { subject: active: } service: { ... } }` | declaration (field, condition) | Decided |
| `browser_session: { cookie: signing_key: origin: }` | value | Decided |
| `api_token: { signing_key: }`, `service_key: { signing_key: }` | value | Decided |
| `external_token: { issuer: audience: }` | value | Decided |
| `external_token: { audience_claim: }` | value | Proposed |
| `external_token: { on_first_sign_in: }` | action | Proposed |
| `sign_in_providers: { connections: }` | connection list | Decided in principle |
| `sign_in_providers: { on_first_sign_in: }` | action | Decided |
| `sign_in_providers: { signed_in: refused: api: }` | value | Proposed |
| `oauth_provider: { scopes: }` | value | Decided |
| `apis: { <name>: { authentication: } }` | value | Decided |

Events: `SignedInWithProvider { user_id, connection }` (proposed). Decision
points: `on_first_sign_in(profile: SignInProfile) -> User.id`.

### Personal data and privacy (`app.jadpo`, entities)

| Setting | Kind | State |
|---|---|---|
| `personal:`, `person:`, `erase:`, `retention:` on entities and fields | value | Decided |
| `privacy: { self_service: { email: } }` | connection | Decided |
| `privacy: { exports: { store: keep: } }` | connection, value | Decided |
| Cooling-off period | value | Decided (name to choose) |
| Page branding (name, logo, colour) | value | Decided ("from configuration") |
| `privacy: { self_service: { messages: { verify:, export_ready:, erasure_scheduled:, erasure_completed: } } }` | declaration (the app's `EmailMessage`) | Proposed |
| `erasure: { key: }` on a connection | declaration (field) | Proposed |
| `export: Export.left_out` on a field | value | Proposed, when asked |

Events (decided): `PrivacyRequestReceived`, `PrivacyExportReady`,
`PrivacyErasureScheduled`, `PrivacyErasureCompleted`,
`PrivacyRequestOverdue`, `PersonErased` (now carrying erasure keys).

### Email

| Setting | Kind | State |
|---|---|---|
| `email: { connection: from: }` | connection, value | Decided |
| `email: { layout: }` | declaration (the app's layout) | Proposed |

Events: none from the capability (bounces are the package's own events,
decided).

### Files

| Setting | Kind | State |
|---|---|---|
| `File` field `store:` | connection | Decided |
| `max_size:`, `types:`, `personal:`, `retention:` | value | Decided |
| `variants:` | value | Proposed |
| `files: { scanning: { connection: } }` | connection | Proposed |

Events: `FileRejected { entity, record_id, field, threat }` (proposed).
Generated: `Entity.field_upload()`, `store.put(bytes, type)` (decided),
`file.link(within:)` (proposed).

### Flags (`flags.jadpo`, `app.jadpo`)

| Setting | Kind | State |
|---|---|---|
| `flags: { store: }` | connection | Decided |
| Per flag: type, `default:`, `owner:`, `expires:`, `client:` | value | Decided |
| `flags: { api: route: }` | value | Proposed |

Events: `FlagChanged { flag, environment, by }` (proposed). Generated:
`flags.for_caller()` (proposed), `GET /flags` (decided).

### Rate limits

| Setting | Kind | State |
|---|---|---|
| `rate_limits: { store: }` or `{ memory: }` | connection, value | Decided |
| `rate_limit: { requests: within: by: }` on an API or route | value | Decided |

Events: none.

### Audit

| Setting | Kind | State |
|---|---|---|
| `audit: { retention: { after: } }` on an entity | value | Decided |

Events: `AuditEntryRecorded` (proposed; emitted only when something
listens). Generated: `Entity.history(id)` (decided).

### Search

| Setting | Kind | State |
|---|---|---|
| `search: { fields: language: }` on an entity | value | Decided |
| `search: { store: }` | connection | Proposed |

Events: none.

### Caches

| Setting | Kind | State |
|---|---|---|
| `caches: { records: { store: keep: } }`, query `cache: { store: keep: }` | connection, value | Decided |

Events: none.

### Events, schedules, publishing, outgoing webhooks

| Setting | Kind | State |
|---|---|---|
| `events: { store: }` | connection | Decided |
| Subscriber `retry: { attempts: within: }` | value | Decided |
| `schedules: { trigger: }` | value | Decided |
| `schedules: { tick_key: }` | value | Proposed |
| Schedule `every:`, `run:` | value, action | Decided |
| `publishing: { <connection>: [events] }` | connection, events | Decided |
| `outgoing_webhooks: { events: owner: signing: retry: }` | value, declaration | Proposed |

Events: `WebhookEndpointDisabled { endpoint_id, owner }` (proposed).
Generated: `WebhookEndpoint.register`, `.list`, `.remove`, `.rotate_secret`
(proposed).

### HTTP and inbound webhooks

| Setting | Kind | State |
|---|---|---|
| `apis: { <name>: { prefix: authentication: idempotency: rate_limit: } }` | value | Decided |
| `apis: { <name>: { failure_format: problem_types: } }` | value | Proposed |
| Route `deadline:`, `settle:`, `status:`, `authentication:` | value, query | Decided |
| Route `bot_check:` | value | Proposed |
| Connection `webhooks: { secret: }` | value | Decided |
| Package `webhooks: { handshake: }` | value | Proposed |
| `bot_check: { connection: }` | connection | Proposed |

Events: the provider package's own webhook events (decided).

### Telemetry, connections and development

| Setting | Kind | State |
|---|---|---|
| `OTEL_*` settings in `config.jadpo` | value | Decided |
| `telemetry: { connection: }` with `Driver.otlp` | connection | Proposed |
| Monitoring thresholds per domain | value | Decided (proposed in area 16) |
| Connection `timeout:`, `retry:`, `environment:`, `domains:`, `outbound:` | value | Decided |
| Connection `development: { sandbox: }` | value | Decided |
| `locales: { default: supported: }` | value | Decided |
| `jadpo.` keys in `translations/` | value | Proposed |

## 4. Capabilities the scenarios imply

The rule (design C, decided): a capability exists only for a job the
language performs itself, and every connection that fills it does the whole
of it.

| Capability | The language's own job | Whole? | Filled by | Verdict |
|---|---|---|---|---|
| `database` | Entities, queries, the outbox | Yes | PostgreSQL | Keep (decided) |
| `cache` | `caches:`, invalidation | Yes | Redis, memory | Keep (decided) |
| `files` | Upload slots, links, deletion, export | Yes for S3 and disk; not media services | S3, disk (proposed), packages | Keep; media services stay packages |
| `email` | Rendered `EmailMessage` after commit, privacy mail | Yes | SMTP, packages | Keep (decided) |
| `publishing` | Chosen events forwarded after commit with trace context | Yes | SQS, Queues, EventBridge, Kafka proxy, Firehose | Keep; outgoing webhooks are not one of its drivers |
| `schedules` | Ticks for `schedule` declarations | Yes | Built-in triggers only | Keep (decided) |
| Sign-in | Generated start and callback routes | Yes | `Driver.oidc`, `.github`, `.apple` only | A drivers-only connection kind, like `cache`; no packages |
| `search` (new) | Index kept in step; `matches` | Yes, defined as index, delete, query words with equality filters | Packages | Add when an app needs an outside index |
| `bot_check` (new) | Checking generated public routes | Yes, defined as pass or fail | Packages | Add with the generated privacy page |
| `scanning` (new) | Refusing infected uploads | Yes, defined as bytes in, verdict out; GuardDuty cannot fill it | Packages (`runs:` or an API) | Add when scanning is built |
| Flags | Evaluating declared flags | No vendor does the whole job | — | Not a capability; a `Driver.ofrep` store only if asked |
| Audit sink | — | — | `publishing` | Not a capability: an event |
| Telemetry | Exporting traces and metrics | One protocol | `Driver.otlp` | A driver, not a capability |
| Rate-limit counters | Counting per route | Redis yes; Cloudflare's binding only short periods | Redis, memory, binding | Store drivers, like `cache`; the whole-job error refuses what a store cannot count |

**Flags (recommendation): no capability.** The compiler-owned part of flags
(declaration, both-values tests, expiry, removal) is the reason they are
built in, and it does not move. Moving the state to a vendor adds a second
definition of every flag and targeting rules the compiler cannot see, and
each vendor's evaluation needs native SDK code in a release. If an app ever
needs a vendor, `Driver.ofrep` as the flags store, evaluation only, is the
one way; it waits for that app.

**Audit (recommendation): yes, through `publishing`, as an event.**
`AuditEntryRecorded` per entry, emitted only when a subscriber or a
`publishing:` entry listens. No audit-specific sink, no capability: the
event uses the mechanism that already sends events out at least once with
their trace context, and the database stays the record of truth that
`Entity.history` reads and erasure reaches. Area 15's access records
(exports, erasures, sensitive reads) stay log events, as decided.

## 5. What this would change, and questions for Richard

### Decided items this would change

| Where | Decided | Change |
|---|---|---|
| [Packages](packages-and-extension-proposals.md), rule 4 (proposed there) | "Override a template or translation by writing its own file under the same name" | A setting names the app's message; only translations override by key, under `jadpo.` |
| Area 15, question 13 | "The built-in emails are subscribers to these; an app can … replace the emails" | Replaced by `messages:` naming the app's types; events stay for adding reactions |
| Area 15, question 15 | `PersonErased`; packages declare a deletion operation | The connection names its erasure key; the event carries it; the declaration is `erasure:` in `provider.jadpo` |
| Area 16, tracing | `OTEL_*` settings in `config.jadpo` | Also a `Driver.otlp` connection named by `telemetry:` |
| Area 16, question 7 | Files: S3 protocol only; scanning and resizing wait | `Driver.disk` on one native server; variants made by the runtime; `scanning` capability |
| Area 16, question 12 | Search services as packages | `store:` on `search:` naming an engine that provides `search` |
| Area 16, email | A shared layout | `layout:` on `email:` |
| Area 11, question 9 | Deny every host no connection lists | One runtime-owned exception for outgoing webhooks |
| Area 11, carried to 16 | "Outgoing webhooks as a built-in publishing driver" | Their own feature on the same outbox |
| Area 11, question 11 | Webhook `check:`, `event_id:`, `event_type:` | Adds `handshake:` |
| Area 8, question 11 | `sign_in_providers:` with presets; first sign-in receives the profile | Lists connections; `signed_in:`, `refused:`, `api:`; `SignInProfile.verified_email`; `external_token` gains `on_first_sign_in` and `audience_claim` |
| Area 7, APIs | One envelope | `failure_format:` per API |
| [Packages](packages-and-extension-proposals.md), "Every extension point" | Search and bot protection capabilities "once the language uses them" | Both proposed, with the trigger named; flags stay a store |

One inconsistency to fix either way: the packages page lists "sign-in,
password resets" as Jadpo's own mail, but area 8 keeps passwords out of
scope and leaves magic links to the app. Today only the privacy mail is
built in.

### Open questions

1. **Adopt the two-mechanism model, with generated operations as how a
   generated route or page is replaced?** *Recommendation:* yes. Every
   scenario fitted; replacing a route is ordinary use, not a third
   mechanism.
2. **Is "a built-in feature emits an event only for what it does without
   the app's code" the rule for which events exist?** *Recommendation:*
   yes; it keeps the list to the handful in section 3.
3. **Do the setting keys follow "`store:` for where state is kept,
   `connection:` for who does a job"?** *Recommendation:* yes; the decided
   settings already do, and it tells an agent which word to write.
4. **Built-in messages: the app's `EmailMessage` type `pick:`s from the
   built-in message, named by a setting?** *Recommendation:* yes; no new
   syntax, and the compiler can always fill the app's type.
5. **Built-in text: keys under a reserved `jadpo.` prefix in the app's
   translations?** *Recommendation:* yes; translation is lookup by key
   anyway, and the prefix makes the override visible where it is written.
6. **Erasure keys: `erasure: { key: User.stripe_customer_id }` on the
   connection, read into `PersonErased` before erasing?** *Recommendation:*
   yes; without it the generated subscriber has nothing to delete by.
7. **Feature flags as a capability?** *Recommendation:* no. Keep the
   database store; `Driver.ofrep`, evaluation only, if an app ever must
   use a vendor.
8. **Audit entries out through `publishing`?** *Recommendation:* yes, as a
   built-in `AuditEntryRecorded` event emitted only when something
   listens; no audit sink.
9. **`on_first_sign_in` on `external_token` too, run in its own
   transaction before the request's work?** *Recommendation:* yes; one
   decision point for every way a new user arrives, and it answers "who may
   self-register" for both.
10. **`SignInProfile.verified_email`, set only when the provider says it is
    verified?** *Recommendation:* yes; the decided linking rule becomes a
    property of the type.
11. **Webhook handshakes as built-in rules (`handshake:`), extended by
    releases?** *Recommendation:* yes, built with the first package that
    needs one (Slack or SNS).
12. **Outgoing webhooks: their own feature with a runtime-owned network
    exception and fixed guards?** *Recommendation:* yes; customer endpoints
    cannot be connections, and a fixed exception is reviewable where a
    setting would not be.
13. **Telemetry as a `Driver.otlp` connection?** *Recommendation:* yes; it
    keeps "every backend is a named connection" true and puts the host in
    the network report, with the standard variable names unchanged.
14. **`failure_format: FailureFormat.problem_details` per API?**
    *Recommendation:* yes; the content is identical, and an organisation
    whose API standard requires RFC 9457 otherwise cannot adopt Jadpo for a
    public API.
15. **`search`, `bot_check` and `scanning` as capabilities, each added
    when its trigger arrives (an outside index, the generated privacy page,
    upload scanning)?** *Recommendation:* yes; each passes the
    whole-job rule as defined in section 4. Scanning is asynchronous, with
    a `scanning` state on the file.
16. **`Driver.disk` for files on one native server, and `Driver.memory`
    for caches in production with a warning?** *Recommendation:* yes for
    both; refused on Workers by the target rule, and the deployable's
    manifest says a disk store needs a single server.
