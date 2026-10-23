# Finished roadmap chunks

Phases 0 to 7 of the [roadmap](../roadmap.md) as they stood when they
finished, and the full entries of the chunks of later phases that have
finished, moved here by chunk 8.4h (2026-10-22) so the roadmap holds only
open work. What each chunk built is written in
[compiler-runtime.md](../compiler-runtime.md) and the
[language pages](../language/README.md), which win wherever an entry here
disagrees; the entries are kept so that a chunk number in a code comment
or a page can still be looked up, with the numbers measured at the time.

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
  [compiler-runtime.md](../compiler-runtime.md): the crates, the graph and its
  identity, the query engine, the shared executor interface, the runtime's
  boundaries and the gate.

## Phase 1 — Graph core (areas 1–4)

- [x] **1.1 Workspace and CLI shell** `opus · high`. Built: the `jadpo` CLI
  shell in the agreed
  [design language](../language/research/14-cli-design-language.md), with
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
  ([decision](../decision-register.md#examples-teach-every-feature--2026-10-14)).
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
  ([decision](../decision-register.md#errors-for-habits-from-other-languages--2026-10-15)).
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
  [area 19](../language/19-standard-library.md).
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
  plan, decided by Richard as [area 20](../language/20-geospatial.md).
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
  Richard as [area 21](../language/21-ai.md).
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
  [record format](../language/research/jadpo-record-format.md) for every
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
[packages and extension points](../language/research/packages-and-extension-proposals.md).

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
- [x] **7.3c3 Email: SpamAssassin's rules in the health score** `opus ·
  high`, after 7.3c2 (split from it on 2026-10-20): when SpamAssassin is
  installed, the dev inbox's score and `jadpo email show` add its open
  rules (area 16, "A mail health score"), the mail run through it as the
  SMTP driver writes it, its run behind a seam its tests fake. Built:
  each caught mail written by the SMTP driver and handed to
  `spamassassin -L -t -x` on the `PATH` off the worker's thread
  (`jadpo_host_native::spamassassin`), its report kept with the mail
  (`spam`); spam one more problem, its rules after Jadpo's checks in
  `jadpo email show` and the page's health view; not installed, not run
  or no report said as such, never as clean ("Chosen in chunk 7.3c3").
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
- [x] **5.5v5b Personal data to models** `opus · high` (area 21, "Personal
  data sent to a model"), after 5.5v5 (split from 5.5v5 on 2026-10-20).
  Built: `ai: { personal_data: { allowed: } }`, left out allowing none
  (`jadpo_check::ai_personal`); `personal:` and `to_models:` read on an
  entity's and an event's fields (`type.setting_value` when they don't);
  the standard types' own categories; and the flow check, the policy
  walk run again in its own mode (`policy::flow::models`): each field of
  a `Prompt` built and each text `ai.embed` sends followed back through
  variables, calls, text and lists built from it, a query's `Output` and
  an event's payload, a parameter checked where it is passed, a marked
  field read from a value it can't follow counting as that field, what
  decided a value not sent with it; `semantic.ai_personal_data_not_allowed`
  and `semantic.ai_field_refused`, naming the field and its category.
  Tests in `crates/jadpo-check/tests/ai_personal.rs`. The prompts' entry
  in the personal-data report comes with the report itself (area 15,
  after the first build, item 1). Choices in area 21's "Chosen in chunk
  5.5v5b".
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
- [x] **5.5v7 OpenAI and Anthropic packages** `opus · high` (area 21),
  after 5.5v3b, 5.5v4b and 5.5v6 (split on 2026-10-20: `jadpo package
  conform` is 5.5v7b). Built: a package's `provide ai { stream: }` (an
  operation whose server-sent events are read as sent, `pieces:`,
  `refused_when:`, `cut_off_when:`, `done_when:`, whole at `[DONE]`) and
  `provide ai { embed: }` (`request.texts` and `request.dimensions`,
  `vectors:`, `vector:`, `index:`, `batch:`, `usage:`, `too_long` from the
  context table); `refused_when: { holds: path: equals: }`; an answer of
  every list item's text; `schema: { envelope: AiEnvelope.anthropic }`;
  `CredentialScheme.header` with `credential_header:`, and `headers:`;
  `ai: { prices: }` overriding a package's price (moved from 5.5v5);
  and `packages/openai` and `packages/anthropic`, written from the
  providers' own documentation (Anthropic has no embeddings), each with
  its context and price tables read on 2026-10-20 and recordings made by
  `jadpo package record` from documented answers. Tests in
  `crates/jadpo-check/tests/ai_providers.rs` and
  `crates/jadpo-cli/tests/ai_packages.rs` (both packages installed with
  `jadpo package add`, generating, streaming and embedding through `jadpo
  dev` against a server started in the test, faked in `jadpo test`, their
  recordings recorded again and decoded) and `ai_budgets.rs` (a price
  the app sets). Choices in area 21's "Chosen in chunk 5.5v7".
- [x] **5.5v7b `jadpo package conform <package> ai`** `opus · high`
  (area 21, "How a provider fills it"; area 11, "Proving a package
  works"), after 5.5v7 (split from 5.5v7 on 2026-10-20). Built: the `ai`
  suite's six cases (a typed answer, each refusal mark, a cut-off at
  `max_output: 16`, a stream, an embedding of 256 dimensions, skipped for
  a package that maps no `embed:`, and the usage read), compiled as a
  scratch app holding the package as installed with `ai:` through it;
  by default against a local fake on the loopback address answering from
  the package's recordings, marked where its mapping reads them; `--live`
  against the provider's API through the app's own connection, refused
  (`conform.refused`) without one, without its credential or address set,
  or without `ai:` through it, the refusal skipped live; a failed case
  `conform.failed`; the screen after the publishing bar's. Tests in
  `crates/jadpo-cli/tests/package_conform.rs`: both first-party packages
  conform against their recordings (Anthropic skipping its embedding),
  nothing goes to the connection's address without `--live`, the live
  run's refusals, and a recording missing a mark failing its cases.
  Choices in area 21's "Chosen in chunk 5.5v7b". **Needs Richard:** the
  live run, `jadpo package conform openai ai --live` and `jadpo package
  conform anthropic ai --live` in an app connected to each with his keys,
  was not made here.
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
- [x] **7.7f The docs renderer's loop** `opus · high`:
  `tools/language_docs.py` rendering the pages looped forever on an
  indented item (area 1's sub-list of unique names, and one in area 19),
  which the gate's `--check` run never rendered. Built: the list parser
  follows indentation, so an indented item starts a list inside the item
  above it and a line indented under an item continues it; `--check`
  renders every page into a scratch folder, each under a 10-second limit;
  and the gate's `docs renderer` step runs the renderer's tests
  (`tools/language_docs_test.py`), which need no `jadpo`.
- [x] **7.8 Teaching examples for Phase 7: email and sign-in** `opus ·
  high` (split on 2026-10-20: packages, calls and receiving are 7.8b, AI
  and passkeys 7.8c). Built: 27-guest-house extended with a shared
  partial (`shared/emails/button`, `{{ include }}`) in both its emails,
  `email.sent` read in action and route tests, magic-link sign-in with
  `generate_routes: false` and its own routes over `magic_links.request`,
  `.redeem` and `.redeem_code` (`SignInToken`, `SignInCode`), its own
  sign-in email picking from `MagicLinkEmail`, `on_first_sign_in:`,
  Jadpo's own messages worded in Welsh, a `SignedIn` subscriber, and a
  route test that signs in through the emailed link, passing
  `--require-examples`.
- [x] **7.8b Teaching examples for Phase 7: packages, calls and
  receiving** `opus · high`, after 7.8. Built: 26-delivery-desk extended
  with Swift Couriers' webhooks (`SignatureScheme.standard_webhooks`, its
  `events/`), the package's own subscriber and action emitting its public
  `ParcelsCollected` to the desk's subscriber, a poll of its price list
  with `max_concurrent:` lowered by the connection, a recipe in
  `recipes/keep_tariff/` compiled in its `sample/` app and copied into
  the desk with `jadpo package recipe`, recordings confirming its calls,
  Resend installed as the email connection (`provides/email.jadpo`), an
  SQS queue through `receiving:` (`provides/receiving.jadpo`), Postcodes
  2.0.0 with its `translations/` held beside 1.0.0 and reworded under
  `postcodes_v2: { }`, Parcel Rates imported from OpenAPI with `amount`
  and `currency` read as `Money` through `wire_parts:` and its types
  `reviewed:`, and a pickup booked with its return as two steps under the
  API's required `Idempotency-Key`, `steps: { encryption_key: }` and a
  20-second deadline, passing `--require-examples` with the exemptions
  for the package folders `subscribers`, `events`, `provides` and
  `translations` and for `syntax PROVIDE_DECL` removed. Found and fixed:
  `jadpo fmt` rewrote installed packages and broke their lock, and a
  test's scripted provider answer was written as the contract writes it,
  not on the provider's wire (`wire_parts:` and encodings), so it didn't
  decode; the example-names check leaves out an example's copy of a
  package this repository holds in `packages/`.
- [x] **7.8c Teaching examples for Phase 7: AI and passkeys** `opus ·
  high`. Built: a new example, 28-lost-property (in place of
  22-plant-helpline, whose tests run only where pgvector is 0.8.0), with
  the first-party OpenAI package installed by `jadpo package add`:
  `ai.embed` in a subscriber, `ai.embed_all`, `ai.distance` in a query's
  condition, `ai.generate` with each failure mapped, `limits:` and
  `personal_data: { allowed: }` with `to_models: false`, `ai.stream` sent
  to a route with its `deadline:`, the fake's scripted, `raw(...)`,
  `partial(...)` and `recorded(...)` answers and `.requests`, and a
  recording; 27-guest-house with `passkey: {}`, its Welsh wording and a
  route test that recovers a lost phone's passkey through the emailed
  link. The coverage test now requires the `ai` calls and their fake and
  drops the exemptions for `provides/` and `PROVIDE_DECL`, which 28's
  installed package uses; the example-name ban skips verbatim copies of
  first-party packages an example installs.
- [x] **7.8d Fixes found in chunk 7.8b** `opus · high`. Built: an app's
  outcome `match` names a package's failure through the package as `fails`
  writes it (`SwiftCouriers.SlotTaken ->`, `Postcodes.v3.InvalidPostcode
  ->` while two versions are held) for provider calls, package actions
  and library functions, an arm resolution refuses no longer also
  reporting the `match` as missing it; a connection only `email:` names
  needs no `domains:`. 26-delivery-desk books the return with its own
  call to Swift, matching `SwiftCouriers.SlotTaken` directly, and its mail
  connection drops `domains: []`.

### Phase 7 review (2026-10-21)

Two independent reviewers, soundness and architecture for the phases
ahead, read everything Phase 7 built; their confirmed findings are the
chunks below, taken before Phase 8, in this order (7.9a, 7.9b and 7.9c
side by side, then 7.9d, then 7.9e).

- [x] **7.9a Phase 7 review fixes: keys and replays** `opus · xhigh`: a
  route whose action calls out but neither writes nor emits keeps the
  client's `Idempotency-Key` (a store for its keys, provider keys derived
  from the client's key, the action resumable); a delivery run while a
  route waits (`settle:`) starts its step counter at 0 and gives the
  route's back; the magic-link `send:` action keyed per attempt, or its
  link and code the same on every attempt; a step's `unavailable` or
  `rate_limited` failure with its own code left in doubt and resent under
  its key, never replayed; the runtime's own timestamps (`emitted_at`,
  AWS signing, the AI budget's day, a received event's) on the wall clock,
  the record's fixed clock for `clock.now` only; an answered unkeyed
  changing call on area 6's rerun parked `outcome_unknown` (areas 12 and
  16 made to agree); and in tests, fakes judged exhausted or unused on the
  plain run only, the `ai` answer cache cleared per attempt. Built: each
  as listed, the sign-in email (its `send:` action and Jadpo's own) keyed
  per attempt from fresh randomness with no record, a route's held commit
  and a delivery's taking the same steps and parking an unkeyed change
  too, and OpenAPI saying where a key is kept; choices in area 12's
  decisions table, "Keys and replays, chosen in chunk 7.9a".
- [x] **7.9b Phase 7 review fixes: policy and privacy** `opus · xhigh`:
  `to_models: false` and `personal_data: { allowed: }` count what a
  condition decides (an `if`, a `match`, a `list.filter` on a marked
  field), only a `for` loop's list dropped, and a write storing marked
  content in a field without the same marks flagged; a webhook reply held
  to area 9's output proof as an output to `Access.public`; the
  magic-link `send:` action run with the tenant of its delivery, reply
  actions and `on_first_sign_in:` with no tenant, and
  `policy.system_grant_missing` checked for all three. Built: the walk
  for what reaches a model counts what an `if`, a `match`, a filter's
  predicate or a `return` after one decided, and leaves out only what a
  loop goes through (the records and rows of a `for`'s list, `list.map`'s
  and `list.count`'s), but for what decided which items it has; a create,
  update or transition storing a marked field's value, or one it decided,
  in a field without its marks is `semantic.ai_field_unmarked`, a
  parameter checked where it is passed and an event's part where a
  subscriber passes it; a reply action's reply proved readable by
  anyone at all (`policy.output_field_unproved`); `System::NO_TENANT` for
  reply actions and `on_first_sign_in:`, the `send:` action given
  `outbox::tenant` of its delivery, and `policy.system_grant_missing` at
  each one's `replies:`, `send:` or `on_first_sign_in:`. Choices in the
  Decisions of areas 21, 9, 11, 8 and 12 (Richard, 2026-10-21).
- [x] **7.9c Phase 7 review fixes: ticks, queues and budgets** `opus ·
  high`. Built: a poll's row keeps the interval it was claimed under
  (`jadpo_polls.every_ms`, 26-delivery-desk's migration adding it), so
  only one claimed under another interval counts as shortened and two
  ticks, or a slower clock, can't both take it; a queue message whose
  event the store refuses as a statement is parked `unreadable`; an `ai`
  call's reservation counts each repair's growing conversation, a longer
  repair or a send again after a provider's error taking what it sends
  first and charged what it sent (`ai.generate`, `ai.embed` and
  `ai.stream`); a delivery's answer closes its connection's breaker
  whatever the breaker was at its claim; `jadpo email open` on Windows
  hands the link to `rundll32 url.dll,FileProtocolHandler` as one
  argument, no shell; `jadpo dev` adds `build/` to `.gitignore`. Each
  choice is in its area's Decisions (11, 12, 14, 16, 21).
- [x] **7.9d One outside-call path** `opus · xhigh`, after 7.9a: one
  `exchange` for whole and streamed answers, owning `max_concurrent:`'s
  slots and the time left (the route's remaining deadline, the background
  budget) so a `Retry-After` never waits past it; one retrying send for
  every call kind (provider, `ai.generate`, `ai.embed`, `ai.stream`,
  files); one place deciding where an answer comes from (replay tape, step
  record, rerun reuse); each call kind only building its request and
  decoding its answer. Built: `jadpo_runtime::outside`, with
  `comes_from` (the action's record, an `ai` call's answer or open stream
  kept for a rerun, or a send at its place), one retrying `send` (a
  provider's `retry:`, an `ai` call's sends within its `timeout:`, or
  once for files, email and deliveries) and one `exchange`, whole or
  streamed, which takes the connection's `max_concurrent:` slot, so
  `ai.generate`, `ai.embed` and `ai.stream` through a package are capped
  too; no wait outlasts a route's remaining deadline or the background
  budget, a `Retry-After` past it ending the call at once with its last
  answer, `unavailable`; `stream::exchange`, `ai::sent` and
  `provider::retried` gone. Choices in area 12's decisions table, "One
  outside-call path, chosen in chunk 7.9d".
- [x] **7.9e The runtime's start-up state** `opus · xhigh`, after 7.9d:
  `jadpo_runtime::Started::new(plan, values, target)` holding the decoded
  settings, keys, credentials, sign-in settings, client address and purge
  periods `Program` holds today, reached through `Ctx`; runtime entry
  points taking `(ctx, site, input)` so the interpreter's eight copies of
  per-call glue go; settings resolved one way (email and receiving honour
  a test's `config:` as `ai` does); the `Executor` shrunk toward places,
  plan and run. Built: `Started` as listed, reached through
  `Ctx::started`, which every context the runtime makes for a program is
  given (`Ctx::of`) and a test harness's is given as the program first
  runs there; every setting read through `Ctx::setting`, a test's own
  value else the program's, so an email through a mail package honours a
  test's `config:` and a queue's mapping reads its settings the same way;
  `jadpo_runtime::RuntimePlan`, the plan the runtime reads, from which it
  builds each part's view, leaving the `Executor` `places`, `callable`,
  `plan`, `started`, a test's `settings` and `clock`, `recording`, `run`
  and `row`; `provider::call`, `ai::generate`, `stream::stream` and
  `embed::embed` each taking the context, the site (`provider::Calling`,
  `ai::Calling`) and the input, the runtime's own calls too.
- [x] **7.9f Fixes Richard found in the editor** `opus · high` (found
  2026-10-21): a field's type written as a value or a call (`Test.a`,
  `Test("x")`) refused, naming the field, in every object type (an input,
  a failure's fields, an event's, an entity's, a setting's), never
  silently dropped; an enum member given a label (`a: "A"`) refused,
  pointing at translations; the language server completing an entity's
  fields after `FoundItem.` in a type position. Built: `type.not_a_type`
  (area 2) for a call, a literal, a block of fields or an enum's variant
  where a field's or setting's type goes, in every block of fields, an
  entity and `config { }`, and for a variant anywhere a type goes (a
  signature, `List<Status.open>`), with the edit writing the type; such a
  field kept with a type that absorbs, so it is never dropped; `[Tag]`
  in a field is `syntax.collection_type`; `type.enum_label` (area 2) for
  a variant given a label or value, its next step the translation; after
  `Owner.` in a type position, written as a member's value or in a
  signature, the language server offers its fields, an enum's variants
  with fields, or a variant's fields, never a plain variant.

- [x] **7.9g Privacy gaps found in 7.9b** `opus · high` (backlog): a
  model's answer carries what its prompt was built from, so a field read
  off it (`.summary` of health notes) is checked like its sources when
  stored or sent again; and a webhook reply that counts or tests for
  records only the system may read is held to the output proof, since
  whether a record exists reaches whoever sent the webhook. Built:
  `ai.generate`'s answer and `ai.stream`'s text, and every field read off
  them, carry everything the prompt was built from, for
  `semantic.ai_field_unmarked`, what reaches a model and area 9's stores
  (example 28's stored reply fields marked `identifying`); the flow walk
  keeps whether a record is there (`Src::Present`) where a condition, a
  loop or a count over records decided a value, which only a reply
  action is held to, so a reply counting or testing for records anyone
  at all may not read is `policy.output_field_unproved`; a reply
  action's failure held to the same proof (what decides it fails, a
  `get`, a change by `id` or a `conflict:` finding a record or not, and
  its client fields); a failure's client fields declared in a domain
  found by its table name, so they are proved at all. Choices in the
  Decisions of areas 9, 11 and 21.

## Phase 8 — Compiled deployables (finished chunks)

Chunk 8.1, compiled native code with every test passing interpreted and
compiled, was split on 2026-10-21 into the chunks below, taken in this
order. 8.1b can run beside 8.1a, and 8.1l and 8.1m, which touch only the
native host, each beside any of 8.1f to 8.1j. What the Phase 4 review
asked for is decided in §2.5 first, in the chunk that builds it.

- [x] **8.1a One runtime plan and its encoding** `opus · high`. Built:
  what the runtime reads of a program as one owned value,
  `jadpo_std::RuntimePlan` (the validator, shape and failure plans, the
  JSON table, the route table with its codecs, settings, subscribers,
  batches, schedules, purges, stores of files, providers, `ai:`, prompts
  and their recordings), built by `jadpo-plan` as `Plan::runtime`, the
  type table's numbered plans moved into it; each thread loading it once
  (`jadpo_runtime::Loaded`: validators compiled once and shared, shapes
  per thread), the interpreter's `Program` among them, and the view the
  runtime's parts read renamed `jadpo_runtime::Plan`, borrowed from it;
  its canonical byte encoding (`jadpo_std::canonical`, on serde's data
  model, with `RuntimePlan::encode` and `decode` behind a format header),
  every page's project and teaching example reading back equal and
  writing the same bytes again, each example the same planned twice; the
  HTTP component's `App` built from the plan (`Plan::app`), so
  `serve::Application` is gone. How compiled code holds it is §2.5's "The
  plan compiled code holds".
- [x] **8.1b The upgrade hook and SMTP over it** `opus · high`. Built:
  `Transport::upgrade`, turning a stream the transport opened into TLS
  with the server's certificate and name checked, shaped for
  PostgreSQL's SSL request too, and the transport's timer, clock and
  local address; natively rustls on `ring` against Mozilla's roots
  compiled in (`jadpo_host_native::tls`), the one TLS stack on the
  request path, the package driver's HTTPS moved to it from OpenSSL; the
  SMTP conversation moved from `jadpo_host_native::smtp` (gone) into
  `jadpo_drivers::smtp::relay` over `Transport`, area 16's rules kept;
  proved against a server on this machine with an authority and
  certificate the test makes (STARTTLS, port 465, a wrong name, roots
  that don't vouch for it, no TLS elsewhere, no greeting). Why these
  roots and this provider is §2.6's "TLS through the transport".
- [x] **8.1c The outcome-and-effect record** `opus · high`, after 8.1a.
  Built: `jadpo_runtime::record`, written by the runtime whichever
  executor runs a test, in order: each statement's SQL text, parameter
  types and rows read or changed (a test's stores wrapped by
  `record::Recorded`), each transaction's end, each event written to the
  outbox with its payload, each outside call's connection, operation and
  answer (through `Host::record`, which only a test's `Faked` answers),
  and the outcome (`record::outcome`); `Options::record` keeps each
  test's `TestRecord` on its result, in canonical form with the final
  database's rows and identities numbered as they first appear, and
  `jadpo_test::differential` names the first differing line with both
  sides. Proved by the corpus run twice with no difference (473 tests)
  and perturbed runs named at an assertion and at a row (§2.5,
  "Differential test").
- [x] **8.1d One connections table and the driver factory** `opus ·
  high`. Built: the plan's connections table
  (`RuntimePlan::connections`, `jadpo_std::connections`: each
  connection's name, its driver with the settings it is built from, and
  the targets it runs on), folding the provider plan's connections, the
  email plan's SMTP servers, the stores of files, and listing `ai:`'s and
  the Cloudflare queues' connections, a provider operation's connection
  and each credential kept by its number there; `Started` holding the
  text each driver is built from; `jadpo_drivers::factory`
  (`Drivers::build`) building PostgreSQL, the package driver,
  `Driver.openai_compatible`, SMTP, the disk and Cloudflare Queues from
  them over the host's `Supplies`, and keeping what it can't build with
  why, so a deployable builds a real SMTP driver from the plan;
  `NativeHost::with_drivers` in place of `with_providers`, `with_models`
  and `with_mail`, `jadpo dev` putting its fakes in front with
  `Faked::choosing`, and the record tools building their one connection's
  driver the same way. `jadpo test` builds none (no test reaches a real
  driver). §2.6's "One connections table and the driver factory".
- [x] **8.1d2 Provider call sites by number** `opus · high`. Built: a
  provider call's site plan naming its operation by its number in the
  plan's provider operations (`CallSite::called`), or a package action's
  own operation by its place among its package's, added to the first
  operation of the connection the action was called through
  (`ProviderPlan::first`); `provider::Calling` down to its plan and site,
  and `provider::call` and the interpreter's failure mapping finding the
  operation by number; `jadpo-plan` numbering the operations before any
  site is decoded (`providers::Numbering`); a mail package's email, a
  queue's receive and acknowledgement and a poll carrying their
  operations' numbers in their plans. The names stay in what is logged
  and faked; `ai:`'s package calls still find theirs by name. §2.6's "Provider call sites by number".
- [x] **8.1e A smaller executor and the `testing` feature** `opus ·
  high`. Built: the `Executor` down to its places, plan, start-up state
  and running a callable (`run`, `function`, `row`); callables found by
  key through the plan (`RuntimePlan::callables`, `Plan::callable`); each
  test's key, `config:` and `clock:` and the recordings in the plan's dev
  and test section (`jadpo_std::testing::TestingPlan`, written after the
  rest of the bytes, left out by `RuntimePlan::encode_deployable`); a
  recording checked by the runtime (`recorded::run`) through the
  package's functions reached through the executor; every context made
  from its executor (`Ctx::of`), so `Ctx::start_from` is gone;
  `ai.generate`, `ai.stream` and `ai.embed` through a package finding
  their operation by number (`PackageAi::called`). A
  `testing` cargo feature on `jadpo-std`, `jadpo-runtime` and
  `jadpo-host-native` carrying the fakes, recordings, crash points, the
  outcome-and-effect record and that section; the gate builds a
  deployable example without it and reads its symbols
  (`tools/deployable.py`). A release build of that example is 30,513,808
  bytes without the feature and 30,629,520 with it. §2.6's "The `testing`
  feature".
- [x] **8.1f Compiled code: the first route** `opus · xhigh`. Built:
  `jadpo-codegen` writing a checked project as one Rust crate from its
  plan (`generate`: `quote!` into `syn`, printed with prettyplease): a
  module per domain, each callable a function over the interpreter's
  `jadpo_value::Value` (a function as `fn`, an action, fixture, test or
  seed as `async fn(&mut Ctx, &Program, …)`), the frame table as plain
  data, and the plan's bytes held as a constant, with and without the dev
  and test section, decoded once per process; compiled code's site plans
  in the runtime plan (`RuntimePlan::sites`: constants, reads, writes and
  outside calls' sites by number; `actions`, what the boundary needs); `jadpo_runtime::compiled`, the compiled program's
  `Executor`, its action body, `invoke` and a function per kind of site
  raising the interpreter's stop at the same site, and
  `jadpo_runtime::operators`, the operators both executors run, so
  arithmetic faults at its site; statements, locals, field reads, calls,
  reads, writes with their `conflict:` arms, builds, lists, operators,
  `if`, `assert`, settings, `clock.now` and a route test's `request`
  compiled, every other form refused by name and place (`Refusal`), 65
  of the corpus's 205 projects that check compiling; `jadpo_test::run_with`
  running tests on any executor, and the differential harness
  (`jadpo_test::compiled`), which builds a project's crate beside a
  harness binary with cargo, offline, in a folder it keeps (73 s cold,
  7 s warm), and runs its tests interpreted and compiled in one process,
  comparing their records; area 7's `todos` project, a route creating a
  todo and one reading it back, with three route tests added to the
  page, passing both ways with the same records. §2.5's "Compiled code".
- [x] **8.1g Compiled code: areas 1 to 4** `opus · xhigh`, after 8.1f.
  Built: `for`, `assert … fails`, value and outcome `match` (the
  handled failure a variable while its arm runs, for `propagate` and a
  `reject`'s cause), `attempt` over a construction, `reject`,
  `Type(value)` checked only where the checker couldn't prove it, every
  standard operation with its inline functions as closures, `emit` and
  `email.sent`, through new site functions in `jadpo_runtime::compiled`;
  every standard operation, interpreted or compiled, through one entry
  point, `effects::standard` (effects as `effects::run`, the rest through
  `jadpo_value::run` with the locales, which joined the runtime plan),
  and an `attempt`'s arm found by `jadpo_std::arm_for`, which
  the checker's `RuleArm` uses too. 183 of the corpus's 205 projects that
  check compile (65 at 8.1f); what is left is 8.1h's and 8.1i's
  (translations, provider operations and package actions, a test's
  `fakes:`, `batches.*`, `clock.advance`). The pages' projects for areas
  1 to 4 all compile but declare no tests, so the gate's compiled step
  runs the whole corpus that compiles: `differential_all` builds every
  project as a crate of one harness, linked once, and compares 208
  projects' 332 tests with the same records (§2.5, "Compiled code").
- [x] **8.1h Compiled code: data, routes and identity** `opus · xhigh`,
  after 8.1g. Built: a caller made from its record (`Principal.user`,
  `Principal.service`), `sessions.*`, tokens and keys, magic links' and a
  test's passkeys' operations, `batches.each`, `batches.status`,
  `batches.cancel`, a file helper's read and write, `clock.advance` and a
  test's `<schedule>.run()` (an area 13 test form, taken from 8.1i),
  through new site functions in `jadpo_runtime::compiled` (`caller`,
  `session`, `batch`, `advance`, `schedule`); area 8's operations and the
  batches through one runtime entry point each that the interpreter calls
  too (`http::session`, `http::caller_from`, `batches::call`), its copies
  gone and a test keeping them gone; an export's rows compiled as
  functions of their own, reached through the crate's `row` table by the
  compiled `Executor::row`, their locals set by `batches::row_locals`,
  which the interpreter uses too. The differential harness runs each
  batch run a test started to its end, at the test's clock
  (`Options::batches`, `host::AtClock`), so runs and an export's rows are
  compared in the records, and area 19's export with a test that starts
  it proves it; the gate copies each teaching example whole, so all 28
  reach it. 190 of the corpus's 206 projects that check compile (184
  before); the gate's compiled step compares 232 projects and 451 tests
  (210 and 334 before), every project of areas 5 to 9 and 13 compiling
  but those using a form of 8.1i's, 22 of the 28 teaching examples with
  all 276 of their tests passing both ways; 26 projects are left, every
  one for 8.1i's forms (translations, provider operations and package
  actions, a test's `fakes:`, a store's `put` and `read`, prompts, `ai.*`
  and email templates). §2.5's "Compiled code".
- [x] **8.1i Compiled code: events, packages and the rest** `opus ·
  xhigh`, after 8.1h. Built: `translations.<message>` (an email
  template's wordings among them), `prompts.render`, an email template's
  `{{ value }}`, a store's `put` and `read`, provider operations with
  their failures mapped by code in an `attempt`, a package's action
  through the connection named, `ai.generate`, `ai.stream`, `ai.embed`
  and `ai.embed_all`, a test's `fakes:` and `fakes: { ai: }` (a
  recording's answer among them) and `ai.<prompt>.requests`, each through
  one runtime entry point that the interpreter calls too
  (`jadpo_runtime::calls`, giving a `Called`), its copies gone and a test
  keeping them gone; the translations in the runtime plan, and the
  runtime's view of it carrying them, the locales and the outside calls'
  sites. Settings, a test's `config:`, `emit` and the
  deliveries between steps, schedules, email, magic links and webhook
  replies were compiled by 8.1f to 8.1h. All 206 of the corpus's projects
  that check without migrations compile (190 before), and the gate's
  compiled step requires every project and teaching example to compile
  and give the same records both ways and every example's tests to pass:
  258 projects and 525 tests, all 28 teaching examples with their 334
  tests (22 before, 276). A project whose migrations need an extension
  the server lacks is skipped on a developer's machine and fails under
  CI. §2.5's "Compiled code".
- [x] **8.1j The corpus compiled** `opus · xhigh`, after 8.1i. Built:
  every page's project and teaching example compiled as modules of one
  generated crate (`jadpo_codegen::module`, `src/p<n>/` with its plan's
  bytes, and `holding`, the crate's manifest and root listing each
  `program` in `PROGRAMS`; generated code naming its callables from its
  project's root, so a project reads the same as a crate of its own),
  one rustc invocation in the dev profile: 177,642 lines in 258 modules,
  10 s of rustc on this machine, starting beside the compiler crates the
  harness needs; the gate's differential step its group of its own,
  `compiled`, which CI runs as a job of its own against its own
  PostgreSQL container, its cache keeping the harness's registry crates,
  with a budget of 12 minutes (20 since 2026-10-22; §2.7) (5 min 31 s and 5 min 41 s with no cache
  at all, 4 min 11 s with it, every one of 258 projects and 525 tests
  giving the same records both ways and all 28 teaching examples' 334
  tests passing; the `tests` job's gate 420 to 764 s in those runs, from
  739 to 873 s); a run of that group alone building only the
  tests it runs. Locally the full gate still runs it; `--profile quick`
  doesn't. §2.5's "Compiled code" and §2.7's "The differential run".
- [x] **8.1k The route trie** `opus · high`, after 8.1a. Built (§2.5,
  "The route trie"): `jadpo_runtime::http::Routes`, built once as a plan
  is loaded from the route table, health's paths and the webhooks, walked
  fixed segment before placeholder as before; health and each webhook an
  entry of the trie (`Found::Health`, `Found::Webhook`) rather than a
  `Run` in the route table, no longer matched in front of it; and no
  allocation per request for paths, labels or the cursor failure's
  lookup, counted by a test binary's allocator
  (`crates/jadpo-runtime/tests/route_allocations/main.rs`).
- [x] **8.1l Native accept and one rate-limit count** `opus · xhigh`,
  after 8.1d. Built: the native acceptor a thread of its own awaiting the
  listener on a current-thread runtime and handing each connection to the
  workers in turn, passing over one whose thread has ended
  (`jadpo_host_native::http`), in place of a polled listener hashing the
  peer's address; one `jadpo_runtime::RateLimits` per server, which
  `http::WorkerThread::host` gives each worker's `NativeHost`, so a caller
  or an address has one allowance a window across the workers: 16 MB in
  all, in up to 16 shards behind a lock each, a key's shard by a hash
  keyed per process, the counter whose window started longest ago dropped
  first; a stopping server handing out what the system had accepted,
  closing the listener, and letting each connection give the answer it
  owes (one that has sent nothing yet a second for its first request)
  before its background work drains. Proved by one client's connections
  reaching four workers in turn, 120 of its 125 requests answered across
  them, a burst of 100 answered in full a quarter per worker, and a stop
  that answers a request in flight and one on a connection accepted
  before it, closes an idle one at once and accepts nothing more. §2.5's
  "Native accept and one rate-limit count"; §2.6's "Single-threaded by
  design" rewritten for it.
- [x] **8.1m A pooled HTTP client** `opus · xhigh`, after 8.1l. Built:
  `jadpo_host_native::fetch::Client`, hyper's HTTP/1.1 client over
  tokio's sockets on the worker's thread with rustls's connector made
  once, behind both `NativeFetch` (one per worker, `NativeFetch::new`,
  or `trusting` a test's authority) and `Outward`, in place of a blocking
  exchange per call on tokio's blocking pool, a thread per stream and a
  second copy of the client: connections kept per host in a pool on the
  worker (at most 8 waiting per host, for 30 seconds), one the server
  closed or said it would close never written to, a request one hands
  back unwritten sent on a fresh one; a call's timeout or its future
  dropped closing its connection at once; a streamed answer read as it
  comes; a name looked up only when a connection opens; `Outward`'s
  guard kept (HTTPS only, public addresses only, no redirect, at most the
  bytes asked for, `Outward::reaching` for a test). Proved against
  servers on this machine: five calls to one host on one connection and
  another host on its own, three HTTPS calls on one handshake, a closed
  or closing connection never used again, a timeout and a dropped call
  or stream closing its connection within a second, a streamed answer's
  first piece before the server goes on, and the guard refusing plain
  HTTP and loopback names and addresses, following no redirect and
  refusing an answer longer than asked for. §2.6's "A pooled HTTP
  client".
- [x] **8.1m2 A flaky test after the pooled client** `opus · high`,
  after 8.1m: two flakes, two causes. `provider_wire`'s connection gave
  each call 300 ms, which a loaded gate passed; the timed-out `GET` was
  sent again and the fake answered the retry with its unscripted `{}`,
  so "nothing at `data`" (reproduced exactly with an answer held 400
  ms). Its `couriers` now waits 5 s and `quick`, the only connection
  whose timeout is tested, 1 s against answers held 1.5 s. And a server
  closing a kept connection as the next request reaches it, a race no
  client sees coming, gave `NoAnswer` on Linux CI: a request that may be
  sent again once it has left (`provider::Request::repeatable`, set by
  `Call::with_request` from the call's `Repeat`; `Outward`'s `GET`s) now
  goes out once more on a fresh connection, a change with no key still
  `NoAnswer`. Proved deterministically by a server that drops every
  kept connection's second request: a `GET` and a keyed `POST` answered
  on a fresh connection, an unkeyed `POST` not sent again.
- [x] **8.1n PostgreSQL over TLS** `opus · high`, after 8.1m. Built:
  as area 5 decides (Richard, 2026-10-21), a database connection always
  over TLS, checked against Mozilla's roots compiled in, but to this
  machine (a Unix socket, `localhost`, `127.0.0.1`, `::1`), through
  PostgreSQL's SSL request and `Transport::upgrade`, which now takes whom
  it trusts (`Trust`); `authority: config.<name>`, a setting of area 19's
  `Certificate`, read from its PEM, trusted alone and to this machine too;
  `tls: false` for a server elsewhere that offers none, listed in the
  network report; an address setting `sslmode` or another `ssl…` refused
  by the driver, `PostgresUrl`'s check and `jadpo migrate`; a refusal a
  fault naming the server, never unavailable. The plan carries it
  (`DatabaseTls`), the factory builds it
  (`Postgres::secured`), and `jadpo dev`, `migrate`, `schedules`, `runs`,
  `deliveries` and `ai usage` open a database as its connection says.
  `jadpo check` refuses another key on a database connection, an
  `authority:` that isn't a `Certificate` setting, `tls:` written other
  than `false`, and both together. Proved against a scripted server and
  live against servers the harness starts on `127.0.0.1`, one with a
  certificate a test authority signed (`Server::start_on_tcp`). §2.6's
  "PostgreSQL over TLS".
- [x] **8.1o A native deployable** `opus · xhigh`, after 8.1j and 8.1n;
  split on 2026-10-21, declared deployables going to 8.1o2. Built:
  `jadpo build` for the one deployable an app that declares no
  `deployables:` has, `app`, natively (area 14, "A native deployable"):
  the project checked as `jadpo check` checks it, stopping at its errors;
  its Rust written into `build/rust/` (`jadpo_codegen::deployable`: the
  program's crate, a binary named after the deployable, the pinned
  toolchain file and the runtime crates' lock) and built in release,
  `--locked`, offline, with its paths remapped and `SOURCE_DATE_EPOCH`
  fixed, into `build/target/app/app` beside each API's OpenAPI document
  and `<api>.failures.<locale>.json`, without the `testing` feature; its
  build stamp (the build id, `b_` and twelve hex digits over the graph's
  digest, the compiler, the target and `jadpo.lock`, the commit, whether
  the tree was modified, the compiler, the deployable and the target) a
  constant in the binary and on every event it logs, the runtime plan
  recording the build id (`Plan::with_build`); 6.9's deployable handler,
  `jadpo_host_native::deployable::run`: its settings read through
  `config::load` from the environment, every problem logged as
  `runtime.startup_failed`, its drivers built by 8.1d's factory and those
  it couldn't build listed, each store compared with the build's
  migrations and the start refused for one not applied, unknown or
  changed (area 5), an unreachable database waited for (area 10), then a
  worker per core on `PORT` serving and ticking through `Serving` on
  `NativeHost`, a fault behind a 500 logged with its `.jadpo` frames,
  and SIGTERM draining it; `toolchain.build_failed`. Teaching example 23
  names its database's address from a setting. Proved live
  (`crates/jadpo-cli/tests/deployable.rs`, in the gate's `compiled`
  group): example 23 built, holding none of the `testing` symbols,
  refusing to start without `DATABASE_URL` and with its last migration
  not applied, then booking a repair, answering its readiness check and a
  request whose body arrived after SIGTERM, and ending with code 0 (49 s
  cold, seconds warm). §2.6's "A native deployable".
- [x] **8.1o2 Declared deployables** `opus · xhigh`, after 8.1o; split
  on 2026-10-21, a deployable compiled from only what it reaches going to
  8.1o3. Built (area 14, "Declared deployables"): `deployables: { }` read
  from `app.jadpo`, each with `serves:`, `runs:` (`Trigger.schedules`,
  `Trigger.subscribers`) and `target:` (`Target.native`, `Target.wasm`),
  or `app` alone with no target when none is declared (Richard,
  2026-10-21), and `deployable.invalid`, `api_unknown`, `trigger_unknown`,
  `target_unknown`, `empty`, `unserved` (an API with routes, or the app's
  schedules or subscribers, that no deployable takes) and `settle_unrun`;
  what each reaches, from its APIs' routes, webhooks' replies, schedules,
  subscribers and batch runs through every callable's calls to the
  connections they use, its polls, webhooks, queues and email
  (`jadpo_check::deployables::reaches`); a deployable that declares a
  target refused each connection it reaches that the target can't run,
  naming the alternative (`deployable.target_unsupported`: the disk, a
  service without `host:` and SMTP on port 25 on Workers, a Cloudflare
  queue natively), and `jadpo check`'s "On Workers" listing each
  `max_concurrent:` a Workers deployable reaches, counted per instance;
  each deployable's plan serving only its APIs' routes and webhooks and
  running only its triggers (`Plan::for_deployable`, `RuntimePlan::runs`;
  the runner and the tick hold to it); `jadpo build
  [<deployable>]`, each into `build/target/<deployable>/` beside its own
  APIs' documents from `build/rust/<deployable>/`, one cargo target for
  all, the program named `jadpo-program-<deployable>` so their units
  don't collide, each binary placed as a new file (macOS kills one copied
  over a binary it ran), a `Target.wasm` one `toolchain.not_built_yet`;
  the manifest's `source` and `deployables`, each one's target and build
  id, which digests a declared deployable's declaration with the graph's
  digest (`jadpo_cli::stamp`). Proved live
  (`crates/jadpo-cli/tests/deployable.rs`): example 23 declared as a
  kiosk serving `main` and running the subscribers and an office serving
  `staff` and running a schedule each second, the kiosk booking and
  refusing the staff's route without running the schedule, the office
  the other way round and running it (20 s warm).
- [x] **8.1o3 A deployable compiled from what it reaches**
  `opus · xhigh`, after 8.1o2. Built (area 14, "Declared deployables"; area
  18, "What a build id digests"): `Plan::for_deployable` plans a
  deployable from what it reaches (`jadpo_plan::Keep`): 8.1o2's
  `reaches` with what Jadpo runs by name (`on_first_sign_in:`'s action
  where an API is served, `magic_link: { send: }`'s and the email
  templates where subscribers run, every package's `decode:` and
  `encode:`, kept whole), the queries they reach and the entities whose
  `get` they call, the served APIs' routes alone, schedules, purges and
  polls only where schedules run, queues, templates and the mail package
  only where subscribers run, every subscriber still listed without its
  action so what it emits waits for the deployable that runs it
  (`Target::Elsewhere`), the connections it reaches with
  every database and Redis, `ai:` where it reaches the model, the prompts
  and event types its callables use, and only the settings those read,
  found by planning twice (`RuntimePlan::setting_names` for those read by
  name); each table numbered from what is kept, and anything a kept part
  names that isn't kept refusing the plan (`jadpo_plan::Unkept`,
  `toolchain.build_failed`). Its build id digests the program as compiled
  (`jadpo_cli::stamp::program`: its Rust as its tokens' text,
  `jadpo_codegen::unprinted`, its plan's bytes, its name and its stores'
  migrations), worked out by `jadpo check` too and left out of the
  manifest while the project has errors (decided in the chunk). A Worker
  links SMTP when its plan holds an SMTP connection; `jadpo build`'s
  `settings` lists the deployables' variables once each. Proved by a
  café's kiosk and office each holding only its own callables, routes,
  schedules, settings and connections, every corpus project's
  deployables naming nothing they don't hold (an undeclared app running
  all the whole plan runs), a kiosk's id unchanged by the office's
  action, route and setting or a test and changed by its own action, and
  live: the kiosk's binary holds no sweeping, the office's no booking.
  Example 23, release, aarch64 macOS, against the whole program: `app`
  21.10 to 21.09 MB (17.19 to 17.17 MB stripped), declared as a kiosk
  and an office 21.12 to 21.09 MB and 21.09 to 20.51 MB, and the kiosk
  on Workers 12.81 to 12.80 MB (3.98 MB gzipped): the runtime and its
  locale data are most of a deployable.
- [x] **8.1p A typed per-route entry** `opus · xhigh`, after 8.1j and
  8.1k. Built, decided first (§2.5, "A typed per-route entry"): each
  route's codecs generated from its types' plans, `entries.rs` in a
  project's crate, a reader per codec a route reads and a writer per codec
  it writes, and each route's entry (`jadpo_runtime::http::Entry`: its
  placeholders' readers, its input's, its output's writer), which the HTTP
  component takes from compiled code (`App::entries`) in place of the
  codec walk, the interpreter keeping the walk as the reference and both
  reading a request through one function (`http::read_request`); a body
  read in place (`jadpo_value::json::Doc`, accepting exactly what
  `json::parse` accepts, held to it by a seeded comparison), the readers
  calling the walk's own rules (`json::presence`, `json::scalar`,
  `Codecs::check`, `json::collected`, `Shape::build_slots`) and writing a
  problem's path only when there is one; `Value` kept from the action on,
  and a codec the generator doesn't walk (a map, a standard type sent as
  its parts, a geometry, a page, a standard enum read ignoring case) read
  and written by the walk at that place. No Rust struct per type: one
  would become a `Value` at once; typed bodies are listed with 9.2. The
  compiled step holds every route's entry to the walk over a seeded set of
  requests and values (`jadpo_test::entries`, 5,832 across the corpus
  here), and the differential run still gives the same records both
  ways (257 projects and 518 tests on this machine, whose pgvector is too
  old for example 22, which CI runs). On example 23's `POST /repairs`, release
  build: reading the request 26 allocations and 1.35 µs by the walk, 8
  and 0.57 µs by its entry; writing the answer 1.05 µs against 0.89 µs,
  17 allocations either way.

Chunk 8.2, WebAssembly on Cloudflare Workers, was split on 2026-10-21
into the chunks below, taken in this order. All but 8.2n run on this
machine with no Cloudflare account and no network past it: workerd,
Cloudflare's own Workers runtime, pinned in `tools/workers/` and
installed there by `npm ci` (never globally), and wasm-bindgen's Rust
library at the version the lock holds, so a build downloads nothing.
8.2a can run beside 8.1o2 and 8.1p; 8.2c beside 8.2a and 8.2b; 8.2l
beside any chunk after 8.2a; 8.2m beside 8.2d; 8.2f and 8.2i beside
each other. Two refusals the entry listed, `Driver.disk` on Workers
and `Driver.cloudflare_queues` in a native deployable, are 8.1o2's,
whose entry names them.

- [x] **8.2a The Workers host** `opus · xhigh`, after 8.1o. Built:
  `jadpo-host-workers` over wasm-bindgen with Jadpo's own bindings to
  `connect()`, `ctx.waitUntil`, `fetch`, `setTimeout` and `crypto`
  (workers-rs rejected: its `start_tls` unwraps and its build tool
  downloads unpinned tools): `WorkersHost`, the runtime's `Host` for one
  invocation (Web Crypto's randomness, `Date.now()` and a monotonic clock
  from it, timers on `setTimeout`, settings from the bindings, `defer`
  through `waitUntil`, rate limits in the isolate's memory, the factory's
  drivers per invocation, and 6.7's budgets set to Workers': 30 s after a
  response and for work handed on, 15 minutes for the cron);
  `WorkersTransport` over `connect()` and `startTls()` (checked for the
  name, a connection's own authority refused, a failure before anything
  has gone through over TLS a failed check); `WorkersFetch`, aborted
  through an `AbortSignal` at its timeout, whole or streamed, following no
  redirect (`redirect: "manual"`), and `WorkersOutbound`, the outbound
  guard the same way (HTTPS only, no address written as one that isn't
  public, `jadpo_runtime::public_address` now shared with the native
  host); `supplies()` for the factory. Proved by sixteen tests of a probe
  Worker built for wasm32, bound by wasm-bindgen's library at the lock's
  release and run under workerd 1.20261007.1, pinned in
  `tools/workers/` and installed there by `npm ci`, against servers the
  test starts (TCP, TLS from a trusted and an untrusted authority,
  HTTP), an aborted call's connection let go while the invocation goes
  on; a step of the gate's `wasm32` group. §2.6's "The Workers host".
- [x] **8.2b A Workers deployable from `jadpo build`** `opus · xhigh`,
  after 8.2a and 8.1o2. Built (area 14, "A Workers deployable"): `jadpo
  build [<deployable>]` builds a `Target.wasm` deployable, no longer
  refused: its program's crate and a `cdylib`, `jadpo-worker-<name>`,
  exporting the Worker's `fetch` through `jadpo-host-workers`'s own
  wasm-bindgen and handing it the program, the stamp and each store's
  migrations (`jadpo_runtime::deployable::Deployable`, now shared with
  the native host), linking PostgreSQL's driver when the project keeps a
  database and SMTP's (a new `smtp` feature) when it reaches an SMTP
  connection; built for `wasm32-unknown-unknown` in release, offline and
  `--locked`, into the cargo target native deployables share, and bound
  by wasm-bindgen's library at the lock's release
  (`jadpo_codegen::workers::bind`); in `build/target/<deployable>/` the
  module, its JavaScript, Jadpo's shim `worker.mjs` and `wrangler.json`
  (the Worker's name with dashes, the shim as `main`, no bundling with a
  rule per kind, compatibility date 2026-10-01, each setting without a
  default a required secret); the stamp's target `wasm`; the screen's
  `worker`, `module` and `size` (raw and gzipped) and `--json`'s
  `wrangler` and `module`. Until 8.2d the Worker makes its program once
  per isolate and answers every request 503, which the screen's alert
  says. Proved (`crates/jadpo-cli/tests/workers.rs`, the gate's `wasm32`
  group): example 23 declared as a kiosk on Workers, built in 42 s cold,
  holding no `testing` symbol, and loaded by the pinned workerd from its
  files. Its module, with 8.2c's locale data, 9.5 MB bound, 3.0 MB
  gzipped (4.5 MB code, 3.9 MB data, 1.1 MB names; 19.7 MB with every
  locale); as cargo built it, 10.2 MB without the `testing` feature and
  11.7 MB with it (3.1 and 3.6 MB gzipped).
- [x] **8.2c Locale data for the supported locales** `opus · xhigh`,
  after 8.1o (from chunk 5.3). Built (area 10, "Locales"; §2.6, "The
  locale data a deployable carries"): `jadpo-cldr`, a tooling crate,
  bakes ICU4X's compiled data again for a deployable's supported locales
  (each marker's entries their lookups walk through, down to `und`;
  every locale's collation, since `list.sort_by` takes any `Locale`;
  every marker with one entry) into `build/rust/cldr/`, which `jadpo
  build` compiles into each deployable it builds through
  `ICU4X_DATA_DIR`, one folder for them all, native and Workers alike;
  `jadpo test` and `jadpo dev` keep every locale. Decided in the chunk
  (area 10, 2026-10-21): a `Locale` the app doesn't support reaching
  `text.format`, `temporal.format_relative` or a message is written in
  the supported locale a header naming it would get, else `default:`, on
  every target (formatting wrote it in its own words, a message
  faulted). Proved live by a shop supporting `en_gb` and `fr_fr`, built
  and run: no German or Spanish weekday in its binary, and its answers
  are `jadpo dev`'s. Before and after: the shop's native binary 31.3 MB
  to 21.0 MB (27.4 to 17.1 MB stripped; release, aarch64 macOS); a wasm32
  module running every operation (`jadpo-value`'s `formats` example,
  release) 18.5 MB to 8.5 MB, 6.1 to 2.7 MB gzipped, 0.9 MB of it every
  locale's collation.
- [x] **8.2d One route on workerd** `opus · xhigh`, after 8.2b. Built
  (area 14, "A Workers deployable"; §2.6, "Serving on Workers"): the
  Worker's `fetch` as 8.1o's native handler, a promise of the `Response`.
  An isolate starts at its first request: the program made once, its
  settings read from the bindings through `config::load`, and a
  `client_address:` source other than `cloudflare` refused
  (`client_address_refused`); a failure is logged once as
  `runtime.startup_failed` and every request answered 503 `unavailable`.
  Each request builds its drivers through the factory on a `WorkersHost`
  of its own and, until the isolate has found every store current,
  compares the stores with the build's migrations first, a store behind,
  ahead or out of reach answered 503 and logged, so a migration applied
  after a deploy is picked up with no restart; then `runtime.started`.
  Each request goes through `Serving`, sharing the isolate's
  `http::Worker` (`Serving::sharing`), the client's address from
  `CF-Connecting-IP` (`Program::with_config_on(values,
  Target::Workers)`), what it leaves run in `waitUntil` and a fault
  behind a 500 logged as `operation.failed`; events are JSON lines on
  the console with the stamp; a streamed answer is sent whole until
  8.2h. The log, the start-up problems, the migrations' comparison and
  the fault observer are shared with the native host
  (`jadpo_runtime::deployable`), and `jadpo build`'s 503 alert is gone.
  Decided in the chunk (area 14, "A Worker's start", 2026-10-21). Proved
  (`crates/jadpo-cli/tests/workers.rs`, the `wasm32` group, now given
  the run's PostgreSQL, which CI's `wasm32` job starts): example 23's
  kiosk under workerd without `DATABASE_URL` answering 503 and logging
  its variable once; against a database missing its last migration
  answering 503 and naming it; then, the migration applied, the same
  isolate booking a repair through `settle:`'s subscribers and answering
  the ticket read back, a thank-you pinned up in `waitUntil` after its
  202, its readiness check reaching the database, and 120 requests a
  minute for each `CF-Connecting-IP`; checked locally against a server
  asking for a SCRAM password too. Its module, now serving, 12.8 MB
  bound and 4.0 MB gzipped (9.5 and 3.0 MB before): past the 3 MB
  Cloudflare documents for its Free plan, inside the Paid plan's 10 MB
  (8.2n measures it on the real plan; 9.2 tunes it).
- [x] **8.2e Route tests on workerd with the same records** `opus ·
  xhigh`, after 8.2d. Built (§2.6, "Route tests on workerd"): a test's
  run one function whichever executor runs it and wherever,
  `jadpo_runtime::test_run::once` (its seed, clock, fakes, record,
  settings, deliveries between steps, crash points and batches run to
  their end, with `Caught`, `rows` and `AtClock` moved there), the
  runner keeping the choosing, the databases, the crash runs and the
  judging, which a run made elsewhere goes through too;
  `jadpo_test::workerd`, a test Worker of a project's compiled programs
  built for wasm32 with the `testing` feature
  (`jadpo_host_workers::testing`, a feature only `jadpo-test` turns on),
  whose `fetch` runs one test by its key on a copy of each store's
  database made on a server it reaches over `connect()` (the run's when
  named by a URL, else one on `127.0.0.1`), answering the record's lines,
  the rows and how the run ended, judged, crashed again and compared with
  the interpreter's (`differential::compare`). Proved in a step of the
  `wasm32` group (the `compiled` job's time ran out with it): area 7's `todos` (3) and teaching example 23's route tests (4)
  passing inside workerd with the interpreter's records. Its module 44.6
  MB bound in the dev profile; 226 s from nothing locally under a load
  average of 150 to 330, 24 s warm (15 s of it binding, 2 s the tests),
  beside the corpus in the same step; workerd installed by one installer
  every Worker test shares, under a lock across processes
  (`jadpo_test::workerd_install`).
- [x] **8.2f The cron tick** `opus · xhigh`, after 8.2d. Built (area
  12, "The tick"; area 14, "A Workers deployable"; §2.6, "The cron on
  Workers"): an `every:` under a minute refused for a Workers deployable
  that runs `Trigger.schedules`, at the schedule's `every:` and naming it
  (`deployable.target_unsupported`); `triggers: { crons: ["* * * * *"]
  }` in the Wrangler file of each Workers deployable whose tick has a
  store (`SchedulesPlan::ticks`); the Worker's `scheduled`, exported and
  handed on by the shim, starting the isolate as `fetch` does and, once
  its stores are current, ticking through `Serving` on a host whose
  ticks share the cron's 15 minutes, then again at each 10-second mark of
  the cron's minute while the app receives a queue, one tick after
  another, ending with the minute. Decided in the chunk (area 12, "The
  tick on Workers", 2026-10-21). Proved under workerd through a service
  binding's `scheduled()` (`crates/jadpo-cli/tests/workers.rs`, the
  `wasm32` group): a depot reading a local SQS through the SQS package,
  out of date logging its migration and leaving the queue alone, then
  keeping and delivering one message, ticking again at its minute's
  50-second mark to receive one sent between, and running its schedule
  once due.
- [x] **8.2g The `trigger:` adapter** `opus · high`, after 8.2f. Built
  (area 12, "The tick" and question 16; area 14, "A native deployable";
  §2.6, "The `trigger:` adapter"): `schedules: { trigger: }`, a
  `ScheduleTrigger`, `native` (the server's own timer, the default) or
  `external`, anything else `schedule.settings_invalid` with the
  nearest, carried by the plan (`SchedulesPlan::trigger`);
  it changes only a native deployable, a Worker ticking on 8.2f's
  cron whichever is written. A native binary takes one command: none or
  `serve` serves, with `external` its server never ticking
  (`AppHandler::untimed`); `<deployable> tick` reads its settings and
  compares its stores' migrations as serving does, waits for no
  database (`store_unreachable`), runs one tick, logs `runtime.ticked`
  (each run by its schedule's name, deliveries, what it received and
  polled, rows deleted, alerts, the stores it couldn't work in) and ends,
  code 1 only when a store couldn't be worked in, the first SIGTERM
  draining it; anything else is `command_unknown`, code 2. No HTTP
  endpoint and no tick secret (Richard, 2026-10-21). `jadpo build` gives
  such a deployable a `tick` line, the command and how often to run it,
  and `--json` its `tick`. Decided in the chunk (area 12, "The `trigger:`
  adapter", 2026-10-21). Proved live (`crates/jadpo-cli/tests/deployable.rs`):
  example 23's office with `ScheduleTrigger.external` serving without
  running its every-second schedule, `office tick` refusing to start
  without `DATABASE_URL`, then running the schedule once and ending with
  code 0, and `office ticks` ending with code 2.
- [x] **8.2h After the response** `opus · xhigh`, after 8.2d. Built
  (area 12, "Running background work"; §2.6, "After the response on
  Workers"): `Host::hand_on` given a `runner::HandOn`, run natively as
  before and on Workers sent as its message, each delivery and batch run
  named by its record and store, to a fresh invocation of the Worker
  through a service binding to its own `JadpoHandOn` entrypoint
  (`JADPO_HAND_ON`, written into the Wrangler file of a Worker running
  subscribers or schedules), which reads each piece back
  (`runner::carry_on`, a delivery run meanwhile left out) and carries it
  on in its own `waitUntil` with 30 seconds of its own; a streamed answer
  sent as a `ReadableStream` pulled as the client reads, a client that
  goes away ending the model's call with the request. Decided in the chunk
  (area 12, "Work handed on, on Workers", 2026-10-21): a request to the
  Worker itself, not a queue. Proved under workerd
  (`crates/jadpo-cli/tests/workers.rs`) by a news desk whose model is a
  server the test starts: a draft's first piece before the model wrote
  the rest, a client going away closing the model's connection once the
  model writes again, and 18
  stories' subscriber deliveries and an 18-item batch run, 2 seconds
  each, past one invocation's 30 seconds and finished by the invocation
  each handed on to, then the minute's cron telling a summary the
  budget's end cut short; natively, work carried on from its message
  alone (`tests/background_budget.rs`).
- [x] **8.2i PostgreSQL through Hyperdrive** `opus · xhigh`, after 8.2d.
  Built (area 5, "PostgreSQL through Hyperdrive"; §2.6): a connection
  names its binding through its `url:` setting, whose variable is a
  Hyperdrive binding on Workers, so `app.jadpo` gains nothing (decided in
  the chunk, 2026-10-21; a text binding stays a direct address);
  `WorkersHost::variable` reads one as its address, of its parts
  percent-encoded, never its `connectionString` with `sslmode=disable`,
  its host the platform's pooler (`Transport::is_pooler`), which the
  driver reaches without TLS: the binding's socket offers none (workerd's
  source), so TLS to the database is Hyperdrive's, by default against the
  public authorities, and the entry's TLS through `startTls()` holds for a
  direct connection only; a certificate Workers refuses at the first
  write after `startTls()` the fault naming the server it is natively
  (`Checked`; unavailable before). `jadpo build` writes each database's
  address as a Hyperdrive binding in the Wrangler file, its id from
  `JADPO_HYPERDRIVE_<variable>`, else none, which `wrangler deploy`
  refuses, the screen's `database` line saying `no id` with an alert to
  create one with caching off, which a Worker can't check (area 14, "A
  Worker's database"). No session state, held by a live test. Proved
  under workerd: a Hyperdrive binding's address and pooler reached in a
  later request; PostgreSQL over TLS through `startTls()` and a refused
  certificate's fault; example 23's kiosk served through workerd's
  Hyperdrive binding to the run's PostgreSQL. Left for 8.2n on the real
  account: the Worker-to-Hyperdrive leg without TLS on Cloudflare's
  network, and tokio-postgres's named statement describing a `vector` or
  `geography` column through Hyperdrive.
- [x] **8.2j Six connections an invocation** `opus · xhigh`, after 8.2h
  and 8.2i. Built (§2.6, "Six connections an invocation"; area 14,
  "Serving"): an invocation's six places (`jadpo_host_workers::Opening`)
  held by its `WorkersHost` and shared by everything it opens, its
  drivers' `WorkersTransport` and `WorkersFetch`
  (`WorkersHost::supplies`), its own `WorkersOutbound` and the work it
  leaves in `waitUntil`; a seventh waiting in the order asked, within
  its own timeout, `NotSent` if that passes first. Counted as Cloudflare
  has since 2026-04-09, six waiting for their answer's head (a `fetch()`
  until its head, a `connect()` until its socket opens), not six held
  open as this entry was written, which would hold a seventh behind a
  store's idle connections; workerd counts none (its server's limit
  enforcer). The isolate's `max_concurrent:` slots a looking set
  (`Slots::looking`): a slot given back handed to the longest waiter,
  which looks every 10 ms on its own invocation's timer (`slots::wait`),
  since wasm-bindgen's executor polls a task in the invocation that woke
  it, and workerd ended the second of two invocations sharing a slot of
  the process's woken set as hung; a holder saying every second it holds
  its slot (`slots::keeping`) and one quiet for five seconds forgotten,
  as Workers cancels an invocation whose client left or whose
  `waitUntil` ran out without dropping what it held. Decided in the
  chunk (area 14 and area 11, 2026-10-22). Proved under workerd
  (`crates/jadpo-host-workers/tests/connections.rs`, the `wasm32` group,
  about 2 s more for its step): eight requests sent uncounted all reaching the server at
  once and counted never more than six, one whose timeout passed while
  it waited `NotSent` and never seen; an echo's `connect()` waiting
  behind six requests' heads; four requests from a response and four
  from its `waitUntil` never more than six at once; a slot one
  invocation gave back taken up by another, its request sent and
  answered in its own; and a slot never given back taken back five
  seconds after its holder went quiet (workerd lets a request whose
  client left run on, so the probe leaves the slot as a cancelled one
  would). The real edge's six is 8.2n's to measure.
- [x] **8.2k Cloudflare Queues' consumer** `opus · high`, after 8.2h
  (from chunk 7.7c2). Built (area 11, "Chosen in chunk 7.7c2"; area 14,
  "A Workers deployable"; §2.6, "Cloudflare Queues' consumer"): the
  Worker's `queue`, exported and handed on by the shim, starting the
  isolate as `fetch` does and handing each batch to `receiving::consume`,
  each message answered on its own (`ack()`, `retry({ delaySeconds })`),
  then the kept events' deliveries in `waitUntil`, handed on past its 30
  seconds; out of date, every message retried after its attempt's delay
  and `queue.retried` logged. `queues: { consumers }` written into the
  Wrangler file (`workers::queued`), and the `.wasm` rule given
  `fallthrough: false`, which Wrangler warned of. Wrangler 4.149.0 pinned
  in `tools/workers/` beside workerd (held to its release by an npm
  override; 70 MB more installed) and installed by the shared installer;
  its `deploy --dry-run` accepts the file without a warning, run with
  every connection refused and reported (`tools/workers/offline.mjs`).
  Wrangler checks the setting names, which were right, and no queue name,
  so Jadpo's form stands for 8.2n. Decided in the chunk (area 11,
  2026-10-22). Proved under workerd through a service binding's
  `queue()` (`crates/jadpo-cli/tests/workers/queues.rs`, the `wasm32`
  group): a warehouse retrying while out of date, then acknowledging a
  text body, a JSON body, a repeat and two messages it parks, delivering
  the two kept after answering, and retrying a batch from a queue it
  doesn't receive.
- [x] **8.2l Passkeys on Workers** `opus · high`, after 8.2a. Built:
  `WorkersPasskeys`, the Workers host's `Host::passkeys`: `webauthn-rs`
  0.5's passkey ceremonies in pure Rust, its options member for member
  with a 32-byte challenge from `getRandomValues`, an answer read and
  checked as it checks one (type, challenge, origin, `crossOrigin` when
  registering, relying party, presence, verification, backup flags,
  counter; CBOR through the same `serde_cbor_2`) and signed with
  `jadpo_value::public_key`'s ES256, DER read strictly, and RS256, the
  key kept as the same COSE bytes the native host writes; attestation
  `none` or packed self-attestation, one signed by a certificate refused
  (area 8, chosen in chunk 8.2l). Held to `webauthn-rs` natively
  (`tests/passkeys.rs`): every ceremony run on both, through the test
  authenticator and about sixty answers made to break one check each,
  kept or refused the same; under workerd a passkey made by the test
  authenticator registered and signed in with twice, keeping what the
  native host keeps of it, another site's answer and a counter gone back
  refused. §2.6's "The Workers host".
- [x] **8.2m What `jadpo check` and `jadpo build` say for Workers**
  `opus · high`, after 8.2b. Built (area 14, "Its target, checked" and
  "A Workers deployable"; area 10, "Client addresses"): a
  `client_address:` source other than `cloudflare` refused for each
  Workers deployable at its `source:`, the deployable's `target:` beside
  it (`deployable.target_unsupported`, in
  `jadpo_check::deployables::target_problems`), which the Worker's start
  still refuses too; `jadpo build`'s screen saying each Worker's caps on
  calls on a `calls` line (`pdf 2 at once in each instance`) and
  `--json`'s `per_instance` for every deployable (`jadpo check`'s "On
  Workers" says it since 8.1o2, which also refuses SMTP on port 25).
  Decided in the chunk (area 14, "What `jadpo check` and `jadpo build`
  say for Workers", 2026-10-21). Proved by the café's kiosk refused each
  of the three other sources on Workers and none natively, area 14's
  third bakery, and the build screen's `calls` in rich, plain and
  `--json`.
- [x] **8.2m2 Rate limits' store on Workers** `opus · high`, after 8.2d.
  Built (area 16, "Rate limits"; "Chosen in chunk 8.2m2"):
  `application { rate_limits: { store: } }` naming a `Driver.redis` or
  `Driver.cloudflare_rate_limits` connection (`rate_limits.store_invalid`),
  the latter on Workers only with a `namespace:`; the plan's route table
  carrying the store (`RateLimitStore`), one binding a limit, named for
  the connection and the limit (`LIMITS_120_PER_60S`) and numbered from
  `namespace:`, which `jadpo build` writes into the Wrangler file's
  `ratelimits`; the Workers host asking the limit's binding
  (`limit({ key })`), a refusal waiting the window's length, and counting
  in the isolate's memory only while the binding is missing or throws;
  `jadpo check` refusing a Workers deployable that serves an API with no
  store (`rate_limits.store_required`), the binding in a native one
  (`deployable.target_unsupported`); every limit counted now is a
  minute's, so the error for another window on the binding comes with a
  route's own `rate_limit:`. Proved under workerd with a
  stand-in binding (`tools/workers/ratelimit-binding.mjs`), since workerd
  has none: the binding deciding in the probe, a broken or missing one
  falling back, and example 23's kiosk counting 120 a minute an address
  in it. Its windows and Wrangler entries are checked on the real edge in
  8.2n.
- [x] **8.3 Performance spike** `opus · high`: one route compiled to native and to
  WebAssembly and measured against an equivalent hand-written Bun and
  TypeScript app, comparing Wasm with native and Bun with native as well
  as Wasm with Bun. Moved from an earlier chunk 4.5, which needed Phase 8's compiled code
  (Richard, 2026-10-14). **Needs Richard** if Wasm is more than 25% behind
  Bun. Measured (§2.11): example 23's `POST /repairs`, Wasm on
  workerd 513 requests a second, Bun 2,052, native 3,554; Wasm leaking
  about 17 KB a booking (8.4e). Richard (2026-10-22): the gap is closed,
  not accepted, in 8.4e and Phase 9.
Chunks from the Phase 8 review, whose findings are on the roadmap:

- [x] **8.4a Phase 8 review fixes: compiled parity** `opus · xhigh`: a
  compiled `match` on a package action's call taking the caught boundary
  (a savepoint) as the interpreter's `Task::Catch` does, so a failed
  package action's writes and events are undone (`jadpo-codegen`
  `subject()` leaving out `Callee::PackageAction`); compiled loops and
  inline-function closures checking the deadline and yielding as the
  interpreter does every 4,096 tasks (area 7, "Time limits"); the
  differential record keeping a failure's site, a fault's site and
  stack, and keyed digests of each outside call's input and each read's
  bound parameters, and recording tests compared on more than their
  verdict; and deep recursion (1,000 frames) tested on a deployable's
  own stack sizes (2 MiB native workers, wasm's 1 MiB), raising
  `recursion_limit` rather than aborting. Each proved by a corpus
  project that fails before the fix. Built: all four (§2.5, "Differential
  test"). A recursion through large frames or `list.map` overflowed both
  deployables before 1,000 frames, a native worker aborting and a Worker
  hanging (V8's own stack, about 1 MB, ran out after 650 levels through
  `list.map`), so native workers get 64 MiB, a
  Workers module 8 MiB with a guard, and inline functions running at once
  are capped at 400 in both executors (area 3, "Recursion", decided
  2026-10-22).
- [x] **8.4b Phase 8 review fixes: Workers background work** `opus ·
  xhigh`: a Queues message the Worker can't take yet (not started, a
  store behind, unavailable) retried with long delays and enough
  attempts to outlast a deploy's migration, and a generated dead-letter
  consumer parking each message for `jadpo deliveries`, as areas 11 and
  12 say; a free connection place handed on when a place is asked for
  (`Placing::poll` calling `hand()`), with 8.2j's reproduction as its
  test; `<deployable> tick` ending with code 1 when its tick panicked or
  outlasted the drain; the hand-on's budget started at the invocation's
  start, its binding following `wrangler deploy --name` and `--env`, and
  a `retry()` that throws not acknowledged; on Workers a Redis
  `rate_limits.store` said by `jadpo check` to count in memory until
  Redis's driver comes, a missing binding logged once, and `limit()`
  given a timeout. Built: retries doubling to an hour over 30 attempts
  (about 22 hours, inside a Free plan queue's retention), each queue's
  dead-letter queue a consumer too (100 retries) whose messages are
  parked as `dead_lettered`, no connection's dead-letter queue another's
  queue; `Placing::poll` handing free places on; `tick` ending 1 when it
  gave nothing; the hand-on through `ctx.exports.JadpoHandOn` rather than
  a service binding (Wrangler keeps a binding's `service` under `--name`
  and `--env`, checked with its dry run), its 30 seconds from the
  invocation's start; a `retry()` that throws ending the handler with an
  error; `rate_limits.counted_in_memory`, declinable, for Redis on
  Workers; the binding not waited for past 500 ms and each fallback
  logged once as `rate_limits.fallback`. Decisions on areas 11, 12 and
  16 (2026-10-22). 8.2n checks `ctx.exports` and the consumers on the
  real edge.
- [x] **8.4c Phase 8 review fixes: what a deployable runs**
  `opus · xhigh`: every deployable serving through the typed per-route
  entries (`Executor::app()`, compiled code's own, used by
  `Serving::handle`), so 8.1p's reading and writing reach production and
  the route tests exercise what deployables run; sign-in's routes,
  signing keys and origin kept only in a deployable that serves an API
  with them (8.1o3's "only the settings those read"); and one start for
  both hosts in `jadpo_runtime::deployable` (the program started, the
  settings read, the stores compared with their migrations, the
  connections not built listed, each tick's result logged as
  `runtime.ticked`), so a store a tick can't work in is logged on
  Workers and on a native server as it is by `tick`. Built:
  `Executor::app`, compiled code's giving its entries, answers every
  request `Serving` handles, and the entries harness holds any executor's
  to them; a deployable keeps a method's signing key only where an API it
  serves takes the method, and the origin only where one takes browser
  sessions (area 14, Decisions); `jadpo_runtime::deployable::start`,
  `compare_stores` and the `Logged` observer serve both hosts, each tick
  logged as `runtime.ticked` with its duration, proved live on example
  23's office and the depot's cron with a swept table renamed.
- [x] **8.4d Phase 8 review fixes: addresses and connections** `opus ·
  high`. Built: `counted_as` keying an IPv6 client's default limits and
  AI budget by its /64; plain HTTP's and email health's "this machine"
  read as an address; `public_address` judging IPv4 inside NAT64, 6to4
  and Teredo and refusing `fec0::/10`; a 10-second native connect
  timeout, each PostgreSQL attempt bounded by `connect_timeout` (10
  seconds unset), start-up's and `tick`'s store checks racing SIGTERM;
  `query_once` resending only when none of the statement was written;
  SMTP to this machine without `STARTTLS` (areas 5, 10, 11 and 16).
  Asked for: rate limits keying an IPv6 client by its /64 (decided
  2026-10-22: stricter, reversible), including the magic-link and
  passkey routes; `127.api.example.com` no longer loopback (the host
  parsed as an address); `public_address` judging the IPv4 address
  inside `64:ff9b::/96` and refusing 6to4, Teredo and `fec0::/10`
  carrying a private one; a native connect timeout, PostgreSQL's
  `connect_timeout` honoured, so start-up and `tick` against an
  unreachable host answer SIGTERM; and an autocommitted statement not
  sent again after a reused connection drops mid-flight; and SMTP
  skipping STARTTLS to a server on this machine (loopback), as
  PostgreSQL does, a remote server's still required and checked
  (Richard, 2026-10-22; a stock Postfix offers it with a self-signed
  certificate, so every send to it is `misconfigured` today).
- [x] **8.4f Before the real deploy** `opus · high`: the release profile
  `jadpo build` writes (link-time optimisation, one codegen unit,
  `opt-level = "s"` for wasm, names stripped), so 8.2n measures the
  module that ships, sizes recorded before and after; a Worker named
  `<app>-<deployable>` so two apps' `kiosk`s don't meet in one account;
  the Wrangler file built as one JSON value in one function, a binding
  never silently dropped; `observability` written if Workers Logs needs
  it (checked against Wrangler's pinned schema). Built (area 14,
  Decisions, 2026-10-22; §2.6, "Before the real deploy", with the
  table): fat LTO in one unit for both hosts, `opt-level = 3` natively;
  a Worker's module bound without its name and `producers` sections, a
  native binary keeping its symbols; `observability: { enabled: true }`,
  since the pinned Wrangler sends none without it. Example 23's Workers
  module 13,503,038 bytes (4,141,531 gzipped) before, 8,062,968
  (2,945,138) after, a booking 8% less CPU; the native binary 21.3 MB
  to 14.8 MB; a rebuild 3.5 s to 63 s natively, 8 s to 33 s for
  Workers. The gate builds its test deployables without LTO
  (`TEST_RELEASE_PROFILE`).
- [x] **8.4g Builds and CI kept quick** `opus · high`: the runtime
  compiled once per Jadpo version, target and locale set (one shared
  cargo target, locale data in a folder named by its digest), which
  tests use too, as §2.5 decided; wasm-bindgen's walrus optimised in
  test builds; `crates/jadpo-cli/tests/workers.rs` split into a file a
  scenario; each CI job's time recorded after. Built (§2.5, "Keeping
  rustc time down"; area 14, Decisions, 2026-10-22): targets in
  `jadpo/builds/` of the user's cache folder, one a host, locale data
  and set of `CARGO_PROFILE_RELEASE_*` overrides, held under a lock
  until the build is copied out, a workspace marking its files changed
  when another project last built its deployable's name. Local, ten-core
  Mac, from nothing: the `wasm32` job's Workers step 505 s (1,407
  CPU-seconds, four 920 MB targets) before, 107 s (294, one 981 MB
  target) after; binding and gzipping a kiosk 2.8 s and 1.7 s before,
  0.8 s and 0.3 s after. The `compiled` job's two native builds carry
  different locales, so it gains nothing and builds two small greeters
  more. CI's times wait for the pull request's run.
- [x] **8.4i Collation for the supported locales** `opus · high`:
  sorting in an unsupported `Locale` ordering by the supported one it
  falls back to (area 10, Richard 2026-10-22), and `jadpo build` keeping
  collation only for the supported locales; the module's size before and
  after recorded. Built (area 10, "Locales"; area 19, "Text"):
  `list.sort_by`'s `locale:` goes through `written_in` as formatting
  does, on every target; `jadpo-cldr` keeps collation as it keeps every
  other marker, the entries the supported locales' lookups walk through.
  Chosen in the chunk (area 10): with no `locales { }` every `Locale`
  sorts by its own and every locale's collation is kept, as before.
  Example 23 on Workers (release, wasm-bindgen's module, gzip -9): as it
  is, declaring no locales, 13,475,196 bytes before and 13,478,712 after
  (4,112,324 and 4,108,944 gzipped); declaring `en_gb`, 13,614,174 to
  12,727,979 (4,154,206 to 3,842,244 gzipped), 0.89 MB less.
- [x] **8.4j `sort_by` by locale requires `locales { }`** `opus · high`:
  `list.sort_by(..., locale:)` a check error without `locales { }`, as
  formatting is (Richard, 2026-10-22), so `jadpo build` keeps collation
  for the declared locales only; chunk 8.4i's "every locale's collation
  with no `locales { }`" replaced. Built (area 10, "Locales" and
  `type.temporal_locales_required`; area 19, "Text"): the call is
  `type.temporal_locales_required` in a project, not for files on their
  own or where `locale:` is already unused for its key; `jadpo-cldr`
  keeps only the root's collation for an app with none. Example 23 sorts
  by no locale; a one-action Workers project declaring no locales
  (release, wasm-bindgen's module, gzip -9) went from 13,017,859 bytes
  to 12,111,677 (3,959,704 to 3,647,526 gzipped), 0.91 MB less.

## Phase 9 — Performance

- [x] **9.1a Instrumentation and focused scaffold:** `opus · high` robust local
  load/verifier instrumentation, generic focused microbenchmark fixtures on
  native and workerd against Bun and Node, p95/cold-start/raw method metadata,
  fault-sensitive correctness checks in the gate. This is not golden-todo
  conformance or an application speed claim; short smoke runs verify the
  instruments only. Method and limits in `tools/bench/README.md`.
  Built: reusable local load/verifier instrumentation and ordinary generic
  fixtures on native, workerd, Bun and Node; short smoke checks validate the
  instruments, without a golden-conformance or application-speed claim.
  Request-path omissions and method in `tools/bench/README.md`; compiler/runtime
  §2.11 records the scope and limits. The original 9.1 remains outstanding
  through 9.1d, 9.1b and 9.1c.

- [x] **9.1d1 Provisioned external JWT subjects:** `opus · high` a generic
  prerequisite of the golden todo: the declared issuer and audience, bounded
  discovery and public signing keys, area 19 verification, and one current
  application-user lookup per request. Public HTTPS providers only, with no
  redirects; trusted issuer metadata may name another public HTTPS key host
  (Richard, 2026-10-22). Native and compiled workerd tests cover algorithms,
  typed subjects, provider failures, cancellation and fresh authority.
  First-sign-in hooks for external subjects and scope declarations remain
  explicitly unbuilt; no application-specific compiler path.

- [x] **9.1d2a Generic runtime contract fixes:** `opus · high` after 9.1a:
  fix the area 5 strict retention cutoff for literal and configured periods,
  and area 11's safe provider-key window for a single changing call, using
  generic regressions including real delivery replay beyond the safe window.
  Preserve keyed replay, failure classification, lease fencing and existing
  single-call applications. A separate branch/worktree and full gate are
  required. Also validate connection URL structure in shared startup/config
  checking, retaining valid explicit local-socket forms and reporting every
  invalid declaration without credentials (Richard, 2026-10-23).

- [x] **9.1d2 Golden todo and executable contract:** `opus · high` after
  9.1d1, delivered through 9.1d2a and 9.1d2b below. Both remain required;
  the split isolates generic defects exposed by the executable contract
  (2026-10-23), without reducing its 44-case scope.
  Built through 9.1d2a and 9.1d2b: generic runtime fixes, the actual golden
  application and all 44 shared cases on native and compiled workerd.

- [x] **9.1d2b Golden todo and all 44 cases:** `opus · high` after 9.1d1
  and 9.1d2a: implement the actual application from `examples/golden-todo`
  using the current area pages, with all 44 case IDs executable on native
  and workerd. Preserve the original candidate and make any correction
  required by a decided area an explicit versioned successor, retaining the
  case and its policy obligation. Include authentication timelines, physical
  query accounting, omission-aware patches, pagination, relationships,
  lifecycle, reminder delivery, preflight and dependency failures.
  Unsupported cases remain visible and block completion; generic fixtures
  do not count. Independent application and harness authoring may proceed
  against an explicitly identified verified prerequisite checkout. Final
  validation uses its own checkout with the exact prerequisite histories
  incorporated, under Richard’s stacked-dependency authorization above.
  Built: the ordinary golden application and versioned executable contract,
  preserving the original candidate. All 44 cases pass on native and compiled
  workerd; the full 89-step gate passes. Exact prerequisite histories are
  incorporated under the stacked-dependency authorization; review and PR merges
  remain pending. Shared local PostgreSQL connection handling and generic
  declared-provider readiness metadata close the contract without application-
  specific compiler paths.

- [x] **9.1d3 Strong TypeScript golden todo:** `opus · high` after 9.1d2:
  implement the same versioned application contract and all 44 cases with
  the specified strict TypeScript, Hono, Zod, Drizzle, CASL and JOSE stack
  on Bun and Node. Audit the reached request path against Jadpo's required
  guarantees, including durable authentication, admission, cancellation,
  transaction retries, idempotency and output validation before commit.
  Declared schemas alone do not establish runtime response validation.
  Its final gate may use the exact unmerged prerequisite histories above;
  dependency review and merges remain separate.
  Built: the strict TypeScript/Hono/Zod/Drizzle/CASL/JOSE application and
  production Node/Bun hosts, durable authentication, admission, cancellation,
  serializable retries, idempotency and output validation before commit. All 44
  cases pass on each of native, workerd, Node and Bun, with 42 TypeScript unit
  tests and the full 90-step gate passing. Existing optional pgvector version
  skips are recorded in the permanent validation facts. General compiler proofs
  and production provider/edge transport remain outside this baseline;
  independent review and performance comparison remain required.

- [x] **9.1d Golden todo and local comparison:** `opus · high` after 9.1a,
  implement the actual golden todo and strong TypeScript baseline from
  `examples/golden-todo` on both sides, establish all 44 acceptance cases,
  audit the reached request path and fix missing required guarantees, then
  compare the complete application plus focused reads, writes, signed-in
  lists, large rows and cold start on native, Wasm under workerd, Bun and
  Node. Validate responses and persisted state, report p95 and throughput
  with repeatable method and honest variability. Phase 10.1's golden todo
  end to end on both targets, its contract cases passing, and independent
  review of the whole first build remain required, as does Phase 10.2's
  agent comparison. Do not substitute generic fixtures for this benchmark.
  Needed before runtime performance changes. Delivered through 9.1d1–9.1d4
  below; sessions take their first unticked concrete subchunk. The split
  changes sizing, not the application or comparison scope.
  Built through 9.1d1–9.1d4: the actual golden app, generic prerequisites,
  its strong TypeScript counterpart and all 44 cases on all four local
  targets. Full application and separate focused comparisons are published
  in compiler/runtime §2.11 with method and busy-host limitations. Phase
  10's whole-build independent review and agent comparison remain required;
  review and PR merges remain pending.

- [x] **9.1d4 Local application comparison:** `opus · high` after 9.1a,
  9.1d2 and 9.1d3: run the complete golden todo plus focused reads, writes,
  signed-in lists, large rows and cold start on native, workerd, Bun and
  Node. Use the same validated contracts, authentication and fixtures;
  check responses and persisted state, and record throughput, p95, cold
  start, memory, exact sources and repeatable method with honest variability.
  Tick 9.1d only after all four subchunks meet its original acceptance.
  Built: ordinary production hosts, strict source/artifact freshness,
  response and persisted-state verification, and complete three-round
  local matrices: 108 golden cells and 72 focused cells, all verified.
  The own-checkout full gate passed 92/92, including all 176 contract
  obligations and 42 TypeScript tests. Measurements are explicitly
  provisional on a busy host; they establish neither edge performance nor
  production capacity. Permanent facts and all-run variability are in
  compiler/runtime §2.11; raw results remain in the ignored harness work
  directories. No PR was merged or set to auto-merge by Codex.

- [x] **9.1b1 Bounded delivery sweeps and the backlog:** `opus · high` after
  9.1d: measure the existing sweep, run deliveries concurrently within a
  recorded bound, and add the 10,000-delivery workload. Keep the decided
  sweep size, connection budgets, leases, retry safety, shutdown and hand-on
  behaviour. Verify responses and durable effects, retain all runs, and
  measure the same local protocol before and after the change.
  Built: four bounded local delivery lanes over the ordinary driver and a
  verified 10,000-delivery workload on native, workerd, Bun and Node.
  Lease fencing, retry safety, sequential descendants, budget/shutdown
  settlement and durable effects remain checked. Matched three-round
  before/after results, CPU trade-offs, unchanged controls, superseded runs
  and exact source identities are in compiler/runtime §2.11. The own-checkout
  full gate passed 92/92; the measured candidate has the same complete tracked
  tree as the gated commit. Exact unmerged prerequisite `64bf4934` is included
  under Richard's stacked-dependency permission; review and merges remain
  pending. No PR was merged or set to auto-merge by Codex.

- [x] **9.1b1a Warm delivery comparison:** `opus · high` after 9.1b1:
  compare local workerd with persistent Bun and Node services, excluding
  process startup and warming their pools consistently. The previous Bun/Node
  fixture launched one process per tick while workerd stayed running, so its
  cross-target result does not establish a warm-service win. Retain that
  protocol and its results, add explicit invocation-lifetime metadata, and
  measure both serial and bounded Jadpo with the same final warm protocol.
  Preserve the durable effects, UUIDv7, leases, atomic handled writes, failure
  controls and strict source/artifact guards. Show Workers/workerd versus Bun
  first; label native's external-tick mode separately if a comparable warm
  native host is unavailable. Record all results, variation and lessons;
  full gate and a separate manual-review PR required (Richard's primary
  comparison clarified 2026-10-23; verifier correction, no language change).
  Built: persistent Bun/Node HTTP tick services, matched pool warmup, explicit
  process-lifetime metadata and real service/SQL recovery controls. Both
  three-round campaigns passed all 24 cells and 40 controls with exact
  durable-effect, replay and freshness audits. Local workerd improved
  1,155→2,239 deliveries/s, with tick p95 107.064→55.206 ms; warm Bun
  remained ahead at 4,017/s and 29.479 ms. The previous startup-asymmetric
  cross-target interpretation is explicitly corrected, with all old
  results retained. Method, source identities, variability and lessons
  are in compiler/runtime §2.11 and `tools/bench/README.md`. The own-checkout
  full gate passed 93/93; the measured bounded tree exactly equals the
  gated tree. Exact unmerged prerequisite `0bf65408` is included under
  Richard's stacked-dependency permission. No PR was merged or set to
  auto-merge by Codex.

- [x] **9.1b2 Fast ticks and hourly housekeeping:** `opus · high` after
  9.1b1a: measure idle-tick statements and latency, separate prompt work from
  an hourly cleanup pass one instance takes under a lease, and batch every
  delete. Preserve retention, handled-record safety, failure recovery and
  multi-instance behaviour; report before/after measurements.
  Built: prompt work remains on each tick; each store has an hourly, identity-fenced
  cleanup lease, bounded deletes and immediate retry after incomplete cleanup.
  Whole-schema cleanup holdings survive deployable pruning, including schedule,
  batch and AI-only stores. Real races, retention and failure controls pass.
  Native and local workerd idle ticks fell from eight SQL statements to five;
  workerd median idle-tick p95 fell from 4.371 to 2.874 ms. The supporting warm
  delivery comparison measured workerd 2,276/s, p95 53.274 ms, against Bun
  3,897/s, p95 30.170 ms; ambient variation prevents a causal delivery-gain claim.
  The final full gate passed 93/93 at `c1d7ab57`; the measured candidate
  `4cb9e82d` has the identical complete tracked tree. Test subprocess ownership
  was repaired; historical orphan-affected runs and failed/superseded attempts
  remain qualified alongside fresh source/artifact identities, all rounds and
  review constraints in compiler/runtime §2.6 and §2.11 and `tools/bench/README.md`.
  Exact unmerged prerequisite `cb1b1a79` is included under Richard's stacked
  dependency permission; Claude's review and merges remain pending. Codex
  opens the PR without auto-merge and never merges it.
