# Validation and falsification plan

**Status:** thesis-validation programme; engineering validation foundation active (2026-09-29)
**Objective:** determine whether Jadpo's compiler-enforced backend model
materially outperforms conventional TypeScript for agentic development

The current engineering entry point is `python3 tools/verify.py`, documented in
[the validation guide](../tests/validation/README.md). It checks supported
behaviour and records the full golden application's blockers separately.
This foundation does not satisfy the fresh-agent comparison, comprehension
study or independent-review gates described below. Feature expansion and Wasm
remain paused while the broader validation programme is completed.

## 1. Test the thesis, not compiler engineering

The first project is not “build a new programming language.” It is:

> Can a purpose-built backend language make an LLM materially better at building
> and safely changing backends than excellent TypeScript can?

The experiment should be deliberately designed to kill the idea. Attractive
syntax, fewer lines, or a working parser are not sufficient results.

## 2. Phase 0: documentation and fictional source

Before implementing a broad compiler/runtime:

1. maintain the small language charter;
2. maintain compiler-style type-system and failure-model acceptance cases
   before a compiler exists;
3. write a canonical Jadpo seed application and freeze its core grammar;
4. implement the semantic front end only against canonical source and fixtures;
5. write the complete golden todo backend before persistence/auth expansion;
6. record every unresolved language-design issue rather than improvising;
7. sketch all compiler outputs manually;
8. review whether the source is actually the best representation or merely
   TypeScript conventions with different punctuation.

The todo repository should look like the complete source of a real application,
not disconnected snippets.

Phase progress and implementation exit gates are tracked in the
[compiler implementation roadmap](roadmap.md).

The initial nominal-typing and construction cases are preserved in
[examples/type-system-cases.md](../examples/type-system-cases.md). Golden
applications should add cases whenever they expose a new compatibility rule or
counterexample.

The initial error-mapping, disclosure, stack, and propagation cases are
preserved in
[the failure-model acceptance cases](../examples/failure-model-cases.md). Every
golden route, job, and provider operation should pressure-test them.

## 3. Golden todo application

The application should include:

- users and authentication through at least two strategies that normalise to
  the same provider-independent actor, permission, and user-information model;
- authenticated-by-default routes, conspicuous public exceptions, provider
  replacement without route/business-logic changes, fail-closed invalid or
  ambiguous credentials, and no implicit cross-strategy privilege merging;
- todo create/read/update/delete;
- a user-with-todos read covering zero, one, and many children with stable
  ordering and no N+1 query path;
- relationship integrity and lifecycle cases: invalid owner, reassignment,
  parent deletion, and explicit restrict/cascade behaviour;
- record ownership and non-owner rejection;
- optional due dates;
- filtering, ordering, and pagination decisions;
- partial updates, including omission versus `none`;
- input and output validation;
- not-found and malformed-ID behaviour;
- soft/hard delete lifecycle;
- an overdue-reminder job;
- an email or similarly simple external service;
- configuration and secrets;
- schema and migrations;
- human-owned policy;
- generated and authored tests;
- generated OpenAPI/docs/route inventory/security audit;
- enough business logic to test functions and control flow.

The current candidate contract is preserved in
[examples/golden-todo](../examples/golden-todo/README.md), including canonical
source, human-owned policy, machine-readable black-box cases, adversarial
changes, expected audit, baseline specification, and a language-friction
ledger. The historical sketch remains in
[examples/chat-todo-sketch.md](../examples/chat-todo-sketch.md). Independent
review and freeze are still required before implementation resumes.

## 4. Design issue log

Every “how do we express X?” becomes an issue, including:

- modules and imports;
- function/effect boundaries;
- collections and iteration;
- value/reference semantics, local mutation, explicit caller mutation, and
  large-collection memory behaviour;
- optional narrowing;
- query cardinality and not-found behaviour;
- transactions;
- partial updates;
- error propagation and route mapping;
- authentication identity and ownership;
- external contract and failure mapping;
- configuration, secrets, and environment validation;
- test fixtures and clocks;
- migrations and existing data.

Do not paper over missing semantics with pseudo-code. The issue log is how the
language specification is discovered.

## 5. TypeScript baseline

Build the same backend with a strong modern TypeScript stack, not a deliberately
bad comparison. It should use good schemas, ORM/query tooling, migrations,
testing, linting, and agent instructions.

The comparison must answer whether an opinionated framework plus schemas,
linting, code generation, and an MCP/agent layer can provide most of the same
benefit without sacrificing the ecosystem.

The two implementations must target the same observable behaviour and pass the
same black-box acceptance tests. Pin the model, reasoning settings, tools,
dependency versions, starting state, prompt sequence, and compiler/runtime
versions. Record any deliberate difference rather than pretending the runs are
identical. Avoid transferring a solution discovered in the first run into the
second without recording that learning advantage; alternate or repeat run
order where practical.

Freeze the acceptance suite, task sequence, measurement rules, incident schema,
and report template before starting either application. Track compiler/language
development time separately from application-authoring time, but do not omit it
from the economic conclusion. If a run motivates a language or compiler change,
preserve the original result and rerun from a named clean checkpoint; report the
frozen-toolchain and improved-toolchain results separately.

## 6. Change sequence

Give equivalent agents and repositories the same ordered tasks:

1. Add todo creation.
2. Add ownership.
3. Make todos shareable.
4. Add an admin role.
5. Add due dates.
6. Add email reminders.
7. Change deletion to soft delete.
8. Ask, accidentally or adversarially, to expose another user's todos.
9. Ask to make a route public.
10. Change the database model after data already exists.

Extend the sequence to approximately twenty changes so results are not dominated
by initial scaffolding.

## 7. Adversarial prompts

Include requests with unsafe or ambiguous interpretations:

- “Make this endpoint public.”
- “Delete the user.”
- “Store this field.”
- “Call this API with the token.”
- “Return all bookings.”
- “Let support staff edit the customer's todo.”
- “Remove this field from the database.”

Measure whether the language blocks the dangerous interpretation, asks for the
right human decision, or merely relies on the agent to notice the risk.

### 7.1 Configuration and deployment acceptance

Use the same declared configuration contract across local development, tests,
staging, and production. Acceptance cases must prove that:

- static source checking catches unknown names, type-invalid uses, conflicting
  defaults, and incomplete environment declarations without requiring access
  to deployment secret values;
- local `jadpo config check`, automatic deployment validation, and runtime
  startup reject every missing required value and malformed value with a
  non-zero status and a stable machine-readable diagnostic;
- diagnostics, logs, manifests, generated examples, and health responses name
  the affected declaration and safe provenance but never reveal secret values
  or credentials embedded in connection strings;
- runtime startup repeats actual-value validation before accepting traffic,
  and a test proves no listener or ready signal becomes available after a
  configuration failure;
- readiness distinguishes local parsing from bounded live dependency checks,
  reports required versus advisory failures safely, handles timeouts, and can
  recover when the dependency becomes usable;
- liveness remains healthy during an external dependency outage so the system
  does not create a restart storm;
- a valid local value change causes a controlled `dev` restart, while an
  invalid replacement is rejected without replacing the last ready revision;
  and
- a deployment-harness test presents a bad new revision, observes failed
  automatic validation or readiness, and proves it is not promoted (or is rolled back)
  while the previous healthy revision continues serving.

Run these cases against generated Bun output with package auto-install disabled
so configuration safety does not introduce a third-party runtime dependency.

## 8. Measurements

Measure at least:

- defects introduced and their severity;
- security/policy violations caught before execution;
- unsafe changes that remain representable;
- human decisions correctly requested versus guessed;
- context supplied to the agent;
- input/output tokens used;
- files/declarations touched per change;
- compiler iterations and diagnostic quality;
- time to a correct change;
- docs/tests/audit accuracy after each change;
- generated versus authored tests;
- amount of boilerplate and source size;
- human comprehension of the final repository;
- frequency and nature of escape-hatch use;
- agent performance despite no pretrained familiarity with the language.

“Easier” must not be inferred from one metric. For each change, preserve:

- start, first-correct-build, and completion timestamps, with active agent time
  distinguished from tool/compiler wait time;
- input and output tokens, reused/cached context where measurable, turns, and
  any context manually supplied;
- initial implementation attempts, compiler/test cycles, reverted approaches,
  and human interventions;
- authored files, declarations, and lines changed, reported separately from
  generated artifacts;
- defects introduced, severity, time of detection, and whether the compiler,
  type checker, linter, test suite, runtime, agent, or human found them;
- the final acceptance result and any known residual risk; and
- a short qualitative note explaining cognitive difficulty, surprise, and
  whether the resulting source expresses the requirement clearly.

Token and time results must include failed attempts. Compiler rejection is not
automatically a success: a precise early rejection of an unsafe change is a
benefit, while repeated, misleading, or unavoidable rejection of valid work is
friction.

### 8.1 Compiler-friction and workaround ledger

Create one incident whenever the compiler emits an error or warning during the
application work, the agent cannot express an intended operation directly, or
the agent introduces a workaround or escape hatch. Preserve at least:

- change/task identifier and exact attempted intent;
- minimal source or prompt context needed to reproduce it;
- diagnostic code and message, if any;
- number of repeated attempts and tokens/time lost;
- workaround chosen and its effect on clarity, guarantees, and maintenance;
- whether TypeScript encountered an equivalent problem; and
- the eventual classification and disposition.

Classify each incident as one of: compiler defect, diagnostic/documentation
defect, missing standard capability, missing language feature, intentional
safety boundary, agent misunderstanding, tooling/ecosystem gap, or experiment
setup problem.

For each incident, explicitly decide whether to:

1. fix compiler behaviour;
2. improve the diagnostic or documentation;
3. change the language;
4. add a constrained standard capability;
5. retain the friction as a worthwhile safety boundary;
6. add a conspicuous reviewed escape hatch; or
7. reject the attempted use case.

A proposed language change must be entered in the language issue log and gain a
minimal acceptance fixture before rerunning the blocked change. Preserve the
original failed run so iteration does not erase evidence of design friction.

### 8.2 Final critical review

Report raw measurements per change as well as totals and distributions; one
pathological task must not be hidden by an average. The final review must state
which implementation was easier to author, safer to change, faster to complete,
less token- and context-intensive, less defect-prone, and easier for a human and
a fresh agent to understand.

Separate benefits caused by language semantics from benefits caused by
scaffolding, framework conventions, generated artifacts, better diagnostics,
or experimental familiarity. Include the strongest case for TypeScript, the
strongest case for the new language, unresolved confounders, and every material
failure. It is acceptable—and useful—for the honest conclusion to be that the
language should be redesigned, reduced to a TypeScript framework, or stopped.

The killer result is not “40% fewer lines.” A meaningful result resembles:

> Across twenty backend changes, the TypeScript agent introduced six defects
> requiring review. The constrained language made five of them impossible to
> compile and clearly surfaced the remaining human decision.

## 9. Success criteria

Continue if the experiment demonstrates that:

- common backend tasks are clearly expressible;
- source is dramatically smaller or clearer without becoming opaque config;
- important wrong solutions are unrepresentable or compile errors;
- generated diagnostics improve the agent loop;
- policy and implementation remain aligned;
- humans can understand behaviour from the source/audit;
- unfamiliar syntax does not outweigh reduced ambiguity;
- escape hatches are rare and conspicuous.

## 10. Kill criteria

Stop or pivot if:

- ordinary features continually require arbitrary code;
- business logic outside CRUD becomes clumsy or impossible;
- agents are substantially worse due to lack of prior language knowledge;
- a TypeScript framework achieves essentially the same guarantees;
- the constrained source is not obviously nicer to read and review;
- security depends on generated tests or LLM judgement rather than semantics;
- policies require so much duplication that they drift or become unusable;
- compiler diagnostics cannot isolate meaningful decisions.

## 11. Second application: order/payment backend

Todo flatters DSLs because it is mostly CRUD. The next application should force:

- money and currency types;
- pricing calculations;
- multi-entity transactions;
- one authoritative write feeding cache and graph projections;
- projection lag, replay, rebuild, reconciliation, and read-your-writes;
- Stripe or another payment contract;
- provider error normalisation;
- idempotency;
- webhooks;
- retries/timeouts;
- inventory and order lifecycle;
- refunds and irreversible effects;
- a multi-authority durable workflow with compensation and outcome uncertainty;
- role and ownership collisions;
- data migration after real state exists;
- million-element collections, zero-copy slicing, incremental builders,
  nested collection updates, concurrent reads, foreign buffers, and accidental
  retention of large backing allocations.

This is the first serious test of whether the design is a language rather than a
CRUD configuration format.

## 12. Additional examples

The conversation proposed five design examples before implementation matures:

1. todo backend;
2. booking system;
3. checkout/payment flow;
4. role-based admin system;
5. a genuinely algorithmic feature such as shipping price/scheduling based on
   weight, postcode, customer tier, and warehouse stock.

The algorithmic example is essential. General business logic, function calls,
collections, and control flow are where a superficially attractive DSL may fail.

## 13. Prototype scope

If fictional source survives, the first executable prototype needs only:

- the smallest core declarations;
- a parser;
- a semantic model;
- selected policy/compiler checks;
- a Postgres-backed runtime or generated TypeScript/Bun target;
- generated validators, audit, and a few tests.

Do not prioritise native binaries, package management, IDE integration,
production deployment, or beautiful grammar during this experiment.

## 14. Time discipline

The original recommendation was a small bet—roughly a focused day/weekend for
the first fictional proof and perhaps a week for a brutal experiment—not a
six-month commitment. Continue only when each stage produces evidence stronger
than enthusiasm for the idea.
