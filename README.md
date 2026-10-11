# Jadpo

**Jadpo — one language for the whole web.**

Jadpo is a programming environment designed for software written primarily by
LLMs but reviewable and editable by humans. Its current implementation focuses
on the backend and is still a design and falsification exercise.

The central premise is that an LLM may propose an implementation, but a boring,
deterministic compiler must decide whether that implementation is valid and
permitted. Human review should focus on intent, policy, and explicit exceptions
rather than generated framework code.

Start with the [documentation index](docs/README.md). It explains the reading
order, the authority of each document, and how decisions are recorded.
The canonical public home for the project is [jadpo.dev](https://jadpo.dev/).
Source and pull requests are hosted at
[richardwillars/jadpo](https://github.com/richardwillars/jadpo).
See [Contributing](CONTRIBUTING.md) for setup, branches, worktrees and review.
Licensing remains undecided.

Implementation progress is tracked in the
[Jadpo implementation roadmap](docs/roadmap.md). The first implementation
contract is the [core grammar](docs/grammar-v0.1.md) exercised by the
[Jadpo seed application](examples/jadpo-seed/app.jadpo).

The project remains a design and falsification exercise. A Rust
compiler workspace now implements the first complete semantic pipeline:
recovering syntax, deterministic identities, nominal typing, validated
construction, closed failure/effect checking, deterministic derived artifacts,
and a reproducible static project scaffold. The Jadpo seed now runs through
a generated TypeScript/Bun HTTP target with boundary validation and safe
failure mapping. A bounded authored module system now enforces explicit module
identity, selective imports, private-by-default visibility, and acyclic
dependencies while preserving legacy header-free projects. The persistence slice is complete and the active P10R phase
now has a candidate golden-todo contract, proof kernel, threat model, approval
protocol, and preregistered comparison protocol awaiting independent review.
Jadpo is not yet a general-purpose web environment.

Build the experimental shared Rust fixture from the repository root:

```sh
jadpo build --target wasm
jadpo build --target native
```

Each command checks the fixture, generates shared Rust, and builds the selected
release target. These are the local [shared-core experiment](experiments/native-conformance/README.md),
not a default-target switch. Bun is not needed. If the CLI is not installed, use
`cargo run --offline --locked --manifest-path jadpo/Cargo.toml -p jadpo-cli -- build --target native`
(or `wasm`). The `bun run build:rust` and `bun run build:wasm` shortcuts also remain
available. See the experiment README for prerequisites and output paths.

The [authenticated policy experiment](experiments/auth-policy/README.md) builds with
`jadpo build experiments/auth-policy/fixture --target native` (or `wasm`). It adds
opaque bearer verification, live authority checks, and owner/editor field policies.
Its [measured performance](docs/experiments/auth-policy-performance-results.md) and
[WASM host trust decision](docs/experiments/wasm-host-trust-decision.md) retain the frozen controls
and distinguish trusted application code from independently enforced guest authority.
The [capability-host follow-up](docs/experiments/capability-host-results.md) implements and tests
a separate trusted Rust authority with isolated application guests. Build it with
`jadpo build experiments/capability-host/fixture --target native` or `--target wasm`.
The [single-pass monitor prototype](docs/experiments/capability-monitor-results.md) removes the
reference application pass while retaining effect, policy, redaction and commit
checks. It is a bounded performance follow-up, not a production backend.

Run the supported-language validation gate with:

```sh
python3 tools/verify.py
```

The [validation guide](tests/validation/README.md) lists prerequisites, suite
coverage, retained reports and the separate full golden-application gate.
Passing supported checks does not close the recorded contract or release gaps.
