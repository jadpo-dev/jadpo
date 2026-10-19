# Jadpo roadmap

**Updated:** 2026-10-19. This page is the only list of outstanding work: the
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

- [ ] **5.12 Leftovers from consolidation** `opus · high`: `Bic`'s
  country checked against `Country` as decided; the code comments that
  name deleted "Chosen in chunk" notes repointed to their sections; area
  14's two record examples shown in format 2; area 12's open question 16
  marked superseded by its 2026-10-19 decision; and the notes Phase 6
  added (6.1a onwards) folded once Phase 6's chunks have merged.

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
- [ ] **6.1c Event records and stored values** `opus · high`, after 6.1b
  (area 12, "Event records", decided 2026-10-19): event shapes recorded
  and diffed in the generated record exactly as an API's is (chunk 7.0's
  format, a new record kind with its header, diff and `jadpo records`
  row), acceptance waiting for area 18 as the API record's does; and from
  chunk 5.1d, what a subscriber stores from an event checked against what
  every `emit` of it put there, as area 9's stored values are (an `emit`
  itself stores nothing a caller reads).
- [x] **6.2 Schedules:** `opus · high` (area 12, "A schedule and
  tenants", and area 9's "Background work and `Access.public`", decided
  2026-10-19: a schedule reaches every tenant, and reaching what only
  `Access.public` grants is a compile error) leases, the native tick, schedule monitoring basics,
  and the runtime's own sweeps, starting with deleting expired idempotency
  keys (area 7). Split on 2026-10-19 because it doesn't fit one session:
  schedules, their leases, the tick, its sweeps and run records here;
  the commands that watch them are 6.2b. Built: schedules read and
  checked (`schedule.` errors), `schedules: { store: }` defaulting to the
  only database (`schedule.store_required` with several),
  `policy.system_grant_missing` for schedules; `jadpo_schedules` and
  `jadpo_schedule_runs`; `jadpo_runtime::schedules::tick`, taking each
  due schedule's lease in one statement, catching up missed ticks as one
  run on the same grid, skipping and recording a leased or paused one,
  running it as the system with no tenant under the tick's clock in a
  new trace, recording every run; the native timer on the server's
  first worker, which `jadpo dev` ticks with; the sweeps on each tick
  (`runner::sweep`, expired idempotency keys, delivered events) and
  `schedules::HOOKS`, which runs 6.4's `batches::resume`; a live test of all of it.
  Choices in area 12's "Chosen in chunk 6.2"; flagged there: when the
  first run is due, the next due time as a failed run's retry, and an
  `every:` under a minute on Workers.
- [ ] **6.2b Watching schedules** `opus · high`, after 6.2 (area 12,
  "Watching schedules"; area 14): `jadpo schedules` with `run`, `pause`,
  `resume` and `history`, `jadpo why <schedule>`, a schedule's runs in
  `jadpo dev`'s output, the metrics and the alerts with no setup, read
  from 6.2's run records; area 5's built-in hourly purge schedule on the
  tick; and area 13's schedule tests (`schedule.run()`, `--kind
  schedule`).
- [ ] **6.3 Routes that wait for follow-on work** `opus · high` (area 7,
  "Waiting for follow-on work", 2026-10-15): `settle:` delivers an action's
  events and their follow-ons at once, answers with the named query when
  they settle and with `202` past `within:`.
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
- [ ] **6.5 Teaching examples for Phase 6** `opus · high`: events,
  schedules and batches, passing `--require-examples`.

## Phase 7 — Packages, capabilities and email (areas 11, 16)

The design is area 11 as revised on 2026-10-16, with its research in
[packages and extension points](language/research/packages-and-extension-proposals.md).

- [ ] **7.0b Capability seam and effectful operations** `opus · xhigh`,
  from the Phase 5 review: one runtime entry point for operations with
  effects (JSON with the plan's codec, keyed `crypto`, the clock,
  randomness), called by both executors instead of the interpreter's own
  dispatch, and a capability-driver seam with fakes and recordings that
  7.1's providers, 5.5v3's `ai.generate` and 8.1's native build use.
- [ ] **6.4b Batch file helpers** `opus · xhigh` (areas 12, 16 and 19),
  moved after 7.0b from Phase 6, since it needs a minimal `files`
  capability on that seam: a store's `put` and `read` of a `File` with a
  fake, then `csv.read`/`write`, `tsv`, `json_lines` and the batch forms
  of `json.read`/`write` on 6.4's runs, as area 19's "Data formats" tables
  them, with `ExportSummary`, `CsvFormat`, a file row's `RowProblem`
  column and line numbers, and `store.read`'s whole-read cap; and from
  6.4, the warning for a `from:` ordered by a field that can be updated,
  and `estimated_total` from a query's count.
- [ ] **7.1 Packages and connections** `opus · high`: the package format
  (`package.jadpo`, `provider.jadpo`, folders, `public` and the public
  surface rule), library and provider kinds, `packages/` installed and
  committed, the app's own integrations in `shared/providers/`,
  `jadpo.lock` with its sources (path, git, link) and verification at
  install, reserved names, `surface.jadpo_record` and `jadpo package show`,
  `connections:` and the network report, lazy loading, library packages'
  translations, and the `jadpo package` commands for adding, linking and
  checking, with `type.standard_exists` on packages' public types in the
  publishing bar (area 19, question 1). The public registry stays after the first build.
- [ ] **7.2 Importing, decoding and faking** `opus · high`: `x-jadpo`
  fields in every API's OpenAPI document and each API serving it at
  `<prefix>/openapi.json` (under its own authentication, `serve_openapi:
  false` to turn off, listed in its own OpenAPI and record); the OpenAPI
  importer reading them, hand-written packages, `wire:`,
  `decode:`, `wire_output:`, `output_at:` and `failed_when:`, recordings
  and `jadpo package record`, generated fakes, provider calls faked by
  default in `jadpo dev` with `development: { sandbox: }` and the call log
  (`jadpo calls`), and derived provider idempotency keys (app,
  environment, caller, step), which also close a collision between users; and mapping imported types
  to standard types (area 11, "Mapping imported types to standard types",
  Richard 2026-10-17): exact format mappings, shape and name detection
  decoded at the edge, declinable findings with `(agent)` steps, one review
  instruction scoped to the types the app's operations reach and tracked per
  type in `jadpo check` and `jadpo approval`, and the import's summary.
- [ ] **7.3 Email:** `opus · high` the capability under design C (whole:
  Jadpo-rendered, attachments), the SMTP driver and a mail package as a
  second connection proving the swap, `emails/` templates and the shared
  layout, the dev inbox, the health score. Chunk 5.3b refuses a dotted
  translation key (area 16's old `"welcome_email.subject"`) as
  `translation.key_invalid`; under translations as messages (5.4c) an
  email's subject is its own member in its domain's language files.
- [ ] **7.3b Magic-link sign-in** `opus · high` (area 8, decided
  2026-10-16): `sign_in:` with `magic_link:` and `providers:`, the
  generated endpoints (the email links to the frontend's `link:` path,
  which posts to `POST /auth/magic-link/confirm`), link and six-digit code,
  the wrong-code and send limits, `on_first_sign_in` for every way in, the
  built-in emails' five levels (area 16), the sign-in failures and events.
  Google and other providers stay after the first build.
- [ ] **5.5z2 Passkey recovery** `opus · high` (area 8, "Passkeys"; split
  from 5.5z), after 7.3b and the events of area 12: `passkey:` needs
  `magic_link:` (`semantic.auth_setting_required`), and area 8's passkeys
  example gains it; the device lists a passkey under `magic_link:`'s
  `address:` rather than the principal's subject; `test_passkeys.register`
  and `.sign_in` with `TestPasskey` in route tests; `SignedIn` and
  `SignInRefused` for passkeys, `PasskeyAdded` and `PasskeyRemoved`. Done
  when a person with no passkey left recovers through a magic link and
  adds a new one, in a route test written with `test_passkeys`.
- [ ] **7.4 Outside calls hold no transaction** `opus · xhigh` (areas 6 and
  11, 2026-10-15): reads before a call in a read-only snapshot closed before
  it, the transaction opened after it, a write to a record loaded before the
  call checked by its row version, and a change rerunning the action with
  the call repeated under its key. Done when a test holds a provider call
  open and shows no transaction open meanwhile, and a change made during
  the call reruns the action.
- [ ] **7.5 Multi-step calls** `opus · xhigh` (areas 11 and 12, decided
  2026-10-16, with the edge-case review's ten fixes): several keyed
  changing calls per action where the key survives a retry, each recorded
  before it is sent and replayed on a rerun, the path checked and
  `diverged` stopping for an operator, `clock.now` fixed per record, key
  windows from the package, no outside calls inside `for`, one runner per
  record, unfinished runs as operator items, and the late-failure warning.
- [ ] **7.6 What package code may do** `opus · high`: package actions
  calling only their own API, package subscribers on their own webhooks,
  public and non-public package events, `polls:` run only while
  subscribed, `max_concurrent:`, and recipes with `jadpo package recipe`.
- [ ] **7.7 Receiving from outside** `opus · high`: webhook replies and
  handshakes, `occurred_at`, and the `receiving` capability for queues
  (`type_at`, `id_at`, `events`), with the SQS package and, on Workers,
  the Cloudflare Queues driver.
- [ ] **5.5v3 The `ai` capability: `generate` and its fake** `opus ·
  xhigh` (area 21, "The `ai` capability" and "Typed output"), after
  5.5v2 and 7.3: `ai:` in `application { }` with models as `ModelId`
  settings, `provides/ai.jadpo`, `ai.generate(prompt, as: T)` with the
  schema from `T`, the provider's structured mode or the schema in the
  instructions, `json.read`'s decoding, one repair with the decode
  problem, the five failures, answers reused on a rerun, the fake in
  `jadpo test` and `jadpo dev` (`fakes: { ai.<prompt>: }`, `raw(...)`,
  `.requests`), and `Driver.openai_compatible`. Done when a test drives
  every failure and a repaired answer through the fake, and a local
  model answers through the driver in `jadpo dev`.
- [ ] **5.5v4 Embeddings and recordings** `opus · high` (area 21), after
  5.5v1 and 5.5v3: `ai.embed` and `ai.embed_all`, `embedding_dimensions:`
  and its check, the fake's seeded embeddings, `jadpo ai record` and
  `recorded(...)`, every recording decoded on every build and a changed
  prompt's recordings warned about. Done when the support example on area
  21 runs end to end in a test: embed, nearest query, typed answer from a
  recording.
- [ ] **5.5v5 Budgets and personal data to models** `opus · high` (area
  21, "Safety and privacy"), after 5.5v3: the per-call, per-request,
  per-day and per-caller limits counted in the main store, the cost
  estimate from the package's prices, `jadpo ai usage`, `personal_data: {
  allowed: }` with the flow check from placeholders back to marked
  fields, `to_models: false`, and the prompts in the personal-data report.
  Done when a test exhausts each limit and is refused `over_budget`, and
  a marked field reaching a prompt unlisted fails to compile.
- [ ] **5.5v6 Streaming to a route** `opus · high` (areas 7 and 21), after
  5.5v3: server-sent events designed on area 7's page, a route returning
  an `AiStream`, the final and error events, and the stream fake. Done
  when a route test reads a scripted stream piece by piece and a failure
  after the first piece arrives as an error event.
- [ ] **5.5v7 OpenAI and Anthropic packages** `opus · high` (area 21),
  after 5.5v4 and 5.5v6: two provider packages filling `ai`, each with its
  context and price tables, recordings and `jadpo package conform <package>
  ai`. **Needs Richard** for the API keys the live conformance run uses.
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
- [ ] **8.2 WebAssembly on Workers:** `opus · xhigh` the cron tick, outbox delivery after
  commit, PostgreSQL through Cloudflare's pooling; the client's address from
  `CF-Connecting-IP` (`ClientAddress::read` with `Target::Workers`, built in
  5.1c), and a `client_address:` source other than `cloudflare` refused when
  a project is built for Workers (area 10, "Client addresses"), which needs
  deployables declared first.
  From chunk 5.3: a deployable carries locale data only for its
  application's supported locales (today all 533 are compiled in: about
  3.5 MB of a 3.9 MB module, 1.2 MB gzipped).
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
