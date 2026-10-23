# Golden todo in TypeScript

The `todo-v0.2` baseline uses strict TypeScript, Hono, Zod, Drizzle, CASL and
JOSE, with pinned dependencies and a checked-in npm lock. Node uses Hono's Node
HTTP adapter; Bun uses `Bun.serve`. Both serve the same production application.
No mail or identity account is needed for local verification.

Install with `npm ci --ignore-scripts --no-audit --no-fund`. Run:

```sh
npm run check
npm run lint
npm run format:check
npm test
python3 adapter/run.py --contract-source /absolute/path/to/reviewed/jadpo --unit
```

The last command starts an isolated PostgreSQL16 cluster on 127.0.0.1, applies
the Drizzle migrations, runs Vitest integration negatives on Node and native
host controls on Node/Bun, and executes every shared
case through raw HTTP on Node and Bun. Supply a checkout containing
`tools/golden-todo/contract.py` and `acceptance-v0.2.json`; unavailable cases
fail. Until reviewed prerequisites and the complete app reach main, the
runner identifies this as a preview and never claims full conformance.
The current authoring checkpoint is the unchanged shared contract in
`chunk-9.1d2` commit `b76debf8dcee0ff5242065ae4427f66b04358232`.
No compiler/runtime implementation from that checkout is imported or built.

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

Reminder events, preparation, attempts and provider keys are durable. Provider
acceptance is recorded only after a validated acknowledgement. Lease tokens,
revision predicates and PostgreSQL row versions protect edits during IO.
The fixture covers lost acknowledgement, ABA changes, stale work, concurrency,
provider outages, retention and operator-held outcomes past the 23-hour safe
retry horizon. Fixture-only signing, SQL, clocks, network answers and mutation
hooks remain under `adapter/`, outside the production server/import graph.

Provider calls retry at most three times with bounded full-jitter waits.
Subscribers have five attempts within 24 hours; durable connection breaker state
holds pending work after five unavailable deliveries. Optional request keys
commit their response receipts atomically with writes, fence expired runners
and replay stored response bytes/status across builds.

The full repository gate, reviewed-main shared conformance, independent review
and performance comparison remain outstanding. Lockfiles/migrations and framework infrastructure must be
counted separately from application source; no throughput or code-size claim is made.
