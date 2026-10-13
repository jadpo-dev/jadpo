# Time and formatting proposals (area 10)

Input to [area 10, question 7](../10-configuration-and-time.md#open-questions).
Researched 2026-10-12.

## What is complicated today

The time plan (`docs/time-testing-plan.md`) gets the hard parts right: one
absolute `Instant`, no zone-less date-times, explicit zones and locales, and
formatted text that can never become data. The surface built on it is large:

- **About 35 `temporal` functions**, including four `add_local_*` functions
  beside four `add_*` functions, `add_elapsed` beside `+`, and
  `millisecond`, `offset`, `same_zone` and `same_local`.
- **Six option enums** (`LocalOverlap`, `LocalGap`, `InvalidDay`,
  `TimeFormat`, `FriendlyTimeFormat`, `Weekday`) plus `InstantRange` and
  `PresentationText`.
- **Two daylight-saving options on every local-time call**, both required,
  even when the author has no view on either:

  ```jadpo fragment
  var opening: Time = temporal.resolve(
      date: day,
      at: "09:30",
      zone: Zone.europe_london,
      overlap: LocalOverlap.reject,
      gap: LocalGap.reject
  )
  ```

- **Formatting takes two steps and two mutually exclusive arguments**: an
  `Instant` must become a `Time` first, then `style:` or `components:`, and
  `components:` is an anonymous object, which area 2 removed everywhere else.

  ```jadpo fragment
  var label: PresentationText = temporal.format(
      temporal.in_zone(order.created_at, Zone.europe_london),
      locale: Locale.en_gb,
      components: {
          day: numeric
          month: short
      }
  )
  ```

- **`format_friendly` takes a `profile:` with exactly one value**,
  `FriendlyTimeFormat.conversational`.
- **`Time` means a zoned date-time**, while everyone else uses "time" for a
  time of day (area 10, question 5).

## How others do it

**JavaScript Temporal (TC39).** Separate types for an instant, a zoned
date-time and plain dates and times. Turning local time into an instant takes
one `disambiguation` option, `compatible` (default), `earlier`, `later` or
`reject`; `compatible` "acts like 'earlier' for backward transitions and
'later' for forward transitions". Formatting is `toLocaleString(locale,
options)`, using `Intl.DateTimeFormat` options, where `dateStyle`/`timeStyle`
cannot be combined with individual components, the same rule Jadpo has.
<https://tc39.es/proposal-temporal/docs/zoneddatetime.html>,
<https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Intl/DateTimeFormat/DateTimeFormat>

**Java `java.time`.** `Instant`, `LocalDate`, `ZonedDateTime`; `Duration`
(exact) is separate from `Period` (calendar). `ZonedDateTime.of` silently
"uses the earlier offset" in an overlap and moves a gap time "later by the
length of the gap"; `withLaterOffsetAtOverlap()` changes it afterwards.
Formatting uses `DateTimeFormatter.ofLocalizedDate(FormatStyle.LONG)` with a
locale. The current time comes from an injectable `Clock`.
<https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/time/ZonedDateTime.html>

**Rust jiff.** `Timestamp`, `Zoned`, `civil::Date`; one `Disambiguation`
option, `Compatible` by default. Formatting is `strftime`; localisation is
deliberately left to ICU through `jiff-icu`. **Rust chrono** returns
`LocalResult::Single`, `Ambiguous` or `None` and makes the caller choose.
<https://docs.rs/jiff/latest/jiff/>

**Elixir.** `DateTime.from_naive` returns `{:ok, dt}`, `{:ambiguous, a, b}`
or `{:gap, before, after}`, so the caller must handle both cases. Core
formatting is `Calendar.strftime` with no locales; `ex_cldr` adds
`to_string(dt, format: :short | :medium | :long | :full, locale: "fr")`,
CLDR format IDs, and relative time ("tomorrow", "demain").
<https://hexdocs.pm/elixir/DateTime.html>,
<https://ex-cldr-dates-times.hexdocs.pm/readme.html>

Patterns worth taking:

1. **One daylight-saving option, not two.** Temporal, jiff and Java all treat
   overlap and gap as one decision. Only Temporal and jiff offer `reject`.
2. **Named styles first** (`short` to `full`); every library with locale
   support leads with them and treats custom shapes as the exception.
3. **Format in one call with the zone and locale** (Temporal's
   `toLocaleString` takes the zone from the value; ex_cldr takes the locale
   as an option). Nobody makes the caller build a zoned value just to format
   it.
4. **Relative time is its own call** without a "profile".

## Options

### A. Fix the syntax only

Keep every function and both daylight-saving options. Replace the anonymous
`components:` object with a declared format type and write locales as names.

```jadpo fragment
type ShortDate = TimeFormat {
    day: numeric
    month: short
}

var label: PresentationText = temporal.format(
    temporal.in_zone(order.created_at, Zone.europe_london),
    locale: Locale.en_gb,
    format: ShortDate
)
```

Cheapest, and it fixes the area 2 conflict. It keeps the 35 functions, two
policies per call and two-step formatting.

### B. One call per job (recommended)

**Formatting.** One function, three named inputs, with the format either a
built-in value or a declared type:

```jadpo fragment
var placed: PresentationText = temporal.format(
    order.created_at,
    zone: customer.zone,
    locale: customer.locale,
    format: ShortDate
)

var due: PresentationText = temporal.format(
    invoice.due_on,
    locale: customer.locale,
    format: TimeFormat.date_long
)

var seen: PresentationText = temporal.format_relative(
    message.sent_at,
    now: clock.now,
    zone: viewer.zone,
    locale: viewer.locale
)
```

`zone:` is required for an `Instant` and not allowed for a `CalendarDate` or
a zoned value, which carries its own. `format:` replaces `style:` and
`components:`, so `type.temporal_format_choice` disappears.

**Local time.** One `dst:` option. Without it, a local time that falls in an
overlap or gap is a failure the caller maps, like any runtime check
(area 2):

```jadpo fragment
var opening: ZonedTime = attempt temporal.resolve(
    day,
    at: "09:30",
    zone: Zone.europe_london
) invalid: OpeningTimeUnclear

var reminder: ZonedTime = temporal.resolve(
    day,
    at: "01:30",
    zone: Zone.europe_london,
    dst: Dst.compatible
)
```

`Dst` has `reject` (the default), `earlier`, `later` and `compatible`
(Temporal's meaning: earlier in an overlap, later in a gap). The same option
applies to `add_days`, `add_months` and `add_years` on a zoned value.

**The library, about 20 functions:**

```text
temporal.in_zone(instant, zone) -> ZonedTime
temporal.resolve(date, at:, zone:, dst:) -> ZonedTime
temporal.between(start, end) -> Duration
temporal.add_days(value, days, dst:)                        // CalendarDate or ZonedTime
temporal.add_months(value, months, invalid_day:, dst:)
temporal.add_years(value, years, invalid_day:, dst:)
temporal.day_bounds(date, zone) / week_bounds(date, zone, starts_on:)
temporal.month_bounds(date, zone) / year_bounds(date, zone)
temporal.date(zoned) / year / month / day / weekday / hour / minute / zone
temporal.format(value, zone:, locale:, format:)
temporal.format_relative(value, now:, zone:, locale:)
```

Removed: `add_elapsed` (use `+`), `add_weeks` (use seven days), the four
`add_local_*` functions (the `add_*` functions take a zoned value), `second`,
`millisecond`, `offset`, `same_zone`, `same_local`, `FriendlyTimeFormat`,
`LocalOverlap` and `LocalGap`. Each can return when an application needs it.

Trade-offs: a one-time rewrite of `examples/temporal` and the compiler's
temporal checks; `dst:` is less fine-grained than two options (an author
cannot ask for "earlier in overlaps, reject gaps"). No surveyed library
offers that combination either.

### C. Pattern strings

`format: "d MMM yyyy"` (ICU patterns) or CLDR format IDs
(`format: :gy_mmm_ed` in ex_cldr). Compact, and translators know them, but
the string is opaque to the compiler and to a reviewer, patterns differ by
locale (`d MMM` is wrong in the US), and the time plan rejected them for that
reason. Not recommended.

## Recommendation

**Option B.** It follows what Temporal, Java and jiff converged on (one
daylight-saving decision, one formatting call, named styles first) while
keeping Jadpo's stricter default: an unclear local time fails unless the
author writes `dst:`. Temporal, jiff and Java silently choose by default;
for booked times and deadlines that silent shift is a known source of bugs,
and Jadpo's failures make the safe default cheap to handle. The surface
shrinks from about 35 functions and six option enums to about 20 functions
and three (`Dst`, `InvalidDay`, `TimeFormat`), and every remaining
argument is named once.
