---
title: Testing
area: 13
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Testing
<!-- topic: testing -->

A Jadpo test runs your real application against a real database. Only
the outside world and the clock are replaced: every call to an outside
service answers from a fake, and the clock stands still until the test
moves it. You don't mock your own actions, and there is no in-memory
stand-in for the database.

This page covers where tests live, the kinds of test, the settings a test
takes, acting as a user, controlling time, configuration and fakes in
tests, shared test data, assertions, running tests, the test database,
and how tests meet caches and events.

Most examples use a todo app's `planning` domain, the folder that holds
one business area of the app. It has `User` and `Todo` entities (kinds of
stored record), a `create_todo` action, a `TodoNotFound` failure, an
`overdue_reminders` schedule and a `ReminderMail` integration with an
outside email service.

## Where tests live
<!-- topic: where-tests-live -->

A test sits beside the file it tests, in a file named
`<name>.test.jadpo`. There is no folder per kind of test. Here the
`planning` domain's tests sit beside its action, route and schedule, and
one scenario that spans several domains sits in the root `tests/`
folder:

```text
domains/planning/
  actions/     create_todo.jadpo        create_todo.test.jadpo
  routes/      todos.jadpo              todos.test.jadpo
  schedules/   overdue_reminders.jadpo  overdue_reminders.test.jadpo
  fixtures/    alice_with_todos.jadpo
tests/         sign_up_to_first_reminder.test.jadpo
```

A test file holds only `test` declarations.

### A test beside its subject

A test beside the file it tests can call anything in its own domain,
private or not. It sees everything its subject declares without importing
it, and brings in anything else it uses on `use` lines, as any file does
([Imports](01-project-layout-and-modules.md#imports)).

### A scenario in `tests/`

A test that spans several domains, called a scenario, goes in the root
`tests/` folder. It sees only what another domain would see: public
declarations and routes. It imports everything it uses. So unit tests stay
close to their code, and scenarios can't depend on a domain's internals.

## Writing a test
<!-- topic: writing-a-test -->

A test is a sentence that names it, then a body of ordinary code. In the
body, `assert` fails the test when its condition is false. Here a test
checks a function that says whether a todo is overdue:

```jadpo check project=writing-a-test file=app.jadpo hidden
application {}
```

```jadpo check project=writing-a-test file=domains/planning/types/due_at.jadpo hidden
type DueAt = Instant {}
```

```jadpo check project=writing-a-test file=domains/planning/types/overdue.jadpo hidden
type Overdue = Bool {}
```

```jadpo check project=writing-a-test file=domains/planning/libraries/due_dates.jadpo hidden
use planning/types { DueAt, Overdue }

library due_dates {
    /// Whether a todo due at `due` is overdue at `now`.
    function is_overdue(due: DueAt, now: DueAt) -> Overdue {
        return Overdue(due < now)
    }
}
```

```jadpo check project=writing-a-test file=domains/planning/actions/remind_if_overdue.jadpo hidden
use planning/types { DueAt, Overdue }
use planning/libraries { due_dates }

action remind_if_overdue(due: DueAt, now: DueAt) -> Overdue {
    return due_dates.is_overdue(due, now)
}
```

```jadpo check project=writing-a-test file=domains/planning/libraries/due_dates.test.jadpo
// domains/planning/libraries/due_dates.test.jadpo
use planning/types { DueAt }

test "a todo due yesterday is overdue" {
    var due: DueAt = "2026-01-14T12:00:00Z"
    var now: DueAt = "2026-01-15T12:00:00Z"
    assert due_dates.is_overdue(due, now)
}
```

`due_dates` is a library in the `planning` domain, and `is_overdue` is one
of its functions. `DueAt` is the domain's type for a due date, an
`Instant`. The test sits beside the library's file, so it calls the
library without importing it.

### The sentence names the test

A test's sentence is its name, and it is used once in the whole project.
A second test with the same sentence is `semantic.duplicate_declaration`.
Jadpo's records know a test by the file it sits beside and its sentence
([Stable identity](18-records-and-manifest.md#stable-identity)).

### Each test starts from nothing

Each test gets its own empty database, its own clock and its own fakes.
So tests never depend on each other, and they can run in any order.

## Kinds of test
<!-- topic: kinds-of-test -->

Jadpo works out a test's kind from what it calls, so you don't declare it.
The report counts each kind separately, because each proves something
different: an action test doesn't prove that the route decodes input or
checks who is signed in. `jadpo test --kind route` runs one kind.

| Kind | The test calls | It proves |
|---|---|---|
| pure | functions only | the calculation |
| action | actions and queries directly | the behaviour and the data it writes |
| route | a route, through `request` | the whole HTTP boundary: input, sign-in, policy, failures, output |
| schedule | a schedule, through `.run()` | what one run of the schedule does |

A test that calls actions to set up its data and then makes a request is a
route test. A fifth kind, recording tests, has no `test` declaration; it
is described at the end of this topic.

### Action tests

An action test calls actions and queries directly. Calling an action is an
ordinary call, with no `call` word.

Only actions write records
([The operations every entity gets](06-reads-and-writes.md#the-operations-every-entity-gets)).
So a test changes data through the action that does it, which is what an
action test proves, and a fixture builds the data it starts from
([Shared test data: fixtures](#shared-test-data-fixtures)). A test may
read straight from an entity with `get`, as it may call a query, and
either makes it an action test with its own database. A write straight to
an entity is `semantic.not_callable`:

```jadpo check project=e-test-writes file=app.jadpo hidden
application {}
```

```jadpo check project=e-test-writes file=domains/shop/types/count.jadpo hidden
type Count = Int {
    min: 0
}
```

```jadpo check project=e-test-writes file=domains/shop/entities/shelf.jadpo hidden migrate=initial
use shop/types { Count }

entity Shelf {
    id: Uuid
    visits: Count
}
```

```jadpo check project=e-test-writes file=domains/shop/entities/shelf.test.jadpo expect=semantic.not_callable
test "a shelf counts its visits" {
    var shelf: Shelf = attempt Shelf.update("0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e") {
        set: {
            visits: 3
        }
    }
    assert shelf.visits == 3
}
```

```text output
error: A test can't call `Shelf.update`
 --> domains/shop/entities/shelf.test.jadpo:2:32
  |
2 |     var shelf: Shelf = attempt Shelf.update("0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e") {
  |                                ^^^^^^^^^^^^ writes a record
  reason: Only actions write records (area 6), so a test changes data through the action that does it, which is what it proves; a fixture, a test-only action, builds the data a test starts from. A test may still read, with `Shelf.get` and queries.
  next: Call the action that writes `Shelf`, or move the write into a fixture (agent)
  rule: semantic.not_callable
  help: https://jadpo.dev/docs/errors/semantic.not_callable

check failed: 1 error, 4 files
```

### Route tests

A route test sends a typed request with `request { }`. The request goes
through the real HTTP layer Jadpo generates for your routes, without a
network:

```jadpo fragment
// domains/planning/routes/todos.test.jadpo
test "a due date in the past is rejected" {
    clock: "2026-01-15T12:00:00Z"

    var alice: Principal.user = alice_with_todos()
    var response: Response = request {
        method: HttpMethod.post
        path: "/todos"
        as: alice
        input: CreateTodo {
            title: "Late"
            due_at: "2026-01-15T11:59:59Z"
        }
    }
    assert response.status == 422
    assert response.code == "due_date_in_past"
}
```

The request names its route by `method:` and by its path as
`jadpo routes` prints it. Each placeholder in the path gets its value in
`params:`, as in `path: "/todos/{todo_id}"` with
`params: { todo_id: todo.id }`. `input:` is the route's `Input`. A
generated magic-link route takes the built-in input the API record names
([Magic links in tests](08-identity.md#magic-links-in-tests)). The
request goes through the runtime's HTTP component exactly as a client's
would, and carries your app's own `Origin`, as a browser's does.

`as:` signs someone in for the request, through the application's real
authentication: a browser session, an API token, an external token from
the test run's local issuer
([External tokens in tests](08-identity.md#external-tokens-in-tests-and-jadpo-dev))
or, for `Principal.service(service)`, a service key
([Signing in, end to end](08-identity.md#signing-in-end-to-end)). Leave
it out and the request is sent with no one signed in.

`request` gives a `Response`, which has:

- `status`, the HTTP status;
- `code`, the failure's code, or `none` on success;
- `output`, the route's typed output, or `none` unless it succeeded.

A route that streams a model's answer gives `pieces`, the text of each
event in order, in place of `output`. Its `code` is that of the error
event that ended the stream, if one did
([Streaming a model's answer](07-http.md#streaming-a-models-answer)). A
defect behind the response, which is a bug such as an overflow, ends the
test as `test.fault`, with its frames.

In this example the route lets anyone call it, so the test signs no one
in. `add_memo` reaches no entity, so the root `policy { }` says who may
call it ([Actions](09-policy.md#actions)):

```jadpo check project=route-test
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
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        status: SuccessStatus.created
        run: add_memo(input)
    }
}

policy {
    actions: {
        add_memo: [Access.public]
    }
}

test "a memo comes back as it was sent" {
    var response: Response = request {
        method: HttpMethod.post
        path: "/memos"
        input: AddMemo {
            text: "Call the plumber"
        }
    }
    assert response.status == 201
    assert response.code == none
    assert response.output == MemoView {
        text: "Call the plumber"
    }
}
```

### Idempotency keys

An idempotency key is a value a client sends with a write so that, if the
request is sent again, the write happens once. A route test sends a fresh
`Idempotency-Key` with every request but a `GET`, as a real client would.
So a route on an API with `idempotency: Idempotency.required`, which
refuses a write without a key, is tested through its success like any
other ([Idempotent writes](07-http.md#idempotent-writes)).

To test what a repeat gets back, or how reusing a key for another request
is refused, give the key yourself with `idempotency_key:`. The request
sends that key instead of a fresh one.

This API refuses a write without a key:

```jadpo check project=route-idempotency file=app.jadpo
application {
    apis: {
        main: {
            prefix: /
            idempotency: Idempotency.required
        }
    }
}
```

```jadpo check project=route-idempotency file=domains/notes/types/memo_text.jadpo hidden
type MemoText = Text {
    min_length: 1
    max_length: 200
}
```

```jadpo check project=route-idempotency file=domains/notes/types/memo_view.jadpo hidden
use notes/entities { Memo }

/// A memo as anyone sees it.
type MemoView = Output {
    pick: [Memo.id, Memo.text]
}
```

```jadpo check project=route-idempotency file=domains/notes/entities/memo.jadpo hidden
use notes/types { MemoText }

/// A note anyone may leave.
entity Memo {
    id: Uuid
    text: MemoText

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=route-idempotency file=domains/notes/actions/add_memo.jadpo hidden
use notes/entities { Memo }
use notes/types { MemoText, MemoView }

/// What leaving a memo takes.
type AddMemo = Input {
    text: MemoText
}

/// Leaves a memo.
action add_memo(input: AddMemo) -> MemoView {
    var memo: Memo = Memo.create {
        set: {
            text: input.text
        }
    }
    return MemoView {
        id: memo.id
        text: memo.text
    }
}
```

```jadpo check project=route-idempotency file=domains/notes/queries/all_memos.jadpo hidden
use notes/entities { Memo }
use notes/types { MemoView }

/// Every memo, by text.
query all_memos() -> List<MemoView> {
    from: Memo
    order: [Memo.text asc]
    limit: 50
}
```

```jadpo check project=route-idempotency file=domains/notes/routes/memos.jadpo migrate=initial hidden
use notes/types { MemoView }
use notes/actions { AddMemo, add_memo }
use notes/queries { all_memos }

/// Memos anyone may leave and read.
routes Memos {
    prefix: /memos

    /// Leaves a memo.
    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        status: SuccessStatus.created
        run: add_memo(input)
    }

    /// Every memo.
    GET / {
        authentication: RouteAuthentication.none
        output: List<MemoView>
        run: all_memos()
    }
}
```

```jadpo check project=route-idempotency file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

api main
    idempotency required

GET /memos
    authentication none
    output List<MemoView>
    status 200

POST /memos
    authentication none
    input AddMemo (body)
    output MemoView
    status 201

GET /openapi.json
    generated openapi
    authentication none
    status 200

input AddMemo
    text: Text { min_length: 1, max_length: 200 }

output MemoView
    id: Uuid
    text: Text { min_length: 1, max_length: 200 }
```

The first test sends no key of its own and is answered as any client
would be. The other two send `memo-1` twice. A repeat gets the first
answer back and leaves no second memo. The same key with another memo is
a 422 with the code `idempotency_key_reused`:

```jadpo check project=route-idempotency file=domains/notes/routes/memos.test.jadpo
use notes/actions { AddMemo }

test "a memo is left through its route, with a fresh key" {
    var response: Response = request {
        method: HttpMethod.post
        path: "/memos"
        input: AddMemo {
            text: "Call the plumber"
        }
    }
    assert response.status == 201
}

test "a memo sent again with its key is left once" {
    var first: Response = request {
        method: HttpMethod.post
        path: "/memos"
        idempotency_key: "memo-1"
        input: AddMemo {
            text: "Call the plumber"
        }
    }
    var resent: Response = request {
        method: HttpMethod.post
        path: "/memos"
        idempotency_key: "memo-1"
        input: AddMemo {
            text: "Call the plumber"
        }
    }
    assert resent.status == 201
    assert resent.output == first.output
    var on_file: Response = request {
        method: HttpMethod.get
        path: "/memos"
    }
    match first.output {
        some(memo) -> {
            assert on_file.output == [memo]
        }
        none -> {
            assert first.output != none
        }
    }
}

test "a key sent again with another memo is refused" {
    var first: Response = request {
        method: HttpMethod.post
        path: "/memos"
        idempotency_key: "memo-1"
        input: AddMemo {
            text: "Call the plumber"
        }
    }
    assert first.status == 201
    var other: Response = request {
        method: HttpMethod.post
        path: "/memos"
        idempotency_key: "memo-1"
        input: AddMemo {
            text: "Fix the tap"
        }
    }
    assert other.status == 422
    assert other.code == "idempotency_key_reused"
}
```

`idempotency_key:` takes text. The runtime checks it as it checks a
client's, so a key that isn't 1 to 255 printable characters is a 400 with
the code `invalid_request`. A `GET` reads no key, so a request to one that
gives `idempotency_key:` is `semantic.request_invalid`.

### Schedule tests

A schedule is work Jadpo runs on a timetable, such as sending reminders
every hour ([Schedules](12-jobs-and-events.md#schedules)). A schedule test
runs one with `.run()`, once, at the test's clock:

```jadpo fragment
// domains/planning/schedules/overdue_reminders.test.jadpo
test "an overdue todo gets one reminder" {
    clock: "2026-01-15T12:00:00Z"
    fakes: {
        ReminderMail.send_overdue_reminder: [ReminderReceipt { accepted_at: "2026-01-15T12:00:00Z" }]
    }

    one_overdue_todo()
    overdue_reminders.run()
    assert list.count(ReminderMail.send_overdue_reminder.requests) == 1
}
```

The run calls the schedule's action as the tick does. The tick is the
runtime's regular pass that runs each schedule when it is due
([How a schedule runs](12-jobs-and-events.md#how-a-schedule-runs)). So the
action gets no arguments, and runs as the system, with no signed-in
caller, reaching every tenant (each customer organisation whose data the
app keeps apart). It runs through the runtime's action boundary at the
test's clock, which stays fixed through the run.

`.run()` gives `Unit` whatever the action returns, since a tick keeps no
value. It fails with the action's failures, so a bare call of a schedule
that can fail takes `attempt`, and
`assert overdue_reminders.run() fails ReminderFailed` checks a failing
run.

`.run()` takes no arguments (`type.argument_count`). It writes no run
record and takes no lease, as the tick does: a schedule test proves what
one run does, and the compiler's own live tests prove the tick's records.
What the run emits is delivered once it comes back, as any step's is
([Caches and events](#caches-and-events)).

A test that runs a schedule is a schedule test, whatever it calls to set
up its data, a `request` included. `--kind schedule` runs only those.

An entity's built-in purge schedule has no name a test can write, so no
test runs it. Purge is the compiler's own operation, proven by the
compiler's live tests rather than each application's
([The purge schedule](12-jobs-and-events.md#the-purge-schedule)).

Only a test runs a schedule by hand. In the application the tick runs it
when it is due, and an operator runs it with `jadpo schedules run`. A test
runs a schedule it can see, as it calls an action, and anything else that
runs one is `semantic.not_callable`:

```jadpo check project=e-schedule-run-outside-test file=app.jadpo hidden
application {}
```

```jadpo check project=e-schedule-run-outside-test file=domains/planning/actions/tidy.jadpo hidden
/// Tidies up.
action tidy() -> Unit {}
```

```jadpo check project=e-schedule-run-outside-test file=domains/planning/schedules/tidy_up.jadpo hidden migrate=initial
use planning/actions { tidy }

/// Tidies up every quarter of an hour.
schedule tidy_up {
    every: 15m
    run: tidy()
}
```

```jadpo check project=e-schedule-run-outside-test file=domains/planning/actions/tidy_now.jadpo expect=semantic.not_callable
/// Tidies up at once.
action tidy_now() -> Unit {
    tidy_up.run()
}
```

```text output
error: An action can't run schedule `tidy_up`
 --> domains/planning/actions/tidy_now.jadpo:3:5
  |
3 |     tidy_up.run()
  |     ^^^^^^^^^^^ runs a schedule
  reason: Only a test runs a schedule by hand, with `.run()`, to prove what one run does; in the application the tick runs it when it is due, and an operator runs it now with `jadpo schedules run`.
  next: Call the action `tidy_up` runs, or move this into the schedule's test (agent)
  rule: semantic.not_callable
  help: https://jadpo.dev/docs/errors/semantic.not_callable

check failed: 1 error, 4 files
```

### Recording tests

A recording is a saved answer from an outside service, kept so the app's
code can be checked against what the service really sends
([Recordings](11-integrations.md#recordings)). Each recording of the app's
own integrations,
`shared/providers/<name>/<version>/recordings/<resource>.<operation>.json`,
is a test of kind `recording`. It is named
`<resource>.<operation>'s recording decodes`, and it reads the recording
as its operation's output through the package's decoders. You don't write
a `test` declaration for it.

`jadpo test --kind recording` runs only these, and giving a recording's
file runs it alone. A recording that doesn't decode is
`test.recording_undecodable` or `test.round_trip_failed`.

Each recording of a prompt's answer,
`prompts/recordings/<prompt>/<name>.json`, is a test of the same kind,
named `<prompt>'s <name> recording decodes`. It is read through each type
the prompt is asked for as ([Testing AI calls](21-ai.md#testing-ai-calls)). A recording made
from an older version of its prompt passes, with an alert on the report.

## Test settings
<!-- topic: test-settings -->

A test needs no settings. Without them, its clock is fixed at
2000-01-01T00:00:00Z, never the real time; each configuration field
has its `test:` or `default:` value from `config.jadpo`; and each call to a
provider answers with its fake's generated default.

To change any of these, write a `name: value` setting at the top of the
test's body, before its first statement. There are three, and all are
optional.

### `clock:`

The instant the test starts at, written out: `"2026-01-15T12:00:00Z"`
([Controlling time](#controlling-time)).

### `config:`

Configuration values that differ from the test values in `config.jadpo`
([Configuration in tests](#configuration-in-tests)).

### `fakes:`

The responses each integration operation gives, in order
([Fakes for integrations](#fakes-for-integrations)).

### Everything in the test itself

Settings are written once per test, so a reader sees everything that
shapes the result in the test itself. Shared data isn't a setting: it
comes from fixtures ([Shared test data: fixtures](#shared-test-data-fixtures)).

## Acting as a user
<!-- topic: acting-as-a-user -->

An action receives its caller, the person or service signed in, as a
typed parameter ([The principal](08-identity.md#the-principal)). So a test
passes one in:

```jadpo fragment
test "bob cannot complete alice's todo" {
    var alice: Principal.user = alice_with_todos()
    var bob: Principal.user = bob_with_no_todos()
    var todo: Todo = first_todo(alice)
    assert complete_todo(todo.id, bob) fails TodoNotFound
}
```

Here Bob tries to complete one of Alice's todos. Policy reports a record
the caller may not see as the same not-found failure as a record that
doesn't exist ([Concealing records](04-failures.md#concealing-records)),
so the test checks exactly what Bob would see.

### Making a caller

Tests, fixtures and seeds make a user caller with `Principal.user(user)`,
and a service caller with `Principal.service(service)`. Application code
can't, because there a caller only comes from signing in.

### Signing in for a route test

A route test signs in with `as:`, which goes through the application's
real authentication. A service signs in with a key from
`Service.issue_key`. So a disabled user is refused exactly as in
production, and the sign-in checks are tested too.

### Calls with no caller

A call a test, fixture or seed makes runs as the caller it passes, as
`complete_todo` runs as Bob here. A call that passes none, such as one
setting up data, runs without policy. So a call with no caller proves
behaviour and data, and a route test or a call with a caller proves
[policy](09-policy.md).

## Controlling time
<!-- topic: controlling-time -->

A test never waits for real time. Its clock starts at `clock:` and moves
only when the test moves it, with `clock.advance`:

```jadpo fragment
// domains/planning/queries/overdue_todos.test.jadpo
test "a todo due in an hour becomes overdue" {
    clock: "2026-01-15T12:00:00Z"

    var alice: Principal.user = alice_with_todos()
    attempt create_todo(CreateTodo {
        title: "Pay rent"
        due_at: "2026-01-15T13:00:00Z"
    }, alice)
    assert list.count(overdue_todos(alice)) == 0
    clock.advance(1h)
    assert list.count(overdue_todos(alice)) == 1
}
```

`clock.advance(1h)` moves the clock on by a duration. It belongs to
`clock`, like `clock.now`
([The current time: `clock.now`](10-configuration-and-time.md#the-current-time-clocknow)).
An action reads one time for its whole run, so advancing changes what the
next call sees, not a call already running.

The clock only moves forward: a negative duration is the fault
`clock_backward` ([Faults](04-failures.md#faults)). `clock.advance` is an error
anywhere but a test, in a fixture or a seed too
(`type.test_clock_context`).

### Schedules don't run by themselves

Advancing the clock doesn't run schedules, so adding a schedule never
changes an unrelated test. A schedule test runs one with `.run()`
([Schedule tests](#schedule-tests)).

### Randomness

The standard library's `random` namespace
([Standard library](19-standard-library.md)) is fixed in tests, as the
clock is. It is seeded per test, from the test alone, never from the run's
order seed. So a rerun gives the same tokens, codes and shuffles, and a
test may name the exact code it expects. New identities share the seed.

## Configuration in tests
<!-- topic: configuration-in-tests -->

A test never reads your machine's environment. Each configuration field
gets its value in a test from `config.jadpo`: its `test:` value, or else
its `default:` ([Defaults and test values](10-configuration-and-time.md#defaults-and-test-values)).
So a test gives the same answer everywhere. A test changes a field for
itself, and the code it calls, with its `config:` setting:

```jadpo check project=test-config
type FromAddress = Email {}

config {
    mail_api_key: Text {
        secret: true
    }
    mail_sender: FromAddress {
        test: "reminders@example.test"
    }
}

function address() -> FromAddress {
    return config.mail_sender
}

test "reminders come from the test sender" {
    assert address() == "reminders@example.test"
}

test "a test can change the sender" {
    config: {
        mail_sender: "planning@example.test"
    }
    assert address() == "planning@example.test"
}
```

### Secrets

Secrets never appear in test source. Each test run gets a generated
throwaway value for each one. That is enough, because a secret can only
reach places the compiler owns, such as signing and integration
credentials, and integrations are faked in tests. A secret in a test's
`config:` is `configuration.secret_default`.

### What the compiler checks

A field that tested code reads with no `test:`, `default:` or `config:`
value is a compile error, `configuration.test_value_missing`. The compiler
follows the functions, actions and fixtures a test calls to find these
reads. A name in a test's `config:` that is no field is
`configuration.field_unknown`. A value in `config:` is checked as a
`default:` is.

## Fakes for integrations
<!-- topic: fakes-for-integrations -->

A test never reaches a real provider, the outside service an integration
talks to. Each provider is described to your app by a package
([Integrations](11-integrations.md)). Every provider call a test reaches
answers through its package's fake, with the operation's generated
default, unless the test's `fakes:` scripts it.

### Scripting answers: `fakes:`

A script lists the responses an operation gives, in order. Each is a value
of the operation's output type, or one of the provider conditions the
integration maps, such as `rate_limited`
([Calling](11-integrations.md#calling)). The response still goes through
the integration's real code: input checks, retries, time limits and
failure mapping. So a test proves how your code handles each outcome.

```jadpo fragment
test "a rate-limited reminder is retried" {
    fakes: {
        ReminderMail.send_overdue_reminder: [rate_limited, ReminderReceipt { accepted_at: "2026-01-15T12:00:00Z" }]
    }
    ...
    var sent: List<ReminderMessage> = ReminderMail.send_overdue_reminder.requests
    assert list.count(sent) == 2
}
```

Here the first send is rate-limited and the retry is accepted, so the code
sent two requests.

### What the code sent: `.requests`

`Integration.operation.requests` lists what the code sent, as typed
values with secrets left out.

### What can't be faked

A fake can't run code of its own, and nothing else in the application can
be faked. A package's action runs its real code in a test, and `fakes:`
scripts the operations it calls, its own package's included.

### The number of answers

If the code calls an operation more times than the fake has responses, or
fewer, the test fails and says so (`test.fake_exhausted` or
`test.fake_unused`). A wrong count is a mistake in the test or the code,
not a provider outcome.

### An example

Here a courier's booking is faked three ways: with a scripted
`AcmeCouriers.Booking`, with the operation's `slot_taken`, which the
action maps to its own failure, and unscripted, with the generated
default:

```jadpo check project=fakes-in-tests file=app.jadpo hidden
application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            environment: config.couriers_environment
        }
    }
}
```

```jadpo check project=fakes-in-tests file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
```

```jadpo check project=fakes-in-tests file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=fakes-in-tests file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
    error_code: "error.code"
}
```

```jadpo check project=fakes-in-tests file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
public resource bookings {
    create: {
        method: HttpMethod.post
        path: "/v1/bookings"
        input: CreateBooking
        output: Booking
        changes: true
        idempotent: true
        failures: [SlotTaken]
        source: PackageSource.written
        docs: "https://docs.acme-couriers.example/bookings"
    }
}
```

```jadpo check project=fakes-in-tests file=shared/providers/acme_couriers/v1/types/postcode.jadpo hidden
/// Where a collection is.
public type Postcode = Text {
    max_length: 8
}
```

```jadpo check project=fakes-in-tests file=shared/providers/acme_couriers/v1/types/booking_id.jadpo hidden
/// The courier's reference for a booking.
public type BookingId = Text {
    max_length: 40
}
```

```jadpo check project=fakes-in-tests file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Postcode
}
```

```jadpo check project=fakes-in-tests file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: BookingId
}
```

```jadpo check project=fakes-in-tests file=shared/providers/acme_couriers/v1/failures/slot_taken.jadpo hidden
/// The slot went before the booking arrived.
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "That slot is taken."
}
```

```jadpo check project=fakes-in-tests file=domains/shipping/failures/slot_gone.jadpo hidden
/// The slot the customer chose has gone.
type SlotGone = Failure {
    kind: FailureKind.conflict
    message: "That slot has gone; choose another."
}
```

```jadpo check project=fakes-in-tests file=domains/shipping/actions/book.jadpo hidden
use app { couriers }
use shipping/failures { SlotGone }
use shared/providers/acme_couriers

/// Books a collection with the courier.
action book(postcode: AcmeCouriers.Postcode) fails SlotGone -> AcmeCouriers.Booking {
    return attempt couriers.bookings.create(
        AcmeCouriers.CreateBooking {
            postcode: postcode
        }
    ) {
        slot_taken: SlotGone
    }
}
```

```jadpo check project=fakes-in-tests file=domains/shipping/actions/book.test.jadpo
use app { couriers }
use shipping/failures { SlotGone }
use shared/providers/acme_couriers

test "the courier's reference is kept" {
    fakes: {
        couriers.bookings.create: [
            AcmeCouriers.Booking {
                id: "b-17"
            }
        ]
    }
    var booking: AcmeCouriers.Booking = attempt book("N1 9GU")
    assert booking.id == "b-17"
}

test "a taken slot is the app's own failure" {
    fakes: {
        couriers.bookings.create: [slot_taken]
    }
    assert book("N1 9GU") fails SlotGone
}

test "an unscripted call answers with the generated default" {
    var booking: AcmeCouriers.Booking = attempt book("N1 9GU")
    assert booking.id == "example"
}
```

### Values a fake answers with

A test scripts a value of the operation's output type, as Jadpo code
sees it. The fake then encodes it as the provider would send it, with the
package's `wire_name:` spellings and `output_at:` envelope, and reads it
back through the operation's real decoding, as a real answer is. So a
scripted answer exercises the package's encoders and decoders too.

So is a generated default. It is a value of the output type that keeps
the type's rules:

- text of its least length, `example` where that fits;
- a number at its least;
- an enum's first plain variant;
- empty lists and maps;
- `none` for what is optional;
- the test's clock for an instant;
- a fresh UUID.

A type whose rules no such value keeps, such as one with a pattern,
faults where the answer is read, naming the field and the rule. The test
then scripts that operation instead.

### Provider calls and capabilities in tests

Every provider call in a test is faked, as in `jadpo dev`. Nothing can be
let through to a provider's sandbox from a test.

#### Provider operations

A provider operation answers with its package fake's valid defaults
unless the test scripts it:
`fakes: { stripe.payment_intents.create: [card_declined, ...] }`. A
fake's `lost` answers as a call that may have acted, and its `in_use` as
`unavailable`. Each keeps keys as the next section says.

A scripted answer is a value of the clean output type or one of the
operation's provider failures. The test never writes the wire shape: the
fake encodes the value and decodes it, as "Values a fake answers with"
says. `jadpo calls script` sets the same answers by hand in
`jadpo dev` ([Debugging and review](17-debugging-and-review.md)).

#### Several changing calls

An action can make more than one outside call that changes something,
such as creating a customer and then charging them. Jadpo keeps a record
of its steps, and each call carries an idempotency key, so a rerun after a
crash picks up where it stopped
([Several changing calls in one action](12-jobs-and-events.md#several-changing-calls-in-one-action)).

In a test, fakes honour those keys. A repeated step under the same key
gets the stored answer. A script can also answer `lost`, meaning the
request went and no answer came back, or `in_use`, meaning another runner
holds the key.

`jadpo test` reruns every test that reaches such an action, once with a
crash at each step boundary: before the send, after the send, and after
the answer is recorded. This is on by default, and the test must pass
every time. The report names the boundary of any run that fails.

A keyed call's answer is kept under its key:

- The same key and request get the first answer again, without using up a
  scripted one.
- A different request under the same key gets the package's `reused:`
  answer.
- A failure is kept as the provider keeps it, unless its kind is
  `unavailable` or `rate_limited`.
- `lost` acted, so what it did, the generated default, answers the next
  call under its key.
- `in_use`, `unavailable` and `misconfigured` did nothing, and keep
  nothing.

A test that passes, and reached steps under a record, runs again once for
each boundary of each step it reached. Each run uses its own copy of the
database and a fresh fake. The crash stops the run with nothing it wrote
kept. The delivery is given back and runs again at once from its record,
and the crash doesn't count as an attempt. A route's request is sent
again under its key.

A delivery stops as `diverged` when a rerun doesn't match the steps its
record holds. One that stops as `diverged` between a test's steps ends the
test as [`test.subscriber_failed`](../errors/test.subscriber_failed.md). A
crash's failure begins with a `crash` line, also in `--json`:

```text
FAIL  a chosen plan starts one subscription
 --> domains/billing/actions/choose.test.jadpo:3

  crash: before step 1 of `subscribe`, `pay.customers.create`, went out; then it ran again
  delivery: `PlanChosen` to subscriber `start_subscription` stopped as diverged on attempt 1 of 10
```

If a crash leaves the fake's effects different from the run without one,
for example a call that acted under a new key, that is a fault in Jadpo,
`test.fault`.

A crash sends its step again. The `ai` fake, or a call with no key,
answers that with the next scripted answer, as a model or provider asked
again would answer afresh. So `test.fake_exhausted` and `test.fake_unused`
are judged on the run without a crash only.

A request or delivery that a crash sends again keeps none of the `ai`
answers the crashed run was given, as a restarted runner keeps none. So a
model asked before the steps is asked again, and a different answer
diverges there, as it would in production. Why it works this way is in
the [Jobs and events design notes](notes/12-jobs-and-events.md).

#### One fake per capability

A capability is a service Jadpo provides itself, such as email, which a
package can fill ([Email](16-built-in-services.md#email)). `email.sent`
lists the mail the code sent. There is no `sms.sent`, since SMS is a
package whose calls are faked like any other. A package that fills a
capability isn't faked separately: the capability's fake stands in for
every connection that fills it.

#### Documents

`documents.pdf` returns a placeholder PDF, and `documents.html` the real
rendered HTML, which a test can assert on
([Documents](16-built-in-services.md#documents)).

#### Bot checks

Bot checks pass through a local driver. A test scripts a failure with
`fakes: { bot_check: [bot_check_failed] }`, and the request is refused
with `403 bot_check_failed` before the action runs.

#### Search

Search uses PostgreSQL's full-text search, whatever engine production
names ([Search](16-built-in-services.md#search)).

#### OAuth grants

An OAuth grant lets the app call another service on a user's behalf
([Acting for each user: OAuth grants](11-integrations.md#acting-for-each-user-oauth-grants)).
A test declares its grants at the top, beside `fakes:`, as
`grants: { calendar: [alice] }`: the connection's name, and the fixtures
whose records own a grant, as `jadpo dev grants add` makes them in
development. A call on behalf of an owner with no grant fails
`grant_missing`, and a script can answer `grant_revoked`.

#### Files

In a test, an in-memory store takes the place of every `files`
connection ([Files](16-built-in-services.md#files)). It can be told to
fail a read (`unavailable`) or a write (`storage_full`). A real file sits
in a domain's `fixtures/` folder, such as `fixtures/files/customers.csv`,
and a test turns it into a stored `File` with
`fixture_file("customers.csv", store: imports)`.

#### Batches

A batch run works through many items, such as the rows of a file, a few
at a time ([Batches: work over many items](12-jobs-and-events.md#batches-work-over-many-items)).
In a test, batches run to the end within the test, after the action that
started them commits, like events. `jadpo test` runs them in batches of
two items and simulates a restart between every batch, so every import or
export test also tests resuming. A test asserts on what the app recorded
and on the summary `finished:` received, and can read `batches.status`.

#### Not built yet

These parts of fakes aren't built yet:

- `.requests`, apart from the `ai` fake's `ai.<prompt>.requests`.
- The capabilities' fakes, apart from `email.sent` and the `ai`
  capability's: its fake, `fakes: { ai.<prompt>: }`,
  `ai.<prompt>.requests`, `recorded("<name>")`, the fake's embeddings and
  `fakes: { ai.embed: }` ([Testing AI calls](21-ai.md#testing-ai-calls)).
- `fixture_file`, and telling the file store to fail from a test. The
  in-memory store itself is built.
- Running batches to the end: a test sees a run as it starts, and a
  file's run reads two rows a batch on a test's host.

What is built is every call a test reaches answering through its
package's fake, and a test's `fakes:` scripting an operation's answers in
order, each a value of its output type, one of its failures by its code,
or a condition. Why fakes work this way is in the
[Integrations design notes](notes/11-integrations.md).

## Shared test data: fixtures
<!-- topic: shared-test-data-fixtures -->

When several test files need the same data, write a fixture. A fixture is
a test-only action, in the domain's `fixtures/` folder, that creates data
by calling the application's own actions and returns what the tests need:

```jadpo fragment
// domains/planning/fixtures/alice_with_todos.jadpo
/// Alice, signed up, with two open todos.
fixture alice_with_todos() -> Principal.user {
    var user: User = attempt register_user("alice@example.test")
    var alice: Principal.user = Principal.user(user)
    attempt create_todo(CreateTodo { title: "Buy milk" }, alice)
    attempt create_todo(CreateTodo { title: "Book dentist" }, alice)
    return alice
}
```

A test calls a fixture by name, like any action. Because the fixture goes
through the real actions, validation, lifecycle and policy apply exactly
as they do for a user. Seeds build their data the same way
([Seeds](05-data.md#seeds)). A failure inside a fixture fails the test
that called it, so a fixture has no `fails` list.

### Who can call a fixture

Only tests, other fixtures and seeds can call a fixture. An action that
calls one is `semantic.not_callable`.

### A fixture for other domains: `public`

A fixture is private to its domain unless it is marked `public`. Another
domain's tests, fixtures and seeds, and a scenario in root `tests/`, call
a public fixture through its domain, like an action:
`planning.alice_with_todos()`. Calling a private one from outside its
domain is `visibility.private_cross_domain`. `public` offers a fixture to
tests, fixtures and seeds only: application code still never calls it.

Here `planning` offers a public fixture, and both a `reminders` test and a
scenario call it:

```jadpo check project=public-fixture file=app.jadpo hidden
application {}
```

```jadpo check project=public-fixture file=domains/planning/types/todo_count.jadpo hidden
/// How many todos are open.
public type TodoCount = Int {
    min: 0
}
```

```jadpo check project=public-fixture file=domains/planning/actions/add_todo.jadpo hidden
use planning/types { TodoCount }

/// The open todos once one more is added.
action add_todo(open: TodoCount) -> TodoCount {
    return TodoCount(open + 1)
}
```

```jadpo check project=public-fixture file=domains/planning/fixtures/two_open_todos.jadpo
// domains/planning/fixtures/two_open_todos.jadpo
use planning/types { TodoCount }
use planning/actions { add_todo }

/// Two open todos, added the way a user adds them.
public fixture two_open_todos() -> TodoCount {
    var one: TodoCount = add_todo(0)
    return add_todo(one)
}
```

```jadpo check project=public-fixture file=domains/reminders/actions/send_reminders.jadpo hidden
/// Sends a reminder for each open todo.
action send_reminders() -> Unit {}
```

```jadpo check project=public-fixture file=domains/reminders/actions/send_reminders.test.jadpo
// domains/reminders/actions/send_reminders.test.jadpo
use planning

test "reminders start from planning's open todos" {
    send_reminders()
    assert planning.two_open_todos() == 2
}
```

```jadpo check project=public-fixture file=tests/a_plan_starts_with_two_todos.test.jadpo
// tests/a_plan_starts_with_two_todos.test.jadpo
use planning

test "a plan starts with two open todos" {
    assert planning.two_open_todos() == 2
}
```

### Seeds and fixtures

A seed is data for local development and demos, made the same way:

| | Seed | Fixture |
|---|---|---|
| Lives in | `database/seeds/` | a domain's `fixtures/` |
| Used by | `jadpo seed <name>`, for local development and demos | tests |
| Creates data by | calling actions | calling actions |
| Data lasts | until you reset the local database | until the test ends |
| In production | refused | never compiled in |

A seed may call fixtures, so demo data and test data share their builders.
A test never loads a seed: seeds change whenever the demo changes.

### Data builders

A data builder makes one record with valid defaults, for a test that
needs a record in a particular state. The compiler generates one for each
entity: `make.Todo { title: "Buy milk" }` fills every other required field
with a valid default and goes through the real create operation, so
validation and policy apply. A builder may set any field that isn't
generated, including ones ordinary inputs don't accept, so hard-to-reach
states are easy to build.

With builders, fixtures remain for meaningful scenarios. They return a
named object holding what they made (`setup.alice`, `setup.todos`), so a
test never has to look its own data up again.

Data builders, and the named object a fixture returns with them, aren't
built yet. Until they are, build the data with a fixture that returns
what the tests need, as above.

## Assertions
<!-- topic: assertions -->

`assert condition` fails the test when the condition is false. On failure,
the report shows the file, the line, the source of the condition, and the
value of each side of a comparison.

### Checking that a call fails

To check that a call fails, name the failure with `fails`:

```jadpo fragment
test "completing a missing todo fails" {
    var alice: Principal.user = alice_with_todos()
    var missing: Todo.id = "01908f6e-1c2a-7b3c-9d4e-5f6a7b8c9d0e"
    assert complete_todo(missing, alice) fails TodoNotFound
}
```

`assert … fails` checks the call itself. A failure that leaves while the
call's arguments are worked out ends the test, like any other unhandled
failure.

### Calls that can fail

A call that can fail still needs `attempt`, `match` or `assert … fails`,
as everywhere else
([Passing a failure on: `attempt`](04-failures.md#passing-a-failure-on-attempt)).
A call with `attempt` that fails ends the test as failed, and names the
failure.

### Comparing values

Whole values are compared with `==`. On failure, records and lists show a
field-by-field diff. A route test checks `response.status`,
`response.code` and the typed output ([Route tests](#route-tests)).

There are no snapshot assertions, and no mocks of your own code.

### Events and query cost

`assert emitted ReminderDue { todo_id: todo.id }` checks that an event
was emitted, as the fakes' `.requests` checks provider calls. It isn't
built yet.

A test doesn't assert what a query costs. The compiler reports each
query's cost in the build report, and a change shows in the approval
review.

### What a failed test reports

A failed test gives a person or an agent everything needed to fix it
without rerunning it, the same standard as the application's own
[failures](04-failures.md). It is shown readably in a terminal, and as
versioned JSON with `--json`:

```text
FAIL  a rate-limited reminder is retried
      domains/planning/schedules/overdue_reminders.test.jadpo:31

  assert list.count(sent) == 2
         │                │
         1                [ReminderMessage { to: "alice@example.test", todo_title: "Buy milk", ... }]

  expected  2
  actual    1

  what ran (trace t-7f3a, clock 2026-01-15T12:00:00Z, order seed 48213):
    overdue_reminders.run()                         2 todos due
      queue_overdue_reminders()                     committed: 2 Todo updated, 2 ReminderDue emitted
    send_reminder (ReminderDue todo=01908f6e…)      reminders.messages.send → rate_limited → retry
    send_reminder (ReminderDue todo=01908f6e…)      retry scheduled for 12:00:02, not yet due
    send_reminder (ReminderDue todo=7c21d0aa…)      reminders.messages.send → accepted

  fakes:  reminders.messages.send   2 scripted, 2 used
  data:   kept as jadpo_test_7f3a (--keep-failed)
  rerun:  jadpo test domains/planning/schedules/overdue_reminders.test.jadpo:31 --seed 48213
```

- **The assertion broken into parts:** every sub-expression's value, with
  records compared field by field in a diff.
- **For `assert … fails X`:** the failure that happened instead, with its
  internal fields, or the value the call returned.
- **What ran:** the test's trace in short
  ([Debugging and review](17-debugging-and-review.md)): actions, queries,
  writes, events delivered or pending, retries, and provider calls with
  what was sent and answered.
- **How to reproduce it:** the clock, the order seed, configuration
  overrides, fake usage, the kept database and the exact rerun command.
- Every failure has a stable code (`test.assertion_failed`,
  `test.fake_unused`, `test.fake_exhausted`, `test.unexpected_failure`,
  `test.fault` for a fault such as an overflow, and
  `test.subscriber_failed` for a subscriber's delivery that failed for
  good between the test's steps) so an agent can act on it
  without parsing text. Each is shown with its real report under
  [Errors](#errors).

#### As JSON

`jadpo test --json` prints one document in the envelope every command
shares (`schemaVersion` 2, `command`, `ok`, `data`, `diagnostics`), and
`jadpo help --json` describes every field under `testReport`. `data` holds the order `seed`; `root`, the project folder as
seen from where the command ran (`"."` from inside it), which every path is
relative to; the `selection`; the `counts`; and the `tests` in the order
they ran. A failed test's `failure` has its `code` and `help`, its
`location` in the test, one of `assertion`, `failure` or `fault`, for
`test.subscriber_failed` the `delivery` it happened in (its `event`,
`subscriber`, `attempt` and `attempts`, `null` otherwise), and the
`rerun` command, which works from where the command ran. Part of the
document for the `test.fault` example under [Errors](#errors):

```json
{
  "code": "test.fault",
  "location": { "path": "plans.test.jadpo", "start": 93, "end": 117, "line": 3, "column": 12, "endLine": 3, "endColumn": 36, "label": "" },
  "fault": {
    "kind": "int_overflow",
    "at": { "path": "plans.jadpo", "start": 118, "end": 134, "line": 6, "column": 19, "endLine": 6, "endColumn": 35, "label": "out of range" },
    "stack": [
      { "location": { "path": "plans.jadpo", "start": 118, "end": 134, "line": 6, "column": 19, "endLine": 6, "endColumn": 35, "label": "" }, "callable": "function plan_effort", "repeats": 1 },
      { "location": { "path": "plans.test.jadpo", "start": 93, "end": 117, "line": 3, "column": 12, "endLine": 3, "endColumn": 36, "label": "" }, "callable": "test \"a long plan still adds up\"", "repeats": 1 }
    ]
  },
  "rerun": "jadpo test plans.test.jadpo:3 --seed 1"
}
```

Every location, a test's, a failure's `at`, a stack frame's and a
diagnostic's, has the one shape `path`, `start`, `end`, `line`, `column`,
`endLine`, `endColumn`, `label`. A stack frame is `{ location, callable,
repeats }`, a recursion counted once. A project with errors runs nothing:
`data` is `{ ran: false, root, fileCount, errors, warnings }` and the errors
are the diagnostics. `--failed` with nothing to rerun gives `{ selection,
gone, tests: [] }`, where `gone` counts the tests that failed last time and
are no longer in the project. `--watch --json` writes one document per
line, its `data` with a `watch` object (`event`, `changed`, `reached`,
`failing`, `files`, `fixed`).

## Running tests
<!-- topic: running-tests -->

`jadpo test` runs every test in the project. To run fewer, name them:

- `jadpo test <file>` runs one file;
- `jadpo test <file>:<line>` runs the test at that line;
- `--name <text>` runs the tests whose names match;
- `--kind route` runs one kind;
- `--failed` runs the tests that failed last time. When every test that
  failed last time is gone, there is nothing to rerun and the run passes.

There is no `test.only` in source: focusing is a command-line choice, so
a focused test is never committed by accident.

### Watch mode: `--watch`

`jadpo test --watch` reruns, on every save, the tests the change reaches.
It finds them with the compiler's own dependency graph, as `--affected`
does ([Tooling](14-tooling.md)).

### Interactive mode: `--interactive`

`jadpo test --interactive` opens a keyboard-driven view of every test in
the app, grouped by domain and file, with each test's last result. You
type to search files and tests; run a file, a single test or everything;
rerun failures; open a failure's full report; and turn watch mode on and
off. It needs a terminal, so agents use the flags above and `--json`.

### Paths in the output

A failed test's rerun command, and every path a run prints, work from the
folder the command ran in. Run from outside the project, they start with
the project as it was typed (`examples/07-testing/domains/…`).

### In the editor

The editor shows a run button above each test and the last result beside
it, through the language server.

Interactive mode isn't built yet: `--interactive` says
`toolchain.not_built_yet`. Use the flags above, or `--watch`, for now.

## The test database
<!-- topic: the-test-database -->

Each test gets its own database at the latest migration, empty except for
what its fixtures create. You write nothing for this; `jadpo test` does
it.

Tests run on PostgreSQL, because every environment runs the same engine:
PostgreSQL, unless the application declares otherwise
([Stores](05-data.md#stores)).

### Where the server comes from

`jadpo test` starts a disposable local PostgreSQL server for the run, from
the PostgreSQL tools on your machine. To use a server of your own, in CI
for example, name it in `JADPO_TEST_DATABASE_URL`.

### A copy for each test

`jadpo test` builds one template database from the migrations, and copies
it for each test (`CREATE DATABASE … TEMPLATE`). A copy takes tens of
milliseconds, and lets tests run in parallel. Copies are prepared in the
background while tests run.

### Real commits

Each test's actions commit for real. So work that runs after a commit,
such as delivering events, and the checks for concurrent changes behave
as in production.

### Keeping a failed test's database: `--keep-failed`

`--keep-failed` keeps a failed test's database, named in the report, and
its trace. `jadpo trace` and the developer console can then inspect
exactly what it did.

## Caches and events
<!-- topic: caches-and-events -->

An event is a fact an action records, such as "an order was placed",
and a subscriber is code that runs when one is delivered
([Subscribers](12-jobs-and-events.md#subscribers)). Events run after the
action that emits them commits. In a test, what a step emits is delivered
before the next line runs, so a test checks an event's effects on the
line after the call:

- **Events** are delivered after each step of the test commits and before
  the next statement runs, at the test's clock, until no events are left.
  A test checks their effects, such as a sent reminder in `requests`. A
  subscriber that fails fails the test, as below.
- **Caches** are copies used only by display reads from routes
  ([Caches](05-data.md#caches)). In a test they are on and start empty,
  in an in-memory cache that behaves like Redis, so a route test sees the
  same invalidation as production. Tests can't read, fill or clear a
  cache directly. Actions never read caches, so only route tests meet
  them.

### Steps and delivery

A step is a call of an action or fixture, a route test's request, a
schedule's `.run()` or `clock.advance(...)`. Once a step comes back, what
is due in the test's outbox, the list of events waiting to be delivered,
is delivered as the runner delivers it anywhere: as the system, limited
to its event's tenant. Then what those subscribers emit is delivered in
turn, until nothing is due.

### The order subscribers run in

Subscribers run in an order shuffled by the run's seed, the one the
report prints and `--seed` gives again, mixed with the test's own
identity. Each delivery is drawn from all those due at that moment. So a
test that relies on the order its subscribers run in fails under some
seed, and rerunning it with that seed fails the same way.

### Retries

A delivery whose subscriber fails in a way worth trying again waits its
full backoff: 30 seconds, and then twice as long each time. There is no
random jitter, since a test has no randomness to draw it from
([Retries and giving up](12-jobs-and-events.md#retries-and-giving-up)).
The test's clock moves only when the test moves it, so the retry runs
once `clock.advance` reaches it.

A fake failing a subscriber's call to a provider is retried and given up
the same way, and a test scripts it with `fakes:`. A subscriber's own
failure worth retrying tests its retries too.

### A delivery that fails for good

A delivery fails for good when its failure isn't worth retrying, when it
fails on its last attempt, or when it hits a fault. It ends the test there
as [`test.subscriber_failed`](../errors/test.subscriber_failed.md), naming
the event, the subscriber and the attempt.

What a route's `settle:` delivers while its request waits belongs to the
request, and the response says how it ended
([Waiting for follow-on work](07-http.md#waiting-for-follow-on-work)).

### An example

Here placing an order emits `OrderPlaced`, and `cook_placed` cooks it
once a chef is on shift:

```jadpo check project=events-in-tests file=app.jadpo hidden
application {}
```

```jadpo check project=events-in-tests file=domains/kitchen/entities/order.jadpo hidden
/// A dinner order.
entity Order {
    id: Uuid
    cooked_at: Instant?

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}
```

```jadpo check project=events-in-tests file=domains/kitchen/entities/chef.jadpo hidden
/// A chef on shift.
entity Chef {
    id: Uuid
    on_shift: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}
```

```jadpo check project=events-in-tests file=domains/kitchen/events/order_placed.jadpo hidden
use kitchen/entities { Order }

/// Someone placed an order.
type OrderPlaced = Event {
    order_id: Order.id
}
```

```jadpo check project=events-in-tests file=domains/kitchen/events/events.jadpo_record hidden
// Generated by Jadpo from the events of domain kitchen; update it with `jadpo records update`, never by hand.
record events 1

event OrderPlaced
    order_id: Uuid
```

```jadpo check project=events-in-tests file=domains/kitchen/failures/no_chef.jadpo hidden
/// Nobody is on shift to cook.
type NoChef = Failure {
    kind: FailureKind.unavailable
    message: "Nobody is cooking yet."
}
```

```jadpo check project=events-in-tests file=domains/kitchen/queries/chefs.jadpo hidden
use kitchen/entities { Chef }

/// The chefs on shift.
query chefs() -> List<Chef> {
    from: Chef
    where: Chef.on_shift == true
    order: [Chef.id asc]
    limit: 10
}
```

```jadpo check project=events-in-tests file=domains/kitchen/actions/place_order.jadpo hidden
use kitchen/entities { Order }
use kitchen/events { OrderPlaced }

/// Places an order; cooking it follows in a subscriber.
action place_order() -> Order {
    var order: Order = Order.create {
        set: {
            cooked_at: none
        }
    }
    emit OrderPlaced {
        order_id: order.id
    }
    return order
}
```

```jadpo check project=events-in-tests file=domains/kitchen/actions/start_shift.jadpo hidden
use kitchen/entities { Chef }

/// Puts a chef on shift.
action start_shift() -> Chef {
    return Chef.create {
        set: {
            on_shift: true
        }
    }
}
```

```jadpo check project=events-in-tests file=domains/kitchen/actions/cook.jadpo
use kitchen/entities { Order, OrderNotFound }
use kitchen/failures { NoChef }
use kitchen/queries { chefs }

/// Cooks an order, once a chef is on shift.
action cook(order_id: Order.id) fails OrderNotFound, NoChef -> Unit {
    if (list.count(chefs()) == 0) {
        reject NoChef {}
    }
    attempt Order.update(order_id) {
        set: {
            cooked_at: clock.now
        }
    }
}
```

```jadpo check project=events-in-tests file=domains/kitchen/subscribers/cook_placed.jadpo migrate=initial
use kitchen/actions { cook }
use kitchen/events { OrderPlaced }

/// Cooks each order placed.
subscriber cook_placed {
    on: OrderPlaced
    run: cook(event.order_id)
}
```

```jadpo check project=events-in-tests file=domains/kitchen/actions/place_order.test.jadpo
use kitchen/entities { Chef, Order }
use kitchen/actions { start_shift }

test "an order is cooked before the next line" {
    clock: "2026-10-20T18:00:00Z"
    var chef: Chef = start_shift()
    assert chef.on_shift
    var order: Order = place_order()
    var cooked: Order = attempt Order.get(order.id)
    assert cooked.cooked_at == "2026-10-20T18:00:00Z"
}

test "an order placed before the shift is cooked when its retry comes round" {
    clock: "2026-10-20T18:00:00Z"
    var order: Order = place_order()
    var chef: Chef = start_shift()
    assert chef.on_shift
    var waiting: Order = attempt Order.get(order.id)
    assert waiting.cooked_at == none
    clock.advance(30s)
    var cooked: Order = attempt Order.get(order.id)
    assert cooked.cooked_at == "2026-10-20T18:00:30Z"
}
```

The first test reads the order cooked on the line after placing it.
In the second, the delivery fails with `NoChef`, which is worth
retrying, so it waits 30 seconds. Starting the shift doesn't move the
clock, and the retry runs once `clock.advance(30s)` reaches it.

Not built yet: caches, which need the Redis driver, and running a batch
to the end in a test. A batch run a step starts is left as it was started
([Provider calls and capabilities in tests](#provider-calls-and-capabilities-in-tests)).

## What is built
<!-- topic: testing-built -->

| Feature | Status |
|---|---|
| Pure and action tests | Built: `jadpo test` runs `test "…" { }` and `assert` in the interpreter, for tests that call functions and actions. Each test gets its own environment (its seed and clock, its database and fakes), tests run on a thread per core, and the results are reported in the order seed's order. Each action or fixture a test calls commits on its own, through the runtime's action boundary; an action it calls joins its transaction. |
| Kind of test | Built: the kind comes from what a test calls (an action, query or fixture makes it an action test, a `request` a route test, a schedule's `.run()` a schedule test), and the footer and `--json` count each kind. |
| Schedule tests | Built: `schedule.run()` in a test runs the schedule once at the test's clock, its action called as the system with no tenant, with its own database like an action test; `--kind schedule` runs only these; anywhere but a test it is `semantic.not_callable`. |
| Route tests | Built: `request { method: path: params: input: }` sends a request through the runtime's HTTP component and gives a `Response` with `status`, `code` and the route's typed `output`; a route test gets its own database like an action test, and a defect behind a 500 ends it as `test.fault`. `as:` signs in a user through a real browser session or API token, and a service through a real service key. Every request but a `GET` sends a fresh `Idempotency-Key`, or the one `idempotency_key:` gives ([Idempotency keys](#idempotency-keys)). An optional field the `input:` leaves out is left out of the body, so a `patch:` keeps it. A streaming route's `Response` has `pieces` in place of `output`, its events read to the last. |
| Assertion messages | Partly built: a failed `assert` reports its file and line, its source with each part's value hung beneath it, expected and actual, a field-by-field diff for records, lists, sets and maps, a stable code and the rerun command with the order seed (see [Errors](#errors)). Not built yet: the trace, fakes, kept database and clock in the report. A statement over several lines shows its first line; `--json` has all of it. |
| Checking a failure | Built: `assert call(…) fails Failure`; when the call returns or fails differently, the report shows the value, or the other failure with its code, kind, fields and where it was raised. A failure under `attempt` ends the test as `test.unexpected_failure`, and a fault as `test.fault`, each with its `.jadpo` frames. |
| Events | Built: what each step emits is delivered once it comes back and before the next statement, at the test's clock, until nothing is due, in an order the run's seed shuffles; a retry waits its full backoff until `clock.advance` reaches it, and a delivery that fails for good, or stops as `diverged`, ends the test as `test.subscriber_failed` ("Caches and events"). |
| Clock | Built: a test's clock starts at its `clock:`, or at 2000-01-01T00:00:00Z without one, and `clock.advance(...)` moves it on for the calls after it; only a test moves it (`type.test_clock_context`, a fixture and a seed included), and a move backwards is the fault `clock_backward`. |
| Configuration | Built: `test:` values on fields, defaults, generated secrets and a test's own `config:`; a request sent through a route is not followed for the compile-time check that every setting a test reads has a value, and a read with none then ends the test as a fault naming the setting. |
| Integration fakes | Partly built: for provider packages, every provider call a test reaches answers through its package's fake, with the operation's generated default unless the test's `fakes:` scripts its answers, each checked against the package (`semantic.fake_invalid`); a script the calls run past is `test.fake_exhausted`, one with answers left `test.fake_unused` ("Fakes for integrations"). Package fakes keep keys, and a test that reaches an action with several changing calls runs again with a crash at each step boundary ("Provider calls and capabilities in tests"). `email.sent` ([Email](16-built-in-services.md#email)): every email a test sends is caught, rendered whole, a mail package's included, and read as a `List<SentEmail>`. Not built yet: `.requests` (built for the `ai` fake, `ai.<prompt>.requests`), the capability fakes but `ai`'s, grants, documents, batches and `fixture_file`. |
| Database | Built: `jadpo test` gives each action test in a project with tables its own copy of a template built from the migrations, on the server `JADPO_TEST_DATABASE_URL` names or a disposable one started from the machine's PostgreSQL tools; copies are prepared in the background, and a test gets its copy the first time it uses a store. `--keep-failed` keeps a failed test's database and names it in the report. Tests read and write records through actions, queries and fixtures; a concurrent change, which needs a second connection, is shown by the runtime's own tests. |
| Caller | Built: `Principal.user(user)` and `Principal.service(service)` in tests, fixtures and seeds; a call passing a caller runs as that caller, one passing none runs without policy. |
| Placement | Built: a test sits in `<name>.test.jadpo` beside its subject, or in root `tests/`; a test elsewhere is `layout.wrong_folder`, which names the `.test.jadpo` file beside it, and a test file with nothing beside it is `layout.test_without_subject`. |
| Watch mode | Partly built: `jadpo test --watch` runs the tests asked for, then on every save the ones that reach what changed, through the graph's digests and `calls` and `uses` edges, on one long-lived database; a save no test reaches says `nothing to rerun`, a broken build shows its errors, and the next good save runs everything changed since the last run, or says `errors fixed` when no test reaches it; a removed file is a change like any other. Run from outside the project, its paths start with the project as typed. It ends when nothing reads its output any more, as when piped into `head`. A save shows its results in about 0.1 s on a 230-file project, and the gate fails above 1.25 s. The keys in the interactive screen (`enter`, `a`, `f`) wait for `--interactive`, which says `toolchain.not_built_yet`; ctrl-c quits. |
| Fixtures | Built: a fixture is checked like an action and called by name from tests, fixtures and seeds; an action calling one is `semantic.not_callable`. A `public` fixture is called through its domain from other domains' tests, fixtures and seeds and from root `tests/`; a private one from there is `visibility.private_cross_domain`. Seeds are checked but `jadpo seed` is not built. |
| Data builders, `assert emitted` | Not built yet. |
| Interactive mode | Not built yet: `jadpo test --interactive` says `toolchain.not_built_yet`. |
| Caches in tests | Not built yet: caches need the Redis driver. |
| Golden application contract | Built: the shared black-box harness in `benchmarks/contract/` executes all 44 versioned application cases interpreted, on generated native code and on compiled workerd, with real credential verification, raw invalid inputs, physical SQL observation, persisted-state assertions, controlled clocks and provider faults. These checks supplement authored Jadpo tests; they add no application-facing test controls. Full-gate ownership is interpreted and native in `compiled`, workerd in `wasm32`, and verifier negative controls in `lint`. |
| `test.no_tests` | Built: raised by `jadpo test` only ([Tooling](14-tooling.md)), when the project has no tests or a file, line, name, kind or `--failed` picks none. |

## Errors

Every error that testing can produce, each with an example that triggers
it: first the six codes a failed test gets from `jadpo test`, each with
its report (run with `--seed 1`), then the `jadpo check` errors.

- [`test.assertion_failed`](../errors/test.assertion_failed.md)
- [`test.unexpected_failure`](../errors/test.unexpected_failure.md)
- [`test.fault`](../errors/test.fault.md)
- [`test.subscriber_failed`](../errors/test.subscriber_failed.md)
- [`test.fake_exhausted`](../errors/test.fake_exhausted.md)
- [`test.fake_unused`](../errors/test.fake_unused.md)
- [`semantic.fake_invalid`](../errors/semantic.fake_invalid.md)
- [`semantic.request_route_unknown`](../errors/semantic.request_route_unknown.md)
- [`semantic.request_invalid`](../errors/semantic.request_invalid.md)
- [`type.test_clock_context`](../errors/type.test_clock_context.md)
- [`configuration.field_unknown`](../errors/configuration.field_unknown.md)
- [`syntax.assertion_form`](../errors/syntax.assertion_form.md)
