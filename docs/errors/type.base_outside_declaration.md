# `type.base_outside_declaration`

A field, parameter or result is given `Object`, `Enum`, `Input` or `Output` as its type, which is only ever the base of a type declaration.

## Why Jadpo refuses it

`Object`, `Enum`, `Input` and `Output` say what a declared type is built
from, written after `=` in `type Name = Base { ... }`. They aren't types
of their own: a bare `Object` has no fields, so nothing says what it
holds. Jadpo has no unnamed shapes, so a nested shape is declared once,
with a name, and every field and signature that holds it uses that name.

## What it looks like

main.jadpo:

```jadpo check project=e-base expect=type.base_outside_declaration
type Basket = Object {
    details: Object
}
```

```text output
error: `Object` is used as a type
 --> main.jadpo:2:14
  |
2 |     details: Object
  |              ^^^^^^ only a base
  reason: `Object` is written only after `=` in a `type` declaration; every shape has a name, so a field or signature names that type.
  next: Declare `type Name = Object { ... }` and use its name here (agent)
  rule: type.base_outside_declaration
  help: https://jadpo.dev/docs/errors/type.base_outside_declaration

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the field holds a shape of its own**, declare that shape as a named
type with its fields, and give the field that name. main.jadpo becomes:

```jadpo check project=e-base-fix
type BasketNote = Text {
    max_length: 200
}

type BasketDetails = Object {
    note: BasketNote
}

type Basket = Object {
    details: BasketDetails
}
```

`Basket.details` now holds a `BasketDetails`, and any other field or
signature that holds the same shape uses the same name.

## Related

- [One form for every type](../language/02-values-and-types.md#one-form-for-every-type)
- [Objects](../language/02-values-and-types.md#objects)
- [Enums](../language/02-values-and-types.md#enums)
