# Authenticated shared-Rust backend performance

**Continue the shared-Rust architecture as a bounded experiment.** Across **1,618,304 measured HTTP requests** (252 cells), all response/state/SQL checks passed. Native showed useful read, CPU, memory and startup gains; write tails and WASM large responses prevent an all-metrics win. This does not qualify the raw-SQL WASM host as an independent authorization boundary.

This continues the [authenticated conformance experiment](auth-policy-conformance-results.md)
and measures its exact frozen native, WASM and generated Bun artifacts. It does
not reuse the older unauthenticated fixture's performance scores. Bun remains the
default target and the old selected WASM candidate remains frozen. There was no
cloud deployment or production migration.

## Work and guarantees compared

Every authorized request performs two live session checks and one live user
lookup. Existing opaque credentials, current/previous signing keys, owner/editor
policies, restricted fields, validation, failure responses and rollback behavior
are unchanged. The previous 888-request conformance run, 51-call source mutation
and lifecycle tests remain attached to the byte-identical artifacts. The new
campaign asserts response content/status, request-ID correlation, cache headers,
SQL counts and final persisted state. It rechecks seeded state before each cell.

The fixture has three users and four notes. Large reads put 16 KiB of ASCII in one
note's private field: the public route returns just id/title, while the private
route returns id/private_note. Both still materialize the full internal row.
Writes change unique titles each request; pairs update two rows in one transaction;
denied pairs update the first row then deny the second and roll back. Missing-auth
requests execute no SQL. This does not represent a large indexed database or a
broad application workload.

All connections reported SQLite 3.39.5, WAL or DELETE as selected,
`synchronous=FULL`, foreign keys on, busy timeout zero, `fullfsync=0` and a
1,000-page WAL autocheckpoint. Servers serialize application execution on one
connection; Bun additionally has a control connection. Twelve concurrent HTTP
clients do not imply twelve simultaneous database transactions. Durability settings
are equivalent here; crash/power-loss recovery was not tested.

| Request | Bun statements | Native / WASM statements |
| --- | ---: | ---: |
| Authorized read | 4 | 4 |
| Single rename | 7 | 6 |
| Successful or denied pair | 13 | 7 |
| Missing credential | 0 | 0 |

Counts include authentication reads and transaction commands. Bun uses pre-reads
and nested savepoints; Rust uses guarded `UPDATE … RETURNING` in one owning
transaction. This fixture propagates nested failure, so commit/rollback outcomes
agree. General handled nested mutation still requires savepoints and is outside
the supported slice. Write speedups therefore cannot be attributed solely to Rust.
Native caches prepared statements; the Bun/WASM adapter retains its existing
prepare path. Bun caches imported cryptographic keys at initialization, while the
Rust boundary decodes and fingerprints the two configured keys per request.
These implementation differences are preserved and disclosed.

## Measurement protocol

The local Apple M1 Pro (10 cores, 16 GiB RAM), macOS kernel 23.1.0 / arm64
development machine runs client and server together, using Bun 1.2.20 and Node
24.18.1. Rust builds use the existing pinned dependencies. Three repetitions rotate target order. Seven workloads × two concurrency
levels × two journals × three targets × three repetitions give 252 cells. Each
cell starts a fresh process/database, makes 32 warmups, then measures at least one
second in blocks of 128 requests. Readiness, setup, warmups, snapshots and trace
controls are excluded from the measured application count.

The frozen adapters collect SQL traces. Draining after each block bounds retained
trace memory without changing measured source/binaries. Request latencies and
active-block throughput exclude drain pauses; inclusive throughput includes them.
Both appear in the [complete table](../../experiments/auth-policy-measure/table.md).
Server CPU uses external `ps` deltas with 10 ms resolution and includes drains;
client CPU is retained separately. Resident memory is sampled after warmup and
after measurement; `/usr/bin/time -l` records lifetime peak RSS and CPU. Neither
resident sample is a promise of an uninstrumented production footprint.

Tables report medians of three per-run metrics, not pooled request percentiles.
Per-run p50/p95/p99/max, raw latency arrays, throughput ranges, RSS and CPU remain in
[results](../../experiments/auth-policy-measure/results.json) and the
[evidence manifest](../../experiments/auth-policy-measure/evidence/manifest.json).
This is closed-loop HTTP with response validation, short warmups and possible
JIT/GC transients. No open-loop overload, coordinated-omission correction,
CPU pinning, confidence interval or sustained storage qualification is claimed.
The development host was not an isolated performance lab.

## Measured HTTP results

WAL, 12 clients. Throughput excludes between-block trace drains; p95 is in milliseconds.

| Workload | Bun req/s | Native req/s | WASM in Bun req/s | Bun p95 | Native p95 | WASM p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Small public read | 5,876 | 10,443 | 7,518 | 2.86 | 1.36 | 2.70 |
| 16 KiB row → public id/title | 6,009 | 9,662 | 6,130 | 2.77 | 1.50 | 3.39 |
| 16 KiB private response | 4,500 | 7,597 | 3,920 | 3.55 | 1.82 | 5.03 |
| Single rename | 3,384 | 5,475 | 3,513 | 4.74 | 2.54 | 5.71 |
| Atomic pair | 2,472 | 3,458 | 2,815 | 7.09 | 5.92 | 7.43 |
| Denied second write / rollback | 3,628 | 6,669 | 4,596 | 4.62 | 2.06 | 4.50 |
| Missing credentials | 30,966 | 31,444 | 31,059 | 0.65 | 0.64 | 0.64 |

Native read throughput is 1.61–1.78× Bun in these rows, with p95 reduced about 46–52%. For the small read, inclusive rates are 5,600 / 9,473 / 7,135 req/s (Bun / native / WASM). WASM’s private-response throughput is about 13% below Bun, with p95 about 42% higher. This is a different guest/fixture from the historical optimized WASM candidate.

Native paired-write p99 is **20.44 ms versus Bun’s 10.17 ms** despite its better median throughput/p95. Across those three WAL pair runs, maximum request latency reaches 92.4 ms native, 54.2 ms Bun and 28.7 ms WASM. No tail outliers were discarded.

DELETE, 12 clients, shows the storage/adapter dependence:

| Workload | Bun req/s | Native req/s | WASM in Bun req/s | Bun p95 | Native p95 | WASM p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Single rename | 1,570 | 1,971 | 1,642 | 10.02 | 8.10 | 12.48 |
| Atomic pair | 1,344 | 1,749 | 1,362 | 11.28 | 9.83 | 15.20 |
| Denied second write / rollback | 1,604 | 2,532 | 1,984 | 9.79 | 5.78 | 10.19 |

At one client with DELETE, native single-write throughput is 1,268 req/s versus Bun’s 1,341, and native’s repetition range is 592–1,798 req/s. Native pair maximum latency reaches 124 ms. Three short repetitions cannot establish stable storage-tail superiority. Read and write latency distributions for both concurrency levels and journals are retained, including these less favorable results.

## CPU, memory, startup and builds

Server CPU per request, WAL / 12 clients (µs, includes trace drains):

| Workload | Bun | Native | WASM in Bun |
| --- | ---: | ---: | ---: |
| Small public read | 242.9 | 95.0 | 197.3 |
| 16 KiB private response | 277.8 | 132.8 | 355.3 |
| Atomic pair | 511.7 | 234.4 | 468.8 |

For small reads, native uses about 61% less server CPU than Bun; for large private reads, about 52% less. These include drivers, HTTP, auth and tracing rather than isolating language execution. Missing-auth throughput is about 31k req/s on all three targets at 12 clients, so it provides little useful separation under this client/protocol.

| Target | Post-phase RSS range, all cells | Largest lifetime peak RSS | Median spawn → first response | First-response range |
| --- | ---: | ---: | ---: | ---: |
| Bun | 58.8–145.8 MiB | 146.0 MiB | 81.62 ms | 79.84–320.78 ms |
| Native | 2.6–16.8 MiB | 17.1 MiB | 7.63 ms | 6.86–9.16 ms |
| WASM in Bun | 73.2–145.5 MiB | 145.5 MiB | 31.36 ms | 30.62–37.38 ms |

WAL small-read median post-phase RSS is 116.9 MiB Bun, 14.2 MiB native and 130.7 MiB WASM-in-Bun. The latter includes its Bun host; the guest’s 8 MiB linear-memory ceiling is not total process memory. The Bun startup outlier (321 ms) is retained. Native is 785,464 bytes; WASM is 272,698 bytes, excluding host/runtime dependencies.

Build time in seconds:

| Build stage | Bun | Native | WASM |
| --- | ---: | ---: | ---: |
| Checked generation only, median of 3 | 0.100 | — | — |
| Complete warm build command, median of 3 | — | 9.815 | 3.435 |
| Cargo no-op, median of 3 | — | 0.096 | 0.058 |
| Clean release Cargo target, one sample | — | 14.505 | 5.469 |

The clean WASM output is byte-identical to its measured artifact. The native build
from a different Cargo target directory has a different binary hash; it was timed
but did not replace the measured native executable. Native reproducible builds
across output directories are not established here. The shared projection compiler was already built; clean runtime-target timings do not include rebuilding that compiler, fetching dependencies or installing toolchains.

Build commands check the Jadpo source and regenerate one Rust application/contract
for either target. They do not execute Bun. The current experimental build script
also emits the Bun reference as source data. Warm commands rerun generation and
can trigger Rust recompilation; a direct Cargo no-op is reported separately.
Clean-target samples use empty Cargo output directories with already downloaded
dependencies and warm filesystem caches, not empty-machine installation times.
Bun check/generation produces runnable TypeScript and has no equivalent link step.
Startup includes process creation and initialization through the first validated
HTTP response; ten fresh processes per target use warm OS caches. WASM includes
Bun startup and WASM module compilation/instantiation.

## Shared code and adapter cost

The same generated application (110 physical lines / 14,347 bytes) and checked
contract (12,628 bytes) feed both Rust targets. Another 799 physical lines of
shared runtime implement invocation state, budgets, authentication, policy SQL,
validation and failure mapping. These are handwritten runtime support plus
compiler-generated application logic, not 799 lines of authored Jadpo. The
native adapter is 298 lines; the WASM ABI is 106 lines plus 29 compact JS driver/SQL
lines, with an additional Bun HTTP/comparison wrapper. Line counts are descriptive,
not a correctness or maintenance-cost score.

The portable business behavior is real. Remaining adapter responsibilities include
HTTP framing/headers, clocks/configuration, driver errors, transaction cleanup,
process scheduling and, for WASM, memory ownership and the host capability boundary.
The experiment still copies lowerer/runtime code to preserve historical controls;
it has not consolidated them into a maintained general compiler backend. Python
lowering remains, and only the checked fixture subset is supported.

## Host decision and recommendation

Pursue the dual-target architecture for another bounded implementation stage.
The native runtime is a standalone executable using rusqlite; Bun is not a native
runtime dependency. Here Bun supplies the comparison server, the WASM host and
synthetic credential issuance. A future non-Bun WASM host still needs its own
adapter and measurements. All WASM numbers in this report are **Bun-hosted**.
There is **no new workerd measurement**; historical workerd results use different
artifacts/protocols and cannot be substituted into these tables.

The [host decision](wasm-host-trust-decision.md) is explicit: keep independently
enforcing authorization when the WASM product promises isolation from guest code.
Five new tests / 18 assertions show both the reviewed guest's enforcement and the
raw adapter's ability to read private data or commit writes from forged guest
SQL. The tested artifact-admission helper rejects changed module/contract bytes,
but does not independently authorize effects and is not installed in or timed as
part of these frozen servers. Performance here does not justify weakening that
boundary. A scoped capability host, potentially using the same trusted Rust policy
library, must be implemented and measured before replacing the old enforcing host.

Do not switch defaults on these results. Broader authentication/policy forms,
issuance/revocation APIs, handled nested savepoints, startup/config lifecycle,
secret management, disconnect cancellation, multi-connection scheduling, recovery
and independent security review remain unqualified. The local adapters contain
harness controls and are not deployment artifacts. The next useful compiler step
is to factor the shared runtime and capability authority cleanly, retain these
conformance cases, and qualify the host boundary before broadening project support.

## Reproduction and preservation

[Commands and protocol](../../experiments/auth-policy-measure/README.md) reproduce the
campaign. Existing short `build:rust` / `build:wasm` aliases still build the original
shared-core fixture. For the authenticated fixture measured here:

```sh
jadpo build experiments/auth-policy/fixture --target native
jadpo build experiments/auth-policy/fixture --target wasm
```

Use `sh experiments/auth-policy/build.sh native` or `wasm` if Jadpo is not installed.
Both build paths and the native runtime work without Bun. The measurement harness
uses Bun deliberately for the reference and WASM hosting.

WASM SHA-256: `0fa980e39ea16f57b95bda7b32a0d35e85f0345b39ce477edf61d0b4a66146b4`.
Native SHA-256: `4cd94eaee6d38309bbb97f8f19ca2be8394471123cf3c35b75de53c4f644c2ec`.
The selected historical WASM candidate and all prior experiment controls remain
unchanged. All 1,188 frozen file hashes, including the unrelated Jadpo source and
VSIX, are checked before/after measurement and builds. Raw observations, synthetic
seed state, exact binaries, new source and documentation are archived with hashes.
