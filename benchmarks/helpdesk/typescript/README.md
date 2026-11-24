# Helpdesk in TypeScript

The helpdesk (workspaces, members, customers, tickets, replies, notes, tags
and macros, with a schedule that flags unanswered tickets) as an ordinary
TypeScript service: Hono on Node, Drizzle over postgres.js, Zod for input.
It answers every route as the Jadpo app in `benchmarks/helpdesk` does, from
the same PostgreSQL schema, so the same SQL fixtures load into either.

## Running it

```sh
npm ci
npm start   # node --experimental-strip-types src/server.ts
```

Settings come from the environment and are checked before it listens:

| Variable                   |                                                              |
| -------------------------- | ------------------------------------------------------------ |
| `DATABASE_URL`             | PostgreSQL, migrated with `drizzle/*.sql`                    |
| `SESSION_SIGNING_KEY`      | at least 32 printable characters                             |
| `BROWSER_ORIGIN`           | the web app's `https://` origin, for browser sessions        |
| `DELETED_TICKET_RETENTION` | how long a binned ticket is kept; default `30d`              |
| `FIRST_RESPONSE_TARGET`    | how long a new ticket has for its first reply; default `24h` |
| `PORT`, `HOST`             | default `3000` on `127.0.0.1`                                |

People sign in with an API token (`Authorization: Bearer jadpo_token_…`) or
the `desk_session` cookie, both kept in `jadpo_sessions` and issued by
trusted infrastructure; the app has no sign-up route.

## Where things are

- `src/routes.ts`: every route, what it reads and the action it runs.
- `src/desk.ts`: the actions. `src/policy.ts`: who may do what.
- `src/schemas.ts`: inputs and how a refused one is reported.
  `src/views.ts`: what each route answers.
- `src/db/schema.ts`: the tables; `npm run db:generate` writes a migration.
- `src/jobs.ts`: the breach and purge schedules, run by the server's tick.
- `src/auth.ts`: credentials. `src/app.ts`: the HTTP envelope.

## Checking it

```sh
npm run check && npm run lint && npm test && npm run format:check
TEST_DATABASE_URL=postgresql://… npm run test:integration
```

`npm test` runs the unit tests. The integration suite needs an empty
PostgreSQL database in `TEST_DATABASE_URL`; it applies `drizzle/` itself.

`adapter/` is test infrastructure, never imported by `src/`:
`adapter/controller.ts` is the process the shared benchmark tests drive, and
`python3 adapter/parity.py` replays `benchmarks/agent-changes/desk_parity.py`
against this app on a PostgreSQL of its own and compares the answers with
`test/parity-reference.json`, what the Jadpo app answered.
