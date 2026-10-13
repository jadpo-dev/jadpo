# Jadpo

Jadpo is a programming language for software written mainly by LLMs and
reviewed by people. The language is fully specified; the compiler is being
rebuilt around one graph, in Rust, with an interpreter for development and
compiled native and WebAssembly deployables.

## Where things are

- Outstanding work, in order: [docs/roadmap.md](docs/roadmap.md). It is the
  only tracker; each session takes the first unticked chunk.
- What the language means: [docs/language](docs/language/README.md), eighteen
  decided areas. Where anything else disagrees, the area page wins.
- How it is built: [docs/compiler-runtime.md](docs/compiler-runtime.md).
- Other decisions: [docs/decision-register.md](docs/decision-register.md).
- The archived v0 compiler: git tag `archive/compiler-v0`; superseded
  documents: [docs/archive](docs/archive/README.md). Read them only when a
  chunk needs the background, and never copy their special cases.

## "Continue the roadmap"

This request runs unattended: follow "How a session works" in the roadmap.
Name the next chunk and its model and effort tag in one line, then carry on
without waiting for a reply. Do each chunk's building in a subagent started
with the chunk's tagged model and effort; Richard asks for this explicitly,
so setting the subagent's `model` and `effort` from the tag is authorised.
Keep going chunk after chunk, merging each through its own pull request,
and stop only at the points the roadmap lists. Report at the end what was
merged and anything that needs Richard.

## Working rules

- `python3 tools/verify.py` is the gate. It must pass before a chunk merges.
- Branch from `main` per chunk, commit and push often, merge through a pull
  request when CI passes. Never create worktrees under `/private/tmp`.
- Plan each chunk in the session from its area pages; write their examples
  and errors as failing tests first. No plan documents, evidence packets,
  review records, timing logs or progress reports.
- Never special-case the compiler for an example application. If a chunk
  needs a language choice the pages do not make, stop and ask Richard with a
  recommendation.
- Every user-facing screen (CLI output, generated pages) follows the
  [CLI design language](docs/language/research/14-cli-design-language.md) and
  is designed with the design skills in `.agents/skills/` before it is built.
- Update the roadmap only when a chunk finishes or the order changes.
