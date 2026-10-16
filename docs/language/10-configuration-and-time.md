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

type FromAddress = Email {}

config {
    page_size: ListLimit {
        default: 25
    }
    mail_sender: FromAddress {
        test: "reminders@example.test"
    }
}

function first_page() -> ListLimit {
    return config.page_size
}

function from_address() -> FromAddress {
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
    verbose
    normal
    terse
}

config {
    log_level: LogLevel {
        default: LogLevel.normal
    }
    business_zone: Zone {
        default: Zone.europe_london
    }
}
```

`LOG_LEVEL=verbose` and `BUSINESS_ZONE=Europe/London` are valid values: an enum
is read as the variant's name, a `Zone` as its IANA name and a `Locale` as its
BCP 47 tag (`en-GB`). A `Duration` is written as it is in source (`30s`, `5m`,
`2h`, `30d`), a `Bool` as `true` or `false`, and an `Instant` and a
`CalendarDate` in ISO 8601. A variable set to nothing counts as not set.

### Defaults and test values

A default is a literal of the field's type, checked while compiling
(`default: 25`, `default: 5s`, `default: LogLevel.normal`), against every rule of
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
$ jadpo config set session_signing_key
? Enter SESSION_SIGNING_KEY hidden
session_signing_key configured locally
    variable  SESSION_SIGNING_KEY
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
Values
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
Authentication reads the key as `signing_key: config.session_signing_key`,
and with a revocation delay signs sessions and tokens with it (area 8, "The
application and revocation").

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
    supported: [Locale.en_gb, Locale.en_us, Locale.fr_fr, Locale.pl_pl]
    in_progress: [Locale.pl_pl]
}
```

`in_progress:` (decided 2026-10-16, built in chunk 5.3) lists supported
locales whose translations are not finished yet; a locale listed there but
not in `supported:` is a compile error. Every other supported locale must
be complete; only one listed here falls back where wording is missing,
with warnings (see Translations).

There is no `unsupported:` setting. A request field typed `Locale` is decoded like
any enum, so an unsupported tag is an `invalid_value` failure; choosing a
language from an `Accept-Language` header is a library call that falls back to
`default`.

Every locale knows its text direction, read as `locale.direction`
(decided 2026-10-16, built in chunk 5.3): a value of the built-in enum
`TextDirection`, `.left_to_right` or `.right_to_left`. Jadpo ships its
wording as the HTML values, so a template writing
`<html dir="{{ locale.direction }}">` writes `ltr` or `rtl`.

## Translations

> Decided on 2026-10-16 (Richard), replacing question 9's JSON files and ICU
> MessageFormat. Built in chunk 5.3; email templates that read translations
> come with email in chunk 7.3 (area 16).

Text the server writes for people (emails, generated pages, a failure's text
for client apps) is translated in Jadpo itself. Each message is a type, each
language is a file of wordings, and both live in the domain that uses them.

### Where translations live

In the domain that uses them, in `domains/<domain>/translations/`, as its
emails live in `emails/`. A message a second domain uses moves to
`shared/translations/` under area 1's promotion rule. The root
`translations/` holds only the app's wording for text it did not write:
Jadpo's own and its library packages' (below).

```text
domains/shipping/translations/
    order_shipped.jadpo       type OrderShipped = Message { ... }
    parcels.jadpo             type Parcels = Message { ... }
    en_gb.jadpo               translations { ... }: the English wording
    de_de.jadpo               translations { ... }: the German wording
    sources.json              generated: what each translation was made from
shared/translations/          messages several domains use
translations/                 the app's wording for Jadpo's and packages' text only
```

### Messages

A message is a type built on `Message`, a type base beside `Failure`,
`Event` and `EmailMessage`, one per file in the domain's `translations/`
folder and named after it (area 1). Its fields are the wording's
placeholders, and its `///` description reaches translators as context,
in the export below.

```jadpo fragment
// domains/shipping/translations/order_shipped.jadpo
/// Subject of the email sent when a parcel leaves the warehouse.
type OrderShipped = Message {
    tone: Tone            // an enum: one wording per value
    name: Text
}

// domains/shipping/translations/parcels.jadpo
/// How many parcels an order has, on the tracking page.
type Parcels = Message {
    count: PluralCount    // plural forms, per language
}
```

`PluralCount` is a built-in whole number (area 2) that picks a plural form.

### Languages

Each language is one unnamed `translations { }` declaration in Jadpo
syntax, in a file named after its locale (`de_de.jadpo`), found by its name
as `config { }` is; code never refers to it by name. A member is a
message's, failure's, enum's or email's name in lower_snake_case, found by
area 1's name conversion (`order_shipped` is `OrderShipped`). Every wording
is a plain string whose only syntax is `{field}`; a literal brace is
doubled, `{{` writing `{` and `}}` writing `}`. There is no grouping inside a
file (the domain is the group, and nesting means variants and plural forms
only, except the root file's groups by owner, below), and no message inside another: a reused value such as a product name
is a placeholder, because languages that inflect cannot reuse a fragment.

```jadpo fragment
// domains/shipping/translations/de_de.jadpo
translations {
    order_shipped: {
        formal: "Guten Tag {name}, Ihre Bestellung wurde versandt."
        casual: "Hallo {name}, deine Bestellung ist unterwegs!"
    }
    parcels: {
        one: "{count} Paket"
        other: "{count} Pakete"
    }
    delivery_window: "Lieferung am {day}, zwischen {hours}."
    slot_taken: "Dieser Termin ist gerade vergeben worden. Der nächste ist {next_slot}."
}
```

How a message's type maps to its wording in each language:

| In the type | In each language's file |
|---|---|
| The type's name, `OrderShipped` | The member `order_shipped`, in the language files of the type's own `translations/` folder |
| No fields | A plain string |
| A `Text`, `PresentationText`, `Int` or `Decimal` field, or a named type built on `Text`, `Int` or `Decimal` | A `{field}` placeholder; numbers are written the locale's way |
| One enum field | One member per value of the enum; the field is not a placeholder |
| One `PluralCount` field | One member per plural form the language needs, from Unicode CLDR (English `one` and `other`; Polish `one`, `few`, `many` and `other`); `{count}` is available in each |
| Any other field type (`Instant`, `Money`, an object) | A compile error: format it in code and pass the text |
| More than one enum or `PluralCount` field | A compile error: split the message, so a translator never fills a grid of combinations |

Dates, times and money are formatted in code (`temporal.format`, the money
formatter) and passed in as `PresentationText`.

### Using a message

The compiler generates one call per message,
`translations.<domain>.<message>(...)`, taking every field by name plus
`locale:`, one of the app's supported locales. It returns
`PresentationText`, so the result can be shown but never stored.

```jadpo fragment
var subject: PresentationText = translations.shipping.order_shipped(
    tone: shop.tone,
    name: customer.first_name,
    locale: customer.locale
)
```

A message in `shared/translations/` is called
`translations.shared.<message>(...)`. In an email template the same call
takes the email's locale and is written without `locale:`:
`{{ translations.shipping.parcels(count: parcel_count) }}` (area 16).

### Email subjects

An `EmailMessage`'s member in its domain's language files is its subject
line, with the email's fields as placeholders; its body is its `.html` and
`.txt` templates (area 16).

```jadpo fragment
// domains/accounts/translations/en_gb.jadpo
translations {
    welcome_email: "Welcome, {display_name}"
}
```

### Enums with wording

An enum can be written out once its domain's language files give each of
its values a wording, checked like a message with one enum field; a value's
fields can be used as placeholders. In a template `{{ order.status }}` then
writes the wording, and code reads it with
`translations.<domain>.<enum>(value, locale: ...)`. Without wording,
writing an enum out is an error that says to add it. Area 2's "enums have
no display labels" stands: the wording lives in translations.

```jadpo fragment
// domains/shipping/translations/en_gb.jadpo
translations {
    order_status: {
        pending: "Waiting to be packed"
        held: "Held: {reason}"
        shipped: "On its way"
    }
}

var status: PresentationText = translations.shipping.order_status(order.status, locale: customer.locale)
```

### Failures for client apps

A `Failure`'s text for client apps is keyed by its code in its domain's
language files, mapped like a message: its `client: { }` fields are the
placeholders, and `kind:` and `message:` are not. The wording is never
written on the failure type: one rule holds for all text, and Jadpo's and
packages' failures could not carry the app's wording in place. Names are
unique across the project, so a failure and a message never share a key.
Every failure
that can reach a response (the compiler knows each route's failures) must
have wording in every supported locale; a failure that never leaves the
server needs none.

A client field that is a date, a time or a number is sent as a value (an
`Instant` stays an instant on the wire, area 7), and the client formats it
for its own language before filling the placeholder, so unlike a
message's, a failure's date or time field can be a placeholder. The
field's type decides how the client formats it:

| Client field type | Format |
|---|---|
| `CalendarDate` | `date` |
| `Instant` | `date_time`, shown in the reader's own zone by the client |
| `Time` (`ZonedTime` once renamed, question 5) | `date_time`, in the zone it carries |
| `Duration` | `duration` |
| `Int` | `number` |
| `Decimal` | `decimal` |
| `Text`, `PresentationText` | none: written as it is |
| A type the client cannot format (an object, a list) | A compile error, as for a message's placeholder |

`jadpo build` writes, beside each API's OpenAPI document, one file per
locale named `<api>.failures.<locale>.json`, for client apps to show. It
maps each failure code to an object of the same shape, so a client has one
way to read it: `text` is the wording in `{field}` syntax, and `formats`
lists only the placeholders the client must format, each as one of
`number`, `decimal`, `date`, `date_time` or `duration`.

```json
{
  "slot_taken": {
    "text": "Ce créneau vient d'être pris. Le prochain est {next_slot}.",
    "formats": { "next_slot": "date_time" }
  },
  "todo_not_found": { "text": "Tâche introuvable.", "formats": {} }
}
```

The server's `message` stays fixed English for developers and logs (area 4).

```jadpo fragment
// domains/shipping/translations/fr_fr.jadpo
translations {
    slot_taken: "Ce créneau vient d'être pris. Le prochain est {next_slot}."
}
```

### Jadpo's and packages' text

Text Jadpo writes itself (built-in emails and its failures' wording; it
generates endpoints only, never pages) is Jadpo's own messages. Jadpo ships their wording in
`en_gb` and `en_us` at first, and releases add locales; an app supporting
any other locale must word them itself, and until it does `jadpo check`
fails with one error listing the missing messages (an `in_progress:`
locale falls back instead). Only the messages of features the app has
switched on count: rewording one of a feature that is off (magic-link
wording without `magic_link:`) is an error saying the feature is off, and
`jadpo translations builtin` lists only the switched-on features'
messages, with their fields. Every Jadpo message may use `{app_name}`,
the application's `display_name:`; an app that needs the name translated
rewords the messages. The magic-link email's messages are listed in area
16, "Built-in emails".

A library package carries translations as a domain does: a
`translations/` folder with its message types, its failures' wording and
one `<locale>.jadpo` per language it supports. The app calls a package's
message as `Postcodes.translations.<message>(...)`. A provider package
shows users no text, because its failures are always mapped to the app's
own, so it carries none.

The root `translations/<locale>.jadpo` holds the app's wording for text it
did not write, grouped by owner: `jadpo: { }` for Jadpo's own and the
package's name for a package's. This is the one place a language file
groups, the group being whose text it is, and each group is checked
against its owner's message and failure types.

```jadpo fragment
// translations/fr_fr.jadpo
translations {
    jadpo: {
        privacy_export_ready: "Votre export de données est prêt."
    }
    postcodes: {
        invalid_postcode: "Ce code postal n'existe pas."
    }
}
```

A package's or Jadpo's message is looked up in the app's root wording for
that locale, then the owner's own; for an in-progress locale, then the
regional sibling in the same order, then the default locale in the same
order. The app's supported locales must be complete for every package and
Jadpo message the app can show, so an English-only package in an app that
supports French is an error until the root file supplies the French.

To show a package failure under a different code, not just different
words, map it to the app's own failure at the call with `attempt`, as for
any package failure (area 11, Calling).

A package changing a message's or failure's fields is a breaking change by
its surface diff, so the app's rewording fails to compile until it is
updated; a package changing its own wording marks the app's rewording
stale through `sources.json`. A library package is published with
complete wording in its own default locale and a `///` description on
every public message.

### What the compiler checks

Coming in chunk 5.3:

- **Errors**: a member no message, failure or enum declares; a variant that
  is not a value of the enum; a missing variant or plural form; a
  placeholder that is not a field; a field type a wording cannot hold, or
  more than one enum or `PluralCount` field; a `locale:` that is not a
  supported locale; and a message, a failure that can reach a response, or
  an enum's wording missing from any supported locale not listed in
  `in_progress:`. Every supported locale must be complete. The missing
  wording's `next:` names the file and the member to add, with the
  placeholders to use (agent).
- **Warnings**: a field no wording in a language uses (one variant leaving
  a field out is normal); wording missing from an in-progress locale,
  which falls back to the same language's regional sibling first (`fr_ca`
  tries `fr_fr`, if supported) and then to the default locale, listed by
  `jadpo translations missing`; removing the locale from `in_progress:`
  makes the gaps errors again. Quality comes from the review setting below,
  not from waiting. A stale translation, made from an older version of the
  default wording, warns until it is updated.
- **Sources**: each `translations/` folder has a generated, committed
  `sources.json` (machine-written, so JSON, and a record in area 18): for
  each locale and message, a hash of the default wording it was translated
  from. A translation whose hash no longer matches the default wording is
  stale. `jadpo translations update` and `import` write it.
- **Review**: `translations: { review: TranslationReview.required }` in
  `app.jadpo` (the default is `TranslationReview.none`) makes every new or
  changed wording outside the default locale unreviewed until a person
  runs `jadpo translations accept <locale>`, which records the acceptance
  in the same `sources.json`. Unaccepted wording appears in the approval
  report. Jadpo cannot tell who wrote a wording, so this is "changed and
  not yet accepted", not "written by an agent".

### Tooling

- `jadpo translations missing` lists missing, stale and unaccepted
  translations, and `jadpo translations unused` messages no code uses.
  Every `jadpo translations` command that prints takes `--json` (area 14).
- `jadpo translations update` records each translation's source in
  `sources.json`; `jadpo translations accept <locale>` accepts a locale's
  changed wording; `jadpo translations builtin` lists Jadpo's own messages
  with their fields.
- `jadpo translations export` and `import` exchange messages, their
  descriptions and their wording with translation services (Crowdin,
  Lokalise) as XLIFF, or JSON.
- Go to definition goes from a call to its message type and to its wording.
- A pseudo-locale, for development and tests, renders every message
  accented and lengthened, to expose hard-coded text and tight layouts.

The app's own data (product names, articles) is not a translation: an app
models translated content as ordinary fields. Library packages' translations
are built with packages in Phase 7 (open item 8 of the
[packages proposals](research/packages-and-extension-proposals.md)).

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
type FromAddress = Email {}

config {
    mail_sender: FromAddress
}

function from_address() -> FromAddress {
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
translations in chunk 5.3: `Message` types and the built-in `PluralCount`,
each domain's `translations { }` language files and their checks, the
generated `translations.<domain>.<message>` calls, enum wording, failures'
client text exported per locale, `locale.direction`, `in_progress:` locales,
Jadpo's own messages and the root file's owner groups, and the
`jadpo translations` commands (decided 2026-10-16, see Translations).
Library packages' translations come with packages in Phase 7. Trusted
proxies, the addresses a client's address is read through, are not decided
on any page; they are chunk 5.1c.

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
| Mail templates using translations and `PresentationText` | 11 | Input to area 11 (Superseded 2026-10-16: templates read `translations.` and are checked strictly; see area 16 question 11.) |
| A library package's own messages, and whether an app can reword them | 11 | Waits for open item 8 of the packages proposals |
| `config.typo` should report the configuration field | 10 | Fixed (chunk 5.1): `configuration.field_unknown` |
| `jadpo fmt` writes `temporal.format (` with a space | 14 | Bug |
| Trusted proxies: which proxies a deployment trusts to say a client's address (`X-Forwarded-For`, `Forwarded`, Cloudflare's `CF-Connecting-IP`) | 7, 10, 16 | Decided 2026-10-16 (question 12): chunk 5.1c |
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
   *Revised 2026-10-16 (Richard):* JSON and ICU MessageFormat are dropped,
   and the default locale's file no longer defines the keys. Each message
   is a `type X = Message` in its domain's `translations/` folder, its
   fields the placeholders; each language is an unnamed `translations { }`
   in a file named after its locale, whose wordings are plain strings with
   `{field}` placeholders, plural forms from CLDR for a `PluralCount` field and
   one wording per value for an enum field. JSON has no comments and
   quotes everything, and the compiler parses only Jadpo (area 11). A
   failure's client text is keyed by its code in the same files and
   exported per locale for client apps, which resolves this question's
   "client failure messages" against area 4 question 12. See
   [Translations](#translations) for the whole design.
10. **How are signing keys rotated?** (carried from area 8) Area 8 keeps
    signing keys, the OIDC issuer and audience in configuration. How does a
    deployment replace a signing key without signing everyone out?
    *Decided 2026-10-12:* a built-in `SigningKey` type. `session_signing_key`
    reads `SESSION_SIGNING_KEY` and an optional `SESSION_SIGNING_KEY_PREVIOUS`;
    new tokens are signed with the current key and both are accepted. Rotate
    by setting the new key and moving the old one to `_PREVIOUS`, then remove
    it once the longest token lifetime has passed. *Built in chunk 5.1* as
    far as reading and checking the two values, and signing with them in
    chunk 5.1b (area 8).
11. **Do configuration fields carry `test:` values?** (carried from area 13)
    Area 13 has tests take each configuration field's `test:` value, while
    "Values on your machine and in production" above says tests get their
    values from fixtures. Which does configuration declare?
    *Decided 2026-10-12:* a field may carry a `test:` value in `config.jadpo`;
    secrets need none, because `jadpo test` generates random ones; a single
    test can override a value. *Built in chunk 5.1.*
12. **How does a deployment say which proxies to trust for a client's
    address?** (raised in chunk 5.1; areas 7 and 16 count requests by
    address, and no page says how the address is read.) Behind a load balancer
    the connection's address is the proxy's, so every client shares one
    rate-limit allowance until the real address is read from the forwarding
    header; reading it from a header anyone can send lets a caller choose
    its own address. Neither area 7, 10 nor 16 decides it. *Recommendation:*
    declare it once in `app.jadpo`, beside the connections, as
    `client_address: { source: ClientAddressSource.forwarded_for hops:
    config.proxy_hops }`, where `ClientAddressSource` is a closed set of
    four: `connection` (the default natively: the connection's own address,
    no header read), `forwarded_for` (`X-Forwarded-For`), `forwarded`
    (RFC 7239's `Forwarded: for=`) and `cloudflare` (`CF-Connecting-IP`),
    and `hops` is how many proxies of the deployment's own sit in front of
    the app, an `Int` setting (a list of addresses cannot be a setting, and a
    count cannot be spoofed the way an allow-list of ranges can drift). With
    `hops: N` the client is the N-th address from the right of the header's
    addresses, all its lines joined in order, since each trusted proxy
    appends the address it saw on its right and whatever a caller adds sits to
    the left; fewer than N addresses, or one that is not an address, falls
    back to the connection's own, never to a guess. On Workers there is no
    connection address to fall back to, and Cloudflare sets `CF-Connecting-IP`
    on every request from the edge and overwrites a caller's, so `cloudflare`
    is the only source accepted there (and its default); `hops` means nothing
    with it. The address chosen is the one rate limits count, `jadpo dev`'s
    request lines show and the request timeline records (areas 16 and 17).
    *Decided 2026-10-16:* yes, as recommended; chunk 5.1c builds it.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Unnamed declarations | Carried from area 1: `config { }` and `locales { }` carry no name | 2026-10-12 |
| Root files | Carried from area 1: settings live in `config.jadpo`; there is no `env.jadpo`; values come from the platform environment and one `.env.local`, with secrets entered through `jadpo config set` | 2026-10-12 |
| Translations folder | Carried from area 1: `translations/` is a fixed root folder; its format is decided here (Superseded 2026-10-16: translations live in each domain's `translations/` folder; the root folder holds only the app's wording for Jadpo's own messages.) | 2026-10-12 |
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
| Locales in progress | `in_progress: [...]` in `locales:` lists supported locales allowed to fall back with warnings, and one not in `supported:` is an error; the rest must be complete (Richard) | 2026-10-16 |
| Failure export shape | `<api>.failures.<locale>.json` maps each code to `{ "text": ..., "formats": { placeholder: format } }`, the same shape for every code; `formats` lists only placeholders the client formats, as `number`, `decimal`, `date`, `date_time` or `duration`, decided by the field's type (`CalendarDate` date, `Instant` date_time in the reader's zone, `Time` date_time in its own zone, `Duration` duration, `Int` number, `Decimal` decimal); a type the client cannot format is an error (decided for Richard) | 2026-10-16 |
| Translations format | One JSON file per locale with ICU MessageFormat; typed calls from the default locale's keys; missing keys or placeholders are compile errors; built with the first email template (Superseded 2026-10-13 and 2026-10-16: built from the start; the format is under Translations, above.) | 2026-10-12 |
| Signing key rotation | Built-in `SigningKey` reading `NAME` and optional `NAME_PREVIOUS`; sign with current, accept both | 2026-10-12 |
| Test values for settings | `test:` on the field; random secrets generated by `jadpo test`; per-test override | 2026-10-12 |
| Translations from the start | Built into the first build, used by mail, generated pages and client failure messages (Richard) | 2026-10-13 |
| Where translations live | In the domain that uses them, `domains/<domain>/translations/`; a message a second domain uses moves to `shared/translations/` under area 1's promotion rule; the root `translations/` holds only the app's wording for Jadpo's own messages (Richard) | 2026-10-16 |
| Messages | `type X = Message { ... }`, a type base beside `Failure`, `Event` and `EmailMessage`, one per file; fields are placeholders; `///` descriptions are translator context (Richard) | 2026-10-16 |
| Language files | One unnamed `translations { }` per locale in a file named after it (`de_de.jadpo`); members are message, failure, enum or email names in lower_snake_case; every wording a plain string whose only syntax is `{field}`, a literal brace doubled (`{{`, `}}`); no grouping or nesting beyond variants and plural forms; JSON and ICU MessageFormat dropped (Richard) | 2026-10-16 |
| Mapping a message to wording | No fields: a string; `Text`, `PresentationText`, `Int`, `Decimal`: placeholders, numbers written the locale's way; one enum field: one wording per value; one `PluralCount` field: one wording per CLDR plural form; any other field type, or more than one enum or `PluralCount` field, an error; a placeholder that is not a field an error; a field no wording uses a warning (Richard) | 2026-10-16 |
| Generated calls | `translations.<domain>.<message>(field: value, ..., locale: ...)`, every field by name, returning `PresentationText`; `translations.shared.<message>` for `shared/translations/`; in email templates the email's locale (Richard) | 2026-10-16 |
| Email subjects | An `EmailMessage`'s member in its domain's language files is its subject line, its fields usable as placeholders; the body is the `.html` and `.txt` templates (Richard) | 2026-10-16 |
| Enum wording | An enum can be written out once its domain's language files give each value a wording, a value's fields usable as placeholders; area 2's "no display labels" stands (Richard) | 2026-10-16 |
| Failures' client text | Keyed by the failure's code in its domain's language files, client fields as placeholders; `jadpo build` writes `<api>.failures.<locale>.json` beside each API's OpenAPI document, from code to wording in `{field}` syntax (revised the same day: with each placeholder's formatting); the server's `message` stays English (Richard) | 2026-10-16 |
| Translation checks | Every supported locale is complete: missing wording for a message, a failure that can reach a response or an enum is an error whose `next:` names the file, member and placeholders; only a locale in `locales: { in_progress: [...] }` falls back with warnings, regional sibling first (`fr_ca` to `fr_fr`), then the default; stale translations warn, judged by a hash of the default wording in each folder's generated, committed `translations/sources.json`; with `translations: { review: TranslationReview.required }` in `app.jadpo` (default `.none`), new or changed non-default wording is unaccepted, and listed in the approval report, until `jadpo translations accept <locale>` (Richard) | 2026-10-16 |
| Text direction | `locale.direction` is the built-in enum `TextDirection` (`.left_to_right`, `.right_to_left`), whose shipped wording is `ltr` and `rtl` (Richard) | 2026-10-16 |
| Jadpo's own text | Jadpo's own messages, with wording shipped in `en_gb` and `en_us` at first and more locales added by releases; another supported locale is an error listing the missing messages until the app words them; only switched-on features' messages count, rewording another is an error, and `jadpo translations builtin` lists only those (Richard) | 2026-10-16 |
| Text the app did not write | The root `translations/<locale>.jadpo` groups wording by owner, `jadpo: { }` and a package's name, the one grouping allowed, checked against the owner's types; lookup is the app's root wording, then the owner's, then (in progress) the regional sibling, then the default; supported locales must be complete for every package and Jadpo message the app can show (Richard) | 2026-10-16 |
| Package translations | A library package carries a `translations/` folder like a domain's, called `Postcodes.translations.<message>(...)`; provider packages carry none; changed fields break the app's rewording, changed wording makes it stale; published with complete default-locale wording and `///` on every public message; a different code for a package failure is mapped with `attempt` (Richard) | 2026-10-16 |
| Failures sent to clients | Wording lives in the language files, never on the failure type (one rule for all text; Jadpo's and packages' failures could not carry it in place); every failure that can reach a response has wording in every supported locale; dates and numbers in client fields are sent as values and formatted by the client, and the per-locale export says which placeholders need formatting and how (Richard) | 2026-10-16 |
| Translation tooling | `jadpo translations missing` and `unused` with `--json`; `update`, `accept <locale>` and `builtin`; `export` and `import` as XLIFF or JSON; `--json` wherever they print; go to definition from a call to its type and wording; a pseudo-locale for development and tests; the app's own data is not a translation (Richard) | 2026-10-16 |
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
| Not built here (chosen in chunk 5.1) | A browser session's `origin:` and `signing_key:` reading settings, with signed sessions and tokens, are chunk 5.1b (built there, area 8). Which proxies a deployment trusts for a client's address is decided on no page: chunk 5.1c | 2026-10-16 |
| Trusted proxies | `client_address: { source: ClientAddressSource.x hops: <Int> }` once in `app.jadpo`; sources `connection` (the native default), `forwarded_for`, `forwarded` and `cloudflare`; the client is the `hops`-th address from the right, falling back to the connection's address when the header is short or invalid; on Workers only `cloudflare` (`CF-Connecting-IP`), its default (question 12, Richard) | 2026-10-16 |
