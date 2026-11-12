---
title: Built-in services, design notes
area: 16
---

# Built-in services: design notes

Why [Built-in services](../16-built-in-services.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Consistency with the rest of the language

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
  scanning follow area 11's capabilities: the provider is a connection
  in `app.jadpo`, and each
  capability is a job Jadpo itself does, whole. Flags, rate limits and
  audit stay in the app's own stores and are not capabilities.
- **Search through two stores.** An engine-backed search reads ids from
  the engine and records from the entity's store, which area 6's one-store
  rule allows: only the store's read decides what the caller sees.
- **Telemetry is not a connection.** It has one protocol and no
  vendor code, so it has its own section rather than a capability with a
  single driver.
- **Size literals.** `max_size: 5mb` is a literal form following area
  10's duration spelling.
- **Retention shape.** Audit retention reuses area 15's
  `retention: { after: }`; `since:` is always the entry's own time.
- **Environment variables.** Area 10 says every setting is declared in
  `config.jadpo`; the OpenTelemetry variables are read by the runtime under
  their standard names instead, listed in the configuration report.

## Why some choices were made

- **Owning a feature.** Where the language can reasonably own a feature
  it does, rather than depending on a third-party service: the rule area
  8 applied to OAuth.
- **No flag vendor.** A `Driver.ofrep` store, evaluating flags only
  through OpenFeature's remote evaluation protocol, waits for an app that
  must use a vendor.
- **Removing a flag from source.** Uber's Piranha does this for Java and
  Swift; Jadpo can do it exactly because every read is known.
- **Per-tenant flags.** Named tenants get a fixed value once area 9
  decides tenancy for flags.
- **Multipart uploads.** A single signed `PUT` would be limited to 5 GiB,
  could not resume, and through Cloudflare's proxy is limited to 100 MB
  on the Free and Pro plans, so fields over 100 MB get a multipart slot.
- **Bounces in the capability.** Bounces and complaints return to the
  `email` capability only when a built-in feature consumes them (Jadpo
  refusing to send to an address that bounced permanently), and then
  every email connection must report them.
- **Two kinds of mail.** Transactional and bulk mail through two
  connections wait for an app that asks.
- **The dev inbox.** Phoenix's dev mailbox and Mailpit offer the browser
  half; the agent commands and the trace link are new.
- **Resend as the second mail connection.** Resend was chosen over
  Postmark, whose credential header no `CredentialScheme` sends yet.

- **Who signs a download link.** A store of files keeps the key its links
  are signed with in its own folder (`link-key`, made on first use, the app's
  user's alone), since the disk driver is one server and needs no setting.
  A key in the same place as the files gives anyone who could forge a link
  no more than they had. A link is a relative address, since the origin is
  the app's own and a `Host` header isn't to be trusted.
- **What a `Range` it won't serve is.** The pages say single ranges only.
  A list of ranges, overlapping or not, is read as no `Range` (RFC 9110
  lets a server ignore one), so the whole file is sent once and a list can't
  ask for a piece many times over. Malformed ones are treated the same; a
  well-formed range past the end is `416`.
- **The file's hash.** A `File` keeps it from the put, or from the join an
  export ends with, so a ranged read never hashes the whole file to make
  its ETag. A `File` kept before files were hashed has none, and no ETag.
- **Refusals.** An expired or altered link is `403`, `link_expired` or
  `link_invalid`, as a signed S3 link is.

The choices below marked *Kept* were made while building files and were
decided 2026-11-11, Richard accepting the recommendation to keep each.

- **What a slot holds.** An `UploadSlot` is `url`, `token` and `max_size`.
  The url is the disk driver's own address, relative, as a download link is;
  the token is what an `Input` attaches by. A token is good for the one field
  it was given for, so a client can't turn a slot for one field into a file
  on another; a mismatch is `invalid_value` rather than a fault, since only a
  client's token is wrong.
  *Kept.*
- **When a slot's clock runs.** The 15 minutes are the address's. The token
  stays good for as long as the bytes are there, a day, so a client slow to
  come back doesn't lose a file it has already sent. *Kept.*
- **One file to a slot.** A second `PUT` to a slot that has its file is
  `409`, since a file's bytes never change and an attach has already checked
  them; the disk driver links the file into place, which refuses a file
  already there. *Kept.*
- **How the address refuses.** A body whose first bytes aren't a type its
  field takes is `415` `file_type_refused`, as is a `Content-Encoding`
  (`content_encoding_refused`); a disk that would be left below its floor is
  `503` `storage_full`. *Kept; they say what happened in the
  refusal's code.*
- **What an upload says of itself.** Its name is read from `Content-Disposition`
  (`filename*=UTF-8''…`, RFC 8187), the last part of a path, with no control
  characters and no more than 255 bytes, and is kept with the file beside the
  bytes; `Content-Type` claims a type. A file's type comes from its first
  bytes: a magic number names it whatever was claimed, and text takes the one
  type its field lists, or the claimed type when it could be several.
  *Kept.*
- **A grant with no record yet.** `Entity.field_upload()` needs the `create`
  or `update` grant, but a grant that tests a record's fields has no record to
  test, so it lets in anyone signed in; the write that attaches is held to the
  grant in full. *Kept.*
- **The attach is an outside call.** Asking the store about the upload comes
  before the statement, as any outside call does: reads before it are a
  snapshot, and a write before it is `data.write_before_call`, so a slow store
  never holds a transaction. *Kept.*
- **What a slot in parts holds.** It is the same `UploadSlot`, with `url` left
  out and `part_size` and `parts_url` added; `token` and `max_size` as any
  slot. A program reads `url`, `part_size` and `parts_url` as optional.
  *Kept; one type lets an action hand back a slot without
  asking how large the field is.*
- **How long a slot in parts is good for.** Six hours, not 15 minutes, since a
  file of gigabytes on a slow connection can take hours. It stays inside the
  day an upload's bytes wait for a record, so nothing is swept while it is
  still being sent. *Kept.*
- **The size of a part.** 16 MiB, the size the page's example shows, unless
  the field is so large it would take over 10,000 parts, S3's and R2's limit
  on one upload; then the whole number of MiB that stays within it. A file
  smaller than one part still goes as one part, the slot having no idea of
  the file's real size. *Kept.*
- **What a part's address refuses.** A part number that isn't whole digits
  from 1, with no leading zero, is `link_invalid`; a number past the last a
  field of `max_size` can have is `file_too_large`, as is a part over
  `part_size` or over what is left of `max_size`. Equal parts are checked at
  the attach, when it is known which part is last, and the first part's bytes
  tell the type at once, since they are the same bytes that will start the
  file. A `Content-Disposition` and `Content-Type` are taken from the first
  part only: a part sent later says nothing of the file. *Kept.*
- **The attach joins.** A missing part and unequal parts are `invalid_value`,
  as any upload that can't be attached is, with the parts kept so the client
  can resend and attach again; no room to join is `503` `storage_full`, as an
  address refuses a disk with no room. Joining is one outside call that copies the parts through the
  disk driver's existing blocking join, so a large file takes a little longer
  to attach; the disk driver has two copies for a moment, against its floor.
  *Kept; a streamed join can come when one is measured to
  matter.*
- **Route tests of parts.** `request { upload: part: }` sends a part, and
  `dropped: true` ends its connection before its last byte, so a route test
  can resend after a real cut body on both executors. *Kept.*
- **How long an upload may take.** The native host gives each piece 30
  seconds and the whole upload an hour, so a client that stops or drips bytes
  can't keep a connection for good. *Kept; a minimum rate
  would be a setting nothing needs yet.*

- **The S3 driver: where a whole upload waits.** A presigned `PUT` goes to
  `uploads/<key>` and the attach copies it to the file's key with
  `x-amz-copy-source-if-match`, then deletes the staging object, so a slot's
  address, still valid for up to 15 minutes, can never change a file already
  attached, with no client cooperation and no conditional-write support
  needed from the service (R2, MinIO and S3 have `If-None-Match`, but it has
  to be sent by the client, and not every S3-compatible service honours it).
  *Kept.*
- **The S3 driver: a part's name and type, and its multipart upload.** A
  `PUT .../parts/<n>` to the app's own origin answers `307` with a presigned
  `UploadPart`, keeping the address the page gives every store. A signed
  `UploadPart` can't carry a file's name or claimed type, so the driver reads
  them from the first part's own `Content-Disposition` and `Content-Type` at
  that redirect and writes them to `uploads/<key>.said` beside the file, which
  the attach reads back. `UploadPart` also needs a multipart upload whose id
  exists before any part can be signed, and parts come in any order, so the
  first request for any part creates it and writes the id to
  `uploads/<key>.upload` with `If-None-Match: *`; a second racing it aborts
  its own and uses the winner's. (A slot's token can't carry the id, since
  making a slot asks nothing of the store.) A client following the redirect
  resends the part to the service, so its body is sent twice only when a
  client doesn't use `Expect: 100-continue` or a helper that asks for the
  address first; the generated client helper can do the latter when it
  comes. *Kept; a part's address stays the same across
  stores.*
- **The S3 driver: what the address can't refuse.** A presigned `PUT` can't
  count or read a body, so over-limit bodies, wrong types and
  `Content-Encoding` are refused at the attach (`422` `invalid_value`), not at
  the address, and aren't copied to the file's key. A part's bytes also go
  straight to the store unchecked: `parts_fit`'s equal-parts rule and the
  field's limit are checked when `ListParts` is read at the attach, as the
  page's "unequal parts" rule says. *Kept; POST policies that
  could limit the size aren't supported on R2.*
- **The S3 driver: a compressed upload.** Refused at the attach, from the
  `Content-Encoding` the service stored with the object: any value but none or
  `identity` is `422` `invalid_value`, and the upload is never copied to the
  file's key, so the sweep deletes it. Signing can't do this instead: a
  presigned `PUT` can require a header's value but not its absence, and a
  client that adds an unsigned `Content-Encoding` still gets it stored. The
  driver reads the header before any of the body, because a Worker's `fetch()`
  can try to unpack a body labelled gzip as it is read. A part's `UploadPart` can't set
  one: an object's encoding is set when its multipart upload is created, which
  the driver does without one. *Kept.*
- **The S3 driver: the hash of a file a client uploads.** None: nothing reads
  an upload's bytes on the way in, and the service serves its own `ETag`. A
  `File` already keeps a hash optional for files kept before they were
  hashed. *Kept.*
- **The S3 driver: recording an upload nobody attaches.** An upload that goes
  straight to the store tells the app nothing, so the file's row in
  `jadpo_files` (held by none) is written when the slot is made, the way a
  disk upload writes it as its bytes land, and the sweep deletes the staging
  object, what was said of it and any multipart upload after a day.
  *Kept.*
- **The S3 driver: what signs a store's slots.** A key derived from the
  connection's secret access key and bucket name with BLAKE3, so the store
  keeps nothing of its own; rotating the credential ends the slots made under
  the old one. *Kept.*
- **The S3 driver: a credential is two settings.** `credential:` has
  `key_id:` and `secret:`, each `config.<name>`, the secret a `secret: true`
  setting, by the example area 11 already gave; every S3-compatible service
  needs both. `host:`, `region:` and `bucket:` are required, so `jadpo check`
  has nothing to warn of for a recognised host that is missing a setting.
  *Kept.*
- **The S3 driver: `jadpo dev` and `jadpo test`.** As for any store of files,
  `jadpo dev` keeps a `Driver.s3` connection's files on the local disk and
  `jadpo test` in memory: nothing reaches a bucket without a deployable.
  *Kept.*
- **The S3 driver: joining an export's pieces.** Each piece is read in order
  and re-cut into 16 MiB parts, since R2 needs every part but the last the
  same size and pieces are a page's rows. A file that fits one part is
  written with one `PutObject`. *Kept.*

## Carried to later areas

| Item | Area | Status |
|---|---|---|
| `flags.jadpo` in the fixed root list; `EmailMessage` types in `emails/` | 1 | Feature flags; Email |
| `5mb` size literals; `File`, `FileUpload`, `UploadSlot`, `SearchText` built-in types | 2 | Files; Search |
| File stores as `files` connections; `search:` replacing the full-text index `kind:` | 5, 11 | Files; Search |
| `matches` in queries; relevance order in keyset pages | 6 | Search |
| `rate_limit:` on APIs and routes; `GET /flags`; rate limits in OpenAPI | 7 | Feature flags; Rate limits |
| Default rate limits on generated authentication routes | 8 | Rate limits |
| Per-tenant flag targeting depends on tenancy; slot and history grants | 9 | Feature flags; Files; Audit trail |
| Translations format for mail (domain language files read as `translations.<domain>.<message>`); OpenTelemetry variables | 10 | Email; Telemetry |
| The safe-HTML type that templates write out unescaped (`Html`); the money formatter | 19 | Email |
| Mail delivery and retries as an event; trace context through the outbox | 12 | Email; Telemetry |
| `flags:` in tests and the both-values check; `email.sent`; an in-memory file store in tests | 13 | Feature flags; Files; Email |
| `jadpo flags` commands and source removal; the `syntax.expected_declaration` message | 14 | Feature flags; Status |
| File erasure and export; audit entries on erasure; field encryption | 15 | Files; Audit trail |
| Where threshold declarations live: `thresholds { }` in `monitoring.jadpo` | 1 | Monitoring thresholds |
| Loop and recursion rule for path numbering | 3 | Path IDs |
| Stable identity for branches as well as declarations | 5 | Path IDs |
| Loosened thresholds need an acceptance: `ThresholdChange.raised`, `removed` and `lowered` | 18 | Monitoring thresholds |
| `jadpo path`; path IDs in `jadpo trace` and `incident`; agent-proposed thresholds through MCP | 17 | Path IDs; Monitoring thresholds |
| `FileContents`, `Document` and the batch types; `store.read`; `5%` in `free_space_floor:` | 2 | Files; Documents |
| The batch helpers (`csv.read`, `csv.write` ...), the outbox feeding search engines, `receiving` and `publishing` | 12 | Files; Search; Outgoing webhooks |
| Faked capabilities: `bot_check_failed`, the documents placeholder PDF, PostgreSQL search in tests | 13 | Search; Bot protection; Documents |
| Search engines and erasure; `AuditEntryRecorded` copies of hashed fields | 15 | Audit trail; Search |
| `jadpo traces`, `metrics` and `logs`; `jadpo dev --export-telemetry` | 17 | Telemetry |
| `metric` declarations in `monitoring.jadpo` | 1 | Telemetry |
| The `CsvFormat`, `json` and `json_lines` namespaces and their encodings | 19 | Files; area 19, "Data formats" |

## Decisions

| Topic | Decision |
|---|---|
| Setting options | Carried from area 2: options are enum values written `Type.value` |
| Colon rule | Carried from area 3: every member is `name: value`; sections are `name: { }` |
| Cache outages | Carried from area 5: an unreachable cache never fails a request |
| Stores | Carried from area 5: stores declared in `app.jadpo`; an action writes at most one store |
| Reading the caller | Carried from area 8: `current_principal` in routes only; policy gets the caller from the runtime |
| Third-party services | Carried from area 8: the language owns a feature rather than relying on a third-party service where it reasonably can |
| Policy model | Carried from area 9: grants keyed by `create`, `read`, `update`, `delete` |
| Flag declaration | Root `flags.jadpo`: `Bool` or enum, default, owner, required `expires:`; one file is the list a reviewer reads, so no flag is lost track of |
| Flag storage and changes | In the app's own database, named explicitly (`flags: { store: main }`; without it, using a flag is a compile error, area 5's rule for every system feature); changed with `jadpo flags`; no flag service; a web screen can come later on the same operations |
| Flags stay in the database | Not a capability; values held in each server's memory, refreshed every few seconds, never cached; `Driver.ofrep` (OpenFeature's remote evaluation, evaluation only) waits for an app that must use a vendor |
| Flag targeting | Named users, named tenants (once area 9 decides tenancy), a percentage by a hash of flag name and user id, then everyone; first match wins; no attribute rules ("country is FR") until an app needs them |
| Flag checks | Unknown flags are errors; tests cover every value read; a flag past its `expires:` is an error, not declinable, judged at today's date by `check` and at the commit's by `jadpo build`; "fully rolled out" comes from `jadpo flags list`, since the compiler can't see production; `jadpo flags remove --keep` rewrites the source |
| OpenFeature | Jadpo evaluates its own flags, so it needs no OpenFeature provider; `GET /flags` returns every `client: true` flag evaluated for the caller (their user, tenant and rollout bucket), and a flag without `client: true` never leaves the server; that route adopts OpenFeature's remote evaluation protocol (OFREP) once it is stable |
| Rate limits | `rate_limit: { requests: within: by: }` on an API or route, keyed by caller or address; the address is the client's, read through the proxies `client_address:` trusts (area 10); on by default for every route: per signed-in caller 600 a minute, per address 120 a minute on public routes, 10 a minute per address on the generated sign-in and privacy routes; `rate_limit:` loosens or tightens them per API or route, and turning one off needs an acceptance; counted in Redis when a cache connection is named (`rate_limits: { store: cache }`), otherwise in each instance's memory within a fixed allowance (16 MB, oldest counters dropped first); on Workers a Redis connection or Cloudflare's rate-limiting binding is required, and counting in memory there is the declinable `rate_limits.counted_in_memory` |
| Rate limits on Workers | Cloudflare's rate-limiting binding as a store for the windows it can count, any other window a compile error naming Redis; `429` with `Retry-After` and `RateLimit` headers, worded from translations; provider rate limits shared through the same store |
| Rate limits on Workers: Redis, timeouts and logging | A Workers deployable serving an API with a Redis store is `rate_limits.counted_in_memory`, declinable, until Redis's driver comes; the binding not waited for past 500 ms; each binding's fallback to memory logged once an isolate as `rate_limits.fallback` |
| A route's or an API's own limit (chunk G4) | A written limit stands in for the defaults on its route, or on its API's routes that write none, so it loosens as well as tightens. A route's has a count of its own; an API's is one count those routes share, as the defaults are shared. A refused credential still counts against the address's default, so a generous limit can't widen guessing. Generated routes keep their own. `within:` is whole seconds from `1s` to `24h`: `Retry-After` and `w=` say seconds, and longer is a plan's limit. A window the binding can't count is `rate_limits.window_unsupported`, never another window. `by:` gains no value counting everyone together; a limit isn't a budget, and the page says so |
| An IPv6 client's count | The default limits count an IPv6 address by its /64 (`2001:db8:1:2::/64`) and an IPv4 address by itself, on every route they guard, the magic-link and passkey routes among them, and an AI budget counted by address does the same; the request line and the timeline still show the whole address. A /56 or /48 a network hands one subscriber is still more than one count |
| Caches | Redis-compatible services through the Redis driver, Workers included; `Driver.memory` in production with a warning, an error on Workers; Cloudflare KV not a cache driver; the cache holds only cached records and queries and rate limits, never idempotency keys, webhook de-duplication, sessions, poll state, schedule leases or the outbox |
| Files | A `File` field whose bytes are in a connection providing `files`; direct uploads through a signed slot and signed links in outputs, so policy, erasure and retention reach the bytes; local disk in development and tests; `store.put(bytes, type)` for files the application makes |
| Files: stores | No presets: S3-compatible services are `Driver.s3` with their settings; `Driver.disk` for production on native only, the manifest recording one server; immutable files under generated sharded keys, streamed uploads synced and renamed, downloads with ranges, ETags, `attachment` and a sandbox policy, `storage_full` and a free-space floor, `jadpo files copy` |
| Files: contents and formats | Storage and formats separate; `store.read` returns `FileContents` for passing bytes on, which format helpers refuse; rows only through area 12's batch helpers; `json.read` for single documents in memory; gzip only with `max_unpacked_size:`, no archives |
| Large uploads | Fields over 100 MB get a slot in parts, the same on S3, R2 and disk, parts resendable, completed on attach, with a generated client helper; built on disk, the client helper waiting for generated bindings |
| Image variants and scanning | Variants (`variants: { thumb: 200x200 }`) made by the runtime over any store; the `scanning` capability scans every upload, a file unattachable until it passes; both built when an app needs them; media services stay packages |
| Outgoing webhooks and field encryption | Both later: outgoing webhooks on `publishing`, typed from the event; encryption with area 15's sensitive fields |
| Outgoing webhooks' network | One runtime-owned network exception with fixed guards: HTTPS only, no private addresses, no redirects; listed once in the network report |
| Addresses customers register (decided 2026-11-11, Richard accepting the recommendation from chunk C1) | The runtime generates a Standard Webhooks secret for each address a customer registers and shows it once, when the address is registered. Each secret is stored sealed with the application's `field_encryption_key:`, since signing needs it back and a verifier, as a hashed key would be, can't sign. A customer registers an address through operations generated on the customer's entity, the way `Service.issue_key` works, under policy. The network exception is built on the runtime's existing `Outward` client: HTTPS only, public addresses checked on the resolved address at every attempt, no redirects followed |
| Audit trail | Opt-in `audit:` per entity, same transaction, personal fields hashed, readable with `read` on the record |
| Audit entries outward | A built-in `AuditEntryRecorded` event, emitted only when something listens, through `publishing`; the database stays the record of truth |
| Audit entries outward, as built (chunk C1, 2026-11-11) | `publishing: { <connection>: { events: [AuditEntryRecorded] } }` counts as listening, so each entry is emitted for it; it is posted as `jadpo.AuditEntryRecorded`, its `data` the event's fields as the contract's JSON (`entity`, `record_id`, then the entry's, each time RFC 3339 text and each enum its value's name), not the outbox's own payload; listing it with no entity audited is `publishing.invalid` |
| Audit trail, as built (chunk B11) | `audit: { retention: { after: } }`, the period a `Duration` setting counted from each entry's own time, so it names no `since:`; `audit: {}` keeps entries until their record is erased. One `jadpo_audit` row a change in the entity's own store, added in the change's transaction after the record is read and locked (`FOR NO KEY UPDATE`, `FOR UPDATE` before a delete), numbered `seq` in its record's order (key `entity`, `record_id`, `seq`, the entity by its born name); only ever added, deleted by the purge schedule when its retention ends and by an erasure with the records it deletes. An entry is a built-in `AuditEntry`: `at`, `change` (`AuditChange.created`, `.updated`, `.deleted`; a lifecycle's `delete` transition is `.updated`), `by` (`AuditBy.user`, `.service`, `.application`, `.no_caller`) with `user_id` and `service_id`, the outermost `action`, the `request` id and the W3C `trace` id (the request behind a subscriber's change is traced by the trace its event keeps, since the outbox keeps no request id), and `fields`, each an `AuditFieldChange` of `field`, `before` and `after` as canonical text (`text.from`, JSON for a type with no single text form), since a personal field's will be a keyed hash, not a value of its type; `FieldAccess.server_only` fields left out. `Entity.history(id)` and `history(id, after:)`: 25 entries a page, newest first, under the record's lifecycle and `read` grant, each field whose own `policy:` narrows reading left out where the caller may not read it; no record, or no grant, an empty page; a call without a caller reads every entry. The purge schedule's own deletions and clearings write no entry, and a record's entries outlive it until their own retention. Refused as `toolchain.not_built_yet` until decided or built: an audited field marked `personal:` or of a standard type that is personal data (P7's keyed hash), a `File` field, a field with its own `retention:`, a person's audited record that a `delete`, its `retention:`, its lifecycle's `purge:` or another entity's cascade removes (its history would outlive erasure), a `delete` whose cascade or clearing changes an audited record (the database's change, with no entry), and an erasure that keeps an audited record. No hash chain: the page asks only that an entry commits with its change; signed log heads come with keys held as data |
| Tracing and metrics | OpenTelemetry generated for everything, no values in spans, so any monitoring tool works |
| Telemetry | Its own `telemetry:` section (endpoint, credential header, `send:`, `detail:` with `full_traces:` and `samples: { per_path: every: }`), not a connection, filled by `OTEL_*`; custom metrics in `monitoring.jadpo`; `telemetry.note`; personal fields refused; `jadpo.` attributes to every provider; nothing exported in development unless asked, read with `jadpo traces`, `metrics` and `logs` |
| Email | `emails/` folder per domain: type, HTML and text templates, shared layout; no MJML, since agents write plain HTML and the health score flags what mail clients mishandle; templates use Jadpo syntax inside `{{ }}` (values, translations, `if`, `for`, typed `include`, formatting), read-only, one syntax and one checker for agents to learn; delivered through the `email` capability |
| Email templates | Subject line from the email's member in its domain's language files, its fields as placeholders; translated text through `translations.<domain>.<message>(...)` with the email's locale; strict checks (unknown names, messages and partials, wrong placeholders or partial fields, `if` on non-`Bool`, `for` over non-lists and other calls are errors; a field nothing reads is `email.field_unused`, declinable); only `Text`, `PresentationText`, `Int`, `Decimal` and enums with wording are written out, anything else an error naming how to format it; values escaped, raw HTML only as the safe-HTML type (area 10) |
| Email naming | Folder `emails/`, capability `email`, base type `EmailMessage` (`Email` stays the address type), commands `jadpo email ...`, app setting `email: { connection: }` |
| Email as a capability | Attachments core, checked against the package's size limit; no standard `EmailBounced` or `EmailComplained` events, bounces being the package's own; hosted templates, suppression lists and marketing sends are package operations; unkeyed sends keep parking on `outcome_unknown` |
| Shared email layout and display name | Optional `shared/emails/layout.html` and `layout.txt`, one per app, wrapping every email, receiving `content` (safe HTML in the HTML layout), `subject`, `locale.direction` and `app.display_name`, strict template rules; Jadpo's plain layout otherwise. `application { display_name: }` required once a built-in email is on, read as `{{ app.display_name }}` and `{app_name}` |
| Built-in emails | Five levels for every email Jadpo sends: nothing (Jadpo's email and wording, `en_gb` and `en_us` at first, other supported locales an error until the app words them); the words, in the root `translations/` under `jadpo: { }`, only for features switched on; branding, through the app's shared layout; the app's own `EmailMessage` (`email:`), picking from the built-in type's fields; the app's own sending (`send:`), an action taking the built-in request, run after commit; `email:` and `send:` together an error; all through the app's one `email:` connection |
| Privacy emails | Built-in `PrivacyAddressCheckEmail`, `PrivacyExportEmail`, `PrivacyErasureConfirmedEmail` and `PrivacyErasureCompletedEmail`, each with a `...Request` type for `send:`; configured in `privacy: { emails: { address_check: export_ready: erasure_confirmed: erasure_completed: } }`, each `email:` or `send:`; a feature with one email takes them directly |
| Email partials | `type Name = EmailPartial { fields }` in `shared/emails/` or a domain's `emails/`, with its `.html` and `.txt` beside it, called by field name |
| Partials: locale, names and reach | A partial's `locale` goes with it from the template that includes it; included by its name in snake case; a domain's partials only its own emails', shared ones everyone's; `.html` from HTML, `.txt` from text; nesting allowed, cycles refused (`email.partial_invalid`); no email in `shared/emails/` |
| `email.sent` | Only in tests (`email.sent_context`): a `List<SentEmail>` of `to`, `from`, `subject`, `html`, `text`, `links` and attachment names; mail through a mail package caught the same way |
| Mail package mapping | `provide email { send: { call: input: receipt: failures: } limits: { attachments: } }`, input keys the input's field names; every part of the message and the attachments given; unmapped failures stay the package's; `File` fields need `max_size:` through a package, their sum within the limit (`email.attachment_limit`); Resend the second connection |
| Local inbox in development | `jadpo dev` catches all mail: browser view in the dev console (rendered, locale, working links, trace link) and `jadpo email list/show/links/open/clear` for agents |
| Dev inbox | The test's catching driver for `email:`'s connection in `jadpo dev`, each mail kept whole in `build/dev/inbox/` (last 200, ids never reused), codes and links only there; a read-only page on `127.0.0.1` at the app's port plus one, answering `localhost` only, the mail's HTML sandboxed; `jadpo email list/show/links/open/clear`; the trace id shown until `jadpo trace` links it |
| `jadpo email open` with no `BROWSER` | The system's opener is handed the link as one argument that no shell reads: `open` on macOS, `xdg-open` elsewhere, and on Windows `rundll32 url.dll,FileProtocolHandler <link>`, never `cmd /C start`, under which a `&` in a link would run a command |
| Mail health score | Deterministic content checks per mail in the dev inbox and `jadpo email show`, optional SpamAssassin rules, `jadpo email check-domain` for SPF/DKIM/DMARC; every score states it is an early indication, cannot predict inbox placement and must not be relied on |
| Health score | Seven checks with fixes and "N of 7 checks pass", the inbox-placement caveat on every score; `email.health_problem` on templates where their own text answers a check, declinable on the type; `check-domain` reads SPF, DKIM with `--selector` and DMARC with generic rules, provider-specific needs waiting for packages to declare them; the unsubscribe check waits for bulk mail |
| SpamAssassin's rules | `spamassassin -L -t -x` with `bayes_auto_learn 0` on the `PATH`, never `spamc`, run on each mail as the SMTP driver writes it (a `Message-ID:` added) as `jadpo dev` catches it, its report kept with the mail; spam at its threshold one more problem, its rules listed after Jadpo's checks; not installed, not run or no report said as such, never as clean |
| Capabilities | Only jobs Jadpo itself does, each covered whole by every connection that fills it; here `files`, `email`, `documents`, `search`, `bot_check`, `scanning`, `publishing` and `receiving`; `sms` is not one; everything provider-specific is the package's own operations and events |
| Documents | The `documents` capability: `type X = Document` with an `.html` template in `documents/`, strict template rules, translations, optional `shared/documents/layout.html`; `documents.pdf` returns `Bytes`, `documents.html` the HTML; run in subscribers, capped by `max_concurrent:`; a placeholder PDF in tests |
| Search | `search:` section with PostgreSQL full-text search, one language per entity, by default |
| Search on PostgreSQL, as built (chunk B12) | `fields:` are text fields, weighted `A`, `B`, `C`, then `D` for the fourth and after; `language:` is required, one of PostgreSQL 16's 29 text search configurations as `SearchLanguage`, `simple` among them; the words are a stored generated `tsvector`, `jadpo_search`, with a GIN index, made again from the fields when they change and dropped with `search:`, no decision needed; `SearchText` is 1 to 200 characters with no control characters, read by `websearch_to_tsquery` with `or` made a word, and a phrase of only stop words finds nothing; `matches` once a query, on a `where:` line alone or under `and`, never under `not` or `or`, in `where_present:`, an include, an aggregate or beside a nearest order; with no `order:` the best matches by `ts_rank` (no length normalisation), then `id`, a page's cursor carrying the rank; each search field must be readable by every caller (`policy.output_field_unproved`); a field of a sensitive category is `toolchain.not_built_yet` until access records record its reads; any other personal field may be searched, its words living in its own row |
| Search as a capability | An engine's package through `search: { store: }`, doing upsert, remove and query by words with equality filters; the engine returns ranked ids filtered by tenant and Jadpo reads the records through policy; kept in step through the outbox; erasure reaches it; `jadpo search reindex`; PostgreSQL in development and tests |
| Bot protection | The `bot_check` capability, one check operation; `bot_check: { connection: }` and `bot_check: true`; token in `X-Bot-Check`, never the input; `bot_check_failed` (403); fails closed unless accepted; local pass-through driver in development and tests |
| Path IDs | Every request carries a Ball–Larus path number that decodes against the build's graph and a stable path key for comparing builds; loops record which inner paths ran, recursion its first entry; needs control flow in the compiler's graph |
| Detail by threshold | Each request buffered; full traces on threshold, failure, fault or new path; otherwise metrics plus one sample per path per minute; set in `telemetry: { detail: }`, with these as the values shown |
| Thresholds in source | One monitoring file per domain, so a reviewer sees the set, the compiler checking every threshold names something that exists; per-request thresholds act in the runtime; across-request ones become OpenSLO and Prometheus rules in `build/monitoring/` and in the build manifest |
| Loosening thresholds | Needs an acceptance and a line in the approval report, since otherwise an agent fixes a noisy alert by raising its limit; tightening does not |
| How a threshold is written (chunk B13, 2026-11-01) | One `thresholds { }` per domain, in its `monitoring.jadpo`, an unnamed singleton like `flags { }` so the set reads as one list; each threshold `name: { }` with a `///` line, one subject and its limits. The subject is `route: { method: path: }`, named as a route test names one, or `watches:` and the name of an action, query, schedule or subscriber, imported like any other, the name saying which since those names are unique across the project (`action:` and the like can't be keys, being reserved words); only the domain's own, so each domain's file is the whole of what watches it. Limits are upper bounds named for what they measure: `duration:` and `queries:` per request, `p50:`, `p90:`, `p95:`, `p99:`, `server_failures:` and `oldest_waiting:` across requests over `within:` (`1m` to `1d`, `5m` when left out). `server_failures:` counts faults and the five kinds answered with a 5xx, never a failure a client caused, so a route that answers 404s isn't alerting. Named thresholds, not ones keyed by their subject, so an acceptance names one as `<domain>.<threshold>`, and a quoted route stays only in acceptances |
| What loosening is (chunk B13) | Compared by name with the before side's monitoring files, read from git: a limit raised or a longer `within:` is `ThresholdChange.raised` on `<domain>.<threshold>.<limit>`; a limit gone is `ThresholdChange.removed` on it; a threshold gone, or watching something else, `ThresholdChange.removed` on `<domain>.<threshold>`; a disk store's `free_space_floor:` lowered, `ThresholdChange.lowered` on `<connection>.free_space_floor`, a store gone taking its floor with it. A new threshold, a lower limit and a higher floor need nothing |
| Rules generated (chunk B13) | `build/monitoring/prometheus.yaml`, an alert per across-request limit and floor, and `build/monitoring/openslo.yaml`, an `Occurrences` SLO per percentile and `server_failures:` over a rolling `within:`, a percentile being the share done within its duration; conditions (`oldest_waiting:`, floors) in Prometheus only. They read OpenTelemetry's `http.server.request.duration` for routes and Jadpo's `jadpo.run.duration`, `jadpo.delivery.oldest_waiting` and `jadpo.files.free`, which the `telemetry:` section must send, each duration histogram with a bucket at every threshold's duration |
| Starting values from the query report (chunk T4; decided 2026-11-11, Richard accepting the recommendation) | The query report gives each route, action, schedule and subscriber the most queries one request or run of it sends, and that number is where its `queries:` starts: shown as "start at `queries: N`" where no threshold watching it sets one, and in the `.json` twin as `startingQueries`; where one does, the report names it and says when it is below what the code can send. Nothing is written into `monitoring.jadpo` for you and no limit is implied: a threshold stays what its file says. Starting values from production wait for telemetry |
| Settled for the build | `store.read` capped at the 10 MB whole-read limit, a larger `max_size` a compile error pointing at the format helpers or `batches.each`; `store.put` a keyed step in a multi-step action; variants read as `field.variants.<name>`; metrics `Counter`, `Measure` and `Gauge`, labels only enums and `Bool`; `telemetry.note(name: value)` with text, numbers, `Bool` or enums; `send:` default traces and metrics on, logs from `LogLevel.warn`; `documents: { layout: }` names the shared document layout; engine-backed `matches` queries take only equality on indexed fields |
| A file that doesn't fit its field | Every create, update and patch that sets a `File` field checks the file's store, type and size against the field's `store:`, `types:` and `max_size:` before its statement runs, store first, then type, then size; a mismatch is the defect `file_refused`, with nothing the action wrote kept. A fault, not a failure, since only the runtime makes a `File`, from what the action gave `put`, as a whole read past 10 MB of a `File` whose field isn't known is one; a client's upload breaking the same settings is `invalid_value` when uploads come (chunk B1) |
| A `File` at a provider, until sending one is built (chunk A25, 2026-11-05) | Refused in an operation's `input:` and a webhook's `reply:`, `toolchain.not_built_yet`, since both were written as the `File`'s record: a provider received the store and key, and a signed webhook was answered with them. Refused in an operation's `output:` and `wire_output:`, a webhook's event and an event a queue lists, `type.batch_input`, since a provider or a sender could name any file. All use `held_file`, as a route's output does, and a prompt's field uses it too, because a placeholder holding an `Object` is written as its JSON. Anything that sends values out when it is built, `publishing:`, outgoing webhooks and `documents:`, needs the same check |
| A `File`'s parts (chunk A26, 2026-11-05) | Reading any part is `type.field_on_non_record`, in code and in a template, since nothing could run it: code would stop with a fault and a template would write nothing. The same goes for `FileContents`. Other built-in values read through the same unchecked path; only these two are refused |
| An attachment's name (chunk A26, 2026-11-05) | A file's own name, or else the email's field and the file's type, `invoice.pdf`: Richard's choice, so that a recipient sees nothing of the key |
| What an email attaches (chunks A26 and B14) | An email's field sends files when it is declared as a `File` or a `FileContents`, or as a list, set or map's values of them, however deep: a `List<File>` attaches each file in order, a `Set<File>` in its order, a `Map` its values in key order, and a list of lists all of them, flattened, since each is the obvious reading of "these files". A map's keys never attach, and a field naming its file through another type (`sheet: PriceSheet.sheet`) stays `toolchain.not_built_yet`, so what attaches is visible in the email's own declaration. A file on a partial stays `toolchain.not_built_yet`: a partial sends nothing. An object or an `ExportSummary` is left alone: a template reading `{{ summary.items }}` is a real use, and refusing it for the file it also holds would stop that. |
| Naming several attachments and a file's bytes (2026-11-11) | A nameless file in a field of several is `<field>-<n>.<ext>`, counted from 1 in the order the files are attached, whatever its collection holds, so two nameless CSVs in `sheets` never share a name; a file with a name of its own keeps it, as one alone does. A `FileContents` has neither name nor type: it is named from its field, and typed by `jadpo_std::files::type_of` on its first bytes, with no claim and any type allowed, so a PDF or an image gets its type and extension and text, which could be any of the text formats, goes as `application/octet-stream` under the field's name alone rather than guessing. |
| How large several attachments may be (2026-11-11) | Through a mail package, a field of several files, or a `FileContents`, gives `max_size:` as the most its files hold in all, counted towards the package's limit as a `File`'s `max_size:` is; without one it is `email.attachment_limit`, as an unsized `File` is. A per-file size and a count would need a setting no page has, and the send is still refused as `message_rejected` if the files read are larger. A `FileContents` rides in the outbox as Base64, so an email's JSON over 64 KB faults at `emit`, as any event's does. |
| Where `FileContents` goes (chunk A26, 2026-11-05) | Each was run in `jadpo dev`, unless it says otherwise. In an operation's `input:` it writes `{"contents":"YSxi"}`, its Base64 as `Bytes` is written: the documented way to pass bytes on, so it stays. In a route's `output:` it would write the same, a way out nobody decided, so it is `toolchain.not_built_yet`, as a `File` is. Nothing decodes one, so an `Input` would answer every request with a 422 and an operation's `output:` would stop with a fault on a correct answer: `type.batch_input`, as for a webhook's event and a queue's by the same reasoning (compiled, not run). In a prompt it would write the Base64 in the model's message: `prompt.placeholder_type`, as `Bytes` is. A webhook's `reply:` stays as an `input:` does (compiled, not run). An `Event`'s fields and a failure's `internal:` are not checked (compiled, not run). An `AnyObject` sends a `FileContents` and a `File` as a declared input does, its checks made where the object is given, since it is the one input a package doesn't declare ([Values and types' notes](02-values-and-types.md#decisions)) |
| A `BatchRun` in a prompt (chunk A26, 2026-11-05) | `prompt.placeholder_type`, however deep, as a `File` is: written out it is only the id a run is found by. `BatchProgress`, `RowProblem` and `ImportSummary` hold counts and names, which a model may read, so they stay |
| Deleting a file's bytes, as built (chunk B3) | The database whose entities' `File` fields name a store of files keeps `jadpo_files`, each file's count of holders, kept by a trigger on each table with a `File` column, so cascades and purges count as deletes do; a file copied into a second record is held twice. The end of a commit that leaves a file held by none marks it as being deleted and keeps a `jadpo.FileReleased` event, whose generated subscriber `files <store>` sends the capability's keyed `delete` and records the bytes gone; a file being deleted or gone can't be held again, the write stopping with a fault. A `put` records its file at none first, and the tick deletes bytes held by none for a day, a commit's lost delivery and a change that left no event included. Fields naming one store of files from two databases each count their own database's rows (below). The disk driver's `delete` unlinks the name under its key, a link itself and never what it names, and syncs the folder; a key naming nothing is deleted already. The capability's `delete` isn't a program's operation: a program lets go of a file by deleting the record or changing the field |
| The sign-in email in `email.sent` | `SentEmail` has `sign_in_token: SignInToken?` and `sign_in_code: SignInCode?`, the sign-in email's link token and code, `none` on every other email, which a route test posts to the generated confirmation. Only a host that catches mail is handed them beside the message, so no driver sees them |
| Dropping the flag tables once the last flag goes (2026-11-09) | A migration of its own, `jadpo migrate new <purpose> --drop jadpo_flags`, both tables together, each listed in the acceptance by its store; `jadpo check` needs nothing after the last flag goes, as for any system table no source asks for, and the erasure statement reaches the tables as long as the latest migration holds them. Decided 2026-11-11, Richard accepting the recommendation, over dropping them automatically in the same migration as the last `flags remove`: the log is "kept for good" and only a person can decide it's no longer wanted, so nothing drops it for them. |
| What a person's erasure does to a flag log row (2026-11-09) | The row stays with the person's `id` taken out of `was` and `now`, even when that leaves a `set` naming no one: the log's "kept for good" rule covers the change, and what's removed is the id. Decided 2026-11-11, Richard accepting the recommendation, over deleting a row that named only them. |
| A flag's `set_at` on an erasure (2026-11-09) | Moves on (never back) to the erasure's time, so the check-and-write an operator's `jadpo flags set` relies on tells an operator holding a stale read that the row changed, rather than quietly writing the id back. The cost: a flag's "ready to remove" clock restarts when a named user is erased, which errs on the safe side. Decided 2026-11-11, Richard accepting the recommendation. |
| What a generated route's limits look like in the documents (2026-11-09) | **Decided 2026-11-11, Richard accepting the recommendation, since the pages don't say:** `x-jadpo-rate-limit` keeps one shape, `requests`, `within` and `by`, the limit every route is counted against. A route Jadpo answers that also limits something else carries `also`, a list of the same shape, `by: "email"` since each counts one email address rather than a caller or a client address, and `counts: "wrong_codes"` on `failed_codes_per_day:`, which isn't counting requests. `jadpo routes` writes the same thing in a line, `120/1m ip, 1/1m email, 10/1h email`. `code_attempts:` isn't shown, since it ends a code rather than refusing a request. A `sign_in_codes_disabled` refusal carries the `RateLimit` headers as `rate_limited` does, since it says when to come back in the same way |
| A store of files' addresses in the documents (2026-11-09) | **Decided 2026-11-11, Richard accepting the recommendation, since the pages say a store's upload and download addresses are answered ahead of every route and don't list them as generated routes:** they are shown so a client can see their limit, the default of 120 a minute from each address. They sit in `jadpo routes` in a `files` block of their own, and in every API's OpenAPI document, since a client of any API may be handed an upload address; they stay out of the API record, which is what an API promises and these addresses belong to no API. Their authentication reads `signature`, since the token or link is what lets a request in. The alternative, a document of their own, would give a client no reason to find it |
| What a flag's tests must reach | `flag.value_untested` counts a test as reaching every flag read in a body it calls, whatever branch it takes; a branch-aware rule would refuse this page's own checkout example (Richard, 2026-11-01) |
| How a test tells a store of files to fail (2026-11-10) | `fakes: { imports: unavailable }` or `fakes: { imports: storage_full }`, one bare word for the store as a whole rather than a list of answers: a store's answers that succeed (a `File`, its bytes) can't be written in a test, and the pages speak of telling the store to fail, not of answers in order. `unavailable` answers every read and every write, as a store that can't be reached does; `storage_full` every write (a `put`, an export's join, an upload and its attach), reads going on. It holds for the rest of the test, a store told twice is `semantic.fake_invalid`, and no `test.fake_unused` is reported for one nothing reached, since it is a state, not a count of answers. `fixture_file` still keeps its file, since that is the test's own setup. Decided 2026-11-11, Richard accepting the recommendation, over scripting `imports.put: [storage_full]` as a provider's operation is scripted. |
| What `fixture_file` reads (2026-11-10) | The file at the name written, below the test's own domain's `fixtures/files/`, a folder within it allowed and no part `.`, `..` or starting with a dot; a test in the root `tests/` has none. Its `FileType` is what its name ends in, `.jpg` as `jpeg` and `.jsonl` as `json_lines`, rather than its first bytes, which can't tell CSV from TSV. The `File` is named as the file is, without its folder. Only a test calls it (`semantic.not_callable` elsewhere, a fixture and a seed included, since a seed runs against `jadpo dev`'s stores); every other mistake is `semantic.fixture_file_invalid`, found as the test is checked. `jadpo test` reads the bytes from the project's folder as the test runs, only a regular file inside the project, so both executors read the same file; a test run inside workerd can't read one, which is a fault. Decided 2026-11-11, Richard accepting the recommendation. |
| How `jadpo files` reaches a store (2026-11-10) | As the deployed app does: a disk store at its `path:`, never made just to be read, and an S3 store with the settings the environment, then `.env.local`, gives; `--dev` reaches `jadpo dev`'s copy under `.jadpo/files/<store>/`. There is no `--env`, as for `jadpo flags`. `copy` also takes a disk store's folder written in full, so an app moves off one server by changing its connection to `Driver.s3`, copying the old folder into it, deploying, and copying again to catch up; it takes no `--dev`. `jadpo files` alone lists the stores. Decided 2026-11-11, Richard accepting the recommendation, over a default of `jadpo dev`'s copy, since the commands exist mostly for production. |
| What `jadpo files copy` copies (2026-11-10) | Every file's key and every key of the erasure log, each one already there with the same size left as it is, since files never change, so a second run copies only what was kept since. Not an upload still waiting for its attach, a person's export waiting to be downloaded (they ask again) or a disk store's link key: each store signs its own links, so a link given before the move stops working, as it would within five minutes anyway. A file over 8 MB goes in 8 MB pieces joined in the target store, so the command holds no more than a piece in memory and S3 takes them as parts; 8 MB stays under the most a store's answer may hold, which an S3 join reads each piece as. The type sent to S3 is the one the file's first bytes show, `application/octet-stream` when they can't tell, since a disk store keeps none and a download's link names its own. `put` puts bytes back only under a file's key that holds nothing, and `get` never writes over a local file. A credential is never printed. Decided 2026-11-11, Richard accepting the recommendation. |
| One store of files held from two databases (2026-11-11) | Each database whose entities' `File` fields name the store keeps its own `jadpo_files` and triggers. The first, the store of the first entity by name with such a field, is where a `put` records its file at none and where each deletion is decided: the subscriber that would delete a file's bytes first marks the file as being deleted in every counting database, the first's mark held in a transaction until the rest are done so two deletions of one file are decided one at a time. A database whose records still hold the file stops it: every mark is deleted again, the delivery succeeds, and the database that lets go last decides afresh. Once all are marked the bytes go and every database records them gone. Counting across databases in one statement isn't possible, and a central count would need a second write per change in another store. While a deletion is being decided, a write elsewhere that would keep the file stops with a fault as it would once the bytes were being deleted: a narrow window, and refusing is the safe side. |
| A file held inside a structured value (2026-11-11) | **Fixed:** every `File` an entity's structured field holds, however deep (`List<File>`, `Set<File>`, a map's values, an object's or a variant's field, an `ExportSummary`), counts as a holder, as a `File` field's does, on insert, update and delete. Such a field names no store, so a database with one counts the holders of every store of files a `File` field names; a file of a store none names is still never deleted. |
