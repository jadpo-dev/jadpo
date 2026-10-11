# Independent persistence/transaction validation

## Expected contract (before implementation inspection)

Authority: `docs/type-system.md` §§3.6,4.2,6.1–6.3;
`docs/persistence-v0.1.md` “Generated storage contract”, required mutations,
omission-aware patches and owning relationships; `docs/entity-query-model.md`
§8 (TX-001 supersedes the older prototype's implicit transaction widening).

- Nullability is intrinsic and inherited through declared field references.
  A stored field that refines a nullable field permits SQL NULL and reconstructs
  `none`; source spelling without a second `?` cannot impose NOT NULL instead.
  Non-nullable fields remain NOT NULL. Schema, metadata and runtime must agree.
- A nullable field remains required in complete record/boundary shapes. An
  optional patch may omit it without clearing the stored value; explicit `none`
  clears it. Required storage fields cannot receive either missing or null data.
- Returned database rows are validated before becoming trusted entities;
  corrupt storage must produce a contained operational fault.
- Mutations matching multiple rows fail without modifying any matching row.
- Explicit atomic boundaries roll back every provisional write on propagated
  failure. Handling a nested failure restores that action's partial writes to a
  savepoint while allowing the outer transaction to proceed.
- Independent entity actions do not silently create cross-domain atomicity.
  Existing transaction source syntax is used as implementation evidence only;
  no new transaction semantics are invented by this package.

Tests will generate isolated source/build artifacts and disposable databases.
No existing application database or inherited DATABASE_URL may be used. The
same runtime contract cases should run on SQLite and the verifier's disposable
PostgreSQL cluster, with required database setup supplied explicitly.

## Results

Added five Rust cases in
`jadpo/crates/core/tests/validation_persistence_contract.rs` and ten real-database
cases in `tests/runtime/validation-persistence.test.ts`. The same authored
Jadpo source and request/state assertions run against SQLite and PostgreSQL;
the suite compiles its source with the current compiler at runtime.

Focused evidence on 2026-09-30:

- Rust persistence contracts: **5 passed**.
- SQLite runtime contracts: **10 passed**, 65 assertions.
- Disposable PostgreSQL runtime contracts: **10 passed**, 65 assertions.
- Existing semantic unit suite after moving storage validation: **34 passed**.

```sh
cargo test --manifest-path jadpo/Cargo.toml -p jadpo-core --test validation_persistence_contract
bun --no-install --no-env-file test tests/runtime/validation-persistence.test.ts
bash tests/runtime/postgres.sh validation-persistence
```

The parent registers the suite and PostgreSQL mode in the shared verifier. The
runtime ignores ambient `DATABASE_URL`/`SQLITE_PATH`; only the dedicated
`JADPO_VALIDATION_PERSISTENCE_DATABASE_URL` from the disposable-cluster runner
selects PostgreSQL. SQLite uses a fresh temporary file. No existing database is
opened. SQL inspection is parameterized, including the deliberately hostile
text literal. Tests use UUID v4 values within the current supported decoder.

## Reproduced defects and repairs

### Inherited nullable storage was incorrectly NOT NULL

This source passed checking:

```jadpo
type Template = Object { note: Text? }
type Linked = Object { note: Template.note }
entity Entry {
    id: Uuid
    label: Text
    note: Linked.note
    identity: id
    persistence { store: primary role: authority }
}
```

Both SQL dialects emitted `"note" TEXT NOT NULL`, and the persistence manifest
claimed `nullable: false`. The two initial Rust tests both failed. The target
now consults resolved reference nullability for column DDL, persistence field
and reference metadata, and parent-relationship plans. Required `label` retains
NOT NULL; no blanket relaxation was applied.

Runtime cases now create an inherited-nullable email field through the generated
HTTP handler, inspect the actual SQL row, and read it back through a named query.
They distinguish a required nullable field from an omitted field and distinguish
patch omission from an explicit clearing `none`.

### Nullable identity/key checks ignored inherited absence

Independent sources with `id: Linked.id` refining `Uuid?`, or a relationship
pointing at a unique field refining nullable UUID, silently passed semantic
analysis. They must fail because identity and relationship keys cannot be absent.

The builder now resolves inherited nullability after collecting all declarations,
before storage and relationship validation. Identity and compound-key nullable
checks are no longer dependent on collection order or the authored suffix.
Tests retain exact identity-span and single-diagnostic expectations, including
`DATA_IDENTITY_NULLABLE` and `DATA_RELATIONSHIP_TARGET_NOT_KEY`.

### A checked local patch binding panicked in target generation

An entity action with this checked local binding reached a compiler panic:

```jadpo
var changes: EntryPatch = input.changes
return attempt update required Entry {
    where: id == input.id
    patch: changes
    empty: PatchEmpty
    missing: EntryMissing
    conflict: EntryConflict
}
```

The target's patch helper searched only callable parameters and then assumed it
had found the record. It now consumes the type checker's recorded patch-expression
type at the exact source/range, preserving lexical resolution rather than
reimplementing binding lookup. A separate runtime case shadows a narrow patch
parameter with a wider local patch in a nested block; both its additional field
and explicit null reach storage. This would be silently incomplete if lowering
used the parameter's field list.

## Transaction and integrity evidence

The runtime source uses DATA-007 entity-owned actions, named authoritative
queries and explicit atomic orchestration. It proves:

- An ambiguous required update fails before either matching row is changed.
- Propagating a nested failure rolls back both the outer and inner nullable
  inserts.
- Handling a nested failure preserves the outer insert, rolls back the inner
  insert and its derived-change journal entry, and commits the outer journal
  entry. The source explicitly declares a derived cache; no cache delivery claim
  is made.
- Persisted malformed semantic email data becomes a contained operational fault
  before trusted application code receives it; public responses expose no
  stack, driver details or database path.
- SQL-injection-shaped input remains a literal field value after storage and
  retrieval.

These are generated-handler and real-database proofs, not public-network HTTP
server tests. They do not establish arbitrary serializability, concurrent
cross-process writer isolation, deadlock retries, crash recovery, migration of
existing schemas, actual Redis delivery, distributed atomicity or exhaustive
constraint coverage. Compound uniqueness retains its existing compatibility
syntax; this package does not invent its still-open canonical spelling.
Mutation challenge and the full shared gate remain parent integration work.

