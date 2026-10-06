# Decision register

**Status:** current design authority  
**Last consolidated:** 2026-10-02

This register separates decisions from attractive ideas. Changing an accepted
item should update the charter, affected specifications, and examples.

## 1. Accepted decisions

### Golden reminder execution and lease profile — 2026-10-06

The owner approved a **60-second execution limit and a renewable 40-second
per-delivery claim lease** for the golden reminder worker (RM-108). This is a
golden-only profile, not a universal job default or measured hard runtime bound. Renewal
uses authoritative database time and the current claim generation, never
extends the execution deadline, and cannot revive an expired or reclaimed claim.
The singleton selection activation has a separate bounded page profile; these
values do not authorise 500 sequential provider calls within one execution.
The existing three scheduled invocations within one hour, provider budget of
three attempts within thirty seconds, and no automatic resend after uncertainty
remain unchanged.

This resolves the profile choice in the [owning delivery plan](work-plans/golden-delivery-planning.md#checkpoint).
Checked worker lowering, scheduler/recovery evidence on both adapters and
required independent reviews still govern activation. The frozen ASYNC-001
contract and golden acceptance source are preserved.

### Completion projection after origin revocation — 2026-10-05

The owner selected continuing narrowly scoped, compiler-controlled completion-view
maintenance for an already committed Todo change after its original actor loses
access. The worker must remain authorised; verified source scope/incarnation,
lifecycle, visibility and disclosure checks still apply. This prevents revocation
alone from leaving an otherwise eligible derived view stale.

This selects the [bounded projection direction](event-model.md#first-system-reaction-protocol-completion-projection),
not a generic system capability or concrete policy grant. Queued user requests,
provider effects and authored projection writes gain no exception. Exact checked
declaration/IR mapping and protected approval binding, incarnation/recovery proofs
and existing golden/contract gates remain prerequisites. Initial view coverage is
partial; this decision grants no bootstrap/rebuild authority or complete-view claim.
Retaining the origin check and requiring separate recovery was considered and not
selected for this exact maintenance. Independent candidate corrections are recorded
in the [review evidence](../tests/validation/event-design-independent-review.json).

### Naming and principal direction — 2026-10-04

The owner explicitly locked in `UpperCamelCase` for named types/declarative
contracts and `lower_snake_case` for runtime values, fields, parameters, variants
and operations. Preserve the existing declaration family, including uppercase
service/application names and current failure/configuration forms. Keep general
`type Name = Base { ... }` syntax alongside specialised keyword declarations.
Uniform lowercase and lowerCamelCase migration are not selected. This reaffirms
the existing naming contract rather than introducing another casing scheme.

The owner also selected an unnamed singleton `principal { ... }` block, with
compiler-defined `Principal` and runtime `current_principal`, eliminating the
redundant authored name/selector. Current authentication invariants remain;
arbitrary construction cannot create trusted identity. This successor syntax
is selected for RM-222/RM-223 review/migration, not implemented or evidence of
new compiler conformance. Preserve frozen current grammar and golden sources.
The [owning naming addendum](naming-and-qualification.md#10-naming-reaffirmation-and-principal-simplification--2026-10-04)
records rationale, rejected alternatives and exact scope.

### Delimited query operation direction — 2026-10-04

The owner selected one query operation with details in its block:
`attempt query(Todo) { cardinality: required ... }`. Keep existing
`required`/`optional`/`many` semantics, typed failure bindings, checked predicates,
freshness, policy and bounded includes/pagination. Named queries remain ordinary
calls such as `attempt Todo.by_id(input.id)`. The alternative
`query_required`/`query_optional`/`query_many` intrinsic family is not selected.
Semicolons are not required. The
[successor syntax inventory](syntax.md#22-successor-syntax-review--2026-10-04)
and [candidate grammar delta](grammar-v0.1.md#18-successor-grammar-candidate--2026-10-04)
own details. This is a selected source direction, not compiler implementation or
an independent public-language review; RM-222/RM-223 retain those gates.

### Jadpo name and canonical domain — 2026-09-25

The owner adopted **Jadpo** and **jadpo.dev**, with `.jadpo` source files and the
`jadpo` CLI. This supersedes the earlier advice to postpone naming; it does not
establish the product thesis. The “Rename language to Jadpo” chat reports the
repository migration and verification complete; domain ownership/DNS/hosting
remain external evidence under RM-1005. Buying `jadpo.com` was an optional later
possibility, never a prerequisite. See the [source reconciliation](conversation-coverage.md#4-roadmap-reconciliation--2026-10-01).

### Delivery defaults and planning directions — 2026-10-01

The owner selected JSON cursor transport with efficient multi-database pagination,
entity-owned lifecycle enforcement, a simple explicit uncertain-write response,
a local HTTP reference mail provider, and database-agnostic durable jobs without
Cloudflare or another hosted queue dependency. Safe transient-failure retries
default on, with bounded exponential backoff and jitter; uncertainty and
non-repeatable effects remain excluded. The compiler/runtime owns these defaults
so application-writing LLMs need fewer architectural decisions. See the
[task plans and owner answers](work-plans/golden-delivery-planning.md#checkpoint)
for scope, acceptance checks and remaining contract/implementation work. These
are accepted directions, not claims that the runtime or formal contracts are complete.

### Lifecycle and reminder service successors — 2026-10-02

The owner selected clause-bound compiler retention maintenance, limited to
already-soft-deleted, expired rows in deterministic batches of at most 500,
without general authored deletion authority. The separate POLICY-D30-M1
addendum preserves the historic D01–D43 approval pin. The owner also selected
a new reminder for each changed due-date schedule, using a fresh durable intent
UUID stable across attempts and reconciliation. The lifetime Todo key remains
historical pressure evidence.

Independent correction reviews accepted [DATA-007](lifecycle-plan.md) and
[SERVICE-001](service-plan.md) for contract freeze. The latter includes
revision-guarded completion, held successor dispatch while an older intent is
unknown, a narrow policy identity amendment, matching request/receipt keys and
compiler observation time distinct from provider evidence. RM-205 and RM-301
are complete decision tasks; compiler, HTTP/job and full golden execution, plus
protected P10R approval, remain separate gates. No human-owned pressure policy
bytes or general application permissions are changed by these records.

### Durable delivery contract — 2026-10-04

The owner-approved database-backed durable-job directions are now frozen as
[ASYNC-001](async-plan.md), after the
[independent transaction/effect review](../tests/validation/rm305-independent-contract-review.json).
The contract uses the existing SQLite/PostgreSQL authority database, atomic
mutation plus intent, fenced claims, fixed UTC/coalesced scheduling, per-key
FIFO, finite cumulative budgets and no blind replay after a possible effect.
This semantic decision does not establish executable conformance: all 18 traces
remain unexecuted until RM-306/RM-307, and RM-108 still supplies the golden limits
and integration evidence. The existing protected policy decisions are unchanged.

### Golden reminder authority and retry limits — 2026-10-04

The owner accepted both recommended RM-108 choices: a dedicated, revocable
reminder-service role, and at most **3 total scheduled invocations within 1 hour**
of the first valid claim for known-no-effect failures. Each invocation keeps
SERVICE-001's 3 provider attempts / 30 seconds. Unknown outcomes never auto-resend;
schedule activations, lease recovery and restarts do not reset either budget.

The role permits only currently reminder-eligible Todo reads, active owners'
recipient details, ReminderMail dispatch, and the matching schedule revision's
`reminder_sent_at` write. It does not grant general update/delete, impersonate
owners or bypass policy because a caller is generated/internal. Existing checked
service authentication and live admission-time authority checks remain required.
See the [owning implementation plan](work-plans/golden-delivery-planning.md#rm-301307108--services-durable-jobs-and-reminders).
This accepts the scope, not implemented permissions or golden conformance;
compiler/runtime and independent policy review gates remain open.

### Golden self-disable field-policy repair — 2026-10-04

After independent review exposed that explicit empty-field denial blocked the
intended self-disable transition, the owner explicitly approved adding only
`UserRole.self: [update]` to the migrated User's `status` and `disabled_at`
field policies. The existing self-scoped `disable_user` named-operation update
exception remains unchanged. No field read, general User update, owner
impersonation or generated/lifecycle policy bypass is granted. Lifecycle ownership
still forbids ordinary `set` and `patch` writes. This narrow source repair does
not rewrite the frozen POLICY-D01–D43 matrix or approve reminder-worker authority.
See the [owning checkpoint](work-plans/golden-delivery-planning.md#checkpoint)
for current tests and independent correction-review status.

### Keyword names and canonical formatting — 2026-10-01

During the parallel roadmap planning assessment, the owner selected:

- **RM-201:** reserve control-flow/declaration words; permit other language words
  as names only in explicit safe contexts supporting both declaration and use.
  This excludes broad keyword-as-name parsing and prevents unreadable bindings.
  The exact matrix and compiler/diagnostic changes remain implementation work;
  see the [naming policy direction](naming-and-qualification.md#9-reserved-word-policy-direction--2026-10-01).
- **RM-202/RM-203:** derive canonical layout from syntax, so equivalent parsed
  code gets identical output regardless of authored non-comment line breaks.
  Preserve comment text, literal bytes, token semantics and comment attachment.
  Preserving authored breaks was considered and not selected. The
  [formatter contract](formatter-rules.md#canonical-output-rules) now records
  the target; the existing implementation still preserves some source breaks.

These answers authorise planning/contract reconciliation in this pass, not
implementation or a claim that the associated tasks are complete. Frozen naming
section-2 bytes and historical formatter evidence are preserved. A whitespace
change that alters parsing (for example a line-sensitive field modifier) is not
an equivalent-input test; canonical output must preserve the original syntax tree.

### Golden fresh-authentication query accounting — 2026-09-30

The owner clarified that the golden application's fresh-authority budget means
one principal lookup, with credential/session checks counted separately. It does
not mean one SQL query total across authentication. Acceptance records physical
principal-authority queries and additional credential-authority queries as
separate counters; combined queries count once in the principal category.
Business queries are separate. This clarification leaves AUTH-001's signed,
bounded, immediate and fresh-authority security guarantees unchanged. See the
[case-by-case reconciliation](../examples/golden-todo/REVIEW.md#2026-09-30-authentication-reconciliation).

### Golden self-service email projection — 2026-10-02

For RM-107, keep `User.email` provisioning-only and omit it from
`GET /users/{user_id}/todos`, including when a provider token carries an email
claim. The human-owned policy remains unchanged; the owner-authorised acceptance
revision changes AUTH-007 and REL-001 and keeps `UserWithTodos` limited to
`user_id` and the bounded `TodoView` list. Its predecessor is preserved at
[`acceptance-before-rm107-email-decision.json`](../tests/validation/golden-baseline/acceptance-before-rm107-email-decision.json),
with hashes and scope recorded in the [review log](../examples/golden-todo/REVIEW.md#2026-10-02-owner-authorised-userwithtodos-email-revision).
The route and child projection remain unimplemented and the cases remain
unexecuted.

### Service-key provisioning authority — 2026-10-02

The owner chose to keep service-key creation at the trusted host provisioning
boundary with one-time reveal to its authorised sink. Authored Jadpo source may
declare the service validator and proposed exchange transport, but does not gain
a business-callable mint operation or a source-declared administration endpoint.
This resolves RM-104's issuance-authority question without changing AUTH-P5's
creation, rotation, expiry and revocation requirements. The exchange syntax and
HTTP lowering still require independent language/security review and runtime
evidence; see the [RM-104 handoff](work-plans/golden-delivery-planning.md#rm-104-strengthened-handoff).

### Product and philosophy

- The target is backend development, initially ordinary SaaS/web applications.
- The environment is designed for agent-primary authorship and human
  readability/editability.
- The LLM is not trusted to provide assurance for its own implementation.
- Human intent and policy, LLM implementation, and compiler verification have
  distinct authority.
- The compiler is deterministic and deliberately boring.
- The optimisation target is minimum ambiguity per token, not minimum
  characters.
- Common constructs should have one canonical form.
- The earlier naming deferral was superseded by the owner’s Jadpo decision above.

### Bindings and data

- `var` is an immutable binding.
- `var mut` is a mutable binding.
- Rebinding an immutable value is invalid.
- Assignment rebinds only a bare, fixed-type `var mut` local with a compatible value; parameters and
  fields are not assignment targets.
- Parameters and returned data have immutable value semantics. Copying, moving,
  sharing, and copy-on-write are permitted implementation strategies only when
  source code cannot observe the difference. There is no accepted `ref`, alias,
  address, or `inout` syntax.
- There is no language `undefined`.
- There is no ambient language `null`.
- `T?` expresses a value that may be absent; the absent value is `none`.
- Omission is separate from nullability and belongs to input/object shape.
- Concrete typed objects always contain all declared fields.
- Database fields are non-nullable by default.
- Database `NULL` maps to language `none` for `T?`.
- Partial selections produce projection types, not partial entities.
- There is no general truthiness.
- A field type on a named record-like declaration is referenced as
  `Type.field`, such as `Customer.id` or `Customer.email`; it retains semantic
  identity rather than collapsing to its representation primitive.
- A field type is a nominal refinement of its declared type. It may widen to
  that type, but base-to-field narrowing and sibling-field substitution are not
  implicit.
- Selecting through a field whose declared type is a named record resolves
  members against that declared record. For example,
  `customer.billing_address.postal_code` has type `Address.postal_code` when
  `Customer.billing_address` refines `Address`; the compiler does not invent a
  synthetic `Customer.billing_address.postal_code` type.
- Field references inherit intrinsic value validation and nullability, but not
  shape omission, defaults, indexes, uniqueness, existence, authorisation, or
  permission to read a field.

### Types and boundaries

- Types are executable contracts rather than annotations.
- An ordinary object type is complete identity-free value data. An `entity` is
  a nominal identity-bearing domain subject whether or not it is stored.
  Persistence is an optional explicit entity capability rather than the source
  of entityhood. A complete entity value is an immutable validated snapshot,
  not a lazy or mutable ORM proxy. See the accepted
  [entity/query model](entity-query-model.md).
- Every entity exposes a compiler-owned nominal reference derived from its
  declared identity. A complete entity value may project to that reference; a
  reference never loads a complete entity implicitly.
- `input:` and `output:` are boundary roles that reference any declared type;
  they are not type categories. Input use derives recursive closed decoding and
  validation, while output use derives recursive closed validation and
  serialization. Reusing a type at both boundaries never bypasses either
  boundary check.
- `Object` supports recursively nested closed object fields. `List<T>` supports
  recursively validated arrays, including `List<Object { ... }>`; compiler-
  synthesized nested identities remain anchored to the containing field rather
  than introducing structural compatibility between unrelated objects.
- The accepted prelude contains representation/time types `Bool`, `Int`,
  `Decimal`, `Text`, `Bytes`, `Uuid`, `Instant`, `CalendarDate`, `Time`,
  `Duration`, and `Unit`; containers `Object`, `List<T>`, `Set<T>`, and
  `Map<K, V>`; the compiler-generated IANA `Zone` enum; the application-bounded
  `Locale` enum; and the compiler-owned validated semantic types `Email`,
  `Url`, and `IpAddress`. The current `Date`, zone-less `Time`, and `DateTime`
  implementation is migration input, not the accepted target model.
  Policy-dependent concepts such as username, slug, phone number, postcode,
  money, and country code remain authored domain types rather than vague
  built-ins.
- Semantic and field types are nominal. Matching primitive representations or
  structures do not create implicit compatibility.
- `Type(value)` constructs a semantic or field value by validation; it is never
  an unchecked cast. Constant arguments are validated during compilation and
  dynamic arguments at runtime.
- Failed dynamic construction is compiler-visible and must be declared, mapped,
  or handled rather than returning `none` or throwing an ambient exception.
- Runtime validation occurs at HTTP input/output, database read/write,
  environment/config, queues/events, external responses, and deserialised cache
  boundaries.
- Callable spelling follows ownership. Authored free functions, actions, and
  queries are unqualified in their current or selective-import scope;
  entity-owned operations are qualified by the entity or an accepted receiver;
  compiler standard-library families use mandatory lowercase domain namespaces
  such as `temporal.*` and `collection.*`. Standard namespaces cannot be opened,
  aliased, or mirrored with method/free-function synonyms. Type construction,
  enum variants, contextual capabilities such as `clock.now`, and data access
  such as `config.mail_sender` are distinct forms rather than callable aliases.
- The accepted [naming and qualification contract](naming-and-qualification.md)
  is bound to section-2 digest
  `sha256:914e6c32c3d349cccfc9184dfc1dc40eb9671b247b4664d41d964c338901c0b4`.
  It applies the same casing, ownership, import, namespace, parameter-order,
  and one-canonical-spelling rules to every present and future language family.
- The approved [TIME-001/TEST-001 contract](time-testing-plan.md) is bound to
  section-2 digest
  `sha256:17b8ac38c645f6e3f42abf9a7644b9c6f67451d0c3407f7bf108ec057a931f01`.
  `Instant` is the ordinary UTC-millisecond timestamp; `CalendarDate` is only a
  genuinely date-only fact; resolved `Time` contains an instant plus one
  compiler-generated `Zone` enum value. Clock fragments are contextual inputs
  to `temporal.resolve`, never independent authority values.
- Authored code has one canonical `temporal` namespace. Operations put the
  primary value first, zone then locale context next, and named policy/options
  last. The compiler rejects host date libraries, synonyms, reversed overloads,
  implicit time conversions, free-form zone strings, and hidden clock/locale
  defaults.
- Human formatting accepts `CalendarDate` or resolved `Time`, returns
  presentation-classified text, and cannot become database/query/time
  authority. Absolute styles and the v0.1 conversational friendly profile are
  closed contracts; friendly output always receives an explicit reference
  instant.
- The generated runtime captures one stable operation instant and uses an
  internal monotonic source for elapsed deadlines. Explicit compiler-owned
  `on: create`/`on: create_or_change` field roles receive operation time;
  callers, authored mutations, database defaults, generated expressions, and
  triggers cannot own those values. Historical migrations never invent an
  unreviewed current time.
- A successfully typed application value has actually been validated.
- Domain constraints should drive all relevant static, runtime, database, API,
  documentation, and test representations.
- A constrained semantic type describes an open set of validated values; its
  individual string or numeric values are not enum members. Finite domain
  alternatives and lifecycle states use an enum or another explicit domain
  model rather than magic sentinel values inside an open semantic type.
- Authored source references enum variants only with qualified dot syntax, such
  as `InviteStatus.blocked`. Call-style construction from a literal or dynamic
  representation—such as `InviteStatus("blocked")` or
  `InviteStatus(raw_text)`—is invalid. External values become enum variants only
  through declared boundary decoding or an explicit typed parsing operation.

### Backend semantics

- Database operations are compiler-understood constructs.
- Postgres is the initial opinionated database.
- Every entity has exactly one authoritative declaration under `entities/`.
  Persistence, entity-specific policy, lifecycle, queries, functions, and
  actions are explicit sections or capabilities in that entity dossier. The
  initial language has no partial entities, extension methods, inheritance,
  overrides, or receiver overloads.
- Dot syntax on an entity value or reference is statically resolved qualified
  call syntax. It never implies dynamic dispatch, hidden mutation, loading,
  saving, exceptions, or transaction creation. Entity operations declare
  whether their receiver requires the reference or complete value. A mutating
  value receiver is an observation rather than concurrency authority and must
  reload/lock current state or use a checked revision/conditional-write guard.
- A named `query` is a restricted read-only runtime operation. It may suspend
  and fail operationally, but cannot mutate, call actions or services, emit
  events, or access secrets. Entity-centred queries live with the entity;
  genuinely cross-entity projections are named top-level queries under
  `queries/` and return complete values. A query over a derived representation
  declares an authoritative, read-your-writes, bounded-staleness, or eventual
  freshness requirement; the compiler may satisfy it with a stronger plan but
  never silently with a weaker one.
- Raw query expressions do not appear in routes, functions, ordinary top-level
  actions, workflows, jobs, policy, or configuration. Those declarations call
  named queries.
- Only actions owned by an entity may directly create, update, or delete that
  entity. Routes and workflows call named entity actions and cannot bypass
  entity invariants, lifecycle, or policy.
- Every mutable fact has one declared authority. Caches, search indexes, graph
  read models, and other derived representations are not independently
  writable application state. An authoritative mutation and its durable change
  record commit together; generated delivery is ordered, idempotent, replayable,
  observable through watermarks, and repairable from authority.
- Persistent creation currently uses `create Entity { ... }` in the executable
  prototype. Fixture-first implementation of the accepted entity dossier will
  determine the final construct spelling while preserving nominal field
  validation and a validated entity result rather than a driver result.
- Persistence SQL is compiler-generated and parameterised. Returned rows cross
  a generated validation boundary before becoming trusted entity values.
- The first executable read form is `query optional Entity { where: field ==
  value }`. It remains prototype evidence for cardinality, validation, and
  operational containment while named entity and top-level query declarations
  are implemented.
- Required-one read uses the same predicate plus `missing: FailureName`; its
  result is non-nullable and the bound failure must be declared and derive from
  `NotFound`.
- Required update/delete bind zero rows to a declared `NotFound` failure and
  database constraints to a declared `Conflict` failure. They return the
  validated affected entity rather than a row count.
- Required mutations preflight cardinality and execute transactionally; a
  multiple-row match is an operational fault and commits no mutation.
- An entity action is independently failure-atomic when invoked directly, but
  composition does not silently merge entity-action transactions. An enclosing
  action reaching multiple mutation scopes must explicitly declare atomic
  intent or a durable-workflow disposition; omission is a compile error. For an
  explicit atomic boundary, the compiler supplies one transaction context to
  nested entity actions and queries,
  derives commit/rollback and handled-failure savepoints, keeps policy and
  invariant reads inside that boundary, and rejects incompatible transaction
  domains or external effects. A nested success is provisional until its outer
  boundary commits. Isolation, locking/conditional-write strategy, deadlock
  ordering, and safe retry behaviour remain visible checked/audited TX-001
  contracts rather than hidden consequences of the call graph.
- Cross-store consistency has four distinct contracts: local atomic,
  compiler-verified prepared atomic, durable projection, and durable workflow.
  `atomic` is accepted across domains only when every adapter proves a common
  prepare/commit and durable-recovery protocol. One authority plus derived
  stores uses an authority transaction plus durable change record; multiple
  authorities use a persisted retry/compensation/reconciliation workflow.
  These contracts never silently degrade into one another.
- Raw SQL is not ordinary application code.
- A stored owning reference may declare a separate logical traversal name with
  `references Target.field as relationship on_delete action`. The stored field
  remains explicit, omission of `as` uses that field name, and the compiler
  never infers a relationship name by stripping `_id` or another suffix.
- External calls occur through reviewed, typed service contracts.
- External/provider failures are normalised at the service boundary.
- Application failures are typed, declared once, and reusable. Every callable
  has a closed set of recoverable application failures and source-agnostic
  operational problems; each must be handled, mapped, or propagated through
  `fails`.
- Recoverable operational problems use a deliberately small vocabulary such as
  `Unavailable`, `TimedOut`, `RateLimited`, and `OutcomeUnknown`. Adapter,
  vendor, and storage names remain internal provenance rather than source-level
  problem types.
- Defects and impossible runtime states are not catchable failures. They are
  contained and reported at the runtime boundary.
- A fallible expression has exactly two acknowledgement forms. `attempt`
  unwraps the successful value and visibly propagates every failure. An
  exhaustive `match` over the call's outcome handles, maps, recovers from, or
  explicitly propagates each success and failure case. A bare fallible call is
  invalid, and ordinary source never stores an exposed `Result` wrapper.
- Outcome matches use `success(value)` and exact `failure FailureName` arms.
  A failure arm may produce a compatible replacement success value, `reject`
  another declared failure, or `propagate` the matched failure. Failure
  wildcards are invalid, and adding a possible failure to the callee makes an
  existing outcome match non-exhaustive until the author decides its meaning.
- After outcome matching and declared propagation, the enclosing callable's
  authored `fails` clause must exactly equal its reachable unhandled problem
  set. A missing or stale extra entry is a compile error. The compiler derives
  and explains the exact set and may offer the corresponding guided edit, but
  never silently changes the source contract or public failure surface.
- Every application domain failure derives from exactly one standard failure
  kind; the kind supplies default transport and runtime behaviour.
- An application failure declares its standard kind with an explicit `kind`
  member, a stable public `code`, an optional safe static `message`, and
  declaration-owned `public` and `internal` context schemas. Production sites
  supply one flat object whose fields are checked for completeness, uniqueness,
  lexical availability, and exact nominal type.
- Application code cannot throw arbitrary exceptions, reject strings, select
  numeric HTTP error statuses, or construct ad hoc error responses.
- Public failure output contains no application data by default. Any public
  detail has an explicit schema; internal context never implicitly crosses the
  client boundary.
- Expected domain failures use semantic traces without native stacks by default.
  Operational faults and defects retain cause chains and low-level stacks
  internally; public clients never receive them.
- `NotVisible` is distinct from `NotPermitted` so policy can conceal resource
  existence with a 404-shaped response.
- Disabling authentication on a route is explicit and conspicuous as
  `auth: none`.
- Required authentication is inherited by every route and is not repeated as
  `auth required` in canonical source.
- Provider-specific authentication terminates at a generated boundary. Routes,
  actions, and policy consume one stable authenticated user or service
  principal with identity, allowlisted user data, and authentication strength
  rather than provider SDKs, tokens, strategy names, roles, or permissions.
  POLICY-001 resolves roles from separate authoritative bindings. Multiple
  configured strategies resolve deterministically and never merge privileges.
- A route is the HTTP boundary, an action is a runtime-managed application
  operation that may perform effects, and a function is pure, non-suspending
  computation from its arguments. The three remain distinct even when
  behaviour is colocated. A function may itself be fallible without becoming
  an action, but it cannot read persistence, call services, emit events, read
  ambient runtime state, or invoke an action.
- Authored Jadpo has no `async`, `await`, promise, or detached-call surface.
  Every ordinary action invocation completes before its caller continues;
  generated targets may suspend internally. Actions may call functions and
  actions, while functions may call only functions. Changing an adapter or
  generated implementation between synchronous and asynchronous execution is
  not a source-contract change.
- Ordinary action execution is sequential. Parallel or background work must
  eventually use an explicit structured-concurrency, event, or durable-job
  boundary with defined cancellation, failure, retry, idempotency, transaction,
  and lifetime semantics; unstructured fire-and-forget execution is invalid.
- A small, one-off endpoint keeps an inline `action` inside its route. A named
  action is extracted only for reuse, a stable domain command, or a boundary
  that deserves independent testing, policy, or transaction reasoning.
- A route contains exactly one behaviour form: either an inline `action` or a
  `run:` invocation of a named action. `run:` exists only for the extracted
  form and makes transport-to-domain argument mapping explicit.
- An inline action has the same semantics as a named action and declares its
  recoverable surface in its brace-delimited header, for example
  `action: fails TodoNotFound, Unavailable { ... }`.
- Route items use the field-style colon separator consistently, including
  block-valued `path:` and `action:` items as well as `auth:`, `input:`,
  `output:`, and `run:`. The former space-only spelling is invalid. Blocks use
  braces; indentation is never semantic.
- Path, query, header, and body bindings belong to the route transport
  boundary and are statically typed before the action runs. Authentication
  headers remain owned by the generated authentication boundary rather than
  ordinary action input.
- Configuration fields use structured option bodies and keep their explicit
  environment binding beside their typed declaration. They are required unless
  they have a checked non-secret literal default; no parallel deployment
  binding file or implicit environment overlay is part of v0.1.
- Local configuration uses only `.env.local`. `jadpo config set <field>` accepts
  prompted values without exposing secrets to chat, shell arguments, or output,
  while `jadpo config check` reports only safe presence/validity status.
- All v0.1 configuration is startup-bound. `jadpo dev` validates and restarts
  after valid changes; deployment integration and runtime startup validate real
  production values automatically. `jadpo check` remains the one full static
  checker, and `build` never requires secret values.
- Generated production targets disable Bun's automatic environment-file
  discovery and add no configuration package. Liveness remains local while
  bounded readiness reflects required dependency availability.
- Route templates spell path placeholders as `{name}`. A brace-delimited
  `path: { name: Type }` group types them, and behaviour reads the validated
  values through `path.name`. Template placeholders and declarations must
  correspond exactly one-to-one; missing, extra, or duplicate names are
  invalid.
- Callable signatures place their closed recoverable surface before their
  success type: `action name(parameters) fails A, B -> Result`. This ordering
  applies consistently to actions and fallible functions; failure-free
  callables remain `name(parameters) -> Result`.
- Routes remain action boundaries. A small route uses an inline action, and
  `run:` invokes a named action only; it does not invoke a function directly.
  Pure reusable computation remains available from either action form.
- `:` associates a named member with a type, value, or setting. Constraint and
  persistence settings therefore use `min_length: 6`, `identity: id`, and
  corresponding colon forms. `=` is reserved for defining a named type or
  binding/assigning a value. Parentheses are limited to callable interfaces,
  calls and scalar constructors, explicit expression grouping, and constructor-
  shaped patterns; persistence metadata does not masquerade as a function call.
- The exact colon-delimited spelling for named compound uniqueness and
  compound non-unique indexes remains unresolved. Implementations may preserve
  the former spelling as migration compatibility, but documentation and
  scaffolds must not present a guessed replacement as canonical.
- Policy is human-owned; audit is compiler-derived.
- CI blocks policy/implementation disagreement.
- The digest-pinned [POLICY-001 implementation plan](policy-plan.md) is the
  accepted authorisation contract. Roles are closed qualified enum values whose
  facts come only from authoritative direct relationship or membership
  bindings. Entity policy is one role-first matrix over compiler-derived
  `create`/`read`/`update`/`delete` effects; routine named queries/actions
  inherit automatic row scoping; exceptional field policy narrows but never
  widens; non-entity operations use local `invoke` policy; and the only
  compiler-owned non-role subjects are the explicit `Access.public` and
  `Access.authenticated`.
- POLICY-001 composes authentication, closed input validation, supplied-field
  tracking, field write ownership, authoritative role resolution, lifecycle
  and business rules, policy-scoped parameterised database access, database
  constraints/result decoding, authorised projections, and exact output
  validation as distinct fail-closed gates. It never silently strips input or
  output fields.
- Policy is semantically colocated with its entity, field, or non-entity
  operation. Human authority comes from the protected approval protocol over
  canonical policy and semantic-graph digests, so access expansion through an
  input, field, projection, output, route, role binding, or call edge is
  reviewable even when the visible policy block is unchanged.
- The accepted POLICY-001 section-2 contract is pinned as
  `sha256:66e7a8f586b62ed92c3a7220f524e25aee5808ca60b8504fb3c2d225ef2d68bd`;
  POLICY-P0–P6 may implement but may not silently change its scoped-role,
  automatic-enforcement, validation, database, output, or approval boundaries.
- The owner's 2026-10-02 narrow retention-maintenance direction is a separate
  [POLICY-D30-M1 contract addendum](policy-plan.md#19-retention-maintenance-addendum--policy-d30-m1),
  outside the unchanged section-2 D01–D43 baseline. Its own row digest and
  independent DATA-007 correction review establish contract freeze; the historic policy
  digest does not approve the new maintenance plane.
- Generated target code is not normal developer-facing source.
- `build/` is compiler-owned disposable output and is excluded from authored
  `.jadpo` discovery.
- P8 derived JSON artifacts are deterministic, versioned, project-relative,
  and contain no timestamp, random identifier, or machine-specific path.
- The first executable prototype and every application that does not declare
  JWT bearer validation are dependency-free TypeScript generated for Bun
  beneath `build/target/app.ts`; `build/` is reproducible and not committed by
  the canonical scaffold.
- Generated Bun targets may import only `bun`, `bun:*`, compiler-owned relative
  TypeScript modules, and the single compiler-selected `jose` package when JWT
  bearer validation is explicitly declared. Only that capability may emit its
  compiler-owned manifest and frozen lock/integrity record. Bun auto-install is
  always disabled; all other external imports and dependency metadata reject.
- Required-one queries bind absence explicitly with `missing:` to a failure
  declared by the enclosing action and derived from `NotFound`; they never
  convert absence into an untyped exception.
- Persistence adapters convert raw SQLite and PostgreSQL driver errors into the
  compiler-owned `PersistenceFault`, retaining the original error only as its
  internal cause.
- The Bun 1.2.20 PostgreSQL adapter uses `prepare: false`: parameter binding is
  retained, while named prepared statements and pipelining remain disabled
  until a pinned runtime upgrade passes the complete live rollback suite. This
  avoids an unresolved `SQL.begin()` promise after a prepared constraint error.
- Target generation fails for authenticated routes until the required-default
  authentication runtime exists. Missing security infrastructure never weakens
  a route to public access.
- IDE semantics should come from one compiler-backed LSP. TextMate and renderer
  grammars are presentation adapters and cannot become a second parser or type
  system.
- Editor navigation must not underline ordinary Jadpo source. Generated-
  artifact navigation is exposed through CodeLens or explicit commands, not
  document links whose decoration competes with diagnostic squiggles.

### Diagnostics and observability

- Diagnostic definitions live in one versioned compiler-owned catalogue. Call
  sites supply typed facts; they do not invent user-facing prose, repair advice,
  or editor-only variants.
- User-facing rule identifiers use readable lower-case dotted names such as
  `failure.must_be_declared`. Existing upper-case codes remain migration aliases
  only while fixtures and integrations move to the catalogue.
- A diagnostic is a guided repair protocol rather than an error string. It has
  a human-first `summary`, a short `reason`, one `recommendedNextStep`, bounded
  `alternatives`, a precise `location`, typed `context`, bounded `impact`, a
  `decisionOwner`, and a stable `helpId`.
- Every error classifies its next step as an automatic fix, a guided semantic
  choice, or a human-owned decision. The compiler marks a fix as preferred only
  when it can justify that preference, previews its behavioural/public-contract
  impact, and validates the edit against the diagnostic's source revision.
- The same semantic diagnostic object generates human CLI text, versioned agent
  JSON, LSP diagnostics and code actions, and reference documentation. Raw JSON
  is not the normal IDE presentation.
- IDEs show the plain summary in the Problems panel and inline squiggle, then
  show the reason, recommended step, alternatives, impact, and help link through
  hover and an expandable details view. Quick Fix lists the recommended verified
  edit first and cleanly separates other valid choices; multi-file edits receive
  a diff preview.
- Hovering a function or action declaration, reference, or call shows its
  complete typed outcome contract: callable kind, parameters, successful result
  type, every declared application failure and built-in operational problem,
  and whether each failure is propagated or handled at that call when known.
  Action hover also explains that the operation may suspend internally but
  completes before the caller continues. IDE presentation must not expose a
  generated `Promise` or `Result` wrapper as authored source semantics.
- Parser cascades and dependent consequences are grouped beneath one root cause.
  Pipeline summaries such as "type checking failed" are report status, not
  additional user problems.
- Public catalogue entries may not fall back to identifier-derived summaries,
  “compiler-enforced invariant” reasons, or “update the source” advice. Every
  public code has authored, fixture-backed repair guidance and passes the shared
  [diagnostic presentation contract](diagnostic-presentation-contract.md).
- An invalid route authentication value is one root diagnostic over the value,
  not a token cascade. Because `auth: none` disables the authenticated default,
  the diagnostic is a human-owned security decision: retain authentication by
  removing the item, or intentionally use the exact `none` opt-out. Neither
  choice is silently preferred or applied by an agent.
- Compiler diagnostics, public HTTP failures, operational telemetry, and
  agent incident packets are distinct audience-specific schemas. None can be
  serialized directly as another.
- Public HTTP failures contain only the declared stable code, static safe
  message, request identifier, and explicitly public details. They never expose
  source locations, repair advice, adapter/provider data, stacks, or internal
  context.
- Diagnostic and logging APIs accept compiler-approved safe value types rather
  than arbitrary formatted values. Secret values are non-renderable; `internal`
  means client-hidden, not automatically safe for logs, telemetry, or an LLM.
- Operational telemetry uses a redacted vendor-neutral event model compatible
  with structured JSON and OpenTelemetry correlation. Provider adapters may map
  safe fields into third-party grouping, tagging, release, and trace features,
  but application code does not call vendor SDKs directly.
- Production events carry stable semantic operation and source-revision IDs. A
  trusted local tool joins those IDs with the compiler graph to build a rich,
  bounded agent incident packet containing source location, rule, context,
  impact, and repair choices without sending customer values or secrets through
  third-party telemetry.

### Process

- Every compiler slice is preceded by canonical source and executable-style
  acceptance fixtures; implementation must not silently decide semantics.
- The basic semantic compiler starts from the Jadpo seed application.
- The first complete application is a todo backend, required before persistence
  and authentication expansion is considered complete.
- The second is a deliberately awkward order/payment backend.
- Maintain a language-design issue log rather than silently inventing syntax.
- Compare against a strong TypeScript baseline and try to falsify the idea.
- Preserve per-change tokens, time, attempts, defects, diagnostics, workarounds,
  and human interventions for that comparison. Every language-friction incident
  receives an explicit disposition; the experiment must not silently change the
  language to make it outperform the baseline.

## 2. Provisional directions

### Enforced component messaging and application graphs — 2026-10-04

The owner selected planning for one compiler-enforced interaction model:
logical components own effects, cross-component behaviour uses typed messages,
subscribers can handle multiple events and use safe defaults, and required facts
are tied to managed transitions rather than remembered calls. Imports cannot
grant effect authority; private local actions across files and pure shared
computation remain useful. Prefer stateful subscribers as the sole authored
coordination model, reconciled with WORKFLOW-001 rather than adding another DSL.

Hierarchical graphs should expose documentation, data/control dependencies,
actual execution and safe diagnostic/performance evidence to UI and LLMs.
Persistent logical identity and exact immutable deployed artifacts must coexist
so production incidents remain understandable across divergent branches and
graph-changing fixes. Existing failure disclosure channels remain authoritative.

Language-wide syntax review comes first. The later
[naming/principal decision](#naming-and-principal-direction--2026-10-04)
settles those directions. The owner prefers existing `attempt` plus explicit
`emit_event(...)` and specialised typed event declarations; postfix propagation
and separate command/fact APIs are not the preferred authoring direction.
The [single-query-operation decision](#delimited-query-operation-direction--2026-10-04)
now selects that syntax direction. Full grammar, delivery semantics and
compatibility still need review. See the [event candidate](event-model.md) and
[identity/incident artifact candidate](generated-artifacts.md#semantic-identity-and-incident-artifact-candidate--2026-10-04).
Keep concepts/defaults simple and preserve the
active golden delivery and frozen ASYNC-001 contracts. These are planning
directions, not accepted amendments or implementation conformance.
See [assessed plans, context and unresolved limits](work-plans/roadmap-assessment.md#syntax-component-messaging-and-graph-intake--2026-10-04),
RM-222/RM-309/RM-219, conditional successors RM-223/RM-310–RM-312 and
[RM-1108's existing E11 plan](work-plans/developer-console-mcp.md#hierarchical-application-graph-plan--2026-10-04).

These are recommended but must survive the golden applications:
- braces, no required semicolons, and canonical non-semantic formatting;
- explicit domain primitive types such as `Email`, `Money`, `Percentage`,
  `Instant`, `CalendarDate`, and `Duration`;
- constrained-type blocks;
- explicit `List<T>`/`Map<K,V>` style collection types;
- namespaced collection functions such as `collection.count(items)` rather
  than prototype magic or unqualified global built-ins;
- exhaustive `match` plus `if`/`else` as the main conditional forms;
- data-carrying enum variants as nominal tagged sums when variant-specific
  payloads make invalid states unrepresentable, subject to P12 pressure tests
  and explicit boundary, persistence, compatibility, and migration semantics;
- no enum methods, traits, automatic display labels, or customer-facing string
  derivation merely as part of adding enum support; named functions and
  exhaustive `match` remain the initial behaviour model;
- `for item in items` as the initial iteration form;
- declarative `create`, `query`, `update`, and `delete` syntax;
- the exact field syntax for typed query, header, and body bindings inside the
  accepted route boundary;
- Rust for a serious compiler/toolchain;
- TypeScript/Bun as a pragmatic first executable target;
- a possible native binary target later;
- stable semantic node IDs plus source maps and `app.meta`;
- checkpoint formatting rather than formatting every agent save;
- Candidate A's elastic single-application tree is the P9–P11 project-structure
  prototype, with a root `app.jadpo` allowed to grow into feature slices;
  final layout and enforcement level remain open until the golden applications
  pressure-test it;
- authored multi-file applications opt into explicit stable logical `module`
  headers, selective imports, and private-by-default declarations exported with
  `public`; the first bounded core requires one module per file and globally
  unique declaration names while aliases, re-exports, relative imports,
  multi-file modules, and same-name namespaces await golden-application
  pressure;
- a deterministic static project scaffold before dynamic profiles; interactive
  creation should be a thin interface over the same finite, reproducible flags
  used by humans and agents;
- prohibiting raw representation primitives in ordinary application callable
  signatures, while retaining them beneath semantic types, in local
  computation, and in low-level standard-library facilities;
- the initial standard failure-kind catalogue and canonical HTTP envelope in
  the failure-model specification;
- a stable machine language identifier, separate from an eventual product
  name, shared by file associations, LSP, Markdown fences, and renderer grammar
  packages.
- explicit named imports for authored multi-file applications, replacing the
  prototype's ambient application-wide namespace once the golden todo is split;
  external dependencies remain separately declared and reviewed.
- the P10R candidate policy kernel's five-way judgement (`proved`, `rejected`,
  `requires human decision`, `unsupported`, and `cannot prove`) with absence of
  proof always failing closed; final acceptance depends on independent review
  of [the kernel](policy-proof-v0.1.md) and its fixtures;
- independent policy-weakening approval bound to canonical policy and semantic-
  graph digests through a protected CI/review attestation, with ordinary
  repository files unable to confer authority; final acceptance depends on the
  [approval protocol](approval-protocol.md) and comprehension experiment; and
- the [golden todo candidate](../examples/golden-todo/README.md) as the complete
  P10R pressure case, while its unsupported syntax remains design input rather
  than accepted grammar; and
- the [AUTH-001 implementation plan](authentication-plan.md) as the approved
  architecture for browser, user-API, and service-to-service authentication;
  cookie and bearer transport remain independent of immediate or bounded
  validation; user and service identities become one closed provider-independent
  principal type; caches are optional; sensitive routes can require fresh
  authority; and permissions remain the responsibility of POLICY-001. Jadpo's
  own signed envelopes use Bun-native cryptographic primitives without an
  application package dependency. JWT bearer validation is explicit opt-in and
  adds exactly one compiler-selected, pinned `jose` package with zero transitive
  dependencies; applications without JWT remain free of package metadata,
  external imports, dormant JOSE code, and installation.
  The accepted section-2 contract is pinned as
  `sha256:5cf32778486586b4645d226f3e4c1ebcd636d0d868b443c2be768d492003fb0b`;
  later syntax or implementation work may realise it but may not silently
  change its principal, selection, revocation, cache-independence, or
  dependency boundaries.
## 3. Rejected current alternatives

- **A loose semantic language.** Rejected because ambiguity gives agents more
  room to drift. Formatting may be forgiving; meaning is strict.
- **Cryptic token minimisation.** Rejected in favour of semantic density and
  readability.
- **Both `null` and `undefined`.** Rejected as needless ambiguity.
- **Missing fields inside concrete typed values.** Rejected; use projections,
  optional input shape, or `T?` explicitly.
- **Structural compatibility for domain values.** Rejected; two values do not
  become interchangeable merely because both are represented as `Text`, `Int`,
  or another primitive.
- **Prototype/magic collection properties such as `items.empty`.** Rejected as
  ambiguous between data, method, and intrinsic.
- **Active-record magic behind entity calls.** Entity-owned actions and checked
  dot calls are accepted, but `Order.create` or `order.update` may not imply
  hidden loading, local mutation, saving, transaction creation, lazy fields, or
  driver behaviour. Persistence remains a compiler-understood explicit effect
  inside the owning entity action.
- **JavaScript ambient exceptions for domain flow.** Rejected because signatures
  hide expected failure.
- **Manual route-by-route status mapping.** Rejected because transport behaviour
  belongs to the standard failure kind and must remain consistent.
- **Client stack traces and raw provider/database messages.** Rejected as an
  information leak; public failure data is allowlisted.
- **Rust-style `Result<T,E>` plumbing throughout source.** The safety goal is
  accepted; exposing the wrapper mechanism on every call is not.
- **Audit as editable source of truth.** Rejected; policy is source, audit is
  derived.
- **Formatting errors as semantic/compiler errors.** Rejected; use a formatter.
- **Reformatting on every tiny agent save.** Rejected provisionally because it
  can invalidate position-based iterative edits.
- **Generated TypeScript as a debugging or review surface.** Rejected.
- **Native compilation as the first proof.** Rejected as solving the wrong risk.
- **Premature naming.** Originally discouraged; superseded by the explicit Jadpo naming decision above. The caution against treating a name as product validation remains.

## 4. Open language questions

- the future bounded companion-file escape when one authoritative entity file
  becomes too large; the initial dossier, receiver, named-query, freshness,
  consistency, and mutation-guard punctuation is now fixture-backed;
- file, module, import, namespace, visibility, and package semantics;
- comment, documentation, intent, rule, and decision syntax;
- exact primitive types and literal forms;
- whether authored low-level functions need a conspicuous escape hatch for raw
  primitive parameters and results;
- money/currency and decimal semantics; recurrence, holidays, business
  calendars, natural-language time parsing, and custom friendly-format profiles
  remain deferred beyond the approved bounded time contract;
- generics and user-defined collection types;
- value-representation optimisation for large data and foreign buffers,
  including copy-on-write, uniqueness, zero-copy views, and builders; any
  caller-visible mutation contract requires new application evidence first;
- lambda/closure support;
- necessity and limits of `while` and recursion;
- resource and termination bounds;
- query cardinality and exact not-found syntax;
- pagination, aggregation, joins, and complex-query capabilities;
- adapter capability proofs and any extension to the implemented
  `consistency: atomic|durable_workflow` spelling or the audited initial
  PostgreSQL/SQLite isolation, concurrency, lock-ordering, savepoint, and
  explicit-idempotent-retry matrix;
- physical authority/projection delivery and durable-workflow adapters,
  change-journal/workflow state storage, revision-token boundaries,
  operational intervention, and generated repair controls; the
  semantic distinction between atomic, durable projection, and compensation is
  already accepted;
- relationship, index, uniqueness, and lifecycle syntax;
- partial-update mechanics;
- exact policy for exposing declared failure context through an optional
  `failure FailureName(problem)` outcome-pattern binding;
- final built-in operational-problem boundary table and non-HTTP adapter
  mappings;
- whether application authors can explicitly `panic`;
- output `none` as JSON `null` versus omission controls;
- exact route query/header/body binding grammar, plus file and streaming
  semantics;
- authentication roles/tenancy extensions beyond the approved user/service
  principal model, plus ownership/authorization proofs;
- policy storage, protection, and approval mechanism;
- external contract import/versioning/review;
- events, jobs, concurrency, ordering, retries, and idempotency;
- configuration source kinds beyond process-environment injection, live reload,
  hosted secret-manager SDKs, and cloud-specific deployment contracts beyond
  the approved [CONFIG-001 implementation plan](configuration-plan.md);
- exact fixture punctuation for the approved typed test semantics; the
  generated/authored evidence boundary itself is accepted;
- escape hatches and extension model.

## 5. Open implementation questions

- semantic graph and proof representation;
- source-map/metadata format and monitoring integration;
- Postgres schema-diff and migration strategy;
- target deployment environments;
- IDE/CLI and machine-readable diagnostics;
- how human approval is distinguished from agent edits.

## 6. Open validation and product questions

- Can agents work effectively in a language absent from pretraining?
- Can a TypeScript framework deliver comparable guarantees?
- How often do real applications need escape hatches?
- Is the source genuinely better for non-CRUD business logic?
- Are generated audits and behavioural diffs understandable to humans?
- What credible commercial model exists beyond an open-source core?
- Can adoption, support, ecosystem, and trust costs be justified?
