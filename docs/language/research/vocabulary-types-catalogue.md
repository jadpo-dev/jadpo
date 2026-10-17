# Vocabulary types catalogue

The full reference list behind the standard types decided in
[area 19](../19-standard-library.md#shared-vocabulary-types): a brainstorm,
2026-10-17, of every thing two packages might each represent their own way,
1,120 entries in 42 domains, each now with its decision. Area 19 is the
specification; where this page and area 19 disagree, area 19 wins.

The standard library owns the shared vocabulary types (one named-field
shape, one wire shape, one storage shape, one validated construction), all
global in one flat set; a package's own equivalent gets
`type.standard_equivalent` (area 19's question 1 proposes making it a
declinable error, `type.standard_exists`).

- **Status** is as brainstormed: `exists` (built in), `proposed` (on area
  19's first draft) or `candidate`. Shapes are sketches in Jadpo's notation.
- **Gut call** is the brainstorm's tier: core, likely, maybe, niche, silly.
- **Decision** is Richard's (2026-10-17), from area 19's adopted list:
  `standard` (in the list, with its list name where it differs),
  `folded → <Type>` (a unit, role or duplicate of a standard type), or
  `out` with its reason: `format` (an encoding, not a type), `app entity`
  (an application's own record), `niche`, `no standard` (a maybe with no
  published standard), `silly`, `sensitive`, `a call`, `by design` (no
  algorithm choices), `runtime-owned`. Names stay subject to area 19's
  naming question.

## Identity and people

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `PersonName` | A person's name as they give it | `Object { given: Text?, family: Text?, full: Text }` | W3C personal names guidance | Every app splits first/last differently; many cultures don't fit | candidate | core | standard |
| `GivenName` | A first or given name | `Text { min_length: 1, max_length: 200 }` | — | first_name vs forename vs given_name drift | candidate | likely | standard |
| `FamilyName` | A surname or family name | `Text { min_length: 1, max_length: 200 }` | — | last_name vs surname drift | candidate | likely | standard |
| `DisplayName` | The name shown for an account | `Text { min_length: 1, max_length: 100 }` | — | Confused with legal name | candidate | maybe | out: no standard |
| `Nickname` | An informal name | `Text { max_length: 100 }` | — | — | candidate | niche | out: niche |
| `HonorificPrefix` | Title before a name (Dr, Ms) | `Enum` plus free text | — | Free-text titles fail matching | candidate | maybe | out: no standard |
| `HonorificSuffix` | Post-nominals (PhD, OBE) | `Text` | — | — | candidate | niche | out: niche |
| `Pronouns` | A person's stated pronouns | `Object { subject: Text, object: Text, possessive: Text }` | — | Free-text vs enum disagreement | candidate | maybe | out: no standard |
| `Gender` | Self-described gender | `Enum` plus `self_described: Text?` | ISO/IEC 5218 (sex only) | Apps hardcode binaries; different codes | candidate | likely | out: sensitive |
| `SexAtBirth` | Recorded sex for clinical or legal use | `Enum { male, female, indeterminate, not_recorded }` | ISO/IEC 5218 | ISO codes 0/1/2/9 vs M/F | candidate | maybe | out: sensitive |
| `DateOfBirth` | A birth date with optional precision | `Object { date: CalendarDate, precision: DatePrecision }` | — | Year-only births stored as 1 January | candidate | likely | folded → `CalendarDate` |
| `Age` | Age in whole years | `Int { min: 0, max: 150 }` | — | Computed differently across zones | candidate | maybe | out: no standard |
| `AgeRange` | An age band (18–24) | `Range<Int>` | — | Bands overlap between vendors | candidate | maybe | out: no standard |
| `Username` | A login handle | `Text { pattern: "[a-z0-9_.-]{1,64}" }` | — | Case and character rules differ | candidate | likely | standard |
| `Handle` | A social handle including platform | `Object { platform: SocialPlatform, handle: Text }` | — | @ prefix included or not | candidate | maybe | out: no standard |
| `UserId` | An opaque account identifier | `Uuid` | — | Every service defines its own | candidate | likely | out: app entity |
| `AvatarUrl` | A profile image location | `Url` | — | — | candidate | maybe | out: no standard |
| `Signature` | A captured handwritten signature | `Object { image: Bytes, signed_at: Instant }` | eIDAS (advanced) | Proof format varies | candidate | niche | out: niche |
| `MaritalStatus` | Marital or civil status | `Enum` | HL7 v3 MaritalStatus | Different code lists | candidate | niche | out: niche |
| `Nationality` | A person's nationality | `Country` | ISO 3166-1 | Demonym text vs code | candidate | maybe | standard |
| `Ethnicity` | Self-reported ethnic group | `Enum` per census scheme | ONS 2021, US OMB | Schemes differ per country | candidate | niche | out: niche |
| `Religion` | Stated religion | `Text` | — | — | candidate | silly | out: silly |
| `Occupation` | A job or occupation code | `Object { scheme: Text, code: Text }` | ISCO-08, SOC 2020 | Free text never aggregates | candidate | niche | out: niche |
| `Initials` | A person's initials | `Text { max_length: 10 }` | — | — | candidate | silly | out: silly |
| `Birthplace` | Place of birth | `Object { locality: Text?, country: Country }` | — | — | candidate | niche | out: niche |
| `DeathDate` | Date of death with precision | `Object { date: CalendarDate, precision: DatePrecision }` | — | — | candidate | niche | out: niche |
| `Disability` | A declared disability category | `Enum` | Equality Act 2010 categories | — | candidate | niche | out: niche |
| `AccessNeeds` | Access requirements for a person | `Set<AccessNeed>` | — | — | candidate | niche | out: niche |
| `EmergencyContact` | A named contact for emergencies | `Object { name: PersonName, relation: Relationship, phone: PhoneNumber }` | — | Shape reinvented per app | candidate | maybe | out: no standard |
| `Relationship` | Relation between two people | `Enum { parent, child, spouse, partner, sibling, guardian, other }` | HL7 RoleCode | Different code lists | candidate | niche | out: niche |
| `GravatarHash` | An email-derived avatar key | `Text { pattern: "[0-9a-f]{32}" }` | Gravatar | — | candidate | silly | out: silly |
| `ZodiacSign` | Western zodiac sign | `Enum` | — | — | candidate | silly | out: silly |

## Contact and addresses

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Email` | An email address | Validated `Text` | RFC 5321, RFC 5322 | Validation rules differ wildly | exists | core | standard |
| `EmailAddressWithName` | A mailbox with display name | `Object { name: Text?, address: Email }` | RFC 5322 mailbox | "Name <a@b>" parsing bugs | candidate | likely | standard, as `EmailWithName` |
| `PhoneNumber` | A phone number | E.164 `Text`, `+447700900123` | ITU-T E.164 | Leading zero and +44 vs 0044 | proposed | core | standard |
| `PhoneExtension` | An internal extension | `Text { pattern: "[0-9]{1,6}" }` | RFC 3966 ext | Stored inside the number | candidate | maybe | standard |
| `PhoneType` | Mobile, landline, fax | `Enum` | vCard TYPE | — | candidate | maybe | out: app entity |
| `PostalAddress` | A deliverable postal address, any country | `Object { lines: List<Text>, locality: Text?, region: Text?, postal_code: PostalCode?, country: Country }` | UPU S42, ISO 19160 | line1/line2/street disagreement; country formats | candidate | core | standard |
| `PostalCode` | A postal code, rules per country | `Text`, validated against `country` | UPU S42 | Space, case and leading zeros lost | candidate | core | standard |
| `UkPostcode` | A UK postcode | `Text { pattern: "[A-Z]{1,2}[0-9][A-Z0-9]? [0-9][A-Z]{2}" }` | Royal Mail PAF | Missing space, lowercase | candidate | likely | folded → `PostalCode` |
| `UsZipCode` | A US ZIP or ZIP+4 | `Text { pattern: "[0-9]{5}(-[0-9]{4})?" }` | USPS | Leading zeros dropped as Int | candidate | likely | folded → `PostalCode` |
| `Uprn` | Unique Property Reference Number | `Int { min: 1, max: 999999999999 }` | GeoPlace (UK) | Stored as text with leading zeros | candidate | maybe | standard |
| `Udprn` | Royal Mail delivery point | `Int` | Royal Mail PAF | — | candidate | niche | out: niche |
| `AddressLine` | One line of an address | `Text { max_length: 100 }` | UPU | — | candidate | maybe | folded → `PostalAddress` |
| `Region` | A country subdivision | ISO 3166-2 `Text`, `GB-ENG`, `US-CA` | ISO 3166-2 | State names vs codes | candidate | likely | folded → `Subdivision` |
| `Locality` | A town or city name | `Text` | — | — | candidate | maybe | out: no standard |
| `What3Words` | A three-word location | `Text { pattern: "///?[a-z]+\.[a-z]+\.[a-z]+" }` | what3words (proprietary) | — | candidate | niche | out: niche |
| `PlusCode` | An Open Location Code | `Text` | Google Open Location Code | — | candidate | niche | out: niche |
| `Geohash` | A geohash cell | `Text { pattern: "[0-9b-hjkmnp-z]{1,12}" }` | geohash.org | — | candidate | maybe | standard |
| `VCard` | A contact card | Structured object, `.vcf` wire | RFC 6350 | Field mapping diverges | candidate | maybe | out: format |
| `ContactMethod` | One way to reach someone | `Enum { email(Email), phone(PhoneNumber), post(PostalAddress), url(Url) }` | — | — | candidate | maybe | out: no standard |
| `ContactPreference` | Opt-in per channel | `Map<Channel, Bool>` | PECR/GDPR | Consent stored inconsistently | candidate | maybe | out: app entity |
| `FaxNumber` | A fax number | `PhoneNumber` | E.164 | — | candidate | silly | out: silly |
| `PoBox` | A post office box | `Text` | UPU | — | candidate | niche | out: niche |
| `SocialPlatform` | A social network name | `Enum` | — | — | candidate | niche | out: niche |

## Geography and places

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Country` | A country | Enum of ISO 3166-1 alpha-2 | ISO 3166-1 | GB vs UK vs GBR vs 826 | proposed | core | standard |
| `CountryAlpha3` | Three-letter country code | Conversion of `Country` | ISO 3166-1 alpha-3 | Mixed with alpha-2 | candidate | maybe | folded → `Country` |
| `CountryNumeric` | Numeric country code | Conversion of `Country` | ISO 3166-1 numeric | Leading zeros lost | candidate | niche | out: niche |
| `Subdivision` | A first-level subdivision | `Text { pattern: "[A-Z]{2}-[A-Z0-9]{1,3}" }` | ISO 3166-2 | State abbreviations collide | candidate | likely | standard |
| `Continent` | A continent | `Enum` | UN M49 | 7 vs 6 continent lists | candidate | maybe | standard |
| `UnM49Region` | A UN statistical region | `Text { pattern: "[0-9]{3}" }` | UN M49 | — | candidate | niche | out: niche |
| `EuNuts` | A NUTS region | `Text { pattern: "[A-Z]{2}[0-9A-Z]{0,3}" }` | Eurostat NUTS | — | candidate | niche | out: niche |
| `UsState` | A US state | Enum of USPS codes | ANSI/FIPS 5-2 | Names vs codes vs FIPS | candidate | maybe | folded → `Subdivision` |
| `UsFipsCounty` | A US county FIPS code | `Text { pattern: "[0-9]{5}" }` | FIPS 6-4 | Leading zero | candidate | niche | out: niche |
| `UkLocalAuthority` | A UK local authority | `Text` | ONS GSS codes | Names change, codes don't | candidate | niche | out: niche |
| `OnsCode` | A UK statistical geography code | `Text { pattern: "[EWSN][0-9]{8}" }` | ONS GSS | — | candidate | niche | out: niche |
| `GeoNamesId` | A GeoNames place | `Int` | GeoNames | — | candidate | niche | out: niche |
| `WikidataId` | A Wikidata entity | `Text { pattern: "Q[0-9]+" }` | Wikidata | — | candidate | niche | out: niche |
| `UnLocode` | A trade and transport location | `Text { pattern: "[A-Z]{2}[A-Z2-9]{3}" }` | UN/LOCODE | With or without space | candidate | maybe | standard |
| `PlaceName` | A place's name in a language | `Object { name: Text, language: Language }` | — | — | candidate | niche | out: niche |
| `CallingCode` | A country dialling prefix | `Text { pattern: "\+[0-9]{1,3}" }` | ITU-T E.164 | + included or not | candidate | maybe | standard |
| `TopLevelDomain` | A country or generic TLD | `Text` | IANA root zone | uk vs gb | candidate | niche | out: niche |
| `Hemisphere` | North/South, East/West | `Enum` | — | — | candidate | silly | out: silly |

## Geospatial

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Point` | A position on Earth | `Object { latitude: Decimal, longitude: Decimal }` WGS84 | WGS84, RFC 7946 | [lat,lng] vs [lng,lat] | proposed | core | standard |
| `Latitude` | Degrees north | `Decimal { min: -90, max: 90 }` | WGS84 | Swapped with longitude | candidate | likely | standard |
| `Longitude` | Degrees east | `Decimal { min: -180, max: 180 }` | WGS84 | 0–360 vs ±180 | candidate | likely | standard |
| `Altitude` | Height above a datum | `Object { metres: Decimal, datum: VerticalDatum }` | EGM96, WGS84 ellipsoid | Ellipsoid vs sea level | candidate | maybe | out: app entity |
| `Point3D` | Position with elevation | `Object { latitude, longitude, altitude: Altitude }` | RFC 7946 | Third coordinate meaning varies | candidate | maybe | folded → `Point` |
| `LineString` | A path of points | `Object { points: List<Point> }` | RFC 7946 | Order and closure | proposed | core | standard |
| `Polygon` | An area with optional holes | `Object { outer: List<Point>, holes: List<List<Point>> }` | RFC 7946 | Winding order, closure | proposed | core | standard |
| `MultiPolygon` | Several areas | `List<Polygon>` | RFC 7946 | — | proposed | likely | standard |
| `MultiPoint` | Several points | `List<Point>` | RFC 7946 | — | candidate | maybe | standard |
| `MultiLineString` | Several paths | `List<LineString>` | RFC 7946 | — | candidate | maybe | standard |
| `GeometryCollection` | Mixed geometries | `List<Geometry>` | RFC 7946 | — | candidate | niche | out: niche |
| `Geometry` | Any geometry | `Enum` of the above | OGC Simple Features | — | candidate | likely | standard |
| `BoundingBox` | A rectangle on the map | `Object { south, west, north, east: Decimal }` | RFC 7946 bbox | Corner order and antimeridian | candidate | core | standard |
| `Circle` | A centre and a radius | `Object { centre: Point, radius: Length }` | — | Radius units | candidate | likely | standard |
| `Bearing` | A compass direction | `Decimal { min: 0, max: 360 }` degrees true | — | True vs magnetic, radians | candidate | likely | standard |
| `CompassPoint` | N, NE, ENE… | `Enum` of 16 or 32 points | — | — | candidate | maybe | out: no standard |
| `Heading` | Direction of travel | `Object { degrees: Decimal, reference: Enum { true, magnetic } }` | — | — | candidate | maybe | out: no standard |
| `GeoDistance` | A great-circle distance | `Length` | — | Miles vs km vs metres | candidate | likely | folded → `Length` |
| `Crs` | A coordinate reference system | `Text { pattern: "EPSG:[0-9]+" }` | EPSG | Assumed WGS84 | candidate | maybe | standard |
| `OsGridReference` | British National Grid reference | `Text`, `TQ 30080 80530` | Ordnance Survey | Digits precision | candidate | niche | out: niche |
| `EastingNorthing` | Projected coordinates | `Object { easting: Decimal, northing: Decimal, crs: Crs }` | EPSG:27700 and others | x/y order | candidate | maybe | standard |
| `Utm` | A UTM coordinate | `Object { zone: Int, band: Text, easting, northing }` | UTM | — | candidate | niche | out: niche |
| `Mgrs` | A military grid reference | `Text` | NATO MGRS | — | candidate | niche | out: niche |
| `MapTile` | A web map tile address | `Object { zoom: Int, x: Int, y: Int }` | OSM slippy map, TMS | y flipped between TMS and XYZ | candidate | maybe | standard |
| `ZoomLevel` | A web map zoom | `Int { min: 0, max: 24 }` | — | — | candidate | niche | out: niche |
| `H3Cell` | An H3 hexagon index | `Text { pattern: "[0-9a-f]{15}" }` | Uber H3 | Int vs hex text | candidate | maybe | standard |
| `S2Cell` | An S2 cell id | `Int` | Google S2 | Signed vs unsigned | candidate | niche | out: niche |
| `Geofence` | A named area for alerts | `Object { name: Text, area: Polygon }` | — | — | candidate | maybe | out: no standard |
| `Route` | An ordered travel path with legs | `Object { legs: List<RouteLeg> }` | — | — | candidate | maybe | out: no standard |
| `Isochrone` | Area reachable in a time | `Object { origin: Point, duration: Duration, area: Polygon }` | — | — | candidate | niche | out: niche |
| `GpsFix` | A GNSS reading | `Object { point: Point, accuracy: Length, at: Instant, fix: Enum }` | NMEA 0183 | Accuracy units | candidate | maybe | out: app entity |
| `LocationAccuracy` | Horizontal error radius | `Length` | W3C Geolocation | — | candidate | maybe | out: app entity |
| `Elevation` | Ground height | `Length` above mean sea level | — | — | candidate | maybe | out: no standard |
| `MagneticDeclination` | Magnetic variation | `Decimal` degrees, east positive | NOAA WMM | Sign convention | candidate | niche | out: niche |
| `GpxTrack` | A recorded track | Conversion edge from GPX | GPX 1.1 | — | candidate | niche | out: niche |
| `Kml` | Keyhole markup | Conversion edge | OGC KML | — | candidate | niche | out: niche |
| `Wkt` | Well-known text geometry | Conversion edge | OGC WKT | Axis order | candidate | maybe | out: format |
| `Wkb` | Well-known binary geometry | Conversion edge | OGC WKB | — | candidate | niche | out: niche |
| `GeocodeResult` | A place found by search | `Object { point: Point, address: PostalAddress, confidence: Percentage }` | — | Each provider's shape | candidate | maybe | out: no standard |
| `FloorLevel` | A floor in a building | `Object { level: Int, label: Text }` | — | UK ground=0 vs US 1 | candidate | maybe | out: no standard |
| `IndoorLocation` | A position inside a building | `Object { building: Text, floor: FloorLevel, point: Point }` | IMDF | — | candidate | niche | out: niche |

## Money and payments

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Money` | An amount in a currency | `Object { amount: Int minor units, currency: Currency }` | ISO 4217 | 10.00 vs "10.00" vs 1000 | proposed | core | standard |
| `Currency` | A currency | Enum of ISO 4217 codes | ISO 4217 | Symbols are ambiguous ($) | proposed | core | standard |
| `CurrencyNumeric` | Numeric currency code | Conversion of `Currency` | ISO 4217 | — | candidate | niche | out: niche |
| `MinorUnits` | A currency's decimal places | `Int { min: 0, max: 4 }` | ISO 4217 | JPY 0, KWD 3, assumed 2 | candidate | likely | folded → `Money` |
| `ExchangeRate` | Rate between two currencies at a time | `Object { from: Currency, to: Currency, rate: Decimal, at: Instant }` | ISO 4217 | Inverted rates | candidate | likely | standard |
| `MoneyRange` | A price range | `Object { min: Money, max: Money }` | — | Mixed currencies | candidate | maybe | out: no standard |
| `Price` | A unit price with tax basis | `Object { amount: Money, includes_tax: Bool }` | — | Gross vs net confusion | candidate | likely | standard |
| `TaxRate` | A tax rate | `Percentage` | — | 0.2 vs 20 | candidate | likely | standard |
| `CryptoAmount` | An amount in a cryptocurrency | `Object { amount: Int base units, asset: CryptoAsset }` | SLIP-44 | 18 decimals overflow Int | candidate | maybe | out: niche |
| `CryptoAsset` | A crypto asset id | `Text` | SLIP-44, CAIP-19 | — | candidate | niche | out: niche |
| `WalletAddress` | A blockchain address | `Object { chain: Text, address: Text }` | CAIP-10, EIP-55 | Checksum case | candidate | niche | out: niche |
| `PaymentCardNumber` | A primary account number (masked or tokenised) | `Text { pattern: "[0-9]{12,19}" }` with Luhn | ISO/IEC 7812 | PCI scope; spaces | candidate | likely | standard |
| `MaskedPan` | Last four and brand | `Object { brand: CardBrand, last4: Text }` | PCI DSS | Leaking full PAN | candidate | likely | standard, as `MaskedCardNumber` |
| `CardBrand` | Visa, Mastercard… | `Enum` | — | Spelling variants | candidate | likely | standard |
| `CardBin` | Bank identification number | `Text { pattern: "[0-9]{6,8}" }` | ISO/IEC 7812 | 6 vs 8 digit BINs | candidate | maybe | standard |
| `CardExpiry` | Card expiry month and year | `Object { month: Int, year: Int }` | ISO/IEC 7813 | MM/YY vs YYYY, end of month | candidate | likely | standard |
| `Cvv` | Card security code | `Text { pattern: "[0-9]{3,4}" }` | PCI DSS | Must never be stored | candidate | maybe | out: sensitive |
| `PaymentMethodType` | Card, bank transfer, wallet… | `Enum` | — | — | candidate | maybe | out: no standard |
| `PaymentStatus` | Payment lifecycle state | `Enum { pending, authorised, captured, failed, refunded… }` | ISO 20022 status codes | Each provider's states | candidate | maybe | out: app entity |
| `PaymentReference` | A payer-visible reference | `Text { max_length: 18 }` | Bacs, Faster Payments | Length limits per scheme | candidate | maybe | standard |
| `RemittanceInformation` | Structured remittance | `Object { reference: Text }` | ISO 11649 (RF creditor ref) | — | candidate | niche | out: niche |
| `CreditorReference` | RF creditor reference | `Text { pattern: "RF[0-9]{2}[A-Z0-9]{1,21}" }` | ISO 11649 | — | candidate | niche | out: niche |
| `MandateReference` | A direct debit mandate id | `Text` | SEPA, Bacs | — | candidate | niche | out: niche |
| `Tip` | A gratuity | `Money` | — | — | candidate | silly | out: silly |
| `Discount` | A money or percentage discount | `Enum { amount(Money), percentage(Percentage) }` | — | Applied before or after tax | candidate | likely | standard |
| `Allocation` | A money split across parts | `List<Money>` summing to a total | — | Rounding pennies lost | candidate | maybe | out: no standard |
| `InvoiceNumber` | An invoice identifier | `Text` | EN 16931 | — | candidate | maybe | standard |
| `PaymentTerms` | Net 30 and similar | `Object { days: Int, discount: Percentage? }` | UN/CEFACT | — | candidate | maybe | out: app entity |
| `InstalmentPlan` | Pay in parts | `List<Object { due: CalendarDate, amount: Money }>` | — | — | candidate | niche | out: niche |
| `Denomination` | A physical coin or note | `Money` | — | — | candidate | silly | out: silly |

## Banking and finance identifiers

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Iban` | International bank account number | `Text` with mod-97 check | ISO 13616 | Spaces, case, check digits | candidate | core | standard |
| `Bic` | Bank identifier code | `Text { pattern: "[A-Z]{6}[A-Z0-9]{2}([A-Z0-9]{3})?" }` | ISO 9362 | 8 vs 11 characters | candidate | likely | standard |
| `UkSortCode` | UK bank sort code | `Text { pattern: "[0-9]{6}" }` | Pay.UK | Dashes or not | candidate | likely | standard |
| `UkAccountNumber` | UK bank account number | `Text { pattern: "[0-9]{8}" }` | Pay.UK | Leading zeros | candidate | likely | standard |
| `UkBankAccount` | Sort code and account | `Object { sort_code: UkSortCode, account_number: UkAccountNumber }` | Pay.UK | — | candidate | likely | folded → `BankAccount` |
| `AbaRoutingNumber` | US routing number | `Text { pattern: "[0-9]{9}" }` with checksum | ABA | Leading zeros | candidate | maybe | standard |
| `UsBankAccount` | US routing and account | `Object { routing: AbaRoutingNumber, account: Text }` | — | — | candidate | maybe | out: no standard |
| `Bsb` | Australian bank-state-branch | `Text { pattern: "[0-9]{6}" }` | APCA | — | candidate | niche | out: niche |
| `Ifsc` | Indian bank branch code | `Text { pattern: "[A-Z]{4}0[A-Z0-9]{6}" }` | RBI | — | candidate | niche | out: niche |
| `Clabe` | Mexican bank account | `Text { pattern: "[0-9]{18}" }` | Banxico | — | candidate | niche | out: niche |
| `BankAccount` | Any bank account | `Enum { iban(Iban), uk(UkBankAccount), us(UsBankAccount), … }` | — | Shape per country | candidate | likely | standard |
| `Lei` | Legal entity identifier | `Text { pattern: "[A-Z0-9]{18}[0-9]{2}" }` | ISO 17442 | — | candidate | maybe | standard |
| `Isin` | Security identifier | `Text { pattern: "[A-Z]{2}[A-Z0-9]{9}[0-9]" }` | ISO 6166 | Check digit | candidate | maybe | standard |
| `Cusip` | US security identifier | `Text { pattern: "[A-Z0-9]{9}" }` | CUSIP | — | candidate | niche | out: niche |
| `Sedol` | UK security identifier | `Text { pattern: "[A-Z0-9]{7}" }` | LSE | — | candidate | niche | out: niche |
| `Figi` | Financial instrument global id | `Text { pattern: "BBG[A-Z0-9]{9}" }` | OMG FIGI | — | candidate | niche | out: niche |
| `TickerSymbol` | An exchange ticker | `Object { exchange: Mic, symbol: Text }` | — | Same symbol on different exchanges | candidate | maybe | out: no standard |
| `Mic` | Market identifier code | `Text { pattern: "[A-Z0-9]{4}" }` | ISO 10383 | — | candidate | niche | out: niche |
| `Cfi` | Instrument classification | `Text { pattern: "[A-Z]{6}" }` | ISO 10962 | — | candidate | niche | out: niche |
| `UkCompanyNumber` | Companies House number | `Text { pattern: "([0-9]{2}\|[A-Z]{2})[0-9]{6}" }` | Companies House | Leading zeros lost | candidate | likely | standard |
| `Duns` | D-U-N-S number | `Text { pattern: "[0-9]{9}" }` | Dun & Bradstreet | — | candidate | niche | out: niche |
| `SwiftMessageType` | MT message type | `Text { pattern: "MT[0-9]{3}" }` | SWIFT | — | candidate | silly | out: silly |
| `Iso20022MessageId` | ISO 20022 message id | `Text` | ISO 20022 | — | candidate | niche | out: niche |
| `TransactionId` | A bank transaction reference | `Text` | — | — | candidate | maybe | out: no standard |
| `Uetr` | Unique end-to-end transaction reference | `Uuid` | SWIFT gpi | — | candidate | niche | out: niche |
| `CreditScore` | A bureau credit score | `Object { bureau: Text, score: Int, scale: Range<Int> }` | — | Scales differ per bureau | candidate | niche | out: niche |
| `InterestRate` | An annual rate | `Object { rate: Percentage, basis: Enum { apr, aer, flat } }` | — | APR vs AER confusion | candidate | likely | standard |
| `DayCountConvention` | Interest day count | `Enum { act_360, act_365, thirty_360… }` | ISDA | Silent wrong interest | candidate | niche | out: niche |

## Accounting and tax

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `VatNumber` | A VAT registration number | `Object { country: Country, number: Text }` | EU VIES, HMRC | Prefix included or not | candidate | likely | standard |
| `UkVatNumber` | A UK VAT number | `Text { pattern: "GB([0-9]{9}\|[0-9]{12})" }` | HMRC | — | candidate | maybe | folded → `VatNumber` |
| `EuVatNumber` | An EU VAT id | `Text` per member state | EU VIES | — | candidate | maybe | folded → `VatNumber` |
| `TaxId` | Any tax identifier | `Object { country: Country, scheme: TaxIdScheme, value: Text }` | OECD TIN | Scheme ambiguity | candidate | likely | standard |
| `UkUtr` | UK unique taxpayer reference | `Text { pattern: "[0-9]{10}" }` | HMRC | — | candidate | maybe | standard |
| `UkNationalInsurance` | National Insurance number | `Text { pattern: "[A-CEGHJ-PR-TW-Z]{2}[0-9]{6}[A-D]" }` | HMRC/DWP | Spaces; invalid prefixes | candidate | likely | standard, as `UkNationalInsuranceNumber` |
| `UsSsn` | US social security number | `Text { pattern: "[0-9]{3}-[0-9]{2}-[0-9]{4}" }` | SSA | Dashes; personal data | candidate | maybe | standard |
| `UsEin` | US employer identification number | `Text { pattern: "[0-9]{2}-[0-9]{7}" }` | IRS | — | candidate | maybe | standard |
| `AbnNumber` | Australian business number | `Text { pattern: "[0-9]{11}" }` | ABR | — | candidate | niche | out: niche |
| `GstNumber` | GST registration (IN, CA, NZ…) | `Object { country: Country, number: Text }` | — | — | candidate | niche | out: niche |
| `TaxCode` | UK PAYE tax code | `Text { pattern: "[SC]?[0-9]*[LMNTKBR]?[A-Z0-9]*" }` | HMRC | — | candidate | niche | out: niche |
| `TaxYear` | A tax year | `Object { country: Country, start: CalendarDate }` | — | UK 6 April vs calendar | candidate | maybe | out: no standard |
| `FiscalYear` | A company financial year | `Object { start: CalendarDate, end: CalendarDate }` | — | Labelled by start or end year | candidate | maybe | out: no standard |
| `FiscalPeriod` | A reporting period | `Object { year: FiscalYear, period: Int }` | — | 4-4-5 calendars | candidate | maybe | out: no standard |
| `Quarter` | A calendar or fiscal quarter | `Object { year: Int, quarter: Int { min: 1, max: 4 } }` | ISO 8601 (none) | Fiscal vs calendar | candidate | likely | standard |
| `LedgerAccountCode` | A chart of accounts code | `Text` | — | — | candidate | maybe | out: no standard |
| `JournalEntry` | Balanced debits and credits | `Object { lines: List<JournalLine> }` summing to zero | Double entry | Sign convention | candidate | maybe | out: app entity |
| `DebitCredit` | Side of an entry | `Enum { debit, credit }` | — | Negative vs side | candidate | maybe | out: no standard |
| `CostCentre` | A cost centre code | `Text` | — | — | candidate | niche | out: niche |
| `TaxCategory` | VAT category (standard, zero, exempt) | `Enum` | UNCL5305 | — | candidate | maybe | out: app entity |
| `HsCode` | Harmonized commodity code | `Text { pattern: "[0-9]{6,10}" }` | WCO HS | 6 vs 8 vs 10 digits | candidate | maybe | standard |
| `CommodityCode` | UK/EU tariff code | `Text { pattern: "[0-9]{10}" }` | UK Global Tariff, EU TARIC | — | candidate | niche | out: niche |
| `Incoterm` | Trade delivery term | `Enum { exw, fca, fob, cif, ddp… }` with version | ICC Incoterms 2020 | Year version omitted | candidate | maybe | standard |
| `Eori` | Economic operator registration | `Text { pattern: "[A-Z]{2}[A-Z0-9]{1,15}" }` | EU, HMRC | — | candidate | niche | out: niche |
| `SicCode` | Industry classification | `Text { pattern: "[0-9]{4,5}" }` | UK SIC 2007 | US vs UK SIC | candidate | niche | out: niche |
| `Naics` | North American industry code | `Text { pattern: "[0-9]{2,6}" }` | NAICS | — | candidate | niche | out: niche |
| `Nace` | EU industry code | `Text` | NACE Rev. 2 | — | candidate | niche | out: niche |
| `ReportingFramework` | Reporting framework | `Enum` | IFRS, US GAAP | — | candidate | silly | out: silly |

## Markets and investment

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `ShareQuantity` | A holding quantity, fractional allowed | `Decimal { scale: 8 }` | — | Fractional shares break Int | candidate | maybe | out: no standard |
| `OrderSide` | Buy or sell | `Enum { buy, sell }` | FIX 54 | — | candidate | maybe | out: app entity |
| `OrderType` | Market, limit, stop | `Enum` | FIX 40 | — | candidate | niche | out: niche |
| `TimeInForce` | Order validity | `Enum { day, gtc, ioc, fok }` | FIX 59 | — | candidate | niche | out: niche |
| `Ohlc` | Open, high, low, close bar | `Object { open, high, low, close: Money, volume: Decimal, period: Interval }` | — | Field order in arrays | candidate | maybe | out: no standard |
| `BasisPoints` | Hundredths of a percent | `Int` | — | bps vs percent | candidate | likely | standard |
| `Yield` | A rate of return | `Percentage` | — | — | candidate | niche | out: niche |
| `CandleInterval` | Bar duration | `Duration` | — | 1m minute vs month | candidate | niche | out: niche |
| `RiskRating` | Investment risk level | `Int { min: 1, max: 7 }` | EU PRIIPs SRI | — | candidate | niche | out: niche |

## Commerce and retail

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Sku` | A seller's stock-keeping unit | `Text { max_length: 64 }` | — | Case and whitespace mismatch | candidate | likely | standard |
| `Gtin` | Global trade item number (8, 12, 13 or 14 digits) | `Text { pattern: "[0-9]{8}\|[0-9]{12,14}" }` with check digit | GS1 General Specifications | UPC vs EAN padding; leading zeros | candidate | core | folded → `Ean13` |
| `Ean13` | European article number | `Gtin` of 13 digits | GS1 | — | candidate | maybe | standard |
| `UpcA` | Universal product code | `Gtin` of 12 digits | GS1 | Leading zero lost | candidate | maybe | standard |
| `Isbn` | International standard book number | `Text`, ISBN-13 normalised | ISO 2108 | ISBN-10 vs 13, hyphens | candidate | likely | standard |
| `Issn` | Serial publication number | `Text { pattern: "[0-9]{4}-[0-9]{3}[0-9X]" }` | ISO 3297 | — | candidate | niche | out: niche |
| `Ismn` | Printed music number | `Text` | ISO 10957 | — | candidate | silly | out: silly |
| `Asin` | Amazon standard identification number | `Text { pattern: "[A-Z0-9]{10}" }` | Amazon (proprietary) | — | candidate | niche | out: niche |
| `Mpn` | Manufacturer part number | `Object { manufacturer: Text, part: Text }` | — | — | candidate | maybe | out: no standard |
| `Brand` | A product brand | `Text` | — | — | candidate | niche | out: niche |
| `Gpc` | GS1 product category | `Int` | GS1 GPC | — | candidate | niche | out: niche |
| `GoogleProductCategory` | Google taxonomy category | `Int` | Google Merchant | — | candidate | niche | out: niche |
| `Sscc` | Serial shipping container code | `Text { pattern: "[0-9]{18}" }` | GS1 | — | candidate | maybe | standard |
| `Gln` | Global location number | `Text { pattern: "[0-9]{13}" }` | GS1 | — | candidate | niche | out: niche |
| `Gs1DigitalLink` | A GS1 Digital Link URI | `Url` | GS1 Digital Link | — | candidate | niche | out: niche |
| `Quantity` | A count of something with a unit | `Object { value: Decimal, unit: UnitOfMeasure }` | UN/ECE Rec 20 | Unit implied | candidate | core | standard |
| `UnitOfMeasure` | A trade unit code | Enum of UN/ECE Rec 20 codes (`KGM`, `EA`) | UN/ECE Rec 20 | "each" vs "pcs" vs "ea" | candidate | likely | folded → `Quantity` |
| `StockLevel` | Units in stock | `Int { min: 0 }` | — | — | candidate | maybe | out: no standard |
| `Availability` | In stock, preorder, discontinued | `Enum` | schema.org ItemAvailability | — | candidate | maybe | out: app entity |
| `ProductCondition` | New, used, refurbished | `Enum` | schema.org OfferItemCondition | — | candidate | maybe | out: app entity |
| `Rating` | A star or score rating | `Object { value: Decimal, best: Decimal, worst: Decimal }` | schema.org Rating | Out of 5 vs 10 vs 100 | candidate | likely | standard |
| `ReviewCount` | Number of reviews | `Int { min: 0 }` | — | — | candidate | silly | out: silly |
| `CouponCode` | A promotion code | `Text { pattern: "[A-Z0-9-]{3,32}" }` | — | Case sensitivity | candidate | maybe | out: no standard |
| `GiftCardCode` | A gift card code | `Text` | — | — | candidate | niche | out: niche |
| `OrderNumber` | A customer-facing order reference | `Text` | — | — | candidate | maybe | out: no standard |
| `OrderStatus` | Order lifecycle state | `Enum` | schema.org OrderStatus | — | candidate | maybe | out: app entity |
| `CartLine` | A line in a basket | `Object { sku: Sku, quantity: Int, unit_price: Money }` | — | — | candidate | maybe | out: no standard |
| `ClothingSize` | A garment size with its system | `Object { system: SizeSystem, value: Text }` | ISO 8559, EN 13402 | UK 10 vs US 6 vs EU 38 | candidate | likely | standard |
| `SizeSystem` | A size convention | `Enum { uk, us, eu, jp, intl… }` | — | — | candidate | maybe | out: no standard |
| `ShoeSize` | A shoe size with system | `Object { system: ShoeSizeSystem, value: Decimal }` | ISO 9407 (Mondopoint) | Men's vs women's US | candidate | maybe | standard |
| `RingSize` | A ring size with system | `Object { system: Enum, value: Text }` | ISO 8653 | — | candidate | niche | out: niche |
| `BraSize` | Band and cup with system | `Object { band: Int, cup: Text, system: SizeSystem }` | EN 13402 | — | candidate | niche | out: niche |
| `HatSize` | Head circumference | `Length` | — | — | candidate | silly | out: silly |
| `Dimensions` | Length, width, height of a thing | `Object { length: Length, width: Length, height: Length }` | — | LxWxH order varies | candidate | likely | standard |
| `PackageDimensions` | Parcel size and weight | `Object { dimensions: Dimensions, weight: Mass }` | — | — | candidate | likely | folded → `Dimensions` |
| `Barcode` | A machine-readable code with its symbology | `Object { symbology: BarcodeSymbology, data: Text }` | ISO/IEC 15417 and others | — | candidate | maybe | standard |
| `BarcodeSymbology` | Code 128, QR, EAN-13… | `Enum` | ISO/IEC | — | candidate | maybe | folded → `Barcode` |
| `QrCodeData` | Payload of a QR code | `Text` | ISO/IEC 18004 | — | candidate | niche | out: niche |
| `Warranty` | A warranty term | `Object { duration: Duration, scope: Text }` | — | — | candidate | niche | out: niche |
| `ReturnPolicy` | Returns window and conditions | `Object { days: Int, free: Bool }` | schema.org MerchantReturnPolicy | — | candidate | niche | out: niche |
| `LoyaltyPoints` | Points balance | `Int` | — | — | candidate | niche | out: niche |
| `EnergyLabel` | Appliance energy class | `Enum { a, b, c, d, e, f, g }` | EU 2017/1369 | A+++ old scale vs new | candidate | niche | out: niche |
| `AgeRestriction` | Minimum purchase age | `Int { min: 0 }` | — | — | candidate | niche | out: niche |
| `PriceBreak` | Tiered unit pricing | `List<Object { min_quantity: Int, price: Money }>` | — | — | candidate | niche | out: niche |
| `UnitPrice` | Price per standard unit | `Object { price: Money, per: Quantity }` | UK Price Marking Order | — | candidate | maybe | standard |

## Time and calendar

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Instant` | A moment in time | As area 10 | RFC 3339 | Seconds vs millis epoch | exists | core | standard |
| `CalendarDate` | A date without time | As area 10 | ISO 8601 | Shifted by zone conversion | exists | core | standard |
| `ZonedTime` | A local time in a zone | As area 10 | RFC 9557 | — | exists | core | standard |
| `Duration` | An exact length of time | As area 10 | ISO 8601 durations | ms vs s | exists | core | standard |
| `Zone` | An IANA time zone | As area 10 | IANA tz | — | exists | core | standard |
| `Weekday` | A day of the week | As area 10 | ISO 8601 | Sunday=0 vs Monday=1 | exists | core | standard |
| `LocalTime` | A wall-clock time without date | `Object { hour, minute, second }` | ISO 8601 | 24:00 and leap seconds | candidate | core | standard |
| `LocalDateTime` | A date and time with no zone | `Object { date: CalendarDate, time: LocalTime }` | ISO 8601 | Mistaken for UTC | candidate | likely | standard |
| `Month` | A month of the year | `Enum` January to December | ISO 8601 | 0- vs 1-based | candidate | likely | standard |
| `YearMonth` | A month in a year | `Text { pattern: "[0-9]{4}-[0-9]{2}" }` | ISO 8601 | Stored as first of month | candidate | likely | standard |
| `Year` | A calendar year | `Int` | ISO 8601 | — | candidate | maybe | standard |
| `IsoWeek` | An ISO week | `Object { year: Int, week: Int { min: 1, max: 53 } }` | ISO 8601 | Week-year vs calendar year | candidate | likely | standard |
| `DayOfYear` | Ordinal day | `Int { min: 1, max: 366 }` | ISO 8601 | — | candidate | niche | out: niche |
| `DateRange` | A span of dates | `Object { start: CalendarDate, end: CalendarDate }`, end inclusive | — | Inclusive vs exclusive end | candidate | core | standard |
| `Interval` | A span between two instants | `Object { start: Instant, end: Instant }`, end exclusive | ISO 8601 intervals | Open-ended and inclusive bugs | candidate | core | standard, as `TimeInterval` |
| `TimeRange` | A span of local times | `Object { start: LocalTime, end: LocalTime }` | — | Crossing midnight | candidate | likely | folded → `TimeInterval` |
| `Period` | A calendar amount (1 month, 2 years) | `Object { years, months, days: Int }` | ISO 8601 P1M | Month length vs Duration | candidate | likely | standard, as `CalendarPeriod` |
| `RecurrenceRule` | A repeating schedule | `Object` matching RRULE fields | RFC 5545 RRULE | Every library's own repeat model | candidate | core | standard |
| `CronExpression` | A cron schedule | `Text` validated | POSIX cron, Quartz | 5 vs 6 vs 7 fields | candidate | likely | out: app entity |
| `OpeningHours` | Weekly opening times with exceptions | `Object { weekly: Map<Weekday, List<TimeRange>>, exceptions: List<…> }` | OSM opening_hours, schema.org | Midnight crossing; holidays | candidate | likely | standard |
| `BusinessDays` | Working days per calendar | `Set<Weekday>` | — | Fri-Sat weekends | candidate | maybe | out: no standard |
| `HolidayCalendar` | A named set of public holidays | `Object { region: Subdivision, dates: Set<CalendarDate> }` | — | — | candidate | maybe | out: no standard |
| `FreeBusy` | Free/busy time | `List<Interval>` | RFC 5545 VFREEBUSY | — | candidate | maybe | out: app entity |
| `CalendarEvent` | An event with time, place, attendees | `Object` matching VEVENT | RFC 5545, JSCalendar (RFC 8984) | All-day events off by one | candidate | likely | standard |
| `AllDayFlag` | An event without times | Part of `CalendarEvent` | RFC 5545 VALUE=DATE | — | candidate | niche | out: niche |
| `Timestamp` | Unix epoch seconds | Conversion edge for `Instant` | POSIX | s vs ms vs ns | candidate | likely | folded → `Instant` |
| `UnixMillis` | Epoch milliseconds | Conversion edge for `Instant` | — | — | candidate | maybe | out: no standard |
| `UtcOffset` | Offset from UTC | `Object { minutes: Int { min: -1080, max: 1080 } }` | ISO 8601 | Sign inverted (POSIX TZ) | candidate | likely | standard |
| `TimeOfDayPart` | Morning, afternoon, evening | `Enum` | — | — | candidate | silly | out: silly |
| `Season` | Spring, summer… | `Enum` with hemisphere | — | Hemispheres | candidate | silly | out: silly |
| `Era` | AD/BC, Japanese eras | `Enum` | CLDR | — | candidate | niche | out: niche |
| `NonGregorianDate` | Hijri, Hebrew, Chinese dates | `Object { calendar: CalendarSystem, year, month, day }` | CLDR calendars | — | candidate | niche | out: niche |
| `CalendarSystem` | Gregorian, Islamic, Hebrew… | `Enum` | BCP 47 -u-ca | — | candidate | niche | out: niche |
| `FuzzyDate` | An approximate date (circa 1850) | `Object { date: CalendarDate, precision: DatePrecision, approximate: Bool }` | EDTF (ISO 8601-2) | Precision lost | candidate | maybe | out: app entity |
| `DatePrecision` | Year, month, day | `Enum` | EDTF | — | candidate | maybe | out: app entity |
| `Deadline` | A due moment with zone | `Instant` | — | — | candidate | silly | out: silly |
| `Countdown` | Remaining time | `Duration` | — | — | candidate | silly | out: silly |
| `Sla` | A service level target | `Object { response: Duration, resolution: Duration, hours: OpeningHours }` | — | — | candidate | niche | out: niche |
| `Timecode` | Video frame time | `Object { hours, minutes, seconds, frames: Int, rate: FrameRate }` | SMPTE ST 12 | Drop-frame | candidate | maybe | standard |
| `LeapSecond` | A leap-second instant | — | IERS | Rare overflow bugs | candidate | silly | out: silly |
| `MoonPhase` | Phase of the moon | `Enum` | — | — | candidate | silly | out: silly |

## Length, area and volume

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Length` | A distance | `Object { metres: Decimal }` with unit at the edges | SI | Unit confusion (Mars Climate Orbiter) | candidate | core | standard |
| `LengthUnit` | m, km, mi, ft, in, nmi… | `Enum` | SI, UCUM | "m" metre vs mile | candidate | likely | folded → `Length` |
| `Metre` | SI base length | `Length` constructor | SI | — | candidate | likely | folded → `Length` |
| `Kilometre` | 1000 metres | `Length` constructor | SI | — | candidate | likely | folded → `Length` |
| `Centimetre` | 0.01 metre | `Length` constructor | SI | — | candidate | maybe | folded → `Length` |
| `Millimetre` | 0.001 metre | `Length` constructor | SI | — | candidate | maybe | folded → `Length` |
| `Micrometre` | 10⁻⁶ metre | `Length` constructor | SI | — | candidate | niche | out: niche |
| `Nanometre` | 10⁻⁹ metre | `Length` constructor | SI | — | candidate | niche | out: niche |
| `Mile` | International mile, 1609.344 m | `Length` constructor | International yard 1959 | Statute vs nautical vs survey | candidate | likely | folded → `Length` |
| `NauticalMile` | 1852 metres | `Length` constructor | SI accepted | — | candidate | maybe | folded → `Length` |
| `Yard` | 0.9144 m | `Length` constructor | International yard 1959 | — | candidate | maybe | folded → `Length` |
| `Foot` | 0.3048 m | `Length` constructor | International yard 1959 | US survey foot | candidate | maybe | folded → `Length` |
| `Inch` | 0.0254 m | `Length` constructor | International yard 1959 | — | candidate | maybe | folded → `Length` |
| `FeetAndInches` | Height as 5′10″ | `Object { feet: Int, inches: Decimal }` conversion edge | — | Stored as 5.10 | candidate | maybe | out: no standard |
| `Height` | A person's height | `Length` | — | cm vs ft-in | candidate | likely | folded → `Length` |
| `Fathom` | 6 feet | `Length` constructor | — | — | candidate | silly | out: silly |
| `Furlong` | 220 yards | `Length` constructor | — | — | candidate | silly | out: silly |
| `LightYear` | Astronomical distance | `Length` constructor | IAU | — | candidate | silly | out: silly |
| `AstronomicalUnit` | 149 597 870 700 m | `Length` constructor | IAU 2012 | — | candidate | niche | out: niche |
| `Parsec` | Astronomical distance | `Length` constructor | IAU | — | candidate | silly | out: silly |
| `TypographicPoint` | 1/72 inch | `TypographicLength` | PostScript | Pica point vs DTP | candidate | niche | out: niche |
| `Pixel` | A CSS or device pixel | `Object { px: Decimal }` | CSS Values | CSS px vs device px | candidate | maybe | out: niche |
| `CssLength` | A CSS length with unit | `Object { value: Decimal, unit: CssUnit }` | CSS Values 4 | rem vs px | candidate | maybe | out: format |
| `Area` | A surface area | `Object { square_metres: Decimal }` | SI | sq ft vs m² | candidate | likely | standard |
| `AreaUnit` | m², ha, acre, ft²… | `Enum` | SI, UCUM | — | candidate | maybe | folded → `Area` |
| `Hectare` | 10 000 m² | `Area` constructor | SI accepted | — | candidate | maybe | folded → `Area` |
| `Acre` | 4046.856 m² | `Area` constructor | — | — | candidate | maybe | out: no standard |
| `SquareFoot` | 0.092903 m² | `Area` constructor | — | Property listings mix | candidate | maybe | out: no standard |
| `Volume` | A volume | `Object { cubic_metres: Decimal }` | SI | Litre vs gallon | candidate | likely | standard |
| `VolumeUnit` | l, ml, m³, gal, fl oz, cup… | `Enum` | SI, UCUM | US vs imperial gallon | candidate | likely | folded → `Volume` |
| `Litre` | 0.001 m³ | `Volume` constructor | SI accepted | — | candidate | likely | folded → `Volume` |
| `Millilitre` | 10⁻⁶ m³ | `Volume` constructor | SI accepted | — | candidate | maybe | folded → `Volume` |
| `ImperialGallon` | 4.54609 l | `Volume` constructor | Weights and Measures Act 1985 | Confused with US gallon | candidate | maybe | folded → `Volume` |
| `UsGallon` | 3.785411784 l | `Volume` constructor | NIST HB 44 | — | candidate | maybe | folded → `Volume` |
| `ImperialPint` | 568.26 ml | `Volume` constructor | — | US pint 473 ml | candidate | maybe | out: no standard |
| `FluidOunce` | Imperial or US fluid ounce | `Volume` constructor with system | — | 28.4 vs 29.6 ml | candidate | maybe | out: no standard |
| `Cup` | Cooking cup with system | `Volume` constructor | — | US 236.6 vs metric 250 ml | candidate | niche | out: niche |
| `Tablespoon` | Cooking tablespoon | `Volume` constructor | — | 15 vs 20 ml (AU) | candidate | niche | out: niche |
| `Teaspoon` | Cooking teaspoon | `Volume` constructor | — | — | candidate | niche | out: niche |
| `Barrel` | Oil barrel, 158.987 l | `Volume` constructor | API | — | candidate | niche | out: niche |
| `CubicFoot` | 0.0283 m³ | `Volume` constructor | — | — | candidate | niche | out: niche |
| `Angle` | An angle | `Object { radians: Decimal }` | SI derived | Degrees vs radians | candidate | likely | standard |
| `AngleUnit` | rad, deg, grad, turn, arcmin, arcsec | `Enum` | SI | — | candidate | maybe | folded → `Angle` |
| `Dms` | Degrees, minutes, seconds | `Object { degrees: Int, minutes: Int, seconds: Decimal }` conversion edge | — | Sign on minutes | candidate | maybe | out: no standard |
| `SolidAngle` | Steradians | `Decimal` | SI | — | candidate | silly | out: silly |

## Mass, force, pressure and energy

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Mass` | A mass | `Object { kilograms: Decimal }` | SI | kg vs lb | candidate | core | standard |
| `MassUnit` | kg, g, mg, t, lb, oz, st… | `Enum` | SI, UCUM | Ton vs tonne | candidate | likely | folded → `Mass` |
| `Kilogram` | SI base mass | `Mass` constructor | SI | — | candidate | likely | folded → `Mass` |
| `Gram` | 0.001 kg | `Mass` constructor | SI | — | candidate | likely | folded → `Mass` |
| `Milligram` | 10⁻⁶ kg | `Mass` constructor | SI | mg vs mcg dosing errors | candidate | likely | folded → `Mass` |
| `Microgram` | 10⁻⁹ kg | `Mass` constructor | SI | "µg" vs "mcg" | candidate | maybe | folded → `Mass` |
| `Tonne` | 1000 kg | `Mass` constructor | SI accepted | vs short and long ton | candidate | maybe | folded → `Mass` |
| `Pound` | 0.45359237 kg | `Mass` constructor | International pound 1959 | — | candidate | likely | folded → `Mass` |
| `Ounce` | 28.349523125 g | `Mass` constructor | — | Troy ounce | candidate | maybe | out: no standard |
| `Stone` | 14 lb | `Mass` constructor | — | UK body weight | candidate | maybe | out: no standard |
| `StonesAndPounds` | Body weight as 11 st 4 lb | Conversion edge | — | — | candidate | niche | out: niche |
| `TroyOunce` | 31.1034768 g | `Mass` constructor | — | Precious metals | candidate | niche | out: niche |
| `Carat` | 200 mg (gems) | `Mass` constructor | — | vs karat purity | candidate | niche | out: niche |
| `Karat` | Gold purity in 24ths | `Int { min: 1, max: 24 }` | — | — | candidate | niche | out: niche |
| `ShortTon` | 2000 lb | `Mass` constructor | — | — | candidate | niche | out: niche |
| `LongTon` | 2240 lb | `Mass` constructor | — | — | candidate | silly | out: silly |
| `Dalton` | Atomic mass unit | `Mass` constructor | SI accepted | — | candidate | niche | out: niche |
| `BodyWeight` | A person's weight | `Mass` | — | kg vs lb vs st | candidate | maybe | out: no standard |
| `Force` | A force | `Object { newtons: Decimal }` | SI | — | candidate | niche | out: niche |
| `Pressure` | A pressure | `Object { pascals: Decimal }` | SI | bar vs psi vs hPa | candidate | likely | standard |
| `PressureUnit` | Pa, kPa, hPa, bar, psi, mmHg, inHg, atm | `Enum` | SI, UCUM | — | candidate | maybe | folded → `Pressure` |
| `TyrePressure` | Tyre inflation | `Pressure` | — | psi vs bar | candidate | niche | out: niche |
| `BloodPressure` | Systolic over diastolic | `Object { systolic: Pressure, diastolic: Pressure }` | LOINC 85354-9 | Order of the pair | candidate | likely | standard |
| `BarometricPressure` | Atmospheric pressure | `Pressure` | WMO | hPa vs inHg | candidate | maybe | folded → `Pressure` |
| `Energy` | An energy | `Object { joules: Decimal }` | SI | kWh vs J vs kcal | candidate | likely | standard |
| `EnergyUnit` | J, kJ, kWh, cal, kcal, BTU, eV, therm | `Enum` | SI, UCUM | Cal vs kcal | candidate | maybe | folded → `Energy` |
| `KilowattHour` | 3.6 MJ | `Energy` constructor | — | kW vs kWh confusion | candidate | likely | folded → `Energy` |
| `Kilocalorie` | Food energy, 4184 J | `Energy` constructor | — | "Calories" ambiguity | candidate | likely | folded → `Energy` |
| `Btu` | British thermal unit | `Energy` constructor | ISO 31-4 | — | candidate | niche | out: niche |
| `Therm` | Gas energy, 100 000 BTU | `Energy` constructor | — | UK gas bills | candidate | niche | out: niche |
| `Electronvolt` | Particle energy | `Energy` constructor | SI accepted | — | candidate | niche | out: niche |
| `Power` | A rate of energy | `Object { watts: Decimal }` | SI | kW vs hp | candidate | likely | standard |
| `PowerUnit` | W, kW, MW, hp, PS, BTU/h | `Enum` | SI | Metric vs imperial horsepower | candidate | maybe | folded → `Power` |
| `Horsepower` | Mechanical or metric hp | `Power` constructor with system | — | hp vs PS vs bhp | candidate | niche | out: niche |
| `Torque` | Rotational force | `Object { newton_metres: Decimal }` | SI | lb-ft vs Nm | candidate | niche | out: niche |
| `Work` | Force times distance | `Energy` | SI | — | candidate | silly | out: silly |
| `Momentum` | Mass times velocity | `Decimal` | SI | — | candidate | silly | out: silly |
| `Density` | Mass per volume | `Object { kg_per_m3: Decimal }` | SI | — | candidate | niche | out: niche |

## Temperature, speed and other physical quantities

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Temperature` | A temperature | `Object { kelvin: Decimal }` | SI | °C vs °F vs K | candidate | core | standard |
| `TemperatureUnit` | K, °C, °F | `Enum` | SI | — | candidate | likely | folded → `Temperature` |
| `Celsius` | Degrees Celsius | `Temperature` constructor | SI | — | candidate | likely | folded → `Temperature` |
| `Fahrenheit` | Degrees Fahrenheit | `Temperature` constructor | — | — | candidate | likely | folded → `Temperature` |
| `TemperatureDifference` | A change in temperature | `Object { kelvin: Decimal }` | SI | Offset vs scale conversion | candidate | maybe | standard |
| `Rankine` | Absolute Fahrenheit | `Temperature` constructor | — | — | candidate | silly | out: silly |
| `Speed` | A speed | `Object { metres_per_second: Decimal }` | SI | mph vs km/h vs knots | candidate | likely | standard |
| `SpeedUnit` | m/s, km/h, mph, kn, Mach | `Enum` | SI | — | candidate | maybe | folded → `Speed` |
| `Knot` | Nautical miles per hour | `Speed` constructor | — | — | candidate | niche | out: niche |
| `Pace` | Time per distance (runners) | `Object { per_kilometre: Duration }` | — | min/km vs min/mile | candidate | maybe | out: no standard |
| `Velocity` | Speed with direction | `Object { speed: Speed, bearing: Bearing }` | — | — | candidate | niche | out: niche |
| `Acceleration` | m/s² | `Decimal` | SI | g vs m/s² | candidate | niche | out: niche |
| `GForce` | Acceleration in g | `Decimal` | — | — | candidate | silly | out: silly |
| `Frequency` | A frequency | `Object { hertz: Decimal }` | SI | — | candidate | likely | standard |
| `FrequencyUnit` | Hz, kHz, MHz, GHz, rpm | `Enum` | SI | — | candidate | niche | out: niche |
| `Rpm` | Revolutions per minute | `Frequency` constructor | — | — | candidate | niche | out: niche |
| `Voltage` | Electric potential | `Object { volts: Decimal }` | SI | — | candidate | maybe | standard |
| `Current` | Electric current | `Object { amperes: Decimal }` | SI | mA vs A | candidate | maybe | standard |
| `Resistance` | Electrical resistance | `Object { ohms: Decimal }` | SI | — | candidate | niche | out: niche |
| `Capacitance` | Farads | `Decimal` | SI | — | candidate | niche | out: niche |
| `Inductance` | Henries | `Decimal` | SI | — | candidate | silly | out: silly |
| `Charge` | Coulombs or mAh | `Object { coulombs: Decimal }` | SI | mAh battery ratings | candidate | niche | out: niche |
| `BatteryCapacity` | Battery energy or charge | `Enum { energy(Energy), charge(Charge) }` | — | Wh vs mAh | candidate | maybe | out: no standard |
| `BatteryLevel` | State of charge | `Percentage` | — | 0–1 vs 0–100 | candidate | maybe | out: no standard |
| `Luminance` | Candela per m² | `Decimal` | SI | nits | candidate | niche | out: niche |
| `Illuminance` | Lux | `Decimal` | SI | — | candidate | maybe | standard |
| `LuminousFlux` | Lumens | `Decimal` | SI | Bulb brightness | candidate | niche | out: niche |
| `LuminousIntensity` | Candela | `Decimal` | SI | — | candidate | silly | out: silly |
| `ColourTemperature` | Light colour in kelvin | `Int { min: 1000, max: 20000 }` | CIE | — | candidate | niche | out: niche |
| `SoundLevel` | Decibels with weighting | `Object { decibels: Decimal, weighting: Enum { a, c, z } }` | IEC 61672 | dB vs dBA | candidate | maybe | standard |
| `Decibel` | A logarithmic ratio | `Decimal` | ISO 80000-3 | dBm vs dBi vs dB | candidate | niche | out: niche |
| `RadiationDose` | Sieverts | `Decimal` | SI | Gy vs Sv vs rem | candidate | niche | out: niche |
| `Radioactivity` | Becquerels | `Decimal` | SI | — | candidate | silly | out: silly |
| `AmountOfSubstance` | Moles | `Decimal` | SI | — | candidate | niche | out: niche |
| `Concentration` | Amount per volume | `Object { value: Decimal, unit: ConcentrationUnit }` | SI, UCUM | mg/dL vs mmol/L | candidate | maybe | standard |
| `FlowRate` | Volume per time | `Object { cubic_metres_per_second: Decimal }` | SI | l/min vs gpm | candidate | niche | out: niche |
| `Viscosity` | Pascal-seconds | `Decimal` | SI | — | candidate | silly | out: silly |
| `Humidity` | Relative humidity | `Percentage` | WMO | — | candidate | maybe | folded → `Percentage` |
| `Ph` | Acidity | `Decimal { min: 0, max: 14 }` | IUPAC | — | candidate | niche | out: niche |
| `Wavelength` | Light wavelength | `Length` | — | nm vs Å | candidate | niche | out: niche |
| `FuelEconomy` | Fuel use per distance | `Object { litres_per_100km: Decimal }` | — | mpg (UK) vs mpg (US) vs l/100km — inverse | candidate | maybe | out: no standard |
| `EnergyEfficiency` | Distance per energy (EVs) | `Object { wh_per_km: Decimal }` | — | mi/kWh vs Wh/km | candidate | niche | out: niche |
| `CarbonDioxideEquivalent` | Greenhouse emissions | `Object { kg_co2e: Decimal }` | GHG Protocol | t vs kg, CO2 vs CO2e | candidate | maybe | folded → `Mass` |
| `PhysicalQuantity` | Any SI-dimensioned value | `Object { value: Decimal, unit: Ucum }` | UCUM | — | candidate | maybe | folded → `Quantity` |
| `Ucum` | A unit expression | `Text` validated | UCUM | "mcg" vs "ug" | candidate | maybe | folded → `Quantity` |

## Data size and digital quantities

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `DataSize` | An amount of data | `Object { bytes: Int { min: 0 } }` | IEC 80000-13 | KB 1000 vs KiB 1024 | candidate | core | standard |
| `DataSizeUnit` | B, kB, MB, KiB, MiB… | `Enum` | IEC 80000-13 | — | candidate | likely | folded → `DataSize` |
| `Bitrate` | Data per second | `Object { bits_per_second: Int }` | — | bits vs bytes | candidate | likely | standard |
| `Bandwidth` | Network capacity | `Bitrate` | — | Mbps vs MB/s | candidate | maybe | out: no standard |
| `Latency` | Network delay | `Duration` | — | — | candidate | maybe | out: no standard |
| `Throughput` | Operations per time | `Object { count: Int, per: Duration }` | — | — | candidate | niche | out: niche |
| `StorageQuota` | Allowed storage | `DataSize` | — | — | candidate | niche | out: niche |

## Maths and numbers

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Percentage` | A proportion where 0.20 is 20% | `Decimal` | — | 0.2 vs 20 | proposed | core | standard |
| `Ratio` | A ratio of two quantities | `Object { numerator: Decimal, denominator: Decimal }` | — | 16:9 vs 1.777 | candidate | likely | standard |
| `Fraction` | An exact rational number | `Object { numerator: Int, denominator: Int { min: 1 } }` | — | Float rounding | candidate | likely | standard |
| `Proportion` | A value between 0 and 1 | `Decimal { min: 0, max: 1 }` | — | 0–1 vs 0–100 | candidate | likely | folded → `Ratio` |
| `PerMille` | Parts per thousand | `Decimal` | — | — | candidate | niche | out: niche |
| `PartsPerMillion` | ppm | `Decimal` | — | — | candidate | niche | out: niche |
| `Range` | A bounded range of an ordered type | `Object { start: T, end: T, start_inclusive: Bool, end_inclusive: Bool }` | ISO 31-11 | Inclusive vs exclusive | candidate | core | standard |
| `NumericInterval` | An interval of numbers | `Range<Decimal>` | — | — | candidate | maybe | out: no standard |
| `Complex` | A complex number | `Object { real: Decimal, imaginary: Decimal }` | — | (re, im) pair | candidate | niche | out: niche |
| `Vector2` | A 2D vector | `Object { x: Decimal, y: Decimal }` | — | — | candidate | maybe | out: no standard |
| `Vector3` | A 3D vector | `Object { x, y, z: Decimal }` | — | Y-up vs Z-up | candidate | maybe | out: no standard |
| `Matrix` | A rectangular grid of numbers | `Object { rows: Int, columns: Int, values: List<Decimal> }` | — | Row- vs column-major | candidate | maybe | out: no standard |
| `Matrix4x4` | A 3D transform | `Matrix` of 4×4 | — | Row- vs column-major | candidate | niche | out: niche |
| `Quaternion` | A rotation | `Object { w, x, y, z: Decimal }` | — | wxyz vs xyzw | candidate | niche | out: niche |
| `EulerAngles` | Roll, pitch, yaw | `Object { roll, pitch, yaw: Angle, order: Enum }` | — | Rotation order | candidate | niche | out: niche |
| `Tensor` | An n-dimensional array | `Object { shape: List<Int>, values: List<Decimal> }` | — | — | candidate | niche | out: niche |
| `Statistic` | A summary value with count | `Object { mean, min, max, stddev: Decimal, count: Int }` | — | — | candidate | niche | out: niche |
| `ConfidenceInterval` | An estimate with bounds | `Object { estimate: Decimal, lower: Decimal, upper: Decimal, level: Percentage }` | — | — | candidate | niche | out: niche |
| `Probability` | A probability | `Decimal { min: 0, max: 1 }` | — | — | candidate | maybe | out: no standard |
| `Odds` | Betting odds | `Enum { fractional(Fraction), decimal(Decimal), american(Int) }` | — | Three notations | candidate | niche | out: niche |
| `Score` | A score out of a maximum | `Object { value: Decimal, out_of: Decimal }` | — | — | candidate | maybe | out: no standard |
| `Rank` | An ordinal position | `Int { min: 1 }` | — | 0- vs 1-based | candidate | maybe | out: no standard |
| `Ordinal` | First, second… | `Int { min: 1 }` | CLDR ordinal rules | — | candidate | niche | out: niche |
| `RomanNumeral` | I, II, IV… | `Text` conversion edge | — | — | candidate | silly | out: silly |
| `BigInteger` | An arbitrary-size integer | `Text { pattern: "-?[0-9]+" }` | — | JSON numbers lose precision past 2⁵³ | candidate | maybe | out: no standard |
| `Hex` | A hex-encoded value | `Text { pattern: "[0-9a-f]*" }` | RFC 4648 | Case and 0x prefix | candidate | maybe | out: format |
| `Base64` | Base64-encoded bytes | Wire form of `Bytes` | RFC 4648 | Standard vs URL-safe, padding | candidate | likely | out: format |
| `Base32` | Base32-encoded bytes | `Text` | RFC 4648 | — | candidate | niche | out: niche |
| `Base58` | Base58-encoded bytes | `Text` | Bitcoin | — | candidate | niche | out: niche |
| `Checksum` | A check value with its algorithm | `Object { algorithm: Enum, value: Text }` | — | — | candidate | maybe | out: no standard |
| `LuhnNumber` | A Luhn-checked number | `Text` with mod-10 check | ISO/IEC 7812-1 | — | candidate | niche | out: niche |
| `SignificantFigures` | A measured value with precision | `Object { value: Decimal, figures: Int }` | — | — | candidate | silly | out: silly |
| `Uncertainty` | A value plus or minus | `Object { value: Decimal, plus_minus: Decimal }` | GUM | — | candidate | niche | out: niche |
| `Histogram` | Bucketed counts | `List<Object { upper: Decimal, count: Int }>` | OpenMetrics | — | candidate | niche | out: niche |
| `SequenceNumber` | A monotonic counter | `Int { min: 0 }` | — | — | candidate | niche | out: niche |
| `StatisticalWeight` | A weighting factor | `Decimal { min: 0 }` | — | — | candidate | silly | out: silly |

## Science

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `ChemicalFormula` | A molecular formula | `Text`, `C6H12O6` | IUPAC | — | candidate | niche | out: niche |
| `CasNumber` | Chemical substance registry number | `Text { pattern: "[0-9]{2,7}-[0-9]{2}-[0-9]" }` | CAS | Check digit | candidate | niche | out: niche |
| `Inchi` | International chemical identifier | `Text { pattern: "InChI=.*" }` | IUPAC | — | candidate | niche | out: niche |
| `InchiKey` | Hashed InChI | `Text { pattern: "[A-Z]{14}-[A-Z]{10}-[A-Z]" }` | IUPAC | — | candidate | niche | out: niche |
| `Smiles` | Molecular notation | `Text` | OpenSMILES | Canonical vs not | candidate | niche | out: niche |
| `ChemicalElement` | An element | `Enum` of symbols | IUPAC | — | candidate | niche | out: niche |
| `EcNumber` | EU chemical number | `Text { pattern: "[0-9]{3}-[0-9]{3}-[0-9]" }` | ECHA | — | candidate | silly | out: silly |
| `UnNumber` | Dangerous goods number | `Text { pattern: "UN[0-9]{4}" }` | UN Model Regulations | — | candidate | maybe | standard |
| `GhsHazard` | Chemical hazard statement | `Text { pattern: "H[0-9]{3}" }` | UN GHS | — | candidate | niche | out: niche |
| `DnaSequence` | A nucleotide sequence | `Text { pattern: "[ACGTN]*" }` | IUPAC nucleotide codes | RNA U vs T | candidate | niche | out: niche |
| `ProteinSequence` | An amino-acid sequence | `Text` | IUPAC | — | candidate | niche | out: niche |
| `GeneSymbol` | A gene name | `Text` | HGNC | Excel turning SEPT1 into dates | candidate | niche | out: niche |
| `Taxon` | A biological species | `Object { scientific_name: Text, rank: Enum }` | ICZN, ICN; NCBI Taxonomy | — | candidate | niche | out: niche |
| `NcbiTaxonId` | NCBI taxonomy id | `Int` | NCBI | — | candidate | silly | out: silly |
| `Doi` | Digital object identifier | `Text { pattern: "10\.[0-9]{4,9}/.+" }` | ISO 26324 | "doi:" or URL prefix | candidate | maybe | standard |
| `Orcid` | Researcher identifier | `Text { pattern: "[0-9]{4}-[0-9]{4}-[0-9]{4}-[0-9]{3}[0-9X]" }` | ORCID | Checksum | candidate | maybe | standard |
| `ArxivId` | arXiv paper id | `Text` | arXiv | Old vs new scheme | candidate | niche | out: niche |
| `PubmedId` | PubMed article id | `Int` | NLM | — | candidate | niche | out: niche |
| `RorId` | Research organisation id | `Text` | ROR | — | candidate | silly | out: silly |
| `Citation` | A bibliographic reference | `Object` CSL-JSON | CSL | BibTeX vs RIS vs CSL | candidate | niche | out: niche |
| `CelestialCoordinate` | Right ascension and declination | `Object { right_ascension: Angle, declination: Angle, epoch: Text }` | IAU | Hours vs degrees for RA | candidate | niche | out: niche |
| `ApparentMagnitude` | Apparent brightness | `Decimal` | — | — | candidate | silly | out: silly |
| `EarthquakeMagnitude` | Seismic magnitude with scale | `Object { value: Decimal, scale: Enum { mw, ml, mb } }` | USGS | — | candidate | silly | out: silly |

## Health and medicine

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `NhsNumber` | NHS England patient number | `Text { pattern: "[0-9]{10}" }` with mod-11 check | NHS Data Dictionary | Spaces; check digit | candidate | likely | standard |
| `ChiNumber` | Scottish patient number | `Text { pattern: "[0-9]{10}" }` | NHS Scotland | — | candidate | niche | out: niche |
| `HcNumber` | Northern Ireland patient number | `Text` | HSC NI | — | candidate | silly | out: silly |
| `MedicareNumber` | Australian Medicare number | `Text` | Services Australia | — | candidate | niche | out: niche |
| `Npi` | US national provider identifier | `Text { pattern: "[0-9]{10}" }` with Luhn | CMS | — | candidate | niche | out: niche |
| `GmcNumber` | UK doctor registration | `Text { pattern: "[0-9]{7}" }` | GMC | — | candidate | niche | out: niche |
| `NmcPin` | UK nurse registration | `Text` | NMC | — | candidate | silly | out: silly |
| `OdsCode` | NHS organisation code | `Text` | NHS ODS | — | candidate | niche | out: niche |
| `BloodType` | ABO and Rh blood group | `Object { abo: Enum { a, b, ab, o }, rh: Enum { positive, negative } }` | ISBT 128 | "O+" vs "O pos" vs "0+" | candidate | maybe | standard |
| `Icd10Code` | Diagnosis code | `Text { pattern: "[A-Z][0-9]{2}(\.[0-9A-Z]{1,4})?" }` | WHO ICD-10 | Dot or not; ICD-10-CM vs WHO | candidate | maybe | standard |
| `Icd11Code` | ICD-11 diagnosis code | `Text` | WHO ICD-11 | — | candidate | niche | out: niche |
| `SnomedConcept` | A SNOMED CT concept | `Object { id: Int, term: Text }` | SNOMED CT | — | candidate | maybe | standard |
| `LoincCode` | Lab observation code | `Text { pattern: "[0-9]{1,5}-[0-9]" }` | LOINC | — | candidate | niche | out: niche |
| `CptCode` | US procedure code | `Text { pattern: "[0-9]{4}[0-9A-Z]" }` | AMA CPT | — | candidate | niche | out: niche |
| `OpcsCode` | UK procedure code | `Text` | NHS OPCS-4 | — | candidate | niche | out: niche |
| `AtcCode` | Drug classification | `Text { pattern: "[A-Z][0-9]{2}[A-Z]{2}[0-9]{2}" }` | WHO ATC | — | candidate | niche | out: niche |
| `DmdCode` | UK medicine code | `Int` | NHS dm+d | — | candidate | niche | out: niche |
| `Ndc` | US national drug code | `Text` | FDA | 4-4-2 vs 5-3-2 vs 5-4-1 | candidate | niche | out: niche |
| `RxNormCode` | US drug concept | `Int` | NLM RxNorm | — | candidate | niche | out: niche |
| `Dose` | An amount of medicine | `Object { amount: Decimal, unit: Ucum }` | UCUM | mg vs mcg tenfold errors | candidate | likely | standard |
| `DosageInstruction` | How to take a medicine | `Object { dose: Dose, route: Route, frequency: Timing, as_needed: Bool }` | FHIR Dosage | Free text "bd" ambiguity | candidate | maybe | standard |
| `RouteOfAdministration` | Oral, IV, topical | `Enum` | SNOMED CT, EDQM | — | candidate | niche | out: niche |
| `DoseFrequency` | Times per day | `Object { count: Int, per: Duration }` | FHIR Timing | bd/tds/qds | candidate | maybe | folded → `DosageInstruction` |
| `HeartRate` | Beats per minute | `Int { min: 0, max: 300 }` | LOINC 8867-4 | — | candidate | maybe | standard |
| `RespiratoryRate` | Breaths per minute | `Int` | LOINC 9279-1 | — | candidate | niche | out: niche |
| `OxygenSaturation` | SpO₂ | `Percentage` | LOINC 59408-5 | — | candidate | niche | out: niche |
| `BodyTemperature` | Core temperature | `Temperature` | LOINC 8310-5 | °F vs °C | candidate | maybe | folded → `Temperature` |
| `BloodGlucose` | Glucose level | `Concentration` | LOINC | mg/dL vs mmol/L (18×) | candidate | maybe | standard |
| `Bmi` | Body mass index | `Decimal { scale: 1 }` | WHO | — | candidate | maybe | out: niche |
| `Allergy` | An allergy or intolerance | `Object { substance: SnomedConcept, severity: Enum, reaction: Text }` | FHIR AllergyIntolerance | — | candidate | maybe | out: app entity |
| `Vaccination` | A vaccine given | `Object { vaccine: Text, given: CalendarDate, dose_number: Int }` | FHIR Immunization, CVX | — | candidate | niche | out: niche |
| `VisualAcuity` | Eyesight measurement | `Text`, 6/6 or 20/20 | — | Metric vs imperial notation | candidate | silly | out: silly |
| `SpectaclePrescription` | Sphere, cylinder, axis per eye | `Object { left: EyeRx, right: EyeRx }` | — | Plus vs minus cylinder | candidate | niche | out: niche |
| `GestationalAge` | Weeks and days of pregnancy | `Object { weeks: Int, days: Int { min: 0, max: 6 } }` | — | — | candidate | niche | out: niche |
| `ApgarScore` | Newborn score | `Int { min: 0, max: 10 }` | — | — | candidate | silly | out: silly |
| `PainScore` | Pain on a 0–10 scale | `Int { min: 0, max: 10 }` | NRS | — | candidate | silly | out: silly |
| `GlasgowComaScale` | Consciousness score | `Object { eye: Int, verbal: Int, motor: Int }` | — | — | candidate | silly | out: silly |
| `Steps` | Step count | `Int { min: 0 }` | — | — | candidate | niche | out: niche |
| `SleepStage` | Awake, light, deep, REM | `Enum` | AASM | — | candidate | niche | out: niche |
| `MenstrualFlow` | Flow level | `Enum` | Apple HealthKit, FHIR | — | candidate | silly | out: silly |
| `FhirResourceRef` | A reference to an FHIR resource | `Text { pattern: "[A-Z][A-Za-z]+/[A-Za-z0-9.-]+" }` | HL7 FHIR | — | candidate | niche | out: niche |
| `DicomUid` | A DICOM object id | `Text { pattern: "[0-9.]{1,64}" }` | DICOM | — | candidate | niche | out: niche |

## Colour, images and visual media

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Colour` | An sRGB colour with alpha | `Object { red, green, blue, alpha: Int 0–255 }` | sRGB, CSS Color 4 | #RGB vs rgba() vs 0–1 floats | proposed | core | standard |
| `HexColour` | #rrggbb notation | Conversion edge of `Colour` | CSS Color 4 | With or without # | candidate | maybe | folded → `Colour` |
| `HslColour` | Hue, saturation, lightness | `Object { hue: Angle, saturation: Percentage, lightness: Percentage }` | CSS Color 4 | 0–1 vs 0–100 | candidate | maybe | folded → `Colour` |
| `OklchColour` | Perceptual colour | `Object { lightness, chroma: Decimal, hue: Angle }` | CSS Color 4 | — | candidate | niche | out: niche |
| `CmykColour` | Print colour | `Object { cyan, magenta, yellow, key: Percentage }` | ISO 12647 | — | candidate | niche | out: niche |
| `LabColour` | CIELAB colour | `Object { l, a, b: Decimal }` | CIE 1976 | — | candidate | niche | out: niche |
| `PantoneColour` | A Pantone reference | `Text` | Pantone (proprietary) | — | candidate | silly | out: silly |
| `RalColour` | A RAL paint code | `Text { pattern: "RAL [0-9]{4}" }` | RAL | — | candidate | silly | out: silly |
| `ColourName` | A named CSS colour | `Enum` | CSS Color 4 | — | candidate | niche | out: niche |
| `Gradient` | A colour gradient | `Object { stops: List<Object { at: Proportion, colour: Colour }>, angle: Angle }` | CSS Images | — | candidate | niche | out: niche |
| `Palette` | An ordered set of colours | `List<Colour>` | — | — | candidate | niche | out: niche |
| `ColourSpace` | sRGB, Display P3, Rec. 2020 | `Enum` | ICC | Mixed spaces wash out | candidate | niche | out: niche |
| `ImageDimensions` | Width and height in pixels | `Object { width: Int { min: 1 }, height: Int { min: 1 } }` | — | w×h vs h×w | candidate | core | standard |
| `AspectRatio` | Width to height | `Object { width: Int, height: Int }` | — | 16:9 vs 1.78 vs 0.5625 | candidate | likely | standard |
| `Orientation` | Portrait or landscape | `Enum` | — | — | candidate | maybe | out: no standard |
| `ExifOrientation` | Image rotation flag | `Int { min: 1, max: 8 }` | EXIF 2.32 | Sideways photos | candidate | maybe | standard |
| `FocalPoint` | Point of interest in an image | `Object { x: Proportion, y: Proportion }` | — | Pixels vs fractions | candidate | maybe | out: no standard |
| `CropRect` | A crop region | `Object { x, y, width, height: Int }` | — | Corner vs centre origin | candidate | maybe | out: no standard |
| `Rect` | A rectangle | `Object { x, y, width, height: Decimal }` | — | x,y,w,h vs x1,y1,x2,y2 | candidate | likely | standard |
| `Size2D` | Width and height | `Object { width: Decimal, height: Decimal }` | — | — | candidate | maybe | out: no standard |
| `Insets` | Top, right, bottom, left | `Object { top, right, bottom, left: Decimal }` | CSS box model | TRBL vs TLBR order | candidate | maybe | standard |
| `ImageFormat` | PNG, JPEG, WebP, AVIF… | `FileType` subset | IANA media types | — | candidate | maybe | folded → `FileType` |
| `Blurhash` | A compact image placeholder | `Text` | blurha.sh | — | candidate | niche | out: niche |
| `Thumbhash` | A compact image placeholder | `Bytes` | thumbhash | — | candidate | silly | out: silly |
| `ImageSet` | Responsive image variants | `List<Object { url: Url, width: Int, density: Decimal }>` | HTML srcset | — | candidate | niche | out: niche |
| `AltText` | An image description | `Text { max_length: 1000 }` | WCAG 2.2 | Missing or "image" | candidate | likely | standard |
| `Icon` | A named icon | `Text` | — | — | candidate | silly | out: silly |
| `Favicon` | A site icon set | `List<Object { url: Url, size: ImageDimensions }>` | W3C manifest | — | candidate | niche | out: niche |
| `Dpi` | Print resolution | `Int { min: 1 }` | — | — | candidate | niche | out: niche |
| `PaperSize` | A4, Letter… | `Enum` | ISO 216, ANSI | A4 vs Letter defaults | candidate | maybe | standard |
| `IiifImage` | A IIIF image service | `Url` | IIIF | — | candidate | silly | out: silly |
| `ExifData` | Camera metadata | `Object` of named EXIF tags | EXIF | GPS stripped or leaked | candidate | niche | out: niche |
| `Font` | A font family and weight | `Object { family: Text, weight: Int { min: 1, max: 1000 }, style: Enum }` | CSS Fonts 4 | "bold" vs 700 | candidate | niche | out: niche |
| `FontWeight` | 100–900 | `Int` | CSS Fonts 4 | — | candidate | niche | out: niche |

## Audio and video

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `MediaDuration` | Length of a recording | `Duration` | — | s vs ms vs "mm:ss" | candidate | likely | folded → `Duration` |
| `FrameRate` | Frames per second | `Fraction`, 30000/1001 | SMPTE | 29.97 rounding | candidate | maybe | standard |
| `VideoResolution` | 720p, 1080p, 4K | `ImageDimensions` | ITU-R BT.709 | "4K" ambiguity | candidate | maybe | standard |
| `VideoCodec` | H.264, HEVC, AV1, VP9 | `Text` RFC 6381 codecs string | RFC 6381 | "avc1.640028" vs "h264" | candidate | maybe | standard |
| `AudioCodec` | AAC, Opus, MP3, FLAC | `Text` | RFC 6381 | — | candidate | maybe | standard |
| `MediaContainer` | MP4, WebM, MKV | `FileType` | ISO/IEC 14496-12 | — | candidate | niche | out: niche |
| `SampleRate` | Audio samples per second | `Int`, 44100 | — | kHz vs Hz | candidate | maybe | out: no standard |
| `BitDepth` | Bits per sample | `Int` | — | — | candidate | niche | out: niche |
| `ChannelLayout` | Mono, stereo, 5.1 | `Enum` | ITU-R BS.2051 | — | candidate | niche | out: niche |
| `Loudness` | Integrated loudness | `Object { lufs: Decimal }` | EBU R128, ITU-R BS.1770 | LUFS vs LKFS vs dB | candidate | niche | out: niche |
| `PlaybackVolume` | Playback volume | `Proportion` | — | 0–1 vs 0–100 | candidate | maybe | out: no standard |
| `SubtitleTrack` | Timed text in a language | `Object { language: Language, cues: List<Cue> }` | WebVTT | SRT vs VTT timing | candidate | maybe | folded → `SubtitleCue` |
| `Cue` | A timed caption | `Object { start: Duration, end: Duration, text: Text }` | WebVTT | — | candidate | maybe | standard, as `SubtitleCue` |
| `Chapter` | A named point in media | `Object { start: Duration, title: Text }` | — | — | candidate | niche | out: niche |
| `MusicalKey` | A key (C major, A minor) | `Object { tonic: PitchClass, mode: Enum }` | — | Camelot vs standard notation | candidate | niche | out: niche |
| `Tempo` | Beats per minute | `Decimal` | — | — | candidate | niche | out: niche |
| `TimeSignature` | 4/4, 6/8 | `Object { beats: Int, unit: Int }` | — | — | candidate | silly | out: silly |
| `MusicalNote` | A pitch and octave | `Object { pitch: PitchClass, octave: Int }` | Scientific pitch | Middle C octave 3 vs 4 | candidate | silly | out: silly |
| `MidiNote` | MIDI note number | `Int { min: 0, max: 127 }` | MIDI 1.0 | — | candidate | silly | out: silly |
| `Isrc` | Recording identifier | `Text { pattern: "[A-Z]{2}[A-Z0-9]{3}[0-9]{7}" }` | ISO 3901 | Hyphens | candidate | niche | out: niche |
| `Iswc` | Musical work identifier | `Text` | ISO 15707 | — | candidate | silly | out: silly |
| `Eidr` | Film and TV identifier | `Text { pattern: "10\.5240/.*" }` | EIDR | — | candidate | silly | out: silly |
| `ImdbId` | IMDb title id | `Text { pattern: "tt[0-9]{7,8}" }` | IMDb | — | candidate | niche | out: niche |
| `ContentRating` | Age rating with scheme | `Object { scheme: Enum { bbfc, mpaa, pegi, esrb… }, rating: Text }` | BBFC, MPA, PEGI | Mixed schemes | candidate | maybe | standard |
| `PlaybackPosition` | Where a viewer stopped | `Duration` | — | — | candidate | niche | out: niche |
| `StreamingManifest` | HLS or DASH manifest URL | `Url` | RFC 8216, ISO/IEC 23009 | — | candidate | niche | out: niche |
| `PodcastEpisode` | An episode with enclosure | `Object { title: Text, audio: Url, duration: Duration }` | RSS 2.0, Podcasting 2.0 | — | candidate | niche | out: niche |
| `Waveform` | Audio peaks for display | `List<Decimal>` | — | — | candidate | silly | out: silly |
| `PlatformMediaId` | A platform media id | `Object { platform: Text, id: Text }` | — | — | candidate | silly | out: silly |

## Web and HTTP

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Url` | An absolute URL | Validated `Text` | WHATWG URL, RFC 3986 | Relative vs absolute, encoding | exists | core | standard |
| `Uri` | Any URI including non-HTTP schemes | `Text` validated | RFC 3986 | URL vs URI vs IRI | candidate | maybe | folded → `Url` |
| `UrlPath` | A path component | `Text { pattern: "/.*" }` | RFC 3986 | Leading slash, encoding | candidate | maybe | standard |
| `RelativeUrl` | A URL reference | `Text` | RFC 3986 | — | candidate | maybe | standard |
| `QueryString` | Decoded query parameters | `List<Object { name: Text, value: Text }>` | WHATWG URLSearchParams | Repeated keys lost in a Map | candidate | maybe | folded → `Url` |
| `UrlScheme` | http, https, mailto… | `Text` | IANA URI schemes | — | candidate | niche | out: niche |
| `Slug` | A URL-safe identifier | `Text { pattern: "[a-z0-9]+(-[a-z0-9]+)*" }` | — | Unicode and case handling | candidate | likely | standard |
| `DomainName` | A DNS name | `Text`, normalised lowercase and A-label | RFC 1035, RFC 5890 (IDNA) | Trailing dot, case, punycode | candidate | core | standard |
| `Hostname` | A host name or IP | `Enum { domain(DomainName), ip(IpAddress) }` | RFC 3986 | — | candidate | likely | standard |
| `Origin` | Scheme, host and port | `Object { scheme: Text, host: Hostname, port: Port? }` | RFC 6454 | Default port included or not | candidate | likely | standard |
| `HttpMethod` | GET, POST… | `Enum` | RFC 9110 | Case | candidate | likely | standard |
| `HttpStatus` | A response status code | `Int { min: 100, max: 599 }` with names | RFC 9110 | — | candidate | likely | standard |
| `HttpHeaderName` | A header field name | `Text`, case-insensitive | RFC 9110 | Case comparison bugs | candidate | maybe | standard |
| `HttpHeaders` | A header list | `List<Object { name, value }>` | RFC 9110 | Repeated headers lost in a Map | candidate | maybe | folded → `HttpHeaderName` |
| `MediaType` | A MIME type with parameters | `Object { type: Text, subtype: Text, parameters: Map<Text, Text> }` | RFC 6838 | charset parameter dropped | candidate | likely | folded → `FileType` |
| `FileType` | Enum of media types | As area 16 | IANA media types | — | exists | core | standard |
| `ContentEncoding` | gzip, br, zstd | `Enum` | RFC 9110 | — | candidate | niche | out: niche |
| `Charset` | A character encoding | `Enum` | IANA charsets | latin-1 vs windows-1252 | candidate | niche | out: niche |
| `Cookie` | A Set-Cookie value | `Object { name, value, domain, path, expires, secure, http_only, same_site }` | RFC 6265bis | SameSite defaults | candidate | maybe | standard |
| `ETag` | An entity tag | `Object { value: Text, weak: Bool }` | RFC 9110 | Quotes and W/ | candidate | niche | out: niche |
| `CacheControl` | Cache directives | `Object { max_age: Duration?, no_store: Bool… }` | RFC 9111 | — | candidate | niche | out: niche |
| `UserAgent` | A user agent string | `Text` | RFC 9110 | Parsing differs; prefer Client Hints | candidate | maybe | standard |
| `AcceptLanguage` | Language preferences | `List<Object { language: Language, quality: Decimal }>` | RFC 9110 | — | candidate | maybe | out: a call |
| `ProblemDetails` | An HTTP API error body | `Object { type: Url, title: Text, status: HttpStatus, detail: Text?, instance: Uri? }` | RFC 9457 | Every API's own error shape | candidate | likely | standard |
| `LinkHeader` | Pagination links | `List<Object { url: Url, rel: Text }>` | RFC 8288 | — | candidate | niche | out: niche |
| `PageCursor` | An opaque pagination cursor | `Text` | — | Offset vs cursor | candidate | maybe | out: no standard |
| `Pagination` | Page request | `Object { limit: Int, cursor: PageCursor? }` | — | page/per_page vs offset/limit | candidate | maybe | out: no standard |
| `JsonPointer` | A pointer into JSON | `Text` | RFC 6901 | Escaping ~0 ~1 | candidate | maybe | standard |
| `JsonPatch` | A JSON patch | `List<PatchOperation>` | RFC 6902 | vs merge patch | candidate | niche | out: niche |
| `JsonPath` | A JSON query path | `Text` | RFC 9535 | Dialects | candidate | niche | out: niche |
| `JsonSchemaRef` | A JSON schema reference | `Uri` | JSON Schema 2020-12 | — | candidate | niche | out: niche |
| `OpenGraph` | Link preview metadata | `Object { title, description, image: Url, url: Url }` | Open Graph protocol | — | candidate | niche | out: niche |
| `RobotsDirective` | index, nofollow | `Set<Enum>` | RFC 9309 | — | candidate | silly | out: silly |
| `CanonicalUrl` | The canonical URL of a page | `Url` | RFC 6596 | — | candidate | niche | out: niche |
| `WebhookSignature` | An HMAC-signed payload header | `Object { algorithm: Enum, signature: Bytes, timestamp: Instant }` | Standard Webhooks | Each provider's own | candidate | maybe | folded → `Hmac` |
| `CorsPolicy` | Allowed origins and methods | `Object { origins: Set<Origin>, methods: Set<HttpMethod> }` | Fetch standard | — | candidate | niche | out: niche |
| `CspDirective` | Content security policy | `Map<Text, List<Text>>` | CSP Level 3 | — | candidate | niche | out: niche |
| `SitemapEntry` | A URL in a sitemap | `Object { url: Url, modified: Instant? }` | sitemaps.org | — | candidate | silly | out: silly |
| `UtmParameters` | Campaign tracking | `Object { source, medium, campaign, term, content: Text? }` | Google Analytics convention | — | candidate | maybe | standard |
| `DeepLink` | An app link | `Uri` | — | — | candidate | niche | out: niche |
| `Emoji` | A single emoji | `Text`, one grapheme from the Unicode emoji list | Unicode UTS #51 | Skin tone modifiers, ZWJ | candidate | maybe | standard |
| `EmojiShortcode` | :smile: | `Text` | CLDR annotations, GitHub | Platform-specific names | candidate | silly | out: silly |

## Networks

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `IpAddress` | An IPv4 or IPv6 address | Validated `Text` | RFC 791, RFC 4291, RFC 5952 | IPv6 compression forms | exists | core | standard |
| `Ipv4Address` | An IPv4 address | `IpAddress` refinement | RFC 791 | Leading zeros as octal | candidate | maybe | folded → `IpAddress` |
| `Ipv6Address` | An IPv6 address | `IpAddress` refinement | RFC 5952 | Many textual forms | candidate | maybe | folded → `IpAddress` |
| `Cidr` | An IP network | `Object { address: IpAddress, prefix: Int }` | RFC 4632 | Host bits set | candidate | likely | standard |
| `IpRange` | A start–end IP range | `Object { start: IpAddress, end: IpAddress }` | — | — | candidate | niche | out: niche |
| `Port` | A TCP or UDP port | `Int { min: 0, max: 65535 }` | IANA | 0 allowed or not | candidate | likely | standard |
| `SocketAddress` | Host and port | `Object { host: Hostname, port: Port }` | — | IPv6 brackets | candidate | maybe | out: no standard |
| `MacAddress` | A hardware address | `Text` normalised `aa:bb:cc:dd:ee:ff` | IEEE EUI-48 | Dashes vs colons vs dots | candidate | likely | standard |
| `Eui64` | A 64-bit hardware id | `Text` | IEEE EUI-64 | — | candidate | niche | out: niche |
| `Asn` | Autonomous system number | `Int { min: 0, max: 4294967295 }` | RFC 6793 | "AS" prefix | candidate | niche | out: niche |
| `DnsRecordType` | A, AAAA, MX, TXT… | `Enum` | IANA DNS parameters | — | candidate | maybe | out: niche |
| `DnsRecord` | A resource record | `Object { name: DomainName, type: DnsRecordType, ttl: Duration, value: Text }` | RFC 1035 | — | candidate | niche | out: niche |
| `Ttl` | Time to live | `Duration` | — | seconds assumed | candidate | niche | out: niche |
| `Protocol` | TCP, UDP, ICMP | `Enum` | IANA protocol numbers | — | candidate | niche | out: niche |
| `WifiSsid` | A network name | `Text { max_length: 32 }` (bytes) | IEEE 802.11 | — | candidate | niche | out: niche |
| `Bssid` | An access point address | `MacAddress` | IEEE 802.11 | — | candidate | silly | out: silly |
| `Rssi` | Signal strength | `Object { dbm: Int }` | — | dBm vs percent | candidate | niche | out: niche |
| `Imei` | Mobile device identifier | `Text { pattern: "[0-9]{15}" }` with Luhn | 3GPP TS 23.003 | — | candidate | maybe | standard |
| `Imsi` | SIM subscriber id | `Text { pattern: "[0-9]{14,15}" }` | ITU-T E.212 | — | candidate | niche | out: niche |
| `Iccid` | SIM card number | `Text { pattern: "[0-9]{19,20}" }` | ITU-T E.118 | — | candidate | niche | out: niche |
| `Msisdn` | Mobile number | `PhoneNumber` | ITU-T E.164 | — | candidate | silly | out: silly |
| `MobileNetworkCode` | Mobile network code | `Object { mcc: Text, mnc: Text }` | ITU-T E.212 | 2 vs 3 digit MNC leading zero | candidate | niche | out: niche |
| `SipUri` | A SIP address | `Uri` | RFC 3261 | — | candidate | niche | out: niche |
| `IpGeolocation` | Location guessed from IP | `Object { country: Country, region: Text?, accuracy: Length }` | — | — | candidate | silly | out: silly |

## Software and development

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `SemanticVersion` | A semver version | `Object { major, minor, patch: Int, pre: Text?, build: Text? }` | SemVer 2.0.0 | "1.10" < "1.9" as text | candidate | core | standard |
| `VersionRange` | A version constraint | `Text` validated | npm semver ranges, Cargo | Range syntaxes differ | candidate | likely | standard |
| `CalendarVersion` | A calver version | `Text` | calver.org | — | candidate | niche | out: niche |
| `GitSha` | A git commit hash | `Text { pattern: "[0-9a-f]{40}\|[0-9a-f]{64}" }` | Git | Short vs full, SHA-256 repos | candidate | likely | folded → `GitRef` |
| `GitRef` | A branch or tag | `Text` | git-check-ref-format | refs/heads/ prefix | candidate | maybe | standard |
| `GitRemote` | A repository location | `Text` | — | SSH vs HTTPS forms | candidate | niche | out: niche |
| `PackageName` | A package in an ecosystem | `Object { ecosystem: Enum, name: Text }` | Package URL | Scoped names | candidate | maybe | folded → `PackageUrl` |
| `Purl` | A package URL | `Text { pattern: "pkg:.*" }` | Package URL (ECMA-427) | — | candidate | maybe | standard, as `PackageUrl` |
| `Cpe` | Platform enumeration | `Text { pattern: "cpe:2\.3:.*" }` | NIST CPE | — | candidate | niche | out: niche |
| `Cve` | A vulnerability id | `Text { pattern: "CVE-[0-9]{4}-[0-9]{4,}" }` | MITRE CVE | — | candidate | maybe | standard |
| `Cwe` | A weakness id | `Text { pattern: "CWE-[0-9]+" }` | MITRE CWE | — | candidate | niche | out: niche |
| `CvssScore` | Vulnerability severity | `Object { score: Decimal { min: 0, max: 10 }, vector: Text }` | FIRST CVSS 4.0 | v3 vs v4 | candidate | niche | out: niche |
| `SpdxLicence` | A licence expression | `Text` | SPDX | "MIT" vs "MIT License" | candidate | maybe | standard |
| `ProgrammingLanguage` | A language name | `Enum` | Linguist | — | candidate | niche | out: niche |
| `SourceLocation` | A file, line and column | `Object { file: Text, line: Int { min: 1 }, column: Int { min: 1 } }` | — | 0- vs 1-based | candidate | likely | out: app entity |
| `SourceRange` | A span in a file | `Object { start: SourceLocation, end: SourceLocation }` | LSP | UTF-16 vs byte columns | candidate | maybe | out: app entity |
| `StackTrace` | A list of frames | `List<Object { function, file, line }>` | — | — | candidate | niche | out: niche |
| `LogLevel` | trace, debug, info, warn, error | `Enum` | RFC 5424 (syslog), OpenTelemetry | Numbers vs names | candidate | likely | standard |
| `LogRecord` | A structured log line | `Object { at: Instant, level: LogLevel, message: Text, attributes: Map }` | OpenTelemetry Logs | — | candidate | maybe | out: app entity |
| `TraceId` | A distributed trace id | `Text { pattern: "[0-9a-f]{32}" }` | W3C Trace Context | — | candidate | likely | standard |
| `SpanId` | A span id | `Text { pattern: "[0-9a-f]{16}" }` | W3C Trace Context | — | candidate | likely | standard |
| `TraceParent` | Trace context header | `Object { trace: TraceId, span: SpanId, flags: Int }` | W3C Trace Context | — | candidate | maybe | standard |
| `MetricName` | A metric identifier | `Text` | OpenMetrics, OTel semantic conventions | dots vs underscores | candidate | niche | out: niche |
| `EnvironmentName` | dev, staging, production | `Enum` | — | "prod" vs "production" | candidate | maybe | out: no standard |
| `FeatureFlagKey` | A feature flag name | `Text` | OpenFeature | — | candidate | niche | out: niche |
| `ExitCode` | A process exit status | `Int { min: 0, max: 255 }` | POSIX | — | candidate | niche | out: niche |
| `Signal` | A POSIX signal | `Enum` | POSIX | — | candidate | silly | out: silly |
| `FilePath` | A path on a file system | `Text` | POSIX, Windows | Separators, case | candidate | maybe | standard |
| `Glob` | A file pattern | `Text` | POSIX glob | Dialects | candidate | niche | out: niche |
| `RegularExpression` | A regex | `Text` validated | ECMA-262, RE2 | Dialects | candidate | maybe | standard |
| `ContainerImage` | A container image reference | `Object { registry, repository: Text, tag: Text?, digest: Digest? }` | OCI Distribution | Default registry and tag | candidate | maybe | standard |
| `Digest` | A content-addressed hash | `Text { pattern: "sha256:[0-9a-f]{64}" }` | OCI | — | candidate | maybe | standard |
| `CloudRegion` | eu-west-2 etc. | `Object { provider: Enum, region: Text }` | — | — | candidate | niche | out: niche |
| `Arn` | An AWS resource name | `Text { pattern: "arn:.*" }` | AWS | — | candidate | niche | out: niche |
| `EnvironmentVariableName` | An env var name | `Text { pattern: "[A-Z_][A-Z0-9_]*" }` | POSIX | — | candidate | silly | out: silly |
| `Ulid` | A sortable unique id | `Text { pattern: "[0-9A-HJKMNP-TV-Z]{26}" }` | ULID spec | vs UUIDv7 | candidate | maybe | standard |
| `UuidV7` | A time-ordered UUID | `Uuid` refinement | RFC 9562 | — | candidate | maybe | folded → `Uuid` |
| `NanoId` | A short random id | `Text` | — | — | candidate | niche | out: niche |
| `SnowflakeId` | A Twitter-style id | `Int` | — | JSON number precision | candidate | niche | out: niche |
| `IdempotencyKey` | A request deduplication key | `Text { max_length: 255 }` | IETF draft Idempotency-Key | — | candidate | maybe | standard |
| `CorrelationId` | A request correlation id | `Text` | — | — | candidate | maybe | out: no standard |
| `ApiVersion` | An API version | `Text` | — | date vs v1 | candidate | niche | out: niche |
| `GraphQlId` | A global node id | `Text` | Relay spec | — | candidate | silly | out: silly |
| `KeyboardShortcut` | A key combination | `Object { key: Text, modifiers: Set<Enum> }` | W3C UI Events KeyboardEvent.key | Cmd vs Ctrl | candidate | niche | out: niche |
| `KeyCode` | A physical key | `Text` | W3C UI Events code | — | candidate | silly | out: silly |
| `OperatingSystem` | An OS and version | `Object { family: Enum, version: SemanticVersion }` | — | — | candidate | niche | out: niche |
| `BrowserName` | Chrome, Firefox, Safari | `Enum` | — | — | candidate | silly | out: silly |
| `DeviceType` | Desktop, tablet, phone | `Enum` | — | — | candidate | niche | out: niche |
| `ScreenSize` | Display dimensions | `ImageDimensions` | — | — | candidate | niche | out: niche |
| `CssSelector` | A CSS selector | `Text` | Selectors Level 4 | — | candidate | silly | out: silly |
| `Markdown` | Markdown text | `Text` | CommonMark, GFM | Flavours differ | candidate | likely | standard |
| `Html` | An HTML fragment | `Text`, sanitised | WHATWG HTML | XSS when unsanitised | candidate | likely | standard |
| `RichText` | Structured formatted text | `List<Block>` | Portable Text, ProseMirror | Every editor's own JSON | candidate | likely | standard |
| `Diff` | A text diff | `Text` unified diff | POSIX diff -u | — | candidate | niche | out: niche |

## Security and cryptography

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `PasswordHash` | A stored password hash with parameters | `Text` PHC string format | PHC string format, RFC 9106 (Argon2) | Algorithm and salt stored separately | candidate | core | standard |
| `PlainPassword` | A password in transit, never stored | `Text { min_length: 8 }`, redacted in logs | NIST SP 800-63B | Logged by accident | candidate | likely | standard, as `Password` |
| `PasswordPolicy` | Password rules | `Object { min_length: Int, max_length: Int, check_breached: Bool }` | NIST SP 800-63B | Composition rules contradict NIST | candidate | maybe | standard |
| `Hash` | A digest with algorithm | `Object { algorithm: HashAlgorithm, value: Bytes }` | FIPS 180-4 | Hex vs base64, algorithm unstated | candidate | likely | standard |
| `HashAlgorithm` | sha256, sha512, blake3… | `Enum` | FIPS 180-4, RFC 7693 | — | candidate | maybe | out: by design |
| `Sha256` | A SHA-256 digest | `Bytes { min_length: 32, max_length: 32 }` | FIPS 180-4 | Case of hex | candidate | likely | folded → `Hash` |
| `Md5` | An MD5 digest (legacy) | `Bytes` of 16 | RFC 1321 | — | candidate | niche | out: niche |
| `Hmac` | A keyed hash | `Object { algorithm: Enum, value: Bytes }` | RFC 2104 | — | candidate | maybe | standard |
| `Jwt` | A JSON web token | `Text { pattern: "[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]*" }` | RFC 7519 | alg=none; unverified claims trusted | candidate | likely | standard |
| `JwtClaims` | Standard token claims | `Object { iss, sub, aud, exp, nbf, iat, jti }` | RFC 7519 | exp in seconds vs ms | candidate | maybe | standard |
| `Jwk` | A JSON web key | `Object` per RFC 7517 | RFC 7517 | — | candidate | maybe | standard |
| `Jwks` | A key set | `List<Jwk>` | RFC 7517 | — | candidate | niche | out: niche |
| `PublicKey` | An asymmetric public key | `Object { algorithm: KeyAlgorithm, der: Bytes }` | RFC 5280 SPKI | PEM vs DER vs JWK | candidate | maybe | standard |
| `PrivateKey` | A private key, secret | `Object { algorithm: KeyAlgorithm, pkcs8: Bytes }` | RFC 5208 | Logged by accident | candidate | maybe | standard |
| `KeyAlgorithm` | Ed25519, P-256, RSA-2048 | `Enum` | IANA JOSE, RFC 8037 | — | candidate | maybe | out: by design |
| `Certificate` | An X.509 certificate | `Bytes` DER | RFC 5280 | PEM vs DER | candidate | maybe | standard |
| `CertificateFingerprint` | A cert's SHA-256 fingerprint | `Sha256` | — | Colons vs none | candidate | niche | out: niche |
| `Pem` | PEM-armoured data | Conversion edge | RFC 7468 | — | candidate | niche | out: niche |
| `SshPublicKey` | An authorized_keys line | `Text` | RFC 4253 | — | candidate | niche | out: niche |
| `PgpFingerprint` | An OpenPGP key fingerprint | `Text { pattern: "[0-9A-F]{40}" }` | RFC 9580 | Spaces | candidate | niche | out: niche |
| `TotpSecret` | A one-time password seed | `Bytes`, base32 on the wire | RFC 6238, RFC 4226 | Base32 padding; stored in clear | candidate | likely | standard |
| `OtpAuthUri` | An authenticator QR payload | `Uri { pattern: "otpauth://.*" }` | Google Key Uri Format | — | candidate | maybe | out: format |
| `OneTimeCode` | A six-digit code | `Text { pattern: "[0-9]{6,8}" }` | RFC 6238 | Leading zeros lost as Int | candidate | likely | standard |
| `RecoveryCode` | An MFA backup code | `Text` | — | — | candidate | maybe | out: no standard |
| `ApiKey` | A secret API key | `Text`, redacted | — | Logged; prefix conventions | candidate | likely | standard |
| `Secret` | Any redacted secret value | `Text`, never printed | — | Leaked to logs | candidate | core | standard |
| `BearerToken` | An access token | `Secret` | RFC 6750 | — | candidate | maybe | standard |
| `OAuthScope` | An OAuth scope | `Text` | RFC 6749 | Space vs comma separated | candidate | maybe | standard |
| `OAuthClientId` | A client identifier | `Text` | RFC 6749 | — | candidate | niche | out: niche |
| `OidcClaims` | Identity claims | `Object { sub, email, email_verified, name… }` | OpenID Connect Core | — | candidate | maybe | standard |
| `WebAuthnCredential` | A passkey credential | `Object { id: Bytes, public_key: PublicKey, sign_count: Int }` | W3C WebAuthn | — | candidate | maybe | standard |
| `CredentialId` | A WebAuthn credential id | `Bytes` | W3C WebAuthn | base64url vs base64 | candidate | niche | out: niche |
| `Nonce` | A number used once | `Bytes` | — | — | candidate | niche | out: niche |
| `Salt` | A hashing salt | `Bytes` | — | — | candidate | niche | out: niche |
| `EncryptedValue` | Ciphertext with its key id and algorithm | `Object { key_id: Text, algorithm: Enum, iv: Bytes, ciphertext: Bytes }` | RFC 7516 (JWE) | IV or tag missing | candidate | likely | standard |
| `KeyId` | A key identifier | `Text` | RFC 7517 kid | — | candidate | niche | out: niche |
| `SessionId` | A session token | `Secret` | OWASP | — | candidate | maybe | out: runtime-owned |
| `CsrfToken` | An anti-forgery token | `Secret` | OWASP | — | candidate | niche | out: niche |
| `Permission` | A named permission | `Text` | — | resource:action conventions | candidate | maybe | out: no standard |
| `Role` | A named role | `Text` | — | — | candidate | maybe | out: no standard |
| `AuditEvent` | Who did what when | `Object { actor: Text, action: Text, target: Text, at: Instant }` | — | — | candidate | maybe | out: no standard |
| `RiskScore` | A fraud or risk score | `Proportion` | — | — | candidate | niche | out: niche |
| `CaptchaToken` | A bot-check response | `Text` | — | — | candidate | silly | out: silly |
| `SecurityLevel` | AAL1–3 | `Enum` | NIST SP 800-63 | — | candidate | niche | out: niche |
| `IpAllowlist` | Allowed networks | `Set<Cidr>` | — | — | candidate | niche | out: niche |
| `DataClassification` | Public, internal, confidential | `Enum` | ISO 27001 | — | candidate | niche | out: niche |

## Documents and file formats

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `FileName` | A file's name | `Text { max_length: 255 }`, no separators | POSIX, Windows rules | Path traversal; reserved names | candidate | likely | standard |
| `FileExtension` | .pdf, .png | `Text { pattern: "[a-z0-9]{1,10}" }` | — | With or without dot | candidate | maybe | out: no standard |
| `FileSize` | Bytes in a file | `DataSize` | — | — | candidate | maybe | out: no standard |
| `StoredFile` | An uploaded file reference | `Object { key: Text, name: FileName, type: FileType, size: DataSize, hash: Sha256 }` | — | Every storage library's own | candidate | likely | out: app entity |
| `Attachment` | A file attached to a record | `StoredFile` plus `description` | — | — | candidate | maybe | out: no standard |
| `DataUri` | Inline data URL | `Text { pattern: "data:.*" }` | RFC 2397 | — | candidate | niche | out: niche |
| `CsvDialect` | Delimiter, quote, header | `Object { delimiter: Text, quote: Text, header: Bool }` | RFC 4180 | Excel vs RFC CSV | candidate | maybe | out: format |
| `Json` | An arbitrary JSON value | `Enum` of JSON kinds | RFC 8259 | — | candidate | likely | out: format |
| `Yaml` | YAML text | `Text` | YAML 1.2 | Norway problem | candidate | niche | out: niche |
| `Xml` | XML text | `Text` | W3C XML 1.0 | — | candidate | niche | out: niche |
| `PdfDocument` | A PDF file | `StoredFile` of `application/pdf` | ISO 32000 | — | candidate | niche | out: niche |
| `PageRange` | Pages 1–3, 5 | `List<Range<Int>>` | — | — | candidate | niche | out: niche |
| `PageNumber` | A page | `Int { min: 1 }` | — | — | candidate | silly | out: silly |
| `DocumentType` | Invoice, receipt, passport… | `Enum` | — | — | candidate | niche | out: niche |
| `DocumentVersion` | A version of a document | `Object { number: Int, at: Instant, by: UserId }` | — | — | candidate | niche | out: niche |
| `WordCount` | Words in a text | `Int { min: 0 }` | — | — | candidate | silly | out: silly |
| `ReadingTime` | Estimated reading time | `Duration` | — | — | candidate | silly | out: silly |
| `MultipartBody` | A multipart body | Conversion edge | RFC 2046 | — | candidate | silly | out: silly |
| `SignatureEnvelope` | A digitally signed document | `Object { document: Bytes, signatures: List<Bytes> }` | ETSI PAdES, XAdES | — | candidate | niche | out: niche |
| `EbookFormat` | EPUB, MOBI | `FileType` | W3C EPUB 3 | — | candidate | silly | out: silly |
| `SpreadsheetCell` | A cell reference (A1) | `Text { pattern: "[A-Z]{1,3}[0-9]+" }` | — | A1 vs R1C1 | candidate | niche | out: niche |
| `SpreadsheetRange` | A1:C10 | `Object { start: SpreadsheetCell, end: SpreadsheetCell }` | — | — | candidate | silly | out: silly |

## Language, text and writing

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Language` | A BCP 47 language tag | `Text`, `pt-BR` | BCP 47 (RFC 5646) | en_GB vs en-GB vs eng | proposed | core | standard |
| `Locale` | A supported app locale | As area 10 | BCP 47, CLDR | — | exists | core | standard |
| `LanguageAlpha3` | A three-letter language code | `Text { pattern: "[a-z]{3}" }` | ISO 639-3 | 639-2/B vs /T (fre vs fra) | candidate | maybe | folded → `Language` |
| `Script` | A writing system | `Text { pattern: "[A-Z][a-z]{3}" }` | ISO 15924 | — | candidate | maybe | standard |
| `TextDirection` | LTR or RTL | `Enum { ltr, rtl, auto }` | Unicode Bidi, HTML dir | — | candidate | maybe | standard |
| `LocalisedText` | Text in several languages | `Map<Language, Text>` | — | Fallback rules differ | candidate | core | standard |
| `TranslationKey` | A message id | `Text` | ICU MessageFormat | — | candidate | maybe | out: app entity |
| `PluralCategory` | zero, one, two, few, many, other | `Enum` | CLDR plural rules | English-only plurals | candidate | maybe | out: app entity |
| `MessageFormat` | An ICU message pattern | `Text` validated | ICU MessageFormat 2 | — | candidate | maybe | out: format |
| `GraphemeCount` | User-perceived characters | `Int` | Unicode UAX #29 | Byte vs code point length | candidate | niche | out: niche |
| `CodePoint` | A Unicode code point | `Int { min: 0, max: 1114111 }` | Unicode | — | candidate | niche | out: niche |
| `UnicodeNormalisation` | NFC, NFD, NFKC, NFKD | `Enum` | Unicode UAX #15 | Duplicate usernames | candidate | niche | out: niche |
| `Transliteration` | Romanised text | `Object { scheme: Text, text: Text }` | ISO 9, BGN/PCGN | — | candidate | silly | out: silly |
| `Hashtag` | A #tag | `Text { pattern: "#[\p{L}\p{N}_]+" }` | — | # included or not | candidate | niche | out: niche |
| `Mention` | An @mention of a user | `Object { user: UserId, text: Text }` | — | — | candidate | niche | out: niche |
| `Tag` | A free-form label | `Text { max_length: 50 }`, lowercase | — | Case duplicates | candidate | maybe | out: no standard |
| `Keyword` | A search keyword | `Text` | — | — | candidate | niche | out: niche |
| `SearchQuery` | A user search string | `Text { max_length: 500 }` | — | — | candidate | niche | out: niche |
| `Snippet` | A highlighted excerpt | `Object { text: Text, highlights: List<Range<Int>> }` | — | Byte vs character offsets | candidate | niche | out: niche |
| `TextSpan` | A range within a text | `Object { start: Int, end: Int }` in characters | — | UTF-8 vs UTF-16 offsets | candidate | maybe | out: no standard |
| `Sentiment` | Positive, neutral, negative | `Object { label: Enum, score: Decimal }` | — | — | candidate | niche | out: niche |
| `ReadingLevel` | Text readability grade | `Decimal` | Flesch–Kincaid | — | candidate | silly | out: silly |
| `ProfanityFlag` | Whether text is offensive | `Bool` | — | — | candidate | silly | out: silly |
| `Abbreviation` | A short form and expansion | `Object { short: Text, long: Text }` | — | — | candidate | silly | out: silly |
| `NumberingSystem` | latn, arab, deva digits | `Text` | CLDR | — | candidate | niche | out: niche |

## Aviation

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `IataAirport` | Three-letter airport code | `Text { pattern: "[A-Z]{3}" }` | IATA Resolution 763 | IATA vs ICAO mixed | candidate | likely | standard |
| `IcaoAirport` | Four-letter airport code | `Text { pattern: "[A-Z]{4}" }` | ICAO Doc 7910 | — | candidate | maybe | standard |
| `IataAirline` | Two-character airline designator | `Text { pattern: "[A-Z0-9]{2}" }` | IATA | — | candidate | maybe | standard |
| `IcaoAirline` | Three-letter airline designator | `Text { pattern: "[A-Z]{3}" }` | ICAO Doc 8585 | — | candidate | niche | out: niche |
| `FlightNumber` | Airline and number | `Object { airline: IataAirline, number: Int { min: 1, max: 9999 }, suffix: Text? }` | IATA SSIM | BA0123 vs BA123 | candidate | likely | standard |
| `FlightDesignator` | A flight on a date | `Object { flight: FlightNumber, date: CalendarDate }` | IATA | Local vs UTC date | candidate | maybe | folded → `FlightNumber` |
| `AircraftType` | An aircraft model code | `Text` | ICAO Doc 8643, IATA | ICAO vs IATA codes | candidate | niche | out: niche |
| `AircraftRegistration` | A tail number | `Text` | ICAO Annex 7 | Hyphen | candidate | niche | out: niche |
| `Icao24` | Transponder address | `Text { pattern: "[0-9a-f]{6}" }` | ICAO Annex 10 | — | candidate | niche | out: niche |
| `Squawk` | Transponder code | `Text { pattern: "[0-7]{4}" }` | ICAO | — | candidate | silly | out: silly |
| `FlightLevel` | Altitude in hundreds of feet | `Int` | ICAO | ft vs m | candidate | niche | out: niche |
| `Callsign` | A radio callsign | `Text` | ICAO | — | candidate | niche | out: niche |
| `SeatNumber` | A seat on a vehicle | `Object { row: Int, letter: Text }` | — | — | candidate | maybe | out: no standard |
| `CabinClass` | Economy, premium, business, first | `Enum` | IATA | — | candidate | maybe | standard |
| `FareBasis` | A fare code | `Text` | IATA | — | candidate | silly | out: silly |
| `BookingReference` | A PNR record locator | `Text { pattern: "[A-Z0-9]{6}" }` | IATA | — | candidate | maybe | standard |
| `TicketNumber` | A 13-digit e-ticket number | `Text { pattern: "[0-9]{13}" }` | IATA | — | candidate | niche | out: niche |
| `BoardingPass` | A BCBP barcode payload | `Text` | IATA Resolution 792 | — | candidate | niche | out: niche |
| `BaggageTag` | A licence plate number | `Text { pattern: "[0-9]{10}" }` | IATA Resolution 740 | — | candidate | niche | out: niche |
| `Metar` | An aviation weather report | `Text` | WMO FM 15 | — | candidate | silly | out: silly |
| `Notam` | A notice to airmen | `Text` | ICAO | — | candidate | silly | out: silly |

## Road, rail and sea transport

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Vin` | Vehicle identification number | `Text { pattern: "[A-HJ-NPR-Z0-9]{17}" }` | ISO 3779 | I, O, Q excluded; check digit (NA) | candidate | likely | standard |
| `VehicleRegistration` | A number plate with country | `Object { country: Country, plate: Text }` | — | Spaces; same plate in two countries | candidate | likely | standard |
| `UkNumberPlate` | A UK registration mark | `Text`, normalised without spaces | DVLA | — | candidate | maybe | folded → `VehicleRegistration` |
| `VehicleMake` | A manufacturer | `Text` | — | "VW" vs "Volkswagen" | candidate | niche | out: niche |
| `VehicleModel` | A model and variant | `Object { make: VehicleMake, model: Text, year: Int }` | — | — | candidate | niche | out: niche |
| `FuelType` | Petrol, diesel, electric, hybrid | `Enum` | — | — | candidate | maybe | out: no standard |
| `EmissionsStandard` | Euro 6 etc. | `Enum` | EU Regulation 715/2007 | — | candidate | niche | out: niche |
| `Odometer` | Distance on a vehicle | `Length` | — | Miles vs km | candidate | maybe | out: no standard |
| `DrivingLicenceNumber` | A licence number with country | `Object { country: Country, number: Text }` | ISO/IEC 18013 | — | candidate | maybe | standard |
| `DrivingLicenceCategory` | B, C1, D | `Enum` | EU Directive 2006/126 | — | candidate | niche | out: niche |
| `ChargingConnector` | Type 2, CCS, CHAdeMO | `Enum` | IEC 62196 | — | candidate | niche | out: niche |
| `EvseId` | A charge point id | `Text` | ISO 15118, eMI3 | — | candidate | niche | out: niche |
| `TyreSize` | 205/55 R16 91V | `Object { width: Int, aspect: Int, rim: Int, load: Int, speed: Text }` | ETRTO, ISO 4000 | — | candidate | niche | out: niche |
| `TrafficSignCode` | A road sign id | `Text` | Vienna Convention | — | candidate | silly | out: silly |
| `RoadClass` | Motorway, A road… | `Enum` | — | — | candidate | silly | out: silly |
| `CrsCode` | UK rail station code | `Text { pattern: "[A-Z]{3}" }` | National Rail | — | candidate | niche | out: niche |
| `Tiploc` | UK rail timing point | `Text` | Network Rail | — | candidate | silly | out: silly |
| `UicStationCode` | European rail station | `Int` | UIC | — | candidate | silly | out: silly |
| `GtfsStopId` | A transit stop | `Text` | GTFS | — | candidate | niche | out: niche |
| `NaptanCode` | UK public transport stop | `Text` | NaPTAN | — | candidate | niche | out: niche |
| `TransitRoute` | A bus or rail route | `Object { agency: Text, short_name: Text }` | GTFS | — | candidate | niche | out: niche |
| `Departure` | A scheduled and expected departure | `Object { scheduled: Instant, expected: Instant?, platform: Text? }` | GTFS-RT, SIRI | — | candidate | niche | out: niche |
| `ImoNumber` | Ship identifier | `Text { pattern: "IMO[0-9]{7}" }` | IMO | — | candidate | niche | out: niche |
| `Mmsi` | Maritime mobile service id | `Text { pattern: "[0-9]{9}" }` | ITU-R M.585 | — | candidate | niche | out: niche |
| `ShipCallsign` | A vessel callsign | `Text` | ITU | — | candidate | silly | out: silly |
| `SeaPort` | A seaport | `UnLocode` | UN/LOCODE | — | candidate | silly | out: silly |

## Logistics and shipping

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `ContainerNumber` | An intermodal container id | `Text { pattern: "[A-Z]{3}[UJZ][0-9]{7}" }` with check digit | ISO 6346 | Check digit | candidate | maybe | standard |
| `TrackingNumber` | A parcel tracking id with carrier | `Object { carrier: Carrier, number: Text }` | — | Number alone ambiguous across carriers | candidate | likely | standard |
| `Carrier` | A shipping carrier | `Text` | SCAC (US) | — | candidate | maybe | standard |
| `Scac` | Standard carrier alpha code | `Text { pattern: "[A-Z]{2,4}" }` | NMFTA | — | candidate | niche | out: niche |
| `ShipmentStatus` | Shipment lifecycle | `Enum` | — | Each carrier's own | candidate | maybe | out: no standard |
| `AirWaybill` | An air cargo waybill number | `Text { pattern: "[0-9]{3}-[0-9]{8}" }` | IATA | — | candidate | niche | out: niche |
| `BillOfLading` | A sea shipping document number | `Text` | — | — | candidate | niche | out: niche |
| `ParcelWeight` | Weight of a parcel | `Mass` | — | — | candidate | maybe | out: no standard |
| `VolumetricWeight` | Dimensional weight | `Object { mass: Mass, divisor: Int }` | — | Divisor varies by carrier | candidate | niche | out: niche |
| `DeliveryWindow` | Promised delivery slot | `Interval` | — | — | candidate | maybe | out: no standard |
| `PalletType` | EUR, GMA… | `Enum` | ISO 6780 | — | candidate | silly | out: silly |
| `WarehouseLocation` | Aisle, bay, shelf, bin | `Object { aisle, bay, level, bin: Text }` | — | — | candidate | niche | out: niche |
| `LotNumber` | A manufacturing batch | `Text` | GS1 AI (10) | — | candidate | maybe | standard |
| `SerialNumber` | An item serial | `Text` | GS1 AI (21) | — | candidate | maybe | standard |
| `ExpiryDate` | Best before or use by | `Object { date: CalendarDate, kind: Enum { use_by, best_before } }` | GS1 AI (17) | Day 00 means end of month | candidate | maybe | folded → `CalendarDate` |
| `Gs1ElementString` | Encoded GS1 data | `Text` | GS1 General Specs | — | candidate | niche | out: niche |
| `HazmatClass` | Dangerous goods class | `Text` | UN Model Regulations | — | candidate | niche | out: niche |
| `CustomsDeclaration` | Goods description and value | `Object { hs_code: HsCode, value: Money, origin: Country }` | WCO | — | candidate | niche | out: niche |
| `CountryOfOrigin` | Where goods were made | `Country` | WCO rules of origin | — | candidate | maybe | folded → `Country` |

## Education

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Grade` | A mark with its scheme | `Object { scheme: GradingScheme, value: Text }` | — | A* vs 9 vs 4.0 GPA | candidate | maybe | out: no standard |
| `GradingScheme` | GCSE 9–1, A-level, GPA… | `Enum` | — | — | candidate | niche | out: niche |
| `Gpa` | Grade point average | `Object { value: Decimal, scale: Decimal }` | — | 4.0 vs 5.0 scale | candidate | niche | out: niche |
| `SchoolYear` | Year group with system | `Object { system: Enum, year: Int }` | DfE | UK Year 7 vs US Grade 6 | candidate | niche | out: niche |
| `AcademicYear` | 2025/26 | `Object { start_year: Int }` | — | Label by start or end | candidate | maybe | out: no standard |
| `Term` | Autumn, spring, summer | `Enum` | — | — | candidate | niche | out: niche |
| `Urn` | UK school reference number | `Int` | DfE GIAS | — | candidate | niche | out: niche |
| `Ukprn` | UK provider number | `Text { pattern: "1[0-9]{7}" }` | UKRLP | — | candidate | niche | out: niche |
| `Upn` | Unique pupil number | `Text { pattern: "[A-Z][0-9]{11}[0-9A-Z]" }` | DfE | — | candidate | niche | out: niche |
| `UcasCode` | A course code | `Text` | UCAS | — | candidate | silly | out: silly |
| `CefrLevel` | Language proficiency A1–C2 | `Enum` | Council of Europe CEFR | — | candidate | maybe | standard |
| `QualificationLevel` | RQF/EQF level | `Int { min: 1, max: 8 }` | EQF, RQF | — | candidate | niche | out: niche |
| `Credits` | Course credit points | `Object { value: Decimal, scheme: Enum { ects, cats, us } }` | ECTS | — | candidate | niche | out: niche |
| `CourseCode` | A module code | `Text` | — | — | candidate | silly | out: silly |
| `LearningOutcome` | — | `Text` | — | — | candidate | silly | out: silly |
| `LtiLaunch` | A learning tool launch | `Object` | IMS LTI 1.3 | — | candidate | silly | out: silly |
| `XapiStatement` | A learning record | `Object { actor, verb, object }` | xAPI | — | candidate | niche | out: niche |

## Legal and government

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `PassportNumber` | A passport number with issuer | `Object { country: Country, number: Text }` | ICAO Doc 9303 | Number alone ambiguous | candidate | maybe | standard |
| `Mrz` | Machine readable zone | `Text` | ICAO Doc 9303 | — | candidate | niche | out: niche |
| `NationalId` | A national identity number | `Object { country: Country, scheme: Text, value: Text }` | — | Format per country | candidate | maybe | out: no standard |
| `IdentityDocument` | A document type, number and expiry | `Object { type: Enum, country: Country, number: Text, expires: CalendarDate }` | ICAO 9303 | — | candidate | maybe | standard |
| `VisaType` | An immigration category | `Text` | — | — | candidate | silly | out: silly |
| `ShareCode` | UK right-to-work code | `Text { pattern: "[A-Z0-9]{9}" }` | UK Home Office | — | candidate | niche | out: niche |
| `DbsCertificateNumber` | UK criminal record check | `Text { pattern: "[0-9]{12}" }` | DBS | — | candidate | niche | out: niche |
| `LegalEntityType` | Ltd, LLP, plc, sole trader | `Enum` | ISO 20275 (ELF) | — | candidate | maybe | out: app entity |
| `LegalEntity` | A company or organisation | `Object { name: Text, type: LegalEntityType, number: Text, country: Country }` | — | — | candidate | maybe | out: no standard |
| `CharityNumber` | A registered charity number | `Object { regulator: Enum, number: Text }` | Charity Commission, OSCR | — | candidate | niche | out: niche |
| `Jurisdiction` | A legal jurisdiction | `Subdivision` or `Country` | ISO 3166 | England vs UK | candidate | maybe | standard |
| `Consent` | A recorded consent | `Object { purpose: Text, given: Bool, at: Instant, version: Text }` | GDPR Art. 7 | Purpose and version missing | candidate | likely | out: app entity |
| `LawfulBasis` | GDPR lawful basis | `Enum { consent, contract, legal_obligation, vital_interests, public_task, legitimate_interests }` | GDPR Art. 6 | — | candidate | maybe | standard |
| `RetentionPeriod` | How long data is kept | `Period` | — | — | candidate | maybe | out: no standard |
| `DataSubjectRequest` | Access, erasure, rectification | `Enum` | GDPR Arts. 15–22 | — | candidate | niche | out: niche |
| `TermsVersion` | An accepted terms version | `Object { version: Text, accepted_at: Instant }` | — | — | candidate | maybe | out: no standard |
| `CaseNumber` | A court case reference | `Text` | — | — | candidate | niche | out: niche |
| `StatuteCitation` | A legal citation | `Text` | — | — | candidate | silly | out: silly |
| `Ecli` | European case law identifier | `Text { pattern: "ECLI:[A-Z]{2}:.*" }` | EU ECLI | — | candidate | silly | out: silly |
| `ElectoralWard` | A voting area | `OnsCode` | ONS | — | candidate | silly | out: silly |
| `Constituency` | A parliamentary constituency | `OnsCode` | ONS | — | candidate | niche | out: niche |
| `LandRegistryTitle` | A UK land title number | `Text` | HM Land Registry | — | candidate | niche | out: niche |
| `PlanningReference` | A planning application id | `Text` | — | — | candidate | silly | out: silly |
| `FoiReference` | — | `Text` | — | — | candidate | silly | out: silly |

## Energy and utilities

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Mpan` | UK electricity meter point | `Text { pattern: "[0-9]{13}" }` (core) plus supplementary data | Elexon MRA | Core vs full 21-digit | candidate | maybe | standard |
| `Mprn` | UK gas meter point | `Text { pattern: "[0-9]{6,10}" }` | Xoserve | — | candidate | maybe | standard |
| `MeterSerialNumber` | A meter's serial | `Text` | — | — | candidate | niche | out: niche |
| `MeterReading` | A meter read | `Object { value: Decimal, at: Instant, kind: Enum { actual, estimated, customer } }` | — | Estimated vs actual | candidate | maybe | out: no standard |
| `Tariff` | An energy tariff | `Object { unit_rate: Money per kWh, standing_charge: Money per day }` | — | Pence vs pounds per kWh | candidate | maybe | out: no standard |
| `TariffRate` | Price per kWh | `Object { price: Money, per: Energy }` | — | — | candidate | maybe | out: no standard |
| `HalfHourlyInterval` | A settlement period | `Object { date: CalendarDate, period: Int { min: 1, max: 50 } }` | Elexon BSC | 46/50 on clock-change days | candidate | niche | out: niche |
| `CarbonIntensity` | gCO₂ per kWh | `Decimal` | — | — | candidate | niche | out: niche |
| `EpcRating` | Energy performance band | `Object { band: Enum { a…g }, score: Int }` | UK EPC | — | candidate | maybe | standard |
| `SolarPanelOutput` | Peak power | `Power` | — | kWp vs kW | candidate | niche | out: niche |
| `WaterMeterReading` | Cubic metres used | `Volume` | — | — | candidate | silly | out: silly |
| `GridFrequency` | Mains frequency | `Frequency` | — | — | candidate | silly | out: silly |
| `PlugType` | Mains plug type A–N | `Enum` | IEC World Plugs | — | candidate | niche | out: niche |
| `MainsVoltage` | 230 V, 120 V | `Voltage` | IEC 60038 | — | candidate | silly | out: silly |
| `Eic` | Energy identification code | `Text { pattern: "[0-9]{2}[A-Z][A-Z0-9-]{12}[A-Z0-9]" }` | ENTSO-E | — | candidate | silly | out: silly |

## IoT, sensors and hardware

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `SensorReading` | A measured value with unit and time | `Object { quantity: Quantity, at: Instant, sensor: Text }` | OGC SensorThings, SenML | Unit and time missing | candidate | likely | out: app entity |
| `SenmlRecord` | A sensor measurement | `Object` per RFC 8428 | RFC 8428 | — | candidate | niche | out: niche |
| `DeviceId` | A hardware device identifier | `Text` | — | — | candidate | maybe | out: no standard |
| `BleUuid` | A Bluetooth service UUID | `Uuid` or 16-bit short form | Bluetooth SIG | 16-bit vs 128-bit forms | candidate | niche | out: niche |
| `AccelerometerReading` | x, y, z acceleration | `Vector3` in m/s² | W3C Generic Sensor | g vs m/s² | candidate | niche | out: niche |
| `GyroscopeReading` | Angular velocity | `Vector3` in rad/s | W3C Generic Sensor | deg/s vs rad/s | candidate | niche | out: niche |
| `AirQualityIndex` | An AQI with scheme | `Object { scheme: Enum { us_epa, uk_daqi, eu_caqi }, value: Int }` | US EPA, DEFRA DAQI | Different scales | candidate | niche | out: niche |
| `Pm25` | Fine particulate matter | `Object { micrograms_per_m3: Decimal }` | WHO | — | candidate | niche | out: niche |
| `Co2Concentration` | Indoor CO₂ ppm | `PartsPerMillion` | — | — | candidate | niche | out: niche |
| `SignalQuality` | Link quality | `Percentage` | — | — | candidate | silly | out: silly |
| `FirmwareVersion` | A device firmware version | `SemanticVersion` or `Text` | — | — | candidate | niche | out: niche |
| `ModbusRegister` | A Modbus address | `Int { min: 0, max: 65535 }` | Modbus | 0- vs 1-based | candidate | silly | out: silly |
| `MqttTopic` | A pub/sub topic | `Text` | MQTT 5.0 | Wildcards in publish | candidate | niche | out: niche |
| `GpioPin` | A pin number | `Int` | — | BCM vs board numbering | candidate | silly | out: silly |
| `MatterDeviceType` | A smart-home device class | `Int` | CSA Matter | — | candidate | silly | out: silly |
| `UsbVendorProduct` | USB vendor and product id | `Object { vendor: Int, product: Int }` | USB-IF | — | candidate | silly | out: silly |
| `RgbLed` | An LED colour | `Colour` | — | — | candidate | silly | out: silly |
| `SerialBaudRate` | Serial speed | `Int` | — | — | candidate | silly | out: silly |

## Gaming

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `DiceRoll` | A dice expression (2d6+3) | `Object { count: Int, sides: Int, modifier: Int }` | — | — | candidate | niche | out: niche |
| `PlayingCard` | A card in a standard deck | `Object { rank: Enum, suit: Enum }` | — | "10" vs "T" | candidate | niche | out: niche |
| `ChessMove` | A move in algebraic notation | `Text` | FIDE SAN, UCI | SAN vs UCI | candidate | silly | out: silly |
| `ChessPosition` | A board position | `Text` FEN | FEN | — | candidate | silly | out: silly |
| `EloRating` | A skill rating | `Int` | — | — | candidate | niche | out: niche |
| `GamerTag` | A platform player name | `Object { platform: Enum, tag: Text }` | — | — | candidate | niche | out: niche |
| `SteamId` | A Steam account id | `Text` | Valve | 64-bit precision in JSON | candidate | silly | out: silly |
| `LeaderboardEntry` | A ranked score | `Object { player: UserId, score: Int, rank: Rank }` | — | — | candidate | niche | out: niche |
| `Achievement` | An unlocked achievement | `Object { id: Text, at: Instant }` | — | — | candidate | silly | out: silly |
| `ExperiencePoints` | XP total | `Int { min: 0 }` | — | — | candidate | silly | out: silly |
| `InGameCurrency` | A virtual currency amount | `Object { amount: Int, currency: Text }` | — | Confused with Money | candidate | niche | out: niche |
| `GameControllerButton` | A gamepad button | `Enum` | W3C Gamepad standard mapping | — | candidate | silly | out: silly |
| `Hitpoints` | Health points | `Int` | — | — | candidate | silly | out: silly |
| `SaveSlot` | A save game slot | `Int` | — | — | candidate | silly | out: silly |
| `LootRarity` | Common to legendary | `Enum` | — | — | candidate | silly | out: silly |

## Sport and fitness

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `MatchScore` | A match score | `Object { home: Int, away: Int }` | — | Home/away order | candidate | maybe | out: no standard |
| `MatchResult` | Win, draw, loss | `Enum` | — | — | candidate | niche | out: niche |
| `LapTime` | A lap duration | `Duration` | — | — | candidate | niche | out: niche |
| `RaceTime` | An elapsed time with precision | `Object { elapsed: Duration, precision: Enum }` | — | Hundredths vs thousandths | candidate | niche | out: niche |
| `RaceDistance` | 5K, 10K, marathon | `Length` | World Athletics | — | candidate | niche | out: niche |
| `HeartRateZone` | Training zone 1–5 | `Int { min: 1, max: 5 }` | — | Different zone models | candidate | niche | out: niche |
| `Vo2Max` | Aerobic capacity | `Decimal` | — | — | candidate | silly | out: silly |
| `CyclingPower` | Watts | `Power` | — | — | candidate | silly | out: silly |
| `Cadence` | Steps or revolutions per minute | `Int` | — | — | candidate | silly | out: silly |
| `WorkoutType` | Run, ride, swim… | `Enum` | Apple HealthKit, Health Connect | — | candidate | niche | out: niche |
| `StrengthSet` | Strength training set | `Object { reps: Int, weight: Mass }` | — | kg vs lb | candidate | niche | out: niche |
| `Handicap` | Golf handicap index | `Decimal` | World Handicap System | — | candidate | silly | out: silly |
| `FifaCountryCode` | Football association code | `Text { pattern: "[A-Z]{3}" }` | FIFA | Differs from ISO alpha-3 | candidate | silly | out: silly |
| `IocCountryCode` | Olympic country code | `Text { pattern: "[A-Z]{3}" }` | IOC | Differs from ISO alpha-3 | candidate | silly | out: silly |
| `PlayingPosition` | A player position | `Text` | — | — | candidate | silly | out: silly |

## Food and drink

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Allergen` | One of the regulated allergens | `Enum` of 14 (UK/EU) plus regional ones | UK FIR 2014, EU 1169/2011 Annex II | Free-text allergen lists | candidate | likely | standard |
| `AllergenStatement` | Contains, may contain, free from | `Object { contains: Set<Allergen>, may_contain: Set<Allergen> }` | EU 1169/2011 | — | candidate | likely | out: app entity |
| `NutritionFacts` | Nutrition per 100 g and per serving | `Object { energy: Energy, fat, saturates, carbohydrate, sugars, protein, salt: Mass, per: Quantity }` | EU 1169/2011, FDA 21 CFR 101.9 | Per serving vs per 100 g; kcal vs kJ | candidate | likely | out: app entity |
| `ServingSize` | A portion | `Quantity` | — | — | candidate | maybe | out: no standard |
| `DietaryTag` | Vegan, vegetarian, halal, kosher, gluten-free | `Enum` | — | Different definitions | candidate | maybe | out: no standard |
| `SpiceLevel` | Heat level | `Int { min: 0, max: 5 }` | — | — | candidate | silly | out: silly |
| `ScovilleHeat` | Chilli heat units | `Int` | — | — | candidate | silly | out: silly |
| `AlcoholByVolume` | ABV | `Percentage` | — | — | candidate | maybe | out: no standard |
| `AlcoholUnits` | UK units of alcohol | `Decimal` | UK CMO | vs US standard drinks | candidate | niche | out: niche |
| `Recipe` | Ingredients and steps | `Object { ingredients: List<Ingredient>, steps: List<Text>, yield: Quantity }` | schema.org Recipe | — | candidate | maybe | out: app entity |
| `Ingredient` | A quantity of a food | `Object { food: Text, quantity: Quantity? }` | — | "1 cup" parsing | candidate | maybe | out: no standard |
| `CookingTemperature` | Oven temperature | `Temperature` or gas mark | — | Fan vs conventional; gas mark | candidate | niche | out: niche |
| `GasMark` | UK oven setting | `Int { min: 1, max: 9 }` | — | — | candidate | silly | out: silly |
| `FoodHygieneRating` | UK FSA score | `Int { min: 0, max: 5 }` | FSA FHRS | Scotland uses pass/fail | candidate | niche | out: niche |
| `Cuisine` | Italian, Thai… | `Enum` | — | — | candidate | silly | out: silly |
| `NovaGroup` | Food processing level | `Int { min: 1, max: 4 }` | NOVA | — | candidate | silly | out: silly |
| `NutriScore` | A–E nutrition grade | `Enum` | Santé publique France | — | candidate | niche | out: niche |
| `ENumber` | Food additive code | `Text { pattern: "E[0-9]{3,4}[a-z]?" }` | EU Regulation 1333/2008 | — | candidate | niche | out: niche |
| `WineVintage` | Year of harvest | `Int` | — | — | candidate | silly | out: silly |
| `CoffeeRoast` | Roast level | `Enum` | — | — | candidate | silly | out: silly |

## Property and construction

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `PropertyType` | Detached, flat, terraced… | `Enum` | — | — | candidate | maybe | out: no standard |
| `Tenure` | Freehold, leasehold, share of freehold | `Enum` | HM Land Registry | — | candidate | maybe | standard |
| `FloorArea` | Internal floor area | `Area` | RICS IPMS | sq ft vs m², gross vs net | candidate | likely | folded → `Area` |
| `Bedrooms` | Number of bedrooms | `Int { min: 0 }` | — | Studio = 0 | candidate | maybe | out: no standard |
| `Bathrooms` | Number of bathrooms | `Decimal` | — | US half baths | candidate | niche | out: niche |
| `CouncilTaxBand` | A–H (I in Wales) | `Enum` | VOA | — | candidate | niche | out: niche |
| `RentFrequency` | Weekly, monthly… | `Enum` | — | pcm vs pw conversion | candidate | maybe | out: no standard |
| `Rent` | A rent with frequency | `Object { amount: Money, frequency: RentFrequency }` | — | — | candidate | maybe | out: no standard |
| `LeaseTerm` | Years remaining on a lease | `Period` | — | — | candidate | niche | out: niche |
| `ServiceCharge` | Annual service charge | `Money` | — | — | candidate | silly | out: silly |
| `UseClass` | Planning use class | `Text` | UK Use Classes Order | — | candidate | silly | out: silly |
| `BuildingHeight` | Height in metres or storeys | `Object { metres: Length?, storeys: Int? }` | — | — | candidate | silly | out: silly |
| `FloorPlan` | A plan image with rooms | `Object { image: StoredFile, rooms: List<Room> }` | — | — | candidate | silly | out: silly |
| `Room` | A room with dimensions | `Object { name: Text, width: Length, length: Length }` | — | — | candidate | niche | out: niche |
| `UValue` | Thermal transmittance | `Decimal` W/m²K | ISO 6946 | — | candidate | silly | out: silly |
| `IfcEntity` | A BIM entity | `Text` | ISO 16739 (IFC) | — | candidate | silly | out: silly |
| `ParcelId` | A land parcel identifier | `Text` | INSPIRE | — | candidate | niche | out: niche |

## Employment and HR

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `EmploymentType` | Full-time, part-time, contract | `Enum` | schema.org EmploymentType | — | candidate | maybe | out: app entity |
| `Salary` | Pay with period | `Object { amount: Money, period: Enum { hour, day, week, month, year } }` | — | Annual vs hourly mixed | candidate | likely | folded → `Money` |
| `SalaryRange` | A pay band | `Object { min: Money, max: Money, period: Enum }` | — | — | candidate | maybe | out: no standard |
| `JobTitle` | A job title | `Text` | — | — | candidate | niche | out: niche |
| `WorkingPattern` | Hours and days worked | `Object { hours_per_week: Decimal, days: Set<Weekday> }` | — | — | candidate | maybe | out: no standard |
| `Fte` | Full-time equivalent | `Decimal { min: 0, max: 1 }` | — | — | candidate | maybe | out: no standard |
| `LeaveType` | Annual, sick, parental… | `Enum` | — | — | candidate | maybe | out: no standard |
| `LeaveBalance` | Holiday remaining | `Object { amount: Decimal, unit: Enum { days, hours } }` | — | Days vs hours | candidate | maybe | out: no standard |
| `NoticePeriod` | Notice required | `Period` | — | — | candidate | niche | out: niche |
| `TimesheetEntry` | Hours worked on a day | `Object { date: CalendarDate, duration: Duration, project: Text? }` | — | — | candidate | niche | out: niche |
| `EmployeeId` | An employee number | `Text` | — | — | candidate | niche | out: niche |
| `OrgUnit` | A department | `Text` | — | — | candidate | silly | out: silly |
| `PayslipLine` | A pay element | `Object { label: Text, amount: Money, kind: Enum }` | — | — | candidate | niche | out: niche |
| `StudentLoanPlan` | UK repayment plan | `Enum` | HMRC | — | candidate | silly | out: silly |
| `PensionScheme` | A pension scheme reference | `Text` | PSR | — | candidate | silly | out: silly |
| `SkillLevel` | Beginner to expert | `Enum` | — | — | candidate | silly | out: silly |
| `Skill` | A named skill | `Object { name: Text, uri: Url? }` | ESCO, O*NET | — | candidate | niche | out: niche |

## AI and machine learning

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Vector` | An embedding | `Vector { dimensions: n }` of `Decimal` | — | Dimension mismatch | proposed | core | standard |
| `Embedding` | A vector with its model | `Object { model: ModelId, vector: Vector }` | — | Comparing across models | candidate | likely | standard |
| `ModelId` | A model identifier | `Object { provider: Text, name: Text }` | — | Version drift in names | candidate | likely | standard |
| `TokenCount` | Tokens in a prompt or completion | `Int { min: 0 }` | — | Tokenizer-dependent | candidate | likely | standard |
| `TokenUsage` | Input and output tokens | `Object { input: TokenCount, output: TokenCount, cached: TokenCount? }` | — | Each provider's shape | candidate | likely | standard |
| `SamplingTemperature` | Sampling temperature | `Decimal { min: 0, max: 2 }` | — | Name clash with Temperature | candidate | maybe | out: no standard |
| `TopP` | Nucleus sampling | `Proportion` | — | — | candidate | niche | out: niche |
| `ChatMessage` | A message in a conversation | `Object { role: Enum { system, user, assistant, tool }, content: List<ContentBlock> }` | — | Every SDK's shape | candidate | likely | standard |
| `ContentBlock` | Text, image, tool call | `Enum` | — | — | candidate | maybe | out: no standard |
| `ToolCall` | A model's function call | `Object { id: Text, name: Text, arguments: Json }` | — | Arguments as string vs object | candidate | maybe | out: no standard |
| `Prompt` | A prompt template | `Text` | — | — | candidate | maybe | out: no standard |
| `StopReason` | Why generation ended | `Enum` | — | — | candidate | niche | out: niche |
| `SimilarityScore` | Cosine similarity | `Decimal { min: -1, max: 1 }` | — | Distance vs similarity | candidate | maybe | out: no standard |
| `Confidence` | A model confidence | `Proportion` | — | — | candidate | maybe | out: no standard |
| `Label` | A classification label with score | `Object { label: Text, score: Proportion }` | — | — | candidate | maybe | out: no standard |
| `BoundingBoxPixels` | An object detection box | `Rect` in pixels | COCO, Pascal VOC | xywh vs xyxy; normalised vs pixels | candidate | maybe | out: app entity |
| `SegmentationMask` | A pixel mask | `Bytes` RLE | COCO RLE | — | candidate | niche | out: niche |
| `Keypoint` | A pose landmark | `Object { x, y: Decimal, visibility: Proportion }` | COCO keypoints | — | candidate | niche | out: niche |
| `Transcript` | Speech-to-text output | `List<Object { start: Duration, end: Duration, text: Text, speaker: Text? }>` | — | — | candidate | maybe | out: no standard |
| `ContextWindow` | Model input limit | `TokenCount` | — | — | candidate | niche | out: niche |
| `CostPerToken` | Price per million tokens | `Object { price: Money, per: Int }` | — | Per 1K vs per 1M | candidate | maybe | out: no standard |
| `EvalScore` | An evaluation metric | `Object { metric: Text, value: Decimal }` | — | — | candidate | niche | out: niche |
| `DatasetSplit` | Train, validation, test | `Enum` | — | — | candidate | silly | out: silly |
| `Hyperparameters` | Model training settings | `Map<Text, Json>` | — | — | candidate | silly | out: silly |
| `ModelCard` | Model documentation | `Object` | Hugging Face model card | — | candidate | silly | out: silly |

## Weather, environment and agriculture

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `WeatherCondition` | Clear, rain, snow… | `Enum` | WMO 4677 present-weather codes | Each API's own icons | candidate | maybe | out: app entity |
| `WindSpeed` | Wind speed | `Speed` | WMO | knots vs m/s vs mph | candidate | maybe | folded → `Speed` |
| `WindDirection` | Direction wind comes from | `Bearing` | WMO | From vs towards | candidate | maybe | folded → `Bearing` |
| `Wind` | Speed, gust and direction | `Object { speed: Speed, gust: Speed?, direction: Bearing }` | WMO | — | candidate | maybe | out: app entity |
| `BeaufortScale` | Wind force 0–12 | `Int { min: 0, max: 12 }` | WMO | — | candidate | silly | out: silly |
| `Precipitation` | Rain amount | `Length` in mm | WMO | mm vs in | candidate | maybe | out: app entity |
| `PrecipitationProbability` | Chance of rain | `Percentage` | — | — | candidate | niche | out: niche |
| `UvIndex` | Ultraviolet index | `Int { min: 0, max: 20 }` | WHO | — | candidate | niche | out: niche |
| `Visibility` | Horizontal visibility | `Length` | WMO | — | candidate | niche | out: niche |
| `Forecast` | A weather forecast for a time and place | `Object { point: Point, at: Interval, condition: WeatherCondition, temperature: Temperature… }` | — | — | candidate | niche | out: niche |
| `DewPoint` | Dew point | `Temperature` | WMO | — | candidate | silly | out: silly |
| `PollenCount` | Pollen level | `Enum` | — | — | candidate | silly | out: silly |
| `TideTime` | High or low tide | `Object { at: Instant, height: Length, kind: Enum }` | — | Chart datum | candidate | silly | out: silly |
| `SunTimes` | Sunrise and sunset | `Object { sunrise: Instant, sunset: Instant }` | — | — | candidate | niche | out: niche |
| `WmoStationId` | A weather station | `Text` | WMO | — | candidate | silly | out: silly |
| `SoilType` | Clay, loam, sand… | `Enum` | USDA soil taxonomy | — | candidate | silly | out: silly |
| `CropType` | A crop | `Text` | FAO | — | candidate | silly | out: silly |
| `LivestockId` | An animal tag | `Text` | ISO 11784 | — | candidate | silly | out: silly |
| `MicrochipNumber` | A pet microchip | `Text { pattern: "[0-9]{15}" }` | ISO 11784/11785 | 9-digit legacy chips | candidate | niche | out: niche |
| `PetSpecies` | Dog, cat… | `Enum` | — | — | candidate | niche | out: niche |
| `DogBreed` | A breed name | `Text` | FCI, The Kennel Club | — | candidate | silly | out: silly |
| `HardinessZone` | Plant hardiness | `Text` | USDA, RHS H1–H7 | — | candidate | silly | out: silly |

## Social, content and communication

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Post` | A social post | `Object { author: UserId, text: Text, at: Instant }` | ActivityStreams 2.0 | — | candidate | niche | out: niche |
| `Reaction` | An emoji reaction | `Object { emoji: Emoji, by: UserId }` | — | — | candidate | niche | out: niche |
| `NotificationChannel` | Email, SMS, push, in-app | `Enum` | — | — | candidate | maybe | out: no standard |
| `PushToken` | A device push token | `Object { platform: Enum { apns, fcm, web }, token: Text }` | APNs, FCM, RFC 8030 | Platform implied | candidate | maybe | standard |
| `SmsMessage` | An SMS body | `Text`, GSM-7 or UCS-2 segments | 3GPP TS 23.038 | Segment counting | candidate | niche | out: niche |
| `EmailMessage` | An email with headers | `Object { from: EmailAddressWithName, to: List<…>, subject: Text, html: Html?, text: Text? }` | RFC 5322 | — | candidate | maybe | out: app entity |
| `MessageId` | An email Message-ID | `Text { pattern: "<.+@.+>" }` | RFC 5322 | Angle brackets | candidate | niche | out: niche |
| `ActivityPubActor` | A fediverse account | `Object { id: Url, handle: Text }` | W3C ActivityPub | — | candidate | niche | out: niche |
| `WebFingerHandle` | user@instance | `Text` | RFC 7033 | — | candidate | niche | out: niche |
| `Did` | A decentralised identifier | `Text { pattern: "did:[a-z0-9]+:.*" }` | W3C DID | — | candidate | niche | out: niche |
| `AtprotoHandle` | A Bluesky handle | `DomainName` | AT Protocol | — | candidate | silly | out: silly |
| `RssFeed` | A feed URL | `Url` | RSS 2.0, Atom (RFC 4287) | — | candidate | niche | out: niche |
| `ContentVisibility` | Public, unlisted, private | `Enum` | — | — | candidate | niche | out: niche |
| `ModerationStatus` | Pending, approved, rejected | `Enum` | — | — | candidate | niche | out: niche |
| `LikeCount` | — | `Int` | — | — | candidate | silly | out: silly |

## Events, travel and hospitality

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `EventTicket` | A ticket for an event | `Object { event: Text, seat: SeatNumber?, barcode: Barcode }` | — | — | candidate | niche | out: niche |
| `VenueSeat` | Section, row, seat | `Object { section: Text, row: Text, seat: Text }` | — | — | candidate | niche | out: niche |
| `HotelStay` | Check-in and check-out | `Object { check_in: CalendarDate, check_out: CalendarDate }` | OpenTravel | Nights off by one | candidate | maybe | out: app entity |
| `Occupancy` | Adults, children and ages | `Object { adults: Int, children_ages: List<Int> }` | OpenTravel | — | candidate | maybe | out: app entity |
| `RoomType` | Single, double, twin, suite | `Enum` | OpenTravel | — | candidate | niche | out: niche |
| `StarRating` | Hotel star class | `Int { min: 1, max: 5 }` | — | — | candidate | niche | out: niche |
| `BookingStatus` | Pending, confirmed, cancelled | `Enum` | — | — | candidate | maybe | out: no standard |
| `CancellationPolicy` | Deadlines and penalties | `List<Object { before: Duration, fee: Money }>` | — | — | candidate | niche | out: niche |
| `Itinerary` | An ordered list of segments | `List<Segment>` | — | — | candidate | niche | out: niche |
| `TravellerType` | Adult, child, infant | `Enum` | IATA | Age thresholds | candidate | niche | out: niche |
| `LoyaltyNumber` | A frequent-flyer number | `Object { programme: Text, number: Text }` | — | — | candidate | niche | out: niche |
| `Reservation` | A table or slot booking | `Object { at: Instant, party_size: Int, name: PersonName }` | schema.org Reservation | — | candidate | niche | out: niche |
| `PartySize` | Number of guests | `Int { min: 1 }` | — | — | candidate | silly | out: silly |
| `DressCode` | — | `Enum` | — | — | candidate | silly | out: silly |

## Miscellaneous

| Type | Represents | Shape | Standard or authority | Why standard | Status | Gut call | Decision |
|---|---|---|---|---|---|---|---|
| `Uuid` | A universally unique id | Built in | RFC 9562 | Case and braces | exists | core | standard |
| `YesNoUnknown` | Yes, no, unknown | `Enum { yes, no, unknown }` | — | Nullable Bool ambiguity | candidate | maybe | out: no standard |
| `TriState` | True, false, not set | `Enum` | — | — | candidate | maybe | out: no standard |
| `SortOrder` | Ascending or descending | `Enum` | — | — | candidate | maybe | out: no standard |
| `Priority` | Low, normal, high, urgent | `Enum` | — | 1 = high vs 1 = low | candidate | maybe | out: no standard |
| `Severity` | Info to critical | `Enum` | RFC 5424 | — | candidate | maybe | out: app entity |
| `Status` | Active, inactive… | — | — | Too generic to standardise | candidate | silly | out: silly |
| `RevisionNumber` | An optimistic lock version | `Int { min: 0 }` | — | — | candidate | maybe | out: no standard |
| `LotteryNumbers` | A set of drawn numbers | `Set<Int>` | — | — | candidate | silly | out: silly |
| `TarotCard` | A tarot card | `Enum` | — | — | candidate | silly | out: silly |
| `Mood` | A mood rating | `Enum` | — | — | candidate | silly | out: silly |
| `ShoeWidth` | Shoe width fitting | `Enum` | — | — | candidate | silly | out: silly |
| `Bookmark` | A saved URL with title | `Object { url: Url, title: Text }` | — | — | candidate | silly | out: silly |

## Tally

1,120 types across 42 domains. Decisions: 247 `standard` (12 already
built), 88 `folded`, 785 `out`.

| Gut call | Count |
|---|---|
| core | 45 |
| likely | 142 |
| maybe | 337 |
| niche | 425 |
| silly | 171 |

### The 30 strongest candidates, as brainstormed

Kept as the brainstorm ranked them; the Decision column above says what became of each.

| # | Type | Why it earns a place |
|---|---|---|
| 1 | `PostalAddress` | Every shop, CRM and delivery package invents line1/line2/street and gets other countries' formats wrong |
| 2 | `PostalCode` | Leading zeros, spaces and case are lost as soon as two packages pass one along; validation is per country |
| 3 | `Subdivision` | ISO 3166-2 sits beside `Country` in every address, tax and shipping rule; state names and abbreviations collide |
| 4 | `PersonName` | A first/last split fails much of the world, and every identity, CRM and email package splits it differently |
| 5 | `Interval` | A span between two `Instant`s; inclusive versus exclusive end and open-ended spans are the common bugs |
| 6 | `DateRange` | Bookings, stays and reports; whether the end date is included is the classic off-by-one |
| 7 | `LocalTime` | Opening hours, schedules and alarms need a wall-clock time with no date, which area 10 doesn't name on its own |
| 8 | `Period` | Calendar amounts (one month) for subscriptions and contracts, which a `Duration` can't express |
| 9 | `RecurrenceRule` | Calendars, billing and reminders each invent a repeat model; RFC 5545 RRULE is what they exchange |
| 10 | `OpeningHours` | Shops, clinics and delivery slots; crossing midnight and holiday exceptions are reinvented everywhere |
| 11 | `Range` | The generic bounded range under the date and number ranges, with its inclusivity said once |
| 12 | `Quantity` | A value with a UN/ECE unit for commerce and inventory, where the unit is otherwise implied |
| 13 | `Length` | Distance, height and dimensions across shipping, maps and health; unit confusion is the textbook failure |
| 14 | `Mass` | Shipping, food, health and fitness; kilograms versus pounds versus stone |
| 15 | `Temperature` | Weather, IoT, health and cooking; °C to °F is an offset, not a factor, and gets converted wrongly |
| 16 | `DataSize` | Uploads, quotas and plans; 1000 versus 1024 bytes per kilobyte |
| 17 | `BoundingBox` | Map search and tiles; corner order is the same bug as `[lat, lng]`, plus the antimeridian |
| 18 | `Iban` | Payouts and direct debits; spaces, case and the mod-97 check |
| 19 | `VatNumber` | B2B invoicing and reverse charge; the country prefix is included or not depending on the package |
| 20 | `ExchangeRate` | The first thing any multi-currency `Money` code needs; inverted rates are a silent bug |
| 21 | `Gtin` | Product feeds and marketplaces; UPC versus EAN padding and leading zeros break matching |
| 22 | `ImageDimensions` | Every media, upload and layout package; width-by-height order and zero sizes |
| 23 | `DomainName` | Under email, URLs, DNS and tenancy; case, trailing dot and punycode make equal names unequal |
| 24 | `SemanticVersion` | Packages themselves (#88) and every API; comparing versions as text puts 1.10 before 1.9 |
| 25 | `PasswordHash` | The PHC string keeps algorithm, parameters and salt with the hash, so auth packages can verify each other's |
| 26 | `Secret` | A value never printed in logs, errors or traces; every auth and integration package needs the same guarantee |
| 27 | `LocalisedText` | Catalogues and content in several languages; each package invents its own map and fallback rule |
| 28 | `ProblemDetails` | One HTTP error body (RFC 9457) instead of every API's own error shape |
| 29 | `StoredFile` | An uploaded file's key, name, type, size and hash; every storage package defines one |
| 30 | `TraceId` | W3C Trace Context ids cross every service boundary and every observability package |
