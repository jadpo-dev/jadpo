---
title: Records and manifest, design notes
area: 18
---

# Records and manifest: design notes

Why [Records and manifest](../18-records-and-manifest.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Consistency with the rest of the language

- **Area 5's registry** is born names plus a list of renames, covering code
  too. Migration snapshots, `migrate new` and drift checks are area 5's.
- **Area 7's acceptances** are one per decision. An acceptance counts only
  in the change that adds it, and one that matches nothing there is an
  error. The API record holds no acceptance names.
- **Area 9:** "the whole application before and after" is the base
  commit's build against the working tree.
- **Area 11:** a changed host appears in the approval report; a host or
  credential-scheme change on a used operation also needs an acceptance.
  New connections only appear.
- **Area 14:** `build/` holds `manifest.json`, `report/` and `review/`.
  `jadpo inspect` prints the manifest, or one node. `check` reads git for
  its base. The manifest for an event's build id is found by rebuilding it.
- **Area 16:** the build manifest carries the thresholds. Per-request
  thresholds are compiled into the deployable.
- **Area 17:** the identity registry covers actions, queries and routes;
  the commit is in the build stamp; the review report lives in
  `build/review/`.
- **Compiler design** (`docs/compiler-runtime.md` §1.1 and §9): the
  manifest is the graph's export. Not shipping maps publicly matches §9.

## How records are built

- **One mechanism for every kind.** A kind is a word, a format number, a
  shape (view or ledger), its header's command and its closed lists of
  headings and facts; a view's writer gives entries and the differ
  compares entries by heading. Event shapes are a view kind beside the
  API record, package surfaces another, and the lock's writer uses the
  ledger.
- **One index of records.** The table at the top of the area page stays
  the single index of every kind of record; `jadpo records` lists a
  project's own.

## Carried to later areas

| Item | Area |
|---|---|
| Pending deliveries and schedule state when a subscriber or schedule is removed, not renamed | 12 |
| Diagnostics for unaccepted changes, unmatched entries, missing reasons and edited acceptances; `jadpo acceptance new`; `inspect <name>`; `check` reading git | 14 |
| The graph's shape: stable identities, per-build sites, the manifest as its export | Compiler design §1.1 |
| Loosened thresholds listed as `ThresholdChange` | 16 |
| Build stamp and `.jadpo` site constants in events; `jadpo incident` rebuilds the manifest | 17 |

## Decisions

| Topic | Decision |
|---|---|
| Compiler data | Carried from area 1: a visible `database/` folder for the identity registry, migrations and seeds |
| New domains | Carried from area 1: creating or renaming a domain appears in the approval review |
| Policy protection | Carried from area 1: through the approval check wherever policy lives, not by file path |
| Stable identity | A key is the kind and the born name (`action complete_todo`); the domain is a field of the node. A function or type declared in a domain carries its domain (`type billing.Address`), and its promotion to `shared/` writes a registry move line |
| Migration snapshots | Carried from area 5: each migration folder holds a generated `snapshot.json`, generated SQL and `changes.jadpo` only for decisions; `check` fails when entities and the latest snapshot disagree |
| Removing data | Carried from area 5: retire, then drop in a later migration; a drop needs explicit acceptance |
| System tables | Carried from area 5: runtime tables prefixed `jadpo_`, part of the migration snapshots |
| API record and acceptance | Carried from area 7: a committed record per API compared by `check`; breaking changes need a person-written acceptance; `acceptances/` owned in CODEOWNERS |
| Permissions report | Carried from area 9: `build/report/permissions.md` and `.json`, shown in the approval review |
| Package pin | Carried from area 11: registry packages by digest in `jadpo.lock`, local ones committed; never fetched during a build |
| Hosts | Carried from area 11: outbound access only to declared connections' hosts; a generated network report; changes in the approval report |
| `check` writes derived files | Carried from area 14: `check` writes `build/` except the runnable app |
| Renaming | Carried from area 14: `jadpo rename <old> <new>` updates source, callers, file names and the identity registry |
| Sign-off | Carried from area 14: acceptance files in a host-protected folder; CI's `jadpo check` fails without them |
| Incident input | Carried from area 14: the build's manifest is found by the event's build id |
| Committed files | A file is committed only if it cannot be recomputed from today's source or is a promise whose diff must be read: the records, migrations and `acceptances/`; area 1's root gains `api/`, `acceptances/` and `packages/`; installed packages are committed |
| Manifest | One `build/manifest.json`, the compiler's graph as versioned canonical JSON; every report and generated document is a view of it |
| Identity | One registry for data and code in `database/identities.jadpo_record`, holding renames only, written by `jadpo rename`, the domain commands and promotion |
| The before side | Current branch against the default branch (`main`/`master` from the remote) at their merge base; `--before <ref>` overrides; one default for `check`, `approval` and the review web view |
| Acceptances | One file per decision listing the changes it covers, with a person's reason; counts only in the change that adds it; `jadpo acceptance new` drafts it; CODEOWNERS approval is the gate |
| Package changes | A person accepts a used operation removed, a used field removed or retyped, a new failure on a used operation, a host or credential-scheme change, a service's new or changed image, or a third party's package from git; `jadpo package update` prints that part of the review |
| Production builds | Deployables carry a build stamp, not the manifest; `jadpo incident` rebuilds from the commit with the exact compiler version |
| What a build id digests | The deployable's program as compiled from what it reaches: the Rust its callables become, as the text of its tokens, its plan's bytes, its name and each store's migrations, with the compiler, the target and `jadpo.lock`; worked out by `jadpo check` without rustc. A change that moves a callable it reaches to other lines changes it, since its frames name them; so does the application's name, its folder's unless `app.jadpo` gives one, which its OpenAPI title holds, so a rebuild that explains an event is made in a folder of the same name. A project with errors builds nothing, and the manifest leaves each deployable's `build` out |
| Translation sources | Each `translations/` folder has a generated, committed `sources.jadpo_record`: per locale and message, the hash of the default wording it was translated from, and acceptances; written by `jadpo translations update`, `import` and `accept` (area 10) |
| Record files | Every committed record but `jadpo.lock` is a readable `.jadpo_record` beside what it describes (`api/main.jadpo_record`, `database/identities.jadpo_record`, `translations/sources.jadpo_record`, `surface.jadpo_record`), its first line naming what wrote it and the command that updates it; `jadpo check` regenerates it and a hand edit is an error naming that command; `jadpo records` lists them and flags any out of date; acceptances are not records |
| The lock's source | `jadpo.lock` records each installed package's version, digest and source: `registry`, `git` (repository, tag and the tag's commit), `path` or `link`; a link is refused by production builds and CI (area 11) |
| The lock's Jadpo pin | `jadpo.lock` may pin the exact Jadpo release the project runs with, one `jadpo version <release>` line before the packages, a release with no pre-release on the line `app.jadpo` names (another line is `record.out_of_date`); `jadpo toolchain pin` and `unpin` write it, and the package commands and `jadpo records update` keep it (area 14, "Which jadpo runs"). The lock stays `record lock 1`: no lock had shipped before the line was added, so format 1 is the format that has it |
| New acceptances | A service's new or changed image and a third party's package from git need a `PackageChange` acceptance, and a connection that receives personal data but cannot erase it an `ErasureNotPossible` one (area 11) |
| The record format | Generated records are `.jadpo_record` files in an indented outline of Jadpo words: views (the API record, package surfaces) rebuilt and checked byte for byte, ledgers (identities, translation sources, the lock) of one-line entries merged by git's `union`; a header naming the generator and command, then `record <kind> <format>`; the API record has no `version:` counter (the OpenAPI version is a hash of the record) and no file pointers; surfaces keep file pointers and each description's first paragraph; `jadpo.lock` uses the format ([The record format](../18-records-and-manifest.md#the-record-format)) |
| Sign-off for security declines and new hosts | Declining a rule the catalogue marks as a security rule is a `PolicyChange`, since an agent can write a decline's reason itself; a new outside host the app can send data to is a `NetworkChange`, personal or not. `access.jadpo_record` and `network.jadpo_record`, kept by `jadpo check` beside `api/`, give each check its before side and put the change in the pull request, where the permissions and network reports in `build/` never reached it (Richard, 2026-10-30) |
| Naming a subject | An acceptance's entry names its subject as a dotted name; a route, which has none, is its API, method and path in quotes (`"main GET /books": PolicyChange.route_public`). A quoted key is allowed only in an acceptance's `changes:`; anywhere else it stays a syntax error (Richard, 2026-10-30) |
| New outside hosts | Every row of `network.jadpo_record` the before side doesn't have is `NetworkChange.host_added`, a new connection's hosts included, named by the connection; this replaces "Package upgrades"' new `outbound:` grant and new connection shown without a person (Richard, 2026-10-30) |
| What widens access | Needs a person: a caller added to any grant list; a field's own row or `server_only` removed; tenancy loosened, or a new entity shared in an app with a tenant; a listed query's line removed or a caller added to it; an `actions:` line added or removed, since a listing replaces its entities' grants; an existing route changed to `authentication none`. Needs no one: narrowing, listing a query, a new route even one anyone may call (Richard, 2026-10-30) |
| Event breaks | Events waiting in the outbox are read with the new shape and subscribers read what they carry, so an event is both sent and read: an event removed, a field removed or retyped, a required field added or a rule tightened is an `ApiChange` (Richard, 2026-10-30) |
| Sign-in loosened | A sign-in setting past its limit and looser than on the before side is `PolicyChange.sign_in_loosened`, subject `magic_link.<setting>`; `access.jadpo_record` keeps the settings that have a limit (Richard, 2026-10-30) |
| No before side | Outside git, with no default branch, before the first commit or for a project new since its base, nothing is compared or judged and `jadpo check` says so in one line; a record the before side doesn't have is compared with nothing and named on that line; a base commit a shallow clone lacks is `acceptance.base_missing`, naming `git fetch`, and CI checks out the whole history (Richard, 2026-10-30) |
| A first outside host | An app with a `connections:` block keeps `network.jadpo_record` even while it reaches no host, its header alone, so the change that adds its first host is compared with an empty record rather than with nothing; decided by the orchestrating session on the safe side, and reversible (2026-10-30) |
| A first connection | A before side with no `network.jadpo_record` whose `app.jadpo` has no `connections:` block reached no outside host, so its record is read as empty and compared as usual: the change that adds the first `connections:` block and host needs `NetworkChange.host_added`. Only a before side with the block and no record, a project from before the record, keeps the no-before-side rule; decided by the orchestrating session, a tightening and reversible (2026-10-31) |
| Change words | `ApiChange.route_removed`, `.input_field_required`, `.rule_tightened`, `.output_field_removed`, `.output_field_retyped`, `.failure_changed`, `.event_removed`; `PolicyChange.route_public`, `.widened`, `.restriction_removed`, `.tenancy_loosened`, `.shared_entity_added`, `.listing_added`, `.listing_removed`, `.<rule>_declined` on the declaration a security rule's decline sits above, `.sign_in_loosened`; `DomainChange.created`; `NetworkChange.host_added`; `PackageChange.git_source`. A security decline is kept in `access.jadpo_record`; the diagnostics are `acceptance.missing`, `.unmatched`, `.no_reason`, `.edited`, `.invalid` and `.base_missing` (Richard, 2026-10-30) |
