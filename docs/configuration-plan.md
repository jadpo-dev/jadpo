# CONFIG-001 configuration plan

**Status:** core implementation in progress; CONFIG-P0/P1/P3 complete, P2/P4/P6 core implemented, CONFIG-P5 database liveness/readiness implemented, provider checks and full adversarial evidence pending

**Approved:** 2026-09-27

**Scope:** typed configuration, local secret entry, deployment bindings,
startup validation, liveness, and readiness for generated Bun services

**Out of scope:** hosted secret storage, cloud-specific deployment generation,
live configuration reload, authorization policy, and general orchestration

This contract is intentionally small. An application declares each typed value
and its deployment binding in one place. Jadpo checks the declaration, helps a
developer enter local secrets without exposing them to an LLM, and validates
real values automatically before a service starts.

## 1. Approved v0.1 contract

- Each configuration field, its type, classification, binding name, and
  optional default live together in the application source. There is no second
  Jadpo binding file to keep in sync.
- Field options use a structured body, not positional or space-separated
  modifiers. The exact grammar implemented by CONFIG-P1 follows this shape:

  ```jadpo
  config AppConfiguration {
      mailer_api_key: MailerApiKey {
          binding: "MAILER_API_KEY"
          secret: true
      }

      request_timeout: Duration {
          binding: "REQUEST_TIMEOUT"
          default: 5s
      }
  }
  ```

- A field is required unless it has a checked literal default. Secrets cannot
  have source defaults.
- Application behavior reads only typed `config.<field>` values. It cannot read
  the process environment, `.env.local`, or raw binding metadata.
- v0.1 bindings obtain values from the process environment. Secret managers and
  deployment platforms remain compatible by injecting those environment
  values; Jadpo does not need their SDKs.
- All v0.1 configuration is startup-bound. A change causes a controlled local
  restart or normal production restart/rolling deployment. There is no
  `restart` versus `reloadable` choice in the language.
- Jadpo uses no configuration-loading package. Generated Bun processes disable
  Bun's automatic `.env` discovery in production.
- Local development has exactly one conventional file: `.env.local`. Jadpo
  does not define `.env`, `.env.development`, `.env.test`, or layered overlay
  precedence. Tests receive configuration from the test harness.
- A missing or invalid required value cannot reach application code or a
  serving listener.
- Secret values cannot enter public output, logs, diagnostics, generated
  examples, manifests, health responses, or agent-readable command output.
- Liveness describes the local process. Readiness separately describes whether
  required dependencies can currently serve application traffic.

## 2. Approved decisions

| ID | v0.1 decision |
|---|---|
| CONFIG-D01 | Put the explicit environment binding beside its typed field in application source; do not generate or require a parallel deployment-binding file. |
| CONFIG-D02 | Use one local `.env.local` file and no implicit application-level environment overlays. Production receives the same binding names from its platform environment. |
| CONFIG-D03 | Treat the authored binding string as stable deployment API. A source-field rename does not silently rename it. |
| CONFIG-D04 | Make fields required by default or give them one checked literal default. Secrets cannot have defaults and an empty string is not optionality. |
| CONFIG-D05 | Make `secret` a checked information-flow classification on the typed value; only compiler-owned declared sinks may receive it. |
| CONFIG-D06 | Make every v0.1 value startup-bound. Local changes restart `dev`; production changes restart or roll the service. Defer live reload until a concrete need justifies its extra model. |
| CONFIG-D07 | Add a safe prompted `jadpo config set <field>` handoff and a value-safe `jadpo config check` for local configuration. |
| CONFIG-D08 | Keep `jadpo check` as the one authoritative static check. `build`, `test`, `watch`, and `dev` reuse it rather than introducing partial semantic checkers. |
| CONFIG-D09 | Validate real values automatically in `dev`, deployment integration, and runtime startup. Do not require developers to remember a separate preflight command. |
| CONFIG-D10 | Keep liveness local and dependency-free; make readiness bounded, secret-safe, and false while a required dependency is unavailable. |

## 3. Safe local secret handoff

The preferred LLM workflow is for the agent to name a command, not request the
secret:

```text
Run `jadpo config set mailer_api_key` in your terminal. It will prompt securely;
do not paste the value into chat.
```

Inside a project, the interaction is:

```text
$ jadpo config set mailer_api_key
Enter MAILER_API_KEY: [hidden]
✓ mailer_api_key configured locally
```

`jadpo config set` must:

- resolve the authored field and binding from the checked configuration schema;
- accept the value only from an interactive prompt, never as a command-line
  argument;
- disable terminal echo for a secret and never reproduce its value;
- decode and validate the value before writing it;
- create `.env.local` with owner-only permissions when possible;
- update it atomically, preserve unrelated entries and comments, and refuse an
  unsafe target such as a symbolic link;
- detect a concurrent file change instead of overwriting it; and
- print only the field/binding identity, status, and safe validation errors.

The user may still edit `.env.local` manually. The command is the preferred
agent handoff because it keeps a secret out of chat, shell history, process
arguments, and agent-readable output. It is a local convenience, not a claim
that `.env.local` is a production secret vault.

`jadpo config check` checks whether the current project's local values are
present, decodable, and valid. It reports only field names and
set/missing/invalid status. It is deliberately narrower than `jadpo check`, but
it does not replace static checking.

## 4. Check, development, and deployment flow

| Command or boundary | Responsibility |
|---|---|
| `jadpo check` | Full-project syntax, type, configuration declaration, binding, and secret-flow checking. It never needs real values. |
| `jadpo build` | Runs the same full check, then generates deterministic output. A build never requires local or production secrets. |
| `jadpo test` | Runs the checked build, then tests with explicit harness-provided values. |
| `jadpo watch` | Repeats the same checked build after authored input changes. |
| `jadpo dev` | Runs the checked build, validates `.env.local`, starts only with valid values, and performs a controlled restart after a valid source or local-configuration change. |
| `jadpo config check` | Quickly reports local value presence and validity without building or starting the service. |
| deployment/startup | Validates the actual platform environment automatically before promotion/readiness and repeats validation before serving. |

Agent guidance is therefore short:

1. Run `jadpo check <project> --diagnostic-format=json` after a coherent edit
   when `dev` is not already reporting the same pipeline.
2. Ask the user to run `jadpo config set <field>` for a missing local secret.
3. Use `jadpo config check` only when diagnosing local values.
4. Finish with the relevant `build` or `test`; a separate check immediately
   beforehand is redundant.

A public `jadpo check config` mode is not added. Configuration references and
secret flow cross application boundaries, so a partial semantic check could
give false confidence. If full checks become slow, the compiler may cache and
incrementally recompute the affected graph without creating a weaker command.

## 5. Runtime and deployment boundary

Bun provides `Bun.env`/`process.env` and can disable automatic environment-file
loading, so Jadpo does not need `dotenv`. The compiler generates a closed loader
that reads only declared binding names, decodes them into nominal types, and
then removes the raw map from application reach.

Production launch disables automatic `.env` discovery. The deployment platform
supplies declared environment variables directly, including values injected by
its secret manager. Generated deployment integration validates them before a
new revision is promoted when the platform supports such a hook. Runtime
startup always repeats validation and never binds a public listener with
invalid configuration.

Valid configuration with an unavailable dependency is different from invalid
configuration. The process remains live, readiness is false, required probes
retry with bounded backoff, and application traffic is not served until the
dependency recovers. Probe output contains stable dependency names and states,
not endpoints, credentials, or native driver text.

## 6. Implementation sequence

### CONFIG-P0 — Propagate the approved contract

- update the decision register, roadmap, validation rules, threat model, golden
  source, expected audit, and comparison baseline;
- freeze diagnostic and safe-status vocabulary; and
- record the dependency-free Bun and explicit empty environment-file launch boundaries.

### CONFIG-P1 — Syntax and semantic graph

- parse the structured configuration declaration and field option bodies;
- assign stable field identities and record explicit binding names;
- type-check literal defaults and prohibit secret defaults;
- resolve every `config.<field>` use; and
- reject duplicate bindings, unknown options, raw environment reads, and
  dynamic configuration lookup.

### CONFIG-P2 — Secret flow and generated schema

- propagate secret classification through values, calls, structures, and
  failures;
- allow secrets only at compiler-owned adapter sinks;
- reject serialization, interpolation, logging, diagnostics, metrics, and
  generated examples containing secrets; and
- generate deterministic, value-free configuration audit and binding schemas.

### CONFIG-P3 — Local configuration commands

- implement `jadpo config set` with hidden TTY input and safe atomic updates;
- implement value-safe `jadpo config check`;
- generate and ignore `.env.local` without adding other environment variants;
  and
- make `dev` validate and safely restart after valid local changes.

### CONFIG-P4 — Startup and deployment validation

- generate the closed Bun environment loader without a package dependency;
- disable automatic `.env` loading in generated production launch;
- validate a complete typed snapshot before adapter initialization or listen;
- emit stable secret-safe startup failures; and
- expose the same validation operation to generated deployment integration.

### CONFIG-P5 — Liveness and readiness

- generate distinct compiler-owned health contracts;
- enforce probe timeouts, concurrency bounds, and bounded retry/backoff;
- classify required and advisory dependencies;
- recover readiness without restarting after a dependency recovers; and
- prove ordinary routes cannot acquire deployment-plane details.

### CONFIG-P6 — Tooling, golden, and adversarial exit run

- add LSP hover/completion/diagnostics and agent-oriented JSON;
- add `audit/configuration.json` with declarations, classifications, bindings,
  sinks, probes, and source spans but no values;
- migrate the golden todo and comparison baseline; and
- run missing, malformed, duplicate, redaction, startup, restart, readiness,
  recovery, concurrent-write, unsafe-file, and secret-canary cases.

### Implementation record

As of 2026-09-27, the compiler parses and checks the configuration declaration,
resolves typed `config.<field>` access, tracks secret values, emits a value-free
configuration audit, and generates a closed package-free Bun loader that
validates before `Bun.serve`. The CLI implements hidden prompted
`config set`, value-safe `config check`, `.env.local` validation, declared-only
environment forwarding, and last-known-good `dev` restart behavior. The LSP,
VS Code grammar/snippet, compile fixtures, CLI integration cases, and a runnable
configuration example cover the new surface.

The compiler now generates public, compiler-owned `GET /health/live` and
`GET /health/ready` handlers. Liveness is local and dependency-free. An authored
`GET /health/live` is supported only with `auth: none`, no request inputs or
runtime reads/calls, and one inline return of an output record containing
literals or declared enum constants. Its validated output is returned before
readiness/authentication; unsupported handlers fail closed with a located
`JADPO_TARGET_LIVENESS_NOT_LOCAL` diagnostic. Applications without that authored
route retain the compiler fallback `{ status: "live" }`. For an
application with generated persistence, readiness checks the stable `database`
dependency ID; other traffic gets a generic 503 while that required dependency
is unavailable. Recoverable database connection failures keep the process live;
incompatible schema/configuration failures still stop startup before binding.
Structured SQLite BUSY/LOCKED (including their extended forms) during schema
initialization are recoverable, not a permanent startup-fatal latch. This
initialization rule does not broaden operation-level retry or availability rules.
Readiness retries on probe requests with a single in-flight check, a one-second
healthy cache and retry backoff from 250 ms up to five seconds. The PostgreSQL
probe formerly awaited the native query after requesting cancellation at one
second. The response-bound continuation below separates the caller response from
that native lifetime; Bun 1.2.20 active-call interruption remains unproved.
Recovery repeats idempotent schema initialization
and replaces the active persistence client without a process restart. SQLite's
local schema-version probe is synchronous. Structured operation-level lock or
contention failures do not by themselves mark the whole database unavailable.

The registered recovery suite also starts the generated first-party-auth
application with its SQLite parent directory missing. The process stays live,
readiness reports the database unavailable, and a protected route is gated with
a generic 503. After the directory is created, readiness initializes the
application and authentication schemas; a loopback-only test harness provisions
a user/session through trusted host integration, then the protected identity
route authenticates successfully without restart. Recovery polling eventually
observes thirty-two ready responses with one authentication initialization.
Requests arriving on opposite sides of the retry cutoff may return a mixed
200/503 batch; each response must carry its corresponding safe ready/not-ready
body. This is bounded local concurrency evidence, not a load or hard-deadline claim.

A listener-free generated-handler continuation repeats database gating and
recovery with 32 simultaneous readiness requests and exactly one authentication
schema initialization, then renames the SQLite auth-session table to inject a
post-startup authentication-store outage. The existing credential receives a
safe `authentication_unavailable` response while the table is absent and
succeeds after the table is restored. This directly covers route/auth-storage
recovery without a listener, but does not establish network serving or
PostgreSQL auth outage/recovery by itself. The registered live PostgreSQL case
now repeats the session-table rename after startup and proves the same credential
returns safe 503 during the outage and its original identity after restoration,
without restarting or reissuing. Both signing keys, credential, connection URL,
native error code and raw SELECT are absent from the checked debug-disabled
response/log paths. The auth and persistence fixtures use separate databases
inside one disposable cluster because their User schemas differ. The seven-case
readiness suite passes, including both PostgreSQL cases; the independent
[PG auth regression review](../tests/validation/rm403-independent-postgres-auth-review.json)
accepts this narrow evidence. Ready remains 200 during private-table absence:
availability (and possibly its healthy cache) is not schema-integrity proof.
This table-rename case does not cover connection loss, concurrent PostgreSQL
recovery or hard initialization bounds. Approved disposable host verification cleared
the former socket/shared-memory restrictions. The supported snapshot passes
61/61 checks, including live HTTP and PostgreSQL readiness
([report](../build/validation/20261004T092753-91801/report.json)). The
[independent correction review](../tests/validation/rm403-independent-correction-review.json)
accepts the mixed-batch assertions and transient initialization classification,
with real SQLite initial/recovery-time exclusive locks, same-process recovery,
one initialization and unchanged fatal secret/schema validation. A deterministic
generated probe covers sixteen calls before and sixteen after the retry cutoff.
These scoped fixes do not establish hard probe/whole-initialization latency.
This is a partial CONFIG-P5 implementation. The owner selected advisory,
unprobed mail on 2026-10-04: SERVICE-001 v0.1 has no safe status operation, so
readiness does not contact the mail provider and mail does not determine the
application's ready/not-ready result. This choice does not bypass startup
configuration validation or change the runtime outcome of an attempted mail
delivery. `GET /health/ready` keeps required dependency IDs and states in
`checks` and reports each declared service under `advisories` by its stable
snake-case service ID with the value `unprobed`; advisory state does not affect
the top-level readiness result. A direct generated-handler test proves the
mail status causes no provider connection or fetch. Broader timeout/concurrency-
bound evidence, the golden P6
adversarial matrix, and platform evidence also remain open. AUTH-001 and
SERVICE-001 still own compiler-controlled secret sinks; the approved
[TIME-001/TEST-001 contract](time-testing-plan.md) supplies monotonic deadlines,
stable operation instants and test isolation. Cloud-specific deployment hooks
remain out of scope unless separately approved.

**Connection-loss continuation, 2026-10-05:** the registered disposable
PostgreSQL wrapper now provisions a separate recovery database and control
connection. The test checks the exact loopback protocol/host/port/user/database
pair before refusing new connections and terminating that database's backends.
After healthy-cache expiry, readiness reports safe 503; 32 concurrent checks
remain not-ready, ordinary authenticated traffic is gated and liveness stays
200. Restoring connections yields 32 ready responses and the same credential's
original identity without restarting the process. Required/advisory bodies and
debug-disabled logs retain secret/native-detail checks. The verifier strips
inherited readiness database variables; only the disposable wrapper supplies
the fault-injection pair, restored in cleanup.

The fresh supported [gate](../build/validation/20261005T010805-27481/report.json)
passes61/61; its live readiness suite passes8/8 (4,721 assertions in that run).
The [scoped independent review](../tests/validation/rm403-independent-connection-recovery-review.json)
approves this evidence, rebuilding isolated targets and independently passing8/8.
This is real connection-loss/concurrent
recovery evidence, not an observed PostgreSQL single-initialization count, hard
probe/whole-initialization bound, golden CONFIG-003 or platform qualification.
The [next response-bound plan](work-plans/golden-delivery-planning.md#rm-301307108--services-durable-jobs-and-reminders)
separates caller response deadlines from native-attempt lifetime and requires
late-success fencing rather than treating a cancellation request as interruption.

**Asynchronous response-bound continuation, 2026-10-05:** after
[independent plan approval](../tests/validation/rm403-independent-response-bound-plan-review.json),
generated readiness now separates the shared one-second caller response budget
from the aggregate native attempt. Expiry marks not-ready and returns false even
if best-effort cancellation throws or does not settle the query. The native
single-flight slot remains owned until actual completion; later requests do not
attach more native observers/timers or launch overlapping checks. Late success,
strict deadline equality, a fatal latch and a newer availability invalidation
all prevent client/ready publication. Recovery initialization stages its success
without publishing; candidate creation is fenced again immediately before one
synchronous publication block. Backoff advances once per failure/expiry and
only a fresh eligible success resets it. Genuine late schema failures still latch
fatal; synthetic timeout does not.

Six deterministic groups and the14-case current live SQLite/PostgreSQL readiness
suite pass. The fresh supported
[gate](../build/validation/20261005T012226-44058/report.json) passes61/61;
the [independent implementation review](../tests/validation/rm403-independent-response-bound-implementation-review.json)
approves this narrow slice with no blocking findings. It independently passes
the six controlled groups and ten in-memory probes of the complete emitted
persistence/authentication modules, including late initialization, genuine late
fatal schema failure, false initialization results and newer invalidation. Those
probes use synthetic SQL/logical time, not live latency measurements. This bounds asynchronous caller response under a progressing event-loop
scheduler, not synchronous SQLite/event-loop blocking, native operation lifetime,
initial startup or whole initialization hard latency. A permanently stalled native
attempt deliberately keeps readiness false with no overlapping replacement.
Mail remains advisory/unprobed and startup configuration/schema validation still
precedes listener binding. These limits and the remaining golden P6/platform
gates are not closed by the response-controller slice.

## 7. Required evidence

| Area | Required positive and adversarial evidence |
|---|---|
| Static contract | Typed required/default fields; reject unknown options, duplicate bindings, invalid/secret defaults, direct environment access, and dynamic lookup. |
| Local handoff | Hidden interactive entry and atomic preservation; reject argument values, non-interactive secret entry, unsafe targets, concurrent changes, and all value disclosure. |
| Secrets | Reach only declared adapter sinks; canaries never enter output, logs, diagnostics, metrics, manifests, health, URLs, or provider errors. |
| Build | Identical artifacts regardless of actual values; no secret access, external package, or implicit environment-file load. |
| Startup | Valid values construct one snapshot; missing or malformed values never bind a listener or expose native errors. |
| Restart | Valid local changes restart `dev`; invalid changes retain the last ready revision and visibly report stale state. |
| Readiness | Required/advisory classification, bounded checks, outage behavior, and recovery without a liveness restart loop. |
| Deployment | Invalid values block promotion/serving automatically; a healthy prior revision remains available where the platform supports rolling deployment. |

## 8. Dependencies and stop conditions

- **AUTH-001** consumes signing keys, issuer/audience values, and credential
  lifetimes through this configuration boundary.
- **SERVICE-001** consumes provider endpoints and secrets through declared
  sinks; CONFIG-001 does not authorize network egress.
- **TIME-001/TEST-001** now supplies the approved clock, monotonic-deadline,
  evidence-timestamp, deterministic-test, and fake-capability foundation.
  SERVICE-001/ASYNC-001 still own retry/backoff policy.
- **DX0.5** supplies the checked local restart and last-known-good harness that
  CONFIG-001 extends.

Return to the owner before adding a new source kind, implicit overlay, live
reload mode, general secret sink, hosted secret SDK, configuration package
dependency, or cloud-specific deployment contract. Do not weaken automatic
startup/deployment validation merely to make a deployment pass.
