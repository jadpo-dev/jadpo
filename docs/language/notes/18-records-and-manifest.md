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
| New acceptances | A service's new or changed image and a third party's package from git need a `PackageChange` acceptance, and a connection that receives personal data but cannot erase it an `ErasureNotPossible` one (area 11) |
| The record format | Generated records are `.jadpo_record` files in an indented outline of Jadpo words: views (the API record, package surfaces) rebuilt and checked byte for byte, ledgers (identities, translation sources, the lock) of one-line entries merged by git's `union`; a header naming the generator and command, then `record <kind> <format>`; the API record has no `version:` counter (the OpenAPI version is a hash of the record) and no file pointers; surfaces keep file pointers and each description's first paragraph; `jadpo.lock` uses the format ([The record format](../18-records-and-manifest.md#the-record-format)) |
