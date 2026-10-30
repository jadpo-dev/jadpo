---
title: Records and manifest
area: 18
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Records and manifest
<!-- topic: records-and-manifest -->

Besides your running app, the compiler writes two kinds of file, and it
keeps them apart so that every change can be reviewed and every
production event traced back to its source.

- A **record** is a promise to the outside world: your public API, the
  history of your database, the packages you pinned, and decisions people
  made. You commit records, review them as diffs, and each is changed by
  one command.
- A **build output** is a fact about one build. It is written again on
  every build and never committed. The main one is the **manifest**, the
  compiler's graph of your application. Every report is a view of it.

Reviewing a change means comparing two manifests. Explaining a
production event means rebuilding the manifest of the build that raised
it.

| Name | Holds | Where | Committed | Written by | Read by |
|---|---|---|---|---|---|
| API record | Each API's routes, fields and failure codes | `api/<api>.jadpo_record` | Yes | `jadpo api update` | `check`, reviewers, client teams |
| Event record | Each domain's events, the fields they carry and the types those name ([The event record](12-jobs-and-events.md#the-event-record)) | `domains/<domain>/events/events.jadpo_record` | Yes | `jadpo records update` | `check`, reviewers |
| Migrations | Each step of the database, with a snapshot of the whole schema | `database/migrations/` | Yes | `jadpo migrate new` | `migrate apply`, `check` |
| Identity registry | What each renamed thing was born as | `database/identities.jadpo_record` | Yes | `jadpo rename`, `jadpo domain …`, `jadpo fmt --layout` | the compiler, `migrate`, `incident` |
| Translation sources | For each locale and message, a hash of the default wording it was translated from, and each acceptance ([Translations as messages](10-configuration-and-time.md#translations-as-messages)) | `translations/sources.jadpo_record` in each translations folder | Yes | `jadpo translations update`, `import`, `accept` | `check`, `translations missing`, `approval` |
| Package surface | An installed package's public surface, each entry pointing at its file ([Integrations](11-integrations.md)) | `packages/<name>/<version>/surface.jadpo_record` | Yes, with the package | `jadpo package check`; verified by `jadpo package add` and `update` | `check`, `package show`, reviewers, the registry |
| Lock | Each installed package's version, digest and source: `registry`, `git` (repository, tag and commit), `path` or `link`; and the Jadpo release the project runs with, when it is pinned | `jadpo.lock` | Yes | `jadpo package add`, `update`, `remove`, `link`, `unlink`; `jadpo toolchain pin`, `unpin` | `check`, `build`, and every command, to choose the Jadpo that runs it |
| Suggestions kept quiet and taught | The suggestions a team ignored, and the patterns it taught `jadpo suggest` ([Suggestions](14-tooling.md#suggestions)) | `ignored_suggestions.jadpo_record`, `taught_suggestions.jadpo_record` | Yes | `jadpo suggest ignore`, `jadpo suggest found` | `suggest` |
| Acceptances | A person's decision to allow listed changes | `acceptances/` | Yes | drafted by `jadpo acceptance new`; the reason by a person | `check`, `approval` |
| Manifest | The application graph: every node with its source place, edges, routes, schedules, hosts, policy, thresholds | `build/manifest.json` | No | `check` | every report, `approval`, `incident`, `inspect`, console, MCP |
| Reports | Views of the manifest: permissions, personal data, network, database, queries, catalogues, OpenAPI, monitoring rules | `build/report/`, `build/catalogue/`, `build/openapi/`, `build/monitoring/` | No | `check` | people, agents, monitoring tools |
| Review | The difference between two manifests, decisions first | `build/review/` | No | `jadpo approval` | reviewers, CI |
| Build stamp | Build id, commit, compiler, deployable, target | Inside each deployable; on every log event | No | `jadpo build` | logs, `incident` |

## What is committed, and why
<!-- topic: what-is-committed-and-why -->

You commit a file only if it **can't be worked out again from today's
source**, or if it is **a promise whose diff a person must read**.
Migrations, renames and the lock hold history that today's source no
longer has. The API record is a promise to your clients. Acceptances are
people's decisions. Anything else would go stale, conflict in merges and
tempt people to edit it by hand, so it is a build output instead.

### Record files

Every record but the lock is a `.jadpo_record` file, kept next to what
it describes:

- `api/main.jadpo_record`, the API record;
- `database/identities.jadpo_record`, the identity registry;
- each `translations/sources.jadpo_record`;
- each installed package's `surface.jadpo_record`;
- the root's `ignored_suggestions.jadpo_record`, the suggestions a team
  keeps quiet ([Suggestions](14-tooling.md#suggestions)), and
  `taught_suggestions.jadpo_record`, the patterns it taught
  `jadpo suggest`.

Each is in Jadpo's readable format, not JSON. Its first line says what
generated it and which command updates it, as the API record's does:

```text
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1
```

Every record but the migrations is written in the
[record format](#the-record-format): the header line, then
`record <kind> <format>`, then its entries. `jadpo.lock` keeps its name.
Files people write, such as acceptances, are not records. A migration's
folder keeps its own files ([Migrations](05-data.md#migrations)).

### How `jadpo check` keeps records up to date

You never edit a record by hand. `jadpo check` works out each record
again and fails while one disagrees with the source, naming the command
that brings it up to date. A hand edit is an error naming that command.
There are three such errors:

- `record.out_of_date`: the record disagrees with the source.
- `record.edited`: Jadpo didn't write the record that way.
- `record.format_newer`: a newer Jadpo wrote it.

The API record is `api/<api>.jadpo_record`, a view rebuilt from the
routes, with no `version:` line. The OpenAPI document's version is the
first 12 hex digits of the SHA-256 of the record the routes give today,
which is the committed record whenever `jadpo check` passes. The
identity registry, the translation sources, the suggestion records and
`jadpo.lock` are ledgers of one-line entries
([Checking a record](#checking-a-record) explains views and ledgers).

### `jadpo records`

`jadpo records` lists every record, what writes it and whether it is up
to date, and names the command for any record that isn't. It groups
them by the command that writes them:

```text
$ jadpo records
jadpo api update
✗ api/main.jadpo_record             out of date

jadpo rename, jadpo domain or jadpo fmt --layout
✓ database/identities.jadpo_record  up to date

✗ records out of date  1 out of date · 2 records

→ jadpo api update  brings api/main.jadpo_record up to date
```

`jadpo records update` brings every record up to date at once. It is
also how a project moves a record from an older name (`api/<api>.api`,
`database/identities.json`) to its name now
([Checking a record](#checking-a-record)).

### Migrations

Migrations are records too. `jadpo migrate new` writes each folder in
`database/migrations/`: `snapshot.json`, `postgres.sql` and, when the
migration needs a person's decision, `changes.jadpo`. `jadpo check`
fails, naming the command to run, while the entities and the latest
snapshot disagree, while the latest migration waits for a decision, and
when a record can't be read. Each folder is a `migration_snapshot` node
in the manifest.

### Folders at the root

[The root](01-project-layout-and-modules.md#the-root) of a project holds
`api/`, `acceptances/`, `packages/` and `jadpo.lock` beside the rest.
`packages/` holds the installed packages. They are committed, so every
upgrade is an ordinary diff. Each is checked against its digest in the
lock and is read-only, but only its `surface.jadpo_record` is a record.
Your app's own integrations, in `shared/providers/`, are source, written
like any other code ([Integrations](11-integrations.md)).

### Checking a record

A record is one of two shapes:

- A **view**, such as the API record or a package surface, is rebuilt
  from the source and compared with the committed file byte for byte.
- A **ledger**, such as the identity registry, the translation sources,
  the suggestion records or the lock, is a list of one-line entries that
  hold history.

**A hand edit of a view is told by its shape.** Without the base
commit's build, a view that differs from what its writer gives today is
`record.edited` when Jadpo couldn't have written it, and
`record.out_of_date` otherwise. Jadpo couldn't have written a comment, a
fact outside the kind's closed list, a tab, a trailing space, a layout
Jadpo doesn't write, or a header other than the one it writes. Both
errors name the same command, which writes the record again, so a
careful edit of one fact reads as out of date.

**A ledger is checked against its own rewrite.** Each line must read,
each key must appear once, and the file must equal what Jadpo writes
from its lines: sorted, with the header in the newest format.

- Lines that git's `union` merge left out of order are `record.edited`,
  and `jadpo records update` puts them right.
- Two lines for one key, such as one thing renamed two ways or one
  package locked twice, are `record.edited` naming both lines, and are
  left for a person to settle.

Checking that each line added since the base is one its command could
write needs the base commit's build too.

**`jadpo records update` brings every record up to date at once.** It
writes each view from the source, rewrites each ledger, and moves a
record under its old name. It is the command an error's `next:` line
names for a ledger, because a ledger's own commands (`jadpo rename`,
`jadpo translations update`, `jadpo suggest ignore`) need arguments.

**Old names are moved, not read forever.** `api/<api>.api` and
`database/identities.json` are `record.out_of_date` ("the record's old
name"). `jadpo api update` and `jadpo records update` write the new file
and remove the old one. Until then the old registry's lines are still
read, so no key changes before the move.

**Older formats are read and rewritten.** `ignored_suggestions` and
`taught_suggestions` are at format 2, with the shared header. Their
format 1, with its own header line, is still read, and
`jadpo records update` rewrites it.

**Records don't stop a program running.** `record.out_of_date` and
`record.edited` are promises that `jadpo check` and CI hold you to, so
`jadpo test` and `jadpo dev` run past them. `record.format_newer` stops
them, because this Jadpo can't read what the project holds.

**`jadpo records` shows each record's state** as a symbol and a word
under the command that writes it: `✓ up to date`, `✗ out of date`,
`✗ edited by hand`, `✗ missing`, `✗ old name`, `✗ newer format`, or
`– not checked` while the source has errors. Like `jadpo check`, it fails
while any record needs its command.

What some entries hold:

- **The API record.** An `api <name>` entry carries one
  `authentication <method>` line per method, sorted, and
  `idempotency required`. The entry is left out when it has neither. A
  route's `generated` and `caller` facts come before `input`. Enum
  variants are sorted like fields, as `variant <name>`, and a variant's
  fields sit one indent deeper. Types sort by born name without their
  domain, then by name now. A failure's entry is its `status`, then its
  `message` as a Jadpo string, then its client fields.
- **The lock.** A `git` source records the repository, the tag and the
  tag's commit: `package AcmeCouriers.v1 version 1.4.0 git
  github.com/acme/acme-couriers tag v1.4.0 commit <40 hex> digest
  sha256:<64 hex>`. A `registry` source is
  `version <v> registry digest <d>`, and a `path` source is
  `version <v> path <folder> digest <d>`. A `link` is a live folder, so
  it has no version or digest: `package AcmePricing.v2 link
  ../acme-pricing`. Versions are written like `1.4.0`, with an optional
  pre-release. A commit is 40 or 64 lower-case hex digits. Lines sort by
  name, then by major version as a number.
- A project that pins the Jadpo release it runs with has that pin as the
  lock's first line, `jadpo version 0.1.2`. The release has no
  pre-release, and it must be on the release line `app.jadpo` names, such
  as 0.1. A pin on another release line is `record.out_of_date`
  ([Which jadpo runs](14-tooling.md#which-jadpo-runs)).

### The record format

Every record is written in one small line format, an outline in Jadpo's
own words. A change touches only the lines of the fact that changed, and
git's hunk header names the entry the change is in.

**The header.** Line 1 is `// Generated by Jadpo from <what it
describes>; update it with <command>, never by hand.` Line 2 is `record
<kind> <format>`, where the kind is one of `api`, `events`, `surface`,
`identities`, `translations`, `ignored_suggestions`,
`taught_suggestions` and `lock`. Any other `//` line is a hand edit.
Nothing anywhere in a record changes on its own: it holds no timestamp,
compiler version, count, total, digest of the whole file or alignment.

**Entries.** A heading starts in column 0, with a word from the kind's
closed list (`POST /todos`, `operation bookings.create`). Its members
follow, indented four spaces, and a member's own fields, such as an enum
variant's, are indented eight. Nothing goes deeper. A member is one of
three things:

- a description, `/// text`;
- a field, `name: value`, written as in source;
- a fact, which starts with a lower-case word from the kind's closed
  list: `status 201`, `fails todo_not_found`, `file types/booking.jadpo`.

A list repeats its word, one item per line. A flag is a fact that is
present or absent (`changes`, `idempotent`, `accepted`), never
`changes: true`. A line's kind is known from its first characters, so a
tool can read a record without a Jadpo parser.

**Views and ledgers.** A view's entries are outlines. A ledger's entry
is its heading alone, one self-contained line, so git's `union` merge
can combine what two branches added. `jadpo check` then catches what that
merge gets wrong ([Checking a record](#checking-a-record)).

**Values are Jadpo**, written on one line as the formatter writes them in
source: names, dotted names, strings, numbers and type expressions
(`Text { min_length: 1, max_length: 200 }`). Paths and URLs are never
quoted, and a file path is relative to the record's folder. A digest is
`sha256:` and 64 lower-case hex digits. A hash is 12 hex digits, since it
isn't a security check.

**Layout.** UTF-8, LF line endings, a final newline, no trailing spaces,
no tabs, one space between tokens and no alignment, so a long new name
never re-indents its neighbours. One blank line follows the header and
separates entries that have members; one-line entries have no blank
lines between them. Strings use Jadpo's escapes and only those: `\"`,
`\\`, `\n`, `\r`, `\t`. A description is the declaration's first
paragraph joined onto one `///` line, so rewrapping the comment in source
changes nothing. Records are compared with CRLF read as LF.

**Order is by identity.** Everything is sorted; only a function's
parameters keep the order they are written in. Fields sort by name,
since field order is no part of a JSON contract. Repeated facts (`fails`,
`host`) sort by value. Where the identity registry has a line for a
thing, the thing sorts by its born name, so a rename changes lines in
place instead of moving a block.

| Record | Sections, in order | Within a section |
|---|---|---|
| `api` | `api`, routes, named types, failures | Routes by path, then method; types by born name; failures by code |
| `surface` | `package`, `provider`, `provides`, operations, actions, polls, events, functions, messages, types, failures, recipes | By born name; operations by `resource.operation` |
| `identities` | One | By born name, then kind |
| `translations` | One | By locale, then member |
| `lock` | One | The `jadpo` pin, then by package name, then major version |

**Versions.** Each kind has its own format number, starting at 1. Any
change to what a kind holds or how it is laid out raises it, additions
included, since a view is compared byte for byte. A view in an older
format is out of date, and its command rewrites it. A ledger holds
history, so Jadpo reads it in every format it has ever written for that
kind, and rewrites it in the newest. A record in a newer format than this
Jadpo writes is `record.format_newer`: Jadpo never guesses. The lock's
format 1 already holds the `jadpo version` pin, since no lock had shipped
before the pin was added.

The identity registry's lines are the kind, the born name, `->` and the
name now. A field, variant or rule follows what it belongs to, so its
line gives only its own new name, and renaming `Task` again never touches
it:

```text
entity Todo -> Task
field Todo.priority -> importance
```

A translation source's line holds three facts: the hash of the default
wording it was translated from (`from`), the hash of its own wording when
that was recorded (`wording`), and `accepted` once a person accepted it
([Translations as messages](10-configuration-and-time.md#translations-as-messages)).
Whether a translation is stale or changed is worked out from those, never
stored:

```text
translation de_de.order_shipped from 3f9a1c0e2b7d wording 77b2e1d4c8a9 accepted
translation fr_fr.order_shipped from 3f9a1c0e2b7d wording a01c9e44f3b8
```

### Records in git and in your editor

`jadpo new` writes these lines into a new project's `.gitattributes`.
`linguist-generated=false` keeps every record's diff expanded in a pull
request, since records are committed so that their diff is read.
`merge=union` is git's own merge driver for ledgers, and needs no set-up:

```text
*.jadpo_record                       text eol=lf linguist-generated=false
jadpo.lock                           text eol=lf linguist-generated=false merge=union
database/identities.jadpo_record     merge=union
**/translations/sources.jadpo_record merge=union
```

Editors get a record language: a TextMate grammar,
`source.jadpo-record`, in `editors/vscode`, and `jadpo lsp` serving a
record's errors, outline, file links and folding.

Not built yet: the base commit's build, which comes with `jadpo
approval`, so every view is checked by its shape alone, and the record
language for editors ([What is built](#what-is-built)).

## The manifest
<!-- topic: the-manifest -->

`jadpo check` writes one manifest for your project,
`build/manifest.json`, whether the project has errors or not. It is the
compiler's graph as versioned JSON: every declaration and field with its
stable identity, source file and lines, content digest, `///`
description, calls, reads, writes, events and failures. It also holds the
routes, schedules, subscribers, connections and their hosts, policy
grants, personal fields, thresholds, the control flow that path numbers
are decoded against, and each deployable's build id.

```json
{
  "format": 2,
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

**Every report is a view of it.** The permissions, personal-data,
network, database and query reports, the catalogues, OpenAPI and the
monitoring rules are all made from the manifest, so they can't disagree
with each other.

**A build id is a digest** of everything that decides a deployable's
behaviour: the source it reaches, the compiler version, the target and
the locked packages. The same build id means the same build. The source
is the deployable's program as it is compiled from what it reaches: its
Rust and its plan, with its name and its stores' migrations. So a change
to what only another deployable reaches, or to a test, leaves its id
alone ([design notes](notes/18-records-and-manifest.md), "What a build id
digests"). A project with errors builds nothing, so the manifest gives
its deployables no `build`.

**The format.** The JSON is canonical: sorted, with relative paths and no
timestamps. The `format` number changes only when a field is removed or
changes meaning; additions keep the number. Manifests are never stored,
so no tool ever reads an old format. `jadpo help --json` documents the
schema, with every node and edge kind and whether it is built yet.

**Its size doesn't matter**, because it is never shipped. An agent asks
for one node with `jadpo inspect <name> --json` rather than reading the
whole file.

### What a node holds

The manifest is at `format` 2, with `compiler`, `errors` and `nodes`. A
node has `id`, `kind`, `name`, `file`, `lines` and `digest`. Where they
apply, it also has:

- `domain`, `parent`, `public` and `doc`;
- a type's `base`, `rules` and `failure`;
- a field's `type`;
- a callable's `signature`;
- an action's or fixture's `isolation`, `read_committed` or
  `repeatable_read` ([Isolation levels](06-reads-and-writes.md#isolation-levels));
- its edges, as sorted lists by kind: `calls`, `uses`, and `raises` for
  the failures that can leave a callable (the `fails` above).

The build stamp is in `source`, `deployables` and their build ids.

Other parts of the language add their own nodes:

- **Integrations, identity, configuration, jobs and events** add a node
  for each connection, package, resource, operation, capability mapping,
  host and lock line, each kind of caller, sign-in method, way of signing
  in and route the runtime answers itself, each setting, the locale set,
  the rate limits, and each schedule and subscriber. Their edges are
  `emits`, `subscribes`, `runs` and `provides`.
- **[Policy](09-policy.md)** adds a `grant` node for each list an
  entity's `policy:` writes, and a `field_restriction` for each field's
  own policy (`serverOnly`, or `who`: each effect's list). It adds a
  `tenancy` for each entity that belongs to the tenant, and an
  `action_exception` for each line under root `policy { actions: }`.
  `grants` edges go to what each decides, and `scopes` edges go from each
  tenancy to its entity.
- **[Prompts](21-ai.md#prompts-as-files)** each add a `prompt` node,
  placed at its `.prompt` file, with its type as its parent and its
  `version`: its name and the first 8 hex characters of a SHA-256 over
  its two files, `answer_question@864ad76b`.

Which parts of the language have nodes so far is in
[What is built](#what-is-built).

## Stable identity
<!-- topic: stable-identity -->

Jadpo knows a declaration or field by **the name it was born with**, so a
rename never looks like something removed and something added. The
identity registry holds one line for each thing that has been renamed
since, mapping its name now to its born name, and nothing else: a line
for every declaration would grow with every new action. Children follow
their parent, so renaming a domain or an entity is one line.

Three things write the registry: `jadpo rename`, the `jadpo domain`
commands, and promoting a function or type declared in a domain to
`shared/`.

```text
// Generated by Jadpo from renames and moves; update it with `jadpo rename`, `jadpo domain` or `jadpo fmt --layout`, never by hand.
record identities 1

entity Todo -> Task
field Todo.priority -> importance
subscriber send_reminder -> notify_owner
```

- **One registry for data and code.** Stored rows name code: outbox
  events, delivery records and schedule state in the
  [system tables](05-data.md#system-tables) refer to events, subscribers
  and schedules. Renaming a subscriber must not orphan its pending
  deliveries, any more than renaming a field may lose its column.
- **Physical names never change.** The table stays `todo` and the column
  `priority`. Migration snapshots hold the whole schema with physical
  names, so a rename made by hand shows as a removal and an addition, and
  `jadpo migrate new` refuses it, pointing you to `jadpo rename`
  ([Tooling](14-tooling.md)).
- **Inside a callable**, branches and fault sites are numbered afresh in
  each build. Only declarations and fields need to be recognised across
  builds.
- `jadpo incident`, [path keys](16-built-in-services.md#path-ids) and the
  review all compare through this registry, so a rename reads as
  "renamed", never as "removed and added".

### Keys

A key is the kind and the born name, in the registry's own form:
`action complete_todo`, `field Todo.title`,
`function rounding.half_even` (a library keeps its name in `shared/`).
Names are unique across the project and across kinds
([Names are unique](01-project-layout-and-modules.md#names-are-unique)),
so the domain is a field of the node, not part of the key.

The exception is a function or type declared in a domain, whose name
needs to be unique only within its domain. Its key carries the domain,
whether it is public or not: `type billing.Address`,
`function billing.format_address`. Its fields and variants follow it:
`field billing.Address.street`.

Adding `public`, or a route starting to return the type, never changes a
key. Promoting one to `shared/` does. So when a type moves to
`shared/types/`, the edits `layout.promotion_required` asks for add its
registry line, `type billing.Address -> Address`, which
`jadpo fmt --layout` writes. The key stays `type billing.Address`, and the
move reads as a move, not as a removal and an addition. That is why the
registry's header names `jadpo fmt --layout` beside `jadpo rename` and
`jadpo domain`.

A field's, variant's or rule's line gives only its own new name
(`field Todo.priority -> importance`), read back through its owner's
line. Every other line gives the whole name now. The registry is
`database/identities.jadpo_record`. Jadpo reads it once `jadpo rename`
writes it; until then every born name is the name now.

A test's key is the file it sits beside and its sentence, with no path:
`test create_todo "rejects a blank title"`. Moving its domain keeps the
key. Two tests with one sentence anywhere in the project are
`semantic.duplicate_declaration`.

Changing what a key holds is a new manifest `format`.

## Comparing two builds
<!-- topic: comparing-two-builds -->

Every review is a diff of two manifests. The **after** side is your
working tree. The **before** side is a git ref.

By default, the before side is the repository's default branch (`main`
or `master`, read from the git remote) at its merge base with your
current branch, as a pull request's diff is. So changes merged into the
default branch since you branched don't show as reversed. On the default
branch itself, the before side is the previous commit. Pass
`--before <branch or commit>` to compare with anything else. The same
default serves `check`, `jadpo approval` and the review web view.

The compiler reads that commit from git, builds it with the same
compiler, and caches its manifest in `build/before/<commit>/`, so this
costs one build per base commit.

- **`jadpo check`** uses the diff to fail on any change that needs a
  person and hasn't been accepted
  ([Review and sign-off](14-tooling.md#review-and-sign-off)). In CI, the
  base commit must be fetched: a full clone, not a depth-1 checkout.
- **`jadpo approval`** explains the same diff as Markdown and JSON in
  `build/review/`, decisions first, in the order
  [Reviewing a change](17-debugging-and-review.md#reviewing-a-change)
  gives. For production, pass the commit that the deployed build's events
  carry.
- **Records come with it.** The API record's diff, the migrations added,
  the lock changes and the acceptances added are part of the before and
  after trees, so the review lists them without a second mechanism.
- **A base that doesn't compile.** If the base doesn't compile with this
  compiler, for example after a language upgrade, `check` compares the
  records only and says so. Upgrade the compiler in its own change.

`jadpo approval` isn't built yet ([What is built](#what-is-built)).

## Acceptances
<!-- topic: acceptances -->

Some changes can break someone else, such as removing a field clients
read. `jadpo check` fails on them until a person accepts them. An
acceptance is one file per **decision**, listing every change it allows,
with the reason in a `///` line. Each entry is `subject: Kind.change`,
with the subject and the change copied from the compiler's message:

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
- **Drafting is a command.** `jadpo acceptance new <purpose>` writes a
  file listing every change not yet accepted, with no reason. An agent
  may run it, then stops: the missing reason keeps `check` failing until
  a person writes one.
- **The git host is the gate.** `acceptances/` and `api/` are owned in
  CODEOWNERS, so the owner's approval of the pull request is what lets a
  decision in, and the host records who gave it
  ([Review and sign-off](14-tooling.md#review-and-sign-off)).

| Needs a person | Kind | Where it is described |
|---|---|---|
| A route removed, a required input field added, a rule tightened, an output field removed or retyped, a failure code changed | `ApiChange` | [The public API record](07-http.md#the-public-api-record) |
| A column or table dropped, an entity moved to another store | `DataChange` | [Migrations](05-data.md#migrations) |
| Any widening of who can do what, however it arises | `PolicyChange` | [Policy](09-policy.md) |
| A domain created or renamed | `DomainChange` | [Domains](01-project-layout-and-modules.md#domains) |
| A risky package change ([Package upgrades](#package-upgrades)) | `PackageChange` | [Integrations](11-integrations.md) |
| A package's service: a new or changed container image, before it runs | `PackageChange` | [Integrations](11-integrations.md) |
| Installing a third party's package from git rather than the registry (an unverified publisher) | `PackageChange` | [Integrations](11-integrations.md) |
| A connection that receives personal data but can't erase it, with the reason the data stays | `ErasureNotPossible` | [Integrations](11-integrations.md), [Erasure](15-personal-data.md#erasure) |
| A threshold raised or removed | `ThresholdChange` | [Monitoring thresholds](16-built-in-services.md#monitoring-thresholds) |

Changes that only add need no acceptance: a new route, a new optional
input field, a new output field, tightened policy. The diff still shows
them.

Acceptances aren't built yet ([What is built](#what-is-built)).

## Package upgrades
<!-- topic: package-upgrades -->

An upgrade is a change to `jadpo.lock` and `packages/`, reviewed like any
other change, its `surface.jadpo_record` diff included. The review
compares the provider operations and library declarations your app uses,
matched by package, resource and operation with the version ignored. So a
call moved from one version's connection to the next is compared, not
listed as removed and added.

- **Needs a person:** a used operation removed; a used field removed or
  retyped; a new failure on a used operation; a host or credential scheme
  changed; a service's image new or changed; a package installed from a
  third party's git repository.
- **Doesn't need a person:** new operations or optional fields,
  documentation, and changes to operations your app never calls.
- **Shown, without needing a person:** a new `outbound:` grant to a
  service, a new connection, a package from your own company's git
  repository.

`jadpo package update <name>` prints this part of the review. It is the
same output as the upgrade plan [Integrations](11-integrations.md)
refers to, not a separate format.

## Production builds
<!-- topic: production-builds -->

A deployable carries a small **build stamp**, not its manifest. The stamp
holds the build id, the commit, whether the tree was modified, the
compiler version, the deployable and the target. Every log event carries
it. Each fault site carries its `.jadpo` file and line as a constant, so
events already show `.jadpo` frames
([Errors point at the `.jadpo` line](17-debugging-and-review.md#errors-point-at-the-jadpo-line)).

- **The manifest is rebuilt, not shipped.** Builds are deterministic
  ([Tooling](14-tooling.md)), so `jadpo incident` builds the event's
  commit, checks that the build id matches, and caches the result.
  Nothing is uploaded or archived. `jadpo` fetches and runs the exact
  compiler version the stamp names, as `rustup` installs a pinned
  toolchain, so an old build can always be rebuilt.
- **No endpoint.** The deployed app reveals nothing about its structure.
  This also works for a worker with no HTTP surface.
- **Native and Wasm alike.** Nothing depends on debug information or
  unwinding: Rust Workers abort on panic with no stack, so the path comes
  from the runtime's own spans. On Workers the stamp is logged beside
  `CF_VERSION_METADATA`, so the platform's version and the Jadpo build can
  be joined.
- **Deploy only commits that stay in history**, on the default branch or
  a tag. A build from a modified tree is marked as such, and can be
  explained only from that exact tree.

`jadpo incident` isn't built yet ([What is built](#what-is-built)).

## What an agent runs
<!-- topic: what-an-agent-runs -->

Every command takes `--json`, and `jadpo help --json` describes them all
([Tools for agents](14-tooling.md#tools-for-agents)).

| Job | Command | Then |
|---|---|---|
| After any edit | `jadpo check --json` | Each error says who acts (`agent` or `human`) and the next command |
| A record is out of date | `jadpo records`, then the command it names (`jadpo api update`, `jadpo migrate new <purpose>` …) | Commit the record with the code |
| Rename anything | `jadpo rename <old> <new>` | The registry gains one line |
| What did my change do? | `jadpo approval --json` | Decisions first |
| A change needs a person | `jadpo acceptance new <purpose>` | Stop and hand over |
| Look something up | `jadpo inspect <name> --json` | One node from the manifest |
| Upgrade a package | `jadpo package update <name>` | Fix compile errors, then as above |
| Explain a production event | `jadpo incident <event>` | `jadpo test new --from-event` |

## Examples: an API change, a package upgrade and an incident
<!-- topic: records-worked-examples (was three-journeys) -->

### An agent removes an API field, and a person accepts it

```text
$ jadpo check
error  api/main.jadpo_record is out of date: priority removed from TodoView, TodoSummary
       next  jadpo api update                                     agent
error  removing an output field can break clients: main.TodoView.priority,
       main.TodoSummary.priority
       next  a person accepts it: jadpo acceptance new <purpose>  human
$ jadpo api update
$ jadpo acceptance new remove_todo_priority
wrote acceptances/20261020T1400_remove_todo_priority.jadpo (2 changes, reason needed)
```

The agent commits and opens the pull request, saying a reason is needed.
CI runs `jadpo check`, which fails on the missing reason, and publishes
the review. The code owner reads it, writes the `///` line, and approves.
CI passes and the change merges. On the default branch the acceptance is
now history.

### A provider package upgrade

```text
$ jadpo package update couriers
couriers  AcmeCouriers.v1 → v2   jadpo.lock updated
  bookings.create   new failure  address_unverifiable           needs a person
  bookings.get      field removed  Booking.driver_phone         needs a person
  host              api.acme.example → api.eu.acme.example      needs a person
```

The removed field is also a compile error in `delivery_view`, which the
agent fixes first. The agent then runs
`jadpo acceptance new upgrade_couriers_v2` and hands over. A person
writes one reason covering all three changes, and the owner approves.

### A production incident traced to source

A Worker logs a fault carrying its stamp (`build b_9a1f…`,
`commit 8d113e6`), its `.jadpo` frames and a path number. Someone pastes
it into `jadpo incident`:

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
compiler is a different version, the build id can't match. The frames and
names are still exact, because they are in the event, and the output says
that the path number can't be decoded.

`jadpo approval`, acceptances and `jadpo incident` aren't built yet
([What is built](#what-is-built)).

## What is built
<!-- topic: records-and-manifest-built -->

| Feature | Status |
|---|---|
| Manifest | Built: `jadpo check` writes `build/manifest.json` at `format` 2, with nodes for [project layout and modules](01-project-layout-and-modules.md), [values and types](02-values-and-types.md), [logic](03-logic.md) and [failures](04-failures.md) and their `calls`, `uses` and `raises` edges, policy's grants, field restrictions, tenancies and listed actions with `grants` and `scopes` edges, prompts, and the connections, packages, operations, hosts, lock lines, principals, sign-in, generated routes, settings, locale set, rate limits, schedules and subscribers of [Identity](08-identity.md), [Configuration and time](10-configuration-and-time.md), [Integrations](11-integrations.md) and [Jobs and events](12-jobs-and-events.md), and the build stamp. The graph's diff classifies their changes as [Package upgrades](#package-upgrades) and the [acceptances](#acceptances) table say (`PackageChange`, `PolicyChange`). Caches, OAuth scopes, personal fields and thresholds join as their parts of the language are built. |
| Views of it | Partly built: the library and failure catalogues are built, and one OpenAPI document per API in `build/openapi/`; the other reports and the monitoring rules come with their parts of the language. |
| Migrations | Built: `jadpo migrate new` writes each folder, and `jadpo check` fails while the entities and the latest snapshot disagree. Each folder is a `migration_snapshot` node in the manifest. |
| Stable identity | Built: a key is the kind and the born name, a function or type declared in a domain carries its domain, and a test's key is the file it sits beside and its sentence. The registry is read once `jadpo rename` writes it ([Tooling](14-tooling.md)). |
| API record | Partly built: `jadpo api update` writes `api/<api>.jadpo_record` for each API with routes, and `jadpo check` fails with `record.out_of_date` while one disagrees with the routes, naming what changed. Not built yet: breaking-change classification and acceptances. |
| `.jadpo_record` files | Partly built: every record in the record format, the API record without `version:`, the registry as `database/identities.jadpo_record`, the header check, `record.out_of_date`, `record.edited` and `record.format_newer`, `jadpo records` and `jadpo records update`, and the `.gitattributes` lines `jadpo new` writes. Not built yet: telling a careful hand edit of a view from a stale one, which needs the base build that comes with `jadpo approval`; the editor grammar and `jadpo lsp` serving records. |
| Event records | Built: one view per domain with events, `domains/<domain>/events/events.jadpo_record`, written by `jadpo records update` and checked as the API record is; what [The event record](12-jobs-and-events.md#the-event-record) records. Acceptances come with the API record's. |
| Lock and package surfaces | Built. The lock is a ledger read and checked like the others, and `jadpo check` holds each installed package to its line and digest ([Integrations](11-integrations.md) gives the digest), and its `jadpo version` pin to the line `app.jadpo` names. `jadpo toolchain pin` and `unpin` write the pin, and the package commands and `jadpo records update` keep it. `jadpo package add`, `update`, `remove`, `link` and `unlink` write it, each line through the ledger's writer, sorted; `git` lines record the repository as written, the tag and the tag's commit, `path` lines a folder relative to the root, and a `link` line replaces the package's line while the replaced one waits in `.jadpo/links` (git-ignored, in the lock's own format) for `unlink`. Two versions of one package are locked side by side, each on its own line, and `update` names the version while two are held. A package surface is `record surface 1`, a view written by `jadpo package check` in a package's own repository, verified at install against its package's code, and read by `jadpo package show`; installed, a surface is held by its package's digest rather than regenerated, so `jadpo records` doesn't list it, and an own integration's is written to `build/providers/` ([Integrations](11-integrations.md)). `jadpo package update` prints this page's review of the surface's changes, each entry marked `used by the app` and `needs a person`; the acceptance it calls for comes with `jadpo approval`. |
| Build stamp | Built for native and Workers deployables: `jadpo build` writes the build id (`b_` and twelve hex digits of a SHA-256 over the deployable's program digest, the compiler, the target and `jadpo.lock`), the commit, whether the tree was modified, the compiler, the deployable and the target into the binary or module as a constant, and every event it logs carries them ([A native deployable](14-tooling.md#a-native-deployable), [A Workers deployable](14-tooling.md#a-workers-deployable)). `jadpo check` writes the manifest's `source` (`commit`, left out when git doesn't track the project, and `modified`) and `deployables`, each deployable's target and build id, which digests a declared deployable's declaration too, so two deployables of one app never share one ([Declared deployables](14-tooling.md#declared-deployables)); a project with errors has none. |
| Approval, acceptances, package upgrade plans, `jadpo incident` | Not built yet; see [the roadmap](../roadmap.md). |

## Errors

- [`record.out_of_date`](../errors/record.out_of_date.md)
- [`record.edited`](../errors/record.edited.md)
- [`record.format_newer`](../errors/record.format_newer.md)
