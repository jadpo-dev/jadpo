---
title: Testing
area: 13
status: decided
checked-with: this checkout's jadpo on every gate run (archived examples: v0 3ce094d0)
rules:
  - configuration.field_unknown
  - configuration.fixture_duplicate
  - configuration.fixture_secret_required
  - configuration.fixture_secret_unexpected
  - configuration.fixture_value_invalid
  - configuration.fixture_value_missing
  - semantic.test_service_fake_required
  - test.assertion_failed
  - test.fault
  - test.unexpected_failure
  - type.test_call_context
  - type.test_clock_context
---

# Testing

How a Jadpo program is tested: where tests live, the four kinds of test,
controlling the clock, configuration and fakes in tests, shared test data,
assertions, the test database, and how tests meet caches and events.

> Decided on 2026-10-13. Code outside the collapsed boxes applies the decisions from areas 1 to 13. The archived compiler's syntax and behaviour (tag `archive/compiler-v0`) are collapsed beneath each section that changed; the [status table](#status) says what the new compiler builds.

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

::: today

Tests can be written in any file, next to the code, and the compiler does not
check where they are.

:::

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

```jadpo check project=writing-a-test file=domains/planning/libraries/due_dates.jadpo hidden
type DueAt = Instant {}

type Overdue = Bool {}

library due_dates {
    /// Whether a todo due at `due` is overdue at `now`.
    function is_overdue(due: DueAt, now: DueAt) -> Overdue {
        return Overdue(due < now)
    }
}
```

```jadpo check project=writing-a-test file=domains/planning/actions/remind_if_overdue.jadpo hidden
action remind_if_overdue(due: DueAt, now: DueAt) -> Overdue {
    return due_dates.is_overdue(due, now)
}
```

```jadpo check project=writing-a-test file=domains/planning/libraries/due_dates.test.jadpo
// domains/planning/libraries/due_dates.test.jadpo
test "a todo due yesterday is overdue" {
    var due: DueAt = "2026-01-14T12:00:00Z"
    var now: DueAt = "2026-01-15T12:00:00Z"
    assert due_dates.is_overdue(due, now)
}
```

::: today

Variables may leave out their type, and literals need constructors:

```jadpo project=today-pure
type Number = Int {}

function add(left: Number, right: Number) -> Number {
    return Number(left + right)
}

test "adds two values" {
    var result = add(Number(2), Number(3))
    assert result == Number(5)
}
```

:::

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

A test that calls actions to set up data and then makes a request is a route
test. Calling an action is an ordinary call; today's `call` word goes. Area 1
named the second kind "integration"; this page calls it "action", because
`integration` is the keyword for third-party APIs (area 11).

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
    assert collection.count(ReminderMail.send_overdue_reminder.requests) == 1
}
```

::: today

Only pure and action tests exist. An action test marks each call with `call`,
and the report labels a test "callable" only when `call` is used: a test that
calls an action without it is reported as pure
(`var next = call next_count(Count(1))`). There is no way to send a request
or run a schedule from a test.

:::

## Test settings

A test's environment is set at the top of its body, before the first
statement, as `name: value` settings (question 2). There are three, and all
are optional:

- `clock:` the instant the test starts at. Without it, the clock is fixed at
  a documented default, never the real time.
- `config:` configuration values that differ from the test values in
  `config.jadpo`.
- `fakes:` the responses each integration operation gives, in order.

Settings are written once per test, so a reader sees everything that shapes
the result in the test itself. Shared data is not a setting; it comes from
fixtures, below.

::: today

The environment is a named `fixture` declaration, attached in the test's
header with `using` (`test "…" using london_noon { }`); see the clock example
below.

:::

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
the test checks exactly what Bob would see.

::: today

Actions read an implicit `principal`, and a test cannot set one: every
direct call runs with no caller (read from generated code).

:::

## Controlling time

A test never waits for real time. The clock starts at `clock:` and moves
only when the test says so, with `clock.advance(5m)`: the clock belongs to
`clock`, like `clock.now`. An action reads one time for its whole run, so
advancing changes what the next call sees, not a call already running.
The clock only moves forward. `clock.advance` is an error outside a test.

```jadpo fragment
// domains/planning/queries/overdue_todos.test.jadpo
test "a todo due in an hour becomes overdue" {
    clock: "2026-01-15T12:00:00Z"

    var alice: Principal.user = alice_with_todos()
    attempt create_todo(CreateTodo {
        title: "Pay rent"
        due_at: "2026-01-15T13:00:00Z"
    }, alice)
    assert collection.count(overdue_todos(alice)) == 0
    clock.advance(1h)
    assert collection.count(overdue_todos(alice)) == 1
}
```

Advancing the clock does not run schedules by itself; a schedule test runs
one with `.run()` (question 5).

::: today

The clock is moved with the sentence `advance clock by`, and both the fixture
and the statement wrap their values in constructors (`fixed Instant("…")`,
`Duration("PT5M")`). Without a fixture clock, the clock is fixed at
2000-01-01T00:00:00Z.

```jadpo project=today-advance
fixture london_noon {
    clock: fixed Instant("2026-01-15T12:00:00Z")
}

value ClockReading {
    value: Instant
}

action read_clock() -> ClockReading {
    return ClockReading {
        value: clock.now
    }
}

test "advancing moves only the next call" using london_noon {
    var first = call read_clock()
    advance clock by Duration("PT5M")
    var second = call read_clock()
    assert first.value == Instant("2026-01-15T12:00:00Z")
    assert second.value == Instant("2026-01-15T12:05:00Z")
}
```

:::

## Configuration in tests

Each configuration field can give a `test:` value in `config.jadpo`, used by
every test (question 3; area 10 settles the configuration syntax). A test
changes one with its `config:` setting. Secrets never appear in test source:
each test run gets a generated throwaway value, which is enough because a
secret can only reach compiler-owned places (signing, integration
credentials), and integrations are faked.

```jadpo fragment
// config.jadpo
config {
    mail_sender: Email {
        test: "reminders@example.test"
    }
    mail_api_key: Text {
        secret: true
    }
}

// a test that needs a different sender
test "reminders come from the configured sender" {
    config: {
        mail_sender: "planning@example.test"
    }
    ...
}
```

A field that tested code reads with no `test:`, `default:` or `config:` value
is a compile error, and so is a secret in a test's `config:`.

::: today

Every fixture lists every configuration field without a default, and each
secret is wrapped in `secret("…")`, repeating that the field is secret:

```jadpo project=today-config
type ApiKey = Text {
    min_length: 8
}

config ApplicationConfiguration {
    api_key: ApiKey {
        binding: "API_KEY"
        secret: true
    }
}

fixture configured_application {
    config {
        api_key: secret("test-only-api-key")
    }
}
```

:::

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
    assert collection.count(sent) == 2
}
```

`Integration.operation.requests` lists what the code sent, as typed values
with secrets left out. A fake cannot run code of its own, and nothing else in
the application can be faked. If the code calls an operation more times than
the fake has responses, or fewer, the test fails and says so (question 7).

::: today

Fakes sit in the named fixture, one line per response with `=>`, and a
successful response needs the `accept` keyword. Running out of responses
counts as `outcome_unknown` instead of failing the test. Checked with
`jadpo test` against `tests/compile/pass/170_checked_service_operation.jadpo`:

```jadpo fragment
fixture mail_rate_limited_then_accepted {
    clock: fixed Instant("2026-01-15T12:00:00Z")

    config {
        mail_api_key: secret("fixture-only-provider-key")
    }

    service ReminderMail: fake {
        send_overdue_reminder => provider.rate_limited
        send_overdue_reminder => accept ReminderReceipt {
            accepted_at: Instant("2026-01-15T12:00:00Z")
        }
    }
}
```

:::

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
type TodoCount = Int {
    min: 0
}
```

```jadpo check project=public-fixture file=domains/planning/actions/add_todo.jadpo hidden
/// The open todos once one more is added.
action add_todo(open: TodoCount) -> TodoCount {
    return TodoCount(open + 1)
}
```

```jadpo check project=public-fixture file=domains/planning/fixtures/two_open_todos.jadpo
// domains/planning/fixtures/two_open_todos.jadpo
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
test "reminders start from planning's open todos" {
    send_reminders()
    assert planning.two_open_todos() == 2
}
```

```jadpo check project=public-fixture file=tests/a_plan_starts_with_two_todos.test.jadpo
// tests/a_plan_starts_with_two_todos.test.jadpo
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

::: today

There are no data fixtures and no seeds. `fixture` names a test environment
(clock, configuration and fakes). The approved test plan describes typed rows
in a fixture (`database primary { }`), which was never built.

:::

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

  assert collection.count(sent) == 2
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
  `test.fake_unused`, `test.fake_exhausted`, `test.unexpected_failure`, and
  `test.fault` for a fault such as an overflow) so an agent can act on it
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
`location` in the test, one of `assertion`, `failure` or `fault`, and the
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
  comes with the rest of area 14's tooling, as `--keep-failed` does until
  the test database.
- A failed test's rerun command and every path a run prints work from the
  folder the command ran in: run from outside the project, they start with
  the project as it was typed (`examples/08-testing/domains/…`). With
  `--failed`, when every test that failed last time is gone, there is
  nothing to rerun and the run passes.
- The editor shows a run button above each test and the last result beside
  it (language server).
- There is no `test.only` in source: focusing is a command-line choice, so a
  focused test is never committed by accident.

::: today

A failed assertion reports only `assertion failed at source bytes 529..542`.
Checking that a call fails needs a `match` whose arms all return the success
type:

```jadpo project=today-fails
type Count = Int {
    min: 0
}

failure TooBig {
    kind: Rejected
    code: "too_big"
    message: "Too big."
}

action check_size(n: Count) fails TooBig -> Count {
    if (n > Count(10)) {
        reject TooBig
    }
    return n
}

test "a large size is rejected" {
    var outcome = match check_size(Count(11)) {
        success (value) => Count(0)
        failure TooBig => Count(1)
    }
    assert outcome == Count(1)
}
```

:::

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

::: today

Each test that touches data gets a fresh in-memory SQLite database whose
tables are created from the entities, not from migrations. With
`DATABASE_URL` set, such a test fails with "isolated tests require the
SQLite test adapter". Tests run one at a time.

:::

## Caches and events

Area 5 decided that caches are copies used only by display reads from
routes, and area 1 that events run after the action that emits them commits.
In a test (question 8 for the database, question 9 for these):

- **Events** are delivered after each step of the test commits and before
  the next statement runs: subscribers run in order, at the test's clock,
  until no events are left. A test checks their effects, such as a sent
  reminder in `requests`. A subscriber that fails fails the test; retries are
  area 12.
- **Caches** are on and start empty for each test, in the application's real
  cache store, so a route test sees the same invalidation as production.
  Tests cannot read, fill or clear a cache directly. Actions never read
  caches, so only route tests meet them.

::: today

Neither exists yet: there are no events or subscribers, and caches build
nothing.

:::

## Errors

> The three `test.*` codes run on the new compiler, with `jadpo test`'s real report. The `check` errors after them (fakes, the test clock and configuration fixtures) are the archived compiler's, in its syntax and with its output, until areas 10 and 11 are built.

Every error this area can produce, with an example that triggers it and
the real output: first the three codes a failed test gets from `jadpo
test`, each with its report (run with `--seed 1`), then the archived
compiler's `jadpo check` errors for what the new compiler hasn't built.

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

`call` belongs to tests only:

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

```jadpo project=e-clock-context expect=type.test_clock_context
value ClockReading {
    value: Instant
}

action move_production_clock() -> ClockReading {
    advance clock by Duration("PT5M")
    return ClockReading {
        value: clock.now
    }
}
```

### `configuration.field_unknown`

From source, only a fixture can name a configuration field that does not
exist:

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

## Review

### Status

Each row says what the new compiler does: built (with the chunk, checked
by `jadpo check` or run by `jadpo test`), or decided and not built yet;
what the archived compiler did (tag `archive/compiler-v0`) is kept where it
explains a decision.

| Feature | Status |
|---|---|
| Pure and action tests | Work in the new compiler (run, chunk 2.2): `jadpo test` runs `test "…" { }` and `assert` in the interpreter, for tests that call functions and actions, one at a time in an order seed's order. |
| Kind of test | Works (run, chunk 2.2): the kind comes from what a test calls (an action, query or fixture makes it an action test), and the footer and `--json` count each kind. Route and schedule tests come with areas 7 and 12; `--kind route` and `--kind schedule` say so (`toolchain.not_built_yet`). |
| Assertion messages | Work (run, chunk 2.2): a failed `assert` reports its file and line, its source with each part's value hung beneath it, expected and actual, a field-by-field diff for records, lists, sets and maps, a stable code and the rerun command with the order seed (see [Errors](#errors)). The trace, fakes, kept database and clock in the decided report come with their phases and are left out until then. A statement over several lines shows its first line; `--json` has all of it. |
| Checking a failure | Works (run, chunk 2.2): `assert call(…) fails Failure`; when the call returns or fails differently, the report shows the value, or the other failure with its code, kind, fields and where it was raised. A failure under `attempt` ends the test as `test.unexpected_failure`, and a fault as `test.fault`, each with its `.jadpo` frames. |
| Clock | Decided, not built: the test clock comes with time (area 10, roadmap 5.2). The archived compiler fixed a fixture's clock and moved it with `advance clock by`, which moved only the next call; without a fixture it was fixed at 2000-01-01T00:00:00Z. |
| Configuration | Decided, not built: configuration in tests comes with configuration (area 10, roadmap 5.1). The archived compiler type-checked fixture values, required exactly the secret fields to be marked secret and every field without a default to be listed, and swapped a global configuration object, so tests couldn't run in parallel. |
| Integration fakes | Decided, not built: fakes come with packages (area 11, roadmap 7.2). In the archived compiler an unfaked operation reachable from a test was an error, responses went through the real adapter with retries and mapping, each test had its own response queue, running out gave `outcome_unknown`, and requests sent couldn't be asserted. |
| Database | Decided, not built: the test database comes with entities (area 5, roadmap 3.1); tests that need data can't be written yet. The archived compiler gave each test an in-memory SQLite database built from the entities, not migrations, which conflicted with area 5, and ran tests one at a time. |
| Caller | Decided, not built: acting as a user comes with identity (area 8, roadmap 4.3). Tests run with no caller, in the new compiler as in the archived one. |
| Placement | Built (chunk 1.3, checked): a test sits in `<name>.test.jadpo` beside its subject, or in root `tests/`; a test elsewhere is `layout.wrong_folder`, which names the `.test.jadpo` file beside it, and a test file with nothing beside it is `layout.test_without_subject`. The archived compiler let tests sit in any file. |
| Watch mode | Works (run, chunk 2.3): `jadpo test --watch` runs the tests asked for, then on every save the ones that reach what changed, through the graph's digests and `calls` and `uses` edges, on one long-lived database; a save no test reaches says `nothing to rerun`, a broken build shows its errors, and the next good save runs everything changed since the last run, or says `errors fixed` when no test reaches it; a removed file is a change like any other. Run from outside the project, its paths start with the project as typed. It ends when nothing reads its output any more, as when piped into `head`. A save shows its results in about 0.1 s on a 230-file project, and the gate fails above 1.25 s. The keys in the decided screen (`enter`, `a`, `f`) wait for `--interactive`, which says `toolchain.not_built_yet`; ctrl-c quits. |
| Fixtures | Work (run): a fixture is checked like an action and called by name from tests, fixtures and seeds; an action calling one is `semantic.not_callable`. A `public` fixture is called through its domain from other domains' tests, fixtures and seeds and from root `tests/`; a private one from there is `visibility.private_cross_domain`. Seeds are checked but `jadpo seed` is not built. |
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
| `Principal.user(user)` in tests, fixtures and seeds, and `as:` signing in through the real authentication method (question 10); a service caller in tests | 8 | Input to area 8 |
| Policy tests are ordinary tests: call as another user and `assert … fails` with the failure policy reports | 9 | Follows from question 6 |
| `test:` values on configuration fields, and the `5m` duration literal | 10 | Input to area 10 |
| Fake responses name the provider conditions in the integration's `failures:` block; checking a fake against the provider's contract or sandbox | 11 | Input to area 11 |
| `overdue_reminders.run()` in tests, subscriber delivery order, and retries of a failing subscriber in tests | 12 | Input to area 12 |
| Caches in tests once the Redis adapter exists | 6 | Input to area 6 |
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
