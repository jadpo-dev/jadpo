# Roadmap test-gap inventory — RM-501

**Reviewed:** 2026-10-01  
**Scope:** accepted or provisional Jadpo contracts and the evidence currently
checked into this repository. This is a static gap review, not a fresh verifier
run. It distinguishes missing tests from unresolved semantics and external
evidence; it does not turn either into an assumed expected result.

## Selected evidence gaps

| Area | Evidence already present | Remaining obligation and next action |
|---|---|---|
| Lexer, parser, modules | Public lexer/parser boundary cases, byte-span checks, bounded malformed-input mutations, and explicit module/import integration tests. See [syntax findings](syntax-findings.md). | Complete NAME-002's grammar-derived identifier matrix after the reserved-word decision: variable and type names in every declaration/reference position, all keyword spellings in both name classes, case sensitivity, ASCII allowed-character/leading-digit rules, underscore and Unicode boundaries, and stable CLI/LSP spans. An accepted contextual keyword must also work at its reference site. Invalid byte-stream handling and resource limits need their own contracts before asserting behaviour. **RM-201, RM-502.** |
| Types and values | Independent schema/runtime agreement cases cover constraints, nullability, identity wire types, failure envelopes and deterministic artifacts. See [artifact findings](artifact-contract-findings.md). | Set/Map HTTP encodings, key representation, duplicate handling and null rules remain unsettled; current generated schemas advertise shapes the handlers reject. Decide the wire contract before adding parity assertions. Persistent enum representation and migration rules also remain provisional. **RM-208; TYPE-006.** |
| Failures and callables | Direct failure propagation, declared failure sets, outcomes, and generated public envelopes have focused checks. | Local `attempt` mapping/cause wrapping is deferred; compatible public failure-code alias syntax and the catalogue's HTTP 422 pressure remain provisional. Select contracts first, then add exact failure-set and cause/disclosure cases. **FAIL-001–003; RM-207.** |
| Persistence and transactions | SQLite rollback/savepoint cases and bounded live PostgreSQL concurrency evidence are recorded in the validation ledger. | A cross-process SQLite probe returned `SQLITE_BUSY` at transaction start. TX-001 leaves retry/failure defaults open; do not assert retry counts or surfaced outcomes until selected. After that decision, add multi-process contention, deadlock, nested rollback and failure-injection cases. **RM-401 → RM-402; RM-504.** |
| Authentication and policy | First-party browser/API, service, JWT, policy, revocation and SQLite/PostgreSQL suites cover their implemented subsets; the approval protocol specifies a separate protected-review gate. | The migrated golden protected routes still fail closed at `toolchain.target_auth_not_implemented`, so no generated golden route behaviour is established. Complete target support before running its route/policy cases; protected attestation still requires X-CI. **RM-102, RM-109, RM-604; X-CI.** |
| Configuration, time and fixtures | Typed configuration, secret-taint checks, deterministic clocks, isolated fixtures and temporal runtime cases have local evidence. | AUTH/SERVICE must supply the remaining compiler-owned secret sinks; dependency-probe semantics belong to SERVICE-001. Timezone/locale engine provenance and equivalent live PostgreSQL time evidence remain external or adapter-specific. Exercise these only with the selected providers and pinned runtime data. **RM-303, RM-403, RM-504.** |
| Tooling and artifacts | Formatter exact-output coverage spans compile-pass fixtures and added grammar alternatives; Prism and Shiki have real engine smoke evidence; diagnostic/LSP contracts have focused tests. | Highlight.js/Monaco host rendering and plain-text fallback lack actual-host evidence. Formatter output after removing nonblank line breaks is blocked by the FMT-005 contract conflict. DX2 comprehension/repair trials need independent users. **RM-210; RM-202 → RM-203; X-REVIEW.** |
| Cross-feature integration | The obligation map retains all 44 golden case IDs and the current migration source checks; policy/auth/runtime evidence covers several isolated paths. | No golden acceptance case has executed. Lifecycle races, service/job replay, auth revocation with policy and transaction failures, and secret canaries require their upstream lifecycle, auth, service/job and transaction contracts. **RM-101–110, RM-205–206, RM-304–307, RM-402, RM-505.** |
| Sustained fuzzing and mutation | RM-502 now runs 2,048 fixed-seed corpus mutations through parser, semantic graph, type, and failure analysis with deterministic-output/span checks and bounded minimisation. RM-503 has killed five targeted mutants across semantic naming, keyword-token boundaries, and generated Unicode length; see [mutation findings](rm503-mutation-findings.md). | Continue targeted mutations from additional named obligation areas and record survivors. Coverage-guided/deep-resource exploration and a broad implementation mutation score are not claimed. **RM-503.** |

## Execution order

1. Proceed with repository-local cases whose expected results are already
   specified, including RM-201's lexical, case and naming-shape matrix; keep
   keyword acceptance assertions gated on the owner's reserved-word decision.
   RM-502's fixed-seed campaign is now integrated in the semantic crate's
   Cargo tests.
2. Keep decision-owned areas testable at the observation level only; do not
   encode a retry, Set/Map, failure-alias, lifecycle, or formatter-line-break
   policy before its owner resolves the contract.
3. Run RM-502 and RM-503 against the accepted contracts and retain reproducible
   evidence. Start RM-504 and RM-505 only when their database/provider and
   upstream contract dependencies are available.
4. Register resulting suites through the unified verifier under RM-506. A
   related unit test does not satisfy an integrated golden obligation.

The source inventory for this review is [the contract/test entry point](README.md),
the [active language issues](../../docs/language-issues.md), the [implementation
roadmap](../../docs/implementation-roadmap.md), the [golden obligation map](golden-obligations.json),
and the linked area findings above. Open gates remain open after this review.
