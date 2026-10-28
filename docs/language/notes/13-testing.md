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
| Side-effect assertions | `assert emitted Event { ... }` and fakes' `.requests` |
| Comparing values | `==` on records and lists with field diffs; no snapshot assertions |
| What a failed test leaves | `--keep-failed` keeps the database and trace |
| Speed of database copies | Copies prepared in the background; measured on the golden app before changing approach |
| Provider calls and capabilities in tests | Every provider call faked; scripted answers in the clean type; fakes honour keys and answer `lost` and `in_use`; crash points at each step boundary of multi-step actions, on by default; one fake per capability (`email.sent`, no `sms.sent`); placeholder PDF with readable HTML; `bot_check_failed` scripted; PostgreSQL search; `grants: { calendar: [alice] }` naming fixtures; file fixtures and failing stores; batches in batches of two with a restart between each |
| Idempotency keys in route tests | A route test sends a fresh `Idempotency-Key` with every request but a `GET`, as a real client would, so a route on an API with `idempotency: Idempotency.required` is tested through its success; a test that gives its own key sends that one instead, to test repeats and refusals |
| Writing a route test's key | A test writes the key it sends as `idempotency_key:` in `request { }`, text, as `as:` stands for the credential: the request has no headers of its own, and an `Input`'s `header:` field is the action's, which the runtime's key never reaches. The runtime checks it as it checks a client's. A `GET` reads no key, so a request to one that gives `idempotency_key:` is `semantic.request_invalid`. A fresh key is a new identity from the test's seed, so a run is the same every time; a route whose `Input` itself reads an `Idempotency-Key` header sends that one |
| An omitted field in a route test | A route test's `input:` leaves an optional field it doesn't write out of the body it sends, as a client would, so a `patch:` keeps that field rather than clearing it; a field written as `none` is sent as `null`. A request's body goes with `Content-Type: application/json` |
| The test clock | A test without `clock:` starts at 2000-01-01T00:00:00Z; `clock:` is an instant written out (`type.setting_value` otherwise). `clock.advance(duration)` moves the clock on for the calls after it, never one already running; it is `type.test_clock_context` anywhere but a test, a fixture and a seed included, and a negative duration is the fault `clock_backward` |
