# Jadpo roadmap

**Updated:** 2026-11-08. This page is the only list of outstanding work: the
delivery plan for building Jadpo from the decided language, one chunk per
session. Record language decisions on the [area pages](language/README.md)
and process rules in [AGENTS.md](../AGENTS.md), not here. A finished chunk
is deleted from this page: what it built is on the area pages and in
compiler-runtime, and its text is in git history. A dependency that is no
longer on this page is done.

## The direction

- **The language is specified** in [docs/language](language/README.md):
  twenty-one areas, all decided. Those pages are the specification and their
  examples become the tests.
- **A new compiler, built around one graph**
  ([compiler-runtime §1.1–1.2](compiler-runtime.md#11-graph-centred-compiler)).
  The first compiler (v0) was not reworked; it stays in git at the tag
  `archive/compiler-v0`.
- **Two ways to run the graph:** an interpreter inside `jadpo` for `dev`,
  `test` and `call`, and compiled Rust for deployables, native and
  WebAssembly (Cloudflare Workers), sharing one runtime crate. Every test runs
  both ways.
- **First build scope:** the graph core, both executors and what the golden
  todo uses. Everything else follows in the order below.

## How a session works

Saying **"continue the roadmap"** starts this routine; it runs unattended
and ends with one report of what was merged and anything that needs a
maintainer.

1. Take the first chunk below. Its tag (`opus · high`)
   is the model and effort it needs. Say in one line which chunk is next and
   its tag, then carry on without waiting. The building is done by a
   subagent started with exactly that model and effort, so the tag applies
   whatever the session itself runs on (setting the subagent's model and
   effort from the tag is authorised); the session plans, reviews the
   result, runs the gate and merges. Chunks marked **needs a
   maintainer** stop for one at the point named; every other chunk runs
   unattended.
2. Plan it in the session, not in a document: read its area pages, list the
   examples and errors it must make pass, and write those as failing tests
   first.
3. Build until its "done when" holds and the gate (`tools/verify.py`) passes.
   The full gate runs once, in the building subagent; when merging, the
   session runs `--profile quick` only if merging `main` brought in code,
   and leaves the full run to CI.
   Every chunk keeps the examples in `examples/` passing; each phase ends
   with an examples chunk that brings them up to date with everything the
   phase built.
4. Open a pull request from a branch named `chunk-<number>`, titled with the
   Conventional Commit for the whole chunk (the title is the commit on
   `main`), squash-merge it when CI passes, and delete it from this page in the same
   pull request; what it built is written in
   [compiler-runtime](compiler-runtime.md) and the area pages, which hold
   every fact, decision and measurement worth keeping.
   If the chunk needs a language decision that is not on the pages, stop and
   ask a maintainer with a recommendation; never special-case the compiler for an
   example.
5. Then take the next chunk the same way. Stop at the end of a phase, at a
   **needs a maintainer** point, when a language question needs one, or when
   the session's context is getting long; the next session picks up from the
   roadmap. Before starting a chunk, check that no other session is on it:
   skip it if an open pull request or a branch named `chunk-<number>` pushed
   in the last three hours exists.

A chunk is sized for one session. If one turns out larger, split it here
before starting the second half. Tags: `opus` or `sonnet`; effort `high`
or `xhigh`, never lower; the hardest chunks (graph,
interpreter, concurrency, policy, compiled targets, WebAssembly
optimisation) are `xhigh`.

## Phase 8 — Compiled deployables

Compiled native code (8.1) and WebAssembly on Cloudflare Workers (8.2),
every test passing interpreted and compiled.

- [ ] **8.2o What a deploy couldn't show** **needs a maintainer**, after 0.3.0
  (2026-11-04): two locations counting apart, from a second network;
  Hyperdrive's default TLS checking the database's certificate, which takes an
  origin with a bad one; the module's 8 MiB stack within an isolate's memory,
  from Cloudflare's logs; a message sent to the dead-letter queue parked
  (`jadpo deliveries`); why two deployables with one `namespace_id` didn't
  share their counts; a Hyperdrive configuration's caching (area 5,
  "PostgreSQL through Hyperdrive"); and the rate-limit binding's windows, the
  Wrangler entries and what its documentation says of locations and accuracy
  (area 16, "What the binding promises").

## Phase 9 — Performance

Developers won't touch Jadpo if it's slower.

- [ ] **9.1 Benchmarks:** `opus · high` the golden todo plus focused workloads (reads,
  writes, signed-in lists, large rows, cold start) on native, Wasm under
  workerd and on Workers, against an equivalent Bun and TypeScript app and
  Node; results published with the method. Full scope remains outstanding
  through 9.1a, 9.1d, 9.1b and 9.1c, in that order; the split changes sizing,
  not acceptance (2026-10-22). This is an umbrella: sessions take the first
  concrete subchunk below; delete 9.1 once all four are complete.
- [ ] **9.1c Workers benchmark results:** `opus · high` **needs a maintainer**
  after 9.1b: approve access and deployment before using the Cloudflare account. Run the
  golden todo plus focused workloads on Workers against the equivalent Bun and
  TypeScript app and Node; results published with the method. Include the
  10,000-delivery backlog and the runtime improvements' before/after results.
  Real edge measurement is required before Phase 9 finishes.

## Documentation and releases

What the jadpo.dev site, built in its own repository, needs from this one
([documentation](documentation.md)). Claude builds these beside Phase 9,
D1 first: the site starts once D1 has merged.

- [ ] **D4 Issues triaged by an agent** `opus · high` after D3: issues
  and feature requests read, answered and turned into pull requests by an
  agent that holds no secrets and cannot merge or release, since issue text
  is anyone's; its pull requests pass the same gate and review.
- [ ] **D8b Rename in the editor** `opus · high`: area 14, "Editor support". Waits: `jadpo rename` and the identity registry (after the first build, items 6 and 7).
- [ ] **D13c The rest of the error pages** `opus · high`: every other page
  in `docs/errors/` to the format, each rule's catalogue summary its
  page's first sentence; `error_format_scope` then covers every page.
  Split by size (2026-11-08) into the parts below, taken in order,
  each widening `error_format_scope` to the rules it finished; delete it
  once all are.
- [ ] **D13c7 `semantic.*`** `opus · high`: 35 pages.
- [ ] **D13c8 `route.*` and `api.*`** `opus · high`: 33 pages.
- [ ] **D13c9 `failure.*`, `decline.*` and `effect.*`** `opus · high`: 31 pages.
- [ ] **D13c10 `query.*`, `record.*` and `visibility.*`** `opus · high`: 28 pages.
- [ ] **D13c11 `policy.*`, `rate_limits.*`, `connection.*`, `constant.*`, `naming.*` and `import.*`** `opus · high`: 30 pages.
- [ ] **D13c12 `configuration.*`, `deployable.*`, `toolchain.*`, `install.*`, `publish.*` and `library.*`** `opus · high`: 34 pages.
- [ ] **D13c13 `translation.*`, `layout.*` and `email.*`** `opus · high`: 33 pages.
- [ ] **D13c14 `package.*`, `test.*`, `migration.*` and `acceptance.*`** `opus · high`: 28 pages.
- [ ] **D13c15 `step.*`, `subscriber.*`, `schedule.*`, `prompt.*`, `event.*`, `ai.*` and `receiving.*`** `opus · high`: 18 pages; then `error_format_scope` covers every rule.

## Phase 10 — Golden todo

- [ ] **10.2 How easily agents write Jadpo** `opus · high`: the same 10–20
  realistic changes to one app (the golden todo, with its TypeScript
  baseline and adversarial changes) done by fresh agent sessions in Jadpo
  and in TypeScript, several runs each, with hidden acceptance tests.
  Measures correctness, attempts and tokens to green, where each mistake
  was caught (check, test or escaped), how easily a person reviews the
  diff, and how often a tempting unsafe shortcut gets through. It also counts how
  often a person is asked to sign off per change, and splits each session's
  tokens between reading docs, reading and writing code, and command output. The agent
  gets the docs as [documentation](documentation.md) delivers them, primer
  first, since no model has seen Jadpo; the results
  also say whether task guides are needed beyond the primer. The method is designed at the start of the chunk; its
  results steer the order of the work after the first build.
  Built: the method, 18 changes and the harness, in [agent-changes](../benchmarks/agent-changes/README.md); the
  measured runs (108 sessions at `claude-opus-5-5`, about $200 estimated before any was measured, $648 at most by the
  $6 cap a run) wait for a maintainer's approval and a signed-in `claude`.

## What the 0.2.0 run found

- [ ] **A16 Run the sign-in half again** **needs a maintainer** after 0.3.0: the
  same brief as 0.1's run, sign-in, roles and reminders included, the only way
  to judge A7, A10 and A11. Until then, no public claim that sign-in is fixed.

What the Reddit thread found (Richard, 2026-11-03):

- [ ] **A19 Typed holes, to decide** `opus · high`: whether an agent can write
  a hole and have the compiler say what type belongs there and what is in
  scope. Waiting for Richard's decision on the recommendation in notes for
  area 2, "Open questions": `_` as a value, always the error `type.hole`,
  naming the type and what in scope fits; measured against the PersonName
  detour, it saves nothing there, since D15, D16 and A14 already remove it.

Not scheduled: a way to write a query the language can't express. Richard
will work through it and other solutions later; don't design or build one
until then.

## The public registry

The registry at jadpo.dev ([External services](language/11-integrations.md#the-public-registry)).
The application itself is built in the jadpo.dev repository against
[the registry protocol](language/11-integrations.md#the-registry-protocol);
these chunks are what is left on either side.

- [ ] **R10b What an app's contract run sends** `opus · high` after K4: area
  11, "What a release shows" and "The registry protocol". The app's opt-in,
  and `jadpo test --kind contract` reporting an answer a package can no longer
  decode to `POST decode-failures` with `jadpo_registry::report::shape_of`.
  Done when a contract run against a local registry sends the shape and no
  value, and the setting that opts in is on its area page.
- [ ] **R11 What the registry application has left** **needs a maintainer**
  to start the jadpo.dev session, after R10: area 14, "Suggestions" and "The
  suggestions report"; `docs/registry/tasks.md`. `POST fingerprints/query`
  served, so `jadpo suggest`'s scan stops saying `registry not reached`;
  `GET log/signed-head`, signing with a key held as data, its public key added
  to `protocol::PUBLIC_KEYS`; `POST decode-failures` kept and shown; `fixes`
  and the `pre-release` verifier image used in compatibility runs;
  REG-35 (publishers verified by a domain they prove, shown by `jadpo package
  add`); REG-36 (the last contract run on a package's page). Sign-up beyond
  the short list waits on terms and a privacy notice.
- [ ] **R12 Jadpo HQ** **needs a maintainer**: area 14, "HQ's agent" and
  "`jadpo suggest`". The service `jadpo suggest share` sends to, built against
  the protocol its local fake speaks today.

## Reviewing a change

What shows a developer why Jadpo is worth trying, so it comes ahead of the
work below (Richard, 2026-11-08).

- [ ] **U1 The review web view** **needs a maintainer**: Richard plans it and
  its design first (2026-11-08), then `opus · high`: area 17, "Reviewing a change"
  and "The web view". A static page in `build/review/` written by `jadpo
  approval`, from the report's JSON: the change at a high level in the
  report's order, the graph of the change as a flow chart, each path opened
  to see what surrounds it, what the change newly reaches, and links from
  every line to its source. No accounts and no approvals; acceptances stay
  files. Done when a change to the
  golden todo opens as this page and CI can publish it beside the pull
  request.
- [ ] **U2 What changed between an agent's revisions** `opus · high` after
  U1: area 17, "The web view". The differences between an agent's successive
  revisions of a change, added to the page.

## After the first build, in order

1. Personal data (area 15), with encrypted fields and keys held as data
   (area 19, "Encrypted fields" and "Language gaps this page depends on")
   and `crypto.verify_totp`: split by size (2026-10-31) into P1 to P12,
   taken in order.
   - [ ] **P5 Export** `opus · high` after P4c and V5: area 15, "Export"
     and "Who may erase and export"; area 13's generated test.
     `User.export(id)` as a `PersonalDataExport`: the person's record,
     every record that belongs to them and those erasure deletes with
     them, as JSON, one list per entity
     with the API's field names, credentials left out, a `File` field as
     a link, with each kind's retention and the companies that receive
     it, for a caller with read on the person's record; and the test
     generated for each person entity that after `erase`, `export`
     returns nothing. Done when export tests pass on both executors.
   - [ ] **P6 Erasure at providers** `opus · high` after P4c and V6: area
     15, "Erasure at providers"; area 11, "Erasure at providers". Each
     connection's key copied into `PersonErased` before the person's
     records go, a generated subscriber calling the package's `erasure:`
     operation after commit and retrying until it confirms, a delivery
     that gives up parked and shown, and a key that is personal data
     kept only in the outbox until it is delivered. V6 read the
     declarations and built `ErasureNotPossible`; this takes away the
     personal-data report's line saying Jadpo doesn't call a provider's
     `erasure:` yet. Done when a test
     erasing a person sees each fake provider's erasure called once,
     after the commit.
   - [ ] **P7 Marked fields in logs and access records** `opus · high`
     after P5: area 15, "Marked fields in logs", "Sensitive fields" and
     "Access records"; area 4, "Logging"; area 19's standard types that
     are personal data, treated as marked there. Every marked field
     logged as a keyed hash of its canonical text, whatever its type and
     in a failure's context too, the same on both executors; every
     export, erasure and read of a sensitive field recorded, without
     values, as a long-kept log event; and an audited entity's marked
     field kept in its history as that hash, lifting B11's
     `toolchain.not_built_yet` (area 16, "What the audit trail doesn't
     keep yet"). The hosts' `failure.rejected` event is built here, since
     no failure is logged yet (area 4, "Logging"), and with the access
     records a field of a sensitive category becomes searchable (area
     16, "Search and personal data"). Done when log and access-record
     tests show both on both executors.
   - [ ] **P8 The personal-data report** `opus · high` after V5 and P7:
     area 15, "The personal-data report", "Where data is stored", "What
     this does not cover" and "What the compiler checks" (a marked field
     nothing reads, declinable). V5 starts the report with the
     connections a marked field reaches; this adds, for each person
     entity, every marked field with its category, store and who may
     read it, the queries that say `include_erased: true`, its retention
     or "kept until erased", what erasure does to
     each record, keys waiting in the outbox, step records, sensitive
     fields, each store's location setting and what the language covers.
     Designed with the design skills first. Done when `jadpo check`
     writes it and its JSON twin, and the approval report shows its diff.
   - [ ] **P10 What keys held as data allow** `opus · high` after P9:
     area 19, "Hashing and cryptography" and "Signed tokens".
     `crypto.verify_totp` (RFC 6238's vectors, one step either side),
     HS256 in `crypto.verify_jwt` with a shared secret held as data, and
     signatures with a key per device. To settle with a maintainer
     first: which HMAC `verify_totp` uses, since the page allows no
     SHA-1, RFC 6238's default, and points two-factor codes at a package.
     Done when each is checked against its standard's known answers on
     both executors and built for WebAssembly.
   - [ ] **P11 Data requests: the routes** `opus · xhigh` after P5 and
     P7: area 15, "Data requests" ("Self-service by email", "Signed in",
     "Made by staff", "Protection" and "Changing the routes"); area 7,
     "Generated routes"; area 16, "Rate limits". `privacy:` in
     `application { }`; `POST /privacy/requests`, `/confirm` and
     `/cancel` with the address-check email; signed-in `POST
     /privacy/export` and `/privacy/erasure` behind a fresh sign-in;
     `privacy.create_request` granted in policy; each request a record
     with its one-month deadline and an event at every step; single-use
     links and rate limits. Done when route tests make each kind of
     request on both executors.
   - [ ] **P12 Data requests: exports, erasures and their emails**
     `opus · xhigh` after P6 and P11: area 15, "Exports", "Erasures",
     "Deadlines and events" and "The privacy emails"; area 16, "Built-in
     emails" and "Files". An export as a single-use download link that
     expires after 7 days, encrypted at rest and deleted with the link
     (the file downloads it needs built here unless item 2 has); an
     erasure after its cooling-off, with its cancel link and a completion
     email naming what was kept, sent once every provider confirms;
     `PrivacyRequestOverdue`; the other three emails at five levels, and
     with no mail connection a required `PrivacyExportReady` subscriber;
     and `jadpo privacy requests`, designed with the design skills first.
     Done when a request runs from asking to completion in tests on both
     executors.
2. Remaining built-in services (area 16): files, flags, audit, search
   and monitoring thresholds, with the `ThresholdChange` acceptance for a
   threshold raised or removed: split by size (2026-10-31) into B1 to
   B13, taken in order. B1 to B8 build files outwards from what a field
   holds; B9 to B13 touch other crates and pages and may run beside them.
   The disk driver in production moved here from item 5; the `search`,
   `bot_check` and `scanning` capabilities, image variants, the
   `telemetry:` section and custom metrics stay under item 5.
   - [ ] **B4 Downloads** `opus · high`: area 16, "Downloading" and "The
     disk driver" (downloads). An `Output` that picks a `File` field
     sends a signed link valid for five minutes with the file's name,
     size and type; the disk driver serves it from the app's own origin,
     single ranges and `If-Range` with an ETag from the file's hash,
     `Content-Disposition: attachment` but for the image allow-list,
     `nosniff` and `sandbox`, read in fixed windows. Done when route
     tests download a file whole and by range on both executors, and an
     expired or altered link is refused.
   - [ ] **B5 Uploads** `opus · xhigh`: area 16, "Uploading", "Compressed
     uploads" and "The disk driver" (uploads); area 2, `FileUpload` and
     `UploadSlot`; area 9, the slot's grant. `Entity.field_upload()`
     giving a 15-minute `UploadSlot` to a caller with `create` or
     `update` on the entity; the disk driver's upload route, streamed to
     a temporary file, `413` at the first byte over `max_size:`, hashed,
     synced and renamed; a `FileUpload` in an `Input`, attached by an
     ordinary create or update once the runtime has checked the file is
     there, its size and its type from its first bytes (`invalid_value`
     otherwise, B1's check for the rest); a `Content-Encoding` refused.
     Done when route tests ask for a slot, upload, attach and download on
     both executors, and each refusal is tested.
   - [ ] **B6 Large uploads in parts** `opus · high`: area 16, "Large
     uploads go in parts". A field over 100 MB gets a multipart slot of
     equal parts, each sent to `PUT /files/<store>/uploads/<slot>/parts/<n>`
     on disk and resendable, joined on attach with B5's checks; the
     client helper comes with item 7's generated bindings. Done when
     route tests upload in parts, resend one after a dropped connection
     and attach, on both executors.
   - [ ] **B7 The S3 driver** `opus · xhigh`: area 16, "Files" (S3-compatible
     services), "Downloading", "Uploading" and "Large uploads go in
     parts"; area 11, "Connecting in `app.jadpo`". `Driver.s3` with
     `host:`, `region:`, `bucket:` and `credential:`, natively and on
     Workers: put, read, join, discard and delete signed with SigV4,
     downloads as presigned `GET` links, upload slots as presigned `PUT`s
     and multipart `UploadPart`s in R2's equal parts. The registry's
     archives can then leave PostgreSQL. Done when it passes the disk
     driver's tests against a stand-in S3 server, interpreted, compiled
     and on workerd.
   - [ ] **B8 Files in tests and `jadpo files`** `opus · high`: area 13,
     "Files" (fakes); area 14, the files commands; area 16, "The disk
     driver" (commands). `fixture_file("customers.csv", store:)` from a
     domain's `fixtures/files/`; a test telling the in-memory store to
     fail a read (`unavailable`) or a write (`storage_full`); and `jadpo
     files ls`, `show`, `get`, `put` and `copy` on any store, designed
     with the design skills first. Done when tests read fixture files and
     see each failure on both executors, and `jadpo files copy` moves a
     disk store's files to S3.
   - [ ] **B14 Where else a `File` goes** `opus · high` after B4 and B5:
     area 16, "What a `File` holds", "Attachments", "`store.read(file)`"
     and "Deleting and erasing"; area 11, "Types". A `File` in an
     operation's `input:` or a webhook's `reply:`, sent as its bytes;
     several attachments from one field (`List<File>`, `Set<File>`, a
     `Map`) and a `FileContents` on an email; fields naming one store of
     files in entities of two databases. Done when each runs on both
     executors in place of its `toolchain.not_built_yet`.
   - [ ] **B15 Flags and erasure** `opus · high` after P4c: area 16, "The
     log", "Removing" and "What is built". A user's erasure reaching their
     `id` in `jadpo_flags` and `jadpo_flag_changes`, and those tables
     dropped once the last flag goes. Named tenants wait on tenancy being
     settled for flags (L3). Done when an erased person leaves no row in
     either table, on both executors.
   - [ ] **B16 What the audit trail doesn't keep yet** `opus · xhigh`
     after P7: area 16, "What the audit trail doesn't keep yet"; area 15,
     "What is built". A `File` field, a field with its own `retention:`, a
     person's record removed other than by erasure, a `delete` whose
     cascade changes an audited record, and an erasure that keeps an
     audited record. The pages don't yet say how each is kept or removed,
     so **needs a maintainer** for that decision first; P7 lifts only the
     marked-field case.
   - [ ] **B17 Rate limits in the documents** `opus · high`: area 16, "What
     is built"; area 7, "The `x-jadpo` fields". A generated route's own
     limits in OpenAPI's `x-jadpo-rate-limit` and `jadpo routes`, and the
     `RateLimit` headers on a magic-link refusal. Redis's store is Q8;
     turning a limit off is L3.
3. Debugging and review (area 17): `why` for anything but a schedule,
   `jadpo trace` and the timelines (the trace `--keep-failed` keeps, each
   step's state, an email's link to its trace), `test new --from` and
   `--from-event`, `call`, mapped `.jadpo` stacks and `jadpo incident`.
   - [ ] **E1 Path IDs, coverage and replay** `opus · xhigh` after item 3's
     trace: area 16, "`path`" and "Setting and changing them"; area 17,
     "Replaying a production request". `path` and `path_key` on requests,
     `jadpo path`, path coverage in `jadpo test`, production counts and
     query cost in review, metrics for each flag value's path, and
     `jadpo replay`. Done when a request's number draws its route on both
     executors and a replay gives the same results.
4. Tooling completeness (area 14): interactive tests, `rename` (refusing a
   name already in use, 2026-10-16), full completion.
   - [ ] **T1 `jadpo rename`** `opus · xhigh`: area 14, "Commands for
     looking into and changing code"; area 18, "Stable identity"; area 5,
     "Migrations". Renames a declaration or field everywhere, keeps its
     stored data by writing the identity registry, and refuses a name in
     use; `jadpo migrate new` stops refusing a rename. D8b follows. Done
     when renaming a stored field keeps its rows on both executors.
   - [ ] **T2 `jadpo domain`** `opus · high` after T1: area 14, "Tools for
     agents"; area 1, "Domains"; area 18, "Keys" and "Acceptances". `rename`,
     `split` and `merge`, moving files and every call, so a domain renamed
     no longer reads as one created. Designed with the design skills
     first. Done when each passes on a two-domain app with no diff from
     the moved code's behaviour.
   - [ ] **T3 `jadpo test --interactive` and `--affected`** `opus · high`:
     area 13, "In the editor" and "What is built"; area 14, "`jadpo test`".
     The keyboard view, its `enter`, `a` and `f` keys under `--watch`, and
     the tests a change reaches against the merge-base with `main`.
     Designed with the design skills first.
   - [ ] **T4 The schema and query reports** `opus · high`: area 14, "The
     `build/` folder"; area 16, "Setting and changing them". Those two
     reports, `diagnostics/`, `inventory/`, `audit/` and `compatibility/`,
     `jadpo sql --explain`, and a threshold's starting values from the query
     report. Designed with the design skills first.
   - [ ] **T5 The agent skill's last three parts** `opus · high` after A17:
     area 14, "Tools for agents". Reading the catalogues and running `jadpo
     impact` first, the parallel-workspace routine, and the rule for a fixed
     value. Generated CI also belongs here, once a page says what generates it
     (L3).
   - [ ] **T6 Small promises from the pages** `opus · high`: area 7, "What
     is built"; area 10, "Where translations live"; area 1, "What is built";
     area 14, "`jadpo migrate`". `jadpo build` writing
     `<api>.failures.<locale>.json`; `layout.promotion_required` covering
     messages; `layout.asset_unreferenced`; and `jadpo migrate new` asking for
     a missing purpose, which comes with prompts.
   - [ ] **T7 Records in the editor** `opus · high`: area 18, "Records in
     git and in your editor" and "What is built". `jadpo check` building
     the base commit, as `jadpo approval` does, to tell a careful hand edit of
     a view from a stale one; the record language's grammar; `jadpo lsp`
     serving records.
5. Packages and capabilities after the first build (decided 2026-10-16):
   services run by Jadpo (`runs:`, Docker or Podman hidden behind
   `jadpo services`, and `jadpo dev` starting them); the `documents`
   capability, with its `Document` base and `documents/` folder; OAuth grants
   per owner (`grants:`, `on_behalf_of:`, `OAuthGrant`, `jadpo dev grants add`
   and grants as test fixtures); search (an engine's `store:` and `jadpo
   search reindex`), `bot_check` and `scanning` capabilities, with image
   variants, each with its fake;
   telemetry's `telemetry:` section, custom metrics and the local
   `jadpo traces`, `metrics` and `logs`; `jadpo watch`; a service's new or
   changed container image and its `outbound:` grants, each with a
   `PackageChange` acceptance; packages from other Jadpo
   apps (`jadpo package add` from a folder or a URL) and the cross-app break
   check; choosing a provider when deploying,
   when an app other people run needs it.
   - [ ] **C1 Publishing events outward and outgoing webhooks** `opus ·
     xhigh`: area 12, "Where events travel" and "Running background work";
     area 16, "Outgoing webhooks" and "Sending audit entries elsewhere";
     area 7, "The `x-jadpo` fields". `publishing:`, its capability, and
     `x-jadpo-events`. Done when a chosen public event reaches a subscriber
     outside the app on both executors, signed and retried.
   - [ ] **K1 Importers other than OpenAPI** `opus · high`, when first
     needed: area 11, "An importer" and "Installing safely"; area 14,
     "Commands for packages". `smithy`, `postman`, `har` (a capture's
     operations) and `dts`.
   - [ ] **K2 What else a provider call can be** `opus · high`: area 11,
     "How provider calls are made", "How connections to packages are read"
     and "How webhooks are received". Protocols other than `json_http` and
     `aws_json`, credential schemes other than the four built, a setting's
     placeholder in a host (`{project_ref}`), defaults for package settings,
     and a field read by `decode:` or `encode:` on a webhook's event.
     Twilio's XML waits on an XML byte format (L2).
   - [ ] **K3 `AnyObject`** `opus · high`: area 2, "Other built-in types";
     area 11, "Types". A package input taking any of the app's object types as
     JSON, as Postmark's template variables and Stripe's metadata need.
   - [ ] **K4 Capability mappings and conformance suites** `opus · high`:
     area 11, "How a package provides a capability", "Proving a package
     works" and "Mistakes not reported yet"; area 21, "The conformance
     suite"; area 14, "Commands for packages". Mappings other than
     `receiving`'s and `email`'s; `jadpo package conform` for each capability
     but `ai`, and for fixed `hosts:`; a passing run pinned to the package's
     digest and shown on its listing; the scheduled CI run and the contract
     run.
   - [ ] **K5 Types from recordings and recipes' tests** `opus · high`:
     area 11, "Recordings", "Recipes" and "Installing safely". An operation's
     output type written from a recorded or sandbox answer, and a recipe's
     tests run in `sample/`.
   - [ ] **K6 What's left of `ai`** `opus · high` after P9: area 21,
     "Budgets" and "Streams, embeddings and the OpenAI and Anthropic
     packages". `keep_content:`, kept in encrypted fields, and an `embed:`
     with a `batch:` below 100, split across requests.
6. Product work: the developer console and MCP, acting as an OAuth
   provider, D1 (area 5, "Actions on D1", with `jadpo sql` for SQLite) and
   row-level security (opt-in, design notes for area 6). The console holds
   telemetry, the schedules' timeline and the dev inbox's page, and `jadpo
   mcp` an agent's proposed thresholds (area 16, "Setting and changing
   them").
7. A versioned application contract for frontend clients: a public contract
   derived by the compiler, client consumption declarations and generated
   TypeScript bindings (a React example first); presentation stays in
   ordinary frontend code.
8. `jadpo docs` or `jadpo mcp` serving the bundle offline
   ([documentation](documentation.md), "Not decided yet").
9. [ ] **D18 Areas by name** `opus · medium`: command and error pages and CLI strings saying "Area 14" or "(area 12)" name the area instead, as the [writing guide](writing.md)'s "Link by name, never by number" says.

10. Data and queries (areas 5, 6, 19 and 20), added by the audit of
    2026-11-08. Each keeps every test passing on both executors.
    - [ ] **Q1 Comparing standard types, money and held values in a query**
      `opus · xhigh`: area 5, "A stored value is checked when it is read"
      and "Migrating structured values"; area 6, "Operations in conditions";
      area 19, "SQL literals" and "Money in a translated message". Comparing and
      ordering by a standard type held in named columns, a standard collection,
      an amount of money or a range, and a condition on a field kept as JSON.
    - [ ] **Q2 Unique rules, indexes and one-step updates over held values**
      `opus · xhigh` after Q1: area 5, "Migrating structured values" and "Special
      kinds of index"; area 19, "Money in a translated message". A `unique:` or
      index over a structured value, a one-step update working one out, other
      index kinds than geographic and HNSW.
    - [ ] **Q3 What `jadpo migrate new` still refuses** `opus · high`: area 5,
      "A stored value is checked when it is read", "Migrations" and "Batches,
      locks and resuming". A new `pattern:` on a stored field; column type
      changes other than widening; a computed backfill for a field that allows
      less; a backfill filling a value held in several columns or reading one held so
      or as JSON; a backfill of a new required money field; and `CHECK`
      constraints holding fields' rules.
    - [ ] **Q4 A `geo.` condition in an include or an aggregate** `opus ·
      high`: area 20, "What is built"; area 6, "The aggregates".
    - [ ] **Q5 Reading a second store** `opus · xhigh`: area 6, "One store per
      action". Reads of another store outside the action's transaction, in place of
      `toolchain.not_built_yet`.
    - [ ] **Q6 A database only when something needs one** `opus · high`, with
      the first feature that can run without one: area 5, "Stores".
    - [ ] **Q7 A standard type swapped under stored data** `opus · high` after T1:
      area 19, "Reference lists ship with Jadpo" and "Names each release adds";
      area 18, "Acceptances". The data migration and acceptances for a
      `type.standard_exists` or `type.standard_clash` swap of a type already
      stored or on the wire.
    - [ ] **Q8 The Redis driver** `opus · xhigh`: area 11, "No presets, no
      dependencies"; area 16, "Where the counts are kept: `rate_limits: { store: }`" and "A
      Redis store on Workers". `Driver.redis` natively and on Workers, and
      rate limits counted there rather than in each server's memory.
    - [ ] **Q9 Caches** `opus · xhigh` after Q8: area 5, "Caches"; area 6,
      "Where a read comes from"; area 7, "Write revisions"; area 13, "What is
      built"; area 11, "Targets". An entity's `caches:`, a query's `cache:`, the
      `cache` capability, the `Jadpo-Revision` header, an in-memory cache checked
      against its target, caches in tests and erasure reaching a cache.
      Clearing only the entries a change affects isn't designed (L3).
    - [ ] **Q10 Entities and geometries in Redis** `opus · xhigh` after Q9: area 5,
      "Entities in Redis"; area 6, "Reading and writing entities in Redis"; area
      20, "What is built".
11. Background work and tests (areas 12 and 13), added by the audit of
    2026-11-08.
    - [ ] **J1 `jadpo batches`** `opus · high`: area 14, "Commands for
      services, calls, email and stored data" and "`jadpo runs unfinished`";
      area 12, "What `batches.status` shows". `list`, `show` and
      `cancel`, and `jadpo deliveries` listing batch runs. Designed with the
      design skills first.
    - [ ] **J2 Batches over more** `opus · high`: area 12, "Walking records
      or a provider's list: `batches.each`", "How a run is kept" and "How a
      file's run is kept"; area 11, "Pages of results". A provider's list operation as
      `from:`, walked by `page.next`; a page type above 1,000 items split into
      batches; `batches.status` and `batches.cancel` with runs in no store or
      several; a file's run with several databases and none written.
    - [ ] **J3 Delivery and operator gaps** `opus · high`: area 12, "Retries and
      giving up", "Watching schedules" and "Running background work". `Retry-After`
      between a delivery's attempts, names for instances, and
      `cli.signal_handler_failed`.
    - [ ] **W1 Data builders and `assert emitted`** `opus · high`: area 13,
      "Data builders" and "What is built". Builders' valid defaults, the
      named object a fixture returns with them, and `assert emitted`.
    - [ ] **W2 Generated tests** `opus · xhigh`: area 13, "In the editor" and
      "What is built". Every route refusing bad input with `{ field, rule }`, each
      declared failure's envelope and the rest of that list, generated, run with
      the project's and shown under the route they test. Area 15's after-`erase`
      test is P5's.
    - [ ] **W3 What a failed test reports** `opus · high` after item 3's trace:
      area 13, "What a failed test reports". The trace, fakes, kept database and
      clock in the report.
12. Sign-in and scopes (areas 8, 9 and 19), added by the audit of
    2026-11-08.
    - [ ] **N1 GitHub, Apple and token subjects** `opus · high`: area 8,
      "Providers", "In `jadpo dev`" and "External tokens in tests and `jadpo dev`";
      area 11, "No presets, no dependencies". `Driver.github` and
      `Driver.apple`, the first for jadpo.dev's accounts, and a principal `subject:`
      made of several columns or a structured value with `external_token`.
    - [ ] **N2 Scopes** `opus · xhigh`: area 8, "What is built"; area 9,
      "What is built"; area 19, "The token types"; area 11, "Mistakes not
      reported yet". Scope declarations, `scopes:` and policy's restrictions
      with `oauth_token` and `external_token`, `OAuthScope` narrowing, and an
      operation whose scope a grant doesn't request.
13. What waits on someone else, added by the audit of 2026-11-08. These
    can't start until what each names has happened.
    - [ ] **L1 Lists that need a licence** **needs a maintainer**: area 19,
      "What is built". The UK modulus check in `BankAccount`, on Vocalink's
      permission to ship its tables; checking `UnLocode`, `IataAirport`,
      `IcaoAirport`, `IataAirline` and `Icd10Code` against their lists, on
      licences that allow shipping them.
    - [ ] **L2 When an application needs it** `opus · high`: area 19, "`json_lines`",
      "Storing an event", "Age ratings", "Sizes" and "Barcodes, amounts and
      ratings"; area 11, "Webhooks that need a reply", "How a package provides a
      capability" and "Queues"; area 21, "A package filling `ai`"; area 16, "`jadpo
      email check-domain`" and "The score's checks". Legacy encodings; a calendar
      file's writer; more age-rating bodies, size systems and symbologies; an
      XML byte format; two kinds of mail through two connections; Kafka as a
      built-in driver; Gemini's envelope; a provider's own DNS records; the
      unsubscribe check, with bulk mail.
    - [ ] **L3 Language questions the pages leave open** **needs a maintainer**:
      each needs a decision, recorded on its area page, before it can be built.
      A map literal with entries and a rule on an entity (area 2); clearing only
      the cache entries a change affects (area 5); a one-step update that refuses when
      the stored value would break a rule (notes for area 6); `If-Match` and
      `ETag`, and a stream failing after its headers (notes for areas 4
      and 7); totals kept on an entity (notes for area 6); what generates a
      project's CI (area 14); flags for named tenants (area 16); turning a
      rate limit off, which has no syntax (area 16); realtime subscriptions
      and generated admin screens (area 16); whether a package may read the
      app's `shared/`, whether the registry marks and caches a provider's own
      package, and what `jadpo packages outdated` does (notes for area 11);
      and, from the registry's asks, personal-data labels on every field a
      package sends, a way for a `GET` to count what it serves, and
      `names.registry_part`.

## How we work

- Branch from `main` for each chunk; commit and push often; squash-merge through a
  pull request, its title the Conventional Commit for the whole change.
  Never put a worktree under `/private/tmp`.
- `tools/verify.py` output and the tests are the evidence. No evidence
  packets, per-step reviews, timing logs or plan documents.
- One independent review per phase, not per chunk, by two reviewers:
  soundness, and architecture for the phases ahead.
- Update this page when a chunk finishes or the order changes, and nothing
  else.
