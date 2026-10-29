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
