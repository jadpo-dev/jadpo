# Independent configuration, time and fixture validation

## Contract expectations recorded before implementation inspection

Authority: `docs/configuration-plan.md` §§1, 2 (CONFIG-D05/D06/D09), 5;
`docs/time-testing-plan.md` TIME-D04/D05/D07, TEST-D01/D02/D07 and §§3.1–3.4,
4.2; `docs/grammar-v0.1.md` §8. The unattended ledger limits this package to
implemented capabilities and requires explicit gaps for unsupported surfaces.

- Invalid/missing required configuration cannot reach application code or a
  listening server. Raw secret values must not appear in startup errors.
- Configuration is typed and startup-bound; fixture/process restart evidence
  must not confuse an old initialized instance with a new configured instance.
- Secrets may flow only to declared compiler-owned sinks, including through
  aliases, records and calls. Source defaults cannot smuggle a secret into an
  artifact or bypass the classification.
- Instants accept strict explicit-zone RFC 3339 input and normalize to UTC
  milliseconds. Invalid dates, missing offsets and arithmetic overflow reject
  at the relevant boundary; no host-specific silent wraparound.
- Each top-level operation has a stable time snapshot shared by nested actions;
  later top-level operations may have a new one. Pure code cannot read it.
- Every authored test receives isolated configuration, clocks and state,
  including after another test fails. Test order must not affect outcomes.

These are expected behaviours, not passing claims. Implementation inspection,
non-duplicate test selection and focused execution follow this record.

## Added cases and findings

`jadpo/crates/core/tests/validation_config_time_contract.rs` adds **11 compiler
integration tests**. They cover secret aliases, explicit reconstruction,
ordinary record/callable sinks, comparison disclosure, secret defaults, fixture
classification and constraints, pure-clock restrictions, test-only clock
advancement, and timestamp calendar/offset/precision/range boundaries. The
constructor and temporal secret regressions additionally pin exact diagnostic
source slices. Positive timestamp endpoint cases prevent rejection of every
offset from satisfying the negative tests.

`tests/runtime/validation-config-time.test.ts` adds **nine runtime tests**.
They compile temporary authored applications with the current compiler and use
synthetic secrets. The fixture cases exercise a deliberate assertion failure
after a millisecond clock advance across leap-day midnight, then other fixtures,
reordered tests, repeated runs and fresh processes. The failure's exact source
span must identify the deliberate `false` assertion, so an earlier failed clock
assertion cannot masquerade as success. Reports must retain callable evidence
classification and redact the secret canary.

Startup cases reject missing/invalid secret and non-secret configuration,
verify ambient environment files cannot supply it, and start two real local
HTTP processes with different configurations to prove restart loading and
stable nested operation snapshots. In-process generated request boundaries
check malformed timestamp input, offset normalization, leap midnight,
millisecond arithmetic, portable endpoints and contained overflow.

### 1. Explicit construction removed secret classification

This compiled without diagnostics before repair:

```jadpo
type ApiKey = Text { min_length: 8 }
config Settings {
    api_key: ApiKey { binding: "WAVE_KEY" secret: true }
}
function leak() -> ApiKey {
    var alias = config.api_key
    return ApiKey(alias)
}
```

CONFIG-D05 forbids this disclosure. `infer_invocation` now carries the input's
secret classification into a nominal constructor result. Repeated constructors
cannot remove it; the ordinary return boundary emits `CONFIG_SECRET_FLOW` at
the returned construction. This preserves typed local validation without
inventing a general secret sink or declassification operation.

### 2. Temporal helpers bypassed secret sink checks

A secret `Instant` configuration field passed to
`temporal.in_zone(config.private_time, Zone.europe_london)` produced an ordinary
public `Time`, without any diagnostic. Temporal representation checks did not
consult the information-flow classification. The shared temporal argument
checker now rejects secret arguments with `CONFIG_SECRET_FLOW`: these helpers
are not approved adapter sinks. Ordinary callables, record construction,
comparisons and conditional/assertion checks already enforce secret boundaries;
the regression suite checks the relevant neighbouring value paths rather than
replacing the flow model.

### 3. Constant timestamp validation disagreed with runtime validation

`Instant("2026-01-01T12:00:00-00:00")` compiled even though the runtime rejects
the unknown local offset. Known offset literals could also normalize outside
the existing portable UTC year range: `0001-01-01T00:00:00+01:00` underflows it,
and `9999-12-31T23:59:59-01:00` overflows it.

The semantic timestamp validator now rejects unknown offsets and checks the
normalized endpoint range. Since supported offsets are less than one day,
only the minimum and maximum calendar dates need this bounded calculation.
Valid adjacent endpoints, including fractional milliseconds, remain accepted.
These changes implement existing strict Instant/constant-validation behaviour;
they add no date syntax or arithmetic features.

### 4. Runtime normalization produced an invalid canonical Instant

The initial HTTP-boundary regression returned 500 for the minimum-year offset
case with a zero duration. Initial decoding accepted the local year 0001,
normalized it to year 0000, and downstream temporal validation rejected that
supposedly checked value. The integrating parent added a canonical year-range
check to the generated decoder. The lasting test now requires 400 at the input
boundary for offset underflow/overflow, 200 for valid endpoints, and idempotent
decoding of every accepted canonical result. Arithmetic overflow from a valid
input remains a separate contained 500 runtime fault.

### 5. The installed Bun ignored the intended environment-file suppression

Under actual Bun **1.2.20**, a clean process with a synthetic `.env` file printed
`{"loaded":true}` when launched with `--no-env-file`. The same process printed
`{"loaded":false}` with `--env-file=/dev/null`. No secret value was printed.

The first unrestricted runtime test therefore started a service from ambient
files rather than failing for missing declared environment values. A sandboxed
run initially masked this by rejecting socket binding; passing a startup-error
check under that sandbox was not proof of configuration isolation. The actual
local-listener run exposed the error and retained the failing expectation.

The parent corrected controlled launch sites to use an explicit empty
environment file, with `/dev/null` on Unix and `NUL` on Windows, while retaining
the pinned Bun version. This suite uses the same portable argument. It tests
actual Bun process behaviour; it does not claim a new `jadpo run` command or a
full black-box `jadpo dev` test. Existing CLI/watch argument tests and the common
gate cover those integrating launch paths separately.

## Verification and proof limits

Focused commands:

```sh
cargo test --manifest-path jadpo/Cargo.toml -p jadpo-core --test validation_config_time_contract -p jadpo-semantic --lib
cargo build --manifest-path jadpo/Cargo.toml -p jadpo-cli
bun test tests/runtime/validation-config-time.test.ts
```

The Rust command passes **11 new cases, 34 semantic unit tests and 57 core unit
tests**. The unrestricted runtime run passes **9 tests and 101 assertions**.
Retained logs, initial static failures, a fresh boolean-only Bun environment-file
counterexample and the summary are in
[`build/validation/config-time-review/2026-09-30-wave2/`](../../build/validation/config-time-review/2026-09-30-wave2/).
No test is ignored or weakened to accept a known bad behaviour. Startup
subprocesses have deadlines, and successful listeners are terminated in
`finally` blocks. The parent runs the full common gate separately.

This package does **not** prove readiness under external dependency outages,
monotonic deadline recovery, fixture database isolation, all secret paths,
concurrent harness executions, job boundaries, authentication fakes, hosted
deployment integration or cross-backend parity. Those remain separate work.
Its startup observations are exit status and structured readiness/failure
events; it does not sample every transient socket state. Nested clock stability
uses both a declared fixed fixture and actual HTTP operation values. This is
not an adversarial host wall-clock adjustment experiment.
