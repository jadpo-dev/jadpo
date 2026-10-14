# Archive

Frozen material. Nothing here is maintained, and links inside these files may
be broken. Current work is in the [roadmap](../roadmap.md); the language is
specified in [docs/language](../language/README.md), which wins wherever an
archived document disagrees.

## The v0 compiler (archived 2026-10-13)

The first compiler, generating TypeScript for Bun, was replaced by the
graph-centred rebuild. Its source, tests, examples, experiments and editor
extension are at the git tag `archive/compiler-v0`
(`git show archive/compiler-v0:<path>`). The language pages' compiled
examples show its frozen results (`docs/language/reference/v0-results.json`).

| Item | What it is |
|---|---|
| [specs-v0/](specs-v0/) | The v0 specifications (grammar, syntax, type system, failure model, persistence, policy, events, time, testing and the delivery plans). Each is replaced by a language area: grammar and syntax by areas 1–3; types by 2; failures by 4; persistence, entities and lifecycle by 5 and 6; policy by 9; authentication by 8; configuration and time by 10; services by 11; events and async by 12; testing by 13; tooling and diagnostics by 14 and 17; approval by 18 |
| [compiler-runtime-v0.md](compiler-runtime-v0.md) | The v0 architecture document in full |
| [decision-register-2026-10-13.md](decision-register-2026-10-13.md) | The register before the language review, with every earlier decision |
| [golden-todo-language-friction-v0.md](golden-todo-language-friction-v0.md) | Where the golden todo pressed on the v0 compiler when its contract was frozen |

## The Codex delivery period (to 2026-10-11)

| Item | What it is |
|---|---|
| [implementation-roadmap-2026-10-11.md](implementation-roadmap-2026-10-11.md) | The full task roadmap (RM-xxx IDs, epics E01–E12) at takeover. Parked scope is described here. |
| [implementation-history.md](implementation-history.md) | Completed tasks and earlier phase descriptions. |
| [work-plans/](work-plans/) | Codex planning loops, including the golden delivery plan. |
| [decision-sprint.md](decision-sprint.md) | Owner decision packages D1–D6 (outcomes are in the decision register). |
| [validation-findings/](validation-findings/) | Findings from mutation, fuzz and conformance campaigns. |
| roadmap-workflow, workflow-cheat-sheet, conversation-coverage | Codex process documents. |

Per-case evidence packets, review records and timing logs were deleted from
the repository and its history. A full pre-clean-up backup is in
`~/Archives/jadpo-pre-cleanup-2026-10-11.bundle` on Richard's machine.

## Unmerged work kept as tags

The 6–7 October work lineage was never merged and conflicts with `main` in 21
core files (both lines built the delivery/scheduler subsystem independently).
It is kept as tags so the language review can draw on it:

| Tag | Contents not on `main` |
|---|---|
| `archive/golden-loop-next` | Integration tip: 44-ID golden checkpoint, operational/HTTP boundary failure metadata, protected approval attempts, point Instant storage, startup ownership, admission witnesses, transaction retry audit. |
| `archive/rm109-closure-audit` | Extends golden-loop-next: many-Instant storage, process-owner IPC, golden closure audit. |
| `archive/rm207-receipt-reconciliation` | Delivery receipt reconciliation and acceptance proofs. |
| `archive/page-instant-lossless` | Page Instant decoding from lossless carriers. |
| `archive/instant-point-product` | Instant precision probes and plans. |
| `archive/rm504-pg-service-restart` | PostgreSQL service-restart probe and result-owner prototype. |
| `archive/list002-detached`, `archive/deployment-metadata-detached` | Two commits that were on no branch. |
