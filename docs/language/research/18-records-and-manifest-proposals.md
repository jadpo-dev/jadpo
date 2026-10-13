# Records and manifest: research and proposals

Area 18 pulls together pieces that eight areas each described separately:
the API record (area 7), the identity registry and migration snapshots
(area 5), the lock file (area 11), acceptances (areas 7 and 9), the reports
(areas 9, 14, 15 and 16), the approval report and incidents (areas 14 and
17). This note checks a starting proposal against how other tools solve
each piece, and records where the proposal changed.

## The starting proposal

- **Records** are promises to the outside world. They are committed and
  reviewed: `api/*.api`, `database/` (identities and migrations),
  `jadpo.lock` and `acceptances/`.
- **Build outputs** are facts about one build. They are regenerated and
  never committed: the reports, the monitoring rules and a **manifest** of
  every node's stable identity and source place.
- **Reviews are diffs.** The approval report, package upgrade plans and the
  review UI are all a diff of two manifests plus the records touched.
- **Production reads the deployed build.** Each deployable embeds its
  manifest and serves it on an authenticated internal endpoint.
- **One acceptance per decision**, which may list several changes. Package
  upgrades need one only for risky changes.

## What is complicated today

The pieces exist, or are proposed, independently, and none links to
another. Each finding below was run with `jadpo` 3ce094d0 on the golden
todo unless marked otherwise.

- **Two manifests.** `build/app.meta.json` is 101 KB: graph nodes keyed by
  a counter, plus operation ids such as `route:GET:/health/live`. The
  approval subject is a separate 6.1 MB file, with a 6.1 MB text copy.
- **No commit is recorded.** The only build identity is a source digest,
  `src_2c78570a2841d2f7`, so an event cannot be traced back to source.
- **`--before` takes a folder, not a git ref.** `jadpo approval --before
  main` fails with `toolchain.project_not_found`, because it expects a
  second checked-out project.
- **The identity registry covers tables and columns only** (read from
  `migration_identity.rs`). The golden todo has no registry.
- **There is no API record, acceptance or lock file.** A compatibility file
  exists, but says `baseline_not_configured`.

## How others do it

### Records of a public surface

| Tool | What is committed | How a break is accepted |
|---|---|---|
| Kotlin binary-compatibility-validator | A readable `.api` dump in `api/`; `apiCheck` runs in `check` | Run `apiDump`, then commit the diff. The PR review is the gate. |
| Microsoft API Extractor | `etc/<pkg>.api.md`. CI writes `temp/` and fails, asking you to copy it | Commit the report. Rushstack puts CODEOWNERS on the reviews folder. |
| cargo-semver-checks | Nothing. It compares with the last release or `--baseline-rev` | Lint levels in `Cargo.toml` |
| buf breaking | Nothing. It compares with `--against '.git#branch=main'` | Rules switched off in config. BSR can hold a breaking push as pending until an admin approves it. |
| oasdiff | Nothing. It compares base with revision | An `--err-ignore` file, one line per accepted break (method, path, change) |

Sources: [Kotlin BCV](https://github.com/Kotlin/binary-compatibility-validator),
[Kotlin ABI validation](https://kotlinlang.org/docs/gradle-binary-compatibility-validation.html),
[API Extractor report](https://api-extractor.com/pages/setup/configure_api_report/),
[cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks),
[buf rules](https://buf.build/docs/breaking/rules/),
[BSR breaking review](https://buf.build/docs/bsr/checks/breaking/),
[oasdiff](https://github.com/oasdiff/oasdiff/blob/main/docs/BREAKING-CHANGES.md).

**Lesson.** Tools that commit a record do so because its diff is what a
reviewer should read. Tools that commit nothing compare against a git ref
or a published release. Both work. Only oasdiff and Atlas record an
accepted break individually, and both key it by subject and change kind.

### Migration state

Prisma commits each migration's SQL, and `migrate deploy` never reads the
schema. Drift is found by replaying history into a shadow database
([Prisma](https://www.prisma.io/docs/orm/prisma-migrate/understanding-prisma-migrate/shadow-database)).
Rails commits the derived `schema.rb` because loading it is faster and old
migrations rot ([Rails](https://guides.rubyonrails.org/active_record_migrations.html)).
Django commits no snapshot and replays every migration in memory
([Django](https://docs.djangoproject.com/en/5.2/topics/migrations/)).

Atlas's `atlas.sum` hashes the migration folder, so two branches that both
add a migration conflict in git, on purpose. Atlas's lint flags destructive
changes, and `-- atlas:nolint` overrides one at a time. Atlas Cloud only
applies a plan that was approved
([Atlas integrity](https://atlasgo.io/concepts/migration-directory-integrity),
[analyzers](https://atlasgo.io/lint/analyzers),
[pre-approval](https://atlasgo.io/integrations/github-actions/pre-approval)).

For renames, Atlas and Gel ask "did you rename X?" when they write a
migration. Prisma separates the code name from the column name with `@map`.
Protobuf makes the field number the identity and reserves retired numbers
([Atlas v0.22](https://atlasgo.io/blog/2024/05/01/atlas-v-0-22),
[Gel](https://docs.geldata.com/learn/migrations),
[Prisma mapping](https://www.prisma.io/docs/orm/prisma-schema/data-model/database-mapping),
[protobuf reserved](https://protobuf.dev/programming-guides/proto3/#reserved)).
Unison names code by the hash of its syntax tree
([Unison](https://www.unison-lang.org/docs/the-big-idea/)).

**Lesson.** Area 5's design already matches the majority: a committed
migration with its snapshot, and drift checked against the latest one. For
renames, the Prisma `@map` idea is the cheapest form of identity: a thing
keeps the name it was born with until it is renamed.

### Locks

`Cargo.lock` pins each package's version, source and SHA-256 checksum.
`package-lock.json` v3 pins `resolved` and an SRI `integrity` hash. Both
are committed
([Cargo](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html),
[npm](https://docs.npmjs.com/cli/v10/configuring-npm/package-lock-json)).
Area 11 already decided the same for `jadpo.lock`.

### Build identity and debug information

- **Go** embeds module versions and `vcs.revision`, `vcs.time` and
  `vcs.modified` in every binary. `go version -m` reads them back
  ([debug.BuildInfo](https://pkg.go.dev/runtime/debug#BuildInfo)).
- **Rust** has no built-in equivalent. `built`, `vergen` and `shadow-rs`
  write the commit and dirty flag into constants. `cargo auditable` embeds
  the dependency tree, compressed, in under 4 kB, for Wasm too
  ([cargo-auditable](https://github.com/rust-secure-code/cargo-auditable)).
- **Sentry** dropped matching source maps by release and file name.
  Moving a file broke the link, so it now injects a content-derived debug
  ID into both the bundle and the map. Native code uses the ELF build-id,
  and Wasm a `build_id` custom section, with debug information split off
  and uploaded
  ([debug IDs](https://docs.sentry.io/platforms/javascript/sourcemaps/troubleshooting_js/debug-ids/),
  [Wasm](https://docs.sentry.io/platforms/native/guides/wasm/)).
- **SLSA provenance** links an artifact's digest to the source and builder
  that produced it, signed in CI
  ([SLSA v1](https://slsa.dev/spec/v1.0/provenance)).
- **Cloudflare Workers** gives each version an id and tag at runtime
  through `CF_VERSION_METADATA`
  ([version metadata](https://developers.cloudflare.com/workers/runtime-apis/bindings/version-metadata/)).
  The limits page, updated 12 October 2026, gives 64 MiB uncompressed and a
  1 s startup limit
  ([limits](https://developers.cloudflare.com/workers/platform/limits/)).
  That contradicts the 3 MB and 10 MB compressed limits documented
  earlier; treat the size as unconfirmed.

**Lesson.** The link between a running build and its description must be
a content-derived id carried by both, never a name. Sentry needs uploads
because a JavaScript bundle cannot be rebuilt identically later. A
deterministic compiler can rebuild it.

### Exposing build information

Spring Boot Actuator serves build and git information. With Spring Security
present, every endpoint except health needs authentication. Kubernetes
makes `/version` public by design. Go's `expvar` serves its JSON with no
authentication at all
([Actuator](https://docs.spring.io/spring-boot/reference/actuator/endpoints.html),
[Kubernetes RBAC](https://kubernetes.io/docs/reference/access-authn-authz/rbac/),
[expvar](https://pkg.go.dev/expvar)).

**Lesson.** A version and commit are safe to show. Routes, hosts and
schedules should not be served without a login, which is a new mechanism
to secure.

### Review gates

CODEOWNERS with required review makes one owner's approval the gate
([GitHub](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners)).
Terraform applies exactly the saved plan and refuses a stale one
([Terraform](https://developer.hashicorp.com/terraform/cli/commands/apply)).
Chromatic keeps accepted baselines per branch in its own service, outside
git ([Chromatic](https://www.chromatic.com/docs/branching-and-baselines/)).

## Pressure-testing the proposal

### 1. Records and build outputs: held

The split matches Kotlin, API Extractor, Rails and every lock file. The
commit rule is a test any agent can apply. Commit a file only if it cannot
be recomputed from today's source, or if it is a promise whose diff a
person must read. Everything else goes stale, conflicts in merges and
invites hand edits.

### 2. Policy, domains and thresholds have no record: the before side is a build

Areas 1, 9 and 16 need a person for a new domain, a policy widening and a
loosened threshold. None of these has a committed record, so there is
nothing to compare against. There are three options.

- **A. Commit more records** (a permissions record, a domain list). This
  gives more files to keep in step, and area 9 decided that the
  permissions report is generated.
- **B. Build the base.** Check out the base commit from git, compile it
  with the same compiler and diff the two manifests. This is what buf,
  oasdiff and cargo-semver-checks do. Results are cached per base commit.
- **C. Records only.** Policy changes would never be caught.

**Recommendation: B.** It is one comparison for every kind of change. The
API record and migrations stay because they are promises and history, not
because the comparison needs them. If a language upgrade means the base no
longer compiles, `check` compares the records only and says so. Upgrade
the compiler in its own change.

### 3. Using up an acceptance: simpler than area 7's

Area 7 has the record note which acceptance allowed each change, and makes
an unused acceptance an error. That puts acceptance names inside the API
record and needs a separate note for policy and domains, which have no
record. With a before side from option B there is a simpler rule. **An
acceptance counts only in the change that adds it.** Acceptances already
on the base are history. An added acceptance that matches nothing is an
error, and editing a merged one is an error. This is oasdiff's subject plus
kind key, without a standing ignore list.

**One per decision.** Removing a field from an output and from an input is
one decision, so one file lists both. Area 7's "one per change" is amended.

### 4. Stable identity: one registry, born names, renames only

There are three options.

- **Dense.** Every node gets a random id in the registry. This is robust,
  but every new action, query or route adds a line, and parallel branches
  conflict.
- **Content hash** (Unison). Renames are free, but any edit changes the
  identity, and text-based review loses track of it.
- **Born name.** A thing is known by the name it was created with. The
  registry records only renames, as new name to born name, and only
  `jadpo rename` and the domain commands write it. This is Prisma's `@map`
  kept out of the source.

**Recommendation: born name, in one registry for data and code.** Code
belongs in it because stored rows name code: outbox events, delivery
records and schedule state (area 5's system tables) refer to events,
subscribers and schedules. A rename must not orphan a pending delivery any
more than a column. Migration snapshots still hold the whole schema, so a
hand-made rename still shows as a removal and an addition, and `migrate
new` refuses it (area 14). Inside a callable, branches and fault sites are
numbered per build. Only declarations and fields need identity across
builds.

### 5. Production: rebuild the manifest from the commit, embed nothing large

| | A. Embed manifest, serve on an endpoint | B. Rebuild from the commit | C. Embed manifest, read from the artifact file |
|---|---|---|---|
| Works without the repository | Yes | No | Yes, given the file |
| Works for deployables with no HTTP | No | Yes | Yes |
| New security surface | An endpoint revealing routes, hosts and policy | None | None |
| Artifact size | Grows with the app (the golden todo's map is 13 KB compressed; a large app is more) | A small build stamp | Grows |
| Can disagree with source | No | No (deterministic builds, verified by build id) | No |

**Recommendation: B.** Area 14 decided that builds are deterministic, so
a commit plus the compiler version gives back the exact manifest, verified
against the build id the event carries. Anyone who can fix an incident has
the repository.

The deployable embeds only a **build stamp**: build id, commit, modified
flag, compiler version, deployable and target. Each fault site carries its
`.jadpo` file and line as a constant, as Rust panic locations do, and that
is what area 17's mapped stacks need. This works the same on native and
Wasm, because nothing depends on DWARF, Wasm `build_id` sections or
unwinding. Cloudflare's Rust Workers abort on panic with no stack, so the
node path comes from the runtime's own spans.

The costs:

- **Deploy only commits that stay in history** (the main branch or a tag).
- **Uncommitted changes.** A build from a modified tree can only be
  explained from that exact tree, which is today's rule.
- **Path numbers need the same compiler.** With a different compiler
  version, frames and node names are still exact, because they are in the
  event, but the path number cannot be decoded, and the output says so.

A hosted monitoring product can still take manifests uploaded by CI later.
That is an addition, not a dependency.

### 6. Package upgrades: classify like cargo-semver-checks, accept like oasdiff

An upgrade appears as a change in `jadpo.lock` or `providers/`. Comparing
the two manifests yields the operations the app uses, matched by package,
resource and operation with the version ignored. Area 11 runs two versions
side by side as two connections, and this matching lets a call moved
between them be compared.

These changes need a person:

- a used operation removed;
- a used field removed or retyped;
- a new failure on a used operation;
- a host or credential scheme changed.

These do not: new operations, new optional fields, documentation, and
changes to operations the app does not call. A used field removed is also
a compile error, which the agent fixes first. The acceptance records that
a person knew.

Area 11's "upgrade plan" idea needs no format of its own. It is this
review, limited to one package.

## Recommendation summary

- **Four records:** `api/`, `database/` (migrations, and a registry that
  holds renames only), `jadpo.lock` and `acceptances/`. Beside the source
  (which includes `providers/`), nothing else is committed.
- **One manifest,** `build/manifest.json`. It is the compiler's graph,
  versioned, and every report is a view of it.
- **One comparison.** Two manifests are diffed: the base commit's (built and
  cached) and the working tree's. `check` gates on that diff, `approval`
  explains it, and incidents use the build an event names.
- **One acceptance per decision,** counted only in the change that adds it.
- **Production carries a stamp, not a manifest.** Every event carries the
  build id and commit, and the manifest is rebuilt from the commit.
