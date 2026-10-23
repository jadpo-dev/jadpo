# Golden todo in TypeScript

The `todo-v0.2` baseline uses strict TypeScript, Hono, Zod, Drizzle, CASL and
JOSE, with pinned dependencies and a checked-in npm lock. Node uses Hono's Node
HTTP adapter; Bun uses `Bun.serve`. Both serve the same production application.
No mail or identity account is needed for local verification.

The full repository gate runs `python3 adapter/verify.py` in its `tests` group.
The wrapper installs the checked-in npm lock with scripts disabled, runs the
orchestration controls, typecheck, lint and formatting, then Vitest integration
negatives and every shared case through the production Node/Bun hosts. CI and
the wrapper use `.node-version` and `.bun-version`. Installation and execution
hold one OS lock, so concurrent verification cannot remove active dependencies.

Run the wrapper from this directory:

```sh
python3 adapter/verify.py
```

It uses `JADPO_TEST_DATABASE_URL` when the repository gate supplies one, accepting
URI and libpq keyword forms while retaining explicit Unix sockets and connection
parameters, and creating/dropping only its own randomly named databases. Otherwise it starts and
stops an isolated PostgreSQL16 cluster on 127.0.0.1. Failed migrations and tests
still clean up owned databases and await child shutdown.

The default shared contract comes from the same checkout's
`tools/golden-todo/contract.py` and `acceptance-v0.2.json`. A successful full
Node/Bun run against those actual files can report `completeContract: true`;
missing prerequisites list every unavailable case as failed. An explicit
checkpoint remains available for independent authoring:

```sh
python3 adapter/verify.py --contract-source /absolute/path/to/jadpo-checkpoint
```

A different checkout always reports `preview: true` and cannot establish own-source
conformance. The private `adapter/run.py --unit` entry performs the same runtime
checks without reinstalling or repeating static checks; it takes the same lock.
Both entries emit the machine result on stdout and subprocess output on stderr.
No compiler/runtime implementation from another checkout is imported or built.

For production startup, supply `DATABASE_URL`, `BROWSER_ORIGIN`, `OIDC_ISSUER`,
`OIDC_AUDIENCE`, `SESSION_SIGNING_KEY` and `MAIL_API_KEY`, then `npm start` or
`npm run start:bun`. An optional previous signing key permits rotation.
Configuration is validated before listening; diagnostics contain declaration
names and safe codes. Provision users, browser/API session verifiers and
service keys through trusted infrastructure: the app has no registration or
automatic external-subject provisioning route. `User.email` is system-only.

`npm run inventory` emits schema-derived route/OpenAPI/data/failure metadata,
secret sinks, declared external boundaries and explicit evidence categories.
The table predicates and route effect labels are reviewable application code,
not a general static proof of policy. Mutation outputs are parsed inside their
transaction; tests deliberately corrupt an output field and prove rollback.
The real-database tests also cover active query cancellation, saturated-pool
cancellation, CPU deadline overruns and serialization-conflict retries.

The request boundary enforces strict JSON/media/body rules, NFC scalar text,
canonical millisecond Instants and default rate/admission limits. Inputs accept
unrefined UUID bit patterns; app-created Todo IDs default to UUIDv7. A closed
principal, centralized CASL/SQL predicates and typed mutation inputs protect
ownership; policy limits apply before pagination. Durable sessions renew
against current authority, and destructive user operations force a fresh read.
A verified external subject must resolve to an active local user; no user and
no sign-in hook produces safe `sign_in_refused` (403), while invalid JWTs stay 401. No provider claims auto-provision users or permissions.

External verification accepts only EdDSA, RS256 and ES256, validates full public
signing-key sets and strict duplicate-free JWT documents, and rechecks subject
authority. Discovery/JWKS fetches have 64KiB/256KiB and 64-key bounds, a
three-second budget each, a five-minute cache, and one coalesced refresh per
30 seconds including failure and cancellation. Invalid issuer/key configuration
is 500; fetch unavailability is 503. All issuer/mail network answers in local
controls are injected; production public DNS/TLS and deployment remain outside
this evidence.

The inventory distinguishes static diagnostic rejection, runtime guards and
behavior exercised by tests. Neither a route effect label nor TypeScript's
structural typing proves arbitrary policy or secret information flow. The
rate limiter enforces a 16MiB accounting budget; JavaScript engine heap
overhead is not proved by that counter.

Reminder events, preparation, attempts and provider keys are durable. Provider
acceptance is recorded only after a validated acknowledgement. Lease tokens,
revision predicates and PostgreSQL row versions protect edits during IO.
The fixture covers lost acknowledgement, ABA changes, stale work, concurrency,
provider outages, retention and operator-held outcomes past the 23-hour safe
retry horizon. Fixture-only signing, SQL, clocks, network answers and mutation
hooks remain under `adapter/`, outside the production server/import graph.

Reminder revisions retain every signed64 bit in PostgreSQL/JavaScript bigint
arithmetic and canonical decimal-string JSON outbox/provider payloads. Provider
calls use the declared `/v1/reminders` protocol and retry at most three times
with bounded full-jitter waits.
Subscribers have five attempts within 24 hours; durable connection breaker state
holds pending work after five unavailable deliveries. Optional request keys
commit their response receipts atomically with writes, fence expired runners
and replay stored response bytes/status across builds.

Shared conformance establishes the exercised contract; independent review and
performance comparison are separate requirements. Lockfiles/migrations and framework infrastructure must be
counted separately from application source; no throughput or code-size claim is made.
