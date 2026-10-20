# 24 · A seed library

A seed library lends packets of saved seed: members borrow a packet,
grow it, and bring back seed saved from their own plants. Every spring
the volunteers check every packet on the shelves, withdrawing any too
old to sow, and keep a count of how the check went. It teaches batches:
work over more items than one action can hold, which carries on after a
crash or a deploy, and never does an item twice.

One action can't check every packet: it would be one transaction over
the whole library, and if it failed on the last packet nothing would be
kept. So the action that starts the check hands the work to a batch,
which walks a query's pages after it commits, calling an action for
each packet. Each page commits on its own with how far the run has got,
so a restart picks up at the next page, and a packet that can't be
checked becomes a problem, not a stop.

## What it teaches

1. **Starting a batch.** `batches.each` with `from:`, a call of a named query returning a `Page`, `each:` calling an action per item, `finished:` once at the end, and `key:`. It gives a `BatchRun` at once, kept on the stock check. See `domains/seedbank/actions/start_check.jadpo` and [area 12, Batches](../../docs/language/12-jobs-and-events.md#batches-work-over-many-items).
2. **The query a batch walks.** A `Page` ordered by `id`, which never changes, so every packet is in exactly one page however the shelves change during the walk. See `domains/seedbank/queries/shelf_packets.jadpo`.
3. **An item's action.** An ordinary action given the item and what the run saved; when it fails, only its writes are undone, and the run keeps a problem in its place. See `domains/seedbank/actions/check_packet.jadpo`.
4. **Finishing.** `finished:` gets an `ImportSummary`: how many items, how many with problems, and each `RowProblem`, never the item's values. See `domains/seedbank/actions/record_check.jadpo`.
5. **What a run saves.** Values `each:` and `finished:` capture, and `key:`, are saved with the run, so a run resumed after a deploy has them; `key:` makes a second start give the same run. See `domains/seedbank/actions/start_check.jadpo` and its test.
6. **Watching and stopping a run.** `batches.status` gives items done, problems so far and the state; `batches.cancel` stops the run after the page in hand. See `domains/seedbank/actions/check_progress.jadpo`, `domains/seedbank/actions/stop_check.jadpo` and [area 12, How a run is kept](../../docs/language/12-jobs-and-events.md#how-a-run-is-kept).
7. **Background work and grants.** A batch runs as the system, so what its query reads and its actions change let `Access.system` in. See `domains/seedbank/entities/seed_packet.jadpo`.
8. **An operation in a condition.** `temporal.year(SeedPacket.donated_on) == season`: the database works the year out exactly as code does, and the migration indexes that expression. See `domains/seedbank/queries/donated_in.jadpo` and [area 6, Operations in conditions](../../docs/language/06-reads-and-writes.md#operations-in-conditions).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/24-seed-library
target/release/jadpo test examples/24-seed-library
target/release/jadpo fmt --check examples/24-seed-library
```

- `check` ends with `checked: <n> files` and no errors.
- `test` needs PostgreSQL, and ends with `tested: 6 passed, 6 action tests`.
  A run starts after its action commits, on the runtime's runner, which
  a test doesn't have, so the tests see each run as it starts, and test
  each page's action on its own.

## Try next

- In `start_check.jadpo`, take `key: stocktake.id` away. The first test fails with `test.assertion_failed`: without a key, every start is a new run, and a double click would check the shelves twice.
  <!-- try {"file": "domains/seedbank/actions/start_check.jadpo", "replace": "        finished: summary -> record_check(summary, stocktake.id),\n        key: stocktake.id\n", "with": "        finished: summary -> record_check(summary, stocktake.id)\n", "test": ["test.assertion_failed"], "failed": 1} -->
- In `start_check.jadpo`, walk `donated_in(stocktake.season)` instead. `jadpo check` reports `type.batch_from_invalid`: a list can't be walked a page at a time, and resumed where it left off.
  <!-- try {"file": "domains/seedbank/actions/start_check.jadpo", "replace": "from: shelf_packets(PacketPage {})", "with": "from: donated_in(stocktake.season)", "also": [{"replace": "use seedbank/queries { shelf_packets }", "with": "use seedbank/queries { donated_in }"}, {"replace": "use seedbank/types { PacketPage }\n", "with": ""}], "check": ["type.batch_from_invalid"]} -->
- In `seed_packet.jadpo`, let `Access.public` change a packet instead of `Access.system`. `jadpo check` reports `policy.system_grant_missing` at `check_packet`: a batch runs as the system, and a grant to anyone at all never lets background work in.
  <!-- try {"file": "domains/seedbank/entities/seed_packet.jadpo", "replace": "update: [Access.system]", "with": "update: [Access.public]", "check": ["policy.system_grant_missing"]} -->
- Then read [25-allotment-rents](../25-allotment-rents/README.md), where schedules run work on a timer for many societies at once.
