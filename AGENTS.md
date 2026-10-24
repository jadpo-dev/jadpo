# Jadpo

Jadpo is a programming language for software written mainly by LLMs and
reviewed by people. The language is fully specified; the compiler is being
rebuilt around one graph, in Rust, with an interpreter for development and
compiled native and WebAssembly deployables.

## Where things are

- Outstanding work, in order: [docs/roadmap.md](docs/roadmap.md). It is the
  only delivery tracker; each session takes the first unticked chunk. Finished
  chunks' full entries: [docs/archive/roadmap-finished.md](docs/archive/roadmap-finished.md).
- Performance exploration: [docs/performance-experiments.md](docs/performance-experiments.md)
  is the canonical hypothesis queue, dependencies, retained results and
  cross-session routine. It does not tick delivery chunks.
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
Keep going chunk after chunk, following the builder-specific pull request
rule below, and stop only at the points the roadmap lists. Report what was
opened or merged and anything that needs Richard.

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

## Performance exploration (Richard, 2026-10-24)

Follow [the experiment catalogue](docs/performance-experiments.md) across
sessions. A dedicated discovery agent maintains credible hypotheses, potential
savings calculations and impact order. Run cheap independent prototypes from
one exact frozen baseline in a reusable sibling scratch worktree; explicitly
stack dependent experiments on retained prerequisite commits and measure both
marginal and combined gains. Freeze each variant's prediction, workload,
metric and reference before implementation; retain the original and record
actual throughput gain beside it, including marginal/combined comparisons.
Save every source/harness commit under a durable Git ref and retain raw
results/artifacts, including losses and adverse metrics, before resetting the
working branch. Stop owned builds/services, including PostgreSQL, and
coordinate a quiet host during measurements.

Explore the available queue, including emerging ideas, to exhaustion before
proper production implementation. There is no maximum experiment count or
batch/task/time quota and no early promotion after a working window. Record
blocked prerequisites, accounts/keys/deployments or language choices and
continue eligible items; a blocked queue is not empty. The already-built 9.2a3
keeper may finish its existing qualification and manual review.

The catalogue is an explicit exception to the no-plan-document rule below.
New ideas/results update it with evidence, dependencies and maintained order.
Coordinate open catalogue PRs and ID allocation first: update a pending PR for
the same scope or serialize edits; batch related results/ideas when useful.
Do not create concurrent conflicting catalogue branches. Catalogue/workflow-only PRs branch from current main and use
`gh pr merge --merge --auto` with required CI, without admin/bypass; never
include compiler/runtime/keeper code or CI changes. This is the only Codex
auto-merge exception. Experiment baseline B is separately frozen and may be an
explicitly authorized unmerged source PR stack; do not assume main contains
its harness or copy its completion ticks. Cheap prototypes need focused checks
and short matched measurements, not full qualification/gate/PR per experiment.
After exploration, production keepers still need proper implementation,
interaction tests, longer confirmation, exact production checks, qualification
and the full gate per delivery chunk; Codex opens them without auto-merge and
never merges. Pure catalogue maintenance uses appropriate documentation and
consistency checks plus required CI; it is not a code delivery chunk and does
not force a full local Rust/native gate.

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
  review records, timing logs or progress reports, except the single
  performance catalogue and linked retained raw results authorized above.
- Never special-case the compiler for an example application. If a chunk
  needs a language choice the pages do not make, stop and ask Richard with a
  recommendation.
- Every user-facing screen (CLI output, generated pages) follows the
  [CLI design language](docs/language/research/14-cli-design-language.md) and
  is designed with the design skills in `.agents/skills/` before it is built.
- Update the roadmap only when a chunk finishes or the order changes.
- Keep the process lean (Richard, 2026-10-15):
  - Claude's delivery PRs may use `gh pr merge --merge --auto`. Codex code
    PRs follow "Who builds what" above: open for Claude, never auto-merge.
    Only the catalogue/workflow documentation exception above auto-merges.
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
