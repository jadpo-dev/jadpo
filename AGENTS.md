# Jadpo

Jadpo is a programming language for software written mainly by LLMs and
reviewed by people. The language is fully specified; the compiler is being
rebuilt around one graph, in Rust, with an interpreter for development and
compiled native and WebAssembly deployables.

## Where things are

- Outstanding work, in order: [docs/roadmap.md](docs/roadmap.md). It is the
  only tracker; each session takes the first unticked chunk. A finished chunk
  is one ticked line naming where its work is written; its full text lives
  in git history.
- What the language means: [docs/language](docs/language/README.md), twenty-one
  decided areas. Where anything else disagrees, the area page wins. Language
  decisions are recorded in the area's design notes
  (`docs/language/notes/`), the rules they settle on the area page; each
  error's examples are in `docs/errors/<rule.id>.md`.
- How it is built: [docs/compiler-runtime.md](docs/compiler-runtime.md).
- Process rules: the working rules above.
- The first compiler (v0): git tag `archive/compiler-v0`. Never copy its
  special cases.

## Performance results and review

Each optimisation's pull request description carries its results: what
changed and why, reference and candidate revisions, before/after with run
variation, CPU and other trade-offs, and adverse results. The repository
keeps only the method in `tools/bench/README.md`, at most one short summary
table in `docs/compiler-runtime.md`, and the regression tests and benchmark
controls that protect the gain; no raw paths, digests, per-round tables or
logs of failed experiments. Never silently restore an older implementation
or omit an adverse result. Correctness takes precedence over speed.

Compare like with like:
Jadpo native against TypeScript on Bun and Node; Jadpo's Wasm build against
TypeScript on workerd, leading with CPU time per request, cold start and
p95. Label local workerd results as local, match and disclose process
lifetimes and warmup, and name the workload and its scope. After each
chunk, report what was built, the gate result and the PR link in three
lines.

## Working rules

- `python3 tools/verify.py` is the gate. It must pass before a chunk merges.
  Run the full gate once per chunk, where the building happens; the session
  that merges doesn't run it again. After merging `main` into the branch it
  runs `--profile quick` only if the merge brought in code, then pushes and
  lets CI run the full gate on the result.
- Branch from `main` per chunk, commit and push often, merge through a pull
  request when CI passes. Never create worktrees under `/private/tmp`.
- Plan each chunk in the session from its area pages; write their examples
  and errors as failing tests first. No plan documents, evidence packets,
  review records, timing logs or progress reports.
- Never special-case the compiler for an example application. If a chunk
  needs a language choice the pages do not make, stop and ask a maintainer with a
  recommendation.
- Every user-facing screen (CLI output, generated pages) follows the
  [CLI design language](docs/cli-design-language.md) and
  is designed with the design skills in `.agents/skills/` before it is built.
- Update the roadmap only when a chunk finishes or the order changes.
- Dependabot's grouped pull requests are merged when CI passes, without
  waiting for a maintainer.
- Keep the process lean:
  - Open each chunk's pull request with `gh pr merge --merge --auto`, so it
    merges itself when CI passes.
  - Run chunks that touch different crates and pages at the same time, each
    in its own worktree beside the repository; otherwise take them in order.
  - Teaching examples are brought up to date once per phase, in the phase's
    examples chunk. Other chunks keep every example passing but don't have
    to add new ones; the coverage step only blocks with `--require-examples`.
  - One independent review per phase, by two reviewers: soundness, and
    architecture for the phases ahead.
  - Building subagents report in a few short lines.
  - Wait for a long command by running it in the background and waiting
    for its own completion; never poll for a process by name (`pgrep -f`
    matches the loop itself and never ends). Stop anything you started
    before you report.
