---
title: HTTP
area: 7
status: decided
checked-with: this checkout's jadpo on every gate run
---

# HTTP

How a Jadpo application answers HTTP requests: route groups, paths, the one
input a route takes, its output and status, time limits, health endpoints,
separate APIs and versions, webhooks, how failures reach the client,
OpenAPI, and how a breaking change to the API is caught.

> Built, but for what [What is built](#what-is-built) lists as not built yet.

The rule behind all of it: **a route is a thin door.** It says which method
and URL, what comes in, what goes out and what runs. Logic lives in actions
and queries; statuses come from failure kinds (area 4).

## Route groups

Every route belongs to a named group in its domain's `routes/` folder, one
group per file, named after the group (area 1).

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
- Hovering over a route in the editor shows its full method and path, and
  `jadpo routes` lists every route with its full path, file, API,
  authentication and the query or action it runs.
- A route is an entry `GET /path { }` inside its group, keyed by method
  and path, which is how HTTP, OpenAPI, logs and every framework name a
  route; it is the colon rule's one exception (area 3). A named form
  (`route get_todo { method: ... path: ... }`) would add a third name to
  keep in step with the action and the URL.

## Paths and placeholders

A placeholder is typed where it appears and reaches the action by its name:

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
follows the method: the query string for `GET` and `DELETE`, the JSON
body for `POST`, `PUT` and `PATCH`.

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
  body, and a body is limited to 1 MB, with no setting, answering 413.
- A body is read only when it is sent as JSON (`Content-Type:
  application/json`, or another `application/…+json` type), so a form a
  browser posts from another site never reaches an action. A query string
  or a body a route doesn't read is unknown input, like an unknown field.

- Text in a request is normalised to Unicode NFC as it is decoded (area
  19), in the body, the path, the query string and headers alike, before
  the input's rules are checked, so equal text compares equal.

## Output and status

`output:` names an `Output` type and must be exactly the action's result,
so the route shows its contract without opening the action. The success
status is `status:`:

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

A route may also send a map layer, a `FeatureCollection<G, P>` (area 20),
bare, so a mapping library fetches it as the whole response, plain RFC
7946 GeoJSON. A query gives one for a `GET` (area 6, "A query giving a
map layer"); an action builds one behind another method.

- **Only a collection, only bare.** A `List` or `Page` of layers, and a
  lone `Feature<G, P>`, are `route.output_not_output`, since a mapping
  library takes a collection as its source and nothing asks for the
  others.
- **What `P` may not hold** is what an `Output`'s fields may not: a
  caller, a password's hash or a secret value, however deep. Since `P` is
  an `Object`, which may hold any of them, the check is the route's: a
  layer whose properties hold one is `route.output_not_output` at
  `output:`, naming what and where. The same search follows a feature's
  properties wherever it looks, so an `Output` with a layer field whose
  properties hold a password's hash is refused at that field.
- **Field policy.** A layer an action returns is what its caller is
  shown, as an `Output` it builds is: every `return` of a
  `FeatureCollection` in an action carries only fields every caller of the
  action may read, or it is `policy.output_field_unproved` at the value
  returned (area 9, "Field restrictions"), whichever route or action
  calls it.
- **Sent as GeoJSON.** The response's body is the collection as
  `json.write` writes it, `{"type":"FeatureCollection","features":[...]}`,
  and a route test reads it back through the same codec, so
  `response.output` is the layer the action returned.

Only the output's declared fields are sent. `IssuedToken` and `IssuedKey`
(area 8) are built in and sent as the objects they are: an `id`,
`started_at` and `ends_at`. Failure statuses are never
written on a route: they come from the kinds of the failures the action can
produce (area 4).

## What a route runs

`run:` calls one named action, passing placeholders, `input` and the caller
(area 8) as ordinary arguments. A `GET` calls a named query instead, so it
can never write, as HTTP's safe methods promise, and a read needs no
wrapper action:

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

One more thing a `GET` runs: the listings the language generates for
sign-in credentials (area 8). `sessions.tokens(user_id)` and
`<Service>.keys(service_id)` read the tokens and keys the runtime keeps, so
a route lists them without an action that copies each one into an `Output`.
The route passes the holder's `id` from a placeholder or `current_principal`,
and its `output:` is the `List<IssuedToken>` or `List<IssuedKey>` the listing
returns, exactly as for a named query:

```jadpo fragment
GET /devices {
    output: List<IssuedToken>
    run: sessions.tokens(current_principal.user.id)
}
```

## Time limits

Every route has a deadline: 10 seconds unless `deadline:` says otherwise,
and never more than 60, since a hung request holds a database connection.
When it passes, the runtime starts no
new work, commits nothing and answers 504 `deadline_exceeded`. That holds
for work that never waits too, such as a long calculation: the runtime
checks the deadline as the work runs, and a body that finishes past it
commits nothing. Compiled code checks it as the interpreter does, every
4,096 calls, loop turns and inline-function calls, in a function's loops
and inside an operation's inline function as in an action's, an action's
loop giving the thread back to the requests beside it as it checks. The deadline
covers integration calls, which have their own shorter timeouts (area 11):
each call's `timeout:` times its attempts, summed along the longest way
through the route's action, must fit within it (area 11's
`route.calls_past_deadline`). Longer work answers
`status: SuccessStatus.accepted` and continues in a subscriber, or the
route waits for it with `settle:`.

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
result rather than `202`, the route waits for that work with `settle:`:

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
  waits: in the request's own invocation, each delivery as any is
  delivered (area 12), as the system limited to its event's tenant, the
  request's caller given back before the answer runs.
- The work has settled when every one of those deliveries has succeeded,
  failed for good or been parked as `outcome_unknown`: a failure that
  isn't retried, a defect, a subscriber the build no longer has, or an
  event 16 deep in a chain (`event_chain_limit`), so a loop settles too.
  A delivery left to retry after a backoff (`unavailable`, `rate_limited`,
  a store that couldn't be reached) has not, and can't be waited for, so
  the route stops waiting at once and answers 202; so does a delivery the
  request couldn't claim, and a batch run the action started, which is
  long work no route waits on, so a route whose action starts a batch
  answers 202 (area 12, "Schedule and batch defaults").
  Waiting stops at the first such piece, and the rest is left for after
  the response, the retry to the outbox's sweep.
- Then the route runs `answer:`, a named query given `result` (the
  action's result) or fields read through it, and nothing else: the query
  reads who is asking through its policy. It runs as the request's caller
  and under the query's policy, as a `GET` running it would, and answers
  with its result and the route's `status:`. If `within:` passes first, it
  runs the same query and answers `202`, so the client gets the same
  `output:` either way and knows from the status whether the work
  finished. The work carries on in the background either way, exactly as
  it would without `settle:`. The query's failures are the route's too,
  beside the action's, in OpenAPI and the record; a query that fails (its
  `missing:`, say) answers with that failure, though the action committed.
- With `settle:`, `output:` is the query's result type, not the action's.
  `within:` counts from when the request came in, as `deadline:` does, and
  must be shorter than the deadline, leaving the answer time to run; the
  answer isn't held to the deadline, so waiting never ends in a 504,
  because the action has already committed (`route.settle_invalid`).
- When `within:` passes, the subscriber's action that is running is
  dropped, nothing it did kept, and its delivery given back `pending` and
  due at once, that attempt not counted; only its body is raced, never
  its commit, and the delivery's own statements are never cut short, so a
  claim is never left in doubt.
- `settle:` goes only on a route that runs an action and answers
  `SuccessStatus.ok` or `.created`: a `GET` sets off nothing to wait for,
  `.accepted` would make both answers 202, and `.no_content` sends no
  answer (`route.settle_invalid`).
- Only work this action set off is waited for, never other requests' work
  or published copies of events (area 12).
- A repeat with the same `Idempotency-Key` runs the answer query again
  rather than replaying a stored body, so it shows how far the work has
  got: the key keeps the query's arguments, each written by its
  parameter's codec, with 202 until the work is known to have settled and
  then the route's status, so a process that stops between the commit and
  the answer never has a repeat say the work finished.
- OpenAPI lists the route's status, "once the work it set off has
  settled", and 202, each sending the query's result, and says a repeat
  runs the answer again; the API record adds a `settle within <duration>`
  line to the route.
- A route test's request waits in the same way. A test host's timers
  never end, so `within:` never passes in a test, and a follow-on left to
  retry gives 202.

## Streaming a model's answer

A route sends a model's answer as it is written, piece by piece, when
what it runs returns an `AiStream`, the text `ai.stream` asks a model
for (area 21). It is sent as server-sent events, the HTML standard's
`text/event-stream`, the shape OpenAI's and Anthropic's APIs stream in;
there are no WebSockets (area 21).

```jadpo check project=ai-stream file=app.jadpo hidden
application {
    connections: {
        local_model: {
            driver: Driver.openai_compatible
            url: config.model_url
        }
    }
    ai: {
        connection: local_model
        models: {
            default: config.drafting_model
        }
        limits: {
            per_day: {
                tokens: 2000000
            }
        }
    }
}
```

```jadpo check project=ai-stream file=config.jadpo hidden migrate=initial
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=ai-stream file=policy.jadpo hidden
use support

policy {
    actions: {
        support.stream_answer: [Access.public]
    }
}
```

```jadpo check project=ai-stream file=domains/support/types/customer_question.jadpo hidden
/// What a customer asked the help desk.
type CustomerQuestion = Text {
    max_length: 2000
}
```

```jadpo check project=ai-stream file=domains/support/types/asked.jadpo hidden
use support/types { CustomerQuestion }

/// A customer's question, as the help desk sends it.
type Asked = Input {
    customer_question: CustomerQuestion
}
```

```jadpo check project=ai-stream file=domains/support/failures/assistant_unavailable.jadpo hidden
/// The assistant can't answer now; a person will reply.
type AssistantUnavailable = Failure {
    kind: FailureKind.unavailable
    message: "The assistant can't answer now; a person will reply."
}
```

```jadpo check project=ai-stream file=domains/support/prompts/answer_question.jadpo hidden
use support/types { CustomerQuestion }

/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: CustomerQuestion
}
```

```jadpo check project=ai-stream file=domains/support/prompts/answer_question.prompt hidden
# Instructions

You answer questions from customers of an online bakery.

# Request

{question}
```

The action asks for the stream and maps how it can fail before its
first piece, as any outside call's failures are mapped; the route names
`output: AiStream`, exactly what the action returns:

```jadpo check project=ai-stream file=domains/support/actions/stream_answer.jadpo
use support/types { Asked }
use support/failures { AssistantUnavailable }
use support/prompts { AnswerQuestion }

/// Streams an answer to a customer's question as it is written.
action stream_answer(asked: Asked) fails AssistantUnavailable -> AiStream {
    return attempt ai.stream(
        AnswerQuestion {
            question: asked.customer_question
        },
        max_output: 600,
        timeout: 8s
    ) {
        refused: AssistantUnavailable
        timed_out: AssistantUnavailable
        over_budget: AssistantUnavailable
        too_long: AssistantUnavailable
    }
}
```

```jadpo check project=ai-stream file=domains/support/routes/help_desk.jadpo
use support/types { Asked }
use support/actions { stream_answer }

/// Customers' questions.
routes HelpDesk {
    prefix: /help_desk

    /// Streams the assistant's reply as it is written.
    POST / {
        authentication: RouteAuthentication.none
        input: Asked
        output: AiStream
        run: stream_answer(input)
    }
}
```

What a client reads, once the first piece has come:

```text
HTTP/1.1 200 OK
content-type: text/event-stream
cache-control: no-store
x-request-id: req_0b1e…

event: piece
data: {"text":"Our sourdough is "}

event: piece
data: {"text":"baked beside sesame."}

event: done
data: {}

```

and a stream that stopped part-way:

```text
event: piece
data: {"text":"Our sourdough is "}

event: error
data: {"error":{"code":"unavailable","message":"A dependency is temporarily unavailable.","request_id":"req_0b1e…"}}

```

| Event | Data | When |
|---|---|---|
| `piece` | `{"text": "..."}`: the next part of the answer, as the model sent it | As each part comes; none for an empty answer |
| `done` | `{}` | The answer is whole; the last event |
| `error` | The failure envelope (area 4), `unavailable`, or `internal_fault` for a defect | The answer stopped before it was whole; the last event |

- **Before the first piece** nothing has been sent, so the route answers
  as any other does: a failure the action's `attempt` mapped from
  `ai.stream`'s `refused`, `timed_out`, `over_budget` or `too_long`
  answers with its envelope and its kind's status, and the deadline
  passing with 504. The action has committed by the time the first piece
  goes out.
- **After it** the status, 200, has gone, so whatever stops the answer
  part-way ends the stream with an `error` event, `unavailable`: the
  model declining to go on, a cut-off at `max_output:`, the call's
  `timeout:` or the route's deadline passing, or the provider's
  connection breaking or sending what isn't a chat completion. The app's
  mapping can't run then, and the client needs only to know the answer
  is incomplete; the code and its wording are the runtime's own.
- **Every stream ends** with exactly one `done` or `error`, after which
  the response ends.
- **Each event** is an `event:` line, one `data:` line of compact JSON
  (so the text's newlines are escaped and never split an event) and a
  blank line, in UTF-8. A piece is what the provider sent, never cut or
  joined, empty ones left out.
- **No resuming.** A model's answer can't be asked for again from where
  it stopped, so events carry no `id:`, the server sends no `retry:` and
  `Last-Event-ID` isn't read; a client that loses a stream sends the
  request again. Only an action asks a model, so a streaming route is
  never a `GET`, and browsers' `EventSource`, which only sends `GET` and
  reconnects by itself, can't call one: a client reads it with `fetch`
  and a stream reader, as it reads OpenAI's and Anthropic's.
- **Time.** The stream is held to the earlier of the call's `timeout:`,
  counted from when the action asked, and the route's deadline, counted
  from when the request was matched; past it, `error`. That each
  `timeout:` an action reaches leaves room inside its route's deadline is
  checked with the deadlines of the route's other outside calls, a
  stream's counted once, since it asks once: at its default, 25 seconds,
  a streaming route gives itself `deadline:`.
- **A client that goes away** stops the stream: the runtime stops reading
  the model's answer and closes its connection to the provider at once,
  so nothing more is spent. What the action committed stays committed.
  On every host: natively the body is a chunked one fed as each event is
  read; on Workers a `ReadableStream` pulled as the client reads, and a
  client that goes away ends the request, which ends the model's call
  with it.
- **Answers 200 and waits for nothing.** A streaming route takes no other
  `status:` and no `settle:` (`route.stream_invalid`).
- **Idempotency.** A stream is never kept for a repeat: an
  `Idempotency-Key` keeps a repeat from running alongside the first until
  its action has committed, and a repeat once the stream has started runs
  again, as for a response that starts a session (`keeps: request` in
  `x-jadpo-idempotency`).
- **OpenAPI** lists the 200 as `text/event-stream`, its schema a string,
  described with its events, and each event's data's schema in
  `x-jadpo-stream` (`events: { piece:, done:, error: }`), which plain
  OpenAPI 3.1 has no field for; the failures before the first piece are
  listed as any route's are. **The API record** writes the route's output
  as `output AiStream (server-sent events)`:

```jadpo check project=ai-stream file=api/main.jadpo_record
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

POST /help_desk
    authentication none
    input Asked (body)
    output AiStream (server-sent events)
    status 200
    fails assistant_unavailable

GET /openapi.json
    generated openapi
    authentication none
    status 200

input Asked
    customer_question: Text { max_length: 2000 }

failure assistant_unavailable
    status 503
    message "The assistant can't answer now; a person will reply."
```

- **In a route test** a streaming route's `Response` has `pieces`, the
  text of each `piece` event in order, in place of `output`, and `code`,
  the `error` event's code when one ended it (`none` after `done`); its
  status is 200 once the first piece came. The `ai` fake cuts a scripted
  text into pieces of a few words, and `partial("...", <condition>)`
  scripts a stream that stops part-way (area 21, "Tests"):

```jadpo check project=ai-stream file=domains/support/routes/help_desk.test.jadpo
use support/types { Asked }

test "a reply comes piece by piece" {
    fakes: {
        ai.answer_question: ["Our sourdough is baked beside sesame."]
    }
    var response: Response = request {
        method: HttpMethod.post
        path: "/help_desk"
        input: Asked {
            customer_question: "Does your sourdough contain sesame?"
        }
    }
    assert response.status == 200
    assert response.pieces == ["Our sourdough is ", "baked beside sesame."]
    assert response.code == none
}

test "a failure after the first piece arrives as an error event" {
    fakes: {
        ai.answer_question: [partial("Our sourdough is baked", timed_out)]
    }
    var response: Response = request {
        method: HttpMethod.post
        path: "/help_desk"
        input: Asked {
            customer_question: "Does your sourdough contain sesame?"
        }
    }
    assert response.status == 200
    assert response.pieces == ["Our sourdough is ", "baked"]
    assert response.code == "unavailable"
}

test "a refusal before the first piece is the desk's own failure" {
    fakes: {
        ai.answer_question: [refused]
    }
    var response: Response = request {
        method: HttpMethod.post
        path: "/help_desk"
        input: Asked {
            customer_question: "Help me pick a lock"
        }
    }
    assert response.status == 503
    assert response.code == "assistant_unavailable"
}
```

## Health

The runtime always serves `GET /health/live` (the process runs) and
`GET /health/ready` (every store declared in `app.jadpo` answers within two
seconds; integrations are reported as unchecked). There is nothing to write,
and neither path is part of an API's OpenAPI or record. Readiness lists
each provider-package connection by name, sorted, as `unchecked`, without
making a provider call, whether stores are ready or unavailable.

## APIs and versions

Most applications have one API. One with several audiences (public, admin,
partners, webhooks) declares them in `app.jadpo`, like stores:

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
- Each API has its own OpenAPI document, record and default
  authentication. Its entry lists the sign-in methods it accepts,
  `authentication: [AuthenticationMethod.browser_session,
  AuthenticationMethod.api_token]`, which a route group or route may only
  narrow (area 8). Each API serves its OpenAPI document at `<prefix>/openapi.json`
  unless its entry says `serve_openapi: false` (see [OpenAPI](#openapi)).
- A new major version is another API (`v2: { prefix: /v2 }`), declared only
  when a break must live beside the old version; Stripe's dated versions
  with response rewriting suit a large public API and are not worth their
  cost here. Deleting `v1` makes the compiler report every group and type
  left unused.

## Webhooks

A provider's webhooks are generated from its package, not written (area
11): a connection with a webhook secret gets
`POST /webhooks/<connection>` in the `webhooks` API, whose runtime checks
the signature over the raw body, drops repeats by the provider's event id,
answers a handshake where the package names one, and writes each event to
the outbox for subscribers (area 12). A webhook that needs a reply runs
the app's answering action inside the request (area 11, "Receiving from
outside"). Its path can change; it cannot be switched off.

The route is
`<prefix>/<connection>` under the prefix of the `webhooks` API the app
declares, whose record and OpenAPI document list it, or `/webhooks/<connection>`
outside every declared API; `jadpo routes` shows it as `generated webhook`
with `signature` for its authentication, and an app route at its path is
`route.path_overlap` ([area 11's notes](notes/11-integrations.md)). Replies and handshakes are
area 11's ("Webhook replies, handshakes and further signature schemes").

Two senders reach ordinary routes: a system that
signs webhooks and has no package gets the app's own integration in
`shared/providers/`, whose webhooks are generated the same way; a partner
with an agreement signs in as a `Service` with a service key (area 8) and
calls an ordinary route.

## Idempotent writes

Every route but a `GET` honours `Idempotency-Key` (the IETF draft
Stripe's design became). HTTP calls `PUT` and `DELETE` idempotent, but a
retried `DELETE` answers 404 where the first answered 204, and the action
behind any write may raise events or call providers. The runtime handles
the key entirely; the action never sees it. Before the action runs, the
runtime claims the key, scoped to the caller; afterwards it keeps the
response with a digest of the request, in the action's transaction, for
24 hours. A repeat gets the kept response; the same key with a different
request fails `idempotency_key_reused` (422); a repeat while the first is
running fails `conflict` (409). Successes and failures the action returns
are kept; `unavailable` and internal failures are not, so a retry runs
again. Integration calls in the action send keys derived from it (area
11). An API can require the key with `idempotency: Idempotency.required`,
and OpenAPI lists the header. The details are in the [design notes](notes/07-http.md),
"Idempotency keys".

## How failures reach the client

One envelope with `code`, `message`, `request_id` and the failure's
client fields under `details` (area 4); the status comes from the kind;
`x-request-id` repeats the id. Bad input lists each problem without its
value, naming the field by its path in the `Input`, with list positions as
numbers, the names the OpenAPI schema uses (JSON Pointer's
`/lines/2/quantity` reads worse beside a form field):

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
| The body is not JSON, isn't sent as JSON (`Content-Type`), or the query string cannot be read | 400 `invalid_request` |
| A header's value isn't printable ASCII | 400 `invalid_request` |
| A field is missing, unknown or breaks a rule (body, query, header or placeholder), or the route reads no query string or body and one came | 422 `invalid_input` |
| No route matches | 404 `route_not_found` |
| The path exists but not for this method | 405 `method_not_allowed`, with `Allow` |
| The body is over the limit | 413 `request_too_large` |
| The deadline passed | 504 `deadline_exceeded` |
| An `Idempotency-Key` is missing where the API requires one, or isn't 1 to 255 printable ASCII characters | 400 `invalid_request` |
| An `Idempotency-Key` comes again with a different request | 422 `idempotency_key_reused` |
| An `Idempotency-Key` comes again while its first request is running | 409 `conflict` |

A request with a key that came before is the same request when its route
(method and declared path) and what it says, decoded, are the same: each
placeholder and the input, from the query string, the headers it reads
and the body, written back through the route's own codecs as the
contract's JSON. That JSON writes every secret value (`Password`,
`BearerToken` and the rest, area 19) as its kind alone, so the digest the
runtime keeps says nothing of a secret, whether it came in the body or the
path, and whoever can read the key table can't test guessed passwords
against it. So a repeat that differs only in a secret, or only in its
JSON's spacing or member order, is the same request, answered as the first
was; a response that starts a session or carries a credential is never
kept. The digest isn't keyed, since a key would need a secret every
deployment holds and Jadpo has none (area 10). A value that isn't secret,
sent and neither stored nor answered, can still be confirmed by guessing
by whoever reads the table, who reads the kept responses and the
application's own tables as well.

A deployable answers under its host's clock. When the clock can't be read
(the system clock before 1970), every request, health's included, is 500
`internal_fault` and nothing runs, its fault for the host's log saying the
host has no clock; `jadpo dev` doesn't start (`cli.dev_start_failed`, "The
system clock can't be read").

The envelope's `message` stays fixed English. Text for people in client
apps comes from the app's language files (area 10): every failure a route can produce has wording in every
supported locale, keyed by its code with its client fields as placeholders,
and client fields that are dates or numbers stay values on the wire for the
client to format in its own language.

## Generated routes

Some features generate routes: magic-link sign-in and passkeys (area 8), the privacy
requests (area 15), `POST /auth/exchange` (area 8), `GET /flags` (area 16),
a provider package's webhooks (area 11), each API's
`GET <prefix>/openapi.json` (below), and connecting and disconnecting an
OAuth grant, `POST /connections/<name>/start`,
`GET /connections/<name>/callback` and `POST /connections/<name>/disconnect`
(areas 8 and 11). Jadpo generates endpoints only, never pages: the app's
frontend owns every page. Generated routes appear in `jadpo routes`, the
API record and OpenAPI like any route, marked generated.

- A generated route takes a `path:` setting where its feature is
  configured (`magic_link: { path: "/login" }`), or `generate_routes: false`,
  after which the app writes its own routes over the feature's generated
  operations (`magic_links.request`, `magic_links.redeem`,
  `sessions.start`), which keep every protection.
- Webhook routes can change path but cannot be switched off, and so can
  passkeys', whose bodies are WebAuthn's rather than the app's (area 8).
- A generated route's response shape is fixed; only its path changes.

## Rate-limited requests

A request over a rate limit (area 16) gets `429` with the usual envelope,
code `rate_limited`, a `Retry-After` header giving the seconds to wait,
and the standard `RateLimit` and `RateLimit-Policy` headers (the IETF
draft `draft-ietf-httpapi-ratelimit-headers`; its current version still to
check). Its wording for people comes from translations, as every failure's
does (area 10), and is in the per-locale failure export.

```text
HTTP/1.1 429 Too Many Requests
Retry-After: 42
RateLimit-Policy: "default";q=5;w=3600
RateLimit: "default";r=0;t=42

{ "error": { "code": "rate_limited", "message": "Too many requests.", "request_id": "req_0b1e…" } }
```

## OpenAPI

The compiler writes one OpenAPI 3.1 document per API into `build/openapi/`:
every route with its input, output, success status and reachable failures,
the 400 and 422 input responses, and the `///` descriptions (area 1). The
title is the application's name and the version the first 12 hex digits
of the SHA-256 of the API's record.
Each route's responses follow from what it runs: 403 where policy can
refuse what it or an action it calls does, 503 where it uses a store, and
an `Idempotency-Key` that promises a repeat the first response only where
that response is kept. Every route shows its security scheme.

Beside each document, `jadpo check` writes (and `jadpo build` is to
write, not built yet) `<api>.failures.<locale>.json`
for each supported locale: each reachable failure's code mapped to
`{ "text": "…{next_slot}…", "formats": { "next_slot": "date_time" } }`, the
same shape for every code. `formats` lists only the placeholders the
client must format, each `number`, `decimal`, `date`, `date_time` or
`duration`, decided by the client field's type (area 10,
"Failures for client apps").

**Served, and readable by other Jadpo apps** (area 11, "Packages from
other Jadpo apps"):

- **Every API serves its document** at `<prefix>/openapi.json` by default
  (`/v1/openapi.json`, `/openapi.json` for `main` at `/`), under the API's
  own default authentication, so an admin API's document needs sign-in.
  `serve_openapi: false` in the API's entry in `apis: { }` turns it off.
- **The served route is listed like any generated route**: in the API's own
  OpenAPI document and in its API record, marked generated, and in
  `jadpo routes`.
- **`x-jadpo` fields** carry what plain OpenAPI lacks: `x-jadpo-changes`
  (the operation changes something), `x-jadpo-idempotency` (whether it
  takes `Idempotency-Key`, and whether the API requires it),
  `x-jadpo-failure` on each failure response (its `code` and `kind`),
  `x-jadpo-constraints` (every field's rules) and `x-jadpo-events` (the
  public events the app publishes). Other tools ignore them;
  area 11's OpenAPI importer reads them whenever it finds them, so another
  Jadpo app's document imports as an exact provider package with no
  corrections or recordings.

Each API with a document serves it at
`<prefix>/openapi.json`, and its record lists the route as the runtime's:

```text
GET /v1/openapi.json
    generated openapi
    authentication none
    status 200
```

- **Its authentication is the API's default:** a signed-in caller, by any
  method the API accepts, so the record line `authentication none` is left
  out. Where the API accepts no way to sign in that is built, every route
  it has lets anyone call it, so its document tells no one more than its
  routes do, and anyone may read it, as above.
- **It answers 200 with the document exactly as `jadpo check` writes it**
  to `build/openapi/<api>.json`, as `application/json`; like any route that
  takes no input, a query string or a body is 422 `invalid_input`. A route
  the application writes at the same path is `route.path_overlap`.
- **The fields' shapes:**

| Field | On | Holds |
|---|---|---|
| `x-jadpo-changes` | Every operation | `true` for a route that runs an action, `false` for a `GET` and for the runtime's own reads |
| `x-jadpo-idempotency` | Every operation that reads an `Idempotency-Key` | `header`, `required` (the API's `idempotency:`), `keeps` (`response`: a repeat gets the first answer; `request`: a repeat waits for the first but runs again; `nothing`: running it again changes nothing, or no store keeps it), `window` (`1d`), `max_length` (255), and the codes a key's repeat answers with, `in_use` (`conflict`) and `reused` (`idempotency_key_reused`) |
| `x-jadpo-failure` | The schema of each failure the application declares | Its `code` and `kind` (`not_found`); sign-in's, policy's and the runtime's own codes carry none, since the caller's runtime handles them |
| `x-jadpo-constraints` | Every field's, placeholder's and success response's schema | Its type and rules as the API record writes them: `Text { min_length: 1, max_length: 200 }`, `List<TodoView>`, `Money?` |

- **Each operation is tagged with its route group**, and the group's `///`
  description is the tag's, so an importer names a resource after it.
- **`x-jadpo-events` waits for publishing events outward** (area 12, "Where
  events travel"): an application publishes no event to another yet, so no
  document carries it.

## The public API record

A breaking change is caught at review, not by a client in production, as
Kotlin's API validator and Microsoft's API Extractor catch one. Each API
has a committed, readable record, `api/main.jadpo_record`, of its
routes, fields and failure codes. `jadpo check` fails while it disagrees with
the source and `jadpo api update` rewrites it, so every API change is a diff
of one file.

The record is written in the [record format](18-records-and-manifest.md#what-is-committed-and-why)
(area 18): its first line says
what generated it and which command updates it, a hand edit is
`record.edited`, and a record out of date is `record.out_of_date`.

A whole group, with what it runs and the record `jadpo api update` writes
for it. The record is the wire contract: a field shows what a client sends
or reads, objects and enums have an entry of their own, and each failure
code has an entry with its status, message and client fields. An API that
signs clients in says how in an `api main` entry (`authentication
browser_session`, one line per method), and a route for one kind of caller
says which (`caller user`), so switching an API from bearer tokens to
cookies is a diff of the record. Everything is sorted (routes by path,
then method; types, fields and variants by name; failures by code), so
reordering the source changes nothing. The record holds no version: two
branches that both change the API would always conflict on one. The
OpenAPI document's version is the first 12 hex digits of the record's
SHA-256, computed at build and never stored:

```jadpo check project=todos file=app.jadpo hidden
application {}
```

```jadpo check project=todos file=domains/planning/types/todo_title.jadpo hidden
type TodoTitle = Text {
    min_length: 1
    max_length: 200
}
```

```jadpo check project=todos file=domains/planning/entities/todo.jadpo hidden migrate=initial
use planning/types { TodoTitle }

entity Todo {
    id: Uuid
    title: TodoTitle
    done: Bool

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=todos file=domains/planning/types/todo_view.jadpo hidden
use planning/entities { Todo }

/// A todo as clients see it.
type TodoView = Output {
    pick: [Todo.id, Todo.title, Todo.done]
}
```

```jadpo check project=todos file=domains/planning/failures/todo_not_found.jadpo hidden
use planning/entities { Todo }

type TodoNotFound = Failure {
    kind: FailureKind.not_found
    message: "Todo not found."
    client: {
        todo_id: Todo.id
    }
}
```

```jadpo check project=todos file=domains/planning/queries/find_todo.jadpo hidden
use planning/entities { Todo }
use planning/types { TodoView }
use planning/failures { TodoNotFound }

query find_todo(todo_id: Todo.id) fails TodoNotFound -> TodoView {
    from: Todo
    where: Todo.id == todo_id
    missing: TodoNotFound {
        todo_id: todo_id
    }
}
```

```jadpo check project=todos file=domains/planning/actions/create_todo.jadpo hidden
use planning/entities { Todo }
use planning/types { TodoTitle, TodoView }

/// What creating a todo takes.
type CreateTodo = Input {
    title: TodoTitle
}

action create_todo(input: CreateTodo) -> TodoView {
    var todo: Todo = Todo.create {
        set: {
            title: input.title
            done: false
        }
    }
    return TodoView {
        id: todo.id
        title: todo.title
        done: todo.done
    }
}
```

```jadpo check project=todos file=domains/planning/routes/todos.jadpo
use planning/entities { Todo }
use planning/types { TodoView }
use planning/actions { CreateTodo, create_todo }
use planning/queries { find_todo }

/// Todos: making one and reading one back.
routes Todos {
    prefix: /todos

    /// Creates a todo.
    POST / {
        authentication: RouteAuthentication.none
        input: CreateTodo
        output: TodoView
        status: SuccessStatus.created
        run: create_todo(input)
    }

    /// One todo.
    GET /{todo_id: Todo.id} {
        authentication: RouteAuthentication.none
        output: TodoView
        run: find_todo(todo_id)
    }
}
```

```jadpo check project=todos file=api/main.jadpo_record
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

GET /openapi.json
    generated openapi
    authentication none
    status 200

POST /todos
    authentication none
    input CreateTodo (body)
    output TodoView
    status 201

GET /todos/{todo_id: Uuid}
    authentication none
    output TodoView
    status 200
    fails todo_not_found

input CreateTodo
    title: Text { min_length: 1, max_length: 200 }

output TodoView
    done: Bool
    id: Uuid
    title: Text { min_length: 1, max_length: 200 }

failure todo_not_found
    status 404
    message "Todo not found."
    todo_id: Uuid
```

```jadpo check project=todos file=domains/planning/routes/todos.test.jadpo hidden
use planning/types { TodoView }
use planning/actions { CreateTodo, create_todo }

test "a todo is created" {
    var response: Response = request {
        method: HttpMethod.post
        path: "/todos"
        input: CreateTodo {
            title: "Buy oat milk"
        }
    }
    assert response.status == 201
    assert response.code == none
}

test "a todo is read back by its id" {
    var basil: TodoView = create_todo(
        CreateTodo {
            title: "Water the basil"
        }
    )
    var response: Response = request {
        method: HttpMethod.get
        path: "/todos/{todo_id}"
        params: {
            todo_id: basil.id
        }
    }
    assert response.status == 200
    assert response.output == basil
}

test "a todo that isn't there is not found" {
    var response: Response = request {
        method: HttpMethod.get
        path: "/todos/{todo_id}"
        params: {
            todo_id: "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e"
        }
    }
    assert response.status == 404
    assert response.code == "todo_not_found"
}
```

A change that can break a client (a route removed, a required input field
added, an output field removed, a failure code changed) also needs an
acceptance written by a person, in the form area 18 decides for every
change that needs one (a database drop, a policy weakening, a new domain):
one file in `acceptances/` per decision, listing the changes it allows,
counted only in the change that adds it. `acceptances/` and `api/` are owned
in CODEOWNERS, so the host's review shows who approved it. Telling breaking
changes apart and requiring their acceptance are not built yet (area 18,
"What is built"); until they are, the record's diff is what a reviewer reads.

## Write revisions

Not built yet. Every write returns a `Jadpo-Revision` header, and browsers
get the same value as a cookie; a client that sends it back skips caches
(area 6), and one that does not may see a cached copy up to its `keep:`.

## What is built

Each feature not built yet is `toolchain.not_built_yet` where it is written.

| Feature | Status |
|---|---|
| Checking routes | Built: `jadpo check` checks route groups, their prefixes and APIs (with `idempotency:`), methods and typed placeholders, the one input (from the query string for `GET` and `DELETE`, the JSON body otherwise, and `header:` fields), `output:` and `status:`, `deadline:`'s range and what `run:` calls, every route of the project against the others and the health paths, and each API's record. |
| OpenAPI | Built: `jadpo check` writes one OpenAPI document per API to `build/openapi/`. Each API serves its own document at `<prefix>/openapi.json` (`serve_openapi: false` turns it off), listed in its record, its document and `jadpo routes`, and the documents carry the `x-jadpo` fields area 11's importer reads ("OpenAPI"). OpenAPI derives each route's responses from what it runs and shows `ZonedTime`, `Zone` and a mixed enum as they are sent. |
| Serving a request | Built: the runtime's HTTP component routes a request, decodes and checks its input (each problem as `{ field, rule }`), runs the action or query, and answers with the output or the failure envelope, its `request_id` and `x-request-id`, with the status the failure's kind sets; it answers `HEAD` and `OPTIONS` itself and counts the default rate limits. It reads a body only as JSON, and refuses a query string or body a route doesn't read, a header it can't read and a body over 1 MB. |
| Deadlines and concurrent changes | Built: the runtime stops a route at its deadline without committing, even work that never waits, and runs an action again on a concurrent change found as it commits. |
| Health endpoints | Built: `/health/live` and `/health/ready`. |
| Idempotent writes | Built: the runtime keeps each write's `Idempotency-Key` with its response. |
| Commands | Built: `jadpo dev` serves the routes and reloads on save, `jadpo routes` lists them and `jadpo api update` writes the records, each `api/<api>.jadpo_record` in the record format with no `version:` line; the OpenAPI version is a hash of it. |
| Native connection limits | Built: natively, a client has 10 seconds for its headers and 30 for its body, and the server holds at most 1,024 connections and 10 database connections per store per worker. |
| Sign-in (area 8) | Built: every route needs a signed-in caller unless it says `authentication: RouteAuthentication.none`; each API lists the sign-in methods it accepts, named in its record; a route passes `current_principal` to what it runs; a signed-in caller's requests count against its own rate limit, and its idempotency keys are its own; a request with a credential on a route anyone may call counts against its address before it runs. |
| Policy (area 9) | Built: policy decides what each route reaches, and OpenAPI lists the 403 `not_permitted` on every route that runs an action that writes or is listed under `actions:`. |
| Issued tokens and keys (area 8) | Built: a `GET` runs `sessions.tokens` and `<Service>.keys` itself, and the record and OpenAPI describe the `IssuedToken` and `IssuedKey` it sends. |
| Map layers | Built: a route may send a `FeatureCollection<G, P>` bare, its properties held to what an `Output` may hold and, where an action returns it, to the fields every caller may read (area 9); OpenAPI describes RFC 7946's object and the record names it with its type arguments. A `GET` sends one a query gives (area 6, "A query giving a map layer"), held to field policy where the query is declared. |
| `settle:` | Built: a route waits for the work its action set off ("Waiting for follow-on work", and `route.settle_invalid`). |
| Streaming | Built: a route whose action returns an `AiStream` sends a model's answer as server-sent events ("Streaming a model's answer", and `route.stream_invalid`). |
| `x-jadpo-events` | Not built yet: it waits for publishing events outward (area 12). |
| A route's own `rate_limit:` and the Redis store for rate limits | Not built yet (area 16). |
| Classifying breaking changes and their acceptances | Not built yet (area 18). |
| The `Jadpo-Revision` header | Not built yet: it comes with caches (area 6). |

## Errors

Every error this area can produce, with an example that triggers it and the
compiler's real output. Most examples are one loose file (area 1), so
the route sits beside what it runs; in a project each lives in its own
folder.

- [`syntax.expected_http_method`](../errors/syntax.expected_http_method.md)
- [`route.item_unknown`](../errors/route.item_unknown.md)
- [`route.item_duplicate`](../errors/route.item_duplicate.md)
- [`route.run_required`](../errors/route.run_required.md)
- [`route.run_invalid`](../errors/route.run_invalid.md)
- [`route.run_kind`](../errors/route.run_kind.md)
- [`route.argument_invalid`](../errors/route.argument_invalid.md)
- [`route.binding_unused`](../errors/route.binding_unused.md)
- [`route.output_required`](../errors/route.output_required.md)
- [`route.output_mismatch`](../errors/route.output_mismatch.md)
- [`route.output_not_output`](../errors/route.output_not_output.md)
- [`route.output_unexpected`](../errors/route.output_unexpected.md)
- [`route.input_not_input`](../errors/route.input_not_input.md)
- [`route.query_field_unsupported`](../errors/route.query_field_unsupported.md)
- [`route.status_invalid`](../errors/route.status_invalid.md)
- [`route.deadline_invalid`](../errors/route.deadline_invalid.md)
- [`route.settle_invalid`](../errors/route.settle_invalid.md)
- [`route.stream_invalid`](../errors/route.stream_invalid.md)
- [`route.authentication_invalid`](../errors/route.authentication_invalid.md)
- [`route.placeholder_type`](../errors/route.placeholder_type.md)
- [`route.placeholder_duplicate`](../errors/route.placeholder_duplicate.md)
- [`route.path_overlap`](../errors/route.path_overlap.md)
- [`route.prefix_required`](../errors/route.prefix_required.md)
- [`route.api_unknown`](../errors/route.api_unknown.md)
- [`route.header_name_invalid`](../errors/route.header_name_invalid.md)
- [`route.header_credential`](../errors/route.header_credential.md)
- [`route.header_duplicate`](../errors/route.header_duplicate.md)
- [`route.header_not_scalar`](../errors/route.header_not_scalar.md)
- [`route.prefix_api`](../errors/route.prefix_api.md)
- [`api.prefix_required`](../errors/api.prefix_required.md)
- [`api.prefix_overlap`](../errors/api.prefix_overlap.md)
- [`api.idempotency_invalid`](../errors/api.idempotency_invalid.md)
- [`api.authentication_invalid`](../errors/api.authentication_invalid.md)
- [`api.serve_openapi_invalid`](../errors/api.serve_openapi_invalid.md)
- [`api.setting_unknown`](../errors/api.setting_unknown.md)
