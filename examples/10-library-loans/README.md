# 10 · A library's loans

A lending library that keeps its members, its books and the loans
between them: the records a project stores, and the migrations that keep
its database in step with them.

An entity is a record with a lasting identity, kept in a store; anything
never stored is a `type`. The entities describe the schema you want, and
each migration in `database/migrations/` records a step from one version
of the database to the next. `jadpo check` fails while the two disagree,
so a change to an entity always comes with its migration.

## What it teaches

1. **The store.** `app.jadpo` declares no database, so the project has one called `main`, on PostgreSQL. See `app.jadpo` and [area 5, Stores](../../docs/language/05-data.md#stores).
2. **Entities and their `id`.** A member, with a unique address and a generated time. See `domains/lending/entities/member.jadpo` and [area 5, Entities and the `id`](../../docs/language/05-data.md#entities-and-the-id).
3. **References.** A loan refers to a book and a member, each saying what deleting it does: `restricted`, `cascade` and, for an optional reference, `cleared`. See `domains/lending/entities/loan.jadpo` and [area 5, References and `on_delete`](../../docs/language/05-data.md#references-and-on_delete).
4. **Unique rules and indexes.** A named rule over two fields, and an index for lookups from outside the application. See `domains/lending/entities/loan.jadpo` and [area 5, Unique rules](../../docs/language/05-data.md#unique-rules).
5. **The first migration.** The tables, keys and indexes the entities asked for when the project began, with the runtime's system tables. See `database/migrations/20261003T1000_initial/` and [area 5, Migrations](../../docs/language/05-data.md#migrations).
6. **A migration with decisions.** A required field added and another removed: the copies already stored get a value with `backfill:`, and the removed field's column is retired. See `domains/lending/entities/book.jadpo` and `database/migrations/20261020T1400_add_wear/`.
7. **Tests and the test database.** An action test runs with its own copy of a database built from the migrations, made the first time the test uses a store. See `domains/lending/actions/loan_period.test.jadpo` and [area 13, The test database](../../docs/language/13-testing.md#the-test-database).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project; `jadpo migrate` works on the project it runs in:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/10-library-loans
target/release/jadpo test examples/10-library-loans
target/release/jadpo fmt --check examples/10-library-loans
cd examples/10-library-loans && ../../target/release/jadpo migrate status
```

- `check` ends with `checked: <n> files` and no errors: the entities match
  the latest migration's snapshot.
- `migrate status` lists the two migrations. Given a PostgreSQL database
  with `--url`, `jadpo migrate apply` creates the tables there and `status`
  says which migrations it has applied.
- `test` ends with `tested: 1 passed`. A test that writes records gets its
  own database, copied from a template the migrations build, on the server
  `JADPO_TEST_DATABASE_URL` names or one `jadpo test` starts from the
  machine's PostgreSQL.
- `fmt --check` ends with `formatted: <n> files`.

## Try next

- In `loan.jadpo`, delete the block after `book_id: Book.id`. `jadpo check` reports `data.on_delete_required`: a reference must say what deleting the book does.
  <!-- try {"file": "domains/lending/entities/loan.jadpo", "replace": "    book_id: Book.id {\n        on_delete: OnDelete.restricted\n    }\n", "with": "    book_id: Book.id\n", "check": ["data.on_delete_required"]} -->
- In `member.jadpo`, add a field `phone: Text?`. `jadpo check` reports `migration.out_of_date` and names the command that writes the migration, `jadpo migrate new add_phone`.
  <!-- try {"file": "domains/lending/entities/member.jadpo", "replace": "    name: Nickname\n", "with": "    name: Nickname\n    phone: Text?\n", "check": ["migration.out_of_date"]} -->
- In the second migration's `changes.jadpo`, change `Wear.pristine` to `Wear.mint`. `jadpo check` reports `migration.decision_invalid`: the backfill must be a value of the field's type.
  <!-- try {"file": "database/migrations/20261020T1400_add_wear/changes.jadpo", "replace": "backfill: Wear.pristine", "with": "backfill: Wear.mint", "check": ["migration.decision_invalid"]} -->
- That is the last example. The [language pages](../../docs/language/README.md) hold the full specification.
