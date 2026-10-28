# `syntax.null_coalescing`

A value that may be `none` is read with a `match` that handles both cases; the edit writes it.

```jadpo check project=e-habit-coalescing expect=syntax.null_coalescing
type ProductName = Text {
    min_length: 1
}

type Listing = Object {
    name: ProductName
    subtitle: ProductName?
}

function headline(listing: Listing) -> ProductName {
    return listing.subtitle ?? listing.name
}
```

```text output
error: Jadpo reads a value that may be `none` with `match`, not `??`
 --> main.jadpo:11:29
   |
11 |     return listing.subtitle ?? listing.name
   |                             ^^ write a `match`
  reason: `??` gives a default for a missing value in JavaScript, TypeScript, C# and Swift, and `?:` in Kotlin; Jadpo reads a value that may be `none` with a `match` that handles both cases.
  next: Write `match listing.subtitle` with the arms `some(subtitle) -> subtitle` and `none -> listing.name` (agent)
  rule: syntax.null_coalescing
  help: docs/language/02-values-and-types.html#syntax-null-coalescing

check failed: 1 error, 1 file
```
