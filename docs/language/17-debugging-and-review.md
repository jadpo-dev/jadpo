---
title: Debugging and review
area: 17
status: decided
checked-with: jadpo 3ce094d0 (2026-10-12)
rules: []
---

# Debugging and review

Jadpo code is written mainly by models and read by people. Two jobs follow
from that. When something goes wrong, a person or an agent must get from the
symptom to the `.jadpo` line and the reason without reading the generated
code. And when a model hands over a large change, a reviewer must see
what it does to the application (who can now do what to which data) before
reading it line by line. This page covers both: where errors point, how to
see what a request did and why, and how a change is reviewed.

> Decided on 2026-10-13. Nothing here is built yet: debugging and review come after the first build (roadmap), and the developer console, MCP and review web view after that, as product work (E11, E06). What other frameworks do, and the options considered, are in [the proposals](research/17-debugging-and-review-proposals.md). Commands, `--json` and the build folder are area 14's; logging is area 4's; tracing is area 16's.

**One rule behind all of it.** The command line, the developer console and
the agent's MCP server are three views of one set of evidence, produced by
the compiler and the runtime and versioned like any other generated file. No
view works anything out for itself: if the console shows why a caller was
refused, it is because the compiler and the runtime recorded it (question 10).

| Capability | Decided | Where |
|---|---|---|
| Errors point at `.jadpo` | Always `.jadpo` file and line, in development and production | Runtime; question 1 |
| Production incidents | Names the line in that build, then where it is now | `jadpo incident`; question 2 |
| Breakpoints | Not now | Question 3 |
| Why the language decided something | `jadpo why` | Question 4 |
| What a request did | Request timeline in `jadpo dev`; `jadpo trace <request id>` | Question 5 |
| Watching code run | `jadpo watch` waits for a named part of the code to run and prints it | Question 15 |
| Provider calls in development | `jadpo calls`, every call logged with full values, development only | Question 16 |
| Traces, metrics and logs in development | `jadpo traces`, `jadpo metrics`, `jadpo logs`, filterable, `--follow`, `--json` | Question 17 |
| Failing request to test | `jadpo test new --from <request id>` | Question 6 |
| Run a query or action | `jadpo call <name> --as <user>` | Question 7 |
| Developer console and MCP | Local web page and read-only MCP over the same evidence | Roadmap E11; question 8 |
| Reviewing a change | Report ordered by behaviour first; static web view of it | Roadmap E06; question 9 |
| JSON for agents | Every command and view | Area 14 question 2; question 10 |
| Coverage by path | Tests and production compared path by path | Question 11 |
| Production in review | The approval report shows traffic and failures on affected paths | Question 12 |
| Replaying a production request | Opt-in recording at effect boundaries, replayed locally | Question 13 |
| Query cost of a change | Queries added or removed per path, in the approval report | Question 14 |

## Errors point at the `.jadpo` line

Every stack, log event and error response that names a place names a
`.jadpo` file and line, never the generated code. This is always on, not
behind a setting (question 1). The compiler records where every piece of
generated code came from, and the runtime uses that to rewrite each frame
before logging, on the native Rust target and on WebAssembly alike (decision
register, 2026-10-13), so a fault reads:

```text
{"event":"operation.failed","classification":"int_overflow",
 "request_id":"req_0b1e…","build":"src_2c78570a2841d2f7",
 "stack":[
   "domains/ordering/actions/price_line.jadpo:4  action price_line",
   "domains/ordering/actions/place_order.jadpo:12  action place_order",
   "domains/ordering/routes/orders.jadpo:8  route POST /orders"]}
```

Frames inside the runtime's own code show as `runtime: <component>` rather
than a generated-code path, so a reader knows the fault is the compiler's, not
the application's. Domain failures still log no stack, only their `site` and
path (decided in area 4). Frames carry places and names, never values.

The runtime maps frames itself rather than relying on the host, because
WebAssembly hosts differ in how much debug information they keep. Mapping in
the runtime works the same under any host.

## Production incidents after the code has changed

A production event names its build and its `.jadpo` frames. `jadpo incident`
takes that event and answers two things (question 2):

1. **Where it happened**, in the source of that build: file, line and the
   lines around it. `jadpo build` records the git commit in
   `build/app.meta.json`, and the runtime logs it with every event, so the
   command can read the old source from git. *Decided 2026-10-13:* yes, as recommended.
2. **Where that is now**: the same declaration on the current branch,
   matched by its stable identity (area 5's identity registry, extended to
   actions, queries and routes), or "changed", "removed" or "split", stated
   rather than guessed.

```text
$ jadpo incident event.json
int_overflow in action price_line
  then   domains/ordering/actions/price_line.jadpo:4   (build src_2c78…, commit 8d113e6)
  now    domains/ordering/actions/price_line.jadpo:6   (changed since: 2 commits)
  path   POST /orders → place_order → price_line
  next   capture it as a test: jadpo test new --from-event event.json
```

## Breakpoints and stepping

Stepping through `.jadpo` code in an editor needs the debugger to map the
generated Rust or WebAssembly back to `.jadpo` lines, which Rust's and
WebAssembly's debug information do not do for a source language they did
not compile. The recommendation is not to promise this now (question 3):
agents cannot use breakpoints, and the timeline, `jadpo why` and captured
tests below answer the same questions in a form both people and agents can
read. Revisit once the Rust and WebAssembly targets exist.

## `jadpo why`: why the language decided something

Much of what surprises people in Jadpo is a decision the language made for
them: policy hid a record, a lifecycle filtered it, a subscriber did not
run, a cache answered. `jadpo why` explains one such decision from the
compiler's facts and the recorded evidence, and says when it does not know
(question 4). It takes a request, event or delivery id, or a record and a
caller:

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

Each answer ends with the `.jadpo` place that made the decision. Locally it
reads `jadpo dev`'s evidence and the local database; for production it reads
an exported log event, as `jadpo incident` does, and so can only explain what
the logs recorded.

## The request timeline in `jadpo dev`

`jadpo dev` records every request it serves and prints one line for each,
with its request id (question 5). `jadpo trace <request id>` shows the
whole timeline: route, caller, actions and queries with their SQL, rows and
timings, writes, events emitted, subscribers run, integration calls, and the
outcome. This is Laravel Telescope's request view and Rails' development log
with the query's source line, built from the spans area 16 already
generates.

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

- SQL is shown with placeholders. Parameters follow area 4's rule for
  internal fields: identifiers, numbers, enums, booleans and times as
  values, text as a keyed hash.
- The last 500 timelines are kept in `build/dev/`, which is generated and
  never committed. Production gets the same spans through OpenTelemetry
  (area 16), viewed in whatever tool the team uses.
- A request that runs several queries for one call, or a query without an
  index, is flagged on its line, matching area 6's query report.
- Provider calls appear in the timeline with where their answer came from
  (`recording`, `generated`, `scripted` or `sandbox`), and their inputs and
  answers in full ("Provider calls in development", below). An action with
  several changing calls shows each step's state: recorded, sent, answered,
  replayed, in doubt (area 12).

## Watching code run: `jadpo watch`

Decided 2026-10-16 (Richard): an agent should be able to wait for a
particular part of the code to run, then see its inputs and outputs,
without reading whole logs.

```text
jadpo watch action shipping.book_collection --once --json
jadpo watch domain billing
jadpo watch call couriers.bookings.create --failed
jadpo watch file domains/shipping/actions/book_collection.jadpo
```

- **Filters name code, as the compiler knows it**: a route, action, query,
  subscriber, schedule, provider operation, domain or file, optionally
  narrowed to `--failed` or `--faulted`. An unknown name is an error naming
  the nearest match, as everywhere else.
- **Each match prints the step** with its inputs, its output or failure,
  the queries, writes, events and calls inside it, timings, and the
  request id, so `jadpo trace <request id>` opens the whole request.
- **`--once` exits after the first match**, so an agent starts it in the
  background, triggers the flow, and waits for the command to finish
  instead of polling; `--count <n>` and `--timeout <duration>` bound it,
  and `--json` gives the same as data.
- **Development only, with values in full**, as the development call log
  shows provider payloads, from `jadpo dev`'s git-ignored data; secrets are
  never shown. Production keeps area 16's spans with no values.

## Provider calls in development: `jadpo calls`

Decided 2026-10-16 (Richard). In `jadpo dev` every provider operation is
faked by default, answered from the package's recordings where there are
some and from generated defaults otherwise, so nothing leaves the machine
and no credential is needed (area 11). Operations or resources named on
the connection's `development: { sandbox: [...] }` go to the provider's
sandbox, never live; `jadpo dev --fake <connection>` and `--sandbox
<operation>` change one session only.

```text
$ jadpo calls
call_31  stripe.customers.create         generated   201   4 ms   req_0c02
call_32  stripe.payment_intents.create   scripted    card_declined   1 ms   req_0c02

$ jadpo calls script stripe.payment_intents.create card_declined
next call to stripe.payment_intents.create answers card_declined
```

- **Every call is logged**, faked or real: the connection, resource and
  operation, the input and the answer or failure in full, where the answer
  came from (`recording`, `generated`, `scripted`, `sandbox`), attempts and
  retries, duration, and the request id and span, so each appears in
  `jadpo trace` beside the queries.
- **`jadpo calls`** lists them (`--connection`, `--operation`,
  `--request`), **`jadpo calls show <id>`** prints one, and both take
  `--json`. They are kept with the last 500 timelines in `build/dev/`,
  never committed.
- **`jadpo calls script <operation> <failure>`** sets the next answer, in
  area 13's `fakes:` form, so an agent can walk a failure path by hand.
  Fakes keep no state, so a flow that creates and then reads one object
  either lets both through to the sandbox or scripts the read.
- **Full values in development only.** The log shows inputs and answers in
  full, which changes this page's rule of hashing text for call payloads
  (question 5) in development only; credentials and `secret: true` values
  are never shown. Production keeps area 16's spans with no values.
- `jadpo dev grants add <connection> --owner <id>` makes a fake OAuth grant
  for an owner (area 11).

> **Built (chunk 7.2d)**, but for `jadpo trace`, `jadpo watch call`, the
> request timelines and recordings (chunk 7.2d2): `jadpo dev` logs each
> provider call as it ends in `build/dev/calls.jsonl`, and `jadpo calls`
> reads it. The list, plain, after a booking answered with the generated
> default and one scripted to find its slot taken:
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
> In a terminal the answers are `✓ answered` and `✗ slot_taken` in their
> colours, and the footer reads `✓ 2 calls  1 answered · 1 failed`. One
> call in full, its request as made and its answer laid out a member a
> line:
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
> The request id in the list is cut as `jadpo dev`'s request lines cut
> it, and `--request` takes any start of it. With no calls yet the list
> says `No calls yet` and points at `jadpo dev`; with filters that match
> none, `No calls match` and the filters. The choices are area 11's
> "Chosen in chunk 7.2d".

## Traces, metrics and logs in development

Decided 2026-10-16 (Richard). In development the app's telemetry (area 16)
stays on the machine unless `jadpo dev --export-telemetry` sends it on,
and is read from the command line, as the call log is:

| Command | Shows |
|---|---|
| `jadpo traces` | Recent requests, schedule runs and deliveries, one line each, opening into `jadpo trace <id>` |
| `jadpo metrics` | The metrics the runtime records and the app's own (`monitoring.jadpo`), per route, action or path |
| `jadpo logs` | The app's logs, its services' included (area 11), by level |

Each takes the same filters, by route, action, domain, path, failure and
slowness (`--slower-than 200ms`), `--follow` to keep printing as new ones
arrive, and `--json`. The developer console shows the same data later.
Their screens follow the CLI design language and are designed with the
design skills before they are built, as `jadpo watch`'s and `jadpo
calls`' are.

## Capturing a failing request as a test

The quickest reliable fix is: reproduce, then fix. `jadpo test new --from
<request id>` writes a route test that replays a request recorded by
`jadpo dev`, with the data it read written as fixtures (question 6). The
test asserts what was observed; you change the assertion to what should
happen, watch it fail, then fix the code. An agent can do the whole loop.

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

- Fixtures go in the domain's `fixtures/` folder (area 1) and create records
  through the entity's generated operations.
- Fields marked `personal:` (area 15) are replaced with generated values,
  so a captured test never commits real personal data.
- Integration calls become `fakes:` with the responses that were received.
- `--from-event <file>` does the same from a production event, using only
  what the event recorded; the fixtures are then left for the author to
  fill in.

## Running a query or action: `jadpo call`

Rails has `rails console`; Jadpo proposes a one-shot command instead
(question 7). `jadpo call` runs one query or action against the local
database as a named caller, with JSON in and out, so an agent can script it.

```text
$ jadpo call todos_for_owner --as 0192…a7 --input '{"owner_id":"0192…a7"}'
{"items":[{"id":"0192…f1","title":"Buy milk"}],"next":null}

$ jadpo call complete_todo --as 0192…a7 --input '{"todo_id":"0192…f1"}' --rollback
{"id":"0192…f1","completed_at":"2026-10-12T09:20:00Z"}
rolled back
```

- **Policy always applies.** `--as <user id>` signs in as that user through
  area 8's generated principal; with no `--as` the caller is public. There
  is no switch that skips policy, so what works here works through a route.
- **Writes are allowed, locally.** `--rollback` runs inside a transaction
  and rolls it back (Rails' `--sandbox`). Like seeds (area 5), `jadpo call`
  refuses any environment marked production.
- Every call is recorded in the timeline, so `jadpo trace` and `jadpo why`
  work on it.

## The developer console and MCP

The same evidence, as a web page and as tools for an agent (question 8;
roadmap E11).

- **Developer console.** `jadpo dev` serves a local page on its own port
  (Encore's local dashboard is the model): recent requests and their
  timelines, errors grouped by place, slow routes and queries, schedule
  runs and deliveries with their attempts (area 12's states), and the
  reports from `build/report/` (permissions, personal data, schema,
  queries).
- **Application graph.** The compiler's call graph (route → action → query
  → entity, event → subscriber), drawn per domain and opened one level at a
  time, with each node linked to its source and `///` description. In
  `jadpo dev`, live overlays colour nodes by recent traffic, failures and
  time spent, and a selected request lights up its path.
- **MCP server.** `jadpo mcp` exposes the same evidence as read-only tools:
  `trace`, `why`, `incident`, `impact`, `sql`, `routes`, the reports and the
  graph. It cannot write data, retry deliveries or edit code; those stay
  commands a person or agent runs deliberately.

The console is read-only. Retrying or dismissing a delivery stays area 14's
`jadpo deliveries` command, which the page shows you how to run. Production
monitoring, including any hosted service, is later product work; the
local console comes first.

## Reviewing a change

A reviewer should meet a model's change in the order it matters (question
9; roadmap E06). `jadpo approval --before <ref>` writes the report in that
order, as Markdown and JSON, and a static web view of the same report:

1. **Decisions that need a person.** Each change that needs an acceptance
   (area 7's form): who can now do what to which data, in plain words
   ("viewers can now read `Project.budget`"), with whether its acceptance
   file exists in `acceptances/`.
2. **Paths.** For each decision, the actor → route → action or query →
   field → output path that makes it true, expandable into its surroundings. *Decided 2026-10-13:* yes, as recommended.
3. **Newly reachable code.** Declarations that did not change but are now
   reached from a new place, which a line diff never shows. *Decided 2026-10-13:* yes, as recommended.
4. **Other behaviour.** API record changes (area 7), schema and migrations
   (area 5), failures, events and subscribers, integrations, and the
   permissions and personal-data reports' differences (areas 9 and 15). *Decided 2026-10-13:* yes, as recommended.
   Packages add to the first item, as decisions that need a person
   (decided 2026-10-16): a third party's package installed from git
   rather than the registry, marked **unverified** until a person accepts
   it; a **package's service** whose image is new or changed, shown with
   its image, digest and the hosts it asks for, which never starts until
   accepted; a new outbound host granted to a service; a connection that
   receives personal data but cannot erase it (area 15); and bot checks
   allowed to pass when their provider is down (area 16).
   With `translations: { review: TranslationReview.required }`, wording
   changed and not yet accepted with `jadpo translations accept <locale>`
   is listed here too (area 10, decided 2026-10-16); until `jadpo approval`
   exists, `jadpo translations missing` lists it (chunk 5.4d).
5. **Source,** grouped by declaration rather than by file.

```text
$ jadpo approval --before main
1 decision needs a person
  viewers can now read Project.budget          acceptance missing
    viewer → GET /projects/{id} → project_detail → ProjectView.budget
2 declarations newly reachable
  budgets.convert_currency   now reached from GET /projects/{id}
Other changes: 1 route added, 1 migration, no failure codes changed
Source: 6 declarations in 4 files
```

The web view adds what text cannot: the graph of the change, an interdiff
between an agent's successive revisions (as Revset does), and links from
every line to its source. It is a file in `build/review/`, opened locally
or published by CI beside the pull request; it holds no accounts and
records no approvals. Acceptances stay files reviewed on the git host
(area 7).

## What the graph and production evidence add

Proposed 2026-10-13. Each of these joins the compiler's graph to path IDs
(area 16 question 13). Production data is always an optional input: a
file exported from the monitoring tool, or a service that holds it. Every
command works without it.

- **Coverage by path** (question 11). `jadpo test` reports which paths the
  tests ran, not which lines. Given production path counts, it lists paths
  that ran in production with no test, ordered by traffic, and `jadpo test
  new --from` writes the missing one. Paths in the graph never seen in
  production for 90 days are listed as candidates for removal.
- **Production in review** (question 12). Given production path counts,
  the approval report says what a change touches in use: "touches paths
  that served 38% of requests last week; one failed 12 times". No other
  tool has both the meaning of a change and per-path traffic.
- **Replaying a production request** (question 13). Time, randomness,
  integrations and the database are declared effects, so the runtime knows
  every source of nondeterminism. When opted in, it records what each
  effect returned for a failing request, with personal fields replaced
  using area 15's marks, and `jadpo replay` runs that request locally with
  the same results. This extends `test new --from` from `jadpo dev` to
  production.
- **Query cost of a change** (question 14). The compiler counts the
  queries on every path, so the approval report says "adds 2 queries to
  `POST /orders`", the hottest path when production counts are given.

## Everything as JSON

Every command on this page takes `--json` (area 14 question 2), and each
kind of evidence has one versioned schema: the fault event, the request
timeline, `why` answers, the graph and the review report. The console and
MCP read those same files, so an agent never scrapes text meant for people
(question 10).

A value in a report (a failed assertion, a failure's fields, a fault, a
timeline and their `--json`) is written as code makes it,
`money.from_minor(1050, Currency.gbp)`, `2.5262metre`, `"2026-10-25"`,
`PostalAddress { ... }`: it keeps a standard type's currency, unit and
every place, and an agent can paste it into a test. A report is read by
people and agents and never read back as data, so it is not one of area
19's boundaries, and a personal value in one follows area 15's rules
(decided by the lead session, 2026-10-18, under Richard's instruction of
2026-10-17 that no standard type loses its currency, unit or precision;
area 19, "Every boundary through the contract").

## Review

### Status

Almost nothing on this page is built yet. Mapped `.jadpo` stacks, `jadpo incident`,
`why`, timelines, `test new --from`, `call` and the approval report come
after the first build (roadmap, areas 17 and 18); the developer console, MCP
and the review web view come after that, as product work (E11 and E06).
Since chunk 6.2b, `jadpo why <schedule>` answers why a schedule didn't run
(area 12, "Watching schedules"); its other forms come with this page.

### Consistency with the rest of the language

- **Values never, places always.** Stacks, timelines and `why` answers follow
  area 4's log rule and area 16's span rule. The exceptions are all local
  to `jadpo dev`: the timeline kept for `test new --from`, which holds the
  full request, and, since 2026-10-16, the development call log and
  `jadpo watch`, which show values in full. All live in `build/dev/` on the
  developer's machine, never logged, committed or exported, and never show
  secrets.
- **`jadpo call` and `jadpo calls`.** One runs a query or action; the other
  lists provider calls. The names are close; `jadpo help` gives both.
- **Concealment.** `jadpo why` says a record was concealed by policy, which
  the client must never learn (area 4). It reads local evidence or logs the
  operator already has, so this reveals nothing to a client.
- **A caller from outside a route.** `jadpo call --as` makes a principal
  outside a route, which area 8 keeps out of application code. Like tests
  and seeds, it is a tool, not code, and goes through the generated
  principal.
- **Read-only screens.** Area 16 left "background job dashboards" to area
  14's commands. The console shows deliveries but acts only through those
  commands, so there is still one way to change them.
- **Sign-off.** The archived plan had the review UI validate signed
  approvals. Area 7 decided acceptances are files owned in CODEOWNERS and
  area 14 recommends no Jadpo accounts, so the review view only reads.
- **A new folder in `build/`.** `build/dev/` and `build/review/` join area
  14's list; `build/` stays generated and uncommitted.

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Compiler writes a source map; runtime logs `.jadpo` frames; `JADPO_DEBUG_TARGET_STACKS` removed | 4 | Question 1 |
| Stable identity for actions, queries and routes, beside the schema identity registry | 5 | Question 2 |
| Commit recorded in `app.meta.json`; `jadpo why`, `trace`, `call`, `test new --from`, `mcp`; `build/dev/` and `build/review/` | 14 | Questions 2 and 4–8 |
| The approval report's order and format | 14 | Question 9 (area 14 question 12 keeps sign-off) |
| Captured tests: fixtures, `fakes:`, personal fields replaced | 13 | Question 6 |
| The developer console and MCP server; application graph and overlays; production monitoring | Roadmap E11 | Question 8 |
| The review web view; interdiff between agent revisions | Roadmap E06 | Question 9 |
| Breakpoints in `.jadpo` on the Rust and WebAssembly targets | Roadmap | Question 3 |
| Path IDs and per-path query counts | 16 | Questions 11–14 |
| Path coverage in `jadpo test`; production counts as a test input | 13 | Question 11 |
| What a replay recording may hold | 15 | Question 13 |
| `jadpo watch`, `calls`, `traces`, `metrics` and `logs`, and `jadpo dev --export-telemetry`, `--fake`, `--sandbox` and `grants add`, in the command reference | 14 | Questions 15 to 17 |
| Unfinished multi-step runs and `diverged` deliveries as operator items, in `jadpo deliveries` | 12, 14 | Question 15 |
| Unverified publishers and service approvals in the approval report | 11, 18 | Question 9 |

### Open questions

1. **Do errors and stacks always point at `.jadpo` lines?** The archived
   compiler printed only TypeScript stacks, behind
   `JADPO_DEBUG_TARGET_STACKS`. *Recommendation:*
   yes, always: the compiler writes a source map, the runtime maps its own
   frames before logging (on every target), runtime-internal frames show as
   `runtime: <component>`, and the flag goes. Rust prints a panic's file and
   line by default; a language whose generated code nobody reads must too.
2. **How does a production incident map back after the code changed?**
   *Recommendation:* the event carries the build and commit; `jadpo incident`
   shows the line in that commit's source, then where the same declaration
   is now, by stable identity, saying "changed", "removed" or "split" when
   it cannot match. This is Sentry's release-plus-source-map model without a
   hosted service.
3. **Should Jadpo offer breakpoints and stepping in `.jadpo` now?**
   *Recommendation:* no, not now. Agents cannot use breakpoints, and the
   Rust and WebAssembly targets do not exist yet. Ship mapped stacks,
   timelines, `why` and captured tests first; revisit stepping on the new
   targets.
4. **Is `jadpo why` one command for every language decision?**
   *Recommendation:* yes: `jadpo why <request|event|delivery id>` and
   `jadpo why <Entity> <id> --as <user>`, covering policy (which grant lists
   were checked against which facts), lifecycle, concealment, cache use and
   subscriber runs, each ending with the `.jadpo` place responsible. No
   framework has this; it is possible because the compiler owns these rules.
5. **Does `jadpo dev` keep a request timeline?** *Recommendation:* yes: one
   line per request, the last 500 timelines in `build/dev/`, `jadpo trace
   <id>` for one, SQL with placeholders and parameters under area 4's rule.
   Telescope, Rails' query log and Encore's dashboard all show this locally,
   and the spans already exist (area 16). *Decided 2026-10-13:* yes, as recommended.
   *Revised 2026-10-16 (Richard):* provider calls in the timeline show
   their inputs and answers in full in development only, and where each
   answer came from; production is unchanged.
6. **Does `jadpo test new --from <request id>` write a replay test?**
   *Recommendation:* yes: a route test with fixtures for the data read,
   fakes for integration calls, personal fields replaced, asserting what
   was observed so the author edits it to the expected outcome. It turns
   "reproduce first" into one step an agent always takes. *Decided 2026-10-13:* yes, as recommended.
7. **Interactive console or one-shot `jadpo call`; may it write, and does
   policy apply?** *Recommendation:* one-shot `jadpo call <name> --as <user>`
   with JSON in and out; writes allowed locally, `--rollback` like Rails'
   `--sandbox`, production refused; policy always applies, no bypass. An
   interactive console can come later on the same mechanism. *Decided 2026-10-13:* yes, as recommended.
8. **What are the developer console and MCP server?** *Recommendation:*
   local and read-only: a page served by `jadpo dev` (requests, errors, slow
   routes, deliveries, reports, the application graph with live overlays)
   and `jadpo mcp` with the same evidence as read-only tools. Acting stays
   in commands; production stays with OpenTelemetry tools. Both are roadmap
   E11 product work, after the evidence they read exists. *Decided 2026-10-13:* yes, with the CLI first. Agents that can run
   commands (Claude Code, Codex, Cursor) use `jadpo` with `--json` and
   `jadpo help --json`; that is enough for building. The MCP server comes
   later for agents with no shell, mainly investigating production from a
   chat or hosted tool, generated from the same command reference so it can
   never drift, read-only, and protected with the application's own OAuth
   (area 8).
9. **What does a reviewer see first?** *Recommendation:* `jadpo approval
   --before <ref>` ordered as decisions needing a person, the paths that make
   each true, newly reachable unchanged code, other behaviour, then source by
   declaration; Markdown and JSON, plus a static web view with the change
   graph and agent-revision interdiffs, published by CI. It records no
   approvals: acceptances stay in `acceptances/` (area 7). The web view is
   roadmap E06. *Decided 2026-10-13:* yes, as recommended.
10. **What is decided in this review, and in what order is it built?**
    *Recommendation:* the contract is the evidence: `.jadpo` locations,
    build and commit identity on every event, the versioned JSON for faults,
    timelines, `why`, the graph and the review report, and the commands
    `why`, `trace`, `call` and `test new --from`. The console, MCP server,
    review web view and any breakpoints are product work. Build order:
    mapped stacks, the timeline and `trace`, `why`, `test new --from`, `call`,
    the review report's order, MCP, the console, then the review web view. *Decided 2026-10-13:* yes, as recommended.
11. **Is test coverage measured by path, and compared with production?**
    *Recommendation:* yes: paths run by tests, paths run in production with
    no test ordered by traffic, and paths unseen for 90 days. Line coverage
    says a branch ran once; path coverage says which combinations ran. *Decided 2026-10-13:* yes, as recommended.
12. **Does the approval report use production counts when given them?**
    *Recommendation:* yes, as an optional input; the report is complete
    without it. It is the one view nobody else can build, because nobody
    else has the meaning of the change. *Decided 2026-10-13:* yes, as recommended.
13. **Can a production request be recorded and replayed locally?**
    *Recommendation:* yes, opt-in per deployment, failing requests only,
    personal fields replaced by area 15's marks, recordings expiring like
    logs. Record-and-replay is hard in most languages because effects are
    everywhere; in Jadpo they are declared. *Decided 2026-10-13:* yes, as recommended.
14. **Does the approval report count queries per path?** *Recommendation:*
    yes, from the compiler alone, ranked by production traffic when given.
    It catches the extra query before it ships, not on a dashboard after. *Decided 2026-10-13:* yes, as recommended.
15. **Can an agent wait for a part of the code to run?** *Decided
    2026-10-16 (Richard):* `jadpo watch` with filters naming a route,
    action, query, subscriber, schedule, provider operation, domain or
    file, narrowed to failures or faults; each match prints the step's
    inputs, output or failure, what ran inside it, timings and the request
    id; `--once`, `--count`, `--timeout` and `--json`; development only,
    values in full, secrets never. Each step of an action with several
    changing calls shows its state, and an unfinished one is an operator
    item (area 12).
16. **What does development show of provider calls?** *Decided
    2026-10-16 (Richard):* every call faked by default and logged, faked
    or real, with inputs and answers in full (development only), source,
    attempts, duration and request, in `jadpo calls`, `jadpo calls show`,
    `jadpo trace` and `--json`; `jadpo calls script` sets the next answer;
    the connection's `development: { sandbox: }` lets named calls through
    to the sandbox.
17. **How are traces, metrics and logs read in development?** *Decided
    2026-10-16 (Richard):* nothing is exported unless `jadpo dev
    --export-telemetry`; `jadpo traces`, `jadpo metrics` and `jadpo logs`
    read them locally, filterable by route, action, domain, path, failure
    and slowness, with `--follow` and `--json`, and the developer console
    shows them later.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Logging | Carried from area 4: one structured event per failure or fault; internal values by area 4's rule; no stack for domain failures | 2026-10-12 |
| Request id | Carried from area 4: every error response and every failure log carries `request_id` | 2026-10-12 |
| Acceptances | Carried from area 7: breaking changes, drops, policy weakening and new domains need a person-written file in `acceptances/`, owned in CODEOWNERS | 2026-10-12 |
| Reading the caller | Carried from area 8: `current_principal` in routes only | 2026-10-12 |
| Permissions report | Carried from area 9: `build/report/permissions.md` and `.json`, shown in the approval review | 2026-10-12 |
| `.jadpo` locations | Always `.jadpo` file and line on every target; the runtime maps its own frames; internal frames as `runtime: <component>` | 2026-10-13 |
| Production incidents | Events carry build and commit; `jadpo incident` shows the line then and now, or changed, removed or split | 2026-10-13 |
| Breakpoints | Not now; revisit on the Rust and WebAssembly targets | 2026-10-13 |
| `jadpo why` | One command explaining policy, lifecycle, concealment, cache use and subscriber runs, ending at the responsible `.jadpo` line | 2026-10-13 |
| Request timeline | Last 500 requests in `jadpo dev`; `jadpo trace <id>` | 2026-10-13 |
| Tests from requests | `jadpo test new --from <request id>` writes a replay test | 2026-10-13 |
| `jadpo call` | One-shot `jadpo call <name> --as <user>`, JSON in and out, local writes with `--rollback`, production refused, policy always applies | 2026-10-13 |
| Developer console and MCP | Local, read-only console in `jadpo dev`; CLI first for agents; MCP later for agents without a shell, generated from the command reference, read-only, OAuth-protected | 2026-10-13 |
| Review report and view | `jadpo approval` ordered decisions first; Markdown and JSON; the graph web view later (E06); acceptances stay files | 2026-10-13 |
| Contract and order | The evidence and commands are decided; console, MCP, review view and breakpoints are product work; build order as listed | 2026-10-13 |
| Coverage by path | Paths run by tests, untested production paths by traffic, paths unseen for 90 days | 2026-10-13 |
| Production in review | Optional production counts in the approval report | 2026-10-13 |
| Production replay | Opt-in, failing requests only, personal fields replaced, recordings expire | 2026-10-13 |
| Query cost of a change | Queries per path computed by the compiler in the approval report | 2026-10-13 |
| Unaccepted translations | With `TranslationReview.required`, changed wording not yet accepted appears in the approval report (Richard, area 10) | 2026-10-16 |
| Watching code run | `jadpo watch` on a named route, action, query, subscriber, schedule, provider operation, domain or file, optionally failures or faults only; prints inputs, outputs, what ran inside and the request id; `--once`, `--count`, `--timeout`, `--json`; development only, values in full, secrets never (Richard) | 2026-10-16 |
| Development call log | Provider calls faked by default, let through to the sandbox by name; every call logged with full inputs and answers in development only, its source, attempts, duration and request; `jadpo calls`, `calls show`, `calls script`, `--json` (Richard) | 2026-10-16 |
| Telemetry in development | Nothing exported unless `--export-telemetry`; `jadpo traces`, `metrics` and `logs`, filterable by route, action, domain, path, failure and slowness, `--follow`, `--json` (Richard) | 2026-10-16 |
| Packages in the approval report | Unverified third-party packages from git, new or changed service images, outbound grants for services, connections that cannot erase and fail-open bot checks need a person's acceptance (Richard) | 2026-10-16 |
