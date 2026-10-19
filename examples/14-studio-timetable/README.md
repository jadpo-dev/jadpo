# 14 · A yoga studio's timetable

A yoga studio in London: classes that repeat every week, doors that open
by the clock on the wall, rooms booked by the hour, memberships that
renew each month, an overnight retreat on the night the clocks change,
and every time shown to members in English or French. It teaches time:
instants, dates, zones, periods and the times a rule repeats on.

Two rules run through it. Code never reads the system clock: an action
reads `clock.now`, the time its operation started, and a function takes
the time as a parameter. And machine values never mix with text for
people: an `Instant` is stored and compared, a `CalendarDate` is a day
with no clock, and a time is formatted into `PresentationText` only to
be shown, in a zone and a locale the code names.

## What it teaches

1. **The studio's zone and locales.** `studio_zone` is a `Zone` setting, and `locales { }` in `app.jadpo` lists the languages times are shown in. See `config.jadpo`, `app.jadpo` and [area 10, Locales](../../docs/language/10-configuration-and-time.md#locales).
2. **Times a rule repeats on.** A class's `RecurrenceRule` gives its next times in its own zone, so a 09:30 class stays at 09:30 when the clocks go back; each is shown with a declared `TimeFormat`, the locale choosing the order of its parts. See `domains/studio/actions/upcoming_classes.jadpo`, `domains/studio/types/class_start_format.jadpo` and [area 19, Time and calendar additions](../../docs/language/19-standard-library.md#time-and-calendar-additions).
3. **Opening hours.** `OpeningHours` carries its zone and its exceptions, and `temporal.open_at` reads it at an instant. See `domains/studio/actions/is_studio_open.jadpo`.
4. **Ranges of time.** An `InstantRange` excludes its end, so bookings that meet on the hour don't clash; `range.overlaps`, `range.intersection` and `temporal.between` say whether and for how long they do. See `domains/studio/actions/check_booking.jadpo` and [area 19, Ranges](../../docs/language/19-standard-library.md#ranges).
5. **A day, a week, a month and a year.** Each starts at midnight in the studio's zone, so a day runs 23, 24 or 25 hours, and a week starts on the weekday the call names. See `domains/studio/actions/count_bookings.jadpo` and [area 10, Zones and local times](../../docs/language/10-configuration-and-time.md#zones-and-local-times).
6. **Calendar arithmetic.** A date moves by calendar days, months and years; a month on from the 31st takes the month's last day, and a year on from 29 February fails unless the call says otherwise. A `CalendarPeriod` counts whole years, months and days between two dates. See `domains/studio/libraries/membership_rules.jadpo`, `domains/studio/actions/show_membership.jadpo` and `domains/studio/actions/anniversary_card.jadpo`.
7. **From a local time to an instant.** On the night the clocks change, 01:30 happens twice or not at all; `dst:` chooses, and a choice passed in that might refuse is mapped with `attempt`. A `ZonedTime` gives its date, its parts, its zone and its instant. See `domains/studio/actions/plan_retreat.jadpo`.
8. **Times for people.** `text.format` shows a time with a format, and `temporal.format_relative` against now: "4 minutes ago", "Yesterday at 14:30", in the reader's language. See `domains/studio/actions/show_booked.jadpo` and [area 10, Showing dates and times to people](../../docs/language/10-configuration-and-time.md#showing-dates-and-times-to-people).
9. **A test's clock.** `clock:` fixes the time a test runs at, and `clock.advance` moves it on, so every test gives the same answer on any day. See `domains/studio/actions/is_studio_open.test.jadpo` and [area 13, Configuration in tests](../../docs/language/13-testing.md#configuration-in-tests).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/14-studio-timetable
target/release/jadpo test examples/14-studio-timetable
target/release/jadpo fmt --check examples/14-studio-timetable
```

- `check` ends with `checked: <n> files` and no errors.
- `test` ends with `tested: 13 passed, 13 action tests`.

## Try next

- In `show_booked.jadpo`, delete `zone: config.studio_zone,`. `jadpo check` reports `type.temporal_zone_required`: an `Instant` has no zone of its own, and the day it falls on depends on one.
  <!-- try {"file": "domains/studio/actions/show_booked.jadpo", "replace": "            zone: config.studio_zone,\n", "with": "", "check": ["type.temporal_zone_required"]} -->
- In `anniversary_card.jadpo`, show the day with `TimeFormat.date_time_full`. `jadpo check` reports `type.temporal_format_requires_time`: a `CalendarDate` has no clock time to show.
  <!-- try {"file": "domains/studio/actions/anniversary_card.jadpo", "replace": "format: TimeFormat.date_full", "with": "format: TimeFormat.date_time_full", "check": ["type.temporal_format_requires_time"]} -->
- In `show_membership.test.jadpo`, show the period with `locale: Locale.de_de`. `jadpo check` reports `type.locale_unsupported`: the studio doesn't write German, so nothing is shown in it.
  <!-- try {"file": "domains/studio/actions/show_membership.test.jadpo", "replace": "text.format(membership.member_for, locale: Locale.en_gb)", "with": "text.format(membership.member_for, locale: Locale.de_de)", "check": ["type.locale_unsupported"]} -->
- In `count_bookings.jadpo`, start the week on `Weekday.sunday`. The counting test fails with `test.assertion_failed`: the Monday booking is in last week now.
  <!-- try {"file": "domains/studio/actions/count_bookings.jadpo", "replace": "starts_on: Weekday.monday", "with": "starts_on: Weekday.sunday", "test": ["test.assertion_failed"], "failed": 1} -->
- Next, [15-parcel-tracker](../15-parcel-tracker/README.md) words its messages in each language.
