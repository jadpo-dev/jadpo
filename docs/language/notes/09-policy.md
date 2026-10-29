---
title: Policy, design notes
area: 9
---

# Policy: design notes

Why [Policy](../09-policy.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Related areas

| Item | Area |
|---|---|
| Who schedules and subscribers act as (the system caller), and how policy applies to them | 12 |
| How schedules and subscribers write a `FieldAccess.server_only` field, which no request may | 12 |
| What a subscriber stores from an event's payload, checked against what every `emit` of it put there | 12, "Subscribers" |
| Testing policy: calling an action as a given user and expecting `not_found` or `not_permitted` | 13 |
| The permissions report (`build/report/permissions.md` and `.json`) and its place in the approval review | 14 |
| The protected sign-off that stops an unaccepted release | 14 |
| Personal-data marking, erasure, export, retention and log redaction | 15 |

## Questions

1. **Which permission model?** Grants are keyed by what you do (`create`,
   `read`, `update`, `delete`); "who" is a user field on the record, a
   membership role, or `Access.authenticated` / `Access.public` (and
   `Access.system` for background work); role enums exist only beside the
   membership that stores them; a user field named in policy cannot be
   changed by an ordinary update; field restrictions use the same shape,
   with `FieldAccess.server_only`; the global view is the generated
   permissions report; root `policy.jadpo` keeps only tenancy and
   `actions:`.
2. **Is policy always on?** Yes. Every reachable create, read, update or
   delete needs a grant from the first line of code, so an application
   cannot ship with policy forgotten. Ash refuses by default too; Supabase
   row-level security only once it is switched on per table, a known
   source of leaks.
3. **How is tenancy declared?** `tenant: Company` in root `policy.jadpo`.
   Every entity then needs one reference to the tenant, or names it with
   `tenant: source_company_id` if it has two, or says
   `tenant: Tenancy.shared`; every grant applies only inside the caller's
   tenants, `Access.authenticated` included. This is the one setting that
   stops a new entity leaking across customers. Rails' `acts_as_tenant` and
   Supabase policies do the same per model or table, where it can be
   forgotten.
4. **Where does permission to call an action live?** Actions have no
   settings (area 3): every action takes its permission from the entities
   it touches, and the few that touch none, or need different people, are
   listed in root `policy.jadpo` under `actions: { domain.action: [...] }`,
   with the same kinds of who. One place answers "which actions are
   exceptions?".
5. **How does a grant choose between two references to a membership's
   scope?** An entity such as a team handover has `from_team_id` and
   `to_team_id`, both `Team.id`, so `TeamRole.lead` alone is ambiguous. The
   grant names the field in brackets, `TeamRole.lead(from_team_id)`, read as
   "lead of the from-team", as area 3 names a variant's data in brackets;
   only when there are two references, and the compiler says so otherwise.
   A `scope:` member for the whole policy couldn't grant leads of either
   team.
6. **How does a grant require an OAuth scope?** Scopes from any token
   narrow what policy allows (area 8). A `scopes:` member in the entity's
   `policy:`, keyed by the same four names,
   `scopes: { read: OAuthScope.todos_read  update: OAuthScope.todos_write }`,
   names them. `OAuthScope` is a standard type (RFC 6749's scope-token), and
   the enum generated from the scopes declared in `authentication.jadpo`
   narrows it to the application's own, a closed subset. A caller whose
   token carries scopes needs both a grant and the scope; signed-in
   browsers and service keys are unaffected. An effect with no scope listed
   is refused to scoped tokens, so a new entity is never exposed to other
   applications by accident; an `external_token` method narrows only if it
   declares the scopes it reads. It is a separate member because grant
   lists mean "any of" and a scope must hold as well. GitHub and Google
   scopes work the same way: the token never exceeds the person. Not built
   yet.
7. **Is a policy refusal listed in `fails`?** No. Like `unauthenticated`,
   `not_permitted` from policy is raised by the runtime with a fixed code,
   and the API description lists the 403 wherever it can happen. A business
   refusal an action raises itself is still an ordinary
   `kind: FailureKind.not_permitted` failure.
8. **Who may issue, list and revoke service keys?** The `Service` record's
   own grants: `update` on the service to issue or revoke a key
   (`Service.issue_key`, `Service.revoke_key`), `read` to list them
   (`Service.keys`). A service owned through `owner_id` is then managed by
   its owner, with no new concept.
9. **Who may call an integration?** Whoever may run the action that calls
   it; there is no separate grant. An action that only calls an integration
   touches no entity, so it is listed under `actions:` anyway.
10. **Is a policy weakening accepted with area 7's shared acceptance
    form?** Yes: one acceptance file per change in `acceptances/`, the same
    form as breaking API changes, database drops and new domains.
11. **Who owns personal-data marking and the classification of internal
    fields for log redaction?** Area 15, Personal data. Erase maps to the
    `delete` grant and export to `read`.

## Decisions

| Topic | Decision |
|---|---|
| Policy protection | Carried from area 1: through the approval check wherever policy lives, not by file path |
| Root file | Carried from area 1: `policy.jadpo` holds application-wide policy, such as tenancy; it is an unnamed singleton |
| Action settings | Carried from area 3: actions have no settings; who may call an action is decided by policy |
| Colon rule | Carried from area 3: sections are `name: { ... }` (`policy: { }`, `membership: { }`) |
| Setting options and lists | Carried from area 2: fixed options are enum values written `Type.value`; lists are `[ ]` |
| Concealment | Carried from area 4: a record hidden by policy reports the same `not_found` failure; the log records the concealment |
| Caches | Carried from area 5: caches serve display reads only; keys include tenant and policy inputs; reads inside policy never use a cache; erasure wipes cached copies |
| Placement | Carried from areas 1 and 5: an entity's policy is in its file; actions and queries are in their own folders |
| Policy in queries | Carried from area 6: policy is compiled into each query's `WHERE`; fields a policy condition filters on get indexes; row-level security later, opt-in |
| The caller | Carried from area 8: policy gets the caller from the runtime; `current_principal` exists in routes only; principal fields are `id`, `subject`, `strength` |
| Public routes | Carried from area 8: a route opts out with `authentication: RouteAuthentication.none` |
| OAuth scopes | Carried from area 8: scopes from any token (`oauth_token`, `external_token`) narrow what policy allows |
| Background work | Carried from area 12: schedules and subscribers act as `Access.system`, which an entity grants explicitly; subscribers stay in their event's tenant |
| Where a record belongs | `scope:` takes a field or `MembershipScope.application`; an entity's `tenant:` takes a field (when it has two references to the tenant) or `Tenancy.shared`. Both read as "where this belongs", so each setting takes either kind of value |
| Who a grant names | A field names the caller it points at: a reference to the principal's user entity (or its service entity, which grants that service), or that entity's own `id`, the caller themselves (`read: [id]` on the user entity). A role is `Role.value`, held by a membership whose `role:` field is typed with the enum, or `Role.value(field)` naming the reference it applies through; role enums are imported like any type. Anything else is `policy.binding_invalid` |
| Memberships | `membership: { scope: member: role: }`: `scope:` a reference or `MembershipScope.application`, `member:` a reference to the user entity, `role:` a field typed with a plain enum. A role applies to a record through its one reference to the membership's scope, through `id` on the scope entity itself, and everywhere for an application-wide membership; with no reference it is `policy.scope_missing`, with two `policy.scope_ambiguous`. A membership the lifecycle hides grants nothing |
| Tenancy | Every entity has one reference to the tenant, names it with `tenant: field`, says `tenant: Tenancy.shared`, or is the tenant itself (through `id`); otherwise `policy.scope_missing` or `policy.scope_ambiguous`. A record belongs to a caller's tenant when the caller holds a current membership, of any role, in a membership entity whose scope is the tenant; `Access.public` on a tenant's records therefore reaches only members. Creating a tenant's own record checks no tenancy, since nobody belongs to it yet, and a new membership in the tenant makes its member one only while nobody belongs to the tenant yet, founding it, so nobody joins someone else's tenant, as a member or its owner, on their own say-so. Root `policy.jadpo` imports what it names, like any root file (`use work/entities { Company }`, `use work` for `work.found_company`); a tenant no membership is held in is `policy.role_unbound` |
| Fields policy fixes | An ordinary update can't change a field a grant points at the caller through, a reference a role or the tenant applies through, or a membership's `scope:` and `member:`, since each hands the record to someone else (`type.field_update_forbidden`); an action listed under `actions:` may. A field marked `immutable: true` never changes |
| Checking routes | Once the routes and the policy read cleanly, each route is checked against what it reaches through every action and query it runs: an effect whose list names nobody but `Access.system`, or an action that reaches no entity and signs no one in or out and isn't listed, is `policy.effect_ungranted`; a route anyone may call reaching what `Access.public` isn't granted, or a tenant's records, is `policy.public_grant_missing`. One problem per route, at the first it reaches |
| When policy says no | A list is filtered in its statement, before paging. A `get`, a query for one record, and an update or delete by `id` that policy hides report the code's own not-found failure, and the log says `concealed_by: policy`: a `get` and a query for one record select the grant in their own statement, so a hidden record costs no second one, and an update or delete by `id` asks a second statement without policy. A create policy refuses, a change to a record the action loaded, and a call an action's line refuses are 403 `not_permitted`, raised by the runtime and never in `fails`. OpenAPI lists the 403 on every route that runs an action that writes or is listed |
| Listed actions | An `actions:` line replaces its entities' grants on the creates, updates, deletes and transitions in the action's own body; its reads, and the actions and queries it calls, keep their own entities' grants, with the same caller. `Entity.field` names a field of that entity; a line that depends on no record (anyone signed in, or a role held across the whole application) is also checked as the action starts, wherever it is called from |
| Tokens and keys | `sessions.start`, `sessions.end` and `sessions.issue_token` need no grant: who may sign in is the application's own check (area 8), and an action that only signs in or out isn't one that reaches nothing. `sessions.tokens` and `Service.keys` list only holders the caller may `read`; `sessions.revoke_token`, `Service.revoke_key` and `Service.issue_key` need `update` on the holder. A refused list is empty, a refused revoke revokes nothing, and a refused issue is 403 `not_permitted`. A `GET` route that runs `sessions.tokens` or `Service.keys` itself reaches the same `read` (area 8) |
| Calling as someone in a test | A call a test, fixture or seed makes runs as the `Principal.user` or `Principal.service` it passes, so policy applies as to that caller; a call that passes none runs without policy, so an action test proves behaviour and data and a route test, or a call with a caller, proves policy (area 13). Nothing a request runs leaves policy out |
| A field's own policy | `policy: { create: read: update: }` on a field narrows those lists, and an effect it leaves out follows the record's; `delete:` is `policy.effect_unknown`, since a field goes with its record. Its entries resolve like the record's, and each must be one the record's list for that effect lets in: the same entry, any entry under `Access.public`, or a user field or role under `Access.authenticated` (`policy.field_widens_entity`). `FieldAccess.server_only` lets no request read or write the field. A user field or reference a field's grant reaches the caller through joins the fields only a listed action changes |
| Writes to a restricted field | The write's statement holds the field's grants beside the record's, with tenancy once: a create's for each field it sets (none in an optional field is never refused), an update's `set:`, a patch's field when the request supplies it, and a transition's fields. Writing no row asks again under the record's grants: found, the field refused the caller and the whole write is 403 `not_permitted`, by `id` or not. A listed action's line replaces the record's grants but never a field's. A route whose action always sets a field nobody may change (an update's `set:`, or a create of a required field) is `policy.effect_ungranted` |
| Proving outputs | Who may read a value is proved from the grants alone, never the data. A query's records are reached through its entity's `read:`, a child's through its own, and a parent's both along the reference (the user a field points at is the user it includes; a role applies to the scope its reference points at; a membership's member holds a role in its scope, which proves a list naming every role) and through the parent's `read:`; a listed action whose line depends on no record adds that line, and an update's or create's record the field grants of what it sets. A value is proved when every grant of one way the caller came proves the field's `read:`. Every parent an include reads is proved readable, or the query is `policy.output_field_unproved` at the include; an output field the query's type carries, at its result type. In actions, values are followed through variables, objects, lists, `match`, `list.filter` and `.map`, and calls (an action's parameter that reaches an output it builds is checked where it is called); a function's result carries all its arguments. An `Output` built, and a failure's `client:` field, are checked where the value is given |
| User fields on creates | When every entry of an entity's `create:` is a user field, a create in an action that isn't listed under `actions:` sets at least one of them from a `Principal.user` or `Principal.service` the action takes (directly, or through a variable or object); otherwise `policy.role_field_create_forbidden` at the first such field's value. The field is set from the caller on every path: every value it may hold, whatever an `if` or `match` decided, is the caller's `id`. Fixtures, tests and seeds aren't checked: their calls without a caller run without policy |
| Policy in the graph | A `grant` node per list an entity's `policy:` writes (`Project.read`, its effect and who as written) with a `grants` edge to the entity; a `field_restriction` node per field's own policy (`Project.budget`) with a `grants` edge to the field; a `tenancy` node per entity that belongs to the tenant (its tenant and reference) with a `scopes` edge to the entity; an `action_exception` node per line under `actions:` (`work.found_company`) with a `grants` edge to the action. The permissions report's JSON lists each field's own lists under `fields` |
| The permissions report | `jadpo check` writes `build/report/permissions.md` and `.json` once policy reads cleanly: a table per entity, `domain.Entity`, with each list as written or "nobody", a line on its tenancy, its membership and the fields only listed actions change, and a table of the actions listed separately; no times or totals, so a change reads as a one-line diff |
| Keeping references inside the tenant | The check is a condition in the write's own statement, `NOT EXISTS` a record with that `id` in another tenant, so it costs no round trip, and its unpoliced twin carries it too: what the data is holds for a test's or a seed's call without a caller as well. A record that isn't there passes, so its foreign key still reports the entity's not-found failure; one in another tenant is 403 `not_permitted`, a create's or, for an update, once the record is found under its grants. It covers references between two entities that belong to the tenant, the tenant itself included as one that belongs to itself; a reference to the tenant is the record's tenancy, or beside the one `tenant:` names, another tenant on purpose, and isn't checked. A write that changes a record's tenancy (a listed action) checks every reference it keeps and that no record of the old tenant refers to it. The record it points at is found by its primary key and the records referring to it by their reference's own index, so no index or migration is added. In proofs, a role held through the record's tenancy, in a membership scoped to the tenant, holds through the parent's, and membership of the tenant moves along the reference |
| Conditions and orders | Checked like outputs against how a caller reached the records: a query's `where:`, `where_present:` and `order:` (the tie-break the compiler adds isn't written, so isn't checked) through its entity's `read:`, an include's through its own entity's, a `get`'s `id`, and a transition's `when:` through the grants its write checked. A lifecycle's `visible:` isn't one: it decides what exists for every caller alike. One problem per field and section, at the field where the condition or order names it |
| Nearest and distance orders | A `nearest` order (area 21) and a `geo.distance` one (area 20) stand in place of `order:` and are orders all the same: the field each measures is checked like any field an `order:` names, `policy.output_field_unproved` at the field where the order names it, so the order nearest records come in can't show a vector or a place a caller may not read. A `geo.distance` order's field is also tested by the `geo.within` line it needs, so it is reported once for each section, as a field both filtered and ordered on is |
| Founding a tenant | "Nobody belongs to it yet" counts every membership held in the tenant, of every membership entity scoped to it, those its lifecycle hides included: a tenant that has had members isn't founded again. The create's own statement checks it (`NOT EXISTS`), so it costs no round trip; the founder's membership is the one being created, which the membership's `scope:` names as the tenancy's reference |
| Moving a record to another tenant | A listed action's update that changes a record's tenancy needs the caller's current membership, of any role, in the new tenant, in the same statement, as a create there would; the line's grants are checked on the record as it was. Refused, it is 403 `not_permitted`, as a field the caller may not change is |
| Listed lines no record lets in | A user field or a role held in a record, on an `actions:` line, lets someone in only on the creates, updates, deletes and transitions of the action's own body whose records it applies to. On an action whose body writes none of them it lets nobody in: the action is checked as it starts against the line's other entries (anyone signed in, a role held across the whole application), and a line left with nobody, or with only `Access.system`, refuses every caller with 403 `not_permitted`. A route running such an action is `policy.effect_ungranted` |
| Failing closed | A gate the plan can't build for a listed line refuses every caller, and a credential statement whose policed twin is missing is a defect when a caller runs it: neither runs without policy |
| Field-refused transitions | A transition setting a field the caller may not change is 403 `not_permitted` before its `when:` can answer with its `conflict:` arm: writing no row asks the condition's question with the fields' grants as well as the record's |
| What decides a value | Functions are followed as actions are, each with a summary of what its result carries and which parts of which parameters reach what a caller is shown. Whatever an `if`'s or `else if`'s condition, a `match`'s subject, a `for`'s list or a `list.filter` predicate tests is carried by every value made under it (assigned, returned, built, or kept by the filter), by `list.count` alone, and by `list.map`'s result, and, when a `return` or `reject` inside it may have left, by every value made after it; whether a record is there carries nothing. Each field is reported once in an action, at the first value it decides. A `reject` of a failure with no client fields is checked against what decided it is raised. A failure an entity operation or another action raises isn't followed |
| Finding records | A `get`'s `id`, each argument a query is run with, and a write's target are checked like an output field, at the value: a record passed whole finds by its `id`. An include's reference is checked like a field the query reads: a parent's on the record that refers to it, a child's on the child |
| What a write gives back | An update's record is proved against its entity's `read:` as well as its fields' own, for every record its target may be (a `get`'s, a create's, one passed in or found by `id`); its `id`, and what its `set:` gave each field, are the caller's own, and a stored field an update by `id` reads is the record's. A create's record holds what the caller set and what is new, so only its fields' own `read:` apply. The values a `set:` gives stay in the record given back. The check runs whether or not any field has a policy |
| Proofs from memberships | A membership whose `role:` may be `none` holds no role a proof counts on, though it still makes its member one of the tenant's people. A membership read as an include's parent, which its lifecycle may hide, proves no role. A caller who belongs to a record's tenant passes a list naming every role of every membership held in the tenant, through the record's tenancy |
| Storing values | Every value a write stores is checked, at the value: a create's and an update's `set:`, each field of a patch's `Input` (at the patch) and what `when_supplied:` sets, which whether the request supplied its field decides, and each field a transition sets (at the write), which what decided that it runs and its `when:` decide. A field's readers are those its record's `read:` and its own `read:` let in, but `Access.system`, which is server work; a field no one else may read, `FieldAccess.server_only` among them, may hold anything. Readers are known only of the record a value is stored in, so the record it came from is reached from there by the grants' shape: the same record (one loaded and changed, one found by the `id` an update by `id` is given, or a stored field a counter reads); the record a reference the write sets points at, by its `id` or the `id` a `get` found it by, or one a `get` found by a reference the stored record holds, which nothing else the action runs, nor any action it calls, may change; a record of the same tenant, when the write sets its tenancy's reference from that record's own; and any other record only as what holds of anyone signed in, or of a role held across the whole application. A `for` loop's item is one record through its turn, and a value carried into another turn, through a variable the loop changes, is any record. A value from a parameter is checked where the action is called, against what is passed, through its summary. One problem per value; a field that only decides is reported once per action. Which record an update changes is checked against its caller ("Finding records"), not its readers, who see its `id` anyway |
| What fails decides what is stored | An action is all-or-nothing, so whatever a `reject` in its body tests decides every value it stores, before the `reject` or after it; a failure an entity operation or another action raises isn't followed, as for outputs. A `return` or `reject` inside a loop decides every turn after the one it may leave, from the top of the loop's body, for outputs and stores alike |
| Events and stores | `emit` stores nothing a caller reads: an event's subscribers are server work, as `Access.system` reading a field is, so a restricted value may go in an event's payload. What a subscriber stores from a payload is a store like any other, checked where the subscriber stores it, against what every `emit` of the event put in it |
| Background work and `Access.public` | A schedule or subscriber that reaches a statement only `Access.public` grants is a compile error: an entity grants background work explicitly with `Access.system`, so the runtime's leniency ("Background work") is never relied on. It is `policy.system_grant_missing`, at the schedule or subscriber: a grant list, a field's own list or the action's line under `actions:` that names `Access.public` and not `Access.system`, reached by its action or any it calls |
| Batches and `Access.public` | A batch whose query or actions reach a statement only `Access.public` grants is a compile error, as a schedule's or subscriber's is |
| Lines of the actions background work calls | Every action a schedule, a subscriber or a batch reaches, itself or through another, is checked against its own line under `actions:`, so a line naming `Access.public` and not `Access.system` is `policy.system_grant_missing` however deep; a listed action's line decides the creates, updates, deletes and transitions of its own body wherever it is called from, as the plan builds them, for a route as for background work |
| The tenant background work names | The tenant id an event or a batch run names must come from a record of that tenant the action read or wrote under its caller's policy, proved from where the value came from; otherwise `policy.event_tenant_unproved`, so a caller can't aim system work at a tenant they don't belong to |
| Whether a record is there, for a webhook's reply | A webhook's reply action finds every record its grants name, so what a condition, a loop or a count over records decides shows whether they are there: a reply decided by it, or a failure raised on it, needs records anyone at all may read (`Access.public` in the entity's `read:`), a record the action created being its own. Its failure goes back to whoever sent the webhook as its reply does, so what decides whether it fails (what decided a `reject` or an `attempt`, a `get` or an update or delete by `id` finding nothing, a write's `conflict:` arms) and a failure's client fields are proved as the reply is (`policy.output_field_unproved`). A route's caller still learns nothing from whether a record is there, since it is given only those it may read. A failure's client fields declared in a domain are proved for every caller too |
| The system inside a request, and a webhook's reply | The magic-link `send:` action runs with its delivery's tenant, as a subscriber's does, which for `jadpo.MagicLinkEmail` is none; a webhook's reply action and `on_first_sign_in:`'s run with no tenant, never every tenant; each reaches only records every tenant shares, and what only `Access.public` lets it in to is `policy.system_grant_missing` at where it is named, as for background work. A webhook's reply is an output to `Access.public`: each field it carries, or that decided it, is one anyone at all may read, a record the action created holding what it set (`policy.output_field_unproved`), proved in the checker where routes' outputs are |
