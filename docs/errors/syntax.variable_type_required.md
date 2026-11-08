# `syntax.variable_type_required`

A variable or a constant is declared without a type.

## Why Jadpo refuses it

Every variable states its type, so a reader sees it where the value is
introduced, without working it out from what the value is built from. A
literal then takes that type, which is why a `0.00` doesn't need a
constructor around it. A constant states its type for the same reason, as a
signature does.

## What it looks like

main.jadpo:

```jadpo check project=e-variable-type expect=syntax.variable_type_required
type ItemPrice = Decimal {
    scale: 2
    min: 0.00
}

function free() -> ItemPrice {
    var price = ItemPrice(0.00)
    return price
}
```

```text output
error: `price` needs a type
 --> main.jadpo:7:9
  |
7 |     var price = ItemPrice(0.00)
  |         ^^^^^ expected `: Type` after the name
  reason: Every variable states its type, so a reader sees it where the value is introduced.
  next: Write `var price: ItemPrice = 0.00` (agent)
  rule: syntax.variable_type_required
  help: https://jadpo.dev/docs/errors/syntax.variable_type_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A constant with no type is the same error:

main.jadpo:

```jadpo check project=e-variable-type-constant expect=syntax.variable_type_required
type Count = Int {}

constant limit = Count(3)

function lim() -> Count {
    return limit
}
```

```text output
error: `limit` needs a type
 --> main.jadpo:3:10
  |
3 | constant limit = Count(3)
  |          ^^^^^ expected `: Type` after the name
  reason: Every constant states its type, as a signature does: `constant name: Type = value`.
  next: Add the type after the name: `constant limit: Type = ...` (agent)
  rule: syntax.variable_type_required
  help: https://jadpo.dev/docs/errors/syntax.variable_type_required

check failed: 1 error, 1 file
```

## How to fix it

**If it is a variable**, write the type after its name:

```jadpo fragment
function free() -> ItemPrice {
    var price: ItemPrice = 0.00
    return price
}
```

```jadpo check project=e-variable-type-fix hidden
type ItemPrice = Decimal {
    scale: 2
    min: 0.00
}

function free() -> ItemPrice {
    var price: ItemPrice = 0.00
    return price
}
```

`price` is now a variable of type `ItemPrice`, and the literal takes that
type.

**If it is a constant**, write the type after its name:

```jadpo fragment
constant limit: Count = 3
```

```jadpo check project=e-variable-type-constant-fix hidden
type Count = Int {}

constant limit: Count = 3

function lim() -> Count {
    return limit
}
```

`limit` is now a constant of type `Count`.

## Related

- [Variables](../language/03-logic.md#variables)
- [Constants](../language/03-logic.md#constants)
