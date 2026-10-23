---
title: Testing
area: 13
status: decided
checked-with: this checkout's jadpo on every gate run (archived examples: v0 3ce094d0)
rules:
  - configuration.fixture_duplicate
  - configuration.fixture_secret_required
  - configuration.fixture_secret_unexpected
  - configuration.fixture_value_invalid
  - configuration.fixture_value_missing
  - semantic.request_invalid
  - semantic.fake_invalid
  - semantic.request_route_unknown
  - semantic.test_service_fake_required
  - syntax.assertion_form
  - test.assertion_failed
  - test.fake_exhausted
  - test.fake_unused
  - test.fault
  - test.subscriber_failed
  - test.unexpected_failure
  - type.test_call_context
  - type.test_clock_context
---

# Testing

How a Jadpo program is tested: where tests live, the four kinds of test,
controlling the clock, configuration and fakes in tests, shared test data,
assertions, the test database, and how tests meet caches and events.

> Decided on 2026-10-13. Code on this page applies the decisions from areas 1 to 13. The new compiler runs pure and action tests in its interpreter (chunk 2.2) and gives each test its own PostgreSQL database (chunk 3.1); the [status table](#status) says what else is built and when the rest comes.

Examples use the golden todo app's `planning` domain: `User` and `Todo`
entities, a `create_todo` action, a `TodoNotFound` failure, an
`overdue_reminders` schedule and a `ReminderMail` integration.

The rule behind all of it: **a test runs the real application against a real
database; only the outside world and the clock are replaced.** There are no
mocks of your own actions and no in-memory stand-in for the database.

## Where tests live

Decided in area 1: a test sits beside the file it tests, as
`<name>.test.jadpo`. A scenario that spans several domains goes in the root
`tests/` folder. There is no folder per kind of test.

```text
domains/planning/
  actions/     create_todo.jadpo        create_todo.test.jadpo
  routes/      todos.jadpo              todos.test.jadpo
  schedules/   overdue_reminders.jadpo  overdue_reminders.test.jadpo
  fixtures/    alice_with_todos.jadpo
tests/         sign_up_to_first_reminder.test.jadpo
```

A test file holds only `test` declarations. A test beside its subject can
call anything in its own domain, private or not. A scenario in `tests/` sees
what another domain would see: public declarations and routes (question 1).
A test file sees everything its subject declares without importing it, and
brings in anything else it uses on `use` lines, as any file does; a
scenario imports all it uses (area 1, Imports; built in chunk 3.5).

## Writing a test

A test is a sentence and a body. The sentence names it, once in the whole
project: a second test with the same sentence is
`semantic.duplicate_declaration` (area 18 keys a test by the file it sits
beside and its sentence). The body is ordinary code plus `assert`,
which fails the test when its condition is false. Each test starts from
nothing: its own empty database, its own clock and its own fakes, so tests
never depend on each other and can run in any order.

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

## Kinds of test

Decided in area 1: the kind of a test is known from what it calls, and
`jadpo test --kind route` runs one kind. The report counts each kind
separately, because each proves something different: an action test does
not prove that the route decodes input or checks who is signed in.

| Kind | The test calls | It proves |
|---|---|---|
| pure | functions only | the calculation |
| action | actions and queries directly | the behaviour and the data it writes |
| route | a route, through `request` | the whole HTTP boundary: input, sign-in, policy, failures, output |
| schedule | a schedule, through `.run()` | what one run of the schedule does |

A fifth kind runs beside them without a `test` declaration (area 11,
chunk 7.2d2): each recording of the app's own integrations,
`shared/providers/<name>/<version>/recordings/<resource>.<operation>.json`,
is read as its operation's output through the package's decoders, a test
of kind `recording` named `<resource>.<operation>'s recording decodes`.
`jadpo test --kind recording` runs only those, and giving the recording's
file runs it alone; one that doesn't decode is area 11's
`test.recording_undecodable` or `test.round_trip_failed`. Each recording
of a prompt's answer, `prompts/recordings/<prompt>/<name>.json`, is a test
of the same kind, `<prompt>'s <name> recording decodes`, read through each
type the prompt is asked for as (area 21, "Recorded answers"; chunk
5.5v4); one made from an older version of its prompt passes, with an
alert on the report.

A test that calls actions to set up data and then makes a request is a route
test. Calling an action is an ordinary call, with no `call` word. Area 1
named the second kind "integration"; this page calls it "action", because
`integration` is the keyword for third-party APIs (area 11).

Only actions write records (area 6), so a test changes data through the
action that does it, which is what an action test proves, and a fixture
builds the data it starts from. A test may read straight from an entity
with `get`, as it may call a query, and either makes it an action test
with its own database; a write straight to an entity is
`semantic.not_callable` (chunk 3.10):

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
  help: docs/language/01-project-layout-and-modules.html#semantic-not-callable

check failed: 1 error, 4 files
```

A route test sends a typed request through the real generated boundary
(question 5; area 7 settles the request's shape):

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

Built in chunk 4.1, and `as:` in chunks 4.3 and 4.3b: it signs in through the application's real authentication, a browser session, an API token or, for `Principal.service(service)`, a service key, as area 8's end-to-end examples show. The route below lets anyone call it, so this test signs no one in; `add_memo` reaches no entity, so root `policy { }` says who may call it (area 9). The
request names its route by `method:` and its path as `jadpo routes` prints
it, each placeholder given in `params:` (`path: "/todos/{todo_id}"` with
`params: { todo_id: todo.id }`), and `input:` is the route's `Input`. It
goes through the runtime's HTTP component exactly as a client's would, and
gives a `Response`: `status`, the failure's `code` (`none` on success) and
the route's `output` (`none` unless it succeeded). A route that streams a
model's answer gives `pieces`, the text of each event in order, in place of
`output`, and `code` the error event's that ended it (area 7, "Streaming a
model's answer"; chunk 5.5v6). A defect behind the response ends the test
as `test.fault`, with its frames:

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

A route test sends a fresh `Idempotency-Key` with every request but a
`GET`, as a real client would, so a route on an API with `idempotency:
Idempotency.required` (area 7) is tested through its success like any
other. A request that gives `idempotency_key:` sends that key instead, to
test what a repeat gets back and what reusing a key for another request is
refused with (Richard, 2026-10-16; built in chunk 4.7). This API refuses a
write without a key:

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
would be; the others send `memo-1` twice. A repeat gets the first answer
back and leaves no second memo; the same key with another memo is 422
`idempotency_key_reused`:

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

`idempotency_key:` takes text; the runtime checks it as it checks a
client's, so a key that isn't 1 to 255 printable characters is 400
`invalid_request`. A `GET` reads no key, so a request to one that gives
`idempotency_key:` is `semantic.request_invalid`.

A schedule test runs the schedule once at the test's clock:

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

Built in chunk 6.2c, but for fakes, which come with packages: the run
calls the schedule's action with nothing, as the system reaching every
tenant, as the tick does (area 12, "How a schedule runs"), through the
runtime's action boundary at the test's clock, which stays fixed through
the run. It gives `Unit` whatever the action returns, since a tick keeps
no value, and fails with the action's failures, so a bare call of one
that can fail takes `attempt`, and `assert overdue_reminders.run() fails
ReminderFailed` checks a failing run. It takes no arguments
(`type.argument_count`), writes no run record and takes no lease: a
schedule test proves what one run does, and the tick's records are the
compiler's own live tests'. What the run emits is delivered once it
comes back, as any step's is ("Caches and events"). A test that
runs a schedule is a schedule test, whatever it calls to set up its data,
a `request` included, and `--kind schedule` runs only those.

An entity's built-in purge schedule has no name a test can write, so no
test runs it: purge is the compiler's own operation, proven by the
compiler's live tests rather than each application's.

Only a test runs a schedule by hand: the tick runs it when it is due, and
an operator with `jadpo schedules run`. A test runs a schedule it can
see, as it calls an action:

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
  help: docs/language/01-project-layout-and-modules.html#semantic-not-callable

check failed: 1 error, 4 files
```

## Test settings

A test's environment is set at the top of its body, before the first
statement, as `name: value` settings (question 2). There are three, and all
are optional:

- `clock:` the instant the test starts at, written out
  (`"2026-01-15T12:00:00Z"`). Without it, the clock is fixed at
  2000-01-01T00:00:00Z, never the real time (chosen in chunk 5.2).
- `config:` configuration values that differ from the test values in
  `config.jadpo`.
- `fakes:` the responses each integration operation gives, in order.

Settings are written once per test, so a reader sees everything that shapes
the result in the test itself. Shared data is not a setting; it comes from
fixtures, below.

## Acting as a user

Area 8 proposes that an action receives its caller as a typed parameter, so a
test passes one in. Tests, fixtures and seeds can make a caller from a user
with `Principal.user(user)`; application code cannot, because there a caller
only comes from signing in (question 10). A route test signs in with `as:`,
which goes through the application's real authentication, so a disabled
user is refused exactly as in production.

```jadpo fragment
test "bob cannot complete alice's todo" {
    var alice: Principal.user = alice_with_todos()
    var bob: Principal.user = bob_with_no_todos()
    var todo: Todo = first_todo(alice)
    assert complete_todo(todo.id, bob) fails TodoNotFound
}
```

Policy reports a hidden record as the same `not_found` failure (area 4), so
the test checks exactly what Bob would see. A call a test, fixture or seed
makes runs as the caller it passes, as `complete_todo` runs as Bob here; a
call that passes none, such as one setting up data, runs without policy, so
it proves behaviour and data, and a route test or a call with a caller
proves policy (chosen in chunk 4.4, area 9).

## Controlling time

A test never waits for real time. The clock starts at `clock:` and moves
only when the test says so, with `clock.advance(5m)`: the clock belongs to
`clock`, like `clock.now`. An action reads one time for its whole run, so
advancing changes what the next call sees, not a call already running.
The clock only moves forward: a negative duration is the fault
`clock_backward`. `clock.advance` is an error outside a test, in a fixture
or a seed too (`type.test_clock_context`, chosen in chunk 5.2).

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

Advancing the clock does not run schedules by itself; a schedule test runs
one with `.run()` (question 5).

> Amended 2026-10-17 by area 19 (question 19): the standard library's
> `random` namespace is fixed in tests as the clock is: seeded per test,
> so a rerun gives the same tokens, codes and shuffles. Built in chunk
> 5.5q: the seed comes from the test alone, never the run's order seed,
> so a test may name the exact code it expects; new identities share it.

## Configuration in tests

Each configuration field can give a `test:` value in `config.jadpo`, used by
every test (question 3; area 10 settles the configuration syntax). A test
reads the field's `test:` value, then its `default:`, and never the
machine's environment, so it gives the same answer everywhere. A test
changes a field for itself, and the code it calls, with its `config:`
setting. Secrets never appear in test source: each test run gets a generated
throwaway value, which is enough because a secret can only reach
compiler-owned places (signing, integration credentials), and integrations
are faked.

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

A field that tested code reads with no `test:`, `default:` or `config:` value
is a compile error (`configuration.test_value_missing`, area 10), followed
through the functions, actions and fixtures a test calls, and so is a secret
in a test's `config:` (`configuration.secret_default`) or a name there that is
no field (`configuration.field_unknown`). A value in `config:` is checked as a
`default:` is. Built in chunk 5.1.

## Fakes for integrations

A test never reaches a real provider. Every integration operation the test
can reach needs a fake, or the test does not compile. A fake lists the
responses the operation gives, in order: a value of the operation's output
type, or one of the provider conditions the integration maps (area 11), such
as `rate_limited`. The response still goes through the integration's real
code: input checks, retries, time limits and failure mapping, so a test
proves how your code handles each outcome.

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

`Integration.operation.requests` lists what the code sent, as typed values
with secrets left out. A fake cannot run code of its own, and nothing else in
the application can be faked. If the code calls an operation more times than
the fake has responses, or fewer, the test fails and says so (question 7).

> **Built (chunk 7.2d)** for provider packages' operations, as below and
> under [Provider calls and capabilities in tests](#provider-calls-and-capabilities-in-tests):
> every call a test reaches answers through its package's fake, and a
> test's `fakes:` scripts an operation's answers in order, each a value of
> its output type, one of its failures by its code, or a condition.
> `.requests` and the capabilities' fakes are not built yet, but for
> `email.sent` (built in chunk 7.3c, area 16) and the `ai` capability's: its fake, `fakes: { ai.<prompt>: }`
> and `ai.<prompt>.requests` are built (chunk 5.5v3; area 21, "Tests"),
> and `recorded("<name>")`, the fake's embeddings and `fakes: { ai.embed:
> }` since chunk 5.5v4.
> The choices are in area 11's "Chosen in chunk 7.2d". Since chunk
> 7.6 a package's action runs its real code in a test, and `fakes:`
> scripts the operations it calls, its package's own included.

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

A scripted value is written as the provider would send it, its
`wire_name:` spellings and `output_at:` envelope included, and read back
through the operation's real decoding, as a real answer is; so is a
generated default, a value of the output type keeping its rules: text of
its least length (`example` where that fits), a number at its least, the
first plain variant, empty lists and maps, `none` for what is optional,
the test's clock for an instant and a fresh UUID. A type whose rules no
such value keeps, a pattern say, faults where the answer is read, naming
the field and the rule, so the test scripts that operation instead.

### Provider calls and capabilities in tests

Decided 2026-10-16 (Richard), with the packages and capabilities decisions
(areas 11, 12 and 16). Every provider call in a test is faked, as in
`jadpo dev`; nothing can be let through to a sandbox from a test.

- **Provider operations** answer with the package fake's valid defaults
  unless the test scripts them, as question 7 decided (built in chunk
  7.2d; a fake's `lost` answers as a call that may have acted, its
  `in_use` as `unavailable`, and since chunk 7.5d each keeps keys as
  below):
  `fakes: { stripe.payment_intents.create: [card_declined, ...] }`. A
  scripted answer is the clean output type or one of the operation's
  provider failures, never the wire shape; the package's own tests cover
  its decoding. `jadpo calls script` sets the same answers by hand in
  `jadpo dev` (area 17).
- **Several changing calls** (area 12): fakes honour idempotency keys, so a
  repeated step under the same key gets the stored answer, and a script
  can answer `lost` (the request went, no answer came back) or `in_use`
  (another runner holds the key). `jadpo test` reruns every test that
  reaches an action with several changing calls once with a crash at each
  step boundary (before the send, after the send, after the answer is
  recorded), on by default, and the test must pass every time; the report
  names the boundary of any run that fails.

  Built in chunk 7.5d. A keyed call's answer is kept under its key: the
  same key and request get the first answer again without taking a
  scripted one, a different request gets the package's `reused:` answer,
  and a failure is kept as the provider keeps it, unless its kind is
  `unavailable` or `rate_limited`. `lost` acted, so what it did (the
  generated default) answers the next call under its key; `in_use`,
  `unavailable` and `misconfigured` did nothing and keep nothing. A test
  that passes and reached steps under a record runs again once for each
  boundary of each step it reached, each time on its own copy of the
  database with a fresh fake; the crash stops the run with nothing it
  wrote kept, the delivery is given back and runs again at once from its
  record (the crash not counted as an attempt), and a route's request is
  sent again under its key. A delivery that stops as `diverged` between a
  test's steps ends the test as
  [`test.subscriber_failed`](#test-subscriber-failed), and a crash's
  failure begins with a `crash` line, also in `--json`:

  ```text
  FAIL  a chosen plan starts one subscription
   --> domains/billing/actions/choose.test.jadpo:3

    crash: before step 1 of `subscribe`, `pay.customers.create`, went out; then it ran again
    delivery: `PlanChosen` to subscriber `start_subscription` stopped as diverged on attempt 1 of 10
  ```

  A crash that left the fake's effects other than the run without one (a
  call that acted under a new key) is a fault in Jadpo, `test.fault`.
  Since a crash sends its step again, and the `ai` fake or a call with no
  key answers it with the next scripted answer, as a model or provider
  asked again would answer afresh, `test.fake_exhausted` and
  `test.fake_unused` are judged on the run without a crash only; and a
  request or delivery a crash sends again keeps none of the `ai` answers
  the crashed run was given, as a restarted runner keeps none, so a model
  asked before the steps is asked again and a different answer diverges
  there as it would in production (chunk 7.9a).
  Live tests in `crates/jadpo-cli/tests/step_crashes.rs`; choices in
  area 12's decisions table, "Tests, chosen in chunk 7.5d".
- **One fake per capability**, under design C: `email.sent` lists the mail
  the code sent (area 16); there is no `sms.sent`, since SMS is a package
  whose calls are faked like any other. A package that fills a capability
  is not faked separately: the capability's fake stands in for every
  connection that fills it.
- **Documents**: `documents.pdf` returns a placeholder PDF, and
  `documents.html` the real rendered HTML, which a test can assert on.
- **Bot checks** pass through a local driver; a test scripts a failure
  with `fakes: { bot_check: [bot_check_failed] }`, and the request is
  refused with `403 bot_check_failed` before the action runs.
- **Search** uses PostgreSQL's full-text search whatever engine production
  names.
- **OAuth grants** are declared at the top of a test beside `fakes:`, as
  `grants: { calendar: [alice] }`: the connection's name, and the fixtures
  whose records own a grant, as `jadpo dev grants add` makes them in
  development. A call on behalf of an owner with no grant fails
  `grant_missing`, and a script can answer `grant_revoked`.
- **Files**: the in-memory store can be told to fail a read
  (`unavailable`) or a write (`storage_full`). A real file sits in a
  domain's `fixtures/` folder (`fixtures/files/customers.csv`), and a test
  turns it into a stored `File` with `fixture_file("customers.csv", store:
  imports)`. Built in chunk 6.4b: one in-memory store, in every `files`
  connection's place, which the runtime's Rust tests tell to have no room
  and to read a few bytes at a time; `fixture_file` and telling it to
  fail from a test come with the fakes (chunk 7.2).
- **Batches** (area 12) run to the end within the test, after the action
  that started them commits, like events (not built yet: a test sees a
  run as it starts; a file's run reads two rows a batch on a test's host,
  chunk 6.4b). `jadpo test` runs them in batches
  of two items and simulates a restart between every batch, so every
  import or export test also tests resuming; a test asserts on what the
  app recorded and on the summary `finished:` received, and can read
  `batches.status`.

## Shared test data: fixtures

Decided in area 1: test data used by several test files lives in the
domain's `fixtures/` folder. A fixture is a test-only action that creates
data by calling the application's own actions, so validation, lifecycle and
policy apply exactly as they do for a user, and returns what the tests need
(question 4). It is called by name like any action. A failure inside a
fixture fails the test that called it, so a fixture has no `fails` list.

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

Only tests, other fixtures and seeds can call a fixture; an action that
calls one is `semantic.not_callable`. A fixture is private to its domain
unless it is marked `public`. Another domain's tests, fixtures and seeds,
and a scenario in root `tests/`, call a public fixture through its domain,
like an action (`planning.alice_with_todos()`); calling a private one from
outside its domain is `visibility.private_cross_domain`. `public` offers a
fixture to tests, fixtures and seeds only: application code still never
calls it.

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

| | Seed | Fixture |
|---|---|---|
| Lives in | `database/seeds/` | a domain's `fixtures/` |
| Used by | `jadpo seed <name>`, for local development and demos | tests |
| Creates data by | calling actions | calling actions |
| Data lasts | until you reset the local database | until the test ends |
| In production | refused (area 5) | never compiled in |

A seed may call fixtures, so demo data and test data share their builders.
A test never loads a seed: seeds change whenever the demo changes.

## Assertions

`assert condition` fails the test when the condition is false. On failure,
the report shows the file, the line, the source of the condition, and the
value of each side of a comparison (question 6). To check that a call fails,
name the failure:

```jadpo fragment
test "completing a missing todo fails" {
    var alice: Principal.user = alice_with_todos()
    var missing: Todo.id = "01908f6e-1c2a-7b3c-9d4e-5f6a7b8c9d0e"
    assert complete_todo(missing, alice) fails TodoNotFound
}
```

A call that can fail still needs `attempt`, `match` or `assert … fails`, as
everywhere else (area 3). A call with `attempt` that fails ends the test
as failed and names the failure. `assert … fails` checks the call itself:
a failure that leaves while its arguments are worked out ends the test,
like any other unhandled failure. A route test checks `response.status`,
`response.code` and the typed output (area 7). There are no snapshot
assertions and no mocks of your own code.

### What a failed test reports

A failed test gives a person or an agent everything needed to fix it without
rerunning it (Richard, 2026-10-13), the same standard as application failures
(area 4). Shown readably in a terminal and as versioned JSON with
`--json`:

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

- **The assertion broken into parts:** every sub-expression's value, as
  pytest and power-assert show; records compared field by field with a diff.
- **For `assert … fails X`:** the failure that happened instead (with its
  internal fields), or the value the call returned.
- **What ran:** the test's trace in short (area 17): actions, queries, writes,
  events delivered or pending, retries, and provider calls with what was sent
  and answered.
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
`jadpo help --json` describes every field under `testReport` (built, chunks
2.2 and 2.8). `data` holds the order `seed`; `root`, the project folder as
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

- `jadpo test` runs everything; `jadpo test <file>` one file;
  `jadpo test <file>:<line>` the test at that line; `--name <text>` tests
  whose names match; `--kind route` one kind; `--failed` the tests that
  failed last time.
- **Watch mode:** `jadpo test --watch` reruns, on every save, the tests the
  change reaches, using the compiler's own dependency graph (area 14's
  `--affected`).
- **Interactive mode:** `jadpo test --interactive` opens a keyboard-driven
  view of every test in the app, grouped by domain and file, with each
  test's last result. Type to search files and tests; run a file, a single
  test or everything; rerun failures; open a failure's full report; toggle
  watch mode. It needs a terminal; agents use the flags above and `--json`.
  Decided, not built: the flag says `toolchain.not_built_yet` until it
  comes with the rest of area 14's tooling. `--keep-failed` is built with
  the test database (chunk 3.1).
- A failed test's rerun command and every path a run prints work from the
  folder the command ran in: run from outside the project, they start with
  the project as it was typed (`examples/07-testing/domains/…`). With
  `--failed`, when every test that failed last time is gone, there is
  nothing to rerun and the run passes.
- The editor shows a run button above each test and the last result beside
  it (language server).
- There is no `test.only` in source: focusing is a command-line choice, so a
  focused test is never committed by accident.

## The test database

Decided in area 5: every environment runs the same engine, PostgreSQL unless
the application declares otherwise, so tests run on PostgreSQL too. Each test
gets its own database at the latest migration, empty except for what its
fixtures create. Nothing about this appears in source; it is the job of
`jadpo test` (question 8):

- `jadpo test` starts a disposable local PostgreSQL server for the run, or
  uses the one named by `JADPO_TEST_DATABASE_URL` (in CI, for example).
- It builds one template database from the migrations and copies it for
  each test (`CREATE DATABASE … TEMPLATE`), which takes tens of milliseconds
  and lets tests run in parallel.
- Each test's actions commit for real, so after-commit work (events) and
  concurrent-change checks behave as in production. A failed test's database
  can be kept for inspection with `--keep-failed`.

The options compared (template copies, PGlite, a transaction rolled back per
test, Testcontainers) are in
[the testing proposals](research/13-testing-proposals.md).

## Caches and events

Area 5 decided that caches are copies used only by display reads from
routes, and area 1 that events run after the action that emits them commits.
In a test (question 8 for the database, question 9 for these):

- **Events** are delivered after each step of the test commits and before
  the next statement runs, at the test's clock, until no events are left.
  A test checks their effects, such as a sent reminder in `requests`. A
  subscriber that fails fails the test; retries are area 12.
  Built in chunk 6.11, as below.
- **Caches** are on and start empty for each test, in the application's real
  cache store, so a route test sees the same invalidation as production.
  Tests cannot read, fill or clear a cache directly. Actions never read
  caches, so only route tests meet them.

What each step emits is delivered as area 12, question 15 decides
(chosen in chunk 6.11):

- A step is a call of an action or fixture, a route test's request, a
  schedule's `.run()` or `clock.advance(...)`. Once it comes back, what
  is due in the test's outbox is delivered as the runner delivers it
  anywhere, as the system limited to its event's tenant, and then what
  those subscribers emit in turn, until nothing is due.
- Subscribers run in an order shuffled by the run's seed, the one the
  report prints and `--seed` gives again, mixed with the test's own
  identity: each delivery is drawn from all those due at that moment, so
  a test that relies on the order its subscribers run in fails under
  some seed, and rerunning it with that seed fails the same way.
- A delivery whose subscriber fails in a way worth trying again waits
  its full backoff, 30 seconds and then twice as long each time, since a
  test has no randomness to draw jitter from. The test's clock moves only
  when the test moves it, so the retry runs once `clock.advance` reaches
  it.
- A delivery that fails for good, with a failure not worth retrying, on
  its last attempt, or with a fault, ends the test there as
  [`test.subscriber_failed`](#test-subscriber-failed), naming the event,
  the subscriber and the attempt. What a route's `settle:` delivers
  while its request waits belongs to the request, and the response says
  how it ended (area 7, "Waiting for follow-on work").
- A batch run a step starts is left as it was started; batches in tests
  come with question 18's.

A fake failing a subscriber's call to a provider is retried and given
up the same way: proved on chunk 7.0c's fakes, with the test's host
wrapped in one; a test scripts it with `fakes:` once roadmap 7.2
generates them. A subscriber's own failure worth retrying tests its
retries already. Here placing an order emits
`OrderPlaced`, and `cook_placed` cooks it once a chef is on shift:

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
retrying, so it waits 30 seconds; starting the shift doesn't move the
clock, and the retry runs once `clock.advance(30s)` reaches it.

## Errors

> The six `test.*` codes run on the new compiler, with `jadpo test`'s real report, and so do `semantic.fake_invalid` (chunk 7.2d) and `type.test_clock_context` (chunk 5.2). The other `check` errors after them (the old service fakes and configuration fixtures) predate the decisions above and are rewritten when areas 10 and 11 are built (roadmap Phases 5 and 7).

Every error this area can produce, with an example that triggers it:
first the six codes a failed test gets from `jadpo test`, each with its
report (run with `--seed 1`), then the `jadpo check` errors.

### `test.assertion_failed`

An `assert` whose condition is false, or an `assert … fails` whose call
returned or failed differently. Records are compared field by field:

```jadpo check project=e-assertion-failed file=todos.jadpo hidden
type TodoTitle = Text {
    min_length: 1
    max_length: 200
}

type Priority = Int {
    min: 1
    max: 9
}

type TodoDraft = Object {
    title: TodoTitle
    priority: Priority
}

function draft(title: TodoTitle) -> TodoDraft {
    return TodoDraft {
        title: title
        priority: 3
    }
}
```

```jadpo check project=e-assertion-failed file=todos.test.jadpo expect=test.assertion_failed
test "a new draft has normal priority" {
    var fresh: TodoDraft = draft("Buy milk")
    assert fresh == TodoDraft {
        title: "Buy milk"
        priority: 2
    }
}
```

```text output
FAIL  a new draft has normal priority
 --> todos.test.jadpo:3

  assert fresh == TodoDraft { ...
         |        |
         |        TodoDraft { title: "Buy milk", priority: 2 }
         TodoDraft { title: "Buy milk", priority: 3 }

  expected: TodoDraft { title: "Buy milk", priority: 2 }
  actual: TodoDraft { title: "Buy milk", priority: 3 }
  diff: ~ priority: expected 2, actual 3

  rule: test.assertion_failed
  rerun: jadpo test todos.test.jadpo:3 --seed 1

test failed: 1 failed, 1 pure test
```

### `test.unexpected_failure`

A call under `attempt` failed, so the test ended with a failure it didn't
check for:

```jadpo check project=e-unexpected-failure file=priorities.jadpo hidden
type Priority = Int {
    min: 1
    max: 9
}

type PriorityTooHigh = Failure {
    kind: FailureKind.invalid_value
}

action raise_priority(priority: Priority) fails PriorityTooHigh -> Priority {
    var higher: Priority = attempt Priority(priority + 1) {
        max: PriorityTooHigh
    }
    return higher
}
```

```jadpo check project=e-unexpected-failure file=priorities.test.jadpo expect=test.unexpected_failure
test "the top priority can still be raised" {
    var bumped: Priority = attempt raise_priority(9)
    assert bumped == 9
}
```

```text output
FAIL  the top priority can still be raised
 --> priorities.test.jadpo:2

  failure: `PriorityTooHigh` from action raise_priority
  at: priorities.jadpo:11:36
   |
11 |     var higher: Priority = attempt Priority(priority + 1) {
   |                                    ^^^^^^^^^^^^^^^^^^^^^^ breaks `max: 9`
  reason: The value built here breaks `max: 9`, so `attempt` raises the failure its block maps that rule to.
  code: priority_too_high
  kind: FailureKind.invalid_value
  path: test "the top priority can still be raised" -> action raise_priority

  rule: test.unexpected_failure
  rerun: jadpo test priorities.test.jadpo:2 --seed 1

test failed: 1 failed, 1 action test
```

### `test.fault`

The runtime stopped the test with a fault (area 4), here an overflow:

```jadpo check project=e-test-fault file=plans.jadpo hidden
type Effort = Int {
    min: 0
}

function plan_effort(per_todo: Effort, todos: Effort) -> Effort {
    return Effort(per_todo * todos)
}
```

```jadpo check project=e-test-fault file=plans.test.jadpo expect=test.fault
test "a long plan still adds up" {
    var per_todo: Effort = 4611686018427387904
    assert plan_effort(per_todo, 2) > 0
}
```

```text output
FAIL  a long plan still adds up
 --> plans.test.jadpo:3

  fault: int_overflow in function plan_effort
  at: plans.jadpo:6:19
  |
6 |     return Effort(per_todo * todos)
  |                   ^^^^^^^^^^^^^^^^ out of range
  reason: `per_todo * todos` gives a whole number outside `Int`'s range, -9223372036854775808 to 9223372036854775807; Jadpo stops rather than wrap around.
  next: Bound the values that feed it with `max`, so the result always fits (agent)
  stack: plans.jadpo:6       function plan_effort
         plans.test.jadpo:3  test "a long plan still adds up"

  rule: test.fault
  rerun: jadpo test plans.test.jadpo:3 --seed 1

test failed: 1 failed, 1 pure test
```

### `test.subscriber_failed`

A delivery between the test's steps failed for good (chosen in chunk
6.11), or stopped as `diverged`, its rerun no longer matching the record
of its steps (chunk 7.5d): here its subscriber's action refuses with a
failure not worth retrying. The report names the event, the subscriber and the attempt,
then the failure as `test.unexpected_failure` shows one, at the step
whose events were being delivered:

```jadpo check project=e-subscriber-failed file=app.jadpo hidden
application {}
```

```jadpo check project=e-subscriber-failed file=domains/post/types/address.jadpo hidden
/// Where a parcel goes.
type Address = Text {
    min_length: 1
    max_length: 200
}
```

```jadpo check project=e-subscriber-failed file=domains/post/entities/parcel.jadpo hidden
use post/types { Address }

/// A parcel handed in at the counter.
entity Parcel {
    id: Uuid
    address: Address
    labelled: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}
```

```jadpo check project=e-subscriber-failed file=domains/post/events/parcel_handed_in.jadpo hidden
use post/entities { Parcel }
use post/types { Address }

/// A parcel was handed in.
type ParcelHandedIn = Event {
    parcel_id: Parcel.id
    address: Address
}
```

```jadpo check project=e-subscriber-failed file=domains/post/events/events.jadpo_record hidden
// Generated by Jadpo from the events of domain post; update it with `jadpo records update`, never by hand.
record events 1

event ParcelHandedIn
    address: Text { min_length: 1, max_length: 200 }
    parcel_id: Uuid
```

```jadpo check project=e-subscriber-failed file=domains/post/failures/no_such_place.jadpo hidden
/// No round goes there.
type NoSuchPlace = Failure {
    kind: FailureKind.rejected
    message: "We don't deliver there."
}
```

```jadpo check project=e-subscriber-failed file=domains/post/actions/hand_in.jadpo hidden
use post/entities { Parcel }
use post/types { Address }
use post/events { ParcelHandedIn }

/// Takes a parcel in; labelling it follows in a subscriber.
action hand_in(address: Address) -> Parcel {
    var parcel: Parcel = Parcel.create {
        set: {
            address: address
            labelled: false
        }
    }
    emit ParcelHandedIn {
        parcel_id: parcel.id
        address: parcel.address
    }
    return parcel
}
```

```jadpo check project=e-subscriber-failed file=domains/post/actions/label.jadpo
use post/entities { Parcel, ParcelNotFound }
use post/types { Address }
use post/failures { NoSuchPlace }

/// Labels a parcel for its round.
action label(parcel_id: Parcel.id, address: Address) fails ParcelNotFound, NoSuchPlace -> Unit {
    if (address == "The Moon") {
        reject NoSuchPlace {}
    }
    attempt Parcel.update(parcel_id) {
        set: {
            labelled: true
        }
    }
}
```

```jadpo check project=e-subscriber-failed file=domains/post/subscribers/label_handed_in.jadpo migrate=initial
use post/actions { label }
use post/events { ParcelHandedIn }

/// Labels each parcel handed in.
subscriber label_handed_in {
    on: ParcelHandedIn
    run: label(event.parcel_id, event.address)
}
```

```jadpo check project=e-subscriber-failed file=domains/post/actions/hand_in.test.jadpo expect=test.subscriber_failed
use post/entities { Parcel }

test "a parcel for the moon is taken in" {
    var parcel: Parcel = hand_in("The Moon")
    assert parcel.labelled == false
}
```

```text output
FAIL  a parcel for the moon is taken in
 --> domains/post/actions/hand_in.test.jadpo:4

  delivery: `ParcelHandedIn` to subscriber `label_handed_in` failed for good on attempt 1 of 10
  failure: `NoSuchPlace` from action label
  at: domains/post/actions/label.jadpo:8:9
  |
8 |         reject NoSuchPlace {}
  |         ^^^^^^^^^^^^^^^^^^^^^ rejected here
  code: no_such_place
  kind: FailureKind.rejected
  message: We don't deliver there.
  path: test "a parcel for the moon is taken in" -> action label

  rule: test.subscriber_failed
  rerun: jadpo test domains/post/actions/hand_in.test.jadpo:4 --seed 1

test failed: 1 failed, 1 action test
```

### `test.fake_exhausted`

A test called a faked provider operation more times than its `fakes:`
scripts answers (built in chunk 7.2d). The call past the script was
answered with the generated default, so whatever the test did next
followed from a call it didn't expect; the report names the operation,
at the line of its first scripted answer:

```jadpo check project=e-fake-exhausted file=app.jadpo hidden
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

```jadpo check project=e-fake-exhausted file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
```

```jadpo check project=e-fake-exhausted file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-fake-exhausted file=shared/providers/acme_couriers/v1/provider.jadpo hidden
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

```jadpo check project=e-fake-exhausted file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
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

```jadpo check project=e-fake-exhausted file=shared/providers/acme_couriers/v1/types/postcode.jadpo hidden
/// Where a collection is.
public type Postcode = Text {
    max_length: 8
}
```

```jadpo check project=e-fake-exhausted file=shared/providers/acme_couriers/v1/types/booking_id.jadpo hidden
/// The courier's reference for a booking.
public type BookingId = Text {
    max_length: 40
}
```

```jadpo check project=e-fake-exhausted file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Postcode
}
```

```jadpo check project=e-fake-exhausted file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: BookingId
}
```

```jadpo check project=e-fake-exhausted file=shared/providers/acme_couriers/v1/failures/slot_taken.jadpo hidden
/// The slot went before the booking arrived.
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "That slot is taken."
}
```

```jadpo check project=e-fake-exhausted file=domains/shipping/failures/slot_gone.jadpo hidden
/// The slot the customer chose has gone.
type SlotGone = Failure {
    kind: FailureKind.conflict
    message: "That slot has gone; choose another."
}
```

```jadpo check project=e-fake-exhausted file=domains/shipping/actions/book.jadpo hidden
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

```jadpo check project=e-fake-exhausted file=domains/shipping/actions/book.test.jadpo expect=test.fake_exhausted
use app { couriers }
use shipping/failures { SlotGone }

test "a taken slot is the app's own failure" {
    fakes: {
        couriers.bookings.create: [slot_taken]
    }
    assert book("N1 9GU") fails SlotGone
    assert book("SW1A 1AA") fails SlotGone
}
```

```text output
FAIL  a taken slot is the app's own failure
 --> domains/shipping/actions/book.test.jadpo:6

  fake: `couriers.bookings.create` was called twice, and its `fakes:` scripts 1 answer
  reason: A scripted operation answers each call with the next scripted answer, so a call past the last means the code calls it more often than the test says; that call was answered with the generated default.
  next: Script an answer for every call of `couriers.bookings.create`, or find why the code calls it again (agent)

  rule: test.fake_exhausted
  rerun: jadpo test domains/shipping/actions/book.test.jadpo:6 --seed 1

test failed: 1 failed, 1 action test
```

### `test.fake_unused`

A test that otherwise passed left answers its `fakes:` scripts for an
operation untaken (built in chunk 7.2d): the code made fewer calls than
the test says.

```jadpo check project=e-fake-unused file=app.jadpo hidden
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

```jadpo check project=e-fake-unused file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
```

```jadpo check project=e-fake-unused file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-fake-unused file=shared/providers/acme_couriers/v1/provider.jadpo hidden
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

```jadpo check project=e-fake-unused file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
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

```jadpo check project=e-fake-unused file=shared/providers/acme_couriers/v1/types/postcode.jadpo hidden
/// Where a collection is.
public type Postcode = Text {
    max_length: 8
}
```

```jadpo check project=e-fake-unused file=shared/providers/acme_couriers/v1/types/booking_id.jadpo hidden
/// The courier's reference for a booking.
public type BookingId = Text {
    max_length: 40
}
```

```jadpo check project=e-fake-unused file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Postcode
}
```

```jadpo check project=e-fake-unused file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: BookingId
}
```

```jadpo check project=e-fake-unused file=shared/providers/acme_couriers/v1/failures/slot_taken.jadpo hidden
/// The slot went before the booking arrived.
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "That slot is taken."
}
```

```jadpo check project=e-fake-unused file=domains/shipping/failures/slot_gone.jadpo hidden
/// The slot the customer chose has gone.
type SlotGone = Failure {
    kind: FailureKind.conflict
    message: "That slot has gone; choose another."
}
```

```jadpo check project=e-fake-unused file=domains/shipping/actions/book.jadpo hidden
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

```jadpo check project=e-fake-unused file=domains/shipping/actions/book.test.jadpo expect=test.fake_unused
use app { couriers }
use shipping/failures { SlotGone }

test "a taken slot is the app's own failure" {
    fakes: {
        couriers.bookings.create: [slot_taken, unavailable]
    }
    assert book("N1 9GU") fails SlotGone
}
```

```text output
FAIL  a taken slot is the app's own failure
 --> domains/shipping/actions/book.test.jadpo:6

  fake: `couriers.bookings.create` was called once, and its `fakes:` scripts 2 answers
  reason: Each scripted answer is for a call the test expects, so one left over means the code calls the operation less often than the test says.
  next: Remove the answers no call takes, or find why the code doesn't call `couriers.bookings.create` (agent)

  rule: test.fake_unused
  rerun: jadpo test domains/shipping/actions/book.test.jadpo:6 --seed 1

test failed: 1 failed, 1 action test
```

### `semantic.fake_invalid`

A test's `fakes:` scripts something no provider operation answers (built
in chunk 7.2d): an operation the app's connections don't have, a name
that is none of the operation's failures' codes or a condition, an
operation scripted twice or with no answers, or a value for one that
gives nothing. A value of the wrong type is the usual `type.mismatch`.

```jadpo check project=e-fake-invalid file=app.jadpo hidden
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

```jadpo check project=e-fake-invalid file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
```

```jadpo check project=e-fake-invalid file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-fake-invalid file=shared/providers/acme_couriers/v1/provider.jadpo hidden
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

```jadpo check project=e-fake-invalid file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
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

```jadpo check project=e-fake-invalid file=shared/providers/acme_couriers/v1/types/postcode.jadpo hidden
/// Where a collection is.
public type Postcode = Text {
    max_length: 8
}
```

```jadpo check project=e-fake-invalid file=shared/providers/acme_couriers/v1/types/booking_id.jadpo hidden
/// The courier's reference for a booking.
public type BookingId = Text {
    max_length: 40
}
```

```jadpo check project=e-fake-invalid file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Postcode
}
```

```jadpo check project=e-fake-invalid file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: BookingId
}
```

```jadpo check project=e-fake-invalid file=shared/providers/acme_couriers/v1/failures/slot_taken.jadpo hidden
/// The slot went before the booking arrived.
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "That slot is taken."
}
```

```jadpo check project=e-fake-invalid file=domains/shipping/failures/slot_gone.jadpo hidden
/// The slot the customer chose has gone.
type SlotGone = Failure {
    kind: FailureKind.conflict
    message: "That slot has gone; choose another."
}
```

```jadpo check project=e-fake-invalid file=domains/shipping/actions/book.jadpo hidden
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

```jadpo check project=e-fake-invalid file=domains/shipping/actions/book.test.jadpo expect=semantic.fake_invalid
use app { couriers }
use shipping/failures { SlotGone }

test "a taken slot is the app's own failure" {
    fakes: {
        couriers.bookings.create: [slot_takn]
    }
    assert book("N1 9GU") fails SlotGone
}
```

```text output
error: `slot_takn` isn't an answer `couriers.bookings.create` gives
 --> domains/shipping/actions/book.test.jadpo:6:36
  |
6 |         couriers.bookings.create: [slot_takn]
  |                                    ^^^^^^^^^ not a failure or a condition
  reason: A scripted answer is a value of `AcmeCouriers.Booking`, one of `couriers.bookings.create`'s failures by its code (`slot_taken`), or a condition: `unavailable`, `lost`, `in_use` or `misconfigured`.
  next: Use `slot_taken` instead of `slot_takn` (agent)
  rule: semantic.fake_invalid
  help: docs/language/13-testing.html#semantic-fake-invalid

check failed: 1 error, 13 files
```

### `semantic.request_route_unknown`

A request names a route the project has, by its method and its path as
`jadpo routes` prints it:

```jadpo check project=e-request-route expect=semantic.request_route_unknown
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

test "a memo comes back" {
    var response: Response = request {
        method: HttpMethod.post
        path: "/memo"
        input: AddMemo {
            text: "Call the plumber"
        }
    }
    assert response.status == 201
}
```

```text output
error: No route answers `POST /memo`
 --> main.jadpo:40:15
   |
40 |         path: "/memo"
   |               ^^^^^^^ no such route
  reason: A request names a route by its method and its path as `jadpo routes` prints it, placeholders by name; the `POST` routes are `/memos`.
  next: Write the path of the route this test exercises, as `jadpo routes` lists it (agent)
  rule: semantic.request_route_unknown
  help: docs/language/13-testing.html#semantic-request-route-unknown

check failed: 1 error, 1 file
```

### `semantic.request_invalid`

A request gives what its route takes: each placeholder in `params:` and
the route's `input:`:

```jadpo check project=e-request-invalid expect=semantic.request_invalid
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

test "a memo comes back" {
    var response: Response = request {
        method: HttpMethod.post
        path: "/memos"
    }
    assert response.status == 201
}
```

```text output
error: `POST /memos` takes `input:`, a `AddMemo`
 --> main.jadpo:38:30
   |
38 |     var response: Response = request {
   |                              ^^^^^^^ doesn't fit the route
  reason: A request gives the route's method, its path as `jadpo routes` prints it, each placeholder in `params:` and, when the route takes one, its `input:`; a request but a `GET` may give the `Idempotency-Key` it sends in `idempotency_key:`.
  next: Give the request what its route takes (agent)
  rule: semantic.request_invalid
  help: docs/language/13-testing.html#semantic-request-invalid

check failed: 1 error, 1 file
```

### `semantic.test_service_fake_required`

A test reaches `ReminderMail.send_overdue_reminder` without a fake:

```jadpo project=e-fake-required file=tests/assurance/service-reference-mail-v0.1.json expect=semantic.test_service_fake_required hidden
{
  "openapi": "3.1.0",
  "info": {
    "title": "Jadpo local reference mail provider",
    "version": "0.1.0"
  },
  "servers": [
    { "url": "https://mail.example.invalid" }
  ],
  "paths": {
    "/v1/messages": {
      "post": {
        "operationId": "send_overdue_reminder",
        "parameters": [
          {
            "name": "Idempotency-Key",
            "in": "header",
            "required": true,
            "schema": { "type": "string", "format": "uuid" }
          }
        ],
        "security": [ { "providerCredential": [] } ],
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["idempotency_key", "from", "to", "todo_title", "due_at"],
                "properties": {
                  "idempotency_key": { "type": "string", "format": "uuid" },
                  "from": { "type": "string", "format": "email" },
                  "to": { "type": "string", "format": "email" },
                  "todo_title": { "type": "string", "minLength": 1, "maxLength": 200 },
                  "due_at": { "type": ["string", "null"], "format": "date-time" }
                }
              }
            }
          }
        },
        "responses": {
          "202": {
            "description": "Accepted or deduplicated by the same key and payload",
            "headers": {
              "Idempotency-Key": {
                "required": true,
                "schema": { "type": "string", "format": "uuid" }
              }
            },
            "content": {
              "application/json": {
                "schema": {
                  "type": "object",
                  "additionalProperties": false,
                  "required": ["accepted_at"],
                  "properties": {
                    "accepted_at": { "type": "string", "format": "date-time" }
                  }
                }
              }
            }
          },
          "400": { "$ref": "#/components/responses/InvalidRecipient" },
          "401": { "$ref": "#/components/responses/Authentication" },
          "409": { "$ref": "#/components/responses/IdempotencyConflict" },
          "429": { "$ref": "#/components/responses/RateLimited" }
        }
      }
    }
  },
  "components": {
    "securitySchemes": {
      "providerCredential": { "type": "http", "scheme": "bearer" }
    },
    "responses": {
      "InvalidRecipient": {
        "description": "Recipient rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/InvalidRecipient" } } }
      },
      "Authentication": {
        "description": "Credential rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Authentication" } } }
      },
      "IdempotencyConflict": {
        "description": "Key reused with a different payload; no new acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/IdempotencyConflict" } } }
      },
      "RateLimited": {
        "description": "Request refused before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/RateLimited" } } }
      }
    },
    "schemas": {
      "InvalidRecipient": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "invalid_recipient" } }
      },
      "Authentication": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "authentication" } }
      },
      "IdempotencyConflict": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "idempotency_conflict" } }
      },
      "RateLimited": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "rate_limited" } }
      }
    }
  }
}
```

```jadpo project=e-fake-required file=mail.jadpo expect=semantic.test_service_fake_required hidden
config TestConfiguration {
    mail_api_key: Text {
        binding: "MAIL_API_KEY"
        secret: true
    }
}

type ReminderIntentId = Uuid
type TodoTitle = Text {
    min_length: 1
    max_length: 200
}

value ReminderMessage {
    idempotency_key: ReminderIntentId
    from: Email
    to: Email
    todo_title: TodoTitle
    due_at: Instant?
}

value ReminderReceipt {
    accepted_at: Instant
}

failure ReminderRecipientRejected {
    kind: Rejected
    code: "recipient_rejected"
    message: "Recipient rejected."
}

failure ReminderTemporarilyUnavailable {
    kind: Unavailable
    code: "temporarily_unavailable"
    message: "Temporarily unavailable."
}

service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}

action deliver(mail: ReminderMessage)
    fails ReminderRecipientRejected, ReminderTemporarilyUnavailable, Misconfigured, Unavailable, OutcomeUnknown
    -> ReminderReceipt
{
    return attempt ReminderMail.send_overdue_reminder(mail)
}

entity ServiceFakeIdentity {
    id: Uuid { generated: identity }
    label: Text

    identity: id

    persistence {
        store: primary
        role: authority
    }

    action create_test_message() -> ReminderMessage {
        var created = attempt create ServiceFakeIdentity {
            label: ServiceFakeIdentity.label("fixture-only")
        }
        return ReminderMessage {
            idempotency_key: ReminderMessage.idempotency_key(created.id)
            from: ReminderMessage.from("reminders@example.test")
            to: ReminderMessage.to("person@example.test")
            todo_title: ReminderMessage.todo_title("Prepare release")
            due_at: none
        }
    }
}
```

```jadpo project=e-fake-required expect=semantic.test_service_fake_required
test "a reminder is delivered" {
    var mail = call ServiceFakeIdentity.create_test_message()
    var receipt = attempt deliver(mail)
    assert receipt.accepted_at == Instant("2026-01-15T12:00:00Z")
}
```

### `type.test_call_context`

Application code uses a call form that belongs to tests:

```jadpo project=e-call-context expect=type.test_call_context
value ClockReading {
    value: Instant
}

action read_clock() -> ClockReading {
    return ClockReading {
        value: clock.now
    }
}

action misuse_test_boundary() -> ClockReading {
    return call read_clock()
}
```

### `type.test_clock_context`

Application code cannot move the clock:

```jadpo check project=e-clock-context expect=type.test_clock_context
type ShiftEnd = Instant {}

action end_shift() -> ShiftEnd {
    clock.advance(5m)
    return ShiftEnd(clock.now)
}
```

```text output
error: An action can't move the clock
 --> main.jadpo:4:5
  |
4 |     clock.advance(5m)
  |     ^^^^^^^^^^^^^ moves the clock
  reason: Only a test moves its clock: elsewhere the time is the operation's `clock.now`, fixed while it runs, and a later request or schedule run reads a later one.
  next: Move the clock in a test with `clock.advance(...)`, or remove this line (agent)
  rule: type.test_clock_context
  help: docs/language/13-testing.html#type-test-clock-context

check failed: 1 error, 1 file
```

### `configuration.field_unknown`

A test's configuration names a field that does not exist:

```jadpo project=e-field-unknown expect=configuration.field_unknown
type Label = Text {
    min_length: 3
}

config Settings {
    label: Label {
        binding: "LABEL"
    }
}

fixture local {
    config {
        label: "valid"
        lable: "valid"
    }
}
```

### `configuration.fixture_duplicate`

```jadpo project=e-fixture-duplicate expect=configuration.fixture_duplicate
type Label = Text {
    min_length: 3
}

config Settings {
    label: Label {
        binding: "LABEL"
    }
}

fixture local {
    config {
        label: "first"
        label: "second"
    }
}
```

### `configuration.fixture_secret_required`

```jadpo project=e-fixture-secret-required expect=configuration.fixture_secret_required
type ApiKey = Text {
    min_length: 8
}

config ApplicationConfiguration {
    api_key: ApiKey {
        binding: "API_KEY"
        secret: true
    }
}

fixture exposed_secret {
    config {
        api_key: "test-only-api-key"
    }
}
```

### `configuration.fixture_secret_unexpected`

```jadpo project=e-fixture-secret-unexpected expect=configuration.fixture_secret_unexpected
type Label = Text {}

config Settings {
    label: Label {
        binding: "LABEL"
    }
}

fixture secret_to_public {
    config {
        label: secret("fixture-only-value")
    }
}
```

### `configuration.fixture_value_invalid`

```jadpo project=e-fixture-value-invalid expect=configuration.fixture_value_invalid
type RequestTimeout = Duration {}

config ApplicationConfiguration {
    request_timeout: RequestTimeout {
        binding: "REQUEST_TIMEOUT"
    }
}

fixture invalid_configuration {
    config {
        request_timeout: "eventually"
    }
}
```

### `configuration.fixture_value_missing`

```jadpo project=e-fixture-value-missing expect=configuration.fixture_value_missing
type Label = Text {
    min_length: 3
}

config Settings {
    label: Label {
        binding: "LABEL"
    }
}

fixture local {
    config {
    }
}
```

### `syntax.assertion_form`

A test checks with `assert condition`: Rust's `assert_eq!`, JUnit's `assertEquals`, Python's `assertEqual`, Jest's `expect(...).toBe(...)` and Node's `assert.equal` are habits from other languages (decision register, 2026-10-15), one error each whose edit writes the `assert`.

```jadpo check project=e-habit-assertion expect=syntax.assertion_form
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    return count
}

test "same gives back its count" {
    assert_eq!(same(2), 2)
}
```

```text output
error: Jadpo writes `assert same(2) == 2`, not `assert_eq!(same(2), 2)`
 --> main.jadpo:10:5
   |
10 |     assert_eq!(same(2), 2)
   |     ^^^^^^^^^^^^^^^^^^^^^^ write `assert same(2) == 2`
  reason: `assert_eq!` checks a test's values in Rust, `assertEquals` in JUnit, `assertEqual` in Python's unittest, `expect(…).toBe(…)` in Jest and `assert.equal` in Node; a Jadpo test checks with `assert condition`.
  next: Write `assert same(2) == 2` (agent)
  rule: syntax.assertion_form
  help: docs/language/13-testing.html#syntax-assertion-form

check failed: 1 error, 1 file
```

## Review

### Status

Each row says what the new compiler does: built (with the chunk, checked
by `jadpo check` or run by `jadpo test`), or decided and not built yet,
with when it comes.

| Feature | Status |
|---|---|
| Pure and action tests | Work in the new compiler (run, chunk 2.2): `jadpo test` runs `test "…" { }` and `assert` in the interpreter, for tests that call functions and actions. Since chunk 2.10 each test gets its own environment (its seed and clock, and the places its database and fakes will take), tests run on a thread per core, and the results are reported in the order seed's order. Each action or fixture a test calls commits on its own, through the runtime's action boundary; an action it calls joins its transaction. |
| Kind of test | Works (run, chunk 2.2): the kind comes from what a test calls (an action, query or fixture makes it an action test, a `request` a route test, and since chunk 6.2c a schedule's `.run()` a schedule test), and the footer and `--json` count each kind. |
| Schedule tests | Work (run, chunk 6.2c): `schedule.run()` in a test runs the schedule once at the test's clock, its action called as the system with no tenant, with its own database like an action test; `--kind schedule` runs only these; anywhere but a test it is `semantic.not_callable`. Fakes for what a run calls come with packages ("Writing a test"). |
| Route tests | Work (run, chunk 4.1): `request { method: path: params: input: }` sends a request through the runtime's HTTP component and gives a `Response` with `status`, `code` and the route's typed `output`; a route test gets its own database like an action test, and a defect behind a 500 ends it as `test.fault`. `as:` signs in a user through a real browser session or API token (chunks 4.3, 4.3b), and a service through a real service key (chunk 4.3b). Since chunk 4.7 every request but a `GET` sends a fresh `Idempotency-Key`, or the one `idempotency_key:` gives ([Idempotency keys](#idempotency-keys)). Since chunk 4.9 an optional field the `input:` leaves out is left out of the body, so a `patch:` keeps it. Since chunk 5.5v6 a streaming route's `Response` has `pieces` in place of `output`, its events read to the last. |
| Assertion messages | Work (run, chunk 2.2): a failed `assert` reports its file and line, its source with each part's value hung beneath it, expected and actual, a field-by-field diff for records, lists, sets and maps, a stable code and the rerun command with the order seed (see [Errors](#errors)). The trace, fakes, kept database and clock in the decided report come with their phases and are left out until then. A statement over several lines shows its first line; `--json` has all of it. |
| Checking a failure | Works (run, chunk 2.2): `assert call(…) fails Failure`; when the call returns or fails differently, the report shows the value, or the other failure with its code, kind, fields and where it was raised. A failure under `attempt` ends the test as `test.unexpected_failure`, and a fault as `test.fault`, each with its `.jadpo` frames. |
| Events | Work (run, chunk 6.11): what each step emits is delivered once it comes back and before the next statement, at the test's clock, until nothing is due, in an order the run's seed shuffles; a retry waits its full backoff until `clock.advance` reaches it, and a delivery that fails for good ends the test as `test.subscriber_failed` ("Caches and events"), as since chunk 7.5d does one that stops as `diverged`. A fake failing a subscriber's call is retried and given up the same way on 7.0c's fakes; a test writes one with `fakes:` once 7.2 generates them. |
| Clock | Works (run, chunk 5.2): a test's clock starts at its `clock:`, or at 2000-01-01T00:00:00Z without one, and `clock.advance(...)` moves it on for the calls after it; only a test moves it (`type.test_clock_context`, a fixture and a seed included), and a move backwards is the fault `clock_backward`. |
| Configuration | Works (check and run, chunk 5.1): `test:` values on fields, defaults, generated secrets and a test's own `config:`; a request sent through a route is not followed for the compile-time check that every setting a test reads has a value, and a read with none then ends the test as a fault naming the setting. |
| Integration fakes | Work for provider packages (run, chunk 7.2d): every provider call a test reaches answers through its package's fake, with the operation's generated default unless the test's `fakes:` scripts its answers, each checked against the package (`semantic.fake_invalid`); a script the calls run past is `test.fake_exhausted`, one with answers left `test.fake_unused` ("Fakes for integrations"). Since chunk 7.5d package fakes keep keys, and a test that reaches an action with several changing calls runs again with a crash at each step boundary ("Provider calls and capabilities in tests"). `email.sent` built in chunk 7.3c (area 16): every email a test sends is caught, rendered whole, a mail package's included, and read as a `List<SentEmail>`. Not built: `.requests` (built for the `ai` fake, `ai.<prompt>.requests`, chunk 5.5v3), and the capability fakes but `ai`'s (chunk 5.5v3), grants, documents and batches, which come with those features (areas 12 and 16). |
| Database | Works (run, chunk 3.1): `jadpo test` gives each action test in a project with tables its own copy of a template built from the migrations, on the server `JADPO_TEST_DATABASE_URL` names or a disposable one started from the machine's PostgreSQL tools; copies are prepared in the background, and a test gets its copy the first time it uses a store. `--keep-failed` keeps a failed test's database and names it in the report. Since chunks 3.2 and 3.3 tests read and write records through actions, queries and fixtures; a concurrent change, which needs a second connection, is shown by the runtime's own tests. |
| Caller | Decided, not built: acting as a user comes with identity (area 8, roadmap 4.3). Until then tests run with no caller. |
| Placement | Built (chunk 1.3, checked): a test sits in `<name>.test.jadpo` beside its subject, or in root `tests/`; a test elsewhere is `layout.wrong_folder`, which names the `.test.jadpo` file beside it, and a test file with nothing beside it is `layout.test_without_subject`. |
| Watch mode | Works (run, chunk 2.3): `jadpo test --watch` runs the tests asked for, then on every save the ones that reach what changed, through the graph's digests and `calls` and `uses` edges, on one long-lived database; a save no test reaches says `nothing to rerun`, a broken build shows its errors, and the next good save runs everything changed since the last run, or says `errors fixed` when no test reaches it; a removed file is a change like any other. Run from outside the project, its paths start with the project as typed. It ends when nothing reads its output any more, as when piped into `head`. A save shows its results in about 0.1 s on a 230-file project, and the gate fails above 1.25 s. The keys in the decided screen (`enter`, `a`, `f`) wait for `--interactive`, which says `toolchain.not_built_yet`; ctrl-c quits. |
| Fixtures | Work (run): a fixture is checked like an action and called by name from tests, fixtures and seeds; an action calling one is `semantic.not_callable`. A `public` fixture is called through its domain from other domains' tests, fixtures and seeds and from root `tests/`; a private one from there is `visibility.private_cross_domain`. Seeds are checked but `jadpo seed` is not built. |
| Golden application contract | Built (chunk 9.1d2b): the shared black-box harness in `tools/golden-todo/` executes all 44 versioned application cases on generated native code and compiled workerd, with real credential verification, raw invalid inputs, physical SQL observation, persisted-state assertions, controlled clocks and provider faults. These checks supplement authored Jadpo tests; they add no application-facing test controls. Full-gate ownership is native in `compiled`, workerd in `wasm32`, and verifier negative controls in `lint`. Independent whole-first-build review remains outstanding. |
| `test.no_tests` | Raised by `jadpo test` only (area 14), when the project has no tests or a file, line, name, kind or `--failed` picks none (run, chunk 2.2). |

### Consistency with the rest of the language

- **`fixture` means two things.** Today it is a test environment (clock,
   configuration, fakes); area 1 decided `fixtures/` holds shared test data.
   Questions 2 and 4 give the word one meaning: data.
- **`using` puts a setting in the header** (`test "…" using x {`), where
   other declarations keep settings in the body (syntax review #71).
- **Wrappers with no meaning of their own.** `fixed Instant("…")`,
   `Duration("PT5M")` and `secret("…")` restate what the field's type or
   declaration already says; area 2's literal typing makes them unnecessary
   (#72, #73), and durations are `5m` elsewhere (#56, area 10).
- **`advance clock by` is a sentence**, while time is read with `clock.now`
   (#75); `clock.advance(5m)` matches.
- **Fakes use `=>` and `accept`.** Area 2 removed `=>`, and `accept` is a
   keyword found nowhere else (#76).
- **`call`** is optional today and adds nothing once the kind of test comes
   from what it calls (#74).
- **SQLite in tests** contradicts area 5's one engine everywhere.
- **Checking a failure** needs a `match` trick, although area 4 made
   failures part of every signature.

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| The `request { }` form in route tests, the `Response` type, its `status`, `code` and typed output | 7 | Input to area 7 |
| `Principal.user(user)` in tests, fixtures and seeds, and `as:` signing in through the real authentication method (question 10); a service caller in tests | 8 | Built in chunk 4.3 for users and chunk 4.3b for services |
| Policy tests are ordinary tests: call as another user and `assert … fails` with the failure policy reports | 9 | Built in chunk 4.4: a call passing a caller runs as that caller; one passing none runs without policy |
| `test:` values on configuration fields, and the `5m` duration literal | 10 | Input to area 10 |
| Fake responses name the provider conditions in the integration's `failures:` block; checking a fake against the provider's contract or sandbox | 11 | Input to area 11 |
| `overdue_reminders.run()` in tests, subscriber delivery order, and retries of a failing subscriber in tests | 12 | Input to area 12 |
| Caches in tests once the Redis adapter exists | 6 | Input to area 6 |
| `lost` and `in_use` answers and crash points for several changing calls; batches in tests | 12 | Question 18 |
| One fake per capability; `bot_check_failed`; the placeholder PDF; PostgreSQL search in tests; `fixture_file` | 16 | Question 18 |
| Declaring OAuth grants in tests | 11 | Question 18 |
| `jadpo test` itself: starting PostgreSQL, template copies, parallel runs, `--kind`, `--keep-failed`, the report; `test.no_tests`, `test.failed` and `test.runtime_start_failed`, which only `jadpo test` raises | 14 | Input to area 14 |
| Tests the compiler generates from facts it already owns (bad input, failure envelopes, closed output) | 14 | Input to area 14 |
| `layout.test_without_subject` and test file contents | 1 | Follows from a decision |

### Open questions

1. **What can a test reach?** *Recommendation:* a test beside its subject
   can call anything in its own domain, including private actions and
   queries; a scenario in root `tests/` sees only public declarations and
   routes, as another domain would. Unit tests stay close to the code they
   test, and scenarios cannot depend on internals. This is the split Rust
   makes between unit tests inside a module and integration tests in
   `tests/`. *Decided 2026-10-13:* yes, as recommended.
2. **Where does a test's environment go?** Today it is a named fixture
   attached with `using`. *Recommendation:* settings at the top of the test
   body (`clock:`, `config:`, `fakes:`), each optional, with a fixed default
   clock; the word `fixture` moves to shared data (question 4). A reader sees
   everything that shapes the result in the test, and the clock default is
   never the real time. *Decided 2026-10-13:* yes, as recommended.
3. **How do tests get configuration?** Today every fixture lists every field
   and wraps secrets in `secret("…")`. *Recommendation:* a `test:` value on
   each field in `config.jadpo`, overridden per test with `config:`; secrets
   get a generated throwaway value per run and cannot be written in tests.
   Rails and Phoenix keep a test environment configuration the same way. *Decided 2026-10-13:* settled by area 10 question 11: `test:` values in `config.jadpo`, generated secrets, per-test override.
4. **What is a fixture?** *Recommendation:* a test-only action in
   `fixtures/` that creates data by calling the application's own actions
   and returns what tests need, called by name. Rows written straight into
   tables (Rails' YAML fixtures, Django's JSON fixtures) skip validation and
   policy and drift from the code; builders that go through the code
   (factory_bot, Elixir's ExMachina) are what those communities moved to.
   It is also how area 5 decided seeds work. *Decided 2026-10-13:* yes, as recommended.
5. **How do route and schedule tests call their boundary?** *Recommendation:*
   `request { method: HttpMethod.post path: "/todos" as: alice input: … }` returning a
   typed `Response`, through the real generated HTTP boundary without a
   network; and `overdue_reminders.run()`, which runs the schedule once at
   the test clock. Advancing the clock does not run schedules by itself, so
   adding a schedule never changes an unrelated test. *Decided 2026-10-13:* settled by areas 7 and 12: `request { }` returning `Response`, and `schedule.run()` under the test clock.
6. **How are results checked?** *Recommendation:* keep `assert condition`,
   report the line, the condition and both sides' values on failure (as
   pytest does), and add `assert call(…) fails FailureName` for expected
   failures. One assertion word, and failures are checked by name, as area 4
   lists them. *Decided 2026-10-13:* yes, as recommended.
7. **How are fakes written, and what if the code calls them a different
   number of times?** *Recommendation:* a `fakes:` setting mapping
   `Integration.operation` to a list of responses (output values or provider
   conditions), `.requests` to check what was sent, and the test fails when
   the list runs out or has responses left over, naming the operation. A
   wrong count is a mistake in the test or the code, not a provider outcome;
   the HTTP mocking library nock checks leftover responses the same way.
   See [the testing proposals](research/13-testing-proposals.md). *Decided 2026-10-13:* yes, updated for area 11: unscripted provider calls get the package fake's valid defaults; scripted answers via `fakes: { stripe.payment_intents.create: [...] }`; running out or leftovers fail the test.
8. **How do tests get a fast, isolated PostgreSQL?** *Recommendation:*
   `jadpo test` starts a disposable PostgreSQL (or uses a given one), builds a
   template database from the migrations once, and copies it for each test,
   running tests in parallel; actions commit for real. Rolling back one
   transaction per test is faster but breaks after-commit events and
   concurrency checks; PGlite needs no install but runs one connection at a
   time. Details in [the testing proposals](research/13-testing-proposals.md). *Decided 2026-10-13:* yes, as recommended.
9. **How do events and caches behave in tests?** *Recommendation:* events
   are delivered after each step commits, before the next statement, until
   none are left; caches are on and empty for each test, and tests cannot
   touch them. Tests then see what production does, deterministically.
   Django's `captureOnCommitCallbacks(execute=True)` runs after-commit work
   at the same point. *Decided 2026-10-13:* settled by areas 6 and 12: events delivered after each commit in a seeded shuffled order; an in-memory cache emptied between tests.
10. **How does a test act as a user?** Area 8 proposes passing the caller to
    actions as a parameter. *Recommendation:* tests, fixtures and seeds make
    one with `Principal.user(user)`, which application code cannot do; a
    route test signs in with `as:`, which runs the application's real
    authentication. Laravel's `actingAs` sets the user directly and skips the
    sign-in checks; signing in for real tests them too. *Decided 2026-10-13:* settled by area 8 question 10: `Principal.user(user)` in tests, fixtures and seeds; `as:` in route tests.
11. **How does a test act as a service?** (carried from area 8) Question 10
    covers users only. How does a test, fixture or seed make a service
    caller, and does a route test sign in with a service key? *Decided 2026-10-13:* `Principal.service(service)` in tests, fixtures and seeds; a route test signs in with a key from `Service.issue_key`.
12. **Can a test assert what a query costs?** (carried from area 6) Area 6
    bounds how many records and database queries a query may cost. Can a
    test assert those numbers, and how? *Decided 2026-10-13:* no runtime assertion; the compiler reports each query's cost in the build report and a change shows in the approval review.
13. **Should the compiler generate data builders?** *Recommendation:* yes.
    `make.Todo { title: "Buy milk" }` fills every other required field with
    a valid default and goes through the real create operation, so
    validation and policy apply (factory_bot's model, generated instead of
    written). Fixtures remain for meaningful scenarios and return a named
    object holding what they made (`setup.alice`, `setup.todos`), so a test
    never has to look its own data up again. *Decided 2026-10-13:* yes, as recommended.
14. **How does a test check side effects?** *Recommendation:*
    `assert emitted ReminderDue { todo_id: todo.id }` for events, beside the
    fakes' `.requests` for provider calls. *Decided 2026-10-13:* yes, as recommended.
15. **How are whole values compared?** *Recommendation:* `==` on records and
    lists, with a field-by-field diff on failure; no snapshot assertions,
    because agents update snapshots without reading them and a failing test
    quietly becomes a passing one. *Decided 2026-10-13:* yes, as recommended.
16. **What does a failed test leave behind?** *Recommendation:* with
    `--keep-failed`, its database (named in the report) and its trace, so
    `jadpo trace` and `jadpo console` can inspect exactly what it did. *Decided 2026-10-13:* yes, as recommended.
17. **Is a database copy per test fast enough?** Estimate, not measured: a
    PostgreSQL template copy likely costs tens to a couple of hundred
    milliseconds. *Recommendation:* the runner prepares copies in the
    background while tests run; measure on the golden app before changing
    the approach. *Decided 2026-10-13:* yes, as recommended.
18. **How do tests treat provider calls, capabilities and batches?**
    (from the packages decisions, 2026-10-16) *Decided 2026-10-16
    (Richard):* every provider call is faked, scripted answers taking the
    clean type or a provider failure; fakes honour keys and answer `lost`
    and `in_use`, and multi-step tests rerun with a crash at each step
    boundary by default; one fake per capability (`email.sent`, no
    `sms.sent`); documents give a placeholder PDF and readable HTML; bot
    checks pass unless a test scripts `bot_check_failed`; search is
    PostgreSQL's; grants are declared with `grants: { <connection>: [<fixtures>] }`; batches run to the end in batches of
    two with a restart between each.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Test placement | Carried from area 1: tests beside their subject as `<name>.test.jadpo`; cross-domain scenarios in root `tests/` | 2026-10-12 |
| Kinds of test | Carried from area 1: pure, integration (action), route and schedule, known from what a test calls; no folder per kind; `jadpo test --kind route` runs one kind | 2026-10-12 |
| Fixtures folder | Carried from area 1: test data shared by several test files lives in the domain's `fixtures/` | 2026-10-12 |
| Database engine | Carried from area 5: one engine in every environment, PostgreSQL `main` by default, so tests run on PostgreSQL | 2026-10-12 |
| Seeds | Carried from area 5: seeds create data through the application's own actions and refuse to run against production | 2026-10-12 |
| Caches and events | Carried from areas 1 and 5: caches serve display reads only; events run after the emitting action commits | 2026-10-12 |
| Code in tests | Carried from areas 2 to 4: `var x: T`, literals take the expected type, `if (…)`, one `match` with `->`, failures named in arms, the colon rule | 2026-10-12 |
| What a test can reach | A test beside its code reaches anything in its domain; a scenario in root `tests/` sees only public declarations and routes | 2026-10-13 |
| Test environment | Optional `clock:`, `config:`, `fakes:` settings at the top of the test; the clock defaults to a fixed time | 2026-10-13 |
| Configuration in tests | Area 10: `test:` values, generated secrets, per-test override | 2026-10-13 |
| Fixtures | Test-only actions in `fixtures/` calling the real actions; return a named object of what they made | 2026-10-13 |
| Public fixtures | A fixture may be `public`: another domain's tests, fixtures and seeds, and scenarios in root `tests/`, call it through its domain (`planning.alice_with_todos()`); a private fixture stays its own domain's; only tests, fixtures and seeds ever call a fixture. This page wins over area 1, which had made `public` on a fixture an error (Richard) | 2026-10-14 |
| Route and schedule tests | `request { }` returning `Response`; `schedule.run()` under the test clock | 2026-10-13 |
| Assertions | `assert condition` with sub-expression values; `assert call(...) fails FailureName` | 2026-10-13 |
| Fakes | Generated per package with valid defaults; scripted answers; `.requests`; running out or leftovers fail the test | 2026-10-13 |
| Test database | Disposable PostgreSQL; a template from the migrations copied per test; parallel; real commits | 2026-10-13 |
| Events and caches in tests | Events delivered after each commit in a seeded shuffled order; in-memory cache emptied per test | 2026-10-13 |
| Acting as a user | `Principal.user(user)` in tests, fixtures and seeds; `as:` in route tests | 2026-10-13 |
| Acting as a service | `Principal.service(service)`; route tests sign in with a key from `Service.issue_key` | 2026-10-13 |
| Query cost assertions | None at runtime; the compiler reports query cost and changes show in review | 2026-10-13 |
| Test failure output | Every failed test reports the assertion broken into sub-expression values, record diffs, the failure that happened instead, the test's trace in short, fake usage, clock, seed, kept database and the rerun command; readable and `--json`; stable `test.*` codes (Richard) | 2026-10-13 |
| Running tests | `jadpo test` by file, `file:line`, `--name`, `--kind`, `--failed`; `--watch` reruns affected tests on save; `--interactive` keyboard view to search, navigate and run every test in the app; editor run buttons; no `test.only` (Richard) | 2026-10-13 |
| Data builders | Generated `make.Entity { ... }` with valid defaults through the real create operation; builders may set any non-generated field, including ones ordinary inputs do not accept, so hard-to-reach states are easy to build; validation still applies | 2026-10-13 |
| Side-effect assertions | `assert emitted Event { ... }` and fakes' `.requests` | 2026-10-13 |
| Comparing values | `==` on records and lists with field diffs; no snapshot assertions | 2026-10-13 |
| What a failed test leaves | `--keep-failed` keeps the database and trace | 2026-10-13 |
| Speed of database copies | Copies prepared in the background; measured on the golden app before changing approach | 2026-10-13 |
| Provider calls and capabilities in tests | Every provider call faked; scripted answers in the clean type; fakes honour keys and answer `lost` and `in_use`; crash points at each step boundary of multi-step actions, on by default; one fake per capability (`email.sent`, no `sms.sent`); placeholder PDF with readable HTML; `bot_check_failed` scripted; PostgreSQL search; `grants: { calendar: [alice] }` naming fixtures; file fixtures and failing stores; batches in batches of two with a restart between each (Richard) | 2026-10-16 |
| Idempotency keys in route tests | A route test sends a fresh `Idempotency-Key` with every request but a `GET`, as a real client would, so a route on an API with `idempotency: Idempotency.required` is tested through its success; a test that writes its own `Idempotency-Key` header sends that one instead, to test repeats and refusals; built in chunk 4.7 (Richard) | 2026-10-16 |
| Writing a route test's key (chosen in chunk 4.7, confirmed by Richard 2026-10-16) | A test writes the key it sends as `idempotency_key:` in `request { }`, text, as `as:` stands for the credential: the request has no headers of its own, and an `Input`'s `header:` field is the action's, which the runtime's key never reaches. The runtime checks it as it checks a client's. A `GET` reads no key, so a request to one that gives `idempotency_key:` is `semantic.request_invalid`. A fresh key is a new identity from the test's seed, so a run is the same every time; a route whose `Input` itself reads an `Idempotency-Key` header sends that one | 2026-10-16 |
| An omitted field in a route test (chosen in chunk 4.9) | A route test's `input:` leaves an optional field it doesn't write out of the body it sends, as a client would, so a `patch:` keeps that field rather than clearing it; a field written as `none` is sent as `null`. A request's body goes with `Content-Type: application/json` | 2026-10-16 |
| The test clock (chosen in chunk 5.2) | A test without `clock:` starts at 2000-01-01T00:00:00Z; `clock:` is an instant written out (`type.setting_value` otherwise). `clock.advance(duration)` moves the clock on for the calls after it, never one already running; it is `type.test_clock_context` anywhere but a test, a fixture and a seed included, and a negative duration is the fault `clock_backward` | 2026-10-16 |
