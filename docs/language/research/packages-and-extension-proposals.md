# Packages and extension points

Status: working document, 2026-10-16, for [area 11](../11-integrations.md),
with changes to areas 1, 2, 8, 14 and 16. Richard and Claude are working
through it one part at a time. Each part says whether it is **decided**
(Richard agreed it) or **proposed** (not yet discussed). When every part is
decided, the decisions move to the area pages and this page becomes the
research behind them.

Richard's question (2026-10-16): before chunks build anything an app might
want to swap, extend or get from a package (email in 7.3 is the example),
how are packages installed, where do they sit, how do they reach the
program, and how is a feature turned on, off or replaced?

## At a glance

**Decided** (2026-10-16 unless marked):

| Part | In short |
|---|---|
| Swapping a provider | A connection in `app.jadpo` (area 11, 2026-10-13) |
| Kinds of package | Library and provider; no feature or driver packages |
| What a package looks like | `package.jadpo` (`licence:`, `jadpo:`), optional `provider.jadpo`, `shared/`-style folders, `public` as the API |
| Where packages live | Installed: `packages/<name>/<version>/`, committed, locked, read-only. Your own: `shared/providers/` |
| Names | Brand names reserved; rename yours on a clash |
| The surface | `surface.jadpo_record`, readable, with file pointers; `jadpo package show` |
| Publishing | The seven-point bar |
| Side effects | None in packages; the app's actions make every call |
| Decoding | `wire:` and `decode:` in the package; translation in the app |
| Presets | None; built-in drivers configured in `app.jadpo`, vendor settings documented per driver |
| Dependencies | None between packages |
| Capabilities | Only jobs Jadpo itself does, each whole (design C) |
| Services | Stateless containers through `runs:`, Docker or Podman hidden, `jadpo services` |
| Development | Provider calls faked by default, let through by name, logged; `jadpo watch` |
| Built in or a package | Agreed as input to chunk 5.4 |
| The golden rule | A package can do nothing the app cannot; the app can do everything a package can |

**Open**, in the order they are being worked through. Each is settled
with Richard one at a time, recorded on this page as decided, and only
then built. No design is left for implementation.

1. ~~The golden rule~~: decided, see "Decided: the golden rule".
2. ~~Translations~~: decided, see "Translations".
3. ~~Authentication~~: decided, see "Authentication" and "Built-in emails".
4. ~~Side effects and packages~~: decided, see "What package code may do".
5. ~~Overriding~~: decided, see "Decided: overriding built-in behaviour".
6. ~~Each override scenario~~: decided, see "Override scenarios" (webhook handshakes move to item 7).
7. ~~Receiving from outside~~: decided, see "Receiving from outside".
7b. ~~Multi-step provider calls~~: decided, see "Multi-step provider calls".
7c. ~~Per-user OAuth~~: decided, see "Acting for each user: OAuth grants".
7d. ~~Redis entity queries~~: decided, see "Redis entities".
8. ~~What a library package may also carry~~: decided, see that section.
9. ~~Software and disk~~: decided, see "Documents" and "Files and formats".
10. ~~Packages generated from your own Jadpo apps~~: decided, see "Packages from other Jadpo apps".
10b. ~~Developing a package~~: decided, see "Developing and publishing packages".
10c. ~~Where committed records live~~: decided, see "Generated files and deploy-time choice".
11. ~~Private packages~~: decided, see "Developing and publishing packages".
12. ~~Choosing a provider when deploying~~: decided, see "Generated files and deploy-time choice".
13. **Then**: move every decision onto the area pages (1, 2, 7, 8, 10,
    11, 12, 14, 15, 16, 17, 18) and the roadmap's chunks, through one pull
    request.

Research behind this page: [package layouts](package-layout-options.md),
[capability providers](capability-providers-options.md),
[override scenarios](override-scenarios.md),
[capabilities](capabilities-proposals.md).

## Decided before this: swapping a provider

Area 11's provider packages (questions 1–18) and capabilities (questions
19–25), decided 2026-10-13, already answer the email case. Email is a
**capability**: a job several providers do, with one API in application
code. SMTP is one built-in driver for it; a mail package such as Postmark
is another, providing the capability through a declarative `provide email
{ }` mapping. `app.jadpo` names which one is used:

```text
// app.jadpo
application {
    connections: {
        main:     { driver: Driver.postgres  url: config.database_url }
        postmark: { package: Postmark.v1  credential: config.postmark_token
                    message_stream: "outbound" }
    }
    email: { connection: postmark  from: config.mail_sender }
}
```

There is nothing to disable: SMTP does nothing unless a connection names
it, and changing provider changes `app.jadpo`, never domain code. The
capabilities are a fixed list owned by the language (database, cache,
files, email, publishing, schedules; SMS removed by design C below), each with standard types and
events, a local driver for development, one fake for tests and a
conformance suite. Packages fill capabilities and never invent them.

## Translations (open item 2)

Decided 2026-10-16 (Richard). Area 10 question 9 chose ICU MessageFormat
with the default locale's file defining the keys; both are replaced below.

**Where they live.** In the domain that uses them, as its emails do; a
message a second domain uses moves to `shared/`, under area 1's promotion
rule. The root `translations/` holds only the app's wording for text it
did not write, grouped by owner (Jadpo's and library packages', see
"Packages and strictness" below); text several domains use is in `shared/`. This changes area 1, which made
`translations/` a root-only folder.

```text
domains/shipping/translations/
    order_shipped.jadpo       type OrderShipped = Message { ... }
    parcels.jadpo             type Parcels = Message { ... }
    en_gb.jadpo               translations { ... }: the English wording
    de_de.jadpo
    sources.json              generated: what each translation was made from
shared/translations/          messages several domains use
translations/                 Jadpo's and packages' text, reworded by the app
```

**Each message is a type.** `Message` is a type base beside `Failure`,
`Event` and `EmailMessage`; its fields are the placeholders, its `///`
descriptions reach translators as context, and its member in each
language file is its name in lower_snake_case. A backend writes few messages (emails, generated pages,
failures for clients), so one file per message follows area 1 without
cost.

```text
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

**Each language is an unnamed `translations { }` in Jadpo syntax**, in a
file named after its locale, as `config { }` is found by its name: a name
would add nothing, since code never refers to it (Richard, 2026-10-16,
after a named `Translations` type was weighed). JSON was dropped: no
comments, quotes everywhere, and area 11's principle that the compiler
parses only Jadpo. A member is a message's or failure's name in
lower_snake_case, found by area 1's name conversion (`order_shipped` is
`OrderShipped`, as a file name and a failure code already are). Every
wording is a plain string whose only syntax is `{field}`; a message with an
enum field has one member per value, one with a `PluralCount` field one member
per plural form the language needs, from Unicode's CLDR data (English `one`
and `other`; Polish `one`, `few`, `many` and `other`). Dates and numbers are
formatted in code with `temporal.format` and passed in as text. Translation
services work through `jadpo translations export` and `import` (XLIFF, or
JSON).

```text
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

```text
translations.shipping.order_shipped(tone: shop.tone, name: customer.first_name, locale: customer.locale)    // in code
{{ translations.shipping.parcels(count: parcel_count) }}                     // in a template
```

**How a type maps to its strings**:

| In the type | In each language's file |
|---|---|
| The type's name, `OrderShipped` | The member `order_shipped`, in the language files of the type's own folder |
| No fields | A plain string |
| A `Text`, `PresentationText`, `Int` or `Decimal` field | A `{field}` placeholder; numbers are written in the language's digits |
| One enum field | An object with one key per value of the enum; the field is not a placeholder |
| One `PluralCount` field | An object with one key per plural form the language needs; `{count}` is available in each |
| Any other field type (`Instant`, an object) | A compile error: format it in code and pass text |
| More than one enum or `PluralCount` field | A compile error: split the message, so a translator never fills a grid of combinations |

A `Failure` maps the same way: its code is the key, its client fields are
the placeholders, and `kind:` and `message:` are not. The generated call
takes every field by name plus `locale:` (in templates the email's
locale), and returns `PresentationText`. A placeholder that is not a field
is an error; a field that no wording in a language uses is a warning, and
one variant leaving a field out is normal. An enum maps the same way as a
message with one enum field (see enums below).

**Checked by the compiler**: a key no message type declares, a variant
that is not a value of the enum, a missing variant or plural form, and a
missing or unknown placeholder are errors; a message not yet translated in
a locale is an error too, unless that locale is in progress ("Packages and
strictness", 1 and 2, below). No grouping inside a domain's file (the
domain is the group; nesting means variants and plural forms only), and no
message inside another (reused values such as a product name are
placeholders), because languages that inflect cannot reuse a fragment.

**Failures and emails follow the same rule.** A `Failure`'s client text
is keyed by its code in its domain's files, like a message (names are
unique across the project, so a failure and a message never share a key),
with its client fields as placeholders; it is exported per locale beside
the OpenAPI document for client apps, while the server's `message` stays
English (area 4). An `EmailMessage`'s templates read `translations.` with the email's
locale.

**Jadpo's own text** (privacy pages, built-in emails, generated messages)
is Jadpo's own messages, with defaults shipped for common locales. The
root `translations/<locale>.jadpo` holds only these: its members are
Jadpo's message names in lower_snake_case with no prefix
(`privacy_export_ready: "..."`), and the app changes one by writing it
there; `jadpo translations builtin` lists them with their fields.

**Tooling**: `jadpo translations missing`, `unused`, `builtin`, `update`,
`accept`, `export` and `import`, with `--json` wherever they print; go to
definition from a call to its type and its strings; a pseudo-locale that
renders every message accented and lengthened to expose hard-coded text
and tight layouts; Crowdin and Lokalise through `jadpo translations export`
and `import`.

**Templates are strict**: a name that is not a field of the email's type,
a message that does not exist, a placeholder missing or of the wrong type,
a partial that does not exist or is called with the wrong fields, `if` on
something that is not `Bool`, `for` over something that is not a list, and
any call other than translations and formatting are compile errors. A field
the template never uses is a warning. Values are escaped; HTML goes in
unescaped only as the safe-HTML type (input to chunk 5.4).

**Only text and numbers are written out** (Richard, 2026-10-16), in
templates as in placeholders: `{{ value }}` takes `Text`,
`PresentationText`, `Int` or `Decimal` (numbers written the locale's
way). Anything else must be formatted first, and writing it out directly
is a compile error that names how: an `Instant` or `Money` with
`temporal.format` or the money formatter, an enum through its translated
wording (below), a `Bool` is a condition for `if`, a list is walked
with `for`, an object's fields are written one by one.

**Also decided on translations** (2026-10-16, as recommended):

- **Every supported language is complete** (widened from the default
  language alone by "Packages and strictness", 1, below): a message missing
  from one is an error.
- **Regional fallback**: a message missing from `fr_ca` tries `fr_fr`, if
  supported, before the default language. (Narrowed by "Strict by default"
  below: only an in-progress locale falls back.)
- **Stale translations are flagged**: each translation records which
  version of the default wording it was made from; when that wording
  changes, the others become warnings until updated, listed by
  `jadpo translations missing`.
- **Agent translations can need review** (made precise in answer 7
  below): an app may mark translations an
  agent wrote as not yet reviewed, listed in the approval report until a
  person accepts them.
- **Text direction**: every locale knows its direction, and templates read
  it (`<html dir="{{ locale.direction }}">`).
- **Enums can be written out once translated** (Richard, 2026-10-16): a
  domain's language files may give an enum one wording per value
  (`order_status: { pending: "Waiting to be packed" ... }`), checked like a
  message, a value's fields usable as placeholders; then `{{ order.status }}`
  writes the translated wording, and code reads it with
  `translations.shipping.order_status(order.status, locale: ...)`. Without
  wording, writing an enum out is an error that says to add it. Area 2's
  "enums have no display labels" stands: the wording lives in translations.
- **Out of scope**: the app's own data (product names, articles) is not a
  translation; an app models translated content as ordinary fields.
- ~~Waiting for open item 8: a library package's own messages, and whether
  the app can reword them.~~ Decided below, "Packages and strictness".

**Answered 2026-10-16 (Richard), so chunk 5.3 leaves nothing to decide:**

1. The plural type is `PluralCount`, not `Count`, so no existing example
   that declares its own `Count` is renamed.
2. **Email subjects**: an `EmailMessage`'s member in its domain's language
   files is its subject line, its fields usable as placeholders
   (`welcome_email: "Welcome, {display_name}"`); the body stays the `.html`
   and `.txt` templates.
3. A message in `shared/translations/` is called
   `translations.shared.<message>(...)`.
4. **Jadpo's own messages** have no `jadpo.` prefix: the root
   `translations/<locale>.jadpo` holds only them, named in lower_snake_case
   (`privacy_export_ready`), and `jadpo translations builtin` lists them
   with their fields. (The no-prefix form is replaced by owner groups in
   "Packages and strictness", 6, below.)
5. A literal brace in a wording is doubled: `{{` writes `{`, `}}` writes
   `}`.
6. **Stale check**: each `translations/` folder has a generated, committed
   `translations/sources.json` (machine-written, so JSON): for each locale
   and message, a hash of the default wording it was translated from.
   `jadpo translations update` and `import` write it; area 18's records
   table lists it.
7. **Review**: `translations: { review: TranslationReview.required }` in
   `app.jadpo` (default `.none`) makes every new or changed non-default
   wording unreviewed until a person runs `jadpo translations accept
   <locale>`, which records the acceptance in the same `sources.json`;
   unaccepted wording appears in the approval report. Jadpo cannot tell who
   wrote a wording, so it is "changed and not yet accepted", not "written
   by an agent".
8. **Failure export**: `jadpo build` writes, beside each API's OpenAPI
   document, one file per locale named `<api>.failures.<locale>.json`, a
   flat object from failure code to wording in the same `{field}` syntax.
   (Revised in "Packages and strictness", 4, below: the file also says
   which placeholders the client formats.)
9. Named types built on `Text`, `Int` or `Decimal` work as placeholders.
10. `locale.direction` is a built-in enum `TextDirection` with
    `.left_to_right` and `.right_to_left`; Jadpo ships its wording as the
    HTML values, so `{{ locale.direction }}` writes `ltr` or `rtl`.

**Packages and strictness** (Richard, 2026-10-16), which also decides
open item 8's translations part:

1. **Strict by default**: every supported locale must be complete; a
   missing message, failure or enum wording in any supported locale is a
   compile error whose `next:` names the file and the member to add, with
   the placeholders to use (agent). This replaces "other locales fall back
   with a warning".
2. **Locales in progress**: `locales: { ... in_progress: [Locale.pl_pl] }`
   in `app.jadpo`. Only a locale listed there falls back (regional
   sibling, then the default locale) with warnings listed by
   `jadpo translations missing`; removing it from the list makes gaps
   errors again. Quality comes from the decided review setting, not from
   waiting.
3. Every failure that can reach a response (the compiler knows from each
   route's failures) must have wording in every supported locale; failures
   that never leave the server need none.
4. A failure's client fields that are dates or numbers are sent as values
   (an `Instant` stays an instant on the wire) and the client formats them
   for its own language before filling the placeholder; the exported
   per-locale file says which placeholders need formatting and how (date,
   time, number).
5. **Library packages carry translations**: a `translations/` folder like
   a domain's (message types, failure wording, one `<locale>.jadpo` per
   language they support); the app calls a package's messages as
   `Postcodes.translations.<message>(...)`. Provider packages show users
   no text (their failures are always mapped to the app's own), so they
   carry none.
6. **The root `translations/<locale>.jadpo`** holds wording for text the
   app did not write, grouped by owner: `jadpo: { privacy_export_ready:
   "..." }` for Jadpo's own, `postcodes: { invalid_postcode: "..." }` for a
   package's. This is the one place grouping is allowed, the group being
   whose text it is; it replaces the no-prefix form in answer 4 above. It
   is checked against the owner's message and failure types.
7. **Lookup** for a package's or Jadpo's message: the app's root wording
   in that locale, then the owner's own, then (for an in-progress locale)
   the regional sibling, then the default locale in the same order.
   Completeness: the app's supported locales must be complete for every
   package and Jadpo message the app can show, so an English-only package
   in an app supporting French is an error until the app's root file
   supplies the French.
8. **Package upgrades**: a package changing a message's or failure's
   fields is a breaking change by the surface diff, so the app's rewording
   fails to compile until updated; a package changing its own wording
   marks the app's rewording stale (`sources.json`). Publishing: a library
   package ships complete wording in its own default locale, with `///`
   descriptions on every public message.
9. Changing a package failure's code (not just its words) is done by
   mapping it to the app's own failure at the call with `attempt`, which
   already exists.

10. Failure wording stays in the language files, not on the failure type
    (one rule for all text; Jadpo's and packages' failures could not carry
    it in place). Recommended, then agreed by Richard on 2026-10-16.
11. **Export shape** (decided for Richard, 2026-10-16):
    `<api>.failures.<locale>.json` maps each failure code to
    `{ "text": "…{next_slot}…", "formats": { "next_slot": "date_time" } }`,
    the same shape for every code; `formats` lists only placeholders the
    client must format, each one of `number`, `decimal`, `date`,
    `date_time` or `duration`; plain text placeholders are
    not listed.
12. **Format by type**: a failure's client field decides its format from
    its type: `CalendarDate` → `date`; `Instant` → `date_time`, shown in
    the reader's own zone by the client; `Time` (`ZonedTime` once renamed)
    → `date_time` in the zone it carries; `Duration` → `duration`; `Int` →
    `number`; `Decimal` → `decimal`. A field type the client cannot format
    (an object, a list) is a compile error for a placeholder, as for
    messages.
13. An `in_progress` locale not in `supported` is a compile error.

## Authentication (open item 3)

Decided 2026-10-16 (Richard). Area 8 keeps its two layers: checking the
person, then a session (`sessions.start` for a browser, `sessions.issue_token`
for the app's own clients).

- **Magic-link sign-in is built in**, beside the providers, replacing area
  8's "how the person is checked stays in the application".
- **`sign_in_providers:` becomes `sign_in:`**, one list of every way to
  check a person (`magic_link:`, `google:` ...), in `authentication.jadpo`
  at the project root.
- **`on_first_sign_in` is the one decision point for every way in**,
  external tokens included: create, link or refuse; a refusal looks the
  same to the person.
- **The flow**: `POST /auth/magic-link` answers the same whether or not an
  account exists; the email carries a link and a six-digit code, both
  single-use, either working; the link opens a one-click confirm page
  (superseded: "Generated routes; no pages" below)
  (mail scanners open links), worded from translations; confirming starts
  a browser session, and an app client posts the code to
  `POST /auth/magic-link/redeem` for an API token. Only verifiers are kept.
  The email goes through the app's `email:` connection; magic links without
  one are a compile error. In development the link works from the local
  inbox; tests read it from `email.sent`.
- **Settings, with defaults**: `address: User.email`, `expires_after: 15m`,
  `code_attempts: 5`, `codes_per_hour: 10`, `failed_codes_per_day: 20`,
  `resend_after: 1m`, `same_browser: false`. Tightening is free; loosening
  past a ceiling (more than 10 attempts per code, an expiry over an hour,
  more than 50 failed codes a day) needs an acceptance in the approval
  report.
- **Wrong codes**: five wrong guesses end that code; every wrong guess gets
  the same answer, never a count; after the day's failures codes stop
  working for that address until the next day but links still work; no
  account lockouts, so an attacker cannot lock a person out.
- **Other rules**: only the latest link and code are valid; sends are
  rate-limited per address with identical answers for unknown addresses;
  after signing in a person is sent only to a path on the app's own site;
  signing in by link or code makes the address verified for
  `on_first_sign_in`; built-in events `SignedIn`, `SignInRefused` and
  `SignInCodesDisabled` for monitoring.

## Built-in emails (authentication follow-up)

Decided 2026-10-16 (Richard), and written into area 8 ("Checking the
person: `sign_in:`") and area 16 ("Built-in emails"), with cross-references
from areas 10, 11 and 15.

- **Delivery**: every email, built in or the app's own, goes through the
  app's one `email: { connection: ... from: ... }` in `application { }`, the
  SMTP driver or any package providing `email`. Magic links without it are
  a compile error, unless the app sends its own (level 5).
- **Level 1, nothing**: Jadpo renders its own email with its own wording,
  shipped in `en_gb` and `en_us` at first, more locales added by releases;
  an app supporting a locale Jadpo does not ship must word it, a compile
  error listing the messages until it does.
- **Level 2, the words**: the root `translations/<locale>.jadpo` under
  `jadpo: { }`. Strict: rewording a message of a feature the app has not
  switched on is an error saying the feature is off; `jadpo translations
  builtin` lists only switched-on features' messages, with their fields.
  The magic-link email's messages are `magic_link_email` (the subject),
  `magic_link_heading`, `magic_link_intro` (`address`),
  `magic_link_button`, `magic_link_code_line` (`code`),
  `magic_link_expiry_line` (`expires_in`) and `magic_link_ignore_line`;
  and the failures' wording (the confirm page's messages were removed by
  "Generated routes; no pages").
- **Level 3, branding**: built-in emails are wrapped in the app's shared
  email layout automatically.
- **Level 4, the app's own email**: `magic_link: { email: accounts.SignInEmail }`
  names an `EmailMessage` that `pick:`s from the built-in `MagicLinkEmail`
  (`address`, `sign_in_link`, `code`, `expires_in` as `PresentationText`
  Jadpo formats, with `to:` and `locale:`), with its own `.html` and `.txt`
  templates and its subject from translations, under the strict template
  rules.
- **Level 5, the app's own sending**: `magic_link: { send: accounts.send_sign_in_link }`
  names an action taking a built-in `MagicLinkRequest` (`address`,
  `sign_in_link`, `code`, `expires_at: Instant`, `locale`), run after
  commit like a subscriber and retried under area 12's rules; in
  development its provider calls are faked and logged, and the dev inbox
  does not show it.
- Levels 4 and 5 are exclusive on one email, an error if both are set. The
  same five levels apply to every email Jadpo sends: the magic-link email
  and area 15's privacy emails.
- **Magic-link details written out for the spec** (area 8): the four
  routes (`POST /auth/magic-link`, `GET` and `POST
  /auth/magic-link/confirm`, `POST /auth/magic-link/redeem`), `return_to`
  limited to a path on the app's own site, the email's locale from the
  request, `same_browser:` as a cookie, the failures
  `sign_in_link_expired`, `sign_in_link_other_browser`,
  `sign_in_code_invalid`, `sign_in_codes_disabled` and `sign_in_refused`,
  `FirstSignIn` for `on_first_sign_in:`, and the events' fields.
- **Privacy emails** (decided for Richard): built-in
  `PrivacyAddressCheckEmail`, `PrivacyExportEmail`,
  `PrivacyErasureConfirmedEmail` and `PrivacyErasureCompletedEmail`, with
  fields Jadpo fills (links as `Url`, times as `PresentationText` in the
  email's language) and a matching `...Request` type for `send:`. A
  feature with one email takes `email:` and `send:` directly; one with
  several takes `emails: { export_ready: { email: ... }
  erasure_confirmed: { send: ... } }`, each entry `email:` or `send:`,
  never both. Area 15's `self_service: { email: <connection> }` becomes
  `self_service: {}`.
- **Shared email layout** (decided for Richard): optional
  `shared/emails/layout.html` and `layout.txt`, one per app, receiving
  `{{ content }}` (safe HTML in the HTML layout), `{{ subject }}`,
  `{{ locale.direction }}` and `{{ app.display_name }}`, able to call
  translations, under the strict template rules; it wraps every email,
  and Jadpo's plain layout is used without it.
- **Display name** (decided for Richard): `application { display_name:
  "Acme" }`, plain text, required once any built-in email is on;
  `{{ app.display_name }}` in templates, `{app_name}` in Jadpo's wording;
  an app that needs it translated rewords Jadpo's messages.
- **Still to decide**: which roadmap chunk builds `sign_in:` (raised with
  Richard).

## Decided: overriding built-in behaviour

Decided 2026-10-16 (Richard), from [the scenarios](override-scenarios.md).

- **Two mechanisms only.** Typed settings decide how a built-in feature
  behaves; a setting's value is a plain value, a connection, one of the
  app's templates or types, or one of the app's actions (a decision point
  with a fixed signature, one handler). Events react after something
  happened, any number of subscribers, unable to change it. No general
  hooks or before/after callbacks.
- **A generated route or page is replaced** by switching it off with a
  setting and writing the app's own route over the same generated
  operations.
- **A built-in feature emits events only for what it does without the
  app's code.**
- **Feature flags stay in the app's database**, not a capability; a
  `Driver.ofrep` store (OpenFeature's remote evaluation), evaluation only,
  waits for an app that must use a vendor.
- **Audit entries can go out** as a built-in `AuditEntryRecorded` event,
  emitted only when something listens, through `publishing`; the database
  stays the record of truth.
- **Outgoing webhooks get one runtime-owned network exception** with fixed
  guards (HTTPS only, no private addresses, no redirects), listed once in
  the network report.

## Override scenarios (open item 6)

Decided 2026-10-16 (Richard), one scenario at a time.

**Files.** A `Driver.disk` store for production on the native target only,
the build manifest recording that the app then runs on one server; image
variants (`variants: { thumb: 200x200 }`) made by the runtime over any
store; virus scanning as a `scanning` capability (Jadpo itself refuses
infected uploads), filled by a ClamAV container package or a scanning API,
a file staying in a scanning state and unattachable until it passes, built
when an app needs it; media services (Cloudinary, Uploadcare) stay
ordinary packages, since they cannot do the whole `files` job. Still to
explore in depth: the disk driver and what it offers (Richard).

**Cache.** Upstash, Valkey, ElastiCache and other Redis-compatible
services use the Redis driver, their settings listed on its page, on
Workers too; `Driver.memory` is allowed in production with a warning (per
server, emptied on restart) and is an error on Workers; Cloudflare KV is
not a cache driver (a change can take a minute to reach every location),
so an app wanting it calls it as a package. Nothing is cached by default
(area 5). The cache holds only what is harmless to lose: cached records
and queries, route rate limits and, through the same
`rate_limits: { store: cache }`, provider rate limits shared across
servers; never idempotency keys, webhook de-duplication, sessions, poll
state, schedule leases or the outbox. Flag values, when flags are built,
are held in each server's memory and refreshed every few seconds.

**Search.** `search` becomes a capability with PostgreSQL full-text search
as the built-in default (`search: { fields: [...] store: products_index }`
naming an engine; left out, PostgreSQL); an engine's package fills it with
upsert, remove and query by words with equality filters. The engine
returns ranked ids filtered by tenant, and Jadpo reads the records through
the policy-applied query, so a result is never one the caller could not
read. Jadpo keeps the index in step after commit through the outbox,
erasure removes a person's records, and `jadpo search reindex` rebuilds it.
Engine-specific features (synonyms, ranking rules) are the package's own
operations. Development and tests use PostgreSQL search whatever
production uses.

**Rate limits.** As area 16 decided, plus: Cloudflare's rate-limiting
binding is a store on Workers for the limits it can count (short windows,
10 or 60 seconds, to check), and a limit it cannot count is an error
naming Redis as the alternative; a limited caller gets area 7's `429` with
`Retry-After` and the standard `RateLimit` headers, its wording from
translations like any failure (to add to area 7 if it does not say so).

**Generated routes; no pages** (Richard: Jadpo is a backend, so
frontends own every page). Jadpo generates endpoints only, never pages:
the magic link points at a path on the app's frontend
(`magic_link: { link: "/sign-in/confirm" }`, at the browser origin
already configured), which posts the token to
`POST /auth/magic-link/confirm`, so mail scanners loading the page consume
nothing; area 15's generated privacy page becomes endpoints only, the
frontend building the page (a recipe can show one), reversing area 15's
decision. A generated route takes a `path:` setting where its feature is
configured, or `routes: false`, after which the app writes its own route
over the generated operations (`magic_links.request`,
`magic_links.redeem`, `sessions.start`), which keep every protection;
webhook routes can change path but cannot be switched off; generated
response shapes are fixed. This removes the confirm page, its wording and
any page layout from the earlier decisions.

**Erasing a person at providers.** A provider package declares its erasure
operation in `provider.jadpo` (`erasure: { call: customers.delete  by:
CustomerId }`, area 15's package-level declaration, not a capability
mapping); the connection names the app field holding the key
(`erasure: { key: User.stripe_customer_id }`). Order: Jadpo copies each
key into the erasure event, the app erases the person in one transaction,
then a generated subscriber calls each provider after commit, retried
until it confirms; area 15's erasure-completed email goes only once every
provider has confirmed, and a delivery that gives up is parked for an
operator and shown, never quietly incomplete. A copied key that is
personal data (an email address) stays only in the outbox until delivered,
listed in the personal-data report. A connection that receives personal
data but cannot erase it is an error unless an acceptance states why the
data stays (tax records), listed in the personal-data report. The app may
add its own `PersonErased` subscribers.

**Bot protection.** `bot_check` becomes a capability (Jadpo checks it on
its own public endpoints: magic-link and privacy requests), filled by a
package (Turnstile, hCaptcha, reCAPTCHA) with one operation, check a token,
pass or fail; `bot_check: { connection: turnstile }` in `application { }`
and `bot_check: true` on a route or a generated feature. The frontend shows
the widget; the token travels in the `X-Bot-Check` header, never in the
route's input, checked before the action within the request's deadline. A
failed check is `bot_check_failed` (403), worded from translations;
reCAPTCHA's score gets a threshold setting on the connection. An
unreachable provider refuses the request (fails closed), and loosening that
needs an acceptance. Development and tests pass every check through a local
driver; a test can script `bot_check_failed`.

**Telemetry.** Its own section in `application { }`, not a connection,
its host still in the network report; one protocol (OpenTelemetry), no
vendor-specific code, a vendor being an endpoint and a header:

```text
telemetry: {
    endpoint: config.telemetry_endpoint
    credential: {
        header: "x-honeycomb-team"
        value: config.telemetry_key
    }
    send: {
        traces: true
        metrics: true
        logs: LogLevel.info
    }
    detail: {
        full_traces: [Trace.failures, Trace.faults, Trace.slow, Trace.new_paths]
        samples: {
            per_path: 1
            every: 1m
        }
    }
}
```

The standard `OTEL_*` variables fill these settings (area 16). Area 16's
rule stays: no values from the app's data unless added deliberately.
Custom metrics are declared in a domain's monitoring file
(`metric checkouts_completed = Counter { by: [plan] }`), recorded with
`telemetry.checkouts_completed.add(1, plan: ...)`; `telemetry.note(...)`
adds an attribute to the current trace; personal fields are refused as
labels or attributes. Jadpo's own data (path ids, stable identities,
source locations, build id) travels to every provider as `jadpo.`
attributes, nothing held back (Richard); a monitoring product decodes them
against the build manifest (area 18), which a deploy uploads once per
build. In development nothing is exported unless `jadpo dev
--export-telemetry`; the data stays local and is readable from the CLI as
the development call log is, filterable and watchable for agents and
developers (`jadpo traces`, `jadpo metrics`, `jadpo logs`, each with
filters by route, action, domain, path, failure and slowness, `--follow`,
and `--json`; `jadpo watch` and `jadpo trace` as decided), and later in the
dev console's UI.

**Failure format.** `failure_format:` per API in `apis:`, Jadpo's envelope
by default and `FailureFormat.problem_details` (RFC 9457) as the one
alternative, the content identical; OpenAPI describes the API's format,
the exported wording keys by `code` either way, statuses unchanged.

**Redis entities and running without a database.** An entity may live in
a Redis store and must expire: `expires_after: 2h`, measured from its last
write, expired records hidden as area 6 says (answering area 5 question
15); which queries a Redis entity supports beyond get, create, update and
delete by id is settled with area 6 (open item). No database is required
unless something uses one; area 5's implied PostgreSQL `main` becomes "only
when needed", and `jadpo check` is an error naming the feature that needs
one when none is declared (sign-in, events and the outbox, idempotency
keys, webhook de-duplication, flags, audit, any database entity).

## Receiving from outside (open item 7)

Decided 2026-10-16 (Richard).

- **Webhooks, end to end** (area 11 as decided): a connection with a
  webhook secret generates `POST /webhooks/<connection>`; the runtime
  checks the signature with a five-minute tolerance, drops repeats by the
  provider's event id and writes the event (public or not, as the package
  marks it) to the app's outbox, and subscribers run after commit. Every
  event carries the provider's `occurred_at`; out-of-order delivery is the
  app's to handle by ignoring an event older than what it stored, stated
  on the page with a recipe.
- **A sender with no package**: a system that signs webhooks gets the app's
  own integration in `shared/providers/` (the golden rule); a partner with
  an agreement is a `Service` with a service key calling an ordinary route
  (area 8).
- **Webhooks that need a reply** (Twilio's call instructions, Slack's
  messages): the package's event type names its reply type
  (`public type IncomingCall = Event { wire_name: "..."  reply: CallReply ... }`,
  the reply type in the package's `types/`); the connection names the
  app's action that answers, keyed by the event's lower_snake name
  (`webhooks: { replies: { incoming_call: phone.answer_call } }`), which
  runs inside the request after the signature check and may read and write
  like any action; repeats get the stored reply; the package encodes the
  reply (Twilio's XML waits for an XML byte format in a Jadpo release).
- **Handshakes**: built-in rules named in the package
  (`handshake: WebhookHandshake.sns`, `.slack`, `.microsoft_graph`),
  answered by the runtime, extended by releases.
- **Queues are read directly** through a `receiving` capability, the
  inbound twin of `publishing`. A queue names the field giving each
  message's type, the field giving its id, and the events it may carry
  (`receiving: { warehouse_queue: { type_at: "type"  id_at: "message_id"
  events: [Warehouse.OrderShipped, Warehouse.StockLow] } }`); a queue of
  one kind lists one event and no `type_at`. The events come from another
  Jadpo app's generated package or are declared by the app (a domain's
  `events/` or its own integration, each with its `wire_name`); the queue's
  package knows only how to fetch. A message whose type is not listed is
  parked for an operator, never dropped. Each message becomes its event for
  the app's subscribers, acknowledged only after the subscriber's transaction
  commits, repeats dropped by message id stored in the database, a message
  that keeps failing left to the queue's dead-letter queue and parked for
  an operator.
- **Which are packages**: by the usual rule, an HTTP API is a package and
  anything needing its own protocol or a platform binding is a built-in
  driver. Amazon SQS and Google Pub/Sub (pull) are packages; SNS and
  Pub/Sub (push) are webhooks through a package with a handshake;
  Cloudflare Queues on Workers is a built-in driver (a binding, Workers
  only) and from a native server a package (its HTTP pull API); Kafka is a
  built-in driver in a later release. On Workers the queue consumer is
  generated into the Wrangler configuration.

## Acting for each user: OAuth grants (open item 7c)

Decided 2026-10-16 (Richard). For apps acting on another service for each
user or organisation (a user's Google Calendar, an organisation's Xero, a
Slack workspace install).

- **A connection's credential may be a grant owned by one of the app's
  records**: `credential: OAuthGrant { owner: User  client_id:  client_secret:
  scopes: [...]  return_to: "/settings/integrations" }`; the owner is `User`
  or any entity (`Organisation`). The provider's facts (authorize, token and
  revoke addresses) are `oauth:` in the package's `provider.jadpo`, and each
  operation declares the scope it needs. Nothing is specific to one
  provider; the connection's name is the app's.
- **Connecting is endpoints, not pages**: `POST /connections/<name>/start`
  for the signed-in caller returns the provider's consent address and keeps
  the `state` on the server against the caller's session or API token; the
  provider returns to `GET /connections/<name>/callback`, which needs no
  cookie, exchanges the code (PKCE), stores the grant and redirects to the
  frontend's `return_to` with success or failure. API-token clients connect
  the same way.
- **Grants are held by the runtime**, like sessions, in a `jadpo_grants`
  system table in the owner entity's store (so grants need a database),
  their tokens encrypted with `application { grants: { encryption_key:
  config.grant_encryption_key } }`, a `SigningKey`-like secret with the
  decided previous-key rotation, and never readable by code.
- **Every call names whose grant it uses**: `on_behalf_of: owner_id`;
  `grant_missing` and `grant_revoked` are failures the app maps.
- **Refresh is automatic**: renewed before expiry, once when calls race,
  rotated refresh tokens kept; a revoked grant gives `grant_revoked` and a
  `GrantRevoked` event so the app can ask the owner to reconnect.
- **Scopes are checked at compile time**: calling an operation whose scope
  the connection does not request is an error.
- **Policy decides who may connect** for an owner (area 9): connecting
  needs the `update` grant on the owner's record, so no new effect.
- **Disconnecting** is `POST /connections/<name>/disconnect` and its
  operation, revoking at the
  provider when the package declares how; erasing a person or record (area
  15) deletes and revokes its grants.
- **Development and tests**: calls faked by default (decided);
  `jadpo dev grants add <connection> --owner <id>` makes a fake grant;
  tests declare grants as fixtures.
- **Later, each when an app needs it**: several accounts per owner,
  incremental scopes, provider notifications per grant, GitHub App
  installation tokens (a credential scheme in the runtime).

## Redis entities (open item 7d)

Decided 2026-10-16 (Richard). A Redis entity supports: get, create, update
and delete by `id`; lookup by a `unique:` field (Jadpo keeps a key per
unique field); listing by equality on one field declared in `indexes:`
(a set per value, pages in creation order) and counting by it; required
`expires_after:`, expired records hidden; concurrent-change checks through
a revision field, as in area 3. Policy grants that are equality on the
record's own fields are checked after reading. Joins and `include`,
ranges, ordering by other fields, `matches`, and grants beyond simple
equality are compile errors naming the reason and PostgreSQL as the
alternative.

## What a library package may also carry (open item 8)

Decided 2026-10-16 (Richard).

- **No emails in packages**: an `emails/` folder in a package is a compile
  error; email templates, partials and layouts are the app's own. (The
  golden rule runs one way, so a package doing less than the app is
  consistent.)
- **Translations**, as decided under Translations, for a package's own
  failures and messages.
- **A usage guide**: one optional `GUIDE.md` in the package root saying how
  to use the package well; every code example in it is compiled and run at
  publish; `jadpo package show <name> --guide` prints it and the surface
  points to it.
- **Recipes in library packages too**, not only provider packages.

## Documents (open item 9)

Decided 2026-10-16 (Richard). A `documents` capability renders a typed
template to HTML or PDF, exactly as email renders: `type InvoiceDocument =
Document { ... }` with its `.html` template in the domain's `documents/`
folder, the strict template rules, translations, and an optional
`shared/documents/layout.html`. `documents: { connection: pdfs  layout:
... }` names the connection that turns HTML into PDF: Gotenberg as a
container package or a hosted API's package (DocRaptor, PDFShift).
`documents.pdf(InvoiceDocument { ... }, locale: ...)` returns `Bytes`;
`documents.html(...)` returns the HTML for previews. It runs in
subscribers, capped by `max_concurrent`. In development `jadpo dev` starts
the declared service; in tests the fake returns a placeholder PDF and a
test can read the rendered HTML. The disk driver is not decided yet: file
reading, writing, very large files and parsing CSV, TSV and JSON are being
researched first (Richard).

## Packages from other Jadpo apps (open item 10)

Decided 2026-10-16 (Richard). For one Jadpo app calling another's API.

- **Jadpo's OpenAPI documents carry `x-jadpo` fields** saying what plain
  OpenAPI lacks: which operations change something, idempotency, each
  failure's code and kind, constraints. Other tools ignore them.
- **The OpenAPI importer uses them whenever it finds them**, so importing a
  Jadpo app's OpenAPI gives an exact package needing no corrections or
  recordings; a provider that is not Jadpo is imported as before. There is
  one importer and one format; nothing is special about the other app
  being Jadpo except those fields.
- **Every API serves its OpenAPI document** at `<prefix>/openapi.json` by
  default, under the API's own default authentication (an admin API's
  document needs sign-in); `serve_openapi: false` turns it off. The
  served route appears in the API's own OpenAPI document and in its API
  record (`api/main.api`), marked generated, like every generated route
  (Richard).
- **Getting it**: in one repository `jadpo package add ../orders` reads the
  other app's generated OpenAPI; across repositories the other app
  publishes its package to the company's private registry (item 11); for
  another company's public API, `jadpo package add
  https://api.orders.example/v1` fetches the served document. Each is read
  once, when the package is added or updated, then pinned in the lock with
  its source and digest, never fetched during a build.
- **The result is an ordinary installed provider package** in
  `packages/<name>/<version>/` with no fixed host (the connection gives
  `host:`) and a service-key credential (area 8); at runtime the two apps
  meet only over HTTP, and the called app cannot tell what made the call.
- **Versions**: an additive change to the other API regenerates as a minor
  version through `jadpo package update`, `jadpo check` saying when it is
  out of date; a breaking change (already needing sign-off in that app,
  area 7) becomes a new major version, side by side while callers move.
- **Breaks across apps**: in one repository, `jadpo check` in the called
  app reads the other apps' locks and reports which of their calls a
  change breaks, before it merges. Built later.

## Multi-step provider calls (open item 7b)

Decided 2026-10-16 (Richard), after the edge-case review in
[multi-step calls review](multi-step-calls-review.md): adopted with all ten
of its must-fix changes and its open questions answered as it recommends.
In short: each step's operation and a fingerprint of its request are
recorded before it is sent; a rerun checks the whole path against the
record and stops as `diverged` for an operator on any difference;
`clock.now` is fixed for the life of a record; provider keys are derived
from the app, environment, caller and step (fixing a collision that
affects single calls too); packages declare their providers' key windows
and errors, and a step whose answer never arrived is not resent after the
window; multi-step actions run only where the key survives a retry
(subscribers, and routes that require `Idempotency-Key`), and a route
without a required key starting one is a compile error (it answers 202 and
a subscriber does the steps); no outside calls inside `for`; one runner per
record; an unfinished run becomes an operator item listing what took
effect, with retry and abandon; records are deleted in the commit,
encrypted, capped in size and listed in the personal-data report. Calls
without idempotency keys stay one per action.

## Files and formats (open item 9)

Decided 2026-10-16 (Richard), from [files and large data](files-and-large-data.md):
storage and formats are separate layers. Storage (`store.read`,
`store.put`) moves bytes in and out of S3, R2 or disk and knows nothing of
what is inside; formats (`csv`, with TSV as its tab-delimited form, `json`,
`json_lines`) are pure standard-library namespaces turning bytes into the
app's types and back, whatever the bytes came from (a file, a request body,
a provider's answer, a queue message); the app's code joins them, and the
large-file `each:` form joins them piece by piece. The format namespaces
are decided on chunk 5.4's page.

**One way to read and write row data, any size** (Richard: every helper
handles small and large files alike). Explicit helpers per format and
direction, all running on one shared machinery, batches:

```text
var import = csv.read(input.file
    as: ProductRow
    each: row -> save_product(row, account: input.account_id)
    finished: summary -> import_finished(summary, account: input.account_id)
    key: input.file.id
)

var export = csv.write(exports
    name: "products.csv"
    format: CsvFormat.spreadsheet
    from: Product.where(Product.account_id == account)
    each: product -> ProductExportRow { sku: product.sku, name: product.name, price: product.price }
    finished: done -> send_export_ready(done.file, to: user.email)
    key: request_id
)
```

- Helpers: `csv.read`/`csv.write`, `tsv.read`/`tsv.write`,
  `json_lines.read`/`json_lines.write`, `json.read(file, items_at:, as:)`/
  `json.write` for a large JSON list, and `batches.each(from:, each:,
  finished:, key:)` for work with no file (a large query, every page of a
  provider's list). `each:` is the inline form area 3 allows for standard
  operations: without a destination it calls an action per item; when
  writing it returns the row to write.
- All start after the calling action commits, like `emit`, run in the
  background and return a run: `batches.status(id)` gives progress and
  problems, `batches.cancel(id)` stops it; `key:` makes a repeated start
  return the same run. Underneath: items grouped into batches, each one
  transaction committing its writes and its position together, so a crash
  or deploy resumes at the next item; file order kept; a row that cannot be
  decoded, or an item whose action fails, becomes a problem (row as a
  spreadsheet numbers it, column, rule, never the value) and the rest carry
  on; values `each:` captures are saved with the run, and only storable
  values may be captured.
- No other way: no loop over a file's rows and no whole-file parse into a
  list. `store.read(file)` returns a file-contents type for passing bytes on
  (to a PDF service, as an attachment) that the format helpers do not
  accept; a provider's CSV report is put in a store and read the same way;
  `json.decode` stays only for single JSON documents in memory (request
  bodies, provider answers).
- From the research (questions 2 to 10, as recommended): rows read are
  `Input`s and rows written `Output`s, with `column:` and the decided
  `wire:` encodings; writing CSV names its reader (`CsvFormat.spreadsheet`:
  byte-order mark, CRLF, formula escaping; `CsvFormat.standard`); read
  UTF-8, UTF-16 with a mark and declared legacy encodings, never guess,
  write UTF-8; gzip only, with a required `max_unpacked_size:`, no zip;
  uploads over 100 MB through a multipart slot, the same on S3, R2 and
  disk; other number and date conventions through `wire:` and `decode:`;
  the batches machinery replaces chained events for paging (area 12's
  16-deep limit), settling area 11's pagination.
- **The disk driver** as specified in the research: native only, one
  server; files immutable under generated sharded keys; uploads streamed to
  a temporary file, then synced and renamed; downloads served with ranges,
  ETags, `attachment` and a sandbox policy; `storage_full` and a free-space
  floor; `jadpo files copy` between stores.

## Developing and publishing packages, privately too (open items 10b and 11)

Decided 2026-10-16 (Richard).

- **A package lives in its own repository**: `jadpo package new <Name>
  --library` or `--provider` writes the package format plus `recipes/`, a
  `sample/` app the recipes and tests run against, and `GUIDE.md`.
- **Developing**: `jadpo test` runs the package's tests, the examples in
  its descriptions, its decoders against recordings and its recipes
  compiled into `sample/`; `jadpo package record` and `conform` work
  against the provider's sandbox; `jadpo package check` runs the whole
  publishing bar locally, writes `surface.jadpo_record`, and compares it with
  the surface at the previous tag, refusing a breaking change without a
  new major version.
- **Trying it in an app before publishing**: `jadpo package link
  ../acme-pricing` compiles the app against the live checkout; the lock
  records the link; `jadpo build` for production and CI refuse a link, so
  it can never ship; `jadpo package unlink` restores the installed version.
- **Private packages come from git**: `jadpo package add
  github.com/acme/acme-pricing@v2.1.0`, a version being a git tag the
  package's CI tags only after `jadpo package check` passes; access is the
  developer's or CI's own git access, with no `jadpo login` and no private
  registry (packages are committed into apps, so a registry's caching adds
  nothing; Go is the precedent); the lock pins the repository, the tag's
  commit and the digest, so a public package can never stand in for it.
  The lock's source field leaves room for a registry later.
- **Every package is verified at install, wherever it came from**:
  `jadpo package add` and `update` regenerate the surface from the
  package's code and refuse one that differs from the committed
  `surface.jadpo_record`. The public registry repeats the checks on upload and
  adds discovery, curated packages, publisher verification, signing and a
  documentation website.
- **Prose is unverified wherever it comes from** (`///` descriptions,
  `GUIDE.md`); nothing safety-relevant relies on it.
- **Publisher trust**: the app's own company's packages from git are
  installed as any; a third party's package from git, not the registry, is
  marked unverified in the approval report and needs a person's acceptance
  to install; registry packages carry their verified publisher, curated
  ones marked official.

## Generated files and deploy-time choice (open items 10c and 12)

Decided 2026-10-16 (Richard).

- **Committed generated files keep their homes, next to what they
  describe, and share one extension, `.jadpo_record`**: `api/main.jadpo_record`
  (was `api/main.api`), `database/identities.jadpo_record` (was
  `identities.json`), each `translations/sources.jadpo_record` (was
  `sources.json`), and each installed package's `surface.jadpo_record` (was
  `surface.jadpo_record`); `jadpo.lock` keeps its name. Each is in Jadpo's
  readable format, not JSON, and starts with a line saying what generated
  it and which command updates it; `jadpo check` regenerates it and a hand
  edit is an error naming that command. People-written files
  (acceptances) are not records. `jadpo records` lists every record and
  flags any out of date; area 18's table stays the single index.
- **Choosing a provider when deploying** is designed now and built when the
  first app other people run needs it: the app declares every connection it
  supports and a setting picks one at start (`email: { connection:
  config.mail_provider }`); every candidate is compiled, checked and
  reported, and an unchosen one needs no credential.

## Decided: the golden rule

Decided 2026-10-16 (Richard): **a package can do nothing the app cannot,
and the app can do everything a package can.** A package is a folder of an
app's own code, published.

- It holds by construction: the app's own integrations in
  `shared/providers/` use the package format whole (`provider.jadpo`,
  `resources/`, `events/`, decoding, `provides/`, `runs:`), and its own
  types, failures, libraries, emails and translations live in `shared/`
  and its domains.
- The compiler treats `packages/` and `shared/providers/` alike; they
  differ only in provenance: the lock entry, the digest check, being
  read-only, and the publishing bar.
- Every future package feature is designed with how the app writes it; a
  proposal that only works inside a package is rejected.
- The gate checks it: each package example in the language pages also
  compiles placed as the app's own integration in `shared/providers/`.
- The rule runs one way only: an app can do far more than a package
  (entities, actions, routes, policy).

## Decided: which kinds of package exist

| Kind | Contains | Used as |
|---|---|---|
| Provider | One API at one version: resources, operations, types, failures, events, hosts; optionally `provide` mappings and library functions | A connection: `stripe.payment_intents.create(...)` (area 11) |
| Library | Types, failures and library functions under `shared/`'s rules: pure, no entities, no routes, no network | `use Postcodes`, then `Postcodes.uk.parse(text)`, `Postcodes.UkPostcode` |

- **No feature packages.** A whole slice of an app (invitations, billing)
  is not a package, installed or copied in. An agent writes it from the
  language pages, the compiler catches the dangerous mistakes, and most
  features other frameworks take from packages (sign-in, flags, audit,
  files, email) are built in. Where agents keep getting one wrong (chunk
  10.2 measures this), the fix is a worked reference in `examples/`.
- **No driver packages.** Anything that needs native code (a new
  database, a signing scheme, a wire format) comes only through a Jadpo
  release. Packages never carry native or WebAssembly code.
- **Library types may be stored** in entity fields and used in route
  inputs and outputs, unlike provider types. An upgrade that changes a
  stored type appears in the upgrade plan and the migration, as a change
  to the app's own type does.
- **One package format; the kind follows from what is inside.** A package
  with `resources/` is a provider package and needs a connection to call
  its operations; one without is a library package. A provider package's
  library functions are callable without a connection.
- **Everything in a provider package follows the provider rules**, its
  library types included: only ids and plain values may be stored,
  because a provider package is versioned by the API's version and the
  next version is a new package. What an app keeps from a provider it
  stores as ids, plain values and built-in types (money, countries, phone
  numbers), or as its own types.

### Names

- **Provider brand names are reserved**: Jadpo ships the list of curated
  provider names, and an app's own integration cannot take one, so `Stripe` in
  code always means the official package.
- **Two packages never share a name in code.** Installing a registry
  package whose name one of the app's own integrations or libraries
  already uses stops and asks for the app's own to be renamed first
  (`jadpo rename`).
- Creating an integration checks the registry and warns when the name is
  taken.

## Agreed: built in or a library package

Input to chunk 5.4 (the standard-library plan), which decides it. A thing
is built in when the compiler gains from knowing it: a type that turns a
mistake into a compile error, carries personal-data marking, has a SQL or
OpenAPI form or generated test values, or a security boundary the language
should own. Saving an install is not enough.

| Thing | Where | Why |
|---|---|---|
| Money and currency | Built in | Mixing currencies is a compile error; rounding fixed; one column form |
| Countries, currencies, languages | Built in | Small, slow to change, needed by money and phone numbers |
| Phone numbers | Built in | Already carried to area 2 for SMS; always personal data |
| HTML cleaning and Markdown, as a safe-HTML type | Built in | A security boundary; the parser Jadpo is worst at writing |
| CSV | Built in | Already in 5.4's encodings |
| Postcodes, IBAN, VAT numbers, ISBN | Package | A long tail of per-country rules |
| Public holidays, business days | Package | Data that changes yearly, per country |
| Repeating dates, `.ics`, QR codes | Package | Niche; can move into the standard library later |

Built-in data ships with Jadpo releases, as time-zone data does in every
language. Public library packages are then the long tail, and the largest
use is likely private: types and rules shared between one company's own
apps. Open for 5.4: where fixed reference data (a country table, phone
formats) can be declared at all, since the language has no way to declare
a table of values today.

## Decided: the layout inside a package

A package is laid out as `shared/` is, so one set of rules covers the app
and every package, and the compiler checks a package as it checks
`shared/`. A library package:

```text
postcodes/v2/
    package.jadpo                   the index: description, licence, Jadpo version
    types/uk_postcode.jadpo         public type UkPostcode = Text { ... }
    failures/invalid_postcode.jadpo
    libraries/uk.jadpo              public library uk { parse(...)  normalise(...) }
    libraries/uk.test.jadpo         tests beside the file, as in an app
    CHANGELOG.md
    surface.jadpo_record                 generated
```

A provider package adds `provider.jadpo`, `resources/`, `events/`,
`recordings/` and, when it fills a capability, `provides/`:

```text
stripe/v2024_06_20/
    package.jadpo                          the same three keys as every package
    provider.jadpo                         how the API is reached and called
    resources/payment_intents.jadpo        imported, corrections merged in
    types/payment_intent.jadpo             imported
    failures/card_declined.jadpo
    events/payment_intent_succeeded.jadpo
    libraries/tables.jadpo                 written: decoders and translation helpers
    libraries/tables.test.jadpo
    recordings/payment_intents.create.json real responses that confirm the types
    surface.jadpo_record
```

- **`public` marks the package's API**, as it marks what a domain offers
  other domains. Everything else is internal and may change freely.
- **The index lists no files**: a file is named after what it declares,
  so the compiler finds a resource by name and loads only what the app
  uses.
- **A re-import rewrites only imported files.** Written libraries and
  corrections survive, as area 11 already says for corrections.
- **Everything reachable from the public surface is public** (decided
  2026-10-16, Richard): a failure a public library's function `fails` with, a
  provider operation's `failures:`, and the types in a public signature
  must themselves be public, checked on every compile; internal failures
  are mapped or handled before they leave a public library. `public` goes
  on a library, never on a function, as in area 1, and internal helpers
  (decoders) live in a library that is not public. The same
  rule applies to a domain's public actions, queries and libraries, which
  area 1 does not state today.
- **A package's tests run when it is published**, not in every app's
  `jadpo test`; the digest guarantees the code is the code that passed.

## Decided: the index, `provider.jadpo`, and drivers in `app.jadpo`

Decided 2026-10-16 (Richard), from
[the layout comparison](package-layout-options.md), its candidate B.
Presets as packages are dropped: every case that made the index messy was a
package configuring a built-in driver.

- **Every index is the same**: `package Name.vN { }` with a `///`
  description and two keys, `licence:` and `jadpo:` (the lowest version
  it needs). There is no `requires:`: packages have no dependencies
  (below).
- **A package that describes an API has one `provider.jadpo`**, a
  `provider { }` declaration with one fixed set of keys: `protocol:`
  (`Protocol.json_http`, `.form_http`, `.multipart_http`, `.aws_json`),
  `hosts:` (`live:`, `sandbox:`; the same host twice when the sandbox is a
  test credential), `runs:` (a container, below), `credential:`,
  `idempotency:` (left out when the API takes no key), `error_code:`,
  `webhooks:` (`check:`, `event_id:`, `event_type:`), `settings:` (typed
  values the app gives) and `imported:`. A package without one is a
  library. One package describes at most one API, because one API is one
  connection with one set of hosts and credentials.
- **A capability mapping is a file**, `provides/<capability>.jadpo`, found by
  name; area 11's `provides:` key in the index goes.
- **Where an API is comes from exactly one place**: `hosts:` (on the
  internet), `runs:` (Jadpo runs it), or neither, when the app gives
  `host:` on the connection (another Jadpo app, a self-hosted product, a
  service on Workers). A connection's `host:` beside `runs:` means it is
  already running there; a package with fixed `hosts:` refuses one.
- **Anything Jadpo has a built-in driver for is configured in `app.jadpo`,
  never packaged**: PostgreSQL, Redis, S3, SMTP, OpenID Connect. Cloudflare
  R2 is `driver: Driver.s3` with its host and `region: "auto"`; Neon is the
  PostgreSQL driver with its pooling mode; Gmail is the SMTP driver with
  its host and port. A vendor with storage and an API is a driver
  connection plus a package, so there is no hybrid case.
- **Vendor settings are documented, so agents look them up**: each
  driver's language page lists the settings for the common services (R2,
  Backblaze B2, DigitalOcean Spaces, MinIO, Amazon S3; Neon, Supabase,
  Amazon RDS; Gmail, Microsoft 365, Amazon SES SMTP; Google, Microsoft),
  each a working connection checked with the pages' other examples, and
  `jadpo check` warns when it recognises a host that is missing one. A
  vendor driver in a Jadpo release only where a flow needs native code.
- **Sign-in**: Google, Microsoft and other OpenID Connect providers are
  `Driver.oidc` connections (`issuer:`, `client_id:`, `credential:`); GitHub
  and Apple, which are not plain OpenID Connect, are the built-in
  `Driver.github` and `Driver.apple`;
  `sign_in: { magic_link: { ... }  providers: [google, github] }` names
  them.
- **A vendor with two APIs under different credentials** (Shopify's Admin
  and Storefront APIs) is two packages.
- **The declaration is called `provider`**, matching provider packages and
  `shared/providers/`.

```text
// app.jadpo
avatars: {
    driver: Driver.s3
    host: config.r2_host
    region: "auto"
    bucket: "avatars"
    credential: {
        key_id: config.r2_key_id
        secret: config.r2_secret
    }
}
stripe: {
    package: Stripe.v2024_06_20
    domains: [payments]
    credential: config.stripe_secret_key
}
```

This changes area 11 question 19 ("files: as S3 presets"), question 21's
`provides:` key, question 25's test-credential sandbox, area 16's R2
example and area 8 question 11's presets.

## Decided: where packages live, and where your own code lives

One rule, by who wrote the code rather than what kind it is:
**`packages/` holds only what was installed. Everything the app's authors
wrote lives where their code already lives, whatever kind it is.**

```text
app.jadpo
domains/
    shipping/libraries/...           the app's helpers for one domain (as now)
shared/
    libraries/...                    the app's helpers for several domains (as now)
    providers/
        acme_couriers/v1/            the app's own integration, in the package format
packages/                            installed only: in jadpo.lock, read-only
    stripe/v2024_06_20/
    postcodes/v2/
jadpo.lock
```

- **Installed packages are committed** under `packages/<name>/<version>/`.
  This changes where area 11 put registry packages (a cache on the
  developer's machine); the pin by digest and "never fetched during a
  build" stand. Agents read them with the tools they already use,
  reviewers see an upgrade as an ordinary diff, and CI needs no fetch
  step.
- **Installed packages are read-only.** The compiler refuses one whose
  files do not match its digest in `jadpo.lock`.
- **The app's own integrations live in `shared/providers/<name>/<version>/`**,
  in exactly the package format, with everything an installed provider can
  have: types, failures, pure functions, decoding and encoding, recordings,
  fakes and `provide` mappings. Who may call one is still the connection's
  `domains:` list. This adds `providers/` to what area 1 allows in
  `shared/` (today pure types, failures and libraries only).
- **A `use` line shows where a name comes from**, as area 1 already
  requires: `use packages/…` is always installed code; anything else is
  the app's own.
- **Installed or the app's own, the only differences are where it sits,
  whether it can be edited, and whether the publishing bar applies.** The
  compiler treats both the same, so replacing an own integration with an
  official package later changes `jadpo.lock`, the folder and the `use`
  lines, not the code that calls it.
- **Publishing moves code out of the app.** A package other apps depend on
  gets its own repository and versions, and every app installs it into
  `packages/`, the one it started in included.
- Two versions can sit side by side: a provider during an upgrade (area
  11 question 3), a candidate version while the upgrade plan compiles the
  app against it, or two library versions while callers move across.
  While two are in use, names carry the version
  (`Postcodes.v3.UkPostcode`), as area 11 question 16 decided for
  providers.
- The commands are `jadpo package …` (`add`, `import`, `record`,
  `conform`, `update`, `remove`, `show`, `publish`), replacing area 11's
  `jadpo provider …`.

## Decided: the package surface

Richard's idea. The compiler works out a package's public surface and
writes it to `surface.jadpo_record`, beside `package.jadpo`: the package's
description, licence and version, every public type with its fields and
constraints, every failure with its kind and code, every public library's
functions with their signatures, failures and descriptions, and for a provider its operations
(wire and output types both), hosts or container, credential scheme,
settings and the capabilities it provides.

- **Readable, in the API record's format** (area 7's `api/main.api`), not
  JSON (decided 2026-10-16, Richard). Installed packages are committed, so
  every upgrade's pull request carries the surface diff, and a reviewer
  reads "removed `bookings.cancel`, new failure `PostcodeNotServed`" in
  plain text.
- **Every entry points at its file** (`types/booking.jadpo`), so an agent
  reads the surface first and opens only what it needs. A file is named
  after what it declares, so the pointer never goes stale.
- **Kept out of `package.jadpo`**, which a person writes; the descriptions
  stay in the source as `///` comments, where the author edits them.
- **`jadpo package show <name>`** prints the description, licence,
  version, kind and public surface, and `jadpo package show <name>
  <declaration>` one operation, type or function with its description and
  file; `--json` gives tools and agents the same. It works for installed
  packages and an app's own integrations alike, and its screens follow the
  CLI design language and are designed before they are built.
- **One source for documentation.** The registry renders every package's
  pages from its surface, so every package is documented the same way.
- **Generated, never written.** `jadpo package publish` writes it, and the
  registry refuses a surface that does not match the code. The surface
  of an app's own integration is generated into `build/`.
- **It decides the version.** Publishing compares the new surface with the
  last release's and refuses a breaking change (a public name removed, a
  field retyped, a new failure) without a new major version, as Elm's
  package manager does. The upgrade plan diffs the same two files.

## Decided: what publishing requires

`jadpo package publish` refuses a package unless:

1. it compiles with no errors and no warnings;
2. every public type, failure, library and library function has a `///`
   description;
3. every function of a public library has at least one example in its
   description,
   compiled and run at publish, and every failure it can raise is shown by
   an example or a test;
4. its tests pass, and every decoder decodes every recorded response;
5. its version agrees with the surface diff;
6. its changelog's list of changes is generated from the surface diff, and
   the author writes only why;
7. it declares a licence from a fixed list and the lowest Jadpo version it
   needs.

The app's own integrations and libraries skip the bar; it applies only to
what is published. Descriptions alone
are not the bar, because the compiler cannot judge whether one says
anything; examples are, because they must run.

## Decided: what package code may do

Decided 2026-10-16 (Richard), replacing "no code with side effects in
packages": the point of a package is expertise written once by someone
who knows the API and tested properly, so a provider package may act on
its own API, but never on the app's data.

- **No state and no database**: a package owns no entities or tables and
  never reads or writes the app's data. Data a package fetches reaches the
  app as a return value or an event, and the app's own action writes it.
- **Package actions call only their own API**: a provider package may
  contain `public action`s in `actions/` that call its own operations in
  any order, pass answers between them and use its pure functions, and
  return a result or fail. They cannot emit app events, send email, call
  any other connection, or read configuration or secrets. The app calls
  one as `connection.action(input)` (`stripe.start_subscription(...)`),
  inside its own action; a package action with several changing calls
  needs the multi-step mechanism (open item 7b). The surface lists each
  package action with the operations it calls and which change something.
- **Package subscribers on its own webhooks**: a package may subscribe to
  its own webhook events, call its own API to enrich them, and emit its
  own declared events (`Stripe.SubscriptionRenewed`).
- **Events only about the outside world**: a package emits events for
  webhooks, enriched webhooks and polls, never as a side effect of the
  app calling it; the app's action emits its own event there, where a
  reviewer sees it.
- **Polling turns APIs without webhooks into events**: `polls:` in
  `provider.jadpo` names an operation, a default interval and the event
  emitted when the answer changes; the runtime runs it, keeps what it last
  saw (runtime state, like webhook de-duplication) and emits only changes;
  a poll runs only while something subscribes to its event; the connection
  can change the interval (`polls: { latest_rates: 6h }`).
- **Services**: a package's own API may be a container it runs (`runs:`),
  which its actions call; it cannot call another package or connection
  (no dependencies). **A package's service never starts until approved**
  (Richard): installing shows the image, its digest and the hosts it asks
  for; the app names it on the connection, and a new or changed image
  needs a person's acceptance in the approval report before it runs, so an
  upgrade cannot swap in a different image unseen.
- **Recipes**: a package may ship `recipes/`, complete wiring examples
  (schedules, actions, entities) copied into a domain with
  `jadpo package recipe <package> <recipe> --into <domain>`, after which
  they are the app's code; each is compiled and tested against a sample app
  at publish and listed in the surface.
- **`public` marks what the app can call** (decided 2026-10-16): a
  `public resource` and a `public action` are callable through the
  connection; everything else only by the package's own actions,
  subscribers and polls. An importer marks every resource public. A
  package's resources and actions share the connection's namespace, so a
  clash is an error in the package.
- **Package code runs only when started**: by the app calling it, or by
  the outside world through a webhook or a poll the app subscribes to.
- **An action that emits cannot be `public`**, so the app can never trigger
  a package's events by calling it; only the package's subscribers and
  polls run it.
- **Package events are public or not**: a `public` event can be subscribed
  to by the app; one without `public` reaches only the package's own
  subscribers (a raw webhook a package enriches can stay internal, its
  enriched event public). An importer marks webhook events public;
  subscribing to a package's non-public event is an error. Package events
  travel through the app's outbox like any other.
- **Polls are for feeds that do not depend on the app's data** (rates, a
  price list); checking each of the app's own records is the app's
  schedule calling the package, given as a recipe.
- **`max_concurrent:`** on a provider caps how many calls run at once per
  app instance, the connection able to lower it; further calls wait, and
  waiting subscriber deliveries retry later. Heavy work such as PDF
  rendering runs in subscribers, never in the route.
- **Full permissions are not built**: a package that owns tables or runs
  its own schedules waits until a real app needs package-owned data;
  adding grants later breaks nothing.

## Decided: decoding and translation

Two kinds of conversion, split by one test: is it needed to make the value
mean what the provider's documentation says it is?

- **Decoding** (yes): dates as Unix seconds, numbers as text, lists as
  `"a,b,c"`, an envelope around the real object, an error inside a `200`,
  a columnar table that is really a list of rows, Stripe's
  `amount: 1250, currency: "gbp"` that is really money. **The package
  declares it and the runtime does it**, so the app only ever sees
  correctly typed values.
- **Translation** (no): turning a correct value into the shape one app
  prefers. **The app's action does it, with an explicit call.**

Decoding is declared on fields and on operations (syntax illustrative):

```text
type Booking = Object {
    collect_at: Instant {
        wire: Wire.unix_seconds
    }
    weight: Decimal {
        scale: 2
        wire: Wire.text
    }
    slot: SlotWindow {
        decode: slots.parse_window
        encode: slots.format_window
    }
}

list_rows: {
    method: HttpMethod.get
    path: "/v1/reports/{id}/rows"
    wire_output: ColumnTable
    output: List<ReportRow>
    decode: tables.to_rows
    output_at: "data"
    failed_when: { path: "ok"  equals: false }
}
```

- **Built-in encodings** for the common cases, a fixed list extended by
  Jadpo releases: Unix seconds and milliseconds, ISO dates, numbers as
  text, joined lists, base64, JSON inside a string, money in minor units,
  envelopes, errors inside a `200`.
- **Pure decode and encode functions** from the package's own
  `libraries/` for anything else, on a field or a whole request or
  response. A field's pair is checked to round-trip at publish; a
  whole-response decoder is run over every recorded response.
- **Byte formats stay in the runtime**: JSON, form-encoded, multipart, XML
  later. Package functions reshape values, never bytes.
- **Decoding is lazy where it can be.** A field decoder runs only when code
  reads the field. A whole-response decoder reads what it reads, so "a
  provider adding fields breaks nothing" holds for fields it ignores.
- **A value that will not decode is a fault** naming the operation and
  field, as a field that does not match its type is today; contract runs
  against the sandbox catch it before production does.
- **App tests script the clean type**, never the wire shape; the package's
  own tests and recordings cover its decoders. Webhook payloads decode the
  same way.
- **The importer cannot see most of this** (OpenAPI often says only
  `integer` for Unix seconds), so the agent adds it with the other
  corrections and `jadpo package record` confirms it against a real
  response.
- The compiler cannot police the line between decoding and translation; the
  surface shows both the wire and the output type, so the published
  documentation and the review make it visible.

## Decided: packages have no dependencies

Decided 2026-10-16 (Richard). A package cannot require another package.

- **The standard library carries the shared vocabulary.** Money,
  countries, currencies and phone numbers are built in (the input to chunk
  5.4), which is what packages elsewhere mostly depend on each other for.
- **What is left is copied.** A package that needs another's code includes
  a copy, checked and tested by the publishing bar like the rest of it. Two
  packages for one vendor (Shopify's Admin and Storefront APIs) each have
  their own types, generated from the vendor's description anyway.
- **It sends effort to the root package.** Npm is full of packages that
  wrap other packages; with no wrapping possible, improvements go into the
  package itself, or into the app that needs them.
- **It removes** version resolution, clashes between packages, "required
  by" tracking and clean-up on removal. Installing is one folder and one
  line in the lock.
- **It is the reversible choice**: adding requirements later breaks
  nothing published, and is done only if real packages keep copying
  something substantial that does not belong in the standard library.
- The cost: two packages' types for the same thing are converted at the
  call, usually in one line.

## Proposed: private packages

A company's own apps sharing types and rules is likely the largest use of
library packages. Decided so far: such a package lives in its own
repository and every app installs it, the one it started in included. Open:
where apps install it from (a private registry, or a git repository the
company controls) and whether the publishing bar applies in full.

## Proposed: the model and its rules

Four layers, each extended in exactly one way:

| Layer | Holds | Extended by |
|---|---|---|
| Runtime | Drivers, byte formats, built-in encodings, credential and signature schemes, schedule ticks | A Jadpo release |
| Capabilities | The swap points | A language change through review |
| Packages | Provider and library packages: Jadpo source | `jadpo package add`, or the app's own in `shared/providers/` |
| Wiring | `connections:` and capability settings in `app.jadpo` | The app |

The rules every chunk would follow:

1. **Nothing is ambient.** Every backend is a named connection, and each
   feature names the connection it uses. Turning a feature off is removing
   its wiring; replacing a provider is changing a name.
2. **Native code only comes from a Jadpo release**, and a package reaches
   the network only through connections the app declares.
3. **A swap point is a capability**; packages fill capabilities and never
   invent them.
4. **Extend by events, override by name.** Built-in features offer events
   and settings; an app overrides a template or translation by writing
   its own file under the same name.
5. **Everything a package brings is reported**: manifest, network,
   personal-data and approval reports.
6. **Development never leaves the machine.**

A new feature that needs a backend asks, in order: is it state the app
keeps (it names a store), a job several providers do that the compiler
gains from knowing (a capability), or one provider's API (a provider
package)?

## Decided: capabilities are only jobs the language does itself

Decided 2026-10-16 (Richard), from
[the capability comparison](capability-providers-options.md), its design C,
with every open question answered as that page recommends.

- **A capability exists only for a job Jadpo performs itself, and covers
  that job whole.** Every connection that fills it does all of it, so
  partial support cannot arise and there are no optional parts.
- **Email is a Jadpo-rendered `EmailMessage` sent after commit**,
  attachments included (a `File` field on the message is an attachment,
  checked against the package's declared size limit), through SMTP, SES,
  Postmark or any package that provides it, chosen in `app.jadpo`. Jadpo's
  own mail, which today is area 15's privacy mail (area 8 leaves magic links
  to the app and has no passwords), always goes this way. A provider that cannot send attachments stays a package but cannot
  be the email connection.
- **Everything provider-specific is the package's own operations**, called
  from an action or subscriber in a domain the connection lists, as
  payments are: hosted templates with variables
  (`postmark.email.send_with_template(...)`), suppression lists, marketing
  sends. One connection serves both: `email:` uses it, and the listed
  domains call it.
- **Bounces and complaints are the package's own webhook events**;
  `EmailBounced` and `EmailComplained` go from area 16. They return to the
  capability only when a built-in feature consumes them.
- **SMS is not a capability**: nothing in the language sends SMS. Twilio
  is a package, its text still typed against translations; `sms` returns
  if SMS sign-in codes are built. `publishing` stays, for outgoing
  webhooks.
- **Erasing a person at a provider** uses area 15's package-level deletion
  declaration, not capability mappings.
- **Mappings stay data**, with no function calls; computation lives in the
  decided encoders and decoders. A mapping gains `each:` and `as:` to map a
  list element by element.
- **A package input may take any of the app's object types** (Postmark's
  template variables, Stripe's metadata), serialised as JSON: a general
  addition for area 2 to design.
- **Image variants**, when built, are made by the runtime over any store,
  so `files` stays whole.
- **Emails without an idempotency key** keep area 12's parking on
  `outcome_unknown`; revisit if chunk 7.3 shows parking is common.
- **Two kinds of mail through two connections** wait for an app that asks.
- **What is lost for hosted-template mail**: the dev inbox entry, the
  health score and the automatic template check (an app can add a contract
  case). Moving provider rewrites its bounce subscribers.

## Decided: provider calls in development

Decided 2026-10-16 (Richard), as in section 4 of the capability comparison.

- **Every provider operation is faked by default in `jadpo dev`**,
  answered from the package's recordings where they exist and generated
  defaults otherwise, so nothing leaves the machine and no credential is
  needed.
- **Named operations or resources go to the sandbox, never live**, through
  `development: { sandbox: [payment_intents.retrieve, customers] }` on the
  connection in `app.jadpo`, where review and agents see it. `--fake` and
  `--sandbox` on `jadpo dev` change one session only.
- **`jadpo calls script <operation> <failure>`** sets the next answer, so an
  agent can walk a failure path by hand.
- **Every call is logged**, faked or real, with its inputs and answer in
  full (development only, secrets never), its source (recording,
  generated, scripted, sandbox), attempts, duration and request: in
  `jadpo calls`, `jadpo calls show <id>`, `jadpo trace` and `--json`. This
  changes area 17's hashing of text for call payloads in development only.

## Proposed: choosing a provider when deploying

`email: { connection: postmark }` is fixed at build time. An app that
other people deploy (an open-source product run by many operators) would
instead declare every connection it supports and let a setting choose one
at start (`connection: config.mail_provider`), so every candidate is still
compiled, checked and reported, and an unchosen one needs no credential.
Depends on whether Jadpo apps will be run by people who did not write
them. Sending different messages through different connections, and
falling back to a second provider, come later and only when an app asks.

## Decided: sign-in providers are connections

Decided 2026-10-16 for Richard (he asked that nothing be left for the
build): every sign-in provider is a connection, `google: { driver:
Driver.oidc  issuer:  client_id:  credential: }` in `connections:`, and
`sign_in: { magic_link: { ... }  providers: [google, github] }` names
them, the way `email:` names its connection.

## Decided: details settled for the build

Decided 2026-10-16 for Richard, closing what the sections above left open:

- **Package keys**: `idempotency: { header:  window: 24h  max_length: 255
  in_use: "<provider code>"  reused: "<provider code>"  new_key_on:
  "<provider code>"  replays_failures: true }` (`new_key_on:` for a
  "first attempt failed, use a new key" answer; `replays_failures:`,
  default false, for a provider that replays failed answers under the same
  key); the
  safe-to-repeat marker is the existing operation key `idempotent: true`;
  an operation's OAuth scope is `scope: "calendar.events"`; an email
  mapping declares `limits: { attachments: 10mb }` in
  `provides/email.jadpo`; in an `each:`/`as:` mapping the current element
  is `item` (`as: { Name: item.name }`).
- **Licences**: `Licence.mit`, `.apache_2_0`, `.bsd_2_clause`,
  `.bsd_3_clause`, `.isc`, `.mpl_2_0`, `.unlicense`, and `.proprietary`,
  which only git-sourced packages may use (the public registry refuses
  it).
- **`x-jadpo` names**: `x-jadpo-changes`, `x-jadpo-idempotency`,
  `x-jadpo-failure` (`code`, `kind`), `x-jadpo-constraints`,
  `x-jadpo-events`.
- **Acceptances**: a connection that cannot erase personal data is
  accepted as `ErasureNotPossible`.
- **No deploy command**: a rerun detects a changed build by the build id
  in the manifest and compares the record's step shape; operators get
  `jadpo batches list`, `show` and `cancel`.

## Revised: other candidate capabilities

Under design C a capability exists only for a job the language does
itself. Search qualifies if an external index ever backs the decided
`search:` section; bot protection would if routes gain it; push
notifications do not until a built-in feature sends them. Payments and AI
models stay provider packages.

## Proposed: what a library package may also carry

- **Email layouts and partials**, so a company's apps share one branded
  layout. Partials are typed and read-only (area 16), so they are pure.
- **Translations** for its own failure messages and partials. *Decided
  2026-10-16:* see "Translations", "Packages and strictness".
- **A short usage guide** for agents: the surface says what a package
  offers, the guide how to use it well (handling a refund with Stripe).

Not packages: starter templates (`jadpo new --template`), which belong
with the references in `examples/` if ever wanted; company-wide rules
("no app sends personal data to this provider"), which are review tooling
for areas 17 and 18.

## Proposed: software and disk

Jadpo code never runs a program or touches the disk: Workers has neither,
and a package that could run programs is npm's install-script problem
again. Software such as LibreOffice, headless Chrome, ImageMagick, ffmpeg,
ClamAV or Tesseract reaches an app in one of two ways:

- **As a service with a provider package.** Most are already packaged as
  small HTTP services (Gotenberg for PDFs, imgproxy for images, REST
  wrappers for ClamAV). The registry carries curated packages for the
  common ones, and the connection gives the service's address, as for any
  self-hosted product (area 11).
- **As a built-in driver behind a capability**, when the compiler gains
  from knowing it: image variants on a `File` field
  (`variants: { thumb: 200x200 }`, as Rails' Active Storage has), virus
  scanning of every upload before it is attached, and a `documents`
  capability that renders a typed template to PDF exactly as email does.
  Each is built when an app needs it.

Disk is the `files` capability. A disk driver for production on the
native target, refused on Workers by the target rule, would let a
single-server app keep files on its own disk.

## Proposed: services, containers run by Jadpo

Richard, 2026-10-16: commissioning a server that needs custom software
means maintaining Docker Compose files and deploying those boxes
separately. If a package can declare the container it needs, everything
the app depends on is described in the app.

```text
// packages/gotenberg/v8/provider.jadpo
provider {
    protocol: Protocol.multipart_http
    runs: {
        image: "gotenberg/gotenberg:8@sha256:…"
        port: 3000
        asks: ["fonts.googleapis.com"]
    }
}

// app.jadpo
pdfs: {
    package: Gotenberg.v8
    domains: [billing]
    outbound: ["fonts.googleapis.com"]
}
```

The image's digest is written in the package source, which the package's
own digest already pins, and an own integration in `shared/providers/` has
no lock entry. The image is listed in the approval report.

**Only stateless services** (decided 2026-10-16, Richard): a service
holds no data and has no volume, and each app instance runs its own copy,
so many servers need nothing shared. Anything that holds data (PostgreSQL,
Redis) is infrastructure the operator provides and the app reaches through
a connection; `jadpo dev` runs those locally as it already does, and
nowhere else.

**Security is the engine's, with three settings Jadpo always applies**
(decided 2026-10-16) when it creates a container: no outbound network
unless granted (`outbound:` on the connection) (the app's own deny-by-default rule, area 11 question 9,
which Docker's default network does not give: a PDF service rendering an
invoice could otherwise send it anywhere), never privileged, and no access
to the host's disk. Jadpo creates the containers itself, so there is no
configuration to audit.

Proposed stages, each useful alone:

1. **Development.** `jadpo dev` starts the services the app's connections
   declare, as it runs the database; tests keep using fakes.
2. **Deployment config.** `jadpo build` writes the configuration that runs
   each service beside the app (Compose first).
3. **Supervision on the native target.** The deployable starts, checks,
   restarts and upgrades its services on its own server through the
   server's container runtime, and their logs and health join the app's
   telemetry. Kamal's "accessories" and .NET Aspire's app host do parts of
   this.

**Outbound network needs the app's permission** (decided 2026-10-16,
Richard). A package can only ask: it lists the hosts its service needs.
The service gets none of them until `app.jadpo` grants them on the
connection, where an agent or developer writes them deliberately, and a
new grant appears in the approval report. Until then `jadpo check` names
the hosts asked for and the line that grants them.

**Reaching a service** (decided 2026-10-16, Richard). Packages never choose a port on the
machine; a package says only which port its service listens on inside
its container, so two packages can both use 3000. Jadpo gives each app a
private network that only it and its services are on, and hands each
connection the address to use: the service's name on that network when
the app itself runs in a container, or a loopback port the engine picked
when the app runs as a plain process. Nobody writes an address, so no two
packages, or two copies of the app on one server, can collide. Still to
check when it is built: Docker's networks that block outbound traffic may
not allow a port to be published to the machine, which would mean either
firewall rules or running the app in a container beside its services.

**How containers run** (decided 2026-10-16, Richard): Jadpo is a client, never a container
runtime. It talks to whatever engine the machine has through the
Docker-compatible API, which Docker and Podman both serve (Podman without
a daemon or root), and leaves pulling, isolation and restart policies to
it. Jadpo's own part is the declaration, the digest pin, locked-down
defaults, start order, health and telemetry. On a Mac, containers need a
Linux virtual machine (Docker Desktop, OrbStack, Podman), so `jadpo dev`
says plainly when none is installed.

**Docker stays behind the scenes** (decided 2026-10-16, Richard). Nobody
drives the engine directly; agents and developers manage services through
`jadpo` and the dev console (area 17), and every command has `--json`:

| Command | Does |
|---|---|
| `jadpo services` | Each service: package, image, state, health, CPU and memory, restarts, uptime |
| `jadpo services start`, `stop`, `restart <name>` | As named |
| `jadpo services logs <name>` | Its logs, `--follow` to stream, `--since` to go back |
| `jadpo services shell <name>` | A shell inside the container, for debugging; development only |

- **Logs join the app's own.** A service's output is collected into the
  app's OpenTelemetry logs under the service's name, so it arrives wherever
  the app's logs go, and a call to a service already appears in the
  request's trace as a provider call.
- **In production the commands talk to the running app**, which supervises
  its services, never to the engine, as area 14's operator commands do.
- **The engine is needed only when a package declares a service.** An app
  with none never needs Docker or Podman. When one does and no engine is
  found, `jadpo dev` and the deployable say so and how to install one.
  Screens follow the CLI design language and are designed before they are
  built.

On Workers, which runs no containers, a service is hosted elsewhere and
the connection gives its address.

## Decided: watching code run in development

Decided 2026-10-16 (Richard): an agent should be able to watch for a particular
part of the code to run, then see its inputs and outputs, without reading
whole logs. For area 17, beside `jadpo trace`.

```text
jadpo watch action shipping.book_collection --once --json
jadpo watch domain billing
jadpo watch call couriers.bookings.create --failed
jadpo watch file domains/shipping/actions/book_collection.jadpo
```

- **Filters name code, as the compiler knows it**: a route, action, query,
  subscriber, schedule, provider operation, domain or file, optionally
  narrowed to failures or faults. Unknown names are an error with the
  nearest match, as everywhere else.
- **Each match prints the step with its inputs, output or failure, the
  queries, writes, events and calls inside it, timings, and the request
  id**, so `jadpo trace <request>` opens the whole request.
- **`--once` exits after the first match**, so an agent starts it in the
  background, triggers the flow, and waits for the command to finish
  rather than polling; `--count` and `--timeout` bound it, and `--json`
  gives the same as data.
- **Development only, with values in full**, as the development call log
  shows provider payloads (capability options, question 14), in
  `jadpo dev`'s git-ignored data; secrets are never shown. Production
  keeps area 16's spans with no values.
- The screens follow the CLI design language and are designed before they
  are built.

## Every extension point

| Thing | Way | State |
|---|---|---|
| Database, cache, schedules | Capability, built-in drivers only | Decided (area 11) |
| Files, email, publishing | Capability, drivers and packages; only jobs the language does itself | Decided |
| SMS, hosted email templates, bounces | Provider packages' own operations and events | Decided (design C) |
| Sign-in providers | Connections: `Driver.oidc` with an issuer, GitHub and Apple as `Driver.github` and `Driver.apple`; named by `sign_in: { providers: [...] }` | Decided |
| Search, bot protection | Capabilities only once the language uses them | Decided rule |
| Rate limits, flags, sessions, idempotency keys, outbox, audit | Name a store; not capabilities | Decided (areas 5, 12, 16) |
| Event delivery | Runtime only; external queues are publishing targets | Decided (area 12) |
| Telemetry | OpenTelemetry settings | Decided (area 16) |
| Secrets | Environment variables through `config.jadpo` | Decided (area 10) |
| Payments, AI models, any other API | Provider package | Decided (area 11) |
| Money, countries, phone numbers, safe HTML, CSV | Built in | Agreed, for 5.4 |
| Shared types and rules | Library package | Decided |
| Whole features | The agent writes them; references in `examples/` | Decided |

## What the coming chunks follow

| Chunk | Follows |
|---|---|
| 5.4 Standard library: the plan | The built-in-or-package table and its rule; where reference data is declared |
| 7.1 Packages and connections | `packages/<name>/<version>/` for installed packages only, committed and checked against `jadpo.lock`; the app's own integrations in `shared/providers/`; reserved brand names; `jadpo package` commands; both kinds laid out as `shared/`; `public` as the API; `use` for library packages; the surface and the publishing bar |
| 7.2 Importing and faking | Imports write into `shared/providers/`; decoding (`wire:`, `decode:`, `wire_output:`) with built-in encodings; recordings checked by decoders; fakes scripting the clean type |
| 7.3 Email | The `email` capability whole (Jadpo-rendered, attachments) with SMTP as one driver and a mail package as the second, proving the swap; provider calls faked in development and logged |
