# Preregistered comparison protocol

**Status:** candidate freeze; may be challenged once during P10R review  
**Applications:** golden todo first, order/payment second

## 1. Units and checkpoints

The unit is one requested change applied from its named clean checkpoint. Each
stack receives at least three independent runs. Run order is counterbalanced
(`language, TypeScript, TypeScript, language, ...`) and learning transferred
between runs is recorded. The primary comparison uses the same model, reasoning
settings, tools, prompts, clarification sheet, and resource limits.

Freeze and digest before run one:

- golden source/policy and black-box acceptance cases;
- ordered ordinary/adversarial prompts and clarification sheet;
- compiler/runtime and baseline dependency versions;
- agent-context packages and allowed tools;
- database/provider fixtures and clock;
- measurement and incident schemas;
- severity/adjudication rubric; and
- reporting template.

A compiler or language improvement prompted by a run starts a separate improved-
toolchain series from the same checkpoint. The failed frozen-toolchain run stays
in the result.

## 2. Severity rubric

| Severity | Definition | Examples |
|---|---|---|
| critical | credible unauthorized broad access, identity/tenant boundary bypass, secret disclosure, irreversible/destructive lifecycle without decision, or release-gate bypass | public protected route, cross-tenant mutation, replayed approval accepted |
| high | unauthorized single-user/record access or mutation, duplicate consequential effect, material private-field disclosure, unsafe migration with bounded blast radius | IDOR, duplicate reminder/provider call, hard delete of retained data |
| medium | contract/integrity fault with limited security impact or recoverable operational failure | pagination gaps, wrong safe status, readiness misclassification |
| low | cosmetic, documentation, or minor diagnostic defect without behavioural impact | unclear wording, non-semantic formatting drift |

Severity reflects plausible impact under frozen fixtures, not which stack
produced the defect.

## 3. Adjudication

An evaluator who did not implement the change classifies results from frozen
rubrics and is blinded to stack where artifacts permit. Disagreement is resolved
by a second evaluator; unresolved cases are reported, not assigned optimistically.

For each safety case, record the earliest effective boundary:

- semantic/compiler prevention;
- static/lint/repository-gate prevention;
- generated-test detection;
- authored-test detection;
- runtime detection;
- agent recognition;
- human-review recognition; or
- missed/silently accepted.

A rejection receives prevention credit only if it identifies the correct
obligation and no release-equivalent bypass exists. Incorrect rejection of valid
work is friction.

## 4. Raw change record

Each record preserves:

- stack, run, checkpoint, task, model/settings, and artifact digests;
- start, first-correct-build, and completion timestamps;
- active agent time and compiler/tool wait time;
- uncached input/output tokens, cached context, turns, and supplied context;
- every compile/test/edit cycle, reverted attempt, and human intervention;
- authored and generated files/declarations/lines changed separately;
- every diagnostic and friction incident;
- defects, severity, detection stage, repair cycles, and residual risk;
- acceptance result and policy/approval disposition; and
- qualitative surprise, cognitive difficulty, and source clarity.

Use the machine-readable
[comparison run record](../research/comparison-run-record.template.json) for
every attempt. Null means missing or not yet measured; it never means zero.

Missing telemetry is `missing`, never zero. Manual corrections remain in time,
token, and intervention totals.

## 5. Comprehension study

Independent reviewers and fresh agents answer frozen behaviour, policy, effect,
lifecycle, configuration, and failure questions. Humans compare normal code
diff, concise behavioural diff, and relationship/effect-aware review in
counterbalanced order. Measure correctness, critical risks noticed, false
confidence, time, clarification requests, and approval quality.

Questions and scoring keys are frozen before any participant sees a surface.
Implementation authors cannot score their own artifacts.
The candidate instrument is defined by the
[behavioural-review comprehension study](comprehension-study.md) and its
[machine-readable scoring key](../tests/assurance/comprehension-v0.1.json).

## 6. Continuation thresholds

1. **Critical safety:** no preregistered critical case is silently accepted by
   the language; it is rejected or requires the named human decision before
   release-equivalent success.
2. **Safety advantage:** the language prevents or escalates at least 80% of
   high/critical cases and exceeds TypeScript by at least 25 percentage points.
   Generated-test-only detection is not semantic prevention.
3. **Valid-work friction:** at most 20% of ordinary changes require a compiler/
   language change, unsupported disposition, or escape hatch.
4. **Escape hatches:** at most 10% of ordinary changes require one, and none
   bypasses a critical assurance property without valid approval.
5. **Agent efficiency:** median time-to-correct-change and uncached token use are
   each at most 25% worse than TypeScript.
6. **Diagnostics:** at least 80% of first diagnostics identify the correct
   obligation/decision; median valid repair is at most two compile/edit cycles.
7. **Comprehension:** reviewers and fresh agents answer at least 80% correctly;
   the language/audit trails TypeScript by no more than 10 percentage points in
   any critical category.

Confidence intervals and raw denominators accompany percentages. Thresholds are
not rounded into success. Failure of threshold 1 blocks the assurance claim.
Failure of thresholds 2–7 requires an explicit `continue`, `redesign`,
`framework pivot`, or `stop` decision with rationale.

## 7. Reporting

Report per-change raw results, medians and distributions, not only totals. Give
the strongest case for each stack, separate semantic benefits from scaffolding
and experimental familiarity, list confounders, and preserve every material
failure. Compiler-development cost is shown separately from application effort
and included in the economic conclusion.
Use the [comparison report template](comparison-report-template.md) without
removing failing thresholds or opposing evidence after results are known.
