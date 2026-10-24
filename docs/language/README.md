# Jadpo language specification

Jadpo is an experimental, statically typed language for web backends written
mainly by LLMs and reviewed by people. These 21 decided areas define the
language. They take precedence over introductions and teaching examples.
The [project README](../../README.md) explains the purpose, capabilities,
current maturity and how to build the compiler.

**Decided does not mean implemented.** Read each page's status notes and
status table, where present, along with the [roadmap](../roadmap.md).
Pages retain earlier proposals and examples; the current decisions and
later amendments govern where they differ. A `jadpo check` code block is
checked with this checkout's compiler; a `fragment` illustrates a rule
without an executable check. Other examples may still use frozen results
from the archived compiler, as described below.

## Read by task

- **Learn the language:** read areas 1–4, then the
  [first nine teaching projects](../../examples/README.md).
- **Build a data-backed HTTP API:** add areas 5–9 and 13; the
  [cake shop](../../examples/11-cake-shop/README.md) and
  [recipe book](../../examples/12-recipe-book/README.md) put these together.
- **Add providers or background work:** read 10–12, 13 and the relevant
  parts of 16; see the [delivery desk](../../examples/26-delivery-desk/README.md).
- **Use AI or vector search:** read 21, plus the data, policy, integration
  and testing rules it links; see [lost property](../../examples/28-lost-property/README.md).
- **Review the design:** read [why Jadpo exists](../vision.md), then 9 and
  15–18. Several review and personal-data capabilities remain planned.
- **Contribute to the compiler:** read [AGENTS.md](../../AGENTS.md),
  [contributing](../../CONTRIBUTING.md) and the
  [compiler architecture](../compiler-runtime.md).

## All 21 areas

| Area | Specification | What it defines |
|---|---|---|
| 1 | [Project layout and modules](01-project-layout-and-modules.md) | Domains, file paths, visibility, imports and naming |
| 2 | [Values and types](02-values-and-types.md) | Named types, validation, refinements, objects, enums and collections |
| 3 | [Logic](03-logic.md) | Functions, actions, expressions, control flow and effects |
| 4 | [Failures](04-failures.md) | Declared domain failures, handling, propagation and runtime faults |
| 5 | [Data](05-data.md) | Entities, relationships, storage, schema history and migrations |
| 6 | [Reads and writes](06-reads-and-writes.md) | Queries, pagination, aggregates, transactions and concurrency |
| 7 | [HTTP](07-http.md) | Routes, boundary contracts, API records and OpenAPI |
| 8 | [Identity](08-identity.md) | Principals, sessions, tokens, keys, magic links and passkeys |
| 9 | [Policy](09-policy.md) | Ownership, grants, roles, tenancy and field-level access |
| 10 | [Configuration and time](10-configuration-and-time.md) | Settings, secrets, connections, clocks, calendars and translations |
| 11 | [Integrations](11-integrations.md) | Provider packages, capabilities, connections, webhooks and recordings |
| 12 | [Jobs and events](12-jobs-and-events.md) | Events, outbox delivery, subscribers, schedules and batches |
| 13 | [Testing](13-testing.md) | Test kinds, fixtures, real databases, fakes and controlled clocks |
| 14 | [Tooling](14-tooling.md) | CLI commands and their status, formatter, editor support and deployables |
| 15 | [Personal data](15-personal-data.md) | Marking, export, erasure, retention and privacy reporting; largely planned |
| 16 | [Built-in services](16-built-in-services.md) | Email, files, rate limits, search, audit and telemetry; status varies |
| 17 | [Debugging and review](17-debugging-and-review.md) | Source-level evidence, request explanations and behaviour review; planned |
| 18 | [Records and manifest](18-records-and-manifest.md) | Committed contracts, generated manifests and planned approval records |
| 19 | [Standard library](19-standard-library.md) | Standard vocabulary types and operations, money, units, text and formats |
| 20 | [Geospatial](20-geospatial.md) | Geometry, geodesic operations, PostGIS and map layers |
| 21 | [AI](21-ai.md) | Prompts, model calls, streaming, embeddings, budgets and personal-data flow |

## Reading with an agent

Give the agent this index and the actual area files needed for its task.
An index is not a substitute for their rules, and some URL tools do not
follow links. The root [llms.txt](../../llms.txt) lists absolute plain-text
URLs for every area; the [README's agent guide](../../README.md#use-jadpo-with-an-ai-agent)
explains local file access, direct links and Gemini import.

Ask the agent to cite the sections it read and distinguish implemented
behaviour from decided future work. For code changes, use the compiler's
structured diagnostics and tests to check the result.

## Maintaining the specification

`tools/language_docs.py` checks these pages and renders them into
`build/language-site/`. The rendered decision log collects the decisions
from every area.

```sh
python3 tools/language_docs.py --check
python3 tools/language_docs.py
```

## Writing a page

- Front matter gives `title`, `area`, `status`, `checked-with` (what checks
  its examples: this checkout's `jadpo` for pages with ` ```jadpo check `
  blocks, and the commit that produced the frozen results for blocks still
  on them) and `rules`: the
  compiler error IDs this area owns (globs allowed). Each rule belongs to one
  area, and the check fails until every owned rule has an example.
- A ` ```jadpo check ` block is checked by the new `jadpo`, built from this
  checkout, with `jadpo check`, which grows with each roadmap chunk: the
  block must check cleanly and be canonically formatted (`jadpo fmt
  --check`), or, with `expect=rule.id`, fail with that rule and be followed
  by a ` ```text output ` block holding the exact plain output (`python3
  tools/language_docs.py --bless` rewrites it). A project with an
  `app.jadpo` block is checked as a project, with the layout rules on;
  without one, its blocks are loose files. Each chunk converts its areas'
  blocks as it builds them.
- Every other non-fragment ` ```jadpo ` block is checked against a frozen
  result in `reference/v0-results.json`, by project name and source, so
  changing its source fails the check; proposed code belongs in `fragment`
  blocks, and each chunk replaces these blocks with `check` blocks as it
  builds its area. Attributes after the language name:
  - `project=name` groups blocks into one multi-file project;
  - `file=path` sets the file's path inside it (default `main.jadpo`);
  - `expect=rule.id` means the project must fail with that error; otherwise it
    must compile and be canonically formatted;
  - `hidden` compiles the block without showing it;
  - `fragment` shows the block without compiling it.
- Under `## Errors`, each `### \`rule.id\`` heading is followed by its example.
  A `check` example shows the compiler's real output; an unmarked one keeps
  its frozen result until its area is built. Each area's errors are designed
  as the roadmap chunk that builds it.
- A line starting with `>` renders as a note.
- Once `status: decided`, a page with `### Decided …` sections under
  `## Review` leads with them and `Decisions`, and collapses the rest of its
  body; every decided page collapses its remaining review notes.
- `## Review` holds status, consistency notes, open questions (a numbered list
  of `**Question?** context *Recommendation:* …`) and decisions. Generated
  public documentation leaves it out.
