---
title: Values and types, design notes
area: 2
---

# Values and types: design notes

Why [Values and types](../02-values-and-types.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Carried to later areas

| Item | Area | Status |
|---|---|---|
| Match arms use `->`, the same arrow as return types; there is no `=>` | 3 | Decided |
| A function argument that accepts several types: an `Enum` whose variants carry each type, with no generics or unions | 3 | Decided in area 3, question 16 |
| Failure `kind` values are written `FailureKind.not_found`, following the setting-options decision | 4 | Follows from a decision |
| Entity field settings (`identity`, `unique`, `references`, `generated`) go in the field's block, following question 20 | 5 | Follows from a decision |
| A package input that takes any of the app's object types, serialised as JSON (Postmark's template variables, Stripe's metadata) | 2, 11 | Decided: the built-in `AnyObject` |
| `5%` as a percentage literal (`free_space_floor:`, area 16) | 2 | Follows from a decision |

## Decisions

| Topic | Decision |
|---|---|
| Runtime construction (question 1) | A runtime `Type(value)` check is fallible: it needs `attempt` and a block mapping each rule that can break to an `invalid_value` failure, `invalid:` covering the rest (see [Construction failures by rule](../02-values-and-types.md#construction-failures-by-rule)). A literal is checked while compiling and never fails at runtime. The failure is in the signature, as with Rust's `TryFrom`, rather than an exception. |
| Object construction (question 2) | Building an object or creating an entity accepts the declared type or anything that widens to it. An object literal never fails: a field whose value might break its own rules is built first, on its own line. Identifiers stay apart, because a reference field's declared type is itself a field type (`customer_id: Customer.id` still rejects `supplier.id`). |
| `optional` and `default` (question 3) | Only on `Input` fields; reading an optional field gives `T?`, a field left out reading as `none`; only the update `patch:` form keeps omitted and `none` apart. TypeScript's `field?: T`, read as `T` or `undefined`, is the same idea. |
| Collection operations (question 4) | List literals (`[a, b]`), `for item in items`, `list.count`, `list.contains`, `list.map` and `list.filter`. Collections stay invariant, as in Kotlin and Rust; TypeScript's covariant arrays are unsound. |
| `Set` and `Map` in JSON (question 5) | A `Set<T>` is a JSON array, refused if it has duplicates; a `Map<K, V>` is a JSON object, allowed at a boundary only when `K` is built on `Text` or `Uuid`. |
| Transparent aliases (question 6) | None: every `type` is a new type. The block-less `type OrderId = Uuid` is refused, because in TypeScript, Rust and Kotlin that shape is a transparent alias, so an agent or reviewer would misread it. |
| Representation access (question 7) | No `as` cast and no unwrap; constructing is the only conversion, and operators work on the representation. |
| Number types (question 8) | No sized types such as `Int32` or `UInt8`: size is a storage detail, and the business rule is the range declared with `min` and `max`, from which the compiler picks the database column (`smallint`, `integer`, `bigint`). `Int` is an exact 64-bit integer whose overflow is an error, never a silent wrap; JSON carries it as a number when the declared range fits JavaScript's safe range and as a string otherwise. `Decimal` is exact, with a required `scale:`; operations that can produce extra places, such as division or percentages, name a rounding mode (`decimal.divide(total, people, rounding: Rounding.half_even)`). JSON carries a `Decimal` as a string (`"12.50"`); PostgreSQL stores `numeric(p, s)` and SQLite, which has no exact decimal, a scaled integer. No `Float` until an application needs approximate measurement, and then as its own explicit type, so money can never use it by accident. |
| `pattern` and `format` (question 9) | A pattern matches the whole text, uses a regular-expression subset with no backreferences or lookaround (RE2-style, so it runs in linear time and behaves the same on every target), is checked while compiling and applies to literals. `format` accepts only `email`; `Url` and `IpAddress` are types, not formats. Every constraint fits its base type, and contradictory or repeated bounds are errors. |
| Type spellings (question 10) | Every type is `type Name = Base { ... }`, including `Object`, `Enum`, `Input` and `Output` (see [One form for every type](../02-values-and-types.md#one-form-for-every-type)); only an `Input` may have optional or default fields; a route accepts only an `Input` and returns only an `Output`. |
| Signature rule scope (question 11) | Representation types and `Instant`, `CalendarDate`, `Time` and `Duration` are never in a signature unnamed, since a bare `Instant` parameter is as ambiguous as a bare `Int`; the closed enums `Zone`, `Locale` and `Weekday` may be used directly; collection element types are checked, and inline `Object` parameters refused. |
| Equality (question 12) | `==` compares contents field by field on objects, lists and enum payloads, with sets and maps compared as sets, as in Rust's derived `PartialEq`, Kotlin data classes and Elixir. Refusing `==` on objects would force agents to write field-by-field comparisons by hand. |
| UUID versions (question 13) | Any version accepted by default and normalised to lowercase; new identities are version 7, which is time-ordered and keeps indexes compact; a named type can pin one, `version: UuidVersion.v4`, governing what is accepted and generated, for example where a version 7 ID would reveal creation times or another system dictates the version. |
| Building types (question 14) | Body lines read top to bottom: `include:`, `pick:`, `omit:` and written fields, under the plain `Output { }` header; see [Building a type from other types](../02-values-and-types.md#building-a-type-from-other-types). Named results only, which covers create, patch, view, summary and versioned shapes without TypeScript's anonymous type algebra, whose expanded types reviewers and error messages cannot name. |
| Literal typing (question 15) | Where the expected type is known (a variable's declared type, a return, a field or an argument), a literal becomes that type and its rules are checked while compiling, so `var price: ItemPrice = 0.0` and `return 1` from a function returning `Quantity` just work. Computed values still need `ItemPrice(x)`, so a constructor in code always means a real check or a change of type. The type is written once: `var price: ItemPrice = 0.0`, never `var price: ItemPrice = ItemPrice(0.0)`, and the formatter rewrites `var price = ItemPrice(0.0)` to that form. |
| Variable types (question 16) | Every `var` states its type (`var total: Stock = Stock(per_pack * packs)`); loop and `match` bindings take theirs from what they bind. The reader sees each value's type where it is introduced, and a wrong type is reported on the line that made it. The verbosity falls on the agent writing the code rather than the person reviewing it, and literal typing removes most of it. |
| Setting options (question 17) | A setting with a fixed set of options takes an enum value, `Type.value`, the same form as in code: `version: UuidVersion.v7`, `format: TextFormat.email`, `kind: FailureKind.not_found`, `on_delete: OnDelete.cascade`. Quantities such as `min`, `max` and `scale` stay numbers. A bare word in a setting is therefore always a name you declared (`store: cache`), and the type name says where the choices are listed. Option values read as a value or a moment, never a verb (`Generated.on_create`, not `Generated.create`). |
| Object keyword (question 18) | An object used inside the application is `type Name = Object { fields }`; there is no `value` keyword. |
| `Object` (question 19) | Only a base in a `type` declaration; an unnamed shape written inline as a field type or inside `List<...>` is an error. |
| Field settings (question 20) | A field's settings go in a block after its type, one `name: value` per line (`optional: true`, `default: 25`, `default: none`), as every other block reads; entity field settings follow the same rule (area 5). |
| Rules across fields | An `Object`, `Input` or `Output` names rules across its fields, `rule ends_after_start: ends > starts`; see [Rules across fields](../02-values-and-types.md#rules-across-fields) (area 19, "Validating mid-program"). |
| Set order | One canonical order for everything that shows a set; see [Collections](../02-values-and-types.md#collections). |
| Named collection construction | A named collection type is built from a collection whose items are already its item type, checking only the collection's own rules; see [Named collection construction](../02-values-and-types.md#named-collection-construction). |
| Lists | Square brackets are the only way to write a list: `pick: [Todo.id, Todo.title]`, `[create, read]`; braces never hold comma lists, apart from a `use` line's names (area 1); signatures keep their commas (parameters, `fails A, B`). |
| Messages and `PluralCount` | `Message` is a type base for translated messages, its fields the placeholders; `PluralCount` is a built-in whole number that picks a plural form, and `TextDirection` a locale's direction; enums have no display labels, and their wording for people lives in translations (area 10). |
| Types from the services decisions | `File`, `FileUpload`, `UploadSlot`, `FileContents`, the batch types, `CsvFormat`, `SearchText`, `OAuthGrant`, `AnyObject` and the `Document` base, as [Other built-in types](../02-values-and-types.md#other-built-in-types) lists them (areas 11, 12 and 16). |

## Why so many standard types

- **Whole categories of bug disappear:** unit confusion, floating-point
  money, mixed currencies, latitude and longitude the wrong way round, and
  month-end and time-zone arithmetic.
- **No choosing between competing libraries** for dates, money or addresses:
  there is one of each.
- **Packages and apps speak the same types,** so integration code doesn't
  translate between them. `type.standard_exists` keeps it that way: e-commerce
  packages share `Money`, `Price`, `UnitPrice`, `TaxRate`, `Discount`, `Sku`,
  `PostalAddress`, `Email` and `PhoneNumber` instead of each declaring its own.
- **Validation, storage columns, JSON and API formats come with each type.**
- **Formatting follows the locale** the same way everywhere.
- **Agents can't hand-roll a broken `Money`,** and a reviewer who reads
  `Money` knows exactly what it means.
- **Hand-rolled versions are found for you.** `jadpo suggest` scans a
  project for money, durations, units, points and loops written by hand
  (`price * 100`, `timeout_ms: Int`, `"£" + text.from(pence)`) and names
  the standard type or operation to use; the agent makes the edit. A miss
  it should have caught is taught with `jadpo suggest found`, and with
  your approval `jadpo suggest share` sends what was learned to Jadpo, so
  the signals improve for everyone ([tooling](../14-tooling.md#suggestions)).

[Area 19](../19-standard-library.md#the-standard-types) has the full list and
every operation.
