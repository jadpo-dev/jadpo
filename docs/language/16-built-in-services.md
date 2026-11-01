---
title: Built-in services
area: 16
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Built-in services
<!-- topic: built-in-services -->

Jadpo gives your app some common backend features itself, so you don't
install a library or write your own: feature flags, rate limits, files,
an audit trail, telemetry, email, documents, search and bot protection.
Each is part of Jadpo's runtime and runs inside your app's process, never
as a separate container. Some reach an outside service, such as a mail
provider, through a connection you declare in `app.jadpo`.
[How packages, connections, capabilities and services fit together](11-integrations.md#how-packages-connections-capabilities-and-services-fit-together)
says how the pieces relate.

A feature is built in only when the compiler gains something by knowing
about it: a mistake becomes a compile error, a test or a report is
generated, policy or erasure reaches it, or the runtime does the work
with nothing for you to write. Saving a library install isn't enough.
The table lists each feature, why it is built in, and where you set it
up. [What is built](#what-is-built) says which of them work today.

| Feature | Native? | Why | Where it lives |
|---|---|---|---|
| Feature flags | Yes | Misspelt flags are errors; both branches tested; expired flags reported; one-step removal | Root `flags.jadpo`; read as `flags.name`; state in the app's database, not a capability |
| Rate limits | Yes | The route knows the caller; 429 is already a failure kind; limits appear in OpenAPI | `rate_limit:` on an API or route; counted in the cache store, or Cloudflare's binding on Workers |
| Files | Yes, through the `files` capability | Size and type checked; policy and erasure reach the bytes; signed links generated | `File` fields; a `files` connection in `app.jadpo`: the S3 driver, or the disk driver on native |
| Audit trail | Yes | The compiler sees every write and its caller; personal fields hashed | `audit:` section on an entity; `AuditEntryRecorded` when something listens |
| Telemetry | Yes, with one settings section | Every route, query, action, job and provider call is known | `telemetry:` in `application { }`; custom metrics in a domain's `monitoring.jadpo` |
| Path IDs and monitoring thresholds | Yes | Every path is known, so it can be named, sampled and given limits | Generated runtime; a monitoring file per domain |
| Caches | Yes ([Caches](05-data.md#caches)) | What may be cached, and what never is, is the language's | Redis-compatible connections; [Caches: drivers and what they hold](#caches-drivers-and-what-they-hold) |
| Email | Yes, through the `email` capability | Templates typed against translations; delivered after commit; any provider without code changes | `type X = EmailMessage` in `emails/`; an `email` connection in `app.jadpo` |
| Documents | Yes, through the `documents` capability | Templates typed and translated as email's; rendered in subscribers | `type X = Document` in `documents/`; a `documents:` connection in `app.jadpo` |
| Search | Yes, through the `search` capability | Index, query and migration generated from one declaration; results always pass policy | `search:` section on an entity; PostgreSQL unless it names an engine |
| Bot protection | Yes, through the `bot_check` capability | Jadpo's own public endpoints need it; the token never enters a route's input | `bot_check:` in `application { }`, `bot_check: true` on a route or feature |
| Outgoing webhooks | Yes | Payloads typed from events; signing and retries generated; one guarded network exception | Later, through `publishing` |
| Field encryption | Yes | Encrypted fields kept out of logs, indexes and filters | [Encrypted fields](15-personal-data.md#encrypted-fields) |
| Realtime, admin screens, payments, CMS, analytics, quotas, media transformations | No, or later | The compiler gains little, or the feature is not a backend's | Not built in; media services (Cloudinary, Uploadcare) are ordinary packages |

## Capabilities the built-in services use
<!-- topic: built-in-service-capabilities (was capabilities-on-this-page) -->

A capability is a job Jadpo does through an outside service, such as
sending email, that any provider can fill. You choose the provider by
naming a connection in `app.jadpo`, and your code stays the same whichever
you choose ([Capabilities](11-integrations.md#capabilities)). Each
capability the built-in services use, and the whole of its job:

| Capability | The whole job | Filled by |
|---|---|---|
| `files` | Put, read, delete, signed links and upload slots, multipart ones included | The S3 driver (any S3-compatible service); the disk driver, native only |
| `email` | Send a Jadpo-rendered `EmailMessage` after commit, attachments included | The SMTP driver; a mail package (Postmark, Amazon SES, Resend) |
| `documents` | Turn the HTML Jadpo renders into a PDF | A container package (Gotenberg) or a hosted API's package (DocRaptor, PDFShift) |
| `search` | Upsert, remove, and query by words with equality filters, returning ranked ids | PostgreSQL full-text search, built in; an engine's package |
| `bot_check` | Check a token: pass or fail | A package (Turnstile, hCaptcha, reCAPTCHA) |
| `scanning` | Scan an uploaded file: clean or infected | A ClamAV container package or a scanning API's package |
| `publishing` | Deliver chosen events outward, and outgoing webhooks | Drivers and packages ([Where events travel](12-jobs-and-events.md#where-events-travel)) |
| `receiving` | Fetch messages from a queue, the inbound twin of `publishing` | Packages; a built-in driver for Cloudflare Queues on Workers ([Where events travel](12-jobs-and-events.md#where-events-travel)) |

[Capabilities](11-integrations.md#capabilities) lists the others, such as
the database, the cache, schedules and `ai`, and what isn't a capability:
SMS, event delivery, and telemetry, which has its own `telemetry:`
section ([Telemetry](#telemetry)). Each capability has a local driver for
development, one fake for tests
([Provider calls and capabilities in tests](13-testing.md#provider-calls-and-capabilities-in-tests))
and a conformance suite that a package filling it must pass.

## Feature flags
<!-- topic: feature-flags -->

A feature flag lets you ship code switched off, then turn it on for some
users, a share of users, or everyone, without a deploy. You declare every
flag once, in the file `flags.jadpo` at the root of the project, with its
type, its default, its owner and the date it expires. The `///` comment
above a flag is its description, as for every declaration.

```jadpo check project=feature-flags file=flags.jadpo
use ordering/types { CheckoutLayout }

flags {
    /// The one-page checkout replacing the three-step form.
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
    }

    /// Which layout the checkout uses while we compare two.
    checkout_layout: CheckoutLayout {
        default: CheckoutLayout.classic
        owner: "payments"
        expires: "2099-12-15"
    }
}
```

```jadpo check project=feature-flags file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=feature-flags file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=feature-flags file=domains/ordering/types/checkout_layout.jadpo hidden
/// How the checkout's form is laid out.
type CheckoutLayout = Enum {
    classic
    single_column
}
```

```jadpo check project=feature-flags file=domains/ordering/types/checkout_steps.jadpo hidden
/// How many pages the checkout takes.
type CheckoutSteps = Int {
    min: 1
    max: 3
}
```

```jadpo check project=feature-flags file=domains/ordering/types/checkout_shown.jadpo hidden
use ordering/types { CheckoutLayout, CheckoutSteps }

/// The checkout a customer is shown.
type CheckoutShown = Output {
    page_count: CheckoutSteps
    arrangement: CheckoutLayout
}
```

A flag is a `Bool`, or an enum when it has more than two values, whose
variants carry no data. Code reads it as `flags.new_checkout`, in actions
and in a route's `run:` arguments. Functions and queries can't read a
flag (`flag.read_context`), so you pass them its value as a parameter. As
with `clock.now`, the value is fixed for the whole operation, so one
request never sees a flag change halfway through.

```jadpo check project=feature-flags file=domains/ordering/actions/start_checkout.jadpo migrate=initial
use ordering/types { CheckoutShown }

/// The checkout this customer is shown.
action start_checkout() -> CheckoutShown {
    if (flags.new_checkout) {
        return CheckoutShown {
            page_count: 1
            arrangement: flags.checkout_layout
        }
    }
    return CheckoutShown {
        page_count: 3
        arrangement: flags.checkout_layout
    }
}
```

### Changing a flag

A flag's current state lives in your app's own database, in a table Jadpo
owns, `jadpo_flags`, in the database `application { flags: { store: } }`
names, or the only one there is. With several databases and none named,
the flags are `flag.store_required`. You change it while the app runs,
with `jadpo flags` against an environment
([Commands for services, calls, email and stored data](14-tooling.md#commands-for-services-calls-email-and-stored-data)).
Each change is logged with who made it.

Each server holds the flag values in its own memory and refreshes them
every few seconds, so a change applies within ten seconds. Flags are
never held in the cache. If the database can't be read, each server
keeps the last values it had, or the defaults, so reading a flag never
fails a request.

Flags aren't a capability, and no outside flag service fills them.

```text
jadpo flags set new_checkout --env production --users 0192... --percent 10
jadpo flags set checkout_layout --env production --split classic=50,single_column=50
jadpo flags list --env production
```

### Who gets which value

The first rule that matches wins:

1. Named users get a fixed value.
2. Named tenants get a fixed value. This rule comes once tenancy is
   settled for flags ([Tenancy](09-policy.md#tenancy)).
3. A percentage of users, chosen by a stable hash of the flag's name and
   the user's `id`. The same user always gets the same answer, raising 10%
   to 20% keeps the first 10% in, and each flag picks a different 10%.
4. Everyone else gets the flag's current value, at first its `default`.

The runtime knows the caller, as it does for policy, so you don't pass
it. Schedules and subscribers have no caller
([Background work](09-policy.md#background-work)), so they get the
everyone value. An action that needs a user's variant later puts the
value in the event it emits.

### What the compiler checks

- `flags.new_chekout` is a compile error, `flag.unknown`, like any
  unknown name.
- Every flag read needs a test for each value. A test chooses the value
  with `flags: { new_checkout: true }`, beside `clock:` and `config:`
  ([Test settings](13-testing.md#test-settings)), for every caller in
  the test; a flag it doesn't name has its `default:`. `jadpo check`
  fails until the tests that reach each read cover both `true` and
  `false`, or every variant.

```jadpo check project=feature-flags file=domains/ordering/actions/start_checkout.test.jadpo
use ordering/types { CheckoutLayout, CheckoutShown }

test "the three-step form is shown by default" {
    var checkout_opened: CheckoutShown = start_checkout()
    assert checkout_opened.page_count == 3
    assert checkout_opened.arrangement == CheckoutLayout.classic
}

test "the new checkout takes one page, in the layout the test gives" {
    flags: {
        new_checkout: true
        checkout_layout: CheckoutLayout.single_column
    }
    var checkout_opened: CheckoutShown = start_checkout()
    assert checkout_opened.page_count == 1
    assert checkout_opened.arrangement == CheckoutLayout.single_column
}
```

- A flag past its `expires:` date is an error naming its owner,
  `flag.expired`, and you can't decline it. `jadpo check` judges the date against today, and
  `jadpo build` against the commit's date, so rebuilding a merged
  revision is never blocked by a date passing. To keep a flag longer you
  edit the date, so keeping one is a reviewed choice.
- Whether a flag has sat at 100% or 0% is runtime state the compiler
  can't see, so `jadpo flags list` marks those flags "ready to remove".

### Removing a flag

`jadpo flags remove new_checkout --keep true` rewrites your source: each
read becomes `true`, the branch that can no longer run goes, the
declaration goes, and test overrides for the removed value are deleted.
Jadpo can do this exactly because it knows every read.

### Flags in browsers and apps

A flag with `client: true` is also returned, worked out for the caller,
by a generated `GET /flags` route in the OpenAPI document.

### What flags are not for

Flags are for releasing code. What a customer's plan includes is your
app's data, guarded by policy. A setting that differs between
environments is configuration
([Settings: `config.jadpo`](10-configuration-and-time.md#settings-configjadpo)).

## Rate limits
<!-- topic: rate-limits -->

A rate limit caps how many requests a caller may make in a period. Every
route has limits by default, so you write nothing to get them: 600 a
minute for each signed-in caller, and 120 a minute for each address on
routes open to anyone. A refused request gets the built-in
`rate_limited` failure: a 429 with a `Retry-After` header
([Rate-limited requests](07-http.md#rate-limited-requests)).

You change a limit with `rate_limit:`, on an API in `app.jadpo` (the
default for its routes) or on one route. Here a public form allows five
requests an hour from each address:

```jadpo fragment
// domains/sales/routes/demo_requests.jadpo
POST / {
    authentication: RouteAuthentication.none
    rate_limit: {
        requests: 5
        within: 1h
        by: RateLimitKey.ip
    }
    input: RequestDemo
    status: SuccessStatus.no_content
    run: request_demo(input)
}
```

### Who is counted: `by:`

`by:` says what a limit counts requests by:

- `RateLimitKey.caller`, the default, counts each signed-in caller
  separately, or each address when the caller isn't signed in.
- `RateLimitKey.ip` counts each address.

The address is the client's address as your app finds it
([Client addresses](10-configuration-and-time.md#client-addresses)). By
default that is the address the connection comes from. If your app runs
behind a proxy or load balancer of your own, you name the header that
proxy sets, so the limits count each client, not the balancer. An IPv4 client is counted by its
address. An IPv6 client is counted by its /64, the smallest block a
network hands one subscriber, so moving to another address in its own
block doesn't get it a fresh count.

The magic-link routes Jadpo generates
([Magic links](08-identity.md#magic-links)) are limited by address
without you writing anything.

### Where the counts are kept: `rate_limits: { store: }`

With no store named, each server counts in its own memory. To share the
counts between servers, name a Redis connection as the store, with
`rate_limits: { store: cache }` in `app.jadpo`'s `application { }`
block. If the store can't be reached, each server counts in its own
memory, so limits stay roughly right and no request fails. Caches follow
the same rule ([Caches](05-data.md#caches)).

The store must be a connection with `driver: Driver.redis` or
`driver: Driver.cloudflare_rate_limits`; anything else is a compile
error (`rate_limits.store_invalid`).

The same store also holds provider rate limits, a package's limits on
calls to its own API ([Calling](11-integrations.md#calling)). Your
servers then share one count of calls to a provider, rather than each
spending the whole allowance.

On Cloudflare Workers, a Worker's instances share no memory, so a
deployable built for Workers that serves an API must name a store, or
the compiler refuses it (`rate_limits.store_required`). The next section
shows the store Workers uses.

### What a client sees

A refused request gets a `429` with `Retry-After` and the standard
`RateLimit` headers, its wording from translations like any failure's
([Rate-limited requests](07-http.md#rate-limited-requests)). Each limit
also appears in its route's OpenAPI entry and in `jadpo routes`.

### Counting on Workers

An edge deployable built for Workers counts its rate limits in
Cloudflare's binding:

```jadpo check project=rate-limits-on-workers file=app.jadpo
application {
    connections: {
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 1001
        }
    }
    rate_limits: {
        store: limits
    }
    deployables: {
        edge: {
            serves: [main]
            target: Target.wasm
        }
    }
}
```

A connection with `driver: Driver.cloudflare_rate_limits` runs only on
Workers. Its `namespace:` is the first of the namespace ids in your
Cloudflare account that its bindings take. A binding counts one limit
over one window, so each limit your app counts there is a binding of its
own: `LIMITS_120_PER_60S` counts 120 a minute. The bindings are numbered
from `namespace:`, in order of window and then of requests, and
`jadpo build` writes each into the Wrangler file's `ratelimits`.

This store can count only windows of 10 or 60 seconds. A limit with any
other window is a compile error, which names Redis as a store that can
count it. The default limits are all per minute, so only a limit you
write yourself, such as `within: 1h`, can raise it.

The binding says only whether a request is within the limit, so a
refused client is told to try again after the window's whole length. If
the Worker hasn't got a binding, or the binding throws or doesn't answer
within 500 ms, the instance counts in its own memory instead. That is
logged once for each instance, and no request fails for it.

[Rate limits on Workers](#rate-limits-on-workers) has the details.
[What is built](#what-is-built) says how a Redis store counts.

## Caches: drivers and what they hold
<!-- topic: caches-drivers-and-what-they-hold -->

Nothing is cached unless you add it. [Caches](05-data.md#caches) says how
you add a cache and how its copies stay current. This topic says which
services can hold a cache and what a cache may hold.

### Redis-compatible services

Upstash, Valkey, Amazon ElastiCache and Redis itself all use the Redis
driver, on the native target and on Workers. Each service's settings are
listed on the driver's page, as every driver's are
([Connecting in `app.jadpo`](11-integrations.md#connecting-in-appjadpo)).

### `Driver.memory`

`Driver.memory` keeps a cache in each server's own memory. Each server
then has its own cache, emptied on every restart. You may use it in
production, and `jadpo check` warns you when you do. On Workers it is a
compile error, because a Worker keeps no memory between requests.

### Cloudflare KV

Cloudflare KV isn't a cache driver. A change to KV can take a minute to
reach every location, so an entry you invalidated could still be served.
An app that wants KV calls it as a package.

### What a cache may hold

A cache holds only what is harmless to lose: cached records and query
results, route rate limits, and provider rate limits shared across
servers. All of them go through the same `rate_limits: { store: cache }`
and caching settings.

A cache never holds idempotency keys, webhook de-duplication, sessions,
OAuth grants, poll state, schedule leases, batch positions, the records
of a multi-step call or the outbox. Each of those must survive a
restart, so each lives in a database. Naming a cache-only store for one
of them is a compile error naming the feature.

## Files
<!-- topic: files -->

To store a file, such as a user's avatar, give an entity a field of type
`File`. The record holds the file's key, name, size and type. The bytes
live in a connection that provides the `files` capability: the built-in
S3 driver for any S3-compatible service, or the built-in disk driver on
the native target. Here an avatar is kept in a Cloudflare R2 bucket:

```jadpo fragment
// app.jadpo
application {
    connections: {
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
    }
}

// domains/accounts/entities/user.jadpo
entity User {
    ...
    avatar: File? {
        store: avatars
        max_size: 5mb
        types: [FileType.png, FileType.jpeg, FileType.webp]
    }
}
```

The field's settings:

- `store:` names the connection, as it does for entities and caches. Two
  buckets are two connections.
- `max_size:` is the largest file the field accepts.
- `types:` lists the file types it accepts.

A file is kept in the field only when it fits all three, which every
write checks ([What a write checks](#what-a-write-checks)).

Cloudflare R2, Amazon S3, Backblaze B2, DigitalOcean Spaces and MinIO all
use the S3 driver, each with its own host and settings, listed on the
driver's page. No package wraps the driver with presets.

### Files in development and tests

In `jadpo dev` and tests, files go to local disk with locally signed
links, so nothing leaves your machine. That local driver doesn't exist in
production builds. An app whose production store is the disk driver uses
the same driver in development, at a folder under `.jadpo/`, so upload
slots, ranges and signed links behave as they will in production.

### Writing a file your app makes: `put`

A file your app makes itself, such as a generated PDF, is written with
`attempt avatars.put(bytes, FileType.pdf)`, which returns a `File` to
store in a field. `put` is an outside call, so you make it before the
action's writes, while no transaction is open
([Outside calls hold no transaction](06-reads-and-writes.md#outside-calls-hold-no-transaction)).
In an action with several changing calls it is a keyed step, like a
provider call, and its key is derived as any step's is, so a rerun writes
the same object again
([Several changing calls in one action](12-jobs-and-events.md#several-changing-calls-in-one-action)).
Bytes that are never attached to a record are deleted after a day, like
abandoned uploads.

A bakery keeps the price sheets its suppliers send. `sheets.put` keeps
the bytes before the action reads or writes anything, and gives the
`File` that the record keeps. A store with no room answers
`storage_full`, which only a failure of kind `unavailable` can map:

```jadpo check project=files-put file=app.jadpo
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=files-put file=domains/bakery/types/sheet_text.jadpo hidden
/// A price sheet as a supplier sends it.
type SheetText = Text {
    min_length: 1
    max_length: 100000
}
```

```jadpo check project=files-put file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=files-put file=domains/bakery/entities/price_sheet.jadpo
/// A supplier's price sheet, kept as it was sent.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 5mb
        types: [FileType.csv]
    }

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}
```

```jadpo check project=files-put file=domains/bakery/actions/keep_prices.jadpo migrate=initial
use app { sheets }
use bakery/entities { PriceSheet }
use bakery/types { SheetText }
use bakery/failures { SheetNotKept }

/// Keeps a price sheet as it was sent.
action keep_prices(sheet_text: SheetText) fails SheetNotKept -> PriceSheet {
    var sheet: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {
        storage_full: SheetNotKept
    }
    return PriceSheet.create {
        set: {
            sheet: sheet
        }
    }
}
```

### Reading a file: `read`, and formats

A store moves bytes in and out and knows nothing of what is inside.
Formats (CSV, TSV, JSON, JSON Lines) turn bytes into your app's types and
back, whatever store the bytes came from. Your code joins the two:

- `attempt store.read(file)` returns the file's contents as
  `FileContents` ([Other built-in types](02-values-and-types.md#other-built-in-types)),
  for passing the bytes on: to a provider as an upload, to a PDF service,
  or as an email attachment. The format helpers don't accept it. It reads
  the whole file into memory, so it is capped: reading a field whose
  `max_size` is over the whole-read limit of 10 MB is a compile error
  pointing at the format helpers or `batches.each`.
- Rows in a file, of any size, are read and written only through the
  batch helpers: `csv.read`, `csv.write`, `tsv`, `json_lines`,
  `json.read` and `json.write`
  ([Batches: work over many items](12-jobs-and-events.md#batches-work-over-many-items)).
  [Data formats](19-standard-library.md#data-formats-json-csv-tsv-and-json-lines)
  lists their operations. There is no loop over a file's rows, and no
  parse of a whole file into a list. A provider's CSV report is put in a
  store and read the same way.
- `json.read(text, as:)` is only for single JSON documents in memory,
  such as request bodies and provider answers.

Media services, such as Cloudinary and Uploadcare, are ordinary packages.
You call them through their own operations and pure functions, such as
one that builds a transformation URL, because they can't do the whole
`files` job.

### Uploading

Files go straight from the client to the store, not through your app, so
the 1 MB limit on a request body ([What is refused](07-http.md#what-is-refused))
isn't in the way:

1. An action calls the generated `User.avatar_upload()`, which returns an
   `UploadSlot`: a signed address valid for 15 minutes that accepts only
   `max_size` bytes.
2. The client sends the file to that address.
3. The client sends the slot's token in an ordinary create or update, in
   an `Input` field of type `FileUpload`. Before the write commits, the
   runtime checks that the file is there, its size, and its type from its
   first bytes rather than what the client claimed. A mismatch is
   `invalid_value`.

Asking for a slot needs the `create` or `update` grant on the entity
([Grants](09-policy.md#grants)). Uploads that are never attached are
deleted after a day.

### Large uploads go in parts

A field whose `max_size` is over 100 MB gets a multipart slot instead of
a single address, the same shape on S3, R2 and disk. The slot names a
part size, with equal parts as R2 requires, and gives an address for each
part: a signed `UploadPart` address on S3 and R2, and
`PUT /files/<store>/uploads/<slot>/parts/<n>` on disk. The client sends
the parts, and may resend any part after a dropped connection. Attaching
the `FileUpload` completes the upload and runs the same size and type
checks. A generated client helper hides the parts.

```text
POST /imports/slot                          an app route calling CustomerImport.sheet_upload()
<- 200 { "token": "upl_…", "part_size": 16777216, "parts_url": "/files/uploads/upl_…/parts" }
PUT /files/uploads/upl_…/parts/1            16 MiB
<- 204
PUT /files/uploads/upl_…/parts/2            the connection drops; the client resends part 2
<- 204
POST /imports { "sheet": "upl_…" }          the ordinary create joins the parts, checks size and type
<- 201
```

### Compressed uploads

An upload sent with a `Content-Encoding` is refused. A gzip file, such as
`.csv.gz`, is read only by a batch helper given `max_unpacked_size:`, and
going past that size fails the file, so a small upload can't unpack to
gigabytes ([Reading gzip](#reading-gzip)). Jadpo unpacks no other
compression and no archives, such as zip.

### Downloading

An `Output` that picks a `File` field sends a signed link valid for five
minutes, with the file's name, size and type. The read has already passed
policy, field restrictions included, so there is no separate download
operation to protect.

### Deleting and erasing

Deleting the record, or clearing the field, deletes the bytes after the
change commits, through a generated subscriber, because an action writes
only one store ([One store per action](06-reads-and-writes.md#one-store-per-action)).
A `File` field can be marked `personal:`. Erasure and retention then
reach it like any field ([Personal data](15-personal-data.md)), and
`User.export` includes it as a link.

### The disk driver

`Driver.disk` keeps files on the server's own disk. It works in
production on the native target only, and on Workers it is a compile
error. The build manifest records that the app then runs on one server,
since a second server wouldn't see the files: each deployable that
reaches a disk store lists those stores as its `one_server`
([The manifest](18-records-and-manifest.md#the-manifest)).

```jadpo fragment
// app.jadpo
application {
    connections: {
        uploads: {
            driver: Driver.disk
            path: "/var/lib/acme/uploads"
            free_space_floor: 5%
        }
    }
}
```

- **Where.** `path:` is the folder the store's files are kept in,
  written in full with no `.` or `..` in it, and never inside the
  project's own folder, where git, `jadpo build` and `jadpo dev` would
  reach them: `jadpo check` refuses any other
  ([`connection.disk_path_invalid`](../errors/connection.disk_path_invalid.md)),
  following a link that leads into the project. As a deployable starts it
  makes the folder, readable by the app's user only, when it is missing,
  and doesn't build a store whose folder other users can write to,
  listing it in `runtime.started`'s `unbuilt`, since they could plant
  files it reads; so is a store with no `path:`, answering
  `misconfigured` when used.
- **Files never change.** Each is kept under a key the runtime
  generates, a UUID sharded into two directory levels, never one derived
  from a file's name. The driver refuses any key that doesn't match its
  own pattern, opens each folder and file relative to the one above it
  without following symbolic links, reads only a regular file, and
  writes folders and files readable by the app's user only.
- **Uploads are streamed** to a temporary file as they arrive. They are
  counted, with a `413` at the first byte over `max_size`, hashed and
  type-checked, then synced and renamed into place, and the directory is
  synced.
- **Your app serves downloads** from its own origin. It answers
  single-range `Range` requests and `If-Range`, with an ETag from the
  file's hash, and reads in fixed sizes, so a 2 GB download uses the
  memory a 2 KB one does. Because the origin is your app's own, every
  download is sent with `Content-Disposition: attachment` (except for an
  allow-list of image types), `X-Content-Type-Options: nosniff` and
  `Content-Security-Policy: sandbox`. The client's file name only ever
  goes in `filename*` (RFC 6266).
- **Space.** A write that runs out of space fails as `storage_full`.
  `free_space_floor:`, a percentage from `0%` up to, but not, `100%`, 5%
  by default, is the share of the disk kept free: a put, an upload or a
  join that would leave less free than it answers `storage_full` and
  writes nothing, so imports and the database aren't starved of space.
  The floor is a [monitoring threshold](#thresholds-across-requests).
- **Commands.** `jadpo files ls`, `show`, `get`, `put` and `copy` work
  on any store, in development or production. `jadpo files copy` moves a
  store's files to another, which is how you move off one server.

A deployable built for Workers that uses `Driver.disk` is refused
([`deployable.target_unsupported`](../errors/deployable.target_unsupported.md)).
`jadpo dev`'s disk driver answers a full disk with `storage_full`.

### Image variants and scanning

Both come when an app needs them.

- **Image variants**, such as a thumbnail, are declared on a `File`
  field, `variants: { thumb: 200x200 }`, and made by the runtime over any
  store, so no store needs to offer them and the `files` job stays the
  same everywhere. The runtime checks an image's pixel dimensions before
  decoding it. A variant is read as `user.avatar.variants.thumb`, a
  `File` that becomes a signed link in an `Output` like the original.
- **Virus scanning** is the `scanning` capability, named in
  `application { }` as `scanning: { connection: clamav }`. A ClamAV
  container package fills it ([Services](11-integrations.md#services)),
  or a scanning API's package. With it set, Jadpo scans every upload
  itself. A file stays in a scanning state, and can't be attached, until
  it passes. An infected file is refused like one of the wrong type, and
  deleted.

## Audit trail
<!-- topic: audit-trail -->

An entity keeps no history of its changes unless you ask. Give it an
`audit:` section and it keeps a history of every change: who made it,
when, through which action and request, and each changed field's value
before and after. `retention:` says how long each entry is kept:

```jadpo fragment
// domains/billing/entities/invoice.jadpo
entity Invoice {
    ...
    audit: {
        retention: {
            after: config.audit_retention
        }
    }
}
```

- Each entry is written in the same transaction as the change, in the
  same store, so a change can't happen without its entry.
- "Who" is the principal, the signed-in caller. For schedules and
  subscribers it is the application, together with the request that
  emitted the event, so a change made later is traced back to the click
  that caused it.
- Fields marked `personal:` are stored as keyed hashes, as they are in
  logs ([Marking personal fields](15-personal-data.md#marking-personal-fields)).
  The history shows that an email changed, not what it was. Server-only
  fields are left out.
- `Invoice.history(invoice_id)` is a generated query returning a page of
  entries. It needs the `read` grant on the record, and fields the caller
  can't read are left out.
- Entries are deleted when their retention ends, and with their record
  when it is erased. The permissions report lists which entities are
  audited.

The audit trail records changes to data. Who exported or erased a
person, and who read sensitive fields, is kept as log events instead
([Access records](15-personal-data.md#access-records)).

### Sending audit entries elsewhere

When something subscribes to the built-in `AuditEntryRecorded` event, or
`publishing` names it, each entry is also emitted as that event, in the
same transaction, and travels through the outbox like any event
([Delivery: at least once, safe to repeat](12-jobs-and-events.md#delivery-at-least-once-safe-to-repeat)).
A SIEM or log archive receives it through `publishing`. With nothing
listening the event is never emitted. The database stays the record of
truth; the event is a copy, carrying the same hashed personal fields.

## Telemetry
<!-- topic: telemetry -->

The generated runtime reports what it does as OpenTelemetry traces,
metrics and logs, the open standard most monitoring tools accept. With
no `telemetry:` section, nothing is sent. To send it, add one to
`application { }`. It isn't a connection, because there is one protocol
and no vendor-specific code: a vendor is an endpoint and a header. Its
host is still listed in the network report
([The network report](11-integrations.md#the-network-report)). Here
telemetry goes to Honeycomb:

```jadpo fragment
// app.jadpo
application {
    telemetry: {
        endpoint: config.telemetry_endpoint
        credential: {
            header: "x-honeycomb-team"
            value: config.telemetry_key
        }
        send: {
            traces: true
            metrics: true
            logs: LogLevel.info
        }
        detail: {
            full_traces: [Trace.failures, Trace.faults, Trace.slow, Trace.new_paths]
            samples: {
                per_path: 1
                every: 1m
            }
        }
    }
}
```

### `send:`, which signals go out

`send:` says which signals go out, and the lowest level of log sent.
Leave it out and traces and metrics are sent, with logs from
`LogLevel.warn`.

### `detail:`, how much of each request

`detail:` says how much of each request is sent. `full_traces:` lists
when a request's whole trace is sent: it failed, it faulted, it crossed a
threshold, or its path is new to this process. `samples:` says how many
ordinary traces each path sends, `per_path:` within `every:`, so there is
always a normal request to compare a bad one with. The values in the
example are what you get by default, as
[Monitoring thresholds](#monitoring-thresholds) describes.

### The standard variables

`endpoint:` and `credential:` come from settings bound to the standard
`OTEL_*` names
([Environment variable names](10-configuration-and-time.md#environment-variable-names)),
so a host that sets `OTEL_EXPORTER_OTLP_ENDPOINT` and
`OTEL_EXPORTER_OTLP_HEADERS` needs nothing else. Your app's code can't
read these settings.

### What is sent

- **Traces.** One span for each route, action, query, provider call,
  cache read, schedule run and event delivery, named after the
  declaration (`POST /todos`, `create_todo`, `todos_for_owner`).
  Incoming and outgoing `traceparent` headers are honoured, and a trace
  continues from the action that emitted an event into its subscriber.
- **Metrics.** Requests and durations for each route, failures for each
  code, query durations, provider calls and retries, cache hits,
  rate-limit refusals, the size of the queue and the age of the oldest
  waiting delivery.
- **No values from your app's data,** unless you add them deliberately.
  Spans carry names, identifiers and failure codes, never field values,
  as logs do ([Logging](04-failures.md#logging)), and marked personal
  fields are hashed. The two deliberate ways to add a value are custom
  metric labels and `telemetry.note`, below, and both refuse personal
  fields.
- **Jadpo's own data, to every provider.** Path IDs, stable identities,
  source locations and the build id travel as attributes named
  `jadpo.…`, such as `jadpo.path` and `jadpo.path_key`. A monitoring
  product decodes them against the build manifest
  ([The manifest](18-records-and-manifest.md#the-manifest)), which a
  deploy uploads once for each build.

### Custom metrics

You declare your own metrics in a domain's `monitoring.jadpo`
([Project layout](01-project-layout-and-modules.md#project-layout)) and
record them in code:

```jadpo fragment
// domains/billing/monitoring.jadpo
/// Checkouts that completed, by plan.
metric checkouts_completed = Counter {
    by: [plan]
}

// domains/billing/actions/complete_checkout.jadpo
telemetry.checkouts_completed.add(1, plan: account.plan)
```

A metric is one of three kinds:

- a `Counter`, a count that only rises, recorded with `add`;
- a `Measure`, a distribution such as a duration or an amount, recorded
  with `record`;
- a `Gauge`, a value at a moment, recorded with `set`.

A metric's labels, its `by:`, may only be enums and `Bool`, so the
number of series stays bounded.

`telemetry.note(name: value)` adds an attribute to the current trace,
for a fact no span carries. It takes text, numbers, `Bool` and enums. A
personal field given as a label or an attribute is a compile error.
There are no custom spans, because every action and query is already
one.

### In development

`jadpo dev` exports nothing unless you give it `--export-telemetry`. The
traces, metrics and logs stay on your machine, and you read them from the
command line with `jadpo traces`, `jadpo metrics` and `jadpo logs`,
filtering and watching them as you do the development call log
([Traces, metrics and logs in development](17-debugging-and-review.md#traces-metrics-and-logs-in-development)),
and in the developer console.

## Path IDs
<!-- topic: path-ids -->

Every request records the path it took through your app: the route,
each action and query, and each branch taken inside them. The compiler
knows every possible path, so a path can be named by a number rather
than described. You write nothing for this.

### `path`

`path` is a number that decodes against that build's graph. It uses
Ball–Larus path numbering, which gives each path without loops through a
callable its own small integer, at little cost to record.
`jadpo path <build> <path>` draws the route from the number alone, with
no recorded data.

### `path_key`

`path_key` is a hash of the stable identities on the path
([Stable identity](18-records-and-manifest.md#stable-identity)), so the
same path can be compared across builds: "p95 of this path since
Tuesday's deploy". An edit inside a declaration changes its key, which
is right, because the path changed.

### Loops and recursion

A `for` body records which inner paths ran, not how many times. A
recursive call records the path of its first entry.

### Where path IDs go

Both go on the root span and on every failure and fault event. A path ID
says which way a request went, not how long it took or what it carried.
Timings, the request and the response come from the trace
([The request timeline in `jadpo dev`](17-debugging-and-review.md#the-request-timeline-in-jadpo-dev)).

Two failures at the same line reached from different routes keep
different paths. A path also shows the branches that ran and finished
before the failure, which a stack no longer holds.

Path IDs need the compiler's graph to hold the control flow inside each
callable. Each body's ways are numbered today, and the differential run
compares the paths each test's bodies took
([How it is built](../compiler-runtime.md)), but no request carries a
`path` or a `path_key` yet ([What is built](#what-is-built)).

## Monitoring thresholds
<!-- topic: monitoring-thresholds -->

A threshold says when something your app does is too slow, too frequent
or failing too often, such as a route's p95 duration. You declare
thresholds in your app's source, in a monitoring file for each domain, so
they are reviewed with the code and versioned with each build rather
than configured in a monitoring tool. There are two kinds, because only
one can be judged by a single process.

### Writing a threshold

A domain's thresholds are its `thresholds { }`, in
`domains/<domain>/monitoring.jadpo`. Each is `name: { }` giving one
thing it watches and its limits, and its `///` line, what it protects,
is the summary of the alerts made from it:

```jadpo check project=monitoring-thresholds file=domains/hello/monitoring.jadpo
use hello/actions { greet }

thresholds {
    /// A visitor is greeted quickly.
    greeting_route: {
        route: {
            method: HttpMethod.post
            path: "/hello"
        }
        duration: 800ms
        queries: 2
        p95: 300ms
        server_failures: 1%
        within: 5m
    }

    /// Working out a greeting stays quick.
    greeting: {
        watches: greet
        p99: 50ms
    }
}
```

```jadpo check project=monitoring-thresholds file=app.jadpo hidden
application {}
```

```jadpo check project=monitoring-thresholds file=policy.jadpo hidden
use hello

policy {
    actions: {
        hello.greet: [Access.public]
    }
}
```

```jadpo check project=monitoring-thresholds file=domains/hello/types/visitor.jadpo hidden
/// Who is calling.
type Visitor = Input {
    name: Text {
        max_length: 40
    }
}
```

```jadpo check project=monitoring-thresholds file=domains/hello/types/greeting.jadpo hidden
/// What a visitor is told.
type Greeting = Output {
    message: Text
}
```

```jadpo check project=monitoring-thresholds file=domains/hello/actions/greet.jadpo hidden
use hello/types { Greeting, Visitor }

/// Greets a visitor by name.
action greet(input: Visitor) -> Greeting {
    return Greeting {
        message: "Hello, {input.name}"
    }
}
```

```jadpo check project=monitoring-thresholds file=domains/hello/routes/hello.jadpo hidden
use hello/types { Greeting, Visitor }
use hello/actions { greet }

/// Greetings.
routes Hello {
    prefix: /hello

    /// Greets whoever calls it.
    POST / {
        authentication: RouteAuthentication.none
        input: Visitor
        output: Greeting
        run: greet(input)
    }
}
```

What it watches is in its own domain:

- `route:` names a route by its `method:` and its path as `jadpo
  routes` prints it, placeholders by name, as a route test names one;
- `watches:` names an action, query, schedule or subscriber, imported
  like any name the file uses. Their names are unique across the
  project, so the name says which.

Its limits, each an upper bound:

| Limit | Kind | Watches | Value |
|---|---|---|---|
| `duration:` | Per request | Anything | The longest one request or run may take; a route's is shorter than its `deadline:` |
| `queries:` | Per request | All but a query | The most queries one may run |
| `p50:`, `p90:`, `p95:`, `p99:` | Across requests | Anything | That share of the requests within `within:` takes at most this long |
| `server_failures:` | Across requests | Anything | The share within `within:` that may end in a fault or a failure of a kind answered with a 5xx: `unavailable`, `timed_out`, `outcome_unknown`, `misconfigured` or `internal_fault`; never one a client caused |
| `oldest_waiting:` | Across requests | A subscriber | The age its oldest waiting delivery may reach |

`within:` is the window the across-request limits are judged over, a
whole number of minutes, hours or a day from `1m` to `1d`, and `5m`
when left out. A percentile is no shorter than a lower one beside it,
and `server_failures:` is above `0%` and below `100%`. A threshold that
names nothing in its domain is
[`threshold.subject_unknown`](../errors/threshold.subject_unknown.md);
any other mistake in one, such as a limit that doesn't apply to what it
watches, is
[`threshold.declaration_invalid`](../errors/threshold.declaration_invalid.md).

### Thresholds per request

These judge one request: its duration, its number of queries, whether
it failed. The runtime keeps each request's spans in memory until the
request ends, then sends:

- the full trace when a threshold was crossed, the request failed or
  faulted, or its path is new to this process;
- otherwise metrics only, plus one sample trace for each path each
  minute, so there is always a normal request to compare a bad one with.

`telemetry: { detail: }` sets both ([Telemetry](#telemetry)):
`full_traces:` names the first list and `samples:` the second.

A process that crashes loses the requests it hasn't sent yet, and long
jobs send on a timeout. This keeps the volume low without losing
anything structural: the compiler knows every path, so the number of
per-path metrics can't grow without bound.

### Thresholds across requests

These judge many requests together: a p95 over a window, an error rate,
the oldest waiting delivery. One instance can't judge them for a fleet,
so the compiler generates rules for your monitoring tool into
`build/monitoring/`: OpenSLO and Prometheus rules, and other tools' only
when asked. The build manifest carries the same thresholds, so a tool
that reads it needs no generated rules. The rules are applied when you
deploy, and a change made in the tool is overwritten, as with Terraform.

A disk store's `free_space_floor:` ([The disk driver](#the-disk-driver))
is one of these too: the rules raise an alert when the store's disk has
less free than its floor, as puts start answering `storage_full`.

### Setting and changing them

- **Starting values** come from what the compiler knows, such as the
  queries on each path from the query report, then from production once
  the app has run. Local timings aren't production timings, so locally
  the compiler reports change on the same fixtures: "30% slower",
  "2 more queries".
- **An agent proposes changes** from observed data as an ordinary
  change to the source, through the MCP server
  ([The developer console and MCP](17-debugging-and-review.md#the-developer-console-and-mcp)).
- **Loosening needs a person.** Raising or removing a threshold is how
  an agent would silence an alert, so it needs an
  [acceptance](18-records-and-manifest.md#acceptances) and appears in
  the approval report. Tightening one doesn't. `jadpo check` compares
  each threshold with the one of the same name on the before side, read
  from git: a limit raised, or a longer `within:`, is
  `planning.todo_list.p95: ThresholdChange.raised`; a limit gone is
  `ThresholdChange.removed` on the limit, and a threshold gone, or
  watching something else, on the threshold, `planning.todo_list`; a
  disk store's floor lowered is `uploads.free_space_floor:
  ThresholdChange.lowered`. A new threshold, a lower limit and a higher
  floor need nothing.
- **Per build and per flag value.** Thresholds belong to a build. Each
  flag value takes its own branch, so it has its own path and metrics,
  with no extra tagging.

OpenTelemetry stays the transport; only what is sent changes.

## Email
<!-- topic: email -->

Most apps send a handful of emails: confirm your address, an invoice, a
welcome. Each is a type, `type X = EmailMessage`, whose fields are the
values its text uses. It lives in its domain's `emails/` folder, beside
its HTML and text templates. Its subject line is its entry in the
domain's language files, with its fields as placeholders
([Email subjects](10-configuration-and-time.md#email-subjects)).

The templates' words come from the domain's translations
([Translations as messages](10-configuration-and-time.md#translations-as-messages)).
A template calls `{{ translations.<domain>.<message>(...) }}`, which
takes the email's locale, and the compiler checks every call and
placeholder against the message types and the email's fields.

Here is a welcome email, from its type to the action that sends it. The
app sends mail over SMTP, its settings and `User` entity are hidden, and
the email's type is:

```jadpo check project=email-welcome file=app.jadpo hidden
application {
    connections: {
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
            credential: {
                username: config.smtp_username
                password: config.smtp_password
            }
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=email-welcome file=config.jadpo hidden
config {
    app_url: Url {
        default: "https://notes.example"
    }
    mail_sender: Email {
        default: "hello@notes.example"
    }
    smtp_host: Hostname
    smtp_password: Text {
        secret: true
    }
    smtp_username: Text
}
```

```jadpo check project=email-welcome file=domains/accounts/types/display_name.jadpo hidden
type DisplayName = Text {
    min_length: 1
    max_length: 80
}
```

```jadpo check project=email-welcome file=domains/accounts/entities/user.jadpo hidden migrate=initial
use accounts/types { DisplayName }

entity User {
    id: Uuid
    email: Email
    display_name: DisplayName
    locale: Locale

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=email-welcome file=domains/accounts/emails/welcome_email.jadpo
use accounts/types { DisplayName }

/// Sent once a user has signed up.
type WelcomeEmail = EmailMessage {
    display_name: DisplayName
    sign_in_link: Url
}
```

Its heading is one of the domain's messages, and the domain's language
file words it. There, `welcome_email` is the email's subject line:

```jadpo check project=email-welcome file=domains/accounts/translations/welcome_heading.jadpo
use accounts/types { DisplayName }

/// The heading of the welcome email.
type WelcomeHeading = Message {
    name: DisplayName
}
```

```jadpo check project=email-welcome file=domains/accounts/translations/sign_in.jadpo hidden
/// The welcome email's link to sign in.
type SignIn = Message {}
```

```jadpo check project=email-welcome file=domains/accounts/translations/en_gb.jadpo
translations {
    sign_in: "Sign in"
    welcome_email: "Welcome, {display_name}"  // the subject line
    welcome_heading: "Welcome, {name}"
}
```

The `.html` template, `domains/accounts/emails/welcome_email.html`, sits
beside the type:

```jadpo check project=email-welcome file=domains/accounts/emails/welcome_email.html
<html dir="{{ locale.direction }}">
<h1>{{ translations.accounts.welcome_heading(name: display_name) }}</h1>
{{ include button(label: translations.accounts.sign_in(), url: sign_in_link) }}
</html>
```

```jadpo check project=email-welcome file=domains/accounts/emails/welcome_email.txt hidden
{{ translations.accounts.welcome_heading(name: display_name) }}

{{ include button(label: translations.accounts.sign_in(), url: sign_in_link) }}
```

### Templates

An email has an `.html` template, a `.txt` one or both, named after its
type. An email with neither is `email.template_missing`, and so is a
template that no email type is named after.

Inside `{{ }}` a template writes Jadpo's own expressions:

- its email's fields, `to` and `locale` among them;
- `{{ if (...) }} ... {{ else if (...) }} ... {{ else }} ... {{ end }}`
  on a `Bool`;
- `{{ for item in list }} ... {{ end }}`;
- translations and `text.format`, both written in the email's locale
  without a `locale:`;
- `{{ locale.direction }}`, the locale's text direction in Jadpo's
  wording, `ltr` or `rtl`, for a `dir=` attribute.

### What a template may not do

Templates are strict. Each of these is a compile error:

- a name that isn't a field of the email's type;
- a message that doesn't exist, or a placeholder that is missing or of
  the wrong type;
- a partial that doesn't exist or is given the wrong fields;
- `if` on something that isn't a `Bool`, and `for` over something that
  isn't a list;
- any call other than translations and formatting, and arithmetic or
  building a value (`email.template_not_allowed`).

A field that no template reads, and no wording of the subject writes, is
`email.field_unused`. It is a judgement call, so a `decline` line above
the field answers it
([Declining a judgement call](01-project-layout-and-modules.md#declining-a-judgement-call)).

### Partials: pieces several emails share

A partial is a piece several emails share, such as a button or a
signature. You declare it as `type Name = EmailPartial { ... }`, with its
`.html` and `.txt` beside it as an email's are:

- in `shared/emails/`, where every domain's emails may include it;
- or in a domain's own `emails/`, where only that domain's emails may.

A template includes one as `{{ include name(field: value, ...) }}`,
giving each of its fields once, by name. An HTML template includes the
partial's `.html`, and a text one its `.txt`. The partial is written in
the locale of the template that includes it, which it gets as its
`locale`. Its templates follow the same strict rules, and may include
other partials, but never lead back to themselves. A partial that
doesn't exist, can't be reached, lacks the template it is included from,
or is given the wrong fields is `email.partial_invalid`.

Here is the button the welcome email includes:

```jadpo check project=email-welcome file=shared/emails/button.jadpo
/// A link set as a button, in every email that asks the reader to act.
type Button = EmailPartial {
    label: PresentationText
    url: Url
}
```

```jadpo check project=email-welcome file=shared/emails/button.html
<a href="{{ url }}" style="display:inline-block;padding:12px 24px;background-color:#1f2937;color:#ffffff;text-decoration:none;">{{ label }}</a>
```

```jadpo check project=email-welcome file=shared/emails/button.txt hidden
{{ label }}: {{ url }}
```

### What a template writes out

Only text and numbers are written out, in templates as in placeholders.
`{{ value }}` takes `Text`, `PresentationText`, `Int` or `Decimal`, with
numbers written the locale's way, or an enum whose values have wording
([Enums with wording](10-configuration-and-time.md#enums-with-wording)).
Writing anything else out is a compile error that says how to format it
(`email.template_value_invalid`):

- an `Instant` or `Money`, with `text.format`
  ([Every standard type meets one contract](19-standard-library.md#every-standard-type-meets-one-contract));
- a `Bool`, as a condition for `if`;
- a list, walked with `for`;
- an object, by writing its fields one by one;
- an enum without wording, by adding wording.

Values are escaped. HTML goes in unescaped only as the safe-HTML type
`Html` ([Text formats](19-standard-library.md#text-formats)), which is
safe because of how it is built.

### Sending

You send an email by emitting it. It goes after the action commits, and
is retried like any delivery
([Delivery: at least once, safe to repeat](12-jobs-and-events.md#delivery-at-least-once-safe-to-repeat)),
so a sign-up that is rolled back never sends a welcome. Every
`EmailMessage` has `to:` and `locale:`.

```jadpo check project=email-welcome file=domains/accounts/actions/sign_up.jadpo
use accounts/entities { User }
use accounts/types { DisplayName }
use accounts/emails { WelcomeEmail }

/// Signs someone up, and welcomes them once it is done.
action sign_up(email: Email, display_name: DisplayName, chosen_locale: Locale) -> User {
    var user: User = User.create {
        set: {
            email: email
            display_name: display_name
            locale: chosen_locale
        }
    }
    emit WelcomeEmail {
        to: user.email
        locale: user.locale
        display_name: user.display_name
        sign_in_link: config.app_url
    }
    return user
}
```

The email is rendered whole when it is sent. Each template is run with
the email's fields and wrapped in the shared layout
([Built-in emails](#built-in-emails), level 3), the subject is worded in
the email's locale, and each `File` field is read from its store as an
attachment. The connection is handed that message, and the delivery's
record alone says it went.

### Where it goes: `email:`

Mail goes through the `email` capability
([Capabilities](11-integrations.md#capabilities)).
`email: { connection: postmark  from: config.mail_sender }` in
`application { }` names a connection, and every email goes through it,
your app's and Jadpo's own ([Built-in emails](#built-in-emails)).

- **SMTP** is a built-in driver, `driver: Driver.smtp`
  ([The SMTP driver](#the-smtp-driver)). On Cloudflare Workers it uses
  port 587, since Workers blocks port 25.
- **A mail package**, such as Postmark, Amazon SES or Resend, is the
  other way.

The `EmailMessage` types, the translations and `emit` are the same
whichever you choose, so changing provider changes only `app.jadpo`.

### Attachments

A `File` field on an `EmailMessage` is an attachment. It is read from
its store when the email is sent, and checked against the size limit the
mail package declares. Through a mail package, each `File` field needs a
`max_size:`, and an email whose fields' sizes add up to more than the
package's `limits: { attachments: }` is `email.attachment_limit`. A
provider that can't send attachments can still be an ordinary package,
but can't be the `email:` connection.

### What belongs to the mail package

Everything specific to one provider is the package's own:

- Per-send values, such as Postmark's message streams or SES
  configuration sets, are typed settings on the connection.
- Hosted templates with variables
  (`postmark.email.send_with_template(...)`), suppression lists and
  marketing sends are the package's operations. You call them from an
  action or subscriber in a domain the connection lists. One connection
  serves both: `email:` uses it, and the listed domains call it. Mail
  sent from a hosted template gets no dev inbox entry, no health score
  and no automatic template check, though your app can add a contract
  case.
- Bounces and complaints are the package's own webhook events, which you
  subscribe to like any package event. There are no standard
  `EmailBounced` and `EmailComplained` events, so moving to another
  provider means rewriting the bounce subscribers.

### A send whose outcome is unknown

Postmark, SES, SendGrid and Mailgun take no idempotency key, so a send
that may or may not have gone out can't safely be repeated. Such a send
is parked for an operator and never resent blindly, since a duplicate
receipt isn't harmless
([Retries and giving up](12-jobs-and-events.md#retries-and-giving-up)).
`jadpo deliveries` can resend in bulk.

### Email in tests

Tests catch mail instead of sending it. `jadpo test` catches every
email, rendered whole once its action commits, so a template that can't
be rendered fails the test. A test reads what it has sent so far as
`email.sent`, a `List<SentEmail>`. Each has its `to`, `from`, `subject`,
`html` and `text` (`none` without that template), every link in it
(`links`), its attachments' file names (`attachments`), and the sign-in
email's token and code (`sign_in_token`, `sign_in_code`). Only a test
can read `email.sent` (`email.sent_context`). The mail is caught whole
whichever connection `email:` names, a mail package's included.

This test signs someone up and checks the welcome email:

```jadpo check project=email-welcome file=domains/accounts/actions/sign_up.test.jadpo
use accounts/entities { User }

test "signing up welcomes the new user, with a link to sign in" {
    var user: User = sign_up("ada@notes.example", "Ada", Locale.en_gb)
    assert user.display_name == "Ada"
    var sent: List<SentEmail> = email.sent
    assert list.count(sent) == 1
    assert list.any(sent, item -> item.to == "ada@notes.example" and item.subject == "Welcome, Ada")
    assert list.all(sent, item -> list.contains(item.links, "https://notes.example"))
}
```

### Email in development: the dev inbox

`jadpo dev` catches every email in a local inbox, so nothing leaves your
machine ([The dev inbox and the health score](#the-dev-inbox-and-the-health-score)).
You open it in the browser, on a page `jadpo dev` serves beside the app.
Each mail is shown as HTML and as text, in its locale, with its links
working: a magic link opens the frontend's page, which signs in locally.
Each mail also names the trace of the request that sent it.

An agent uses the same inbox from the command line, with
`jadpo email list`, `jadpo email show <id> --json`,
`jadpo email links <id>` and `jadpo email open <id> <link>`, so it can
follow a sign-up or a privacy request from start to finish.
`jadpo email clear` empties the inbox.

### The mail health score

Every mail in the dev inbox, and `jadpo email show <id>`, shows what
Jadpo's own checks found and how to fix each problem: no plain-text
part, content that is only images, broken HTML, links that aren't
absolute HTTPS, link shorteners, a subject in capitals, a missing
unsubscribe header on bulk mail, and more than 102 KB, where Gmail clips
a mail. `jadpo check` reports templates that would fail as
[`email.health_problem`](../errors/email.health_problem.md), an error
you can decline
([Declining a judgement call](01-project-layout-and-modules.md#declining-a-judgement-call)).
When SpamAssassin is installed, its open rules are added
([SpamAssassin in the health score](#spamassassin-in-the-health-score)).
`jadpo email check-domain` compares the sending domain's SPF, DKIM and
DMARC records with what the mail connection needs.

The score is an early indication, not a guarantee, and every screen that
shows one says so. It can't predict whether Gmail, Outlook or any other
provider puts a mail in the inbox, because they use their own filters
and the sender's history, which change constantly. A clean score means
the common mistakes are absent, nothing more, so don't rely on it.
Testing where mail really lands needs seed-account services outside
Jadpo.

### Email and personal data

The personal-data report lists the mail connection as receiving every
marked field a mail carries. A mail package that declares an `erasure:`
operation is called when a person is erased
([Erasure at providers](15-personal-data.md#erasure-at-providers)).

## The SMTP driver
<!-- topic: the-smtp-driver -->

The SMTP driver sends your app's email through a mail server you name. A
connection with `driver: Driver.smtp` gives:

- `host:`, the server's name, written out or read from a setting built
  on `Text`;
- `port:`, which is 587 when you leave it out: the submission port,
  which Cloudflare Workers allows;
- `credential: { username: password: }`, each read from a setting, the
  password from a secret one.

A mistake in any of them is `connection.key_invalid`,
`connection.key_missing` or `connection.value_invalid`
([Connecting in `app.jadpo`](11-integrations.md#connecting-in-appjadpo)).

```jadpo check project=email-smtp file=app.jadpo
application {
    connections: {
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
            port: 587
            credential: {
                username: config.smtp_username
                password: config.smtp_password
            }
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
}
```

```jadpo check project=email-smtp file=config.jadpo hidden
config {
    mail_sender: Email
    smtp_host: Hostname
    smtp_password: Text {
        secret: true
    }
    smtp_username: Text
}
```

### TLS

The driver always uses TLS, except to a server on the same machine. It
upgrades the connection with `STARTTLS`, or speaks TLS from the start on
port 465, and checks the server's certificate and name. Anywhere else, a
server that offers no TLS, or whose certificate fails the check, makes
the send `misconfigured`, so a credential never goes in the clear.

A server on the same machine (`localhost`, `127.0.0.1`, `::1`) is spoken
to without `STARTTLS`, whether it offers it or not, as a database there
is ([PostgreSQL over TLS](05-data.md#postgresql-over-tls)). A stock
Postfix offers `STARTTLS` with a self-signed certificate that no check
passes.

### The message it writes

Lines end in CRLF and every part is in base64. The text and the HTML go
as `multipart/alternative`, with attachments beside them in
`multipart/mixed`. A subject that isn't plain ASCII is written as RFC
2047 encoded words, and `Date:` comes from the host's clock. The server
adds the `Message-ID:`, as RFC 6409 says a submission server does.

### When a send goes wrong

The driver classifies what happened as every driver does
([The seam every outside call goes through](#the-seam-every-outside-call-goes-through)):

- A server that can't be reached, refuses for now (`4xx`), or drops the
  connection before the end of the message went out surely took nothing,
  so the delivery is retried.
- Once the end of the message went out, no answer may still mean the
  message was taken. SMTP takes no idempotency key, so the delivery is
  parked as `outcome_unknown`.
- A recipient refused for good (`5xx`) fails as `recipient_rejected`, and
  a refused message as `message_rejected`.
- A refused credential or sender is `misconfigured`.

## Built-in emails
<!-- topic: built-in-emails -->

Jadpo sends some emails itself: the magic-link email
([Magic links](08-identity.md#magic-links)) and the privacy emails
([Data requests](15-personal-data.md#data-requests)). Each works with
nothing written, and your app can change as much of it as it needs, in
five levels. Each level keeps the ones below it.

Every email, built in or your app's own, goes through your app's one
`email:` setting in `application { }`. It names a connection that
provides `email`: the built-in [SMTP driver](#the-smtp-driver) or any
package that provides it.

```jadpo fragment
// app.jadpo
application {
    connections: {
        postmark: {
            package: Postmark.v1
            credential: config.postmark_token
        }
    }
    email: {
        connection: postmark
        from: config.mail_sender
    }
}
```

`magic_link:` with no `email:` setting is a compile error, unless it
sends its own email (level 5).

### Level 1: nothing

Jadpo renders its own email, HTML and text, in the person's language,
with Jadpo's wording ([Jadpo's own magic-link email](#jadpos-own-magic-link-email)).
Jadpo ships its wording in `en_gb` and `en_us`, and releases add
locales. If your app supports a locale Jadpo doesn't ship, you supply
the wording (level 2). Until you do, `jadpo check` fails with one error
listing every Jadpo message the locale is missing, unless the locale is
`in_progress:`, which falls back as any translation does
([Languages](10-configuration-and-time.md#languages)).

### Level 2: change the words

You write Jadpo's messages in the root `translations/<locale>.jadpo`,
under `jadpo: { }`
([Jadpo's and packages' text](10-configuration-and-time.md#jadpos-and-packages-text)):

```jadpo fragment
// translations/en_gb.jadpo
translations {
    jadpo: {
        magic_link_email: "Your link to sign in to {app_name}"
        magic_link_intro: "Someone asked to sign in to {app_name} as {address}."
        sign_in_codes_disabled: "Too many wrong codes today. Use the link in your email instead."
    }
}
```

You can reword only the messages of features your app has switched on.
Magic-link wording without `magic_link:` is an error saying the feature
is off. `jadpo translations builtin` lists the messages of the features
your app has on, with their fields. Jadpo shows no pages, so its wording
is for emails and failures only.

Every Jadpo message may also use `{app_name}`, the application's
`display_name:` ([The application's name](#the-applications-name)). An
app that needs the name translated rewords the messages.

The magic-link email uses these messages:

| Message | Fields | `en_gb` wording |
|---|---|---|
| `magic_link_email` (the subject) | | "Sign in to {app_name}" |
| `magic_link_heading` | | "Sign in" |
| `magic_link_intro` | `address: Email` | "Someone asked to sign in as {address}." |
| `magic_link_button` | | "Sign in" |
| `magic_link_code_line` | `code: SignInCode` | "Or enter this code: {code}" |
| `magic_link_expiry_line` | `expires_in: PresentationText` | "The link and the code work for {expires_in}." |
| `magic_link_ignore_line` | | "If you didn't ask to sign in, you can ignore this email." |
| `sign_in_link_expired` | | "This link has expired or has been used. Ask for a new one." |
| `sign_in_link_other_browser` | | "Open this link in the browser where you asked for it, or enter the code instead." |
| `sign_in_code_invalid` | | "That code isn't right. Check it, or ask for a new one." |
| `sign_in_codes_disabled` | | "Too many wrong codes today. Use the link in your email instead." |
| `sign_in_refused` | | "We couldn't sign you in." |
| `sign_in_return_invalid` (only with `generate_routes: false`, [Signing in from code the app writes](08-identity.md#signing-in-from-code-the-app-writes)) | | "We can't send you back there after signing in. Start again from this site." |

`expires_in` is `expires_after:` as Jadpo formats it in the email's
language: "15 minutes". The failures' wording also goes into the
per-locale failure export for client apps
([Failures for client apps](10-configuration-and-time.md#failures-for-client-apps)).

These twelve messages are in Jadpo's catalogue, with the wording above.
They are on once you declare `magic_link:`. Then they are listed,
required in your app's other locales and rewordable, the email is
written with them, and the failures' wording is in the per-locale
failure export.

[Passkeys](08-identity.md#passkeys) bring no email, only the wording of
their failures, which a client app shows. These messages are on, listed,
required in your app's other locales and rewordable once you declare
`passkey:`:

| Message | `en_gb` wording |
|---|---|
| `passkey_challenge_expired` | "That took too long. Try your passkey again." |
| `passkey_rejected` | "We couldn't sign you in with that passkey. Try another passkey, or sign in with your email." |
| `passkey_already_registered` | "This passkey is already set up." |
| `passkey_limit_reached` | "You have as many passkeys as an account can hold. Remove one to add another." |
| `passkey_not_found` | "That passkey isn't on your account." |

### Level 3: branding

Every email, built in or your app's own, is wrapped in your app's shared
email layout, so built-in emails carry your branding with no other
change. The layout is optional, and an app has at most one, in
`shared/emails/layout.html` and `shared/emails/layout.txt`. Without it,
Jadpo's plain layout is used. The layout is checked under the same strict
rules as [templates](#what-a-template-may-not-do), may call translations,
and receives:

| In the layout | What it is |
|---|---|
| `{{ content }}` | The rendered body: the safe-HTML type in `layout.html`, text in `layout.txt` |
| `{{ subject }}` | The email's subject line, `PresentationText` |
| `{{ locale.direction }}` | `ltr` or `rtl` ([Locales](10-configuration-and-time.md#locales)) |
| `{{ app.display_name }}` | The application's `display_name:` |

```jadpo check project=email-layout file=app.jadpo hidden
application {
    display_name: "Acme"
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=email-layout file=shared/emails/layout.html
<html dir="{{ locale.direction }}">
<head><title>{{ subject }}</title></head>
<body>
  <header>{{ app.display_name }}</header>
  {{ content }}
</body>
</html>
```

The layout wraps each of your app's emails: its `.html` in `layout.html`
and its `.txt` in `layout.txt`. You can keep either file without the
other. `{{ app.display_name }}` without a `display_name:` is
`email.settings_invalid`.

#### The application's name

`application { display_name: "Acme" }` in `app.jadpo`, plain text, is
required once any built-in email is switched on, such as magic links or
the privacy emails. Without it, `jadpo check` names the feature that
needs it. Templates read it as `{{ app.display_name }}`, and Jadpo's own
wording as `{app_name}`.

### Level 4: your own email

`magic_link: { email: accounts.SignInEmail }` names an `EmailMessage`
type of your app's, which Jadpo fills and sends instead of its own. The
type takes its fields from the built-in `MagicLinkEmail` with `pick:`,
and may have no others, since Jadpo fills it:

| `MagicLinkEmail` field | Type | Holds |
|---|---|---|
| `address` | `Email` | The address the link was asked for; also the email's `to:` |
| `sign_in_link` | `Url` | The link to the frontend's `link:` path, carrying the token |
| `code` | `SignInCode` | The six-digit code, text so a leading `0` stays |
| `expires_in` | `PresentationText` | How long both work, formatted by Jadpo in the email's language |

Every `EmailMessage` also has `to:` and `locale:` ([Sending](#sending)).
Jadpo sets the locale as it does for its own email
([Magic links](08-identity.md#magic-links)).

Here the app's sign-in email picks three fields. In the domain's
language file, `sign_in_email` is the email's subject line:

```jadpo fragment
// domains/accounts/emails/sign_in_email.jadpo
/// The sign-in email, in the app's own words and design.
type SignInEmail = EmailMessage {
    pick: [MagicLinkEmail.sign_in_link, MagicLinkEmail.code, MagicLinkEmail.expires_in]
}

// domains/accounts/translations/en_gb.jadpo
translations {
    sign_in_email: "Your Notes sign-in link"
    sign_in_heading: "Welcome back to Notes"
    sign_in_button: "Open Notes"
    sign_in_code_line: "Or type {code} on the sign-in page."
    sign_in_expiry: "Both work for {expires_in}."
}

// authentication.jadpo
authentication {
    ...
    sign_in: {
        magic_link: {
            email: accounts.SignInEmail
        }
    }
}
```

Its HTML template, `domains/accounts/emails/sign_in_email.html`:

```text
<!-- domains/accounts/emails/sign_in_email.html -->
<h1>{{ translations.accounts.sign_in_heading() }}</h1>
<p><a href="{{ sign_in_link }}">{{ translations.accounts.sign_in_button() }}</a></p>
<p>{{ translations.accounts.sign_in_code_line(code: code) }}</p>
<p>{{ translations.accounts.sign_in_expiry(expires_in: expires_in) }}</p>
```

The messages it calls, such as `SignInHeading` and `SignInCodeLine`, are
the domain's own `Message` types, one to a file in `translations/`. Its
`.html` and `.txt` templates sit beside it in `emails/`, and its subject
is its entry in the domain's language files. All of it follows the
strict [template rules](#what-a-template-may-not-do), and it is wrapped
in the shared layout like any of your app's emails. Your app's code never
emits `MagicLinkEmail`. [Your own magic-link email](#your-own-magic-link-email)
says how Jadpo fills and sends it.

### Level 5: your own sending

`magic_link: { send: accounts.send_sign_in_link }` names an action that
sends the email itself, through any package, instead of Jadpo sending
one. Its signature is fixed: it takes one built-in `MagicLinkRequest`
and returns `Unit`.

| `MagicLinkRequest` field | Type |
|---|---|
| `address` | `Email` |
| `sign_in_link` | `Url` |
| `code` | `SignInCode` |
| `expires_at` | `Instant` |
| `locale` | `Locale` |

Here `messages` is a connection to a package of the app's choice:

```jadpo fragment
// domains/accounts/actions/send_sign_in_link.jadpo
action send_sign_in_link(request: MagicLinkRequest) fails MessageNotSent -> Unit {
    attempt messages.send(...)
}
```

- The action runs after the request commits, like a subscriber, and is
  retried under the same rules
  ([Retries and giving up](12-jobs-and-events.md#retries-and-giving-up)).
  Its failures are a subscriber's.
- Each attempt is given a link and a code of its own, which ends the
  last attempt's. So its provider calls go under new keys each attempt,
  and keep no record of their steps: a provider that replays a key's
  first answer still sends the link that works.
- In development its provider calls are faked and logged, as every
  provider call is
  ([Provider calls in development](11-integrations.md#provider-calls-in-development)).
  The dev inbox doesn't show the mail, since Jadpo sends none.
- Levels 4 and 5 can't be combined: `email:` and `send:` on one email is
  a compile error.

### Every built-in email

The same five levels apply to each email Jadpo sends. A feature with one
email takes `email:` or `send:` directly. A feature with several takes
`emails: { }`, with one entry for each email, and each entry takes
`email:` or `send:`, never both. Delivery is always through the one
`email:` connection.

| Email | Built-in types | Sent when | Configured in |
|---|---|---|---|
| Magic link | `MagicLinkEmail`, `MagicLinkRequest` | `POST /auth/magic-link` ([Magic links](08-identity.md#magic-links)) | `authentication { sign_in: { magic_link: { email: / send: } } }` |
| Address check | `PrivacyAddressCheckEmail`, `PrivacyAddressCheckRequest` | A person asks through `POST /privacy/requests` ([Data requests](15-personal-data.md#data-requests)) | `privacy: { emails: { address_check: { } } }` |
| Export ready | `PrivacyExportEmail`, `PrivacyExportRequest` | `PrivacyExportReady` ([Export](15-personal-data.md#export)) | `privacy: { emails: { export_ready: { } } }` |
| Erasure confirmed, with its cancel link | `PrivacyErasureConfirmedEmail`, `PrivacyErasureConfirmedRequest` | `PrivacyErasureScheduled` ([Erasure](15-personal-data.md#erasure)) | `privacy: { emails: { erasure_confirmed: { } } }` |
| Erasure completed, with what was kept | `PrivacyErasureCompletedEmail`, `PrivacyErasureCompletedRequest` | `PrivacyErasureCompleted` ([Erasure](15-personal-data.md#erasure)) | `privacy: { emails: { erasure_completed: { } } }` |

```jadpo fragment
// app.jadpo
application {
    display_name: "Acme"
    privacy: {
        self_service: {}
        emails: {
            export_ready: {
                email: privacy_mail.ExportEmail
            }
            erasure_confirmed: {
                send: privacy_mail.send_erasure_confirmation
            }
        }
    }
}
```

Jadpo fills the privacy emails' fields. Every one has `address: Email`,
which is also its `to:`, and the `locale:` of the request. Each request
type, for `send:`, has the same fields, with times as `Instant` in place
of text:

| Email type | Its other fields | Request type's other fields |
|---|---|---|
| `PrivacyAddressCheckEmail` | `confirm_link: Url`, `expires_in: PresentationText` | `confirm_link: Url`, `expires_at: Instant` |
| `PrivacyExportEmail` | `download_link: Url`, `expires_in: PresentationText` (7 days) | `download_link: Url`, `expires_at: Instant` |
| `PrivacyErasureConfirmedEmail` | `erases_on: PresentationText`, `cancel_link: Url` | `erases_at: Instant`, `cancel_link: Url` |
| `PrivacyErasureCompletedEmail` | `kept: List<PrivacyKeptRecord>` | `kept: List<PrivacyKeptRecord>` |

`PrivacyKeptRecord` has two fields:

- `name: PresentationText`, the kept record's wording. It is the
  entity's entry in its domain's language files, which every entity
  marked `Erasure.retained` must have.
- `kept_until: PresentationText`, the end of its retention, in the
  email's language.

A template walks `kept` with `for`. The address-check link lasts 15
minutes, as a magic link does.

## Documents
<!-- topic: documents -->

A document is something your app renders for people to keep, such as an
invoice as a PDF. The `documents` capability renders a typed template to
HTML or PDF, exactly as email renders a message. A document is a
`type X = Document { ... }` whose fields are the values its template
uses. It lives in its domain's `documents/` folder, beside its `.html`
template. The template follows the strict
[template rules](#what-a-template-may-not-do), reads `translations.` in
the locale it is given, and is wrapped in the optional
`shared/documents/layout.html`.

Here an invoice is rendered to a PDF by a subscriber, through Gotenberg,
and kept as a file:

```jadpo fragment
// domains/billing/documents/invoice_document.jadpo
/// An invoice as the customer receives it.
type InvoiceDocument = Document {
    number: InvoiceNumber
    customer_name: PersonName
    total: PresentationText
}

// app.jadpo
application {
    connections: {
        pdfs: {
            package: Gotenberg.v8
            domains: [billing]
        }
    }
    documents: {
        connection: pdfs
    }
}

// domains/billing/actions/render_invoice.jadpo
var pdf: Bytes = attempt documents.pdf(InvoiceDocument { ... }, locale: customer.locale)
var file: File = attempt invoices.put(pdf, FileType.pdf)
```

- `documents.pdf(document, locale:)` returns the PDF's `Bytes`.
  `documents.html(document, locale:)` returns the rendered HTML, for
  previews.
- `documents: { layout: ... }` names the shared document layout file.
  Leave it out and `shared/documents/layout.html` is used if it exists,
  and Jadpo's plain layout otherwise.
- The connection turns HTML into a PDF. It is Gotenberg as a container
  package, which `jadpo dev` starts in development
  ([Services](11-integrations.md#services)), or a hosted API's package,
  such as DocRaptor or PDFShift. `documents:` with no connection that
  provides `documents` is a compile error.
- Rendering is heavy work, so it runs in subscribers, never in a route,
  and no more renders run at once than the provider's `max_concurrent:`
  allows ([How polls and `max_concurrent:` run](11-integrations.md#how-polls-and-max_concurrent-run)).
  A `documents.pdf` call reached from a route is a compile error naming
  the subscriber shape.
- In tests the fake returns a placeholder PDF, and a test can read the
  rendered HTML
  ([Provider calls and capabilities in tests](13-testing.md#provider-calls-and-capabilities-in-tests)).

## Search
<!-- topic: search -->

Give an entity a `search:` section and people can search its records by
words. The compiler generates the index, keeps it up to date and
generates its migration, and a query finds records with `matches`. By
default the search runs on PostgreSQL's full-text search, so you need
nothing else. Search is the `search` capability, so an engine's package,
such as Meilisearch, Typesense, Algolia or OpenSearch, can fill it
instead.

```jadpo check project=search file=app.jadpo hidden
application {}
```

```jadpo check project=search file=domains/planning/types/todo_title.jadpo hidden
/// A todo's title.
type TodoTitle = Text {
    max_length: 200
}
```

```jadpo check project=search file=domains/planning/types/todo_notes.jadpo hidden
/// What a todo says beyond its title.
type TodoNotes = Text {
    max_length: 2000
}
```

```jadpo check project=search file=domains/planning/entities/todo.jadpo
use planning/types { TodoNotes, TodoTitle }

/// Something to do.
entity Todo {
    id: Uuid
    title: TodoTitle
    notes: TodoNotes?

    search: {
        fields: [title, notes]
        language: SearchLanguage.english
    }
}
```

```jadpo check project=search file=domains/planning/types/todo_view.jadpo hidden
use planning/entities { Todo }

/// A todo in a list.
type TodoView = Output {
    pick: [Todo.id, Todo.title]
}
```

```jadpo check project=search file=domains/planning/types/todo_search.jadpo
/// What someone typed into the search box, and where its page starts.
type TodoSearch = Input {
    phrase: SearchText
    page_size: PageSize {
        default: 2
    }
    after: PageCursor {
        optional: true
    }
}
```

```jadpo check project=search file=domains/planning/queries/search_todos.jadpo migrate=initial
use planning/entities { Todo }
use planning/types { TodoSearch, TodoView }

/// The todos whose words match a search, best first.
query search_todos(asked: TodoSearch) -> Page<TodoView> {
    from: Todo
    where: Todo matches asked.phrase
    page: {
        size: asked.page_size
        after: asked.after
    }
}
```

```jadpo check project=search file=domains/planning/actions/add_todo.jadpo hidden
use planning/entities { Todo }
use planning/types { TodoNotes, TodoTitle }

/// Adds a todo.
action add_todo(title: TodoTitle, notes: TodoNotes?) -> Todo {
    return Todo.create {
        set: {
            title: title
            notes: notes
        }
    }
}
```

- **The fields.** `fields:` lists text fields of the entity (a type built
  on `Text`, kept in one text column), by name. Fields listed first count
  most when ordering results: the first field's words weigh most, then
  the second's and the third's, and the fourth and every field after it
  weigh least, PostgreSQL's four weights. A field that is `none` has no
  words.
- **The language.** `language:` is one of `SearchLanguage`'s, PostgreSQL
  16's text search languages: `arabic`, `armenian`, `basque`, `catalan`,
  `danish`, `dutch`, `english`, `finnish`, `french`, `german`, `greek`,
  `hindi`, `hungarian`, `indonesian`, `irish`, `italian`, `lithuanian`,
  `nepali`, `norwegian`, `portuguese`, `romanian`, `russian`, `serbian`,
  `simple`, `spanish`, `swedish`, `tamil`, `turkish` and `yiddish`. It
  reduces each word to its stem, so "packing" finds "pack", and leaves out
  its stop words, such as "the". `simple` stems nothing and leaves nothing
  out. A PostgreSQL search always says its language: there is no default.
- **The phrase.** `matches` takes a `SearchText`
  ([Values and types](02-values-and-types.md)), what people type into
  search boxes: 1 to 200 characters with no line breaks, tabs or other
  control characters. It is read as words, quoted phrases and `-word`,
  never raw query syntax: `pack "the van" -books` finds records with
  "pack" and the phrase "the van" but not "books". Every word must match;
  an `or` someone types is a word like any other, and a stop word in most
  languages. A phrase of only stop words, or of no words, finds nothing.
- **The order.** With no `order:`, the best matches come first, then by
  `id`, in keyset pages like every other list
  ([Pages](06-reads-and-writes.md#pages)): a page's cursor holds the last
  record's rank and `id`. A record's rank is worked out from its own
  words and the phrase alone. With an `order:`, the records that match
  come in that order instead.
- **Where `matches` stands.** On a `where:` line of a list or a page,
  alone or joined to other conditions by `and`, once in a query. Not
  under `not` or `or`, not in `where_present:`, an include or an
  aggregate query, and not beside a `nearest` or `geo.distance` order
  (`query.condition_unsupported`).
- `search:` replaces the full-text index `kind:`
  ([Special kinds of index](05-data.md#special-kinds-of-index)): one
  declaration, not an index plus hand-written conditions.

The test adds four todos and searches them a page of two at a time.
"Pack books" matches in its title and its notes, so it comes first:

```jadpo check project=search file=domains/planning/queries/search_todos.test.jadpo
use planning/entities { Todo }
use planning/types { TodoSearch, TodoView }
use planning/actions { add_todo }

/// Titles joined by commas.
type TitleList = Text {}

/// The titles a page holds, in order.
function titles(page: Page<TodoView>) -> TitleList {
    return TitleList(text.join(list.map(page.items, item -> item.title), ", "))
}

test "the best matches come first, a page at a time" {
    var in_notes: Todo = add_todo("Call the bank", "ask about packing boxes")
    var in_title: Todo = add_todo("Pack the van", none)
    var in_both: Todo = add_todo("Pack books", "packing tape too")
    var watering: Todo = add_todo("Water plants", none)
    var first: Page<TodoView> = search_todos(
        TodoSearch {
            phrase: "packing"
        }
    )
    assert titles(first) == "Pack books, Pack the van"
    var second: Page<TodoView> = search_todos(
        TodoSearch {
            phrase: "packing"
            after: first.next
        }
    )
    assert titles(second) == "Call the bank"
    assert second.next == none
    assert in_notes.title == "Call the bank"
    assert in_title.title == "Pack the van"
    assert in_both.title == "Pack books"
    assert watering.title == "Water plants"
}

test "a quoted phrase, a word left out, and or as a word" {
    var van_todo: Todo = add_todo("Pack the van", none)
    var books: Todo = add_todo("Pack books", "the van is full")
    assert van_todo.title == "Pack the van"
    assert books.title == "Pack books"
    var phrase: Page<TodoView> = search_todos(
        TodoSearch {
            phrase: "\"pack the van\""
            page_size: 10
        }
    )
    assert titles(phrase) == "Pack the van"
    var no_books: Page<TodoView> = search_todos(
        TodoSearch {
            phrase: "pack -books"
            page_size: 10
        }
    )
    assert titles(no_books) == "Pack the van"
    var both_words: Page<TodoView> = search_todos(
        TodoSearch {
            phrase: "books or van"
            page_size: 10
        }
    )
    assert titles(both_words) == "Pack books"
    var stop_words: Page<TodoView> = search_todos(
        TodoSearch {
            phrase: "the"
            page_size: 10
        }
    )
    assert stop_words.items == []
}
```

### How it is kept

The words are a column of the entity's table that PostgreSQL works out
from the fields on every write, `jadpo_search`, so they can never
disagree with the record, and a GIN index finds the records that match.
`jadpo migrate new` writes both; the migration for the todos above holds:

```sql
CREATE TABLE "todo" (
    "id" uuid NOT NULL,
    "title" text NOT NULL,
    "notes" text,
    "jadpo_search" tsvector GENERATED ALWAYS AS (setweight(to_tsvector('english'::regconfig, coalesce("title", '')), 'A') || setweight(to_tsvector('english'::regconfig, coalesce("notes", '')), 'B')) STORED,
    CONSTRAINT "todo_pkey" PRIMARY KEY ("id")
);
CREATE INDEX "todo_jadpo_search_idx" ON "todo" USING gin ("jadpo_search");
```

- **No decision to make.** Adding `search:` to an entity that already
  has records adds the column, which PostgreSQL fills for every record
  there, and its index. Changing `fields:` or `language:` makes the
  column again from them, its index with it; removing `search:` drops
  both. None needs a backfill or an acceptance, since the column holds
  nothing that isn't in the fields.
- **The column's name is Jadpo's.** An entity with `search:` can't have a
  field stored as `jadpo_search`.
- **Lifecycle.** Like every index the compiler derives, the index holds
  only the records the entity's `visible:` lets queries see
  ([Indexes](05-data.md#indexes)).

`jadpo sql search_todos` shows the statement, the rank selected beside
the record's fields and each page starting after the last one's rank and
`id`:

```sql
SELECT "id", "title", ts_rank("jadpo_search", websearch_to_tsquery('english'::regconfig, $1))::float8 AS "jadpo_rank"
FROM "todo"
WHERE "jadpo_search" @@ websearch_to_tsquery('english'::regconfig, $1)
  AND ($2 IS NULL OR ts_rank("jadpo_search", websearch_to_tsquery('english'::regconfig, $1))::float8 < $2 OR (ts_rank("jadpo_search", websearch_to_tsquery('english'::regconfig, $1))::float8 = $2 AND "id" > $3))
ORDER BY "jadpo_rank" DESC, "id" ASC
LIMIT $4
```

### Search and policy

A search is a query, so policy filters its records exactly as it filters
any query's, in the same statement and before the page is cut
([When policy says no](09-policy.md#when-policy-says-no)). A search never
shows a record or a field its caller can't read:

- **Records.** Only the records the caller may read are searched. A
  record's rank comes from its own words and the phrase, never from
  other records, so a record the caller can't see never moves one they
  can, never changes where a page ends and is never counted: a `Page`
  holds no total.
- **Fields.** Which records match a search, and how well, shows the
  words of every field it lists. So each field in `fields:` must be one
  everyone who may run the query may read, as a condition's must
  ([What else shows a field](09-policy.md#what-else-shows-a-field));
  otherwise `policy.output_field_unproved` names the `matches` line.
- **No snippets.** A result is the query's output type: no highlighted
  text from a field the output doesn't pick.

Here each member reads only their own notes. Bob's note uses the word
three times, but Ann's search never finds it, and her two notes come one
to a page as if Bob's weren't there:

```jadpo check project=search-policy file=app.jadpo hidden
use notes/entities { Member }

application {}

principal {
    user: {
        subject: Member.handle
    }
}
```

```jadpo check project=search-policy file=authentication.jadpo hidden
authentication {
    browser_session: {
        cookie: "notes_session"
    }
}
```

```jadpo check project=search-policy file=domains/notes/types/handle.jadpo hidden
/// What a member signs in as.
type Handle = Text {
    min_length: 3
    max_length: 40
}
```

```jadpo check project=search-policy file=domains/notes/types/note_text.jadpo hidden
/// What a note says.
type NoteText = Text {
    max_length: 2000
}
```

```jadpo check project=search-policy file=domains/notes/failures/handle_taken.jadpo hidden
/// Another member signed up with the handle first.
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
```

```jadpo check project=search-policy file=domains/notes/entities/member.jadpo hidden
use notes/types { Handle }

/// Someone who signs in.
entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }
}
```

```jadpo check project=search-policy file=domains/notes/entities/note.jadpo
use notes/entities { Member }
use notes/types { NoteText }

/// A member's note, which only they may read.
entity Note {
    id: Uuid
    owner_id: Member.id {
        on_delete: OnDelete.cascade
    }
    body: NoteText

    policy: {
        create: [owner_id]
        read: [owner_id]
    }

    search: {
        fields: [body]
        language: SearchLanguage.english
    }
}
```

```jadpo check project=search-policy file=domains/notes/types/note_view.jadpo hidden
use notes/entities { Note }

/// A note in a list.
type NoteView = Output {
    pick: [Note.id, Note.body]
}
```

```jadpo check project=search-policy file=domains/notes/queries/search_notes.jadpo migrate=initial
use notes/entities { Note }
use notes/types { NoteView }

/// The caller's notes that match a search, best first, one at a time.
query search_notes(user: Principal.user, phrase: SearchText, after: PageCursor?) -> Page<NoteView> {
    from: Note
    where: Note matches phrase
    page: {
        size: 1
        after: after
    }
}
```

```jadpo check project=search-policy file=domains/notes/actions/register.jadpo hidden
use notes/entities { Member }
use notes/types { Handle }
use notes/failures { HandleTaken }

/// Registers a member; anyone may.
action register(handle: Handle) fails HandleTaken -> Member {
    return attempt Member.create {
        set: {
            handle: handle
        }
        conflict: {
            Member.handle: HandleTaken
        }
    }
}
```

```jadpo check project=search-policy file=domains/notes/actions/write_note.jadpo hidden
use notes/entities { Note }
use notes/types { NoteText }

/// Writes a note for its owner.
action write_note(owner: Principal.user, body: NoteText) -> Note {
    return Note.create {
        set: {
            owner_id: owner.id
            body: body
        }
    }
}
```

```jadpo check project=search-policy file=domains/notes/queries/search_notes.test.jadpo
use notes/entities { Member, Note }
use notes/types { NoteView }
use notes/actions { register, write_note }

test "a search finds only the caller's notes, and others' never move them" {
    var ann: Member = attempt register("ann")
    var bob: Member = attempt register("bob")
    var budget_plan: Note = write_note(Principal.user(ann), "budget plan")
    var budget_review: Note = write_note(
        Principal.user(ann),
        "budget review, and the budget after it"
    )
    var bobs_note: Note = write_note(Principal.user(bob), "budget budget budget")
    assert budget_plan.body == "budget plan"
    assert budget_review.body == "budget review, and the budget after it"
    assert bobs_note.body == "budget budget budget"
    var first: Page<NoteView> = search_notes(Principal.user(ann), "budget", none)
    assert list.map(first.items, item -> item.body) == ["budget review, and the budget after it"]
    var second: Page<NoteView> = search_notes(Principal.user(ann), "budget", first.next)
    assert list.map(second.items, item -> item.body) == ["budget plan"]
    assert second.next == none
    var bobs: Page<NoteView> = search_notes(Principal.user(bob), "budget", none)
    assert list.map(bobs.items, item -> item.body) == ["budget budget budget"]
    assert bobs.next == none
}
```

### Search and personal data

A field marked `personal:` may be searched
([Marking personal fields](15-personal-data.md#marking-personal-fields)).
Its words are kept only in its record's own row, worked out from the
field, so whatever clears or deletes the field (erasure, anonymising,
retention) clears its words in the same statement, and nothing is copied
anywhere else. A field of a sensitive category, such as
`PersonalData.health`, isn't searched yet: every read of a sensitive
field is recorded in the [access records](15-personal-data.md#access-records),
which aren't built, so it is `toolchain.not_built_yet` in `fields:`.
An [encrypted field](15-personal-data.md#encrypted-fields) is never
searched.

### An engine instead of PostgreSQL

`search: { fields: [...]  store: products_index }` names a connection
whose package provides `search`. Leave `store:` out and the search is
PostgreSQL's. Not built yet: `store:` is `toolchain.not_built_yet` where
it is written ([What is built](#what-is-built)). The engine's package does the whole job: it upserts a
record's searchable fields, removes a record, and queries by words with
equality filters.

```jadpo fragment
// domains/catalogue/entities/product.jadpo
entity Product {
    ...
    search: {
        fields: [name, description]
        store: products_index
    }
}
```

- **Only equality filters reach an engine.** A `matches` query on an
  entity whose search uses an engine may only have `where:` lines that
  test equality on fields the index holds. Anything else is a compile
  error naming PostgreSQL search, which can filter any way.
- **Ids from the engine, records through policy.** The engine returns
  ranked ids, already filtered by tenant. Jadpo then reads those records
  through the query, with policy applied as for any query
  ([Policy](09-policy.md)), so a result is never one the caller couldn't
  read.
- **Kept up to date after commit.** Every committed change to a
  searchable field reaches the engine through the outbox, and is retried
  like any delivery
  ([Delivery: at least once, safe to repeat](12-jobs-and-events.md#delivery-at-least-once-safe-to-repeat)).
  Erasing a person removes their records from the index
  ([Erasure](15-personal-data.md#erasure)). `jadpo search reindex <entity>`
  rebuilds the index.
- **Engine features** (synonyms, ranking rules, typo tolerance settings)
  are the package's own operations, called from a domain the connection
  lists.
- **Development and tests use PostgreSQL search** whatever production
  uses, so nothing leaves the machine and no engine is needed to run a
  test.

## Bot protection
<!-- topic: bot-protection -->

Bot protection stops scripts from using a public form, such as a sign-up
or a demo request. The browser shows a challenge widget, and your app
checks the token it gives. `bot_check` is a capability with one
operation, checking a token: pass or fail. A package fills it, such as
Cloudflare Turnstile, hCaptcha or reCAPTCHA, and you name its connection
in `application { }`. A route, or a generated feature, asks for the
check with `bot_check: true`:

```jadpo fragment
// app.jadpo
application {
    connections: {
        turnstile: {
            package: Turnstile.v0
            credential: config.turnstile_secret
        }
    }
    bot_check: {
        connection: turnstile
    }
}

// domains/sales/routes/demo_requests.jadpo
POST / {
    authentication: RouteAuthentication.none
    input: RequestDemo
    status: SuccessStatus.no_content
    run: request_demo(input)
    bot_check: true
}
```

- **Jadpo's own public endpoints are checked too,** the magic-link and
  privacy requests, whenever `bot_check:` names a connection.
- **The frontend shows the widget.** The token travels in the
  `X-Bot-Check` header, never in the route's input, and is checked
  before the action runs, within the request's deadline.
- **A failed check** is the built-in failure `bot_check_failed` (403),
  worded from translations like every failure
  ([Failures for client apps](10-configuration-and-time.md#failures-for-client-apps)).
- **reCAPTCHA's score** gets a threshold setting on its connection.
- **An unreachable provider refuses the request,** so the check fails
  closed. Letting requests through while it is down needs an
  [acceptance](18-records-and-manifest.md#acceptances), listed in the
  approval report.
- **Development and tests pass every check** through a local driver,
  and a test can script `bot_check_failed`
  ([Fakes for integrations](13-testing.md#fakes-for-integrations)).
- `bot_check: true` with no `bot_check:` connection is a compile error
  naming the setting to add.

## Outgoing webhooks
<!-- topic: outgoing-webhooks -->

Outgoing webhooks let your app's customers receive its events at an
address of their own. They are built on `publishing`
([Where events travel](12-jobs-and-events.md#where-events-travel)): a
customer registers an address, and the public events you choose are
delivered to it, typed from the event, signed and retried.

The customers' addresses can't be listed as connections, so outgoing
webhooks get one network exception, owned by the runtime. Its guards are
fixed:

- HTTPS only;
- no private, loopback or link-local addresses, checked on the resolved
  address at every attempt;
- no redirects followed.

The exception is listed once in the network report
([The network report](11-integrations.md#the-network-report)). Nothing
else in your app, a package included, can reach an address that isn't a
declared connection.

Outgoing webhooks aren't built yet ([What is built](#what-is-built)).

## Not built in
<!-- topic: not-built-in -->

Some features other frameworks offer aren't part of Jadpo, because the
compiler gains little by knowing about them. You build them from your
app's own data and packages, or they come later:

| Feature | Why not, or not yet |
|---|---|
| Realtime subscriptions | Policy on a live stream of changes is a large design, so it comes later. Until then, clients poll with the revision header ([Write revisions](07-http.md#write-revisions)) |
| Generated admin screens | Jadpo makes backends, not interfaces. Operators use `jadpo` commands ([The `jadpo` command](14-tooling.md#the-jadpo-command)); a screen can come later on the same operations |
| Payments | Money moves at the provider, so an integration plus [webhooks](07-http.md#webhooks) ([Integrations](11-integrations.md)) is the right shape, and the compiler gains nothing more |
| Content management | Content is application data |
| Product analytics | Events and metrics above feed whichever tool a team uses |
| Quotas and plan limits | A plan's limits are application data, checked by an action; counting uses [counters](06-reads-and-writes.md#counters-and-other-one-step-updates) |
| Background job dashboards | Already the operator commands ([Commands for background work](14-tooling.md#commands-for-background-work)) |

## Rate limits on Workers
<!-- topic: rate-limits-on-workers -->

A Worker's instances share no memory, so on Cloudflare Workers your rate
limits need a store that every instance can reach. This topic is how
that store works ([Rate limits](#rate-limits) has the limits themselves).

### Choosing the store

`rate_limits: { store: }` in `application { }` names one connection,
with `Driver.redis` or `Driver.cloudflare_rate_limits`, and nothing else
(`rate_limits.store_invalid`).

A deployable built for Workers that serves an API needs a store
(`rate_limits.store_required`). One that serves no API has no routes to
limit, and counts the requests it refuses beside its health checks in
its own memory.

### Cloudflare's binding: `Driver.cloudflare_rate_limits`

`Driver.cloudflare_rate_limits` runs only on Workers, so a native
deployable that serves an API with that store is refused
(`deployable.target_unsupported`), naming Redis. Its `namespace:` is a
whole number of at least 1.

- **One binding for each limit.** A binding counts one limit over one
  window, so each limit your app counts is a binding of its own. It is
  named for the connection and the limit, such as `LIMITS_120_PER_60S`,
  and numbered from `namespace:` in order of window and then of
  requests. `jadpo build` writes each into the Wrangler file as
  `ratelimits: [{ name, namespace_id, simple: { limit, period } }]`.
  Adding a limit can renumber the ones after it, which loses at most a
  window's counts, and those are harmless to lose.
- **Windows** of 10 and 60 seconds. Every limit counted today is a
  minute's (600 for a signed-in caller, 120 for an address), so all of
  them map onto the binding. The error for any other window comes with a
  route's own `rate_limit:`, the first thing that can write one.
- **`Retry-After`** is the window's whole length, since the binding says
  only whether a request is within the limit, not when its window ends.

### When the binding can't be asked

- **Falling back.** If the Worker hasn't got the binding, or the binding
  throws or rejects, the request is counted in the isolate's own memory,
  so no request fails for it.
- **Not waited for.** A binding that hasn't answered within 500 ms isn't
  waited for: the request is counted in the isolate's memory, as for one
  that throws. The binding answers from a count kept where the Worker
  runs, so one slower than that is in trouble, and a request isn't held
  up by it.
- **Logged once.** Each binding that can't be asked is logged once for
  each isolate, as `rate_limits.fallback`, with the binding's name and
  why (`missing`, `failed` or `timed_out`), never a key. An operator sees
  that a Wrangler file lost its bindings, without a log line for every
  request.

### A Redis store on Workers

Until Redis's driver comes with caches, a deployable whose store is
Redis counts in its own memory, the same fallback as when the store
can't be reached. A Worker's instances share no memory, so a deployable
built for Workers that serves an API with a Redis store is
`rate_limits.counted_in_memory`. It is a judgement call, since Jadpo has
no warnings. You answer it with Cloudflare's binding, or with a
`decline` line above the deployable that keeps Redis and counts in
memory until its driver comes. A native deployable with a Redis store is
unaffected.

The windows and the Wrangler entries aren't yet checked against the real
edge: the workerd pinned in `tools/fixtures/workers/` has no binding of
its own, and its types give only `limit({ key })` and `success`
([What is built](#what-is-built)).

## The dev inbox and the health score
<!-- topic: the-dev-inbox-and-the-health-score -->

`jadpo dev` keeps every email your app sends in a local inbox, and
scores each one for the common mistakes that send mail to spam. This
topic is how the inbox, its commands and the score work
([Email](#email) has the overview).

### One catcher, kept on disk

`jadpo dev` gives the connection `email:` names the same catching driver
a test's host gives it, so each message is handed over whole, a mail
package's included. Each is kept in `build/dev/inbox/` as a JSON file,
with its attachments' bytes beside it, written whole and then renamed.
`jadpo email` reads the inbox from another terminal, while `jadpo dev`
runs or after.

The last 200 mails are kept. Their ids are `mail_<n>`, and an id is
never given twice, even after `jadpo email clear`. A mail package's
other operations still go to its fake. An SMTP connection's settings
that nothing gives get stand-ins, as a provider's do, since its server
is never reached.

### Codes and links only in the inbox

A mail is kept whole, its sign-in link and code included, because
development needs them. `jadpo dev`'s line for a caught mail names its
id, its email and its address, never its subject or its words, and the
call log never sees it.

### The inbox page

`jadpo dev` serves the page on `127.0.0.1`, on the port after the app's,
or on one the system chooses when that is taken. Its address is on the
ready screen once the app sends mail, and it answers only requests
addressed to `localhost`. The page is read-only, as the console is
([The developer console and MCP](17-debugging-and-review.md#the-developer-console-and-mcp)).

It lists the mail newest first. The mail you choose is shown with its
HTML in a sandboxed frame on white, its links opening a new tab, then
its text, its links numbered with the `jadpo email open` command for
each, its attachments and its health. It looks for new mail every two
seconds. It shows the id of the trace that sent each mail; the link to
the trace comes with `jadpo trace`
([The request timeline in `jadpo dev`](17-debugging-and-review.md#the-request-timeline-in-jadpo-dev)).

### `jadpo email`

- `list`, the default, shows the mail oldest first, and `--to` narrows
  it.
- `show <id>` shows the text part, or the HTML with `--html` or when
  there is no text.
- `links <id>` lists the links, numbered from 1.
- `open <id> <link>` opens a link, by number or written out, in
  `$BROWSER` or the system's opener. It opens only `http://` and
  `https://` links.
- `clear` empties the inbox.

An id is written `mail_3` or `3`.

### The score's checks

The score runs seven checks. Each gives the same answer every time, and
each comes with its fix:

1. No plain-text part beside the HTML.
2. Images with fewer than 20 characters of text.
3. HTML that doesn't parse cleanly: an end tag that closes nothing or
   closes another element first, an element never closed (other than
   those HTML lets a writer leave open), or a tag never finished.
4. A link that isn't absolute `https://`. This covers every `href` and
   every web address in the text. `mailto:`, `tel:` and `sms:` pass, and
   in the dev inbox so do links to this machine.
5. A link through a shared shortener, from a fixed list.
6. A subject of four or more letters that have a case, none of them
   lower case.
7. HTML over 102 KB.

A score is the problems found and "N of 7 checks pass", with the note
that it can't predict inbox placement. The check for an unsubscribe
header waits for bulk mail, which no email is yet.
[SpamAssassin in the health score](#spamassassin-in-the-health-score)
adds SpamAssassin's rules.

### What `jadpo check` scores

`jadpo check` scores what a template's own text answers, as
`email.health_problem`, which you can decline on the email's or
partial's type:

- an `.html` with no `.txt`;
- a link written out whole, not built inside `{{ }}`, links to this
  machine included;
- the HTML's structure with its `{{ }}` left out, only in a template
  with no `{{ if }}`, since a condition may open an element in one
  branch and close it in another;
- an email's own HTML with images and neither text nor a value written
  as text;
- a template over 102 KB;
- a subject whose words, as a locale writes them, are in capitals.

What a template writes from its fields is scored once it is rendered,
in the dev inbox. One `decline` above the type answers every finding
there, as a decline covers its rule on its declaration
([Declining a judgement call](01-project-layout-and-modules.md#declining-a-judgement-call)).

### `jadpo email check-domain`

`jadpo email check-domain` reads the domain you give it. Without one it
reads `email: { from: }`'s: the address written out, or the setting's
value from the environment, `.env.local` or its default. It checks:

- **SPF**, at the domain: one `v=spf1` record, ending `-all` or `~all`,
  its own terms needing at most 10 lookups. `+all` fails, and no record,
  or `?all`, is an alert.
- **DKIM**, at `<selector>._domainkey.<domain>`, only with `--selector`:
  a key with a `p=` that isn't empty.
- **DMARC**, at `_dmarc.<domain>`: one `v=DMARC1` record. `p=quarantine`
  or `p=reject` passes, and `p=none` is an alert.

What a provider needs beyond these, such as its include and its
selector, waits for a mail package to declare it. The records are asked
of the name server `/etc/resolv.conf` names, over UDP, then over TCP
when the answer is cut short, behind a seam its tests fake. The command
exits 0 with its verdicts, and `--json` says whether every record
`passed`.

## SpamAssassin in the health score
<!-- topic: spamassassin-in-the-health-score -->

When SpamAssassin is installed on your machine, `jadpo dev` adds its
rules to each mail's health score. Without it, the score is Jadpo's own
seven checks ([The dev inbox and the health score](#the-dev-inbox-and-the-health-score)).

### When it runs

`jadpo dev` hands each mail to `spamassassin` on the `PATH` as it
catches the mail, before keeping it, off the worker's thread. The mail
is as the SMTP driver writes it, with the `Message-ID:` a submission
server adds. What SpamAssassin found is kept with the mail, and a mail
is never scored again.

Jadpo runs only `spamassassin`, never `spamc`, whose daemon's network
tests aren't Jadpo's to switch off. It runs with `-L -t -x` and
`bayes_auto_learn 0`: no network tests, no preferences made and nothing
learnt. A run is given up after 30 seconds. The mail goes to its
standard input only, its standard error is thrown away, and nothing it
read is logged.

### What is kept

Jadpo keeps the last report SpamAssassin prints: its score, the score it
calls spam at, and each rule that matched with its points and
description, the most points first. A line quoting the mail
(`[URIs: …]`) is left out. Decimals stay as SpamAssassin wrote them, as
text in `--json`.

### How it changes the score

A mail SpamAssassin calls spam, its score at or over its threshold, has
one more problem. "N of 7 checks pass" stays Jadpo's own count.
`jadpo email show` and the page's health view list SpamAssassin's rules
after Jadpo's checks, with points over zero in the warning colour. The
score's line and the page's strip name SpamAssassin's verdict. A mail is
called "clean" only when SpamAssassin scored it too.

### When it wasn't run

SpamAssassin may not be installed, the mail may have been caught before
it was run, or there may be no report, because a run failed, was too
slow, or printed a report that couldn't be read. Each case says that
SpamAssassin wasn't run or couldn't score the mail, that its rules
aren't in the score, and what to do. None says the mail is clean.

Jadpo's own tests of this fake SpamAssassin's run, so nothing needs
installing for them.

## Mail packages, partials and `email.sent`
<!-- topic: mail-packages-partials-and-email-sent -->

This topic has the exact rules for partials, for `email.sent` in tests,
and for a mail package that fills the `email` capability
([Email](#email) has the overview).

### A partial's locale goes with it

Every `EmailPartial` has `locale:` before its own fields, as every email
has `to:` and `locale:`. The template that includes the partial gives
it, never the include itself: declaring `locale:` on a partial, or
giving it in an include, is refused.

A template includes a partial by its type's name in `lower_snake_case`,
`{{ include button(...) }}`, giving each field once, by name. A field
that may be `none` may be left out. Partials' names are unique in the
app, as emails' are.

### Which partials a template reaches

A template reaches the partials in `shared/emails/` and those in its own
domain's `emails/`. The shared layout reaches only the shared ones.
Another domain's partial is `email.partial_invalid`, its `next:` saying
to move the partial to `shared/emails/`.

### What an include writes

An HTML template includes the partial's `.html`, and a text one its
`.txt`, which must exist. What the partial writes goes in as it is,
since its own values are escaped already. A partial may include others,
but never lead back to itself, directly or through others.

A partial's field that none of its templates reads is
`email.field_unused`. A partial that no template includes isn't
reported. An `EmailMessage` in `shared/emails/` is `layout.wrong_folder`,
because an email is sent by a domain.

### `email.sent` in a test

`email.sent` is a `List<SentEmail>` of the mail caught so far, in the
order it was sent. A `SentEmail` holds:

- `to`, `from` and `subject`;
- `html` and `text`, each a `Text?`;
- `links`: each `href` in the HTML and each `http://` or `https://`
  address in the text, once each, in order;
- `attachments`: their file names;
- `sign_in_token`, a `SignInToken?`, and `sign_in_code`, a
  `SignInCode?`: the sign-in email's link token and code, which a route
  test posts to the confirmation
  ([In development and tests](08-identity.md#in-development-and-tests)).
  Both are `none` on every other email. Only a host that catches mail
  is handed them, beside the message, so no driver ever sees them.

A local variable named `email` is read before the namespace, since an
address is often called that. A test that reaches no store reads an
empty list.

### One fake for every connection that fills `email`

A test's host catches the rendered message whole for the connection
`email:` names, a mail package's included. That package's own fake then
doesn't answer the send.

### A mail package's mapping

A mail package fills `email` with a `provides/email.jadpo`, which gives
`send:` (with `call:`, `input:`, `receipt:` and `failures:`) and
`limits: { attachments: }`.

- **`input:`** keys are the names of the operation's input fields. Their
  wire names come from the types, as `provide receiving`'s do
  ([Capabilities](11-integrations.md#capabilities) shows Postmark's wire names).
- **Every part of the message** is given somewhere: `from`, `to`,
  `subject`, `html`, `text`, and the attachments as
  `{ each: message.attachments  as: { } }`, with each one's `item.name`
  and `item.content`. Its media type, `item.content_type`, is optional.
  `html` and `text` go only to fields that may be `none`, the content
  only to `Bytes`, sent in base64, and the rest to text.
- **`receipt:`** is optional and names text in the answer. It is read,
  and not kept anywhere yet.
- **`failures:`** maps the operation's failure codes onto
  `EmailFailure.recipient_rejected` or `.message_rejected`. A failure it
  doesn't map ends the delivery as the package's own.
- **`limits:`** is required.

### Attachments within the limit

Through a mail package, each `File` field of an email gives
`max_size:`, and their sum is at most the package's limit
(`email.attachment_limit`). A send whose attachments are larger still is
refused as `message_rejected` before it goes out. Through SMTP, which
declares no limit, `max_size:` is optional.

### The connection

`email:` may name a package connection whose package has
`provides/email.jadpo`. A connection that only `email:` names needs no
`domains:`, since no domain's action calls it. The delivery calls the
operation as any provider call is made, with no key, so a server's error
after the message went out parks it as `outcome_unknown`.

An email without an `.html` or a `.txt`, or an attachment without a
media type, sends the provider's input without that field, rather than
with `null`, which a provider needn't accept.

### Resend

Resend is the second mail connection, in `packages/resend`. As Resend's
documentation gives it (resend.com/docs, "Send Email" and "Errors"):

- It sends with `POST /emails` and `Authorization: Bearer`, giving
  `from`, `to`, `subject`, `html` and `text` (both optional, the text
  made from the HTML when left out), and `attachments` of
  `{ filename, content (base64), content_type? }`.
- An email may be 40 MB once its attachments are in base64, so its limit
  is 30 MB of files.
- Its refusals of the message (`validation_error`,
  `missing_required_field`, `missing_required_parameter`,
  `invalid_attachment`, `invalid_parameter`) are `message_rejected`.
- Its rate and quota limits (`rate_limit_exceeded`,
  `daily_quota_exceeded`, `monthly_quota_exceeded`, all `429`) mean
  nothing was sent, so they are `unavailable` and retried, never a
  failure of the message.
- A refused key (`401`, `403`) is `misconfigured`.

Its connection gives `host:`, `https://api.resend.com`.

## Your own magic-link email
<!-- topic: custom-magic-link-email (was levels-4-and-5-of-the-magic-link-email) -->

Your app can send its own magic-link email instead of Jadpo's, at
level 4 or level 5 of [Built-in emails](#built-in-emails). This topic is
how Jadpo fills and sends it. The checks are on the Identity page
([Magic links](08-identity.md#magic-links)).

### Still one delivery

Whichever level you choose, asking for a link leaves the same outbox
row: `jadpo.MagicLinkEmail`, with the address, the locale and the
request's id. The link and the code are made as it is sent.

- **At level 4** that delivery fills your app's email from them, `to:`
  the address and `locale:` the request's, with `expires_in` worded as
  Jadpo words it. It renders the email as any of your app's emails is
  rendered, in its layout.
- **At level 5** the delivery runs your app's action instead, giving it
  a `MagicLinkRequest`.

A delivery whose request was used, replaced or has expired sends
nothing, at either level.

### Each attempt its own link and code

Your app's action is retried as a subscriber's is, with the default
retries and no `retry:` of its own
([Retries and giving up](12-jobs-and-events.md#retries-and-giving-up)).
Each attempt makes a new link and code, replacing the last, so a message
that went out before a failure can't sign anyone in once a later one
has. An outside call within one attempt is sent again with the same link
and code, as any call is.

### Only the email holds the code

The email `email:` names is never emitted. A `SignInCode` is never kept,
answered, put in a failure or an event, or written as JSON, and
`MagicLinkEmail`'s fields are copied only into an email. What your own
sending does with the link and the code is up to you, as with any value
you send a provider. In development that call is logged with its values,
as every provider call is
([Provider calls in development: `jadpo calls`](17-debugging-and-review.md#provider-calls-in-development-jadpo-calls)).

## Jadpo's own magic-link email
<!-- topic: default-magic-link-email (was jadpo-s-own-magic-link-email) -->

With nothing written, Jadpo sends its own magic-link email, at level 1
of [Built-in emails](#built-in-emails). This topic is how that email is
written and sent.

### Written by the runtime

Jadpo's email has no templates. The runtime writes its HTML and text
from Jadpo's seven email messages, each worded in the email's locale:
your app's rewording first, then Jadpo's. `{app_name}` is the
application's `display_name:`, and `expires_in` is the duration in words
in that locale, such as "15 minutes". Without `locales { }` the email is
in `en_gb`, in Jadpo's wording.

### How it looks

The email is one neutral column at most 480 pixels wide, with inline
styles only and no images or fonts. Colours are left to the reader's
mail client, except the button's: a dark fill, `#1f2937`, under white
text, a contrast of about 15:1. In order, it has the heading, the line
naming the address, the button, and the code line with the six digits
set apart, monospaced, spaced and never wrapped, so they read and copy
easily. The expiry and ignore lines follow, a size smaller. The text
part has the same lines, with the link written out. The whole email is
in the locale's direction: `dir="rtl"` for a right-to-left locale.

### Its layout

With your app's layout, the HTML body goes into `layout.html` as its
`{{ content }}`, and the text into `layout.txt`. Without one, Jadpo's
plain layout is a minimal document: `<!DOCTYPE html>`, the locale's
`lang` and `dir`, and the subject as its title.

### Its delivery

It is delivered by a subscriber of its own, `email jadpo.MagicLinkEmail`,
tried as an email is. Its outbox payload is the address, the locale and
the sign-in request's id, never the link or the code, which are made as
it is sent ([Magic links](08-identity.md#magic-links)).

## Email templates and sending
<!-- topic: email-templates-and-sending -->

This topic has the exact rules for how an email's templates are checked
and how the email is sent ([Email](#email) has the overview).

### Rendered when it is sent

The outbox keeps an email's fields as its payload: its type's contract
JSON, within the outbox's 64 KB limit
([Limits and runtime faults](12-jobs-and-events.md#limits-and-runtime-faults)).
Its delivery renders it as it is sent: its templates, its subject and
its attachments. A rendered body never fills the outbox, and a retry
renders the same text from the same fields.

### A template is a callable

Each `.html` and `.txt` is read beside its type, as a prompt's text is
([Prompts as files](21-ai.md#prompts-as-files)). The body checker checks
it as a pure callable taking its email's fields, with `to` and `locale`
first, and lowers it to a typed body the executor runs, so its
expressions are Jadpo's own, typed once. The layout's callable takes
`content`, `subject` and `locale`.

### At least one template

An email has an `.html`, a `.txt` or both. These are
`email.template_missing`: an email with neither, a template named after
no email, and a file in `shared/emails/` other than the layout's two.

### What a template may do

A template may use literals, its names and their fields, comparisons,
`and`, `or`, `not`, enum values, translations and `text.format`.
Arithmetic, any other call, building a value, and reading a setting or
a constant are `email.template_not_allowed`. Translations and
`text.format` take the email's locale when `locale:` is left out.

`{{ locale.direction }}` is Jadpo's wording of `TextDirection`
(`jadpo.text_direction`): `ltr` or `rtl`, unless the root translations
file rewords it. `{{ app.display_name }}` is
`application { display_name: }`, plain text on one line.

### What is written out

- A value that may be `none` is refused.
- A number is written by `text.format` in the email's locale.
- An enum is written by its wording.
- `Html` is written as it is in an `.html` template, and refused in a
  `.txt` one.
- Everything else is escaped in HTML: `&`, `<`, `>`, `"` and `'` become
  character references.

### A subject in every locale

An email's entry in its domain's language files is required in every
supported locale that isn't in progress, as a message's wording is
(`translation.wording_missing`).

### A field nothing reads

A field nothing reads is `email.field_unused`, which you can decline.
`to`, `locale` and `File` fields, which are attachments, are never
reported, and a field the subject's wording writes in any locale counts
as read.

### The layout

When the layout exists, it wraps each body: the `.html` in
`layout.html` and the `.txt` in `layout.txt`. You can keep either file
without the other. Without a layout, "Jadpo's plain layout" is the
template's own text, sent as it is.

### `email:`

`email:` names a connection with `driver: Driver.smtp`, or a mail
package's connection that fills `email`. Its `from:` is an `Email`
setting or an address written out. Emitting an email with no `email:` is
`email.connection_required`.

### Its delivery

Each email type has a subscriber of its own, `email <domain>.<Type>`,
tried as a subscriber is by default: 10 attempts within 24 hours. Its
record alone says it went.

The delivery renders the email first, so an email that can't be
rendered is settled as a subscriber's fault is, and nothing is sent.
Then it is marked `sending` and sent with no key. The driver is given
`from`, `to`, `subject`, `html`, `text` and `attachments`: each
attachment's name, media type and bytes, read whole from its store, at
most 10 MB, as `store.read` reads.

### Development and tests

`jadpo test` gives the email connection a driver that catches each
message and answers that it went, so every email is rendered and nothing
leaves your machine. `jadpo dev` catches mail the same way, in the dev
inbox.

### Where a partial is declared

A partial such as
`type Button = EmailPartial { label: PresentationText  url: Url }` is
declared in `shared/emails/` or a domain's `emails/`, with its `.html`
and `.txt` beside it as an email's are, and called with its fields by
name.

## The seam every outside call goes through
<!-- topic: the-seam-every-outside-call-goes-through -->

Every call that leaves your app's process, to a capability or to a
provider, goes through one entry point in the runtime: the seam. This
topic is how the seam carries out
[Capabilities the built-in services use](#capabilities-the-built-in-services-use),
[A send whose outcome is unknown](#a-send-whose-outcome-is-unknown) and
[Outside calls hold no transaction](06-reads-and-writes.md#outside-calls-hold-no-transaction).

### One way out

Both executors call the same entry point. A driver is named by the
connection it serves ([Connecting in `app.jadpo`](11-integrations.md#connecting-in-appjadpo))
and answers in one of five ways:

- a value;
- one of its operation's own failures, which the call maps;
- *surely did nothing*;
- *may have acted*;
- *misconfigured*.

A call to a connection that no driver fills is `misconfigured` and is
never sent.

### Logged without its values

Each call is logged as it goes out, and again when it is answered. The
log has its connection and operation, and whether it may be sent again:
it changes nothing, it changes something under a key, or it changes
something with no key. It also has where its answer came from (the
driver, a fake or a replay), when it went out, how long it took, and how
it ended.

The log never has the call's input or its answer, which can hold secrets
and personal data. `jadpo calls`'s full values in development are kept
apart, by `jadpo dev`'s host only
([Provider calls in development: `jadpo calls`](17-debugging-and-review.md#provider-calls-in-development-jadpo-calls)).
A response carries its request's log.

### Recorded and replayed

A recording keeps each call's answer, in order, beside the random bytes
it keeps. A replay gives the answers back without the calls going out. A
replayed call that isn't the recording's next one is refused before it
goes out.

### `outcome_unknown` only where a repeat could act twice

A call that may have acted raises `outcome_unknown` when it changes
something with no key. One under a key, or one that changes nothing,
raises `unavailable`, since sending it again acts only once.

### A delivery can go to a capability

Besides running a subscriber's action, a delivery can hand its event
whole to an operation of the capability a connection fills, as an
`EmailMessage`'s delivery does. Its record alone says it was sent: there
is no action, so there is no handled record.

### Parked, never sent again blindly

A delivery is parked as `outcome_unknown` for an operator when its send
may have acted and has no key
([What an operator sees](12-jobs-and-events.md#what-an-operator-sees)).
That happens when its driver said so, when its run's time ran out while
the send was out, or when its worker was lost while it was out. An
unkeyed send is marked `sending` on its delivery record before it goes
out, so the sweep parks a delivery whose lease passed instead of
claiming it again.

- **A subscriber's action** that sent such a call and then ended in a
  way that is otherwise retried, such as a store that couldn't be
  reached or its time running out, is parked too, since a retry would
  send the call again. One that fails for good stays `failed`.
- **After a concurrent change**, an action that sent such a call is
  never run again
  ([All-or-nothing actions and concurrent changes](06-reads-and-writes.md#all-or-nothing-actions-and-concurrent-changes)).
  It stops as `outcome_unknown`, which a route keeps for its client's
  key and a delivery parks
  ([Several changing calls in one action](12-jobs-and-events.md#several-changing-calls-in-one-action)).
- **A send under a key**, or one that surely did nothing, is retried as
  any delivery is.
- **A lost worker.** A subscriber's action whose worker is lost while
  its call is out is handled by the step record
  ([Several changing calls in one action](12-jobs-and-events.md#several-changing-calls-in-one-action)).
- **A schedule's run** has no retry to stop, since its next run is the
  next tick's, so its run record keeps how it ended.

### No call inside an open transaction

What the action has only read is a snapshot, which the call closes. A
call made once the action has written, while its transaction is open, is
refused before it goes out, with a fault naming the call and saying to
make it before the action's first write
([Outside calls hold no transaction](06-reads-and-writes.md#outside-calls-hold-no-transaction)).
The checker proves the order. Calls made by a batch's items run inside
their batch's transaction, and are refused the same way.

### Fakes wrap the host

A fake is a wrapper around the host that puts a scripted driver in
place of some connections, and leaves everything else as the host's own.
A test runs on it, and `jadpo dev`'s host gives a package's fake the
same way. The runtime's Rust tests share one test host, which fakes wrap
the same way.

## Files: `put`, `read` and the disk driver in development
<!-- topic: files-put-read-and-the-disk-driver-in-development -->

This topic has the exact rules for `File` fields, `store.put`,
`store.read`, and the store `jadpo dev` and tests keep files in
([Files](#files) has the overview).

### What a `File` holds

A `File` holds the connection its bytes are in, the key the runtime made
for them, its name (none for one `store.put` made), its size and its
`FileType`. It is kept as its JSON in a `jsonb` column.

Only the runtime builds one: `File { ... }` is `type.not_record`. No
request carries a `File`, and no output sends one, since an output's
signed link comes with downloads. A program reads none of its parts yet.

### A `File` field's settings

- `store:` must name a connection that provides `files`.
- `max_size:` is required: a whole number of `b`, `kb`, `mb` or `gb`,
  each a thousand times the last. `5mb` is 5,000,000 bytes, which is why
  this page writes "100 MB" apart from "16 MiB".
- `types:` is optional.

A mistake in any of them is `data.file_field_invalid`. The three
settings on a field that isn't a `File` are `type.unknown_setting`.
Every write of the field holds the file it is given to them
([What a write checks](#what-a-write-checks)).

### `store.put(bytes, type)`

You call `store.put` only in an action, before its first write, as every
outside call is made. Its key is made from the invocation's randomness,
so a replay keeps it. It goes out as a keyed call, since writing the
same key again writes the same object, which never changes.

A store with no room answers `storage_full`, the call's one condition.
Its `attempt` maps it to a failure of kind `unavailable`; it isn't a
fault of a value, so `invalid:` doesn't cover it. A store that can't be
reached is the runtime's own `unavailable`.

### What a write checks

A `File` is kept in a field only when it fits the field's settings.
Every `create`, `update` and patch that sets a `File` field checks,
before its statement runs, that the file is:

1. in the store the field's `store:` names;
2. of a type its `types:` lists, when it lists any;
3. no larger than its `max_size:`.

A file that breaks one stops the write with the fault `file_refused`,
naming the field and what didn't fit, and nothing the action wrote is
kept ([Defects](04-failures.md#defects)). It is a fault, not a failure,
because only the runtime makes a `File`, from the bytes, the type and
the store the action gave `put`, so the mismatch is the program's own.
Bound the bytes before the put, such as by the `max_length:` of the
text they are made from, or change the field's settings. Clearing a
field, or a patch that leaves it out, checks nothing.

Here the bakery's sheets are at most 32 bytes, so a four-line sheet
isn't kept:

```jadpo check project=files-held file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=files-held file=domains/bakery/types/sheet_text.jadpo hidden
/// A price sheet as a supplier sends it.
type SheetText = Text {
    min_length: 1
    max_length: 100000
}
```

```jadpo check project=files-held file=domains/bakery/types/supplier_name.jadpo hidden
/// Who sent a price sheet.
type SupplierName = Text {
    min_length: 1
    max_length: 80
}
```

```jadpo check project=files-held file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=files-held file=domains/bakery/entities/price_sheet.jadpo
use bakery/types { SupplierName }

/// A supplier's price sheet, kept as it was sent.
entity PriceSheet {
    id: Uuid
    supplier: SupplierName
    sheet: File {
        store: sheets
        max_size: 32b
        types: [FileType.csv]
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=files-held file=domains/bakery/actions/keep_prices.jadpo hidden migrate=initial
use app { sheets }
use bakery/entities { PriceSheet }
use bakery/types { SheetText, SupplierName }
use bakery/failures { SheetNotKept }

/// Keeps a price sheet as its supplier sent it.
action keep_prices(supplier: SupplierName, sheet_text: SheetText) fails SheetNotKept -> PriceSheet {
    var sheet: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {
        storage_full: SheetNotKept
    }
    return PriceSheet.create {
        set: {
            supplier: supplier
            sheet: sheet
        }
    }
}
```

```jadpo check project=files-held file=domains/bakery/actions/keep_prices.test.jadpo expect=test.fault
use bakery/entities { PriceSheet }
use bakery/types { SheetText }

test "a short sheet is kept" {
    var price_sheet: PriceSheet = attempt keep_prices("Millers", "flour,1.20\nrye,2.40\n")
    assert price_sheet.supplier == "Millers"
}

test "a longer sheet isn't kept" {
    var sheet: SheetText = "flour,1.20\nrye,2.40\nspelt,3.10\noats,0.90\n"
    var price_sheet: PriceSheet = attempt keep_prices("Millers", sheet)
    assert price_sheet.supplier == "Millers"
}
```

```text output
FAIL  a longer sheet isn't kept
 --> domains/bakery/actions/keep_prices.test.jadpo:11

  fault: file_refused in action keep_prices
  at: domains/bakery/actions/keep_prices.jadpo:11:12
   |
11 |     return PriceSheet.create {
   |            ^^^^^^^^^^^^^^^^^^^ not kept
  reason: `PriceSheet.sheet` holds files of at most 32 bytes, and this one is 41 bytes; a field keeps only a file that fits its settings, so the write stopped before it ran and nothing the action wrote was kept.
  next: Bound the bytes before they are put, such as by the `max_length:` of the text they are made from, or raise `PriceSheet.sheet`'s `max_size:` (agent)
  stack: domains/bakery/actions/keep_prices.jadpo:11       action keep_prices
         domains/bakery/actions/keep_prices.test.jadpo:11  test "a longer sheet isn't kept"

  rule: test.fault
  rerun: jadpo test domains/bakery/actions/keep_prices.test.jadpo:11 --seed 1

test failed: 1 failed, 1 passed, 2 action tests
```

### `store.read(file)`

`store.read` reads at most 10 MB into memory. Reading a field whose
`max_size` is over that is `type.file_read_too_large`, where it is read.
A `File` whose field isn't known, such as a parameter of type `File`, is
held to the limit as it runs, and going over it is a fault. A file whose
bytes are gone from its store is a fault too.

`FileContents` is the file's bytes. Nothing takes it yet but what passes
bytes on, such as an email's attachment or a provider's upload. Like
`put`, `read` is called only in an action, before its first read or
write. A test reaches files through the application's actions.

### The disk driver in `jadpo dev`

`jadpo dev` keeps every store of files under
`.jadpo/files/<connection>/`, whatever its driver, so nothing leaves
your machine:

- Each file is kept under its key, sharded into two directory levels by
  its id. A key of any other shape is refused, and so is a symbolic link
  anywhere on the way.
- A file is written to a temporary file readable by the application's
  user only, synced, and renamed into place, and its folder is synced.
- A full disk answers `storage_full`, and so does a put below the
  store's `free_space_floor:`, as it does in production.

Its reads and writes block, a window of at most a megabyte at a time,
on your own disk.

### Files in a test

In a test, one in-memory store serves every connection
([Provider calls and capabilities in tests](13-testing.md#provider-calls-and-capabilities-in-tests)),
and a file helper's run reads two rows a batch. `fixture_file`, and
telling the store to fail from a test's source, come with the testing
fakes ([What is built](#what-is-built)).

## Files: an export's pieces
<!-- topic: export-file-pieces (was files-an-export-s-pieces) -->

A large export is written a page of rows at a time, and each page is a
piece of the file. This topic is what that asks of the `files`
capability ([How an export's run is kept](12-jobs-and-events.md#how-an-exports-run-is-kept);
[Data formats](19-standard-library.md#data-formats-json-csv-tsv-and-json-lines)).

### Writing the pieces

Each page's rows are written by an ordinary keyed `put`, under a key
made from the file's own key (`jadpo_std::files::piece_key`). That key
has the runtime's shape, so a driver refuses no piece, and finds each
piece from the file's key alone.

Two more operations go through the seam, each a keyed call:

- `join(key, pieces, type)` joins the pieces, in order, into the file
  under `key`. Repeating it once the file is whole does nothing.
- `discard(key, pieces)` drops a cancelled export's pieces, and its file
  if they were joined.

A store with no room answers `storage_full`, and a join with a piece
missing answers `not_found`.

### The disk driver joins whole

The disk driver copies the pieces in order into a temporary file
readable by the application's user only, a window at a time. It syncs
the file, renames it into place and syncs its folder, then removes the
pieces, so the file appears whole or not at all. The in-memory store a
test's files are in does the same in memory.

### `FileType.json_lines`

`FileType.json_lines` is a written JSON Lines file's type,
`application/jsonl`, as jsonlines.org names it. It comes after `json`.

## Reading gzip
<!-- topic: reading-gzip -->

Some uploads arrive as gzip files, such as a supplier's `.csv.gz`. This
topic is how one is read ([Compressed uploads](#compressed-uploads)).

### Only the file helpers unpack, and only when told how far

A file whose first two bytes are gzip's magic number is unpacked by a
format helper given `max_unpacked_size:`
([Data formats](19-standard-library.md#data-formats-json-csv-tsv-and-json-lines)),
and by nothing else. `store.read` gives its bytes as they are stored, and
a helper given no `max_unpacked_size:` stops the run before its first
row.

The limit is the most text the run reads. The run stops at the row that
would reach past it, with the rows wholly inside it done, and unpacks at
most 64 KiB past it, so a small upload can't unpack to gigabytes.

### gzip as RFC 1952 writes it, every member in turn

A gzip file is one member or more, one after another, as
`cat a.gz b.gz` makes, and Jadpo reads them as one text.

- Each member's header is read whole, with every optional part its
  flags say it has (an extra field, a name, a comment), and its own
  CRC-16 checked when it has one.
- A reserved flag, a method other than deflate, or anything after a
  member that isn't another member is damage.
- Each member's trailer is held to what the member unpacked to: its
  CRC-32, and its size modulo 2³².

A damaged file, and one cut short inside a member, stop the run where
the damage is found, with the rows before it done. No other compression,
and no archive such as zip or tar, is opened.

### Unpacked as it is read, never whole

The packed bytes are read from the store through the seam a window at a
time, as a plain file's are, and unpacked (with miniz_oxide's streaming
inflate) only as far as a page needs. gzip can't be entered part way, so
a runner that picks up a run unpacks the file from its start to the
run's position, discarding what comes before. Within one runner, each
page carries on from the last.

### A file that isn't gzip

A file that isn't gzip is read as it is, whatever `max_unpacked_size:`
says, so one helper serves uploads that are packed and ones that
aren't.

## Flags: declared, read and tested
<!-- topic: flags-declared-read-and-tested -->

Each flag in `flags { }` is a `Bool` or an enum whose variants carry no
data, with a `default:` written as a test's `flags:` writes a value, an
`owner:` in quotes, and an `expires:` date in quotes, `"2027-01-31"`. A
flag missing one of the three, giving one that isn't valid, of another
type, or declared twice is `flag.declaration_invalid`. `client:` is
`toolchain.not_built_yet` where it is written, until `GET /flags` is
built.

### Where it is read

An action, a test and a route's `run:` arguments read `flags.<name>`; a
test's read gives what its `flags:` says. A function, a constant, a rule
or a template reading one is `flag.read_context`, and a name no flag has
is `flag.unknown`, in code and in a test's `flags:`, with the nearest
flag's name offered.

### The state each server holds

`jadpo_flags` holds a row for each flag that has been changed: everyone's
value, `NULL` for the `default:`; the named users' values, an object of
each user's `id` to a value; and the shares of users, an array of
`[value, percent]` pairs in order, each a whole percentage. A value is
`true` or `false`, or a variant's name. A flag with no row, and a value
no variant has any more, give the `default:`.

Each server reads the table at most every five seconds, as a request or
a tick begins, so a change applies within ten seconds. A read that
fails keeps what the server had, or nothing, which gives every flag its
`default:`, and the next is tried five seconds later. The first flag an
operation reads fixes the state for the rest of it, and each flag's
value is worked out once per operation.

### Who is in a share

A user's place among 100 is the first eight bytes of the BLAKE3 hash of
the flag's name, a zero byte and the user's `id` in lowercase, read as a
little-endian number, modulo 100. A user whose place is below the first
share's percentage gets its value, below the first two shares' sum the
second's, and so on; anyone else gets everyone's value. A named user's
value comes before any share. A service, a schedule's run, a
subscriber's delivery and a request no one signed in to have no user, so
they get everyone's value.

### Judging a flag's date

`jadpo check` judges `expires:` at today and `jadpo build` at the
commit's date, both in UTC, or at `SOURCE_DATE_EPOCH` when it is set. A
flag is past its date from the day after it, and `flag.expired` names
its owner. Running a program or its tests judges no date.

## Monitoring thresholds: checked and generated
<!-- topic: monitoring-thresholds-generated -->

How `jadpo check` reads a domain's thresholds and what it writes from
them (chunk B13; the form of a threshold decided 2026-11-01).

### Where they are read

A domain has one `thresholds { }`, in its `monitoring.jadpo`; a second,
or one in any other file of a project, is
`threshold.declaration_invalid`. A loose file may hold one, watching
what the loose files declare. A threshold's name is unique in its
domain, and its subject, as an acceptance names it, is
`<domain>.<threshold>`.

### In the deployable

The per-request limits, `duration:` and `queries:`, go into the
runtime's plan with the identity key of what each watches, so both
executors and every deployable hold the same thresholds. They are what
the `telemetry:` section judges each request by once it is built; until
then nothing reads them.

### The rules in `build/monitoring/`

`jadpo check` writes two files once the thresholds read cleanly, and
none while there are no across-request limits or floors:

- **`prometheus.yaml`**, Prometheus alerting rules: a group for each
  domain, `jadpo.<domain>`, and `jadpo.files` for the disk floors, with
  an alert for each across-request limit, named
  `<domain>_<threshold>_<limit>` and labelled `jadpo_threshold` with its
  subject, its `///` line as the summary.
- **`openslo.yaml`**, OpenSLO v1: a `Service` for each domain and an
  `SLO` for each percentile and `server_failures:`, each counted by
  `Occurrences` over a rolling `within:`. A `p95: 300ms` is the share of
  requests done within 300 ms, with a target of `0.95`; a
  `server_failures: 1%` the share that didn't fail on the server, with a
  target of `0.99`. `oldest_waiting:` and the floors are conditions
  rather than objectives, so they are Prometheus rules only.

The rules read the metrics the runtime will send, under the names a
Prometheus that takes OpenTelemetry gives them:

| Metric | Prometheus name | Attributes |
|---|---|---|
| A route's duration, a histogram in seconds | `http_server_request_duration_seconds` | `http.request.method`, `http.route` as `jadpo routes` prints it, `http.response.status_code` |
| An action's, query's, schedule run's or delivery's duration, a histogram in seconds | `jadpo_run_duration_seconds` | `jadpo.kind` (`action`, `query`, `schedule`, `subscriber`), `jadpo.name` (`hello.greet`), `jadpo.outcome` (`ok`, `failure`, `server_failure`) |
| The age of a subscriber's oldest waiting delivery, a gauge in seconds | `jadpo_delivery_oldest_waiting_seconds` | `jadpo.name` |
| The share of a disk store's disk that is free, a gauge from 0 to 1 | `jadpo_files_free_ratio` | `jadpo.connection` |

Each duration histogram has a bucket boundary at every duration a
threshold on it names, so a percentile's count is exact rather than
read between buckets.

### In the manifest

Each threshold is a `threshold` node named by its subject, with a
`uses` edge to what it watches, `watches` saying what kind of thing that
is, and its limits as written, its per-request ones under `perRequest`
and the others under `acrossRequests` beside `within`. A disk store's
floor is a `threshold` node named `<connection>.free_space_floor`, which
watches `disk` and uses its connection.

## What is built
<!-- topic: built-in-services-built -->

The errors this page describes are built with the feature they belong to.

| Feature | Status |
|---|---|
| Email | Built: email's templates, the shared layout, `email:`, sending after commit through the SMTP driver, attachments and parking ([Email templates and sending](#email-templates-and-sending)). The SMTP conversation runs in the driver, over the host's transport and its TLS upgrade ([How it is built](../compiler-runtime.md), "TLS through the transport"). |
| Jadpo's own magic-link email | Built: at all five levels, worded by Jadpo or reworded by the app, wrapped in the app's layout, the app's own email filled with the link and the code, or the app's own sending given them ([Jadpo's own magic-link email](#jadpos-own-magic-link-email), [Your own magic-link email](#your-own-magic-link-email)). |
| The privacy emails | Not built yet. They come with data requests ([Data requests](15-personal-data.md#data-requests)). |
| Mail packages | Built: a mail package as the connection, partials and `email.sent` ([Mail packages, partials and `email.sent`](#mail-packages-partials-and-emailsent)). |
| The dev inbox and the health score | Built: the dev inbox, `jadpo email`, the health score, `email.health_problem` and `jadpo email check-domain` ([The dev inbox and the health score](#the-dev-inbox-and-the-health-score)), with SpamAssassin's rules in the score ([SpamAssassin in the health score](#spamassassin-in-the-health-score)). Not built yet: the inbox page as part of the developer console, and its link to each mail's trace, which comes with `jadpo trace`. |
| Rate limits | Built: default rate limits with routes, and their store on Workers, Cloudflare's rate-limiting binding ([Rate limits on Workers](#rate-limits-on-workers)). Not built yet: Redis's driver, which comes with caches. Until then a deployable whose store is Redis counts in its own memory, which on Workers is the judgement call `rate_limits.counted_in_memory`. The binding's windows and the Wrangler entries aren't yet checked against the real edge. |
| The capability seam | Built: the seam every capability's calls go through, with a delivery that hands its event to a capability, parked as `outcome_unknown` when a send without a key may have acted ([The seam every outside call goes through](#the-seam-every-outside-call-goes-through)). |
| Files | Partly built. Built: the `files` capability's first part on that seam: `Driver.disk` connections, `File` fields, `store.put`, `store.read` with its whole-read cap and its two errors, the disk driver `jadpo dev` keeps files with and the in-memory store a test's are in, which the file helpers' reads walk ([Data formats](19-standard-library.md#data-formats-json-csv-tsv-and-json-lines); [Files: `put`, `read` and the disk driver in development](#files-put-read-and-the-disk-driver-in-development)); every write of a `File` field holding the file to the field's `store:`, `types:` and `max_size:` ([What a write checks](#what-a-write-checks)); the pieces the file helpers' writes put, joined into one file or dropped by the store ([Files: an export's pieces](#files-an-exports-pieces)); and a gzip file unpacked as the file helpers read it, up to `max_unpacked_size:` ([Reading gzip](#reading-gzip)). A native deployable's disk driver keeps each store at its `path:`, made the app's user's alone as it starts, a root other users can write to not built, and a write below `free_space_floor:` answering `storage_full`, in `jadpo dev` too; `jadpo check` refuses a `path:` that is relative or inside the project ([`connection.disk_path_invalid`](../errors/connection.disk_path_invalid.md)); and the manifest lists the disk stores that keep a deployable on one server ([The disk driver](#the-disk-driver)). The floor is a monitoring threshold ([Monitoring thresholds](#monitoring-thresholds)). Not built yet: upload slots, multipart ones included, downloads, deleting and erasing a file's bytes, the S3 driver, `fixture_file` and telling the store to fail from a test, and image variants and scanning when an app needs them, the application's `scanning:` being `toolchain.not_built_yet` where it is written. |
| Flags | Partly built: `flags { }` of `Bool` and enum flags with their `default:`, `owner:` and `expires:`; `flags.<name>` in actions, tests and a route's `run:` arguments, worked out for the caller from `jadpo_flags` in `flags: { store: }`, held in each server's memory and fixed for the operation; a test's `flags:`; `flag.unknown`, `flag.read_context` and `flag.expired` ([Flags: declared, read and tested](#flags-declared-read-and-tested)). Not built yet: `jadpo flags set`, `list` and `remove`, the log of changes, `check` failing until both values are tested, `client: true` and `GET /flags`, and named tenants, which wait for tenancy to be settled for flags. |
| Audit | Not built yet: an entity's `audit:` is `toolchain.not_built_yet` where it is written. |
| Search | Built on PostgreSQL: `search: { fields: language: }` with its generated column, GIN index and migrations, `matches` with a `SearchText`, the best matches first in keyset pages, and policy over every search ([Search](#search)). Not built yet: an engine's `store:`, `toolchain.not_built_yet` where it is written, and `jadpo search reindex`, which come with the `search` capability's packages; and searching a field of a sensitive category, `toolchain.not_built_yet` in `fields:` until the access records are built. |
| Documents | Not built yet: the application's `documents:` is `toolchain.not_built_yet` where it is written. |
| Bot protection | Not built yet: the application's `bot_check:` and a route's are `toolchain.not_built_yet` where they are written. |
| Outgoing webhooks | Not built yet. |
| The `telemetry:` section | Not built yet: it is `toolchain.not_built_yet` where it is written. Nor is telemetry in the developer console. |
| Path IDs | Not built yet for requests. Each body's ways are numbered, and the differential run compares the paths each test's bodies took, but no request carries a `path` or a `path_key`. |
| Monitoring thresholds | Built: `thresholds { }` in each domain's `monitoring.jadpo`, each threshold checked to watch a route, action, query, schedule or subscriber of its domain with limits that apply to it; the per-request limits in the runtime's plan, the across-request limits and the disk floors written to `build/monitoring/` as Prometheus and OpenSLO rules and carried in the manifest; and a limit raised or removed, or a floor lowered, refused by `check` until a `ThresholdChange` acceptance names it ([Monitoring thresholds: checked and generated](#monitoring-thresholds-checked-and-generated)). Not built yet: acting on the per-request limits, which comes with the `telemetry:` section, starting values from the query report and production, and an agent's proposals through MCP. |

[The roadmap](../roadmap.md) says when.

## Errors

- [`connection.disk_path_invalid`](../errors/connection.disk_path_invalid.md)
- [`data.file_field_invalid`](../errors/data.file_field_invalid.md)
- [`type.file_read_too_large`](../errors/type.file_read_too_large.md)
- [`email.template_missing`](../errors/email.template_missing.md)
- [`email.template_value_invalid`](../errors/email.template_value_invalid.md)
- [`email.template_not_allowed`](../errors/email.template_not_allowed.md)
- [`email.partial_invalid`](../errors/email.partial_invalid.md)
- [`email.field_unused`](../errors/email.field_unused.md)
- [`email.health_problem`](../errors/email.health_problem.md)
- [`email.settings_invalid`](../errors/email.settings_invalid.md)
- [`email.attachment_limit`](../errors/email.attachment_limit.md)
- [`email.connection_required`](../errors/email.connection_required.md)
- [`email.sent_context`](../errors/email.sent_context.md)
- [`search.settings_invalid`](../errors/search.settings_invalid.md)
- [`search.undeclared`](../errors/search.undeclared.md)
- [`rate_limits.counted_in_memory`](../errors/rate_limits.counted_in_memory.md)
- [`rate_limits.store_invalid`](../errors/rate_limits.store_invalid.md)
- [`rate_limits.store_required`](../errors/rate_limits.store_required.md)
- [`flag.declaration_invalid`](../errors/flag.declaration_invalid.md)
- [`flag.expired`](../errors/flag.expired.md)
- [`flag.read_context`](../errors/flag.read_context.md)
- [`flag.store_invalid`](../errors/flag.store_invalid.md)
- [`flag.store_required`](../errors/flag.store_required.md)
- [`flag.unknown`](../errors/flag.unknown.md)
- [`threshold.declaration_invalid`](../errors/threshold.declaration_invalid.md)
- [`threshold.subject_unknown`](../errors/threshold.subject_unknown.md)
