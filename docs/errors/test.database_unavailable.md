# `test.database_unavailable`

The tests that need a database couldn't run, because PostgreSQL for the tests couldn't be started or reached.

## Why Jadpo refuses it

An action, route or schedule test runs against a real database, so with
no server there is nothing for it to run on. Nothing in your project is
wrong, and no test failed: telling a coding agent to fix the tests would
send it after a problem that isn't in them. So the run stops once, before
any test, and says so in one error rather than in each test that needed
the database.

Jadpo doesn't step out of a sandbox or install anything by itself. What
is allowed to run on your machine is yours to decide.

## What it looks like

A project with tables and tests, run where PostgreSQL can't make shared
memory, as inside some sandboxes. This output is copied rather than
checked, since an ordinary machine never raises this error:

```text
error: PostgreSQL for the tests couldn't start
  reason: `initdb` failed: could not create shared memory segment: Operation not permitted. Nothing in the project is wrong; the 5 tests that need a database didn't run. `jadpo test --full` shows all it said.
  next: Run `jadpo test` outside the sandbox, where PostgreSQL can start, or set `JADPO_TEST_DATABASE_URL` to a server's admin URL (human)
  rule: test.database_unavailable
  help: https://jadpo.dev/docs/errors/test.database_unavailable
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides what may run on your machine and where. With `--json` this is the
one diagnostic in the document, so an agent reading only the top level sees
it. `jadpo test --full` gives everything PostgreSQL printed.

The other two forms differ in what they ask of you. On a machine with no
PostgreSQL, the error is `No PostgreSQL for the tests`, and its next step
is to install it. When `JADPO_TEST_DATABASE_URL` names a server that can't
be used, the error is `The PostgreSQL server for the tests can't be used`,
and its next step is to check that server. The address is never printed,
since it may hold a password.

When some of the tests chosen need no database, an `impact:` line says
that `jadpo test --kind pure` still runs them.

## How to fix it

**If a sandbox is in the way**, run the tests outside it:

```sh
jadpo test
```

PostgreSQL can make its shared memory there, so the server starts and the
tests run.

**If you want the tests on a server of your own**, name it:

```sh
JADPO_TEST_DATABASE_URL=postgresql://postgres@localhost:5432/postgres jadpo test
```

The tests' databases are made on that server, so `jadpo test` starts none.

**If PostgreSQL isn't installed**, install it, then check that its tools
are on your `PATH`:

```sh
initdb --version
```

`jadpo test` starts its disposable server from those tools.

Your last run's failures are left as they were, so `jadpo test --failed`
still reruns them.

## Related

- [The test database](../language/13-testing.md#the-test-database)
- [`jadpo test`](../language/14-tooling.md#jadpo-test)
