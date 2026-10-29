# Strong TypeScript baseline specification

**Status:** candidate freeze  
**Purpose:** prevent comparison against a deliberately weak conventional stack

Exact dependency versions are pinned when the experiment checkpoint is cut.
P10R freezes facilities and selection criteria now; a version-refresh after the
checkpoint is reported as a separate run rather than silently changing tools.

## Stack

- TypeScript in strict mode on the same Bun runtime used by generated output;
- Hono for HTTP routing and middleware;
- Zod for request, response, configuration, and provider-boundary schemas;
- Drizzle ORM and drizzle-kit for typed queries, schema, and migrations;
- PostgreSQL in deployed tests and SQLite only where the language stack also
  receives an equivalent local mode;
- an explicit policy layer using CASL or an equivalently mature typed
  authorization library selected before the checkpoint;
- signed browser sessions, short-lived signed user bearers, service API keys/exchange,
  and allowlisted JOSE JWT validation, all normalised into one closed
  application-owned user/service `Principal` type;
- Vitest for unit/integration tests and the shared black-box runner for contract
  tests;
- ESLint with type-aware rules, dependency/egress restrictions, and no floating
  promises;
- OpenAPI generated from the same route schemas, not handwritten separately;
- structured logging with redaction and one explicit error-boundary mapper; and
- an agent instruction file, architecture constraints, and task-specific MCP or
  repository tools equivalent in discoverability to the language compiler.

Substitutions are allowed only before freeze and must improve the baseline, not
make it easier for the language to win.

## Required architecture

The baseline must centralise authentication, principal normalisation, error
mapping, output validation, secret injection, database transactions, policy
checks, and job execution. It may use code generation, lint rules, repository
boundaries, and custom static analysis. The comparison question is whether
those facilities can provide most of the same safety without a new language.

The baseline is not required to imitate the new language's source layout.
Idiomatic, expert TypeScript is the intended competitor.

## Equivalent obligations

Both implementations must:

- pass the unmodified `acceptance.json` cases;
- produce route, policy, data-access, external-effect, secret, failure, and
  lifecycle inventories;
- expose exactly the same HTTP contracts and safe failure envelopes;
- use the same authentication test identities and provider failure fixtures;
- enforce exactly-one cookie/bearer credential selection, the declared
  immediate/bounded revocation guarantees, and fresh-authority routes without
  depending on a cache;
- operate under the same database, network, clock, and package-install
  constraints;
- receive the same task prompts and clarification answers; and
- preserve all failed attempts, tool/compiler cycles, tokens, timing, and human
  interventions.

## Baseline safety credit

A TypeScript change counts as prevented before execution only when its type
system, linter/static rule, repository gate, or policy checker rejects it before
the test/runtime stage. A failing test receives test-detection credit. Human or
agent recognition receives review credit. The language stack is scored under
the same distinction.

## Agent context

The baseline agent receives concise generated architecture and contract
summaries, schema/type navigation, runnable validation commands, and relevant
examples. The language agent receives the corresponding compiler context. Raw
framework boilerplate is not intentionally dumped into either context.

### Golden authentication accounting clarification (2026-09-30)

The shared golden cases use bounded signed user credentials. Fresh-authority
routes have one principal-resolution database query, with additional credential
or session checks recorded separately. The TypeScript implementation receives
the same counters and budget; no total-authentication query count is inferred.
Authoritative profile data remains an application read, never a signed claim.
Disablement cases use the same explicit fresh/refresh checks and expiry clock
as Jadpo, retaining todos and forbidding new credential issuance after disable.
