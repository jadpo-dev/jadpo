# Product, adoption, and strategy considerations

**Status:** preserved strategic context, not a business plan

## 1. Why the idea feels timely

The current web ecosystem has accumulated many compatible-but-independent
layers, tools, and choices. Developers spend significant effort on integration,
configuration, upgrades, architecture, and review rather than domain behaviour.

LLMs reduce the cost of producing code but increase the amount of code humans
are expected to trust and review. Most emerging tools remain built on the same
TypeScript/JavaScript ecosystem and rely on agents to follow instructions,
invoke security skills, and correctly review their own output.

The opportunity is not to generate more code. It is to simplify the backend
model and replace probabilistic procedural assurance with deterministic
language/runtime guarantees.

## 2. Differentiation

The product cannot be sold merely as:

- fewer tokens;
- nicer syntax;
- a CRUD generator;
- TypeScript generated from a DSL;
- another opinionated backend framework.

The differentiated claim is:

> AI-written backends can be evolved through a constrained semantic model in
> which policy, boundaries, data access, and effects are deterministically
> checked and auditable.

The generated target is secondary. “This makes AI-built backends not terrifying”
is closer to the adoption story than performance.

## 3. The strongest competing hypothesis

An excellent TypeScript framework may deliver most of the value through:

- opinionated architecture;
- generated schemas and migrations;
- Zod or equivalent runtime contracts;
- lint rules and static analysis;
- policy-as-code;
- a constrained data layer;
- an MCP/agent interface;
- code generation and audits.

This approach keeps the language, ecosystem, libraries, tooling, and developer
familiarity. The new-language project must prove that the guarantees require a
more closed semantic model and cannot be achieved cleanly as framework rules.

## 4. Adoption challenges

A new backend language faces:

- no pretrained agent familiarity;
- no package ecosystem;
- limited editor/debugger/tooling;
- trust requirements for compiler/runtime/database handling;
- migration from existing systems;
- operational support burden;
- community and documentation costs;
- pressure for escape hatches;
- scepticism if the first target is merely generated TypeScript.

Human readability and conventional syntax help, but they do not remove these
costs.

## 5. Open source and commercialisation

The natural core may be open source, and the original discussion did not find a
clear near-term commercial model. Possible future value could exist in hosted
build/deployment, policy approval, audit/compliance, observability, enterprise
governance, or managed infrastructure, but none was validated.

Building distribution, support, community, and trust is a substantial project
separate from proving the language idea.

## 6. Time and portfolio decision

The conclusion of the strategic discussion was intentionally hard-nosed:

- this is not an obvious near-term money project;
- it should not displace projects with clearer returns;
- AI can perform much of a prototype, but the concept still needs significant
  human guidance;
- preserve it as a research idea and run a small proof when time permits;
- if a one-day fictional todo design is not strikingly better, let it go;
- if it is clearly better on safety and clarity, earn the next experiment.

The documentation effort now makes that small experiment cheaper without
pretending commercial validation has occurred.

## 7. Naming restraint

The original advice was to postpone naming during the hypothesis phase. The owner
later adopted Jadpo and jadpo.dev ([recorded decision](decision-register.md#jadpo-name-and-canonical-domain--2026-09-25)).
The underlying caution remains: naming creates attachment and can turn a
testable idea into a project that feels obliged to survive. Preserve that
rationale without treating it as an instruction to undo the adopted name.

## 8. Strategic upside if proven

If the validation succeeds, possible strategic value includes:

- a trusted application model for agent-generated software;
- portable semantics across coding-model providers;
- far smaller human-review surfaces;
- built-in security and compliance evidence;
- deterministic change-impact analysis;
- generated docs/audits that do not drift;
- a runtime/hosting ecosystem designed around the semantic graph;
- a foundation for software specified through intent and policy rather than
  implementation review.

These are outcomes to validate, not assumptions.

## 9. Optional advisory intelligence and hosted observability

**Status:** recovered exploratory discussion; not an approved dependency or
business plan. Source: “LLM Backend Language Idea”, 2026-09-25 follow-up turns
`d96d3a2f-e8ab-4e36-91ad-236af2ae564b`,
`72d783f6-dccd-4c97-939e-a4ac6e2966e5` and
`b51b72e0-569c-4207-abfe-e240080c8d09`. Roadmap owners: **RM-1106/RM-1107**.

The user raised **Jev** for development assistance, debugging and monitoring,
including health prediction and min/max thresholds, and asked whether it could
be self-hosted or bundled. They also raised proxying such a service through
their website as a paid offering. Preserve the idea without treating historical
claims about Jev availability, prices, latency or commercial terms as current
verified facts; those need checking if the idea is selected.

The discussed advisory uses include prioritising root causes among cascading
compiler errors; choosing useful human/agent explanations; detecting possible
intent drift; classifying semantic-change/migration risk; prioritising additional
tests; and identifying when an agent loop needs a human decision. Runtime uses
include failure clustering, likely causes, deployment regressions, operation-level
health, anomalies, capacity risk and predicted SLO/threshold breaches from
latency/error/load/queue/database signals and historical baselines.

The proposed boundary keeps compilation, policy checks and required tests
deterministic and usable offline. Advisory judgements retain confidence,
provenance and underlying facts, cannot validate an invalid program or authorise
a repair, and supplement deterministic hard limits. A replaceable provider
interface could support a hosted API, a future local model or no model; source,
customer data, logs and secrets do not acquire permission to leave the system.
These are evaluation constraints, not a committed Jev integration or model.

A possible commercial service would consume opt-in compiler-derived semantic
telemetry: route/action/entity/effect identity, source/deployment changes and
bounded incident evidence. Potential value includes monitoring/debugging,
longer history, comparisons, alerts, incident timelines and assistance preparing
reviewable fixes. Hosted builds/deployments were a separate later possibility.
The local language/compiler should remain useful without a subscription. A
provider proxy alone does not establish differentiation; customer demand,
cost/privacy/retention, provider terms, portability and support burden remain
unvalidated. E11's requested console/MCP functionality does not require this
commercial or probabilistic layer.

### Follow-up discussion — 2026-10-13

**Status:** exploratory, not a business plan. Source: a 2026-10-13 session
with the owner; the technical ideas are proposals in
[area 16](language/16-built-in-services.md) (path IDs, monitoring
thresholds) and [area 17](language/17-debugging-and-review.md) (path
coverage, production in review, replay, query cost).

Jadpo has to be worth using with no paid feature. Nothing commercial
matters until the language proves itself (§6).

**Stopping people bypassing a hosted service.** It cannot be done and should
not be attempted: anyone can fork an open compiler and point it at their own
endpoint, and DRM in an open compiler is forked out quickly and costs trust.
The endpoint stays an ordinary setting and the manifest format stays open.
What protects a hosted service is that running it well is real work
(storage, retention, querying at scale, alerting, uptime, team features),
priced below the cost of building and running a copy. Sentry, Grafana,
PostHog, Temporal and Encore sell hosting around open or self-hostable
cores. If the hosted service's own code is published, a source-available
licence such as Sentry's Functional Source License (Apache after two years)
protects it while the compiler stays permissively licensed.

**Do not bypass OpenTelemetry.** Capture is not where Jadpo differs;
analysis is. Standard transport means teams can keep their existing tools,
and a Jadpo-aware service reads the same stream plus the build manifest
(graph, stable identities, thresholds) published per build, so it needs no
source code.

**Candidate offerings, strongest first:**

1. **A review gate on the git host.** The approval report, required sign-off
   for policy and personal-data changes, and interdiffs between agent
   revisions, as a pull-request app. Free for public repositories, paid for
   private ones and teams. This sells the reason Jadpo exists: people
   reviewing what models wrote.
2. **Compliance evidence.** A signed record per build of who can reach which
   data and who accepted each change. Regulated firms buy audit trails.
3. **Hosted observability built on the manifest.** Path-aware storage,
   cross-build comparison, generated alerts, an MCP endpoint for agents.
   Strongest combined with item 1 (production traffic in the review);
   observability alone is a crowded market.
4. **Hosted build and deploy,** as Encore Cloud does. Plausible, and heavy
   to operate.

**Keeping the options open costs little now:** stable identities for every
declaration, the build manifest as a versioned build output, and path IDs as
OpenTelemetry attributes.
