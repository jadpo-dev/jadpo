# Golden todo language-friction ledger

**Snapshot:** the archived v0 compiler at `df2d7be`, 2026-09-30

**Status:** a record of where the golden todo pressed on the language, not
permission to weaken the application. The dispositions below describe the
archived compiler; the decided language answers these gaps on the
[language pages](../language/README.md), and roadmap Phase 10 writes
the application in it.

The v0 compiler could not check the original `app.jadpo` or `policy.jadpo`
in full. Each gap below needed a minimal fixture and a language-issue
disposition before implementation. The original failed design pressure remains
evidence even after support is added.

| ID | Required construct | Disposition at the snapshot | Issue |
|---|---|---|---|
| GF-001 | application declaration and secure authentication default | bounded application and closed user/service principal declarations plus protected `POST /todos` source migrated and checked; target generation and golden runtime integration remain open | `AUTH-001` |
| GF-002 | browser, API, and service strategies normalised to one closed principal | signed browser/API, JWT user, and service API-key validators with active-user/service authority resolution and explicit service-credential owner mapping are migrated; accepted browser-origin configuration, authored golden issuance/exchange operations, and integrated execution remain open (the generated runtime already supports issuance/exchange) | `AUTH-001` |
| GF-003 | exactly-one credential selection and conflicting credential rejection | exactly-one selector implemented and adversarially tested; golden integration pending | `AUTH-001` |
| GF-004 | closed user/service principal and authoritative active-principal lookup | bounded closed principal and active-user/service authority resolutions are migrated; integrated target/runtime evidence remains open | `AUTH-001` |
| GF-005 | enums, qualified defaults, and boundary decoding | closed enum/tagged-sum typing and boundaries implemented; `default none` is supported on nullable input fields, while entity creation defaults must be assigned in migrated actions and broader default evolution remains open | `TYPE-006` |
| GF-006 | typed configuration, secrets, provenance, startup validation | typed golden configuration, secret markers, and retention default are migrated and checked; startup/runtime validation, service sinks, and dependency readiness remain pending | `CONFIG-001` |
| GF-007 | compound non-unique indexes | missing persistence capability | `DATA-003` |
| GF-008 | output projection from entities | explicit typed output projections implemented; migrate superseded golden projection syntax | `QUERY-001` |
| GF-009 | path and query parameter declarations | typed path bindings implemented with brace placeholders; typed query decoding remains open | `ROUTE-001` |
| GF-009A | typed success semantics for created and no-content routes | implemented in route syntax, type checks, Bun responses, route inventory, and OpenAPI; golden integration remains pending | `ROUTE-001` |
| GF-010 | cursor pagination with compound stable ordering | missing query capability | `QUERY-001` |
| GF-011 | optional predicate composition | missing query capability | `QUERY-001` |
| GF-012 | omission-aware patch application | implemented for direct optional-field patch inputs and migrated into the Todo patch action/route; integrated golden execution pending | `DATA-002` |
| GF-013 | explicit supplied-field inspection | typed empty-patch rejection and supplied-triggered derived sets implemented; general supplied-expression inspection remains open | `DATA-002` |
| GF-013A | nullable input default used to make create-time `due_at` omissible | `default none` is parsed, type-checked for nullable input fields, and materialized by generated input validation; the golden runtime case remains unexecuted | `DATA-002` |
| GF-013B | patch-dependent derived write clears `reminder_sent_at` when `due_at` is supplied | implemented as fixed derived `set:` with `when input.due_at supplied` and migrated into the Todo patch action; integrated golden execution still depends on other gaps | `DATA-002`, `ASYNC-001` |
| GF-014 | optional and required relationship traversal with filters | direct traversal plus a bounded owning-parent/optional-inverse depth-two path are implemented; relationship filters and other nested shapes remain scheduled | `DATA-001` |
| GF-015 | one-to-one/required parent include spelling | explicit `references User.id as owner` naming plus unique-backed `inverse ...: optional ... via ...` are implemented; required inverse-one semantics and composite-reference pressure remain scheduled | `DATA-001`, `DATA-004` |
| GF-016 | policy declarations and proof obligations | direct Todo owner binding/effects, identity-bound `UserRole.self` read scope, and protected Todo read/create/patch source are migrated and checked; integrated route policy evidence, User disablement/soft-delete lifecycle, remaining protected operations, and approval remain open | `POLICY-001`, `DATA-007` |
| GF-017 | concealed authorization/not-found equivalence | concealed denial has runtime evidence; exact integrated golden ownership cases remain unexecuted | `FAIL-002`, `POLICY-001` |
| GF-018 | clock capability and deterministic test injection | operation clocks, timestamps and deterministic fixed/advanceable fixtures implemented; golden integration pending | new `TIME-001` |
| GF-019 | service contract, egress, secret injection, failure mapping | deferred | `SERVICE-001` |
| GF-020 | job schedule, bounds, concurrency, retries, and idempotency | deferred | `ASYNC-001` |
| GF-021 | bounded iteration syntax | provisional, no resource rule | `TYPE-002`, `ASYNC-001` |
| GF-022 | service call followed by durable success marker | unresolved effect/delivery semantics | `ASYNC-001` |
| GF-023 | bounded batch hard delete | missing persistence capability | `DATA-003`, `ASYNC-001` |
| GF-024 | soft-delete migration after existing data | schema identity and reviewed bounded SQLite rebuilds implemented; golden-specific lifecycle migration still needs a reviewed plan | `DATA-003` |
| GF-025 | generated structural tests and authored clock/service fixtures | authored tests, typed config/clock fixtures and isolated SQLite state implemented; route/job activation and service/auth fakes remain open | new `TEST-001` |
| GF-026 | modules/imports for a real multi-file source tree | bounded explicit-module/selective-import core implemented; golden split and advanced namespace features pending | `MOD-001` |
| GF-027 | approval-bound human-owned policy artifact | P10R protocol required | `POLICY-001` |
| GF-028 | deployment-plane readiness distinct from application routes/auth | missing operational capability | `CONFIG-001` |
| GF-029 | public liveness response with no dependency checks | static public liveness route is migrated with no configuration or dependency reads; ambient-credential, outage, and exact golden runtime cases remain unexecuted | `CONFIG-001` |
| GF-030 | rename-stable schema/migration identity | rename-stable identity and reviewed migration plans implemented; golden migration evidence pending | `TOOL-001`, `DATA-003` |
| GF-031 | first-class entity dossiers, named reads, entity-owned mutations, and multi-entity workflows | compiler-checked migration contains four persistent dossiers, supported values, static liveness, Todo owner policy, identity-bound User self-read, Todo by-id read, and Todo create/read/patch routes; patch runtime, User disablement/lifecycle-aware operations, full source integration, and durable external workflows remain open | `DATA-007`, `QUERY-001`, `LAYOUT-001` |

| GF-032 | golden invalid-input status and code contract | golden CREATE-002/READ-003/LIST-004 require 422 with specific codes; current target uses 400 invalid_request; preserve candidate assertions until explicit boundary-contract disposition | `ROUTE-001`, `FAIL-002` |

## Required order

The first implementation package should migrate source to accepted entity,
named-query, policy and authentication forms, then build golden configuration
and public liveness fixtures (AUTH-012, PUBLIC-001, CONFIG-001/002). Follow with
browser/signed-user authentication and owner-scoped read projections and patches.
The existing `clock.now` spelling is supported; do not invent a replacement.

Typed path routes, enums, selector behavior, configuration and clocks already
have evidence in the compile fixtures and runtime suites. Consult
`authentication-selector.test.ts`, `first-party-authentication.test.ts`,
`entity-dossier.test.ts`, `policy-runtime.test.ts`, `temporal.test.ts`, and the
validation auth/persistence/config/artifact suites before implementing duplicate
features. Existing behavior is not proof that the complete golden cases passed.

Keep created/no-content route results, query parameters/cursors, general input
defaults beyond `none`, JWT/service credentials, readiness and job/service
semantics as explicit gaps.
No partial slice may remove those declarations or obligations from the complete
candidate, relabel unrelated unit tests as full golden execution, or replace
its 422 boundary assertions merely because the prototype currently returns 400.

The bounded value/reference item is now closed without adding a golden-todo
capability: ordinary parameters and returned data are immutable values, and
`var mut` permits only type-compatible lexical rebinding. The golden design presents
no need for caller-visible mutation, so `inout`, references, field assignment,
and observable aliasing remain absent rather than speculative.

No gap is resolved by embedding unchecked TypeScript in authored source. If a
constrained escape hatch becomes necessary, its authority, data/effect surface,
and audit visibility require a separate human decision and adversarial case.
