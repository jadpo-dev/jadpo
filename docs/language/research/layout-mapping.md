# Layout mapping: three candidates tested against the scenarios

**Status:** analysis and recommendation, 2026-10-11. Decides nothing until
recorded in [area 1](../01-project-layout-and-modules.md#decisions).
**Inputs:** [layout patterns](layout-patterns.md) (evidence, principles P1–P12,
candidates A–C), [layout scenarios](layout-scenarios.md) (S1–S6, questions
Q1–Q25), area 1, the event model §3 and the
entity, query and transaction model.

**Evidence vs judgement.** "patterns §n" and "S1–S6" cite the research.
Golden-app facts were checked again against `examples/golden-todo-migration/`
and `examples/golden-todo/app.jadpo`. Everything else is my judgement; **J**
marks the calls that decide something. The S6 per-folder file counts are
estimates.

## 1. Facts that constrain every candidate

Each of these is verified in the source or the docs.

- **Routes have no names** (`route GET /todos { … }`). **Tests have string names**
  (`test "…"`). So neither can be filed by declaration name: a route file needs a
  key (its path), and a test file needs a subject.
- Names are already unique project-wide. The compiler has the whole reference
  graph, and moving a file never changes database identity.
- The event model needs three things:
  - every effectful declaration resolves to exactly one owner;
  - an owner can span many files;
  - only pure, data, query and event contracts cross between owners.

  Today it maps owners by module namespace "not filesystem paths" (event-model §3).
- The golden app's references decide where things go. `TodoView` is referenced by
  the Todo and User dossiers. `UserStatus`, `ServiceStatus` and `CredentialStatus`
  are each referenced by one entity plus `authentication.jadpo`. `PublicHealth` is
  referenced only by the health route. `AuthenticationStrength` is referenced only
  by the principal.

## 2. Common core (shared by all three candidates)

The three candidates differ only on the scale axis: where groups go, and where
the kind is encoded. Everything else is shared, so the comparison isolates that
axis. The whole core is **J**, backed by the principle cited for each rule.

| # | Rule |
|---|---|
| K1 | **Single-file mode.** If `app.jadpo` is a project's only `.jadpo` file, it may declare any kind. A second `.jadpo` file switches the full layout on, and `jadpo fmt --layout` splits `app.jadpo` deterministically. (P12; keeps the seed one file.) |
| K2 | **Fixed root files.** Each holds only its own kinds; all except `app.jadpo` are optional. `app.jadpo`: application, principal, readiness, surface, group/domain, deployable. `config.jadpo`: config. `authentication.jadpo`: authentication. `policy.jadpo`: application-wide policy declarations. |
| K3 | **One primary declaration per file, named after it** (K8). Primary kinds: entity, query, workflow (top-level action), service, event, subscriber, job, migration, and promoted supporting declarations. |
| K4 | **Supporting declarations follow their users.** These are types, failures, pure functions and fixtures. *Users* are the files that reference the declaration. Test files are not users. Route and job files count only if nothing else refers. For a failure, the users are the files that *raise* it, not the files that list it in `fails`. One user file: the declaration lives there. Two or more: it is promoted to `values/` (types and failures), `functions/` or `fixtures/`, one declaration per file. The rule is computed users-first, so a type used only by a promoted type joins that type's file. It is enforced in both directions, and `jadpo fmt --layout` makes the move. (P8; Nx and FSD promotion, patterns §2.) |
| K5 | **Routes are filed by surface and resource.** A `surface` declares a path prefix. The default is `api` at `/`, implied when no surface is declared. Prefixes other than `/` may not nest. The file stem is `<surface>_<resource>`. The resource is the first static segment after the prefix, lower-cased, with `-` and `.` turned into `_`; a route at the prefix itself uses `root`. Each stem has exactly one file in the project. |
| K6 | **Tests sit beside their subject.** `<stem>.test.jadpo` goes next to `<stem>.jadpo`. It holds `test` and `fixture` declarations plus test-only supporting declarations. (P10.) |
| K7 | **Non-Jadpo assets sit beside the file that references them.** They are named `<stem>.<label>.<ext>`, with ext one of json, html, txt, xml or csv. Every asset must be referenced from `<stem>.jadpo` or `<stem>.test.jadpo`, and every reference must resolve. |
| K8 | **Name-to-file mapping** (§6h). The mapping must round-trip. Mapped names are unique across all kinds, and file basenames are unique project-wide. (P1–P3.) |
| K9 | **Dossier reads.** A raw `query` in an entity file may target only that entity. To read another entity, call its named query or move the read to `queries/`. |
| K10 | **Call direction.** Entry points (route, job, subscriber) may call workflows, queries and entity operations. Entity operations may call other entity operations and functions. Functions may call only functions. Entity operations may never call workflows. (P9.) |
| K11 | **Deployables are declarations, not folders.** `deployable` selects surfaces and entry kinds, and each deployable is compiled from what it reaches. |
| K12 | **Compiler-owned paths.** `build/` is disposable and never scanned. `.jadpo/` is committed and holds `project.json` and `schema.identities.json`. A `.jadpo` file outside an allowed path is an error. Folders with no `.jadpo` files (`docs/`, `deploy/`, `.github/`) are ignored. (P11.) |

## 3. The three candidates

### A. Role-first; groups optional inside the roles

| # | Rule |
|---|---|
| A1 | The root holds the K2 files plus role folders: `entities/ values/ functions/ queries/ workflows/ routes/ events/ subscribers/ services/ jobs/ fixtures/ migrations/`. Each kind has one role folder, and a folder exists only when it is non-empty. |
| A2 | Groups are optional and declared in `app.jadpo` (`group ordering`). Once any group exists, every file sits in `<role>/<group>/`, and the reserved group `shared` holds supporting declarations used across groups. Maximum depth is 3. |
| A3 | Groups only organise files. Ownership is a separate mapping, `domain Ordering { groups: [ordering] }`, in which each group belongs to exactly one domain. With no domains there is one generated owner. `public` works per domain. |

### B. Role folders, nested under domains once domains exist

| # | Rule |
|---|---|
| B1 | With no domains, B is A without groups: one implicit domain and one generated owner. |
| B2 | Domains are declared in `app.jadpo` (`domain Ordering`). Once one exists, the root holds the K2 files, `shared/` and `domains/`, and role folders at the root are forbidden. Each `domains/<snake(name)>/` holds only A1 role folders. Maximum depth is 4. |
| B3 | A domain is the logical owner and the visibility boundary. Declarations are private to their domain. `public` may expose entity identity and `Ref` types, types, failures, events, queries and functions. Actions, workflows, routes, jobs, subscribers and services can never be public, so mutation crosses domains only through events. |
| B4 | `shared/` holds only `values/` and `functions/`: pure declarations with no references into a domain. Promotion across domains: if the users span two or more domains, the declaration goes to the one domain its own references point into (marked public); with no such references it goes to `shared/`; if its references span two or more domains, that is an error. |
| B5 | Root files belong to no domain and may reference anything. Moving a file within a domain changes nothing. Moving it to another domain changes its owner, and the approval subject shows that. |

### C. Kind in the file name; flat groups

| # | Rule |
|---|---|
| C1 | The root holds the K2 files and `src/`. Files are `src/[<group>/]<stem>.<kind>.jadpo`, where kind is one of entity, value, function, query, workflow, routes, event, subscriber, service, job, fixture or migration. No other folders exist. Maximum depth is 3. |
| C2 | Tests are `<stem>.<kind>.test.jadpo`. Assets are `<stem>.<kind>.<label>.<ext>`. |
| C3 | Groups are declared and all-or-nothing, with a reserved `shared` group. A group is an owner and visibility boundary, as in B3–B5. |

**Finding:** A and B are identical until a project declares its first group or
domain. The decision is only *where groups go when a project grows*: inside the
roles (A), above the roles (B), or replacing the roles with suffixes (C).

## 4. Scenario trees

### S1 golden todo

A and B produce the same tree, shown under the recommendation (§7.2): 17 source
files in 7 folders. Under C:

```text
app.jadpo  config.jadpo  authentication.jadpo
src/
  todo.entity.jadpo  todo.entity.test.jadpo  user.entity.jadpo  user.entity.test.jadpo
  service.entity.jadpo  service_credential.entity.jadpo
  todo_view.value.jadpo  user_status.value.jadpo  service_status.value.jadpo  credential_status.value.jadpo
  send_due_reminders.workflow.jadpo  overdue_reminders.job.jadpo
  reminder_mail.service.jadpo  reminder_mail.service.contract.json
  api_todos.routes.jadpo  api_todos.routes.test.jadpo  api_users.routes.jadpo  api_health.routes.jadpo
.jadpo/  build/
```

### S2 e-commerce (one team, so no groups)

A and B:

```text
app.jadpo            # application, principal, surfaces admin + webhooks, readiness
config.jadpo  authentication.jadpo  policy.jadpo        # 4 authentications; StaffRole
entities/     cart.jadpo cart_line.jadpo category.jadpo coupon.jadpo customer.jadpo …
              order.jadpo order.test.jadpo order_line.jadpo payment.jadpo … webhook_receipt.jadpo   # 22 + tests
values/       money.jadpo currency.jadpo sku.jadpo quantity.jadpo address_snapshot.jadpo
              order_status.jadpo payment_status.jadpo out_of_stock.jadpo …        # ~30 promoted; ~80 contracts stay in dossiers
functions/    calculate_totals.jadpo
queries/      product_listing.jadpo … daily_sales.jadpo                            # 10
workflows/    checkout.jadpo checkout.test.jadpo confirm_payment.jadpo … reconcile_payment.jadpo
routes/       api_products.jadpo api_cart.jadpo api_checkout.jadpo api_orders.jadpo api_account.jadpo …
              admin_products.jadpo admin_inventory.jadpo admin_orders.jadpo admin_refunds.jadpo …
              webhooks_payments.jadpo webhooks_payments.test.jadpo webhooks_payments.signed_capture.json
              webhooks_carrier.jadpo
events/       order_event.jadpo payment_event.jadpo refund_event.jadpo inventory_event.jadpo
subscribers/  send_order_confirmation.jadpo release_stock_on_cancel.jadpo …       # 9
services/     payment_provider.jadpo payment_provider.contract.json mailer.jadpo
              mailer.order_confirmation.html tax_calculator.jadpo shipping_carrier.jadpo
jobs/         expire_cart_reservations.jadpo … retry_stuck_refunds.jadpo            # 6
fixtures/     seeded_catalogue.jadpo
```

The largest folder is `entities/`, with about 44 files. Under C, the same app is
about 160 files in one `src/` folder, sorted by name with the kinds interleaved.
Splitting it would force a group decision for a one-team app:

```text
src/  address_snapshot.value.jadpo  admin_orders.routes.jadpo  api_cart.routes.jadpo
      calculate_totals.function.jadpo  cart.entity.jadpo  cart.entity.test.jadpo  cart_line.entity.jadpo
      checkout.workflow.jadpo  …  order.entity.jadpo  order_event.event.jadpo  order_status.value.jadpo  …
```

### S6 at scale: 72 entities, 8 teams, 5 surfaces, 4 deployables

**A**, where each team appears in 12 role folders:

```text
app.jadpo    # 8 groups, 8 domain→group mappings, surfaces v1 v2 partner backoffice webhooks, 4 deployables
config.jadpo  authentication.jadpo  policy.jadpo
entities/    identity/ user.jadpo session.jadpo …   ordering/ cart.jadpo order.jadpo order.test.jadpo order_line.jadpo …   …
values/      shared/ money.jadpo country_code.jadpo address.jadpo locale.jadpo …   ordering/ order_status.jadpo …
functions/   ordering/ calculate_totals.jadpo …
queries/     ordering/ order_history.jadpo …   reporting/ …
workflows/   ordering/ checkout.jadpo …   payments/ post_ledger_entries.jadpo …
routes/      ordering/ v1_orders.jadpo v2_orders.jadpo partner_orders.jadpo backoffice_orders.jadpo …
events/  subscribers/  services/  jobs/  fixtures/  migrations/      # each split into the same 8 groups
```

**B**, where each team appears in one folder:

```text
app.jadpo  config.jadpo  authentication.jadpo  policy.jadpo
shared/
  values/     money.jadpo country_code.jadpo address.jadpo locale.jadpo …   # ~80, pure only
  functions/  round_money.jadpo …
domains/
  identity/   entities/ …  routes/ v1_users.jadpo backoffice_users.jadpo …
  ordering/
    entities/     cart.jadpo cart_line.jadpo order.jadpo order.test.jadpo order_line.jadpo quote.jadpo …
    values/       order_status.jadpo …
    functions/    calculate_totals.jadpo
    queries/      order_history.jadpo …
    workflows/    checkout.jadpo checkout.test.jadpo …
    routes/       v1_orders.jadpo v2_orders.jadpo partner_orders.jadpo backoffice_orders.jadpo v1_carts.jadpo …
    events/       order_event.jadpo
    subscribers/  cancel_order_on_payment_failed.jadpo …
    jobs/         expire_quotes.jadpo …
    migrations/   backfill_order_channel.jadpo
  payments/   workflows/ post_ledger_entries.jadpo  subscribers/ post_ledger_on_order_placed.jadpo
              services/ stripe.jadpo stripe.contract.json adyen.jadpo  routes/ webhooks_stripe.jadpo …
  catalogue/  pricing/  fulfilment/  support/  partners/
```

**C**, where each group is one folder of about 115 files (estimate):

```text
app.jadpo  config.jadpo  authentication.jadpo  policy.jadpo
src/
  shared/    money.value.jadpo  country_code.value.jadpo …
  ordering/  calculate_totals.function.jadpo  cart.entity.jadpo  cart.entity.test.jadpo  checkout.workflow.jadpo
             order.entity.jadpo  order_event.event.jadpo  order_history.query.jadpo  v1_orders.routes.jadpo …
  payments/  stripe.service.jadpo  stripe.service.contract.json  webhooks_stripe.routes.jadpo …
```

**CODEOWNERS.** In A, one glob line per team (`/*/ordering/`) works, so the
scenario's "~56 lines" overstates the cost. But a team's change still touches up
to 12 directories, and ownership needs a second mapping. In B the rule is
`/domains/ordering/`, with `/shared/` and the root files going to architecture
reviewers. C is the same as B, using `/src/ordering/`.

### S3–S5

| | A | B | C |
|---|---|---|---|
| S3 B2B SaaS | Fine flat. Surfaces `v1`, `scim` (prefix `/scim/v2`), `internal`, `webhooks`. Tenancy goes in `policy.jadpo`. The back-office is a `deployable`, not a folder. | Same as A. Domains are optional at 19 entities. | Flat `src/` holds about 140 files. |
| S4 marketplace | 14 jobs become 14 binding files. A `host_listings` route file sits beside `api_listings`. About 25 subscribers. Shared payments library: no answer. | Same as A. | Same rules, but one folder of about 170 files. |
| S5 editorial | `api_articles.jadpo` holds all 22 article routes (about 200 lines, including `/articles/{id}/versions`). The search mapping is `entities/article.search_mapping.json`, and the RSS template sits beside `api_feed.jadpo`. | Same as A. | Same, with suffixes. |

## 5. The 25 ranked questions

✗ means the candidate has no good answer. "core" means the K-rule answer, which
is the same for all three candidates.

| # | Question | A role-first | B domain-first | C kind-suffix |
|---|---|---|---|---|
| 1 | Shared values | core K4; `values/<group>/`, `values/shared/` | core K4; domain `values/`, then `shared/values/` | core K4; `<n>.value.jadpo` in group or `shared/` |
| 2 | Route granularity | core K5: surface + first segment; `/users/{id}/todos` goes in `api_users` | same | same, plus `.routes` |
| 3 | Large dossiers | one file, no companions (§6c) | same | same |
| 4 | Failure homes | in the raiser's file, else `values/` | same | same, as `.value` |
| 5 | Route audiences | `surface` declaration; stem prefix | same | same |
| 6 | Root files | four fixed files; single-file mode | same | same |
| 7 | Service bundle | `services/<n>.jadpo` with its single-use types and failures, plus `<n>.contract.json`; secrets in `config.jadpo` | same, inside the domain | `<n>.service.jadpo` plus contract |
| 8 | Inbound webhook | 4 role folders plus root authentication | one domain folder plus root authentication | one group folder plus root authentication |
| 9 | Entity-local enums | in the dossier (K4) | same | same |
| 10 | Events and subscribers | `events/`, `subscribers/`, one per file | event in the emitting domain, subscriber in the reacting domain | `.event` / `.subscriber` in the group |
| 11 | Jobs | `jobs/<name>.jadpo` binding; the action stays in the dossier | same, per domain | `<name>.job.jadpo` |
| 12 | Tests and fixtures | beside the subject; shared `fixtures/`; data assets beside the test | same | same |
| 13 | Shared pure function | `functions/<group>/` | domain `functions/`, then `shared/functions/` | `.function` in the group |
| 14 | Cross-cutting policy | partial: root `policy.jadpo`, grammar undesigned | same | same |
| 15 | Child entities | own dossier file | same | same |
| 16 | Cross-entity read in a dossier | K9: raw query on its own entity only | same | same |
| 17 | Non-Jadpo assets | beside the referrer (K7) | same | same |
| 18 | Domain grouping at scale | ✗ a team spans 12 folders | `domains/<d>/` | weak: about 115 files per folder |
| 19 | Multiple deployables | `deployable` declarations | same | same |
| 20 | Ownership mechanism | ✗ a second mapping, domain→groups | the domain folder | the group folder |
| 21 | Name collisions | global folded uniqueness | same; the path still shows the domain | same |
| 22 | API versions | surface per version (`v1_orders`) | same | same |
| 23 | Cross-domain workflow | `workflows/<initiating group>/`, with subscribers elsewhere | initiating domain, with subscribers in the reacting domains | same as B |
| 24 | Reusable libraries | ✗ packages undesigned | ✗ (a domain is the nearest shape) | ✗ |
| 25 | Data migrations | `migrations/<group>/` (kind undesigned) | domain `migrations/` | `.migration` |

The core answers Q1–Q17 the same way for every candidate. The candidates differ
only on Q8, Q10, Q18, Q20 and Q23, which are exactly the questions the scenarios
rank as hardest to reverse.

## 6. Specific issues

### (a) Domains as ownership and visibility boundaries, replacing modules

**J: yes.** Remove `module` and `import` (22 diagnostics). Keep `public`, but
make it per domain. The table shows how B meets the event model's needs:

| Event-model need (§3) | How B meets it |
|---|---|
| Every effectful declaration has one owner | Each file is in exactly one domain folder, and a single-domain project has the generated owner. |
| An owner spans many files and calls private actions across them | A domain is a folder of many files, with no import lists. |
| Only data, event, query and pure contracts cross owners | `public` is limited to those kinds; actions are never public. |
| A stable, reviewable component identity | `domain Ordering` is declared in `app.jadpo` and goes into the registry and the approval subject. |
| No implicit owner around a partial mapping | Domains are all-or-nothing (B2). |

This deliberately reverses the event model's "module names, not filesystem
paths". That rule was written to avoid *inferring* owners from unchecked
folders. Under a closed, checked layout, the folder **is** the declared
membership. A separate mapping would recreate the two organising systems that
area 1 already flags. P4 ("folders don't carry meaning") then holds everywhere
except one deliberate place: the domain folder. A move across domains is an
authority change, and it should be visible in the diff.

### (b) One root file or fixed root files

Use fixed root files (K2), plus single-file mode for tiny apps. The evidence
points this way:

- the migrated golden app already uses three root files;
- S3 needs four authentications and 30 config keys, which would make a single
  root file 600+ lines;
- mapping each kind to a file leaves no choices.

### (c) Large entity dossiers

**J: no companion files now.** The entity-query model forbids partial entities.
The evidence for small files is indirect (SWE-agent viewer, lost-in-the-middle;
patterns §3). In the scenarios, Order is around 800 lines. A dossier whose
sections are put in a fixed order by the formatter can be navigated by outline
and grep. Companion files add a choice and new grammar.

The trigger to revisit is measured dossier sizes in the P12 order/payment app.
If they are too large, add exactly one fixed companion, `<entity>.contracts.jadpo`,
holding only that dossier's colocated supporting declarations. That is not a
partial entity, and `fmt` could reverse it.

### (d) Route files, audiences and versions

- **File key:** one file per surface and resource (K5). The URL is what a client,
  a log line or a reviewer has in hand, so the file can be found from the URL.
- **Audiences** (public, admin, partner, webhook, back-office) are `surface`
  declarations. A surface carries a prefix, its default authentications, and
  the deployable that serves it.
- **API versions** are surfaces too (`v1`, `v2`). To remove v1, delete the
  surface; the compiler then flags each `v1_*` file, and after that the
  now-unused `…V1` contracts.
- **Consequence under B:** a URL resource belongs to one domain. So a *mutating*
  route nested under another domain's resource (`POST /v1/customers/{id}/orders`)
  is rejected; use `/v1/orders`. Nested reads call public queries.
- This rule catches the golden app's misfiled `GET /todos/{todo_id}` in the
  health file.

### (e) Shared vs single-use values; enforced promotion

Use K4, enforced both ways, with `fmt` doing the moves (**J**). Enforcing only
promotion would let two equivalent projects differ, against the
one-canonical-form goal. Diffs will show declarations moving, but the moves are
deterministic and made by the tool.

The exclusions are what make the rule workable:

- **Tests** don't count as users.
- **Route and job bindings** count only when nothing else refers. Without this,
  every contract type would be promoted, because the route binding and the
  operation signature both name it.
- **Failures** count only their raisers. Without this, every failure a workflow
  propagates would leave its dossier.

### (f) Multiple deployables

Every deployable is built from one source tree. A `deployable` declaration
selects surfaces and entry kinds; for example, `worker` runs jobs and
subscribers and serves no routes. Each deployable is compiled from what it
reaches, so the worker never compiles route code. A workspace of several
projects is only for genuinely separate applications or future packages.

### (g) Non-Jadpo assets

Assets follow K7. The golden mail contract moves from `tests/assurance/` to
`services/reminder_mail.contract.json`. Email and RSS templates sit beside the
service or route that uses them. A search mapping sits beside its entity, and
signed webhook bodies sit beside their test. Generated OpenAPI stays in `build/`.
An unreferenced asset is an error, so there is no drawer of loose files.

### (h) File-name mapping and round trip

1. A `lower_snake` name maps to itself.
2. An `UpperCamel` name maps by inserting `_` before every uppercase letter
   except the first, then lower-casing: `ServiceCredential` →
   `service_credential`, `OrderViewV1` → `order_view_v1`, `Base64Url` →
   `base64_url`.
3. Round trip: `camel(snake(n)) == n`, where `camel` upper-cases the first letter
   of each segment. This forbids consecutive capitals: `VATRate` fails, so write
   `VatRate`. This is the Zeitwerk acronym trap, removed at the naming rule
   rather than by inflection tables.
4. Folded uniqueness: no two declarations of any kind may have the same mapped
   name. Type `Checkout` and workflow `checkout` collide.
5. Every stem is lower-case and unique project-wide, so case-insensitive file
   systems are safe and every basename is unique. That includes route stems
   (`api_cart` cannot collide with entity `cart`), and test and asset names
   inherit their subject's stem.
6. The check runs in both directions. Each primary declaration's file stem must
   equal its mapped name. Each file must contain the primary declaration its
   stem names, or for a route file, routes whose computed stem matches.

## 7. Recommendation: B

B with an implicit single domain. A and B are the same until a project grows, so
the choice is B's growth lever. The basis:

| Call | Basis |
|---|---|
| Role folders at small and mid size, not C's suffixes | **J**, plus facts: these are the folders the compiler enforces today (DATA-007), so the golden migration needs renames only. S2 fits in about 44 files per folder flat, where C needs about 160 in one folder. Kind is visible in a folder listing (Agentless reads trees; patterns §3). |
| Domains above roles, not groups inside roles (A) | Shopify's reorganisation from role folders to components; Phoenix, Django and Encore on starting with one context (patterns §2). One folder per team gives CODEOWNERS and event ownership without a second mapping. |
| Uniqueness that C gets from suffixes | Taken from K8 instead: folded uniqueness plus surface-prefixed route stems. |

### 7.1 Rules

**Files and placement**

1. Source is every `.jadpo` file under the root, except in `build/` and `.jadpo/`. A `.jadpo` file anywhere these rules don't allow is an error.
2. **Single-file mode:** if `app.jadpo` is the only source file, it may hold every kind.
3. **Root files.** `app.jadpo` is required and holds application, principal, readiness, surface, domain and deployable. `config.jadpo` holds config. `authentication.jadpo` holds authentications. `policy.jadpo` holds application-wide policy. No other `.jadpo` files go at the root.
4. **Role folders** are `entities/ values/ functions/ queries/ workflows/ routes/ events/ subscribers/ services/ jobs/ fixtures/ migrations/`. They have no subfolders and exist only when non-empty.
5. **Each kind has one home:**
   - entity → `entities/`; query → `queries/`; top-level action → `workflows/`;
   - route → `routes/`; event → `events/`; subscriber → `subscribers/`;
   - service → `services/`; job → `jobs/`; migration → `migrations/`;
   - promoted type or failure → `values/`; promoted function → `functions/`; promoted fixture → `fixtures/`.
6. **One primary declaration per file;** the file stem is its mapped name (rule 15).
7. **Supporting declarations** (type, failure, pure function, fixture) follow K4. If one file uses it, it lives there; if more than one does, it is promoted.
8. **An entity file** holds one entity, its operations and its colocated supporting declarations. No companion files.
9. **Raw queries in an entity file** may target only that entity.
10. **Route files** are named `<surface>_<resource>` (K5), and each stem exists once in the project.
11. **A job file** holds one job whose `run:` is a single call.
12. **Tests** go in `<stem>.test.jadpo` beside their subject, and contain only tests, fixtures and test-only supporting declarations.
13. **Assets** are `<stem>.<label>.<ext>` beside the file that references them, and must be referenced (K7).
14. **Names** must round-trip (no consecutive capitals), and mapped names must be unique across all kinds.
15. **Mapping** follows §6h. Basenames are unique project-wide.

**Domains**

16. Domains are declared in `app.jadpo`. With none declared, the project has one implicit domain with one generated owner.
17. Once any domain is declared, the role folders live only under `domains/<snake(name)>/`. The root then holds rule-3 files, `domains/` and `shared/`.
18. A domain is the event-model owner of every effectful declaration in its folder.
19. Declarations are private to their domain. `public` is allowed only on an entity (exposing its identity and `Ref`), a type, failure, event, query or function. There is no `module` and no `import`.
20. Code may call another domain only through its public queries and functions. Mutating another domain's data is possible only by emitting an event.
21. `shared/` holds only `values/` and `functions/`: pure declarations that never reference a domain.
22. Promotion across domains follows B4.
23. Root files belong to no domain and may reference any declaration.

**Dependencies, deployables, output**

24. Call direction follows K10.
25. `deployable` declarations select surfaces and entry kinds.
26. `build/` is disposable. `.jadpo/` holds committed compiler data: `project.json` (moved from `.app/`) and `schema.identities.json` (moved from the root).
27. Folders that contain no `.jadpo` files are ignored.
28. `jadpo fmt --layout` repairs every violation that has exactly one fix. `jadpo layout split <domain> <Entity…>` moves the named entities and their single-use dependants into a domain.

### 7.2 Golden todo under the rules

```text
app.jadpo                  application TodoApplication, principal Principal, readiness database, AuthenticationStrength
config.jadpo               config TodoConfiguration, DatabaseUrl, SessionSigningKey, MailApiKey
authentication.jadpo       browser_session, api_bearer, UserDisabled, ServiceDisabled
entities/
  todo.jadpo               Todo + TodoRole TodoStatus TodoTitle PageSize CreateTodo PatchTodo ListTodos
                           TodoCursor TodoPage + TodoNotFound DueDateInPast EmptyPatch TodoMutationConflict
  todo.test.jadpo
  user.jadpo               User + UserRole DisableUser UserWithTodos + UserNotFound UserMutationConflict
  user.test.jadpo
  service.jadpo
  service_credential.jadpo ServiceCredential + CredentialVerifier
values/
  todo_view.jadpo          used by Todo and User
  user_status.jadpo        used by User and authentication
  service_status.jadpo  credential_status.jadpo
workflows/
  send_due_reminders.jadpo
routes/
  api_todos.jadpo          POST, GET /todos; GET, PATCH, DELETE /todos/{todo_id}
  api_todos.test.jadpo
  api_users.jadpo          GET /users/{user_id}/todos; DELETE /users/{user_id}
  api_health.jadpo         GET /health/live + PublicHealth, HealthStatus
jobs/
  overdue_reminders.jadpo
services/
  reminder_mail.jadpo      ReminderMail + ReminderMessage ReminderReceipt
                           + ReminderRecipientRejected ReminderTemporarilyUnavailable
  reminder_mail.contract.json
.jadpo/  project.json  schema.identities.json
build/
```

This tree differs from today's in five places:

- `values/domain.jadpo` and `values/contracts.jadpo` dissolve into their users.
- The misfiled `GET /todos/{todo_id}` moves out of the health file into `api_todos`.
- The two `/users` routes leave the todos file for `api_users`.
- The dropped job, service and readiness return.
- The purge job is now a lifecycle rule, so it has no file.

### 7.3 What the compiler must check

| Rule ID | Check | Auto-fix |
|---|---|---|
| `layout.unknown_location` | A `.jadpo` file is in a path the rules don't allow. | no |
| `layout.wrong_home` | A declaration kind is outside its home (rule 5). | yes |
| `layout.file_name_mismatch` | A file's stem is not its primary declaration's mapped name, or the file has two primary declarations. | yes |
| `layout.supporting_home` | A supporting declaration is in the wrong file under K4 or B4, in either direction. | yes |
| `layout.value_home_ambiguous` | A promoted declaration references two or more domains. | no |
| `layout.route_file_mismatch` / `layout.route_stem_duplicate` | A route's computed stem differs from its file, or one stem has two files. | yes / no |
| `layout.test_without_subject` / `layout.asset_unreferenced` / `layout.asset_missing` | K6 and K7. | no |
| `naming.round_trip` / `naming.folded_collision` | Rule 14. | no |
| `layout.domain_folder_mismatch` / `layout.mixed_domain_mode` | A domain is declared without a folder or has a folder without a declaration, or root role folders exist alongside domains. | partly |
| `visibility.private_cross_domain` / `visibility.public_kind_invalid` | Rules 19–20. | no |
| `layout.shared_impure` | `shared/` holds an effectful declaration or a domain reference. | no |
| `layout.dossier_foreign_query` / `layout.call_direction` | Rule 9 and K10. | no |

### 7.4 Hard-to-reverse decisions

1. **Domain folder = owner and visibility boundary, with modules removed.** This
   sets authority semantics, the shape of approval subjects and team boundaries.
   Taking it back later means rebuilding a second ownership system.
2. **The name mapping and the no-consecutive-capitals rule.** Names reach
   OpenAPI, the migration registry and event payloads, so tightening this later
   renames public contracts.
3. **Route file key and `surface`.** These add route grammar and set how
   deployables select code.
4. **Global name uniqueness.** It is reversible in one direction only: it can be
   relaxed later at no cost, but restoring it would break code. Keep it strict.

Everything else can be redone with `jadpo fmt --layout`, because moves never
change identity. That covers companion files, promotion enforcement, jobs, tests,
asset naming and single-file mode.

### 7.5 Open questions for Richard

Only questions whose answer changes what gets built are listed.

1. **Should domain folders replace modules as the owner and visibility
   boundary?**
   - The rest of the design depends on this (§6a). It reverses the event model's
     module-namespace mapping and deletes modules and imports.
   - **Recommend yes.** Without it, ownership needs a second hand-maintained
     mapping, and the layout cannot protect one team's entities from another
     team's code.
   - If no: keep modules for ownership, and B's domains become organisation only,
     like A's groups.
2. **How is policy protected from agent edits?**
   - The original golden app kept policy in a human-owned `policy.jadpo` that
     agents could not edit. Under these rules, entity policy lives inside each
     agent-editable dossier, and only application-wide policy has its own root file.
   - **Recommend:** protect policy through the approval subject, which already
     hashes policy in `build/approval/`. Every policy change then needs human
     approval wherever it lives, so file paths are not used for protection.
   - If you want path-based protection instead, each dossier needs a fixed
     `<entity>.policy.jadpo` companion. That reopens the companion rule.
3. **Should deployables be declarations in one project, not a workspace of
   projects?**
   - This replaces project-structure.md's workspace-first direction for multiple
     deployables.
   - **Recommend declarations** (§6f). A workspace would make the back-office and
     the worker share entities through an undesigned package system.
