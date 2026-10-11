# Independent migration mutation challenge

Reviewed another author's `validation-fixtures-migrations.test.ts`, specifically
“reviewed additive SQLite rebuild preserves data, identities, foreign keys and
unique indexes”. The accepted migration evidence supplies a literal backfill;
executing its reviewed SQL on a disposable database must preserve the literal's
exact content, including apostrophes and SQL-shaped text.

A separate `/private/tmp` source copy was compiled with its own Cargo target.
The shared production tree was never mutated. The challenge replaced SQL
apostrophe doubling with apostrophe removal in `sql_string_literal` within
`migration_identity.rs`. This mutant still compiles and emits executable SQL;
it corrupts the requested value instead of merely making SQL unparsable.

| Run | Build | Targeted regression |
|---|---|---|
| Baseline | pass | pass |
| Mutant | pass | fails exact stored-value assertion |
| Restored source | pass | pass |

The failing assertion observed `OReilly); DROP TABLE parent; --` instead of
`O'Reilly'); DROP TABLE parent; --` in the migrated row. Both original and
restored source hashes match. The other four tests in the file were explicitly
filtered for this bounded challenge; they are not counted as reviewed mutations.

Evidence, logs and the replay script are retained under
`build/validation/migration-mutation-review/jadpo-migration-mutation-2_j7v5sw/`.
This proves one meaningful regression detector, not comprehensive migration,
SQL-injection, fixture-isolation or mutation coverage. No real application
database, deployed migration or shared source was changed.
