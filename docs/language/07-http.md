---
title: HTTP
area: 7
status: decided
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - route.*
  - syntax.expected_http_method
  - syntax.expected_invocation
  - syntax.expected_route_item
  - type.route_binding_collision
  - type.route_header_name_invalid
  - type.route_header_scalar_required
  - type.route_header_wire_duplicate
  - type.route_query_field_unsupported
  - type.route_query_object_required
---

# HTTP

How a Jadpo application answers HTTP requests: route groups, paths, the one
input a route takes, its output and status, time limits, health endpoints,
separate APIs and versions, webhooks, how failures reach the client,
OpenAPI, and how a breaking change to the API is caught.

> Decided on 2026-10-12. Code on this page applies the decisions from areas 1 to 7. The new compiler does not build HTTP yet: roadmap chunks 4.1 (routes) and 4.2 (request guarantees) build it.

The rule behind all of it: **a route is a thin door.** It says which method
and URL, what comes in, what goes out and what runs. Logic lives in actions
and queries; statuses come from failure kinds (area 4).

## Route groups

Every route belongs to a named group in its domain's `routes/` folder, one
group per file, named after the group (decided in area 1).

```jadpo fragment
// domains/planning/routes/todos.jadpo
routes Todos {
    prefix: /todos

    GET / { ... }
    POST / { ... }
    GET /{todo_id: Todo.id} { ... }
    PATCH /{todo_id: Todo.id} { ... }
}
```

- A route's path follows the prefix: `GET /{todo_id: Todo.id}` answers
  `GET /todos/{todo_id}`.
- The prefix's fixed segments joined with `_` give the file name: `/todos`
  is `todos.jadpo`, `/admin/orders` is `admin_orders.jadpo`.
- A prefix may hold typed placeholders, available to every route in the
  group: `prefix: /projects/{project_id: Project.id}/todos` lives in
  `projects_todos.jadpo`. Prefixes may share a start, but every URL matches
  exactly one group, which the compiler checks, so a URL from a log leads to
  one file.
- Routes are never `public` (area 1); who may call one is authentication's
  and policy's job (areas 8 and 9).

## Paths and placeholders

A placeholder is typed where it appears and reaches the action by its name
(question 2):

```jadpo fragment
PATCH /{todo_id: Todo.id} {
    input: PatchTodo
    output: TodoView
    run: patch_todo(todo_id, input, current_principal.user)
}
```

- Methods are `GET`, `POST`, `PUT`, `PATCH` and `DELETE`; the runtime
  answers `HEAD` and `OPTIONS` itself.
- A placeholder's type is a named scalar or an `X.id`, never optional. The
  segment is decoded once and checked; a bad value is a 422 like any input.
- Two routes that could match one request (`/{todo_id: Todo.id}` and
  `/{slug: Slug}`) are an error. A fixed segment beats a placeholder, so
  `GET /archived` can sit beside `GET /{todo_id: Todo.id}`.

## One input

A route takes at most one `input:`, an `Input` type (area 2). Its source
follows the method (question 2): the query string for `GET` and `DELETE`,
the JSON body for `POST`, `PUT` and `PATCH`.

```jadpo fragment
type ListTodos = Input {
    status: TodoStatus {
        optional: true
    }
    limit: PageSize {
        default: 25
    }
    request_tag: RequestTag {
        header: "X-Request-Tag"
        optional: true
    }
}

GET / {
    input: ListTodos
    output: TodoPage
    run: todos_for_owner(input, current_principal.user)
}
```

- Query-string fields are scalars, enums or lists of them, a list being the
  key repeated (`?status=open&status=done`). Nested objects need a body.
- A field read from a header says so with `header:`. Headers are scalars,
  named once, and never credentials (`Authorization`, cookies, `X-Jadpo-*`),
  which authentication owns (area 8).
- Unknown keys and fields are refused. A `GET` or `DELETE` never reads a
  body, and a body is limited to 1 MB (question 5).

## Output and status

`output:` names an `Output` type and must be exactly the action's result,
so the route shows its contract without opening the action. The success
status is `status:` (question 3):

| `status:` | HTTP | Needs |
|---|---|---|
| left out (`SuccessStatus.ok`) | 200 | `output:` |
| `SuccessStatus.created` | 201 | `output:` |
| `SuccessStatus.accepted` | 202 | `output:`; the work continues in the background (area 12) |
| `SuccessStatus.no_content` | 204 | no `output:`; the action returns `Unit` |

```jadpo fragment
POST / {
    input: CreateTodo
    output: TodoView
    status: SuccessStatus.created
    run: create_todo(input, current_principal.user)
}
```

Only the output's declared fields are sent. Failure statuses are never
written on a route: they come from the kinds of the failures the action can
produce (area 4).

## What a route runs

`run:` calls one named action, passing placeholders, `input` and the caller
(area 8) as ordinary arguments. A `GET` calls a named query instead, so it
can never write (question 4):

```jadpo fragment
GET /{todo_id: Todo.id} {
    output: TodoView
    run: todo_view(todo_id)              // queries/todo_view.jadpo
}

POST /{todo_id: Todo.id}/complete {
    output: TodoView
    run: complete_todo(todo_id)          // actions/complete_todo.jadpo
}
```

Own-domain actions and queries are called bare, another domain's public ones
through its name (area 1). There is no inline action in a route.

## Time limits

Every route has a deadline: 10 seconds unless `deadline:` says otherwise,
and never more than 60 (question 5). When it passes, the runtime starts no
new work, commits nothing and answers 504 `deadline_exceeded`. The deadline
covers integration calls, which have their own shorter timeouts (area 11);
longer work answers `status: SuccessStatus.accepted` and continues in a
subscriber, or the route waits for it with `settle:`.

```jadpo fragment
POST /import {
    input: ImportTodos
    output: ImportResult
    deadline: 30s
    run: import_todos(input)
}
```


## Waiting for follow-on work

An action does its own database work and leaves the rest, such as a second
provider call, to subscribers (area 12). When the client wants the finished
result rather than `202`, the route waits for that work with `settle:`
(Richard, 2026-10-15):

```jadpo fragment
POST /orders {
    input: PlaceOrder
    output: OrderView
    status: SuccessStatus.created
    run: place_order(input, current_principal.user)
    settle: {
        answer: order_view(result.id)
        within: 5s
    }
}
```

- After the action commits, the runtime delivers the events it emitted at
  once, and the events their subscribers emit in turn, while the request
  waits.
- The work has settled when every one of those deliveries has succeeded,
  failed for good or been parked as `outcome_unknown`. A delivery waiting
  to retry (`unavailable`, `rate_limited`) has not.
- Then the route runs `answer:`, a named query given `result` (the
  action's result), and answers with its result and the route's `status:`.
  If `within:` passes first, it runs the same query and answers `202`, so
  the client gets the same `output:` either way and knows from the status
  whether the work finished. The work carries on in the background either
  way, exactly as it would without `settle:`.
- With `settle:`, `output:` is the query's result type, not the action's.
  `within:` must fit inside the route's `deadline:`, and waiting never ends
  in a 504, because the action has already committed.
- Only work this action set off is waited for, never other requests' work
  or published copies of events (area 12).
- A repeat with the same `Idempotency-Key` runs the answer query again
  rather than replaying a stored body, so it shows how far the work has
  got.

## Health

The runtime always serves `GET /health/live` (the process runs) and
`GET /health/ready` (every store declared in `app.jadpo` answers within two
seconds; integrations are reported as unchecked). There is nothing to write,
and neither path is part of an API's OpenAPI or record (question 6).

## APIs and versions

Most applications have one API. One with several audiences (public, admin,
partners, webhooks) declares them in `app.jadpo`, like stores (question 7):

```jadpo fragment
// app.jadpo
application {
    apis: {
        main: {
            prefix: /v1
            authentication: [AuthenticationMethod.browser_session, AuthenticationMethod.api_token]
        }
        admin: {
            prefix: /admin
        }
        webhooks: {
            prefix: /webhooks
        }
    }
}

// domains/ordering/routes/admin_orders.jadpo
routes AdminOrders {
    prefix: /admin/orders
    api: admin
    ...
}
```

- With nothing declared there is one API, `main`, at `/`. A group names its
  API with `api:` (default `main`), and its prefix starts with the API's.
- Each API has its own OpenAPI document, record and default authentication
  (area 8).
- A new major version is another API (`v2: { prefix: /v2 }`), declared only
  when a break must live beside the old version. Deleting `v1` makes the
  compiler report every group and type left unused.

## Webhooks

An inbound webhook is an ordinary route in the `webhooks` API (question 8):

```jadpo fragment
// domains/payments/routes/webhooks_stripe.jadpo
routes WebhooksStripe {
    prefix: /webhooks/stripe
    api: webhooks

    POST / {
        authentication: stripe.signature    // checked over the raw body (areas 8 and 11)
        input: StripeEvent
        status: SuccessStatus.no_content
        run: record_stripe_event(input)
    }
}
```

The action stores the event under a unique rule on the provider's event id,
so a redelivery changes nothing, and the real work runs in a subscriber
(area 12).

## How failures reach the client

Decided in area 4: one envelope with `code`, `message`, `request_id` and the
failure's client fields under `details`; the status comes from the kind;
`x-request-id` repeats the id. Bad input lists each problem without its
value, naming the field by its path in the `Input`, with list positions as
numbers (question 9):

```json
{
  "error": {
    "code": "invalid_input",
    "message": "Request input is invalid.",
    "request_id": "req_0b1e…",
    "details": {
      "problems": [
        { "field": "lines.2.quantity", "rule": "max" },
        { "field": "colour", "rule": "unknown_field" }
      ]
    }
  }
}
```

| Condition | Status and code |
|---|---|
| The body is not JSON, or the query string cannot be read | 400 `invalid_request` |
| A field is missing, unknown or breaks a rule (body, query, header or placeholder) | 422 `invalid_input` |
| No route matches | 404 `route_not_found` |
| The path exists but not for this method | 405 `method_not_allowed`, with `Allow` |
| The body is over the limit | 413 `request_too_large` |
| The deadline passed | 504 `deadline_exceeded` |

## OpenAPI

The compiler writes one OpenAPI 3.1 document per API into `build/openapi/`:
every route with its input, output, success status and reachable failures,
the 400 and 422 input responses, and the `///` descriptions (area 1). The
title is the application's name and the version the API record's version.

## The public API record

A breaking change is caught at review, not by a client in production
(question 10, researched in [the proposals](research/07-http-proposals.md)).
Each API has a committed, readable record, `api/main.api`, of its routes,
fields and failure codes. `jadpo check` fails while it disagrees with the
source and `jadpo api update` rewrites it, so every API change is a diff of
one file.

A change that can break a client (a route removed, a required input field
added, an output field removed, a failure code changed) also needs an
acceptance written by a person. The same form accepts a database drop
(area 5), a policy weakening (area 9) and a new domain (area 1):

```jadpo fragment
// acceptances/20261020T1400_remove_todo_priority.jadpo
/// Priority was never shown by any client.
acceptance {
    change: ApiChange.output_field_removed
    subject: main.TodoView.priority
}
```

Each acceptance is used once: the record notes which one allowed each
change. `acceptances/` is owned in CODEOWNERS, so the host's review shows who
approved it.

## Errors

> These examples predate the decisions above and are rewritten when the area is built (roadmap Phase 4).

Every error this area can produce, with an example that triggers it.

### `syntax.expected_http_method`

```jadpo project=e-http-method expect=syntax.expected_http_method
output Trace {
    ok: Bool
}

route FETCH /trace {
    auth: none
    output: Trace
    action: {
        return Trace { ok: true }
    }
}
```

### `syntax.expected_route_item`

A status written as a number:

```jadpo project=e-route-item expect=syntax.expected_route_item
output Trace {
    ok: Bool
}

action trace() -> Trace {
    return Trace { ok: true }
}

route GET /trace {
    auth: none
    status: 200
    output: Trace
    run: trace()
}
```

### `syntax.expected_invocation`

```jadpo project=e-invocation expect=syntax.expected_invocation
output Trace {
    ok: Bool
}

route GET /trace {
    auth: none
    output: Trace
    run: true
}
```

### `route.item_colon_required`

```jadpo project=e-item-colon expect=route.item_colon_required
input RegisterCustomer {
    email: Text
}

output RegistrationAccepted {
    accepted: Bool
}

action register_customer(input: RegisterCustomer) -> RegistrationAccepted {
    return RegistrationAccepted { accepted: true }
}

route POST /registrations {
    auth: none
    input RegisterCustomer
    output: RegistrationAccepted
    run: register_customer(input)
}
```

### `route.item_duplicate`

```jadpo project=e-item-duplicate expect=route.item_duplicate
output Health {
    ok: Bool
}

action health() -> Health {
    return Health { ok: true }
}

route GET /health {
    auth: none
    output: Health
    run: health()
    run: health()
}
```

### `route.auth_value_invalid`

```jadpo project=e-auth-value expect=route.auth_value_invalid
input RegisterCustomer {
    email: Text
}

output RegistrationAccepted {
    accepted: Bool
}

action register_customer(input: RegisterCustomer) -> RegistrationAccepted {
    return RegistrationAccepted { accepted: true }
}

route POST /registrations {
    auth: nonke
    input: RegisterCustomer
    output: RegistrationAccepted
    run: register_customer(input)
}
```

### `route.behaviour_required`

```jadpo project=e-behaviour-required expect=route.behaviour_required
route GET /health {
    auth: none
}
```

### `route.behaviour_conflict`

```jadpo project=e-behaviour-conflict expect=route.behaviour_conflict
value Result {
    ok: Bool
}

action complete() -> Result {
    return Result { ok: true }
}

route GET /result {
    auth: none
    output: Result
    run: complete()
    action: {
        return Result { ok: true }
    }
}
```

### `route.deadline_invalid`

```jadpo project=e-deadline expect=route.deadline_invalid
route GET /deadline {
    auth: none
    deadline: 0s
    action: {}
}
```

### `route.path_placeholder_invalid`

```jadpo project=e-placeholder-invalid expect=route.path_placeholder_invalid
output Health {
    ok: Bool
}

route GET /health/{bad-name} {
    auth: none
    output: Health
    action: {
        return Health { ok: true }
    }
}
```

### `route.path_placeholder_duplicate`

```jadpo project=e-placeholder-duplicate expect=route.path_placeholder_duplicate
type CustomerId = Uuid {}

output CustomerView {
    id: CustomerId
}

route GET /customers/{id}/related/{id} {
    auth: none
    path: {
        id: CustomerId
    }
    output: CustomerView
    action: {
        return CustomerView { id: path.id }
    }
}
```

### `route.path_binding_missing`

```jadpo project=e-binding-missing expect=route.path_binding_missing
type CustomerId = Uuid {}

output CustomerView {
    id: CustomerId
}

route GET /customers/{customer_id}/orders/{order_id} {
    auth: none
    path: {
        customer_id: CustomerId
    }
    output: CustomerView
    action: {
        return CustomerView { id: path.customer_id }
    }
}
```

### `route.path_binding_extra`

```jadpo project=e-binding-extra expect=route.path_binding_extra
type CustomerId = Uuid {}

output CustomerView {
    id: CustomerId
}

route GET /customers/{customer_id} {
    auth: none
    path: {
        customer_id: CustomerId
        order_id: CustomerId
    }
    output: CustomerView
    action: {
        return CustomerView { id: path.customer_id }
    }
}
```

### `route.path_binding_duplicate`

```jadpo project=e-binding-duplicate expect=route.path_binding_duplicate
type CustomerId = Uuid {}

output CustomerView {
    id: CustomerId
}

route GET /customers/{id} {
    auth: none
    path: {
        id: CustomerId
        id: CustomerId
    }
    output: CustomerView
    action: {
        return CustomerView { id: path.id }
    }
}
```

### `route.path_field_modifier_invalid`

```jadpo project=e-path-modifier expect=route.path_field_modifier_invalid
output Health {
    ok: Bool
}

route GET /health/{probe_id} {
    auth: none
    path: {
        probe_id: Uuid index
    }
    output: Health
    action: {
        return Health { ok: true }
    }
}
```

### `type.route_query_object_required`

```jadpo project=e-query-object expect=type.route_query_object_required
output Trace {
    ok: Bool
}

route GET /trace {
    auth: none
    query: Text
    output: Trace
    action: {
        return Trace { ok: true }
    }
}
```

### `type.route_query_field_unsupported`

```jadpo project=e-query-field expect=type.route_query_field_unsupported
value Filter {
    tags: List<Text>
}

output Trace {
    ok: Bool
}

route GET /trace {
    auth: none
    query: Filter
    output: Trace
    action: {
        return Trace { ok: true }
    }
}
```

### `type.route_binding_collision`

The placeholder `id` and the query field `id` would both be called `id`:

```jadpo project=e-binding-collision expect=type.route_binding_collision
type TraceId = Uuid {}

value Filter {
    id: TraceId
}

output Trace {
    ok: Bool
}

route GET /trace/{id} {
    auth: none
    path: {
        id: TraceId
    }
    query: Filter
    output: Trace
    action: {
        return Trace { ok: true }
    }
}
```

### `route.header_from_required`

```jadpo project=e-header-from expect=route.header_from_required
output Trace {
    ok: Bool
}

route GET /trace {
    auth: none
    headers: {
        trace: Text wire "X-Trace"
    }
    output: Trace
    action: {
        return Trace { ok: true }
    }
}
```

### `type.route_header_name_invalid`

```jadpo project=e-header-name expect=type.route_header_name_invalid
output Trace {
    ok: Bool
}

route GET /trace {
    auth: none
    headers: {
        trace: Text from "Bad Header" optional
    }
    output: Trace
    action: {
        return Trace { ok: true }
    }
}
```

### `type.route_header_wire_duplicate`

Header names are compared without case:

```jadpo project=e-header-duplicate expect=type.route_header_wire_duplicate
output Trace {
    ok: Bool
}

route GET /trace {
    auth: none
    headers: {
        first: Text from "X-Trace" optional
        second: Text from "x-trace" optional
    }
    output: Trace
    action: {
        return Trace { ok: true }
    }
}
```

### `type.route_header_scalar_required`

```jadpo project=e-header-scalar expect=type.route_header_scalar_required
value Nested {
    id: Uuid
}

output Trace {
    ok: Bool
}

route GET /trace {
    auth: none
    headers: {
        nested: Nested from "X-Nested" optional
    }
    output: Trace
    action: {
        return Trace { ok: true }
    }
}
```

## Review

### Status

The new compiler builds none of this area yet. Roadmap chunk 4.1 builds
route groups, inputs, outputs, statuses, OpenAPI and the API record; chunk
4.2 builds deadlines, body limits and health.

### Consistency with the rest of the language

- **`query` means three things**: the route item, the `query` declaration
  and the `query required` expression. One `input:` removes the route item.
- **`success:` is also a `match` arm** (`success(value)`); `status:` removes
  the clash.
- **Four names for request data**: `path.x`, `query.x`, `headers.x` and
  `input`. Proposed: placeholders by their own name, and `input`.
- **`headers:` is an anonymous shape** with a sentence-like `from "..."` and a
  trailing `optional`, against area 2's named types and settings blocks.
- **Two behaviour forms** (`run:` and inline `action:`), and the inline form
  puts `fails` after a colon.
- **`auth: none`** abbreviates a word spelled in full elsewhere
  (`authentication browser_session`); area 8 settles it.
- **`routes` is plural** while other keywords are singular and their folders
  plural (`schedule` in `schedules/`). It names a group, so it is kept, and
  the folder `routes/` still matches it.
- **The default API is `main`**, like the default store, and not `public`,
  which already means "visible to other domains".
- **Health is the only route with special rules** today; making it generated
  removes them.

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Route authentication: `authentication: RouteAuthentication.none` and `.fresh`, each API's default, how the caller is passed (`current_principal`) | 8 | Input to area 8 |
| Webhook signature checking over the raw body, declared with the provider's integration | 8, 11 | Input to areas 8 and 11 |
| Policy on routes and the `not_permitted` answer | 9 | Input to area 9 |
| A new root folder for records and acceptances (`api/`, `acceptances/`) in area 1's fixed root list | 1 | Depends on question 10 |
| Database drops and store moves accepted with the shared `acceptance` form | 5 | Depends on question 10 |
| Policy weakening and new domains accepted with the shared form, enforced through the approval protocol | 9, 1 | Depends on question 10 |
| Integration timeouts inside a route's deadline | 11 | Input to area 11 |
| Background work after `status: SuccessStatus.accepted`; webhook events processed by subscribers | 12 | Input to area 12 |
| Calling routes in tests | 13 | Input to area 13 |
| `toolchain.target_liveness_not_local`, raised only by `jadpo build`; the `jadpo api update` command and CI's record check | 14 | Input to area 14 |
| `precondition_failed` for `If-Match` (from area 4); until then a concurrent change answers 409 through area 5's check | 7 | Deferred |
| Response headers: `Location` for created records, caching, `ETag` | 7 | Deferred |
| Streaming responses that fail after headers are sent (ROUTE-002, from area 4) | 7 | Deferred |
| Per-route rate limits (`rate_limited`) | 7 | Deferred |

### Open questions

1. **How are route groups written and checked?** Area 1 decided named groups
   with a checked prefix, but not what is checked. *Recommendation:*
   `routes Todos { prefix: /todos ... }`; the prefix's fixed segments joined
   with `_` must give the file name (`/admin/orders` in
   `admin_orders.jadpo`); prefixes never repeat or nest, so a URL leads to
   one file. This is area 1's research rule K5 without its separate
   "surface" naming. Rails and Laravel group routes the same way
   (`scope "/admin"`, `Route::prefix('admin')`) but check nothing.
   *Richard asked (2026-10-12)* how a prefix with a placeholder works
   (`/projects/{project_id}/todos`). *Revised recommendation:* a prefix may
   hold typed placeholders, `prefix: /projects/{project_id: Project.id}/todos`,
   available to every route in the group; the file name joins the fixed
   segments (`projects_todos.jadpo`). Prefixes may then share a start
   (`/projects` and `/projects/{project_id}/todos`), so the rule becomes:
   every URL matches exactly one group, checked by the compiler. Without
   this, a multi-tenant app's routes (`/orgs/{org_id}/...`) would all pile
   into one group in the wrong domain. *Decided 2026-10-12:* the revised recommendation.
2. **One input per route, with placeholders typed in the path?** Today
   `path:`, `query:`, `headers:` and `input:`. *Recommendation:* yes:
   `GET /{todo_id: Todo.id}`, the value read as `todo_id`; one `input:` whose
   source follows the method (query string for `GET` and `DELETE`, body
   otherwise); headers as `header:` settings on `Input` fields. This removes
   two anonymous shapes and one meaning of `query` (syntax review top 10,
   item 6). FastAPI and ASP.NET infer each parameter's source the same way. *Decided 2026-10-12:* yes, as recommended.
3. **`status:` and a checked `output:`?** *Recommendation:* rename
   `success:` to `status:` taking a `SuccessStatus`: `SuccessStatus.ok`,
   `SuccessStatus.created`, `SuccessStatus.accepted` or
   `SuccessStatus.no_content`; `output:` must be an `Output` and exactly the
   action's result type, and is required unless the status is
   `SuccessStatus.no_content`. This fixes today's 500 on
   a mismatch and the silent `null` body. *Decided 2026-10-12:* yes, as recommended.
4. **What may `run:` call?** Area 3 left a route calling a query to area 7.
   *Recommendation:* a `GET` calls a named query and every other method a
   named action, so a `GET` never writes (HTTP's "safe" methods), and no
   wrapper action is needed for a read. Inline route actions and calls to
   plain functions go. *Decided 2026-10-12:* yes, as recommended.
5. **Default deadline and body limit?** Today neither. *Recommendation:* a
   10-second deadline unless `deadline:` says otherwise, at most 60 seconds;
   a 1 MB body limit answering 413, with no setting until an application
   needs one. Fastify defaults to a 1 MB body and Express to 100 KB; a hung
   request without a deadline holds a database connection. *Decided 2026-10-12:* yes, as recommended.
6. **Should health be generated only?** *Recommendation:* yes: fixed
   `/health/live` and `/health/ready`, readiness from the stores declared in
   `app.jadpo` with a two-second check, no authored liveness and no
   `health { }` syntax. Everything it needs is already declared. *Decided 2026-10-12:* yes, as recommended.
7. **How are audiences and versions declared?** *Recommendation:* an
   `apis: { }` block in `application`, `main` at `/` implied; a group names
   its API with `api:`; one OpenAPI document, record and default
   authentication per API. A new major version is another API, declared only
   when a break must coexist with the old version. Stripe's dated versions
   with response rewriting suit a large public API and are not worth their
   cost here. *Decided 2026-10-12:* yes, as recommended.
8. **How are inbound webhooks written?** *Recommendation:* as ordinary routes
   in a `webhooks` API, with the provider's signature check as their
   authentication (areas 8 and 11), a unique rule on the provider's event id
   so redeliveries do nothing, and the work in a subscriber. No webhook
   keyword. *Decided 2026-10-12:* yes, as recommended.
9. **How is a field named in `{ field, rule }`?** Area 4 left the format to
   area 7. *Recommendation:* the `Input` field's path, with list positions as
   numbers (`lines.2.quantity`), the names the OpenAPI schema uses; a header
   field by its field name. JSON Pointer (`/lines/2/quantity`, used in
   RFC 9457 examples) is more standard but reads worse beside a form field;
   Stripe uses `lines[2][quantity]`. *Decided 2026-10-12:* yes, as recommended.
10. **How are breaking API changes caught and accepted?** Area 4 carried the
    record here, with Richard's idea of one acceptance mechanism.
    *Recommendation:* option C in [the proposals](research/07-http-proposals.md):
    a committed readable record per API (`api/main.api`) that `jadpo check`
    compares, as Kotlin's API validator and Microsoft's API Extractor do;
    changes that can break a client need an `acceptance` file written by a
    person, one per change and used once, in the same form for database
    drops, policy weakening and new domains; the `acceptances/` folder is
    owned in CODEOWNERS so the host's review records who approved it. *Decided 2026-10-12:* yes, as recommended.
11. **How does a write's revision reach the client?** (carried from area 6)
    Area 6 lets a client that just wrote skip caches by its revision. What
    are the response header and cookie (`jadpo-revision`) called, and must
    non-browser clients echo the revision back?
    *Decided 2026-10-12:* every write returns a `Jadpo-Revision` header and
    browsers get the same value as a cookie; a client that sends it back skips
    caches; one that does not may see a cached copy up to its `keep:`.
12. **Where do generated authentication routes and security schemes
    appear?** (carried from area 8) Does the generated `POST /auth/exchange`
    appear in the route inventory and OpenAPI, and how is each route's
    security scheme shown?
    *Decided 2026-10-12:* yes: generated routes appear in the route list and
    OpenAPI like any route, marked generated; every route shows its security
    scheme in OpenAPI.
13. **How does an API declare its accepted authentication methods?**
    (carried from area 8) Area 8 has each API list its methods,
    `authentication: [AuthenticationMethod.browser_session, AuthenticationMethod.api_token]`,
    with a route only narrowing them; the `apis: { }` example above shows
    none, and the text mentions only a "default authentication". Where does
    the list go?
    *Decided 2026-10-12:* in each API's entry in `apis: { }`, as
    `authentication: [...]`; a route group or route may only narrow it.
14. **What do route tests send and get back?** (carried from area 13) Area
    13's tests use a `request { }` form returning a `Response` with its
    `status`, `code` and typed output. Does this page define that form and
    type?
    *Decided 2026-10-12:* area 13 owns the `request { }` form and the
    `Response` type (`status`, `code`, typed output, headers); this page links
    to it.
15. **Does a route show the URL it answers?** *Decided 2026-10-12
    (Richard):* yes: hovering over a route shows its full method and path,
    and `jadpo routes` lists every route with its full path, file, API,
    authentication and the query or action it runs.
16. **Is a route written `GET /path { }`?** Every other declaration starts
    with a keyword and a name; a route entry starts with its method and path
    and has no colon. *Recommendation:* keep it. The group is the declaration
    (`routes Todos { }`, keyword first); routes are its entries, keyed by
    method and path, which is how HTTP, OpenAPI, logs and every framework
    name a route. A named form (`route get_todo { method: Method.get path:
    ... }`) adds a third name to keep in step with the action and the URL,
    and hides the URL among settings. The colon rule names this as its one
    exception. *Decided 2026-10-12:* yes, as recommended.
17. **Are writes idempotent out of the box?** Nothing on this page covers a
    client retrying a write after a timeout. *Recommendation:* yes, for every
    method except `GET` (`POST`, `PUT`, `PATCH` and `DELETE`): HTTP calls
    `PUT` and `DELETE` idempotent, but a retried `DELETE` answers 404 where
    the first answered 204, and the action behind any write may raise events
    or call providers. A client may send `Idempotency-Key` (the IETF draft
    Stripe's design became). The runtime handles it entirely; the action
    never sees the key. Before the action runs it looks the key up, scoped to
    the caller; after, it stores the key, a hash of the request and the
    response in the main store, in the same transaction as the action's
    writes and events, for 24 hours. A repeat returns the stored response;
    the same key with a different request fails `idempotency_key_reused`
    (422); a repeat while the first is running fails `conflict` (409).
    Successes and failures the action returns are stored; `unavailable` and
    internal failures are not, so a retry runs again. Integration calls in
    the action send keys derived from it (area 11). An API can require the
    key with `idempotency: Idempotency.required`; OpenAPI lists the header. *Decided 2026-10-12:* yes, as recommended.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Route groups | Carried from area 1: named route groups with a prefix the compiler checks, one group per file in the domain's `routes/` folder, named after the group | 2026-10-12 |
| Triggers | Carried from area 1: routes call actions; routes are never `public` | 2026-10-12 |
| Documentation comments | Carried from area 1: `///` comments feed OpenAPI | 2026-10-12 |
| Route types | Carried from area 2: routes accept `Input` and return `Output` | 2026-10-12 |
| Colon rule | Carried from area 3: every route item is `name: value`; a route entry, keyed by method and path, is the one exception | 2026-10-12 |
| Failure responses | Carried from area 4: one envelope with `code`, `message`, `request_id` and client `details`; status from the kind; no status mapping on routes; `x-request-id` header | 2026-10-12 |
| Invalid input | Carried from area 4: each problem as `{ field, rule }`, never the value; area 7 sets the field format | 2026-10-12 |
| Shared acceptance | Carried from areas 1 and 5: new domains, policy changes and database drops need an explicit acceptance; area 7 designs the shared mechanism | 2026-10-12 |
| Route groups and prefix check | `routes Todos { prefix: ... }`; file name from the prefix's fixed segments; prefixes may hold typed placeholders; every URL matches exactly one group | 2026-10-12 |
| One input and typed placeholders | Placeholders typed in the path; one `input:` whose source follows the method; headers as `header:` on `Input` fields | 2026-10-12 |
| `status:` and checked `output:` | `status: SuccessStatus.x`; `output:` must be an `Output` and exactly the action's result type, required unless `no_content` | 2026-10-12 |
| What `run:` calls | A `GET` calls a named query, every other method a named action; no inline route actions | 2026-10-12 |
| Deadline and body limit | 10-second default deadline, `deadline:` up to 60 seconds; 1 MB body limit answering 413 | 2026-10-12 |
| Health | Generated only: `/health/live` and `/health/ready`, readiness from the declared stores | 2026-10-12 |
| APIs and versions | `apis: { }` in `application`, `main` at `/` implied; a group names its API with `api:`; one OpenAPI document and record per API; a new major version is another API | 2026-10-12 |
| Webhooks | Ordinary routes in a `webhooks` API; the provider's signature check as authentication; a unique rule on the provider's event id; work in a subscriber (Amended 2026-10-13 by area 11: provider webhooks are generated from the package and arrive as events; no route is written) | 2026-10-12 |
| Field path format | The `Input` field's dotted path with list positions as numbers (`lines.2.quantity`) | 2026-10-12 |
| API record and acceptance | Committed readable record per API (`api/main.api`) compared by `jadpo check`; breaking changes need a person-written acceptance file, one per change, the same form for drops, policy weakening and new domains; `acceptances/` owned in CODEOWNERS | 2026-10-12 |
| Write revision header | `Jadpo-Revision` header on every write, a cookie for browsers; sending it back skips caches | 2026-10-12 |
| Generated authentication routes in OpenAPI | Listed like any route, marked generated; every route shows its security scheme | 2026-10-12 |
| Accepted authentication methods per API | `authentication: [...]` in each API's entry in `apis: { }`; groups and routes may only narrow it | 2026-10-12 |
| Route URL shown | Hover shows a route's full method and path; `jadpo routes` lists every route with path, file, API, authentication and what it runs (Richard) | 2026-10-12 |
| Route test request and `Response` | Owned by area 13; this page links to it | 2026-10-12 |
| Route declaration form | `GET /path { }` entries inside a `routes` group, keyed by method and path; the one exception to the colon rule | 2026-10-12 |
| Idempotent writes | Every non-`GET` route honours `Idempotency-Key`, handled entirely by the runtime: stored with the response in the action's transaction for 24 hours, scoped to the caller; reuse with a different request is 422, in flight is 409; `unavailable` and internal failures not stored; `idempotency: Idempotency.required` per API | 2026-10-12 |
| Expired idempotency keys | A lookup ignores a key older than 24 hours; the runtime deletes expired keys in batches from its own minute tick (area 12), so the table holds about a day of keys (Richard, from an outside review) | 2026-10-15 |
| Waiting for follow-on work | `settle: { answer: <query>(result...) within: <duration> }`: after commit the runtime delivers the action's events and their follow-ons at once and waits until each has succeeded, failed for good or been parked, then answers with the query and the route's status; past `within:` the same query answers 202; `output:` is the query's result; a repeat with the same `Idempotency-Key` reruns the query (Richard, from an outside review) | 2026-10-15 |
