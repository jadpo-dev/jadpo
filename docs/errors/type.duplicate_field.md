# `type.duplicate_field`

A value is built with the same field given twice, or a type declares a field twice.

## Why Jadpo refuses it

Each field of a value has one value, so building a `Product` with two
`name:` lines leaves a reader to work out which one counts. Jadpo won't
pick the first or the last, because the two may differ and either
choice would quietly drop the other.

## What it looks like

main.jadpo:

```jadpo check project=e-duplicate-field expect=type.duplicate_field
type ProductName = Text {
    max_length: 120
}

type Product = Object {
    name: ProductName
}

function build_product(name: ProductName) -> Product {
    return Product {
        name: name
        name: name
    }
}
```

```text output
error: `name` is given twice
 --> main.jadpo:12:9
   |
11 |         name: name
   |         ---- first given here
12 |         name: name
   |         ^^^^ given again
  reason: Each field of `Product` is given once, so there is never a question of which value counts.
  next: Remove one `name` (agent)
  rule: type.duplicate_field
  help: https://jadpo.dev/docs/errors/type.duplicate_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A type that declares the same field twice is the same error:

```jadpo check project=e-duplicate-field-declared expect=type.duplicate_field
type ProductName = Text {
    max_length: 120
}

type Product = Object {
    name: ProductName
    name: ProductName
}
```

```text output
error: `Product` declares `name` twice
 --> main.jadpo:7:5
  |
6 |     name: ProductName
  |     ---- first declared here
7 |     name: ProductName
  |     ^^^^ declared again
  reason: Each field is written once; a second line with the same name would leave its reader guessing which one counts.
  next: Remove one `name`, or rename it (agent)
  rule: type.duplicate_field
  help: https://jadpo.dev/docs/errors/type.duplicate_field

check failed: 1 error, 1 file
```

## How to fix it

**If both lines give the same field**, keep the one with the value you
mean and remove the other:

```jadpo fragment
    return Product {
        name: name
    }
```

```jadpo check project=e-duplicate-field-fix hidden
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

The `Product` is built with one `name`. If the second line was meant
for another field, give it that field's name, and declare the field in
the type if it has none.

**If the type declares it twice**, remove one line, or rename it if it
was meant to be another field:

```jadpo fragment
type Product = Object {
    name: ProductName
}
```

```jadpo check project=e-duplicate-field-declared-fix hidden
type ProductName = Text {
    max_length: 120
}

type Product = Object {
    name: ProductName
}
```

The type now has one `name`, so a value of it has one too.

## Related

- [Objects](../language/02-values-and-types.md#objects)
- [Creating values](../language/02-values-and-types.md#creating-values)
