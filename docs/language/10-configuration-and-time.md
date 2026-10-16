---
title: Configuration and time
area: 10
status: decided
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

> Decided on 2026-10-12. Code on this page applies the decisions from areas 1 to 10. The new compiler has built the time literals and the built-in `Zone`, `Weekday` and `Locale` enums (chunk 1.9) and checks where `clock.now` may be read; the rest comes in Phase 5 (chunks 5.1 to 5.3).

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

### Environment variable names

The variable name is the field name in capitals. Write `binding:` only when it
must differ, for example `binding: "HEROKU_POSTGRESQL_URL"` to keep a name a
hosting platform already uses.

The cost of deriving it: renaming a field renames the variable, which breaks
a deployment that still sets the old name. A changed name is therefore shown
in the approval review like any other contract change (question 1).

### What a setting can hold

A setting is read from one environment variable, so its type must be decodable
from one piece of text: text-based types, `Int`, `Decimal`, `Bool`, `Uuid`,
`Email`, `Url`, `IpAddress`, `Duration`, `Instant` and `CalendarDate`. Lists
and objects are rejected (`configuration.type_unsupported`): split them into
fields.

Proposed: also accept enums, written `Type.value`, and the built-in enums
`Zone` and `Locale` (question 2):

```jadpo fragment
config {
    log_level: LogLevel {        // type LogLevel = Enum { debug info warning }
        default: LogLevel.info
    }
    business_zone: Zone {
        default: Zone.europe_london
    }
}
```

`LOG_LEVEL=debug` and `BUSINESS_ZONE=Europe/London` would then be valid
values.

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

Comparisons are included: `key == config.api_key` is rejected, because a
yes/no answer leaks the secret one guess at a time.

## Database and other connection URLs

Area 5 decided that stores take their address from configuration:

```jadpo fragment
application {
    stores: {
        main: {
            engine: Engine.postgres
            url: config.database_url
        }
    }
}
```

`Url` accepts only `http://` and `https://`, so it cannot hold a
`postgres://user:password@host/db` address.

Proposed (question 3): built-in connection types, one per store engine,
`PostgresUrl` (`postgres://` or `postgresql://`) and `RedisUrl` (`redis://`
or `rediss://`). They are always secret, because they normally carry a
password, and a store's `url:` must have the type matching its `engine:`.
The runtime opens the store only through that field.

## Values on your machine and in production

Production values come from the platform's environment (or its secret
manager, which sets the same variables). Locally there is one git-ignored
file, `.env.local`; Jadpo defines no `.env`, `.env.development` or layering.
Tests use each field's `test:` value, and `jadpo test` generates random
secrets; a single test can override a value (area 13).

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
name, never the value. `jadpo dev` runs the same check against `.env.local` and keeps the last good version
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
  `type.clock_context`; any other member of `clock`, `config`,
  `translations`, `sessions` or `oauth` is `toolchain.not_built_yet` until
  its area is built.

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
`number`, `decimal`, `date`, `date_time`, `time_of_day` or `duration`.

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

Text Jadpo writes itself (the generated privacy pages, built-in emails,
generated messages) is Jadpo's own messages, with wording shipped for
common locales; `jadpo translations builtin` lists them with their fields.

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
(area 5). How retention fits the wider GDPR support (personal-data marking,
erasure, export) is designed in area 15 (Personal data).

## Errors

> Most of these examples predate the decisions above and are rewritten when the area is built (roadmap Phase 5).

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

A PostgreSQL address is not a `Url`:

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
action as `type.clock_context`. Settings and secrets come in chunk 5.1, time
arithmetic and formatting in chunk 5.2, and locales and translations in
chunk 5.3: `Message` types and the built-in `PluralCount`, each domain's
`translations { }` language files and their checks, the generated
`translations.<domain>.<message>` calls, enum wording, failures' client
text exported per locale, `locale.direction`, `in_progress:` locales,
Jadpo's own messages and the root file's owner groups, and the
`jadpo translations` commands (decided 2026-10-16, see Translations).
Library packages' translations come with packages in Phase 7.

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
| Fixture configuration values, the fixture clock and `advance clock by` (syntax review proposes `clock.advance(5m)` and plain fixture values); `configuration.fixture_*`, `configuration.field_unknown` (raised from source only by a fixture) and `type.test_clock_context` | 13 | Input to area 13 |
| `jadpo config set`, `config check` and `dev` restarts; `configuration.value_invalid`, `configuration.value_missing`, `configuration.local_*` and `cli.config_*`, none of which `jadpo check` raises; the startup report (`runtime.startup_failed`) | 14 | Input to area 14 |
| Health and readiness, including the generated `health { }` proposal (syntax review #65) and `semantic.service_configuration_missing` | 11 | Input to area 11 |
| Integration credentials and authentication signing keys as the only secret sinks; their setting names (`token:`, `signing_key:`) | 8, 11 | Input to areas 8 and 11 |
| Mail templates using translations and `PresentationText` | 11 | Input to area 11 (Superseded 2026-10-16: templates read `translations.` and are checked strictly; see area 16 question 11.) |
| A library package's own messages, and whether an app can reword them | 11 | Waits for open item 8 of the packages proposals |
| `config.typo` should report the configuration field, not `__jadpo_config`; `jadpo fmt` writes `temporal.format (` with a space | 14 | Bug |
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
   validators the same code, so they cannot disagree again. *Decided 2026-10-12:* yes, as recommended.
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
    it once the longest token lifetime has passed.
11. **Do configuration fields carry `test:` values?** (carried from area 13)
    Area 13 has tests take each configuration field's `test:` value, while
    "Values on your machine and in production" above says tests get their
    values from fixtures. Which does configuration declare?
    *Decided 2026-10-12:* a field may carry a `test:` value in `config.jadpo`;
    secrets need none, because `jadpo test` generates random ones; a single
    test can override a value.

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
| Failure export shape | `<api>.failures.<locale>.json` maps each code to `{ "text": ..., "formats": { placeholder: format } }`, the same shape for every code; `formats` lists only placeholders the client formats, as `number`, `decimal`, `date`, `date_time`, `time_of_day` or `duration`, decided by the field's type (`CalendarDate` date, `Instant` date_time in the reader's zone, `Time` date_time in its own zone, `Duration` duration, `Int` number, `Decimal` decimal); a type the client cannot format is an error (decided for Richard) | 2026-10-16 |
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
| Jadpo's own text | Jadpo's own messages, with wording shipped for common locales; `jadpo translations builtin` lists them (Richard) | 2026-10-16 |
| Text the app did not write | The root `translations/<locale>.jadpo` groups wording by owner, `jadpo: { }` and a package's name, the one grouping allowed, checked against the owner's types; lookup is the app's root wording, then the owner's, then (in progress) the regional sibling, then the default; supported locales must be complete for every package and Jadpo message the app can show (Richard) | 2026-10-16 |
| Package translations | A library package carries a `translations/` folder like a domain's, called `Postcodes.translations.<message>(...)`; provider packages carry none; changed fields break the app's rewording, changed wording makes it stale; published with complete default-locale wording and `///` on every public message; a different code for a package failure is mapped with `attempt` (Richard) | 2026-10-16 |
| Failures sent to clients | Wording lives in the language files, never on the failure type (one rule for all text; Jadpo's and packages' failures could not carry it in place); every failure that can reach a response has wording in every supported locale; dates and numbers in client fields are sent as values and formatted by the client, and the per-locale export says which placeholders need formatting and how (Richard) | 2026-10-16 |
| Translation tooling | `jadpo translations missing` and `unused` with `--json`; `update`, `accept <locale>` and `builtin`; `export` and `import` as XLIFF or JSON; `--json` wherever they print; go to definition from a call to its type and wording; a pseudo-locale for development and tests; the app's own data is not a translation (Richard) | 2026-10-16 |
