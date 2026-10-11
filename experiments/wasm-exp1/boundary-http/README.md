# JSON boundary and local HTTP follow-up

This is a bounded extension of the instance-reuse experiment, authorised by the
owner on 30 September 2026. Production code and the frozen original evidence are
unchanged. The [plan](plan.md) was recorded before profiling or implementation.

## Candidate

`compiler/generate.py` differs only in its literal emitter: compiler-owned constant
JSON is parsed once per Wasm instance using `OnceLock<Value>` and cloned before
request values are inserted. Request data and principals are never cached in that
constant. The ABI, pooling driver, Rust runtime and generated application control
flow are unchanged. The candidate module is a new artifact; it does not inherit
the previous module's Cloudflare qualification.

`adapter.ts` snapshots the previous cached adapter, replacing per-call canonical
JSON sorting/stringification with a checker compiled from the privately owned
checked policy plan. It still verifies every scalar, object key, array element
and array order. Closed descriptor objects and principal, field, operation,
freshness and value checks remain. Unconstrained text no longer allocates a
Unicode character array solely to calculate an unused length. Constrained text
still counts Unicode scalar values.

## Profiling and reproducibility

Use the repository's existing pinned tools (Bun 1.2.20, Rust 1.78, serde_json
1.0.133) and the original checked projection/baseline. From repository root:

```sh
bun --no-install --env-file=/dev/null experiments/wasm-exp1/boundary-http/profile.ts
sh experiments/wasm-exp1/boundary-http/build.sh
python3 experiments/wasm-exp1/boundary-http/mutation-rebuild.py
python3 experiments/wasm-exp1/boundary-http/negatives.py
bun --no-install --env-file=/dev/null test experiments/wasm-exp1/boundary-http/reuse.test.ts experiments/wasm-exp1/boundary-http/cached-adapter.test.ts experiments/wasm-exp1/boundary-http/descriptor.test.ts
bun --no-install --env-file=/dev/null experiments/wasm-exp1/boundary-http/run-cached.ts experiments/wasm-exp1/boundary-http/compiler/build/application.wasm --mutated=experiments/wasm-exp1/boundary-http/compiler/build/mutated.wasm
bun --no-install --env-file=/dev/null experiments/wasm-exp1/boundary-http/atomic.ts
node experiments/wasm-exp1/boundary-http/http.ts --smoke
bun --no-install --env-file=/dev/null experiments/wasm-exp1/boundary-http/micro.ts
node experiments/wasm-exp1/boundary-http/http.ts
python3 experiments/wasm-exp1/boundary-http/summarize.py
```

Run timing programs sequentially, with no builds or tests alongside them.
`profile.ts` uses the installed `bun:jsc` sampling profiler plus diagnostic stage
and micro timings. The stripped Wasm samples do not name individual Rust source
functions. Stage timing instrumentation adds overhead; isolated means are not
additive estimates of end-to-end p95. The JSON codec microcases use pre-serialized
strings and therefore do not measure all serialization work.

`micro.ts` isolates the changed core, host, and combined candidate against the
previous pooled/cached module and generated Bun. Its small probe and larger-row
read use direct application boundaries, not HTTP. Bun receives native values;
Wasm incurs its JSON ABI. These numbers do not replace the earlier experiment's
different wire-roundtrip baseline protocol.

## HTTP protocol

`http.ts` is the Node 24.18.1 load generator and orchestration process, using the
built-in HTTP client with explicitly owned keep-alive connections. It starts a separate
`server.ts` Bun process for one target at a time on an ephemeral loopback port.
Both the generated Bun app and Wasm receive the same HTTP handler, JSON request
and response envelope, fixture principal, on-disk SQLite durability settings and
seeded rows. The server reports its settings and eight correctness preflights.
The endpoints require a random per-process fixture token, bind only to loopback
and are removed when the server process exits. This is not production auth.

The generated Bun baseline retains its own persistence/transaction code. Wasm
retains the SQL host boundary and synchronous atomic authority. The harness does
not force the two implementations to issue identical SQL. `environment.ts` maps
validated arguments into the existing generated callables and normalizes declared
outcomes. No application logic or domain recovery is reimplemented there.

Five repetitions rotate targets, with concurrency one and sixteen and 1 second
warmup plus 3 seconds timed per target/workload. Workloads are a 256-byte probe
input, a full row read with a 16 KiB note, a changing 256-character note update,
and a two-row atomic rename. Request sizes include an additional HTTP envelope;
16 KiB is the note size, not the total response size. Every response is checked;
post-run snapshots verify persistence, intact unrelated rows and paired updates.

The no-work HTTP control uses the same parse/serialize/client checks for small
and larger read responses, but does no application/database work in the timed
handler. It diagnoses a load-generator ceiling, not a network hardware maximum.
Client and server share a workstation. CPU includes runtime and HTTP overhead;
handler timings include body parsing and response construction but exclude socket
queueing. Closed-loop load does not establish fixed-arrival-rate latency SLOs,
production saturation behavior, cold starts or distributed/cloud capacity.

See the [results report](../../../docs/experiments/wasm-http-experiment-results.md),
[HTTP table](table.md), [paired summaries](results.json) and
[raw evidence manifest](evidence/manifest.json) for the final conclusion.

Profiling API reference: [Bun JavaScriptCore profiler](https://bun.sh/reference/bun/jsc/profile).
The installed version was checked for this API before use. The load-generator
control follows the concern documented in [Bun benchmarking guidance](https://bun.sh/docs/project/benchmarking): a slow client can hide server throughput.

Two initial longer runs using Bun 1.2.20 fetch stopped with ConnectionRefused
during the second no-work control warmup, despite a live server and passing
preflight. Short reproduction runs did not fail. The cause was not established;
`http-bun-client.ts` retains the diagnostic harness and aborted evidence is kept.
The final comparison uses Node for every target and does not retry failed requests.
