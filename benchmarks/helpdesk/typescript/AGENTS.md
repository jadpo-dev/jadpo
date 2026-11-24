# Helpdesk TypeScript app

An ordinary TypeScript service: Hono, Drizzle (postgres.js), Zod. The product
and its rules are in the product description; this file is how the code
holds them.

- Start at `src/routes.ts` (each route and the action it runs), then
  `src/desk.ts` (the actions) and `src/policy.ts` (grants by role).
- Every query is walled to the caller's workspaces through `granted(...)`;
  a record the caller can't read is its `<kind>_not_found`, one they can
  read but not change is 403 `not_permitted`, a list shows only what they
  may read. A create checks the grant first, then its references, which must
  exist and stay in the same workspace. Binned tickets (`deleted_at` set)
  are filtered out of every ticket query.
- Inputs are Zod schemas in `src/schemas.ts`; a refusal is 422
  `invalid_input` with `{field, rule}` problems. Outputs are the views in
  `src/views.ts`; never return a row as it is.
- Change the schema in `src/db/schema.ts` and run `npm run db:generate`
  for a new migration in `drizzle/`; never edit an applied one.
- Timestamps come from the injected clock, never `Date.now()` in actions.
- `adapter/` is private test infrastructure: never import it from `src/`.
- Before you finish: `npm run check`, `npm run lint`, `npm test`,
  `npm run format:check`, and `npm run test:integration` with
  `TEST_DATABASE_URL` set to an empty database.
