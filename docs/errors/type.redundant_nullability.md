# `type.redundant_nullability`

A `?` is added to a type that already allows `none`.

## Why Jadpo refuses it

`?` says a type also allows `none`. `Listing.subtitle` already does,
because it is declared `ProductName?`, so a second `?` adds nothing. Jadpo
refuses it so that nobody reading the type looks for a difference that
isn't there.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.redundant_nullability

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the type already allows `none`**, remove the extra `?`:

```jadpo fragment
type ListingPreview = Object {
    subtitle: Listing.subtitle
}
```

```jadpo check project=e-redundant-fix hidden
type ProductName = Text {
    max_length: 120
}

type Listing = Object {
    subtitle: ProductName?
}

type ListingPreview = Object {
    subtitle: Listing.subtitle
}
```

`ListingPreview.subtitle` still allows `none`, from `Listing.subtitle`.

## Related

- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)
- [Field types](../language/02-values-and-types.md#field-types)
