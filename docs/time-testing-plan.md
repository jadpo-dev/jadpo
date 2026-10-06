# TIME-001 and TEST-001 decision plan

**Status:** approved contract; Temporal/runtime-clock, typed callable-fixture core and independently reviewed checked authored service fakes implemented; broader capability/job integration remains pending

**Prepared:** 2026-09-27

**Approved:** 2026-09-27

**Accepted-contract digest:** `sha256:17b8ac38c645f6e3f42abf9a7644b9c6f67451d0c3407f7bf108ec057a931f01` (section 2)

**Scope:** application-visible time, the compiler-owned temporal standard
library, bounded timezone resolution and human-readable formatting, runtime
deadlines, deterministic test capabilities, fixture isolation, boundary
invocation, and the division between semantic checks, generated tests, and
authored tests
**Out of scope:** recurrence rules, holidays and business calendars, natural-
language date parsing, general application-message translation, custom locale
packs, distributed clock synchronisation, queue scheduling and delivery policy,
general property-test syntax, fuzzing syntax, and arbitrary dependency
injection

This plan resolves the time and testability decisions needed by authentication,
configuration readiness, services, jobs, and workflows. It extends the
implemented top-level `test` plus Boolean `assert` core; it does not replace it
with a second test language.

## 1. Existing constraints

The decision must preserve these repository commitments:

- `Date`, `Time`, `DateTime`, and `Duration` are currently distinct prelude
  types; this contract replaces ambiguous `DateTime` with `Instant`, narrows
  date-only facts to `CalendarDate`, and redefines `Time` as a resolved
  zone-aware value through the accepted TIME/TEST-P0 migration;
- the golden todo evaluates due dates against an injected clock and passes
  explicit cutoff values into reminder and retention actions;
- authentication requires deterministic expiry, issued-at, revocation-bound,
  and skew evidence;
- readiness, retries, service timeouts, schedules, and workflows require
  deadlines that are not vulnerable to wall-clock adjustment;
- `jadpo test` performs the authoritative checked build before executing tests;
- authored `test "name"` declarations and Boolean `assert` already compile to
  deterministic, stack-free reports;
- generated tests are defence-in-depth evidence, never substitutes for static
  proof or human-owned business intent; and
- tests receive configuration from the harness rather than `.env.local` or
  ambient production configuration.

The current examples contain both `clock.now` and actions that accept
`at: DateTime`. The accepted model must migrate those values to `Instant` and
explain when operation time versus an explicit instant is appropriate rather
than treating them as interchangeable styles.

## 2. Approved v0.1 decisions

| ID | Decision |
| --- | --- |
| TIME-D01 | Replace ambiguous `DateTime` with `Instant`. An `Instant` is one absolute point on the timeline, normalised to UTC and represented across generated targets at millisecond precision. |
| TIME-D02 | External RFC 3339 instant values may contain `Z` or an explicit numeric offset. Validation normalises them to UTC and canonical output uses `Z`; an offset-free local timestamp is invalid as `Instant`. |
| TIME-D03 | `Duration` is fixed elapsed time. v0.1 literals use integer `ms`, `s`, `m`, or `h`; `d`, `w`, months, and years are excluded because calendar and elapsed meanings diverge at daylight-saving transitions. Bounded calendar manipulation uses explicit date operations and overflow policies rather than `Duration`. |
| TIME-D04 | Authored effectful code reads time only through the compiler-owned `clock.now`. Pure functions and named queries cannot read the clock; they receive an `Instant` or `Duration` parameter when time affects their result. |
| TIME-D05 | `clock.now` is one stable wall-time snapshot for a top-level operation. Nested actions share it. A later route request, job activation, retry attempt, or workflow resumption receives a new snapshot. v0.1 has no authored “refresh now” operation. |
| TIME-D06 | Runtime timeouts and elapsed deadlines use a compiler-owned monotonic source that is not exposed as a serialisable application value. Persisted deadlines and externally visible timestamps remain UTC `Instant` values. |
| TIME-D07 | The compiler passes application-visible timestamps into persistence statements. Authored semantic values do not use database `CURRENT_TIMESTAMP`, JavaScript `Date.now()`, host fake timers, or another ambient clock. |
| TIME-D08 | A single production UTC clock is target-owned, so application source does not select a clock implementation. `clock.now` already makes the effect visible. The earlier `application clock: system_utc` line is removed rather than becoming a configurable plug-in point. |
| TIME-D09 | Type-level `default now` is rejected. Time-dependent values are set explicitly by the owning action or by a future compiler-owned lifecycle feature with its own semantics and audit entry. |
| TIME-D10 | `Instant` is the ordinary authoritative timestamp. `CalendarDate` exists only for genuinely date-only domain facts. There is no general-purpose zone-less `Time` or `LocalDateTime` value. |
| TIME-D11 | `Time` is a fully resolved, zone-aware human view containing an underlying `Instant` plus one `Zone`. It may be created from an instant and zone, or by resolving a calendar date plus clock text and zone. Ambiguous and nonexistent local input rejects unless an explicit overlap/gap policy is supplied. |
| TIME-D12 | `Zone` is a compiler-generated closed enum derived from a pinned IANA timezone database. Source uses values such as `Zone.europe_london`; wire/storage form remains the canonical `Europe/London` identifier. Abbreviations and fixed offsets are not zone enum values. |
| TIME-D13 | Jadpo supplies a compiler-owned `temporal` standard library for instant arithmetic, date arithmetic, zone resolution, local-period bounds, component inspection, comparison, and formatting. Authored code does not import a host date library. |
| TIME-D14 | Human-readable formatting accepts `CalendarDate` or resolved `Time` plus an explicit supported `Locale`. An `Instant` must first become `Time` through `temporal.in_zone`. The library provides named styles plus checked structured component options rather than opaque `strftime`-style pattern strings. |
| TIME-D15 | Human formatting returns compiler-classified `PresentationText`, not ordinary authoritative `Text`. It is never accepted as a canonical instant, used for time comparison, or stored in place of the underlying typed value. Parsing remains strict and separate from formatting. |
| TIME-D16 | Applications declare a bounded set of supported BCP 47 locales. The compiler/runtime owns and records the timezone and locale-data versions used for resolution and formatting so deployment hosts cannot silently change behaviour. |
| TIME-D17 | Wire representations are compiler-owned and type-specific: `Instant` uses the strict RFC 3339 profile; `CalendarDate` uses ISO full-date; resolved `Time` uses a closed object containing canonical `instant` and `zone`; `Zone` uses its IANA identifier; `Locale` uses a declared BCP 47 tag; and `Duration` uses the ISO 8601 time-only subset. Generated decoders convert them before business code and serializers emit one canonical form. |
| TIME-D18 | Time/date types do not implicitly convert to each other or to/from `Text`. Their permitted contexts are checked: resolved `Time` provides the zone-aware inspection view, and human formatting returns presentation-classified text that cannot become time authority or a database predicate. |
| TIME-D19 | Compiler-managed lifecycle timestamps are explicit field roles, not magic names. The canonical roles are declared as `on: create` and `on: create_or_change`; the compiler supplies one operation-time value and callers or authored mutation bodies cannot set those fields. |
| TIME-D20 | Jadpo-managed domain tables do not use database current-time defaults, generated timestamp columns, or timestamp-mutating triggers. Generated persistence sends explicit `Instant` parameters, and schema/drift checks reject competing database ownership. |
| TIME-D21 | A field declared `on: create_or_change` advances only when a successful mutation changes persisted semantic state. A no-op does not change the timestamp; command receipt or attempted changes belong in explicit audit/event records. |
| TIME-D22 | External/provider timestamps are validated as declared `Instant` values and remain distinct from compiler-owned receipt time. Security, ordering, or idempotency may rely on provider time only when its service contract explicitly permits that trust. |
| TIME-D23 | A migration never backfills missing historical timestamps with an unreviewed `now`. It must preserve absence, derive from named existing authority, use a reviewed recorded constant, or require a lifecycle decision. |
| TIME-D24 | Generated database adapters decode and validate every time/date column into its declared Jadpo type before constructing an entity or query result. Authored code never receives driver strings, numeric epochs, host `Date` objects, or database-native time objects. |
| TIME-D25 | A time/date column whose database kind, nullability, range, precision, or value contradicts the checked schema produces a contained `PersistenceFault`; it is never silently coerced. Jadpo-managed writes are millisecond-aligned, while imported higher-precision or zone-less legacy columns require an explicit reviewed mapping. |
| TIME-D26 | The time library uses the compiler-owned lowercase `temporal` namespace under the language-wide standard-library ownership rule; it is not a special calling convention. The value being operated on is first, environmental context such as zone/locale follows, and policy/options are last and named. Synonymous APIs, reversed argument order, boolean flags, implicit defaults, unqualified aliases, and host-library escape routes are rejected. |
| TIME-D27 | Timeline ordering compares the underlying instant. `Instant` and resolved `Time` may be ordered safely; zone identity and local-representation equality use separately named operations. A clock fragment such as `17:00` cannot be compared until resolved for a calendar date and zone. |
| TIME-D28 | Friendly formatting is deterministic and separate from absolute formatting. It requires a value, explicit zone/locale context, and an explicit reference `Instant`; named profiles own thresholds, rounding, and fallback rather than scattering prose rules through application code. |
| TIME-D29 | IANA enum evolution is reviewed. Canonical additions may extend the enum; renamed/linked identifiers remain deprecated source aliases for compatibility but canonical output and new source use the current identifier. Rule-data version changes are visible in audit and reproducibility evidence. |
| TIME-D30 | Each application declares its supported BCP 47 locales and one default/fallback. That declaration generates a closed `Locale` enum such as `Locale.en_gb`; arbitrary host or request locale strings never reach formatting directly. |
| TIME-D31 | Absolute formats are closed `TimeFormat` enum values. `temporal.format` accepts exactly one named format or one checked structured component object, never both. The initial named set is `date_full|long|medium|short`, `time_full|long|medium|short`, and `date_time_full|long|medium|short`. |
| TIME-D32 | v0.1 includes one frozen `FriendlyTimeFormat.conversational` profile: under one minute uses “just now”/“in under a minute”; under one elapsed hour uses whole minutes with past values rounded down and future values up; otherwise the same local date uses “Today”, adjacent dates use “Yesterday”/“Tomorrow”, dates two through six local days away use localized weekday plus short time, and all other values use `date_time_long` with short zone name. |
| TEST-D01 | Every top-level test receives a fresh checked application instance and isolated state. Test order is semantically irrelevant; parallel execution is permitted only when isolation is proven. |
| TEST-D02 | The harness provides a fixed wall clock by default and may advance wall and monotonic test time through an explicit test-only operation. Production code cannot detect that the capability is a test implementation. |
| TEST-D03 | Pure tests continue to use ordinary calls and `assert`. Integration tests may invoke one of three explicit surfaces: a callable directly, an HTTP route through its real generated boundary, or a job activation through its generated entry boundary. Reports identify which surface supplied the evidence. |
| TEST-D04 | Direct callable tests do not count as route authentication, decoding, failure-envelope, or serialisation evidence. Direct job-action calls do not count as scheduling, lease, retry, or job-boundary evidence. |
| TEST-D05 | Persistence tests use the real selected adapter: isolated SQLite by default and explicitly provisioned PostgreSQL for backend-specific evidence. Jadpo does not define an in-memory repository mock that can drift from transaction or query semantics. |
| TEST-D06 | External services, authentication verifiers, clocks, entropy, queues, and similar declared capabilities may have compiler-owned fakes. A fake exposes only declared operations and outcomes; tests cannot replace arbitrary functions, inspect private implementation, or inject host-language callbacks. |
| TEST-D07 | Configuration values are supplied as typed fixture values. Secret fixture values remain secret-classified and are redacted from reports, snapshots, diagnostics, and interaction traces. |
| TEST-D08 | Capability interaction assertions use compiler-owned, typed traces of declared calls and outcomes. There are no arbitrary spies or stringly typed method assertions. |
| TEST-D09 | Compiler-generated tests exercise facts already present in the semantic graph and generated runtime boundaries. Authored tests own business examples and requirements. Generated tests cannot satisfy policy proof, external review, or authored acceptance obligations. |
| TEST-D10 | General property-test declarations and automatic arbitrary-value derivation are deferred. Compiler and adapter implementations may continue to use host-level fuzzing and upstream conformance suites as internal evidence. |
| TEST-D11 | Authored source has no ambient randomness in v0.1. Compiler-owned identity, credential, and cryptographic operations use a declared entropy capability internally; tests may supply a deterministic byte stream, but that test fake is never production-security evidence. |
| TEST-D12 | Crash and duplicate-delivery tests operate at named compiler-owned checkpoints and durable boundaries. They do not monkey-patch generated code or infer correctness from sleeps. Later async/workflow plans define the actual checkpoints. |

## 3. Time semantics

### 3.1 Values and canonical representation

`Instant` identifies one point on the global timeline. The target boundary
accepts a strict RFC 3339 timestamp with an explicit `Z` or numeric offset,
rejects offset-free local timestamps, normalises the value to UTC, and emits
canonical UTC with exactly millisecond precision.

Examples:

```text
Instant("2026-01-15T12:00:00Z")
Instant("2026-01-15T13:00:00+01:00") // same instant
```

Both canonicalise to:

```text
2026-01-15T12:00:00.000Z
```

This precision is the portable intersection of the initial Bun, SQLite, and
PostgreSQL targets. Higher-precision database values must be rejected or
normalised at the adapter boundary rather than changing equality by target.

`CalendarDate` is a deliberately date-only business fact, not an instant
truncated at midnight. `Time` is never a zone-less clock fragment: it is a
resolved value containing an underlying `Instant` plus a `Zone` for human-local
inspection and formatting. A clock fragment such as `09:30` exists only as a
named argument while resolving a `Time`; it cannot be persisted or compared by
itself.

### 3.2 Arithmetic

The bounded v0.1 operations are:

```text
Instant + Duration -> Instant
Instant - Duration -> Instant
Instant - Instant -> Duration
Duration + Duration -> Duration
Duration - Duration -> Duration
```

Resolved `Time` and `Instant` values may be ordered by their underlying instant.
Whether two values retain the same zone or local representation uses separately
named inspection operations rather than timeline equality. Overflow or a value outside the portable
range is rejected during constant checking when known and contained as a
compiler-owned runtime fault otherwise. Date arithmetic and zone conversion use
the named standard-library operations below. Rounding, natural-language
parsing, recurrence, holidays, and business-day calculations are unsupported
rather than target-defined.

### 3.3 Operation time

An effectful entry obtains one `operation_time` when it starts. Every
`clock.now` read in that operation, including nested actions in the same local
transaction, resolves to that value. This guarantees that created/updated
timestamps, expiry comparisons, policy checks, and emitted records cannot
disagree merely because execution crossed a millisecond boundary.

```text
action create_todo(input: CreateTodo, principal: Principal.user) -> TodoView {
    if input.due_at exists and input.due_at <= clock.now {
        reject DueDateInPast
    }

    var todo = attempt create Todo {
        // ...
        created_at: clock.now
        updated_at: clock.now
    }

    return project todo as TodoView
}
```

If computation is meaningfully parameterised by a cutoff, that cutoff is data:

```text
query reminders_due(at: Instant) -> List<Todo> freshness authoritative {
    where: due_at < at
}

action send_due_reminders(at: Instant) -> Unit {
    var todos = attempt reminders_due(at)
    // ...
}
```

The job boundary may later pass its scheduled occurrence separately from the
operation's actual start time. `TIME-001` does not decide queue lateness or
missed-schedule policy.

### 3.4 Deadlines

Timeouts are expressed as `Duration`, but the generated runtime measures them
against a monotonic source. Wall-clock changes therefore do not lengthen or
shorten an in-flight timeout. Authored code never serialises or compares a
monotonic reading.

When a deadline must survive a process restart, the owning subsystem persists
an absolute UTC `Instant` plus the contract from which it was computed. On
resumption, it compares that deadline with the new activation's wall snapshot
and uses monotonic time for any new in-process wait. The async and workflow
plans remain responsible for retry and wake-up semantics.

### 3.5 Zone enum, resolved `Time`, and database query bounds

`Zone` is a compiler-generated enum over the pinned canonical IANA zone set.
Source uses checked, discoverable values:

```text
Zone.europe_london
Zone.america_new_york
Zone.asia_tokyo
```

Each variant maps to its canonical wire/storage identifier such as
`Europe/London`. `BST`, `GMT`, and numeric offsets are not variants: an
abbreviation is ambiguous and an offset cannot describe future daylight-saving
transitions. Known IANA links may remain deprecated compatibility aliases, but
new source and canonical output use the current canonical variant.

Producing a human-zone view from an instant is always unambiguous:

```text
var london_time = temporal.in_zone(
    clock.now,
    Zone.europe_london
)
```

Resolving human-local input names every missing fact:

```text
var cutoff = temporal.resolve(
    date: CalendarDate("2026-10-25"),
    at: "09:30",
    zone: Zone.europe_london,
    overlap: LocalOverlap.reject,
    gap: LocalGap.reject
)
```

The result is `Time`, containing the resolved instant and zone. If the clock
fragment occurs twice or does not exist, the default result is a typed
resolution failure. A caller selects another overlap or gap policy only where
product intent requires it; no runtime or database default may guess.

Because `Time` is resolved, it may be ordered directly with `Instant`:

```text
if clock.now < cutoff {
    // Before 09:30 London time on the named date.
}
```

A query for a local day first derives a half-open instant range:

```text
var bounds = temporal.day_bounds(
    CalendarDate("2026-10-25"),
    Zone.europe_london
)

var orders = attempt orders_created_between(bounds.start, bounds.end)
```

The generated predicate is `created_at >= start and created_at < end`. It stays
correct when a London day contains 23, 24, or 25 hours and never depends on the
database session zone or a `DATE(timestamp)` conversion.

An application normally persists `Instant`. A `Time` field is appropriate only
when retaining the chosen human zone is domain data; its persistence mapping
contains the instant plus canonical zone. Recurrence and cron execution remain
separate later concepts and are not implied by `Time`.

### 3.6 Temporal standard library

The compiler owns one portable temporal library. Application code must not
select a host package or depend on JavaScript, database, or operating-system
date behaviour directly. `temporal` follows the language-wide callable
ownership rule in the [naming and qualification
contract](naming-and-qualification.md): authored free callables are unqualified in their module/import
scope, entity operations are qualified by their entity (or receiver), and
compiler standard-library families are always qualified by a lowercase domain
namespace. Thus `temporal.in_zone(...)` is the same category as
`collection.count(...)`, not a one-off exception or method call.

The bounded v0.1 surface includes:

- construct and strictly validate `Instant`, `CalendarDate`, resolved `Time`,
  `Zone`, and `Locale` values;
- order `Instant` and resolved `Time` by underlying instant;
- add or subtract fixed `Duration` values from `Instant`;
- calculate the `Duration` between two instants;
- inspect year, month, day, weekday, hour, minute, second, offset, and zone identity
  through `Time`;
- add days or weeks to a `CalendarDate`;
- add months or years to a `CalendarDate` only with an explicit
  `InvalidDay.reject` or `InvalidDay.last_valid_day` policy;
- resolve a `CalendarDate`, clock fragment, and `Zone` into `Time` using explicit
  gap/overlap policy;
- add exact elapsed duration or explicit local calendar movement to `Time`
  without conflating the two;
- derive half-open day, week, month, and year bounds as instants for a zone; and
- format typed values for humans using the checked presentation API below.

There is no catch-all dynamic operation, natural-language parser, mutable date
object, or implicit conversion between civil values and instants.

The API follows one consistency grammar:

- every operation lives under the lowercase `temporal` namespace;
- standard-library namespaces cannot be imported open or aliased to create a
  second unqualified spelling;
- the value being transformed or inspected is the first positional parameter;
- environmental context follows in the stable order `zone`, then `locale`;
- policies, precision, rounding, and presentation choices are named options at
  the end;
- ranges are always `start`, then exclusive `end`;
- `between(start, end)` means `end - start`;
- `add_elapsed` means exact duration while `add_local_*` means calendar
  movement in the value's retained zone;
- `parse_*` is reserved for explicit untyped-ingestion outcomes; ordinary
  checked boundaries decode automatically;
- `format` is one-way presentation and never a parser; and
- the language does not offer method/free-function duplicates, synonyms such
  as `plus`/`advance`/`shift`, reversed overloads, or boolean policy flags.

The complete authored v0.1 function surface is:

```text
temporal.in_zone(instant, zone) -> Time
temporal.resolve(date:, at:, zone:, overlap:, gap:) -> Time

temporal.add_elapsed(value, duration) -> same type as value
temporal.between(start, end) -> Duration

temporal.add_days(date, days:) -> CalendarDate
temporal.add_weeks(date, weeks:) -> CalendarDate
temporal.add_months(date, months:, invalid_day:) -> CalendarDate
temporal.add_years(date, years:, invalid_day:) -> CalendarDate
temporal.add_local_days(time, days:, overlap:, gap:) -> Time
temporal.add_local_weeks(time, weeks:, overlap:, gap:) -> Time
temporal.add_local_months(time, months:, invalid_day:, overlap:, gap:) -> Time
temporal.add_local_years(time, years:, invalid_day:, overlap:, gap:) -> Time

temporal.day_bounds(date, zone) -> InstantRange
temporal.week_bounds(date, zone, starts_on:) -> InstantRange
temporal.month_bounds(date, zone) -> InstantRange
temporal.year_bounds(date, zone) -> InstantRange

temporal.calendar_date(time) -> CalendarDate
temporal.year(time) -> Int
temporal.month(time) -> Int
temporal.day(time) -> Int
temporal.weekday(time) -> Weekday
temporal.hour(time) -> Int
temporal.minute(time) -> Int
temporal.second(time) -> Int
temporal.millisecond(time) -> Int
temporal.offset(time) -> Duration
temporal.zone(time) -> Zone
temporal.same_zone(left, right) -> Bool
temporal.same_local(left, right) -> Bool

temporal.format(value, locale:, style:) -> PresentationText
temporal.format(value, locale:, components:) -> PresentationText
temporal.format_friendly(value, relative_to:, locale:, profile:) -> PresentationText
```

Here, `value`, `start`, and `end` accept `Instant` or resolved `Time` where the
signature's result remains unambiguous; `add_elapsed` retains a `Time` value's
zone. `at` is strict ISO local clock text (`HH:MM`, optionally seconds and
milliseconds) accepted only in the contextual `resolve` position. It never
becomes a storable, comparable, or authoritative value. A non-literal `at`
value is checked at runtime before resolution and failure remains visible to
the caller.

The policy sets are closed enums:

```text
LocalOverlap.reject | LocalOverlap.earlier | LocalOverlap.later
LocalGap.reject | LocalGap.shift_forward | LocalGap.shift_backward
InvalidDay.reject | InvalidDay.last_valid_day
Weekday.monday | Weekday.tuesday | Weekday.wednesday | Weekday.thursday
Weekday.friday | Weekday.saturday | Weekday.sunday
```

`earlier` and `later` choose the corresponding instant when a local clock value
occurs twice. Gap shifting preserves the requested minute/second position by
moving it by the exact timezone-transition gap; it does not snap to an
arbitrary boundary. Every resolution and local-calendar movement names both
overlap and gap policy explicitly, including `reject`; there is no hidden DST
default. `week_bounds` likewise names the first weekday instead of consulting a
host locale.

`Instant == Instant`, `Time == Time`, and cross `Instant == Time` equality use
the underlying instant, as do ordering and `between`. `same_zone` compares
retained zone identity. `same_local` compares local calendar and clock
components without treating that match as timeline equality. Local-period
bounds return `{ start: Instant, end: Instant }` with an exclusive end and fail
if the requested civil period has no representable instant in the pinned tzdb.

The two `format` lines are one checked function with mutually exclusive named
forms, not overloads: exactly one of `style` or `components` is required.
Multi-argument construction and policy operations use named arguments so source
does not depend on memorising the order of several values with similar types.
The compiler, LSP, and generated reference expose only these canonical
spellings. Adding another authored temporal function or alias requires contract
review rather than a target-library convenience wrapper.

### 3.7 Human-readable formatting

Canonical transport and persistence values remain machine-readable. Formatting
is a separate, one-way presentation operation that returns compiler-classified
`PresentationText` for web pages, server-rendered HTML, email, documents, and
exports.

Named styles cover ordinary use:

```text
temporal.format(
    temporal.in_zone(order.created_at, Zone.europe_london),
    locale: Locale.en_gb,
    style: TimeFormat.date_time_long
)
```

An example result is:

```text
27 September 2026 at 14:30 BST
```

Changing only the locale may produce a different ordering, month name, and
hour cycle. Changing the zone may produce a different local time. Locale and
zone are therefore independent typed inputs; the server's locale and timezone
are never silent defaults.

The closed named set is `date_full`, `date_long`, `date_medium`, `date_short`,
`time_full`, `time_long`, `time_medium`, `time_short`, `date_time_full`,
`date_time_long`, `date_time_medium`, and `date_time_short`. When product copy
needs a more specific shape, source uses structured, compiler-checked
components rather than a cryptic pattern string:

```text
temporal.format(
    temporal.in_zone(order.created_at, viewer.zone),
    locale: viewer.locale,
    components: {
        weekday: short
        day: numeric
        month: short
        year: numeric
        hour: two_digit
        minute: two_digit
        zone_name: short
    }
)
```

The application declares its supported locales so data inclusion and fallback
are bounded. An unsupported locale rejects or follows an explicitly declared
fallback; it never inherits the deployment host locale. The generated audit
records supported locales plus timezone- and locale-data provenance/version.

The initial declaration shape is:

```text
locales {
    default: "en-GB"
    supported: ["en-GB", "en-US", "fr-FR"]
    unsupported: fallback_to_default
}
```

It generates `Locale.en_gb`, `Locale.en_us`, and `Locale.fr_fr`. Rejecting an
unsupported locale instead is expressed as `unsupported: reject`; there is no
implicit host-locale fallback.

Human-formatted text is not round-trippable input. It cannot be assigned to an
`Instant`, used for time ordering, or stored as the authoritative value.
Strict constructors and boundary decoders remain the only parsing path.

Friendly output is also supported, but it must receive an explicit reference
instant and profile so results do not change through hidden `now` or threshold
rules:

```text
temporal.format_friendly(
    temporal.in_zone(message.sent_at, viewer.zone),
    relative_to: clock.now,
    locale: viewer.locale,
    profile: FriendlyTimeFormat.conversational
)
```

Depending on the declared profile and values, results may include:

```text
3 minutes ago
Today at 14:30
Tomorrow at 09:00
Monday at 16:15
27 September 2026 at 14:30 BST
```

The frozen conversational profile behaves as follows:

- less than one elapsed minute uses localized “just now” for the past and “in
  under a minute” for the future;
- less than one elapsed hour uses whole localized minutes, rounding past values
  down and future values up so it never claims an event has already occurred;
- after that, the same local calendar date uses “Today at” plus short time;
- the immediately previous or next local date uses “Yesterday at” or “Tomorrow
  at” plus short time;
- two through six local calendar days away uses localized weekday plus short
  time; and
- every other value uses `TimeFormat.date_time_long` with the short zone name.

“Today”, “tomorrow”, and weekday selection compare calendar dates in the
`Time` value's zone, not elapsed 24-hour blocks. v0.1 has no custom friendly
profiles; adding one requires new product pressure and a separately reviewed
contract rather than authored prose or opaque pattern strings.

The explicit `relative_to` instant makes tests deterministic and prevents two
fields rendered in one operation from disagreeing around a clock boundary. The
renderer can also preserve the canonical instant in semantic HTML or equivalent
machine metadata while showing friendly `PresentationText` to the user.

Natural-language input such as “next Friday” and general translated application
messages remain outside the v0.1 surface unless a frozen application supplies
concrete pressure.

### 3.8 Enforcement from boundary to presentation

The author should not have to remember a storage or wire-format checklist. The
compiler and generated adapters enforce one typed path:

```text
wire text
  -> generated strict decoder
  -> trusted time/date value
  -> typed application/query operations
  -> generated persistence parameter
  -> validated database value
  -> explicit formatter
  -> PresentationText
```

Each type has one boundary representation:

| Type | Accepted wire form | Canonical output | Important rejection |
| --- | --- | --- | --- |
| `Instant` | RFC 3339 with `Z` or numeric offset | UTC with `Z` and milliseconds | no offset, zone abbreviation, or local timestamp |
| `CalendarDate` | ISO `YYYY-MM-DD` | ISO `YYYY-MM-DD` | timestamp or locale-formatted date |
| `Time` | `{ "instant": <Instant>, "zone": <Zone> }` | same object with canonical members | zone-less clock fragment or inconsistent pair |
| `Zone` | canonical IANA identifier decoded to a known enum value | canonical identifier | unknown identifier, abbreviation, or numeric offset |
| `Locale` | one application-declared BCP 47 tag | declared canonical tag | host-default or undeclared locale |
| `Duration` | ISO 8601 `PT` form containing only hours, minutes, seconds, and at most millisecond fractions | normalized `PT` form | days, weeks, months, years, or excess precision |

Generated route, configuration, event, service, and database adapters decode
before an authored action can run. Invalid values produce the owning boundary's
typed safe failure. Business code receives no raw time/date strings and cannot
construct a trusted time/date value through a cast.

The type checker then prevents category errors:

- `Text` cannot satisfy `Instant`, even if its contents look correct;
- `CalendarDate` cannot be compared with `Instant`;
- a clock fragment is valid only as the checked `at` argument of
  `temporal.resolve`, beside a `CalendarDate`, `Zone`, and resolution policy;
- resolved `Time` may be ordered with `Instant` because it already contains
  one, while its zone/local representation is inspected separately;
- `PresentationText` may flow to declared presentation sinks but not time/date
  constructors, database predicates, indexes, schedules, or authority fields;
  and
- an API may deliberately expose both a canonical `Instant` and a display
  label, but the latter remains visibly presentation-only in schemas and audit.

Generated OpenAPI, clients, LSP completion, hover, and diagnostics show the
accepted form and offer task-specific repair. An offset-free timestamp error,
for example, explains: use `CalendarDate` for a calendar day, add an offset for
an exact instant, or use `temporal.resolve` with a date, clock fragment, and
`Zone` enum value for local intent. This makes the safe
path the easiest path rather than requiring prior timezone expertise.

### 3.9 Compiler-owned lifecycle timestamps

Creation and modification time are common enough to be semantic field roles,
but they must be explicit rather than inferred from names:

```text
entity Todo {
    created_at: Instant generated {
        on: create
    }

    updated_at: Instant generated {
        on: create_or_change
    }
}
```

This is the initial canonical source shape. Fixture design may simplify
punctuation only if it produces the same semantic nodes and audit facts; any
semantic change returns to owner review. The semantics are fixed:

- the fields are excluded from caller input and patch types;
- authored create/update bodies cannot assign them;
- the compiler supplies the top-level operation's stable `clock.now`;
- `created_at` is immutable after insertion;
- `updated_at` changes only when persisted semantic state actually changes;
- create sets both fields to the same operation time;
- nested writes in one operation share the same instant;
- the generated SQL binds explicit instant parameters; and
- output, audit, migration, and schema metadata identify the compiler-owned
  field role.

Jadpo-managed schema generation does not emit `DEFAULT CURRENT_TIMESTAMP`,
`ON UPDATE`, generated time expressions, or triggers for these domain fields.
Drift inspection rejects an existing database default/trigger that competes
with compiler ownership. This keeps SQLite/PostgreSQL behaviour, application
validation, audit records, and deterministic tests on the same clock.

Database-generated identity and database-internal metadata are separate issues;
this rule concerns application-visible domain time. An externally managed
database that requires trigger-owned timestamps would need a future explicit
adapter contract with weaker guarantees. v0.1 does not silently accommodate it.

Other timestamps do not become automatic merely because of their names.
`deleted_at`, `published_at`, `paid_at`, provider occurrence time, and similar
facts belong to explicit lifecycle actions or external contracts. The compiler
may supply operation time once that semantic event is declared, but it must not
guess the event from a suffix.

A migration adding a non-null historical timestamp cannot use “now” as an
unreviewed backfill. It must choose one visible disposition: keep the field
optional for legacy rows, derive it from named existing authority, use a
reviewed recorded constant, or stop for a lifecycle decision.

### 3.10 Database reads become Jadpo values

Database rows are untrusted boundary input. The generated query plan knows the
declared type of every selected column and its projection, so its adapter must
decode and validate the complete row before returning it to authored code.

For example:

```text
query latest_order(id: Order.id) -> Order freshness authoritative {
    where: id == id
}
```

If `Order.created_at` is `Instant`, then `latest_order(...).created_at` is
already a trusted `Instant`. The author does not parse a string, attach UTC,
choose a timezone, or handle a JavaScript `Date`.

The initial mappings are compiler-owned:

| Jadpo type | PostgreSQL authority mapping | SQLite authority mapping | Adapter result |
| --- | --- | --- | --- |
| `Instant` | `timestamptz` containing a millisecond-aligned instant | integer epoch milliseconds | `Instant` |
| `CalendarDate` | `date` | canonical ISO date storage | `CalendarDate` |
| `Time` | millisecond-aligned `timestamptz` plus checked canonical-zone text | epoch milliseconds plus checked canonical-zone text | resolved `Time` |
| `Zone` | checked canonical IANA text | checked canonical IANA text | known `Zone` enum value |
| `Locale` | checked text from the declared locale set | checked text | `Locale` |
| `Duration` | checked integer elapsed milliseconds | checked integer elapsed milliseconds | `Duration` |

The generated adapter validates database kind, nullability, range, precision,
and semantic form. A required timestamp returned as null, an invalid zone, an
out-of-range epoch, a zone-less database timestamp mapped as an `Instant`, or
unexpected sub-millisecond precision is a contained `PersistenceFault`. It
does not become a partially trusted application value.

Jadpo-managed writes always use the matching canonical representation, so this
failure normally indicates schema drift, corruption, or an unauthorised
external writer. An imported legacy `timestamp without time zone` cannot be
guessed into an instant; its adapter or migration must name the source zone and
gap/overlap policy explicitly.

Typed projections preserve these rules. Selecting, joining, ordering, or taking
`min`/`max` of an `Instant` remains typed as `Instant`; extracting a civil date
requires an explicit `Zone`. Raw database formatting or session-timezone casts
cannot leak a `Text` timestamp into an otherwise typed query result.

Once decoded, the same value can be compared, manipulated, serialized to its
canonical wire form, or explicitly formatted for humans through the time
standard library. The host target's native date APIs remain an implementation
detail below the adapter boundary.

## 4. Test model

### 4.1 Test layers

The evidence layers are intentionally distinct:

| Layer | Exercises | Does not prove |
| --- | --- | --- |
| Compile fixture | parsing, semantic graph, types, effects, diagnostics | generated runtime behaviour |
| Pure authored test | deterministic business computation | adapters or transport boundaries |
| Direct callable test | generated callable execution and declared outcomes | HTTP/auth/serialization or scheduler behaviour |
| Route test | real generated listener and full request boundary | scheduler or external production service behaviour |
| Job activation test | generated job entry, fixture clock, leases/checkpoints when defined | production queue availability |
| Adapter conformance test | real adapter contract, fault normalisation, recovery | a managed provider's operational reliability |
| External trial | human or deployed-system evidence | static semantic proof |

The test report and generated audit keep these labels. One layer never silently
substitutes for another exit gate.

### 4.2 Fixtures and isolation

A fixture is a typed, checked description of initial capabilities and state. It
may provide:

- a fixed UTC clock and deterministic monotonic origin;
- typed configuration values;
- real isolated database state;
- declared principal/credential inputs at the boundary being tested;
- finite declared outcomes for service or authentication fakes; and
- a deterministic entropy byte stream for compiler-owned operations.

Every test starts from its fixture independently. Mutable state persists across
steps inside that test so sequences can exercise revocation, retries, or
partial progress, but it never leaks into another test.

The exact punctuation is frozen through positive and negative fixtures before
parser changes. The required semantic shape is:

```text
fixture todo_at_noon {
    clock: fixed Instant("2026-01-15T12:00:00Z")

    config {
        mail_sender: Email("reminders@example.test")
        mail_api_key: secret("test-only-value")
    }

    database primary {
        // typed rows validated against the checked schema
    }

    service ReminderMail: fake {
        send => accept ReminderReceipt {
            accepted_at: Instant("2026-01-15T12:00:00Z")
        }
    }
}
```

Negative fixtures must be rejected when they name undeclared capabilities,
construct invalid nominal values, provide unknown database fields, configure a
service outcome the declaration cannot produce, or attempt to reveal a secret.

### 4.3 Invocation surfaces

The existing pure form remains canonical:

```text
test "addition preserves precedence" {
    var result = add(Number(2), Number(3 * 4))
    assert result == Number(14)
}
```

Integration punctuation must preserve an explicit boundary marker. Candidate
shapes are:

```text
test "past due date is rejected" using todo_at_noon {
    var response = request POST "/todos" {
        authentication: alice_session
        json: { title: "Late", due_at: "2026-01-15T11:59:59Z" }
    }

    assert response.status == 422
    assert response.failure.code == "invalid_input"
}
```

```text
test "reminder action uses the explicit cutoff" using todo_at_noon {
    var outcome = call send_due_reminders(
        Instant("2026-01-15T12:00:00Z")
    )

    assert outcome succeeded
    assert calls(ReminderMail.send) == 1
}
```

```text
test "overdue reminder activation" using todo_at_noon {
    var run = activate job overdue_reminders {
        scheduled_at: Instant("2026-01-15T12:00:00Z")
    }

    assert run succeeded
}
```

These are accepted semantic shapes. TIME/TEST-P3 freezes their minimal grammar
through positive and negative fixtures before implementation, without merging
the three evidence surfaces or changing the approved contract.

### 4.4 Clock control

The fixture starts at a fixed instant. A test may advance time explicitly:

```text
advance clock by 5m
```

Advancing updates both the next wall snapshot and the monotonic test source by
the same duration. Code already executing retains its stable operation-time
snapshot; the next request, direct effectful call, job activation, retry, or
workflow resumption observes the advanced value. Tests never wait for real
time or depend on scheduler sleeps.

Backward wall-clock movement and independent monotonic advancement are reserved
for compiler/runtime conformance tests, not ordinary application tests. They
remain available in the lower-level runtime harness to prove deadline safety.

### 4.5 Capability fakes and traces

A service fake chooses only outcomes declared by the checked service contract.
It may define a finite sequence such as timeout then success, but cannot run an
arbitrary Jadpo callback in place of the adapter. The harness validates request
values before the fake adapter attempt and validates the selected response
before returning it.

The implementation attaches a finite fake queue to each authored test's
operation context. Nested callable invocations keep that test's queue, while a
new test receives fresh queue cursors and synthetic monotonic elapsed time. The
generated service wrapper validates the complete typed request before calling
the adapter. The adapter consumes the declared outcome, applies the same
attempt budget, deadline, retry rules and failure classifier as live transport,
and the generated wrapper validates an accepted response against the declared
output type. Queue exhaustion and malformed runtime receipts become
`OutcomeUnknown`; request/credential validation happens before a queue entry is
consumed. Test fakes never select a host or open a provider connection.

Runtime coverage runs the authored fixture suite twice with the transport
connector replaced by a fail-fast counter. It checks accepted, rate-limited
retry, three pre-dispatch timeouts, malformed acknowledgement and possible
dispatch timeout, along with deterministic elapsed values, per-test isolation
and credential redaction. Direct adapter cases verify invalid keys do not
consume fake state and exhausted/malformed queues resolve to unknown outcome.
These checks establish fixture integration and adapter behavior; the separate
HTTP/TLS adapter suite remains the evidence for actual loopback request handling.

**RM-304 closure, 2026-10-04:** the
[independent correction review](../tests/validation/rm304-independent-correction-review.json)
accepts empty/missing checked authored fake containment and fresh-operation elapsed
baselines. The baseline is keyed by shared attempt-budget identity, so cloned and
nested contexts keep their limits while a new operation resets its relative
elapsed budget without rewinding the queue. The registered suite passes 7/7
(40 assertions), and the [fresh supported gate](../build/validation/20261004T084327-70967/report.json)
passes 61/61. This scope excludes malformed host-injected nullish map entries and
direct-host receipt validation seams identified by the reviewer; checked authored
lowering cannot construct those inputs. Broader capability and job integration
remains open. Timing and completion are linked from [history](implementation-history.md#2026-10-04--rm-304-checked-authored-service-fakes).

Typed traces expose only reviewable facts:

- capability and operation identity;
- ordinal and operation-time snapshot;
- validated non-secret request fields;
- selected declared outcome; and
- idempotency identity or compiler-owned checkpoint when the contract has one.

Secret fields are represented only as redacted presence/classification facts.
Raw provider objects, native errors, SQL text, credentials, and stack traces are
never test assertion surfaces.

## 5. Generated versus authored ownership

The compiler may generate boundary tests directly from facts it already owns,
including malformed-input rejection, closed output schemas, default
authentication, declared failure envelopes, nominal validation, and adapter
row validation. Such tests demonstrate that generated runtime code implements
the compiler's graph.

The author owns examples whose expected result expresses business intent:

- which due dates are permitted;
- lifecycle sequences;
- pricing or scheduling results;
- provider-outcome decisions;
- idempotency expectations; and
- user-visible behaviour across multiple steps.

The generated audit reports these categories separately:

```text
tests:
  semantic_checks: ...
  generated_runtime_cases: ...
  authored_business_cases: ...
  external_evidence: ...
```

Counts are inventory, not a coverage percentage or assurance score.

## 6. Diagnostics and fail-closed rules

Implementation fixtures must define stable authored diagnostics for at least:

- clock access from a pure function or query;
- `default now` in a type or field declaration;
- offset-free, over-precision, or out-of-range `Instant` values;
- unknown, deprecated, or non-canonical `Zone` enum and locale values;
- implicit civil-time-to-instant conversion;
- ambiguous or nonexistent local time without an explicit permitted policy;
- direct `Instant` formatting without first producing a zone-aware `Time`, or
  formatting without a supported locale;
- friendly formatting without explicit `relative_to` and profile inputs;
- reversed time-operation arguments, unnamed ambiguity policies, or use of a
  non-canonical synonym absent from the standard namespace;
- an opaque or invalid custom formatting pattern;
- use of `PresentationText` as time/date input, authority storage, or comparison;
- an implicit conversion between `Text` and a time/date type;
- caller or authored-action assignment to a compiler-owned lifecycle timestamp;
- a database current-time default, generated expression, or trigger competing
  with a compiler-owned domain timestamp;
- an unreviewed “now” backfill for historical rows;
- a database time/date column whose kind, nullability, range, precision, or
  value contradicts the checked result type;
- an imported zone-less timestamp mapped to `Instant` without an explicit zone
  and ambiguity policy;
- unsupported recurrence, business-calendar, or natural-language operations;
- a fixture referring to an undeclared capability;
- invalid typed configuration or database fixture data;
- a fake outcome absent from the service contract;
- an attempt to expose or assert a secret value;
- a direct invocation claimed as route or scheduler evidence;
- shared mutable fixture state between tests;
- use of real waits or host timer controls in authored tests; and
- a generated test presented as policy proof or authored acceptance evidence.

If the compiler cannot classify the invocation surface or isolate the fixture,
the test must not run under a weaker mode.

## 7. Required evidence

TIME-001 and TEST-001 implementation is complete only when evidence covers:

1. offset normalisation and canonical millisecond output across Bun, SQLite,
   and PostgreSQL;
2. London and at least one non-European daylight-saving gap and overlap,
   including explicit rejection and every supported resolution policy;
3. 23-, 24-, and 25-hour local-day bounds producing correct half-open instant
   database predicates;
4. complete generated `Zone` enum coverage, canonical wire round trips,
   deprecated IANA-link compatibility, and reviewed tzdb-version changes;
5. signature-level conformance proving every `temporal` function follows the
   namespace, naming, primary-value-first, context-order, and named-policy
   contract without synonyms or reversed overloads;
6. named and structured formatting across the declared locales, zones, hour
   cycles, and timezone-name options without host-default leakage;
7. friendly formatting around minute, midnight, adjacent-day, weekday,
   DST-transition, threshold, and absolute-fallback boundaries using an
   explicit fixed reference instant;
8. identical formatting for the same declared data versions across supported
   targets, with provenance in the generated audit;
9. strict wire decoding and canonical output for every time/date type at route,
   configuration, event, service, and database boundaries;
10. rejection of formatted text as time/date input or authority storage;
11. generated create/change timestamps sharing operation time, remaining
   caller-inaccessible, and not advancing on a no-op;
12. schema generation and drift inspection rejecting competing database
   defaults, expressions, and triggers;
13. database reads decoding directly into Jadpo types across scalar, nullable,
    projection, join, aggregate, and round-trip cases without host date objects;
14. nullability, corruption, precision, range, zone, schema-drift, and legacy
    zone-less timestamp failures being contained as `PersistenceFault`;
15. migration cases proving historical timestamps are never invented silently;
16. stable repeated `clock.now` values throughout a nested operation;
17. a new snapshot for the next request, retry, or activation;
18. wall-clock movement not affecting an in-flight monotonic timeout;
19. explicit time cutoff parameters in named queries;
20. rejection of ambient host/database clocks and `default now`;
21. deterministic clock advance without real sleeps;
22. per-test database and capability isolation under serial and parallel runs;
23. real SQLite plus backend-specific PostgreSQL adapter tests;
24. typed service/auth fake outcomes and secret-safe traces;
25. direct-call, route, and job-boundary evidence reported separately;
26. deterministic repeat runs with identical source, fixture, seed, and target;
27. crash/restart hooks at compiler-owned checkpoints without monkey-patching;
    and
28. unchanged operation without a clock, fake, config, or entropy capability
    producing no corresponding generated runtime surface.

## 8. Implementation sequence after acceptance

### TIME/TEST-P0 — Freeze the contract

- record the accepted decisions and digest;
- align the golden todo, authentication, configuration, syntax, issue log,
  assurance model, and comparison baseline; and
- replace earlier ambient/default-time examples that contradict the contract.

### TIME/TEST-P1 — Values and semantic effects

- freeze canonical `Instant`/`Duration` validation and arithmetic;
- add the generated `Zone` enum, `Locale`, `CalendarDate`, and resolved `Time`
  semantics plus reviewed timezone-data evolution;
- implement bounded date manipulation, zone resolution, and local-period
  instant bounds with explicit ambiguity/overflow policies;
- implement the canonical `temporal` namespace and signature-conformance gate;
- implement named, structured, and friendly human formatting with declared
  locale support, explicit reference time, and audited data provenance;
- enforce type-specific boundary encodings, context restrictions, and
  presentation-text flow;
- add explicit compiler-owned create/change timestamp field roles;
- add the compiler-owned time-read effect and context rules;
- reject `default now` and unsupported ambient time sources; and
- emit clock usage and time/date parameters in semantic/audit output.

### TIME/TEST-P2 — Runtime clock and deadlines

- generate stable operation-time capture;
- thread it through nested actions and persistence parameters;
- generate lifecycle timestamp parameters without database time defaults or
  triggers and reject competing schema drift;
- decode and validate database time/date columns directly into Jadpo types,
  including typed projections and aggregates;
- implement internal monotonic deadlines; and
- prove cross-target precision and normalisation.

### TIME/TEST-P3 — Typed fixtures and isolation

- freeze minimal fixture punctuation through compile cases;
- implement fixed/advanceable clock, typed config, isolated real database, and
  deterministic entropy capabilities; and
- reject secret disclosure and undeclared fake surfaces.

### TIME/TEST-P4 — Invocation and declared fakes

- implement direct callable, HTTP route, and job activation boundaries;
- implement finite service/auth fake outcomes and typed traces; and
- label evidence surfaces in the versioned test report and audit.

### TIME/TEST-P5 — Adversarial exit run

- run repeatability, isolation, parallelism, crash, timeout, clock-adjustment,
  secret-canary, duplicate, and adapter conformance cases; and
- verify generated and authored evidence remains separately attributable.

## 9. Dependencies and stop conditions

This plan unblocks the clock and testability portions of AUTH-001, CONFIG-001,
SERVICE-001, ASYNC-001, and WORKFLOW-001. It does not choose their retry,
scheduling, provider, or workflow semantics.

Stop and return to owner review if implementation would:

- expose monotonic time or host clock APIs to authored application code;
- introduce calendar/time-zone behaviour outside the bounded standard-library
  contract or delegate it to host defaults;
- accept an arbitrary source string where a `Zone` enum value is required or
  change canonical zone identity without reviewed compatibility handling;
- add a `temporal` synonym, reversed overload, inconsistent parameter order, or
  boolean policy flag outside the canonical namespace contract;
- let friendly formatting read ambient time or use an undocumented threshold;
- make the production clock application-selectable;
- permit raw time/date strings or implicit time/date conversions in business
  code;
- let callers, authored mutations, database defaults, or triggers own a
  compiler-managed lifecycle timestamp;
- invent a historical timestamp during migration;
- add arbitrary callbacks, mocks, spies, or dependency injection;
- substitute an in-memory repository for real persistence semantics;
- allow a test to read a secret or raw adapter/provider value;
- use sleeps or timing races as deterministic evidence;
- merge direct-call evidence with route/job boundary evidence;
- treat generated tests as proof or human-owned intent; or
- add general property-test syntax without new application pressure.

## 10. Approval record

The project owner approved the section-2 contract on 2026-09-27 after reviewing
UTC/instant semantics, the zone-aware `Time` model, closed zone values, API and
database boundaries, compiler-owned lifecycle timestamps, manipulation naming,
absolute/friendly formatting, deterministic clocks, test isolation, capability
fakes, and evidence attribution. The digest above binds that accepted decision
table. TIME/TEST-P0–P5 are authorised for fixture-first unattended
implementation subject to the stop conditions in section 9.

Implementation record: the compiler now owns canonical `Instant`,
`CalendarDate`, `Duration`, resolved `Time`, generated `Zone` and declared
`Locale` values; the consistently qualified `temporal` surface; strict
boundary/database decoding; stable operation time; monotonic runtime
deadlines; generated lifecycle timestamps; operation-time authority change
records without database clock defaults; and explicit absolute/friendly
formatting. Typed fixtures provide isolated SQLite persistence, fixed and
advanceable clocks, typed configuration, secret-only fixture entry, callable
boundary invocation, and versioned evidence reports. Runtime evidence covers
London and New York gaps/overlaps, 23/24/25-hour days, calendar versus elapsed
arithmetic, explicit-reference formatting, invalid policies, and runtime
ICU/engine provenance. Route/job activation, service/auth fakes, crash/retry
checkpoints, live PostgreSQL, and identical pinned timezone-rule evidence remain
outside this core until their owning AUTH/SERVICE/ASYNC adapters or external
test environment exist.

The emitted temporal provenance distinguishes the compiler's closed zone
identifier enumeration from runtime transition rules. Its legacy `tzdb` label
is explicitly scoped to that enum; the enum has its own version and exact source
byte digest. `Intl.DateTimeFormat` supplies runtime rules, with the genuine
`process.versions.tz` value recorded only when the host reports it, otherwise
`unreported`. Engine and ICU release reports remain separate, and runtime rule
bytes have no claimed digest. This additive metadata does not change zone
acceptance, arithmetic, formatting or the accepted TIME decisions. Identical
pinned timezone-rule evidence remains outside this checkpoint.

The [scoped implementation evidence](../tests/validation/evidence/rm504-temporal-provenance-distinction/report.json) records the actual engine reports, source/enum/compiler pins, metadata-only emitted-byte comparison and independent-review status.

The [independent PostgreSQL policy review](../tests/validation/evidence/rm504-independent-policy-runtime-382/review.json) qualifies nine imported generated policy cases on both adapters, with stored denial/revocation checks and equal observations. The [credential restart review](../tests/validation/evidence/rm504-independent-service-restart/review.json) qualifies actual same-store child validation on both adapters. These finite database gaps are resolved in their maintained fixtures; authentication/main-entry scope, local-day database predicates and broader typed temporal evidence remain separate.

### Initial Instant/Duration schema gate — bounded plan, 2026-10-06

The independently reviewed [typed temporal checkpoint](../tests/validation/evidence/rm504-independent-temporal-decoder-502/review.json) preserves the native PostgreSQL Duration string decoder correction and both-adapter generated handler roundtrips. Its original diagnostic establishes two remaining physical-schema gaps: an existing Duration TEXT column accepts canonical numeric/wire values, and an existing six-digit Instant column is truncated by the driver before typed decoding. Those observations do not qualify TIME-D25.

The next [bounded plan](../tests/validation/rm504-temporal-schema-gate-plan.json) proposes compiler-private initial/recovery catalog validation for persisted Instant/Duration kind, Instant declared precision and nullable shape before readiness publication, under the existing transaction and deadline owner. PostgreSQL uses actual int8/timestamptz(3) catalog identity; SQLite checks INTEGER affinity. Projects without these stored types and private delivery metadata are outside the generated check. Planning is saved; implementation awaits different-author plan assessment. Actual settings remain gpt-6.1-sol/high; preparation forecast0.25–0.75h and implementation forecast0.5–2h, with run RM-504-1f5d29eeeb86.

After-ready concurrent schema drift, broader stored corruption/precision/default/trigger/migration and CalendarDate/Time/Zone/Locale mapping gates remain open. This initial gate cannot claim full per-query TIME-D25, authentication, local-day database predicates, or runtime Intl rule-byte qualification.

### Checked local-day query prerequisite — bounded plan

The [preserved prerequisite](../tests/validation/rm504-temporal-day-predicate-prerequisite.json) now includes a planning-only proposal to extend existing `query page` clauses with required non-nullable `Instant >= start` and `Instant < end`, allowing only the same-field lower/upper pair. Existing query-many limits, optional filters, typed nominal authority, policy scope and cursor semantics stay explicit. Independent language-plan review selected direct required input-field operands and strict target-field nominal compatibility; isolated implementation is underway. No integrated range parser or native database qualification is claimed. The proposed native tests compose generated day bounds with generated typed queries over 23-, 24- and 25-hour days.

The [independent initial schema-gate review](../tests/validation/evidence/rm504-independent-temporal-schema-e633/review.json) approves e633 separately from the88-check fusion: SQLite 10/64 and PostgreSQL 13/92 pass; the same original-compiler cases admit incompatible declarations and fail the expected refusal. Actual catalog execution held before acknowledgement ends at 10.034s under the existing native owner, with 10 connections closed, zero backend owners and 903 HTTP polls observing no listener. Its initial/recovery Instant/Duration kind, precision and nullability gate now passes the [92-check combined gate](../tests/validation/evidence/rm402504-query-schema-public-fusion/report.json) alongside public retry and query-page leaves. After-ready drift, SQLite per-row/CHECK ownership and the wider TIME-D25 matrix remain open.

The [required query-page range review](../tests/validation/evidence/rm504-independent-required-page-range-885/review.json) approves the narrow source slice for one same-field half-open stored Instant range, supplied by direct required input fields with the target field's declared type. Independent actual generated handlers pass SQLite 5/136 and PostgreSQL 5/137: London 23/24/25-hour dates, boundaries, ties, pagination, repeat/reverse order, empty ranges and invalid ingress are observed with native SQL and physical catalogs. Changing only the generated upper SQL comparison from `<` to `<=` fails all three date controls on both adapters. Source-level `day_bounds` callable grammar, generic range expressions, cross-request snapshot semantics, exact Intl rule bytes and wider TIME-D25 remain outside this bounded leaf. The source passes that same 92-check combined gate.

### RM-504 current stored and HOST checkpoint — 2026-10-06

The [private Duration correction review](../tests/validation/evidence/rm504-duration-storage-fallback/independent-review/review.json) and [current96-check gate](../tests/validation/evidence/rm402-startup-duration-contention-reviewed-fusion/report.json) now refuse stored SQLite ISO Duration TEXT as contained data faults while preserving valid integer/canonical PostgreSQL representations and public ISOwire input. Original counterexamples remain archived. The [HOST independent review](../tests/validation/evidence/rm504-host-time-claim-matrix/native-execution/independent-review/review.json) passes original9/49 and bounded6/200 with five causal controls, but reproduces French future sub-minute wording and four accepted zone-name mapping failures. [Assessed follow-on plans](../tests/validation/evidence/rm504-host-time-claim-matrix/follow-on-plans/parent-plan-assessment.json) authorize narrow fixes and an existing CalendarDate projection native probe. These five diagnostic reds are separate from the96 gate. PGafter-ready microsecond narrowing, broader temporal decoding and unreported Intl runtime rulebytes remain unqualified.

### HOST fixes and page projection containment — 2026-10-06

The [HOST product review](../tests/validation/evidence/rm504-host-time-claim-matrix/product-fix-independent-review/review.json)
and [current98 gate](../tests/validation/evidence/rm402-audit-host-ordinary-reviewed-fusion/report.json)
qualify the declared Intl component-option mapping and Frenchfuture subminute
wording. Existing Duration/zone/threshold behavior is retained; effective locale
variation was inconclusive and exact runtimeICU/timezone rule bytes remain unqualified.
The [independent page probe](../tests/validation/evidence/rm504-page-projection-containment/independent-review/review.json)
reproduces admitted impossible CalendarDate projections on both adapters returning
safe500 with an uncontained ValidationError. The [assessed narrow fix](../tests/validation/evidence/rm504-page-projection-containment/follow-on-plan/parent-plan-assessment.json)
is in different-author review, preserving original authored evaluation order and
native storage faults outside the new data-decode catch. It is not selected in98;
cursor/join/aggregate/other temporal corruption and PGprecision claims remain separate.

The [current102 gate](../tests/validation/evidence/rm504601-page-implicit-reviewed-fusion/report.json) now selects the [independently approved corrected page wrapper](../tests/validation/evidence/rm504-page-projection-containment/product-fix-independent-review/review.json). Successful CalendarDate result decoding is contained while original effectful Int limit order and PostgreSQL driver500 classification are preserved. The [precision independent review](../tests/validation/evidence/rm504-after-ready-stored-corruption/residual-precision-plan/native-probe-independent-review/review.json) reproduces20 native cases/269 assertions in an experimental point-query copy; this is not original product coverage. The [assessed private carrier plan](../tests/validation/evidence/rm504-after-ready-stored-corruption/residual-precision-plan/product-plan/parent-assessment/assessment.json) requires untouched raw policy inputs, post-visibility decoding and actual selected-connection settings before any product correction. Whole RM-504 remains open.
