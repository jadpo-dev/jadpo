# 19 · A book club

A neighbourhood book club: its shelf, the books members wish for, the
rota of who leads each discussion, the reading queue on the noticeboard,
ratings at each meeting, the register, the monthly draw, meeting times
and the old card catalogue. It teaches collections and the standard
library's smaller tools: lists, sets and maps, numbers, randomness,
codes in other code systems, versions, a rule across a type's fields,
and declining a standard type.

A list, a set or a map is a value: each operation gives a new one and
none changes the one it was given, so a list a test builds is the list
it checks. Operations that might find nothing give `T?`, and the
compiler makes the caller say what happens then.

## What it teaches

1. **Lists.** `distinct`, `flatten`, `max_by`, `min_by`, `sum` and `group_by` make a shortlist; `sort_by` shelves the books; `zip_with` pairs leaders with books; `append`, `concat`, `first`, `at`, `last`, `slice`, `keep_first`, `keep_last`, `drop_first`, `drop_last`, `reverse`, `all` and `find` lay out the queue. See `domains/bookclub/actions/make_shortlist.jadpo`, `domains/bookclub/actions/sort_shelf.jadpo`, `domains/bookclub/actions/plan_rota.jadpo`, `domains/bookclub/actions/show_queue.jadpo` and [area 19, Lists](../../docs/language/19-standard-library.md#lists).
2. **Maps.** Ratings merged, counted, read by key, filtered, mapped and taken from. See `domains/bookclub/actions/tally_ratings.jadpo` and [area 19, Maps](../../docs/language/19-standard-library.md#maps).
3. **Sets.** Who came against who said they would: difference, intersection, union, adding, removing and asking. See `domains/bookclub/actions/take_register.jadpo` and [area 19, Sets](../../docs/language/19-standard-library.md#sets).
4. **Numbers.** `decimal.divide` for an average once a condition proves there is something to divide by, `decimal.min` and `max` to keep it in range, and `integer.abs`, `min`, `max` and `divide` for a reading pace. See `domains/bookclub/actions/tally_ratings.jadpo`, `domains/bookclub/actions/pages_per_week.jadpo` and [area 19, Numbers](../../docs/language/19-standard-library.md#numbers).
5. **Randomness.** `random.sample`, `shuffle` and `integer`, read only in actions and seeded in a test. See `domains/bookclub/actions/hold_draw.jadpo` and [area 19, Randomness](../../docs/language/19-standard-library.md#randomness).
6. **Codes in other systems.** An ISBN-10 read as its `Isbn`, and a country's three-letter and numeric codes, with `form:`. See `domains/bookclub/actions/read_old_card.jadpo`.
7. **Versions.** `semver.satisfies` asks whether a member's e-reader app fits a range. See `domains/bookclub/actions/reader_supported.jadpo`.
8. **A rule across fields, and declining a standard type.** `rule ends_after_start` keeps every meeting's end after its start, and `attempt` maps it by name; a poll's proposed slot has a start and an end like the standard `InstantRange`, and `decline type.standard_exists:` says why it isn't one. See `domains/bookclub/types/club_meeting.jadpo`, `domains/bookclub/actions/schedule_meeting.jadpo`, `domains/bookclub/types/proposed_slot.jadpo` and [area 2, Rules across fields](../../docs/language/02-values-and-types.md).
9. **Fixtures for test data.** Each book the tests use is a fixture, so the tests read as the club talks. See `domains/bookclub/fixtures/`.

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/19-book-club
target/release/jadpo test examples/19-book-club
target/release/jadpo fmt --check examples/19-book-club
```

- `check` ends with `checked: <n> files` and no errors.
- `test` ends with `tested: 14 passed, 14 action tests`.

## Try next

- In `proposed_slot.jadpo`, delete the `decline` line. `jadpo check` reports `type.standard_exists`: a start and an end of instants is the standard `InstantRange`, unless the type says why not.
  <!-- try {"file": "domains/bookclub/types/proposed_slot.jadpo", "replace": "decline type.standard_exists: \"A proposal in a poll, not a time anyone has booked; the chosen one becomes a Meeting\"\n", "with": "", "check": ["type.standard_exists"]} -->
- In `schedule_meeting.jadpo`, delete the `ends_after_start` arm. `jadpo check` reports `failure.rule_unmapped`: nothing proves the meeting ends after it starts, so the rule needs an arm.
  <!-- try {"file": "domains/bookclub/actions/schedule_meeting.jadpo", "replace": "        ends_after_start: BackToFront\n", "with": "", "check": ["failure.rule_unmapped"]} -->
- In `tally_ratings.jadpo`, divide by `raters` instead of `how_many`. `jadpo check` reports `type.divisor_zero`: a plain count can be 0, and the condition proves it isn't only by building a `RaterCount`.
  <!-- try {"file": "domains/bookclub/actions/tally_ratings.jadpo", "replace": "decimal.divide(total * 1.00, how_many,", "with": "decimal.divide(total * 1.00, raters,", "check": ["semantic.unused_variable", "type.divisor_zero"]} -->
- Then read [20-fell-race](../20-fell-race/README.md), which works with places.
