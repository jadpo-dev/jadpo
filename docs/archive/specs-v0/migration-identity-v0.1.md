# Migration identity v0.1

**Status:** provisional contract; rename-stable registry, bound decisions, and non-executable adapter review plans implemented  
**Scope:** rename-stable persistent schema identity and change classification  
**Not yet claimed:** generated migration SQL or safe execution against existing data

## 1. Purpose

A source name is readable, but it is not durable identity. Treating
`entity Todo` renamed to `Task` as “drop Todo, create Task” risks data loss;
guessing that an unrelated addition is a rename is equally unsafe. Migration
planning therefore needs identity that survives spelling and physical-name
changes without hiding lifecycle decisions in heuristics.

## 2. Authoritative registry

Migration-aware projects check in a compiler-owned `schema.identities.json` at
the project root. When present, the registry is validated by `check`, `inspect`,
`artifacts`, and `build`. The registry is not a disposable file beneath
`build/`.

The registry assigns immutable IDs to persistent entities, fields, named
constraints, and generated indexes. Each entry separately records:

- immutable identity;
- current semantic path, such as `Todo.owner_id`;
- current physical table/column/constraint name;
- kind and owning identity;
- prior names created only by an explicit rename operation.

Initial IDs deterministically encode the registry version, kind, and initial
semantic path. Once written, an ID never changes because its name changes. IDs
identify declarations; they are not security boundaries or database names.

## 3. Operations

The implemented tooling surface is deliberately explicit:

```text
jadpo schema init <project>
jadpo schema check <project>
jadpo schema add <project>
jadpo schema rename <project> entity Todo Task
jadpo schema rename <project> field Todo.owner_id Todo.assignee_id
jadpo schema snapshot <project> <output>
jadpo schema diff <project> --against <snapshot>
jadpo schema decision-template <project> --against <snapshot> <output>
jadpo schema decision-check <project> --against <snapshot> <artifact>
jadpo schema plan <project> --against <snapshot> --decisions <artifact> --adapter <postgres|sqlite> <output>
jadpo schema sql <project> --against <snapshot> --decisions <artifact> --adapter <postgres|sqlite> <output>
```

`schema init` refuses to overwrite an existing registry. `schema add` registers
only declarations newly present in checked source and refuses to proceed if any
registered declaration disappeared, preventing a rename or removal from being
laundered as an addition. Rename operations
resolve one exact current identity, update the semantic path, retain history,
and do not silently rename the physical database object. A later migration plan
must explicitly choose whether to preserve or rename that physical name.

Manual source renames without a matching registry update fail with a diagnostic
that reports the unmatched removal and addition. The compiler may suggest a
rename command, but similarity is never enough to apply one.

`schema snapshot` writes an immutable canonical checked-schema snapshot and
refuses to overwrite it. In addition to registry identity, each snapshot records
field nominal type, nullability, identity/unique/index flags, intrinsic
constraints, relationship target/name/delete lifecycle, and compound
constraint/index members. `schema diff` compares current checked shape against
that explicit snapshot. The output always contains `migration_plan: false`; it
does not contain SQL or approve lifecycle effects.

## 4. Change classification

Given a prior snapshot plus current checked source and registry, the compiler
now classifies identity changes as:

- identity-preserving source rename;
- physical rename requested or physical name preserved;
- additive entity, nullable field, index, or compatible constraint;
- representation/constraint change requiring existing-data validation;
- removal requiring lifecycle disposition;
- ambiguous/unmatched state, which blocks planning.

The implemented change set distinguishes `added`, `removed`,
`logical_rename`, `physical_rename`, `logical_and_physical_rename`, and
`incompatible_identity_change`. It assigns conservative dispositions such as
`requires_shape_analysis`, `requires_schema_plan`, or
`requires_lifecycle_decision`. A logical rename with the same physical name is
reported as `identity_preserved`.

Field additions are classified as `added_nullable_field` or
`added_required_field`; required additions demand an existing-data decision.
A required-to-nullable change with every other shape component unchanged is
classified as `field_nullability_widened` with `requires_schema_plan`; it cannot
invalidate existing rows and needs no existing-data decision. Every other
change to a field's recorded shape remains conservatively classified as
`field_shape_changed` with `requires_existing_data_decision`, except that the
inverse nullable-to-required change is classified as
`field_nullability_narrowed`. Narrowing admits `validate_existing` or a staged
transition at the decision boundary; reviewed SQL currently accepts only
`validate_existing` with the exact compiler-owned `not_null` predicate. This
can later be refined into other backfills, validation scans, staged constraints,
or representation migrations without weakening the default.

A reference `on_delete` change is classified separately as
`relationship_lifecycle_changed`. Every change requiring existing-data or
lifecycle judgment now carries a typed `decision_requirement` whose status is
`unresolved`. The requirement enumerates admissible strategies and their typed
evidence slots—for example `backfill` requires a `typed_expression`, while
`destructive_remove` requires an `approval_reference`. This fail-closed
interface is consumed by the authored decision artifact; it is not an inferred
decision or an approval.

`schema decision-template` now writes a non-overwriting artifact containing the
entire canonical change set as its exact binding plus one unresolved entry for
every blocker. The author selects an admissible strategy and supplies named,
non-empty evidence values. `schema decision-check` regenerates the change set
and rejects stale bindings, missing or duplicate entries, invalid strategies,
and missing, unexpected, duplicate, or empty evidence. A successful result
means only that the decision boundary is complete and current; it remains
marked `migration_plan: false`.

`schema plan` accepts only a complete, current decision artifact. It writes an
immutable ordered review plan for PostgreSQL or SQLite, selects adapter-specific
operations (including SQLite table rebuilds where PostgreSQL can alter a
constraint or column), classifies rollback as automatic, conditional, or
unavailable, and lists every irreversible step. A `reject` lifecycle decision
prevents plan creation because the checked source change cannot simultaneously
be implemented and rejected. Evidence values are carried into the review plan
but are not yet parsed as source expressions, so every plan is explicitly
`executable: false` and `sql_generated: false`.

`schema sql` is a narrower reviewed-output gate. It currently compiles
primitive `literal(...)` backfills by nominal field type, escapes identifiers
and text values, and emits forward/rollback SQL for nullable column additions
on both adapters and required-column additions with backfill on PostgreSQL.
The artifact remains `executable: false` and `review_required: true`. A bounded
SQLite path coalesces compatible primitive field additions on each unrenamed
table into one rebuild: shadow table, typed copy/backfill, nullable `NULL`
population, swap, index recreation, rollback rebuild, transaction boundaries,
and `foreign_key_check`. Existing scalar references retain their target, delete
action, and generated lookup index; explicit indexes and named compound
uniqueness that excludes every added field are also reconstructed. Constraints
on added fields, transforms, removals, lifecycle changes, nominal refinements,
and other unsupported changes fail closed. Required-to-nullable widening emits
`DROP NOT NULL` plus a restoring PostgreSQL rollback; SQLite emits a reversible
rebuild whose rollback restores the required column.
Nullable-to-required narrowing is also bounded: PostgreSQL emits a null-count
review and `SET NOT NULL`, while SQLite copies into a required shadow-table
column. Both paths require exact-bound `not_null` evidence and roll back to a
nullable column; arbitrary predicate strings do not compile to review SQL.

The classifier must emit machine-readable evidence connecting every change to
the semantic node, registry ID, prior/current names, affected dependants, and
required decision.

## 5. Existing-data and lifecycle rules

No migration is generated merely because a declaration disappeared. Removal,
new required fields, stricter constraints, representation changes, and
relationship lifecycle changes block until the author supplies an explicit
plan for existing rows. Valid decisions include backfill from a typed source,
retain/preserve, anonymise, staged transition, or destructive removal with the
required human approval. The compiler never invents a default value or treats
an empty development database as evidence about production.

Foreign-key and index identities derive from declaration identities rather than
current spellings. This permits readable generated names while allowing the
diff engine to distinguish a rename from constraint replacement.

## 6. Determinism and trust boundary

Registry serialization is canonical and byte-stable. Duplicate IDs, duplicate
current paths, wrong owners, kind changes, unknown entries, missing live
declarations, and source/registry drift are hard errors. Build-local semantic
graph ordinals remain deterministic but are not migration identities.

The compiler owns registry mutation. Agents may propose schema operations, but
they do not hand-edit IDs or rewrite history. Code review sees the source change,
registry change, classified diff, lifecycle decisions, and generated plan
together.

## 7. Implementation sequence

1. Define registry version 1 and canonical serialization. **Implemented.**
2. Add deterministic, non-overwriting `schema init`. **Implemented.**
3. Parse and validate registry/source agreement when a registry exists.
   **Implemented.**
4. Register additions while rejecting concurrent removals/renames.
   **Implemented.**
5. Add explicit entity and field rename operations with collision checks.
   **Implemented.**
6. Emit a machine-readable identity-aware change set from two snapshots.
   **Implemented.**
7. Extend snapshots with field type, nullability, constraints, references, and
   delete lifecycle. **Implemented.**
8. Add typed existing-data/lifecycle decisions.
   - Typed unresolved requirements and admissible strategy/evidence contracts:
     **Implemented.**
   - Authored decision artifacts, validation, and exact stale-change binding:
     **Implemented.**
9. Generate adapter-specific migration output only after the change set is fully
   decided, with rollback and irreversible-step metadata.
   - Ordered PostgreSQL/SQLite non-executable review plans: **Implemented.**
   - Primitive literal compilation and additive forward/rollback SQL review:
     **Implemented.**
   - Bounded SQLite compatible multi-addition rebuild, including independent
     typed backfills, nullable additions, and preservation of existing scalar
     references, indexes, and compound uniqueness:
     **Implemented.**
   - Reversible required-to-nullable PostgreSQL and SQLite review SQL:
     **Implemented.**
   - Evidence-checked nullable-to-required PostgreSQL and SQLite review SQL:
     **Implemented.**
   - Other transforms, removals, and lifecycle SQL: deferred/fail closed.

The implemented identity steps prove rename continuity. They do not by
themselves claim a safe migration system.
