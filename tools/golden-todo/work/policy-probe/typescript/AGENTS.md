# Golden TypeScript baseline

This is an ordinary strong TypeScript implementation of the `todo-v0.2` golden
application. The Jadpo area pages remain the comparison authority. Do not edit
shared acceptance expectations to make this implementation pass.

- Read `README.md`, then `src/schemas.ts` and `src/app.ts` for the installed API.
- `src/auth.ts` owns credential selection, signatures, durable refresh and JOSE.
  Business code receives only the closed user/service `Principal`.
- `src/policy.ts` owns owner/self predicates. All Todo SQL filters owner and
  active lifecycle before sorting, pagination and limits. Ownership never
  enters the public create/patch inputs or mutation API.
- `src/db/store.ts` owns serializable retries, cancellation, transaction
  admission and awaited rollback. Validate mutation output before commit.
- `src/egress.ts` owns public HTTPS/DNS/TLS and prohibits redirects. Jobs release
  transactions before IO, keep immutable keyed payloads and fence receipts.
- Runtime probes and native host binding live outside application auth/policy.
- `adapter/` is private test infrastructure. Never import it from `src/`, add
  test/authentication fixture routes, or expose fixture secrets in production.
- Run `npm run check`, `npm run lint`, `npm test`, `npm run format:check`, then
  the real PostgreSQL/shared runner described in the README. It starts and
  stops only its own localhost server. Missing prerequisites fail visibly.
- `npm run inventory` derives JSON from installed route schemas and Drizzle
  tables and distinguishes runtime/test evidence from general proof.
