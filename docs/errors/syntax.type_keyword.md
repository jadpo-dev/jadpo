# `syntax.type_keyword`

A type is declared with `struct`, `interface` or `enum` instead of `type`.

## Why Jadpo refuses it

`struct` declares a record in Rust, Go, Swift and C#, `interface` an object's
shape in TypeScript, and `enum` a list of cases in most languages. Every Jadpo
type is `type Name = Base { ... }`, with `Object` or `Enum` as the base, so a
new kind of type never needs a new keyword.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-struct expect=syntax.type_keyword
type Count = Int {
    min: 0
}

struct GridCell {
    across: Count
    down: Count
}
```

```text output
error: Jadpo writes `type GridCell = Object`, not `struct GridCell`
 --> main.jadpo:5:1
  |
5 | struct GridCell {
  | ^^^^^^^^^^^^^^^ write `type GridCell = Object`
  reason: `struct` declares a record in Rust, Go, Swift and C#, `interface` an object's shape in TypeScript, and `enum` a list of cases in most languages; every Jadpo type is `type Name = Base { ... }`, with `Object` or `Enum` as the base.
  next: Write `type GridCell = Object` (agent)
  rule: syntax.type_keyword
  help: https://jadpo.dev/docs/errors/syntax.type_keyword

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`enum` is the same error, and names `Enum` as the base:

main.jadpo:

```jadpo check project=e-habit-enum expect=syntax.type_keyword
enum TicketStatus {
    open
    closed
}
```

```text output
error: Jadpo writes `type TicketStatus = Enum`, not `enum TicketStatus`
 --> main.jadpo:1:1
  |
1 | enum TicketStatus {
  | ^^^^^^^^^^^^^^^^^ write `type TicketStatus = Enum`
  reason: `struct` declares a record in Rust, Go, Swift and C#, `interface` an object's shape in TypeScript, and `enum` a list of cases in most languages; every Jadpo type is `type Name = Base { ... }`, with `Object` or `Enum` as the base.
  next: Write `type TicketStatus = Enum` (agent)
  rule: syntax.type_keyword
  help: https://jadpo.dev/docs/errors/syntax.type_keyword

check failed: 1 error, 1 file
```

`interface`, and Go's `type GridCell struct {`, are the same error as
`struct`, with `Object` as the base.

## How to fix it

**If it holds fields**, write `type Name = Object`:

```jadpo fragment
type GridCell = Object {
    across: Count
    down: Count
}
```

```jadpo check project=e-habit-struct-fix hidden
type Count = Int {
    min: 0
}

type GridCell = Object {
    across: Count
    down: Count
}
```

`GridCell` is now an object type with two fields.

**If it lists cases**, write `type Name = Enum`:

```jadpo check project=e-habit-enum-fix
type TicketStatus = Enum {
    open
    closed
}
```

`TicketStatus` is now an enum with two cases.

## Related

- [One form for every type](../language/02-values-and-types.md#one-form-for-every-type)
- [Objects](../language/02-values-and-types.md#objects)
- [Enums](../language/02-values-and-types.md#enums)
