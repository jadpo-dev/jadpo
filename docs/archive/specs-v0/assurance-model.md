# Assurance model

**Status:** conceptual draft  
**Premise:** an LLM may implement the program, but may not be the authority that
declares its implementation safe.

## 1. Assurance hierarchy

The system separates four concerns:

1. **Human-owned intent and policy** define what is permitted.
2. **Application source** describes how the requirement is implemented.
3. **Compiler checks and proofs** determine whether implementation satisfies
   policy and language invariants.
4. **Derived audit, docs, and tests** explain and exercise the result.

The audit is never the source of truth. If both implementation and audit are
maintained by the same agent, the audit becomes “marking its own homework.” The
audit must be mechanically derived, while the policy it is checked against is
protected by a human-approval boundary.

## 2. Safe defaults

The initial model assumes:

- authentication is required unless public access is explicit;
- authentication strategies are configured behind one compiler-owned principal
  boundary, while qualified scoped roles come from authoritative POLICY-001
  bindings, so routes and business logic do not bind to provider SDKs, token
  formats, session mechanisms, or credential-carried permissions;
- every route has typed input and output;
- inputs, outputs, database reads/writes, config, queues, cache, and external
  responses are runtime validated;
- database access uses safe declarative queries;
- output only contains declared fields;
- network egress is restricted to declared services;
- secrets cannot flow into outputs or ordinary logs;
- rate limits, timeouts, transactions, and audit logging have safe defaults;
- asynchronous/external effects have declared retry and idempotency behaviour;
- migrations are generated and lifecycle ambiguity blocks the build;
- resource limits are automatic or explicitly declared;
- public failure responses contain only a stable safe envelope and explicitly
  declared public fields;
- internal diagnostic context, provider/database details, and stacks never
  reach public clients by default.

The source must positively state when a protection is removed. An omission
cannot silently create a public endpoint or an unbounded operation.

## 3. Compiler-enforced invariants

The original discussion proposed at least these invariants:

- every database mutation belongs to an action;
- every protected action is associated with one principal/authorisation context;
- every route has input and output contracts;
- every selected record is proven readable by the current principal's scoped
  roles under policy;
- every mutation is proven permitted under policy;
- every external side effect is visible in the effect graph;
- every async/external operation has delivery, retry, and idempotency behaviour;
- secrets are non-returnable and controlled in logs/egress;
- undeclared fields are rejected or cannot enter trusted values;
- a complete typed value never contains a missing declared field;
- optional values are narrowed before use;
- enum matches are exhaustive;
- deleted records/fields have explicit lifecycle and migration behaviour;
- output serialization cannot leak richer internal records;
- implementation cannot be more permissive than human-owned policy;
- every expected failure is declared, derives from a standard kind, and is
  handled, mapped, or propagated exhaustively;
- application code cannot use arbitrary exceptions or raw numeric HTTP error
  statuses;
- failure public schemas obey secret-flow, private-field, and output policy;
- database and provider errors are normalised before crossing their boundary.

## 4. Policy conformance

The accepted [POLICY-001 contract](policy-plan.md) colocates one role-first
matrix with the protected entity. For example:

```text
enum TodoRole {
    owner
}

entity Todo {
    owner_id: User.id {
        role: TodoRole.owner
        immutable: true
    }

    policy {
        TodoRole.owner: [create, read, update]
        ApplicationRole.support: [read]
        ApplicationRole.administrator: [read, delete]
    }
}
```

If an implementation allows any authenticated user to delete another user's
todo, compilation should fail even if the generated TypeScript is type-correct:

```text
POLICY VIOLATION

DELETE /todos/:id

Expected:
  Todo.delete = ApplicationRole.administrator

Implementation permits:
  TodoRole.owner

Build blocked.
```

The compiler separately identifies when lifecycle or another human-owned
contract lacks a necessary decision—for example, what deletion means for
related payment records. That decision is not disguised as a role rule.

## 5. Security properties targeted

The language should make these common classes structurally difficult or
impossible:

- missing authentication;
- insecure direct object references and unscoped multi-tenant queries;
- undeclared or unvalidated inputs;
- output of private/internal fields;
- secret leakage through return values, logs, or service calls;
- SQL injection and unrestricted arbitrary queries;
- arbitrary network egress;
- forgotten transaction boundaries;
- missing timeouts, retries, or idempotency on side effects;
- silently destructive schema/lifecycle changes;
- malformed identifiers reaching the database;
- inconsistent docs/tests after a change;
- stack traces, SQL/constraint details, and raw provider errors reaching a
  client;
- resource enumeration through inconsistent not-found and permission failures.

The compiler should not pretend to prove that the business requirement is wise
or that all domain logic is correct.

## 6. Audit output

Audit is a generated, readable view for review and CI. An illustrative route
entry includes:

```text
GET /todos
  authentication: required
  principal: current user
  reads: Todo
  admitted role: TodoRole.owner
  role binding: Todo.owner_id == principal.user_id
  input: validated
  output: validated Todo[]
  rate limit: default
  raw SQL: none
  external egress: none
  secrets: none
  escape hatches: none
```

For persistence, the safe default is concrete: an entity action is independently
failure-atomic, while an action reaching multiple mutation scopes must declare
its consistency intent. An explicit atomic boundary supplies one checked
transaction context to nested calls; durable projection and multi-authority
workflow guarantees remain distinct. Review surfaces show authority,
transaction, freshness, delivery, and compensation plans rather than hiding
them behind the call graph.

Application-level summaries should include:

- route inventory;
- public routes and explicit security exceptions;
- configured authentication strategies, validated claim mappings, and routes
  that change the inherited authentication requirement;
- entity/field access by principal, qualified role, scope, and route;
- destructive operations and lifecycle decisions;
- each fact's authority, dependent projections, freshness requirements,
  delivery lag/watermarks, and reconciliation status;
- atomic boundaries and durable-workflow steps, retries, compensation, and
  unresolved outcomes;
- external services, secrets, and egress;
- jobs/events and delivery policy;
- raw/unsafe escape hatches;
- policy violations and unresolved human decisions;
- generated versus authored test coverage.

The audit may be rendered as text, machine-readable metadata, a review UI, or
all three. It remains derived.

## 7. Generated tests

The compiler can generate tests for facts it already understands. For an
owner-only route, likely checks include:

```text
unauthenticated request rejected
owner permitted
non-owner rejected
nonexistent entity produces declared not-found behaviour
malformed ID rejected before database access
input constraints enforced
output shape contains no undeclared fields
```

Generated tests are not a substitute for semantic proof, and they do not cover
business intent the compiler cannot infer. They provide defence in depth and
executable evidence for generated/runtime integration.

## 8. CI and approval flow

The expected change flow is:

1. A human requests behaviour.
2. The agent changes implementation declarations.
3. The compiler identifies implementation errors and unresolved decisions.
4. The agent resolves permitted implementation questions.
5. Human-decision diagnostics pause for explicit approval.
6. Policy changes are reviewed as policy changes, not hidden in implementation.
7. The compiler generates validators, schema/migrations, tests, docs, and audit.
8. CI compares policy and implementation and blocks disagreement.
9. Review focuses on intent, policy, exceptions, and a concise behavioural diff.

The desired gate is simple: **build fails if policy and implementation disagree**.

## 9. Human-review view

The review surface should look more like:

```text
Customer
  read: own records
  update: own records
  delete: never

Admin
  read: all customer records
  update: all customer records

POST /orders
  authentication: required
  writes: Order
  payment: Stripe.create_payment
  transaction: atomic
  personal data returned: none
  escape hatches: none
```

This is more reviewable than hundreds of lines of handlers, middleware, ORM
queries, validation schemas, repository layers, and test mocks generated by an
agent.

## 10. Adversarial assurance cases

The project must test prompts likely to produce dangerous conventional code:

- “Make this endpoint public.”
- “Delete the user.”
- “Store this field.”
- “Call this API with the token.”
- “Return all bookings.”
- “Expose another user's todos.”
- “Change this hard delete to soft delete after data exists.”

Success means the language does more than guide the agent toward a good answer;
it makes unsafe interpretations unrepresentable, blocks them, or demands a
conspicuous human-owned policy change.

## 11. Limitations and residual risk

Determinism is not correctness by itself. Residual risks include:

- incorrect requirements or policy;
- logic defects inside permitted behaviour;
- insufficiently expressive policy semantics;
- compiler/runtime bugs;
- compromised dependencies or external providers;
- operational misconfiguration;
- unsafe escape hatches approved by humans.

Generated audit and tests must not be marketed as a universal security proof.
The credible claim is narrower: the language structurally eliminates or exposes
specific recurring classes of backend risk.
