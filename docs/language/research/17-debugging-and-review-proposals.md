# Debugging and review: research and proposals

Research behind [area 17](../17-debugging-and-review.md). Two questions: how
does a person or an agent get from a symptom to the `.jadpo` line and the
reason, and how does a reviewer understand what a large model-written change
does? The second was first raised as a custom review UI (archived roadmap
E06, "Revset and the web review UI"); the first as a developer console and
MCP server (archived E11, RM-1101 to RM-1108) and revision-matched source
maps (RM-219).

## What others do

| Tool | What it shows | What Jadpo takes from it |
|---|---|---|
| Laravel Telescope | A local web page of "watchers": requests, queries with SQL and a slow threshold, jobs, exceptions with stacks, events and listeners, cache hits and misses, gate and policy checks, mail, outgoing HTTP | The request timeline, and that policy and cache decisions are worth recording |
| Rails development log and console | Each SQL statement followed by the app file and line that ran it (verbose query logs, on in development); `rails console --sandbox` rolls back on exit | Query lines in the timeline; `--rollback` on `jadpo call` |
| Phoenix LiveDashboard | Request logging per request, database table and index statistics, metrics from telemetry | A local page reading telemetry the framework already emits |
| Encore local dashboard | Opens with `encore run`: service catalogue, API explorer, traces of every request (queries, calls, messages) from compile-time instrumentation, and a "Flow" architecture diagram | Closest match: the compiler knows the application, so the dashboard needs no setup; the graph comes from the compiler |
| Temporal Web UI | Each workflow's event history (start, close, every event and its fields), saved filters, download as JSON; the CLI shows the same history | One evidence record behind UI, CLI and JSON; deliveries shown by state |
| Revset | A large AI-written diff turned into a graph of changed functions and calls; flags a removed method that still has callers, new code with no callers, signature changes with their call sites; an interdiff between an agent's revisions fades unchanged nodes | Review starting from structure and behaviour, then source; interdiffs between revisions |
| Graphite | Stacked pull requests reviewed bottom-up, each as if it stands alone; marks when code already reviewed changes further up the stack | Large changes split into reviewable pieces; flag re-changed code |
| Reviewable | Shows only what changed since you last reviewed; comments marked blocking or resolved | "Since last review" view for agent revisions |
| Sentry | Errors grouped by release; uploaded source maps turn minified stacks into source lines for the exact release | Each event names its build; the map of that build resolves it |
| Tidewave | An MCP server for Phoenix and Rails apps that lets an agent read logs, run code and inspect the running app | Prior art for an agent-facing runtime server; Jadpo's would be read-only |

Sources:
[Laravel Telescope](https://laravel.com/docs/telescope),
[Rails debugging guide (verbose query logs)](https://guides.rubyonrails.org/debugging_rails_applications.html),
[Rails sandbox console](https://makandracards.com/makandra/22901-rails-sandbox-console),
[Phoenix LiveDashboard](https://github.com/phoenixframework/phoenix_live_dashboard/),
[Encore local development dashboard](https://encore.dev/docs/ts/observability/dev-dash),
[Encore tracing](https://encore.dev/product/tracing),
[Temporal Web UI](https://docs.temporal.io/web-ui),
[Revset](https://revset.dev/),
[Graphite: reviewing stacks](https://graphite.dev/docs/best-practices-for-reviewing-stacks),
[Sentry source maps](https://docs.sentry.io/platforms/javascript/sourcemaps/),
[Tidewave for Phoenix](https://github.com/tidewave-ai/tidewave_phoenix),
[Bun debugger](https://bun.com/docs/runtime/debugger),
[Bun VS Code debugging](https://bun.com/docs/guides/runtime/vscode-debugger),
[Bun bundler source maps](https://bun.com/docs/bundler).

**What none of them has.** Every tool above shows what happened; none can
say *why the framework decided it*, because policy, soft delete, caching and
delivery are library code to them. Jadpo's compiler owns those rules, so
`jadpo why` can explain them exactly. That is the one genuinely new part.

## Source maps under Bun

Bun maps its own transpilation: a TypeScript error points at the `.ts` line,
at runtime and in the debugger. `bun build` can emit linked, external or
inline maps. But a map from `.ts` back to another language is not applied:
in a probe with Bun 1.2.20, an inline `sourceMappingURL` pointing a `.ts`
file at a `.jadpo` file was ignored at runtime, the same for `.js`, and
`bun build --sourcemap` produced a map to the `.js` file, not through it.
Bun's own guide also calls its VS Code debugging extension unreliable and
recommends the web debugger.

Options:

1. **Rely on Bun.** Emit a standard map and hope Bun applies it. Fails today.
2. **The runtime maps its own frames.** The compiler writes a map from
   generated lines to `.jadpo` lines; the runtime rewrites each stack frame
   before logging. Works today, under any host, and the same map serves
   `jadpo incident`.
3. **Explicit sites only.** The compiler passes a `site` constant into every
   call. Precise for failures (area 4 already logs `site`) but no stack for
   a defect deep in a helper.

*Recommendation:* 2, with 3 kept for domain failures as area 4 decided.
Breakpoints wait: they need Bun's inspector to honour the map, which it does
not now.

## The evidence contract

The archived plans asked for "one versioned evidence contract" behind the
CLI, the console and MCP. Concretely, five records, each with a
`schemaVersion`, the build id and the commit:

| Record | Written by | Read by |
|---|---|---|
| Fault and failure events | Runtime, to the log | `jadpo incident`, `jadpo why`, console, MCP |
| Request timeline | Runtime under `jadpo dev`, to `build/dev/` | `jadpo trace`, `jadpo why`, `test new --from`, console, MCP |
| Application graph | Compiler, to `build/` (today's `semantic_graph` in `app.meta.json`, extended) | `jadpo impact`, console, MCP, review |
| Reports | Compiler, to `build/report/` (permissions, personal data, schema, queries) | console, MCP, review |
| Review report | `jadpo approval --before` | the review web view, CI, MCP |

Options for where the views live:

1. **One local server** (`jadpo dev`) serving the console, plus `jadpo mcp`
   reading the same files. Simple; local only.
2. **A hosted service** receiving production telemetry (archived RM-1107, a
   possible paid product). Useful later, but a business decision, and
   OpenTelemetry already gets production data to existing tools.
3. **Console in the generated app itself**, behind a route. Rejected: it
   puts a debugging surface into production code and policy.

*Recommendation:* 1 now; 2 stays parked on the roadmap.

## The review report

The approval report exists but is about 6 MB of JSON for the golden todo,
with nothing ordered for a reader. Three shapes were considered:

1. **A better line diff** (semantic diff tools hide formatting and show
   moves). Helps, but still makes the reviewer infer behaviour.
2. **A graph first** (Revset). Good for structure; a reviewer still has to
   work out what a structural change means for users and data.
3. **Behavioural decisions first**, then the paths that make each true,
   newly reachable code, other behaviour, then source by declaration. The
   graph and interdiff are views inside it.

*Recommendation:* 3, as Markdown and JSON from `jadpo approval --before`,
with a static web view in `build/review/` that CI publishes beside the pull
request. It holds no accounts and no approvals: acceptances are files in
`acceptances/`, reviewed on the git host (area 7). The archived plan's
signed attestations are superseded by that decision.

The archived requirement to measure comprehension against a plain diff
(RM-605) still stands for the web view: it should be tried on one real
policy-widening change before more is built.

## What is contract and what is product

| Part | Kind | Order |
|---|---|---|
| `.jadpo` frames in every stack and event; build and commit on every event | Contract (this review) | 1 |
| Request timeline in `jadpo dev`; `jadpo trace` | Contract | 2 |
| `jadpo why` | Contract | 3 |
| `jadpo test new --from` | Contract | 4 |
| `jadpo call` | Contract | 5 |
| Review report order and JSON | Contract | 6 |
| `jadpo incident` across changed code (stable identity for actions, queries, routes) | Contract; needs area 5's registry extended | 7 |
| MCP server | Product (E11) | 8 |
| Developer console, application graph with live overlays | Product (E11) | 9 |
| Review web view, interdiffs | Product (E06) | 10 |
| Breakpoints in `.jadpo` | Product, after a Bun probe | Later |
| Hosted monitoring | Product decision (RM-1107) | Parked |

MCP comes before the console because agents are the main users and it is
a thin layer over commands that already return JSON.
