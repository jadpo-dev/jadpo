---
title: Standard library, design notes
area: 19
---

# Standard library: design notes

Why [Standard library](../19-standard-library.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Consistency with the rest of the language

- **Area 1:** `list`, `set`, `map`, `range`, `text`, `markup`, `money`,
  `units`, `hash`, `encoding`, `crypto`, `random`, `json`, `csv`, `tsv`,
  `json_lines`, `colour`, `media`, `web`, `semver`, `geo`, `ai` and
  `locale` are reserved namespaces; fields and named arguments keep every
  word (`note.text`, `set:`). It is `units` rather than `quantity`, since
  areas 1 and 4 name variables `quantity`.
- **Area 2:** the standard types, `EncryptionKey` and `PasswordCheck` are
  built-in names; `for` and maps as "Language gaps" says.
- **Area 3:** inline functions may take two parameters for standard
  operations; `any`, `all`, `find`, `min_by`, `max_by`, `sort_by` and
  `group_by` take inline functions. Text order is `sort_by` with a text
  key. No user-defined operators stands beside area 3's generics rule,
  and the `%` literal beside its remainder operator.
- **Area 4:** decoding failures are conditions mapped by name, as area
  10's `dst:` is, and overflow stays a defect.
- **Areas 5 and 6:** `Money`'s two columns; aggregate queries (area 6);
  one-step updates as area 6 has them.
- **Area 10:** a secret stays unreadable, with the `key:` of `crypto`
  among the compiler's places; `random` is generated like `clock`;
  `{price, number, currency}` is among the placeholder styles; the time
  types' format is `text.format`.
- **Area 15:** encrypted fields with one application key; per-person keys
  stay out; data sent to a model is listed in its report.
- **Packages:** money, countries, languages and phone numbers are built
  in, not packages; the file formats' operations live here, and area 12
  points to them.
- **Compiler design** (`docs/compiler-runtime.md`, the standard library):
  the operation table has `never_empty`, `never_negative`,
  `at_most_length_of`, literal-only `scale:` and `fill:`, conditions no
  option settles (`malformed`, `mismatch`, `unreadable`,
  `currency_mismatch`, `too_many_places`), a `sort_by` key limited to
  ordered kinds, actions-only operations, and a SQL form per operation.

## Decisions

| Topic | Decision |
|---|---|
| Shared vocabulary types | The standard library defines a type wherever two packages would otherwise each invent their own, even when the heavy operations live in a package or another area; each has one construction that validates, one wire shape and one storage shape; positional pairs are designed out by named fields, and an axis order exists only in a conversion that names it (`geo.from_lng_lat`) |
| Standard types are global, in one flat set | Every standard type is a global name, with no split between core types and types inside namespaces; functions stay in namespaces. Apps name their own types for purpose and shape (`DeliveryStop { address: PostalAddress }`, `BillingContact`), never re-declaring a standard one |
| The standard types | 247 adopted types in 18 groups (table under "The standard types"), and the parts and kinds they name; units folded into their quantity type, roles into the general type, duplicates merged, formats and application entities out; country-specific identifiers are global types; gender and sex at birth stay out; a quantity is stored in its SI base unit, with the unit chosen at construction and display |
| A standard type's parts are standard types | Each part or kind a standard type names (`IdentityDocumentKind`, `InterestRateBasis`, `ContentRatingScheme`, `RichTextBlock`, `RichTextSpan`, `RichTextMark`, `ClothingSizeSystem`, `ShoeSizeSystem`, `RouteOfAdministration`, `ChatRole`) is a standard type too, so it is read, sent and stored by its code |
| Every standard type meets one contract | Every standard type has a library with the same baseline: construction and validation from its parts and from text with failures named by rule; formatting for people as `PresentationText`, through the locale where it matters; a canonical text form where one exists; one JSON shape and one storage shape; `==`, and order where the type is ordered; conversions at named edges. Some groups add extras |
| The contract, generated | Each standard type is a stub declaration in `crates/jadpo-check/std/` plus a data entry (canonical text grammar, JSON shape, storage columns, CLDR format key and options, order, conversions) and at most one named validator where data can't say it (IBAN's mod-97, Luhn, ISBN and NHS check digits, E.164's metadata); one implementation of each baseline operation reads the data in the operation table, the interpreter and the runtime, and one harness tests every type: valid and invalid examples per rule, and the round trip through text, JSON and storage. Calls are `text.parse(raw, as: T)`, `text.format(value, locale:)` and `text.from(value)` over every type; the time types' format is `text.format`, with a habit error whose edit writes it; `format_relative` stays in `temporal` |
| Precise names | A standard type's name is (a) the defining standard's term, qualified until unambiguous, else (b) the noun qualified by what it holds, and (c) never a bare word an app plausibly uses for something else, except a generic container; `Point` and `Length` are the exceptions; built-in names older than the rule keep their names |
| `type.standard_exists` | A type whose shape is a standard type's is the declinable error `type.standard_exists`, in packages' public types and app code alike; it stops `check` and the build until the type is swapped (in one step when not stored or on the wire, otherwise with its data migration through `ApiChange` and `DataChange` acceptances) or declined with area 1's `decline type.standard_exists: "<reason>"` line, which an agent or a person may write and `jadpo approval` lists |
| Distinctive shapes only | The shape check matches only distinctive shapes: at least two fields, at least one domain field name (not `name`, `value`, `id`, `code`, `type`, `text`), field types lining up, no extra fields; single values never match by shape |
| A release adding a standard type | Never a silent break: `check` reports each clash with the new type and its release (`type.standard_clash`), fixed by swapping or renaming; `jadpo fix` applies the chosen fix through the editor's code; release notes list added names |
| Changing a standard type's encoding | A Jadpo release changes how a standard type is sent, stored or written as text only together with a reader for the old form and a release note; a golden file per type, built from its examples, pins every form so a change can't pass unnoticed |
| Every boundary through the contract | No standard type loses its currency, unit or precision on the way into or out of a program: every boundary encodes a standard type only through its contract's JSON, storage and text shapes, as the table under "Every boundary through the contract" lists; every object and choice is stored in named columns; fault text never carries a value; one reader per text form, and map keys as canonical text; JSON-held values stored as their contract's JSON; one JSON encoder and one OpenAPI schema per type, read from its entry; every SQL literal the value the storage codec binds, a parameter wherever the statement takes one |
| One namespace per type | `list.`, `set.` and `map.`, with a habit error for `collection.` |
| Reserved namespaces | Every standard namespace is reserved; fields and named arguments keep every word |
| List operations | No push or pop; `last`, `at`, `min_by`, `max_by`, `append`, `flatten` and `group_by` |
| Cutting from either end | `keep_first`, `keep_last`, `drop_first`, `drop_last` on text and lists, `n` never negative and proved so; `slice` for the middle; no negative positions |
| Sets and maps | The seven `set` and ten `map` operations, returning new values; `for` walks a set in area 2's canonical order; maps walked through `map.keys`; `[]` is an empty map where one is expected; no dynamic keys on records |
| `at_most_length_of` | Recorded for `trim`, `slice`, `keep_*` and `drop_*` only |
| Code points | Text is counted and cut in Unicode code points; graphemes left out |
| Slugs and text order | `text.slug` is ASCII-only; text sorts in code points unless `locale:` says otherwise |
| `Rounding.down` | `Rounding` has `down`, toward zero, with SQL form `trunc`; `floor`, `ceiling` and `half_down` stay out |
| Maths on `Decimal` | The eight operations, correctly rounded to a named scale, the same on every target; no `Float` |
| `Money` and the operator table | Minor units with the ISO 4217 exponent, stored as a `bigint` and a currency column; `money.from_minor` and `money.of` (failing `too_many_places`); arithmetic through the standard operator table, functions only for `allocate`, `split` and `convert`; a currency mismatch a compile error where both types fix it, else `currency_mismatch` under `attempt`, needing no arm inside an `if` comparing currencies; a `rounding:` setting, `half_even` by default; `/` by an `Int` refused as `type.money_split`; wire `{"amount": 1050, "currency": "GBP"}`; `text.format` in code and `{price, number, currency}` and `{n, number, percent}` in messages; `20%` a `Percentage`, remainder keeping its spaced `%`, `20%3` an error; no user-defined operators |
| Quantities | Always made with a unit; inside a program no unit; a bare number where a quantity is expected is `type.unit_required`, never declinable; the product and quotient table, `==` across units, any other mix an error; wire `{ "value": "2.5", "unit": "kg" }`; case-sensitive suffixed literals as tabled under "Suffixed literals", `m` staying minutes and metres written `5metre`, the longest suffix winning, a near-miss by case an error; a `Mass` held to the microgram |
| `Colour` | `red`, `green`, `blue`, `alpha`, each 0 to 255, in sRGB, written `#rrggbb` or `#rrggbbaa`; extras `colour.from_hsl`, `from_oklch`, `mix`, `with_lightness` and `contrast` (WCAG 2), all giving sRGB; CMYK a package |
| JSON in memory | `json.read(text, as:)` and `json.write(value)`, one name per direction beside the batch forms; `mismatch` carries the first problem's path and rule; no dynamic JSON value; CSV file-only |
| Data formats | The `csv`, `tsv`, `json_lines` and `json` file operations work through batches (area 12) and are tabled on this page, one shape per namespace with the in-memory `json` forms |
| Validating mid-program | Construction is the way, nothing new for values or records; rules across an object's fields are area 2's |
| Closed value sets are enums | A closed, authority-published, rarely changing list is a built-in enum (those marked "(enum)", `CountrySubdivision` included); large or fast-changing lists stay text with a `pattern:`; variants are the code in lower case with `_` (`Country.gb`), a reserved word or `all` taking a trailing `_`; text becomes an enum at a boundary and by `text.parse` mid-program, ignoring case; other code systems through `form: CodeForm.alpha3`; `text.format` gives the CLDR name and `.all` lists every variant of an enum without data; no matching of names; retired codes stay readable, and writing one is the declinable error `type.variant_retired` |
| `HttpMethod`'s variants | The five a route is written with (`get`, `post`, `put`, `patch`, `delete`), its codes RFC 9110's tokens |
| A digest is `Bytes` | `hash.sha256` gives `Bytes`, written as text by `encoding` |
| `crypto` | Ten operations with no algorithm choices: XChaCha20-Poly1305 with a required `context:`, Argon2id with `PasswordCheck.correct_outdated`, HMAC-SHA-256 checked in constant time, Ed25519, JWT and TOTP checks; keys are opaque settings written `key: config.<name>`, a compiler-owned place; `crypto.public_key` gives a `PrivateKey` setting's public half |
| A `PasswordHash` never leaves the server | It is stored as its PHC string, but refused in an `Output`, a request's `Input`, a failure's fields and `json.write` and `json.read`, as `type.password_hash_exposed` or `type.mismatch` |
| JWTs | `crypto.verify_jwt` verifies EdDSA, RS256 and ES256, verification only, each key's own algorithm; `as:` decodes the claims into an application's `Object`, a field named for a registered claim filled with the value checked; a `Jwt` is a secret value |
| Encrypted fields | An `encrypted: true` field setting, specified in area 15, with one application key, kept out of indexes, unique rules, filters, ordering, search and logs; per-person keys stay out |
| Randomness | A generated `random` namespace, actions only, seeded per test, recorded for replay, with `random.token` and `random.code` |
| No gates or bits | No logic gates beyond `and`, `or`, `not` (`!=` is exclusive or); no bitwise operations |
| SQL forms | Used by computed backfills and by aggregate queries, each tested against the runtime |
| Geospatial and AI | Each its own area, [20](../20-geospatial.md) and [21](../21-ai.md); this page holds their types |
| A chat message is sensitive personal data | `ChatMessage` is personal data whatever field holds it, in area 15's sensitive `health` category, the most protective that fits free text |
| `TokenUsage` keeps no total | Its `input`, `output` and optional `cached`; the total is `input + output`, worked out where needed, and a `total` member is refused |
| Generated scopes narrow `OAuthScope` | Area 9's enum of an application's declared scopes narrows the standard `OAuthScope` to them, a closed subset, rather than reusing the name |
| A key's `key_ops` | Read and held as `Set<Text>`, not refused: RFC 7517 §4.3 allows it and WebCrypto always writes it. A key whose `key_ops` leaves out `"verify"` checks no signature (Richard, 2026-10-30) |

## The opening topics to SQL forms

The reasons behind some of [Standard library](../19-standard-library.md)'s
rules, and how the compiler and runtime build them.

- **One flat, global set of types.** Every standard type is a global name
  (`PostalAddress`, never `geo.Point` beside a global `Email`), since two
  tiers of type would be one more thing to remember, and apps should look
  alike. An app that can't declare its own `Address` names its types for
  their purpose and shape, and that forced explicitness is the point.
- **The contract, as data.** Each standard type is a stub in
  `crates/jadpo-check/std/standard.jadpo` and a data entry in
  `jadpo-std`'s `standard` module, which the three operations, JSON,
  storage and `<` read, and one harness runs every type through. An entry
  (`StandardType`) holds what the type holds, its `TextForm`, its `Json`
  shape, its storage columns, its `FormatKey` and the options it takes,
  whether it is ordered, its named `Validator`, the shape
  `type.standard_exists` matches and the examples the harness reads; a
  stub whose base is its own name is a type built into the language, and a
  test holds the stubs and the entries to each other. An entry may take a
  type argument (`Range<T>`, whose examples name the type they hold), be a
  choice of other types (`Discount`, an enum with data, area 2, whose
  variants' fields are its entry's), be built on a built-in type with
  bounds of its own (`TaxRate`), or be a named collection (`RichText` a
  list, `LocalisedText` a map). Each entry has its `signals`, the data
  `jadpo suggest` finds hand-rolled uses of the type by, beside its shape
  (area 14, "Suggestions").
- **Rules across a type's parts are checks in `jadpo-std`**
  (`standard::across`), not conditions written in Jadpo: a postal code by
  its country and a document's number by its kind and country read tables
  no operator can, and making them standard operations a rule may call
  would add names to the language. The simple orderings are checks too,
  so there is one way. Each has one implementation, which the checker
  reads a literal by and the runtime runs on every value made. The
  checker doesn't hold the contract's text readers, which is why text
  written where such a type goes is refused.
- **Every boundary is held to the contract by tests.** Storage, requests
  and responses, OpenAPI, `json.read` and `json.write`, settings, query
  strings, paths and headers, `text.parse` and `text.from`, and SQL
  literals are each held to the contract by a test made from every
  standard type's entry. Each event type is numbered in the plan's JSON
  table for the outbox (area 5).
- **The storage layout** is named by each entry's columns, and
  `jadpo_std::standard::storage::layout` says from the type's parts alone
  what each column holds, a test holding the two together.
- **One storage codec for compound values.** `jadpo-value`'s
  `standard::to_parts` and `from_parts` turn every compound value into
  its columns and back from the layout, and the plan reads any compound
  field through them.
- **JSON-held values** (`RichText`, `RichTextBlock`, `LocalisedText`):
  the plan numbers each such field's type in the JSON table beside
  `json.read`'s; a column reads it and a write's parameter writes it.
- **A stored range that holds nothing.** Proving a stored range non-empty
  at compile time would be a language change, and the language doesn't
  make it.
- **One reader per text form** is `jadpo_value::standard::builtin_text`,
  which `text.parse`, a JSON string, a path, a query string, a header, a
  setting and a map's key all use.
- **One JSON encoder.** The JSON reader and writer and OpenAPI name no
  standard type a built-in type holds, which a test made from the entries
  holds them to, and the per-type harness reads every type through the
  codecs the plan's own builder makes.
- **SQL literals.** The encoder of a parameter is
  `jadpo_std::standard::storage::bind`, with the `SqlValue` it makes; the
  runtime's `to_sql` calls it, and the compiler makes every literal in SQL
  with it, after decoding the literal for its field exactly as the plan
  decodes a body's (`jadpo_std::constant::literal`).
- **Golden files.** A golden file writes each storage column's value as
  `jadpo_sql::literal` writes it into a statement. The release's encoding
  is `jadpo_std::standard::ENCODING`. A passkey is stored as
  `WebAuthnCredential`'s contract JSON through the plan's JSON table (area
  8, "Encodings of signed parts and passkeys"; now on area 5's "Encodings
  in system tables"). The table of encodings each release writes is held
  to the golden files by a test.
- **Changing a form on purpose.** Each standard type's golden file is in
  `crates/jadpo-interp/tests/goldens/standard/`, made by the codecs a
  program gets; a test fails when any form changes. Changing a form on
  purpose takes four steps: raise the encoding; read the old form
  wherever it is kept, chosen by the encoding the row or token records;
  add the encoding's row to the page's table, naming each type whose forms
  it changed; then `JADPO_BLESS=1 cargo test -p jadpo-interp --test
  integration standard_goldens::`, which rewrites a changed file only past
  the encoding it records. A new type, or a new example, needs only the
  last step.
- **Fixes.** `type.standard_clash` and `jadpo fix` with the editor's "Swap
  for the standard ..." quick fix both go through `jadpo_check::fixes`;
  the rename is `jadpo fix`'s, through the same code. A general `jadpo
  rename` is area 14's, after the first build.
- **Releases of standard types.** Each data entry carries its `release`.
  The table of names each release adds is held to the data entries by a
  test until releases have notes of their own.
- **Moved operations** are listed in `MOVED` and `MOVED_NAMESPACES` in
  `jadpo-std`.
- **Pinned reference data.** Unicode data, CLDR formats and ISO lists are
  the versions pinned in `jadpo-value`.
- **The operation table's facts.** The table's `never_zero` gained
  `never_empty` and `never_negative` with this area, and
  `at_most_length_of` was new here; `rounding:`, `dst:`, `invalid_day:`
  and `list.count`'s `at_least: 0` came before it.
- **The operator table** generalises area 10's rule for time (`Instant -
  Instant` gives a `Duration`), and area 3's rules for numbers and `+` on
  text stand. No user-defined operators stands beside area 3's rule that
  keeps generics to the standard library, for the same reason.
- **SQL forms are rendered in one place,** `jadpo_sql::form::operation`,
  in the SQL leaf, from each argument's SQL expression and how it is held.
- **How SQL forms are tested.** A test holds `trim`'s list to every
  character Rust calls white space. The test reads every form in a
  session with the time zone `Pacific/Chatham`, dates day first, the SQL
  standard's intervals and `standard_conforming_strings` off. It runs
  each operation with a form in the runtime and as its form, on the same
  columns a backfill reads, written through the storage codec, and
  compares what each gives as the codec binds it or as the runtime reads
  it back, places and widths included; an input the runtime stops on must
  fail the statement. It covers every rounding mode on halves either side
  of zero, the largest and smallest whole numbers in each column width,
  decimals of 38 digits, every white space character, four-byte
  characters and combining marks, open and held range ends, every pair of
  days across a leap February and thousands of pairs from the year 0 to
  9999, and IPv4-mapped IPv6 addresses. A test holds the tables' SQL form
  column and the renderer to the same operations; computed backfills use
  them, and aggregate queries' time buckets use `temporal.year`,
  `temporal.month` and `temporal.day` of a date (area 6).

## Why lists, sets, maps, ranges and numbers are as they are

Moved here from the area page's "Lists", "Sets", "Maps", "Ranges" and
"Numbers" topics, which now state only the rules.

- **No negative positions.** In JavaScript `slice(-2)` counts from the
  end, `substring(-2)` treats it as 0 and `substr(-2)` counts from the end
  again, and an off-by-one that goes negative silently reads from the
  other end instead of failing. Jadpo's `keep_*` and `drop_*` cut from a
  named end, and a count that might be negative doesn't compile.
- **No `reduce` or `fold`.** A `for` with a `var mut` is the one way to
  accumulate, and reads more plainly in review; `sum` covers the common
  case.
- **No sort with a comparator.** `sort_by` and `reverse` express every
  order a key can, and a comparator that isn't consistent gives different
  orders on different targets.
- **No `items[i]`.** Besides needing a failure arm or a defect for an
  index out of range, the syntax would suggest the list can be assigned
  into, which it can't.
- **An empty `sum`.** The checker fills an implicit `zero` of the items'
  representation (`Param::implicit`), so an empty list gives `0` or
  `0.00` as its type says.
- **Set operations don't wait for `for`.** Each is one operation in the
  runtime. `filter`, `any`, `all` and `map` on a set stay out until a
  scenario needs one; `for` covers them meanwhile.
- **Records have no dynamic keys, on purpose.** `record[name]` would give
  a value of no single type, lose the field type that keeps identifiers
  apart, and hide which field policy and personal-data marks apply to.
- **Where `Range<T>` is implemented.** The range operations' stubs are
  `crates/jadpo-check/std/range.jadpo`; `Range<T>`'s entry is
  `Holds::Range`, `TextForm::Range` and `FormatKey::Range`, with ranges of
  dates as its examples, `Examples::of`.
- **Maths on `Decimal`, not a `Float`.** A float's `sin` differs between
  platforms' maths libraries, and has `NaN`. Geospatial distance and
  statistics (a standard deviation) are what need these operations, and
  the language has no `Float`, so each result is correctly rounded by
  Jadpo's own decimal code and is the same on every target.
- **`Rounding` has four modes.** `floor`, `ceiling` and `half_down` stay
  out until amounts below zero need them.
- **A rate the wrong way round is `type.mismatch`.** Where the checker
  knows both currencies, `money.convert` with a rate from another
  currency reuses `type.mismatch` rather than adding a rule.

## Money operations to calendar events

Why and how the topics from [Money operations](../19-standard-library.md#money-operations)
to [Calendar events](../19-standard-library.md#calendar-events) work as they do.

- **Money's canonical text,** `GBP 10.50`, is how JSR 354 and Joda-Money
  write an amount.
- **Money's formatter** is ICU4X's currency formatter
  (`icu_experimental`), already pinned; the currencies' display names
  come through `icu_provider`.
- **`Discount` and `UnitPrice` were the first standard types that are a
  choice of other types.** Each is declared as a stub `= Enum { ... }`
  whose variants carry fields, each written as the built-in type its part
  is built on.
- **`UnitPrice`'s four kinds** are the ones the Price Marking Order 2004
  (Schedule 1) prices goods by. It is a choice by kind because a field
  holding a quantity of any kind isn't a type the language has.
- **More off than the price leaves nothing,** never less, as Stripe
  applies a coupon.
- **`BasisPoints` is shown with `bp`** in every locale because CLDR's only
  name for it is the per-myriad sign.
- **The calendar types** from `LocalTime` to `CalendarPeriod` are records
  of their parts in the contract (`Holds::Object`), each field a whole
  number with a range (`Part::Whole`) or another standard type, and their
  JSON is the canonical text (`Codec::Standard`). `Year` is a record of
  one `year` because a standard type built on `Int` with rules of its own
  would need declarations the project doesn't have.
- **`RecurrenceRule` and `OpeningHours`** are held as text
  (`Holds::Text`). A rule's expansion is in
  `jadpo_std::standard::recurrence`, which the checker and the runtime
  share, and RFC 5545's examples are its tests.
- **A recurrence's start must be its first occurrence** because RFC 5545
  leaves any other start undefined and calendars disagree about it:
  libical counts the start anyway, while `python-dateutil` and `rrule.js`
  don't.

### UK modulus checking

A UK account number also has to fit its sort code, by the modulus check
Vocalink publishes for Pay.UK's schemes ("Validating account numbers: UK
modulus checking", version 9.10, 5 September 2026). Each range of sort
codes has one or two checks, a standard modulus 10 or 11 or a double
alternate, with fourteen exceptions. The check needs Vocalink's weight
table and sort code substitution table, published with the specification
under "all rights reserved", so no release ships them. A `BankAccount`
takes any sort code and account number that are each well formed, since
the specification (§2.2.1) says to presume valid a sort code its table
doesn't cover; the check isn't wired into construction or `json.read`.

`jadpo_std::standard::modulus` holds the check, ready for the tables once
they may ship: the standard modulus 10 and 11 checks, the double
alternate check and exceptions 1 to 14, with the order of the checks and
which exceptions need only one to pass taken from the specification's 34
test cases where its flow diagram disagrees with them (exception 5 needs
both, as cases 23 and 24 say). It reads Vocalink's tables in their own
text format.

The tables, `valacdos.txt` and `scsubtab.txt`, are published under "©
Vocalink Limited 2026. All rights reserved", which says the information
may be used only for the purpose it was provided for, with no licence to
redistribute it. This repository is Apache-2.0, so no release ships them,
as for UN/LOCODE, pending Vocalink's permission, and no part of them is
in the repository.

The check's own tests run on a weight and a substitution table made up
for them, covering every method and exceptions 1 to 14. The 34 cases
(each only its sort code, account number and expected result, cited by
number) run against Vocalink's tables only when `JADPO_VOCALINK_TABLES`
names a folder holding the two files downloaded from Vocalink's site,
`JADPO_VOCALINK_TABLES=<folder> cargo test -p jadpo-std modulus`, and are
skipped with a message otherwise; with the tables, it passes all 34.

## Text, text formats, hashing and signed tokens

Moved here from the area page's "Text", "Text formats", "Hashing and
cryptography" and "Signed tokens" topics.

### Text

- **Left out by design:** `trim_start`, `trim_end` and `pad_end`, since
  no scenario needs them and each is a second way near one that exists;
  and iterating characters, since every job the scenarios found (slugs,
  masks, previews, case) is an operation, and character loops are where
  combining marks break.
- **Code points, not graphemes:** text is counted as `max_length` counts
  it and as PostgreSQL's `char_length` and Python do; graphemes are left
  out.

### Text formats

- **The sanitiser** is ammonia 4.2.4 (MIT or Apache-2.0). Beyond the
  three rules the page names (no scripts, no event handlers, links only
  to `https:` and `mailto:`), its policy is the most conservative choice.
  Ammonia brings Servo's `cssparser` and `dtoa-short` (MPL-2.0,
  file-level copyleft), accepted since they are unmodified, their source
  is available and they serve only the `style` attributes the policy
  drops.
- **Markdown** is rendered by pulldown-cmark 0.13.4 (MIT).
- **`RichText`** takes Portable Text's flat shape with its default
  schema because it is the most conservative documented shape.
- **An `InstantRange`'s `format`** joins its two ends by ` – `, CLDR's
  English interval fallback, until interval patterns are needed.

### Hashing and cryptography

- The eight operations (`encrypt`, `decrypt`, `hash_password`,
  `verify_password`, `hmac`, `verify_hmac`, `sign`, `verify_signature`)
  are in `jadpo-value`'s `crypto` module, with `public_key` beside them,
  on crates pinned exactly: `chacha20poly1305` 0.11.0, `argon2` 0.6.0,
  `hmac` 0.13.0 over `sha2` 0.11.1, and `ed25519-dalek` 3.0.0 with its
  precomputed tables.

### Signed tokens

- `crypto.verify_jwt` and `crypto.read_certificate` are in
  `jadpo-value`'s `jwt` module. RS256 and ES256 are on `rsa` 0.9.10 and
  `p256` 0.14.0 with its `ecdsa` feature, pinned exactly, both without
  `std`, so neither reads randomness and both build for wasm32.
- Both only verify: RUSTSEC-2023-0071 (Marvin), open against every `rsa`
  release, leaks a private key through the timing of decryption and
  signing, which nothing here does with RSA.

## Secrets, randomness, encodings and data formats

Behind the area page's "Secret values", "Security records", "Randomness", "Encodings" and `json` topics.

- **How secret values are built.** The seven secret values' entries read
  them from text and JSON and never write, show or store them
  (`TextForm::Secret`, `Json::Secret`, `FormatKey::Redacted`,
  `Storage::Never`). The runtime holds one as `jadpo-value`'s
  `Value::Secret`, with the standard type it is. It is built wherever a
  value of a type built on one is: a construction, a request's or
  `json.read`'s JSON, `text.parse`, a setting, and a literal in a test,
  which the plan keeps as `Constant::Secret`, its text printed
  `Hidden(..)`. `Display` writes it `<redacted ApiKey>` and `Debug` writes
  `Secret(ApiKey, redacted)`. A JSON writer that reached one anyway writes
  that text, and storage refuses it. The bytes for credentials are drawn
  through `Ctx::secret_random_bytes`, which keeps them off the tape.
- **How Security records is built.** `Hash`, `Hmac` and `EncryptedValue`
  are each built on `Bytes` (`Holds::Bytes`), and the proof that each is
  built from its own call is `security::made_by`. `WebAuthnCredential`'s
  parts are shaped to `webauthn-rs`'s stored `Passkey`, a `Credential`
  (its `CredentialV5`), so its tests build one from the other part by
  part (`jadpo-runtime`'s `tests/webauthn_reference.rs`), the reference
  filling in what isn't held as it loads one. Checking an answer runs on
  `webauthn-rs` 0.5.5, whose stored `Credential` is 0.5.2's. A `Bytes`
  part of a standard object is bounded by its length through
  `Part::Bounded`. A `Certificate`'s canonical text and JSON are its PEM
  (`TextForm::Pem`), read by `jadpo-value`'s `certificate` module on
  Rusticata's `x509-parser` 0.16.0 (pure Rust, built for wasm32), used
  without its `verify` feature, so with no `ring`. It is shown through
  `FormatKey::FirstPart`, which passes over empty text.
- **How Randomness is built.** `random` is declared and typed as the
  standard namespaces are (the stub `random.jadpo`), so its calls are
  checked as theirs are. What makes it generated is the operation
  table's `Op::reads_generator`, which the checker reads for
  `type.random_context` and each executor for the bytes it hands the
  operation. The native host reads the generator through `getrandom`
  0.4.3. `jadpo-value` never reads randomness itself, so it builds for
  wasm32 with no generator of its own.
- **How Encodings is built.** `hash.sha256` is RustCrypto's `sha2`, pure
  Rust on every target, tested against NIST's FIPS 180-4 examples
  natively and under WebAssembly.
- **How `json` is built.** `json.read` reads through the request body's
  codec, `jadpo-plan`'s `JsonTable`, numbered per call as the route
  table's are.
- **`random.token` is in `random`, not `crypto`,** because the test
  seed is: `random` is what a test seeds.
- **No base32 operation.** No scenario needs base32 but TOTP, which is a
  package.
- **No compression operation.** gzip is allowed only inside the file
  readers (the batch helpers), with `max_unpacked_size:`.
- **No dynamic JSON value.** No scenario needs a JSON value of no type: a
  payload whose shape isn't known is kept as `Text` and passed on.
- **`DecodeProblem` added no error code.** A failure that declares
  `problem: DecodeProblem` is checked with the existing
  `failure.duplicate_context_field`, `failure.missing_context_field` and
  `type.not_record`.

## Geospatial and AI on pages of their own

[Geospatial](../20-geospatial.md) has its own page because new stored
types, PostGIS, query predicates and indexes touch areas 2, 5 and 6. The
standard library's page keeps only the 17 types themselves.

AI models are provider packages (area 11). What makes them first class in
typed applications is a set of standard pieces Jadpo itself does, as a
capability: Jadpo renders the prompt from a checked file and decodes the
answer into a type, as it renders an email and hands it to a sender.
Training models is left out because it needs GPUs, datasets and long
jobs, none of which an application request does, so a package or an
outside service trains, and the app calls the result.

## Gates and bits

No logic gate beyond `and`, `or` and `not` is added, since any other is a
one-line function. Bitwise operations on `Int` and `Bytes` (and, or, xor,
not, shifts) are left out: flags are a `Set` of an enum, which a reviewer
can read, and the one scenario with bit-packed data, device payloads,
decodes them in the provider package that knows the format.

## Scenario coverage

The nine scenario applications on the
[area page](../19-standard-library.md#scenario-applications) were used to
find what the standard library needs. The coverage matrix follows; each
gap is fixed on the area page or left out with a reason.

● covered on this page or by another area; ◐ waits on keys held as data
("Language gaps"), not built yet; blank: not needed.

| Group | Shop | Bookings | Delivery | Property | SaaS | CMS | IoT | Admin | AI |
|---|---|---|---|---|---|---|---|---|---|
| `list` | ● | ● | ● | | ● | ● | ● | ● | ● |
| `set` | | ● | | ● | ● | ● | ● | ● | |
| `map` | | | | | ● | | ● | | |
| Cutting either end | | | ● | ● | | ● | | ● | ● |
| `integer`, `decimal` | ● | | | ● | ● | | ● | | |
| `Decimal` maths | | | ● | ● | | | ● | | |
| `Money` | ● | | | ● | ● | | | | |
| `text` | ● | | | ● | | ● | | ● | ● |
| `hash`, `encoding` | ● | ● | ● | | ● | | | | |
| `crypto` | | | | | ● | | ◐ | | |
| `random` | ● | ● | ● | | | | | | |
| `json` in memory | | | | | ● | | | ● | ● |
| File formats | ● | | ● | ● | | ● | ● | ● | |
| `temporal` | | ● | ● | | ● | | ● | | |
| Construction | ● | ● | ● | ● | ● | ● | ● | ● | ● |
| Cross-field rules | | ● | | | ● | | | | |
| Aggregate queries | | | | | ● | | ● | ● | |
| Geospatial | | | ● | ● | | | | | |
| AI | | | | | | | | | ● |

**What covers each need the scenarios found:** human-readable
references, `random.code`; money in a currency known only while running,
`Money` with proof by condition; searching sets, `set.*`; grouping and
the latest per key, `list.group_by` and `list.last`; spread and distance,
`Decimal` maths; JSON in memory, `json.read` and `json.write`; locale
order for editors, `locale:` on `sort_by`; cross-field rules, aggregate
queries and keys held as data, the language changes below; geospatial
and AI, areas 20 and 21. Left out with a reason: bits (device packages),
TOTP (a package), calendar files (a package).

## Language gaps this page depends on


Nothing is left out because the language can't do it: each gap is
either left out by design (with its reason, above and in the next table)
or closed by a language change, with the operations it unblocks on this
page. Only `crypto.verify_totp` and HS256 are not built yet
([the roadmap](../../roadmap.md)).

| Gap | Fix | Owning area | Unblocks |
|---|---|---|---|
| Walking sets and maps | `for` takes a `Set`, in area 2's canonical order; maps through `map.keys` | 2, 3 | Walking sets and maps |
| A map literal | `[]` where a map is expected; area 2 has no literal with entries | 2 | Building maps |
| Inline functions of two values | Two parameters, `(a, b) -> ...`, for standard operations only (area 3) | 3 | `list.zip_with`, `map.filter`, `map.map_values` |
| Maths without a `Float` | Maths on `Decimal` with `scale:` and `rounding:`, deterministic everywhere; still no `Float` | 2 | 8 `decimal` operations; geospatial distance |
| Using a secret without reading it | Opaque key settings, used only as a `crypto` call's `key:` | 10 | `crypto` |
| Keys held as data (per device, per person) | A key-type field, kept sealed, or a `var` holding `crypto.new_key`, usable only as `key:`; built. "Code never makes a key" narrowed to "code never builds a key from a value it can see" (Richard, 2026-11-01). `crypto.verify_totp` and HS256 are the next chunk | 10, 15 | Per-device signatures, `crypto.verify_totp`, `crypto.verify_jwt` with a shared secret (HS256), storing a `TotpSecret` or a `SecretText` |
| Randomness | Generated `random`, actions only, seeded in tests, recorded for replay | 10, 13, 17 | `random.*`, `encrypt`, `hash_password` |
| Locale order and case | `locale:` on `list.sort_by`, `text.lower` and `text.upper`, through ICU4X's collator and case mapping (two more crates of the pinned ICU4X) | 10 | Locale sorting and case |
| Values formatted for people in code | The contract's `text.format`, giving `PresentationText`, for every standard type | 10 | Showing numbers, money, quantities and addresses in code |
| Parsing from text | The contract's `text.parse(raw, as: T)`, canonical forms only | 2 | Text from a person or a file cell |
| Queries that compute | Aggregate queries: `count`, `sum`, `min`, `max`, `average`, grouped by a field or a time bucket, using operations with SQL forms | 6 | Reports, dashboards, usage totals |
| Rules across fields | Named rules on an object type (`rule ends_after_start: ends > starts`), proved or attempted like any rule | 2 | Bookings, date ranges, price bands |
| Normalised text from outside | NFC applied as text is decoded at a boundary | 2, 7 | Equal text comparing equal |
| Money as a language type | Built-in `Money` and `Currency` | 2 | `money` |
| Geospatial types | [Area 20](../20-geospatial.md) | 20 | Geospatial |
| AI pieces | [Area 21](../21-ai.md) | 21 | AI |

### Left out by design

| Left out | Why | Instead |
|---|---|---|
| Generic functions in your own code | Area 3 leaves them out: no type-parameter errors for agents to fight | The standard library stays generic |
| `reduce`, `fold` | A second way to accumulate | `for` with `var mut`; `list.sum` |
| Comparators | A key expresses every order, and an inconsistent comparator differs by target | `sort_by`, `reverse` |
| `items[i]` | Out of range needs an arm or a defect | `list.at` gives `T?` |
| Regular expressions as calls | A pattern is a type's rule, in linear time | `pattern:` and construction (area 2) |
| Graphemes | They'd disagree with `max_length` and PostgreSQL, and change with each Unicode version | Code points |
| Dynamic keys on records | A field is its own type and carries its own policy and marks | `Map` |
| A dynamic JSON value | No scenario needs one | `json.read(..., as:)`; `Text` to pass on |
| CSV in memory | One way to read rows | `csv.read` through a store |
| MD5, SHA-1 | Broken | A package (area 11) |
| Bitwise operations | Flags read better as a set; device formats belong in their package | `Set<Enum>` |
| Fetching exchange rates | Outside data with a time and a source | A provider package; `money.convert` applies a rate the app holds |
| Gender, sex at birth | Contested, and personal data | The app's own type, marked sensitive (area 15) |
| CMYK and print colour | Needs a colour profile | A package on the standard `Colour` |
| Training models | Needs GPUs, datasets and long jobs | A package or an outside service |
| Aliases (`size` for `count`) | One obvious name each | The table's name |

## How the five languages compare


What TypeScript, Rust, Go, Elixir and Python do for the same jobs, and the
choice each row led to. Rust's and Go's numbers are for their standard
libraries only.

| Job | TypeScript | Rust | Go | Elixir | Python | Jadpo |
|---|---|---|---|---|---|---|
| Text length | UTF-16 units | bytes (`len`) | bytes (`len`) | graphemes | code points | code points, as `max_length` |
| Cutting from the end | `slice(-2)`, but `substring(-2)` is from the start | byte ranges | `s[len(s)-2:]` | `String.slice(s, -2..-1)` | `s[-2:]` | `keep_last(x, 2)`, no negatives |
| Rounding a number | `Math.round`, ties toward +∞ | `round`, ties away from zero | `math.Round`, ties away from zero | `Float.round`, ties up | `round`, ties to even | named every time, `rounding:` |
| Exact decimals | none | none | none | library | `decimal` | built in, with `scale` |
| Money | libraries disagree on shape | crates | libraries | `Money` library | libraries | built in, minor units and currency |
| Sorting | in place; default compares as strings | `sort_by_key` | `slices.SortFunc` | `Enum.sort_by` | `sorted(key=)` | `list.sort_by(items, key)`, a new list |
| Set operations | `union` etc. new in 2024 | `HashSet` methods | none | `MapSet` | operators | `set.union` etc. |
| SHA-256 | `crypto.subtle`, asynchronous | none | `crypto/sha256` | `:crypto.hash` | `hashlib` | `hash.sha256`, pure |
| Password hashing | libraries | crates | `x/crypto` | libraries | libraries | `crypto.hash_password`, Argon2id fixed |

What carried over: Go's and Elixir's namespaces by subject (`strings`,
`MapSet`), which read like Jadpo's; Python's code-point length; Python's
`decimal`, whose context names its rounding; Stripe's minor units. What
didn't: TypeScript's string-comparing default sort and asynchronous
hashing, negative positions, Rust's and Go's byte lengths, and the aliases
and second ways every one of them has.
