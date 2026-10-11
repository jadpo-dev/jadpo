# RM-213 — Keep project documentation useful and maintainable

Intake from the owner, 2026-10-01. First priority for the next unscoped roadmap
run. Planning and audit/cleanup completed on 2026-10-01.

| Task ID | Planning depth and reason | Dependency evidence / blocker | Planning state | Execution state |
|---|---|---|---|---|
| RM-213 | Short audit and focused documentation cleanup | Acceptance evidence recorded below and in history | planned | done |

**Outcome:** retain what future work needs—current contracts, decisions and their
essential rationale, task context and evidence—without repeated prose or logs
obscuring it. Estimate after the initial inventory; do not assume it needs a rewrite.

**Completion conditions:**

- Assess structure, verbosity, relevance, duplication, stale/conflicting guidance
  and the cost of finding context for representative upcoming tasks. Include
  workflow instructions, plans, generated reports and archives, not just specs.
- If warranted, consolidate, shorten, reorganise or archive material while
  preserving accepted semantics, unresolved decisions, provenance and necessary
  evidence. If already adequate, record that finding briefly instead of churning docs.
- Define a small set of practical authoring/retention rules: where each fact belongs,
  when to update an existing document, when a new one is justified, what stays in
  chat or raw evidence, and when plans/logs should be archived. Integrate them into
  existing agent guidance to prevent unnecessary LLM output; avoid another large manual.
- Check before/after that upcoming tasks can find the authoritative contract,
  relevant decisions, current state and next step without trawling historical logs.
  Check affected links and retain essential context; shorter word counts alone
  are not evidence of improvement.
- Decide whether these controls are sufficient. If repeated maintenance remains
  difficult, propose a minimal reusable cleanup skill with a clear trigger and
  preservation rules, rather than assuming a new skill is required.

This concerns internal project documentation. It does not replace RM-701's
assurance reconciliation or E10's conditional public-site research, and does not
silently expand a fixed task set in an already-running Goal.

## Execution plan

1. Inventory active guidance separately from history, generated snapshots and raw
   evidence. Classify each document's purpose/authority and identify concrete
   duplicate or stale passages; do not use age or length alone as a deletion rule.
2. Walk three representative tasks: protected todo generation (RM-102), service
   contract preparation (RM-301), and policy attestation (RM-603). Record the
   minimum reading path to accepted semantics, current evidence, blockers and
   next steps; note conflicting sources and unnecessary historical detours.
3. Make the smallest justified changes in coherent documentation-only slices:
   canonical ownership/index first, then consolidation and archival. Preserve
   frozen contracts, decision rationale, raw results and old link targets or
   redirects. Leave disputed semantic content for an explicit owner decision.
4. Put a short write/update/archive rule in existing agent guidance and explain
   document ownership in the existing index. Avoid parallel manuals and automatic
   per-turn prose logs. Propose a cleanup skill only if recurring work remains.
5. Repeat the three context lookups, check affected Markdown links/anchors and
   `git diff --check`, and verify preserved requirements against the source diff.
   Report useful before/after evidence and limitations in the task's history entry.

Planning evidence: 74 Markdown files under docs, approximately 182,192 words at
inspection; 31,459 words are in implementation-history.md. These are inventory
observations, not proof of excess or a target reduction. The task remains
unestimated: measure the audit slice before forecasting the cleanup scope.

Source basis: working tree at HEAD `1da4f900b0e489796acf2c72651c5681468ccf95`, with
substantial concurrent/uncommitted work. Re-read affected documents before edits.
Recommended: Luna High for inventory/edits; Sol High if authority conflicts need
complex synthesis. Actual model/effort unknown; selected settings unchanged.
Planning run: `RM-213-eca999414c8d`. No owner input is needed to begin this plan.

## Audit outcome — 2026-10-01

Inspected 76 Markdown files / 188,883 words, separating specs, active work records,
generated evidence and history. Concrete problems: the workflow repeated skill
procedures and Goal prompts already present in the cheat sheet; the index's
30-item reading sequence lacked task-specific paths; mutable phase summaries
duplicated roadmap state. Detailed specifications and the 31,459-word history
were not shown to be disposable, so their contracts/evidence were preserved.

The workflow fell from 1,692 to 719 words. The index now sends RM-102 directly to
AUTH-001, migration evidence and its delivery plan; RM-301 to D3, the owner/provider
decision and remaining contract work; RM-603 to the approval protocol, AP fixtures
and local-versus-hosted boundary. Each lookup now has one named row linking its
authority, evidence and next action; none requires searching legacy phase history.
These are inspected navigation paths, not measured first-user reading times.

Ownership/retention rules now live in the index and are linked from AGENTS.md.
The generic reusable procedure stays in personal skills. A cleanup skill is not
justified yet; use these rules and revisit only if repeated drift demonstrates a
remaining need. Ordinary checks suffice: local links/anchors, preserved headings,
unchanged hashes for eight authority/frozen files, unchanged historical snapshot,
and clean `git diff --check`. Implementation run: `RM-213-5dd4a47ebfda`; original
estimate stayed unknown rather than backfilling it after observing the work.


## Follow-up scope reconciliation — 2026-10-01

The owner reported missing roadmap context after reorganisation. The follow-up
[conversation/source audit](../conversation-coverage.md#4-roadmap-reconciliation--2026-10-01)
restores explicit topic/issue coverage and preserves the source snapshot. This is
additional documentation work, not a new completion of RM-213 or implementation
of the recovered tasks. Timing: `META-ROADMAP-RECONCILIATION-c50aee9fe93b`.

The rows below began as intake records. Their current planning states were
refreshed by the [2026-10-03 planning sweep](roadmap-assessment.md#planning-only-sweep--2026-10-03);
the linked assessment owns their next-stage plans. Original estimates and
execution states remain with the roadmap. E11's recovered ideas have their
planning records in [its existing intake](developer-console-mcp.md).

| Task ID | Planning state | Planning need |
|---|---|---|
| RM-214 | planned | Bounded decision/probe plan in [roadmap assessment](roadmap-assessment.md#e02-language-tooling-and-http-boundaries); semantic approval remains. |
| RM-215 | planned | Assessed; bounded next-stage procedure in [roadmap assessment](roadmap-assessment.md). Implementation and required review remain. |
| RM-216 | planned | Conditional gap-by-gap decision plan in [roadmap assessment](roadmap-assessment.md#e02-language-tooling-and-http-boundaries). |
| RM-217 | planned | Conditional cancellation/streaming decision plan in [roadmap assessment](roadmap-assessment.md#e02-language-tooling-and-http-boundaries). |
| RM-218 | planned | Enforcement inventory plan in [roadmap assessment](roadmap-assessment.md#e02-language-tooling-and-http-boundaries). |
| RM-219 | planned | Assessed; bounded next-stage procedure in [roadmap assessment](roadmap-assessment.md). Implementation and required review remain. |
| RM-220 | planned | Conditional one-extension-at-a-time plan in [roadmap assessment](roadmap-assessment.md#e02-language-tooling-and-http-boundaries). |
| RM-408 | planned | Assessed; bounded next-stage procedure in [roadmap assessment](roadmap-assessment.md). Implementation and required review remain. |
| RM-409 | planned | Assessed; bounded next-stage procedure in [roadmap assessment](roadmap-assessment.md). Implementation and required review remain. |
| RM-410 | planned | Assessed; bounded next-stage procedure in [roadmap assessment](roadmap-assessment.md). Implementation and required review remain. |
| RM-508 | planned | Assessed; bounded next-stage procedure in [roadmap assessment](roadmap-assessment.md). Implementation and required review remain. |
| RM-1005 | planned | Conditional domain/launch evidence plan in [roadmap assessment](roadmap-assessment.md#e10-conditional-public-documentation-site). |
