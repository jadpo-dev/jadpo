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
| The surface | `package.surface`, readable, with file pointers; `jadpo package show` |
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
6. **Each override scenario, one at a time.** The eighteen in the
   scenarios page (files, cache, schedules, search, sign-in, flags, audit,
   rate limits, built-in emails and text, generated routes, decision
   points, erasure at providers, bot protection, outgoing webhooks,
   publishing targets, telemetry, failure format, what cannot be
   overridden), each shown simply and decided, with its smaller answers:
   erasure keys, webhook handshakes, telemetry as a connection, RFC 9457
   per API, `search`, `bot_check` and `scanning` capabilities, disk and
   memory drivers in production.
7. **Receiving from outside: webhooks and queues.** *Decided* (area 11
   question 11): a connection with a webhook secret generates
   `POST /webhooks/<connection>`, checks the signature, drops repeats by
   event id and publishes each as a Jadpo event that subscribers take with
   `on: Stripe.PaymentIntentSucceeded`. *To walk through and design:* the
   whole Stripe flow end to end; webhooks for the app's own integrations
   and for senders with no package (a partner posting to the app); webhooks
   that need a reply body rather than `200` (Twilio's TwiML, Slack's
   interactive replies), which an event cannot give; handshakes (Slack,
   Microsoft Graph, SNS); ordering and out-of-date events; whether queues
   and topics (SQS, SNS, Cloudflare Queues, Kafka, EventBridge) are
   consumed directly (a connection the runtime reads, each message arriving
   as an event) or only through webhooks (SNS's HTTP delivery); retries and
   dead letters; Workers' queue consumers; how this fits area 12's events.
7b. **Multi-step provider calls**: an action (the app's or a package's)
    making several changing calls in order, answers recorded as they
    happen, a key per step, resuming after a failure, a warning where a
    step can fail for good after an earlier one took effect; calls without
    an idempotency key stay alone. Reopens areas 11 and 12's "one changing
    call per action". Settle before chunk 7.4.
7c. **Per-user OAuth**: apps acting for each user on Google, Xero or
    HubSpot need those users' tokens kept and refreshed safely, owned by
    the runtime like sessions.
8. **What a library package may also carry**: email partials and layouts,
   translations, a usage guide for agents. *Recommendation:* yes to all
   three. *Translations decided 2026-10-16 (Richard):* see "Translations",
   "Packages and strictness".
9. **Software and disk**: a disk driver for files on one native server,
   image variants made by the runtime, virus scanning, and the `documents`
   capability rendering typed templates to HTML or PDF, now needed (the
   invoice scenario in [the scenario sketches](package-scenarios-code.md)
   has nothing to render its page).
10. **Packages generated from your own Jadpo apps.** Four calls: generated
    from the API record, read-only, in `packages/`; `jadpo package add
    ../orders` within one repository, the path in the lock; breaking
    changes (already needing sign-off) become a new major version;
    `jadpo check` reports what a change breaks in other apps in the same
    repository. *Recommendation:* yes to all four.
11. **Private packages**: a company's shared packages live in their own
    repositories (decided); where apps install them from (a private
    registry or a git repository) and whether the publishing bar applies
    in full.
12. **Choosing a provider when deploying**: only if Jadpo apps will be run
    by people who did not write them; every candidate declared, a setting
    picks one.
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
  the confirm page's `magic_link_confirm_heading` (`address`) and
  `magic_link_confirm_button`; and the failures' wording.
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
  "Acme" }`, plain text, required once any built-in email or page is on;
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
    libraries/uk.jadpo              library uk { public parse(...)  normalise(...) }
    libraries/uk.test.jadpo         tests beside the file, as in an app
    CHANGELOG.md
    package.surface                 generated
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
    package.surface
```

- **`public` marks the package's API**, as it marks what a domain offers
  other domains. Everything else is internal and may change freely.
- **The index lists no files**: a file is named after what it declares,
  so the compiler finds a resource by name and loads only what the app
  uses.
- **A re-import rewrites only imported files.** Written libraries and
  corrections survive, as area 11 already says for corrections.
- **Everything reachable from the public surface is public** (decided
  2026-10-16, Richard): a failure a public function `fails` with, a
  provider operation's `failures:`, and the types in a public signature
  must themselves be public, checked on every compile; internal failures
  are mapped or handled before they leave a public function. The same
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
  `Driver.oidc` with an issuer; area 8's built-in presets narrow to GitHub
  and Apple, which are not plain OpenID Connect.
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
writes it to `package.surface`, beside `package.jadpo`: the package's
description, licence and version, every public type with its fields and
constraints, every failure with its kind and code, every function's
signature, failures and description, and for a provider its operations
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
2. every public type, failure and function has a `///` description;
3. every public function has at least one example in its description,
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

## Proposed: sign-in providers as a capability

Decided above: Google, Microsoft and other OpenID Connect providers are
`Driver.oidc` with an issuer, and built-in presets remain only for GitHub
and Apple. Still proposed: making sign-in a `sign_in` capability, so
`sign_in: { connections: [google, github] }` names the connections the way
`email:` does.

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
| Sign-in providers | `Driver.oidc` with an issuer; GitHub and Apple built in; a capability if agreed | Decided; capability proposed |
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
