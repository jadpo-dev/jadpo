---
title: Standard library
area: 19
status: proposed
checked-with: nothing yet; the examples are fragments until chunk 5.5 builds them
rules: []
---

# Standard library

The standard library is the set of operations every program can call
without declaring them: `collection.count(items)`, `temporal.in_zone(at,
zone)`. Areas 2, 3 and 10 each decided the few their own features needed.
This page collects them, adds what real code keeps reaching for (splitting
and trimming text, rounding and converting numbers, hashing, encodings),
and says what is left out on purpose and why.

> Proposed, not decided (chunk 5.4, 2026-10-17). Nothing on this page beyond what areas 2, 3 and 10 already decided is built; chunk 5.5 builds what Richard decides here. Where this page and a decided area disagree, the area wins, and the disagreement is a mistake on this page.

| Namespace | Holds | Operations | Decided in |
|---|---|---|---|
| `collection` | Operations on lists | 14 (4 decided, 10 new) | [Area 2](02-values-and-types.md#collections), question 4; inline functions in [area 3](03-logic.md#passing-functions) |
| `integer` | Operations on values built on `Int` | 5 (1 decided, 4 new) | [Area 3](03-logic.md), question 7 |
| `decimal` | Operations on values built on `Decimal` | 5 (1 decided, 4 new) | [Area 3](03-logic.md), question 7 |
| `text` | Operations on values built on `Text` | 14, new | This page |
| `hash` | SHA-256 digests | 1, new | This page |
| `encoding` | Bytes as text and back | 9, new | This page |
| `temporal` | Instants, dates, local times, zones, formats | 21, unchanged | [Area 10](10-configuration-and-time.md#zones-and-local-times) |
| `locale` | Choosing a supported locale | 1, unchanged | [Area 10](10-configuration-and-time.md#choosing-a-locale-from-a-request) |

70 operations in all: 28 decided, 42 proposed here.

## What every operation follows

These hold for every namespace, and are what the decided areas already
apply to `collection`, `decimal`, `integer` and `temporal`.

- **Always called through its namespace** ([area 1](01-project-layout-and-modules.md)):
  `text.trim(name)`, never `trim(name)`. A namespace's name can't name a
  declaration, domain, library, parameter or variable; a field can still be
  called `text`, since `note.text` is never read as the namespace.
- **Pure.** Each gives the same result for the same arguments, in the
  interpreter and on every compiled target. None reads the clock,
  configuration, randomness, the network or the database, so every one may
  be called in a function. Unicode data (case mapping, whitespace, the
  slug's decomposition) is the version pinned in `jadpo-value`, and
  upgrading it is a reviewed change, as the time zone database is.
- **Representations in, representations out**
  ([area 3](03-logic.md), operators). A parameter written `Text` takes any
  value built on `Text`, `Email` and `Url` included, and a result written
  `Text` is plain text, turned into a named type by constructing it. A
  result keeps the argument's named type only where the operation returns
  one of its arguments or their items unchanged (`integer.max`,
  `collection.filter`), since only then can no rule break.
- **Named arguments only where a value needs saying** (area 3): the first
  arguments are positional, and options and modes are named, as `rounding:`
  is today.
- **Failures are typed** ([area 4](04-failures.md)). An operation that can
  fail names each condition it fails on, and the call's `attempt` maps it
  by that name, as `dst:` and `invalid_day:` are mapped today
  ([area 10](10-configuration-and-time.md#zones-and-local-times)): the
  failure is `invalid_value`, and `invalid:` covers every condition without
  its own arm. Overflow is a defect, as it is for `+`. Where a bad argument
  can be ruled out while compiling, it is, as `/` rules out a zero divisor:
  the table's `never_zero` gains `never_empty` (a literal isn't `""` and a
  value's type has a `min_length` of at least 1) and `never_negative` (a
  literal isn't below 0 and a value's type has a `min` of at least 0).
- **Facts for proofs.** A result whose type can't say its bounds carries
  them in the table, as `collection.count`'s `at_least: 0` does today
  ([area 2](02-values-and-types.md)), so constructing from it needs no arm
  for a rule it can't break. New here: `at_most_length_of`, for a text
  result never longer than an argument (`text.trim`, `text.slice`), so
  `Title(text.trim(title))` can't break `Title`'s `max_length`, though it
  can still break `min_length` or a `pattern`.

```jadpo fragment
text.trim(input.title)                                   // "  Rye loaf " gives "Rye loaf"
text.split("flour,sugar,,eggs", ",")                     // ["flour", "sugar", "", "eggs"]
text.pad_start(text.from(order.number), 6, fill: "0")    // 42 gives "000042"
collection.sum(collection.map(basket.lines, line -> line.price * line.count))
encoding.to_hex(hash.sha256(upload.body))                // 64 lowercase hexadecimal digits
attempt encoding.from_base64(input.attachment) {
    malformed: AttachmentUnreadable
}
```

## SQL forms

A **SQL form** is a PostgreSQL 16 expression that gives exactly the result
the runtime gives, for every argument. It matters in one place: a computed
backfill whose every operation has a SQL form runs as one `UPDATE` per
batch, and any other runs through `jadpo migrate apply`, reading and
writing each batch ([area 5](05-data.md#computed-backfills)). Both are
correct; the SQL form is only faster. Queries still compute nothing and a
one-step update's `set:` still works a stored field out only with `+` and
`-` (area 6), so neither calls an operation in SQL (question 9).

- An operation that can fail has no SQL form, since a statement can't map
  a failure to a row.
- Text is compared and ordered in code points, as queries sort it
  (area 6, with `COLLATE "C"` indexes, area 5), so a SQL form never
  depends on the database's collation. That rules out `lower` and
  `upper`: PostgreSQL 16's follow the collation, and under `"C"` change
  only ASCII letters.
- A list is stored as `jsonb` (area 5); only `collection.count` has a SQL
  form.
- Each SQL form is tested against the runtime on the same inputs, Unicode
  edge cases included, before a backfill may use it.

In the tables below, `x` is the first argument and `a`, `b` the others.

## Collections

The four decided operations, and ten more, each called as
`collection.<name>`. All take lists only: sets and
maps can't be walked yet ([area 2](02-values-and-types.md#collections),
question 10 here). `T` is the item type, fixed by the list. A function
argument is written inline, `item -> expression` (area 3).

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `count(items)` | `Int`, at least 0 | | `jsonb_array_length(x)` |
| `contains(items, item)` | `Bool`: an item `==` `item` | | none |
| `map(items, each)` | `List<U>`, in order | | none |
| `filter(items, keep)` | `List<T>`, the items `keep` accepts, in order | | none |
| `sum(items)` | `Int` or `Decimal`, the items added; `0` for none. Items built on `Int` or `Decimal` | overflow is a defect | none |
| `any(items, test)` | `Bool`: `test` accepts at least one item; `false` for none | | none |
| `all(items, test)` | `Bool`: `test` accepts every item; `true` for none | | none |
| `find(items, test)` | `T?`, the first item `test` accepts | | none |
| `first(items)` | `T?`, the first item | | none |
| `sort_by(items, key)` | `List<T>`, ordered by `key` ascending; equal keys keep their order. `key` gives a number, a time, a date, a duration or text, text in code-point order as queries sort it | | none |
| `reverse(items)` | `List<T>`, last item first | | none |
| `distinct(items)` | `List<T>`, each item's first occurrence, by `==` | | none |
| `slice(items, from, count)` | `List<T>`, up to `count` items from position `from` (0 is the first), cut short at the end. Both never negative | | none |
| `concat(first, second)` | `List<T>`, `first`'s items then `second`'s | | none |

- **One way each.** Descending is `reverse(sort_by(...))`; the largest is
  `first(reverse(sort_by(...)))`, and in a query it is `order:` and
  `limit:`. Adding one item is `concat(items, [item])`. A sum of a field is
  `sum(map(...))`.
- **`sum` gives a representation**, as `+` does, so `Pence(collection.sum(prices))`
  builds it back, with an arm for `max` when the type has one.
- **Not here:** `reduce` and `fold` (a `for` with a `var mut` is the one
  way to accumulate, and inline functions take one parameter); an index
  (`items[2]`, `slice` and `first` instead); `min_by`, `max_by`, `last`,
  `group_by`, `zip` and `flatten` (each is `sort_by`, `reverse`, a loop or a
  query, and none has come up in an example).

## Numbers

`integer` operations take values built on `Int`, and `decimal` operations
values built on `Decimal`. `/` and `%` stay as area 3 decided; these cover
what an operator can't.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `integer.divide(dividend, divisor, rounding:)` | `Int` (decided, area 3) | | every mode: `div` and `mod` with a `CASE` |
| `integer.abs(value)` | `Int`, at least 0 | overflow on the smallest `Int` is a defect | `abs(x)` |
| `integer.min(a, b)`, `integer.max(a, b)` | the smaller or larger, keeping their named type (both the same, as `+` needs) | | `LEAST(a, b)`, `GREATEST(a, b)` |
| `integer.from_decimal(value, rounding:)` | `Int`, `value` rounded to a whole number | overflow is a defect | `round(x)::bigint` for `half_up`; a `CASE` on the dropped digits for the others |
| `decimal.divide(dividend, divisor, rounding:)` | `Decimal` (decided, area 3) | | every mode: as `integer.divide`, on the values scaled to the dividend's places |
| `decimal.abs(value)` | `Decimal`, at least 0, the same scale | | `abs(x)` |
| `decimal.min(a, b)`, `decimal.max(a, b)` | as for `integer` | | `LEAST(a, b)`, `GREATEST(a, b)` |
| `decimal.round(value, scale:, rounding:)` | `Decimal` with at most `scale` places; `scale:` a whole number written out, at least 0 | | `round(x, s)` for `half_up`; a `CASE` on the dropped digits for the others |

- **Rounding to a type's scale needs no call.** Constructing into a type
  with a fixed `scale` rounds half to even (area 2), so `decimal.round` is
  for another mode or fewer places than the type keeps.
- **An `Int` becomes a `Decimal` by multiplying,** `count * 1.00`, since
  `*` may mix the two (area 3), so there is no `decimal.from_integer`.
  The other way had no form, hence `integer.from_decimal`.
- **PostgreSQL's `round` ties away from zero,** which is `half_up`; the
  other modes are spelled out with a `CASE`, never left to the database,
  which would round its own way (area 6, stored sums).
- **`Rounding` stays `half_even`, `half_up` and `up`,** unless question 4
  adds `down`.
- **Not here:** `pow`, `sqrt`, logarithms and trigonometry (no `Float`,
  area 2, question 8); parsing a number from text (question 7); showing a
  number to a person, which is a translation's `{n, number}`
  ([area 10](10-configuration-and-time.md#translations)), with currencies
  and percentages left out there on purpose (question 11).

## Text

Each operation is called as `text.<name>`. Text is counted, cut and
padded in Unicode code points, as `max_length`
counts it (area 2) and as PostgreSQL's `char_length` does (question 2).
`PresentationText` is its own representation (area 10), so none of these
take it.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `length(value)` | `Int`, at least 0, in code points | | `char_length(x)` |
| `trim(value)` | `Text` without Unicode white space at either end; never longer | | `btrim(x, chars)`, `chars` the Unicode `White_Space` characters |
| `lower(value)`, `upper(value)` | `Text` in Unicode's default case mapping, the same in every locale | | none |
| `contains(value, part)` | `Bool` | | `strpos(x, a) > 0` |
| `starts_with(value, prefix)`, `ends_with(value, suffix)` | `Bool` | | `starts_with(x, a)`; `right(x, char_length(a)) = a` |
| `replace(value, find, with)` | `Text` with every `find` replaced, left to right; `find` never empty | | `replace(x, a, b)` |
| `split(value, separator)` | `List<Text>`, at least one item; `separator` never empty; `""` gives `[""]`, and separators side by side give `""` between them | | none |
| `join(items, separator)` | `Text`, the items with `separator` between them | | none |
| `slice(value, from, count)` | `Text`, up to `count` code points from position `from` (0 is the first), cut short at the end; both never negative; never longer | | `substr(x, a + 1, b)` |
| `pad_start(value, length, fill:)` | `Text` of at least `length` code points, `fill` added at the start; never cut; `fill:` one character written out | | `CASE` on `char_length(x)` with `lpad` |
| `from(value)` | `Text`, the value as JSON carries it (areas 2 and 10): an `Int`'s digits, a `Decimal` with its scale's places, a `Uuid` in lowercase, `true` or `false`, a time in its wire form | | `x::text` for `Int`, `Decimal`, `Uuid` and `Bool`; times none |
| `slug(value)` | `Text` of lowercase ASCII letters, digits and single `-`: accents dropped, every other run of characters one `-`, none at either end; may be empty | | none |

- **Matching is a type's `pattern`.** Whether text looks like a postcode
  is a construction, `attempt Postcode(raw) { pattern: ... }` (area 2), so
  there is no `text.matches`, and no replacing or capturing by regular
  expression.
- **Joining two pieces is `+`** (area 3); `join` is for a list.
- **`from` takes no enum and no text.** An enum has no display label (area
  2), so showing one is a `match`; a value built on `Text` already is text.
- **`slice` can split a character a person sees as one,** such as an
  emoji with a skin tone, which is two code points. A preview cut this way
  is still valid text; graphemes are left out (question 2).
- **Not here:** `trim_start`, `trim_end` and `pad_end` (no example needs
  them); case folding and locale-aware case (Turkish `İ`), which need a
  locale and differ between databases; Unicode normalisation (whether text
  is normalised as it arrives is a question for area 2, not a call);
  iterating characters; comparing or sorting text by a locale's rules
  (question 6).

## Hashing

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `hash.sha256(value)` | `Bytes`, the 32-byte SHA-256 digest of a value built on `Bytes`, or of a `Text`'s UTF-8 bytes | | `sha256(x)`, or `sha256(convert_to(x, 'UTF8'))` for text |

The digest is `Bytes`, and `encoding` writes it as text: `encoding.to_hex`
for an ETag or a content key, `to_base64` where another system asks for
that (question 3).

- **No keyed hash, HMAC or signature.** A key is a secret, and code never
  reads a secret ([area 10](10-configuration-and-time.md#secrets)): a
  webhook's signature is its connection's `SignatureScheme` (area 11), a
  session's or token's is the runtime's (area 8), and a log's keyed hash of
  text is the runtime's (area 4).
- **No password hashing,** which belongs to signing in (area 8, area 11's
  providers), and **no encryption**, which would need a key.
- **No MD5 or SHA-1.** Both are broken; an outside system that still asks
  for one is reached through a package (area 11).

## Encodings

Each operation is called as `encoding.<name>`.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `to_base64(bytes)` | `Text`, standard alphabet, padded | | `translate(encode(x, 'base64'), E'\n', '')` |
| `from_base64(text)` | `Bytes` | `malformed` | none |
| `to_base64url(bytes)` | `Text`, URL alphabet, unpadded, as area 8's signed parts are | | `rtrim(translate(encode(x, 'base64'), E'+/\n', '-_'), '=')` |
| `from_base64url(text)` | `Bytes`; padding refused | `malformed` | none |
| `to_hex(bytes)` | `Text`, lowercase | | `encode(x, 'hex')` |
| `from_hex(text)` | `Bytes`; either case | `malformed` | none |
| `to_utf8(text)` | `Bytes` | | `convert_to(x, 'UTF8')` |
| `from_utf8(bytes)` | `Text`; invalid UTF-8 refused, never replaced | `malformed` | none |
| `to_url_component(text)` | `Text` with everything but RFC 3986's unreserved characters percent-encoded as UTF-8 | | none |

- **`malformed`** is the one condition decoding fails on, so a call maps
  it as `malformed: AttachmentUnreadable`, or `invalid:` covers it.
- **There is no `from_url_component`:** the runtime decodes paths and
  query strings before code sees them (area 7). `to_url_component` is for
  a link code builds, such as an email's sign-in link with a value in its
  query string (area 16).
- **Not here:** JSON (typed and decoded only at a boundary, areas 7 and
  11); CSV, XML and compression; base32.

## What is left out, and why

| Left out | Why | Instead |
|---|---|---|
| Randomness | No operation reads anything but its arguments; identities and tokens come from the runtime | A new `Uuid` on create (area 5), tokens (area 8); question 8 |
| The clock, configuration, locale data reads | Already rules of their own | `clock.now` in actions, `config.<name>`, `temporal.format` (area 10) |
| Regular expressions as calls | A pattern is a type's rule, in linear time | `pattern:` and construction (area 2) |
| Parsing numbers and times from text | Text from outside is decoded at the boundary by its type | Typed inputs and responses (areas 2, 7, 11); question 7 |
| Keys, HMAC, encryption, password hashing | Code never reads a secret | The runtime and connections (areas 8, 10, 11) |
| `reduce`, `fold`, indexing | One way to accumulate | `for` with `var mut` (area 3) |
| Floating point and its maths | No `Float` (area 2, question 8) | `Decimal` |
| Formatting numbers, currencies and percentages for people | Area 10 left them out of messages | `{n, number}` in a translation |
| Locale-aware sorting and case | Needs a locale and differs between targets and databases | Code-point order, as queries sort |
| Aliases (`size` for `count`, `strip` for `trim`) | One obvious name each | The table's name |
| JSON, CSV, XML, URL building | Each is a boundary or a package | Areas 7, 11 and 16 |

## How the five languages compare

What TypeScript, Rust, Go, Elixir and Python do for the same jobs, and the
choice each row led to. Rust's and Go's numbers are for their standard
libraries only.

| Job | TypeScript | Rust | Go | Elixir | Python | Jadpo |
|---|---|---|---|---|---|---|
| Text length | UTF-16 units | bytes (`len`) | bytes (`len`) | graphemes | code points | code points, as `max_length` |
| Trim, case, split, replace | methods | methods | `strings` | `String` | methods | `text` |
| Rounding a number | `Math.round`, ties toward +∞ | `round`, ties away from zero | `math.Round`, ties away from zero | `Float.round`, ties up | `round`, ties to even | named every time, `rounding:` |
| Exact decimals | none | none | none | library | `decimal` | built in, with `scale` |
| Sorting | in place; default compares as strings | `sort_by_key` | `slices.SortFunc` | `Enum.sort_by` | `sorted(key=)` | `sort_by(items, key)`, a new list |
| SHA-256 | `crypto.subtle`, asynchronous | none | `crypto/sha256` | `:crypto.hash` | `hashlib` | `hash.sha256`, pure |
| Base64 and hex | `btoa` (Latin-1 only), `Buffer` in Node | none | `encoding/base64`, `hex` | `Base` | `base64`, `binascii` | `encoding` |

What carried over: Go's and Elixir's namespaces by subject (`strings`,
`Enum`), which read like Jadpo's; Python's code-point length; Python's
`decimal`, whose context names its rounding. What didn't: TypeScript's
string-comparing default sort and asynchronous hashing, Rust's and Go's
byte lengths, and the aliases and second ways every one of them has
(TypeScript cuts text with `slice`, `substring` and `substr`).

## Review

### Status

| Feature | Status |
|---|---|
| `collection.count`, `contains`, `map`, `filter` | Decided (area 2) and built |
| `integer.divide`, `decimal.divide` | Decided (area 3) and built |
| `temporal`, `locale` | Decided (area 10) and built; unchanged here |
| Everything else on this page | Proposed (chunk 5.4); built in chunk 5.5 once decided |
| SQL forms | Proposed; built with computed backfills (chunk 5.6), their only user |

### Consistency with the rest of the language

- **Area 1:** `text`, `hash` and `encoding` become reserved namespaces
  (question 1). Two page examples then need renaming: area 8's `var text:
  Memo` (a checked example) and area 16's query parameter `text:
  SearchText` (a fragment). Fields called `text` (areas 8 and 9) are
  unaffected.
- **Area 2:** `collection` grows by ten operations under question 4's
  pattern; collections stay invariant. `max_length` counting code points is
  why text is counted that way here.
- **Area 3:** inline functions stay arguments to collection operations
  only (question 13); `any`, `all`, `find` and `sort_by` are such
  operations. Area 3 question 6's "text order belongs in a standard
  function" is `sort_by` with a text key, in code-point order.
- **Area 4:** decoding failures are conditions mapped by name, as area 10's
  `dst:` is, and overflow stays a defect.
- **Area 5:** a computed backfill's one-`UPDATE` path uses the SQL forms
  here.
- **Area 6:** unchanged. Queries compute nothing, and one-step updates
  work a stored field out only with `+` and `-`.
- **Area 10:** a secret stays unreadable, which is why there is no keyed
  hash; number formatting for people stays in translations.
- **Compiler design** (`docs/compiler-runtime.md`, the standard library):
  the operation table gains `never_empty`, `never_negative`,
  `at_most_length_of`, a literal-only whole number (`scale:`) and character
  (`fill:`), a condition no option settles (`malformed`), a `sort_by` key
  limited to ordered kinds, and a SQL form per operation.

### Open questions

1. **Are the new namespaces `text`, `hash` and `encoding`, at the cost of
   reserving those names?** A namespace can't be a parameter or variable
   name (area 1), and `text` is one an author reaches for. Alternatives:
   `strings` (Go's, but a plural nothing else uses), `digest` for `hash`
   (as common a variable name). *Recommendation:* `text`, `hash` and
   `encoding`. They are the words an author writes the call with, a
   variable called `text` says nothing about what it holds, the error
   already names the fix, and fields are unaffected. Two page examples are
   renamed in 5.5.
2. **Is text counted and cut in code points?** Graphemes (what a person
   sees as one character) would make `slice` safe for emoji, but then
   `text.length` and `max_length` would disagree, the count would change
   with each Unicode version, and PostgreSQL can't count them.
   *Recommendation:* code points everywhere, as Python and PostgreSQL do,
   and graphemes left out.
3. **Does `hash.sha256` give `Bytes`, or hexadecimal text?** Most callers
   want hex, which with `Bytes` is two calls. *Recommendation:* `Bytes`,
   written as text by `encoding`. One digest serves hex, Base64 and
   comparison with other bytes, and the hash namespace doesn't grow a
   variant per encoding.
4. **Does `Rounding` gain `down`, toward zero?** `decimal.round` and
   `integer.from_decimal` can't truncate without it (`Int` `/` already
   drops the remainder, so it is the one mode the operators have and the
   calls don't). *Recommendation:* yes, `Rounding.down`, with SQL form
   `trunc`. `floor`, `ceiling` and `half_down` stay out until amounts below
   zero need them.
5. **Is `text.slug` in, with an ASCII-only result?** Code can't write it
   itself: there is no character loop or regular-expression replace. ASCII
   folding drops letters with no ASCII form, so a title in Greek gives
   `""`, which a slug type's `min_length` turns into a failure to map.
   *Recommendation:* yes, ASCII-only, as most sites' URLs are; keeping
   other scripts' letters can be a later option.
6. **May `collection.sort_by`'s key be text, in code-point order?**
   Area 3 left text order to a standard function, and queries already sort
   text in code-point order (area 6), so `"Zebra"` comes before `"apple"`.
   *Recommendation:* yes, matching queries. A locale's order (`"apple"`
   first, accents beside their letters) waits for a screen that needs it,
   and would take a `locale:`.
7. **Is there no parsing of numbers from text?** Text from outside is
   decoded by its type at the boundary, and no example has needed more.
   *Recommendation:* leave it out. If a file import needs it,
   `integer.parse` and `decimal.parse` failing `malformed` fit this page
   without changing anything else.
8. **Is randomness left out?** Short codes, shuffles and sampling need it,
   and area 17 already lists randomness as an effect to record for replay.
   *Recommendation:* left out of this page. If an application needs it, a
   generated namespace like `clock`, readable only in actions and replaced
   in tests, decided with area 17.
9. **Are SQL forms used only by backfills?** One-step updates could use
   them too (`set: { title: text.trim(Todo.title) }` as one statement), and
   queries with an expression index. *Recommendation:* backfills only, for
   now. Area 6's one-step updates and value-only queries stay as decided,
   and the record is loaded where they don't apply.
10. **Do collection operations take only lists?** Sets can't be walked yet
    (area 2). *Recommendation:* lists only in 5.5. When `for` walks a set,
    every operation here that ignores order (`count`, `contains`, `any`,
    `all`, `sum`, `filter`, `map`) takes one too.
11. **Do currencies and percentages stay out?** *Recommendation:* yes, as
    area 10 decided for messages, until the language has a money type that
    knows its currency.
12. **Does the table record `at_most_length_of`?** It saves an arm on
    `Title(text.trim(title))` for `max_length`, at the cost of one more
    fact in the checker. *Recommendation:* yes, for `trim` and `slice`
    only. `lower` and `upper` can lengthen text (`ß` becomes `SS`), and
    `replace` and `pad_start` do.

### Building it

Chunk 5.5 as written is more than one session: 42 operations, each with
its stub, table entry, Rust function and page examples as tests, plus the
new facts the table must carry. Proposed split:

- **5.5a Collections and numbers** `opus · high`: the ten `collection`
  operations and the eight `integer` and `decimal` ones; `never_negative`,
  the `sort_by` key's ordered kinds, results that keep a named type, and
  `scale:` written out; `Rounding.down` if question 4 says so.
- **5.5b Text, hashing and encodings** `opus · high`: the 24 `text`,
  `hash` and `encoding` operations; `never_empty`, `fill:` written out,
  `at_most_length_of`, the `malformed` condition and its mapping; the
  namespaces reserved and the two page examples renamed.
- **SQL forms move to 5.6**, computed backfills, their only user: each
  operation's SQL form, tested against the runtime on PostgreSQL.
- **5.7** brings the teaching examples up to date, as planned, with
  `sum` in place of the summing loops in examples 04 and 09.
