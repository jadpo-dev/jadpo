# `type.unknown_field`

Code names a field its type doesn't declare, often a misspelling of one it does.

## Why Jadpo refuses it

A type lists its fields, so a name that isn't among them can never be one.
Jadpo reports it when you check, and lists the fields there are, so a
misspelling is caught before the value is built or read.

## What it looks like

main.jadpo:

```jadpo check project=e-unknown-field expect=type.unknown_field
type ProductName = Text {
    max_length: 120
}

type Product = Object {
    name: ProductName
}

function build_product(name: ProductName) -> Product {
    return Product {
        name: name
        title: name
    }
}
```

```text output
error: `Product` has no field `title`
 --> main.jadpo:12:9
   |
12 |         title: name
   |         ^^^^^ not declared
  reason: `Product`'s fields are `name`.
  next: Remove `title`, or declare it on `Product` (agent)
  rule: type.unknown_field
  help: https://jadpo.dev/docs/errors/type.unknown_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the type should have this field.

**If it shouldn't**, remove the line, or correct it to a field the type
has:

```jadpo fragment
    return Product {
        name: name
    }
```

```jadpo check project=e-unknown-field-fix-1 hidden
type ProductName = Text {
    max_length: 120
}

type Product = Object {
    name: ProductName
}

function build_product(name: ProductName) -> Product {
    return Product {
        name: name
    }
}
```

`build_product` now builds a `Product` from the one field it has.

**If it should**, add the field to the type:

```jadpo fragment
type Product = Object {
    name: ProductName
    title: ProductName
}
```

```jadpo check project=e-unknown-field-fix-2 hidden
type ProductName = Text {
    max_length: 120
}

type Product = Object {
    name: ProductName
    title: ProductName
}

function build_product(name: ProductName) -> Product {
    return Product {
        name: name
        title: name
    }
}
```

Every `Product` now has a `title`, so other places that build one have to
give it.

## Related

- [Objects](../language/02-values-and-types.md#objects)
