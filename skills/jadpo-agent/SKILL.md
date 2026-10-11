---
name: jadpo-agent
description: Inspect, repair, and explain Jadpo projects using the compiler-owned grammar, diagnostics, semantic graph, and generated contracts. Use for Jadpo source edits, diagnostic repair, symbol or impact questions, and local runtime-incident investigation.
---

# Jadpo agent interface

Treat the compiler as the semantic authority. Do not infer new Jadpo syntax,
failure handling, authentication, policy, persistence, or approval behavior.

For source work:

1. Read the relevant accepted contract in `docs/decision-register.md`, then
   `docs/roadmap.md`, `docs/developer-tooling.md`,
   `docs/syntax.md`, and `docs/semantic-model.md`. Use
   `docs/grammar-v0.1.md` for the currently executable subset.
2. Run `jadpo check <project> --diagnostic-format=json`. Consume the version-2
   diagnostic object; use the dotted `ruleId`, reason, decision owner, impact,
   and bounded alternatives rather than parsing terminal prose.
3. Apply an `automatic_fix` only from its exact revision-bound text edits.
   Re-run the compiler if `sourceRevision` changed. For a `guided_choice`, show
   the bounded alternatives and their contract effects. Stop for a
   `human_decision` instead of choosing across the protected boundary.
4. Query symbols and dependencies with `jadpo inspect <project>`. Generate
   route, callable, failure, OpenAPI, validation, compatibility, and diagnostic
   views with `jadpo artifacts <project>`; read only the artifact relevant to
   the question.
5. For a safe local runtime event, run
   `jadpo incident <project> <event-json-file>`. It requires an exact matching
   source revision and compiler manifest and deliberately ignores event
   payloads that are not compiler-approved identifiers.

Use the LSP for unsaved-source diagnostics, hover guidance, related locations,
and code-action previews. Do not recreate diagnostic definitions or a second
parser in an editor or agent integration.
