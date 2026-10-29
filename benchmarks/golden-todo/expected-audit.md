# Expected derived audit

**Contract:** `todo-v0.1`  
**Status:** candidate expected output; never an authority over policy or source

This document fixes the minimum human-readable evidence the compiler must derive
from the golden application. Exact layout may improve, but facts and uncertainty
classifications may not disappear.

## Application summary

```text
TodoApplication
  routes                         8
  public routes                  1
  protected routes               7
  credential slots               2
  authentication validators      4
  entities                        2
  jobs                            2
  external services              1
  declared secrets               3
  raw SQL                         none
  undeclared egress               none
  escape hatches                  none
  unresolved human decisions      none in frozen policy
```

The audit must distinguish `proved`, `runtime validated`, `generated-test
supported`, `operationally enforced`, `assumed`, `unsupported`, and `cannot
prove`. A green summary may not collapse those categories into one claim.

## Authentication

```text
default                           required, bounded 5m
credential slots                  todo_session cookie, Authorization bearer
validators                        signed_session, signed_user_bearer,
                                  service_api_key, jwt_bearer
selection                         exactly one presented credential; one validator
conflicting identities            reject
invalid beside valid              401 ambiguous_credentials before validation
privilege merging                 never
application principal             Principal.user | Principal.service; identity only
fresh-authority routes             user disable, credential exchange
disabled users/services            reject at fresh/refresh/exchange checks
bounded existing credentials        rejected no later than expiry, at most 5m
fresh principal-authority queries    exactly 1
credential-authority queries         recorded separately from principal queries
permissions in credentials         never
provider objects in business code none
jwt dependency                     one pinned direct package, zero transitive
```

## Routes

| Route | Access | Reads | Writes/effects | Policy scope | Output |
|---|---|---|---|---|---|
| `GET /health/live` | public, explicit exception | none | none | public exception | `PublicHealth` |
| `POST /todos` | authenticated user | current user | create `Todo` | owner forced from principal | `TodoView` |
| `GET /todos` | authenticated user | `Todo` | none | `owner_id == principal.user.id`, active only | `TodoPage` |
| `GET /todos/:todo_id` | authenticated | `Todo` | none | owner, active only | `TodoView` |
| `PATCH /todos/:todo_id` | authenticated | `Todo` | update supplied public fields | owner, active only | `TodoView` |
| `DELETE /todos/:todo_id` | authenticated | `Todo` | soft delete | owner, active only | none |
| `GET /users/:user_id/todos` | authenticated | `User`, `Todo` | none | self and owned children | `UserWithTodos` |
| `DELETE /users/:user_id` | authenticated user, fresh authority | `User` | disable user | self | none |

For each route, the machine-readable audit must link the policy rule, query or
mutation predicates, reachable failures, validator, transaction boundary,
acceptance cases, and responsible source spans.

Readiness is a deployment-plane probe, not an application route. It performs a
bounded database check, exposes only safe dependency status, and is not
reachable through either application credential slot.

## Data access and lifecycle

```text
Todo.owner_id
  relationship                     required -> User.id
  reassignment                     prohibited
  parent hard delete               restrict

Todo.delete
  operation                        soft delete
  read/update after deletion       concealed as TodoNotFound
  retention                        TodoConfiguration.soft_delete_retention
  hard purge                       purge_deleted_todos, bounded 500/run
  restore                          prohibited

User.disable
  operation                        soft state transition active -> disabled
  child todos                      retained
  later fresh/refresh authority     rejected; no new credential issued
  ordinary bounded credential       rejected at expiry, no later than 5m
```

`GET /users/:user_id/todos` must expose an exact query plan: one self-scoped
parent lookup plus one ordered child query, with no N+1 path and no
cross-parent mixing. Apply owner and soft-delete filters in SQL before the
100-child cap. Its `UserWithTodos` projection contains `user_id` and at most 100
ordered `TodoView` values. It must not expose `User.email`, which remains
provisioning-only under the human-owned policy. The migrated route now uses the
existing indexed keyset-page lowering to meet those conditions; focused SQLite,
PostgreSQL and JWT HTTP evidence is recorded under RM-107.

## Patch semantics

```text
title omitted                      unchanged
title supplied                     replace
status omitted                     unchanged
status supplied                    replace
due_at omitted                     unchanged
due_at supplied Instant            replace
due_at supplied none               clear
due_at supplied                    clear reminder_sent_at for the new schedule
zero supplied fields               reject EmptyPatch
owner/generated/lifecycle fields   not representable in PatchTodo
```

## Jobs and external effects

The following job wording describes the retained pressure source. The frozen
2026-10-02 lifecycle/service successors require
clause-bound generated retention maintenance, a durable UUID per reminder
schedule, private intent state, revision guards and compiler receipt observation.
Future generated audit must represent those reviewed successors and distinguish
provider evidence from application time; the historical `Todo.id` key below is
not the new service identity.

`overdue_reminders` reads at most 500 open, undeleted, unreminded todos whose
due date is strictly before the injected clock, in deterministic order,
traverses the required owner relationship, sends only through `ReminderMail`,
uses `Todo.id` as the idempotency key, and records success before the todo is
eligible again. The audit must mark the service-call/database-update atomicity
gap as a runtime idempotency guarantee, not a database transaction proof.

`purge_deleted_todos` performs bounded hard deletion only after the configured
retention interval. It has no external effect.

## Secrets and egress

| Secret | Permitted sink | Public output | Ordinary log |
|---|---|---:|---:|
| `database_url` | database adapter credential | prohibited | redacted |
| `session_signing_key` | session verifier | prohibited | redacted |
| `mail_api_key` | `ReminderMail` credential | prohibited | redacted |

The only declared network egress beyond the database adapter is
`mail.example.invalid:443`. Derived evidence must show configuration provenance
without values.

## Generated versus authored evidence

The compiler should generate structural tests for authentication defaults,
boundary validation, owner scoping, safe output projection, concealed
not-found, relationship integrity, configuration redaction, and service egress.
Authored black-box cases own due-date business rules, patch omission, stable
pagination, reminder eligibility/idempotency, disablement, retention, and the
combined cross-boundary behaviours in `acceptance.json`.
