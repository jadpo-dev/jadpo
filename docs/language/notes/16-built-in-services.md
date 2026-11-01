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
| Where threshold declarations live | 1 | Monitoring thresholds |
| Loop and recursion rule for path numbering | 3 | Path IDs |
| Stable identity for branches as well as declarations | 5 | Path IDs |
| Loosened thresholds need an acceptance | 7 | Monitoring thresholds |
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
| An IPv6 client's count | The default limits count an IPv6 address by its /64 (`2001:db8:1:2::/64`) and an IPv4 address by itself, on every route they guard, the magic-link and passkey routes among them, and an AI budget counted by address does the same; the request line and the timeline still show the whole address. A /56 or /48 a network hands one subscriber is still more than one count |
| Caches | Redis-compatible services through the Redis driver, Workers included; `Driver.memory` in production with a warning, an error on Workers; Cloudflare KV not a cache driver; the cache holds only cached records and queries and rate limits, never idempotency keys, webhook de-duplication, sessions, poll state, schedule leases or the outbox |
| Files | A `File` field whose bytes are in a connection providing `files`; direct uploads through a signed slot and signed links in outputs, so policy, erasure and retention reach the bytes; local disk in development and tests; `store.put(bytes, type)` for files the application makes |
| Files: stores | No presets: S3-compatible services are `Driver.s3` with their settings; `Driver.disk` for production on native only, the manifest recording one server; immutable files under generated sharded keys, streamed uploads synced and renamed, downloads with ranges, ETags, `attachment` and a sandbox policy, `storage_full` and a free-space floor, `jadpo files copy` |
| Files: contents and formats | Storage and formats separate; `store.read` returns `FileContents` for passing bytes on, which format helpers refuse; rows only through area 12's batch helpers; `json.read` for single documents in memory; gzip only with `max_unpacked_size:`, no archives |
| Large uploads | Fields over 100 MB get a multipart slot, the same on S3, R2 and disk, parts resendable, completed on attach, with a generated client helper |
| Image variants and scanning | Variants (`variants: { thumb: 200x200 }`) made by the runtime over any store; the `scanning` capability scans every upload, a file unattachable until it passes; both built when an app needs them; media services stay packages |
| Outgoing webhooks and field encryption | Both later: outgoing webhooks on `publishing`, typed from the event; encryption with area 15's sensitive fields |
| Outgoing webhooks' network | One runtime-owned network exception with fixed guards: HTTPS only, no private addresses, no redirects; listed once in the network report |
| Audit trail | Opt-in `audit:` per entity, same transaction, personal fields hashed, readable with `read` on the record |
| Audit entries outward | A built-in `AuditEntryRecorded` event, emitted only when something listens, through `publishing`; the database stays the record of truth |
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
| Settled for the build | `store.read` capped at the 10 MB whole-read limit, a larger `max_size` a compile error pointing at the format helpers or `batches.each`; `store.put` a keyed step in a multi-step action; variants read as `field.variants.<name>`; metrics `Counter`, `Measure` and `Gauge`, labels only enums and `Bool`; `telemetry.note(name: value)` with text, numbers, `Bool` or enums; `send:` default traces and metrics on, logs from `LogLevel.warn`; `documents: { layout: }` names the shared document layout; engine-backed `matches` queries take only equality on indexed fields |
| A file that doesn't fit its field | Every create, update and patch that sets a `File` field checks the file's store, type and size against the field's `store:`, `types:` and `max_size:` before its statement runs, store first, then type, then size; a mismatch is the defect `file_refused`, with nothing the action wrote kept. A fault, not a failure, since only the runtime makes a `File`, from what the action gave `put`, as a whole read past 10 MB of a `File` whose field isn't known is one; a client's upload breaking the same settings is `invalid_value` when uploads come (chunk B1) |
| Deleting a file's bytes, as built (chunk B3) | The database whose entities' `File` fields name a store of files keeps `jadpo_files`, each file's count of holders, kept by a trigger on each table with a `File` column, so cascades and purges count as deletes do; a file copied into a second record is held twice. The end of a commit that leaves a file held by none marks it as being deleted and keeps a `jadpo.FileReleased` event, whose generated subscriber `files <store>` sends the capability's keyed `delete` and records the bytes gone; a file being deleted or gone can't be held again, the write stopping with a fault. A `put` records its file at none first, and the tick deletes bytes held by none for a day, a commit's lost delivery and a change that left no event included. Fields naming one store of files from two databases are `toolchain.not_built_yet`, since a trigger counts only its own database's rows. The disk driver's `delete` unlinks the name under its key, a link itself and never what it names, and syncs the folder; a key naming nothing is deleted already. The capability's `delete` isn't a program's operation: a program lets go of a file by deleting the record or changing the field |
| The sign-in email in `email.sent` | `SentEmail` has `sign_in_token: SignInToken?` and `sign_in_code: SignInCode?`, the sign-in email's link token and code, `none` on every other email, which a route test posts to the generated confirmation. Only a host that catches mail is handed them beside the message, so no driver sees them |
