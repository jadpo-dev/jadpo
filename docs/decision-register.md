# Decision register

**Updated:** 2026-10-13. Decisions that are not about the language itself.
Every language decision is recorded on its area page in
[docs/language](language/README.md), and the rendered decision log collects
them. Decisions made before the language review, most of them now superseded
by it, are kept in [the archived register](archive/decision-register-2026-10-13.md).

### Build approach — 2026-10-13

Richard: build a new compiler around one graph (compiler-runtime §1.1–1.2)
and archive the current one, tagged `archive/compiler-v0` and deleted from
`main`, referenced through git. Two executors share one runtime crate: an
interpreter inside `jadpo` for development and tests, and compiled Rust for
deployables, native and WebAssembly; every test runs both ways. The first
build is the graph core, both executors and what the golden todo uses. A
performance phase compares native, WebAssembly and an equivalent Bun app and
optimises WebAssembly towards Bun. The delivery plan is the
[roadmap](roadmap.md).

### Runtime targets — 2026-10-13

Richard: the TypeScript/Bun target is to be dropped. Jadpo targets native Rust
for long-running servers and WebAssembly for hosts such as Cloudflare Workers.
Language features must work on both; where a host supplies a mechanism (for
example scheduling), Jadpo offers its own by default and the host's as an
option. How and when the targets are built is part of roadmap step 3. Not yet
implemented.

### Language specification — 2026-10-13

Richard decided all eighteen areas of the language review between 12 and 13
October 2026. The area pages in [docs/language](language/README.md) are the
specification; where an archived document disagrees, the area page wins.
