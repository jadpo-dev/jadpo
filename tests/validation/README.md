# Validation entry point

Run from the repository root:

```sh
python3 tools/verify.py
```

This builds the current compiler and every runnable example, runs the Rust
workspace, all compile fixtures, editor checks, the candidate-contract verifier,
authored tests, each generated runtime suite in a separate process, and both
persistence/authentication suites against disposable PostgreSQL clusters.
It requires Rust/Cargo, Bun, Node, Ruby, Python 3.9+, Bash and PostgreSQL tools
(`initdb`, `pg_ctl`, `createdb`, `postgres`) on `PATH`, plus permission to bind
localhost sockets. Bun runs with package auto-install and env-file loading
disabled using an explicit empty environment file (`/dev/null`, or `NUL` on
Windows); Bun 1.2.20 ignores the unsupported `--no-env-file` flag. No existing
application database is used. Persistence and auth/policy integration cases run
against both SQLite and fresh PostgreSQL clusters.

The gate explicitly installs the compiler-pinned JWT test dependency into an
isolated cache before runtime tests. This is a named installation step with the
compiler-owned manifest/lock; Bun runtime auto-install remains disabled. The
service and generated JWT integration suites also run in disposable PostgreSQL
modes. SQLite observes the JWT authority-query count directly; that SQLite-only
instrumentation is reported as skipped in PostgreSQL, not as a PostgreSQL count
proof. Non-JWT generated applications retain zero package dependencies.

`--profile quick` explicitly skips live PostgreSQL and records those skips.
`--require-golden` additionally fails while the full golden behavioural contract
has no executable evidence. The golden compiler diagnostics and every acceptance
case's gap are retained even in a supported-check run. An ordinary successful
run reports `supported_checks_passed_with_open_gates`, never release readiness.

Each invocation writes a fresh `build/validation/<run>/report.json` and individual
logs; failures stop the run and produce a nonzero exit code. No old green report
is overwritten or reused. A missing tool is an error, not a silently skipped test.
The manifest must classify every example and register every runtime test file.
New suites cannot quietly sit outside the command. The separate-process policy
avoids shared environment and generated-module caches contaminating runtime tests.

Historical validation reports linked from the specifications, plans and completion
records are retained with their raw logs at their original `build/validation/`
paths. These selected snapshots are deliberately versioned even though new build
output is ignored. Preserve their bytes and failed results; they describe the
recorded revision, not the current checkout. When a durable record links a new
local run, explicitly retain that run's report, logs and diagnostics in Git or
provide a durable artifact link. Do not force-add the entire generated build tree.
Hosted runs retain their reports as GitHub Actions artifacts.

The [GitHub Actions workflow](https://github.com/richardwillars/jadpo/actions/workflows/validation.yml)
uses the same full command on pushes to `main` and PRs and retains reports even
on failure. Feature-branch pushes do not create a duplicate full validation run.
A manually requested golden-gate run defaults to requiring the currently blocked
golden contract. The public repository is connected as `origin`; the initial
[hosted validation run](https://github.com/richardwillars/jadpo/actions/runs/37386118262)
tests the committed development baseline. Branch protection requires pull
requests with an up-to-date passing `supported-language` check and resolved
review conversations, including for administrators. It blocks force pushes and
branch deletion without imposing a mandatory human approval count. Required
independent implementation reviews still follow the
[project workflow](../../docs/roadmap-workflow.md). CI integration does not
satisfy P10R approval attestation. Action configuration follows the official
[checkout](https://github.com/actions/checkout),
[setup-bun](https://github.com/oven-sh/setup-bun),
[setup-node](https://github.com/actions/setup-node) and
[upload-artifact](https://github.com/actions/upload-artifact) instructions.

## Contract authority and gaps

DATA-007 is the accepted entity model. Legacy syntax still accepted by the
compiler is compatibility evidence, not a second design recommendation. Its
migration/deprecation and public example alignment remain work in the validation
phase; passing compatibility fixtures does not resolve that work.

The [golden obligation map](golden-obligations.json) retains all 44 candidate
case IDs, migrated declarations, responsible tasks and required observations.
The [RM-101 baseline](golden-baseline/provenance.json) preserves original
candidate diagnostics and informational contract digests. No case is marked passed because a related
unit test exists. The [2026-09-30 review](../../examples/golden-todo/REVIEW.md#2026-09-30-authentication-reconciliation)
resolved the four recorded auth conflicts and query-budget ambiguity against
approved AUTH-001 and the owner's explicit principal/credential accounting
choice. The map pins the revised acceptance digest; all cases remain unexecuted
until the full application and harness implement them.

The legacy `examples/test1` scratch source has old syntax and is explicitly
excluded from executable examples. The golden todo is a design contract with
unsupported capabilities, not a passing application. Both classifications are
visible in the manifest/report. The separate `examples/golden-todo-migration`
package is also classified as a design contract while its source migration is
incomplete; its source-check result is never counted as runtime or acceptance
evidence. Migration work must preserve intended behaviour.

## Independent test-authoring work packages

The cross-area evidence gaps selected for the next validation wave are tracked
in the [RM-501 roadmap gap inventory](roadmap-gap-inventory.md). It separates
repository-executable tests from semantic decisions and external evidence, so
later campaigns do not encode unsupported expectations.

The reproducible [RM-502 parser/type/failure campaign](fuzz-campaign.md) runs
as a Cargo integration test and consumes the registered compile-fixture corpus.

The [RM-503 mutation findings](rm503-mutation-findings.md) record five killed
targeted mutations and identify the additional obligation areas still to review.

Agents should derive cases from accepted specifications before consulting the
implementation, state expected results, and cite the contract for each claim.
Assign coherent areas rather than individual keywords or diagnostic strings:

| Area | Required kinds of evidence |
|---|---|
| Lexer/parser/modules | Token boundaries, Unicode byte spans, malformed recovery, EOF/truncation, visibility and cycles |
| Types/values | Nominality, constraints, omission/none, construction, narrowing, field/reference composition |
| Failures/callables | Exact failure sets, attempt/match, control-flow paths, propagation, safe disclosure |
| Persistence/transactions | SQL behaviour, cardinality, integrity, rollback/savepoints, isolation and concurrent mutation |
| Authentication/policy | Cross-user/service boundaries, ambiguous credentials, revocation, secret containment, predicate placement |
| Config/time/fixtures | Startup-before-listen, clocks, time boundaries, secret sinks, fixture isolation |
| Targets/artifacts/tooling | Generated behaviour, stable contracts, diagnostic repair, source mapping, reproducibility |

Every area needs positive, negative and boundary cases plus interactions with
adjacent areas. Unit tests prove local rules; runtime/HTTP/database tests prove
observable effects. Each public diagnostic should have a real trigger, expected
span/message/repair, and a check that its repair preserves the human decision
boundary. Unspecified behaviour is a reported contract gap, not an invented
expectation. Never weaken a test or policy to make the implementation pass.

A separate review should try plausible implementation mutations and explain
which tests catch them. Shared integration cases should cover auth plus policy
plus transaction failure, and config/time behaviour across restarts. Run agents
in bounded waves and integrate each area through this command before starting
the next wave. Three bounded waves now cover syntax/modules, types, failures,
persistence, auth/policy, config/time, artifacts, diagnostic discovery, LSP,
fixture isolation, reviewed SQLite migrations and Unicode length parity, with
independent mutation challenges.
Assignments, findings and integration
status are recorded in [the progress ledger](unattended-progress.md). This
test-authoring work does not satisfy the formal comparative trials or human
comprehension study. A [single internal cold-start pilot](fresh-agent-pilot/findings.md)
passed its twelve frozen black-box obligations and identified actionable
documentation and diagnostic-location problems; it is not a P10R/P12 gate.

The current artifact review also exposed an explicit [Set/Map HTTP wire gap](artifact-contract-findings.md#explicit-setmap-wire-contract-gap):
generated schemas advertise JSON representations that handlers reject. The
accepted contracts do not yet settle key encoding and duplicate handling, so
these boundaries are recorded as unsupported evidence rather than passing
collection parity.
