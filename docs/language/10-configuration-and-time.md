---
title: Configuration and time
area: 10
status: in-review
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - configuration.binding_empty
  - configuration.binding_invalid
  - configuration.binding_required
  - configuration.default_invalid
  - configuration.default_literal_required
  - configuration.default_type
  - configuration.duplicate_binding
  - configuration.duplicate_option
  - configuration.expected_field
  - configuration.multiple_declarations
  - configuration.secret_default
  - configuration.secret_flow
  - configuration.type_unsupported
  - configuration.unknown_option
  - type.clock_context
  - type.temporal_*
  - type.locale_*
  - syntax.locales_*
  - semantic.multiple_locale_declarations
  - type.presentation_text_persistence
---

# Configuration and time

How an application declares its settings and secrets, where their values come
from, and how code reads the current time, works with dates and zones, and
shows times to people in their own language.

> In review. Code outside the collapsed boxes applies the decisions from areas 1 to 5 and this page's recommendations; each recommendation names its open question. Today's compiled syntax is collapsed beneath each section that changes.

Two rules run through the page. **Code never reads the environment or the
system clock directly**: settings arrive as typed `config.<field>` values,
checked before the app starts, and the time arrives as `clock.now`, fixed for
the whole operation. **Machine values and text for people never mix**: an
`Instant` is stored and compared; a formatted date is only shown.

## Settings: `config.jadpo`

Every setting the application needs is a field in the one `config { }`
declaration in the root file `config.jadpo`. A field has a type, and
optionally `secret: true` or a `default:`. Code reads it as `config.<field>`
anywhere, functions included, because a setting cannot change while the app
runs.

```jadpo fragment
config {
    database_url: PostgresUrl
    session_signing_key: SessionSigningKey {
        secret: true
    }
    browser_origin: Url
    mail_sender: Email
    soft_delete_retention: Duration {
        default: 30d
    }
}
```

- The declaration has no name (decided in area 1: unnamed singletons).
- Each value comes from an environment variable named after the field in
  capitals: `mail_sender` reads `MAIL_SENDER` (question 1).
- A field is required unless it has a `default:`. There are no optional
  settings: give a default instead, or leave the feature's setting required.
- There is no `env.jadpo` and no second binding file (decided in area 1).

::: today

The declaration needs a name nobody uses, and every field repeats its own
name as `binding:`:

```jadpo project=today-config
config TodoConfiguration {
    mail_sender: Email {
        binding: "MAIL_SENDER"
    }

    soft_delete_retention: Duration {
        binding: "SOFT_DELETE_RETENTION"
        default: 30d
    }
}
```

:::

### Environment variable names

The variable name is the field name in capitals. Write `binding:` only when it
must differ, for example `binding: "HEROKU_POSTGRESQL_URL"` to keep a name a
hosting platform already uses.

Today every field must state `binding:`, and every binding in the repository
is the field name in capitals. The cost of deriving it: renaming a field
renames the variable, which breaks a deployment that still sets the old name.
The compiler already writes every binding to
`build/audit/configuration.json`, so a changed name can be shown in the
approval review like any other contract change (question 1).

### What a setting can hold

A setting is read from one environment variable, so its type must be decodable
from one piece of text: text-based types, `Int`, `Decimal`, `Bool`, `Uuid`,
`Email`, `Url`, `IpAddress`, `Duration`, `Instant` and `CalendarDate`. Lists
and objects are rejected (`configuration.type_unsupported`): split them into
fields.

Proposed: also accept enums, written by variant name, and the built-in enums
`Zone` and `Locale` (question 2):

```jadpo fragment
config {
    log_level: LogLevel {        // type LogLevel = Enum { debug info warning }
        default: info
    }
    business_zone: Zone {
        default: europe_london
    }
}
```

`LOG_LEVEL=debug` and `BUSINESS_ZONE=Europe/London` would then be valid
values. Today `zone: Zone` is `configuration.type_unsupported`.

### Defaults

A default is a literal of the field's type, checked while compiling
(`default: 25`, `default: 5s`). A secret cannot have one, so a forgotten
production secret stops the app instead of falling back to a value written in
source.

## Secrets

`secret: true` marks a value that must never leave the server. The compiler
tracks it through variables and calls, and accepts it only at places the
compiler itself owns: a store's `url:`, an integration's credentials (area 11)
and authentication signing keys (area 8). Returning it, storing it, logging
it, putting it in a failure, building another value from it or even comparing
it to something is `configuration.secret_flow`.

```jadpo fragment
action leak() -> ApiKey {
    return config.mail_api_key      // error: configuration.secret_flow
}
```

This works today, including comparisons (`key == config.api_key` is rejected,
because a yes/no answer leaks the secret one guess at a time).

## Database and other connection URLs

Area 5 decided that stores take their address from configuration:

```jadpo fragment
application {
    stores: {
        main: {
            engine: postgres
            url: config.database_url
        }
    }
}
```

Today that cannot work. The only URL type, `Url`, accepts `http://` and
`https://` only, so a real `postgres://user:password@host/db` is rejected as a
default and stops the app at startup (both verified). The golden todo app
declares `database_url: DatabaseUrl` with `type DatabaseUrl = Url {}` and
could never start with a PostgreSQL address; the generated runtime instead
reads `DATABASE_URL` from the environment directly, bypassing the declared
field (read from generated code).

Proposed (question 3): built-in connection types, one per store engine,
`PostgresUrl` (`postgres://` or `postgresql://`) and `RedisUrl` (`redis://`
or `rediss://`). They are always secret, because they normally carry a
password, and a store's `url:` must have the type matching its `engine:`.
The runtime opens the store only through that field.

## Values on your machine and in production

Production values come from the platform's environment (or its secret
manager, which sets the same variables). Locally there is one git-ignored
file, `.env.local`; Jadpo defines no `.env`, `.env.development` or layering.
Tests get their values from test fixtures (area 13).

A secret is never typed into a chat with an agent. The agent asks you to run:

```text
$ jadpo config set mail_api_key
Enter MAIL_API_KEY: [hidden]
✓ mail_api_key configured locally
```

The command reads the value from a hidden prompt (never a command-line
argument), checks it against the field's type, and writes `.env.local`
safely. `jadpo config check` lists each field as `set`, `default`, `missing`
or `invalid`, never the value. Both are tooling (area 14).

## Checking values before the app starts

The generated app reads only the declared variables, checks every value
against its type, and refuses to start if any is missing or invalid. It never
opens its port with bad configuration, and it reports the field and variable
name, never the value:

```text
{"kind":"configuration_diagnostic","code":"config_invalid","declaration":"App.database_url","binding":"DATABASE_URL"}
{"kind":"operational_log_event","eventName":"operation.failed","classification":"RUNTIME_STARTUP_FAILED", ...}
```

Verified under Bun 1.2.20 with missing, malformed and valid values. `jadpo
dev` runs the same check against `.env.local` and keeps the last good version
running when a change is invalid. A database that is configured correctly
but down is not a configuration error: the app starts, reports not ready and
recovers (health checks are area 11).

## Time types

| Type | What it is | Example | On the wire |
|---|---|---|---|
| `Instant` | One point on the timeline, in UTC, to the millisecond | when a todo was created | `"2026-01-15T12:00:00.000Z"` |
| `CalendarDate` | A date with no time or zone | a birthday, an invoice date | `"2026-10-25"` |
| `Time` | An `Instant` seen in a `Zone`, for showing or reasoning about local time | "09:30 in London" | `{ "instant": …, "zone": "Europe/London" }` |
| `Duration` | Elapsed time | a timeout, a retention period | `"PT5M"` |
| `Zone` | An IANA time zone, a built-in enum | `Zone.europe_london` | `"Europe/London"` |
| `Weekday` | A built-in enum | `Weekday.monday` | `"monday"` |

- Store `Instant`. Use `CalendarDate` only for facts that really are dates.
  There is no time without a date and zone, and no date-time without a zone.
- `Instant` keeps milliseconds: the precision every target (JavaScript,
  SQLite, PostgreSQL) shares. A literal with microseconds is rejected, and
  writes to PostgreSQL's microsecond column are always whole milliseconds
  (carried from area 5).
- `Time` is a misleading name: in Java, JavaScript's Temporal and Elixir a
  "time" is a time of day. Proposed: rename it `ZonedTime` (question 5).
- Types never convert to each other or to `Text` silently; signatures use
  named types built on them (decided in area 2), such as
  `type DueAt = Instant {}`.

## Durations

Proposed (question 4): one spelling everywhere in source, `500ms`, `30s`,
`5m`, `2h`, `30d`, with ISO 8601 (`"PT5M"`) only in JSON.

```jadpo fragment
type RequestTimeout = Duration {
    max: 30s
}

var grace: Duration = 5m
```

::: today

Configuration accepts `5m` and `30d` but code needs `Duration("PT5M")`; `5m`
in an expression and `max: 1h` in a constraint are syntax errors (verified).
The time plan excludes `d`, because a calendar day is not always 24 hours,
yet configuration accepts it.

```jadpo project=today-duration
type Wait = Duration {}

function standard_wait() -> Wait {
    return Wait("PT5M")
}
```

:::

`30d` would mean exactly 30 × 24 hours, which is what a retention period or a
token lifetime means. Moving a date by calendar days is a separate operation
on `CalendarDate` (below).

## The current time: `clock.now`

`clock.now` is the time an operation started. It is fixed for the whole
operation, nested actions included, so every timestamp one request writes
agrees. A later request, schedule run or retry gets a new value. There is no
way to read a fresher time, and code never reads the system clock.

Only actions read the clock. Functions and queries must give the same answer
for the same arguments, so they take the time as a parameter
(`type.clock_context`):

```jadpo fragment
type ReminderCutoff = Instant {}

action send_due_reminders() -> Unit {
    var cutoff: ReminderCutoff = ReminderCutoff(clock.now)
    var todos: List<Todo> = todos_due_before(cutoff)
    // ...
}

query todos_due_before(cutoff: ReminderCutoff) -> List<Todo> {
    // where: Todo.due_at < cutoff   (query syntax is area 6)
}
```

Timeouts are measured on a separate clock that wall-clock changes cannot
move; code never sees it. Tests fix and move the clock through fixtures
(area 13). The generated SQL always passes the time as a value; the database's
own `now()` is never used. This all works today.

## Arithmetic and comparison

`Instant + Duration` and `Instant - Duration` give an `Instant`;
`Instant - Instant` gives a `Duration`; `Duration ± Duration` gives a
`Duration`. Instants compare with `<` and `==` on the timeline, so
`12:00Z` equals `13:00+01:00`.

Proposed (question 6): adding a duration keeps the instant's named type, so
no rebuild is needed:

```jadpo fragment
function reminder_time(due_at: DueAt, notice: ReminderNotice) -> DueAt {
    return due_at - notice      // today: an Instant, rebuilt as DueAt(due_at - notice)
}
```

## Zones and local times

A zone turns an instant into local time; this is never ambiguous:

```jadpo fragment
var local: Time = temporal.in_zone(clock.now, Zone.europe_london)
```

Going the other way, from a date and a clock time to an instant, can be: at
01:30 on the day clocks go back, London has two 01:30s, and on the day they go
forward 01:30 does not exist. Today every call must say what to do in both
cases:

```jadpo fragment
var cutoff: Time = temporal.resolve(
    date: CalendarDate("2026-10-25"),
    at: "09:30",
    zone: Zone.europe_london,
    overlap: LocalOverlap.reject,
    gap: LocalGap.reject
)
```

A query for "orders on 25 October in London" uses the day's bounds, which are
correct whether the day has 23, 24 or 25 hours:

```jadpo fragment
var day: InstantRange = temporal.day_bounds(CalendarDate("2026-10-25"), Zone.europe_london)
// where: Order.created_at >= day.start and Order.created_at < day.end
```

The `temporal` library has about 35 functions and six option enums. A
smaller form, with one daylight-saving option instead of two, is in the
[time and formatting proposals](research/10-configuration-and-time-proposals.md)
(question 7).

::: today

The forms above compile today; this test is from `examples/temporal`:

```jadpo project=today-zones
test "London autumn day has twenty five hours" {
    var bounds = temporal.day_bounds(
        CalendarDate("2026-10-25"),
        Zone.europe_london
    )
    assert temporal.between(bounds.start, bounds.end) == Duration("PT25H")
}
```

:::

## Showing dates and times to people

Formatting needs three things the server must never guess: the zone, the
language and the format. Proposed (question 7): one call that takes all
three, with the format either built in or declared once as a named type:

```jadpo fragment
type ShortDate = TimeFormat {
    day: numeric
    month: short
    year: numeric
}

var placed: PresentationText = temporal.format(
    order.created_at,
    zone: customer.zone,
    locale: customer.locale,
    format: ShortDate
)
```

A built-in format is written `format: TimeFormat.date_time_long`. The
built-in formats are `date_`, `time_` and `date_time_` each with `full`,
`long`, `medium` or `short`; `date_time_long` gives "27 September 2026 at
14:30 BST" in `en_gb`. "3 minutes ago" style text uses
`temporal.format_relative(value, now: clock.now, zone:, locale:)` with fixed
rules: "just now", minutes, "Today at", "Yesterday at", the weekday, then the
long date.

The result is `PresentationText`: it can go into a response, an email or a
page, but it cannot be stored (`type.presentation_text_persistence`), turned
back into a time, compared as a time or rebuilt into `Text`. Clients that
format for themselves receive the `Instant`.

::: today

An `Instant` must first become a `Time`, the format is either `style:` or an
anonymous `components: { }` object (which area 2 forbids elsewhere), and
relative text is `temporal.format_friendly(value, relative_to:, locale:,
profile: FriendlyTimeFormat.conversational)`, naming its only profile. The
space in `temporal.format (` is the formatter's doing (a bug; it writes that
space after every `format`):

```jadpo project=today-format
locales {
    default: "en-GB"
    supported: ["en-GB", "en-US", "fr-FR"]
    unsupported: fallback_to_default
}

test "human formats require a resolved zone" {
    var value = temporal.in_zone(
        Instant("2026-09-27T13:30:00Z"),
        Zone.europe_london
    )
    var label = temporal.format (
        value,
        locale: Locale.en_gb,
        components: {
            day: numeric
            month: long
            year: numeric
        }
    )
    assert label == label
}
```

:::

## Locales

An application lists the languages it supports, once, in `app.jadpo`. The
list creates the `Locale` enum (`Locale.en_gb`), and formatting accepts only
those. The server's own locale is never used.

```jadpo fragment
locales {
    default: en_gb
    supported: [en_gb, en_us, fr_fr]
}
```

Proposed (question 8): write locales as option names, the same names code
uses, and drop `unsupported:`. A request field typed `Locale` is decoded like
any enum, so an unsupported tag is an `invalid_value` failure; choosing a
language from an `Accept-Language` header is a library call that falls back to
`default`.

::: today

Locales are written as text tags while code uses `Locale.en_gb`, and
`unsupported:` must choose `reject` or `fallback_to_default`:

```jadpo project=today-locales
locales {
    default: "en-GB"
    supported: ["en-GB", "en-US", "fr-FR"]
    unsupported: fallback_to_default
}
```

:::

## Translations

Failure messages are translated by the client using the failure's `code`
(decided in area 4); `message` stays fixed English. Text the server writes
for people, such as emails and server-rendered pages, needs translating on
the server, and area 1 reserved the root `translations/` folder for it.
Nothing is built today.

Proposed (question 9): one file per supported locale, `translations/en_gb.json`,
holding keys and messages in ICU MessageFormat, the format translation
services already import. The default locale's file defines the keys; the
compiler generates one typed call per key and checks every other file has
the same keys and placeholders:

```json
{
  "order_shipped": "Your order {order_number} has shipped.",
  "items_in_cart": "{count, plural, one {# item} other {# items}} in your cart"
}
```

```jadpo fragment
var subject: PresentationText = translations.order_shipped(
    locale: customer.locale,
    order_number: order.number
)
```

## Retention periods

Retention periods are ordinary settings (`soft_delete_retention: Duration`)
read by a lifecycle's `purge: { after: config.soft_delete_retention }`
(area 5). How retention fits the wider GDPR review (personal-data marking,
erasure, export) is carried from area 5 and not decided here.

## Errors

> The examples in this section use today's syntax, because they show the compiler's real output; they are rewritten as the rework lands.

### `configuration.binding_required`

```jadpo project=e-binding-required expect=configuration.binding_required
config App {
    mail_sender: Email {}
}
```

### `configuration.binding_empty`

```jadpo project=e-binding-empty expect=configuration.binding_empty
config App {
    mail_sender: Email {
        binding: ""
    }
}
```

### `configuration.binding_invalid`

```jadpo project=e-binding-invalid expect=configuration.binding_invalid
config App {
    mail_sender: Email {
        binding: "MAIL-SENDER"
    }
}
```

### `configuration.duplicate_binding`

```jadpo project=e-duplicate-binding expect=configuration.duplicate_binding
config App {
    mail_sender: Email {
        binding: "MAIL"
    }

    reply_to: Email {
        binding: "MAIL"
    }
}
```

### `configuration.duplicate_option`

```jadpo project=e-duplicate-option expect=configuration.duplicate_option
config App {
    mail_sender: Email {
        binding: "MAIL_SENDER"
        binding: "SENDER"
    }
}
```

### `configuration.unknown_option`

```jadpo project=e-unknown-option expect=configuration.unknown_option
config App {
    mail_sender: Email {
        binding: "MAIL_SENDER"
        required: true
    }
}
```

### `configuration.expected_field`

```jadpo project=e-expected-field expect=configuration.expected_field
config App {
    123
}
```

### `configuration.multiple_declarations`

```jadpo project=e-multiple expect=configuration.multiple_declarations
config Mail {
    mail_sender: Email {
        binding: "MAIL_SENDER"
    }
}

config Web {
    browser_origin: Url {
        binding: "BROWSER_ORIGIN"
    }
}
```

### `configuration.type_unsupported`

```jadpo project=e-type-unsupported expect=configuration.type_unsupported
config App {
    business_zone: Zone {
        binding: "BUSINESS_ZONE"
    }
}
```

### `configuration.default_literal_required`

```jadpo project=e-default-literal expect=configuration.default_literal_required
config App {
    page_size: Int {
        binding: "PAGE_SIZE"
        default:
    }
}
```

### `configuration.default_type`

```jadpo project=e-default-type expect=configuration.default_type
config App {
    page_size: Int {
        binding: "PAGE_SIZE"
        default: "many"
    }
}
```

### `configuration.default_invalid`

A PostgreSQL address is not a `Url` today:

```jadpo project=e-default-invalid expect=configuration.default_invalid
type DatabaseUrl = Url {}

config App {
    database_url: DatabaseUrl {
        binding: "DATABASE_URL"
        default: "postgres://localhost:5432/todo"
    }
}
```

### `configuration.secret_default`

```jadpo project=e-secret-default expect=configuration.secret_default
type ApiKey = Text {
    min_length: 8
}

config App {
    api_key: ApiKey {
        binding: "API_KEY"
        secret: true
        default: "development-key"
    }
}
```

### `configuration.secret_flow`

```jadpo project=e-secret-flow expect=configuration.secret_flow
type ApiKey = Text {
    min_length: 8
}

config App {
    api_key: ApiKey {
        binding: "API_KEY"
        secret: true
    }
}

function expose_secret() -> ApiKey {
    return config.api_key
}
```

### `type.clock_context`

```jadpo project=e-clock-context expect=type.clock_context
type ReminderCutoff = Instant {}

function cutoff() -> ReminderCutoff {
    return ReminderCutoff(clock.now)
}
```

### `type.temporal_clock_invalid`

```jadpo project=e-clock-invalid expect=type.temporal_clock_invalid
test "clock text is strict" {
    var opening = temporal.resolve(
        date: CalendarDate("2026-10-25"),
        at: "9:30",
        zone: Zone.europe_london,
        overlap: LocalOverlap.reject,
        gap: LocalGap.reject
    )
}
```

### `type.temporal_locales_required`

```jadpo project=e-locales-required expect=type.temporal_locales_required
test "formatting needs declared locales" {
    var value = temporal.in_zone(Instant("2026-09-27T13:30:00Z"), Zone.europe_london)
    var label = temporal.format(value, style: TimeFormat.date_long)
}
```

### `type.temporal_format_requires_time`

```jadpo project=e-requires-time expect=type.temporal_format_requires_time
locales {
    default: "en-GB"
    supported: ["en-GB"]
    unsupported: reject
}

test "an instant needs a zone" {
    var label = temporal.format(
        Instant("2026-10-25T08:30:00Z"),
        locale: Locale.en_gb,
        style: TimeFormat.date_long
    )
}
```

### `type.temporal_format_choice`

```jadpo project=e-format-choice expect=type.temporal_format_choice
locales {
    default: "en-GB"
    supported: ["en-GB"]
    unsupported: reject
}

test "one format choice" {
    var value = temporal.in_zone(Instant("2026-10-25T08:30:00Z"), Zone.europe_london)
    var label = temporal.format(
        value,
        locale: Locale.en_gb,
        style: TimeFormat.date_long,
        components: { day: numeric }
    )
}
```

### `type.temporal_format_components_empty`

```jadpo project=e-components-empty expect=type.temporal_format_components_empty
locales {
    default: "en-GB"
    supported: ["en-GB"]
    unsupported: reject
}

test "components cannot be empty" {
    var value = temporal.in_zone(Instant("2026-10-25T08:30:00Z"), Zone.europe_london)
    var label = temporal.format(value, locale: Locale.en_gb, components: {})
}
```

### `type.temporal_format_component_invalid`

```jadpo project=e-component-invalid expect=type.temporal_format_component_invalid
locales {
    default: "en-GB"
    supported: ["en-GB"]
    unsupported: reject
}

test "component values are checked" {
    var value = temporal.in_zone(Instant("2026-10-25T08:30:00Z"), Zone.europe_london)
    var label = temporal.format(value, locale: Locale.en_gb, components: { day: enormous })
}
```

### `type.temporal_format_component_duplicate`

```jadpo project=e-component-duplicate expect=type.temporal_format_component_duplicate
locales {
    default: "en-GB"
    supported: ["en-GB"]
    unsupported: reject
}

test "components are not repeated" {
    var value = temporal.in_zone(Instant("2026-10-25T08:30:00Z"), Zone.europe_london)
    var label = temporal.format (
        value,
        locale: Locale.en_gb,
        components: { day: numeric day: two_digit }
    )
}
```

### `type.presentation_text_persistence`

```jadpo project=e-presentation-persist expect=type.presentation_text_persistence
entity Receipt {
    id: Uuid
    identity: id
    issued_label: PresentationText

    persistence {
        store: primary
        role: authority
    }
}
```

### `syntax.locales_default_required`

```jadpo project=e-locales-default expect=syntax.locales_default_required
locales {
    supported: ["en-GB"]
    unsupported: reject
}
```

### `syntax.locales_supported_required`

```jadpo project=e-locales-supported expect=syntax.locales_supported_required
locales {
    default: "en-GB"
    unsupported: reject
}
```

### `syntax.locales_unsupported_required`

The three items are positional today, so a different third name is reported:

```jadpo project=e-locales-unsupported-required expect=syntax.locales_unsupported_required
locales {
    default: "en-GB"
    supported: ["en-GB"]
    otherwise: reject
}
```

### `syntax.locales_unsupported_invalid`

```jadpo project=e-locales-unsupported-invalid expect=syntax.locales_unsupported_invalid
locales {
    default: "en-GB"
    supported: ["en-GB"]
    unsupported: guess
}
```

### `type.locale_invalid`

```jadpo project=e-locale-invalid expect=type.locale_invalid
locales {
    default: "en-GB"
    supported: ["en-GB", "not a locale"]
    unsupported: reject
}
```

### `type.locale_duplicate`

```jadpo project=e-locale-duplicate expect=type.locale_duplicate
locales {
    default: "en-GB"
    supported: ["en-GB", "en-GB"]
    unsupported: reject
}
```

### `type.locale_default_unsupported`

```jadpo project=e-locale-default expect=type.locale_default_unsupported
locales {
    default: "fr-FR"
    supported: ["en-GB"]
    unsupported: reject
}
```

### `semantic.multiple_locale_declarations`

```jadpo project=e-locales-multiple expect=semantic.multiple_locale_declarations
locales {
    default: "en-GB"
    supported: ["en-GB"]
    unsupported: reject
}

locales {
    default: "en-US"
    supported: ["en-US"]
    unsupported: reject
}
```

## Review

### Status

Each row says how it was checked: compiled with `jadpo check`, run under Bun
1.2.20, or read from generated code.

| Feature | Status |
|---|---|
| Declaring settings | Works: bindings, defaults, secrets and duplicates are checked (compiled). **Bug:** a `Duration` default is never checked: `default: 99999999999999999999d` compiles, and the app then refuses every start with `config_required` for that field instead of saying the default is invalid (compiled and run). The `d` unit contradicts the time plan, which allows only `ms`, `s`, `m` and `h`. |
| Reading settings | Works as `config.<field>` in actions and functions. **Bug:** a misspelt field, `config.mail_snder`, reports "`__jadpo_config` has no `mail_snder` field", naming an internal variable (compiled). |
| Database URL | **Bug:** `Url` accepts only `http(s)://`, so a PostgreSQL address is rejected as a default (compiled) and at startup (run). The generated runtime opens PostgreSQL from `DATABASE_URL` directly rather than from the declared field (read from generated code). |
| Two URL checks | **Bug:** `jadpo config check` accepts `DATABASE_URL=http://db:5432/x`, but the app refuses to start with it, because the runtime also requires a dot in the host name (run both). A Docker Compose host such as `db` can never pass. |
| Secrets | Works: returning, copying into another type and comparing a secret are all `configuration.secret_flow` (compiled); startup reports field and variable names only (run). |
| Startup validation | Works: missing or invalid values stop the app before it opens its port, for both missing and malformed values (run). |
| Local values | `jadpo config set` and `config check` work as described (run `config check`; `set` needs a terminal and was not run). |
| Clock | Works: `clock.now` in a function or query is `type.clock_context`; actions read it freely (compiled). |
| Time types | Microsecond literals are rejected (compiled). `Instant + Duration` returns plain `Instant`, so a named type must be rebuilt (compiled). Duration bounds (`max: 1h`) cannot be written (compiled; also noted in area 2). |
| Formatting | Works as described, including the locale requirement and `PresentationText` checks; `PresentationText` may be returned in an `output` (compiled). Output text was not run. **Bug:** `jadpo fmt` rewrites every `temporal.format(` as `temporal.format (` (verified with `jadpo fmt`). |
| Translations | Not built. |

### Consistency with the rest of the language

- **Names that repeat a name.** The config declaration's name and every
  `binding:` restate a name already present (syntax review #2, #55); area 1
  dropped singleton names, and area 4 derived failure codes the same way.
- **Two duration spellings** (`5m` in configuration, `Duration("PT5M")` in
  code), and each place rejects the other (syntax review #56).
- **Locales written as text** in their declaration but as `Locale.en_gb` in
  code (syntax review #57), against area 2's rule that closed options are
  names.
- **An anonymous object as an argument.** `components: { day: numeric }`
  breaks area 2's decision that there are no anonymous objects (syntax
  review #58).
- **One-value settings.** `profile: FriendlyTimeFormat.conversational` has
  only one possible value, the kind of ritual line the syntax review removes
  elsewhere (§4).
- **`secret` means five things** (syntax review #46): the config flag, an
  authentication key, an integration credential, a test fixture wrapper and a
  design-only setting. Keep it for the config flag only (areas 8, 11, 13).
- **Config fields escape the named-type rule.** `oidc_audience: Text`
  compiles, although area 2 requires named types in signatures. A setting
  read into an action is a signature in all but name; recommend the same rule
  (`type OidcAudience = Text { }`).
- **Policy options in code are qualified** (`overlap: LocalOverlap.reject`),
  consistent with area 2's rule that bare names are only for settings
  (syntax review #59).

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Fixture configuration values, the fixture clock and `advance clock by` (syntax review proposes `clock.advance(5m)` and plain fixture values); `configuration.fixture_*`, `configuration.field_unknown` (raised from source only by a fixture) and `type.test_clock_context` | 13 | Input to area 13 |
| `jadpo config set`, `config check` and `dev` restarts; `configuration.value_invalid`, `configuration.value_missing`, `configuration.local_*` and `cli.config_*`, none of which `jadpo check` raises; the startup report (`runtime.startup_failed`) | 14 | Input to area 14 |
| Health and readiness, including the generated `health { }` proposal (syntax review #65) and `semantic.service_configuration_missing` | 11 | Input to area 11 |
| Integration credentials and authentication signing keys as the only secret sinks; their setting names (`token:`, `signing_key:`) | 8, 11 | Input to areas 8 and 11 |
| Mail templates using translations and `PresentationText` | 11 | Input to area 11 |
| `config.typo` should report the configuration field, not `__jadpo_config`; `jadpo fmt` writes `temporal.format (` with a space | 14 | Bug |
| GDPR: retention periods as settings, alongside personal-data marking, erasure and export | 5, 9, 10 | Carried from area 5; not decided here |

### Open questions

1. **Should the environment variable name come from the field name?** Every
   binding in the repository is the field name in capitals. Deriving it
   removes a line per field; the risk is that renaming a field silently
   renames the variable. *Recommendation:* derive it (`mail_sender` reads
   `MAIL_SENDER`), keep `binding:` only to differ, and show a changed variable
   name in the approval review as a deployment change, through the shared
   "accept a reviewed change" mechanism carried to area 7. Rails credentials
   and Spring Boot's relaxed binding derive names the same way.
2. **Which types can a setting have?** Today text-based, number, boolean,
   UUID, email, URL, IP, duration, instant and date types. *Recommendation:*
   add enums written by variant name, `Zone` and `Locale`, so a log level or
   business time zone is checked at startup instead of parsed by hand; keep
   lists and objects out (one variable, one value).
3. **How is a database address typed?** `Url` cannot hold `postgres://`, so
   area 5's `url: config.database_url` cannot work. *Recommendation:*
   built-in `PostgresUrl` and `RedisUrl`, each checking its schemes, always
   secret, with a store's `url:` required to match its `engine:`; the runtime
   connects only through it. Also make the startup and `config check`
   validators the same code, so they cannot disagree again.
4. **One duration spelling, and does `d` stay?** *Recommendation:* `500ms`,
   `30s`, `5m`, `2h` and `30d` everywhere in source, including constraints
   (`max: 30s`), with ISO 8601 only in JSON. Keep `d` as exactly 24 hours,
   because retention and token lifetimes are elapsed time and `720h` is
   unreadable; calendar days stay a date operation. Go has no `d` and
   Rust's `humantime` has it; no `w`, months or years, which vary.
5. **Rename `Time` to `ZonedTime`?** Java, Temporal and Elixir all use "time"
   for a time of day, so `Time` will be misread by people and agents.
   *Recommendation:* yes, `ZonedTime` (Temporal says `ZonedDateTime`, jiff
   `Zoned`), with the wire form unchanged.
6. **Does adding a duration keep the instant's named type?** Today
   `due_at - notice` is a plain `Instant` and must be rebuilt.
   *Recommendation:* yes for `+` and `-` with a `Duration`, the one mixed
   case area 3's operator rule did not cover; `Instant - Instant` gives
   `Duration`.
7. **Should the time library and formatting be simplified?** About 35
   functions, six option enums, two daylight-saving options on every call, a
   two-step format and an anonymous components object.
   *Recommendation:* Option B in the
   [time and formatting proposals](research/10-configuration-and-time-proposals.md):
   `temporal.format(value, zone:, locale:, format:)` with named built-in or
   declared formats, `format_relative` without a profile, one `dst:` option
   that defaults to failing, and about 20 functions.
8. **Locales as names, and drop `unsupported:`?** *Recommendation:* yes,
   `default: en_gb` and `supported: [en_gb, fr_fr]`, in `app.jadpo`. An
   unsupported tag in a request is an ordinary `invalid_value` like any
   enum; header negotiation is a library call that falls back to `default`.
9. **What format does `translations/` use?** *Recommendation:* one JSON file
   per supported locale with ICU MessageFormat messages (what FormatJS, Java
   and translation services such as Crowdin and Lokalise use), typed calls
   generated from the default locale's keys, and missing keys or
   placeholders in other locales as compile errors. Build it when the first
   email template needs it (area 11), not before.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Unnamed declarations | Carried from area 1: `config { }` and `locales { }` carry no name | 2026-10-12 |
| Root files | Carried from area 1: settings live in `config.jadpo`; there is no `env.jadpo`; values come from the platform environment and one `.env.local`, with secrets entered through `jadpo config set` | 2026-10-12 |
| Translations folder | Carried from area 1: `translations/` is a fixed root folder; its format is decided here | 2026-10-12 |
| Failure messages | Carried from area 4: clients translate by failure `code`; `message` stays fixed English | 2026-10-12 |
| Store addresses | Carried from area 5: stores are declared in `app.jadpo` and take `url: config.<field>` | 2026-10-12 |
| Signature types | Carried from area 2: signatures use named types built on `Instant`, `CalendarDate`, `Time` and `Duration`; `Zone`, `Locale` and `Weekday` may be used directly | 2026-10-12 |
| Option names | Carried from area 2: closed options in settings are lower_snake_case names; in code, enum values are qualified | 2026-10-12 |
| Environment variable names | | |
| Setting types | | |
| Database address type | | |
| Duration spelling | | |
| `Time` name | | |
| Instant arithmetic | | |
| Time library and formatting | | |
| Locales declaration | | |
| Translations format | | |
