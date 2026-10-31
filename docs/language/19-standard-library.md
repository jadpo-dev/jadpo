---
title: Standard library
area: 19
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Standard library
<!-- topic: standard-library -->

The standard library is the set of types and operations every program can
use without declaring them, such as `Money`, `list.count(items)` and
`temporal.in_zone(at, zone)`. A few are defined on the pages whose
features need them: [Values and types](02-values-and-types.md),
[Logic](03-logic.md) and [Configuration and time](10-configuration-and-time.md).
This page collects them all, adds the types and operations real
applications keep reaching for, lists the language changes they rely on,
and says what is left out on purpose.

Operations are grouped into namespaces, each named for what it works on.
You always call an operation through its namespace: `list.count(items)`,
never `count(items)`.

| Namespace | Holds | Operations | Defined in |
|---|---|---|---|
| `list` | Operations on `List<T>` | 26 (4 from Values and types) | This page and [Collections](02-values-and-types.md#collections); inline functions in [Passing functions](03-logic.md#passing-functions) |
| `set` | Operations on `Set<T>` | 7 | This page |
| `map` | Operations on `Map<K, V>` | 10 | This page |
| `range` | Operations on `Range<T>` and the ranges built on it | 3 | This page |
| `integer` | Operations on values built on `Int` | 5 (1 from Logic) | This page and [Logic](03-logic.md) |
| `decimal` | Operations on values built on `Decimal` | 13 (1 from Logic) | This page and [Logic](03-logic.md) |
| `money` | `Money`, its rates and discounts | 10 | This page |
| `units` | Physical quantities, `DataSize` and `Bitrate` | 3 | This page |
| `text` | Operations on text, and the contract's `parse`, `format` and `from` for every type | 21 | This page |
| `hash` | SHA-256 digests | 1 | This page |
| `encoding` | Bytes as text and back | 9 | This page |
| `crypto` | Encryption, passwords, signatures, message authentication | 10 | This page |
| `random` | Tokens, codes, shuffles; actions only | 5 | This page |
| `json` | One JSON document in memory, and large JSON lists through batches | 4 | This page; the batch forms in [Batches](12-jobs-and-events.md#batches-work-over-many-items) |
| `csv` | CSV files, read and written through batches | 2 | [Batches](12-jobs-and-events.md#batches-work-over-many-items) |
| `tsv` | TSV files, CSV's tab-separated form | 2 | [Batches](12-jobs-and-events.md#batches-work-over-many-items) |
| `json_lines` | JSON Lines files, one value per line | 2 | [Batches](12-jobs-and-events.md#batches-work-over-many-items) |
| `temporal` | Instants, dates, local times, zones, formats, recurrence | 24 (21 from Configuration and time) | [Zones and local times](10-configuration-and-time.md#zones-and-local-times) and this page |
| `locale` | Choosing a supported locale | 1 | [Choosing a locale from a request](10-configuration-and-time.md#choosing-a-locale-from-a-request) |
| `colour` | Converting `Colour` between spaces, contrast | 5 | This page |
| `web` | Building URLs, address blocks | 3 | This page |
| `semver` | Version ranges | 1 | This page |
| `media` | Fitting dimensions | 1 | This page |
| `markup` | Markdown and HTML, made safe | 3 | This page |

There are 171 operations in all: 28 defined on the Values and types,
Logic, and Configuration and time pages, 8 file helpers defined with
[batches](12-jobs-and-events.md#batches-work-over-many-items), and 135
defined here. Operators are counted apart, in
[the standard operator table](#the-standard-operator-table). Operations
on geometry and AI are counted on the [Geospatial](20-geospatial.md) and
[AI](21-ai.md) pages.

Every standard type also gets the same baseline of operations, generated
for it and not counted per type: construction, `text.parse`,
`text.format`, `text.from`, JSON, storage, `==` and order
([Every standard type meets one contract](#every-standard-type-meets-one-contract)).

The [design notes](notes/19-standard-library.md) record why each part of
this page is the way it is, and [What is built](#what-is-built) says
which parts work today. One operation, `crypto.verify_totp`, waits on
keys held as data.

## Shared vocabulary types
<!-- topic: shared-vocabulary-types -->

Some types are needed by nearly every package and app, and each would
otherwise invent its own. Geospatial libraries disagree on whether a
point is `[lat, lng]` or `[lng, lat]`, and payment libraries on whether
ten pounds is `10.00`, `"10.00"` or `1000`. When the language owns the
type, every package and every app uses the same one, and that whole class
of bug goes away. So the standard library defines a type wherever two
packages would otherwise each define their own, even when the heavy
operations on it live in a package or on another page.

- **Each one is defined once.** It has one construction that validates
  it, one shape on the wire and one shape in storage. Nothing else may
  give it a second.
- **Ambiguity is designed out.** Fields are named, never positional:
  `Point { latitude: ...  longitude: ... }`, never a pair. An axis order
  appears only in a conversion whose name says it: `geo.from_lng_lat(list)`,
  `geo.from_lat_lng(list)`, `geo.from_geojson`.
- **The types are global names.** Every standard type is a global name,
  as `Email` and `Instant` are, so you write `PostalAddress`, never
  something like `geo.Point`. Only the functions live in namespaces
  (`geo.`, `money.`, `list.`).
- **Your app names its own types for their purpose.** Don't declare your
  own `Address`. Name a type for what it is for and what it holds:
  `DeliveryStop { address: PostalAddress }`, `BillingContact`.
- **A standard type's name can't be reused.** Declaring a type with a
  standard type's name is the error `type.standard_clash`
  ([A release that adds a standard type](#a-release-that-adds-a-standard-type)),
  as reusing any built-in name is an error
  ([Values and types](02-values-and-types.md)). So the shape check below
  only has to catch equivalents under other names.
- **Packages and apps use them.** When a package's public types, or your
  app's own code, declare their own equivalent of a standard type, the
  compiler reports the error `type.standard_exists`, pointing at the
  standard type ([Packages use the standard types](#packages-use-the-standard-types)).
  You can decline it with a reason. It is an error rather than a warning
  because Jadpo has no warnings
  ([Declining a judgement call](01-project-layout-and-modules.md#declining-a-judgement-call)).

## The standard types
<!-- topic: the-standard-types -->

There are 247 standard types, listed here by group. Each part or kind a
standard type names is a standard type too, so it is read, sent and
stored by its code: `IdentityDocumentKind`, `InterestRateBasis`,
`ContentRatingScheme`, `RichTextBlock`, `RichTextSpan`, `RichTextMark`,
`ClothingSizeSystem`, `ShoeSizeSystem`, `RouteOfAdministration` and
`ChatRole`. GeoJSON's `Feature` and `FeatureCollection` are defined on
the [Geospatial](20-geospatial.md) page.

In the table, "(enum)" marks a closed list held as a built-in enum, and
"Extras in" names the namespace whose operations go beyond the
[baseline](#every-standard-type-meets-one-contract). Every type is built,
which the bold marks.

| Group | Types | Extras in |
|---|---|---|
| People and contact (12) | **`PersonName`**, **`GivenName`**, **`FamilyName`**, **`Username`**, **`Email`**, **`EmailWithName`**, **`PhoneNumber`**, **`PhoneExtension`**, **`PostalAddress`**, **`PostalCode`**, **`Uprn`**, **`Nationality`** (enum) | `web` |
| Places (10) | **`Country`** (enum), **`CountrySubdivision`** (enum), **`Continent`** (enum), **`UnLocode`**, **`CallingCode`**, **`Language`** (enum), **`WritingScript`** (enum), **`TextDirection`** (enum), **`Locale`** (enum), **`Zone`** (enum) | |
| Geospatial (17) | **`Point`** (optional altitude), **`Latitude`**, **`Longitude`**, **`LineString`**, **`Polygon`**, **`MultiPoint`**, **`MultiLineString`**, **`MultiPolygon`**, **`Geometry`**, **`BoundingBox`**, **`Circle`**, **`Bearing`**, **`Crs`**, **`EastingNorthing`**, **`MapTile`**, **`H3Cell`**, **`Geohash`** | `geo` ([Geospatial](20-geospatial.md)) |
| Money, payments, banking (24) | **`Money`**, **`Currency`** (enum), **`ExchangeRate`**, **`Price`**, **`UnitPrice`**, **`TaxRate`**, **`Discount`**, **`BasisPoints`**, **`InterestRate`**, **`PaymentCardNumber`**, **`MaskedCardNumber`**, **`CardBrand`** (enum), **`CardBin`**, **`CardExpiry`**, **`PaymentReference`**, **`InvoiceNumber`**, **`Iban`**, **`Bic`**, **`UkSortCode`**, **`UkAccountNumber`**, **`BankAccount`**, **`AbaRoutingNumber`**, **`Lei`**, **`Isin`** | `money` |
| Tax and identity numbers (11, and `IdentityDocumentKind`) | **`VatNumber`**, **`TaxId`**, **`UkUtr`**, **`UkCompanyNumber`**, **`UkNationalInsuranceNumber`**, **`UsSsn`**, **`UsEin`**, **`PassportNumber`**, **`DrivingLicenceNumber`**, **`IdentityDocument`** with **`IdentityDocumentKind`** (enum), **`NhsNumber`** | |
| Commerce and logistics (20, and `ClothingSizeSystem` and `ShoeSizeSystem`) | **`Sku`**, **`Ean13`**, **`UpcA`**, **`Isbn`**, **`Sscc`**, **`Barcode`**, **`UnitQuantity`**, **`Rating`**, **`ClothingSize`** with **`ClothingSizeSystem`** (enum), **`ShoeSize`** with **`ShoeSizeSystem`** (enum), **`Dimensions`**, **`HsCode`**, **`Incoterm`** (enum), **`TrackingNumber`**, **`Carrier`**, **`ContainerNumber`**, **`LotNumber`**, **`SerialNumber`**, **`Vin`**, **`VehicleRegistration`** | `units`, `media` |
| Time and calendar (19) | **`Instant`**, **`CalendarDate`**, **`ZonedTime`**, **`Duration`**, **`Weekday`** (enum), **`LocalTime`**, **`LocalDateTime`**, **`Month`** (enum), **`YearMonth`**, **`Year`**, **`Quarter`**, **`IsoWeek`**, **`UtcOffset`**, **`DateRange`**, **`InstantRange`**, **`CalendarPeriod`**, **`RecurrenceRule`**, **`OpeningHours`**, **`CalendarEvent`** | `temporal`, `range` |
| Physical quantities (17) | **`Length`**, **`Area`**, **`Volume`**, **`Angle`**, **`Mass`**, **`Pressure`**, **`Energy`**, **`Power`**, **`Temperature`**, **`TemperatureDifference`**, **`Speed`**, **`Frequency`**, **`Voltage`**, **`Current`**, **`Illuminance`**, **`SoundLevel`**, **`Concentration`** | `units` |
| Maths and data (8) | **`Percentage`**, **`Ratio`**, **`Fraction`**, **`Range<T>`**, **`DataSize`**, **`Bitrate`**, **`Vector`**, **`Embedding`** | `range`, `units`; [AI](21-ai.md) |
| Health (9, and `RouteOfAdministration`) | **`BloodType`** (enum), **`Icd10Code`**, **`SnomedConcept`**, **`Dose`**, **`DosageInstruction`** with **`RouteOfAdministration`** (enum), **`HeartRate`**, **`BloodPressure`**, **`BloodGlucose`**, **`Allergen`** (enum) | |
| Media and visual (15, and `ContentRatingScheme`) | **`Colour`**, **`ImageDimensions`**, **`AspectRatio`**, **`Rect`**, **`Insets`**, **`PaperSize`** (enum), **`ExifOrientation`** (enum), **`AltText`**, **`FrameRate`**, **`VideoResolution`**, **`VideoCodec`**, **`AudioCodec`**, **`Timecode`**, **`SubtitleCue`**, **`ContentRating`** with **`ContentRatingScheme`** (enum) | `colour`, `media` |
| Web and network (23) | **`Url`**, **`UrlPath`**, **`RelativeUrl`**, **`Slug`**, **`DomainName`**, **`Hostname`**, **`WebOrigin`**, **`HttpMethod`** (enum), **`HttpStatus`**, **`HttpHeaderName`**, **`FileType`**, **`UserAgent`**, **`ProblemDetails`**, **`JsonPointer`**, **`Cookie`**, **`UtmParameters`**, **`IpAddress`**, **`Cidr`**, **`NetworkPort`**, **`MacAddress`**, **`Imei`**, **`Emoji`**, **`PushToken`** | `web` |
| Software (19) | **`SemanticVersion`**, **`VersionRange`**, **`GitRef`**, **`PackageUrl`**, **`Cve`**, **`SpdxLicence`**, **`TraceId`**, **`SpanId`**, **`TraceParent`**, **`LogLevel`** (enum), **`FilePath`**, **`FileName`**, **`RegularExpression`**, **`ContainerImage`**, **`Digest`**, **`Uuid`**, **`Ulid`**, **`IdempotencyKey`**, **`Doi`** | `semver` |
| Text formats (4) | **`Markdown`**, **`Html`**, **`RichText`**, **`LocalisedText`** | `markup`, `text` |
| Security (20) | **`Password`**, **`PasswordHash`**, **`PasswordPolicy`**, **`Hash`**, **`Hmac`**, **`Jwt`**, **`JwtClaims`**, **`Jwk`**, **`PublicKey`**, **`PrivateKey`**, **`Certificate`**, **`TotpSecret`**, **`OneTimeCode`**, **`ApiKey`**, **`SecretText`**, **`BearerToken`**, **`OAuthScope`**, **`OidcClaims`**, **`WebAuthnCredential`**, **`EncryptedValue`** | `crypto` |
| Travel (6) | **`IataAirport`**, **`IcaoAirport`**, **`IataAirline`**, **`FlightNumber`**, **`CabinClass`** (enum), **`BookingReference`** | |
| Other with a published standard (10) | **`UnNumber`**, **`Orcid`**, **`CefrLevel`** (enum), **`Mpan`**, **`Mprn`**, **`EpcRating`** (enum), **`Tenure`** (enum), **`LawfulBasis`** (enum), **`Jurisdiction`**, **`ModelId`** | |
| AI (3, and `ChatRole`) | **`TokenCount`**, **`TokenUsage`**, **`ChatMessage`** with **`ChatRole`** (enum) | [AI](21-ai.md) |

Three rules shape the list:

- **Country-specific identifiers are global types,** like the rest.
  `UkSortCode`, `UsSsn` and `NhsNumber` are named by their country, so
  they don't clash.
- **Gender and sex at birth are left out.** They are contested, and they
  are personal data, so your app models them deliberately, and
  [Personal data](15-personal-data.md) treats them as sensitive.
- **A quantity is stored in its SI base unit.** You choose a unit when
  you make the value and when you show it: `units.of(5, LengthUnit.mile)`
  holds metres, and `text.format(distance, locale:, unit: LengthUnit.mile)`
  shows miles. So `==` and ordering just work, and storage is one
  `numeric` column.

### How standard types are named

A standard type's name says exactly what it is, so it doesn't take a word
your app needs for something else. The name is, in order of preference:

1. the defining standard's own term, qualified until it is unambiguous:
   `PostalAddress`, `RecurrenceRule`, `ProblemDetails`;
2. otherwise the noun, qualified by what it holds: `DateRange`,
   `DataSize`, `ImageDimensions`;
3. never a bare word an app plausibly uses for its own, different thing,
   except a generic container that takes a type, such as `Range<T>`, as
   `List<T>` is.

So it is `CountrySubdivision`, not `Subdivision`, which could be a housing
subdivision, and `UnitQuantity`, not `Quantity`, which could be an order
line's count. The same rule gives `SecretText`, `NetworkPort`,
`WritingScript` and `WebOrigin`, and a time interval is the built-in
`InstantRange`. A bare word passes when an app using it means the
standard thing: `Price`, `Area`, `Mass`, `Currency`.

`Point` and `Length` are the two exceptions, so that geography and
quantities read naturally (`var shelf: Length = 2metre`). Built-in names
older than the rule (`File`, `Page`, `Access`) keep their names.

### Types that aren't on the list

Some types you might look for are folded into another, so nothing is
lost: units into their quantity type, roles into the general type, and
duplicates into the one type they duplicate. Formats and application
entities are left out:


| What | Became |
|---|---|
| Units (`Metre`, `Mile`, `Kilogram`, `Pound`, `Celsius`, `Litre`, `KilowattHour`, every `...Unit` enum and 17 more) | The quantity type, with a unit at construction and display |
| Roles: `Height`, `GeoDistance` · `FloorArea` · `WindSpeed` · `BodyTemperature` · `BarometricPressure` · `MediaDuration` · `DateOfBirth`, `ExpiryDate` · `CountryOfOrigin` · `UsState`, `Region` · `Salary` | `Length` · `Area` · `Speed` · `Temperature` · `Pressure` · `Duration` · `CalendarDate` · `Country` · `CountrySubdivision` · `Money`: a field of the general type |
| Duplicates: `Timestamp`, `Uri`, `Ipv4Address`, `Ipv6Address`, `HexColour`, `HslColour`, `Sha256`, `UuidV7`, `Point3D`, `CountryAlpha3`, `LanguageAlpha3`, `UkPostcode`, `UsZipCode`, `UkNumberPlate`, `EuVatNumber`, `UkVatNumber`, `TimeRange`, `MinorUnits`, `MediaType`, `ImageFormat` | The one standard type each duplicates |
| Also folded in review: `AddressLine`, `BarcodeSymbology`, `DoseFrequency`, `FlightDesignator`, `GitSha`, `Gtin`, `HttpHeaders`, `Humidity`, `PackageDimensions`, `PackageName`, `PhysicalQuantity`, `UnitOfMeasure`, `Ucum`, `Proportion`, `QueryString`, `SubtitleTrack`, `UkBankAccount`, `WebhookSignature`, `WindDirection`, `CarbonDioxideEquivalent` | `PostalAddress`, `Barcode`, `DosageInstruction`, `FlightNumber`, `GitRef`, `Ean13`, `HttpHeaderName`, `Percentage`, `Dimensions`, `PackageUrl`, `UnitQuantity` (three), `Ratio`, `Url`, `SubtitleCue`, `BankAccount`, `Hmac`, `Bearing`, `Mass` |
| Formats: `Hex`, `Base64`, `Wkt`, `Json`, `CsvDialect`, `MessageFormat`, `VCard`, `OtpAuthUri`, `CssLength` | Out: `encoding.` and `json.` operations |
| Application entities: `Recipe`, `HotelStay`, `OrderStatus`, `PaymentStatus`, `Consent`, `SensorReading`, `StoredFile`, `UserId`, `Allergy` and 31 more | Out: an application's own records |
| `Gender`, `SexAtBirth`, `Cvv` | Out: sensitive (a CVV is never stored) |
| `HashAlgorithm`, `KeyAlgorithm`; `SessionId`; `AcceptLanguage` | Out: no algorithm choices; the runtime's ([Identity](08-identity.md)); a call, `locale.from_accept_language` |
| Niche, silly, or a maybe with no published standard | Out: 729 entries, a package's to define |

## Every standard type meets one contract
<!-- topic: every-standard-type-meets-one-contract -->

Every standard type comes with the same baseline of operations, so once
you know how one type is built, read, sent, stored and shown, you know
them all. Some types add operations of their own on top
([Extras beyond the baseline](#extras-beyond-the-baseline)).

| Baseline | What it is | Where a type has none |
|---|---|---|
| Construct and validate | From its parts (`PostalCode(raw)`, `Money` from `money.from_minor`), and from text with `text.parse(raw, as: T)`; each failure named by the rule it breaks, `malformed` for text that isn't the type at all | Never: every type has both |
| `format` for people | `text.format(value, locale:)`, giving `PresentationText`, through CLDR where the locale matters (an address's line order, a name's order, a quantity's unit) | Every type has one |
| Canonical text | `text.from(value)`: E.164, ISO codes, `#rrggbb`, ISO 8601, `pkg:` URLs; the form `text.parse` reads back | Types with no single text form (`PostalAddress`, `PersonName`); asking is a compile error |
| JSON | One shape, read and written by `json.read`, `json.write` and every route | Never |
| Storage | One shape: a column type, or named columns for an object (`Money`'s two) | Never |
| `==`, and order | `==` on every type; `<` and `sort_by` where the type is ordered (quantities of one kind, `Money` of one currency, times, `SemanticVersion`, `Percentage`) | Order only where it means something |
| Conversions at named edges | `geo.from_lng_lat`, `units.in`, `Isbn` 10 to 13, `MaskedCardNumber` from a `PaymentCardNumber` | Only where the type has another form |

Each of `text.parse`, `text.format` and `text.from` is one operation over
every standard type, as `json.read` is one over every input type, so the
contract adds 2 names, not 700.

A standard type may take a type argument (`Range<T>`), be a choice of
other types (`Discount`, an [enum with data](02-values-and-types.md#enums)),
be built on a built-in type with bounds of its own (`TaxRate`), or be a
named collection (`RichText` a list, `LocalisedText` a map). A named
collection is built, sent and read as one your app declares is. Every
standard object and choice is stored in named columns laid out from its
parts ([Every boundary through the contract](#every-boundary-through-the-contract)).

`jadpo suggest` uses each standard type's shape and its signals, such as
conversion constants, the words of names, format literals and validation
texts, to find code that hand-rolls the type
([Suggestions](14-tooling.md#suggestions)).

In this example a form sends a quantity as text, an order's time goes
into a log line as its canonical text, and a day, a duration and a zone
are shown to people in their own words:

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

### Extras beyond the baseline

Many groups of types have operations beyond the baseline. Each has its
full table under [Operations](#operations):

| Group | Extras | Count |
|---|---|---|
| Money and payments | `money.from_minor`, `of`, `minor`, `currency`, `sum`, `allocate`, `split`, `convert` (with a given `ExchangeRate`, never fetched), `mask` (a `MaskedCardNumber` from a `PaymentCardNumber`), `apply_discount` (a `Discount` off a `Price`) | 10; arithmetic by the operator table |
| Physical quantities, `DataSize`, `Bitrate` | `units.of`, `in`, `sum`; operators within a kind and the defined products and quotients | 3 |
| Time and calendar | The 21 from [Configuration and time](10-configuration-and-time.md), plus `temporal.occurrences`, `temporal.open_at` and `temporal.period_between`; `CalendarPeriod` arithmetic by the operator table | 3 new |
| Ranges (`Range<T>`, `DateRange`, `InstantRange`) | `range.contains`, `range.overlaps`, `range.intersection` | 3 |
| Colour | `colour.from_hsl`, `from_oklch`, `mix`, `with_lightness`, `contrast` (WCAG 2) | 5 |
| Security | `crypto` as tabled below, with `sign`, `verify_signature`, `verify_jwt` and `verify_totp` | 10 |
| Web | `web.with_query(url, entries)`, `web.resolve(base, relative)`, `web.cidr_contains(range, address)` | 3 |
| Software | `semver.satisfies(version, range)` | 1 |
| Media | `media.fit(dimensions, within:)`, keeping the aspect ratio | 1 |
| Text formats | `markup.to_html`, `rich_to_html`, `to_plain_text`; `text.pick` with fallback. `Html` is always safe: its construction sanitises | 4 |
| Geometry | `geo.distance`, `length`, `bearing`, `area`, `intersects`, `contains`, `within`, `centroid`, `bounding_box`, the constructions and WKT's two | [Geospatial](20-geospatial.md) |
| AI (`Vector`, `Embedding`, `ChatMessage`) | `ai.generate`, `stream`, `embed`, `embed_all`, `distance` | [AI](21-ai.md) |
| People, places, identity numbers, commerce codes, health, travel, other | Baseline only: each identifier's check digit or pattern is its construction | |

### Rules across a type's parts

Some standard types have parts that constrain each other: a range's
start can't be after its end. Each such constraint is a named rule, as on
an object type you declare
([Rules across fields](02-values-and-types.md#rules-across-fields)). So a
value you build in code is proved or attempted exactly as a value of your
own type is. Here are each type's rules, in the order they are checked,
which is also the order OpenAPI's description lists them in:

| Type | Rule | What it holds |
|---|---|---|
| `InstantRange`, `DateRange`, `Range<T>` | `start_not_after_end` | `start <= end`; an open end of a `Range<T>` keeps it |
| `IsoWeek` | `week_in_year` | week 53 only in a year with 53 ISO weeks |
| `CalendarEvent` | `days_or_times` | both days and no start, end or zone, or all three and no days |
| | `last_day_not_before_first` | `last_day >= first_day` |
| | `end_after_start` | `end > start` |
| | `recurrence_from_start` | the recurrence starts on `first_day`, or at `start` in `zone` |
| `PostalAddress` | `postal_code_of_country` | `postal_code`, where it has one, in its `country`'s form |
| `IdentityDocument` | `number_of_kind_and_country` | `number` in the form of its `kind` from its `country` |
| `PassportNumber`, `DrivingLicenceNumber`, `TaxId`, `VatNumber` | `number_of_country` | `number` in its `country`'s form |
| `ContentRating` | `rating_of_scheme` | `rating` one of its `scheme`'s categories |
| `Timecode` | `frames_within_rate` | `frames` below the rate's whole frames a second |
| | `drop_frame_rate` | drop-frame only at 30000/1001 and 60000/1001 |
| | `frame_not_dropped` | never a label drop-frame counting skips |
| `SubtitleCue` | `start_not_negative` | `start >= 0ms` |
| | `end_after_start` | `end > start` |
| | `text_without_arrow` | `text` holds no `-->` |
| `Rating` | `best_above_worst` | `best > worst` |
| | `value_on_scale` | `worst <= value and value <= best` |
| `BloodPressure` | `diastolic_below_systolic` | `diastolic < systolic` |
| `MapTile` | `within_zoom` | `x` and `y` each below 2 to the `zoom` |
| `TokenUsage` | `cached_within_input` | `cached <= input`, where it is given |
| `PasswordPolicy` | `min_not_above_max` | `min_length <= max_length` |
| `BoundingBox` | `south_not_above_north` | `south <= north` |
| `Polygon` | `rings_valid` | each ring closed and simple, no rings touching, holes inside the outer ring, every edge under half the globe ([Polygons are valid when built](20-geospatial.md#polygons-are-valid-when-built)) |
| | `altitudes_agree` | every position with an altitude, or none ([Altitudes are all or nothing](20-geospatial.md#altitudes-are-all-or-nothing)) |
| `LineString`, `MultiPoint`, `MultiLineString`, `MultiPolygon` | `altitudes_agree` | every position with an altitude, or none, a multi-geometry's parts with each other ([Altitudes are all or nothing](20-geospatial.md#altitudes-are-all-or-nothing)) |

Here a slot built from two bookings' instants is attempted by its rule,
and one whose order a condition has already checked needs no `attempt`:

```jadpo check project=standard-rules
type SlotBackwards = Failure {
    kind: FailureKind.invalid_value
}

/// A slot from one booking's start to another's end.
function combined_slot(
    earlier_booking: InstantRange,
    later_booking: InstantRange
) fails SlotBackwards -> InstantRange {
    return attempt InstantRange {
        start: earlier_booking.start
        end: later_booking.end
    } {
        start_not_after_end: SlotBackwards
    }
}

/// The same, refused before it is built.
function combined_slot_checked(
    earlier_booking: InstantRange,
    later_booking: InstantRange
) fails SlotBackwards -> InstantRange {
    if (later_booking.end < earlier_booking.start) {
        reject SlotBackwards
    }
    return InstantRange {  // the condition proves `start_not_after_end`
        start: earlier_booking.start
        end: later_booking.end
    }
}
```

```jadpo check project=standard-rules file=main.test.jadpo
test "a slot never ends before it starts" {
    var morning_slot: InstantRange = InstantRange {
        start: "2026-10-26T09:00:00Z"
        end: "2026-10-26T12:00:00Z"
    }
    var afternoon_slot: InstantRange = InstantRange {
        start: "2026-10-26T13:00:00Z"
        end: "2026-10-26T17:00:00Z"
    }
    var whole_day: InstantRange = attempt combined_slot(morning_slot, afternoon_slot)
    assert whole_day.end == afternoon_slot.end
    assert combined_slot(afternoon_slot, morning_slot) fails SlotBackwards
    assert combined_slot_checked(afternoon_slot, morning_slot) fails SlotBackwards
}
```

- **Each rule behaves like a condition you write.** It is named, proved,
  attempted and reported exactly as a condition is. It runs on every
  value made, however it is made: an object literal, `text.parse`, a
  request's JSON or `json.read`, in compiled code as in the interpreter.
  The checker reads a literal by the same check. Every check across a
  standard type's parts is one of these rules, so there is no second
  mechanism.
- **How the checker proves a rule.** It reads a literal by what it shows
  of each part. A part written out is known, a part written `none` is
  absent, and a part built from a value of a type that can't be `none` is
  given. A rule its parts decide either holds or is the error
  `type.invalid_literal`, naming the rule.
- **A rule ordering two parts** (a range's ends, a rating's, a password
  policy's lengths) can otherwise be proved as a condition comparing two
  fields is: by a comparison known where the value is built, or by the
  declared ranges. A password policy's minimum length of 8, for example,
  is never above its maximum, which is at least 64. Any other rule on a
  value built from other values is attempted.
- **A rule may read one part.** A subtitle cue's start at 0 or later, and
  its text without `-->`, are named rules checked in the cue's order, not
  bounds the part's value would be built against on its own. Where one
  statement covers it, it is one rule: a rating's value from its worst to
  its best is `value_on_scale`, and a map tile's column and row below 2 to
  the power of its zoom is `within_zoom`.
- **Text and JSON break rules by name.** Text in the type's form that
  breaks a rule (week 53 of 2027, a cue ending before it starts, `PEGI
  12A`) is read and then breaks the rule; it isn't `malformed`. Only text
  that isn't the type at all is `malformed`. So
  `attempt text.parse(raw, as: T)` maps each of `T`'s rules, or
  `invalid:`. JSON reports the rule at the object's path, or at the
  field's path for a type sent as its text.
- **Text written where such a type goes is refused.** That text would be
  the type's canonical text, which is read only when the program runs.
  The checker doesn't have the readers the program uses for it, so it
  couldn't check the rules. The literal is `type.invalid_literal`: write
  the value out as an object literal, whose rules are checked, or read the
  text with `text.parse` and `attempt`. A type with one text form and no
  such rules (`SemanticVersion`, `CardExpiry`, `Colour`) still takes its
  text.
- **Any other literal where a standard object type goes is
  `type.mismatch`:** a number, a `Bool`, or text where a type with no
  single text form goes (`PostalAddress`, or WKT where a geometry goes,
  mixed altitudes included).
- **A value read from the store is checked against its rules,** as a
  value made anywhere else is. Jadpo never writes a value that breaks
  one, so a row that does was written by something else. Reading it is a
  defect naming the rule, never the values. A rule the running program
  can't decide, such as one on a part of the wrong kind, counts as broken.
- **OpenAPI** describes a standard object's schema with "Rules across its
  parts:" and each rule. `Range<T>` is an object of `start` and `end`,
  each its item or `null`, with its rule in its description.

A sale's dates written as a `DateRange`'s text can't be checked while
compiling:

```jadpo check project=e-unread-text expect=type.invalid_literal
/// When the shop's winter sale runs.
function winter_sale() -> DateRange {
    return "2026-12-27/2027-01-03"
}
```

```text output
error: `"2026-12-27/2027-01-03"` can't be checked as a `DateRange` while compiling
 --> main.jadpo:3:12
  |
3 |     return "2026-12-27/2027-01-03"
  |            ^^^^^^^^^^^^^^^^^^^^^^^ canonical text
  reason: A `DateRange` has rules across its parts, which every literal is checked against while compiling; text written where one goes is its canonical text, which isn't read until the program runs, so its rules can't be checked.
  next: Write it out as `DateRange { ... }`, or read the text with `text.parse(..., as: DateRange)` and `attempt` (agent)
  rule: type.invalid_literal
  help: https://jadpo.dev/docs/errors/type.invalid_literal

check failed: 1 error, 1 file
```

Written out, the sale's rules are checked while compiling:

```jadpo check project=unread-text-written
/// When the shop's winter sale runs.
function winter_sale() -> DateRange {
    return DateRange {
        start: "2026-12-27"
        end: "2027-01-03"
    }
}
```

## Every boundary through the contract
<!-- topic: every-boundary-through-the-contract -->

A standard value keeps its currency, unit and precision wherever it goes
into or out of your program. Every place a value crosses encodes a
standard type only through the contract's JSON, storage and text shapes,
never through an encoder of its own:

| Boundary | Encoding | Specified in |
|---|---|---|
| HTTP requests and responses, OpenAPI | The contract's JSON, through the route codec | [HTTP](07-http.md), this page |
| Storage and migrations | The contract's storage: one column, named columns laid out from the type's parts, or a native PostgreSQL type (a range, `time`, `timestamp`); a quantity's column carries a comment naming its SI unit; a fixed currency or rate a check constraint | [Data](05-data.md), this page |
| SQL literals: a query's or a lifecycle's condition, a transition's `set:`, a backfill | The value the storage codec makes, which the runtime binds: a parameter wherever the statement takes one, otherwise that value written as SQL text | [Data](05-data.md) and [Reads and writes](06-reads-and-writes.md), this page |
| Caches and Redis entities | The contract's JSON, as [Caches](05-data.md#caches) holds the API's JSON there | [Data](05-data.md) |
| Event, job and batch payloads | The contract's JSON of the event's or the job's type, written and read by the route codec, and a batch's captured values the same; read with the type as it is when delivered, so a change an API record would call breaking needs an [acceptance](18-records-and-manifest.md#acceptances), and a payload that no longer reads is parked, never dropped | [Jobs and events](12-jobs-and-events.md) |
| CSV and TSV cells | A standard type's canonical text (`text.from`), read by `text.parse`; a type with no single text form (`PostalAddress`, `Price`, a choice) is the compile error naming JSON Lines that a list or an object is | This page, [Data formats](#data-formats-json-csv-tsv-and-json-lines) |
| JSON Lines rows, `json.read` and `json.write` | The contract's JSON | This page |
| Logs and traces | Values as [Logging](04-failures.md#logging) and [Personal data](15-personal-data.md) allow; a personal value's keyed hash covers the UTF-8 of its canonical text, or of its contract JSON where it has no single text form | [Personal data](15-personal-data.md) |
| Settings | `text.parse`'s canonical text, with the source spellings a setting also takes (`5m`) | [Settings](10-configuration-and-time.md#settings-configjadpo) |
| Translation placeholders and email | Shown to people by `text.format` (`PresentationText`), never read back | [Configuration and time](10-configuration-and-time.md) and [Built-in services](16-built-in-services.md) |
| Outbound provider calls and webhooks | The contract's JSON for a field without `wire:`; `wire:` only names another encoding a provider uses | [Integrations](11-integrations.md) |
| Records and the manifest | Types and defaults as source writes them; no runtime values | [Records and manifest](18-records-and-manifest.md) |
| Test fixtures and seeds | Written through the actions they call, so through storage | [Data](05-data.md) and [Testing](13-testing.md) |
| Files | Bytes; the format namespaces above turn them into values | [Files](16-built-in-services.md#files) |
| Developer reports | A value as code makes one (`money.from_minor(1050, Currency.gbp)`), which keeps its currency, unit and places and is never read back | [Debugging and review](17-debugging-and-review.md) |
| The interpreter and compiled code, native and wasm32 | One runtime crate, so one codec for each shape | [The compiler and runtime](../compiler-runtime.md) |

Your app's own structured values are stored as their contract's JSON
too ([Structured values](05-data.md#structured-values)).

### Storage layout

Every standard type is stored in one layout, worked out from its parts:

- **An object** is its parts in order, each in as many columns as it
  takes. An amount of money takes two, its minor units and its
  currency's code. Another standard object or choice takes its own
  columns, and a list of text or codes takes one `text[]` column. A
  `ZonedTime` is two columns, its instant and its zone.
- **A choice** is its variant's name in a `kind` column, then each field
  its variants use, once, in the order they first appear. Two variants
  that use one field name share its column (a `UnitQuantity`'s `amount`,
  at the finest scale any kind keeps). A column is `NULL` where the
  stored variant doesn't have that field.
- **Native where PostgreSQL has the type,** in one column named for the
  field (a suffix of `""`): ranges in `daterange`, `tstzrange` (an
  `InstantRange`), `numrange` and `int8range` (a `Range<Duration>` in
  milliseconds), a `LocalTime` in `time`, a `LocalDateTime` and an
  event's local times in `timestamp` without a zone, a `Year` and a
  `UtcOffset` in `smallint`. A `CalendarPeriod` is three `integer`
  columns, never an `interval`
  ([Time and calendar additions](#time-and-calendar-additions)).
- **A range that holds nothing isn't stored.** PostgreSQL keeps every
  range whose start is its end and that doesn't hold its end as `empty`,
  without its ends, so it couldn't be read back. Storing one is a fault
  that says so, and reading `empty` is one too. A `DateRange` of one day
  holds that day, so it is stored. The compiler doesn't prove at compile
  time that a stored range isn't empty.
- **Every standard enum is stored by its code,** as `Currency` is:
  `Zone` as `Europe/London`, `Locale` as `en-GB`, `HttpMethod` as `GET`,
  `FileType` as `image/png`.
- **A quantity's column carries a comment** giving its kind and SI unit,
  as the units table writes it: `Length in metres (m)`. A column that a
  choice's variants share names each kind's unit with the variant that
  holds it. A migration sets the comment with the column and changes it
  with the type, and the migration snapshot records it.
- **A fixed currency is a check constraint** on each column a type
  fixes, by the setting that fixes it: a `Money` type's `currency:`, and a
  rate type's `from:` and `to:` (`<table>_<column>_from_check`). So a
  stored rate's currencies are always its type's.
- **An empty value in a field that can't be `none`** reads back as that
  value: a `ProblemDetails` that says nothing, whose parts may all be
  `none`, comes back as an empty `ProblemDetails`. In a field that may be
  `none`, every column `NULL` is `none`, so an empty `ProblemDetails`
  there reads back as `none`.
- **A part that is a built-in type held as a standard type** is read as
  that type, as `OidcClaims`' `email` is read as an `Email`.
- **JSON-held values.** `RichText`, `RichTextBlock` (a choice whose
  variants hold lists of objects) and `LocalisedText` are one `jsonb`
  column each, as your app's [structured values](05-data.md#structured-values)
  are. The column holds exactly what `json.write` writes for the value,
  and is read back as `json.read` reads it, with an `Input`'s rules.
  PostgreSQL keeps `jsonb` in its own key order and spacing, so the
  stored text isn't byte for byte what was written, but it reads back
  `==`. Ordering a query by a JSON-held field is refused.
- **What doesn't read back is a fault,** as any column that doesn't make
  its type is, named by its field's path and the rule it broke.

**Faults and errors never carry a value.** A value a column can't hold, a
stored code that isn't one of its enum's, a decimal or a time the
PostgreSQL wire can't carry, and PostgreSQL's own message for a value it
can't take (its class 22, whose message may quote the value) are each
reported by what the value is and the rule it broke, as a `DecodeProblem`
names the field and the rule. A statement's parameter doesn't know its
field, so the field isn't named there.

### Text at a boundary

Text at a boundary is read one way, wherever it arrives:

- **One reader per text form.** A built-in type's text is read by one
  reader, which `text.parse`, a JSON string, a path, a query string, a
  header, a setting and a map's key all use:
  - an `Instant` ends in `Z`, also as a `ZonedTime`'s instant in JSON and
    as an `InstantRange`'s ends;
  - an `Int` is digits, with a `-` when negative;
  - a `Decimal` has places after a point, with digits on both sides;
  - a duration's days are digits.

  Text is composed to NFC first. So an `Int` sent as `"+5"`, a `Decimal`
  written `1_000`, `.5` or `--5`, a duration of days with a sign
  (`P+1D`) and a whole-number standard type written `-0` are refused.
- **No request input carries an offset.** Every instant on the wire ends
  in `Z` (`"2026-01-15T12:00:00Z"`), as [HTTP](07-http.md) and
  [Time types](10-configuration-and-time.md#time-types) say. An offset is
  only a way to write an `Instant` literal in source, so a request, a
  setting and `json.read` refuse one (`2026-10-25T10:30:00+01:00`), also
  as a `ZonedTime`'s `instant`, breaking `format`.
- **A place that holds text reads the canonical text** of every standard
  type that has one. A map's key is a `Money`, a quantity, a `ZonedTime`,
  a `Range` or a standard object sent as its fields, written as
  `text.from` writes it. A whole number with a text form of its own
  (`HttpStatus`, `BasisPoints`) is read as `text.parse` reads it, so `-0`
  isn't one.
- **No object in a URL.** A query string and a path's placeholder hold no
  object ([The query string](07-http.md#the-query-string),
  [Paths and placeholders](07-http.md#paths-and-placeholders)). The
  contract's JSON sends an amount of money, a quantity and a `ZonedTime`
  as an object, so each is `route.query_field_unsupported` or
  `route.placeholder_type` there, as an object is. A standard object with
  a canonical text stays out of a query string too, since a query string
  takes only scalars and enums.
- **A setting holds the types
  [What a setting can hold](10-configuration-and-time.md#what-a-setting-can-hold)
  lists,** read by `text.parse`'s canonical text. The table's "Settings"
  row names their encoding; it doesn't add types. A `Duration` setting
  reads `5m` and `PT5M` alike.
- **A standard named collection crosses a boundary as its type allows:**
  `type.map_key_boundary` doesn't refuse a `LocalisedText`, whose keys
  are locales' tags, in an `Input` or an `Output`.

### JSON at a boundary

Every standard type's JSON has one encoder:

- **Each type's JSON is read and written from one description of it.**
  `Money`, `ZonedTime` and every quantity are sent as an object of the
  members their type lists, each a kind of member: a whole number of
  minor units, a standard enum's code, an instant, a decimal written as
  text, or one of the kind's units. OpenAPI describes it from the same
  list.
- **A part is described as its type.** In OpenAPI, a standard object's
  part that is a standard type a built-in type holds is that type's
  schema: `OidcClaims`' `email` is a string of `format: email` and its
  `picture` one of `format: uri`.
- **Zones and sign counts.** A `ZonedTime`'s `zone` is read ignoring
  case, as a `Zone` is, and OpenAPI lists its zones by name, as it does a
  `Zone`'s. A `WebAuthnCredential`'s `sign_count` is a JSON number.
- **A JSON object that names a member twice isn't a document Jadpo
  reads.** `json.read` gives `malformed`, and a request's body is refused
  as any body that isn't JSON is, whether the object is an `Object`'s, an
  `Input`'s or a map's. Another reader might keep the other value, so the
  value read could be one the sender didn't mean; `crypto.verify_jwt`
  refuses a token whose claims do the same. A set's repeated item stays a
  `duplicate` problem, since it is a value, not the document's shape.
- **The reader is Jadpo's own:** RFC 8259's grammar as `serde_json` reads
  it, with the same 128 levels of nesting. It reads every number's double
  correctly rounded, and keeps each number's text as written beside the
  document.

### SQL literals

A literal in SQL is the value the storage codec makes for it:

- **The literal is decoded for its field first,** exactly as a value in a
  request body is: `"2026-10-25"` where a `CalendarDate` goes is the date,
  `5` where `scale: 2` goes is `5.00`, `20%` is the fraction `0.20`,
  `2.5km` its metres, and `Currency.gbp` the code `GBP`. The same codec
  then makes both the runtime's parameters and the compiler's literals.
- **A parameter wherever the statement takes one.** A query's, a `get`'s
  and an include's conditions, a lifecycle's `visible:` and `when:`, a
  transition's `set:`, a create's `initial:`, a purge, and a membership's
  roles in policy bind each literal and enum value as a parameter, the
  value the codec made. PostgreSQL plans an unnamed statement with its
  parameters' values, so a bound `visible:` still proves a partial
  index's predicate. `jadpo sql` shows a bound literal's parameter as SQL
  writes its value: `$3: 'live' · text`.
- **Written as SQL text only where a statement takes no parameters:** a
  partial index's predicate (DDL), a migration's backfill (its
  `postgres.sql` runs as written), and the identity statements whose
  parameters are fixed (a passkey's, a session's and a service key's
  lookup, for the caller's `active:` and its policy). The compiler's own
  text, a column's comment and the codes a check constraint names, is
  escaped the same way.
- **How the SQL text is written.** It is PostgreSQL's spelling of the
  codec's value, cast where the bare spelling could read as another type:
  - text is quoted, with each `'` doubled. When it holds a backslash or an
    ASCII control character, it is an escape string (`E'...'`) with each
    backslash doubled and each control character escaped. So
    `standard_conforming_strings` can't change its meaning, and it stays on
    one line: a migration's sections are read line by line, so a `\r`
    would be dropped, and a line of text starting `-- store:` would be read
    as a section of its own;
  - an instant is written in UTC with `+00`, so the session's `TimeZone`
    can't change it;
  - a number is written only when it is digits, a sign and a point.
- **Text holding NUL is never stored.** PostgreSQL text can't hold the
  NUL character: a backfill holding it is `migration.decision_invalid`, a
  predicate holding it isn't written (the index holds every row), and a
  condition binds it, which the store refuses as it refuses a runtime
  value holding one.
- **A backfill takes every literal its field's type has,** an instant, a
  date, a duration, a quantity and a percentage included, each checked as
  any literal of its type is, so a standard type's own check (an email
  address's) refuses a backfill that breaks it.
- **A literal is never absent,** so comparing an optional field with a
  literal in a `where:` line, or testing it `in` a list of literals,
  compiles; it isn't `query.optional_value`.

Comparing a JSON-held field in a query's condition isn't built yet: it is
`toolchain.not_built_yet`, as a condition on a compound type is.

## Changing a standard type's encoding
<!-- topic: standard-type-encodings (was changing-a-standard-type-s-encoding) -->

Upgrading Jadpo never makes your stored data unreadable. A Jadpo release
changes how a standard type is sent, stored or written as text only
together with a reader for the old form and a release note. A golden file
per type, built from its examples, pins every form, so a change can't pass
unnoticed.

A value outlives the release that wrote it wherever it is kept: in a
column, in an event waiting to be delivered, in a passkey, in a signed
session cookie. Each of those records which encoding wrote it. So a
release that changes a form reads what an earlier release kept with that
release's reader, and an upgrade never strands it.

The encodings each release writes, as its release notes list them:

| Encoding | Release | What it changed |
|---|---|---|
| 1 | 0.1.0 | The first: every standard type's forms as its golden file holds them |
| 2 | the next | `Jwk` holds `key_ops`, sent as RFC 7517's member and stored in a `text[]` column of its own; encoding 1's `Jwk` reads as `none` there |

- **What a golden file pins.** For each valid example of the type: its
  canonical text (`none` for a type with no single text form, or for a
  secret value), the JSON the route codec writes, and each storage
  column's value as it is written into a statement, which is the value
  the runtime binds. Above them it records each column's suffix,
  PostgreSQL type and comment. A secret value's JSON is the redaction the
  codec writes, and a secret value is stored `never`. `Feature` and
  `FeatureCollection` are read and written through a program whose
  properties are `{ name }`, as their examples' are, since their second
  type argument is always your app's own object. People's formats aren't
  pinned: CLDR's wording isn't an encoding, and no stored value holds it.
- **One encoding number for a whole release,** currently 1, not one per
  type. A row or a token holds values of many types, so one number says
  which reader each needs. A golden file records the encoding that last
  changed it. A form whose type's file records an encoding at or past the
  release's can't be rewritten, so changing a form means raising the
  encoding, and the release notes' row for that encoding must name every
  type whose file it changed. A new type, or an example added or dropped,
  changes no form. A type dropped from the release keeps failing the
  test, since a release keeps reading every type it wrote.
- **Where the encoding is recorded:** in a signed part's payload (`enc`);
  in `jadpo_outbox.encoding` for an event's payload, which a delivery
  reads it by; in `jadpo_batch_runs.encoding` for a batch run's saved
  values and `key:`; and in `jadpo_passkeys.encoding` for a passkey, which
  is stored as `WebAuthnCredential`'s contract JSON
  ([Encodings in system tables](05-data.md#encodings-in-system-tables)).
  A sealed value keeps its own version byte, which says how it is sealed,
  not how what it holds is written. An idempotency key's stored response
  is the bytes that were sent, read back as they were, and holds no
  encoding.
- **What was kept before encodings were recorded** is read as it was
  written, by the reader it was kept with: a signed part without `enc`,
  whose members are encoding 1's, and a passkey whose `encoding` is
  `NULL`, whose count is a number or its digits.
- **An encoding this release doesn't know,** a later release's, is never
  guessed at. A signed part with one opens nothing and the record is
  read, a passkey is refused, an event's delivery is parked and a batch
  run ends `failed`.
- **An entity's own columns record no encoding.** A form that changes
  their columns' types shows in the migration snapshot, so `jadpo check`
  asks for a migration. A form that changes what a column of the same
  type holds needs the release to provide the migration that rewrites
  stored values. No release has needed one yet.

## Packages use the standard types
<!-- topic: packages-use-the-standard-types -->

When you declare a type with the same shape as a standard type, the
compiler reports the error `type.standard_exists` and points you at the
standard type. It does this in a package's public types (the ones
`surface.jadpo_record` lists) and in your app's own code alike, because
an app's `Location { lat, lng }` reaches its API and its tables exactly
as a package's would. You can
[decline](01-project-layout-and-modules.md#declining-a-judgement-call) it,
so a type with a genuinely different meaning is kept, with a reason on
its declaration.

The check matches by shape and by the common names for each field, not by
exact names. It matches an `Object` declaration whose only fields are
those in the table below, in any order, none of them optional (`T?`).

Only distinctive shapes match, to keep false matches rare across hundreds
of types. A shape matches when it has:

- at least two fields;
- at least one field name that belongs to the domain rather than being
  generic: `latitude`, `currency` and `iban` count, but `name`, `value`,
  `id`, `code`, `type` and `text` don't;
- field types that line up with the standard's, such as numbers for
  coordinates, or text or an enum for a currency;
- no fields beyond those.

Single-value types never match by shape. A declaration with a standard
type's name is already an error, and a differently named `Text` says
nothing about what it holds.

| Standard type | Matches an `Object` whose only fields are |
|---|---|
| `Point` | a latitude (`latitude`, `lat`) and a longitude (`longitude`, `lng`, `lon`, `long`), both built on `Int` or `Decimal` |
| `Money` | an amount (`amount`, `value`, `amount_minor`, `minor`, `cents`, `pence`) built on `Int` or `Decimal`, and a currency (`currency`, `currency_code`) built on `Text` or an enum |
| `InstantRange` | a start (`start`, `starts_at`, `start_at`) and an end (`end`, `ends_at`, `end_at`), each built on `Instant` |
| `Colour` | `r`, `g`, `b` and optionally `a`, or `red`, `green`, `blue` and optionally `alpha`, built on `Int` or `Decimal` |
| `LineString`, `Polygon` | a list of a type that itself matches `Point` (`points`, `coordinates`), and nothing else |

The other vocabulary types are single values (`Email`, `Country`,
`PhoneNumber`), where a shape says nothing, so a package's
`type Iso2 = Text { pattern: "[A-Z]{2}" }` isn't flagged.

Here is the error for a package that declares its own point:

```text
error: `GeoPoint` is the standard `Point` under another name
 --> types/geo_point.jadpo:3:13
  |
3 | public type GeoPoint = Object {
  |             ^^^^^^^^ a latitude and a longitude
  reason: `GeoPoint` has only a latitude (`lat`) and a longitude (`lng`), which is the standard `Point`. Packages that each declare their own disagree on order and on names, and an app has to convert between them.
  next: Use `Point` in the package's public types, converting at the edge with `geo.from_lat_lng` or `geo.from_lng_lat` where the provider sends a pair, or write `decline type.standard_exists: "<why>"` above it if this is deliberate (agent)
  rule: type.standard_exists
  help: https://jadpo.dev/docs/errors/type.standard_exists
```

The error stops `check` and the build until you do one of two things.

### Swap to the standard type

For a type not yet stored or on the wire, the fix makes the swap in one
step, and an agent may apply it: `jadpo fix <name> swap`, or the editor's
quick fix ([A release that adds a standard type](#a-release-that-adds-a-standard-type)).
Where the provider sends another form, convert at the edge, in a
package's `wire:` or `decode:`.

For a type already stored or in an API record, the fix writes the edit
and the data migration, and the change goes through the existing
`ApiChange` and `DataChange` [acceptances](18-records-and-manifest.md#acceptances).

### Decline, with a reason, on the type itself

Write the usual [decline line](01-project-layout-and-modules.md#writing-a-decline)
directly above the declaration:
`decline type.standard_exists: "Grid squares, not WGS84 degrees"`. An
agent or a person may decline. `jadpo approval` lists every decline with
its reason, so declining by reflex shows in review.

### False matches

The cost of matching by shape is false matches: an `Object` of `x` and
`y` grid cells, or an `amount` and `currency` that are a quote rather
than money, matches. The required reason handles them, and review sees
each one.

### Where the error is reported

The error above is a package's. `jadpo package check` reports it in a
package's own repository, and `jadpo check` reports it for an installed
package or your app's own integration, on its `public` types only. A type
a package keeps to itself is the shape it decodes from
([Decoding](11-integrations.md#decoding)). On your app's code the error
reads as under [Errors](#errors).

`Colour`'s alpha is the first field a shape lets an `Object` leave out.
`LineString` and `Polygon` match through a list, which is one shape, so
the error names both ([Interop](20-geospatial.md#interop); its example is
under [Errors](#errors)).

### Reference lists ship with Jadpo

The `Country`, `Language` and `PhoneNumber` lists ship with each Jadpo
release, as the time-zone database does. Fixed reference data is declared
in the release, as these enums, not by applications.

Not built yet: the data migration and the acceptances for swapping a type
already stored or on the wire come with
[Records and manifest](18-records-and-manifest.md), and until then such a
swap stops.

## A release that adds a standard type
<!-- topic: standard-type-clashes (was a-release-that-adds-a-standard-type) -->

When a Jadpo upgrade adds a standard type with a name your app already
uses for a type of its own, your app never breaks silently. The first
`check` after the upgrade reports each clash as the error
`type.standard_clash`, naming the standard type and the release that
added it. Each release's notes list the names it adds. There are two
fixes:

- **Swap to the standard type.** This is the same swap as
  `type.standard_exists` offers: every reference to your type becomes a
  reference to the standard one, each field the shape matched is read by
  the standard field's name, and your declaration goes.
- **Rename your type,** keeping its own meaning under a name of its own.

`jadpo fix` applies whichever you pick, through the same code as the
editor's quick fix ([Editor support](14-tooling.md#editor-support)).

The clash is reported on every declaration named like a standard type,
whenever it was written. The checker keeps no record of the release a
project last checked with, so "the first `check` after the upgrade" is
every `check` until the clash is fixed.

For a standard type's name, `type.standard_clash` replaces
`semantic.duplicate_declaration`'s "is a built-in type". A language
type's name, such as `Text` or `Int`, keeps
`semantic.duplicate_declaration`. The clash can't be declined, since two
declarations can't share a name, and a declaration it names is never
also `type.standard_exists`. While it stands, a reference to the name
means the standard type. The errors that gives are the swap's, and the
rename clears them.

`jadpo fix` with no arguments lists each finding it can fix.
`jadpo fix <name> swap` or `jadpo fix <name> rename <NewName>` makes one,
listing what changed file by file:

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

### The swap

The swap is offered on a type only. A failure, an event, an email or an
entity named like a standard type is renamed instead.

The swap deletes the declaration, with its comments and `decline` line,
and its file when nothing else is in it. It writes the standard type's
name at each reference in the files that can see the declaration. It
renames each field the shape matched where it is read: in a body by what
the name resolved to, elsewhere as `Type.field` is written. And it takes
the name out of `use` lines. Whether the standard type fits is decided by
checking the result, not by comparing shapes beforehand.

### The rename

The rename writes the new name at the declaration and at every reference
in the files that can see it, `use` lines included. It moves the file
named after the declaration to the new name's file. The new name must be
an unused `UpperCamelCase` name that reads back from its file name, or
`jadpo fix` refuses it with `cli.fix_arguments`.

The rename belongs to `jadpo fix` alone and isn't a general
`jadpo rename`, so it writes no identity line
(`database/identities.json`). A rename whose result changes stored data
or an API record stops on that error. The editor offers only the swap,
since a quick fix can't ask for a name.

### Naming what to fix

The name you give `jadpo fix` is the declaration's, as its finding names
it. A name that two domains each declare privately is refused, naming
both places.

### A fix never adds an error

Each fix is checked once made. A fix whose result has an error the
project didn't have, by rule and headline, changes nothing and shows that
error. A swap to a standard enum that lacks a variant your code names is
refused this way. Errors the project already had stay, so you fix one
finding at a time even while other errors stand: `jadpo check` shows
them, and `jadpo fix` says how many findings are left. So a swap of a
type already stored or in an API record stops, until its migration and
acceptances are built.

### Names each release adds

Each standard type records the release it ships in, as
`major.minor.patch`. Every type built before Jadpo's first release
carries `0.1.0`, the workspace's version. The names each release adds,
as its release notes list them:

| Release | Standard types it adds |
|---|---|
| 0.1.0 | `AbaRoutingNumber`, `Allergen`, `AltText`, `Angle`, `ApiKey`, `Area`, `AspectRatio`, `AudioCodec`, `BankAccount`, `Barcode`, `BasisPoints`, `BearerToken`, `Bearing`, `Bic`, `Bitrate`, `BloodGlucose`, `BloodPressure`, `BloodType`, `BookingReference`, `BoundingBox`, `CabinClass`, `CalendarDate`, `CalendarEvent`, `CalendarPeriod`, `CallingCode`, `CardBin`, `CardBrand`, `CardExpiry`, `Carrier`, `CefrLevel`, `Certificate`, `ChatMessage`, `ChatRole`, `Cidr`, `Circle`, `ClothingSize`, `ClothingSizeSystem`, `Colour`, `Concentration`, `ContainerImage`, `ContainerNumber`, `ContentRating`, `ContentRatingScheme`, `Continent`, `Cookie`, `Country`, `CountrySubdivision`, `Crs`, `Currency`, `Current`, `Cve`, `DataSize`, `DateRange`, `Digest`, `Dimensions`, `Discount`, `Doi`, `DomainName`, `DosageInstruction`, `Dose`, `DrivingLicenceNumber`, `Duration`, `Ean13`, `EastingNorthing`, `Email`, `EmailWithName`, `Embedding`, `Emoji`, `EncryptedValue`, `Energy`, `EpcRating`, `ExchangeRate`, `ExifOrientation`, `FamilyName`, `Feature`, `FeatureCollection`, `FileName`, `FilePath`, `FileType`, `FlightNumber`, `Fraction`, `FrameRate`, `Frequency`, `Geohash`, `Geometry`, `GitRef`, `GivenName`, `H3Cell`, `Hash`, `HeartRate`, `Hmac`, `Hostname`, `HsCode`, `Html`, `HttpHeaderName`, `HttpMethod`, `HttpStatus`, `IataAirline`, `IataAirport`, `Iban`, `IcaoAirport`, `Icd10Code`, `IdempotencyKey`, `IdentityDocument`, `IdentityDocumentKind`, `Illuminance`, `ImageDimensions`, `Imei`, `Incoterm`, `Insets`, `Instant`, `InstantRange`, `InterestRate`, `InterestRateBasis`, `InvoiceNumber`, `IpAddress`, `Isbn`, `Isin`, `IsoWeek`, `JsonPointer`, `Jurisdiction`, `Jwk`, `Jwt`, `JwtClaims`, `Language`, `Latitude`, `LawfulBasis`, `Lei`, `Length`, `LineString`, `LocalDateTime`, `LocalTime`, `Locale`, `LocalisedText`, `LogLevel`, `Longitude`, `LotNumber`, `MacAddress`, `MapTile`, `Markdown`, `MaskedCardNumber`, `Mass`, `ModelId`, `Money`, `Month`, `Mpan`, `Mprn`, `MultiLineString`, `MultiPoint`, `MultiPolygon`, `Nationality`, `NetworkPort`, `NhsNumber`, `OAuthScope`, `OidcClaims`, `OneTimeCode`, `OpeningHours`, `Orcid`, `PackageUrl`, `PaperSize`, `PassportNumber`, `Password`, `PasswordCheck`, `PasswordHash`, `PasswordPolicy`, `PaymentCardNumber`, `PaymentReference`, `Percentage`, `PersonName`, `PhoneExtension`, `PhoneNumber`, `Point`, `Polygon`, `PostalAddress`, `PostalCode`, `Power`, `Pressure`, `Price`, `ProblemDetails`, `PublicKey`, `PushToken`, `Quarter`, `Range`, `Rating`, `Ratio`, `Rect`, `RecurrenceRule`, `RegularExpression`, `RelativeUrl`, `RichText`, `RichTextBlock`, `RichTextMark`, `RichTextSpan`, `RouteOfAdministration`, `SecretText`, `SemanticVersion`, `SerialNumber`, `ShoeSize`, `ShoeSizeSystem`, `Sku`, `Slug`, `SnomedConcept`, `SoundLevel`, `SpanId`, `SpdxLicence`, `Speed`, `Sscc`, `SubtitleCue`, `TaxId`, `TaxRate`, `Temperature`, `TemperatureDifference`, `Tenure`, `TextDirection`, `Timecode`, `TokenCount`, `TokenUsage`, `TotpSecret`, `TraceId`, `TraceParent`, `TrackingNumber`, `UkAccountNumber`, `UkCompanyNumber`, `UkNationalInsuranceNumber`, `UkSortCode`, `UkUtr`, `Ulid`, `UnLocode`, `UnNumber`, `UnitPrice`, `UnitQuantity`, `UpcA`, `Uprn`, `Url`, `UrlPath`, `UsEin`, `UsSsn`, `UserAgent`, `Username`, `UtcOffset`, `UtmParameters`, `Uuid`, `VatNumber`, `Vector`, `VehicleRegistration`, `VersionRange`, `VideoCodec`, `VideoResolution`, `Vin`, `Voltage`, `Volume`, `WebAuthnCredential`, `WebOrigin`, `Weekday`, `WritingScript`, `Year`, `YearMonth`, `Zone`, `ZonedTime` |

The example of `type.standard_clash` is under [Errors](#errors). The
editor's quick fix is titled "Swap for the standard ...".

Not built yet: the migration and acceptances a swap of a stored type
needs ([Records and manifest](18-records-and-manifest.md)), and a general
`jadpo rename` ([the roadmap](../roadmap.md)).

## What every operation follows
<!-- topic: what-every-operation-follows -->

Every namespace follows the same rules, so a call you haven't seen before
reads like one you have. Here are some calls, and what they give:

```jadpo fragment
text.trim(input.title)                                   // "  Rye loaf " gives "Rye loaf"
text.split("flour,sugar,,eggs", ",")                     // ["flour", "sugar", "", "eggs"]
text.pad_start(text.from(order.number), 6, fill: "0")    // 42 gives "000042"
list.sum(list.map(basket.lines, line -> line.price * line.count))
set.contains(recipe.allergens, Allergen.tree_nuts)
encoding.to_hex(hash.sha256(upload.body))                // 64 lowercase hexadecimal digits
attempt encoding.from_base64(input.attachment) {
    malformed: AttachmentUnreadable
}
```

- **One namespace per type it works on:** `list.`, `set.` and `map.` for
  `List<T>`, `Set<T>` and `Map<K, V>`, `text.` for text, `money.` for
  `Money`. The namespace says what the first argument is, so
  `set.contains` and `list.contains` stay distinct where they behave
  differently (order, repeats).
- **Always called through its namespace:** `text.trim(name)`, never
  `trim(name)` ([Project layout and modules](01-project-layout-and-modules.md)).
  A namespace's name can't be used to name a declaration, domain,
  library, parameter or variable. A field or a named argument can still
  be called `text`, since `note.text` and `set:` are never read as the
  namespace.
- **A moved operation keeps working while you rename the call.** A call
  to an operation, or through a namespace, that the library has moved is
  the error `semantic.standard_operation_moved` where it is resolved,
  with an edit to the new name. Meanwhile it is checked as the operation
  it moved to.
- **Pure, with one named exception.** Each operation gives the same
  result for the same arguments, in the interpreter and on every compiled
  target, and may be called in a function. The exception is randomness:
  `random.*`, `crypto.encrypt` and `crypto.hash_password` read the random
  generator, so, as with `clock.now`, you call them only in actions, and
  tests fix their results.
- **Reference data is pinned.** Unicode data (case mapping, white space,
  the slug's decomposition), CLDR formats and ISO lists are fixed
  versions shipped with Jadpo, and upgrading them is a reviewed change, as
  the time-zone database is.
- **Representations in, representations out**
  ([Expressions and operators](03-logic.md#expressions-and-operators)). A
  parameter written `Text` takes any value built on `Text`, `Email` and
  `Url` included, and one written `Bytes` any value built on `Bytes`. A
  result written `Text` is plain text, which you turn into a named type by
  constructing it. A result keeps the argument's named type only where the
  operation returns one of its arguments or their items unchanged
  (`integer.max`, `list.filter`, `set.union`), since only then can no rule
  break.
- **Named arguments only where a value needs saying**
  ([Calling](03-logic.md#calling)). The first arguments are positional,
  and options and modes are named, as `rounding:` is.
- **Failures are typed** ([Failures](04-failures.md)). An operation that
  can fail names each condition it fails on, and the call's `attempt` maps
  each by that name, as `dst:` and `invalid_day:` are mapped in
  [Zones and local times](10-configuration-and-time.md#zones-and-local-times).
  The failure is `invalid_value`, and `invalid:` covers every condition
  without its own arm. Overflow is a defect, as it is for `+`.
- **Bad arguments are ruled out while compiling where they can be,** as
  `/` rules out a zero divisor. Beside `never_zero`, an operation's
  argument can be `never_empty` (a literal isn't `""`, and a value's type
  has a `min_length` of at least 1) or `never_negative` (a literal isn't
  below 0, and a value's type has a `min` of at least 0).
- **Results carry facts for proofs.** A result whose type can't say its
  bounds carries them in the operation table, as `list.count`'s
  `at_least: 0` does ([Values and types](02-values-and-types.md)), so
  constructing a value from it needs no arm for a rule it can't break.
  `at_most_length_of` marks a text result never longer than an argument
  (`text.trim`, `text.keep_first`). So `Title(text.trim(title))` can't
  break `Title`'s `max_length`, though it can still break `min_length` or
  a `pattern`.

## Validating in the middle of a program
<!-- topic: validating-in-the-middle-of-a-program -->

To check a value partway through a program, construct it. `Type(value)`
checks every rule on the type and the types it builds on, and an
`attempt` maps each rule that can break to the failure the caller gets
([Construction failures by rule](02-values-and-types.md#construction-failures-by-rule)).
That holds wherever the value came from: a computation, a part of a
request, a cell of an import, or `json.read`.

For one value, name the rule that can break:

```jadpo fragment
var quantity: Quantity = attempt Quantity(line.count * 2) {
    max: OrderTooLarge
}
```

For a whole record, build each part first, as its field's type. The
object literal then can't fail ([Objects](02-values-and-types.md#objects)):

```jadpo fragment
var name: Badge.name = attempt Badge.name(text.trim(profile.display_name)) {
    max_length: BadgeNameTooLong
}
return Badge {
    name: name
    colour: profile.colour
}
```

For data that arrives as text, `json.read` decodes it and checks every
field rule of the type in one step, so there is no second validation:

```jadpo fragment
var order: OrderImport = attempt json.read(message.body, as: OrderImport) {
    malformed: ImportUnreadable
    mismatch: ImportInvalid
}
```

A rule across fields, such as "ends after it starts", is a named rule on
the object type
([Rules across fields](02-values-and-types.md#rules-across-fields)),
checked by the same construction.

## Operations
<!-- topic: standard-library-operations -->

The sections below list every operation beyond the contract's baseline,
with its signature, failures and SQL form, one section per namespace or
group of types. The baseline itself (construction, `text.parse`,
`text.format`, `text.from`, JSON, storage, `==` and order) is defined
once, under
[Every standard type meets one contract](#every-standard-type-meets-one-contract),
and isn't repeated.

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
| Time and calendar additions | `temporal` | 24 (21 from Configuration and time) |
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
| Locale | `locale` | 1 ([Locales](10-configuration-and-time.md#locales)) |

## The standard operator table
<!-- topic: the-standard-operator-table -->

Operators on standard types work by one fixed table rather than a
library function per job, so `price * 20%` and `distance / 2h` read as
they would on paper. The compiler knows each row and checks an operator
as strictly as a call. Anything not in the table is a compile error
naming the two types.

The table extends the rule for time in
[Arithmetic and comparison](10-configuration-and-time.md#arithmetic-and-comparison),
where `Instant - Instant` gives a `Duration`. The rules for numbers and
for `+` on text in
[Expressions and operators](03-logic.md#expressions-and-operators) still
hold.

| Left | Operator | Right | Gives | Notes |
|---|---|---|---|---|
| `Instant` | `+`, `-` | `Duration` | `Instant` | [Time](10-configuration-and-time.md#arithmetic-and-comparison) |
| `Instant` | `-` | `Instant` | `Duration` | [Time](10-configuration-and-time.md#arithmetic-and-comparison) |
| `Duration` | `+`, `-` | `Duration` | `Duration` | [Time](10-configuration-and-time.md#arithmetic-and-comparison) |
| `CalendarDate` | `+`, `-` | `CalendarPeriod` | `CalendarDate` | Years, then months, then days; a day past the month's end clamps to its last day |
| `CalendarPeriod` | `+`, `-` | `CalendarPeriod` | `CalendarPeriod` | Part by part (`P1M + P10D` is `P1M10D`) |
| `CalendarPeriod` | `*` | `Int` | `CalendarPeriod` | Each part multiplied |
| A quantity | `+`, `-`, `*`, `/` | as tabled | as tabled | "Physical quantities" below |
| `Money` | `+`, `-`, `<` ... | `Money` | `Money`, `Bool` | One currency (below) |
| `Money` | `*` | a number or `Percentage` | `Money` | Rounded by the type's `rounding:` |
| `Money` | `/` | `Decimal` | `Money` | A rate (`/ 1.20` takes VAT off), rounded by the type's `rounding:` |
| `Money` | `/` | `Money` | `Decimal` | One currency; as `/` gives for `Decimal` ([Logic](03-logic.md#expressions-and-operators)) |
| `Money` | `/` | `Int` | refused | `type.money_split`: use `money.split` or `money.allocate` |
| `Money` | `+`, `-` | `Percentage` | refused | Ambiguous: write `price * 120%` or `price + price * 20%` |
| `Percentage` | `+`, `-` | `Percentage` | `Percentage` | `20% + 5%` is `25%` |
| `Percentage` | `*` | a number or `Percentage` | `Percentage` | `50% * 20%` is `10%` |
| A number or quantity | `*` | `Percentage` | the same kind | `distance * 10%` |

### Money

- **Money in one currency.** Where both types fix the currency
  (`type CakePrice = Money { currency: Currency.gbp }`), adding two
  currencies is a compile error. Where a currency is known only while the
  program runs, as in a SaaS app billing each customer in their own, `+`,
  `-` and comparisons can fail with the condition `currency_mismatch`. You
  map it as any construction rule is mapped:
  `attempt subtotal + line.amount { currency_mismatch: InvoiceCurrencyMismatch }`.
  Inside `if (money.currency(a) == money.currency(b))` the checker has
  the proof and the operator is plain, as a range proves a construction
  ([Conditions as proof](02-values-and-types.md#conditions-as-proof)).
- **Two amounts follow the named-type rule** in
  [Expressions and operators](03-logic.md#expressions-and-operators):
  `+`, `-` and `/` take two amounts of one named type, or one and a plain
  amount that an operator or a `money` operation made. A currency that
  both sides fix differently is reported first, as
  `type.arithmetic_operand` or `type.ordering_operand`.
- **What the checker knows of a result's currency.** Operators give a
  plain `Money`, and the checker carries what it knows of its currency,
  as it carries a number's range: a type's `currency:`, a currency
  written out in `money.from_minor`, `of` or `sum`, one currency through
  `+`, `-`, `*` and `/`, and a condition that compares currencies.
- **`==` and `!=` never fail.** Two amounts in different currencies are
  unequal, as the contract's `==` says. Only `<`, `<=`, `>` and `>=` need
  one currency.
- **An `attempt` around a chain of operators covers every operator in
  it,** as in `attempt a + b + c { currency_mismatch: ... }`, but not an
  operator inside a call. In an `attempt`, a block straight after a
  local's value (`line.amount { ... }`) is the attempt's mappings, not a
  value built from a block, since only a type builds one. `attempt` still
  starts a statement, so a comparison under `attempt` is written as a
  `var`.
- **The money is written first:** `price * 2`, never `2 * price`, which is
  `type.arithmetic_operand`, whose next step writes it the other way.
  `%`, `-price` and `price * price` are refused. `Money / Money` takes a
  divisor as `/` does, a type whose `min` is at least 1, and gives `/`'s
  `Decimal`.
- **Rounding is the type's.** A `Money` type may declare `rounding:`
  (`type CakePrice = Money { currency: Currency.gbp  rounding: Rounding.half_up }`).
  `*` and `/` round to a whole minor unit by the left amount's type's
  `rounding:`, or by `half_even` where it declares none, as constructing
  into a type with a fixed `scale` rounds
  ([Values and types](02-values-and-types.md)). So multiplying needs no
  call, and there is no `money.multiply` or `money.divide`.
- **`/` by an `Int` is refused,** with a fix to `money.split` or
  `money.allocate`. Dividing money by a count is almost always sharing it
  out, and rounding division loses a minor unit: `£10.00 / 3` is `£3.33`,
  and three of those make `£9.99`. `split` hands every penny to someone.
  `/` by a `Decimal` is allowed, because that is a rate, not a share:
  `gross / 1.20` takes VAT off. Halving a price is `price * 50%`.

### Percentages

- **`Percentage` is a type built into the language,** held as the exact
  fraction it is (`20%` is 0.20). Like `Money`, it is a representation: a
  signature names a type built on it
  (`type VatRate = Percentage { min: 0%  max: 100% }`), whose `min:` and
  `max:` are written with `%`, while a `var` may be a plain `Percentage`.
  Percentages are ordered.
- **The `%` literal.** `20%` is a `Percentage`: digits and `%` with
  nothing between. Its fraction keeps the places written and two more
  (`12.5%` is 0.125). It may be negative (`-5%`), as a quantity literal
  may.
- **`%` as remainder** ([Expressions and operators](03-logic.md#expressions-and-operators))
  is always written with spaces, as the formatter writes every binary
  operator, so the two never meet in formatted code. A number, `%` and a
  name or number all touching (`20%3`, `20%rate`) is
  `syntax.percent_ambiguous`, naming both readings. It offers no edit,
  since either could be meant, and the text is read as the remainder so
  the file still parses.
- **A bare number where a percentage goes is refused,** as an argument,
  in a comparison, beside `+` or as a bound, since confusing 0.2 and 20
  is what the type exists to end. The error is `type.mismatch` (or
  `type.incomparable`, `type.ordering_operand`, `type.arithmetic_operand`
  or `type.setting_value`, depending on where the number stands). Its next
  step names both readings, `25%` if `0.25` is a ratio, or `0.25%`, with
  no edit.
- **A ratio becomes a `Percentage` by construction.** Dividing two
  quantities gives a plain `Decimal` (`lip / total`), and
  `Percentage(ratio)` makes it a percentage for display: `0.25` gives
  `25%`. Every value becomes a named type this way, so there is no
  `units` operation for it.
- **`Percentage(ratio)` takes a `Decimal`,** a value or a decimal
  literal, into `Percentage` or any type built on it, checking that
  type's rules. A whole number (`VatRate(20)`) or an `Int` value is
  `type.constructor_input`, naming both readings. A named percentage
  never becomes the plain `Percentage`, as for any representation
  ([Values and types](02-values-and-types.md)).
- **A percentage scales what it follows.** "The same kind" in the table,
  for a number, is a number: `Int * Percentage` gives a `Decimal`, as `*`
  by a `Decimal` does, rather than rounding to a whole number. A
  percentage is written after the amount, number or quantity it scales,
  as a number is (`price * 20%`). `20% * price` and `15% * length` are
  refused, with the fix written the other way, while `20% * 3` is a
  percentage, as the table's own row says. A quantity times a percentage
  rounds half to even to its kind's scale, as it does when multiplied by
  a number. `+` and `-` of two percentages follow the named-type rule.
- **No other row.** Nothing divides a percentage (`rate / 2` is refused;
  `rate * 50%` halves it), adds one to a number or a quantity, or negates
  a percentage value (`-rate`). A quantity and a percentage are reported
  as different kinds. `Money + Percentage` is `type.arithmetic_operand`,
  whose next step writes both readings with the percentage worked out:
  `price * 120%` or `price + price * 20%` (`price * 80%` or
  `price - price * 20%` for `-`), and for a percentage value,
  `price * (100% + rate)`.
- **A percentage's text and wire.** The canonical text is the fraction
  as a percentage with no trailing zeros: `20%`, `12.5%`. `text.parse`
  reads a number, one optional space and `%`, since French and German
  write `20 %`; anything else is `malformed`. On the wire it is that
  text, `"20%"`, rather than the fraction a `Decimal` would send, so no
  client reads 0.2 as 0.2%. A JSON number is the problem `type`, text
  without `%` is `format`, and OpenAPI shows a string with its pattern.
- **A percentage in storage** is one `numeric` column holding the
  fraction, 0.15 for 15%, at whatever places the value has.
- **A percentage for people.** `text.format` is ICU4X's percent formatter
  over CLDR's pattern: the locale's digits and separators, every place the
  value keeps as a percentage, and the `%` where the locale puts it, as
  in `12.5%`, or `20 %` in `de_de` with a no-break space. It takes no
  options. `Display` writes `20%`.
- **A percentage in a message.** In a message's wording,
  `{rate, number, percent}` is the style for a field built on
  `Percentage`, as `{price, number, currency}` is money's, and shows it as
  `text.format` does. `{rate}` alone for a percentage, or either style on
  a field of another kind, is `translation.placeholder_style`, whose edit
  writes the right one. Any other style is `translation.wording_invalid`.

### Ratios and fractions

`Ratio` and `Fraction` are standard objects. A `Ratio`'s `numerator` and
`denominator` are `Decimal`s of any value, as an exchange rate's are. A
`Fraction`'s are whole numbers, its `denominator` at least 1.

Each is held as written, so `16:9` and `32:18`, or `1/2` and `2/4`, are
two different values, as any object's fields compare. Neither is
ordered: no operation reduces them or compares them by value.

Their canonical texts are `16:9` (each number with the places it was
given) and `3/4`, with no spaces, and they are read back exactly so; a
denominator below 1 is `malformed`. Each is sent as that text, shown as it
whatever the locale, and stored in a column per part: `numeric` for a
`Ratio` and `bigint` for a `Fraction`. `type.standard_exists` matches an
object of a `numerator` and a `denominator`, by what they are built on.

### No user-defined operators

Your app's types can't declare operators, so `a + b` always means a row
of this table or of
[Expressions and operators](03-logic.md#expressions-and-operators). A
reviewer never has to find an overload to know what an operator does,
whether it can fail or what it costs.

### VAT, offcuts and pass rates

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
is. Here `Voucher` is an [entity](05-data.md#entities-and-the-id), a kind
of stored record:

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

## SQL forms
<!-- topic: sql-forms -->

A **SQL form** is a PostgreSQL 16 expression that gives exactly the result
the runtime gives, for every argument. It lets the database do work that
would otherwise take a round trip through your app. It matters in three
places:

- **Computed backfills.** A backfill whose every operation has a SQL form
  runs as one `UPDATE` per batch. Any other runs through
  `jadpo migrate apply`, reading and writing each batch
  ([Computed backfills](05-data.md#computed-backfills)). Both are
  correct; the SQL form is only faster.
- **Aggregate queries.** An aggregate query
  ([Counts, sums and groups](06-reads-and-writes.md#counts-sums-and-groups))
  is always one statement. It writes its time buckets (`temporal.year`,
  `temporal.month` and `temporal.day` of a date) and its averages
  (`decimal.divide`) with their SQL forms.
- **Conditions.** A query's condition may compare an operation of a field
  that has a SQL form, with its other arguments written out, through an
  index on the form
  ([Operations in conditions](06-reads-and-writes.md#operations-in-conditions)).
  Otherwise a query's conditions compute nothing, and a one-step update's
  `set:` still works a stored field out only with `+` and `-`
  ([Counters and other one-step updates](06-reads-and-writes.md#counters-and-other-one-step-updates)).

The rules every SQL form follows:

- **An operation that can fail has no SQL form,** since a statement can't
  map a failure to a row.
- **Some arguments have no form.** An operation's form is written from
  its arguments: each a SQL expression and how it is held (a built-in
  type's column, JSON, a range of a built-in type), money's two columns,
  or what a call writes out, such as a rounding mode, `decimal.round`'s
  `scale:` or `text.pad_start`'s `fill:`. An operation has no form for
  arguments the tables don't cover: a rounding mode that isn't written
  out, `text.from` with `form:` or of a time, or `hash.sha256` of a secret
  value.
- **Given values, never `NULL`.** Code tests a value that may be `none`
  before calling an operation on it, so a form never decides what `none`
  means. `range.intersection`, whose result may be `none`, gives `NULL`
  for it.
- **A whole number is always `bigint`.** A field with a narrow range is
  stored in a `smallint` or an `integer`
  ([Storage types](05-data.md#storage-types)), whose `abs` and `/`
  overflow at -32768 where an `Int`'s don't. So `integer`'s operations
  widen their arguments first, and `char_length` and
  `jsonb_array_length` are cast, so arithmetic on what they give
  overflows exactly where the runtime's does. A value tested against a
  range is cast to the range's end type for the same reason.
- **A decimal keeps the places the runtime keeps.** A `numeric` carries
  its places as a runtime `Decimal` does, so the forms read them with
  `scale()`. `decimal.round` keeps a value with at most `scale:` places
  as it is, where `round(x, s)` adds zeros (`1.5` to two places stays
  `1.5`). `decimal.divide` works the quotient out from the two mantissas
  with `div` and `mod`, exactly, as the runtime does, never with
  PostgreSQL's `/`, which rounds to the places it chooses.
- **A defect stops the statement.** Where the runtime stops with a
  defect, a whole number past 64 bits or a decimal past 38 digits, the
  form fails too: `bigint` overflows where an `Int` does, and
  `decimal.divide`'s mantissa passes through `numeric(38, 0)`, so a
  backfill stops at the row the runtime would.
- **`temporal.period_between` without `age`.** The form counts the
  months between the two months, one fewer when the end's day of the
  month hasn't reached the start's (one more going back), then steps once
  toward zero where adding them to the start, years first and then
  months, each clamped to a month's last day, would pass the end; the
  days are what is left. The runtime steps until it stops passing; once
  is always enough, since a clamped day is never past the start's. Year
  0 is PostgreSQL's 1 BC, whose `extract` gives -1, so the form counts
  from year 0 as the runtime does. It names its two dates once, in a
  subquery, and gives the three columns a `CalendarPeriod` is stored in.
- **Counts and positions clamped to `integer`.** `left`, `right`,
  `substr` and `lpad` take PostgreSQL's 32-bit `integer`; text is never
  longer than a gigabyte, so a larger count means what the largest does
  and is clamped rather than cast, which would fail.
- **Text in code points, whatever the collation.** Text is compared and
  ordered in code points, as queries sort it
  ([What can be sorted](06-reads-and-writes.md#what-can-be-sorted)), with
  `COLLATE "C"` indexes ([Indexes](05-data.md#indexes)). So a SQL form
  never depends on the database's collation. That rules out `lower` and
  `upper`: PostgreSQL 16's follow the collation, and under `"C"` they
  change only ASCII letters. `ends_with` compares with `COLLATE "C"`;
  `strpos`, `starts_with`, `replace` and `btrim` match code points
  already. `trim`'s characters are Unicode's 25 `White_Space` characters,
  which Rust's `str::trim` removes.
- **No session setting changes a form.** Dates are added without a zone,
  and escapes are written `E'...'`, so a session's time zone, date style,
  interval style and `standard_conforming_strings` make no difference.
- **Lists, sets and maps.** A list or set is stored as a `jsonb` array
  and a map as a `jsonb` object ([Data](05-data.md)); only `count` has a
  SQL form.
- **Each form is tested against the runtime before anything uses it.**
  Each operation with a form is run in the runtime and as its form, on
  the same columns a backfill reads, and the two results must match,
  places and widths included. An input the runtime stops on must fail the
  statement. Every form in the tables below is tested over a hundred
  thousand inputs on live PostgreSQL.

A backfill's operators and conditionals have SQL forms too, tested
against the runtime the same way. In this table `a` and `b` are their
operands:

| Operator | Operands | SQL form |
|---|---|---|
| `and`, `or` | `Bool` | `CASE WHEN a THEN b ELSE FALSE END`, `CASE WHEN a THEN TRUE ELSE b END`, so they stop early as the runtime does |
| `not` | `Bool` | `NOT a` |
| `==`, `!=` | two of one kind: whole numbers and decimals, text and enums, `Bool`, `Uuid`, `Bytes`, dates, instants, durations | `a = b`, `a <> b`, text `COLLATE "C"`, whole numbers as `bigint` |
| `<`, `<=`, `>`, `>=` | numbers, dates, instants, durations | `a < b` |
| `+`, `-`, `*`, `/`, `%`, unary `-` | whole numbers | each as `bigint`, so it overflows where an `Int` does; `/` and `%` drop the remainder toward zero |
| `+`, `-` | decimals, or a decimal and a whole number | the sum at the larger scale, stopping where the runtime's 128-bit mantissas or 38 digits would |
| unary `-` | decimals | `-a` |
| `+`, `-` | durations | as `bigint` milliseconds |
| `+` | text | `a \|\| b` |
| `x == none`, `x != none` | a value that may be `none` | `x IS NULL`, `x IS NOT NULL` |
| `if` chains, `match` | a function's `if`s whose branches return; a `match` on a plain variant, `true`, `false`, a literal, `none` or `some` | `CASE`, which works out only the branch it takes |

A decimal's `*` and `/`, `in`, `matches`, and the operators on money,
quantities, percentages, periods and times moved by a duration have no
form yet.

In the tables below, `x` is the first argument and `a`, `b` the others.

## Lists
<!-- topic: lists -->

A list holds items of one type in order, like an array in TypeScript,
except that it never changes: every operation that changes a list gives
you a new one. Each operation is called through the `list` namespace, as
`list.<name>`. In the table below, `T` is the item type.

The list fixes the item type. A list written out of values fixes it for
the call it is passed to, as in `list.sort_by([oat, zopf], product ->
...)`. A list with a literal in it takes the parameter's type.

`collection` is a reserved namespace with no operations. A call through
it is `semantic.standard_operation_moved`, and its edit writes `list.`,
`set.` or `map.`: chosen by the type of the call's first value when that
value is a name or a field, and `list.` otherwise. The call is then
checked as that operation.

Some operations take a function, written inline as `item -> expression`
([Passing functions](03-logic.md#passing-functions)). Three operations
give their function two values: `list.zip_with` gives an item from each
list, and `map.filter` and `map.map_values` give an entry's key and its
value. Their function takes two parameters, written in parentheses,
`(key, value) -> ...`, and only that way. A function with one parameter
is never written `(item) ->`. Each parameter takes the type of the value
the operation gives in its place, and you don't have to use every one.
(The operations' declarations write such a function as
`Function<K, V, U>`, the last type being its result.)

A function with a different number of parameters than the operation
gives is `type.inline_function_parameters`
([Two parameters](03-logic.md#two-parameters)). A function with two
parameters anywhere but these three operations is
`type.inline_function_placement`, and its reason names the three.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `count(items)` | `Int`, at least 0 | | `jsonb_array_length(x)::bigint` |
| `contains(items, item)` | `Bool`: an item `==` `item` | | none |
| `map(items, each)` | `List<U>`, in order | | none |
| `filter(items, keep)` | `List<T>`, the items `keep` accepts, in order | | none |
| `sum(items)` | `Int` or `Decimal`, the items added; `0` for none. Items built on `Int` or `Decimal`; `Money` uses `money.sum` | overflow is a defect | none |
| `any(items, check)` | `Bool`: `check` accepts at least one; `false` for none | | none |
| `all(items, check)` | `Bool`: `check` accepts every one; `true` for none | | none |
| `find(items, check)` | `T?`, the first item `check` accepts | | none |
| `first(items)`, `last(items)` | `T?` | | none |
| `at(items, position)` | `T?`, the item at `position` (0 is the first); `position` never negative | | none |
| `min_by(items, key)`, `max_by(items, key)` | `T?`, the item with the smallest or largest key; the first of equal ones | | none |
| `sort_by(items, key, locale:)` | `List<T>`, ordered by `key` ascending; equal keys keep their order. `key` gives a number, a time, a date, a duration, text or a standard type the contract orders (`Money` of one currency): text in code points as queries sort it, or by a locale's rules with `locale:` | | none |
| `reverse(items)` | `List<T>`, last item first | | none |
| `distinct(items)` | `List<T>`, each item's first occurrence, by `==` | | none |
| `slice(items, from, count)` | `List<T>`, up to `count` items from position `from`, cut short at the end; both never negative | | none |
| `keep_first(items, n)`, `keep_last(items, n)` | `List<T>`, at most `n` items from that end; `n` never negative | | none |
| `drop_first(items, n)`, `drop_last(items, n)` | `List<T>` without `n` items from that end; all gone when `n` is longer; `n` never negative | | none |
| `append(items, item)` | `List<T>`, `item` added at the end | | none |
| `concat(first, second)` | `List<T>`, `first`'s items then `second`'s | | none |
| `flatten(lists)` | `List<T>` from a `List<List<T>>`, in order | | none |
| `group_by(items, key)` | `Map<K, List<T>>`: each key with its items in order | | none |
| `zip_with(first, second, (a, b) -> ...)` | `List<U>`, pairing items by position, as long as the shorter | | none |

- **No push or pop.** A list's items never change
  ([Values never change](02-values-and-types.md#values-never-change)). To
  add an item, `list.append(items, item)` gives a new list. To take the
  last item off, read it with `list.last`, then `list.drop_last(items, 1)`
  gives the rest.
- **The largest is `max_by`,** not `first(reverse(sort_by(...)))`, which
  sorts the whole list to read one item. In a query, you still write
  `order:` and `limit:`.
- **Cutting from either end.** `keep_first`, `keep_last`, `drop_first`
  and `drop_last` cut items from either end, and text has the same four.
  `slice` is for a window in the middle. No position counts back from the
  end: a count or a position is never negative, and one that might be
  doesn't compile (the operation's `never_negative`). A literal below `0`,
  or a value whose type has no `min` of at least `0`, is
  `type.argument_negative`. A call's result, `list.count`'s included, is
  a plain `Int`, which proves nothing about its sign, so build a count as
  a type with `min: 0` first.
- **Keys that order.** `sort_by`, `min_by` and `max_by` need a key that
  can be put in order: one built on a number, `Text`, `Instant`,
  `CalendarDate` or `Duration`, or a standard type the contract marks
  ordered. The key can never be `none`. A `ZonedTime` has no order of its
  own, just as `<` doesn't compare one
  ([Expressions and operators](03-logic.md#expressions-and-operators)),
  so use its instant as the key (`temporal.instant`). Any other key is
  `type.ordering_operand`. Keys compare in the canonical order, the one
  fixed order Jadpo gives every type's values
  ([Collections](02-values-and-types.md#collections), "Set order"), so
  text sorts by code point, or by a locale's rules with `locale:`
  ([Text](#text)).
- **Named types.** An operation that gives `T`, `T?` or `List<T>` keeps
  the items' named type, such as `Batch` or `BatchWeight`. It doesn't keep
  a named list type, because `filter` or `drop_first` can break that
  type's `min_length`; build the result back into it by construction
  ([Named collection construction](02-values-and-types.md#named-collection-construction)).
  `flatten` takes a `List<List<T>>`. Collections are invariant: a list of
  a named list type doesn't stand in for a `List<List<T>>`.
- **Equal means `==`.** `group_by` groups items whose keys are `==`, and
  gives the keys in the canonical order, so a key may be of any type.
  `distinct` keeps each item's first occurrence, found through the
  canonical order, which agrees with `==` for values of one type.
- **There is no `items[i]`.** Use `list.at`, which gives `T?`, and handle
  the missing case with `match`. An index can be out of range, so
  `items[i]` would need a failure arm or a defect.
- **`sum` gives a representation,** as `+` does: an `Int` or a `Decimal`,
  by its items' representation. An empty list gives `0` or `0.00`, as its
  type says. `Stock(list.sum(counts))` builds the result back into a named
  type. When the items' type has a `min` of at least `0`, the sum is
  proved to be at least `0`, so building it needs no arm for `min`. It
  still needs one for `max` when the type has one.
- **No `reduce`, `fold` or comparator.** To accumulate, use a `for` loop
  with a `var mut`, or `sum` for the common case. To sort, give `sort_by`
  a key, and `reverse` the result for the other direction; there is no
  sort with a comparator function.

A day's baking, through every list operation:

```jadpo check project=list-operations
type BatchName = Text {
    min_length: 1
    max_length: 40
}

type BatchWeight = Mass {}

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
    weight: BatchWeight
    shelf: Shelf
    price: LoafPrice
}

type OverPlan = Bool {}

/// The flour a day's batches take: `list.sum` adds numbers, and
/// `units.sum` adds quantities of one kind.
function flour_needed(bakings: List<Batch>) -> BatchWeight {
    return BatchWeight(units.sum(list.map(bakings, batch -> batch.weight)))
}

/// The day's takings at full price.
function takings(bakings: List<Batch>) -> LoafPrice {
    return LoafPrice(list.sum(list.map(bakings, batch -> batch.price)))
}

/// The heaviest batch, if there is one: `max_by` reads each batch once,
/// where sorting would order them all to read one.
function heaviest(bakings: List<Batch>) -> Batch? {
    return list.max_by(bakings, batch -> batch.weight)
}

/// The batches on each shelf, in baking order.
function by_shelf(bakings: List<Batch>) -> Map<Shelf, List<Batch>> {
    return list.group_by(bakings, batch -> batch.shelf)
}

/// Whether each batch came out heavier than planned, batch by batch: a
/// two-parameter inline function is given one item from each list.
function over_plan(bakings: List<Batch>, goals: List<BatchWeight>) -> List<OverPlan> {
    return list.zip_with(bakings, goals, (batch, goal) -> OverPlan(batch.weight > goal))
}
```

Each operation gives a new list, or an item of the list as it is, still
a `Batch`. `takings([])` is `0.00`, a `Decimal`, because a sum has its
items' representation even when there are none. `by_shelf`'s map is read
with `map.get` and `map.keys` ([Maps](#maps)).

```jadpo check project=list-operations file=main.test.jadpo
test "list operations give new lists and keep the items as they are" {
    var rye: Batch = Batch {
        name: "Rye"
        weight: 800g
        shelf: 2
        price: 4.20
    }
    var bap: Batch = Batch {
        name: "Bap"
        weight: 90g
        shelf: 1
        price: 0.60
    }
    var spelt: Batch = Batch {
        name: "Spelt"
        weight: 800g
        shelf: 2
        price: 3.80
    }
    var day: List<Batch> = [rye, bap, spelt]
    var weights: List<BatchWeight> = [800g, 90g, 800g]
    assert flour_needed(day) == 1690g
    assert takings(day) == 8.60
    assert takings([]) == 0.00
    assert list.any(day, batch -> batch.shelf == 1)
    assert not list.all(day, batch -> batch.weight > 100g)
    assert list.find(day, batch -> batch.shelf == 2) == rye
    assert list.first(day) == rye
    assert list.last(day) == spelt
    assert list.at(day, 1) == bap
    assert list.at(day, 3) == none
    assert heaviest(day) == rye  // the first of equal keys
    assert list.min_by(day, batch -> batch.price) == bap
    assert list.sort_by(day, batch -> batch.name) == [bap, rye, spelt]
    assert list.sort_by(day, batch -> batch.weight) == [bap, rye, spelt]  // equal keys keep their order
    assert list.reverse(day) == [spelt, bap, rye]
    assert list.distinct(weights) == [800g, 90g]
    assert list.slice(day, 1, 5) == [bap, spelt]
    assert list.keep_first(day, 2) == [rye, bap]
    assert list.keep_last(day, 2) == [bap, spelt]
    assert list.drop_first(day, 1) == [bap, spelt]
    assert list.drop_last(day, 5) == []
    assert list.append([rye], bap) == [rye, bap]
    assert list.concat([rye], [bap, spelt]) == day
    assert list.flatten([[rye], [], [bap, spelt]]) == day
    assert over_plan(day, [700g, 100g, 800g]) == [true, false, false]
    assert over_plan(day, [700g]) == [true]  // as long as the shorter
}

test "group_by keeps each shelf's batches in baking order" {
    var rye: Batch = Batch {
        name: "Rye"
        weight: 800g
        shelf: 2
        price: 4.20
    }
    var bap: Batch = Batch {
        name: "Bap"
        weight: 90g
        shelf: 1
        price: 0.60
    }
    var spelt: Batch = Batch {
        name: "Spelt"
        weight: 800g
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
([Errors](#errors)), and a key that gives something with no order, such
as a `Bool`, is `type.ordering_operand`.

## Sets
<!-- topic: sets -->

A set holds items of one type, each only once, and has no order your
code can use. Each operation is called as `set.<name>`, and each one that
changes a set gives you a new one.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `count(items)` | `Int`, at least 0 | | `jsonb_array_length(x)::bigint` |
| `contains(items, item)` | `Bool` | | none |
| `with(items, item)` | `Set<T>` with `item`; the same set if already there | | none |
| `without(items, item)` | `Set<T>` without `item` | | none |
| `union(a, b)` | `Set<T>`, items in either | | none |
| `intersection(a, b)` | `Set<T>`, items in both | | none |
| `difference(a, b)` | `Set<T>`, items in `a` and not in `b` | | none |

- **No loop needed.** Each of these is one operation in the runtime, so
  you never walk a set with `for` to get its result.
- **Named types.** As for lists, an operation that gives `T` or `Set<T>`
  keeps the items' named type. It doesn't keep a named set type, because
  `without` can break that type's `min_length`; build the result back
  into it by construction, as a named collection's lengths require
  ([Named collection construction](02-values-and-types.md#named-collection-construction)).
- **`for` walks a set** in the canonical order of its values
  ([Collections](02-values-and-types.md#collections)), the order
  everything that shows a set uses. The loop's variable takes the set's
  item type, as it does over a list. A set still has no index and no
  first item.
- **There is no `filter`, `any`, `all` or `map` on a set.** Use `for`.

A bakery's labels, through every set operation and a `for` over a set:

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

Two sets are equal when they hold the same items, whatever the order, so
the literal on the right of `==` can be written in any order. Walking a
set is the only place its order shows, and it is always the canonical
order, on every target.

## Maps
<!-- topic: maps -->

A map holds values by key, like a `Map` in TypeScript, except that it
never changes. Each operation is called as `map.<name>`, and each one
that changes a map gives you a new one.

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

- **Walk a map through its keys:** `for key in map.keys(prices)` goes
  through them in the canonical order. `for` over the map itself is
  `type.mismatch`, and its edit writes `map.keys(<the map>)`.
- **An empty map is `[]` written where a map is expected,** just as a
  list literal written where a set is expected is a set
  ([Collections](02-values-and-types.md#collections)). A named map type's
  `min_length` checks that literal's length (`type.invalid_literal`).
  Add entries with `map.with`. There is no map literal with entries, so a
  list with items in it, where a map is expected, is `type.mismatch`.
- **Named types.** As for lists, an operation that gives `V?`, `Set<K>`
  or `List<V>` keeps the named type of the keys or values. It doesn't
  keep a named map type, because `without` can break that type's
  `min_length`.
- **Records have no dynamic keys.** There is no `record[name]`: a
  record's fields are fixed, and each has its own type, such as
  `Customer.email`. Keys known only while the program runs are what a
  `Map` is for.

A shop's stock, through every map operation. `filter` and `map_values`
give their inline function each entry's key and value, in the canonical
order of the keys:

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
<!-- topic: ranges -->

A range is a span of values between a start and an end, such as a
booking's hours or a band of cake sizes. `Range<T>` holds a `start` and
an `end` of one ordered type, and either end may be `none` for an open
end. Two standard ranges say once, in their type, whether they hold
their end: a `DateRange` includes its end date, and an `InstantRange`
excludes its end instant.

A range's start is never after its end, a rule named
`start_not_after_end`. So a range built from values is proved by a
condition or built under `attempt`, as an object's rule across its
fields is ([Rules across a type's parts](#rules-across-a-types-parts)).

Ranges are stored as PostgreSQL's range types (`daterange`,
`tstzrange`, `numrange`, and `int8range` for whole numbers and
durations), so their SQL forms are the range operators. A range that
holds nothing, because its start is its end, isn't stored: PostgreSQL
keeps every such range as `empty`, without its ends, so storing one is a
fault rather than losing where it was.

`T` must be a type `<` orders: one built on `Int`, `Decimal`, `Instant`,
`CalendarDate` or `Duration`. `Text` isn't one, because `<` doesn't take
it. Any other type is `type.ordering_operand`, reported where the type is
written or where a literal's ends give it. `Range` with no type argument
is `type.type_argument_count`.

A `Range<T>` holds its start and not its end, as `InstantRange` does and
as PostgreSQL's canonical ranges do. An open end holds everything on its
side, so a range with two open ends holds every value. Two ranges that
only meet, one's end being the other's start, don't overlap. A range
whose start is its end holds nothing, so `range.overlaps` is `false` and
`range.intersection` is `none` for it, as PostgreSQL's `&&` and
`NULLIF(a * b, 'empty')` give.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `range.contains(range, value)` | `Bool`: `value` is inside, by the range's own end rule | | `x @> a` |
| `range.overlaps(a, b)` | `Bool`: the two share at least one value | | `a && b` |
| `range.intersection(a, b)` | `Range<T>?`, the shared part; `none` when they don't overlap | | `NULLIF(a * b, 'empty')` |

The three operations take a `Range<T>`, an `InstantRange` or a
`DateRange`, which holds its end date. The ranges in one call are all of
one range type, since each type keeps its own rule for its end: an
`InstantRange` and a `Range<Instant>` don't mix (`type.mismatch`). A
range an operation gives is of that same type, so `range.intersection`
of two `InstantRange`s is an `InstantRange?`. `range.contains` takes a
value built on the range's item type.

An `InstantRange` written as an object literal has its two fields
checked as an object's fields declared `Instant` are, so a value of a
named instant type, such as `Moment`, is a mismatch there. A value of the
type, or a field declared `InstantRange`, has its `start` and `end` read
through it (`booking.slot.start`). An `InstantRange` holds every instant
from its start up to, but not including, its end. So two bookings that
meet at the hour don't overlap, and one that starts and ends at the same
instant holds nothing:

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

Write a `Range<T>` as `Range { start: ..., end: ... }`, giving both
ends, with `none` for an open one. Where a `Range<T>` is expected, its
ends are checked as `T?`. Elsewhere, its first end that isn't `none`
gives `T`. Read its ends as `range.start` and `range.end`, each a `T?`.

- **Text.** A range's canonical text is each end's canonical text joined
  by `/`, with an open end written `..`, as ISO 8601-2 writes one
  (`2026-10-01/..`). A start after its end breaks
  `start_not_after_end`.
- **JSON** is an object of `start` and `end`, with `null` for an open
  end.
- **For people,** `text.format` shows each end as its item type is shown,
  with the item type's options, joined by ` – `, and an open end as `…`.
  A range of numbers shows each end as a number is shown
  ([Numbers](#numbers)) and takes no options.
- **Reading text.** `text.parse(raw, as: Range<T>)` reads a range of a
  standard type, or of a representation written as itself (`Range<Int>`,
  `Range<CalendarDate>`). A range of a type with rules of its own, such
  as `Range<LoafWeight>`, is `type.mismatch`, since nothing would check
  those rules on its ends.
- **No order, no shape.** A `Range<T>` isn't ordered.
  `type.standard_exists`, the error for an app type with a standard
  type's shape, matches no shape for it, since `InstantRange` already
  claims a start and an end.

Bands of cake sizes in slices, one with no upper limit:

```jadpo check project=generic-ranges
type SliceCount = Int {
    min: 0
}

type CakeName = Text {
    min_length: 1
}

type Cake = Object {
    name: CakeName
    slices: SliceCount
}

type InBand = Bool {}

/// The cakes sold as large: 16 slices and up, with no upper limit.
function large() -> Range<SliceCount> {
    return Range {
        start: 16
        end: none
    }
}

/// Whether a cake's slice count is in `band`.
function in_band(cake: Cake, band: Range<SliceCount>) -> InBand {
    return InBand(range.contains(band, cake.slices))
}

/// The slice counts two bands share, if any.
function common_weights(first: Range<SliceCount>, second: Range<SliceCount>) -> Range<SliceCount>? {
    return range.intersection(first, second)
}
```

```jadpo check project=generic-ranges file=main.test.jadpo
test "a range of any ordered type, either end open" {
    var medium: Range<SliceCount> = Range {
        start: 8
        end: 16
    }
    var small: Range<SliceCount> = Range {
        start: none
        end: 10
    }
    var both: Range<SliceCount> = Range {
        start: 8
        end: 10
    }
    var rye: Cake = Cake {
        name: "Rye"
        slices: 16
    }
    assert in_band(rye, large())
    assert not in_band(rye, medium)  // a range doesn't hold its end
    assert common_weights(medium, large()) == none  // they meet at 16
    assert common_weights(medium, small) == both
    assert range.overlaps(small, medium)
    assert text.from(large()) == "16/.."
}
```

## Numbers
<!-- topic: numbers -->

Numbers have two namespaces: `integer` operations take values built on
`Int`, and `decimal` operations take values built on `Decimal`. The
operators `/` and `%` are in
[Expressions and operators](03-logic.md#expressions-and-operators);
these operations do what an operator can't.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `integer.divide(dividend, divisor, rounding:)` | `Int` ([Expressions and operators](03-logic.md#expressions-and-operators)) | | every mode: `div` and `mod` with a `CASE` |
| `integer.abs(value)` | `Int`, at least 0 | overflow on the smallest `Int` is a defect | `abs(x)` |
| `integer.min(a, b)`, `integer.max(a, b)` | the smaller or larger, keeping their named type (both the same, as `+` needs) | | `LEAST(a, b)`, `GREATEST(a, b)` |
| `integer.from_decimal(value, rounding:)` | `Int`, `value` rounded to a whole number | overflow is a defect | `round(x)::bigint` for `half_up`; a `CASE` on the dropped digits for the others |
| `decimal.divide(dividend, divisor, rounding:)` | `Decimal` ([Expressions and operators](03-logic.md#expressions-and-operators)) | | every mode: as `integer.divide`, on the values scaled to the dividend's places |
| `decimal.abs(value)` | `Decimal`, at least 0, the same scale | | `abs(x)` |
| `decimal.min(a, b)`, `decimal.max(a, b)` | as for `integer` | | `LEAST(a, b)`, `GREATEST(a, b)` |
| `decimal.round(value, scale:, rounding:)` | `Decimal` with at most `scale` places; `scale:` a whole number written out, at least 0 | | `x` when it has at most `s` places, where `round` would add zeros; otherwise `round(x, s)` for `half_up`, `trunc(x, s)` for `down` and a `CASE` on the dropped digits for the others |
| `decimal.sqrt(value, scale:, rounding:)` | `Decimal`; `value` never negative | | none |
| `decimal.power(value, exponent)` | `Decimal`, exact; `exponent` a whole number, never negative | overflow is a defect | none |
| `decimal.exp(value, scale:, rounding:)`, `decimal.ln(value, scale:, rounding:)` | `Decimal`; `ln`'s `value` above 0 | `ln`: `out_of_range` | none |
| `decimal.sin(radians, scale:, rounding:)`, `decimal.cos(radians, scale:, rounding:)` | `Decimal` | | none |
| `decimal.atan2(y, x, scale:, rounding:)` | `Decimal`, radians | | none |
| `decimal.pi(scale:, rounding:)` | `Decimal` | | none |

- **Rounding to a type's scale needs no call.** Building a value of a
  type with a fixed `scale` rounds it half to even
  ([Conditions as proof](02-values-and-types.md#conditions-as-proof)), so
  `decimal.round` is for another rounding mode, or for fewer places than
  the type keeps.
- **The rounding modes.** `Rounding` has four: `half_even` takes a half to
  the even digit, `half_up` takes a half away from zero, `up` rounds away
  from zero, and `down` rounds toward zero, its SQL form `trunc`. There
  is no `floor`, `ceiling` or `half_down`.
- **An `Int` becomes a `Decimal` by multiplying,** as in `count * 1.00`,
  since `*` may mix the two
  ([Expressions and operators](03-logic.md#expressions-and-operators)).
  There is no `decimal.from_integer`.
- **Parsing and formatting** work as for every standard type, through
  the contract
  ([Every standard type meets one contract](#every-standard-type-meets-one-contract)):
  `text.parse(raw, as: Quantity)` and `text.format(n, locale:)`. Parsing
  takes only the form a number has on the wire: no grouping, no decimal
  comma, no exponent. A spreadsheet's `1.234,50` is a job for a cell
  decoder, a package's `decode:`.
- **Formatting for people** uses CLDR's decimal format for the locale,
  with its digits, grouping and decimal separator, and shows every place
  the value keeps, as a translation's `{n}` writes it
  ([Using a message](10-configuration-and-time.md#using-a-message)):
  `1234.50` is `1,234.50` in `en_gb` and `1.234,50` in `de_de`. It takes
  no options, so `zone:` or `format:` is `type.temporal_option_unused`. A
  `Bool` has no format for people.
- **`min` and `max` take two values of one type,** as `+` does, and give
  one of them as it is. Two named types that aren't one type after
  widening are `type.mismatch`. Where one widens to the other, the result
  has the wider type
  ([When one type can stand in for another](02-values-and-types.md#when-one-type-can-stand-in-for-another)).
  A literal takes the other value's type and is checked against all its
  rules, since the result may be the literal. A plain number, which
  carries no type of its own, makes the result the plain representation,
  and so do two literals. Of two equal values, `decimal.min` and
  `decimal.max` give the first, so `decimal.min(1.50, 1.5)` keeps
  `1.50`'s places.
- **`scale:` is a whole number from 0 to 18,** the range a type's `scale`
  takes ([Representation types](02-values-and-types.md#representation-types)),
  written out at the call. A value there is `type.mismatch`, as it is for
  `text.parse`'s `as:`. A literal below 0 is `type.argument_negative`, and
  any other literal that isn't a whole number up to 18 is
  `type.mismatch`. `decimal.round` gives *at most* `scale` places, so it
  keeps a value that has no more places than `scale:` as it is. The maths
  operations give exactly `scale` places: `decimal.sqrt(4.00, scale: 2,
  ...)` is `2.00`.
- **What the checker knows about results.** These facts let it prove a
  result fits a type without a check
  ([Conditions as proof](02-values-and-types.md#conditions-as-proof)).
  `abs` is at least 0, as the table says. `min` and `max` carry their
  arguments' bounds: the smaller of two values is never above either's
  top. `integer.from_decimal` and `decimal.round` keep the sign of a value
  bounded at zero, since no mode rounds across zero. `sqrt` and `exp` are
  at least 0, and `pi` is at least 3. The checker knows nothing finer: a
  rounded value's other bounds, and anything built from `sin` or `cos`,
  which carry no bounds, need a check.
- **Arguments that must be proved.** `sqrt`'s value and `power`'s
  exponent are never negative, proved as a count is (`never_negative`),
  or the call is `type.argument_negative`. `ln` fails `out_of_range`, and
  that is settled by proof, as a construction's rule is. A literal above
  0, or a value whose type has a `min` above 0, can't fail, so its call
  takes no `attempt`; one written with an `attempt` is
  `failure.attempt_infallible`. Any other value may fail, and the call
  maps `out_of_range` or `invalid:`; its failure is `invalid_value`.
- **Edges of the maths.** `atan2(0, 0)` is 0, where no angle is meant,
  rather than a failure. `atan2(0, x)` for `x` below 0 is π, as `Decimal`
  has no negative zero to make it -π. `power(value, 0)` is 1 at `value`'s
  scale, `0^0` included, and every other power keeps the places repeated
  `*` keeps: `1.50` squared is `2.25`. A `power` past 38 digits or 38
  places, or an `exp` past 38 digits, is overflow, a defect. An `exp` so
  far below 0 that it is under half of 10^-18 gives 0, or, with `up`,
  which rounds away from zero, one unit of the last place.
- **The same digits on every target.** The maths works on `Decimal`, as
  there is no `Float`
  ([Representation types](02-values-and-types.md#representation-types)).
  Jadpo's own decimal code rounds each result correctly to `scale` with
  the named rounding, so it is the same wherever the program runs.
- **In SQL,** PostgreSQL's `round` ties away from zero, which is
  `half_up`. The other modes are spelled out with a `CASE`, never left to
  the database.

A bakery's counts, weights and prices, rounded and compared:

```jadpo check project=number-operations
type Portions = Int {
    min: 0
}

type TrayCount = Int {
    min: 1
    max: 12
}

type LoafCount = Int {}

type FlourWeight = Mass {
    min: 0g
}

type Hydration = Decimal {
    scale: 3
    min: 0.000
}

type LoafPrice = Decimal {
    scale: 2
    min: 0.00
}

/// How far a counted batch is from its recipe, either way. `abs` is never
/// below 0, so building `Portions` from it needs no arm.
function drift(in_batch: Portions, recipe: Portions) -> Portions {
    return Portions(integer.abs(in_batch - recipe))
}

/// The trays to bake: what was ordered, but never more than the oven
/// holds. `min` gives one of its two values as it is, still a `TrayCount`.
function trays_to_bake(requested: TrayCount, oven: TrayCount) -> TrayCount {
    return integer.min(requested, oven)
}

/// Whole loaves a sack of flour makes at 450g each: part of a loaf isn't
/// one, so the count rounds down, toward zero. Dividing a weight by a weight
/// gives a plain `Decimal`.
function loaves_from(flour: FlourWeight) -> LoafCount {
    return LoafCount(integer.from_decimal(flour / 450g, rounding: Rounding.down))
}

/// A loaf's price at a fifth off, never below the lowest price.
function sale_price(price: LoafPrice, lowest: LoafPrice) -> LoafPrice {
    return decimal.max(LoafPrice(price * 0.80), lowest)
}

/// A dough's hydration to one place, for its label.
function label_hydration(hydration: Hydration) -> Hydration {
    return Hydration(decimal.round(hydration, scale: 1, rounding: Rounding.half_up))
}

/// The price as a customer reads it, in their own locale.
function price_label(price: LoafPrice, customer: Locale) -> PresentationText {
    return text.format(price, locale: customer)
}
```

A literal passed to `min` or `max` takes the other value's type and is
checked against it, so `integer.min(requested, 20)` is an error, because
`TrayCount` stops at 12. A plain number, such as `requested + 1`, makes
the result a plain `Int`. The tests check these functions, and each
rounding mode to one place:

```jadpo check project=number-operations file=main.test.jadpo
test "number operations give what the page says" {
    assert drift(980, 1000) == 20
    assert drift(1020, 1000) == 20
    assert trays_to_bake(10, 8) == 8
    assert trays_to_bake(3, 8) == 3
    assert loaves_from(10kg) == 22
    assert loaves_from(449g) == 0
    assert sale_price(4.20, 3.50) == 3.50
    assert sale_price(5.00, 3.50) == 4.00
    assert label_hydration(1.250) == 1.3
    assert label_hydration(1.249) == 1.2
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

A `scale:` that isn't a whole number from 0 to 18 written out at the
call, as a type's `scale` is, is `type.mismatch`, and one below 0 is
`type.argument_negative` ([Errors](#errors)).

The maths operations, from `sqrt` to `pi`, on the bakery's tins,
starters and deliveries. Each result is the true value rounded to
`scale` places by the mode named, so the same call gives the same digits
on every target:

```jadpo check project=number-maths
type TinSize = Decimal {
    scale: 2
    min: 0.00
}

type TinSurface = Decimal {
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

type RoundDistance = Decimal {
    scale: 1
}

type Course = Decimal {
    scale: 6
}

type NeverRose = Failure {
    kind: FailureKind.invalid_value
}

/// The surface of a round tin, π r².
function tin_area(radius: TinSize) -> TinSurface {
    return TinSurface(decimal.pi(scale: 6, rounding: Rounding.half_even) * radius * radius)
}

/// The side of a square tin the same size, rounded down so the cake
/// fits. A size is never negative, so its square root needs no check.
function square_side(surface: TinSurface) -> TinSize {
    return TinSize(decimal.sqrt(surface, scale: 2, rounding: Rounding.down))
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
function course(northing: RoundDistance, easting: RoundDistance) -> Course {
    return Course(decimal.atan2(easting, northing, scale: 6, rounding: Rounding.half_even))
}

/// Where a van is, east and north of the bakery, `driven` metres on
/// `course`.
function easting_after(driven: RoundDistance, course: Course) -> RoundDistance {
    return RoundDistance(driven * decimal.sin(course, scale: 6, rounding: Rounding.half_even))
}

function northing_after(driven: RoundDistance, course: Course) -> RoundDistance {
    return RoundDistance(driven * decimal.cos(course, scale: 6, rounding: Rounding.half_even))
}
```

`sqrt`'s value and `power`'s exponent must be proved never negative, as
a count is, or the call is `type.argument_negative`. `ln` fails with
`out_of_range` unless its value is proved above 0, by a literal above 0
or a type whose `min` is above 0. `Growth` allows 0, so `hourly_rise`
maps the failure. A result past 38 digits, such as
`decimal.exp(100.0, ...)`, is a defect, as overflow is for `*`. The tests
check these functions, and each mode on the true value:

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
<!-- topic: money-and-payments -->

Use `Money` for an amount of money and `Currency` for its currency. Both
are standard types. An amount is held as a whole number of the
currency's minor units, such as pence or cents, together with its
`Currency`. It is never a decimal of pounds. Each currency's exponent,
the number of digits after its decimal point, comes from ISO 4217: `jpy`
0, `gbp` 2, `bhd` 3.

`Money` is a type built into the language, as `Duration` is, not an
object: an amount has no fields, and `money.minor` and `money.currency`
read its two parts. Like `Int`, it is a representation, so a signature
names a type built on it
([Signatures use named types](02-values-and-types.md#signatures-use-named-types)).
A type built on `Money` takes these settings:

- `currency:`, a `Currency` variant, fixes the currency;
- `rounding:`, a `Rounding` variant, says how `*` and `/` round to a
  minor unit;
- `min:` and `max:` are whole numbers of minor units.

An amount holds at most 2^53 - 1 minor units either side of zero,
JavaScript's largest safe integer, so the `amount` sent on the wire is
always a JSON number a client reads exactly. Making or working out an
amount past that is overflow, a defect, as it is for `+` on `Int`.

`Currency` is an enum of the active ISO 4217 codes, written
`Currency.gbp`, and each release ships the list. It is ISO 4217's list
less the codes with no minor unit (`N.A.`: the precious metals, the SDR,
the bond-market units, `XSU`, `XUA`, `XTS` and `XXX`): 164 codes, each
with its exponent. The codes ISO has withdrawn since 2021 (`ANG`, `BGN`,
`CUC`, `HRK`, `SLL`, `ZWL`) are retired variants: your code can read
them but not write them (`type.variant_retired`). Albania's lek is
`Currency.all_`, because `.all` lists an enum's variants; its code is
still `ALL`.

A `Currency` is spelt by its code, `GBP`, on the wire, in settings, in
OpenAPI and in its column, as every standard enum is, so the database
reads as the API does. `text.format` gives its CLDR display name, such as
"Japanese Yen".

A bakery prices its cakes in pounds, takes orders from abroad in the
customer's currency, and shares a bill between friends:

```jadpo check project=money
type CakePrice = Money {
    currency: Currency.gbp  // fixed: adding another currency doesn't compile
    min: 0  // in minor units: pence
    rounding: Rounding.half_up  // how `*` and `/` round to a penny
}

type Payment = Money {}  // any currency, known only while running

type EnteredAmount = Decimal {
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
function entered(amount: EnteredAmount, currency: Currency) fails AmountTooPrecise -> Payment {
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

Inside `if (money.currency(a) == money.currency(b))`, the checker knows
the two currencies match, so `+` needs no `attempt`. Comparing a
currency with one written out, as in `money.currency(a) ==
Currency.gbp`, proves that currency, so the amount builds a type that
fixes it with no arm for `currency`:

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

## Exchange rates
<!-- topic: exchange-rates -->

An `ExchangeRate` says how much one currency is worth in another at a
moment, so you can convert an amount. It is a standard object with four
fields: `from` and `to`, each a `Currency`; `rate`, a `Decimal`; and
`at`, the `Instant` it was quoted at. You build one with its object
literal and read its fields. One unit of `from` is `rate` of `to`, as a
currency pair is quoted: `GBP/EUR 1.17` means £1 is €1.17.

A rate can be any `Decimal`. It isn't checked to be above 0, because no
rule a field can carry says "greater than" and a rate is a provider's
data. A rate between a currency and itself is allowed.

- **Text.** Its canonical text is the pair, the rate and the instant,
  `GBP/EUR 1.17 2026-10-18T09:00:00Z`: the ISO 4217 codes, base first,
  joined by `/` as a currency pair is written; the rate with the places
  it was given; and the instant in UTC. Codes are read ignoring case. No
  standard writes a rate with its time, so this is the narrowest text
  that round-trips. `text.format` shows that same text, whatever the
  locale, as for a `Uuid`.
- **JSON** is an object of the four fields, each in its own canonical
  text.
- **Storage** is a column per field, `<column>_from` and `<column>_to`
  (each a `char(3)` code), `<column>_rate` (`numeric`) and `<column>_at`
  (`timestamptz(3)`). A rate type's `from:` and `to:` add check
  constraints holding those columns to its currencies
  (`<table>_<column>_from_check` and `<table>_<column>_to_check`), as a
  `Money` type's `currency:` holds its currency column.
- **Shape.** `type.standard_exists`, the error for a declared type with
  a standard type's shape, matches an object of `from`, `to`, `rate` and
  `at`, by their common names.

A shop that takes orders from abroad is given a rate each morning, and
prices a cake in the customer's currency. `money.convert` takes a rate
only from the amount's currency. Where the checker knows both
currencies, a rate the wrong way round doesn't compile. Otherwise the
call fails `currency_mismatch`, and a condition on `rate.from` proves
that away:

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

The checker knows an amount's currency from its type or from how it was
made. It knows a rate's `from` and `to` from its object literal written
out, or from a condition such as `rate.from == Currency.gbp` or
`rate.from == money.currency(price)`; such a condition also settles
`currency_mismatch`, with no `attempt`. Where it knows both, a rate from
another currency is a type error, `type.mismatch`, not a rule of its
own. Elsewhere the call fails `currency_mismatch` under `attempt`. The
result is in the rate's `to` currency where that is known, so building a
type that fixes that currency needs no arm.

Here the condition says the rate is from euros, and the cake is in
pounds:

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
  help: https://jadpo.dev/docs/errors/type.mismatch

check failed: 1 error, 1 file
```

A type built on `ExchangeRate` can fix its currencies with `from:` and
`to:`, as a `Money` type fixes its currency with `currency:`. A
signature then carries them. The right rate needs no `attempt`, the
wrong one doesn't compile wherever it is passed, and a rate the app was
given, or reads from JSON or text, is checked against them where it
becomes one:

- **`from:` and `to:` are a rate type's settings,** each a `Currency`
  variant written out: `type PoundsToEuros = ExchangeRate { from:
  Currency.gbp, to: Currency.eur }`. Either can be used alone, as in
  `type FromPounds = ExchangeRate { from: Currency.gbp }`. On any other
  type they are `type.constraint_base`, and so is `currency:` on a rate
  type.
- **The checker reads the rate's type first,** before a rate written out
  or a condition on `rate.from`. An amount whose type or making fixes the
  rate type's `from` needs no `attempt`. One fixed to another currency is
  `type.mismatch` wherever the rate was passed from, its label naming
  both currencies (`a rate from EUR to GBP`). The result is in the type's
  `to`. An amount whose currency only the running program knows still
  takes `currency_mismatch` under `attempt`.
- **A rate type is built from a value of `ExchangeRate`,** which is its
  representation, as for any type built on an object
  ([Representation types](02-values-and-types.md#representation-types)):
  `PoundsToEuros(rate)`, from an `ExchangeRate` or from another type built
  on one. The value is checked against the rules the type adds. These
  fail as `from` and `to`, rule keys named for their settings, and are
  settled with no arm by the value's own type, a rate written out, or a
  condition on `rate.from` and `rate.to`. An arm for a currency the type
  leaves open is `failure.unknown_mapping`. Braces don't build a rate
  type: `PoundsToEuros { ... }` is `type.not_record`, so a rate written
  out is `PoundsToEuros(ExchangeRate { ... })`.
- **A rate read at the boundary is checked against its type.**
  `json.read` and a request body check the whole rate once its fields
  are read, failing `mismatch` with the rule `from` or `to` in its
  `DecodeProblem`. `text.parse` fails `from` or `to`, beside `malformed`.
  OpenAPI shows each fixed currency as an `enum` of its one code, as it
  does for a `Money` type's currency. The API record writes the type as
  `ExchangeRate { from: GBP, to: EUR }`, so changing either shows as an
  API change.

```jadpo check project=money-rate-types
type CakePrice = Money {
    currency: Currency.gbp
    min: 0
}

type EuroPrice = Money {
    currency: Currency.eur
}

/// The rate a shop is given each morning for pricing in euros.
type PoundsToEuros = ExchangeRate {
    from: Currency.gbp  // fixed: a rate from euros doesn't compile
    to: Currency.eur
}

type QuoteJson = Text {}

type WrongRate = Failure {
    kind: FailureKind.invalid_value
}

type NotARate = Failure {
    kind: FailureKind.invalid_value
}

/// A cake's price in euros: the rate's type fixes both currencies, so
/// the call needs no `attempt` and the result is in euros.
function priced_in_euros(cake: CakePrice, rate: PoundsToEuros) -> EuroPrice {
    return EuroPrice(money.convert(cake, rate))
}

/// The morning's rate, checked once, where it arrives.
function morning_rate(todays_quote: ExchangeRate) fails WrongRate -> PoundsToEuros {
    return attempt PoundsToEuros(todays_quote) {
        from: WrongRate  // a rate from euros, the wrong way round
        to: WrongRate  // a rate to dollars
    }
}

/// A rate a provider sent as JSON.
function sent_rate(sent: QuoteJson) fails NotARate -> PoundsToEuros {
    return attempt json.read(sent, as: PoundsToEuros) {
        malformed: NotARate
        mismatch: NotARate  // other currencies
    }
}
```

```jadpo check project=money-rate-types file=main.test.jadpo
test "a rate type converts from its own currency only" {
    var todays_quote: ExchangeRate = ExchangeRate {
        from: Currency.gbp
        to: Currency.eur
        rate: 1.17
        at: "2026-10-18T09:00:00Z"
    }
    var backwards_quote: ExchangeRate = ExchangeRate {
        from: Currency.eur
        to: Currency.gbp
        rate: 0.85
        at: "2026-10-18T09:00:00Z"
    }
    var rate: PoundsToEuros = attempt morning_rate(todays_quote)
    var cake: CakePrice = CakePrice(money.from_minor(1050, Currency.gbp))
    assert priced_in_euros(cake, rate) == money.from_minor(1228, Currency.eur)
    assert morning_rate(backwards_quote) fails WrongRate
    var sent: PoundsToEuros = attempt sent_rate(
        "{\"from\":\"GBP\",\"to\":\"EUR\",\"rate\":\"1.17\",\"at\":\"2026-10-18T09:00:00Z\"}"
    )
    assert sent == rate
    assert sent_rate(
        "{\"from\":\"EUR\",\"to\":\"GBP\",\"rate\":\"0.85\",\"at\":\"2026-10-18T09:00:00Z\"}"
    ) fails NotARate
}
```

A rate whose type is the wrong way round is a type error in the function
it is passed to, with no condition in sight:

```jadpo check project=money-rate-type-inverted expect=type.mismatch
type CakePrice = Money {
    currency: Currency.gbp
}

type Payment = Money {}

type EurosToPounds = ExchangeRate {
    from: Currency.eur
    to: Currency.gbp
}

/// A cake's price, at a rate whose type is the wrong way round.
function priced_abroad(cake: CakePrice, rate: EurosToPounds) -> Payment {
    return Payment(money.convert(cake, rate))
}
```

```text output
error: `rate` converts from EUR, and `cake` is in GBP
 --> main.jadpo:14:40
   |
14 |     return Payment(money.convert(cake, rate))
   |                                        ^^^^ a rate from EUR to GBP
  reason: `money.convert` applies a rate to an amount in the rate's `from` currency. `cake` is in GBP and `rate` converts from EUR, as their types, how they were made or a condition around the call fix, so the rate is the wrong way round or for another currency.
  next: Pass a rate from GBP, or an amount in EUR (agent)
  rule: type.mismatch
  help: https://jadpo.dev/docs/errors/type.mismatch

check failed: 1 error, 1 file
```

A type built on one that fixes a currency repeats that currency or
leaves it out, because a type built on another can only narrow it
([Constraints](02-values-and-types.md#constraints)). A different
currency, in an amount's `currency:` or a rate's `from:` and `to:`,
leaves no value that could follow both. It is
`type.constraint_contradiction`, as a `scale:` other than the inherited
one is, and its one-step fix removes the setting. A currency the parent
leaves open, such as `to:` on a type built on `FromPounds`, may be
fixed:

```jadpo check project=money-currency-kept expect=type.constraint_contradiction
type CakePrice = Money {
    currency: Currency.gbp
    min: 0
}

/// A cake's price for the shop in Paris.
type ParisPrice = CakePrice {
    currency: Currency.eur
}
```

```text output
error: `currency: Currency.eur` is different from `CakePrice`'s `currency: Currency.gbp`
 --> main.jadpo:8:15
  |
8 |     currency: Currency.eur
  |               ^^^^^^^^^^^^ different from `CakePrice`'s `currency: Currency.gbp`
  reason: `CakePrice` fixes the currency at GBP, and a type built on another only narrows it, so it repeats that currency or leaves it out; with another one no value could follow both, and the type could never be built.
  next: Remove `currency:`, so the type keeps `CakePrice`'s GBP (agent)
  rule: type.constraint_contradiction
  help: https://jadpo.dev/docs/errors/type.constraint_contradiction

check failed: 1 error, 1 file
```

## Money operations
<!-- topic: money-operations -->

The `money` operations make, take apart, add up, share out and convert
amounts of [`Money`](#money-and-payments). Adding, subtracting and
comparing amounts use the ordinary operators; the operations below do
the rest. The "Fails" column names the failure a call can give, which
you map in an `attempt` (each mapping line is an arm,
[Attempt](03-logic.md#attempt)).

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `money.from_minor(amount, currency)` | `Money` of `amount` minor units | | none |
| `money.of(amount, currency)` | `Money` from a `Decimal` in major units | `too_many_places` (more places than the currency has) | none |
| `money.minor(value)` | `Int`, the minor units | | the amount column |
| `money.currency(value)` | `Currency` | | the currency column |
| `money.sum(items, currency:)` | `Money`; `currency:` gives the zero for an empty list | `currency_mismatch` | none |
| `money.allocate(total, ratios)` | `List<Money>` in proportion to `ratios` (whole numbers), adding up to `total` exactly: leftover minor units go one each to the first shares | | none |
| `money.split(total, parts)` | `List<Money>`, `parts` near-equal shares adding up to `total`; `parts` at least 1 | | none |
| `money.mask(card)` | `MaskedCardNumber` of a `PaymentCardNumber`: its brand by its issuer range (`none` for a range no listed scheme issues) and its last four | | none |
| `money.convert(value, rate)` | `Money` in the rate's other currency, rounded half to even to its minor unit; `rate` an `ExchangeRate` the app was given | `currency_mismatch` (the rate is for another currency) | none |
| `money.apply_discount(price, discount)` | `Price`, `discount` taken off its amount and `includes_tax` kept: an amount off, or that percentage of the amount rounded half to even to a minor unit; more off than the price leaves nothing | `currency_mismatch` (an amount off in another currency) | none |

### Money arithmetic

`+`, `-` and comparisons work within one currency. Mixing two currencies
is a compile error where both types fix their currency. Where the
currencies are known only while the program runs, it is the failure
`currency_mismatch`, which you handle in an `attempt`.

You can multiply money by a number or a `Percentage`, and divide it by a
`Decimal`. The result is rounded by the type's `rounding:`, which is
`half_even` unless the type declares another. Dividing money by an `Int`
is refused: use `money.split` instead. The full rules are in
[The standard operator table](#the-standard-operator-table).

### Sharing money out

`money.split` and `money.allocate` never round: every minor unit goes to
some share, so the shares always add up to the total.

- `money.split`'s `parts` is never 0 and never negative. The compiler
  proves it, as it proves a divisor or a count.
- `money.allocate`'s ratios are a list of whole numbers. The list is
  never empty (`type.argument_empty`) and no ratio is negative
  (`type.argument_negative`). Ratios that are all 0 are a defect, as
  dividing by zero would be.
- Leftover minor units go one each to the first shares whose ratio isn't
  0, so a share whose ratio is 0 always gets nothing.
- A negative total, such as a refund, shares out as a payment does, and
  every share is negative.

### Calls that need no arm

In some cases a call can't fail as its "Fails" column says, and its
`attempt` needs no arm for that failure:

- `money.sum` checks every item's currency against `currency:`, which is
  also the total of an empty list. When the items' type fixes that
  currency, the call needs no arm.
- `money.of`'s `too_many_places` needs no arm when the currency is
  written out and the amount is an `Int`, a literal with no more decimal
  places than the currency has, or a type whose `scale` is no more than
  the currency's exponent (its number of decimal places).

### Converting between currencies

`money.convert` applies an `ExchangeRate` your app already holds, so a
rate applied the wrong way round is a type error, not a silent bug.
Jadpo doesn't fetch rates: a rate is outside data with a time and a
source, which a provider package fetches
([Exchange rates](#exchange-rates)).

The result is rounded half to even to the target currency's minor unit,
whatever the amount type's `rounding:` says: £10.50 at 1.17 is €12.285,
so €12.28.

### Money as text

Money's canonical text is the currency code, a space, and the amount in
major units with exactly the currency's number of decimal places:
`GBP 10.50`, `JPY 500`, `BHD 1.250`. `text.parse` reads the code in any
case. It refuses any other number of places, or an amount past
[`Money`'s limit](#money-and-payments), as `malformed`.

Money has no literal, so `Display`, the form test reports and failed
assertions use, writes an amount as the code that makes one:
`money.from_minor(1050, Currency.gbp)`.

### Storing money

A money field is stored in two columns: a `bigint` of minor units named
`<column>_amount`, and a `char(3)` named `<column>_currency` holding the
ISO 4217 code. It uses both even when the type fixes the currency, so
opening the type to other currencies later needs no migration, and a
query can group by currency.

A type that fixes the currency also adds a check constraint,
`<table>_<column>_currency_check`, holding the currency column to that
code. A migration adds or drops it as the type changes.

### Money in JSON

Money is sent in one fixed shape, the amount in integer minor units as
Stripe sends it: `{"amount": 1050, "currency": "GBP"}`. `json.read` and
`json.write` use the same shape.

- The amount is always a JSON number. It is never sent as a string,
  which is how [Values and types](02-values-and-types.md) sends a large
  `Int`.
- JSON past `Money`'s limit is refused as `max`.
- The currency is read in any case and written in capitals.
- OpenAPI describes it by the JSON it sends, as it does every standard
  type: an object of `amount`, with the type's bounds, and `currency`, an
  `enum` of the one code when the type fixes it.

A provider that uses another shape, such as `"10.50"`, is mapped in its
package's `wire:`.

### Showing money to people

`text.format` shows money the way a locale writes it, from CLDR's data:
the symbol or code, the locale's grouping and decimal separators, the
currency's decimal places and where the symbol goes. For example
`£1,234.50`, or `1 234,50 €`. It takes three options, each a built-in
enum:

- `precision:` is a `MoneyPrecision`. The default, `MoneyPrecision.minor`,
  gives the currency's places as CLDR gives them for display.
  `MoneyPrecision.whole` gives `£1,235`, rounding a half away from zero.
- `display:` is a `MoneyDisplay`: `symbol` (the default), `code` or
  `name`.
- `sign:` is a `MoneySign`: `standard` or `accounting`, which shows
  `(£5.00)`.

### Money in a translated message

Inside a [translated message](10-configuration-and-time.md#translations-as-messages),
a money field is written `{price, number, currency}`, one of the
messages' placeholder styles. It shows the amount as `text.format` does
with its default options, in the locale the message is written in. Code
uses `text.format`; messages use the placeholder.

It is the only style for a field built on `Money`, in a message's
wording or an email's subject, and it is written exactly so:

- `{price}` for an amount, or this style on any other field, is
  `translation.placeholder_style`, whose edit writes the right one.
- Any other spelling, such as `{price,number,currency}`, is
  `translation.wording_invalid`.

A failure's `client:` fields can't be money: the client app formats
those itself, and the export has no money format yet.

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

Not built yet: a query that compares or orders by a money amount, a
one-step update that works one out, and a backfill of a new required
money field. Each is reported as `toolchain.not_built_yet`.

## Prices, discounts and rates
<!-- topic: prices-discounts-and-rates -->

These types hold a price, a discount, a price per kilogram or litre, and
the rates a business quotes: tax, basis points and interest. Each is
built from `Money`, `Percentage` or a [quantity](#physical-quantities):

| Type | What it holds | Sent as | Shown as |
|---|---|---|---|
| `Price` | `amount`, a `Money` never below 0, and `includes_tax`, a `Bool`, so a gross price is never taken for a net one | `{"amount": {"amount": 1050, "currency": "GBP"}, "includes_tax": true}` | Its amount, `£10.50` |
| `Discount` | A choice: `amount_off { amount }`, a `Money` never below 0, or `percentage_off { percentage }`, from 0% to 100% | `{"tag": "percentage_off", "percentage": "20%"}` | Its part, `£5.00` or `20%` |
| `UnitPrice` | A choice by the quantity's kind, as the UK's Price Marking Order prices goods: `per_mass`, `per_volume`, `per_length` or `per_area`, each a `price` and the amount it is `per` | `{"tag": "per_mass", "price": {...}, "per": {"value": "1", "unit": "kg"}}` | `£2.50/1 kg` |
| `TaxRate` | A `Percentage` never below 0% | `"20%"` | `20%` |
| `BasisPoints` | A whole number of hundredths of a percent, of any sign | `25`, a JSON number | `25 bp` |
| `InterestRate` | `rate`, a `Percentage` a year that may be below 0%, and `basis`, an `InterestRateBasis`: `aer`, `apr` or `flat` | `{"rate": "5.5%", "basis": "apr"}` | `5.5% APR` |

### Choices: `Discount` and `UnitPrice`

`Discount` and `UnitPrice` are each a choice between variants: an
[enum with data](02-values-and-types.md#enums), the language's tagged
union, where each variant carries its own fields. You build, match and
read one as you would your own enum.
`Discount.percentage_off { percentage: 20% }` builds one, and the
`match` arm `Discount.percentage_off(off) -> off.percentage` reads it.
It is sent in JSON in the same tagged form as your own enums, and shown
by the parts its variant has.

The four records, `Price`, `Discount`, `UnitPrice` and `InterestRate`,
have no single text form, so `text.from` and `text.parse` refuse them, as
they refuse a `PostalAddress`. None is ordered, and `.all` refuses a
choice, as it refuses any enum with data.

### `TaxRate` and the parts' bounds

A `TaxRate` is a `Percentage` that is never below 0%. The standard
library declares it as a type built on `Percentage` with `min: 0%`, as it
declares the types held as text, so building one below 0% breaks `min`.
Its text, JSON, storage and format are `Percentage`'s. It has no maximum,
because some duties are set above 100%.

The records' parts keep bounds the same way: a price's `Money` and an
amount off are never below 0, a percentage off is from 0% to 100%, and a
unit price's amount is never below 0.

### `Price`

A `Price` is an `amount` and `includes_tax`, so a price with tax is
never taken for one without. It is shown as its amount, because CLDR has
no words for tax status; your app writes "inc. VAT" itself.

### `Discount`'s variants

`Discount`'s variants, `amount_off` and `percentage_off`, each hold one
named field, because a variant's data is always named fields.

### Applying a discount

`money.apply_discount(price, discount)` takes a `Price` and gives one,
keeping `includes_tax`. It is an operation in `money` because
[the standard operator table](#the-standard-operator-table) has no row
for a `Discount`.

A percentage's share is rounded half to even to a minor unit before it
comes off, so a receipt's discount line and its total agree. More off
than the price leaves nothing, never less. An amount off in another
currency fails `currency_mismatch`. Which variant a `Discount` holds is
known only while the program runs, so every call needs its `attempt`.

### `UnitPrice`

A `UnitPrice` is a price per amount of something, as a shelf label
shows it. It is a choice by the quantity's kind, `per_mass`,
`per_volume`, `per_length` or `per_area`, the kinds UK law prices goods
by. Each holds a `price` and the amount it is `per` (`1kg`, `100g`).

It is shown as the price over the amount in its kind's base unit,
`£2.50/1 kg`, by CLDR's short "per" pattern. A release adds a kind, such
as energy per kWh, when an application needs one.

### `BasisPoints`

A `BasisPoints` is a whole number of hundredths of a percent, of any
sign, held within JavaScript's safe integers as an amount of money is.
Its text is its digits, `-25` for a negative one. It is sent as a JSON
number, and shown with `bp` in every locale: `25 bp`. There is no
conversion to a `Percentage`.

### `InterestRate`

An `InterestRate` is a `rate` a year, which may be below 0%, and a
`basis`. The basis is an `InterestRateBasis`, a standard enum of `aer`,
`apr` and `flat`. Each value is its own code, and is shown as lenders
write it, in every locale: `AER`, `APR`, `flat`.

None of these types has a shape, the set of fields Jadpo uses to spot an
app type that copies a standard one (`type.standard_exists`). None is
personal data.

A bakery puts offers on its cakes, works out the VAT on a price before tax, and
labels its apples and its loan as the law and the lender write them:

```jadpo check project=prices
type WrongCurrency = Failure {
    kind: FailureKind.invalid_value
}

type ShareOff = Percentage {}

type CakeAmount = Money {}

type FormText = Text {}

type NotARate = Failure {
    kind: FailureKind.invalid_value
}

/// A cake's price with a deal taken off it; an amount off in another
/// currency fails.
function with_deal(price: Price, deal: Discount) fails WrongCurrency -> Price {
    var reduced: Price = attempt money.apply_discount(price, deal) {
        currency_mismatch: WrongCurrency
    }
    return reduced
}

/// A deal as the shop's window shows it.
function deal_label(deal: Discount, customer: Locale) -> PresentationText {
    return text.format(deal, locale: customer)
}

/// The share of the price a deal takes off, where it names one.
function share_off(deal: Discount) -> ShareOff? {
    return match deal {
        Discount.amount_off -> none
        Discount.percentage_off(off) -> ShareOff(off.percentage)
    }
}

/// The VAT on a price before tax at `rate`, rounded half to even to a
/// penny.
function vat_on(untaxed: Price, rate: TaxRate) -> CakeAmount {
    return CakeAmount(untaxed.amount * rate)
}

/// A VAT rate typed into the till's settings.
function vat_rate_from(raw: FormText) fails NotARate -> TaxRate {
    var rate: TaxRate = attempt text.parse(raw, as: TaxRate) {
        malformed: NotARate
        invalid: NotARate
    }
    return rate
}

/// Apples, priced by the kilogram as the shelf label must show them.
function apples() -> UnitPrice {
    return UnitPrice.per_mass {
        price: money.from_minor(250, Currency.gbp)
        per: 1kg
    }
}

/// The rate on the bakery's loan, as the lender quotes it.
function loan_rate() -> InterestRate {
    return InterestRate {
        rate: 6.9%
        basis: InterestRateBasis.apr
    }
}
```

```jadpo check project=prices file=main.test.jadpo
test "a deal comes off a price either way" {
    var cake: Price = Price {
        amount: money.from_minor(1250, Currency.gbp)
        includes_tax: true
    }
    var five_off: Discount = Discount.amount_off {
        amount: money.from_minor(500, Currency.gbp)
    }
    var fifth_off: Discount = Discount.percentage_off {
        percentage: 20%
    }
    var after_five: Price = attempt with_deal(cake, five_off)
    assert after_five.amount == money.from_minor(750, Currency.gbp)
    assert after_five.includes_tax
    var after_fifth: Price = attempt with_deal(cake, fifth_off)
    assert after_fifth.amount == money.from_minor(1000, Currency.gbp)
    var in_euros: Discount = Discount.amount_off {
        amount: money.from_minor(500, Currency.eur)
    }
    assert with_deal(cake, in_euros) fails WrongCurrency
    assert deal_label(five_off, Locale.en_gb) == "£5.00"
    assert deal_label(fifth_off, Locale.en_gb) == "20%"
    assert share_off(fifth_off) == 20%
    assert share_off(five_off) == none
}

test "more off than the price leaves nothing" {
    var bun: Price = Price {
        amount: money.from_minor(300, Currency.gbp)
        includes_tax: true
    }
    var five_off: Discount = Discount.amount_off {
        amount: money.from_minor(500, Currency.gbp)
    }
    var after: Price = attempt with_deal(bun, five_off)
    assert after.amount == money.from_minor(0, Currency.gbp)
}

test "rates are shown as they are quoted" {
    var untaxed: Price = Price {
        amount: money.from_minor(1000, Currency.gbp)
        includes_tax: false
    }
    var vat: TaxRate = attempt vat_rate_from("20%")
    assert vat_on(untaxed, vat) == money.from_minor(200, Currency.gbp)
    assert vat_rate_from("-5%") fails NotARate
    assert vat_rate_from("20") fails NotARate
    assert text.format(apples(), locale: Locale.en_gb) == "£2.50/1 kg"
    assert text.format(loan_rate(), locale: Locale.en_gb) == "6.9% APR"
    var over_base: BasisPoints = 25
    assert text.format(over_base, locale: Locale.en_gb) == "25 bp"
    assert text.from(over_base) == "25"
}
```

A percentage off past 100% doesn't compile, as a literal past any type's
bounds doesn't:

```jadpo check project=discount-past-whole expect=type.invalid_literal
/// Everything off, and more.
function clearance() -> Discount {
    return Discount.percentage_off {
        percentage: 120%
    }
}
```

```text output
error: `120%` isn't a valid `Discount.percentage_off.percentage`
 --> main.jadpo:4:21
  |
4 |         percentage: 120%
  |                     ^^^^ breaks `max: 100%`
  reason: `Discount.percentage_off.percentage` declares `max: 100%`, and the value is above 100%. A literal is checked against every rule while compiling.
  next: Use a value that `Discount.percentage_off.percentage` allows (agent)
  rule: type.invalid_literal
  help: https://jadpo.dev/docs/errors/type.invalid_literal

check failed: 1 error, 1 file
```

### Storing prices and rates

A `TaxRate` is stored in one `numeric` column and a `BasisPoints` in one
`bigint`. The four records are stored a column per part, as an
`ExchangeRate` is. A `Price` uses `amount`, `currency` and
`includes_tax`. A choice such as `Discount` stores its variant's name in
`kind`, then a column for each part its variants hold, `NULL` where the
stored variant doesn't have that part:

```jadpo check project=loan-rates file=app.jadpo hidden
application {}
```

```jadpo check project=loan-rates file=domains/bakery/entities/loan.jadpo migrate=initial
/// A loan the bakery took out, at a rate above the base rate.
entity Loan {
    id: Uuid
    over_base: BasisPoints  // one `bigint` column: 25 for 0.25%
    vat_rate: TaxRate  // one `numeric` column: 0.2 for 20%
}
```

```jadpo check project=loan-rates file=domains/bakery/actions/take_loan.jadpo hidden
use bakery/entities { Loan }

/// Records a loan at `over_base` above the base rate.
action take_loan(over_base: BasisPoints, vat_rate: TaxRate) -> Loan {
    return Loan.create {
        set: {
            over_base: over_base
            vat_rate: vat_rate
        }
    }
}
```

```jadpo check project=loan-rates file=domains/bakery/actions/take_loan.test.jadpo
use bakery/entities { Loan }

test "a loan keeps its rate over base as it was written" {
    var taken: Loan = take_loan(25, 20%)
    var on_file: Loan = attempt Loan.get(taken.id)
    assert on_file.over_base == 25
    assert on_file.vat_rate == 20%
}
```

## Bank accounts
<!-- topic: bank-accounts -->

A `BankAccount` holds the details a payment to a bank account needs. It
is a choice of the three ways an account is written down:

| Variant | What it holds | Sent as |
|---|---|---|
| `uk` | `sort_code`, a `UkSortCode`, and `account_number`, a `UkAccountNumber` | `{"tag": "uk", "sort_code": "089999", "account_number": "66374958"}` |
| `iban` | `iban`, an `Iban` | `{"tag": "iban", "iban": "GB29NWBK60161331926819"}` |
| `other` | `country`, a `Country`; `bank_code`, its bank's code where the country uses one (a US routing number, an Indian IFSC), or `none`; and `account_number`, each up to ISO 20022's lengths in capital letters and digits | `{"tag": "other", "country": "US", "bank_code": "021000021", "account_number": "123456789"}` |

`other` serves every other country, since no standard defines a US or
other national account. The parts of `uk` and `iban` are the standard
payment and identity numbers listed under
[Groups with the baseline only](#groups-with-the-baseline-only), and
keep their rules: a sort code's six digits, an IBAN's mod-97 check and
its country's length.

In `other`, `bank_code` is `none` where the bank is part of the account
number, as in New Zealand. `bank_code` and `account_number` are capital
letters and digits with no spaces or punctuation, as a document's number
is, up to ISO 20022's lengths, 35 and 34.

Jadpo doesn't check any country's own form in `other` yet, as it doesn't
check a document's number for a country without a published one. A
release adds them as they are needed, such as a US bank code as an
`AbaRoutingNumber`. `other` doesn't refuse a country that uses IBANs.

### Showing and storing an account

A `BankAccount` is `financial`
[personal data](15-personal-data.md#marking-personal-fields), as an IBAN
and a UK account number are on their own. `text.format` shows it masked
to the last four characters of its number, as it shows a card number:
`•••• 4958`. That is the last four of the account number for `uk` and
`other`, and of the IBAN for `iban`. The sort code isn't shown, though it
isn't secret, so one form serves every variant.

It has no single text form. It is stored as a choice is, like a
`Discount`: its variant's name in `kind`, then a column for each part,
`sort_code`, `account_number`, `iban`, `country` and `bank_code`. It has
no shape, as no choice has one, so your app's own
`Object { sort_code, account_number }` isn't `type.standard_exists`.

### UK account numbers and the modulus check

A UK account number also has to fit its sort code, by the modulus check
Vocalink publishes for Pay.UK's payment schemes. Jadpo can't ship the
tables that check needs, so a `BankAccount` takes any sort code and
account number that are each well formed. That follows the
specification, which says to presume valid a sort code its tables don't
cover. Neither construction nor `json.read` runs the check. The
[design notes](notes/19-standard-library.md#uk-modulus-checking) say why,
and what is ready for when the tables may ship.

A business pays its suppliers by the account on each invoice, reads a
payee's account from a payments form, and shows each account only by its
last four:

```jadpo check project=bank-accounts
type FormText = Text {}

type NotAnAccount = Failure {
    kind: FailureKind.invalid_value
}

type ByFasterPayments = Bool {}

/// A supplier's UK account, from the sort code and number on its invoice.
function supplier_account(
    printed_code: UkSortCode,
    printed_number: UkAccountNumber
) -> BankAccount {
    return BankAccount.uk {
        sort_code: printed_code
        account_number: printed_number
    }
}

/// A payee's account as the payments form sends it.
function payee_account(sent: FormText) fails NotAnAccount -> BankAccount {
    return attempt json.read(sent, as: BankAccount) {
        malformed: NotAnAccount
        mismatch: NotAnAccount
    }
}

/// Whether a payment to the account goes by Faster Payments, which takes
/// a UK sort code and account number.
function paid_by_faster_payments(account: BankAccount) -> ByFasterPayments {
    return match account {
        BankAccount.uk -> true
        BankAccount.iban -> false
        BankAccount.other -> false
    }
}

/// An account as a payment's confirmation shows it.
function account_label(account: BankAccount, viewer: Locale) -> PresentationText {
    return text.format(account, locale: viewer)
}
```

```jadpo check project=bank-accounts file=main.test.jadpo
test "an account is paid by its kind and shown by its last four" {
    var supplier: BankAccount = supplier_account("089999", "66374958")
    assert paid_by_faster_payments(supplier)
    assert account_label(supplier, Locale.en_gb) == "•••• 4958"
    var abroad: BankAccount = attempt payee_account(
        "{\"tag\":\"iban\",\"iban\":\"DE89370400440532013000\"}"
    )
    assert not paid_by_faster_payments(abroad)
    assert account_label(abroad, Locale.de_de) == "•••• 3000"
    var in_dollars: BankAccount = attempt payee_account(
        "{\"tag\":\"other\",\"country\":\"US\",\"bank_code\":\"021000021\",\"account_number\":\"123456789\"}"
    )
    assert account_label(in_dollars, Locale.en_us) == "•••• 6789"
}

test "an account whose parts break their rules isn't read" {
    assert payee_account(
        "{\"tag\":\"iban\",\"iban\":\"DE89370400440532013001\"}"
    ) fails NotAnAccount
    assert payee_account(
        "{\"tag\":\"uk\",\"sort_code\":\"08-99-99\",\"account_number\":\"66374958\"}"
    ) fails NotAnAccount
    assert payee_account(
        "{\"tag\":\"other\",\"country\":\"US\",\"bank_code\":\"021000021\",\"account_number\":\"1234-5678\"}"
    ) fails NotAnAccount
}
```

A sort code written out is checked while compiling, as any part's
literal is:

```jadpo check project=bank-account-literal expect=type.invalid_literal
/// The account the business's own payments go out from.
function paying_account() -> BankAccount {
    return BankAccount.uk {
        sort_code: "08-99-99"
        account_number: "66374958"
    }
}
```

```text output
error: `"08-99-99"` isn't a valid `BankAccount.uk.sort_code`
 --> main.jadpo:4:20
  |
4 |         sort_code: "08-99-99"
  |                    ^^^^^^^^^^ not a valid `UkSortCode`
  reason: The value isn't a valid `UkSortCode`: its pattern and check digits accept only `UkSortCode`'s canonical text, and a literal is checked while compiling.
  next: Write a valid `UkSortCode` in its canonical text (agent)
  rule: type.invalid_literal
  help: https://jadpo.dev/docs/errors/type.invalid_literal

check failed: 1 error, 1 file
```

## Physical quantities
<!-- topic: physical-quantities -->

A quantity type holds a measured amount, such as a length, a mass, a
temperature or a file's size. Each one (`Length`, `Mass`, `Temperature`
... and `DataSize`, `Bitrate`) holds an exact `Decimal` in its kind's SI
base unit, at a fixed scale, a number of decimal places, for each kind.

You choose a unit only when you make a value or show one. A `Length` is
a `Length` whatever unit built it, so `==` and comparisons work across
units: `1km == 1000metre`. Inside a program a quantity has no unit, so
metres plus centimetres is just `Length + Length`.

A quantity is a type built into the language, as `Money` is. You build a
type on one with `min:` and `max:`, as in `type ShelfLength = Length {
... }`. Like `Int`, a quantity is a representation type: a signature
names a type built on it, while a `var` may be a plain `Length`
([Signatures use named types](02-values-and-types.md#signatures-use-named-types)).
Every kind is ordered, `Temperature` and `SoundLevel` included.

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

### Making a quantity

Making a quantity always names a unit. There are four ways:

- `units.of(value, unit)`, from an `Int` or a `Decimal` and a unit of
  the kind: `units.of(5, LengthUnit.kilometre)`.
- A literal with a unit suffix, such as `5km`, `250g` or `2.5kg`
  ([Suffixed literals](#suffixed-literals)).
- `text.parse("5 km", as: Length)`, through the contract. It fails
  `unknown_unit` for a unit that isn't the kind's, `malformed` for text
  with no number, or with a rule of the type.
- Another value of the same kind, by construction into a named type
  (`RoomWidth(length)`), which checks the type's rules. No conversion is
  needed, since both are already in metres.

A bare number where a quantity is expected is a compile error,
`type.unit_required`, whose fix asks for the unit (`5km`, or
`units.of(5, LengthUnit.kilometre)`). It can never be declined
([Declining a judgement call](01-project-layout-and-modules.md#declining-a-judgement-call)):
a number without its unit is the mistake the type exists to prevent.

A suffix that names no unit, or misses one only by case (`5mw`, between
`mW` and `MW`), is `type.unit_unknown`, which names each unit it was
near.

### Units

Each kind's units are an enum named for the kind, such as `LengthUnit`,
whose variants are the units' names. A `Length` has `millimetre`,
`centimetre`, `metre`, `kilometre`, `inch`, `foot`, `yard`, `mile` and
`nautical_mile`, and so on for each kind. There is one variant for each
suffix in the table under [Suffixed literals](#suffixed-literals), plus
the units that have no suffix: the cubic metre
(`VolumeUnit.cubic_metre`), and a `TemperatureDifference`'s `celsius`,
`fahrenheit` and `kelvin`.

A unit's code is its suffix, which text and JSON use too; the cubic
metre's code is `cubic_metre`. `text.format(value, locale:, unit:)`
shows a quantity in one of these units, or in its kind's base unit when
you leave `unit:` out.

### What the suffixes mean

- `floz` is the US fluid ounce (29.5735295625 ml), since UK labels give
  millilitres.
- A degree of angle is π / 180 radians, to 30 digits.
- `psi` is a pound-force on a square inch, `kcal` the thermochemical
  4,184 J, `t` the tonne and `knot` 1,852 m an hour.

A value is converted exactly where the unit is defined exactly, and
rounded half to even to its kind's scale. A `Mass` is held in kilograms
to 9 places, `numeric(38, 9)`, so `1mcg` is exact, as a medicine's dose
needs. No other kind has a suffix finer than its scale.

### A literal's suffix decides its kind

A duration's suffix (`ms`, `s`, `m`, `h`, `d`) makes a `Duration`, and
any other suffix makes a quantity, matched exactly. A quantity literal
may be negative (`-40degC`); a duration literal may not.

`-` on a quantity value is refused, as it is on any value but a number
([Expressions and operators](03-logic.md#expressions-and-operators)).
Write the quantity first when you multiply: `bracket * 2`, never
`2 * bracket`, as for money.

### Quantities as text

A quantity's canonical text is the number in the base unit with no
trailing zeros, a space, and the base unit's code: `2.5 kg`,
`1000 metre`.

`text.parse` reads a number, an optional space, and any of the kind's
units by its code, matching case exactly. It fails `malformed` for text
with no number or no unit, and `unknown_unit` for another kind's unit or
a word that is no unit.

### Showing quantities to people

`text.format`'s `unit:` is a unit of the value's kind. The number is the
value in that unit, in the locale's digits and separators. It is exact
where the conversion ends within the places the kind's scale gives in
that unit, and otherwise rounded half to even to one place fewer: `1oz`,
held as 0.028349523 kg, is shown as `1 oz`.

For lengths, areas, volumes and masses, ICU4X has CLDR's unit names, and
the locale's short pattern places the unit (`252,62 cm`). For the other
kinds it writes the number, a space and the unit's SI symbol (`21 °C`,
but `90°`), until ICU4X has their names.

`Display` writes a quantity as code makes one, in its base unit:
`2.5262metre`, or `units.of(0.001, VolumeUnit.cubic_metre)` for a kind
whose base unit has no suffix.

### Suffixed literals

Durations already have suffixes, `500ms`, `30s`, `5m`, `2h` and `30d`
([Durations](10-configuration-and-time.md#durations)). `5m` is five
minutes, so you write metres as `5metre`, or with `units.of`. The type a
literal is expected to have never changes what its suffix means, because
a reviewer can't see that type at the literal.

Suffixes are case-sensitive, as SI writes them, and the longest suffix
wins (`dB` before `d`, `mmHg` before `mm`). A near-miss by case (`5mw`,
between milliwatts and megawatts) is an error naming both. No suffix
clashes with a duration's or with another unit's; the last column says
what each kind's suffixes avoid:

| Kind | Suffixes | Clash avoided |
|---|---|---|
| `Length` | `mm`, `cm`, `metre`, `km`, `inch`, `ft`, `yd`, `mi`, `nmi` | `m` is minutes; `in` is a keyword (`for x in`) |
| `Mass` | `mcg`, `mg`, `g`, `kg`, `t`, `oz`, `lb` | `µg` isn't typeable everywhere |
| `Volume` | `ml`, `litre`, `floz`, `ukgal`, `usgal` | `l` reads as `1`; `gal` alone is ambiguous |
| `Area` | `sqmm`, `sqm`, `sqkm`, `ha`, `sqft`, `acre` | `m2` puts a digit in a suffix |
| `Angle` | `deg`, `rad` | `d` is days; longest wins |
| `Temperature` | `degC`, `degF`, `kelvin` | `K` reads as thousands ("5K") |
| `Speed` | `mps`, `kmh`, `mph`, `knot` | `ms` is milliseconds |
| `Pressure` | `Pa`, `hPa`, `kPa`, `bar`, `mbar`, `psi`, `mmHg` | `h` is hours; longest wins (`mmHg` before `mm`) |
| `Energy` | `J`, `kJ`, `Wh`, `kWh`, `kcal` | `cal` refused: food labels mean `kcal` |
| `Power` | `mW`, `W`, `kW`, `MW` | `mW` and `MW` differ only by case |
| `Frequency` | `Hz`, `kHz`, `MHz`, `GHz`, `bpm` | `bps` is bits a second |
| `Voltage`, `Current` | `mV`, `V`, `kV`; `mA`, `A` | |
| `Illuminance`, `SoundLevel` | `lx`; `dB` | `d` is days; longest wins |
| `Concentration` | `mgL`, `gL` | |
| `DataSize` | `B`, `kB`, `MB`, `GB`, `TB`, `KiB`, `MiB`, `GiB`, `TiB` | `b` (bits) refused, so bytes and bits can't be confused |
| `Bitrate` | `bps`, `kbps`, `Mbps`, `Gbps` | |

`TemperatureDifference` has no suffix of its own: you get one by
subtracting two temperatures, or from `units.of`. The clinical units,
`mmHg` and `bpm` among them, are under
[Groups with the baseline only](#groups-with-the-baseline-only).

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

type MilesWalked = Decimal {
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
function in_miles(stroll: Plank) -> MilesWalked {
    return MilesWalked(units.in(stroll, LengthUnit.mile, scale: 2, rounding: Rounding.half_even))
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

### Quantity arithmetic

`+` and `-` work within one kind, and `*` and `/` by a plain number keep
the kind. One kind divided by the same kind gives a plain `Decimal`, as
`/` does for a named type
([Expressions and operators](03-logic.md#expressions-and-operators)).

Within a kind, `+`, `-` and `/` follow the same named-type rule: the two
sides are one named type, or a plain quantity an operator or a `units`
operation made. A divisor's type must exclude zero; for a duration, its
type's `min:` counts.

These products and quotients are defined, and any other mix is a compile
error naming the two kinds:

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

- **Rounding.** A product is exact, and a quotient is worked out exactly
  where it ends. Either is then rounded half to even to the result
  kind's scale, as constructing into a type with a fixed `scale` rounds
  ([Creating values](02-values-and-types.md#creating-values)). For
  another rounding, use `units.in` with `rounding:`.
- **`Temperature` doesn't add to itself:** 20 °C plus 20 °C isn't 40 °C.
  It adds only a `TemperatureDifference`. `SoundLevel` is logarithmic, so
  it compares but has no `+` or `-`.
- **`Duration`** is still the type [Durations](10-configuration-and-time.md#durations)
  describes. It takes part in the table above without becoming a
  quantity.

### Storing quantities

A quantity is stored in one `numeric` column, in the base unit at the
kind's scale from the table above: `numeric(38, 6)` for a `Length`. The
column's comment names its kind and SI unit as the units table writes
it, `Length in metres (m)`. A migration sets the comment with the column
and changes it with the type. A query compares (`<`, `==`) and orders by
a quantity field as it does by a number.

### Quantities in JSON

A quantity is sent with its unit named: `{ "value": "2.5", "unit": "kg" }`.
A bare number in the base unit wouldn't do, because a client can't tell
it from a number in another unit. The number is decimal text, as every
`Decimal` is sent ([Values and types](02-values-and-types.md)).

Jadpo writes the base unit and reads any of the kind's units. A JSON
number for `value` fails as `type`, and another kind's unit as
`unknown_unit`. `json.read` and `json.write` use the same shape, and
OpenAPI lists the kind's units as the unit's `enum`.

### `units` operations

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `units.of(value, unit)` | The quantity of `unit`'s kind: `value`, an `Int` or `Decimal`, converted exactly to the base unit and rounded half to even to the kind's scale | overflow is a defect | none |
| `units.in(value, unit, scale:, rounding:)` | `Decimal`, the quantity in `unit`, rounded | | none |
| `units.sum(items)` | The total of one kind; zero for none | overflow is a defect | none |

`units.in` needs both `scale:` and `rounding:`, as `decimal.round` does.
`units.sum` refuses a list of temperatures or sound levels
(`type.arithmetic_operand`), which don't add. A list written out takes
its first item's kind.

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

`Percentage`, `Ratio`, `Fraction` and the `%` literal are under
[The standard operator table](#the-standard-operator-table).

## Time and calendar additions
<!-- topic: time-and-calendar-additions -->

These types add to the [time types](10-configuration-and-time.md#time-types):
a time of day, a month, a week, a range of dates, an amount of calendar
time, and the text forms of recurrences, opening hours and calendar
events. The 21 `temporal` operations of the time types still apply;
three of them have SQL forms, so an aggregate query can group by time
buckets. The types here add three operations:

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `temporal.year(value)`, `temporal.month(value)`, `temporal.day(value)` | `Int`, of a date or a local time's date ([Zones and local times](10-configuration-and-time.md#zones-and-local-times)) | | of a date: `extract`, as `bigint`, with PostgreSQL's 1 BC counted as the year 0; none of a local time, whose zone PostgreSQL's time zone database may not see as Jadpo's does |
| `temporal.occurrences(rule, from:, count:)` | `List<ZonedTime>`, the next `count` occurrences of a `RecurrenceRule` (RFC 5545) at or after `from`, an `Instant`, in the rule's zone; from a `CalendarDate`, `List<CalendarDate>`, the days they fall on, which a rule on dates gives; `count` 1 to 1000 | | none |
| `temporal.open_at(hours, at)` | `Bool`: an `OpeningHours` is open at the `Instant`, in its zone, across midnight and its exceptions | | none |
| `temporal.period_between(start, end)` | `CalendarPeriod` in whole years, months and days from one `CalendarDate` to another, so adding it to `start` gives `end`; negative parts when `end` is earlier | | the months between the two months, one fewer or more by the day of the month, stepped once toward zero where adding them clamps past the end, and the days left; not `age`, which borrows days from the start's month |

### `CalendarPeriod`

A `CalendarPeriod` is an amount of calendar time: whole years, months and
days, written `P1Y2M10D` in ISO 8601. It is neither a `Duration`, which
is exact time, nor a `DateRange`, which is a span between two dates. It
has no order, because one month and 30 days don't compare, and `==`
compares it part by part.

It is stored as its three parts in `integer` columns, not as an
`interval`. PostgreSQL keeps an interval as months and days, so `P12M`
would come back as `P1Y`.

`CalendarDate + CalendarPeriod` applies years, then months, then days
([The standard operator table](#the-standard-operator-table)). A day
past the month's end becomes that month's last day, so 31 January plus
one month is 28 or 29 February. That is `temporal.add_months` with
`InvalidDay.last_valid_day`
([Zones and local times](10-configuration-and-time.md#zones-and-local-times)).
An app that must refuse such a date calls `add_months` with
`InvalidDay.reject` instead.

A `ZonedTime` takes no `CalendarPeriod` by operator, because it would
also need a `dst:` choice. Use `temporal.add_months` and its siblings.

### Moving a date by a period

Moving a date by a period keeps the date's own named type, as
`Instant + Duration` does
([Arithmetic and comparison](10-configuration-and-time.md#arithmetic-and-comparison)).
`date - period` adds the period with every part negated, so 31 March
less a month is 28 February.

The date comes first. `Int * period`, a `Decimal` factor, a `ZonedTime`
and `<` on periods are refused, as `type.arithmetic_operand` or
`type.ordering_operand`. Moving past the years a date can hold is the
fault `time_out_of_range`, and a part past 64 bits is an overflow, which
is a defect.

### The period between two dates

`temporal.period_between(start, end)` counts a month once the end's day
of the month reaches the start's, as PostgreSQL's `age` and `java.time`
do: 30 January to 28 February is `P29D`. It takes one month fewer where
clamping to a month's last day would pass the end, and gives the rest as
days. So `start + period` is `end` for every pair, and every part has
one sign.

Going backwards it counts from the end, so it isn't always the forward
period negated: 25 October to 29 March is `P-6M-27D`, and forward it is
`P6M26D`.

Its SQL form doesn't use `age`, which borrows days from the start's
month and adds back past the end: 29 January to 1 March 2028 is `age`'s
1 month 3 days, and here `P1M1D`.

### The calendar types

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
| `RecurrenceRule` | held as its text (RFC 5545): the first occurrence as a local time in its zone, or a date alone, a line feed, the rule | `DTSTART;TZID=Europe/London:20261026T093000` and `RRULE:FREQ=WEEKLY;BYDAY=MO`, on two lines; on dates, `DTSTART;VALUE=DATE:20240229` and `RRULE:FREQ=YEARLY` | as written |
| `OpeningHours` | held as its text (OpenStreetMap's `opening_hours`) and its zone | `Mo-Fr 09:00-17:30; Sa 10:00-16:00 [Europe/London]` | as written |
| `CalendarEvent` | `summary`; an all-day event's `first_day` and `last_day`, both included, or a timed one's `start` and `end` (`LocalDateTime`) and `zone`; `location`, `description` and `recurrence` (`RecurrenceRule`, on dates for an all-day event), each may be `none` | an RFC 5545 `VEVENT`, an all-day `DTEND` the day after the last; JSON its parts | as written |

### Calendar types are records of their parts

The types in the table from `LocalTime` to `CalendarPeriod`, except the
enum `Month`, are records of their parts, as `InstantRange` is, so code
reads `at.hour` and `week.year`. Each field is either a built-in whole
number with a range, or another standard type (`LocalDateTime`'s
`time`). The ranges are `LocalTime`'s `hour` from 0 to 23, a year from 0
to 9999 as a `CalendarDate`'s is, `Quarter`'s `quarter` from 1 to 4, and
`UtcOffset`'s `minutes` within 18 hours.

An object literal proves each range as it proves any object's field
rules ([Objects](02-values-and-types.md#objects)): a literal out of range
is `type.invalid_literal`, and a value not proved in range is
`type.field_unproved`. `LocalTime.hour(raw)` builds a field's value with
its `min` and `max`. An `IsoWeek`'s week 53 in a year with 52 weeks
breaks its rule across fields, `week_in_year`, in text too. `Year` is a
record of one field, `year`.

### Calendar types as text

Canonical text is ISO 8601's, to the second and with nothing optional,
and only the form written is read:

- `09:30:00` for a `LocalTime`, never `09:30`, `24:00:00` or a leap
  second.
- `2026-10-25T09:30:00` for a `LocalDateTime`, with no offset.
- `2026-10` for a `YearMonth`, and `2026`, in four digits, for a `Year`.
- `2026-W43` for an `IsoWeek`, with week 53 only in a year that has one.
- `+05:30` for a `UtcOffset`. UTC is `+00:00`, never `Z` or `-00:00`.
- `2026-10-01/2026-10-31` for a `DateRange`.
- `2026-Q4`, upper case, for a `Quarter`, which has no ISO 8601 form.
- `october` for a `Month`: its code is its own name, as `Weekday`'s is.

A `CalendarPeriod` writes each part that isn't zero, in the order years,
months, days, with its own sign when negative, as PostgreSQL writes an
interval (`P1Y-2M3D`). A period of nothing is `P0D`. Weeks (`P1W`), a
`+` and a written zero part are refused, so a period has one text.

### Calendar types in JSON and storage

Each type's JSON is its canonical text, as a `Duration`'s is, except a
`DateRange`, which is an object of its two dates, as an `InstantRange`
is.

Each is stored natively where PostgreSQL has the type: a `LocalTime` in
`time`, a `LocalDateTime` in `timestamp` without a zone, a `Year` and a
`UtcOffset` (its minutes) in a `smallint`, and a `DateRange` in
`daterange`. A `YearMonth`, a `Quarter` and an `IsoWeek` are stored in a
`smallint` per part, a `CalendarPeriod` in its three `integer` columns,
and a `Month` by its code.

### Which calendar types are ordered

`LocalTime`, `LocalDateTime`, `YearMonth`, `Year`, `Quarter` and
`IsoWeek` are ordered, part by part in declared order, which is the
canonical order. `<` and `sort_by` take them. `UtcOffset`, `Month`,
`DateRange` and `CalendarPeriod` aren't ordered; `Month` is an enum, and
`Weekday` isn't ordered either.

### Showing calendar types to people

`LocalTime` and `LocalDateTime` take `format:`, as the time types do,
and only a format of the parts they have. A date in a `LocalTime`'s
format, or a zone in either's, is
`type.temporal_format_part_missing` ([Errors](#errors)), and `zone:` is
`type.temporal_option_unused`. A `DateRange` shows each date by a date
format, and a clock time in it is `type.temporal_format_requires_time`.

The others use CLDR's patterns:

- `Month` is CLDR's stand-alone month name: "October".
- `YearMonth` and `Year` use CLDR's pattern for the parts they have:
  "October 2026".
- `UtcOffset` is CLDR's long localized GMT format: "GMT+05:30",
  "GMT+00:00".
- `CalendarPeriod` is CLDR's long unit names joined as a list of units,
  as a `Duration` is: "1 year, 2 months, 10 days", "0 days", each
  negative part with its sign.
- `Quarter` and `IsoWeek` are shown as their canonical text, because
  ICU4X has no quarter or week-of-year patterns yet.

### `DateRange`

A `DateRange` includes its end in the `range` operations
([Ranges](#ranges)): a range whose start is its end holds that day, and
two ranges that meet on a day overlap on it. Its fields take values
built on `CalendarDate` as an `InstantRange`'s take instants, so a named
date type is a mismatch there.

Its shape, the fields that mark your own type as a copy of it for
`type.standard_exists`, is a first day (`start`, `starts_on` or
`start_date`) and a last day (`end`, `ends_on` or `end_date`), each
built on `CalendarDate`.

A rental runs for a calendar period from its first day. How long someone
has lived somewhere is a period between two days, whatever the clocks
did in between:

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

## Recurrences and opening hours
<!-- topic: recurrences-and-opening-hours -->

A `RecurrenceRule` says when something repeats, such as a class every
Monday at 09:30. An `OpeningHours` says when a place is open, such as
`Mo-Fr 09:00-17:30`. Calendars and maps exchange and store each as one
text, so each is held as that text, and its parts (`BYDAY`, a rule's
spans) aren't fields code reads.

Each is a type built on `Text` that the standard library declares, as
the payment numbers are. A value is its standard text, its JSON a
string, and its storage one `text` column. Its validator checks
construction and literals, and text that breaks it breaks `format`. Your
app's own type can be built on one.

`==` compares the text, so two rules written differently are different
values even when they give the same occurrences. Neither type is
ordered, and `text.format` shows each as written, because CLDR has words
for neither.

### A recurrence's text

A recurrence is two lines joined by a line feed, the form `rrule.js` and
`python-dateutil` write.

The first line is the start. For a timed rule it is `DTSTART;TZID=`, an
IANA zone, `:` and the first occurrence's local date and time
(`20261026T093000`). For a rule on dates it is `DTSTART;VALUE=DATE:` and
a date, as RFC 5545 writes a date-only start (3.3.10) and an all-day
`VEVENT` writes its `DTSTART`. The type's `pattern` takes either.

The second line is `RRULE:` and the rule in RFC 5545's grammar, its
parts in any order, each once.

These aren't read: a start in UTC or with no zone (floating), a carriage
return, a folded line, `RDATE` and `EXDATE`. The rule itself has these
limits:

- `FREQ` is daily, weekly, monthly or yearly. `SECONDLY`, `MINUTELY` and
  `HOURLY` are refused, because a rule within a day is where calendars
  disagree most across a clock change.
- `UNTIL` is included. It is a UTC time with a zoned start, and a date
  with a date start, as RFC 5545 requires; the other kind is refused.
- `COUNT` and `UNTIL` aren't both given.
- A part RFC 5545's table rules out for the frequency is refused:
  `BYMONTHDAY` on a weekly rule, `BYWEEKNO` on any but a yearly one, and
  an ordinal on `BYDAY` on any but a monthly or yearly one.
- `BYSETPOS` with no other `BY` part is refused.
- `BYHOUR`, `BYMINUTE` and `BYSECOND` with a date start are refused, as
  RFC 5545 forbids them.
- **The start must be the rule's first occurrence.** RFC 5545 leaves any
  other start undefined, and calendars disagree about it, so refusing it
  keeps every reader of the text agreeing.

### How a rule expands

A rule expands as `python-dateutil` expands one, in local time. Each
period (a year, a month, a week from `WKST`, a day) gives the days its
`BY` parts pick, with RFC 5545's defaults from the start: a yearly rule
takes the start's month and day, a monthly one its day, a weekly one its
weekday, and every rule its clock time.

An ordinal on `BYDAY` counts within the month for a monthly rule or one
with `BYMONTH`, and within the year otherwise. `BYSETPOS` picks from the
period's set.

A day a month doesn't have (the 31st of a 30-day month, 29 February) is
never made, so it is skipped and not counted, as the RFC says: a yearly
rule from 29 February 2024 gives 2028, 2032 and so on.

### Recurrences and daylight saving

Daylight saving follows RFC 5545 (3.3.5 and 3.8.5.3): an occurrence is
its local time in the rule's zone. One the clocks skip is left out and
not counted, and one they show twice is the first of the two.

A start that falls in a gap takes the offset before the gap, an hour on
(`Dst.compatible`), and is still the first occurrence. A rule on dates
has no zone, and its occurrences are days.

### `temporal.occurrences`

`temporal.occurrences(rule, from:, count:)` gives the next `count`
occurrences at or after `from:`, or fewer when the rule ends first.
`COUNT` counts from the rule's start, not from `from:`.

The rule's form is in its text, which the checker can't see in a value,
so the result's type comes from `from:`:

- An `Instant` gives `List<ZonedTime>`, in the rule's zone.
- A `CalendarDate` gives `List<CalendarDate>`, the days of the next
  `count` occurrences on or after it. A rule on dates gives its own
  days. A timed rule gives the local date of each occurrence in its
  zone, one for each, from the first instant of `from` there.

A rule on dates asked for instants has no zone to place them in, so it
is a defect fault, never a guessed zone or an empty list. Code that may
hold either form asks for days. Text written out for `from:` is an
`Instant` or a date by how it is written.

`count` is proved from 1 to 1000 while compiling, as a count's sign is:
a literal within them, or a value whose type's `min` and `max` are.
Anything else is `type.mismatch`, as a `scale:` outside 0 to 18 is.
Without `COUNT`, the expansion starts near `from` rather than at the
rule's start.

### Opening hours' text

Opening hours are written in the part of OpenStreetMap's
`opening_hours` syntax that every reader agrees on, then a space and the
zone in brackets, as RFC 9557 writes one:
`Mo-Fr 09:00-17:30; Sa 10:00-16:00 [Europe/London]`.

- Rules are joined by `; `. Each is weekdays (`Mo-Fr`, `Sa,Su`, or
  `Fr-Mo` across the week's end) or one date (`2026 Dec 25`), a space,
  and `HH:MM-HH:MM` spans joined by commas, or `off`.
- An end is up to `24:00`. An end at or before its start runs past
  midnight.
- A later rule replaces an earlier one on the days both select, as in
  OpenStreetMap.
- A night past midnight belongs to the day it opens, so a day marked
  `off` doesn't cut short the night before.
- A day no rule selects is closed.

These aren't read: `closed`, `24/7`, public holidays (`PH`), months,
weeks, ranges of dates, sunrise and sunset, comments, and hours past
`24:00`.

### `temporal.open_at`

`temporal.open_at(hours, at)` reads the clock on the wall in the hours'
zone. A span holds the minute it opens, not the minute it closes. On
the night the clocks go back, a bar open to 02:00 is open both times
01:30 comes round, and on the night they go forward the skipped hour
never comes.

A class repeats every Monday at 09:30 in London, whatever the clocks do,
and a bar's Friday and Saturday nights run past midnight. The class
keeps its local time; a time the clocks skip is left out and not
counted, and of a time they show twice the first is taken. The bar's
hours are read by the clock on the wall:

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

## Calendar events
<!-- topic: calendar-events -->

A `CalendarEvent` is an entry in a calendar: a holiday, a booking, a
shift. It is either all-day or timed.

An all-day event holds its first and last days, both included, and no
zone. A holiday on 25 December is on 25 December wherever it is read, so
it never moves a day for a reader in Kiritimati, fourteen hours ahead of
UTC, or in Pago Pago, eleven hours behind.

A timed event holds its local start and end and the zone they are in,
as a recurrence does, so a shift that starts at 22:00 keeps starting at
22:00 when the clocks change. `temporal.resolve` turns its start into an
instant, with `dst:` choosing on the nights the clocks change.

An all-day event repeats on its days, with a rule that starts on a date
and has no zone. A birthday on 29 February comes round only in leap
years, as RFC 5545 skips a day a year doesn't have.

Its text is RFC 5545's `VEVENT`, where an all-day event's end is the day
after its last, and its JSON is its parts:

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
        invalid: DiaryUnreadable
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
                ReaderDay(first) <= reader_local_day and reader_local_day <= ReaderDay(last)
            )
            none -> CoversReaderDay(false)
        }
        none -> CoversReaderDay(false)
    }
}

/// When a timed event starts: its local start in its zone, the earlier of
/// the two on the night the clocks go back and an hour on in the hour they
/// skip; none for an all-day event.
function event_start(event: CalendarEvent) -> SeenAt? {
    return match event.start {
        some(start) -> match event.zone {
            some(zone) -> SeenAt(
                temporal.instant(temporal.resolve(start, zone: zone, dst: Dst.compatible))
            )
            none -> none
        }
        none -> none
    }
}

type StartUnclear = Failure {
    kind: FailureKind.invalid_value
}

/// Where a local time lands in `zone` under each `dst:` choice: the earlier
/// instant, the later, and the compatible one, earlier of two and later in
/// a gap.
function landings(at: LocalDateTime, zone: Zone) -> List<SeenAt> {
    return [
        SeenAt(temporal.instant(temporal.resolve(at, zone: zone, dst: Dst.earlier))),
        SeenAt(temporal.instant(temporal.resolve(at, zone: zone, dst: Dst.later))),
        SeenAt(temporal.instant(temporal.resolve(at, zone: zone, dst: Dst.compatible)))
    ]
}

/// Where a local time lands in `zone` when it must be clear: one the
/// clocks skip or show twice fails.
function clear_landing(at: LocalDateTime, zone: Zone) fails StartUnclear -> SeenAt {
    var landed: ZonedTime = attempt temporal.resolve(at, zone: zone) {
        dst: StartUnclear
    }
    return SeenAt(temporal.instant(landed))
}

/// The days the next `count` of a repeating event fall on, from `from`; an
/// all-day event's are its own days, whichever zone they are read in.
function next_days(event: CalendarEvent, from: ReaderDay, count: ShiftCount) -> List<ReaderDay> {
    return match event.recurrence {
        some(rule) -> list.map(
            temporal.occurrences(rule, from: from, count: count),
            day -> ReaderDay(day)
        )
        none -> []
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
    assert event_start(night_shift) == SeenAt("2026-10-24T21:00:00Z")
}

test "a start resolves across both clock changes as `dst:` chooses" {
    // London's clocks go forward at 01:00 on 29 March 2026, so 01:30 never
    // comes, and back at 02:00 on 25 October, so it comes twice.
    var london_gap: LocalDateTime = LocalDateTime {
        date: "2026-03-29"
        time: LocalTime {
            hour: 1
            minute: 30
            second: 0
        }
    }
    var london_overlap: LocalDateTime = LocalDateTime {
        date: "2026-10-25"
        time: LocalTime {
            hour: 1
            minute: 30
            second: 0
        }
    }
    assert landings(london_gap, Zone.europe_london) == [
        SeenAt("2026-03-29T00:30:00Z"),
        SeenAt("2026-03-29T01:30:00Z"),
        SeenAt("2026-03-29T01:30:00Z")
    ]
    assert landings(london_overlap, Zone.europe_london) == [
        SeenAt("2026-10-25T00:30:00Z"),
        SeenAt("2026-10-25T01:30:00Z"),
        SeenAt("2026-10-25T00:30:00Z")
    ]
    assert clear_landing(london_gap, Zone.europe_london) fails StartUnclear
    assert clear_landing(london_overlap, Zone.europe_london) fails StartUnclear
    // New York's go forward at 02:00 on 8 March and back at 02:00 on 1
    // November.
    var new_york_gap: LocalDateTime = LocalDateTime {
        date: "2026-03-08"
        time: LocalTime {
            hour: 2
            minute: 30
            second: 0
        }
    }
    var new_york_overlap: LocalDateTime = LocalDateTime {
        date: "2026-11-01"
        time: LocalTime {
            hour: 1
            minute: 30
            second: 0
        }
    }
    assert landings(new_york_gap, Zone.america_new_york) == [
        SeenAt("2026-03-08T06:30:00Z"),
        SeenAt("2026-03-08T07:30:00Z"),
        SeenAt("2026-03-08T07:30:00Z")
    ]
    assert landings(new_york_overlap, Zone.america_new_york) == [
        SeenAt("2026-11-01T05:30:00Z"),
        SeenAt("2026-11-01T06:30:00Z"),
        SeenAt("2026-11-01T05:30:00Z")
    ]
    assert clear_landing(new_york_gap, Zone.america_new_york) fails StartUnclear
    assert clear_landing(new_york_overlap, Zone.america_new_york) fails StartUnclear
    var shift_start: LocalDateTime = LocalDateTime {
        date: "2026-10-24"
        time: LocalTime {
            hour: 22
            minute: 0
            second: 0
        }
    }
    var shift_landed: SeenAt = attempt clear_landing(shift_start, Zone.europe_london)
    assert shift_landed == SeenAt("2026-10-24T21:00:00Z")
}

test "a birthday on 29 February comes round in leap years" {
    var birthday: CalendarEvent = CalendarEvent {
        summary: "Sam's birthday"
        first_day: "2024-02-29"
        last_day: "2024-02-29"
        start: none
        end: none
        zone: none
        location: none
        description: none
        recurrence: "DTSTART;VALUE=DATE:20240229\nRRULE:FREQ=YEARLY"
    }
    assert next_days(birthday, "2024-03-01", 2) == [
        ReaderDay("2028-02-29"),
        ReaderDay("2032-02-29")
    ]
    assert event_start(birthday) == none
    var birthday_vevent: DiaryText = DiaryText(text.from(birthday))
    assert birthday_vevent == "BEGIN:VEVENT\nDTSTART;VALUE=DATE:20240229\nDTEND;VALUE=DATE:20240301\nSUMMARY:Sam's birthday\nRRULE:FREQ=YEARLY\nEND:VEVENT"
    var birthday_from_text: CalendarEvent = attempt diary_entry_from_text(birthday_vevent)
    assert birthday_from_text == birthday
    var birthday_from_json: CalendarEvent = attempt diary_entry_from_json(
        DiaryJson(json.write(birthday))
    )
    assert birthday_from_json == birthday
}
```

### An event's parts

A `CalendarEvent` has these parts, in order:

- `summary`, one line of text.
- For an all-day event, `first_day` and `last_day`, each a
  `CalendarDate`. The last day is included, as a `DateRange`'s end is.
- For a timed event, `start` and `end`, each a `LocalDateTime`, and its
  `zone`.
- `location`, one line of text.
- `description`, text over several lines. It holds no control character
  but a tab or a line feed, because RFC 5545 text can't hold one.
- `recurrence`, a `RecurrenceRule`.

Every part but `summary` may be `none`. Its rules across parts say which
go together:

- `days_or_times`: an all-day event has both days and none of `start`,
  `end` and `zone`; a timed one has all three and neither day. A timed
  event's start and end are in its one zone: a flight between zones is
  two local times in one of them. A floating time (no zone) and a time
  in UTC aren't read.
- `last_day_not_before_first`: an all-day event's last day is on or
  after its first.
- `end_after_start`: a timed event's end is later than its start, as RFC
  5545 requires.
- `recurrence_from_start`: an all-day event's recurrence is a rule on
  dates whose start is its `first_day`, and a timed event's is a zoned
  rule from its `start` in its `zone`, because a `VEVENT` writes one
  `DTSTART` for both.

A literal that breaks one of these rules is `type.invalid_literal`. In a
literal, a part written `none` is known to be absent, and one written
out (text, a variant, an object) is known to be given. A part built from
a value is proved or attempted by the rule, and text or JSON that breaks
a rule breaks it by name.

### No `UID` or `DTSTAMP`

RFC 5545 requires a `UID` and a `DTSTAMP` of a `VEVENT` in a calendar
file, but they belong to that file's copy of the event, not to the
event: an identity, and the time a calendar store last revised it. A
`CalendarEvent` is the event itself, so two with the same parts are
equal. Your app keeps its own identity and times beside one, such as an
entity's `id`, and the writer of calendar files writes them from those.
A text with either is `malformed`.

### An event as text

An event's text is the `VEVENT`'s lines joined by line feeds, as a
`RecurrenceRule`'s are, in one order: `BEGIN:VEVENT`, `DTSTART`,
`DTEND`, `SUMMARY`, then `LOCATION`, `DESCRIPTION` and `RRULE` where it
has them, and `END:VEVENT`.

- An all-day event's start and end are dates, `DTSTART;VALUE=DATE:`, and
  its `DTEND` is the day after its last, as RFC 5545 writes it.
- A timed event's are local times with the zone,
  `DTSTART;TZID=Europe/London:`, as a recurrence's start is.
- Its `RRULE` is the recurrence's rule line. It is read with the event's
  `DTSTART` line as the rule's start, and written back the same way.
- Text values escape `\`, `;`, `,` and a line feed as RFC 5545 3.3.11
  does (`\n`, never `\N`).

Only that form is read: no carriage return, folded line, other property
or parameter, or `DURATION` for an end. RFC 5545 asks a calendar file
for carriage returns and folded lines, which the writer of calendar
files adds. An event whose last day is 31 December 9999 has no day after
it to write, so it is the fault `time_out_of_range`.

### An event in JSON

An event's JSON is its parts, each in its own canonical text and `null`
where it is `none`: `{"summary": "Half-term walk", "first_day":
"2026-10-24", "last_day": "2026-10-26", "start": null, ...}`. A local
time is `"2026-10-24T22:00:00"`, the zone `"Europe/London"`, and the
recurrence its two lines.

It is shown to people as its canonical text, as a recurrence is, because
CLDR has no pattern for an event. Your app shows the parts it wants,
each by its own format, such as `event.first_day` by a date format.

It isn't ordered, and `==` compares it part by part. It has no shape:
`type.standard_exists` matches no app's event type, since
`{ title, start, end }` is too common to be distinctive. It isn't
personal data of itself.

### Local times and `temporal.resolve`

`temporal.resolve` places a local date and time in a zone. Its first
argument is a date, given its clock time with `at:`, or a
`LocalDateTime`, which carries its own and takes no `at:`. That first
parameter is still named `date`. `at:` is a clock time written out, or a
`LocalTime`.

Either way the local time is placed in `zone:` under `dst:`
([Zones and local times](10-configuration-and-time.md#zones-and-local-times)),
unchanged:

- In a gap, `Dst.earlier` moves it back by the gap, and `Dst.later` and
  `Dst.compatible` move it forward.
- In an overlap, `Dst.earlier` and `Dst.compatible` take the first
  instant, and `Dst.later` the second.
- Left out, or `Dst.reject`, the call fails and its `attempt` maps
  `dst:`.

A date with no `at:` is `type.argument_count`, and `at:` beside a
`LocalDateTime` is `type.temporal_option_unused`. A value that is
neither a `LocalTime` nor a clock time written out is
`type.temporal_clock_invalid`. An editor's snippet for the call leaves
`at:` out, since it is an option.

### Storing an event

An event is stored a column per part, and each part that may be `none`
is a column that may be `NULL`. The days are in `date` columns, which no
reader's zone moves; a local time in a `timestamp` without a zone; the
zone as its IANA name; and the recurrence as its text. So a stored event
comes back as it was written:

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

Not built yet: a writer of calendar files, which adds the carriage
returns and line folding RFC 5545 asks of a file and writes `UID` and
`DTSTAMP` from your app's own identity and times.

## Text
<!-- topic: text -->

The `text` operations count, search, cut, pad and change the case of
text, and turn other values into text and back. Each is called as
`text.<name>`, such as `text.trim(name)`.

Text is counted, cut and padded in Unicode code points, as `max_length`
counts it ([Constraints](02-values-and-types.md#constraints)) and as
PostgreSQL's `char_length` and Python do. A code point isn't always what a
person sees as one character: an emoji with a skin tone is two. No
operation counts those characters (graphemes).

None of these operations takes a `PresentationText`, the text Jadpo
formats for people, because it is a representation of its own
([Showing dates and times to people](10-configuration-and-time.md#showing-dates-and-times-to-people)).

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `length(value)` | `Int`, at least 0, in code points | | `char_length(x)::bigint` |
| `trim(value)` | `Text` without Unicode white space at either end; never longer | | `btrim(x, chars)`, `chars` the Unicode `White_Space` characters |
| `lower(value, locale:)`, `upper(value, locale:)` | `Text` in Unicode's default case mapping, or a locale's (`İ` in Turkish) with `locale:` | | none |
| `contains(value, part)` | `Bool` | | `strpos(x, a) > 0` |
| `starts_with(value, prefix)`, `ends_with(value, suffix)` | `Bool` | | `starts_with(x, a)`; `right(x, char_length(a)) = a COLLATE "C"` |
| `replace(value, find, with)` | `Text` with every `find` replaced, left to right; `find` never empty | | `replace(x, a, b)` |
| `split(value, separator)` | `List<Text>`, at least one item; `separator` never empty; `""` gives `[""]`, and separators side by side give `""` between them | | none |
| `join(items, separator)` | `Text`, the items with `separator` between them | | none |
| `slice(value, from, count)` | `Text`, up to `count` code points from position `from`, cut short at the end; both never negative; never longer | | `substr(x, a + 1, b)` |
| `keep_first(value, n)`, `keep_last(value, n)` | `Text`, at most `n` code points from that end; `n` never negative; never longer | | `left(x, n)`; `right(x, n)` |
| `drop_first(value, n)`, `drop_last(value, n)` | `Text` without `n` code points from that end; never longer | | `substr(x, n + 1)`; `CASE WHEN n = 0 THEN x ELSE left(x, -n) END` |
| `pad_start(value, length, fill:)` | `Text` of at least `length` code points, `fill` added at the start; never cut; `fill:` one code point written out | | `CASE` on `char_length(x)` with `lpad` |
| `from(value)` | `Text`, the value as JSON carries it: an `Int`'s digits, a `Decimal` with its scale's places, a `Uuid` in lowercase, `true` or `false`, a time in its wire form | | `x::text` for `Int`, `Decimal`, `Uuid` and `Bool`; times none |
| `parse(raw, as: T)` | `T`, any standard type or type built on a representation, from its canonical text ([the contract](#every-standard-type-meets-one-contract)) | `malformed`, or the rule it breaks | none |
| `format(value, locale:)` | `PresentationText` for people, any standard type, with the type's own options (`unit:`, `precision:`) ([the contract](#every-standard-type-meets-one-contract)) | | none |
| `pick(value, locale:)` | `Text` from a `LocalisedText`: the locale's entry, else its language's, else the default | | none |
| `slug(value)` | `Text` of lowercase ASCII letters, digits and single `-`: accents dropped, every other run of characters one `-`, none at either end; may be empty | | none |

A bakery's product names, through every text operation. It sorts in
German and Swedish, so it declares those locales:

```jadpo check project=text-operations
locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.de_de, Locale.sv_se]
}

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
each half of an emoji with a skin tone count as one each. Text in another
script is cut the same way.

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

### Checking text against a pattern

There is no `text.matches`, and no replacing or capturing by regular
expression. To check whether text looks like a postcode, build the type
whose `pattern` says what a postcode is:
`attempt Postcode(raw) { pattern: ... }`
([Creating values](02-values-and-types.md#creating-values)). A pattern is
a type's rule, and it is checked in linear time.

### Joining text

To join two pieces of text, use `+`
([Expressions and operators](03-logic.md#expressions-and-operators)).
`text.join` is for a list.

`join` takes a list of any type built on `Text`, such as
`List<TagName>`, as `list.sum` takes a list of any number type.
Collections stay invariant everywhere else.

### Cutting and padding

`trim`, `slice` and the four cuts (`keep_first`, `keep_last`,
`drop_first` and `drop_last`) never give text longer than they were
given. The compiler knows this (`at_most_length_of`), so a type built
from the result keeps the text's `max_length` without an `attempt` arm.
It reads the bound from the cut text's type's `max_length`, from a
literal's length, or from a cut inside a cut. A count written out, as in
`keep_first(name, 12)`, bounds nothing, because `at_most_length_of`
records the argument only. `split`'s "at least one item" doesn't prove a
`min_length` either.

`lower` and `upper` can make text longer (`ß` becomes `SS`), so they
carry no such bound.

`keep_first` can split what a person sees as one character, such as an
emoji with a skin tone, which is two code points. The preview it cuts is
still valid text.

`pad_start`'s `length` is any `Int`. One at or below the text's own
length gives the text back. `fill:` is required, and is one code point
written as a text literal. A value, or a literal of any other length, is
`type.mismatch`, as a `scale:` that isn't written out is.

White space, for `trim`, is Unicode's `White_Space` property.

There is no `trim_start`, `trim_end` or `pad_end`, and no way to go
through text character by character. Each job that needs one, such as a
slug, a mask, a preview or a change of case, is an operation instead.
Unicode normalisation happens where text comes in, not in a call (below,
and [Language gaps](#language-gaps-this-page-depends-on)).

### Arguments checked while compiling

`replace`'s `find` and `split`'s `separator` are never empty
(`never_empty`). A literal can't be `""`, and a value's type must have a
`min_length` of at least 1; otherwise the call is `type.argument_empty`
([Errors](#errors)). Counts and positions are never negative, as for
lists.

### Text from outside is NFC

The same character can be written in two ways: `é` as one code point, or
as `e` followed by a combining accent. Every text a request carries, in
its body, path, query string or headers, is composed to Unicode's NFC as
it is decoded, before its rules are checked. So `café` typed either way is
the same text, and `max_length` counts the code points it is kept as
([Constraints](02-values-and-types.md#constraints),
[Text is normalised](07-http.md#text-is-normalised)).

This holds for every type built on `Text`, an `Email` included, read
through the route's codecs, a map's keys among them. Text written in code
and text read back from the database are kept as they are.

### Case and sorting in a locale

Without `locale:`, case follows Unicode's full default mapping (`ß`
becomes `SS`, and `İ` becomes `i` and a combining dot), and
`list.sort_by` sorts text keys by code point, as queries sort them.

With `locale:`, case follows that locale's rules (`i` becomes `İ` in
Turkish), and `list.sort_by` orders text keys as the locale's alphabet
does (`Ö` comes after `Z` in Swedish, and beside `O` in German). Both go
through ICU4X's case mapping and collation.

Case mapping takes any `Locale`, not only the ones `locales { }`
declares, because its data is Unicode's: one table for every locale.

Sorting uses only the locales `locales { }` declares. A `Locale` your
application doesn't support sorts as the supported locale that a header
asking for it would get, else as `default:`, the same way text for people
is written ([Locales](10-configuration-and-time.md#locales)). A
deployable carries only its supported locales' collation, so it sorts as
`jadpo test` does. A project that sorts by a locale and declares none is
`type.temporal_locales_required`, as one that formats is. Files handed to
the compiler on their own have no application to declare locales, so
they sort by the `Locale` they are given.

Collation is the locale's default, at CLDR's default strength, so keys
that are equal by its rules keep their order. On `list.sort_by`,
`locale:` orders text keys only. With any other key it is
`type.temporal_option_unused`, whose edit removes it.

Here a shop supports British English and Swedish. A shopper writing
Swedish in Finland gets Swedish order, the supported locale of their
language. One writing Finnish gets British English order, the shop's
default, since it supports no Finnish; in Finnish's own alphabet `Ö`
would come after `Z`:

```jadpo check project=sorting-supported-locales file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.sv_se]
}
```

```jadpo check project=sorting-supported-locales file=domains/shop/types/product_name.jadpo
type ProductName = Text {
    min_length: 1
    max_length: 60
}
```

```jadpo check project=sorting-supported-locales file=domains/shop/actions/in_name_order.jadpo
use shop/types { ProductName }

/// The loaves in the order the shopper's alphabet puts their names, of the
/// alphabets the shop supports.
action in_name_order(loaves: List<ProductName>, shopper: Locale) -> List<ProductName> {
    return list.sort_by(loaves, loaf -> loaf, locale: shopper)
}
```

```jadpo check project=sorting-supported-locales file=domains/shop/actions/in_name_order.test.jadpo
use shop/types { ProductName }

test "a locale the shop doesn't support sorts in one it does" {
    var loaves: List<ProductName> = ["Zopf", "Ölandsbröd", "Oat"]
    assert in_name_order(loaves, Locale.sv_se) == ["Oat", "Zopf", "Ölandsbröd"]
    assert in_name_order(loaves, Locale.sv_fi) == ["Oat", "Zopf", "Ölandsbröd"]
    assert in_name_order(loaves, Locale.fi_fi) == ["Oat", "Ölandsbröd", "Zopf"]
}
```

### Values as text: `from`, `parse` and `format`

`text.from` gives a value as JSON carries it
([Built-in types](02-values-and-types.md#built-in-types),
[Time types](10-configuration-and-time.md#time-types)). It takes a
standard type with a canonical text, and `Int`, `Decimal` and `Bool`;
anything else is `type.mismatch`. It takes no enum your application
declares: such an enum has no display label
([Enums](02-values-and-types.md#enums)), so showing one is a `match`. It
takes no text either, since a value built on `Text` already is text
(`Email`, `Url` and `IpAddress` included).

A standard type's canonical text is the one form
[the contract](#every-standard-type-meets-one-contract) writes it as:

- A standard enum's code is its variant's entry in its table: a
  `Country`'s `GB`, a `Zone`'s IANA name, a `Locale`'s BCP 47 tag, a
  `FileType`'s media type (its variants are `png`, `jpeg`, `webp`, `pdf`,
  `csv`, `tsv` and `json`), and a `Weekday`'s own name (`monday`), which
  has no other code. Its `format` is its CLDR name where it has one.
- An `Instant` ends in `Z`.
- A `ZonedTime` is RFC 9557
  (`2026-10-25T09:30:00+00:00[Europe/London]`), and is read only with the
  offset its zone has then.
- An `InstantRange` is ISO 8601's interval, `<start>/<end>`, its start
  not after its end (`start_not_after_end`).
- A `Duration` is the ISO 8601 form JSON already carries, in hours
  (`PT26H`).

`text.parse(raw, as: T)` reads a value from its canonical text. `as:` is
a type written out: a standard type, or a type built on one or on `Text`,
`Int`, `Decimal` or `Bool`. Your application's enum or object is
`type.mismatch`. Text that isn't the type at all fails `malformed`, an
`Email` that isn't an address included. The rules of the type `as:`
names break by their own names, as a construction's do. Text read as
text is never `malformed`.

`text.format(value, locale:)` gives text for people:

- `Email`, `Url`, `IpAddress`, `Uuid` and `FileType` as their canonical
  text.
- A `Weekday` as CLDR's stand-alone weekday name.
- A `Locale` as CLDR's display name ("British English").
- A `Zone` as CLDR's generic location format ("UK Time").
- A `Duration` in CLDR's long unit names, joined as the locale joins a
  list of units ("1 hour, 30 minutes").
- An `InstantRange` as its two ends in its format, joined by ` – `,
  CLDR's English interval fallback.

`zone:` and `format:` are options of the time types only, as their
entries say. A missing `format:` is `type.temporal_format_invalid`, and
an option a type doesn't take is `type.temporal_option_unused`. How a
number is formatted is under [Numbers](#numbers).

### Slugs

`text.slug` decomposes text canonically (NFD) and drops every mark
(Unicode general category M), so `é` becomes `e`. Compatibility
characters, such as `ﬁ` or a full-width `Ａ`, aren't folded: like any
other character outside ASCII, each becomes `-`.

### `text` is a reserved name

`text` is a standard-library namespace, as every standard namespace is.
It can't name a variable, parameter or declaration, so `text.` always
means the standard library. A field or a named argument may still be
called `text`:

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
  help: https://jadpo.dev/docs/errors/semantic.standard_namespace_reserved

check failed: 1 error, 1 file
```

## Text formats
<!-- topic: text-formats -->

Four standard types hold text with structure:

- `Html` is HTML that is always safe to put in a page.
- `Markdown` is CommonMark text.
- `RichText` is a structured document of blocks and marks, as a rich
  text editor holds one.
- `LocalisedText` maps locales to text, and is read with `text.pick`.

None of them is personal data, and none has a shape.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `markup.to_html(markdown)` | `Html`, the CommonMark rendered, then made safe as any `Html` is | | none |
| `markup.rich_to_html(rich)` | `Html` from a `RichText` | | none |
| `markup.to_plain_text(html)` | `Text`, the text content, with a new line between blocks | | none |

A food blog turns a post written in Markdown into its page, keeps only
the safe part of a profile pasted from a rich text editor, and shows the
words of a post in a notification:

```jadpo check project=text-formats
type FormText = Text {}

type PreviewText = Text {}

/// A post's body as its author wrote it, made into the page's HTML.
function post_body(post_markdown: Markdown) -> Html {
    return markup.to_html(post_markdown)
}

/// A profile's about section, pasted from a rich text editor: whatever it
/// holds, only the safe part is kept.
function about_section(pasted_about: FormText) -> Html {
    return Html(pasted_about)
}

/// The words a notification shows from a post, without its markup.
function preview(post_page: Html) -> PreviewText {
    return PreviewText(markup.to_plain_text(post_page))
}
```

```jadpo check project=text-formats file=main.test.jadpo
test "a post's Markdown becomes safe HTML" {
    assert post_body("Soup of the **day**") == "<p>Soup of the <strong>day</strong></p>\n"
    assert post_body(
        "[menu](https://example.com/menu) or [prize](javascript:steal())"
    ) == "<p><a href=\"https://example.com/menu\" rel=\"noopener noreferrer nofollow\">menu</a> or <a rel=\"noopener noreferrer nofollow\">prize</a></p>\n"
}

test "pasted HTML keeps only what is safe" {
    assert about_section("<p onclick=\"steal()\">Chef</p><script>steal()</script>") == "<p>Chef</p>"
    assert preview(
        about_section("<h2>Menu</h2><ul><li>Soup</li><li>Bread</li></ul>")
    ) == "Menu\nSoup\nBread"
}
```

### `Html` is safe whatever it is built from

An `Html` value holds only the markup its construction keeps: no
scripts, no event handlers, and links only to `https:` and `mailto:`. It
is a security boundary. Every way text becomes `Html` runs the one
sanitiser: `Html(raw)`, `text.parse(raw, as: Html)`, a request's JSON as
it is read, and `markup.to_html`. None of them fails, because whatever
the text holds, only the safe part is kept. So none needs an `attempt`,
and `text.parse` is never `malformed`.

A type built on `Html` keeps its own rules, counted on what is kept: its
`max_length` counts the text once the script is gone. Naming `format` in
its `attempt` is `failure.rule_cannot_break`.

A literal written where an `Html` goes must already be what the
sanitiser keeps, since a literal is checked while compiling. That way
code never holds HTML that changes when it runs, and a test compares
against what is really kept. Any other literal is `type.invalid_literal`,
whose reason and `next:` give the kept form: `"Tom & Jerry"` is written
`"Tom &amp; Jerry"`.

A plain `Text` never becomes an `Html` without `Html(...)`, which is the
sanitiser, and text operations on an `Html` give plain `Text`. So the
only `Html` a template may write out unescaped
([Email](16-built-in-services.md#email)) is one made safe. An `Html` and
a `Markdown` are each sent and stored as their text. An `Html` read back
from storage is the text that was stored, made safe when it was built.

### What the sanitiser keeps

The sanitiser is ammonia, an allow-list over html5ever, which parses
HTML as browsers do. Beyond the three rules above, it keeps as little as
it can. It keeps these elements, with only the attributes in brackets:
`a` (`href`, `title`), `abbr` (`title`), `b`, `blockquote`, `br`, `code`,
`del`, `em`, `h1` to `h6`, `hr`, `i`, `ins`, `kbd`, `li`, `mark`, `ol`
(`start`), `p`, `pre`, `q`, `s`, `small`, `strong`, `sub`, `sup`, `u` and
`ul`. It keeps no other element or attribute: no images (a remote image
tells its server who read the page), tables, forms, frames, media,
styles, classes or ids.

A link without its own `https:` or `mailto:` scheme loses its `href`:
a relative link, a protocol-relative `//host`, `http:`, and `javascript:`
however it is encoded. Every link gets `rel="noopener noreferrer
nofollow"`, whatever it was written with. Comments go.

A `script`, `style`, `svg`, `math`, `template`, `iframe`, `object`,
`embed`, `noscript`, `noembed`, `noframes`, `textarea`, `title`, `xmp`,
`plaintext`, `select`, `applet`, `frame` or `frameset` goes with
everything inside it. Any other element the sanitiser doesn't allow goes
but leaves its text.

What the sanitiser keeps is a fixed point: its output, sanitised again,
must come back unchanged. That way a browser parsing the page builds the
tree the allow-list kept, which is the defence against mutation XSS.
Output that doesn't come back unchanged is kept as escaped text instead,
and as nothing when even that doesn't. Markup nested more than 512
elements deep, Blink's limit, is kept as text too: html5ever parses deep
nesting in quadratic time, so one large value could stall a program.

### Markdown

A `Markdown` value is CommonMark 0.31.2, rendered by pulldown-cmark with
none of its extensions: no tables, strikethrough, task lists or
footnotes. It is any text with no control character but a tab, a line
feed or a carriage return, so a form's text area, whose lines end in
CRLF, sends Markdown as it is.

`markup.to_html` renders raw HTML inside the Markdown as CommonMark says,
then sanitises the whole result, so raw HTML is never passed through
unsanitised. An image is shown as its description, since `Html` keeps no
images.

### Plain text from HTML

`markup.to_plain_text` sanitises, then puts each block (a paragraph, a
heading, a list item, a quote, a code block, a rule) on its own line and
turns `<br>` into a line break. It runs white space together as a
browser shows it, trims each line and leaves out empty ones. Inside
`<pre>` it keeps the lines as written, less blank ones at either end. A
list item gets no bullet or number.

`text.format` shows the text people read: an `Html` value's
`markup.to_plain_text`, and a `Markdown` text's once rendered. An `Html`
value's `text.from` is the HTML itself.

### `RichText`

A `RichText` is a document as a rich text editor holds one, in Portable
Text's flat shape with its default schema. It is a list of blocks, each
holding spans of text. A block is a paragraph, a heading of level 1 to 6,
a quote, or a bulleted or numbered list item with its level, 1 to 9.
There is no code block, rule, image or table.

A span has its marks and at most one link. The marks are `strong`,
`emphasis`, `code`, `underline` and `strike_through`: Portable Text's
default decorators, with its `em` and `strike-through` renamed. An HTML
link can't hold another, so the link is a part of the span, not a mark.

Nothing nests, so no type refers to itself. A list item's level says how
deep it sits, as Portable Text's `level` does and as Word and Google Docs
nest lists.

Its parts are three standard types, since a part must be a standard type
to be read, sent and stored: `RichTextBlock`, a choice; `RichTextSpan`,
an object sent as its members; and `RichTextMark`, an enum.

It is sent in Jadpo's own form, not with Portable Text's `_type`, `_key`,
`children` and `markDefs`. The form is an array of its blocks, each in
the tagged form an enum variant with fields takes in JSON
([Enums](02-values-and-types.md#enums)). Each span is an object of
`text`, `marks` and `link`, leaving out the link when it has none:

```json
[
  {"tag": "heading", "level": 2, "spans": [{"text": "Method", "marks": []}]},
  {"tag": "paragraph", "spans": [{"text": "our mill", "marks": ["emphasis"], "link": "https://example.com/mill"}]}
]
```

Every block and mark is an element the sanitiser keeps. A link must be
one the sanitiser keeps as it is, which is either:

- an `https:` URL whose host is ASCII labels of letters, digits and
  single hyphens, the last beginning with a letter (never an IP address
  or punycode), with a port up to 65535; or
- a `mailto:` address,

each in the characters a URL writes unescaped. That refuses some links
the sanitiser would keep, such as a host written in Unicode, and none it
would drop; a test of random links holds the two together.

A link that isn't one of these is refused where the document is built or
read, breaking `pattern`, rather than dropped. A literal that breaks it
is `type.invalid_literal`, which says the rule in words rather than as
its pattern. A span's text is never empty, and has no control character
but a tab or a line feed.

`markup.rich_to_html` writes `<p>`, `<h1>` to `<h6>`, `<blockquote>`,
list items in `<ul>` or `<ol>` (a deeper item in a list inside the item
before it), `<strong>`, `<em>`, `<code>`, `<u>` and `<s>`, links with
`rel="noopener noreferrer nofollow"`, a line feed as `<br>`, and text
escaped as the sanitiser's serialiser escapes it. It writes no white
space between elements. A span's marks are written once each, in one
nesting order, `strong` outermost and `strike_through` innermost, inside
the link, whatever order the span lists them in. So it writes nothing the
sanitiser would change, which tests hold it to, and it sanitises the
result all the same: an `Html` is only ever what the sanitiser keeps.

A recipe site keeps each method as its editor sends it:

```jadpo check project=rich-text
type EditorJson = Text {}

type MethodUnreadable = Failure {
    kind: FailureKind.invalid_value
}

/// A recipe's method as the editor sent it: its blocks and marks, with
/// links only to `https:` and `mailto:` addresses.
function method_from(sent: EditorJson) fails MethodUnreadable -> RichText {
    return attempt json.read(sent, as: RichText) {
        malformed: MethodUnreadable
        mismatch: MethodUnreadable
    }
}

/// The method on the recipe's page.
function method_page(recipe_method: RichText) -> Html {
    return markup.rich_to_html(recipe_method)
}
```

```jadpo check project=rich-text file=main.test.jadpo
test "a recipe's method becomes safe HTML" {
    var oat_method: RichText = attempt method_from(
        "[{\"tag\":\"heading\",\"level\":2,\"spans\":[{\"text\":\"Method\",\"marks\":[]}]},{\"tag\":\"numbered_item\",\"level\":1,\"spans\":[{\"text\":\"Soak the oats\",\"marks\":[]}]},{\"tag\":\"numbered_item\",\"level\":1,\"spans\":[{\"text\":\"Bake for \",\"marks\":[]},{\"text\":\"40 minutes\",\"marks\":[\"strong\"]}]},{\"tag\":\"paragraph\",\"spans\":[{\"text\":\"Flour from \",\"marks\":[]},{\"text\":\"our mill\",\"marks\":[\"emphasis\"],\"link\":\"https://example.com/mill\"}]}]"
    )
    assert method_page(
        oat_method
    ) == "<h2>Method</h2><ol><li>Soak the oats</li><li>Bake for <strong>40 minutes</strong></li></ol><p>Flour from <a href=\"https://example.com/mill\" rel=\"noopener noreferrer nofollow\"><em>our mill</em></a></p>"
    assert text.format(
        oat_method,
        locale: Locale.en_gb
    ) == "Method\nSoak the oats\nBake for 40 minutes\nFlour from our mill"
}

test "a link goes only to https: and mailto:" {
    assert method_from(
        "[{\"tag\":\"paragraph\",\"spans\":[{\"text\":\"prize\",\"marks\":[],\"link\":\"javascript:steal()\"}]}]"
    ) fails MethodUnreadable
    var note: RichText = RichText(
        [
            RichTextBlock.paragraph {
                spans: [
                    RichTextSpan {
                        text: "Write to us"
                        marks: []
                        link: "mailto:hello@example.com"
                    }
                ]
            }
        ]
    )
    assert method_page(
        note
    ) == "<p><a href=\"mailto:hello@example.com\" rel=\"noopener noreferrer nofollow\">Write to us</a></p>"
}
```

### `LocalisedText`

A `LocalisedText` maps locales to text, such as a product's name in each
language a shop sells in. It is a named map, `Map<Locale, Text>`, and may
be empty. It is sent as JSON-LD sends a language map: an object keyed by
each locale's tag, `{"en-GB": "Rye loaf", "fr-FR": "Pain
de seigle"}`, rather than a list of entries, so each locale has one entry
by construction.

Code builds one from a map, `LocalisedText(map.with(...))`, as it builds
any named map of your application's, since there is no map literal with
entries ([Named collection construction](02-values-and-types.md#named-collection-construction)).

Its keys are `Locale`s like any other. One read from a request or from
JSON is a locale your application supports: an unsupported tag breaks
`variant`, and OpenAPI lists only the supported tags as its keys. One
written in code may be any locale.

`text.pick` chooses an entry by the fallback chain
[Locales](10-configuration-and-time.md#locales) describes: the locale's
own entry, else one in its language (the locale CLDR's likely subtags
give for the language first, then in the canonical order), else the
entry of the application's default locale. Text with none of those gives
its first entry in the canonical order, so text that has any entry always
shows one. An empty one gives `""`.

`text.pick`'s `locale:` is checked as `text.format`'s is, since it
chooses text for a person. One written out must be a locale the
application supports (`type.locale_unsupported`), and an application with
no `locales { }` is `type.temporal_locales_required`:

```jadpo check project=localised-text file=app.jadpo
application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_us, Locale.fr_fr, Locale.fr_ca, Locale.de_de]
}
```

```jadpo check project=localised-text file=domains/catalogue/actions/shelf_name.jadpo
type ShelfName = Text {}

/// A loaf's name in the shopper's language: their locale's entry, else
/// one in their language, else the shop's default locale's.
action shelf_name(name: LocalisedText, shopper: Locale) -> ShelfName {
    return ShelfName(text.pick(name, locale: shopper))
}
```

```jadpo check project=localised-text file=domains/catalogue/actions/shelf_name.test.jadpo
test "a loaf's name in the shopper's language" {
    var rye: LocalisedText = LocalisedText(
        map.with(map.with([], Locale.fr_fr, "Pain de seigle"), Locale.en_gb, "Rye loaf")
    )
    assert shelf_name(rye, Locale.fr_fr) == "Pain de seigle"  // its own entry
    assert shelf_name(rye, Locale.fr_ca) == "Pain de seigle"  // its language's
    assert shelf_name(rye, Locale.de_de) == "Rye loaf"  // the default locale's
    var roggen: LocalisedText = LocalisedText(map.with([], Locale.de_de, "Roggenbrot"))
    assert shelf_name(roggen, Locale.fr_fr) == "Roggenbrot"  // the only one it has
}
```

### Showing, sending and storing rich and localised text

`text.format` shows a `RichText`, a block or a span as the text people
read in it (its HTML's `markup.to_plain_text`), each block on its own
line. It shows a `LocalisedText` as `text.pick` gives it in that locale.
Neither type has a single text form, so `text.from` and `text.parse`
refuse them, and neither is ordered.

Each is a named collection the standard library declares, as a standard
type may be a list or a map of parts. It is sent as JSON's own
collection, and a `RichText`, a `RichTextBlock` and a `LocalisedText` are
each stored as that JSON in one `jsonb` column.

## Hashing and cryptography
<!-- topic: hashing-and-cryptography -->

The `hash` and `crypto` operations hash values, encrypt and decrypt
them, hash and check passwords, sign and check messages, and check
tokens and one-time codes. Each job has one construction, chosen to be
safe, so you never choose an algorithm.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `hash.sha256(value)` | `Bytes`, the 32-byte SHA-256 digest of a value built on `Bytes`, or of a `Text`'s UTF-8 bytes | | `sha256(x)`, or `sha256(convert_to(x, 'UTF8'))` for text |
| `crypto.encrypt(value, key:, context:)` | `Bytes`: XChaCha20-Poly1305 with a fresh random nonce, the format's version and the key's identity in front. `value` is `Bytes` or a `Text`'s UTF-8; `context:` text bound to the result (a record's id), so a sealed value can't be moved to another row. Actions only | | none |
| `crypto.decrypt(sealed, key:, context:)` | `Bytes`, trying the current key, then `_PREVIOUS` | `unreadable` (wrong key or context, or altered) | none |
| `crypto.hash_password(password)` | `PasswordHash` of a `Password`, a built-in text type holding the PHC string: Argon2id at the parameters fixed for the release (OWASP's: 19 MiB, 2 passes, 1 lane). Actions only | | none |
| `crypto.verify_password(password, hash)` | `PasswordCheck` for a `Password`: `wrong`, `correct`, or `correct_outdated` (store a fresh hash) | | none |
| `crypto.hmac(value, key:)` | `Bytes`, HMAC-SHA-256 with the current key | | none |
| `crypto.verify_hmac(value, mac, key:)` | `Bool`, in constant time, against the current or previous key | | none |
| `crypto.sign(value, key:)` | `Bytes`, an Ed25519 signature; `key:` a `PrivateKey` setting | | none |
| `crypto.public_key(key:)` | `PublicKey`, the Ed25519 public half of a `PrivateKey` setting, which the app may publish so others can check what it signs | | none |
| `crypto.verify_signature(value, signature, key)` | `Bool`; `key` a `PublicKey`, which is not secret and may be read | | none |
| `crypto.verify_jwt(token, keys:, issuer:, audience:, as:)` | `JwtClaims`, checked against a set of `Jwk`s, each key's own algorithm only (never `none`, never chosen by the token): Ed25519, RS256 for an RSA key and ES256 for a P-256 key, with expiry, not-before and issued-at against `clock.now`, and the issuer and audience the call names, each optional; with `as:`, an `Object` of the application's, the claims it names instead, read as `json.read` reads them, a registered one as `JwtClaims` holds it; it reads the clock, so actions call it | `invalid_signature`, `expired`, `not_yet_valid`, `malformed`, `wrong_audience`, `wrong_issuer` when `issuer:` is given, and `mismatch` when `as:` is | none |
| `crypto.read_certificate(der)` | `Certificate`, read from its DER `Bytes` as its PEM's DER is: a key set's `x5c` after `encoding.from_base64`, or a file's bytes. It needs no key, so a function may call it | `malformed` | none |
| `crypto.verify_totp(code, secret)` | `Bool` for a `OneTimeCode` against a `TotpSecret`, RFC 6238 with one step either side. Waits for keys held as data | | none |

A clinic seals each clinical note to its own record, hashes its staff's
passwords, checks the webhooks its payment provider sends with the secret
they share, and signs the receipts it issues. The keys are settings,
each given as a call's `key:` and read nowhere else:

```jadpo check project=cryptography file=config.jadpo
config {
    notes_key: EncryptionKey
    payments_webhook_secret: SigningKey
    receipt_key: PrivateKey
}
```

```jadpo check project=cryptography
type ClinicalNote = Text {
    max_length: 4000
}

type NoteId = Text {}

type SealedNote = Bytes {}

type StaffPassword = Password {
    min_length: 12
    max_length: 128
}

type WebhookBody = Text {
    max_length: 100000
}

type WebhookSignature = Bytes {}

type Receipt = Text {
    max_length: 2000
}

type ReceiptSignature = Bytes {}

type Genuine = Bool {}

type NoteUnreadable = Failure {
    kind: FailureKind.invalid_value
}

/// A clinical note sealed to its own record, so it can't be moved to
/// another.
action seal_note(note: ClinicalNote, record: NoteId) -> SealedNote {
    return SealedNote(crypto.encrypt(note, key: config.notes_key, context: record))
}

/// A sealed note's text: another record's id, an altered note or another
/// key can't open it.
function open_note(sealed_note: SealedNote, record: NoteId) fails NoteUnreadable -> ClinicalNote {
    var note_bytes: Bytes = attempt crypto.decrypt(
        sealed_note,
        key: config.notes_key,
        context: record
    ) {
        unreadable: NoteUnreadable
    }
    var note_text: Text = attempt encoding.from_utf8(note_bytes) {
        malformed: NoteUnreadable
    }
    return attempt ClinicalNote(note_text) {
        max_length: NoteUnreadable
    }
}

/// The hash a member of staff's password is stored as.
action stored_password(staff_password: StaffPassword) -> PasswordHash {
    return crypto.hash_password(staff_password)
}

/// Whether a password signs in; `correct_outdated` asks for a fresh hash.
function sign_in_check(staff_password: StaffPassword, stored_hash: PasswordHash) -> PasswordCheck {
    return crypto.verify_password(staff_password, stored_hash)
}

/// The signature on a webhook the clinic sends to the payment provider.
function webhook_signature(body: WebhookBody) -> WebhookSignature {
    return WebhookSignature(crypto.hmac(body, key: config.payments_webhook_secret))
}

/// Whether a webhook came from the payment provider, which shares the
/// secret.
function from_provider(body: WebhookBody, provider_signature: WebhookSignature) -> Genuine {
    return Genuine(
        crypto.verify_hmac(body, provider_signature, key: config.payments_webhook_secret)
    )
}

/// A receipt's signature, which anyone with the clinic's public key can
/// check.
function receipt_signature(receipt: Receipt) -> ReceiptSignature {
    return ReceiptSignature(crypto.sign(receipt, key: config.receipt_key))
}

/// The clinic's public key, which it publishes so anyone can check its
/// receipts.
function clinic_public_key() -> PublicKey {
    return crypto.public_key(key: config.receipt_key)
}

/// Whether a receipt was signed with the private half of `issuer_key`.
function receipt_from(
    receipt: Receipt,
    claimed_signature: ReceiptSignature,
    issuer_key: PublicKey
) -> Genuine {
    return Genuine(crypto.verify_signature(receipt, claimed_signature, issuer_key))
}
```

A test run makes its own throwaway keys, and its nonces and salts come
from the test's seed, as `random`'s values do. The older hash is the
same password at 4 MiB and 3 passes, and the signature is RFC 8032's
second test vector, the message `r` under its key:

```jadpo check project=cryptography file=main.test.jadpo
test "a sealed note opens only for its own record" {
    var sealed_copy: SealedNote = seal_note("Allergic to penicillin.", "note-1")
    var read_back: ClinicalNote = attempt open_note(sealed_copy, "note-1")
    assert read_back == "Allergic to penicillin."
    assert open_note(sealed_copy, "note-2") fails NoteUnreadable
    assert seal_note("Allergic to penicillin.", "note-1") != sealed_copy
}

test "a password signs in, and an older hash asks for a fresh one" {
    var stored_hash: PasswordHash = stored_password("correct horse battery staple")
    assert sign_in_check("correct horse battery staple", stored_hash) == PasswordCheck.correct
    assert sign_in_check("correct horse battery stapler", stored_hash) == PasswordCheck.wrong
    assert sign_in_check(
        "correct horse battery staple",
        "$argon2id$v=19$m=4096,t=3,p=1$AAECAwQFBgcICQoLDA0ODw$VTsSHiMxkq48zEo8LUSymjr7JjaRSgsxGX2Z8HQE9Cw"
    ) == PasswordCheck.correct_outdated
}

test "a webhook is checked against its signature" {
    var body: WebhookBody = "{\"paid\": true}"
    assert from_provider(body, webhook_signature(body))
    assert not from_provider("{\"paid\": false}", webhook_signature(body))
}

test "a receipt's signature checks with its issuer's public key" {
    var rfc_signature_bytes: Bytes = attempt encoding.from_hex(
        "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00"
    ) {
        malformed: NoteUnreadable
    }
    var rfc_signature: ReceiptSignature = ReceiptSignature(rfc_signature_bytes)
    assert receipt_from("r", rfc_signature, "PUAXw-hDiVqStwqnTRt-vJyYLM8uxJaMwM1V8Sr0Zgw")
    assert not receipt_from("R", rfc_signature, "PUAXw-hDiVqStwqnTRt-vJyYLM8uxJaMwM1V8Sr0Zgw")
    assert text.length(encoding.to_hex(receipt_signature("r"))) == 128
}

test "what the clinic signs checks with the key it publishes" {
    var paid_signature: ReceiptSignature = receipt_signature("Paid in full.")
    assert receipt_from("Paid in full.", paid_signature, clinic_public_key())
    assert not receipt_from("Paid in part.", paid_signature, clinic_public_key())
}
```

### One construction for each job

A release that changes a construction keeps reading the old format, by
the sealed value's version byte or the password hash's PHC string, and
`correct_outdated` tells your app to hash a password again when its owner
signs in. [Identity](08-identity.md) leaves checking a password to your
application, which is why it needs these operations.

### Keys are settings that code never reads

Code never reads a secret
([Secrets](10-configuration-and-time.md#secrets)), and a key is no
exception. A key is an opaque value, held in a setting:

- an `EncryptionKey` setting (32 random bytes) for `encrypt` and
  `decrypt`;
- a `SigningKey` setting
  ([Signing keys](10-configuration-and-time.md#signing-keys)) for `hmac`;
- a `PrivateKey` setting (an Ed25519 key's 32 bytes) for `sign` and
  `public_key`.

You write each key by name, `key: config.<name>`, and nowhere else. That
makes a `crypto` operation's `key:` one of the places the compiler owns,
alongside those [Secrets](10-configuration-and-time.md#secrets) lists. A
key given by position, or read anywhere else, is still
`configuration.secret_flow`, whose message names the one place the key
goes. A `key:` given anything but a setting, such as a parameter or
text, is `type.mismatch`.

Reading a key anywhere but a `key:` is `configuration.secret_flow`, even
to take a fingerprint of it:

```jadpo check project=e-key-read expect=configuration.secret_flow
type KeyFingerprint = Bytes {}

config {
    notes_key: EncryptionKey
}

/// A fingerprint of the notes key, to tell in a log which key is in use.
function key_fingerprint() -> KeyFingerprint {
    return KeyFingerprint(hash.sha256(config.notes_key))
}
```

```text output
error: `config.notes_key` is a key
 --> main.jadpo:9:39
  |
9 |     return KeyFingerprint(hash.sha256(config.notes_key))
  |                                       ^^^^^^^^^^^^^^^^ a key, read here
  reason: An `EncryptionKey` is opaque: code never reads, prints, stores, compares or passes on a key, so the one place it goes is a `crypto` call's `key:`, which the compiler owns.
  next: Give `config.notes_key` only as a `crypto` call's `key:`, written `key: config.notes_key` (agent)
  rule: configuration.secret_flow
  help: https://jadpo.dev/docs/errors/configuration.secret_flow

check failed: 1 error, 1 file
```

An `EncryptionKey` rotates as a `SigningKey` does, through a
`NAME_PREVIOUS` setting. A `PrivateKey` reads no `_PREVIOUS`: whoever
checks a signature holds the public key, so rotating one is the
verifiers' job.

An `EncryptionKey` or `PrivateKey` setting's environment variable holds
its 32 bytes as unpadded Base64url, 43 characters. Anything else is
refused at startup, by the check before the app starts
([Checking values before the app starts](10-configuration-and-time.md#checking-values-before-the-app-starts)).
`hmac` and `verify_hmac` use a `SigningKey`'s text bytes directly as the
HMAC key, so an outside system that shares the secret can check what
your app signs.

### A key is never a value

`EncryptionKey`, `PrivateKey` and `SigningKey` are built-in secret
types. Until keys held as data arrive
([Language gaps](#language-gaps-this-page-depends-on)), only a setting
holds one. So a key has no text, no JSON form (for `json.write` and
`json.read`) and no storage: the secret rule wins over the contract's
"never". Code never makes a key, and building a key type from a value is
`type.constructor_input`.

Each executor gives the operation one opaque key value, which prints as
`<key>`. It comes from the source the plan decides for the call: the
setting now, and an encrypted field once keys held as data arrive. Only
the runtime turns it into the key it holds
([compiler-runtime §2.5](../compiler-runtime.md#25-executors),
"A key as a value").

### The sealed format

`crypto.encrypt` writes version 1 of the sealed format: a version byte,
an algorithm byte (1, for XChaCha20-Poly1305), the key's identity (4
bytes), the 24-byte random nonce, the ciphertext and its 16-byte tag. The
associated data is the first six bytes and the call's `context:` as
UTF-8, so the header, the context and the ciphertext are all
authenticated.

The key's identity is the first four bytes of HMAC-SHA-256 of a fixed
label under the key, which says nothing about the key. `decrypt` tries
each of the current and the previous key whose identity the value
carries, current first. Anything else is `unreadable`, never saying
which: a value too short, another version or algorithm, an altered byte,
or another context or key. An empty value seals and opens.

### Password hashes

`crypto.hash_password` uses OWASP's first choice of Argon2id parameters:
19,456 KiB, 2 passes, 1 lane, version 19, a 16-byte salt from the
invocation's random bytes and a 32-byte hash. It writes the reference
PHC string (`$argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>`, in standard
Base64 without padding). A hash checks `correct_outdated` when its
memory, passes, lanes or hash length differs from the release's.

A `PasswordHash` reads only Argon2id at version 19, in its one spelling,
with bounded costs: at most 2 GiB (RFC 9106's costliest
recommendation), 16 passes and 16 lanes, a salt of 8 to 64 bytes and a
hash of 16 to 64. So a hash from outside can't make a sign-in allocate
without bound. `verify_password` reads the hash again and gives `wrong`
for anything else, never a fault. A `PasswordHash` is stored as its PHC
string and never leaves the server (`type.password_hash_exposed`).

`PasswordCheck` is a standard enum, written and stored as its variant's
name.

### Signatures

A `PublicKey` is an Ed25519 key's 32 bytes as unpadded Base64url, the
same spelling as a `PrivateKey` setting's, and its construction checks
the spelling. Signing is pure Ed25519, with no prehash and no context.
`verify_signature` verifies strictly, by RFC 8032's checks, refusing
small-order keys and non-canonical signatures. So a key that isn't a
point on the curve, or a signature of another length, gives `false`.

`verify_hmac` takes the same time whichever key matches: it computes the
MACs of both keys whatever the first gives, and compares each in constant
time.

### Keys and nonces in tests

A test run makes 32 random bytes, as Base64url, for each `EncryptionKey`
and `PrivateKey`, as it makes a signing key's, while a nonce and a salt
come from the test's seed. A deployable takes both from the host's
generator, so no key and nonce repeat.

### Digests

A digest is `Bytes`, and `encoding` writes it as text: `encoding.to_hex`
for an ETag or a content key.

There is no MD5 or SHA-1, since both are broken. An outside system that
still asks for one is reached through a package
([Integrations](11-integrations.md)). That includes TOTP, whose RFC 6238
default is HMAC-SHA-1 and whose per-person secret is a key held as data
([Language gaps](#language-gaps-this-page-depends-on)).

### Encrypted fields

`notes: ClinicalNote { encrypted: true }` on an entity stores the field
sealed with the application's `field_encryption_key:`, and decrypts it on
load. The field is kept out of unique rules, indexes, filters, ordering,
search and logs, as the table on
[Built-in services](16-built-in-services.md) says of field encryption.
There is one key for the application, never one per person
([Encrypted fields](15-personal-data.md#encrypted-fields)).

> `encrypt`, `decrypt`, `hash_password`, `verify_password`, `hmac`,
> `verify_hmac`, `sign`, `verify_signature` and `public_key` are built,
> each checked against its standard's known answers and built for
> WebAssembly. None reads randomness itself, and the cipher, Argon2 and
> Ed25519 wipe their own copies of a key when dropped. Encrypted fields
> aren't built yet ([the roadmap](../roadmap.md)).

## Signed tokens
<!-- topic: signed-tokens -->

`crypto.verify_jwt` checks a JSON Web Token (JWT) that another service
signed, such as the token a partner's sign-in service gives a person.
You give it the service's public keys, and it gives the token's claims
once the signature, the times, the issuer and the audience all hold. It
checks a token as RFC 8725 advises.

A clinic lets members sign in through a partner's sign-in service, which
signs each token with Ed25519 and publishes its public keys as a key set
(RFC 7517). The clinic reads the keys as `Jwk`s and checks each token
against them. The algorithm is the key's, never the token's, and the
token's times, issuer and audience must hold. `verify_jwt` reads the
clock, so it is called in an action:

```jadpo check project=tokens
type PartnerKeySet = Object {
    keys: List<Jwk>
}

type KeySetText = Text {
    max_length: 100000
}

type KeySetUnreadable = Failure {
    kind: FailureKind.invalid_value
}

type PartnerTokenRefused = Failure {
    kind: FailureKind.invalid_value
}

type PartnerTokenExpired = Failure {
    kind: FailureKind.invalid_value
}

/// The partner's public keys, from the key set it publishes (RFC 7517).
function partner_keys(published_key_set: KeySetText) fails KeySetUnreadable -> List<Jwk> {
    var partner_key_set: PartnerKeySet = attempt json.read(published_key_set, as: PartnerKeySet) {
        malformed: KeySetUnreadable
        mismatch: KeySetUnreadable
    }
    return partner_key_set.keys
}

/// What a token from the partner's sign-in service says, once its
/// signature, its times, its issuer and its audience hold.
action partner_sign_in(
    partner_token: Jwt,
    partner_signing_keys: List<Jwk>
) fails PartnerTokenRefused, PartnerTokenExpired -> JwtClaims {
    return attempt crypto.verify_jwt(
        partner_token,
        keys: partner_signing_keys,
        issuer: "https://id.example.com",
        audience: "clinic"
    ) {
        invalid_signature: PartnerTokenRefused
        expired: PartnerTokenExpired
        not_yet_valid: PartnerTokenRefused
        malformed: PartnerTokenRefused
        wrong_audience: PartnerTokenRefused
        wrong_issuer: PartnerTokenRefused
    }
}
```

The test's clock stands at 2026-10-18T09:00:00Z, and each token was made
with RFC 8032's first test key, whose public half the key set holds. The
refused tokens are the classic attacks: one past its expiry, one that
names `alg: none`, one signed with HMAC SHA-256 keyed with the public key
(mistaking a public key for a shared secret), one whose claims were
changed after signing, and one issued for another audience:

```jadpo check project=tokens file=main.test.jadpo
test "a partner's token signs in only while it holds" {
    clock: "2026-10-18T09:00:00Z"

    var partner_signing_keys: List<Jwk> = attempt partner_keys(
        "{\"keys\": [{\"kty\": \"OKP\", \"crv\": \"Ed25519\", \"x\": \"11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo\", \"kid\": \"2026-10\"}]}"
    )
    var genuine: Jwt = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.nOqN7320O3T8e_AyF7XNt32jmgjE8VCU7GVHL89ZUeiRcU3gGM3EgiNxVp3_y422De1PZWd2tZ0LuSqdfm6QCg"
    var partner_claims: JwtClaims = attempt partner_sign_in(genuine, partner_signing_keys)
    assert partner_claims.sub == "member-42"
    assert partner_claims.aud == ["clinic"]
    var past_expiry: Jwt = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzEzOTY5LCJqdGkiOiJhMWIyYzMifQ.0XDMfIlEkopB8bJAJCe95i2zQMGu9mbv_kUtc1QC0xMm5wxmsS9munCCG36_4NkaNcfDKNgaGpswq5QNuh_KDw"
    assert partner_sign_in(past_expiry, partner_signing_keys) fails PartnerTokenExpired
    var alg_none_token: Jwt = "eyJhbGciOiJub25lIiwia2lkIjoiMjAyNi0xMCJ9.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.bUBJ-HYfcv-x-XVebqotpXUryxrUsu15W2sejqXFebmY2OB3-XXAQIWG71XLjOBQDrJnDyfN4RTsPW20OqqBDA"
    assert partner_sign_in(alg_none_token, partner_signing_keys) fails PartnerTokenRefused
    var shared_secret_forgery: Jwt = "eyJhbGciOiJIUzI1NiIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.LEqGD4Thr8CB21DsacQjPDe-b_CRUkja0n3gjc6sT7M"
    assert partner_sign_in(shared_secret_forgery, partner_signing_keys) fails PartnerTokenRefused
    var altered: Jwt = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTEiLCJhdWQiOiJjbGluaWMiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMifQ.nOqN7320O3T8e_AyF7XNt32jmgjE8VCU7GVHL89ZUeiRcU3gGM3EgiNxVp3_y422De1PZWd2tZ0LuSqdfm6QCg"
    assert partner_sign_in(altered, partner_signing_keys) fails PartnerTokenRefused
    var for_the_pharmacy: Jwt = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoicGhhcm1hY3kiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMifQ.ysn61bX4ZDaJ7OCiuPNtYwJ1POMD7b1tVLIfZsDS1oimtwY3XgyhZDpLSkDph6j3fol8nv4KPZQ0g7Ai9pd4Bw"
    assert partner_sign_in(for_the_pharmacy, partner_signing_keys) fails PartnerTokenRefused
}
```

### Three kinds of key, one algorithm each

A `Jwk`'s `kty` is one of:

- `OKP`, an Ed25519 key (`crv` and `x`), used with `EdDSA`;
- `RSA` (`n` and `e`), used with `RS256`;
- `EC`, a P-256 key (`crv`, `x` and `y`), used with `ES256`.

A token is tried only against keys used with the algorithm its header
names, and each with that key's own algorithm. So `none`, `HS256`, an
RS256 token naming an EC key's id, and the reverse, all fail
`invalid_signature` before any key is tried. A key whose own `alg` names
an algorithm other than the token's is never tried. A key set naming any other algorithm,
such as RS384, RS512, PS256 or ES384, doesn't read. HS256 needs a shared
secret held as data, which waits for keys held as data
([Language gaps](#language-gaps-this-page-depends-on)).

### Keys come only from the call

A header's `jwk`, `jku`, `x5u` and `x5c` are never read. A header's
`kid` narrows the keys tried to those with that id; with none, every key
is tried. A header with `crit` fails `invalid_signature` (RFC 7515
§4.1.11), and a nested token (`cty: JWT`) is `malformed`.

### The signature first

Nothing the claims say is read until a key has verified the signature,
strictly. Then come the times, then the issuer, then the audience.

- Ed25519 is checked as `verify_signature` checks it.
- RS256 is RSASSA-PKCS1-v1_5 with SHA-256. It is checked by encoding the
  whole padded digest and comparing it (RFC 8017 §8.2.2), never by
  parsing it, and the signature must be exactly as long as the modulus.
- An ES256 signature is R and S, 64 octets (RFC 7518 §3.4), each from 1
  to below the group's order. A high S is accepted, as RFC 7518, `p256`
  and RFC 7515 §A.3's own example allow. That matters only to something
  keyed on a token's text, and Jadpo keeps no token.

### Times

The times have 30 seconds' leeway: `exp` must be later than now, and
`nbf` and `iat` no later. A token that breaks one fails `expired` or
`not_yet_valid`. A token with no `exp` is `malformed`, so no token lives
for ever. A numeric date is a whole number of seconds within an
`Instant`'s range; a fraction is `malformed`.

`verify_jwt` reads the clock (`Op::reads_clock`), as `clock.now` does,
so a function calling it is `type.clock_context`. In a test, it reads the
test's fixed clock.

### Issuer and audience

`issuer:` and `audience:` are each optional. The issuer is checked only
when the call names one, so `wrong_issuer` needs its arm only then. The
audience is always checked: a token with an audience is refused unless
the call names one of them, and a token with none is refused when the
call names one (RFC 7519 §4.1.3). Both compare exactly, as RFC 7519
compares a StringOrURI.

### One reading of the JSON

The header and the claims must each be one JSON object with no member
named twice at any depth, or the token is `malformed` (RFC 7515 §5.2 and
RFC 7519 §7.2 allow refusing one). So no other reader can take a
different value from the same token.

### Refused tokens

Refusals are `invalid_value` failures, as every standard operation's
conditions map to. Nothing here makes a refused token `unauthenticated`.

### Your own claims: `as:`

`as:` reads a token's claims into an `Object` your application declares.
Its fields are read as `json.read` reads an object's. A claim missing, of
the wrong kind or breaking a rule is `mismatch`, and its first problem
fills a `problem: DecodeProblem` field as `json.read`'s does. Claims the
type doesn't name are ignored, as RFC 7519 §4 requires of claims not
understood, though a nested object it names is read strictly.

Anything but your application's `Object` (`Text`, `JwtClaims`, a list)
is `type.mismatch`. Nothing is read as the type until the signature, the
times, the issuer and the audience hold, and `mismatch` needs its arm
only when `as:` is given.

A field the type has for one of the seven registered claims (`iss`,
`sub`, `aud`, `exp`, `nbf`, `iat`, `jti`) is filled with the value the
call checked, exactly as `JwtClaims` holds it. So one call gives a
token's subject and its own claims. `iss`, `sub` and `jti` are text,
`aud` a list of text, and `exp`, `nbf` and `iat` an `Instant`. A field
declared as another type (`sub: Int`, `exp: Text`) is `type.mismatch`.
One built on its type (`sub: MemberId`) keeps its own rules, and one
without `?` that the token leaves out is `mismatch` (`required`).

### The token types

These types hold what `verify_jwt` takes and gives.

**`Jwt`** is a JWS in compact form: three parts of unpadded Base64url in
one spelling, none empty, so an unsecured token (RFC 7519 §6) is never
one, and at most 8,192 characters. A token is a credential, so it is a
secret value ([Secret values](#secret-values)), never shown.

**`Jwk`** holds one public key and never a secret. Its parts are `kty`,
`crv`, `x`, `y`, `n`, `e`, `kid` (1 to 256 printable ASCII characters),
`alg`, `use_` (`sig`, sent as RFC 7517's `use`), `key_ops` and `x5c`,
each but `kty` optional. Three rules across them hold a key to its kind:
`parts_of_its_kty` (its kind's parts and no other's),
`algorithm_of_its_kty` (an `alg`, where given, is its kind's) and
`a_usable_key`.

- Ed25519's `x` is 32 bytes as unpadded Base64url in one spelling.
- An RSA modulus is 2,048 to 4,096 bits, odd and in its fewest octets.
  Under 2,048 bits can be factored (RFC 7518 §3.3 requires 2,048), and
  4,096 is the most `rsa` checks by default, so a key set can't make
  checking a token cost without bound. The exponent is odd, from 3 to
  2^32 - 1, in its fewest octets.
- A P-256 point is checked to be on the curve, both by the type's rule
  and by `p256` again before it is used, each coordinate 32 octets in
  full (RFC 7518 §6.2.1.2).

A `Jwk` is shown by its key id, else its `x`, else its `n`.

Members a `Jwk` doesn't hold are ignored, as RFC 7517 §4 requires
(Microsoft's key set sends `issuer` and `cloud_instance_name`, and
Microsoft's and Auth0's send `x5t`). It is the one standard object that
ignores them. But a private key's or a shared secret's members (`d`, `p`,
`q`, `dp`, `dq`, `qi`, `oth`, `k`) make JSON not a `Jwk`. So a key set
read from outside can't put a secret in a program. A key set whose keys
don't all read is `mismatch` as a whole, as any list is.

`key_ops` (RFC 7517 §4.3), which WebCrypto's `exportKey("jwk")` always
writes, is held as `key_ops: Set<Text>`, or `none` when the JSON leaves it
out. A key whose `key_ops` leaves out `"verify"` can't check a signature:
`crypto.verify_jwt` doesn't try it, as with a key for another algorithm.
Each operation is 1 to 64 printable ASCII characters, and one named twice
makes JSON not a `Jwk`, as RFC 7517 §4.3 forbids.

A `Jwk`'s `x5c` is carried, and held to the key. It is the certificates'
DER in standard Base64, as RFC 7517 §4.7 sends them, read with
`encoding.from_base64` and `crypto.read_certificate`. A key with
certificates is tried only when its first certificate reads strictly and
holds the same public key (RFC 7517 §4.7's MUST). Nothing checks the
chain, which `x5c` alone doesn't make trusted. `x5u`, `x5t` and a
header's `x5c` are never read.

**`JwtClaims`** holds RFC 7519's seven registered claims by their own
names: `aud` is a list (one audience is read as a list of one), `exp` is
always there, and each text claim is one line of 1 to 1,024 characters.
It is sent with those names and Jadpo's instants, not numeric dates, and
leaves out the claims it doesn't have. It is stored in a column per
claim, and shown by its subject, else its issuer, else its id, else its
expiry. A token's other claims (`scope`, `email`) are read with `as:`.

**`OAuthScope`** is one RFC 6749 scope-token of 1 to 256 characters.
The enum [Policy](09-policy.md) generates from the scopes
`authentication.jadpo` declares narrows this type to them, as a closed
subset, rather than reusing its name.

**`OidcClaims`** holds OpenID Connect Core's subject (1 to 255 printable
ASCII characters) and the claims a sign-in reads: `name` and
`preferred_username` as lines, `given_name` and `family_name` as the
standard types, `email`, `email_verified`, `phone_number`,
`phone_number_verified` and `picture`. `gender` stays out, by the rule
in [The standard types](#the-standard-types). `birthdate`, `address`,
`locale`, `zoneinfo` and `updated_at` aren't held, since a sign-in
doesn't need them. JSON naming a claim it doesn't hold isn't an
`OidcClaims`, as for every object. It is shown by its name, else its
email, else its subject.

> `crypto.verify_jwt` and `crypto.read_certificate` are built, and read
> and check Google's, Microsoft's and Auth0's key sets and tokens. They
> are tested against RFC 7515's, RFC 7519's, RFC 7520's and RFC 8037's
> examples and the classic attacks under each algorithm. Neither reads
> randomness, and both build for WebAssembly. Both only verify, so
> RUSTSEC-2023-0071 (Marvin), which leaks an RSA private key through the
> timing of decryption and signing, doesn't touch them. RS384, RS512,
> PS256, ES384 and the other algorithms aren't built, and neither is
> `OAuthScope`.

## Secret values
<!-- topic: secret-values -->

A secret value is a password, key, token or code that your app receives
and checks, but never sends, logs, stores or shows. It arrives in a
request's `Input`, or through `text.parse` or `json.read`, and the
compiler keeps it from going any further. There are seven:

| Type | What it holds |
|---|---|
| `Password` | 8 to 256 characters with no control character; any other Unicode and spaces are allowed. 8 is NIST SP 800-63B's minimum. Raise it with `min_length:`; NIST asks for 15 where a password is the only factor |
| `ApiKey` | 1 to 512 printable ASCII characters with no spaces, whatever prefix the provider uses |
| `BearerToken` | RFC 6750's `b64token`, at most 8,192 characters |
| `Jwt` | A JSON Web Token, which `crypto.verify_jwt` checks. Its text is RFC 6750's `b64token`, at most 8,192 characters |
| `TotpSecret` | RFC 4648 Base32 in upper case without padding, in its one spelling, holding 16 to 64 bytes (at least RFC 4226's 128 bits) |
| `SecretText` | 1 to 8,192 characters of anything, line breaks included, so it has room for a PEM key |
| `OneTimeCode` | The code a person types: six to eight ASCII digits (at least RFC 4226's six, at most RFC 6238's eight), a leading zero kept |

None of them is ordered: your code can't compare one with `<` or sort
by it. Sets and maps still keep secrets in an order of their own, which
[Comparing secrets](#comparing-secrets) describes.

Here a sign-up form takes a password, which goes no further than its
hash, and a partner's API key is recognised by what is kept of it:

```jadpo check project=secret-values
type MemberHandle = Text {
    max_length: 40
}

type ChosenPassword = Password {
    min_length: 15
}

/// What the sign-up form sends: the password arrives, and goes no further
/// than its hash.
type SignUpForm = Input {
    handle: MemberHandle
    chosen_password: ChosenPassword
}

type KeyVerifier = Bytes {}

type Recognised = Bool {}

/// The hash a new member's password is kept as.
action password_to_keep(sign_up: SignUpForm) -> PasswordHash {
    return crypto.hash_password(sign_up.chosen_password)
}

/// What is kept of a partner's API key: its SHA-256, which signs no one
/// in.
function key_verifier(partner_key: ApiKey) -> KeyVerifier {
    return KeyVerifier(hash.sha256(partner_key))
}

/// Whether a key a partner sends is the one it was issued.
function recognised(offered_key: ApiKey, kept_verifier: KeyVerifier) -> Recognised {
    return Recognised(key_verifier(offered_key) == kept_verifier)
}
```

```jadpo check project=secret-values file=main.test.jadpo
test "a password is kept as its hash" {
    var sign_up: SignUpForm = SignUpForm {
        handle: "ada"
        chosen_password: "correct horse battery staple"
    }
    var kept_hash: PasswordHash = password_to_keep(sign_up)
    assert crypto.verify_password(sign_up.chosen_password, kept_hash) == PasswordCheck.correct
}

test "a partner's key is recognised by its verifier" {
    var kept_verifier: KeyVerifier = key_verifier("example_key_0123456789abcdef")
    assert recognised("example_key_0123456789abcdef", kept_verifier)
    assert not recognised("example_key_0123456789abcdeX", kept_verifier)
}
```

### Where a secret can go

A secret stays on the server. An `Input`, an `Object`, a variable and a
parameter may hold one, and `json.read` reads one, since none of them
leaves it.

Nothing that leaves the server may hold one, however deeply it is
nested in lists, maps, options, objects, `Input`s or a variant's data.
The compiler refuses a secret in each of these with
`type.secret_exposed`:

- an `Output`;
- a failure's `client` or `internal` fields. A failure's fields are
  logged, so this keeps secrets out of the logs too;
- an event, which is kept in the outbox and handed on;
- an email or a message.

`json.write` can't write one either (`type.mismatch`). A `PasswordHash`
has a rule of its own, `type.password_hash_exposed`, because a hash,
unlike a secret, is stored.

### How a secret is kept

A secret is never stored as itself. Each kind is kept as something that
does the same job:

- A password is kept as its `PasswordHash`.
- An API key or a bearer token is kept as `hash.sha256` of it, as
  [Identity](08-identity.md#api-tokens) keeps a token's verifier.
- A `Jwt` is checked when it arrives and never kept.
- A one-time code is kept only as the time it was used.
- A `TotpSecret`, which checking a code needs back, and a `SecretText`
  are stored sealed with an encrypted field's key
  ([Encrypted fields](15-personal-data.md#encrypted-fields)).

An entity's field that holds any other secret is `type.secret_exposed`,
and the error's `next:` line names what to keep instead. Keeping the
password itself is refused:

```jadpo check project=e-password-stored expect=type.secret_exposed
type MemberHandle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: MemberHandle {
        unique: true
    }
    chosen_password: Password
}
```

```text output
error: `Member.chosen_password` keeps a `Password`
 --> main.jadpo:10:22
   |
10 |     chosen_password: Password
   |                      ^^^^^^^^ a secret, stored
  reason: A `Password` is never stored as itself, so a copy of the database holds none. A password is hashed with `crypto.hash_password` and checked with `crypto.verify_password`.
  next: Store `crypto.hash_password(...)` of it in a `PasswordHash` field in `chosen_password`'s place (agent)
  rule: type.secret_exposed
  help: https://jadpo.dev/docs/errors/type.secret_exposed

check failed: 1 error, 1 file
```

### What may read a secret

Only the operation made for a secret reads it:

- `crypto.hash_password` and `crypto.verify_password` take a `Password`.
- `crypto.verify_jwt` takes a `Jwt`.
- `hash.sha256` takes an `ApiKey` or a `BearerToken`. It never takes a
  `Password`, whose only hash is Argon2id's, nor a `OneTimeCode`, since
  the digest of a six-digit code is found in a million tries.

Everything else that would read a secret is `type.secret_exposed`, so no
length, part, text or format for people can carry one out:

- any other operation, one over a collection of secrets included, so
  `text.join` can't join them;
- any operator but `==` and `!=`, since an order or `matches` would
  answer questions about the text;
- `text.from` and `text.format`;
- building any type except one built on the same secret.

Building a type that is built on it is allowed, as in
`StaffPassword(password)`.

### Comparing secrets

`==` and `!=` on secrets take a time that depends on the two values'
lengths alone, so how long a comparison takes says nothing of where two
values differ. Every byte up to the longer value's length is compared,
and the answer is decided only once all of them have been, so the
comparison never stops early. This is how
[Identity](08-identity.md#api-tokens) compares its verifiers, and a
secret compared with text written in source is compared the same way.

A set or a map orders secrets by a comparison that also looks at every
byte.

A secret value equals its text wherever the two meet, in time that
depends on the lengths alone: inside a list, a set, a map's keys and
values, an object and an enum's data, however deep
(`[p] == ["hunter2hunter2"]`). In the order sets and maps keep, a
secret value sits among text values, ordered by its text. So
`set.contains` finds a secret by its text, and finds text by an equal
secret, and that order always agrees with `==`.

### Reports show a secret redacted

Wherever Jadpo shows a value, it shows a secret as `<redacted ApiKey>`,
naming its type: a failed test's values, `jadpo test`'s output and its
`--json`, a failure's report, a fault, and anything a debugger or
`jadpo watch` shows. Every one of them writes a value the same way.

A test that fails over a secret shows it redacted wherever it shows a
value, and its diff still says which field differs:

```jadpo check project=secret-redacted
type PartnerName = Text {
    max_length: 100
}

type PartnerLogin = Object {
    name: PartnerName
    partner_key: ApiKey
}

/// A partner, signed in with the key it sent.
function signed_in(name: PartnerName, offered_key: ApiKey) -> PartnerLogin {
    return PartnerLogin {
        name: name
        partner_key: offered_key
    }
}
```

```jadpo check project=secret-redacted file=main.test.jadpo expect=test.assertion_failed
test "a partner keeps the key it signed in with" {
    var issued_key: ApiKey = "example_key_0123456789abcdef"
    assert signed_in("Northwind", issued_key) == PartnerLogin {
        name: "Northwind"
        partner_key: "example_key_9876543210fedcba"
    }
}
```

```text output
FAIL  a partner keeps the key it signed in with
 --> main.test.jadpo:3

  assert signed_in("Northwind", issued_key) == PartnerLogin { ...
         |                      |              |
         |                      |              PartnerLogin { name: "Northwind", partner_key: <redacted ApiKey> }
         |                      <redacted ApiKey>
         PartnerLogin { name: "Northwind", partner_key: <redacted ApiKey> }

  expected: PartnerLogin { name: "Northwind", partner_key: <redacted ApiKey> }
  actual: PartnerLogin { name: "Northwind", partner_key: <redacted ApiKey> }
  diff: ~ partner_key: expected <redacted ApiKey>, actual <redacted ApiKey>

  rule: test.assertion_failed
  rerun: jadpo test main.test.jadpo:3 --seed 1

test failed: 1 failed, 1 pure test
```

### A setting that holds a secret

A setting whose type is a secret value follows its `secret:` flag, as
every setting does ([Secrets](10-configuration-and-time.md#secrets)).
With `secret: true`, it goes only to places the compiler owns. Without
it, your code reads it as a secret value, which the rules above keep in
and show redacted.

### Recordings for replay

A recording of a request, kept so it can be replayed
([What the graph and production evidence add](17-debugging-and-review.md#what-the-graph-and-production-evidence-add)),
holds no credential the runtime made. The bytes the runtime makes a
session, an API token, a service key or a CSRF token from are never kept
on a recording's tape, nor taken from a replayed one, so a replay makes
fresh ones. The program's own [`random`](#randomness) draws still replay
in order.

No `crypto.encrypt` nonce is kept either. It is drawn as a credential's
bytes are, so a replay never seals with a nonce that a production run
used under the same key. A password's salt isn't secret and stays on the
tape, so a replayed `hash_password` gives the hash the recorded run made.

### OpenAPI

OpenAPI describes a secret value as a `writeOnly` string, and a
`Password` with `format: password` as well, so generated clients hide it
as it is typed.

> Not built yet: storing a `TotpSecret` or a `SecretText` needs keys held
> as data ([Language gaps this page depends on](#language-gaps-this-page-depends-on)).
> Until that is built, storing one is `toolchain.not_built_yet` rather
> than refused. No operation reads a `OneTimeCode` yet: `crypto.verify_totp`
> waits for keys held as data. `jadpo replay`, which the recordings above
> are for, isn't built; when it is, it keeps out the draws a secret value
> was made from.

## Security records
<!-- topic: security-records -->

These are the types for what security code keeps and checks: digests,
signatures and sealed values, a password policy, passkeys and
certificates.

### `Hash`, `Hmac` and `EncryptedValue`

These three hold the bytes `hash.sha256`, `crypto.hmac` and
`crypto.encrypt` give, as types of their own, so a digest can't be passed
where a signature is expected. An `EncryptedValue` is ciphertext, so it
may be stored and sent; only `crypto.decrypt`, with the key, reads what it
holds.

Each is built on `Bytes`. The operations still give `Bytes`, and
building each type from its own call is proved while compiling, so
`Hash(hash.sha256(x))` needs no `attempt` arm. Any other bytes are
checked as they are built, so `Hmac(hash.sha256(x))` and bytes from
anywhere else need one. `crypto.decrypt` takes an `EncryptedValue` as the
`Bytes` it is.

Each type checks its bytes, and bytes that don't fit break `format`:

- A `Hash` is a SHA-256 digest's 32 bytes.
- An `Hmac` is an HMAC-SHA-256 tag's 32 bytes.
- An `EncryptedValue` is the sealed format: its version (1) and its
  algorithm (1, XChaCha20-Poly1305), then room for its key identity,
  nonce and tag, so at least 46 bytes, with no most. Whether a sealed
  value opens is for `crypto.decrypt` to say, with the key.

Their text carries no algorithm tag. Each type has one algorithm
([Hashing and cryptography](#hashing-and-cryptography) makes no
algorithm choices), and a sealed value carries its own in its second
byte. Their canonical text and their JSON are the Base64 of the bytes,
standard alphabet, padded, in one spelling, which is how every `Bytes`
value is sent. So a field that was `Bytes {}` becomes a `Hash` with no
change on the wire. To write a digest as hexadecimal, as for an ETag,
use `encoding.to_hex`. Each is stored as `bytea`, shown to people as its
Base64, and not ordered.

Every `Bytes` value compares in constant time, not only these three:
`==` looks at every byte of two values of one length, whatever the first
difference, inside records and lists too, and the canonical order a set
keeps does the same. So comparing a received signature with the one
expected says nothing of where they differ.

A clinic keeps what it needs to recognise a partner's API key, signs the
webhooks it sends, and keeps each referral letter sealed to its record:

```jadpo check project=security-records file=config.jadpo
config {
    letters_key: EncryptionKey
    outbound_webhook_secret: SigningKey
}
```

```jadpo check project=security-records
type ReferralText = Text {
    max_length: 4000
}

type ReferralId = Text {}

type PartnerKeyDigest = Hash {}

type OutboundSignature = Hmac {}

type KeyRecognised = Bool {}

type ReferralUnreadable = Failure {
    kind: FailureKind.invalid_value
}

/// What is kept of a partner's API key: its SHA-256, which signs no one
/// in.
function partner_key_digest(partner_key: ApiKey) -> PartnerKeyDigest {
    return PartnerKeyDigest(hash.sha256(partner_key))
}

/// Whether a key a partner sends is the one it was issued; the digests
/// compare in constant time.
function partner_key_recognised(
    offered_key: ApiKey,
    kept_digest: PartnerKeyDigest
) -> KeyRecognised {
    return KeyRecognised(partner_key_digest(offered_key) == kept_digest)
}

/// The signature on a webhook the clinic sends.
function outbound_signature(outgoing_body: ReferralText) -> OutboundSignature {
    return OutboundSignature(crypto.hmac(outgoing_body, key: config.outbound_webhook_secret))
}

/// A referral letter sealed to its own record, which may be stored and
/// sent as it is.
action sealed_referral(referral_letter: ReferralText, referral: ReferralId) -> EncryptedValue {
    return EncryptedValue(
        crypto.encrypt(referral_letter, key: config.letters_key, context: referral)
    )
}

/// A sealed referral's text, for its own record only.
function opened_referral(
    kept_letter: EncryptedValue,
    referral: ReferralId
) fails ReferralUnreadable -> ReferralText {
    var letter_bytes: Bytes = attempt crypto.decrypt(
        kept_letter,
        key: config.letters_key,
        context: referral
    ) {
        unreadable: ReferralUnreadable
    }
    var letter_text: Text = attempt encoding.from_utf8(letter_bytes) {
        malformed: ReferralUnreadable
    }
    return attempt ReferralText(letter_text) {
        max_length: ReferralUnreadable
    }
}
```

The digest of `abc` is FIPS 180-4's own example:

```jadpo check project=security-records file=main.test.jadpo
test "a partner's key is recognised by its digest" {
    var kept_digest: PartnerKeyDigest = partner_key_digest("example_key_0123456789abcdef")
    assert partner_key_recognised("example_key_0123456789abcdef", kept_digest)
    assert not partner_key_recognised("example_key_0123456789abcdeX", kept_digest)
    assert text.from(partner_key_digest("abc")) == "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0="
}

test "a sealed referral opens only for its own record" {
    var kept_letter: EncryptedValue = sealed_referral("Please see Ms Okafor.", "referral-7")
    var read_back: ReferralText = attempt opened_referral(kept_letter, "referral-7")
    assert read_back == "Please see Ms Okafor."
    assert opened_referral(kept_letter, "referral-8") fails ReferralUnreadable
}
```

### `PasswordPolicy`

A `PasswordPolicy` holds the rules your app sets for a password, as NIST
SP 800-63B describes them: the fewest and most characters, and whether a
new password is checked against a list of breached ones. It has no
composition rules, no expiry and no hints, which NIST says not to
impose.

- `min_length` is from NIST's 8 (15 where a password is the only factor)
  to a `Password`'s 256.
- `max_length` is from NIST's "at least 64" to 256.
- The fewest is never more than the most. This is proved on a literal,
  and checked as JSON is read.
- `check_breached` records whether your app checks a new password against
  a list of breached ones. NIST says a new password SHALL be compared
  against such a list, but Jadpo has no list and no operation for it.

It is sent as an object of its parts, stored in a column per part, and
shown as its lengths, `15–64`.

### `WebAuthnCredential`

A `WebAuthnCredential` is a passkey as the server keeps it
([Passkeys](08-identity.md#passkeys)): its credential ID, its public key,
its signature counter, its transports, whether it may be and is backed
up, the name the person gave it, and when it was made and last used. It
never holds a biometric.

| Part | `webauthn-rs` | What it holds |
|---|---|---|
| `credential_id` | `cred_id` | 1 to 1,023 bytes (WebAuthn Level 3) |
| `public_key` | `cred`, a `COSEKey` | The COSE_Key (RFC 9052 §7) in CBOR, 1 to 2,048 bytes, written as its labels (`1` the key type, `3` the algorithm, `-1` the curve or RSA's `n`, `-2` `x` or RSA's `e`, `-3` `y`), which `webauthn-rs`'s own `COSEKey::try_from` reads back |
| `sign_count` | `counter` | 0 to 4,294,967,295, a `u32` |
| `transports` | `transports` | `usb`, `nfc`, `ble`, `internal`, `hybrid`; an empty list is `None`; `smart-card` and `webauthn-rs`'s own `test` aren't kept |
| `backup_eligible` | `backup_eligible` | Whether it may be backed up or synced |
| `backed_up` | `backup_state` | Whether it is |
| `name` | none | The person's name for it, one line |
| `registered_at`, `last_used_at` | none | When it was registered, and last used (`none` until then) |

It is sent with its parts' names, its bytes as Base64 and its count as
any `Int` is sent, stored in a column per part, and shown by its name.
Each `Bytes` part is bounded by its length.

The `webauthn-rs` column names the part of `webauthn-rs`'s stored
passkey that each part matches: `webauthn-rs` is the reference the
runtime's passkey ceremonies are held to. What `webauthn-rs` keeps
besides is the same for every passkey Jadpo registers, so it isn't held:

- `user_verified` is true and `registration_policy` is `required`, since
  a passkey is always verified;
- `extensions` is `RegisteredExtensions::none()`;
- attestation is never requested, so `attestation` is
  `ParsedAttestation::default()` and `attestation_format` is `None`.

Checking a passkey's answer reads a stored credential's key, counter,
`user_verified`, `registration_policy` and backup flags, and its
`transports` when it lists the credentials allowed, never its
`extensions`, so leaving them out changes nothing.

### `Certificate`

A `Certificate` is an X.509 certificate (RFC 5280), read from its PEM
(RFC 7468). Use it to read a certificate a partner sends, pin it by its
fingerprint and check its dates. Its parts are:

- `subject` and `issuer`, each a distinguished name as RFC 4514 writes
  it;
- `serial_number`;
- `not_before` and `not_after`, the first and last instants it is valid
  at;
- `fingerprint`, the SHA-256 of its DER, as a `Hash`;
- `der`, the DER itself, so a program can pass it on.

There is no common-name part; read the subject instead.

Each part is read from what the certificate's issuer signed, so code
reads the parts and never sets them. An object literal of a
`Certificate` is `type.constructor_input`, and because
[values never change](02-values-and-types.md#values-never-change),
nothing sets a part later.

Nothing here checks a certificate's signature, its chain, its extensions
or whether it was revoked. A certificate isn't secret. A key's PEM is
never a certificate, and reading one fails `malformed`.

A clinic receives lab results from a partner, which sends the certificate
it signs them with. The clinic pins that certificate by its fingerprint
and checks it is in date:

```jadpo check project=certificates
type PartnerPem = Text {
    max_length: 100000
}

type CertificateRefused = Failure {
    kind: FailureKind.invalid_value
}

type CheckedAt = Instant {}

type CertificateHolds = Bool {}

/// The certificate a partner sent, read from its PEM.
function partner_certificate(sent_pem: PartnerPem) fails CertificateRefused -> Certificate {
    return attempt text.parse(sent_pem, as: Certificate) {
        malformed: CertificateRefused
    }
}

/// Whether a certificate is the one pinned for the partner: its SHA-256
/// fingerprint, compared in constant time.
function is_pinned(presented: Certificate, pinned_fingerprint: Hash) -> CertificateHolds {
    return CertificateHolds(presented.fingerprint == pinned_fingerprint)
}

/// Whether a certificate is in date: its first and last instants are both
/// included.
function in_date(presented: Certificate, checked_at: CheckedAt) -> CertificateHolds {
    return CertificateHolds(
        presented.not_before <= checked_at and checked_at <= presented.not_after
    )
}
```

```jadpo check project=certificates file=main.test.jadpo
test "a partner's certificate is read, pinned and dated" {
    var lab_certificate: Certificate = attempt partner_certificate(
        "-----BEGIN CERTIFICATE-----\nMIIBljCCAUigAwIBAgIIHy49TFtqeYgwBQYDK2VwMEgxCzAJBgNVBAYTAkdCMRww\nGgYDVQQKDBNKYWRwbyBUZXN0LCBTaWduaW5nMRswGQYDVQQDDBJqYWRwby10ZXN0\nLmV4YW1wbGUwHhcNMjYxMDAxMDAwMDAwWhcNMzYxMDAxMDAwMDAwWjBIMQswCQYD\nVQQGEwJHQjEcMBoGA1UECgwTSmFkcG8gVGVzdCwgU2lnbmluZzEbMBkGA1UEAwwS\namFkcG8tdGVzdC5leGFtcGxlMCowBQYDK2VwAyEAAJyTOxfkgCmTOYhl/AXEMRpT\n07QqDzpBZIRCSESxhYejUDBOMB0GA1UdDgQWBBTH4Arltue6aEupl+oqXRoIZos6\nLTAfBgNVHSMEGDAWgBTH4Arltue6aEupl+oqXRoIZos6LTAMBgNVHRMBAf8EAjAA\nMAUGAytlcANBAHkV9ilHQ5TVpSZyPobRcjo9bERYbugOAsKXD4ZJjCRPqpyK+aDc\n+FxV2FFnXI2UQxnYsieWNSclooiCSIuDmwA=\n-----END CERTIFICATE-----\n"
    )
    assert lab_certificate.subject == "CN=jadpo-test.example,O=Jadpo Test\\, Signing,C=GB"
    assert lab_certificate.issuer == lab_certificate.subject
    assert lab_certificate.serial_number == "1f2e3d4c5b6a7988"
    var pinned: Hash = lab_certificate.fingerprint
    assert text.from(pinned) == "18Ma3EAjFJeXynaXWJODAHbnCHr90gDsttEzYJqXWg0="
    assert is_pinned(lab_certificate, pinned)
    assert in_date(lab_certificate, "2026-10-18T09:00:00Z")
    assert in_date(lab_certificate, "2036-10-01T00:00:00Z")
    assert not in_date(lab_certificate, "2036-10-01T00:00:01Z")
    assert partner_certificate(
        "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAGb9ECWmEzf6FQbrBZ9w7lshQhqowtrbLDFw4rXAxZuE=\n-----END PUBLIC KEY-----\n"
    ) fails CertificateRefused
}
```

Building a certificate from parts chosen in code is refused, since
nothing would have signed them:

```jadpo check project=e-certificate-made-up expect=type.constructor_input
type CertificateDer = Bytes {}

type IssuedAt = Instant {}

/// A certificate made up in code, its parts chosen rather than signed.
function made_up(
    chosen_der: CertificateDer,
    chosen_digest: Hash,
    issued_at: IssuedAt
) -> Certificate {
    return Certificate {
        subject: "CN=bank.example"
        issuer: "CN=Trusted Root"
        serial_number: "01"
        not_before: issued_at
        not_after: issued_at
        fingerprint: chosen_digest
        der: chosen_der
    }
}
```

```text output
error: `Certificate` isn't built from its parts
 --> main.jadpo:11:12
   |
11 |     return Certificate {
   |            ^^^^^^^^^^^ parts set in code
  reason: A `Certificate`'s subject, issuer, serial number, validity and fingerprint are read from the DER its issuer signed, so code reads them and never sets them.
  next: Read the `Certificate` from its PEM with `text.parse(..., as: Certificate)`, or from JSON with `json.read` (agent)
  rule: type.constructor_input
  help: https://jadpo.dev/docs/errors/type.constructor_input

check failed: 1 error, 1 file
```

#### How a certificate is read

- **Its text and its JSON are its PEM,** in RFC 7468's strict form: one
  `CERTIFICATE` block, its DER in standard Base64 lines of 64
  characters, the last line 1 to 64, each line ending in a line feed. It
  is read with line feeds, or with a carriage return and a line feed
  throughout, and the last line's ending is optional. Nothing else is
  read: text before or after the block, headers, white space, a second
  block and any other label, `TRUSTED CERTIFICATE` and every key's
  included, are each `malformed`. Its JSON is that text, as for every
  type with a canonical text, not its DER in Base64. It is stored in a
  column per part.
- **Only DER is read,** at most 64 KiB, whether it is inside a PEM or
  read from bytes by `crypto.read_certificate`: each length definite and
  in its fewest octets, no tag number of 31 or more, each INTEGER in its
  fewest octets, a BOOLEAN all ones or all zeros, a BIT STRING's unused
  bits zero, each arc of an OBJECT IDENTIFIER in its fewest octets, no
  universal type constructed but a SEQUENCE or a SET, each time in RFC
  5280's form (`YYMMDDHHMMSSZ`, `YYYYMMDDHHMMSSZ`), and nothing after
  the certificate. The algorithm outside what was signed must be the one
  inside (RFC 5280 §4.1.1.2), each compared as its DER.
- **The serial number** is positive and at most 20 octets, its sign
  octet included (RFC 5280 §4.1.2.2), so a negative or a zero one is
  `malformed`. It is written in lowercase hexadecimal, two digits an
  octet, without the sign octet and with no separators:
  `8210cfb0d240e3594463e0bb63828b00` for ISRG Root X1.
- **Names are written as RFC 4514 writes them:** the relative names last
  first, joined by `,`, and the attributes of one joined by `+`. The
  nine types RFC 4514 §3 names go by their short names (`CN`, `L`, `ST`,
  `O`, `OU`, `C`, `STREET`, `DC`, `UID`), and any other type by its
  dotted OID. A value of a known string kind (UTF8String,
  PrintableString, IA5String, NumericString, VisibleString, BMPString,
  UniversalString) is written as its text, escaped as §2.4 says and with
  every control character as `\` and its hexadecimal, so a name shows on
  one line. A TeletexString, any other kind and any type without a short
  name is written as `#` and the hexadecimal of its DER. A string that
  isn't its kind's, such as UTF-8 that isn't valid, makes the
  certificate `malformed`.
- **Validity** is two instants, both included (RFC 5280 §4.1.2.5).
  `99991231235959Z`, which RFC 5280 writes for a certificate with no
  well-defined expiry, is past the latest instant Jadpo holds, so a time
  in the last hours of 9999 reads as that latest instant. A certificate
  whose last instant is before its first is read, and is valid at no
  instant.
- **It is shown by its subject,** or by its serial number when the
  subject is empty: RFC 5280 lets a certificate whose names are all in
  its extensions have an empty subject. It is not ordered, and `==`
  compares every part, the bytes in constant time.

## Randomness
<!-- topic: randomness -->

`random` gives your app random tokens, codes, whole numbers, shuffles and
samples. In production it reads the operating system's cryptographic
generator. In a test it gives the same values on every run, as a test's
clock is fixed, so a test can name the very values it expects.

`random` is a generated namespace, like `clock`, and only actions, tests
and fixtures call it. A function is
pure, so calling `random` from one is `type.random_context`. Every value
it hands out is recorded for replay, since randomness is one of the
effects a replayed request depends on
([What the graph and production evidence add](17-debugging-and-review.md#what-the-graph-and-production-evidence-add)).

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `random.token()` | `Text`, 32 random bytes as Base64url, 43 characters: for links and keys a person never types | | none |
| `random.code(length)` | `Text` of `length` characters from Crockford's Base32 (digits and capitals without `I`, `L`, `O`, `U`): booking and order references a person reads out; `length` 4 to 32 | | none |
| `random.integer(min, max)` | `Int`, uniformly from `min` to `max`, both included; `min` at most `max` | | none |
| `random.shuffle(items)` | `List<T>`, the items in random order | | none |
| `random.sample(items, count)` | `List<T>`, `count` items from different positions, in random order; all of them when `count` is larger | | none |

A shop gives each order a reference a customer reads out on the phone and
a cancel link no one types; a board game throws a die; a quiz asks its
rounds in a random order and a prize draw picks its winners:

```jadpo check project=randomness
type OrderReference = Text {}

type CancelToken = Text {}

type DiceRoll = Int {
    min: 1
    max: 6
}

type Entrant = Text {
    max_length: 40
}

type WinnerCount = Int {
    min: 0
    max: 10
}

/// An order's reference, which a customer reads out on the phone.
action new_order_reference() -> OrderReference {
    return OrderReference(random.code(8))
}

/// The token in a booking's cancel link, which no one types.
action new_cancel_token() -> CancelToken {
    return CancelToken(random.token())
}

/// A die's throw in a board game.
action throw_die() -> DiceRoll {
    return DiceRoll(random.integer(1, 6))
}

/// The order a quiz asks its rounds in.
action round_order(quiz_rounds: List<Entrant>) -> List<Entrant> {
    return random.shuffle(quiz_rounds)
}

/// A prize draw's winners, each entrant at most once.
action prize_winners(entrants: List<Entrant>, count: WinnerCount) -> List<Entrant> {
    return random.sample(entrants, count)
}
```

```jadpo check project=randomness file=main.test.jadpo
test "an order's reference is the same on every run" {
    var reference: OrderReference = new_order_reference()
    assert reference == "QHSF1J29"
    assert text.length(new_cancel_token()) == 43
}

test "a prize draw picks different entrants" {
    var winners: List<Entrant> = prize_winners(["Ada", "Grace", "Alan", "Edsger"], 2)
    assert winners == ["Alan", "Edsger"]
    assert list.count(round_order(["Ada", "Grace", "Alan"])) == 3
}
```

### Every value is as likely as every other

- A token is 32 bytes as Base64url without padding.
- A code's character is the low five bits of its own byte, so each of
  the 32 characters is as likely.
- A whole number is drawn by rejection: a 64-bit draw below 2^64 mod the
  count is drawn again, then the remainder is taken, so the low numbers
  aren't favoured. The whole range of `Int` is a draw itself.
- A shuffle is Fisher and Yates's. A sample is the first `count` steps of
  one, so its items are in random order too.

### Where the random bytes come from

Every draw takes its bytes from one source, the invocation's context: a
replayed run's recorded bytes first, then the host's generator, then the
test's seed. With none of them, the call is a fault (`internal_fault`),
never a weaker value. The native host reads the operating system's
generator (getrandom(2), getentropy), and the Workers host reads the Web
Crypto API's.

### The same values on every test run

A test's seed is its own, made from the test's key alone, not from the
run. So every run gives a test the same values, whatever seed orders the
run, and a test can name the very code it expects. New identities share
the seed, so they don't change with the order seed either. `--seed`
decides only the order tests run in.

### Replaying a recorded run

While a run is recorded, every random byte handed out is kept on its
tape. A replayed run is given them back in order, so it makes the same
tokens, codes and shuffles; when they run out there is no randomness to
give. The bytes the runtime makes credentials from are kept off the tape
([Recordings for replay](#recordings-for-replay)).

### What the checker proves

`random.integer(1, 6)` is proved to give 1 to 6 for the type it builds,
so `DiceRoll(random.integer(1, 6))` needs no `attempt` arm. The proof
comes from the arguments' bounds, not their order: `min` above `max` is a
defect where it runs, as overflow is. The checker proves a code's
`length` from 4 to 32 and a sample's `count` not negative, as it does
`temporal.occurrences`' `count` and `list.slice`'s.

> Not built yet: `jadpo replay` itself
> ([Debugging and review](17-debugging-and-review.md)). Since the
> recorded bytes can hold tokens, nothing writes them out until it is.

## Encodings
<!-- topic: encodings -->

The `encoding` operations turn bytes into text and back: Base64 and
Base64url for bytes sent as text, hexadecimal for digests, UTF-8 for text
as bytes, and percent-encoding for a part of a link. Each is called as
`encoding.<name>`.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `to_base64(bytes)` | `Text`, standard alphabet, padded | | `translate(encode(x, 'base64'), E'\n', '')` |
| `from_base64(text)` | `Bytes` | `malformed` | none |
| `to_base64url(bytes)` | `Text`, URL alphabet, unpadded, as [Identity](08-identity.md)'s signed parts are | | `rtrim(translate(encode(x, 'base64'), E'+/\n', '-_'), '=')` |
| `from_base64url(text)` | `Bytes`; padding refused | `malformed` | none |
| `to_hex(bytes)` | `Text`, lowercase | | `encode(x, 'hex')` |
| `from_hex(text)` | `Bytes`; either case | `malformed` | none |
| `to_utf8(text)` | `Bytes` | | `convert_to(x, 'UTF8')` |
| `from_utf8(bytes)` | `Text`; invalid UTF-8 refused, never replaced | `malformed` | none |
| `to_url_component(text)` | `Text` with everything but RFC 3986's unreserved characters percent-encoded as UTF-8 | | none |

Writing never fails. Reading fails `malformed` for text that isn't the
encoding at all, and reads only the one form writing gives.

A bakery's photos, attachments and links go through `hash.sha256` and
each encoding. `hash.sha256` takes any value built on `Bytes`, such as
`PhotoBytes`, or on `Text`, whose UTF-8 it hashes
(`hash.sha256(email)`). A digest is `Bytes`, which `encoding` writes as
text:

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
    var sent_bytes: Bytes = attempt encoding.from_base64(sent) {
        malformed: AttachmentUnreadable
    }
    return attempt PhotoBytes(sent_bytes) {
        max_length: AttachmentTooLarge
    }
}

/// A photo sent back as Base64 text.
function sent_photo(photo: PhotoBytes) -> LinkPart {
    return LinkPart(encoding.to_base64(photo))
}

/// A photo's key read back as its 32 bytes, in either case.
function key_bytes(key: PhotoKey) fails AttachmentUnreadable, AttachmentTooLarge -> PhotoBytes {
    var sent_bytes: Bytes = attempt encoding.from_hex(key) {
        malformed: AttachmentUnreadable
    }
    return attempt PhotoBytes(sent_bytes) {
        max_length: AttachmentTooLarge
    }
}

/// A share code read back as the digest it was written from.
function code_digest(code: ShareCode) fails AttachmentUnreadable, AttachmentTooLarge -> PhotoBytes {
    var sent_bytes: Bytes = attempt encoding.from_base64url(code) {
        malformed: AttachmentUnreadable
    }
    return attempt PhotoBytes(sent_bytes) {
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

The values in the test are published ones: SHA-256 of "abc" is NIST's
FIPS 180-4 example, and the Base64 of "f", "fo" and "foo" are RFC 4648's
test vectors:

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

### `malformed`

`malformed` is the only condition a decoder fails on. No decoder can be
proved while compiling, so `malformed` is settled only by an `attempt`,
never by an option or a proof.

### One text for each value

What decodes reads only what encodes writes, so no two texts read as the
same bytes:

- `from_base64` takes standard, padded Base64 only, its length a
  multiple of four, and `from_base64url` takes unpadded Base64url only.
  Both refuse white space, the other alphabet's characters and bits after
  the last byte that aren't zero (RFC 4648, 3.5).
- A `Bytes` value on the wire is read and written by the same function,
  so a request, a response and `json.read` read bytes alike: a request
  whose Base64 isn't padded breaks `format`.
- `from_hex` takes an even number of digits in either case. `to_hex`
  writes lowercase, and `to_url_component`'s escapes are uppercase (RFC
  3986, 2.1).

### Links

There is no `from_url_component`: the runtime decodes paths and query
strings before your code sees them
([Paths and placeholders](07-http.md#paths-and-placeholders)).
`to_url_component` is for a link your code builds, such as an email's
sign-in link.

### No base32 and no compression

There is no base32 operation and no compression. gzip is read only
inside the file readers, with `max_unpacked_size:`
([Data formats](#data-formats-json-csv-tsv-and-json-lines)).

## Data formats: JSON, CSV, TSV and JSON Lines
<!-- topic: data-formats-json-csv-tsv-and-json-lines -->

Four namespaces turn data in a format into your app's types and back,
each in one shape. `json` reads and writes one document in memory, or a
large JSON list in a file. `csv`, `tsv` and `json_lines` read and write
files only.

These namespaces are pure: they turn bytes into your app's values and
back, wherever the bytes came from. Moving the bytes is a store's job,
with `store.read` and `store.put` ([Files](16-built-in-services.md#files)).

### Reading and writing a file's rows

A file's rows, however many, are read and written only through these
operations' batch forms. They run as
[batches](12-jobs-and-events.md#batches-work-over-many-items): background
work over many items that keeps its progress. There is no loop over a
file's rows and no reading a whole file into a list. How a run goes
(batches, transactions, resuming, `batches.status` and `batches.cancel`,
problems) is described with batches; the operations are here.

- A batch form is called only in an action. It starts after the action
  commits, as `emit` does, and returns a `BatchRun` at once. If the
  action rolls back, the run never starts.
- `key:` makes a repeated start return the same run.
- `each:` and `finished:` take an inline function, written with `->`
  ([Passing functions](03-logic.md#passing-functions)). Reading, `each:`
  calls one action per row. Writing, it is a pure expression that gives
  the row. The values they capture are saved with the run, so they may
  capture only values that can be stored.
- Writing takes the store to write to, the file's `name:`, and `from:`.
  `from:` is a call to a named query that returns a `Page`, walked in
  keyset pages ([Pages](06-reads-and-writes.md#pages)), or to a
  provider's list operation, walked by `page.next`
  ([Calling](11-integrations.md#calling)).
- `finished:` receives an `ImportSummary` after a read, and after a write
  an `ExportSummary`, which holds the stored `File`
  ([Other built-in types](02-values-and-types.md#other-built-in-types)).

### `csv`

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `csv.read(file, as:, each:, finished:, key:, max_unpacked_size:)` | `BatchRun`: each row of a stored `File` decoded as an `Input` of `as:` and handed to `each:`; `finished:` receives an `ImportSummary`; a gzip file unpacked up to `max_unpacked_size:` | Never as a call: a row that can't be decoded, or whose action fails, is a `RowProblem` and the rest carry on; a file-level fault (a missing required column, an unterminated quote named by the line where it opened, an encoding the format doesn't declare, text that isn't text) stops the run | none |
| `csv.write(store, name:, format:, from:, each:, finished:, key:)` | `BatchRun`: each item of `from:` turned into an `Output` row by `each:` and written to `name:` in `store`; `format:` a `CsvFormat`, required; `finished:` receives an `ExportSummary` | Never as a call: an item whose action fails is a `RowProblem` | none |

- **Rows are typed both ways.** A row read is an `Input`, and a row
  written is an `Output`, with their rules, problems and JSON encoding
  ([HTTP](07-http.md)). A column maps to a field by the field's name, or
  by `column: "Email address"`. Cells written in other conventions use
  the `wire:` encodings, such as `Wire.day_month_year`, or a pure
  `decode:` function. A CSV row holds single values only: a list or an
  object in one is a compile error that points you to JSON Lines.
- **A standard type in a cell is its canonical text,** as `text.from`
  writes it (`GBP 10.50`, `2.5 kg`, `2026-10-25T09:30:00Z`). It is read
  by `text.parse`, with its failures as the row's problems, so a cell
  keeps a currency, a unit and every decimal place. A standard type with
  no single text form (`PostalAddress`, `PersonName`, `Price`, a choice
  such as `Discount`) counts as an object here, so the compile error
  points to JSON Lines, whose rows carry the contract's JSON
  ([Every boundary through the contract](#every-boundary-through-the-contract)).
- **Writing names its reader.** `CsvFormat.spreadsheet` writes a
  byte-order mark and CRLF line endings, and escapes text that looks
  like a formula. `CsvFormat.standard` writes CRLF only. A write without
  `format:` is a compile error.
- **Encodings.** Reading takes UTF-8 (a byte-order mark removed), UTF-16
  with a mark, and legacy encodings the format declares, never guessing.
  Writing is UTF-8. gzip (`.csv.gz`) is read only with
  `max_unpacked_size:`, and no archives are opened.
  `max_unpacked_size:` is a size written as a `File` field's `max_size:`
  is (`200mb`), and every reader takes it. A file that isn't gzip is
  read as it is
  ([How a file's run is kept](12-jobs-and-events.md#how-a-files-run-is-kept)).
- **A `RowProblem`** holds the row as a spreadsheet numbers it (the
  header is row 1, the first data row row 2), the column, the field and
  the rule, never the value, so personal data stays out of problem lists
  and logs.
- **No CSV in memory.** A provider's CSV answer goes into a store and
  through `csv.read`, so there is one way to read rows.

How a row is written:

- **The header and the cells.** A CSV or TSV file starts with its
  header, the row type's columns in declared order, each named by its
  `column:` or by its field's name, then one line for each row. A cell is
  its field's text as a query string holds it, and is read back the same
  way: a standard type is its canonical text, an enum its variant's
  name, and `none` an empty cell.
- **Quoting.** A cell holding the delimiter, a double quote, a carriage
  return or a line feed is quoted, with its quotes doubled (RFC 4180),
  in TSV too. Every line, the last among them, ends with CRLF.
- **Who reads it.** `CsvFormat.spreadsheet` writes a UTF-8 byte-order
  mark first. It also puts `'` before a cell of a field built on `Text`
  that starts with `=`, `+`, `-`, `@`, a tab or a carriage return, so a
  spreadsheet never reads it as a formula. A number's cell, a negative
  one among them, is never changed. `CsvFormat.standard` does neither.
- **JSON.** A JSON Lines file is each row's JSON, as a response's body
  writes it (by its fields' names; `column:` names only a CSV column),
  with a line feed after each. `json.write`'s file form writes one
  compact list of the rows. An export of no items is its header alone,
  an empty file, or `[]`.
- **The file.** UTF-8, named by `name:`, its `FileType` `csv`, `tsv`,
  `json_lines` (`application/jsonl`) or `json`.

A bakery writes its price list for the shop's spreadsheet. `each:` turns
each loaf into a `PriceLine`, and `finished:` keeps the file it wrote:

```jadpo check project=csv-export file=app.jadpo
application {
    connections: {
        price_lists: {
            driver: Driver.disk
            path: "/var/lib/bakery/price-lists"
        }
    }
}
```

```jadpo check project=csv-export file=domains/bakery/types/loaf_code.jadpo hidden
/// A loaf's code on the bakery's lists.
type LoafCode = Text {
    min_length: 1
    max_length: 20
}
```

```jadpo check project=csv-export file=domains/bakery/types/loaf_price.jadpo hidden
/// What a loaf costs.
type LoafPrice = Decimal {
    scale: 2
    min: 0.00
}
```

```jadpo check project=csv-export file=domains/bakery/types/loaf_page.jadpo hidden
/// Where a walk over the loaves has got to.
type LoafPage = Input {
    page_size: PageSize {
        default: 100
    }
    after: PageCursor {
        optional: true
    }
}
```

```jadpo check project=csv-export file=domains/bakery/types/price_line.jadpo
use bakery/types { LoafCode, LoafPrice }

/// One line of the price list a spreadsheet reads.
type PriceLine = Output {
    code: LoafCode {
        column: "Loaf code"
    }
    price: LoafPrice {
        column: "Price"
    }
}
```

```jadpo check project=csv-export file=domains/bakery/entities/loaf.jadpo hidden
use bakery/types { LoafCode, LoafPrice }

/// A loaf the bakery sells.
entity Loaf {
    id: Uuid
    code: LoafCode
    price: LoafPrice

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}
```

```jadpo check project=csv-export file=domains/bakery/entities/price_list.jadpo hidden
/// A price list written for the shop, and how writing it went.
entity PriceList {
    id: Uuid
    list_file: File? {
        store: price_lists
        max_size: 20mb
        types: [FileType.csv]
    }
    list_run: BatchRun?
    loaves_listed: Int?

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.public, Access.system]
    }
}
```

```jadpo check project=csv-export file=domains/bakery/queries/loaves_by_id.jadpo hidden
use bakery/entities { Loaf }
use bakery/types { LoafPage }

/// Every loaf, a page at a time, in an order no change moves.
query loaves_by_id(page: LoafPage) -> Page<Loaf> {
    from: Loaf
    order: [Loaf.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
```

```jadpo check project=csv-export file=domains/bakery/actions/prices_listed.jadpo hidden
use bakery/entities { PriceList, PriceListNotFound }

/// Keeps the written price list, and how many loaves it lists.
action prices_listed(done: ExportSummary, list_id: PriceList.id) fails PriceListNotFound -> Unit {
    attempt PriceList.update(list_id) {
        set: {
            list_file: done.file
            loaves_listed: done.items
        }
    }
}
```

```jadpo check project=csv-export file=domains/bakery/actions/list_prices.jadpo migrate=initial
use app { price_lists }
use bakery/entities { PriceList, PriceListNotFound }
use bakery/types { LoafPage, PriceLine }
use bakery/actions { prices_listed }
use bakery/queries { loaves_by_id }

/// Starts writing every loaf's price into a spreadsheet, once.
action list_prices(list_id: PriceList.id) fails PriceListNotFound -> PriceList {
    var price_list: PriceList = attempt PriceList.get(list_id)
    var list_run: BatchRun = csv.write(
        price_lists,
        name: "prices.csv",
        format: CsvFormat.spreadsheet,
        from: loaves_by_id(LoafPage {}),
        each: loaf -> PriceLine {
            code: loaf.code
            price: loaf.price
        },
        finished: done -> prices_listed(done, price_list.id),
        key: price_list.id
    )
    return PriceList.update(price_list) {
        set: {
            list_run: list_run
        }
    }
}
```

Three loaves, one of them with a code a spreadsheet would read as a
formula, give this file: one line for each loaf, in the order of their
ids, after the byte-order mark, each line ending in CRLF:

```text
Loaf code,Price
rye,3.50
spelt,4.20
'=sour,5.00
```

A bakery reads its suppliers' price sheets into its loaves. The sheet's
header names the columns, and `column:` names one that isn't the field's
name. A row whose price isn't a price, or whose loaf's code is taken, is
a problem, named by its row and column, and the rest carry on:

```jadpo check project=csv-prices file=app.jadpo
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=csv-prices file=domains/bakery/types/loaf_code.jadpo hidden
/// A loaf's code on the bakery's lists.
type LoafCode = Text {
    min_length: 1
    max_length: 20
}
```

```jadpo check project=csv-prices file=domains/bakery/types/loaf_price.jadpo hidden
/// What a loaf costs.
type LoafPrice = Decimal {
    scale: 2
    min: 0.00
}
```

```jadpo check project=csv-prices file=domains/bakery/types/price_row.jadpo
use bakery/types { LoafCode, LoafPrice }

/// One sheet_line of a supplier's price sheet.
type PriceRow = Input {
    code: LoafCode {
        column: "Loaf code"
    }
    price: LoafPrice {
        column: "Price"
    }
}
```

```jadpo check project=csv-prices file=domains/bakery/failures/code_taken.jadpo hidden
/// Another loaf has that code.
type CodeTaken = Failure {
    kind: FailureKind.conflict
}
```

```jadpo check project=csv-prices file=domains/bakery/entities/loaf.jadpo hidden
use bakery/types { LoafCode, LoafPrice }

/// A loaf the bakery sells.
entity Loaf {
    id: Uuid
    code: LoafCode {
        unique: true
    }
    price: LoafPrice

    policy: {
        create: [Access.system]
        read: [Access.public, Access.system]
    }
}
```

```jadpo check project=csv-prices file=domains/bakery/entities/price_sheet.jadpo hidden
/// A supplier's price sheet, and how loaf_run it went.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 5mb
        types: [FileType.csv]
    }
    loaf_run: BatchRun?
    priced: Int?
    refused: Int?

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.public, Access.system]
    }
}
```

```jadpo check project=csv-prices file=domains/bakery/actions/add_loaf.jadpo hidden
use bakery/entities { Loaf }
use bakery/types { PriceRow }
use bakery/failures { CodeTaken }

/// Adds the loaf one sheet_line of a price sheet lists.
action add_loaf(sheet_line: PriceRow) fails CodeTaken -> Unit {
    attempt Loaf.create {
        set: {
            code: sheet_line.code
            price: sheet_line.price
        }
        conflict: {
            Loaf.code: CodeTaken
        }
    }
}
```

```jadpo check project=csv-prices file=domains/bakery/actions/prices_read.jadpo hidden
use bakery/entities { PriceSheet, PriceSheetNotFound }

/// Keeps how loaf_run a price sheet went.
action prices_read(
    summary: ImportSummary,
    sheet_id: PriceSheet.id
) fails PriceSheetNotFound -> Unit {
    attempt PriceSheet.update(sheet_id) {
        set: {
            priced: summary.items - summary.items_with_problems
            refused: summary.items_with_problems
        }
    }
}
```

```jadpo check project=csv-prices file=domains/bakery/actions/read_prices.jadpo migrate=initial
use bakery/entities { PriceSheet, PriceSheetNotFound }
use bakery/types { PriceRow }
use bakery/actions { add_loaf, prices_read }

/// Starts loaf_run a kept price sheet into the loaves, once.
action read_prices(sheet_id: PriceSheet.id) fails PriceSheetNotFound -> PriceSheet {
    var supplier_sheet: PriceSheet = attempt PriceSheet.get(sheet_id)
    var loaf_run: BatchRun = csv.read(
        supplier_sheet.sheet,
        as: PriceRow,
        each: sheet_line -> add_loaf(sheet_line),
        finished: summary -> prices_read(summary, supplier_sheet.id),
        key: supplier_sheet.id
    )
    return PriceSheet.update(supplier_sheet) {
        set: {
            loaf_run: loaf_run
        }
    }
}
```

A sheet like this one adds two loaves. Row 3's price and row 4's code are
problems, `(3, "Price", "type")` and `(4, "", "code_taken")`, and
`finished:` hears of four rows, two of them with problems:

```text
Loaf code,Price
rye,3.50
spelt,cheap
rye,3.80
oat,2.95
```

### `tsv`

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `tsv.read(file, as:, each:, finished:, key:, max_unpacked_size:)` | As `csv.read`, with a tab between cells | As `csv.read` | none |
| `tsv.write(store, name:, format:, from:, each:, finished:, key:)` | As `csv.write`, with a tab between cells | As `csv.write` | none |

TSV is CSV with a tab between cells, so every rule under `csv` applies.

### `json_lines`

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `json_lines.read(file, as:, each:, finished:, key:, max_unpacked_size:)` | `BatchRun`: each line of a stored `File` decoded as one JSON value, an `Input` of `as:`, and handed to `each:`; `finished:` receives an `ImportSummary` | As `csv.read`: a line that can't be decoded is a `RowProblem`; text that isn't text stops the run | none |
| `json_lines.write(store, name:, from:, each:, finished:, key:)` | `BatchRun`: each item of `from:` turned into an `Output` by `each:` and written as one line; `finished:` receives an `ExportSummary` | As `csv.write` | none |

A JSON Lines row may hold lists and objects, which is why a CSV row that
needs them is pointed here. Encodings and gzip are as for `csv`, and a
JSON Lines file takes no `format:`.

> Not built yet: reading legacy encodings. They come when an application
> needs one.
>
> The readers run as part of a `batches.each` run
> ([How a file's run is kept](12-jobs-and-events.md#how-a-files-run-is-kept)),
> and the writers as an export's run does
> ([How an export's run is kept](12-jobs-and-events.md#how-an-exports-run-is-kept)).

## `json`
<!-- topic: json -->

`json` reads and writes JSON. Given text, `json.read` reads one document
as one of your types, checking every rule of the type, and `json.write`
writes a value as compact text. Given a stored file, the same two names
read and write a large JSON list, a row at a time, as a batch.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `json.read(text, as: T)` | `T`, decoded and checked against every rule of `T`; `text` is a value built on `Text`, or on `Bytes` in UTF-8 | `malformed` (not JSON, or an object naming a member twice), `mismatch` (a field missing, unknown or breaking a rule) | none |
| `json.write(value)` | `Text`, compact; fields in declared order, sets and map keys in canonical order, so the same value always gives the same text | | none |
| `json.read(file, items_at:, as:, each:, finished:, key:, max_unpacked_size:)` | `BatchRun` over a large JSON list in a stored `File`, `items_at:` naming where the list sits in the document, or left out when the document is the list; each item an `Input` of `as:`, handed to `each:` | As `csv.read` | none |
| `json.write(store, name:, from:, each:, finished:, key:)` | `BatchRun` writing a large JSON list, each item of `from:` turned into an `Output` by `each:` | As `csv.write` | none |

An order a supplier's system sends as JSON is read as the bakery's type
and written back. Reading checks every rule of the type, fills `rush`'s
default and leaves `note` out. Writing gives the fields in declared
order, with no white space:

```jadpo check project=json-in-memory
type SupplierCode = Text {
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
    sku: SupplierCode
    count: LineCount
    price: LinePrice
}

type SupplierOrder = Input {
    reference: SupplierCode
    lines: List<ImportedLine>
    rush: Bool {
        default: false
    }
    note: SupplierCode {
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

Text that isn't JSON fails `malformed`. JSON that isn't a
`SupplierOrder` (a line's `count` of 0, an unknown field, a missing
`lines`) fails `mismatch`, and the failure's report names the first
problem's path and rule, `lines.0.count` and `min`, never the value:

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

### One name for each direction

`json.read` given text reads one document, and given a file a large
list; `json.write` is the same. There is no `json.decode`.

The two forms can't be confused. Given text, `json.read` returns the
value; given a stored `File` with `each:`, it returns a run; using one
where the other is expected is a type error. `json.write` given a store
and named arguments is its file form, and given one value its text form.

### What JSON can hold

`as:` takes any type an `Input` may hold, and reads it with an `Input`'s
rules: unknown fields are refused, and `optional` and `default` are
applied. `json.write` takes any value an `Output` may hold. Both use the
wire forms that requests and responses use, `Money`'s included
([Values and types](02-values-and-types.md), [HTTP](07-http.md) and
[Configuration and time](10-configuration-and-time.md)).

That is what the wire can carry: values built on the built-in and
standard types, enums your app or the standard library declares, and
`Object`, `Input` and `Output` types made of them, in lists, sets and
maps, nested to any depth.

Everything else is `type.mismatch` either way, and the error names the
type inside that has no JSON form: a failure, an event, an email, a
message, an entity, `Unit`, a secret (`ApiToken`, `ServiceKey`,
`PostgresUrl`, `RedisUrl`, `SigningKey`), a magic link's `SignInCode` or
`SignInToken` ([Magic links](08-identity.md#magic-links)), a
`PluralCount`, and the built-in types other pages define (`Page`,
`File`, `IssuedToken`, `DecodeProblem`). An entity reaches a client only
through an `Output` that picks what it sends, and a secret only through
a route's `Output` or its setting.

`as:` is a type written out, never `T?`. Keep a document that may be
absent as a `Text?`, and read it only when it is there.

### Reading one document

`json.read` reads as a request's body is read, through the same codec:
one JSON document, with white space allowed around it. Text that isn't
one, or bytes that aren't UTF-8, are `malformed`. The first problem is
the first one the wire's reading finds, an object's fields in declared
order before its unknown ones. A problem with the whole document has an
empty path.

### Writing one document

`json.write` gives a response body's text: compact UTF-8, with non-ASCII
characters as themselves and only `"`, `\` and the characters below
U+0020 escaped. The same value gives byte-identical text on native and
on WebAssembly, which the gate checks against one fixed text.

### Map keys

A map's key is its canonical text. `json.write` writes a key that isn't
text as `text.from` writes it, a date as `2026-10-25` and an amount of
money as `GBP 10.50`, and `json.read` reads it back as `text.parse`
does. A map keyed by a type with no single text form (an object, a
choice, a collection, an enum with data) is `type.mismatch` either way.

### No JSON value of no type

There is no dynamic JSON value. Keep a payload whose shape isn't known
as `Text`, and pass it on.

### What `mismatch` tells you

`mismatch` names the first problem the way HTTP's `invalid_input` does
([How failures reach the client](07-http.md#how-failures-reach-the-client)):

- **The field's path,** with list positions as numbers
  (`lines.2.quantity`) and a map's entry as `<key>`, whether its key or
  its value broke the rule, at any depth (`scores.<key>.<key>`). An
  entry is never named by its key's text or position, since a key is a
  value. An unknown field's name stays in the path, as it does for a
  request.
- **The rule** (`max`), never the value.

The failure's report names both beside its caret: "breaks `mismatch`
(`min` on `lines.0.count`)".

### A failure that says what was wrong: `DecodeProblem`

To tell the caller which field was wrong, declare a field
`problem: DecodeProblem` on the failure you map `mismatch` to, in its
`client` or its `internal` block. It receives the first problem: its
`field`, the path, with list positions as numbers and an unknown field's
name as a part, and its `rule`, never the value. A `DecodeProblem` is a
built-in type holding `field: Text` and `rule: Text`, named as HTTP's
input problems are.

- Only a field written `problem: DecodeProblem` receives it, by that name
  and that type exactly (not `DecodeProblem?`). A field of another name
  or type is an ordinary field the arm gives.
- The arm gives every other field and never `problem`; giving it is
  `failure.duplicate_context_field`.
- `invalid:` fills it only when it covers `mismatch` alone.
- Nothing else fills one. So mapping `malformed:`, a construction's rule
  or a `reject` to such a failure is `failure.missing_context_field`,
  its reason naming `mismatch`.
- Only the runtime builds a `DecodeProblem`. `DecodeProblem { ... }` is
  `type.not_record`, as `IssuedToken { ... }` and `IssuedKey { ... }`
  are, so a problem never holds a value a program wrote.
- Its name is the language's. Like an issued token, it is a built-in
  object: the API record and the OpenAPI document list it under its own
  name, with its two fields required and no others.

A program reads its two fields, as the `match` on the outcome does here:

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

The first test reads the problem back: the field and the rule, even where
the value is a secret. The second lets the failure through, so
`jadpo test` shows its report, with the problem among the client fields
and nothing the document held:

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
failure's `details`
([How failures reach the client](07-http.md#how-failures-reach-the-client)),
under the names HTTP's input problems use. A `problem` declared in
`internal` stays on the server:

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

### A large JSON list in a file

Given a stored `File`, `json.read` reads a large JSON list a row at a
time, as `csv.read` does. How its run goes is in
[How a file's run is kept](12-jobs-and-events.md#how-a-files-run-is-kept).

- **The file form is the one given `each:`.** A `json.read` call with
  `each:`, or with anything else only the file form takes (`items_at:`,
  `finished:`, `key:`, `max_unpacked_size:`), is the file form, whose
  first argument is a stored `File`. Given only text and `as:`, it reads
  one document in memory, as above.
- **`items_at:` says where the list sits,** as text written out in the
  call. It names the members that lead from the document's top object to
  its list, between dots, as a problem's path names fields
  (`"data.items"`). It names members only, so a list inside a list isn't
  reached, and a member whose name holds a dot can't be named. When the
  document is itself the list, as `json.write` writes one, leave
  `items_at:` out. Text that isn't written out, or that has an empty
  member name, is `type.batch_row_invalid`.
- **Each item is a row,** an `Input` of `as:`. Unlike a CSV row, it may
  hold lists and objects, as a JSON Lines row may. Its row is its place
  in the list, counting from 1, and a problem's column is the field's
  path (`lines.0.count`), never the value. An item that isn't JSON is a
  `malformed` problem. An item that isn't the row type has its
  `mismatch` problem's rule, and an item that isn't an object has the
  rule `type`.
- **The document around the items is held to JSON's structure:** the
  objects on the way to the list name each member once. Anything the
  reader can't read on from stops the run, with the items before it done
  ([Batches](12-jobs-and-events.md#batches-work-over-many-items)).

A seed library reads the catalogue a partner library exports: a JSON
document whose packets sit at `catalogue.packets`, each packet's sowing
months a list, and which arrives gzipped:

```jadpo check project=json-file file=app.jadpo
application {
    connections: {
        catalogues: {
            driver: Driver.disk
            path: "/var/lib/seeds/catalogues"
        }
    }
}
```

```jadpo check project=json-file file=domains/seeds/types/packet_code.jadpo hidden
/// A packet's code in a catalogue.
type PacketCode = Text {
    min_length: 1
    max_length: 20
}
```

```jadpo check project=json-file file=domains/seeds/types/sowing_month.jadpo hidden
/// A month to sow in, January as 1.
type SowingMonth = Int {
    min: 1
    max: 12
}
```

```jadpo check project=json-file file=domains/seeds/types/packet_entry.jadpo
use seeds/types { PacketCode, SowingMonth }

/// One packet a partner's catalogue lists.
type PacketEntry = Input {
    code: PacketCode
    sow_in: List<SowingMonth>
}
```

```jadpo check project=json-file file=domains/seeds/entities/packet.jadpo hidden
use seeds/types { PacketCode }

/// A packet the library lends.
entity Packet {
    id: Uuid
    code: PacketCode
    sowing_months: Int

    policy: {
        create: [Access.system]
        read: [Access.public, Access.system]
    }
}
```

```jadpo check project=json-file file=domains/seeds/entities/partner_catalogue.jadpo hidden
/// A partner library's catalogue, kept as it arrived.
entity PartnerCatalogue {
    id: Uuid
    catalogue_file: File {
        store: catalogues
        max_size: 20mb
        types: [FileType.json]
    }
    catalogue_run: BatchRun?

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.public, Access.system]
    }
}
```

```jadpo check project=json-file file=domains/seeds/actions/add_packet.jadpo hidden
use seeds/entities { Packet }
use seeds/types { PacketEntry }

/// Adds the packet one catalogue entry lists.
action add_packet(listed_packet: PacketEntry) -> Unit {
    var packet: Packet = Packet.create {
        set: {
            code: listed_packet.code
            sowing_months: list.count(listed_packet.sow_in)
        }
    }
    assert packet.sowing_months >= 0
}
```

```jadpo check project=json-file file=domains/seeds/actions/read_catalogue.jadpo migrate=initial
use seeds/entities { PartnerCatalogue, PartnerCatalogueNotFound }
use seeds/types { PacketEntry }
use seeds/actions { add_packet }

/// Starts reading a kept catalogue's packets, once, unpacking it to at
/// most 200 MB.
action read_catalogue(
    catalogue_id: PartnerCatalogue.id
) fails PartnerCatalogueNotFound -> PartnerCatalogue {
    var partner: PartnerCatalogue = attempt PartnerCatalogue.get(catalogue_id)
    var catalogue_run: BatchRun = json.read(
        partner.catalogue_file,
        items_at: "catalogue.packets",
        as: PacketEntry,
        each: listed_packet -> add_packet(listed_packet),
        key: partner.id,
        max_unpacked_size: 200mb
    )
    return PartnerCatalogue.update(partner) {
        set: {
            catalogue_run: catalogue_run
        }
    }
}
```

A document like this one, gzipped or not, adds two packets. The second
item's month is a problem, `(2, "sow_in.1", "max")`, and the rest carry
on:

```json
{
  "partner": "Hilltop Seed Swap",
  "catalogue": {
    "packets": [
      {"code": "PEA-01", "sow_in": [3, 4]},
      {"code": "BEAN-02", "sow_in": [5, 13]},
      {"code": "KALE-03", "sow_in": [6]}
    ]
  }
}
```

> Not built yet: the runtime's hosts don't write the `failure.rejected`
> event ([Logging](04-failures.md#logging)). Until they do, a failure's
> log is its report, with the problem among the fields it is declared
> in. The event, once written, holds a `DecodeProblem` as itself, not as
> a keyed hash, since a problem never holds a value. There is no XML
> format namespace yet.

## Colour
<!-- topic: colour -->

`Colour` is a colour as a web page uses it: written as CSS writes a hex
colour, `#1e6fd9`, and held as its red, green, blue and alpha channels.
Use it for a brand colour, a theme, or a check that text is readable on
its background.

Here a design system reads its brand colour from a form, checks that body
text is readable on its page colour, lightens a button for its hover
state, makes a tint, and turns a designer's HSL and OKLCH values into
colours:

```jadpo check project=colour
type FormText = Text {}

type YesNo = Bool {}

type SwatchHue = Angle {}

type SwatchShare = Percentage {
    min: 0%
    max: 100%
}

type SwatchChroma = Decimal {
    scale: 4
    min: 0.0000
}

type NotAColour = Failure {
    kind: FailureKind.invalid_value
}

/// The brand colour a settings form names.
function settings_colour(raw: FormText) fails NotAColour -> Colour {
    var typed_colour: Colour = attempt text.parse(raw, as: Colour) {
        malformed: NotAColour
    }
    return typed_colour
}

/// A colour as a style sheet writes it.
function css_colour(swatch: Colour) -> FormText {
    return FormText(text.from(swatch))
}

/// Whether body text in `text_colour` on `page_colour` meets WCAG 2's AA.
function body_text_passes(text_colour: Colour, page_colour: Colour) -> YesNo {
    return YesNo(colour.contrast(text_colour, page_colour) >= 4.5)
}

/// A button's hover colour: its hue, lighter.
function hover_colour(button_colour: Colour) -> Colour {
    return colour.with_lightness(button_colour, 75%)
}

/// A tint of a colour: a fifth of the way to white.
function tint(swatch: Colour) -> Colour {
    var white: Colour = Colour {
        red: 255
        green: 255
        blue: 255
        alpha: 255
    }
    return colour.mix(swatch, white, 20%)
}

/// A designer's HSL swatch.
function hsl_swatch(
    swatch_hue: SwatchHue,
    swatch_saturation: SwatchShare,
    swatch_lightness: SwatchShare
) -> Colour {
    return colour.from_hsl(swatch_hue, swatch_saturation, swatch_lightness)
}

/// A designer's OKLCH swatch.
function oklch_swatch(
    swatch_lightness: SwatchShare,
    swatch_chroma: SwatchChroma,
    swatch_hue: SwatchHue
) -> Colour {
    return colour.from_oklch(swatch_lightness, swatch_chroma, swatch_hue)
}
```

```jadpo check project=colour file=main.test.jadpo
test "a colour is read and written as CSS writes it" {
    var house_colour: Colour = attempt settings_colour("#1E6FD9")
    assert css_colour(house_colour) == "#1e6fd9"
    assert house_colour.blue == 217
    assert settings_colour("#1e6") fails NotAColour
    assert settings_colour("rgb(30 111 217)") fails NotAColour
}

test "contrast is WCAG 2's ratio, never rounded up" {
    var black: Colour = attempt settings_colour("#000000")
    var white: Colour = attempt settings_colour("#ffffff")
    var mid_grey: Colour = attempt settings_colour("#767676")
    var light_grey: Colour = attempt settings_colour("#777777")
    assert colour.contrast(black, white) == 21.00
    assert colour.contrast(white, white) == 1.00
    assert colour.contrast(mid_grey, white) == 4.54
    assert body_text_passes(mid_grey, white)
    assert not body_text_passes(light_grey, white)
}

test "lightness, mixes, HSL and OKLCH give sRGB colours" {
    var house_colour: Colour = attempt settings_colour("#1e6fd9")
    assert css_colour(hover_colour(house_colour)) == "#5fadff"
    assert css_colour(tint(house_colour)) == "#518de3"
    assert css_colour(hsl_swatch(120deg, 100%, 25%)) == "#008000"
    assert css_colour(hsl_swatch(210deg, 75%, 45%)) == "#1d73c9"
    assert css_colour(oklch_swatch(62.8%, 0.2577, 29.23deg)) == "#ff0000"
}
```

### The value

A `Colour` is an object of four channels, `red`, `green`, `blue` and
`alpha`, each a whole number from 0 to 255, in the sRGB colour space that
web pages use. Alpha is how opaque the colour is: 255 is fully opaque.

Its text is CSS Color 4's hex form: `#1e6fd9`, or `#1e6fd980` when the
colour isn't opaque. It is written in lower case and read in either case.
CSS's three- and four-digit short forms, such as `#1e6`, are refused, so
every colour has text of one length. `#1e6fd9ff` reads as opaque, the same
colour as `#1e6fd9`.

A colour is sent as that text and stored as one `smallint` column per
channel. It isn't ordered, so `<` doesn't compare two colours. It is shown
as its text in every locale, and it isn't personal data.

CMYK, the colours a printer uses, needs a colour profile, so a package
provides it rather than the standard library.

### Colour operations

Every operation gives an sRGB `Colour`. Other colour spaces, such as HSL
and OKLCH, appear only as arguments. No operation fails: a value out of
range is clamped, and a colour sRGB can't show is clipped into it.

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `colour.from_hsl(hue, saturation, lightness)` | `Colour`; `hue` an `Angle`, the others `Percentage` | | none |
| `colour.from_oklch(lightness, chroma, hue)` | `Colour`, clipped into sRGB; `lightness` a `Percentage`, `chroma` a `Decimal`, `hue` an `Angle` | | none |
| `colour.mix(a, b, ratio)` | `Colour`, mixed in OKLCH as CSS's `color-mix()` mixes; `ratio` a `Percentage` of `b` | | none |
| `colour.with_lightness(value, lightness)` | `Colour` with its OKLCH lightness set, chroma, hue and alpha kept | | none |
| `colour.contrast(a, b)` | `Decimal`, WCAG 2's contrast ratio, 1.00 to 21.00, rounded down to 2 places | | none |

A parameter written `Percentage` or `Angle` takes any type built on it, as
a parameter written `Money` does, so your own named `Angle` or
`Percentage` type passes. A bare number doesn't: it is `type.mismatch`, or
`type.unit_required` where an `Angle` is expected.

### Values out of range

As in CSS, saturation, lightness and a mix's ratio are clamped to 0% to
100%, and a hue wraps round the circle. An OKLCH lightness is clamped to
0% to 100%, and its chroma to 0 to 1. CSS clamps chroma only below 0; the
upper bound of 1 is past every display's gamut, and it keeps the
arithmetic bounded.

An `Angle` is held in radians to 9 places, so its degrees are rounded to
6 places before use. That gives an angle written in whole degrees, such
as `120deg`, exactly.

### How the conversions are worked out

The conversions are CSS Color 4's exactly. They use its sample code's
rational sRGB and XYZ matrices and its OKLab matrices, to their 16 places.
A hue is powerless, so it is ignored, at a chroma of 0.000004 or less.

To clip a colour into sRGB, Jadpo clamps each channel to 0 to 1 after the
transfer function. It doesn't use CSS Color 4's chroma-reducing gamut
mapping. Each channel is then rounded to the nearest whole number from 0
to 255, a half rounding up, as CSS writes a channel out and as culori's
`formatHex` does. So `hsl(120deg 100% 25%)`, whose green is 127.5, is
`#008000`.

`mix` is CSS Color 5's `color-mix(in oklch, a, b ratio)`. Both colours
are taken into OKLCH, with lightness and chroma premultiplied by alpha.
The hue goes the shorter way round, and a powerless hue takes the other
colour's. The alpha is mixed, then rounded as a channel is.

`with_lightness` sets the lightness, keeps the chroma, hue and alpha, and
then clips the result into sRGB.

The arithmetic is Jadpo's own: fixed point to 18 places in an `i128`,
with `Decimal`'s correctly rounded `exp`, `ln`, `sin`, `cos`, `atan2`,
`sqrt` and π. So every target gives the same channels. Tests hold the
results to CSS Color 4's sample code run in f64, on this page's values and
200 generated ones. They leave out values within a millionth of a
rounding boundary, where an f64 can't say which side it is on.

### Contrast

`colour.contrast` gives WCAG 2's contrast ratio, which says whether text
is readable on its background. Its success criteria 1.4.3 and 1.4.6 test
against it: 4.5 and 7 for body text.

The ratio is `(L1 + 0.05) / (L2 + 0.05)`, where `L1` is the relative
luminance of the lighter colour and `L2` of the darker, so the order of
the two arguments doesn't matter. Relative luminance uses WCAG 2.0 and
2.1's threshold of 0.03928, weighted 0.2126, 0.7152 and 0.0722. WCAG
2.2's sRGB threshold of 0.04045 gives the same result for every 8-bit
channel.

The ratio is worked out to 12 places, then rounded down to 2. WCAG says a
ratio isn't rounded up to meet a threshold, so `>= 4.5` never passes a
pair just below it: `#777777` on white is 4.47 (4.478) and fails 4.5, and
red on white is 3.99.

Alpha isn't read, because WCAG 2 defines contrast only for opaque colours.
WCAG 3's draft contrast method, APCA, isn't used.

### Your own colour types

`type.standard_exists` reports a type of your own with a colour's shape,
so that you use `Colour` instead. The shape is a red, a green and a blue,
named in full or by their initials (`red` or `r`) and built on `Int` or
`Decimal`, and optionally an alpha (`alpha` or `a`). It is the first shape
with a field that may be left out. The swap renames the fields. A literal
of a three-channel type then needs its `alpha`, which `check` asks for.

## Media
<!-- topic: media -->

The media types describe images, video, audio and captions: an image's
size, a frame rate, a timecode, a subtitle cue, a film's age rating.
Each follows its own standard. As every standard type does, each is read
only in its canonical text. None is ordered, and none is personal data.

Here a video library makes thumbnails, reads a film's age rating, shows
where an edit starts and writes captions:

```jadpo check project=media
type FormText = Text {}

type NotARating = Failure {
    kind: FailureKind.invalid_value
}

/// A still's thumbnail: scaled to fit a 320-pixel square, never enlarged.
function thumbnail(video_still: ImageDimensions) -> ImageDimensions {
    var tile: ImageDimensions = ImageDimensions {
        width: 320
        height: 320
    }
    return media.fit(video_still, within: tile)
}

/// The age rating a film's form names.
function film_rating(raw: FormText) fails NotARating -> ContentRating {
    var typed_rating: ContentRating = attempt text.parse(raw, as: ContentRating) {
        malformed: NotARating
        rating_of_scheme: NotARating
    }
    return typed_rating
}

/// Where an edit starts, as the edit list shows it.
function edit_start(edit_point: Timecode) -> PresentationText {
    return text.format(edit_point, locale: Locale.en_gb)
}

/// A caption as a WebVTT file writes it.
function caption_text(opening_caption: SubtitleCue) -> FormText {
    return FormText(text.from(opening_caption))
}
```

```jadpo check project=media file=main.test.jadpo
test "a thumbnail fits its tile and is never enlarged" {
    var landscape: ImageDimensions = ImageDimensions {
        width: 4000
        height: 3000
    }
    var wide_still: ImageDimensions = ImageDimensions {
        width: 1000
        height: 333
    }
    var icon: ImageDimensions = ImageDimensions {
        width: 200
        height: 100
    }
    assert text.from(thumbnail(landscape)) == "320x240"
    assert text.from(thumbnail(wide_still)) == "320x106"
    assert thumbnail(icon) == icon
}

test "a rating is one of its body's categories" {
    var given_rating: ContentRating = attempt film_rating("BBFC 12A")
    assert given_rating.scheme == ContentRatingScheme.bbfc
    assert film_rating("BBFC PG-13") fails NotARating
    assert film_rating("12A") fails NotARating
}

test "a timecode is shown in SMPTE's notation, a cue as WebVTT writes it" {
    var edit_point: Timecode = Timecode {
        hours: 1
        minutes: 10
        seconds: 0
        frames: 0
        rate: FrameRate {
            numerator: 30000
            denominator: 1001
        }
        drop_frame: true
    }
    assert edit_start(edit_point) == "01:10:00;00"
    var opening_caption: SubtitleCue = SubtitleCue {
        start: 1s
        end: 4s
        text: "Mind the gap."
    }
    assert caption_text(opening_caption) == "00:00:01.000 --> 00:00:04.000\nMind the gap."
}
```

### Fitting an image into a box

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `media.fit(dimensions, within:)` | `ImageDimensions` scaled to fit inside `within`, keeping the aspect ratio, never larger than the original, rounded down to whole pixels and never below 1 | | none |

`media.fit` works as CSS's `object-fit: scale-down` does, in whole
numbers. The side that limits the fit becomes exactly the box's size. The
other side is scaled with it and rounded down, never below 1. An image
that already fits is kept as it is, never enlarged.

### Sizes, ratios and frame rates

- An `ImageDimensions` and a `VideoResolution` are a `width` and a
  `height` in whole pixels, from 1 to 2,147,483,647. That is PNG's limit,
  the largest of the common formats'. Each is written `1920x1080`, width
  first, with a lower-case `x`. Each is sent as named fields, so nothing
  reads them the wrong way round, and stored as a column each.
- A `VideoResolution` has the same shape under its own name. It names a
  resolution by its pixels, never by a name like "4K", which means both
  3840 and 4096 pixels across.
- An `AspectRatio` is a `width` and a `height`, whole numbers from 1,
  written `16:9`. It is held as written, as a `Ratio` is, so `16:9` and
  `32:18` are two different values. It is sent as its text and stored as
  a column each. A cinema's 2.39:1 is written `239:100`. A later release
  can accept decimals without changing any value.
- A `FrameRate` is a `numerator` and a `denominator`, each from 1 to
  2,147,483,647, written `30000/1001` or `25/1`. The denominator is always
  written, and the rate is held as written. It is sent as its text and
  stored as a column each.

### Boxes, paper and orientation

- A `Rect` is an `x` and a `y`, each any `Decimal`, and a `width` and a
  `height`, never below 0. Its text is SVG's `viewBox`: `0 0 300 200`.
- An `Insets` is a `top`, `right`, `bottom` and `left`, each never below
  0, so a negative margin is refused. Its text is the CSS box model's four
  sides: `8 16 8 16`.
- In both texts each number keeps the places it was given, has no leading
  zero, and is one space from the next, and every part is always written.
  Each is sent as named fields and stored as a column per part.
- A `PaperSize` is one of ISO 216's A0 to A10 and B0 to B10, or ANSI/ASME
  Y14.1's letter, legal and tabloid, coded `A4` or `Letter`. ISO 269's
  envelope sizes and ledger (tabloid on its side) are left out.
- An `ExifOrientation` says how a stored image must be turned to show it
  upright. Its variants are named for where the stored image's first row
  and column are seen, as libtiff names TIFF 6.0's values: `top_left` to
  `left_bottom`. They are coded `1` to `8` and sent as that text, as every
  enum is sent as its code.
- An `AltText` is one line of an image's text alternative, as WCAG
  describes it: 1 to 1,000 characters, with no space at either end. An
  image with nothing to say has `none` in a field that may be empty, never
  empty text.

### Codecs

A `VideoCodec` and an `AudioCodec` are the codec strings the W3C
WebCodecs Codec Registry lists, in RFC 6381's form, such as `avc1.42001E`
or `opus`:

- video: `av01`, `avc1`, `avc3`, `hev1`, `hvc1` and `vp09`, each with its
  elements, and `vp8`;
- audio: `mp4a` with its elements, `flac`, `mp3`, `opus`, `vorbis`,
  `ulaw`, `alaw` and the five `pcm-` forms.

The elements after the first aren't checked further. Case is kept, as the
registry writes them.

### Timecodes and subtitles

A `Timecode` is a position in a video as SMPTE ST 12-1 writes it:
`hours` (0 to 23), `minutes` and `seconds` (0 to 59) and `frames` (0 to
119, room for 120 frames a second), with its `rate` and whether it counts
`drop_frame`. Drop-frame timecode skips some frame labels so that the
clock keeps time with a 29.97 frames-a-second video. A timecode has three
rules:

- `frames_within_rate`: its frames are below its rate's frames a second,
  rounded up to a whole number;
- `drop_frame_rate`: drop-frame is used only at 30000/1001 and 60000/1001
  frames a second, compared by value;
- `frame_not_dropped`: a drop-frame timecode never names a label that
  drop-frame skips, frames 0 and 1 (or 0 to 3 at the higher rate) at the
  start of each minute except every tenth.

Its notation, `01:00:00;00`, doesn't say its rate, so a timecode has no
single text form. It is sent as its parts, stored as a column per part
(its rate in two), and shown in that notation.

A `SubtitleCue` is a WebVTT cue: a `start` and an `end`, each a
`Duration` from the start of the media to the millisecond, and its
`text`, lines with no control character and no blank line. Its rules are
`start_not_negative`, `end_after_start` and `text_without_arrow`, which
refuses `-->` in the text. Its canonical text is the cue's timings line,
its hours in two digits or more, then a line feed and its text, with no
identifier or settings: `00:00:01.000 --> 00:00:04.000` and its text. It
is sent as named fields and stored as a column per part.

When you write a `Timecode` or a `SubtitleCue` out as a literal, its rules
are checked while compiling. A value read from JSON is checked as it is
read. One built in code from other values is built with `attempt`, which
fails by the rule it breaks.

### Age ratings

A `ContentRating` is a `scheme` and a `rating`, one of that body's
categories as it writes them, its rule `rating_of_scheme`. It is written
`BBFC 12A` or `PEGI 16`, the body's code read ignoring case. It is sent
as that text and stored as a column each.

The bodies are a standard enum, `ContentRatingScheme`:

- BBFC: U, PG, 12A, 12, 15, 18, R18;
- the MPA, the MPAA's name since 2019: G, PG, PG-13, R, NC-17;
- PEGI: 3, 7, 12, 16, 18;
- ESRB: E, E10+, T, M, AO, RP.

Other bodies, such as USK, the ACB and US television's, come with a
later release.

## Web and software
<!-- topic: web-and-software -->

The web types are the values a web app and its network pass around: URLs
and paths, domain names, HTTP statuses and headers, cookies, IP ranges,
and the problem documents an API reports failures with. Each follows its
standard, and, as every standard type is, each is read only in its
canonical text.

Here a shop builds its search links, follows the links on its pages, lets
its office's network in, reads the domain a seller types, retries a call
the server failed, and reports a sold-out item as RFC 9457 has it:

```jadpo check project=web-and-network
type ShopLink = Url {}

type SearchTerm = Text {
    min_length: 1
    max_length: 100
}

type CallerAddress = IpAddress {}

type FormText = Text {}

type YesNo = Bool {}

type NotADomain = Failure {
    kind: FailureKind.invalid_value
}

/// A search page's link for a term, on its first page.
function search_link(catalogue_page: ShopLink, tea_term: SearchTerm) -> ShopLink {
    var none_yet: Map<Text, Text> = []
    var tea_query: Map<Text, Text> = map.with(map.with(none_yet, "q", tea_term), "page", "1")
    return ShopLink(web.with_query(catalogue_page, tea_query))
}

/// Where a link on `page` leads.
function link_target(page: ShopLink, page_link: RelativeUrl) -> ShopLink {
    return ShopLink(web.resolve(page, page_link))
}

/// The basket's address from any page: a path is a relative URL too.
function basket_from(page: ShopLink, basket: UrlPath) -> ShopLink {
    return ShopLink(web.resolve(page, basket))
}

/// Whether a caller is on the office's network.
function from_office(office_network: Cidr, caller: CallerAddress) -> YesNo {
    return YesNo(web.cidr_contains(office_network, caller))
}

/// The domain a seller typed for their shop.
function shop_domain(raw: FormText) fails NotADomain -> DomainName {
    var typed_domain: DomainName = attempt text.parse(raw, as: DomainName) {
        malformed: NotADomain
    }
    return typed_domain
}

/// Whether a failed call is worth trying again: the server's fault, not
/// the request's.
function worth_retrying(status: HttpStatus) -> YesNo {
    return YesNo(status >= 500)
}

/// The problem a sold-out item is reported with.
function sold_out(item_page: ShopLink) -> ProblemDetails {
    return ProblemDetails {
        type_: "https://tea.example/problems/sold-out"
        status: 409
        title: "Sold out"
        detail: "Nothing is left of this item."
        instance: item_page
    }
}

/// A problem as a person reads it.
function problem_shown(problem: ProblemDetails, customer_locale: Locale) -> PresentationText {
    return text.format(problem, locale: customer_locale)
}

/// A problem as it is sent.
function problem_sent(problem: ProblemDetails) -> FormText {
    return FormText(json.write(problem))
}
```

```jadpo check project=web-and-network file=main.test.jadpo
test "links follow RFC 3986" {
    var search_page: ShopLink = "https://tea.example/search"
    var searched: ShopLink = "https://tea.example/search?page=1&q=tea%20%26%20cake"
    assert search_link(search_page, "tea & cake") == searched
    var sencha: ShopLink = "https://tea.example/teas/green/sencha"
    assert link_target(sencha, "../black/assam") == "https://tea.example/teas/black/assam"
    assert link_target(sencha, "#reviews") == "https://tea.example/teas/green/sencha#reviews"
    assert basket_from(sencha, "/basket") == "https://tea.example/basket"
}

test "an address is inside a range of its own family only" {
    assert from_office("192.0.2.0/24", "192.0.2.77")
    assert not from_office("192.0.2.0/24", "198.51.100.1")
    assert from_office("2001:db8::/32", "2001:db8:1::5")
    assert not from_office("::/0", "192.0.2.77")
}

test "a domain name is read in its ASCII form only" {
    var typed_domain: DomainName = attempt shop_domain("xn--mnchen-3ya.de")
    assert typed_domain == "xn--mnchen-3ya.de"
    assert shop_domain("München.de") fails NotADomain
    assert shop_domain("example.com.") fails NotADomain
}

test "a status's class is a comparison, and a problem is sent as RFC 9457 has it" {
    assert worth_retrying(503)
    assert not worth_retrying(404)
    var problem: ProblemDetails = sold_out("https://tea.example/items/assam")
    assert problem_shown(problem, Locale.en_gb) == "Sold out\nNothing is left of this item."
    var sent: FormText = "{\"type\":\"https://tea.example/problems/sold-out\",\"status\":409,\"title\":\"Sold out\",\"detail\":\"Nothing is left of this item.\",\"instance\":\"https://tea.example/items/assam\"}"
    assert problem_sent(problem) == sent
}
```

### Web operations

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `web.with_query(url, entries)` | `Url` with each of `entries` (a `Map<Text, Text>`) added to its query, percent-encoded, in key order | | none |
| `web.resolve(base, relative)` | `Url`: a `RelativeUrl` or `UrlPath` resolved against `base` by RFC 3986 | | none |
| `web.cidr_contains(range, address)` | `Bool`: the `IpAddress` is inside the `Cidr` | | `x >>= a` |
| `semver.satisfies(version, range)` | `Bool`: a `SemanticVersion` is inside a `VersionRange` | | none |

`web`'s operations never fail, because each takes values that were
checked when they were built.

- `web.with_query`'s map is called `entries`, because `query` is a
  reserved word. Its entries are added in key order, by code point, after
  any query the URL already has and before its fragment. An empty map
  leaves the URL as it was.
- `web.resolve` is RFC 3986 §5.2's strict resolution. It normalises
  nothing but dot segments (`.` and `..`).
- `web.cidr_contains` never matches across address families, just as
  PostgreSQL's `>>=` doesn't. So an IPv4-mapped IPv6 address isn't inside
  an IPv4 range.

`semver.satisfies` is under [Software types](#software-types).

### Which standard each type follows

| Type | Standard |
|---|---|
| `RelativeUrl`, `UrlPath` | RFC 3986's relative reference and absolute path |
| `DomainName` | UTS 46's ASCII form, `xn--mnchen-3ya.de` |
| `HttpStatus` | RFC 9110's three digits |
| `ProblemDetails` | RFC 9457's object |
| `JsonPointer` | RFC 6901 |
| `Cookie` | RFC 6265's `Set-Cookie` value |
| `Cidr` | RFC 4632's range, with its host bits zero |
| `MacAddress` | IEEE 802's form, `AC-DE-48-00-00-80` |
| `Imei` | 3GPP's 15 digits, with their Luhn check digit |
| `Emoji` | one of Unicode's fully-qualified emoji |

Every `UrlPath` is a `RelativeUrl`, and every `DomainName` is a
`Hostname`. So the first of each pair is built on the second (`public
type UrlPath = RelativeUrl {}`), and a value of the first passes where the
second is taken: `web.resolve`'s `relative` takes a path. It never works
the other way round.

### Types held as text

`UrlPath`, `RelativeUrl`, `Slug`, `DomainName`, `Hostname`, `WebOrigin`,
`HttpHeaderName`, `UserAgent`, `JsonPointer`, `Cookie`, `Cidr`,
`MacAddress`, `Imei`, `Emoji` and `PushToken` are held as text, as the
payment numbers are. Each is a pattern and, where a pattern can't say
everything, a named validator. For each of them:

- only the canonical text is read;
- none is ordered;
- `text.from` is `type.mismatch`, because each is text already;
- `text.parse` fails only as `malformed`.

`Hostname`, `WebOrigin` and `Cidr` are text, not an enum and objects,
because no operation reads their parts and each value has one text. In
detail:

- A **`RelativeUrl`** is RFC 3986's relative reference (§4.2), and is
  never empty. It is a path of any of the RFC's three kinds, a query and
  a fragment, or an authority after `//` followed by those. The
  authority's host is a `DomainName` or an IP address in its canonical
  text, IPv6 in brackets, and its port is from 1 to 65535. User
  information before the host is refused, because §3.2.1 deprecates it. A
  percent-encoding is in upper case (§2.1).
- A `RelativeUrl` is ASCII. An IRI, a URL with characters beyond ASCII
  (RFC 3987), is percent-encoded first, as `encoding.to_url_component`
  does.
- A **`UrlPath`** is RFC 3986's `path-absolute`, a path starting with
  `/`, with no query or fragment.
- A **`DomainName`** is what UTS 46's strict processing gives: IDNA 2008
  with STD3's rules, `CheckHyphens` and the DNS lengths, as Servo's `idna`
  1.1.0 does it on the pinned ICU4X. That means lower case; letters,
  digits and hyphens; each label beyond ASCII as its A-label (`xn--...`);
  no dot at the end; and a last label that isn't all digits.
- It refuses what IDNA 2008 refuses: `--` in the third and fourth places
  of a label other than an A-label's `xn--`, as some CDNs' host names
  have, and `_`, as DNS records' names (`_dmarc`) have. `localhost`, a
  single label, is a domain name.
- A domain name is shown as its A-label. Showing the Unicode form safely
  needs UTS 39's rules for characters that look alike, which aren't
  decided.
- A **`WebOrigin`** is `http` or `https` only. The HTML standard's other
  tuple origins, `ws`, `wss` and `ftp`, aren't what an `Origin` header
  carries from a page. The default port is left out, as the standard
  writes an origin.
- An **`HttpHeaderName`** is a header name in lower case, as HTTP/2 and
  HTTP/3 send field names, so `==` ignores case without doing anything
  special.
- A **`UserAgent`** is RFC 9110's grammar in ASCII, with one space between
  products and comments.
- A **`JsonPointer`** is RFC 6901's string form, not its URI fragment
  form. The empty pointer, which points at the whole document, is one.
- A **`Cookie`** is a server's `Set-Cookie` value in RFC 6265's strict
  syntax (§4.1.1), with RFC 6265bis's `SameSite` and name prefixes. Its
  attributes come after `; `, in the RFC's order and spelling, each at
  most once. An attribute the RFCs don't define, such as `Partitioned` or
  `Priority`, is refused.
- A cookie's `Max-Age` is never 0, as the strict syntax has it. To remove
  a cookie, give it an `Expires` in the past, whose weekday must be its
  date's. `SameSite=None` needs `Secure`, and the `__Secure-` and
  `__Host-` prefixes need their rules, compared ignoring case.
- A **`Cidr`**, a range of IP addresses, writes its address as
  `IpAddress` would write it canonically, RFC 5952's form for IPv6, with
  every host bit zero.
- A **`MacAddress`** is IEEE 802's own form, upper-case pairs joined by
  `-`, for EUI-48 only.
- An **`Imei`**, a phone's identity, is 15 digits with 3GPP TS 23.003's
  Luhn check digit. An IMEISV, the form with a software version, isn't
  one.
- An **`Emoji`** is one of Emoji 18.0's 3,963 fully-qualified emoji, from
  Unicode 18.0.0's final `emoji-test.txt` under the Unicode License v3,
  generated once into `jadpo_std::standard::emoji_data`. An unqualified or
  minimally-qualified form is refused, and so is a skin tone on its own.
- A **`PushToken`** is the token a phone gives an app for push
  notifications: opaque text of the characters Apple's and Google's tokens
  use, up to 4096 of them. It has no part saying which platform it is for,
  which would add an enum to the language. A Web Push subscription isn't
  a token.

### Types held as numbers, enums and objects

- An **`HttpStatus`** is a whole number from 100 to 599, RFC 9110 §15's
  three digits, with its registry open. A **`NetworkPort`** is one from 1
  to 65535; IANA reserves 0, so it is refused.
- Each is a type built on `Int` with its range. So `<` orders it, and
  `status >= 500` asks for a class of statuses. JSON sends it as a number,
  an `integer` column stores it, and `text.format` shows its digits, never
  grouped: `8080`, not `8,080`.
- `text.parse` reads only digits within the range, with no sign and no
  leading zero. Anything else is `malformed`. A type of your own with a
  narrower range, such as `type HighPort = NetworkPort { min: 1024 }`,
  breaks `min` or `max` by name.
- **`HttpMethod`** is the enum that [HTTP](07-http.md),
  [External services](11-integrations.md) and [Testing](13-testing.md)
  use. Its five variants are the methods routes are written with: `get`,
  `post`, `put`, `patch` and `delete`. Its codes are RFC 9110's tokens,
  such as `GET`, read ignoring case at a boundary as every standard enum's
  are, and it is stored by its code.
- RFC 9110's `HEAD`, `OPTIONS`, `CONNECT` and `TRACE` aren't variants.
  The runtime answers `HEAD` and `OPTIONS` itself, and no route is written
  with any of them. Adding them later breaks nothing. A proxy that must
  name the others holds the method as text.
- A **`ProblemDetails`** is RFC 9457's object for reporting an API
  failure: its five members, each of which may be `none`, as the RFC
  allows. `type` is a reserved word, so code writes `type_`, and the wire
  has `type`. That is the trailing `_` a closed list's code takes when it
  is a reserved word, used here for a standard object's part
  (`jadpo_std::standard::wire_name`).
- Its `type` and `instance` are absolute `Url`s. The RFC allows relative
  references; resolve one with `web.resolve` first. Its `title` is a line
  and its `detail` a paragraph.
- A member the RFC doesn't define is refused, as any object refuses an
  unknown field. An application with extension members declares its own
  type.
- A problem is sent with its `none` members left out, and read with
  missing members as `none`. It is stored as a column per member. In a
  field that may be `none`, a row whose columns are all `NULL` reads back
  as `none`; in a field that can't be, as a problem that says nothing.
- It has no single text form. `text.format` shows its title and detail
  on their own lines; with neither, its status; with no status, its type,
  or `about:blank`.
- A **`UtmParameters`** is the campaign tags a marketing link carries: an
  object of `source`, `medium` and `campaign`, each a line, and `term` and
  `content`, which may be `none`, as Google Analytics' campaign builder
  has them. GA4's `utm_id` and the other later tags aren't parts.
- Its canonical text is the query that carries them,
  `utm_source=mailing&utm_medium=email&utm_campaign=autumn_sale`, in that
  order, each value percent-encoded as `encoding.to_url_component`
  encodes it. Its JSON is its parts, and it is stored as a column per
  part.

### Personal data and shapes

A user agent, a cookie, a MAC address, an IMEI and a push token are
`online` [personal data](15-personal-data.md#marking-personal-fields).
The other web types aren't personal data.

`type.standard_exists` matches none of these types. An application's
object can't have a field called `type`, as `ProblemDetails` does, and
`UtmParameters`' part names are too common to be a sign that a type is
one.

## Software types
<!-- topic: software-types -->

The software types are the values tools and build systems pass around:
version numbers and ranges, git refs, package and container names,
licences, trace ids, file names and paths. Each follows its standard, as
the web types do.

Here a shop's plugin registry reads the version a release form names,
offers a release only to the shops whose range it fits, and tells a newer
release from an older one by SemVer's rules, not by its text:

```jadpo check project=software
type FormText = Text {}

type YesNo = Bool {}

type NotAVersion = Failure {
    kind: FailureKind.invalid_value
}

/// The version a plugin's release form names.
function plugin_version(raw: FormText) fails NotAVersion -> SemanticVersion {
    var typed_version: SemanticVersion = attempt text.parse(raw, as: SemanticVersion) {
        malformed: NotAVersion
    }
    return typed_version
}

/// Whether a plugin's release fits the range a shop asks for.
function fits_shop(plugin_release: SemanticVersion, shop_wants: VersionRange) -> YesNo {
    return YesNo(semver.satisfies(plugin_release, shop_wants))
}

/// Whether an offered release is newer than the one installed.
function newer_than(installed_release: SemanticVersion, offered_release: SemanticVersion) -> YesNo {
    return YesNo(offered_release > installed_release)
}

/// Whether a release is offered to every shop: it has no pre-release.
function generally_available(plugin_release: SemanticVersion) -> YesNo {
    return YesNo(plugin_release.pre_release == none)
}

/// A release's version as its tag writes it.
function release_tag(plugin_release: SemanticVersion) -> FormText {
    return FormText(text.from(plugin_release))
}
```

```jadpo check project=software file=main.test.jadpo
test "versions follow SemVer's precedence, not their text" {
    var nine: SemanticVersion = attempt plugin_version("1.9.0")
    var ten: SemanticVersion = attempt plugin_version("1.10.0")
    var release_candidate: SemanticVersion = attempt plugin_version("1.10.0-rc.1")
    assert newer_than(nine, ten)
    assert newer_than(release_candidate, ten)
    assert not generally_available(release_candidate)
    assert release_tag(release_candidate) == "1.10.0-rc.1"
    assert plugin_version("v1.10.0") fails NotAVersion
}

test "a range takes a pre-release only where it names one" {
    var patch_release: SemanticVersion = attempt plugin_version("1.4.2")
    var beta_release: SemanticVersion = attempt plugin_version("2.0.0-beta.1")
    assert fits_shop(patch_release, "^1.2.3")
    assert not fits_shop(patch_release, "~1.2.3")
    assert not fits_shop(beta_release, "^1.2.3")
    assert not fits_shop(beta_release, ">=1.0.0")
    assert fits_shop(beta_release, ">=2.0.0-beta.0 <3.0.0")
}
```

### Which standard each type follows

| Type | Standard |
|---|---|
| `SemanticVersion` | SemVer 2.0.0, `1.2.3-alpha.1+build.5` |
| `VersionRange` | npm's range grammar, written one way: `^1.2.3`, `>=1.0.0 <2.0.0`, `1.2.x \|\| >=2.5.0-beta.1` |
| `GitRef` | what `git check-ref-format` accepts |
| `PackageUrl` | ECMA-427's canonical `pkg:` URL |
| `Cve` | the CVE programme's id |
| `SpdxLicence` | an SPDX expression over the SPDX licence list |
| `TraceId`, `SpanId`, `TraceParent` | W3C Trace Context |
| `LogLevel` | OpenTelemetry's six severities |
| `FileName`, `FilePath` | names and paths every common file system takes, written with `/` |
| `RegularExpression` | a pattern that `pattern:`'s linear-time engine takes |
| `ContainerImage` | the OCI distribution reference, with its registry and a tag or a digest |
| `Digest` | the OCI image specification |
| `Ulid` | the ULID specification's 26 characters |
| `IdempotencyKey` | what the `Idempotency-Key` header carries |
| `Doi` | the DOI Handbook's name |

None of them is personal data. `type.standard_exists` matches none of
them, so your own `{ major, minor, patch }` type stays your own for now:
adding a shape later would report every such type at once.

### Versions

A `SemanticVersion` is an object of `major`, `minor` and `patch`, each a
whole number from 0 up, and `pre_release` and `build`, each text or
`none`. So code reads its parts: `release.pre_release == none`.

Its canonical text has no `v` before it, and a number too large for an
`Int` is `malformed`. It is sent as that text and shown as its text. It is
stored as a column per part: `bigint` for the numbers, and `text` or
`NULL` for the rest.

`<` is SemVer 2.0.0's precedence (§11), so `1.10.0` comes after `1.9.0`
and every pre-release comes before its release:

- the numbers compare numerically;
- a pre-release is below its release;
- pre-release identifiers compare numerically when they are numbers and
  in ASCII order when they aren't, numbers first.

Build metadata is ignored, as §10 says, so neither of `1.0.0+a` and
`1.0.0+b` is below the other. They still aren't equal, because `==`
compares every part. Sorting and a set's order go by precedence, then by
build metadata as text, so they stay total and agree with `==`.

A query can't order by a version, because its columns don't sort by
precedence.

### Version ranges

A `VersionRange` is the range a package asks for, in npm's range grammar
(node-semver's), because SemVer defines none. It is one or more sets
joined by ` || `. Each set is made of comparisons (`<`, `<=`, `>`, `>=`),
`~`, `^`, partial versions (`1.2`, `1.2.x`) and hyphen ranges
(`1.0.0 - 1.4.2`), with `*` for any version.

A range is read in one spelling only:

- no `v` or `=` before a version;
- no space after an operator, and one space between comparisons;
- the wildcard `x` inside a version, and `*` only on its own;
- no build metadata, which a range can't use.

Each form means what node-semver expands it to: `^0.2.3` is
`>=0.2.3 <0.3.0-0`. A range is held as its text, so two spellings of one
set, such as `1.2` and `1.2.x`, are different values.

`semver.satisfies(version, range)` says whether a version is inside a
range, and never fails. It keeps node-semver's rule for pre-releases,
which Cargo shares: a version with a pre-release is inside a range only
where a comparison in the same set names a pre-release of the same major,
minor and patch. So `^1.2.3` never takes `2.0.0-beta.1`, and `*` takes no
pre-release.

### Types held as text

The other software types are held as text:

- A **`GitRef`** is any name `git check-ref-format --allow-onelevel`
  accepts: a branch (`main`), a tag, a full ref (`refs/heads/main`) or a
  commit's full hash. It isn't normalised with `--normalize`, so a leading
  `/` or a `//` is refused. Its length isn't limited, as git's isn't.
- A **`PackageUrl`** is ECMA-427's canonical form: `pkg:` with no `//`, a
  lower-case type, and each component percent-encoded only where §5.4
  says, in upper-case hex, with none of `A-Z a-z 0-9 . - _ ~ :` encoded.
  Qualifier keys are lower case, sorted, and each appears once with a
  value. A subpath has no `.`, `..` or empty segment. A registered type's
  own rules, such as npm's lower-case names or PyPI's normalisation,
  aren't checked; checking them would refuse more, never accept more.
- A **`Cve`** is `CVE-`, a year from 1999, a `-` and a sequence number of
  four digits, or of five to nineteen digits with no leading zero, in
  upper case. That is the CVE programme's 2014 syntax, with its JSON
  schema's limit of nineteen digits.
- A **`SpdxLicence`** is an SPDX 2.3 expression (Annex D) over the SPDX
  License List 3.29.0, bundled by Embark's `spdx` crate 0.13.6 (CC0 data,
  Apache-2.0 code). It holds each identifier as the list spells it,
  deprecated ones included since they still name a licence; `+`; `WITH`
  an exception on the list; `AND`, `OR` and parentheses; and
  `LicenseRef-` and `DocumentRef-` references. Operators are upper case,
  words are one space apart, and parentheses have no space inside.
  `NONE` and `NOASSERTION` are values a document uses, not expressions,
  and are refused.
- A **`TraceId`** and a **`SpanId`** are W3C Trace Context's ids, in
  lower-case hexadecimal, never all zeros.
- A **`TraceParent`**, the header that carries a trace from one service
  to the next, is text, as `Cidr` is, because no operation reads its
  parts. Only version `00` is read: it is the one version defined, and
  its length is fixed. Its flags are any two hex digits.
- A **`FileName`** is a name every common file system takes:
  - 1 to 255 bytes of UTF-8;
  - no `/`, `\`, NUL or other control character, and none of Windows'
    `< > : " | ? *`;
  - not `.` or `..`, and no space or dot at the end, which Windows drops;
  - not a Windows device name (`CON`, `PRN`, `AUX`, `NUL`, `COM0` to
    `COM9`, `LPT0` to `LPT9`, with `¹²³`), in any case and with any
    extension, so `nul.txt` is refused.
- A **`FilePath`** is written with `/`, as POSIX systems and URLs write
  one, so a Windows path such as `C:\Users` is refused. It is `/` alone,
  or names joined by `/`, absolute after a leading `/`, each a `FileName`.
  It is in its shortest form: no `.` segment, `..` only at the start of a
  relative path, no empty segment, and no `/` at the end. It is at most
  4096 bytes, Linux's `PATH_MAX`. It says nothing about where files are
  kept: the [file stores](16-built-in-services.md#files) never make a key
  from a name.
- A **`RegularExpression`** is a pattern that a type's `pattern:` takes
  ([Constraints](02-values-and-types.md#constraints)), compiled by
  `jadpo_std::compile_pattern` with the `regex` crate. So backreferences
  and lookaround are refused, as they are there. It is matched against
  the whole text, and it isn't empty.
- A **`ContainerImage`** is an OCI distribution reference, written in
  full: a registry first (a domain in lower case with a `.` or a port, an
  IP address, or `localhost`), then the repository's components in lower
  case, then a tag, a digest or both. So `nginx:1.27`, Docker Hub's short
  form of `docker.io/library/nginx:1.27`, is refused, and so is a
  reference with neither a tag nor a digest (an implied `latest`). Each
  image has one text.
- A **`Digest`** follows the OCI image specification's grammar. Its
  registered algorithms are held to their own forms: `sha256` and
  `blake3` are 64 lower-case hex digits, `sha512` 128. An unregistered
  algorithm passes when it meets the grammar, as the specification asks.
- A **`Ulid`** is in upper case only, its first character at most `7`. It
  isn't ordered, as no type held as text is.
- An **`IdempotencyKey`**, the key a client sends so that a repeated
  request isn't carried out twice, is 1 to 255 printable ASCII characters
  with no `"` or `\` and no space at either end. That lets a structured
  header field carry it unescaped. The draft standard recommends a UUID
  but doesn't require one.
- A **`Doi`** is `10.`, a registrant code of four to nine digits with any
  subdivisions, `/`, and a suffix of printable characters, its ASCII
  letters in lower case because the DOI Handbook compares them ignoring
  case. `doi:` and resolver URLs are refused.

### Log levels

A **`LogLevel`** is an enum of OpenTelemetry's six severities: `trace`,
`debug`, `info`, `warn`, `error` and `fatal`. All six are there, so a
later addition doesn't break a `match`. Its codes are their short names,
such as `INFO`, read ignoring case, and it is stored by its code, as
`HttpMethod` is.

A `LogLevel` isn't ordered, because no enum is yet. So when
[Telemetry](16-built-in-services.md#telemetry) keeps "logs from
`LogLevel.warn`", the runtime reads that order itself.

## Groups with the baseline only
<!-- topic: groups-with-the-baseline-only -->

Many standard types are a value with a check and nothing more: an IBAN, a
card number, a barcode, an NHS number. They have no operations of their
own. They do what every standard type does
([Every standard type meets one contract](#every-standard-type-meets-one-contract)):
each is built and checked from text or from its parts, sent, stored and
shown to people. Their rules are how they are built, which `text.parse`
and the type's data entry already cover.

Here a marketplace pays its sellers into the account they type, takes a
patient's NHS number from a referral, and shows a card on file by its
last four digits:

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

### Checks beyond a pattern

Most of these types are a `pattern:`. Where a rule is more than a pattern
can say, such as a check digit, the type names one validator:

| Group | Validators beyond a pattern |
|---|---|
| People and contact | `PhoneNumber` (E.164 per country), `PostalCode` (per country), `PostalAddress` and `PersonName` (CLDR order for `text.format`) |
| Places | `CountrySubdivision` against ISO 3166-2, `UnLocode` by its pattern and its country, its list not shipped |
| Payment and identity numbers | `Iban` (mod-97), `PaymentCardNumber` (Luhn), `UkSortCode` with `UkAccountNumber` (modulus check), `AbaRoutingNumber`, `Lei`, `Isin`, `NhsNumber` (mod-11), `UkNationalInsuranceNumber` (prefix rules) |
| Tax and identity documents | `VatNumber`, `TaxId`, `PassportNumber`, `DrivingLicenceNumber` and `IdentityDocument`, each number by its country's published form and check digit (VIES, the OECD's and the EU's TIN pages, each issuer's own), by length where a country publishes none |
| Commerce and logistics | `Ean13`, `UpcA`, `Isbn`, `Sscc` (GS1 check digits), `Vin` (check digit, for North American manufacturers), `ContainerNumber` (ISO 6346) |
| Health | `SnomedConcept` (Verhoeff); `Icd10Code` by pattern, its release not shipped |
| Travel | `IataAirport`, `IcaoAirport`, `IataAirline` by pattern, their lists not shipped |
| Other | `Orcid` (ISO 7064 MOD 11-2), `Mpan` (Elexon's check digit) |

### Payment and identity numbers

Seventeen payment and identity numbers are held as text:
`PaymentCardNumber`, `CardBin`, `PaymentReference`, `InvoiceNumber`,
`Iban`, `Bic`, `UkSortCode`, `UkAccountNumber`, `AbaRoutingNumber`,
`Lei`, `Isin`, `UkUtr`, `UkCompanyNumber`, `UkNationalInsuranceNumber`,
`UsSsn`, `UsEin` and `NhsNumber`.

**Held as text.** Each is a type built on `Text` that the standard
library declares; its stub is `public type Iban = Text {}`. So everything
[Values and types](02-values-and-types.md) says of text applies:
construction, literals checked while compiling, `attempt`, JSON (a
string), storage (a `text` column) and `==`. Your own type can be built
on one and add rules: `type SterlingIban = Iban { pattern: "GB.*" }`.

Its pattern, matched against the whole text, and its validator are one
check. Text that fails it breaks `format`, as text that isn't an `Email`
does, so there is no new rule name. `text.parse` fails only as
`malformed`. `text.from` is `type.mismatch`, because the value is text
already. None of them is ordered.

**The canonical text is the electronic form,** in upper case with no
spaces or dashes: `GB29NWBK60161331926819`, the sort code `089999`, the
SSN `123456789` and the EIN `123456789`. Only that form is read, as
`text.parse` reads only canonical text everywhere, so a printed IBAN and
lower case are refused. Strip a form's spaces before you build one, with
`text.replace`. An SSN or an EIN with dashes is the printed form.

**Shown to people in the printed form,** whatever the locale:

- an IBAN in groups of four;
- a sort code in pairs with dashes, `08-99-99`;
- a National Insurance number in pairs, `AB 12 34 56 C`;
- an NHS number in groups of 3, 3 and 4, `943 476 5919`;
- an EIN as 2 then 7 digits;
- the others as their canonical text.

The runtime sees only text, so the checker gives `text.format` the name of
the standard type the value is held as, as an argument you don't write.

**Masked by `text.format`, to be safe.** A `PaymentCardNumber` and a
`UsSsn` are shown as their last four only, `•••• 4242` and `•••-••-6789`.
That is as much of a card number as PCI DSS lets a page show, and how the
SSA and the IRS shorten an SSN. Code that must show the whole number has
it as text already.

**Personal data.** Each type names the
[personal data category](15-personal-data.md#marking-personal-fields)
its values are in, whatever field holds them:

- `financial` for a card number, an IBAN and a UK account number;
- `government_id` for a National Insurance number, an SSN and a UTR;
- `health` for an NHS number.

[Personal data](15-personal-data.md) treats a field of such a type as
marked with that category: hashed in logs, kept out of caches when the
category is sensitive, and its reads recorded. Logs carry no values,
only a failure's code and a fault, so nothing writes one in clear. The
[check of what reaches a model](21-ai.md#personal-data-sent-to-a-model)
already reads the category. A failed assertion in `jadpo test` still
shows the value it compared, which is test data.

**The validators:**

- an IBAN's mod-97, with each country's length from SWIFT's IBAN
  Registry. A country the registry adds later is refused until a release
  adds it, as a new currency is;
- an LEI's ISO 7064 mod 97-10;
- a card number's Luhn check;
- an ISIN's Luhn check over its letters as numbers;
- an ABA routing number's 3-7-1 checksum;
- an NHS number's mod-11. A check digit of 10 is never issued;
- a National Insurance number's prefix rules, from HMRC's NIM39110, with
  a suffix of A to D only;
- a BIC's country, its fifth and sixth letters, which must be a current
  ISO 3166-1 code as `Country` holds them, or Kosovo's user-assigned
  `XK`, which SWIFT issues BICs under as it does IBANs. So `YU`, which was
  withdrawn, is refused.

The rest are patterns: a card BIN of 6 or 8 digits; a payment reference
of up to 18 of Bacs's characters; an invoice number of 1 to 200
characters with no control character and no space at either end; a sort
code; an account number; a UTR; a company number; an EIN; and an SSN,
with the areas, groups and serials the SSA never issues refused.

The UK modulus check needs a sort code and an account number together, so
it belongs to `BankAccount`, under [Bank accounts](#bank-accounts).

**Test values are published ones** where a scheme publishes them:
SWIFT's registry IBANs, Stripe's test cards, the NHS Number standard's
example and its 999 test range, Vocalink's modulus checking test cases
and GLEIF's LEIs. Where a scheme publishes none, they are made up to the
rules. Each type's entry says which.

Personal data's handling of these categories isn't built yet; see
[What is built](#what-is-built).

## Cards and identity documents
<!-- topic: cards-and-identity-documents -->

Cards and identity documents are numbers a person holds: a card's brand
and last four digits, its expiry, a VAT number or tax id, a passport or a
driving licence. Each number is checked against its country's published
form, and the ones that identify a person are shown masked.

Here a shop keeps what it may of a customer's card once its payment
provider has the number, checks a supplier's VAT number by its country's
form, and records the passport a traveller shows:

```jadpo check project=cards-and-documents
type FormText = Text {}

type NotAVatNumber = Failure {
    kind: FailureKind.invalid_value
}

/// What the shop keeps of a card: its brand and last four.
function card_on_file(card: PaymentCardNumber) -> MaskedCardNumber {
    return money.mask(card)
}

/// A supplier's VAT number as its invoice prints it, with its country's
/// code.
function supplier_vat(raw: FormText) fails NotAVatNumber -> VatNumber {
    var number: VatNumber = attempt text.parse(raw, as: VatNumber) {
        malformed: NotAVatNumber
        number_of_country: NotAVatNumber
    }
    return number
}

/// A card on file as a receipt shows it.
function card_label(card: MaskedCardNumber, viewer: Locale) -> PresentationText {
    return text.format(card, locale: viewer)
}

/// A document as a booking shows it: its number's last four.
function document_label(held_document: IdentityDocument, viewer: Locale) -> PresentationText {
    return text.format(held_document, locale: viewer)
}
```

```jadpo check project=cards-and-documents file=main.test.jadpo
test "a card is kept as its brand and last four" {
    var stored_card: MaskedCardNumber = card_on_file(PaymentCardNumber("4242424242424242"))
    assert stored_card == MaskedCardNumber {
        brand: CardBrand.visa
        last_four: "4242"
    }
    assert card_label(stored_card, Locale.en_gb) == "Visa •••• 4242"
    var charge_card: MaskedCardNumber = card_on_file(PaymentCardNumber("378282246310005"))
    assert charge_card.brand == CardBrand.american_express
    var runs_until: CardExpiry = CardExpiry {
        year: 2028
        month: 12
    }
    assert text.from(runs_until) == "2028-12"
    assert text.format(runs_until, locale: Locale.en_gb) == "12/28"
}

test "a VAT number fits its country's form" {
    var supplier: VatNumber = attempt supplier_vat("DE143454214")
    assert supplier.country == Country.de
    assert supplier_vat("DE143454215") fails NotAVatNumber
    var greek_supplier: VatNumber = attempt supplier_vat("EL094259216")
    assert greek_supplier.country == Country.gr
    assert supplier_vat("GR094259216") fails NotAVatNumber
}

test "an identity document is shown by its number's last four" {
    var passport: IdentityDocument = IdentityDocument {
        kind: IdentityDocumentKind.passport
        country: Country.fr
        number: "12AB34567"
        expires: "2031-05-31"
    }
    assert document_label(passport, Locale.fr_fr) == "•••• 4567"
}
```

A number written out with its country is checked while compiling. A
French passport's number is two digits, two letters and five digits, so
this one is refused:

```jadpo check project=passport-form expect=type.invalid_literal
/// The passport a traveller showed at the desk.
function shown_at_desk() -> PassportNumber {
    return PassportNumber {
        country: Country.fr
        number: "123456789"
    }
}
```

```text output
error: This `PassportNumber` breaks its rule `number_of_country`
 --> main.jadpo:5:17
  |
5 |         number: "123456789"
  |                 ^^^^^^^^^^^ not a number of the document's country
  reason: A `PassportNumber`'s `number` is in the form its `country`'s passport office writes. A literal is checked while compiling.
  next: Write a number in the country's form, or change `country` (agent)
  rule: type.invalid_literal
  help: https://jadpo.dev/docs/errors/type.invalid_literal

check failed: 1 error, 1 file
```

### Cards

- **`CardBrand`** is the eight schemes that issue cards under ranges of
  their own worldwide: American Express, Diners Club, Discover, JCB,
  Maestro, Mastercard, UnionPay and Visa. No authority gives them codes,
  so each variant's code is its own name, `american_express`, as a
  `Continent`'s is. It is shown by the scheme's own name in every locale,
  "American Express". It isn't personal data.
- **`MaskedCardNumber`** is what a shop may keep of a card once its
  payment provider has the number: its last four digits, `last_four`, and
  its `brand`, a `CardBrand` that is `none` for a card no listed scheme
  issued. It has no single text form. Its JSON is its parts, and it is
  stored as a column per part. It is shown as a receipt prints it,
  `Visa •••• 4242`, or `•••• 1234` with no brand. It is `financial`
  personal data, as the number is.
- **`money.mask(card)` makes one** from a `PaymentCardNumber`. It is an
  operation rather than `MaskedCardNumber(card)`, because a construction
  converts only between types with one representation
  ([Creating values](02-values-and-types.md#creating-values)). It takes
  the brand from the longest issuer range the number starts with:
  Discover Global Network's published ranges for Discover, Diners Club
  International, JCB and UnionPay; Mastercard's for its 2-series; and the
  long-known ranges for Visa, American Express and Maestro.
- **`CardExpiry`** is a `year` from 2000 to 2099 and a `month`, the year
  first so that it orders as a month does. Its canonical text is ISO
  8601's `2028-12`, and only that is read. It is shown as the card prints
  it, `12/28`, in every locale; the bounds on the year keep that to one
  month. It is sent as its text and stored as two `integer` columns. It is
  `financial` personal data, as PCI DSS counts it. No operation says
  whether a card has expired on a given day.

### Document numbers

A document's number is in upper case with no spaces or punctuation, as
the payment numbers are, and it fits its country's published form. The
named rule is `number_of_country`, or `number_of_kind_and_country` for an
`IdentityDocument`:

- where a literal writes the kind and the country, the rule is proved
  while compiling, as the passport example above shows;
- where the value is built from other values, it is built with
  `attempt`;
- in text and JSON, a number that doesn't fit breaks the rule by name.

A country that publishes no form takes any number within the type's
bounds: 1 to 25 capitals and digits, the longest a US state's licence
number may be by AAMVA's 2020 card standard, or 2 to 15 for a VAT number.
So a number is never refused for its country alone. A release adds
countries as their forms are published.

### VAT numbers and tax ids

**`VatNumber`** is a `Country` and its number. Its canonical text is the
one VIES, the EU's VAT checking service, uses: the country's code, then
the number, `GB980780684`.

- Greece's code is `EL`, so `GR...` is malformed.
- Northern Ireland's `XI` isn't a country code, so a trader there has its
  `GB` number.
- It is sent as that text, stored as a column per part, and shown as it
  is, because a business prints it on its invoices. It isn't personal
  data, as a US EIN isn't.

The forms come from VIES's table for the 27 member states; HMRC's for the
United Kingdom, with its mod-97 check and the 9755 variant for newer
numbers, a branch's three digits unchecked; Norway's and Switzerland's
mod-11 organisation numbers with their `MVA` and `MWST` suffixes; and,
outside Europe, Australia's ABN, whose check the ABR publishes.

The European Commission publishes no member state's check digit. So only
Germany's (ISO 7064 MOD 11,10), France's (a key of two digits; one with a
letter is unchecked) and Ireland's check letter are checked, since every
number those authorities issue passes them. Ireland's old numbers with
`+` or `*` aren't read.

**`TaxId`** is a `Country` and its number, with no scheme. The OECD's
common reporting standard records a jurisdiction and a tax
identification number (TIN), and code that needs one scheme has that
scheme's type: `UkUtr`, `UsSsn` or `UsEin`. A number fits any of its
country's forms, from the OECD's TIN page for each country and the EU's
TIN algorithms (DG TAXUD's functional specification 5.03, 2017-02-24):

- the United Kingdom's UTR, unchecked because HMRC publishes no check, or
  its National Insurance number;
- Germany's tax identification number, with its repetition rule;
- France's numéro fiscal;
- Ireland's PPS number;
- the US SSN, ITIN and EIN, by the prefixes the IRS assigns;
- India's PAN;
- Australia's tax file number, the older eight-digit form by its length
  only because its check isn't published;
- Canada's SIN, by its Luhn check.

### Passports, licences and identity documents

**`PassportNumber`** and **`DrivingLicenceNumber`** are a `Country` and a
number.

- Passports: the United Kingdom's nine digits (HMPO); Germany's nine
  characters of its 26 (the Federal Interior Ministry); the United
  States' nine digits, or a letter and eight (the State Department); and
  France's, Ireland's, India's, Canada's and Japan's forms as their
  offices state them.
- Licences: Great Britain's by the pattern DVLA's own API checks;
  Northern Ireland's eight digits; Germany's eleven characters, its check
  digit unchecked because its algorithm is published only second-hand;
  and India's fifteen. France and Ireland publish no form, and the United
  States has none nationally, so each takes the bounds.

**`IdentityDocument`** is its `kind`, its `country`, its `number`, and
the day it `expires`, which may be `none`, as Spain's DNI for someone
over 70 never expires. The part is called `kind` because `type` is a
reserved word. The kind is `IdentityDocumentKind`, a standard enum of
`passport`, `identity_card`, `driving_licence` and `residence_permit`,
each its own code, shown as its code.

The number fits its kind and country: a passport's or a licence's forms
above, Germany's identity card, Ireland's Passport Card, and India's
Aadhaar number by Verhoeff's check (UIDAI). No residence permit's form is
published, and the United Kingdom, the United States and France publish
no identity card's.

### Masking, test values and shapes

**Masked by `text.format`, to be safe.** A `TaxId`, a `PassportNumber`, a
`DrivingLicenceNumber` and an `IdentityDocument` are shown as their
number's last four, `•••• 6789`, as a card number and an SSN are. Each is
`government_id` personal data, which identifies its holder to the state.
Code that must show the whole number reads `number`.

**Test values are published ones** where an issuer publishes them: DVLA's
licence examples, the EU's TIN samples, the OECD's PAN, the ATO's own ABN
and Stripe's test cards. Some come from a company printing its own: SAP's,
TotalEnergies', Google Ireland's and Equinor's VAT numbers. Elsewhere they
are made up to the rules. `documents::DOCUMENT_EXAMPLES` holds a valid
and an invalid number for each rule, for GB, DE, FR, IE, US and one
country outside Europe (Australia, India or Japan), for each type.

**No shapes.** `type.standard_exists` matches none of these objects,
because `{ country, number }` is too common in applications' own types to
be a sign that a type is one.

The money group's records are under
[Money and payments](#money-and-payments), and `BankAccount` under
[Bank accounts](#bank-accounts).

## Names, addresses and countries
<!-- topic: names-addresses-and-countries -->

These types hold people's names, postal addresses and email addresses,
and the closed lists every app needs: countries and their subdivisions,
languages, scripts, continents, nationalities. Each list is a built-in
enum, read by its code and shown by its name in the reader's language.

Here a shop sends parcels abroad and prints each label for the person who
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

A name and an address are stored as a column per part, so a query can
read a town or a country as it reads any column. They come back as they
were written:

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

### The lists, and where they come from

Each list is shipped with the release, generated once into
`jadpo_std::standard::places_data`:

- `Country` is ISO 3166-1's 249 assigned codes as CLDR 48.2 carries them:
  its regular regions, less the exceptionally reserved `AC`, `CP`, `CQ`,
  `DG`, `EA`, `IC` and `TA` and the user-assigned `XK`. Its alpha-3 and
  numeric codes come from CLDR's code mappings. Its retired codes are the
  eleven ISO 3166-3 codes CLDR marks deprecated (`AN`, `BU`, `CS`, `DD`,
  `FX`, `NT`, `SU`, `TP`, `YD`, `YU`, `ZR`), shown by ISO 3166-3's English
  names, because CLDR names none of them.
- `CountrySubdivision` is ISO 3166-2 as CLDR 48.2 carries it: 5,027
  codes, with the 626 that CLDR marks deprecated retired.
- `Language` is ISO 639-1's 183 codes, from the Library of Congress's ISO
  639-2 list (2026-10-18). Its alpha-3 form is ISO 639-2/T. `in`, `iw`,
  `ji`, `jw` and `mo` are retired. A language with only a three-letter
  code isn't in the list.
- `WritingScript` is ISO 15924's 224 codes (2026-10-18), less the
  private-use range. Its numeric form is ISO 15924's number.
- `Continent` is UN M49's five regions, and Antarctica, which M49 lists
  beside them. It follows the authority's list, so the Americas are one
  region. Its codes are digits, so each variant's code is its own name, as
  a `Weekday`'s is, and M49's number is its numeric form.
- `TextDirection` is the enum `locale.direction` gives. Its codes are
  `ltr` and `rtl`, as HTML's `dir` attribute writes them.
- `Nationality` takes `Country`'s codes and names, as a passport names a
  nationality by its country. CLDR has no names for nationalities, and
  ICAO's codes for travel documents belong to `IdentityDocument`.
- `Currency` also holds the six codes ISO 4217 has withdrawn since 2021
  (`ANG`, `BGN`, `CUC`, `HRK`, `SLL`, `ZWL`), retired.

### Closed lists are enums

A standard type is a built-in enum when its list is closed, published by
an authority and rarely changes, as `Locale`, `Zone` and `Weekday` are. A
huge or fast-changing list stays text with a `pattern:`, and is checked
against its list where this page says so: `IataAirport`, `PostalCode`,
`HsCode`, `UnLocode`, `Icd10Code`, `SpdxLicence`, `Emoji`.

- `HttpStatus` stays a number, because its registry is open by design: an
  unknown code is still a status.
- `CallingCode` stays text, because its codes are digits.
- `CountrySubdivision`, about 5,000 codes with a few changes a year, is
  an enum. Validating an address needs the list anyway, CLDR names each
  subdivision, and its changes arrive with releases, as the time-zone
  database's do.

Jadpo doesn't match names such as "England" or "UK" to codes, in
`text.parse` or in any operation. Which name means which code is a
judgement, and it differs by locale: England is
`CountrySubdivision.gb_eng`, not a country. A dropdown built from `.all`
already asks a person to pick. A package can offer name matching if an
import needs it.

### Variants and `.all`

A variant is its code in lower case with `_` for `-`:
`CountrySubdivision.gb_eng`, as `Locale` writes its variants. A code that
is a [reserved word](01-project-layout-and-modules.md#naming), or is
`all`, takes a trailing `_`: `Country.in_`, `Language.or_`.

**`.all`** lists an enum's variants in its order, a closed list's by
code, as a `List` of the enum. Retired codes are left out, because a
dropdown never offers Yugoslavia. On an enum whose variants carry data,
`.all` is `type.unknown_enum_variant`, and a variant called `all` is
`type.reserved_variant_name`.

### Retired codes

A retired code is refused where code writes it as a value, comparisons
included: `type.variant_retired`. You can decline it, and the decline's
reason says why the code still needs the old value. A `match` arm on a
retired code isn't refused, because it only reads, and a `match` lists
every variant anyway ([Match](03-logic.md#match)).

### Reading codes: `form:`

`form:` picks which of a list's code systems to read or write, as a
`CodeForm` written out: `CodeForm.alpha3`, `CodeForm.numeric`. A value
held in a variable is `type.mismatch`. A form the type has no codes in is
`type.temporal_option_unused`, as an option that means nothing for a type
is.

`text.parse` reads a form's code ignoring case. Where a retired variant
shares a code with a current one, the code reads as the current one:
`ind` is `Language.id`, not `Language.in_`.

At a boundary, such as a request, JSON, a CSV cell or a setting, a
standard enum's code is read ignoring case and written back in its
canonical case. No two codes in these lists differ only by case, so that
is safe. An unknown code is `invalid_value` naming the field, with the
rule `variant`. Spaces and other spellings are still refused.

### Stored and shown

**Stored:** a closed list's column holds its code, as `Currency`'s does:
`GB`, and `IN` rather than `in_`. An object is stored as a column per
part, named `<field>_<part>`. A part that may be `none` is a column that
may be `NULL`. A `PostalAddress`'s lines are a `text[]` column, and its
country is its code.

**Shown:** a country, a continent, a language and a script by CLDR's name
in the reader's locale, through ICU4X's display names, and a withdrawn
code by its list's English name. A subdivision is shown by CLDR's English
name in every locale, because CLDR names few in other languages and
ICU4X doesn't include them. A continent is shown by CLDR's name for its
M49 region, and Antarctica by its territory's name.

### People's names

**`PersonName`** is a given name and, where the person has one, a family
name. A single name is a given name, as CLDR holds a mononym, so no rule
across the fields is needed. It has no single text form, and its JSON is
its parts.

`text.format` applies CLDR's medium, formal pattern for referring to a
person, from CLDR 48.2's person-name data:

- the name's locale is the reader's when the name is in the reader's
  script; otherwise it is the likeliest locale for the name's script,
  which then formats it;
- the reader's locale's lists give the order of the parts;
- the name's locale gives the space between them, none in Japanese.

Jadpo applies the patterns itself. ICU4X's formatter is experimental,
leaves a pattern's literals behind ("Ada Lovelace ,") and doesn't work out
the order.

### Addresses

**`PostalAddress`** is its lines as written, then a locality, a region
and a postal code, each one line that may be `none`, and its `Country`.
The region is text as the envelope writes it, not a
`CountrySubdivision`. A dependent locality or a sorting code goes in the
lines, and the recipient's name is your application's own field.

CLDR has no address formats, so `text.format` uses libaddressinput's,
Google's formats from UPU data (Apache-2.0, commit `81eb962` of
2026-05-15):

- a reader of one of the country's languages gets the country's own
  format, and anyone else its Latin-script format;
- the text between two fields appears only where the fields beside it
  are there;
- the country's CLDR name comes last for a reader in another country.

Nothing is put in capitals. UPU's capitals are for the envelope, not for
people reading.

**`EmailWithName`** is a name that may be `none` and an `Email`. Its
canonical text is RFC 5322's mailbox with RFC 6532's UTF-8:
`Ada Lovelace <ada@example.com>`. The name is quoted only where it must
be, and an address with no name is written alone. Only that text is
read. Its JSON is its parts.

### Lines and other text

**A line**, an object's part of text, is 1 to 200 characters with no
control character and no space at either end, as an invoice number is. A
literal is checked while compiling.

These are held as text:

- `GivenName` and `FamilyName` are a line;
- `Username` is `[a-z0-9_.-]{1,64}`;
- `PhoneExtension` is up to 20 digits, the longest libphonenumber reads
  after `ext.`;
- `Uprn`, a UK property's unique reference number, is 1 to 12 digits with
  no leading zero, held as text so that no digit is lost.

### Personal data and shapes

**Personal data:** a name, a given or family name and a username are
`identifying`; a mailbox, an address, an extension and a UPRN are
`contact`; a nationality is `demographic`. The lists of places aren't
personal data.

**No shapes:** `type.standard_exists` matches none of the objects,
because `{ name, email }` and `{ first_name, last_name }` are too common
in applications' own types to be a sign that a type is one.

## Phone numbers and postal codes
<!-- topic: phone-numbers-and-postal-codes -->

A phone number is kept in its international form and a postal code in
its country's form, but people type both in their own way. These types
read what people type, given the country they typed it in, and show a
phone number as the reader's region dials it.

Here a shop takes a customer's phone number and postcode as they type
them in their own country, and shows the number to its staff as their
region dials it:

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

`PhoneNumber`, `CallingCode`, `PostalCode` and `UnLocode` are held as
text, as the payment numbers are. Each is a pattern and, where it needs
one, a named validator, and its canonical text is the value. None is
ordered.

### Phone numbers

A **`PhoneNumber`** is its E.164 text, the international form with no
spaces: `+447400123456`. It is valid when libphonenumber's
`isValidNumber`, the stricter of its two checks, says so: the national
number matches one of its region's number types (fixed line, mobile,
freephone and so on) with that type's lengths, not merely a length the
region uses.

Its data is libphonenumber's `PhoneNumberMetadata.xml`, release v9.0.41
(2026-10-08, Apache-2.0), generated once into
`jadpo_std::standard::phones_data`. A range a regulator opens later is
refused until a release brings newer data, as a new currency is. A phone
number is `contact` personal data.

**`text.parse` reads E.164 only,** as it reads only canonical text
everywhere. To read a number as people write it, pass **`country:`**, a
`Country` value, the country it was typed in. Then `text.parse` reads:

- a national number with its prefix, `07400 123456`;
- an international number after `+` or the region's international
  prefix, `00 44 ...` or `011 44 ...`;
- a national prefix written after the country code, `+44 (0)7400 ...`,
  which it leaves out;
- spaces, dashes, dots, slashes and brackets anywhere.

A number valid in another region with the same calling code is taken,
such as a Jersey number typed in Britain. An extension belongs in a
`PhoneExtension`, so text with one is `malformed`. `country:` on a type
other than `PhoneNumber` and `PostalCode` is `type.temporal_option_unused`,
as an option that means nothing for a type is.

**`text.format` picks a phone number's format from the locale,** with no
option to set. A reader whose region has the number's calling code gets
the national format, `07400 123456`. The region is the locale's, or its
likeliest one, as for an address's reader. Any other reader gets the
international format, `+44 7400 123456`. Both use libphonenumber's formats
for the calling code's main region.

A **`CallingCode`** is a code ITU-T assigned, with its `+`: `+44`, `+1`
for the whole North American plan, `+800` and the other non-geographic
codes. The list is the one libphonenumber's data holds. It isn't
personal data.

### Postal codes

A **`PostalCode`** on its own is text any country's postal code could be:
1 to 12 capitals, digits, spaces and dashes, with a letter or digit at
each end. Its country's rule applies where the country is known: in a
`PostalAddress`, and in `text.parse(raw, as: PostalCode, country:)`.

The patterns are libaddressinput's, from the commit the address formats
come from (Apache-2.0), in `postal_codes_data`. Where a pattern leaves a
separator optional but the country's own examples all write it one way,
the separator is fixed: a space in `SW1A 1AA`, a dash in `154-0023` and
`22162-1010`, none in `75004`. So each postal code has one text, and `==`
holds.

With `country:`, a postal code is read as people type it: in capitals,
with one space between words and the separators where the country writes
them, so `sw1a1aa` is `SW1A 1AA`. Text that could be read two ways is
refused. A country libaddressinput has no pattern for, such as Ireland,
whose Eircodes it doesn't hold, takes any postal code. A postal code is
`contact` personal data, as an address is.

**A `PostalAddress`'s postal code must fit its country.** The rule is
`postal_code_of_country`. An object literal never fails
([Objects](02-values-and-types.md#objects)), so the rule is proved while
compiling where a literal writes both the postal code and the country
(`type.invalid_literal`). An address built in code from other values is
built with `attempt`. JSON, in a request or through `json.read`, breaks
the rule by name at the address's path. Stored addresses are read as
they were written.

### UN/LOCODEs

A **`UnLocode`** is a UN code for a trade or transport location: five
characters with no space, `GBLON`, its first two a current ISO 3166-1
country code. It is printed with a space, `GB LON`.

The UN/LOCODE list isn't shipped, so a code isn't checked against it.
UNECE's site terms allow only personal, non-commercial use without
redistribution, while its footer says UN/CEFACT standards are under
CC BY 4.0, and it doesn't say which covers the code list. The
`datasets/un-locode` package records this.

**No shapes:** `type.standard_exists` matches none of these four types,
because each is a single value.

## Commerce, travel and health codes
<!-- topic: commerce-travel-and-health-codes -->

These are the codes trade, transport and healthcare use: barcodes,
ISBNs, container numbers, delivery terms, airport codes, blood types,
diagnosis codes. Each is checked by its standard's pattern or check
digit, and the closed lists are enums.

Here a till scans a product's barcode, a supplier's order names its
delivery term in whatever case its system writes, and a dock prints each
container's number as its side is painted:

```jadpo check project=codes-and-lists
type ScannedText = Text {}

type NotAProductCode = Failure {
    kind: FailureKind.invalid_value
}

type UnknownTradeTerm = Failure {
    kind: FailureKind.invalid_value
}

/// The product a till's scanner read: 13 digits, the last GS1's check
/// digit.
function scanned_product(raw: ScannedText) fails NotAProductCode -> Ean13 {
    var till_scan: Ean13 = attempt Ean13(raw) {
        format: NotAProductCode
    }
    return till_scan
}

/// The delivery term a supplier's order names, by its Incoterms code.
function agreed_term(raw: ScannedText) fails UnknownTradeTerm -> Incoterm {
    var trade_term: Incoterm = attempt text.parse(raw, as: Incoterm) {
        malformed: UnknownTradeTerm
    }
    return trade_term
}

/// A container on the dock's manifest, as its side is painted.
function container_label(shipped_in: ContainerNumber, viewer: Locale) -> PresentationText {
    return text.format(shipped_in, locale: viewer)
}
```

```jadpo check project=codes-and-lists file=main.test.jadpo
test "a code is checked by its check digit as it is built" {
    var till_scan: Ean13 = attempt scanned_product("4006381333931")
    assert till_scan == Ean13("4006381333931")
    assert scanned_product("4006381333932") fails NotAProductCode
    assert scanned_product("400638133393") fails NotAProductCode
    assert container_label(ContainerNumber("CSQU3054383"), Locale.en_gb) == "CSQU 305438 3"
    assert text.format(UnNumber("UN1203"), locale: Locale.en_gb) == "UN 1203"
}

test "a closed list reads its codes in either case" {
    var trade_term: Incoterm = attempt agreed_term("fob")
    assert trade_term == Incoterm.fob
    assert agreed_term("Free On Board") fails UnknownTradeTerm
    assert text.from(BloodType.o_negative) == "O-"
    assert text.format(CabinClass.premium_economy, locale: Locale.fr_fr) == "Premium economy"
    assert list.count(Incoterm.all) == 11
    assert list.count(Allergen.all) == 14
}
```

### Codes held as text

The codes are held as text, as the payment numbers are. Each is a pattern
and, where the table under
[Groups with the baseline only](#groups-with-the-baseline-only) names
one, a named validator. Its canonical text is the value, in upper case
where the standard writes it so and never read in another case. None is
ordered.

### Check digits

Where each check comes from:

- GS1's mod 10 (General Specifications 7.9) for `Ean13`, `UpcA`, `Sscc`
  and `Isbn`. An ISBN is its 13 digits with the prefix 978 or 979 and no
  hyphens, because where the hyphens go depends on the ISBN agency's
  ranges.
- ISO 6346's for `ContainerNumber`.
- ISO 7064 MOD 11-2 for `Orcid`.
- Verhoeff's for `SnomedConcept`, with a concept's partition identifier
  (`00`, or `10` in an extension), so a description's identifier is
  refused.
- Elexon's primes for an `Mpan` core.
- A `Vin`'s check digit is checked where 49 CFR 565 requires it: for a
  manufacturer whose identifier starts 1 to 5, in North America.
  Elsewhere ISO 3779 leaves the ninth character to the manufacturer, and
  European VINs often don't carry a check digit, so checking every VIN
  would refuse real ones.

### Patterns only

- a `Sku` is 1 to 64 characters with no space or control character, kept
  as written;
- a `LotNumber` and a `SerialNumber` are 1 to 20 characters of GS1's
  character set 82 (Application Identifiers 10 and 21);
- an `HsCode`, a customs tariff code, is six to ten digits from chapter
  01, as a tariff writes it without dots;
- a `Carrier` is its Standard Carrier Alpha Code, two to four capitals;
- an `Icd10Code` is its letter, two digits and, after the dot, up to four
  more, which covers ICD-10-CM;
- an `IataAirline` is two letters, or a letter and a digit;
- a `BookingReference` is six capitals and digits;
- a `UnNumber`, which names a dangerous good, is `UN` and four digits;
- an `Mprn`, a UK gas supply's number, is 6 to 10 digits.

### Lists that aren't shipped

IATA's airport and airline codes and ICAO's location indicators are sold
under licence. WHO's ICD-10 may be used, but not redistributed in
software without its permission. So `IataAirport`, `IcaoAirport`,
`IataAirline` and `Icd10Code` are checked by pattern only, as `UnLocode`
is. An application that holds a licence checks against its own copy.

A SNOMED CT concept's term belongs to its release, under SNOMED
International's licence, so `SnomedConcept` is its identifier alone,
without a term.

### Shown to people

A container number is shown as its side is painted, `CSQU 305438 3`, and
a UN number as a transport document writes it, `UN 1203`. The rest are
shown as their canonical text.

### The closed lists

Each of these is a built-in enum, sorted by its code. Its code is read
ignoring case and stored as written (`O-`, `FOB`):

- `Incoterm`: Incoterms 2020's eleven delivery terms, with `DAT`, `DAF`,
  `DES`, `DEQ` and `DDU` from earlier editions retired;
- `BloodType`: ABO and RhD, `A+` to `O-`, whose variants spell the sign
  out, `BloodType.o_negative`;
- `Allergen`: the 14 that the UK's Food Information Regulations 2014 and
  the EU's Regulation 1169/2011 name, without regional additions;
- `CabinClass`: economy, premium economy, business and first;
- `CefrLevel`: language levels `A1` to `C2`;
- `EpcRating`: energy performance bands `A` to `G`, without a score;
- `Tenure`: freehold, leasehold and commonhold, as HM Land Registry
  registers land in England and Wales; a share of a freehold is a
  leasehold with a share;
- `LawfulBasis`: GDPR Article 6(1)'s six lawful bases.

Where codes are words (`premium_economy`), each variant's code is its
own name, as `Continent`'s are.

**Shown by code or by the authority's wording.** CLDR names none of these
lists. A list whose codes are written the same in every language (`FOB`,
`B2`, `O-`, `C`) is shown by its code. A list whose codes are words is
shown by its authority's English wording in every locale
(`FormatKey::Names`, as a card brand is), just as a subdivision is shown
by its English name: "Premium economy", "Cereals containing gluten",
"Legitimate interests". To show them in another language, word them in
your app's own messages.

**None is ordered,** not even `CefrLevel` and `EpcRating`. A `match` can
say their order. The contract orders only what it lists.

### Personal data and test values

**Personal data:** an `Icd10Code`, a `SnomedConcept` and a `BloodType`
are `health`; an `Orcid` is `identifying`, as a username is; an `Mpan` and
an `Mprn` are `contact`, as a `Uprn` is, because each names a home's
supply. An `Allergen` isn't personal data: a menu's allergens describe
food, and a person's allergy is your application's own field, which you
mark.

**Test values are published ones** where a standard publishes them:
GS1's, the ISBN Users' Manual's, NHTSA's, ISO 6346's, ORCID's, SNOMED
CT's, and UPS's sample tracking number. Where a standard publishes none,
they are made up to the rules. Each type's entry says which.

## Records and health measures
<!-- topic: records-and-health-measures -->

These types are the records that sit beside an app's own data: a
parcel's size and tracking number, a flight, a clothing size, a vehicle
registration, a rating, a barcode. With them are the health measures a
clinic records: a pulse, a blood pressure, a glucose level, and a
prescription's dose.

Here a courier labels each parcel with its size and its carrier's
tracking number, a travel desk reads the flight a booking names, and a
library reads the ten-digit ISBNs its old catalogue lists as the books'
13-digit ones:

```jadpo check project=records
type ScannedText = Text {}

type NotABook = Failure {
    kind: FailureKind.invalid_value
}

type NotAFlight = Failure {
    kind: FailureKind.invalid_value
}

/// A book the old catalogue lists by its ISBN-10, read as its `Isbn`.
function catalogued_book(raw: ScannedText) fails NotABook -> Isbn {
    var book: Isbn = attempt text.parse(raw, as: Isbn, form: CodeForm.isbn10) {
        malformed: NotABook
    }
    return book
}

/// The flight a booking names, as the airline writes it.
function booked_flight(raw: ScannedText) fails NotAFlight -> FlightNumber {
    var ba_flight: FlightNumber = attempt text.parse(raw, as: FlightNumber) {
        malformed: NotAFlight
    }
    return ba_flight
}

/// A parcel's size as its label prints it.
function parcel_size(shipped_box: Dimensions, packer: Locale) -> PresentationText {
    return text.format(shipped_box, locale: packer)
}

/// The same in inches, for a carrier that measures in them.
function parcel_in_inches(shipped_box: Dimensions, packer: Locale) -> PresentationText {
    return text.format(shipped_box, locale: packer, unit: LengthUnit.inch)
}
```

```jadpo check project=records file=main.test.jadpo
test "an ISBN-10 is read as its 13 digits" {
    var book: Isbn = attempt catalogued_book("0306406152")
    assert book == Isbn("9780306406157")
    var with_x: Isbn = attempt catalogued_book("080442957X")
    assert with_x == Isbn("9780804429573")
    assert catalogued_book("0306406153") fails NotABook
    assert catalogued_book("0-306-40615-2") fails NotABook
}

test "a flight is written one way" {
    var ba_flight: FlightNumber = attempt booked_flight("BA123")
    assert ba_flight.airline == IataAirline("BA")
    assert ba_flight.number == 123
    assert ba_flight.suffix == none
    assert booked_flight("BA0123") fails NotAFlight
    assert booked_flight("BA 123") fails NotAFlight
}

test "a parcel is labelled in the carrier's units" {
    var shipped_box: Dimensions = Dimensions {
        length: 12inch
        width: 9inch
        height: 4inch
    }
    assert parcel_size(shipped_box, Locale.en_gb) == "30.48 × 22.86 × 10.16 cm"
    assert parcel_in_inches(shipped_box, Locale.en_us) == "12 × 9 × 4 in"
    var ups_label: TrackingNumber = TrackingNumber {
        carrier: "UPSN"
        number: "1Z999AA10123456784"
    }
    assert text.from(ups_label) == "UPSN 1Z999AA10123456784"
}

test "the other records are read and shown by their standards" {
    var size: ClothingSize = ClothingSize {
        system: ClothingSizeSystem.uk
        size: "10"
    }
    assert text.from(size) == "UK 10"
    var munich_plate: VehicleRegistration = VehicleRegistration {
        country: Country.de
        plate: "M-AB 1234"
    }
    assert text.from(munich_plate) == "DE M-AB 1234"
    var stars: Rating = Rating {
        value: 4.5
        best: 5.0
        worst: 1.0
    }
    assert text.format(stars, locale: Locale.en_gb) == "4.5/5.0"
    var flour: UnitQuantity = UnitQuantity.mass {
        amount: 2.5kg
    }
    assert text.format(flour, locale: Locale.en_gb) == "2.5 kg"
    var scanned: Barcode = Barcode.ean13 {
        data: "4006381333931"
    }
    assert text.format(scanned, locale: Locale.en_gb) == "4006381333931"
    var applies_in: Jurisdiction = Jurisdiction.subdivision {
        subdivision: CountrySubdivision.gb_eng
    }
    assert text.format(applies_in, locale: Locale.en_gb) == "England"
}
```

Here a clinic shows each reading as a clinician reads it, whatever unit
a monitor sent it in, and keeps a prescription's instruction as its
parts, never as free text to read back:

```jadpo check project=health-measures
type MonitorText = Text {}

type MeterReading = Decimal {
    scale: 1
}

type BadPulse = Failure {
    kind: FailureKind.invalid_value
}

type BadGlucose = Failure {
    kind: FailureKind.invalid_value
}

/// A pulse as a monitor sends it, `72 bpm`: no unit, the wrong kind and a
/// pulse past 300 beats a minute each fail.
function monitored_pulse(raw: MonitorText) fails BadPulse -> HeartRate {
    var pulse: HeartRate = attempt text.parse(raw, as: HeartRate) {
        malformed: BadPulse
        unknown_unit: BadPulse
        min: BadPulse
        max: BadPulse
    }
    return pulse
}

/// A glucose level as a UK meter gives it, in millimoles a litre to one
/// place.
function metered_glucose(meter_value: MeterReading) fails BadGlucose -> BloodGlucose {
    var metered_level: BloodGlucose = attempt BloodGlucose(
        units.of(meter_value, ConcentrationUnit.millimole_of_glucose_per_litre)
    ) {
        min: BadGlucose
    }
    return metered_level
}

/// A reading as the chart shows it.
function pressure_shown(chart_reading: BloodPressure, viewer: Locale) -> PresentationText {
    return text.format(chart_reading, locale: viewer)
}

/// A glucose level in millimoles a litre, as a UK meter reads it.
function glucose_shown(glucose_level: BloodGlucose, viewer: Locale) -> PresentationText {
    return text.format(glucose_level, locale: viewer)
}

/// The same in milligrams a decilitre, as a US laboratory reports it.
function glucose_for_us_lab(glucose_level: BloodGlucose, viewer: Locale) -> PresentationText {
    return text.format(
        glucose_level,
        locale: viewer,
        unit: ConcentrationUnit.milligram_per_decilitre
    )
}

/// How to take a medicine, as the label prints it.
function label_line(leaflet_dose: DosageInstruction, viewer: Locale) -> PresentationText {
    return text.format(leaflet_dose, locale: viewer)
}
```

```jadpo check project=health-measures file=main.test.jadpo
test "a pulse is read in either unit and shown in beats a minute" {
    var pulse: HeartRate = attempt monitored_pulse("72 bpm")
    var in_hertz: HeartRate = attempt monitored_pulse("1.2 Hz")
    assert pulse == in_hertz
    assert pulse == HeartRate(72bpm)
    assert text.format(pulse, locale: Locale.en_gb) == "72 bpm"
    assert text.from(pulse) == "1.2 Hz"
    assert monitored_pulse("72") fails BadPulse
    assert monitored_pulse("72 mmHg") fails BadPulse
    assert monitored_pulse("400 bpm") fails BadPulse
}

test "a reading is shown as a clinician reads it" {
    var at_rest: BloodPressure = BloodPressure {
        systolic: 120mmHg
        diastolic: 80mmHg
    }
    assert pressure_shown(at_rest, Locale.en_gb) == "120/80 mmHg"
    var in_kilopascals: BloodPressure = BloodPressure {
        systolic: 16kPa
        diastolic: 10.7kPa
    }
    assert pressure_shown(in_kilopascals, Locale.en_gb) == "120/80 mmHg"
    var fasting: BloodGlucose = attempt metered_glucose(5.5)
    assert glucose_shown(fasting, Locale.en_gb) == "5.5 mmol/L"
    assert glucose_for_us_lab(fasting, Locale.en_us) == "99 mg/dL"
    assert metered_glucose(-1.0) fails BadGlucose
}

test "an instruction is kept as its parts" {
    var twice_daily: DosageInstruction = DosageInstruction {
        dose: Dose.mass {
            amount: 500mg
        }
        route: RouteOfAdministration.oral
        times: 2
        per: 24h
        as_needed: false
    }
    assert label_line(twice_daily, Locale.en_gb) == "500 mg, Oral use, 2 times per 1 day"
    var when_needed: DosageInstruction = DosageInstruction {
        dose: Dose.volume {
            amount: 10ml
        }
        route: RouteOfAdministration.oral
        times: 1
        per: 6h
        as_needed: true
    }
    assert label_line(
        when_needed,
        Locale.en_gb
    ) == "10 ml, Oral use, 1 time per 6 hours, when needed"
}
```

A reading written out with its pressures the wrong way round is refused
while compiling, because the pressure between beats is always below the
pressure as the heart beats:

```jadpo check project=pressure-order expect=type.invalid_literal
/// A reading typed into the chart.
function typed_reading() -> BloodPressure {
    return BloodPressure {
        systolic: 80mmHg
        diastolic: 120mmHg
    }
}
```

```text output
error: This `BloodPressure` breaks its rule `diastolic_below_systolic`
 --> main.jadpo:5:20
  |
5 |         diastolic: 120mmHg
  |                    ^^^^^^^ not below the systolic
  reason: A `BloodPressure`'s `diastolic`, the pressure between beats, is below its `systolic`, the pressure as the heart beats (LOINC 85354-9). A literal is checked while compiling.
  next: Write a `diastolic` below the `systolic`, or swap the two (agent)
  rule: type.invalid_literal
  help: https://jadpo.dev/docs/errors/type.invalid_literal

check failed: 1 error, 1 file
```

### Health measures

Three supporting enums go with these types: `ClothingSizeSystem` and
`ShoeSizeSystem`, the systems a size is in, and `RouteOfAdministration`,
the routes a dose is taken by.

**Four units** serve the measures:

- `PressureUnit.millimetre_of_mercury`, `mmHg`, 133.322387415 Pa as CLDR
  converts it, with the literal suffix `120mmHg`;
- `FrequencyUnit.beat_per_minute`, `bpm`, 1/60 Hz, with the literal
  suffix `72bpm`;
- `ConcentrationUnit.milligram_per_decilitre`, `mgdL`, 0.01 g/L;
- `ConcentrationUnit.millimole_of_glucose_per_litre`, for glucose alone,
  whose code is its name: 0.1801557 g/L, by glucose's molar mass of
  180.1557 g/mol, as CLDR converts its
  `milligram-ofglucose-per-deciliter`.

The two concentration units have no literal suffix, because theirs would
hold a `/`. Each unit is read by its code. Text and the wire still write
a quantity in its base unit.

**The measures are quantities,** each held in its kind's base unit:

- `HeartRate` is a `Frequency` from 0 to 300 beats a minute, shown in
  whole beats a minute. A frequency is held to the millihertz, so 73 bpm
  is held as 1.217 Hz and shown as 73 again.
- `BloodPressure` is a `systolic` and a `diastolic` `Pressure`, neither
  below 0, the diastolic below the systolic. It is shown `120/80 mmHg`, in
  whole millimetres of mercury.
- `BloodGlucose` is a `Concentration`, never below 0, shown in millimoles
  of glucose a litre to one place, or in whole milligrams a decilitre with
  `unit:`.

`text.format` takes `unit:` for each, and a value made in another unit,
such as `16kPa`, is shown in the clinician's. They are a person's
`health` data ([Personal data](15-personal-data.md)).

### Doses and instructions

**`Dose`** is a choice of an amount by `mass` or by `volume`, at least a
microgram or a microlitre, the smallest step each kind holds. It is shown
in milligrams or millilitres exactly, so a microgram dose is never shown
in a unit a reader could take for a milligram. A count of tablets or of
units of insulin is your application's own type.

**`DosageInstruction`** is the commonest form of FHIR's `Dosage`, held as
parts, never as free text to read back: a `dose`, a `route`, `times` (at
least once) `per` a `Duration` above nothing, and `as_needed`. It is
shown in English, as a route's name is: `500 mg, Oral use, 2 times per 1
day`. To show it in another language, word it in your app's own
messages.

A dose and an instruction describe a medicine, so neither is personal
data. A patient's prescription is your application's record, which you
mark.

**`RouteOfAdministration`** is the 14 routes of EDQM's Standard Terms
that a leaflet most often names. Each variant's code is its own name, and
it is shown as EDQM words it ("Oral use"). EDQM's numeric codes aren't
used.

### Sizes

**The size systems** are the UK's, continental Europe's and the US's,
written `UK`, `EU` and `US`. Mondopoint (ISO 9407) and other national
systems come with a later release.

A size is the label in its system, not a measurement. A clothing size is
a number of up to three digits, the US's `00`, or `XXS` to `5XL`. A shoe
size is a whole or half size. Whose scale it is on, women's, men's or
children's, belongs to the product, as Google's and schema.org's product
data hold it. Each size is written as the label writes it, `UK 10` or
`EU 42`, and stored as a column each.

### Barcodes, amounts and ratings

**`Barcode`** is a choice by symbology:

- `ean13` and `upc_a`, whose data is the code type of that name, with its
  GS1 check digit;
- `code128`, 1 to 80 printable ASCII characters;
- `qr_code`, 1 to 2,953 characters of ISO 8859-1 in byte mode, as many as
  a version 40 symbol holds.

It is sent tagged and shown as its data. Other symbologies come with a
later release.

**`UnitQuantity`** is a choice of a count, `each` (UN/ECE Recommendation
20's `EA`), or an amount by `mass`, `volume`, `length` or `area`, the
kinds `UnitPrice` prices by. None is below 0. It is shown in its kind's
base unit, as `UnitPrice` is. Recommendation 20's list of trade units
isn't adopted.

**`Rating`** is schema.org's `value`, `best` and `worst`: the value is on
the scale, and the best is above the worst. It is held as written, so 4.5
of 5 and 9 of 10 are different values, and shown `4.5/5`.

**Rules across parts:** a rating's `best_above_worst` and
`value_on_scale`, and a blood pressure's `diastolic_below_systolic`. Each
is proved while compiling for a literal that writes both parts as numbers,
or as quantities of one kind (`type.invalid_literal`). For a value built
in code from other values it is proved or attempted. It is checked as
JSON is read.

### Parcels, vehicles and flights

**`Dimensions`** is a `length`, `width` and `height`, none below 0. It is
sent as named fields and stored as a column each. It is shown in
centimetres, or in the unit `unit:` names, with the unit written once:
`30 × 20 × 10 cm`. A parcel's weight is a `Mass` field of your own beside
it.

**`TrackingNumber`** is a `Carrier` and 6 to 40 capitals and digits,
which covers the large carriers' published forms. A carrier's own form
and check digit aren't checked.

**`VehicleRegistration`** is a `Country` and a mark of up to four groups
of capitals, letters of a script with no case, or digits, joined by a
space or a hyphen. It is kept as written, because a separator is part of
some marks, such as a German district's code.

A tracking number and a registration are each written as the code, a
space and the rest, and stored as a column each. Neither is personal
data, as a `Vin` isn't.

**`FlightNumber`** is IATA's SSIM designator: an `IataAirline`, a number
from 1 to 9999, and an optional capital suffix. It is written with no
space or leading zero, `BA123`, and stored as a column each. A flight on
a date is your application's record of a `FlightNumber` and a
`CalendarDate`.

### Jurisdictions and models

**`Jurisdiction`** is a choice of a `country` or a `subdivision`, sent
tagged and shown by its CLDR name. Like every choice, it has no single
text form.

**`ModelId`** names an AI model: a `provider` and a `name`, written as
Hugging Face writes a repository, `meta-llama/Llama-3.1-8B-Instruct`. It
is kept as written, and the name may hold the `:` and `@` of a version
tag. It is stored as a column each.

### Text forms and ISBN-10

**Text forms:** `TextForm::Joined` writes two parts with a separator, the
first an enum's code, a standard type's text or a pattern's.
`TextForm::FlightNumber` writes IATA's designator.

**ISBN-10:** to read an old ten-digit ISBN, use
`text.parse(raw, as: Isbn, form: CodeForm.isbn10)`. Like `Country`'s
`form:`, it only reads. It takes nine digits and a check character, a
digit or `X` in either case, with no hyphens, and gives the ISBN-13: `978`,
the first nine digits, and GS1's check digit. A wrong check character is
`malformed`. `text.from` never writes an ISBN-10: asking it to is
`type.temporal_option_unused`, as asking for a form a list doesn't have
is.

## Logic and bits
<!-- topic: logic-and-bits -->

`and`, `or` and `not` are the logical operators
([Expressions and operators](03-logic.md#expressions-and-operators)).
Exclusive or on `Bool` is `!=`. There are no other logic gates; any other
gate is a one-line function:

```jadpo fragment
function nand(a: Bool, b: Bool) -> Bool {
    return not (a and b)
}
```

There are no bitwise operations on `Int` or `Bytes`: no and, or, xor,
not or shifts. Hold flags as a `Set` of an enum instead. Bit-packed data,
such as a device's payloads, is decoded by the provider package that
knows the device's format.

## Geospatial types
<!-- topic: geospatial-types -->

The geospatial types hold places and shapes on the Earth: a point, a
line, a polygon, a bounding box, a circle, a map tile. All 17 are in
WGS84, the coordinate system GPS uses, and every coordinate is a named
field, never a pair of numbers whose order you have to remember.

[Geospatial](20-geospatial.md) covers the work you do with them: the
`geo.` operations, from measures and predicates to building values with
`geo.from_geojson`, `geo.from_lng_lat`, `geo.from_lat_lng`, and WKT's
`geo.from_wkt` and `geo.to_wkt`; storage in PostGIS `geography` columns;
query conditions and their indexes; and GeoJSON's `Feature` and
`FeatureCollection`. Routing and geocoding come from provider packages.
This section describes the types themselves.

### Points and coordinates

A **`Point`** is a `latitude`, a `longitude`, and an `altitude` that may
be `none`.

- The latitude and longitude are the standard `Latitude` and `Longitude`,
  each a `Decimal` with no `scale:`. Each is held as the shortest decimal
  of the nearest IEEE double, as PostGIS stores it
  ([Precision and determinism](20-geospatial.md#precision-and-determinism)).
  So `51.50720000001234567` is kept as `51.50720000001235`, and every
  coordinate of up to 15 significant digits is kept unchanged.
- A latitude runs from -90 to 90 and a longitude from -180 to 180, both
  ends included. So -180 and 180 are both the antimeridian, as RFC 7946
  takes them.
- The altitude is a `Length` of either sign: the height above the WGS84
  ellipsoid, which is what RFC 7946's third coordinate is.

A **`Bearing`** is a direction in degrees clockwise from true north, from
0 to 360, both ends included, since no bound leaves its end out. 360 and
0 are the same direction but different numbers, so an application that
compares bearings writes north one way. A wind's direction is a
`Bearing`: the direction it comes from, as WMO reports it.

### Geometries

A **`Geometry`** is a choice of six variants. Each holds its geometry
under the variant's own name: `Geometry.polygon { polygon: zone }`.

The geometries' parts are objects, so all six are built and read alike:

- a `LineString` and a `MultiPoint` are their `points`;
- a `Polygon` is its `outer` ring and its `holes`;
- a `MultiLineString` is its `lines`;
- a `MultiPolygon` is its `polygons`.

### No text form

A geometry has no single text form, a `Point` included. Neither does a
`BoundingBox`, a `Circle`, an `EastingNorthing` or a `MapTile`.
`text.from` and `text.parse` on one are `type.mismatch`, as on a
`PostalAddress`. Every text form these have (WKT, the `geo:` URI, a
tile's `z/x/y`, a box's four numbers) depends on the order of its
numbers, and an axis order exists only at an edge that names it. WKT is a
format, not a type: only
the two WKT operations ([Interop](20-geospatial.md#interop)) read and write it.

### GeoJSON

A geometry's JSON is GeoJSON (`Json::GeoJson`), read and written by
`jadpo-value`'s `geo` module, the one place where `[longitude, latitude]`
order exists. Each of the five geometries is its RFC 7946 object.
`Geometry` is the object of the geometry it holds, its `type` saying
which; it is never the tagged form other choices are sent in
([Enums](02-values-and-types.md#enums)).

Reading is strict where RFC 7946 says MUST, and refuses what this page
leaves out:

- a latitude or a longitude out of range is refused, as `min` or `max` at
  its place in `coordinates`;
- a position is two or three numbers. More are refused, because RFC 7946
  says not to extend it. A geometry's positions all have an altitude or
  none do (`altitudes_agree`; see
  [Altitudes are all or nothing](20-geospatial.md#altitudes-are-all-or-nothing));
- a line has two points or more, and a ring four or more, closed so that
  its last point is its first. Otherwise it is `min_length` or `format`;
- a multi-geometry has at least one part, though RFC 7946 allows none,
  and a polygon has at least its outer ring;
- the members are `type`, `coordinates` and `bbox`. A `bbox` is checked
  as four or six numbers and otherwise ignored, because a value's box is
  worked out from its points. `crs` and any other foreign member are
  `unknown_field`, as every input refuses a field it doesn't have, and
  WGS84 is the only coordinate system;
- `GeometryCollection` is refused. So are a `Feature` and a
  `FeatureCollection` where a geometry goes. Those two are standard types
  of their own, `Feature<G, P>` and `FeatureCollection<G, P>`, read where
  one is the type
  ([Interop](20-geospatial.md#interop)).

**Ring winding.** A ring wound either way is read, and rewound to the
right-hand rule: the outer ring counterclockwise and each hole clockwise.
RFC 7946 §3.1.6 asks a parser not to refuse a ring wound the other way.
Every ring is written by the rule, including a polygon built in code. The
winding is found exactly, on the plane of longitude and latitude, each
rounded to 9 places. A ring with no area is kept as it is. Whether the
holes sit inside the outer ring and whether rings cross is the rule
`rings_valid`
([Polygons are valid when built](20-geospatial.md#polygons-are-valid-when-built)).

**Numbers on the wire.** RFC 7946 is I-JSON, so a coordinate is read as
the double it is and held as the shortest decimal that is that double; a
whole number is held exactly. It is written as a JSON number with its
places, and an altitude in metres with no trailing zeros. A `Latitude`,
`Longitude` or `Bearing` on its own is a `Decimal`, and is sent as text,
as every `Decimal` is.

### Boxes, circles and other types

- A **`BoundingBox`** is its sides by name, `south`, `west`, `north` and
  `east`, sent as named fields, never as RFC 7946's `bbox` array. Its
  south is at or below its north, its rule `south_not_above_north`. A
  west that is east of its east crosses the antimeridian (RFC 7946 §5.2).
  It is shown from its south-west corner to its north-east one.
- A **`Circle`** is a `centre`, a `Point`, and a `radius`, a `Length`
  never below 0. It is sent as named fields, its centre as GeoJSON, and
  shown `1,000 m around 51.5072° N, 0.1276° W`.
- A **`Crs`**, a coordinate reference system, is `EPSG:` and its code, 1
  to 6 digits with no leading zero. WGS84 is `EPSG:4326`. URNs and
  `OGC:CRS84` aren't read.
- A **`Geohash`** is 1 to 12 characters of its base 32, in lower case
  only.
- An **`H3Cell`** is a cell of Uber's H3 grid, its index (H3's mode 1) as
  15 lower-case hexadecimal digits. Its bits are checked: reserved bits
  clear, a resolution from 0 to 15, one of the 122 base cells, a digit
  from 0 to 6 for each resolution it has and 7 for each it hasn't, and no
  deleted subsequence on a pentagon. An edge's or a vertex's index isn't a
  cell.
- A **`MapTile`** is a tile in OpenStreetMap's XYZ scheme, its row counted
  from the north, not TMS's. Its `zoom` is from 0 to 30, which keeps `x`
  and `y` within a 32-bit integer, as PostGIS's `ST_TileEnvelope` takes
  them. Each of `x` and `y` is below 2 to the power of the zoom, its rule
  `within_zoom`. It is shown as a tile's URL writes it, `10/511/340`.
- An **`EastingNorthing`** is a position on a national grid: an `easting`
  and a `northing`, `Decimal`s held as written, and its `Crs`. It is shown
  with labels, `easting 530,000, northing 180,000 (EPSG:27700)`. It isn't
  converted to WGS84 ([What geospatial leaves out, and what to use instead](20-geospatial.md#what-geospatial-leaves-out-and-what-to-use-instead)).

### Shown, stored and matched

**Shown to people:** a latitude or a longitude in the locale's digits,
with a degree sign and its hemisphere's letter, `N`, `S`, `E` or `W` in
every locale, and no letter for 0. A point is shown as its latitude, then
its longitude, then its altitude. A line or a ring is its points in
brackets, joined by `; `, and a geometry made of others is theirs in
brackets, joined by `, `.

**Stored:** a geometry is one PostGIS `geography` column, checked to its
type (`Storage::Geography`), and a `BoundingBox` and a `Circle` are named
columns ([Storage](20-geospatial.md#storage)). A `Latitude`, `Longitude`
or `Bearing` is one `numeric` column; a `Crs`, `Geohash` or `H3Cell` one
`text` column; and a `MapTile` and an `EastingNorthing` a column per
part, an easting's and a northing's `numeric`.

**Shapes:** `type.standard_exists` matches `Point`'s shape: a latitude and
a longitude by their common names, each built on `Int` or `Decimal`.
`LineString`'s and `Polygon`'s shapes match through a list of a type that
matches `Point`. They are one shape, so the error names both
([Packages use the standard types](#packages-use-the-standard-types)).

**Not personal data:** a position is as often where a depot or a sensor
is as where a person is. To mark a person's position, use the
[`location` category](15-personal-data.md#marking-personal-fields).

[What is built: Geospatial](20-geospatial.md#what-is-built) says which
parts of geospatial work are built.

## AI types
<!-- topic: ai-types -->

The AI types are the values an app passes to and from a model: chat
messages and their roles, vectors and embeddings, and token counts.
[AI](21-ai.md) covers the work you do with them: the `ai` capability and
its operations `ai.generate`, `ai.stream`, `ai.embed` and `ai.embed_all`,
with their failures ([The `ai` capability](21-ai.md#the-ai-capability));
prompts as checked files; vectors stored in pgvector and the
nearest-neighbour query; budgets; personal data sent to a model; and the
fake that tests answer from.

The models themselves come from provider packages
([External services](11-integrations.md)). Jadpo renders the prompt from
a checked file and decodes the answer into a type, as it renders an email
and hands it to a sender. Training a model is left out: a package or an
outside service trains it, and your app calls the result. This section
describes the types themselves.

### Messages

- **`ChatRole`** is a standard enum of a message's four roles, `system`,
  `user`, `assistant` and `tool`. It is written and shown as its variant's
  name in every locale, as every provider's API and a transcript write a
  `role`.
- **`ChatMessage`** is its `role` and its `content`, any text: the shape
  OpenAI's, Anthropic's and Gemini's APIs share,
  `{"role": "user", "content": "..."}`. A message's name, a tool call's
  id, and content parts (images, tool calls and results as blocks) aren't
  part of it, and a member for any of them is refused;
  [AI](21-ai.md#what-ai-leaves-out-and-what-to-use-instead) leaves tool calling and content parts
  out. It is stored as a `text` column each, and shown after its role:
  `user: Which soups are vegan?`.
- **A message is personal data, `health`,** whatever field holds it
  ([Personal data](15-personal-data.md)). What a person writes to a model,
  or is written back, is free text that can hold anything, special
  category details included. So it takes the sensitive category that fits
  such text: it is never cached, and every read is recorded.

### Vectors

An embedding is a list of numbers a model makes from a text, so that
texts with similar meanings have nearby vectors. A `Vector` holds one.

**Its items are single-precision floats,** as pgvector's `vector` column
stores them and as models make embeddings, so storing one loses nothing.
Each item is held as the decimal that is the nearest float to the number
it was built or read from, rounded half to even as IEEE 754 rounds, in
its shortest form.

- Building one from `0.123456789` keeps `0.12345679`, as `scale:` keeps a
  decimal to its places, and a vector never breaks `format`.
- A JSON number is rounded once, from its text as written to the nearest
  float. So a float a client wrote out as a double
  (`-0.006929283495992422`) is that float again (`-0.0069292835`), and a
  vector written and read back is the vector it was.
- A decimal holds 38 places, so an item below `1e-30` is held as 0. That
  is far below a float's precision beside an embedding's other items. An
  item of `1e38` or more is refused, as `max` (or `min` below 0) at its
  place in the array.
- JSON has no NaN or infinity, so neither can be read.

**A vector is a list.** To code it is a `List<Decimal>`, so `list.sum`
and the other list operations take it. It has 1 to 16,000 items, the
most pgvector allows in a `vector` column. It is built from a list,
`Vector([0.5, -0.25])`. It is sent as a JSON array of numbers, never as
decimals in text as other `Decimal`s are, because every provider sends
one that way. It is shown in the locale's digits in brackets, joined by
`; `, as a line's points are: `[0.5; -0.25; 1]`. `==` compares its items
in order. It isn't ordered.

**`dimensions:` fixes a vector type's length:**
`type ArticleVector = Vector { dimensions: 1536 }`, a whole number from 1
to 16,000. It goes only on a type built on `Vector`; elsewhere it is
`type.constraint_base`.

- A vector of another length breaks `dimensions`, a rule named for its
  setting, as `from` and `to` are. It is checked first, so where it
  decides the length it settles `min_length` and `max_length` too.
- It is proved for a list written out with that many items, or a value of
  a type with the same `dimensions:`. JSON, `json.read` and OpenAPI
  (`minItems` and `maxItems`) check it, and the API record writes it.
- A plain `Vector` has any length, so an `Embedding` can hold any model's.
  `min_length` and `max_length` narrow a vector type as they narrow a
  list.
- A type built on a vector type repeats its `dimensions:` or leaves it
  out. One outside its lengths, or a different one, is
  `type.constraint_contradiction`, as a different `scale:` is.

A vector type also holds its items to what its `distance:` and its index
need: not all zeros when compared by cosine, breaking `distance`, and
within ±65,504 when indexed at half precision, breaking `min` and `max`
([Storing a vector](21-ai.md#storing-a-vector) and
[The vector index](21-ai.md#the-vector-index)).

**No text form.** A vector, an embedding, a usage and a message have no
text form: `text.from` and `text.parse` on one are `type.mismatch`, as on
a `PostalAddress`. A vector's JSON array is its one form, and it is also
pgvector's text.

### Embeddings and token counts

- **`Embedding`** is its `model`, a `ModelId`, and its `vector`, a plain
  `Vector`. It is sent as named fields, its model as `ModelId`'s text, and
  shown as its model, then its vector.
- **`TokenCount`** is a whole number from 0 to JavaScript's largest safe
  integer, as `UnitQuantity`'s count is, so a period's totals fit too. It
  is sent as a JSON number, stored in a `bigint`, and shown in the
  locale's digits with no unit, since "1 tokens" needs a plural form CLDR
  doesn't name. It is ordered.
- **`TokenUsage`** is a call's `input`, `output` and `cached` tokens, the
  last `none` where a provider doesn't say. `cached` counts the input
  tokens the provider's cache served, within `input`, as OpenAI and Gemini
  count them, so it is at or below `input`: its rule
  `cached_within_input`. A provider package that counts them apart, such
  as Anthropic's, adds them in.
- A usage's total is `input + output`, worked out where it is needed and
  never kept, so it can't disagree. A `total` member is refused, as any
  unknown field is. A usage is sent as named fields, `cached` as `null`
  when it is `none`, stored as a `bigint` column each, and shown by
  labels, the same in every locale, as projected coordinates are:
  `input 1,200, output 350, cached 800`.

Vectors, embeddings and counts aren't personal data on their own. Mark
an embedding of a person's text with that text's category.

### Stored and matched

**Stored:** a vector is one pgvector `vector(n)` column (`Storage::Vector`),
and an embedding is three columns, its model's two and its vector
([Storing a vector](21-ai.md#storing-a-vector)). A count is one `bigint`
column. A usage and a message are stored as a column per part.

**Shapes:** none of these has a distinctive shape for
`type.standard_exists`. A `{ role, content }` or an `{ input, output }` is
too common to flag.

Here a help article's embedding is three numbers, where a real model's
are 1,536 or more. A list of the wrong length is refused by name:


```jadpo check project=embeddings
type ModelReply = Text {}

type EmbeddingUnreadable = Failure {
    kind: FailureKind.invalid_value
}

/// A help article's embedding, as the support site's model makes it.
type ArticleVector = Vector {
    dimensions: 3
}

/// A help article's embedding from the model's reply: JSON that isn't one
/// at all, or a vector of another length.
function article_vector(model_reply: ModelReply) fails EmbeddingUnreadable -> ArticleVector {
    return attempt json.read(model_reply, as: ArticleVector) {
        malformed: EmbeddingUnreadable
        mismatch: EmbeddingUnreadable
    }
}
```

```jadpo check project=embeddings file=main.test.jadpo
test "an embedding has its model's length" {
    var article_meaning: ArticleVector = attempt article_vector("[0.5,-0.25,1]")
    assert article_meaning == ArticleVector([0.5, -0.25, 1.0])
    assert article_vector("[0.5,-0.25]") fails EmbeddingUnreadable
    assert article_vector("[0.5,-0.25,1e39]") fails EmbeddingUnreadable
}
```


[What is built: AI](21-ai.md#what-is-built) says which parts of AI work
are built.

## Scenario applications
<!-- topic: scenario-applications -->

Nine kinds of application Jadpo is meant for, what each does, and the
operations it uses. Use these tables to find the operation for a job;
each operation is described in its section of this page, or on the page
linked beside it.

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
| A booking ends after it starts | A rule across fields ([Rules across fields](02-values-and-types.md#rules-across-fields)) |
| Calendar invite file | `.ics` package |

**3. Delivery and logistics**

| Does | Needs |
|---|---|
| Driver's next stop and the rest | `list.first`, `list.drop_first` |
| Estimated arrival | `Instant + Duration` ([Arithmetic and comparison](10-configuration-and-time.md#arithmetic-and-comparison)) |
| Is the address inside the zone, distance to depot | [Geospatial](20-geospatial.md) |
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
| Map search in the visible area | [Geospatial](20-geospatial.md) |
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
| Usage per day | `map.get`, `map.with`; monthly totals by an aggregate query ([Counts, sums and groups](06-reads-and-writes.md#counts-sums-and-groups)) |

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
| Hourly charts over months | Aggregate queries ([Counts, sums and groups](06-reads-and-writes.md#counts-sums-and-groups)) |
| Each device signs its messages with its own key | Keys held as data (see [What is built](#what-is-built)) |
| Bit-packed payloads | The device's provider package ([Logic and bits](#logic-and-bits)) |

**8. Internal admin tool**

| Does | Needs |
|---|---|
| Bulk actions on selected records | `Set<Uuid>`, `set.difference` |
| Email shown masked | `text.keep_first`, `text.keep_last` |
| Export of a filtered list | `csv.write` |
| A record's raw JSON for support | `json.write` |
| Counts by status | Aggregate queries ([Counts, sums and groups](06-reads-and-writes.md#counts-sums-and-groups)) |
| Roles | `set.contains` |
| Two-factor codes | TOTP package; passkeys or a provider ([Ways to sign in](08-identity.md#ways-to-sign-in)) |

**9. Support assistant with retrieval (AI)**

| Does | Needs |
|---|---|
| Embed help articles and find the closest | `Vector`, pgvector query ([Embeddings and search](21-ai.md#embeddings-and-search)) |
| Answer as a typed reply with sources | `ai.generate(..., as:)` ([Typed output](21-ai.md#typed-output)) |
| Show the answer as it is written | `ai.stream` ([Streaming](21-ai.md#streaming)) |
| Keep within a monthly budget | AI limits ([Budgets](21-ai.md#budgets)) |
| Strip personal data before sending | `text.replace`; the [personal-data report](15-personal-data.md#the-personal-data-report) |
| Chunk articles into passages | `text.split`, `list.keep_first` |

How much of the standard library each application uses, and how each
need was met, is in the
[design notes](notes/19-standard-library.md#scenario-coverage).

## Language gaps this page depends on
<!-- topic: language-gaps-this-page-depends-on -->

Some operations on this page rely on parts of the language that other
pages describe: `for` over a set, `[]` as an empty map, inline functions
of two values, maths on `Decimal`, secrets used only as a `crypto` key,
`random`, `locale:` on sorting and case, `text.format` and `text.parse`,
aggregate queries, rules across fields, normalised text, and `Money`. The
[design notes](notes/19-standard-library.md#language-gaps-this-page-depends-on)
list each one, the page it lives on and the operations it makes possible.

Some things are left out on purpose. Here is what to use instead:

| Left out | Instead |
|---|---|
| Generic functions in your own code | The standard library stays generic ([Generic code](03-logic.md#generic-code)) |
| `reduce`, `fold` | `for` with `var mut`; `list.sum` |
| Comparators | `sort_by`, `reverse` |
| `items[i]` | `list.at`, which gives `T?` |
| Regular expressions as calls | `pattern:` and construction ([Constraints](02-values-and-types.md#constraints)) |
| Graphemes | Code points |
| Dynamic keys on records | `Map` |
| A dynamic JSON value | `json.read(..., as:)`; `Text` to pass on |
| CSV in memory | `csv.read` through a store |
| MD5, SHA-1 | A package ([External services](11-integrations.md)) |
| Bitwise operations | `Set<Enum>` |
| Fetching exchange rates | A provider package; `money.convert` applies a rate the app holds |
| Gender, sex at birth | Your app's own type, marked sensitive ([Personal data](15-personal-data.md)) |
| CMYK and print colour | A package on the standard `Colour` |
| Training models | A package or an outside service |
| Aliases (`size` for `count`) | The table's name |

Why each is left out is in the
[design notes](notes/19-standard-library.md#left-out-by-design).

Of the parts of the language these operations rely on, only keys held as
data aren't built yet; see [What is built](#what-is-built).

## How the five languages compare
<!-- topic: how-the-five-languages-compare -->

If you know TypeScript, a few of the standard library's choices will
differ from what you expect:

- Text length counts code points, as `max_length` does, not UTF-16 units.
- There are no negative positions. To cut from the end, use
  `keep_last(x, 2)`, not `slice(-2)`.
- Rounding is named every time, with `rounding:`, never a hidden default.
- Exact decimals and `Money`, in minor units with a currency, are built
  in.
- `list.sort_by(items, key)` gives a new list, sorted by a key, never in
  place and never comparing as strings by default.
- Hashing, `hash.sha256`, is an ordinary call, not asynchronous, and
  password hashing is `crypto.hash_password`, Argon2id with no choices.

How TypeScript, Rust, Go, Elixir and Python each handle these jobs, and
what Jadpo took from each, is in the
[design notes](notes/19-standard-library.md#how-the-five-languages-compare).

## What is built
<!-- topic: standard-library-built -->

| Feature | Status |
|---|---|
| Everything on this page not listed below | Built |
| Keys held as data, with encrypted fields: `crypto.verify_totp`, HS256 in `crypto.verify_jwt`, per-device signatures, storing a `TotpSecret` or a `SecretText` | Not built yet: waits on [Personal data](15-personal-data.md), after the first build ([the roadmap](../roadmap.md)) |
| A `type.standard_exists` or `type.standard_clash` swap of a type already stored or on the wire, and `jadpo rename` | Not built yet: a swap waits on [Records and manifest](18-records-and-manifest.md), `jadpo rename` on [Tooling](14-tooling.md) |
| A query comparing or ordering by an amount of money, a one-step update working one out, a backfill of a new required one, and a condition on a JSON-held field | Not built yet: `toolchain.not_built_yet` names each |
| The UK modulus check in `BankAccount` | Not built yet: waits on Vocalink's permission to ship its tables |
| Checking `UnLocode`, `IataAirport`, `IcaoAirport`, `IataAirline` and `Icd10Code` against their lists | Not built yet: waits on licences that allow shipping the lists |
| [Policy](09-policy.md)'s scopes narrowing `OAuthScope` | Not built yet: waits on [Policy](09-policy.md) |
| Legacy encodings in the file readers | Not built yet: waits on an application that needs one |
| `jadpo replay`, and a calendar file's writer | Not built yet: `jadpo replay` waits on [Debugging and review](17-debugging-and-review.md), a calendar file's writer on an application that needs one |
| What [Personal data](15-personal-data.md) does with each standard type's personal data category | Not built yet: waits on [Personal data](15-personal-data.md) |
| The rest of geospatial and AI | Partly built: [What is built: Geospatial](20-geospatial.md#what-is-built) and [What is built: AI](21-ai.md#what-is-built) say which parts |

## Errors

Every error the standard library can produce, with an example that
triggers it and the compiler's real output from `jadpo check`. The
examples are checked as loose files.

- [`type.standard_clash`](../errors/type.standard_clash.md)
- [`type.standard_exists`](../errors/type.standard_exists.md)
- [`semantic.standard_operation_moved`](../errors/semantic.standard_operation_moved.md)
- [`type.argument_negative`](../errors/type.argument_negative.md)
- [`type.argument_empty`](../errors/type.argument_empty.md)
- [`type.money_split`](../errors/type.money_split.md)
- [`type.unit_required`](../errors/type.unit_required.md)
- [`type.unit_unknown`](../errors/type.unit_unknown.md)
- [`syntax.percent_ambiguous`](../errors/syntax.percent_ambiguous.md)
- [`type.temporal_format_part_missing`](../errors/type.temporal_format_part_missing.md)
- [`type.variant_retired`](../errors/type.variant_retired.md)
- [`type.reserved_variant_name`](../errors/type.reserved_variant_name.md)
- [`type.random_context`](../errors/type.random_context.md)
- [`type.password_hash_exposed`](../errors/type.password_hash_exposed.md)
- [`type.secret_exposed`](../errors/type.secret_exposed.md)
