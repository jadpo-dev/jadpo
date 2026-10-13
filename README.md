# Jadpo

**Jadpo — one language for the whole web.**

Jadpo is a programming language for software written mainly by LLMs and
reviewed by people. A model proposes the code; a deterministic compiler
decides whether it is valid and permitted; a person reviews what it does
(who can now do what to which data) rather than reading generated framework
code.

## Where it stands

- **The language is specified:** eighteen areas, all decided in October 2026,
  in [docs/language](docs/language/README.md).
- **The compiler is being rebuilt** around one graph, in Rust, with an
  interpreter for development and tests and compiled deployables for native
  servers and WebAssembly (Cloudflare Workers). The plan is the
  [roadmap](docs/roadmap.md).
- **The first compiler** (generating TypeScript for Bun) is archived at the git
  tag `archive/compiler-v0`.

Start with the [documentation index](docs/README.md). The project's public
home is [jadpo.dev](https://jadpo.dev/); source and pull requests are at
[richardwillars/jadpo](https://github.com/richardwillars/jadpo). See
[Contributing](CONTRIBUTING.md). Licensing has not been chosen yet.
