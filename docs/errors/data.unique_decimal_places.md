# `data.unique_decimal_places`

A unique rule covers a field kept as JSON that holds a percentage, or
another value whose places its type doesn't fix, which its JSON can write
two ways.

## Why Jadpo refuses it

A field whose type is an object, an enum with data, a list, a set or a
map is kept whole as its JSON, and a unique rule over it compares the
digest of that JSON. A percentage keeps its places in its JSON, so `12.5%`
and `12.50%` are `==` in code but written differently, as is any value
whose places its type doesn't fix. The rule would let both in, and two
records would hold what code calls the same value.

## What it looks like

main.jadpo:

```jadpo check project=e-unique-decimal-places expect=data.unique_decimal_places
/// What a cup of coffee is made with.
type Recipe = Object {
    milk: Percentage
}

entity Coffee {
    id: Uuid
    recipe: Recipe {
        unique: true
    }
}
```

```text output
error: `Coffee.recipe` can be written two ways, so no unique rule can compare it
 --> main.jadpo:9:9
  |
9 |         unique: true
  |         ^^^^^^ kept as JSON
  reason: `Coffee.recipe` is kept as its JSON, and a unique rule compares the digest of that JSON; it holds a `Percentage`, whose JSON keeps its places, so `12.5%` and `12.50%` are `==` in code but written differently, and the rule would let both in.
  next: Hold a decimal whose type has a `scale:` instead, or leave the field out of the unique rule (agent)
  rule: data.unique_decimal_places
  help: https://jadpo.dev/docs/errors/data.unique_decimal_places

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the share has a fixed number of places**, keep it as a decimal
whose type has a `scale:`, so each value is written one way:

```jadpo check project=e-unique-decimal-places-fix
type MilkShare = Decimal {
    scale: 1
    min: 0.0
    max: 100.0
}

/// What a cup of coffee is made with.
type Recipe = Object {
    milk: MilkShare
}

entity Coffee {
    id: Uuid
    recipe: Recipe {
        unique: true
    }
}
```

No two coffees can now share a recipe.

**If two records may share one**, remove the rule:

```jadpo check project=e-unique-decimal-places-fix-2
/// What a cup of coffee is made with.
type Recipe = Object {
    milk: Percentage
}

entity Coffee {
    id: Uuid
    recipe: Recipe
}
```

Two coffees may now be made with the same recipe.

## Related

- [A unique rule over a structured value](../language/05-data.md#a-unique-rule-over-a-structured-value)
- [Structured values](../language/05-data.md#structured-values)
