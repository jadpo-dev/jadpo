# Instant

One moment in time, the same everywhere in the world: `2026-10-25T09:30:00Z`.

Use it for when something happened, such as when an order was placed. For a day on
a calendar, use `CalendarDate`. For a time on someone's clock in a place, use
`ZonedTime`. For a time with no place yet, use `LocalDateTime`.

```jadpo
Instant("2026-10-25T09:30:00Z")
```

- It is always in UTC, written with `Z`. An offset such as `+01:00` is refused.
- `clock.now` gives the current one. Adding a `Duration` gives another:
  `clock.now + 14d`.
- `text.format` needs a zone and a format, since a moment looks different in each
  place.

# CalendarDate

A day on the calendar, with no time or zone: `2026-10-25`.

Use it for birthdays, due dates and anything else that is a day wherever you are.
For a moment in time, use `Instant`. For a span of days, use `DateRange`.

```jadpo
CalendarDate("2026-10-25")
```

- `temporal.add_days`, `temporal.add_months` and `temporal.add_years` give another.
- It checks the date is real, so `2026-02-30` is refused.

# ZonedTime

A moment seen on the clock in one place: `2026-10-25T09:30:00+00:00[Europe/London]`.

Use it to do clock and calendar sums in a place, such as "9am the next day in
London". To store a moment, use `Instant`. `temporal.in_zone` gives a `ZonedTime`
from an `Instant`.

```jadpo
temporal.in_zone(Instant("2026-10-25T09:30:00Z"), Zone.europe_london)
```

- Adding a day keeps the clock time across a daylight saving change, which adding
  24 hours to an `Instant` doesn't.

# Duration

An exact length of time, to the millisecond: 90 minutes is `PT1H30M`.

Use it for timeouts and gaps between two instants. For a month, which has no fixed
length, use `CalendarPeriod`.

```jadpo
Duration(90m)
```

- In code, write a unit after the number: `500ms`, `30s`, `90m`, `2h`, `14d`. `m`
  is minutes, not metres.
- `temporal.between` gives the one between two instants.

# Weekday

A day of the week: `monday` to `sunday`.

Use it for opening days and weekly schedules. `temporal.weekday` gives one from a
date. For a whole week's opening times, use `OpeningHours`.

```jadpo
Weekday.monday
```

# LocalTime

A time on the clock, with no date or zone: `09:30:00`.

Use it for a time that repeats, such as a shop opening at 09:30. For one moment,
use `Instant`.

```jadpo
LocalTime { hour: 9 minute: 30 second: 0 }
```

- The hour runs from 0 to 23. `24:00:00` is refused, and text needs the seconds, so
  `09:30` is refused as well.

# LocalDateTime

A date and a clock time with no zone: `2026-10-25T09:30:00`.

Use it for a future appointment, where the clock time must hold even if a country
changes its daylight saving rules. It isn't one moment until you give it a zone.
For a moment, use `Instant`.

```jadpo
LocalDateTime { date: "2026-10-25" time: LocalTime { hour: 9 minute: 30 second: 0 } }
```

- `temporal.resolve` turns one into an instant in a `Zone`.

# Month

A month of the year: `january` to `december`.

Use it for something that recurs each year, such as a seasonal price. For one
particular month, use `YearMonth`.

```jadpo
Month.october
```

# YearMonth

One month of one year: `2026-10`.

Use it for a billing period or a monthly report. For the month a card expires, use
`CardExpiry`. For a month in any year, use `Month`.

```jadpo
YearMonth { year: 2026 month: 10 }
```

# Year

A calendar year: `2026`.

Use it for a tax year's label or a year of publication, from 0 to 9999. In JSON it
is a string. For a plain count of years, use `CalendarPeriod`.

```jadpo
Year { year: 2026 }
```

# Quarter

A quarter of a calendar year: `2026-Q4`.

Use it for financial reports by quarter. Quarters here follow the calendar year, so
for a financial year with another start, keep your own type.

```jadpo
Quarter { year: 2026 quarter: 4 }
```

# IsoWeek

A numbered week of a year, as ISO 8601 counts them: `2026-W43`.

Use it for weekly reports and rotas. A week's number doesn't always belong to the
year of its days, so for a span of dates, use `DateRange`.

```jadpo
IsoWeek { year: 2026 week: 43 }
```

- Week 53 exists only in some years, so `2027-W53` is refused and `2026-W53` is not.

# UtcOffset

A fixed gap from UTC, in minutes: `+05:30`.

Use it to record the offset a device or a message reported. For a place, use
`Zone`, which follows the place's daylight saving rules.

```jadpo
UtcOffset { minutes: 330 }
```

- UTC itself is `+00:00`. `Z` is refused.

# DateRange

The days from one date to another, both included: `2026-10-01/2026-10-31`.

Use it for bookings, holidays and reporting periods. For moments, use `InstantRange`,
which leaves out its end. For a range of any other ordered type, use `Range`.

```jadpo
DateRange { start: "2026-10-01" end: "2026-10-31" }
```

- Unlike every other range, the last day counts: a one-day range has `start` and
  `end` on the same date.
- `range.contains`, `range.overlaps` and `range.intersection` work on it.

# InstantRange

The moments from a start up to, but not including, an end.

Use it for a shift or a meeting slot. Because the end is left out, one slot can
start exactly when the last one ends. For whole days, use `DateRange`.

```jadpo
InstantRange { start: "2026-10-25T09:00:00Z" end: "2026-10-25T17:00:00Z" }
```

- `start` can't come after `end`.
- `temporal.day_bounds` gives the range of a whole day in a `Zone`.

# CalendarPeriod

An amount of calendar time in years, months and days: `P1Y2M10D`.

Use it for "one month from now", where a month is a month however many days it has.
For an exact amount of time, use `Duration`.

```jadpo
CalendarPeriod { years: 1 months: 2 days: 10 }
```

- Two values are equal only part by part, so one month doesn't equal 30 days, and
  `<` doesn't take them.

# RecurrenceRule

When something repeats, in the calendar standard's own text.

Use it for repeating events, such as every Monday at 09:30. `temporal.occurrences`
gives the next ones. For a shop's weekly hours, use `OpeningHours`.

```jadpo
RecurrenceRule("DTSTART;TZID=Europe/London:20261026T093000\nRRULE:FREQ=WEEKLY;BYDAY=MO")
```

- It holds its first time on one line and its rule on the next, with the zone in
  the first, so daylight saving is kept right.

# OpeningHours

When a place is open each week, and the days that differ.

Use it for a shop's or a clinic's hours. `temporal.open_at` tells you whether it is
open at an instant. For repeating calendar events, use `RecurrenceRule`.

```jadpo
OpeningHours("Mo-Fr 09:00-17:30; Sa 10:00-16:00 [Europe/London]")
```

- It uses OpenStreetMap's form and ends with its zone in brackets. Times need two
  digits, so `9:00` is refused.

# CalendarEvent

A calendar event, as a calendar app reads one: a title, and its days or its times.

Use it to send an invitation or an `.ics` entry. For when something repeats on its
own, use `RecurrenceRule`.

```jadpo
CalendarEvent {
    summary: "Team lunch"
    first_day: "2026-12-18"
    last_day: "2026-12-18"
    start: none
    end: none
    zone: none
    location: none
    description: none
    recurrence: none
}
```

- An event has either days (`first_day` and `last_day`) or times (`start`, `end` and
  `zone`), never both.
- `last_day` is included, even though calendar files write the day after.
