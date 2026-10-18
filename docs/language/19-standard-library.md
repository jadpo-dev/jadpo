---
title: Standard library
area: 19
status: decided
checked-with: this checkout's jadpo on every gate run, from chunk 5.5a; the other examples are fragments until chunks 5.5c to 5.5s build them
rules:
  - semantic.standard_operation_moved
  - type.argument_empty
  - type.argument_negative
  - type.money_split
  - type.reserved_variant_name
  - type.standard_clash
  - syntax.percent_ambiguous
  - type.standard_exists
  - type.temporal_format_part_missing
  - type.unit_required
  - type.unit_unknown
  - type.variant_retired
---

# Standard library

The standard library is the set of types and operations every program can
use without declaring them: `Money`, `list.count(items)`,
`temporal.in_zone(at, zone)`. Areas 2, 3 and 10 each decided the few their
own features needed. This page collects them, adds what real applications
keep reaching for, says which gaps in the language block the rest and how
to close them, and says what is left out on purpose and why.

> Decided 2026-10-17 (Richard), as recommended: "Trusting your recommendations". Chunk 5.4 wrote it; the "Decisions" table under "Review" records each decision. Built through chunks 5.5h2, 5.5k and 5.5k2: the contract every standard type meets, over the 13 standard types already built, with `type.standard_exists` on app code (5.5a); the clash a release's new standard type makes with an app's own, with `jadpo fix` (5.5a2); editor completion (5.5a3); the list operations (5.5b); the set, map and range operations (5.5c); two-parameter inline functions and `Range<T>` (5.5c2); the number operations (5.5d); maths on `Decimal` (5.5e); the text operations, with `locale:` and NFC at the boundary (5.5f); `hash.sha256`, the encodings and `json.read` and `json.write` in memory (5.5g); the `DecodeProblem` a failure mapped from `mismatch` receives (5.5g2); `Money` and `Currency`, the operator table's `Money` rows and the money operations but `convert`, stored in two columns (5.5h); `ExchangeRate`, `money.convert` and `{price, number, currency}` in messages (5.5h2); ten of the time types, with `CalendarPeriod`'s operator rows and `temporal.period_between` (5.5k); and `RecurrenceRule` and `OpeningHours`, with `temporal.occurrences` and `temporal.open_at` (5.5k2). The rest is built by chunks 5.5i to 5.5s, in the order under "Building it"; the "Status" table under "Review" says what each feature waits on. Where this page changes another area, that page carries a dated "Amended 2026-10-17 by area 19" note, or the change is work for the chunk named here. The packages design it builds on is [packages and extension points](research/packages-and-extension-proposals.md) (#88, merged 2026-10-17).

| Namespace | Holds | Operations | Decided in |
|---|---|---|---|
| `list` | Operations on `List<T>` (was `collection`) | 26 (4 decided, 22 new) | [Area 2](02-values-and-types.md#collections), question 4; inline functions in [area 3](03-logic.md#passing-functions) |
| `set` | Operations on `Set<T>` | 7, new | This page |
| `map` | Operations on `Map<K, V>` | 10, new | This page |
| `range` | Operations on `Range<T>` and the ranges built on it | 3, new | This page |
| `integer` | Operations on values built on `Int` | 5 (1 decided, 4 new) | [Area 3](03-logic.md), question 7 |
| `decimal` | Operations on values built on `Decimal` | 13 (1 decided, 12 new) | [Area 3](03-logic.md), question 7 |
| `money` | `Money` and its rates | 8, new | This page |
| `units` | Physical quantities, `DataSize` and `Bitrate` | 3, new | This page |
| `text` | Operations on text, and the contract's `parse`, `format` and `from` for every type | 21, new | This page |
| `hash` | SHA-256 digests | 1, new | This page |
| `encoding` | Bytes as text and back | 9, new | This page |
| `crypto` | Encryption, passwords, signatures, message authentication | 10, new | This page |
| `random` | Tokens, codes, shuffles; actions only | 5, new | This page |
| `json` | One JSON document in memory, and large JSON lists through batches | 4 (2 decided, 2 new) | This page; the batch forms in [area 12](12-jobs-and-events.md#batches-work-over-many-items) |
| `csv` | CSV files, read and written through batches | 2, decided | [Area 12](12-jobs-and-events.md#batches-work-over-many-items) |
| `tsv` | TSV files, CSV's tab-separated form | 2, decided | [Area 12](12-jobs-and-events.md#batches-work-over-many-items) |
| `json_lines` | JSON Lines files, one value per line | 2, decided | [Area 12](12-jobs-and-events.md#batches-work-over-many-items) |
| `temporal` | Instants, dates, local times, zones, formats, recurrence | 24 (21 decided, 3 new) | [Area 10](10-configuration-and-time.md#zones-and-local-times) |
| `locale` | Choosing a supported locale | 1, unchanged | [Area 10](10-configuration-and-time.md#choosing-a-locale-from-a-request) |
| `colour` | Converting `Colour` between spaces, contrast | 5, new | This page |
| `web` | Building URLs, address blocks | 3, new | This page |
| `semver` | Version ranges | 1, new | This page |
| `media` | Fitting dimensions | 1, new | This page |
| `markup` | Markdown and HTML, made safe | 3, new | This page |

169 operations in all: 28 decided in areas 2, 3 and 10, 8 file helpers decided with batches (area 12), and 133 decided here, beside the operators of the standard operator table. On top of them every
standard type has the contract's baseline (construction, `text.parse`,
`text.format`, `text.from`, JSON, storage, `==` and order), generated, not
counted per type. Of the new operations, 8 wait on maths on `Decimal`
(question 18) and 1 on keys held as data; the 3 that waited on
two-parameter inline functions were built in chunk 5.5c2. The geospatial and AI areas' operations are counted in those areas.

## Shared vocabulary types

> Decided 2026-10-17 (Richard): the principle; the check on a package that declares its own equivalent; every standard type global, in one flat set; the list of standard types below, with its three rules; `Point` and `Length` as the exceptions to the naming rule; and that every standard type meets one contract. How the contract is expressed (question 2) and the naming rule (question 3) were decided the same day, as recommended.

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
- **One flat, global set** (decided). Every standard type is a global
  name, as `Email` and `Instant` are today: `PostalAddress`, never
  `geo.Point` beside a global `Email`, since two tiers of type would be
  one more thing to remember and apps should look alike. Functions stay
  in namespaces (`geo.`, `money.`, `list.`); only the types are global.
  An app doesn't declare its own `Address`: it names its types for their
  purpose and shape (`DeliveryStop { address: PostalAddress }`,
  `BillingContact`), and that forced explicitness is the point. A
  declaration that reuses a standard name is already an error (built-in
  names can't be reused, area 2; for a standard type's name,
  `type.standard_clash`, under "A release that adds a standard type"), so
  the shape check below only has to catch equivalents under other names.
- **Packages use them** (decided): a package's public API, or an app's
  own code, that declares its own equivalent of a standard type gets the
  declinable error `type.standard_exists`, pointing at it ("Packages use
  the standard types", below; question 1), since area 1 has no warnings
  (its question 11).

### The standard types

Decided 2026-10-17 (Richard): 247 types, 87 of them built already (bold; `Range<T>` in chunk 5.5c2, `Money` and `Currency` in 5.5h, `ExchangeRate` in 5.5h2, 17 payment and identity numbers in 5.5i, the 17 quantities with `DataSize` and `Bitrate` in 5.5j, `Percentage`, `Ratio` and `Fraction` in 5.5j2, ten time types in 5.5k, `RecurrenceRule` and `OpeningHours` in 5.5k2, 15 of people and places in 5.5l, the other four in 5.5l2); `CalendarPeriod` and `UkCompanyNumber` were added the same day, after review. "(enum)" marks a closed list held as a built-in enum: twelve are built (`Currency` in chunk 5.5h, `Month` in 5.5k, the seven of people and places in 5.5l), and question 22 decided the others.
Units fold into their quantity type, roles into the general type,
duplicates are merged, and formats and application entities are out. The
[vocabulary types catalogue](research/vocabulary-types-catalogue.md) is
the full reference list behind it: all 1,120 brainstormed entries, each
with its decision. The names follow question 3's rule, which renamed seven
of the adopted names (`CountrySubdivision`, `UnitQuantity`,
`Length`, `SecretText`, `NetworkPort`, `WritingScript`,
`WebOrigin`) and merged `TimeInterval` into the built-in `InstantRange`;
the catalogue keeps the names as brainstormed. `Point` is the one
exception to the rule.

| Group | Types | Extras in |
|---|---|---|
| People and contact (12) | **`PersonName`**, **`GivenName`**, **`FamilyName`**, **`Username`**, **`Email`**, **`EmailWithName`**, **`PhoneNumber`**, **`PhoneExtension`**, **`PostalAddress`**, **`PostalCode`**, **`Uprn`**, **`Nationality`** (enum) | `web` |
| Places (10) | **`Country`** (enum), **`CountrySubdivision`** (enum), **`Continent`** (enum), **`UnLocode`**, **`CallingCode`**, **`Language`** (enum), **`WritingScript`** (enum), **`TextDirection`** (enum), **`Locale`** (enum), **`Zone`** (enum) | |
| Geospatial (17) | `Point` (optional altitude), `Latitude`, `Longitude`, `LineString`, `Polygon`, `MultiPoint`, `MultiLineString`, `MultiPolygon`, `Geometry`, `BoundingBox`, `Circle`, `Bearing`, `Crs`, `EastingNorthing`, `MapTile`, `H3Cell`, `Geohash` | `geo` (future area) |
| Money, payments, banking (24) | **`Money`**, **`Currency`** (enum), **`ExchangeRate`**, `Price`, `UnitPrice`, `TaxRate`, `Discount`, `BasisPoints`, `InterestRate`, **`PaymentCardNumber`**, `MaskedCardNumber`, `CardBrand` (enum), **`CardBin`**, `CardExpiry`, **`PaymentReference`**, **`InvoiceNumber`**, **`Iban`**, **`Bic`**, **`UkSortCode`**, **`UkAccountNumber`**, `BankAccount`, **`AbaRoutingNumber`**, **`Lei`**, **`Isin`** | `money` |
| Tax and identity numbers (11) | `VatNumber`, `TaxId`, **`UkUtr`**, **`UkCompanyNumber`**, **`UkNationalInsuranceNumber`**, **`UsSsn`**, **`UsEin`**, `PassportNumber`, `DrivingLicenceNumber`, `IdentityDocument`, **`NhsNumber`** | |
| Commerce and logistics (20) | `Sku`, `Ean13`, `UpcA`, `Isbn`, `Sscc`, `Barcode`, `UnitQuantity`, `Rating`, `ClothingSize`, `ShoeSize`, `Dimensions`, `HsCode`, `Incoterm` (enum), `TrackingNumber`, `Carrier`, `ContainerNumber`, `LotNumber`, `SerialNumber`, `Vin`, `VehicleRegistration` | `units`, `media` |
| Time and calendar (19) | **`Instant`**, **`CalendarDate`**, **`ZonedTime`**, **`Duration`**, **`Weekday`** (enum), **`LocalTime`**, **`LocalDateTime`**, **`Month`** (enum), **`YearMonth`**, **`Year`**, **`Quarter`**, **`IsoWeek`**, **`UtcOffset`**, **`DateRange`**, **`InstantRange`**, **`CalendarPeriod`**, **`RecurrenceRule`**, **`OpeningHours`**, `CalendarEvent` | `temporal`, `range` |
| Physical quantities (17) | **`Length`**, **`Area`**, **`Volume`**, **`Angle`**, **`Mass`**, **`Pressure`**, **`Energy`**, **`Power`**, **`Temperature`**, **`TemperatureDifference`**, **`Speed`**, **`Frequency`**, **`Voltage`**, **`Current`**, **`Illuminance`**, **`SoundLevel`**, **`Concentration`** | `units` |
| Maths and data (8) | **`Percentage`**, **`Ratio`**, **`Fraction`**, **`Range<T>`**, **`DataSize`**, **`Bitrate`**, `Vector`, `Embedding` | `range`, `units`; AI area |
| Health (9) | `BloodType` (enum), `Icd10Code`, `SnomedConcept`, `Dose`, `DosageInstruction`, `HeartRate`, `BloodPressure`, `BloodGlucose`, `Allergen` (enum) | |
| Media and visual (15) | `Colour`, `ImageDimensions`, `AspectRatio`, `Rect`, `Insets`, `PaperSize` (enum), `ExifOrientation` (enum), `AltText`, `FrameRate`, `VideoResolution`, `VideoCodec`, `AudioCodec`, `Timecode`, `SubtitleCue`, `ContentRating` | `colour`, `media` |
| Web and network (23) | **`Url`**, `UrlPath`, `RelativeUrl`, `Slug`, `DomainName`, `Hostname`, `WebOrigin`, `HttpMethod` (enum), `HttpStatus`, `HttpHeaderName`, **`FileType`**, `UserAgent`, `ProblemDetails`, `JsonPointer`, `Cookie`, `UtmParameters`, **`IpAddress`**, `Cidr`, `NetworkPort`, `MacAddress`, `Imei`, `Emoji`, `PushToken` | `web` |
| Software (19) | `SemanticVersion`, `VersionRange`, `GitRef`, `PackageUrl`, `Cve`, `SpdxLicence`, `TraceId`, `SpanId`, `TraceParent`, `LogLevel` (enum), `FilePath`, `FileName`, `RegularExpression`, `ContainerImage`, `Digest`, **`Uuid`**, `Ulid`, `IdempotencyKey`, `Doi` | `semver` |
| Text formats (4) | `Markdown`, `Html`, `RichText`, `LocalisedText` | `markup`, `text` |
| Security (20) | `Password`, `PasswordHash`, `PasswordPolicy`, `Hash`, `Hmac`, `Jwt`, `JwtClaims`, `Jwk`, `PublicKey`, `PrivateKey`, `Certificate`, `TotpSecret`, `OneTimeCode`, `ApiKey`, `SecretText`, `BearerToken`, `OAuthScope`, `OidcClaims`, `WebAuthnCredential`, `EncryptedValue` | `crypto` |
| Travel (6) | `IataAirport`, `IcaoAirport`, `IataAirline`, `FlightNumber`, `CabinClass` (enum), `BookingReference` | |
| Other with a published standard (10) | `UnNumber`, `Orcid`, `CefrLevel` (enum), `Mpan`, `Mprn`, `EpcRating` (enum), `Tenure` (enum), `LawfulBasis` (enum), `Jurisdiction`, `ModelId` | |
| AI (3) | `TokenCount`, `TokenUsage`, `ChatMessage` | AI area |

**Its three rules**, decided with it:

- **Country-specific identifiers are global types,** like the rest:
  `UkSortCode`, `UsSsn` and `NhsNumber` are named by their country, so
  they don't clash.
- **Gender and sex at birth stay out.** They are contested and personal
  data: an application models them deliberately, and area 15 marks them
  sensitive.
- **A quantity is stored in its SI base unit,** with the unit chosen at
  construction and display: `units.of(5, LengthUnit.mile)` holds metres,
  and `text.format(distance, locale:, unit: LengthUnit.mile)` shows miles.
  `==` and ordering then just work, and storage is one `numeric` column.

**Folded or cut, so nothing is lost** (each entry's decision is in the
catalogue's Decision column):

| What | Became |
|---|---|
| Units (`Metre`, `Mile`, `Kilogram`, `Pound`, `Celsius`, `Litre`, `KilowattHour`, every `...Unit` enum and 17 more) | The quantity type, with a unit at construction and display |
| Roles: `Height`, `GeoDistance` · `FloorArea` · `WindSpeed` · `BodyTemperature` · `BarometricPressure` · `MediaDuration` · `DateOfBirth`, `ExpiryDate` · `CountryOfOrigin` · `UsState`, `Region` · `Salary` | `Length` · `Area` · `Speed` · `Temperature` · `Pressure` · `Duration` · `CalendarDate` · `Country` · `CountrySubdivision` · `Money`: a field of the general type |
| Duplicates: `Timestamp`, `Uri`, `Ipv4Address`, `Ipv6Address`, `HexColour`, `HslColour`, `Sha256`, `UuidV7`, `Point3D`, `CountryAlpha3`, `LanguageAlpha3`, `UkPostcode`, `UsZipCode`, `UkNumberPlate`, `EuVatNumber`, `UkVatNumber`, `TimeRange`, `MinorUnits`, `MediaType`, `ImageFormat` | The one standard type each duplicates (the catalogue names which) |
| Also folded in review: `AddressLine`, `BarcodeSymbology`, `DoseFrequency`, `FlightDesignator`, `GitSha`, `Gtin`, `HttpHeaders`, `Humidity`, `PackageDimensions`, `PackageName`, `PhysicalQuantity`, `UnitOfMeasure`, `Ucum`, `Proportion`, `QueryString`, `SubtitleTrack`, `UkBankAccount`, `WebhookSignature`, `WindDirection`, `CarbonDioxideEquivalent` | `PostalAddress`, `Barcode`, `DosageInstruction`, `FlightNumber`, `GitRef`, `Ean13`, `HttpHeaderName`, `Percentage`, `Dimensions`, `PackageUrl`, `UnitQuantity` (three), `Ratio`, `Url`, `SubtitleCue`, `BankAccount`, `Hmac`, `Bearing`, `Mass` |
| Formats: `Hex`, `Base64`, `Wkt`, `Json`, `CsvDialect`, `MessageFormat`, `VCard`, `OtpAuthUri`, `CssLength` | Out: `encoding.` and `json.` operations |
| Application entities: `Recipe`, `HotelStay`, `OrderStatus`, `PaymentStatus`, `Consent`, `SensorReading`, `StoredFile`, `UserId`, `Allergy` and 31 more | Out: an application's own records |
| `Gender`, `SexAtBirth`, `Cvv` | Out: sensitive (a CVV is never stored) |
| `HashAlgorithm`, `KeyAlgorithm`; `SessionId`; `AcceptLanguage` | Out: no algorithm choices; the runtime's (area 8); a call, `locale.from_accept_language` |
| Niche, silly, or a maybe with no published standard | Out: 729 entries, a package's to define |

### Every standard type meets one contract

Decided 2026-10-17 (Richard): every standard type has a library, at least
a baseline, and the baseline is the same for every type, not chosen type
by type. How it is expressed is question 2.

| Baseline | What it is | Where a type has none |
|---|---|---|
| Construct and validate | From its parts (`PostalCode(raw)`, `Money` from `money.from_minor`), and from text with `text.parse(raw, as: T)`; each failure named by the rule it breaks, `malformed` for text that isn't the type at all | Never: every type has both |
| `format` for people | `text.format(value, locale:)`, giving `PresentationText`, through CLDR where the locale matters (an address's line order, a name's order, a quantity's unit) | Every type has one |
| Canonical text | `text.from(value)`: E.164, ISO codes, `#rrggbb`, ISO 8601, `pkg:` URLs; the form `text.parse` reads back | Types with no single text form (`PostalAddress`, `PersonName`); asking is a compile error |
| JSON | One shape, read and written by `json.read`, `json.write` and every route | Never |
| Storage | One shape: a column type, or named columns for an object (`Money`'s two) | Never |
| `==`, and order | `==` on every type; `<` and `sort_by` where the type is ordered (quantities of one kind, `Money` of one currency, times, `SemanticVersion`, `Percentage`) | Order only where it means something |
| Conversions at named edges | `geo.from_lng_lat`, `units.in`, `Isbn` 10 to 13, `MaskedCardNumber` from a `PaymentCardNumber` | Only where the type has another form |

`text.parse`, `text.format` and `text.from` are each one operation over
every standard type, as `json.read` is one over every input type, so the
contract adds 2 names, not 700.

> Built in chunk 5.5a for the 13 standard types built so far: each is a
> stub in `crates/jadpo-check/std/standard.jadpo` and a data entry in
> `jadpo-std`'s `standard` module, which the three operations, JSON,
> storage and `<` read, and one harness runs every type through. Each
> later group arrives as stubs and entries ("Chosen in chunk 5.5a", under
> "Review"). Chunk 5.5c2 added `Range<T>`, the first that takes a type
> argument, whose entry's examples name the type they hold ("Chosen in
> chunk 5.5c2").

A form sends a quantity as text, an order's time goes into a log line as
its canonical text, and a day, a duration and a zone are shown to people
in their own words:

```jadpo check project=contract
type OrderedAt = Instant {}

type LogText = Text {}

type FormText = Text {}

type Quantity = Int {
    min: 1
    max: 12
}

type QuantityUnreadable = Failure {
    kind: FailureKind.invalid_value
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

/// The quantity a form sent: text that may not be a number at all
/// (`malformed`), or a number outside `Quantity`'s rules.
function quantity_from(raw: FormText) fails QuantityUnreadable, QuantityTooLarge -> Quantity {
    var quantity: Quantity = attempt text.parse(raw, as: Quantity) {
        malformed: QuantityUnreadable
        invalid: QuantityTooLarge
    }
    return quantity
}

/// When an order was placed, as a log line writes it.
function placed_text(at: OrderedAt) -> LogText {
    return LogText(text.from(at))
}
```

```jadpo check project=contract file=main.test.jadpo
test "standard values are read, written and shown one way" {
    var ordered_quantity: Quantity = attempt quantity_from("4")
    assert ordered_quantity == 4
    assert quantity_from("13") fails QuantityTooLarge
    assert quantity_from("four") fails QuantityUnreadable
    assert placed_text("2026-10-25T09:30:00Z") == "2026-10-25T09:30:00Z"
    assert text.from(Zone.europe_london) == "Europe/London"
    assert text.format(Weekday.monday, locale: Locale.fr_fr) == "lundi"
    assert text.format(90m, locale: Locale.en_gb) == "1 hour, 30 minutes"
    assert text.format(Locale.fr_fr, locale: Locale.en_gb) == "French (France)"
}
```

**Extras beyond the baseline**, by group, each with its full table under "Operations" below:

| Group | Extras | Count |
|---|---|---|
| Money and payments | `money.from_minor`, `of`, `minor`, `currency`, `sum`, `allocate`, `split`, `convert` (with a given `ExchangeRate`, never fetched) | 8; arithmetic by the operator table |
| Physical quantities, `DataSize`, `Bitrate` | `units.of`, `in`, `sum`; operators within a kind and the defined products and quotients | 3 |
| Time and calendar | Area 10's 21, plus `temporal.occurrences`, `temporal.open_at` and `temporal.period_between`; `CalendarPeriod` arithmetic by the operator table | 3 new |
| Ranges (`Range<T>`, `DateRange`, `InstantRange`) | `range.contains`, `range.overlaps`, `range.intersection` | 3 |
| Colour | `colour.from_hsl`, `from_oklch`, `mix`, `with_lightness`, `contrast` (WCAG 2) | 5 |
| Security | `crypto` as tabled below, with `sign`, `verify_signature`, `verify_jwt` and `verify_totp` | 10 |
| Web | `web.with_query(url, query)`, `web.resolve(base, relative)`, `web.cidr_contains(range, address)` | 3 |
| Software | `semver.satisfies(version, range)` | 1 |
| Media | `media.fit(dimensions, within:)`, keeping the aspect ratio | 1 |
| Text formats | `markup.to_html`, `rich_to_html`, `to_plain_text`; `text.pick` with fallback. `Html` is always safe: its construction sanitises (#88) | 4 |
| Geometry | `geo.intersects`, `contains`, `within`, `distance`, `bounds` | Future area |
| AI (`Vector`, `Embedding`, `ChatMessage`) | Similarity and generation | Future area |
| People, places, identity numbers, commerce codes, health, travel, other | Baseline only: each identifier's check digit or pattern is its construction | |

### Packages use the standard types

Decided 2026-10-17 (Richard), as a warning on packages' public types,
then made a declinable error for every type declaration the same day
(question 1). A type whose shape is a standard type's is the error
`type.standard_exists`: in a package's public types (#88's
`surface.jadpo_record` lists them) and in an application's own code alike,
since an app's `Location { lat, lng }` reaches its API and its tables
exactly as a package's would. It is declinable (area 1, question 11), so a
genuinely different meaning is kept with a reason on the declaration. It
matches by shape and by the common names for each field, not by exact
names: an `Object` whose only fields are those below, in any order, and
only distinctive shapes match (question 5).

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
error: `GeoPoint` is the standard `Point` under another name
 --> types/geo_point.jadpo:3:13
  |
3 | public type GeoPoint = Object {
  |             ^^^^^^^^ a latitude and a longitude
  reason: `GeoPoint` has only a latitude (`lat`) and a longitude (`lng`), which is the standard `Point`. Packages that each declare their own disagree on order and on names, and an app has to convert between them.
  next: Use `Point` in the package's public types, converting at the edge with `geo.from_lat_lng` or `geo.from_lng_lat` where the provider sends a pair (agent), or decline with a reason (`decline type.standard_exists: "..."`) if it means something else
  rule: type.standard_exists
  help: docs/language/19-standard-library.html#packages-use-the-standard-types

check failed: 1 error, 1 file
```

The error stops `check` and the build until one of two things is done:

- **Swap to the standard type.** For a type not yet stored or on the
  wire, the fix makes the swap in one step, and an agent may apply it,
  converting at the edge, in a package's `wire:` or `decode:` (#88), from
  whatever the provider sends. For a type already stored or in an API
  record, the fix writes the edit and the data migration, and the change
  goes through the existing `ApiChange` and `DataChange` acceptances
  (area 18).
- **Decline, with a reason, on the type itself,** with area 1's generic
  line: `decline type.standard_exists: "Grid squares, not WGS84 degrees"`
  directly above the declaration. An agent or a person may decline, and
  `jadpo approval` lists every decline with its reason, so declining by
  reflex shows in review.

The cost is false positives: an `Object` of `x` and `y` grid cells, or an
`amount` and `currency` that are a quote, not money, matches by shape. The
required reason handles them, and review sees each one.

> Built in chunk 5.5a on app code, with the first match signature,
> `InstantRange`'s (a start and an end, each an `Instant`), since `Point`,
> `Money`, `Colour`, `LineString` and `Polygon` arrive with their groups;
> the output above is the error as it will read for `Point`. The swap's
> one-step edit is `jadpo fix <name> swap` and the editor's quick fix
> (chunk 5.5a2, under "A release that adds a standard type"); the data
> migration and the acceptances for a type already stored or on the wire
> come with area 18's records, and until then such a swap stops (below).
> Packages' public types come with the publishing bar (chunk 7.1). Its
> example is under "Errors".

The `Country`, `Language` and `PhoneNumber` lists ship with each Jadpo
release, as the time-zone database does (#88, "built in or a library
package"). That also answers #88's open point for 5.4, where fixed
reference data can be declared: in the release, as these enums, not by
applications.

### A release that adds a standard type

Decided 2026-10-17 (Richard, question 4): a Jadpo upgrade that adds a
standard type an application already names for itself never breaks it
silently. The first `check` after the upgrade reports each clash as the
error `type.standard_clash`, naming the standard type and the release that
added it, with two fixes:

- **Swap to the standard type,** the same swap as `type.standard_exists`'s:
  every reference to the application's type becomes a reference to the
  standard one, each field the shape matched is read by the standard
  field's name, and the declaration goes.
- **Rename the application's type,** keeping its own meaning under a name
  of its own.

`jadpo fix` applies whichever the author picks, through the same code as
the editor's quick fix (area 14), and each release's notes list the names
it adds.

`jadpo fix` with no arguments lists each finding it can fix, and `jadpo
fix <name> swap` or `jadpo fix <name> rename <NewName>` makes one, listing
what changed file by file:

```text
  domains/rooms/types/slot.jadpo
    - Slot        type, the standard InstantRange replaces it
    - slot.jadpo  file, nothing left in it

  domains/rooms/actions/slot_of.jadpo
    ~ line 4      InstantRange, was Slot
    ~ line 5      InstantRange, was Slot
    ~ line 6      start, was starts_at
    ~ line 7      end, was ends_at

✓ Swapped Slot for the standard InstantRange

→ jadpo check  shows the error the project still has
```

A fix never adds an error: one whose result would have an error the
project didn't have changes nothing and shows that error, as a swap to a
standard enum without a variant the code names does. Errors the project
already had stay, and `jadpo check` shows them.

**Names each release adds,** the release notes' list, held to the data
entries by a test:

| Release | Standard types it adds |
|---|---|
| 0.1.0 | `AbaRoutingNumber`, `Angle`, `Area`, `Bic`, `Bitrate`, `CalendarDate`, `CalendarEvent`, `CalendarPeriod`, `CallingCode`, `CardBin`, `Concentration`, `Continent`, `Country`, `CountrySubdivision`, `Currency`, `Current`, `DataSize`, `DateRange`, `Duration`, `Email`, `EmailWithName`, `Energy`, `ExchangeRate`, `FamilyName`, `FileType`, `Fraction`, `Frequency`, `GivenName`, `Iban`, `Illuminance`, `Instant`, `InstantRange`, `InvoiceNumber`, `IpAddress`, `Isin`, `IsoWeek`, `Language`, `Lei`, `Length`, `LocalDateTime`, `LocalTime`, `Locale`, `Mass`, `Money`, `Month`, `Nationality`, `NhsNumber`, `OpeningHours`, `PaymentCardNumber`, `PaymentReference`, `Percentage`, `PersonName`, `PhoneExtension`, `PhoneNumber`, `PostalAddress`, `PostalCode`, `Power`, `Pressure`, `Quarter`, `Range`, `Ratio`, `RecurrenceRule`, `SoundLevel`, `Speed`, `Temperature`, `TemperatureDifference`, `TextDirection`, `UkAccountNumber`, `UkCompanyNumber`, `UkNationalInsuranceNumber`, `UkSortCode`, `UkUtr`, `UnLocode`, `Uprn`, `Url`, `UsEin`, `UsSsn`, `Username`, `UtcOffset`, `Uuid`, `Voltage`, `Volume`, `Weekday`, `WritingScript`, `Year`, `YearMonth`, `Zone`, `ZonedTime` |

> Built in chunk 5.5a2: each data entry's `release`, `type.standard_clash`
> on any declaration named like a standard type (its example is under
> "Errors"), and `jadpo fix` with the editor's "Swap for the standard ..."
> quick fix, both through `jadpo_check::fixes`. The rename is `jadpo fix`'s
> only, written through the same code: it isn't `jadpo rename` (area 14,
> after the first build), so it writes no identity line, and a rename
> whose result changes stored data or an API record stops ("Chosen in
> chunk 5.5a2", under "Review").

## What every operation follows

These hold for every namespace, and are what the decided areas already
apply to `collection`, `decimal`, `integer` and `temporal`.

- **One namespace per type it works on** (question 6): `list.`, `set.` and
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
  (question 19). Unicode data (case mapping, white space, the slug's
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

**Construction is the way** (question 15). `Type(value)` checks every rule
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
so cross-field rules are a language change of their own, decided with
question 15 and built in chunk 5.5t ("Language gaps").

## Operations

Every operation beyond the contract's baseline, with its signature,
failures and SQL form, one section per namespace or type group. The
baseline itself (construction, `text.parse`, `text.format`, `text.from`,
JSON, storage, `==` and order) is defined once, under "Every standard
type meets one contract", and isn't repeated here.

| Section | Namespace | Operations |
|---|---|---|
| The standard operator table | | Operators, not counted |
| Lists | `list` | 26 |
| Sets | `set` | 7 |
| Maps | `map` | 10 |
| Ranges | `range` | 3 |
| Numbers | `integer`, `decimal` | 18 |
| Money and payments | `money` | 8 |
| Physical quantities | `units` | 3 |
| Time and calendar additions | `temporal` | 3 new, 21 decided |
| Text | `text` | 21 |
| Text formats | `markup` | 3 |
| Hashing and cryptography | `hash`, `crypto` | 11 |
| Randomness | `random` | 5 |
| Encodings | `encoding` | 9 |
| Data formats | `json`, `csv`, `tsv`, `json_lines` | 10 |
| Colour | `colour` | 5 |
| Media | `media` | 1 |
| Web and software | `web`, `semver` | 4 |
| Groups with the baseline only | | 0 |
| Locale | `locale` | 1 decided |

## The standard operator table

Operators on standard types follow one fixed, documented table, instead of
a library function per job. It generalises what area 10 already decided
for time (`Instant - Instant` gives a `Duration`): the compiler knows each
row, so `price * 20%` and `distance / 2h` read as they would on paper and
are checked as strictly as a call. Anything not in the table is a compile
error naming the two types. Area 3's rules for numbers and `+` on text
stand.

| Left | Operator | Right | Gives | Notes |
|---|---|---|---|---|
| `Instant` | `+`, `-` | `Duration` | `Instant` | Decided (area 10) |
| `Instant` | `-` | `Instant` | `Duration` | Decided (area 10) |
| `Duration` | `+`, `-` | `Duration` | `Duration` | Decided (area 10) |
| `CalendarDate` | `+`, `-` | `CalendarPeriod` | `CalendarDate` | Years, then months, then days; a day past the month's end clamps to its last day |
| `CalendarPeriod` | `+`, `-` | `CalendarPeriod` | `CalendarPeriod` | Part by part (`P1M + P10D` is `P1M10D`) |
| `CalendarPeriod` | `*` | `Int` | `CalendarPeriod` | Each part multiplied |
| A quantity | `+`, `-`, `*`, `/` | as tabled | as tabled | "Physical quantities" below |
| `Money` | `+`, `-`, `<` ... | `Money` | `Money`, `Bool` | One currency (below) |
| `Money` | `*` | a number or `Percentage` | `Money` | Rounded by the type's `rounding:` |
| `Money` | `/` | `Decimal` | `Money` | A rate (`/ 1.20` takes VAT off), rounded by the type's `rounding:` |
| `Money` | `/` | `Money` | `Decimal` | One currency; as `/` gives for `Decimal` (area 3) |
| `Money` | `/` | `Int` | refused | `type.money_split`: use `money.split` or `money.allocate` |
| `Money` | `+`, `-` | `Percentage` | refused | Ambiguous: write `price * 120%` or `price + price * 20%` |
| `Percentage` | `+`, `-` | `Percentage` | `Percentage` | `20% + 5%` is `25%` |
| `Percentage` | `*` | a number or `Percentage` | `Percentage` | `50% * 20%` is `10%` |
| A number or quantity | `*` | `Percentage` | the same kind | `distance * 10%` |

- **Money in one currency.** Where both types fix the currency
  (`type Price = Money { currency: Currency.gbp }`), adding two currencies
  is a compile error. Where a currency is known only while running (a
  SaaS billing each customer in theirs), `+`, `-` and comparisons can fail
  with the condition `currency_mismatch`, mapped as any construction rule
  is: `attempt subtotal + line.amount { currency_mismatch:
  InvoiceCurrencyMismatch }`. Inside `if (money.currency(a) ==
  money.currency(b))` the checker has the proof and the operator is plain,
  as a range proves a construction (area 2).
- **Rounding is the type's.** A Money type may declare `rounding:`
  (`type Price = Money { currency: Currency.gbp  rounding: Rounding.half_up
  }`), used by `*` and `/`; without one it is `half_even`, as constructing
  into a type with a fixed `scale` rounds (area 2). So multiplying needs no
  call, and `money.multiply` and `money.divide` are gone.
- **`/` by an `Int` is refused** with a fix to `money.split` or
  `money.allocate`. Dividing money by a count is almost always sharing it
  out, and rounding division loses a minor unit (`£10.00 / 3` is `£3.33`,
  three times `£9.99`); `split` hands every penny to someone. `/` by a
  `Decimal` stays, because that is a rate, not a share: `gross / 1.20`
  takes VAT off. Halving a price is `price * 50%`.
- **The `%` literal.** `20%` is a `Percentage` (`0.20`); `%` as remainder
  (area 3) is always written with spaces, as the formatter writes every
  binary operator, so the two never meet in formatted code. `20%3` is an
  error naming both readings (`syntax.percent_ambiguous`).
- **A ratio becomes a `Percentage` by construction.** Dividing two
  quantities gives a plain `Decimal` (`lip / total`), and
  `Percentage(ratio)` makes it one for display: `0.25` gives `25%`
  ("Chosen in chunk 5.5d").
- **No user-defined operators.** An application's types can't declare
  operators, so `a + b` always means a row of this table or area 3's, and
  a reviewer never has to find an overload to know what it does, whether
  it can fail or what it costs. Area 3 already keeps generics to the
  standard library for the same reason.

A bakery adds VAT to its cakes, trims its shelves and reports how much of
a batch passed, each with a percentage:

```jadpo check project=percentages
type CakePrice = Money {
    currency: Currency.gbp
}

type VatRate = Percentage {
    min: 0%
    max: 100%
}

type PassRate = Percentage {}

type Offcut = Length {}

type Share = Decimal {
    scale: 4
}

/// The VAT on a cake's price at `rate`, rounded half to even to a penny.
function vat_on(price: CakePrice, rate: VatRate) -> CakePrice {
    return CakePrice(price * rate)
}

/// A cake's price with VAT added: the whole price and the rate on top.
function with_vat(price: CakePrice, rate: VatRate) -> CakePrice {
    return CakePrice(price * (100% + rate))
}

/// What is trimmed off a shelf cut from a 2.5262 m board: 15% of it.
function offcut() -> Offcut {
    var total: Length = 2metre + 45cm + 3inch
    var trim: Length = total * 15%
    return Offcut(trim)
}

/// The offcut for the customer, in centimetres.
function offcut_label(customer: Locale) -> PresentationText {
    return text.format(offcut(), locale: customer, unit: LengthUnit.centimetre)
}

/// How much of a batch passed, from the share that did.
function pass_rate(good_share: Share) -> PassRate {
    return PassRate(good_share)
}

/// A rate for the customer, as their locale writes a percentage.
function rate_label(rate: PassRate, customer: Locale) -> PresentationText {
    return text.format(rate, locale: customer)
}

/// Each of four people's share of a cake.
function cake_share() -> Fraction {
    return Fraction {
        numerator: 1
        denominator: 4
    }
}
```

```jadpo check project=percentages file=main.test.jadpo
test "a percentage scales money, numbers and quantities" {
    var cake: CakePrice = CakePrice(money.from_minor(1250, Currency.gbp))
    assert vat_on(cake, 20%) == money.from_minor(250, Currency.gbp)
    assert with_vat(cake, 20%) == money.from_minor(1500, Currency.gbp)
    var penny_cake: CakePrice = CakePrice(money.from_minor(100, Currency.gbp))
    assert vat_on(penny_cake, 12.5%) == money.from_minor(12, Currency.gbp)
    assert 50% * 20% == 10%
    assert 20% + 5% == 25%
}

test "the offcut is exact in metres and shown in centimetres" {
    assert offcut() == 0.37893metre
    assert offcut_label(Locale.en_gb) == "37.893 cm"
    assert offcut_label(Locale.de_de) == "37,893 cm"
}

test "a ratio becomes a percentage, and fractions are written as people write them" {
    assert pass_rate(0.25) == 25%
    assert rate_label(pass_rate(0.175), Locale.en_gb) == "17.5%"
    assert text.from(pass_rate(0.175)) == "17.5%"
    assert text.from(cake_share()) == "1/4"
    assert cake_share().denominator == 4
}
```

A voucher keeps its discount in one `numeric` column, as the fraction it
is:

```jadpo check project=percentage-vouchers file=app.jadpo hidden
application {}
```

```jadpo check project=percentage-vouchers file=domains/bakery/types/discount_rate.jadpo hidden
/// A discount off a cake, as a percentage of its price.
type DiscountRate = Percentage {
    min: 0%
    max: 100%
}
```

```jadpo check project=percentage-vouchers file=domains/bakery/entities/voucher.jadpo migrate=initial
use bakery/types { DiscountRate }

/// A voucher a customer brings to the counter.
entity Voucher {
    id: Uuid
    reduction: DiscountRate  // one `numeric` column: 0.15 for 15%
}
```

```jadpo check project=percentage-vouchers file=domains/bakery/actions/issue_voucher.jadpo hidden
use bakery/entities { Voucher }
use bakery/types { DiscountRate }

/// Issues a voucher taking `reduction` off a cake's price.
action issue_voucher(reduction: DiscountRate) -> Voucher {
    return Voucher.create {
        set: {
            reduction: reduction
        }
    }
}
```

```jadpo check project=percentage-vouchers file=domains/bakery/actions/issue_voucher.test.jadpo
use bakery/entities { Voucher }

test "a voucher keeps its discount as it was issued" {
    var voucher_made: Voucher = issue_voucher(15%)
    var voucher_on_file: Voucher = attempt Voucher.get(voucher_made.id)
    assert voucher_on_file.reduction == 15%
    assert text.from(voucher_on_file.reduction) == "15%"
}
```

> Built in chunk 5.5j2: `Percentage`, `Ratio` and `Fraction` on the
> contract; the `%` literal with `syntax.percent_ambiguous`; the table's
> `Percentage` rows; `Percentage(ratio)`; one `numeric` column; and
> `{rate, number, percent}` in messages (area 10). What the pages left
> open is "Chosen in chunk 5.5j2", under "Review".

## SQL forms

A **SQL form** is a PostgreSQL 16 expression that gives exactly the result
the runtime gives, for every argument. It matters in one place today: a
computed backfill whose every operation has a SQL form runs as one
`UPDATE` per batch, and any other runs through `jadpo migrate apply`,
reading and writing each batch ([area 5](05-data.md#computed-backfills)).
Both are correct; the SQL form is only faster. Queries still compute
nothing and a one-step update's `set:` still works a stored field out only
with `+` and `-` (area 6); aggregate queries would be their second user
(question 28, "Language gaps").

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
(question 6). `T` is the item type, fixed by the list. A function argument
is written inline, `item -> expression` (area 3).

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `count(items)` | `Int`, at least 0 (decided) | | `jsonb_array_length(x)` |
| `contains(items, item)` | `Bool`: an item `==` `item` (decided) | | none |
| `map(items, each)` | `List<U>`, in order (decided) | | none |
| `filter(items, keep)` | `List<T>`, the items `keep` accepts, in order (decided) | | none |
| `sum(items)` | `Int` or `Decimal`, the items added; `0` for none. Items built on `Int` or `Decimal`; `Money` uses `money.sum` | overflow is a defect | none |
| `any(items, check)` | `Bool`: `check` accepts at least one; `false` for none | | none |
| `all(items, check)` | `Bool`: `check` accepts every one; `true` for none | | none |
| `find(items, check)` | `T?`, the first item `check` accepts | | none |
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
| `zip_with(first, second, (a, b) -> ...)` | `List<U>`, pairing items by position, as long as the shorter | | none |

- **No push or pop.** Values never change (area 2), so adding is
  `list.append(items, item)`, a new list, and taking the last off is
  `list.last` then `list.drop_last(items, 1)`.
- **The largest is `max_by`,** not `first(reverse(sort_by(...)))`, which
  the first draft of this page had to write and which sorts the whole list
  to read one item. In a query it stays `order:` and `limit:`.
- **Cutting from either end** is `keep_first`, `keep_last`, `drop_first`
  and `drop_last`, the same four on text (question 7). `slice` stays for a
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

A day's baking, through every list operation (chunks 5.5b and 5.5c2):

```jadpo check project=list-operations
type BatchName = Text {
    min_length: 1
    max_length: 40
}

type Grams = Int {
    min: 0
}

type Shelf = Int {
    min: 1
    max: 6
}

type LoafPrice = Decimal {
    scale: 2
    min: 0.00
}

type Batch = Object {
    name: BatchName
    grams: Grams
    shelf: Shelf
    price: LoafPrice
}

type OverPlan = Bool {}

/// The flour a day's batches take. Items never below zero add up to a sum
/// never below zero, so building `Grams` from it needs no arm.
function flour_needed(batches: List<Batch>) -> Grams {
    return Grams(list.sum(list.map(batches, batch -> batch.grams)))
}

/// The day's takings at full price.
function takings(batches: List<Batch>) -> LoafPrice {
    return LoafPrice(list.sum(list.map(batches, batch -> batch.price)))
}

/// The heaviest batch, if there is one: `max_by` reads each batch once,
/// where sorting would order them all to read one.
function heaviest(batches: List<Batch>) -> Batch? {
    return list.max_by(batches, batch -> batch.grams)
}

/// The batches on each shelf, in baking order.
function by_shelf(batches: List<Batch>) -> Map<Shelf, List<Batch>> {
    return list.group_by(batches, batch -> batch.shelf)
}

/// Whether each batch came out heavier than planned, batch by batch: a
/// two-parameter inline function is given one item from each list.
function over_plan(batches: List<Batch>, goals: List<Grams>) -> List<OverPlan> {
    return list.zip_with(batches, goals, (batch, goal) -> OverPlan(batch.grams > goal))
}
```

Each operation gives a new list, or an item of the list as it is, still
a `Batch`; `takings([])` is `0.00`, a `Decimal`, because a sum is its
items' representation even when there are none. `by_shelf`'s map is read
with `map.get` and `map.keys` (under "Maps").

```jadpo check project=list-operations file=main.test.jadpo
test "list operations give new lists and keep the items as they are" {
    var rye: Batch = Batch {
        name: "Rye"
        grams: 800
        shelf: 2
        price: 4.20
    }
    var bap: Batch = Batch {
        name: "Bap"
        grams: 90
        shelf: 1
        price: 0.60
    }
    var spelt: Batch = Batch {
        name: "Spelt"
        grams: 800
        shelf: 2
        price: 3.80
    }
    var day: List<Batch> = [rye, bap, spelt]
    var weights: List<Grams> = [800, 90, 800]
    assert flour_needed(day) == 1690
    assert takings(day) == 8.60
    assert takings([]) == 0.00
    assert list.any(day, batch -> batch.shelf == 1)
    assert not list.all(day, batch -> batch.grams > 100)
    assert list.find(day, batch -> batch.shelf == 2) == rye
    assert list.first(day) == rye
    assert list.last(day) == spelt
    assert list.at(day, 1) == bap
    assert list.at(day, 3) == none
    assert heaviest(day) == rye  // the first of equal keys
    assert list.min_by(day, batch -> batch.price) == bap
    assert list.sort_by(day, batch -> batch.name) == [bap, rye, spelt]
    assert list.sort_by(day, batch -> batch.grams) == [bap, rye, spelt]  // equal keys keep their order
    assert list.reverse(day) == [spelt, bap, rye]
    assert list.distinct(weights) == [800, 90]
    assert list.slice(day, 1, 5) == [bap, spelt]
    assert list.keep_first(day, 2) == [rye, bap]
    assert list.keep_last(day, 2) == [bap, spelt]
    assert list.drop_first(day, 1) == [bap, spelt]
    assert list.drop_last(day, 5) == []
    assert list.append([rye], bap) == [rye, bap]
    assert list.concat([rye], [bap, spelt]) == day
    assert list.flatten([[rye], [], [bap, spelt]]) == day
    assert over_plan(day, [700, 100, 800]) == [true, false, false]
    assert over_plan(day, [700]) == [true]  // as long as the shorter
}

test "group_by keeps each shelf's batches in baking order" {
    var rye: Batch = Batch {
        name: "Rye"
        grams: 800
        shelf: 2
        price: 4.20
    }
    var bap: Batch = Batch {
        name: "Bap"
        grams: 90
        shelf: 1
        price: 0.60
    }
    var spelt: Batch = Batch {
        name: "Spelt"
        grams: 800
        shelf: 2
        price: 3.80
    }
    var shelves: Map<Shelf, List<Batch>> = by_shelf([rye, bap, spelt])
    assert map.keys(shelves) == [1, 2]
    assert map.get(shelves, 2) == [rye, spelt]
    assert map.get(shelves, 1) == [bap]
    assert map.get(shelves, 3) == none
    assert map.count(by_shelf([])) == 0
}
```

A count or a position that might be negative is `type.argument_negative`
(under "Errors"), and a key that gives something with no order, such as a
`Bool`, is area 3's `type.ordering_operand`.

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
- **`for` walks a set** (question 8, "Language gaps"). Area 2 already
  decided the order: everything that shows a set, iteration "once `for`
  takes sets" included, uses one canonical order by value. So the only
  change is `for` accepting a `Set`, in that order. A set is still never
  indexed and has no first item.
- **`filter`, `any`, `all` and `map` on a set** wait for a scenario that
  needs them; `for` covers it meanwhile.
- **Example 04** keeps `Recipe.allergens` as a list only because a set
  couldn't be searched; with `set.contains` it becomes the set it is.

A bakery's labels, through every set operation and a `for` over a set
(chunk 5.5c):

```jadpo check project=set-operations
type RecipeName = Text {
    min_length: 1
    max_length: 40
}

type Hazard = Enum {
    gluten
    milk
    nuts
    sesame
}

type Recipe = Object {
    name: RecipeName
    warnings: Set<Hazard>
}

type SafeToEat = Bool {}

/// Whether `recipe` carries none of the warnings in `avoid`.
function safe_for(recipe: Recipe, avoid: Set<Hazard>) -> SafeToEat {
    return SafeToEat(set.count(set.intersection(recipe.warnings, avoid)) == 0)
}

/// Every warning a tray of recipes carries, each once.
function tray_warnings(recipes: List<Recipe>) -> Set<Hazard> {
    var mut found: Set<Hazard> = []
    for recipe in recipes {
        found = set.union(found, recipe.warnings)
    }
    return found
}

/// The warnings as a label prints them: a set is walked in its canonical
/// order, which for an enum is the order its variants are declared in.
function label(warnings: Set<Hazard>) -> List<Hazard> {
    var mut shown: List<Hazard> = []
    for hazard in warnings {
        shown = list.append(shown, hazard)
    }
    return shown
}
```

```jadpo check project=set-operations file=main.test.jadpo
test "set operations give new sets" {
    var loaf: Recipe = Recipe {
        name: "Seeded loaf"
        warnings: [Hazard.sesame, Hazard.gluten]
    }
    var flapjack: Recipe = Recipe {
        name: "Flapjack"
        warnings: [Hazard.milk]
    }
    assert set.count(loaf.warnings) == 2
    assert set.contains(loaf.warnings, Hazard.sesame)
    assert not set.contains(flapjack.warnings, Hazard.nuts)
    assert set.with(loaf.warnings, Hazard.gluten) == loaf.warnings  // already there
    assert set.with(flapjack.warnings, Hazard.nuts) == [Hazard.milk, Hazard.nuts]
    assert set.without(loaf.warnings, Hazard.sesame) == [Hazard.gluten]
    var in_either: Set<Hazard> = set.union(loaf.warnings, flapjack.warnings)
    assert in_either == [Hazard.gluten, Hazard.milk, Hazard.sesame]
    assert set.intersection(loaf.warnings, [Hazard.sesame, Hazard.nuts]) == [Hazard.sesame]
    assert set.difference(loaf.warnings, [Hazard.sesame]) == [Hazard.gluten]
    assert safe_for(flapjack, [Hazard.nuts])
    assert not safe_for(loaf, [Hazard.nuts, Hazard.sesame])
    assert tray_warnings([loaf, flapjack]) == [Hazard.milk, Hazard.sesame, Hazard.gluten]
    assert label(loaf.warnings) == [Hazard.gluten, Hazard.sesame]
}
```

A set compares without order, so the literal on the right of `==` can be
written in any order; walking one is the only place its order shows, and
it is always the canonical one, on every target.

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
| `filter(entries, (key, value) -> ...)` | `Map<K, V>`, the entries accepted | | none |
| `map_values(entries, (key, value) -> ...)` | `Map<K, U>`, each key with the value given for it | | none |

- **A map is walked through its keys:** `for key in map.keys(prices)`,
  in canonical order.
- **An empty map is `[]` written where a map is expected,** as a list
  literal written where a set is expected is a set (area 2); entries are
  added with `map.with`. A literal with entries is area 2's to design
  (question 8).
- **Records have no dynamic keys,** on purpose. A record's fields are
  fixed and each is its own type (`Customer.email`), so `record[name]`
  would give a value of no single type, lose the field type that keeps
  identifiers apart, and hide which field policy and personal-data marks
  apply to. Keys known only while running are what a `Map` is for.

A shop's stock, through every map operation (chunks 5.5c and 5.5c2);
`filter` and `map_values` give their inline function each entry's key and
value, in the canonical order of the keys:

```jadpo check project=map-operations
type Loaf = Text {
    min_length: 1
    max_length: 40
}

type Stock = Int {
    min: 0
}

/// The stock after `count` more of `loaf` arrive.
function delivered(stock: Map<Loaf, Stock>, loaf: Loaf, count: Stock) -> Map<Loaf, Stock> {
    var on_hand: Stock = match map.get(stock, loaf) {
        some(found) -> found
        none -> 0
    }
    return map.with(stock, loaf, Stock(on_hand + count))
}

/// The loaves with none left, in the canonical order of their names: a
/// map is walked through its keys.
function sold_out(stock: Map<Loaf, Stock>) -> List<Loaf> {
    var mut out: List<Loaf> = []
    for loaf in map.keys(stock) {
        if (map.get(stock, loaf) == 0) {
            out = list.append(out, loaf)
        }
    }
    return out
}

/// How many loaves are on the shelves altogether.
function on_shelves(stock: Map<Loaf, Stock>) -> Stock {
    return Stock(list.sum(map.values(stock)))
}

/// The loaves to put out: any with some left, but never `withdrawn`.
function on_show(stock: Map<Loaf, Stock>, withdrawn: Loaf) -> Map<Loaf, Stock> {
    return map.filter(stock, (loaf, count) -> count > 0 and loaf != withdrawn)
}

/// Each loaf's stock once a tray of `per_tray` more arrives for it.
function restocked(stock: Map<Loaf, Stock>, per_tray: Stock) -> Map<Loaf, Stock> {
    return map.map_values(stock, (loaf, count) -> Stock(count + per_tray))
}
```

```jadpo check project=map-operations file=main.test.jadpo
test "map operations give new maps" {
    var no_stock: Map<Loaf, Stock> = []
    var stock: Map<Loaf, Stock> = map.with(map.with(no_stock, "Rye", 4), "Bap", 0)
    assert map.count(no_stock) == 0
    assert map.count(stock) == 2
    assert map.get(stock, "Rye") == 4
    assert map.get(stock, "Spelt") == none
    assert map.has(stock, "Bap")
    assert not map.has(stock, "Spelt")
    assert map.keys(stock) == ["Bap", "Rye"]
    assert map.values(stock) == [0, 4]  // in the canonical order of their keys
    assert map.with(stock, "Rye", 6) == map.with(map.with(no_stock, "Bap", 0), "Rye", 6)
    assert map.without(stock, "Bap") == map.with(no_stock, "Rye", 4)
    assert map.without(stock, "Spelt") == stock
    var fresh: Map<Loaf, Stock> = map.with(no_stock, "Rye", 9)
    assert map.merge(stock, fresh) == map.with(map.with(no_stock, "Bap", 0), "Rye", 9)
    assert delivered(stock, "Bap", 12) == map.with(stock, "Bap", 12)
    assert delivered(stock, "Spelt", 3) == map.with(stock, "Spelt", 3)
    assert sold_out(stock) == ["Bap"]
    assert on_shelves(stock) == 4
    var three: Map<Loaf, Stock> = map.with(stock, "Spelt", 2)
    assert on_show(three, "Spelt") == map.with(no_stock, "Rye", 4)
    assert restocked(stock, 6) == map.with(map.with(no_stock, "Bap", 6), "Rye", 10)
}
```

`map.get` gives `V?`, so a key that may be missing is handled where it is
read; `map.has` says whether it is there without reading it.

## Ranges

`Range<T>` holds a `start` and an `end` of one ordered type, either of
which may be `none` for an open end; `DateRange` includes its end date and
`InstantRange` excludes its end instant, each saying so once in its type.
Stored as PostgreSQL's range types (`daterange`, `tstzrange`,
`numrange`), so the SQL forms are the range operators.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `range.contains(range, value)` | `Bool`: `value` is inside, by the range's own end rule | | `x @> a` |
| `range.overlaps(a, b)` | `Bool`: the two share at least one value | | `a && b` |
| `range.intersection(a, b)` | `Range<T>?`, the shared part; `none` when they don't overlap | | `NULLIF(a * b, 'empty')` |

The three operations take a `Range<T>` or an `InstantRange` (chunk
5.5c2), and a `DateRange` (chunk 5.5k), which holds its end date; a call's ranges are
all one range type, and a range it gives is that type too. An
`InstantRange` holds every instant from its start up to, not including,
its end, so two bookings that meet at the hour don't overlap, and one that
starts and ends together holds nothing:

```jadpo check project=range-operations
type Booking = Object {
    bench: BenchName
    slot: InstantRange
}

type BenchName = Text {
    min_length: 1
}

type BookingTime = Instant {}

type Booked = Bool {}

type DoubleBooked = Bool {}

/// Whether the bench is booked at `at`.
function booked_at(booking: Booking, at: BookingTime) -> Booked {
    return Booked(range.contains(booking.slot, at))
}

/// Whether two bookings of one bench double_booked.
function double_booked(first: Booking, second: Booking) -> DoubleBooked {
    return DoubleBooked(first.bench == second.bench and range.overlaps(first.slot, second.slot))
}

/// The time two bookings share, if any.
function common_time(first: Booking, second: Booking) -> InstantRange? {
    return range.intersection(first.slot, second.slot)
}
```

```jadpo check project=range-operations file=main.test.jadpo
test "ranges include their start and exclude their end" {
    var morning: Booking = Booking {
        bench: "Oven bench"
        slot: InstantRange {
            start: "2026-10-26T09:00:00Z"
            end: "2026-10-26T12:00:00Z"
        }
    }
    var lunch: Booking = Booking {
        bench: "Oven bench"
        slot: InstantRange {
            start: "2026-10-26T11:00:00Z"
            end: "2026-10-26T13:00:00Z"
        }
    }
    var afternoon: Booking = Booking {
        bench: "Oven bench"
        slot: InstantRange {
            start: "2026-10-26T12:00:00Z"
            end: "2026-10-26T17:00:00Z"
        }
    }
    var nothing: Booking = Booking {
        bench: "Oven bench"
        slot: InstantRange {
            start: "2026-10-26T10:00:00Z"
            end: "2026-10-26T10:00:00Z"
        }
    }
    var lunch_overlap: InstantRange = InstantRange {
        start: "2026-10-26T11:00:00Z"
        end: "2026-10-26T12:00:00Z"
    }
    assert booked_at(morning, "2026-10-26T09:00:00Z")
    assert not booked_at(morning, "2026-10-26T12:00:00Z")
    assert double_booked(morning, lunch)
    assert not double_booked(morning, afternoon)  // they meet at noon
    assert not double_booked(morning, nothing)
    assert common_time(morning, lunch) == lunch_overlap
    assert common_time(morning, afternoon) == none
}
```

A `Range<T>` is written `Range { start: ..., end: ... }` where a range is
expected, both ends given, `none` for an open one. It holds its start and
not its end, as an `InstantRange` does, and an open end holds everything
on its side; its canonical text writes an open end `..`, as ISO 8601-2
does (chunk 5.5c2):

```jadpo check project=generic-ranges
type Grams = Int {
    min: 0
}

type LoafName = Text {
    min_length: 1
}

type Loaf = Object {
    name: LoafName
    weight: Grams
}

type InBand = Bool {}

/// The weights sold as large: 800 g and up, with no upper limit.
function large() -> Range<Grams> {
    return Range {
        start: 800
        end: none
    }
}

/// Whether a loaf's weight is in `band`.
function in_band(loaf: Loaf, band: Range<Grams>) -> InBand {
    return InBand(range.contains(band, loaf.weight))
}

/// The weights two bands share, if any.
function common_weights(first: Range<Grams>, second: Range<Grams>) -> Range<Grams>? {
    return range.intersection(first, second)
}
```

```jadpo check project=generic-ranges file=main.test.jadpo
test "a range of any ordered type, either end open" {
    var medium: Range<Grams> = Range {
        start: 400
        end: 800
    }
    var small: Range<Grams> = Range {
        start: none
        end: 500
    }
    var both: Range<Grams> = Range {
        start: 400
        end: 500
    }
    var rye: Loaf = Loaf {
        name: "Rye"
        weight: 800
    }
    assert in_band(rye, large())
    assert not in_band(rye, medium)  // a range doesn't hold its end
    assert common_weights(medium, large()) == none  // they meet at 800
    assert common_weights(medium, small) == both
    assert range.overlaps(small, medium)
    assert text.from(large()) == "800/.."
}
```

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
- **Parsing and formatting are the contract's** (`text.parse(raw, as:
  Quantity)`, `text.format(n, locale:)`), as for every type. Parsing takes
  the wire's form only: no grouping, no locale's decimal comma, no
  exponent. A spreadsheet's `1.234,50` is a cell decoder's job (#88's
  `decode:`).
- **Maths on `Decimal`, not a `Float`** (question 18). Each result is
  correctly rounded to `scale` with the named rounding, by Jadpo's own
  decimal code, so it is the same on every target; a float's `sin` differs
  between platforms' maths libraries, and has `NaN`. Geospatial distance
  and statistics (a standard deviation) are what need them.
- **PostgreSQL's `round` ties away from zero,** which is `half_up`; the
  other modes are spelled out with a `CASE`, never left to the database.
- **`Rounding` is `half_even`, `half_up`, `up` and `down`,** `down`
  (toward zero, SQL form `trunc`) added by question 25; `floor`,
  `ceiling` and `half_down` stay out until amounts below zero need them.

A bakery's numbers, through the operations chunk 5.5d built:

```jadpo check project=number-operations
type Grams = Int {
    min: 0
}

type TrayCount = Int {
    min: 1
    max: 12
}

type LoafCount = Int {
    min: 0
}

type Kilograms = Decimal {
    scale: 3
    min: 0.000
}

type LoafPrice = Decimal {
    scale: 2
    min: 0.00
}

/// How far a weighed batch is from its recipe, either way. `abs` is never
/// below 0, so building `Grams` from it needs no arm.
function drift(weighed: Grams, recipe: Grams) -> Grams {
    return Grams(integer.abs(weighed - recipe))
}

/// The trays to bake: what was ordered, but never more than the oven
/// holds. `min` gives one of its two values as it is, still a `TrayCount`.
function trays_to_bake(requested: TrayCount, oven: TrayCount) -> TrayCount {
    return integer.min(requested, oven)
}

/// Whole loaves a sack of flour makes at 0.450 kg each: part of a loaf
/// isn't one, so the count rounds down, toward zero.
function loaves_from(flour: Kilograms) -> LoafCount {
    return LoafCount(integer.from_decimal(flour / 0.450, rounding: Rounding.down))
}

/// A loaf's price at a fifth off, never below the lowest price.
function sale_price(price: LoafPrice, lowest: LoafPrice) -> LoafPrice {
    return decimal.max(LoafPrice(price * 0.80), lowest)
}

/// A batch's weight to the nearest 100 g, for its label.
function label_weight(weight: Kilograms) -> Kilograms {
    return Kilograms(decimal.round(weight, scale: 1, rounding: Rounding.half_up))
}

/// The price as a customer reads it, in their own locale.
function price_label(price: LoafPrice, customer: Locale) -> PresentationText {
    return text.format(price, locale: customer)
}
```

`min` and `max` take two values of one type, as `+` does: a literal
takes the other's type and is checked against it, so `integer.min(requested,
20)` is an error where `TrayCount` stops at 12, and a plain number, such as
`requested + 1`, makes the result a plain `Int`. A rounding never crosses
zero, so `loaves_from`'s count of flour never below zero is never below
zero either. The page's values, and each mode to one place:

```jadpo check project=number-operations file=main.test.jadpo
test "number operations give what the page says" {
    assert drift(980, 1000) == 20
    assert drift(1020, 1000) == 20
    assert trays_to_bake(10, 8) == 8
    assert trays_to_bake(3, 8) == 3
    assert loaves_from(10.000) == 22
    assert loaves_from(0.449) == 0
    assert sale_price(4.20, 3.50) == 3.50
    assert sale_price(5.00, 3.50) == 4.00
    assert label_weight(1.250) == 1.3
    assert label_weight(1.249) == 1.2
    assert integer.max(3, 8) == 8
    assert decimal.abs(-2.50) == 2.50
    assert decimal.min(4.20, 3.80) == 3.80
    assert price_label(1234.50, Locale.en_gb) == "1,234.50"
    assert price_label(1234.50, Locale.de_de) == "1.234,50"
}

test "every rounding mode, to one place" {
    assert decimal.round(2.25, scale: 1, rounding: Rounding.half_even) == 2.2
    assert decimal.round(2.35, scale: 1, rounding: Rounding.half_even) == 2.4
    assert decimal.round(2.25, scale: 1, rounding: Rounding.half_up) == 2.3
    assert decimal.round(-2.25, scale: 1, rounding: Rounding.half_up) == -2.3
    assert decimal.round(2.21, scale: 1, rounding: Rounding.up) == 2.3
    assert decimal.round(-2.21, scale: 1, rounding: Rounding.up) == -2.3
    assert decimal.round(2.29, scale: 1, rounding: Rounding.down) == 2.2
    assert decimal.round(-2.29, scale: 1, rounding: Rounding.down) == -2.2
    assert integer.from_decimal(2.5, rounding: Rounding.half_even) == 2
    assert integer.from_decimal(-2.5, rounding: Rounding.half_up) == -3
    assert integer.divide(7, 2, rounding: Rounding.down) == 3
}
```

`scale:` that isn't a whole number from 0 to 18 written at the call, as a
type's `scale` is, is `type.mismatch`, and one below 0
`type.argument_negative` (under "Errors").

The maths, from `sqrt` to `pi` (chunk 5.5e), on the bakery's tins,
starters and deliveries. Each result is the true value rounded to
`scale` places by the mode named, so the same call gives the same digits
on every target:

```jadpo check project=number-maths
type Centimetres = Decimal {
    scale: 2
    min: 0.00
}

type SquareCentimetres = Decimal {
    scale: 2
    min: 0.00
}

type Tier = Int {
    min: 0
    max: 6
}

type RecipeScale = Decimal {
    scale: 4
}

type Growth = Decimal {
    scale: 4
    min: 0.0000
}

type Rise = Decimal {
    scale: 6
}

type ProvingTime = Decimal {
    scale: 1
    min: 0.0
}

type Metres = Decimal {
    scale: 1
}

type Course = Decimal {
    scale: 6
}

type NeverRose = Failure {
    kind: FailureKind.invalid_value
}

/// The surface of a round tin, π r².
function tin_area(radius: Centimetres) -> SquareCentimetres {
    return SquareCentimetres(decimal.pi(scale: 6, rounding: Rounding.half_even) * radius * radius)
}

/// The side of a square tin the same size, rounded down so the cake
/// fits. A size is never negative, so its square root needs no check.
function square_side(surface: SquareCentimetres) -> Centimetres {
    return Centimetres(decimal.sqrt(surface, scale: 2, rounding: Rounding.down))
}

/// How much more each tier below the surface needs, at half again a tier:
/// exact, as `1.5 * 1.5 * 1.5` is.
function tier_scale(tier: Tier) -> RecipeScale {
    return RecipeScale(decimal.power(1.5, tier))
}

/// How much a starter grows over `proving`, rising all the while by
/// `rise` an hour. `e` to any power is never below 0, so `Growth` needs no arm.
function risen(rise: Rise, proving: ProvingTime) -> Growth {
    return Growth(decimal.exp(rise * proving, scale: 4, rounding: Rounding.half_even))
}

/// How fast a starter rose an hour, from how much bigger it had `grown`
/// in a day. Growth of 0 has no logarithm: a starter that shrank to
/// nothing never rose.
function hourly_rise(grown: Growth) fails NeverRose -> Rise {
    var daily: Decimal = attempt decimal.ln(grown, scale: 6, rounding: Rounding.half_even) {
        out_of_range: NeverRose
    }
    return Rise(daily / 24.0)
}

/// The direction of a customer at `northing` and `easting` from the
/// bakery, in radians clockwise from north.
function course(northing: Metres, easting: Metres) -> Course {
    return Course(decimal.atan2(easting, northing, scale: 6, rounding: Rounding.half_even))
}

/// Where a van is, east and north of the bakery, `driven` metres on
/// `course`.
function easting_after(driven: Metres, course: Course) -> Metres {
    return Metres(driven * decimal.sin(course, scale: 6, rounding: Rounding.half_even))
}

function northing_after(driven: Metres, course: Course) -> Metres {
    return Metres(driven * decimal.cos(course, scale: 6, rounding: Rounding.half_even))
}
```

`sqrt`'s value and `power`'s exponent are never negative, proved as a
count is (`type.argument_negative`). `ln` fails with `out_of_range`
unless its value is proved above 0, by a literal above 0 or a type whose
`min` is; `Growth` allows 0, so `hourly_rise` maps it. A result past 38
digits, such as `decimal.exp(100.0, ...)`, is a defect, as overflow is
for `*`. The page's values, and each mode on the true value:

```jadpo check project=number-maths file=main.test.jadpo
test "the maths gives what the page says" {
    assert tin_area(10.00) == 314.16
    assert square_side(314.16) == 17.72
    assert tier_scale(3) == 3.375
    assert tier_scale(0) == 1.0
    assert risen(0.100000, 6.0) == 1.8221
    var rise: Rise = attempt hourly_rise(2.0000)
    assert rise == 0.028881
    assert hourly_rise(0.0000) fails NeverRose
    assert course(100.0, 100.0) == 0.785398
    assert course(-100.0, 0.0) == 3.141593
    assert easting_after(1000.0, 0.523599) == 500.0
    assert northing_after(1000.0, 0.523599) == 866.0
}

test "each mode rounds the true value" {
    assert decimal.sqrt(2.0, scale: 3, rounding: Rounding.down) == 1.414
    assert decimal.sqrt(2.0, scale: 3, rounding: Rounding.up) == 1.415
    assert decimal.pi(scale: 4, rounding: Rounding.half_even) == 3.1416
    assert decimal.pi(scale: 4, rounding: Rounding.down) == 3.1415
    assert decimal.exp(1.0, scale: 5, rounding: Rounding.half_up) == 2.71828
    assert decimal.ln(10.0, scale: 5, rounding: Rounding.up) == 2.30259
}
```

## Money and payments

`Money` and `Currency` are standard types (decided with the list,
2026-10-17). Decided with question 16: an amount held as a
whole number of the currency's minor units (pence, cents) and its
`Currency`, never a decimal of pounds. Each currency's exponent comes from
ISO 4217: `jpy` 0, `gbp` 2, `bhd` 3. `Currency` is an enum of the active
ISO 4217 codes, written `Currency.gbp`, shipped with each release.

A bakery prices its cakes in pounds, takes orders from abroad in the
customer's currency, and shares a bill between friends:

```jadpo check project=money
type CakePrice = Money {
    currency: Currency.gbp  // fixed: adding another currency doesn't compile
    min: 0  // in minor units: pence
    rounding: Rounding.half_up  // how `*` and `/` round to a penny
}

type Payment = Money {}  // any currency, known only while running

type Pounds = Decimal {
    scale: 2
}

type Guests = Int {
    min: 1
    max: 12
}

type AmountTooPrecise = Failure {
    kind: FailureKind.invalid_value
}

type MixedCurrencies = Failure {
    kind: FailureKind.invalid_value
}

/// A cake's price, written in pence.
function list_price() -> CakePrice {
    return CakePrice(money.from_minor(1050, Currency.gbp))  // £10.50
}

/// An amount a customer typed, in major units of their currency.
function entered(amount: Pounds, currency: Currency) fails AmountTooPrecise -> Payment {
    var entered_amount: Money = attempt money.of(amount, currency) {
        too_many_places: AmountTooPrecise  // 10.50 in yen
    }
    return Payment(entered_amount)
}

/// A cake delivered: one currency, so a plain `+`.
function delivered(cake: CakePrice, delivery: CakePrice) -> CakePrice {
    return CakePrice(cake + delivery)
}

/// The VAT in a price at 20%, rounded half up to a penny.
function vat_in(total: CakePrice) -> CakePrice {
    return CakePrice(total * 0.2)
}

/// A price before VAT: dividing by a rate.
function before_vat(gross: CakePrice) -> CakePrice {
    return CakePrice(gross / 1.20)
}

/// A bill shared between friends, every penny to someone.
function bill_shares(total: CakePrice, guests: Guests) -> List<Payment> {
    return list.map(money.split(total, guests), share -> Payment(share))
}

/// Payments added up, in whatever currency they share.
function received(first: Payment, second: Payment) fails MixedCurrencies -> Payment {
    var total: Money = attempt first + second {
        currency_mismatch: MixedCurrencies  // a pound and a euro
    }
    return Payment(total)
}

/// A price for people in the reader's locale.
function price_label(cake: CakePrice, customer: Locale) -> PresentationText {
    return text.format(cake, locale: customer)
}
```

```jadpo check project=money file=main.test.jadpo
test "amounts add, round and share in minor units" {
    var delivery: CakePrice = CakePrice(money.from_minor(250, Currency.gbp))
    var gross: CakePrice = CakePrice(money.from_minor(1200, Currency.gbp))
    var bill: CakePrice = CakePrice(money.from_minor(1000, Currency.gbp))
    assert list_price() == money.from_minor(1050, Currency.gbp)
    assert delivered(list_price(), delivery) == money.from_minor(1300, Currency.gbp)
    assert vat_in(list_price()) == money.from_minor(210, Currency.gbp)
    assert before_vat(gross) == money.from_minor(1000, Currency.gbp)
    var pence: List<Int> = list.map(bill_shares(bill, 3), share -> money.minor(share))
    assert pence == [334, 333, 333]
}

test "a currency known only while running is checked as it runs" {
    var in_yen: Payment = attempt entered(500.00, Currency.jpy)
    assert in_yen == money.from_minor(500, Currency.jpy)
    assert entered(10.50, Currency.jpy) fails AmountTooPrecise
    var euros: Payment = Payment(money.from_minor(500, Currency.eur))
    var pounds: Payment = Payment(money.from_minor(250, Currency.gbp))
    var both: Payment = attempt received(euros, euros)
    assert both == money.from_minor(1000, Currency.eur)
    assert received(euros, pounds) fails MixedCurrencies
}

test "an amount is shown, written and read one way" {
    var large: CakePrice = CakePrice(money.from_minor(123450, Currency.gbp))
    var abroad: Payment = Payment(money.from_minor(123450, Currency.eur))
    assert price_label(large, Locale.en_gb) == "£1,234.50"
    assert text.format(abroad, locale: Locale.en_us) == "€1,234.50"
    assert text.format(list_price(), locale: Locale.en_gb, precision: MoneyPrecision.whole) == "£11"
    assert text.from(list_price()) == "GBP 10.50"
    assert text.format(Currency.jpy, locale: Locale.en_gb) == "Japanese Yen"
}
```

Inside `if (money.currency(a) == money.currency(b))` the checker has the
proof, and `+` needs no `attempt`; a currency compared with one written
out (`money.currency(a) == Currency.gbp`) proves that currency, so the
amount builds a type that fixes it with no arm for `currency`:

```jadpo check project=money-proved
type Payment = Money {}

type SterlingPayment = Money {
    currency: Currency.gbp
}

/// Two payments added when they share a currency, the first otherwise.
function sum_if_shared(a: Payment, b: Payment) -> Payment {
    if (money.currency(a) == money.currency(b)) {
        return Payment(a + b)
    }
    return a
}

/// A payment in pounds, when it is one.
function sterling(paid: Payment) -> SterlingPayment? {
    if (money.currency(paid) == Currency.gbp) {
        return SterlingPayment(paid)
    }
    return none
}
```

A shop that takes orders from abroad is given a rate each morning and
prices a cake in the customer's currency. `money.convert` takes the rate
only from the amount's currency; where the checker knows both, a rate the
wrong way round doesn't compile, and otherwise the call fails
`currency_mismatch`, which a condition on `rate.from` proves away:

```jadpo check project=money-rates
type CakePrice = Money {
    currency: Currency.gbp
    min: 0
}

type Payment = Money {}

type WrongRate = Failure {
    kind: FailureKind.invalid_value
}

/// A cake's price in the rate's other currency, at a rate the shop was
/// given; a rate from any other currency fails.
function priced_abroad(cake: CakePrice, rate: ExchangeRate) fails WrongRate -> Payment {
    var charged: Money = attempt money.convert(cake, rate) {
        currency_mismatch: WrongRate  // a rate from euros, the wrong way round
    }
    return Payment(charged)
}

/// The same, with the rate's currency checked first, so it needs no
/// `attempt`.
function priced_if_from_pounds(cake: CakePrice, rate: ExchangeRate) -> Payment? {
    if (rate.from == Currency.gbp) {
        return Payment(money.convert(cake, rate))
    }
    return none
}
```

```jadpo check project=money-rates file=main.test.jadpo
test "a rate converts from its own currency only" {
    var pounds_to_euros: ExchangeRate = ExchangeRate {
        from: Currency.gbp
        to: Currency.eur
        rate: 1.17
        at: "2026-10-18T09:00:00Z"
    }
    var euros_to_pounds: ExchangeRate = ExchangeRate {
        from: Currency.eur
        to: Currency.gbp
        rate: 0.85
        at: "2026-10-18T09:00:00Z"
    }
    var cake: CakePrice = CakePrice(money.from_minor(1050, Currency.gbp))
    var in_euros: Payment = attempt priced_abroad(cake, pounds_to_euros)
    assert in_euros == money.from_minor(1228, Currency.eur)  // €12.285, half to even
    assert priced_abroad(cake, euros_to_pounds) fails WrongRate
    assert priced_if_from_pounds(cake, pounds_to_euros) == money.from_minor(1228, Currency.eur)
    assert priced_if_from_pounds(cake, euros_to_pounds) == none
    assert text.from(pounds_to_euros) == "GBP/EUR 1.17 2026-10-18T09:00:00Z"
}
```

Where the checker knows the rate is from another currency, by a
condition or a rate written out, the call is a type error:

```jadpo check project=money-rate-inverted expect=type.mismatch
type CakePrice = Money {
    currency: Currency.gbp
}

type Payment = Money {}

/// A cake's price at a rate quoted from euros.
function priced_abroad(cake: CakePrice, rate: ExchangeRate) -> Payment? {
    if (rate.from == Currency.eur) {
        return Payment(money.convert(cake, rate))
    }
    return none
}
```

```text output
error: `rate` converts from EUR, and `cake` is in GBP
 --> main.jadpo:10:44
   |
10 |         return Payment(money.convert(cake, rate))
   |                                            ^^^^ a rate from EUR
  reason: `money.convert` applies a rate to an amount in the rate's `from` currency. `cake` is in GBP and `rate` converts from EUR, as their types, how they were made or a condition around the call fix, so the rate is the wrong way round or for another currency.
  next: Pass a rate from GBP, or an amount in EUR (agent)
  rule: type.mismatch
  help: docs/language/02-values-and-types.html#type-mismatch

check failed: 1 error, 1 file
```

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `money.from_minor(amount, currency)` | `Money` of `amount` minor units | | none |
| `money.of(amount, currency)` | `Money` from a `Decimal` in major units | `too_many_places` (more places than the currency has) | none |
| `money.minor(value)` | `Int`, the minor units | | the amount column |
| `money.currency(value)` | `Currency` | | the currency column |
| `money.sum(items, currency:)` | `Money`; `currency:` gives the zero for an empty list | `currency_mismatch` | none |
| `money.allocate(total, ratios)` | `List<Money>` in proportion to `ratios` (whole numbers), adding up to `total` exactly: leftover minor units go one each to the first shares | | none |
| `money.split(total, parts)` | `List<Money>`, `parts` near-equal shares adding up to `total`; `parts` at least 1 | | none |
| `money.convert(value, rate)` | `Money` in the rate's other currency, rounded half to even to its minor unit; `rate` an `ExchangeRate` the app was given | `currency_mismatch` (the rate is for another currency) | none |

- **Arithmetic is the standard operator table's:** `+`, `-` and
  comparisons within one currency (a compile error where both types fix
  it, `currency_mismatch` under `attempt` where it is known only while
  running), `*` by a number or a `Percentage` and `/` by a `Decimal`
  rounded by the type's `rounding:` (`half_even` unless declared), and `/`
  by an `Int` refused in favour of `split` (question 16). Splitting never
  rounds: `allocate` and `split` hand every minor unit to some share.
- **Applying a rate is in; fetching one is out.** `money.convert`
  applies an `ExchangeRate` the app already holds, so an inverted rate is
  a type error, not a silent bug; the rate itself is outside data with a
  time and a source, fetched by a provider package. An `ExchangeRate` is
  an object of `from` and `to` (each a `Currency`), `rate` (a `Decimal`:
  one `from` is `rate` of `to`) and `at` (the `Instant` it was quoted
  at), written `GBP/EUR 1.17 2026-10-18T09:00:00Z` (chunk 5.5h2).
- **Storage** is a `bigint` of minor units and a `char(3)` currency code,
  two columns, also for a type that fixes the currency, so opening a
  type's currency later needs no migration and a query can group by it;
  a type that fixes it adds a check constraint holding the currency
  column to that code (chunk 5.5h).
- **Wire** is one fixed shape, `{"amount": 1050, "currency": "GBP"}`, in
  integer minor units as Stripe sends it; `json.read` and `json.write` use
  it too. A provider's other shape (`"10.50"`) is mapped in its package
  (#88's `wire:`) (question 16).
- **Formatting for people** is the contract's `text.format`, with Money's
  options. It writes the symbol or code, the
  locale's grouping and decimal separator, the currency's places and the
  symbol's position from CLDR: `£1,234.50`, `1 234,50 €`. `precision:
  MoneyPrecision.whole` gives `£1,235`; `display:` is `symbol` (default),
  `code` or `name`; `sign:` is `standard` or `accounting` (`(£5.00)`).
  Inside a translation the same formatter is `{price, number, currency}`,
  amending area 10's list of placeholder styles. Code uses `text.format`;
  messages use the placeholder. ICU4X, already pinned, has the currency
  formatter (`icu_experimental`).
- **What changes in area 2:** `Money` and `Currency` become built-in
  names, so area 2's examples that declare `type Money = Object { amount,
  currency }` (Objects, the type reference, question 12's sample) are
  renamed or use the built-in, in chunk 5.5h. Example 09's `Pence` and its
  percentage discount move to `Money` in 5.7.

An order keeps its total, in pounds, and a deposit in the customer's
currency, each in two columns:

```jadpo check project=money-orders file=app.jadpo hidden
application {}
```

```jadpo check project=money-orders file=domains/bakery/types/cake_price.jadpo hidden
/// What a cake costs, in pounds.
type CakePrice = Money {
    currency: Currency.gbp
    min: 0
}
```

```jadpo check project=money-orders file=domains/bakery/types/deposit.jadpo hidden
/// Money paid ahead, in the customer's currency.
type Deposit = Money {}
```

```jadpo check project=money-orders file=domains/bakery/entities/order.jadpo migrate=initial
use bakery/types { CakePrice, Deposit }

/// A cake ordered for collection.
entity Order {
    id: Uuid
    total: CakePrice  // columns `total_amount` and `total_currency`
    deposit: Deposit?
}
```

```jadpo check project=money-orders file=domains/bakery/actions/place_order.jadpo hidden
use bakery/entities { Order }
use bakery/types { CakePrice }

/// Takes an order for a cake at `total`, with nothing paid yet.
action place_order(total: CakePrice) -> Order {
    return Order.create {
        set: {
            total: total
            deposit: none
        }
    }
}
```

```jadpo check project=money-orders file=domains/bakery/actions/pay_deposit.jadpo hidden
use bakery/entities { Order }
use bakery/types { Deposit }

/// Records a deposit paid on `order`.
action pay_deposit(order: Order, paid: Deposit) -> Order {
    return Order.update(order) {
        set: {
            deposit: paid
        }
    }
}
```

```jadpo check project=money-orders file=domains/bakery/actions/place_order.test.jadpo
use bakery/entities { Order }
use bakery/types { CakePrice, Deposit }
use bakery/actions { pay_deposit }

test "an order keeps its amounts as they were written" {
    var placed: Order = place_order(CakePrice(money.from_minor(4250, Currency.gbp)))
    var order_on_file: Order = attempt Order.get(placed.id)
    assert order_on_file.total == money.from_minor(4250, Currency.gbp)
    assert order_on_file.deposit == none
    var paid: Order = pay_deposit(order_on_file, Deposit(money.from_minor(2000, Currency.eur)))
    var order_after_deposit: Order = attempt Order.get(placed.id)
    assert order_after_deposit.deposit == paid.deposit
    assert order_after_deposit.deposit == Deposit(money.from_minor(2000, Currency.eur))
}
```

> Built in chunk 5.5h: `Money` and `Currency` on the contract, the
> operations but `convert`, the operator table's `Money` rows with
> `currency_mismatch`, `rounding:` and `type.money_split`, the two
> columns, the wire shape and `text.format` with its options; what the
> page left open is "Chosen in chunk 5.5h", under "Review".
> Built in chunk 5.5h2: `ExchangeRate` on the contract, `money.convert`
> with its inverted-rate type error, and `{price, number, currency}` in
> translation messages (area 10); what the pages left open is "Chosen in
> chunk 5.5h2". `Money * Percentage` came with `Percentage` in chunk
> 5.5j2, under "The standard operator table".

## Physical quantities

Each quantity type (`Length`, `Mass`, `Temperature` ... and `DataSize`,
`Bitrate`) holds an exact `Decimal` in its SI base unit at a fixed scale
per kind. The unit is chosen only when a value is made or shown, so a
`Length` is a `Length` whatever unit built it, and `==` and comparisons
work across units: `1km == 1000metre`. Inside a program a quantity has
no unit, so metres plus centimetres is just `Length + Length`; units
exist only at construction and display.

| Kind | Base unit | Scale (smallest step) |
|---|---|---|
| `Length` | metre | 6 (a micrometre) |
| `Area` | square metre | 6 |
| `Volume` | cubic metre | 9 (a cubic millimetre) |
| `Angle` | radian | 9 |
| `Mass` | kilogram | 9 (a microgram) |
| `Pressure` | pascal | 3 |
| `Energy` | joule | 3 |
| `Power` | watt | 3 |
| `Temperature`, `TemperatureDifference` | kelvin | 3 |
| `Speed` | metre per second | 6 |
| `Frequency` | hertz | 3 |
| `Voltage` | volt | 6 |
| `Current` | ampere | 6 |
| `Illuminance` | lux | 3 |
| `SoundLevel` | decibel | 2 |
| `Concentration` | kilogram per cubic metre | 9 |
| `DataSize` | byte | 0 |
| `Bitrate` | bit per second | 0 |

**Making one** always names a unit:

- `units.of(value, unit)` from an `Int` or a `Decimal` and a unit of the
  kind (`units.of(5, LengthUnit.kilometre)`);
- a unit-suffixed literal, `5km`, `250g`, `2.5kg` (question 17);
- `text.parse("5 km", as: Length)`, through the contract, failing
  `unknown_unit` (a unit that isn't the kind's), `malformed` (no number),
  or a rule of the type;
- another value of the same kind, by construction into a named type
  (`RoomWidth(length)`), which checks the type's rules; no conversion is
  needed, since both are already in metres.

**A bare number where a quantity is expected is a compile error,**
`type.unit_required`, whose fix asks for the unit (`5km`, or
`units.of(5, LengthUnit.kilometre)`). It is never declinable: a number
without its unit is the mistake the type exists to prevent. A suffix that
names no unit, or misses one only by case (`5mw`, between `mW` and `MW`),
is `type.unit_unknown`, naming each unit it was near.

**Each kind's units** are an enum named for it, `LengthUnit`, whose
variants are the units' names: `millimetre`, `centimetre`, `metre`,
`kilometre`, `inch`, `foot`, `yard`, `mile`, `nautical_mile` for a
`Length`, and so on for each kind, one variant for each suffix in question
17's table, the cubic metre (`VolumeUnit.cubic_metre`) and a
`TemperatureDifference`'s `celsius`, `fahrenheit` and `kelvin`, which have
no suffix. `text.format(value, locale:, unit:)` shows a quantity in one of
them, its kind's base unit when `unit:` is left out (chunk 5.5j).

A joinery cuts shelves to a customer's length, in whatever unit they give
it, and works out floors and paces from lengths:

```jadpo check project=quantities
type ShelfLength = Length {
    min: 10cm  // a bound written with its unit, held in metres
    max: 3metre
}

type Plank = Length {}

type RoomFloor = Area {}

type WalkingPace = Speed {}

type WalkTime = Duration {
    min: 1s
}

type Miles = Decimal {
    scale: 2
}

type FormText = Text {}

type ShelfUnreadable = Failure {
    kind: FailureKind.invalid_value
}

type ShelfUnitWrong = Failure {
    kind: FailureKind.invalid_value
}

type ShelfOutOfRange = Failure {
    kind: FailureKind.invalid_value
}

/// A shelf with a bracket at each end: lengths add, whatever unit made
/// them.
function with_brackets(shelf: ShelfLength, end_block: Plank) -> Plank {
    return Plank(shelf + end_block * 2)
}

/// A shelf length a customer typed, with its unit: `90 cm`, `3 ft`.
function typed_shelf(
    raw: FormText
) fails ShelfUnreadable, ShelfUnitWrong, ShelfOutOfRange -> ShelfLength {
    var shelf: ShelfLength = attempt text.parse(raw, as: ShelfLength) {
        malformed: ShelfUnreadable  // no number, or no unit
        unknown_unit: ShelfUnitWrong  // `90 kg`
        min: ShelfOutOfRange
        max: ShelfOutOfRange
    }
    return shelf
}

/// A room's floor: a length times a length is an area.
function floor_of(width: Plank, room_depth: Plank) -> RoomFloor {
    return RoomFloor(width * room_depth)
}

/// How fast a walk went: a length over a duration is a speed.
function pace_of(stroll: Plank, stroll_time: WalkTime) -> WalkingPace {
    return WalkingPace(stroll / stroll_time)
}

/// A distance in miles, to two places.
function in_miles(stroll: Plank) -> Miles {
    return Miles(units.in(stroll, LengthUnit.mile, scale: 2, rounding: Rounding.half_even))
}

/// A plank's length for the reader, in centimetres.
function plank_label(plank: Plank, customer: Locale) -> PresentationText {
    return text.format(plank, locale: customer, unit: LengthUnit.centimetre)
}
```

```jadpo check project=quantities file=main.test.jadpo
test "lengths add across units and are shown in any" {
    var fitted: Length = 2metre + 45cm + 3inch
    assert fitted == 2.5262metre
    assert 1km == 1000metre
    assert units.of(5, LengthUnit.kilometre) == 5km
    assert plank_label(Plank(fitted), Locale.en_gb) == "252.62 cm"
    assert plank_label(Plank(fitted), Locale.de_de) == "252,62 cm"
    assert text.format(fitted, locale: Locale.en_gb) == "2.5262 m"
    assert text.from(2.5kg) == "2.5 kg"
    assert in_miles(5km) == 3.11
}

test "kinds multiply and divide as the table says" {
    assert with_brackets(90cm, 2cm) == 94cm
    assert floor_of(3metre, 2metre) == 6sqm
    assert pace_of(100metre, 8s) == 12.5mps
    assert 25degC - 20degC == units.of(5, TemperatureDifferenceUnit.kelvin)
    assert units.sum([1km, 500metre]) == 1.5km
}

test "text is read with its kind's units" {
    var shelf: ShelfLength = attempt typed_shelf("90 cm")
    assert shelf == 0.9metre
    var in_feet: ShelfLength = attempt typed_shelf("3 ft")
    assert in_feet == 0.9144metre
    assert typed_shelf("90") fails ShelfUnreadable
    assert typed_shelf("90 kg") fails ShelfUnitWrong
    assert typed_shelf("5 metre") fails ShelfOutOfRange
}
```

**Arithmetic.** `+` and `-` within one kind; `*` and `/` by a plain number
keep the kind; one kind divided by the same kind gives a plain `Decimal`,
as `/` does for a named type (area 3). These products and quotients are
defined; any other mix is a compile error naming the two kinds:

| Left | Operator | Right | Gives |
|---|---|---|---|
| `Length` | `*` | `Length` | `Area` |
| `Area` | `*` | `Length` | `Volume` |
| `Area` | `/` | `Length` | `Length` |
| `Volume` | `/` | `Area` | `Length` |
| `Volume` | `/` | `Length` | `Area` |
| `Length` | `/` | `Duration` | `Speed` |
| `Speed` | `*` | `Duration` | `Length` |
| `Length` | `/` | `Speed` | `Duration` |
| `Energy` | `/` | `Duration` | `Power` |
| `Power` | `*` | `Duration` | `Energy` |
| `Voltage` | `*` | `Current` | `Power` |
| `Power` | `/` | `Voltage` | `Current` |
| `Power` | `/` | `Current` | `Voltage` |
| `Mass` | `/` | `Volume` | `Concentration` |
| `Concentration` | `*` | `Volume` | `Mass` |
| `DataSize` | `/` | `Duration` | `Bitrate` |
| `Bitrate` | `*` | `Duration` | `DataSize` |
| `Temperature` | `-` | `Temperature` | `TemperatureDifference` |
| `Temperature` | `+` or `-` | `TemperatureDifference` | `Temperature` |

- **Rounding:** a product is exact and a quotient is worked out exactly
  where it ends; either is then rounded half to even to the result kind's
  scale, as constructing into a type with a fixed `scale` rounds
  (area 2). Another rounding is `units.in` with `rounding:`.
- **`Temperature` doesn't add to itself** (20 °C plus 20 °C isn't 40 °C);
  it adds only a `TemperatureDifference`. `SoundLevel` is logarithmic, so
  it compares but has no `+` or `-`.
- **`Duration`** stays area 10's type; it takes part in the table above
  without becoming a quantity.
- **Storage and wire.** One `numeric` column in the base unit at the
  kind's scale, `numeric(38, 6)` for a `Length` (the list's rule). On the
  wire, `{ "value": "2.5", "unit": "kg" }`, the unit named, rather than a
  bare number in the base unit (question 17), and the number a string, as
  area 2 sends every `Decimal` (chunk 5.5j); written in the base unit and
  read in any of the kind's units. `json.read` and `json.write` use it too.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `units.of(value, unit)` | The quantity of `unit`'s kind: `value`, an `Int` or `Decimal`, converted exactly to the base unit and rounded half to even to the kind's scale | overflow is a defect | none |
| `units.in(value, unit, scale:, rounding:)` | `Decimal`, the quantity in `unit`, rounded | | none |
| `units.sum(items)` | The total of one kind; zero for none | overflow is a defect | none |

A shelf on file keeps its length in metres, one column, however it was
cut:

```jadpo check project=quantity-shelves file=app.jadpo hidden
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.de_de]
}
```

```jadpo check project=quantity-shelves file=domains/joinery/types/cut_length.jadpo hidden
/// How long a shelf is cut, held in metres whatever unit it was given in.
type CutLength = Length {
    min: 10cm
    max: 3metre
}
```

```jadpo check project=quantity-shelves file=domains/joinery/entities/cut_shelf.jadpo migrate=initial
use joinery/types { CutLength }

/// A shelf cut to order.
entity CutShelf {
    id: Uuid
    cut_to: CutLength  // one `numeric(38, 6)` column, in metres
}
```

```jadpo check project=quantity-shelves file=domains/joinery/actions/cut_to_order.jadpo hidden
use joinery/entities { CutShelf }
use joinery/types { CutLength }

/// Records a shelf cut to `cut_to`.
action cut_to_order(cut_to: CutLength) -> CutShelf {
    return CutShelf.create {
        set: {
            cut_to: cut_to
        }
    }
}
```

```jadpo check project=quantity-shelves file=domains/joinery/actions/cut_to_order.test.jadpo
use joinery/entities { CutShelf }
use joinery/types { CutLength }

test "a shelf keeps its length as it was cut" {
    var shelf_made: CutShelf = cut_to_order(CutLength(2.5262metre))
    var shelf_on_file: CutShelf = attempt CutShelf.get(shelf_made.id)
    assert shelf_on_file.cut_to == 2metre + 45cm + 3inch
    assert text.format(
        shelf_on_file.cut_to,
        locale: Locale.en_gb,
        unit: LengthUnit.centimetre
    ) == "252.62 cm"
    assert text.format(
        shelf_on_file.cut_to,
        locale: Locale.de_de,
        unit: LengthUnit.centimetre
    ) == "252,62 cm"
}
```

> Built in chunk 5.5j: the 17 quantities with `DataSize` and `Bitrate` on
> the contract, each with its unit enum; suffixed literals with
> `type.unit_required` and `type.unit_unknown`; the product and quotient
> table and `==` and `<` across units; the 3 `units` operations and
> `text.format`'s `unit:`; one `numeric` column, compared and ordered in
> queries; and the wire's `{ "value": ..., "unit": ... }`. What the pages
> left open is "Chosen in chunk 5.5j", under "Review". `Percentage`,
> `Ratio`, `Fraction` and the `%` literal came in chunk 5.5j2, under "The
> standard operator table", with `distance * 15%`.

## Time and calendar additions

Area 10's 21 `temporal` operations stand. The new time types add three.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `temporal.occurrences(rule, from:, count:)` | `List<ZonedTime>`, the next `count` occurrences of a `RecurrenceRule` (RFC 5545) at or after `from`, in the rule's zone; `count` 1 to 1000 | | none |
| `temporal.open_at(hours, at)` | `Bool`: an `OpeningHours` is open at the `Instant`, in its zone, across midnight and its exceptions | | none |
| `temporal.period_between(start, end)` | `CalendarPeriod` in whole years, months and days from one `CalendarDate` to another, so adding it to `start` gives `end`; negative parts when `end` is earlier | | `age(b, a)`, reduced to years, months and days |

`CalendarPeriod` is an amount of calendar time, `P1Y2M10D` in ISO 8601:
whole years, months and days, unlike a `Duration`, which is exact time,
and a `DateRange`, which is a span between two dates. It has no order
(one month and 30 days don't compare) and `==` compares part by part. It
is stored as an `interval` holding only years, months and days.
`CalendarDate + CalendarPeriod` (the operator table) applies years, then
months, then days, and clamps a day past the month's end to its last day,
so 31 January plus one month is 28 or 29 February. That is area 10's
`temporal.add_months` with `InvalidDay.last_valid_day`; an application
that must refuse instead calls `add_months` with `InvalidDay.reject`.
A `ZonedTime` takes no `CalendarPeriod` by operator, since it would also
need a `dst:` choice; it uses `temporal.add_months` and its siblings.

> Built in chunk 5.5k: ten of the 13 new types on the contract, with
> `CalendarPeriod`'s operator rows, `temporal.period_between` and
> `DateRange` in the `range` operations. Built in chunk 5.5k2:
> `RecurrenceRule` with `temporal.occurrences` and `OpeningHours` with
> `temporal.open_at`. Built in chunk 5.5k3: `CalendarEvent`, an object
> whose canonical text is an RFC 5545 `VEVENT`. The choices the page
> didn't make are under "Chosen in chunk 5.5k", "Chosen in chunk 5.5k2"
> and "Chosen in chunk 5.5k3".

| Type | Fields | Canonical text and JSON | Shown in `en_gb` |
|---|---|---|---|
| `LocalTime` | `hour` (0 to 23), `minute`, `second` (0 to 59) | `09:30:00` | by `format:`, a time format: "09:30" |
| `LocalDateTime` | `date: CalendarDate`, `time: LocalTime` | `2026-10-25T09:30:00` | by `format:`, with no zone: "25 Oct 2026, 09:30:00" |
| `Month` (enum) | `january` to `december` | `october` | "October" |
| `YearMonth` | `year` (0 to 9999), `month` (1 to 12) | `2026-10` | "October 2026" |
| `Year` | `year` (0 to 9999) | `2026` | "2026" |
| `Quarter` | `year`, `quarter` (1 to 4) | `2026-Q4` | its canonical text |
| `IsoWeek` | `year`, `week` (1 to 52, or 53) | `2026-W43` | its canonical text |
| `UtcOffset` | `minutes` (-1080 to 1080) | `+05:30` | "GMT+05:30" |
| `DateRange` | `start`, `end`, both `CalendarDate`, the end included | `2026-10-01/2026-10-31`; JSON `{ "start", "end" }` | each end by `format:`, a date format |
| `CalendarPeriod` | `years`, `months`, `days` | `P1Y2M10D` | "1 year, 2 months, 10 days" |
| `RecurrenceRule` | held as its text (RFC 5545): the first occurrence as a local time in its zone, a line feed, the rule | `DTSTART;TZID=Europe/London:20261026T093000` and `RRULE:FREQ=WEEKLY;BYDAY=MO`, on two lines | as written |
| `OpeningHours` | held as its text (OpenStreetMap's `opening_hours`) and its zone | `Mo-Fr 09:00-17:30; Sa 10:00-16:00 [Europe/London]` | as written |
| `CalendarEvent` | `summary`; an all-day event's `first_day` and `last_day`, both included, or a timed one's `start` and `end` (`LocalDateTime`) and `zone`; `location`, `description` and `recurrence` (`RecurrenceRule`), each may be `none` | an RFC 5545 `VEVENT`, an all-day `DTEND` the day after the last; JSON its parts | as written |

A rental runs for a calendar period from its first day, and how long
someone has lived somewhere is a period between two days, whatever the
clocks did in between:

```jadpo check project=calendar
type RentalDay = CalendarDate {}

type SeenAt = Instant {}

type Rental = Object {
    first_day: RentalDay
    rental_term: CalendarPeriod
}

/// The day a rental ends: its rental_term on from the day it first_day, a day past
/// a month's end becoming that month's last day.
function rental_end(rental: Rental) -> RentalDay {
    return rental.first_day + rental.rental_term
}

/// The day it is in `zone` at `at`.
function local_day(at: SeenAt, zone: Zone) -> RentalDay {
    return RentalDay(temporal.date(temporal.in_zone(at, zone)))
}

/// How long someone has lived somewhere, in whole years, months and
/// days, from the day they moved in to `checked_on`.
function time_lived(moved_in: RentalDay, checked_on: RentalDay) -> CalendarPeriod {
    return temporal.period_between(moved_in, checked_on)
}

type InRental = Bool {}

/// Whether `day` is one of the days a rental in_rental, its last included.
function in_rental(days: DateRange, day: RentalDay) -> InRental {
    return InRental(range.contains(days, day))
}
```

```jadpo check project=calendar file=main.test.jadpo
test "a period counts calendar days, whatever the clocks do" {
    var london_spring: RentalDay = local_day("2026-03-29T00:30:00Z", Zone.europe_london)
    var london_autumn: RentalDay = local_day("2026-10-25T23:30:00Z", Zone.europe_london)
    assert london_spring == RentalDay("2026-03-29")
    assert london_autumn == RentalDay("2026-10-25")
    assert time_lived(london_spring, london_autumn) == CalendarPeriod {
        years: 0
        months: 6
        days: 26
    }
    assert rental_end(
        Rental {
            first_day: london_spring
            rental_term: time_lived(london_spring, london_autumn)
        }
    ) == london_autumn
    var new_york_spring: RentalDay = local_day("2026-03-08T07:30:00Z", Zone.america_new_york)
    var new_york_autumn: RentalDay = local_day("2026-11-02T03:30:00Z", Zone.america_new_york)
    assert new_york_autumn == RentalDay("2026-11-01")
    assert text.from(time_lived(new_york_spring, new_york_autumn)) == "P7M24D"
    assert text.from(time_lived(new_york_autumn, new_york_spring)) == "P-7M-24D"
}

test "a month on from the 31st is the month's last day" {
    var rental: Rental = Rental {
        first_day: "2026-01-31"
        rental_term: CalendarPeriod {
            years: 0
            months: 1
            days: 0
        }
    }
    assert rental_end(rental) == "2026-02-28"
    assert rental.rental_term * 12 + rental.rental_term == CalendarPeriod {
        years: 0
        months: 13
        days: 0
    }
    var days: DateRange = DateRange {
        start: "2026-01-31"
        end: "2026-02-28"
    }
    assert in_rental(days, "2026-02-28")
    assert not in_rental(days, "2026-03-01")
    assert text.format(rental.rental_term * 3, locale: Locale.en_gb) == "3 months"
}
```

A class repeats every Monday at 09:30 in London, whatever the clocks do,
and a bar's Friday and Saturday nights run past midnight. A recurrence
keeps its local time, leaves out one the clocks skip without counting it,
and takes the first of one they show twice (RFC 5545); opening hours are
read by the clock on the wall:

```jadpo check project=schedules
type BarHours = OpeningHours {}

type CheckedAt = Instant {}

type ClassCount = Int {
    min: 1
    max: 10
}

type ClassSchedule = RecurrenceRule {}

type ClassTime = ZonedTime {}

type IsOpen = Bool {}

type SearchFrom = Instant {}

/// Whether a bar is open at `at`, by the clock in its zone.
function is_open(hours: BarHours, at: CheckedAt) -> IsOpen {
    return IsOpen(temporal.open_at(hours, at))
}

/// The next `count` classes on a timetable from `from`, each at its local
/// time in the timetable's zone.
function next_classes(
    timetable: ClassSchedule,
    from: SearchFrom,
    count: ClassCount
) -> List<ClassTime> {
    return list.map(temporal.occurrences(timetable, from: from, count: count), at -> ClassTime(at))
}
```

```jadpo check project=schedules file=main.test.jadpo
test "a class keeps its local time when the clocks change" {
    var monday_classes: List<ClassTime> = next_classes(
        "DTSTART;TZID=Europe/London:20261019T093000\nRRULE:FREQ=WEEKLY;BYDAY=MO",
        "2026-10-19T00:00:00Z",
        3
    )
    assert list.map(monday_classes, at -> temporal.instant(at)) == [
        Instant("2026-10-19T08:30:00Z"),
        Instant("2026-10-26T09:30:00Z"),
        Instant("2026-11-02T09:30:00Z")
    ]
    var gap_days: List<ClassTime> = next_classes(
        "DTSTART;TZID=America/New_York:20260307T023000\nRRULE:FREQ=DAILY;COUNT=3",
        "2026-03-01T00:00:00Z",
        10
    )
    assert list.map(gap_days, at -> temporal.instant(at)) == [
        Instant("2026-03-07T07:30:00Z"),
        Instant("2026-03-09T06:30:00Z"),
        Instant("2026-03-10T06:30:00Z")
    ]
    var fold_days: List<ClassTime> = next_classes(
        "DTSTART;TZID=America/New_York:20261101T013000\nRRULE:FREQ=DAILY",
        "2026-11-01T00:00:00Z",
        2
    )
    assert list.map(fold_days, at -> temporal.instant(at)) == [
        Instant("2026-11-01T05:30:00Z"),
        Instant("2026-11-02T06:30:00Z")
    ]
}

test "a night past midnight is open by the clock on the wall" {
    var pub_hours: BarHours = "Mo-Th 17:00-23:00; Fr,Sa 17:00-02:00; 2026 Dec 25 off [Europe/London]"
    assert is_open(pub_hours, "2026-10-25T00:30:00Z")
    assert is_open(pub_hours, "2026-10-25T01:30:00Z")
    assert not is_open(pub_hours, "2026-10-25T02:00:00Z")
    assert is_open(pub_hours, "2026-03-29T00:59:00Z")
    assert not is_open(pub_hours, "2026-03-29T01:00:00Z")
    assert not is_open(pub_hours, "2026-12-25T18:00:00Z")
    var club_hours: BarHours = "Mo-Th 17:00-23:00; Fr,Sa 17:00-02:00 [America/New_York]"
    assert is_open(club_hours, "2026-03-08T06:30:00Z")
    assert not is_open(club_hours, "2026-03-08T07:00:00Z")
    assert is_open(club_hours, "2026-11-01T06:30:00Z")
    assert not is_open(club_hours, "2026-11-01T07:00:00Z")
}
```

A calendar event is all-day or timed. An all-day event holds its first
and last days, both included, and no zone: a holiday on 25 December is on
25 December wherever it is read, so it never moves a day for a reader in
Kiritimati, fourteen hours ahead of UTC, or Pago Pago, eleven behind. A
timed event holds its local start and end and the zone they are in, as a
recurrence does, so a shift that starts at 22:00 keeps starting at 22:00
when the clocks change. Its text is RFC 5545's `VEVENT`, where an all-day
event's end is the day after its last, and its JSON is its parts:

```jadpo check project=calendar-events
type DiaryText = Text {}

type DiaryJson = Text {}

type DiaryUnreadable = Failure {
    kind: FailureKind.invalid_value
}

type ReaderDay = CalendarDate {}

type SeenAt = Instant {}

type CoversReaderDay = Bool {}

type SearchFrom = Instant {}

type ShiftCount = Int {
    min: 1
    max: 10
}

/// An event read back from the text calendars exchange.
function diary_entry_from_text(calendar_text: DiaryText) fails DiaryUnreadable -> CalendarEvent {
    return attempt text.parse(calendar_text, as: CalendarEvent) {
        malformed: DiaryUnreadable
    }
}

/// An event read back from its JSON.
function diary_entry_from_json(sent: DiaryJson) fails DiaryUnreadable -> CalendarEvent {
    return attempt json.read(sent, as: CalendarEvent) {
        malformed: DiaryUnreadable
        mismatch: DiaryUnreadable
    }
}

/// Whether an all-day event covers the day it is for someone in `zone` at
/// `at`: its own days, whichever zone they are read in. A timed event has
/// no days of its own.
function covers_reader_day(event: CalendarEvent, at: SeenAt, zone: Zone) -> CoversReaderDay {
    var reader_local_day: ReaderDay = ReaderDay(temporal.date(temporal.in_zone(at, zone)))
    return match event.first_day {
        some(first) -> match event.last_day {
            some(last) -> CoversReaderDay(
                range.contains(
                    DateRange {
                        start: first
                        end: last
                    },
                    reader_local_day
                )
            )
            none -> CoversReaderDay(false)
        }
        none -> CoversReaderDay(false)
    }
}

/// When the next `count` of a repeating event start, at its local time in
/// its zone; none for one that doesn't repeat.
function next_shift_starts(
    event: CalendarEvent,
    from: SearchFrom,
    count: ShiftCount
) -> List<SeenAt> {
    return match event.recurrence {
        some(rule) -> list.map(
            temporal.occurrences(rule, from: from, count: count),
            at -> SeenAt(temporal.instant(at))
        )
        none -> []
    }
}
```

```jadpo check project=calendar-events file=main.test.jadpo
test "an all-day event keeps its days, wherever and whenever it is read" {
    var half_term: CalendarEvent = CalendarEvent {
        summary: "Half-term walk"
        first_day: "2026-10-24"
        last_day: "2026-10-26"
        start: none
        end: none
        zone: none
        location: "Keswick, Cumbria"
        description: none
        recurrence: none
    }
    var half_term_vevent: DiaryText = DiaryText(text.from(half_term))
    assert half_term_vevent == "BEGIN:VEVENT\nDTSTART;VALUE=DATE:20261024\nDTEND;VALUE=DATE:20261027\nSUMMARY:Half-term walk\nLOCATION:Keswick\\, Cumbria\nEND:VEVENT"
    var half_term_from_text: CalendarEvent = attempt diary_entry_from_text(half_term_vevent)
    assert half_term_from_text == half_term
    var half_term_json: DiaryJson = DiaryJson(json.write(half_term))
    assert half_term_json == "{\"summary\":\"Half-term walk\",\"first_day\":\"2026-10-24\",\"last_day\":\"2026-10-26\",\"start\":null,\"end\":null,\"zone\":null,\"location\":\"Keswick, Cumbria\",\"description\":null,\"recurrence\":null}"
    var half_term_from_json: CalendarEvent = attempt diary_entry_from_json(half_term_json)
    assert half_term_from_json == half_term
    assert covers_reader_day(half_term, "2026-10-26T10:30:00Z", Zone.pacific_pago_pago)
    assert not covers_reader_day(half_term, "2026-10-26T10:30:00Z", Zone.pacific_kiritimati)
    assert covers_reader_day(half_term, "2026-10-23T10:30:00Z", Zone.pacific_kiritimati)
    assert not covers_reader_day(half_term, "2026-10-23T10:30:00Z", Zone.pacific_pago_pago)
    assert covers_reader_day(half_term, "2026-10-25T01:30:00Z", Zone.europe_london)
}

test "a timed event runs past midnight and repeats at its local time" {
    var night_shift: CalendarEvent = CalendarEvent {
        summary: "Night shift"
        first_day: none
        last_day: none
        start: LocalDateTime {
            date: "2026-10-24"
            time: LocalTime {
                hour: 22
                minute: 0
                second: 0
            }
        }
        end: LocalDateTime {
            date: "2026-10-25"
            time: LocalTime {
                hour: 2
                minute: 0
                second: 0
            }
        }
        zone: Zone.europe_london
        location: none
        description: "Keys at the front desk; sign in, then out"
        recurrence: "DTSTART;TZID=Europe/London:20261024T220000\nRRULE:FREQ=WEEKLY;COUNT=4"
    }
    var shift_vevent: DiaryText = DiaryText(text.from(night_shift))
    assert shift_vevent == "BEGIN:VEVENT\nDTSTART;TZID=Europe/London:20261024T220000\nDTEND;TZID=Europe/London:20261025T020000\nSUMMARY:Night shift\nDESCRIPTION:Keys at the front desk\\; sign in\\, then out\nRRULE:FREQ=WEEKLY;COUNT=4\nEND:VEVENT"
    var shift_from_text: CalendarEvent = attempt diary_entry_from_text(shift_vevent)
    assert shift_from_text == night_shift
    var shift_from_json: CalendarEvent = attempt diary_entry_from_json(
        DiaryJson(json.write(night_shift))
    )
    assert shift_from_json == night_shift
    assert next_shift_starts(night_shift, "2026-10-01T00:00:00Z", 3) == [
        SeenAt("2026-10-24T21:00:00Z"),
        SeenAt("2026-10-31T22:00:00Z"),
        SeenAt("2026-11-07T22:00:00Z")
    ]
    assert diary_entry_from_text(
        "BEGIN:VEVENT\nDTSTART;VALUE=DATE:20261024\nDTEND;VALUE=DATE:20261024\nSUMMARY:Half-term walk\nEND:VEVENT"
    ) fails DiaryUnreadable
    assert diary_entry_from_json(
        "{\"summary\":\"Night shift\",\"first_day\":\"2026-10-24\",\"last_day\":\"2026-10-26\",\"start\":null,\"end\":null,\"zone\":\"Europe/London\",\"location\":null,\"description\":null,\"recurrence\":null}"
    ) fails DiaryUnreadable
}
```

An event is stored a column per part, its days `date` columns, so a stored
event comes back as it was written:

```jadpo check project=diary-bookings file=app.jadpo hidden
application {}
```

```jadpo check project=diary-bookings file=domains/diary/entities/booking.jadpo migrate=initial
/// A slot in someone's diary: a day off or a shift.
entity Booking {
    id: Uuid
    slot: CalendarEvent  // a column per part, its days `date`s
}
```

```jadpo check project=diary-bookings file=domains/diary/actions/book.jadpo hidden
use diary/entities { Booking }

/// Books a slot in the diary.
action book(slot: CalendarEvent) -> Booking {
    return Booking.create {
        set: {
            slot: slot
        }
    }
}
```

```jadpo check project=diary-bookings file=domains/diary/actions/book.test.jadpo
use diary/entities { Booking }

test "an event is kept part by part" {
    var half_term: CalendarEvent = CalendarEvent {
        summary: "Half-term walk"
        first_day: "2026-10-24"
        last_day: "2026-10-26"
        start: none
        end: none
        zone: none
        location: "Keswick, Cumbria"
        description: none
        recurrence: none
    }
    var half_term_booked: Booking = book(half_term)
    var half_term_kept: Booking = attempt Booking.get(half_term_booked.id)
    assert half_term_kept.slot == half_term
    assert text.from(half_term_kept.slot) == text.from(half_term)
    var night_shift: CalendarEvent = CalendarEvent {
        summary: "Night shift"
        first_day: none
        last_day: none
        start: LocalDateTime {
            date: "2026-10-24"
            time: LocalTime {
                hour: 22
                minute: 0
                second: 0
            }
        }
        end: LocalDateTime {
            date: "2026-10-25"
            time: LocalTime {
                hour: 2
                minute: 0
                second: 0
            }
        }
        zone: Zone.europe_london
        location: none
        description: "Keys at the front desk"
        recurrence: "DTSTART;TZID=Europe/London:20261024T220000\nRRULE:FREQ=WEEKLY;COUNT=4"
    }
    var shift_booked: Booking = book(night_shift)
    var shift_kept: Booking = attempt Booking.get(shift_booked.id)
    assert shift_kept.slot == night_shift
}
```

## Text

Each operation is called as `text.<name>`. Text is counted, cut and
padded in Unicode code points, as `max_length`
counts it (area 2) and as PostgreSQL's `char_length` does (question 23).
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
| `parse(raw, as: T)` | `T`, any standard type or type built on a representation, from its canonical text (the contract) | `malformed`, or the rule it breaks | none |
| `format(value, locale:)` | `PresentationText` for people, any standard type, with the type's own options (`unit:`, `precision:`) (the contract) | | none |
| `pick(value, locale:)` | `Text` from a `LocalisedText`: the locale's entry, else its language's, else the default | | none |
| `slug(value)` | `Text` of lowercase ASCII letters, digits and single `-`: accents dropped, every other run of characters one `-`, none at either end; may be empty | | none |

- **Matching is a type's `pattern`.** Whether text looks like a postcode
  is a construction, `attempt Postcode(raw) { pattern: ... }` (area 2), so
  there is no `text.matches`, and no replacing or capturing by regular
  expression: a pattern is a type's rule, checked in linear time.
- **Joining two pieces is `+`** (area 3); `join` is for a list.
- **`from` takes no enum an application declares, and no text.** Such an
  enum has no display label (area 2), so showing one is a `match`; a value
  built on `Text` already is text. A standard enum's `from` is its code
  (`GB`) and its `format` its CLDR name (question 22).
- **`keep_first` can split a character a person sees as one,** such as an
  emoji with a skin tone, which is two code points. A preview cut this way
  is still valid text; graphemes are left out (question 23).
- **Left out by design:** `trim_start`, `trim_end` and `pad_end` (no
  scenario needs them, and each is a second way near one that exists);
  iterating characters (every job the scenarios found, slugs, masks,
  previews, case, is an operation, and character loops are where combining
  marks break). Unicode normalisation is a boundary rule, not a call
  ("Language gaps").
- **Text from outside is NFC.** Every text a request carries, in its body,
  path, query string or headers, is composed to Unicode's NFC as it is
  decoded, before its rules are checked, so `café` typed as `e` and a
  combining accent equals `café` typed as one character, and `max_length`
  counts the code points it is kept as (areas 2 and 7). Text written in
  code is kept as written.
- **Checked while compiling.** `replace`'s `find` and `split`'s
  `separator` are never empty (`never_empty`: a literal isn't `""` and a
  value's type has a `min_length` of at least 1, `type.argument_empty`
  under "Errors"); counts and positions are never negative, as for lists;
  `fill:` is one character written out. `trim`, `slice` and the four cuts
  are never longer than their text (`at_most_length_of`, question 29), so
  a type built from one keeps the text's `max_length` without an arm;
  `lower` and `upper` can lengthen it (`ß` becomes `SS`), so they don't.
- **`locale:`** maps case by a locale's rules (`i` becomes `İ` in Turkish)
  and orders `list.sort_by`'s text keys as the locale's alphabet does (`Ö`
  after `Z` in Swedish, beside `O` in German), through ICU4X's case mapping
  and collation; without it, case is Unicode's default and text sorts by
  code point, as queries sort it (question 26).

A bakery's product names, through every text operation (chunk 5.5f):

```jadpo check project=text-operations
type ProductName = Text {
    min_length: 1
    max_length: 60
}

type Caption = Text {
    max_length: 60
}

type TagName = Text {
    min_length: 1
    max_length: 20
}

type Divider = Text {
    min_length: 1
    max_length: 3
}

type ProductSlug = Text {}

type ShelfLabel = Text {}

type TagLine = Text {}

type OrderReference = Text {}

type OrderNumber = Int {
    min: 0
}

type PieceCount = Int {
    min: 0
}

type Product = Object {
    name: ProductName
    tags: List<TagName>
}

type NameBlank = Failure {
    kind: FailureKind.invalid_value
}

/// A product name as it was typed, without the spaces around it. Trimming
/// never lengthens text, so `max_length` can't break; it can leave
/// nothing, so `min_length` can.
function tidy(typed_name: ProductName) fails NameBlank -> ProductName {
    return attempt ProductName(text.trim(typed_name)) {
        min_length: NameBlank
    }
}

/// The first twelve code points of a name, for a narrow card: never
/// longer than the name, so building `Caption` needs no arm.
function caption(name: ProductName) -> Caption {
    return Caption(text.keep_first(name, 12))
}

/// The name in a web address: lowercase ASCII letters, digits and `-`.
function product_slug(name: ProductName) -> ProductSlug {
    return ProductSlug(text.slug(name))
}

/// The name for a shelf edge, in capitals by the shopper's own rules.
function shelf_label(name: ProductName, shopper: Locale) -> ShelfLabel {
    return ShelfLabel(text.upper(name, locale: shopper))
}

/// An order's number as six digits, `42` written `000042`.
function order_reference(number: OrderNumber) -> OrderReference {
    return OrderReference(text.pad_start(text.from(number), 6, fill: "0"))
}

/// A product's tags on one line.
function tag_line(product: Product) -> TagLine {
    return TagLine(text.join(product.tags, ", "))
}

/// How many pieces a line holds between separators. A separator is
/// never empty, which its type's `min_length` proves.
function count_pieces(line: ProductName, spacer: Divider) -> PieceCount {
    return PieceCount(list.count(text.split(line, spacer)))
}

/// The products in the order the shopper's alphabet puts their names.
function in_name_order(products: List<Product>, shopper: Locale) -> List<Product> {
    return list.sort_by(products, product -> product.name, locale: shopper)
}
```

Text is counted and cut in code points, so a mark written on its own and
the two halves of an emoji with a skin tone each count as one; text in
another script is cut the same way.

```jadpo check project=text-operations file=main.test.jadpo
test "text operations give what the page says" {
    var tidied: ProductName = attempt tidy("  Rye loaf ")
    assert tidied == "Rye loaf"
    assert tidy("   ") fails NameBlank
    assert caption("Seeded sourdough boule") == "Seeded sourd"
    assert product_slug("Crème Brûlée & Co.") == "creme-brulee-co"
    assert order_reference(42) == "000042"
    assert order_reference(1234567) == "1234567"  // never cut
    assert text.length("Rye") == 3
    assert text.lower("RYE Loaf") == "rye loaf"
    assert text.contains("Rye loaf", "loaf")
    assert text.starts_with("Rye loaf", "Rye")
    assert text.ends_with("Rye loaf", "loaf")
    assert not text.ends_with("Rye loaf", "Rye")
    assert text.replace("salt & pepper & oil", " & ", ", ") == "salt, pepper, oil"
    assert text.split("flour,sugar,,eggs", ",") == ["flour", "sugar", "", "eggs"]
    assert text.split("", ",") == [""]
    assert text.join(["flour", "sugar"], " and ") == "flour and sugar"
    assert text.slice("sourdough", 4, 3) == "dou"
    assert text.slice("sourdough", 7, 10) == "gh"  // cut short at the end
    assert text.keep_last("sourdough", 5) == "dough"
    assert text.drop_first("sourdough", 4) == "dough"
    assert text.drop_last("sourdough", 5) == "sour"
    assert text.drop_last("rye", 9) == ""
}

test "text is counted and cut in code points" {
    assert text.length("café") == 4  // é as one code point
    assert text.length("café") == 5  // e and a combining acute: two
    assert text.drop_last("café", 1) == "cafe"  // the mark goes on its own
    assert text.length("面包") == 2
    assert text.keep_first("面包店", 2) == "面包"
    assert text.length("👍🏽") == 2  // a thumb and a skin tone
    assert text.keep_first("👍🏽", 1) == "👍"
    assert text.trim("\t rye\n") == "rye"
    assert text.upper("straße") == "STRASSE"  // longer than it was
    assert product_slug("面包 Bakery") == "bakery"
}

test "a locale's case and order" {
    assert text.upper("istanbul") == "ISTANBUL"
    assert text.upper("istanbul", locale: Locale.tr_tr) == "İSTANBUL"
    assert text.lower("DİYARBAKIR", locale: Locale.tr_tr) == "diyarbakır"
    assert shelf_label("içli köfte", Locale.tr_tr) == "İÇLİ KÖFTE"
    var oat: Product = Product {
        name: "Oat"
        tags: []
    }
    var zopf: Product = Product {
        name: "Zopf"
        tags: ["Swiss", "plaited"]
    }
    var oland: Product = Product {
        name: "Ölandsbröd"
        tags: []
    }
    assert list.sort_by([zopf, oland, oat], product -> product.name) == [oat, zopf, oland]  // by code point
    assert in_name_order([zopf, oland, oat], Locale.de_de) == [oat, oland, zopf]
    assert in_name_order([zopf, oland, oat], Locale.sv_se) == [oat, zopf, oland]
    assert tag_line(zopf) == "Swiss, plaited"
    assert count_pieces("Swiss,plaited", ",") == 2
}
```

`text` is a standard-library namespace from this chunk on, so it names no
variable, parameter or declaration, and `text.` always means the standard
library (question 30); a field or a named argument may still be called
`text`.

```jadpo check project=e-text-reserved expect=semantic.standard_namespace_reserved
type Title = Text {
    min_length: 1
    max_length: 80
}

type PosterLine = Text {}

/// A recipe's title, kept for the poster.
function poster_line(title: Title) -> PosterLine {
    var text: PosterLine = PosterLine(title)
    return text
}
```

```text output
error: `text` is a standard-library namespace
 --> main.jadpo:10:9
   |
10 |     var text: PosterLine = PosterLine(title)
   |         ^^^^ reserved
  reason: Standard-library namespaces can't name a declaration, domain, library or variable, so `text.` always means the standard library.
  next: Choose another name than `text` (agent)
  rule: semantic.standard_namespace_reserved
  help: docs/language/01-project-layout-and-modules.html#semantic-standard-namespace-reserved

check failed: 1 error, 1 file
```

## Text formats

`Html` is always safe: its construction from text keeps only the allowed
elements and attributes (no scripts, no event handlers, links only to
`https:` and `mailto:`), the security boundary #88 asks for. `Markdown` is
CommonMark text; `RichText` is a structured document of blocks and marks;
`LocalisedText` maps locales to text and is read with `text.pick`.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `markup.to_html(markdown)` | `Html`, the CommonMark rendered, then made safe as any `Html` is | | none |
| `markup.rich_to_html(rich)` | `Html` from a `RichText` | | none |
| `markup.to_plain_text(html)` | `Text`, the text content, with a new line between blocks | | none |

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
| `crypto.sign(value, key:)` | `Bytes`, an Ed25519 signature; `key:` a `PrivateKey` setting | | none |
| `crypto.verify_signature(value, signature, key)` | `Bool`; `key` a `PublicKey`, which is not secret and may be read | | none |
| `crypto.verify_jwt(token, keys:)` | `JwtClaims`, checked against a set of `Jwk`s, each key's own algorithm only (never `none`, never chosen by the token), with expiry and not-before against `clock.now` | `invalid_signature`, `expired`, `not_yet_valid`, `malformed` | none |
| `crypto.verify_totp(code, secret)` | `Bool` for a `OneTimeCode` against a `TotpSecret`, RFC 6238 with one step either side. Waits for keys held as data | | none |

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
  `encoding.to_hex` for an ETag or a content key (question 24).
- **No MD5 or SHA-1.** Both are broken; an outside system that still asks
  for one is reached through a package (area 11). That includes TOTP,
  whose RFC 6238 default is HMAC-SHA-1 and whose per-person secret is a
  key held as data ("Language gaps").
- **Encrypted fields** (question 10): `notes: ClinicalNote { encrypted:
  true }` on an entity would store the field sealed with the
  application's `field_encryption_key:`, decrypt it on load, and keep it
  out of unique rules, indexes, filters, ordering, search and logs, as
  area 16's table already says of field encryption. One key for the
  application, never one per person: area 15 decided against per-person
  keys (question 5 there), and this keeps that.

## Randomness

`random` is a generated namespace like `clock` (question 19): it reads
the operating system's cryptographic generator in production, is called
only in actions, gives the same values on every rerun of a test (seeded
per test, as the test clock is fixed), and is recorded for replay as
area 17 already lists randomness among the effects.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `random.token()` | `Text`, 32 random bytes as Base64url, 43 characters: for links and keys a person never types | | none |
| `random.code(length)` | `Text` of `length` characters from Crockford's Base32 (digits and capitals without `I`, `L`, `O`, `U`): booking and order references a person reads out; `length` 4 to 32 | | none |
| `random.integer(min, max)` | `Int`, uniformly from `min` to `max`, both included; `min` at most `max` | | none |
| `random.shuffle(items)` | `List<T>`, the items in random order | | none |
| `random.sample(items, count)` | `List<T>`, `count` items from different positions, in random order; all of them when `count` is larger | | none |

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

A bakery's photos, attachments and links, through `hash.sha256` and each
encoding (chunk 5.5g). A digest is `Bytes`, written as text by
`encoding`; writing never fails, and reading fails `malformed` for text
that isn't the encoding at all, reading only the one form writing gives:

```jadpo check project=hashing-encodings
type PhotoBytes = Bytes {
    max_length: 5000000
}

type PhotoKey = Text {}

type ShareCode = Text {}

type SentAttachment = Text {
    max_length: 7000000
}

type RecipeTitle = Text {
    max_length: 100
}

type LinkPart = Text {}

type AttachmentUnreadable = Failure {
    kind: FailureKind.invalid_value
}

type AttachmentTooLarge = Failure {
    kind: FailureKind.invalid_value
}

/// A photo's content key: the same bytes always give the same 64
/// lowercase hexadecimal digits.
function photo_key(photo: PhotoBytes) -> PhotoKey {
    return PhotoKey(encoding.to_hex(hash.sha256(photo)))
}

/// A short code for a recipe, from its title's UTF-8, safe in a URL.
function share_code(title: RecipeTitle) -> ShareCode {
    return ShareCode(encoding.to_base64url(hash.sha256(title)))
}

/// An attachment a client sends as Base64 text, as its bytes.
function attachment(
    sent: SentAttachment
) fails AttachmentUnreadable, AttachmentTooLarge -> PhotoBytes {
    var unpacked: Bytes = attempt encoding.from_base64(sent) {
        malformed: AttachmentUnreadable
    }
    return attempt PhotoBytes(unpacked) {
        max_length: AttachmentTooLarge
    }
}

/// A photo sent back as Base64 text.
function sent_photo(photo: PhotoBytes) -> LinkPart {
    return LinkPart(encoding.to_base64(photo))
}

/// A photo's key read back as its 32 bytes, in either case.
function key_bytes(key: PhotoKey) fails AttachmentUnreadable, AttachmentTooLarge -> PhotoBytes {
    var unpacked: Bytes = attempt encoding.from_hex(key) {
        malformed: AttachmentUnreadable
    }
    return attempt PhotoBytes(unpacked) {
        max_length: AttachmentTooLarge
    }
}

/// A share code read back as the digest it was written from.
function code_digest(code: ShareCode) fails AttachmentUnreadable, AttachmentTooLarge -> PhotoBytes {
    var unpacked: Bytes = attempt encoding.from_base64url(code) {
        malformed: AttachmentUnreadable
    }
    return attempt PhotoBytes(unpacked) {
        max_length: AttachmentTooLarge
    }
}

/// A title through its UTF-8 bytes and back; bytes that aren't UTF-8 are
/// refused, never replaced.
function title_again(title: RecipeTitle) fails AttachmentUnreadable -> LinkPart {
    var title_back: Text = attempt encoding.from_utf8(encoding.to_utf8(title)) {
        malformed: AttachmentUnreadable
    }
    return LinkPart(title_back)
}

/// A title as it goes in a link a sign-in email carries.
function link_part(title: RecipeTitle) -> LinkPart {
    return LinkPart(encoding.to_url_component(title))
}
```

The page's values: SHA-256 of "abc" is NIST's FIPS 180-4 example, and
Base64 of "f", "fo" and "foo" RFC 4648's test vectors:

```jadpo check project=hashing-encodings file=main.test.jadpo
test "hashing and the encodings give what the page says" {
    var abc_photo: PhotoBytes = attempt attachment("YWJj")
    assert photo_key(
        abc_photo
    ) == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    assert sent_photo(abc_photo) == "YWJj"
    var f: PhotoBytes = attempt attachment("Zg==")
    assert sent_photo(f) == "Zg=="
    var fo: PhotoBytes = attempt attachment("Zm8=")
    assert sent_photo(fo) == "Zm8="
    assert attachment("Zg") fails AttachmentUnreadable
    assert attachment("Zm9v YmFy") fails AttachmentUnreadable
    var key: PhotoBytes = attempt key_bytes(
        "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD"
    )
    assert sent_photo(key) == "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0="
    assert key_bytes("ba78z6") fails AttachmentUnreadable
    assert share_code("Rye loaf") == "_Hbznn1YqQK2jbbPF34hJmf_7AY2WLJKH4ZE68W-z_I"
    var code_bytes: PhotoBytes = attempt code_digest("_Hbznn1YqQK2jbbPF34hJmf_7AY2WLJKH4ZE68W-z_I")
    assert sent_photo(code_bytes) == "/Hbznn1YqQK2jbbPF34hJmf/7AY2WLJKH4ZE68W+z/I="
    assert code_digest("_Hbznn1YqQK2jbbPF34hJmf_7AY2WLJKH4ZE68W-z_I=") fails AttachmentUnreadable
    var title_back: LinkPart = attempt title_again("Crème brûlée")
    assert title_back == "Crème brûlée"
    assert link_part("Crème brûlée & rye") == "Cr%C3%A8me%20br%C3%BBl%C3%A9e%20%26%20rye"
}
```

## Data formats: JSON, CSV, TSV and JSON Lines

The four format namespaces, each in one shape: `json` for one document in
memory and for a large JSON list in a file, and `csv`, `tsv` and
`json_lines` for files. The file forms were decided with batches on
2026-10-16 ([the packages design](research/packages-and-extension-proposals.md#files-and-formats-open-item-9)
and [area 12](12-jobs-and-events.md#batches-work-over-many-items)) and
their operations live here; the in-memory `json.read` and `json.write`
were decided with this page (question 14). How a run goes (batches,
transactions, resuming, `batches.status` and `batches.cancel`, problems)
stays area 12's.

- **Storage and formats are separate layers.** `store.read` and
  `store.put` (area 16) move bytes; these namespaces are pure, turning
  bytes into the app's types and back, wherever the bytes came from.
- **Rows of a file of any size go only through the batch forms.** Each
  is called only in an action, starts after the action commits, as `emit`
  does, and returns a `BatchRun` at once; if the action rolls back, the run
  never starts. `key:` makes a repeated start return the same run. There
  is no loop over a file's rows and no whole-file parse into a list.
- **`each:` and `finished:` are the inline `->` form** area 3 allows for
  standard operations: reading, `each:` calls one action per row; writing,
  it is a pure expression giving the row. Values they capture are saved
  with the run, so only storable values may be captured.
- **Writing takes the store to write to,** the file's `name:`, and
  `from:`: a call to a named query returning a `Page` (area 6), walked in
  keyset pages, or to a provider's list operation, walked by `page.next`
  (area 11). `finished:` receives an `ImportSummary` after a read and an
  `ExportSummary`, holding the stored `File`, after a write (area 2's
  types).

### `csv`

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `csv.read(file, as:, each:, finished:, key:)` | `BatchRun`: each row of a stored `File` decoded as an `Input` of `as:` and handed to `each:`; `finished:` receives an `ImportSummary` | Never as a call: a row that can't be decoded, or whose action fails, is a `RowProblem` and the rest carry on; a file-level fault (a missing required column, an unterminated quote named by the line where it opened, an encoding the format doesn't declare, text that isn't text) stops the run | none |
| `csv.write(store, name:, format:, from:, each:, finished:, key:)` | `BatchRun`: each item of `from:` turned into an `Output` row by `each:` and written to `name:` in `store`; `format:` a `CsvFormat`, required; `finished:` receives an `ExportSummary` | Never as a call: an item whose action fails is a `RowProblem` | none |

- **Rows are typed both ways.** A row read is an `Input` and a row
  written an `Output` (area 7's rules, problems and JSON encoding). A
  column maps to a field by its name or by `column: "Email address"`;
  cells in other conventions use the decided `wire:` encodings
  (`Wire.day_month_year`) or a pure `decode:` function. A CSV row holds
  single values only: a list or an object in one is a compile error
  naming JSON Lines.
- **Writing names its reader.** `CsvFormat.spreadsheet` writes a
  byte-order mark, CRLF line endings and escapes formula-like text;
  `CsvFormat.standard` writes CRLF only. A write without `format:` is a
  compile error.
- **Encodings.** Reading takes UTF-8 (a byte-order mark removed), UTF-16
  with a mark, and legacy encodings the format declares, never guessing;
  writing is UTF-8. gzip (`.csv.gz`) is read only with
  `max_unpacked_size:`, and no archives are opened.
- **A `RowProblem`** holds the row as a spreadsheet numbers it (the
  header is row 1, the first data row row 2), the column, the field and
  the rule, never the value, so personal data stays out of problem lists
  and logs.
- **No CSV in memory.** A provider's CSV answer goes into a store and
  through `csv.read`, so there is one way to read rows.

### `tsv`

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `tsv.read(file, as:, each:, finished:, key:)` | As `csv.read`, with a tab between cells | As `csv.read` | none |
| `tsv.write(store, name:, format:, from:, each:, finished:, key:)` | As `csv.write`, with a tab between cells | As `csv.write` | none |

TSV is CSV's tab-delimited form: every rule under `csv` applies.

### `json_lines`

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `json_lines.read(file, as:, each:, finished:, key:)` | `BatchRun`: each line of a stored `File` decoded as one JSON value, an `Input` of `as:`, and handed to `each:`; `finished:` receives an `ImportSummary` | As `csv.read`: a line that can't be decoded is a `RowProblem`; text that isn't text stops the run | none |
| `json_lines.write(store, name:, from:, each:, finished:, key:)` | `BatchRun`: each item of `from:` turned into an `Output` by `each:` and written as one line; `finished:` receives an `ExportSummary` | As `csv.write` | none |

A JSON Lines row may hold lists and objects, which is why a CSV row that
needs them is pointed here. Encodings and gzip are as for `csv`.

### `json`

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `json.read(text, as: T)` | `T`, decoded and checked against every rule of `T`; `text` is `Text`, or `Bytes` in UTF-8 | `malformed` (not JSON), `mismatch` (a field missing, unknown or breaking a rule) | none |
| `json.write(value)` | `Text`, compact; fields in declared order, sets and map keys in canonical order, so the same value always gives the same text | | none |
| `json.read(file, items_at:, as:, each:, finished:, key:)` | `BatchRun` over a large JSON list in a stored `File`, `items_at:` naming where the list sits in the document; each item an `Input` of `as:`, handed to `each:` | As `csv.read` | none |
| `json.write(store, name:, from:, each:, finished:, key:)` | `BatchRun` writing a large JSON list, each item of `from:` turned into an `Output` by `each:` | As `csv.write` | none |

- **One name per direction** (question 14): the packages design's
  `json.decode` for single documents is `json.read` given text.
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
  them (chunk 5.5g2): a built-in type holding `field: Text` and
  `rule: Text`, named as area 7's problems are, which only the runtime
  builds.
- **No dynamic JSON value.** No scenario needs a JSON value of no type;
  a payload whose shape isn't known is kept as `Text` and passed on.

An order a supplier's system sends as JSON, read as the bakery's type and
written back (chunk 5.5g). Reading checks every rule of the type, fills
`rush`'s default and leaves `note` out; writing gives the fields in
declared order, with no white space:

```jadpo check project=json-in-memory
type Sku = Text {
    max_length: 20
}

type LineCount = Int {
    min: 1
    max: 1000
}

type LinePrice = Decimal {
    scale: 2
    min: 0.00
}

type ImportedLine = Input {
    sku: Sku
    count: LineCount
    price: LinePrice
}

type SupplierOrder = Input {
    reference: Sku
    lines: List<ImportedLine>
    rush: Bool {
        default: false
    }
    note: Sku {
        optional: true
    }
}

type SupplierMessage = Text {
    max_length: 100000
}

type OrderJson = Text {}

type ImportUnreadable = Failure {
    kind: FailureKind.invalid_value
}

type ImportInvalid = Failure {
    kind: FailureKind.invalid_value
}

/// The order a supplier's message holds, checked against every rule of
/// `SupplierOrder` in one step.
function supplier_order(
    sent_order: SupplierMessage
) fails ImportUnreadable, ImportInvalid -> SupplierOrder {
    return attempt json.read(sent_order, as: SupplierOrder) {
        malformed: ImportUnreadable
        mismatch: ImportInvalid
    }
}

/// The same order, as the one text `json.write` gives it.
function order_json(
    sent_order: SupplierMessage
) fails ImportUnreadable, ImportInvalid -> OrderJson {
    var order: SupplierOrder = attempt supplier_order(sent_order)
    return OrderJson(json.write(order))
}
```

Text that isn't JSON fails `malformed`; JSON that isn't a `SupplierOrder`
(a line's `count` of 0, an unknown field, a missing `lines`) fails
`mismatch`, and the failure's report names the first problem's path and
rule, `lines.0.count` and `min`, never the value:

```jadpo check project=json-in-memory file=main.test.jadpo
test "json reads and writes what the page says" {
    var order_text: OrderJson = attempt order_json(
        "{\"lines\": [{\"price\": \"3.5\", \"count\": 2, \"sku\": \"rye\"}], \"reference\": \"A-12\"}"
    )
    assert order_text == "{\"reference\":\"A-12\",\"lines\":[{\"sku\":\"rye\",\"count\":2,\"price\":\"3.50\"}],\"rush\":false}"
    var order_back: OrderJson = attempt order_json(
        "{\"reference\":\"A-12\",\"lines\":[{\"sku\":\"rye\",\"count\":2,\"price\":\"3.50\"}],\"rush\":false}"
    )
    assert order_back == order_text
    assert supplier_order("{\"reference\": ") fails ImportUnreadable
    assert supplier_order(
        "{\"reference\": \"A-12\", \"lines\": [{\"sku\": \"rye\", \"count\": 0, \"price\": \"3.50\"}]}"
    ) fails ImportInvalid
    assert supplier_order(
        "{\"reference\": \"A-12\", \"lines\": [], \"colour\": \"red\"}"
    ) fails ImportInvalid
    assert supplier_order("{\"reference\": \"A-12\"}") fails ImportInvalid
}
```

A failure that says what was wrong (chunk 5.5g2). One mapped from
`mismatch` that declares `problem: DecodeProblem`, in its `client` or its
`internal` block, receives the first problem: its `field`, the path with
list positions as numbers, and its `rule`, never the value. The arm gives
every other field and never `problem`, and `invalid:` fills it only when
it covers `mismatch` alone. Nothing else fills one, so `malformed:`, a
construction's rule or a `reject` naming such a failure is
`failure.missing_context_field`. Only the runtime builds a
`DecodeProblem`; a program reads its two fields, as the outcome `match`
here does:

```jadpo check project=json-decode-problem
type ProductCode = Text {
    max_length: 20
}

type LineQuantity = Int {
    min: 1
    max: 1000
}

type SupplierLine = Input {
    sku: ProductCode
    quantity: LineQuantity
}

type SupplierOrder = Input {
    reference: ProductCode
    lines: List<SupplierLine>
}

type SupplierMessage = Text {
    max_length: 100000
}

type ProblemPart = Text {}

type OrderUnreadable = Failure {
    kind: FailureKind.invalid_value
}

type OrderInvalid = Failure {
    kind: FailureKind.invalid_value
    message: "The order breaks a rule."
    client: {
        problem: DecodeProblem
    }
}

/// The order a supplier's message holds; one that isn't a
/// `SupplierOrder` fails `OrderInvalid` with its first problem.
function supplier_order(
    sent_order: SupplierMessage
) fails OrderUnreadable, OrderInvalid -> SupplierOrder {
    return attempt json.read(sent_order, as: SupplierOrder) {
        malformed: OrderUnreadable
        mismatch: OrderInvalid
    }
}

/// What is wrong with a message that isn't an order: the field's path
/// and the rule.
function order_problem(sent_order: SupplierMessage) -> List<ProblemPart> {
    return match supplier_order(sent_order) {
        success -> []
        OrderUnreadable -> []
        OrderInvalid(invalid) -> [
            ProblemPart(invalid.problem.field),
            ProblemPart(invalid.problem.rule)
        ]
    }
}
```

The first test reads the problem back, the field and the rule even where
the value is a secret; the second lets the failure through, so `jadpo
test` shows its report, the problem among the client fields and nothing
the document held:

```jadpo check project=json-decode-problem file=main.test.jadpo expect=test.unexpected_failure
test "the problem names the field and the rule, never the value" {
    assert order_problem(
        "{\"reference\": \"A-12\", \"lines\": [{\"sku\": \"rye\", \"quantity\": 2}, {\"sku\": \"spelt\", \"quantity\": 1}, {\"sku\": \"oat\", \"quantity\": 1001}]}"
    ) == ["lines.2.quantity", "max"]
    assert order_problem("{\"reference\": \"sk_live_51HxQ9eSECRETv4lue\", \"lines\": []}") == [
        "reference",
        "max_length"
    ]
}

test "an order whose third line is over the maximum" {
    var order: SupplierOrder = attempt supplier_order(
        "{\"reference\": \"A-12\", \"lines\": [{\"sku\": \"rye\", \"quantity\": 2}, {\"sku\": \"spelt\", \"quantity\": 1}, {\"sku\": \"oat\", \"quantity\": 1001}]}"
    )
    assert order.reference == "A-12"
}
```

```text output
FAIL  an order whose third line is over the maximum
 --> main.test.jadpo:12

  failure: `OrderInvalid` from function supplier_order
  at: main.jadpo:43:20
   |
43 |     return attempt json.read(sent_order, as: SupplierOrder) {
   |                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ breaks `mismatch` (`max` on `lines.2.quantity`)
  reason: The value built here breaks `mismatch` (`max` on `lines.2.quantity`), so `attempt` raises the failure its block maps that rule to.
  code: order_invalid
  kind: FailureKind.invalid_value
  message: The order breaks a rule.
  client: { problem: DecodeProblem { field: "lines.2.quantity", rule: "max" } }
  path: test "an order whose third line is over the maximum" -> function supplier_order

  rule: test.unexpected_failure
  rerun: jadpo test main.test.jadpo:12 --seed 1

test failed: 1 failed, 1 passed, 2 pure tests
```

A route whose action reads an order this way sends the problem in the
failure's `details` (area 7), under the names area 7's input problems
use; a `problem` declared in `internal` stays on the server:

```json
{
  "error": {
    "code": "order_invalid",
    "message": "The order breaks a rule.",
    "request_id": "req_0b1e…",
    "details": {
      "problem": { "field": "lines.2.quantity", "rule": "max" }
    }
  }
}
```

XML is deferred: it waits for an XML byte format in a later release.

## Colour

`Colour` holds `red`, `green`, `blue` and `alpha`, each 0 to 255, in sRGB.
Every operation gives an sRGB `Colour`; other spaces appear only as
arguments.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `colour.from_hsl(hue, saturation, lightness)` | `Colour`; `hue` an `Angle`, the others `Percentage` | | none |
| `colour.from_oklch(lightness, chroma, hue)` | `Colour`, clipped into sRGB | | none |
| `colour.mix(a, b, ratio)` | `Colour`, mixed in OKLCH; `ratio` a `Percentage` of `b` | | none |
| `colour.with_lightness(value, lightness)` | `Colour` with its OKLCH lightness set, hue kept | | none |
| `colour.contrast(a, b)` | `Decimal`, WCAG 2's contrast ratio, 1.00 to 21.00 | | none |

## Media

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `media.fit(dimensions, within:)` | `ImageDimensions` scaled to fit inside `within`, keeping the aspect ratio, never larger than the original, rounded down to whole pixels | | none |

## Web and software

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `web.with_query(url, query)` | `Url` with each entry of `query` (a `Map<Text, Text>`) added, percent-encoded, in key order | | none |
| `web.resolve(base, relative)` | `Url`: a `RelativeUrl` or `UrlPath` resolved against `base` by RFC 3986 | | none |
| `web.cidr_contains(range, address)` | `Bool`: the `IpAddress` is inside the `Cidr` | | `x >>= a` |
| `semver.satisfies(version, range)` | `Bool`: a `SemanticVersion` is inside a `VersionRange` | | none |

## Groups with the baseline only

These groups have nothing beyond the contract: each type's rules are its
construction, which the contract's `text.parse` and data entry already
cover. Where a rule is more than a pattern, the type names one validator.

| Group | Validators beyond a pattern |
|---|---|
| People and contact | `PhoneNumber` (E.164 per country), `PostalCode` (per country), `PostalAddress` and `PersonName` (CLDR order for `text.format`) |
| Places | `CountrySubdivision` against ISO 3166-2, `UnLocode` against its list |
| Payment and identity numbers | `Iban` (mod-97), `PaymentCardNumber` (Luhn), `UkSortCode` with `UkAccountNumber` (modulus check), `AbaRoutingNumber`, `Lei`, `Isin`, `NhsNumber` (mod-11), `UkNationalInsuranceNumber` (prefix rules) |
| Commerce and logistics | `Ean13`, `UpcA`, `Isbn`, `Sscc` (GS1 check digits), `Vin` (check digit), `ContainerNumber` (ISO 6346) |
| Health | `SnomedConcept` (Verhoeff), `Icd10Code` against its release |
| Travel | `IataAirport`, `IcaoAirport`, `IataAirline` against their lists |
| Other | `Orcid` (ISO 7064), `Mpan` (check digit) |

A marketplace pays its sellers into the account they type, takes a
patient's NHS number from a referral, and shows a card on file by its
last four:

```jadpo check project=payment-numbers
type FormText = Text {}

type NotAnIban = Failure {
    kind: FailureKind.invalid_value
}

type NotAnNhsNumber = Failure {
    kind: FailureKind.invalid_value
}

/// The account a seller is paid into, as they typed it: checked by its
/// country's length and its mod-97 check digits.
function payout_account(raw: FormText) fails NotAnIban -> Iban {
    var account: Iban = attempt Iban(raw) {
        format: NotAnIban
    }
    return account
}

/// A patient's NHS number, as a referral sends it.
function patient_number(raw: FormText) fails NotAnNhsNumber -> NhsNumber {
    var number: NhsNumber = attempt text.parse(raw, as: NhsNumber) {
        malformed: NotAnNhsNumber
    }
    return number
}

/// An account as a statement prints it.
function account_label(account: Iban, viewer: Locale) -> PresentationText {
    return text.format(account, locale: viewer)
}

/// A card on file as a receipt shows it: its last four only.
function card_label(card: PaymentCardNumber, viewer: Locale) -> PresentationText {
    return text.format(card, locale: viewer)
}
```

```jadpo check project=payment-numbers file=main.test.jadpo
test "payment and identity numbers are checked as they are built" {
    var account: Iban = attempt payout_account("GB29NWBK60161331926819")
    assert account == Iban("GB29NWBK60161331926819")
    assert payout_account("GB29NWBK60161331926818") fails NotAnIban
    assert payout_account("GB29 NWBK 6016 1331 9268 19") fails NotAnIban
    var patient: NhsNumber = attempt patient_number("9434765919")
    assert patient == NhsNumber("9434765919")
    assert patient_number("9434765918") fails NotAnNhsNumber
}

test "they are shown as they are printed" {
    var account: Iban = Iban("GB29NWBK60161331926819")
    assert account_label(account, Locale.en_gb) == "GB29 NWBK 6016 1331 9268 19"
    assert card_label(PaymentCardNumber("4242424242424242"), Locale.en_gb) == "•••• 4242"
    assert text.format(NhsNumber("9434765919"), locale: Locale.en_gb) == "943 476 5919"
}
```

> Built in chunk 5.5i for the 17 payment and identity numbers that are
> text: `PaymentCardNumber`, `CardBin`, `PaymentReference`,
> `InvoiceNumber`, `Iban`, `Bic`, `UkSortCode`, `UkAccountNumber`,
> `AbaRoutingNumber`, `Lei`, `Isin`, `UkUtr`, `UkCompanyNumber`,
> `UkNationalInsuranceNumber`, `UsSsn`, `UsEin` and `NhsNumber`, each a
> pattern and, where the table above names one, a validator; the UK
> modulus check, which needs a sort code and an account number together,
> comes with `BankAccount` in chunk 5.5i2, with the money group's records
> and the identity documents. What the page left open is "Chosen in chunk
> 5.5i", under "Review".

A shop sends parcels abroad and prints each label for the person who
packs it, greets its customers by name, and reads the countries a
supplier's file gives by their three-letter codes:

```jadpo check project=people-and-places
type FileText = Text {}

type UnknownCountry = Failure {
    kind: FailureKind.invalid_value
}

/// A parcel's label, as its packer reads an address: in the order of the
/// parcel's country for the packer's language.
function parcel_label(parcel_to: PostalAddress, packer: Locale) -> PresentationText {
    return text.format(parcel_to, locale: packer)
}

/// A customer's name as a greeting writes it in the reader's language.
function greeting_name(customer: PersonName, viewer: Locale) -> PresentationText {
    return text.format(customer, locale: viewer)
}

/// The country a supplier's file gives by its three-letter code.
function supplier_country(raw: FileText) fails UnknownCountry -> Country {
    var made_in: Country = attempt text.parse(raw, as: Country, form: CodeForm.alpha3) {
        malformed: UnknownCountry
    }
    return made_in
}
```

```jadpo check project=people-and-places file=main.test.jadpo
test "an address is in its country's order for the reader" {
    var jp_parcel: PostalAddress = PostalAddress {
        lines: ["丸の内2-7-2"]
        locality: "千代田区"
        region: "東京都"
        postal_code: "100-8994"
        country: Country.jp
    }
    assert parcel_label(jp_parcel, Locale.ja_jp) == "〒100-8994\n東京都千代田区\n丸の内2-7-2"
    assert parcel_label(jp_parcel, Locale.en_gb) == "丸の内2-7-2\n千代田区, 東京都\n100-8994\nJapan"
    var de_parcel: PostalAddress = PostalAddress {
        lines: ["Platz der Republik 1"]
        locality: "Berlin"
        region: none
        postal_code: "11011"
        country: Country.de
    }
    assert parcel_label(de_parcel, Locale.de_de) == "Platz der Republik 1\n11011 Berlin"
    assert parcel_label(de_parcel, Locale.en_us) == "Platz der Republik 1\n11011 Berlin\nGermany"
}

test "a name is in the order the reader's language gives" {
    var countess: PersonName = PersonName {
        given: "Ada"
        family: "Lovelace"
    }
    assert greeting_name(countess, Locale.en_gb) == "Ada Lovelace"
    assert greeting_name(countess, Locale.hu_hu) == "Lovelace Ada"
    var yamada: PersonName = PersonName {
        given: "太郎"
        family: "山田"
    }
    assert greeting_name(yamada, Locale.en_gb) == "山田太郎"
}

test "a closed list reads its codes and its other code systems" {
    var made_in: Country = attempt supplier_country("gbr")
    assert made_in == Country.gb
    assert supplier_country("UK") fails UnknownCountry
    assert text.from(Country.in_) == "IN"
    assert text.from(Country.gb, form: CodeForm.numeric) == "826"
    assert text.format(Country.gb, locale: Locale.fr_fr) == "Royaume-Uni"
    assert text.format(Language.ja, locale: Locale.en_gb) == "Japanese"
    assert list.count(Country.all) == 249
}
```

A recipient's name and address are stored a column per part, so a
query can read a town or a country as it reads any column, and come back
as they were written:

```jadpo check project=recipient-addresses file=app.jadpo hidden
application {}
```

```jadpo check project=recipient-addresses file=domains/mailroom/entities/recipient.jadpo migrate=initial
/// Someone the mailroom sends parcels to.
entity Recipient {
    id: Uuid
    name: PersonName  // columns `name_given` and `name_family`
    ship_to: PostalAddress  // a column per part, its lines a `text[]`
    citizen_of: Nationality?  // its code, `IN`
}
```

```jadpo check project=recipient-addresses file=domains/mailroom/actions/enrol.jadpo hidden
use mailroom/entities { Recipient }

/// Enrols a recipient with the address the mailroom sends to.
action enrol(name: PersonName, ship_to: PostalAddress, citizen_of: Nationality?) -> Recipient {
    return Recipient.create {
        set: {
            name: name
            ship_to: ship_to
            citizen_of: citizen_of
        }
    }
}
```

```jadpo check project=recipient-addresses file=domains/mailroom/actions/readdress.jadpo hidden
use mailroom/entities { Recipient }

/// Sends a recipient's parcels to a new address.
action readdress(recipient: Recipient, new_address: PostalAddress) -> Recipient {
    return Recipient.update(recipient) {
        set: {
            ship_to: new_address
        }
    }
}
```

```jadpo check project=recipient-addresses file=domains/mailroom/actions/enrol.test.jadpo
use mailroom/entities { Recipient }
use mailroom/actions { readdress }

test "a name and an address are kept part by part" {
    var enrolled: Recipient = enrol(
        PersonName {
            given: "Mary"
            family: none
        },
        PostalAddress {
            lines: ["Flat 2", "14 Rue de Rivoli"]
            locality: "Paris"
            region: none
            postal_code: "75004"
            country: Country.fr
        },
        Nationality.in_
    )
    var kept_record: Recipient = attempt Recipient.get(enrolled.id)
    assert kept_record.name == enrolled.name
    assert kept_record.ship_to == enrolled.ship_to
    assert kept_record.citizen_of == Nationality.in_
    var readdressed: Recipient = readdress(
        kept_record,
        PostalAddress {
            lines: ["2-7-2 Marunouchi"]
            locality: "Chiyoda-ku"
            region: "Tokyo"
            postal_code: "100-8994"
            country: Country.jp
        }
    )
    var kept_after: Recipient = attempt Recipient.get(enrolled.id)
    assert kept_after.ship_to == readdressed.ship_to
}
```

> Built in chunk 5.5l for 15 of people and places: the closed lists
> `Country`, `CountrySubdivision`, `Continent`, `Language`,
> `WritingScript`, `TextDirection` and `Nationality`, each with `.all`,
> its other code systems as `form:` and its withdrawn codes read but not
> written (`type.variant_retired`), with the currencies ISO 4217 withdrew
> since 2021; `GivenName`, `FamilyName`, `Username`, `PhoneExtension` and
> `Uprn` held as text; and `PersonName`, `EmailWithName` and
> `PostalAddress` as objects with parts that may be `none`, each stored in
> a column per part. A name is shown by CLDR's person-name patterns and an
> address by libaddressinput's formats, since CLDR has none for
> addresses. `PhoneNumber`, `CallingCode`, `PostalCode` and `UnLocode`
> were built in chunk 5.5l2, below. What the page left open is "Chosen in
> chunk 5.5l", under "Review".

A shop takes a customer's phone number and postcode as they type them in
their own country, and shows the number to its staff as their region
dials it:

```jadpo check project=phones-and-postcodes
type TypedText = Text {}

type PhoneUnreadable = Failure {
    kind: FailureKind.invalid_value
}

type PostcodeUnreadable = Failure {
    kind: FailureKind.invalid_value
}

/// A customer's phone number as they typed it in their country: national
/// or international, with any spaces and brackets.
function typed_phone(raw: TypedText, typed_in: Country) fails PhoneUnreadable -> PhoneNumber {
    var phone: PhoneNumber = attempt text.parse(raw, as: PhoneNumber, country: typed_in) {
        malformed: PhoneUnreadable
    }
    return phone
}

/// A customer's postcode as they typed it, in their country's own form.
function typed_postcode(raw: TypedText, typed_in: Country) fails PostcodeUnreadable -> PostalCode {
    var postcode: PostalCode = attempt text.parse(raw, as: PostalCode, country: typed_in) {
        malformed: PostcodeUnreadable
    }
    return postcode
}

/// A number as someone in the staff_locale's region dials it.
function dialled_as(phone: PhoneNumber, staff_locale: Locale) -> PresentationText {
    return text.format(phone, locale: staff_locale)
}
```

```jadpo check project=phones-and-postcodes file=main.test.jadpo
test "a phone number is read as it is typed and kept in E.164" {
    var mobile: PhoneNumber = attempt typed_phone("07400 123456", Country.gb)
    assert mobile == PhoneNumber("+447400123456")
    var from_abroad: PhoneNumber = attempt typed_phone("+44 (0)7400 123456", Country.fr)
    assert from_abroad == mobile
    assert typed_phone("07400 12345", Country.gb) fails PhoneUnreadable
    assert typed_phone("07400 123456", Country.us) fails PhoneUnreadable
}

test "a phone number is shown as the staff_locale's region dials it" {
    var mobile: PhoneNumber = PhoneNumber("+447400123456")
    assert dialled_as(mobile, Locale.en_gb) == "07400 123456"
    assert dialled_as(mobile, Locale.fr_fr) == "+44 7400 123456"
    assert dialled_as(PhoneNumber("+12015550123"), Locale.en_us) == "(201) 555-0123"
}

test "a postcode is in its country's form" {
    var royal_code: PostalCode = attempt typed_postcode("sw1a1aa", Country.gb)
    assert royal_code == PostalCode("SW1A 1AA")
    var us_code: PostalCode = attempt typed_postcode("221621010", Country.us)
    assert us_code == PostalCode("22162-1010")
    assert typed_postcode("75004", Country.gb) fails PostcodeUnreadable
    assert typed_postcode("2A004", Country.fr) fails PostcodeUnreadable
}
```

> Built in chunk 5.5l2: `PhoneNumber`, valid by libphonenumber's metadata
> for its region, `CallingCode` against the codes ITU-T assigned,
> `PostalCode` with each country's pattern from libaddressinput, and
> `UnLocode` by its pattern with its country checked, each held as text;
> `country:` on `text.parse` for the first and third; and a
> `PostalAddress`'s postal code, now a `PostalCode`, fitting its country,
> proved in a literal and checked as JSON is read. The UN/LOCODE list
> itself isn't shipped: its terms are unclear. What the page left open is
> "Chosen in chunk 5.5l2", under "Review".

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
left out (question 27): flags are a `Set` of an enum, which a reviewer can
read, and the one scenario with bit-packed data, device payloads, decodes
them in the provider package that knows the format.

## Geospatial: a future area

Geospatial work needs its own area page, not a section here: new stored
types, PostGIS, query predicates and indexes touch areas 2, 5 and 6, which
this page doesn't. Its scope, decided with question 12, for chunk 5.5u's
area page to specify:

| Part | Scope |
|---|---|
| Types | `Point { latitude, longitude }`, `LineString`, `Polygon`, `MultiPoint`, `MultiLineString`, `MultiPolygon` and `Geometry` (the standard types), in WGS84; named fields, never a pair |
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
Its scope, decided with question 20, for chunk 5.5v's area page to
specify:

| Part | Scope |
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
| Basket total and a 10% discount | `money.sum`, `total * 90%` |
| Discount spread across the lines for the receipt | `money.allocate` |
| Product page address from its title | `text.slug` |
| Order reference read out on the phone | `random.code` |
| Products by price, highest first | `list.sort_by`, `list.reverse`; `order:` in a query |
| Product feed for a marketplace | `csv.write` |
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
| Carrier manifest import | `csv.read` |
| Proof-of-delivery photo fingerprint | `hash.sha256` |
| Parcels by weight band | `list.group_by` |

**4. Property listings**

| Does | Needs |
|---|---|
| Asking price shown in the visitor's locale | `text.format` |
| Price per square metre | `price / units.in(floor_area, AreaUnit.square_metre, ...)`: `Money` by a `Decimal` |
| Listing address | `text.slug` |
| Description preview | `text.trim`, `text.keep_first` |
| Features filter | `set.contains`, `set.intersection` |
| Map search in the visible area | Geospatial (gap: future area) |
| Portal feed import | `json.read` as a batch |

**5. SaaS with subscriptions and billing**

| Does | Needs |
|---|---|
| Plan prices per customer's currency | `Money` with a currency known while running |
| Proration for a mid-month change | `price * (days_left / days_in_month)`, the type's rounding |
| Invoice total of lines | `money.sum` |
| Signed webhooks to customers | `crypto.hmac`, `json.write` |
| Customers' third-party tokens kept safe | `crypto.encrypt`, encrypted fields |
| Features per plan | `set.contains` |
| Usage per day | `map.get`, `map.with`; monthly totals need aggregate queries (gap: language) |

**6. CMS with CSV imports and exports**

| Does | Needs |
|---|---|
| Import articles from a spreadsheet | `csv.read`, construction per row |
| Export for editors | `csv.write` with `CsvFormat.spreadsheet` |
| Tags typed by hand, tidied | `text.trim`, `text.lower`, `set.with` |
| Excerpt and word count | `text.keep_first`, `text.split`, `list.filter`, `list.count` |
| Articles in a locale's alphabetical order | `list.sort_by` with `locale:` |
| Rich text kept safe | `Html`, made safe by its construction |

**7. IoT telemetry**

| Does | Needs |
|---|---|
| Readings arriving in bulk | `json_lines.read` |
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
| Export of a filtered list | `csv.write` |
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
| File formats | ● | | ● | ● | | ● | ● | ● | |
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
`total * discount`, a `Percentage`; 05's divisions and 10's due dates are already decided.

## Language gaps this page depends on

Nothing is left out because the language can't do it yet: each gap is
sorted into "left out by design" (with its reason, above and in the next
table) or a language change decided here (question 21), with the
operations it unblocks kept on this page.

| Gap | Fix | Owning area | Chunk | Unblocks |
|---|---|---|---|---|
| `for` can't walk sets or maps | `for` takes a `Set`, in area 2's canonical order; maps through `map.keys` | 2, 3 | 5.5c, built | Walking sets and maps |
| No map literal | `[]` where a map is expected; a literal with entries designed in area 2 | 2 | 5.5c, built | Building maps |
| Inline functions take one parameter | Two parameters, `(a, b) -> ...`, for standard operations only (area 3's question 13 widened) | 3 | 5.5c2, built | `list.zip_with`, `map.filter`, `map.map_values` |
| No `Float` and no maths | Maths on `Decimal` with `scale:` and `rounding:`, deterministic everywhere; still no `Float` | 2 (question 8 there) | 5.5e, built | 8 `decimal` operations; geospatial distance |
| Code can't read a secret | Opaque key settings, used only as a `crypto` call's `key:` | 10 | 5.5q | `crypto` |
| Keys held as data (per device, per person) | An encrypted field of a key type, usable only as `key:` | 10, 15 | With encrypted fields, after the first build (personal data) | Per-device signatures, `crypto.verify_totp` |
| No randomness | Generated `random`, actions only, seeded in tests, recorded for replay | 10, 13, 17 | 5.5q | `random.*`, `encrypt`, `hash_password` |
| Text has no locale order or case | `locale:` on `list.sort_by`, `text.lower` and `text.upper`, through ICU4X's collator and case mapping (two more crates of the pinned ICU4X) | 10 | 5.5f, built | Locale sorting and case |
| Values formatted for people only in translations | The contract's `text.format`, giving `PresentationText`, for every standard type | 10 | 5.5a | Showing numbers, money, quantities and addresses in code |
| No parsing from text | The contract's `text.parse(raw, as: T)`, canonical forms only | 2 | 5.5a | Text from a person or a file cell |
| Queries compute nothing | Aggregate queries: `count`, `sum`, `min`, `max`, `average`, grouped by a field or a time bucket, using operations with SQL forms; area 6 already lists counts and sums in queries as deferred | 6 | 5.6c | Reports, dashboards, usage totals |
| No rule across fields | Named rules on an object type (`rule ends_after_start: ends > starts`), proved or attempted like any rule | 2 | 5.5t | Bookings, date ranges, price bands |
| Text from outside not normalised | NFC applied as text is decoded at a boundary | 2, 7 | 5.5f, built | Equal text comparing equal |
| `Money` is an application's own object | Built-in `Money` and `Currency` | 2 | 5.5h | `money` |
| No geospatial types | A future area | New area | 5.5u plans it | Geospatial |
| No AI pieces | A future area | New area | 5.5v plans it | AI |

**Left out by design, after the audit:**

| Left out | Why | Instead |
|---|---|---|
| Generic functions in your own code | Area 3 decided against them (question 16 there): no type-parameter errors for agents to fight | The standard library stays generic |
| `reduce`, `fold` | A second way to accumulate | `for` with `var mut`; `list.sum` |
| Comparators | A key expresses every order, and an inconsistent comparator differs by target | `sort_by`, `reverse` |
| `items[i]` | Out of range needs an arm or a defect | `list.at` gives `T?` |
| Regular expressions as calls | A pattern is a type's rule, in linear time | `pattern:` and construction (area 2) |
| Graphemes | They'd disagree with `max_length` and PostgreSQL, and change with each Unicode version | Code points (question 23) |
| Dynamic keys on records | A field is its own type and carries its own policy and marks | `Map` |
| A dynamic JSON value | No scenario needs one | `json.read(..., as:)`; `Text` to pass on |
| CSV in memory | #88: one way to read rows | `csv.read` through a store |
| MD5, SHA-1 | Broken | A package (area 11) |
| Bitwise operations | Flags read better as a set; device formats belong in their package | `Set<Enum>`; question 27 |
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

## Errors

Every error this area can produce, with an example that triggers it and the
new compiler's real output from `jadpo check`. The examples are checked as
loose files.

### `type.standard_clash`

A declaration named like a standard type: a Jadpo release added the
standard type after the application named its own. Its fixes are the swap
and the rename, each made by `jadpo fix`.

```jadpo check project=e-standard-clash expect=type.standard_clash
/// A room booking's reference, from before Jadpo had a `Uuid` of its own.
type Uuid = Text {
    max_length: 36
}
```

```text output
error: `Uuid` is the name of a standard type
 --> main.jadpo:2:6
  |
2 | type Uuid = Text {
  |      ^^^^ standard since Jadpo 0.1.0
  reason: Jadpo 0.1.0 added the standard type `Uuid`, and the application's own type can't share its name, or `Uuid` would mean two things. An upgrade never changes what code means silently, so `check` stops here until this type is swapped for the standard type or renamed.
  next: Run `jadpo fix Uuid swap` to use the standard `Uuid` in its place, or `jadpo fix Uuid rename <NewName>` to keep this one under a name of its own (agent)
  rule: type.standard_clash
  help: docs/language/19-standard-library.html#type-standard-clash

check failed: 1 error, 1 file
```

### `type.standard_exists`

A declaration whose only fields are a standard type's, by their common
names in any order and what they are built on, is that type under another
name. It is declinable: a type that means something else keeps its shape
with a reason on the declaration.

```jadpo check project=e-standard-exists expect=type.standard_exists
/// When a room is booked, from its start to its end.
type Booking = Object {
    starts_at: Instant
    ends_at: Instant
}
```

```text output
error: `Booking` is the standard `InstantRange` under another name
 --> main.jadpo:2:6
  |
2 | type Booking = Object {
  |      ^^^^^^^ a start and an end
  reason: `Booking` has only a start (`starts_at`) and an end (`ends_at`), each built on `Instant`, which is the standard `InstantRange`. Declarations of one shape under different names disagree on names and meaning, and every package and application has to convert between them; the standard type has one construction, one wire shape and one storage shape.
  next: Run `jadpo fix Booking swap` to use `InstantRange` where `Booking` is used, then convert at the edge where data arrives in another shape, or write `decline type.standard_exists: "<why>"` above it if this is deliberate (agent)
  rule: type.standard_exists
  help: docs/language/19-standard-library.html#type-standard-exists

check failed: 1 error, 1 file
```

Declined, with the reason review reads:

```jadpo check project=standard-exists-declined
/// A shift as the rota plans it, before anyone has worked it.
decline type.standard_exists: "Planned hours on the rota, not a range anyone has worked"
type PlannedShift = Object {
    starts_at: Instant
    ends_at: Instant
}
```

### `semantic.standard_operation_moved`

An operation the standard library has moved to another name: its edit
writes the new name, and the call is checked as the new operation.
`temporal.format` became the time types' `text.format` (question 2).

```jadpo check project=e-operation-moved expect=semantic.standard_operation_moved
type OrderedAt = Instant {}

function placed(at: OrderedAt) -> PresentationText {
    return temporal.format(
        at,
        zone: Zone.europe_london,
        locale: Locale.en_gb,
        format: TimeFormat.date_long
    )
}
```

```text output
error: `temporal.format` is now `text.format`
 --> main.jadpo:4:12
  |
4 |     return temporal.format(
  |            ^^^^^^^^^^^^^^^ moved
  reason: The standard library has one name for each operation; `temporal.format` moved to `text.format`, which takes the same values.
  next: Write `text.format` (agent)
  rule: semantic.standard_operation_moved
  help: docs/language/19-standard-library.html#semantic-standard-operation-moved

check failed: 1 error, 1 file
```

`collection.` became `list.`, `set.` and `map.` (question 6): a call
through it is reported with an edit to the namespace for its first value,
a list unless it is a set or a map, and checked as that operation where
it is built.

```jadpo check project=e-collection-moved expect=semantic.standard_operation_moved
type BatchName = Text {
    min_length: 1
}

type BatchCount = Int {
    min: 0
}

function batches_baked(batches: List<BatchName>) -> BatchCount {
    return BatchCount(collection.count(batches))
}
```

```text output
error: `collection.count` is now `list.count`
 --> main.jadpo:10:23
   |
10 |     return BatchCount(collection.count(batches))
   |                       ^^^^^^^^^^^^^^^^ moved
  reason: The standard library has one namespace for each type an operation works on, so `collection.` became `list.`, `set.` and `map.`; `batches` is a list.
  next: Write `list.count` (agent)
  rule: semantic.standard_operation_moved
  help: docs/language/19-standard-library.html#semantic-standard-operation-moved

check failed: 1 error, 1 file
```

### `type.argument_negative`

A count or a position is never negative (question 7): `list.at`'s
`position`, `slice`'s `from` and `count`, and the `n` of `keep_first`,
`keep_last`, `drop_first` and `drop_last`. A literal isn't below `0` and a
value's type has a `min` of at least `0`, so an off-by-one never reads
from the other end, as a negative position does in JavaScript.

```jadpo check project=e-argument-negative expect=type.argument_negative
type BatchName = Text {
    min_length: 1
}

type TraysOut = Int {}

/// The batches still to bake once `done` of them are out of the oven.
function still_to_bake(batches: List<BatchName>, done: TraysOut) -> List<BatchName> {
    return list.drop_first(batches, done)
}
```

```text output
error: `done` may be negative
 --> main.jadpo:9:37
  |
9 |     return list.drop_first(batches, done)
  |                                     ^^^^ `TraysOut` can be below `0`
  reason: `list.drop_first`'s `n` counts from the start, so it is never negative, which is proved while compiling: a literal isn't below `0` and a value's type has a `min` of at least `0`; `TraysOut` allows values below `0`.
  next: Pass a value whose type has `min: 0`, building it where a negative one is handled; count from the end with `keep_last` or `drop_last` (agent)
  rule: type.argument_negative
  help: docs/language/19-standard-library.html#type-argument-negative

check failed: 1 error, 1 file
```

### `type.argument_empty`

Text an operation never takes empty (`never_empty`): `text.replace`'s
`find` and `text.split`'s `separator`. A literal isn't `""` and a value's
type has a `min_length` of at least 1, so a search never looks for
nothing and a split never cuts between every character, which other
languages each do differently.

```jadpo check project=e-argument-empty expect=type.argument_empty
type Ingredients = Text {
    max_length: 400
}

type Divider = Text {
    max_length: 3
}

type IngredientCount = Int {
    min: 0
}

/// How many ingredients a recipe line lists, split where the cook says.
function ingredient_count(line: Ingredients, spacer: Divider) -> IngredientCount {
    return IngredientCount(list.count(text.split(line, spacer)))
}
```

```text output
error: `spacer` may be empty
 --> main.jadpo:15:56
   |
15 |     return IngredientCount(list.count(text.split(line, spacer)))
   |                                                        ^^^^^^ `Divider` can be empty
  reason: `text.split`'s `separator` is never empty, which is proved while compiling: a literal isn't `""` and a value's type has a `min_length` of at least 1; `Divider` allows empty text.
  next: Pass a value whose type has `min_length: 1`, building it where empty text is handled (agent)
  rule: type.argument_empty
  help: docs/language/19-standard-library.html#type-argument-empty

check failed: 1 error, 1 file
```

### `type.money_split`

Money divided by a count: `/` by an `Int`. Dividing money by a count is
sharing it out, and rounding each share loses a minor unit; `money.split`
hands every one to some share, and `money.allocate` shares in proportion.
`/` by a `Decimal` stays, since that is a rate (`/ 1.20` takes VAT off).

```jadpo check project=e-money-split expect=type.money_split
type BillTotal = Money {
    currency: Currency.gbp
    min: 0
}

/// One friend's share of a bill split three ways.
function share(total: BillTotal) -> BillTotal {
    return BillTotal(total / 3)
}
```

```text output
error: `BillTotal` is divided by a count
 --> main.jadpo:8:22
  |
8 |     return BillTotal(total / 3)
  |                      ^^^^^^^^^ money shared out by `/`
  reason: Dividing money by a count is sharing it out, and rounding each share loses minor units: £10.00 / 3 is £3.33, three times £9.99. `money.split` hands every minor unit to some share, and `/` takes only a `Decimal` rate, as `/ 1.20` takes a 20% tax off; `3` is a whole number.
  next: Share it with `money.split(total, 3)`, which gives a list of shares adding up to the total, or `money.allocate` for unequal shares; halve an amount with `* 0.5` (agent)
  rule: type.money_split
  help: docs/language/19-standard-library.html#type-money-split

check failed: 1 error, 1 file
```

Two amounts in currencies both types fix differently don't compile, by
area 3's `type.arithmetic_operand` (and `type.ordering_operand` for `<`):

```jadpo check project=e-currency-mismatch expect=type.arithmetic_operand
type CakePrice = Money {
    currency: Currency.gbp
}

type EuroPrice = Money {
    currency: Currency.eur
}

/// A cake priced in pounds, delivered from a shop that charges in euros.
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
  help: docs/language/03-logic.html#type-arithmetic-operand

check failed: 1 error, 1 file
```

### `type.unit_required`

A bare number where a physical quantity goes: a quantity is always made
with its unit, a suffixed literal (`90cm`), `units.of(90,
LengthUnit.centimetre)`, `text.parse` of text that names one, or another
quantity of its kind. A number alone could be any length, which is the
mistake the type exists to prevent, so this is never declinable. A plain
number still scales a quantity, by `*` and `/`.

```jadpo check project=e-unit-required expect=type.unit_required
type ShelfLength = Length {
    max: 3metre
}

/// The shelf a kitchen gets unless the customer asks for another.
function standard_shelf() -> ShelfLength {
    return ShelfLength(90)
}
```

```text output
error: `ShelfLength` needs a unit, and `90` is a bare number
 --> main.jadpo:7:24
  |
7 |     return ShelfLength(90)
  |                        ^^ no unit
  reason: A `Length` is always made with its unit, so a number alone could be any length: a number without its unit is the mistake the type exists to prevent.
  next: Write its unit after the number, such as `90metre`, or make it with `units.of(90, LengthUnit.metre)` in any of `LengthUnit`'s units (agent)
  rule: type.unit_required
  help: docs/language/19-standard-library.html#type-unit-required

check failed: 1 error, 1 file
```

Two kinds that the table doesn't combine don't compile either, by area
3's `type.arithmetic_operand`, which names the two kinds:

```jadpo check project=e-quantity-kinds expect=type.arithmetic_operand
type ShelfLength = Length {}

type ShelfLoad = Mass {}

/// What a shelf is rated for, with its length added by mistake.
function rated(shelf_span: ShelfLength, load: ShelfLoad) -> ShelfLoad {
    return ShelfLoad(load + shelf_span)
}
```

```text output
error: `ShelfLoad` and `ShelfLength` can't be added: a `Mass` and a `Length` are different kinds
 --> main.jadpo:7:22
  |
7 |     return ShelfLoad(load + shelf_span)
  |                      ^^^^^^^^^^^^^^^^^ different kinds
  reason: `+` works within one kind of quantity, in any of its units; a `Mass` and a `Length` are different kinds, so the sum means nothing.
  next: Use values of one kind, or a product or quotient the table defines (agent)
  rule: type.arithmetic_operand
  help: docs/language/03-logic.html#type-arithmetic-operand

check failed: 1 error, 1 file
```

### `type.unit_unknown`

A number written with a suffix that is no unit: neither a duration's
(`ms`, `s`, `m`, `h`, `d`) nor a quantity's in question 17's table. Units
are written as SI writes them, and case matters, so a suffix that misses
one only by case names each it was near: `mw` is between milliwatts and
megawatts, and the program can't guess which.

```jadpo check project=e-unit-unknown expect=type.unit_unknown
type HeaterPower = Power {}

/// The fan heater the workshop runs in winter.
function workshop_heater() -> HeaterPower {
    return HeaterPower(2mw)
}
```

```text output
error: `2mw` has no unit `mw`: did you mean `2mW` (milliwatts) or `2MW` (megawatts)?
 --> main.jadpo:5:24
  |
5 |     return HeaterPower(2mw)
  |                        ^^^ unit in the wrong case
  reason: Units are written as SI writes them, and case matters: `mW` is milliwatts and `MW` is megawatts.
  next: Write `2mW` (milliwatts) or `2MW` (megawatts), whichever is meant (agent)
  rule: type.unit_unknown
  help: docs/language/19-standard-library.html#type-unit-unknown

check failed: 1 error, 1 file
```

### `syntax.percent_ambiguous`

A number touching `%` is a percentage, `20%`, and the remainder (area 3)
is written with spaces, `20 % 3`, as the formatter writes every binary
operator. With a number or a name touching the `%` on both sides, either
could be meant, so neither is guessed.

```jadpo check project=e-percent-ambiguous expect=syntax.percent_ambiguous
type BoxCount = Int {}

/// The cakes left over when twenty are packed in boxes of three.
function twenty_in_threes() -> BoxCount {
    return BoxCount(20%3)
}
```

```text output
error: `20%3` could be `20% 3` or `20 % 3`
 --> main.jadpo:5:21
  |
5 |     return BoxCount(20%3)
  |                     ^^^^ a percentage or a remainder
  reason: A number touching `%` is a percentage, `20%`, and the remainder is written with spaces, `20 % 3`, as the formatter writes every binary operator; with `3` touching the `%` too, either could be meant.
  next: Write `20 % 3` for the remainder, or put the percentage `20%` apart from what follows it (agent)
  rule: syntax.percent_ambiguous
  help: docs/language/19-standard-library.html#syntax-percent-ambiguous

check failed: 1 error, 1 file
```

Adding a percentage to money doesn't compile either, by area 3's
`type.arithmetic_operand`, since it could mean the price raised by a
fifth or a fifth of something else; the fix writes both:

```jadpo check project=e-money-plus-percentage expect=type.arithmetic_operand
type CakePrice = Money {
    currency: Currency.gbp
}

/// A cake's price with VAT added.
function with_vat(price: CakePrice) -> CakePrice {
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
  help: docs/language/03-logic.html#type-arithmetic-operand

check failed: 1 error, 1 file
```

### `type.temporal_format_part_missing`

A `LocalTime` is a clock time with no date or zone, and a
`LocalDateTime` a date and a clock time with no zone, so `text.format`
shows each only in a format of the parts it has: a format showing a
date is refused for a `LocalTime`, and one showing a zone for either, as
`type.temporal_format_requires_time` refuses a clock time for a date
(area 10). Nothing is made up to fill the part.

```jadpo check project=e-temporal-format-part-missing expect=type.temporal_format_part_missing
type OpeningSign = PresentationText {}

/// When the shop opens each day, as its sign shows it.
function opening_sign(doors: LocalTime, sign_locale: Locale) -> OpeningSign {
    return OpeningSign(text.format(doors, locale: sign_locale, format: TimeFormat.date_time_short))
}
```

```text output
error: `TimeFormat.date_time_short` shows a part `LocalTime` doesn't have
 --> main.jadpo:5:72
  |
5 |     return OpeningSign(text.format(doors, locale: sign_locale, format: TimeFormat.date_time_short))
  |                                                                        ^^^^^^^^^^^^^^^^^^^^^^^^^^ shows a date
  reason: `LocalTime` is a clock time, with no date or zone, so `text.format` can show only the parts it has.
  next: Use a time format such as `TimeFormat.time_short` (agent)
  rule: type.temporal_format_part_missing
  help: docs/language/19-standard-library.html#type-temporal-format-part-missing

check failed: 1 error, 1 file
```

### `type.variant_retired`

A closed list keeps a code its authority withdraws (question 22), so
stored data and old messages still read, but new code that writes one
is refused: a shipment can't be sent to Yugoslavia. It is declinable,
since code that reads old records names the code on purpose, with
`decline type.variant_retired: "..."` and its reason above the
declaration. A `match` arm on one is reading, and isn't refused.

```jadpo check project=e-variant-retired expect=type.variant_retired
type Consignment = Object {
    delivered_to: Country
}

/// A consignment to a country by its old code.
function to_belgrade() -> Consignment {
    return Consignment {
        delivered_to: Country.yu
    }
}
```

```text output
error: `Country.yu` is a retired code
 --> main.jadpo:8:23
  |
8 |         delivered_to: Country.yu
  |                       ^^^^^^^^^^ only read
  reason: ISO 3166-1 withdrew `YU` (Yugoslavia). `Country.yu` stays so that stored data and old messages still read, but new code doesn't write it.
  next: Use the code that replaced it, or write `decline type.variant_retired: "<why>"` above it if this is deliberate (agent)
  rule: type.variant_retired
  help: docs/language/19-standard-library.html#type-variant-retired

check failed: 1 error, 1 file
```

### `type.reserved_variant_name`

`.all` lists every variant of an enum whose variants carry no data
(question 22), so no enum declares a variant called `all`. A closed list
whose code is `all` writes it with a trailing `_`, as Albania's lek is
`Currency.all_`.

```jadpo check project=e-reserved-variant-name expect=type.reserved_variant_name
type Newsletter = Enum {
    subscribed
    all
}
```

```text output
error: `all` can't be a variant of `Newsletter`
 --> main.jadpo:3:5
  |
3 |     all
  |     ^^^ reserved
  reason: `Newsletter.all` lists every variant of `Newsletter`, so no variant takes the name `all`.
  next: Rename the variant (agent)
  rule: type.reserved_variant_name
  help: docs/language/19-standard-library.html#type-reserved-variant-name

check failed: 1 error, 1 file
```

## Review

### Status

| Feature | Status |
|---|---|
| `list.`, formerly `collection.` | `count`, `contains`, `map` and `filter` decided in area 2; renamed from `collection.` (question 6) and joined by the 21 operations that need no language change in 5.5b, and `zip_with` in 5.5c2 |
| `set.`, `map.`, `range.`; `for` over a set; `[]` as an empty map | Decided 2026-10-17 (question 8); built in 5.5c: the 7 `set` operations, the 8 `map` operations that take no inline function, and the 3 `range` operations on `InstantRange`; built in 5.5c2: `map.filter`, `map.map_values`, and `Range<T>` with open ends, which the `range` operations take |
| `integer.divide`, `decimal.divide` | Decided (area 3) and built |
| `integer` and `decimal`'s other operations, `Rounding.down` | `abs`, `min`, `max`, `from_decimal` and `round` built in 5.5d, with `text.format` for numbers; the maths, from `sqrt` to `pi`, in 5.5e |
| `temporal`, `locale` | Decided (area 10) and built; `occurrences`, `open_at` and `period_between` decided here; `period_between` built in 5.5k, `occurrences` and `open_at` in 5.5k2 |
| The standard types and their contract | Decided 2026-10-17; the contract built in 5.5a over the 13 built types, `Range<T>` in 5.5c2, `Money` and `Currency` in 5.5h, `ExchangeRate` in 5.5h2, 17 payment and identity numbers in 5.5i, the 17 quantities with `DataSize` and `Bitrate` in 5.5j, `Percentage`, `Ratio` and `Fraction` in 5.5j2, ten time types in 5.5k, 15 of people and places in 5.5l and the other four in 5.5l2, each other group of types in 5.5i2 to 5.5s |
| `money.`, the operator table's `Money` rows | Built in 5.5h; `convert` and `ExchangeRate` in 5.5h2 with `{price, number, currency}`; the `Percentage` rows in 5.5j2 with `{rate, number, percent}` |
| `text.parse`, `text.format`, `text.from`; `type.standard_exists` on app code; `temporal.format` moved | Built in 5.5a |
| The other 17 `text` operations; `locale:` on case and `list.sort_by`; NFC at the boundary; `text` reserved | Built in 5.5f; `text.pick` with `LocalisedText` in 5.5o |
| A release that adds a standard type, and `jadpo fix` | Decided (question 4); built in 5.5a2: `type.standard_clash`, `jadpo fix`'s swap and rename, the editor's swap |
| `Email`, `Url`, `IpAddress`, `FileType`, `InstantRange` | Decided (areas 2, 10, 16) and built |
| Everything else on this page | Decided 2026-10-17, as recommended; built in chunks 5.5a to 5.5s |
| `hash.sha256`, the 9 `encoding` operations | Decided 2026-10-17 (question 24); built in 5.5g |
| The format namespaces | File forms decided with batches (area 12), built with them in 6.4; `json.read` and `json.write` in memory decided here, built in 5.5g with `malformed` and `mismatch`, whose first problem's path and rule the failure's report names; the `DecodeProblem` a failure mapped from `mismatch` receives built in 5.5g2 |
| SQL forms | Decided; built with computed backfills (chunk 5.6) |
| Geospatial, AI | Decided as future areas, planned in 5.5u and 5.5v |

### Consistency with the rest of the language

- **Area 1:** `list`, `set`, `map`, `range`, `text`, `markup`, `money`,
  `units`, `hash`, `encoding`, `crypto`, `random`, `json`, `csv`, `tsv`,
  `json_lines`, `colour`, `media`, `web` and `semver` become reserved
  namespaces, and `geo` and `ai` with their areas (`locale` already is).
  `units` rather than `quantity`, since areas 1 and 4 name variables
  `quantity`. Only `text` and `range` clash: for `text`,
  five page examples (area 2's `work_email(text:)`, areas 7's and 8's `var
  text: Memo`, area 9's `edit_note(text:)`, area 16's `search_todos(text:)`)
  and about ten Jadpo snippets in crate tests; for `range`, area 10's
  `deliveries_in(range:)` query, now `deliveries_in(arrivals:)`; all renamed in 5.5f and 5.5c (question
  30). Area 1 carries an amendment note.
- **Area 2:** `collection.` becomes `list.` (question 6, below); the 234 new standard
  types, `EncryptionKey` and `PasswordCheck` become built-in names, and the
  page and example types that reuse one are renamed (question 3 lists
  them); `for` and maps as "Language gaps" says. Area 2 carries an
  amendment note.
- **Area 3:** inline functions may take two parameters for standard
  operations; `any`, `all`, `find`, `min_by`, `max_by`, `sort_by` and
  `group_by` take inline functions. Area 3 question 6's "text order
  belongs in a standard function" is `sort_by` with a text key. No
  user-defined operators is recorded beside area 3's generics decision,
  and the `%` literal beside its remainder operator; area 3 carries an
  amendment note.
- **Area 4:** decoding failures are conditions mapped by name, as area 10's
  `dst:` is, and overflow stays a defect.
- **Areas 5 and 6:** `Money`'s two columns; aggregate queries decided for
  area 6, built in 5.6c; one-step updates unchanged. Area 6 carries an
  amendment note.
- **Area 10:** a secret stays unreadable, with the `key:` of `crypto` added
  to the compiler's places; `random` generated like `clock`; `{price,
  number, currency}` added to the placeholder styles; `temporal.format`
  becomes the time types' `text.format` (question 2). Area 10 carries an
  amendment note.
- **Area 15:** encrypted fields with one application key; per-person keys
  stay out, as decided; data sent to a model listed in its report. Area 15
  carries an amendment note.
- **#88:** its `json.decode` becomes `json.read` given text; its "built in
  or package" table is followed (money, countries, languages, phone
  numbers built in). The format helpers' operations moved here from it and
  from area 12, each of which keeps a pointer.
- **Compiler design** (`docs/compiler-runtime.md`, the standard library):
  the operation table gains `never_empty`, `never_negative`,
  `at_most_length_of`, literal-only `scale:` and `fill:`, conditions no
  option settles (`malformed`, `mismatch`, `unreadable`,
  `currency_mismatch`, `too_many_places`), a `sort_by` key limited to
  ordered kinds, actions-only operations, and a SQL form per operation.

**Renaming `collection`.** Done in 5.5b: the stub is
`crates/jadpo-check/std/list.jadpo`, the operation table's entries are
`list.`, and every call in the pages, the examples and the tests is
renamed. `collection.` gives one error whose edit writes `list.`, `set.`
or `map.` from the first value's type (`semantic.standard_operation_moved`,
under "Errors").

### Chosen in chunk 5.5a

- **The extension point.** A standard type is a stub in
  `crates/jadpo-check/std/standard.jadpo` and an entry in `jadpo-std`'s
  `standard` module (`StandardType`): what it holds, its `TextForm`, its
  `Json` shape, its storage column, its `FormatKey` and the options it
  takes, whether it is ordered, its named `Validator`, the shape
  `type.standard_exists` matches, and the examples the harness reads. A
  stub whose base is its own name is a type built into the language. A
  test holds the stubs and the entries to each other, and the harness has a
  runtime half (`jadpo-runtime`) and a checker half (`jadpo-check`).
- **Canonical text.** An enum's code is its variant's entry in its table:
  a `Zone`'s IANA name, a `Locale`'s BCP 47 tag, a `FileType`'s media type,
  and a `Weekday`'s own name (`monday`), which has no other code. A
  `ZonedTime` is RFC 9557 (`2026-10-25T09:30:00+00:00[Europe/London]`),
  read only with the offset its zone has then; an `InstantRange` is ISO
  8601's interval (`<start>/<end>`), its start no later than its end; a
  `Duration` is the ISO 8601 form JSON already carries, in hours
  (`PT26H`), and an `Instant` ends in `Z`.
- **`text.parse`.** Text that isn't the type at all is `malformed`, an
  address that isn't one included; the rules of the type `as:` names break
  by their own names, as a construction's do. `as:` is a type written out:
  a standard type, or a type built on one or on `Text`, `Int`, `Decimal` or
  `Bool`; text read as text is never `malformed`. An application's enum or
  object is refused, with `type.mismatch`.
- **`text.from`** takes a standard type with a canonical text that isn't
  built on `Text`, and `Int`, `Decimal` and `Bool`; `Email`, `Url` and
  `IpAddress` are text already ("Text", above), and an application's enum
  has no text of its own; each is `type.mismatch`.
- **`text.format`.** `Email`, `Url`, `IpAddress`, `Uuid` and `FileType`
  show their canonical text; a `Weekday` is CLDR's stand-alone weekday
  name, a `Locale` CLDR's display name ("British English"), a `Zone` CLDR's
  generic location format ("UK Time"), a `Duration` CLDR's long unit names
  joined as the locale joins a list of units ("1 hour, 30 minutes"), and an
  `InstantRange` its two ends in its format, joined by ` – `, CLDR's
  English interval fallback, until interval patterns are needed. `zone:`
  and `format:` are the time types' only, as their entries say: a
  missing `format:` is `type.temporal_format_invalid`, and an option a type
  doesn't take `type.temporal_option_unused`. Numbers took no format for
  people until chunk 5.5d ("Chosen in chunk 5.5d").
- **`FileType`'s variants** are the media types the pages name: `png`,
  `jpeg`, `webp`, `pdf`, `csv`, `tsv` and `json`; more come with chunk
  5.5n.
- **Storage.** `InstantRange` is stored as its JSON, as an object and a
  `ZonedTime` are, until objects get named columns with `Money` (chunk
  5.5h).
- **`type.standard_exists`** matches only an `Object` declaration, whose
  fields are exactly a shape's, none of them optional; `InstantRange`'s is
  the first shape (`start`, `starts_at` or `start_at`, and `end`, `ends_at`
  or `end_at`, each built on `Instant`).
- **`text` stayed free** for parameters and variables until chunk 5.5f
  reserved it (question 30); a local called `text` hid the namespace
  meanwhile.
- **Moved operations** are a table in `jadpo-std` (`MOVED`), each reported
  as `semantic.standard_operation_moved` where it is resolved and checked
  as the operation it moved to; chunk 5.5b added `collection.`'s, a whole
  namespace (`MOVED_NAMESPACES`).

### Chosen in chunk 5.5b

- **`check`, not `test`.** `test` is a reserved word (area 13's
  `test "..."`), so `any`, `all` and `find` call their function `check`;
  it is written by position anyway.
- **A namespace moved whole.** `collection` stays a reserved namespace
  with no operations (`MOVED_NAMESPACES` in `jadpo-std`). A call through
  it is `semantic.standard_operation_moved`, with an edit to `list.`,
  `set.` or `map.` read from its first value's type when that value is a
  name or a field, and to `list.` otherwise; it is then checked as the
  `list` operation, and a `set.` or `map.` one as that operation from
  chunk 5.5c.
- **Counts that may be negative** are `type.argument_negative`: a literal
  below `0`, or a value whose type has no `min` of at least `0`. A call's
  result, `list.count`'s included, is a plain `Int` and proves nothing,
  so a count is built as a type with `min: 0` first. `drop_first` and
  `drop_last` take a count that is never negative too, as question 7
  decides for all four.
- **Ordered keys.** `sort_by`, `min_by` and `max_by` take a key built on
  a number, `Text`, `Instant`, `CalendarDate` or `Duration`, or a standard
  type the contract marks ordered, and never `none`; a `ZonedTime` has no
  order of its own, as for `<` (area 3), so it is ordered by its instant.
  `Money` keys come with chunk 5.5h and `locale:` with chunk 5.5f. A key
  that isn't ordered is area 3's `type.ordering_operand`. Keys compare in
  the canonical order (area 2, "Set order"), so text goes by code point.
- **`sum`'s zero.** `list.sum` gives `Int` or `Decimal` by its items'
  representation; the checker fills an implicit `zero` of that
  representation (`Param::implicit`), so an empty list gives `0` or
  `0.00` as its type says. Items whose type has a `min` of at least `0`
  prove the sum is at least `0`, so `Grams(list.sum(...))` needs no arm
  for `min`; `max` still needs one.
- **Named types.** An operation giving `T`, `T?` or `List<T>` keeps the
  items' named type (`Batch`, `Grams`). A named list type isn't kept,
  since `filter` or `drop_first` can break its `min_length`; it is built
  back by construction (area 2).
- **`group_by`** gives `Map<K, List<T>>`, its keys in the canonical order
  and `==` grouping them, so a key may be any type. Its page test waits
  for `map.get` (chunk 5.5c, which added it); the interpreter's and the runtime's tests
  read the map meanwhile.
- **`distinct`** keeps each first occurrence, found through the canonical
  order, which agrees with `==` for values of one type.
- **`flatten`** takes a `List<List<T>>`; collections stay invariant, so a
  list of a named list type isn't one.
- **`list` is reserved** (question 30). No page or example used the name;
  one checker test's action was renamed.

### Chosen in chunk 5.5c

- **The range operations take an `InstantRange`**, the only range type
  built until `Range<T>` came to the contract in 5.5c2; `DateRange` comes
  with the time types (5.5k). Their stubs are
  `crates/jadpo-check/std/range.jadpo`.
- **An empty range holds nothing and overlaps nothing.** An
  `InstantRange` whose start is its end contains no instant, so
  `range.overlaps` is `false` and `range.intersection` `none` for it, as
  PostgreSQL's `&&` and `NULLIF(a * b, 'empty')` give; two ranges that
  only meet (one's end is the other's start) don't overlap either.
- **An `InstantRange` written as an object literal** checks its two
  fields against its data entry's `Instant`s and runs, built from the
  entry's fields; until this chunk it checked as nothing and stopped as
  not built. Its fields take values built on `Instant` only as an
  object's fields do, so a named instant type (`Moment`) is a mismatch,
  as it is for any field declared `Instant`.
- **`for` over a map** is `type.mismatch`, whose edit writes
  `map.keys(<the map>)`; `for` over a set takes the set's item type, as
  over a list.
- **`[]` where a map is expected** is an empty map, whose length a named
  map type's `min_length` checks as a literal's (`type.invalid_literal`);
  a list with items there is `type.mismatch`, since a literal with
  entries is still area 2's to design.
- **Named types.** As for lists, an operation giving `T`, `V?`, `Set<T>`,
  `Set<K>` or `List<V>` keeps the items' named type, and a named set or
  map type isn't kept, since `without` can break its `min_length`.
- **`set` and `map` are reserved** with `range` (question 30); no page,
  example or test used either name. `collection.` on a set or a map now
  checks as the `set.` or `map.` operation it moved to.

### Chosen in chunk 5.5c2

- **Two parameters are written in parentheses,** `(key, value) -> ...`,
  and only that way; one parameter stays `item -> ...`, never `(item) ->`.
  Each takes the type of the value the operation gives in its place:
  `list.zip_with` an item from each list, `map.filter` and
  `map.map_values` an entry's key and its value, in the canonical order of
  the keys. Their stubs write the function `Function<K, V, U>`, the last
  type its result. Neither parameter has to be used, as for one.
- **A different number of parameters than the operation gives** is a new
  rule, `type.inline_function_parameters` (area 3), in both directions; two
  parameters anywhere else is `type.inline_function_placement`, whose
  reason then names the three operations that take two.
- **`Range<T>` holds its start and not its end,** as `InstantRange` does
  and as PostgreSQL's canonical ranges do; `DateRange` will include its
  end, saying so in its own type. An open end (`none`) holds everything on
  its side, so two open ends hold every value; two ranges that only meet
  don't overlap; a range whose start is its end holds nothing.
- **`T` is a type `<` orders:** one built on `Int`, `Decimal`, `Instant`,
  `CalendarDate` or `Duration`. Any other is area 3's
  `type.ordering_operand`, where the type is written or where a literal's
  ends say it; `Range` with no type argument is `type.type_argument_count`.
  `Text` isn't one: `<` doesn't take it.
- **A range literal is `Range { start: ..., end: ... }`,** both ends
  given, `none` for an open one. Where a `Range<T>` is expected its ends
  are checked as `T?`; elsewhere its first end that isn't `none` gives
  `T`. Its ends are read as `range.start` and `range.end`, each a `T?`.
  Nothing checks that the start isn't after the end, as for an
  `InstantRange`, until named rules across fields (5.5t).
- **A `range` call's ranges are all one range type,** since each keeps its
  own end rule: an `InstantRange` and a `Range<Instant>` don't mix
  (`type.mismatch`). A range it gives is that range type, so
  `range.intersection` of two `InstantRange`s is an `InstantRange?`.
  `range.contains`' value is built on the range's item type.
- **The contract.** Its canonical text is each end's canonical text joined
  by `/`, an open end written `..` as ISO 8601-2 writes one
  (`2026-10-01/..`); a start after its end is `malformed`. JSON is an
  object of `start` and `end`, `null` for an open end. Storage is its JSON,
  as `InstantRange`'s is. `text.format` shows each end as its item type is
  shown, with the item type's options, joined by ` – `, and an open end as
  `…`; a range of numbers shows each end as chunk 5.5d shows a number,
  and takes no options.
  `text.parse(raw, as: Range<T>)` reads a range of a standard type or a
  representation written as itself (`Range<Int>`, `Range<CalendarDate>`):
  a range of a type with rules of its own (`Range<Grams>`) is
  `type.mismatch`, since nothing would check those rules on its ends. It
  isn't ordered, and `type.standard_exists` matches no shape for it, since
  `InstantRange` already claims a start and an end. Its data entry's
  examples are ranges of dates (`Examples::of`), and `Holds::Range`,
  `TextForm::Range` and `FormatKey::Range` are the code it added.

### Chosen in chunk 5.5d

- **`min` and `max` take two values of one type,** as `+` does (area 3),
  and give one of them as it is: two named types that aren't one after
  widening are `type.mismatch`; a literal takes the other's type and is
  checked against all its rules, since the result may be the literal; a
  plain number, which carries no type of its own, makes the result the
  plain representation; two literals give it too. Where one type widens
  to the other, the result is the wider.
- **`scale:` is a whole number from 0 to 18,** the range a type's
  `scale` takes (area 2), written at the call. A value is `type.mismatch`,
  as `text.parse`'s `as:` is, a literal below 0 is
  `type.argument_negative`, and any other literal `type.mismatch`. A
  value with no more places than `scale:` is kept as it is, since the
  result has *at most* `scale` places.
- **Facts for proofs.** Besides `abs`'s floor of 0 in the table, `min`
  and `max` carry their arguments' bounds (the smaller of two values is
  never above either's top), and `integer.from_decimal` and
  `decimal.round` keep the sign of a value bounded at zero, since no mode
  rounds across it. Nothing finer: a rounded value's other bounds need a
  check.
- **Of equal values, `decimal.min` and `max` give the first,** so
  `decimal.min(1.50, 1.5)` keeps `1.50`'s places.
- **`text.format` on a number** is CLDR's decimal format for the locale,
  with its digits, grouping and decimal separator, and every place the
  value keeps, as a translation's `{n}` writes it (area 10): `1234.50` is
  `1,234.50` in `en_gb` and `1.234,50` in `de_de`. It takes no options;
  `zone:` or `format:` is `type.temporal_option_unused`. A `Bool` still
  has no format for people.
- **A ratio becomes a `Percentage` by construction,** `Percentage(ratio)`,
  `0.25` giving `25%`, as every value becomes a named type (question 15):
  the page named no conversion, so this is the conservative reading, with
  no `units` operation for it. It is built with `Percentage` in chunk
  5.5j2.

### Chosen in chunk 5.5e

- **A result has exactly `scale` places,** `decimal.sqrt(4.00, scale: 2,
  ...)` giving `2.00`: unlike `decimal.round`, nothing is kept as given,
  so there are no places to keep.
- **`sqrt`'s value and `power`'s exponent are proved never negative,**
  the table's "never negative" read as `never_negative` (question 7): a
  literal isn't below 0 and a value's type has a `min` of at least 0,
  or the call is `type.argument_negative`.
- **`ln`'s `out_of_range` is settled by proof,** as a construction's rule
  is: a literal above 0, or a value whose type has a `min` above 0,
  can't fail, so its call takes no `attempt` (`failure.attempt_infallible`
  if it has one); any other value may, and the call maps `out_of_range`
  or `invalid:`, its failure `invalid_value`.
- **`atan2(0, 0)` is 0,** where no angle is meant, rather than a failure
  the page doesn't list; `atan2(0, x)` for `x` below 0 is π, as `Decimal`
  has no negative zero to make it -π.
- **`power(value, 0)` is 1 at `value`'s scale,** `0^0` included, and
  every other power keeps the places repeated `*` keeps: `1.50` squared
  is `2.25`. Past 38 digits or 38 places is overflow, a defect.
- **`exp` past 38 digits is overflow,** a defect; one so far below 0 that
  it is under half of 10^-18 gives 0, or one unit of the last place for
  `up`, which rounds away from zero.
- **Facts for proofs:** `sqrt` and `exp` are at least 0 and `pi` at
  least 3; `sin` and `cos` carry no bounds, so building a type from one
  checks its rules.

### Chosen in chunk 5.5a2

- **A release** is each data entry's `release`, `major.minor.patch`. The
  13 standard types built before Jadpo's first release carry `0.1.0`, the
  workspace's version, and a later group carries the version it ships in.
  The release notes' list is "Names each release adds", held to the
  entries by a test until releases have notes of their own.
- **`type.standard_clash`** is reported on every declaration named like a
  standard type, whenever it was written: the checker keeps no record of
  the release a project last checked with, so "the first `check` after
  the upgrade" is every `check` until the clash is fixed. It replaces
  `semantic.duplicate_declaration`'s "is a built-in type" for a standard
  type's name (a language type's, `Text` or `Int`, keeps it), it is not
  declinable, since two declarations can't share a name, and a
  declaration it names is never also `type.standard_exists`. While the
  clash stands, a reference to the name means the standard type, as
  before chunk 5.5a2; the errors that gives are the swap's, and the
  rename clears them.
- **The swap is offered on a type only**; a failure, event, email or
  entity named like a standard type is renamed. It deletes the
  declaration (with its comments and `decline` line, and its file when
  nothing else is in it), writes the standard type's name at each
  reference in the files that can see the declaration, renames each field
  the shape matched where it is read (in a body by what the name resolved
  to, elsewhere as `Type.field` is written), and takes the name out of
  `use` lines. Whether the standard type fits is the result's check, not
  a test of shapes beforehand.
- **The rename is `jadpo fix`'s, through the quick-fix code, not a general
  `jadpo rename`.** It writes the new name at the declaration and every
  reference in the files that can see it, `use` lines included, and moves
  the file named after the declaration to the new name's file. It writes
  no identity line (`database/identities.json`), so a rename whose result
  changes stored data or an API record stops on that error. The new name must be an unused `UpperCamelCase`
  name that reads back from its file name, refused otherwise with
  `cli.fix_arguments`. The editor offers only the swap, since a quick fix
  can't ask for a name; its rename comes with `jadpo rename`.
- **A fix never adds an error.** It is checked once made: an error the
  project didn't have before, by rule and headline, undoes it and is
  shown. Errors it already had stay, so one finding is fixed at a time
  even while others, or other errors, stand, and `jadpo fix` says how
  many are left. A swap of a type already stored or in an API record
  therefore stops, until area 18's records bring its migration and
  acceptances.
- **`jadpo fix`'s name is the declaration's,** as its finding names it; a
  name two domains each declare privately is refused, naming both places.
- **A field declared `InstantRange`** has its `start` and `end` read
  through the field, as a value of the type has; chunk 5.5a read them only
  on a value.

### Chosen in chunk 5.5f

- **Empty text is `type.argument_empty`,** a new rule beside
  `type.argument_negative`: a literal `""`, or a value whose type has no
  `min_length` of at least 1, where `never_empty` text is taken
  (`text.replace`'s `find`, `text.split`'s `separator`).
- **`fill:` is required and one code point,** written as a text literal;
  a value, or a literal of any other length, is `type.mismatch`, as a
  `scale:` that isn't written out is. `pad_start`'s `length` is any
  `Int`: one at or below the text's own length gives the text back.
- **`join` takes a list of any text,** its items built on `Text`
  (`List<TagName>`), as `list.sum` takes any list of numbers; collections
  stay invariant elsewhere.
- **`locale:` takes any `Locale`,** not only the ones `locales { }`
  declares: case mapping and collation are Unicode's and CLDR's data, not
  the application's translations. On `list.sort_by` it orders text keys
  only; with any other key it is `type.temporal_option_unused`, whose
  edit removes it. Collation is the locale's default at CLDR's default
  strength, so keys equal by its rules keep their order.
- **Without `locale:`,** `lower` and `upper` are Unicode's full default
  mapping (`ß` becomes `SS`, `İ` becomes `i` and a combining dot), and
  white space is Unicode's `White_Space` property.
- **`slug`** decomposes canonically (NFD) and drops every mark (general
  category M), so `é` becomes `e`; compatibility characters (`ﬁ`, a
  full-width `Ａ`) aren't folded, and become `-` like any other
  character outside ASCII.
- **NFC at the boundary** is every text a request carries, read through
  the route's codecs: body, path, query string, headers and a map's keys,
  for every type built on `Text` (an `Email` included), before its rules
  are checked. Text written in code and text read back from the database
  are kept as they are.
- **`at_most_length_of`** is read from the cut text's type's
  `max_length`, a literal's length, or a cut inside a cut; a count
  written out (`keep_first(name, 12)`) bounds nothing, since question 29
  records the argument only. `split`'s "at least one item" proves no
  `min_length` either.
- **A list written out of values fixes the item type** of a standard
  call it is given to (`list.sort_by([oat, zopf], product -> ...)`); a
  list with a literal in it still takes the parameter's type. Before
  this chunk such a call checked and then stopped at the inline
  function.
- **`text` is reserved,** and with it every standard namespace from the
  chunk that adds it: the list of names kept free (chunk 5.5a) is gone.
  Areas 2, 3, 7, 8, 9 and 16 and six crate test files renamed theirs.

### Chosen in chunk 5.5g

- **What decodes reads only what encodes writes.** `from_base64` takes
  standard, padded Base64 only, its length a multiple of four, and
  `from_base64url` unpadded only, as the page says; both refuse white
  space, the other alphabet's characters and bits after the last byte
  that aren't zero (RFC 4648, 3.5), so no two texts read as the same
  bytes. The wire's `Bytes` (area 2) is the same function both ways, so a
  request, a response and `json.read` read bytes alike: a request whose
  Base64 isn't padded now breaks `format` where it was read before.
  `from_hex` takes an even number of digits in either case; `to_hex` and
  `to_url_component`'s escapes write their own case, lowercase and
  uppercase (RFC 3986, 2.1). None of them can be proved while compiling,
  so `malformed`, and `json.read`'s `mismatch`, are settled only by an
  `attempt`, never by an option or a proof.
- **`hash.sha256` and `json.read` take a value built on `Bytes`, or one
  built on `Text`,** as their `T` in the stubs, so `hash.sha256(email)`
  hashes the address's UTF-8 and `json.read(upload, as: ...)` reads
  bytes that are UTF-8; a parameter written `Bytes` takes any value built
  on `Bytes` (`PhotoBytes`), as one written `Text` takes a value built on
  `Text`. SHA-256 is RustCrypto's `sha2`, pure Rust on every target,
  tested against NIST's FIPS 180-4 examples natively and under
  WebAssembly.
- **"Any type an `Input` may hold" and "any value an `Output` may hold"**
  are read as what the wire can carry: values built on the built-in and
  standard types, enums the application or the standard library
  declares, and `Object`, `Input` and `Output` types made of them, in
  lists, sets and maps, nested to any depth. A failure, an event, an
  email, a message, an entity, `Unit`, a secret (`ApiToken`,
  `ServiceKey`, `PostgresUrl`, `RedisUrl`, `SigningKey`), a
  `PluralCount` and the built-in types later areas define (`Page`,
  `File`, `IssuedToken`) are `type.mismatch` either way, naming the type
  inside that has no JSON form: an entity reaches a client only through
  an `Output` that picks what it sends, and a secret only through a
  route's `Output` or its setting. `as:` is a type written out, never
  `T?`: a document that may be absent is a `Text?` read only when it is
  there.
- **`json.read` reads as a request's body is read,** through the same
  codec (`jadpo-plan`'s `JsonTable`, numbered per call as the route
  table's are): one JSON document with white space around it; text that
  isn't one, or bytes that aren't UTF-8, are `malformed`. A repeated key
  in an object is read as the wire reads one, the last counting, until
  the wire refuses them. The first problem is the first the wire's
  reading finds, an object's fields in declared order before its unknown
  ones; a problem with the whole document has an empty path.
- **The failure's report names the problem** beside its caret ("breaks
  `mismatch` (`min` on `lines.0.count`)"), never its value; since chunk
  5.5g2 the failure can carry it too, under "Chosen in chunk 5.5g2".
- **`json.write` is a response body's text,** compact UTF-8 with
  non-ASCII characters as themselves and only `"`, `\` and the
  characters below U+0020 escaped, so the same value gives
  byte-identical text on native and WebAssembly, which the gate checks
  against one fixed text.

### Chosen in chunk 5.5g2

- **`DecodeProblem` holds `field: Text` and `rule: Text`,** named as area
  7's input problems are, so a client reads the same `{ field, rule }` in
  a failure's `details` as in `invalid_input`'s. The path is the one the
  report names (chunk 5.5g). A map's key and an unknown field's name are
  parts of the path, as area 7 names them for a request; neither is a
  field's value.
- **Only a field written `problem: DecodeProblem` receives it,** by that
  name and that type exactly (not `DecodeProblem?`); a field of another
  name or type is an ordinary field the arm gives. An arm that gives
  `problem` itself is `failure.duplicate_context_field`. A failure that
  declares it, mapped from anything but `mismatch` (`malformed:`, an
  `invalid:` that covers `malformed` too, a construction's rule) or
  raised by `reject`, is `failure.missing_context_field`, its reason
  naming `mismatch`. No error code is new.
- **Only the runtime builds a `DecodeProblem`.** `DecodeProblem { ... }`
  is `type.not_record`, as `IssuedToken { ... }` and `IssuedKey { ... }`
  now are (they checked without an error before), so a problem never
  holds a value a program wrote. It has no JSON form of its own for
  `json.read` or `json.write`, and its name is the language's. Like an
  issued token it is a built-in object: the API record and the OpenAPI
  document list it under its own name, its two fields required and no
  others.
- **The failure's log is its report** until the runtime's hosts write area
  4's `failure.rejected` event: the report shows the problem among the
  fields it is declared in, `client: { problem: DecodeProblem { field:
  "lines.2.quantity", rule: "max" } }`. That event writes a
  `DecodeProblem` as itself, not as a keyed hash, since it never holds a
  value.

### Chosen in chunk 5.5h

- **`Money` is a type built into the language** (`Builtin::Money`), as
  `Duration` is, not an object: `type CakePrice = Money { ... }` takes
  `currency:` (a `Currency` variant), `rounding:` (a `Rounding` variant)
  and `min:` and `max:` as whole numbers of minor units, and an amount
  has no fields; `money.minor` and `money.currency` read it. Like `Int`,
  it is a representation, so a signature names a type built on it.
- **`Currency` is ISO 4217's list one, less the codes with no minor unit**
  (`N.A.`: the precious metals, the SDR, the bond-market units, `XSU`,
  `XUA`, `XTS`, `XXX`), 164 codes with their exponents. Codes ISO has
  withdrawn (`HRK`, `BGN` on 2026-01-01, `SLL`, `ANG`, `ZWL`, `CUC`) and
  `type.variant_retired` come with chunk 5.5l, which builds retired codes
  for every closed list. Albania's lek is `Currency.all_`, since `.all`
  lists an enum's variants (question 22); the wire still writes `ALL`.
- **Canonical text is the code, a space and the major units with exactly
  the currency's places,** `GBP 10.50`, `JPY 500`, `BHD 1.250`, as
  JSR 354 and Joda-Money write it; `text.parse` reads the code ignoring
  case and refuses any other number of places as `malformed`.
- **`+`, `-` and `/` of two amounts keep area 3's named-type rule:** both
  of one named type, or one a plain amount an operator or a `money`
  operation made. A currency both sides fix differently is reported
  first, as `type.arithmetic_operand` or `type.ordering_operand`, no new
  rule. Operators give a plain `Money`, and the checker carries what it
  knows of its currency (a type's `currency:`, a currency written out in
  `money.from_minor`, `of` or `sum`, one currency through `+`, `-`, `*`
  and `/`, and a condition that compares currencies) as it carries a
  number's range.
- **`==` and `!=` never fail:** two amounts in different currencies are
  unequal, as the contract's `==` says. Only `<`, `<=`, `>` and `>=` need
  one currency.
- **An `attempt` around an operator chain covers every operator in it,**
  `attempt a + b + c { currency_mismatch: ... }`, but not one inside a
  call. In an `attempt`, a block straight after a local's value
  (`line.amount { ... }`) is the attempt's mappings, not a value built
  from a block, since only a type builds one; `attempt` still starts a
  statement, so an ordering under `attempt` is a `var`.
- **The money is written first:** `price * 2`, never `2 * price`, which is
  `type.arithmetic_operand`, whose next step writes it the other way; `%`, `-price` and
  `price * price` are refused. `Money / Money` takes divisors as `/` does
  (a type with a `min` of at least 1) and gives `/`'s `Decimal`.
- **`money.split`'s `parts` is never 0 and never negative,** proved as a
  divisor and a count are; `money.allocate`'s ratios are a list of whole
  numbers that is never empty (`type.argument_empty`) and whose items are
  never negative (`type.argument_negative`). Every ratio 0 is a defect, as
  dividing by zero would be. The leftover minor units go one each to the
  first shares whose ratio isn't 0, so a share of ratio 0 is always
  nothing; a negative total shares out as a payment does, every share
  negative.
- **`money.sum` checks every item against `currency:`,** which is also the
  empty list's zero, and needs no arm when the items' type fixes that
  currency; `money.of`'s `too_many_places` needs none for a currency
  written out and an `Int`, a literal with no more places, or a type whose
  `scale` is no more than the currency's exponent.
- **Rounding:** `*` and `/` round to a whole minor unit by the left
  amount's type's `rounding:`, `half_even` when it declares none; the
  plan gives each such operator its mode. `text.format`'s
  `precision: MoneyPrecision.whole` rounds a half away from zero, as the
  page's `£1,235` shows; the default is `MoneyPrecision.minor`, the
  currency's places as CLDR gives them for display.
- **Options:** `MoneyPrecision` (`minor`, `whole`), `MoneyDisplay`
  (`symbol`, `code`, `name`) and `MoneySign` (`standard`, `accounting`)
  are built-in enums; `text.format` gives an amount through ICU4X's
  currency formatter, and a `Currency` its CLDR display name ("Japanese
  Yen"), through `icu_provider`, now a direct dependency at ICU4X's pinned
  release.
- **The wire's amount is always a JSON number,** as Stripe sends it,
  however large; the currency is read ignoring case and written in
  capitals. OpenAPI shows an object of `amount` (with the type's bounds)
  and `currency` (an `enum` of the one code when the type fixes it).
- **Storage is general for compound standard types:** an entry's storage
  is one column or named columns (`Storage::Columns`; `Money`: `amount
  bigint` and `currency char(3)`), turned into a value and back by one
  codec (`standard::to_parts` and `from_parts`), and a field is
  stored as `<column>_amount` and `<column>_currency`, always both, and a
  read or write passes each column through the plan (`Column::Compound`
  and `Part`, `WriteSource::Part`). When the type fixes the currency, a
  check constraint holds the currency column to that code
  (`<table>_<column>_currency_check`), and a migration adds or drops it as
  the type changes. A query comparing or ordering by an amount, a
  one-step update working one out, and a backfill of a new required one
  aren't built yet; each says so.
- **`Currency` columns hold the ISO 4217 code,** `GBP`, as the wire
  sends it, so the database reads as the API does: a `Currency` field's
  column, a `Money` field's currency column, a query's condition and a
  backfill all write the code. Other enums still hold the variant's name;
  whether every standard enum should hold its code is chunk 5.5w's.
- **Standard enums are spelt by their entry's codes** on the wire, in
  settings and in OpenAPI (`Spelling::Codes`), rather than a list of
  enums by name, so `Currency.gbp` goes out as `GBP`. A standard object
  type (`InstantRange`) is described in OpenAPI as the object its entry
  sends, and the per-type harness holds every entry's description to its
  JSON shape.
- **An amount holds at most 2^53 - 1 minor units either side of zero,**
  JavaScript's largest safe integer, so the wire's `amount` is always a
  JSON number a client reads exactly, as question 16 fixes it, never the
  string area 2 sends a large `Int` as. Making or working out one past it
  is overflow, a defect, as for `+` on `Int`; JSON past it is refused as
  `max`, and canonical text past it is `malformed`.
- **`Display` writes an amount as code makes one,**
  `money.from_minor(1050, Currency.gbp)`, in reports and failed
  assertions, since there is no literal for money.
- **Area 2's renames:** `PriceTag` and `OrderLine` for its `Money`
  objects, its `Currency` enum gone, and crate tests the same; example
  09's library `money` became `pricing`, since `money` is now a reserved
  namespace.

### Chosen in chunk 5.5h2

- **`ExchangeRate` is a standard object type**, the catalogue's shape:
  `from` and `to`, each a `Currency`, `rate`, a `Decimal`, and `at`, the
  `Instant` it was quoted at, built with its object literal and read by
  its fields. One `from` is `rate` of `to`, as a currency pair is quoted
  (`GBP/EUR 1.17`: £1 is €1.17). Its JSON is an object of the four
  fields, each in its own canonical text, and it is stored as that JSON,
  as `InstantRange` is. Its shape is matched by `type.standard_exists`
  (`from`, `to`, `rate` and `at`, by their common names).
- **Its canonical text is the pair, the rate and the instant,** `GBP/EUR
  1.17 2026-10-18T09:00:00Z`: the ISO 4217 codes base first, joined by
  `/` as a currency pair is written, the rate with the places it was
  given, and the instant in UTC; codes are read ignoring case. No
  standard writes a rate with its time, so this is the narrowest text
  that round-trips. `text.format` shows that text, the locale changing
  nothing, as for a `Uuid`.
- **A rate is any `Decimal`**, not checked to be above 0: no rule a field
  can carry says "greater than", and a rate is a provider's data; a rate
  between a currency and itself is allowed.
- **No type fixes a rate's currencies.** An inverted rate is a type error
  where the checker knows both currencies, as for `+` on two amounts: the
  amount's from its type or how it was made, the rate's `from` and `to`
  from its object literal written out or from a condition (`rate.from ==
  Currency.gbp`, `rate.from == money.currency(price)`), which also settles
  `currency_mismatch` with no `attempt`. It is reported as
  `type.mismatch`, no new rule. Elsewhere the call fails
  `currency_mismatch` under `attempt`. The result is in the rate's `to`
  where that is known, so building a type that fixes that currency needs
  no arm. A `from:` and `to:` on a type built on `ExchangeRate`, as
  `Money` has `currency:`, would carry the currencies in signatures; it
  is left out until an app needs it.
- **`money.convert` rounds half to even** to the `to` currency's minor
  unit, whatever the amount's type's `rounding:`, as the operation table
  says: £10.50 at 1.17 is €12.285, so €12.28.
- **`{price, number, currency}` is the one placeholder style, written
  exactly so,** for a field built on `Money`, in a message's wording or an
  email's subject: `{price}` for an amount, or the style on any other
  field, is `translation.placeholder_style`, whose edit writes the right
  one; any other style (`{price,number,currency}`, `{n, number,
  percent}` until `Percentage` in 5.5j2) is still
  `translation.wording_invalid`. It shows the amount as `text.format`
  does with its default options, in the locale the message is written in.
  A failure's `client:` fields still can't be money: the client app
  formats those, and the export has no money format yet.

### Chosen in chunk 5.5j

- **Split.** `Percentage`, `Ratio`, `Fraction`, the `%` literal and the
  operator table's `Percentage` rows are chunk 5.5j2, with the showcase
  `total * 15%`.
- **A quantity is a type built into the language**
  (`Builtin::Quantity(kind)`), as `Money` is: `type ShelfLength = Length {
  ... }` takes `min:` and `max:`, and like `Int` it is a representation,
  so a signature names a type built on it while a `var` may be a plain
  `Length`. Every kind is ordered, `Temperature` and `SoundLevel`
  included.
- **Each kind's unit enum** is `<Kind>Unit`, its variants the units' names
  in full (`LengthUnit.nautical_mile`, `VolumeUnit.us_gallon`,
  `TemperatureUnit.celsius`), one for each suffix in question 17's table,
  plus the cubic metre and a `TemperatureDifference`'s `celsius`,
  `fahrenheit` and `kelvin`, which have no suffix. A unit's code is its
  suffix, which text and the wire name too; the cubic metre's is
  `cubic_metre`.
- **What the table left open about units:** `floz` is the US fluid ounce
  (29.5735295625 ml), since UK labels give millilitres; a degree of angle
  is π / 180 radians to 30 digits; `psi` is a pound-force on a square inch,
  `kcal` the thermochemical 4,184 J, `t` the tonne and `knot` 1,852 m an
  hour. A value is converted exactly where the unit is defined exactly and
  rounded half to even to its kind's scale. A `Mass` was first held to the
  milligram, so `1mcg` was held as nothing; the lead session corrected it
  on 2026-10-18 to the microgram ("Decisions").
- **A literal's suffix decides its kind:** a duration's suffix (`ms`, `s`,
  `m`, `h`, `d`) makes a `Duration`, any other a quantity, matched exactly;
  a quantity literal may be negative (`-40degC`), as a duration may not.
  `-` on a quantity value is refused, as for any value but a number (area
  3), and the quantity is written first: `bracket * 2`, never `2 *
  bracket`, as for money.
- **Area 3's named-type rule holds within a kind:** `+`, `-` and `/` take
  one named type, or a plain quantity an operator or a `units` operation
  made; one kind over itself gives `/`'s `Decimal`; and a divisor's type
  excludes zero, a duration type's `min:` included.
- **Text.** The canonical text is the number in the base unit with no
  trailing zeros, a space and the base unit's code, `2.5 kg`, `1000
  metre`; `text.parse` reads a number, an optional space and any of the
  kind's units by its code, case and all, failing `malformed` for no
  number or no unit and `unknown_unit`, a new condition, for another
  kind's unit or none at all.
- **The wire's number is a string,** `{ "value": "2.5", "unit": "kg" }`, as
  area 2 sends every `Decimal`, where the page showed a JSON number; it is
  written in the base unit and read in any of the kind's units, a JSON
  number being `type` and another kind's unit `unknown_unit`. OpenAPI
  lists the kind's units as the unit's `enum`.
- **`text.format`'s `unit:`** is a unit of the value's kind, the base unit
  when left out. The number is the value in that unit, exact where the
  conversion ends within the places the kind's scale gives in that unit,
  otherwise rounded half to even to one place fewer (`1oz`, held as
  0.028349523 kg, is shown as `1 oz`), in the locale's digits and separators.
  Where ICU4X compiles CLDR's unit names, for lengths, areas, volumes and
  masses, the locale's short pattern places it (`252,62 cm`); for the
  other kinds, the number, a space and the unit's SI symbol (`21 °C`, but
  `90°`), until ICU4X has their names.
- **Storage** is `numeric(38, <scale>)` in the base unit, and a query
  compares (`<`, `==`) and orders by a quantity field as by a number. The
  column's comment naming the unit comes with chunk 5.5w, whose audit the
  roadmap gives it.
- **`units.in`'s `scale:` and `rounding:`** are both given, as
  `decimal.round`'s are. `units.sum` refuses a list of temperatures or
  sound levels (`type.arithmetic_operand`), which don't add, and a list
  written out takes its first item's kind.
- **`Display`** writes a quantity as code makes one, in its base unit,
  `2.5262metre`, or `units.of(0.001, VolumeUnit.cubic_metre)` for a kind
  whose base unit has no suffix.

### Chosen in chunk 5.5j2

- **`Percentage` is a type built into the language**
  (`Builtin::Percentage`), held as the exact fraction it is (`20%` is
  0.20), and like `Money` a representation: a signature names a type
  built on it (`type VatRate = Percentage { min: 0%  max: 100% }`),
  whose `min:` and `max:` are written with `%`, while a `var` may be a
  plain `Percentage`. It is ordered.
- **The `%` literal** is digits and `%` with nothing between, its
  fraction keeping the places written and two more (`12.5%` is 0.125);
  it may be negative (`-5%`), as a quantity literal may. A number, `%`
  and a name or number all touching (`20%3`, `20%rate`) is
  `syntax.percent_ambiguous`, a new rule, with no edit since either
  reading could be meant; the text is read as the remainder so the file
  still parses.
- **A bare number where a percentage goes is refused**, as an argument,
  in a comparison, beside `+` or as a bound, since 0.2 and 20 are the
  confusion the type exists to end: `type.mismatch` (or
  `type.incomparable`, `type.ordering_operand`, `type.arithmetic_operand`
  or `type.setting_value` where it stands), whose next step names both
  readings, `25%` if `0.25` is a ratio or `0.25%`, with no edit.
- **`Percentage(ratio)` takes a `Decimal`,** a value or a decimal literal,
  into `Percentage` or any type built on it, checking that type's rules
  (`Percentage(0.25)` is 25%). A whole number (`VatRate(20)`) or an `Int`
  value is `type.constructor_input`, naming both readings, and a named
  percentage never becomes the plain `Percentage`, as for any
  representation (area 2).
- **"The same kind" for a number is a number:** `Int * Percentage` gives
  a `Decimal`, as `*` by a `Decimal` does (area 3), rather than rounding
  to a whole number. A percentage is written after the amount, number or
  quantity it scales, as a number is (`price * 20%`); `20% * price` and
  `15% * length` are refused with the fix written the other way, while
  `20% * 3` is a percentage, as the table's own row says. A quantity
  times a percentage rounds half to even to its kind's scale, as by a
  number. `+` and `-` of two percentages keep area 3's named-type rule.
- **No other row:** nothing divides a percentage (`rate / 2` is refused;
  `rate * 50%` halves it), adds one to a number or a quantity, or negates
  a percentage value (`-rate`); a quantity and a percentage are reported
  as different kinds.
- **`Money + Percentage`** is `type.arithmetic_operand`, whose next step
  writes both readings with the percentage worked out: `price * 120%` or
  `price + price * 20%` (`price * 80%` or `price - price * 20%` for `-`);
  for a percentage value, `price * (100% + rate)`.
- **Text.** The canonical text is the fraction as a percentage with no
  trailing zeros, `20%`, `12.5%`; `text.parse` reads a number, one
  optional space and `%`, since French and German write `20 %`, and
  anything else is `malformed`.
- **The wire is that text,** `"20%"`, rather than the fraction a `Decimal`
  would send, so no client reads 0.2 as 0.2%; a JSON number is `type` and
  text without `%` `format`. OpenAPI shows a string with its pattern.
- **Storage** is one `numeric` column holding the fraction, 0.15 for
  15%, at whatever places the value has.
- **`text.format`** is ICU4X's percent formatter over CLDR's pattern
  (`icu_experimental`, already pinned): the locale's digits and
  separators, every place the value keeps as a percentage, and the `%`
  where the locale puts it, `12.5%`, `20 %` in `de_de` with a no-break
  space. It takes no options. `Display` writes `20%`.
- **`{rate, number, percent}`** is the style of a field built on
  `Percentage`, as `{price, number, currency}` is money's, shown as
  `text.format` shows it; `{rate}` for one, or either style on a field of
  another kind, is `translation.placeholder_style`, whose edit writes the
  right one. Any other style is still `translation.wording_invalid`.
- **`Ratio` and `Fraction` are standard objects** of the catalogue's
  shapes: a `Ratio`'s `numerator` and `denominator` are `Decimal`s, any
  value, as an exchange rate's is; a `Fraction`'s are whole numbers, its
  `denominator` at least 1. Each is held as written, so `16:9` and
  `32:18`, or `1/2` and `2/4`, are two values, as any object's fields
  compare, and neither is ordered: reducing or comparing by value is an
  operation no page decides. Their canonical texts are `16:9` (each
  number with the places it was given) and `3/4`, with no spaces, read
  back exactly so, a denominator below 1 being `malformed`; each is sent
  as that text and stored as it in `jsonb`, as the time objects are, and
  shown as it whatever the locale. `type.standard_exists` matches an
  object of a `numerator` and a `denominator`, by what they are built on.
  No `Ratio` or `Fraction` operations are decided, so none is built.

### Chosen in chunk 5.5k

- **Split.** `RecurrenceRule`, `OpeningHours` and `CalendarEvent`, with
  `temporal.occurrences` and `temporal.open_at`, are chunk 5.5k2; their
  calls were `toolchain.not_built_yet` until it.
- **The ten types are records of their parts** (`Holds::Object`), as
  `InstantRange` is, so code reads `at.hour` and `week.year`; nothing new
  is built into the language. A field is a built-in type, a whole number
  with a range (`Part::Whole`: `LocalTime`'s `hour` from 0 to 23, a
  year from 0 to 9999 as a `CalendarDate`'s, `Quarter`'s `quarter`
  from 1 to 4, `UtcOffset`'s `minutes` within 18 hours), or another
  standard type (`LocalDateTime`'s `time`). An object literal proves
  each range as an object's field rules are proved (area 2):
  `type.invalid_literal` for a literal, `type.field_unproved` for a
  value, and `LocalTime.hour(raw)` builds one with its `min` and `max`.
  A rule across fields, an `IsoWeek`'s week 53 in a year with 52, waits
  for 5.5t, as `InstantRange`'s start after its end does; its text is
  refused. `Year` is a record of one `year`, since a standard type
  built on `Int` with rules of its own would need declarations the
  project doesn't have.
- **Canonical text** is ISO 8601's, to the second and with nothing
  optional, and only the form written is read: `09:30:00` (never
  `09:30`, `24:00:00` or a leap second), `2026-10-25T09:30:00` with no
  offset, `2026-10`, `2026` in four digits, `2026-W43` (week 53 only in
  a year that has one), `+05:30` (UTC is `+00:00`, never `Z` or
  `-00:00`), and `2026-10-01/2026-10-31` for a `DateRange`. A quarter
  has no ISO 8601 form, so it is `2026-Q4`, upper case. A
  `CalendarPeriod` writes each part that isn't zero, in the order years,
  months, days, with its own sign when negative, as PostgreSQL writes an
  interval (`P1Y-2M3D`), and nothing at all as `P0D`; weeks (`P1W`), a
  `+` and a written zero part are refused, so a period has one text.
- **JSON is the canonical text** for each, as a `Duration`'s is
  (`Codec::Standard`), except a `DateRange`, which is an object of its
  two dates as `InstantRange` is. **Storage** is that JSON in one
  `jsonb` column, as `InstantRange`'s is, rather than chunk 5.5h's named
  columns; chunk 5.5w moves ranges and periods to the native `daterange`
  and `interval` columns the page names.
- **Order.** `LocalTime`, `LocalDateTime`, `YearMonth`, `Year`,
  `Quarter` and `IsoWeek` are ordered, part by part in declared order,
  which is the canonical order; `UtcOffset`, `Month` (an enum, as
  `Weekday` isn't ordered either), `DateRange` and `CalendarPeriod`
  aren't. `<` and `sort_by` take the ordered ones.
- **Shown to people.** `LocalTime` and `LocalDateTime` take `format:`, as
  the time types do, and only a format of the parts they have:
  `type.temporal_format_part_missing` (under "Errors") refuses a date
  for a `LocalTime` and a zone for either, and `zone:` is
  `type.temporal_option_unused`. A `DateRange` shows each date by a date
  format, `type.temporal_format_requires_time` refusing a clock time.
  `Month` is CLDR's stand-alone month name, `YearMonth` and `Year`
  CLDR's pattern for the parts they have ("October 2026"), `UtcOffset`
  CLDR's long localized GMT format ("GMT+05:30", "GMT+00:00"), and
  `CalendarPeriod` CLDR's long unit names joined as a list of units, as
  a `Duration` is ("1 year, 2 months, 10 days", "0 days", each negative
  part with its sign). ICU4X has no quarter or week-of-year patterns
  yet, so `Quarter` and `IsoWeek` are shown as their canonical text.
- **`Month`'s code is its own name** (`october`), as `Weekday`'s is.
- **`DateRange`'s shape** for `type.standard_exists` is a first day
  (`start`, `starts_on` or `start_date`) and a last day (`end`,
  `ends_on` or `end_date`), each built on `CalendarDate`. In the `range`
  operations it holds its end: a range whose start is its end holds that
  day, and two that meet on a day overlap on it. Its fields take values
  built on `CalendarDate` as an `InstantRange`'s take instants, so a
  named date type is a mismatch there.
- **The operator rows.** A date moved by a period keeps the date's own
  named type, as `Instant + Duration` does (area 10); `date - period`
  adds the period with every part negated, so 31 March less a month is
  28 February. The date comes first; `Int * period`, a `Decimal` factor,
  a `ZonedTime` and `<` on periods are `type.arithmetic_operand` or
  `type.ordering_operand`. Past the years a date holds is the fault
  `time_out_of_range`, and a part past 64 bits overflow, a defect.
- **`temporal.period_between`** counts a month once the end's day of the
  month reaches the start's, as PostgreSQL's `age` and `java.time` do
  (30 January to 28 February is `P29D`), takes fewer where clamping would
  pass the end, and gives the rest as days, so `start + period` is `end`
  for every pair, every part of one sign. Going backwards it counts from
  the end, so it isn't always the forward period negated (25 October to
  29 March is `P-6M-27D`, forward `P6M26D`). Where `age` borrows days
  from the start's month and adds back past the end (29 January to 1
  March 2028 is `age`'s 1 month 3 days, here `P1M1D`), its SQL form
  differs; chunk 5.6, its first user, checks the form against the
  runtime before relying on it.

### Chosen in chunk 5.5k2

- **Split.** `CalendarEvent` is chunk 5.5k3. Code reads an event's
  parts, so it is an object, and its location, description and
  recurrence are optional and an all-day event has dates where a timed
  one has times, which a standard object's parts can't say yet (each is
  a built-in type, a whole number or another standard type). Held as
  text here, as the other two are, its parts would be out of code's
  reach.
- **Held as text.** `RecurrenceRule` and `OpeningHours` are types built
  on `Text` that the standard library declares (`Holds::Text`), as chunk
  5.5i's numbers are: a value is its standard text, JSON a string,
  storage one `text` column, construction and a literal checked by its
  validator (broken as `format`), and an application's own type can be
  built on one. Each is one text that calendars and maps exchange and
  store whole, and its parts (`BYDAY`, a rule's spans) aren't fields code
  reads, so chunk 5.5h's named columns, which are for an object's parts,
  don't apply. `==` compares the text, so two rules written differently
  are different values even when they give the same occurrences.
  Neither is ordered, and `text.format` shows each as written, since CLDR
  has words for neither.
- **A recurrence's text** is two lines joined by a line feed, as
  `rrule.js` and `python-dateutil` write one: `DTSTART;TZID=`, an IANA
  zone, `:` and the first occurrence's local date and time
  (`20261026T093000`), then `RRULE:` and the rule in RFC 5545's grammar,
  its parts in any order, each once. Not read: a start in UTC, floating
  or a date only, a carriage return, a folded line, `RDATE` and
  `EXDATE`. `FREQ` is daily, weekly, monthly or yearly; `SECONDLY`,
  `MINUTELY` and `HOURLY` are refused, since a rule within a day is where
  calendars disagree most across a clock change, and a later chunk can
  add them. `UNTIL` is a UTC time, as RFC 5545 requires with a zoned
  start, and is included; `COUNT` and `UNTIL` aren't both given; a part
  RFC 5545's table rules out for the frequency (`BYMONTHDAY` weekly,
  `BYWEEKNO` other than yearly, an ordinal on `BYDAY` other than monthly
  or yearly) and `BYSETPOS` with no other `BY` part are refused. **The
  start must be the rule's first occurrence:** RFC 5545 leaves any other
  undefined, and calendars disagree (libical counts the start anyway,
  `python-dateutil` and `rrule.js` don't), so refusing it keeps every
  reader of the text agreeing.
- **Expansion** is `python-dateutil`'s, in local time, in
  `jadpo_std::standard::recurrence` so the checker and the runtime share
  it: each period (a year, a month, a week from `WKST`, a day) gives the
  days its `BY` parts pick, with RFC 5545's defaults from the start (a
  yearly rule its month and day, a monthly one its day, a weekly one its
  weekday, and its clock time); an ordinal on `BYDAY` counts within the
  month for a monthly rule or one with `BYMONTH` and within the year
  otherwise; `BYSETPOS` picks from the period's set. RFC 5545's examples
  are its tests. A day a month doesn't have (the 31st of a 30-day month,
  29 February) is never made, so it is skipped and not counted, as the RFC
  says.
- **Daylight saving is RFC 5545's** (3.3.5 and 3.8.5.3): an occurrence is
  its local time in the rule's zone; one the clocks skip is left out and
  not counted, and one they show twice is the first. The start itself,
  in a gap, takes the offset before the gap, an hour on (`Dst.compatible`),
  and is still the first occurrence.
- **`temporal.occurrences`' `from:`** is an `Instant`; the occurrences
  are at or after it, in the rule's zone, fewer when the rule ends first,
  and `COUNT` counts from the start. `count` is proved from 1 to 1000
  while compiling as a count's sign is, a literal within them or a value
  whose type's `min` and `max` are, and anything else is `type.mismatch`,
  as a `scale:` outside 0 to 18 is, so no new rule. Without `COUNT` the
  expansion starts near `from` rather than at the start.
- **Opening hours' text** is the part of OpenStreetMap's syntax every
  reader agrees on, then a space and the zone in brackets, as RFC 9557
  writes one: rules joined by `; `, each weekdays (`Mo-Fr`, `Sa,Su`,
  `Fr-Mo` across the week's end) or one date (`2026 Dec 25`), a space,
  and `HH:MM-HH:MM` spans joined by commas, or `off`. An end is up to
  `24:00`, and one at or before its start runs past midnight. Not read:
  `closed`, `24/7`, public holidays (`PH`), months, weeks, ranges of
  dates, sunrise and sunset, comments, and hours past `24:00`. A later
  rule replaces an earlier one on the days both select, as in
  OpenStreetMap; a night past midnight belongs to the day it opens, so a
  day marked `off` doesn't cut short the night before; a day no rule
  selects is closed.
- **`temporal.open_at`** reads the clock on the wall in the hours' zone:
  a span holds the minute it opens, not the minute it closes. On the night
  the clocks go back, a bar open to 02:00 is open both times 01:30 comes
  round, and on the night they go forward the skipped hour never comes.

### Chosen in chunk 5.5k3

- **Its parts,** in order: a `summary`, one line of text; an all-day
  event's `first_day` and `last_day`, `CalendarDate`s, the last included as
  a `DateRange`'s end is; a timed event's `start` and `end`,
  `LocalDateTime`s, and its `zone`; and a `location` (one line), a
  `description` (text over several lines, with no control character but a
  tab or a line feed, which RFC 5545 text can't hold) and a `recurrence`
  (a `RecurrenceRule`), each of which may be `none`. Every part but the
  summary may be `none`, and the rules below say which go together.
- **No `UID` or `DTSTAMP`.** RFC 5545 requires both of a `VEVENT` in a
  calendar file, but they are that file's copy of the event, not the event:
  an identity and the time a calendar store last revised it. A value is the
  event itself, so two with the same parts are equal; an application keeps
  its own identity and times beside one (an entity's `id`), and the chunk
  that writes calendar files writes them from those. A text with either is
  `malformed`.
- **Days or times, never both, and one zone.** An all-day event has both
  days and none of `start`, `end` and `zone`; a timed one all three and
  neither day. A timed event's start and end are in its one zone (a flight
  between zones is two local times in one of them), its end later than
  its start, as RFC 5545 requires; an all-day event's last day is on or
  after its first. A floating time (no zone) and a time in UTC aren't
  read.
- **A recurrence only on a timed event, from its start.** A
  `RecurrenceRule` starts at a local time in a zone, so an all-day event
  doesn't recur until a later chunk lets a rule start on a date (a
  birthday); a timed event's rule starts at the event's own `start` in its
  `zone`, since a `VEVENT` writes one `DTSTART` for both.
- **These rules are proved where a literal shows them,** as an address's
  postal code is (chunk 5.5l2): a part written `none` is known to be
  absent and one written out (text, a variant, an object) known to be
  given, so a literal that breaks one is `type.invalid_literal`, no new
  rule. A part built from a value that may be `none` waits on cross-field
  rules (chunk 5.5t). A value read from text or JSON is checked as it is
  read: text that breaks one is `malformed`, and JSON breaks `format` at
  the part.
- **Its text** is the `VEVENT`'s lines joined by line feeds, as a
  `RecurrenceRule`'s are, in one order: `BEGIN:VEVENT`, `DTSTART`, `DTEND`,
  `SUMMARY`, then `LOCATION`, `DESCRIPTION` and `RRULE` where it has them,
  and `END:VEVENT`. An all-day event's are dates, `DTSTART;VALUE=DATE:`
  and its `DTEND` the day after its last, as RFC 5545 writes it; a timed
  one's are local times with the zone, `DTSTART;TZID=Europe/London:`, as a
  recurrence's start is, and its `RRULE` is the recurrence's rule line.
  Text values escape `\`, `;`, `,` and a line feed as RFC 5545 3.3.11
  does (`\n`, never `\N`). Only that form is read: no carriage return,
  folded line, other property or parameter, or `DURATION` for an end. A
  calendar file's writer (a later chunk) adds the carriage returns and
  folding RFC 5545 asks of a file. An event whose last day is 31 December
  9999 has no day after it to write, the fault `time_out_of_range`.
- **JSON is its parts,** each in its own canonical text and `null` where
  it is `none`: `{"summary": "Half-term walk", "first_day": "2026-10-24",
  "last_day": "2026-10-26", "start": null, ...}`, a local time
  `"2026-10-24T22:00:00"`, the zone `"Europe/London"`, the recurrence its
  two lines.
- **Stored a column per part** through chunk 5.5h's named columns, each
  that may be `none` a column that may be `NULL`: the days in `date`
  columns, which no reader's zone moves; a local time as its canonical
  text in a `text` column, as a time type is stored as its text until
  chunk 5.5w's native columns (`timestamp` without a zone); the zone its
  IANA name; the recurrence its text.
- **Shown to people as its canonical text,** as a recurrence is: CLDR has
  no pattern for an event, and an application shows the parts it wants,
  each by its own format (`event.first_day` by a date format).
- **Not ordered;** `==` compares part by part. No shape:
  `type.standard_exists` matches no application's event type, since `{
  title, start, end }` is too common to be distinctive (question 5). It
  isn't personal data of itself.

### Chosen in chunk 5.5i

- **Split.** The 17 that are text are built here; the 15 that are
  records or choices of other types wait for what they hold, as chunk
  5.5i2: `Price`, `UnitPrice`, `TaxRate`, `Discount`, `BasisPoints` and
  `InterestRate` for `Percentage` and the quantities (5.5j), `VatNumber`,
  `TaxId`, `PassportNumber`, `DrivingLicenceNumber` and
  `IdentityDocument` for `Country` (5.5l), and `CardBrand`,
  `MaskedCardNumber`, `CardExpiry` and `BankAccount` with them.
- **Held as text.** Each is a type built on `Text` that the standard
  library declares (`Holds::Text`, its stub `public type Iban = Text
  {}`), so construction, literals checked while compiling, `attempt`,
  JSON (a string), storage (a `text` column) and `==` are area 2's, and
  an application's own type can be built on one, adding rules: `type
  SterlingIban = Iban { pattern: "GB.*" }`. Its pattern, matched whole,
  and its validator are one check, which text of another type breaks as
  `format`, as an `Email`'s does, so no new rule name; `text.parse`
  fails only `malformed`, `text.from` is `type.mismatch` (it is text
  already), and none is ordered.
- **Canonical text is the electronic form,** upper case with no spaces
  or dashes: `GB29NWBK60161331926819`, sort code `089999`, SSN
  `123456789`, EIN `123456789`. Only that is read, so the printed IBAN
  and lower case are refused, as `text.parse` reads only canonical text
  everywhere; an application strips a form's spaces before building one
  (`text.replace`). The catalogue's sketches of the SSN and EIN with
  dashes are the printed form.
- **Shown to people in the printed form,** which the locale doesn't
  change (`FormatKey::Grouped`): an IBAN in fours, a sort code in pairs
  with dashes (`08-99-99`), a National Insurance number in pairs
  (`AB 12 34 56 C`), an NHS number 3-3-4 (`943 476 5919`), an EIN 2-7;
  the others as their canonical text. `text.format` is given the name of
  the standard type a value is held as (an implicit `standard`
  argument the checker fills), since the runtime sees only text.
- **Masked by `text.format`, the conservative choice:** a
  `PaymentCardNumber` and a `UsSsn` are shown as their last four only
  (`•••• 4242`, `•••-••-6789`), as PCI DSS lets a card number be shown
  and as the SSA and IRS truncate an SSN. Code that must show a whole
  number has it as text already.
- **Personal data.** Each entry names the area 15 category its values
  are, whatever field holds them: `financial` for a card number, an IBAN
  and a UK account number, `government_id` for a National Insurance
  number, an SSN and a UTR, and `health` for an NHS number. Area 15 isn't
  built yet (personal data comes after the first build), and logs carry
  no values today (a failure's code and a fault), so nothing writes one in
  clear; its build treats a field of such a type as marked with that
  category, hashed in logs, kept out of caches when sensitive, and its
  reads recorded. A failed assertion in `jadpo test` still shows the
  value it compared, which is test data.
- **Validators** are the page's: IBAN's mod-97 with each country's
  length from SWIFT's IBAN Registry (a country the registry adds later is
  refused until a release adds it, as a currency is), LEI's ISO 7064 mod
  97-10, a card number's Luhn, an ISIN's Luhn over its letters as
  numbers, an ABA routing number's 3-7-1 checksum, an NHS number's mod-11
  (a check digit of 10 is never issued), and a National Insurance
  number's prefix rules from HMRC's NIM39110 (suffix A to D only). The
  rest are patterns: a BIC (its country checked against ISO 3166 once
  `Country` is built), a card BIN of 6 or 8 digits, a payment reference
  of up to 18 of Bacs's characters, an invoice number of 1 to 200
  characters with no control character and no space at either end, a
  sort code, an account number, a UTR, a company number, an EIN, and an
  SSN with the areas, groups and serials the SSA never issues refused.
  The UK modulus check needs a sort code and an account number together,
  so it is `BankAccount`'s, in 5.5i2.
- **Test values are published ones** where a scheme publishes them
  (SWIFT's registry IBANs, Stripe's test cards, the NHS Number standard's
  example and the 999 test range, Vocalink's modulus checking test cases,
  GLEIF's LEIs), and made up to the rules where it doesn't; each entry
  says which.

### Chosen in chunk 5.5l

- **Split.** 15 of the 19 are built here; `PhoneNumber`, `CallingCode`,
  `PostalCode` and `UnLocode` are chunk 5.5l2, since the first two share
  libphonenumber's metadata and the last two each need a list per
  country. Until then a `PostalAddress`'s `postal_code` is a line of text.
- **The lists, and where they come from** (each shipped with the release,
  generated once into `jadpo_std::standard::places_data`):
  - `Country` is ISO 3166-1's 249 assigned codes as CLDR 48.2 carries
    them (its regular regions less the exceptionally reserved `AC`, `CP`,
    `CQ`, `DG`, `EA`, `IC` and `TA` and the user-assigned `XK`), with
    alpha-3 and numeric codes from CLDR's code mappings. Its retired
    codes are the eleven ISO 3166-3 codes CLDR marks deprecated (`AN`,
    `BU`, `CS`, `DD`, `FX`, `NT`, `SU`, `TP`, `YD`, `YU`, `ZR`), shown by
    ISO 3166-3's English names, since CLDR names none of them.
  - `CountrySubdivision` is ISO 3166-2 as CLDR 48.2 carries it: 5,027
    codes, and the 626 CLDR marks deprecated, retired.
  - `Language` is ISO 639-1's 183 codes, from the Library of Congress's
    ISO 639-2 list (2026-10-18), its alpha-3 form ISO 639-2/T; `in`, `iw`,
    `ji`, `jw` and `mo` are retired. A language with only a three-letter
    code isn't in the list.
  - `WritingScript` is ISO 15924's 224 codes (2026-10-18) less the
    private-use range, its numeric form ISO 15924's number.
  - `Continent` is UN M49's five regions and Antarctica, which M49 lists
    beside them: the authority's list, the Americas one region. Its codes
    are digits, so each variant's code is its own name, as a `Weekday`'s
    is, and M49's number is its numeric form.
  - `TextDirection` is the enum `locale.direction` already gave, its codes
    `ltr` and `rtl` as HTML's `dir` and its wording write them.
  - `Nationality` takes `Country`'s codes and names, as a passport names
    a nationality by its country: CLDR has no names for nationalities,
    and ICAO's codes for travel documents are `IdentityDocument`'s.
  - `Currency` gains the six codes ISO 4217 withdrew since 2021 (`ANG`,
    `BGN`, `CUC`, `HRK`, `SLL`, `ZWL`), retired, as chunk 5.5h left them.
- **Variants** are the code in lower case, and a code that is one of area
  1's reserved words or `all` takes a trailing `_` (`Country.in_`,
  `Language.or_`), as decided.
- **`.all`** lists an enum's variants in its order (a closed list's by
  code) as a `List` of the enum, its retired codes left out, since a
  dropdown never offers Yugoslavia. On an enum whose variants carry data
  it is `type.unknown_enum_variant`, and a variant called `all` is the new
  rule `type.reserved_variant_name`.
- **A retired code is refused where code writes it as a value**
  (`type.variant_retired`, declinable), comparisons included, which is
  what a decline's reason is for. A `match` arm on one isn't refused: it
  reads, and a `match` lists every variant anyway (area 3).
- **`form:`** is a `CodeForm` written out (`alpha3`, `numeric`): a value
  is `type.mismatch`, and a form the type has no codes in is
  `type.temporal_option_unused`, as an option that means nothing for a
  type is, so no new rule. `text.parse` reads a form's code ignoring case,
  and a code a retired variant shares with a current one reads as the
  current one (`ind` is `Language.id`, not `Language.in_`).
- **At a boundary** a standard enum's code is read ignoring case, as the
  question says, in JSON as in settings.
- **Stored:** a closed list's column holds its code (`GB`, and `IN`
  rather than `in_`), as `Currency`'s does. An object is stored in a
  column per part, `<field>_<part>`, through chunk 5.5h's named columns:
  a part that may be `none` is a column that may be `NULL`, a
  `PostalAddress`'s lines a `text[]`, its country its code.
- **Shown:** a country, a continent, a language and a script by CLDR's
  name in the reader's locale (ICU4X's display names), a withdrawn code by
  its list's English name; a subdivision by CLDR's English name in every
  locale, since CLDR names few in other languages and ICU4X doesn't bake
  them; a continent by CLDR's name for its M49 region, Antarctica by its
  territory's.
- **`PersonName`** is its given name and, where the person has one, their
  family name: a single name is a given name, as CLDR holds a mononym, so
  no cross-field rule is needed. It has no single text form; its JSON is
  its parts. `text.format` applies CLDR's medium, formal pattern for
  referring to a person, from CLDR 48.2's person-name data: the name's
  locale is the reader's when the name is in the reader's script, else
  the likeliest for its script, which then formats it; the reader's
  locale's lists give the order, and its own space between the parts
  (none in Japanese). ICU4X's formatter is experimental, leaves a pattern's
  literals behind ("Ada Lovelace ,") and doesn't derive the order, so
  Jadpo applies the patterns itself.
- **`PostalAddress`** is the catalogue's shape: its lines as written, then
  a locality, a region and a postal code, each one line that may be
  `none`, and its `Country`. The region is text as the envelope writes it,
  not a `CountrySubdivision`; a dependent locality or a sorting code goes
  in the lines, and the recipient is the application's. CLDR has no
  address formats, so `text.format` uses libaddressinput's (Google's,
  from UPU data, Apache-2.0, commit `81eb962` of 2026-05-15): the country's
  own format for a reader of one of its languages and its Latin-script
  format for anyone else, the text between fields only where the fields
  beside it are there, and the country's CLDR name last for a reader in
  another country. Nothing is put in capitals: UPU's capitals are for the
  envelope, not for people reading.
- **`EmailWithName`** is a name that may be `none` and an `Email`. Its
  canonical text is RFC 5322's mailbox with RFC 6532's UTF-8, `Ada
  Lovelace <ada@example.com>`, the name quoted only where it must be and
  the address alone with no name; only that is read. Its JSON is its
  parts.
- **A line**, an object's part of text, is 1 to 200 characters with no
  control character and no space at either end, as an invoice number is,
  proved in a literal while compiling.
- **Held as text:** `GivenName` and `FamilyName` are a line; `Username`
  the catalogue's `[a-z0-9_.-]{1,64}`; `PhoneExtension` up to 20 digits,
  the longest libphonenumber reads after `ext.`; `Uprn` 1 to 12 digits
  with no leading zero, held as text so none is lost.
- **Personal data:** a name, a given or family name and a username are
  `identifying`; a mailbox, an address, an extension and a UPRN
  `contact`; a nationality `demographic`. The lists of places aren't.
- **No shapes:** `type.standard_exists` matches none of the new objects,
  since `{ name, email }` and `{ first_name, last_name }` are too common
  in applications' own types to be distinctive (question 5).
- **Area 6's rename:** `query.include_depth`'s `Country` entity is
  `Territory`.

### Chosen in chunk 5.5l2

- **Held as text,** as the payment numbers are (chunk 5.5i): each a
  pattern and, where it needs one, a named validator, its canonical text
  the value; none is ordered.
- **`PhoneNumber`** is its E.164 text, `+447400123456`, and is valid as
  libphonenumber's `isValidNumber` says, the stricter of its two levels:
  the national number matches one of its region's number types (fixed
  line, mobile, freephone ...) with that type's lengths, not merely a
  length the region uses. Its metadata is libphonenumber's
  `PhoneNumberMetadata.xml`, release v9.0.41 (2026-10-08, Apache-2.0),
  generated once into `jadpo_std::standard::phones_data`; a range a
  regulator opens later is refused until a release brings newer
  metadata, as a currency is. It is `contact` personal data.
- **`text.parse` reads E.164 only,** as it reads only canonical text
  everywhere; **`country:`**, a `Country` value, is the one way to read a
  number as people write it: national with its prefix (`07400 123456`),
  or international after `+` or the region's international prefix (`00
  44 ...`, `011 44 ...`), a national prefix after the code left out (`+44
  (0)7400 ...`), with spaces, dashes, dots, slashes and brackets
  anywhere. A number valid in another region of the same calling code is
  taken (a Jersey number typed in Britain). An extension is
  `PhoneExtension`'s, so text with one is `malformed`. `country:` on any
  other type is `type.temporal_option_unused`, as an option that means
  nothing is, so no new rule.
- **`text.format` chooses the format by the locale,** with no option: the
  national format, `07400 123456`, for a reader whose region (the
  locale's, or its likely one, as an address's reader's is) has the
  number's calling code, and the international format, `+44 7400 123456`,
  for any other, by libphonenumber's formats for the code's main region.
- **`CallingCode`** is a code ITU-T assigned, with its `+` (`+44`, `+1`
  for the whole North American plan, `+800` and the other
  non-geographic codes), the list libphonenumber's metadata holds; not
  personal data.
- **`PostalCode`** on its own is text any country's postal code could be:
  capitals, digits, spaces and dashes, 1 to 12, a letter or digit at each
  end. Its country's rule applies where the country is known: in a
  `PostalAddress` and in `text.parse(raw, as: PostalCode, country:)`.
  The patterns are libaddressinput's (the commit chunk 5.5l's formats
  come from, Apache-2.0), in `postal_codes_data`, each separator a
  pattern leaves optional made fixed where the country's own examples all
  write it one way (a space in `SW1A 1AA`, a dash in `154-0023` and
  `22162-1010`, none in `75004`), so one postal code has one text and `==`
  holds. With `country:` it is read as typed: capitals, one space between
  words, the separators where the country writes them (`sw1a1aa` is `SW1A
  1AA`), refused where two readings fit. A country libaddressinput has no
  pattern for (Ireland, whose Eircodes it doesn't hold) takes any postal
  code. It is `contact` personal data, as an address is.
- **`PostalAddress`'s postal code is a `PostalCode` that fits its
  country.** An object literal never fails (area 2), so the rule is proved
  where a literal writes both the postal code and the country
  (`type.invalid_literal`), and checked when an address is read from JSON,
  in a request or `json.read`, breaking `format` at `postal_code`. One
  built in code from values isn't checked until cross-field rules (chunk
  5.5t) make it a named rule, proved or attempted. Stored addresses are
  read as written.
- **`UnLocode`** is its five characters with no space, `GBLON`, its
  country a current ISO 3166-1 code, and printed with the space, `GB LON`.
  The UN/LOCODE list isn't shipped: UNECE's site terms allow only
  personal, non-commercial use without redistribution, while its footer
  says UN/CEFACT standards are under CC BY 4.0, and which covers the code
  list isn't stated (as the `datasets/un-locode` package records it).
  Checking against the list waits on that.
- **No shapes:** `type.standard_exists` matches none of the four, each a
  single value.
- **Area 2's** `PostalCode` example type is `ShippingCode`.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Shared vocabulary types | The standard library defines a type wherever two packages would otherwise each invent their own, even when the heavy operations live in a package or a later area; each has one construction that validates, one wire shape and one storage shape; positional pairs are designed out by named fields, and an axis order exists only in a conversion that names it (`geo.from_lng_lat`) (Richard) | 2026-10-17 |
| Packages use the standard types | A package's public `Object` whose only fields match a standard type's shape, by the common field names in any order (table under "Packages use the standard types"), is the warning `type.standard_equivalent`, whose `next:` is to use the standard type and convert at the edge; a warning on #88's publishing bar, not a refusal (Richard). Superseded the same day by question 1: the declinable error `type.standard_exists`, for app code too | 2026-10-17 |
| Standard types are global, in one flat set | Every standard vocabulary type is a global name, with no split between core types and types inside namespaces; functions stay in namespaces. Apps name their own types for purpose and shape (`DeliveryStop { address: PostalAddress }`, `BillingContact`), never re-declaring a standard one (Richard) | 2026-10-17 |
| The standard types | 247 types in 18 groups (245 as adopted, with `CalendarPeriod` and `UkCompanyNumber` added after review) (table under "The standard types"); units folded into their quantity type, roles into the general type, duplicates merged, formats and application entities out; country-specific identifiers are global types; gender and sex at birth stay out; a quantity is stored in its SI base unit, with the unit chosen at construction and display; the catalogue records the decision for each of the 1,120 brainstormed entries (Richard) | 2026-10-17 |
| Every standard type meets one contract | Every standard type has a library with the same baseline: construction and validation from its parts and from text with failures named by rule; formatting for people as `PresentationText`, through the locale where it matters; a canonical text form where one exists; one JSON shape and one storage shape; `==`, and order where the type is ordered; conversions at named edges. Some groups add extras (Richard) | 2026-10-17 |
| `Point` | The geographic point is `Point`, the one exception to the naming rule (Richard) | 2026-10-17 |
| `Length` | The physical length is `Length`, not `PhysicalLength`: a second exception to the naming rule, beside `Point`, so quantities read naturally (`var shelf: Length = 2metre`) (Richard) | 2026-10-17 |
| `Colour` | A standard type with named channels in one canonical colour space; conversions to and from other spaces are extras. The channels, the space and the extras are question 13's (Richard) | 2026-10-17 |
| `type.standard_exists` (question 1) | A type whose shape is a standard type's is the declinable error `type.standard_exists` (renamed from `type.standard_equivalent`), in packages' public types and app code alike; it stops `check` and the build until the type is swapped (in one step when not stored or on the wire, otherwise with its data migration through `ApiChange` and `DataChange` acceptances) or declined with area 1's `decline type.standard_exists: "<reason>"` line, which an agent or a person may write and `jadpo approval` lists. On app code in 5.5a after 5.4b; on packages with their publishing bar in 7.1 (Richard, as recommended) | 2026-10-17 |
| The contract, generated (question 2) | Each standard type is a stub declaration in `crates/jadpo-check/std/` plus a data entry (canonical text grammar, JSON shape, storage columns, CLDR format key and options, order, conversions) and at most one named validator; one implementation of each baseline operation reads the data in the operation table, the interpreter and the runtime, and one harness tests every type. Calls are `text.parse(raw, as: T)`, `text.format(value, locale:)` and `text.from(value)` over every type; area 10's `temporal.format` becomes the time types' `text.format`, with a habit error whose edit writes it; `format_relative` stays in `temporal` (Richard, as recommended) | 2026-10-17 |
| Precise names (question 3) | A standard type's name is (a) the defining standard's term, qualified until unambiguous, else (b) the noun qualified by what it holds, and (c) never a bare word an app plausibly uses for something else, except a generic container; `Point` and `Length` are the exceptions. Renamed: `CountrySubdivision`, `UnitQuantity`, `SecretText`, `NetworkPort`, `WritingScript`, `WebOrigin`; `TimeInterval` merged into the built-in `InstantRange`. Existing built-in names stay. Page and example types reusing a standard name are renamed in the chunk that builds it (Richard, as recommended) | 2026-10-17 |
| A release adding a standard type (question 4) | Never a silent break: the first `check` after the upgrade reports each clash with the new type and its release, fixed by swapping (as question 1) or renaming (`jadpo rename`); a new `jadpo fix` applies the chosen fix through the editor's code; release notes list added names (Richard, as recommended) | 2026-10-17 |
| Distinctive shapes only (question 5) | The shape check matches only distinctive shapes: at least two fields, at least one domain field name (not `name`, `value`, `id`, `code`, `type`, `text`), field types lining up, no extra fields; single values never match by shape; each matching type lists its signature in the catalogue (Richard, as recommended) | 2026-10-17 |
| One namespace per type (question 6) | `list.`, `set.` and `map.` replace `collection.`, with a habit error; the four built operations renamed (Richard, as recommended) | 2026-10-17 |
| Cutting from either end (question 7) | `keep_first`, `keep_last`, `drop_first`, `drop_last` on text and lists, `n` never negative and proved so; `slice` for the middle; no negative positions (Richard, as recommended) | 2026-10-17 |
| Sets and maps (question 8) | The seven `set` and eight map operations (ten with the two-parameter ones), returning new values; `for` walks a set in area 2's canonical order; maps walked through `map.keys`; `[]` is an empty map where one is expected, a literal with entries left to area 2; no dynamic keys on records (Richard, as recommended) | 2026-10-17 |
| `crypto` (question 9) | Ten operations with no algorithm choices: XChaCha20-Poly1305 with a required `context:`, Argon2id with `PasswordCheck.correct_outdated`, HMAC-SHA-256 checked in constant time, Ed25519, JWT and TOTP checks; keys are opaque settings written `key: config.<name>`, a compiler-owned place (Richard, as recommended) | 2026-10-17 |
| Encrypted fields (question 10) | An `encrypted: true` field setting, specified in area 15, with one application key, kept out of indexes, unique rules, filters, ordering, search and logs; per-person keys stay out (Richard, as recommended) | 2026-10-17 |
| List operations (question 11) | No push or pop; `last`, `at`, `min_by`, `max_by`, `append`, `flatten` and `group_by` added (Richard, as recommended) | 2026-10-17 |
| Geospatial (question 12) | Its own area, scoped as under "Geospatial: a future area"; roadmap chunk 5.5u "Geospatial: the plan" right after 5.5, its build placed by that page (Richard, as recommended) | 2026-10-17 |
| `Colour`'s shape (question 13) | `red`, `green`, `blue`, `alpha`, each 0 to 255, in sRGB, written `#rrggbb` or `#rrggbbaa`; extras `colour.from_hsl`, `from_oklch`, `mix`, `with_lightness` and `contrast` (WCAG 2), all giving sRGB; CMYK a package (Richard, as recommended) | 2026-10-17 |
| JSON in memory (question 14) | `json.read(text, as:)` and `json.write(value)`, one name per direction beside the batch forms; the packages design's `json.decode` is `json.read`; `mismatch` carries the first problem's path and rule; no dynamic JSON value; CSV file-only (Richard, as recommended) | 2026-10-17 |
| Validating mid-program (question 15) | Construction is the way, nothing new for values or records; named rules across an object's fields are an area 2 change, chunk 5.5t (Richard, as recommended) | 2026-10-17 |
| `Money` and the operator table (question 16) | Minor units with the ISO 4217 exponent, stored as a `bigint` and a currency column; `money.from_minor` and `money.of` (failing `too_many_places`); arithmetic through the standard operator table, functions only for `allocate`, `split` and `convert`; a currency mismatch a compile error where both types fix it, else `currency_mismatch` under `attempt`, needing no arm inside an `if` comparing currencies; a `rounding:` setting, `half_even` by default; `/` by an `Int` refused as `type.money_split`; wire `{"amount": 1050, "currency": "GBP"}`; `text.format` in code and `{price, number, currency}` and `{n, number, percent}` in messages; `20%` a `Percentage`, remainder keeping its spaced `%`, `20%3` an error; no user-defined operators, recorded in area 3 (Richard, as recommended) | 2026-10-17 |
| Quantities (question 17) | Always made with a unit; inside a program no unit; a bare number where a quantity is expected is `type.unit_required`, never declinable; the product and quotient table, `==` across units, any other mix an error; wire `{ "value": 2.5, "unit": "kg" }`; case-sensitive suffixed literals as tabled, `m` staying minutes and metres written `5metre`, the longest suffix winning, a near-miss by case an error (Richard, as recommended) | 2026-10-17 |
| Maths on `Decimal` (question 18) | The eight operations, correctly rounded to a named scale, the same on every target; still no `Float` (Richard, as recommended) | 2026-10-17 |
| Randomness (question 19) | A generated `random` namespace, actions only, seeded per test, recorded for replay, with `random.token` and `random.code` (Richard, as recommended) | 2026-10-17 |
| AI (question 20) | Its own area, scoped as under "AI: a future area" (the `ai` capability, typed `generate`, streaming, `Vector` and similarity queries, checked prompts, a fake, limits, area 15 reporting; training out); roadmap chunk 5.5v "AI: the plan" after the geospatial plan (Richard, as recommended) | 2026-10-17 |
| Language gaps (question 21) | Closed row by row as "Language gaps this page depends on" tables them: `for` over sets, the empty map and two-parameter inline functions in 5.5c; parsing and formatting in 5.5a; `locale:` order and case and NFC in 5.5f; opaque keys and `random` in 5.5q; cross-field rules as 5.5t, aggregate queries as 5.6c, keys held as data with encrypted fields (Richard, as recommended) | 2026-10-17 |
| Closed value sets are enums (question 22) | A closed, authority-published, rarely changing list is a built-in enum (the 25 marked "(enum)", `CountrySubdivision` included); large or fast-changing lists stay text with a `pattern:`; variants are the code in lower case with `_` (`Country.gb`), a reserved word taking a trailing `_`; text becomes an enum automatically at a boundary and by `text.parse` mid-program, ignoring case; other code systems through `form: CodeForm.alpha3`; `text.format` gives the CLDR name and `.all` lists every variant of an enum without data; no matching of names; retired codes stay readable, and writing one is the declinable error `type.variant_retired` (Richard, as recommended) | 2026-10-17 |
| Code points (question 23) | Text is counted and cut in Unicode code points; graphemes left out (Richard, as recommended) | 2026-10-17 |
| A digest is `Bytes` (question 24) | `hash.sha256` gives `Bytes`, written as text by `encoding` (Richard, as recommended) | 2026-10-17 |
| `Rounding.down` (question 25) | `Rounding` gains `down`, toward zero, with SQL form `trunc`; `floor`, `ceiling` and `half_down` stay out (Richard, as recommended) | 2026-10-17 |
| Slugs and text order (question 26) | `text.slug` is ASCII-only; text sorts in code points unless `locale:` says otherwise (Richard, as recommended) | 2026-10-17 |
| No gates or bits (question 27) | No logic gates beyond `and`, `or`, `not` (`!=` is exclusive or); no bitwise operations (Richard, as recommended) | 2026-10-17 |
| SQL forms (question 28) | Used by computed backfills in 5.6, then by aggregate queries in 5.6c (Richard, as recommended) | 2026-10-17 |
| `at_most_length_of` (question 29) | Recorded for `trim`, `slice`, `keep_*` and `drop_*` only (Richard, as recommended) | 2026-10-17 |
| Reserved namespaces (question 30) | Every standard namespace is reserved (`list`, `set`, `map`, `range`, `text`, `markup`, `money`, `units`, `hash`, `encoding`, `crypto`, `random`, `json`, `csv`, `tsv`, `json_lines`, `colour`, `media`, `web`, `semver`, then `geo` and `ai`); fields and named arguments keep every word; clashing names renamed in the chunk that adds each namespace (`text` in 5.5f, `range` in 5.5c) (Richard, as recommended) | 2026-10-17 |
| A `Mass` to the microgram | A `Mass` is held in kilograms to 9 places, `numeric(38, 9)`, so the table's `mcg` is exact (`1mcg` is 0.000000001 kg), as a medicine's dose needs; it was first built to 6, which held `1mcg` as nothing. No other kind has a suffix finer than its scale. A correction made by the lead session, not a question for Richard | 2026-10-18 |
| Data formats moved here | The `csv`, `tsv`, `json_lines` and `json` file operations, decided with batches (area 12 and the packages design, 2026-10-16), are tabled on this page, one shape per namespace with the in-memory `json` forms; the decisions are unchanged (Richard) | 2026-10-17 |

### Open questions

Richard's points first, then the first draft's remaining questions. Richard decided all thirty on 2026-10-17, as recommended ("Trusting your recommendations"); each is a row of "Decisions", above, and where a question offered alternatives, only the recommended one is recorded. The vocabulary-types principle, the package check, the standard types with their rules and contract, `Point` and `Colour` were decided earlier the same day.

1. **Does `type.standard_equivalent` block until it is resolved?**
   Richard's refinement of the decided warning. Detection stops `check` and the build until one of two things is done:
   - **Swap to the standard type.** For a type not yet stored or on the
     wire, the error's fix makes the swap in one step, and an agent may
     apply it. For one already stored or in an API record, the fix writes
     the edit and the data migration, and the change goes through the
     existing `ApiChange` and `DataChange` acceptances (area 18).
   - **Decline, with a reason, on the type itself,** with area 1's
     decided generic line (question 11 there): `decline
     type.standard_exists: "Grid squares, not WGS84 degrees"` directly
     above the declaration. An agent or a person may decline, and `jadpo
     approval` lists every decline with its reason, so declining by reflex
     shows. That line replaces Richard's first form, a `not_standard:`
     option, and needs no reserved field name.

   *Recommendation:* yes, with three choices made:
   - A **declinable error**, as area 1 now makes every judgement call
     (its question 11, decided): there are no warnings left to block.
     Renamed `type.standard_exists`, to read as an error and match
     `library.similar_exists` and `constant.similar_exists`.
   - **Widened** from packages' public types to every type declaration in
     app code too, since an app's own `Location { lat, lng }` reaches its
     API and its tables exactly as a package's would.
   - **The cost is false positives:** an `Object` of `x` and `y` grid
     cells, or an `amount` and `currency` that are a quote, not money,
     matches by shape. The required reason handles them, and review sees
     each one. *Decided 2026-10-17 (Richard):* as recommended.
2. **How is the standard type contract expressed and called?**
   *Recommendation:* generated from each type's declaration and data,
   not written per type. Each standard type is a stub
   declaration in `crates/jadpo-check/std/` (as `types.jadpo` holds the
   built ones) plus a data entry: its canonical text grammar, JSON shape,
   storage columns, CLDR format key and options, whether and how it is
   ordered, its conversions, and the one named validator where data can't
   say it (IBAN's mod-97, Luhn for card numbers, ISBN and NHS check
   digits, E.164's metadata). One implementation of each baseline
   operation reads that data, in the operation table, the interpreter and
   the runtime alike, and one harness tests every type the same way:
   valid and invalid examples per rule, and the round trip through text,
   JSON and storage. A new type is then data plus at most one validator,
   which is how 247 types fit the chunks below. Calls are one name each
   over every type, `text.parse(raw, as: T)`, `text.format(value,
   locale:)` and `text.from(value)`, rather than a `parse` per namespace.
   That makes area 10's decided `temporal.format` the time types'
   `text.format`: move it, with a habit error whose edit writes
   `text.format`, as `collection.` becomes `list.`; `format_relative`
   stays a `temporal` extra. *Decided 2026-10-17 (Richard):* as recommended.
3. **Precise names.** A standard type's name says exactly what it is, so
   it doesn't take a word an app needs for something else.
   *Recommendation:* this rule, in order: (a) the defining standard's
   own term, qualified until it is unambiguous (`PostalAddress`,
   `RecurrenceRule`, `ProblemDetails`); (b) otherwise the noun qualified
   by what it holds (`DateRange`, `DataSize`, `ImageDimensions`); (c)
   never a bare word an app plausibly uses for its own, different thing,
   except a generic container that takes a type (`Range<T>`, as
   `List<T>`). `Point` is the decided exception. Applied to the adopted
   list, 8 names change: `Subdivision` to `CountrySubdivision` (a
   housing subdivision), `Quantity` to `UnitQuantity` (an order line's
   count, area 2's own example), `Length` to `Length` (a text's
   or a video's length), `Secret` to `SecretText` (area 10's `secret:`
   settings), `Port` to `NetworkPort` (a sea port, in logistics),
   `Script` to `WritingScript`, `Origin` to `WebOrigin` (a country of
   origin), and `TimeInterval` merges into the existing built-in
   `InstantRange`. The rest pass, because an app using the word means
   the standard thing (`Price`, `Area`, `Mass`, `Currency`). Existing
   built-in names (`File`, `Page`, `Access`) predate the rule and aren't
   renamed. Page and example types that reuse a standard name are renamed
   in the chunk that builds it: `Money`, `Currency`, `PostalCode`, `Sku`,
   `Dimensions` and `Price` in areas 1 to 4, `ApiKey` and `LogLevel` in areas 10 and 13, `Discount` in area 1
   and example 09, `Allergen` in examples 03, 04 and 12, `Isbn` in
   example 10, `Price` in example 05. *Decided 2026-10-17 (Richard):* as recommended.
4. **A compiler upgrade that adds a standard type** an app has already
   named for itself. *Recommendation:* never a silent break. The first
   `check` after the upgrade reports each clash as an error naming the
   new standard type and the release that added it, with two fixes:
   swap to the standard type when the shapes match (the same swap as
   question 1, data migration and acceptances included), or rename the
   app's type (`jadpo rename`, decided in area 14, keeping stored data).
   A new `jadpo fix` applies whichever the author picks, through the
   same code as the editor's quick fix. Each release's notes list the
   names it adds. *Decided 2026-10-17 (Richard):* as recommended.
5. **Keeping the shape check's false positives low with hundreds of
   types.** *Recommendation:* match only distinctive shapes, and say
   which ones in the catalogue. A shape is distinctive when it has at
   least two fields, at least one field name belongs to the domain
   rather than being generic (`latitude`, `currency`, `iban` count;
   `name`, `value`, `id`, `code`, `type`, `text` don't), the field types
   line up with the standard's (numbers for coordinates, text or an enum
   for a currency), and the declaration has no fields beyond those.
   Single-value types never match by shape: a same-named declaration is
   already an error, and a differently named `Text` says nothing. Each
   standard type that has a distinctive shape lists its match signature
   (fields and their common synonyms) in the catalogue, reviewed with
   it; of the 247, 100 are objects, and only those with a
   domain field name carry one. *Decided 2026-10-17 (Richard):* as recommended.
6. **One namespace per type: `list.`, `set.` and `map.` replace
   `collection.`?** Costs: `list`, `set`, `map` and `text` can't be
   variable or parameter names (`locale` already can't), and the four
   built operations are renamed. *Recommendation:* yes. The namespace says
   what the first argument is, `set.contains` and `list.contains` stay
   distinct where their behaviour differs (order, repeats), a variable
   called `list` says nothing about what it holds, and only `text`
   clashes today. *Decided 2026-10-17 (Richard):* as recommended.
7. **`keep_first`, `keep_last`, `drop_first` and `drop_last` on text and
   lists, with no negative positions?** *Recommendation:* yes, `n` never
   negative and proved so, `slice` kept for the middle. Negative positions
   are where JavaScript has three behaviours for one call. *Decided 2026-10-17 (Richard):* as recommended.
8. **Sets and maps get their operations, `for` walks a set, and an empty
   map is `[]`?** *Recommendation:* yes: the seven set and eight map
   operations, all returning new values; `for` over a set in area 2's
   canonical order; maps walked through `map.keys`; a literal with entries
   left to area 2 (`["gbp": 2]`, Swift's form beside the list's brackets,
   is the obvious candidate). No dynamic keys on records. *Decided 2026-10-17 (Richard):* as recommended.
9. **A `crypto` namespace with no algorithm choices, keys as opaque
   settings?** *Recommendation:* yes, the ten operations as tabled:
   XChaCha20-Poly1305 with a required `context:`, Argon2id with
   `PasswordCheck.correct_outdated`, HMAC-SHA-256 with a constant-time
   check; `key: config.<name>` joins area 10's compiler-owned places. *Decided 2026-10-17 (Richard):* as recommended.
10. **An `encrypted` field setting?** *Recommendation:* yes, to be decided
   in area 15, with one application key and the field kept out of indexes,
   unique rules, filters, ordering, search and logs, as area 16's table
   expects. Per-person keys stay out (area 15, question 5). *Decided 2026-10-17 (Richard):* as recommended.
11. **No push or pop; `last`, `at`, `min_by`, `max_by`, `append`,
   `flatten`, `group_by` added?** *Recommendation:* yes; the page's own
   `first(reverse(sort_by(...)))` showed `max_by` was missing. *Decided 2026-10-17 (Richard):* as recommended.
12. **Geospatial as its own area, and a roadmap entry for it?**
   *Recommendation:* yes, scope as sketched; add **"Geospatial: the plan"
   `opus · high`** right after 5.5 (5.5u), writing the new area page, with its
   build placed by that page. *Decided 2026-10-17 (Richard):* as recommended.
13. **`Colour`'s shape and extras?** Decided: `Colour` is a standard
    type with named channels in one canonical space, other spaces as
    extras ("Decisions"). This question settles which channels, space and
    extras. *Recommendation:* `red`, `green`, `blue` and `alpha`, each 0 to 255,
    in sRGB, constructed from and written as `"#rrggbb"` or
    `"#rrggbbaa"`; extras `colour.from_hsl`, `from_oklch`, `mix`,
    `with_lightness` and `contrast` (WCAG 2's ratio, for checking text
    against its background), all giving sRGB. CMYK needs a colour profile and stays a
    package. *Decided 2026-10-17 (Richard):* as recommended.
14. **`json.read` and `json.write` in memory, one name per direction with
    #88's batch forms, no dynamic JSON value?** *Recommendation:* yes;
    #88's `json.decode` renamed `json.read`; `mismatch` carries the first
    problem's path and rule; CSV stays file-only as #88 decided. *Decided 2026-10-17 (Richard):* as recommended.
15. **Construction is the way to validate mid-program, plus cross-field
    rules?** *Recommendation:* yes, construction as shown, nothing new for
    values or records; and named rules across an object's fields as an
    area 2 change, after 5.5, since the bookings scenario can't be covered
    without one. *Decided 2026-10-17 (Richard):* as recommended.
16. **`Money` (a standard type, decided) in minor units, and the
    standard operator table?** Sub-choices:
    - Storage and construction: *recommendation:* a whole number of the
      currency's minor units with its ISO 4217 exponent, stored as a
      `bigint` and a currency column; made with `money.from_minor(amount,
      currency)`, or `money.of(amount, currency)` from a `Decimal`, which
      fails `too_many_places`.
    - Operators or functions: *recommendation:* operators, through the
      standard operator table, for `+`, `-`, comparisons, `*` and `/`;
      functions only for what no operator says (`allocate`, `split`,
      `convert`). `money.add`, `money.multiply` and `money.divide` don't
      exist.
    - A currency mismatch: *recommendation:* a compile error where both
      types fix the currency; otherwise the condition `currency_mismatch`,
      mapped with `attempt` like a construction rule, and needing no arm
      inside an `if` that compares the currencies.
    - Rounding for `*` and `/`: *recommendation:* a `rounding:` setting on
      the Money type, `half_even` when not declared, matching area 2's
      construction into a fixed scale, rather than making it required on
      every Money type.
    - `/` by an `Int`: *recommendation:* refused as `type.money_split`,
      with a fix to `split` or `allocate`; `/` by a `Decimal` rate allowed.
    - Wire shape: *recommendation:* fixed, `{"amount": 1050, "currency":
      "GBP"}`; a provider's decimal string is mapped in its package.
    - Formatting: *recommendation:* the contract's `text.format` in code
      and `{price, number, currency}` in messages, one formatter; and
      `{n, number, percent}` for a `Percentage`.
    - The `%` literal: *recommendation:* `20%` is a `Percentage`; remainder
      keeps its spaced `%`; `20%3` is an error naming both.
    - No user-defined operators: *recommendation:* recorded as a rule of
      the language, in area 3 beside its generics decision: an
      application's types never declare operators, so every `a + b` is a
      row a reviewer can look up. *Decided 2026-10-17 (Richard):* as recommended.
17. **Quantities: made with a unit, arithmetic by kind, the wire shape,
    and unit-suffixed literals with the `m` clash?** Sub-choices:
    - Construction: *recommendation:* always with a unit, from an `Int` or
      `Decimal` (`units.of`), a suffixed literal, `text.parse("5 km", as:
      Length)`, or another value of the same kind by construction; inside
      a program a quantity has no unit, so metres plus centimetres just
      works.
    - A bare number where a quantity is expected: *recommendation:* the
      compile error `type.unit_required`, never declinable.
    - Arithmetic: *recommendation:* the table under "Physical
      quantities", with `==` and comparisons across units (`1km ==
      1000metre`) and any other mix a compile error naming the two kinds.
    - Wire shape: *recommendation:* `{ "value": 2.5, "unit": "kg" }`, the
      unit named, rather than a bare number in the base unit, which a
      client can't tell apart from another unit.
    - Literals: durations already have `500ms`, `30s`, `5m`, `2h` and
      `30d` (area 10), and `5m` is minutes. *Recommendation:* suffixed
      literals, with `m` staying minutes and metres written `5metre` or
      with `units.of`. Not a meaning chosen by the expected type: a
      reviewer can't see the target type at the literal. Suffixes are
      case-sensitive as SI writes them, the longest suffix wins (`dB`
      before `d`), and a near-miss by case (`5mw`, between milliwatts and
      megawatts) is an error naming both. The suffixes, each
      checked against the duration suffixes and each other:

    | Kind | Suffixes | Clash avoided |
    |---|---|---|
    | `Length` | `mm`, `cm`, `metre`, `km`, `inch`, `ft`, `yd`, `mi`, `nmi` | `m` is minutes; `in` is a keyword (`for x in`) |
    | `Mass` | `mcg`, `mg`, `g`, `kg`, `t`, `oz`, `lb` | `µg` isn't typeable everywhere |
    | `Volume` | `ml`, `litre`, `floz`, `ukgal`, `usgal` | `l` reads as `1`; `gal` alone is ambiguous |
    | `Area` | `sqmm`, `sqm`, `sqkm`, `ha`, `sqft`, `acre` | `m2` puts a digit in a suffix |
    | `Angle` | `deg`, `rad` | `d` is days; longest wins |
    | `Temperature` | `degC`, `degF`, `kelvin` | `K` reads as thousands ("5K") |
    | `Speed` | `mps`, `kmh`, `mph`, `knot` | `ms` is milliseconds |
    | `Pressure` | `Pa`, `hPa`, `kPa`, `bar`, `mbar`, `psi` | `h` is hours; longest wins |
    | `Energy` | `J`, `kJ`, `Wh`, `kWh`, `kcal` | `cal` refused: food labels mean `kcal` |
    | `Power` | `mW`, `W`, `kW`, `MW` | `mW` and `MW` differ only by case |
    | `Frequency` | `Hz`, `kHz`, `MHz`, `GHz` | |
    | `Voltage`, `Current` | `mV`, `V`, `kV`; `mA`, `A` | |
    | `Illuminance`, `SoundLevel` | `lx`; `dB` | `d` is days; longest wins |
    | `Concentration` | `mgL`, `gL` | |
    | `DataSize` | `B`, `kB`, `MB`, `GB`, `TB`, `KiB`, `MiB`, `GiB`, `TiB` | `b` (bits) refused, so bytes and bits can't be confused |
    | `Bitrate` | `bps`, `kbps`, `Mbps`, `Gbps` | |

    `TemperatureDifference` has no suffix of its own; it comes from
    subtracting two temperatures, or from `units.of`. *Decided 2026-10-17 (Richard):* as recommended.
18. **Maths on `Decimal` rather than a `Float`?** *Recommendation:* yes,
    the eight operations, correctly rounded to a named scale, the same on
    every target; area 2's "no `Float` until needed" stands, since this
    meets the need. *Decided 2026-10-17 (Richard):* as recommended.
19. **Randomness as a generated `random` namespace?** This reverses the
    first draft. *Recommendation:* yes, actions only, seeded per test,
    recorded for replay, with `random.token` (your point 6's
    `crypto.random_token`) and `random.code` for references people read. *Decided 2026-10-17 (Richard):* as recommended.
20. **AI as its own area, with the standard pieces sketched?**
    *Recommendation:* yes: the `ai` capability, typed `generate`,
    streaming, `Vector` and similarity queries, checked prompt files, a
    fake for tests, limits, and area 15 reporting; training out. Add **"AI:
    the plan" `opus · high`** after the geospatial plan (5.5v). *Decided 2026-10-17 (Richard):* as recommended.
21. **Are the language gaps closed as "Language gaps this page depends
    on" sets out?** *Recommendation:* yes, row by row: `for` over sets,
    the empty map and two-parameter inline functions in 5.5c; parsing and
    formatting through the contract in 5.5a; `locale:` order and case and
    NFC in 5.5f; secrets as opaque keys and `random` in 5.5q. Three need roadmap entries of their
    own: **"Cross-field rules" `opus · high`** (area 2) after 5.5 (5.5t),
    **"Aggregate queries" `opus · high`** (area 6) after 5.6 (5.6c), and
    keys held as data with encrypted fields (area 15, after the first
    build). *Decided 2026-10-17 (Richard):* as recommended.
22. **Closed value sets are enums?** Sub-choices:
    - The rule: *recommendation:* a standard type is a built-in enum
      when its list is closed, published by an authority and rarely
      changes, as `Locale`, `Zone` and `Weekday` already are (area 10):
      the 22 marked "(enum)" in the standard types table, among them
      `Country` (ISO 3166-1), `Currency` (ISO 4217), `Language` (ISO
      639), `WritingScript` (ISO 15924), `HttpMethod` and `CardBrand`. A huge or
      fast-changing list stays text with a `pattern:`, checked against its
      list where "Groups with the baseline only" says so: `IataAirport`,
      `PostalCode`, `HsCode`, `UnLocode`, `Icd10Code`, `SpdxLicence`,
      `Emoji`. `HttpStatus` stays a number, since its registry is open by
      design (an unknown code is still a status), and `CallingCode` text,
      since its codes are digits.
    - `CountrySubdivision` (ISO 3166-2, about 5,000 codes, a few changes a
      year): *recommendation:* an enum. Validating an address needs the
      list anyway ("Groups with the baseline only"), CLDR names each
      subdivision, its changes arrive with releases as the time-zone
      database's do, and a withdrawn code stays readable (below).
    - Variants in code: *recommendation:* the code in lower case with `_`
      for `-`, as `Locale` writes them: `Country.gb`, `Currency.gbp`,
      `CountrySubdivision.gb_eng`. A code that is a reserved word (area 1) takes
      a trailing `_`: `Country.in_` for India, `Language.or_` for Odia;
      the wire still writes `IN` and `or`.
    - Text to enum at a boundary: *recommendation:* automatic, in a
      request, JSON, a CSV cell or a setting, as `Locale` is decoded: an
      unknown code is `invalid_value` naming the field, rule `variant`.
    - Text to enum mid-program: *recommendation:* the contract's `parse`,
      `attempt text.parse(raw, as: Country) { invalid: UnknownCountry }`;
      `text.from(Country.gb)` gives `GB` back.
    - Case: *recommendation:* `parse` and boundary decoding ignore case
      and write the canonical case back, as a `Locale` setting is already
      read: no two codes in these lists differ only by case, BCP 47 tags
      are case-insensitive, and `gb` typed into a form plainly means `GB`.
      Spaces and other spellings are still refused.
    - Other code systems: *recommendation:* a `form:` option on the
      contract's calls, not a function per form, since functions live in
      namespaces and a type carries none: `text.parse("GBR", as: Country,
      form: CodeForm.alpha3)`, `form: CodeForm.numeric` for `826`, and
      `text.from(country, form: CodeForm.alpha3)` to write one. The type's
      own code is the default, and a form the type has no data for is a
      compile error.
    - Showing it: *recommendation:* `text.format(country, locale:)` gives
      the CLDR name in that locale ("Royaume-Uni"), and `Country.all`
      lists every variant, in the enum's canonical order, for a dropdown.
      `.all` exists on every enum whose variants carry no data, so `all`
      can't name a variant; no page or example enum has one.
    - Matching names ("England", "UK"): *recommendation:* left out, not
      in `parse` and not as a separate operation. Which name means which
      code is a judgement (England is `CountrySubdivision.gb_eng`, not a
      country), differs by locale, and a list of candidates still needs a
      person to pick one, which a dropdown from `.all` already asks for. A
      package can offer it if an import needs it.
    - Retired codes: *recommendation:* each enum's data ships with the
      compiler and is pinned in `jadpo-value`, as the time-zone database
      is. A code its authority withdraws stays a variant, marked retired,
      so stored data and old messages still decode; new code that writes
      it is the declinable error `type.variant_retired` (area 1's question
      11), whose reason says why, such as reading old records. *Decided 2026-10-17 (Richard):* as recommended.
23. **Is text counted and cut in code points?** *Recommendation:* yes, as
    Python and PostgreSQL do; graphemes left out. *Decided 2026-10-17 (Richard):* as recommended.
24. **Does `hash.sha256` give `Bytes`?** *Recommendation:* yes, written as
    text by `encoding`. *Decided 2026-10-17 (Richard):* as recommended.
25. **Does `Rounding` gain `down`, toward zero?** *Recommendation:* yes,
    with SQL form `trunc`; `floor`, `ceiling` and `half_down` stay out
    until amounts below zero need them. *Decided 2026-10-17 (Richard):* as recommended.
26. **Is `text.slug` ASCII-only, and does text sort in code points unless
    `locale:` says otherwise?** *Recommendation:* yes to both, matching
    URLs and queries. *Decided 2026-10-17 (Richard):* as recommended.
27. **No logic gates beyond `and`, `or` and `not`, and bitwise
    operations as a `bits` namespace?** *Recommendation:* no gates added:
    exclusive or on `Bool` is `!=`, and any other gate is a one-line
    function. No `bits`: no scenario but device payloads needs them, and
    those decode in their provider package; flags are a `Set` of an enum. *Decided 2026-10-17 (Richard):* as recommended.
28. **Are SQL forms used by backfills, and later by aggregate queries?**
    *Recommendation:* backfills in 5.6; aggregate queries as a new area 6
    chunk after it, using the same SQL forms, which also gives reports
    their `lower` (where a form exists) and time buckets. *Decided 2026-10-17 (Richard):* as recommended.
29. **Does the table record `at_most_length_of`?** *Recommendation:* yes,
    for `trim`, `slice`, `keep_*` and `drop_*` only; `lower` and `upper`
    can lengthen text (`ß` becomes `SS`). *Decided 2026-10-17 (Richard):* as recommended.
30. **Every standard namespace reserved, `locale` included?** Area 1
    reserves standard-library namespaces
    (`semantic.standard_namespace_reserved`), so each namespace this page
    adds takes a word from declarations, domains, libraries, parameters
    and variables: `list`, `set`, `map`, `range`, `text`, `markup`,
    `money`, `units`, `hash`, `encoding`, `crypto`, `random`, `json`,
    `csv`, `tsv`, `json_lines`, `colour`, `media`, `web` and `semver`,
    then `geo` and `ai` with their areas; `locale` already is, which first
    raised this after chunk 5.3c. Fields and named arguments keep every
    word (`note.text`, `set:`). Today only `text` (five page examples,
    about ten crate snippets) and `range` (area 10's
    `deliveries_in(range:)`) clash. *Recommendation:* yes, reserve them
    all, as area 1's rule already does for `temporal` and `locale`, so
    `text.` and `range.` always mean the standard library; the clashing
    names are renamed in the chunk that adds each namespace (`text` in
    5.5f, `range` in 5.5c). *Decided 2026-10-17 (Richard):* as recommended.

### Building it

5.5 as one chunk is far more than a session: 133 operations and 234 new
types. The split, each one session, in order, is on the roadmap as 5.5a
to 5.5s, after 5.4b's declinable errors and 5.4c's translations as
messages (which the contract's `text.format` builds on). The contract comes
first; each group of types then arrives with its baseline and its extras.

- **5.5a The standard type contract** `opus · high`: a type's stub and data
  entry generating construction, `text.parse`, `text.format`, `text.from`,
  JSON, storage, `==` and order; named validators; the one test harness;
  proven on the 13 built types; `temporal.format` moved under
  `text.format`; `type.standard_exists` on app code. Built; split on
  2026-10-17, the rest is 5.5a2.
- **5.5a2 Standard type clashes and `jadpo fix`** `opus · high`: the clash
  check for a release that adds a standard type, with `jadpo fix`
  (question 4). Built.
- **5.5b Lists** `opus · high`: `collection.` renamed `list.` everywhere,
  with its habit error; the 21 new list operations that need no language
  change; `never_negative`, the `sort_by` key's ordered kinds, results
  that keep a named type. Built.
- **5.5c Sets, maps and ranges** `opus · high`: the 7 `set`, 8 `map` and 3
  `range` operations; `for` over a set; `[]` as an empty map; `range`
  reserved and area 10's `deliveries_in(range:)` renamed. Built; split on
  2026-10-17, the rest is 5.5c2.
- **5.5c2 Two-parameter inline functions and `Range<T>`** `opus · high`:
  `(a, b) -> ...` for standard operations, with `list.zip_with`,
  `map.filter` and `map.map_values`; `Range<T>` on the contract, the
  `range` operations taking it. Built.
- **5.5d Numbers** `opus · high`: the 8 new `integer` and `decimal`
  operations; `scale:` written out; `Rounding.down`. Built, with
  `text.format` for numbers.
- **5.5e Decimal maths** `opus · high`: the eight
  correctly rounded operations, tested against reference values. Built.
- **5.5f Text** `opus · high`: the 17 text operations (the contract's
  three come in 5.5a, `text.pick` in 5.5o), `locale:` on case
  and sorting, NFC at the boundary; `never_empty`, `fill:`,
  `at_most_length_of`; `text` reserved and the clashing examples renamed.
  Built.
- **5.5g Hashing, encodings and JSON** `opus · high`: `hash`, `encoding`,
  `json.read` and `json.write` in memory with `mismatch`'s problem; the
  batch forms and `csv`, `tsv` and `json_lines` come with batches (6.4).
  Built; split on 2026-10-17, the rest is 5.5g2.
- **5.5g2 The decode problem on a failure** `opus · high`: the built-in
  `DecodeProblem`, and a failure mapped from `mismatch` that declares a
  `problem: DecodeProblem` field receiving it. Built.
- **5.5h Money** `opus · high`: `Money` and `Currency`, the 7 `money`
  operations other than `convert`, the operator table's `Money` rows with
  `currency_mismatch`, `rounding:` and `type.money_split`, the two
  columns, the wire shape, `text.format`, and area 2's examples renamed.
  Built; split on 2026-10-17, the rest is 5.5h2.
- **5.5h2 Exchange rates and money in messages** `opus · high`:
  `ExchangeRate` and `money.convert`; `{price, number, currency}` in
  translation messages. Built.
- **5.5i Payment and identity numbers** `opus · high`: the 17 payment and
  identity numbers that are text, each a pattern or check digit (mod-97,
  Luhn, mod-11) on the contract. Built; split on 2026-10-18, the rest is
  5.5i2.
- **5.5i2 Payment records and identity documents** `opus · high`, after
  5.5j and 5.5l: the money group's records (`Price`, `TaxRate`,
  `Discount`, `MaskedCardNumber`, `BankAccount` with the UK modulus check,
  and the rest) and the identity documents on `Country`.
- **5.5j Quantities** `opus · high`: the 17 physical quantities with
  `DataSize` and `Bitrate`, their units, SI storage at each kind's
  scale, suffixed literals, the product and quotient table,
  `type.unit_required`, and the 3 `units` operations. Built; split on
  2026-10-18, the rest is 5.5j2.
- **5.5j2 Percentages, ratios and fractions** `opus · high`:
  `Percentage`, `Ratio` and `Fraction`, the `%` literal and the operator
  table's `Percentage` rows. Built.
- **5.5k Time and calendar** `opus · high`: ten of the 13 new time
  types, `CalendarPeriod`'s operator rows and `temporal.period_between`.
  Built; split on 2026-10-17, the rest is 5.5k2.
- **5.5k2 Recurrence and opening hours** `opus · high`: `RecurrenceRule`
  expansion (`temporal.occurrences`) and `OpeningHours`
  (`temporal.open_at`). Built; split on 2026-10-18, `CalendarEvent` is
  5.5k3.
- **5.5k3 Calendar events** `opus · high`: `CalendarEvent`, an object
  whose parts code reads, with optional parts and an all-day form. Built.
- **5.5l People and places** `opus · high`: the 19 new types, with
  `PhoneNumber`'s E.164 metadata, `PostalAddress` and `PersonName` in each
  locale's order, and `PostalCode` per country. 15 built; split on
  2026-10-18, `PhoneNumber`, `CallingCode`, `PostalCode` and `UnLocode`
  are 5.5l2.
- **5.5l2 Phone numbers and postal codes** `opus · high`: the other four,
  with libphonenumber's metadata and libaddressinput's patterns. Built;
  the UN/LOCODE list waits on its terms.
- **5.5m Commerce, health, travel and the rest** `opus · high`: 45 types,
  mostly codes with a pattern or check digit (`Ean13`, `Isbn`, `Vin`,
  `IataAirport`).
- **5.5n Web and network** `opus · high`: the 20 web and network types
  and `web`'s 3 operations. Split on 2026-10-18; the 18 software types and
  `semver.satisfies` are 5.5n2.
- **5.5n2 Software** `opus · high`: the 18 software types and
  `semver.satisfies`.
- **5.5o Text formats** `opus · high`: `Markdown`, `Html` (whose
  construction is the sanitiser, a security boundary per #88), `RichText`
  and `LocalisedText`, `markup`'s 3 operations and `text.pick`.
- **5.5p Media and colour** `opus · high`: the 15 types, `colour`'s 5
  operations and `media.fit`.
- **5.5q Randomness and cryptography** `opus · high`: the generated
  `random` namespace with test seeding; `crypto`'s 8 operations that need
  no keys held as data; `EncryptionKey`, `PasswordHash`, `PasswordCheck`,
  `PrivateKey`, `PublicKey` and `key:` as a compiler-owned place.
- **5.5r Security types** `opus · high`: the other 17 security types
  (`Password`, `Jwt`, `Jwk`, `Certificate`, `ApiKey` ...) and
  `crypto.verify_jwt`; `verify_totp` follows keys held as data.
- **5.5s Geospatial and AI types' baselines** `opus · high`: the 17
  geospatial types and `Vector`, `Embedding`, `TokenCount`, `TokenUsage`
  and `ChatMessage`, with construction, text and JSON from the contract;
  their storage and operations come with their own areas.
- **`type.standard_exists`**: on app code in 5.5a, after 5.4b's declines
  (area 1's question 11), the check in 5.5a and each type's match
  signature with its group; on packages' public types with the publishing
  bar (7.1).
- **Later, outside 5.5a to 5.5s:** cross-field rules (area 2) as 5.5t;
  the geospatial plan as 5.5u and the AI plan as 5.5v; aggregate queries
  (area 6) as 5.6c; encrypted fields and keys held as data with personal
  data (area 15), after the first build.
- **SQL forms move to 5.6**, computed backfills, their only user for now.
- **5.7** brings the teaching examples up to date, as planned.
