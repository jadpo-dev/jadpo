# Research brief

**Status:** candidate P10R brief  
**Last reviewed:** not yet independently reviewed

## Problem

Coding agents can generate backend code faster than humans can economically
review it. Important behaviour is distributed across handlers, schemas, queries,
policy, configuration, jobs, provider integrations, framework defaults, and
generated artifacts. A plausible code diff does not reliably tell an accountable
human what the system can now do.

## Product hypothesis

A constrained backend language with a compiler-owned semantic graph can make
agent-written services safer to change and easier to understand by making
authentication, policy, data access, boundaries, lifecycle, and external effects
explicit, deterministically checked, and rendered as a focused behavioural
review.

The generated target is an implementation detail. The value claim is prevention
or conspicuous escalation of unsafe changes with a smaller, more accurate human
decision surface.

## Counter-hypothesis

An excellent TypeScript framework with schemas, a constrained data layer,
policy-as-code, lint/static rules, code generation, strong agent context, and a
behavioural review tool can deliver essentially the same safety and comprehension
while retaining ecosystem familiarity. If so, building a new language is not
justified.

## Initial research user

An experienced product-engineering team using coding agents for substantial
greenfield or newly isolated SaaS-backend work, where a human remains accountable
for security and policy but cannot review all generated implementation code.

- **Daily user:** product engineer directing and debugging coding agents.
- **Accountable human:** senior engineer or security-aware technical lead who
  owns policy decisions and release approval.
- **Adoption trigger:** generated change volume has outgrown reliable code review,
  particularly around authorization, data lifecycle, and provider effects.
- **Current alternative:** TypeScript plus an opinionated framework, schemas,
  ORM, policy library, linting, CI, generated docs, and conventional review.
- **Smallest credible path:** one new or isolated HTTP/database service with
  generated boundaries; no required rewrite and no broad package ecosystem.
- **Required interoperability:** HTTP/OpenAPI, PostgreSQL, OIDC/session identity,
  deployable Bun output, logs/telemetry, and declared external services.
- **Trust objections to test:** new compiler/runtime correctness, escape hatches,
  debugging generated output, operational ownership, approval fatigue, limited
  libraries, and whether behavioural review hides material implementation facts.
- **Acceptable switching cost hypothesis:** one working day to become productive
  on the golden todo is acceptable only if safety/comprehension gains are clear.

Five structured reviews with people matching this profile remain required before
P10R exits. Their raw notes, recruitment fit, and disconfirming evidence must be
recorded; interviews are product evidence, not proof of safety.
Use the [first-user structured review guide](first-user-review-guide.md) and
[machine-readable record template](../research/first-user-review-record.template.json)
without silently changing the instrument between participants.

## Test programme

1. Freeze the compiler-independent golden todo contract and adversarial sequence.
2. Freeze named proof rules, threat boundaries, approval protocol, measurement,
   and a strong TypeScript baseline.
3. Implement the language and baseline without changing the contract to fit.
4. Run at least three counterbalanced independent trials per stack using the same
   primary model/settings and clean checkpoints.
5. Compare prevention, escalation, friction, efficiency, diagnostics, and human/
   fresh-agent comprehension.
6. Apply the preregistered thresholds and record `continue`, `redesign`,
   `framework pivot`, or `stop`.

## Continuation rules

Critical authentication, tenant, authorization, secret-disclosure, and
destructive-lifecycle cases must never receive silent release-equivalent success.
The remaining quantitative thresholds and adjudication live in
[comparison-protocol.md](comparison-protocol.md). Threshold failure is not
smoothed into a narrative win; mixed results remain mixed.

## Current evidence

- Exploratory: semantic compiler, deterministic artifacts, Bun target, SQLite
  and PostgreSQL persistence slice.
- Candidate design evidence: golden todo source, policy, black-box cases,
  adversarial sequence, expected audit, proof kernel, threat model, and approval
  protocol.
- Missing validation evidence: independent candidate review, five research-user
  reviews, implemented P11 contract, TypeScript baseline runs, repeated trial
  results, comprehension study, and continuation decision.

Compiler progress is not product validation. No current evidence establishes
that the new language outperforms the TypeScript counter-hypothesis.

## Review deferral

On 2026-09-25 the project owner directed implementation to continue while the
outside reviews are skipped for now, then clarified that the five first-user
sessions should occur only after feature-complete implementation because the
technical work has so far been inexpensive. The candidate instruments and gate
remain intact. Technical work may continue through P10.5, P11, and the P12
reference implementations, but it is exploratory evidence only and must not be
reported as independent review, product validation, Milestone C assurance, or a
comparative result. Candidate protocols and fixtures retain dated digests;
review-driven revisions must preserve the original version and record which
version influenced implementation. The deferred sessions and external freeze
must occur before final P12 trials or a continuation decision.
