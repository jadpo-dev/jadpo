# `constant.cycle`

A constant's value depends on itself.

## Why Jadpo refuses it

A constant's value is worked out while compiling, from the constants it reads.
When a chain of those reads leads back to the constant you started with, there
is no value to start from. Jadpo can't choose where to break the loop, because
that depends on which number you meant.

## What it looks like

```jadpo check project=e-constant-cycle expect=constant.cycle
type Stock = Int {
    min: 0
}

constant small_batch: Stock = large_batch
constant large_batch: Stock = small_batch
```

```text output
error: `large_batch` depends on itself
 --> main.jadpo:6:31
  |
6 | constant large_batch: Stock = small_batch
  |                               ^^^^^^^^^^^ leads back to it
  reason: A constant's value is worked out from the constants it reads, and `large_batch` reads `small_batch`, which reads `large_batch`, so it has no value.
  next: Write `large_batch`'s value without reading a constant that reads it (agent)
  rule: constant.cycle
  help: https://jadpo.dev/docs/errors/constant.cycle

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If one of the constants has a value of its own**, write it there:

```jadpo fragment
constant small_batch: Stock = 250
constant large_batch: Stock = 1000
```

```jadpo check project=e-constant-cycle-fix hidden
type Stock = Int {
    min: 0
}

constant small_batch: Stock = 250
constant large_batch: Stock = 1000
```

Each constant now has a value of its own, so neither depends on the other.

## Related

- [Constants](../language/03-logic.md#constants)
