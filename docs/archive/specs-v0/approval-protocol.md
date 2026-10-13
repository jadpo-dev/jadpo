# Human policy approval protocol

**Status:** candidate P10R protocol  
**Security property:** separation of implementation and policy authority

## 1. Release rule

A policy weakening cannot produce release-equivalent success unless a protected
CI/review system supplies a valid attestation. An ordinary repository file,
commit signature, agent assertion, local environment variable, or generated
test cannot serve as that attestation.

Local builds may inspect and render an unapproved change. Their result must be
visibly `non-releasable` and use a distinct exit/status artifact. There is no
“temporary” success mode that CI can mistake for an approved release.

## 2. Canonical approval subject

The compiler must emit a canonical `ApprovalSubject` containing:

- policy schema and compiler versions;
- before/after policy digests;
- before/after semantic-graph digests;
- a stable list of individual human decisions;
- affected actors, routes, entities, fields, relationships, lifecycles,
  services, secrets, jobs, configuration, and deployment surfaces;
- direct and transitive reads, writes, emissions, external effects,
  transactions, retries/idempotency, and failure/disclosure changes;
- newly possible counterexamples;
- evidence classifications and unresolved uncertainty; and
- the exact source/artifact provenance being considered.

Canonicalization sorts by stable semantic identity, includes absence explicitly,
normalises text and numeric encodings, and is versioned. The attestation binds
to the digest of the canonical bytes, not a UI rendering.

The compiler emits a **partial local behavioral subject v5** at
`build/approval/subject.json`. It extracts checked declaration tokens (including
body values), separate entity grants, operation policy, data/transaction/failure
contracts and conservative transitive effect paths. Every unsupported category
has an explicit status and `null` facts rather than an empty assertion of safety.
Generated authentication is reported as an opaque boundary. One shortest path
per reachable terminal effect is shown; the full call-edge set retains alternate
paths and cycles. Conditional reachability does not prove execution.
The canonical `scenarios` list compares before/after HTTP-route effect paths and
marks newly reachable, no-longer-reachable, witness-changed, and after-only
effects. If a route's authentication mode changes while its effect path stays
the same, v5 retains a scenario and records whether unauthenticated callers are
newly admitted or excluded. This is an actor-scope candidate derived from the
checked route contract, not a feasible counterexample. Each scenario also
retains before/after route policy obligations and explicit added/removed actor-
subject deltas, including restricted field-read constraints when present. These
are exact compiler-checked policy subjects; live role membership and principal-specific
authorization remain unresolved. Effect scenarios remain
`compiler_checked_may_call` summaries. Routes with no terminal effect instead
receive a distinct `kind: route_admission`, `effect: null` row for addition,
removal, authentication or actor-source contract changes, or after-only inspection.
These rows use a separate stable identity domain and
`compiler_checked_route_contract` certainty; they do not invent a data/service
effect or execution witness. Before/after actor-source contracts are retained in
both categories. Execution and input/data feasibility are not proved, and
concrete counterexamples are not generated.

For comparison, `jadpo approval <project> --before <source-project>` checks both
source states afresh. `--expected-before sha256:...` pins the baseline's
`source_digest`; a mismatch fails. No previous artifact is accepted as trusted
source or issuer. `--intent <text-file>` includes supplied, unverified human
intent; omission is explicit. The command prints JSON without overwriting a
build; `--text` emits the complete subject with an explanatory heading. Local
inspection status is always `local_inspection_non_releasable`.

Prefer `--expected-before-state sha256:...` when resuming a review. It pins the
baseline's `state_digest`, including source, registry, compiler-input provenance
and extracted facts. The state digest hashes the complete snapshot before adding
its own `state_digest` field. Both pin options require a supplied baseline; they
may be used together. The older source-only option deliberately does not pin a
registry or compiler change. Baselines are rechecked with the current compiler,
not represented as historical execution by an earlier compiler.

Each snapshot records the exact bytes and parsed facts of a present, validated
`schema.identities.json`, or explicit absence. Validation and hashing use the
same captured bytes. Invalid or unreadable registries fail inspection. Registry
changes appear as a separate decision even when source is unchanged. Compiler
provenance includes a build-time SHA-256 manifest of workspace manifests, lockfile,
compiler crate sources/build scripts, bundled runtime code and language data.
It contains relative paths and no timestamps or Git metadata. This is source-input
provenance, not a toolchain, executable or protected-build attestation; those
limits remain explicit. Authentication details are structured JSON facts.

Canonicalization `jadpo.approval-subject.v5` uses recursively sorted object keys,
compact UTF-8 JSON and no final newline in the hashed bytes. Set-like collections
are sorted; ordered syntax tokens and witness paths retain their order. Generated
numbers are integers; authored literal spellings are strings, never floating
point conversions. Unicode and authored literal spelling are preserved exactly,
not conflated by a normalization rule. SHA-256 from RustCrypto's `sha2` binds the
canonical object; source manifests separately hash exact file bytes and relative
names. Text headings and the output file's trailing newline are outside the digest.
FNV elsewhere in the compiler remains correlation-only.

Each changed fact gets a stable decision ID derived from its category/name.
Changes include exact before/after values, conservative review reasons and
retain/accept/clarify choices, with no approval attached. A token value change
cannot hide behind unchanged graph topology. Renames are removal/addition;
general rename-stable identities remain RM-219. An unchanged source rebuild at a
different checkout path is identical; changed source bytes or names invalidate
exact binding even when behavior is equivalent. Rebase metadata alone is not an
input. This is deliberately conservative, not a semantic equivalence proof.

RM-601 now includes the pinned SERVICE-001 contract and source-derived
transitive mail-service effects in route impact, with an omission-conformance
test that rejects a rehashed export after that effect is removed. The artifact
keeps service runtime conformance explicitly unestablished. Lifecycle contracts
are also compiler-checked. RM-601 v5 surfaces unauthenticated caller changes
and exact actor-subject deltas even when effect paths are unchanged. Its reserved
`impact.jobs` slot now records the checked nonexecuting scheduled-job audit,
sorted by semantic job identity: exact integer interval, action/nominal clock
constructor, declared failure contract and static reachable external effects.
Before/after changes become separate decisions even when graph topology does
not change; rehashing a rendered export cannot hide omitted job/effect facts
from comparison with the fresh compiler subject. Zero checked jobs is explicit
absence, while emissions/deployment remain unsupported with null facts.
Worker authority, durable delivery/revision/profile bindings, feasible job
scenarios and runtime conformance remain explicitly unresolved. These facts
do not grant execution or release authority. The v5 slot/canonicalization is
unchanged, and build-time compiler-input provenance binds the extraction change.
The reserved `impact.actors` object also exposes sorted checked source role-binding
and membership contracts, retaining its existing inventory/facts shape. Each
`role_binding:Entity.field` and `membership:Entity` has an individual before/after
decision, including removal and explicit absence. `principal_entity` names the
checked reference entity, not an authenticated principal kind. The evidence label
is `compiler_checked_contract_not_live_authority`: these source contracts were
already included in the aggregate policy audit; the new facets improve review
granularity without establishing live role assignments, principal-specific access
or input/data feasibility. They do not grant runtime authority.

`impact.actors.principal_contracts` derives checked application principal and
revocation, principal variants/field types, authentication transports, validator
principal selections/settings/credential slots, claims and authority resolutions.
Resolutions retain the exact active-predicate tokens, source-to-principal field
mappings and inactive failure. These contracts and each target-keyed projection
receive separate decisions. Semantic sets are sorted; expression/literal spelling
is retained without trivia or source offsets. Settings contain declared references
or source literals, never resolved runtime configuration/credential values. Changing
a mapping's source preserves its target identity; changing its target is removal/
addition. Checked empty lists mean absence, not missing analysis. These remain
source contracts, not actual authenticated users, effective memberships, adapter
runtime conformance or feasible counterexamples; generated authentication stays
an opaque boundary.

`impact.actor_sources` links every reachable checked policy operation to HTTP
entry points through semantic call endpoints. One deterministic shortest witness
is retained per operation; the full semantic graph still retains alternate paths
and cycles. Original obligation and restricted-field-read provenance are preserved
separately from copied route aggregates. Subject alternatives within a surface
are OR; independent surfaces and reachable operations are conjunctive requirements,
not a union of granted powers. Distinct copied OR groups have group-aware stable
surface/link identities and remain separate AND requirements; only truly identical
groups are deduplicated. Built-in public access does not override route
authentication, and authenticated access is not restricted to a user principal.
Qualified roles retain exact direct-role/resource-scope and membership enum/
resource-or-application-scope candidates, with symbolic identity/key/current-row
conditions. A binding is not itself an effect grant. Nominal principal fields
that explicitly reference a checked entity identity are distinct from authority
resolution candidates; nonidentity projections are labelled `not_present` for
identity composition. Plain UUID fields, `id`/`*_id` spellings and same-entity
nonidentity mappings are not treated as proof of identity. Source binding/member
values also retain their exact declared type and effective
reference path, with explicit reference precedence and checked self-identity
handling. Source-value identity composition is separate from principal-side
identity candidates: nonidentity references are labelled `not_identity_reference`
and a missing checked identity is `unresolved`. Resource membership scope
references are retained separately; application scope has no value reference.
Equality and current-row conditions remain required and unproved. Authenticated entry
points retain checked authentication candidate contracts even without a policy
surface; public entries do not acquire those candidates solely from global auth
declarations. Route validator selection, live authority and feasibility remain
unresolved. These are static source conditions and may-call links, not effective
permission, runtime conformance, complete field/output analysis or release authority.

`impact.field_source_contracts` preserves the existing `impact.fields` inventory
and adds checked declaration, expression and route contracts. Its catalogue covers
Entity/Value/Input/Output fields, enum payload fields and scalar aliases. Flattened
callable owners retain parameters, result/failures, receiver, guard, freshness and
consistency. Body contracts retain predicates, literal/derived/plain-patch field
mappings, symbolic branches/failure triggers, constructor target fields, whole-value
return boundaries, page order/cursor tuples and actual include bounds. Nullable
patch assignments use the checked inherited-nullability catalogue; omission is
not a null assignment. Missing/conflicting expression observations remain explicit,
and do not erase declared page contracts or become empty write sets.

Compiler-created inline Object and route header/path display names are normalized
through their exact checked source context, not prefix heuristics. Authored names
and tokens remain unchanged. Tuple IDs include the owning operation and structural
syntax address; statement ordinals are build-local, not insertion-stable identities.
Semantic call endpoints supply deterministic route may-call witnesses and explicit
unreachable owners. Individual field/source/route facts receive comparison
decisions. Pure response changes do not create actor-admission or terminal-effect
scenarios. Nominal references and observed type displays never prove database
origins: aliases, whole-value/call boundaries, path feasibility and runtime
conformance remain unresolved. This is an additive source-contract slice, not
complete field-flow/taint analysis or approval authority.

`provenance.generated_artifacts` binds fresh expected generator bytes: sorted
path/producer/byte-length/SHA-256 descriptors for metadata/audits and supported
target outputs, including emitted SQL, authentication and dependency files.
Descriptors use the same producers as the build, not a pre-existing build directory.
The approval JSON/text are explicitly excluded to avoid self-reference. Each output
pin and the target-generation disposition are facts with individual comparison
decisions; the complete descriptor set is bound into snapshot/behavior digests.
Unsupported target generation records its diagnostic code/rule and no target files,
distinct from successful generation. Known metadata source paths are normalized by
the actual producer before byte hashing; authored values/import paths are unchanged.
These are expected bytes, not written-file conformance, toolchain/executable
attestation, runtime proof, production deployment qualification or release authority.

RM-601 remains open for complete durable-job/emission integration, live role/principal membership
resolution and concrete input/data-feasible
counterexample analysis, trusted compiler/build provenance, and independent
artifact review. SHA-256 provides binding, not authority;
[source-derived route-effect and omission regression](../jadpo/crates/core/tests/validation_approval_subject.rs)
proves contract/path inclusion and detects a rehashed export that omits the effect.
RM-603/RM-604 still own issuer validation and release enforcement. No runtime,
operational, test-pass or policy-weakening proof is inferred from extraction.

## 3. Decision granularity

Each weakening is a separate decision. A reviewer chooses the narrowest option
that expresses intent. Blanket “approve all changes” is not accepted when the
compiler can separate decisions.

An approval record contains:

- subject digest and decision identifier;
- authenticated reviewer identity and authority/role;
- decision (`approve`, `reject`, or `request clarification`);
- selected policy alternative and its exact delta;
- required rationale;
- issue/change identifier;
- issued-at and optional expiry;
- superseded approval identifier, when applicable; and
- review-system attestation identity.

The implementation agent identity may never satisfy the reviewer-authority
predicate for the same change.

## 4. Validation

CI rebuilds the subject from a clean checkout and verifies every decision. It
rejects an approval that is missing, expired, revoked, superseded, self-issued,
issued by an unauthorised reviewer, created for another compiler/policy schema,
bound to another graph/policy digest, incomplete for the current decision set,
or already consumed where the policy requires one-time use.

The initial implementation should use a protected code-review/CI attestation
provider with authenticated identities and branch/environment protection. The
project does not invent its own cryptography.

## 5. Review surface

The primary surface is an intent-first behavioural review, not a shortened code
diff. For each decision it presents:

1. the original request and why a human decision is necessary;
2. current versus proposed behaviour in domain language;
3. actor- and scenario-based “what becomes possible” examples;
4. direct and transitive semantic-graph paths;
5. reads, writes, effects, lifecycle, disclosure, configuration, and deployment
   impact;
6. proved, runtime-validated, tested, operational, assumed, unsupported, and
   uncertain claims as separate categories;
7. counterexamples and alternatives; and
8. the narrow choices available and exact policy delta of each.

Reviewers can drill into source, proof obligations, tests, runtime boundaries,
and generated target code without losing the decision context. A stable
non-graphical export contains the same facts for CI, accessibility, and archive.

The web UI consumes versioned jadpo artifacts. It may not infer policy or
semantic meaning independently.

## 6. Required adversarial cases

| ID | Attempt | Required result |
|---|---|---|
| AP-01 | agent adds an ordinary `approval.json` | reject: untrusted issuer |
| AP-02 | agent copies a valid approval from another change | reject: subject digest mismatch |
| AP-03 | policy unchanged but semantic graph differs | reject: graph digest mismatch |
| AP-04 | graph unchanged but policy differs | reject: policy digest mismatch |
| AP-05 | same reviewer is the implementation agent | reject: separation-of-authority failure |
| AP-06 | reviewer lacks policy authority | reject: authority mismatch |
| AP-07 | approval expired or was revoked | reject |
| AP-08 | one of two policy decisions is approved | reject release; show remaining decision |
| AP-09 | compiler/schema version changes interpretation | reject; require re-render/review |
| AP-10 | source is rebased without semantic change | accept only if canonical subject digest remains exact |
| AP-11 | UI summary omits a transitive external effect | fail review-surface conformance test |
| AP-12 | protected attestation provider unavailable | fail closed for release, permit local inspection only |

The same cases are frozen in machine-readable form at
[`tests/assurance/approval-protocol-v0.1.json`](../tests/assurance/approval-protocol-v0.1.json).
They are candidate expectations until the approval gate exists; documentation
alone is not implementation evidence.

## 7. Comprehension experiment

P10R compares a normal pull-request diff, a concise behavioural diff, and the
relationship/effect-aware decision view. Counterbalance order and measure
correct answers, critical risks noticed, false confidence, review time,
clarification requests, and approval quality. Include locally reasonable diffs
whose transitive behaviour is unsafe.

The protocol fails if identity mechanics are secure but reviewers cannot
reliably understand the decision. A target of at least 80% correct frozen
behaviour/policy/effect/lifecycle answers applies; critical misunderstandings
are reported individually rather than hidden in an average.
