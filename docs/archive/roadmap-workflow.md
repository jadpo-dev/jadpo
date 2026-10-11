# Jadpo roadmap workflow

**Status:** repository operating workflow, adopted 2026-10-01.

This file owns Jadpo-specific priorities, evidence gates and record locations.
Personal skills own reusable procedure; [AGENTS.md](../AGENTS.md) routes requests.
The [agent development workflow](agent-workflow.md) describes the intended product,
not how to work in this repository.

## Entry points and scope

Use `adaptive-delivery` for planning, implementation and review; `roadmap-tidy`
for restructuring; and `roadmap-add` for intake. The
[cheat sheet](workflow-cheat-sheet.md) has copyable commands and Goal prompts.
Planning-only work never becomes implementation without a request. A loop's
working window is not its total scope: continue eligible tasks within the selected
IDs, asking concrete owner questions once and saving a resume checkpoint.

Create a persistent Goal only when explicitly requested; use supported host
controls and lifecycle rules. Neither a skill invocation nor an ordinary loop
request authorises a scheduler, other chats, delegation or publishing.

## Adding ideas

The active roadmap owns stable IDs, epics, dependencies and execution state.
Check both it and history for overlap; keep conditional work behind its activation
condition. Record new ideas as needing planning, with honest unknown estimates.
Adding an idea does not expand an existing fixed task/Goal scope.

## Preserving roadmap coverage

The roadmap's discussion/legacy and language-issue indexes are part of intake
and restructuring checks. Keep recognisable names, motivation, constraints,
acceptance and rejected alternatives reachable from each task. A row is a
summary of its linked requirements, not permission to discard them. Every
moved/merged idea needs an active task, evidence-backed completion, explicit
superseding decision or conditional/deferred disposition. Preserve source
snapshots and original anchors; history is not a disposal location for open work.
Check every active language issue and newly discussed topic against this index
before declaring a reconciliation complete. Record source-access limits in
conversation-coverage.md; never claim unseen chats were checked. New recovered
scope does not automatically expand a separately authorised delivery loop.

## Goals and verify-loop

`adaptive-delivery` assesses and suggests `verify-loop` where measured feedback,
reproduction or verifier quality should guide the next change. Jadpo examples
include compiler correctness campaigns, intermittent runtime faults and capability
performance investigations. Routine implementation and documentation use ordinary
checks. Read verify-loop before campaign setup; retain its target and actual-model
checks. A Goal or "where appropriate" does not supply missing setup decisions.
Keep campaign contracts/raw results in the existing validation or experiment area
and link them from the task plan. A subtask's passing result does not close a Goal.

## Project sources and priority

Start at the [task reading paths and document ownership](README.md#start-with-the-task).
The [roadmap](implementation-roadmap.md) owns open work; [history](implementation-history.md)
owns completed evidence. [Decision register](decision-register.md), relevant specs
and [language issues](language-issues.md) determine semantic status.

While RM-213 is open, start unscoped work with its documentation audit. Then prefer
the complete golden todo on Bun and its required dependencies, respecting explicit
user priorities. Conditional work needs its named trigger. Resolve completed task
IDs through history. Compile-only evidence never satisfies runtime acceptance.

## Local execution and verification

Revalidate saved plans against current source and preserve concurrent changes.
Prefer one writer per checkout; authorised parallel implementation needs isolated
ownership/workspaces and integration checks. Plans are not locks.
The [contribution workflow](../CONTRIBUTING.md#branches-and-worktrees) defines
the shared `main` baseline, task branches, pull requests and integration ownership.

For compiler/runtime changes, use the
[supported validation gate](../tests/validation/README.md):
`python3 tools/verify.py`, with focused checks during iteration. Add
`--require-golden` before claiming complete golden behaviour. Documentation-only
changes need affected-link, consistency and diff checks. Missing runtime/external
gates stay visible; partial evidence is not release-equivalent success.

Self-review each change. Authentication, policy, transactions and public language
changes need their required review recorded. Use an independent reviewer when
authorised and available; otherwise leave that review pending. Self-review is not
independent evidence. Routine choices do not gain extra approval gates.

Close a task only with its acceptance evidence; move its row into history and add
its completed-task marker, retaining the stable ID and timing references. Put new
completion entries above the preserved historical snapshot. Partial sessions stay
open. Do not put progress logs in the active roadmap.

## Model recommendations

Use adaptive-delivery's entry model check and batch pinning. The roadmap's
`Next stage / model routing` column records the next work and candidate settings;
the selected batch plan/checkpoint owns the actual pin, boundary, mismatch and
resume condition. Linked plans hold supporting reasons and alternatives.
Assess rows when selected, including whether existing planning is sufficient.
`Assess when selected` is not evidence of suitability. Model deferral leaves
planning, execution and dependency status intact. The active chat's metadata or
explicit user confirmation establishes actual settings; defaults do not.

Richard's current preference is frugal routing: use the lowest sufficient model
and reasoning effort for the hardest expected work in each bounded batch. A batch
may span several tasks and their planning, implementation, verification and
self-review. Historical Luna Extra High/max preferences and stronger alternatives
are not overrides. At batch entry, pause on a mismatch in either direction,
including unnecessary capability/effort, and record the deferral. Once verified,
retain the pin through ordinary stage and task transitions; reassess only when the
batch ends, scope materially expands, evidence shows the pin is insufficient, or
work moves to a separate execution context or authorised reviewer. Generic or
incomplete metadata during continuation does not invalidate the recorded pin by
itself. Verify-loop applies the same approach to a bounded experiment batch;
`xhigh` and `max` remain distinct. Recommendations do not change settings. Follow
host Goal lifecycle rules while waiting; a routing pause does not itself authorise
changing Goal status to paused.

## Timing and calibration

Use the [local recorder](task-timing/README.md) before substantive task work:
`python3 tools/task-time.py start RM-xxx --actor CHAT_ID --category CATEGORY`
and include the original estimate and actual model/effort only when known.
Record work phases, pause/finish before non-work gaps and retain failed/partial
runs. Follow the guide for interrupted intervals and calibration. Agent effort,
human effort and external waiting are separate; no universal speed-up multiplier.

## Portability and unavailable skills

If a personal skill is unavailable, continue from this workflow with proportional
planning, unchanged selected models, existing acceptance checks and honest timing/
checkpoints. Report the limitation; do not stop merely to install a skill.

Follow the [documentation maintenance rules](README.md#document-ownership-and-maintenance)
when recording context. The [daily report](progress/latest.md) derives state from
roadmap/history/plan records under the [reporting rules](progress/README.md);
reporting never substitutes for task acceptance. Existing chats may need to be
asked to reload updated instructions; files alone do not resume or retask them.
