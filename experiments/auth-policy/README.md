# Authenticated shared-Rust policy experiment

Local follow-up to the shared-core conformance slice. The previous native/WASM
experiments remain frozen; Bun remains the default target. This is not a
production migration or authentication security qualification.

The checked fixture uses existing language features: opaque bearer credentials,
immediate revocation, live user resolution, direct owner/editor roles, narrowing
field read/update policies, and atomic paired writes/domain rejection. Rust and
WASM share the generated application plus credential verification, route
validation, policy SQL construction and failure mapping. Existing rusqlite and
Bun SQLite drivers execute prepared SQL; the Bun reference runs the unmodified
generated application/authentication/persistence with a budget wrapper.

Build without Bun (Python, Cargo, native SQLite development library and the
`wasm32-unknown-unknown` Rust target are required):

```sh
jadpo build experiments/auth-policy/fixture --target native
jadpo build experiments/auth-policy/fixture --target wasm
```

Direct shell equivalents are `sh experiments/auth-policy/build.sh native` and
`sh experiments/auth-policy/build.sh wasm`. These also work without an installed
Jadpo CLI. With no target argument the script builds both.

Builds use pinned offline dependencies. For an empty Cargo cache, first fetch the
locked dependencies for this workspace and `compiler/Cargo.toml`. The checked
projection compiler generates Bun reference source as data; building either Rust
target does not execute Bun. Outputs live under ignored `build/` directories.

The comparison harness uses Bun to issue **synthetic** credentials through the
existing generated authentication host and create disposable SQLite databases.
The native executable needs only its configured SQLite file and secrets at runtime.
Servers bind loopback; harness-only database controls require a separate test token.
Never deploy these benchmark/control adapters.

Excluded: signed/browser credentials, CSRF, JWT, service authentication,
memberships, indirect scopes, general project compilation, nested handled-write
savepoints, cloud/workerd testing and performance claims. Unsupported checked
plans must fail compilation. SQL traces separate auth reads, policy predicates,
transaction controls and generated Bun's additional reads/savepoints.

## Reproduce conformance

After building both targets, run from the repository root:

```sh
bun --no-install --env-file=/dev/null experiments/auth-policy/seed.ts
cargo test --offline --locked --manifest-path experiments/auth-policy/compiler/Cargo.toml --target-dir build/auth-policy/compiler > build/auth-policy/compiler-tests.log 2>&1
cargo test --offline --locked --manifest-path experiments/auth-policy/Cargo.toml --target-dir build/auth-policy/target -p jadpo-auth-policy-native > build/auth-policy/native-tests.log 2>&1
bun --no-install --env-file=/dev/null test experiments/auth-policy/wasm.test.ts > build/auth-policy/wasm-tests.log 2>&1
python3 experiments/auth-policy/mutation.py > build/auth-policy/mutation.log 2>&1
node experiments/auth-policy/run.mjs > build/auth-policy/http.log 2>&1
RUSTFLAGS='-C link-arg=--max-memory=8388608 -C link-arg=-zstack-size=262144' cargo build --offline --locked --release --manifest-path experiments/auth-policy/Cargo.toml --target-dir build/auth-policy/wasm-clean -p jadpo-auth-policy-wasm --target wasm32-unknown-unknown > build/auth-policy/clean-wasm.log 2>&1
python3 experiments/auth-policy/archive.py
```

Run these sequentially: generation and mutation use the same ignored output
paths. Mutation restores the original compiled artifacts and issues fresh test
credentials. Seed credentials expire after an hour; rerun `seed.ts` before later
HTTP runs. Unit tests use the saved issuance time for deterministic verification.
The harness creates a separate SQLite copy for each target/journal combination
and asserts all open connections use the expected durability settings.

The HTTP adapters have a common 32 KiB raw request-body limit and 64 KiB JSON
bridge-frame limit. Guest host replies/completions are bounded too. Bun's wrapper
checks storage/output budgets inside the generated transaction, before commit,
and rejects lone UTF-16 surrogates at the post-authentication JSON boundary to
match the Rust decoder.
These are experiment limits, not new language rules. HTTP cache headers and
request-ID correlation are checked; random IDs are normalized for comparison.

See [results and limitations](../../docs/experiments/auth-policy-conformance-results.md).
