---
title: Debugging and review, design notes
area: 17
---

# Debugging and review: design notes

Why [Debugging and review](../17-debugging-and-review.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Consistency with the rest of the language

- **Values never, places always.** Stacks, timelines and `why` answers follow
  area 4's log rule and area 16's span rule. The exceptions are all local
  to `jadpo dev`: the timeline kept for `test new --from`, which holds the
  full request, and the development call log and `jadpo watch`, which show
  values in full. All live in `build/dev/` on the developer's machine,
  never logged, committed or exported, and never show secrets.
- **`jadpo call` and `jadpo calls`.** One runs a query or action; the other
  lists provider calls. The names are close; `jadpo help` gives both.
- **Concealment.** `jadpo why` says a record was concealed by policy, which
  the client must never learn (area 4). It reads local evidence or logs the
  operator already has, so this reveals nothing to a client.
- **A caller from outside a route.** `jadpo call --as` makes a principal
  outside a route, which area 8 keeps out of application code. Like tests
  and seeds, it is a tool, not code, and goes through the generated
  principal.
- **Read-only screens.** Area 16 leaves background job dashboards to area
  14's commands. The console shows deliveries but acts only through those
  commands, so there is one way to change them.
- **Sign-off.** Acceptances are files owned in CODEOWNERS (area 7) and
  there are no Jadpo accounts (area 14), so the review view only reads.
- **A new folder in `build/`.** `build/dev/` and `build/review/` join area
  14's list; `build/` stays generated and uncommitted.

## Carried to later areas

| Item | Area |
|---|---|
| Compiler writes a source map; runtime logs `.jadpo` frames | 4 |
| Stable identity for actions, queries and routes, beside the schema identity registry | 5 |
| Commit recorded in the build stamp; `jadpo why`, `trace`, `call`, `test new --from`, `mcp`; `build/dev/` and `build/review/` | 14 |
| The approval report's order and format | 14 |
| Captured tests: fixtures, `fakes:`, personal fields replaced | 13 |
| The developer console and MCP server; application graph and overlays; production monitoring | Product work |
| The review web view; interdiff between agent revisions | Product work |
| Breakpoints in `.jadpo` on the Rust and WebAssembly targets | Roadmap |
| Path IDs and per-path query counts | 16 |
| Path coverage in `jadpo test`; production counts as a test input | 13 |
| What a replay recording may hold | 15 |
| `jadpo watch`, `calls`, `traces`, `metrics` and `logs`, and `jadpo dev --export-telemetry`, `--fake`, `--sandbox` and `grants add`, in the command reference | 14 |
| Unfinished multi-step runs and `diverged` deliveries as operator items, in `jadpo deliveries` | 12, 14 |
| Unverified publishers and service approvals in the approval report | 11, 18 |

## Decisions

| Topic | Decision |
|---|---|
| Logging | Carried from area 4: one structured event per failure or fault; internal values by area 4's rule; no stack for domain failures |
| Request id | Carried from area 4: every error response and every failure log carries `request_id` |
| Acceptances | Carried from area 7: breaking changes, drops, policy weakening and new domains need a person-written file in `acceptances/`, owned in CODEOWNERS |
| Reading the caller | Carried from area 8: `current_principal` in routes only |
| Permissions report | Carried from area 9: `build/report/permissions.md` and `.json`, shown in the approval review |
| `.jadpo` locations | Always `.jadpo` file and line on every target; the runtime maps its own frames; internal frames as `runtime: <component>` |
| Production incidents | Events carry build and commit; `jadpo incident` shows the line then and now, or changed, removed or split |
| Breakpoints | Not now; revisit on the Rust and WebAssembly targets |
| `jadpo why` | One command explaining policy, lifecycle, concealment, cache use and subscriber runs, ending at the responsible `.jadpo` line |
| Request timeline | Last 500 requests in `jadpo dev`; `jadpo trace <id>`; provider calls with full inputs and answers in development only |
| Tests from requests | `jadpo test new --from <request id>` writes a replay test |
| `jadpo call` | One-shot `jadpo call <name> --as <user>`, JSON in and out, local writes with `--rollback`, production refused, policy always applies |
| Developer console and MCP | Local, read-only console in `jadpo dev`; CLI first for agents; MCP later for agents without a shell, generated from the command reference, read-only, OAuth-protected |
| Review report and view | `jadpo approval` ordered decisions first; Markdown and JSON; the graph web view later; acceptances stay files |
| Contract and order | The evidence and commands are decided; console, MCP, review view and breakpoints are product work; build order as listed under Status |
| Coverage by path | Paths run by tests, untested production paths by traffic, paths unseen for 90 days |
| Production in review | Optional production counts in the approval report |
| Production replay | Opt-in per deployment, failing requests only, personal fields replaced, recordings expire |
| Query cost of a change | Queries per path computed by the compiler in the approval report |
| Unaccepted translations | With `TranslationReview.required`, changed wording not yet accepted appears in the approval report (area 10) |
| Watching code run | `jadpo watch` on a named route, action, query, subscriber, schedule, provider operation, domain or file, optionally failures or faults only; prints inputs, outputs, what ran inside and the request id; `--once`, `--count`, `--timeout`, `--json`; development only, values in full, secrets never |
| Development call log | Provider calls faked by default, let through to the sandbox by name; every call logged with full inputs and answers in development only, its source, attempts, duration and request; `jadpo calls`, `calls show`, `calls script`, `--json` |
| Telemetry in development | Nothing exported unless `--export-telemetry`; `jadpo traces`, `metrics` and `logs`, filterable by route, action, domain, path, failure and slowness, `--follow`, `--json` |
| Packages in the approval report | Unverified third-party packages from git, new or changed service images, outbound grants for services, connections that cannot erase and fail-open bot checks need a person's acceptance |
