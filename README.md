# Jadpo — a programming language for AI-written backends

Jadpo is an experimental, statically typed programming language for web
backends written mainly by LLMs and reviewed by people. Data, HTTP APIs,
authentication, permissions, external services and background work belong
to one application model that the compiler checks.

The aim is to make a large AI-written change reviewable: what can this
application do, who can do it, and which data and outside services can it
reach? A model writes the implementation; the compiler checks the rules;
a person owns the intent and permission decisions.

[Try Jadpo](#try-jadpo) · [Language specification](docs/language/README.md) ·
[Examples](examples/README.md) · [Explore with your agent](#explore-with-claude-code-or-codex) ·
[Roadmap](docs/roadmap.md) · [Contribute](CONTRIBUTING.md)

## Explore with Claude Code or Codex

Want a guided introduction? Give your coding assistant this prompt. It can
explain the design, walk through real code and help you run a first example.
A local clone gives it access to the full specification and tests.

```text
Help me explore Jadpo: https://github.com/richardwillars/jadpo
Use a local clone, or clone it into a new directory. Read README.md and
AGENTS.md, then docs/language/README.md and the current decisions and status
notes in areas 1–4. Explain why Jadpo exists, what it can build, how its
checks work and what is still planned. Cite the files you actually read.
Walk me through examples/01-first-project, including its source and tests.
Check the prerequisites, build the CLI and run check and test on that
example. If a prerequisite is missing, explain what I need to install.
Then help me try a small change in a separate copy of the example and use
the compiler and tests to understand the result. Follow the relevant
specification pages as we go.
```

Already have the repository? Open its folder in your assistant. In a
terminal, run `claude` for [Claude Code](https://code.claude.com/docs/en/quickstart)
or `codex` for [Codex](https://learn.chatgpt.com/docs/cli), then paste the
prompt. You can also open the local folder in their desktop or editor
interfaces. No Jadpo-specific agent integration is required.

Prefer to read and run it yourself? [Start with the setup below](#try-jadpo).
For other assistants, see [direct links and Gemini import](#giving-an-assistant-direct-links).

## Why build a language for this?

Coding agents can produce more software than a person can comfortably
review line by line. In a conventional backend, important facts are spread
across framework conventions, middleware, database code, configuration and
instructions to the agent. Jadpo makes more of those facts explicit in the
language, so a deterministic checker can follow them across the application.

- **Less architecture to infer.** Every project uses the same domain-based
  layout. A declaration's kind and business domain determine its file path;
  imports say where each name comes from.
- **Meaning in the types.** An order ID and a customer ID are different
  types even when both are UUIDs. Named types carry validation rules;
  values cross boundaries through checked contracts.
- **Failures a caller must consider.** An action declares its possible
  domain failures. Each caller must handle them or declare that it passes
  them on.
- **Permissions in the application model.** Entity and field grants,
  ownership and tenancy are checked together with queries, outputs and
  writes. The compiler produces a permissions report.
- **One graph for the tools and executable.** The Rust compiler builds a
  typed graph with control flow. Checking, editor tooling, the development
  interpreter and code generation read that graph, giving review tools a
  common account of the program.

These checks do not establish that the requirements are right. Human
review and application tests remain necessary. The longer explanation is
in the [vision and design philosophy](docs/vision.md).

## What can you build with Jadpo?

Jadpo targets data-backed HTTP services: business applications, APIs,
applications serving several customers, integrations and background
workflows. It can also call AI providers and search stored embeddings.
Your frontend can use its HTTP API; browser UI components and CSS are
outside Jadpo's scope.

The following capabilities are implemented in the experimental compiler.
The linked specification pages define their rules and record limitations.

| Capability | What Jadpo provides | Learn more |
|---|---|---|
| Types and business logic | Named and refined types, objects, enums, collections, pure functions, actions and checked failures | [Types](docs/language/02-values-and-types.md), [logic](docs/language/03-logic.md), [failures](docs/language/04-failures.md) |
| Stored data | PostgreSQL entities, references, constraints, migrations, named queries, pagination, aggregates and transactional writes | [Data](docs/language/05-data.md), [reads and writes](docs/language/06-reads-and-writes.md) |
| HTTP APIs | Typed request and response contracts, routes, API records and generated OpenAPI documents | [HTTP](docs/language/07-http.md) |
| Identity and access | Sessions, tokens, service keys, magic links and passkeys; ownership, roles, tenancy and field permissions | [Identity](docs/language/08-identity.md), [policy](docs/language/09-policy.md) |
| Configuration and everyday values | Typed settings and secret bindings, time zones, recurring times, translations, money, units and validated standard types | [Configuration and time](docs/language/10-configuration-and-time.md), [standard library](docs/language/19-standard-library.md) |
| External services | Typed provider packages and connections, webhooks, recordings, test fakes and multi-step calls | [Integrations](docs/language/11-integrations.md) |
| Background work | Events, a transactional outbox, subscribers, retries, batches and schedules | [Jobs and events](docs/language/12-jobs-and-events.md) |
| Testing and tools | Pure, action, route and schedule tests; real test databases; controlled clocks and provider fakes; watch mode, formatter and language server | [Testing](docs/language/13-testing.md), [tooling](docs/language/14-tooling.md) |
| Geography | Points, lines and polygons, geodesic operations, PostGIS queries and GeoJSON map layers | [Geospatial](docs/language/20-geospatial.md) |
| AI inside your app | Checked prompt files, typed model answers, streaming, token budgets, embeddings, pgvector search and checks on personal data sent to models | [AI](docs/language/21-ai.md) |

## Project status

**Experimental and under active development.** The language has 21 decided
specification areas. A decided feature is not necessarily implemented:
each area's status notes and the [roadmap](docs/roadmap.md) distinguish the
two.

The compiler supports an interpreter for development and tests, compiled
native deployables, and WebAssembly deployables for Cloudflare Workers.
The verification gate compares interpreted and compiled test outcomes and
includes tests under Cloudflare's local `workerd` runtime. Performance work
is in progress; a real Cloudflare deployment remains an explicit roadmap
validation step.

Automated personal-data export, erasure and retention; the full behavioural
change review and approval workflow; richer debugging tools; the developer
console; an MCP interface; and a public package registry are planned work.
Existing checks on personal data sent to AI models do not mean that the
whole [personal-data specification](docs/language/15-personal-data.md) is built.

Start by exploring and testing the examples. Production readiness and
performance parity with established stacks have not been established.

## Try Jadpo

Install Git and Rust through `rustup`, then build from source. The
repository's `rust-toolchain.toml` pins the Rust toolchain and components.
The first example needs no database or AI-provider credentials.

```sh
git clone https://github.com/richardwillars/jadpo.git
cd jadpo
cargo build --release --locked -p jadpo-cli
target/release/jadpo check examples/01-first-project
target/release/jadpo test examples/01-first-project
```

The [28 teaching projects](examples/README.md) progress from a first
function to databases, authentication, permissions, integrations and AI.
Each has a reading order, comments, tests and instructions. Examples that
use stored data need PostgreSQL; see [contributing](CONTRIBUTING.md) for
the build and verification prerequisites.

For an existing example, the development tools include:

```sh
target/release/jadpo fmt --check examples/01-first-project
target/release/jadpo test --watch examples/01-first-project
target/release/jadpo check --json examples/01-first-project
```

See [tooling](docs/language/14-tooling.md) for development servers, native
and Workers builds, editor support, commands and their implementation status.

## What Jadpo code looks like

A type gives a value a meaning and its validation rules. This one comes
from the [bakery example](examples/09-bakery-orders/README.md):

```jadpo
type PenceCount = Int {
    min: 0
}
```

An action makes its failures and recovery choices visible. This excerpt
from the same example handles an unknown or wrong-currency discount code
by charging full price, while passing other failures to its caller:

```jadpo
use shared/types { Sterling }
use sales/types { Basket }
use sales/failures { BasketEmpty, CodeNotInPounds, CodeUnknown, DiscountTooLarge }
use sales/actions { price_basket, price_lines }

action till_total(basket: Basket) fails BasketEmpty, DiscountTooLarge -> Sterling {
    return match price_basket(basket) {
        success(receipt) -> receipt.total
        CodeUnknown -> price_lines(basket)
        CodeNotInPounds -> price_lines(basket)
        BasketEmpty -> propagate
        DiscountTooLarge -> propagate
    }
}
```

The [complete source](examples/09-bakery-orders/domains/sales/actions/till_total.jadpo)
and its [tests](examples/09-bakery-orders/domains/sales/actions/till_total.test.jadpo)
show the surrounding declarations. Handled failures leave the action's
`fails` list; unhandled failures prevent it from checking.

For the wider application model, follow the
[recipe book's tenancy and field permissions](examples/12-recipe-book/README.md),
the [repair café's events and outbox](examples/23-repair-cafe/README.md), and
[lost property's typed AI calls and vector search](examples/28-lost-property/README.md).

## How the compiler works

```text
.jadpo source → typed application graph → checks, reports and editor tooling
                                      → interpreter for development and tests
                                      → generated Rust → native or WebAssembly
```

The graph includes declarations, calls and control flow. Stored schemas,
API contracts and package locks have committed records whose changes can
be reviewed; manifests and reports are regenerated from the checked
application. The planned change-review workflow will compare manifests
to show behaviour and permission changes before a reviewer reads the code.

See the [compiler and runtime architecture](docs/compiler-runtime.md) and
[records and manifest specification](docs/language/18-records-and-manifest.md)
for what is built and how the remaining review tools fit.

## Use Jadpo with an AI agent

Jadpo is new: give an assistant the specification rather than expecting it
to know the language from training or infer it from one example. You can
use any coding assistant that can read the relevant files and run the CLI;
Jadpo does not require a particular model provider.

1. **Read the language map:** [all 21 specification areas](docs/language/README.md).
   Start with layout, types, logic and failures (areas 1–4).
2. **Read the rules for the task:** for an authenticated API that changes
   data, continue with areas 5–9 and 13. For integrations, read 11–12;
   for AI features, read 21. Read the current decisions and status notes
   as well as the examples; pages retain older alternatives for context.
3. **Open a complete example** from the [example index](examples/README.md),
   including its tests. Check intended features against their area pages
   and the roadmap before using them.
4. **Run the compiler and tests.** Use `jadpo check --json` for structured
   diagnostics, then `jadpo test`. Treat a missing rule or unresolved
   permission decision as something to resolve explicitly.

For work on the compiler itself, also read [AGENTS.md](AGENTS.md),
[contributing](CONTRIBUTING.md) and the architecture document.

### Giving an assistant direct links

For assistants with web-fetch tools, start with the
[plain-text README](https://raw.githubusercontent.com/richardwillars/jadpo/main/README.md)
and [plain-text specification index](https://raw.githubusercontent.com/richardwillars/jadpo/main/docs/language/README.md).
The [llms.txt reading index](llms.txt) supplies absolute, direct links to
every area. It is a navigation aid: assistants may need each relevant URL
provided explicitly, and file access depends on their tools and GitHub's
access controls. A link does not guarantee that its contents were read.

### Gemini repository import

In the Gemini web app on a computer, use **Add file → More uploads → Import
code**, then enter `https://github.com/richardwillars/jadpo`. Google's
[GitHub import documentation](https://support.google.com/gemini/answer/16176929?hl=en)
says that the GitHub app cannot read a repository merely from a URL in the
prompt. It supports one repository per chat, up to 5,000 files and 100 MB;
the import is a snapshot and does not sync later changes.

The Gemini API's [URL context tool](https://ai.google.dev/gemini-api/docs/url-context)
is a separate mechanism: it can fetch supplied public URLs, but does not
follow their nested links automatically. Supply the individual pages the
task needs and verify retrieval. If importing or fetching is unavailable,
attach the relevant Markdown and example files, or use a local clone with
a file-reading coding agent.

## Contribute and follow the project

Jadpo is maintained by [Richard Willars](https://github.com/richardwillars).
The public home for the project is currently
[this GitHub repository](https://github.com/richardwillars/jadpo).

- [Contributing](CONTRIBUTING.md): setup, the verification gate and working rules.
- [Roadmap](docs/roadmap.md): the only tracker for planned implementation work.
- [Issues](https://github.com/richardwillars/jadpo/issues): report bugs or ask questions.
  Include the source, compiler output and expected behaviour where possible.
- [Documentation index](docs/README.md): architecture, design rationale and decisions.

Feedback on the language model, diagnostics and complete examples is
particularly useful. Check the roadmap and open pull requests before
starting implementation, since other people and agents may be working on it.
Star the repository to find it again, or use GitHub's Watch menu for updates.

## Licence

Code is [Apache 2.0](LICENSE); documentation is [CC BY 4.0](docs/LICENSE.md).
See [TRADEMARKS.md](TRADEMARKS.md) for use of the Jadpo name and the naming
requirements for modified versions. Design skills in `.agents/skills/`
are third-party material under their own licences.
