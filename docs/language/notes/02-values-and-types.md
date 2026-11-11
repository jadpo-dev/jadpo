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

## Open questions

1. **Typed holes: may an agent write `_` where it doesn't know the value,
   and have `jadpo check` say what goes there?** A recommendation, awaiting
   a decision (roadmap A19).

   *Recommendation:* yes, narrowly. `_` may stand where a value goes (an
   argument, a field of an object being built, a `return`, a `var`'s value,
   a `set:` line), and is always the error `type.hole`, decided by the
   agent:

   ```text
   error: `_` stands for a `Greeting.name`
    --> domains/hello/actions/greet.jadpo:7:15
     |
   7 |         name: _
     |               ^ a `Greeting.name` goes here
     reason: `Greeting.name` is a `PersonName { given: GivenName, family: FamilyName? }`. In scope, `input.name` fits.
     next: Write `input.name` (agent)
     rule: type.hole
   ```

   - **What it says:** the type, written as the code would write it, with
     an object's shape and the field type it is built through, as the
     first `type.mismatch` already gives them; then up to five values that
     fit, closest first: locals and parameters, their fields one level
     down (`input.name`), constants, and library and standard calls whose
     result fits. With one fit, `next:` carries its edit; with none, the
     edit that builds one (`Greeting.name(PersonName { given: ..., family:
     none })`) and `jadpo help <Type>` for a standard type. `--json` adds
     `expected` (the type as written) and `fits` (each one's text, type
     and edit), so a tool reads them without parsing the reason. Hovering
     over `_` in the editor shows the same.
   - **One form, `_`, and only as a value.** No named holes (`?name`, as in
     Agda and Idris): every error already gives its place. `_` is already
     the `match` wildcard, "anything" in a pattern; as a value it reads as
     "not chosen yet", as in Haskell. Today `_` as a value is
     `syntax.expected_expression`, which ignores the type the checker
     knows is wanted there, so this turns an existing, unhelpful error into
     a useful one, with no new token or reserved word. Not in a type
     (`List<_>`) or a pattern: type positions have completion and `jadpo
     help types`.
   - **Where nothing says the type** (`_ + 1`, `_.count`), the same rule
     says so, and to declare the value with its type first. That is rare,
     since every `var`, parameter, field and return states its type.
   - **It can never ship.** A hole is an error like every other finding
     (there is no warning severity) and can't be declined, so `check`,
     `test` and `build` refuse the project and CI fails it. There is no
     deferred mode, as GHC's `-fdefer-typed-holes` or Idris running until
     it reaches one: tests run only on a project that checks, and a hole
     that ran would be one more runtime failure for every reviewer to
     think about. Lean's `sorry`, a gap that compiles with a warning, is
     the shape to avoid. A hole is safer than what an agent writes today
     when it doesn't know, a placeholder that compiles (`""`, `0`) and can
     ship.

   **What it would save, measured against the `PersonName` detour.** In
   the outside agent's 0.2.0 run, runs 17 to 19 took 3 minutes 14 seconds
   and gave 7 of the run's 16 compiler errors (4, then 3). The agent wrote
   text where `PersonName`, an object, was required, believing it knew the
   type; then hit three 404s, one from a broken link, found `jadpo help
   PersonName` didn't exist, and read the Rust source at the release tag
   for the two fields; then wrapped the value in its field type
   (`Book.author(PersonName { ... })`) as the second error's edit said.

   A hole would have saved none of that: an agent writes `_` only where it
   knows it doesn't know, and this one thought it knew. What removes the
   detour is already built: links that can't break, `jadpo help
   PersonName`, and a first `type.mismatch` that shows the shape and the
   field type together. Reproduced on 2026-11-08 with jadpo 0.3.0 in a new
   project giving an input and an output a `PersonName` field, written as
   text in the action and its test: the first `jadpo check` names the
   shape, the wrapper, `jadpo help PersonName` and an exact edit at each
   place, with no lookup. One round is left: the edit writes `PersonName {
   given: "Ann" }`, which is `type.missing_field` for `family`, since an
   object literal writes a nullable field as `none`. Fixing that edit, not
   a hole, makes the detour one `check`.

   Where a hole does help is the case the run didn't have: an agent that
   knows it doesn't know which value goes in, which field types make
   common (`Customer.email` against `Email`, a reference's `Customer.id`
   against `Supplier.id`). Today it guesses and reads a `type.mismatch`,
   one `check`; with a hole it is still one `check`, but the answer lists
   what in scope fits rather than leaving the agent to search for it. So
   the saving is lookups, not runs, and it is unmeasured: the next outside
   run's journal should count the holes written and the lookups after
   them.

   **Its size:** one piece of work of moderate size. The parser makes `_` a
   value node; the checker already records the type each position
   requires, for completion, and raises `type.hole` there; the fits come
   from completion's names in scope and its test of what fits, which
   already puts what fits first, adding fields one level down; the shape
   and the wrapper come from `type.mismatch`'s object builder; then the
   rule's page, a section on this area's page, and area 14's lines for
   `check --json` and hover. Nothing in the runtime, the interpreter or
   code generation changes, since a project with a hole never passes
   `check`.

   **What it overlaps:** completion, the same list ranked the same way,
   for an agent that runs `check` rather than an editor; `type.mismatch`,
   the same shape, wrapper and building edit; `jadpo help <Type>`, which
   it points to; and `jadpo inspect` (roadmap A17), the project map, where
   a hole answers at one place. A position query, `jadpo inspect
   <file>:<line>:<column>`, was weighed instead of syntax and rejected: an
   agent edits text and doesn't know columns, and writing `_` is an edit
   it already makes.

   **Prior art.** GHC reports `_` (and `_name`) as "Found hole" with its
   type, the bindings in scope and the "valid hole fits", an error unless
   deferred. Agda's and Idris's `?` goals show the type and context in the
   editor, and Idris 2 runs a program until it reaches one. Lean's `_`
   asks the elaborator to fill it and `sorry` admits a gap. TypeScript has
   none: agents write `as any` or a dummy value. Work on Hazel (OOPSLA
   2024) found that giving a model the type and the relevant definitions
   at a hole improved what it wrote, which is the information this report
   gives.

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
| Names from reserved words (Richard, 2026-11-03) | Inside a type's body a reserved word may name a field or a variant, where `package: Text` and `Kind.library` can't be misread; an entity's body counts, since `pick:` copies its fields and a query names them. Refused, as positions that can't be read safely: a word alone in a value, which a rule's condition would need (`syntax.expected_name`), though a whole list item may be one, as `unique:`, a policy's lines and `order:` name fields that way, and `true`, `false` and `none` as variants (`type.reserved_variant_name`), since `Kind.none` beside a `Kind?` reads as the `none` it isn't. `wire_name:` stays provider-package only; see [Names from reserved words](../02-values-and-types.md#names-from-reserved-words). |
| No names that dodge a reserved word (Richard, 2026-11-04) | Once a field or variant may take the word, nothing stands in for it: `ProblemDetails.type` (RFC 9457's `type`, no longer `type_`), `Jwk.use` (RFC 7517's `use`), `Country.in`, `Language.in` and `Language.or`, and the Anthropic package's four `type` fields (were `kind` with `wire_name: "type"`). Kept, since each is still refused or was never a dodge of a reserved word: `units.in_` where its stub declares a function, which code calls as `units.in`; `web.with_query`'s `entries`, a parameter; the settings `generate_routes:` and `watches:`; `Currency.all_`, since `all` is an enum's own list; the importer's `_value` on an operation or a variant `none`. `IdentityDocument.kind` keeps its name for now: it is sent and stored as `kind`, so renaming it changes a form area 19 promises to go on reading, and needs a new encoding, a reader and a column migration. |
| Rules across fields | An `Object`, `Input` or `Output` names rules across its fields, `rule ends_after_start: ends > starts`; see [Rules across fields](../02-values-and-types.md#rules-across-fields) (area 19, "Validating mid-program"). |
| Set order | One canonical order for everything that shows a set; see [Collections](../02-values-and-types.md#collections). |
| Named collection construction | A named collection type is built from a collection whose items are already its item type, checking only the collection's own rules; see [Named collection construction](../02-values-and-types.md#named-collection-construction). |
| Lists | Square brackets are the only way to write a list: `pick: [Todo.id, Todo.title]`, `[create, read]`; braces never hold comma lists, apart from a `use` line's names (area 1); signatures keep their commas (parameters, `fails A, B`). |
| Messages and `PluralCount` | `Message` is a type base for translated messages, its fields the placeholders; `PluralCount` is a built-in whole number that picks a plural form, and `TextDirection` a locale's direction; enums have no display labels, and their wording for people lives in translations (area 10). |
| Types from the services decisions | `File`, `FileUpload`, `UploadSlot`, `FileContents`, the batch types, `CsvFormat`, `SearchText`, `OAuthGrant`, `AnyObject` and the `Document` base, as [Other built-in types](../02-values-and-types.md#other-built-in-types) lists them (areas 11, 12 and 16). |
| What an `AnyObject` takes (2026-11-11, the obvious recommendation) | One of the app's own `Object` types, declared in a domain or `shared/` outside every package, or a field type naming one (`Order.details`), or an `AnyObject` passed on; anything else is `type.mismatch`, saying an object type goes there. Not a package's type, which the page's "the app's object types" leaves out and whose fields go on a provider's wire in that provider's spellings; not an entity, an `Input`, an `Output` or an event, whose shapes are a store's or a route's, not something to send as variables. A package's own code never names one of the app's types, so there it only passes on an `AnyObject` it was given. |
| Where `AnyObject` is written (2026-11-11, the obvious recommendation) | Only as a field of a provider package's `Object` type, `AnyObject` or `AnyObject?`, never in a `List`, `Set` or `Map`, a signature, a `var` or the app's own types: `type.any_object_outside_input`. A collection is left out because its items would each be any type, which no template or metadata needs and assignability doesn't follow (a `List<Order>` is never a `List<AnyObject>`). A type that holds one, however deep, may be only what an operation's `input:` holds: an operation's `output:` or `wire_output:`, a webhook's `reply:` and a type with any base but `Object` (an event, an email, an `Input`, an entity) holding one is the same rule, since nothing records which type its JSON was and so nothing reads one back. |
| How an `AnyObject` goes out (2026-11-11, the obvious recommendation) | As its own type's codec writes it, the app's contract, with `none` fields left out as on any provider wire. Because the package's input doesn't declare what it holds, the checks a declared input gets run where the object is given: a `Password` in it is `type.password_sent_to_provider`, declined above the action as at a call; a `File` is sent as its bytes and a `FileContents` as its Base64, as a declared input sends them (chunk B14), so a store and key never go out; other secret values go out as themselves, as in a declared input. The plan holds a codec for each of the app's object types under the name its records carry, built only when a type holding an `AnyObject` is sent. |

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

[Standard types](https://jadpo.dev/docs/types) lists every type, and
[Standard library](../19-standard-library.md) every operation.
