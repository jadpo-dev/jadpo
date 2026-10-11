# E11 intake — Developer console and MCP runtime inspection

Owner idea, 2026-10-01. The 2026-10-03
[planning sweep](roadmap-assessment.md#e11-conditional-console-mcp-and-advisory-research)
provides bounded next-stage plans. Feature contracts, activation and several
estimates remain open; there is no implementation or new Goal. Activate by
explicitly selecting E11 or a named task. Preserve the golden-todo milestone.

**Outcome:** make running Jadpo applications understandable to both developers
and their connected LLMs. A UI shows performance over time, errors, changes,
scheduled jobs and execution history/status/errors/output. A first-party MCP
server exposes the relevant runtime evidence so an LLM can investigate in depth,
identify bottlenecks and support verified fixes. Shared compiler/runtime identity
and semantics should connect a slow operation to its source, effects and revision.

| Task ID | Planning depth and reason | Dependency evidence / blocker | Planning state | Execution state |
|---|---|---|---|---|
| RM-1101 | Conditional measurement/data decision plan | RM-211 runtime evidence; retention/overhead to assess | planned | conditional |
| RM-1102 | Conditional performance/error/review journey plan | RM-1101, RM-602; UI delivery model to assess | planned | conditional |
| RM-1103 | Conditional scheduled-result contract plan | RM-1102, RM-308; results/disclosure/retention to assess | planned | conditional |
| RM-1104 | Conditional first-party MCP access decision plan | RM-1101, RM-308; connection/auth/transport contract to assess | planned | conditional |
| RM-1105 | Conditional comparable-measurement plan | RM-1104, RM-1102; representative workload and success target to choose | planned | conditional |

The earlier Jev discussion is now retained separately as RM-1106 (optional
advisory intelligence) and RM-1107 (commercial monitoring research), with its
[rationale and constraints](../product-strategy.md#9-optional-advisory-intelligence-and-hosted-observability).
These are exploratory additions; the console/MCP outcomes above do not depend on them.

| Task ID | Planning depth and reason | Dependency evidence / blocker | Planning state | Execution state |
|---|---|---|---|---|
| RM-1106 | Conditional advisory decision/probe plan | RM-1101; explicit selection, provider/privacy/measurement decisions | planned | conditional |
| RM-1107 | Conditional commercial research plan | Explicit selection and customer/cost evidence; research does not require an advisory provider | planned | conditional |

**Overlap:** RM-211 already owns runtime-event enrichment/exporter coverage;
RM-601/RM-602 own behavioural artifacts and change review; RM-307/RM-308 own durable
jobs and operator controls. Reuse those deliverables. E11 adds longitudinal
measurement, an integrated console, scheduled-run views and first-party MCP
inspection. It does not reimplement the existing CLI/LSP or make an LLM a policy
approver. The MCP adapter idea already appears in developer-tooling.md; this
intake adds the concrete runtime/performance/job outcomes and task IDs.

**Constraints and planning questions:** keep application contracts database- and
provider-agnostic, with SQLite/PostgreSQL adapters first and no Cloudflare tie-in.
Prefer compiler-owned sensible defaults over per-application architectural choices.
Decide console delivery/local-versus-remote access, supported MCP exposure and
identity scopes, history storage/retention/sampling and acceptable overhead during
planning. UI and MCP consume the same versioned evidence; do not expose secrets or
arbitrary internal/customer values as job output. Reuse the existing disclosure
types and revision checks. Connection to an inspection server does not itself
authorise replaying jobs, modifying code or deploying a fix.

**Proposed success checks:** a seeded route/action slowdown is visible across
comparable time windows, with sample counts and source/deployment identity; a user
and an MCP-connected LLM can trace a failed scheduled run to bounded error/output
evidence; a reviewed repair has before/after performance and correctness evidence.
Thresholds, workloads, retention and exact metrics remain planning decisions;
do not claim universal automatic optimisation or store unlimited raw payloads.

**Prior art:** do not record “first language with built-in MCP” as established.
[Tidewave](https://github.com/tidewave-ai/tidewave_phoenix) already provides an MCP
server with runtime tools for Phoenix/Elixir, and
[Ballerina's MCP module](https://central.ballerina.io/ballerina/mcp/latest) supports
MCP servers/clients (sources checked 2026-10-01; the module search listing was
available but its page fetch timed out). These are different integration levels,
not proof of an identical design. Investigate compiler-native semantic context,
shared UI/MCP evidence and verified repair as the potential distinction, without
making an unsupported priority claim. No comparative study is part of this intake.

## Hierarchical application graph plan — 2026-10-04

RM-1108 planning was explicitly selected in the **Assess event-driven
architecture** chat. It retains conditional implementation and the existing
golden priority. [Session requirements and assessed plans](roadmap-assessment.md#syntax-component-messaging-and-graph-intake--2026-10-04)
own the full intake; naming/principal directions are now selected, while full
event/query grammar remains review work. Planning
actual/batch pin is user-confirmed `gpt-6-astra/medium`. No new telemetry runtime,
UI, MCP server or independent review was executed by this session.

| Task ID | Planning depth and reason | Dependency evidence / blocker | Planning state | Execution state |
|---|---|---|---|---|
| RM-1108 | Conditional shared graph/evidence contract and probe | RM-222 syntax, RM-309 component semantics, RM-219 identity, RM-1101 evidence; RM-311 required for real durable fan-out overlays | planned | conditional |

**One model, three evidence layers:** compiler graph is possible dependencies;
execution traces are actual activity; metric windows are sampled/aggregated
activity. Define independent versioned envelopes with common build/logical
identity. UI and MCP consume those contracts instead of independently guessing
topology from logs. Component/operation/coordinated subgraphs reference source,
type/ownership/effect contracts, documentation and edge rationale. Carry
compiler provenance separately from authored rationale; a documented claim is
not a proof. Keep full graph export bounded/paginated, support neighbours/path/
subgraph queries, and make hidden cross-subgraph edges discoverable.

**Instrumentation and production:** the compiler supplies operation/call-site
identity at entry/effect boundaries; the runtime supplies isolated execution
context through nested actions, outbox delivery and joins. No logger needs to
discover a graph ID. Record execution ID, event/delivery/attempt identity,
causation and links to all contributing inputs at fan-in. Keep the event’s
producer-origin manifest/site and each attempt’s actual execution manifest/site
separate from its enrolled handler contract version. A build-A event processed
by build B resolves each endpoint in its own immutable graph; causal links cross
those builds. Each endpoint gets its own optional current-branch mapping.
A single parent trace is insufficient for a join.
Record queued/running/retry/success/failure/cancelled/unknown states, including
background activity after the HTTP response. Use monotonic local durations;
do not subtract arbitrary host wall clocks to invent queue/execution latency.
Durable causal links survive restart even if a live span is missing.

Extend the existing structured redacted telemetry and restricted diagnostics,
not an unrestricted stack/payload dump. Fields need disclosure classification,
tenant/access control and bounded storage. High-cardinality request/event IDs
belong in trace/lookup records, not unlimited metric labels. Logging failure
must not make business commits disappear or stall durable work indefinitely;
bound buffers and record missing/dropped observations. Documentation/source
links are untrusted rendered content with normal access checks, not instructions
an LLM or renderer executes. Declare sampling/expiry/missing mappings visibly.

**Historical/current graph:** consume RM-219's immutable deployed manifest and
source map first. Expose a separately labelled mapping to the selected current
branch; keep changed/deleted/split/ambiguous nodes and behaviour fingerprints
visible. Missing retained artifacts degrade enrichment, not the standard
failure record. Source bundles have a defined retention/access policy outside
disposable build output. A current graph must never quietly replace the original
incident graph during a fix or rollback.

**Delivery slices (after each slice's accepted prerequisites):**

1. Freeze the graph, trace/link and aggregate schemas with RM-1101/RM-219.
   Extend existing semantic/artifact owners; build deterministic seeded evidence
   for a two-subscriber Todo path and an order join. Seeded evidence proves
   consumer contract/UX, not running application instrumentation.
2. Integrate runtime context and causal envelope at route/action/service/delivery
   boundaries. Test concurrent unrelated requests, a restarted subscriber,
   cross-component fan-out and join input links. RM-311 supplies real durable
   states; preserve the existing disclosure/exporter adapters.
3. Extend RM-1102 console navigation and RM-1104 bounded read tools over the same
   schema. A human and LLM can select a subgraph/request, follow its actual path,
   inspect safe evidence and switch explicitly between deployment and branch.
   Reuse RM-1103 scheduled-job views; do not fork a second job console.
4. Seed one slow subscriber, one slow database operation and one failed provider
   outcome. Distinguish queue wait, execution, DB/provider wait and coordinated
   wait, with counts and percentiles rather than only averages. Follow the
   production incident across unrelated edits, rename and two topology-changing
   bugfixes. A proposed repair needs comparable before/after correctness and
   performance evidence under RM-1105; graph proximity alone is not causality.

**Verification/probe plan:** schema compatibility/deterministic artifacts;
identity/branch fixture including A-origin/B-execution and two later fixes;
concurrent context isolation; fan-in/fan-out/retry/
background paths; all four failure disclosure channels; bounded queries,
access denial and expired evidence; two UI/MCP incident journeys. For collection
cost, recommend a future bounded `verify-loop` baseline: compare collection off,
production defaults and dev live detail under equivalent seeded workloads;
measure p50/p95/p99 latency, throughput, RSS, CPU and storage bytes per delivery.
Choose overhead/retention targets from that baseline before tuning or claiming
success. No arbitrary percentage, campaign, Goal or optimisation run is started
by this planning recommendation.

**Open decisions / exit:** RM-1101 resolves storage/sampling/overhead and console
access, RM-219 identity/lineage/retention, RM-309 event/coordination semantics,
RM-222 grammar. Required graph fields and compatibility versions must be frozen
before implementation; real overlay acceptance needs actual delivery evidence.
Split/re-estimate RM-1108 after the first artifact/consumer probe. The whole
feature remains unestimated rather than assigning its contract probe's hours
to instrumentation plus UI/MCP integration. Self-review and docs checks here
do not establish runtime performance, independent review or production readiness.


Design handoff for the graph successor: the
[event model candidate](../event-model.md#10-generated-graph-and-catalogue)
defines static and causal relationships; the
[identity/incident artifact candidate](../generated-artifacts.md#semantic-identity-and-incident-artifact-candidate--2026-10-04)
defines persistent declaration IDs, immutable deployment bundles and explicit
current-branch mapping states. These are review inputs, not a second implemented
schema or evidence that collection overhead and production mapping already pass.


Independent review’s measurement constraints: compare compatible build/behaviour
versions, workload windows and sampling populations. A stable logical ID alone
is insufficient for aggregation across changed behaviour. Preserve counts and
mergeable distributions where required; do not average node percentiles into an
application percentile. Measure serial-lane head-of-line blocking before adding
concurrency overrides. These refine the existing overhead/performance probe,
not a separate monitoring subsystem.
