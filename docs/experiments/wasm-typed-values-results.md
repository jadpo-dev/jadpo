# Immutable Wasm row values and SQLite write attribution — 30 September 2026

**Result: modest additional large-read gains; a much larger SQLite journal-mode
write improvement on this local host. Large-read Bun parity still fails. Keep the
candidate experimental and Bun as the default target.**

This is a separately frozen follow-up to the
[row-transport experiment](wasm-row-transport-results.md). Both targets run in Bun;
no standalone Wasm host, EC2 deployment or new Cloudflare qualification is implied.

## Candidate

A checked flat storage row becomes an immutable sequence of owned scalars in
compiler-generated schema order. Passing the whole row through functions shares
ownership rather than copying large text. Input and host JSON decoding still use
`serde_json::Value`; updates, control messages, and unsupported row shapes retain
the reference representation. This is a limited internal representation change,
not a complete typed backend or a specialised JSON parser.

After the existing guest entity validator succeeds, the accepted host JSON frame's
length is retained as an upper bound on the canonical success frame for that same
row. It is derived inside the guest, not accepted as metadata. Immutability keeps
the bound valid, so returning the row in the existing binary format avoids another
full text scan. Materialising JSON drops the proof. Unknown provenance and binary
ingress retain the reference size checks; binary frame overflow still falls back
to valid JSON. Strings are owned; no borrowed view into exported Wasm memory is
retained across execution or suspension.

Validation, policy, authoritative reads and external HTTP JSON are unchanged. There
is no result cache, projection shortcut or replacement of a storage read with an
old row. Existing query failures and safe fault handling still execute in Wasm.

The selected candidate SHA-256 is
`631ea8e20a3cc55936f857acefe3fe68ada60b83a7b737fc651a51fae40b8b91`;
its reference is the preceding row-egress candidate
`b488c657e7184ed1c8689504941d5a3db8cd77e555fa800cbdb93405bb3956a9`.
Selection preceded HTTP qualification and used one implementation, without tuning
against qualification results. Implementation checkpoint: `0bf92f9`.

## Isolated selection

Five rotated runs, 0.2 seconds warmup and one second measured, full output checks:

| Target | Small operations/s | Small p95 µs | 16 KiB operations/s | 16 KiB p95 µs |
| --- | ---: | ---: | ---: | ---: |
| Generated Bun | 40,283 | 28.96 | 31,073 | 37.58 |
| Previous row-egress Wasm | 43,158 | 29.83 | 18,324 | 67.46 |
| Immutable-row Wasm | 42,732 | 29.92 | 19,266 | 64.17 |

Ratios of medians give approximately 5.1% higher large-row throughput and 4.9%
lower p95 versus previous Wasm. The small probe is roughly unchanged. These
application-boundary calls are not HTTP capacity measurements.

## HTTP qualification

Eighty read cells used fresh Bun/previous/candidate/no-work controls, five rotated
runs, concurrency one and sixteen, two seconds warmup and ten seconds measured.
There were 11,812,565 verified read requests and zero errors. The new harness also
retains maximum request latency, which the previous harness omitted.

At concurrency sixteen, medians of five repetitions for the full 16 KiB row:

| Metric | Bun | Previous Wasm | Immutable-row Wasm |
| --- | ---: | ---: | ---: |
| Requests/s | 16,355 | 11,642 | **11,984** |
| p95 ms | 1.7882 | 2.4132 | **2.3660** |
| Server CPU µs/request | 58.60 | 84.38 | **81.77** |

Ratios of medians give +2.9% throughput, -2.0% p95 and -3.1% CPU versus previous
Wasm. Throughput and p95 improve in four of five pairs, CPU in all five. Throughput
pair ratios range from a 0.2% loss to a 7.8% gain; p95 from a 0.3% regression to a
6.6% improvement. This is descriptive local evidence, not statistical significance.
At concurrency one, median throughput moves from 5,887 to 5,926 requests/s and
p95 from 0.2610 to 0.2601 ms; CPU falls from 97.37 to 94.79 µs/request.

The registered large-row practical Bun parity gate **fails** at both concurrencies;
the primary small-probe regression gate **passes**. Concurrent small reads are
25,045/s at 1.1462 ms p95, versus previous Wasm's 24,997/s at 1.1500 ms and Bun's
24,280/s at 1.2123 ms. No-work controls reach about 31.2k small and 19.0k large
responses/s: they still lack the required 2x headroom, so maximum capacity remains
unqualified. Workstation load, thermal state and filesystem caches are uncontrolled.

The 120-cell supplemental matrix retains five shorter 0.1+0.5-second runs per
size/type/target. Throughput gains versus previous Wasm were about 4.7% for 16 KiB
ASCII, 6.7% for the corresponding escaped case, 7.8% for 48 KiB ASCII and 8.5% for
the largest escaped case. Escaped rows have smaller actual payloads to fit the JSON
budget: their complete frame sizes are recorded (6,036 and 17,812 bytes in the two
larger cells). The 256 B full-record ASCII case regressed 4.3% in throughput and
3.9% in p95; the small probe is a different workload. Generic materialisation and
small-row packing are not free. No blanket improvement across all rows is claimed.

## Writes with the original DELETE/FULL settings

Thirty-six ordinary write cells completed 167,044 requests with zero errors, using
three rotations and one second warmup plus three seconds measurement. Concurrent
write medians and maximum observed request times remain adverse:

| Workload / metric | Bun | Previous Wasm | Candidate |
| --- | ---: | ---: | ---: |
| Single update requests/s | 1,378 | 1,208 | **888** |
| Single update p95 / p99 ms | 14.49 / 86.64 | 13.17 / 46.95 | 11.24 / 50.19 |
| Single update max ms | 821 | 1,015 | **3,389** |
| Atomic pair requests/s | 2,038 | 1,377 | 1,220 |
| Atomic pair p95 / p99 ms | 13.23 / 17.54 | 12.64 / 104.97 | 11.66 / 15.15 |
| Atomic pair max ms | 1,283 | 2,024 | 2,024 |

Candidate throughput is worse in both write workloads in this run. Even Bun has
occasional long pauses; the run does not establish a compiler-specific cause or
excuse the candidate's worse observations. Rare multi-second stalls can be absent
from p99, which is why maximum latency is retained.

## Write attribution

Forty diagnostic cells compared previous/candidate Wasm, single/pair updates and
file-backed/in-memory SQLite, with five rotated repetitions and 0.2 seconds warmup
plus 1.5 seconds measured. Every response was checked. The existing Bun native
transaction implementation was retained; instrumentation recorded every SQL call
and bracketed the application callback. Disk used DELETE journal and FULL
synchronous durability, matching previous qualification.

Observed maximum operation times reached 2.797 seconds on disk versus 3.017 ms in
memory. Slow transactions (at least 10 ms) spent approximately 98% of their summed
transaction time after the callback returned. This narrows the dominant observed
stall to transaction completion, which includes native commit and wrapper work;
it is not a direct fsync syscall measurement. Some statement/body stalls also
occurred. Both Wasm candidates exhibited disk stalls; neither exhibited a 10 ms
transaction stall in the memory control.

The memory comparison is an attribution control, not a proposal to remove durable
storage. Timing perturbs execution and this is the same local workstation, not a
controlled EC2 deployment. A separate 36-cell instrumented HTTP run completed another 176,286 requests with
zero errors. It captured 117 slow transactions in previous Wasm and 278 in the
candidate. Transaction completion accounts for 96.0% and 76.4% of summed slow
transaction time respectively. Some long pauses also occur inside `UPDATE`:
the candidate's longest measured statement was 3.226 seconds. Thus commit is the
main observed location, but not the only native SQLite call that stalls. The
measurement does not identify a particular filesystem or sync syscall cause.

The journal control retains `synchronous=FULL`, rather than disabling sync. SQLite
[documents](https://sqlite.org/pragma.html#pragma_synchronous) an additional WAL
sync at each commit with this setting. DELETE/FULL itself is not a proof of
power-loss durability on every filesystem; this experiment checks normal execution
and clean reopening, not crash/power-loss recovery. WAL also has a separate
[checkpoint lifecycle](https://sqlite.org/wal.html#checkpointing), so short-run
throughput cannot establish long-running operational behaviour.

## Journal control and HTTP confirmation

After the disk/memory result, a separate five-repetition control compared
DELETE/FULL and WAL/FULL, rotating both modes and Wasm candidates on the same
filesystem. Forty cells verified 558,514 calls. The pinned host reports **SQLite
3.39.5**, `fullfsync=0`, `synchronous=2`, and `wal_autocheckpoint=1000`. Both modes
passed atomic rollback preflight and all twenty database files survived clean
close/reopen with the expected rows. This is not a power-loss recovery test.

For the new candidate, median isolated single-update throughput was 1,596/s with
DELETE versus 20,582/s with WAL; pair throughput was 2,541/s versus 12,791/s. No WAL
transaction exceeded 10 ms in this run, while DELETE reached 4.94 seconds. These
instrumented rates are not HTTP capacity.

A final **uninstrumented** HTTP write comparison applied WAL/FULL to generated Bun
and both frozen Wasm targets. Thirty-six cells, three rotated repetitions,
concurrency one/sixteen and 1+3-second timings completed **1,010,320 requests with
zero errors**. At concurrency sixteen:

| Workload / metric | Bun + WAL | Previous Wasm + WAL | Candidate + WAL |
| --- | ---: | ---: | ---: |
| Single update requests/s | 12,124 | 15,475 | **15,959** |
| Single update p95 ms | 2.4699 | 1.7860 | **1.7445** |
| Single update p99 ms | 3.5685 | 3.4973 | **3.2093** |
| Single update max ms | 9.291 | 43.278 | 8.901 |
| Atomic pair requests/s | 7,484 | 11,374 | **11,247** |
| Atomic pair p95 ms | 4.0178 | 2.5624 | **2.5887** |
| Atomic pair p99 ms | 5.2960 | 3.5504 | **3.6320** |
| Atomic pair max ms | 15.987 | 23.513 | 13.031 |

The candidate's single-update throughput is about 18x its earlier DELETE/FULL
HTTP median, with p95 about 84% lower. Both targets benefit from WAL. Within the
same WAL run, the candidate has about 32% more single-update throughput and 50%
more pair throughput than the generated Bun baseline, with lower p95 and CPU.
Previous Wasm also benefits, and slightly exceeds the new candidate's pair rate.
The large storage gain is not attributable to the immutable-row compiler change.

These targets execute the same checked fixture obligations but different host
implementations: generated Bun's SQLite mutation reads the existing row before
updating and runs its general policy/transaction machinery; the Wasm adapter
performs scoped `UPDATE ... RETURNING`. This is a comparison of generated systems,
not equal SQL work or a pure JavaScript-versus-Wasm instruction-speed comparison.
Bun could also benefit from compiler/storage optimisations. More general policy,
change-record and transaction semantics are outside the fixture.

The observed maximum over the entire WAL HTTP run was 43.3 ms. That is encouraging
tail evidence, not a guarantee that longer workloads, different hardware or WAL
checkpoint pressure cannot stall. No WAL-specific no-work ceiling, fixed-arrival
SLO, sustained checkpoint campaign or crash recovery campaign was measured.
Controlled-host qualification and a current SQLite/runtime comparison remain open;
no production journal setting or Cloudflare-managed database was changed.

## Startup, artifact size and decision

Twenty fresh processes per target, in alternating order, initialise private SQLite
and return one verified HTTP read. There is no application preflight or warmup;
OS caches remain warm, and timing excludes shutdown/cleanup.

| Median | Bun | Candidate |
| --- | ---: | ---: |
| Process to ready | 70.455 ms | 27.543 ms |
| First HTTP after ready | 3.741 ms | 4.734 ms |
| Process to first verified response | **74.149 ms** | **32.377 ms** |

Process-to-response ranges were 72.854–107.945 ms and 31.019–36.361 ms. This does
not measure Cloudflare cold starts. The module is 172,443 raw / 66,219 gzip bytes,
versus 154,462 / 60,516 previously: +11.6% / +9.4%. It retains the prior experiment's
unused ingress variant and is not a minimised deployment artifact.

Keep the bounded row representation as experimental evidence. Its small read gain,
small full-row regression and larger module do not justify default adoption.
For the read gap, direct schema-generated decoding that avoids constructing the
initial generic JSON tree remains unimplemented. For writes, WAL/FULL on a
controlled host with matching Bun/Wasm settings is the strongest next qualification
step, including sustained checkpoint behaviour and recovery. Full backend coverage,
production authentication and hosted evidence still gate any target switch.

## Correctness and scope

Thirty-three Bun codec/runtime/adapter tests passed with 11,949 assertions, plus a
separate diagnostic commit/rollback test and six native Rust tests. Fifteen probe
and sixteen runtime cases passed; two existing hosted cases remain unexecuted.
Five rejection gates and four adversarial rollback cases passed. Source constraint
mutation, actual entity/field renaming and a clean byte-identical rebuild passed.
Coverage includes exact JSON bounds, noncanonical JSON spelling, forged size
metadata, malformed rows, copy isolation, reset/stale handles and 10,000 leases.

The earlier fault-location fallback for oversized host frames remains. Pinned Bun
1.2.20 SQLite's leading-BOM storage issue remains; transport-only tests preserve it.
There is no latest-artifact Cloudflare qualification, production authentication,
complete language coverage or default backend change.

All four HTTP runs together verified **13,166,215 requests with zero errors**, in
188 measured cells with 94 servers / 752 passing preflight cases. This includes
instrumented diagnostic traffic; it is not one combined performance score. The two
standalone write diagnostics verified an additional 1,297,855 calls.

[Reproduction](../../experiments/wasm-exp1/typed-values/README.md),
[paired summaries and gates](../../experiments/wasm-exp1/typed-values/results.json),
[ordinary HTTP table](../../experiments/wasm-exp1/typed-values/table.md),
and [raw-evidence manifest](../../experiments/wasm-exp1/typed-values/evidence/manifest.json).
