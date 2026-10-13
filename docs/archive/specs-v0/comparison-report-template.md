# Comparison report template

**Protocol version:** `p10r-comparison-v0.1`  
**Status:** frozen only when the P10R digest set is approved

## 1. Decision

Select exactly one: `continue`, `redesign`, `framework pivot`, or `stop`.

State which continuation thresholds passed, failed, or were inconclusive. A
critical-safety failure blocks the current assurance claim regardless of other
scores.

## 2. Reproducibility

- contract and instrument digest set:
- language/compiler checkpoint and version:
- TypeScript baseline checkpoint and dependency lock digest:
- primary model/version/reasoning settings:
- run-order assignment:
- database/provider fixture digests:
- evaluators and blinding limits:
- deviations from protocol:
- missing telemetry:

## 3. Raw run inventory

List every run record, including failed, superseded, frozen-toolchain, and
improved-toolchain runs. Do not remove warm-up or pathological results; label
their role and include/exclude decision using the preregistered rule.

## 4. Threshold table

| Threshold | Language result | TypeScript result | Required | Outcome |
|---|---:|---:|---:|---|
| Critical safety silently accepted | | | 0 language cases | |
| High/critical prevented or escalated | | | language ≥80% | |
| Safety advantage | | | language ≥TS +25 percentage points | |
| Ordinary changes with major friction | | | ≤20% | |
| Ordinary changes using escape hatch | | | ≤10% | |
| Critical property bypass via escape | | | 0 | |
| Median time-to-correct-change ratio | | | language ≤1.25× TS | |
| Median uncached-token ratio | | | language ≤1.25× TS | |
| Correct first diagnostics | | | ≥80% | |
| Median valid repair cycles | | | ≤2 | |
| Overall comprehension | | | ≥80% | |
| Critical-category comprehension gap | | | language no worse than TS −10pp | |

Include raw numerator/denominator and uncertainty intervals beside every
percentage. Do not round a failing raw value into a pass.

## 5. Per-change results

For each task report both stacks and all runs:

- final disposition and acceptance result;
- defects, severity, and earliest detection boundary;
- active time, wait time, uncached/cached tokens, turns, and repair cycles;
- authored versus generated changes;
- compiler/linter/test/runtime/human interventions;
- friction incidents, workarounds, escape hatches, and language changes;
- policy decisions and approval behavior; and
- residual risk and qualitative difficulty.

## 6. Safety analysis

Separate:

- semantic/compiler prevention;
- static/lint/repository-gate prevention;
- generated-test detection;
- authored-test detection;
- runtime detection;
- agent recognition;
- human recognition; and
- missed or silently accepted cases.

Generated-test detection is not semantic prevention. Incorrect rejection of
valid work is friction, not a safety win.

## 7. Comprehension and approval

Report correctness, critical facts missed, false confidence, time,
clarification requests, and approval/rejection quality for each surface and
stack. List every critical misunderstanding even if the aggregate passes.

## 8. Economic and usability result

State which stack was easier to author, faster, less token/context intensive,
less defect-prone, easier to debug, and easier for humans and fresh agents to
understand. Include compiler/language development effort separately and in the
economic conclusion.

## 9. Attribution and confounders

Attribute observed benefits separately to language semantics, generated
scaffolding, framework conventions, diagnostics, agent context, familiarity,
run order, or evaluator artifacts. Record learning transfer, tool outages,
missing telemetry, and any inability to blind the stack.

## 10. Strongest opposing cases

- strongest case for the new language:
- strongest case for the TypeScript framework:
- evidence that falsified an initial belief:
- unresolved mixed result:
- use case the project should reject:

## 11. Decision rationale

Explain why the evidence supports the selected decision without changing the
preregistered thresholds. For `continue` or `redesign`, name the narrow next
hypothesis and work permitted. For `framework pivot` or `stop`, preserve the
reusable compiler/research evidence without reframing the result as success.

