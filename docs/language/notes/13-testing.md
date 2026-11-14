---
title: Testing, design notes
area: 13
---

# Testing: design notes

Why [Testing](../13-testing.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Carried to later areas

| Item | Area |
|---|---|
| The `request { }` form in route tests, the `Response` type, its `status`, `code` and typed output | 7 |
| `Principal.user(user)` in tests, fixtures and seeds, and `as:` signing in through the real authentication method; a service caller in tests | 8 |
| Policy tests are ordinary tests: call as another user and `assert … fails` with the failure policy reports | 9 |
| `test:` values on configuration fields, and the `5m` duration literal | 10 |
| Fake responses name the provider conditions in the integration's `failures:` block; checking a fake against the provider's contract or sandbox | 11 |
| `overdue_reminders.run()` in tests, subscriber delivery order, and retries of a failing subscriber in tests | 12 |
| Caches in tests once the Redis adapter exists | 6 |
| `lost` and `in_use` answers and crash points for several changing calls; batches in tests | 12 |
| One fake per capability; `bot_check_failed`; the placeholder PDF; PostgreSQL search in tests; `fixture_file` | 16 |
| Declaring OAuth grants in tests | 11 |
| `jadpo test` itself: starting PostgreSQL, template copies, parallel runs, `--kind`, `--keep-failed`, the report; `test.no_tests`, `test.failed` and `test.runtime_start_failed`, which only `jadpo test` raises | 14 |
| Tests the compiler generates from facts it already owns (bad input, failure envelopes, closed output) | 14 |
| `layout.test_without_subject` and test file contents | 1 |

## Decisions

| Topic | Decision |
|---|---|
| Test placement | Carried from area 1: tests beside their subject as `<name>.test.jadpo`; cross-domain scenarios in root `tests/` |
| Kinds of test | Carried from area 1: pure, action, route and schedule, known from what a test calls; no folder per kind; `jadpo test --kind route` runs one kind |
| Fixtures folder | Carried from area 1: test data shared by several test files lives in the domain's `fixtures/` |
| Database engine | Carried from area 5: one engine in every environment, PostgreSQL `main` by default, so tests run on PostgreSQL |
| Seeds | Carried from area 5: seeds create data through the application's own actions and refuse to run against production |
| Caches and events | Carried from areas 1 and 5: caches serve display reads only; events run after the emitting action commits |
| Code in tests | Carried from areas 2 to 4: `var x: T`, literals take the expected type, `if (…)`, one `match` with `->`, failures named in arms, the colon rule |
| What a test can reach | A test beside its code reaches anything in its domain; a scenario in root `tests/` sees only public declarations and routes |
| Test environment | Optional `clock:`, `config:`, `fakes:` settings at the top of the test; the clock defaults to a fixed time |
| Configuration in tests | Area 10: `test:` values, generated secrets, per-test override |
| Fixtures | Test-only actions in `fixtures/` calling the real actions; return a named object of what they made |
| Public fixtures | A fixture may be `public`: another domain's tests, fixtures and seeds, and scenarios in root `tests/`, call it through its domain (`planning.alice_with_todos()`); a private fixture stays its own domain's; only tests, fixtures and seeds ever call a fixture. This page wins over area 1 |
| Route and schedule tests | `request { }` returning `Response`; `schedule.run()` under the test clock |
| Assertions | `assert condition` with sub-expression values; `assert call(...) fails FailureName` |
| Fakes | Generated per package with valid defaults; scripted answers; `.requests`; running out or leftovers fail the test |
| Test database | Disposable PostgreSQL; a template from the migrations copied per test; parallel; real commits |
| Events and caches in tests | Events delivered after each commit in a seeded shuffled order; in-memory cache emptied per test |
| Acting as a user | `Principal.user(user)` in tests, fixtures and seeds; `as:` in route tests |
| Acting as a service | `Principal.service(service)`; route tests sign in with a key from `Service.issue_key` |
| Query cost assertions | None at runtime; the compiler reports query cost and changes show in review |
| Test failure output | Every failed test reports the assertion broken into sub-expression values, record diffs, the failure that happened instead, the test's trace in short, fake usage, clock, seed, kept database and the rerun command; readable and `--json`; stable `test.*` codes |
| Running tests | `jadpo test` by file, `file:line`, `--name`, `--kind`, `--failed`; `--watch` reruns affected tests on save; `--interactive` keyboard view to search, navigate and run every test in the app; editor run buttons; no `test.only` |
| Data builders | Generated `make.Entity { ... }` with valid defaults through the real create operation; builders may set any non-generated field, including ones ordinary inputs do not accept, so hard-to-reach states are easy to build; validation still applies |
| What a builder fills a field with (2026-11-12, chunk W1, the obvious recommendation) | A field a lifecycle owns starts at its `initial:` value, as on any new record; one a builder gives is written instead, the one create that may, so a record can start in a state only transitions reach. A field that may be absent is `none`, not a made value, so a test sees exactly what it set. A reference that can't be absent names a record the referenced entity's own builder makes with every field filled, as factory libraries build associations; a chain of required references back to a record being made is `type.missing_field`. Any other field holds what a fake's generated default would ("Values a fake answers with"), read back as the field's type, so builders and fakes share one generator. A field a unique rule holds, alone or with others, gets text, an email address or a link carrying part of a fresh identity from the test's seed (`example-1a2b3c4d`, `someone-1a2b3c4d@example.com`), so two records made the same way don't collide and a rerun with the seed makes the same values |
| Fields a builder can't fill (2026-11-12, chunk W1, the obvious recommendation) | A `File`, which only a store makes; a key, which only `crypto.new_key` makes; and text whose type has a `pattern:`, which no generator reliably matches. A builder must be given those, and one that would have to fill one, itself or through a reference, is `type.missing_field` naming each field and why. No new rule: it is a field missing from a value being built |
| Who a builder runs as (2026-11-12, chunk W1, the obvious recommendation) | As the code around it: in a test, with no caller, so policy lets the create through as for a call passing none; in a fixture called with a caller, as that caller. `make` takes no caller of its own; a test that needs a caller's create calls the action |
| Where builders are written (2026-11-12, chunk W1, the obvious recommendation) | `make` is a generated namespace, reserved as `clock` is; `make.Entity` takes the entity's name as written, imported as any type is. Only tests, fixtures and seeds use it, as only they call fixtures; anywhere else `semantic.not_callable`. A builder has no `fails` list and needs no `attempt`: a failure inside it, a reference given that names nothing or a unique rule broken by a value given, fails the test, as a fixture's does. In a test it commits as a step of its own, so what it emits is delivered before the next line |
| A fixture's named object (2026-11-12, chunk W1, the obvious recommendation) | An `Object` type declared for the fixture in its domain's `types/`, as every shape has a name (area 2): nothing new to build, and the page shows it with builders |
| Side-effect assertions | `assert emitted Event { ... }` and fakes' `.requests` |
| What `assert emitted` compares (2026-11-12, chunk W1, the obvious recommendation) | Any event of the type that a committed step of the test kept, or a subscriber delivered since; only the fields written in its block, with `==`, so a test names what it cares about and an event gaining a field breaks no test. An event an action emitted and rolled back was never emitted. `emitted` is a keyword only right after `assert` and before a name on the same line, so a value called `emitted` still reads as one. A type that isn't an event is `event.emit_invalid`, the rule for emitting one. A failed one is `test.assertion_failed`, showing the event it wanted and each of that type that was emitted, with a field diff when there is one. A batch's items run on a runner of their own, as after a restart, and the events they emit aren't listed |
| What a generated test holds a route to (2026-11-12, chunk W2, the obvious recommendation) | Its API's OpenAPI document, the contract a client reads, which the check writes from the route's declarations: the input's required members, each member's kind and rules, each placeholder's type, each declared failure's status, code and client fields under the response that lists it, and the success's status and schema. The route answers through the program's own codecs and failure plans, so the test sees the two agree on whichever executor runs it |
| Which routes get generated tests (2026-11-12, chunk W2, the obvious recommendation) | Every route the application declares that runs an action, a query or a listing, in an API with a document. The runtime's own routes (sign-in, passkeys, data requests, the OpenAPI document) are proven by the compiler's own tests |
| How a generated test's requests are answered (2026-11-12, chunk W2, the obvious recommendation) | As a caller of the kind the route takes, given in place of signing in, with an identity from the test's key, as `Principal.user` gives an action test's call one: what the tests check comes after sign-in, and no record is needed. Bad input is refused before anything runs. A failure or an output is given in place of what the route runs, once its key is claimed and with a confirmation taken as given: the failure made from the program's own failure plan, its client fields and the output made as a fake's defaults are. Every request but a `GET` sends a key of its own. Only `jadpo test` can give either; a deployable is built without them |
| What bad input a generated test sends (2026-11-12, chunk W2, the obvious recommendation) | One problem at a time on an otherwise valid request: a member no input declares, `not_a_field`, in the query string and in a body; each required member left out (`required`); each member given a value of another kind (`type`); a whole number past its `min:` or `max:`, and text of no other form past its `min_length:` or `max_length:`; and each number, yes-or-no or UUID placeholder or query member given text (`type`, or `format` for a UUID). The answer must name that problem among its problems, not alone, since a member no valid value can be made for adds its own |
| Generated tests' names, place and counts (2026-11-12, chunk W2, the obvious recommendation) | `<METHOD> <path> refuses bad input with { field, rule }`, `... sends each failure it declares in its envelope` and `... sends only the fields its output declares`, keyed `generated <check> <route key>`: a route test at the route's lines, so a file, a line, `--name`, `--kind route` and `--failed` pick it, and `--affected` and `--watch` rerun it when its route is reached. The footer counts the tests a project writes by kind and these apart; `--json` has `generated: true`, `counts.generated` and, with `--full`, each one's `requests` |
| A generated test that fails (2026-11-12, chunk W2, the obvious recommendation) | `test.assertion_failed` at the route, with the request, what it was to be answered with and what it was. The compiler writes both the route and its document, so the next step is to report it (human); no new code is needed |
| What isn't generated (2026-11-12, chunk W2, the obvious recommendation) | The failure and output tests of a route whose placeholders, input or output hold something no value can be made for, as for data builders (text with a `pattern:`, a key, a file), and the output test of a route with `settle:`, a stream or `sends:`, whose answer isn't its output's JSON |
| Which people get a generated erasure test (2026-11-14, chunk W2b, the obvious recommendation) | Each person entity no test reaches an `erase` of, through what it calls, the routes it sends requests through and the actions they run, the schedules it runs, and the subscribers of the events those emit, however deep: found from the graph's edges and the bodies, so it is the same whichever tests a run picks. A person some test erases is held by that test's own check after it. A person no builder can make, or whose erasure isn't built, gets none |
| What the generated erasure test makes (2026-11-14, chunk W2b, the obvious recommendation) | The person, as `make.<Person> {}` would, then one record of each kind that belongs to them: every entity with a reference to them, another person entity included, each of its references to the person given as theirs, and every other field filled as a builder fills it. A reference it can't be without is a record that entity's builder makes, and one to the person names theirs, so all of it is the person's. A kind any of whose values no builder can make (a `pattern:`, a key, a file, or a type whose rules a generated value doesn't keep) is left out with what only it needs, as a route whose types no value can be made for gets only its bad-input test |
| How the generated erasure test runs (2026-11-14, chunk W2b, the obvious recommendation) | Each record is made through its entity's create with no caller, as a builder's create runs in a test, a step of its own with its deliveries before the next; then the person is erased the same way. Before the erasure, their export must hold at least the person and each record of a kind of theirs it made, another person's own record aside, which their export leaves out, so the test can't pass by making nothing; after the erasure's deliveries, the runtime's check that holds every test's erased people to an export of nothing holds this one. The runtime runs it from the plan's test section, which carries each create and the erasure, so every executor runs the same steps. A record a builder can't make, or an export that holds too few or too many, is `test.fault`, naming the entity, the failure's code or how many records, never a value |
| The generated erasure test's name, place and selection (2026-11-14, chunk W2b, the obvious recommendation) | `<Person>.erase leaves nothing in <Person>.export`, keyed `generated erasure <entity key>`, at the entity's lines, an action test counted with the generated tests. A file, a line, `--name`, `--kind action` and `--failed` pick it, and `--affected` and `--watch` rerun it when an entity it makes or the person's export reads is reached. With `--full`, `--json` lists `makes`, each record's entity in the order made |
| Generated tests inside workerd (2026-11-14, chunk W2b, the obvious recommendation) | The Worker comparison runs a project's route tests and its generated tests, a route's and a person's. A generated route test's requests go to the Worker as data, with what each forces, and the Worker makes them on its own program as the runner makes them natively; the erasure test goes by its person's name. Their answers are held to the API's document and their records to the interpreter's, as natively |
| What a fake's `.requests` holds | Decided 2026-11-07 (Richard): a list typed as the operation's input, kept at the call site in the order sent, each request as the package encoded it, with secret and credential fields redacted. Not built: roadmap chunk A27, "Fakes' `.requests`" |
| Comparing values | `==` on records and lists with field diffs; no snapshot assertions |
| What a failed test leaves | `--keep-failed` keeps the database and trace |
| Speed of database copies | Copies prepared in the background; measured on the golden app before changing approach |
| Provider calls and capabilities in tests | Every provider call faked; scripted answers in the clean type; fakes honour keys and answer `lost` and `in_use`; crash points at each step boundary of multi-step actions, on by default; one fake per capability (`email.sent`, no `sms.sent`); placeholder PDF with readable HTML; `bot_check_failed` scripted; PostgreSQL search; `grants: { calendar: [alice] }` naming fixtures; file fixtures and failing stores; batches in batches of two with a restart between each |
| Idempotency keys in route tests | A route test sends a fresh `Idempotency-Key` with every request but a `GET`, as a real client would, so a route on an API with `idempotency: Idempotency.required` is tested through its success; a test that gives its own key sends that one instead, to test repeats and refusals |
| Writing a route test's key | A test writes the key it sends as `idempotency_key:` in `request { }`, text, as `as:` stands for the credential: the request has no headers of its own, and an `Input`'s `header:` field is the action's, which the runtime's key never reaches. The runtime checks it as it checks a client's. A `GET` reads no key, so a request to one that gives `idempotency_key:` is `semantic.request_invalid`. A fresh key is a new identity from the test's seed, so a run is the same every time; a route whose `Input` itself reads an `Idempotency-Key` header sends that one |
| An omitted field in a route test | A route test's `input:` leaves an optional field it doesn't write out of the body it sends, as a client would, so a `patch:` keeps that field rather than clearing it; a field written as `none` is sent as `null`. A request's body goes with `Content-Type: application/json` |
| The test clock | A test without `clock:` starts at 2000-01-01T00:00:00Z; `clock:` is an instant written out (`type.setting_value` otherwise). `clock.advance(duration)` moves the clock on for the calls after it, never one already running; it is `type.test_clock_context` anywhere but a test, a fixture and a seed included, and a negative duration is the fault `clock_backward` |
| Requests to generated routes | A route test's `request { }` calls a generated magic-link route as it calls the app's own, with the built-in input the API record names, and every request carries the app's own `Origin`, as a browser's does (area 8, "In development and tests") |
| Sessions in a route test | A route test signs each caller in once: the first request `as:` a person starts a session, and later requests by the same caller with the same `sign_in:` and `strength:` send it again, as a browser keeps its cookie, so `clock.advance` past its end gives 401 and a role taken away shows on the next request. A different caller, or a different `sign_in:` or `strength:`, starts its own. A service keeps its key the same way; an external token, which no session keeps, is fresh from the local issuer per request (Richard, 2026-10-31) |

## Moved from the area page

When [Testing](../13-testing.md) was rewritten to the
[writing guide](../../writing.md), its reasons, its comparisons with
other tools and its references to other areas by number moved here or
became links by title. Nothing in this list changed a rule.

- **Why tests are placed as they are.** A test beside its subject reaches
  its whole domain and a scenario in root `tests/` sees only public
  declarations and routes, so unit tests stay close to their code and
  scenarios can't depend on internals. Placement and the kinds of test
  come from area 1, Imports included.
- **Why the second kind is "action", not "integration".** `integration`
  is the keyword for third-party APIs (area 11).
- **Why fixtures call actions.** Rows written straight into tables, as
  Rails' YAML fixtures and Django's JSON fixtures are, would skip
  validation and policy and drift from the code. Seeds work the same way
  (area 5), and a seed refuses to run in production (area 5).
- **Why there are no snapshot assertions.** Agents update snapshots
  without reading them, and a failing test quietly becomes a passing one.
- **The assertion broken into parts** shows every sub-expression's value,
  as pytest and power-assert do.
- **Why each test's actions commit for real.** Rolling back one
  transaction per test would be faster, but breaks after-commit work
  (events) and concurrent-change checks. PGlite needs no install, but runs
  one connection at a time.
- **References by number the page now makes by title:** the request's
  shape and streaming (area 7); signing in with `as:` and the caller
  (area 8); root `policy { }`, policy proved by calls with a caller
  (area 9); only actions write records (area 6); recordings (area 11) and
  prompt recordings (area 21, "Recorded answers"); how a schedule runs,
  the tick, subscribers, retries, batches and several changing calls
  (area 12); concealed records and the failure standard (area 4);
  configuration and `configuration.test_value_missing` (area 10); the
  `random` namespace (area 19); `email.sent` and the capability fakes
  (area 16); the trace and `jadpo calls script` (area 17); `--affected`
  and `test.no_tests` (area 14); the database engine and caches
  (area 5, area 6); `attempt` (area 3); stable identity of a test
  (area 18).

## `.requests` (chunk A27)

Decisions the page didn't make, decided 2026-11-11, Richard accepting
the recommendations:

- **The form it is read in.** `.requests` is read as
  `connection.resource.operation.requests`, the form the code calls and
  `fakes:` scripts. The page's earlier fragments named a `ReminderMail`
  integration by two names, from before packages and connections;
  they read `reminder_mail.reminders.send_overdue_reminder` now, and
  `rate_limited`, which isn't one of a fake's conditions, became
  `unavailable`.
- **A repeat under a kept key is listed once.** The same request sent
  again under a key the provider keeps, by a crash's rerun or a retry
  after `lost`, isn't a second entry: `jadpo test` reruns a test with a
  crash at each step boundary and the test must pass each time, so the
  list can't grow with the crash. A send answered `unavailable` kept
  nothing and is listed again, which is what the area page's retried
  example shows.
- **What a crash sends again with no key** (a read, or a call its package
  marks safe) is listed again, as the `ai` fake's is: the rerun does send
  it. A test that counts such a call over an action with several changing
  calls fails naming the crash boundary; it should count the keyed calls
  instead. A later chunk could match a rerun's sends against the attempt
  it replaced if tests need that.
- **An operation with no `input:`** lists one `Unit` per send.
- **A connection that fills a capability.** A mail connection's
  operation lists what the caught mail was sent, as its fake keeps it; a
  connection that fills `ai` has nothing here, since `ai.<prompt>.requests`
  reads those by prompt.

## Following a file's link in a route test

`request { link: <File or text>, range: <text> }` follows a signed link as a
client does, through the HTTP component, so a route test can download a file
on both executors, whole and by range, and see an expired or altered one
refused. `link:` takes text as well as a `File` so a test can send a link it
altered, since no test can read a `File`'s parts. It names no route, and its
`Response` holds `status`, `code` and `body` rather than `output`.

## Keys under `--watch` and the keyboard view (chunk T3)

Decisions the page didn't make, decided 2026-11-11, Richard accepting
the recommendations:

- **What the watch keys run.** `enter` reruns the tests the last run ran,
  `a` every test the command asked for (its path, `--name` and `--kind`),
  `f` the tests failing now, as a test runner's watch mode does. Each run
  appends a rule naming its key, as a save's names its file (`── 14:02:31
  · run all · 8 tests ──`). `q` quits beside ctrl-c, because a terminal
  that delivers keys as they are pressed delivers ctrl-c as a key, and
  the session must put the terminal back before it ends. Keys are read
  only when standard input is a terminal and the waiting line is drawn
  for one; piped, or with `--json`, nothing changes.
- **`--interactive` takes no selection.** A path, `--name`, `--kind`,
  `--failed` or `--affected` is `cli.test_arguments`, since the view lists
  every test and searches inside it; `--seed`, `--keep-failed`, `--full`
  and `--watch` (open with watching on) are taken.
- **Its refusals are usage errors.** `--json`, and a standard input or
  output that isn't a terminal or isn't rich (`JADPO_PLAIN`, `TERM=dumb`),
  are `cli.test_arguments`, exit 2, naming the flags that do the same job;
  no rule of their own.
- **Each test's last result.** The project's record of the last run,
  `build/test/last-run.json`, now keeps the tests that passed as well as
  those that failed. A test that passed is shown passed until it runs
  again, even if its code changed since, as `--failed` trusts the record.
- **What `a` and `enter` run under a search.** `a` runs every test, the
  search notwithstanding; `enter` on a file runs the file's tests the
  search shows.
- **A report in the view is whole.** A fault that repeats an earlier
  test's is shown in full, since it is read on its own.
- **Leaving the view.** It draws on the alternate screen and gives the
  terminal back on quit, leaving the last run's footer in the scrollback;
  it exits 0, as a watch session does.
