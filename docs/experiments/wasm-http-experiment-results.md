# Wasm boundary and HTTP experiment — 30 September 2026

The subsequent [value-ownership/speed-build pass](wasm-value-path-results.md) found
a repeatable concurrent small-read win over generated Bun. Larger returned values
and severe durable-write tails remain concerns. This report is the preceding
checkpoint.

**Recommendation: retain Wasm as a credible backend candidate, keep Bun as the
working target, and focus the next performance investigation on larger returned
values. Small-read overhead is now modest; bulk-value transport still matters.**

This owner-requested extension profiles the remaining JSON and validation work
following [instance reuse](wasm-optimization-results.md), and measures the generated
Bun app and both Wasm candidates behind the same local HTTP server boundary.
It does not change the production backend or select a Rust/direct compiler route.

## What the profile established

On the previous pooled/cached path, the diagnostic small-read measurements found
roughly 7.4 µs inside guest start (input decoding, validation, description creation
and pending-output encoding), 3.2 µs inside guest resume, and 9.9 µs in the checked
SQL host call. The host's policy canonicalization alone took about 4 µs in its
isolated control. An unnecessary Unicode character count took about 16.4 µs for
an unconstrained 16 KiB text value. These are instrumented or isolated means,
not additive shares of uninstrumented p95 latency.

JavaScriptCore's sampling profile placed JSON stringify and parse at the top of
its named functions, alongside SQL calls, Wasm functions and validation. The
stripped Wasm profile does not identify individual Rust source functions. The
codec microcases used already serialized strings, so they understate the entire
serialization path. We did not conclude that validation itself can be removed.

Two candidate variants were tested separately and together:

- The compiler parses constant policy descriptions once per Wasm instance and
  clones those private constants before inserting request values. No request or
  principal data enters the retained constants.
- The host checks incoming descriptions against a precompiled exact structure,
  avoiding sorting and stringify on every call. It also skips character counting
  for text without length constraints. All declared constraints and policy checks
  remain enforced.

The application still executes inside Wasm and uses the same JSON ABI, pooled
instance driver and SQL capability boundary. There is no binary ABI or native FFI.

## Isolated application measurements

Five rotated repetitions per variant and workload, 0.2 seconds warmup and one
second measured. These use native application arguments; the generated Bun
baseline does not perform the earlier experiment's artificial wire roundtrip.
They therefore isolate this extension's changes rather than replacing the prior
benchmark protocol.

| Variant | Small probe operations/s | Small p95 ms | 16 KiB row operations/s | Row p95 ms |
| --- | ---: | ---: | ---: | ---: |
| Generated Bun | 40,008 | 0.029708 | 31,244 | 0.037417 |
| Previous optimised Wasm | 28,901 | 0.042292 | 10,885 | 0.105625 |
| Guest constant cache only | 31,424 | 0.039500 | 11,113 | 0.104542 |
| Host checks only | 34,923 | 0.035875 | 14,378 | 0.083167 |
| Combined candidate | 37,560 | 0.033458 | 14,899 | 0.080958 |

The small probe is now close to Bun. Larger returned values still show a substantial
Wasm cost despite improved host validation. JSON encoding/decoding, copying and
generic `serde_json::Value` ownership remain plausible targets; this experiment
does not separate all of those remaining costs or promise a particular improvement.

## HTTP protocol and interpretation

The Node 24.18.1 load generator and Bun 1.2.20 server run in separate processes
on the same workstation.
Only one target server runs at once. Generated Bun, the earlier Wasm module and the
new candidate use the same HTTP parser/serializer, response envelopes, trusted
fixture owner, seeded rows and on-disk SQLite durability settings (DELETE journal,
FULL synchronous). Existing generated Bun persistence and transaction handling are
retained; both implementations enforce their own storage and policy paths.

Five repetitions rotate targets and alternate workload order, at concurrency one
and sixteen, with one second warmup and three seconds measured. Every response is
checked, and post-run snapshots check persisted values, paired updates and intact
unrelated rows. A no-work HTTP control handles the same read requests and responses
without application or database execution, to expose load-generator limits.

A small read sends a 256-byte probe input plus the harness envelope. A large read
returns the full row with a 16 KiB note. The single update changes a 256-character
note; the atomic action changes two titles. These are representative operations
within the existing bounded fixture, not a production workload mix or full auth.

End-to-end latency includes HTTP and queueing. Handler timing begins at handler
entry and ends after response construction; it omits queueing before the handler.
For asynchronous generated Bun transactions it includes time awaiting the serialized
transaction queue. Server CPU includes JavaScriptCore, HTTP and SQLite work. Client
CPU is also retained. The host is not thermally/load controlled; short closed-loop
runs do not establish fixed-arrival-rate SLOs or deployment capacity.

## HTTP results

**140 complete measurements, 3,457,655 requests, zero errors.** The separate
microbenchmarks completed 50 measurements and 1,281,101 operations without errors.
Every one of the 40 final server processes passed eight preflight checks, and the
runner awaited server shutdown after each target. No external resources were
created by this extension.

Medians of five runs, with sixteen requests outstanding:

| Workload | Bun requests/s | Candidate requests/s | Bun p95 ms | Candidate p95 ms |
| --- | ---: | ---: | ---: | ---: |
| Small probe/read | 23,151 | 21,455 | 1.2857 | 1.3385 |
| Full row with 16 KiB note | 15,297 | 9,520 | 1.8971 | 2.8654 |
| Single durable update | 1,938 | 1,631 | 13.2407 | 10.7257 |
| Two-row atomic update | 1,845 | 1,834 | 14.2232 | 13.5157 |

At concurrency one, small reads were 8,993/s for Bun and 8,904/s for the candidate,
with p95 0.1739 ms and 0.1668 ms respectively. Large-row reads were 6,761/s versus
5,231/s, with p95 0.2330 ms versus 0.2980 ms. These differences are small in
absolute time but become meaningful compute/capacity differences for larger values.

The server CPU measurements support that distinction. At concurrency sixteen,
small reads used approximately 41.8 µs/request for Bun and 45.4 µs for the candidate
(about 9% more). Large reads used 59.4 µs versus 103.8 µs (about 75% more). The
candidate reduced large-read server CPU from the previous Wasm path's 131.9 µs.
This is process CPU in this harness, not a cloud billing prediction.

The write results are noisy and mixed. Candidate single-write throughput ranged
from 1,025 to 2,501/s across the concurrent repetitions; the median throughput is
lower than Bun even though median p95 is lower. Short-run throughput and tail
quantiles need not move together under stalls, queueing and workstation variation.
Do not interpret the favorable write latency as a demonstrated backend speedup.
The two-row atomic action is approximately level in median throughput.

The no-work control reached 30,534/s for small responses and 18,367/s for large
responses at concurrency sixteen, only about 1.32× and 1.20× the corresponding
Bun results. The larger Bun response path is close enough to this client/HTTP
ceiling that these are **not reliable maximum-server-capacity measurements**.
The CPU measurements and the isolated application comparison are additional
evidence of the remaining larger-value cost, rather than claiming saturated
HTTP throughput has been established.

Using the earlier 20% latency threshold only as a reference, the candidate's
concurrent small-read p95 ratios are below 1.20 in all five pairs, but large-read
ratios are 1.32–1.61. Concurrency-one paired ratios are variable; small-read ratios
range from 0.73–1.55 despite near-equal medians. This is not a blanket gate pass,
and the different HTTP workload does not replace the frozen original acceptance
protocol. No statistical-significance claim is made.

## Decision

The original large high-traffic penalty is not an inherent result of choosing
Wasm. Reuse removed the dominant prototype cost; this pass further improved host
checks and constant handling. Small operations are now close enough to Bun in
this local experiment that performance alone is not a reason to discard Wasm.

Larger returned rows remain an important limitation. The next bounded question
should be whether a typed or more compact value boundary can reduce repeated
JSON parsing, serialization and owned-value copying, while retaining validation,
policy, lifetime and failure guarantees. Profile those guest costs before
committing to a new ABI. Higher-capacity external load generation and longer
controlled runs are needed before making a hosting/capacity decision. Bun remains
the default; language coverage, production auth and cloud qualification are
separate requirements, not answered by these timings.

## Correctness and retained limits

The changed candidate passed 20 tests with 10,502 assertions, the unchanged 15 probe
and 16 local runtime cases, five explicit rejection gates and four adversarial
rollback cases. The new descriptor tests mutate every leaf/shape of the checked
policy and test Unicode constraints; the reused-instance matrix includes 10,000
sequential requests and interleaved principals. Source mutation changes behavior,
and rebuilding from a clean Cargo target reproduces the original candidate bytes.

Each HTTP server also checks owner/outsider reads, invalid input, missing data,
empty patches, uniqueness conflicts, atomic rollback and outsider writes before
serving benchmark traffic. HTTP callers cannot choose a principal; the fixture
uses a fixed trusted owner and a private loopback token. This is not production
authentication qualification.

The changed artifact has not been deployed to Cloudflare in this extension. Earlier
cloud passes remain evidence for their exact earlier artifacts. The original
oversized-host-result diagnostic fallback, resource/admission/cancellation limits,
external review and full language-backend work remain open. No backend migration
or additional experiment is started by these results.

Two initial Bun-fetch load-generator attempts stopped during a no-work control
warmup with ConnectionRefused while the server was still alive. All completed
application measurements passed, but those incomplete runs are retained separately
and excluded from the final comparison. The failure did not reproduce in short
runs and its root cause was not established. The final harness uses Node
keep-alive HTTP connections for all targets and never retries measured requests.

[Reproduction](../../experiments/wasm-exp1/boundary-http/README.md),
[full HTTP table](../../experiments/wasm-exp1/boundary-http/table.md),
[summary and paired ratios](../../experiments/wasm-exp1/boundary-http/results.json),
[artifact hashes](../../experiments/wasm-exp1/boundary-http/artifacts.json),
[retained raw evidence](../../experiments/wasm-exp1/boundary-http/evidence/manifest.json),
and [effort accounting](../../experiments/wasm-exp1/boundary-http/effort.json)
are retained with the experiment sources. Sampling used the installed
[Bun JavaScriptCore profiler](https://bun.sh/reference/bun/jsc/profile); the
HTTP control addresses the load-generator limitation discussed in
[Bun's benchmarking guidance](https://bun.sh/docs/project/benchmarking).
