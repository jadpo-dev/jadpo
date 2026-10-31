# 25 · Allotment rents

Allotment societies keep their plots here, each society's committee
seeing only its own site. Once a day the application looks for rent
that has gone unpaid past its due date, across every society, and
reminds each plot's holder; once a year a society's treasurer charges
every plot its rent. It teaches schedules, and how background work
meets tenants: a schedule reaches every society, a subscriber and a
batch only one.

A schedule is a timer that runs one action. It does no slow work itself:
it finds what is due, marks it, and emits an event per item, so each
reminder is its own delivery, retried on its own, within its own
society. Charging the rent is a batch, as in
[24-seed-library](../24-seed-library/README.md), limited to the society
whose plots it walks.

## What it teaches

1. **Schedules.** `every:` and `run:`, in `schedules/`. Runs fall on a grid in UTC that a restart or a deploy doesn't move; only one runs at a time, ticks missed while the application was down become one run, and a failed run is tried again at the next due time. See `domains/plots/schedules/overdue_rent.jadpo` and [area 12, Schedules](../../docs/language/12-jobs-and-events.md#schedules).
2. **The action a schedule runs.** It is passed nothing and reads `clock.now`, fixed for the whole run. It marks each overdue plot in its own transaction, so the next day's run can't find it again, and emits an event for each. See `domains/plots/actions/flag_overdue_rent.jadpo` and [area 12, Batch work keeps its progress](../../docs/language/12-jobs-and-events.md#batch-work-keeps-its-progress).
3. **A schedule reaches every tenant.** Its run is started by no society's request or event, so it runs as the system across every society; `Access.system` in each entity's grants lets it in. See `policy.jadpo`, `domains/plots/entities/plot.jadpo` and [area 9, Background work](../../docs/language/09-policy.md#background-work).
4. **A subscriber reaches one tenant.** `RentOverdue` holds the society's id in a field of the tenant's own `id` type, and its subscriber is limited to that society. See `domains/plots/events/rent_overdue.jadpo` and `domains/plots/subscribers/remind_overdue.jadpo`.
5. **A batch reaches one tenant.** A run is limited to the society its saved values name, here the round's `society_id`, a `Society.id`, whose plots it walks. `finished:` gets an `ImportSummary`, counts and problems but never a plot's values, and keeps them on the round, a record of the same society whose treasurer reads the plots they count. See `domains/plots/actions/invoice_round.jadpo`, `domains/plots/actions/record_round.jadpo` and [area 12, What a run reaches under policy](../../docs/language/12-jobs-and-events.md#what-a-run-reaches-under-policy).
6. **Testing what a run does.** A schedule test runs the schedule with `overdue_rent.run()`, as the tick would, at a fixed clock, then a day later, and sees each plot flagged once; what the run emits is delivered before the test's next line, so it sees each holder reminded once too. `jadpo test --kind schedule` runs only such tests. See `domains/plots/schedules/overdue_rent.test.jadpo` and [area 13, Kinds of test](../../docs/language/13-testing.md#kinds-of-test).
7. **Watching schedules.** Every run, and every tick skipped and why, is recorded. `jadpo schedules` lists each schedule with its last run and when it is next due, `pause` and `resume` stop and start it, `run` runs it now under its lease, `history` shows its runs, and `jadpo why` says why it didn't run. See [area 12, Watching schedules](../../docs/language/12-jobs-and-events.md#watching-schedules).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/25-allotment-rents
target/release/jadpo test examples/25-allotment-rents
target/release/jadpo fmt --check examples/25-allotment-rents
cd examples/25-allotment-rents && ../../target/release/jadpo dev
```

- `check` ends with `checked: <n> files` and no errors.
- `test` needs PostgreSQL, and ends with `tested: 4 passed, 2 action tests, 2 schedule tests`.
- `dev` serves the application with a disposable PostgreSQL, and ticks
  once a minute. In a second terminal in the project, the schedule
  commands read that database:

  ```sh
  ../../target/release/jadpo schedules
  ../../target/release/jadpo schedules pause overdue_rent
  ../../target/release/jadpo why overdue_rent
  ../../target/release/jadpo schedules resume overdue_rent
  ../../target/release/jadpo schedules run overdue_rent
  ../../target/release/jadpo schedules history overdue_rent
  ```

  `why` answers that it is paused, and its due ticks are skipped until it
  is resumed; `history` lists the run made by hand.

## Try next

- In `overdue_rent.jadpo`, run it `every: 500ms`. `jadpo check` reports `schedule.every_invalid`: a schedule runs on whole seconds, minutes, hours or days.
  <!-- try {"file": "domains/plots/schedules/overdue_rent.jadpo", "replace": "every: 1d", "with": "every: 500ms", "check": ["schedule.every_invalid"]} -->
- In `plot.jadpo`, let `Access.public` change a plot instead of `Access.system`. `jadpo check` reports `policy.system_grant_missing` at the schedule: background work is let in only by name. It also reports `record.out_of_date`: `access.jadpo_record` holds who can do what, and `jadpo records update` writes it again.
  <!-- try {"file": "domains/plots/entities/plot.jadpo", "replace": "update: [OfficerRole.secretary, Access.system]", "with": "update: [OfficerRole.secretary, Access.public]", "check": ["policy.system_grant_missing", "record.out_of_date"]} -->
- In `overdue_plots.jadpo`, leave out `where: Plot.reminded_on == none`. The second test fails with `test.assertion_failed`: the next day's run flags Ada's plot again, and she would be reminded every day.
  <!-- try {"file": "domains/plots/queries/overdue_plots.jadpo", "replace": "    where: Plot.reminded_on == none\n", "with": "", "test": ["test.assertion_failed"], "failed": 1} -->
- In `plot.jadpo`, let only the secretary read who holds a plot, with `read: [OfficerRole.secretary, Access.system]` in a `policy:` of `plot_holder`'s own. `jadpo check` reports `policy.output_field_unproved` at the batch: which plots `invoice_plot` refuses shows who holds one, and the treasurer reads the invoices it writes and the round's count of refused plots. It also reports `record.out_of_date`: `access.jadpo_record` holds who can do what, and `jadpo records update` writes it again.
  <!-- try {"file": "domains/plots/entities/plot.jadpo", "replace": "    plot_holder: HolderName?\n", "with": "    plot_holder: HolderName? {\n        policy: {\n            read: [OfficerRole.secretary, Access.system]\n        }\n    }\n", "check": ["policy.output_field_unproved", "record.out_of_date"]} -->
- Next, [26-delivery-desk](../26-delivery-desk/README.md) installs a package and writes its own integration with a courier's API.
