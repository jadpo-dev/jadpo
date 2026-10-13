# Testing: fast isolated PostgreSQL, and fakes for providers

Research behind questions 7 and 8 of [Testing](../13-testing.md). Where the
two differ, the page wins.

## 1. The test database

### What is hard

Area 5 decided one engine in every environment, PostgreSQL by default, so
tests must run on PostgreSQL. Each test must start from an empty database at
the current schema, tests must be able to run in parallel, and the suite has
to stay fast as it grows to hundreds or thousands of tests.

Two Jadpo rules make this harder than in most frameworks:

- **Every action commits.** An action is all-or-nothing in its own
  transaction (area 3), and events run only after that commit (area 1). A
  test that never commits never sees its events.
- **Writes check for concurrent change** (area 3). Testing that needs two
  real transactions on two connections.

Today each test gets an in-memory SQLite database built from the entities,
not the migrations, and PostgreSQL is refused (read from generated code).
The repository's own validation already starts disposable PostgreSQL clusters
with `initdb` and `pg_ctl` in a temporary folder (`tests/runtime/postgres.sh`).

### Options

**A. A template database copied per test.** Build one database from the
migrations, mark it as a template, and create each test's database with
`CREATE DATABASE test_42 TEMPLATE jadpo_template`. PostgreSQL copies the
files directly, so the cost depends on the schema size, not on the number of
migrations. Reported costs are tens of milliseconds per copy for typical
schemas, around 40 ms for a 14 MB template in one write-up and a 98 ms mean
in another, falling towards 10 to 20 ms when copies are made ahead of time
and pooled. IntegreSQL does exactly this: it hashes the migration and fixture
files, rebuilds the template only when they change, and keeps a warm pool of
copies. The one catch is that nothing may be connected to the template while
it is copied, so copies are serialised.

- Real commits, real after-commit work, real concurrency, real migrations.
- Parallel by construction: each test owns a database.
- A failed test's database can be kept and opened.
- Needs a PostgreSQL server for the run.

**B. One transaction per test, rolled back.** Rails' transactional tests,
Django's `TestCase` and Elixir's Ecto sandbox wrap each test in a
transaction and roll it back. It is the fastest option, with no database to
create. Its known costs: after-commit hooks do not fire (Django's
`on_commit` needs `TransactionTestCase` or `captureOnCommitCallbacks`, and
Rails needed a fix for `after_commit`); deferred constraints are never
checked; work on another connection cannot see the test's data; and tests
that touch the same unique values can block or deadlock each other under the
Ecto sandbox. For Jadpo every action's commit would become a savepoint, so
events and concurrent-change checks would behave differently from
production, which is the thing area 5's decision set out to prevent.

**C. PGlite.** PostgreSQL compiled to WebAssembly, running inside the test
process with no install; `new PGlite()` gives an in-memory database. It is
the real PostgreSQL code, but it runs in single-user mode: one connection,
one thread. Version 0.4 can multiplex several clients onto that connection,
and its authors warn that not every use behaves as on a full server. It has
a 4 GiB memory ceiling and only the extensions compiled into its build.
Concurrency tests cannot run on it.

**D. Testcontainers.** Starts PostgreSQL in Docker for the run. It answers
"where does the server come from", not isolation: it is usually combined
with A or B. It needs Docker, and the container start adds seconds per run,
not per test.

### Comparison

| | Speed per test | Real commits and events | Concurrency tests | Parallel | Needs |
|---|---|---|---|---|---|
| A. Template copy | tens of ms | yes | yes | yes | a PostgreSQL server |
| B. Rollback | ~0 | no | no | with care | a PostgreSQL server |
| C. PGlite | in-process | yes | no | one instance per worker | nothing |
| D. Testcontainers | (server only) | with A | with A | with A | Docker |

### Recommendation

Option A, run by `jadpo test`, with nothing in source:

1. Use `JADPO_TEST_DATABASE_URL` when it is set (CI, a shared server).
   Otherwise start a disposable cluster from the local PostgreSQL binaries,
   as `tests/runtime/postgres.sh` does today. If neither exists, stop with a
   message saying how to install PostgreSQL; never fall back to another
   engine.
2. Build the template from the migrations once per run, and again only when
   a migration changes.
3. Keep a small pool of copies ahead of the tests, give each test its own,
   and drop it afterwards; `--keep-failed` keeps a failed test's copy.
4. Let actions commit for real, deliver events after each step, and run
   tests in parallel up to the number of copies in the pool.

Option B could later become an invisible speed-up for tests the compiler can
prove do not emit events or test concurrency, since the compiler knows both;
it should not be the default. PGlite is worth revisiting if installing
PostgreSQL proves a real obstacle for new users, but it cannot replace A
because of concurrency tests.

## 2. Fakes for HTTP providers

### What tools do

- **Interceptors** (nock, Mock Service Worker) catch outgoing HTTP calls in
  the test process and answer them. nock is built around expected calls and
  reports responses that were never used; MSW is built around a reusable
  fake API.
- **Stub servers** (WireMock, Stripe's `stripe-mock`) run as a server.
  WireMock can record real traffic and play it back. `stripe-mock` is built
  from Stripe's OpenAPI document and returns responses of the right shape,
  but Stripe says it does not reproduce real behaviour and that integrations
  should still be tested against test mode.
- **Verified fakes** are fakes whose behaviour is checked by running the
  same tests against the fake and the real service, so they cannot quietly
  drift.

### What Jadpo already does well

Today's fakes are close to the best of these. A fake can only return
outcomes the integration's checked contract allows, the response still runs
through the generated adapter (input checks, retries, time limits, failure
mapping), each test gets its own queue of responses, and an unfaked call is a
compile error (`semantic.test_service_fake_required`). That is
`stripe-mock`'s "right shape" guarantee plus nock's per-test expectations,
checked while compiling.

### Gaps and proposals

1. **Syntax.** `service X: fake { op => accept T { } }` uses a removed arrow
   and a one-off keyword. Proposed: a `fakes:` setting in the test,
   `Integration.operation: [responses]`, where a response is an output value
   or a provider condition named in the integration's `failures:` block.
2. **Wrong number of calls.** Running out of responses is reported as
   `outcome_unknown` today, which hides a mistake in the test as a provider
   outcome. Proposed: running out, or leaving responses unused, fails the
   test and names the operation, as nock's pending-mock check does.
3. **What was sent.** There is no way to check the request. Proposed:
   `Integration.operation.requests`, a typed list with secrets left out.
4. **Drift from the provider.** The contract file pins the shape, not the
   behaviour. A small set of sandbox checks run against the provider's test
   mode would make the fakes verified fakes; that belongs to area 11.

## Sources

- PostgreSQL, [Template databases](https://www.postgresql.org/docs/current/manage-ag-templatedbs.html)
- IntegreSQL, [allaboutapps/integresql](https://github.com/allaboutapps/integresql)
- Brandur Leach, [pgtestdb](https://brandur.org/fragments/pgtestdb) (timings
  for template copies against migrating per test)
- Nirvana, [How we made PostgreSQL work in unit tests](https://www.nirvanatech.com/blog/how-we-made-postgresql-work-in-unit-tests)
- ElectricSQL, [PGlite](https://github.com/electric-sql/pglite) and
  [Announcing PGlite v0.4](https://electric-sql.com/blog/2026/03/25/announcing-pglite-v04)
- Ecto, [SQL sandbox](https://ecto-sql.hexdocs.pm/Ecto.Adapters.SQL.Sandbox.html)
- Adam Johnson, [Django's test case classes](https://adamj.eu/tech/2019/07/15/djangos-test-case-classes-and-a-three-times-speed-up/)
- Justin Weiss, [A couple of callback gotchas (and a Rails 5 fix)](https://www.justinweiss.com/articles/a-couple-callback-gotchas-and-a-rails-5-fix/)
- Testcontainers, [PostgreSQL module](https://testcontainers.com/modules/postgresql/)
- Stripe, [stripe-mock](https://github.com/stripe/stripe-mock) and
  [Test mode](https://docs.stripe.com/test-mode)
- Theodo, [nock vs MSW](https://www.theodo.com/blog/nock-vs-msw-i-tested-both-and-here-is-what-i-learned)
- WireMock, [Record and playback](https://wiremock.org/docs/record-playback/)
- Itamar Turner-Trauring, [Verified fakes](https://pythonspeed.com/articles/verified-fakes)
