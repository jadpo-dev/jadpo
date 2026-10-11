# Fixture, migration and build-state closure — 2026-09-30

## Contract authority

Expectations were derived from `docs/time-testing-plan.md` TEST-D01/D05 and
§4.2, `docs/persistence-v0.1.md`, and `docs/migration-identity-v0.1.md` §§2–6
before inspecting implementation:

- top-level authored tests start from independent real-adapter state, including
  after failures, while changes remain visible inside an individual test;
- explicit identity operations preserve IDs through source renames; similarity
  and `schema add` cannot silently reinterpret a removal as a rename;
- existing-data decisions bind the exact change set and remain separate from
  executable migration approval;
- registry, snapshot and review artifacts are non-overwriting evidence;
- failed output staging retains the previous complete build and persistent
  source-owned schema identity evidence.

The migration contract explicitly does **not** claim generally safe execution
against arbitrary existing data. Its supported SQL output is a reviewed artifact
marked `executable: false` and `review_required: true`. This package executes
only those reviewed statements in disposable SQLite databases; it does not add
or endorse a deployment migration runner.

## Executable evidence

`jadpo/crates/core/tests/validation_migrations_contract.rs`: **8 tests passed**.

```sh
cargo test --manifest-path jadpo/Cargo.toml -p jadpo-core --test validation_migrations_contract
```

The tests independently exercise:

1. Registry initialization and snapshot creation refusing overwrite while
   retaining byte-identical prior evidence.
2. A source-only rename failing registry validation and `schema add`, with the
   old registry unchanged.
3. Successive explicit entity and field renames retaining every registry ID,
   owning ID and physical name, recording history, and classifying the changes
   as identity-preserving renames rather than additions/removals.
4. Unresolved required-field decisions producing neither plan nor SQL output.
5. A previously complete decision becoming stale after another shape change.
6. A complete authored decision containing `clock.now` still being insufficient
   for reviewed SQL: the ordered plan is non-executable and SQL generation
   rejects the unsupported expression.
7. Reviewed SQL refusing to overwrite an existing review artifact.
8. A write failure after partially staging a new build preserving the old app,
   old metadata, registry and snapshot; subsequent successful emission removes
   obsolete build artifacts while retaining the registry.

`tests/runtime/validation-fixtures-migrations.test.ts`: **5 tests passed**.

```sh
bun test tests/runtime/validation-fixtures-migrations.test.ts
```

The fixture source inserts a row and intentionally fails one assertion. Later
and reordered tests reuse the same identity in an initially empty database,
while another test proves that inserted state stays visible within that test.
Repeated runs, concurrent invocations of this database-only fixture suite, and
a fresh process all retain the expected two successes and one intentional
failure. A separate application SQLite database contains a sentinel with the
same identity throughout; it remains untouched. The host regression tests pass
only when that intentional failure is correctly reported and isolated.

The migration runtime cases use compiler-generated initial schemas and reviewed
SQL. One required-plus-nullable addition rebuild preserves row IDs, foreign-key
relationships, cascade behaviour, lookup indexes and compound uniqueness. Its
backfill contains apostrophes and SQL-shaped text, which must remain literal
data. The rollback review removes the new columns and restores the old schema
without losing existing rows.

A nullable-to-required review fails on an existing null. The explicit test
executor catches that SQL error, rolls back the open SQLite transaction and
restores foreign-key checking. Assertions establish unchanged rows/schema and
no leaked shadow table. After repairing the fixture's row, forward SQL succeeds,
rejects new nulls, and rollback SQL restores nullable behaviour.

No production defects were reproduced in this bounded package. No production
source files or shared manifests were changed by this worker. The parent owns
suite registration and common-gate evidence.

## Limits and contract gaps

- Failure rollback above includes the test executor's explicit `ROLLBACK`; it
  does not claim that review SQL autonomously handles errors or is deployable.
- SQLite evidence does not establish PostgreSQL migration parity, arbitrary
  transforms, inbound-reference rebuilds, historical timestamp backfills,
  irreversible removal, crash recovery or a general migration execution system.
- Parallel evidence applies to these database-only authored fixtures. It does
  not prove concurrent mutable configuration/clock/capability isolation.
- The fixture source uses supported Text identity construction and exhaustive
  `none`/`some` matches. `Uuid` field construction from a string and direct
  `nullable == none` assertions were not accepted by the current compiler; they
  were not needed to establish fixture isolation and were not treated as new
  language requirements here.
- This package does not inject process crashes during artifact promotion or
  claim to prove filesystem transactionality under concurrent writers.

## Diagnostic closure follow-up

The syntax-aware catalogue scan exposed previously uncounted CONFIG/POLICY
families. The new core integration suites execute real triggers rather than
catalogue renderings:

- `validation_diagnostic_policy.rs`: **16 tests passed**, exercising all 17
  newly uncovered policy codes. The cases enforce POLICY-D04–07/D12/D19/D22:
  qualified immutable role bindings, unique membership/policy/scope/subject
  declarations, complete membership settings, closed unique effect lists,
  authoritative role sources, reachable scope relationships and actual named
  operation effects. Corrected source must compile cleanly. Source errors have
  bounded nonempty spans, exact excerpts where the current emitter identifies
  the offending item, useful next steps and no automatic public-access repair.
- `validation_diagnostic_config.rs`: **6 tests passed**, exercising unknown
  fixture/local fields, duplicate fixture values, omitted required fixture
  values, missing local values and invalid local values. Checked defaults are
  accepted; rejected setter values preserve the previous file byte-for-byte;
  malformed stored values cannot become runtime environment values; corrected
  values succeed. Local files live only in unique disposable directories.

Evidence: `/private/tmp/jadpo-diagnostic-policy-config.log`. Every requested code
was reachable through authored source or a public core API. No production
changes were made. Duplicate membership/policy parser diagnostics currently
point immediately after the second declaration (often the containing `}`),
so those cases establish a usable bounded span but do not claim precise
highlighting of the duplicate block. This location-quality limitation was
reported to the parent rather than hidden by an exact-location assertion.

## Independent review: configuration write race

Review of the new local-configuration diagnostic tests found a real remaining
write race. `set_local_configuration` compared the original file before staging,
then wrote and synced the replacement before renaming it without another check.
A disposable public-API probe waited for its staging file, edited `.env.local`,
and observed the setter return success while discarding the concurrent edit.
The original helper-only revision test could not detect this integration gap.

With parent authorization, the bounded fix rechecks expected bytes and rejects
an unsafe target immediately before promotion. On rejection it removes only the
staging file it created. Deterministic tests inject an edit after staging, a
newly created destination after staging, and a foreign staging-file collision;
they require preserved external data, the intended diagnostic, and appropriate
cleanup. The original revision diagnostic test remains. All four focused unit
tests pass. Public-API reproduction source and raw before/after evidence are
retained under ignored `build/validation/config-review-probe/`. Against the fixed
library the same public probe reports `setter=error` and
`concurrent_value_preserved=true`; all ten configuration integration cases also
pass. Focus logs are `/private/tmp/jadpo-config-staged-write.log` and
`/private/tmp/jadpo-config-staged-integration.log`.

This does not implement atomic compare-and-swap against arbitrary manual
writers: a final check/rename window remains. No lock protocol or broader
filesystem-transaction guarantee is claimed.

## Independent review: diagnostic scanner boundaries

Read-only probe evidence in `build/validation/scanner-review-probe/` showed that
ignored tests and never-called nested function bodies are counted as test
references, while compound `cfg(all(test, unix))` test modules are counted as
production emitters. These exact edge cases were reported to the parent, who
is integrating scanner regression tests and corrections. No current ignored or compound-test-cfg occurrence was found, so the
review does not claim that the present suite's execution evidence is false.
Controlled CLI runtime-launcher tests accurately establish process status
mapping; they do not independently establish authored-test runtime semantics,
which are tested by the separate fixture runtime suite above.
