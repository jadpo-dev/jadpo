# Policy and proof kernel v0.1

**Status:** pre-POLICY-001 candidate retained as migration evidence
**Scope:** historical authorization/proof vocabulary for the golden todo application

The approved [POLICY-001 decision plan](policy-plan.md) supersedes this
document's actor/owner/tenant predicate vocabulary, explicit concealment choice,
and lifecycle-in-policy assumptions. POLICY-P0 must revise the named kernel
rules and machine-readable fixtures around qualified scoped roles,
authoritative direct/membership bindings, compiler-derived entity effects,
narrowing field policy, automatic database scoping, and the complete
input/database/output validation chain before implementation claims conformance.

The conservative judgement outcomes below remain accepted design input; the
older source facts and examples are not current policy syntax authority.

This document defines when the project may say the compiler has proved a policy
property. A property without a named rule and fixture is validation, testing,
generation, convention, operational enforcement, or residual risk—not a
compiler proof.

## 1. Judgement outcomes

Every obligation produces exactly one outcome:

- `proved(rule, facts)` — a named rule derives the required predicate;
- `rejected(counterexample)` — derived behaviour contradicts policy;
- `requires human decision(decision)` — policy is missing or would be weakened;
- `unsupported(capability)` — semantics are understood but unavailable;
- `cannot prove(obligation, missing facts)` — no contradiction is known, but
  the permitted behaviour cannot be derived.

Only `proved` permits release-equivalent compilation. `cannot prove` is never
permission. `requires human decision` remains blocked until an independently
authorised approval is bound to the exact policy and semantic graph. Approval
changes the human-owned premise; it does not turn an unsound derivation into a
proof.

## 2. Terms and facts

The kernel reasons over:

- actors: `authenticated(a)`, `subject(a, s)`, `user(a, u)`, `role(a, r)`,
  `tenant(a, t)`, `strength(a, n)`;
- records: `record(x, E)`, `field(x, f, v)`, `owner(x, a)`, `tenant_of(x, t)`,
  `active(x)`;
- requests: `route(q, r)`, `path_value(q, p, v)`, `input_value(q, f, v)`;
- behaviour: `reads(c, E, predicate, projection)`, `writes(c, E, predicate,
  fields)`, `deletes(c, E, predicate, lifecycle)`, `calls(c, service,
  operation, payload)`, `emits(c, event)`, `returns(c, output)`; and
- policy: `permits(operation, predicate)`, `field_rule`, `lifecycle_rule`,
  `public_exception`, `effect_rule`.

Predicates are closed, typed semantic expressions. Host-language callbacks,
uninterpreted strings, ambient globals, and raw SQL do not introduce proof
facts.

## 3. Trusted fact introduction

Facts may enter a derivation only through:

| Source | Facts introduced | Boundary |
|---|---|---|
| successful authentication | actor identity, allowlisted claims, strategy, strength | generated auth adapter validates provider result |
| actor-to-user resolution | application user identity/status | policy-scoped required query |
| validated request input | typed path/query/body values and supplied-field set | generated boundary validator |
| guarded query result | entity identity plus the exact query predicate | generated parameterised persistence adapter |
| guarded mutation result | affected entity plus exact predicate/fields | generated transactional persistence adapter |
| trusted configuration | typed value, classification, provenance—not secret contents | deployment/startup validator |
| prior proof step | the rule conclusion only | same semantic graph/version |

Tests, comments, agent assertions, generated prose, and successful historical
runs never introduce proof facts.

## 4. Proof obligations

The compiler creates obligations at these semantic nodes:

| Node | Required obligations |
|---|---|
| query | actor access to every possible selected row; permitted projection; boundedness where required; chosen representation meets freshness and revocation policy |
| mutation | actor access to every possible affected row; field write permissions; lifecycle transition; authoritative guard reads; transaction/effect constraints |
| relationship traversal | permission for the source and target rows; policy-preserving join path; bounded cardinality |
| route | inherited or explicit access; input/output closure; transitive read/write/effect/failure conformance |
| action/function call | callee preconditions implied by caller facts; every transitive effect and failure represented |
| service call | operation allowed from caller; egress, secret, payload, retry, timeout, and idempotency rules satisfied |
| job | authority source, schedule, bounds, concurrency, transitive effects, retry/delivery policy |
| output projection | every returned field allowed; no richer record or secret flow |
| policy weakening | independent approval bound to exact before/after policy and semantic graph |

## 5. Normative rules

### AUTH-EXACTLY-ONE

If exactly one credential is presented in the configured cookie/bearer slots,
its selected strategy validates, and its mapped subject produces exactly one
active authoritative user or service principal when authority is required,
derive `authenticated(p)`. Zero credentials yields an authentication rejection.
Multiple credentials, duplicate values, multiple identities, ambiguity, or an
invalid credential beside a valid credential rejects. Claims, roles,
permissions, and strength are never unioned across credentials or strategies.
Immediate mode performs authority resolution on every request; bounded mode
may defer it only until the declared maximum delay, and a fresh-authority route
always resolves authority before application behavior.

### ROUTE-DEFAULT-AUTH

Every route requires `authenticated(a)` unless its policy contains a matching
`public_exception(method, path, output, effects)`. Source omission cannot derive
public access.

### QUERY-SCOPE

For query predicate `Q(x)` and policy predicate `P(a, x)`, read access is proved
only when the kernel can establish `Q(x) -> P(a, x)` for every selected row.
Checking `P` after an unscoped read does not satisfy the rule.

### MUTATION-SCOPE

For mutation predicate `Q(x)` and policy predicate `P(a, x)`, mutation access
is proved only when `Q(x) -> P(a, x)` and every written field is allowed by the
field policy. A later ownership check cannot authorise an earlier write.

### CREATE-FORCED-FIELDS

Creation is proved when policy permits creation for the actor and every
human-owned, server-owned, immutable, lifecycle, and derived field is either
forced by trusted facts or absent from client input. Merely validating a client
supplied owner identifier is insufficient.

### PATCH-SUPPLIED-FIELDS

An omission-aware patch creates one field obligation for each supplied field.
Omitted fields create no write. Supplying `none` is a write of absence and must
be permitted by both type and field policy. A dynamic field set not represented
in the semantic graph is `cannot prove`.

### RELATIONSHIP-COMPOSE

A traversal from `S` to `T` is proved when the source read is proved, the
relationship identity/cardinality is declared, the join preserves the source
policy facts, every target row satisfies the target policy, and required bounds
are present. Source permission never automatically implies target permission.

### PROJECTION-SUBSET

Returning output `O` is proved when every serialized field is declared in `O`,
each value has the exact or permitted nominal field type, policy permits each
field, and no secret-classified value can flow to the serializer. Returning a
richer entity and relying on JSON conventions is rejected.

### FAILURE-CONCEAL

An unauthorised row may share a public not-found response only when policy names
the concealment equivalence and all observable status, code, body shape, and
timing-class requirements are compatible. Internal telemetry retains the
distinct cause without disclosing it publicly.

### LIFECYCLE-TRANSITION

A delete or state transition is proved only when policy names the transition,
dependent relationship behaviour, retained data, later visibility, restoration
rule, migration effect, and eventual purge behaviour. Missing dependent-data
semantics requires a human decision.

### EXTERNAL-EFFECT

A service call is proved permitted when policy names the caller and operation,
the destination matches declared egress, every secret flows only to a declared
credential slot, payload fields satisfy their output/egress policy, and timeout,
retry, and idempotency obligations are met. Permission to call one operation
does not generalise to its host or service.

### CALL-COMPOSE

A caller is conformant only when it proves every callee precondition and its
declared policy includes every transitive read, write, lifecycle transition,
external effect, emission, and public failure. Effects cannot disappear behind
a function or helper boundary.

### PUBLIC-WEAKENING

A change that expands an actor set, record set, field set, lifecycle power,
public surface, secret sink, egress destination, or escape-hatch authority
requires independent approval. Without a valid bound approval, the outcome is
`requires human decision`, never `proved`.

## 6. Composition and conservative limits

The kernel supports conjunction, equality over nominal identifiers and enum
variants, finite role/capability membership, monotonic range refinement, and
declared relationship substitution. Disjunction, negation, optional traversal,
time-dependent predicates, custom functions in policy, and provider-specific
claims are supported only when a named rule gives them semantics. Otherwise the
result is `unsupported` or `cannot prove`.

Runtime validation can uphold a boundary assumption but cannot rescue an
unproved authorization implication. Generated tests can find integration bugs
but cannot establish universal row-level conformance.

## 7. Minimal fixture matrix

| Rule | Positive | Negative | Indeterminate |
|---|---|---|---|
| `AUTH-EXACTLY-ONE` | one valid session | session Alice + OIDC Bob | unknown custom claim mapper |
| `ROUTE-DEFAULT-AUTH` | protected route omission | undeclared public bypass | generated target lacks auth adapter |
| `QUERY-SCOPE` | `owner_id == actor.user_id` | load by ID then check owner | helper predicate with no kernel semantics |
| `MUTATION-SCOPE` | owner in update predicate | update all then filter return | dynamic predicate callback |
| `CREATE-FORCED-FIELDS` | owner forced from actor | client supplies owner | ownership derived by unsupported service |
| `PATCH-SUPPLIED-FIELDS` | permitted title supplied | client supplies owner | reflective patch map |
| `RELATIONSHIP-COMPOSE` | self user -> owned todos | user -> all todos | optional nested path without rule |
| `PROJECTION-SUBSET` | explicit `TodoView` | return `Todo` entity | custom serializer |
| `FAILURE-CONCEAL` | approved owner/not-found alias | public forbidden vs missing detail | unbounded timing side channel |
| `LIFECYCLE-TRANSITION` | approved soft delete + purge | hard delete with dependants unspecified | provider retention outside contract |
| `EXTERNAL-EFFECT` | mail operation with injected key | token in payload/output | dynamic destination |
| `CALL-COMPOSE` | declared transitive mail effect | pure function calls service | opaque escape hatch |
| `PUBLIC-WEAKENING` | exact independent attestation | agent-edited approval file | unavailable approval provider |

Each cell becomes an executable compiler fixture before the corresponding rule
is marked implemented. The candidate machine-readable cases live in
[`tests/assurance/policy-proof-v0.1.json`](../tests/assurance/policy-proof-v0.1.json);
the P10R verifier requires all three polarities for every named rule.

## 8. Claims and non-claims

The intended soundness claim is narrow: for supported constructs, release-
equivalent compilation does not accept behaviour that exceeds the frozen policy
under these rules, assuming the trusted computing base behaves as specified.

This kernel does not prove that policy is wise, business logic is correct,
providers honour contracts, the compiler/runtime is bug-free, timing reveals
nothing, deployment is uncompromised, or an authorised human made a good
decision. Those are threat-model and residual-risk concerns.
