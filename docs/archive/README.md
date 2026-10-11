# Archive

Frozen material from the Codex delivery period (to 2026-10-11). Nothing here is
maintained, and links inside these files may be broken. Current work is in the
[roadmap](../roadmap.md).

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
