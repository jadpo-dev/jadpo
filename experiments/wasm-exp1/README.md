# WASM-EXP1

Experimental compiler and host work only. The production target remains Bun.
The fixed protocol is [the preregistration](../../docs/experiments/wasm-experiment-plan.md).

The authentication baseline is commit `4dc5604`. Its 45-step validation report
is `build/validation/20260930T024217-25646/report.json` (SHA-256
`7325a2ce5851b6e63361dfc2185b3b0c145fab7ba8550a3b3ed0d8acfd318ad9`).
External release gates and the complete golden app remain open.

Sources, compiler experiments, tooling locks and compact evidence live here.
Generated output, installed tools and raw logs live under `build/wasm-exp1/`.
No handwritten replacement of generated application code counts as a pass.
Fixture principals are synthetic trusted harness inputs, not authentication.

## Rebuild and inspect

Read the [results and limitations](../../docs/experiments/wasm-experiment-results.md) before
interpreting a passing fixture as backend readiness. Tool installation is pinned
in [tooling](tooling/README.md). The [compiler projection](compiler/README.md)
uses the existing checked frontend; the original production target is unchanged.

With the documented tools installed, run these from the repository root:

```sh
cargo run --offline --locked --manifest-path experiments/wasm-exp1/compiler/Cargo.toml -- experiments/wasm-exp1/fixture experiments/wasm-exp1/compiler/build/projected --bun
bun --no-install --env-file=/dev/null experiments/wasm-exp1/baseline/run.ts
sh experiments/wasm-exp1/rust/build.sh
sh experiments/wasm-exp1/direct/build.sh
sh experiments/wasm-exp1/rust-full/build.sh
sh experiments/wasm-exp1/rust-full/mutation.sh
bun --no-install --env-file=/dev/null experiments/wasm-exp1/full/run-local.ts experiments/wasm-exp1/rust-full/build/probe.wasm --mutated=experiments/wasm-exp1/rust-full/build/mutated-probe.wasm
```

Individual route READMEs include their mutation, rejection and edge-case checks.
The full runtime suite intentionally reports A17/A18 separately; use the linked
compiler rejection and rebuild commands to reproduce those gates. Every cloud
URL in evidence identifies a historical disposable deployment. Both Workers and
their synthetic Durable Object namespace were removed after verification; see
[evidence/cleanup.json](evidence/cleanup.json). Reproduction does not silently
recreate cloud resources.

Raw acceptance evidence is retained under `evidence/raw/*.gz`. Decode with
Python's `gzip.decompress(Path(file).read_bytes())` or a standard gzip reader;
`evidence/raw-manifest.json` records uncompressed SHA-256 digests. Generated Rust,
WAT, Wasm and projections live in ignored per-route `build/` directories; larger
measurement and Cloudflare packaging logs are under the root `build/wasm-exp1/`.
The source-map/entry inventory is also retained in `rust-full/report.json`.

Run timing workloads sequentially, with other experiment jobs idle. The frozen
throughput command is:

```sh
bun --no-install --env-file=/dev/null experiments/wasm-exp1/measurement/throughput.ts experiments/wasm-exp1/rust-full/build/probe.wasm
python3 experiments/wasm-exp1/measurement/summarize-throughput.py
```

It takes roughly half an hour at the frozen forty-run duration. The summarizer
rejects partial runs and semantic errors. Read its qualifications before using
the results; this prototype includes fresh per-request Wasm instances and JSON
copies. Boundary/startup and matched-build commands have separate READMEs under
`measurement/` and `rust-full/`.
