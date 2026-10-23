# Decision register

**Updated:** 2026-10-15. Decisions that are not about the language itself.
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
option. The targets are built in roadmap Phase 8 (compiled deployables).

### Language specification — 2026-10-13

Richard decided all eighteen areas of the language review between 12 and 13
October 2026. The area pages in [docs/language](language/README.md) are the
specification; where an archived document disagrees, the area page wins.

### Examples teach every feature — 2026-10-14

Richard: some people learn better by example, so `examples/` holds carefully
written Jadpo projects in real project layout that together use every
feature the compiler has built, with inline comments throughout explaining
what each part does and why. They are teaching material first and the test
corpus second: each has its own `.test.jadpo` tests, the gate runs `jadpo
test` over all of them, and Phase 8 runs them interpreted and compiled. They
add to the area pages, which stay the specification, and are not copies of
the golden todo. A gate check fails when a built feature (syntax, declaration
setting, standard-library operation, graph node kind) appears in no example.
Every chunk adds or extends examples for what it builds; chunk 2.4 covers
Phases 1 and 2.

Revised 2026-10-15 (Richard): examples are brought up to date once per
phase, in an examples chunk at the phase's end, and the coverage check
blocks only there (`tools/verify.py --require-examples`); every chunk still
keeps every example passing.

### Errors for habits from other languages — 2026-10-15

Richard: people and agents learning Jadpo will write constructs from other
languages out of muscle memory, so each common one gets its own error. A
catalogue surveys TypeScript and JavaScript, Python, Rust, Go, Java and C#,
Kotlin and Swift, and SQL for constructs with an unambiguous Jadpo spelling
(`===`, `&&`, `let`, `fn`, `null`, `=>` in a `match`, `try`/`catch`,
`switch`, lowercase built-in types, `T[]`, `?.`, `??`, `import … from`, `#`
comments, `x++`, semicolons and the like). Each gets one rule whose message
names the habit and the Jadpo form, with an edit, and parsing carries on as
if the Jadpo form had been written. Behind it, a syntax error never produces
a second error on the same line, checked over a corpus of broken files.
Built in roadmap chunk 3.8.

### Licence — 2026-10-15

Richard: Jadpo should be free to use, fork and build commercial software
with, and leave room for commercial offerings of his own. The code (compiler,
runtime, standard library, tools, editor extension, examples) is Apache-2.0
alone: its patent grant protects users from contributors' patents, and
unlike a dual MIT OR Apache-2.0 licence it lets nobody take Jadpo under MIT
to escape the clause that ends the licence of anyone who sues over patents.
The documentation is CC BY 4.0. A source-available licence (FSL) that would
forbid selling Jadpo itself was considered and rejected: it would cost
adoption, and nobody pays for a copy of something free. Instead
[TRADEMARKS.md](../TRADEMARKS.md) requires a modified version to use a
different name. Commercial offerings will be separate code, licensed when
they exist.


### Connection URL structure — 2026-10-23

Richard: validate the structure of typed connection URLs at startup and
`config check`, alongside the existing scheme and connection-owned TLS
checks. Keep valid PostgreSQL multi-host, IPv6, encoded credential and
explicit socket forms; do not resolve hosts or try connections to decide
whether configuration is valid. Collect every invalid declaration and never
show credentials. Built in chunk 9.1d2a in the shared built-in text validator;
[area 10](language/10-configuration-and-time.md#database-and-other-connection-urls) specifies it.
