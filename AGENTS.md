# Jadpo

Jadpo is a programming language for software written mainly by LLMs and
reviewed by people. The language is fully specified; the compiler is being
rebuilt around one graph, in Rust, with an interpreter for development and
compiled native and WebAssembly deployables.

## Where things are

- Outstanding work, in order: [docs/roadmap.md](docs/roadmap.md). It is the
  only tracker; each session takes the first unticked chunk. Finished
  chunks' full entries: [docs/archive/roadmap-finished.md](docs/archive/roadmap-finished.md).
- What the language means: [docs/language](docs/language/README.md), twenty-one
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

## Who builds what (Richard, 2026-10-22)

Claude finishes Phase 8 (the review chunks 8.4a–8.4j) as usual. Codex
builds Phase 9, performance, starting once 8.4c, 8.4e and 8.4f have
merged, since they change what a deployable runs and how it is built.
When Codex builds a chunk it works on its own `chunk-<id>` branch in a
worktree beside the repository, follows the working rules below and
passes the full gate, ticks the chunk as the roadmap's "How a session
works" says (one line naming where what it built is written), pushes, and
opens the pull request **without** `--auto`, its description saying what
was built, what the gate showed and any decision it recorded. It never
merges. Claude reviews the pull request against the area pages, fixes
what it finds on the same branch, and merges it.

## Performance results and review (Richard, 2026-10-23)

Record every optimisation round in the existing performance sections of
`docs/compiler-runtime.md` and the relevant `tools/bench/README.md` method,
so its pull request carries the results and the reasoning a reviewer needs.
Include what changed and why; the exact reference and candidate revisions;
the workload, build settings, host conditions and reproduction commands;
before/after results with run variation, CPU and other relevant trade-offs;
raw result paths and digests; and the correctness checks and invariants that
must remain true. Retain unsuccessful experiments and superseded protocols
with their lessons and explain which results can be compared. Use these
existing fact and method sections, not another tracker or a standalone packet.
Link the record from the pull request description.

Keep the deterministic regression tests and benchmark controls that protect
the improvement. If review changes a measured path, rerun the affected
comparison and update its record, making any lost gain or correctness trade-off
explicit. Do not silently restore an older implementation or omit an adverse
result. Correctness still takes precedence over speed.

After each optimisation, lead Richard's update with Jadpo on Workers versus
Bun: before/after throughput and p95 latency where the workload supports them,
and the remaining gap. This is Phase 9's main target. Label local workerd
measurements explicitly; they are not real Workers results. Match and disclose
process lifetimes and startup/pool warmup in cross-target comparisons; a
persistent workerd isolate versus fresh Bun processes is not a warm-service
comparison. Native Jadpo and
Node comparisons provide supporting context where the same workload supports
it. Name the workload and its scope, variation and trade-offs; do not imply
that a background-work result also measures HTTP requests. After each completed
chunk, also report what was built, the gate result and the PR link in three
lines. Codex never merges these pull requests or enables auto merge.

## Working rules

- `python3 tools/verify.py` is the gate. It must pass before a chunk merges.
  Run the full gate once per chunk, where the building happens; the session
  that merges doesn't run it again. After merging `main` into the branch it
  runs `--profile quick` only if the merge brought in code, then pushes and
  lets CI run the full gate on the result (Richard, 2026-10-15).
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
- Keep the process lean (Richard, 2026-10-15):
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
