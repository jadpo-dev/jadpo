# `constant.cycle`

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
