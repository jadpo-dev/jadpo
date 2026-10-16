---
title: Configuration and time
area: 10
status: decided
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - configuration.binding_empty
  - configuration.binding_invalid
  - configuration.default_invalid
  - configuration.default_literal_required
  - configuration.default_type
  - configuration.duplicate_binding
  - configuration.duplicate_option
  - configuration.expected_field
  - configuration.field_unknown
  - configuration.multiple_declarations
  - configuration.secret_default
  - configuration.secret_flow
  - configuration.secret_invalid
  - configuration.test_value_missing
  - configuration.type_unsupported
  - configuration.unknown_option
  - configuration.url_type
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

> Decided on 2026-10-12. Code on this page applies the decisions from areas 1 to 10. The new compiler has built the time literals and the built-in `Zone`, `Weekday` and `Locale` enums (chunk 1.9) and checks where `clock.now` may be read. Chunk 5.1 built settings: `config.jadpo`, secrets, signing keys, the connections' addresses, `jadpo config`, the check before the app starts and a test's own values; the examples marked `check` in those sections and in the configuration errors run with this checkout's `jadpo`. Time and formatting come in chunk 5.2, locales and translations in chunk 5.3.

Two rules run through the page. **Code never reads the environment or the
system clock directly**: settings arrive as typed `config.<field>` values,
checked before the app starts, and the time arrives as `clock.now`, fixed for
the whole operation. **Machine values and text for people never mix**: an
`Instant` is stored and compared; a formatted date is only shown.

## Settings: `config.jadpo`

Every setting the application needs is a field in the one `config { }`
declaration in the root file `config.jadpo`. A field has a type, and
optionally `secret: true`, a `default:`, a `binding:` and a `test:` value.
Code reads it as `config.<field>` anywhere, functions included, because a
setting cannot change while the app runs.

```jadpo check project=settings
config {
    database_url: PostgresUrl
    session_signing_key: SigningKey
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
- A setting is typed, and so is what code reads: `config.page_size` is the
  `ListLimit` the field declares, checked when the app starts, so code never
  builds it again. Declare the named type the code uses
  (`page_size: ListLimit { default: 25 }`) and read `config.page_size` where a
  `ListLimit` goes.

```jadpo check project=settings-in-code
type ListLimit = Int {
    min: 1
    max: 100
}

type Sender = Email {}

config {
    page_size: ListLimit {
        default: 25
    }
    mail_sender: Sender
}

function first_page() -> ListLimit {
    return config.page_size
}

function from_address() -> Sender {
    return config.mail_sender
}
```

### Environment variable names

The variable name is the field name in capitals. Write `binding:` only when it
must differ, for example `binding: "HEROKU_POSTGRESQL_URL"` to keep a name a
hosting platform already uses. A binding is capital letters, digits and `_`,
starting with a capital letter, and no two settings read the same variable.

```jadpo check project=binding
config {
    database_url: PostgresUrl {
        binding: "HEROKU_POSTGRESQL_URL"
    }
}
```

The cost of deriving it: renaming a field renames the variable, which breaks
a deployment that still sets the old name. A changed name is therefore shown
in the approval review like any other contract change (question 1; the review
comes with area 18).

### What a setting can hold

A setting is read from one environment variable, so its type must be decodable
from one piece of text: text-based types, `Int`, `Decimal`, `Bool`, `Uuid`,
`Email`, `Url`, `IpAddress`, `Duration`, `Instant` and `CalendarDate`, a named
type built on one of them, an enum whose variants carry no data, and the
built-in enums `Zone` and `Locale` (question 2). Lists, objects, maps, `T?`
and data-carrying enums are rejected (`configuration.type_unsupported`):
split them into fields.

```jadpo check project=settings-enums
type LogLevel = Enum {
    debug
    info
    warning
}

config {
    log_level: LogLevel {
        default: LogLevel.info
    }
    business_zone: Zone {
        default: Zone.europe_london
    }
}
```

`LOG_LEVEL=debug` and `BUSINESS_ZONE=Europe/London` are valid values: an enum
is read as the variant's name, a `Zone` as its IANA name and a `Locale` as its
BCP 47 tag (`en-GB`). A `Duration` is written as it is in source (`30s`, `5m`,
`2h`, `30d`), a `Bool` as `true` or `false`, and an `Instant` and a
`CalendarDate` in ISO 8601. A variable set to nothing counts as not set.

### Defaults and test values

A default is a literal of the field's type, checked while compiling
(`default: 25`, `default: 5s`, `default: LogLevel.info`), against every rule of
the type and the types it is built on. The same is true of `test:`, the value
every test reads (below); a field that has none of its own default and is read
by a test needs one.

A secret cannot have either, so a forgotten production secret stops the app
instead of falling back to a value written in source, and secrets never appear
in test source.

### Chosen in chunk 5.1

- **Named types.** A setting may be declared with a built-in type such as
  `Duration` or `Int`, but a signature takes named types (area 2), so a
  setting that code passes on is best declared as the named type it is
  passed as: the app then checks it when it starts, and `config.page_size`
  is a `ListLimit` with nothing to build. Building one from a plainly typed
  setting is an ordinary construction, which can fail and so needs
  `attempt`.
- **Which values count.** A variable set to nothing is not set. The process's
  environment wins over `.env.local` for a variable both set.
- **`binding:` and the previous key.** A `SigningKey` setting also reads the
  variable named like its own with `_PREVIOUS` after it (below), so no other
  setting may bind that name.

## Secrets

`secret: true` marks a value that must never leave the server. The compiler
accepts it only at places the compiler itself owns: a connection's `url:`, an
integration's `credential:` (and its webhook `secret:`, area 11) and an
authentication `signing_key:` (area 8), all written in `app.jadpo` or
`authentication.jadpo`. Nothing in a function, action, query or test is such
a place, so reading a secret there at all (returning it, storing it, logging
it, putting it in a failure, building another value from it, passing it on
or even comparing it to something) is `configuration.secret_flow`. A value
that is always secret needs no flag: `PostgresUrl`, `RedisUrl` and
`SigningKey`.

```jadpo check project=secret-sink
config {
    database_url: PostgresUrl
}

application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

Comparisons are included: `key == config.api_key` is rejected, because a
yes/no answer leaks the secret one guess at a time. Because the secret is
never read by code, nothing printed, stored, returned or logged by the
application can hold it; `jadpo config check`, `jadpo dev` and the start-up
check say which setting and which variable, never the value.

## Database and other connection URLs

Area 5 decided that stores take their address from configuration, and area 11
put them in one `connections:` block:

```jadpo check project=connection-urls file=app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        cache: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
}
```

```jadpo check project=connection-urls file=config.jadpo
config {
    database_url: PostgresUrl
    redis_url: RedisUrl
}
```

`Url` accepts only `http://` and `https://`, so it cannot hold a
`postgres://user:password@host/db` address. The built-in connection types
(question 3), one per driver, can: `PostgresUrl` (`postgres://` or
`postgresql://`) and `RedisUrl` (`redis://` or `rediss://`). They are always
secret, because they normally carry a password, and a connection's `url:` must
read a setting of the type its `driver:` needs (`configuration.url_type`); an
address written in source is refused the same way. The runtime opens the
connection only through that setting.

A project that declares no database has one called `main` on PostgreSQL, and
it opens at `config.database_url` when the project declares one (and it is a
`PostgresUrl`).

## Values on your machine and in production

Production values come from the platform's environment (or its secret
manager, which sets the same variables). Locally there is one git-ignored
file, `.env.local`; Jadpo defines no `.env`, `.env.development` or layering.
The process's environment wins over the file for a variable both set, and
nothing else is read. Tests use each field's `test:` value, then its default,
and `jadpo test` makes a random throwaway value for every secret; a single
test can override a value with its `config:` setting (area 13).

A secret is never typed into a chat with an agent. The agent asks you to run:

```text
$ jadpo config set mail_api_key
? Enter MAIL_API_KEY hidden
mail_api_key configured locally
    variable  MAIL_API_KEY
    file      .env.local
```

The command asks at a prompt (never a command-line argument), hiding what is
typed when the setting is secret, checks the value against the field's type
before anything is written, and writes `.env.local` safely: only an ordinary
file, through a temporary file only its owner can read, only if the file has
not changed since it was read, and with `.env.local` added to `.gitignore`.
A value that breaks the type is refused by the rule it breaks, never by
printing it. `jadpo config check` lists each field as `set`, `default`,
`missing` or `invalid`, with the variable it reads, and never the value:

```text
$ jadpo config check
Settings
  database_url         set      DATABASE_URL
  session_signing_key  missing  SESSION_SIGNING_KEY
  log_level            default  LOG_LEVEL

error: `SESSION_SIGNING_KEY` isn't set
  reason: `session_signing_key` is required: it has no `default:`, so the app can't start without a value for it.
  next: Run `jadpo config set session_signing_key` to store it in `.env.local`, or set `SESSION_SIGNING_KEY` in the environment (human)
  rule: configuration.value_missing
  help: docs/language/14-tooling.html#command-errors

config incomplete: 1 problem, 3 settings

next: jadpo config set session_signing_key  stores it in .env.local
```

Both are tooling (area 14). `config set` needs a terminal and refuses
`--json`; `config check --json` lists the same settings as data.

## Signing keys

A `SigningKey` is a built-in secret type for the key sessions and tokens are
signed with: at least 32 characters with no spaces. A setting of that type
reads its variable and, when that is set, `NAME_PREVIOUS` as well:
`session_signing_key` reads `SESSION_SIGNING_KEY` and `SESSION_SIGNING_KEY_PREVIOUS`
(question 10). New tokens are signed with the current key and both are
accepted, so a deployment rotates a key by setting the new one and moving the
old one to `_PREVIOUS`, then removing it once the longest token lifetime has
passed. Both are checked when the app starts, and neither is ever printed.
Authentication reads the key as `signing_key: config.session_signing_key`;
signing sessions and tokens with it comes with chunk 5.1b.

## Checking values before the app starts

The generated app reads only the declared variables, checks every value
against its type, and refuses to start if any is missing or invalid. It never
opens its port with bad configuration, and it reports the field and variable
name, never the value. `jadpo dev` runs the same check against the environment
over `.env.local`, watches `.env.local` as it does the source, and keeps the
last good version running when a change is invalid. A database that is
configured correctly but down is not a configuration error: the app starts,
reports not ready and recovers (health checks are area 11).

The check is one function in the runtime, `jadpo_runtime::config::load`, given
the plan's settings and a source of variables (a host capability:
`Host::variable` natively, the process's environment; a binding on
Workers), so the interpreter and compiled code on every platform start the
same way, and `jadpo config check` and the app cannot disagree about what is
valid. `jadpo dev` is the one place that makes up a value: a database
connection whose address no variable gives gets a database of its own on a
disposable PostgreSQL for the session, so a fresh clone runs.

```text
$ jadpo dev
jadpo dev, started 12:52:17
error: `GREETING` isn't set
  reason: `greeting` is required: it has no `default:`, so the app can't start without a value for it.
  next: Run `jadpo config set greeting` to store it in `.env.local`, or set `GREETING` in the environment (human)
  rule: configuration.value_missing
  help: docs/language/14-tooling.html#command-errors

not running this version: 1 error, waiting for a save that fixes them
```

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
- `Instant` keeps milliseconds: the precision JavaScript clients, SQLite and
  PostgreSQL all share. A literal with microseconds is rejected, and
  writes to PostgreSQL's microsecond column are always whole milliseconds
  (carried from area 5).
- `Time` is a misleading name: in Java, JavaScript's Temporal and Elixir a
  "time" is a time of day. Proposed: rename it `ZonedTime` (question 5).
- Types never convert to each other or to `Text` silently; signatures use
  named types built on them (decided in area 2), such as
  `type DueAt = Instant {}`.
- Built in chunk 1.9 (decided in the Phase 1 review): an `Instant` literal is
  ISO 8601 with a `Z` or an offset, to the millisecond
  (`"2026-10-25T08:30:00Z"`), a `CalendarDate` literal a real date
  (`"2026-10-25"`), both checked while compiling; a `Duration` is written only
  with a unit (`15m`), and `min:` and `max:` on a `Duration` type are
  enforced; a `Time` has no literal. `Zone`, `Weekday` and `Locale` are
  built-in enums: `Zone` has a variant for each zone in the IANA database's
  `zone.tab` (2026d) plus `utc`, named in lower_snake_case
  (`Zone.america_argentina_buenos_aires`); `Locale` has one for each
  locale Unicode CLDR 48.2.3 gives modern coverage, plus the language with
  its likely region for each without one (`Locale.fr` and `Locale.fr_fr`,
  `Locale.en_gb`; 533 in all), named in lower_snake_case from the BCP 47
  tag. Both tables live in `jadpo-std` (`src/zones.rs`, which says how each
  was made) and change only in a change of their own.
- `clock.now` in a function, a library function or an inline function is
  `type.clock_context`; any other member of `clock`, `translations`,
  `sessions` or `oauth` is `toolchain.not_built_yet` until its area is built.
  `config.<name>` is a setting (chunk 5.1).

## Durations

Proposed (question 4): one spelling everywhere in source, `500ms`, `30s`,
`5m`, `2h`, `30d`, with ISO 8601 (`"PT5M"`) only in JSON.

```jadpo fragment
type RequestTimeout = Duration {
    max: 30s
}

var grace: Duration = 5m
```

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
own `now()` is never used.

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
forward 01:30 does not exist. The call says what to do in both cases:

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

## Showing dates and times to people

Formatting needs three things the server must never guess: the zone, the
language and the format. Proposed (question 7): one call that takes all
three, with the format either built in or declared once as a named type:

```jadpo fragment
type ShortDate = TimeFormat {
    day: ComponentStyle.numeric
    month: ComponentStyle.short
    year: ComponentStyle.numeric
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

## Locales

`Locale` is a built-in enum of the standard locales (`Locale.en_gb`,
`Locale.fr_fr`). An application lists the ones it supports, once, in
`app.jadpo`; formatting and translations accept only those, and using any
other is a compile error. The server's own locale is never used.

```jadpo fragment
locales: {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_us, Locale.fr_fr]
}
```

There is no `unsupported:` setting. A request field typed `Locale` is decoded like
any enum, so an unsupported tag is an `invalid_value` failure; choosing a
language from an `Accept-Language` header is a library call that falls back to
`default`.

## Translations

Failure messages are translated by the client using the failure's `code`
(decided in area 4); `message` stays fixed English. Text the server writes
for people, such as emails and server-rendered pages, needs translating on
the server, and area 1 reserved the root `translations/` folder for it.
It comes in chunk 5.3.

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
(area 5), which takes a `Duration` setting that isn't secret
(`data.lifecycle_purge_after` otherwise). The period is the value the app
started with, so a purge run is given it (`jadpo_runtime::purge_after`), and
one that is not above zero deletes nothing. How retention fits the wider GDPR support (personal-data marking,
erasure, export) is designed in area 15 (Personal data).

## Errors

> The configuration errors below run with this checkout's `jadpo` (chunk 5.1). The time and locale errors after them predate the decisions above and are rewritten when chunks 5.2 and 5.3 build them.

### `configuration.binding_empty`

```jadpo check project=e-binding-empty expect=configuration.binding_empty
config {
    mail_sender: Email {
        binding: ""
    }
}
```

```text output
error: `binding:` names no variable
 --> main.jadpo:3:18
  |
3 |         binding: ""
  |                  ^^ empty
  reason: `binding:` names the environment variable a setting is read from; an empty name reads nothing.
  next: Name the variable, or leave `binding:` out to read the setting's name in capitals (agent)
  rule: configuration.binding_empty
  help: docs/language/10-configuration-and-time.html#configuration-binding-empty

check failed: 1 error, 1 file
```

### `configuration.binding_invalid`

```jadpo check project=e-binding-invalid expect=configuration.binding_invalid
config {
    mail_sender: Email {
        binding: "MAIL-SENDER"
    }
}
```

```text output
error: `MAIL-SENDER` can't be a variable name
 --> main.jadpo:3:18
  |
3 |         binding: "MAIL-SENDER"
  |                  ^^^^^^^^^^^^^ not a variable name
  reason: A variable name is capital letters, digits and `_`, starting with a capital letter, so every platform accepts it.
  next: Use `MAIL_SENDER` (agent)
  rule: configuration.binding_invalid
  help: docs/language/10-configuration-and-time.html#configuration-binding-invalid

check failed: 1 error, 1 file
```

### `configuration.duplicate_binding`

```jadpo check project=e-duplicate-binding expect=configuration.duplicate_binding
config {
    mail_sender: Email {
        binding: "MAIL"
    }

    reply_to: Email {
        binding: "MAIL"
    }
}
```

```text output
error: `reply_to` and `mail_sender` both read `MAIL`
 --> main.jadpo:7:18
  |
3 |         binding: "MAIL"
  |                  ------ first read here
7 |         binding: "MAIL"
  |                  ^^^^^^ the same variable
  reason: Each setting has its own environment variable; two settings sharing one would always hold the same value, and renaming either would change both.
  next: Give `reply_to` a variable of its own with `binding:`, or merge the two settings (agent)
  rule: configuration.duplicate_binding
  help: docs/language/10-configuration-and-time.html#configuration-duplicate-binding

check failed: 1 error, 1 file
```

### `configuration.duplicate_option`

```jadpo check project=e-duplicate-option expect=configuration.duplicate_option
config {
    mail_sender: Email {
        binding: "MAIL_SENDER"
        binding: "SENDER"
    }
}
```

```text output
error: `binding:` is given twice on `mail_sender`
 --> main.jadpo:4:9
  |
3 |         binding: "MAIL_SENDER"
  |         ------- first given here
4 |         binding: "SENDER"
  |         ^^^^^^^ given again
  reason: A setting says each of `secret:`, `default:`, `binding:` and `test:` once, so there is one answer to read.
  next: Remove one `binding:` (agent)
  rule: configuration.duplicate_option
  help: docs/language/10-configuration-and-time.html#configuration-duplicate-option

check failed: 1 error, 1 file
```

### `configuration.unknown_option`

```jadpo check project=e-unknown-option expect=configuration.unknown_option
config {
    mail_sender: Email {
        binding: "MAIL_SENDER"
        required: true
    }
}
```

```text output
error: `required` isn't an option a setting takes
 --> main.jadpo:4:9
  |
4 |         required: true
  |         ^^^^^^^^ unknown option
  reason: `mail_sender` can say `secret:`, `default:`, `binding:` and `test:`. A setting is required unless it has a `default:`; there are no optional settings, so give a default or leave the setting required.
  next: Remove `required` (agent)
  rule: configuration.unknown_option
  help: docs/language/10-configuration-and-time.html#configuration-unknown-option

check failed: 1 error, 1 file
```

### `configuration.expected_field`

```jadpo check project=e-expected-field expect=configuration.expected_field
config {
    mail_sender
}
```

```text output
error: `mail_sender` isn't a setting
 --> main.jadpo:2:5
  |
2 |     mail_sender
  |     ^^^^^^^^^^^ not `name: Type`
  reason: `config { }` holds settings, each written `name: Type`, with a block after the type when it has `secret:`, `default:`, `binding:` or `test:`.
  next: Write the setting as `name: Type`, or remove this line (agent)
  rule: configuration.expected_field
  help: docs/language/10-configuration-and-time.html#configuration-expected-field

check failed: 1 error, 1 file
```

### `configuration.multiple_declarations`

```jadpo check project=e-multiple expect=configuration.multiple_declarations
config {
    mail_sender: Email {
        binding: "MAIL_SENDER"
    }
}

config {
    browser_origin: Url {
        binding: "BROWSER_ORIGIN"
    }
}
```

```text output
error: `config { }` is declared twice
 --> main.jadpo:7:1
  |
1 | config {
  | ------ first declared here
7 | config {
  | ^^^^^^ declared again
  reason: Every setting the application needs is a field of the one `config { }`, in `config.jadpo`, so one place lists everything a deployment must provide.
  next: Move the settings this `config` holds into the first, and remove it (agent)
  rule: configuration.multiple_declarations
  help: docs/language/10-configuration-and-time.html#configuration-multiple-declarations

check failed: 1 error, 1 file
```

### `configuration.type_unsupported`

```jadpo check project=e-type-unsupported expect=configuration.type_unsupported
config {
    admin_emails: List<Email>
}
```

```text output
error: `admin_emails` can't be a `List<Email>`
 --> main.jadpo:2:19
  |
2 |     admin_emails: List<Email>
  |                   ^^^^^^^^^^^ not read from one variable
  reason: A setting is read from one environment variable, so its type must be decodable from one piece of text: text-based types, `Int`, `Decimal`, `Bool`, `Uuid`, `Email`, `Url`, `IpAddress`, `Duration`, `Instant`, `CalendarDate` and enums. A list is one value in many variables or one variable holding a format; split it into settings.
  next: Split `admin_emails` into settings of those types (agent)
  rule: configuration.type_unsupported
  help: docs/language/10-configuration-and-time.html#configuration-type-unsupported

check failed: 1 error, 1 file
```

### `configuration.default_literal_required`

```jadpo check project=e-default-literal expect=configuration.default_literal_required
config {
    page_size: Int {
        default: largest_page
    }
}
```

```text output
error: `default:` on `page_size` isn't a literal
 --> main.jadpo:3:18
  |
3 |         default: largest_page
  |                  ^^^^^^^^^^^^ not a literal
  reason: A `default:` is a literal of the setting's type, checked while compiling (`default: 25`, `default: 5s`); it can't be computed, because nothing runs before the application starts.
  next: Write a literal value after `default:`, or remove it (agent)
  rule: configuration.default_literal_required
  help: docs/language/10-configuration-and-time.html#configuration-default-literal-required

check failed: 1 error, 1 file
```

### `configuration.default_type`

```jadpo check project=e-default-type expect=configuration.default_type
config {
    page_size: Int {
        default: "many"
    }
}
```

```text output
error: `"many"` can't be the default of `page_size`
 --> main.jadpo:3:18
  |
3 |         default: "many"
  |                  ^^^^^^ the wrong kind of value
  reason: `page_size` is `Int`, and this is a different kind of value; the default must be a literal of the setting's own type.
  next: Write a `Int` value after `default:` (agent)
  rule: configuration.default_type
  help: docs/language/10-configuration-and-time.html#configuration-default-type

check failed: 1 error, 1 file
```

### `configuration.default_invalid`

A PostgreSQL address is not a `Url`:

```jadpo check project=e-default-invalid expect=configuration.default_invalid
type PrimaryDatabase = Url {}

config {
    primary_database: PrimaryDatabase {
        default: "postgres://localhost:5432/todo"
    }
}
```

```text output
error: `"postgres://localhost:5432/todo"` isn't a valid `PrimaryDatabase`
 --> main.jadpo:5:18
  |
5 |         default: "postgres://localhost:5432/todo"
  |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not a valid value
  reason: The value isn't an `http://` or `https://` URL with a host, such as `https://example.com`; a default is checked while compiling, with the rules of the setting's type.
  next: Write a valid `PrimaryDatabase` value (agent)
  rule: configuration.default_invalid
  help: docs/language/10-configuration-and-time.html#configuration-default-invalid

check failed: 1 error, 1 file
```

### `configuration.secret_default`

```jadpo check project=e-secret-default expect=configuration.secret_default
type ApiKey = Text {
    min_length: 8
}

config {
    api_key: ApiKey {
        secret: true
        default: "development-key"
    }
}
```

```text output
error: `api_key` is secret, and has a `default:` in source
 --> main.jadpo:8:9
  |
8 |         default: "development-key"
  |         ^^^^^^^ a secret written in source
  reason: A secret's value comes only from the deployment's environment, so a forgotten production secret stops the app instead of falling back to a value written in source; `jadpo test` generates its own for a test run, so it needs no `default:` either.
  next: Remove `default:` from `api_key` (agent)
  rule: configuration.secret_default
  help: docs/language/10-configuration-and-time.html#configuration-secret-default

check failed: 1 error, 1 file
```

### `configuration.secret_invalid`

```jadpo check project=e-secret-invalid expect=configuration.secret_invalid
config {
    api_key: Text {
        secret: yes
    }
}
```

```text output
error: `secret:` on `api_key` isn't `true` or `false`
 --> main.jadpo:3:17
  |
3 |         secret: yes
  |                 ^^^ not a flag
  reason: `secret: true` marks a value that must never leave the server; the setting is either secret or it is not.
  next: Write `secret: true`, or leave `secret:` out (agent)
  rule: configuration.secret_invalid
  help: docs/language/10-configuration-and-time.html#configuration-secret-invalid

check failed: 1 error, 1 file
```

### `configuration.secret_flow`

```jadpo check project=e-secret-flow expect=configuration.secret_flow
type ApiKey = Text {
    min_length: 8
}

config {
    api_key: ApiKey {
        secret: true
    }
}

function expose_secret() -> ApiKey {
    return config.api_key
}
```

```text output
error: `config.api_key` is a secret
 --> main.jadpo:12:12
   |
12 |     return config.api_key
   |            ^^^^^^^^^^^^^^ a secret, read here
  reason: A secret never leaves the server, so it is accepted only where the compiler itself owns the place: a connection's `url:`, an integration's `credential:` and an authentication `signing_key:`. Returning it, storing it, logging it, putting it in a failure, building another value from it or comparing it with something could leak it.
  next: Give `config.api_key` to the connection, integration or authentication that needs it in `app.jadpo` or `authentication.jadpo`, or leave `secret: true` off `api_key` if its value is public (agent)
  rule: configuration.secret_flow
  help: docs/language/10-configuration-and-time.html#configuration-secret-flow

check failed: 1 error, 1 file
```

### `configuration.field_unknown`

```jadpo check project=e-field-unknown expect=configuration.field_unknown
type Sender = Email {}

config {
    mail_sender: Sender
}

function from_address() -> Sender {
    return config.mail_sendr
}
```

```text output
error: `config.mail_sendr` isn't a setting
 --> main.jadpo:8:12
  |
8 |     return config.mail_sendr
  |            ^^^^^^^^^^^^^^^^^ no such setting
  reason: `config { }` declares no setting called `mail_sendr`. Did you mean `mail_sender`?
  next: Use `config.mail_sender` instead of `config.mail_sendr` (agent)
  rule: configuration.field_unknown
  help: docs/language/10-configuration-and-time.html#configuration-field-unknown

check failed: 1 error, 1 file
```

### `configuration.url_type`

```jadpo check project=e-url-type expect=configuration.url_type file=config.jadpo
config {
    cache_url: RedisUrl
}
```

```jadpo check project=e-url-type expect=configuration.url_type file=app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.cache_url
        }
    }
}
```

```text output
error: `main`'s address must be a `PostgresUrl` setting
 --> app.jadpo:5:18
  |
5 |             url: config.cache_url
  |                  ^^^^^^^^^^^^^^^^ `cache_url` is `RedisUrl`
  reason: A connection opened with `driver: Driver.postgres` takes an address of its own type, which checks the scheme and is always secret; `config.cache_url` is `RedisUrl`.
  next: Declare `<name>: PostgresUrl` in `config.jadpo`, and write `url: config.<name>` (agent)
  rule: configuration.url_type
  help: docs/language/10-configuration-and-time.html#configuration-url-type

check failed: 1 error, 2 files
```

### `configuration.test_value_missing`

A test reads each setting's `test:` value, or its default; this one has neither:

```jadpo check project=e-test-value-missing expect=configuration.test_value_missing
type Greeting = Text {
    max_length: 80
}

config {
    greeting: Greeting
}

function hello() -> Greeting {
    return config.greeting
}

test "the greeting is read" {
    assert hello() == "hello"
}
```

```text output
error: `config.greeting` has no value in tests
 --> main.jadpo:10:12
   |
10 |     return config.greeting
   |            ^^^^^^^^^^^^^^^ no `test:` or `default:`
  reason: The test "the greeting is read" reaches this read. A test reads each setting's `test:` value, or its `default:`, and `greeting` has neither; only a secret is made up for a run, and `greeting` isn't one.
  next: Give `greeting` a `test:` value, or a `default:`, in `config.jadpo` (agent)
  rule: configuration.test_value_missing
  help: docs/language/10-configuration-and-time.html#configuration-test-value-missing

check failed: 1 error, 1 file
```

### `type.clock_context`

```jadpo check project=e-clock-context expect=type.clock_context
type ReminderCutoff = Instant {}

function cutoff() -> ReminderCutoff {
    return ReminderCutoff(clock.now)
}
```

```text output
error: A function can't read `clock.now`
 --> main.jadpo:4:27
  |
4 |     return ReminderCutoff(clock.now)
  |                           ^^^^^^^^^ reads the clock
  reason: A function is pure: it gives the same result for the same arguments, so the time it needs arrives as a parameter; only actions read the clock.
  next: Pass the time in as a parameter, read in the action that calls this (agent)
  rule: type.clock_context
  help: docs/language/10-configuration-and-time.html#type-clock-context

check failed: 1 error, 1 file
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

A third item with a different name:

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

The new compiler has built the time literals and the built-in `Zone`,
`Weekday` and `Locale` enums (chunk 1.9), and reports `clock.now` outside an
action as `type.clock_context`. Settings, secrets, signing keys, connection
addresses, `jadpo config`, the check before the app starts and a test's own
values were built in chunk 5.1 (the readings it chose are in the Decisions
table). Time arithmetic and formatting come in chunk 5.2, and locales and
translations in chunk 5.3. Trusted proxies, the addresses a client's address is
read through, are not decided on any page; they are chunk 5.1c.

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
  consistent with area 2's rule that every option is written `Type.value`,
  in settings and in code (syntax review #59).

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Fixture configuration values, the fixture clock and `advance clock by` (syntax review proposes `clock.advance(5m)` and plain fixture values); `configuration.fixture_*` and `type.test_clock_context` (a test's own `config:` is built, chunk 5.1; `configuration.field_unknown` is area 10's now) | 13 | Input to area 13 |
| `jadpo config set`, `config check`, `dev` restarts, `configuration.value_invalid`, `configuration.value_missing`, `configuration.local_*` and `cli.config_*`, none of which `jadpo check` raises | 14 | Built (chunk 5.1) |
| The startup report of a deployable (`runtime.startup_failed`), the generated app refusing to start | 14 | Input to area 14; `jadpo dev` refuses today, and the runtime's check is the one a deployable calls |
| Health and readiness, including the generated `health { }` proposal (syntax review #65) and `semantic.service_configuration_missing` | 11 | Input to area 11 |
| Integration credentials and authentication signing keys as the only secret sinks; their setting names (`token:`, `signing_key:`) | 8, 11 | Input to areas 8 and 11 |
| Mail templates using translations and `PresentationText` | 11 | Input to area 11 |
| `config.typo` should report the configuration field | 10 | Fixed (chunk 5.1): `configuration.field_unknown` |
| `jadpo fmt` writes `temporal.format (` with a space | 14 | Bug |
| Trusted proxies: which proxies a deployment trusts to say a client's address (`X-Forwarded-For`, `Forwarded`, Cloudflare's `CF-Connecting-IP`) | 7, 10, 16 | Not decided on any page: chunk 5.1c |
| GDPR: retention periods as settings, alongside personal-data marking, erasure and export | 15 | Moved to area 15 |

### Open questions

1. **Should the environment variable name come from the field name?** Every
   binding in the repository is the field name in capitals. Deriving it
   removes a line per field; the risk is that renaming a field silently
   renames the variable. *Recommendation:* derive it (`mail_sender` reads
   `MAIL_SENDER`), keep `binding:` only to differ, and show a changed variable
   name in the approval review as a deployment change, through the shared
   "accept a reviewed change" mechanism carried to area 7. Rails credentials
   and Spring Boot's relaxed binding derive names the same way. *Decided 2026-10-12:* yes, as recommended.
2. **Which types can a setting have?** Today text-based, number, boolean,
   UUID, email, URL, IP, duration, instant and date types. *Recommendation:*
   add enums, written `Type.value`, and `Zone` and `Locale`, so a log level or
   business time zone is checked at startup instead of parsed by hand; keep
   lists and objects out (one variable, one value). *Decided 2026-10-12:* yes, as recommended.
3. **How is a database address typed?** `Url` cannot hold `postgres://`, so
   area 5's `url: config.database_url` cannot work. *Recommendation:*
   built-in `PostgresUrl` and `RedisUrl`, each checking its schemes, always
   secret, with a store's `url:` required to match its `engine:`; the runtime
   connects only through it. Also make the startup and `config check`
   validators the same code, so they cannot disagree again. *Decided 2026-10-12:* yes, as recommended. *Built in chunk 5.1* (the Decisions table says how).
4. **One duration spelling, and does `d` stay?** *Recommendation:* `500ms`,
   `30s`, `5m`, `2h` and `30d` everywhere in source, including constraints
   (`max: 30s`), with ISO 8601 only in JSON. Keep `d` as exactly 24 hours,
   because retention and token lifetimes are elapsed time and `720h` is
   unreadable; calendar days stay a date operation. Go has no `d` and
   Rust's `humantime` has it; no `w`, months or years, which vary. *Decided 2026-10-12:* yes, as recommended.
5. **Rename `Time` to `ZonedTime`?** Java, Temporal and Elixir all use "time"
   for a time of day, so `Time` will be misread by people and agents.
   *Recommendation:* yes, `ZonedTime` (Temporal says `ZonedDateTime`, jiff
   `Zoned`), with the wire form unchanged. *Decided 2026-10-12:* yes, as recommended.
6. **Does adding a duration keep the instant's named type?** Today
   `due_at - notice` is a plain `Instant` and must be rebuilt.
   *Recommendation:* yes for `+` and `-` with a `Duration`, the one mixed
   case area 3's operator rule did not cover; `Instant - Instant` gives
   `Duration`. *Decided 2026-10-12:* yes, as recommended.
7. **Should the time library and formatting be simplified?** About 35
   functions, six option enums, two daylight-saving options on every call, a
   two-step format and an anonymous components object.
   *Recommendation:* Option B in the
   [time and formatting proposals](research/10-configuration-and-time-proposals.md):
   `temporal.format(value, zone:, locale:, format:)` with named built-in or
   declared formats, `format_relative` without a profile, one `dst:` option
   that defaults to failing, and about 20 functions. *Decided 2026-10-12:* yes, as recommended.
8. **Locales as names, and drop `unsupported:`?** *Recommendation:* yes,
   `default: Locale.en_gb` and `supported: [Locale.en_gb, Locale.fr_fr]`, in
   `app.jadpo`, with `Locale` a built-in enum of the standard locales. An
   unsupported tag in a request is an ordinary `invalid_value` like any
   enum; header negotiation is a library call that falls back to `default`. *Decided 2026-10-12:* yes, as recommended.
9. **What format does `translations/` use?** *Recommendation:* one JSON file
   per supported locale with ICU MessageFormat messages (what FormatJS, Java
   and translation services such as Crowdin and Lokalise use), typed calls
   generated from the default locale's keys, and missing keys or
   placeholders in other locales as compile errors. Build it when the first
   email template needs it (area 11), not before. *Decided 2026-10-12:* yes, as recommended.
   *Amended 2026-10-13 (Richard):* translations are supported out of the box
   from the first build, not deferred: mail, the generated privacy pages,
   client failure messages and any generated page use them.
10. **How are signing keys rotated?** (carried from area 8) Area 8 keeps
    signing keys, the OIDC issuer and audience in configuration. How does a
    deployment replace a signing key without signing everyone out?
    *Decided 2026-10-12:* a built-in `SigningKey` type. `session_signing_key`
    reads `SESSION_SIGNING_KEY` and an optional `SESSION_SIGNING_KEY_PREVIOUS`;
    new tokens are signed with the current key and both are accepted. Rotate
    by setting the new key and moving the old one to `_PREVIOUS`, then remove
    it once the longest token lifetime has passed. *Built in chunk 5.1* as
    far as reading and checking the two values; signing with them is chunk
    5.1b.
11. **Do configuration fields carry `test:` values?** (carried from area 13)
    Area 13 has tests take each configuration field's `test:` value, while
    "Values on your machine and in production" above says tests get their
    values from fixtures. Which does configuration declare?
    *Decided 2026-10-12:* a field may carry a `test:` value in `config.jadpo`;
    secrets need none, because `jadpo test` generates random ones; a single
    test can override a value. *Built in chunk 5.1.*

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Unnamed declarations | Carried from area 1: `config { }` and `locales { }` carry no name | 2026-10-12 |
| Root files | Carried from area 1: settings live in `config.jadpo`; there is no `env.jadpo`; values come from the platform environment and one `.env.local`, with secrets entered through `jadpo config set` | 2026-10-12 |
| Translations folder | Carried from area 1: `translations/` is a fixed root folder; its format is decided here | 2026-10-12 |
| Failure messages | Carried from area 4: clients translate by failure `code`; `message` stays fixed English | 2026-10-12 |
| Store addresses | Carried from area 5: stores are declared in `app.jadpo` and take `url: config.<field>` | 2026-10-12 |
| Signature types | Carried from area 2: signatures use named types built on `Instant`, `CalendarDate`, `Time` and `Duration`; `Zone`, `Locale` and `Weekday` may be used directly | 2026-10-12 |
| Option names | Carried from area 2: closed options in settings are lower_snake_case names; in code, enum values are qualified (Superseded 2026-10-12: options are written `Type.value`; see area 2 question 17.) | 2026-10-12 |
| Environment variable names | Derived from the field name (`mail_sender` reads `MAIL_SENDER`); `binding:` only to differ; a changed name shows in the approval review | 2026-10-12 |
| Setting types | Adds enums, `Zone` and `Locale`; no lists or objects | 2026-10-12 |
| Database address type | Built-in `PostgresUrl` and `RedisUrl`, always secret, matching the store's engine; startup and `config check` share one validator | 2026-10-12 |
| Duration spelling | `500ms`, `30s`, `5m`, `2h`, `30d` everywhere in source, `d` exactly 24 hours; ISO 8601 only in JSON | 2026-10-12 |
| `Time` name | Renamed `ZonedTime` | 2026-10-12 |
| Instant arithmetic | Adding or subtracting a `Duration` keeps the named type; `Instant - Instant` gives `Duration` | 2026-10-12 |
| Time library and formatting | Option B: `temporal.format(value, zone:, locale:, format:)` with named formats, one `dst:` option failing by default, about 20 functions | 2026-10-12 |
| Locales declaration | `locales: { default: Locale.en_gb supported: [...] }` in `app.jadpo`; `Locale` built in; no `unsupported:` | 2026-10-12 |
| Translations format | One JSON file per locale with ICU MessageFormat; typed calls from the default locale's keys; missing keys or placeholders are compile errors; built with the first email template | 2026-10-12 |
| Signing key rotation | Built-in `SigningKey` reading `NAME` and optional `NAME_PREVIOUS`; sign with current, accept both | 2026-10-12 |
| Test values for settings | `test:` on the field; random secrets generated by `jadpo test`; per-test override | 2026-10-12 |
| Translations from the start | Built into the first build, used by mail, generated pages and client failure messages (Richard) | 2026-10-13 |
| Reading `config { }` (chosen in chunk 5.1) | A setting is `name: Type` with an optional block of `secret:`, `default:`, `binding:` and `test:`, each once and no others (`configuration.duplicate_option`, `configuration.unknown_option`); a line that is not a setting is `configuration.expected_field`, a second `config { }` is `configuration.multiple_declarations`, and a name that is no setting is `configuration.field_unknown` with the nearest name. A variable set to nothing is not set; the process's environment wins over `.env.local` | 2026-10-16 |
| What a setting can be (chosen in chunk 5.1) | A named or built-in type of one piece of text, a number, `Bool`, `Uuid`, `Duration`, `Instant` or `CalendarDate`, an enum with no data, `Zone` (read as `Europe/London`) or `Locale` (read as `en-GB`). Not `Bytes`, `Time`, a list, object, map or `T?`, or the types the runtime makes (`PageCursor`, `PageSize`, `ApiToken`, `ServiceKey`): `configuration.type_unsupported`. A secret is text, so a test run can make one. A setting may be declared with a built-in type; naming the type code passes it as saves building it | 2026-10-16 |
| Variables (chosen in chunk 5.1) | A variable is capitals, digits and `_` starting with a capital (`configuration.binding_invalid`, `configuration.binding_empty`); no two settings share one, and a `SigningKey`'s `_PREVIOUS` counts as taken (`configuration.duplicate_binding`) | 2026-10-16 |
| `default:` and `test:` (chosen in chunk 5.1) | A literal of the setting's type, checked against every rule of the type and its parents (`configuration.default_literal_required`, `configuration.default_type`, `configuration.default_invalid`, the same three for `test:` and a test's `config:`); an enum's variant is written `Type.value`. A secret has neither (`configuration.secret_default`), and `secret:` is `true` or `false` (`configuration.secret_invalid`) | 2026-10-16 |
| Built-in connection types (chosen in chunk 5.1) | `Url` accepts only `http://` and `https://`. `PostgresUrl` (`postgres://`, `postgresql://`), `RedisUrl` (`redis://`, `rediss://`) and `SigningKey` (at least 32 visible characters) are built in, text underneath, always secret and never stored by an entity | 2026-10-16 |
| Where a secret may go (chosen in chunk 5.1) | The value of a `url:`, `credential:`, `signing_key:` or `secret:` member of `application { }` or `authentication { }`, and nowhere else: no function, action, query, fixture or test is a place the compiler owns, so any read of a secret there is `configuration.secret_flow`, which needs no tracking through variables and calls, and covers comparing | 2026-10-16 |
| Connection addresses (chosen in chunk 5.1) | A connection's `url: config.<name>` reads a setting of the type its driver needs (`configuration.url_type`); an address written in source is refused the same way. A project with no database declared opens `main` at `config.database_url` when it declares one (of the right type) | 2026-10-16 |
| Values in tests (chosen in chunk 5.1) | A test reads each setting's `test:` value, then its default; a secret is a fresh random hexadecimal value for each run (a `PostgresUrl` or `RedisUrl` an address at `127.0.0.1:1`); the environment is never read. A test's `config:` changes non-secret settings for that test and the code it calls; a secret there is `configuration.secret_default`. A setting a test reaches through calls with none of the three is `configuration.test_value_missing` (a request through a route is not followed; the run's fault names it) | 2026-10-16 |
| Starting (chosen in chunk 5.1) | `jadpo_runtime::config::load` checks every setting against its type with a host's variables (`Host::variable`: the process natively, bindings on Workers) and gives each problem as the setting, its variable and the rule it breaks, never the value; the same function is behind `jadpo config check` and the interpreter, so they cannot disagree | 2026-10-16 |
| `jadpo config` (chosen in chunk 5.1) | `check` lists each setting as `set`, `default`, `missing` or `invalid` with its variable, shows each problem as `configuration.value_missing` or `configuration.value_invalid`, exits 1 on any and supports `--json`. `set <field>` needs a terminal and refuses `--json`, hides what is typed for a secret and shows it otherwise, checks the value before writing, and writes `.env.local` only as an ordinary file, through a private temporary file, if unchanged since read, and adds it to `.gitignore` | 2026-10-16 |
| `jadpo dev` and `jadpo migrate` (chosen in chunk 5.1) | `dev` checks the environment over `.env.local` as the app starts, watches `.env.local` with the source, and keeps the last good version serving through an invalid change; a connection whose address no variable gives gets a database on a disposable PostgreSQL (`--url` still names the database directly). `migrate apply` and `status` use a connection's setting when `--url` is not given | 2026-10-16 |
| Retention period (chosen in chunk 5.1) | `purge: { after: config.x }` takes a `Duration` setting that is not secret (`data.lifecycle_purge_after` otherwise); the period is the value the app started with, given to a run (`jadpo_runtime::purge_after`), and a period not above zero deletes nothing | 2026-10-16 |
| Not built here (chosen in chunk 5.1) | A browser session's `origin:` and `signing_key:` reading settings, with signed sessions and tokens, are chunk 5.1b. Which proxies a deployment trusts for a client's address is decided on no page: chunk 5.1c | 2026-10-16 |
