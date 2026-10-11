# Authenticated backend measurement and WASM trust decision

Measures the frozen `auth-policy` conformance artifacts, without changing their
source or binaries. Local only. WASM is hosted by Bun, not workerd. Bun remains
the default production target. Native needs no Bun at runtime or for building;
Bun is used here for the reference server, WASM host and synthetic credential issuer.

## Registered protocol

Three sequential repetitions with rotating target order, concurrency 1 and 12,
WAL and DELETE journals, FULL synchronization, one serialized application database
connection per server (Bun also opens a control connection). Seven workloads:
small public read, 16 KiB ASCII public/private read, single rename, atomic pair,
denied second pair with rollback, and missing credentials. No response cache or
freshness relaxation. Every response and final database state is checked.

Per cell: fresh server/database, 32 warmups, at least one second of measurement
in blocks of 128 requests. The existing adapters retain SQL traces; drain after
each block outside request timing to bound memory without modifying the artifacts.
Report both active-block throughput and inclusive wall throughput. Latency includes
closed-loop client scheduling/HTTP/JSON verification; no coordinated-omission
correction, open-loop load or multi-connection SQL scalability claim. Retain all
latencies, counts, statement totals and public-response assertions. Trace/control
work is disclosed, never silently treated as an uninstrumented production result.

Server CPU is external `ps` cumulative CPU delta (10 ms resolution on this Mac),
including trace drains during the measured phase. Client CPU is separately
recorded. RSS is external `ps` resident bytes after warmup and measurement;
`/usr/bin/time -l` retains process-lifetime peak RSS and user/system CPU. RSS after
warmup is neither idle footprint nor a guaranteed peak. CPU totals include HTTP
and instrumentation, not just the Rust core. Startup: ten fresh processes per
target, rotating order, spawn-to-ready and spawn-to-first validated read with warm
OS caches. Build: three complete warm commands and three Cargo no-ops per target, plus a separate clean Cargo target
directory; dependencies and filesystem caches warm, offline. Bun generation has
no equivalent native link phase; report stages rather than invent a single ratio.

## Reproduce

From repository root, first build the auth-policy fixture with its documented
`jadpo build … --target native|wasm` commands if outputs are absent. The frozen
artifact hashes must match before running:

```sh
mkdir -p build/auth-policy-measure
python3 experiments/auth-policy-measure/freeze.py
bun --no-install --env-file=/dev/null experiments/auth-policy/seed.ts
node experiments/auth-policy-measure/run.mjs --smoke
node experiments/auth-policy-measure/run.mjs > build/auth-policy-measure/http.log 2>&1
node experiments/auth-policy-measure/run.mjs --startup > build/auth-policy-measure/startup.log 2>&1
python3 experiments/auth-policy-measure/build-times.py
bun --no-install --env-file=/dev/null test experiments/auth-policy-measure/trust.test.ts > build/auth-policy-measure/trust.log 2>&1
python3 experiments/auth-policy-measure/summarize.py
python3 experiments/auth-policy-measure/archive.py
```

Local HTTP listeners require an execution environment allowing loopback. Run the
campaign sequentially, without concurrent builds or other benchmarks. Credentials
expire after an hour; refresh before a new campaign. `--smoke` is harness validation,
not part of reported performance. No frozen results are overwritten.

The first expanded smoke attempt reused database path names from a previous smoke
run; stale WAL sidecars caused the initial-state assertion to fail. The harness
now uses unique timestamped paths and explicitly checks the seeded state before
measurement. The full campaign uses that corrected harness. Only final successful
smoke results are performance-independent validation evidence.

The five trust probes deliberately demonstrate that the raw SQL host accepts
unauthorized effects from a malicious guest. A passing probe is evidence of that
limitation, not security qualification. The admission helper is a separate tested
prototype and is not wired into or measured as part of the frozen HTTP server.
See [the host decision](../../docs/experiments/wasm-host-trust-decision.md).

Thirty-two warmup requests and short one-second cells include possible JIT/GC
transients; this is a bounded local comparison, not steady-state capacity testing.
Native also caches prepared statements while the Bun adapter uses its existing
prepare path. The benchmark preserves those adapter choices rather than claiming
an isolated comparison of execution-language speed.
