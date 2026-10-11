# Source conversation extraction and coverage

**Source:** [shared ChatGPT conversation](https://chatgpt.com/share/6ab5a5d4-8f44-83ed-8381-4a9b8518bf3d)  
**Conversation title:** “LLM Backend Language Idea”  
**Shape:** eighty user prompt/voice segments plus assistant responses  
**Purpose:** demonstrate that the documentation preserves the substantive
content, rationale, alternatives, and unresolved questions from the source

This is not a verbatim transcript. Voice fragments, acknowledgements, repeated
ideas, and half-finished sentences are consolidated into their complete meaning.
Historical syntax is preserved where it influenced a later decision. Current
authority remains the [decision register](decision-register.md).

## 1. Chronological extraction

### 1.1 Initial proposition

The conversation opened with a language written specifically for LLMs to build
website backends. The first answer reframed it away from “a nicer language that
LLMs happen to write well” toward a narrow mixture of:

- database schema;
- API contract;
- permissions;
- business rules;
- events/background work;
- external service declarations.

Early examples introduced entities, actions, routes, events, jobs, services,
policies, and tests. The key proposed advantage was eliminating entire classes
of choices: no default raw SQL, ORM selection, web framework selection, folder
architecture debate, or arbitrary library choice.

The compiler would make invalid architectures difficult by requiring
authorisation, declared side effects, safe secrets, route schemas,
retry/idempotency behaviour, generated migrations, lifecycle choices, resource
limits, and transactions.

This introduced the first major thesis: **the compiler becomes part of the
LLM's reasoning loop**, presenting unresolved decisions as structured tasks.

Captured in:

- [vision](vision.md), sections 2–4;
- [semantic model](semantic-model.md), sections 1–2;
- [assurance model](assurance-model.md), sections 2–3.

### 1.2 Earlier backend-language idea and LLM feasibility

The user explained this was an older idea: a prescribed backend language where
a route kept auth, title/description, tests, validation, and documentation
together. Inputs, outputs, and values going to the database would be validated.

It had previously seemed irrational to create a new language when TypeScript was
battle-tested and unusual requirements eventually need general-purpose code.
LLMs made implementation feel more feasible and changed the value proposition:
the language could be simpler, predictable, and potentially more token-efficient
for agent authorship.

The conversation recognised the tension between a very narrow CRUD language
and the need for unusual or algorithmic business logic. A possible extension or
handoff mechanism was raised but not resolved.

Captured in:

- [vision](vision.md), sections 1, 6, and 7;
- [validation plan](validation-plan.md), sections 10–12;
- [decision register](decision-register.md), open escape-hatch questions.

### 1.3 Related tools and positioning

The assistant noted adjacent backend DSLs, schema-driven generators, low-code
platforms, and AI-native languages, but characterised most AI-native work as
agent orchestration or full-stack generation rather than a deliberately boring,
constrained backend language.

No external comparison in the conversation established market uniqueness. The
documentation therefore preserves this as context, not evidence.

Captured in:

- [product strategy](product-strategy.md), sections 2–4.

### 1.4 Living documentation and provider independence

The user emphasised project documents that update as functionality is
implemented, carrying context and decisions in the repository. A new developer
or model should not have to reconstruct instructions. The language should work
across LLM providers rather than depend on Claude Code or any other single
agent.

This became the code/intent/policy/audit model and the repository-as-living-
specification workflow, related to the user's Project OS thinking.

Captured in:

- [vision](vision.md), section 5;
- [agent workflow](agent-workflow.md), sections 3 and 8;
- [semantic model](semantic-model.md), intent/rule/decision concept.

### 1.5 Safety nets enabled by default

The user proposed that protections such as authentication should be enabled by
default on every route and that an LLM must explicitly disable them. The system
should make it hard to go wrong.

The discussion added easy, generated audits of each route's security properties
and suggested checking those audits against a separate statement of intended
security.

Captured in:

- [charter](charter.md), accepted foundations;
- [assurance model](assurance-model.md), sections 2–6;
- [vision](vision.md), section 9.

### 1.6 Policy ownership versus derived audit

The conversation identified the “marking your own homework” problem: if the LLM
maintains both implementation and its check file, the check may be meaningless.
The resolution was:

- human-owned intent/auth policy is the source;
- implementation may be agent-authored;
- audit is compiler-derived;
- policy and implementation are compared in CI;
- the build fails when they disagree;
- an agent-assisted policy edit must still be explicitly driven/approved by the
  user.

Captured in:

- [assurance model](assurance-model.md), sections 1, 4, and 8;
- [POLICY-001 decision plan](policy-plan.md), especially the qualified scoped
  role model, membership/direct bindings, automatic database scoping, complete
  input-to-output validation chain, and semantic weakening approval;
- [agent workflow](agent-workflow.md), section 7;
- [decision register](decision-register.md), accepted and rejected items.

### 1.7 Syntax strictness and formatting

The user asked whether syntax should be strict or loose and whether formatting
mistakes such as two spaces should fail compilation. The conclusion was:

- semantic structure should be strict;
- syntax should be labelled, predictable, and readable rather than cryptic;
- humans must be able to inspect and sometimes write it;
- whitespace differences should not be semantic;
- a formatter should normalise source;
- formatting on every save risks disrupting an agent's iterative edits;
- formatting should therefore happen at explicit/checkpoint boundaries such as
  compile, pre-commit, pre-merge, or CI.

Captured in:

- [syntax](syntax.md), source form and formatting;
- [compiler/runtime](compiler-runtime.md), section 13;
- [decision register](decision-register.md), accepted/rejected alternatives.

### 1.8 Postgres and declarative data access

The user liked an opinionated Postgres model where source could not normally
write SQL, while recognising complex queries as a pressure point. The assistant
described the model as a hidden ORM with declarative or safe query blocks and a
deliberate audited escape hatch for the minority of unsupported cases.

Captured in:

- [semantic model](semantic-model.md), entity/query sections;
- [syntax](syntax.md), entities and persistence;
- [compiler/runtime](compiler-runtime.md), section 7.

### 1.9 External API contracts

The conversation rejected requiring every provider to build a bespoke library.
Instead, OpenAPI or provider documentation could produce a reviewed contract.
The compiler would then understand permitted operations, schemas,
authentication, errors, retries, and egress. Documentation-to-contract
conversion could use an LLM once, followed by review and commitment.

Captured in:

- [semantic model](semantic-model.md), service;
- [syntax](syntax.md), external services;
- [compiler/runtime](compiler-runtime.md), section 8.

### 1.10 Toolchain and target debate

Rust and Zig were raised for implementing the language. Rust was preferred for a
compiler and safety. The assistant suggested generating TypeScript initially to
avoid rebuilding the world, with Bun or similar as a runtime and Postgres as the
boring infrastructure.

The user worried that a TypeScript target weakened differentiation and
performance. The conclusion was:

- a native binary is a cleaner possible long-term outcome;
- an initial TypeScript target is pragmatic;
- performance is not the first win;
- TypeScript cannot be the product story;
- constraints and audit are the reason the compiler exists.

Captured in:

- [compiler/runtime](compiler-runtime.md), sections 2–4;
- [product strategy](product-strategy.md), sections 2–3.

### 1.11 First full synthesis

The idea was summarised as:

> A backend language where the compiler owns boring decisions, the human owns
> intent, and the LLM performs implementation work between them.

The strong combination was code + intent + policy + audit. Authentication,
validation, controlled data access, no raw SQL/arbitrary HTTP, safe secrets,
timeouts, transactions, rate limiting, and audit logging should default safely.

The conversation advised starting with the semantic model rather than the
grammar: entity, route, query, action, policy, event, job, service, and possibly
decision. A possible compiler pipeline was parser → AST → semantic model →
policy checker → audit graph → executable.

The source should make wrong solutions unrepresentable or compilation errors.
The optimisation target was refined to **minimum ambiguity per token**.

Captured across:

- [charter](charter.md);
- [vision](vision.md);
- [semantic model](semantic-model.md);
- [compiler/runtime](compiler-runtime.md).

### 1.12 Todo application as the first concrete test

The user proposed a todo list as the real test. The response sketched users,
todos, authentication, ownership, validation, filters, CRUD, tests, a policy
file, generated security tests, a route audit, and an overdue-reminder job.

The compiler could infer schema, migrations, validation, auth, 404/malformed-ID
behaviour, serialisation, OpenAPI, docs, route inventory, and security audit.

The historical syntax and artifacts are preserved in
[examples/chat-todo-sketch.md](../examples/chat-todo-sketch.md). Other early
examples are preserved in
[examples/chat-design-sketches.md](../examples/chat-design-sketches.md).

### 1.13 Worth-pursuing and kill-test discussion

The conclusion was “yes, as a small, deliberately brutal experiment,” not “yes,
build a language now.” The key research question was whether the new model makes
agents materially better than TypeScript.

The proposed experiment builds the same todo backend twice and gives both a
sequence of changes including ownership, sharing, admin, due dates, reminders,
soft delete, public routes, unsafe cross-user access, and schema evolution.

Measurements include context, tokens, security defects, touched files, docs and
test drift, compiler catches, and human understandability. Three explicit kill
conditions were repeated:

- ordinary SaaS constantly needs arbitrary escape hatches;
- agents are worse because they do not know the language;
- an opinionated TypeScript framework supplies equivalent guarantees.

Captured in:

- [validation plan](validation-plan.md);
- [product strategy](product-strategy.md), section 3.

### 1.14 Project OS save

The original chat recorded that the idea was saved to the user's Project OS as
“LLM-native backend programming language,” including human policy, audits/docs,
external contracts, Postgres defaults, provider independence, the todo
comparison, and continuation criteria.

This is historical context only; these repository documents now contain the
working detail.

### 1.15 Time, commercial value, and project prioritisation

The user expressed frustration with the fragmented web stack but also limited
time, many projects, and uncertainty about monetising an open-source language.
The response separated two ideas: building a new language and proving that
backend development can be radically simplified.

The advice was not to prioritise it as a near-term money project. Adoption,
distribution, support, and community are substantial work. Preserve it as a
small research bet; if a one-day/weekend todo spec is not strikingly good, stop.
AI can implement much of a prototype, but the concept still requires focused
human guidance.

Captured in:

- [product strategy](product-strategy.md), sections 5–6;
- [validation plan](validation-plan.md), section 14.

### 1.16 Renewed assurance framing

The user returned to the idea because AI-written websites were shipping without
meaningful review, with security delegated to optional agent skills. This
produced the sharpest framing:

- do not build a language that merely makes AI programming easier;
- build an environment in which AI is deliberately not trusted;
- implementation and guarantees must not both be probabilistic;
- as models produce more code, human review becomes less viable;
- the language/runtime should replace instruction soup with invariants;
- humans review intent, policy, and exceptions.

The discussion listed preventable classes including missing auth, IDOR,
arbitrary queries, private-field output, secret leaks, undeclared input,
unbounded external calls, and forgotten transactions, while acknowledging that
requirements, business logic, and external systems can still be wrong.

Captured in:

- [vision](vision.md), sections 2–3 and 10;
- [assurance model](assurance-model.md), sections 3, 5, and 11.

### 1.17 Debugging generated TypeScript

The user raised a detailed concern: production errors would mention generated
TypeScript, so how would they map back to original source? The response defined:

- generated TypeScript is never developer-facing source;
- ordinary source maps map target locations;
- richer semantic metadata maps generated code to stable AST/semantic node IDs;
- runtime errors are caught and translated into original language concepts;
- generated behaviour without a literal source line maps to the declaration and
  internal conceptual component;
- low-level traces remain for compiler/runtime maintainers;
- semantic logs integrate with Sentry/Datadog;
- deployment may include `app.js`, `app.js.map`, and `app.meta`;
- target compile failures are compiler bugs, not application fixes;
- agent-facing errors should be smaller and more actionable than target traces.

Captured in:

- [compiler/runtime](compiler-runtime.md), sections 9–12 and 14.

### 1.18 General programming and human-readable syntax

The user wanted to explore real business logic, not only routes and CRUD, and
required the language to remain readable/writable by humans. The assistant
proposed conventional TypeScript/Python/Rust-shaped syntax with novelty in
semantics rather than punctuation.

The language needs ordinary functions, values/structs, enums, lists/maps,
optionals, conditionals, exhaustive matches, iteration, and returns, while
excluding reflection, metaprogramming, runtime loading, raw sockets, `eval`, and
untyped dynamic objects.

Other points:

- immutable by default;
- domain-aware types such as email, money, percentage, datetime, and duration;
- exhaustive matches;
- explicit optional narrowing;
- route-local behaviour with extraction only when reuse justifies it;
- discourage controller/service/repository/factory/mapper proliferation;
- choose `if` and `match`, not multiple equivalent conditional forms;
- one error model, not exceptions + codes + result + callback + promise styles;
- test todo, bookings, checkout, admin, and an algorithmic shipping/scheduling
  case before implementation.

Captured in:

- [syntax](syntax.md), throughout;
- [semantic model](semantic-model.md), locality/abstraction;
- [validation plan](validation-plan.md), sections 11–12.

### 1.19 Variable clarity, collection operations, and error model

The user objected that the first syntax did not visually distinguish variables,
disliked ambiguous `.empty` prototype-like syntax, preferred immutability by
default, and wanted error handling better than both JavaScript exceptions and
Rust `Result` plumbing.

The evolved response proposed:

- explicit binding keyword, later settled as `var` / `var mut`;
- language/standard operations such as `collection.count(items)` rather than magic
  properties or prototype methods;
- visible compiler-understood `create`, `query`, `update`, `delete` rather than
  `Order.create`/`order.update`;
- declared `fails` and `reject` without wrapping every successful value;
- propagation only when the enclosing context declares/handles the same
  failures;
- a possible `attempt` mapping block;
- three categories: domain failure, infrastructure fault, bug/impossible state;
- `T?`/`none` instead of exposed `Option<T>` mechanics;
- no truthiness.

Captured in:

- [syntax](syntax.md), bindings, collections, persistence, and errors;
- [semantic model](semantic-model.md), failure model;
- [decision register](decision-register.md), accepted/rejected choices.

### 1.20 `var`, absence, omission, and service error normalisation

The user explicitly preferred `var total = ...` and `var mut retries = 0`, asked
about Stripe-style errors, and asked whether to support `undefined`, `null`, or
neither.

The final choices were:

- `var` immutable, `var mut` mutable;
- no `undefined`;
- no ambient `null`;
- one absence value, `none`;
- `T?` for a value that may be absent;
- omitted input field is different from a supplied `none`;
- `Text? optional` can represent patch “not supplied / set / clear” states;
- non-null database fields by default;
- database `NULL` maps to `none`;
- concrete objects have no missing fields;
- projections represent selected subsets;
- incoming JSON `null` may map to `none` and outgoing `none` provisionally maps
  to JSON `null`;
- service contracts map many provider errors into a smaller domain vocabulary
  or operational faults.

Captured in:

- [syntax](syntax.md), sections 4–6 and 12;
- [semantic model](semantic-model.md), trust and failure models;
- [decision register](decision-register.md).

### 1.21 Runtime type safety

The user explicitly confirmed the need for runtime safety. The response made
boundary validation a language property, not a remembered library call. It
listed HTTP input/output, database read/write, config, queues/events, external
responses, and caches.

Domain constraints should become static checks where possible, runtime
validators, database constraints, OpenAPI/docs, and tests. An invalid Postgres
row should be a hard contract failure. Output validation prevents returning a
full internal user where a public shape was declared.

The final promise was: **types are executable contracts**.

Captured in:

- [semantic model](semantic-model.md), sections 2–3;
- [compiler/runtime](compiler-runtime.md), sections 6–8;
- [charter](charter.md).

### 1.22 Agreed next step

The closing recommendation was not more isolated syntax exploration and not a
compiler. The sequence was:

1. freeze a tiny v0 charter;
2. write the complete golden todo backend;
3. maintain a language-design issue log;
4. write a deliberately difficult payment/order example;
5. only then design the final AST/semantic implementation;
6. sketch generated artifacts and repeatedly ask whether TypeScript habits are
   being copied unnecessarily.

The conversation advised resisting a language name until the hypothesis is
proven.

Captured in:

- [documentation index](README.md), current phase;
- [charter](charter.md), current test;
- [validation plan](validation-plan.md).

## 2. Topic-to-document matrix

| Source topic | Primary document | Supporting documents |
| --- | --- | --- |
| Core thesis and division of authority | [vision](vision.md) | [charter](charter.md), [agent workflow](agent-workflow.md) |
| Language concepts and guarantees | [semantic model](semantic-model.md) | [type system](type-system.md), [syntax](syntax.md) |
| Nominal types, field references, and validated construction | [type system](type-system.md) | [type-system acceptance cases](../examples/type-system-cases.md), [syntax](syntax.md) |
| Domain failures, HTTP mapping, disclosure, and stacks | [failure model](failure-model.md) | [failure-model acceptance cases](../examples/failure-model-cases.md), [syntax](syntax.md) |
| Concrete surface syntax | [syntax](syntax.md) | [decision register](decision-register.md) |
| Safe defaults and security | [assurance model](assurance-model.md) | [charter](charter.md), [semantic model](semantic-model.md) |
| Policy ownership and CI gate | [assurance model](assurance-model.md) | [agent workflow](agent-workflow.md) |
| Living docs / Project OS connection | [agent workflow](agent-workflow.md) | [vision](vision.md) |
| Postgres and data model | [semantic model](semantic-model.md) | [compiler/runtime](compiler-runtime.md), [syntax](syntax.md) |
| External API contracts | [semantic model](semantic-model.md) | [compiler/runtime](compiler-runtime.md), [syntax](syntax.md) |
| Runtime validation | [compiler/runtime](compiler-runtime.md) | [type system](type-system.md), [semantic model](semantic-model.md) |
| TypeScript/Bun/native target trade-off | [compiler/runtime](compiler-runtime.md) | [product strategy](product-strategy.md) |
| Source maps and semantic diagnostics | [compiler/runtime](compiler-runtime.md) | [agent workflow](agent-workflow.md) |
| Todo example | [historical todo sketch](../examples/chat-todo-sketch.md) | [validation plan](validation-plan.md) |
| Booking, service, lifecycle, and diagnostic examples | [historical design sketches](../examples/chat-design-sketches.md) | [semantic model](semantic-model.md), [assurance model](assurance-model.md) |
| Experiment, metrics, and kill criteria | [validation plan](validation-plan.md) | [product strategy](product-strategy.md) |
| Time, commercial, and adoption caveats | [product strategy](product-strategy.md) | [validation plan](validation-plan.md) |
| Decisions and unresolved questions | [decision register](decision-register.md) | all specifications |

## 3. Completeness note

The documentation intentionally does not preserve filler acknowledgements,
voice-transcription artefacts, repeated fragments, or a prior tool claim that
the idea was saved elsewhere. It does preserve every substantive design idea,
motivation, example class, recommendation, objection, trade-off, decision,
rejected alternative, implementation proposal, validation criterion, and
commercial caution identified in the second end-to-end reading.

Future work should cite and update these documents rather than relying on the
shared conversation. The conversation remains useful only as historical
provenance.

## 4. Roadmap reconciliation — 2026-10-01

The roadmap reorganisation preserved an earlier full snapshot, but compressed
unfinished briefs into short task rows and left some issue-log questions without
an explicit task. Revset was searchable only inside history. A separate Jev
follow-up was absent from both roadmaps. This audit restores discoverability
and outstanding scope without treating every brainstorm as approved delivery.

The active roadmap now owns a [discussion/legacy index](implementation-roadmap.md#discussion-and-legacy-work-index),
[a complete active-issue mapping](implementation-roadmap.md#language-issue-coverage)
and the [Revset review brief](implementation-roadmap.md#revset-and-the-web-review-ui).
Existing IDs and estimates are retained. Newly explicit tasks RM-214–RM-220,
RM-408–RM-410, RM-508, RM-1005 and RM-1106–RM-1107 are unestimated and need
planning. Conditional items retain activation conditions; this does not enlarge
the other chat's selected delivery scope. The original history snapshot and
original extraction above are preserved.

### Sources and limits

Checked the current roadmap and preserved pre-reorganisation snapshot against
the language-issue log, decision register, original 22-theme conversation
extraction, owning specifications, validation records and relevant active and
archived chats returned by the app. The machine-readable
[audit evidence](../tests/validation/roadmap-context-audit.json) records source IDs,
retrieved page/turn counts, file hashes, issue coverage, task/dependency checks,
and per-epic counts/effort at inspection time. Raw/private chat transcripts are
not copied into the repository.

Relevant Codex chat pages were followed until their API reported no further
pages. ChatGPT retrieval returned short follow-up windows, sometimes only a few
turns even when the original discussion was longer; a `hasMore: false` response
is not proof of a complete lifetime transcript. The original shared discussion
is covered by this document's existing extraction, not a fresh full-transcript
revalidation. Tool turn summaries and truncated text cannot establish that no
unseen idea exists. This audit accounts for the recovered material and names
that access limitation rather than promising omniscience. Messages arriving in
other chats after this audit's source capture require normal intake.

### Recovered context and dispositions

| Source discussion | What must survive | Active destination |
|---|---|---|
| Inspect revset.dev | Explicit request to add focused graphs; relationships/new reachability, revision comparison, evidence, unchanged-graph behaviour and comprehension experiment; implement within Jadpo without Revset dependency | RM-601/RM-602/RM-605/RM-704 and the active full brief |
| Review project documentation | Human inability to reconstruct AI diffs, side effects and relationships; custom web UI; assurance critique and consistent nominal/enum syntax | E06/E07, original P10R rationale, type/failure specifications |
| LLM Backend Language Idea follow-ups | Jev for development guidance, debugging, health/min-max prediction; possible paid monitoring/provider proxy; deterministic compiler remains independent | RM-1106/RM-1107 and [preserved strategy context](product-strategy.md#9-optional-advisory-intelligence-and-hosted-observability) |
| Name the language and domain / Rename language to Jadpo | Owner adoption of Jadpo/jadpo.dev, reported repository rename, optional future .com and still-unverified external domain setup | [Superseding naming decision](decision-register.md#jadpo-name-and-canonical-domain--2026-09-25), RM-1005; old naming restraint is historical advice |
| Find top developer websites | Every named benchmark, evidence strength, shared rubric, real journeys, prototypes and maintenance cost | RM-1001–RM-1004 and linked full website brief |
| Continue through the roadmap | Formatter rules/tests for every grammar variation and whitespace perturbation; reserved words in type/variable positions, case, characters, leading digits | RM-201/RM-202/RM-203; partial tests do not close the whole request |
| Find roadmap for compiler IDE / Continue next task / Implement P10.6 and DX2 | Hand-written learning loop, watch/health/restart, beautiful human terminal plus rich machine packets, useful contextual diagnostics, every public error and IDE parity | Completed DX foundations; RM-210/RM-211/RM-219/RM-508/RM-704 |
| Discuss next part / Implement roadmap tasks | Non-persistent entity power, entity dossiers/operations, restricted named reads, graph/cache representations, automatic local safety versus explicit multi-owner intent | DATA-007/TX/CONSISTENCY authority; RM-205/RM-206/RM-212/RM-405–RM-407/E08 |
| Plan next work / Build unblocked Jadpo roadmap items | All auth strategies including APIs and server-to-server; optional pinned JWT dependency; company/field policy; Temporal/Zone/friendly dates; secret prompts; consistency across naming | AUTH/POLICY/CONFIG/TIME plans, E01–E04, RM-504/RM-220 |
| Recall Rust enum formatting | Payload variants and exhaustive matches; no automatic display/method/trait feature; defer five external sessions until implementation is complete, not forever | TYPE-006, RM-801/RM-408, E07 |
| Review shared ChatGPT conversation / Continue current task / Continue task / Continue the build | Nominal field identity, nested objects, auth defaults, joins, immutability, imports, transactions, index acceptance, deterministic scaffold, config readiness and fair comparison | Original extraction/specs, E01–E08 and issue coverage |
| Find remaining roadmap items / Run bounded native Rust experiment / Cloudflare Workers Runtimes / Roadmap And WebAssembly Feasibility | Rust/Wasm capability and host limits, source/compiler-owned lowering versus fixtures, native/Workers choices, boundary enforcement, large rows, performance metrics, simple build commands and deferred embedded stack ideas | E09, compiler/runtime and retained experiment plans/results; no automatic target promotion |
| Build verifiers for agent loops | Separate verification of Jadpo from applications authored with it; measured robustness rather than hoping code works | E05, TEST-001, golden acceptance and reusable verify-loop workflow |
| Reorganize roadmap into epics | Complete idea capture, stable epic/task dependencies, honest measured estimates, uninterrupted eligible work, model suitability, user questions, progress artifacts and useful documentation; later console/MCP/job history idea | Workflow/timing/reporting, E11 and coverage-preservation rule |

The issue index also restores explicit visibility for migrations beyond bounded
SQL, precise constraint identities, workload-aware indexes, ordinary logic/type
extensions, source identity/maps, resource limits, HTTP streaming/cancellation,
durable intent/docs, failure/localisation decisions and future config/time/test
extensions. Existing constraints and speculative status are retained rather
than silently selecting language designs. Newly counted rows represent recovered
or clarified scope, not newly completed functionality.

Verification and timing: `META-ROADMAP-RECONCILIATION-c50aee9fe93b`. Completion of
this documentation run does not close any recovered implementation task.

## 5. Component events, syntax and application graph intake — 2026-10-04

Source: the visible conversation in **Assess event-driven architecture**, thread
`01a105c2-7987-7571-86c8-2a4804c75f53`, including the owner's latest withdrawal
of postfix `?` and request for options/examples/effects. This supplements the
original extraction; it is not a re-audit of unseen conversations. Read-only
delivery records established a concurrent golden scope; no message, new Goal,
delegation or implementation was authorised by this planning intake.

| Theme / correction | Owning destination |
|---|---|
| Monolith rationale; LLM reaction wiring; one enforceable pattern; imports must not confer side-effect authority | RM-309/RM-310 and [context plus effect/bypass matrix](work-plans/roadmap-assessment.md#rm-309--one-enforced-interaction-model) |
| Mandatory publication across all mutation paths, generated typed catalogue, payload sufficiency/privacy and business-intent limits | Same contract plan; RM-311 durability, RM-312 successor golden evidence |
| Multi-event handlers; optional execution keys/defaults; private actions across files; stateful subscribers instead of redundant workflow syntax | RM-309, reconciled WORKFLOW-001/RM-802–RM-804; no frozen contract silently replaced |
| Durable fan-out, per-subscriber recovery, retries/backoff, uncertainty, enrollment, capacity, versioning/replay | RM-309/RM-311; reuse frozen ASYNC-001/SERVICE-001 and current worker instead of another engine |
| Language-wide syntax before event/graph work; rejection of stacked `attempt send`; postfix preference then withdrawal; full option/effect comparison | RM-222 [syntax plan and reopened comparison](work-plans/roadmap-assessment.md#rm-222--language-wide-syntax-decision-and-migration-brief), conditional RM-223 migration |
| Hierarchical documented graphs for UI/LLM, live paths/logs/timing/errors and filtering, measured bottleneck repair | RM-1108 [graph contract/probe](work-plans/developer-console-mcp.md#hierarchical-application-graph-plan--2026-10-04), existing RM-1101–RM-1105 consumers |
| Production graph attribution, immutable incident deployment, branch divergence/graph-changing fixes, stable versus build-local IDs | Expanded RM-219; graph plan and existing standard/restricted failure channels |
| Preserve complete session context, plan now, report remaining issues honestly, protect other session | [Session/planning checkpoint](work-plans/roadmap-assessment.md#session-context-and-planning-entry--2026-10-04), [provisional decision direction](decision-register.md#enforced-component-messaging-and-application-graphs--2026-10-04), conditional successors after RM-110 |

Only documentation consistency checks belong to this planning session. Negative
compiler cases, crash/adapter traces, independent reviews and overhead experiments
are planned acceptance work, not passing evidence. No new task completion marker
is added. Timing: `META-EVENT-GRAPH-PLANNING-644a33831de8`.

**Later owner decision in the same chat:** retain `UpperCamelCase` for named
types/contracts and `lower_snake_case` for runtime names; retain uppercase
services/applications and the existing specialised declaration family. Remove
the redundant authored principal name in a future singleton-block migration,
with compiler-defined `Principal` and runtime `current_principal`. This
supersedes suggestions of all-lowercase or lowerCamelCase naming and lowercase
service/application declarations. The owner also favours prefix `attempt`,
parenthesised `emit_event(...)` and event-only typed variants over separate
command/fact APIs; admission/completion/authority rules still need review.
See the [locked naming/principal decision](decision-register.md#naming-and-principal-direction--2026-10-04)
and updated RM-222 plan. Decision capture is not compiler implementation or task
completion. Timing: `META-NAMING-DECISION-aedc99eda0f6`.


**Continued design batch:** the owner authorised continuing here, with shared
implementation waiting for the other delivery session and RM-110. The owner
selected `attempt query(Todo) { cardinality: required ... }`: one operation with
query details inside the block, not cardinality-named aliases. No required
semicolons or change to `attempt` propagation is implied. The
[syntax inventory](syntax.md#22-successor-syntax-review--2026-10-04),
[grammar delta](grammar-v0.1.md#18-successor-grammar-candidate--2026-10-04),
[event contract candidate](event-model.md) and
[identity/incident candidate](generated-artifacts.md#semantic-identity-and-incident-artifact-candidate--2026-10-04)
capture the design and its limits. Conditional technical plans now cover
RM-223/RM-310–RM-312. Required reviews and executable evidence remain pending;
no frozen contract or implementation task closes. Batch settings were explicitly
confirmed as `gpt-6-astra/medium`. Timing:
`META-EVENT-CONTRACT-DESIGN-b62dfe08bb0b`.


**Later continuation and independent detail review:** concrete `triggers:`
restrictions now attach to entity/service effect leaves; explicit event
invoke/read policy and delegated worker/origin intersection cannot be replaced
by a trigger match. Bounded publication evaluates typed snapshots before commit.
The expanded query/mutation candidate covers current cardinalities and paging;
proposed cursor tuple inference and block-contained create/page metadata remain
review candidates beyond the owner-selected outer query form. Standalone emission
uses the existing `Unit` type and always requires `attempt`. The independent
[review record](../tests/validation/event-design-independent-review.json) preserves
three additional findings and their accepted corrections, without claiming freeze
or implementation. Timing: `META-EVENT-CONTRACT-DETAIL-1605fb22931d`.

**2026-10-05 continuation:** the owner selected allowing narrowly scoped,
compiler-controlled completion-view maintenance after origin revocation, while
retaining worker authorisation and source visibility/privacy checks. The
[decision](decision-register.md#completion-projection-after-origin-revocation--2026-10-05)
does not grant request/provider authority, raw projection writes or bootstrap.
Independent review corrected compiler-owned replacement, partial coverage and
read-time incarnation/generation binding. The event owner also records concrete
persisted-state coordination pressure cases; the syntax owner maps current
fixtures to future migration comparisons. All remain design evidence, with
implementation behind RM-110 and the relevant contract gates. Timing:
`META-EVENT-REACTION-CONTRACT-5d4ac92b09e5`.
