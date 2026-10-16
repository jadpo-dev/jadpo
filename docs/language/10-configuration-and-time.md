---
title: Configuration and time
area: 10
status: decided
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - configuration.binding_empty
  - configuration.binding_invalid
  - configuration.client_address_duplicate
  - configuration.client_address_hops_invalid
  - configuration.client_address_hops_unused
  - configuration.client_address_invalid
  - configuration.client_address_source_unknown
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
  - type.temporal_clock_invalid
  - type.temporal_option_unused
  - type.temporal_zone_required
  - type.temporal_format_component_duplicate
  - type.temporal_format_component_invalid
  - type.temporal_format_components_empty
  - type.temporal_format_invalid
  - type.temporal_format_requires_time
  - type.temporal_locales_required
  - type.locale_default_unsupported
  - type.locale_duplicate
  - type.locale_invalid
  - type.locale_unsupported
  - syntax.locales_default_required
  - syntax.locales_duplicate_option
  - syntax.locales_supported_required
  - syntax.locales_unknown_option
  - semantic.multiple_locale_declarations
  - type.presentation_text_persistence
---

# Configuration and time

How an application declares its settings and secrets, where their values come
from, and how code reads the current time, works with dates and zones, and
shows times to people in their own language.

> Decided on 2026-10-12. Code on this page applies the decisions from areas 1 to 10. The new compiler has built the time literals and the built-in `Zone`, `Weekday` and `Locale` enums (chunk 1.9) and checks where `clock.now` may be read. Chunk 5.1 built settings: `config.jadpo`, secrets, signing keys, the connections' addresses, `jadpo config`, the check before the app starts and a test's own values; chunk 5.1c built the client's address behind trusted proxies (`client_address:`). Chunk 5.2 built time: `ZonedTime`, durations, arithmetic and comparison, the `temporal` library over a bundled time zone database, and a test's clock; chunk 5.2b built the formats and `PresentationText`, and checks `temporal.format` and `format_relative`. The examples marked `check` in those sections and their errors run with this checkout's `jadpo`. Chunk 5.3 built locales (`locales { }`, checked wherever a locale is written) and the text `temporal.format` and `format_relative` give, in each locale's own words, order and digits; translations come in chunk 5.3b, and choosing a language from `Accept-Language` in chunk 5.3c.

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

## Client addresses

Rate limits count a request by its address while no one is signed in (areas
7 and 16), and `jadpo dev`'s request lines and the request timeline (area 17)
show it. Behind a load balancer or a CDN the connection's own address is the
proxy's, so every client would share one allowance until the real address is
read from the header the proxy adds; but a header anyone can send lets a
caller choose its own address. `client_address:` in `app.jadpo` says, once,
where the real address is and how many proxies of the deployment's own to
believe (question 12):

```jadpo check project=client-address file=config.jadpo
use edge/types { ProxyHops }

config {
    proxy_hops: ProxyHops {
        default: 1
    }
}
```

```jadpo check project=client-address file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hops: config.proxy_hops
    }
}
```

```jadpo check project=client-address file=domains/edge/types/proxy_hops.jadpo hidden
type ProxyHops = Int {
    min: 1
    max: 8
}
```

`ClientAddressSource` is a closed set of four:

| Source | The client's address is | `hops:` |
|---|---|---|
| `connection` | the connection's own address; no header is read (the native default) | not allowed |
| `forwarded_for` | in `X-Forwarded-For` | required |
| `forwarded` | in RFC 7239's `Forwarded: for=` | required |
| `cloudflare` | in `CF-Connecting-IP` (the default on Workers, the only source accepted there) | not allowed |

`hops` is how many proxies of the deployment's own sit in front of the app. It
is a count and not a list of trusted ranges, so it cannot drift when a
provider's addresses change. Each trusted proxy adds the address it saw to the
right of the list, and whatever a caller sends sits to the left of that, so
with `hops: N` the client is the N-th address from the right of the header's
addresses, every line of the header joined in order: with one proxy,
`X-Forwarded-For: 203.0.113.9` and with a caller's own claim in front,
`X-Forwarded-For: 10.9.9.9, 203.0.113.9`, are both `203.0.113.9`. If the header
has fewer than N addresses, or the one chosen is not an address, the request
counts as the connection's own address, never as a guess. `hops` is a whole
number from 1, written out (`hops: 1`) or read from an `Int` setting whose
type declares `min: 1` or more, so a count below 1 cannot be started with.

The address chosen is the one the default rate limits count, `jadpo dev`'s
request lines show (after the time, muted, and as `address` with `--json`) and
the request timeline records. Natively the connection's address comes from the
host and the rest is the runtime's, so a deployable chooses an address the
way `jadpo dev` does (`jadpo_runtime::http::ClientAddress`).

On Workers there is no connection address to fall back to, and Cloudflare sets
`CF-Connecting-IP` on every request from its edge and overwrites a caller's, so
`cloudflare` is the only source accepted there and what a deployable for
Workers uses without a `client_address:`. An address Cloudflare did not give
is unknown there, and requests with no known address share one allowance.

### Chosen in chunk 5.1c

- **What `hops` needs.** `forwarded_for` and `forwarded` need it, because a
  count of trusted proxies is never guessed; `connection` and `cloudflare`
  refuse it (`configuration.client_address_hops_unused`), since no header is
  counted from the right.
- **An address.** Anything `IpAddress` accepts, as IPv4 or IPv6, and in a
  header also with a port (`203.0.113.9:4711`) or in brackets
  (`[2001:db8::17]:4711`, `[2001:db8::17]`). It is kept in one spelling, so a
  client is one count whichever way a proxy writes it: IPv6 in lower case with
  the zeros shortened, and an IPv4 address written as IPv6
  (`::ffff:203.0.113.9`) as IPv4. The connection's own address is spelled the
  same way. A name, `unknown`, an obfuscated identifier (`_hidden`) or a
  zone (`fe80::1%eth0`) is not an address.
- **Reading the lists.** Every header line of the name counts, in the order
  received, and each is split at commas. An empty entry still takes its place
  in the count, so a gap never moves the client left, but it is not an address.
  In `Forwarded`, each element (split at commas outside quotes) gives one
  entry, from its `for` parameter (any case; a token or a quoted string); an
  element without one is an entry that is not an address. A `Forwarded` header
  that does not parse (an unterminated quote, two `for` in one element) is
  invalid as a whole.
- **`cloudflare`.** Exactly one `CF-Connecting-IP` line holding one address;
  anything else falls back, as a header that is short or invalid does.
- **Falling back is whole.** The connection's address replaces the choice; part
  of a header is never mixed with it.
- **Settings.** A setting that `hops:` reads must be an `Int` (or a type built
  on it) that declares `min:` of at least 1
  (`configuration.client_address_hops_invalid` otherwise).
- **Workers.** A source other than `cloudflare` is refused when the project
  is built for Workers; deployables are declared in chunk 8.2, which makes
  that check (`ClientAddress::read` already refuses it for that target).

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
| `Instant` | One point on the timeline, in UTC, to the millisecond | when a todo was created | `"2026-01-15T12:00:00Z"`, `"2026-01-15T12:00:00.250Z"` |
| `CalendarDate` | A date with no time or zone | a birthday, an invoice date | `"2026-10-25"` |
| `ZonedTime` | An `Instant` seen in a `Zone`, for showing or reasoning about local time | "09:30 in London" | `{ "instant": "2026-10-25T08:30:00Z", "zone": "Europe/London" }` |
| `Duration` | Elapsed time | a timeout, a retention period | `"PT5M"` |
| `Zone` | An IANA time zone, a built-in enum | `Zone.europe_london` | `"Europe/London"` |
| `Weekday` | A built-in enum | `Weekday.monday` | `"monday"` |

- Store `Instant`. Use `CalendarDate` only for facts that really are dates.
  There is no time of day on its own and no date-time without a zone:
  `ZonedTime` (renamed from `Time`, question 5) holds an instant and the
  zone it is seen in.
- `Instant` keeps milliseconds: the precision JavaScript clients, SQLite and
  PostgreSQL all share. A literal with microseconds is rejected, and
  writes to PostgreSQL's microsecond column are always whole milliseconds
  (carried from area 5).
- Types never convert to each other or to `Text` silently; signatures use
  named types built on them (decided in area 2), such as
  `type DueAt = Instant {}`.
- Built in chunk 1.9 (decided in the Phase 1 review): an `Instant` literal is
  ISO 8601 with a `Z` or an offset, to the millisecond
  (`"2026-10-25T08:30:00Z"`), a `CalendarDate` literal a real date
  (`"2026-10-25"`), both checked while compiling; a `Duration` is written only
  with a unit (`15m`), and `min:` and `max:` on a `Duration` type are
  enforced; a `ZonedTime` has no literal. `Zone`, `Weekday` and `Locale` are
  built-in enums: `Zone` has a variant for each zone in the IANA database's
  `zone.tab` (2026d) plus `utc`, named in lower_snake_case
  (`Zone.america_argentina_buenos_aires`); `Locale` has one for each
  locale Unicode CLDR 48.2.3 gives modern coverage, plus the language with
  its likely region for each without one (`Locale.fr` and `Locale.fr_fr`,
  `Locale.en_gb`; 533 in all), named in lower_snake_case from the BCP 47
  tag. Both tables live in `jadpo-std` (`src/zones.rs`, which says how each
  was made) and change only in a change of their own.
- An `Instant` holds the years 0000 to 9999, up to
  `9999-12-30T22:00:00.999Z`, the last instant whose local date is in year
  9999 in every zone, and a `CalendarDate` the same years; a literal past
  them is refused while compiling, and arithmetic that goes past them is the
  fault `time_out_of_range` (chosen in chunk 5.2).
- Two `ZonedTime`s are equal when both their instants and their zones are.
  They have no order of their own, since two in different zones have none;
  their instants do, through `temporal.instant(...)` (chosen in chunk 5.2).
- A `ZonedTime` field of an entity is stored as an object is, as the API's
  JSON in `jsonb`, which is not built yet (area 5); store the `Instant` and,
  where the zone matters, a `Zone` field beside it (chosen in chunk 5.2).
- `clock.now` in a function, a library function or an inline function is
  `type.clock_context`; `clock.advance(...)` outside a test is
  `type.test_clock_context` (area 13); any other member of `clock`,
  `translations`, `sessions` or `oauth` is `toolchain.not_built_yet` until
  its area is built. `config.<name>` is a setting (chunk 5.1).

```jadpo check project=time-types
/// When an invoice falls due.
type DueAt = Instant {}

/// The day an invoice is dated.
type InvoiceDate = CalendarDate {}

test "time literals are checked while compiling and compare as time" {
    assert DueAt("2026-10-25T13:00:00+01:00") == DueAt("2026-10-25T12:00:00Z")
    assert InvoiceDate("2026-10-25") < InvoiceDate("2026-10-26")
}
```

## Durations

Decided (question 4): one spelling everywhere in source, `500ms`, `30s`,
`5m`, `2h`, `30d`, with ISO 8601 (`"PT5M"`) only on the wire.

```jadpo check project=durations
/// How long a request may take.
type RequestTimeout = Duration {
    max: 30s
}

/// How long a reminder comes before its due time.
type ReminderNotice = Duration {
    min: 1m
    max: 30d
}

test "durations are written with a unit, and add up" {
    var allowed: RequestTimeout = 5s
    assert allowed + 500ms == 5500ms
    var notice: ReminderNotice = 2h
    assert notice - 30m == 90m
}
```

`30d` means exactly 30 × 24 hours, which is what a retention period or a
token lifetime means. Moving a date by calendar days is a separate operation
on `CalendarDate` (below).

On the wire a duration is ISO 8601 in hours, minutes and seconds, as the
runtime writes it: `"PT5M"`, `"PT1.5S"`, `"PT720H"` for `30d`, `"-PT5M"`
for a negative one. A request's JSON, query string, path and headers give
it the same way, with days (`"P1DT2H"`) accepted as 24 hours each; `5m` is
the source's spelling and is refused there (chosen in chunk 5.2). A setting
is the exception, read as source writes it, `5m` (chunk 5.1).

## The current time: `clock.now`

`clock.now` is the time an operation started. It is fixed for the whole
operation, nested actions included, so every timestamp one request writes
agrees. A later request, schedule run or retry gets a new value. There is no
way to read a fresher time, and code never reads the system clock.

Only actions read the clock. Functions and queries must give the same answer
for the same arguments, so they take the time as a parameter
(`type.clock_context`); a query's condition may compare with `clock.now`,
which is the calling action's (area 6):

```jadpo check project=clock-now
type ReminderCutoff = Instant {}

/// The latest due time a reminder goes out for now: an hour ahead.
action reminder_cutoff() -> ReminderCutoff {
    return ReminderCutoff(clock.now + 1h)
}

test "an action reads the test's clock" {
    clock: "2026-01-15T12:00:00Z"

    assert reminder_cutoff() == ReminderCutoff("2026-01-15T13:00:00Z")
    clock.advance(30m)
    assert reminder_cutoff() == ReminderCutoff("2026-01-15T13:30:00Z")
}
```

Timeouts are measured on a separate clock that wall-clock changes cannot
move; code never sees it. Tests fix the clock with `clock:` and move it with
`clock.advance(...)` (area 13). The generated SQL always passes the time as
a value; the database's own `now()` is never used.

## Arithmetic and comparison

`Instant + Duration`, `Duration + Instant` and `Instant - Duration` give an
`Instant` of the instant's own named type, so no rebuild is needed
(question 6). `Instant - Instant` gives the `Duration` between them, which is
negative when the second is later; `Duration ± Duration` gives a
`Duration`. As with numbers, both sides are the same type after widening, or
one is a literal, and the result of two of the same is a plain `Duration`.
Instants, dates and durations order with `<`, `<=`, `>` and `>=` and compare
with `==` on the timeline, so `12:00Z` equals `13:00+01:00`.

A `CalendarDate` has no clock time, so a `Duration` doesn't move it; it moves
by calendar days with `temporal.add_days`. A `ZonedTime` moves on its
calendar the same way, or through its instant. `*`, `/` and `%` don't take
time values (chosen in chunk 5.2).

```jadpo check project=arithmetic
type DueAt = Instant {}

type ReminderNotice = Duration {
    min: 1m
    max: 7d
}

type Lateness = Duration {}

/// When to send a reminder: the notice before the due time. Taking a
/// duration from a `DueAt` gives a `DueAt`.
function reminder_time(due_at: DueAt, notice: ReminderNotice) -> DueAt {
    return due_at - notice
}

/// How late something done at `done_at` was: negative when it was early.
function lateness(due_at: DueAt, done_at: DueAt) -> Lateness {
    return Lateness(done_at - due_at)
}

test "a duration moves an instant and keeps its type" {
    assert reminder_time("2026-10-25T09:00:00Z", 30m) == "2026-10-25T08:30:00Z"
    assert lateness("2026-10-25T09:00:00Z", "2026-10-25T11:15:00Z") == 135m
    assert lateness("2026-10-25T09:00:00Z", "2026-10-25T08:00:00Z") < 0s
}

test "instants compare on the timeline" {
    assert DueAt("2026-10-25T12:00:00Z") == DueAt("2026-10-25T13:00:00+01:00")
    assert DueAt("2026-10-25T12:00:00Z") < DueAt("2026-10-25T12:00:00.001Z")
}
```

## Zones and local times

A zone turns an instant into local time; this is never ambiguous:
`temporal.in_zone(clock.now, Zone.europe_london)` is a `ZonedTime`.

Going the other way, from a date and a clock time to an instant, can be: at
01:30 on the day clocks go back, London has two 01:30s, and on the day they
go forward 01:30 does not exist. One option, `dst:`, says which instant to
take (question 7): `Dst.earlier` or `Dst.later` takes that instant of an
overlap, and in a gap moves the clock time back or forward by the gap's
length; `Dst.compatible` is earlier in an overlap and later in a gap, as
JavaScript's Temporal does. Left out, or `Dst.reject`, an unclear local time
fails, and the call's `attempt` maps it by the option's name, `dst:`, the
way a construction's broken rule is mapped (area 2): the failure is
`invalid_value`, and `invalid:` covers every condition without its own arm.

```jadpo check project=local-times
type ShopDay = CalendarDate {}

type Opening = ZonedTime {}

type OpeningUnclear = Failure {
    kind: FailureKind.invalid_value
}

/// When the night shift opens on `day`: 01:30, which on the day the clocks
/// go back happens twice, and on the day they go forward not at all.
function night_opening(day: ShopDay) fails OpeningUnclear -> Opening {
    var opening: ZonedTime = attempt temporal.resolve(day, at: "01:30", zone: Zone.europe_london) {
        dst: OpeningUnclear
    }
    return Opening(opening)
}

/// The same, taking the earlier 01:30 of the two and, when there is none,
/// the time an hour on.
function night_opening_anyway(day: ShopDay) -> Opening {
    return Opening(
        temporal.resolve(day, at: "01:30", zone: Zone.europe_london, dst: Dst.compatible)
    )
}

test "an unclear local time fails unless `dst:` decides" {
    assert night_opening("2026-10-25") fails OpeningUnclear
    assert night_opening("2026-03-29") fails OpeningUnclear
    assert temporal.instant(night_opening_anyway("2026-10-25")) == Instant("2026-10-25T00:30:00Z")
    assert temporal.instant(night_opening_anyway("2026-03-29")) == Instant("2026-03-29T01:30:00Z")
}

test "a day on keeps the clock time, 25 hours later across the change" {
    var saturday: ZonedTime = temporal.resolve(
        "2026-10-24",
        at: "09:00",
        zone: Zone.europe_london,
        dst: Dst.compatible
    )
    var sunday: ZonedTime = temporal.add_days(saturday, 1, dst: Dst.compatible)
    assert temporal.instant(sunday) - temporal.instant(saturday) == 25h
    assert temporal.hour(sunday) == 9
    assert temporal.weekday(sunday) == Weekday.sunday
}

test "a month on from the 31st needs a day the month has" {
    var end_of_january: CalendarDate = "2026-01-31"
    assert temporal.add_months(
        end_of_january,
        1,
        invalid_day: InvalidDay.last_valid_day
    ) == CalendarDate("2026-02-28")
}
```

`at:` is a clock time written out, `"09:30"` or `"09:30:15"`, checked while
compiling (`type.temporal_clock_invalid`): there is no time-of-day type, so
no value holds one. Only a `dst:` written out settles the condition: a `Dst`
passed as a value might be `reject`, so the call can still fail. Moving a
date by months or years can land on a day the month doesn't have (31
January plus a month): `invalid_day: InvalidDay.last_valid_day` takes the
month's last day, and left out, or `InvalidDay.reject`, the call fails with
`invalid_day:` to map. `dst:` means something only for a `ZonedTime`; on a
`CalendarDate` it is `type.temporal_option_unused` (chosen in chunk 5.2).

A query for "deliveries on 25 October in London" uses the day's bounds, from
its first instant up to the next day's, which are correct whether the day has
23, 24 or 25 hours; a query compares a field with an `InstantRange`
parameter's `start` and `end` as with any parameter's fields (area 6):

```jadpo check project=deliveries file=app.jadpo hidden
application {}
```

```jadpo check project=deliveries file=domains/shop/types/delivery_note.jadpo hidden
type DeliveryNote = Text {
    max_length: 200
}
```

```jadpo check project=deliveries file=domains/shop/types/shop_day.jadpo hidden
type ShopDay = CalendarDate {}
```

```jadpo check project=deliveries file=domains/shop/entities/delivery.jadpo hidden
use shop/types { DeliveryNote }

entity Delivery {
    id: Uuid
    note: DeliveryNote
    arrived_at: Instant {
        generated: Generated.on_create
    }
}
```

```jadpo check project=deliveries file=domains/shop/types/delivery_view.jadpo hidden
use shop/entities { Delivery }

type DeliveryView = Output {
    pick: [Delivery.note, Delivery.arrived_at]
}
```

```jadpo check project=deliveries file=domains/shop/actions/record_delivery.jadpo hidden
use shop/entities { Delivery }

/// Records a delivery as it arrives: `arrived_at` is the action's
/// `clock.now`.
action record_delivery(note: Delivery.note) -> Delivery {
    return Delivery.create {
        set: {
            note: note
        }
    }
}
```

```jadpo check project=deliveries file=domains/shop/queries/deliveries_in.jadpo
use shop/entities { Delivery }
use shop/types { DeliveryView }

/// The deliveries that arrived from `range.start` up to `range.end`.
query deliveries_in(range: InstantRange) -> List<DeliveryView> {
    from: Delivery
    where: Delivery.arrived_at >= range.start
    where: Delivery.arrived_at < range.end
    order: [Delivery.arrived_at asc]
    limit: 100
}
```

```jadpo check project=deliveries file=domains/shop/actions/deliveries_on.jadpo migrate=initial
use shop/types { DeliveryView, ShopDay }
use shop/queries { deliveries_in }

/// The deliveries on `day` in London, however many hours it has.
action deliveries_on(day: ShopDay) -> List<DeliveryView> {
    return deliveries_in(temporal.day_bounds(day, Zone.europe_london))
}
```

The test runs against PostgreSQL. London's clocks go back at 02:00 on 25
October 2026, so the day runs from 23:00 UTC on the 24th to midnight UTC on
the 26th:

```jadpo check project=deliveries file=domains/shop/actions/deliveries_on.test.jadpo
use shop/actions { record_delivery }

test "a London day has 25 hours when the clocks go back" {
    clock: "2026-10-24T22:59:00Z"

    assert record_delivery("before midnight").note == "before midnight"
    clock.advance(2m)
    assert record_delivery("just after midnight").note == "just after midnight"
    clock.advance(24h)
    assert record_delivery("late on the 25th").note == "late on the 25th"
    clock.advance(1h)
    assert record_delivery("the 26th").note == "the 26th"
    assert collection.map(deliveries_on("2026-10-25"), delivery -> delivery.note) == [
        "just after midnight",
        "late on the 25th"
    ]
}
```

The `temporal` library (question 7, Option B in the
[time and formatting proposals](research/10-configuration-and-time-proposals.md)),
always called through its namespace. "A date or local time" is a value built
on `CalendarDate` or `ZonedTime`, and a result of "its type" keeps the
argument's named type:

| Operation | Gives | Fails on, unless an option settles it |
|---|---|---|
| `in_zone(instant, zone)` | `ZonedTime` | |
| `resolve(date, at:, zone:, dst:)` | `ZonedTime` | `dst` |
| `between(start, end)` | `Duration`, `end - start` | |
| `add_days(value, days, dst:)` | its type, a whole number of calendar days on, the clock time kept | `dst`, for a `ZonedTime` |
| `add_months(value, months, invalid_day:, dst:)` | its type, months on, the day and clock time kept | `invalid_day`, and `dst` for a `ZonedTime` |
| `add_years(value, years, invalid_day:, dst:)` | its type, twelve months a year | the same |
| `day_bounds(date, zone)` | `InstantRange`: `start`, and `end` not included | |
| `week_bounds(date, zone, starts_on:)` | the week holding `date`, from `starts_on` (a `Weekday`) | |
| `month_bounds(date, zone)`, `year_bounds(date, zone)` | the month or year holding `date` | |
| `year(value)`, `month(value)`, `day(value)` | `Int`, of a date or a local time's date; `month` and `day` from 1 | |
| `weekday(value)` | `Weekday` | |
| `date(zoned)`, `hour(zoned)`, `minute(zoned)` | a local time's `CalendarDate`, and `Int`s from 0 | |
| `zone(zoned)`, `instant(zoned)` | the `Zone` it is seen in, and the `Instant` it is | |
| `format(value, zone:, locale:, format:)`, `format_relative(value, now:, zone:, locale:)` | `PresentationText`, below | |

A day's bounds start at its first instant: midnight, or, in a zone whose
clocks skip midnight, the first time they show that day. `instant(...)` is
the one operation the proposal didn't list: a `ZonedTime` has no order of
its own, and its instant does (chosen in chunk 5.2).

Zones come from the IANA time zone database bundled into every build
(jiff's `jiff-tzdb`, release 2026e), never the platform's own files, so the
interpreter, a native deployable and WebAssembly on Workers answer alike,
and every `Zone` variant is in it, which a test holds. A newer release is a
dependency change of its own (chosen in chunk 5.2).

## Showing dates and times to people

> Built in chunk 5.2b: the formats, `PresentationText` and the checks on
> both calls. Built in chunk 5.3: the text they give, in the words, order,
> punctuation and digits of each locale, and the locales an application
> supports.

Formatting needs three things the server must never guess: the zone, the
language and the format. Decided (question 7): one call that takes all
three, with the format either built in or declared once as a named type.
The language is one the application supports, declared in `app.jadpo`
("Locales", below), so the example is a project:

```jadpo check project=format-call file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=format-call file=domains/shop/types/ordered_at.jadpo
/// When an order was placed.
type OrderedAt = Instant {}
```

```jadpo check project=format-call file=domains/shop/types/short_date.jadpo
/// A date as "25 Oct 2026" in `Locale.en_gb`.
type ShortDate = TimeFormat {
    day: ComponentStyle.numeric
    month: ComponentStyle.short
    year: ComponentStyle.numeric
}
```

```jadpo check project=format-call file=domains/shop/types/order_placed.jadpo
/// When an order was placed, shown to its customer.
type OrderPlaced = Output {
    placed: PresentationText
}
```

```jadpo check project=format-call file=domains/shop/actions/show_placed.jadpo
use shop/types { OrderPlaced, OrderedAt, ShortDate }

/// The order's placing, in the customer's zone and language.
action show_placed(ordered_at: OrderedAt, zone: Zone, locale: Locale) -> OrderPlaced {
    return OrderPlaced {
        placed: temporal.format(ordered_at, zone: zone, locale: locale, format: ShortDate)
    }
}
```

The zone decides the date, and the locale the words and their order: at
23:30 UTC on 25 October 2026 it is already the 26th in Paris.

```jadpo check project=format-call file=domains/shop/actions/show_placed.test.jadpo
test "the placing is shown in the customer's zone and language" {
    assert show_placed(
        "2026-10-25T09:30:00Z",
        Zone.europe_london,
        Locale.en_gb
    ).placed == "25 Oct 2026"
    assert show_placed(
        "2026-10-25T09:30:00Z",
        Zone.europe_paris,
        Locale.fr_fr
    ).placed == "25 oct. 2026"
    assert show_placed(
        "2026-10-25T23:30:00Z",
        Zone.europe_paris,
        Locale.fr_fr
    ).placed == "26 oct. 2026"
}
```

A built-in format is written `format: TimeFormat.date_time_long`. The
built-in formats are `date_`, `time_` and `date_time_` each with `full`,
`long`, `medium` or `short`, CLDR's four lengths: the locale's own pattern
for a date, a time, or both with the words it joins them with. In `en_gb`,
for 14:30:05 on Sunday 27 September 2026 in London, where the clocks are on
British Summer Time:

| Format | `en_gb` |
|---|---|
| `date_full` | Sunday, 27 September 2026 |
| `date_long` | 27 September 2026 |
| `date_medium` | 27 Sept 2026 |
| `date_short` | 27/09/2026 |
| `time_full` | 14:30:05 British Summer Time |
| `time_long` | 14:30:05 BST |
| `time_medium` | 14:30:05 |
| `time_short` | 14:30 |
| `date_time_full` | Sunday, 27 September 2026 at 14:30:05 British Summer Time |
| `date_time_long` | 27 September 2026 at 14:30:05 BST |
| `date_time_medium` | 27 Sept 2026, 14:30:05 |
| `date_time_short` | 27/09/2026, 14:30 |

This page checks the table; the project that does is hidden.

```jadpo check project=format-builtins file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
```

```jadpo check project=format-builtins file=tests/built_in_formats.test.jadpo hidden
test "the twelve built-in formats in en_gb" {
    var seen: ZonedTime = temporal.in_zone("2026-09-27T13:30:05Z", Zone.europe_london)
    assert temporal.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.date_full
    ) == "Sunday, 27 September 2026"
    assert temporal.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.date_long
    ) == "27 September 2026"
    assert temporal.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.date_medium
    ) == "27 Sept 2026"
    assert temporal.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.date_short
    ) == "27/09/2026"
    assert temporal.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.time_full
    ) == "14:30:05 British Summer Time"
    assert temporal.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.time_long
    ) == "14:30:05 BST"
    assert temporal.format(seen, locale: Locale.en_gb, format: TimeFormat.time_medium) == "14:30:05"
    assert temporal.format(seen, locale: Locale.en_gb, format: TimeFormat.time_short) == "14:30"
    assert temporal.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.date_time_full
    ) == "Sunday, 27 September 2026 at 14:30:05 British Summer Time"
    assert temporal.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.date_time_long
    ) == "27 September 2026 at 14:30:05 BST"
    assert temporal.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.date_time_medium
    ) == "27 Sept 2026, 14:30:05"
    assert temporal.format(
        seen,
        locale: Locale.en_gb,
        format: TimeFormat.date_time_short
    ) == "27/09/2026, 14:30"
}
```

A declared format lists the parts it shows, each once, with a
`ComponentStyle`, and the locale decides their order and punctuation and
whether the clock has 12 or 24 hours, as CLDR's skeletons and JavaScript's
`Intl.DateTimeFormat` do:

| Part | Takes |
|---|---|
| `weekday` | `long`, `short`, `narrow` |
| `day`, `year` | `numeric`, `two_digit` |
| `month` | `numeric`, `two_digit`, `long`, `short`, `narrow` |
| `hour`, `minute`, `second` | `numeric`, `two_digit` |
| `zone_name` | `long` ("British Summer Time"), `short` ("BST") |

A format with no parts is `type.temporal_format_components_empty`, a part
twice `type.temporal_format_component_duplicate`, a style the part doesn't
take `type.temporal_format_component_invalid`, and a name that is no part
`type.unknown_setting`. A format holds no value: `format:` names it
written out, and it is never the type of a field, a parameter or a
variable, nor a value passed in (`type.temporal_format_invalid`). The
operation is given it as text, `day:numeric month:short year:numeric` or
`date_time_long`.

`zone:` is given exactly when the value is an `Instant`
(`type.temporal_zone_required`): a `ZonedTime` is shown in its own zone and
a `CalendarDate` has none, so `zone:` with either is
`type.temporal_option_unused`. A `CalendarDate` has no clock time, so a
`time_` or `date_time_` format, or one showing `hour`, `minute`, `second`
or `zone_name`, is `type.temporal_format_requires_time`. `locale:` is a
`Locale` the application supports (`type.temporal_locales_required`,
`type.locale_unsupported`; "Locales", below).

"3 minutes ago" style text uses
`temporal.format_relative(value, now: clock.now, zone:, locale:)`, for an
`Instant` or a `ZonedTime`, with `zone:` as for `format`. Its rules are
fixed, so two readers of the same value see the same words:

| `value` against `now` | Text, in `en_gb` |
|---|---|
| under a minute, either way | "just now" |
| under an hour | "4 minutes ago", "in 10 minutes" |
| the same date | "Today at 13:00" |
| the date before, or after | "Yesterday at 14:30", "Tomorrow at 14:30" |
| two to six days away | "Friday at 07:30" |
| anything else | "20 September 2026" |

```jadpo check project=format-relative file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```jadpo check project=format-relative file=domains/chat/types/sent_at.jadpo
/// When a message was sent.
type SentAt = Instant {}
```

```jadpo check project=format-relative file=domains/chat/types/sent_text.jadpo
/// When a message was sent, as its reader reads it.
type SentText = Output {
    seen: PresentationText
}
```

```jadpo check project=format-relative file=domains/chat/actions/show_sent.jadpo
use chat/types { SentAt, SentText }

/// When a message was sent, against now in London.
action show_sent(sent_at: SentAt, locale: Locale) -> SentText {
    return SentText {
        seen: temporal.format_relative(
            sent_at,
            now: clock.now,
            zone: Zone.europe_london,
            locale: locale
        )
    }
}
```

```jadpo check project=format-relative file=domains/chat/actions/show_sent.test.jadpo
test "a time is shown against now" {
    clock: "2026-09-27T13:30:00Z"

    assert show_sent("2026-09-27T13:29:40Z", Locale.en_gb).seen == "just now"
    assert show_sent("2026-09-27T13:26:00Z", Locale.en_gb).seen == "4 minutes ago"
    assert show_sent("2026-09-27T12:00:00Z", Locale.en_gb).seen == "Today at 13:00"
    assert show_sent("2026-09-26T13:30:00Z", Locale.en_gb).seen == "Yesterday at 14:30"
    assert show_sent("2026-09-20T13:30:00Z", Locale.en_gb).seen == "20 September 2026"
    assert show_sent("2026-09-27T13:26:00Z", Locale.fr_fr).seen == "il y a 4 minutes"
    assert show_sent("2026-09-26T13:30:00Z", Locale.fr_fr).seen == "Hier à 14:30"
}
```

The result is `PresentationText`, a built-in type: it can go into a
response, an email or a page, a signature may use it directly, and text
written in quotes may stand for one, which is how a test compares formatted
text with what it expects. It cannot be stored: an entity field holding it,
directly or through a named type, a list, an object or a variant's data, is
`type.presentation_text_persistence`, and it is not a setting. It cannot be
turned back into a time, ordered, added to or rebuilt into `Text`; two
compare with `==` and `!=`. On the wire it is a JSON string. Clients that
format for themselves receive the `Instant`.

### Chosen in chunk 5.3

- **Where the words come from.** Unicode CLDR 48, through ICU4X (the
  `icu_datetime`, `icu_calendar`, `icu_time`, `icu_locale` and
  `icu_experimental` crates, pinned together), compiled into the binary:
  the interpreter, a native deployable and WebAssembly on Workers write the
  same text, and nothing is read from the platform. `Locale` was generated
  from the same release, so every variant has data.
- **The calendar and the digits.** Always the Gregorian calendar, as
  `calendar: "gregory"` gives in `Intl.DateTimeFormat` (a Thai or Arabic
  reader of a `CalendarDate` sees the same year the database holds), and
  always the digits the locale writes (`ar_eg`, `fa`, `bn`, `mr` and `ne`
  write their own). CLDR's narrow no-break space before `PM` is written as
  a plain space, as browsers and Node write it, so a test is written with
  the space it shows.
- **A declared format.** The locale's pattern for the parts is taken, then
  each part is set to its style. A month written as a name is taken from
  the long date pattern (the medium one where that writes the month as a
  name too), a number from the short one, and `month: short` is that
  pattern with the abbreviated name; `numeric` leaves a number as wide as
  the locale writes it (`dd/MM/y` stays) and `two_digit` makes it at least
  two; a date and a time are joined with the words the locale joins them
  with for that month ("at" for `long`, a comma for `short`). So the output
  is the locale's own and matches `Intl.DateTimeFormat` for most locales
  and most options, but not all of them: where `Intl` shortens `01:59` to
  `1:59` for `hour: numeric minute: two_digit`, this writes the locale's
  `01:59`, and a Japanese or Korean weekday beside a long date has no
  brackets.
- **Parts no pattern holds together.** A year with a day (and no month), a
  weekday and a year, a minute without an hour, or a month with a time are
  written one after another with a space: `27 2026`, `30:05`,
  `September, 14:30`. Only `zone_name` alone is the name alone ("British
  Summer Time"), where `Intl` adds the date.
- **The built-in formats.** The locale's patterns at CLDR's four lengths;
  `date_full` is the long date with the weekday, so `date_time_long` is
  "27 September 2026 at 14:30:05 BST", with the seconds `timeStyle: "long"`
  has. For a handful of locales (Japanese and Korean `time_` formats, the
  punctuation of a few `date_` formats) the pattern is ICU4X's, which is
  not always the one `Intl.DateTimeFormat` writes.
- **The zone's name.** From the zone and the instant: "BST" or "British
  Summer Time" in summer, "GMT" or "Greenwich Mean Time" in winter, and
  where CLDR has no name for the locale, its offset ("GMT+1", "UTC+1").
- **`format_relative`'s words.** The locale's own for "now", "4 minutes
  ago", "yesterday" and "tomorrow"; "just now" is English's, and every
  other locale says what CLDR says for "now". Under a minute counts
  either way; under an hour counts whole minutes, a past value down and a
  future one up, so a value is never said to be further away than it is.
  Dates are the dates on the clock in the zone the value is shown in, not
  blocks of 24 hours, so a day with 25 hours is still one day. "Tomorrow"
  is the mirror of "Yesterday". The first letter of "Today at", "Yesterday
  at", "Tomorrow at" and a weekday is a capital where the script has them.
  A locale with no word for yesterday or tomorrow shows the long date.
  "Two to six days away" is a weekday and the time, in the past or the
  future alike; a week or more is the long date, with no time.
- **Not here.** Translations (below) are chunk 5.3b, and choosing a locale
  from an `Accept-Language` header is chunk 5.3c.

## Locales

`Locale` is a built-in enum of the standard locales (`Locale.en_gb`,
`Locale.fr_fr`): every locale Unicode CLDR 48 covers fully, and for each
language without a region the region it most likely means. An application
lists the ones it supports, once, in `app.jadpo`; formatting and
translations accept only those, and using any other is a compile error. The
server's own locale is never used.

```jadpo check project=locales file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_us, Locale.fr_fr]
}
```

`locales { }` is a declaration with no name, like `config { }`, with
exactly two options, each once: `default:` and `supported:`, a list. Each
entry is written `Locale.<tag>`, the tag in lower case with `_` for `-`
(`Locale.zh_hans` is zh-Hans). A locale written out where text is formatted,
or as the `default:` or `test:` of a `Locale` setting, that the list doesn't
have is `type.locale_unsupported`; a project that formats text and declares
none is `type.temporal_locales_required`.

There is no `unsupported:` setting. A request field typed `Locale` is
decoded like any enum, so an unsupported tag is an `invalid_value` failure
and the OpenAPI document lists only the supported tags; a setting typed
`Locale` is read the same way when the app starts. Choosing a language from
an `Accept-Language` header is a library call that falls back to `default`
(chunk 5.3c).

## Translations

Failure messages are translated by the client using the failure's `code`
(decided in area 4); `message` stays fixed English. Text the server writes
for people, such as emails and server-rendered pages, needs translating on
the server, and area 1 reserved the root `translations/` folder for it.
It comes in chunk 5.3b.

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

> The errors below run with this checkout's `jadpo`: the configuration errors (chunk 5.1), `type.clock_context` and the time errors after it (chunk 5.2), the formatting errors (chunk 5.2b) and the locale errors at the end (chunk 5.3).

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

### `configuration.client_address_duplicate`

`client_address:` is declared once, in `app.jadpo`; a second one:

```jadpo check project=e-client-address-duplicate expect=configuration.client_address_duplicate file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.connection
    }
    client_address: {
        source: ClientAddressSource.cloudflare
    }
}
```

```text output
error: `client_address:` is declared twice
 --> app.jadpo:5:5
  |
2 |     client_address: {
  |     -------------- first declared here
5 |     client_address: {
  |     ^^^^^^^^^^^^^^ declared again
  reason: The client's address is read one way for the whole application, so every request is counted by the same rule.
  next: Keep one `client_address:` and remove the other (agent)
  rule: configuration.client_address_duplicate
  help: docs/language/10-configuration-and-time.html#configuration-client-address-duplicate

check failed: 1 error, 1 file
```

### `configuration.client_address_invalid`

`client_address:` is a block of `source:` and `hops:` and nothing else:

```jadpo check project=e-client-address-invalid expect=configuration.client_address_invalid file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hop: 1
    }
}
```

```text output
error: `hop` isn't an option of `client_address:`
 --> app.jadpo:4:9
  |
4 |         hop: 1
  |         ^^^ unknown option
  reason: `client_address:` takes `source:` and `hops:`, and nothing else; which proxies to trust is a count and not a list. Did you mean `hops`?
  next: Use `hops` instead of `hop` (agent)
  rule: configuration.client_address_invalid
  help: docs/language/10-configuration-and-time.html#configuration-client-address-invalid

check failed: 1 error, 1 file
```

### `configuration.client_address_source_unknown`

`source:` is one of the four `ClientAddressSource` values:

```jadpo check project=e-client-address-source-unknown expect=configuration.client_address_source_unknown file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forward_for
        hops: 1
    }
}
```

```text output
error: `ClientAddressSource.forward_for` isn't a source of the client's address
 --> app.jadpo:3:17
  |
3 |         source: ClientAddressSource.forward_for
  |                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not a `ClientAddressSource`
  reason: `ClientAddressSource` has no value called `forward_for`; the four are `connection`, `forwarded_for`, `forwarded`, `cloudflare`. Did you mean `forwarded_for`?
  next: Use `ClientAddressSource.forwarded_for` instead (agent)
  rule: configuration.client_address_source_unknown
  help: docs/language/10-configuration-and-time.html#configuration-client-address-source-unknown

check failed: 1 error, 1 file
```

### `configuration.client_address_hops_invalid`

`hops:` counts proxies from 1, a whole number or an `Int` setting that declares `min:` of 1 or more:

```jadpo check project=e-client-address-hops-invalid expect=configuration.client_address_hops_invalid file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.forwarded_for
        hops: 0
    }
}
```

```text output
error: `0` can't be a number of proxies
 --> app.jadpo:4:15
  |
4 |         hops: 0
  |               ^ below 1
  reason: `hops:` is how many proxies of the deployment's own sit in front of the app: a whole number from 1, or an `Int` setting whose type declares `min:` of at least 1.
  next: Write a whole number from 1, such as `hops: 1`, or read an `Int` setting that declares `min: 1` (agent)
  rule: configuration.client_address_hops_invalid
  help: docs/language/10-configuration-and-time.html#configuration-client-address-hops-invalid

check failed: 1 error, 1 file
```

### `configuration.client_address_hops_unused`

`cloudflare` and `connection` count no proxies, so `hops:` means nothing with them:

```jadpo check project=e-client-address-hops-unused expect=configuration.client_address_hops_unused file=app.jadpo
application {
    client_address: {
        source: ClientAddressSource.cloudflare
        hops: 1
    }
}
```

```text output
error: `hops:` means nothing with `cloudflare`
 --> app.jadpo:4:15
  |
4 |         hops: 1
  |               ^ no proxies are counted
  reason: Cloudflare sets `CF-Connecting-IP` on every request from its edge and overwrites a caller's, so there is no list to count from the right.
  next: Remove `hops:` (agent)
  rule: configuration.client_address_hops_unused
  help: docs/language/10-configuration-and-time.html#configuration-client-address-hops-unused

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

```jadpo check project=e-clock-invalid expect=type.temporal_clock_invalid
type ShopDay = CalendarDate {}

type Opening = ZonedTime {}

function opening(day: ShopDay) -> Opening {
    return Opening(temporal.resolve(day, at: "9:30", zone: Zone.utc, dst: Dst.later))
}
```

```text output
error: `"9:30"` isn't a clock time
 --> main.jadpo:6:46
  |
6 |     return Opening(temporal.resolve(day, at: "9:30", zone: Zone.utc, dst: Dst.later))
  |                                              ^^^^^^ not `HH:MM`
  reason: `temporal.resolve`'s `at:` is a clock time written `HH:MM`, or `HH:MM:SS` with seconds, each part two digits, from `00:00` to `23:59`.
  next: Write the time with two-digit hours and minutes, such as `"09:30"` (agent)
  rule: type.temporal_clock_invalid
  help: docs/language/10-configuration-and-time.html#type-temporal-clock-invalid

check failed: 1 error, 1 file
```

### `type.temporal_option_unused`

```jadpo check project=e-option-unused expect=type.temporal_option_unused
type ShopDay = CalendarDate {}

function next_day(day: ShopDay) -> ShopDay {
    return temporal.add_days(day, 1, dst: Dst.compatible)
}
```

```text output
error: `dst:` means nothing for a `ShopDay`
 --> main.jadpo:4:38
  |
4 |     return temporal.add_days(day, 1, dst: Dst.compatible)
  |                                      ^^^ `ShopDay` has no clock time
  reason: `dst:` chooses an instant when a local time falls in a daylight-saving gap or overlap; `temporal.add_days` on a `ShopDay` moves a date, which has no clock time, so no such time arises.
  next: Remove `dst: ...` (agent)
  rule: type.temporal_option_unused
  help: docs/language/10-configuration-and-time.html#type-temporal-option-unused

check failed: 1 error, 1 file
```

`zone:` on a date or a local time is reported the same way: a
`ZonedTime` carries its zone, and a `CalendarDate` has none.

### `type.temporal_zone_required`

```jadpo check project=e-zone-required expect=type.temporal_zone_required
type OrderedAt = Instant {}

function placed_label(ordered_at: OrderedAt, locale: Locale) -> PresentationText {
    return temporal.format(ordered_at, locale: locale, format: TimeFormat.date_long)
}
```

```text output
error: `temporal.format` needs the zone to show `OrderedAt` in
 --> main.jadpo:4:12
  |
4 |     return temporal.format(ordered_at, locale: locale, format: TimeFormat.date_long)
  |            ^^^^^^^^^^^^^^^ no `zone:`
  reason: An `Instant` is one moment for everyone, and its date and clock time depend on where it is read; the server never guesses a zone, so `zone:` says which.
  next: Add `zone: ...` with the reader's zone, such as `zone: Zone.europe_london` (agent)
  rule: type.temporal_zone_required
  help: docs/language/10-configuration-and-time.html#type-temporal-zone-required

check failed: 1 error, 1 file
```

### `type.temporal_format_invalid`

```jadpo check project=e-format-invalid expect=type.temporal_format_invalid
type OrderedAt = Instant {}

function placed_label(ordered_at: OrderedAt, zone: Zone, locale: Locale) -> PresentationText {
    return temporal.format(ordered_at, zone: zone, locale: locale, format: TimeFormat.date_longest)
}
```

```text output
error: `TimeFormat.date_longest` isn't a time format
 --> main.jadpo:4:76
  |
4 |     return temporal.format(ordered_at, zone: zone, locale: locale, format: TimeFormat.date_longest)
  |                                                                            ^^^^^^^^^^^^^^^^^^^^^^^ not a format
  reason: `temporal.format`'s `format:` is written out: a built-in format, `TimeFormat.` followed by `date_`, `time_` or `date_time_` and `full`, `long`, `medium` or `short`, or the name of a format declared as `type Name = TimeFormat { ... }`.
  next: Write a built-in format such as `TimeFormat.date_long`, or declare one with `type Name = TimeFormat { ... }` (agent)
  rule: type.temporal_format_invalid
  help: docs/language/10-configuration-and-time.html#type-temporal-format-invalid

check failed: 1 error, 1 file
```

A format named as the type of a field, a parameter or a variable, or used
as a value, is reported the same way: it holds no value.

### `type.temporal_format_requires_time`

```jadpo check project=e-requires-time expect=type.temporal_format_requires_time
type InvoiceDate = CalendarDate {}

function invoice_dated(day: InvoiceDate, locale: Locale) -> PresentationText {
    return temporal.format(day, locale: locale, format: TimeFormat.date_time_long)
}
```

```text output
error: `TimeFormat.date_time_long` shows a clock time, which `InvoiceDate` doesn't have
 --> main.jadpo:4:57
  |
4 |     return temporal.format(day, locale: locale, format: TimeFormat.date_time_long)
  |                                                         ^^^^^^^^^^^^^^^^^^^^^^^^^ shows a clock time
  reason: `InvoiceDate` is a date, with no clock time or zone, so `temporal.format` can show only its weekday, day, month and year.
  next: Use a date format such as `TimeFormat.date_long`, or format an `Instant` or a `ZonedTime` instead (agent)
  rule: type.temporal_format_requires_time
  help: docs/language/10-configuration-and-time.html#type-temporal-format-requires-time

check failed: 1 error, 1 file
```

### `type.temporal_format_components_empty`

```jadpo check project=e-components-empty expect=type.temporal_format_components_empty
type Blank = TimeFormat {}
```

```text output
error: `Blank` shows nothing
 --> main.jadpo:1:6
  |
1 | type Blank = TimeFormat {}
  |      ^^^^^ no parts
  reason: A format lists the parts of a date or a time it shows, each with a `ComponentStyle`, and the locale arranges them; with none listed there is nothing to show.
  next: List the parts to show, such as `day: ComponentStyle.numeric`, or use a built-in format such as `TimeFormat.date_long` (agent)
  rule: type.temporal_format_components_empty
  help: docs/language/10-configuration-and-time.html#type-temporal-format-components-empty

check failed: 1 error, 1 file
```

### `type.temporal_format_component_invalid`

```jadpo check project=e-component-invalid expect=type.temporal_format_component_invalid
type ShortDate = TimeFormat {
    day: ComponentStyle.long
    month: ComponentStyle.short
}
```

```text output
error: `day` isn't shown `long`
 --> main.jadpo:2:10
  |
2 |     day: ComponentStyle.long
  |          ^^^^^^^^^^^^^^^^^^^ not a style `day` takes
  reason: A format's `day` is shown `ComponentStyle.numeric` or `ComponentStyle.two_digit`; the locale decides the rest, such as the order and the punctuation.
  next: Write `day: ComponentStyle.numeric` (agent)
  rule: type.temporal_format_component_invalid
  help: docs/language/10-configuration-and-time.html#type-temporal-format-component-invalid

check failed: 1 error, 1 file
```

A part that isn't one a format shows, such as `days:`, is
`type.unknown_setting`, with the nearest part's name.

### `type.temporal_format_component_duplicate`

```jadpo check project=e-component-duplicate expect=type.temporal_format_component_duplicate
type ShortDate = TimeFormat {
    day: ComponentStyle.numeric
    month: ComponentStyle.short
    day: ComponentStyle.two_digit
}
```

```text output
error: `day` is given twice
 --> main.jadpo:4:5
  |
2 |     day: ComponentStyle.numeric
  |     --- first given here
4 |     day: ComponentStyle.two_digit
  |     ^^^ given again
  reason: A format shows each part once, in one style, so there is never a question of which `day` counts.
  next: Remove one `day` (agent)
  rule: type.temporal_format_component_duplicate
  help: docs/language/10-configuration-and-time.html#type-temporal-format-component-duplicate

check failed: 1 error, 1 file
```

### `type.presentation_text_persistence`

```jadpo check project=e-presentation-persist expect=type.presentation_text_persistence
entity Receipt {
    id: Uuid
    issued_label: PresentationText
}
```

```text output
error: `Receipt.issued_label` would store text formatted for a person
 --> main.jadpo:3:19
  |
3 |     issued_label: PresentationText
  |                   ^^^^^^^^^^^^^^^^ `PresentationText`, stored
  reason: `Receipt.issued_label` holds `PresentationText`: text in one reader's language and zone, which another reader, a change of locale or a correction to the zone database would show differently, and which can't be read back as a time. A record keeps the value itself and formats it when it is shown.
  next: Store the `Instant`, `CalendarDate` or `ZonedTime` instead, and format `Receipt.issued_label` in the output that shows it (agent)
  rule: type.presentation_text_persistence
  help: docs/language/10-configuration-and-time.html#type-presentation-text-persistence

check failed: 1 error, 1 file
```

### `type.temporal_locales_required`

A project that formats text for people declares the locales it supports:

```jadpo check project=e-locales-required file=app.jadpo hidden
application {}
```

```jadpo check project=e-locales-required file=domains/shop/types/ordered_at.jadpo hidden
/// When an order was placed.
type OrderedAt = Instant {}
```

```jadpo check project=e-locales-required file=domains/shop/types/placed_text.jadpo hidden
/// An order's placing, shown to its customer.
type PlacedText = Output {
    placed: PresentationText
}
```

```jadpo check project=e-locales-required file=domains/shop/actions/show_placed.jadpo expect=type.temporal_locales_required
use shop/types { OrderedAt, PlacedText }

/// The placing, in the customer's language.
action show_placed(ordered_at: OrderedAt, locale: Locale) -> PlacedText {
    return PlacedText {
        placed: temporal.format(
            ordered_at,
            zone: Zone.utc,
            locale: locale,
            format: TimeFormat.date_long
        )
    }
}
```

```text output
error: `temporal.format` needs the application's locales
 --> domains/shop/actions/show_placed.jadpo:6:17
  |
6 |         placed: temporal.format(
  |                 ^^^^^^^^^^^^^^^ no locales declared
  reason: Text for a person is written in one of the locales the application supports. None is declared, so there is nothing to check `locale:` against, and nothing for a request with no usable `Accept-Language` to fall back to.
  next: Declare them in `app.jadpo`, for instance `locales { default: Locale.en_gb supported: [Locale.en_gb] }` (agent)
  rule: type.temporal_locales_required
  help: docs/language/10-configuration-and-time.html#type-temporal-locales-required

check failed: 1 error, 4 files
```

### `syntax.locales_default_required`

```jadpo check project=e-locales-default expect=syntax.locales_default_required
locales {
    supported: [Locale.en_gb]
}
```

```text output
error: `locales` has no `default:`
 --> main.jadpo:1:1
  |
1 | locales {
  | ^^^^^^^ no `default:`
  reason: The default is the locale a choice from an `Accept-Language` header falls back to, so every request ends up with a language the application supports.
  next: Add `default: Locale.en_gb` (agent)
  rule: syntax.locales_default_required
  help: docs/language/10-configuration-and-time.html#syntax-locales-default-required

check failed: 1 error, 1 file
```

### `syntax.locales_supported_required`

```jadpo check project=e-locales-supported expect=syntax.locales_supported_required
locales {
    default: Locale.en_gb
}
```

```text output
error: `locales` has no `supported:`
 --> main.jadpo:1:1
  |
1 | locales {
  | ^^^^^^^ no `supported:`
  reason: `supported:` lists every locale formatting and translations accept; a locale not listed is a compile error where it is written and an invalid value where it is read.
  next: Add `supported: [Locale.en_gb]`, listing every locale the application speaks (agent)
  rule: syntax.locales_supported_required
  help: docs/language/10-configuration-and-time.html#syntax-locales-supported-required

check failed: 1 error, 1 file
```

### `syntax.locales_unknown_option`

There is no `unsupported:` setting any more (see "Locales"). A misspelt
option (`suported:`) is reported the same way, with the nearest name:

```jadpo check project=e-locales-unknown expect=syntax.locales_unknown_option
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
    unsupported: Locale.fr_fr
}
```

```text output
error: `unsupported` isn't an option of `locales`
 --> main.jadpo:4:5
  |
4 |     unsupported: Locale.fr_fr
  |     ^^^^^^^^^^^ unknown option
  reason: There is no `unsupported:` setting. A request field typed `Locale` is decoded like any enum, so a tag the application doesn't support is an `invalid_value` failure, and choosing a language from an `Accept-Language` header falls back to `default:`.
  next: Remove `unsupported:` (agent)
  rule: syntax.locales_unknown_option
  help: docs/language/10-configuration-and-time.html#syntax-locales-unknown-option

check failed: 1 error, 1 file
```

### `syntax.locales_duplicate_option`

```jadpo check project=e-locales-twice expect=syntax.locales_duplicate_option
locales {
    default: Locale.en_gb
    default: Locale.fr_fr
    supported: [Locale.en_gb, Locale.fr_fr]
}
```

```text output
error: `default:` is given twice
 --> main.jadpo:3:5
  |
2 |     default: Locale.en_gb
  |              ------------ first given here
3 |     default: Locale.fr_fr
  |     ^^^^^^^ given again
  reason: `locales` says each of `default:` and `supported:` once.
  next: Keep one `default:` and remove the other (agent)
  rule: syntax.locales_duplicate_option
  help: docs/language/10-configuration-and-time.html#syntax-locales-duplicate-option

check failed: 1 error, 1 file
```

### `type.locale_invalid`

A locale is a name in `Locale`, not text; a tag in quotes is the common
slip, and the fix is one edit:

```jadpo check project=e-locale-invalid expect=type.locale_invalid
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, "fr-FR"]
}
```

```text output
error: `"fr-FR"` isn't written as a locale
 --> main.jadpo:3:31
  |
3 |     supported: [Locale.en_gb, "fr-FR"]
  |                               ^^^^^^^ not a `Locale`
  reason: A locale is a name in `Locale`, written `Locale.<tag>` with the tag in lower case and `_` for `-`: `Locale.en_gb` is en-GB. Closed choices are names, not text, so a misspelt one is caught while compiling.
  next: Write `Locale.fr_fr` instead (agent)
  rule: type.locale_invalid
  help: docs/language/10-configuration-and-time.html#type-locale-invalid

check failed: 1 error, 1 file
```

### `type.locale_duplicate`

```jadpo check project=e-locale-duplicate expect=type.locale_duplicate
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_gb]
}
```

```text output
error: `Locale.en_gb` is listed twice
 --> main.jadpo:3:31
  |
3 |     supported: [Locale.en_gb, Locale.en_gb]
  |                               ^^^^^^^^^^^^ listed again
  |                 ------------ first listed here
  reason: A locale is supported or it is not; listing it twice says nothing more.
  next: Remove the second `Locale.en_gb` (agent)
  rule: type.locale_duplicate
  help: docs/language/10-configuration-and-time.html#type-locale-duplicate

check failed: 1 error, 1 file
```

### `type.locale_default_unsupported`

```jadpo check project=e-locale-default expect=type.locale_default_unsupported
locales {
    default: Locale.fr_fr
    supported: [Locale.en_gb]
}
```

```text output
error: The default `Locale.fr_fr` isn't a supported locale
 --> main.jadpo:2:14
  |
2 |     default: Locale.fr_fr
  |              ^^^^^^^^^^^^ not in `supported:`
3 |     supported: [Locale.en_gb]
  |                -------------- the supported locales
  reason: The default is what a request with no usable `Accept-Language` is given, and formatting and translations accept only the supported locales, so the default has to be one.
  next: Add `Locale.fr_fr` to `supported:`, or choose a default that is listed (agent)
  rule: type.locale_default_unsupported
  help: docs/language/10-configuration-and-time.html#type-locale-default-unsupported

check failed: 1 error, 1 file
```

### `type.locale_unsupported`

```jadpo check project=e-locale-unsupported expect=type.locale_unsupported
type OrderedAt = Instant {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}

function placed_label(ordered_at: OrderedAt) -> PresentationText {
    return temporal.format(
        ordered_at,
        zone: Zone.utc,
        locale: Locale.fr_fr,
        format: TimeFormat.date_long
    )
}
```

```text output
error: `Locale.fr_fr` isn't a locale this application supports
 --> main.jadpo:12:17
   |
12 |         locale: Locale.fr_fr,
   |                 ^^^^^^^^^^^^ not in `locales`
  reason: `temporal.format` writes text in the locales listed in `locales { supported: }`, in `app.jadpo`; `Locale.fr_fr` is not one of them, and the server never guesses a language.
  next: Add `Locale.fr_fr` to `supported:` in `app.jadpo`, or use a locale that is listed (agent)
  rule: type.locale_unsupported
  help: docs/language/10-configuration-and-time.html#type-locale-unsupported

check failed: 1 error, 1 file
```

### `semantic.multiple_locale_declarations`

```jadpo check project=e-locales-multiple expect=semantic.multiple_locale_declarations
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}

locales {
    default: Locale.en_us
    supported: [Locale.en_us]
}
```

```text output
error: `locales { }` is declared twice
 --> main.jadpo:6:1
  |
1 | locales {
  | ------- first declared here
6 | locales {
  | ^^^^^^^ declared again
  reason: An application lists the locales it supports once, in `app.jadpo`, so formatting and translations all accept the same ones.
  next: Move what this `locales` lists into the first, and remove it (agent)
  rule: semantic.multiple_locale_declarations
  help: docs/language/10-configuration-and-time.html#semantic-multiple-locale-declarations

check failed: 1 error, 1 file
```

## Review

### Status

The new compiler has built the time literals and the built-in `Zone`,
`Weekday` and `Locale` enums (chunk 1.9), and reports `clock.now` outside an
action as `type.clock_context`. Settings, secrets, signing keys, connection
addresses, `jadpo config`, the check before the app starts and a test's own
values were built in chunk 5.1 (the readings it chose are in the Decisions
table), and the trusted proxies a client's address is read through in chunk
5.1c. Chunk 5.2 built time: `ZonedTime` in place of the time of day the
first build had, durations, arithmetic and comparison, the `temporal`
library over the IANA database bundled into every target, time values on
the wire, in OpenAPI and in queries, and a test's `clock:` and
`clock.advance(...)` (area 13); its readings are in the Decisions table.
Chunk 5.2b built `TimeFormat`, declared and built in, `ComponentStyle` and
`PresentationText`, and checks `temporal.format` and `format_relative`; its
readings are in the Decisions table too. Chunk 5.3 built locales
(`locales { }` with its errors, `locale:` checked against it, a `Locale` on
the wire or in a setting limited to the supported ones) and the text
`temporal.format` and `format_relative` give, over CLDR data compiled into
every target. Translations (`translations/`, ICU MessageFormat and typed
calls) are chunk 5.3b, and choosing a language from `Accept-Language`
chunk 5.3c.

### Consistency with the rest of the language

- **Names that repeat a name.** The config declaration's name and every
  `binding:` restate a name already present (syntax review #2, #55); area 1
  dropped singleton names, and area 4 derived failure codes the same way.
- **Two duration spellings** (`5m` in configuration, `Duration("PT5M")` in
  code), and each place rejects the other (syntax review #56).
- **Locales written as text** in their declaration but as `Locale.en_gb` in
  code (syntax review #57), against area 2's rule that closed options are
  names. Fixed by question 8: `locales { }` lists `Locale.en_gb`, and a
  tag in quotes is `type.locale_invalid` with the one-edit fix.
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
| Trusted proxies: which proxies a deployment trusts to say a client's address (`X-Forwarded-For`, `Forwarded`, Cloudflare's `CF-Connecting-IP`) | 7, 10, 16 | Built (chunk 5.1c, question 12); a source other than `cloudflare` refused for Workers comes with chunk 8.2 |
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
   Rust's `humantime` has it; no `w`, months or years, which vary. *Decided 2026-10-12:* yes, as recommended. *Built in chunk 5.2.*
5. **Rename `Time` to `ZonedTime`?** Java, Temporal and Elixir all use "time"
   for a time of day, so `Time` will be misread by people and agents.
   *Recommendation:* yes, `ZonedTime` (Temporal says `ZonedDateTime`, jiff
   `Zoned`), with the wire form unchanged. *Decided 2026-10-12:* yes, as recommended. *Built in chunk 5.2*, replacing the time of day the first build had called `Time`.
6. **Does adding a duration keep the instant's named type?** Today
   `due_at - notice` is a plain `Instant` and must be rebuilt.
   *Recommendation:* yes for `+` and `-` with a `Duration`, the one mixed
   case area 3's operator rule did not cover; `Instant - Instant` gives
   `Duration`. *Decided 2026-10-12:* yes, as recommended. *Built in chunk 5.2.*
7. **Should the time library and formatting be simplified?** About 35
   functions, six option enums, two daylight-saving options on every call, a
   two-step format and an anonymous components object.
   *Recommendation:* Option B in the
   [time and formatting proposals](research/10-configuration-and-time-proposals.md):
   `temporal.format(value, zone:, locale:, format:)` with named built-in or
   declared formats, `format_relative` without a profile, one `dst:` option
   that defaults to failing, and about 20 functions. *Decided 2026-10-12:* yes, as recommended. *Built in chunks 5.2, 5.2b and 5.3.*
8. **Locales as names, and drop `unsupported:`?** *Recommendation:* yes,
   `default: Locale.en_gb` and `supported: [Locale.en_gb, Locale.fr_fr]`, in
   `app.jadpo`, with `Locale` a built-in enum of the standard locales. An
   unsupported tag in a request is an ordinary `invalid_value` like any
   enum; header negotiation is a library call that falls back to `default`. *Decided 2026-10-12:* yes, as recommended. *Built in chunk 5.3* (the `Accept-Language` call is chunk 5.3c).
9. **What format does `translations/` use?** *Recommendation:* one JSON file
   per supported locale with ICU MessageFormat messages (what FormatJS, Java
   and translation services such as Crowdin and Lokalise use), typed calls
   generated from the default locale's keys, and missing keys or
   placeholders in other locales as compile errors. Build it when the first
   email template needs it (area 11), not before. *Decided 2026-10-12:* yes, as recommended.
   *Amended 2026-10-13 (Richard):* translations are supported out of the box
   from the first build, not deferred: mail, the generated privacy pages,
   client failure messages and any generated page use them. *Chunk 5.3b.*
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
    *Decided 2026-10-16:* yes, as recommended. *Built in chunk 5.1c.*

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
| Not built here (chosen in chunk 5.1) | A browser session's `origin:` and `signing_key:` reading settings, with signed sessions and tokens, are chunk 5.1b (built there, area 8). Which proxies a deployment trusts for a client's address is chunk 5.1c's, below | 2026-10-16 |
| Trusted proxies | `client_address: { source: ClientAddressSource.x hops: <Int> }` once in `app.jadpo`; sources `connection` (the native default), `forwarded_for`, `forwarded` and `cloudflare`; the client is the `hops`-th address from the right, falling back to the connection's address when the header is short or invalid; on Workers only `cloudflare` (`CF-Connecting-IP`), its default (question 12, Richard) | 2026-10-16 |
| Trusted proxies (chosen in chunk 5.1c) | `hops:` is required with `forwarded_for` and `forwarded` and refused with `connection` and `cloudflare` (`configuration.client_address_hops_unused`); written out it is a whole number from 1, and read from a setting that setting is an `Int` (or a type built on it) declaring `min:` of at least 1, so a count below 1 never starts (`configuration.client_address_hops_invalid`). A second `client_address:` is `configuration.client_address_duplicate`, a `client_address:` that is not a block of `source:` and `hops:` is `configuration.client_address_invalid`, and a `source:` that is no `ClientAddressSource` is `configuration.client_address_source_unknown` | 2026-10-16 |
| Reading the address (chosen in chunk 5.1c) | Every line of the header joined in order, split at commas; `Forwarded` by elements, each giving the entry of its `for`; an empty entry, a name, `unknown`, an obfuscated identifier or a zone takes its place in the count but is no address; a `Forwarded` that does not parse, or a `CF-Connecting-IP` that is not exactly one address, falls back whole. Addresses are accepted with a port or in brackets and kept in one spelling (IPv6 lower case and shortened, an IPv4-mapped address as IPv4), the connection's too. With no `client_address:` the address is the connection's natively and `CF-Connecting-IP` on Workers, where any other source is refused and an address Cloudflare did not give is unknown | 2026-10-16 |
| Where the address is chosen (chosen in chunk 5.1c) | In the runtime, as plan data (`ClientAddressPlan` in the route table) read with the settings the app started with: `serve_request` counts a request by the address `ClientAddress` chooses from its headers and the connection address the host gives, and carries it on the response, so `jadpo dev` shows it after the time on each request line and as `address` with `--json`. The native host only supplies the connection's address | 2026-10-16 |
| `ZonedTime` (chosen in chunk 5.2) | Replaces the time of day the first build called `Time`: an instant and the `Zone` it is seen in, with no literal, sent as `{ "instant": ..., "zone": "Europe/London" }`. Two are equal when both parts are; they have no order of their own, their instants do (`temporal.instant(...)`). An entity's `ZonedTime` field is stored as an object is, as `jsonb`, not built yet; it is not a setting. The only clock time is `resolve`'s `at:`, written out | 2026-10-16 |
| Time range (chosen in chunk 5.2) | An `Instant` holds 0000-01-01T00:00:00Z to 9999-12-30T22:00:00.999Z, the last whose local date is in year 9999 in every zone, and a `CalendarDate` 0000-01-01 to 9999-12-31; a literal past them is refused while compiling, and arithmetic past them is the fault `time_out_of_range` | 2026-10-16 |
| Time arithmetic (chosen in chunk 5.2) | `Instant ± Duration` and `Duration + Instant` keep the instant's named type; `Instant - Instant` of the same type after widening gives a plain `Duration`, negative when the second is later; `Duration ± Duration` of the same type, or with a literal, gives a plain `Duration`. A `CalendarDate` or `ZonedTime` takes no `Duration`, and `*`, `/` and `%` take no time value (`type.arithmetic_operand`, naming `temporal.add_days`) | 2026-10-16 |
| Time on the wire (chosen in chunk 5.2) | A `Duration` is written ISO 8601 in hours, minutes and seconds (`PT720H` for `30d`, `-PT5M`), and read the same way in JSON, query strings, paths and headers, days counting 24 hours; `5m` is source's spelling and refused there, but a setting keeps it. A `Zone` is sent as its IANA name and a `Locale` as its BCP 47 tag, as a setting reads them, and OpenAPI lists them so; a `ZonedTime` is an object of a `date-time` `instant` and a `zone`, both required | 2026-10-16 |
| Unclear local times (chosen in chunk 5.2) | `dst:` and `invalid_day:` each settle a condition a call fails on, which its `attempt` maps by the option's name (`dst:`, `invalid_day:`) as a construction's broken rule is mapped: an `invalid_value` failure, `invalid:` for the rest, `failure.rule_unmapped` and `failure.rule_cannot_break` as for rules. Only a value written out other than `reject` settles one; a value passed in might be `reject`. `Dst` keeps Temporal's meanings (earlier or later of an overlap; in a gap, the clock moved back or forward by its length; `compatible` earlier in an overlap and later in a gap); `InvalidDay` is `reject` or `last_valid_day`. `dst:` on a `CalendarDate` is `type.temporal_option_unused` | 2026-10-16 |
| `at:` (chosen in chunk 5.2) | A clock time written out, `HH:MM`, `HH:MM:SS` or `HH:MM:SS.mmm`, each field two digits; anything else, a value passed in included, is `type.temporal_clock_invalid`, since no type holds a time of day | 2026-10-16 |
| The `temporal` library (chosen in chunk 5.2) | Option B's operations, nineteen built: `in_zone`, `resolve`, `between`, `add_days`, `add_months` and `add_years` (twelve months a year) on a date or a local time, keeping its named type; `day_bounds`, `week_bounds` (from `starts_on:`), `month_bounds` and `year_bounds` giving an `InstantRange` whose `start` is the period's first instant (midnight, or the first time a zone's clocks show when they skip it) and whose `end` is the next period's, not included; `year`, `month`, `day` and `weekday` of a date or a local time's date; `date`, `hour`, `minute`, `zone` and `instant` of a local time, `instant` added because a `ZonedTime` has no order of its own. A query compares a field with an `InstantRange` parameter's `start` and `end` | 2026-10-16 |
| Time zone data (chosen in chunk 5.2) | The `temporal` operations run in `jadpo-value` on jiff 0.2.38 with its bundled database (`jiff-tzdb`, IANA 2026e, about 200 KB): pure Rust, built for wasm32, with Temporal's handling of gaps and overlaps, and nothing read from the platform's zone files or clock, so the interpreter, native deployables and Workers answer alike. Every `Zone` variant is in it, which a test holds; a newer release is a change of its own | 2026-10-16 |
| Not built here (chosen in chunk 5.2) | The formats (`TimeFormat`, `ComponentStyle`, the built-in formats), `PresentationText` and its errors, and `temporal.format` and `format_relative` checked are chunk 5.2b; the text they give, in each locale, is chunk 5.3 | 2026-10-16 |
| Formats (chosen in chunk 5.2b) | `TimeFormat` is a base, as `Object` is: `type Name = TimeFormat { part: ComponentStyle.style ... }` declares a format, and `TimeFormat.` names the twelve built-in ones (`date_`, `time_`, `date_time_` with `full`, `long`, `medium`, `short`). The parts are `weekday` (`long`, `short`, `narrow`), `day`, `year`, `hour`, `minute` and `second` (`numeric`, `two_digit`), `month` (all five) and `zone_name` (`long`, `short`), as `Intl.DateTimeFormat` takes them; the locale orders them. At least one part (`type.temporal_format_components_empty`), each once (`type.temporal_format_component_duplicate`), in a style it takes (`type.temporal_format_component_invalid`); any other name is `type.unknown_setting`. A format holds no value: `format:` names one written out, and a format as a field's, parameter's or variable's type, as a value or passed in is `type.temporal_format_invalid`. The operation is given it as text, `day:numeric month:short year:numeric` or `date_long` (`jadpo_std::TimeFormatSpec`), as `at:` gives its clock time | 2026-10-16 |
| What formatting takes (chosen in chunk 5.2b) | `format` shows an `Instant`, a `CalendarDate` or a `ZonedTime`, and `format_relative` an `Instant` or a `ZonedTime` against `now:`, an `Instant`. `zone:` is given exactly for an `Instant` (`type.temporal_zone_required`), and with a date or a local time is `type.temporal_option_unused`. A date takes only a date format: a `time_` or `date_time_` one, or a part `hour`, `minute`, `second` or `zone_name`, is `type.temporal_format_requires_time`, which names that now rather than an instant without a zone. `locale:` was any `Locale` until chunk 5.3, which checks it against the supported ones. `type.temporal_format_choice` is gone: `format:` replaced `style:` and `components:` (Option B) | 2026-10-16 |
| `PresentationText` (chosen in chunk 5.2b) | A built-in type that is its own representation, so no constructor turns it into `Text` or back, and nothing orders it, adds to it or reads a time from it; `==` and `!=` compare it. A signature may use it directly, as `Email`, and text in quotes may stand for one, which is how a test compares formatted text. An `Output` holds it, sent as a JSON string, and a request may send one as text. An entity field holding it, directly or through a named type, a collection, an object's field or a variant's data, is `type.presentation_text_persistence`; a setting of it is `configuration.type_unsupported` | 2026-10-16 |
| Not built here (chosen in chunk 5.2b) | The text: a call to `temporal.format` or `format_relative` that checked was `toolchain.not_built_yet`, naming chunk 5.3. Chunk 5.3 built it (below) and removed the refusal | 2026-10-16 |
| `locales { }` (chosen in chunk 5.3) | An unnamed singleton declaration in `app.jadpo` (as area 1 reserves the word), not a member of `application { }`: the page's `locales: { ... }` was written with a colon and is amended to `locales { default: Locale.en_gb supported: [Locale.en_gb, ...] }`, each once and no others, entries written `Locale.<tag>` (`type.locale_invalid` for text, a non-list, a variant `Locale` has not; the fix for `"fr-FR"` is one edit), none twice (`type.locale_duplicate`), the default among them (`type.locale_default_unsupported`). A missing option is `syntax.locales_default_required` or `syntax.locales_supported_required`, a given-twice option `syntax.locales_duplicate_option`, any other name `syntax.locales_unknown_option` (`unsupported:` among them: there is no such setting), a second `locales { }` `semantic.multiple_locale_declarations`. `syntax.locales_unsupported_required` and `syntax.locales_unsupported_invalid` are gone with `unsupported:` | 2026-10-16 |
| What a locale is checked against (chosen in chunk 5.3) | A project formats text for people only with `locales { }` declared (`type.temporal_locales_required`, at the call); files handed to the compiler on their own have no application to declare them, so the rule does not apply. A `Locale.x` written as `locale:`, or as the `default:` or `test:` of a `Locale` setting, that is not supported is `type.locale_unsupported`. A `Locale` passed in is read through the wire or a setting, which accept only the supported tags (`invalid_value`, rule `variant`; OpenAPI lists only those); with no `locales { }` every `Locale` is accepted as before. A problem in the declaration is reported once, and the call is not also reported | 2026-10-16 |
| Locale data (chosen in chunk 5.3) | ICU4X 2.3 with Unicode CLDR 48 compiled into the binary (`icu_datetime`, `icu_calendar`, `icu_time`, `icu_locale` 2.3 and `icu_experimental` 0.6 for the relative-time words, pinned exactly; pure Rust, builds for wasm32): the interpreter, native deployables and Workers write the same text. A WebAssembly module that formats is about 3.5 MB larger than one that does not (3.9 MB against 0.35 MB; 1.2 MB against 0.1 MB compressed with gzip, 0.8 MB with Brotli), because the data holds every locale `Locale` lists. The next lever, if a deployable needs it, is data exported for the supported locales only | 2026-10-16 |
| Writing a time (chosen in chunk 5.3) | Always the Gregorian calendar (as `Intl.DateTimeFormat` with `calendar: "gregory"`), the locale's own digits, CLDR's narrow no-break space written as a plain space. A built-in format is the locale's pattern at CLDR's four lengths (`date_full` with the weekday), so `date_time_long` is "27 September 2026 at 14:30:05 BST" in `en_gb`, with seconds as `timeStyle: "long"` has. A declared format takes the locale's pattern for its parts (a month as a name from the long or medium date pattern, a number from the short one; a date and a time joined as the locale joins them for that month) and sets each part's width: `numeric` leaves a number as the locale writes it, `two_digit` makes it at least two. Parts with no pattern together are written one after another with a space, and `zone_name` alone is the name alone | 2026-10-16 |
| `format_relative` (chosen in chunk 5.3) | Against `now:`, in the zone the value is seen in: under a minute either way "just now" (English; the locale's "now" elsewhere); under an hour whole minutes, a past value rounded down and a future one up; the same local date "Today at" and the short time; the date before or after "Yesterday at" or "Tomorrow at"; two to six days away the weekday and the short time; anything else the long date. The words are the locale's, the first letter a capital; a locale with no word for yesterday or tomorrow shows the long date | 2026-10-16 |
| Not built here (chosen in chunk 5.3) | Translations (`translations/`, ICU MessageFormat, typed calls, their errors) and choosing a locale from an `Accept-Language` header, which the page calls a library call without naming it, are chunks 5.3b and 5.3c | 2026-10-16 |
