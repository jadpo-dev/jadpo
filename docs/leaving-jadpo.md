# Leaving Jadpo

What a project keeps if it stops using Jadpo: its database, its API's
description, the licence of the code it runs, and where that code comes
from. Each claim names the test or check in the gate
(`python3 tools/verify.py`) that shows it, so a claim no test shows isn't
here.

## Your database is plain PostgreSQL

Jadpo keeps your records in ordinary PostgreSQL tables that any tool can
read and write. Each migration's schema change is a SQL file,
`database/migrations/<name>/postgres.sql`, which PostgreSQL's own `psql`
applies to an empty database.

- **A table for each entity**, named after it in `lower_snake_case`:
  `Todo` is `todo`.
- **A column for each field**, of PostgreSQL's own type for it: `Text` is
  `text`, `Instant` is `timestamptz`, an enum is `text` holding the
  variant's name. [Storage types](language/05-data.md#storage-types) has
  the whole list.
- **Keys, required fields and references are the database's own**
  constraints: primary keys, `NOT NULL`, unique constraints and foreign
  keys.
- **The runtime's own tables start with `jadpo_`**, such as `jadpo_outbox`
  for events not yet delivered
  ([System tables](language/05-data.md#system-tables)). A program that
  only reads your records can leave them alone.
- **Nothing of Jadpo's runs inside the database.** The golden todo's
  migrations add no function, trigger or extension. An app that stores
  geometries gets PostGIS, PostgreSQL's own extension for them
  ([Storage types](language/05-data.md#storage-types)).

Shown by `crates/jadpo-cli/tests/leaving.rs`. It applies the golden todo's
committed migrations (`benchmarks/golden-todo/`) with `psql` alone, reads
the tables, columns and constraints back from PostgreSQL's catalogue, and
checks the database has no new function, trigger or extension. Then a
project's seed writes records through its actions, and `psql` reads them
back with a plain `SELECT`.

Some of what your app means lives in its Jadpo code, not in the database,
so another program reading the tables has to apply it itself:

- **Each field's own rules**, such as a title's longest length. The
  database holds the type and whether the field is required; the app
  checks the rest ([Storage types](language/05-data.md#storage-types)).
- **Who may read and change each record.** Policy is part of every
  statement the app sends, and a program with its own database login
  isn't held to it.
- **An encrypted field** holds sealed bytes, and only the app's key opens
  them. Shown by
  `a_sealed_field_round_trips_and_its_column_holds_only_sealed_bytes` in
  `crates/jadpo-cli/tests/encrypted_fields.rs`.

## Your API is described in OpenAPI 3.1

`jadpo check` writes an OpenAPI 3.1 document for each API, to
`build/openapi/<api>.json`, and the app serves it at
`<prefix>/openapi.json` ([OpenAPI](language/07-http.md#openapi)). Any
OpenAPI tool can read it to generate a client, test the API or write a
server that answers the same routes.

- **It is valid OpenAPI 3.1**, by the specification's own JSON Schema.
  Shown by `tools/checks/openapi_valid.py`: it checks every document the
  golden todo and the teaching examples write, with a validator pinned in
  `tools/fixtures/openapi/`.
- **It is what `jadpo check` writes.** Shown by
  `the_openapi_document_matches_its_golden_copy` in
  `crates/jadpo-cli/tests/http.rs`, which holds a project's document to
  its committed copy.
- **The app serves the same document, byte for byte.** Shown by
  `crates/jadpo-cli/tests/served_openapi.rs`.

The document also carries a few fields of Jadpo's own, such as
`x-jadpo-rate-limit`. OpenAPI allows extra fields named `x-`, and the
validator accepts them.

## The licence

Jadpo's code, the compiler and the runtime alike, is under the
[Apache License 2.0](../LICENSE). Its documentation is under
[CC BY 4.0](LICENSE.md).

A program `jadpo build` makes compiles Jadpo's runtime crates and the
crates they depend on.

- **Every Jadpo crate it compiles is Apache 2.0.** Shown by
  `every_carried_crate_is_apache_2_0` in
  `crates/jadpo-cli/src/runtime_source.rs`, which reads the manifests of
  the crates `jadpo` carries for a build.
- **Every other crate is under a permissive licence**, such as MIT or
  BSD, but two: `cssparser` and `dtoa-short`, which the HTML sanitiser
  brings, are under the Mozilla Public License 2.0. It applies file by
  file: if you change one of those crates' files and ship the program,
  you share that file's changes under the same licence. Jadpo uses them
  unchanged. Shown by `cargo deny check`, a step of the
  gate, which holds every dependency to the licences `deny.toml` lists.

## Where the runtime comes from

The `jadpo` binary carries the runtime's source with it. `jadpo build`
writes your app's Rust into `build/rust/<deployable>/` and compiles it
with that source. It fetches two things the first time, each checked
before it is used:

- **The Rust toolchain**, from Rust's own distribution server. The
  release pins its version and the SHA-256 of its manifest, and each
  download is checked against that manifest.
- **The crates the runtime depends on**, from crates.io, each checked
  against the checksum in the `Cargo.lock` the release carries.

Shown by
`a_released_jadpo_fetches_its_toolchain_and_builds_both_targets_outside_this_checkout`
in `crates/jadpo-cli/tests/released.rs`. It copies a built `jadpo` out of
the checkout and builds the golden todo with no Rust installed, against a
stand-in for each of the two servers, and the program it builds runs.
The tests in `crates/jadpo-cli/src/runtime_source.rs` check the carried
source is the runtime's crates and nothing else.

How a build is pinned is in
[Compiler and runtime architecture](compiler-runtime.md), "Determinism and
the build stamp".
