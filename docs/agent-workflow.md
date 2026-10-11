# Agent development workflow

**Status:** workflow hypothesis

This document describes Jadpo's intended product workflow. For developing this
repository now, use the [adaptive roadmap workflow](archive/roadmap-workflow.md), routed
through [AGENTS.md](../AGENTS.md).

## 1. Intended collaboration

The environment is built around three collaborators with different authority:

| Participant | Primary responsibility | Must not silently decide |
| --- | --- | --- |
| Human | intent, policy, risk, exceptions | implementation detail that the compiler/agent can resolve |
| LLM agent | implementation inside permitted semantics | weakening policy or inventing missing human intent |
| Compiler/runtime | deterministic checks, derivation, execution | ambiguous product or business decisions |

The division is the product. Merely giving an LLM shorter syntax is not enough.

## 2. Requirement-to-build loop

An illustrative change request is:

> Add an endpoint for customers to cancel bookings.

The desired loop is:

1. The agent locates the relevant entities, routes, policy, intent, and tests.
2. It proposes or edits the action and route.
3. The compiler recognises a destructive mutation and checks authentication,
   cancellation policy, payment lifecycle, related records, and side effects.
4. Deterministic implementation errors return directly to the agent.
5. Missing human decisions are explicitly classified as such.
6. The human approves the role, cancellation window, payment behaviour, or
   policy exception.
7. The agent completes the implementation without changing approved policy.
8. The compiler produces validators, migration changes, docs, generated tests,
   route inventory, and audit changes.
9. CI blocks any policy/implementation mismatch.
10. The human reviews a behavioural/policy diff and exceptions rather than a
    large generated-code diff.

## 3. Repository as living context

All essential project context should live in provider-independent declarations
and documentation in the repository:

- application semantics;
- human intent and decisions;
- policy;
- external contracts;
- generated route/data/effect graph;
- tests and audits;
- design issue log.

An agent should not require a model-specific instruction hierarchy to remember
fundamental security rules. When an instruction says “the LLM must always…”, the
design should ask whether that rule can become a compiler invariant.

This is related to the earlier “Project OS” idea: source and derived metadata can
act as a machine-readable specification that remains aligned with execution.

## 4. Structured compiler tasks

Compiler output should be actionable and bounded. Each diagnostic should
include:

- a complete plain-language summary and short reason before structured detail;
- a readable lower-case dotted rule identifier and category;
- source and semantic node;
- the violated invariant or unresolved decision;
- affected routes/entities/effects;
- one recommended next step plus allowed alternatives when the choice set is
  closed;
- whether the step is an automatic verified edit, an agent-owned semantic
  choice, or a human-owned decision;
- the source revision and behavioural/public-contract effect of any proposed
  edit; and
- relevant declarations, not an unfiltered stack or dependency trace.

This converts compilation into part of the agent's reasoning loop rather than a
late pass/fail tool. The agent receives rich compiler and locally enriched
incident packets, while browsers and third-party telemetry receive separate,
smaller audience-safe schemas. Stable semantic operation IDs join those views
without sending customer values or secrets through an external logging system.

## 5. Change impact

Before editing, an agent should be able to ask the semantic model:

- which routes expose this field or entity;
- which policies cover it;
- which jobs/events/services depend on it;
- what migration and existing-data decisions a change implies;
- which tests are generated and which business tests may need revision;
- which human decisions are protected.

After editing, the same graph produces a concise impact summary.

## 6. Formatting checkpoints

Agents may make rough intermediate edits. The formatter should not constantly
reposition source while an agent is working. The iteration is:

```text
draft edit
  -> checkpoint format
  -> compile and policy check
  -> structured diagnostics
  -> next edit against canonical source
```

## 7. Human approval boundary

The exact product mechanism is open, but the rules are not:

- policy may be authored with agent assistance;
- a policy weakening is surfaced distinctly;
- the agent cannot approve its own weakening;
- CI can prove that the approved policy version matches the implementation;
- audit remains derived and cannot substitute for approval.

## 8. Multi-model portability

The language and repository model must not assume a particular coding agent.
Claude, Codex, or another LLM should receive equivalent context and diagnostics.
Provider-specific integrations may improve ergonomics, but correctness cannot
depend on them.

## 9. Review ergonomics

The normal human review should emphasise:

- requested intent;
- policy changes;
- new public surfaces;
- data read/write scope;
- lifecycle/migration decisions;
- external services and secrets;
- new escape hatches;
- generated audit and test changes;
- unresolved compiler warnings.

Generated target code is excluded from ordinary review.

## 10. Open workflow questions

- How is a human decision cryptographically or procedurally distinguished from
  an agent edit?
- Should policy live in protected files, a signed artifact, a review database,
  or normal source with branch protection?
- How does an agent query semantic metadata before code exists?
- Which derived artifacts are committed versus generated in CI?
- How are changes to generated external contracts reviewed?
- What is the minimal IDE/CLI experience for structured compiler tasks?
- How are multiple agents coordinated without weakening policy ownership?

## 11. Measure roadmap work

For implementation tasks, follow the task-timing workflow
and repository `AGENTS.md`. Record the task ID, original estimate, UTC start/end,
work/verification/review phases, pauses and blockers, actor, and completion
or partial-work evidence using `python3 tools/task-time.py`. Store each run in
`docs/task-timing/runs/`; keep later attempts under the same task ID.

Compare comparable completed tasks using total active time including rework.
Human effort, external wait and overlapping agent time remain separate. A
completed chat turn is historical session evidence, not automatically the full
duration of a roadmap task. Never reconstruct missing timestamps from commit
gaps or count an interrupted timer's unattended tail as active work.
