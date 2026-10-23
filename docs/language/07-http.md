---
title: HTTP
area: 7
status: decided
checked-with: jadpo chunk-4.9 (2026-10-16)
rules:
  - route.*
  - api.*
  - syntax.expected_http_method
---

# HTTP

How a Jadpo application answers HTTP requests: route groups, paths, the one
input a route takes, its output and status, time limits, health endpoints,
separate APIs and versions, webhooks, how failures reach the client,
OpenAPI, and how a breaking change to the API is caught.

> Decided on 2026-10-12. Code on this page applies the decisions from areas 1 to 7. Built in the new compiler in roadmap chunk 4.1: route groups, typed placeholders, the one input (query string, body and headers), outputs and statuses, what a route runs, APIs, the failure envelope with request ids, OpenAPI, the API record and the default rate limits, served by `jadpo dev`; the request guarantees (deadlines, the body limit, health and idempotent writes) in chunk 4.2; route authentication, each API's sign-in methods and `current_principal` in chunk 4.3 (area 8); policy on what each route reaches, and the 403 it can answer in OpenAPI, in chunk 4.4 (area 9); a `GET` listing tokens and keys itself, in chunk 4.4c (area 8); the generated `POST /auth/exchange` in the route list, the API record and OpenAPI, in chunk 5.1b (area 8); a map layer, a `FeatureCollection<G, P>`, sent bare, in chunk 5.5u7 (area 20), and from a `GET` running a query that gives one in chunk 5.5u8 (area 6). `settle:`, waiting for follow-on work, in chunk 6.3 (area 12). A model's answer streamed as server-sent events, in chunk 5.5v6 (area 21).

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
- A body is read only when it is sent as JSON (`Content-Type:
  application/json`, or another `application/…+json` type), so a form a
  browser posts from another site never reaches an action. A query string
  or a body a route doesn't read is unknown input, like an unknown field.

> Amended 2026-10-17 by area 19 (question 21): text in a request is
> normalised to Unicode NFC as it is decoded, so equal text compares
> equal (built in chunk 5.5f, for the body, the path, the query string
> and headers alike, before the input's rules are checked). The page
> examples that named a variable `text` were renamed in that chunk, since
> `text` is a reserved standard namespace.

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
and never more than 60 (question 5). When it passes, the runtime starts no
new work, commits nothing and answers 504 `deadline_exceeded`. That holds
for work that never waits too, such as a long calculation: the runtime
checks the deadline as the work runs, and a body that finishes past it
commits nothing. Compiled code checks it as the interpreter does, every
4,096 calls, loop turns and inline-function calls, in a function's loops
and inside an operation's inline function as in an action's, an action's
loop giving the thread back to the requests beside it as it checks
(chunk 8.4a). The deadline
covers integration calls, which have their own shorter timeouts (area 11):
each call's `timeout:` times its attempts, summed along the longest way
through the route's action, must fit within it (area 11's
`route.calls_past_deadline`, built in chunk 7.5e). Longer work answers
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
  answers 202 (area 12, "Schedule and batch defaults", 2026-10-19).
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
there are no WebSockets (area 21, question 6). Decided and built in
chunk 5.5v6.

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
  checked with the deadlines of the route's other outside calls (chunk
  7.5e), a stream's counted once, since it asks once (chunk 7.7e): at its
  default, 25 seconds, a streaming route gives itself `deadline:`.
- **A client that goes away** stops the stream: the runtime stops reading
  the model's answer and closes its connection to the provider at once,
  so nothing more is spent. What the action committed stays committed.
  On every host: natively the body is a chunked one fed as each event is
  read; on Workers a `ReadableStream` pulled as the client reads, and a
  client that goes away ends the request, which ends the model's call
  with it (chunk 8.2h).
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
and neither path is part of an API's OpenAPI or record (question 6).

Built in chunk 9.1d2b: the route table carries the checked provider-package
connection names in sorted order. Readiness reports each as `unchecked`
without making a provider call, whether stores are ready or unavailable.

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
  (area 8), and serves its OpenAPI document at `<prefix>/openapi.json`
  unless its entry says `serve_openapi: false` (see [OpenAPI](#openapi)).
- A new major version is another API (`v2: { prefix: /v2 }`), declared only
  when a break must live beside the old version. Deleting `v1` makes the
  compiler report every group and type left unused.

## Webhooks

A provider's webhooks are generated from its package, not written (area 11,
amending question 8 on 2026-10-13): a connection with a webhook secret gets
`POST /webhooks/<connection>` in the `webhooks` API, whose runtime checks
the signature over the raw body, drops repeats by the provider's event id,
answers a handshake where the package names one, and writes each event to
the outbox for subscribers (area 12). A webhook that needs a reply runs
the app's answering action inside the request (area 11, "Receiving from
outside", 2026-10-16). Its path can change; it cannot be switched off.

Built in chunk 7.7 (area 11, "Chosen in chunk 7.7"): the route is
`<prefix>/<connection>` under the prefix of the `webhooks` API the app
declares, whose record and OpenAPI document list it, or `/webhooks/<connection>`
outside every declared API; `jadpo routes` shows it as `generated webhook`
with `signature` for its authentication, and an app route at its path is
`route.path_overlap`. Replies and handshakes come with chunk 7.7b.

Two senders still reach ordinary routes (decided 2026-10-16): a system that
signs webhooks and has no package gets the app's own integration in
`shared/providers/`, whose webhooks are generated the same way; a partner
with an agreement signs in as a `Service` with a service key (area 8) and
calls an ordinary route.

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
apps comes from the app's language files (area 10, decided 2026-10-16,
built in chunk 5.4d): every failure a route can produce has wording in every
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
(areas 8 and 11). Jadpo generates endpoints
only, never pages: the app's frontend owns every page (decided
2026-10-16). Generated routes appear in `jadpo routes`, the API record and
OpenAPI like any route, marked generated (question 12).

- A generated route takes a `path:` setting where its feature is
  configured (`magic_link: { path: "/login" }`), or `generate_routes: false`,
  after which the app writes its own routes over the feature's generated
  operations (`magic_links.request`, `magic_links.redeem`,
  `sessions.start`), which keep every protection.
- Webhook routes can change path but cannot be switched off, and so can
  passkeys', whose bodies are WebAuthn's rather than the app's (area 8,
  chosen in chunk 5.5z).
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
that response is kept.

Beside each document, `jadpo check` writes (and `jadpo build` will write)
`<api>.failures.<locale>.json`
for each supported locale: each reachable failure's code mapped to
`{ "text": "…{next_slot}…", "formats": { "next_slot": "date_time" } }`, the
same shape for every code. `formats` lists only the placeholders the
client must format, each `number`, `decimal`, `date`, `date_time` or
`duration`, decided by the client field's type (area 10,
"Failures for client apps"; decided 2026-10-16).

**Served, and readable by other Jadpo apps** (decided 2026-10-16, Richard,
area 11's "Packages from other Jadpo apps"):

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

Built in chunk 7.2. Each API with a document serves it at
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
- **The fields' shapes** (chosen in chunk 7.2):

| Field | On | Holds |
|---|---|---|
| `x-jadpo-changes` | Every operation | `true` for a route that runs an action, `false` for a `GET` and for the runtime's own reads |
| `x-jadpo-idempotency` | Every operation that reads an `Idempotency-Key` | `header`, `required` (the API's `idempotency:`), `keeps` (`response`: a repeat gets the first answer; `request`: a repeat waits for the first but runs again; `nothing`: running it again changes nothing, or no store keeps it), `window` (`1d`), `max_length` (255), and the codes a key's repeat answers with, `in_use` (`conflict`) and `reused` (`idempotency_key_reused`) |
| `x-jadpo-failure` | The schema of each failure the application declares | Its `code` and `kind` (`not_found`); sign-in's, policy's and the runtime's own codes carry none, since the caller's runtime handles them |
| `x-jadpo-constraints` | Every field's, placeholder's and success response's schema | Its type and rules as the API record writes them: `Text { min_length: 1, max_length: 200 }`, `List<TodoView>`, `Money?` |

- **Each operation is tagged with its route group**, and the group's `///`
  description is the tag's, so an importer names a resource after it.
- **`x-jadpo-events` waits for publishing events outward** (area 12, "Event
  transport"): an application publishes no event to another yet, so no
  document carries it.

## The public API record

A breaking change is caught at review, not by a client in production
(question 10, researched in [the proposals](research/07-http-proposals.md)).
Each API has a committed, readable record, `api/main.jadpo_record`, of its
routes, fields and failure codes. `jadpo check` fails while it disagrees with
the source and `jadpo api update` rewrites it, so every API change is a diff
of one file.

The record is written in the [record format](18-records-and-manifest.md#what-is-committed-and-why)
(area 18, decided 2026-10-16, built in chunk 7.0): its first line says
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
changes apart and requiring their acceptance come after the first build
(area 18, "Status"); until then the record's diff is what a reviewer reads.

## Errors

Every error this area can produce, with an example that triggers it and the
new compiler's real output. Most examples are one loose file (area 1), so
the route sits beside what it runs; in a project each lives in its own
folder.

### `syntax.expected_http_method`

A route is written with `GET`, `POST`, `PUT`, `PATCH` or `DELETE`; a word another framework uses is read as the method it means:

```jadpo check project=e-expected-http-method expect=syntax.expected_http_method
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    CREATE / {
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `CREATE` isn't a method a route is written with
 --> main.jadpo:24:5
   |
24 |     CREATE / {
   |     ^^^^^^ not a route method
  reason: Routes are written with `GET`, `POST`, `PUT`, `PATCH` or `DELETE`.
  next: Use `POST` instead of `CREATE` (agent)
  rule: syntax.expected_http_method
  help: docs/language/07-http.html#syntax-expected-http-method

check failed: 1 error, 1 file
```

### `route.item_unknown`

A route holds `authentication:` (area 8), `input:`, `output:`, `status:`, `deadline:` and `run:` (with `rate_limit:` and `settle:` from later chunks); the old `success:`, `query:`, `path:` and `headers:` items are gone:

```jadpo check project=e-item-unknown expect=route.item_unknown
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        output: MemoView
        run: add_memo(input)
        success: SuccessStatus.created
    }
}
```

```text output
error: `success` isn't something a route holds
 --> main.jadpo:28:9
   |
28 |         success: SuccessStatus.created
   |         ^^^^^^^ unknown item
  reason: `success:` is now `status:`, which takes a `SuccessStatus`. A route holds `authentication`, `rate_limit`, `input`, `output`, `status`, `deadline`, `run` and `settle`.
  next: Use `status` instead of `success` (agent)
  rule: route.item_unknown
  help: docs/language/07-http.html#route-item-unknown

check failed: 1 error, 1 file
```

### `route.item_duplicate`

Each item is given once:

```jadpo check project=e-item-duplicate expect=route.item_duplicate
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        output: MemoView
        run: add_memo(input)
        run: add_memo(input)
    }
}
```

```text output
error: `run:` is given twice
 --> main.jadpo:28:9
   |
27 |         run: add_memo(input)
   |         --- first given here
28 |         run: add_memo(input)
   |         ^^^ given again
  reason: A route has one `run:`, so there is never a question of which counts.
  next: Remove one `run:` (agent)
  rule: route.item_duplicate
  help: docs/language/07-http.html#route-item-duplicate

check failed: 1 error, 1 file
```

### `route.run_required`

A route runs a named action, or a named query for a `GET`:

```jadpo check project=e-run-required expect=route.run_required
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        output: MemoView
    }
}
```

```text output
error: `POST /memos` doesn't say what it runs
 --> main.jadpo:24:5
   |
24 |     POST / {
   |     ^^^^^^ no `run:`
  reason: A route runs one named action, or a named query for a `GET`, with `run:`; the logic lives there, not in the route.
  next: Add `run: <action>(...)` (agent)
  rule: route.run_required
  help: docs/language/07-http.html#route-run-required

check failed: 1 error, 1 file
```

### `route.run_invalid`

There is no inline action in a route, and a function is called from an action, not run by a route:

```jadpo check project=e-run-invalid expect=route.run_invalid
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        output: MemoView
        run: true
    }
}
```

```text output
error: `run: true` isn't a call
 --> main.jadpo:27:14
   |
27 |         run: true
   |              ^^^^ not a call
  reason: `run:` calls one named action, or a named query for a `GET`, passing the route's placeholders and `input`; there is no inline action in a route.
  next: Write `run: <name>(...)`, moving any logic into an action (agent)
  rule: route.run_invalid
  help: docs/language/07-http.html#route-run-invalid

check failed: 1 error, 1 file
```

### `route.run_kind`

A `GET` never writes, so it runs a query; every other method runs an action:

```jadpo check project=e-run-kind expect=route.run_kind
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    GET / {
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `GET /memos` runs action `add_memo`
 --> main.jadpo:27:14
   |
27 |         run: add_memo(input)
   |              ^^^^^^^^ an action
  reason: A `GET` is safe to repeat and never writes, so it runs a named query; every other method runs an action.
  next: Run a query here, or answer `/memos` with another method (agent)
  rule: route.run_kind
  help: docs/language/07-http.html#route-run-kind

check failed: 1 error, 1 file
```

### `route.argument_invalid`

A route passes its placeholders, `input` and their fields; anything worked out goes in what it runs:

```jadpo check project=e-argument-invalid expect=route.argument_invalid
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST / {
        output: MemoView
        run: add_memo(
            AddMemo {
                text: "Hello"
            }
        )
    }
}
```

```text output
error: A route passes its placeholders and input, not a value built in the route
 --> main.jadpo:27:13
   |
27 |             AddMemo {
   |             ^^^^^^^^^ a value built in the route
  reason: A route is a thin door: what it runs gets the route's placeholders, its `input` and their fields as written, and works out anything else itself.
  next: Pass a placeholder, `input` or a field of them, and move the rest into what the route runs (agent)
  rule: route.argument_invalid
  help: docs/language/07-http.html#route-argument-invalid

check failed: 1 error, 1 file
```

### `route.binding_unused`

Everything a route reads from a request reaches what it runs:

```jadpo check project=e-binding-unused expect=route.binding_unused
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id: MemoId} {
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `POST /memos/{memo_id}` reads placeholder `memo_id` and never passes it on
 --> main.jadpo:27:14
   |
27 |         run: add_memo(input)
   |              ^^^^^^^^ `memo_id` not passed
  reason: Everything a route reads from a request reaches what it runs by name; a value read and ignored is a URL or a field that does nothing.
  next: Pass `memo_id` to `add_memo`, or remove it from the route (agent)
  rule: route.binding_unused
  help: docs/language/07-http.html#route-binding-unused

check failed: 1 error, 1 file
```

### `route.output_required`

A route shows its contract without opening what it runs:

```jadpo check project=e-output-required expect=route.output_required
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        run: add_memo(input)
    }
}
```

```text output
error: `POST /memos` has no `output:`
 --> main.jadpo:24:5
   |
24 |     POST / {
   |     ^^^^^^ no `output:`
  reason: A route shows its contract without opening what it runs: `output:` names what it sends, exactly what `add_memo` returns (`MemoView`).
  next: Add `output: MemoView` (agent)
  rule: route.output_required
  help: docs/language/07-http.html#route-output-required

check failed: 1 error, 1 file
```

What returns `Unit` sends nothing, so the route answers 204 rather than
naming an `output:` (Phase 6 review, chunk 6.6b):

```jadpo check project=e-output-required-unit expect=route.output_required
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

action check_memo(input: AddMemo) -> Unit {
    assert input.text == input.text
}

routes Memos {
    prefix: /memos

    POST /checks {
        input: AddMemo
        run: check_memo(input)
    }
}
```

```text output
error: `POST /memos/checks` has no `output:`
 --> main.jadpo:16:5
   |
16 |     POST /checks {
   |     ^^^^^^^^^^^^ no `output:`
  reason: A route shows its contract without opening what it runs: `check_memo` returns `Unit`, so the route sends nothing, which `status: SuccessStatus.no_content` says, answering 204 with no body.
  next: Add `status: SuccessStatus.no_content` (agent)
  rule: route.output_required
  help: docs/language/07-http.html#route-output-required

check failed: 1 error, 1 file
```

### `route.output_mismatch`

`output:` is exactly what the route runs returns:

```jadpo check project=e-output-mismatch expect=route.output_mismatch
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

type MemoSummary = Output {
    text: Memo
}

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        output: MemoSummary
        run: add_memo(input)
    }
}
```

```text output
error: `POST /memos` sends `MemoSummary`, but `add_memo` returns `MemoView`
 --> main.jadpo:30:17
   |
30 |         output: MemoSummary
   |                 ^^^^^^^^^^^ not `MemoView`
  reason: A route's `output:` is exactly what it runs returns, so the route shows its contract and never sends a field its type doesn't declare.
  next: Write `output: MemoView` (agent)
  rule: route.output_mismatch
  help: docs/language/07-http.html#route-output-mismatch

check failed: 1 error, 1 file
```

### `route.output_not_output`

A route sends only an `Output`, or a map layer whose properties hold only what an `Output` may, so only fields declared for clients leave the server:

```jadpo check project=e-output-not-output expect=route.output_not_output
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

type MemoRecord = Object {
    text: Memo
}

action keep_memo(input: AddMemo) -> MemoRecord {
    return MemoRecord {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        output: MemoRecord
        run: keep_memo(input)
    }
}
```

```text output
error: `MemoRecord` isn't an `Output`
 --> main.jadpo:36:17
   |
36 |         output: MemoRecord
   |                 ^^^^^^^^^^ not an `Output`
  reason: A route returns only an `Output` type (area 2), a `List` or `Page` of one as a query gives them, a `FeatureCollection<G, P>` whose properties `P` hold only what an `Output` may (area 20), or an `AiStream`, a model's answer sent as it is written, so only fields declared for clients are ever sent.
  next: Declare `type <Name> = Output { ... }` with the fields clients see, and return it (agent)
  rule: route.output_not_output
  help: docs/language/07-http.html#route-output-not-output

check failed: 1 error, 1 file
```

### `route.output_unexpected`

A `no_content` route sends no body:

```jadpo check project=e-output-unexpected expect=route.output_unexpected
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        output: MemoView
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
}
```

```text output
error: `POST /memos` answers with no content
 --> main.jadpo:26:9
   |
26 |         output: MemoView
   |         ^^^^^^^^^^^^^^^^ no body is sent
  reason: `SuccessStatus.no_content` answers 204 with no body, so the route has no `output:` and what it runs returns `Unit`.
  next: Remove `output:`, or choose another status (agent)
  rule: route.output_unexpected
  help: docs/language/07-http.html#route-output-unexpected

check failed: 1 error, 1 file
```

### `route.input_not_input`

A route accepts only an `Input`:

```jadpo check project=e-input-not-input expect=route.input_not_input
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST / {
        input: MemoView
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `MemoView` isn't an `Input`
 --> main.jadpo:25:16
   |
25 |         input: MemoView
   |                ^^^^^^^^ not an `Input`
  reason: A route accepts only an `Input` type (area 2): the only objects that may leave fields out or fill them by default, and the ones a request's fields are checked against.
  next: Declare `type <Name> = Input { ... }` and take it here (agent)
  rule: route.input_not_input
  help: docs/language/07-http.html#route-input-not-input

check failed: 1 error, 1 file
```

### `route.query_field_unsupported`

A `GET` or `DELETE` reads its input from the query string, which holds scalars, enums and lists of them (an amount of money, a quantity and a `ZonedTime`, which JSON sends as objects, are objects here too, chunk 5.5w2):

```jadpo check project=e-query-field-unsupported expect=route.query_field_unsupported
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

type ForgetMemos = Input {
    tags: Map<Memo, Memo>
}

action forget_memos(input: ForgetMemos) -> MemoView {
    return MemoView {
        text: "forgotten"
    }
}

routes Memos {
    prefix: /memos

    DELETE / {
        input: ForgetMemos
        output: MemoView
        run: forget_memos(input)
    }
}
```

```text output
error: `ForgetMemos.tags` can't come in a query string
 --> main.jadpo:35:16
   |
35 |         input: ForgetMemos
   |                ^^^^^^^^^^^ `tags` is `Map<Memo, Memo>`
  reason: A `DELETE` route reads its input from the query string, which holds scalars, enums and lists of them (`?status=open&status=done`); nested objects and maps need a body.
  next: Give `tags` a scalar type, or take this input on a `POST` (agent)
  rule: route.query_field_unsupported
  help: docs/language/07-http.html#route-query-field-unsupported

check failed: 1 error, 1 file
```

### `route.status_invalid`

A success status is a `SuccessStatus`; failure statuses come from failure kinds:

```jadpo check project=e-status-invalid expect=route.status_invalid
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        output: MemoView
        status: 201
        run: add_memo(input)
    }
}
```

```text output
error: `201` isn't a success status
 --> main.jadpo:27:17
   |
27 |         status: 201
   |                 ^^^ not a `SuccessStatus`
  reason: `status:` takes `SuccessStatus.ok`, `.created`, `.accepted` or `.no_content`; failure statuses come from the kinds of the failures the route can produce, never from the route.
  next: Write `SuccessStatus.created` (agent)
  rule: route.status_invalid
  help: docs/language/07-http.html#route-status-invalid

check failed: 1 error, 1 file
```

### `route.deadline_invalid`

A deadline is above zero and at most 60 seconds:

```jadpo check project=e-deadline-invalid expect=route.deadline_invalid
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        output: MemoView
        deadline: 90s
        run: add_memo(input)
    }
}
```

```text output
error: `90s` can't be a route's deadline
 --> main.jadpo:27:19
   |
27 |         deadline: 90s
   |                   ^^^ outside 1 ms to 60 s
  reason: A route's deadline is a duration above zero and at most 60 seconds; longer work answers `SuccessStatus.accepted` and carries on in a subscriber.
  next: Write a duration such as `30s`, or leave `deadline:` out for the 10-second default (agent)
  rule: route.deadline_invalid
  help: docs/language/07-http.html#route-deadline-invalid

check failed: 1 error, 1 file
```

### `route.settle_invalid`

`settle:` holds `answer:`, a named query given `result` or its fields,
and `within:`, shorter than the route's deadline, on a route that runs
an action and answers `.ok` or `.created`:

```jadpo check project=e-settle-invalid expect=route.settle_invalid
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST / {
        input: AddMemo
        output: MemoView
        status: SuccessStatus.created
        run: add_memo(input)
        settle: {
            answer: add_memo(input)
            within: 5s
        }
    }
}
```

```text output
error: `answer: add_memo` is an action
 --> main.jadpo:28:21
   |
28 |             answer: add_memo(input)
   |                     ^^^^^^^^ an action here
  reason: `settle:` holds `answer:`, a named query given `result` (what the action returns) or fields read through it, and `within:`, how long the route waits for the work its action set off, a duration shorter than the route's `deadline:`.
  next: Answer with a named query, which reads how far the work has got (agent)
  rule: route.settle_invalid
  help: docs/language/07-http.html#route-settle-invalid

check failed: 1 error, 1 file
```

### `route.stream_invalid`

A route whose action returns an `AiStream` sends the model's answer as
it is written, answering 200 once the first piece has come; it takes no
other `status:` and no `settle:`:

```jadpo check project=e-route-stream-invalid file=app.jadpo hidden
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

```jadpo check project=e-route-stream-invalid file=config.jadpo hidden migrate=initial
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=e-route-stream-invalid file=policy.jadpo hidden
use support

policy {
    actions: {
        support.stream_answer: [Access.public]
    }
}
```

```jadpo check project=e-route-stream-invalid file=domains/support/types/customer_question.jadpo hidden
/// What a customer asked the help desk.
type CustomerQuestion = Text {
    max_length: 2000
}
```

```jadpo check project=e-route-stream-invalid file=domains/support/types/asked.jadpo hidden
use support/types { CustomerQuestion }

/// A customer's question, as the help desk sends it.
type Asked = Input {
    customer_question: CustomerQuestion
}
```

```jadpo check project=e-route-stream-invalid file=domains/support/failures/assistant_unavailable.jadpo hidden
/// The assistant can't answer now; a person will reply.
type AssistantUnavailable = Failure {
    kind: FailureKind.unavailable
    message: "The assistant can't answer now; a person will reply."
}
```

```jadpo check project=e-route-stream-invalid file=domains/support/prompts/answer_question.jadpo hidden
use support/types { CustomerQuestion }

/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: CustomerQuestion
}
```

```jadpo check project=e-route-stream-invalid file=domains/support/prompts/answer_question.prompt hidden
# Instructions

You answer questions from customers of an online bakery.

# Request

{question}
```

```jadpo check project=e-route-stream-invalid file=domains/support/actions/stream_answer.jadpo hidden
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

```jadpo check project=e-route-stream-invalid file=domains/support/routes/help_desk.jadpo expect=route.stream_invalid
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
        status: SuccessStatus.created
        run: stream_answer(input)
    }
}
```

```text output
error: `POST /help_desk` streams a model's answer with `SuccessStatus.created`
 --> domains/support/routes/help_desk.jadpo:13:9
   |
13 |         status: SuccessStatus.created
   |         ^^^^^^ not `SuccessStatus.ok`
  reason: A route whose action returns an `AiStream` sends the model's answer as server-sent events as it is written, answering 200 once its first piece has come; it has no other success to report, and nothing to wait for with `settle:`.
  next: Remove `status:`; a stream answers 200 (agent)
  rule: route.stream_invalid
  help: docs/language/07-http.html#route-stream-invalid

check failed: 1 error, 9 files
```

### `route.authentication_invalid`

A route says only when it differs from needing a signed-in caller, and an
unclear value is refused without a guess, since one fix keeps the route
protected and the other opens it (area 8):

```jadpo check project=e-route-authentication expect=route.authentication_invalid
type Memo = Text {
    max_length: 200
}

type AddMemo = Input {
    text: Memo
}

action add_memo(input: AddMemo) -> Unit {
    var memo: Memo = input.text
    assert memo != ""
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: none
        input: AddMemo
        status: SuccessStatus.no_content
        run: add_memo(input)
    }
}
```

```text output
error: `none` isn't a `RouteAuthentication`
 --> main.jadpo:18:25
   |
18 |         authentication: none
   |                         ^^^^ not `.none` or `.fresh`
  reason: Every route needs a signed-in caller unless it says otherwise: `RouteAuthentication.none` lets anyone call it, and `RouteAuthentication.fresh` checks the caller's record on this request. One opens the route and the other keeps it protected, so neither is guessed.
  next: Write `RouteAuthentication.fresh` to keep the route signed in, `RouteAuthentication.none` to let anyone call it, or remove `authentication:` (human)
  rule: route.authentication_invalid
  help: docs/language/07-http.html#route-authentication-invalid

check failed: 1 error, 1 file
```

### `route.placeholder_type`

A placeholder is typed with a named scalar or an `X.id`, never optional:

```jadpo check project=e-placeholder-type expect=route.placeholder_type
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id: MemoId?} {
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }
}
```

```text output
error: `memo_id` can't be typed `MemoId?`
 --> main.jadpo:30:21
   |
30 |     POST /{memo_id: MemoId?} {
   |                     ^^^^^^^ not a named scalar
  reason: A placeholder's type is a named scalar or an `X.id`: a placeholder is always there, so it is never optional.
  next: Type it with a named type built on a scalar, or an `X.id` (agent)
  rule: route.placeholder_type
  help: docs/language/07-http.html#route-placeholder-type

check failed: 1 error, 1 file
```

A placeholder typed with a field its type doesn't declare is
`type.unknown_field` at its type, as any field type is (chunk 6.6d):

```jadpo check project=e-placeholder-unknown-field expect=type.unknown_field
type NoteText = Text {
    max_length: 200
}

type NoteView = Output {
    text: NoteText
}

entity Note {
    id: Uuid
    text: NoteText

    policy: {
        read: [Access.public]
    }
}

query find_note(note_id: Note.id) fails NoteNotFound -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}

routes Notes {
    prefix: /notes

    GET /{note_id: Note.ident} {
        authentication: RouteAuthentication.none
        output: NoteView
        run: find_note(note_id)
    }
}
```

```text output
error: `Note` has no field `ident`
 --> main.jadpo:27:20
   |
27 |     GET /{note_id: Note.ident} {
   |                    ^^^^^^^^^^ not declared
  reason: A field type is written `Type.field`, naming a field the type declares.
  next: Name a field of `Note` (agent)
  rule: type.unknown_field
  help: docs/language/02-values-and-types.html#type-unknown-field

check failed: 1 error, 1 file
```

### `route.placeholder_duplicate`

Each placeholder is named once in a path, its group's prefix included:

```jadpo check project=e-placeholder-duplicate expect=route.placeholder_duplicate
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos/{memo_id: MemoId}

    POST /{memo_id: MemoId} {
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }
}
```

```text output
error: Placeholder `memo_id` appears twice
 --> main.jadpo:30:12
   |
28 |     prefix: /memos/{memo_id: MemoId}
   |                     ------- first named here
30 |     POST /{memo_id: MemoId} {
   |            ^^^^^^^ named again
  reason: Each placeholder reaches what the route runs by its name, so a path names each once, its group's prefix included.
  next: Rename one `memo_id` (agent)
  rule: route.placeholder_duplicate
  help: docs/language/07-http.html#route-placeholder-duplicate

check failed: 1 error, 1 file
```

### `route.path_overlap`

Two routes that could answer one request are an error; a fixed segment beats a placeholder, so `/memos/archived` could sit beside `/memos/{memo_id}`:

```jadpo check project=e-path-overlap expect=route.path_overlap
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }

    POST /{shelf_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(shelf_id, input)
    }
}
```

```text output
error: `POST /memos/{shelf_id}` could answer the same requests as `POST /memos/{memo_id}`
 --> main.jadpo:37:5
   |
30 |     POST /{memo_id: MemoId} {
   |     ----------------------- already answered here
37 |     POST /{shelf_id: MemoId} {
   |     ^^^^^^^^^^^^^^^^^^^^^^^^ answers the same requests
  reason: A request such as `/memos/…` matches both `POST /memos/{memo_id}` and `POST /memos/{shelf_id}`: where two paths differ, one must have fixed text where the other has a placeholder, since a fixed segment beats a placeholder.
  next: Give one of the paths fixed text where the other has a placeholder (agent)
  rule: route.path_overlap
  help: docs/language/07-http.html#route-path-overlap

check failed: 1 error, 1 file
```

### `route.prefix_required`

Every group has a prefix, which names its file:

```jadpo check project=e-prefix-required expect=route.prefix_required
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    POST / {
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `Memos` has no prefix
 --> main.jadpo:21:8
   |
21 | routes Memos {
   |        ^^^^^ no `prefix:`
  reason: Every route in a group follows the group's prefix, and the prefix's fixed segments give the file its name, so a URL from a log leads to one file.
  next: Add `prefix: /<path>` as the group's first line (agent)
  rule: route.prefix_required
  help: docs/language/07-http.html#route-prefix-required

check failed: 1 error, 1 file
```

### `route.api_unknown`

A group names an API `app.jadpo` declares; with none declared there is one, `main`:

```jadpo check project=e-api-unknown expect=route.api_unknown
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos
    api: admin

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `Memos` belongs to API `admin`, which isn't declared
 --> main.jadpo:23:10
   |
23 |     api: admin
   |          ^^^^^ not declared
  reason: With no `apis:` in `app.jadpo` there is one API, `main`, at `/`.
  next: Declare `admin` under `apis:` in `app.jadpo` (agent)
  rule: route.api_unknown
  help: docs/language/07-http.html#route-api-unknown

check failed: 1 error, 1 file
```

### `route.header_name_invalid`

A field read from a header names it as HTTP writes it:

```jadpo check project=e-header-name-invalid expect=route.header_name_invalid
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

type TaggedMemo = Input {
    text: Memo
    tag: Memo {
        header: "Request Tag"
        optional: true
    }
}

action add_tagged_memo(input: TaggedMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

policy {
    actions: {
        add_tagged_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: TaggedMemo
        output: MemoView
        run: add_tagged_memo(input)
    }
}
```

```text output
error: `Request Tag` isn't a header name
 --> main.jadpo:24:17
   |
24 |         header: "Request Tag"
   |                 ^^^^^^^^^^^^^ not a header name
  reason: A header name is letters, digits and `-` (HTTP allows a few more symbols), with no spaces: `X-Request-Tag`.
  next: Write the header's name without spaces (agent)
  rule: route.header_name_invalid
  help: docs/language/07-http.html#route-header-name-invalid

check failed: 1 error, 1 file
```

### `route.header_credential`

Credentials and Jadpo's own headers belong to authentication (area 8),
and `Idempotency-Key` to the runtime, which reads it before anything runs
(question 17):

```jadpo check project=e-header-credential expect=route.header_credential
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

type TaggedMemo = Input {
    text: Memo
    token: Memo {
        header: "Authorization"
    }
}

action add_tagged_memo(input: TaggedMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

policy {
    actions: {
        add_tagged_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: TaggedMemo
        output: MemoView
        run: add_tagged_memo(input)
    }
}
```

```text output
error: `Authorization` belongs to authentication
 --> main.jadpo:24:17
   |
24 |         header: "Authorization"
   |                 ^^^^^^^^^^^^^^^ read by authentication
  reason: Credentials (`Authorization`, cookies) and Jadpo's own `X-Jadpo-` headers are read by authentication (area 8), so a route never sees them as input.
  next: Remove the field, and take the caller as the route's `current_principal` (agent)
  rule: route.header_credential
  help: docs/language/07-http.html#route-header-credential

check failed: 1 error, 1 file
```

### `route.header_duplicate`

Header names are compared without case:

```jadpo check project=e-header-duplicate expect=route.header_duplicate
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

type TaggedMemo = Input {
    text: Memo
    first: Memo {
        header: "X-Request-Tag"
        optional: true
    }
    second: Memo {
        header: "x-request-tag"
        optional: true
    }
}

action add_tagged_memo(input: TaggedMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

policy {
    actions: {
        add_tagged_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: TaggedMemo
        output: MemoView
        run: add_tagged_memo(input)
    }
}
```

```text output
error: Header `x-request-tag` is read twice
 --> main.jadpo:28:17
   |
24 |         header: "X-Request-Tag"
   |                 --------------- first read here
28 |         header: "x-request-tag"
   |                 ^^^^^^^^^^^^^^^ read again
  reason: Header names are compared without case, so two fields reading one header would always hold the same value.
  next: Remove one of the fields (agent)
  rule: route.header_duplicate
  help: docs/language/07-http.html#route-header-duplicate

check failed: 1 error, 1 file
```

### `route.header_not_scalar`

A header holds one value:

```jadpo check project=e-header-not-scalar expect=route.header_not_scalar
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

type TaggedMemo = Input {
    text: Memo
    tags: List<Memo> {
        header: "X-Tags"
        optional: true
    }
}

action add_tagged_memo(input: TaggedMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

policy {
    actions: {
        add_tagged_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: TaggedMemo
        output: MemoView
        run: add_tagged_memo(input)
    }
}
```

```text output
error: A header can't hold `List<Memo>`
 --> main.jadpo:24:17
   |
24 |         header: "X-Tags"
   |                 ^^^^^^^^ one value per header
  reason: A header holds one value, so a field read from one is a scalar or an enum without data; lists and objects come in the body.
  next: Move the field to the body, or give it a scalar type (agent)
  rule: route.header_not_scalar
  help: docs/language/07-http.html#route-header-not-scalar

check failed: 1 error, 1 file
```

### `route.prefix_api`

A group's prefix starts with its API's, so each API's routes, OpenAPI
document and record stay its own:

```jadpo check project=e-prefix-api file=app.jadpo
application {
    apis: {
        main: {
            prefix: /v1
        }
    }
}
```

```jadpo check project=e-prefix-api file=domains/memos/actions/add_memo.jadpo hidden
use memos/types { Memo, MemoView }

type AddMemo = Input {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}
```

```jadpo check project=e-prefix-api file=domains/memos/types/memo.jadpo hidden
type Memo = Text {
    max_length: 200
}
```

```jadpo check project=e-prefix-api file=domains/memos/types/memo_view.jadpo hidden
use memos/types { Memo }

type MemoView = Output {
    text: Memo
}
```

```jadpo check project=e-prefix-api file=domains/memos/routes/memos.jadpo expect=route.prefix_api
use memos/types { MemoView }
use memos/actions { AddMemo, add_memo }

routes Memos {
    prefix: /memos

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: add_memo(input)
    }
}
```

```text output
error: `Memos`'s prefix doesn't start with API `main`'s
 --> domains/memos/routes/memos.jadpo:5:13
  |
5 |     prefix: /memos
  |             ^^^^^^ outside `/v1`
  reason: A group's prefix starts with its API's (`/v1` for `main`), so each API's routes, OpenAPI document and record stay its own.
  next: Start the prefix with `/v1`, or name the group's API with `api:` (agent)
  rule: route.prefix_api
  help: docs/language/07-http.html#route-prefix-api

check failed: 1 error, 5 files
```

### `api.prefix_required`

Every declared API says the path its routes start with:

```jadpo check project=e-api-prefix file=app.jadpo expect=api.prefix_required
application {
    apis: {
        main: {
            prefix: /v1
        }
        admin: {}
    }
}
```

```text output
error: API `admin` has no prefix
 --> app.jadpo:6:9
  |
6 |         admin: {}
  |         ^^^^^ no `prefix:`
  reason: Every declared API says the path its routes start with, so a URL says which API it belongs to: `main: { prefix: /v1 }`.
  next: Add `prefix: /admin` to `admin` (agent)
  rule: api.prefix_required
  help: docs/language/07-http.html#api-prefix-required

check failed: 1 error, 1 file
```

### `api.prefix_overlap`

Each API has a prefix of its own:

```jadpo check project=e-api-overlap file=app.jadpo expect=api.prefix_overlap
application {
    apis: {
        main: {
            prefix: /v1
        }
        partners: {
            prefix: /v1
        }
    }
}
```

```text output
error: APIs `main` and `partners` share the prefix `/v1`
 --> app.jadpo:6:9
  |
6 |         partners: {
  |         ^^^^^^^^ same prefix
  reason: Each API's routes start with its own prefix, so a URL says which API, OpenAPI document and record it belongs to.
  next: Give `partners` a prefix of its own (agent)
  rule: api.prefix_overlap
  help: docs/language/07-http.html#api-prefix-overlap

check failed: 1 error, 1 file
```

### `api.idempotency_invalid`

An API that refuses writes without an `Idempotency-Key` says so with an `Idempotency`:

```jadpo check project=e-api-idempotency file=app.jadpo expect=api.idempotency_invalid
application {
    apis: {
        main: {
            prefix: /v1
            idempotency: true
        }
    }
}
```

```text output
error: `true` isn't an `Idempotency`
 --> app.jadpo:5:26
  |
5 |             idempotency: true
  |                          ^^^^ not an `Idempotency`
  reason: `idempotency:` takes `Idempotency.required`, so a write without an `Idempotency-Key` header is refused, or `Idempotency.optional`, the default, where a client may send one.
  next: Write `Idempotency.required`, or leave `idempotency:` out (agent)
  rule: api.idempotency_invalid
  help: docs/language/07-http.html#api-idempotency-invalid

check failed: 1 error, 1 file
```

### `api.authentication_invalid`

An API lists the sign-in methods it accepts, each one `authentication.jadpo`
declares (area 8):

```jadpo check project=e-api-authentication file=app.jadpo expect=api.authentication_invalid
application {
    apis: {
        main: {
            prefix: /v1
            authentication: [AuthenticationMethod.browser_session]
        }
    }
}
```

```text output
error: API `main` accepts `browser_session`, which isn't declared
 --> app.jadpo:5:30
  |
5 |             authentication: [AuthenticationMethod.browser_session]
  |                              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not declared
  reason: An API accepts only the ways to sign in the application declares; `authentication.jadpo` declares no way to sign in yet.
  next: List the methods the API accepts, each one `authentication.jadpo` declares (agent)
  rule: api.authentication_invalid
  help: docs/language/07-http.html#api-authentication-invalid

check failed: 1 error, 1 file
```

### `api.serve_openapi_invalid`

An API serves its OpenAPI document unless its entry turns it off with
`false`:

```jadpo check project=e-api-serve-openapi file=app.jadpo expect=api.serve_openapi_invalid
application {
    apis: {
        admin: {
            prefix: /admin
            serve_openapi: "no"
        }
    }
}
```

```text output
error: `"no"` isn't `true` or `false`
 --> app.jadpo:5:28
  |
5 |             serve_openapi: "no"
  |                            ^^^^ not a `Bool`
  reason: `serve_openapi:` says whether the API serves its OpenAPI document at `<prefix>/openapi.json`: `false` turns it off, and `true`, the default, can be left out.
  next: Write `serve_openapi: false`, or leave `serve_openapi:` out (agent)
  rule: api.serve_openapi_invalid
  help: docs/language/07-http.html#api-serve-openapi-invalid

check failed: 1 error, 1 file
```

### `api.setting_unknown`

```jadpo check project=e-api-setting file=app.jadpo expect=api.setting_unknown
application {
    apis: {
        main: {
            prefix: /v1
            version: 2
        }
    }
}
```

```text output
error: `version` isn't an API setting
 --> app.jadpo:5:13
  |
5 |             version: 2
  |             ^^^^^^^ unknown setting
  reason: An API in `apis:` holds its `prefix:`, `idempotency:`, `authentication:` and `serve_openapi:`, and later its `rate_limit:`.
  next: Remove `version` (agent)
  rule: api.setting_unknown
  help: docs/language/07-http.html#api-setting-unknown

check failed: 1 error, 1 file
```

## Review

### Status

Built in chunks 4.1 and 4.2. `jadpo check` checks route groups, their
prefixes and APIs (with `idempotency:`), methods and typed placeholders,
the one input (from the query string for `GET` and `DELETE`, the JSON body
otherwise, and `header:` fields), `output:` and `status:`, `deadline:`'s
range and what `run:` calls, every route of the project against the others
and the health paths, and each API's record; it writes one OpenAPI document
per API to `build/openapi/`. The runtime's HTTP component routes a request,
decodes and checks its input (each problem as `{ field, rule }`), runs the
action or query, and answers with the output or the failure envelope, its
`request_id` and `x-request-id`, with the status the failure's kind sets; it
answers `HEAD` and `OPTIONS` itself and counts the default rate limits. It
refuses a body over 1 MB, stops a route at its deadline without committing,
answers `/health/live` and `/health/ready`, and keeps each write's
`Idempotency-Key` with its response. `jadpo dev` serves it and reloads on
save, `jadpo routes` lists the routes and `jadpo api update` writes the
records. Since chunk 7.0 (area 18) each record is `api/<api>.jadpo_record`
in the record format, with no `version:` line, and the OpenAPI version is
a hash of it. Since chunk 7.2 each API serves its own document at
`<prefix>/openapi.json` (`serve_openapi: false` turns it off), listed in
its record, its document and `jadpo routes`, and the documents carry the
`x-jadpo` fields area 11's importer reads ("OpenAPI").

Since chunk 4.3 (area 8) every route needs a signed-in caller unless it
says `authentication: RouteAuthentication.none`, each API lists the sign-in
methods it accepts, a route passes `current_principal` to what it runs, a
signed-in caller's requests count against its own rate limit, and its
idempotency keys are its own. Since chunk 4.4 (area 9) policy decides what
each route reaches, and OpenAPI lists the 403 `not_permitted` on every
route that runs an action that writes or is listed under `actions:`.
Since chunk 4.4c (area 8) a `GET` runs `sessions.tokens` and
`<Service>.keys` itself, and the record and OpenAPI describe the
`IssuedToken` and `IssuedKey` it sends.

Since chunk 4.9 (the Phase 4 review) the runtime reads a body only as
JSON, refuses a query string or body a route doesn't read and a header it
can't read, stops work that never waits at its deadline, runs an action
again on a concurrent change found as it commits, and counts a request
with a credential on a route anyone may call against its address before
it runs; OpenAPI derives each route's responses from what it runs and
shows `ZonedTime`, `Zone` and a mixed enum as they are sent, and the record names each
API's sign-in methods. Natively, a client has 10 seconds for its headers
and 30 for its body, and the server holds at most 1,024 connections and 10
database connections per store per worker.

Since chunk 5.5u7 (area 20) a route may send a `FeatureCollection<G, P>`
bare, its properties held to what an `Output` may hold and, where an
action returns it, to the fields every caller may read (area 9); OpenAPI
describes RFC 7946's object and the record names it with its type
arguments. Since chunk 5.5u8 a `GET` sends one a query gives (area 6, "A
query giving a map layer"), held to field policy where the query is
declared.

Since chunk 6.3 a route waits for the work its action set off with
`settle:` (area 12): the runtime delivers its events and their
follow-ons while the request waits, then answers with `answer:`'s query
and the route's status, or 202 past `within:` or while a delivery waits
to retry; OpenAPI and the record show both answers, and a repeat with
the same `Idempotency-Key` runs the query again ("Waiting for follow-on
work", and `route.settle_invalid`).

Since chunk 5.5v6 a route whose action returns an `AiStream` sends a
model's answer as server-sent events as it is written: `piece` events,
then `done`, or an `error` event for a failure after the first piece;
OpenAPI lists it as `text/event-stream` with `x-jadpo-stream`, the record
as `output AiStream (server-sent events)`, and a route test reads its
`pieces` ("Streaming a model's answer", and `route.stream_invalid`).

Not yet: `x-jadpo-events`, which waits for publishing events outward
(area 12); a route's own `rate_limit:` and the Redis
store for rate limits (area 16); classifying breaking changes and their
acceptances (after the first build, area 18); the `Jadpo-Revision` header,
which comes with caches (area 6). Each is `toolchain.not_built_yet` where it
is written.

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
| Streaming responses that fail after headers are sent (ROUTE-002, from area 4) | 7 | Decided and built in chunk 5.5v6: an `error` event ("Streaming a model's answer") |
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
    a committed readable record per API (`api/main.api`, now
    `api/main.jadpo_record`) that `jadpo check`
    compares, as Kotlin's API validator and Microsoft's API Extractor do;
    changes that can break a client need an `acceptance` file written by a
    person, one per change and used once, in the same form for database
    drops, policy weakening and new domains; the `acceptances/` folder is
    owned in CODEOWNERS so the host's review records who approved it. *Decided 2026-10-12:* yes, as recommended.
    *Revised 2026-10-16 (Richard, area 18):* the record is named
    `api/main.jadpo_record`, like every committed generated file.
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
    *Extended 2026-10-16 (Richard, area 11):* so does each API's served
    `GET <prefix>/openapi.json`, which also appears in the API record.
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
| What `run:` calls | A `GET` calls a named query, every other method a named action; no inline route actions (amended in chunk 4.4c: a `GET` also runs `sessions.tokens` and `<Service>.keys`, area 8) | 2026-10-12 |
| Deadline and body limit | 10-second default deadline, `deadline:` up to 60 seconds; 1 MB body limit answering 413 | 2026-10-12 |
| Health | Generated only: `/health/live` and `/health/ready`, readiness from the declared stores | 2026-10-12 |
| APIs and versions | `apis: { }` in `application`, `main` at `/` implied; a group names its API with `api:`; one OpenAPI document and record per API; a new major version is another API | 2026-10-12 |
| Webhooks | Ordinary routes in a `webhooks` API; the provider's signature check as authentication; a unique rule on the provider's event id; work in a subscriber (Amended 2026-10-13 by area 11: provider webhooks are generated from the package and arrive as events; no route is written) | 2026-10-12 |
| Field path format | The `Input` field's dotted path with list positions as numbers (`lines.2.quantity`) | 2026-10-12 |
| API record and acceptance | Committed readable record per API (`api/main.api`) compared by `jadpo check`; breaking changes need a person-written acceptance file, one per change, the same form for drops, policy weakening and new domains; `acceptances/` owned in CODEOWNERS (Revised 2026-10-16: `api/main.jadpo_record`, area 18) | 2026-10-12 |
| Write revision header | `Jadpo-Revision` header on every write, a cookie for browsers; sending it back skips caches | 2026-10-12 |
| Generated authentication routes in OpenAPI | Listed like any route, marked generated; every route shows its security scheme | 2026-10-12 |
| Accepted authentication methods per API | `authentication: [...]` in each API's entry in `apis: { }`; groups and routes may only narrow it | 2026-10-12 |
| Route URL shown | Hover shows a route's full method and path; `jadpo routes` lists every route with path, file, API, authentication and what it runs (Richard) | 2026-10-12 |
| Route test request and `Response` | Owned by area 13; this page links to it | 2026-10-12 |
| Route declaration form | `GET /path { }` entries inside a `routes` group, keyed by method and path; the one exception to the colon rule | 2026-10-12 |
| Idempotent writes | Every non-`GET` route honours `Idempotency-Key`, handled entirely by the runtime: stored with the response in the action's transaction for 24 hours, scoped to the caller; reuse with a different request is 422, in flight is 409; `unavailable` and internal failures not stored; `idempotency: Idempotency.required` per API | 2026-10-12 |
| Expired idempotency keys | A lookup ignores a key older than 24 hours; the runtime deletes expired keys in batches from its own minute tick (area 12), so the table holds about a day of keys (Richard, from an outside review) | 2026-10-15 |
| Route readings (chosen in chunk 4.1) | `output:` may also be a `List` or `Page` of an `Output`, as a query returns them; a placeholder or `input` a route never passes to what it runs is an error (`route.binding_unused`); a header field may not read `Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie` or `X-Jadpo-*`; besides a type's own rules, input problems name `required`, `unknown_field`, `type` (the wrong JSON kind), `variant` and `duplicate`, and text that isn't a UUID, an instant or a date breaks `format`; an `Int` is a JSON number only when its declared range has both ends within JavaScript's safe integers, a string otherwise, as area 2 decides, and a number is accepted for it either way | 2026-10-15 |
| Runtime answers (chosen in chunk 4.1) | Unmatched paths answer 404 `route_not_found`, a known path with another method 405 `method_not_allowed` with `Allow`; defects answer 500 `internal_fault` with "The server failed to answer the request."; a declared failure without a `message:` gets its kind's message; `request_id` is `req_` and 16 hexadecimal digits, on every response as `x-request-id` | 2026-10-15 |
| OpenAPI and the record (chosen in chunk 4.1) | The record is the wire contract: routes in path order, then each `input`, `output`, object and enum by name with each field's wire type and rules, then each failure code with its status, message and client fields; `version:` rises by one when `jadpo api update` changes it and starts at 1. An OpenAPI document's title is the project folder's name until `app.jadpo` names the application, and its version the record's (0 before one exists). A record out of date fails `jadpo check` but doesn't stop `jadpo test` or `jadpo dev` | 2026-10-15 |
| Default rate limits until sign-in (chosen in chunk 4.1) | (Amended in chunk 4.3: a signed-in caller's requests count against its own allowance, 600 a minute, and the rest against their address; area 8's Decisions.) With no one signed in yet, every request counts against its address (the client's, as area 10's `client_address:` chooses it: the connection's own unless the application trusts a proxy's header; chunk 5.1c): 120 a minute, in memory per process within 16 MB, the oldest counters dropped first; the native host sends each address's connections to one worker thread, so no lock is taken. The `jadpo check` warning about per-instance limits comes with the Redis store, since until then nothing could silence it | 2026-10-15 |
| Request guarantees (chosen in chunk 4.2) | A body over 1 MB answers 413 before routing; the host stops holding it there, reading and dropping up to 16 MB more so the client reads the answer. A route's deadline counts from when it is matched: when it passes, the running work is dropped with its open transaction, which the store rolls back, and a claimed key is given up; an action's commit runs after its body, outside the deadline, so a request that committed never answers 504, and its message is "The request ran out of time, and nothing it did was kept." A `GET` never takes part, whatever headers it sends | 2026-10-15 |
| Health (chosen in chunk 4.2) | `/health/live` answers 200 `{"status":"live"}`; `/health/ready` runs `SELECT 1` on each PostgreSQL store an entity lives in, within two seconds in all, and answers 200 `{"status":"ready","stores":{"main":"ready"},"integrations":{}}`, or 503 with `not_ready` and each store that didn't answer as `unavailable`; integrations appear by name as `unchecked` once area 11 builds them. Both answer `HEAD` and `OPTIONS`, any other method 405, with `Cache-Control: no-store`; they are answered before the rate limits, so probes are never refused; a route on either path is `route.path_overlap`. Each worker (natively a worker thread, on Workers an isolate) reuses its last readiness answer for a second, so probes cost a store at most one `SELECT 1` a second per worker (amended in chunk 4.10) | 2026-10-15 |
| Idempotency keys (chosen in chunk 4.2) | `Idempotency.optional` (the default) and `Idempotency.required`, which an API's record shows as `idempotency: required` and OpenAPI as a required header on every write; any other value is `api.idempotency_invalid`. A key is 1 to 255 printable ASCII characters, else 400 `invalid_request`, as is a missing one where it is required. Until sign-in (4.3) every caller is the scope `anonymous` (amended in chunk 4.3: a signed-in caller's keys are kept under `user <id>`, and a route anyone may call keeps `anonymous`). The request's digest covers its method, path, query string, the headers its input reads and its body, byte for byte (superseded in chunk 5.8a: its route and what it says decoded, every secret value left out). A key is claimed in one statement of its own before the action runs, with no response and kept only for the route's deadline and five seconds, so a request that never finished leaves a claim that lapses; the response (status, the body exactly as sent and its code) is written over it in the action's transaction, or in one statement of its own for a failure the action returned, kept 24 hours from the request. A repeat answers with the stored response under its own `x-request-id`. Keys go in the store the action writes; an action that writes no store keeps none, since running it again changes nothing, until area 12's `events: { store: }` names one (built in chunk 6.1b: an action that writes no store but emits keeps its keys beside its events, in the store `events: { store: }` names; one that does neither still keeps none. Amended in chunk 7.9a: one that makes a changing call keeps them there too, beside the record of its steps, and OpenAPI says so; with no store at all its calls' keys are still derived from the client's key, never the request's id) | 2026-10-15 |
| `jadpo dev` (chosen in chunk 4.1) | Listens on 127.0.0.1, port 3000 unless `--port`; the database is the one `--url` names (until configuration, area 10) or a disposable PostgreSQL for the session, and pending migrations are applied when it starts and after each save that adds one | 2026-10-15 |
| Waiting for follow-on work | `settle: { answer: <query>(result...) within: <duration> }`: after commit the runtime delivers the action's events and their follow-ons at once and waits until each has succeeded, failed for good or been parked, then answers with the query and the route's status; past `within:` the same query answers 202; `output:` is the query's result; a repeat with the same `Idempotency-Key` reruns the query (Richard, from an outside review) | 2026-10-15 |
| Route authentication (chosen in chunk 4.3) | A route's `authentication:` takes `RouteAuthentication.none` or `.fresh`; anything else is `route.authentication_invalid`, which offers both without an edit, since one opens the route. An API's `authentication:` is a list of `AuthenticationMethod`s `authentication.jadpo` declares, each once, else `api.authentication_invalid`; an API that names none accepts every declared method. Area 8's Decisions hold the rest | 2026-10-16 |
| Failure wording for clients | Every failure a route can produce is worded in every supported locale in the app's language files; `jadpo build` writes `<api>.failures.<locale>.json` beside each OpenAPI document; dates and numbers stay values for the client to format; `message` stays English (Richard, area 10) | 2026-10-16 |
| Generated routes | Endpoints only, never pages; `path:` where the feature is configured or `generate_routes: false` over generated operations that keep every protection; webhook routes change path but cannot be switched off; response shapes fixed (Richard) | 2026-10-16 |
| Rate-limited response | `429`, code `rate_limited`, `Retry-After` and the IETF `RateLimit` and `RateLimit-Policy` headers, wording from translations (Richard) | 2026-10-16 |
| Serving OpenAPI | Every API serves its OpenAPI document at `<prefix>/openapi.json` under its own default authentication, `serve_openapi: false` to turn it off; the route is listed, marked generated, in its own OpenAPI document, its API record and `jadpo routes` (Richard, area 11) | 2026-10-16 |
| `x-jadpo` fields | Jadpo's OpenAPI documents carry `x-jadpo-changes`, `x-jadpo-idempotency`, `x-jadpo-failure` (`code`, `kind`), `x-jadpo-constraints` and `x-jadpo-events`; other tools ignore them; area 11's importer reads them to make an exact package (Richard, area 11) | 2026-10-16 |
| Serving OpenAPI and its fields (chosen in chunk 7.2) | The served route needs a signed-in caller by any method its API accepts, unless the API accepts no built method, when anyone may read it; it answers with `build/openapi/<api>.json`'s bytes and refuses input; the record lists it as `generated openapi`. `x-jadpo-changes` is `true` for a route that runs an action; `x-jadpo-idempotency` gives `header`, `required`, `keeps`, `window`, `max_length`, `in_use` and `reused`; `x-jadpo-failure` is on each declared failure's schema; `x-jadpo-constraints` is a field's type as the record writes it; each operation is tagged with its group. `x-jadpo-events` waits for publishing events outward. The document's title is the project folder's name, held by the project itself so the plan can serve it | 2026-10-20 |
| API record name | `api/<api>.jadpo_record`, replacing `api/main.api`: readable, starting with the line that says what wrote it, a hand edit an error (Richard, area 18) | 2026-10-16 |
| Webhook senders without a package | A signing system with no package gets the app's own integration in `shared/providers/`; a partner with an agreement calls an ordinary route as a `Service` with a service key (Richard, area 11) | 2026-10-16 |
| Request reading (chosen in chunk 4.9) | A body is read only with a JSON `Content-Type` (`application/json` or `application/…+json`, any parameters, any case), else 400 `invalid_request` "A request body is JSON, sent with `Content-Type: application/json`."; a header value that isn't printable ASCII (spaces and tabs allowed) is 400 `invalid_request` "A header's value isn't printable ASCII.", whichever header it is, so a credential or key is never dropped; a route that reads no query string answers each key it gets as `{ field: <key>, rule: unknown_field }`, and one that reads no body answers a body that isn't blank as `{ field: "", rule: unknown_field }`, 422 `invalid_input`, before anything runs; a page's cursor that isn't one of its query's is 422 `invalid_input` with `{ field: <its path in the input>, rule: format }` | 2026-10-16 |
| Deadlines on work that never waits (chosen in chunk 4.9) | The runtime reads the host's monotonic clock against the route's deadline: the interpreter checks it every 4,096 steps and stops the body there, and any body that finishes past the deadline, stopped or not, is answered 504 `deadline_exceeded` with nothing committed and its claimed key given up. Each of those checks also gives the worker thread back, so the other requests it serves carry on while such a body computes (amended in chunk 4.10) | 2026-10-16 |
| A concurrent change at commit (chosen in chunk 4.9) | A change another transaction made, found as the action commits, runs the whole action again, as area 6 says, within the same three attempts and the route's deadline, then 503 `unavailable`; a claim that lapsed and was taken before the commit is `unavailable` too, with nothing kept | 2026-10-16 |
| OpenAPI responses (chosen in chunk 4.9) | Every route lists 400, 413, 422, 429, 500 and 504, since any request can be unreadable, too large or carry unknown input; 503 `unavailable` where what it runs uses a store; 403 where policy can refuse its action or one it calls; a write's 409 and 422 for a key only where a key is claimed, and its `Idempotency-Key` described by what is kept: the response, nothing for a response that starts or ends a session or carries a new credential, or nothing at all for an action that writes no store. A `Time` is a string `09:30` (seconds and milliseconds when it has them), and a mixed enum's plain variant is its name as a string constant. *Superseded in chunk 5.2 (area 10):* there is no time of day; a `ZonedTime` is an object of its `instant`, a `date-time` string, and its `zone`, an IANA name, both required and nothing else; a `Zone` is sent as its IANA name (`Europe/London`) and a `Locale` as its BCP 47 tag (`en-GB`), and OpenAPI lists them so; a `Duration` is ISO 8601 in JSON, query strings, paths and headers alike | 2026-10-16 |
| Sign-in methods in the record (chosen in chunk 4.9) | Each API's record starts with `authentication:` and the methods it accepts, its own list or every declared one, when `authentication.jadpo` declares any; a route that selects one kind of caller says `caller: user` or `caller: service` | 2026-10-16 |
| Native request limits (chosen in chunk 4.9) | A client has 10 seconds to send a request's headers and 30 its body, else the connection is closed without an answer; the server holds at most 1,024 connections, closing one past that as it is accepted; each worker opens at most 10 connections per store, a transaction past that waiting for one, inside the route's deadline. Configuration may set them once area 10 does | 2026-10-16 |
| `Idempotency-Key` as input (chosen in chunk 4.9) | An `Input` field may not read `Idempotency-Key`, which the runtime reads before anything runs: `route.header_credential`, "`Idempotency-Key` belongs to the runtime" | 2026-10-16 |
| A map layer as output (Richard, 2026-10-19) | Besides an `Output`, or a `List` or `Page` of one, a route may send a `FeatureCollection<G, P>` (area 20) bare, because mapping libraries fetch a layer as RFC 7946 GeoJSON; its properties type `P` is held to what an `Output` may hold, so only fields declared for clients leave the server. `route.output_not_output`'s reason names it. Built in chunk 5.5u7 | 2026-10-19 |
| Streaming a model's answer (chunk 5.5v6) | A route whose action returns an `AiStream` (area 21) names `output: AiStream` and sends it as server-sent events, `text/event-stream` with `Cache-Control: no-store`, once the action has committed and the first piece has come: each `piece` event `{"text": ...}` as it is read, then `done` (`{}`), or `error` with the failure envelope, `unavailable` (`internal_fault` for a defect), for anything that stops it part-way; one `event:` and one `data:` line of compact JSON per event; no `id:`, no `retry:`, `Last-Event-ID` not read, so a stream can't be resumed; a failure before the first piece answers as any route's does. The stream ends by the call's `timeout:` and the route's deadline, whichever is first, and a client that goes away closes the provider's connection. Only `SuccessStatus.ok` and no `settle:` (`route.stream_invalid`); the response is never kept for an `Idempotency-Key` repeat; OpenAPI's 200 is `text/event-stream` with each event's data in `x-jadpo-stream`; the record says `output AiStream (server-sent events)`; a route test's `Response` has `pieces` in place of `output`. The standard shape, chosen conservatively, since area 21 (question 6) chose server-sent events | 2026-10-20 |
