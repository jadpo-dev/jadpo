---
title: HTTP
area: 7
status: in-review
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

> In review. Code outside the collapsed boxes applies the decisions from areas 1 to 5 and this page's recommendations; each names its open question. Today's compiled syntax is collapsed beneath each section that changes.

The rule behind all of it: **a route is a thin door.** It says which method
and URL, what comes in, what goes out and what runs. Logic lives in actions
and queries; statuses come from failure kinds (area 4).

```jadpo hidden project=today file=shared.jadpo
type TodoId = Uuid {}

type TodoTitle = Text {
    min_length: 1
    max_length: 200
}

type PageSize = Int {
    min: 1
    max: 100
}

type RequestId = Text {
    max_length: 64
}

type Status = Enum {
    live
}

input RenameTodo {
    title: TodoTitle
}

input CreateTodo {
    title: TodoTitle
}

input ListTodos {
    limit: PageSize default 25
}

output TodoView {
    title: TodoTitle
}

output TodoCount {
    limit: PageSize
}

output Health {
    status: Status
}

action rename_todo(todo_id: TodoId, input: RenameTodo) -> TodoView {
    return TodoView { title: input.title }
}

action count_todos(input: ListTodos) -> TodoCount {
    return TodoCount { limit: input.limit }
}

action create_todo(input: CreateTodo) -> TodoView {
    return TodoView { title: input.title }
}

action delete_todo(todo_id: TodoId) -> Unit {}
```

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
  is `todos.jadpo`, `/admin/orders` is `admin_orders.jadpo` (question 1).
  Prefixes never repeat or nest, so a URL from a log leads to one file; a
  nested resource such as `/users/{user_id}/todos` sits in `Users`.
- Routes are never `public` (area 1); who may call one is authentication's
  and policy's job (areas 8 and 9).

::: today

No groups: each route is a top-level `route GET /todos/{todo_id} { ... }`
with its full path, and nothing ties a file to its URLs. The golden app keeps
a todo route in its health file (area 1).

:::

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

::: today

Placeholders are typed in a separate `path: { }` block and read as
`path.todo_id`. `/a/{id}` and `/a/{other}` both compile and the first wins;
`/todos/:todo_id`, the design source's spelling, compiles as a literal
segment.

```jadpo project=today file=paths.jadpo
route PATCH /todos/{todo_id} {
    auth: none
    path: {
        todo_id: TodoId
    }
    input: RenameTodo
    output: TodoView
    run: rename_todo(path.todo_id, input)
}
```

:::

## One input

A route takes at most one `input:`, an `Input` type (area 2). Its source
follows the method (question 2): the query string for `GET` and `DELETE`,
the JSON body for `POST`, `PUT` and `PATCH`.

```jadpo fragment
type ListTodos = Input {
    status: TodoStatus { optional: true }
    limit: PageSize { default: 25 }
    request_tag: RequestTag { header: "X-Request-Tag"  optional: true }
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

::: today

Three channels: `query:` (read as `query.x`), an anonymous `headers:` block
with `from "..."` and a trailing `optional` (read as `headers.x`), and
`input:` (the body). A `GET` with a body `input:` compiles. Lists are refused
in a query but a nested object is accepted as one JSON value. Nothing limits
the body size.

```jadpo project=today file=inputs.jadpo
route GET /todos {
    auth: none
    query: ListTodos
    headers: {
        request_id: RequestId from "X-Request-Id"
    }
    output: TodoCount
    run: count_todos(query)
}
```

:::

## Output and status

`output:` names an `Output` type and must be exactly the action's result,
so the route shows its contract without opening the action. The success
status is `status:` (question 3):

| `status:` | HTTP | Needs |
|---|---|---|
| left out (`ok`) | 200 | `output:` |
| `created` | 201 | `output:` |
| `accepted` | 202 | `output:`; the work continues in the background (area 12) |
| `no_content` | 204 | no `output:`; the action returns `Unit` |

```jadpo fragment
POST / {
    input: CreateTodo
    output: TodoView
    status: created
    run: create_todo(input, current_principal.user)
}
```

Only the output's declared fields are sent. Failure statuses are never
written on a route: they come from the kinds of the failures the action can
produce (area 4).

::: today

The item is `success:`, which is also the `match` arm `success(value)`.
**Bugs (compiled):** `output:` is not checked against the action's result,
and a mismatch answers 500 at runtime; a route without `output:` whose action
returns a value answers 200 with `null` (read from generated code); `output:`
accepts `input` and `value` types.

```jadpo project=today file=status.jadpo
route POST /todos {
    auth: none
    input: CreateTodo
    output: TodoView
    success: created
    run: create_todo(input)
}

route DELETE /todos/{todo_id} {
    auth: none
    path: {
        todo_id: TodoId
    }
    success: no_content
    run: delete_todo(path.todo_id)
}
```

:::

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

::: today

A route may hold an inline `action: { ... }` instead (see Health), and
`run:` also accepts a plain function, which area 3 rules out (compiled).

:::

## Time limits

Every route has a deadline: 10 seconds unless `deadline:` says otherwise,
and never more than 60 (question 5). When it passes, the runtime starts no
new work, commits nothing and answers 504 `deadline_exceeded`. The deadline
covers integration calls, which have their own shorter timeouts (area 11);
longer work answers `status: accepted` and continues in a subscriber.

```jadpo fragment
POST /import {
    input: ImportTodos
    output: ImportResult
    deadline: 30s
    run: import_todos(input)
}
```

::: today

`deadline:` exists (`0s` and `1d` are errors) but has no default, so a route
without one never times out, and `deadline: 10h` compiles.

:::

## Health

The runtime always serves `GET /health/live` (the process runs) and
`GET /health/ready` (every store declared in `app.jadpo` answers within two
seconds; integrations are reported as unchecked). There is nothing to write,
and neither path is part of an API's OpenAPI or record (question 6).

::: today

Both are generated. An application may also write `GET /health/live` as an
inline action returning literals; anything else fails at build time with
`toolchain.target_liveness_not_local`. The design source's
`readiness database { ... }` never parsed.

```jadpo project=today file=health.jadpo
route GET /health/live {
    auth: none
    output: Health
    action: {
        return Health { status: Status.live }
    }
}
```

:::

## APIs and versions

Most applications have one API. One with several audiences (public, admin,
partners, webhooks) declares them in `app.jadpo`, like stores (question 7):

```jadpo fragment
// app.jadpo
application {
    apis: {
        main: {
            prefix: /v1
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

Today there is one flat list of routes and one OpenAPI document.

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
        status: no_content
        run: record_stripe_event(input)
    }
}
```

The action stores the event under a unique rule on the provider's event id,
so a redelivery changes nothing, and the real work runs in a subscriber
(area 12). Today nothing supports webhooks: there is no signature check, and
the body is parsed before code could see its raw bytes.

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

::: today

Bad body input answers 422 `invalid_input` with no details, a bad
placeholder 422 `invalid_value`, and a wrong method 404.

:::

## OpenAPI

The compiler writes one OpenAPI 3.1 document per API into `build/openapi/`:
every route with its input, output, success status and reachable failures,
the 400 and 422 input responses, and the `///` descriptions (area 1). The
title is the application's name and the version the API record's version.

::: today

One `openapi.json` titled "Application API" at version `0.0.0`. It lists
`/health/live` but not `/health/ready`, and omits the 400 and 422 input
responses (read from generated output).

:::

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
    change: output_field_removed
    subject: main.TodoView.priority
}
```

Each acceptance is used once: the record notes which one allowed each
change. `acceptances/` is owned in CODEOWNERS, so the host's review shows who
approved it.

::: today

`build/compatibility/public-failure-codes.json` holds failure codes only, is
regenerated every build and says `baseline_not_configured`: nothing is
compared. Routes and fields are not recorded.

:::

## Errors

> The examples in this section use today's syntax, because they show the compiler's real output; they are rewritten as the rework lands.

Every error this area can produce, with an example that triggers it and the
compiler's actual output.

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

`status:` does not exist yet:

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

Each row says how it was checked: compiled with `jadpo check`, read from
generated code or output (`jadpo build` / `jadpo artifacts` on a copy of
`examples/golden-todo-migration`), or read from the docs.

| Feature | Status |
|---|---|
| Methods and paths | Works: five methods, placeholders checked against the `path:` block, duplicate `GET /a` is `semantic.duplicate_declaration` (compiled). **Gaps (compiled):** `/a/{id}` and `/a/{other}` both compile, and the generated handler tries routes in order, so the first wins (read from generated code); `/a/:id` compiles as a literal segment. |
| Input channels | Works: `query:`, `headers:` and `input:` are checked as grammar §13 says, including header names and duplicates (compiled). **Gaps (compiled):** a `GET` with a JSON-body `input:` compiles and reads the body (read from generated code); lists are refused in a query but nested objects are accepted. |
| Output | **Bug (compiled, then read from generated code):** `output:` is not checked against the action's result; a mismatch answers 500 at runtime. A route with no `output:` whose action returns a value answers 200 with `null`. `output:` accepts `input` and `value` types. |
| Behaviour | `run:` and inline `action:` both work; `run:` also accepts a plain function (compiled). |
| Deadlines | `deadline:` works and maps to 504 `deadline_exceeded` (read from generated code). **Gap:** no default and no maximum (`deadline: 10h` compiles). |
| Body size | **Gap (read from generated code):** no limit; `Bun.serve` is started with its own default. |
| Health | `GET /health/live` and `GET /health/ready` are generated; readiness checks the database and reports integrations as `unprobed` (configuration plan, CONFIG-P5). An authored liveness route is allowed with special rules. |
| OpenAPI | Generated as one document (read from generated output). **Gaps:** title "Application API", version `0.0.0`; `/health/live` listed and `/health/ready` not; 400 and 422 input responses missing (area 4). |
| Failures to clients | Envelope, statuses and request id work (area 4, run under Bun 1.2.20). A wrong method answers 404, not 405 (read from generated code). |
| API record | **Gap:** `public-failure-codes.json` holds failure codes only, in `build/`, with `baseline_not_configured`; nothing records routes or fields. |
| Webhooks, separate APIs, versions | None. |

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
| Route authentication: `authentication: none` and `fresh`, each API's default, how the caller is passed (`current_principal`) | 8 | Input to area 8 |
| Webhook signature checking over the raw body, declared with the provider's integration | 8, 11 | Input to areas 8 and 11 |
| Policy on routes and the `not_permitted` answer | 9 | Input to area 9 |
| A new root folder for records and acceptances (`api/`, `acceptances/`) in area 1's fixed root list | 1 | Depends on question 10 |
| Database drops and store moves accepted with the shared `acceptance` form | 5 | Depends on question 10 |
| Policy weakening and new domains accepted with the shared form, enforced through the approval protocol | 9, 1 | Depends on question 10 |
| Integration timeouts inside a route's deadline | 11 | Input to area 11 |
| Background work after `status: accepted`; webhook events processed by subscribers | 12 | Input to area 12 |
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
2. **One input per route, with placeholders typed in the path?** Today
   `path:`, `query:`, `headers:` and `input:`. *Recommendation:* yes:
   `GET /{todo_id: Todo.id}`, the value read as `todo_id`; one `input:` whose
   source follows the method (query string for `GET` and `DELETE`, body
   otherwise); headers as `header:` settings on `Input` fields. This removes
   two anonymous shapes and one meaning of `query` (syntax review top 10,
   item 6). FastAPI and ASP.NET infer each parameter's source the same way.
3. **`status:` and a checked `output:`?** *Recommendation:* rename
   `success:` to `status:` with `ok`, `created`, `accepted` and `no_content`;
   `output:` must be an `Output` and exactly the action's result type, and
   is required unless the status is `no_content`. This fixes today's 500 on
   a mismatch and the silent `null` body.
4. **What may `run:` call?** Area 3 left a route calling a query to area 7.
   *Recommendation:* a `GET` calls a named query and every other method a
   named action, so a `GET` never writes (HTTP's "safe" methods), and no
   wrapper action is needed for a read. Inline route actions and calls to
   plain functions go.
5. **Default deadline and body limit?** Today neither. *Recommendation:* a
   10-second deadline unless `deadline:` says otherwise, at most 60 seconds;
   a 1 MB body limit answering 413, with no setting until an application
   needs one. Fastify defaults to a 1 MB body and Express to 100 KB; a hung
   request without a deadline holds a database connection.
6. **Should health be generated only?** *Recommendation:* yes: fixed
   `/health/live` and `/health/ready`, readiness from the stores declared in
   `app.jadpo` with a two-second check, no authored liveness and no
   `health { }` syntax. Everything it needs is already declared.
7. **How are audiences and versions declared?** *Recommendation:* an
   `apis: { }` block in `application`, `main` at `/` implied; a group names
   its API with `api:`; one OpenAPI document, record and default
   authentication per API. A new major version is another API, declared only
   when a break must coexist with the old version. Stripe's dated versions
   with response rewriting suit a large public API and are not worth their
   cost here.
8. **How are inbound webhooks written?** *Recommendation:* as ordinary routes
   in a `webhooks` API, with the provider's signature check as their
   authentication (areas 8 and 11), a unique rule on the provider's event id
   so redeliveries do nothing, and the work in a subscriber. No webhook
   keyword.
9. **How is a field named in `{ field, rule }`?** Area 4 left the format to
   area 7. *Recommendation:* the `Input` field's path, with list positions as
   numbers (`lines.2.quantity`), the names the OpenAPI schema uses; a header
   field by its field name. JSON Pointer (`/lines/2/quantity`, used in
   RFC 9457 examples) is more standard but reads worse beside a form field;
   Stripe uses `lines[2][quantity]`.
10. **How are breaking API changes caught and accepted?** Area 4 carried the
    record here, with Richard's idea of one acceptance mechanism.
    *Recommendation:* option C in [the proposals](research/07-http-proposals.md):
    a committed readable record per API (`api/main.api`) that `jadpo check`
    compares, as Kotlin's API validator and Microsoft's API Extractor do;
    changes that can break a client need an `acceptance` file written by a
    person, one per change and used once, in the same form for database
    drops, policy weakening and new domains; the `acceptances/` folder is
    owned in CODEOWNERS so the host's review records who approved it.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Route groups | Carried from area 1: named route groups with a prefix the compiler checks, one group per file in the domain's `routes/` folder, named after the group | 2026-10-12 |
| Triggers | Carried from area 1: routes call actions; routes are never `public` | 2026-10-12 |
| Documentation comments | Carried from area 1: `///` comments feed OpenAPI | 2026-10-12 |
| Route types | Carried from area 2: routes accept `Input` and return `Output` | 2026-10-12 |
| Colon rule | Carried from area 3: every route item is `name: value` | 2026-10-12 |
| Failure responses | Carried from area 4: one envelope with `code`, `message`, `request_id` and client `details`; status from the kind; no status mapping on routes; `x-request-id` header | 2026-10-12 |
| Invalid input | Carried from area 4: each problem as `{ field, rule }`, never the value; area 7 sets the field format | 2026-10-12 |
| Shared acceptance | Carried from areas 1 and 5: new domains, policy changes and database drops need an explicit acceptance; area 7 designs the shared mechanism | 2026-10-12 |
| Route groups and prefix check | | |
| One input and typed placeholders | | |
| `status:` and checked `output:` | | |
| What `run:` calls | | |
| Deadline and body limit | | |
| Health | | |
| APIs and versions | | |
| Webhooks | | |
| Field path format | | |
| API record and acceptance | | |
