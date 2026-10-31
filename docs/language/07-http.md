---
title: HTTP
area: 7
status: decided
checked-with: this checkout's jadpo on every gate run
---

# HTTP
<!-- topic: http -->

A Jadpo app answers HTTP requests with routes. A route says which method
and URL it answers, what comes in, what goes out, and which action or query
runs:

```jadpo fragment
// domains/planning/routes/todos.jadpo
routes Todos {
    prefix: /todos

    POST / {
        input: CreateTodo
        output: TodoView
        status: SuccessStatus.created
        run: create_todo(input, current_principal.user)
    }
}
```

This route answers `POST /todos`. Jadpo reads a `CreateTodo` from the
JSON body and checks it, and `run:` gets it as `input`. `run:` calls the
action `create_todo` for the signed-in user (`current_principal.user`, see
[Who is calling](08-identity.md#who-is-calling-current_principal)). The
action returns a `TodoView`, which must be the route's `output:`, and
`status: SuccessStatus.created` makes the route answer 201 Created rather
than 200.

A route holds no logic of its own. The work lives in
[actions and queries](03-logic.md#functions-and-actions). When the action
fails, the HTTP status comes from the failure's kind, a category such as
`not_found` (404), never from the route ([Kinds](04-failures.md#kinds)).

Every route needs a signed-in caller unless an `authentication:` line in
the route says otherwise
([Routes: signed in by default](08-identity.md#routes-signed-in-by-default)).

The topics of this page cover:

- how routes are grouped and matched: [Route groups](#route-groups) and
  [Paths and placeholders](#paths-and-placeholders);
- what a route reads and answers: [One input](#one-input) and
  [Output and status](#output-and-status);
- how long it may run: [Time limits](#time-limits);
- what Jadpo writes for you: [Health](#health) checks,
  [Generated routes](#generated-routes), an [OpenAPI](#openapi) document
  for each API, and [The public API record](#the-public-api-record), a
  file in your project that lists each API's routes and fields, so every
  change to the API shows as a diff of that file.

## Route groups
<!-- topic: route-groups -->

Every route belongs to a route group: a `routes` declaration with a name
and a `prefix:`. Each group has a file of its own in its domain's `routes/`
folder.

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

The prefix comes before every route's own path, so
`GET /{todo_id: Todo.id}` here answers `GET /todos/{todo_id}`.

The file is named after the prefix's fixed segments, joined with `_`:
`/todos` is `todos.jadpo`, and `/admin/orders` is `admin_orders.jadpo`.

A prefix may hold typed placeholders, which every route in the group can
use: `prefix: /projects/{project_id: Project.id}/todos` lives in
`projects_todos.jadpo`. Two prefixes may start the same way, but the
compiler checks that every URL matches exactly one group, so a URL from a
log leads to one file.

A route is written `GET /path { }`, keyed by its method and path, which is
how HTTP, OpenAPI and logs name a route. Everywhere else, a declaration's
items are written `name: value`; a route entry is the one exception.

Routes are never `public`, the word that lets another domain use a
declaration. Who may call a route is decided by sign-in
([Routes: signed in by default](08-identity.md#routes-signed-in-by-default))
and by [policy](09-policy.md).

To see a route's full method and path, hover over it in the editor.
`jadpo routes` lists every route with its full path, file, API,
authentication and the query or action it runs.

## Paths and placeholders
<!-- topic: paths-and-placeholders -->

A placeholder in a path is typed where it appears, and reaches what the
route runs by its name:

```jadpo fragment
PATCH /{todo_id: Todo.id} {
    input: PatchTodo
    output: TodoView
    run: patch_todo(todo_id, input, current_principal.user)
}
```

Here `{todo_id: Todo.id}` reads one segment of the path as a todo's id and
passes it to `patch_todo` as `todo_id`.

- A placeholder's type is a [named type](02-values-and-types.md#named-types)
  over a scalar, or an entity's id (`X.id`). It is never optional.
- The segment is decoded once and checked. A bad value is a 422, like any
  bad input.
- The methods are `GET`, `POST`, `PUT`, `PATCH` and `DELETE`. The runtime
  answers `HEAD` and `OPTIONS` itself.
- Two routes that could match the same request, such as
  `/{todo_id: Todo.id}` and `/{slug: Slug}`, are an error. A fixed segment
  wins over a placeholder, so `GET /archived` can sit beside
  `GET /{todo_id: Todo.id}`.

## One input
<!-- topic: one-input -->

A route reads at most one input, named by `input:`. It is an `Input` type,
an object a client sends ([Values and types](02-values-and-types.md)).
Where the input comes from follows the method: the query string for `GET`
and `DELETE`, and the JSON body for `POST`, `PUT` and `PATCH`.

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

`GET /?status=open` fills `status` from the query string, and
`request_tag` from the `X-Request-Tag` header when it is sent. `limit` is
25 when the request leaves it out.

### The query string

Query-string fields are scalars, enums or lists of them. A list is the key
repeated: `?status=open&status=done`. A nested object needs a body.

### Headers

A field read from a header says so with `header:`. A header field is a
scalar, and each header is named once. A header field never reads a
credential (`Authorization`, cookies, `X-Jadpo-*`): those belong to
sign-in ([Identity](08-identity.md)).

### What is refused

- Unknown keys and fields are refused. A query string or a body that the
  route doesn't read counts as unknown input too.
- A `GET` or `DELETE` never reads a body.
- A body is at most 1 MB, and no setting changes that. A larger one
  answers 413.
- A body is read only when it is sent as JSON (`Content-Type:
  application/json`, or another `application/…+json` type). So a form a
  browser posts from another site never reaches an action.

### Text is normalised

Text in a request is normalised to Unicode NFC as it is decoded, in the
body, the path, the query string and headers alike, before the input's
rules are checked. NFC writes each character one standard way, so equal
text compares equal however the client spelled it
([Standard library](19-standard-library.md)).

## Output and status
<!-- topic: output-and-status -->

`output:` names what the route answers with: an `Output` type, an object
made to be sent to clients. It must be exactly what the action returns, so
the route shows its contract without anyone opening the action.

A route that succeeds answers 200, unless `status:` says otherwise:

```jadpo fragment
POST / {
    input: CreateTodo
    output: TodoView
    status: SuccessStatus.created
    run: create_todo(input, current_principal.user)
}
```

| `status:` | HTTP | Needs |
|---|---|---|
| left out (`SuccessStatus.ok`) | 200 | `output:` |
| `SuccessStatus.created` | 201 | `output:` |
| `SuccessStatus.accepted` | 202 | `output:`; the work carries on in the background ([Jobs and events](12-jobs-and-events.md)) |
| `SuccessStatus.no_content` | 204 | no `output:`; the action returns `Unit` |

Only the output's declared fields are sent. `IssuedToken` and `IssuedKey`,
the built-in types for an issued sign-in token and a service's key
([Identity](08-identity.md)), are sent as the objects they are: an `id`,
`started_at` and `ends_at`.

A route never writes a failure's status. It comes from the kind of each
failure the action can produce ([Failures](04-failures.md)).

### Map layers

A route may also answer with a map layer: a `FeatureCollection<G, P>`, a
collection of shapes of geometry `G`, each with properties of type `P`
([Geospatial](20-geospatial.md)). It is sent bare, as the whole response,
in plain RFC 7946 GeoJSON, so a mapping library fetches it as its source.
A `GET` gets one from a query ([Reads and writes](06-reads-and-writes.md),
"A query giving a map layer"); a route with another method gets one from
an action that builds it.

- **Only a collection, only bare.** A `List` or `Page` of layers, or a lone
  `Feature<G, P>`, is `route.output_not_output`. A mapping library takes a
  collection as its source, and nothing asks for the others.
- **What `P` may not hold** is what an `Output`'s fields may not: a caller,
  a password's hash or a secret value, however deep. `P` is an `Object`,
  which could hold any of them, so the route checks it: a layer whose
  properties hold one is `route.output_not_output` at `output:`, naming
  what and where. The same search looks inside a feature's properties
  wherever a layer appears, so an `Output` with a layer field whose
  properties hold a password's hash is refused at that field.
- **Field policy.** A layer an action returns is shown to its caller, just
  as an `Output` the action builds is. So every `return` of a
  `FeatureCollection` in an action carries only fields every caller of the
  action may read. Otherwise it is `policy.output_field_unproved` at the
  value returned ([Field restrictions](09-policy.md#field-restrictions)),
  whichever route or action calls it.
- **Sent as GeoJSON.** The response's body is the collection as
  `json.write` writes it, `{"type":"FeatureCollection","features":[...]}`.
  A route test reads it back through the same codec, so `response.output`
  is the layer the action returned.

## What a route runs
<!-- topic: what-a-route-runs -->

`run:` calls one named action. It passes the placeholders, `input` and the
caller as ordinary arguments. A `GET` calls a named query instead, so it
can never write, as HTTP promises of a `GET`, and a read needs no action
wrapped around its query:

```jadpo fragment
GET /{todo_id: Todo.id} {
    output: TodoView
    run: todo_view(todo_id)
}

POST /{todo_id: Todo.id}/complete {
    output: TodoView
    run: complete_todo(todo_id)
}
```

The query `todo_view` lives in its domain's `queries/` folder, and the
action `complete_todo` in `actions/`. Your own domain's actions and queries
are called by their names. Another domain's `public` ones are called
through that domain's name, as in `billing.charge(order)`
([Project layout and modules](01-project-layout-and-modules.md)). A route
can't hold an action of its own written inline.

### Listing tokens and keys

A `GET` can also run the listings Jadpo generates for sign-in credentials.
`sessions.tokens(user_id)` lists the tokens a user holds, and
`<Service>.keys(service_id)` the keys a service holds
([Service credentials](08-identity.md#service-credentials)). They read the
tokens and keys the runtime keeps, so a route lists them without an action
that copies each one into an `Output`.

The route passes the holder's `id`, from a placeholder or from
`current_principal`. Its `output:` is the `List<IssuedToken>` or
`List<IssuedKey>` the listing returns, exactly as for a named query:

```jadpo fragment
GET /devices {
    output: List<IssuedToken>
    run: sessions.tokens(current_principal.user.id)
}
```

## Time limits
<!-- topic: time-limits -->

Every route has a deadline: 10 seconds, unless `deadline:` sets another,
and never more than 60. A request that hangs holds a database connection,
so no route may run for long.

```jadpo fragment
POST /import {
    input: ImportTodos
    output: ImportResult
    deadline: 30s
    run: import_todos(input)
}
```

When the deadline passes, the runtime starts no new work, commits nothing
and answers 504 `deadline_exceeded`.

That holds for work that never waits too, such as a long calculation. The
runtime checks the deadline as the work runs, and a body that finishes past
it commits nothing. Compiled code checks it as the interpreter does: every
4,096 calls, loop turns and inline-function calls, in a function's loops
and inside an operation's inline function as in an action's. Each time an
action's loop checks, it gives the thread back to the other requests
beside it.

The deadline also covers calls to outside services, which have their own
shorter timeouts. Each call's `timeout:` times its attempts, summed along
the longest way through the route's action, must fit within the deadline,
or it is `route.calls_past_deadline`
([Calling](11-integrations.md#calling)).

Longer work answers `status: SuccessStatus.accepted` and carries on in a
subscriber, code that runs when an event is delivered
([Subscribers](12-jobs-and-events.md#subscribers)). Or the route waits for
it with `settle:` ([Waiting for follow-on work](#waiting-for-follow-on-work)).

## Waiting for follow-on work
<!-- topic: waiting-for-follow-on-work -->

An action does its own database work and leaves the rest, such as a second
call to a provider, to subscribers: code that runs when an event the action
emits is delivered ([Subscribers](12-jobs-and-events.md#subscribers)).
Without anything more, the route answers once the action commits, and that
work carries on afterwards. When the client wants the finished result
rather than a `202`, the route waits for the work with `settle:`:

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

This route places the order, then waits up to 5 seconds for the work it
set off. It answers with `order_view(result.id)`: 201 if the work finished,
202 if the 5 seconds ran out first. The body is an `OrderView` either way.

### What the route waits for

After the action commits, the runtime delivers the events it emitted at
once, while the request waits, and then the events their subscribers emit
in turn. They run in the request's own invocation, each delivery as any is
delivered
([Delivery: at least once, safe to repeat](12-jobs-and-events.md#delivery-at-least-once-safe-to-repeat)),
as the system limited to its event's tenant
([Background work](09-policy.md#background-work)). The request's caller is
given back before the answer runs.

The work has settled when every one of those deliveries has succeeded,
failed for good, or been parked as `outcome_unknown`. That covers a failure
that isn't retried, a defect (a bug in the code, such as an `Int`
overflow), a subscriber the build no longer has, and an event 16 deep in a
chain of events (`event_chain_limit`), so a loop of events settles too.

Some work can't be waited for:

- A delivery left to retry after a pause (`unavailable`, `rate_limited`,
  or a store that couldn't be reached) hasn't settled, so the route stops
  waiting at once and answers 202.
- So does a delivery the request couldn't claim.
- So does a batch run the action started. A batch is long work no route
  waits on, so a route whose action starts one answers 202
  ([Batches: work over many items](12-jobs-and-events.md#batches-work-over-many-items)).

Waiting stops at the first such piece. The rest is left for after the
response, and a retry is left to the outbox's sweep, the runtime's regular
pass over deliveries that are due
([Retries and giving up](12-jobs-and-events.md#retries-and-giving-up)).

### The answer

Then the route runs `answer:`. It is a named query, given `result` (what
the action returned) or fields read through it, and nothing else; the
query reads who is asking through its policy. It runs as the request's
caller and under the query's policy, as a `GET` running it would. The route
answers with its result and the route's `status:`.

If `within:` passes first, the route runs the same query and answers
`202`. So the client gets the same `output:` either way, and knows from the
status whether the work finished. The work carries on in the background
either way, exactly as it would without `settle:`.

The query's failures are the route's too, beside the action's, in OpenAPI
and the record. A query that fails, through its `missing:` say, answers
with that failure, though the action committed.

### Rules

- With `settle:`, `output:` is the query's result type, not the action's.
- `within:` counts from when the request came in, as `deadline:` does. It
  must be shorter than the deadline, leaving the answer time to run, or it
  is `route.settle_invalid`. The answer isn't held to the deadline, because
  the action has already committed, so waiting never ends in a 504.
- When `within:` passes, the subscriber's action that is running is
  dropped and nothing it did is kept. Its delivery goes back to `pending`,
  due at once, and that attempt isn't counted. Only its body is cut short,
  never its commit, and the delivery's own statements are never cut short,
  so a claim is never left in doubt.
- `settle:` goes only on a route that runs an action and answers
  `SuccessStatus.ok` or `.created`, or it is `route.settle_invalid`. A
  `GET` sets off nothing to wait for, `.accepted` would make both answers
  202, and `.no_content` sends no answer.
- Only work this action set off is waited for, never other requests' work
  or published copies of events
  ([Where events travel](12-jobs-and-events.md#where-events-travel)).

### Repeats, OpenAPI and tests

- A repeat with the same `Idempotency-Key`
  ([Idempotent writes](#idempotent-writes)) runs the answer query again,
  rather than replaying a stored body, so it shows how far the work has
  got. The key keeps the query's arguments, each written by its
  parameter's codec. A repeat answers 202 until the work is known to have
  settled, and the route's status after that. So a process that stops
  between the commit and the answer never has a repeat say the work
  finished.
- OpenAPI lists the route's status, "once the work it set off has
  settled", and 202, each sending the query's result, and says a repeat
  runs the answer again. The API record adds a `settle within <duration>`
  line to the route.
- A route test's request waits in the same way. A test host's timers never
  go off, so `within:` never passes in a test, and a follow-on left to
  retry gives 202.

## Streaming a model's answer
<!-- topic: streaming-a-model-s-answer -->

When what a route runs returns an `AiStream`, the route sends a model's
answer as it is written, piece by piece. An `AiStream` is the text
`ai.stream` asks a model for ([Streaming](21-ai.md#streaming)). It is sent
as server-sent events, the HTML standard's `text/event-stream`, which is
the shape OpenAI's and Anthropic's APIs stream in. Jadpo has no WebSockets
([AI](21-ai.md)).

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

The action asks for the stream. As with any outside call, it maps each way
the call can fail before its first piece to one of its own failures:

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

The route names `output: AiStream`, exactly what the action returns:

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

A stream that stopped part-way ends like this:

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
| `error` | The failure envelope ([Failures](04-failures.md)), with `unavailable`, or `internal_fault` for a defect | The answer stopped before it was whole; the last event |

### Before and after the first piece

- **Before the first piece** nothing has been sent, so the route answers
  as any other does. A failure the action's `attempt` mapped from
  `ai.stream`'s `refused`, `timed_out`, `over_budget` or `too_long`
  answers with its envelope and its kind's status, and the deadline
  passing answers 504. The action has committed by the time the first
  piece goes out.
- **After it**, the 200 has been sent, so whatever stops the answer
  part-way ends the stream with an `error` event, `unavailable`. That
  covers the model declining to go on, a cut-off at `max_output:`, the
  call's `timeout:` or the route's deadline passing, and the provider's
  connection breaking or sending something that isn't a chat completion.
  The app's mapping can't run then, and the client only needs to know the
  answer is incomplete, so the code and its wording are the runtime's own.
- **Every stream ends** with exactly one `done` or `error`, and then the
  response ends.

### The events

- **Each event** is an `event:` line, one `data:` line of compact JSON and
  a blank line, in UTF-8. The JSON escapes the text's newlines, so they
  never split an event. A piece is what the provider sent, never cut or
  joined, and empty ones are left out.
- **No resuming.** A model's answer can't be asked for again from where it
  stopped, so events carry no `id:`, the server sends no `retry:` and
  `Last-Event-ID` isn't read. A client that loses a stream sends the
  request again.
- **Never a `GET`.** Only an action asks a model, so a streaming route is
  never a `GET`. A browser's `EventSource` only sends `GET` and reconnects
  by itself, so it can't call one. A client reads the stream with `fetch`
  and a stream reader, as it reads OpenAI's and Anthropic's.

### Time, and a client that goes away

- **Time.** The stream is held to whichever comes first: the call's
  `timeout:`, counted from when the action asked, or the route's deadline,
  counted from when the request was matched. Past it, the stream ends with
  `error`.
- **Its timeout inside the deadline.** The check that each `timeout:` an
  action reaches leaves room inside its route's deadline
  ([Time limits](#time-limits)) counts a stream once, since it asks once.
  A stream's timeout is 25 seconds by default, so a streaming route sets
  its own `deadline:`.
- **A client that goes away** stops the stream. The runtime stops reading
  the model's answer and closes its connection to the provider at once, so
  nothing more is spent. What the action committed stays committed. This
  holds on every host: natively, the body is chunked and fed as each event
  is read; on Workers, it is a `ReadableStream` pulled as the client
  reads, and a client that goes away ends the request, which ends the
  model's call with it.

### Status, repeats, OpenAPI and the record

- **It answers 200 and waits for nothing.** A streaming route takes no
  other `status:` and no `settle:` (`route.stream_invalid`).
- **A stream is never kept for a repeat.** An `Idempotency-Key` keeps a
  repeat from running alongside the first until its action has committed.
  A repeat after the stream has started runs again, as for a response that
  starts a session (`keeps: request` in `x-jadpo-idempotency`, see
  [OpenAPI](#openapi)).
- **OpenAPI** lists the 200 as `text/event-stream`, its schema a string,
  described with its events. Each event's data schema is in
  `x-jadpo-stream` (`events: { piece:, done:, error: }`), since plain
  OpenAPI 3.1 has no field for it. The failures before the first piece are
  listed as any route's are.
- **The API record** writes the route's output as
  `output AiStream (server-sent events)`:

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

### In a route test

In a route test, a streaming route's `Response` has `pieces`, the text of
each `piece` event in order, in place of `output`. Its `code` is the
`error` event's code when one ended the stream, and `none` after `done`.
Its status is 200 once the first piece came. The `ai` fake cuts a scripted
text into pieces of a few words, and `partial("...", <condition>)` scripts
a stream that stops part-way ([Testing AI calls](21-ai.md#testing-ai-calls)):

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
<!-- topic: health -->

The runtime always serves two health checks, and there is nothing to
write:

- `GET /health/live` says the process is running.
- `GET /health/ready` says every store declared in `app.jadpo` answers
  within two seconds. It lists each provider-package connection by name,
  sorted, as `unchecked`, without calling the provider, whether the stores
  are ready or unavailable.

Neither path is part of an API's OpenAPI document or record.

## APIs and versions
<!-- topic: apis-and-versions -->

Most apps have one API. With nothing declared, there is one API, `main`,
at `/`.

An app with several audiences, such as its own clients, an admin team,
partners and webhooks, declares its APIs in `app.jadpo`, as it declares
stores:

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

A route group names its API with `api:`, and is in `main` when it says
nothing. Its prefix starts with its API's.

Each API has its own OpenAPI document, its own record and its own default
sign-in. Its entry lists the sign-in methods it accepts: here, a browser
session or an API token for `main`. A route group or a route can accept
fewer of them, but never add one
([Apps with more than one API](08-identity.md#apps-with-more-than-one-api)).
Each API serves its OpenAPI document at `<prefix>/openapi.json`, unless its
entry says `serve_openapi: false` ([OpenAPI](#openapi)).

### A new major version

A new major version is another API, `v2: { prefix: /v2 }`. Declare one only
when a breaking change must live beside the old version. When you delete
`v1`, the compiler reports every group and type left unused.

## Webhooks
<!-- topic: webhooks -->

You don't write routes for a provider's webhooks: Jadpo generates them
from the provider's package
([Webhooks as events](11-integrations.md#webhooks-as-events)). A connection
with a webhook secret gets `POST /webhooks/<connection>` in the `webhooks`
API. For each request, the runtime:

- checks the provider's signature over the raw body;
- drops repeats, by the provider's event id;
- answers a handshake, where the package names one;
- writes each event to the outbox, where events wait to be delivered to
  subscribers ([Subscribers](12-jobs-and-events.md#subscribers)).

A webhook that needs a reply runs the app's answering action inside the
request
([Webhooks that need a reply](11-integrations.md#webhooks-that-need-a-reply)).
You can change a webhook route's path, but you can't switch it off.

The route is `<prefix>/<connection>` under the prefix of the `webhooks` API
the app declares, whose record and OpenAPI document list it. Outside every
declared API, it is `/webhooks/<connection>`. `jadpo routes` shows it as
`generated webhook`, with `signature` for its authentication. An app route
at the same path is `route.path_overlap` (see the
[Integrations design notes](notes/11-integrations.md)). Replies and
handshakes are in
[Webhook replies, handshakes and further signature schemes](11-integrations.md#webhook-replies-handshakes-and-further-signature-schemes).

Two kinds of sender have no provider package:

- A system that signs its webhooks but has no package gets the app's own
  integration in `shared/providers/`, whose webhooks are generated the
  same way ([Senders with no package](11-integrations.md#senders-with-no-package)).
- A partner with an agreement signs in as a `Service`, with a service key
  ([Service credentials](08-identity.md#service-credentials)), and calls an
  ordinary route.

## Idempotent writes
<!-- topic: idempotent-writes -->

A client that retries a write after a timeout can't tell whether the first
try worked. So every route but a `GET` honours the `Idempotency-Key`
header: the client sends the same key with each try, and a repeat gets the
first try's answer. The runtime handles the key entirely; the action never
sees it.

HTTP already calls `PUT` and `DELETE` idempotent, but a retried `DELETE`
answers 404 where the first answered 204, and the action behind any write
may raise events or call providers. So the key covers them too.

- Before the action runs, the runtime claims the key, scoped to the
  caller.
- Afterwards, it keeps the response with a digest (a hash) of the request,
  in the action's transaction, for 24 hours.
- A repeat gets the kept response.
- The same key with a different request fails `idempotency_key_reused`
  (422). A repeat while the first is still running fails `conflict` (409).
- Successes and failures the action returns are kept. `unavailable` and
  internal failures are not, so a retry runs again.
- Calls to outside services in the action send keys derived from the
  client's key ([Calling](11-integrations.md#calling)).

An API can require the key with `idempotency: Idempotency.required`, and
OpenAPI lists the header. When a repeat counts as the same request is under
[How failures reach the client](#how-failures-reach-the-client). The rest
is in the [design notes](notes/07-http.md), "Idempotency keys".

## How failures reach the client
<!-- topic: how-failures-reach-the-client -->

Every failure reaches the client in one envelope: `code`, `message`,
`request_id`, and the failure's client fields under `details`
([Failures](04-failures.md)). The status comes from the failure's kind, and
the `x-request-id` header repeats the id.

Bad input lists each problem without its value. It names the field by its
path in the `Input`, with list positions as numbers, the names the OpenAPI
schema uses:

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

The runtime answers these itself, before or around what the route runs:

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

### When a repeat is the same request

A request with a key that came before is the same request when its route
(its method and declared path) and what it says, decoded, are the same.
What it says is each placeholder and the input: from the query string, the
headers it reads and the body. The runtime writes these back through the
route's own codecs as the contract's JSON, and keeps a digest of that.

That JSON writes every secret value (`Password`, `BearerToken` and the
rest, see [Standard library](19-standard-library.md)) as its kind alone. So
the digest the runtime keeps says nothing of a secret, whether it came in
the body or the path, and whoever can read the key table can't test
guessed passwords against it. A repeat that differs only in a secret, or
only in its JSON's spacing or member order, is the same request, answered
as the first was.

A response that starts a session or carries a credential is never kept.

The digest isn't keyed (no secret is mixed into the hash), since a key
would need a secret every deployment holds, and Jadpo has none
([Configuration and time](10-configuration-and-time.md)). So a value that
isn't secret, sent and neither stored nor answered, can still be confirmed
by guessing, by whoever reads the table. That person reads the kept
responses and the application's own tables as well.

### When the host has no clock

A deployable, a program `jadpo build` makes from the app such as a native
server ([Declared deployables](14-tooling.md#declared-deployables)),
answers under its host's clock. When the clock can't be read (the system
clock is before 1970), every request, health checks included, is 500
`internal_fault` and nothing runs. Its fault, for the host's log, says the
host has no clock. `jadpo dev` doesn't start (`cli.dev_start_failed`, "The
system clock can't be read").

### Wording for people

The envelope's `message` stays fixed English. Text for people in client
apps comes from the app's language files
([Failures for client apps](10-configuration-and-time.md#failures-for-client-apps)).
Every failure a route can produce has wording in every supported locale,
keyed by its code, with its client fields as placeholders. Client fields
that are dates or numbers stay values on the wire, for the client to
format in its own language.

## Generated routes
<!-- topic: generated-routes -->

Some features generate their routes for you:

- magic-link sign-in and passkeys ([Magic links](08-identity.md#magic-links),
  [Passkeys](08-identity.md#passkeys)), and `POST /auth/exchange`
  ([Identity](08-identity.md));
- the privacy requests ([Data requests](15-personal-data.md#data-requests));
- `GET /flags` ([Feature flags](16-built-in-services.md#feature-flags));
- a provider package's webhooks ([Webhooks](#webhooks));
- each API's `GET <prefix>/openapi.json` ([OpenAPI](#openapi));
- connecting and disconnecting an OAuth grant:
  `POST /connections/<name>/start`, `GET /connections/<name>/callback` and
  `POST /connections/<name>/disconnect`
  ([Acting for a user at another service: OAuth grants](08-identity.md#acting-for-a-user-at-another-service-oauth-grants),
  [Acting for each user: OAuth grants](11-integrations.md#acting-for-each-user-oauth-grants)).

Jadpo generates endpoints only, never pages: the app's frontend owns every
page. Generated routes appear in `jadpo routes`, the API record and OpenAPI
like any route, marked generated.

### Moving or replacing one

- A generated route takes a `path:` setting where its feature is
  configured (`magic_link: { path: "/login" }`).
- Or the feature takes `generate_routes: false`, and the app writes its own
  routes over the feature's generated operations (`magic_links.request`,
  `magic_links.redeem`, `sessions.start`), which keep every protection.
- Webhook routes, and passkeys' routes, whose bodies are WebAuthn's rather
  than the app's, can change path but can't be switched off
  ([Passkeys](08-identity.md#passkeys)).
- A generated route's response shape is fixed; only its path changes.

## Rate-limited requests
<!-- topic: rate-limited-requests -->

A request over a rate limit
([Rate limits](16-built-in-services.md#rate-limits)) gets `429` with the
usual envelope and the code `rate_limited`:

```text
HTTP/1.1 429 Too Many Requests
Retry-After: 42
RateLimit-Policy: "default";q=5;w=3600
RateLimit: "default";r=0;t=42

{ "error": { "code": "rate_limited", "message": "Too many requests.", "request_id": "req_0b1e…" } }
```

`Retry-After` gives the seconds to wait. `RateLimit` and `RateLimit-Policy`
are the standard headers of the IETF draft
`draft-ietf-httpapi-ratelimit-headers`. The failure's wording for people
comes from translations, as every failure's does, and is in the per-locale
failure export ([OpenAPI](#openapi)).

## OpenAPI
<!-- topic: openapi -->

The compiler writes one OpenAPI 3.1 document per API into `build/openapi/`.
It lists every route with its input, output, success status and the
failures it can reach, the 400 and 422 responses for bad input, and the
`///` descriptions
([Project layout and modules](01-project-layout-and-modules.md)). The
document's title is the application's name, and its version the first 12
hex digits of the SHA-256 of the API's record
([The public API record](#the-public-api-record)).

Each route's responses follow from what it runs:

- 403 where policy can refuse what it does, or what an action it calls
  does;
- 503 where it uses a store;
- an `Idempotency-Key` that promises a repeat the first response, only
  where that response is kept.

Every route shows its security scheme.

### Failure wording for client apps

Beside each document, `jadpo check` writes
`<api>.failures.<locale>.json` for each supported locale. It maps each
failure code the API can reach to
`{ "text": "…{next_slot}…", "formats": { "next_slot": "date_time" } }`, the
same shape for every code. `formats` lists only the placeholders the
client must format, each `number`, `decimal`, `date`, `date_time` or
`duration`, decided by the client field's type
([Failures for client apps](10-configuration-and-time.md#failures-for-client-apps)).

### Serving the document

Every API serves its document at `<prefix>/openapi.json` by default:
`/v1/openapi.json`, or `/openapi.json` for `main` at `/`. Other Jadpo apps
read it to call this one
([Packages from other Jadpo apps](11-integrations.md#packages-from-other-jadpo-apps)).
`serve_openapi: false` in the API's entry in `apis: { }` turns it off.

The served route is listed like any generated route: in the API's own
OpenAPI document and in its API record, marked generated, and in
`jadpo routes`. Its record entry lists it as the runtime's:

```text
GET /v1/openapi.json
    generated openapi
    authentication none
    status 200
```

- **It needs the API's default sign-in:** a signed-in caller, by any
  method the API accepts, so an admin API's document needs sign-in, and
  the record leaves out the `authentication none` line. Where the API
  accepts no way to sign in that is built, every route it has lets anyone
  call it, so its document tells no one more than its routes do, and
  anyone may read it, as above.
- **It answers 200 with the document exactly as `jadpo check` writes it**
  to `build/openapi/<api>.json`, as `application/json`. Like any route that
  takes no input, it answers a query string or a body with 422
  `invalid_input`.
- A route the application writes at the same path is `route.path_overlap`.

### The `x-jadpo` fields

Jadpo's documents carry `x-jadpo` fields for what plain OpenAPI lacks.
Other tools ignore them. Jadpo's OpenAPI importer reads them whenever it
finds them, so another Jadpo app's document imports as an exact provider
package, with no corrections or recordings
([How the OpenAPI importer reads a document](11-integrations.md#how-the-openapi-importer-reads-a-document)).

| Field | On | Holds |
|---|---|---|
| `x-jadpo-changes` | Every operation | Whether the operation changes something: `true` for a route that runs an action, `false` for a `GET` and for the runtime's own reads |
| `x-jadpo-idempotency` | Every operation that reads an `Idempotency-Key` | Whether it takes `Idempotency-Key`, and whether the API requires it: `header`, `required` (the API's `idempotency:`), `keeps` (`response`: a repeat gets the first answer; `request`: a repeat waits for the first but runs again; `nothing`: running it again changes nothing, or no store keeps it), `window` (`1d`), `max_length` (255), and the codes a key's repeat answers with, `in_use` (`conflict`) and `reused` (`idempotency_key_reused`) |
| `x-jadpo-failure` | The schema of each failure the application declares | Its `code` and `kind` (`not_found`); sign-in's, policy's and the runtime's own codes carry none, since the caller's runtime handles them |
| `x-jadpo-constraints` | Every field's, placeholder's and success response's schema | Every field's rules: its type and rules as the API record writes them, `Text { min_length: 1, max_length: 200 }`, `List<TodoView>`, `Money?` |

Each operation is tagged with its route group, and the group's `///`
description is the tag's, so an importer names a resource after it.

> `x-jadpo-events`, the public events the app publishes, waits for
> publishing events outward
> ([Where events travel](12-jobs-and-events.md#where-events-travel)): an
> application publishes no event to another yet, so no document carries
> it.

## The public API record
<!-- topic: the-public-api-record -->

Each API has a committed, readable record of its routes, fields and
failure codes: `api/main.jadpo_record` for `main`. `jadpo check` fails
while the record disagrees with the source, and `jadpo api update`
rewrites it. So every change to the API is a diff of one file, and a
breaking change is caught at review, not by a client in production.

The record is written in the
[record format](18-records-and-manifest.md#what-is-committed-and-why). Its
first line says what generated it and which command updates it. A hand
edit is `record.edited`, and a record out of date is `record.out_of_date`.

Here is a whole route group, with what it runs, and the record
`jadpo api update` writes for it:

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

### What the record holds

- **The wire contract.** A field shows what a client sends or reads.
  Objects and enums have an entry of their own, and each failure code has
  an entry with its status, message and client fields.
- **How clients sign in.** An API that signs clients in says how in an
  `api main` entry, one line per method (`authentication
  browser_session`). A route for one kind of caller says which
  (`caller user`). So switching an API from bearer tokens to cookies is a
  diff of the record.
- **A fixed order.** Everything is sorted: routes by path, then method;
  types, fields and variants by name; failures by code. Reordering the
  source changes nothing.
- **No version.** Two branches that both changed the API would always
  conflict on one. The OpenAPI document's version is the first 12 hex
  digits of the record's SHA-256, computed at build and never stored.

### Breaking changes

A change that can break a client also needs an acceptance written by a
person: a route removed, a required input field added, an output field
removed, or a failure code changed. An acceptance is a file in
`acceptances/` saying a person accepts the change
([Acceptances](18-records-and-manifest.md#acceptances)). Every change that
needs one uses the same form, such as a database drop, a policy weakening
or a new domain: one file per decision, listing the changes it allows,
counted only in the change that adds it.

`jadpo check` compares each API's record with the record on the default
branch at its merge base, and names each break with the entry an
acceptance lists: `"main GET /todos/{todo_id: Uuid}":
ApiChange.route_removed`, `main.CreateTodo.due:
ApiChange.input_field_required`, `main.CreateTodo.title:
ApiChange.rule_tightened` (a bound raised or lowered, a rule added, a type
changed, `none` no longer allowed, an enum variant gone),
`main.TodoView.priority: ApiChange.output_field_removed` or
`.output_field_retyped`, and `main.todo_locked: ApiChange.failure_changed`
(the code gone or its status changed). A route that now takes or
answers with another type is compared field by field with the one it had.
A route made open to anyone is `PolicyChange.route_public`
([Protecting policy](09-policy.md#protecting-policy)).

`acceptances/` and `api/` are owned in CODEOWNERS, the file a code host
reads to know who must approve changes to a path, so the host's review
shows who approved it. `jadpo new` writes one listing them.

## Write revisions
<!-- topic: write-revisions -->

After a write, a client can make sure its next read sees that write rather
than an older cached copy. Every write returns a `Jadpo-Revision` header,
and browsers get the same value as a cookie. A client that sends it back
skips caches ([Reads and writes](06-reads-and-writes.md)); one that does
not may see a cached copy up to its `keep:`.

> The `Jadpo-Revision` header isn't built yet: it comes with caches.

## What is built
<!-- topic: http-built -->

A feature that isn't built yet is the error `toolchain.not_built_yet`
where an app uses it.

| Feature | Status |
|---|---|
| Checking routes | Built: `jadpo check` checks route groups, their prefixes and APIs (with `idempotency:`), methods and typed placeholders, the one input (from the query string for `GET` and `DELETE`, the JSON body otherwise, and `header:` fields), `output:` and `status:`, `deadline:`'s range and what `run:` calls, every route of the project against the others and the health paths, and each API's record. |
| OpenAPI | Built: `jadpo check` writes one OpenAPI document per API to `build/openapi/`, and the failure wording for client apps beside it. Each API serves its own document at `<prefix>/openapi.json` (`serve_openapi: false` turns it off), listed in its record, its document and `jadpo routes`, and the documents carry the `x-jadpo` fields the OpenAPI importer reads ([OpenAPI](#openapi)). OpenAPI derives each route's responses from what it runs and shows `ZonedTime`, `Zone` and a mixed enum as they are sent. |
| Serving a request | Built: the runtime's HTTP component routes a request, decodes and checks its input (each problem as `{ field, rule }`), runs the action or query, and answers with the output or the failure envelope, its `request_id` and `x-request-id`, with the status the failure's kind sets; it answers `HEAD` and `OPTIONS` itself and counts the default rate limits. It reads a body only as JSON, and refuses a query string or body a route doesn't read, a header it can't read and a body over 1 MB. |
| Deadlines and concurrent changes | Built: the runtime stops a route at its deadline without committing, even work that never waits, and runs an action again on a concurrent change found as it commits. |
| Health endpoints | Built: `/health/live` and `/health/ready`. |
| Idempotent writes | Built: the runtime keeps each write's `Idempotency-Key` with its response. |
| Commands | Built: `jadpo dev` serves the routes and reloads on save, `jadpo routes` lists them and `jadpo api update` writes the records, each `api/<api>.jadpo_record` in the record format with no `version:` line; the OpenAPI version is a hash of it. |
| Native connection limits | Built: natively, a client has 10 seconds for its headers and 30 for its body, a write waits at most 30 seconds for it to read, and the server holds at most 1,024 connections and 10 database connections per store per worker. |
| Sign-in | Built: every route needs a signed-in caller unless it says `authentication: RouteAuthentication.none`; each API lists the sign-in methods it accepts, named in its record; a route passes `current_principal` to what it runs; a signed-in caller's requests count against its own rate limit, and its idempotency keys are its own; a request with a credential on a route anyone may call counts against its address before it runs ([Identity](08-identity.md)). |
| Policy | Built: policy decides what each route reaches, and OpenAPI lists the 403 `not_permitted` on every route that runs an action that writes or is listed under `actions:` ([Policy](09-policy.md)). |
| Issued tokens and keys | Built: a `GET` runs `sessions.tokens` and `<Service>.keys` itself, and the record and OpenAPI describe the `IssuedToken` and `IssuedKey` it sends ([Service credentials](08-identity.md#service-credentials)). |
| Map layers | Built: a route may send a `FeatureCollection<G, P>` bare, its properties held to what an `Output` may hold and, where an action returns it, to the fields every caller may read ([Field restrictions](09-policy.md#field-restrictions)); OpenAPI describes RFC 7946's object and the record names it with its type arguments. A `GET` sends one a query gives ([Reads and writes](06-reads-and-writes.md), "A query giving a map layer"), held to field policy where the query is declared. |
| `settle:` | Built: a route waits for the work its action set off ([Waiting for follow-on work](#waiting-for-follow-on-work), and `route.settle_invalid`). |
| Streaming | Built: a route whose action returns an `AiStream` sends a model's answer as server-sent events ([Streaming a model's answer](#streaming-a-models-answer), and `route.stream_invalid`). |
| Failure wording from a build | Not built yet: `jadpo check` writes `<api>.failures.<locale>.json` beside each OpenAPI document, and `jadpo build` is to write it too. |
| `x-jadpo-events` | Not built yet: it waits for publishing events outward ([Where events travel](12-jobs-and-events.md#where-events-travel)). |
| A route's own `rate_limit:` and the Redis store for rate limits | Not built yet ([Rate limits](16-built-in-services.md#rate-limits)). |
| Classifying breaking changes and their acceptances | Built ([Acceptances](18-records-and-manifest.md#acceptances)). |
| The `Jadpo-Revision` header | Not built yet: it comes with caches ([Write revisions](#write-revisions)). |

## Errors

Every error this page's routes and APIs can produce, with an example that
triggers it and the compiler's real output. Most examples are one loose
file, checked without a project's layout rules, so the route sits beside
what it runs; in a project each lives in its own folder.

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

