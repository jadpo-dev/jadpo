---
title: Values and types
area: 2
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Values and types
<!-- topic: values-and-types -->

What kinds of value a Jadpo program works with, how you give a value a
meaning in your business, and when one type can stand in for another.

The idea behind all of it: **how a value is stored is not what it means**. An
email address and a username are both text underneath, and an order ID and a
customer ID are both UUIDs, but Jadpo treats them as different types. A value
becomes a type only by being checked against that type's rules, and the
compiler never converts between types without being asked.

## Built-in types
<!-- topic: built-in-types -->

Jadpo comes with its types built in. You use them without declaring or
importing anything, and you can't reuse their names for your own types. There
are three groups: the six representation types, the standard types, and a few
types the language's own features need.

### Representation types

The representation types say how a value is held: as a number, as text, as
bytes. They are the raw material you build your own types from.

| Type | What it holds |
|---|---|
| `Bool` | `true` or `false` |
| `Int` | An exact 64-bit integer |
| `Decimal` | An exact decimal with a required `scale`, the number of decimal places it keeps |
| `Text` | Unicode text |
| `Bytes` | A sequence of bytes |
| `Uuid` | A universally unique identifier |

There is no `Float`. Arithmetic on `Decimal` is exact, and its other
operations are under `decimal.` ([Numbers](19-standard-library.md#numbers)).

Text that arrives from outside, such as a request body, is put into Unicode's
NFC form as it is read, before its rules are checked. NFC writes each
character one way, so the same text always compares equal.

By default a `Uuid` accepts any UUID version and is kept in lowercase. The IDs
Jadpo generates for new records are version 7. A named type can fix the
version, which decides both what it accepts and what it generates:

```jadpo check project=q13-uuid-versions
entity Order {
    id: Uuid  // generated as version 7
}

type PartnerRef = Uuid {
    version: UuidVersion.v4  // accepts and generates only version 4
}
```

### Standard types

The standard types are one shared definition each of the things most apps
need: a postal address, an amount of money, a length, a point on the earth.
Without them, every app and every package would invent its own. Each has one
way to build it that checks its rules, one JSON shape and one storage shape.
Its fields are named, never positional:
`Point { latitude: ... longitude: ... }`.

Their names are global, like `Int`, so you never import one. Their
operations are grouped under a prefix such as `money.`, `units.` or
`temporal.`. There are 253 standard types, and
[The standard types](19-standard-library.md#the-standard-types) lists every
one and every operation. The groups and their best-known types are:

| Group | Headline types | Namespaces |
|---|---|---|
| Money and payments | `Money`, `Currency`, `ExchangeRate`, `Price`, `TaxRate`, `Discount`, `Iban`, `BankAccount`, `PaymentCardNumber` | `money` |
| Time and calendar | `Instant`, `CalendarDate`, `ZonedTime`, `Duration`, `LocalTime`, `DateRange`, `RecurrenceRule`, `OpeningHours` | `temporal`, `range` |
| Quantities and units | `Length`, `Mass`, `Temperature`, `Speed`, `Energy`, `DataSize`, `Percentage`, `Ratio`, `Range<T>` | `units`, `range` |
| People and places | `PersonName`, `Username`, `Email`, `PhoneNumber`, `PostalAddress`, `PostalCode`, `Country`, `Language`, `Locale`, `Zone` | `locale`, `text` |
| Web and network | `Url`, `Slug`, `DomainName`, `HttpStatus`, `IpAddress`, `Cidr`, `Cookie` | `web` |
| Software | `SemanticVersion`, `GitRef`, `FilePath`, `TraceId`, `Ulid`, `IdempotencyKey` | `semver` |
| Text and documents | `Markdown`, `Html`, `RichText`, `LocalisedText` | `markup`, `text` |
| Media | `Colour`, `ImageDimensions`, `AspectRatio`, `FrameRate`, `VideoCodec`, `Timecode`, `ContentRating` | `colour`, `media` |
| Security and secrets | `Password`, `PasswordHash`, `Hash`, `Hmac`, `Jwt`, `PublicKey`, `Certificate`, `ApiKey`, `SecretText`, `EncryptedValue` | `crypto`, `hash`, `random` |
| Geospatial | `Point`, `Latitude`, `Polygon`, `Geometry`, `BoundingBox`, `Geohash` | `geo` ([Geospatial](20-geospatial.md)) |
| AI | `TokenCount`, `TokenUsage`, `ChatMessage`, `ChatRole`, `Vector`, `Embedding` | `ai` ([AI](21-ai.md)) |
| Codes and identifiers | `Sku`, `Ean13`, `Isbn`, `Barcode`, `TrackingNumber`, `Vin`, `VatNumber`, `NhsNumber`, `Icd10Code`, `IataAirport`, `FlightNumber`, `Orcid` | none beyond the contract below |

Every standard type, by group. "(enum)" marks a fixed list of values,
held as a built-in [enum](#enums):

| Group | Types |
|---|---|
| People and contact (12) | `PersonName`, `GivenName`, `FamilyName`, `Username`, `Email`, `EmailWithName`, `PhoneNumber`, `PhoneExtension`, `PostalAddress`, `PostalCode`, `Uprn`, `Nationality` (enum) |
| Places (10) | `Country` (enum), `CountrySubdivision` (enum), `Continent` (enum), `UnLocode`, `CallingCode`, `Language` (enum), `WritingScript` (enum), `TextDirection` (enum), `Locale` (enum), `Zone` (enum) |
| Geospatial (17) | `Point`, `Latitude`, `Longitude`, `LineString`, `Polygon`, `MultiPoint`, `MultiLineString`, `MultiPolygon`, `Geometry`, `BoundingBox`, `Circle`, `Bearing`, `Crs`, `EastingNorthing`, `MapTile`, `H3Cell`, `Geohash` |
| Money, payments, banking (24) | `Money`, `Currency` (enum), `ExchangeRate`, `Price`, `UnitPrice`, `TaxRate`, `Discount`, `BasisPoints`, `InterestRate`, `PaymentCardNumber`, `MaskedCardNumber`, `CardBrand` (enum), `CardBin`, `CardExpiry`, `PaymentReference`, `InvoiceNumber`, `Iban`, `Bic`, `UkSortCode`, `UkAccountNumber`, `BankAccount`, `AbaRoutingNumber`, `Lei`, `Isin` |
| Tax and identity numbers (12) | `VatNumber`, `TaxId`, `UkUtr`, `UkCompanyNumber`, `UkNationalInsuranceNumber`, `UsSsn`, `UsEin`, `PassportNumber`, `DrivingLicenceNumber`, `IdentityDocument`, `IdentityDocumentKind` (enum), `NhsNumber` |
| Commerce and logistics (22) | `Sku`, `Ean13`, `UpcA`, `Isbn`, `Sscc`, `Barcode`, `UnitQuantity`, `Rating`, `ClothingSize`, `ClothingSizeSystem` (enum), `ShoeSize`, `ShoeSizeSystem` (enum), `Dimensions`, `HsCode`, `Incoterm` (enum), `TrackingNumber`, `Carrier`, `ContainerNumber`, `LotNumber`, `SerialNumber`, `Vin`, `VehicleRegistration` |
| Time and calendar (19) | `Instant`, `CalendarDate`, `ZonedTime`, `Duration`, `Weekday` (enum), `LocalTime`, `LocalDateTime`, `Month` (enum), `YearMonth`, `Year`, `Quarter`, `IsoWeek`, `UtcOffset`, `DateRange`, `InstantRange`, `CalendarPeriod`, `RecurrenceRule`, `OpeningHours`, `CalendarEvent` |
| Physical quantities (17) | `Length`, `Area`, `Volume`, `Angle`, `Mass`, `Pressure`, `Energy`, `Power`, `Temperature`, `TemperatureDifference`, `Speed`, `Frequency`, `Voltage`, `Current`, `Illuminance`, `SoundLevel`, `Concentration` |
| Maths and data (8) | `Percentage`, `Ratio`, `Fraction`, `Range<T>`, `DataSize`, `Bitrate`, `Vector`, `Embedding` |
| Health (10) | `BloodType` (enum), `Icd10Code`, `SnomedConcept`, `Dose`, `DosageInstruction`, `RouteOfAdministration` (enum), `HeartRate`, `BloodPressure`, `BloodGlucose`, `Allergen` (enum) |
| Media and visual (16) | `Colour`, `ImageDimensions`, `AspectRatio`, `Rect`, `Insets`, `PaperSize` (enum), `ExifOrientation` (enum), `AltText`, `FrameRate`, `VideoResolution`, `VideoCodec`, `AudioCodec`, `Timecode`, `SubtitleCue`, `ContentRating`, `ContentRatingScheme` (enum) |
| Web and network (23) | `Url`, `UrlPath`, `RelativeUrl`, `Slug`, `DomainName`, `Hostname`, `WebOrigin`, `HttpMethod` (enum), `HttpStatus`, `HttpHeaderName`, `FileType`, `UserAgent`, `ProblemDetails`, `JsonPointer`, `Cookie`, `UtmParameters`, `IpAddress`, `Cidr`, `NetworkPort`, `MacAddress`, `Imei`, `Emoji`, `PushToken` |
| Software (19) | `SemanticVersion`, `VersionRange`, `GitRef`, `PackageUrl`, `Cve`, `SpdxLicence`, `TraceId`, `SpanId`, `TraceParent`, `LogLevel` (enum), `FilePath`, `FileName`, `RegularExpression`, `ContainerImage`, `Digest`, `Uuid`, `Ulid`, `IdempotencyKey`, `Doi` |
| Text formats (4) | `Markdown`, `Html`, `RichText`, `LocalisedText` |
| Security (20) | `Password`, `PasswordHash`, `PasswordPolicy`, `Hash`, `Hmac`, `Jwt`, `JwtClaims`, `Jwk`, `PublicKey`, `PrivateKey`, `Certificate`, `TotpSecret`, `OneTimeCode`, `ApiKey`, `SecretText`, `BearerToken`, `OAuthScope`, `OidcClaims`, `WebAuthnCredential`, `EncryptedValue` |
| Travel (6) | `IataAirport`, `IcaoAirport`, `IataAirline`, `FlightNumber`, `CabinClass` (enum), `BookingReference` |
| Other with a published standard (10) | `UnNumber`, `Orcid`, `CefrLevel` (enum), `Mpan`, `Mprn`, `EpcRating` (enum), `Tenure` (enum), `LawfulBasis` (enum), `Jurisdiction`, `ModelId` |
| AI (4) | `TokenCount`, `TokenUsage`, `ChatMessage`, `ChatRole` (enum) |

The money types have a few rules of their own, because money is where
mistakes cost most. `Money` holds a whole number of a currency's minor units,
such as pence or cents, and its `Currency`. It never holds a decimal number of
pounds. A type built on `Money` takes `currency:`, `rounding:`, and `min:` and
`max:` in minor units. The `money.` operations (`money.from_minor`,
`money.minor`, `money.sum`, `money.allocate`) build amounts and work them out.
A named type for an amount in pounds:

```jadpo check project=money-type
type Sterling = Money {
    currency: Currency.gbp
    min: 0
    rounding: Rounding.half_even
}
```

An amount in another currency doesn't compile where a `Sterling` is wanted.
Use `Int` for counts and sizes, and `Money` for amounts of money.
[Money and payments](19-standard-library.md#money-and-payments) has the rest.

#### What every standard type does

Every standard type meets the same contract, so one operation works across all
of them:

- **Construct and validate.** From its parts (`PostalCode(raw)`, or
  `money.from_minor` for `Money`), and from text with
  `text.parse(raw, as: T)`. Each failure is named by the rule it breaks, and
  `malformed` means the text isn't that type at all.
- **Show and write.** `text.format(value, locale:)` gives text for people, and
  `text.from(value)` gives the canonical text (E.164, ISO codes, ISO 8601)
  that `text.parse` reads back. A type with no single text form, such as
  `PostalAddress`, can't be asked for one.
- **Storage.** One shape: a column, or named columns for an object (`Money`'s
  two).
- **JSON.** One shape, read and written by `json.read`, `json.write` and every
  route.
- **Equality and order.** `==` works on every type; `<` and `sort_by` work
  where order means something (quantities of one kind, `Money` of one
  currency, times, `SemanticVersion`, `Percentage`).

A type that copies a standard type under another name is the error
`type.standard_exists`. For example, a `Booking` with only a start and an
end, each an `Instant`, is an `InstantRange`. When the copy is deliberate,
you can [decline the error](01-project-layout-and-modules.md#declining-a-judgement-call)
with a reason. A declaration with the same name
as a standard type is `type.standard_clash`. Both are explained under
[The standard types](19-standard-library.md#the-standard-types) and
[A release that adds a standard type](19-standard-library.md#a-release-that-adds-a-standard-type).

### What the standard types do

Standard types come with their own arithmetic, units and rules, so you
write a sum as you would on paper and it is checked as strictly as a call.
Which operators work on which types is set by the
[standard operator table](19-standard-library.md#the-standard-operator-table).

A sum of lengths can mix units, and the result is a `Length`. Inches are
written `inch`, because `m` means minutes:

```jadpo check project=std-length
type Shelf = Length {}

/// A metre, 2.3 inches and 3 centimetres, then 20% of that: a `Length`.
function fifth_of(base: Shelf) -> Shelf {
    return Shelf((base + 2.3inch + 3cm) * 20%)
}
```

```jadpo check project=std-length file=main.test.jadpo
test "units mix within a kind" {
    assert fifth_of(Shelf(1metre)) == Shelf((1metre + 2.3inch + 3cm) * 20%)
}
```

Mix up a unit and the compiler says so. Here `1m` is one minute, a
`Duration`, which can't be added to a `Length`:

```jadpo check project=std-length-minutes expect=type.arithmetic_operand
type Shelf = Length {}

/// `m` is minutes, so this adds a `Duration` to a `Length`.
function mistaken(base: Shelf) -> Shelf {
    return Shelf((1m + 2.3inch + 3cm) * 20%)
}
```

```text output
error: A duration and a quantity can't be added: a `Duration` and a `Length` are different kinds
 --> main.jadpo:5:19
  |
5 |     return Shelf((1m + 2.3inch + 3cm) * 20%)
  |                   ^^^^^^^^^^^^ different kinds
  reason: `+` works within one kind of quantity, in any of its units; a `Duration` and a `Length` are different kinds, so the sum means nothing.
  next: Use values of one kind, or a product or quotient the table defines (agent)
  rule: type.arithmetic_operand
  help: https://jadpo.dev/docs/errors/type.arithmetic_operand

check failed: 1 error, 1 file
```

Money is a whole number of minor units in one currency, so the operator
table allows less on it. A percentage scales it: `price * 120%` adds 20% VAT.
To share an amount out, use `money.split`, which loses no penny: £10.00 three
ways is £3.34, £3.33 and £3.33. Dividing, as in `price / 3`, is refused with
`type.money_split`, which suggests `money.split`
([Money operations](19-standard-library.md#money-operations)):

```jadpo check project=std-money
type CakePrice = Money {
    currency: Currency.gbp
    min: 0
}

type Payment = Money {}

/// A price with 20% VAT added.
function with_vat(price: CakePrice) -> CakePrice {
    return CakePrice(price * 120%)
}

/// A bill shared three ways, every penny to someone.
function shares(total: CakePrice) -> List<Payment> {
    return list.map(money.split(total, 3), share -> Payment(share))
}
```

```jadpo check project=std-money file=main.test.jadpo
test "VAT and sharing lose nothing" {
    var price: CakePrice = CakePrice(money.from_minor(1000, Currency.gbp))
    assert with_vat(price) == money.from_minor(1200, Currency.gbp)
    assert shares(price) == [
        Payment(money.from_minor(334, Currency.gbp)),
        Payment(money.from_minor(333, Currency.gbp)),
        Payment(money.from_minor(333, Currency.gbp))
    ]
}
```

`price + 20%` could mean two things, so it is refused. Write `price * 120%`
instead:

```jadpo check project=std-money-percent expect=type.arithmetic_operand
type CakePrice = Money {
    currency: Currency.gbp
}

/// Twenty percent added to a price: ambiguous, so refused.
function more(price: CakePrice) -> CakePrice {
    return CakePrice(price + 20%)
}
```

```text output
error: `CakePrice` and a percentage can't be added: it could mean two things
 --> main.jadpo:7:22
  |
7 |     return CakePrice(price + 20%)
  |                      ^^^^^^^^^^^ an amount and a percentage
  reason: `price + 20%` could mean the amount raised by 20% of itself, or 20% of some other amount; the standard operator table refuses it rather than guess. A percentage scales money by `*`.
  next: Write `price * 120%`, or `price + price * 20%`, whichever is meant (agent)
  rule: type.arithmetic_operand
  help: https://jadpo.dev/docs/errors/type.arithmetic_operand

check failed: 1 error, 1 file
```

Adding two amounts whose types are in different currencies is a compile
error:

```jadpo check project=std-money-currency expect=type.arithmetic_operand
type CakePrice = Money {
    currency: Currency.gbp
}

type EuroPrice = Money {
    currency: Currency.eur
}

/// A cake in pounds and a delivery charged in euros.
function delivered(price: CakePrice, delivery: EuroPrice) -> CakePrice {
    return CakePrice(price + delivery)
}
```

```text output
error: `CakePrice` and `EuroPrice` can't be added: they're in different currencies
 --> main.jadpo:11:22
   |
11 |     return CakePrice(price + delivery)
   |                      ^^^^^^^^^^^^^^^^ GBP and EUR
  reason: `CakePrice` is in GBP and `EuroPrice` in EUR, as their types or the calls that made them fix; amounts are added only in one currency.
  next: Use two amounts in the same currency (agent)
  rule: type.arithmetic_operand
  help: https://jadpo.dev/docs/errors/type.arithmetic_operand

check failed: 1 error, 1 file
```

Dates, times and temperatures work the same way. A `CalendarDate` plus a
one-month `CalendarPeriod` stops at the last day of the month when the day
doesn't exist there. `Instant - Instant` gives a `Duration`. Subtracting one
temperature from another gives a `TemperatureDifference`:

```jadpo check project=std-time
type DeliveryDay = CalendarDate {}

type SeenAt = Instant {}

type Gap = Duration {}

/// A day a month on: the 31st of January gives the last day of February.
function month_on(day: DeliveryDay) -> DeliveryDay {
    return DeliveryDay(
        day + CalendarPeriod {
            years: 0
            months: 1
            days: 0
        }
    )
}

/// How long between two moments.
function between(start: SeenAt, end: SeenAt) -> Gap {
    return Gap(end - start)
}
```

```jadpo check project=std-time file=main.test.jadpo
test "calendar and clock arithmetic" {
    assert month_on(DeliveryDay("2026-01-31")) == DeliveryDay("2026-02-28")
    assert between(SeenAt("2026-10-25T09:00:00Z"), SeenAt("2026-10-25T11:30:00Z")) == Gap(150m)
    assert 25degC - 20degC == units.of(5, TemperatureDifferenceUnit.kelvin)
}
```

### Other built-in types

These types belong to features of the language. Each row links the page
that covers the feature.

| Kind | Types | Notes |
|---|---|---|
| Containers | `List<T>`, `Set<T>`, `Map<K, V>` | Described below. |
| Translation | `PluralCount`, `TextDirection`, `TranslationReview` | `PluralCount` is a whole number that picks a message's plural form, used directly as a `Message` field; `TextDirection` is a locale's direction, `.left_to_right` or `.right_to_left`, which `locale.direction(value)` gives; `TranslationReview` is `.none` or `.required`, the `review:` of `translations:` in `application { }` ([What the compiler checks of translations](10-configuration-and-time.md#what-the-compiler-checks-of-translations)). |
| Files | `File`, `FileUpload`, `UploadSlot`, `FileContents` | A stored file (its key, name, size and type) as an entity field; the token of a finished upload in an `Input`; the signed address, or addresses for each part, an upload goes to; and what `store.read` returns, a file's bytes for passing on (to a provider, a PDF service, an email), which the format helpers do not accept ([Files](16-built-in-services.md#files)). |
| Batches | `BatchRun`, `BatchProgress`, `RowProblem`, `ImportSummary`, `ExportSummary`, `CsvFormat` | A batch run, as the batch helpers return it and as an entity field can store it; its progress from `batches.status`; one bad row or failed item, without its value; what `finished:` receives after reading (or `batches.each`) and after writing, the latter holding the stored `File`; and who a CSV is written for, `CsvFormat.spreadsheet` or `.standard` ([Batches](12-jobs-and-events.md#batches-work-over-many-items)). |
| Services | `SearchText`, `OAuthGrant`, `AnyObject` | What people type into a search box ([Search](16-built-in-services.md#search)); a connection's credential held for each owner record, `OAuthGrant { owner: ... }`, written only in a connection's `credential:` and never readable by code ([Acting for each user: OAuth grants](11-integrations.md#acting-for-each-user-oauth-grants)); and, only as a package operation's input field, any of the app's object types, sent in its JSON form (Postmark's template variables, Stripe's metadata). |
| Bases | `Object`, `Enum`, `Input`, `Output` | Used only after `=` in a `type` declaration. Other features add bases the same way: `Failure` ([Failures](04-failures.md)), `Event` ([Declaring an event](12-jobs-and-events.md#declaring-an-event)), `EmailMessage` ([Email](16-built-in-services.md#email)), `Document`, a template rendered to HTML or PDF ([Documents](16-built-in-services.md#documents)), and `Message`, a translated message whose fields are its placeholders ([Translations as messages](10-configuration-and-time.md#translations-as-messages)). |
| Decoding | `DecodeProblem` | `field: Text`, a field's path with list positions as numbers, and `rule: Text`; never the value. It is what a failure mapped from `json.read`'s `mismatch` receives in its `problem` field, and only the runtime builds one. |
| Nothing | `Unit` | What an action returns when it returns nothing. |

The batch types' fields, as [Batches](12-jobs-and-events.md#batches-work-over-many-items)
uses them:

| Type | Fields |
|---|---|
| `BatchRun` | `id: Uuid`; storable in an entity field, so an app can keep the run on its own import record |
| `BatchProgress` | `items_done: Int`, `estimated_total: Int?` (from the file's size or the query's count), `problems: Int` (so far), `state: BatchState` (`.running`, `.finished`, `.failed`, `.cancelled`) |
| `RowProblem` | `row: Int` (as a spreadsheet numbers it), `column: Text`, `rule: Text` (the rule's code, as a refused request's problems give it, [HTTP](07-http.md)); never the value |
| `ImportSummary` | `items: Int`, `items_with_problems: Int`, `problems: List<RowProblem>` |
| `ExportSummary` | The same three fields, and `file: File`, the file written |

The literals, values written straight into code, are `true` and `false`,
whole numbers (`25`, `-3`), decimals (`1.25`), strings (`"Hello"`, with
`\"`, `\\`, `\n`, `\r` and `\t` escapes) and `none`.

## Named types
<!-- topic: named-types -->

Most of a program's types are named types: a built-in representation, a name
that says what the value means, and the rules a valid value must follow.

```jadpo check project=shop
type Stock = Int {
    min: 0
}

type Quantity = Int {
    min: 1
    max: 1000
}

type ProductName = Text {
    min_length: 1
    max_length: 120
}

type OrderId = Uuid {}
```

`OrderId` has no rules, but it is still its own type: an `OrderId` can't be
passed where another UUID-based type is expected. Every `type` declaration
creates a new type. There are no aliases, names that are just another way of
writing an existing type.

A named type can build on another named type and add rules. The new type
keeps every rule of the one it builds on, so it can be used wherever that
type is expected. The other way round needs a construction, which checks the
extra rules.

```jadpo check project=refine
type ProductName = Text {
    min_length: 1
    max_length: 120
}

type ShortProductName = ProductName {
    max_length: 40
}

function catalogue_name(name: ShortProductName) -> ProductName {
    return name
}
```

### Constraints

| Constraint | For | Value | Meaning |
|---|---|---|---|
| `min`, `max` | `Int`, `Decimal` | number | Inclusive bounds |
| `min_length`, `max_length` | `Text`, `Bytes`, `List<T>`, `Set<T>`, `Map<K, V>` | whole number | Inclusive bounds; text counts Unicode characters, collections count items |
| `pattern` | `Text` | string | A regular expression the text must match |
| `format` | `Text` | `email` | The text must be an email address |
| `dimensions` | a type built on `Vector` | whole number, 1 to 16,000 | Exactly that many items ([AI types](19-standard-library.md#ai-types)) |
| `distance` | a type built on `Vector` | a `VectorDistance`: `cosine` (the default), `euclidean` or `inner_product` | How its vectors are compared: `ai.distance` and its index follow it ([Storing a vector](21-ai.md#storing-a-vector)) |

Each constraint is written `name: value`, one per line. There are no other
constraints and no custom validation functions.

A refinement, a type that builds on another, can only add to the rules it
inherits. These are errors:

- `type.constraint_contradiction`: a `max` below an inherited `min`, or the
  other way round, and the same for lengths and durations. It is also a
  `Decimal` refinement whose `scale` differs from the one it inherits, since
  the scale decides how the value is stored and sent; a currency other than
  the one it inherits, in a `Money` type's `currency:` or a rate type's
  `from:` and `to:`; and a vector type's `dimensions:` other than the one it
  inherits or outside its lengths.
- `type.constraint_loosened`: a bound looser than the one it inherits, such
  as `max_length: 50` on a type with `max_length: 40`.

The same rules hold for the settings on the fields of every type with
fields, entities and events included. The settings that only an entity's or
an event's fields take (`generated`, `on_delete`, `unique`, `immutable`,
`role`, `personal`, `erase`, `retention`) are described with those
features, starting at [Data](05-data.md).

An exact number holds at most 38 digits, and a number literal with more is
`type.invalid_literal`.

## Creating values
<!-- topic: creating-values -->

Applying a type to a value, `Type(value)`, creates a value of that type by
checking every rule on it and on the types it builds on. It is validation,
never a cast.

```jadpo check project=literals
type Quantity = Int {
    min: 1
    max: 1000
}

type ItemPrice = Decimal {
    scale: 2
    min: 0.00
}

function starting_quantity() -> Quantity {
    return 1
}

function free() -> ItemPrice {
    return 0.00
}
```

When the value is a literal, the compiler checks it while compiling:
`Quantity(0)` is a compile error,
[`type.invalid_literal`](../errors/type.invalid_literal.md). When the value is
computed, the check runs when the program runs.

A constructor accepts a value with the same representation as the type:
`Int` or any `Int`-based type for `Quantity`, and a decimal literal such as
`0.0`, not `0`, for a `Decimal`-based type. This is also how you convert
between two named types on purpose: the value is checked again against the
target's rules.

A value that may be `none` builds nothing. `Quantity(maybe)` with
`maybe: Count?` is `type.constructor_input`: handle the absent case first,
with `match` ([Absent values](#absent-values)).

```jadpo check project=convert
type Stock = Int {
    min: 0
}

type Quantity = Int {
    min: 1
    max: 1000
}

function stock_needed(per_pack: Stock, packs: Quantity) -> Stock {
    return Stock(per_pack * packs)
}
```

Arithmetic works on the representation, so `per_pack * packs` is a plain `Int`
and has to be turned back into `Stock`. A plain `Int` can never be returned
where `Stock` is expected.

Here `Stock(per_pack * packs)` can't fail. `per_pack` and `packs` are never
below zero and `Stock` has no maximum, so the ranges their types declare
prove the product always follows `Stock`'s rules. These declared ranges are
one of the two kinds of proof the compiler reads; the other is the
conditions around a construction ([Conditions as proof](#conditions-as-proof)).

### Construction failures by rule

Building a value from a computed one can fail when nothing proves it
follows the target's rules: neither the declared ranges nor the conditions
around it. Then the statement starts with `attempt`, and a block after the
call maps each rule that can break to the failure the caller gets, one arm
per line. Writes and calls to outside services use the same block
([Failures](04-failures.md)).

```jadpo check project=by-rule file=types.jadpo hidden
type Count = Int {}

type CakeCount = Int {
    min: 1
    max: 12
}

type OrderEmpty = Failure {
    kind: FailureKind.invalid_value
}

type OrderTooLarge = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=by-rule
function doubled(count: CakeCount) fails OrderTooLarge -> CakeCount {
    var result: CakeCount = attempt CakeCount(count * 2) {  // at least 2, so only `max` can break
        max: OrderTooLarge
    }
    return result
}

function from_count(n: Count) fails OrderEmpty, OrderTooLarge -> CakeCount {
    var result: CakeCount = attempt CakeCount(n) {  // anything, so both can break
        max: OrderTooLarge
        invalid: OrderEmpty
    }
    return result
}
```

- **The keys are rules.** Each key names a rule of the type, or of a type it
  builds on, that neither the declared ranges nor a condition proves holds: `min`, `max`,
  `min_length`, `max_length`, `pattern`, `format`, `version`, a `Money`
  type's `currency`, or, for an object literal, a rule the object's type
  names ([Rules across fields](#rules-across-fields)). Each such rule needs
  exactly one arm.
- **`invalid:` is the catch-all.** It covers every rule that can break and
  has no arm of its own, so `{ invalid: OrderTooLarge }` alone covers them
  all. It covers only a construction's rules. A write's conditions
  (`missing:` and `conflict:`, [Reads and writes](06-reads-and-writes.md))
  and a call to an outside service's ([Calling](11-integrations.md#calling))
  each still need their own arm, and `invalid:` on them is an error.
- **Every arm can run.** An arm for a rule that can't break, an `invalid:`
  left covering nothing, a key that isn't one of the type's rules and a
  key given twice are errors ([Failures](04-failures.md)).
- **What the compiler can't reason about can always break.** A `pattern`
  other than the one the value already has always counts as able to break.
  `scale` never does: it rounds the value as it is kept.
- **A built-in type's own check is `format`.** A type built on `Email`,
  `Url` or `IpAddress` checks its text. When the value isn't already built
  on the same type, that check is the `format` rule and can break. An
  `Email` already is an address, and so is text whose own type has
  `format: TextFormat.email`, which is the same check.
- **Every failure is `invalid_value`.** At runtime the failure records the
  rule that broke, such as `max: 12`, or the built-in type whose check
  refused the text, such as `Email`, for the log, never the value.

```jadpo check project=by-rule-format
type ContactText = Text {
    max_length: 200
}

type WorkEmail = Email {
    pattern: ".*@acme[.]com"
}

type NotAnAddress = Failure {
    kind: FailureKind.invalid_value
}

type NotWork = Failure {
    kind: FailureKind.invalid_value
}

function work_email(contact: ContactText) fails NotAnAddress, NotWork -> WorkEmail {
    var email: WorkEmail = attempt WorkEmail(contact) {  // any text, so the address check can break
        format: NotAnAddress
        pattern: NotWork
    }
    return email
}

function work_address(address: Email) fails NotWork -> WorkEmail {
    var email: WorkEmail = attempt WorkEmail(address) {  // already an address: only the pattern
        pattern: NotWork
    }
    return email
}
```

### Conditions as proof

An `if` condition counts as proof. After a check that leaves the function
when the value is out of range, the construction after it is plain: it takes
no `attempt`.

```jadpo check project=conditions file=types.jadpo hidden
type Count = Int {}

type CakeCount = Int {
    min: 1
    max: 12
}

type OrderEmpty = Failure {
    kind: FailureKind.invalid_value
}

type OrderTooLarge = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=conditions
function doubled_checked(count: CakeCount) fails OrderTooLarge -> CakeCount {
    if (count * 2 > 12) {
        reject OrderTooLarge
    }
    return CakeCount(count * 2)  // at most 12 here, and at least 2
}

function from_count(n: Count) fails OrderEmpty, OrderTooLarge -> CakeCount {
    if (n < 1) {
        reject OrderEmpty
    } else if (n > 12) {
        reject OrderTooLarge
    }
    return CakeCount(n)  // neither condition held, so 1 to 12
}

function inside_range(n: Count) fails OrderTooLarge -> CakeCount {
    if (n >= 1 and n <= 12) {
        return CakeCount(n)  // both hold here
    }
    reject OrderTooLarge
}
```

What counts as proof, exactly:

- **A comparison bounds a number.** `<`, `<=`, `>`, `>=`, `==` and `!=`
  between two numbers bound each side by the other side's range: a
  literal's value, or the range another value has at that point. A side is
  bounded when it is made of locals (parameters, `var` and `var mut`
  variables, loop items, names a `match` arm binds), number literals, their
  fields (`line.quantity`), standard-library calls on them
  (`list.count(items)`) and arithmetic. A bound on `list.count`
  of a list bounds that list's `min_length` and `max_length` too.
- **The same number is written the same way.** A bound on `count * 2`
  proves `CakeCount(count * 2)`, brackets aside, and a bound on `count`
  carries through arithmetic: after `count > 6` leaves, `count * 2` is at
  most 12. A bound on `2 * count` says nothing about `count * 2`, and one
  on `count * 2` says nothing about `count` alone.
- **Whole numbers step.** Between two `Int`-based values, `n > 12` leaves
  `n` at least 13 where it holds and at most 12 where it doesn't, and after
  `n == 0` leaves, a range starting at 0 starts at 1. A `Decimal` can come
  as close to a value as it likes, so `!=` bounds no decimal.
- **`and`, `or` and `not` combine.** Inside `if (a and b)` both hold, and
  after `if (a or b) { reject ... }` neither does; `not` turns a condition
  round. Where only one of two may hold, as inside `if (a or b)`, a bound
  survives only when both give one, widened to cover both. The right side
  of `and` is read knowing its left side holds, and the right side of `or`
  knowing its left side doesn't, so `n >= 1 and is_small(CakeCount(n))` is
  plain too.
- **Where it holds.** A condition holds inside the branch it guards. An
  `else if` and the `else` know every earlier condition is false. After the
  `if`, what is known is what holds at the end of every branch that can
  reach the next line. A branch ending in `return` or `reject`, or in a
  `match` whose every arm leaves, never reaches it. With no `else`, the way
  past the chain knows every condition is false.
- **What ends a bound.** Assigning a `var mut` ends every bound that
  mentions it. A `for` loop ends the bounds of every `var mut` it assigns,
  before it and after it, since a later pass sees what an earlier one
  assigned. What a block finds out ends with the block, so a check inside a
  `match` arm, a loop or a branch that goes on proves nothing after it.
- **Nothing else is proof.** A variable's starting value is not:
  `var doubled: Int = count * 2` is an `Int` with no range, until a
  condition bounds `doubled` itself. Nor is an assignment, a `match`
  pattern, an `assert`, or a call to your own function.
- **It proves every construction.** `Type(value)`, `Owner.field(value)`
  and an object literal's field (`type.field_unproved`) all read it. A
  divisor still needs a type that excludes zero ([Logic](03-logic.md)),
  which a condition can prove by building that type first.
- **A fixed `scale` rounds first.** A type with a fixed `scale` keeps a
  value rounded half to even (a half goes to the even digit, so 0.125 is
  kept as 0.12), and its bounds are compared after rounding: a
  value of at most 10.004 fits `max: 10.00` with `scale: 2`, and one of at
  most 10.07 doesn't fit `max: 10.07` with `scale: 1`, since 10.07 is kept
  as 10.1. A constant's construction rounds the same way, as the runtime
  builds it: `constant rate: Rate = Rate(0.10 * 1.25)` with `scale: 2` is
  0.12, not an error. A literal written with more places than its type
  keeps is still refused where it is written, and a computed constant kept
  past 38 digits is `type.invalid_literal`, as arithmetic past them is.

When you change a range or a condition, constructions elsewhere can start
or stop being able to fail, and the compiler reports each one where it is
written:

- A construction that can now fail needs `attempt` and an arm for each rule
  that can break (`failure.attempt_required`, `failure.rule_unmapped`).
- One that no longer can takes no `attempt` and no arm for that rule
  (`failure.attempt_infallible`, `failure.rule_cannot_break`,
  `failure.invalid_covers_nothing`).

When a condition is what proves it, the error points at the comparison:

```jadpo check project=e-condition-proves expect=failure.attempt_infallible
type CakeCount = Int {
    min: 1
    max: 12
}

type OrderTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled_checked(count: CakeCount) fails OrderTooLarge -> CakeCount {
    if (count * 2 > 12) {
        reject OrderTooLarge
    }
    var doubled: CakeCount = attempt CakeCount(count * 2) {
        max: OrderTooLarge
    }
    return doubled
}
```

```text output
error: `CakeCount(...)` can't fail, so it takes no `attempt`
 --> main.jadpo:14:30
   |
11 |     if (count * 2 > 12) {
   |         -------------- checked here
14 |     var doubled: CakeCount = attempt CakeCount(count * 2) {
   |                              ^^^^^^^ nothing to pass on
  reason: The declared ranges and the condition `count * 2 > 12` prove `CakeCount(...)` always follows its type's rules, so building it never fails; `attempt` marks only what can.
  next: Remove `attempt` (agent)
  rule: failure.attempt_infallible
  help: https://jadpo.dev/docs/errors/failure.attempt_infallible

check failed: 1 error, 1 file
```

```jadpo check project=e-condition-proves-rule expect=failure.rule_cannot_break
type Count = Int {}

type CakeCount = Int {
    min: 1
    max: 12
}

type OrderEmpty = Failure {
    kind: FailureKind.invalid_value
}

type OrderTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function from_count(n: Count) fails OrderEmpty, OrderTooLarge -> CakeCount {
    if (n > 12) {
        reject OrderTooLarge
    }
    var count: CakeCount = attempt CakeCount(n) {
        min: OrderEmpty
        max: OrderTooLarge
    }
    return count
}
```

```text output
error: `CakeCount(...)` can't break `max`, so it takes no arm
 --> main.jadpo:22:9
   |
17 |     if (n > 12) {
   |         ------ checked here
22 |         max: OrderTooLarge
   |         ^^^ never runs
  reason: The declared ranges and the condition `n > 12` prove the value always follows `max: 12`, so this arm could never run.
  next: Remove `max: ...` (agent)
  rule: failure.rule_cannot_break
  help: https://jadpo.dev/docs/errors/failure.rule_cannot_break

check failed: 1 error, 1 file
```

## Signatures use named types
<!-- topic: signatures-use-named-types -->

A function, action or query can't take or return a built-in representation
type directly. `Text`, `Int`, `Bool`, `Uuid`, the time types, and the forms
of them that may be `none`, are each wrapped in a named type that says what
the value means.

```jadpo check project=signature
type CustomerNote = Text {
    max_length: 500
}

type NoteWasEdited = Bool {}

function edited(before: CustomerNote, after: CustomerNote) -> NoteWasEdited {
    return NoteWasEdited(before != after)
}
```

`Email`, `Url`, `IpAddress`, objects, enums, field types and `Unit` are
allowed. Inside a body, local variables can still hold built-in types.

## One form for every type
<!-- topic: one-form-for-every-type -->

Every type is declared the same way, `type Name = Base { ... }`. The base
says what the type is built from, and the block holds the details: rules for
a number or text, fields for an object, variants for an enum. An `entity` is
declared on its own, because it is more than a type: it is stored, and it has
actions, queries and policy ([Data](05-data.md)).

```jadpo check project=one-form file=names.jadpo hidden
type ReceiptId = Uuid {}

type DeclineReason = Text {
    max_length: 200
}

type OrderNote = Text {
    max_length: 500
}

type OrderLineRequest = Input {
    sku: StockCode
}

entity Order {
    id: Uuid
    total: ItemPrice
}
```

```jadpo check project=one-form file=main.jadpo
type ItemPrice = Decimal {  // rules on a number
    scale: 2
    min: 0.00
}

type StockCode = Text {  // rules on text
    pattern: "[A-Z]{3}-[0-9]{4}"
}

type OrderId = Uuid {}  // a new identity type with no extra rules

type OrderLine = Object {  // fields
    sku: StockCode
    price: ItemPrice
}

type OrderStatus = Enum {  // variants
    pending
    paid
    shipped
}

type PaymentOutcome = Enum {  // variants that carry fields
    pending
    paid: {
        receipt_id: ReceiptId
    }
    declined: {
        reason: DeclineReason
    }
}

type CreateOrder = Input {  // an object arriving from outside
    lines: List<OrderLineRequest>
    note: OrderNote {
        optional: true
    }
}

type OrderView = Output {  // an object sent back
    id: Order.id
    total: ItemPrice
}
```

- `Object`, `Enum`, `Input` and `Output` are only ever used as a base here.
  There is no anonymous object written inline; every shape has a name.
- `Input` objects may have `optional` and `default` fields and are the only
  objects a route accepts. `Output` objects are the only objects a route
  returns.
- There are no `value`, `enum`, `input` or `output` keywords, and the
  `types/` folder holds only `type` declarations.

## Objects
<!-- topic: objects -->

An object is a fixed set of named fields. The base after `=` says what the
object is for:

```jadpo check project=objects file=names.jadpo hidden
type ProductName = Text {
    min_length: 1
    max_length: 120
}

type Stock = Int {
    min: 0
}
```

```jadpo check project=objects file=main.jadpo
type StockTag = Object {  // used inside the application
    amount: Stock
}

type CreateProduct = Input {  // arrives from outside, such as a request body
    name: ProductName
    stock: Stock
}

type ProductView = Output {  // sent back, such as a response
    name: ProductName
    stock: StockTag
}
```

Only an `Input` may leave fields out or give them defaults. A route accepts
only an `Input` and returns only an `Output`. There is no unnamed object: a
nested shape is declared once, with a name, and used by that name.

An object value is written with its type name and every field:

```jadpo check project=objects file=view.jadpo
function view(name: ProductName, stock: Stock) -> ProductView {
    return ProductView {
        name: name
        stock: StockTag {
            amount: stock
        }
    }
}
```

Every declared field must be given, and unknown fields are errors. Fields can
hold other objects and lists, nested as deeply as needed.

An object literal never fails on a field. A field accepts its declared type or anything
that widens to it, and when the field adds rules of its own, the declared
ranges must prove the value follows them (`type.field_unproved`). A value
that might break them is built first, on its own line, as the field's type
(below), where its failure is mapped. Only a rule the type writes across
its fields can make a literal fail ("Rules across fields", below):

```jadpo check project=field-first file=profiles.jadpo hidden
type Profile = Object {
    display_name: Text {
        max_length: 80
    }
}

type BadgeNameTooLong = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=field-first
type Badge = Object {
    name: Text {
        max_length: 12
    }
}

function make_badge(profile: Profile) fails BadgeNameTooLong -> Badge {
    var name: Badge.name = attempt Badge.name(profile.display_name) {
        max_length: BadgeNameTooLong
    }
    return Badge {
        name: name
    }
}
```

### Field types

Every field of an object, entity or enum variant is a type of its own, written
`Owner.field`. `Customer.email` is a refinement of `Email`: it can be used
wherever an `Email` is expected, but an `Email` is not automatically a
`Customer.email`, and `Supplier.email` is never one.

```jadpo check project=fields
entity Customer {
    id: Uuid
    email: Email
}

entity Supplier {
    id: Uuid
    email: Email
}

function contact_address(email: Email) -> Email {
    return email
}

function customer_contact(customer: Customer) -> Email {
    return contact_address(customer.email)
}

function supplier_contact(supplier: Supplier) -> Email {
    return contact_address(supplier.email)
}
```

Field types are how identifiers stay apart: `Customer.id` and `Supplier.id`
are both UUIDs, and passing one where the other is expected is
`type.sibling_mismatch`. To move a value into a field type on purpose, use the
field as a constructor, as in `Customer.email(email)`.

A field can add rules of its own in a block after its type:

```jadpo check project=badge file=names.jadpo hidden
type ProductName = Text {
    min_length: 1
    max_length: 120
}
```

```jadpo check project=badge file=main.jadpo
type Badge = Object {
    label: ProductName {
        max_length: 12
    }
}
```

Reading through a nested object gives the inner object's field type:
`customer.billing_address.postal_code` has type `Address.postal_code`, whether
it came from the billing or the shipping address.

A field's type is always a type: a type's name, a field type such as
`Customer.email`, a package's type, `List<T>`, `T?`, or a type with rules in
a block. A value written there, whether a call (`TicketStatus("open")`), a
literal or an enum's variant (`TicketStatus.open`), is `type.not_a_type`,
naming the field, in every block of fields: an object's, an input's, an
event's, a failure's, a variant's, an entity's and `config { }`'s settings.

### Rules across fields

A field's rules say what one value may be. A rule across fields says how
the values of one object relate: a booking ends after it starts, a
discount is below the price. An `Object`, `Input` or `Output` names each
such rule in its block, and the rule is checked whenever a value of the
type is made.

```jadpo check project=across-fields file=types.jadpo hidden
type Moment = Instant {}

type BookingBackwards = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=across-fields
type Booking = Object {
    starts: Moment
    ends: Moment
    rule ends_after_start: ends > starts
}

function book(starts: Moment, ends: Moment) fails BookingBackwards -> Booking {
    var booking: Booking = attempt Booking {  // nothing proves the rule
        starts: starts
        ends: ends
    } {
        ends_after_start: BookingBackwards
    }
    return booking
}

function book_checked(starts: Moment, ends: Moment) fails BookingBackwards -> Booking {
    if (ends <= starts) {
        reject BookingBackwards
    }
    return Booking {  // the condition proves `ends_after_start`
        starts: starts
        ends: ends
    }
}

test "a booking that ends before it starts is refused" {
    assert book("2026-03-02T10:00:00Z", "2026-03-02T09:00:00Z") fails BookingBackwards
    assert book_checked("2026-03-02T10:00:00Z", "2026-03-02T09:00:00Z") fails BookingBackwards
}
```

- **Written in the type's block.** `rule name: condition`, one per line,
  usually after the fields. A rule's name is lower_snake_case and its own:
  no field or other rule of the type has it, and it is never `invalid`,
  which an `attempt` keeps for every rule (`type.rule_name_taken`). `rule`
  isn't reserved: a field may still be called `rule`.
- **A condition over the type's own fields.** It names the fields bare
  (`ends`) and reads through them (`stay.ends`); it uses literals,
  variants, operators, `match` and the standard operations that always
  give a result from their arguments alone. Nothing else: no function or
  action of the application, constant, setting, clock, random value, key or
  record (`type.rule_outside_fields`). Not even a constant: a rule means
  the same wherever its type is read. It is a `Bool`
  (`type.rule_not_condition`). A field that may be `none` is read with
  `match`, as anywhere else.
- **Checked whenever a value is made,** once every field has its value,
  whether the object stands alone, as a list's item or as a map's entry. A
  value is made by an object literal, a construction, `json.read`, a
  request's body or query string, the output a route test reads,
  `text.parse` of a standard type's text, and a read from the store. Jadpo
  never stores a value that breaks a rule, so one read back from the store
  is a defect ([Data](05-data.md)).
- **One rule is reported.** The field rules run first, then each rule in the
  order written, and the first one broken is the one reported, so a report
  never depends on more than one rule.
- **A setting never holds an object,** so never one with rules
  (`configuration.type_unsupported`).
- **A query's result type has none** (`query.result_rule`,
  [Reads and writes](06-reads-and-writes.md)). A read has no `attempt` to
  map a broken rule, so the type is refused rather than each row checked as
  it is read.
- **An object literal can break one.** Building a type with rules is a
  construction, mapped as `Type(value)` is
  ([Construction failures by rule](#construction-failures-by-rule)). Each
  rule that neither the declared ranges nor a condition proves needs an arm
  keyed by its name, or `invalid:` covers it.
- **A literal is checked while compiling, or refused.** One whose values
  are all literals is worked out while compiling, so a booking written
  backwards is `type.invalid_literal`. A literal is never kept unchecked:
  text where a standard object type with rules across its parts goes is
  `type.invalid_literal`, since the compiler can't read its text, and any
  other literal a standard object can't be is `type.mismatch`
  ([Rules across a type's parts](19-standard-library.md#rules-across-a-types-parts)).
- **Proof reads the values given.** A rule is read with each field
  replaced by the value given for it, so `ends > starts`, built with
  `starts: begin` and `ends: finish`, is proved after `if (finish <= begin)
  { reject ... }` leaves, or inside `if (finish > begin)`. A comparison
  proves itself between any two values written the same way, instants and
  dates as well as numbers ([Conditions as proof](#conditions-as-proof)); numbers are proved by
  their declared ranges too, and `and`, `or` and `not` combine as they do
  in a condition. A rule that calls a standard operation or uses `match`
  is never proved this way, so a literal that may break it is always
  attempted.
- **Every failure is `invalid_value`.** The report names the rule
  (`ends_after_start`), never the values. JSON that breaks one, in a
  request or `json.read`, is a problem at the object's path, empty for the
  whole input, whose rule is the rule's name
  ([Data formats](19-standard-library.md#data-formats-json-csv-tsv-and-json-lines)).
- **OpenAPI says them in words.** OpenAPI can't express most rules, so each
  object's schema description lists its rules as written ([HTTP](07-http.md)).
- **A rule belongs to the type that writes it.** `include` and `pick` copy
  fields, never rules: a type built from `Booking` writes the rule again if
  it holds there too. Only an `Object`, `Input` or `Output` has rules
  (`type.rule_outside_object`); events, failures, messages, enums and
  refinements have none. A rule on an entity, which the database would
  check and a write would map, is not designed yet.
- **The standard object types have rules too:** an `InstantRange`'s
  `start_not_after_end`, a `PostalAddress`'s `postal_code_of_country`. Each
  is named, proved, attempted and reported as a rule written here is.
  [Rules across a type's parts](19-standard-library.md#rules-across-a-types-parts)
  lists them.

In a rule, numbers are proved by their declared ranges as a field's rules
are, and a field that may be `none` is read with `match`:

```jadpo check project=across-fields-input file=types.jadpo hidden
type StayDate = CalendarDate {}

type GuestCount = Int {
    min: 1
    max: 12
}

type ChildCount = Int {
    min: 0
    max: 6
}
```

```jadpo check project=across-fields-input
type StaySearch = Input {
    arrives: StayDate
    leaves: StayDate
    adults: GuestCount
    children: ChildCount {
        optional: true
    }
    rule leaves_after_arrival: leaves > arrives
    rule children_with_adults: match children {
        some(count) -> count <= adults * 2
        none -> true
    }
}

type Party = Object {
    adults: GuestCount
    children: ChildCount
    rule not_crowded: children <= adults * 6
}

function party(adults: GuestCount, children: ChildCount) -> Party {
    return Party {  // at most 6 children, and at least 6 places: always holds
        adults: adults
        children: children
    }
}
```

A construction whose every rule is proved can't fail, so it takes no
`attempt`, and the error points at the condition that proves it, as it
does for any other construction:

```jadpo check project=e-across-fields-proved expect=failure.attempt_infallible
type Moment = Instant {}

type BookingBackwards = Failure {
    kind: FailureKind.invalid_value
}

type Booking = Object {
    starts: Moment
    ends: Moment
    rule ends_after_start: ends > starts
}

function book(starts: Moment, ends: Moment) fails BookingBackwards -> Booking {
    if (ends <= starts) {
        reject BookingBackwards
    }
    var booking: Booking = attempt Booking {
        starts: starts
        ends: ends
    } {
        ends_after_start: BookingBackwards
    }
    return booking
}
```

```text output
error: `Booking { ... }` can't fail, so it takes no `attempt`
 --> main.jadpo:17:28
   |
14 |     if (ends <= starts) {
   |         -------------- checked here
17 |     var booking: Booking = attempt Booking {
   |                            ^^^^^^^ nothing to pass on
  reason: The declared ranges and the condition `ends <= starts` prove `Booking { ... }` always follows its type's rules, so building it never fails; `attempt` marks only what can.
  next: Remove `attempt` (agent)
  rule: failure.attempt_infallible
  help: https://jadpo.dev/docs/errors/failure.attempt_infallible

check failed: 1 error, 1 file
```

## Building a type from other types
<!-- topic: building-a-type-from-other-types -->

Most object types in a real application are slices of an entity or of another
object: a create input, a patch input where every field is optional, a public
view, a list summary, a v2 response that adds a field. Instead of writing
each field out (`title: Todo.title`), a type's body can copy fields from
other types. The body keeps its ordinary `Output { }` form. Four kinds of
line build it. They are **read from top to bottom**, and each changes the
result of the lines above it:

| Line | Does |
|---|---|
| `include: [Todo]` | adds every field of each listed type (types only; never on an `Input`) |
| `pick: [Todo.id, Todo.title]` | adds just the listed fields, each keeping its own name |
| `omit: [Todo.internal_note]` | removes fields present at that point |
| `bar: User.bar` | a written field: replaces a field of the same name above, or adds a new one |

```jadpo check project=building file=names.jadpo hidden
type TodoTitle = Text {
    max_length: 200
}

type TodoStatus = Enum {
    open
    done
}

type DayCount = Int {
    min: 0
}

type Quantity = Int {
    min: 1
}

entity User {
    id: Uuid
    email: Email
}

entity Todo {
    id: Uuid
    title: TodoTitle
    status: TodoStatus
    due_at: Instant?
    deleted_at: Instant?
}

type OrderLineView = Output {
    quantity: Quantity
}

type Order = Object {
    id: Uuid
    lines: List<OrderLineView>
}
```

```jadpo check project=building file=main.jadpo
type TodoView = Output {
    pick: [Todo.id, Todo.title, Todo.status, Todo.due_at]
}

type TodoDetail = Output {
    include: [Todo]
    omit: [Todo.deleted_at]
    owner_email: User.email
}

type PatchTodo = Input {
    pick: [Todo.title, Todo.due_at]
    optional: all
}

type OrderView = Output {
    include: [Order]
    lines: List<OrderLineView>  // replaces Order.lines with a view of each line
}

type TodoReminder = Object {
    pick: [Todo.id, Todo.title, Todo.due_at]
    owner_email: User.email
    days_overdue: DayCount
}
```

Rules:

- **Top to bottom.** The same kind of line may appear more than once, and the
  formatter keeps the order written, because order matters here.
- **Fields keep their declared order.** Copied fields stand where their
  `include` or `pick` line is, an `include`'s in the order its type
  declares them and a `pick`'s in the order listed; a written field that
  replaces a copied one takes its place. Values, hover and the manifest
  (each field's `order`) follow it.
- **Lists are always in brackets**, even with one entry: `include: [Todo]`.
- **`omit` removes only what is there.** Omitting a field that is not present
  at that point is an error, which catches typos and fields since removed.
- **Written fields win.** A field you write replaces any field of the same
  name above it, visibly; otherwise it is added.
- **Copied fields never silently replace copied fields.** An `include` or
  `pick` that would replace a field brought in by an earlier `include` or
  `pick` is an error: resolve it with `omit` or by writing the field. Copied
  fields are not visible in the declaration, so a clash there would change a
  type unseen when a source type gains a field.
- **No `include` on an `Input`.** Taking every field of `User` would accept
  any field added to `User` later, so a client could set a field it was
  never meant to (the mass-assignment bug). Inputs `pick` or write their
  fields. Generated fields such as `id` and `created_at` can never
  appear in an `Input`.
- **`optional`** makes the listed fields, or `all`, optional, only on an
  `Input`.
- **Copied fields follow their source.** Renaming `Todo.title` renames it in
  every type that copies it, and its validation rules come with it.
- **No type includes itself.** An `include` that leads back to its own type,
  directly or through another, is `type.copy_cycle`.
- **Every built type has a name;** there are no inline types in signatures.
  Hover in your editor shows the final fields and where each came from
  (`title ← Todo.title`, `lines ← written, replaces Order.lines`).
- **Outputs may `include`.** A field added to `Todo` later reaches
  `TodoDetail`, and the committed API record shows the addition in review
  ([HTTP](07-http.md)). Fields that policy hides from clients stay hidden
  when the app runs ([Field restrictions](09-policy.md#field-restrictions)).
- `include`, `pick`, `omit` and `optional` are reserved and cannot be field
  names.

## Absent values
<!-- topic: absent-values -->

There is no `null` or `undefined`. A type followed by `?` may hold `none`,
and reading it means handling both cases, usually with
[`match`](03-logic.md).

Writing another language's form is an error with an edit that writes the
Jadpo form: `null`, `nil`, `undefined` and `None` are each
`type.foreign_value`, and `??` and `?.` are `syntax.null_coalescing` and
`syntax.optional_chaining`, whose edits write the `match`.

```jadpo check project=absent file=names.jadpo hidden
type ProductName = Text {
    min_length: 1
    max_length: 120
}
```

```jadpo check project=absent file=main.jadpo
type Listing = Object {
    name: ProductName
    subtitle: ProductName?
}

function headline(listing: Listing) -> ProductName {
    return match listing.subtitle {
        some(subtitle) -> subtitle
        none -> listing.name
    }
}
```

A field that may be `none` must still be given: a `Listing` must say
`subtitle: none` explicitly. Being allowed to leave a field out is a separate
idea, and only an `Input` has it:

```jadpo check project=input-settings file=names.jadpo hidden
type ProductName = Text {
    min_length: 1
    max_length: 120
}
```

```jadpo check project=input-settings file=main.jadpo
type ListProducts = Input {
    name: ProductName {
        optional: true
    }
    page_size: PageSize {
        default: 25
    }
}

type RenameProduct = Input {
    name: ProductName
    subtitle: ProductName? {
        default: none
    }
}
```

- `optional: true` lets the request leave the field out. Reading it gives
  `T?`, and a field left out reads as `none`. Only an update's `patch:` tells
  "left out" apart from "sent as `none`"
  ([Reads and writes](06-reads-and-writes.md)).
- `default: 25` fills in a value when the request leaves the field out,
  checked against the field's rules while compiling.
- `default: none` fills in `none` for a field that may be `none`.

A value that may be `none` widens as the value itself does: a
`Customer.email?` can be used where an `Email?` is expected.

## Enums
<!-- topic: enums -->

An enum is a closed list of named alternatives. Variants are always written
with their enum's name.

```jadpo check project=enum-values
type TicketStatus = Enum {
    open
    waiting
    closed
}

function reopen() -> TicketStatus {
    return TicketStatus.open
}
```

A variant can carry its own fields, which makes an enum a tagged union: each
value is exactly one variant and has only that variant's fields.

```jadpo check project=tagged-values file=names.jadpo hidden
type ReceiptId = Uuid {}

type DeclineReason = Text {
    min_length: 1
    max_length: 200
}

type OutcomeLabel = Text {
    max_length: 20
}
```

```jadpo check project=tagged-values file=main.jadpo
type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
        paid_at: Instant
    }
    declined: {
        reason: DeclineReason
    }
}

function declined_payment(reason: DeclineReason) -> PaymentOutcome {
    return PaymentOutcome.declined {
        reason: reason
    }
}

function label(outcome: PaymentOutcome) -> OutcomeLabel {
    return match outcome {
        PaymentOutcome.pending -> "pending"
        PaymentOutcome.paid -> "paid"
        PaymentOutcome.declined -> "declined"
    }
}
```

The only way to read a variant's fields is to `match` on it. Payload fields
have field types too, written `PaymentOutcome.declined.reason`.

An enum can't be created from text: `TicketStatus("open")` is an error.
Text from outside, such as a request body, becomes a variant only as it is
read and checked on the way in. In JSON a plain variant is its name as a
string (`"open"`), and a variant with fields is an object with a `tag`
naming the variant, plus its fields.

Enums have no methods or display labels. A variant is a name, or a name with
a block of its fields; one given a label, `open: "Open"`, is
`type.enum_label`. Wording for people comes from translations: a domain's
language files can give each value of an enum a wording, which a template
then writes out ([Enums with wording](10-configuration-and-time.md#enums-with-wording)).

## Collections
<!-- topic: collections -->

`List<T>`, `Set<T>` and `Map<K, V>` can be used as field types, parameter
types and boundary types, nested in each other or holding objects.

```jadpo check project=lists file=names.jadpo hidden
type Tag = Text {
    min_length: 1
    max_length: 30
}

type Stock = Int {
    min: 0
}
```

```jadpo check project=lists file=main.jadpo
type BasketLine = Object {
    tag: Tag
    stock: Stock
}

type Basket = Output {
    lines: List<BasketLine>
    tags: List<Tag> {
        max_length: 10
    }
}
```

A collection keeps its element type: a `List<Customer.email>` is not a
`List<Email>`. Nothing converts one collection into another:
`List<Tag>(names)` is `type.constructor_input`, since only a named type is
built from a value. A named collection type is a named type, so it can be
built from a collection of its items ([Named collection construction](#named-collection-construction)).

How you build and walk them:

- **A list literal** is written in square brackets, its items taking the
  list's element type: `["urgent", "home"]`. Written where a set is
  expected, it is a set.
- **`for` walks a list** ([Logic](03-logic.md)). It walks a set too, in the
  set order below, and a map through its keys:
  `for key in map.keys(prices)`.
- **Operations.** `list.count`, `list.contains`, `list.map` and
  `list.filter` take a list. The `set.` and `map.` operations search and
  rebuild sets and maps ([Sets](19-standard-library.md#sets),
  [Maps](19-standard-library.md#maps)).
- **An empty map** is `[]` written where a map is expected, and `map.with`
  adds entries to it.

In JSON, a `Set<T>` is an array and a `Map<K, V>` an object:

```text
{ "tags": ["urgent", "home"], "weights": { "urgent": 3, "home": 1 } }
```

- An array that repeats a value is refused where a set is expected.
- A map in an `Input` or `Output`, however deeply nested, is keyed by a type
  built on `Text` or `Uuid`, because JSON object keys are text
  (`type.map_key_boundary`). A standard named collection such as
  `LocalisedText` is sent as its own entry in the standard library says.

Sets have rules of their own:

- A set literal that repeats a value is `type.invalid_literal`.
- A list written where a named set type is expected
  (`type Tags = Set<TagName> {}`) is a set, so `[b, a]` equals `[a, b]`.
- Equal computed items collapse into one, so the compiler can't count them
  ahead of time. A set literal's `min_length` counts only its distinct
  literals, or one value when it has any items: `[a, b]` into a set type
  with `min_length: 2` is `type.invalid_literal`.

**Set order.** A set has no order your code can use: no index and no first
item. Where order matters, sort it or use a `List`. Everything that shows a
set (JSON, reports, `for`) uses one fixed order, the same in interpreted and
compiled code: by value, numbers numerically, text by Unicode code point,
enum variants in declaration order, and objects and tagged unions field by
field in declared order.

### Named collection construction

A named list, set or map type is built from a collection whose items are
already its item type. It is an ordinary construction, `Type(value)`, that
checks only the collection's own rules: the `min_length` and `max_length` of
the type and of the types it builds on. When one of them can break, the
construction takes `attempt` and an arm for each, or `invalid:` for all of
them. The declared lengths and the conditions around it prove it as they
prove any construction, and a proven one is plain.

```jadpo check project=named-collections file=types.jadpo hidden
type GuestName = Text {
    min_length: 1
    max_length: 40
}

type NoGuests = Failure {
    kind: FailureKind.invalid_value
}

type TooManyGuests = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=named-collections
type Guests = List<GuestName> {
    min_length: 1
    max_length: 12
}

type GuestBook = Set<GuestName> {
    min_length: 1
}

function guest_list(invited: List<GuestName>) fails NoGuests, TooManyGuests -> Guests {
    var guests: Guests = attempt Guests(invited) {  // any list, so both can break
        min_length: NoGuests
        max_length: TooManyGuests
    }
    return guests
}

function seated(invited: List<GuestName>) fails NoGuests, TooManyGuests -> Guests {
    if (list.count(invited) == 0) {
        reject NoGuests
    } else if (list.count(invited) > 12) {
        reject TooManyGuests
    }
    return Guests(invited)  // 1 to 12 names here
}

function signed_in(guests: Guests) -> GuestBook {
    return GuestBook(guests)  // at least one name, each kept once
}
```

- **The items already are the item type.** Building the collection never
  converts its items: a `Text` becomes a `GuestName` where it is built,
  each with its own arms. A collection type never stands in for another,
  so a list of a type built on `GuestName` doesn't fit either.
- **A list from a list, a map from a map.** A map's keys and values must
  both already be its own. A list is never built from a set, since a set
  has no order a list could keep.
- **A set from a set or a list.** A list's repeats collapse, and the set
  keeps the set order ([Collections](#collections)). It holds at most as many items
  as the list, so the list's length proves the set's `max_length`, and at
  least one when the list has any, so the list proves a `min_length` of 1
  and no more: two of its items might be equal.
- **From another named collection.** A narrower named collection widens
  to the one it builds on with no construction at all. One of the same
  item type that doesn't build on it is built like any value, its own
  declared lengths proving what they can.
- **Field types too.** A field type with collection rules is built the
  same way: `Party.guests(invited)`.
- **A list written in the construction** takes the collection's type and
  its length is checked while compiling, as a literal's is:
  `Guests([])` is `type.invalid_literal`.
- **Only a named type.** `List<GuestName>(invited)` stays
  `type.constructor_input`: a collection type that isn't named is never
  built from a value.

Passing a collection whose items aren't the item type yet is
`type.constructor_input`, and its `next` says where to build them:

```jadpo check project=e-named-collection-items expect=type.constructor_input
type GuestName = Text {
    min_length: 1
}

type Guests = List<GuestName> {
    min_length: 1
}

type RawName = Text {}

type NoGuests = Failure {
    kind: FailureKind.invalid_value
}

function guest_list(invited: List<RawName>) fails NoGuests -> Guests {
    var guests: Guests = attempt Guests(invited) {
        min_length: NoGuests
    }
    return guests
}
```

```text output
error: `Guests` can't be built from `List<RawName>`
 --> main.jadpo:16:41
   |
16 |     var guests: Guests = attempt Guests(invited) {
   |                                         ^^^^^^^ this is `List<RawName>`
  reason: `Guests` is a list of `GuestName`, and this is a list of `RawName`. Building a `Guests` checks only the list's own rules; it never converts the items.
  next: Build each item as a `GuestName` where it is made, so the list is a `List<GuestName>` (agent)
  rule: type.constructor_input
  help: https://jadpo.dev/docs/errors/type.constructor_input

check failed: 1 error, 1 file
```

A set where a named list is needed is the same error:

```jadpo check project=e-named-collection-set expect=type.constructor_input
type GuestName = Text {
    min_length: 1
}

type Guests = List<GuestName> {
    min_length: 1
}

type NoGuests = Failure {
    kind: FailureKind.invalid_value
}

function guest_list(invited: Set<GuestName>) fails NoGuests -> Guests {
    var guests: Guests = attempt Guests(invited) {
        min_length: NoGuests
    }
    return guests
}
```

```text output
error: `Guests` can't be built from `Set<GuestName>`
 --> main.jadpo:14:41
   |
14 |     var guests: Guests = attempt Guests(invited) {
   |                                         ^^^^^^^ this is `Set<GuestName>`
  reason: `Guests` is a list of `GuestName`, and this is a set. A set has no order a list could keep, so a list is built only from a list.
  next: Keep the items in a `List<GuestName>` where they are collected (agent)
  rule: type.constructor_input
  help: https://jadpo.dev/docs/errors/type.constructor_input

check failed: 1 error, 1 file
```

## When one type can stand in for another
<!-- topic: when-one-type-can-stand-in-for-another -->

A value can be used where its own type is expected, and where a type it
widens to is expected. Widening means stepping from a type to the more
general one it was declared from. Going the other way always goes through a
constructor. With `Customer` and `Supplier` entities that each have an
`email: Email` field:

| You have | Where `Customer.email` is expected | Where `Email` is expected | Where `Text` is expected |
|---|---|---|---|
| `Customer.email` | yes | yes, it widens | no |
| `Supplier.email` | no | yes, it widens | no |
| `Email` | only through `Customer.email(...)` | yes | no |
| `Text` | only through `Customer.email(...)` | only through `Email(...)` | yes |

- **Widening** follows declared steps only: a field type to its declared type,
  a named type to the named type it builds on.
- **No silent narrowing.** Going from a general type to a specific one always
  goes through a constructor, which re-checks the rules.
- **Same shape is not same type.** Two objects with identical fields are still
  different types.
- **Nothing unwraps.** A named type never becomes its built-in
  representation, except where arithmetic and comparison operators work on
  it.

`Customer.id` and `Supplier.id` stay distinct even when both are version 4
UUIDs. A function that genuinely accepts either takes an enum whose
variants carry each one, which keeps the choice explicit. Your own code has
no generics or unions ([Generic code](03-logic.md#generic-code)).

## Values never change
<!-- topic: values-never-change -->

Every value is immutable. Passing a value to a function or returning it never
lets anyone else change it. A local declared with `var mut` can be pointed at a
new value of the same type, but fields cannot be assigned: to change an object,
build a new one.

```jadpo check project=values file=names.jadpo hidden
type ProductName = Text {
    min_length: 1
    max_length: 120
}

type Stock = Int {
    min: 0
}
```

```jadpo check project=values file=main.jadpo
type Product = Object {
    name: ProductName
    stock: Stock
}

function restock(product: Product, stock: Stock) -> Product {
    return Product {
        name: product.name
        stock: stock
    }
}
```

## What is built
<!-- topic: values-and-types-built -->

| Feature | Status |
|---|---|
| Everything else on this page | Built. |
| A map literal with entries | Not built yet: it is not designed yet. |
| `FileUpload`, `UploadSlot`, `SearchText`, `OAuthGrant`, `AnyObject` and the `Document` base | Not built yet: each comes with the features that use it. |
| `jadpo inspect`, showing a built type's final fields and where each came from | Not built yet. Hover shows them. |
| `BatchRun`, `BatchProgress`, `RowProblem`, `ImportSummary`, `ExportSummary`, `File` and `FileContents` | Partly built: only the runtime builds one. A program reads the batch types' fields, and an entity field keeps a `BatchRun` or a `File` as its JSON; a program can't read a `File`'s or a `FileContents`'s parts yet. |

## Errors

Every error this page's rules produce. Each page has an example that
triggers it and the compiler's actual output.

- [`type.mismatch`](../errors/type.mismatch.md)
- [`type.sibling_mismatch`](../errors/type.sibling_mismatch.md)
- [`type.primitive_signature`](../errors/type.primitive_signature.md)
- [`type.invalid_literal`](../errors/type.invalid_literal.md)
- [`type.constructor_input`](../errors/type.constructor_input.md)
- [`type.constructor_argument_count`](../errors/type.constructor_argument_count.md)
- [`type.missing_field`](../errors/type.missing_field.md)
- [`type.field_unproved`](../errors/type.field_unproved.md)
- [`type.rule_outside_object`](../errors/type.rule_outside_object.md)
- [`type.rule_name_taken`](../errors/type.rule_name_taken.md)
- [`type.rule_not_condition`](../errors/type.rule_not_condition.md)
- [`type.rule_outside_fields`](../errors/type.rule_outside_fields.md)
- [`type.unknown_field`](../errors/type.unknown_field.md)
- [`type.duplicate_field`](../errors/type.duplicate_field.md)
- [`type.not_record`](../errors/type.not_record.md)
- [`type.field_on_non_record`](../errors/type.field_on_non_record.md)
- [`type.nullable_selection`](../errors/type.nullable_selection.md)
- [`type.redundant_nullability`](../errors/type.redundant_nullability.md)
- [`type.unknown_enum_variant`](../errors/type.unknown_enum_variant.md)
- [`type.missing_variant_field`](../errors/type.missing_variant_field.md)
- [`type.unknown_variant_field`](../errors/type.unknown_variant_field.md)
- [`type.duplicate_variant_field`](../errors/type.duplicate_variant_field.md)
- [`type.enum_empty`](../errors/type.enum_empty.md)
- [`type.enum_label`](../errors/type.enum_label.md)
- [`type.duplicate_variant`](../errors/type.duplicate_variant.md)
- [`type.unknown_setting`](../errors/type.unknown_setting.md)
- [`type.setting_value`](../errors/type.setting_value.md)
- [`type.constraint_base`](../errors/type.constraint_base.md)
- [`type.constraint_repeated`](../errors/type.constraint_repeated.md)
- [`type.constraint_contradiction`](../errors/type.constraint_contradiction.md)
- [`type.constraint_loosened`](../errors/type.constraint_loosened.md)
- [`type.invalid_pattern`](../errors/type.invalid_pattern.md)
- [`type.decimal_scale_required`](../errors/type.decimal_scale_required.md)
- [`type.optional_outside_input`](../errors/type.optional_outside_input.md)
- [`type.reserved_field_name`](../errors/type.reserved_field_name.md)
- [`type.base_outside_declaration`](../errors/type.base_outside_declaration.md)
- [`type.not_a_type`](../errors/type.not_a_type.md)
- [`type.type_argument_count`](../errors/type.type_argument_count.md)
- [`type.copy_into_input`](../errors/type.copy_into_input.md)
- [`type.absent_field`](../errors/type.absent_field.md)
- [`type.copy_clash`](../errors/type.copy_clash.md)
- [`type.copy_cycle`](../errors/type.copy_cycle.md)
- [`type.map_key_boundary`](../errors/type.map_key_boundary.md)
- [`syntax.type_parent_required`](../errors/syntax.type_parent_required.md)
- [`syntax.type_block_required`](../errors/syntax.type_block_required.md)
- [`syntax.variable_type_required`](../errors/syntax.variable_type_required.md)

### Habits from other languages

People and agents write constructs from other languages out of habit. Each
has an error of its own that names the habit and the Jadpo form, with an
edit. Parsing carries on as if the Jadpo form had been written, so a line
gives one error. When the Jadpo form only renames or drops words, the rest
of the project is checked too. The list of habits is
`jadpo_syntax::habits`, and
[Habits from other languages](03-logic.md#habits-from-other-languages) on
the Logic page explains how a line shows one error at a time.

- [`syntax.string_quotes`](../errors/syntax.string_quotes.md)
- [`syntax.type_keyword`](../errors/syntax.type_keyword.md)
- [`syntax.collection_type`](../errors/syntax.collection_type.md)
- [`syntax.nullable_type`](../errors/syntax.nullable_type.md)
- [`syntax.null_coalescing`](../errors/syntax.null_coalescing.md)
- [`syntax.optional_chaining`](../errors/syntax.optional_chaining.md)
- [`syntax.new_keyword`](../errors/syntax.new_keyword.md)
- [`type.foreign_type`](../errors/type.foreign_type.md)
- [`type.foreign_value`](../errors/type.foreign_value.md)
