# Jadpo compiler workspace

This Rust workspace implements the phases tracked in
[the implementation roadmap](../docs/roadmap.md).

Milestone A is complete: syntax analysis, declaration indexing, nominal type
checking, constraint validation, and failure/effect checking are implemented.
P8 derived artifact generation and the static base scaffold are complete. P9
TypeScript/Bun target generation is complete for the Jadpo seed. The P10
persistence core is complete: entity identity/unique/index metadata and
parameterised SQLite/PostgreSQL create, optional/required query, update, and
delete paths execute end to end. Explicit owning references generate nominally
checked foreign keys, lifecycle actions, dependency-safe table order, and
lookup indexes. Named inverse collections and explicit required-parent includes
compile to validated nested output through a bounded two-query batch whose plan
is recorded in the persistence manifest. Included to-many collections require
deterministic ordering plus explicit positive `limit` and non-negative `offset`
bounds, emitted as SQL parameters on both adapters.
Many-parent includes page parents in a CTE before a single ranked-child join,
then regroup the flat rows into validated nested output without an N+1 path.
Repeated includes use bounded independent plans and merge by a deterministic
parent key, so multiple to-many relationships cannot create a Cartesian row
product. Fixed-shape multi-field updates are checked field-by-field and lowered
to one parameterised statement inside the automatically inferred action
transaction.
Omission-aware `patch:` updates accept direct input records whose fields are all
optional and nominally matched to entity fields. Generated SQLite/PostgreSQL
statements use fixed supplied-flag/value pairs, preserving omission separately
from explicit `none` without request-shaped SQL.
Patch updates may add fixed derived `set:` values, including a narrow
`when patch.field supplied` condition that reuses the same supplied flag in the
single generated statement. Patch/derived field overlap is rejected.
Required-child queries may explicitly include one owning parent with
`include: relationship required|optional into: Output`. An owning reference can
declare that logical name with `references User.id as owner` while retaining a
stored `owner_id` field. The compiler checks reference
nullability and the exact nested output, then generates a bounded child lookup
plus referenced-parent lookup and records that plan in persistence metadata.
Unique-backed optional inverse declarations provide the opposite zero-or-one
direction. They compile to one bounded parent lookup plus one optional child
lookup, returning `none` when absent and rejecting non-unique owning fields.
A bounded depth-two include may compose a non-nullable owning-parent hop with
one optional inverse. The generated three-query plan records its path and
maximum depth explicitly; other nested shapes remain rejected.
Named compound uniqueness constraints and constraint-specific conflict
bindings compile to stable SQLite/PostgreSQL schema identities. Native failures
are normalised to compiler-owned names before business logic selects a typed
domain failure; raw database identifiers remain behind the adapter boundary.
The live Postgres 16 suite passes all 22 cases. Generated PostgreSQL clients use
non-pipelined `prepare: false` mode so constraint failures reliably complete
automatic rollback on the supported Bun 1.2.20 runtime while retaining bound
parameters.

The language-learning completion slice now includes plain and data-carrying
closed `enum` declarations, exhaustive typed `match`, optional `some(value)`
narrowing, arithmetic/ordering/Boolean operators with fixed precedence, and
authored `test`/`assert` blocks. Generated boundaries validate tagged payloads
and publish discriminated OpenAPI unions.

The DATA-007/TX-001 foundation adds first-class identity-bearing entity
dossiers with optional authority persistence, compiler-owned `.Ref` types,
reference/value receivers, entity-owned functions/actions, and named read-only
queries with explicit freshness. The checker enforces mutation ownership,
persistence capability, project roles, value-receiver guards, explicit
multi-owner consistency, and same-domain atomicity. Builds emit
`audit/entities.json` and `audit/transactions.json`. Generated SQLite and
PostgreSQL clients join nested actions to the enclosing transaction with
compiler-owned savepoints; entities declaring derived representations also
record per-entity revisioned authority changes in the same local transaction.
Physical derived-store delivery and durable-workflow execution are intentionally
fail-closed until their adapter and operational contracts are decided.

AUTH-P1a adds one project-wide `application` authentication default and one
closed `principal` declaration with required user and service variants.
Immediate and positive bounded revocation contracts are checked and exposed in
the semantic manifest. AUTH-P1b adds named strategies, reserved cookie or
authorization-header credential slots, and signed, opaque, API-key, or JWT
validators linked to the closed principal variants. AUTH-P1c adds explicit
claim mappings and authoritative user/service resolution with initial uniqueness,
active-state, nominal-type, and completeness checks. Principal construction is
not generated yet, and protected routes continue to fail closed.

```text
cargo run -p jadpo-cli -- new /tmp/example_application
cargo run -p jadpo-cli -- check ../examples/jadpo-seed
cargo run -p jadpo-cli -- check ../examples/jadpo-seed --diagnostic-format=plain
cargo run -p jadpo-cli -- check ../examples/jadpo-seed --diagnostic-format=json
cargo run -p jadpo-cli -- watch ../examples/jadpo-seed --diagnostic-format=json
PORT=3000 cargo run -p jadpo-cli -- dev ../examples/jadpo-seed --diagnostic-format=json
cargo run -p jadpo-cli -- check ../examples/module-seed
cargo run -p jadpo-cli -- inspect ../examples/jadpo-seed
cargo run -p jadpo-cli -- artifacts ../examples/jadpo-seed
cargo run -p jadpo-cli -- build ../examples/jadpo-seed
cargo run -p jadpo-cli -- build --target native
cargo run -p jadpo-cli -- build --target wasm
cargo run -p jadpo-cli -- test ../tests/compile/pass/58_authored_tests.jadpo
cargo run -p jadpo-cli -- config check
cargo run -p jadpo-cli -- config set mailer_api_key
cargo run -p jadpo-cli -- fmt ../examples/jadpo-seed --check
cargo run -p jadpo-cli -- lsp
cargo run -p jadpo-cli -- schema init ../examples/persistence-seed
cargo run -p jadpo-cli -- schema check ../examples/persistence-seed
cargo run -p jadpo-cli -- schema snapshot ../examples/persistence-seed /tmp/schema-before.json
cargo run -p jadpo-cli -- schema diff ../examples/persistence-seed --against /tmp/schema-before.json
cargo run -p jadpo-cli -- schema decision-template ../examples/persistence-seed --against /tmp/schema-before.json /tmp/schema-decisions.json
cargo run -p jadpo-cli -- schema decision-check ../examples/persistence-seed --against /tmp/schema-before.json /tmp/schema-decisions.json
cargo run -p jadpo-cli -- schema plan ../examples/persistence-seed --against /tmp/schema-before.json --decisions /tmp/schema-decisions.json --adapter postgres /tmp/migration-plan.json
cargo run -p jadpo-cli -- schema sql ../examples/persistence-seed --against /tmp/schema-before.json --decisions /tmp/schema-decisions.json --adapter postgres /tmp/migration-sql-review.json
cargo run -p jadpo-cli -- schema index-recommend ../examples/persistence-seed
cargo run -p jadpo-cli -- schema index-accept ../examples/persistence-seed Customer.id
```

Human commands use a rich source-and-repair presentation when their output is
attached to a capable terminal. Pipes and redirected output automatically use
deterministic ANSI-free plain text. Force a presentation with
`--diagnostic-format=human|plain|json`, control colour with
`--color=auto|always|never`, set `NO_COLOR` to disable colour (including a
forced colour request), or set `JADPO_ASCII=1` to replace Unicode decoration.
Machine integrations consuming `check`, `watch`, or `dev` diagnostics should
request `--diagnostic-format=json`; other machine-oriented commands already
emit their JSON contracts directly.

- `new` creates the byte-stable scaffold without overwriting a non-empty
  destination.
- `check` validates syntax, declaration names, nominal types, constraints,
  failure propagation, rejection payloads, disclosure contracts, and the
  initial function/action effect boundary. It also enforces immutable ordinary
  parameters and bindings: only a local declared with `var mut` may be
  reassigned, and its established nominal type remains fixed. A project must
  contain at least one declaration, although individual files in a multi-file
  project may be empty. Modular projects additionally require explicit unique
  module headers, selective imports of public
  declarations, and an acyclic module graph; legacy header-free projects keep
  the original ambient namespace.
- `config check` reports only whether declared local values are set, defaulted,
  missing, or invalid. `config set <field>` prompts in the terminal (hidden for
  secrets), validates the value, and atomically updates `.env.local`; it never
  accepts the value as an argument.
  `--diagnostic-format=json` emits one version-2 machine-readable report with
  the check status, summary counts, and ordered diagnostics. Each diagnostic
  includes severity, stable code, message, nullable source/byte range, and
  notes. Human-readable output remains the default and renders available source
  as line/column, excerpt, caret, and repair notes.
- `inspect` emits the deterministic checked manifest with graph nodes, source
  spans, refinement and call edges, inferred expression types, failure
  contracts, disclosure fields, derived route failures, and module/import/export
  metadata.
- `artifacts` writes the semantic manifest, inventories, audit, validator plan,
  compatibility snapshot, and OpenAPI subset beneath `build/`.
- `build` checks the frontend, refreshes all derived artifacts, and writes the
  dependency-free Bun target to `build/target/app.ts`. Generation rejects bare
  package imports and dependency manifests. Every runnable target includes a
  compiler-owned `GET /health` endpoint returning `{ "ready": true }`; an
  authored public `GET /health` route replaces that default. Artifact and target
  files are first completed in a sibling staging directory; promotion replaces
  the complete `build/` revision and restores the prior revision if promotion
  fails.
- `build --target native|wasm` builds the bounded shared-Rust conformance fixture
  from a repository checkout without Bun. `rust` aliases `native`; an explicit
  project path may also select `../experiments/auth-policy/fixture` for the
  authenticated owner/editor policy slice. Other project paths are rejected. This is a local experiment,
  not a general backend or a change to the default Bun target. See the
  [experiment build guide](../experiments/native-conformance/README.md) for
  dependencies and artifact paths. Python still performs experimental lowering;
  Cargo compiles the shared core and selected adapter.
- `test` builds the project and executes top-level authored `test` blocks with
  Bun. It prints one compact versioned report and never exposes a JavaScript
  stack trace for assertion failures.
- `fmt` deterministically normalises indentation, blank lines, comments, and
  the final newline. `--check` reports drift without writing files.
- `lsp` runs the compiler-backed language server over standard input/output.
  It provides live unsaved diagnostics, symbols, definitions, references,
  inferred-type and callable-outcome hover, outcome-aware callable signature
  help, contextual completion, semantic tokens, conservative rename, and
  canonical formatting to VS Code or any standard LSP client. Callable outcomes
  show source-level success, failures, and completion semantics without target
  `Promise` or `Result` wrappers. Outcome-match call sites show each handled,
  mapped, or propagated failure, and inline actions show their route-derived
  contract. Compiler byte ranges are converted to UTF-16 editor positions.
- `watch` performs an immediate checked build, then watches authored `.jadpo`
  files and `schema.identities.json`. Polling snapshots coalesce rapid saves
  after a short quiet window and ignore `build/` plus compiler staging/backup
  directories. Every valid revision is promoted through the same build path;
  invalid revisions retain the last complete output and are reported as stale.
  Human lifecycle output is the default. `--diagnostic-format=json` emits
  version-2 `checking`, `build_succeeded`, `build_failed`, and `watch_failed`
  events with monotonic sequence and revision numbers.
- `dev` adds generated Bun execution to the same loop. It reads `PORT` (default
  `3000`), starts `bun --no-install`, waits up to three seconds for HTTP 200 from
  the compiler-owned health endpoint, and emits runtime-starting, ready, restart, and failure
  events. Invalid source revisions do not stop an already-ready process;
  successful builds trigger a controlled Bun restart. Bun standard output is
  redirected to standard error in JSON mode so standard output remains a pure
  lifecycle stream. Generated runtime faults are compact JSON diagnostics and
  omit target stacks unless `JADPO_DEBUG_TARGET_STACKS=1` is set. Startup
  rollback preserves and restarts the last-known-good generated revision when a
  replacement fails readiness. Portable interruption stops and waits for the
  child runtime before emitting a structured `shutdown` event.
- `schema init` writes a canonical checked-in `schema.identities.json` for
  persistent entities, fields, constraints, and indexes. It refuses to
  overwrite an existing registry. `schema check` and ordinary compiler commands
  reject registry/source drift, `schema add` registers additions only, and
  `schema rename <project> <entity|field> <old> <new>` preserves immutable IDs,
  physical names, and prior paths. `schema snapshot` creates an immutable
  checked-shape comparison input; `schema diff` emits deterministic identity
  additions/removals, logical/physical renames, incompatible changes, required
  versus nullable field additions, and field-shape changes with
  `migration_plan: false`. Blocking changes include typed, unresolved
  existing-data or lifecycle requirements, the admissible strategies, and the
  evidence each strategy must provide. `schema decision-template` creates a
  non-overwriting authored artifact bound to the complete canonical change set;
  `schema decision-check` rejects stale, missing, duplicate, incompatible,
  unresolved, or incomplete decisions. Successful validation still emits
  `migration_plan: false`; migration SQL is not generated.
  `schema plan` consumes a successfully validated decision artifact and writes
  an immutable, ordered PostgreSQL or SQLite review plan. Every step includes a
  rollback classification; destructive, anonymising, and transforming choices
  are listed as irreversible. Plans are deliberately marked
  `executable: false` and `sql_generated: false` until evidence expressions are
  compiled and checked.
- `schema sql` provides the first fail-closed SQL review subset. It emits
  escaped PostgreSQL/SQLite forward and rollback statements for nullable field
  additions and PostgreSQL required-field additions backed by a type-checked
  primitive `literal(...)`. A bounded SQLite rebuild coalesces compatible
  primitive additions on each unrenamed table: required fields receive their
  independently checked literal backfills and nullable fields receive `NULL`
  during one shadow-table copy. It preserves existing scalar references, their
  delete actions and lookup indexes, named compound uniqueness that does not
  include an added field, and explicit indexes while performing rollback
  reconstruction and foreign-key integrity checks. The artifact remains
  non-executable and requires review. A required-to-nullable change is
  classified separately as a safe widening: PostgreSQL drops `NOT NULL` and
  SQLite uses a reversible table rebuild. Nullable-to-required narrowing
  requires a `validate_existing` decision with the compiler-owned `not_null`
  predicate; PostgreSQL emits a null-count review plus enforcing DDL and SQLite
  enforces the copy through a required shadow-table column. Constraints on
  added fields, other shape transforms, removals, and lifecycle changes are
  rejected rather than represented by placeholder SQL.
- `check` warns when checked query predicates or ordering fields lack an
  identity, uniqueness guarantee, declared index, or reference-generated index.
  `schema index-recommend` emits deterministic static-query evidence without
  changing source. `schema index-accept <project> <Entity.field>` accepts one
  current recommendation, adds the explicit `index` modifier, checks the
  result, and registers its persistent schema identity. The edit is restored if
  checking or identity registration fails. This is static guidance, not a
  substitute for production cardinality, workload, or `EXPLAIN` evidence.

Run the generated seed and its real HTTP acceptance suite with:

```text
PORT=3000 bun --no-install ../examples/jadpo-seed/build/target/app.ts
cd ..
bun --no-install test tests/runtime/jadpo-seed.test.ts
bun --no-install test tests/runtime/startup-failure.test.ts
bun --no-install test tests/runtime/outcome-sequencing.test.ts
bun --no-install test tests/runtime/persistence-seed.test.ts
DATABASE_URL=postgres://postgres@127.0.0.1:5432/postgres \
  bun --no-install test tests/runtime/persistence-postgres.test.ts
```

No `bun install` step exists. The generated target uses only Bun/Web runtime
capabilities and compiler-owned relative modules.

The P9 target supports the checked core grammar and explicit public routes.
Attempting to generate a route that relies on the future default-authentication
runtime fails with `JADPO_TARGET_AUTH_NOT_IMPLEMENTED` instead of silently making
it public.

Source discovery recursively finds `.jadpo` files but never traverses
`build/`. Generated output is never written into authored source directories.
