# `syntax.null_coalescing`

A default for a missing value is written with `??` or `?:` instead of a `match`.

## Why Jadpo refuses it

`??` gives a default for a missing value in JavaScript, TypeScript, C# and
Swift, and `?:` in Kotlin. Jadpo reads a value that may be `none` with a
`match` that handles both cases, so what happens when it's missing is written
next to what happens when it's there, in the one form used everywhere.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/syntax.null_coalescing

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Kotlin's `?:` is the same error, with the same fix.

## How to fix it

**If you wanted a default**, write the `match` the error names:

```jadpo fragment
function headline(listing: Listing) -> ProductName {
    return match listing.subtitle {
        some(subtitle) -> subtitle
        none -> listing.name
    }
}
```

```jadpo check project=e-habit-coalescing-fix hidden
type ProductName = Text {
    min_length: 1
}

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

`headline` now gives the subtitle when there is one, and the name when there
isn't.

## Related

- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)
- [Match](../language/03-logic.md#match)
