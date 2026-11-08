# `syntax.type_block_required`

A type is declared without a block after its base.

## Why Jadpo refuses it

Every type is written `type Name = Base { ... }`, even when the block is
empty. Without a block it reads like an alias, and Jadpo has no aliases:
every type is a new type, which a value of its base can't be used as by
accident.

## What it looks like

main.jadpo:

```jadpo check project=e-type-block expect=syntax.type_block_required
type OrderId = Uuid
```

```text output
error: `OrderId` needs a block
 --> main.jadpo:1:16
  |
1 | type OrderId = Uuid
  |                ^^^^ expected `{}` after the base
  reason: Every type is written `type Name = Base { ... }`. Without a block it reads like an alias, and Jadpo has no aliases: every type is a new type.
  next: Add `{}` after `Uuid` (agent)
  rule: syntax.type_block_required
  help: https://jadpo.dev/docs/errors/syntax.type_block_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the type has no rules of its own**, add an empty block:

```jadpo check project=e-type-block-fix
type OrderId = Uuid {}
```

`OrderId` is now a type of its own, built from `Uuid`.

## Related

- [Named types](../language/02-values-and-types.md#named-types)
- [One form for every type](../language/02-values-and-types.md#one-form-for-every-type)
