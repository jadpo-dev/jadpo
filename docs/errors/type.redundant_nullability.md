# `type.redundant_nullability`

```jadpo check project=e-redundant expect=type.redundant_nullability
type ProductName = Text {
    max_length: 120
}

type Listing = Object {
    subtitle: ProductName?
}

type ListingPreview = Object {
    subtitle: Listing.subtitle?
}
```

```text output
error: `Listing.subtitle` already permits `none`
 --> main.jadpo:10:15
   |
10 |     subtitle: Listing.subtitle?
   |               ^^^^^^^^^^^^^^^^^ nullable twice
  reason: `Listing.subtitle` takes `none` from its declared type; a second `?` adds nothing.
  next: Remove the extra `?` (agent)
  rule: type.redundant_nullability
  help: docs/language/02-values-and-types.html#type-redundant-nullability

check failed: 1 error, 1 file
```
