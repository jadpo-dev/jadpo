# Jadpo roadmap

**Updated:** 2026-10-20. This page is the only list of outstanding work: the
delivery plan for building Jadpo from the decided language, one chunk per
session. Record decisions in the [decision register](decision-register.md)
and the [language pages](language/README.md), not here.

## The direction (decided 2026-10-13)

- **The language is specified** in [docs/language](language/README.md):
  twenty-one areas, all decided. Those pages are the specification and their
  examples become the tests.
- **A new compiler, built around one graph**
  ([compiler-runtime §1.1–1.2](compiler-runtime.md#11-graph-centred-compiler-decided-2026-10-13)).
  The first compiler (v0) was archived, not reworked.
- **Two ways to run the graph:** an interpreter inside `jadpo` for `dev`,
  `test` and `call`, and compiled Rust for deployables, native and
  WebAssembly (Cloudflare Workers), sharing one runtime crate. Every test runs
  both ways.
- **First build scope:** the graph core, both executors and what the golden
  todo uses. Everything else follows in the order below.

## How a session works

Saying **"continue the roadmap"** starts this routine; it runs unattended.

1. Take the first chunk below that is not ticked. Its tag (`opus · high`)
   is the model and effort it needs. Say in one line which chunk is next and
   its tag, then carry on without waiting. The building is done by a
   subagent started with exactly that model and effort, so the tag applies
   whatever the session itself runs on; the session plans, reviews the
   result, runs the gate and merges. Chunks marked **needs
   Richard** stop for him at the point named; every other chunk runs
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
   phase built (Richard, 2026-10-15).
4. Open a pull request from a branch named `chunk-<number>`, merge it when CI
   passes, and tick it here in the same pull request.
   If the chunk needs a language decision that is not on the pages, stop and
   ask Richard with a recommendation; never special-case the compiler for an
   example.
5. Then take the next chunk the same way. Stop at the end of a phase, at a
   **needs Richard** point, when a language question needs him, or when the
   session's context is getting long; the next session picks up from the
   roadmap. Before starting a chunk, check that no other session is on it:
   skip it if an open pull request or a branch named `chunk-<number>` pushed
   in the last three hours exists.

A chunk is sized for one session. If one turns out larger, split it here
before starting the second half. Tags: `opus` or `sonnet`; effort `high`
or `xhigh`, never lower (Richard, 2026-10-13); the hardest chunks (graph,
interpreter, concurrency, policy, compiled targets, WebAssembly
optimisation) are `xhigh`.

## Phase 0 — Reset

- [x] **0.1 Merge the language review**. Built: the `language-review` branch
  merged into `main` through a pull request.
- [x] **0.2 Archive the current compiler**. Built: the v0 compiler tagged
  `archive/compiler-v0` and removed from `main`, keeping the golden
  contract, the experiments' results and the diagnostics catalogue as
  reference.
- [x] **0.3 Documentation clean-up**. Built: the language pages made the
  only specification, older specifications moved to `docs/archive/`, and
  `AGENTS.md` and `docs/README.md` rewritten for this roadmap.
- [x] **0.4 Architecture plan**. Built: the architecture plan in
  [compiler-runtime.md](compiler-runtime.md): the crates, the graph and its
  identity, the query engine, the shared executor interface, the runtime's
  boundaries and the gate.

## Phase 1 — Graph core (areas 1–4)

- [x] **1.1 Workspace and CLI shell** `opus · high`. Built: the `jadpo` CLI
  shell in the agreed
  [design language](language/research/14-cli-design-language.md), with
  self-describing help, `--json` and a completion skeleton.
- [x] **1.2 Parser, syntax tree and formatter** `opus · high`. Built: the
  parser, syntax tree and formatter for the decided grammar.
- [x] **1.3 Project layout and domains** `opus · high`. Built: project
  layout and domains (area 1).
- [x] **1.4 Values and types** `opus · high`. Built: values and types (area
  2).
- [x] **1.5 Logic** `opus · high`. Built: logic (area 3).
- [x] **1.6 Failures** `opus · high`. Built: failures (area 4).
- [x] **1.7 The graph, identity and manifest** `opus · xhigh`. Built: the
  graph, its identity and `build/manifest.json` written by `jadpo check`,
  with diagnostics and hover in the language server.
- [x] **1.8 Typed bodies and one resolver** `opus · xhigh`. Built: one
  per-callable typed-body query that diagnostics, graph edges, cycles and
  the language server all derive from, with per-name lookups and a typed
  standard-library operation table shared with the runtime.
- [x] **1.9 Phase 1 review fixes** `opus · high`. Built: the Phase 1
  review's fixes, and area 1's exception letting a type fold to the name of
  its file's callable.

Done when areas 1–4's decided examples check, and each area's errors are
listed on its page with the new compiler's real output (the v0 lists are
reference only).

## Phase 2 — Interpreter and tests (area 13)

- [x] **2.1 Interpreter** `opus · xhigh`. Built: the interpreter for pure
  logic over the typed bodies, with errors at `.jadpo` lines and name errors
  from one walk.
- [x] **2.2 Test runner** `opus · high`. Built: the test runner, with file,
  line, name, seed and failure reports and `--json`.
- [x] **2.3 Watch mode** `opus · high`. Built: watch mode under the 1-second
  target.
- [x] **2.4 Teaching examples** `opus · high`. Built: commented teaching
  examples in `examples/` tested in the gate, and the coverage check that
  fails when a built feature appears in no example
  ([decision](decision-register.md#examples-teach-every-feature--2026-10-14)).
- [x] **2.5 Construction failures by rule** `opus · high`. Built:
  construction failures by rule,
  `attempt CakeCount(n) { max: OrderTooLarge }` with `invalid:` as the
  catch-all, and every example moved to it.
- [x] **2.6 Conditions as proof** `opus · xhigh`. Built: `if` conditions
  narrowing the ranges the checker knows, so a construction they make safe
  needs no `attempt`.
- [x] **2.7 Phase 2 review: soundness** `opus · xhigh`. Built: the Phase 2
  soundness fixes: unchecked format types, decimal overflow and precision,
  wrong `match`, set, `include` and `%` results, silent run-time faults, and
  refinements that loosen a rule.
- [x] **2.8 Phase 2 review: tooling and docs** `opus · high`. Built: the
  Phase 2 tooling and docs fixes: rerun commands, `next` steps, watch exits,
  one `--json` location shape, `not_built_yet` flags,
  `visibility.public_unused` and stable gate snapshots.
- [x] **2.9 Phase 2 review: teaching examples** `opus · high`. Built: the
  teaching examples reworked so types carry meaning, business rules are
  raised with `reject` and failures are handled in application code.
- [x] **2.10 Executor shape for Phase 3** `opus · xhigh`. Built: the
  executor shape for data work: pausing sites known from the typed bodies,
  actions interpreted asynchronously, the runtime owning the action
  boundary, `jadpo-plan`'s first plans, and watch classifying every edge
  kind.
- [x] **2.11 Named collection construction** `opus · high`. Built: named
  collection construction, `Names(found)`, with arms, `invalid:` and proof
  as for any construction.

## Phase 3 — Data (areas 5, 6)

- [x] **3.1 Entities, migrations and PostgreSQL** `opus · high`. Built:
  entities, migrations and PostgreSQL, with snapshots, system tables and the
  test database template.
- [x] **3.2 Queries** `opus · high`. Built: queries with filters, order,
  pages, includes and selected columns, and `jadpo sql`.
- [x] **3.3 Writes** `opus · xhigh`. Built: entity writes, conflicts, the
  concurrent-change rerun and counters.
- [x] **3.4 Lifecycle and purge** `opus · high`. Built: lifecycle and purge.
- [x] **3.5 Imports** `opus · xhigh`. Built: imports
  (`use <folder> { Names }` and `use <domain>`) with their errors,
  `jadpo fmt` adding, removing and sorting them, and go to definition, with
  every example migrated.
- [x] **3.6 VS Code extension** `sonnet · high`. Built: the VS Code
  extension in `editors/vscode`, starting `jadpo lsp`, with highlighting.
- [x] **3.7 Isolation by what an action does** `opus · xhigh`. Built:
  isolation by what an action does: read committed for an action that loads
  no record, repeatable read for every other.
- [x] **3.8 Habits from other languages** `opus · high`. Built: the
  researched catalogue of habits from other languages, each with its own
  error, edit and recovery
  ([decision](decision-register.md#errors-for-habits-from-other-languages--2026-10-15)).
- [x] **3.9 One declaration and what it uses** `opus · high`. Built:
  `layout.unused_declaration` for a supporting declaration the primary one
  doesn't use, with every example rewritten to obey it.
- [x] **3.10 Phase 3 review: soundness** `opus · high`. Built: the Phase 3
  soundness fixes: savepoints for called actions, purge through cascades,
  safe patches and text joins on stored fields, check-time `not_built_yet`
  in place of run-time faults, rules in migration snapshots and
  `COLLATE "C"` text indexes.
- [x] **3.11 Test databases that scale** `opus · high`. Built: test
  databases that scale: one PostgreSQL server per gate run through
  `JADPO_TEST_DATABASE_URL`, parallel or reused copies, and a quick profile
  needing no PostgreSQL.
- [x] **3.12 The runtime owns what both executors need** `opus · xhigh`.
  Built: failure shapes, refusals and plans moved into `jadpo-std` as plain
  data, one runtime entry point for reads and writes, and PostgreSQL text
  rendered by one leaf crate.
- [x] **3.13 Incremental data checking** `opus · xhigh`. Built: incremental
  data checking, with per-name entity and query models and diagnostics per
  file, in the scale test.

## Phase 4 — HTTP, identity and policy (areas 7–9)

- [x] **4.1 Routes** `opus · high`. Built: routes with groups, inputs,
  outputs, statuses, OpenAPI, the API record and default rate limits.
- [x] **4.2 Request guarantees** `opus · high`. Built: idempotency,
  deadlines, body limits and health.
- [x] **4.3 Sign-in** `opus · high`. Built: signing in: the principal,
  browser sessions, routes signed in by default, `current_principal` and
  signing in from route tests.
- [x] **4.3b API tokens and service keys** `opus · high`. Built: service
  keys and API tokens (`Service.issue_key`, `sessions.issue_token`, their
  lists and revocation) and service callers.
- [x] **4.4 Policy** `opus · xhigh`. Built: policy grants compiled into
  queries, tenancy, the permissions report, and revocation gated by the
  owning record's grants.
- [x] **4.4b Field restrictions** `opus · xhigh`. Built: field restrictions:
  a field's `policy:` and `FieldAccess.server_only`, the flow check behind
  `policy.output_field_unproved`, refused writes, and fields in the report,
  graph and manifest.
- [x] **4.4c Listing tokens and keys from a `GET`** `sonnet · high`. Built:
  `sessions.tokens` and `Service.keys` as generated queries on a `GET`
  route, with `IssuedToken` and `IssuedKey` allowed in an `Output`.
- [x] **4.6 Teaching examples for Phase 4** `opus · high`. Built: the
  teaching examples brought up to date with routes, identity and policy,
  passing `--require-examples`.
- [x] **4.7 Phase 4 decisions** `opus · high`. Built: tenant-owned
  references kept inside their tenant, conditions and orders on unreadable
  fields refused, and route tests sending a fresh `Idempotency-Key` on every
  write.
- [x] **4.8 Phase 4 review: policy soundness** `opus · xhigh`. Built: the
  Phase 4 policy soundness fixes: ungated actions, unchecked outputs, values
  losing where they came from, proofs for updates and memberships, fallbacks
  failing closed, and no one joining a tenant as owner.
- [x] **4.9 Phase 4 review: request soundness** `opus · high`. Built: the
  Phase 4 request soundness fixes: rate limits counted before the work,
  deadlines on work that never waits, header and body handling, OpenAPI
  responses derived from the plan, reruns at commit and the native host's
  timeouts.
- [x] **4.10 The runtime owns Phase 4's request path** `opus · xhigh`.
  Built: the runtime owning the request path: action gates in
  `jadpo_runtime::action`, one `serve_request` for `jadpo dev` and every
  deployable, rate limits behind a host capability, and one-statement
  sign-in lookups and idempotency claims.
- [x] **4.11 Incremental policy and route checking** `opus · xhigh`. Built:
  incremental policy and route checking on one positionless per-callable
  query of what a body reads, writes, calls and loads.

## Phase 5 — Configuration, time, translations and the standard library (areas 10 and 19)

- [x] **5.1 Configuration** `sonnet · high`. Built: configuration:
  `config.jadpo`, environments, secrets, signing keys, `jadpo config`, the
  check before the app starts and a test's own `config:`.
- [x] **5.1b Signed sessions** `opus · high`. Built: sessions and tokens
  signed and checked without a read within area 8's `revocation_delay:`,
  `POST /auth/exchange` for service keys, and a browser session's `origin:`
  from configuration.
- [x] **5.1c Trusted proxies** `sonnet · high`. Built: trusted proxies
  declared in `app.jadpo`, so the default rate limits count the client's
  address read from `X-Forwarded-For`, `Forwarded` or `CF-Connecting-IP`.
- [x] **5.1d Restricted values stored elsewhere** `opus · high`. Built:
  `policy.output_field_unproved` at the `set:` that stores a restricted
  value where readers who may not read it can.
- [x] **5.2 Time** `opus · high`. Built: durations, instants, zones and the
  `temporal` library, `ZonedTime`, time on the wire and in queries, and a
  test's clock.
- [x] **5.2b Time formats** `opus · high`. Built: `TimeFormat`,
  `ComponentStyle`, `PresentationText` and checked `temporal.format` and
  `format_relative`.
- [x] **5.2c A type name is not a value** `sonnet · high`. Built: an error,
  with its one-step fix, for a type's or declaration's bare name where a
  value is expected.
- [x] **5.3 Locales and the text for people** `sonnet · high`. Built:
  `locales { }`, `locale:` and `Locale` checked against it, and date and
  time formatting over ICU4X's compiled CLDR data.
- [x] **5.3b Translations** `sonnet · high`. Built: translations as ICU
  MessageFormat JSON files with typed calls, since replaced by 5.4c.
- [x] **5.3c Choosing a locale from a request** `sonnet · high`. Built:
  `locale.from_accept_language(header)`, picking a supported `Locale` and
  falling back to `default`.
- [x] **5.3d Names private to a domain** `opus · high`. Built: names private
  to a domain in the name table, resolution, imports and identity keys, with
  promotion to `shared/` writing a registry move line.
- [x] **5.3e Constants** `opus · high`. Built:
  `constant name: Type = value`, worked out and checked while compiling and
  read in bodies, queries and updates by `id`, with its errors and
  `constant.similar_exists`.
- [x] **5.3f Making a constant a setting** `opus · high`. Built:
  `jadpo config move <constant>` and the editor's "Make this a setting", and
  `jadpo fmt` sorting `config { }` fields by name.
- [x] **5.4 Standard library: the plan** `opus · high`. Built: the standard
  library plan, decided by Richard as
  [area 19](language/19-standard-library.md).
- [x] **5.4b Declinable errors** `opus · high`. Built: declinable errors in
  place of warnings: `decline <rule>: "<reason>"`, `jadpo declines list` and
  `clear`, and the editor's decline actions.
- [x] **5.4c Translations as messages** `sonnet · high`. Built: translations
  as messages: `type X = Message`, `PluralCount`, one `translations { }` per
  locale in each domain and `shared/` with their checks, and typed
  `translations.<domain>.<message>(..., locale:)` calls, replacing 5.3b's
  JSON files.
- [x] **5.4d Translations: failures, review and commands** `sonnet · high`.
  Built: failures worded in every locale and exported beside OpenAPI, stale
  translations through `sources.jadpo_record`, `TextDirection`, the
  `jadpo translations` commands and the pseudo-locale `Locale.en_xa`.
- [x] **5.4e Translations: Jadpo's text** `sonnet · high`. Built: Jadpo's
  own messages as a catalogue shipped in `en_gb` and `en_us`, the root
  `translations/` folder with its errors, and `jadpo translations builtin`.
- [x] **5.4f Translations: the exchange** `sonnet · high`. Built:
  `jadpo translations export` and `import` in XLIFF 2.0 or Chrome-style
  JSON, checked by `jadpo check`'s own rules.
- [x] **5.5a The standard type contract** `opus · high`. Built: the standard
  type contract: a stub and a data entry per type generating construction,
  text, JSON, storage, `==` and order, one harness for every type, and
  `type.standard_exists`.
- [x] **5.5a2 Standard type clashes and `jadpo fix`** `opus · high`. Built:
  each data entry's release, the clash report after an upgrade, and
  `jadpo fix` applying the swap or the rename.
- [x] **5.5a3 Editor completion** `opus · high`. Built: editor completion
  from the checker's own declarations and data, with snippets and VS Code's
  triggers.
- [x] **5.5b Lists** `opus · high`. Built: the 21 `list` operations that
  need no language change, `collection.` renamed to `list.` with its
  moved-namespace error.
- [x] **5.5c Sets, maps and ranges** `opus · high`. Built: the 7 `set`
  operations, the 8 `map` operations without inline functions and the 3
  `range` operations, `for` over a set and a map's keys, and `range`
  reserved.
- [x] **5.5c2 Two-parameter inline functions and `Range<T>`** `opus · high`.
  Built: two-parameter inline functions for standard operations
  (`list.zip_with`, `map.filter`, `map.map_values`) and `Range<T>` with open
  ends.
- [x] **5.5d Numbers** `opus · high`. Built: the 8 new `integer` and
  `decimal` operations, `scale:` and `Rounding.down`, alike on both
  executors.
- [x] **5.5e Decimal maths** `opus · high`. Built: correctly rounded `sqrt`,
  `power`, `exp`, `ln`, `sin`, `cos`, `atan2` and `pi` on `Decimal`, with
  the same digits on native and WebAssembly.
- [x] **5.5f Text** `opus · high`. Built: the 17 text operations, case and
  sorting by locale through ICU4X, NFC at the boundary, and `text` reserved.
- [x] **5.5g Hashing, encodings and JSON** `opus · high`. Built:
  `hash.sha256`, the 9 `encoding` operations, and `json.read` and
  `json.write` in memory.
- [x] **5.5g2 The decode problem on a failure** `opus · high`. Built: the
  built-in `DecodeProblem` received by a failure mapped from `json.read`'s
  `mismatch`.
- [x] **5.5h Money** `opus · high`. Built: `Money` and `Currency`, 7 `money`
  operations, the operator table's `Money` rows with `currency_mismatch`,
  and storage in two columns.
- [x] **5.5h2 Exchange rates and money in messages** `opus · high`. Built:
  `ExchangeRate`, `money.convert`, and prices in translation messages.
- [x] **5.5h3 Definition before use** `sonnet · high`. Built: `jadpo fmt`
  ordering every file definition before use with the primary declaration
  last, and every source reformatted.
- [x] **5.5i Payment and identity numbers** `opus · high`. Built: the 17
  payment, tax and identity number types, each checked by a pattern or a
  check digit.
- [x] **5.5j Quantities** `opus · high`. Built: the 17 physical quantities
  with `DataSize` and `Bitrate`, their unit enums, SI storage, suffixed
  literals, the product and quotient table and the `units` operations.
- [x] **5.5j2 Percentages, ratios and fractions** `opus · high`. Built:
  `Percentage`, `Ratio` and `Fraction`, the `%` literal, the operator
  table's `Percentage` rows and percentages in messages.
- [x] **5.5k Time and calendar** `opus · high`. Built: ten new time types,
  `CalendarPeriod`'s operators, `temporal.period_between` and `DateRange` in
  the `range` operations.
- [x] **5.5k2 Recurrence and opening hours** `opus · high`. Built:
  `RecurrenceRule` (RFC 5545) with `temporal.occurrences`, and
  `OpeningHours` with `temporal.open_at`.
- [x] **5.5k3 Calendar events** `opus · high`. Built: `CalendarEvent`, an
  object of optional parts whose text is an RFC 5545 `VEVENT`, stored a
  column per part.
- [x] **5.5k4 Local times to instants, and all-day recurrence**
  `opus · high`. Built: `temporal.resolve` turning local times into instants
  under `dst:`, and date-only `RecurrenceRule` starts for all-day events.
- [x] **5.5l People and places** `opus · high`. Built: 15 people and places
  types: the CLDR and ISO closed lists as built-in enums with retired codes,
  the name types, and `PersonName`, `EmailWithName` and `PostalAddress`
  written in each locale's order.
- [x] **5.5l2 Phone numbers and postal codes** `opus · high`. Built:
  `PhoneNumber` on libphonenumber's metadata, `CallingCode`, `PostalCode`
  per country, and `UnLocode` by pattern and country only, since whether the
  UN/LOCODE list ships **needs Richard**.
- [x] **5.5h4 Exchange rate types that fix their currencies** `opus · high`.
  Built: exchange rate types that fix their currencies, so an inverted rate
  is a compile error across function boundaries.
- [x] **5.5i2 Cards and identity documents** `opus · high`. Built:
  `CardBrand`, `MaskedCardNumber` with `money.mask`, `CardExpiry`, and the
  tax and identity documents checked by their countries' published formats.
- [x] **5.5i3 Prices, discounts and rates** `opus · high`. Built: `Price`,
  `UnitPrice`, `TaxRate`, `Discount`, `BasisPoints` and `InterestRate`,
  choice types in the contract, and `money.apply_discount`.
- [x] **5.5i4 Bank accounts** `opus · high`. Built: `BankAccount` and the UK
  modulus check passing all 34 of Vocalink's cases, run by no `BankAccount`
  until Richard says whether Vocalink's tables, published with all rights
  reserved, may ship.
- [x] **5.5m Commerce, health, travel and the rest: codes and lists**
  `opus · high`. Built: the 29 commerce, health and travel codes and closed
  lists, each checked by a pattern and, where one exists, a check digit.
- [x] **5.5m2 Commerce, health, travel and the rest: records and measures**
  `opus · high`. Built: the 16 commerce, health and travel records and
  measures, four clinical units, and reading an ISBN-10 as an `Isbn`.
- [x] **5.5n Web and network** `opus · high`. Built: the 20 web and network
  types and `web`'s 3 operations, with domain names by UTS 46.
- [x] **5.5n2 Software** `opus · high`. Built: the 18 software types,
  `SemanticVersion` ordered by SemVer's precedence, and `semver.satisfies`.
- [x] **5.5o Text formats** `opus · high`. Built: `Html`, whose construction
  is ammonia's sanitiser, `Markdown`, and `markup.to_html` and
  `to_plain_text`.
- [x] **5.5o2 Rich and localised text** `opus · high`. Built: `RichText` in
  Portable Text's shape with `markup.rich_to_html`, and `LocalisedText` with
  `text.pick`.
- [x] **5.5p Media and colour** `opus · high`. Built: the 15 media and
  colour types, `colour`'s 5 operations with WCAG 2's `contrast`, and
  `media.fit`.
- [x] **5.5q Randomness** `opus · high`. Built: the `random` namespace for
  actions, seeded per test and recorded for replay.
- [x] **5.5q2 Cryptography** `opus · high`. Built: `crypto`'s 8 operations
  that need no keys held as data, and the key types read only through
  `key:`, on audited crates with the standards' known-answer tests.
- [x] **5.5r Tokens and `verify_jwt`** `opus · high`. Built: `Jwt`,
  `JwtClaims`, `Jwk`, `OAuthScope` and `OidcClaims`, and an EdDSA
  `crypto.verify_jwt` that refuses the known attacks.
- [x] **5.5r2 Secret values** `opus · high`. Built: `Password`, `ApiKey`,
  `BearerToken`, `TotpSecret`, `SecretText` and `Jwt` as secret values,
  redacted, compared in constant time and refused by `type.secret_exposed`
  wherever they would leave.
- [x] **5.5r4 JWTs from identity providers** `opus · high`. Built: RS256 and
  ES256 verification, typed custom claims through `as:`, and
  `crypto.read_certificate(der)`.
- [x] **5.5r3 Security records** `opus · high`. Built: `Hash`, `Hmac`,
  `OneTimeCode`, `PasswordPolicy`, `WebAuthnCredential` and
  `EncryptedValue`.
- [x] **5.5r5 Certificates** `opus · high`. Built: `Certificate`, an X.509
  certificate read from PEM on `x509-parser`.
- [x] **5.5z Passkeys** `opus · high`. Built: passkey registration and
  sign-in on `webauthn-rs` through the host's `Passkeys` capability, native
  only until a pure-Rust release builds for wasm32.
- [x] **5.5s Geospatial types' baselines** `opus · high`. Built: the 17
  geospatial types' construction, text and JSON, with GeoJSON read strictly
  on the wire.
- [x] **5.5s2 AI types' baselines** `opus · high`. Built: `Vector`,
  `Embedding`, `TokenCount`, `TokenUsage` and `ChatMessage`, with
  construction, text and JSON.
- [x] **5.5t Cross-field rules** `opus · high`. Built:
  `rule name: condition` across an object type's fields, proved or attempted
  like any rule, with a rule on an entity refused until Richard decides it.
- [x] **5.5t2 Cross-field rules: the standard types** `opus · high`. Built:
  the standard object types' cross-part checks as named rules in
  `jadpo_std::standard::across`, broken alike by literals, code and JSON.
- [x] **5.5u Geospatial: the plan** `opus · high`. Built: the geospatial
  plan, decided by Richard as [area 20](language/20-geospatial.md).
- [x] **5.5u2 Geospatial measures** `opus · xhigh`. Built: `geo.distance`,
  `length`, `bearing` and `area` by Karney's methods in fixed point,
  matching GeographicLib's `GeodTest` alike on native and wasm32.
- [x] **5.5u3 Geospatial predicates and shapes** `opus · xhigh`. Built:
  `geo.intersects`, `contains`, `within`, `centroid`, `bounding_box` and
  `rings_valid` on an exact great-circle geometry, agreeing with PostGIS.
- [x] **5.5u4 Geospatial storage and indexes** `opus · high`. Built: PostGIS
  `geography` columns with their checks, the extension created by migrations
  with `migration.extension_missing`, and declared GiST indexes.
- [x] **5.5u5 Geospatial query conditions** `opus · high`. Built: the `geo.`
  query conditions, box and circle filters and nearest-first order, each a
  GiST search the runtime rechecks, with the indexes they need derived.
- [x] **5.5u6 Geospatial interop** `opus · high`. Built: `Feature` and
  `FeatureCollection`, `geo.from_wkt` and `geo.to_wkt`, and the `LineString`
  and `Polygon` shapes for `type.standard_exists`.
- [x] **5.5v AI: the plan** `opus · high`. Built: the AI plan, decided by
  Richard as [area 21](language/21-ai.md).
- [x] **5.5v1 Vectors in PostgreSQL: storage** `opus · high`. Built: vector
  types stored in pgvector columns, `ai.distance` computed as pgvector
  computes it, and CI on `tools/postgres/Dockerfile` with pgvector and
  PostGIS.
- [x] **5.5v1b Vectors in PostgreSQL: nearest queries** `opus · high`.
  Built: `order: [F nearest v]` giving `Nearest<T>` through a derived HNSW
  index, with `migration.extension_outdated` and the `query.nearest_`
  errors.
- [x] **5.5v1c Vector indexes: longer vectors and tuning** `opus · high`.
  Built: half-precision indexes for 2,001 to 4,000 dimensions, the
  declinable `query.nearest_exact` beyond, and tuned HNSW indexes.
- [x] **5.5v2 Prompt files** `opus · high`. Built: prompt files
  (`<name>.prompt`) with checked placeholders and escaped data blocks,
  `prompts.render`, `jadpo ai show`, and each prompt's digest in the
  manifest.
- [x] **5.5w Every boundary through the contract: storage** `opus · high`.
  Built: every standard object, choice and enum type stored in columns laid
  out from its parts by one codec, and every boundary's encoding decided on
  its owning page.
- [x] **5.5w2 Every boundary through the contract: JSON and the built
  boundaries** `opus · high`. Built: one text reader per built-in type
  behind every built boundary, map keys as canonical text, and a boundary
  test generated from the entries.
- [x] **5.5w2b Every boundary through the contract: the JSON-held values in
  storage** `opus · high`. Built: `RichText`, `RichTextBlock` and
  `LocalisedText` stored as `jsonb` through the route codec.
- [x] **5.5w3 Every boundary through the contract: one encoder**
  `opus · high`. Built: the JSON boundary and OpenAPI calling the contract
  for every standard type, held to it by
  `no_boundary_has_an_encoder_of_its_own`.
- [x] **5.5w3b Every boundary through the contract: SQL literals**
  `opus · high`. Built: compile-time SQL literals made by the runtime's
  storage codec, `jadpo_std::standard::storage::bind`.
- [x] **5.5x `jadpo suggest`** `opus · high`. Built: `jadpo suggest`,
  hinting where code could use a standard type or operation from signals on
  the contract, with `ignore`, `found` and `--catalogue`.
- [x] **5.5y `jadpo suggest share`, against a fake Jadpo HQ** `opus · high`.
  Built: `jadpo suggest share` against a local fake of Jadpo HQ, sending
  hashes and only approved snippets, with no HQ address set until HQ runs.
- [x] **5.6 SQL forms** `opus · high`. Built: the SQL forms of all 38
  standard operations in `jadpo_sql::form`, agreeing with the runtime on
  live PostgreSQL.
- [x] **5.6b Computed backfills** `opus · high`. Built: computed backfills
  as SQL in resumable batches of 1000 rows, then a write-locked last step
  making the column required.
- [x] **5.6b2 Backfills worked out in Jadpo** `opus · high`. Built:
  backfills with no SQL form worked out in Jadpo in the same batches, marked
  `in Jadpo` with a digest of the code they run.
- [x] **5.6c Aggregate queries** `opus · high`. Built: `group:` and
  `aggregate:` queries (`count`, `sum`, `min`, `max`, `average`) by a field
  or a time bucket, each one SQL statement, with a bucket of an instant left
  for Richard.
- [x] **5.5u7 Altitudes all or nothing; a layer from a route**
  `opus · high`. Built: mixed altitudes refused at every door by
  `altitudes_agree`, and a route sending a bare `FeatureCollection`.
- [x] **5.7 Teaching examples for Phase 5** `opus · high`. Built: the Phase
  5 teaching examples, split into 5.7 to 5.7d below.
- [x] **5.7 Teaching examples for Phase 5: settings, time and words**
  `opus · high`. Built: examples 13 to 15 for settings, constants, keys,
  time, translations and locales, and a coverage test that reads more kinds
  of use.
- [x] **5.7b Teaching examples: money, quantities and the standard
  vocabulary** `opus · high`. Built: examples 16 to 19 for money,
  quantities, text and the standard vocabulary, and two compiler bugs they
  found fixed.
- [x] **5.7c Teaching examples: geography and stored data** `opus · high`.
  Built: examples 20 to 22 for geography, stored data, vectors and prompts,
  example 09 moved to `Money`, and two compiler bugs they found fixed.
- [x] **5.7d Teaching examples: the closed lists** `opus · high`. Built:
  each closed list counted as one coverage feature, so the examples pass
  `--require-examples`.

### Phase 5 review (2026-10-19)

Two independent reviewers, soundness and architecture for the phases
ahead, read everything Phase 5 built. Their confirmed findings are the
chunks below, taken before Phase 6.

- [x] **5.8 Gate honesty** `opus · high`. Built: one shared
  `jadpo_test::live()` so no live PostgreSQL test skips silently, a
  crate-wide "one encoder" guard, and the example-name lists worked out at
  check time.
- [x] **5.8a Phase 5 review fixes: secrets and privacy** `opus · xhigh`.
  Built: the Phase 5 secrets and privacy fixes: an idempotency digest
  without secret values, `nearest` orders under field policy, `Secret`
  compared in every container, map keys kept out of problem paths, secret
  nonces, and no start without a clock.
- [x] **5.8b Phase 5 review fixes: migrations and backfills**
  `opus · xhigh`. Built: the Phase 5 migration and backfill fixes: scale
  kept by `UPDATE`, a digest over every type reached and the toolchain
  version, and every backfill filled before one lock-free step makes the
  columns required.
- [x] **5.8c Phase 5 review fixes: rules and vectors** `opus · high`. Built:
  query result types with rules refused, vector distances and half-precision
  items held safe, Jadpo's own JSON reader, and standard-object literals and
  stored values checked.
- [x] **5.5u8 A query giving a map layer** `opus · high`. Built: a query
  returning a `FeatureCollection<G, P>` through `geometry:`, so a `GET`
  route sends a map layer.
- [x] **5.9 Durable values** `opus · xhigh`. Built: 259 golden files pinning
  each standard type's encodings, and an encoding version on every durable
  row and token.
- [x] **5.9b JSON columns** `opus · xhigh`. Built: JSON columns for any
  structured type the route codec handles, with the field's rules applied on
  read and event types in the JSON table.
- [x] **5.9c Migrating stored structured values** `opus · high`. Built:
  migrations that check, refuse or rewrite stored JSON values when their
  type or rules change.
- [x] **5.9d Field rules on read; index reasons** `opus · high`. Built:
  every stored field checked against its rules on read, and derived indexes
  compared by what builds them.
- [x] **5.10 Docs consolidation** `opus · high`. Built: every "Chosen in
  chunk" note to 5.9d, 7.0, 6.1a (but area 12's) and 6.3b folded into the
  text it amends, its contradictions settled against the code and area 12's
  outbox and schedule state put where its 2026-10-19 decision does, so
  areas 1 (2,652 to 2,644 lines), 2 (3,662 to 3,613), 3 (3,952 to 3,939),
  5 (3,055 to 2,981), 6 (4,885 to 4,785), 7 (2,667 to 2,642), 9 (3,465 to
  3,457), 10 (5,988 to 5,855), 12 (1,645 to 1,647), 14 (1,404 to 1,364), 18
  (898 to 893), 19 (11,963 to 11,229), 20 (2,051 to 1,910) and 21 (2,052 to
  1,904), `compiler-runtime.md` (2,873 to 2,185, §2.1 812 to 124) and this
  roadmap (2,201 to 854, finished chunks one line each).
- [x] **5.11 Geo constructions** `opus · high`. Built: `geo.from_geojson`,
  `geo.from_lng_lat` and `geo.from_lat_lng`.

- [x] **5.12 Leftovers from consolidation** `opus · high`. Built: `Bic`'s
  country checked against `Country`, the comments and cross-references
  naming deleted "Chosen in chunk" notes repointed to their sections, area
  14's record examples in format 2 and area 12's open question 16 marked
  superseded. Folding Phase 6's notes is 5.12b.

## Phase 6 — Events and schedules (area 12)

- [x] **7.0 Generated records** `opus · high`. Built: the
  [record format](language/research/jadpo-record-format.md) for every
  generated record, the API and identity records, `jadpo.lock` as a ledger,
  the `record.` errors, and `jadpo records update`.
- [x] **6.1a System caller and delivery engine** `opus · xhigh`. Built: the
  system caller (`Access.system`) and the runtime's runner, driving work
  deferred, inline and per item, with backfills worked out on it.
- [x] **6.1b Events: outbox, delivery and retries** `opus · high`. Built:
  subscribers read and checked, `emit` writing the outbox row and a
  delivery record per subscriber as the action commits, and delivery after
  the response as the system limited to its event's tenant, with retries,
  trace context, `events: { store: }` and the outbox sweep for 6.2's tick.
- [x] **6.1c Event records and stored values** `opus · high`. Built: a
  domain's event shapes in `events/events.jadpo_record`, checked and
  updated as the API record is, and what a subscriber stores from its
  event held to what every `emit` put there.
- [x] **6.2 Schedules** `opus · high`. Built: schedules, their state,
  leases and run records, `schedules: { store: }`, the native tick and
  its sweeps, a schedule reaching every tenant and refused what only
  `Access.public` grants.
- [x] **6.2b Watching schedules** `opus · high`. Built: `jadpo schedules`
  with `run`, `pause`, `resume` and `history`, `jadpo why <schedule>`,
  each run and alert in `jadpo dev`, the alerts with no setup and the
  metrics.
- [x] **6.2c The purge schedule and schedule tests** `opus · high`.
  Built: each purging entity's hourly `Member.purge` schedule on the tick,
  recorded and watched like any, and `schedule.run()` in tests.
- [x] **6.3 Routes that wait for follow-on work** `opus · high`. Built:
  `settle: { answer: within: }`, delivering an action's events inline and
  answering with its query, or 202 past `within:`.
- [x] **6.3b One condition lowering** `opus · xhigh`. Built: one condition
  lowering to an exact SQL form or an index search the runtime rechecks, all
  SQL text in `jadpo-sql`, and operations in `where:` with expression
  indexes.
- [x] **6.4 Batches** `opus · xhigh`. Built: `batches.each` over a named
  query's pages as the system, each page committing its items' writes,
  problems and position together, with `key:`, `batches.status`,
  `batches.cancel`, leases and `batches::resume` for 6.2's tick.
- [x] **6.4c Batches with tenants and grants** `opus · high`. Built:
  `batches.each` in an application with a tenant, each run limited to
  the tenant its saved tenant ids name (none, or different ones, reaching
  no tenant's records), and a batch's query or actions reaching what only
  `Access.public` grants refused as `policy.system_grant_missing`.
- [x] **6.5 Teaching examples for Phase 6** `opus · high`. Built: examples
  23-repair-cafe (events and `settle:`), 24-seed-library (batches) and
  25-allotment-rents (schedules and tenants), passing
  `--require-examples`.
- [x] **6.4d A batch's summary under policy** `opus · xhigh`. Built: a
  batch's summary holding counts and problems only, each item followed
  into what its action stores, and what a run saves proved by the tenant
  it is limited to.
- [x] **6.2d `jadpo schedules run` against a deployment** `opus · high`.
  Built: against a deployment the command asks the app's own tick to run
  the schedule and waits for its record (`schedules.no_tick`); against
  `jadpo dev` it runs it straight away.

### Phase 6 review (2026-10-19)

Two independent reviewers, soundness and architecture for the phases
ahead, read everything Phase 6 built; their confirmed findings are the
chunks below, taken before Phase 7, in this order.

- [x] **6.6a Phase 6 review fixes: exactly-once and limits** `opus ·
  xhigh`. Built: a handled record beside an emitting action's outbox, the
  sweep at the host's time, a batch run's chain depth, 1,000 events per
  item, deliveries given up at the claim, the run limit racing the body
  only, and batch runs deleted 30 days after they ended.
- [x] **6.6b Phase 6 review fixes: policy proofs** `opus · xhigh`.
  Built: every action background work reaches checked against its own
  `actions:` line, `type.batch_input`, and `batches.status` held to what
  decides every batch's failures.
- [x] **6.6d A placeholder naming a missing field** `opus · high`. Built:
  a route placeholder typed with a field its type doesn't declare
  (`{x: Note.nope}`) is `type.unknown_field` at its type, with the
  nearest field offered, rather than dropping its route silently; a
  checked example in area 7.
- [x] **6.6c Tenancy of background work** `opus · high`. Built: the
  tenant an event or a batch's run names proved to be one its action
  reached (`policy.event_tenant_unproved`), and a batch's `key:` unique
  per call, tenant and key.
- [x] **6.7 Background work within a host's budget** `opus · xhigh`.
  Built: a budget for each invocation of background work, every lease the
  budget and a minute, a batch run carried on by time in a fresh
  invocation, and a native server draining on SIGTERM.
- [x] **6.8 Durable background records** `opus · high`. Built: an outbox
  row read by the encoding it records, a batch's call named by identity
  keys with its shape in canonical type text and its saved values'
  encoding, and the system tables defined once in `jadpo_std::system`.
- [x] **6.9 Serving, alerts and `why` in the runtime** `opus · high`.
  Built: what a host does with a request and a tick in the runtime
  (`serve::Serving`), which `jadpo dev` only observes; alerts raised by
  the tick and recorded once on the schedule's state row; `jadpo why`'s
  answer decided by the runtime as data.
- [x] **5.13 Gate time** `opus · high`: test threads capped per gate
  step; the HTTP worker test's 150 ms ordering made deterministic;
  `tools/verify.py` honouring `CARGO_TARGET_DIR`; §2.7's timings
  measured again. Built: the live PostgreSQL steps on two test threads
  each (capping every binary made `compiler tests` the bottleneck, 126
  to 168 s); the worker test sending its quick request once
  `pg_stat_activity` shows the slow one computing; the reload test
  saving atomically (its 404 reproduced from a split write); the gate
  finding builds where cargo says. Full gate warm 86 to 107 s before
  (median 100 s), 89 and 96 s after, on a machine shared with two other
  gates; cold 219 s before, 316 s after under heavier load; CI's gate
  step a median of 280 s before, 219 s in this branch's one run.
- [x] **5.12b Fold Phase 6's notes** `opus · high`. Built: every
  "Chosen in chunk" note from 6.1a to 6.9 folded into the text it
  amends, with the comments, examples and help link naming them
  repointed; `compiler-runtime.md`'s stale trait, host and tick names
  settled against the code; the older Decisions rows on schedules and
  batches pointed at the 2026-10-19 rows that settled them; "no tenant"
  given one meaning per kind of background work (area 12, "Background
  work and tenants"); and 6.10, 6.11, 6.12, 5.13b and 7.2b for what
  nothing builds. Areas 7 (2,852 to 2,830 lines), 9 (4,250 to 4,278),
  12 (3,515 to 3,258) and 13 (1,980 to 1,970), `compiler-runtime.md`
  (2,382 to 2,388) and this roadmap (1,088 to 987).
- [x] **6.10 The tick's remaining deletes** `opus · high`. Built: the
  tick deletes ended browser sessions and API tokens, and expired service
  keys, in the store each is kept in (`SchedulesPlan`'s `sessions` and
  `keys`); and a subscriber's handled records once their delivery's
  record is gone: the statement deleting a settled event deletes those
  kept in its store, and the tick then deletes the rest in each other
  store by the deliveries' ids, so a store it can't reach keeps a few
  records rather than losing one early. 1,000 rows (events, for the
  outbox) at a time, ten times a tick; statements in
  `jadpo_runtime::statements`; live tests in `tick_deletes.rs`. Choices
  in area 12, "The tick".
- [x] **6.11 Events in tests** `opus · high`. Built: what each step of
  a test emits (a call, a request, a schedule's run, `clock.advance`)
  delivered before the next statement, at the test's clock, until none
  is due, each drawn in an order the printed seed and the test's key
  decide; a retry waiting its full backoff for `clock.advance`; a
  delivery that fails for good ending the test as
  `test.subscriber_failed`, a fake failing a subscriber's call (7.0c's
  `Faked`) retried and given up the same way, a test's `fakes:` coming
  with 7.2's generated fakes; examples 23 and 25 reading what their
  subscribers did.
- [x] **6.12 `jadpo deliveries`** `opus · high`. Built: `jadpo
  deliveries` listing what failed or is in doubt (`--state` for the
  rest), and `retry`, `dismiss`, `cancel` and `resolve --done|--not-done`
  on a delivery or a schedule's latest run, each from its state only and
  logged in `jadpo_operator_actions` with the operator's name and
  database role; a retried delivery's fresh budget (`retried_at`), and a
  schedule's run retried through the app's tick. `show`, `retry
  --not-done` and `abandon` come with multi-step calls (7.5), and batch
  runs and parked queue messages with 6.4b and 7.7c.
- [x] **5.13b `jadpo dev` and half-written files** `opus · high`.
  Built: the watcher behind `jadpo dev` and `jadpo test --watch` takes a
  file that had text and is empty now as a save in place not yet
  written, and waits for its text, up to 2 seconds, before handing the
  burst over; chosen over a longer settle, which only covers shorter
  gaps and slows every save (area 14, "Watching saves"). A live test
  truncates the routes file, then writes it 400 ms later, and the old
  version serves between; `config.rs` saves `.env.local` atomically.

- [x] **6.13 Keeping what an operator settled** `opus · high` (found in
  chunk 6.12). Built: the tick deletes a delivery dismissed or cancelled
  30 days after the operator's last action on it (the log's time, so the
  deliveries' table is unchanged), with its handled records wherever
  they are, before it deletes events, so its event goes on the same tick
  once every delivery of it left succeeded; and an operator's log rows
  30 days old once their delivery or run is gone. A delivery that could
  run is never deleted. 1,000 rows at a time, ten times a tick, per
  store; statements in `jadpo_runtime::statements`; live tests in
  `tick_deletes.rs` and `deliveries.rs`. Choices in area 12, "The tick"
  and "What an operator sees".
## Phase 7 — Packages, capabilities and email (areas 11, 16)

The design is area 11 as revised on 2026-10-16, with its research in
[packages and extension points](language/research/packages-and-extension-proposals.md).

- [x] **7.0b Effectful operations in the runtime** `opus · xhigh`.
  Built: `jadpo_runtime::effects::run`, the one entry point both
  executors call for `clock.now` (the graph's own `Expr::Now`, no longer
  source text), `random`, keyed `crypto` and a token's claims,
  `json.read` and `json.write` with the plan's codec, and `emit`, the
  interpreter's copy removed and held off by
  `the_interpreter_runs_effects_only_through_the_runtime`; `Ctx`'s
  background-run state (`Background`) and capabilities (`Capabilities`)
  each a struct of its own, the boundary's state in `boundary.rs`, and
  every field private, reached through methods. Choices in
  compiler-runtime.md §2.5.
- [x] **7.0c The capability seam** `opus · xhigh`. Built:
  `jadpo_runtime::effects::call`, the one way a call leaves the process,
  through one driver trait, `capability::Capability`, which
  `Host::capability(connection)` gives: refused while the action's
  transaction is open (`call_in_transaction`), logged on its invocation
  without its values (`capability::Logged`, on `Response::calls` for
  7.2's `jadpo calls`), and recorded and replayed beside the random bytes
  (`Ctx::record_run`); fakes as host wrappers (`capability::Faked`,
  `Fake`) and one shared test host, `jadpo_test::host::Harness`, now under
  nineteen of `crates/jadpo-cli/tests`' files; a subscriber's `Target`, an
  action or a capability's operation, and a delivery whose unkeyed call
  may have acted parked as `outcome_unknown` (its driver said so, it was
  cut off or its worker lost while the call was out, or its action would
  be retried after one), proved live with a test capability. A batch's
  items still run inside its transaction, so an item's call is refused
  until 7.4. Choices in area 16 and compiler-runtime.md §2.6.
- [x] **6.4b Files and reading rows into batches** `opus · xhigh`
  (areas 12, 16 and 19; split from 6.4b Batch file helpers on
  2026-10-20). Built: `Driver.disk` connections and `File` fields
  (`store:`, `max_size:` in SI sizes, `types:`; kept as JSON;
  `data.file_field_invalid`); `store.put` on 7.0c's seam, keyed, its
  `storage_full` mapped to an `unavailable` failure, and `store.read`,
  10 MB at most (`type.file_read_too_large`, and held to it as it runs);
  the disk driver `jadpo dev` keeps every store's files with under
  `.jadpo/files/`, and the in-memory store a test's are in; `csv.read`,
  `tsv.read` and `json_lines.read` with `column:`, each batch's rows read
  from the store before its transaction opens and committed with the
  run's place (byte, row and line), a row's problems by its row as a
  spreadsheet numbers it and its column, the faults that stop a file's
  run, UTF-8 and marked UTF-16; `type.batch_row_invalid`. Live tests in
  `file_imports.rs`; example 24 keeps and reads donation sheets. Choices
  in area 12's "How a file's run is kept" and area 16's "Chosen in chunk
  6.4b": a file's run is kept where its actions write, and a CSV column
  no field reads stops the run.
- [x] **6.4b2 Exports** `opus · xhigh`, after 6.4b (split from 6.4b2
  Exports and the rest of the file helpers on 2026-10-20). Built:
  `csv.write`, `tsv.write`, `json_lines.write` and `json.write`'s file
  form, each a run over a named query's pages, `each:` an inline
  function giving each item's `Output` row (the action's locals it reads
  saved with the run), `CsvFormat` required for CSV and TSV
  (`spreadsheet`: a byte-order mark and formula-like text escaped), and
  `ExportSummary` holding the written `File`; each page read in a
  transaction that commits before its rows go out, put as a piece of the
  file through the seam, then its position committed, so a resumed page
  puts its piece again under the same key; the pieces joined by the
  store (`join`), a cancelled export's dropped (`discard`); `column:` on
  an `Output`'s fields and `FileType.json_lines`. Live tests in
  `file_exports.rs`; example 24 writes a shelf list. Choices in area 12's
  "How an export's run is kept", area 16's "Chosen in chunk 6.4b2" and
  area 19's "How a row is written".
- [x] **6.4b3 The rest of the file helpers** `opus · xhigh`, after
  6.4b2. Built: `json.read`'s file form, given `each:`, walking the list
  `items_at:` names (member names between dots, written out; left out
  when the document is the list) a window at a time, holding only the
  item in hand, each item a row by its place in the list and the rest of
  the document held to JSON's structure; `max_unpacked_size:` on every
  reader, a gzip file (every member, each header and trailer checked)
  unpacked as it is read and the run stopped at the row that would pass
  it, a resumed run unpacking from the start; `type.batch_order_updatable`
  for a `from:` ordered by a field that isn't `id`, `immutable: true` or
  generated on create, declinable on the action; and `estimated_total`
  from the query's count as its first page reads it, or a file run's
  items scaled by how far into its file they end, the items done once
  finished (`jadpo_batch_runs.estimated_total`). Example 25 walks plots
  by `id`; the tests that walk by a code or a title mark it `immutable:
  true`. Live tests in `file_imports.rs` and `batches.rs`. Choices in
  area 12's "How a run is kept" and "How a file's run is kept", area
  16's "Chosen in chunk 6.4b3" and area 19's "How a JSON file's list is
  read".
- [x] **7.1 Packages: format, kinds, names and the lock** `opus · high`
  (split from 7.1 Packages and connections on 2026-10-20). Built:
  `packages/<name>/<version>/` and `shared/providers/<name>/<version>/`
  read as packages laid out as `shared/` is, `package.jadpo` and
  `provider.jadpo` checked, names scoped to their package and written
  through it (`use packages/postcodes`, `Postcodes.UkPostcode`,
  `Postcodes.uk.tidy(...)`) in the checker, the graph and the
  interpreter, each package's code seeing only its own names, `public`
  and `visibility.public_reaches_private` in packages and domains,
  reserved and taken names, and `jadpo check` holding each installed
  package to its line and digest in `jadpo.lock`; the `package.*` errors
  and example 26. Choices in area 11's "Chosen in chunk 7.1", with one
  flagged for Richard (a package's code doesn't see the app's `shared/`).
- [x] **7.1b Package surface, `show` and `check`** `opus · high`, after
  7.1 (split from 7.1b Package surface and commands on 2026-10-20).
  Built: `surface.jadpo_record` (`record surface 1`) written from a
  package's code, every entry pointing at its file; `jadpo package show`
  of a package, one declaration in full or its guide, installed or the
  app's own, and each own integration's surface in `build/providers/`;
  `jadpo package check` in a package's own repository, checked as the
  app's own integration, with the publishing bar, the version worked out
  from the surface at the last tag, `publish.description_missing`,
  `publish.breaking_change`, and `type.standard_exists` on a package's
  public types only, in a package's words. Choices in area 11's "Chosen in
  chunk 7.1b"; two questions for Richard (area 11, questions 27 and 28:
  how an example in a description is written, and the changelog's form),
  whose bar items say "not checked yet" meanwhile.
- [x] **7.1b2 Installing packages** `opus · high`, after 7.1b. Built:
  `jadpo package add` from a folder or a git repository's release tag,
  compiled as the app's own integration and its surface checked against
  its committed `surface.jadpo_record` before it is copied and locked
  (`install.surface_mismatch`, `install.fetch_failed`); `update` to a
  folder's or a repository's newest release with area 18's review of what
  changed; `remove`, refused while the app uses the package
  (`install.in_use`); `link`, every check reading the checkout in the
  installed folder's place, and `unlink`, restoring the line kept in
  `.jadpo/links`; with `CI` set, `jadpo check` and `jadpo test` refuse a
  link (`install.link_refused`). The bar's examples, ```` ```jadpo ````
  blocks in descriptions run as tests (`publish.example_missing`,
  `publish.example_failed`), and changelog, a `CHANGELOG.md` section per
  version written from the surface with the author's why
  (`publish.changelog_missing`). Choices in area 11's "Chosen in chunk
  7.1b2"; publisher trust waits for `jadpo approval`.
- [x] **7.1b3 Library packages' translations** `opus · high`, after
  7.1b: a library package's `translations/` (area 10, "Jadpo's and
  packages' text"), `Postcodes.translations.<message>(...)`, the root
  file's wording grouped by package, and its messages in the surface and
  the publishing bar. Built: a library package's `translations/` checked
  as a domain's, each file against the locale it is named for;
  `Postcodes.translations.<message>(...)` through the checker, the graph
  and the interpreter; the root file's package groups checked against the
  package's public messages, enums and failures, each supported locale
  worded there or by the package, the app's wording first; their lines in
  the root `sources.jadpo_record`, stale when the package rewords its
  text; a package failure a route answers with worded for clients; `jadpo
  translations missing`; messages and locales in the surface and `jadpo
  package show`; and the bar's `wording` row and message descriptions.
  Choices in area 10's "Chosen in chunk 7.1b3", one flagged for Richard
  (a package has no default locale, so every language it ships must be
  complete); `jadpo translations export`, `import` and `unused` don't take
  a package's text yet.
- [x] **7.1b4 Package text in the translation commands** `opus · high`
  (found in chunk 7.1b3): `jadpo translations export`, `import` and
  `unused` taking in a library package's messages and the app's wording
  of them, as `missing` already does. Built: export sends what a
  supported locale must word of each package, and anything else of it
  the app words, as `postcodes.<member>` units translated from the app's
  default wording else the package's, with the app's root wording else
  the package's own as the target; import writes what comes back into
  the root file's package group and the root record, never the
  package's files; `unused` lists, under `Package wording`, root-file
  members wording package text nothing calls, the package no longer has
  or keeps private, or no package owns. Choices in area 10, "Chosen in
  chunk 7.1b4".
- [x] **7.1d Package names kept to their package** `opus · high` (found
  in chunk 7.1b2): a library function in an installed package clashing
  with an app action of the same name (`Postcodes.uk.tidy` against an
  app's `tidy` gives `semantic.duplicate_declaration`), though a
  package's names are scoped to it; and `jadpo dev` and `jadpo test
  --watch` watching a linked checkout, not only reading it at start.
  Built: a package's declarations unique only within it, never clashing
  with the app's (types, failures, events, actions, queries, libraries)
  or another package's, each name reaching the right one in the checker,
  the graph and the interpreter; the shared watcher reading the lock at
  every look and watching a linked checkout at the package's paths, with
  5.13b's half-written-file guard, `test --watch` reading a relinked
  folder again; and a linked package's problems shown in its checkout by
  `jadpo test` and `dev` as by `check`. Choices in area 11's "Chosen in
  chunk 7.1d".
- [x] **7.1c Connections and the network report** `opus · high`, after
  7.1b2 (split from 7.1c Connections, the network report, lazy loading
  and versions side by side on 2026-10-20): `connections:` entries naming
  a package (`package:`, `domains:`, `credential:`, `environment:`,
  `host:` against `hosts:` and `runs:`, settings), the network report,
  and the dev and test hosts answering a connection only with its
  declared driver. The public registry stays after the first build.
  Built: `package:` naming a provider package the app holds by name and
  version with no `use` line; `domains:`, `credential:` from a secret
  setting, `environment:` from a `ProviderEnvironment` setting (a new
  built-in enum), `host:` from a `Url` setting only where the package has
  no fixed hosts, and the package's own `settings:` (plain values) by
  name, with `connection.package_unknown`, `key_invalid`, `key_missing`
  and `value_invalid`; `build/report/network.md` and `.json`, a row per
  host with when, what for and the domains using it; and the dev and test
  hosts giving the disk driver only for the plan's stores of files, so a
  package's connection has no driver until 7.2c. Choices in area 11's
  "Chosen in chunk 7.1c" (area 10 notes the settings).
- [x] **7.1c2 Lazy loading and two versions side by side** `opus · high`,
  after 7.1c and 7.1d (both touch how names through a package resolve):
  lazy loading of a package's resources by file name with the check
  cached per digest, and two versions of one package side by side, names
  carrying the version (`Postcodes.v3.UkPostcode`). Built: an installed
  provider's `resources/<resource>.jadpo` compiled only once the app
  writes `<connection>.<resource>`, still counted in the digest; `jadpo
  check` keeping each installed package's clean files in
  `~/.cache/jadpo/packages/<digest>/<compiler>` and skipping them after;
  two versions of one package side by side in the checker, the graph and
  the interpreter, each locked, connected by version and named by version
  in the `jadpo package` commands, with `package.version_needed` and
  `package.version_unneeded`; and a connection named like a domain or
  library refused (question 17). Choices in area 11's "Chosen in chunk
  7.1c2", one flagged for Richard: two versions of a package with
  translations wait, recommendation the version joins the wording's key.
- [x] **7.2 Served OpenAPI and the importer** `opus · high` (split from
  7.2 Importing, decoding and faking on 2026-10-20): `x-jadpo` fields in
  every API's OpenAPI document, and each API serving it at
  `<prefix>/openapi.json` (under its own authentication, `serve_openapi:
  false` to turn off, listed in its own OpenAPI and record and in `jadpo
  routes`); `jadpo package import openapi <file>` reading a local
  document into the app's own integration in
  `shared/providers/<name>/<version>/` (index, `provider.jadpo` with
  `imported:`, resources, types and failures), exact where it finds
  `x-jadpo` fields, with area 11's exact format mappings (its first
  layer), merging into an earlier import rather than overwriting it, and
  its summary. Built: `GET <prefix>/openapi.json` answered by the runtime
  with the document `jadpo check` writes (the plan holds it; the
  project's name, which titles it, is now an input of the project),
  needing a caller by any method its API accepts unless the API accepts
  none; `x-jadpo-changes`, `-idempotency`, `-failure` and `-constraints`
  (a field's type as the record writes it) and a tag per route group,
  with `x-jadpo-events` waiting for publishing events outward (area 7,
  "OpenAPI"); and `jadpo package import openapi`, with its choices in
  area 11's "Chosen in chunk 7.2", one flagged for Richard: a provider
  field's or variant's wire spelling (`collectAt`) has no Jadpo form, so
  the importer lists each; recommendation `wire_name:`.
- [x] **7.1c3 Wording two versions of a package** `opus · high` (area 11,
  decided 2026-10-20): two versions of a library package with
  `translations/` held side by side, the app wording each under
  `<package>_v<n>: { }`, in the root file, the record, the translation
  commands and the publishing bar. Built: while names carry the version,
  each version's text keyed `postcodes_v3` in the root files' groups, the
  root record, `jadpo translations missing`, `export`, `import` and
  `unused`, and in calls through the checker, the graph and the
  interpreter, the "not built yet" refusal lifted; a group keyed for what
  was held before (the folder's name once two are held, a version's key
  once one is, a version no longer held) reported as
  `translation.group_unknown` with the rename as its edit, nothing
  moved by `add`, `remove` or `jadpo fix`; record lines following the
  wording to its new key, so acceptance stays and staleness is judged
  against the version it now words; a domain named like a version's key
  `package.name_taken`; the publishing bar unchanged, one package being
  one version. Choices in area 10's "Chosen in chunk 7.1c3".
- [x] **7.2c Provider calls** `opus · high`, after 7.2 and 7.1c (split
  from 7.2c Wire, decoding and provider calls on 2026-10-20): a provider
  operation called through its connection over 7.0c's seam by the package
  driver (JSON over HTTP), tested against a local HTTP server started in
  the test; a package's operations read and checked; `output_at:` and
  `failed_when:`; derived provider idempotency keys (app, environment,
  caller, step), which also close a collision between users; and the
  connection's `domains:` enforced at the call.
  Also `wire_name:` on a provider's fields and enum variants (area 11,
  decided 2026-10-20), written by the importer where the spelling
  differs. Built: a package's operations read from `resources/` and
  checked (`package.operation_invalid`); `connection.resource.operation(
  input)` in an action of a listed domain (`connection.domain_not_listed`),
  its package failures mapped at the call by code to the app's own of the
  same kind with the provider's as the cause, unmapped ones faults; the
  plan's `ProviderPlan`, the runtime making each request (wire names,
  placeholders, query or JSON body, bearer or basic credential, a key
  derived from the app, environment, connection, caller's scope, action's
  key and step) and decoding the answer from `output_at:` with
  `failed_when:`; the package driver (`jadpo_drivers::package`) over a
  host's `Fetch`, natively HTTPS through OpenSSL or plain HTTP to this
  machine, sending nothing elsewhere; `wire_name:` on fields and variants
  (`package.wire_name_invalid`), written by the importer; and package
  object types built through the package's name. Live tests in
  `provider_calls.rs`, a call before the first write going out and one
  after it refused (`call_in_transaction`, until 7.4); example 26 books a
  pickup. Choices in area 11's "Chosen in chunk 7.2c"; `jadpo dev` and
  `jadpo test` give a package connection no driver until 7.2d.
- [x] **7.2c2 Wire encodings and decoders** `opus · high`, after 7.2c:
  hand-written packages' `wire:` (the built-in encodings), `decode:` and
  `encode:` on fields, and `wire_output:` with `decode:` on operations,
  each package function run by the executor at the call's site; decoding
  lazily, only the fields code reads (7.2c decodes every declared field
  as the answer arrives); `timed_out:` mapped on an idempotent operation;
  and `retry:` with its backoff. Built: ten built-in encodings
  (`Wire.unix_seconds` to `Wire.minor_units`, `Wire.number` flagged for
  Richard) applied on the provider's wire only, the contract's JSON
  everywhere else, through a codec of the field's own; a field's
  `decode:` and `encode:` and an operation's `wire_output:` with
  `decode:`, checked against the package's functions
  (`package.wire_invalid`, `package.operation_invalid`) and run by the
  interpreter at the call's site (`provider::Functions`); only the
  top-level fields an action reads decoded, worked out per call site in
  the plan; numbers kept as written both ways; a connection's `retry:`
  (`attempts:`, `within:`) sending again what is safe to, with full
  jitter and `Retry-After`; and `timed_out:` mapped on an operation safe
  to send again, the driver telling a timeout apart. Live tests in
  `provider_wire.rs`; the surface and `jadpo package show` give an
  operation's answer as sent. Choices in area 11's "Chosen in chunk
  7.2c2".
- [x] **7.2d Fakes and the call log** `opus · high`, after 7.2c (split
  from 7.2d Fakes, recordings and the call log on 2026-10-20): a
  generated fake per package and `fakes:` for provider operations in
  tests; provider calls faked by default in `jadpo dev`, with
  `development: { sandbox: }`; and the call log screen, `jadpo calls`,
  `jadpo calls show` and `jadpo calls script`, on 7.0c's data. Built:
  a package's fake (`jadpo_runtime::fakes::ProviderFake`) in every
  provider connection's place in tests, answering the operation's
  generated default, a value of its output type keeping its rules,
  written as the provider sends it and read back through the real
  decoding; a test's `fakes:` scripting each operation's answers in
  order (a value, a failure's code, or `unavailable`, `lost`, `in_use`,
  `misconfigured`), checked against the package
  (`semantic.fake_invalid`), a script run past `test.fake_exhausted`
  and one with answers left `test.fake_unused`; `jadpo dev` faking every
  provider call with no credential needed (a missing provider setting
  gets a stand-in, and the connection stays faked with an alert naming
  the variable), letting `development: { sandbox: }`'s operations or
  resources through to the sandbox once its credential is set, never
  live (`cli.dev_start_failed`), `--fake` and `--sandbox` for a session;
  each request's line listing its calls; and the call log in
  `build/dev/calls.jsonl`, with `jadpo calls`, `calls show` and `calls
  script` (`cli.calls_arguments`). Example 26 books a pickup through
  `POST /pickups`, faked in its tests and in `jadpo dev`. Choices in
  area 11's "Chosen in chunk 7.2d". Not built: `.requests`, `jadpo
  trace`, timelines; enum variants written through a package's name in
  an expression (`AcmeCouriers.Window.morning`) are refused as
  `type.not_a_value`, so a scripted value can't give an enum field yet
  (built in 7.1e).
- [x] **7.1e Enum values through a package's name** `opus · high` (found
  in chunk 7.2d). Built: a package enum's value written through the
  package, and its version while two are held
  (`AcmeCouriers.Window.morning`, `Postcodes.v3.Round.morning`), in code,
  constants, a test's `fakes:`, a variant built with its fields and a
  `match` pattern, one path with a type's name through a package. Choices
  in area 11's "Chosen in chunk 7.1e".
- [x] **7.2d2 Recordings** `opus · high`, after 7.2d and 7.2c2:
  recordings and `jadpo package record` (against a local HTTP server in
  tests), every decoder run over every recording in `jadpo test` and the
  publishing bar, and `jadpo dev`'s fakes answering from a package's
  recordings, their placeholders filled. Built: a recording keeps the
  answer along `output_at:` and the members the output type declares,
  each plain text or number a placeholder naming its built-in type
  (`"{Text}"`), what the package's own encodings and decoders read kept
  as sent (`jadpo_runtime::recorded`); `jadpo test` reads every recording
  of the app's own integrations through the package's decoders as a test
  of kind `recording`, a field's `decode:` and `encode:` checked to give
  back what was recorded (`test.recording_undecodable`,
  `test.round_trip_failed`), and the publishing bar as its `decoders`
  row; `jadpo dev`'s fake answers an operation from its recording, its
  placeholders filled, logged as `recording`; and `jadpo package record
  <connection> <operation> --input <json>` calls the sandbox once through
  the package driver, never live and never with a stand-in credential
  (`recording.refused`), keeps an answer that decodes and refuses one
  that doesn't or a refusal (`recording.failed`), into the app's own
  integration or a linked checkout. Live tests in `recordings.rs`
  against a local server. Choices in area 11's "Chosen in chunk 7.2d2";
  writing an operation's output type from the answer waits for 7.2e.
- [x] **7.2e Mapping imported types to standard types** `opus · high`,
  after 7.2d and 7.1b (area 11, "Mapping imported types to standard
  types", Richard 2026-10-17): shape and name detection decoded at the
  edge, and declinable findings with `(agent)` steps, in the import's
  summary. Split in this chunk: the scoped review and unconfirmed
  operations are 7.2e2. Built: `wire_parts:` on a provider package's
  field, a standard type read from and written as members of the object
  holding it (`Money` from Stripe's `amount` and `currency`, a `Point` from
  `lat` and `lng`, fields sharing a member), checked as
  `package.wire_invalid` and `package.wire_name_invalid`; the importer
  writing it for an amount beside its currency and a latitude beside a
  longitude, and an object that is `Money` or `InstantRange` whole as that
  type; `package.standard_candidate`, a declinable error on a whole
  number named like a time, one named like an amount beside a currency
  and an unmapped latitude and longitude; the summary's `standard` count,
  "Mapped by shape" list and each guess under "To check", `mapped` in
  `--json`. Choices in area 11's "Chosen in chunk 7.2e".
- [x] **7.2e2 The scoped review and unconfirmed operations** `opus ·
  high`, after 7.2d2 and 7.2e (area 11, "Mapping imported types to
  standard types", layers 4 and 5): the import's one review instruction
  scoped to the types the app's operations reach, each imported type
  tracked as reviewed or not in `jadpo check` (and recorded for `jadpo
  approval`, which comes after the first build), growing when the app
  calls something new; operations the app calls still unconfirmed by a
  recording, listed in `jadpo check` and the import's summary. Built:
  `reviewed:` in `provider.jadpo`'s `imported:`, each one of the
  package's own types once (`package.provider_invalid`); the scope worked
  out from the app's calls on every check, the types their input and
  output reach; `jadpo check` listing the types still to review and the
  calls no recording confirms once it passes, counted in its footer and
  given as `providers` in `--json`, and writing both to
  `build/report/providers.md` and `.json` for `jadpo approval`; the
  import ending with the scoped instruction and the unconfirmed calls,
  keeping `reviewed:` when it merges, less any type the description
  added to. Choices in area 11's "Chosen in chunk 7.2e2".
- [x] **7.2b The outbox's circuit breaker** `opus · high` (area 12,
  question 18), after 7.2c: per connection, after a run of `unavailable`
  failures from one provider, deliveries that call it are held rather
  than attempted, one probe delivery is tried after a pause, and success
  releases the rest; held deliveries keep their retry budget, are shown
  in `jadpo deliveries` and alerted. Built: `jadpo_circuit_breakers` in
  every store that keeps deliveries, and `held_for`, `held_since` and
  `held_ms` on `jadpo_deliveries`; five delivery attempts in a row whose
  call through a connection found its provider unavailable open its
  breaker, and the deliveries whose action calls it (the plan's
  `connections`, from the checker's `called_connections`) are held at
  their claim, the attempt given back; a probe after a minute, doubling
  after each failed one up to ten minutes, the sweep waking the delivery
  held longest; an answer closes it and releases them all; the time held
  not counted towards `within:`; the tick raising
  `connection_unavailable` once per outage; `held` in `jadpo deliveries`
  with an alert line per connection. Live tests in
  `circuit_breaker.rs` against a local server answering `503`. Choices
  in area 12's "When a provider keeps failing".
- [x] **7.3 Email: templates and the capability over SMTP** `opus · high`
  (split from 7.3 Email on 2026-10-20): every `EmailMessage`'s `to:` and
  `locale:`; its `.html` and `.txt` templates in `emails/` under the strict
  rules, checked by the body checker and run by the executor; the shared
  layout; `email: { connection: from: }`; `emit` of an email delivered to
  the capability after commit, rendered whole at send, attachments
  included, an unknown outcome parked; and the SMTP driver, tested against
  a local SMTP server. Built: an email's `.html` and `.txt` read as
  records beside its type, checked as pure callables taking its fields
  (`email.template_missing`, `template_not_allowed`,
  `template_value_invalid`, declinable `field_unused`), translations and
  `text.format` in its locale, `{{ locale.direction }}` from Jadpo's
  `text_direction` wording, numbers formatted, enums worded, HTML
  escaped unless `Html`; `shared/emails/layout.*` with `{{ content }}`,
  `{{ subject }}` and `{{ app.display_name }}` from `application {
  display_name: }`; `email:` and `Driver.smtp` connections
  (`email.settings_invalid`, `email.connection_required`, area 11's
  connection rules for `host:`, `port:` and `credential:`, the network
  report's row); email subjects required in every complete locale; each
  email type a subscriber of its own targeting `Target::Email`, the
  runtime rendering it at send (`jadpo_runtime::email`) and sending it
  unkeyed; the SMTP driver (`jadpo_drivers::smtp`, MIME and the seam's
  classification) over the native host's `NativeRelay` (`STARTTLS` or TLS
  on 465 through OpenSSL, in the clear only to this machine); `jadpo
  test` catching every email. Live tests in `crates/jadpo-cli/tests/
  emails.rs` against an SMTP server started in the test; example 27
  teaches email. Choices in area 16's "Chosen in chunk 7.3", one flagged
  for Richard: where a partial is declared; recommendation `type X =
  EmailPartial { ... }` with its templates beside it.
- [x] **7.3c Email: the second connection, partials and `email.sent`**
  `opus · high`, after 7.3 (split from 7.3 Email on 2026-10-20; the dev
  inbox and the health score split to 7.3c2 the same day): a mail package
  as a second connection proving the swap; partials; `email.sent` in
  tests. Built: `provides/email.jadpo`'s `provide email` read and checked
  (`send:` with `call:`, `input:` giving every part of the message and the
  attachments as `each:`/`as:`, `receipt:`, `failures:` onto
  `EmailFailure`, and `limits: { attachments: }`), `email:` naming a
  package's connection, which needs no `domains:`, each email's `File`
  fields' `max_size:` within the limit (`email.attachment_limit`), and the
  delivery calling the mapped operation with no key, its failures an
  email's; `packages/resend`, tested against a local server, the swap
  changing `app.jadpo` and its settings only; partials, `type X =
  EmailPartial` in `shared/emails/` or a domain's `emails/`, included as
  `{{ include name(field: value) }}` with the including template's locale,
  nested but never cyclic (`email.partial_invalid`); `email.sent` in tests
  as a `List<SentEmail>` (`email.sent_context` elsewhere), the mail caught
  whole whichever connection `email:` names. Choices in area 16's "Chosen
  in chunk 7.3c".
- [x] **7.3c2 Email: the dev inbox and the health score** `opus · high`,
  after 7.3c (split from 7.3c on 2026-10-20; SpamAssassin's rules split to
  7.3c3 the same day): the dev inbox in `jadpo dev` and `jadpo email
  list`, `show`, `links`, `open` and `clear`, and its page; the health
  score, `jadpo check`'s warnings about templates that would fail it, and
  `jadpo email check-domain` (its DNS lookups behind a seam, tested with a
  fake resolver). Built: `jadpo dev` giving `email:`'s connection the
  test's catching driver, each mail kept whole in `build/dev/inbox/`
  (`dev_inbox`), the last 200, its line naming only its id, email and
  address, an SMTP connection's unset settings given stand-ins; the
  inbox's read-only page on `127.0.0.1` at the app's port plus one
  (`inbox_page`), answering `localhost` only, the mail's HTML sandboxed,
  its text, numbered links, attachments and health, new mail looked for
  every two seconds; `jadpo email` (`cli.email_arguments`); the score's
  seven checks with their fixes and the inbox-placement caveat
  (`jadpo_std::email_health`); `email.health_problem`, declinable, on
  templates where their own text answers a check (`mail_health`); and
  `check-domain`'s SPF, DKIM (`--selector`) and DMARC rules over
  `jadpo_host_native::dns`. Choices in area 16's "Chosen in chunk 7.3c2".
- [ ] **7.3c3 Email: SpamAssassin's rules in the health score** `opus ·
  high`, after 7.3c2 (split from it on 2026-10-20): when SpamAssassin is
  installed, the dev inbox's score and `jadpo email show` add its open
  rules (area 16, "A mail health score"), the mail run through it as the
  SMTP driver writes it, its run behind a seam its tests fake.
- [x] **7.3b Magic-link sign-in** `opus · high` (area 8, decided
  2026-10-16; split on 2026-10-20, the rest in 7.3b2): `sign_in:` with
  `magic_link:`, the generated endpoints, link and six-digit code, the
  wrong-code and send limits, `on_first_sign_in`, the built-in email's
  first three levels and the sign-in failures. Built: `magic_link:` read
  and checked (`address:`, `link:`, `expires_after:`, `code_attempts:`,
  `codes_per_hour:`, `failed_codes_per_day:`, `resend_after:`,
  `same_browser:`, `path:`; a limit loosened is `toolchain.not_built_yet`
  until acceptances), needing the browser session's `origin:`, the
  `email:` connection and `display_name:`; `sign_in_providers:` refused
  with its rename; `on_first_sign_in:` checked as an action taking the
  built-in `FirstSignIn` and returning the user's `id`, with
  `SignInMethod`; the runtime's `POST /auth/magic-link`, `/confirm` and
  `/redeem` (`http::magic_links`), one row per address in
  `jadpo_sign_in_requests`, the link and code made as the email is sent
  and kept only as verifiers, codes compared in constant time, only the
  latest valid, single use, per-address send limits alike for every
  address, wrong codes ending a code and the day's stopping codes but not
  links, `same_browser:`, and the first sign-in run as the system; Jadpo's
  own email at levels 1 to 3 (its wording, rewordable under `jadpo: { }`,
  wrapped in the app's layout); the routes in the API record, OpenAPI and
  `jadpo routes`, and route overlaps refused. Live tests in
  `crates/jadpo-cli/tests/magic_links.rs` with the email connection
  faked; area 8 gains a whole magic-link application. Choices in area 8's
  "Magic links (chosen in chunk 7.3b)" rows and area 16's "Chosen in
  chunk 7.3b"; area 8's question 15 (`routes` is a reserved word)
  decided as `generate_routes: false`.
- [x] **7.3b2 Magic links: the app's own email and sending** `opus · high`,
  after 7.3b (split from 7.3b on 2026-10-20, and again on 2026-10-20 into
  7.3b2, 7.3b3 and 7.3b4): `magic_link: { email: }` (level 4,
  `MagicLinkEmail` with `pick:`, `SignInCode`) and `send:` (level 5, an
  action taking `MagicLinkRequest`, retried as a subscriber),
  `email.connection_required` waived with `send:`. Built: `email:`
  checked as an `EmailMessage` of the app's whose every field besides
  `to:` and `locale:` is picked from the built-in `MagicLinkEmail`
  (`address`, `sign_in_link`, `code`, `expires_in`), never emitted;
  `send:` checked as an action taking one `MagicLinkRequest` and
  returning `Unit`, needing no email connection; `SignInCode`, six digits
  of text, never kept, answered, put in a failure or an event, or written
  as JSON, and `MagicLinkEmail` copied only into an email; the one
  `jadpo.MagicLinkEmail` delivery filling and rendering the app's email
  from the link and the code it makes, or running the app's action as a
  subscriber's, each attempt with a new link and code. Live tests in
  `crates/jadpo-cli/tests/magic_links.rs`, the provider faked. Choices in
  area 8's "The app's own sign-in email and sending (chosen in chunk
  7.3b2)" and area 16's "Chosen in chunk 7.3b2".
- [x] **7.3b3 Magic links: the app's own routes** `opus · high`, after
  7.3b2 (split from 7.3b2 on 2026-10-20): `generate_routes: false` and
  `magic_links.request`, `.redeem` and `.redeem_code` for the app's own
  routes, keeping every protection (`SignInToken`, `MagicLinkRedeemed`,
  `same_browser:`'s cookie on the response of the route whose action asks).
  Built: `generate_routes:` read (never beside `path:`), switching the
  generated routes off everywhere they are listed; the three operations
  checked in actions only, needing `generate_routes: false`, and run by
  the generated routes' own code, asking in the action's transaction with
  the email sent once it commits, and using a link or a code in a
  transaction of its own so wrong codes stay counted; `SignInToken` never
  kept, answered, put in a failure, an event or an email, or written as
  JSON; `MagicLinkRedeemed` made only by the runtime; the binding cookie
  set and cleared on the app's route's response; `Retry-After` on the
  operations' limits; `sign_in_return_invalid`. Live tests in
  `crates/jadpo-cli/tests/magic_links.rs`. Choices in area 8's "The app's
  own routes (chosen in chunk 7.3b3)".
- [x] **7.3b4 Sign-in events** `opus · high`, after 7.3b3 (split from 7.3b2
  on 2026-10-20): the events `SignedIn`, `SignInRefused` (with
  `SignInRefusal`) and `SignInCodesDisabled`, the refusal's reason logged;
  `FirstSignIn` made in tests to call `on_first_sign_in:`'s action
  directly. Built: the three events as built-in types a subscriber names
  in `on:` (needing `magic_link:`), only the runtime emitting them, kept
  under `jadpo.<Name>` in the sessions' store's outbox only when a
  subscriber listens; `SignedIn` committed with the session or token, or
  as the app's own action's event from its own routes; refusals and
  codes stopping kept in a transaction of their own, with the user's `id`
  where the address has one, for links, codes and first sign-ins through
  the generated and the app's own routes; the reason, and the failure
  the app's action refused with, on `jadpo dev`'s request line, never in
  the answer; `FirstSignIn` and the events built in tests, fixtures and
  seeds. Live tests in `crates/jadpo-cli/tests/magic_links.rs`. Choices
  in area 8's "Sign-in events (chosen in chunk 7.3b4)".
- [x] **5.5z2 Passkey recovery** `opus · high` (area 8, "Passkeys"; split
  from 5.5z), after 7.3b4 and the events of area 12: `passkey:` needs
  `magic_link:` (`semantic.auth_setting_required`), and area 8's passkeys
  example gains it; the device lists a passkey under `magic_link:`'s
  `address:` rather than the principal's subject; `test_passkeys.register`
  and `.sign_in` with `TestPasskey` in route tests; `SignedIn` and
  `SignInRefused` for passkeys, `PasskeyAdded` and `PasskeyRemoved`. Done
  when a person with no passkey left recovers through a magic link and
  adds a new one, in a route test written with `test_passkeys`. Built: the
  check and area 8's example with magic links and a route test; the
  device's name read from the address field; the passkey events published
  through 7.3b4's code, committed with the session or passkey, refusals
  logged; `test_passkeys` checked in tests only and run on the test
  host's software authenticator. Recovery through the link's code is
  proved live (`crates/jadpo-cli/tests/passkeys.rs`); in a route test
  `as:` stands in for the link until `email.sent` (7.3c). Choices in area
  8's "Passkey recovery, events and route tests (chosen in chunk
  5.5z2)".
- [x] **7.4 Outside calls hold no transaction** `opus · xhigh` (areas 6 and
  11, 2026-10-15): reads before a call in a read-only snapshot closed before
  it, the transaction opened after it, a write to a record loaded before the
  call checked by its row version, and a change rerunning the action with
  the call repeated under its key. Built: at the capability seam, what an
  action has only read when a call goes out is its snapshot, which reads
  the `xmin` of the rows it loaded (`RowVersions` on each plan `Select`
  that reads an entity's `id`, one statement per table) and rolls back
  before the call; the next store operation begins a new transaction
  (`Ctx::read_transaction` for reads, `Ctx::transaction` marking it
  written); every update and delete plan's `RowCheck` holds a row loaded
  before a call to that version, read `FOR NO KEY UPDATE` once per
  transaction, and a reference to a record deleted meanwhile is a
  concurrent change too; a change reruns the action within the budget, its
  keyed call repeated under its key; a call after a write is still
  `call_in_transaction`; and a caught action whose call closed the
  snapshot rolls its own transaction back whole. Live tests in
  `call_snapshots.rs` hold the provider's request while they check
  `pg_stat_activity` and change or delete the shipment the action read;
  the SQL goldens show both statements. Batch items' calls and the
  compile-time order check split to 7.4b. Choices in area 6's "Outside
  calls hold no transaction" and compiler-runtime.md §2.6.
- [x] **7.4b Calls from a batch's items, and calls checked before writes**
  `opus · xhigh`, after 7.4 (split from 7.4 on 2026-10-20): when
  `each:`'s action makes outside calls, each item a batch of its own, run
  as a subscriber's delivery is, its calls first under keys derived from
  the run and the item's position, then its writes and the run's position
  committed together (area 12, "How a run goes"); and a write placed
  before a later outside call a compile error naming the call it must
  follow (area 6, "Several outside calls"), so checked code never reaches
  `call_in_transaction`. Built: `outside_calls` in the checker works out
  which actions make outside calls and which write, through the actions
  they call, and `data.write_before_call` (not declinable) is reported at
  a write some way through the body runs before a call, naming the last
  one; the plan marks an `each:` or `finished:` action that calls, and
  the runner's `batches::calling` runs each such item as a batch of its
  own: the run's place read on its own, the item read as a page of one
  giving its own cursor (or a file's next row), its action run with no
  transaction open and its calls keyed from the run and that cursor or
  row, then the run's row locked and held to that place and the item's
  writes, problem and next position committed together; `finished:`
  likewise. Live tests in `batch_calls.rs` hold each call to
  `pg_stat_activity` showing no open transaction, crash an item between
  its call and its commit and resume it under the same key, and run
  `jadpo check` on a write before a call; `provider_calls.rs`'s write
  before a call is now refused by `jadpo check`. Choices in area 6's
  "Several outside calls" and area 12's "How a run is kept".
- [x] **7.5 Multi-step calls: what `jadpo check` keeps** `opus · xhigh`
  (areas 3, 11 and 12, decided 2026-10-16, with the edge-case review's ten
  fixes; split on 2026-10-20 into this, 7.5b and 7.5c): no outside call
  inside `for` in any action; up to 8 changing calls on one way through an
  action, counted through the actions it calls; a changing call with no
  key the only changing call in its action; several changing calls only
  where the action's key survives a retry (a subscriber, or a route whose
  API requires `Idempotency-Key`), never from a schedule or a batch's
  item; no `large_output: true` operation among them; and a store for
  their record. Built: `types::action_steps` works out, once per project,
  the changing calls (a provider operation with `changes: true`, a
  store's `put`) on the longest way through each action, each way through
  an `if` or a `match` on its own and an action called adding its own;
  `step_problems` reports `effect.call_in_loop` (area 3), `step.too_many`,
  `step.unkeyed`, `step.large_output`, `step.context` (at the schedule,
  the route or the run's call) and `step.store_required` (area 12), none
  declinable; an operation's `large_output:` is read. Tests in
  `crates/jadpo-check/tests/steps.rs`; examples and errors on areas 3 and
  12. Choices in area 3's "Outside calls in loops, checked" and area 12's
  "Several changing calls, checked"; area 11's 256 KB answer cap corrected
  to area 12's 1 MB.
- [x] **7.5b Multi-step calls: the record** `opus · xhigh`, after 7.5
  (split from 7.5 on 2026-10-20): each step recorded before it is sent
  and its answer after, in the runtime's own short transactions in the
  store 7.5 names, extending `effects::exchange` and the provider call
  rather than a parallel path; a rerun (a retry, a redelivery, area 6's
  rerun) replaying answered steps and checking the whole path, `diverged`
  held for an operator; `clock.now` fixed per record; a step in doubt
  sent again under its key within the package's `window:`, parked after
  it, on no key or on a replayed `5xx`; the provider's `in_use` waited
  for and `reused` diverged; a delivery's lease generation checked by
  every record write; the record deleted in the commit, with the build id
  and the step shape it started under; answers encrypted with `application {
  steps: { encryption_key: } }` (area 12, decided 2026-10-20). Built:
  `jadpo_step_records` and `jadpo_steps` in the store such an action
  writes; the plan's `RecordPlan` (store, step-shape digest), steps key
  and build digest; `steps_settings` with `step.encryption_key_required`
  and `step.settings_invalid`; the provider plan reads `window:`,
  `replays_failures:`, `new_key_on:` and `changes:`. The runtime's
  `steps` module opens the record at an outermost boundary whose runner
  set its generation (a delivery's attempt, a route's claim under a
  required `Idempotency-Key`), fixes the clock, and checks each step
  (operation, an HMAC of each request field) before replaying its sealed
  answer or sending it, `exchange` writing the step first; a `put`'s
  object keyed from its step; `FaultKind::Diverged` and the delivery
  state `diverged`; the record deleted in `seal`. Live tests in
  `step_records.rs`: replay with the first run's time, a changed request
  diverging and finishing once put back, the window, a replayed `5xx`,
  `in_use` and `reused`, an older generation writing nothing, an answer
  past 1 MB, area 6's rerun, a route's client retry, and a recorded step
  never reached. Choices in area 12's "The record, chosen in chunk 7.5b".
- [x] **7.5c Multi-step calls: operators** `opus · high`, after 7.5b
  (split from 7.5 on 2026-10-20, and again into this and 7.5d): an
  unfinished record as an operator item (`jadpo deliveries show`, `retry
  --not-done` under the next key generation, `abandon`, a route's key
  lapsing), kept 30 days, with 7.5b's `diverged` deliveries listed and
  retried, and the provider's `new_key_on:` sending under the next key
  generation; `jadpo runs unfinished --build`. Built: the runtime's
  `unfinished` module reads a record's steps (never a request, answer or
  key) and changes it beside its delivery, in one statement with the log
  row in the same store; `deliveries` gains `diverged`, `unfinished` (a
  route's record a day after its last run) and `abandoned`, the actions
  `retry_not_done` and `abandon`, and a retry resetting the record's
  generation; the tick drops answers after 30 days and deletes abandoned
  records and deliveries 30 days on; an abandoned record refuses a rerun;
  `steps::new_key` and the provider call's resend on `new_key_on:`;
  `jadpo deliveries show` and `abandon`, `retry --not-done`, and the
  `jadpo runs unfinished` command (`cli.runs_arguments`,
  `runs.unavailable`). Live tests in `step_operators.rs`. Choices in area
  12's "Operators, chosen in chunk 7.5c".
- [x] **7.5d Multi-step calls: fakes and crash points** `opus · high`,
  after 7.5c (split from 7.5c on 2026-10-20, and again into this, 7.5e
  and 7.5f): fakes keeping keys with `lost` and `in_use`; `jadpo test`'s
  crash at each step boundary. Built: a provider call carries the key it
  goes under (`capability::SentKey`), and `fakes::KeptKeys` keeps each
  key's first answer for the test's fake and `jadpo dev`'s, `lost` keeping
  what it did; the runtime's `crashes` module counts the steps a test's
  run reaches under a record and crashes it once at a step's boundary
  (`FaultKind::Crashed`), a crashed delivery given back and a route's
  request sent again under its key; `jadpo test` reruns a passing test
  that reached steps once per boundary per step, reporting the first
  that fails with its `crash` line (and `--json`'s `crash`), and a
  difference in the fake's effects as a fault; a diverged delivery between
  a test's steps ends it as `test.subscriber_failed`; each delivery attempt
  in a test counts its keyed calls from none under its own key. Live tests
  in `step_crashes.rs`. Choices in area 12's "Tests, chosen in chunk 7.5d".
- [x] **7.5e Multi-step calls: the late-failure warning** `opus · high`,
  after 7.5d (split from 7.5d on 2026-10-20): the late-failure warning (a
  declinable error) after the first changing step, reaching through
  package actions, and the deadline sum: the timeout times the attempts of
  every step on the longest path against a reaching route's deadline,
  with the single call's deadline warning (area 11) it sums. Built: the
  steps walk keeps each way's first changing call and how long its
  outside calls can take; `step.late_failure` (declinable) once for each
  action at its first `attempt`, `reject` or `propagate` after one that
  can end it for good (a failure not `unavailable` or `rate_limited`),
  unless a `match` handles it; a package's action lists them on its
  surface as `after_effect` (and `jadpo package show`'s "after effect"),
  and the app's call of it is the point; `route.calls_past_deadline` at a
  route whose action's provider and `ai.generate` calls, each `timeout:`
  times its attempts, add up on the longest way past its `deadline:`,
  the one check chunk 5.5v6's streaming routes rely on. Example 26's
  route waits 20 seconds. Choices in area 12's "The late-failure warning
  and the deadline sum, chosen in chunk 7.5e".
- [x] **7.5f Multi-step calls: development and the metric** `opus ·
  high`, after 7.5d (split from 7.5d on 2026-10-20): each step's state in
  `jadpo dev`'s request lines and timeline; the metric and the no-setup
  alert on unfinished records (their number and the oldest's age). Each
  step's state in `jadpo watch` comes with `jadpo watch` (after the first
  build, item 6), and the records' entry in the personal-data report with
  the report itself (area 15, after the first build, item 1). Built: the
  runtime's `steps` module notes what became of each step a run reached
  (`StepState`: replayed, answered, failed by its code, in doubt,
  diverged, outcome unknown) and keeps it on the invocation as the record
  closes (`RunSteps`), a response carrying its own and each call logged
  with its step (`Logged::step`); a delivery's attempt with steps is
  handed to the development call log (`CallLog::delivered`); `jadpo dev`
  shows each step under its request's line in place of its call, and a
  delivery with steps as a line of its own with `jadpo deliveries show`
  when it needs an operator, both in `--json`. The `unfinished` module's
  `Metric` (records not abandoned, the oldest's start) is carried by the
  tick for each store, which raises `unfinished_steps` once a record has
  been unfinished a day, once until none is (`jadpo_step_records.
  alerted_at`); `jadpo deliveries` shows it and its `--json` the metric.
  Until area 17's timelines, a request's line and its trace are its
  timeline. Live tests in `step_development.rs`. Choices in area 12's
  "Development and the metric, chosen in chunk 7.5f".
- [x] **7.6 What package code may do: actions and recipes** `opus ·
  high` (split on 2026-10-20 into this and 7.6b; public and non-public
  package events came with 7.7). Built: a package's action calls its own
  operations, public or not, by their bare names (`customers.create`),
  their failures passed on as its own; the app calls a `public` one as
  `connection.action(input)` from a listed domain, its failures mapped at
  the call by their codes (`Callee::PackageAction`, run by the interpreter
  with its operations going through that connection); its changing calls
  count as the caller's steps, at check time and in the keys the runtime
  derives, and a package action over 8 is `step.too_many` where it is
  declared, so `jadpo package check` refuses it; `package.action_invalid`
  for a package action calling another action, a `public` one that emits
  and an action named as a resource; public actions on the surface; a
  test's `fakes:` scripting the package's own operations; and `jadpo
  package recipe <package> <recipe> --into <domain>`, all or nothing.
  Tests in `crates/jadpo-check/tests/package_actions.rs` and live ones in
  `crates/jadpo-cli/tests/package_actions.rs` and `package_recipes.rs`;
  example 26 calls a package action. Choices in area 11's "Chosen in
  chunk 7.6".
- [x] **7.6b Package subscribers and recipes in `sample/`** `opus ·
  high`, after 7.6 (split from 7.6 on 2026-10-20, and again on
  2026-10-20: polls and `max_concurrent:` are 7.6c). Built: a package's
  subscribers on its own webhooks' events, public or not
  (`subscriber.event_invalid` for any other), each heard once per
  connection receiving them (`subscriber AcmePay.renewal payments`) and
  running one of the package's actions with its operations going through
  that connection; a package action without `public` emitting the
  package's own events, declared without `wire_name:`, kept in the
  outbox where the webhooks are and delivered to the app's subscribers
  of its public ones (`package.action_invalid` for emitting a webhook's
  event); and `jadpo package check`'s `recipes` row, each recipe compiled
  in the one domain of the package's `sample/` app, shown at its own path
  (`publish.sample_missing` without such an app). Tests in
  `crates/jadpo-check/tests/package_subscribers.rs`, live ones in
  `crates/jadpo-cli/tests/package_subscribers.rs`, and the bar's in
  `package_recipes.rs`. Choices in area 11's "Chosen in chunk 7.6b".
- [x] **7.6c Polls and `max_concurrent:`** `opus · high`, after 7.6b
  (split from 7.6b on 2026-10-20). Built: `polls:` in `provider.jadpo`
  (`call:` an operation that takes and changes nothing, `every:` a
  minute or more, `emits:` a public event of the package's own made from
  the answer's fields) and the connection's `polls: { <name>: <interval>
  }`, each connection's poll run by the tick only while the app
  subscribes to its event, claimed by one instance, what it last saw kept
  in `jadpo_polls` beside the outbox and its event kept with the new
  digest in one transaction only when the answer changed, a failed call
  tried again a minute later; and `max_concurrent:` in `provider.jadpo`,
  lowered by a connection, a slot per call attempt per app instance
  (further calls wait, a delivery that would wait given back to the
  outbox uncounted). Tests in `crates/jadpo-check/tests/package_polls.rs`,
  live ones in `crates/jadpo-cli/tests/package_polls.rs`. Choices in
  area 11's "Chosen in chunk 7.6c".
- [x] **7.7 Webhooks as events** `opus · high` (split from 7.7 Receiving
  from outside on 2026-10-20: no webhook was received before it, so
  replies and handshakes had nothing to build on). Built: a provider's
  `webhooks:` (`check:` one of `SignatureScheme.stripe`,
  `.standard_webhooks`, `.hmac_sha256` with `header:` and `.basic`;
  `event_id:`, `event_type:` and `occurred_at:` places in the body), a
  package's `events/` read (each with its `wire_name:` and `occurred_at:
  Instant`) and subscribed to by the app, a connection's `webhooks: {
  secret: }`, and `POST <prefix>/<connection>` in the `webhooks` API
  (`/webhooks` when none is declared; `jadpo routes`, and the declared
  API's record and OpenAPI document), answered by the runtime: the
  signature checked over the raw body with a five-minute tolerance, the
  body read with the event's codec on the provider's wire, `occurred_at`
  filled from the provider's time, the provider's id recorded in
  `jadpo_received_events` with the outbox row and deliveries in one
  transaction, a repeat answered and dropped, and the ids deleted by the
  tick after 30 days. Live tests in `webhooks.rs`. Choices in area 11's
  "Chosen in chunk 7.7".
- [x] **7.7b Webhook replies and handshakes** `opus · high`, after 7.7.
  Built: `reply:` on a package's event (an object of its own `types/`)
  and the connection's `webhooks: { replies: }`, the answering action run
  as the system inside the request after the signature check, its reply
  kept with the event's id in `jadpo_received_events` so a repeat gets it,
  and the event never kept for subscribers; `handshake:`
  (`WebhookHandshake.sns`, `.slack`, `.microsoft_graph`); Twilio's
  HMAC-SHA1 over a form body, Slack's scheme (`SignatureScheme.slack`),
  and SNS's certificates, fetched only from AWS's SNS hosts through the
  host's outbound guard (HTTPS, no redirect, no private address) and kept
  a day, the connection's secret the topic's ARN. Live tests in
  `webhook_replies.rs`, SNS's against a local certificate and server.
  Choices in area 11's "Chosen in chunk 7.7b".
- [x] **7.7c Queues through `receiving`** `opus · high` (split on
  2026-10-20: the Cloudflare Queues driver is 7.7c2). Built: `receiving:`
  in `application { }` (`type_at:`, `id_at:`, `events:` named as code
  names them, each with a text `wire_name:` when the queue has types;
  `receiving.invalid`), a package's `provides/receiving.jadpo` mapping
  `receive:` and `acknowledge:` onto its own operations as data, and the
  tick fetching each queue through its connection, at least every 10
  seconds: each message kept once as its event, its id in
  `jadpo_received_events` with its outbox row, and only then deleted from
  the queue; an unlisted type, an unreadable message or one over 64 KB
  parked whole in `jadpo_parked_messages` and listed, retried through the
  next tick and dismissed by `jadpo deliveries`; `Protocol.aws_json`,
  `aws: { service: target: }` and `CredentialScheme.aws_sigv4` with a
  `key_id:` and `secret:` credential, checked against AWS's published
  test suite; a `T?` member a provider leaves out read as `none`; and the
  SQS package in `packages/amazon_sqs`, tested against a local fake of
  SQS's JSON API that checks every signature (`receiving.rs`). Choices in
  area 11's "Chosen in chunk 7.7c".
- [x] **7.7c2 Cloudflare Queues on Workers** `opus · high`. Built:
  `driver: Driver.cloudflare_queues` with `queue:` and a required, different
  `dead_letter_queue:` (`connection.key_missing`, `.key_invalid`,
  `.value_invalid`), read by `receiving:` as a package's connection is;
  the seam the Workers consumer will call, `receiving::consume`, keeping
  each message of a batch as 7.7c keeps one and answering each:
  acknowledged once kept, as a repeat or parked, retried after a delay
  growing per attempt when the store can't take it now, a batch from an
  unknown queue retried whole; the tick fetching nothing from it but
  rereading retried parked messages; and its consumer for the Wrangler
  file (`wrangler_queues`, as JSON). Tested in-process against PostgreSQL
  (`cloudflare_queues.rs`); deploying it is Phase 8's. Choices in area
  11's "Chosen in chunk 7.7c2".
- [x] **5.5v3 The `ai` capability: `generate`, its fake and the local
  driver** `opus · xhigh` (area 21, "The `ai` capability" and "Typed
  output"; split on 2026-10-20: a package filling `ai` is 5.5v3b).
  Built: `ai:` in `application { }` naming a `Driver.openai_compatible`
  connection (`url:`, plain HTTP only to this machine) and its models,
  each a `ModelId` setting, read from its text (`ai.settings_invalid`,
  `ai.connection_required`), and a `ModelId` written in code refused
  (`semantic.ai_model_literal`); `ai.generate(prompt, as: T)` with
  `model:`, `max_output:`, `timeout:` and `attempts:` written out, `as:`
  refusing a reference, a file, bytes and display text
  (`semantic.ai_output_type`) and secrets, each of its five failures
  mapped to an `unavailable` failure and `malformed`'s carrying the last
  `DecodeProblem`; at run time (`jadpo_runtime::ai`) the prompt
  rendered and sent as OpenAI's Chat Completions request through the
  package driver's HTTP, `T`'s OpenAPI schema in the strict structured
  mode (a `T` that isn't an object wrapped as `value`; the schema in the
  instructions built for 5.5v3b's packages), the answer decoded by
  `json.read`'s decoding, one repair with the decode problem, the five
  failures read from the answer, and answers reused on area 6's rerun by
  step and request and kept as steps of a 7.5b record; the `ai` fake in
  every test (`fakes: { ai.<prompt>: }` with values, `raw(...)` and the
  five failures, one answered attempt each, `ai.<prompt>.requests`,
  generated values, `test.fake_exhausted` and `test.fake_unused`);
  `jadpo dev` asking a model on this machine, each attempt in `jadpo
  calls`; and the network report's row. Tests in `ai_generate.rs` (every
  failure and a repair through the fake in `jadpo test`; `jadpo dev`
  against an OpenAI-compatible server started in the test) and the live
  `ai_steps.rs` (a model's answer replayed from a record). Choices in
  area 21's "Chosen in chunk 5.5v3".
- [x] **5.5v3b Providers filling `ai`** `opus · high` (area 21, "How a
  provider fills it"), after 5.5v3 (split from 5.5v3 on 2026-10-20).
  Built: a provider package's `provides/ai.jadpo`, `provide ai {
  generate: models: }` (`jadpo_check::ai_mapping`), read as data and
  checked against the package's operation, settings and libraries
  (`package.provider_invalid`; `stream:` and `embed:`
  `toolchain.not_built_yet`): `call:` an operation that changes nothing,
  `input:` from `request.model` (the name after the provider),
  `request.max_output`, `request.instructions`, `{ each: request.messages
  as: { } }`, `settings.*` and values, `answer:` and `refused_when:` read
  from the decoded output, `cut_off_when:` from the answer as sent,
  `usage:`, `schema: { mode: at: }` with OpenAI's envelope written at
  `at:`, `models: { context: }` a library constant; `ai: { connection: }`
  naming a package's connection (`ai.settings_invalid` when it has no
  mapping); at run time the request made through the package driver,
  `AiSchema.json_only` and `.none` sending the schema in the
  instructions, `too_long` from a four-bytes-a-token estimate against the
  context table before sending; the `ai` fake ahead of the package's in
  tests; `jadpo dev` faking it with generated values,
  `development: { sandbox: [<prompt>] }` letting prompts through to the
  sandbox, and `jadpo calls script ai.<prompt> <condition>`. Tests in
  `crates/jadpo-check/tests/ai_providers.rs` and
  `crates/jadpo-cli/tests/ai_providers.rs` (`jadpo test` through the
  fake; `jadpo dev` faked, scripted, and through a sandbox started in the
  test). Choices in area 21's "Chosen in chunk 5.5v3b".
- [x] **5.5v4 Embeddings and recordings** `opus · high` (area 21; split
  on 2026-10-20: `jadpo ai record` is 5.5v4b). Built: `ai.embed(text, as:
  V)` and `ai.embed_all(texts, as: V)` as outside calls only an action
  makes, of a `Text` or `Markdown` (or a list of up to 100), asking the
  `embedding` model unless `model:` names another, with `timeout:` (15s)
  and `timed_out`, `over_budget` and `too_long` each mapped to an
  `unavailable` failure; `embedding_dimensions:` in `ai:` and
  `semantic.ai_dimensions_mismatch` for a `V` of another length or none
  given; the built-in driver's OpenAI embeddings request
  (`jadpo_runtime::embed`), each vector read as `V`, another length
  `misconfigured`, answers reused on a rerun and kept as steps; the fake's
  embedding from the text's SHA-256 and the test's seed, and `fakes: {
  ai.embed: }` scripting its failures; recordings of a prompt's answer,
  `prompts/recordings/<prompt>/<name>.json` with `answer` (plain values
  as placeholders) and `version`, `recorded("<name>")` in a test's
  `fakes:`, every recording decoded by `jadpo test` as a `recording`
  test (`test.recording_undecodable`), and one made from an older prompt
  an alert on the report and in `data.alerts`. Tests in `ai_embed.rs`
  (the checker's, and the CLI's: the fake, `jadpo dev` against an
  OpenAI-compatible server started in the test, and the support desk end
  to end in PostgreSQL: embed, nearest query, typed answer from a
  recording). Choices in area 21's "Chosen in chunk 5.5v4".
- [x] **5.5v4b `jadpo ai record`** `opus · high` (area 21, "Tests"; split
  from 5.5v4 on 2026-10-20). Built: `jadpo ai record <prompt> --as
  <name>`, asking the configured model once as the prompt's first
  `ai.generate` call asks, with the values of its last `jadpo dev` call
  (the call log now keeps a prompt's values) or `--fixture <fixture>`'s,
  and keeping the answer as 5.5v4's recording, its plain values
  placeholders by area 11's code, with the prompt's version; through a
  package's connection refused when live or unset by `jadpo package
  record`'s own refusals (`recording.refused`), and nothing kept on any
  condition (`recording.failed`); the stale recording's alert naming it
  as the next step, in the hint and `data.alerts[].next`. Tests in
  `crates/jadpo-cli/tests/ai_record.rs`, against an OpenAI-compatible
  server started in the test: a recording made from a `jadpo dev` call
  answers a test through `recorded(...)`. Choices in area 21's "Chosen in
  chunk 5.5v4b".
- [x] **5.5v5 Budgets** `opus · high` (area 21, "Budgets"), after 5.5v3b
  (split on 2026-10-20: personal data to models is 5.5v5b). Built:
  `ai: { limits: }` with `per_request:`, `per_day: { tokens: cost: }`
  (required, `semantic.ai_budget_missing`), `per_caller_per_day:` and
  `store:` (`jadpo_check::ai_limits`); a `Money` setting read from
  `"GBP 20.00"`; `jadpo_ai_usage` in the main store, one row per day and
  window (the app's, each caller's digest, each prompt and model's), the
  tick deleting days past 62; before sending, the call's most possible use
  (its prompt's estimate and `max_output:`, times `attempts:`) checked
  against the request's count and taken from the day's rows by one
  conditional statement each, `over_budget` with nothing sent when a
  limit can't cover it, `unavailable` when the store can't be reached;
  after, charged the answer's reported usage or the estimate
  (`jadpo_runtime::ai_budget`); `models: { prices: }` read and the cost
  estimated in millionths of the price's currency, another currency than
  the limit's `misconfigured`; every call counted in tests, `jadpo dev`
  (the development daily limit on sandboxed prompts) and production; and
  `jadpo ai usage` (`ai.usage_unavailable`). Tests in
  `crates/jadpo-check/tests/ai_generate.rs` and `ai_providers.rs`, and
  `crates/jadpo-cli/tests/ai_budgets.rs` (each limit exhausted in `jadpo
  test`; a caller's day and a sandboxed prompt's reported usage in `jadpo
  dev`, and `jadpo ai usage`). A setting overriding a price moved to
  5.5v7. Choices in area 21's "Chosen in chunk 5.5v5".
- [ ] **5.5v5b Personal data to models** `opus · high` (area 21, "Personal
  data sent to a model"), after 5.5v5 (split from 5.5v5 on 2026-10-20):
  `personal_data: { allowed: }` with the flow check from placeholders back
  to marked fields (`personal:` read on fields for it, and the standard
  types' own categories), `to_models: false`. Done when a marked field
  reaching a prompt unlisted fails to compile. The prompts' entry in the
  personal-data report comes with the report itself (area 15, after the
  first build, item 1).
- [x] **5.5v6 Streaming to a route** `opus · high` (areas 7 and 21), after
  5.5v3: server-sent events designed on area 7's page, a route returning
  an `AiStream`, the final and error events, the stream fake. The check
  that a route's deadline leaves room for the `timeout:` of each `ai` call
  it reaches (left from 5.5v3) moved to chunk 7.5e, which builds one
  deadline sum over every outside call a route reaches; an `ai.stream`
  call counts its `timeout:` once. Done
  when a route test reads a scripted stream piece by piece and a failure
  after the first piece arrives as an error event.
  Built: `ai.stream(prompt)` with `model:`, `max_output:` and `timeout:`,
  only in an action, giving an `AiStream`, its four conditions before the
  first piece mapped as `generate`'s; OpenAI's streamed chat completion
  read piece by piece (`jadpo_runtime::stream`), the open answer kept on
  the invocation and given back on a rerun; a route with `output:
  AiStream` answering `text/event-stream` once its action commits:
  `piece` events, then `done`, or `error` (`unavailable`) for anything
  after the first piece, ended by the call's `timeout:` or the route's
  deadline; no `id:` or `retry:`; only `.ok` and no `settle:`
  (`route.stream_invalid`); never kept for a repeat; OpenAPI's
  `text/event-stream` with `x-jadpo-stream` and the record's `output
  AiStream (server-sent events)`; a route test's `response.pieces`; the
  fake's text cut into pieces of three words and `partial("...",
  <condition>)`; natively, each event sent as it is read and the
  provider's connection closed when the client goes away. Tests in
  `ai_stream.rs` (the checker's, and the CLI's: the route tests, and
  `jadpo dev` streaming from a server started in the test). Choices in
  area 7's "Streaming a model's answer" and area 21's "Chosen in chunk
  5.5v6".
- [ ] **5.5v7 OpenAI and Anthropic packages** `opus · high` (area 21),
  after 5.5v3b, 5.5v4b and 5.5v6: a package's `provide ai { embed: }`
  (`toolchain.not_built_yet` since 5.5v3b; 5.5v4 built `ai.embed` through
  the built-in driver alone), and two provider packages filling `ai`, each
  with its context and price tables, a setting overriding a price (moved
  from 5.5v5), recordings and `jadpo package conform
  <package> ai`, and a package's `provide ai { stream: }` mapping (5.5v6
  streams through the built-in driver only). **Needs Richard** for the API keys the live conformance run uses.
- [x] **7.7d Fixes found in Phase 7** `opus · high` (found in chunks
  7.7c2 and 5.5v3): a served OpenAPI schema keeps a field's inline rules
  (`reply: Text { max_length: 2000 }` gives `maxLength`, and the same for
  the `ai` schema); and the two tests that failed once and passed on
  rerun made deterministic: `http::an_api_token_signs_its_app_in_until_it_is_ended`
  (compares timestamps as text) and
  `batch_calls::a_files_rows_that_call_are_each_a_batch_of_their_own`.
  Built: every field's schema, parameter and record line reads the field
  itself (`wire::field_type`), so its inline rules come before its
  type's, in OpenAPI, the API record and `ai.generate`'s schema alike;
  the token test compares instants as instants, and the open-transaction
  count of the call tests leaves out the server's autovacuum workers.
- [x] **7.7e Follow-ups from 7.5e** `opus · high` (found in chunk
  7.5e): new default timeouts (Richard, 2026-10-20, option A), since at
  the old ones the route-deadline sum refused almost every route that
  calls out: a provider connection's `timeout:` 3s (3 attempts, 9s, inside
  a route's 10s default deadline) and `ai.generate`'s 25s (2 attempts,
  50s, so a route that asks a model writes `deadline:` up to 60s), the
  `deadline:` and `timeout:` lines 7.5e added for the old ones removed;
  `ai.stream`'s `timeout:` counted once in the sum (`Calls::time`); and a
  package's failure has one name inside its own code, so a `match` on its
  own operation's outcome takes the bare `CardDeclined`. Also under the
  decision: `ai.embed` and `ai.embed_all` counted once in the sum, their
  default `timeout:` 5s, so a search route embedding its query fits the
  10s default.
- [ ] **7.8 Teaching examples for Phase 7** `opus · high`: packages,
  capabilities, email, sign-in, multi-step calls and receiving, passing
  `--require-examples`.

## Phase 8 — Compiled deployables

- [ ] **8.1 Native:** `opus · xhigh` graph to Rust; every test passes interpreted and
  compiled. Includes TLS for database connections (a hook on the driver's
  `Transport`, rustls natively), and an outcome and effect record (SQL, row
  counts, events) both executors write, for the differential comparison.
  From the Phase 4 review, decided in §2.5 first: a typed per-route entry
  so generated code decodes a request straight into its structs; plan data
  embedded as constants rather than rebuilt at start; a route trie and no
  per-request allocation for paths, labels and failure lookups; the native
  host accepting asynchronously and spreading connections round-robin,
  with one rate-limit count per caller across its worker threads (today
  each thread counts its own, so a caller on several gets several
  allowances).
  From the Phase 6 review: one `RuntimePlan` with a canonical byte
  encoding behind a smaller `Executor` (places, plan, run); 6.9's
  deployable handler; the corpus compiled as one generated crate, in its
  own CI job with a time budget.
- [ ] **8.2 WebAssembly on Workers:** `opus · xhigh` the cron tick, outbox delivery after
  commit, PostgreSQL through Cloudflare's pooling; the client's address from
  `CF-Connecting-IP` (`ClientAddress::read` with `Target::Workers`, built in
  5.1c), and a `client_address:` source other than `cloudflare` refused when
  a project is built for Workers (area 10, "Client addresses"), which needs
  deployables declared first.
  From chunk 5.3: a deployable carries locale data only for its
  application's supported locales (today all 533 are compiled in: about
  3.5 MB of a 3.9 MB module, 1.2 MB gzipped).
  From the Phase 6 review: the `trigger:` adapter; an `every:` under a
  minute an error naming the schedule (area 12, decided 2026-10-19); a
  batch carried on in a fresh invocation; 6.7's budgets set to Workers'
  (30 s for `waitUntil`, 15 minutes' wall time for the cron).
  From chunk 7.7c2: the queue handler calling `receiving::consume` and
  answering each message (`ack()`, `retry({ delaySeconds })`), the kept
  events' deliveries in `waitUntil`; `wrangler_queues` written into the
  Wrangler file, its setting names and Jadpo's queue-name form checked
  against Wrangler's own; `Driver.cloudflare_queues` refused in a native
  deployable.
- [ ] **8.3 Performance spike** `opus · high`: one route compiled to native and to
  WebAssembly and measured against an equivalent hand-written Bun and
  TypeScript app, comparing Wasm with native and Bun with native as well
  as Wasm with Bun. Moved from an earlier chunk 4.5, which needed Phase 8's compiled code
  (Richard, 2026-10-14). **Needs Richard** if Wasm is more than 25% behind
  Bun.

## Phase 9 — Performance

Richard, 2026-10-13: developers won't touch this if it's slower.

- [ ] **9.1 Benchmarks:** `opus · high` the golden todo plus focused workloads (reads,
  writes, signed-in lists, large rows, cold start) on native, Wasm under
  wasmtime and on Workers, against an equivalent Bun and TypeScript app and
  Node; results published with the method.
  From the Phase 6 review: deliveries swept concurrently within each
  sweep, with a bound, and a backlog of 10,000 deliveries among the
  workloads.
- [ ] **9.2 Optimise WebAssembly** `opus · xhigh` until it matches the Bun app on throughput
  and p95, or Richard accepts a measured gap. The earlier experiments reached
  about 79% of Bun's throughput and list what to try next
  ([optimisation results](experiments/wasm-optimization-results.md)).

## Phase 10 — Golden todo

- [ ] **10.1 The golden todo end to end** `opus · high` on both targets, rewritten in the
  decided language, its contract cases passing. One independent review of
  the whole first build.
- [ ] **10.2 How easily agents write Jadpo** `opus · high`: the same 10–20
  realistic changes to one app (the golden todo, with its TypeScript
  baseline and adversarial changes) done by fresh agent sessions in Jadpo
  and in TypeScript, several runs each, with hidden acceptance tests.
  Measures correctness, attempts and tokens to green, where each mistake
  was caught (check, test or escaped), how easily a person reviews the
  diff, and how often a tempting unsafe shortcut gets through. The agent
  gets the language pages, since no model has seen Jadpo (Richard,
  2026-10-14). The method is designed at the start of the chunk; its
  results steer the order of the work after the first build.

## After the first build, in order

1. Personal data (area 15), with encrypted fields and keys held as data
   (area 19, questions 10 and 21) and `crypto.verify_totp`, which waits on
   them.
2. Remaining built-in services (area 16): flags, files, audit, search,
   telemetry thresholds.
3. Debugging and review (area 17): `why`, timelines, `test new --from`,
   `call`.
4. Records and acceptances (area 18): the approval report, acceptance
   matching, package upgrade plans.
5. Tooling completeness (area 14): interactive tests, `rename` (refusing a
   name already in use, 2026-10-16), `impact`,
   full completion.
6. Packages and capabilities after the first build (decided 2026-10-16):
   services run by Jadpo (`runs:`, Docker or Podman hidden behind
   `jadpo services`); the `documents` capability; OAuth grants per owner;
   search, `bot_check` and `scanning` capabilities; the disk driver;
   telemetry's `telemetry:` section, custom metrics and the local
   `jadpo traces`, `metrics` and `logs`; `jadpo watch`; Google and other
   sign-in providers; publishing packages (`jadpo package new`, `check`,
   `publish`, git-tag versions, publisher trust); packages from other Jadpo
   apps and the cross-app break check; choosing a provider when deploying,
   when an app other people run needs it.
7. Product work: the developer console and MCP (E11), the graph review UI
   (E06, Revset-inspired), the public package registry, acting as an OAuth
   provider, D1 and row-level security.

Older epics (E03–E12) are described in the
[archived roadmap](archive/implementation-roadmap-2026-10-11.md) and are
replanned against this list when reached.

## How we work

- Branch from `main` for each chunk; commit and push often; merge through a
  pull request. Never put a worktree under `/private/tmp`.
- `tools/verify.py` output and the tests are the evidence. No evidence
  packets, per-step reviews, timing logs or plan documents.
- One independent review per phase, not per chunk, by two reviewers:
  soundness, and architecture for the phases ahead.
- Update this page when a chunk finishes or the order changes, and nothing
  else.
