# Learn Jadpo
<!-- topic: learn -->

This path builds one app, a todo list with sign-in, from an empty folder
to a deployed server. Each page adds one idea to the app and ends with
it running. It's written for a backend developer who knows TypeScript,
SQL and HTTP and has never seen Jadpo.

Read the pages in order: each one uses only what the pages before it
introduced. Each page links the reference topics it uses, which have
the full rules.

| | Page | After it you can | It introduces | Needs |
|---|---|---|---|---|
| 1 | [What Jadpo is, and setting up](01-setting-up.md) | build `jadpo` and run it on an example | what Jadpo is for; the `jadpo` command; `check` and `test`; how an error reads | none |
| 2 | [A project and your first route](02-first-route.md) | run an app that answers `POST /todos` and call it with `curl` | a project and `app.jadpo`; domains; `use` lines; input and output types; actions; routes; a first permission rule; the API record; `jadpo dev`; the error body | 1 |
| 3 | [Types](03-types.md) | give a todo's title rules, and see bad values refused while compiling and at the door | named types and their rules; standard types; enums; `?`, `none` and defaults; functions | 2 |
| 4 | [Storing todos](04-storing.md) | keep every todo in PostgreSQL | entities (a table with types); `id`; generated fields; policy on an entity; `pick:`; `Todo.create`; `var`; migrations; `jadpo sql` | 3 |
| 5 | [Failures](05-failures.md) | refuse a due date in the past with a clear 422 | failures and their kinds; `fails` and `reject`; `match`; `if`; the clock; the error body every client gets | 4 |
| 6 | [Reading todos](06-reading.md) | answer `GET /todos` and `GET /todos/{id}` | queries; `where:`, `where_present:`, `order:` and `limit:`; one record or a 404; path placeholders; indexes the compiler asks for | 5 |
| 7 | [Changing and deleting todos](07-changing.md) | mark a todo done, edit it and delete it | `get`, `update` with `patch:` and `delete`; `attempt`; an action's writes all happening or none | 6 |
| 8 | [Tests](08-tests.md) | prove each route and action works, with a database of its own per test | test files; `assert`; action and route tests; `request { }`; fixtures; a test's clock | 7 |
| 9 | [Settings](09-settings.md) | read a limit from the environment, and keep secrets safe | `config.jadpo`; defaults and test values; `.env.local`; secrets; `jadpo config set` | 8 |
| 10 | [Users and the principal](10-users.md) | give every todo an owner, and make every route need someone signed in | users; the principal; sessions; references; `Principal.user` and `current_principal`; a migration that needs a decision; `conflict:`; tests that sign in with `as:` | 9 |
| 11 | [Signing in by email](11-sign-in.md) | let people sign in with an emailed link or code, and read their mail in development | connections and email; magic links; the first sign-in; `Access.system`; cookies and CSRF headers; the dev inbox | 10 |
| 12 | [Who may see what](12-policy.md) | make each todo visible to its owner only | the kinds of access; owners; policy in the SQL; why a forbidden record is a 404; the permissions report | 11 |
| 13 | [Deploying to a server](13-server.md) | build the app into one program and run it against a real database | the database connection; `jadpo build`; a local PostgreSQL; settings in a file; `jadpo migrate apply`; health checks | 12 |
| 14 | [Deploying to Cloudflare Workers](14-workers.md) | build the app as a Worker and deploy it | a Cloudflare account and Wrangler; deployables; rate limits on Workers; Hyperdrive; Worker variables and secrets | 13 |

When you finish, the [primer](../primer.md) is a one-page summary of the
language to keep beside you, and the teaching projects in the
`examples/` folder of the `jadpo` repository, which you clone on page 1,
show what this path leaves out.
