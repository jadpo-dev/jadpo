# `type.nullable_selection`

Code reads a field from a value that may be `none`, before a `match` has found it.

## Why Jadpo refuses it

A value of type `T?` holds a `T` or `none`, and `none` has no fields. So
reading a field from one would fail whenever it is `none`. Jadpo has no
`?.` that quietly gives `none` instead, because what should happen when
the value is missing is a choice about your app: a `match` says it.

## What it looks like

main.jadpo:

```jadpo check project=e-nullable expect=type.nullable_selection
type ProductName = Text {
    max_length: 120
}

type Product = Object {
    name: ProductName
}

function listed_name(product: Product?) -> ProductName {
    return product.name
}
```

```text output
error: `product` may be `none`, so `name` can't be read yet
 --> main.jadpo:10:20
   |
10 |     return product.name
   |                    ^^^^ may be `none`
  reason: `product` is `Product?`; a field is read once `match` has found a value.
  next: Match on `product` and read `name` inside `some(...)` (agent)
  rule: type.nullable_selection
  help: https://jadpo.dev/docs/errors/type.nullable_selection

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is what to give when the value is missing.

**If something else stands in**, match on the value and read the field
inside `some(...)`:

```jadpo fragment
function listed_name(product: Product?) -> ProductName {
    return match product {
        some(found) -> found.name
        none -> "Unlisted"
    }
}
```

```jadpo check project=e-nullable-fix-1 hidden
type ProductName = Text {
    max_length: 120
}

type Product = Object {
    name: ProductName
}

function listed_name(product: Product?) -> ProductName {
    return match product {
        some(found) -> found.name
        none -> "Unlisted"
    }
}
```

A missing product is now listed as "Unlisted".

**If the value is never missing**, take it as `Product`, so the caller
deals with `none`:

```jadpo fragment
function listed_name(product: Product) -> ProductName {
    return product.name
}
```

```jadpo check project=e-nullable-fix-2 hidden
type ProductName = Text {
    max_length: 120
}

type Product = Object {
    name: ProductName
}

function listed_name(product: Product) -> ProductName {
    return product.name
}
```

`listed_name` reads the name directly, and a caller holding a `Product?`
matches on it first.

## Related

- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)
- [Match](../language/03-logic.md#match)
