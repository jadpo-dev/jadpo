# <Task ID or batch scope>: <outcome>

Use only for work needing a saved plan. Replace placeholders when creating a
plan; omit fields that do not help. Keep accepted semantics in their owning spec.

## Context

- Requested outcome and scope:
- Coordinating chat / workspace / branch / source revision:
- Relevant accepted specifications and decisions:
- Current evidence and pre-existing changes:
- Work mode: one planning batch, planning loop, implementation loop or review;
  planning-only scope does not authorise implementation:
- Persistence: ordinary task or explicitly requested Goal; Goal objective,
  completion evidence, scope and user-specified budget if applicable:
- Scope snapshot / selected task IDs; pending-input policy (continue independent
  work unless the user requests stopping at the first question):

## Current working window (usually 3–5 tasks; replenish in loop mode)

| Task ID | Planning depth and reason | Dependency evidence / blocker | Planning state | Execution state | Roadmap routing reference |
|---|---|---|---|---|---|
| <RM-xxx> | <Direct / Short plan / Decision or experiment; existing plan sufficient?> | <IDs + evidence> | <needs planning / planned / needs input> | <queued / ready / active / blocked / needs review / done> | <Link to next-stage/model note in roadmap> |

## Per-task plan

- Outcome and non-goals:
- Affected source surfaces / intended approach:
- Source/spec revision inspected; assumptions to revalidate before implementation:
- Invariants and acceptance scenarios, including relevant negative cases:
- Exact verification commands and expected observations:
- Remaining decision, options and owner (only if required):
- Recovery approach / experiment stop condition (if applicable):
- Required review and whether it is available:
- Recommended model/effort for planning, implementation and review, with reasons
  (omit stages that are unnecessary):
- Suitable alternatives and why; link the roadmap's current routing note instead
  of maintaining a duplicate mutable queue:
- Actual selected model/effort and verification source; if unknown, entry waits
  for confirmation. Model deferral/reason/resume condition or explicit override:
- Original estimate and timing run IDs (including retries and model changes):
- Verification mode: normal project checks or verify-loop campaign; if a campaign,
  link its contract, target confirmation/delegation, model-setting verification,
  baseline, guardrails, experiment limits and ledger rather than duplicating them:

## Checkpoint / handoff

- Accepted decisions (links; do not duplicate specifications):
- Changes made, last checks and evidence:
- Current failure or blocked acceptance condition:
- Outstanding review / human or external input:
- Pending question already raised, affected IDs and recommendation; do not repeat
  unchanged questions on every continuation:
- Remaining model groups with next-stage IDs/counts, recommended switch and why;
  separate tasks unlocked by a switch from other dependency/input blockers:
- Exact next step:
- Goal/campaign disposition if applicable; distinguish task completion from the
  overall objective, and preserve unmet targets and exact resume conditions:
- Final outcome and task-history link when complete:
