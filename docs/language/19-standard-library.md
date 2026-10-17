---
title: Standard library
area: 19
status: proposed
checked-with: nothing yet; the examples are fragments until chunk 5.5 builds them
rules: []
---

# Standard library

The standard library is the set of types and operations every program can
use without declaring them: `Money`, `list.count(items)`,
`temporal.in_zone(at, zone)`. Areas 2, 3 and 10 each decided the few their
own features needed. This page collects them, adds what real applications
keep reaching for, says which gaps in the language block the rest and how
to close them, and says what is left out on purpose and why.

> Proposed, not decided (chunk 5.4, revised 2026-10-17 after Richard's review). Nothing on this page beyond what areas 2, 3 and 10 already decided is built; chunk 5.5 builds what Richard decides here. Where this page and a decided area disagree, the area wins, and the disagreement is a mistake on this page. The packages design it builds on ([#88](https://github.com/richardwillars/jadpo/pull/88)) is not merged yet.

| Namespace | Holds | Operations | Decided in |
|---|---|---|---|
| `list` | Operations on `List<T>` (was `collection`) | 26 (4 decided, 22 new) | [Area 2](02-values-and-types.md#collections), question 4; inline functions in [area 3](03-logic.md#passing-functions) |
| `set` | Operations on `Set<T>` | 7, new | This page |
| `map` | Operations on `Map<K, V>` | 10, new | This page |
| `integer` | Operations on values built on `Int` | 7 (1 decided, 6 new) | [Area 3](03-logic.md), question 7 |
| `decimal` | Operations on values built on `Decimal` | 15 (1 decided, 14 new) | [Area 3](03-logic.md), question 7 |
| `money` | `Money` in one currency | 10, new | This page |
| `text` | Operations on values built on `Text` | 18, new | This page |
| `hash` | SHA-256 digests | 1, new | This page |
| `encoding` | Bytes as text and back | 9, new | This page |
| `crypto` | Encryption, passwords, message authentication | 6, new | This page |
| `random` | Tokens, codes, shuffles; actions only | 5, new | This page |
| `json` | One JSON document in memory; large files through #88's batches | 2, new | This page and #88 |
| `temporal` | Instants, dates, local times, zones, formats | 23 (21 decided, 2 new) | [Area 10](10-configuration-and-time.md#zones-and-local-times) |
| `locale` | Choosing a supported locale | 1, unchanged | [Area 10](10-configuration-and-time.md#choosing-a-locale-from-a-request) |

140 operations in all: 28 decided, 112 proposed here. Of the new ones, 3
wait on a language change this page proposes (two-parameter inline
functions) and 8 on the choice of maths on `Decimal` (question 13). #88's
file helpers (`csv`, `tsv`, `json_lines` and the batch forms of `json`)
are counted there, not here.

## Shared vocabulary types

> Decided 2026-10-17 (Richard): the principle, and the warning on a package that declares its own equivalent. The types in the table below are still proposals, each with its own question.

**The standard library defines the types two packages would otherwise
each invent**. Geospatial libraries disagree on
whether a point is `[lat, lng]` or `[lng, lat]`; payment libraries on
whether ten pounds is `10.00`, `"10.00"` or `1000`. When the language owns
the type, every package and every application speaks it, and that whole
class of bug goes away. A standard type exists wherever two packages
would otherwise define their own, even when the heavy operations live in
a package or a later area.

- **Each vocabulary type is defined once:** one construction that
  validates, one wire shape, one storage shape. Nothing else may give a
  second.
- **Ambiguity is designed out.** Fields are named, never positional:
  `Point { latitude: ...  longitude: ... }`, never a pair. An axis order
  exists only at a named conversion edge that says it in its name
  (`geo.from_lng_lat(list)`, `geo.from_lat_lng(list)`, `geo.from_geojson`).
- **Packages use them** (decided): a package's public API that declares
  its own equivalent of a standard type is warned and pointed at it
  ("Packages use the standard types", below).

| Type | Fields or representation | Wire (JSON) | Storage | Standard library vs elsewhere | Status |
|---|---|---|---|---|---|
| `Instant`, `CalendarDate`, `ZonedTime`, `Duration`, `Zone`, `Locale`, `Weekday` | As area 10 | Area 7's wire forms | Area 5 | `temporal`, `locale` | Exists (areas 2, 10) |
| `Email`, `Url`, `IpAddress` | Validated `Text` | A string | `text` | Construction only | Exists (area 2) |
| `FileType` | Enum of media types | `"image/png"` | `text` | Construction only | Exists (area 16) |
| `Money`, `Currency` | Minor units (`Int`) and an ISO 4217 currency | `{"amount": 1050, "currency": "GBP"}` | `bigint` and a `char(3)` code | `money`; no exchange rates | Proposed here (question 12); built in per #88 |
| `Country` | Enum of ISO 3166-1 alpha-2 codes, `Country.gb` | `"GB"` | `char(2)` | Construction only; names through translations | Proposed here; built in per #88 |
| `Language` | BCP 47 language tag (`en`, `pt-BR`), any language, not only the app's supported `Locale`s | `"pt-BR"` | `text` | Construction only | Proposed here; built in per #88 |
| `PhoneNumber` | E.164 text, `+447700900123` | A string | `text` | Construction, which normalises; formatting for people a package | Proposed here; carried to area 2 by area 11, question 19 |
| `Percentage` | `Decimal` where `0.20` is 20%, scale chosen by the type | `"0.20"`, as any `Decimal` | `numeric` | `money.multiply`, `decimal` operations; `{n, number, percent}` in messages | Proposed here (question 12) |
| `Colour` | `red`, `green`, `blue` and `alpha`, each 0 to 255, in sRGB | `"#rrggbb"` or `"#rrggbbaa"` | `text`, the same | Construction from hex; conversions to other spaces a package | Proposed here (question 9); reverses Richard's point 8 |
| `Point` | `latitude`, `longitude` (WGS84 degrees) | GeoJSON | PostGIS `geography` | Type here; operations in the geospatial area | Proposed for a future area (question 8) |
| `LineString`, `Polygon`, `MultiPolygon` | Named lists of `Point`s | GeoJSON | PostGIS `geography` | As `Point` | Proposed for a future area |
| `Vector` | A fixed number of `Decimal`s, `Vector { dimensions: 1536 }` | A JSON array of numbers | pgvector `vector(n)` | Type here; similarity queries in the AI area | Proposed for a future area (question 15) |

### Packages use the standard types

Decided 2026-10-17 (Richard). A public type in a package (#88's
`surface.jadpo_record` lists them) whose shape is a standard type's gets
`type.standard_equivalent`, a warning: it joins #88's publishing bar as a
warning, not a refusal, so a genuinely different meaning can still publish.
It matches by shape and by the common names for each field, not by exact
names: an `Object` whose only fields are those below, in any order.

| Standard type | Matches an `Object` whose only fields are |
|---|---|
| `Point` | a latitude (`latitude`, `lat`) and a longitude (`longitude`, `lng`, `lon`, `long`), both built on `Int` or `Decimal` |
| `Money` | an amount (`amount`, `value`, `amount_minor`, `minor`, `cents`, `pence`) built on `Int` or `Decimal`, and a currency (`currency`, `currency_code`) built on `Text` or an enum |
| `Colour` | `r`, `g`, `b` and optionally `a`, or `red`, `green`, `blue` and optionally `alpha`, built on `Int` or `Decimal` |
| `LineString`, `Polygon` | a list of a type that itself matches `Point` (`points`, `coordinates`), and nothing else |

The other vocabulary types are single values (`Email`, `Country`,
`PhoneNumber`), where a shape says nothing; a package's `type Iso2 = Text
{ pattern: "[A-Z]{2}" }` isn't flagged.

```text
warning: `GeoPoint` is the standard `Point` under another name
 --> types/geo_point.jadpo:3:13
  |
3 | public type GeoPoint = Object {
  |             ^^^^^^^^ a latitude and a longitude
  reason: `GeoPoint` has only a latitude (`lat`) and a longitude (`lng`), which is the standard `Point`. Packages that each declare their own disagree on order and on names, and an app has to convert between them.
  next: Use `Point` in the package's public types, converting at the edge with `geo.from_lat_lng` or `geo.from_lng_lat` where the provider sends a pair (agent)
  rule: type.standard_equivalent
  help: docs/language/19-standard-library.html#packages-use-the-standard-types

checked: 1 warning, 1 file
```

The one-step fix is the `next:`: replace the package's type with the
standard one in its public types, and convert at the edge, in the
package's `wire:` or `decode:` (#88), from whatever the provider sends.

The `Country`, `Language` and `PhoneNumber` lists ship with each Jadpo
release, as the time-zone database does (#88, "built in or a library
package"). That also answers #88's open point for 5.4, where fixed
reference data can be declared: in the release, as these enums, not by
applications.

## What every operation follows

These hold for every namespace, and are what the decided areas already
apply to `collection`, `decimal`, `integer` and `temporal`.

- **One namespace per type it works on** (question 2): `list.`, `set.` and
  `map.` for `List<T>`, `Set<T>` and `Map<K, V>`, `text.` for text,
  `money.` for `Money`. The name says what the first argument is.
- **Always called through its namespace** ([area 1](01-project-layout-and-modules.md)):
  `text.trim(name)`, never `trim(name)`. A namespace's name can't name a
  declaration, domain, library, parameter or variable; a field or a named
  argument can still be called `text`, since `note.text` and `set:` are
  never read as the namespace.
- **Pure, with one named exception.** Each gives the same result for the
  same arguments, in the interpreter and on every compiled target, and may
  be called in a function. The exception is randomness: `random.*`,
  `crypto.encrypt` and `crypto.hash_password` read the generator, so, as
  `clock.now` is, they are called only in actions and are fixed in tests
  (question 14). Unicode data (case mapping, white space, the slug's
  decomposition), CLDR formats and ISO lists are the versions pinned in
  `jadpo-value`, and upgrading them is a reviewed change, as the time-zone
  database is.
- **Representations in, representations out**
  ([area 3](03-logic.md), operators). A parameter written `Text` takes any
  value built on `Text`, `Email` and `Url` included, and a result written
  `Text` is plain text, turned into a named type by constructing it. A
  result keeps the argument's named type only where the operation returns
  one of its arguments or their items unchanged (`integer.max`,
  `list.filter`, `set.union`), since only then can no rule break.
- **Named arguments only where a value needs saying** (area 3): the first
  arguments are positional, and options and modes are named, as `rounding:`
  is today.
- **Failures are typed** ([area 4](04-failures.md)). An operation that can
  fail names each condition it fails on, and the call's `attempt` maps it
  by that name, as `dst:` and `invalid_day:` are mapped today
  ([area 10](10-configuration-and-time.md#zones-and-local-times)): the
  failure is `invalid_value`, and `invalid:` covers every condition without
  its own arm. Overflow is a defect, as it is for `+`. Where a bad argument
  can be ruled out while compiling, it must be, as `/` rules out a zero
  divisor: the table's `never_zero` gains `never_empty` (a literal isn't
  `""` and a value's type has a `min_length` of at least 1) and
  `never_negative` (a literal isn't below 0 and a value's type has a `min`
  of at least 0).
- **Facts for proofs.** A result whose type can't say its bounds carries
  them in the table, as `list.count`'s `at_least: 0` does today
  ([area 2](02-values-and-types.md)), so constructing from it needs no arm
  for a rule it can't break. New here: `at_most_length_of`, for a text
  result never longer than an argument (`text.trim`, `text.keep_first`), so
  `Title(text.trim(title))` can't break `Title`'s `max_length`, though it
  can still break `min_length` or a `pattern`.

```jadpo fragment
text.trim(input.title)                                   // "  Rye loaf " gives "Rye loaf"
text.split("flour,sugar,,eggs", ",")                     // ["flour", "sugar", "", "eggs"]
text.pad_start(text.from(order.number), 6, fill: "0")    // 42 gives "000042"
list.sum(list.map(basket.lines, line -> line.price * line.count))
set.contains(recipe.allergens, Allergen.nuts)
encoding.to_hex(hash.sha256(upload.body))                // 64 lowercase hexadecimal digits
attempt encoding.from_base64(input.attachment) {
    malformed: AttachmentUnreadable
}
```

## Validating in the middle of a program

**Construction is the way** (question 11). `Type(value)` checks every rule
on the type and the types it builds on, and an `attempt` maps each rule
that can break to the failure the caller gets ([area 2](02-values-and-types.md#construction-failures-by-rule)).
That holds wherever the value came from: a computation, a part of a
request, a cell of an import, or `json.read`.

```jadpo fragment
// One value: the rule that can break is named.
var quantity: Quantity = attempt Quantity(line.count * 2) {
    max: OrderTooLarge
}

// A whole record from parts: each part is built first, as its field type,
// and the object literal then can't fail (area 2, Objects).
var name: Badge.name = attempt Badge.name(text.trim(profile.display_name)) {
    max_length: BadgeNameTooLong
}
return Badge {
    name: name
    colour: profile.colour
}

// Data from text: json.read decodes and checks every field rule of the
// type in one step, so there is no second validation.
var order: OrderImport = attempt json.read(message.body, as: OrderImport) {
    malformed: ImportUnreadable
    mismatch: ImportInvalid
}
```

Nothing new is needed for single values or whole records. One thing
construction can't do is a rule across fields ("ends after it starts"),
because a type has no such rule today: the bookings scenario needs one,
so cross-field rules are proposed as a language change (question 11,
"Language gaps").

## SQL forms

A **SQL form** is a PostgreSQL 16 expression that gives exactly the result
the runtime gives, for every argument. It matters in one place today: a
computed backfill whose every operation has a SQL form runs as one
`UPDATE` per batch, and any other runs through `jadpo migrate apply`,
reading and writing each batch ([area 5](05-data.md#computed-backfills)).
Both are correct; the SQL form is only faster. Queries still compute
nothing and a one-step update's `set:` still works a stored field out only
with `+` and `-` (area 6); aggregate queries would be their second user
(question 22, "Language gaps").

- An operation that can fail has no SQL form, since a statement can't map
  a failure to a row.
- Text is compared and ordered in code points, as queries sort it
  (area 6, with `COLLATE "C"` indexes, area 5), so a SQL form never
  depends on the database's collation. That rules out `lower` and
  `upper`: PostgreSQL 16's follow the collation, and under `"C"` change
  only ASCII letters.
- A list or set is stored as a `jsonb` array and a map as a `jsonb` object
  (area 5); only `count` has a SQL form.
- Each SQL form is tested against the runtime on the same inputs, Unicode
  edge cases included, before a backfill may use it.

In the tables below, `x` is the first argument and `a`, `b` the others.

## Lists

Each operation is called as `list.<name>`, replacing `collection.`
(question 2). `T` is the item type, fixed by the list. A function argument
is written inline, `item -> expression` (area 3).

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `count(items)` | `Int`, at least 0 (decided) | | `jsonb_array_length(x)` |
| `contains(items, item)` | `Bool`: an item `==` `item` (decided) | | none |
| `map(items, each)` | `List<U>`, in order (decided) | | none |
| `filter(items, keep)` | `List<T>`, the items `keep` accepts, in order (decided) | | none |
| `sum(items)` | `Int` or `Decimal`, the items added; `0` for none. Items built on `Int` or `Decimal`; `Money` uses `money.sum` | overflow is a defect | none |
| `any(items, test)` | `Bool`: `test` accepts at least one; `false` for none | | none |
| `all(items, test)` | `Bool`: `test` accepts every one; `true` for none | | none |
| `find(items, test)` | `T?`, the first item `test` accepts | | none |
| `first(items)`, `last(items)` | `T?` | | none |
| `at(items, position)` | `T?`, the item at `position` (0 is the first); `position` never negative | | none |
| `min_by(items, key)`, `max_by(items, key)` | `T?`, the item with the smallest or largest key; the first of equal ones | | none |
| `sort_by(items, key, locale:)` | `List<T>`, ordered by `key` ascending; equal keys keep their order. `key` gives a number, a time, a date, a duration, `Money` of one currency or text: text in code points as queries sort it, or by a locale's rules with `locale:` | | none |
| `reverse(items)` | `List<T>`, last item first | | none |
| `distinct(items)` | `List<T>`, each item's first occurrence, by `==` | | none |
| `slice(items, from, count)` | `List<T>`, up to `count` items from position `from`, cut short at the end; both never negative | | none |
| `keep_first(items, n)`, `keep_last(items, n)` | `List<T>`, at most `n` items from that end; `n` never negative | | none |
| `drop_first(items, n)`, `drop_last(items, n)` | `List<T>` without `n` items from that end; all gone when `n` is longer | | none |
| `append(items, item)` | `List<T>`, `item` added at the end | | none |
| `concat(first, second)` | `List<T>`, `first`'s items then `second`'s | | none |
| `flatten(lists)` | `List<T>` from a `List<List<T>>`, in order | | none |
| `group_by(items, key)` | `Map<K, List<T>>`: each key with its items in order | | none |
| `zip_with(first, second, (a, b) -> ...)` | `List<U>`, pairing items by position, as long as the shorter. Waits for two-parameter inline functions | | none |

- **No push or pop.** Values never change (area 2), so adding is
  `list.append(items, item)`, a new list, and taking the last off is
  `list.last` then `list.drop_last(items, 1)`.
- **The largest is `max_by`,** not `first(reverse(sort_by(...)))`, which
  the first draft of this page had to write and which sorts the whole list
  to read one item. In a query it stays `order:` and `limit:`.
- **Cutting from either end** is `keep_first`, `keep_last`, `drop_first`
  and `drop_last`, the same four on text (question 3). `slice` stays for a
  window in the middle. There are no negative positions: in JavaScript
  `slice(-2)` counts from the end, `substring(-2)` treats it as 0 and
  `substr(-2)` counts from the end again, and an off-by-one that goes
  negative silently reads from the other end instead of failing. Here a
  count that might be negative doesn't compile (`never_negative`).
- **`items[i]` isn't syntax.** An index can be out of range, so it would
  need a failure arm or a defect; `list.at` gives `T?`, which `match`
  handles, and nothing suggests the list can be assigned into.
- **`sum` gives a representation**, as `+` does, so `Pence(list.sum(prices))`
  builds it back, with an arm for `max` when the type has one.
- **Left out by design:** `reduce` and `fold` (a `for` with a `var mut` is
  the one way to accumulate, and reads more plainly in review; `sum`
  covers the common case); sorting with a comparator (`sort_by` and
  `reverse` express every order a key can, and a comparator that isn't
  consistent gives different orders on different targets).

## Sets

Each operation is called as `set.<name>`. All return a new set.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `count(items)` | `Int`, at least 0 | | `jsonb_array_length(x)` |
| `contains(items, item)` | `Bool` | | none |
| `with(items, item)` | `Set<T>` with `item`; the same set if already there | | none |
| `without(items, item)` | `Set<T>` without `item` | | none |
| `union(a, b)` | `Set<T>`, items in either | | none |
| `intersection(a, b)` | `Set<T>`, items in both | | none |
| `difference(a, b)` | `Set<T>`, items in `a` and not in `b` | | none |

- **These need no walking by code.** Each is one operation in the
  runtime, so none waits for `for`. A named set type is built back from
  the result by construction, as area 2's lengths require.
- **`for` walks a set** (question 4, "Language gaps"). Area 2 already
  decided the order: everything that shows a set, iteration "once `for`
  takes sets" included, uses one canonical order by value. So the only
  change is `for` accepting a `Set`, in that order. A set is still never
  indexed and has no first item.
- **`filter`, `any`, `all` and `map` on a set** wait for a scenario that
  needs them; `for` covers it meanwhile.
- **Example 04** keeps `Recipe.allergens` as a list only because a set
  couldn't be searched; with `set.contains` it becomes the set it is.

## Maps

Each operation is called as `map.<name>`. All return a new map.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `count(entries)` | `Int`, at least 0 | | `(SELECT count(*) FROM jsonb_object_keys(x))` |
| `get(entries, key)` | `V?` | | none |
| `has(entries, key)` | `Bool` | | none |
| `keys(entries)` | `Set<K>` | | none |
| `values(entries)` | `List<V>`, in the canonical order of their keys | | none |
| `with(entries, key, value)` | `Map<K, V>` with `key` set to `value`, added or replaced | | none |
| `without(entries, key)` | `Map<K, V>` without `key` | | none |
| `merge(first, second)` | `Map<K, V>` with both; `second` wins where both have a key | | none |
| `filter(entries, (key, value) -> ...)` | `Map<K, V>`, the entries accepted. Waits for two-parameter inline functions | | none |
| `map_values(entries, (key, value) -> ...)` | `Map<K, U>`. Waits for two-parameter inline functions | | none |

- **A map is walked through its keys:** `for key in map.keys(prices)`,
  in canonical order.
- **An empty map is `[]` written where a map is expected,** as a list
  literal written where a set is expected is a set (area 2); entries are
  added with `map.with`. A literal with entries is area 2's to design
  (question 4).
- **Records have no dynamic keys,** on purpose. A record's fields are
  fixed and each is its own type (`Customer.email`), so `record[name]`
  would give a value of no single type, lose the field type that keeps
  identifiers apart, and hide which field policy and personal-data marks
  apply to. Keys known only while running are what a `Map` is for.

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
| `integer.parse(text)` | `Int` from an optional `-` and ASCII digits, nothing else | `malformed` | none |
| `integer.format(value, locale:)` | `PresentationText`, as `{n, number}` writes it | | none |
| `decimal.divide(dividend, divisor, rounding:)` | `Decimal` (decided, area 3) | | every mode: as `integer.divide`, on the values scaled to the dividend's places |
| `decimal.abs(value)` | `Decimal`, at least 0, the same scale | | `abs(x)` |
| `decimal.min(a, b)`, `decimal.max(a, b)` | as for `integer` | | `LEAST(a, b)`, `GREATEST(a, b)` |
| `decimal.round(value, scale:, rounding:)` | `Decimal` with at most `scale` places; `scale:` a whole number written out, at least 0 | | `round(x, s)` for `half_up`; a `CASE` on the dropped digits for the others |
| `decimal.parse(text)` | `Decimal` from an optional `-`, digits and one `.`, with the places written | `malformed` | none |
| `decimal.format(value, locale:)` | `PresentationText`, as `{n, number}` writes it | | none |
| `decimal.sqrt(value, scale:, rounding:)` | `Decimal`; `value` never negative | | none |
| `decimal.power(value, exponent)` | `Decimal`, exact; `exponent` a whole number, never negative | overflow is a defect | none |
| `decimal.exp(value, scale:, rounding:)`, `decimal.ln(value, scale:, rounding:)` | `Decimal`; `ln`'s `value` above 0 | `ln`: `out_of_range` | none |
| `decimal.sin(radians, scale:, rounding:)`, `decimal.cos(radians, scale:, rounding:)` | `Decimal` | | none |
| `decimal.atan2(y, x, scale:, rounding:)` | `Decimal`, radians | | none |
| `decimal.pi(scale:, rounding:)` | `Decimal` | | none |

- **Rounding to a type's scale needs no call.** Constructing into a type
  with a fixed `scale` rounds half to even (area 2), so `decimal.round` is
  for another mode or fewer places than the type keeps.
- **An `Int` becomes a `Decimal` by multiplying,** `count * 1.00`, since
  `*` may mix the two (area 3), so there is no `decimal.from_integer`.
- **Parsing takes the wire's form only:** no grouping, no locale's
  decimal comma, no exponent. A spreadsheet's `1.234,50` is a cell
  decoder's job (#88's `decode:`).
- **Maths on `Decimal`, not a `Float`** (question 13). Each result is
  correctly rounded to `scale` with the named rounding, by Jadpo's own
  decimal code, so it is the same on every target; a float's `sin` differs
  between platforms' maths libraries, and has `NaN`. Geospatial distance
  and statistics (a standard deviation) are what need them.
- **PostgreSQL's `round` ties away from zero,** which is `half_up`; the
  other modes are spelled out with a `CASE`, never left to the database.
- **`Rounding` stays `half_even`, `half_up` and `up`,** unless question 19
  adds `down`.

## Money

`Money` is a built-in vocabulary type (question 12): an amount held as a
whole number of the currency's minor units (pence, cents) and its
`Currency`, never a decimal of pounds. Each currency's exponent comes from
ISO 4217: `jpy` 0, `gbp` 2, `bhd` 3. `Currency` is an enum of the active
ISO 4217 codes, written `Currency.gbp`, shipped with each release.

```jadpo fragment
type Price = Money {
    currency: Currency.gbp    // fixed: arithmetic with another currency doesn't compile
    min: 0                    // in minor units
}

var list_price: Price = money.from_minor(1050, Currency.gbp)        // £10.50
var entered: Money = attempt money.of(input.amount, input.currency) {
    too_many_places: AmountTooPrecise                                 // 10.505 in GBP
}
var total: Price = Price(list_price + delivery)                      // same currency: plain +
var vat: Price = money.multiply(total, by: 0.20, rounding: Rounding.half_even)
var thirds: List<Money> = money.split(total, 3)                       // 3.50, 3.50, 3.50
money.format(total, locale: Locale.en_gb)                            // "£10.50"
```

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `money.from_minor(amount, currency)` | `Money` of `amount` minor units | | none |
| `money.of(amount, currency)` | `Money` from a `Decimal` in major units | `too_many_places` (more places than the currency has) | none |
| `money.minor(value)` | `Int`, the minor units | | the amount column |
| `money.currency(value)` | `Currency` | | the currency column |
| `money.sum(items, currency:)` | `Money`; `currency:` gives the zero for an empty list | `currency_mismatch` | none |
| `money.multiply(value, by:, rounding:)` | `Money` times a number (tax, discount, a rate), rounded to a minor unit | overflow is a defect | none |
| `money.divide(value, by:, rounding:)` | `Money` divided by a number, rounded to a minor unit; `by` never zero | | none |
| `money.allocate(total, ratios)` | `List<Money>` in proportion to `ratios` (whole numbers), adding up to `total` exactly: leftover minor units go one each to the first shares | | none |
| `money.split(total, parts)` | `List<Money>`, `parts` near-equal shares adding up to `total`; `parts` at least 1 | | none |
| `money.format(value, locale:, precision:, display:, sign:)` | `PresentationText` | | none |

- **`+`, `-` and comparisons** work on two `Money` values of the same
  currency, as on numbers. Where both types fix the currency, a mismatch
  is a compile error. Where it is only known while running (a SaaS that
  bills each customer in theirs), the checker needs it proven, as it
  proves a range: inside `if (money.currency(a) == money.currency(b))` the
  `+` is plain, and outside it the error's `next:` says to check first.
  There is no `money.add`: one way each (question 12).
- **Multiplying or dividing always names its rounding,** since a third of
  a penny has to go somewhere. Splitting never rounds: `allocate` and
  `split` hand every minor unit to some share.
- **No exchange rates.** Converting between currencies needs a rate,
  which is outside data with a time and a source; it is a provider
  package's operation, and its result is constructed with `money.of` like
  any other amount.
- **Storage** is a `bigint` of minor units and a `char(3)` currency code,
  two columns, also for a type that fixes the currency, so opening a
  type's currency later needs no migration and a query can group by it.
- **Wire** is one fixed shape, `{"amount": 1050, "currency": "GBP"}`, in
  integer minor units as Stripe sends it; `json.read` and `json.write` use
  it too. A provider's other shape (`"10.50"`) is mapped in its package
  (#88's `wire:`) (question 12).
- **Formatting for people.** `money.format` writes the symbol or code, the
  locale's grouping and decimal separator, the currency's places and the
  symbol's position from CLDR: `£1,234.50`, `1 234,50 €`. `precision:
  MoneyPrecision.whole` gives `£1,235`; `display:` is `symbol` (default),
  `code` or `name`; `sign:` is `standard` or `accounting` (`(£5.00)`).
  Inside a translation the same formatter is `{price, number, currency}`,
  amending area 10's list of placeholder styles. Code uses `money.format`;
  messages use the placeholder. ICU4X, already pinned, has the currency
  formatter (`icu_experimental`).
- **What changes in area 2:** `Money` and `Currency` become built-in
  names, so area 2's examples that declare `type Money = Object { amount,
  currency }` (Objects, the type reference, question 12's sample) are
  renamed or use the built-in. Example 09's `Pence` and its percentage
  discount move to `Money` in 5.7.

## Text

Each operation is called as `text.<name>`. Text is counted, cut and
padded in Unicode code points, as `max_length`
counts it (area 2) and as PostgreSQL's `char_length` does (question 17).
`PresentationText` is its own representation (area 10), so none of these
take it.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `length(value)` | `Int`, at least 0, in code points | | `char_length(x)` |
| `trim(value)` | `Text` without Unicode white space at either end; never longer | | `btrim(x, chars)`, `chars` the Unicode `White_Space` characters |
| `lower(value, locale:)`, `upper(value, locale:)` | `Text` in Unicode's default case mapping, or a locale's (`İ` in Turkish) with `locale:` | | none |
| `contains(value, part)` | `Bool` | | `strpos(x, a) > 0` |
| `starts_with(value, prefix)`, `ends_with(value, suffix)` | `Bool` | | `starts_with(x, a)`; `right(x, char_length(a)) = a` |
| `replace(value, find, with)` | `Text` with every `find` replaced, left to right; `find` never empty | | `replace(x, a, b)` |
| `split(value, separator)` | `List<Text>`, at least one item; `separator` never empty; `""` gives `[""]`, and separators side by side give `""` between them | | none |
| `join(items, separator)` | `Text`, the items with `separator` between them | | none |
| `slice(value, from, count)` | `Text`, up to `count` code points from position `from`, cut short at the end; both never negative; never longer | | `substr(x, a + 1, b)` |
| `keep_first(value, n)`, `keep_last(value, n)` | `Text`, at most `n` code points from that end; `n` never negative; never longer | | `left(x, n)`; `right(x, n)` |
| `drop_first(value, n)`, `drop_last(value, n)` | `Text` without `n` code points from that end; never longer | | `substr(x, n + 1)`; `CASE WHEN n = 0 THEN x ELSE left(x, -n) END` |
| `pad_start(value, length, fill:)` | `Text` of at least `length` code points, `fill` added at the start; never cut; `fill:` one character written out | | `CASE` on `char_length(x)` with `lpad` |
| `from(value)` | `Text`, the value as JSON carries it (areas 2 and 10): an `Int`'s digits, a `Decimal` with its scale's places, a `Uuid` in lowercase, `true` or `false`, a time in its wire form | | `x::text` for `Int`, `Decimal`, `Uuid` and `Bool`; times none |
| `slug(value)` | `Text` of lowercase ASCII letters, digits and single `-`: accents dropped, every other run of characters one `-`, none at either end; may be empty | | none |

- **Matching is a type's `pattern`.** Whether text looks like a postcode
  is a construction, `attempt Postcode(raw) { pattern: ... }` (area 2), so
  there is no `text.matches`, and no replacing or capturing by regular
  expression: a pattern is a type's rule, checked in linear time.
- **Joining two pieces is `+`** (area 3); `join` is for a list.
- **`from` takes no enum and no text.** An enum has no display label (area
  2), so showing one is a `match`; a value built on `Text` already is text.
- **`keep_first` can split a character a person sees as one,** such as an
  emoji with a skin tone, which is two code points. A preview cut this way
  is still valid text; graphemes are left out (question 17).
- **Left out by design:** `trim_start`, `trim_end` and `pad_end` (no
  scenario needs them, and each is a second way near one that exists);
  iterating characters (every job the scenarios found, slugs, masks,
  previews, case, is an operation, and character loops are where combining
  marks break). Unicode normalisation is a boundary rule, not a call
  ("Language gaps").

## Hashing and cryptography

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `hash.sha256(value)` | `Bytes`, the 32-byte SHA-256 digest of a value built on `Bytes`, or of a `Text`'s UTF-8 bytes | | `sha256(x)`, or `sha256(convert_to(x, 'UTF8'))` for text |
| `crypto.encrypt(value, key:, context:)` | `Bytes`: XChaCha20-Poly1305 with a fresh random nonce, the format's version and the key's identity in front. `value` is `Bytes` or a `Text`'s UTF-8; `context:` text bound to the result (a record's id), so a sealed value can't be moved to another row. Actions only | | none |
| `crypto.decrypt(sealed, key:, context:)` | `Bytes`, trying the current key, then `_PREVIOUS` | `unreadable` (wrong key or context, or altered) | none |
| `crypto.hash_password(password)` | `PasswordHash`, a built-in text type holding the PHC string: Argon2id at the parameters fixed for the release (OWASP's: 19 MiB, 2 passes, 1 lane). Actions only | | none |
| `crypto.verify_password(password, hash)` | `PasswordCheck`: `wrong`, `correct`, or `correct_outdated` (store a fresh hash) | | none |
| `crypto.hmac(value, key:)` | `Bytes`, HMAC-SHA-256 with the current key | | none |
| `crypto.verify_hmac(value, mac, key:)` | `Bool`, in constant time, against the current or previous key | | none |

- **No algorithm choices.** Each job has one construction, chosen for
  safety by default; a release that changes one keeps reading the old
  format (the version byte, the PHC string) and `correct_outdated` tells
  the app to rehash on sign-in. Area 8 leaves checking a password to the
  application, which is why it needs these.
- **Code still never reads a secret** ([area 10](10-configuration-and-time.md#secrets)).
  A key is an opaque value: an `EncryptionKey` setting (32 random bytes,
  new here) for `encrypt` and `decrypt`, and a `SigningKey` setting
  ([area 10](10-configuration-and-time.md#signing-keys)) for `hmac`. Each
  is written `key: config.<name>` and nowhere else, so the `key:` of a
  `crypto` operation joins area 10's list of places the compiler owns;
  any other read stays `configuration.secret_flow`. Both rotate as a
  `SigningKey` does, through `NAME_PREVIOUS`.
- **A digest is `Bytes`,** and `encoding` writes it as text:
  `encoding.to_hex` for an ETag or a content key (question 18).
- **No MD5 or SHA-1.** Both are broken; an outside system that still asks
  for one is reached through a package (area 11). That includes TOTP,
  whose RFC 6238 default is HMAC-SHA-1 and whose per-person secret is a
  key held as data ("Language gaps").
- **Encrypted fields** (question 6): `notes: ClinicalNote { encrypted:
  true }` on an entity would store the field sealed with the
  application's `field_encryption_key:`, decrypt it on load, and keep it
  out of unique rules, indexes, filters, ordering, search and logs, as
  area 16's table already says of field encryption. One key for the
  application, never one per person: area 15 decided against per-person
  keys (question 5 there), and this keeps that.

## Randomness

`random` is a generated namespace like `clock` (question 14): it reads
the operating system's cryptographic generator in production, is called
only in actions, gives the same values on every rerun of a test (seeded
per test, as the test clock is fixed), and is recorded for replay as
area 17 already lists randomness among the effects.

| Operation | Gives |
|---|---|
| `random.token()` | `Text`, 32 random bytes as Base64url, 43 characters: for links and keys a person never types |
| `random.code(length)` | `Text` of `length` characters from Crockford's Base32 (digits and capitals without `I`, `L`, `O`, `U`): booking and order references a person reads out; `length` 4 to 32 |
| `random.integer(min, max)` | `Int`, uniformly from `min` to `max`, both included; `min` at most `max` |
| `random.shuffle(items)` | `List<T>`, the items in random order |
| `random.sample(items, count)` | `List<T>`, `count` items from different positions, in random order; all of them when `count` is larger |

`random.token` is what Richard's point 6 called `crypto.random_token`; it
lives here because the test seed does.

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

- **`malformed`** is the one condition decoding fails on.
- **There is no `from_url_component`:** the runtime decodes paths and
  query strings before code sees them (area 7). `to_url_component` is for
  a link code builds, such as an email's sign-in link.
- **Left out by design:** base32 as a call (no scenario but TOTP, which is
  a package) and compression (#88 allows gzip only inside the batch
  helpers, with `max_unpacked_size:`).

## Data formats: JSON, CSV, TSV and JSON Lines

#88 ([the packages design](https://github.com/richardwillars/jadpo/blob/packages-design/docs/language/research/packages-and-extension-proposals.md),
"Files and formats", with its area 12 "Batches" section) already decides
these, and this page agrees with it rather than redesigning them:

- Storage and formats are separate layers. `store.read` and `store.put`
  move bytes; `csv` (with `tsv` as its tab-separated form), `json` and
  `json_lines` are pure standard-library namespaces turning bytes into the
  app's types and back, wherever the bytes came from.
- Rows of a file of any size go only through the batch helpers:
  `csv.read`/`csv.write`, `tsv.read`/`tsv.write`,
  `json_lines.read`/`json_lines.write`, `json.read(file, items_at:, as:)`/
  `json.write`, and `batches.each` for work with no file. Each starts
  after the action commits and returns a run with progress and problems.
  There is no loop over a file's rows and no whole-file parse into a list.
- Rows read are `Input`s and rows written `Output`s, with `column:` and the
  decided `wire:` encodings. Writing CSV names its reader
  (`CsvFormat.spreadsheet`: byte-order mark, CRLF, formula escaping;
  `CsvFormat.standard`). Reading takes UTF-8, UTF-16 with a mark and
  declared legacy encodings, never guessed.
- XML is deferred: it waits for an XML byte format in a later release.

**One JSON document in memory** (question 10, Richard's point 10). #88
keeps `json.decode` for single documents. This page proposes one name per
direction instead, so there is one namespace and one shape:

| Call | Gives | Fails |
|---|---|---|
| `json.read(text, as: T)` | `T`, decoded and checked against every rule of `T`; `text` is `Text`, or `Bytes` in UTF-8 | `malformed` (not JSON), `mismatch` (a field missing, unknown or breaking a rule) |
| `json.write(value)` | `Text`, compact; fields in declared order, sets and map keys in canonical order, so the same value always gives the same text | |
| `json.read(file, items_at:, as:, each:, finished:, key:)` | #88's batch run | as #88 |
| `json.write(store, name:, from:, each:, finished:, key:)` | #88's batch run | as #88 |

- **`as:` takes any type an `Input` may hold,** read with an `Input`'s
  rules (unknown fields refused, `optional` and `default` applied);
  `json.write` takes any value an `Output` may hold. Both use the wire
  forms areas 2, 7 and 10 decided, `Money`'s included.
- **The two forms can't be confused:** given text it returns the value,
  given a stored `File` with `each:` it returns a run, and using one where
  the other is expected is a type error.
- **`mismatch` names the first problem** the way area 7's `invalid_input`
  does: the field's path with list positions as numbers
  (`lines.2.quantity`) and the rule (`max`), never the value. A failure
  mapped from it that declares a `problem: DecodeProblem` field receives
  them.
- **No dynamic JSON value.** No scenario needs a JSON value of no type;
  a payload whose shape isn't known is kept as `Text` and passed on.
- **No CSV in memory.** #88's rule stands: a provider's CSV answer goes
  into a store and through `csv.read`, so there is one way to read rows.

## Logic and bits

`and`, `or` and `not` are the logical operators (area 3), and exclusive or
on `Bool` is `!=`. Any other gate is a one-line function, so none is
added:

```jadpo fragment
function nand(a: Bool, b: Bool) -> Bool {
    return not (a and b)
}
```

Bitwise operations on `Int` and `Bytes` (and, or, xor, not, shifts) are
left out (question 21): flags are a `Set` of an enum, which a reviewer can
read, and the one scenario with bit-packed data, device payloads, decodes
them in the provider package that knows the format.

## Geospatial: a future area

Geospatial work needs its own area page, not a section here: new stored
types, PostGIS, query predicates and indexes touch areas 2, 5 and 6, which
this page doesn't. Proposed scope (question 8):

| Part | Proposal |
|---|---|
| Types | `Point { latitude, longitude }`, `LineString`, `Polygon`, `MultiPolygon`, in WGS84; named fields, never a pair |
| Wire | GeoJSON, whose `[longitude, latitude]` order exists only inside the conversion |
| Construction | `geo.from_geojson(text)`, `geo.from_lng_lat(numbers)`, `geo.from_lat_lng(numbers)`, each failing `malformed` or `out_of_range`; a polygon's ring closed and valid |
| Operations | `geo.intersects`, `geo.contains`, `geo.within`, `geo.distance` (metres, on the sphere, using the `Decimal` maths here) |
| Storage | PostGIS `geography` columns |
| Queries | `within`, `intersects` and `distance` as query conditions and order, with GiST indexes (area 5) |
| Left out | Projections other than WGS84, routing, geocoding (provider packages) |

## AI: a future area

Area 11 and #88 decided that AI models are provider packages, and that
stays. What makes them first class in typed applications is a set of
standard pieces Jadpo itself does, which is what #88's design C asks of a
capability: Jadpo renders the prompt from a checked file and decodes the
answer into a type, as it renders an email and hands it to a sender.
Proposed scope for its own area page (question 15):

| Part | Proposal |
|---|---|
| Capability | `ai`, filled by provider packages (Anthropic, OpenAI, a local model) through `provides/ai.jadpo`, with a local driver, one fake and a conformance suite, as every capability has |
| Typed answers | `ai.generate(prompt, as: T)` gives a `T` through the same decoding as `json.read`; fails `refused`, `malformed`, `timed_out`, `over_budget` |
| Streaming | `ai.stream(prompt)` for text shown as it arrives, to a route that streams |
| Prompts | Checked files with typed placeholders, as translations are: a missing or mistyped placeholder doesn't compile |
| Embeddings | `Vector { dimensions: n }` (vocabulary types), `ai.embed(text)`, a pgvector column and index, and a nearest-neighbour query condition and order, for semantic search and retrieval |
| Tests | The fake answers from fixtures, so tests are deterministic and never call a model |
| Limits and data | Token and cost limits per call and per period; every field sent to a model listed in area 15's personal-data report |
| Left out | Training models: it needs GPUs, datasets and long jobs, none of which an application request does; a package or an outside service trains, and the app calls the result |

## Scenario applications

Nine applications Jadpo is meant for, what each does, and the operations
that needs. The coverage matrix follows; each gap is fixed on this page or
left out with a reason.

**1. Online shop**

| Does | Needs |
|---|---|
| Basket total and a 10% discount | `money.sum`, `money.multiply` with `rounding:` |
| Discount spread across the lines for the receipt | `money.allocate` |
| Product page address from its title | `text.slug` |
| Order reference read out on the phone | `random.code` |
| Products by price, highest first | `list.sort_by`, `list.reverse`; `order:` in a query |
| Product feed for a marketplace | `csv.write` (#88) |
| Image cache key | `hash.sha256`, `encoding.to_hex` |

**2. Appointment bookings**

| Does | Needs |
|---|---|
| Slots in the clinic's zone | `temporal.resolve`, `temporal.day_bounds` |
| Free slots: all slots without booked ones | `set.difference` |
| Earliest free slot | `list.min_by` |
| Cancel link in the confirmation email | `random.token`, `encoding.to_url_component` |
| Reminder a day before | `temporal.add_days` |
| A booking ends after it starts | Cross-field rule (gap: language) |
| Calendar invite file | `.ics` package (#88) |

**3. Delivery and logistics**

| Does | Needs |
|---|---|
| Driver's next stop and the rest | `list.first`, `list.drop_first` |
| Estimated arrival | `Instant + Duration` (area 10) |
| Is the address inside the zone, distance to depot | Geospatial (gap: future area) |
| Tracking code | `random.code` |
| Carrier manifest import | `csv.read` (#88) |
| Proof-of-delivery photo fingerprint | `hash.sha256` |
| Parcels by weight band | `list.group_by` |

**4. Property listings**

| Does | Needs |
|---|---|
| Asking price shown in the visitor's locale | `money.format` |
| Price per square metre | `money.divide` with `rounding:` |
| Listing address | `text.slug` |
| Description preview | `text.trim`, `text.keep_first` |
| Features filter | `set.contains`, `set.intersection` |
| Map search in the visible area | Geospatial (gap: future area) |
| Portal feed import | `json.read` as a batch (#88) |

**5. SaaS with subscriptions and billing**

| Does | Needs |
|---|---|
| Plan prices per customer's currency | `Money` with a currency known while running |
| Proration for a mid-month change | `money.multiply` with `rounding:` |
| Invoice total of lines | `money.sum` |
| Signed webhooks to customers | `crypto.hmac`, `json.write` |
| Customers' third-party tokens kept safe | `crypto.encrypt`, encrypted fields |
| Features per plan | `set.contains` |
| Usage per day | `map.get`, `map.with`; monthly totals need aggregate queries (gap: language) |

**6. CMS with CSV imports and exports**

| Does | Needs |
|---|---|
| Import articles from a spreadsheet | `csv.read` (#88), construction per row |
| Export for editors | `csv.write` with `CsvFormat.spreadsheet` (#88) |
| Tags typed by hand, tidied | `text.trim`, `text.lower`, `set.with` |
| Excerpt and word count | `text.keep_first`, `text.split`, `list.filter`, `list.count` |
| Articles in a locale's alphabetical order | `list.sort_by` with `locale:` |
| Rich text kept safe | The safe-HTML type (#88; not this page) |

**7. IoT telemetry**

| Does | Needs |
|---|---|
| Readings arriving in bulk | `json_lines.read` (#88) |
| Average, spread and extremes per window | `list.sum`, `decimal.divide`, `decimal.sqrt`, `list.min_by`, `list.max_by` |
| Drop repeated messages | `set.contains`, `list.distinct` |
| Latest reading per device | `list.group_by`, `list.last` |
| Hourly charts over months | Aggregate queries (gap: language) |
| Each device signs its messages with its own key | Keys held as data (gap: language) |
| Bit-packed payloads | The device's provider package (bits left out) |

**8. Internal admin tool**

| Does | Needs |
|---|---|
| Bulk actions on selected records | `Set<Uuid>`, `set.difference` |
| Email shown masked | `text.keep_first`, `text.keep_last` |
| Export of a filtered list | `csv.write` (#88) |
| A record's raw JSON for support | `json.write` |
| Counts by status | Aggregate queries (gap: language) |
| Roles | `set.contains` |
| Two-factor codes | TOTP package; passkeys or a provider through area 8 |

**9. Support assistant with retrieval (AI)**

| Does | Needs |
|---|---|
| Embed help articles and find the closest | `Vector`, pgvector query (gap: future AI area) |
| Answer as a typed reply with sources | `ai.generate(..., as:)` (gap: future AI area) |
| Show the answer as it is written | `ai.stream` (gap: future AI area) |
| Keep within a monthly budget | AI limits (gap: future AI area) |
| Strip personal data before sending | `text.replace`; area 15's report |
| Chunk articles into passages | `text.split`, `list.keep_first` |

### Coverage

● covered on this page or by #88; ◐ covered once the language change in
"Language gaps" lands; ○ a future area; blank: not needed.

| Group | Shop | Bookings | Delivery | Property | SaaS | CMS | IoT | Admin | AI |
|---|---|---|---|---|---|---|---|---|---|
| `list` | ● | ● | ● | | ● | ● | ● | ● | ● |
| `set` | | ● | | ● | ● | ● | ● | ● | |
| `map` | | | | | ● | | ● | | |
| Cutting either end | | | ● | ● | | ● | | ● | ● |
| `integer`, `decimal` | ● | | | ● | ● | | ● | | |
| `Decimal` maths | | | ○ | ○ | | | ● | | |
| `Money` | ● | | | ● | ● | | | | |
| `text` | ● | | | ● | | ● | | ● | ● |
| `hash`, `encoding` | ● | ● | ● | | ● | | | | |
| `crypto` | | | | | ● | | ◐ | | |
| `random` | ● | ● | ● | | | | | | |
| `json` in memory | | | | | ● | | | ● | ● |
| #88 file formats | ● | | ● | ● | | ● | ● | ● | |
| `temporal` | | ● | ● | | ● | | ● | | |
| Construction | ● | ● | ● | ● | ● | ● | ● | ● | ● |
| Cross-field rules | | ◐ | | | ◐ | | | | |
| Aggregate queries | | | | | ◐ | | ◐ | ◐ | |
| Geospatial | | | ○ | ○ | | | | | |
| AI | | | | | | | | | ○ |

**Gaps the scenarios found, and where each went:** human-readable
references (now `random.code`); money in a currency known only while
running (`Money`, proof by condition); searching sets (`set.*`);
grouping and the latest per key (`list.group_by`, `list.last`); spread
and distance (`Decimal` maths); JSON in memory (`json.read`/`write`);
locale order for editors (`locale:` on `sort_by`); cross-field rules,
aggregate queries and keys held as data (language changes below);
geospatial and AI (their own areas). Left out with a reason: bits (device
packages), TOTP (a package), calendar files (a package, per #88).

**The example apps** in `examples/` need nothing outside this page: 04's
allergens become a set once `set.contains` exists; 04's and 09's summing
loops become `list.sum`; 09's percentage discount becomes
`money.multiply`; 05's divisions and 10's due dates are already decided.

## Language gaps this page depends on

Nothing is left out because the language can't do it yet: each gap is
sorted into "left out by design" (with its reason, above and in the next
table) or a language change proposed here, with the operations it
unblocks kept in the proposal.

| Gap | Fix proposed | Owning area | Chunk | Unblocks |
|---|---|---|---|---|
| `for` can't walk sets or maps | `for` takes a `Set`, in area 2's canonical order; maps through `map.keys` | 2, 3 | 5.5b | Walking sets and maps |
| No map literal | `[]` where a map is expected; a literal with entries designed in area 2 | 2 | 5.5b | Building maps |
| Inline functions take one parameter | Two parameters, `(a, b) -> ...`, for standard operations only (area 3's question 13 widened) | 3 | 5.5b | `list.zip_with`, `map.filter`, `map.map_values` |
| No `Float` and no maths | Maths on `Decimal` with `scale:` and `rounding:`, deterministic everywhere; still no `Float` | 2 (question 8 there) | 5.5d | 8 `decimal` operations; geospatial distance |
| Code can't read a secret | Opaque key settings, used only as a `crypto` call's `key:` | 10 | 5.5h | `crypto` |
| Keys held as data (per device, per person) | An encrypted field of a key type, usable only as `key:` | 10, 15 | After encrypted fields | Per-device signatures, TOTP |
| No randomness | Generated `random`, actions only, seeded in tests, recorded for replay | 10, 13, 17 | 5.5h | `random.*`, `encrypt`, `hash_password` |
| Text has no locale order or case | `locale:` on `list.sort_by`, `text.lower` and `text.upper`, through ICU4X's collator and case mapping (two more crates of the pinned ICU4X) | 10 | 5.5e | Locale sorting and case |
| Numbers formatted only in translations | `integer.format`, `decimal.format`, `money.format`, giving `PresentationText` | 10 | 5.5c, 5.5g | Showing numbers in code |
| No parsing of numbers and times | `integer.parse`, `decimal.parse`, `temporal.parse_date`, `temporal.parse_instant`, wire forms only | 2 | 5.5c, 5.5e | Text from a person or a file cell |
| Queries compute nothing | Aggregate queries: `count`, `sum`, `min`, `max`, `average`, grouped by a field or a time bucket, using operations with SQL forms; area 6 already lists counts and sums in queries as deferred | 6 | New, after 5.6 | Reports, dashboards, usage totals |
| No rule across fields | Named rules on an object type (`rule ends_after_start: ends > starts`), proved or attempted like any rule | 2 | New, after 5.5 | Bookings, date ranges, price bands |
| Text from outside not normalised | NFC applied as text is decoded at a boundary | 2, 7 | 5.5e | Equal text comparing equal |
| `Money` is an application's own object | Built-in `Money` and `Currency` | 2 | 5.5g | `money` |
| No geospatial types | A future area | New area | After 5.5 | Geospatial |
| No AI pieces | A future area | New area | After geospatial | AI |

**Left out by design, after the audit:**

| Left out | Why | Instead |
|---|---|---|
| Generic functions in your own code | Area 3 decided against them (question 16 there): no type-parameter errors for agents to fight | The standard library stays generic |
| `reduce`, `fold` | A second way to accumulate | `for` with `var mut`; `list.sum` |
| Comparators | A key expresses every order, and an inconsistent comparator differs by target | `sort_by`, `reverse` |
| `items[i]` | Out of range needs an arm or a defect | `list.at` gives `T?` |
| Regular expressions as calls | A pattern is a type's rule, in linear time | `pattern:` and construction (area 2) |
| Graphemes | They'd disagree with `max_length` and PostgreSQL, and change with each Unicode version | Code points (question 17) |
| Dynamic keys on records | A field is its own type and carries its own policy and marks | `Map` |
| A dynamic JSON value | No scenario needs one | `json.read(..., as:)`; `Text` to pass on |
| CSV in memory | #88: one way to read rows | `csv.read` through a store |
| MD5, SHA-1 | Broken | A package (area 11) |
| Bitwise operations | Flags read better as a set; device formats belong in their package | `Set<Enum>`; question 21 |
| Exchange rates | Outside data with a time and a source | A provider package |
| Colour conversions (HSL, CMYK, contrast) | Specialist, and several defensible formulas | A package on the standard `Colour` |
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

## Review

### Status

| Feature | Status |
|---|---|
| `collection.count`, `contains`, `map`, `filter` | Decided (area 2) and built; renamed `list.` in 5.5 if question 2 holds |
| `integer.divide`, `decimal.divide` | Decided (area 3) and built |
| `temporal`, `locale` | Decided (area 10) and built; two parse operations added here |
| `Email`, `Url`, `IpAddress`, `FileType` | Decided (areas 2, 16) and built |
| Everything else on this page | Proposed (chunk 5.4); built in chunk 5.5 once decided |
| The format namespaces | Decided in #88, unmerged; `json.read` and `json.write` in memory proposed here |
| SQL forms | Proposed; built with computed backfills (chunk 5.6) |
| Geospatial, AI | Proposed as future areas |

### Consistency with the rest of the language

- **Area 1:** `list`, `set`, `map`, `text`, `money`, `hash`, `encoding`,
  `crypto`, `random`, `json`, `csv`, `tsv` and `json_lines` become
  reserved namespaces (`locale` already is). Only `text` clashes today:
  five page examples (area 2's `work_email(text:)`, areas 7's and 8's `var
  text: Memo`, area 9's `edit_note(text:)`, area 16's `search_todos(text:)`)
  and about ten Jadpo snippets in crate tests, renamed in 5.5.
- **Area 2:** `collection.` becomes `list.` (below); `Money`, `Currency`,
  `Country`, `Language`, `PhoneNumber`, `Percentage`, `Colour`,
  `PasswordHash` and `EncryptionKey` become built-in names; `for` and maps
  as "Language gaps" says.
- **Area 3:** inline functions may take two parameters for standard
  operations; `any`, `all`, `find`, `min_by`, `max_by`, `sort_by` and
  `group_by` take inline functions. Area 3 question 6's "text order
  belongs in a standard function" is `sort_by` with a text key.
- **Area 4:** decoding failures are conditions mapped by name, as area 10's
  `dst:` is, and overflow stays a defect.
- **Areas 5 and 6:** `Money`'s two columns; aggregate queries proposed for
  area 6; one-step updates unchanged.
- **Area 10:** a secret stays unreadable, with the `key:` of `crypto` added
  to the compiler's places; `random` generated like `clock`; `{price,
  number, currency}` added to the placeholder styles.
- **Area 15:** encrypted fields with one application key; per-person keys
  stay out, as decided; data sent to a model listed in its report.
- **#88:** its `json.decode` becomes `json.read` given text; its "built in
  or package" table is followed (money, countries, languages, phone
  numbers built in).
- **Compiler design** (`docs/compiler-runtime.md`, the standard library):
  the operation table gains `never_empty`, `never_negative`,
  `at_most_length_of`, literal-only `scale:` and `fill:`, conditions no
  option settles (`malformed`, `mismatch`, `unreadable`,
  `currency_mismatch`, `too_many_places`), a `sort_by` key limited to
  ordered kinds, actions-only operations, and a SQL form per operation.

**Renaming `collection`.** `collection.count`, `contains`, `map` and
`filter` are decided in area 2 and built (`crates/jadpo-check/std/collection.jadpo`,
`jadpo-std`'s operation table). 5.5 renames the stub to `list.jadpo`, the
four table entries and every call: about 40 files across areas 2, 3, 9, 13
and this page, examples 04, 06, 08, 09, 10, 11 and 12, and the checker,
interpreter and CLI tests. `collection.` then gives one error whose edit
writes `list.`, `set.` or `map.` from the argument's type.

### Decided

| Topic | Decision | Date |
|---|---|---|
| Shared vocabulary types | The standard library defines a type wherever two packages would otherwise each invent their own, even when the heavy operations live in a package or a later area; each has one construction that validates, one wire shape and one storage shape; positional pairs are designed out by named fields, and an axis order exists only in a conversion that names it (`geo.from_lng_lat`) (Richard) | 2026-10-17 |
| Packages use the standard types | A package's public `Object` whose only fields match a standard type's shape, by the common field names in any order (table under "Packages use the standard types"), is the warning `type.standard_equivalent`, whose `next:` is to use the standard type and convert at the edge; a warning on #88's publishing bar, not a refusal (Richard). Whether it becomes a blocking error with a declared opt-out, for app code too, is open question 1 | 2026-10-17 |

### Open questions

Richard's points first, then the first draft's remaining questions. The vocabulary-types principle and the package warning are decided ("Decided", above).

1. **Does `type.standard_equivalent` block until it is resolved?**
   Richard's refinement of the decided warning, proposed, not decided.
   Detection stops `check` and the build until one of two things is done:
   - **Swap to the standard type.** For a type not yet stored or on the
     wire, the error's fix makes the swap in one step, and an agent may
     apply it. For one already stored or in an API record, the fix writes
     the edit and the data migration, and the change goes through the
     existing `ApiChange` and `DataChange` acceptances (area 18).
   - **Decline, with a reason, on the type itself:**
     `not_standard: "Grid squares, not WGS84 degrees"` as a declaration
     setting in the type's body, as an entity's `store:` sits beside its
     fields (area 5). It costs one more reserved field name, beside
     `optional`, `include`, `pick` and `omit` (`type.reserved_field_name`).
     An agent or a person may decline; the approval report (`jadpo
     approval`, area 18; there is no `jadpo review`) lists every decline
     with its reason, so declining by reflex shows. Area 1's question 11
     proposes the same for every warning in Jadpo: if it is decided,
     `not_standard:` becomes its generic `decline type.standard_exists:
     "..."` line.

   *Recommendation:* yes, with three choices made:
   - An **error with an opt-out**, not a blocking warning: warnings never
     fail `check` today (area 1: `library.similar_exists` is "a warning,
     so `check` still passes"), and a warning that blocks would be the
     only one. Renamed `type.standard_exists`, to read as an error and
     match `library.similar_exists` and `constant.similar_exists`.
   - **Widened** from packages' public types to every type declaration in
     app code too, since an app's own `Location { lat, lng }` reaches its
     API and its tables exactly as a package's would.
   - **The cost is false positives:** an `Object` of `x` and `y` grid
     cells, or an `amount` and `currency` that are a quote, not money,
     matches by shape. The required reason handles them, and review sees
     each one.
2. **One namespace per type: `list.`, `set.` and `map.` replace
   `collection.`?** Costs: `list`, `set`, `map` and `text` can't be
   variable or parameter names (`locale` already can't), and the four
   built operations are renamed. *Recommendation:* yes. The namespace says
   what the first argument is, `set.contains` and `list.contains` stay
   distinct where their behaviour differs (order, repeats), a variable
   called `list` says nothing about what it holds, and only `text`
   clashes today.
3. **`keep_first`, `keep_last`, `drop_first` and `drop_last` on text and
   lists, with no negative positions?** *Recommendation:* yes, `n` never
   negative and proved so, `slice` kept for the middle. Negative positions
   are where JavaScript has three behaviours for one call.
4. **Sets and maps get their operations, `for` walks a set, and an empty
   map is `[]`?** *Recommendation:* yes: the seven set and eight map
   operations, all returning new values; `for` over a set in area 2's
   canonical order; maps walked through `map.keys`; a literal with entries
   left to area 2 (`["gbp": 2]`, Swift's form beside the list's brackets,
   is the obvious candidate). No dynamic keys on records.
5. **A `crypto` namespace with no algorithm choices, keys as opaque
   settings?** *Recommendation:* yes, the six operations as tabled:
   XChaCha20-Poly1305 with a required `context:`, Argon2id with
   `PasswordCheck.correct_outdated`, HMAC-SHA-256 with a constant-time
   check; `key: config.<name>` joins area 10's compiler-owned places.
6. **An `encrypted` field setting?** *Recommendation:* yes, to be decided
   in area 15, with one application key and the field kept out of indexes,
   unique rules, filters, ordering, search and logs, as area 16's table
   expects. Per-person keys stay out (area 15, question 5).
7. **No push or pop; `last`, `at`, `min_by`, `max_by`, `append`,
   `flatten`, `group_by` added?** *Recommendation:* yes; the page's own
   `first(reverse(sort_by(...)))` showed `max_by` was missing.
8. **Geospatial as its own area, and a roadmap entry for it?**
   *Recommendation:* yes, scope as sketched; add **"Geospatial: the plan"
   `opus · high`** right after 5.5, writing the new area page, with its
   build placed by that page. The roadmap isn't edited until you agree.
   - **`Point`, `Position` or `Location`?** *Recommendation:* `Point`.
     It is the GeoJSON, PostGIS and OGC term, and sits in the
     `LineString`, `Polygon`, `MultiPolygon` family. `Location` is what
     applications name their own entities (a branch, a delivery stop) and
     is an HTTP header; `Position` is GeoJSON's word for the bare
     coordinate array, the very ambiguity being designed out, and also
     means order in a list or on screen. The cost: built-in names can't
     be reused (area 2, "Built-in types"), so an app's own `x`/`y` point
     for drawing must be called something else (`CanvasPoint`), and area
     2's `struct Point` habit example and one CLI test's `type Point`
     are renamed when the type is built. A `Geo` prefix for the whole
     family (`GeoPoint`, `GeoPolygon`) avoids that but is clunky on every
     use, for a clash few apps have; rejected.
9. **`Colour` as a standard type?** This reverses your point 8 under the
    vocabulary principle. *Recommendation:* yes, the type only: named
    channels in sRGB, constructed from `"#rrggbb"`, one wire and storage
    form; conversions to other spaces stay a package.
10. **`json.read` and `json.write` in memory, one name per direction with
    #88's batch forms, no dynamic JSON value?** *Recommendation:* yes;
    #88's `json.decode` renamed `json.read`; `mismatch` carries the first
    problem's path and rule; CSV stays file-only as #88 decided.
11. **Construction is the way to validate mid-program, plus cross-field
    rules?** *Recommendation:* yes, construction as shown, nothing new for
    values or records; and named rules across an object's fields as an
    area 2 change, after 5.5, since the bookings scenario can't be covered
    without one.
12. **Built-in `Money` and `Currency` in minor units?** Sub-choices:
    - Operators or functions: *recommendation:* `+`, `-` and comparisons
      as operators, one way each, with functions only where a rounding
      must be named (`multiply`, `divide`).
    - A currency mismatch: *recommendation:* a compile error where both
      types fix the currency, and proved by an `if` on the currencies
      where it is known only while running.
    - Wire shape: *recommendation:* fixed, `{"amount": 1050, "currency":
      "GBP"}`; a provider's decimal string is mapped in its package.
    - Formatting: *recommendation:* `money.format` in code and `{price,
      number, currency}` in messages, one formatter.
    - `Percentage` as a vocabulary type beside it: *recommendation:* yes,
      a `Decimal` where `0.20` is 20%, with `{n, number, percent}` in
      messages.
13. **Maths on `Decimal` rather than a `Float`?** *Recommendation:* yes,
    the eight operations, correctly rounded to a named scale, the same on
    every target; area 2's "no `Float` until needed" stands, since this
    meets the need.
14. **Randomness as a generated `random` namespace?** This reverses the
    first draft. *Recommendation:* yes, actions only, seeded per test,
    recorded for replay, with `random.token` (your point 6's
    `crypto.random_token`) and `random.code` for references people read.
15. **AI as its own area, with the standard pieces sketched?**
    *Recommendation:* yes: the `ai` capability, typed `generate`,
    streaming, `Vector` and similarity queries, checked prompt files, a
    fake for tests, limits, and area 15 reporting; training out. Add **"AI:
    the plan" `opus · high`** after the geospatial plan.
16. **Are the language gaps closed as "Language gaps this page depends
    on" proposes?** *Recommendation:* yes, row by row: `for` over sets,
    the empty map and two-parameter inline functions in 5.5b; secrets as
    opaque keys and `random` in 5.5h; `locale:` order and case, parsing,
    `format` and NFC in 5.5c and 5.5e. Three need roadmap entries of their
    own, added once you agree: **"Cross-field rules" `opus · high`** (area
    2) after 5.5, **"Aggregate queries" `opus · high`** (area 6) after 5.6,
    and keys held as data after encrypted fields (area 15).
17. **Is text counted and cut in code points?** *Recommendation:* yes, as
    Python and PostgreSQL do; graphemes left out.
18. **Does `hash.sha256` give `Bytes`?** *Recommendation:* yes, written as
    text by `encoding`.
19. **Does `Rounding` gain `down`, toward zero?** *Recommendation:* yes,
    with SQL form `trunc`; `floor`, `ceiling` and `half_down` stay out
    until amounts below zero need them.
20. **Is `text.slug` ASCII-only, and does text sort in code points unless
    `locale:` says otherwise?** *Recommendation:* yes to both, matching
    URLs and queries.
21. **Bitwise operations as a `bits` namespace?** *Recommendation:* no,
    for now. No scenario but device payloads needs them, and those decode
    in their provider package; flags are a `Set` of an enum. If a protocol
    package needs them, six operations on `Int` fit without changing
    anything else.
22. **Are SQL forms used by backfills, and later by aggregate queries?**
    *Recommendation:* backfills in 5.6; aggregate queries as a new area 6
    chunk after it, using the same SQL forms, which also gives reports
    their `lower` (where a form exists) and time buckets.
23. **Does the table record `at_most_length_of`?** *Recommendation:* yes,
    for `trim`, `slice`, `keep_*` and `drop_*` only; `lower` and `upper`
    can lengthen text (`ß` becomes `SS`).

### Building it

5.5 as one chunk is far more than a session. Proposed split, each one
session, in order:

- **5.5a Lists** `opus · high`: `collection.` renamed `list.` everywhere,
  with its habit error; the 21 new list operations that need no language
  change; `never_negative`, the `sort_by` key's ordered kinds, results
  that keep a named type.
- **5.5b Sets and maps** `opus · high`: the 7 `set` and 8 `map`
  operations; `for` over a set; `[]` as an empty map; two-parameter
  inline functions and the 3 operations waiting on them.
- **5.5c Numbers** `opus · high`: the `integer` and `decimal` operations
  except maths, `parse` and `format` included; `scale:` written out;
  `Rounding.down` if question 19 says so.
- **5.5d Decimal maths** `opus · high`, if question 13 holds: the eight
  correctly rounded operations, tested against reference values.
- **5.5e Text** `opus · high`: the 18 `text` operations, `locale:` on case
  and sorting, `temporal.parse_date` and `parse_instant`, NFC at the
  boundary; `never_empty`, `fill:`, `at_most_length_of`; `text` reserved
  and the clashing examples renamed.
- **5.5f Hashing, encodings and JSON** `opus · high`: `hash`, `encoding`,
  `json.read` and `json.write` in memory with `mismatch`'s problem; the
  batch forms come with #88's batches.
- **5.5g Money** `opus · high`: `Money`, `Currency`, `Percentage`, the 10
  `money` operations, the two columns, the wire shape, `{price, number,
  currency}`, and area 2's examples renamed.
- **5.5h Randomness and cryptography** `opus · high`: the generated
  `random` namespace with test seeding; `crypto` with `EncryptionKey`,
  `PasswordHash`, `PasswordCheck` and `key:` as a compiler-owned place.
- **5.5i Vocabulary types** `opus · high`: `Country`, `Language`,
  `PhoneNumber` (E.164 normalisation) and `Colour`, each with its
  construction, wire and storage shape. The `type.standard_equivalent`
  warning comes with package publishing (#88, Phase 7), once the types it
  points at exist.
- **Later, outside 5.5:** encrypted fields with area 15; cross-field rules
  (area 2) and aggregate queries (area 6) as their own chunks; the
  geospatial and AI plans.
- **SQL forms move to 5.6**, computed backfills, their only user for now.
- **5.7** brings the teaching examples up to date, as planned.
