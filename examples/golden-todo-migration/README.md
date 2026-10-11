# Golden todo migrated source

This package is the compiler-checked successor source for the frozen
[`todo-v0.1` candidate](../golden-todo/README.md). The candidate's
[`acceptance.json`](../golden-todo/acceptance.json) and human-owned
[`policy.jadpo`](../golden-todo/policy.jadpo) continue to define the required
behaviour and permissions. The owner-authorised
[acceptance revision](../golden-todo/REVIEW.md#2026-10-02-owner-authorised-userwithtodos-email-revision)
omits provisioning-only email from the self-service `UserWithTodos` response;
it leaves the human-owned policy unchanged.

The original [`app.jadpo`](../golden-todo/app.jadpo) remains the preserved
pre-DATA-007 pressure source. New implementation source belongs here and uses
the accepted entity dossier layout: one authoritative entity per file under
`entities/`, with value declarations under `values/`.

## Current slice

The four persistent domain entities (`User`, `Service`, `ServiceCredential`,
and `Todo`) now have explicit identity, authority-store persistence, unique
keys, and declared ownership references. Their scalar fields and enums use the
candidate's declared types. The values role contains the supported patch and
disable inputs, cursor and output projections, health response, reminder
message/receipt, and shared enums/refinements.

The Todo UUID identity is compiler-generated, so the migrated create action
does not accept or assign it. Creation timestamps remain lifecycle-generated
on the entity. `CreateTodo` declares the frozen nullable `due_at` omission
default as `default none`, and the input validator materializes it before
application code runs. `ListTodos` stays in the original pressure source
until typed URL query decoding, optional predicate composition, cursor
pagination, and page-size defaulting can be migrated without changing its
contract. The typed configuration, bounded application principal, signed
browser/user bearer and JWT user strategy declarations, active-user authority
resolution, and static public liveness route have also moved here. Browser
authentication now requires `BROWSER_ORIGIN` and binds it to the signed cookie
validator under the [owner-authorised revision](../golden-todo/REVIEW.md#2026-10-01-browser-origin-configuration-revision).
The original candidate is preserved. The generated protected-browser suite now
proves origin/CSRF behavior for the migrated target, with [RM-103 closure evidence](../../docs/archive/implementation-history.md#2026-10-01--rm-103-browser-origin-and-csrf-closure). RM-106 now applies the accepted lifecycle to User and Todo, with fresh-authenticated self-disable and owner-scoped soft-delete routes. SQLite and PostgreSQL HTTP evidence, including concealed reads/mutations, retained child rows, bounded credentials and a patch/delete race, is recorded in the [runtime suite](../../tests/runtime/README.md#generated-runtime-acceptance-tests) and [lifecycle plan](../../docs/lifecycle-plan.md#rm-106-golden-lifecycle-route-checkpoint-2026-10-03). Its independent auth/lifecycle review approves this scoped integration ([history](../../docs/archive/implementation-history.md#2026-10-03--rm-106-golden-lifecycle-routes)); remaining protected operations/routes, workflows, jobs and full frozen golden acceptance remain open. The User identity now binds `UserRole.self`,
preserving the frozen `principal.user_id == User.id` self-scope; only `id` is
readable under this role, while provisioning and lifecycle fields have empty
field policies. `Todo.owner_id` binds the accepted `TodoRole.owner` role and
the entity matrix grants that role `create`, `read`, `update`, and `delete`.
`Todo.by_id` now uses an authoritative required ID query, so the normal entity
read policy applies automatically; a soft-deleted result maps to the same
`TodoNotFound` failure before the existing `TodoView` projection is returned.
Entity action `Todo.get` and authenticated `GET /todos/{todo_id}` now call this
query, and the checked call graph derives its concealed 404 failure. The create
action preserves future-only due dates, sets the authenticated owner's ID and
initial open state, maps storage conflicts, and projects generated IDs and
timestamps to `TodoView`. Its explicit `Todo.owner_id(...)` construction keeps
the identity conversion visible; compiler policy analysis accepts that wrapper
only when its value comes from the authenticated principal. The PATCH action
and `PATCH /todos/{todo_id}` route preserve empty-patch rejection, reject past
due dates, clear `reminder_sent_at` whenever `due_at` is supplied, and call the
authoritative `Todo.by_id` lookup before the entity-scoped update. This source
path relies on automatic `TodoRole.owner` scoping; its integrated behavior and
atomic interaction with future soft-delete lifecycle work remain open. Source
checking now includes the self-scoped `GET /users/{user_id}/todos` route. Its
named query reads the user once, then runs the existing indexed keyset-page
query with owner and soft-delete filters applied before the 100-item bound. The
page lowers directly to `TodoView`, and `UserWithTodos` includes only the user
ID and those projected todos; `User.email` is not read into the response.
SQLite SQL tracing confirms two application data reads, and PostgreSQL exercises
the same ordered, capped result. The JWT HTTP test supplies a conflicting email
claim and confirms it is absent from the response. These are focused RM-107
route cases; the frozen 44-case source-bound gate and independent policy review
remain open. Source checking passes eleven files and 61 declarations. Protected `POST /todos`
passes `current_principal.user` to the create action; protected route variants
are enforced by the first-party runtime. The selected authentication
adapter/configuration/principal combination now lowers to a generated target,
including the separate credential and service identity binding. The API-key
validator binds the credential's identity, service reference, verifier, active
state, expiry and revocation fields explicitly. Its owner is `Service.owner_id`;
resolution uses the unique nonsecret `Service.name` and stable `Service.id`.
The service signed validator and strategy-level `exchange` declaration lower
to a compiler-owned `POST /auth/exchange`; the endpoint returns only a new
bounded bearer, its type and exact expiry. Trusted host provisioning still owns
one-time key issuance, and verifier material is never a principal subject. The
generated SQLite/PostgreSQL HTTP evidence is recorded in the
[RM-104 checkpoint](../../docs/archive/work-plans/golden-delivery-planning.md#rm-104-checked-exchange-decision-packet--2026-10-01);
the independent runtime/security review
approves the scoped exchange implementation. The complete frozen golden gate remains RM-109/RM-110.

Registered runtime suites execute protected user/browser paths and service
lifecycle checks, including SQLite/PostgreSQL issuance, expiry, revocation,
service disablement and rollback. See the [runtime evidence](../../tests/runtime/README.md)
and [authentication contract](../../docs/auth-runtime-extensions.md).
Authored key issuance, broader lifecycle mutation and the integrated acceptance
harness remain separate work. The frozen human-owned source policy is unchanged.
No frozen acceptance case is counted as executed.

Check this source with:

```sh
jadpo check examples/golden-todo-migration
```
