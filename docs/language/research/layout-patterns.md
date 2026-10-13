# Project layout patterns: research for Jadpo

Researched 2026-10-11. This is input to the [project structure workstream](../../archive/specs-v0/project-structure.md)
and to [Project layout and modules](../01-project-layout-and-modules.md). It is
evidence for choosing a layout, not the decision itself.

Every claim carries one of four labels:

- **[M] measured:** a controlled or quantitative result.
- **[D] documented:** framework docs or compiler behaviour.
- **[P] practitioner report:** experience from people running real codebases.
- **[O] opinion or inference:** an argument, mine or a named author's.

## 1. Layout families

| Family | Exemplars | Path is keyed on | Enforced by tooling? |
|---|---|---|---|
| Role-first (MVC folders) | Rails, Laravel, Redwood `api/src/services` | technical role | Rails: file name must match the constant ([Zeitwerk](https://guides.rubyonrails.org/autoloading_and_reloading_constants.html)). Laravel: "free to organize your application however you like" ([docs](https://laravel.com/docs/12.x/structure)) |
| Domain modules / contexts | Django apps, Phoenix contexts, Ash domains, NestJS modules, Spring Modulith | business area, then role or resource | Mostly advisory. Ash says "none of the things we show you here are requirements" ([docs](https://ash.hexdocs.pm/project-structure.html)). Spring Modulith checks module dependencies with `verify()` ([docs](https://docs.spring.io/spring-modulith/reference/fundamentals.html)) |
| Service-per-folder | Encore.ts / Encore.go | deployable/API boundary | Yes. A folder with `encore.service.ts` is a service, and services cannot nest ([docs](https://encore.dev/docs/ts/primitives/app-structure)) |
| Route-as-folder | Next.js app router, SvelteKit, Remix | URL | Yes for routes. Next.js is "unopinionated" about everything else ([docs](https://nextjs.org/docs/app/getting-started/project-structure)) |
| Vertical slice / feature-first | Bogard's vertical slices, Feature-Sliced Design, "package by feature" | use case or feature | FSD has a strict import rule ([docs](https://feature-sliced.design/docs/get-started/overview)). Vertical slices have no tooling |
| Hexagonal / clean | ports/adapters folder trees | dependency ring | Convention only |
| Go | `cmd/`, `internal/`, one package per directory | package = directory | The compiler and `go` tool enforce `internal/` ([Go 1.4 notes](https://go.dev/doc/go1.4#internalpackages)) and reject import cycles ([spec](https://go.dev/ref/spec#Import_declarations)). The layout beyond that is guidance only ([go.dev](https://go.dev/doc/modules/layout)) |
| Monorepo | Nx `apps/`+`libs/`, Turborepo `apps/`+`packages/` | deployable vs library, then domain | Nx lints domain boundaries using project tags ([docs](https://nx.dev/docs/kb/folder-structure)) |

## 2. What developers praise and complain about

**Role-first is predictable, but features scatter.** The Rails doctrine says
conventions are decisions "that just need to be made once" ([doctrine](https://rubyonrails.org/doctrine)) [D/O].
At scale, the cost shows up. Shopify reorganised about 6,000 Rails classes from
"software concepts (models, views, controllers)" into business components
(orders, billing and so on) so code was easier to find and coupling was visible ([Shopify](https://shopify.engineering/deconstructing-monolith-designing-software-maximizes-developer-productivity)) [P].
Simon Brown notes that package-by-layer is the default only because books and
framework samples teach it, and that it tends towards a "big ball of mud" ([DZone](https://dzone.com/articles/package-component-and)) [O].
Ben Johnson argues the Rails-style "group by functional type" layout carries
badly into Go ([post](https://medium.com/@benbjohnson/standard-package-layout-7cdbc8391fc1)) [O].
Bogard's vertical slices exist because one feature change touches every layer ([post](https://www.jimmybogard.com/vertical-slice-architecture/)) [O].

**Domain-first is cohesive, but naming boundaries early is hard.** Phoenix
contexts are the clearest case. Arrowsmith argues they "do more harm than good"
for newcomers, because "you have to think of context names long before it's clear what the contexts should even be",
and contexts fill up with thin `Repo` wrappers ([post](https://arrowsmithlabs.com/blog/did-contexts-kill-phoenix)) [P/O].
An ElixirConf talk describes teams struggling once apps grow, and recommends
letting boundaries emerge ([talk](https://elixirconf.com/talks/phoenix-contexts-in-the-real-world-from-theory-to-survival/)) [P].
Django developers report the same thing. DoorDash found that cross-app foreign
keys produced migration orderings that sometimes would not apply to a blank
database ([DoorDash](https://careersatdoordash.com/?p=144)) [P]. Forum advice
converges on "one app until proven otherwise" ([forum](https://forum.djangoproject.com/t/best-practices-for-avoiding-circular-imports-and-maintaining-app-independence-in-django/37946),
[forum](https://forum.djangoproject.com/t/why-do-we-need-apps/827)) [P]. Encore
likewise says a new app "should generally start with one service" ([docs](https://encore.dev/docs/ts/primitives/app-structure)) [D].

**Teams that keep a role-first layout still re-layer inside it.** The HackSoft
Django style guide adds `services.py` (writes) and `selectors.py` (reads) to
each app, and splits them into sub-domain packages as they grow ([guide](https://github.com/HackSoftware/Django-Styleguide)) [P].
This is the same read/write split Jadpo already makes with queries and workflows.

**Colocating by route works, but every file has the same name.** Next.js lets
non-route files sit inside route folders safely ([docs](https://nextjs.org/docs/app/getting-started/project-structure)) [D].
The recurring complaint is that every file is called `page.tsx`, which makes
tabs, diffs and file lists ambiguous ([r/nextjs](https://www.reddit.com/r/nextjs/comments/1kf2qzq/every_file_is_pagetsx/)) [P].
Someone filed this as a feature request for an AI editor's file listing ([Cursor forum](https://forum.cursor.com/t/show-path-and-not-just-filename-in-composer-file-listing/21697)) [P].
SvelteKit's `+page.svelte` draws the same comment ([dev.to](https://dev.to/mjoycemilburn/ngatesystems-post-series-v2-24-4421)) [P].
When folders carry meaning, they also need a growing set of special syntaxes.
Next.js uses `(group)`, `_private`, `@slot` and `(..)intercept` ([docs](https://nextjs.org/docs/app/getting-started/project-structure)) [D].
Remix v2 moved to dot-delimited flat file names, and kept an opt-out package for
the old folder convention ([Remix](https://v2.remix.run/docs/start/v2)) [D].

**Putting the kind in the file name is contested.** Angular v20 stopped
generating `.component.ts`/`.service.ts` suffixes. Users pushed back, asking
for the old naming back on readability grounds ([#30609](https://github.com/angular/angular-cli/issues/30609),
[#30594](https://github.com/angular/angular-cli/issues/30594)) [P].

**"Shared" needs a rule.** Nx's guidance: "Group libraries by the application
or business domain they serve, not by their technical type", and "promote a
library into `shared` when a second domain needs it" ([Nx](https://nx.dev/docs/kb/folder-structure)) [D].
FSD makes `shared` the bottom layer, which may not import upward ([FSD](https://feature-sliced.design/docs/get-started/overview)) [D].

**A layout is only a standard if it is enforced or official.** The unofficial
`golang-standards/project-layout` repository was widely treated as standard.
Russ Cox opened an issue saying it is not, that most Go packages don't use
`pkg/`, and that the "golang-standards" name misleads people ([#117](https://github.com/golang-standards/project-layout/issues/117)) [P].
The official guide is deliberately minimal: start with one file, and keep server
code in `internal/` ([go.dev](https://go.dev/doc/modules/layout)) [D].

## 3. What helps LLM coding agents navigate

The direct evidence is thin. No study I found tests layout as a variable.

| Finding | Label | What it implies for layout |
|---|---|---|
| Claude Code's search is "really just glob and grep", and it beat RAG and vector indexes ([Pragmatic Engineer](https://newsletter.pragmaticengineer.com/p/building-claude-code-with-boris-cherny)) | [P] | Agents find code through paths and names, so predictable, unique file names pay off directly [O] |
| Agentless localises faults by showing the model a `tree`-style listing of the repository and asking it to pick the suspicious files ([arXiv 2407.01489](https://arxiv.org/abs/2407.01489)) | [D] (method) | The file tree itself is a navigation signal. Paths that state the kind and the name carry more information [O] |
| SWE-agent: a 100-line file viewer resolved 18.0% of tasks, against 14.3% with 30 lines and 12.7% with whole files. Summarised search beat iterative search, 18.0% vs 12.0% ([arXiv 2405.15793](https://arxiv.org/abs/2405.15793)) | [M] | Tool design matters. Small, single-concept files fit the window, but this tested the viewer, not file size [O] |
| Context files did not raise success rates and cost over 20% more. Repository overviews were "not helpful". Context files help mainly with "non-standard" practices ([arXiv 2602.11988](https://arxiv.org/abs/2602.11988)) | [M] | A layout the agent already knows needs no explanation. Each project-specific deviation costs context [O] |
| AGENTS.md was associated with 28.6% lower median runtime across 10 repos and 124 PRs. This is a correlation ([arXiv 2601.20404](https://arxiv.org/abs/2601.20404)) | [M] | Weak support for written conventions. Conflicts in part with the row above |
| Models use information in the middle of long contexts poorly ([arXiv 2307.03172](https://arxiv.org/abs/2307.03172)) | [M] | Indirect support for small files [O] |
| Colocating tests with code tells readers a module is tested and reminds them to update the tests ([Kent C. Dodds](https://kentcdodds.com/blog/colocation)). Go puts `_test.go` files beside the code ([testing](https://pkg.go.dev/testing)) | [O]/[D] | Tests next to their subject |

## 4. Does enforcement help or annoy?

- **It helps when the rule is mechanical and lossless.** Zeitwerk makes file
  names define constants, which is the basis of Rails navigability ([guide](https://guides.rubyonrails.org/autoloading_and_reloading_constants.html)) [D].
  Go's `internal/` and no-cycle rules are rarely debated. The layout fight in Go
  is about the parts that are *not* enforced (§2) [O].
- **It annoys when the mapping loses information.** Zeitwerk derives
  `app/models/vat.rb` → `Vat`, so a class named `VAT` fails until someone adds an
  inflection rule ([dev.to](https://dev.to/rodreegez/switching-to-zeitwerk-366n)) [P].
  Route-as-folder conventions produce identical file names (§2) [P].
- **Unenforced conventions drift.** Laravel's freedom statement, Next.js's
  "unopinionated" stance with several organising strategies, and Ash's
  "recommendations only" all leave the choice to each team ([Laravel](https://laravel.com/docs/12.x/structure),
  [Next.js](https://nextjs.org/docs/app/getting-started/project-structure), [Ash](https://ash.hexdocs.pm/project-structure.html)) [D].
  Shopify and Nx both ended up building boundary checkers ([Shopify](https://shopify.engineering/deconstructing-monolith-designing-software-maximizes-developer-productivity),
  [Nx](https://nx.dev/docs/kb/folder-structure)) [P/D].

## 5. Design principles

1. **A declaration's path is a function of its kind and name.** An agent or
   human can work out where a declaration lives without searching. Evidence:
   Zeitwerk naming, agents navigating by glob and grep, and Agentless
   localising through the tree. [D/P]
2. **File names are unique across the project.** Jadpo names are already unique
   project-wide, so file names can be too. This avoids the `page.tsx` problem
   in tabs, diffs and agent file listings. [P]
3. **The kind→name→path mapping is lossless, and the compiler checks it in both
   directions.** Zeitwerk's acronym failures come from a lossy case mapping.
   *Inference:* forbid declaration names that differ only in case, or that
   don't round-trip through the file-name form. macOS file systems are
   case-insensitive by default. [P/O]
4. **Folders organise; they don't carry meaning.** Jadpo already guarantees that
   moving a file never changes meaning or database identity. Keep it that way.
   When folders carry meaning, special syntaxes multiply (Next.js, Remix). [D/O]
5. **Don't force boundary decisions on day one.** Phoenix, Django and Encore all
   point the same way: boundaries named before the domain is understood are
   often wrong. Grouping should be optional, and cheap to add later. [P]
6. **Make growth a mechanical move, not a redesign.** Shopify's
   reorganisation was a script-driven, location-only PR. Jadpo's
   move-safe identity means `jadpo fmt --layout` could make any re-grouping a
   one-command operation. [P/O]
7. **One concept per file, with the entity file as the dossier.** The entity
   file owns its queries, actions, failures and policy. Anything touching two or
   more entities goes in a workflow or query file. This is the
   services/selectors split that experienced Django teams add by hand. [P]
8. **Single-use types live next to their one user; shared types are promoted.**
   This stops `values/` becoming a dumping ground. The rule mirrors Nx's
   "promote when a second domain needs it" and FSD's `shared` layer. The
   compiler can detect single-use types. [D/O]
9. **The compiler checks dependency direction, not just placement.** For
   example: routes may depend on workflows, workflows on entities, entities on
   values, and never the reverse. Precedents: Go cycles, FSD's import rule, Nx
   tags, Spring Modulith `verify()`, Django circular-import pain. [D/P]
10. **Tests and fixtures sit beside their subject.** [O/D]
11. **Compiler-owned output and the migration identity registry live at one
    fixed path, outside source.** Redwood, Laravel and Next.js all isolate
    generated files. The registry is authoritative data and should be
    reviewable. [D/O]
12. **Role folders appear only when used.** Laravel creates `Jobs/`, `Events/`
    and others only on first use. This keeps small apps small. [D]

## 6. Candidate layouts

The same mid-size app is shown in each layout: a team task tracker with
billing. It has 8 entities, mail and payment providers, two jobs and one event.
Files use snake_case of the declaration name (principle 3). Tests are colocated
as `<name>.test.jadpo`, and fixtures as `<name>.fixture.jadpo`.

### A. Role-first, flat, name-derived (extends today's convention)

```text
application.jadpo                 # application + authentication
config.jadpo
entities/
  user.jadpo  user.test.jadpo
  organisation.jadpo  membership.jadpo
  project.jadpo  task.jadpo  task.test.jadpo  comment.jadpo
  subscription.jadpo  invoice.jadpo
values/                           # promoted (multi-use) types only
  email_address.jadpo  money.jadpo  task_summary.jadpo
queries/      project_dashboard.jadpo  overdue_tasks.jadpo
workflows/    invite_member.jadpo  close_project.jadpo  start_subscription.jadpo
routes/       members.jadpo  projects.jadpo  tasks.jadpo  billing.jadpo
jobs/         send_daily_digest.jadpo  retry_failed_invoices.jadpo
services/     postmark.jadpo  stripe.jadpo
events/       task_completed.jadpo
.jadpo/       # compiler-owned: generated/, identity registry (committed)
```

**For it:** an agent never has to make a structural choice; the path follows
from kind and name. It matches the existing compiler checks and the Rails
predictability argument. Depth is never more than 2.
**Against it:** a feature such as billing is spread across 6 folders. That is
the problem Shopify paid to undo at around 6,000 classes, and that Brown and
Bogard criticise. At 150 entities, `entities/` holds 300+ files including
tests. Nothing groups related declarations except names.
**Scale lever:** add optional one-level group folders inside each role, such as
`entities/billing/invoice.jadpo`. That keeps paths derivable but scatters a
domain across roles even more visibly.

### B. Domain-first, fixed role folders inside each domain (contexts)

```text
application.jadpo
config.jadpo
domains/
  accounts/
    entities/   user.jadpo  organisation.jadpo  membership.jadpo
    workflows/  invite_member.jadpo
    routes/     members.jadpo
  projects/
    entities/   project.jadpo  task.jadpo  task.test.jadpo  comment.jadpo
    queries/    project_dashboard.jadpo  overdue_tasks.jadpo
    workflows/  close_project.jadpo
    routes/     projects.jadpo  tasks.jadpo
    jobs/       send_daily_digest.jadpo
    events/     task_completed.jadpo
  billing/
    entities/   subscription.jadpo  invoice.jadpo
    workflows/  start_subscription.jadpo
    jobs/       retry_failed_invoices.jadpo
    services/   stripe.jadpo
    routes/     billing.jadpo
shared/
  values/       email_address.jadpo  money.jadpo
  services/     postmark.jadpo
.jadpo/
```

**For it:** high cohesion, and a feature lives in one subtree. Domains can
become checked visibility boundaries, like Spring Modulith, Nx tags or Shopify
components, with an explicit export list. This scales to 200 entities and to
team ownership.
**Against it:** every new file needs two decisions, a domain and a role. The
domain decision is the one Phoenix and Django users report getting wrong early.
Does `Membership` belong in accounts or projects? Small apps carry ceremony:
a todo app becomes `domains/todos/entities/todo.jadpo`. Depth reaches 4.
Cross-domain workflows need a home of their own, or a rule about which domain
owns them.
**Mitigation:** a single implicit domain until the second one is declared, so
that A is B's degenerate case. Splitting then becomes a mechanical move.

### C. Kind in the file name, optional flat grouping folders

```text
application.jadpo
config.jadpo
src/
  accounts/
    user.entity.jadpo  user.test.jadpo
    organisation.entity.jadpo  membership.entity.jadpo
    invite_member.workflow.jadpo
    members.routes.jadpo
  projects/
    project.entity.jadpo  task.entity.jadpo  task.test.jadpo  comment.entity.jadpo
    project_dashboard.query.jadpo  overdue_tasks.query.jadpo
    close_project.workflow.jadpo
    projects.routes.jadpo  tasks.routes.jadpo
    send_daily_digest.job.jadpo
    task_completed.event.jadpo
  billing/
    subscription.entity.jadpo  invoice.entity.jadpo
    start_subscription.workflow.jadpo
    retry_failed_invoices.job.jadpo
    stripe.service.jadpo  billing.routes.jadpo
  shared/
    email_address.value.jadpo  money.value.jadpo  postmark.service.jadpo
.jadpo/
```

**For it:** the kind is visible in every path and in every tab, so
`glob '**/*.workflow.jadpo'` replaces a role folder. Grouping is one level, has
free names and is optional. A small app is just a flat `src/`, and grouping can
come later (principle 5). Any feature lives in one folder, but there is no
second axis of role folders to choose. The compiler checks that the suffix
matches the declaration kind and the base name matches the declaration name.
**Against it:** longer file names. The suffix restates what the declaration
already says; Angular dropped suffixes for this reason and users pushed back.
Folder names drift unless the compiler limits depth and checks references
across groups. Without role folders, a reviewer can't see "all routes" as one
directory listing, only through a glob.

### Comparison

| Criterion | A role-first | B domain-first | C kind-suffix |
|---|---|---|---|
| Structural choices per new file | 0 | 2 (domain, role) | 0, or 1 if grouped |
| Small app overhead | low | high unless implicit domain | lowest |
| Feature cohesion at 150+ entities | poor | high | high |
| Kind visible from path alone | folder | folder | file name |
| Natural place for visibility boundaries | none | domain | group (if made semantic) |
| Precedent | Rails, Laravel | Phoenix, Ash, Django, Modulith | Angular (pre-v20), Next.js colocation |
| Reported failure mode | scattering (Shopify) | premature boundaries (Phoenix, Django) | naming drift, suffix verbosity |

*Inference that matters for the decision:* the compiler knows every
declaration's kind, name and references, and moves never change identity. So
A, B and C can be converted into one another mechanically. The layout choice is
reversible in a way it isn't for Rails or Phoenix. The irreversible decisions
are the file-name mapping (principle 3) and whether groups become visibility
boundaries.
