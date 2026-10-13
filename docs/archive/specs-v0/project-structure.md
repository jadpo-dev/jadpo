# Project structure workstream

**Status:** entity/query/workflow role validation implemented; broader workspace enforcement remains provisional
**Accepted decision:** DATA-007, 2026-09-26
**Remaining pressure point:** large applications, packages, and companion files

## 1. Motivation

A common project structure could remove an entire class of low-value decisions
from both developers and coding agents. A developer moving between projects
would know where domain types, actions, routes, policy, tests, integrations, and
generated artifacts live. An LLM would spend less context and fewer tokens
rediscovering local naming and directory conventions, and would be less likely
to create parallel abstractions in arbitrary locations.

This is aligned with the language's wider preference for one canonical form,
but structure has a larger blast radius than surface syntax. An over-fitted
layout can make unusual or larger systems awkward, encourage empty ceremony in
small applications, or accidentally turn directories into an undeclared module
system. The project should therefore test a concrete structure before enforcing
it.

## 2. Accepted semantic roles and remaining hypothesis

The leading hypothesis is a hybrid structure:

- a small fixed set of top-level semantic roles;
- feature or domain slices beneath the application area;
- optional roles disappear when unused rather than requiring empty folders;
- generated output has one compiler-owned location and never mixes with source;
- naming is canonical where a role has exactly one meaning;
- bounded extension points cover integrations and genuinely unusual assets;
- arbitrary alternative layouts are not supported merely for personal taste.

DATA-007 now accepts a bounded role-first source core:

```text
app.jadpo
entities/       # one authoritative identity-bearing concept per file
values/         # identity-free domain values and projections
queries/        # named cross-entity read models
workflows/      # multi-entity and application-level actions
routes/         # transport bindings
jobs/           # scheduled/durable entry points once specified
```

For the current executable layout, put shared `failure` declarations in root
`app.jadpo`; an entity dossier file may also contain its supporting failures.
`values/` accepts value declarations only, so a failure beside a value in that
directory is rejected. Root `app.jadpo` can also hold authored tests and fixtures;
no separate failure or test directory role is required for a small application.


Unused roles may be absent. Entity-centred queries, functions, and actions live
in the authoritative entity dossier; cross-entity reads live under `queries/`;
multi-entity transactions are orchestrated under `workflows/`. The complete
semantic contract is the [entity, query, and transaction
model](entity-query-model.md).

Store authority, caches, and projections are capabilities declared in the
authoritative entity dossier, not extra entity definitions or arbitrary
integration files. Multi-authority durable workflows remain under
`workflows/`. Generated outbox delivery, projection workers, and reconciliation
artifacts belong to compiler-owned output rather than authored source roles.

These directories are compiler-validated organisation, not a second identity system.
Explicit declarations, modules, and compiler-owned stable IDs remain semantic
authority, so moving a file does not create a new schema object. Invalid
declaration kinds in recognised role directories produce
`DATA_PROJECT_ROLE_INVALID`. Large-entity companion files, packages, monorepos,
and integrations remain hypotheses to pressure-test.

## 3. Scaffolding strategy

The initial recommendation is a deterministic static base scaffold, not a
free-form “describe your application” generator.

```text
jadpo new my_application
```

The same command must work for a human or an LLM and produce the same files for
the same compiler version. The base should establish project identity, source
and test roots, generated-output boundaries, configuration conventions, and one
small compiling application. It should not guess domains, entities, routes, or
architecture from natural-language answers.

Physical and conceptual structure need not be identical. The base may omit
unused empty directories while still defining where each recognised role will
be created. Deterministic additive commands can materialise them when needed:

```text
compiler add http
compiler add persistence
compiler add authentication
compiler add jobs
compiler add integration payments
```

These are capability packs, not alternative architectures. They may add known
directories, declarations, configuration, and tests, but must not rename or
rearrange the canonical base. Adding a capability later should produce the same
shape as selecting it during creation.

An interactive wizard may exist for humans, but it must be a thin interface
over explicit, finite options:

```text
jadpo new my_application --with http,persistence,authentication
```

Agents should normally use the non-interactive form. The chosen capabilities
and scaffold version should be recorded in machine-readable project metadata so
the result is reproducible and upgradeable.

The first prototype should support only the static base. Capability packs or a
small set of application profiles should be added only when repeated project
shapes demonstrate real value. The generator must not create arbitrary folders
or naming schemes from an unconstrained model response.

## 4. Remaining questions

The accepted role core resolves where entities, value projections,
cross-entity queries, workflows, and routes belong. The remaining proposal must
decide explicitly:

- how layout relates to future files, modules, imports, visibility, and package
  identity;
- whether a tiny application may remain a single source file;
- when an entity may gain compiler-recognised companion files and which
  filenames are canonical without permitting partial entity declarations;
- where shared semantic types live without becoming an unstructured dumping
  ground;
- where application-wide policy, events, service contracts, configuration,
  migrations, integrations, and tests belong;
- whether tests are colocated, mirrored in a test tree, or compiler-derived;
- how monorepos, multiple deployable applications, libraries, and generated
  clients fit without weakening the default;
- which files are authored, compiler-generated, local-only, or deployable;
- whether the compiler discovers files by convention, a manifest, or both;
- how moves and renames preserve stable semantic identity;
- what escape mechanism exists, who may use it, and whether it produces an
  audit finding;
- whether the base physically creates every conventional directory or records
  optional locations and creates them on demand;
- which capabilities are sufficiently stable to deserve additive scaffold
  commands;
- how scaffold upgrades work without overwriting authored files;
- how a generated project records the scaffold version and exact creation
  recipe.

## 5. Enforcement ladder

The decision is not simply “convention or no convention.” Candidate levels are:

1. documentation and examples only;
2. a canonical scaffolder default;
3. compiler warnings for non-canonical placement;
4. compiler errors for invalid placement;
5. strict defaults with a narrow, explicit, reviewable escape declaration.

The project should adopt the strongest level supported by evidence. If an
escape exists, it must not silently disable semantic checks or permit generated
code to be mixed into authored source.

## 6. Required pressure tests

Any candidate must be exercised against at least:

- the single-file Jadpo seed;
- a small CRUD application;
- the canonical todo backend with authentication, ownership, jobs, and an
  external service;
- a multi-domain order/payment application;
- an integration-heavy service with generated provider contracts;
- multiple persistence adapters or deployment targets;
- a repository containing more than one deployable application;
- growth from a small file to a feature directory without changing semantic
  meaning;
- creating the same project interactively and non-interactively with
  byte-identical results;
- adding a capability at creation time versus adding it later.

For each case, record discoverability, number of structural choices left to an
agent, naming collisions, import complexity, refactor cost, navigation quality,
and required exceptions.

## 7. Acceptance criteria

An enforceable structure should:

- make the expected location of a new declaration predictable from its role;
- keep a small application small;
- scale by repetition of known shapes rather than new folder inventions;
- avoid coupling ordinary refactors to public API identity;
- give the compiler deterministic discovery without a large configuration
  language;
- separate authored, generated, secret, local, test, and deployment material;
- support the golden applications without broad escape hatches;
- reduce agent decisions measurably compared with an unconstrained layout;
- produce local, repair-oriented diagnostics for misplaced files;
- give humans and agents one deterministic project-creation command;
- record enough scaffold metadata to reproduce and safely evolve the project;
- ensure optional capabilities only add canonical shapes rather than creating
  alternative architectures;
- remain understandable without compiler implementation knowledge.

## 8. Decision sequence

1. During P8, document two or three concrete candidate trees, select a leading
   prototype, and specify the deterministic static base scaffold.
2. Before P9, fix the compiler-owned generated-output location and discovery
   boundary so runtime work does not create accidental conventions.
3. During P9, scaffold and build the seed using the prototype layout and decide
   whether evidence justifies the first additive capability pack.
4. During P10.6/P11, implement the accepted entity/query/workflow roles, apply
   them to the todo backend, and decide whether each remaining placement rule
   should be a warning, error, or convention.
5. During P12, pressure-test the decision against the order/payment application
   and TypeScript baseline.
6. Only then mark the broader workspace/package layout accepted and move its
   remaining enforcement into the project specification.

## 9. P8 candidate trees

### Candidate A — elastic single application

```text
my_application/
  .app/project.json
  app.jadpo
  tests/README.md
  build/                       # compiler-owned, ignored
```

When one file becomes difficult to navigate, the same application can grow
without changing semantic identity:

```text
my_application/
  .app/project.json
  app/
    shared.jadpo
    customers/
      model.jadpo
      actions.jadpo
      routes.jadpo
    registrations/
      model.jadpo
      actions.jadpo
      routes.jadpo
  integrations/
    invitations.jadpo
  tests/
    registrations/
  deploy/
  build/
```

This keeps the tiny case tiny and repeats feature slices in the large case.
Directories are organisational during the prototype; declarations remain in
one application namespace and retain identity when a file moves.

### Candidate B — mandatory role-first source root

```text
my_application/
  .app/project.json
  src/
    types/
    entities/
    actions/
    routes/
    integrations/
  tests/
  deploy/
  build/
```

This makes declaration roles predictable but creates ceremony for the seed and
scatters one feature across many directories. It also risks turning folders
into a second type system before module semantics exist.

### Candidate C — workspace-first deployables

```text
workspace/
  apps/
    api/
      .app/project.json
      app/
      tests/
      build/
    worker/
      .app/project.json
      app/
      tests/
      build/
  libraries/
  integrations/
```

This is the strongest multi-application shape, but adopting it as the base
would make every ordinary project pay monorepo complexity. It remains the
leading future wrapper around multiple Candidate A applications.

## 10. Prototype selection and boundaries

Candidate A is selected for P9–P11 pressure testing. `jadpo new <name>` now
creates its minimal form with five deterministic files: scaffold metadata,
`.gitignore`, `README.md`, `app.jadpo`, and `tests/README.md`. The source is a
compiling public health route. The metadata records schema version, scaffold
version, Jadpo version, project name, and the empty capability recipe.
The generated runtime also supplies the same `GET /health` contract when source
does not declare it, so hand-created and type-only projects retain a readiness
boundary. An authored public health route replaces the compiler default.

The current boundaries are explicit:

- the compiler recursively discovers authored `.jadpo` files from the project
  path;
- `build/` is always excluded from discovery and exclusively compiler-owned;
- `.app/project.json` is Jadpo scaffold metadata, not language source;
- `.local/`, `.env`, `.env.local`, and `build/` are ignored by the generated
  repository; only `.env.local` is a Jadpo local-configuration input;
- tests are authored beneath `tests/`; deployment material may later live in
  `deploy/`, but the base does not create speculative files;
- source directories have no semantic meaning yet, so moving a declaration
  does not change its semantic name;
- the scaffolder refuses a non-empty destination rather than merging with or
  overwriting authored material.

Candidate A remains historical scaffold evidence. DATA-007 supersedes its
unstructured source growth with accepted entity/query/workflow roles for the
next compiler slice. P11 and P12 still decide the enforcement level for
remaining roles, the small single-file escape, packages, and a narrow reviewed
large-application escape.
