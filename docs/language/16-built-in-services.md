---
title: Built-in services
area: 16
status: decided
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - data.file_field_invalid
  - type.file_read_too_large
---

# Built-in services

Which common backend features Jadpo should provide itself, rather than
leaving each application to install a library or write its own: feature
flags, rate limits, files, audit trails, tracing, email and search.

> Decided on 2026-10-13. Only the capability seam (chunk 7.0c) and the `files` capability's first part (chunk 6.4b) are built; the [status table](#status) says when each part comes. Examples apply the decisions from areas 1 to 16.

**The test for "native".** A feature is built in only when the compiler
gains something by knowing about it: a mistake becomes a compile error, a
test or a report is generated, policy or erasure reaches it, or the runtime
does the work with nothing for the author to write. Saving a library install
is not enough. Where the language can reasonably own a feature it does, rather
than depending on a third-party service (the rule area 8 applied to OAuth).

| Feature | Native? | Why | Where it lives |
|---|---|---|---|
| Feature flags | Yes | Misspelt flags are errors; both branches tested; expired flags reported; one-step removal | Root `flags.jadpo`; read as `flags.name`; state in the app's database, not a capability |
| Rate limits | Yes | The route knows the caller; 429 is already a failure kind; limits appear in OpenAPI | `rate_limit:` on an API or route; counted in the cache store, or Cloudflare's binding on Workers |
| Files | Yes, through the `files` capability | Size and type checked; policy and erasure reach the bytes; signed links generated | `File` fields; a `files` connection in `app.jadpo`: the S3 driver, or the disk driver on native |
| Audit trail | Yes | The compiler sees every write and its caller; personal fields hashed | `audit:` section on an entity; `AuditEntryRecorded` when something listens |
| Telemetry | Yes, with one settings section | Every route, query, action, job and provider call is known | `telemetry:` in `application { }`; custom metrics in a domain's `monitoring.jadpo` |
| Path IDs and monitoring thresholds | Proposed | Every path is known, so it can be named, sampled and given limits | Generated runtime; a monitoring file per domain; questions 13–16 |
| Caches | Yes (area 5) | What may be cached, and what never is, is the language's | Redis-compatible connections; "Caches: drivers and what they hold" below |
| Email | Yes, through the `email` capability | Templates typed against translations; delivered after commit; any provider without code changes | `type X = EmailMessage` in `emails/`; an `email` connection in `app.jadpo` |
| Documents | Yes, through the `documents` capability | Templates typed and translated as email's; rendered in subscribers | `type X = Document` in `documents/`; a `documents:` connection in `app.jadpo` |
| Search | Yes, through the `search` capability | Index, query and migration generated from one declaration; results always pass policy | `search:` section on an entity; PostgreSQL unless it names an engine |
| Bot protection | Yes, through the `bot_check` capability | Jadpo's own public endpoints need it; the token never enters a route's input | `bot_check:` in `application { }`, `bot_check: true` on a route or feature |
| Outgoing webhooks | Yes | Payloads typed from events; signing and retries generated; one guarded network exception | Later, through `publishing`; question 8 |
| Field encryption | Yes | Encrypted fields kept out of logs, indexes and filters | Area 15; question 8 |
| Realtime, admin screens, payments, CMS, analytics, quotas, media transformations | No, or later | The compiler gains little, or the feature is not a backend's | Not built in; media services (Cloudinary, Uploadcare) are ordinary packages |

## Capabilities on this page

A capability is a job that a connection does for Jadpo, chosen in
`app.jadpo` (area 11). Decided 2026-10-16 (Richard): **a capability exists
only for a job Jadpo itself performs, and covers that job whole**, so every
connection that fills one does all of it and there are no optional parts.
Everything else a provider offers is the package's own operations and
events, called from the domains its connection lists, as payments are.
Packages fill capabilities and never invent them.

| Capability | The whole job | Filled by |
|---|---|---|
| `files` | Put, read, delete, signed links and upload slots, multipart ones included | The S3 driver (any S3-compatible service); the disk driver, native only |
| `email` | Send a Jadpo-rendered `EmailMessage` after commit, attachments included | The SMTP driver; a mail package (Postmark, Amazon SES, Resend) |
| `documents` | Turn the HTML Jadpo renders into a PDF | A container package (Gotenberg) or a hosted API's package (DocRaptor, PDFShift) |
| `search` | Upsert, remove, and query by words with equality filters, returning ranked ids | PostgreSQL full-text search, built in; an engine's package |
| `bot_check` | Check a token: pass or fail | A package (Turnstile, hCaptcha, reCAPTCHA) |
| `scanning` | Scan an uploaded file: clean or infected | A ClamAV container package or a scanning API's package |
| `publishing` | Deliver chosen events outward, and outgoing webhooks | Drivers and packages (area 12) |
| `receiving` | Fetch messages from a queue, the inbound twin of `publishing` | Packages; a built-in driver for Cloudflare Queues on Workers (area 12) |

The database, cache and schedules are capabilities with built-in drivers
only (areas 5 and 12). `sms` is not a capability, since nothing in the
language sends SMS: Twilio is a package, its text still typed against
translations, and `sms` returns only if a built-in feature (SMS sign-in
codes) needs it. Telemetry is not a connection but its own `telemetry:`
section ("Telemetry", below). Each capability has a local driver for
development, one fake for tests (area 13) and a conformance suite a filling
package must pass.

## Feature flags

A feature flag lets a team ship code switched off, then turn it on for some
users, a share of users, or everyone, without a deploy. Every flag is
declared once, in a new root file `flags.jadpo`, with its type, default,
owner and expiry (question 1). Its description is the `///` comment above
it, as for every declaration.

```jadpo fragment
// flags.jadpo
flags {
    /// The one-page checkout replacing the three-step form.
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2027-01-31"
    }

    /// Which layout the checkout uses while we compare two.
    checkout_layout: CheckoutLayout {
        default: CheckoutLayout.classic
        owner: "payments"
        expires: "2026-12-15"
    }
}
```

A flag is a `Bool`, or an `Enum` for more than two variants. Code reads it as
`flags.new_checkout`, in actions and in a route's `run:` arguments; functions
and queries take the value as a parameter. As with `clock.now`, the value is
fixed for the whole operation, so one request never sees a flag change
halfway through.

```jadpo fragment
// domains/ordering/actions/place_order.jadpo
action place_order(user_id: User.id, input: PlaceOrder) -> OrderView {
    if (flags.new_checkout) {
        return place_order_in_one_step(user_id, input)
    }
    ...
}
```

**Changing a flag.** A flag's current state lives in the app's own
database, in a table the language owns, and is changed at runtime with
`jadpo flags` against an environment (area 14's operator commands,
question 2). Each change is logged with who made it. Each server holds the
flag values in its own memory and refreshes them every few seconds, so a
change applies within ten seconds; flags are never held in the cache. If
the database cannot be read, each instance keeps the last values it had,
or the defaults; reading a flag never fails a request.

Flags are not a capability and no vendor fills them (decided 2026-10-16,
Richard). A `Driver.ofrep` store, evaluation only through OpenFeature's
remote evaluation protocol, waits for an app that must use a vendor.

```text
jadpo flags set new_checkout --env production --users 0192... --percent 10
jadpo flags set checkout_layout --env production --split classic=50,compact=50
jadpo flags list --env production
```

**Who gets what.** The first rule that matches wins (question 3):

1. Named users get a fixed value.
2. Named tenants get a fixed value, once area 9 decides tenancy.
3. A percentage of users, chosen by a stable hash of the flag's name and the
   user's `id`: the same user always gets the same answer, raising 10% to
   20% keeps the first 10% in, and each flag picks a different 10%.
4. Everyone else gets the flag's current value, at first its `default`.

The caller comes from the runtime, as for policy. Schedules and subscribers
have no caller (area 12), so they get the everyone value; an action that
needs a user's variant later puts it in the event it emits.

**What the compiler checks** (question 4):

- `flags.new_chekout` is a compile error, like any unknown name.
- Every flag read needs tests with each value: a test chooses with
  `flags: { new_checkout: true }` beside `clock:` and `config:` (area 13), and
  `jadpo check` fails until tests reaching each read cover both `true` and
  `false`, or every variant.
- A flag past its `expires:` date is a warning naming its owner. Extending it
  means editing the date, so keeping a flag is a reviewed choice.
  *Amended 2026-10-17 (Richard, area 1, question 11):* an error, not declinable, judged at today's
  date by `check` and at the commit's date by `jadpo build`, so rebuilding
  a merged revision is never blocked by a date passing.
- Whether a flag has sat at 100% or 0% is runtime state the compiler cannot
  see, so `jadpo flags list` marks those flags "ready to remove".

**Removing a flag.** `jadpo flags remove new_checkout --keep true` rewrites
the source: each read becomes `true`, the dead branch goes, the declaration
goes, and test overrides for the removed value are deleted. Uber's Piranha
does this for Java and Swift; Jadpo can do it exactly because every read is
known.

**Flags in browsers and apps.** A flag with `client: true` is also returned,
for the caller, by a generated `GET /flags` route in the OpenAPI document
(question 5).

Flags are for releasing code. What a customer's plan includes is application
data with policy, and a setting that differs per environment is
configuration (area 10).

## Rate limits

A rate limit caps how many requests a caller may make in a period. It is set
on an API in `app.jadpo` (the default for its routes) or on one route
(question 6), and refused requests get the built-in `rate_limited` failure:
429, with a `Retry-After` header.

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

- `by:` is `RateLimitKey.caller` (the default: the signed-in principal, or
  the address when there is none) or `RateLimitKey.ip`. The address is the
  client's, chosen by `application { client_address: }` (area 10, "Client
  addresses"): the connection's own unless the application names a header
  of a proxy of its own to believe, so behind a load balancer the limits
  count each client and not the balancer.
- Counters live in the Redis store named by `rate_limits: { store: cache }`
  in `application { }`. If it cannot be reached, each instance counts in its
  own memory, so limits stay roughly right and no request fails, the same
  rule as caches (area 5). With no Redis store, counting is in memory.
- The same store also holds provider rate limits (a package's limits on
  its own API, area 11), so servers share one count of calls to a provider
  instead of each spending the whole allowance.
- On Cloudflare Workers, Cloudflare's rate-limiting binding can be the
  store, for the limits it can count: short windows only (10 or 60
  seconds; Cloudflare's current windows still to check). A limit with any
  other window on that store is a compile error naming Redis as the store
  that can count it (decided 2026-10-16).
- A refused request gets area 7's `429` with `Retry-After` and the
  standard `RateLimit` headers, its wording from translations like any
  failure.
- Area 8's generated magic-link routes get their limits by address without
  anything written.
- The limit appears in the route's OpenAPI entry and in `jadpo routes`.

## Caches: drivers and what they hold

Area 5 decides what is cached (nothing, unless added) and how copies stay
current. Decided 2026-10-16 (Richard), which services hold it and what
it may hold:

- **Redis-compatible services use the Redis driver**: Upstash, Valkey,
  Amazon ElastiCache, Redis itself, on the native target and on Workers.
  Their settings are listed on the driver's page, like every driver's
  vendors (area 11).
- **`Driver.memory`** keeps a cache in each server's own memory. It is
  allowed in production with a warning from `jadpo check` (the cache is per
  server and emptied on every restart) and is a compile error on Workers,
  which keeps no memory between requests.
- **Cloudflare KV is not a cache driver**: a change can take a minute to
  reach every location, so an invalidated entry could still be served. An
  app that wants KV calls it as a package.
- **The cache holds only what is harmless to lose**: cached records and
  query results (area 5), route rate limits, and provider rate limits
  shared across servers, all through the same
  `rate_limits: { store: cache }` and caching settings. It never holds
  idempotency keys, webhook de-duplication, sessions, OAuth grants, poll
  state, schedule leases, batch positions, multi-step call records or the
  outbox: each of those must survive a restart, so each lives in a
  database. Naming a cache-only store for one of them is a compile error
  naming the feature.

## Files

A file is a field of type `File` on an entity. The bytes live in a
connection that provides the `files` capability (area 11): the built-in S3
driver for any S3-compatible service, or the built-in disk driver on the
native target. The record holds the file's key, name, size and type
(question 7, revised 2026-10-13 and 2026-10-16).

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

- `store:` names the connection, as for entities and caches. Two buckets
  are two connections.
- Cloudflare R2, Amazon S3, Backblaze B2, DigitalOcean Spaces and MinIO are
  all the S3 driver with their own host and settings, listed on the
  driver's page; no package is a preset over it (decided 2026-10-16).
- In `jadpo dev` and tests, files go to local disk with locally signed
  links, so nothing leaves the machine; the local driver does not exist in
  production builds. An app whose production store is the disk driver uses
  that same driver in development, at a folder under `.jadpo/`, so slots,
  ranges and signed links behave as they will in production.
- A file the application makes itself (a generated PDF) is written with
  `attempt avatars.put(bytes, FileType.pdf)`, which returns a `File` to
  store in a field. It is an outside call, made before the action's writes
  with no transaction open (area 6); in an action with several changing
  calls it is a keyed step like a provider call, its key derived as any
  step's is, so a rerun writes the same object again (area 12). Bytes
  never attached are deleted after a day, like abandoned uploads.
- Media services (Cloudinary, Uploadcare) are ordinary packages, called
  through their own operations and pure functions (a transformation URL),
  because they cannot do the whole `files` job.

**Storage and formats are separate layers** (decided 2026-10-16). A store
moves bytes in and out and knows nothing of what is inside; formats (CSV,
TSV, JSON, JSON Lines) turn bytes into the app's types and back, whatever
the bytes came from. The app's code joins them:

- `attempt store.read(file)` returns the file's contents as `FileContents`
  (area 2), for passing the bytes on: to a provider as an upload, to a PDF
  service, as an email attachment. The format helpers do not accept it.
  It reads the whole file into memory, so it is capped: reading a field
  whose `max_size` is over the whole-read limit (10 MB, confirmed by
  measurement in chunk 8.3) is a compile error pointing at the format
  helpers or `batches.each`.
- Rows in a file, of any size, are read and written only through the batch
  helpers (`csv.read`, `csv.write`, `tsv`, `json_lines`, `json.read` and
  `json.write`; area 12, "Batches"), whose operations are tabled in
  [area 19, "Data formats"](19-standard-library.md#data-formats-json-csv-tsv-and-json-lines).
  There is no loop over a file's rows and no parse of a whole file into a
  list. A provider's CSV report is put in a store and read the same way.
- `json.read(text, as:)` (the `json.decode` of the packages design,
  renamed by area 19 on 2026-10-17) is only for single JSON documents in
  memory: request bodies and provider answers.

> Built in chunk 6.4b: `Driver.disk` connections, `File` fields with
> `store:`, `max_size:` and `types:`, `store.put` and `store.read`, the
> disk driver `jadpo dev` keeps every store's files with, and the
> in-memory store a test's are in; the choices are under "Chosen in
> chunk 6.4b" below. Built in chunk 6.4b2: a written file's pieces
> joined, or dropped, by the store ("Chosen in chunk 6.4b2"). Uploads, downloads, deleting and erasing come with
> the rest of the files capability ("Remaining built-in services" on the
> roadmap).

A bakery keeps the price sheets its suppliers send. `sheets.put` keeps
the bytes before the action reads or writes anything, since a call that
leaves the process never waits inside a transaction (area 6), and gives
the `File` the record keeps; a store with no room is `storage_full`,
which only an `unavailable` failure maps:

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

**Uploading.** Files go straight from the client to the store, not through
the application, so the 1 MB body limit (area 7) is not in the way:

1. An action calls the generated `User.avatar_upload()`, which returns an
   `UploadSlot`: a signed address valid for 15 minutes that accepts only
   `max_size` bytes.
2. The client sends the file to that address.
3. The client sends the slot's token in an ordinary create or update, in an
   `Input` field of type `FileUpload`. Before the write commits, the runtime
   checks the file is there, its size, and its type from its first bytes
   rather than what the client claimed. A mismatch is `invalid_value`.

Asking for a slot needs `create` or `update` on the entity (area 9).
Uploads never attached are deleted after a day.

**Large uploads go in parts** (decided 2026-10-16). A field whose
`max_size` is over 100 MB gets a multipart slot instead of a single
address, the same shape on S3, R2 and disk: the slot names a part size
(equal parts, as R2 requires) and gives an address per part (on S3 and R2
a signed `UploadPart` address; on disk `PUT /files/<store>/uploads/<slot>/parts/<n>`);
the client sends the parts and may resend any part after a dropped
connection; attaching the `FileUpload` completes the upload and runs the
same size and type checks. A generated client helper hides the parts. A
single signed `PUT` would be limited to 5 GiB, could not resume, and
through Cloudflare's proxy is limited to 100 MB on the Free and Pro plans.

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

Uploads sent with a `Content-Encoding` are refused. A gzip file
(`.csv.gz`) is read only by a batch helper given `max_unpacked_size:`;
passing it is a file-level failure, so a small upload cannot unpack to
gigabytes. Jadpo unpacks no other compression and no archives (zip).

**Downloading.** An `Output` that picks a `File` field sends a signed link
valid for five minutes, with name, size and type. The read has already passed
policy, including field restrictions, so there is no download operation to
protect separately.

**Deleting and erasing.** Deleting the record, or clearing the field,
deletes the bytes after commit through a generated subscriber, because an
action writes one store (area 5). A `File` field can be marked `personal:`,
erasure and retention reach it like any field (area 15), and
`User.export` includes it as a link.

### The disk driver

Decided 2026-10-16 (Richard). `Driver.disk` keeps files on the server's own
disk, for production on the native target only; on Workers it is a compile
error by the target rule. The build manifest records that the app then
runs on one server, since a second server would not see the files.

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

- **Files are immutable** under keys the runtime generates (a UUID,
  sharded into two directory levels), never derived from names; the driver
  refuses any key that does not match its own pattern, opens files relative
  to its root without following symbolic links, and writes them readable
  by the app's user only. The root must not be inside the app's source or
  build folders.
- **Uploads are streamed** to a temporary file as they arrive, counted
  (a `413` at the first byte over `max_size`), hashed and type-checked, then
  synced, renamed into place and the directory synced.
- **Downloads are served by the app** from its own origin, with
  single-range `Range` requests, `If-Range` and an ETag from the file's
  hash, in fixed-size reads, so a 2 GB download uses the memory a 2 KB one
  does. Because the origin is the app's own, every download is sent with
  `Content-Disposition: attachment` (except an allow-list of image types),
  `X-Content-Type-Options: nosniff` and `Content-Security-Policy: sandbox`,
  and the client's file name only ever in `filename*` (RFC 6266).
- **Space**: a write that runs out of space fails as `storage_full`, and
  below `free_space_floor:` (5% by default) new uploads are refused so
  imports and the database are not starved; the floor is reported as a
  monitoring threshold.
- `jadpo files ls`, `show`, `get`, `put` and `copy` work on any store, in
  development or production; `jadpo files copy` moves a store's files to
  another, the way off one server.

The disk driver's errors (`Driver.disk` on Workers, a root inside the
project) and `storage_full` are built with the driver, after the first
build ("Remaining built-in services" on the roadmap).

### Image variants and scanning

Both are built when an app needs them (decided 2026-10-16).

- **Image variants** are declared on a `File` field
  (`variants: { thumb: 200x200 }`) and made by the runtime over any store,
  so no store needs to offer them and `files` stays whole. The runtime
  checks an image's pixel dimensions before decoding it. A variant is read
  as `user.avatar.variants.thumb`, a `File` that becomes a signed link in
  an `Output` like the original.
- **Virus scanning** is the `scanning` capability, named in
  `application { }` as `scanning: { connection: clamav }` and filled by a
  ClamAV container package (`runs:`, area 11) or a scanning API's package.
  With it set, Jadpo itself scans every upload: a file stays in a scanning
  state, and cannot be attached, until it passes; an infected file is
  refused like a wrong type and deleted.

## Audit trail

An entity with an `audit:` section keeps a history of every change to it:
who made it, when, through which action and request, and each changed
field's value before and after (question 9).

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

- Entries are written in the same transaction as the change, in the same
  store, so a change cannot happen without its entry.
- "Who" is the principal; for schedules and subscribers it is the
  application, with the request that emitted the event, so a change made
  later is traced back to the click that caused it.
- Fields marked `personal:` are stored as keyed hashes, the same as logs
  (area 15): the history shows that an email changed, not what it was.
  Server-only fields are left out.
- `Invoice.history(invoice_id)` is a generated query returning a page of
  entries. It needs `read` on the record, and fields the caller cannot read
  are left out.
- Entries are deleted when their retention ends, and with their record on
  erasure. The permissions report lists which entities are audited.

This is a record of changes to data. Who exported or erased a person, and
who read sensitive fields, is area 15's access record, kept as log events.

**Audit entries can go out** (decided 2026-10-16, Richard). When something
subscribes to the built-in `AuditEntryRecorded` event, or `publishing`
names it, each entry is also emitted as that event, in the same
transaction, and travels through the outbox like any event (area 12): a
SIEM or log archive receives it through `publishing`. With nothing
listening the event is never emitted. The database stays the record of
truth; the event is a copy, carrying the same hashed personal fields.

## Telemetry

The generated runtime reports what it does as OpenTelemetry traces,
metrics and logs, the open standard most monitoring tools accept (question
10). Decided 2026-10-16 (Richard): telemetry has its own section in
`application { }`, not a connection, with one protocol and no
vendor-specific code: a vendor is an endpoint and a header. Its host is
still listed in the network report.

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

- **`send:`** says which signals go out, and the lowest level of log sent.
  Left out, traces and metrics are sent and logs from `LogLevel.warn`.
- **`detail:`** says how much of each request is sent ("Monitoring
  thresholds", below): `full_traces:` lists when a request's whole trace is
  sent (it failed, it faulted, it crossed a threshold, its path is new to
  this process), and `samples:` how many ordinary traces each path sends,
  `per_path:` within `every:`, so there is always a normal request to
  compare a bad one with. The values shown are the decided behaviour of
  question 14.
- **The standard variables fill it.** `endpoint:` and `credential:` come
  from configuration fields bound to the standard `OTEL_*` names (area 10),
  so a host that sets `OTEL_EXPORTER_OTLP_ENDPOINT` and
  `OTEL_EXPORTER_OTLP_HEADERS` needs nothing else. With no `telemetry:`
  section nothing is sent. Application code cannot read these settings.
- **Traces.** One span per route, action, query, provider call, cache
  read, schedule run and event delivery, named after the declaration
  (`POST /todos`, `create_todo`, `todos_for_owner`). Incoming and outgoing
  `traceparent` headers are honoured, and a trace continues from the action
  that emitted an event into its subscriber.
- **Metrics.** Requests and durations per route, failures per code, query
  durations, provider calls and retries, cache hits, rate-limit refusals,
  queue size and the age of the oldest waiting delivery.
- **No values from the app's data** unless added deliberately. Spans carry
  names, identifiers and failure codes, never field values, following area
  4's log rule; marked personal fields are hashed. The two deliberate ways
  in are custom metric labels and `telemetry.note`, below, and both refuse
  personal fields.
- **Jadpo's own data goes to every provider**, nothing held back (Richard):
  path ids, stable identities, source locations and the build id travel as
  attributes named `jadpo.…` (`jadpo.path`, `jadpo.path_key`). A monitoring
  product decodes them against the build manifest (area 18), which a deploy
  uploads once per build.

**Custom metrics** are declared in a domain's `monitoring.jadpo` (area 1)
and recorded in code:

```jadpo fragment
// domains/billing/monitoring.jadpo
/// Checkouts that completed, by plan.
metric checkouts_completed = Counter {
    by: [plan]
}

// domains/billing/actions/complete_checkout.jadpo
telemetry.checkouts_completed.add(1, plan: account.plan)
```

A metric is a `Counter` (a count that only rises, `add`), a `Measure` (a
distribution, such as a duration or an amount, `record`) or a `Gauge` (a
value at a moment, `set`). Labels (`by:`) may be only enums and `Bool`, so
the number of series stays bounded. `telemetry.note(name: value)` adds an
attribute to the current trace, for a fact a span does not carry; it
takes text, numbers, `Bool` and enums. A personal field given as a label
or an attribute is a compile error. There are no custom spans: every action and query is
already one.

**In development** nothing is exported unless `jadpo dev
--export-telemetry` is given. The traces, metrics and logs stay on the
machine and are read from the command line, filterable and watchable, as
the development call log is: `jadpo traces`, `jadpo metrics` and `jadpo
logs` (area 17), and later the developer console.

## Path IDs

Proposed 2026-10-13 (question 13). Every request records the path it took
through the application: the route, each action and query, and each branch
taken inside them. The compiler knows every possible path, so a path can be
named by a number rather than described:

- **`path`** is a number that decodes against that build's graph, using
  Ball–Larus path numbering (each acyclic path through a callable gets a
  unique small integer, with minimal instrumentation). `jadpo path <build>
  <path>` draws the route from the number alone, with no recorded data.
- **`path_key`** is a hash of the stable identities on the path, so the
  same path can be compared across builds ("p95 of this path since
  Tuesday's deploy"). An edit inside a declaration changes its key, which
  is the truth: the path changed.
- **Loops and recursion.** A `for` body records which inner paths ran, not
  how many times; a recursive call records the path of its first entry.

Both go on the root span and on every failure and fault event. A path ID
says which way a request went, not how long it took or what it carried:
timings, the request and the response come from the trace (area 17).

Two failures at the same line reached from different routes keep different
paths, and a path shows the branches that ran and finished before the
failure, which a stack no longer holds.

Path IDs need control flow inside each callable in the compiler's graph.

## Monitoring thresholds

Proposed 2026-10-13 (questions 14–16). Thresholds are declared in the
application, reviewed with it and versioned with each build, not configured
in a monitoring tool. There are two kinds, because only one can be judged
by a single process.

**Per request** (duration, query count, failure). The runtime keeps each
request's spans in memory until it ends, then sends:

- the full trace when a threshold was crossed, the request failed or
  faulted, or its path is new to this process;
- otherwise metrics only, plus one sample trace per path per minute, so
  there is always a normal request to compare a bad one with.

Both are what `telemetry: { detail: }` sets ("Telemetry", above):
`full_traces:` names the first list and `samples:` the second.

A process crash loses its unsent requests; long jobs send on a timeout.
This keeps volume low without losing anything structural: the compiler
knows every path, so per-path metrics cannot grow without bound.

**Across requests** (p95 over a window, error rate, oldest waiting
delivery). One instance cannot judge these for a fleet, so the compiler
generates the rules for the monitoring tool into `build/monitoring/`:
OpenSLO and Prometheus rules first, other tools only when asked. The build
manifest carries the same thresholds, so a tool that reads it needs no
generated rules. Rules are applied at deploy; a change made in the tool is
overwritten, as with Terraform.

- **Starting values** come from what the compiler knows (queries per path
  from area 6's query report), then from production once it has run. Local
  timings are not production timings; locally the compiler reports
  relative change on the same fixtures ("30% slower", "2 more queries").
- **An agent proposes changes** from observed data as an ordinary source
  diff, through the MCP server (area 17).
- **Loosening needs a person.** Raising or removing a threshold is how an
  agent silences an alert, so it needs an acceptance file (area 7's form)
  and appears in the approval report; tightening does not.
- **Per build and per flag value.** Thresholds belong to a build, and each
  flag value takes its own branch, so its own path and metrics, without
  extra tagging.

OpenTelemetry stays the transport; only what is sent changes.

## Email

Applications send a handful of messages: confirm your address, reset your
password, an invoice. A message is a type, `type X = EmailMessage`, whose fields are
the values its text uses, in its domain's `emails/` folder beside its HTML
and text templates (question 11). Its subject line is its member in the
domain's language files, with its fields as placeholders. The templates'
words come from the domain's translations (area 10): a template calls
`{{ translations.<domain>.<message>(...) }}`, which takes the email's
locale, and the compiler checks every call and placeholder against the
message types and the email's fields.

```jadpo fragment
// domains/accounts/emails/welcome_email.jadpo
/// Sent once a user has signed up.
type WelcomeEmail = EmailMessage {
    display_name: DisplayName
    sign_in_link: Url
}

// domains/accounts/translations/welcome_heading.jadpo
/// The heading of the welcome email.
type WelcomeHeading = Message {
    name: DisplayName
}

// domains/accounts/translations/en_gb.jadpo
translations {
    welcome_email: "Welcome, {display_name}"      // the subject line
    welcome_heading: "Welcome, {name}"
}
```

```text
<!-- domains/accounts/emails/welcome_email.html -->
<html dir="{{ locale.direction }}">
<h1>{{ translations.accounts.welcome_heading(name: display_name) }}</h1>
<a href="{{ sign_in_link }}">{{ translations.accounts.sign_in() }}</a>
</html>
```

**Templates are strict** (decided 2026-10-16, built with email in chunk
7.3). These are compile errors: a name that is not a field of the email's
type; a message that does not exist; a placeholder missing or of the wrong
type; a partial that does not exist or is called with the wrong fields;
`if` on something that is not `Bool`; `for` over something that is not a
list; and any call other than translations and formatting. A field the
template never uses is a warning.

**Only text and numbers are written out**, in templates as in
placeholders: `{{ value }}` takes `Text`, `PresentationText`, `Int` or
`Decimal` (numbers written the locale's way), or an enum whose values have
wording (area 10). Writing anything else out is a compile error that names
how to format it: an `Instant` or `Money` with `text.format` (area 19),
a `Bool` as a condition for `if`, a list walked with
`for`, an object's fields one by one, an enum without wording by adding
it. Values are escaped; HTML goes in unescaped only as the safe-HTML type,
which the standard library defines (input to chunk 5.4): area 19's
`Html`, made safe by its construction (amended 2026-10-18 by area 19,
built in chunk 5.5o).

Sending is emitting: the mail goes after the action commits, retried like
any delivery (area 12), so a rolled-back sign-up never sends a welcome. Every
`EmailMessage` has `to:` and `locale:`.

```jadpo fragment
emit WelcomeEmail {
    to: user.email
    locale: user.locale
    display_name: user.display_name
    sign_in_link: config.app_url
}
```

- **Where it goes** is the `email` capability (area 11, revised
  2026-10-13): `email: { connection: postmark  from: config.mail_sender }` in
  `application { }` names a connection, and every email, the app's and
  Jadpo's built-in ones, goes through it (see "Built-in emails"). SMTP is one built-in driver
  (`driver: Driver.smtp`, port 587 on Cloudflare Workers, which blocks 25);
  a mail package (Postmark, Amazon SES, Resend) is another. The `EmailMessage`
  types, translations and `emit` are the same whichever is chosen, so
  changing provider changes `app.jadpo` only.
- **Attachments are part of the capability** (decided 2026-10-16): a
  `File` field on an `EmailMessage` is an attachment, read from its store
  at send time and checked against the size limit the mail package
  declares; a `max_size` above it is a compile error. A provider that
  cannot send attachments stays an ordinary package but cannot be the
  `email:` connection.
- **Everything provider-specific is the package's own** (design C, decided
  2026-10-16): per-send values such as Postmark's message streams or SES
  configuration sets are typed settings on the connection; hosted
  templates with variables (`postmark.email.send_with_template(...)`),
  suppression lists and marketing sends are the package's operations,
  called from an action or subscriber in a domain the connection lists.
  One connection serves both: `email:` uses it, and the listed domains call
  it. What hosted-template mail does not get: a dev inbox entry, the health
  score and the automatic template check (an app can add a contract case).
- **Bounces and complaints are the package's own webhook events**,
  subscribed to like any package event; there are no
  standard `EmailBounced` and `EmailComplained` events. They return to the
  capability only when a built-in feature consumes them (Jadpo refusing to
  send to an address that bounced permanently), and then every email
  connection must report them. Moving provider rewrites the bounce
  subscribers.
- **Sends without an idempotency key** (Postmark, SES, SendGrid and
  Mailgun take none) keep area 12's rule: a send whose outcome is unknown
  is parked for an operator, never resent blindly, since a duplicate
  receipt is not harmless; `jadpo deliveries` can resend in bulk.
- Tests capture mail instead of sending it, and read it as `email.sent`.
- **`jadpo dev` catches every email in a local inbox** (Richard, 2026-10-13).
  Nothing leaves the machine. A person opens it in the browser from the dev
  console (area 17): each mail rendered as HTML and as text, in its locale,
  with its links working (a magic link opens the frontend's page, which
  signs in locally) and a link to
  the trace of the request that sent it. An agent uses the same inbox from
  the command line: `jadpo email list`, `jadpo email show <id> --json`,
  `jadpo email links <id>` and `jadpo email open <id> <link>`, so it can follow
  a sign-up or privacy-request flow end to end; `jadpo email clear` empties
  it. Phoenix's dev mailbox and Mailpit offer the browser half; the
  agent commands and the trace link are new.
- **A mail health score: an early indication, not a guarantee** (Richard,
  2026-10-13). Every mail in the dev inbox, and `jadpo email show <id>`,
  shows what Jadpo's own checks found and how to fix each: no plain-text
  part, image-only content, broken HTML, links that are not absolute HTTPS,
  link shorteners, an all-capitals subject, a missing unsubscribe header on
  bulk mail, more than 102 KB (Gmail clips it). `jadpo check` warns about
  templates that would fail (a declinable error from chunk 5.4b, area 1's
  question 11). When SpamAssassin is installed its open rules
  are added. `jadpo email check-domain` compares the sending domain's SPF,
  DKIM and DMARC records with what the mail connection needs. What it does
  not do, said on every screen that shows a score: it cannot predict whether
  Gmail, Outlook or any other provider puts a mail in the inbox, because they
  use their own filters and the sender's history, which change constantly.
  A clean score means the common mistakes are absent, nothing more; it must
  not be relied on. Testing real inbox placement needs seed-account services
  outside Jadpo.
- The personal-data report lists the mail connection as receiving every
  marked field a mail carries; a mail package that declares an `erasure:`
  operation is called when a person is erased (area 15, "Erasure at
  providers").
- Two kinds of mail through two connections (transactional and bulk) wait
  for an app that asks.

### Built-in emails

> Decided on 2026-10-16 (Richard). Built with email (chunk 7.3) and the
> features that send them: magic-link sign-in (area 8, not yet on the
> roadmap) and data requests (area 15, after the first build).

Jadpo sends some emails itself: the magic-link email (area 8) and the
privacy emails (area 15). Each works with nothing written, and an app can
change as much of it as it needs, in five levels. Each level keeps the ones
below it.

**Delivery.** Every email, built in or the app's own, goes through the
app's one `email:` setting in `application { }`, which names a connection
providing `email` (area 11): the built-in SMTP driver or any package that
provides it. (The SMTP driver's own connection settings are area 11's.)

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

`magic_link:` with no `email:` setting is a compile error, unless it sends
its own email (level 5).

**Level 1: nothing.** Jadpo renders its own email, HTML and text, in the
person's language, with Jadpo's wording. Jadpo ships its wording in
`en_gb` and `en_us` at first, and releases add locales. An app that
supports a locale Jadpo does not ship must supply the wording (level 2):
until it does, `jadpo check` fails with one error listing every Jadpo
message the locale is missing, unless the locale is `in_progress:`, which
falls back as any translation does (area 10).

**Level 2: change the words.** The app writes Jadpo's messages in the root
`translations/<locale>.jadpo`, under `jadpo: { }` (area 10, "Jadpo's and
packages' text"):

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

Rewording a message of a feature the app has not switched on (magic-link
wording without `magic_link:`) is an error saying the feature is off, and
`jadpo translations builtin` lists only the messages of the features the
app has on, with their fields. Jadpo shows no pages, so its wording is
for emails and failures only. Every Jadpo message may also use
`{app_name}`, the application's `display_name:` (below); an app that needs
the name translated rewords the messages. The magic-link email uses these
messages:

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

`expires_in` is `expires_after:` as Jadpo formats it in the email's
language ("15 minutes"). The failures' wording also goes into the
per-locale failure export for client apps (area 10).

Built in chunk 5.4e: these twelve messages are in Jadpo's catalogue (area
10, "Jadpo's and packages' text") with the wording above, but no application
has the feature on until magic-link sign-in is built, so none is listed,
required or rewordable yet.

Passkeys (area 8, "Passkeys") bring no email, only their failures' wording,
which a client app shows; they are on, listed, required in an app's other
locales and rewordable once `passkey:` is declared (chunk 5.5z):

| Message | `en_gb` wording |
|---|---|
| `passkey_challenge_expired` | "That took too long. Try your passkey again." |
| `passkey_rejected` | "We couldn't sign you in with that passkey. Try another passkey, or sign in with your email." |
| `passkey_already_registered` | "This passkey is already set up." |
| `passkey_limit_reached` | "You have as many passkeys as an account can hold. Remove one to add another." |
| `passkey_not_found` | "That passkey isn't on your account." |

**Level 3: branding.** Every email, built in or the app's own, is wrapped
in the app's shared email layout, so built-in emails carry the app's
branding with no other change. The layout is optional, one per app, in
`shared/emails/layout.html` and `shared/emails/layout.txt`; without it
Jadpo's plain layout is used. It is checked under the strict template
rules and may call translations, and it receives:

| In the layout | What it is |
|---|---|
| `{{ content }}` | The rendered body: the safe-HTML type in `layout.html`, text in `layout.txt` |
| `{{ subject }}` | The email's subject line, `PresentationText` |
| `{{ locale.direction }}` | `ltr` or `rtl` (area 10) |
| `{{ app.display_name }}` | The application's `display_name:` |

```text
<!-- shared/emails/layout.html -->
<html dir="{{ locale.direction }}">
<head><title>{{ subject }}</title></head>
<body>
  <header>{{ app.display_name }}</header>
  {{ content }}
</body>
</html>
```

**The application's name.** `application { display_name: "Acme" }` in
`app.jadpo`, plain text, is required once any built-in email is
switched on (magic links, the privacy emails); without it `jadpo check`
names the feature that needs it. Templates read it as
`{{ app.display_name }}`, and Jadpo's own wording as `{app_name}`.

**Level 4: the app's own email.** `magic_link: { email: accounts.SignInEmail }`
names an `EmailMessage` type of the app's that Jadpo fills and sends instead
of its own. The type takes its fields from the built-in `MagicLinkEmail`
with `pick:`, and may have no others, since Jadpo fills it:

| `MagicLinkEmail` field | Type | Holds |
|---|---|---|
| `address` | `Email` | The address the link was asked for; also the email's `to:` |
| `sign_in_link` | `Url` | The link to the frontend's `link:` path, carrying the token |
| `code` | `SignInCode` | The six-digit code, text so a leading `0` stays |
| `expires_in` | `PresentationText` | How long both work, formatted by Jadpo in the email's language |

Every `EmailMessage` also has `to:` and `locale:` (above); Jadpo sets the
locale as for its own email (area 8).

```jadpo fragment
// domains/accounts/emails/sign_in_email.jadpo
/// The sign-in email, in the app's own words and design.
type SignInEmail = EmailMessage {
    pick: [MagicLinkEmail.sign_in_link, MagicLinkEmail.code, MagicLinkEmail.expires_in]
}

// domains/accounts/translations/en_gb.jadpo
translations {
    sign_in_email: "Your Notes sign-in link"       // the subject line
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

```text
<!-- domains/accounts/emails/sign_in_email.html -->
<h1>{{ translations.accounts.sign_in_heading() }}</h1>
<p><a href="{{ sign_in_link }}">{{ translations.accounts.sign_in_button() }}</a></p>
<p>{{ translations.accounts.sign_in_code_line(code: code) }}</p>
<p>{{ translations.accounts.sign_in_expiry(expires_in: expires_in) }}</p>
```

The messages it calls (`SignInHeading`, `SignInCodeLine` ...) are the
domain's own `Message` types, one per file in `translations/`. Its `.html`
and `.txt` templates sit beside it in `emails/`, its subject is
its member in the domain's language files, and all of it follows the strict
template rules above; it is wrapped in the shared layout like any of the
app's emails. `MagicLinkEmail` is never emitted by app code.

**Level 5: the app's own sending.** `magic_link: { send: accounts.send_sign_in_link }`
names an action that sends the email itself, through any package, instead
of Jadpo sending one. Its signature is fixed: it takes one built-in
`MagicLinkRequest` and returns `Unit`.

| `MagicLinkRequest` field | Type |
|---|---|
| `address` | `Email` |
| `sign_in_link` | `Url` |
| `code` | `SignInCode` |
| `expires_at` | `Instant` |
| `locale` | `Locale` |

```jadpo fragment
// domains/accounts/actions/send_sign_in_link.jadpo
action send_sign_in_link(request: MagicLinkRequest) fails MessageNotSent -> Unit {
    attempt messages.send(...)          // a package of the app's choice
}
```

- It runs after the request commits, like a subscriber, and is retried
  under area 12's rules; its failures are a subscriber's.
- In development its provider calls are faked and logged, as every
  provider call is (area 11), and the local inbox does not show it, since
  Jadpo sends no email.
- Levels 4 and 5 are exclusive: `email:` and `send:` on one email is a
  compile error.

**Every built-in email.** The same five levels apply to each email Jadpo
sends. A feature with one email takes `email:` or `send:` directly; a
feature with several takes `emails: { }`, one entry per email, each taking
`email:` or `send:`, never both. Delivery is always the one `email:`
connection.

| Email | Built-in types | Sent when | Configured in |
|---|---|---|---|
| Magic link | `MagicLinkEmail`, `MagicLinkRequest` | `POST /auth/magic-link` (area 8) | `authentication { sign_in: { magic_link: { email: / send: } } }` |
| Address check | `PrivacyAddressCheckEmail`, `PrivacyAddressCheckRequest` | A person asks through `POST /privacy/requests` (area 15) | `privacy: { emails: { address_check: { } } }` |
| Export ready | `PrivacyExportEmail`, `PrivacyExportRequest` | `PrivacyExportReady` (area 15) | `privacy: { emails: { export_ready: { } } }` |
| Erasure confirmed, with its cancel link | `PrivacyErasureConfirmedEmail`, `PrivacyErasureConfirmedRequest` | `PrivacyErasureScheduled` (area 15) | `privacy: { emails: { erasure_confirmed: { } } }` |
| Erasure completed, with what was kept | `PrivacyErasureCompletedEmail`, `PrivacyErasureCompletedRequest` | `PrivacyErasureCompleted` (area 15) | `privacy: { emails: { erasure_completed: { } } }` |

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

The privacy emails' fields, which Jadpo fills: every one has `address:
Email` (also its `to:`) and the `locale:` of the request, and each request
type has the same fields with times as `Instant` in place of text.

| Email type | Its other fields | Request type's other fields |
|---|---|---|
| `PrivacyAddressCheckEmail` | `confirm_link: Url`, `expires_in: PresentationText` | `confirm_link: Url`, `expires_at: Instant` |
| `PrivacyExportEmail` | `download_link: Url`, `expires_in: PresentationText` (7 days) | `download_link: Url`, `expires_at: Instant` |
| `PrivacyErasureConfirmedEmail` | `erases_on: PresentationText`, `cancel_link: Url` | `erases_at: Instant`, `cancel_link: Url` |
| `PrivacyErasureCompletedEmail` | `kept: List<PrivacyKeptRecord>` | `kept: List<PrivacyKeptRecord>` |

`PrivacyKeptRecord` has `name: PresentationText`, the kept record's wording
(its entity's member in its domain's language files, required for every
entity marked `Erasure.retained`), and `kept_until: PresentationText`, the
end of its retention in the email's language. A template walks `kept`
with `for`. The address-check link lasts 15 minutes, as a magic link
does.

## Documents

Decided 2026-10-16 (Richard). The `documents` capability renders a typed
template to HTML or PDF exactly as email renders a message: a
`type X = Document { ... }` whose fields are the values its template uses,
in its domain's `documents/` folder beside its `.html` template, under the
strict template rules above, reading `translations.` with the locale it is
given, wrapped in the optional `shared/documents/layout.html`.

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

// domains/billing/actions/render_invoice.jadpo, run by a subscriber
var pdf: Bytes = attempt documents.pdf(InvoiceDocument { ... }, locale: customer.locale)
var file: File = attempt invoices.put(pdf, FileType.pdf)
```

- `documents.pdf(document, locale:)` returns the PDF's `Bytes`;
  `documents.html(document, locale:)` returns the rendered HTML, for
  previews.
- `documents: { layout: ... }` names the shared document layout file, as
  email's shared layout is found; left out, `shared/documents/layout.html`
  is used if it exists, and Jadpo's plain layout otherwise.
- The connection turns HTML into a PDF: Gotenberg as a container package
  (`runs:`, area 11; started by `jadpo dev` in development) or a hosted
  API's package (DocRaptor, PDFShift). `documents:` with no connection that
  provides `documents` is a compile error.
- Rendering is heavy work, so it runs in subscribers, never in a route,
  capped by the provider's `max_concurrent:` (area 11); a `documents.pdf`
  call reached from a route is a compile error naming the subscriber
  shape.
- In tests the fake returns a placeholder PDF, and a test can read the
  rendered HTML (area 13).

## Search

An entity with a `search:` section can be searched by words (question 12,
revised 2026-10-16). The compiler generates the index, keeps it in step and
generates its migration, and a query uses `matches`. Search is the `search`
capability: PostgreSQL full-text search is the built-in default, and an
engine's package (Meilisearch, Typesense, Algolia, OpenSearch) can fill it.

```jadpo fragment
// domains/planning/entities/todo.jadpo
entity Todo {
    ...
    search: {
        fields: [title, notes]
        language: SearchLanguage.english
    }
}

// domains/planning/queries/search_todos.jadpo
query search_todos(owner_id: User.id, search_phrase: SearchText) -> Page<TodoView> {
    from: Todo
    where: Todo.owner_id == owner_id
    where: Todo matches search_phrase
}
```

- Fields listed first count most when ordering results.
- With no `order:`, best matches come first, then by `id`, in keyset pages
  like every other list (area 6).
- `search_phrase` takes what people type into search boxes: words, quoted phrases and
  `-word`, never raw query syntax.
- This replaces area 5's `kind:` full-text index: one declaration, not an
  index plus hand-written conditions.

**An engine instead of PostgreSQL.** `search: { fields: [...]  store:
products_index }` names a connection whose package provides `search`;
left out, the search is PostgreSQL's. The engine's package does the whole
job: upsert a record's searchable fields, remove one, and query by words
with equality filters.

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
  entity whose search uses an engine may carry only `where:` lines of
  equality on fields the index holds; anything else is a compile error
  naming PostgreSQL search, which can filter any way.
- **Ids from the engine, records through policy.** The engine returns
  ranked ids, already filtered by tenant; Jadpo then reads those records
  through the query, with policy applied as for any query (area 9), so a
  result is never one the caller could not read.
- **Kept in step after commit**: every committed change to a searchable
  field reaches the engine through the outbox (area 12), retried like any
  delivery; erasing a person (area 15) removes their records from the
  index; `jadpo search reindex <entity>` rebuilds it.
- **Engine features** (synonyms, ranking rules, typo tolerance settings)
  are the package's own operations, called from a domain the connection
  lists.
- **Development and tests use PostgreSQL search** whatever production
  uses, so nothing leaves the machine and no engine is needed to run a
  test.

## Bot protection

Decided 2026-10-16 (Richard). `bot_check` is a capability with one
operation, check a token: pass or fail. A package fills it (Cloudflare
Turnstile, hCaptcha, reCAPTCHA), named in `application { }`; a route, or a
generated feature, asks for it with `bot_check: true`.

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

- **Jadpo checks it on its own public endpoints** (the magic-link and
  privacy requests) whenever `bot_check:` names a connection.
- **The frontend shows the widget**; the token travels in the
  `X-Bot-Check` header, never in the route's input, and is checked before
  the action runs, within the request's deadline.
- **A failed check** is the built-in failure `bot_check_failed` (403),
  worded from translations like every failure (area 10).
- **reCAPTCHA's score** gets a threshold setting on its connection.
- **An unreachable provider refuses the request** (fails closed); letting
  requests through when it is down needs an acceptance (area 7's form),
  listed in the approval report.
- **Development and tests pass every check** through a local driver; a test
  can script `bot_check_failed` (area 13).
- `bot_check: true` with no `bot_check:` connection is a compile error
  naming the setting to add.

## Outgoing webhooks

Built later (question 8), on `publishing` (area 12): an app's customers
register an address, and chosen public events are delivered to it, typed
from the event, signed and retried. Decided 2026-10-16 (Richard): they get
**one network exception owned by the runtime**, since their addresses are
the customers' and cannot be listed as connections. Its guards are fixed:
HTTPS only, no private, loopback or link-local addresses (checked on the
resolved address, at every attempt), and no redirects followed. The
exception is listed once in the network report (area 11), and nothing else
in the app, a package included, can reach an address that is not a
declared connection.

## Not built in

| Feature | Why not, or not yet |
|---|---|
| Realtime subscriptions | Policy on a live stream of changes is a large design; later, after the golden todo. Until then, clients poll with the revision header (area 7) |
| Generated admin screens | Jadpo makes backends, not interfaces. Operators use `jadpo` commands (area 14); a screen can come later on the same operations |
| Payments | Money moves at the provider; an integration plus webhooks (areas 7 and 11) is the right shape, and the compiler gains nothing more |
| Content management | Content is application data |
| Product analytics | Events and metrics above feed whichever tool a team uses |
| Quotas and plan limits | A plan's limits are application data, checked by an action; counting uses area 6's counters |
| Background job dashboards | Already area 14's operator commands (question 13 there) |

## Errors

### `data.file_field_invalid`

A `File` field says where its bytes are kept, `store:` naming a
connection that provides `files`, and how many it holds, `max_size:`;
`types:`, when given, lists `FileType`'s variants. A field whose
`store:` names the database is refused:

```jadpo check project=e-file-field-invalid file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-file-field-invalid file=domains/bakery/entities/price_sheet.jadpo expect=data.file_field_invalid
/// A supplier's price sheet, kept as it was sent.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: main
        max_size: 5mb
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```text output
error: `PriceSheet.sheet`'s `store:` isn't a store of files
 --> domains/bakery/entities/price_sheet.jadpo:5:16
  |
5 |         store: main
  |                ^^^^ not a `files` connection
  reason: A `File` field's `store:` names a connection that provides `files`; `main` is a connection that keeps no files, and `app.jadpo` declares `sheets`.
  next: Write `store: sheets` (agent)
  rule: data.file_field_invalid
  help: docs/language/16-built-in-services.html#data-file-field-invalid

check failed: 1 error, 2 files
```

### `type.file_read_too_large`

`store.read` reads a whole file into memory, so it reads a field only
when its `max_size:` is 10 MB or less; a file's rows, of any size, are
read a batch at a time by the format helpers:

```jadpo check project=e-file-read-too-large file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-file-read-too-large file=domains/bakery/entities/price_sheet.jadpo hidden migrate=initial
/// A supplier's price sheet, kept as it was sent.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 50mb
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=e-file-read-too-large file=domains/bakery/actions/sheet_bytes.jadpo expect=type.file_read_too_large
use app { sheets }
use bakery/entities { PriceSheet }

/// A price sheet's bytes, whole.
action sheet_bytes(sheet: PriceSheet.sheet) -> FileContents {
    return sheets.read(sheet)
}
```

```text output
error: `sheets.read` can't read `PriceSheet.sheet` whole
 --> domains/bakery/actions/sheet_bytes.jadpo:6:24
  |
6 |     return sheets.read(sheet)
  |                        ^^^^^ holds up to 50 MB
  reason: `sheets.read` reads a whole file into memory, for passing its bytes on, so it reads at most 10 MB; `PriceSheet.sheet` holds up to 50 MB. A file's rows, of any size, are read a batch at a time by the format helpers (`csv.read`, `tsv.read`, `json_lines.read`).
  next: Read `PriceSheet.sheet`'s rows with a format helper, such as `csv.read`, or keep its `max_size:` at 10 MB or less if `sheets.read` is what it needs (agent)
  rule: type.file_read_too_large
  help: docs/language/16-built-in-services.html#type-file-read-too-large

check failed: 1 error, 3 files
```

## Review

### Status

Email comes with chunk 7.3, and default rate limits with routes (chunk
4.1); flags, the rest of files (multipart slots, and image variants and
scanning when an app needs them),
audit, search, documents, bot protection, the `telemetry:` section and
monitoring thresholds come after the first build ("Remaining built-in
services" on the roadmap). The errors this page describes are built with
the feature they belong to. Built in chunk 7.0c: the seam every
capability's calls will go through, with a test capability standing in
for the first real one, and a delivery that hands its event to a
capability, parked as `outcome_unknown` when a send without a key may
have acted ("Chosen in chunk 7.0c" below). Built in chunk 6.4b: the
`files` capability's first part on that seam, `Driver.disk`
connections, `File` fields, `store.put`, `store.read` with its
whole-read cap and its two errors, the disk driver `jadpo dev` keeps
files with and the in-memory store a test's are in, which the file
helpers' reads walk (area 19, "Data formats"; "Chosen in chunk 6.4b"
below). Built in chunk 6.4b2: the pieces the file helpers' writes put,
joined into one file or dropped by the store ("Chosen in chunk 6.4b2"
below). Built in chunk 6.4b3: a gzip file unpacked as the file helpers
read it, up to `max_unpacked_size:` ("Chosen in chunk 6.4b3" below).
Upload slots, downloads, deleting and erasing a file's bytes,
the S3 driver, a deployable's disk driver at its `path:` and
`free_space_floor:`, and checking a stored file against its field's
`max_size:`, `types:` and `store:` as it is written come with the rest
of the files capability.

### Chosen in chunk 7.0c

What "Capabilities on this page", "Sends without an idempotency key" and
area 6's "Outside calls hold no transaction" leave to the build of the
seam, from the Phase 6 architecture review, each the most conservative
reading:

- **One way out.** Every call that leaves the process, a capability's or
  a provider's, goes through one runtime entry point that both executors
  call. A driver is named by the connection it serves (area 11) and
  answers in one of five ways: a value; one of its operation's own
  failures, which the call maps; *surely did nothing*; *may have acted*;
  or *misconfigured*. A call to a connection no driver fills is
  `misconfigured` and never sent.
- **Logged on its invocation, without its values.** Each call is logged
  as it goes out and again when it is answered: its connection and
  operation, whether it may be sent again (it changes nothing, it
  changes something under a key, or it changes something with no key),
  where its answer came from (the driver, a fake, a replay), when it went
  out, how long it took, and how it ended. Never its input or its
  answer, which can hold secrets and personal data; `jadpo calls`'s full
  values in development (area 17) are kept apart, by `jadpo dev`'s host
  only (built in chunk 7.2d, with its screen). A
  response carries its request's log.
- **Recorded and replayed with the other effects.** A recording keeps
  each call's answer, in order, beside the random bytes it keeps, and a
  replay gives the answers back without the calls going out; a replayed
  call the recording doesn't have next is refused before it goes out.
- **`outcome_unknown` only where a repeat could act twice.** A call that
  may have acted raises `outcome_unknown` when it changes something with
  no key; one under a key, or one that changes nothing, raises
  `unavailable`, since sending it again acts once.
- **A delivery can target a capability.** Besides a subscriber's action,
  a delivery can hand its event whole to an operation of the capability
  a connection fills, as an `EmailMessage`'s will (chunk 7.3). Its record
  alone says it was sent: there is no action, so no handled record.
- **Parked, never sent again blindly.** A delivery is parked as
  `outcome_unknown` for an operator (area 12, "What an operator sees")
  when its send may have acted and has no key: its driver said so, its
  run's time ran out while the send was out, or its worker was lost
  while it was out. An unkeyed send is marked `sending` on its delivery
  record before it goes out, so the sweep parks one whose lease passed
  instead of claiming it again. A subscriber's action that sent such a
  call and then ended in a way that is otherwise retried (a store that
  couldn't be reached, its time running out) is parked too, since a
  retry would send the call again; one that fails for good stays
  `failed`. A send under a key, or one that surely did nothing, is
  retried as any delivery is. A subscriber's action whose worker is lost
  while its call is out is the step record's (area 12, "Several changing
  calls in one action"; chunk 7.5): until then it is claimed again as
  any lost delivery is. A schedule's run has no retry to stop, its next
  run being the next tick's, so its run record keeps how it ended, as
  before.
- **No call inside an open transaction.** A call made once the action
  has read or written, while its transaction is open, is refused before
  it goes out, with a fault naming the call and saying to make it before
  the action's first read or write. Closing a read snapshot before the
  call instead, and the checker proving it, are chunk 7.4's. A batch's
  items run inside their batch's transaction, so an item's call is
  refused the same way until then.
- **Fakes wrap the host.** A fake is a host wrapper giving a scripted
  driver in some connections' place, everything else the host's own: a
  test runs on it, and `jadpo dev`'s host gives a package's fake the same
  way (built in chunk 7.2d). The runtime's Rust
  tests share one test host, which fakes wrap the same way.

### Chosen in chunk 6.4b

What "Files" and "The disk driver" leave to the build of the `files`
capability's first part, each the most conservative reading:

- **What a `File` holds.** The connection its bytes are in, the key the
  runtime made for them, its name (none for one `store.put` made), its
  size and its `FileType`, kept as its JSON in a `jsonb` column. Only the
  runtime builds one (`File { ... }` is `type.not_record`), no request
  carries one and no output sends one (an output's signed link comes
  with downloads), and a program reads none of its parts yet.
- **A `File` field's settings.** `store:` must name a connection that
  provides `files`, and `max_size:` is required, a whole number of `b`,
  `kb`, `mb` or `gb`, each a thousand times the last (`5mb` is
  5,000,000 bytes, as this page writes "100 MB" apart from "16 MiB");
  `types:` is optional. A mistake in any is `data.file_field_invalid`,
  and the three on a field that isn't a `File` are
  `type.unknown_setting`.
- **`store.put(bytes, type)`** is called only in an action, before its
  first read or write, as every outside call is. Its key is made from
  the invocation's randomness, so a replay keeps it; it goes out as a
  keyed call, since writing the same key again writes the same immutable
  object. A store with no room answers `storage_full`, the call's one
  condition, which its `attempt` maps to an `unavailable` failure (not a
  value's fault, so `invalid:` doesn't cover it); a store that can't be
  reached is the runtime's `unavailable`.
- **`store.read(file)`** reads at most 10 MB into memory: a field over
  it is `type.file_read_too_large` where it is read, and a `File` whose
  field isn't known (a parameter of type `File`) is held to it as it
  runs, a fault. A file whose bytes are gone from its store is a fault
  too. `FileContents` is the file's bytes; nothing takes it yet but what
  passes bytes on (an email's attachment, chunk 7.3; a provider's upload,
  chunk 7.2). Like `put`, it is called only in an action, before its
  first read or write; a test reaches files through the application's
  actions.
- **The disk driver in `jadpo dev`.** Every store of files is kept under
  `.jadpo/files/<connection>/`, whatever its driver, so nothing leaves
  the machine: a file under its key, sharded into two directory levels
  by its id, a key of any other shape refused, a symbolic link anywhere
  on the way refused, written to a temporary file readable by the
  application's user only, synced, renamed into place and its folder
  synced; a full disk answers `storage_full`. Its I/O is blocking, a
  window at most a megabyte, on the developer's own disk.
- **In a test** one in-memory store serves every connection (area 13,
  "Provider calls and capabilities in tests"), and a file helper's run
  reads two rows a batch. `fixture_file`, and telling the store to fail
  from a test's source, come with area 13's fakes.

### Chosen in chunk 6.4b2

What the file helpers' writes (area 12, "How an export's run is kept";
area 19, "Data formats") leave to the `files` capability, each the most
conservative reading:

- **An export's pieces.** Each page's rows are an ordinary keyed `put`,
  under a key made from the file's own (`jadpo_std::files::piece_key`, a
  key of the runtime's shape, so a driver refuses no piece and finds
  each from the file's key alone). Two more operations go through the
  seam, each a keyed call: `join(key, pieces, type)` joins the pieces,
  in order, into the file under `key`, a repeat once it is whole doing
  nothing, and `discard(key, pieces)` drops a cancelled export's pieces,
  and its file if they were joined. A store with no room answers
  `storage_full`, and a join missing a piece `not_found`.
- **The disk driver joins whole.** It copies the pieces in order into a
  temporary file readable by the application's user only, a window at a
  time, syncs it, renames it into place and syncs its folder, then
  removes the pieces, so the file appears whole or not at all; the
  in-memory store a test's files are in does the same in memory.
- **`FileType.json_lines`,** `application/jsonl` as jsonlines.org names
  it, a written JSON Lines file's type, after `json`.

### Chosen in chunk 6.4b3

What "Uploads sent with a `Content-Encoding` are refused" and question 7
leave to the build of reading gzip, each the most conservative reading:

- **Only the file helpers unpack, and only when told how far.** A file
  whose first two bytes are gzip's magic number is unpacked by a format
  helper given `max_unpacked_size:` (area 19, "Data formats"), and by
  nothing else: `store.read` gives its bytes as they are stored, and a
  helper given none stops the run before its first row. The limit is the
  most text the run reads: it stops at the row that would reach past it,
  the rows wholly inside it done, and unpacks at most 64 KiB past it, so
  a small upload can't unpack to gigabytes.
- **gzip as RFC 1952 writes it, every member in turn.** A file is one
  member or more, one after another, as `cat a.gz b.gz` makes, read as
  one text. Each member's header is read whole, with every optional part
  its flags say it has (an extra field, a name, a comment) and its own
  CRC-16 checked when it has one; a reserved flag, a method other than
  deflate, or anything after a member that isn't another is damage. Each
  member's trailer is held to what it unpacked to, its CRC-32 and its size
  modulo 2³². A damaged file, and one cut short inside a member, stop the
  run where they are found, the rows before done. No other compression,
  and no archive (zip, tar), is opened.
- **Unpacked as it is read, never whole.** Its packed bytes are read
  from the store through the seam a window at a time, as a plain file's
  are, and unpacked (miniz_oxide's streaming inflate) as far as a page
  needs. gzip can't be entered part way, so a runner that picks a run up
  unpacks the file from its start to the run's position, discarding what
  comes before; within one runner each page goes on from the last.
- **A file that isn't gzip is read as it is,** whatever
  `max_unpacked_size:` says, so one helper serves uploads that are packed
  and ones that aren't.

### Consistency with the rest of the language

- **A new root file.** `flags.jadpo` joins area 1's fixed root list, as an
  unnamed `flags { }` singleton like `config { }`.
- **Reading flags without a caller parameter.** `current_principal` exists
  only in routes (area 8), yet flags are evaluated for the caller inside
  actions. This follows policy, which also gets the caller from the runtime;
  the value is fixed per operation like `clock.now`.
- **Graceful degradation.** Flags and rate limits follow area 5's cache
  rule: an unreachable store never fails a request.
- **One store per action.** File bytes are written by the client and deleted
  by a generated subscriber, so no action writes two stores.
- **Mail as an event.** Reusing `emit` and `events/` adds no new statement or
  folder; `EmailMessage` is a new type base beside `Event` and `Failure`.
- **Capabilities.** Files, email, documents, search, bot protection and
  scanning follow area 11's capabilities under design C (decided
  2026-10-16): the provider is a connection in `app.jadpo`, and each
  capability is a job Jadpo itself does, whole. Flags, rate limits and
  audit stay in the app's own stores and are not capabilities.
- **Search through two stores.** An engine-backed search reads ids from
  the engine and records from the entity's store, which area 6's one-store
  rule allows: only the store's read decides what the caller sees.
- **Telemetry is not a connection.** It has one protocol and no
  vendor code, so it has its own section rather than a capability with a
  single driver.
- **Size literals.** `max_size: 5mb` is a new literal form, following the
  duration spelling of area 10 question 4.
- **Retention shape.** Audit retention reuses area 15's
  `retention: { after: }`; `since:` is always the entry's own time.
- **Environment variables.** Area 10 says every setting is declared in
  `config.jadpo`; the OpenTelemetry variables are read by the runtime under
  their standard names instead, listed in the configuration report.

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| `flags.jadpo` in the fixed root list; `EmailMessage` types in `emails/` | 1 | Questions 1 and 11 |
| `5mb` size literals; `File`, `FileUpload`, `UploadSlot`, `SearchText` built-in types | 2 | Questions 7 and 12 |
| File stores as `files` connections; `search:` replacing the full-text index `kind:` | 5, 11 | Questions 7 and 12 |
| `matches` in queries; relevance order in keyset pages | 6 | Question 12 |
| `rate_limit:` on APIs and routes; `GET /flags`; rate limits in OpenAPI | 7 | Questions 5 and 6 |
| Default rate limits on generated authentication routes | 8 | Question 6 |
| Per-tenant flag targeting depends on tenancy; slot and history grants | 9 | Questions 3, 7 and 9 |
| Translations format for mail (area 10 question 9, revised 2026-10-16: domain language files read as `translations.<domain>.<message>`); OpenTelemetry variables | 10 | Questions 10 and 11 |
| The safe-HTML type that templates write out unescaped (`Html`, built in 5.5o); the money formatter | Standard library (chunk 5.4) | Question 11 |
| Mail delivery and retries as an event; trace context through the outbox | 12 | Questions 10 and 11 |
| `flags:` in tests and the both-values check; `email.sent`; an in-memory file store in tests | 13 | Questions 4, 7 and 11 |
| `jadpo flags` commands and source removal; the `syntax.expected_declaration` message | 14 | Question 2; Status |
| File erasure and export; audit entries on erasure; field encryption | 15 | Questions 7, 8 and 9 |
| Where threshold declarations live | 1 | Question 15 |
| Loop and recursion rule for path numbering | 3 | Question 13 |
| Stable identity for branches as well as declarations | 5 | Question 13 |
| Loosened thresholds need an acceptance | 7 | Question 16 |
| `jadpo path`; path IDs in `jadpo trace` and `incident`; agent-proposed thresholds through MCP | 17 | Questions 13 and 15 |
| `FileContents`, `Document` and the batch types; `store.read`; `5%` in `free_space_floor:` (the `%` literal decided by area 19, built in chunk 5.5j2) | 2 | Questions 7 and 19 |
| The batch helpers (`csv.read`, `csv.write` ...), the outbox feeding search engines, `receiving` and `publishing` | 12 | Questions 7, 12 and 8 |
| Faked capabilities: `bot_check_failed`, the documents placeholder PDF, PostgreSQL search in tests | 13 | Questions 12, 18 and 19 |
| Search engines and erasure; `AuditEntryRecorded` copies of hashed fields | 15 | Questions 9 and 12 |
| `jadpo traces`, `metrics` and `logs`; `jadpo dev --export-telemetry` | 17 | Question 10 |
| `metric` declarations in `monitoring.jadpo` | 1 | Question 10 |
| The `CsvFormat`, `json` and `json_lines` namespaces and their encodings | Standard library (chunk 5.4) | Question 7; decided 2026-10-17 in area 19, "Data formats" |

### Open questions

1. **Are flags declared in a root `flags.jadpo`, with type, default, owner
   and expiry?** *Recommendation:* yes, `Bool` or an `Enum`, with `expires:`
   required. Laravel Pennant defines flags in code with no list or expiry,
   and teams lose track of them; one file is the list a reviewer reads. *Decided 2026-10-13:* yes, as recommended.
2. **Is flag state stored in the application's database and changed with
   `jadpo flags`?** *Recommendation:* yes, no flag service. Pennant also
   stores flags in the app's database; LaunchDarkly or Unleash would be a
   third-party dependency for something small enough to own. A web screen
   can come later on the same operations.
   *Revised 2026-10-13 (Richard):* the store is named, never assumed:
   `flags: { store: main }` in `app.jadpo`; without it, using a flag is a
   compile error (the area 5 rule for every system feature). *Decided 2026-10-13:* as revised.
   *Revised 2026-10-16 (Richard):* flags stay in the app's database and are
   not a capability; each server holds their values in memory, refreshed
   every few seconds, never in the cache. A `Driver.ofrep` store
   (OpenFeature's remote evaluation, evaluation only) waits for an app that
   must use a vendor.
3. **Which targeting rules?** *Recommendation:* named users, named tenants
   (once area 9 decides tenancy), a percentage by a hash of flag name and
   user `id`, then everyone; first match wins. No attribute rules
   ("country is FR") until an app needs them; Firebase and Unleash start
   from the same percentage rule. *Decided 2026-10-13:* yes, as recommended.
4. **What does the compiler check?** *Recommendation:* unknown flags are
   errors; tests must cover every value of each flag read; expired flags are
   warnings; "fully rolled out" comes from `jadpo flags list`, because the
   compiler cannot see production; `jadpo flags remove --keep` rewrites the
   source. *Decided 2026-10-13:* yes, as recommended.
5. **Should Jadpo support OpenFeature?** *Recommendation:* not now. Jadpo
   evaluates its own flags, so it needs no OpenFeature provider. Client
   flags go out on a plain generated `GET /flags`; adopt OpenFeature's remote
   evaluation protocol (OFREP) for that route once it is stable, which its
   authors say it is not yet.
   *Decided 2026-10-13 (Richard):* yes, and the client sees its own state:
   `GET /flags` returns every flag marked `client: true`, evaluated for the
   caller (their user, tenant and rollout bucket), so a frontend shows
   exactly what this user has. Flags without `client: true` never leave the
   server. *Decided 2026-10-13:* as revised.
6. **Are rate limits `rate_limit: { requests: within: by: }` on an API or
   route, counted in Redis with an in-memory fallback?** *Recommendation:*
   yes, keyed by caller or address, with built-in limits on the generated
   authentication routes. This is Rails 8's `rate_limit to: within: by:` in
   Jadpo's form.
   *Revised 2026-10-13 (Richard): on by default for every route.* With no
   setting, every API gets sensible limits: per signed-in caller 600 a
   minute, per address 120 a minute on public routes, and 10 a minute per
   address on the generated sign-in and privacy routes; `rate_limit:`
   loosens or tightens them per API or route, and turning one off needs an
   acceptance. Counts live in Redis when a cache connection is named
   (`rate_limits: { store: cache }`); without one they are kept in memory
   with a fixed allowance (`memory: 16MB` by default, oldest counters
   dropped first), and `jadpo check` warns that limits are then per
   instance, multiplied by the number running, and recommends Redis (a
   declinable error from chunk 5.4b, area 1's question 11). On
   Workers there is no lasting memory, so a Redis connection (or
   Cloudflare's rate-limiting binding as a driver) is required. *Decided 2026-10-13:* as revised.
   *Revised 2026-10-16 (Richard):* on Workers, Cloudflare's rate-limiting
   binding is a store for the limits it can count (windows of 10 or 60
   seconds, to check), and any other window on it is a compile error naming
   Redis; a refused caller gets area 7's `429` with `Retry-After` and the
   standard `RateLimit` headers, worded from translations. The same
   `rate_limits: { store: }` holds provider rate limits, shared across
   servers.
7. **Are files a `File` field with a file store, direct uploads through a
   signed slot, and signed links in outputs?** *Recommendation:* yes, S3
   protocol only. It is how Rails Active Storage and Laravel work, and the
   compiler then applies policy, erasure and retention to the bytes.
   Image resizing and virus scanning wait for a real app.
   *Revised 2026-10-13 (capabilities, area 11 questions 19–24):* the store
   is a connection providing `files`: the built-in S3 driver, or a package
   that is a preset over it (R2's upload slots still need S3 keys, even on
   Workers); local disk in development and tests; `store.put(bytes, type)`
   for files the application makes. *Decided 2026-10-13:* as revised.
   *Revised 2026-10-16 (Richard):* no package is a preset over the S3
   driver: R2 and the other S3-compatible services are `Driver.s3` with
   their host and settings. `Driver.disk` serves production on the native
   target only, the manifest recording one server, as "The disk driver"
   says. Storage and formats are separate layers: `store.read` returns
   `FileContents` for passing bytes on, and rows are read and written only
   through area 12's batch helpers. Fields over 100 MB upload through a
   multipart slot; gzip is read only with `max_unpacked_size:`, and no
   archives. Image variants are made by the runtime over any store and
   scanning is the `scanning` capability, each built when an app needs it;
   media services stay ordinary packages.
8. **Should outgoing webhooks and field encryption be built in?**
   *Recommendation:* yes, both later. Outgoing webhooks are events with
   signed, retried delivery to a customer's address, which the compiler can
   type from the event and document; field encryption (Rails has
   `encrypts`) belongs in area 15, which knows sensitive fields. Neither is
   needed by the golden todo. *Decided 2026-10-13:* yes, as recommended.
   *Revised 2026-10-16 (Richard):* outgoing webhooks are built on
   `publishing`, with one runtime-owned network exception whose guards are
   fixed (HTTPS only, no private addresses, no redirects), listed once in
   the network report.
9. **Is the audit trail an opt-in `audit:` section, written in the same
   transaction, with personal fields hashed?** *Recommendation:* yes, opt-in
   per entity, history readable with `read` on the record. Rails and Django
   need a library (PaperTrail, django-simple-history) for this, and those
   cannot hash personal fields or know the request. *Decided 2026-10-13:* yes, as recommended.
   *Revised 2026-10-16 (Richard):* entries can also go out as a built-in
   `AuditEntryRecorded` event, emitted only when something listens, through
   `publishing`; the database stays the record of truth.
10. **Is OpenTelemetry generated for everything, configured by the standard
    variables?** *Recommendation:* yes, traces and metrics with no syntax and
    no values in spans. Phoenix generates telemetry for every request and
    query the same way; using the standard variables means any monitoring
    tool works. *Decided 2026-10-13:* yes, with its settings declared in `config.jadpo` like any setting, bound to the standard `OTEL_*` variable names (area 10).
    *Revised 2026-10-16 (Richard):* telemetry is its own `telemetry:`
    section in `application { }` (endpoint, credential header, `send:` and
    `detail:`), not a connection, one protocol with no vendor code; the
    `OTEL_*` variables fill it. Custom metrics are declared in a domain's
    `monitoring.jadpo` (`metric x = Counter { by: [...] }`) and recorded
    with `telemetry.x.add(...)`; `telemetry.note(...)` adds an attribute to
    the current trace; personal fields are refused as labels and
    attributes. Jadpo's own data (path ids, stable identities, source
    locations, build id) goes to every provider as `jadpo.` attributes,
    decoded against the build manifest a deploy uploads. In development
    nothing is exported unless `jadpo dev --export-telemetry`, and the data
    is read with `jadpo traces`, `metrics` and `logs` (area 17).
11. **Is email built in as `type X = EmailMessage`, emitted after commit and sent
    over SMTP?** *Recommendation:* yes, templates from `translations/`, SMTP
    only, provider APIs as integrations. This replaces the golden app's
    `ReminderMail` integration; Rails, Django and Laravel all ship mail with
    SMTP and a test inbox.
    *Revised 2026-10-13 (capabilities, area 11 questions 19–24):* email is
    no longer SMTP only. Sending goes through the `email` capability: SMTP is
    one built-in driver and a mail package (Postmark, SES) another, chosen
    by `email: { connection: }` with no code change; bounces and complaints
    come back as standard events; provider features are connection
    settings or the package's own operations. Laravel's mailers and Rails'
    delivery methods work the same way.
   *Revised 2026-10-13 (Richard):* mails live in their domain's `emails/`
   folder: the type (`welcome_email.jadpo`), an HTML template
   (`welcome_email.html`) and a text template (`welcome_email.txt`), plus a
   shared layout. No MJML: agents write plain HTML, which is safer and more
   flexible, and the health score flags what mail clients mishandle.
   Templates use a small template language, checked by the compiler against
   the email's type: `{{ display_name }}` (escaped), `{{ t.welcome_heading }}`
   for translated text, conditions, loops, partials and the standard
   formatting functions (`{{ text.format(due_at) }}`); the syntax for
   conditions and loops is the open point below.
   Nothing else: values are prepared in the mail's fields, not computed in
   the template.
    *Template syntax (Richard asked, 2026-10-13):* recommendation, use
    Jadpo's own syntax inside `{{ }}` rather than a second mini-language:
    `{{ if (due_at != none) }} ... {{ else if (...) }} ... {{ else }} ...
    {{ end }}`, `{{ for todo in todos }} ... {{ end }}`, expressions exactly
    as in Jadpo code, and partials as typed calls,
    `{{ include button(label: t.open_app, url: app_url) }}`. Templates may
    only read fields, translations and formatting functions: no variables,
    no `attempt`, no other calls. Razor does the same with C#; one syntax
    means one thing for agents to learn and one parser and type checker. *Decided 2026-10-13:* the revisions above, with Jadpo's own syntax inside `{{ }}` for conditions, loops and typed partials.
    *Revised 2026-10-16 (Richard, design C):* the capability is a
    Jadpo-rendered `EmailMessage` sent after commit, attachments included (a
    `File` field, checked against the package's size limit), so a provider
    without attachments cannot be the `email:` connection. `EmailBounced`
    and `EmailComplained` go: bounces and complaints are the package's own
    webhook events until a built-in feature consumes them. Hosted templates,
    suppression lists and marketing sends are the package's operations,
    called from the domains the connection lists. Sends without an
    idempotency key keep the parking rule on `outcome_unknown`.
    *Revised 2026-10-16 (Richard, area 10 translations):* templates read
    translated text as `{{ translations.<domain>.<message>(field: value) }}`
    with the email's locale, not `t.`, and partials pass it the same way
    (`{{ include button(label: translations.accounts.open_app(), url: app_url) }}`).
    Templates are strict: a name that is not a field of the email's type, a
    message that does not exist, a placeholder missing or of the wrong type,
    an unknown partial or a partial called with the wrong fields, `if` on a
    non-`Bool`, `for` over a non-list and any call other than translations
    and formatting are compile errors, and an unused field is a warning.
    Only text and numbers are written out (`Text`, `PresentationText`,
    `Int`, `Decimal`, and enums with wording); anything else is an error
    naming how to format it (`text.format`, area 19, `if`,
    `for`, field by field). Values are escaped, and raw HTML goes in only as
    the safe-HTML type (input to chunk 5.4). An email's subject line is
    its member in its domain's language files (`welcome_email: "Welcome,
    {display_name}"`), its fields usable as placeholders; the body stays
    the `.html` and `.txt` templates. See "Email" above.
    *Revised 2026-10-16 (Richard, built-in emails):* every email Jadpo
    sends (the magic-link email, the privacy emails) can be changed in five
    levels, from nothing to the app's own sending; see "Built-in emails".
12. **Is search a `search:` section using PostgreSQL full-text search?**
    *Recommendation:* yes, one language per entity, with search services as
    integrations. Django's `SearchVector` and Laravel Scout's database engine
    do the same with more hand-written setup. *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* search is the `search` capability,
    PostgreSQL full-text search by default and an engine's package when
    `search: { store: }` names one, doing upsert, remove and query by words
    with equality filters. The engine returns ranked ids filtered by tenant
    and Jadpo reads the records through the policy-applied query; the index
    is kept in step through the outbox, erasure reaches it, and
    `jadpo search reindex` rebuilds it. Development and tests use PostgreSQL
    search whatever production uses.
13. **Does every request carry a path ID?** *Recommendation:* yes, two:
    a Ball–Larus `path` number that decodes against the build's graph, and
    a `path_key` hash of stable identities for comparing builds; loops
    record which inner paths ran, recursion its first entry. Sentry groups
    errors by guessing from stacks; the compiler can name the path exactly.
    Needs control flow in the compiler's graph. *Decided 2026-10-13:* yes, as recommended.
14. **Does the runtime decide how much to send per request?**
    *Recommendation:* yes: buffer each request, send the full trace on a
    crossed threshold, failure, fault or new path, otherwise metrics plus
    one sample per path per minute. This is tail sampling done in the
    process, which generic tools cannot do because they do not know what a
    new path is. *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* the rule is set in `telemetry: {
    detail: { full_traces: [...] samples: { per_path: every: } } }`, with
    this question's behaviour as the values shown on the page.
15. **Are thresholds declared in source, and are monitoring rules
    generated?** *Recommendation:* yes, one monitoring file per domain, so
    a reviewer sees the set; per-request thresholds act in the runtime,
    across-request ones become OpenSLO and Prometheus rules in
    `build/monitoring/` and in the build manifest. OpenSLO and Sloth keep
    SLOs as code beside the app, where a renamed route breaks them silently;
    here the compiler checks every threshold names something that exists. *Decided 2026-10-13:* yes, as recommended.
16. **Does loosening a threshold need a person?** *Recommendation:* yes,
    an acceptance file and a line in the approval report; tightening does
    not. Otherwise an agent fixes a noisy alert by raising its limit. *Decided 2026-10-13:* yes, as recommended.
17. **Which services hold a cache, and what may it hold?** *Decided
    2026-10-16 (Richard):* Redis-compatible services (Upstash, Valkey,
    ElastiCache) through the Redis driver, on Workers too; `Driver.memory`
    in production with a warning, and an error on Workers; Cloudflare KV is
    not a cache driver. The cache holds only cached records and queries,
    route rate limits and shared provider rate limits, never idempotency
    keys, webhook de-duplication, sessions, poll state, schedule leases or
    the outbox.
18. **How are bots kept off public endpoints?** *Decided 2026-10-16
    (Richard):* `bot_check` is a capability with one operation, filled by a
    package (Turnstile, hCaptcha, reCAPTCHA); `bot_check: { connection: }`
    in `application { }` and `bot_check: true` on a route or generated
    feature; the token in `X-Bot-Check`, checked before the action;
    `bot_check_failed` (403) from translations; fails closed unless an
    acceptance loosens it; a local driver passes everything in development
    and tests.
19. **How does an app make PDFs?** *Decided 2026-10-16 (Richard):* the
    `documents` capability renders `type X = Document { ... }` with its
    `.html` template in the domain's `documents/`, under the strict
    template rules, and the connection (Gotenberg as a container package,
    or a hosted API's package) turns it into a PDF:
    `documents.pdf(...)` returns `Bytes` and `documents.html(...)` the
    HTML; it runs in subscribers, capped by `max_concurrent:`.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Setting options | Carried from area 2: options are enum values written `Type.value` | 2026-10-12 |
| Colon rule | Carried from area 3: every member is `name: value`; sections are `name: { }` | 2026-10-12 |
| Cache outages | Carried from area 5: an unreachable cache never fails a request | 2026-10-12 |
| Stores | Carried from area 5: stores declared in `app.jadpo`; an action writes at most one store | 2026-10-12 |
| Reading the caller | Carried from area 8: `current_principal` in routes only; policy gets the caller from the runtime | 2026-10-12 |
| Third-party services | Carried from area 8: the language owns a feature rather than relying on a third-party service where it reasonably can | 2026-10-12 |
| Policy model | Carried from area 9: grants keyed by `create`, `read`, `update`, `delete` | 2026-10-12 |
| Flag declaration | Root `flags.jadpo`: `Bool` or enum, default, owner, required `expires:` | 2026-10-13 |
| Flag storage and changes | In the app's own database, named explicitly (`flags: { store: main }`); changed with `jadpo flags`; no flag service | 2026-10-13 |
| Flag targeting | Named users, named tenants, percentage by hash of flag and user id, then everyone; first match wins | 2026-10-13 |
| Flag checks | Unknown flags are errors; tests cover every value read; expired flags warn; `jadpo flags list` shows rollout; `jadpo flags remove --keep` | 2026-10-13 |
| OpenFeature | Not now; `GET /flags` returns `client: true` flags evaluated for the caller (Richard) | 2026-10-13 |
| Rate limits | The address they count is the client's, read through the proxies `client_address:` trusts (area 10, chunk 5.1c); on by default for every route with sensible limits; `rate_limit:` adjusts; turning off needs an acceptance; Redis when named, otherwise bounded memory with a warning; Redis or a platform binding required on Workers (Richard) | 2026-10-13 |
| Files | A connection providing `files` (built-in S3 driver or a preset package; revised 2026-10-16: no presets, and the disk driver on native); signed upload slots and links; local disk in development and tests | 2026-10-13 |
| Outgoing webhooks and field encryption | Both later; encryption with area 15's sensitive fields | 2026-10-13 |
| Audit trail | Opt-in `audit:` per entity, same transaction, personal fields hashed, readable with `read` on the record | 2026-10-13 |
| Tracing and metrics | OpenTelemetry generated for everything, no values in spans; settings in `config.jadpo` bound to `OTEL_*` names (revised 2026-10-16: the `telemetry:` section) | 2026-10-13 |
| Email | `emails/` folder per domain: type, HTML and text templates, shared layout; no MJML; templates use Jadpo syntax inside `{{ }}` (values, translations, `if`, `for`, typed `include`, formatting), read-only; delivered through the `email` capability (Richard) | 2026-10-13 |
| Email templates | Subject line from the email's member in its domain's language files, its fields as placeholders; translated text through `translations.<domain>.<message>(...)` with the email's locale, not `t.`; strict checks (unknown names, messages and partials, wrong placeholders or partial fields, `if` on non-`Bool`, `for` over non-lists and other calls are errors; an unused field warns); only `Text`, `PresentationText`, `Int`, `Decimal` and enums with wording are written out, anything else an error naming how to format it; values escaped, raw HTML only as the safe-HTML type (Richard, area 10) | 2026-10-16 |
| Search | `search:` section with PostgreSQL full-text search, one language per entity; search services as packages (revised 2026-10-16: an engine's package fills the `search` capability) | 2026-10-13 |
| Path IDs | Every request carries a path number and a stable path key | 2026-10-13 |
| Detail by threshold | Full traces on threshold, failure, fault or new path; otherwise metrics plus one sample per path per minute | 2026-10-13 |
| Thresholds in source | One monitoring file per domain, checked by the compiler; rules generated into `build/monitoring/` | 2026-10-13 |
| Loosening thresholds | Needs an acceptance; tightening does not | 2026-10-13 |
| Local inbox in development | `jadpo dev` catches all mail: browser view in the dev console (rendered, locale, working links, trace link) and `jadpo email list/show/links/open/clear` for agents (Richard) | 2026-10-13 |
| Mail health score | Deterministic content checks per mail in the dev inbox and `jadpo email show`, optional SpamAssassin rules, `jadpo email check-domain` for SPF/DKIM/DMARC; every score states it is an early indication, cannot predict inbox placement and must not be relied on (Richard) | 2026-10-13 |
| Email naming | Folder `emails/`, capability `email`, base type `EmailMessage` (`Email` stays the address type), commands `jadpo email ...`, app setting `email: { connection: }` (Richard) | 2026-10-13 |
| Shared email layout and display name | Optional `shared/emails/layout.html` and `layout.txt`, one per app, wrapping every email, receiving `content` (safe HTML in the HTML layout), `subject`, `locale.direction` and `app.display_name`, strict template rules; Jadpo's plain layout otherwise. `application { display_name: }` required once a built-in email is on, read as `{{ app.display_name }}` and `{app_name}` (decided for Richard) | 2026-10-16 |
| Privacy emails | Built-in `PrivacyAddressCheckEmail`, `PrivacyExportEmail`, `PrivacyErasureConfirmedEmail` and `PrivacyErasureCompletedEmail`, each with a `...Request` type for `send:`; configured in `privacy: { emails: { address_check: export_ready: erasure_confirmed: erasure_completed: } }`, each `email:` or `send:`; a feature with one email takes them directly (decided for Richard) | 2026-10-16 |
| Built-in emails | Five levels for every email Jadpo sends: nothing (Jadpo's email and wording, `en_gb` and `en_us` at first, other supported locales an error until the app words them); the words, in the root `translations/` under `jadpo: { }`, only for features switched on; branding, through the app's shared layout; the app's own `EmailMessage` (`email:`), picking from the built-in type's fields; the app's own sending (`send:`), an action taking the built-in request, run after commit; `email:` and `send:` together an error; all through the app's one `email:` connection (Richard) | 2026-10-16 |
| Capabilities | Only jobs Jadpo itself does, each covered whole by every connection that fills it (design C); here `files`, `email`, `documents`, `search`, `bot_check`, `scanning`, `publishing` and `receiving`; `sms` is not one; everything provider-specific is the package's own operations and events (Richard) | 2026-10-16 |
| Flags stay in the database | Not a capability; values held in each server's memory, refreshed every few seconds, never cached; `Driver.ofrep` waits for an app that must use a vendor (Richard) | 2026-10-16 |
| Rate limits on Workers | Cloudflare's rate-limiting binding as a store for the windows it can count, any other window a compile error naming Redis; `429` with `Retry-After` and `RateLimit` headers, worded from translations; provider rate limits shared through the same store (Richard) | 2026-10-16 |
| Caches | Redis-compatible services through the Redis driver, Workers included; `Driver.memory` in production with a warning, an error on Workers; Cloudflare KV not a cache driver; the cache holds only cached records and queries and rate limits, never idempotency keys, webhook de-duplication, sessions, poll state, schedule leases or the outbox (Richard) | 2026-10-16 |
| Files: stores | No presets: S3-compatible services are `Driver.s3` with their settings; `Driver.disk` for production on native only, the manifest recording one server; immutable files under generated sharded keys, streamed uploads synced and renamed, downloads with ranges, ETags, `attachment` and a sandbox policy, `storage_full` and a free-space floor, `jadpo files copy` (Richard) | 2026-10-16 |
| Files: contents and formats | Storage and formats separate; `store.read` returns `FileContents` for passing bytes on, which format helpers refuse; rows only through area 12's batch helpers; `json.decode` for single documents in memory; gzip only with `max_unpacked_size:`, no archives (Richard) | 2026-10-16 |
| Large uploads | Fields over 100 MB get a multipart slot, the same on S3, R2 and disk, parts resendable, completed on attach, with a generated client helper (Richard) | 2026-10-16 |
| Image variants and scanning | Variants (`variants: { thumb: 200x200 }`) made by the runtime over any store; the `scanning` capability scans every upload, a file unattachable until it passes; both built when an app needs them; media services stay packages (Richard) | 2026-10-16 |
| Email under design C | Attachments core, checked against the package's size limit; `EmailBounced` and `EmailComplained` removed, bounces being the package's own events; hosted templates, suppression lists and marketing sends are package operations; unkeyed sends keep parking on `outcome_unknown` (Richard) | 2026-10-16 |
| Documents | The `documents` capability: `type X = Document` with an `.html` template in `documents/`, strict template rules, translations, optional `shared/documents/layout.html`; `documents.pdf` returns `Bytes`, `documents.html` the HTML; run in subscribers; a placeholder PDF in tests (Richard) | 2026-10-16 |
| Search as a capability | PostgreSQL full-text search by default, an engine's package through `search: { store: }`; the engine returns ranked ids filtered by tenant and Jadpo reads the records through policy; kept in step through the outbox; erasure reaches it; `jadpo search reindex`; PostgreSQL in development and tests (Richard) | 2026-10-16 |
| Bot protection | The `bot_check` capability, one check operation; `bot_check: { connection: }` and `bot_check: true`; token in `X-Bot-Check`, never the input; `bot_check_failed` (403); fails closed unless accepted; local pass-through driver in development and tests (Richard) | 2026-10-16 |
| Telemetry | Its own `telemetry:` section (endpoint, credential header, `send:`, `detail:` with `full_traces:` and `samples: { per_path: every: }`), filled by `OTEL_*`; custom metrics in `monitoring.jadpo`; `telemetry.note`; personal fields refused; `jadpo.` attributes to every provider; nothing exported in development unless asked, read with `jadpo traces`, `metrics` and `logs` (Richard) | 2026-10-16 |
| Audit entries outward | A built-in `AuditEntryRecorded` event, emitted only when something listens, through `publishing`; the database stays the record of truth (Richard) | 2026-10-16 |
| Outgoing webhooks' network | One runtime-owned network exception with fixed guards: HTTPS only, no private addresses, no redirects; listed once in the network report (Richard) | 2026-10-16 |
| Settled for the build | `store.read` capped at the 10 MB whole-read limit, a larger `max_size` a compile error pointing at the format helpers or `batches.each`; `store.put` a keyed step in a multi-step action; variants read as `field.variants.<name>`; metrics `Counter`, `Measure` and `Gauge`, labels only enums and `Bool`; `telemetry.note(name: value)` with text, numbers, `Bool` or enums; `send:` default traces and metrics on, logs from `LogLevel.warn`; `documents: { layout: }` names the shared document layout; engine-backed `matches` queries take only equality on indexed fields (decided for Richard) | 2026-10-16 |
