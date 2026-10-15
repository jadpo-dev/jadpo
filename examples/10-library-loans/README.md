# 10 · A library's loans

A lending library that keeps its members, its books and the loans
between them: the records a project stores, the migrations that keep its
database in step with them, the queries that read them back, the writes
that change them, and the lifecycles that hide, move along and finally
remove them.

An entity is a record with a lasting identity, on_shelf in a store; anything
never stored is a `type`. The entities describe the schema you want, and
each migration in `database/migrations/` records a step from one version
of the database to the next. `jadpo check` fails while the two disagree,
so a change to an entity always comes with its migration.

A query is one declarative read, in its own file in a domain's
`queries/` folder: which entity, which records, in what order, how many,
and the related records to bring with them. Its result type says how many
records come back, and the compiler writes the SQL and the indexes it
needs.

Every entity has `create`, `update` and `delete`, called from its
domain's actions. The compiler knows each rule a write can break, a
unique rule or a reference that refuses a delete, and asks for the
failure each one becomes; an action that finds what it read changed by
another request is run again from the start.

A lifecycle on an entity says which records normal reads see, the named
transitions that are the only way to change the fields it owns, and when
hidden records are purged for good. A member who leaves is hidden at once
and removed 30 days later; a loan goes from out to returned, and stays on
record.

## What it teaches

1. **The store.** `app.jadpo` declares no database, so the project has one called `main`, on PostgreSQL. See `app.jadpo` and [area 5, Stores](../../docs/language/05-data.md#stores).
2. **Entities and their `id`.** A member, with a unique address and a generated time. See `domains/lending/entities/member.jadpo` and [area 5, Entities and the `id`](../../docs/language/05-data.md#entities-and-the-id).
3. **References.** A loan refers to a book and a member, each saying what deleting it does: `restricted`, `cascade` and, for an optional reference, `cleared`. See `domains/lending/entities/loan.jadpo` and [area 5, References and `on_delete`](../../docs/language/05-data.md#references-and-on_delete).
4. **Unique rules and indexes.** A named rule over two fields, and an index for lookups from outside the application. See `domains/lending/entities/loan.jadpo` and [area 5, Unique rules](../../docs/language/05-data.md#unique-rules).
5. **The first migration.** The tables, keys and indexes the entities asked for when the project began, with the runtime's system tables. See `database/migrations/20261003T1000_initial/` and [area 5, Migrations](../../docs/language/05-data.md#migrations).
6. **A migration with decisions.** A required field added and another removed: the copies already stored get a value with `backfill:`, and the removed field's column is retired. See `domains/lending/entities/book.jadpo` and `database/migrations/20261020T1400_add_wear/`.
7. **Tests and the test database.** An action test runs with its own copy of a database built from the migrations, made the first time the test uses a store. See `domains/lending/actions/loan_period.test.jadpo` and [area 13, The test database](../../docs/language/13-testing.md#the-test-database).
8. **Creating and reading by id.** `Member.create { set: { ... } }` adds a record, and `Member.get(member_id)` reads one or fails with the domain's `MemberNotFound`. See `domains/lending/actions/enrol.jadpo`, `domains/lending/actions/issue_card.jadpo` and `domains/lending/failures/member_not_found.jadpo`, and [area 6, The operations every entity gets](../../docs/language/06-reads-and-writes.md#the-operations-every-entity-gets).
9. **Test data through the application's actions.** A fixture builds its records with the same actions the desk uses, so every rule applies. See `domains/lending/fixtures/ada_borrows_two.jadpo` and `domains/lending/actions/lend.test.jadpo`.
10. **A query for at most one record.** A unique field tested with `==`, returning the entity itself. See `domains/lending/queries/member_by_email.jadpo` and [area 6, Named queries](../../docs/language/06-reads-and-writes.md#named-queries).
11. **Exactly one record, with what it includes.** `missing:` names the failure when nothing matches; `include:` adds the member's card through a unique reference and their loans through `Loan.member_id`, each loan with its book, one statement per include. See `domains/lending/queries/member_loans.jadpo`, `domains/lending/types/member_page.jadpo` and [area 6, Children and parents](../../docs/language/06-reads-and-writes.md#children-and-parents).
12. **Pages and optional filters.** `Page<BookView>`, the built-in `PageSize` and `PageCursor`, and `where_present:`. See `domains/lending/queries/browse_books.jadpo`, `domains/lending/types/catalogue_filter.jadpo` and [area 6, Order and pages](../../docs/language/06-reads-and-writes.md#order-and-pages).
13. **Conditions.** `in` a list, `== none`, `and`, `or` and `not`, and a list bounded by `limit:`. See `domains/lending/queries/loans_due.jadpo` and [area 6, Filters](../../docs/language/06-reads-and-writes.md#filters).
14. **The indexes queries need.** The third migration adds the card table and the indexes the queries asked for, and drops a reference's own index once a query's index starts with its column. See `database/migrations/20261103T0900_add_cards_and_reads/` and [area 5, Indexes](../../docs/language/05-data.md#indexes).
15. **A unique rule a write can break.** `Member.email` is unique, so `enrol`'s create maps it in `conflict:` to `EmailTaken`, with its fields; `issue_card` maps a card's unique reference, and `lend` the loan's named rule. Every rule a write can break needs an arm. See `domains/lending/actions/enrol.jadpo`, `domains/lending/failures/email_taken.jadpo` and [area 6, When a write is refused](../../docs/language/06-reads-and-writes.md#when-a-write-is-refused).
16. **Updating a record read earlier.** `Loan.update(loan) { set: { ... } }` changes a loan `vouch` just read, which fails with `LoanNotFound`: no file declares it, so the compiler provides it. See `domains/lending/actions/vouch.jadpo` and [area 6, The operations every entity gets](../../docs/language/06-reads-and-writes.md#the-operations-every-entity-gets).
17. **A patch.** `patch:` changes only the fields a request supplied, telling one left out from one sent as `none`, and `when_supplied:` clears the voucher when the due date moves. See `domains/lending/actions/renew.jadpo`, `domains/lending/types/loan_changes.jadpo` and `domains/lending/actions/renew.test.jadpo`.
18. **Deleting.** `Card.delete(card)` removes a card read earlier, which frees the member to hold a new one; `Book.delete(book_id)` takes a copy out of the catalogue by its id, fails with `BookNotFound` when none has it, and is refused while a `restricted` loan refers to it, `BookHasLoans`. See `domains/lending/actions/revoke_card.jadpo`, `domains/lending/actions/withdraw_copy.jadpo` and their tests.
19. **A counter.** `lend` counts each loan with an update by `id` whose value reads the stored one, `Book.times_lent + 1`: the database works it out in one step, so two desks lending at once both count. The fourth migration adds the count, starting existing copies at 0. See `domains/lending/actions/lend.jadpo`, `database/migrations/20261110T0900_count_lending/` and [area 6, Counters and other one-step updates](../../docs/language/06-reads-and-writes.md#counters-and-other-one-step-updates).

20. **A lifecycle that hides records.** A member's `visible: left_at == none` leaves a member who has left out of every query, and `Member.get`, updates and deletes by `id` find nothing. `Member.delete(member_id)` runs the lifecycle's `delete` transition, which sets `left_at` and keeps the row; their loans stay visible, since visibility never spreads. See `domains/lending/entities/member.jadpo`, `domains/lending/actions/leave.jadpo` and its test, and [area 5, Lifecycle](../../docs/language/05-data.md#lifecycle).
21. **Purge.** `purge: { after: 30d since: left_at }` removes a member for good 30 days after they left, with the card and loans that `cascade` from them, in small batches: the compiler's own operation, which the built-in hourly schedule runs once schedules come. See `domains/lending/entities/member.jadpo`.
22. **Transitions and what a lifecycle owns.** A loan starts `out` from `initial:`, and its transitions are the only way to move it on: `Loan.check_in(loan)` marks one read earlier returned, and `Loan.declare_lost(loan_id)` marks one lost by its id, failing with `LoanNotFound` when none has it. Each `when:` allows it only while the loan is out, so checking in twice is refused, and each caller maps that refusal in `conflict:`, keyed by the transition, to `LoanNotOut`. No code sets `status` or `returned_at` any other way, and with no `delete` transition no code deletes a loan. See `domains/lending/entities/loan.jadpo`, `domains/lending/actions/return_book.jadpo`, `domains/lending/actions/declare_lost.jadpo` and their tests, and [area 6, The operations every entity gets](../../docs/language/06-reads-and-writes.md#the-operations-every-entity-gets).
23. **A partial index.** `members_named` reads only members still in the library, so the fifth migration creates its index `WHERE "left_at" IS NULL`, and backfills every loan already stored as `out`. See `domains/lending/queries/members_named.jadpo` and `database/migrations/20261110T0901_add_lifecycles/`.

## Run it

From the repository root, build `jadpo` once, then run each command on
this project; `jadpo migrate` works on the project it runs in:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/10-library-loans
target/release/jadpo test examples/10-library-loans
target/release/jadpo fmt --check examples/10-library-loans
cd examples/10-library-loans && ../../target/release/jadpo migrate status
../../target/release/jadpo sql member_loans
../../target/release/jadpo sql return_book
```

- `check` ends with `checked: <n> files` and no errors: the entities match
  the latest migration's snapshot.
- `migrate status` lists the five migrations. Given a PostgreSQL database
  with `--url`, `jadpo migrate apply` creates the tables there and `status`
  says which migrations it has applied.
- `test` ends with `tested: 19 passed`. A test that writes records gets its
  own database, copied from a template the migrations build, on the server
  `JADPO_TEST_DATABASE_URL` names or one `jadpo test` starts from the
  machine's PostgreSQL.
- `fmt --check` ends with `formatted: <n> files`.
- `sql member_loans` prints the four statements the query sends, each
  with the parameters that fill it: the member, then one for the card,
  one for the loans and one for their books, however many loans there
  are; members who have left are left out of the first. `sql
  return_book` prints the transition's `UPDATE`, which changes the loan
  only while it is out, and the statement that asks, when it changes
  nothing, whether the loan is there, so returning it twice is
  `LoanNotOut`.

## Try next

- In `loan.jadpo`, delete the block after `book_id: Book.id`. `jadpo check` reports `data.on_delete_required`: a reference must say what deleting the book does.
  <!-- try {"file": "domains/lending/entities/loan.jadpo", "replace": "    book_id: Book.id {\n        on_delete: OnDelete.restricted\n    }\n", "with": "    book_id: Book.id\n", "check": ["data.on_delete_required"]} -->
- In `member.jadpo`, add `indexes: { by_name: [name] }` after the fields. `jadpo check` reports `migration.out_of_date` and names the command that writes the migration for the new index.
  <!-- try {"file": "domains/lending/entities/member.jadpo", "replace": "    joined_at: Instant {\n        generated: Generated.on_create\n    }\n", "with": "    joined_at: Instant {\n        generated: Generated.on_create\n    }\n\n    indexes: {\n        by_name: [name]\n    }\n", "check": ["migration.out_of_date"]} -->
- In `member_loans.jadpo`, change `limit: 50` to `limit: 0`. `jadpo check` reports `query.limit_range`: a list holds at least one record.
  <!-- try {"file": "domains/lending/queries/member_loans.jadpo", "replace": "limit: 50", "with": "limit: 0", "check": ["query.limit_range"]} -->
- In `browse_books.jadpo`, sort by `Book.wear` instead of `Book.title`. `jadpo check` reports `query.order_field`: the database keeps an enum as its variant's name, so it can't sort copies in the enum's order.
  <!-- try {"file": "domains/lending/queries/browse_books.jadpo", "replace": "order: [Book.title asc]", "with": "order: [Book.wear asc]", "check": ["query.order_field"]} -->
- In `member_by_email.jadpo`, change `==` to `!=`. `jadpo check` reports `query.not_unique`: `Member?` is at most one record, and every other address could match many.
  <!-- try {"file": "domains/lending/queries/member_by_email.jadpo", "replace": "where: Member.email == email", "with": "where: Member.email != email", "check": ["query.not_unique"]} -->
- In `lend.jadpo`, delete the `conflict:` block after the loan's `set:`. `jadpo check` reports `data.conflict_unmapped`: lending a copy twice on one day breaks `Loan.one_loan_per_due_date`, and something must say what the caller gets.
  <!-- try {"file": "domains/lending/actions/lend.jadpo", "replace": "        conflict: {\n            Loan.one_loan_per_due_date: BookAlreadyLent\n        }\n", "with": "", "check": ["data.conflict_unmapped"]} -->
- In `lend.jadpo`, change `Book.times_lent + 1` to `book.times_lent + 1`. `jadpo check` reports `data.update_by_id_value`: an update by `id` adds to the stored count, never to a copy read earlier, which another desk may have changed since.
  <!-- try {"file": "domains/lending/actions/lend.jadpo", "replace": "Book.times_lent + 1", "with": "book.times_lent + 1", "check": ["data.update_by_id_value"]} -->
- In `withdraw_copy.jadpo`, change `Loan.book_id: BookHasLoans` to `Loan.member_id: BookHasLoans`. `jadpo check` reports `data.conflict_cannot_break`: a loan's member doesn't stop a book leaving, and the error names `Loan.book_id`, the reference that does.
  <!-- try {"file": "domains/lending/actions/withdraw_copy.jadpo", "replace": "Loan.book_id: BookHasLoans", "with": "Loan.member_id: BookHasLoans", "check": ["data.conflict_cannot_break"]} -->
- In `vouch.jadpo`, add `returned_at: clock.now` to the update's `set:`. `jadpo check` reports `data.lifecycle_owned_field`: only the lifecycle's `check_in` marks a loan returned, and the next step says to call it.
  <!-- try {"file": "domains/lending/actions/vouch.jadpo", "replace": "            vouched_by: voucher.id\n", "with": "            vouched_by: voucher.id\n            returned_at: clock.now\n", "check": ["data.lifecycle_owned_field"]} -->
- In `return_book.jadpo`, delete the `conflict:` block. `jadpo check` reports `data.conflict_unmapped`: `check_in` is refused for a loan already returned, and something must say what the caller gets.
  <!-- try {"file": "domains/lending/actions/return_book.jadpo", "replace": "    return attempt Loan.check_in(loan) {\n        conflict: {\n            Loan.check_in: LoanNotOut\n        }\n    }\n", "with": "    return attempt Loan.check_in(loan)\n", "check": ["data.conflict_unmapped"]} -->
- In `member.jadpo`, change `visible: left_at == none` to `visible: Member.left_at == none`. `jadpo check` reports `data.lifecycle_condition`: a lifecycle's conditions are about the record they are written in, so they name its fields bare.
  <!-- try {"file": "domains/lending/entities/member.jadpo", "replace": "visible: left_at == none", "with": "visible: Member.left_at == none", "check": ["data.lifecycle_condition"]} -->
- That is the last example. The [language pages](../../docs/language/README.md) hold the full specification.
