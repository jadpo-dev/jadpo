---
title: Records and manifest
area: 18
status: decided
checked-with: jadpo 3ce094d0 (2026-10-12)
rules: []
---

# Records and manifest

The compiler produces two kinds of output, and keeping them apart is what
makes changes reviewable and production traceable. **Records** are promises
to the outside world. They are committed, reviewed as diffs, and changed by
one command each: the public API, the database's history, the pinned
packages and people's decisions. **Build outputs** are facts about one
build. They are regenerated every time and never committed. The main one is
the **manifest**, the compiler's graph of the application, and every report
is a view of it. Reviewing a change means comparing two manifests, and
explaining a production event means rebuilding the manifest of the build
that raised it.

> Decided on 2026-10-13.

| Name | Holds | Where | Committed | Written by | Read by |
|---|---|---|---|---|---|
| API record | Each API's routes, fields and failure codes | `api/<api>.api` | Yes | `jadpo api update` | `check`, reviewers, client teams |
| Migrations | Each step of the database, with a snapshot of the whole schema | `database/migrations/` | Yes | `jadpo migrate new` | `migrate apply`, `check` |
| Identity registry | What each renamed thing was born as | `database/identities.json` | Yes | `jadpo rename`, `jadpo domain …` | the compiler, `migrate`, `incident` |
| Lock | Registry packages by version and digest | `jadpo.lock` | Yes | `jadpo provider add` / `update` | `check`, `build` |
| Acceptances | A person's decision to allow listed changes | `acceptances/` | Yes | drafted by `jadpo acceptance new`; the reason by a person | `check`, `approval` |
| Manifest | The application graph: every node with its source place, edges, routes, schedules, hosts, policy, thresholds | `build/manifest.json` | No | `check` | every report, `approval`, `incident`, `inspect`, console, MCP |
| Reports | Views of the manifest: permissions, personal data, network, database, queries, catalogues, OpenAPI, monitoring rules | `build/report/`, `build/catalogue/`, `build/openapi/`, `build/monitoring/` | No | `check` | people, agents, monitoring tools |
| Review | The difference between two manifests, decisions first | `build/review/` | No | `jadpo approval` | reviewers, CI |
| Build stamp | Build id, commit, compiler, deployable, target | Inside each deployable; on every log event | No | `jadpo build` | logs, `incident` |

## What is committed, and why

Commit a file only if it **cannot be recomputed from today's source**, or
if it is **a promise whose diff a person must read** (question 1).
Migrations, renames and the lock record history that today's source no
longer holds. The API record is a promise to clients. Acceptances are
people's decisions. Everything else would go stale, conflict in merges and
invite hand edits, so it is a build output.

- `jadpo check` fails while a record disagrees with the source, naming the
  command that brings it up to date. A record is never edited by hand.
- Area 1's fixed root gains `api/` and `acceptances/`. `jadpo.lock` and
  `providers/` are already there (area 11). `providers/` is source, not a
  record: packages and their recorded responses, written like any code.

::: today

There are no records. `build/compatibility/public-failure-codes.json` is
regenerated every build and reports `baseline_not_configured`. The registry
is `schema.identities.json`, and only `jadpo schema` commands write it.

:::

## The manifest

`jadpo check` writes one manifest for the project, `build/manifest.json`
(question 2). It is the compiler's graph as versioned JSON: every
declaration and field with its stable identity, source file and lines,
content digest, `///` description, calls, reads, writes, events and
failures. It also holds the routes, schedules, subscribers, connections and
their hosts, policy grants, personal fields, thresholds, the control flow
that path numbers decode against, and each deployable's build id.

```json
{
  "format": 1,
  "source": { "commit": "8d113e6", "modified": false },
  "compiler": "0.4.0",
  "deployables": { "api": { "target": "wasm", "build": "b_9a1f03c6d2e4" } },
  "nodes": [
    {
      "id": "action complete_todo",
      "kind": "action",
      "domain": "planning",
      "file": "domains/planning/actions/complete_todo.jadpo",
      "lines": [4, 22],
      "digest": "9f2c41d0",
      "calls": ["Todo.get", "Todo.update"],
      "fails": ["TodoNotFound", "TodoMutationConflict"]
    }
  ]
}
```

- **Every report is a view of it.** The permissions, personal-data,
  network, database and query reports, the catalogues, OpenAPI and the
  monitoring rules are rendered from the manifest, so they cannot disagree
  with each other.
- **A build id is a digest** of everything that decides a deployable's
  behaviour: the source it reaches, the compiler version, the target and
  the locked packages. The same build id means the same build.
- **Format.** The JSON is canonical: sorted, with relative paths and no
  timestamps. `format` changes only when a field is removed or changes
  meaning; additions keep the number. Manifests are never stored, so no
  tool ever reads an old format. `jadpo help --json` documents the schema.
- **Size does not matter**, because it is never shipped. Today's map of
  the golden todo is 101 KB (13 KB compressed). Agents ask for one node
  with `jadpo inspect <name> --json` rather than reading the file.

> **Built (chunk 1.7).** For a project, `jadpo check` writes
> `build/manifest.json`, errors or not, at `format` 1: `compiler`, `errors`
> and `nodes`. A node has `id`, `kind`, `name`, `file`, `lines` and `digest`,
> and where they apply `domain`, `parent`, `public`, `doc`, a type's `base`,
> `rules` and `failure`, a field's `type`, a callable's `signature`, and its
> edges as sorted lists by kind: `calls`, `uses`, and `raises` for the
> failures that can leave a callable (the `fails` above). Nodes are built for
> areas 1 to 4; the build stamp (`source`, `deployables`, build ids),
> routes, schedules, hosts, policy, personal fields and thresholds join as
> their areas are built. `jadpo help --json` prints the schema with every
> node and edge kind and whether it is built yet.

::: today

`build/app.meta.json` holds a graph whose nodes are numbered in order, so a
node's number changes when anything before it changes. Operations are named
like `route:GET:/health/live`. The source is identified only by a digest
(`src_2c78570a2841d2f7`), with no commit. The approval subject is a second,
separate file of 6.1 MB, and `inventory/` and `audit/` are a third set of
views. All of these fold into the manifest and its reports.

:::

## Stable identity

A declaration or field is known by **the name it was born with**
(question 3). The registry holds one line for each thing that has since
been renamed, mapping its name now to its born name. Only `jadpo rename`
and the `jadpo domain` commands write it. Children follow their parent:
renaming a domain or an entity is one line.

```json
{
  "entity Task": "Todo",
  "field Task.importance": "Todo.priority",
  "subscriber notify_owner": "send_reminder"
}
```

- **One registry for data and code.** Stored rows name code: outbox
  events, delivery records and schedule state (area 5's system tables)
  refer to events, subscribers and schedules. Renaming a subscriber must
  not orphan its pending deliveries, any more than renaming a field may
  lose its column.
- **Physical names never change.** The table stays `todos` and the column
  `priority`. Migration snapshots hold the whole schema with physical names,
  so a rename made by hand shows as a removal and an addition, and
  `jadpo migrate new` refuses it with a pointer to `jadpo rename` (area 14).
- **Inside a callable**, branches and fault sites are numbered per build.
  Only declarations and fields need to be recognised across builds.
- `jadpo incident`, path keys (area 16) and the review all compare through
  this registry, so a rename reads as "renamed", never as "removed and
  added".

> **Built (chunk 1.7).** A key is the kind and the born name, in the
> registry's own form: `action complete_todo`, `field Todo.title`,
> `function rounding.half_even`. It is not qualified by domain, as the
> manifest example above is: names are unique across the project, and
> promotion moves a declaration to `shared/` without a registry line, which
> a domain in the key would turn into a removal and an addition. Question 8
> asks Richard to confirm it. The registry is read once `jadpo rename`
> writes it; until then every born name is the name now.

## Comparing two builds

Every review is a diff of two manifests (question 4). The **after** side is
the working tree. The **before** side is a git ref, by default the commit
the branch started from (on the default branch, the previous commit). The
compiler reads that commit from git, builds it with the same compiler, and
caches its manifest in `build/before/<commit>/`, so this costs one build
per base commit.

- **`jadpo check`** uses the diff to fail on any change that needs a
  person and is not accepted (area 14, question 12). CI needs the base
  commit fetched: a full clone, not a depth-1 checkout.
- **`jadpo approval`** explains the same diff, decisions first (area 17's
  order), as Markdown and JSON in `build/review/`. `--before <ref>` picks
  another base. For production, pass the commit that the deployed build's
  events carry.
- **Records come for free.** The API record's diff, the migrations added,
  the lock changes and the acceptances added are part of the before and
  after trees, so the review lists them without a second mechanism.
- If the base does not compile with this compiler, for example after a
  language upgrade, `check` compares the records only and says so. Upgrade
  the compiler in its own change.

::: today

`jadpo approval <project> --before <folder>` compares two checked-out
project folders: `--before main` fails with `toolchain.project_not_found`.
The result is the 6.1 MB subject in `build/approval/`, plus a text copy
that is one long line of JSON.

:::

## Acceptances

One file per **decision**, listing every change it allows, with the reason
in `///` (question 5). Each entry is `subject: Kind.change`, with the
subject and the change copied from the compiler's message:

```jadpo fragment
// acceptances/20261020T1400_remove_todo_priority.jadpo
/// Priority was never shown by any client; the mobile app stopped sending it in 3.2.
acceptance {
    changes: {
        main.TodoView.priority: ApiChange.output_field_removed
        main.TodoSummary.priority: ApiChange.output_field_removed
    }
}
```

- **It counts only in the change that adds it.** Acceptances already on
  the before side are history, and editing one is an error. `check` fails
  when a change needs a person and no added acceptance lists it, when a
  listed entry matches nothing, or when the reason is missing.
- **Drafting is a command.** `jadpo acceptance new <purpose>` writes a file
  listing every change still unaccepted, with no reason. An agent may run
  it, then stops: the missing reason keeps `check` failing until a person
  writes one.
- **The gate is the git host.** `acceptances/` and `api/` are owned in
  CODEOWNERS, so the owner's approval of the pull request is what lets a
  decision in, and the host records who gave it (area 14, question 12).

| Needs a person | Kind | Area |
|---|---|---|
| A route removed, a required input field added, a rule tightened, an output field removed or retyped, a failure code changed | `ApiChange` | 7 |
| A column or table dropped, an entity moved to another store | `DataChange` | 5 |
| Any widening of who can do what, however it arises | `PolicyChange` | 9 |
| A domain created or renamed | `DomainChange` | 1 |
| A risky package change (next section) | `PackageChange` | 11 |
| A threshold raised or removed | `ThresholdChange` | 16 (proposed) |

Additive changes need no acceptance: a new route, a new optional input
field, a new output field, tightened policy. The diff still shows them.

## Package upgrades

An upgrade is a change to `jadpo.lock` or `providers/`, reviewed like any
other (question 6). The review compares the provider operations the app
calls, matched by package, resource and operation with the version
ignored, so a call moved from one version's connection to the next is
compared, not listed as removed and added.

- **Needs a person:** a used operation removed; a used field removed or
  retyped; a new failure on a used operation; a host or credential scheme
  changed.
- **Does not:** new operations or optional fields, documentation, and
  changes to operations the app never calls.
- `jadpo provider update <name>` prints this section of the review. Area
  11's "upgrade plan" idea is this same output, not a separate format.

## Production builds

A deployable carries a small **build stamp**, not its manifest
(question 7). The stamp holds the build id, the commit, whether the tree
was modified, the compiler version, the deployable and the target. Every
log event carries it. Each fault site carries its `.jadpo` file and line as
a constant, so events already show `.jadpo` frames (area 17).

- **The manifest is rebuilt, not shipped.** Builds are deterministic (area
  14), so `jadpo incident` builds the event's commit, checks that the build
  id matches, and caches the result. Nothing is uploaded or archived.
- **No endpoint.** The deployed app reveals nothing about its structure.
  This also works for a worker with no HTTP surface.
- **Native and Wasm alike.** Nothing depends on debug information or
  unwinding: Rust Workers abort on panic with no stack, so the path comes
  from the runtime's own spans. On Workers the stamp is logged beside
  `CF_VERSION_METADATA`, so the platform's version and the Jadpo build can
  be joined.
- **Deploy only commits that stay in history** (the default branch or a
  tag). A build from a modified tree is marked as such, and can only be
  explained from that exact tree.

## What an agent runs

Every command takes `--json`, and `jadpo help --json` describes them all
(area 14).

| Job | Command | Then |
|---|---|---|
| After any edit | `jadpo check --json` | Each error says who acts (`agent` or `human`) and the next command |
| A record is out of date | `jadpo api update` or `jadpo migrate new <purpose>` | Commit the record with the code |
| Rename anything | `jadpo rename <old> <new>` | The registry gains one line |
| What did my change do? | `jadpo approval --json` | Decisions first |
| A change needs a person | `jadpo acceptance new <purpose>` | Stop and hand over |
| Look something up | `jadpo inspect <name> --json` | One node from the manifest |
| Upgrade a package | `jadpo provider update <name>` | Fix compile errors, then as above |
| Explain a production event | `jadpo incident <event>` | `jadpo test new --from-event` |

## Three journeys

**An agent removes an API field, and a person accepts it.**

```text
$ jadpo check
error  api/main.api is out of date: priority removed from TodoView, TodoSummary
       next  jadpo api update                                     agent
error  removing an output field can break clients: main.TodoView.priority,
       main.TodoSummary.priority
       next  a person accepts it: jadpo acceptance new <purpose>  human
$ jadpo api update
$ jadpo acceptance new remove_todo_priority
wrote acceptances/20261020T1400_remove_todo_priority.jadpo (2 changes, reason needed)
```

The agent commits and opens the pull request, saying a reason is needed.
CI runs `jadpo check`, which fails on the missing reason, and publishes the
review. The code owner reads it, writes the `///` line, and approves. CI
passes and the change merges. On the default branch the acceptance is now
history.

**A provider package upgrade.**

```text
$ jadpo provider update couriers
couriers  AcmeCouriers.v1 → v2   jadpo.lock updated
  bookings.create   new failure  address_unverifiable           needs a person
  bookings.get      field removed  Booking.driver_phone         needs a person
  host              api.acme.example → api.eu.acme.example      needs a person
```

The removed field is also a compile error in `delivery_view`, which the
agent fixes first. The agent then runs `jadpo acceptance new
upgrade_couriers_v2` and hands over. A person writes one reason covering
all three changes, and the owner approves.

**A production incident traced to source.** A Worker logs a fault carrying
its stamp (`build b_9a1f…`, `commit 8d113e6`), its `.jadpo` frames and a
path number. Someone pastes it into `jadpo incident`:

```text
$ jadpo incident
int_overflow in action price_line
  build  b_9a1f03c6d2e4  rebuilt from 8d113e6, matches
  then   domains/ordering/actions/price_line.jadpo:4
  now    domains/ordering/actions/line_price.jadpo:6   renamed, changed since (2 commits)
  path   POST /orders → place_order → price_line, branch "discounted"
  next   jadpo test new --from-event
```

"Now" comes from the registry and the current manifest. If the local
compiler is a different version, the build id cannot match: the frames and
names are still exact, because they are in the event, and the output says
that the path number cannot be decoded.

## Review

### Status

Each row says how it was checked: run with `jadpo` 3ce094d0 on a copy of
`examples/golden-todo-migration`, or read from source.

| Feature | Status |
|---|---|
| Manifest | **Partial (run):** `jadpo artifacts` writes `build/app.meta.json` (101 KB, 13 KB compressed): 734 graph nodes keyed by a counter, operation ids such as `route:GET:/health/live`, and a source digest. No commit, no compiler version, no deployables. |
| Views of it | **Separate (run):** `inventory/`, `audit/`, `openapi/`, `compatibility/`, `validators/` and `approval/` are written independently of `app.meta.json`; 16 files in all. |
| Approval | **Gap (run):** `--before` takes a second project folder, not a git ref (`--before main` fails with `toolchain.project_not_found`). `subject.json` and its text copy are 6.1 MB each. Nothing blocks a release. |
| Build stamp | **Partial (run):** the generated `app.ts` embeds the source digest `src_2c78570a2841d2f7`; no commit. |
| `jadpo incident` | **Exists (read, `cli/src/main.rs`):** refuses with `cli.incident_revision_mismatch` unless the local source is exactly the event's. |
| Identity registry | **Partial (read, `migration_identity.rs`):** `schema.identities.json` gives every table and column an id and physical name; no code; the golden todo has none. |
| API record, acceptances, lock | **Not built (read):** no `api/`, no `acceptance` declaration, no `jadpo.lock`; the compatibility file reports `baseline_not_configured` (run). |

### Consistency with the rest of the language

- **Area 1, question 10** (root `api/` and `acceptances/`) is answered by
  question 1. `jadpo.lock` and `providers/` are already in the root.
- **Area 5's registry is amended** (question 3). "Gives each table and
  column a stable identity" becomes born names plus a list of renames,
  covering code too. Area 1's layout comment on `identities.json` changes
  with it. Migration snapshots, `migrate new` and drift checks are
  unchanged.
- **Area 7 is amended** (question 5). "One per change" becomes one per
  decision. "The record notes which acceptance allowed each change" and
  "an acceptance that no record uses is an error" become: an acceptance
  counts only in the change that adds it, and one that matches nothing
  there is an error. The API record holds no acceptance names.
- **Area 9:** "the whole application before and after" is now defined: the
  before side is the base commit's build (question 4).
- **Area 11:** a changed host was decided as "appears in the approval
  report". Here a host or credential-scheme change on a used operation also
  needs an acceptance (question 6). New connections still only appear.
- **Area 14:** the `build/` listing changes. `app.meta.json`, `inventory/`,
  `audit/`, `compatibility/` and `approval/` fold into `manifest.json`,
  `report/` and `review/`. `jadpo inspect` prints the manifest, or one node.
  `jadpo acceptance new` is a new command. `check` reads git for its base.
  The decided "manifest found by the event's build id" is met by rebuilding
  it (question 7).
- **Area 16:** the build manifest carries the thresholds, as proposed.
  Per-request thresholds are compiled into the deployable.
- **Area 17:** question 2's identity extension is question 3 here, and
  "`jadpo build` records the commit in `app.meta.json`" becomes the build
  stamp. Its review report lives in `build/review/`.
- **Compiler design** (`docs/compiler-runtime.md` §1.1 and §9): the
  manifest is the graph's export. Not shipping maps publicly matches §9.
- **Archived plan:** RM-219 separated a thing's lasting identity from the
  build it ran in. The build id and born names do that. RM-603's signed
  approvals stay replaced by the git host's review (area 14).

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Pending deliveries and schedule state when a subscriber or schedule is removed, not renamed | 12 | Question 3 |
| Diagnostics for unaccepted changes, unmatched entries, missing reasons and edited acceptances; `jadpo acceptance new`; `inspect <name>`; `check` reading git | 14 | Questions 4 and 5 |
| The graph's shape: stable identities, per-build sites, the manifest as its export | Compiler design §1.1 | Question 2 |
| Loosened thresholds listed as `ThresholdChange` | 16 | Question 5 |
| Build stamp and `.jadpo` site constants in events; `jadpo incident` rebuilds the manifest | 17 | Question 7 |

### Open questions

1. **Which files are records, and is everything else uncommitted?**
   *Recommendation:* four records: `api/`, `database/` (migrations and the
   registry), `jadpo.lock` and `acceptances/`. A file is committed only if
   it cannot be recomputed from today's source or is a promise whose diff
   must be read. Area 1's root gains `api/` and `acceptances/`. This is the
   Kotlin, API Extractor and lock-file pattern. *Decided 2026-10-13:* yes, as recommended.
2. **What is the manifest?** *Recommendation:* one `build/manifest.json`,
   the compiler's graph as canonical JSON with a `format` number. Every
   report, catalogue, OpenAPI document and monitoring rule is a view of
   it, and `app.meta.json`, `inventory/`, `audit/`, `compatibility/` and
   `approval/` go. Build ids are digests of what decides behaviour. *Decided 2026-10-13:* yes, as recommended.
3. **How does identity survive renames, and is there one registry?**
   *Recommendation:* one registry, `database/identities.json`, for data and
   code. Each thing is known by its born name, and the registry holds only
   renames, written by `jadpo rename` and the domain commands. Code is in
   it because stored deliveries and schedule state name code. This amends
   area 5's dense registry. A dense registry would add a line for every
   new action. *Decided 2026-10-13:* yes, as recommended.
4. **What is the "before" in every comparison?** *Recommendation:* a git
   ref, by default where the branch started. That commit is built with
   the same compiler and cached per commit. `check` gates on the diff,
   `approval` explains it, and production passes the commit its events
   carry. This is buf's `--against`, applied to the whole application.
   *Decided 2026-10-13 (Richard):* by default, the current branch against
   the repository's default branch (`main` or `master`, read from the git
   remote), measured from where the branch left it (the merge base, as a
   pull request's diff is), so changes merged into `main` since do not show
   as reversed; `--before <branch or commit>` compares with anything else.
   The same default serves `check`, `jadpo approval` and the review web view
   (area 17, roadmap E06).
5. **How are acceptances written, matched and used up?** *Recommendation:*
   one file per decision, listing `subject: Kind.change` entries, with a
   `///` reason from a person. It counts only in the change that adds it,
   and is never edited once merged. `jadpo acceptance new <purpose>`
   drafts it without a reason, and the CODEOWNERS approval is the gate.
   This amends area 7's "one per change" and its notes in the record. *Decided 2026-10-13:* yes, as recommended.
6. **Which package changes need a person?** *Recommendation:* a used
   operation removed, a used field removed or retyped, a new failure on a
   used operation, or a host or credential scheme changed. Everything else
   only appears in the review. `provider update` prints that part of the
   review, which is what the "upgrade plan" idea becomes. *Decided 2026-10-13:* yes, as recommended.
7. **Does a deployable carry its manifest?** *Recommendation:* no, only a
   build stamp (build id, commit, modified, compiler, deployable, target)
   on every event, plus `.jadpo` locations at fault sites. `jadpo incident`
   rebuilds the manifest from the commit and checks the build id. There is
   no endpoint to secure, it works for workers and Wasm alike, and nothing
   is archived. Deploy only commits that stay in history. This replaces
   the starting idea of embedding the manifest and serving it. *Decided 2026-10-13:* yes, with one addition: `jadpo` fetches and runs the exact compiler version named in the stamp (as `rustup` installs a pinned toolchain), so an old build can always be rebuilt.

8. **Is a stable identity qualified by its domain?** Raised while building
   chunk 1.7: the manifest example writes `planning.complete_todo`, the
   registry example `entity Task` but `subscriber planning.notify_owner`.
   *Recommendation:* no, as built: a key is the kind and the born name,
   `action complete_todo`, and the domain is a field of the node. Names are
   unique across the project and across kinds (area 1), so the domain adds
   nothing to a key, and promotion to `shared/` moves a declaration to
   another owner without a registry line, so a domain in the key would show
   a routine move as a removal and an addition, losing incidents, coverage
   and stored deliveries that name it. Changing this later is a new
   manifest `format`.
   *Decided 2026-10-13 (Richard):* as recommended; the examples above now
   use the unqualified form.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Compiler data | Carried from area 1: a visible `database/` folder for the identity registry, migrations and seeds | 2026-10-12 |
| New domains | Carried from area 1: creating or renaming a domain appears in the approval review | 2026-10-12 |
| Policy protection | Carried from area 1: through the approval check wherever policy lives, not by file path | 2026-10-12 |
| Stable identity | A key is the kind and the born name (`action complete_todo`), never qualified by domain; the domain is a field of the node, so promotion to `shared/` keeps the key | 2026-10-13 |
| Migration snapshots | Carried from area 5: each migration folder holds a generated `snapshot.json`, generated SQL and `changes.jadpo` only for decisions; `check` fails when entities and the latest snapshot disagree | 2026-10-12 |
| Removing data | Carried from area 5: retire, then drop in a later migration; a drop needs explicit acceptance | 2026-10-12 |
| System tables | Carried from area 5: runtime tables prefixed `jadpo_`, part of the migration snapshots | 2026-10-13 |
| API record and acceptance | Carried from area 7: a committed record per API compared by `check`; breaking changes need a person-written acceptance; `acceptances/` owned in CODEOWNERS | 2026-10-12 |
| Permissions report | Carried from area 9: `build/report/permissions.md` and `.json`, shown in the approval review | 2026-10-12 |
| Package pin | Carried from area 11: registry packages by digest in `jadpo.lock`, local ones committed; never fetched during a build | 2026-10-13 |
| Hosts | Carried from area 11: outbound access only to declared connections' hosts; a generated network report; changes in the approval report | 2026-10-13 |
| `check` writes derived files | Carried from area 14: `check` writes `build/` except the runnable app | 2026-10-13 |
| Renaming | Carried from area 14: `jadpo rename <old> <new>` updates source, callers, file names and the identity registry | 2026-10-13 |
| Sign-off | Carried from area 14: acceptance files in a host-protected folder; CI's `jadpo check` fails without them | 2026-10-13 |
| Incident input | Carried from area 14: the build's manifest is found by the event's build id | 2026-10-13 |
| Records | Four committed records: `api/`, `database/`, `jadpo.lock`, `acceptances/`; area 1's root gains `api/` and `acceptances/` | 2026-10-13 |
| Manifest | One `build/manifest.json`, the compiler's graph as versioned canonical JSON; every report and generated document is a view of it | 2026-10-13 |
| Identity | One registry for data and code in `database/identities.json`, holding renames only, written by `jadpo rename` and the domain commands | 2026-10-13 |
| The before side | Current branch against the default branch (`main`/`master` from the remote) at their merge base; `--before <ref>` overrides; one default for `check`, `approval` and the review web view (Richard) | 2026-10-13 |
| Acceptances | One file per decision listing the changes it covers, with a person's reason; counts only in the change that adds it; `jadpo acceptance new` drafts it; CODEOWNERS approval is the gate | 2026-10-13 |
| Package changes | A person accepts a used operation removed, a used field removed or retyped, a new failure on a used operation, or a host or credential-scheme change | 2026-10-13 |
| Production builds | Deployables carry a build stamp, not the manifest; `jadpo incident` rebuilds from the commit with the exact compiler version | 2026-10-13 |
