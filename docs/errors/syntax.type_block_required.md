# `syntax.type_block_required`

A type always has a block, even an empty one, so it never reads like an alias:

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
  help: docs/language/02-values-and-types.html#syntax-type-block-required

check failed: 1 error, 1 file
```
