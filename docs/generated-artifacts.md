# Generated artifact contract

**Status:** accepted for the P8/P9 prototype  
**Schema version:** 1

`inventory/routes.json` has its own schema version, currently 2, because each
route now records its success mode and HTTP status.

`jadpo artifacts <project>` checks the complete frontend before writing
anything. A valid project produces exactly these compiler-owned files:

```text
build/
  app.meta.json
  inventory/
    routes.json
    callables.json
  audit/
    failures.json
    entities.json
    transactions.json
    configuration.json
    policy.json
  approval/
    subject.json
    subject.txt
  validators/
    plan.json
  compatibility/
    public-failure-codes.json
  openapi/
    openapi.json
```

`build/` is disposable output. Authored source, tests, configuration, secrets,
and deployment material never belong beneath it. Source discovery does not
traverse it, even if a target generator accidentally creates a file ending in
`.jadpo`.

## File responsibilities

- `app.meta.json` is the checked semantic manifest: stable nodes and source
  ranges, refinement and call edges, inferred expression types, failure
  contracts, and derived route failures. Its nested checked manifest is schema v2:
  a failure without a shared HTTP default has `http_status: null`. Source paths are project-relative so
  absolute and relative invocations produce identical bytes.
- `inventory/routes.json` schema version 2 records method, path,
  explicit/default authentication, boundary types, selected success mode and
  status, invoked callable, and derived public failures.
- `inventory/callables.json` records callable kind, nominal parameters, output,
  and declared closed failure set.
- `audit/jobs.json` is conditional on checked scheduled jobs. Its schema v1
  records the exact action/nominal clock binding, constructor-validation proof,
  the exact checked integer millisecond interval, derived failures and static
  reachable external service effects. It explicitly
  marks durable failure dispositions/profile and runtime lowering unavailable;
  jobs are not ordinary callable or HTTP inventories. Frontend artifact generation
  is not permission to execute a job: build/target fails closed until those gates
  are implemented. See the [runtime boundary](runtime-target-v0.1.md#checked-scheduled-job-frontend-rm-306).
  The local approval subject also includes these checked nonexecuting facts in
  `impact.jobs`, sorted by job identity, with explicit empty-job absence and
  unestablished worker authority/runtime evidence. It is not an attestation.
- `audit/failures.json` schema v2 makes public versus internal disclosure visible,
  retains null for failure kinds without a shared HTTP default, and records the
  separate Bun operational envelopes. Route inventory and OpenAPI expose those
  same envelopes; internal context cannot reach the client.
- `approval/subject.json` is the versioned, canonical local approval subject.
  Schema v5 includes source/state/behavior/policy/graph SHA-256 digests, checked registry
  provenance and a build-time compiler source-input manifest, checked facts,
  individual decisions, checked actor binding/principal projection contracts,
  source-condition-to-route may-call links, checked field/output source contracts
  with route witnesses, fresh expected metadata/audit/target byte pins (including
  SQL/authentication/dependencies, with explicit approval self-exclusion and
  unsupported-target disposition), distinct effectless admission rows and
  explicitly unsupported analysis. Live authorization and feasibility are not
  established by those links; byte pins do not establish written-file/runtime
  conformance or compiler executable/deployment authority. Default builds have
  no before baseline or supplied intent. It does not authorize release; see the
  [approval protocol](approval-protocol.md#2-canonical-approval-subject) for
  canonical bytes, limitations and the `jadpo approval` comparison command.
- `approval/subject.txt` is a non-graphical export containing the same subject
  JSON and digest for accessibility, CI logs and archival review.
- `validators/plan.json` records named constraints, closed record shapes,
  optionality/nullability, and route boundary validation obligations. It is a
  plan for P9 generation, not executable validation code.
- `compatibility/public-failure-codes.json` is the current public failure
  contract snapshot and names the fields whose change is breaking. Until a
  versioned baseline is configured, its status is
  `baseline_not_configured`; it does not pretend to have performed a diff.
- `openapi/openapi.json` is the OpenAPI 3.1 subset derivable from the current
  grammar: request and success schemas, closed record shapes, scalar
  constraints, and reachable failure responses. It deliberately omits features
  not yet expressible in source.

Every JSON file ends with one newline. Arrays and object members derived from
semantic sets use canonical ordering. Repeating the command with the same
compiler and authored inputs is byte-identical. Output contains no timestamp,
machine path, random identifier, or environment-specific value.

The compatibility snapshot is not a stored baseline. A later versioning design
must put reviewed baselines outside `build/`, because disposable output cannot
be authoritative input to its own next generation.

## Semantic identity and incident artifact candidate — 2026-10-04

**RM-219/RM-1108 design candidate; not an implemented output-tree change.**
Current semantic nodes use deterministic build-local `NodeId(u32)` ordinals,
source names/ranges and a checked source revision. That is sufficient for one
checked manifest, not rename/branch-stable logical identity. Persistent schema
IDs already exist in [schema.identities.json](migration-identity-v0.1.md).

### Three distinct identities

| Identity | Candidate representation and purpose |
|---|---|
| Logical declaration | App identity plus immutable registered declaration identity; survives explicit rename/move |
| Exact build | Canonical manifest digest over checked source, compiler/target identity, graph/schema/contracts and relevant declared build inputs |
| Build-local site | Node/expression/call-site key within that manifest, with original source file/range and generated target mapping |

Deployment ID and runtime execution/attempt IDs are attached at runtime, outside
deterministic compiler output. A Git commit alone is insufficient for a dirty
build; include checked source-content identity. Do not hash secret values into
public fingerprints. A matching logical ID does not prove matching behaviour;
even matching build bytes do not make different inputs/environment equivalent.

### Registry lifecycle

Propose a versioned compiler-owned `semantic.identities.json` outside `build/`,
covering component, route, callable, event family/variant, subscriber/handler and
other non-schema declarations used by inspection. Reuse existing schema IDs for
schema-owned entities/fields rather than maintaining a competing identity.

Fresh logical IDs are allocated once and stored as reviewed source inputs.
Canonical output remains deterministic for fixed checked inputs, even if the
initial allocation uses a unique identifier. Scaffold/tooling can create and
update the registry; authors do not supply per-handler IDs. `check`/artifact
validation must not secretly invent rename mappings or rewrite history.

Use the schema registry's guarded init/add/explicit-rename/remove discipline.
Adding an unambiguous declaration can get a generated entry; missing old entries
plus new declarations cannot silently become a rename. Offer a focused compiler
repair/code action. Exact CLI spelling is implementation planning, not a new
command claimed to work here.

Entries retain kind, owner, current qualified path, old paths and tombstone or
lineage references. Enforce unique IDs/current paths and checked owner/kind
compatibility. A copy gets a new ID. An explicit move/rename retains the ID where
the logical role is preserved; owner/effect changes still affect review. A
split/merge produces explicit lineage and possibly multiple new identities,
never an invented one-to-one continuity. Deleted IDs remain tombstoned and cannot
be reused for a later declaration with the same name.

Independent branches that allocate the same source path differently, duplicate
an ID or disagree about ownership require conflict resolution. Neither path
similarity nor a matching content hash authorises automatic coalescing. Retain
both branches' deployed artifacts regardless of the eventual source resolution.

### Candidate retained bundle

Extend the versioned checked manifest rather than introduce a second semantic
truth. A bundle can contain:

```text
manifest.json              # digests, compiler/target and input identities
app.meta.json              # existing checked semantics, extended/versioned
graph.json                 # hierarchical graph projection from that semantics
source-map.json            # exact build-local target-to-source/site mappings
sources/                   # authorised source snapshot, access-controlled
```

These are proposed retained bundle entries, not additions to the current output
contract above. CI/deployment publishes immutable digest-addressed bundles outside
disposable `build/`; runtime logs carry their reference. Publication/retention
needs an explicit operational adapter contract. No SaaS is required. Executable
old handler retention is a separate deployment obligation from this diagnostic
bundle. Restrict sources/stacks independently of safe ordinary telemetry.

Graph nodes carry logical ID, kind, owner, build-local source reference,
documentation/rationale references and contract/behaviour fingerprints. Edges
carry source/target IDs, kind and declaration provenance. An aggregated local
call edge may have many build-local call sites. Event subscription edges derive
from event-variant/handler IDs, so adding an unrelated node does not renumber
existing edges. Arbitrary expression sites remain build-local; do not promise
every line has stable identity across edits.

### Incident lookup and branch mapping

1. Resolve the incident's exact manifest digest and validate integrity/schema.
   Use its original source and target map; never substitute checkout HEAD.
2. Locate the logical operation plus original call site and causal execution
   path. For a delayed or rolling-deployment delivery, resolve the producer endpoint
   using its immutable origin manifest/site and each processing attempt using its
   actual execution manifest/site. Enrollment’s selected handler contract version
   does not identify the executable build that ran it. Cross-build causal links
   connect these views; never project all frames onto one convenient graph.
   Compiler-generated frames map to a named compiler/runtime component if no
   authored source site exists.
3. Optionally resolve the logical ID against the selected current registry.
   Return an explicit mapping status: same-version, changed, renamed/moved,
   deleted, split/merged, ambiguous or unavailable. Compare version/fingerprint
   separately; a rename and behaviour change may both apply.
4. Keep the historical view immutable while applying/fixing code in the current
   branch. Every later fix refreshes only the current mapping.

If the bundle expired or is unavailable, display the safe recorded name/code,
original source reference, build identity and causal IDs with missing enrichment
marked. Never discard or rewrite the incident to match a current graph. Privacy
policy governs which user/LLM can retrieve each layer. Retention must cover the
declared support/rollback horizon and must not remove a bundle still required by
retained incidents without an explicit disposition. Numeric storage/retention
defaults remain an E11 baseline decision; no permanent infinite source retention
is assumed.

### Planned proof matrix

| Case | Required observation |
|---|---|
| Same source/registry/compiler, relative versus absolute invocation | Canonical output identical; no machine/time/runtime IDs leak into build bytes |
| Unrelated declaration insertion | Existing logical IDs and subscription relationships preserved |
| Explicit rename/move | Original incident still resolves; current path differs with continuity recorded |
| Copy; delete then recreate same name | New logical identity; old incident cannot attach to the new declaration |
| Divergent branches and conflicting registry edits | Exact deployed views work independently; current ambiguity is explicit |
| Split/merge | Lineage visible without fabricated one-to-one source mapping |
| Two topology-changing fixes | Both current mappings update; original incident bundle unchanged |
| Build-A origin, build-B compatible handler execution, then two fixes | Emission resolves in A, failing attempt in B; causal endpoints retain their own manifests and current mappings without rewriting either |
| Stale/tampered/missing artifact | Wrong artifact rejected, missing enrichment labelled; standard log remains useful |
| Concurrent requests/fan-out/join | Runtime context is isolated and includes multiple causal links where required |

Implement registry/manifest/source-map probes before UI polish. These cases are
planned evidence only. Public metadata compatibility and source-retention access
need review before schema freeze. The
[E11 plan](archive/work-plans/developer-console-mcp.md#hierarchical-application-graph-plan--2026-10-04)
owns live inspection and overhead validation.
