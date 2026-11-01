---
title: Debugging and review
area: 17
status: decided
checked-with: this checkout's jadpo on every gate run
rules: []
---

# Debugging and review
<!-- topic: debugging-and-review -->

Jadpo code is written mainly by coding agents and read by people, so two
jobs matter most. When something goes wrong, you or your agent need to get
from the symptom to the `.jadpo` line and the reason, without reading the
code Jadpo generates. And when an agent hands over a large change, you need
to see what it does to your app, such as who can now read which data,
before you read it line by line. This page covers both: where errors
point, how to see what a request did and why, and how to review a change.

The command line, the developer console and the agent tools (MCP) all show
the same evidence. The compiler and the runtime record that evidence, and
each view only displays it. If the console shows why a caller was refused,
that's because the compiler and the runtime recorded the reason, not
because the console worked it out.

| Tool | What it does |
|---|---|
| Errors point at `.jadpo` | Always a `.jadpo` file and line, in development and production |
| Production incidents | `jadpo incident` names the line in the build that failed, then where that code is now |
| Breakpoints | Not offered |
| Why the language decided something | `jadpo why` |
| What a request did | The request timeline in `jadpo dev`; `jadpo trace <request id>` |
| Watching code run | `jadpo watch` waits for a named part of the code to run and prints it |
| Provider calls in development | `jadpo calls`: every call logged with full values, in development only |
| Traces, metrics and logs in development | `jadpo traces`, `jadpo metrics`, `jadpo logs`, with filters, `--follow` and `--json` |
| Failing request to test | `jadpo test new --from <request id>` |
| Run a query or action | `jadpo call <name> --as <user>` |
| Developer console and MCP | A local web page and read-only agent tools over the same evidence |
| Reviewing a change | A report with behaviour first, and a static web view of it |
| JSON for agents | Every command and view ([Tools for agents](14-tooling.md#tools-for-agents)) |
| Coverage by path | Tests and production compared path by path |
| Production in review | The approval report shows traffic and failures on the paths a change touches |
| Replaying a production request | Opt-in recording of what each effect returned, replayed locally |
| Query cost of a change | Queries added or removed per path, in the approval report |

Most of this isn't built yet; [What is built](#what-is-built) says which
parts are.

## Errors point at the `.jadpo` line
<!-- topic: errors-point-at-the-jadpo-line -->

Every stack, log event and error response that names a place names a
`.jadpo` file and line, never the generated code. You don't turn this on;
it is always the case. The compiler writes a source map, a record of which
`.jadpo` line each piece of generated code came from. Before a frame is
logged, the runtime uses that map to rewrite it, on the native target and
on WebAssembly alike. A fault reads:

```text
{"event":"operation.failed","classification":"int_overflow",
 "request_id":"req_0b1e…","build":"src_2c78570a2841d2f7",
 "stack":[
   "domains/ordering/actions/price_line.jadpo:4  action price_line",
   "domains/ordering/actions/place_order.jadpo:12  action place_order",
   "domains/ordering/routes/orders.jadpo:8  route POST /orders"]}
```

A frame inside the runtime's own code shows as `runtime: <component>`
rather than a generated-code path. That tells you the fault is in Jadpo,
not in your app.

A domain failure, an outcome your app declares such as `TodoNotFound`,
still logs no stack. It logs only where it was raised and the path it
travelled ([Logging](04-failures.md#logging)). Frames carry places and
names, never values.

## Production incidents after the code has changed
<!-- topic: production-incidents-after-the-code-has-changed -->

By the time you look at a production failure, the code may have moved on.
Every production event names the build it came from and its `.jadpo`
frames, and `jadpo incident` takes that event and tells you two things:

1. **Where it happened**, in the source of that build: the file, the line
   and the lines around it. `jadpo build` records the git commit in the
   deployable's build stamp, the small record of what was built
   ([Production builds](18-records-and-manifest.md#production-builds)).
   The runtime logs the stamp with every event, so the command can read
   the old source from git.
2. **Where that code is now**: the same declaration on your current
   branch. Jadpo finds it by its stable identity, an ID that follows a
   declaration through renames and moves
   ([Stable identity](18-records-and-manifest.md#stable-identity)). If the
   declaration has changed, been removed or been split, the command says
   so rather than guessing.

```text
$ jadpo incident event.json
int_overflow in action price_line
  then   domains/ordering/actions/price_line.jadpo:4   (build src_2c78…, commit 8d113e6)
  now    domains/ordering/actions/price_line.jadpo:6   (changed since: 2 commits)
  path   POST /orders → place_order → price_line
  next   capture it as a test: jadpo test new --from-event event.json
```

## Breakpoints and stepping
<!-- topic: breakpoints-and-stepping -->

You can't set a breakpoint in a `.jadpo` file or step through one in an
editor. An editor's debugger would need to map the generated Rust or
WebAssembly back to `.jadpo` lines, and their debug information can't do
that for a language they didn't compile.

Use the [request timeline](#the-request-timeline-in-jadpo-dev),
[`jadpo why`](#jadpo-why-why-the-language-decided-something) and
[captured tests](#capturing-a-failing-request-as-a-test) instead. They
answer the questions a debugger would, in a form both you and your agent
can read. Agents can't use breakpoints anyway.

## `jadpo why`: why the language decided something
<!-- topic: explain-a-decision (was jadpo-why-why-the-language-decided-something) -->

Much of what surprises people in Jadpo is a decision the language made for
them: policy hid a record, a lifecycle filtered it out, a subscriber didn't
run, or a cache answered. `jadpo why` explains one such decision from what
the compiler knows and what was recorded, and it says so when it doesn't
know.

It covers:

- **policy**: which grant lists were checked against which facts
  ([Policy](09-policy.md));
- **lifecycle**: a record that normal reads don't see, such as one that
  was soft-deleted ([Lifecycle](05-data.md#lifecycle));
- **concealment**: a record policy hid by answering as if it didn't exist
  ([Concealing records](04-failures.md#concealing-records));
- **cache use**: a read served from a cached copy;
- **subscriber runs**: which subscribers ran for an event, and why others
  didn't ([Subscribers](12-jobs-and-events.md#subscribers)).

Give it a request, event or delivery id (`jadpo why <id>`), or a record and
a caller (`jadpo why <Entity> <id> --as <user>`):

```text
$ jadpo why req_0b1e
GET /todos/0192… answered 404 todo_not_found
  Todo 0192… exists, but policy concealed it (area 4: hidden looks absent)
  read: [owner_id]          owner_id is user 0192…a7; caller is user 0192…c3   no match
  policy   domains/planning/entities/todo.jadpo:14

$ jadpo why Todo 0192…f1 --as 0192…a7
Todo 0192…f1 is not visible to normal reads
  lifecycle visible: deleted_at == none   deleted_at is 2026-10-11T08:02:00Z
  purged after config.soft_delete_retention (30d), on 2026-11-10
  lifecycle  domains/planning/entities/todo.jadpo:22

$ jadpo why evt_7c21
TodoCompleted from req_9a40 (complete_todo, committed)
  update_statistics   succeeded, 1 attempt, 12 ms
  send_reminder       not subscribed: listens for ReminderDue

$ jadpo why req_5d3e --query todos_for_owner
todos_for_owner served from cache `cache` (age 14 s, keep: 60 s)
  display read on GET /todos; your own write was older than the copy (area 6)
```

Each answer ends with the `.jadpo` place that made the decision. On your
machine, `jadpo why` reads what `jadpo dev` recorded and the local
database. For production, it reads a log event you exported, as
`jadpo incident` does, so it can only explain what the logs recorded.

## The request timeline in `jadpo dev`
<!-- topic: the-request-timeline-in-jadpo-dev -->

`jadpo dev` records every request it serves and prints one line for each,
with its request id. `jadpo trace <request id>` shows that request's whole
timeline: the route, the caller, the actions and queries with their SQL,
rows and timings, the writes, the events emitted, the subscribers run, the
calls to outside services, and the outcome. It is like a development log
that also gives each query's source line. It is built from the spans, the
timed steps of a trace, that Jadpo already records
([Telemetry](16-built-in-services.md#telemetry)).

```text
$ jadpo dev
listening on http://localhost:3000
req_0b1e  POST /todos/0192…/complete   409 todo_mutation_conflict   41 ms
req_0b1f  GET /todos                   200   9 ms   3 queries

$ jadpo trace req_0b1e
POST /todos/0192…/complete   409 todo_mutation_conflict   41 ms   as user 0192…a7
  route    domains/planning/routes/todos.jadpo:31
  action   complete_todo                   domains/planning/actions/complete_todo.jadpo:4
    query  Todo.get           3 ms  1 row  SELECT … FROM todos WHERE id = $1 AND deleted_at IS NULL
    write  Todo.update        2 ms  0 rows (expected revision 7, found 8)
    fail   TodoMutationConflict           complete_todo.jadpo:9
  rolled back; no events emitted
```

### What the SQL shows

SQL is shown with placeholders. Its parameters follow the rule for a
failure's internal fields in logs ([Logging](04-failures.md#logging)):
identifiers, numbers, enums, booleans and times are shown as values, and
text as a keyed hash, so you can tell two values apart without seeing
either.

### Where timelines are kept

The last 500 timelines are kept in `build/dev/`, which Jadpo generates and
you never commit. Production records the same spans and sends them through
OpenTelemetry ([Telemetry](16-built-in-services.md#telemetry)), so you
view them in whatever tool your team already uses.

### What gets flagged

A request that runs several queries for one call, or runs a query with no
index to use, is flagged on its line. These are the same problems the
query report flags, the report `jadpo check` writes to `build/report/`
([Tools for agents](14-tooling.md#tools-for-agents)).

### Provider calls in the timeline

Calls to a provider, an outside service such as Stripe, appear in the
timeline with their inputs and answers in full and where each answer came
from: `recording`, `generated`, `scripted` or `sandbox`
([Provider calls in development](#provider-calls-in-development-jadpo-calls)).

An action can make several calls that change something at a provider.
For such an action, the timeline shows each step's state: recorded, sent,
answered, replayed or in doubt
([Several changing calls in one action](12-jobs-and-events.md#several-changing-calls-in-one-action)).

## Watching code run: `jadpo watch`
<!-- topic: watching-code-run-jadpo-watch -->

Your agent can wait for one part of the code to run and then see its
inputs and outputs, without reading whole logs.

```text
jadpo watch action shipping.book_collection --once --json
jadpo watch domain billing
jadpo watch call couriers.bookings.create --failed
jadpo watch file domains/shipping/actions/book_collection.jadpo
```

### What you can watch

You name the code as the compiler knows it: a route, action, query,
subscriber, schedule, provider operation, domain or file. Add `--failed` or
`--faulted` to see only the runs that failed or faulted. A name the
compiler doesn't know is an error that names the nearest match, as
everywhere else in Jadpo.

### What each match prints

Each match prints the step that ran: its inputs, its output or failure, the
queries, writes, events and calls inside it, its timings, and the request
id, so `jadpo trace <request id>` opens the whole request.

For an action with several changing calls, each step shows its state. A
step that didn't finish is an item for an operator, the person running the
app, to resolve
([What an operator sees](12-jobs-and-events.md#what-an-operator-sees)).

### Waiting for one run

`--once` exits after the first match. Your agent starts the command in the
background, triggers the flow, and waits for the command to finish rather
than polling. `--count <n>` and `--timeout <duration>` limit how long it
waits, and `--json` gives the same output as data.

### Values in full, in development only

`jadpo watch` works only in development. It shows values in full, as the
[development call log](#provider-calls-in-development-jadpo-calls) shows
what was sent to providers, read from data that `jadpo dev` keeps out of
git. Secrets are never shown. In production, only the spans are kept, with
no values ([Telemetry](16-built-in-services.md#telemetry)).

## Provider calls in development: `jadpo calls`
<!-- topic: provider-calls-in-development-jadpo-calls -->

In `jadpo dev`, every provider operation is faked by default, so nothing
leaves your machine and you need no credentials. A fake answers from the
package's recordings, real answers saved with the package, where it has
some, and from generated defaults otherwise
([Provider calls in development](11-integrations.md#provider-calls-in-development)).

### Letting calls through to a sandbox

To use the provider's sandbox for some calls, name the operations or
resources in the connection's `development: { sandbox: [...] }`. Those go
to the sandbox, never to the live service. `jadpo dev --fake <connection>`
and `jadpo dev --sandbox <operation>` change this for one session only.

```text
$ jadpo calls
call_31  stripe.customers.create         generated   201   4 ms   req_0c02
call_32  stripe.payment_intents.create   scripted    card_declined   1 ms   req_0c02

$ jadpo calls script stripe.payment_intents.create card_declined
next call to stripe.payment_intents.create answers card_declined
```

### What is logged

Every call is logged, faked or real. The log holds the connection,
resource and operation, the input, the answer or failure in full, where the
answer came from (`recording`, `generated`, `scripted` or `sandbox`), the
attempts and retries, how long it took, and the request id and span. So
each call appears in `jadpo trace` beside the queries.

### Listing calls

`jadpo calls` lists the calls, and `--connection`, `--operation` and
`--request` narrow the list. `jadpo calls show <id>` prints one call. Both
take `--json`. The calls are kept with the last 500 timelines in
`build/dev/` and never committed.

### Scripting the next answer

`jadpo calls script <operation> <failure>` sets the next answer to an
operation, in the same form as a test's `fakes:`
([Fakes for integrations](13-testing.md#fakes-for-integrations)). Your
agent can use it to walk through a failure path by hand.

Fakes keep no state. So for a flow that creates an object and then reads
it, either let both calls through to the sandbox, or script the read.

### What the call log shows in full

The log shows inputs and answers in full. Everywhere else, text in a
call's payload is hashed; this is the one exception, and it holds in
development only. Credentials and `secret: true` values are never shown.
In production, only the spans are kept, with no values
([Telemetry](16-built-in-services.md#telemetry)).

### Fake OAuth grants

`jadpo dev grants add <connection> --owner <id>` makes a fake OAuth grant,
the permission a person gives your app to act for them at a provider, for
that owner
([Acting for each user: OAuth grants](11-integrations.md#acting-for-each-user-oauth-grants)).

> **Built**, except `jadpo trace`, `jadpo watch call` and the request
> timelines. `jadpo dev` logs each provider call as it ends in
> `build/dev/calls.jsonl`, and logs an answer from a recording as
> `recording`. `jadpo calls` reads that file. Here is the plain list after
> one booking answered with the generated default and one scripted to find
> its slot taken:
>
> ```text
> $ jadpo calls
>   call    operation                source     answer        time  request
>   call_1  couriers.pickups.create  generated  + answered    0 ms  req_58d4
>   call_2  couriers.pickups.create  scripted   x slot_taken  0 ms  req_9588
>
> calls: 2 calls, 1 answered, 1 failed
>
> next: jadpo calls show call_2  shows what was sent and what came back
> ```
>
> In a terminal, the answers are `✓ answered` and `✗ slot_taken` in their
> colours, and the footer reads `✓ 2 calls  1 answered · 1 failed`. One
> call shown in full, with its request as made and its answer laid out one
> member to a line:
>
> ```text
> $ jadpo calls show call_2
> `couriers.pickups.create` failed with `slot_taken`
>     call     call_2
>     source   scripted
>     request  req_9588e1fb3505d4a7
>     at       2026-10-20T06:34:34.202Z
>     time     0 ms
>
> sent
>   POST https://sandbox.swift-couriers.example/v1/pickups
>   {
>     "postcode": "N1 9GU",
>     "slot": "pm"
>   }
>
> $ jadpo calls script couriers.pickups.create slot_taken
> Scripted `slot_taken` for the next call to `couriers.pickups.create`
>
> next: jadpo calls  shows the call once the app makes it
> ```
>
> The request id in the list is shortened as `jadpo dev`'s request lines
> shorten it, and `--request` takes any start of it. With no calls yet,
> the list says `No calls yet` and points at `jadpo dev`. With filters
> that match nothing, it says `No calls match` and repeats the filters.
> The [design notes for integrations](notes/11-integrations.md) record the
> choices behind the development call log.

## Traces, metrics and logs in development
<!-- topic: traces-metrics-and-logs-in-development -->

In development, your app's telemetry, its traces, metrics and logs, stays
on your machine ([Telemetry](16-built-in-services.md#telemetry)).
`jadpo dev --export-telemetry` sends it on to a collector. You read it from
the command line, as you read the call log:

| Command | Shows |
|---|---|
| `jadpo traces` | Recent requests, schedule runs and deliveries, one line each, opening into `jadpo trace <id>` |
| `jadpo metrics` | The metrics the runtime records and your app's own (`monitoring.jadpo`), per route, action or path |
| `jadpo logs` | Your app's logs, including those of its [services](11-integrations.md#services), by level |

Each command takes the same filters: by route, action, domain, path,
failure and slowness (`--slower-than 200ms`). `--follow` keeps printing
as new entries arrive, and `--json` gives the output as data. The
developer console will show the same data.

These commands' screens follow the
[CLI design language](../cli-design-language.md), as the screens of
`jadpo watch` and `jadpo calls` do.

## Capturing a failing request as a test
<!-- topic: capturing-a-failing-request-as-a-test -->

The quickest reliable fix starts by reproducing the bug.
`jadpo test new --from <request id>` writes a route test that replays a
request `jadpo dev` recorded, with the data the request read written as
fixtures, the test data a test starts from. The test asserts what was
observed. You change the assertion to what should happen, watch the test
fail, then fix the code. Your agent can do the whole loop.

```jadpo fragment
// domains/planning/routes/todos.test.jadpo
/// Captured from req_0b1e by jadpo test new --from.
test "completing a todo twice answers a conflict" {
    clock: "2026-10-12T09:14:03Z"

    var alice: Principal.user = captured_user_1()
    var todo: Todo = captured_todo_1(alice)
    var response: Response = request {
        method: HttpMethod.post
        path: "/todos/{todo_id}/complete"
        params: {
            todo_id: todo.id
        }
        as: alice
    }
    // observed: 409 todo_mutation_conflict
    assert response.status == 409
    assert response.code == "todo_mutation_conflict"
}
```

### What the captured test holds

- **Fixtures** go in the domain's `fixtures/` folder and create records
  through the entity's generated operations
  ([Shared test data: fixtures](13-testing.md#shared-test-data-fixtures)).
- **Personal data** is never copied. Fields marked `personal:` are replaced
  with generated values, so a captured test never commits real personal
  data ([Marking personal fields](15-personal-data.md#marking-personal-fields)).
- **Calls to providers** become `fakes:` that give the answers the request
  received.

### From a production event

`--from-event <file>` does the same from a production event. It uses only
what the event recorded, so it can't write the fixtures; you fill those in
yourself.

## Running a query or action: `jadpo call`
<!-- topic: running-a-query-or-action-jadpo-call -->

Where Rails has an interactive `rails console`, Jadpo has a one-shot
command. `jadpo call` runs one query or action against your local database
as a caller you name, with JSON in and out, so your agent can script it.

```text
$ jadpo call todos_for_owner --as 0192…a7 --input '{"owner_id":"0192…a7"}'
{"items":[{"id":"0192…f1","title":"Buy milk"}],"next":null}

$ jadpo call complete_todo --as 0192…a7 --input '{"todo_id":"0192…f1"}' --rollback
{"id":"0192…f1","completed_at":"2026-10-12T09:20:00Z"}
rolled back
```

### Policy always applies

`--as <user id>` signs in as that user, through the same principal, the
record of who is calling, that a route would build
([The principal](08-identity.md#the-principal)). Without `--as`, the
caller is someone who isn't signed in. Nothing switches policy off, so
what works here also works through a route.

### Writes, on your machine only

An action may write. `--rollback` runs it inside a transaction and then
rolls the transaction back, like Rails' `--sandbox`. As with
[seeds](05-data.md#seeds), `jadpo call` refuses to run against any
environment marked production.

Every call is recorded in the timeline, so `jadpo trace` and `jadpo why`
work on it.

## The developer console and MCP
<!-- topic: the-developer-console-and-mcp -->

The same evidence is also there as a web page, and as tools for an agent
that can't run commands. An agent that can run commands, such as Claude
Code, Codex or Cursor, uses `jadpo` with `--json`, and `jadpo help --json`
for the command reference. That is enough for building an app.

### The developer console

`jadpo dev` serves a local page on its own port. It shows recent requests
and their timelines, errors grouped by place, slow routes and queries,
schedule runs and deliveries with their attempts and states
([How a delivery runs](12-jobs-and-events.md#how-a-delivery-runs)), and
the reports in `build/report/`: permissions, personal data, schema and
queries.

The console only reads. To retry or dismiss a delivery, you run
`jadpo deliveries`
([Commands for background work](14-tooling.md#commands-for-background-work)),
and the console shows you how.

### The application graph

The application graph is the compiler's call graph: route → action → query
→ entity, and event → subscriber. It is drawn per domain and opened one
level at a time, with each node linked to its source and its `///`
description. In `jadpo dev`, nodes are coloured by recent traffic, failures
and time spent, and a request you select lights up its path.

### The MCP server

`jadpo mcp` gives an agent the same evidence as read-only tools: `trace`,
`why`, `incident`, `impact`, `sql`, `routes`, the reports and the graph.
It is for agents with no shell, mostly investigating production from a
chat or a hosted tool.

It can't write data, retry deliveries or edit code; a person or an agent
runs commands for those, deliberately. It is generated from the same
command reference as `jadpo`, so the two always match. It is protected by
your app's own OAuth ([Identity](08-identity.md)).

### Production monitoring

For production, use your OpenTelemetry tools. The console is for your
machine.

## Reviewing a change
<!-- topic: reviewing-a-change -->

When an agent hands you a change, you should see it in the order it
matters. `jadpo approval --before <ref>` writes a report in that order, as
Markdown and JSON in `build/review/`, and a static web view of the same
report:

1. **Decisions that need a person.** Each change that needs an acceptance,
   a file in `acceptances/` in which a person accepts it
   ([Acceptances](18-records-and-manifest.md#acceptances)). Each is said in
   plain words, who can now do what to which data ("`CompanyRole.viewer`
   can now read `work.Project.budget`"), with whether its acceptance file
   exists: `acceptance missing`, `reason missing` while one lists it with
   no `///` reason yet, or `accepted`, with the reason.
2. **Paths.** Under each decision, the path that makes it true: actor →
   route → action or query → field → output. A path starts only with
   callers who can take it: `anyone` only through a route anyone may call,
   and a query or action listed under `actions:` only for those on its line
   ([Listing a query](09-policy.md#listing-a-query)). A decision with no
   such way says none was found; the source and the records still hold it.
3. **Newly reachable code.** Declarations that didn't change but are now
   reached from a new place, which a line diff never shows.
4. **Other behaviour.** Changes to the
   [public API record](07-http.md#the-public-api-record), the schema and
   [migrations](05-data.md#migrations), failures, events and subscribers,
   integrations, and the differences in the
   [permissions report](09-policy.md#the-permissions-report), as
   `access.jadpo_record` holds it, and the
   [personal-data report](15-personal-data.md#the-personal-data-report). A
   record's changes are shown as a diff of its entries. Beside them: what
   widens access with no grant changed, an output gaining a field or an
   action reading or writing another entity; every
   [decline](01-project-layout-and-modules.md#declining-a-judgement-call)
   the change adds or edits, with its reason; each prompt that changed; the
   imported types and the calls that ship unreviewed or unconfirmed
   ([Mapping imported types](11-integrations.md#mapping-imported-types-to-standard-types));
   and [wording not yet accepted](#wording-not-yet-accepted).
5. **Source,** grouped by declaration rather than by file, by domain, each
   with whether it was added, changed or removed and where it is.

The terminal says the decisions in full, with each one's first path, and
counts the rest. Exit code 0 means the report was written, whatever it
says: `jadpo check` is what fails until each decision has its acceptance.

```text
$ jadpo approval --before main
1 decision needs a person
◆ `CompanyRole.viewer` can now read `work.Project.budget`     acceptance missing
    CompanyRole.viewer → GET /projects/{id} → work.project_detail
      → ProjectView.budget

1 declaration newly reachable
  budgets.convert_currency  now reached from GET /projects/{id}

  before    main at 1a2b3c4
  api       1 route added
  schema    1 migration added
  access    1 field's grants changed
  source    6 declarations in 4 files

✓ Wrote build/review/approval.md

◆ decision  1 change waits for a person's acceptance
  next      A person accepts them: `jadpo acceptance new <purpose>` writes the
            draft for them. (human)
```

With `--json`, `data` holds the same report as `build/review/approval.json`
and the `file` it was written to. A source with errors writes no report:
what changed can only be worked out from a source that reads
(`approval not written`, with each error). An argument, a `--before` git
doesn't know, or nothing to compare with is
[`cli.approval_arguments`](../errors/cli.approval_arguments.md); a base the
clone doesn't hold is
[`acceptance.base_missing`](../errors/acceptance.base_missing.md).

### Packages and services that need a person

Packages add their own decisions to the first section, each needing a
person's acceptance:

- A third party's package installed from git rather than the registry. It
  is marked **unverified** until a person accepts it.
- A **package's service** whose image is new or changed, shown with its
  image, digest and the hosts it asks to reach. It never starts until
  accepted.
- A new outside host a service is allowed to reach.
- A connection that receives personal data but can't erase it
  ([Erasure at providers](15-personal-data.md#erasure-at-providers)).
- Bot checks allowed to pass when their provider is down
  ([Bot protection](16-built-in-services.md#bot-protection)).

### Wording not yet accepted

With `translations: { review: TranslationReview.required }`, wording that
changed and hasn't been accepted with
`jadpo translations accept <locale>` is listed under other behaviour too
([Tooling for translations](10-configuration-and-time.md#tooling-for-translations)).

### The web view

The web view adds what text can't: the graph of the change, the
differences between an agent's successive revisions of it, each path opened
to see what surrounds it, and links from every line to its source. It is a file in `build/review/`, which you open
on your machine or CI publishes beside the pull request. It holds no
accounts and records no approvals. Acceptances stay files, reviewed on the
git host like any other change.

## What the graph and production evidence add
<!-- topic: what-the-graph-and-production-evidence-add -->

Each of these joins the compiler's graph to path IDs. Every request
records the path it took through your app, the route, each action and
query, and each branch taken inside them, as a number the graph can
decode ([Path IDs](16-built-in-services.md#path-ids)). Production data is always
an optional input: a file exported from your monitoring tool, or a service
that holds it. Every command works without it.

### Coverage by path

`jadpo test` reports which paths your tests ran, not which lines. Line
coverage says a branch ran once; path coverage says which combinations of
branches ran.

Given production path counts, it also lists the paths that ran in
production but have no test, busiest first, and
`jadpo test new --from` writes the missing test. Paths in the graph that
production hasn't used for 90 days are listed as candidates for removal.

### Production in review

Given production path counts, the approval report says how much a change
touches what is in use: "touches paths that served 38% of requests last
week; one failed 12 times".

### Replaying a production request

Time, randomness, calls to providers and the database are all declared
effects, so the runtime knows every source of results that can differ from
one run to the next. A deployment can opt in to recording what each effect
returned for a failing request:

- only failing requests are recorded;
- personal fields are replaced, using their `personal:` marks
  ([Marking personal fields](15-personal-data.md#marking-personal-fields));
- the recordings expire, as logs do.

`jadpo replay` then runs that request on your machine and gets the same
results. It extends `test new --from` from `jadpo dev` to production.

### Query cost of a change

The compiler counts the queries on every path by itself, so the approval
report can say "adds 2 queries to `POST /orders`". Given production
counts, it also says which of those paths is busiest.

## Everything as JSON
<!-- topic: everything-as-json -->

Every command on this page takes `--json`
([Tools for agents](14-tooling.md#tools-for-agents)), and each kind of
evidence has one versioned schema: the fault event, the request timeline,
`why` answers, the graph and the review report. The console and the MCP
server read those same files, so an agent never has to scrape text meant
for people.

That evidence is what other tools can rely on: `.jadpo` locations, the
build and commit on every event, these schemas, and the commands `why`,
`trace`, `call` and `test new --from`. The console, the MCP server and the
review web view are built on top of it.

### Values in reports

A report writes a value the way code makes it:
`money.from_minor(1050, Currency.gbp)`, `2.5262metre`, `"2026-10-25"`,
`PostalAddress { ... }`. That applies to a failed assertion, a failure's
fields, a fault and a timeline, and to their `--json`. So a value keeps a
standard type's currency, unit and every decimal place, and your agent can
paste it into a test.

A report is read by people and agents and never read back in as data, so
it isn't one of the boundaries a standard type is encoded across
([Every boundary through the contract](19-standard-library.md#every-boundary-through-the-contract)).
Still, no standard type loses its currency, unit or precision in one. A
personal value in a report follows the rules for
[personal data](15-personal-data.md).

## What is built
<!-- topic: debugging-and-review-built -->

| Feature | Status |
|---|---|
| `jadpo calls`, `calls show` and `calls script` | Built. |
| Each step a request's action reached, as a trace line in `jadpo dev` | Built. Until the timelines are built, each step is a line under its request's line in `jadpo dev`, and a delivery whose action reached steps gets a line of its own ([design notes for jobs and events](notes/12-jobs-and-events.md)). |
| `jadpo why <schedule>` | Built: it answers why a schedule didn't run ([Watching schedules](12-jobs-and-events.md#watching-schedules)). |
| `jadpo approval` | Built: the report in `build/review/` in [this page's order](#reviewing-a-change), the before side built from git and its manifest kept in `build/before/<commit>/`. The personal-data report's difference comes with connections in that report, and a package's service and a bot check with those features. |
| Mapped `.jadpo` stacks, `jadpo incident`, the other forms of `why`, timelines, `test new --from` and `call` | Not built yet: these come first ([the roadmap](../roadmap.md)). |
| The developer console, MCP and the review web view | Not built yet: they come after those, as product work. |
| `jadpo traces`, `metrics`, `logs` and `watch`; path coverage, production counts in review, replay and query cost | Not built yet. |
