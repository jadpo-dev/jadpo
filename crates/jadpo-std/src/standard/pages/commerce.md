# Sku

A seller's own stock code: up to 64 characters with no spaces, such as `AB-1234-XL`.

Use it for your own product codes. For a code printed on the packaging, use `Ean13`,
`UpcA` or `Isbn`. For one physical item, use `SerialNumber`.

```jadpo
Sku("AB-1234-XL")
```

# Ean13

The 13-digit barcode number on most products outside North America.

Use it to identify a retail product. The US and Canadian form is a `UpcA`, a book's
number is an `Isbn`, and your own code is a `Sku`.

```jadpo
Ean13("4006381333931")
```

- It checks its own last digit, so a mistyped number is usually refused.

# UpcA

The 12-digit barcode number on products in the US and Canada.

Use it to identify a retail product there. Elsewhere, use `Ean13`.

```jadpo
UpcA("036000291452")
```

- It is text so that its leading zero is kept, and it checks its own last digit.

# Isbn

A book's number, in its 13 digits with no hyphens.

Use it for books. For other retail products, use `Ean13`.

```jadpo
Isbn("9780306406157")
```

- A 10-digit ISBN is refused, so convert it first. Remove any hyphens as well.

# Sscc

The 18-digit number on a pallet or shipping container's label.

Use it to track a whole shipping unit through a warehouse. For a parcel with a
courier, use `TrackingNumber`. For a sea container, use `ContainerNumber`.

```jadpo
Sscc("106141412345678908")
```

# Barcode

What a barcode encodes, and which kind of barcode it is.

Use it when one field can hold several kinds, such as tickets and products together.
If you only have product codes, `Ean13` or `UpcA` alone is simpler.

```jadpo
Barcode.qr_code { data: "https://example.com/tickets/42" }
```

- The others are `Barcode.ean13`, `Barcode.upc_a` and `Barcode.code128`, each with
  `data`. An `ean13` checks its digits. `code128` takes printable ASCII only.
- It holds the data, not an image of the barcode.

# UnitQuantity

How much of something: a count of items, or an amount by mass, volume, length or area.

Use it on an order line or a stock count that sells both "3 items" and "2.5 kg". If
you sell only whole items, your own type on `Int` is simpler.

```jadpo
UnitQuantity.each { count: 3 }
```

- The others are `.mass`, `.volume`, `.length` and `.area`, each with an `amount`:
  `UnitQuantity.mass { amount: 2.5kg }`.
- It can't be below zero.

# Rating

A rating on its own scale: `4.5/5`.

Use it for reviews and scores. It keeps its scale with it, so 4 of 5 and 8 of 10
aren't mixed up. For a plain percentage score, use `Percentage`.

```jadpo
Rating { value: 4.5 best: 5.0 worst: 1.0 }
```

- `best` must be above `worst`, and `value` between them.

# ClothingSize

A clothing size in its system, as the label writes it: `UK 10`, `US M`.

Use it for clothes in a shop. For shoes, use `ShoeSize`.

```jadpo
ClothingSize { system: ClothingSizeSystem.uk size: "10" }
```

- Two sizes in different systems aren't compared or converted. `UK 10` and `EU 38`
  are simply different values.

# ClothingSizeSystem

Which system a clothing size is in: `UK`, `EU` or `US`.

Use it as part of a `ClothingSize`, or to remember which sizes a shopper prefers.

```jadpo
ClothingSizeSystem.uk
```

# ShoeSize

A shoe size in its system, as the box writes it: `UK 7.5`, `EU 42`.

Use it for shoes in a shop. For clothes, use `ClothingSize`.

```jadpo
ShoeSize { system: ShoeSizeSystem.uk size: "7.5" }
```

- A half size is written with a point, `7.5`. `7,5` and `7.25` are refused.

# ShoeSizeSystem

Which system a shoe size is in: `UK`, `EU` or `US`.

Use it as part of a `ShoeSize`, or to remember which sizes a shopper prefers.

```jadpo
ShoeSizeSystem.uk
```

# Dimensions

A box's length, width and height.

Use it for parcels and products. For a picture's size in pixels, use
`ImageDimensions`. For one measurement, use `Length`.

```jadpo
Dimensions { length: 30cm width: 20cm height: 10cm }
```

- None can be below zero. `text.format` shows centimetres, `30 × 20 × 10 cm`, or
  another unit with `unit:`.

# HsCode

A customs code for a kind of goods: six to ten digits, such as `090111` for coffee.

Use it on customs declarations. For your own product code, use `Sku`.

```jadpo
HsCode("090111")
```

- Write it with no dots. It keeps a leading zero.

# Incoterm

A rule saying who pays for what in international shipping: `FOB`, `DDP` and so on.

Use it on a trade deal or a shipping order. These are the Incoterms 2020 rules.

```jadpo
Incoterm.fob
```

- Earlier rules such as `DAT` are still read from old data, so don't write new ones.

# TrackingNumber

A parcel's tracking number, with its carrier: `UPSN 1Z999AA10123456784`.

Use it to track a parcel with a courier. For a pallet, use `Sscc`. For a sea
container, use `ContainerNumber`.

```jadpo
TrackingNumber { carrier: "UPSN" number: "1Z999AA10123456784" }
```

- The number is in capitals, as carriers print it.

# Carrier

A shipping company by its two-to-four-letter code: `UPSN`, `MAEU`.

Use it to name a carrier, as part of a `TrackingNumber` or alone. Codes are the
Standard Carrier Alpha Codes.

```jadpo
Carrier("UPSN")
```

# ContainerNumber

A shipping container's number, checked by its check digit: `CSQU3054383`.

Use it for sea and rail freight containers. For a parcel, use `TrackingNumber`. For a
software container, use `ContainerImage`.

```jadpo
ContainerNumber("CSQU3054383")
```

- Write it with no spaces. `text.format` adds them: `CSQU 305438 3`.

# LotNumber

A manufacturing batch: up to 20 characters, such as `ABC123`.

Use it to trace a production run, as a recall needs. For one single item, use
`SerialNumber`.

```jadpo
LotNumber("ABC123")
```

- It can't contain spaces or `#`, since these are the characters GS1 barcodes take.

# SerialNumber

One item's own serial number: up to 20 characters, such as `SN-0042a`.

Use it for individual items you may need to trace, such as a warranty. For a whole
batch, use `LotNumber`. For a kind of product, use `Sku`.

```jadpo
SerialNumber("SN-0042a")
```

- It can't contain spaces or `#`, since these are the characters GS1 barcodes take.

# Vin

A vehicle identification number: 17 characters with no `I`, `O` or `Q`.

Use it to identify one vehicle for life. Its plate is a `VehicleRegistration`, which
can change.

```jadpo
Vin("1HGCM82633A004352")
```

- It is upper case. Letters that look like digits aren't allowed, so `O` is refused.

# VehicleRegistration

A vehicle's registration mark with its country: `GB AB12 CDE`.

Use it for parking, fleets and insurance. For the number that stays with a vehicle
for life, use `Vin`.

```jadpo
VehicleRegistration { country: Country.gb plate: "AB12 CDE" }
```

- The mark is kept as its plate shows it: in capitals, with single spaces.
