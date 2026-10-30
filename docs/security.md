# Security

Who attacks something built with Jadpo, or Jadpo itself, what each can
reach, and what stops them. The rules themselves are on the area pages and
in [compiler and runtime](compiler-runtime.md); this page says which ones
carry the weight. How to report a vulnerability is in
[SECURITY.md](../SECURITY.md).

## A Jadpo app

| Who | What they can send | What holds them |
|---|---|---|
| Anyone on the internet | Any bytes to any route | The request edge ([area 7](language/07-http.md), "What is built"; compiler/runtime §2.6, "Deadlines and request limits"): hyper's framing, 1 MiB bodies, header, body and write timeouts, 1,024 connections natively and one rate-limit count per client; generic 500s that name no internals |
| A signed-in user | Requests under their own session or token | [Area 8](language/08-identity.md): 32-byte random credentials kept only as BLAKE3 verifiers, constant-time comparison, `SameSite=Strict` cookies with a CSRF token and an `Origin` on writes; passkeys added and managed only from a browser session |
| A user of another tenant | Ids and references of records they can't see | [Area 9](language/09-policy.md): every statement's grants and tenancy in its own SQL; references kept inside the tenant; a record moved to another tenant only where its mover could create it there; unique rules within each tenant |
| A webhook sender | Requests to the app's webhook routes | [Area 11](language/11-integrations.md), "Webhooks": each scheme checked over the raw body in constant time, a 5-minute window and 30 days of ids against replays |
| A provider, or whoever answers for it | Answers to the app's calls | One outside-call path: HTTPS only, no redirects, a size cap and timeouts, answers decoded to the operation's type; natively, names resolved and private, loopback, link-local and metadata addresses refused before connecting; on Workers, `global_fetch_strictly_public` |
| Someone whose text reaches a call | Values in a provider's path, query or body | Placeholders escaped as one path segment each (`.`, `..` and empty refused); query and body as JSON |
| A model | Its answers | No tool calling; data blocks escaped against closing tags in every encoding ([area 21](language/21-ai.md)) |
| A package the app installs | Its own code, run inside the app's actions | [Area 11](language/11-integrations.md), "What package code may do": it calls only its own API, through the connection the app gave it, and reads none of the app's settings or keys, data or sign-in |

On Workers the same runtime runs: secrets are Wrangler secrets, never
`vars`; module globals hold the program, counters and caches but no
request's data; randomness is Web Crypto's; the `scheduled` and `queue`
handlers and the Worker's binding to itself are the platform's alone, with
no HTTP route to start background work.

## A developer's machine

| Who | What they can do | What holds them |
|---|---|---|
| A package, or an example someone clones | Put any file in the project | Its Jadpo code, interpreted by `jadpo test` and `dev` or compiled by `jadpo build`, reaches no file, process or variable but what its connections and settings name, and the generated Rust takes its names and text as identifiers and literals; files are read only inside the project, and a link out of it is unreadable (compiler/runtime §2.2, "Loading"); `jadpo dev` and its inbox listen on `127.0.0.1`, the inbox checking the `Host` header against DNS rebinding |
| Whoever serves the Rust toolchain `jadpo build` fetches | Answer its downloads | The channel manifest's SHA-256 pinned in the release, each archive checked against the manifest before it is unpacked, nothing unpacked outside its folder (compiler/runtime §2.8, "What a released `jadpo` builds with") |
| A workspace's own editor settings | Name the binary the VS Code extension runs | The extension is off in an untrusted workspace (`untrustedWorkspaces: false`) |

## Fuzzing

What anyone can send reaches three surfaces, each a coverage-guided fuzz
target in `fuzz/` ([cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz)
over libFuzzer, a crate outside the workspace with its own lock), whose
properties are in `fuzz/src/`:

| Target | What it is given | What it holds |
|---|---|---|
| `parse` | Any text, as a file, a snippet and a template | The parser's guarantees (compiler/runtime §2.2): no panic, no lost text, one error a line with its edits inside the text, clean edits converging; what parses cleanly formats without `format.unsafe` |
| `check` | A loose file, or a project's files each headed `//// <path>` | Loaded by the loader's rules, checked, and planned for the interpreter when it checks: no panic |
| `request` | An application byte, then a request as text: `METHOD TARGET`, header lines, a blank line, the body | Only what hyper could hand over, made as the native host makes it, answered by a teaching example with routes or webhooks served by the interpreter, then read by the input reader of the route its path finds: no panic |

The seeds are the teaching examples, the golden todo, the parser's broken
files, the error pages' `jadpo` blocks, and a request for each route of
each served example, its placeholders and input made from their types.
With no sanitizer (the code forbids `unsafe`), cargo-fuzz runs on the
pinned stable toolchain:

```sh
cargo install cargo-fuzz --version 0.13.2 --locked
cargo fuzz build -s none
fuzz/target/<host triple>/release/seeds
cargo fuzz run -s none request fuzz/corpus/request -- -max_total_time=600 -timeout=30
```

A crash's input is written under `fuzz/artifacts/<target>/`, and
`cargo fuzz run -s none <target> <file>` runs it again. Each one found is
fixed with a regression test in the crate it was in. CI's `Fuzz` workflow
runs each target for five minutes every night on `main`, and on a pull
request that changes `fuzz/`; a crash, timeout or out-of-memory fails it
and keeps the input.

## Supply chain

- Rust dependencies are pinned in `Cargo.lock` and come only from
  crates.io. The gate runs `cargo audit --deny warnings` (no RustSec
  advisory, no yanked crate) and `cargo deny check` against
  [`deny.toml`](../deny.toml) (permissive licences, MPL-2.0 only for
  Servo's CSS parser, no other registry or git source, no wildcard
  versions). An advisory is ignored only with its reason beside it, in
  `deny.toml` and `.cargo/audit.toml`.
- npm is used only on this machine and in CI (benchmarks, the Workers
  test bed, the extension), from lockfiles, with `npm ci --ignore-scripts`
  wherever the gate installs and wherever CI packages or publishes the
  extension, so no dependency's install script runs.
- Dependabot opens grouped updates and security updates for every lockfile
  and for the workflows' actions.

## CI and releases

- Every workflow is read-only by default; a job that writes (release-please,
  attaching to a release, publishing it) says so itself, and a job that
  runs the repository's code writes nothing but its release.
- Every action is pinned to a commit, its release in a comment beside it.
  No workflow runs on `pull_request_target` or `workflow_run`, and no
  checkout keeps its credentials.
- A pull request from a fork gets a read-only token and no secrets; the
  secrets (release-please's token, the extension's publishing tokens) are
  read only on a push to `main` or a release started by hand.
- dist builds the installers on each release, installed itself from its
  own release archive checked against a SHA-256 pinned in
  `.github/install-dist.sh`, and writes a SHA-256 beside each archive.
- Each archive and installer dist builds, and the VS Code extension's
  `.vsix`, has a GitHub artifact attestation, which `gh attestation verify`
  checks (documentation, "Installing"): dist's two build jobs attest what
  they build, and the extension's `attach` job the `.vsix` before
  attaching it. Only those jobs may mint an OIDC token.
- CodeQL scans the Rust, JavaScript and TypeScript, Python and workflow code
  on each pull request to `main`, each push to `main` and weekly
  (`codeql.yml`, no build), leaving out only the crates' integration
  tests and `jadpo-value`'s known-answer examples, whose passwords, keys
  and secrets are test vectors. No release step runs a script fetched
  unchecked: dist comes from `.github/install-dist.sh`, and no build
  installs Rust by `curl | sh`. The OpenSSF Scorecard runs on each push to
  `main`, on a branch protection change and weekly (`scorecard.yml`) and
  publishes its result. Both upload to code scanning, so findings are under
  the repository's Security tab, Code scanning; only their jobs write
  `security-events`, and the Scorecard job also mints an OIDC token to
  publish. Dependabot keeps their actions current.
- Issues and feature requests will be read by an agent (roadmap, D4). Issue
  text is anyone's, so that agent holds no secrets, has a read-only token
  where it reads, and can't merge, release or change a workflow; what it
  writes is a pull request that passes the same gate and review.
