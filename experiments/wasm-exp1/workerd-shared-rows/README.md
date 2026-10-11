# Shared ownership of typed input frames — unselected experiment

Follow-up to [SIMD/bulk-memory results](../../../docs/experiments/wasm-guest-scans-results.md).
This local workerd selection pass tests whether avoiding per-field Rust string
copies closes the remaining gap to generated JavaScript.

`alloc` still creates fully initialized, bounded guest-owned buffers. Its registry
holds `Rc<Vec<u8>>`; typed fields hold a clone and a checked byte range after full
UTF-8 validation. The buffer therefore outlives the decoder and every field that
uses it. Generic JSON rows keep ordinary owned strings; materialization creates
independent strings and drops receipt provenance as before. No allocation or
permission result is cached across requests. No reference points into freed data.

The unchanged experiment driver writes each new allocation before handing it to
resume, never overwrites accepted input, holds an exclusive guest lease over
awaits, and only resets a completed request. These are required ownership rules.
This is not permission for an arbitrary embedder to mutate a live frame through
exported memory. As with the earlier Rust guest, the host must obey the memory
ABI. The shared variant is not promoted to a generic external-host ABI.

The 64 KiB physical/logical bounds, SIMD UTF-8/exact escaping checks, source
validators, authority adapter, error envelopes, fresh SQL and receipt checks are
unchanged. New native tests prove an offset frame survives dropping its original
owner, independently materializes edited JSON, and is freed after the final field
owner drops. Source mutation and clean rebuild checks are repeated for this guest.

## Result

**Unselected.** Five rotated isolated repetitions of JS, frozen bulk-memory WASM,
shared rows and no-op across six workloads produce 120 cells and 240,000 verified
measured calls. Each warms 200 and checks 2,000 in one Object batch. Median times
in microseconds:

| Target | Small | 16 KiB ASCII | Escaped Unicode | 48 KiB ASCII | Unicode only | Late escapes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Generated JS | 16.05 | 18.73 | 33.34 | 25.07 | 81.53 | 36.22 |
| Frozen SIMD + bulk memory | 20.14 | 28.67 | 49.37 | 44.00 | 117.01 | 65.00 |
| Shared input frames | 20.00 | 28.90 | 48.68 | 43.33 | 115.30 | 64.14 |

Most gains are about 1–2%; 16 KiB ASCII regresses slightly. These are small
within-run descriptive changes, not a robust broad improvement or JS parity.
Additional ownership complexity is not justified by this evidence. Bulk-memory
copies are already cheap. No new HTTP qualification or default target switch
follows; keep the frozen `workerd-guest-scans` candidate.

The shared guest passes **52 host tests / 14,733 assertions and 10 native Rust
tests**. Actual WASM differential cases compare the original guest, frozen bulk
guest and shared guest, including malformed UTF-8 and exact logical limits.
Native tests do not stand in for WASM/SIMD checks. The renamed/refined source
fixtures govern behaviour and a clean cache rebuild reproduces the same module.
The per-driver pool statistics aggregate both measured modes, so they are not used
to claim a per-mode memory saving. Total process memory is not measured.

## Reproduce

Same pinned tools, `simdutf8 0.1.5`, checked projection and fixtures as the preceding
pass. All frozen prior module imports are copied from their experiment paths.
Run builds, tests and measurements sequentially.

```sh
sh experiments/wasm-exp1/workerd-shared-rows/build-guest.sh
python3 experiments/wasm-exp1/workerd-shared-rows/mutation-rebuild.py
cargo test --offline --locked --features vector --manifest-path experiments/wasm-exp1/workerd-shared-rows/compiler/Cargo.toml --target-dir experiments/wasm-exp1/workerd-shared-rows/compiler/build/native-tests -- --test-threads=1
bun --no-install --env-file=/dev/null test experiments/wasm-exp1/workerd-shared-rows/*.test.ts experiments/wasm-exp1/boundary-http/cached-adapter.test.ts experiments/wasm-exp1/boundary-http/descriptor.test.ts
node experiments/wasm-exp1/workerd-shared-rows/build.mjs
caffeinate -i node experiments/wasm-exp1/workerd-shared-rows/runner.mjs --micro
python3 experiments/wasm-exp1/workerd-shared-rows/summarize.py
```

The inherited runner's HTTP mode is available but was not run for this unselected
variant. Selection is not HTTP latency/capacity. No deployment, production CPU,
hosted cold-start or full backend-coverage claim follows. The evidence manifest
retains raw results, tests, generated/source code and exact modules/bundle.
